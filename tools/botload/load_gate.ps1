#requires -Version 5
<#
.SYNOPSIS
  load_gate.ps1 — 假人压测档位（C6 载荷门禁）：200/500/1000 档，采登录成功率 / 掉线 / tick P99 / 内存曲线。

.DESCRIPTION
  编排（不含业务）：写 BotSrv 配置 → 起 BotSrv（N 个假人）→ 等登录爬坡到 N → 稳态期按秒采内存曲线
  → 停 BotSrv → 用 tools/botload/load_report.py 归算该档指标 → 汇总。

  指标口径见 tools/botload/load_report.py 头部（与 M2 的 p99-baseline **同一份口径**）：
    · tick 服务时延 = 假人收到明文动作帧 `#+GD/<rtime>!` 的时刻 − 帧里带的 rtime
      （两者都是 Environment.TickCount，全机同域）⇒ 反映世界线程/发送队列被拖住的程度
    · 登录成功率 = 稳态窗内 login_ok 峰值 ÷ 档位人数（BotSrv 内部计数，不抓日志行）
    · 掉线 = 登录后 socket 关闭/超时（与"连不上"分开计），判据只看稳态窗
    · 内存 = 各服务进程 WS/私有字节/CPU 时间序列 + 末段斜率

  前置：
    · 服务端全栈已在跑（`E:\MirServer\start-all.ps1`）—— 或用 `-RestartStackPerTier` 让本脚本自己整栈重启
    · 目标账号已建好且**在 LoginSrv 启动之前**建好：
      `tools/client/account_provision.ps1 -Prefix loadbot -Count 1000`
      （LoginSrv 启动时把账号读进内存，之后再建它看不到）

  为什么多档连跑要 `-RestartStackPerTier`（stack_e2e 的多档路径会自动带上）：本工程实测过"网关连接抖动会让 GameSvr 网关槽位 UserList 置空 →
  进世界玩家在 SetGateUserList 上 NRE 死循环"（§C1 交付说明）。一档结束时**成百上千个假人同时断连**
  正是这种抖动 ⇒ 不重启就测下一档，会拿被污染的栈做基线。重启一次约 40s，换口径干净。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -Tiers 200 -HoldSec 60
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -Tiers 200,500,1000 -RestartStackPerTier
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -SelfTestRed   # 红检：指向无监听端口，必须判红
#>
param(
  # 档位。**故意用 string 收**（同 tools/e2e/stack_e2e.ps1）：`powershell -File` 下
  # `-Tiers 200,500,1000` 会被当成带千分位的整数 2005001000（50 万假人）；string 拿到原样文本，
  # `-File` ⇒ "200,500,1000"、`-Command "& … -Tiers 200,500,1000"` ⇒ "200 500 1000"，
  # 都被下面的 `[,\s]+` 拆回数组。见 tests/loadgate/README.md。
  [string]$Tiers = "200,500,1000",
  [int]$HoldSec = 60,
  [int]$RampTimeoutSec = 600,
  [string]$Prefix = "loadbot",
  [string]$ServerName = "热血传奇",
  [string]$Address = "127.0.0.1",
  [int]$Port = 7000,
  [int]$StaggerMs = 20,
  [int]$MemSampleSec = 2,
  [string]$OutDir = "",
  [switch]$NewAccount,
  [switch]$RestartStackPerTier,
  [switch]$SkipBuild,
  [switch]$SelfTestRed,
  [string]$Criteria = "login,conn,tick",  # 判据子集；M1 ⑤（只比登录+选角）用 "login,conn"
  [switch]$NoActionProbe,                # 关掉假人动作探针（默认开：否则采不到 #+GD ack ⇒ tick 无样本）
  [switch]$AllowMissingTick,             # 显式放行"本档没有 tick 样本"（报告里会写 missing_tick_note）；
                                         # 不加则缺样本一律判 RED——**--Criteria 不能替代本开关**
  [int]$ActionProbeMs = 1000,            # 探针频率：每个假人每 N 毫秒发一个 CM_TURN
  [int]$LoginResendMs = 0,               # T-11 诊断对照：登录包发出后 N ms 内没应答就在同一连接重发一次
                                         # （0=关；只在「残余丢包方向定位」那一轮开，正常门禁不开）
  [int]$Repeat = 1,                      # 同一档位同命令连跑次数（owner 裁决 11dbde39 ①：3 连跑取最差）
  [int]$LoginThresholdPct = 0            # 登录成功率阈值（%）；0 = 按档位取默认（tier>=1000→95，其余 100）
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path

# 档位参数归一化 + 防线（2026-10-11 实测：`-File -Tiers 500,1000` 变 5001000，整档跑满 600s 才 RED）：
#   ① 把 string 拆回 [int[]]：`,  空格 都算分隔符 ⇒ `-File`/`-Command` 两种传法都正确；
#   ② 单档数值超出合理上限即**响亮失败**，不再静默把「逗号拼接」当 50 万人的档位跑掉。
$MaxPlausibleTier = 5000
$TierList = @()
if ($Tiers.Trim() -ne "") {
    $TierList = @($Tiers -split '[,\s]+' | Where-Object { $_ -ne "" } | ForEach-Object { [int]$_ })
}
if ($TierList.Count -eq 0) { throw "档位不能为空" }
if ($TierList.Count -eq 1 -and $TierList[0] -gt $MaxPlausibleTier) {
    throw ("档位 " + $TierList[0] + " 超出上限 " + $MaxPlausibleTier +
           "：疑似经 -File 传递时被逗号拼接（200,500,1000 -> 2005001000）。" +
           "请用 -Tiers '200,500,1000'（-File 下加引号）或 -Command ""& 'tools\botload\load_gate.ps1' -Tiers 200,500,1000""。")
}
if (@($TierList | Where-Object { $_ -le 0 }).Count -gt 0) { throw "档位必须为正整数" }
if ($Repeat -lt 1) { throw "-Repeat 必须 >= 1" }

# 登录判据：默认按档位取可复现下界（tier>=1000 为 95%，其余 100%）。
# 为什么 tier-1000 是 95%：被测栈是 C# 部署栈，1000 并发登录下有 ~0-5% 的连接在发出 CM_IDPASSWORD 后
# 服务端一个字节都不回（见 tests/loadgate/README.md《残余丢包方向》）；这是上游性质，不是 Rust 移植缺陷。
function Get-LoginThreshold([int]$tier) {
    if ($LoginThresholdPct -gt 0) { return [double]$LoginThresholdPct }
    if ($tier -ge 1000) { return 95.0 }
    return 100.0
}

$BotBinDir = Join-Path $RepoRoot "src\BotSrv\bin\Release"
$BotExe = Join-Path $BotBinDir "BotSrv.exe"
$StartAll = "E:\MirServer\start-all.ps1"
$MemProcNames = @("GameSrv", "LoginSrv", "DBSrv", "LoginGate", "SelGate", "GameGate", "mysqld")

if ($SelfTestRed) {
    $TierList = @(3)
    $Port = 17999
    $RampTimeoutSec = 30
    $HoldSec = 5
    Write-Output "SELFTEST_RED: 指向 127.0.0.1:17999（无监听），预期判红"
}

if ($OutDir -eq "") {
    # 默认落在 tests/loadgate/（**可入库**：报告本身是交付物）；大日志在 .gitignore 里排除
    $OutDir = Join-Path $RepoRoot ("tests\loadgate\load-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# ---- 构建 ----
if (-not $SkipBuild) {
    Write-Output "BUILD BotSrv..."
    dotnet build (Join-Path $RepoRoot "src\BotSrv\BotSrv.csproj") -c Release --nologo -v q | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "BotSrv 构建失败" }
}
if (-not (Test-Path $BotExe)) { throw "找不到 $BotExe（先构建 BotSrv）" }

function Read-SharedText([string]$Path) {
    if (-not (Test-Path $Path)) { return "" }
    try {
        $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
        try { return (New-Object System.IO.StreamReader($fs)).ReadToEnd() } finally { $fs.Close() }
    } catch { return "" }
}

function Get-LastStats([string]$Path) {
    $txt = Read-SharedText $Path
    if ($txt -eq "") { return $null }
    $lines = $txt -split "`n" | Where-Object { $_.Trim() -ne "" }
    if ($lines.Count -eq 0) { return $null }
    try { return ($lines[-1] | ConvertFrom-Json) } catch { return $null }
}

function Wait-Port([int]$P, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        if (Get-NetTCPConnection -State Listen -LocalPort $P -ErrorAction SilentlyContinue) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return [bool](Get-NetTCPConnection -State Listen -LocalPort $P -ErrorAction SilentlyContinue)
}

function Restart-Stack {
    if (-not (Test-Path $StartAll)) { throw "找不到 $StartAll（-RestartStackPerTier 需要它）" }
    Write-Output "STACK restart: stop -> start"
    # 坑（2026-10-11 实测）：**不要把 start-all.ps1 的输出接成管道再等它退出**——它用 Start-Process
    # 起的七个服务进程会继承那个管道的写端，父进程退出后管道仍不关闭 ⇒ 调用方永久挂住
    # （上一批的"多档连跑卡死/要杀任务树"就是它）。做法改成：
    #   ① stop 重定向到文件；② start 用 Start-Process 起在独立进程 + 输出重定向到文件；
    #   ③ 只等**端口就绪**（栈健康与否本来就看端口，不等子进程树）。
    & powershell -ExecutionPolicy Bypass -File $StartAll -Stop *> (Join-Path $OutDir "stack-stop.log")
    Start-Process -FilePath "powershell" `
        -ArgumentList @('-ExecutionPolicy', 'Bypass', '-File', $StartAll) -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $OutDir "stack-start.out.log") `
        -RedirectStandardError (Join-Path $OutDir "stack-start.err.log") | Out-Null
    if (-not (Wait-Port $Port 150)) { throw "整栈重启后端口 $Port 未就绪（见 $OutDir\stack-start.*.log）" }
}

$tierReports = @()
for ($run = 1; $run -le $Repeat; $run++) {
if ($Repeat -gt 1) { Write-Output ("===== REPEAT " + $run + "/" + $Repeat + " =====") }
foreach ($n in $TierList) {
    # 三档基线一律从**干净栈**开始（含第一档）：上一批的实测教训是"同一条栈连跑两档，
    # 第二档假人能登录但进不了世界"。整栈重启 ~60s，比拿被污染的栈当基线便宜。
    if ($RestartStackPerTier) { Restart-Stack }

    $tierDir = if ($Repeat -gt 1) { Join-Path $OutDir ("run-" + $run + "\tier-" + $n) } else { Join-Path $OutDir ("tier-" + $n) }
    New-Item -ItemType Directory -Force -Path $tierDir | Out-Null
    $loginThr = Get-LoginThreshold $n
    $rampTarget = [int][math]::Ceiling($n * $loginThr / 100.0)
    Write-Output ("==== TIER " + $n + " (stagger=" + $StaggerMs + "ms, hold=" + $HoldSec + "s, login_threshold=" + $loginThr + "% => ramp_target=" + $rampTarget + "/" + $n + ") -> " + $tierDir)

    # ---- BotSrv 配置 ----
    $cfg = @{
      BotPlay = @{
        ServerName       = $ServerName
        Address          = $Address
        Port             = $Port
        ChrCount         = $n
        TotalChrCount    = $n
        NewAccount       = [bool]$NewAccount
        LoginAccount     = $Prefix
        ConnectStaggerMs = $StaggerMs
      }
    } | ConvertTo-Json -Depth 4
    [System.IO.File]::WriteAllText((Join-Path $BotBinDir "AppSetting.json"), $cfg, (New-Object System.Text.UTF8Encoding($false)))
    # 输出目录：环境变量 + exe 同级 load_out.conf 双保险（经验：Env 在托管启动下不一定继承）
    [System.IO.File]::WriteAllText((Join-Path $BotBinDir "load_out.conf"), $tierDir, (New-Object System.Text.UTF8Encoding($false)))
    $env:MIR2_BOT_OUT = $tierDir
    # 动作探针（tick 样本来源）：默认开。BotSrv 的挂机/移动层是跨假人共享全局态，N 假人动不了，
    # 探针只依赖假人自己的 socket + 场景状态，进世界后每 $ActionProbeMs 发一个 CM_TURN ⇒ 服务端回 #+GD/<rtime>!
    if ($NoActionProbe) { $env:MIR2_BOT_ACTION_PROBE = "0" } else { $env:MIR2_BOT_ACTION_PROBE = "1" }
    $env:MIR2_BOT_ACTION_PROBE_MS = "$ActionProbeMs"
    # T-11 登录重发对照（默认 0=关，保持正常门禁行为不变）
    $env:MIR2_BOT_LOGIN_RESEND_MS = "$LoginResendMs"

    $botLog = Join-Path $tierDir "bots.log"
    $proc = Start-Process -FilePath $BotExe -WorkingDirectory $BotBinDir -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput $botLog -RedirectStandardError (Join-Path $tierDir "bots.err.log")
    Write-Output ("BOTSRV_PID=" + $proc.Id)

    # ---- 爬坡：等登录数到 N ----
    $statsPath = Join-Path $tierDir "load_stats.ndjson"
    $rampDeadline = (Get-Date).AddSeconds($RampTimeoutSec)
    $rampOk = $false
    while ((Get-Date) -lt $rampDeadline) {
        Start-Sleep -Seconds 2
        $st = Get-LastStats $statsPath
        if ($st -ne $null) {
            Write-Output ("  RAMP login_ok=" + $st.login_ok + "/" + $n + " spawned=" + $st.spawned + " tick_samples=" + $st.tick_samples)
            if ([int]$st.login_ok -ge $rampTarget) { $rampOk = $true; break }
        }
        if ($proc.HasExited) { Write-Output ("  WARN BotSrv 退出 code=" + $proc.ExitCode); break }
    }
    if (-not $rampOk) { Write-Output ("  RED 爬坡未达阈值（" + $RampTimeoutSec + "s 内未到 " + $rampTarget + "/" + $n + "）") }

    # ---- 稳态采样：内存曲线（tick 由 BotSrv 自己持续落盘）----
    $memPath = Join-Path $tierDir "mem.ndjson"
    $memLines = New-Object System.Collections.Generic.List[string]
    $holdDeadline = (Get-Date).AddSeconds($HoldSec)
    while ((Get-Date) -lt $holdDeadline) {
        $ts = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
        foreach ($name in $MemProcNames) {
            $ps = Get-Process -Name $name -ErrorAction SilentlyContinue
            foreach ($p in $ps) {
                $ws = [math]::Round($p.WorkingSet64 / 1MB, 2)
                $pv = [math]::Round($p.PrivateMemorySize64 / 1MB, 2)
                $cpu = $p.TotalProcessorTime.TotalSeconds
                $memLines.Add(('{{"t_ms":{0},"proc":"{1}","pid":{2},"ws_mb":{3},"priv_mb":{4},"cpu_s":{5}}}' -f $ts, $name, $p.Id, $ws, $pv, [math]::Round($cpu, 2)))
            }
        }
        Start-Sleep -Seconds $MemSampleSec
    }
    [System.IO.File]::WriteAllLines($memPath, $memLines)

    # ---- 停 BotSrv（收尾前再读一次统计）----
    if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 1

    # ---- 归算 ----
    $reportPath = Join-Path $tierDir "report.json"
    $criteriaArgs = @("--criteria", $Criteria, "--login-threshold", "$loginThr")
    $loginConnsPath = Join-Path $tierDir "login_conns.ndjson"
    if (Test-Path $loginConnsPath) { $criteriaArgs += @("--login-conns", $loginConnsPath) }
    if ($AllowMissingTick) { $criteriaArgs += "--allow-missing-tick" }
    python (Join-Path $RepoRoot "tools\botload\load_report.py") summarize --stats $statsPath --mem $memPath `
        --count $n --stagger-ms $StaggerMs --hold-sec $HoldSec @criteriaArgs --out $reportPath | Out-Null
    $tierRc = $LASTEXITCODE
    $tierJson = $null
    if (Test-Path $reportPath) { $tierJson = Get-Content $reportPath -Raw | ConvertFrom-Json }
    if ($tierJson -ne $null) {
        $tierJson | Add-Member -NotePropertyName run -NotePropertyValue $run -Force
        Write-Output ("  RESULT tier=" + $n + " verdict=" + $tierJson.verdict +
                      " login=" + $tierJson.login_success_pct + "% conn_lost_steady=" + $tierJson.conn_lost_steady +
                      " tick_p50=" + $tierJson.tick_p50_ms + " p90=" + $tierJson.tick_p90_ms +
                      " p99=" + $tierJson.tick_p99_ms + "ms samples=" + $tierJson.samples +
                      " in_world=" + $tierJson.world_ok + " probe_sent=" + $tierJson.probe_sent)
        if ($tierJson.mem.PSObject.Properties.Name -contains "GameSrv") {
            $g = $tierJson.mem.GameSrv
            Write-Output ("  MEM GameSvr ws_peak=" + $g.ws_peak_mb + "MB mean=" + $g.ws_mean_mb +
                          "MB slope_tail=" + $g.slope_tail_mb_per_s + "MB/s")
        }
        $tierReports += $tierJson
    } else {
        Write-Output "  RED 归算没产出 report.json"
    }
    if ($tierRc -ne 0) { Write-Output ("  (tier rc=" + $tierRc + ")") }
}
}

# ---- 汇总 ----
# 同命令多连跑取最差（owner 裁决 11dbde39 ①）：按档位把 N 轮的登录率取 min，阈值不变。
$worstByTier = [ordered]@{}
foreach ($t in $tierReports) {
    $key = [string][int]$t.count
    if (-not $worstByTier.Contains($key)) {
        $worstByTier[$key] = @{
            count = [int]$t.count
            threshold = (Get-LoginThreshold ([int]$t.count))
            worst_login_pct = [double]$t.login_success_pct
            runs = @()
        }
    }
    $e = $worstByTier[$key]
    $lp = [double]$t.login_success_pct
    if ($lp -lt $e.worst_login_pct) { $e.worst_login_pct = $lp }
    $e.runs += @{
        run = $t.run
        login_success_pct = $lp
        verdict = $t.verdict
        conn_lost_steady = $t.conn_lost_steady
        internal_errors = $t.internal_errors
        samples = $t.samples
        tick_p99_ms = $t.tick_p99_ms
    }
}
foreach ($key in @($worstByTier.Keys)) {
    $e = $worstByTier[$key]
    $ok = ($e.worst_login_pct -ge $e.threshold)
    foreach ($r in $e.runs) { if ($r.verdict -ne "GREEN") { $ok = $false } }
    $e.verdict = if ($ok) { "GREEN" } else { "RED" }
}

$summaryPath = Join-Path $OutDir "load_gate_summary.json"
@{
  out_dir    = $OutDir
  tiers      = $TierList
  repeat     = $Repeat
  stagger_ms = $StaggerMs
  hold_sec   = $HoldSec
  caliber    = "tick 服务时延 = BotSrv 收到 `#+GD/<rtime>! 的时刻 − rtime（Environment.TickCount，全机同域）；详见 tools/botload/load_report.py"
  criteria   = $Criteria
  allow_missing_tick = [bool]$AllowMissingTick
  action_probe = (-not $NoActionProbe)
  action_probe_ms = $ActionProbeMs
  login_resend_ms = $LoginResendMs
  login_threshold_pct = $LoginThresholdPct
  worst_of_runs_rule = "每档取 N 轮里最低的登录成功率与阈值比；且每一轮的 conn/tick/internal 判据都必须 GREEN"
  worst_by_tier = $worstByTier
  reports    = $tierReports
} | ConvertTo-Json -Depth 8 -WarningAction SilentlyContinue | Set-Content -Path $summaryPath -Encoding utf8
Write-Output ("SUMMARY " + $summaryPath)

$allGreen = $tierReports.Count -gt 0
foreach ($key in @($worstByTier.Keys)) { if ($worstByTier[$key].verdict -ne "GREEN") { $allGreen = $false } }
foreach ($e in @($worstByTier.Values)) {
    Write-Output ("  WORST tier=" + $e.count + " runs=" + @($e.runs).Count + " worst_login=" + $e.worst_login_pct + "% threshold=" + $e.threshold + "% verdict=" + $e.verdict)
}

if ($SelfTestRed) {
    if (-not $allGreen) {
        Write-Output "SELFTEST_RED GREEN: 连不通 => 判红成立（红检本身按预期变红 = 自测通过）"
        exit 0
    }
    Write-Output "SELFTEST_RED RED: 连不通却判绿 —— 判据失效！"
    exit 1
}

if ($tierReports.Count -eq 0) { Write-Output "RED: 一档都没跑成"; exit 1 }
if ($allGreen) {
    Write-Output ("GREEN: " + $tierReports.Count + " 轮次全过（登录 ≥ 档位阈值 / 稳态无掉线 / tick P99 ≤ 100ms）")
    exit 0
}
Write-Output "RED: 有档位未达标（见各 tier 的 report.json）"
exit 1
