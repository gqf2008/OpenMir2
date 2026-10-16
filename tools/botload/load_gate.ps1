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

  内存护栏（2026-10-11 死机取证后补）：起 BotSrv 前注入进程级 GC 堆硬上限（`-BotHeapLimitGB`，默认 4GiB，
  起进程前打印审计行 `BOTGUARD_ENV …`）；稳态采样**含 BotSrv 自己**，任一被采样进程私有字节/工作集越过
  `-MemKillThresholdGB`（默认 8GiB）即杀 BotSrv + 打 RED `BOTGUARD_MEM_BREACH` 并收尾（mem.ndjson /
  报告照写，绝不静默）；`-MaxRunSec`/`-MaxMemSamples` 给无人值守跑一个显式上界。helper 见
  tools/botload/BotGuard.psm1，离线自检 tools/m5-acceptance/check_botguard.ps1。

  越界 = **环境级异常，提前终止整个门禁**（独立复验者 must_fix #2，2026-10-16）：越界（或全门禁墙钟上界）
  触发后 **不再跑剩余档位**（之前只 break 了内层稳态循环，外层 tier/repeat 会继续并**每档再杀一次 BotSrv**），
  在 `load_gate_summary.json` 的 `bot_guard.aborted_early/abort_reason/abort_note` 里写明"因护栏越界提前终止，
  剩余档位未跑"。

  保护体检（独立复验者 must_fix #1/#2，2026-10-16；B3 收口 2026-10-16）：`-BotHeapLimitGB` 与
  `-MemKillThresholdGB` 两项**都失效**⇒ **拒绝启动**（fail-closed，rc=2）；只失效一项 ⇒ 审计行与 summary
  显式标红该保护缺失。默认配置恒为"有保护"（FULL）。**"有效"不是"取值 > 0"**：一项护栏有效 = 阈值 <
  有效上界（= 机器推导天花板 = `本机提交上限 × 0.5`，或操作者给的更严上界）。`-MemKillThresholdCeilingGB`
  只能把上界调**更严**、**不能放大**（放大值被夹回机器推导值）——见 BotGuard.psm1 ④ 与 check_botguard.ps1 牙⑥。

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
  [int]$LoginThresholdPct = 0,           # 登录成功率阈值（%）；0 = 按档位取默认（tier>=1000→95，其余 100）
  # ---- 内存护栏（2026-10-11 死机取证后补）----
  [int]$BotHeapLimitGB = 4,              # 起 BotSrv 前注入的进程级 GC 堆硬上限（GiB）；0=不注入
  [double]$MemKillThresholdGB = 8,       # 看门狗阈值（GiB）：任一被采样进程越过即杀 BotSrv + RED；0=关
  [double]$MemKillThresholdCeilingGB = 0,  # 保护体检上界（GiB）：可选，只能把"有效上界"调**更严**（0=不额外设限，用机推上限）；给成比机推上限大无用（被夹回，见 BotGuard.psm1 ④）
  [int]$MaxRunSec = 10800,               # 全门禁墙钟上界（秒）；到点正常停 BotSrv + 收尾；0=不限
  [int]$MaxMemSamples = 0                # 每档稳态采样迭代上界；0=仅由 HoldSec 决定
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
# 内存护栏共享 helper（进程级堆上限 / 看门狗 / 有界跑）；见 BotGuard.psm1 头部取证说明。
Import-Module (Join-Path $Here "BotGuard.psm1") -Force

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
# BotSrv 也算被采样进程：M5 W2 需要"假人内存斜率"，且看门狗要能看见假人自己越界（2026-10-11 的 106GB 就是它）。
$MemProcNames = @("GameSrv", "LoginSrv", "DBSrv", "LoginGate", "SelGate", "GameGate", "mysqld", "BotSrv")
$MemKillThresholdBytes = if ($MemKillThresholdGB -gt 0) { [int64]($MemKillThresholdGB * 1GB) } else { [int64]0 }
$GuardStart = Get-Date

