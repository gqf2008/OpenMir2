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

  为什么默认 `-RestartStackPerTier`：本工程实测过"网关连接抖动会让 GameSvr 网关槽位 UserList 置空 →
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
  [int[]]$Tiers = @(200, 500, 1000),
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
  [switch]$SelfTestRed
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
$BotBinDir = Join-Path $RepoRoot "src\BotSrv\bin\Release"
$BotExe = Join-Path $BotBinDir "BotSrv.exe"
$StartAll = "E:\MirServer\start-all.ps1"
$MemProcNames = @("GameSrv", "LoginSrv", "DBSrv", "LoginGate", "SelGate", "GameGate", "mysqld")

if ($SelfTestRed) {
    $Tiers = @(3)
    $Port = 17999
    $RampTimeoutSec = 30
    $HoldSec = 5
    Write-Output "SELFTEST_RED: 指向 127.0.0.1:17999（无监听），预期判红"
}

if ($OutDir -eq "") {
    $OutDir = Join-Path $RepoRoot ("tests\botload\load-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
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
    & powershell -ExecutionPolicy Bypass -File $StartAll -Stop 2>&1 | Out-Null
    & powershell -ExecutionPolicy Bypass -File $StartAll 2>&1 | Out-Null
    if (-not (Wait-Port $Port 90)) { throw "整栈重启后端口 $Port 未就绪" }
}

$tierReports = @()
$firstTier = $true
foreach ($n in $Tiers) {
    if ($RestartStackPerTier -and -not $firstTier) { Restart-Stack }
    $firstTier = $false

    $tierDir = Join-Path $OutDir ("tier-" + $n)
    New-Item -ItemType Directory -Force -Path $tierDir | Out-Null
    Write-Output ("==== TIER " + $n + " (stagger=" + $StaggerMs + "ms, hold=" + $HoldSec + "s) -> " + $tierDir)

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
            if ([int]$st.login_ok -ge $n) { $rampOk = $true; break }
        }
        if ($proc.HasExited) { Write-Output ("  WARN BotSrv 退出 code=" + $proc.ExitCode); break }
    }
    if (-not $rampOk) { Write-Output ("  RED 爬坡未完成（" + $RampTimeoutSec + "s 内未到 " + $n + "）") }

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
    python (Join-Path $RepoRoot "tools\botload\load_report.py") summarize --stats $statsPath --mem $memPath `
        --count $n --stagger-ms $StaggerMs --hold-sec $HoldSec --out $reportPath | Out-Null
    $tierRc = $LASTEXITCODE
    $tierJson = $null
    if (Test-Path $reportPath) { $tierJson = Get-Content $reportPath -Raw | ConvertFrom-Json }
    if ($tierJson -ne $null) {
        Write-Output ("  RESULT tier=" + $n + " verdict=" + $tierJson.verdict +
                      " login=" + $tierJson.login_success_pct + "% conn_lost_steady=" + $tierJson.conn_lost_steady +
                      " tick_p50=" + $tierJson.tick_p50_ms + " p90=" + $tierJson.tick_p90_ms +
                      " p99=" + $tierJson.tick_p99_ms + "ms samples=" + $tierJson.samples)
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

# ---- 汇总 ----
$summaryPath = Join-Path $OutDir "load_gate_summary.json"
@{
  out_dir    = $OutDir
  tiers      = $Tiers
  stagger_ms = $StaggerMs
  hold_sec   = $HoldSec
  caliber    = "tick 服务时延 = BotSrv 收到 `#+GD/<rtime>! 的时刻 − rtime（Environment.TickCount，全机同域）；详见 tools/botload/load_report.py"
  reports    = $tierReports
} | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding utf8
Write-Output ("SUMMARY " + $summaryPath)

$allGreen = $true
foreach ($t in $tierReports) { if ($t.verdict -ne "GREEN") { $allGreen = $false } }

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
    Write-Output ("GREEN: " + $tierReports.Count + " 档全过（登录 100% / 稳态无掉线 / tick P99 ≤ 100ms）")
    exit 0
}
Write-Output "RED: 有档位未达标（见各 tier 的 report.json）"
exit 1