# 护栏④（must_fix #1，2026-10-16）：保护体检 —— 默认须有保护，任何"零保护"组合都不许悄悄跑起来。
# 放在一切副作用（建目录 / 构建 / 起进程）之前：零护栏时**根本不进入执行**。
$guardProtection = Get-BotGuardProtection -HeapLimitGb $BotHeapLimitGB -MemKillThresholdGb $MemKillThresholdGB `
    -ThresholdCeilingGb $MemKillThresholdCeilingGB
Write-Output (Format-BotGuardProtectionAudit -Protection $guardProtection)
if ($guardProtection.refuse_start) {
    Write-Output ("RED: BotGuard 拒绝启动 —— heap_limit 与 watchdog 同时缺失（bot_heap_limit_gb=" +
                  $BotHeapLimitGB + " mem_kill_threshold_gb=" + $MemKillThresholdGB +
                  " 机推上限=" + $guardProtection.threshold_ceiling_gb + "GiB）；" +
                  "不允许在零护栏下长跑。请至少打开一项：-BotHeapLimitGB > 0 且 < 机推上限，或 0 < -MemKillThresholdGB < 机推上限（本机提交上限 × 0.5）。")
    exit 2
}

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

# 本机负载探针（owner 裁决 82f8e0c3 ④；口径照 tickbench 的 `probe_load_window`：
# CPU 均值（%）+ 重型进程数 rustc/cargo/cargo-clippy/msbuild）。它只**标注**不判红：
# tick P99 在共享机器上不可归属（C# 参考栈 vs 同机并发编译），需要空载窗口才能定。
function Get-HostLoadLine([long]$ts) {
    $cpu = -1
    try { $cpu = [int]((Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average) } catch { $cpu = -1 }
    $heavy = @(Get-Process -Name rustc,cargo,cargo-clippy,msbuild -ErrorAction SilentlyContinue)
    $names = (@($heavy | ForEach-Object { $_.ProcessName }) | Sort-Object -Unique) -join ","
    return ('{{"t_ms":{0},"cpu_pct":{1},"heavy_procs":{2},"heavy_names":"{3}"}}' -f $ts, $cpu, $heavy.Count, $names)
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

$guardBreachAny = $false   # 任一档触发内存看门狗（越界）
$guardBoundAny  = $false   # 任一档触发有界跑上界
# 越界 / 全门禁墙钟上界 = 环境级异常 ⇒ 提前终止整个门禁（停掉后续档位），而不是只 break 内层稳态循环
# （must_fix #2：原实现只 break 内层，外层 tier/repeat 仍继续、每档再杀一次 BotSrv）。
$guardAbortReason = $null  # 'mem_breach' | 'wall_bound' | $null
$tierReports = @()
$guardTiersRun = 0
:runLoop for ($run = 1; $run -le $Repeat; $run++) {
if ($Repeat -gt 1) { Write-Output ("===== REPEAT " + $run + "/" + $Repeat + " =====") }
:tierLoop foreach ($n in $TierList) {
    $guardTiersRun++
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

    # 护栏①：起进程前注入进程级 GC 堆硬上限，并把最终生效的 env 打出来（可审计）
    $envOverrides = New-BotGuardEnvOverrides -HeapLimitGb $BotHeapLimitGB
    Set-BotGuardEnv -Overrides $envOverrides
    Write-Output ("  " + (Format-BotGuardEnvAudit -Exe $BotExe -Overrides $envOverrides -HeapLimitGb $BotHeapLimitGB))

    $botLog = Join-Path $tierDir "bots.log"
    $proc = Start-Process -FilePath $BotExe -WorkingDirectory $BotBinDir -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput $botLog -RedirectStandardError (Join-Path $tierDir "bots.err.log")
    Write-Output ("BOTSRV_PID=" + $proc.Id)

    # ---- 爬坡：等登录数到 N ----
    $statsPath = Join-Path $tierDir "load_stats.ndjson"
    $hostLoadPath = Join-Path $tierDir "hostload.ndjson"
    $hostLoadLines = New-Object System.Collections.Generic.List[string]
    $rampDeadline = (Get-Date).AddSeconds($RampTimeoutSec)
    $rampOk = $false
    while ((Get-Date) -lt $rampDeadline) {
        Start-Sleep -Seconds 2
        $tsRamp = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
        $hostLoadLines.Add((Get-HostLoadLine $tsRamp))
        $st = Get-LastStats $statsPath
        if ($st -ne $null) {
            Write-Output ("  RAMP login_ok=" + $st.login_ok + "/" + $n + " spawned=" + $st.spawned + " tick_samples=" + $st.tick_samples)
            if ([int]$st.login_ok -ge $rampTarget) { $rampOk = $true; break }
        }
        if ($proc.HasExited) { Write-Output ("  WARN BotSrv 退出 code=" + $proc.ExitCode); break }
        # 护栏④：有界跑——墙钟上界（爬坡阶段也算），到点即停 BotSrv 收尾
        $elapsedRamp = ((Get-Date) - $GuardStart).TotalSeconds
        if (Test-BotRunBound -ElapsedSec $elapsedRamp -MaxRunSec $MaxRunSec -Iteration 0 -MaxIterations 0) {
            $guardBoundAny = $true
            $guardAbortReason = 'wall_bound'
            Write-Output ("  BOTGUARD_BOUND wall-clock during ramp: elapsed=" + [int]$elapsedRamp + "s max=" + $MaxRunSec + "s")
            if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
            break
        }
    }
    if (-not $rampOk) { Write-Output ("  RED 爬坡未达阈值（" + $RampTimeoutSec + "s 内未到 " + $rampTarget + "/" + $n + "）") }

    # ---- 稳态采样：内存曲线（tick 由 BotSrv 自己持续落盘）----
    $memPath = Join-Path $tierDir "mem.ndjson"
    $memLines = New-Object System.Collections.Generic.List[string]
    $holdDeadline = (Get-Date).AddSeconds($HoldSec)
    $memSampleIter = 0
    $memBreach = $null
    while ((Get-Date) -lt $holdDeadline) {
        $ts = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
        $hostLoadLines.Add((Get-HostLoadLine $ts))
        # 本轮采样：既落 mem.ndjson（含 BotSrv），也喂给看门狗做越界判定
        $iterSamples = New-Object System.Collections.Generic.List[object]
        foreach ($name in $MemProcNames) {
            $ps = Get-Process -Name $name -ErrorAction SilentlyContinue
            foreach ($p in $ps) {
                $ws = [math]::Round($p.WorkingSet64 / 1MB, 2)
                $pv = [math]::Round($p.PrivateMemorySize64 / 1MB, 2)
                $cpu = $p.TotalProcessorTime.TotalSeconds
                $memLines.Add(('{{"t_ms":{0},"proc":"{1}","pid":{2},"ws_mb":{3},"priv_mb":{4},"cpu_s":{5}}}' -f $ts, $name, $p.Id, $ws, $pv, [math]::Round($cpu, 2)))
                $iterSamples.Add([pscustomobject]@{ proc = $name; pid = $p.Id; priv_bytes = [int64]$p.PrivateMemorySize64; ws_bytes = [int64]$p.WorkingSet64 })
            }
        }
        $memSampleIter++
        # 护栏②：看门狗——任一被采样进程越界 ⇒ 杀 BotSrv + RED + 提前终止外层档位循环（绝不静默）
        $breach = Find-MemBreach -Samples $iterSamples -ThresholdBytes $MemKillThresholdBytes
        if ($breach) {
            $memBreach = $breach
            $guardBreachAny = $true
            $guardAbortReason = 'mem_breach'
            Write-Output ("  " + (Format-BotGuardBreachRed -Breach $breach))
            foreach ($kid in @(Stop-BotSrvForBreach -Breach $breach -BotPids @($proc.Id))) { Write-Output ("  BOTGUARD_KILL pid=" + $kid) }
            break
        }
        # 护栏④：有界跑——每档稳态采样迭代上界
        $iterBound = Test-BotRunBound -ElapsedSec 0 -MaxRunSec 0 -Iteration $memSampleIter -MaxIterations $MaxMemSamples
        if ($iterBound) {
            $guardBoundAny = $true
            Write-Output ("  BOTGUARD_BOUND iterations: sample_iter=" + $memSampleIter + " max=" + $MaxMemSamples)
            break
        }
        # 护栏④：有界跑——全门禁墙钟上界
        $elapsed = ((Get-Date) - $GuardStart).TotalSeconds
        $wallBound = Test-BotRunBound -ElapsedSec $elapsed -MaxRunSec $MaxRunSec -Iteration 0 -MaxIterations 0
        if ($wallBound) {
            $guardBoundAny = $true
            $guardAbortReason = 'wall_bound'
            Write-Output ("  BOTGUARD_BOUND wall-clock: elapsed=" + [int]$elapsed + "s max=" + $MaxRunSec + "s")
            if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
            break
        }
        Start-Sleep -Seconds $MemSampleSec
    }
    [System.IO.File]::WriteAllLines($memPath, $memLines)
    [System.IO.File]::WriteAllLines($hostLoadPath, $hostLoadLines)

    # ---- 停 BotSrv（收尾前再读一次统计）----
    if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 1

    # ---- 归算 ----
    $reportPath = Join-Path $tierDir "report.json"
    $criteriaArgs = @("--criteria", $Criteria, "--login-threshold", "$loginThr")
    $loginConnsPath = Join-Path $tierDir "login_conns.ndjson"
    if (Test-Path $loginConnsPath) { $criteriaArgs += @("--login-conns", $loginConnsPath) }
    if (Test-Path $hostLoadPath) { $criteriaArgs += @("--hostload", $hostLoadPath) }
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
        if ($tierJson.host_load -ne $null) {
            $hl = $tierJson.host_load
            Write-Output ("  HOSTLOAD cpu_mean=" + $hl.cpu_mean_pct + "% cpu_peak=" + $hl.cpu_peak_pct +
                          "% heavy_max=" + $hl.heavy_procs_max + " names=" + ($hl.heavy_names_seen -join ","))
        }
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
    # 越界 / 墙钟上界 ⇒ 停掉后续档位（环境级异常；不再对下一档重复起停 BotSrv）
    if ($guardAbortReason) { break tierLoop }
}
if ($guardAbortReason) { break runLoop }
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
        in_world = $t.world_ok
        world_ok_pct = $t.world_ok_pct
        host_cpu_peak_pct = $(if ($t.host_load) { $t.host_load.cpu_peak_pct } else { $null })
        host_heavy_max = $(if ($t.host_load) { $t.host_load.heavy_procs_max } else { $null })
    }
}
foreach ($key in @($worstByTier.Keys)) {
    $e = $worstByTier[$key]
    $ok = ($e.worst_login_pct -ge $e.threshold)
    foreach ($r in $e.runs) { if ($r.verdict -ne "GREEN") { $ok = $false } }
    $e.verdict = if ($ok) { "GREEN" } else { "RED" }
}

# F9（C6 原卡 reviewer-4）：产物 {file,sha256} 清单——供复核者核对引用件未被后续冒烟覆写。
$artifactFiles = @()
foreach ($pat in @("report.json", "load_stats.ndjson", "mem.ndjson", "hostload.ndjson", "login_conns.ndjson")) {
    $artifactFiles += @(Get-ChildItem -Path $OutDir -Recurse -Filter $pat -ErrorAction SilentlyContinue)
}
$artifactManifest = @()
foreach ($f in ($artifactFiles | Sort-Object FullName)) {
    $rel = $f.FullName.Substring($OutDir.Length).TrimStart('\', '/')
    $artifactManifest += @{ file = $rel; sha256 = (Get-FileHash -Algorithm SHA256 -Path $f.FullName).Hash.ToLower() }
}
@{ generated_ms = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()); files = $artifactManifest } |
    ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $OutDir "artifacts.sha256.json") -Encoding utf8

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
  bot_guard = @{
    protection_level       = $guardProtection.level
    protection_missing     = @($guardProtection.missing)
    refuse_start           = [bool]$guardProtection.refuse_start
    threshold_ceiling_gb   = [double]$guardProtection.threshold_ceiling_gb
    requested_ceiling_gb   = [double]$guardProtection.requested_ceiling_gb
    machine_commit_limit_gb= [double]$guardProtection.machine_commit_limit_gb
    ceiling_clamped        = [bool]$guardProtection.ceiling_clamped
    bot_heap_limit_gb      = $BotHeapLimitGB
    mem_kill_threshold_gb  = $MemKillThresholdGB
    max_run_sec            = $MaxRunSec
    max_mem_samples        = $MaxMemSamples
    mem_proc_names         = $MemProcNames
    breach_tripped         = [bool]$guardBreachAny
    bound_tripped          = [bool]$guardBoundAny
    aborted_early          = [bool](-not [string]::IsNullOrEmpty($guardAbortReason))
    abort_reason           = $guardAbortReason
    tiers_planned          = ($TierList.Count * $Repeat)
    tiers_run              = $guardTiersRun
    abort_note             = $(if ($guardAbortReason -eq 'mem_breach') { "因护栏越界提前终止，剩余档位未跑（abort_reason=mem_breach）" } elseif ($guardAbortReason) { "因护栏墙钟上界提前终止，剩余档位未跑（abort_reason=" + $guardAbortReason + "）" } else { "" })
  }
  action_probe = (-not $NoActionProbe)
  action_probe_ms = $ActionProbeMs
  login_resend_ms = $LoginResendMs
  login_threshold_pct = $LoginThresholdPct
  worst_of_runs_rule = "每档取 N 轮里最低的登录成功率与阈值比；且每一轮的 conn/tick/internal 判据都必须 GREEN"
  host_load_caliber = "CPU=Win32_Processor.LoadPercentage 均值(%)；heavy=rustc/cargo/cargo-clippy/msbuild 进程数（叧 tickbench）；只标注不判红"
  host_load_note = "tick P99 在共享机器上不可归属（C# 参考栈 vs 同机并发编译）；CPU 峰≥40% 或 heavy 峰≥4 ⇒ 该轮 P99 未归属，需空载窗口复测"
  artifacts  = $artifactManifest
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
if ($guardBreachAny) {
    Write-Output ("RED: BotGuard 内存看门狗触发（有进程越界，已杀 BotSrv，提前终止；tiers_run=" + $guardTiersRun + "/" +
                  ($TierList.Count * $Repeat) + "，剩余档位未跑）—— 见各档 mem.ndjson 与上面的 RED BOTGUARD_MEM_BREACH 行")
    exit 1
}
if ($guardBoundAny) {
    Write-Output ("RED: BotGuard 有界跑上界触发（MaxRunSec=" + $MaxRunSec + " / MaxMemSamples=" + $MaxMemSamples +
                  "；tiers_run=" + $guardTiersRun + "/" + ($TierList.Count * $Repeat) + "）—— 本轮未跑完，判不 GREEN")
    exit 1
}
if ($allGreen) {
    Write-Output ("GREEN: " + $tierReports.Count + " 轮次全过（登录 ≥ 档位阈值 / 稳态无掉线 / tick P99 ≤ 100ms）")
    exit 0
}
Write-Output "RED: 有档位未达标（见各 tier 的 report.json）"
exit 1
