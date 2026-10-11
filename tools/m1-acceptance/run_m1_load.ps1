#requires -Version 5
<#
.SYNOPSIS
  run_m1_load.ps1 — M1⑤②③：对**全 Rust 边界链**跑一层假人（默认 200）登录+选角，
  取 BotSrv 侧计数与 Rust 服务侧日志/计数，并按 `tools/botload/load_report.py` 的同一口径归算内存曲线。

.DESCRIPTION
  - 假人编排**只读复用** `tools/botload/load_gate.ps1`（C6 线的件，本脚本不改它）；
    判据子集固定 `login,conn`（M1⑤ 只要求登录 + 选角，不要求 tick）。
  - 内存曲线：`load_gate.ps1` 的 `MemProcNames` 是 C# 进程名（GameSrv/LoginSrv/…），拿不到 Rust 进程
    ⇒ 本脚本在 load 期间**另起一个采样作业**采 `login-gate/login-srv/sel-gate/db-srv` 的 WS/私存/CPU
    （schema 与 load_gate 一致：`t_ms/proc/pid/ws_mb/priv_mb/cpu_s`），再用 `load_report.py summarize`
    （**同一口径**）归算出 `report-rust.json`（含 `slope_tail_mb_per_s`）。
  - Rust 侧证据：从 `-StackLogDir` 的服务 stdout 里数 `[登录] 帐号=` / `[选服] account=`（login-srv）、
    `[QueryChr]` 与 `玩家获取游戏网关信息`（db-srv），并扫 `panicked`；BotSrv 侧从 `bots.log` 数
    `查询角色.` / `选择角色 ` / `准备进入游戏`。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m1-acceptance/run_m1_load.ps1 -Tier 200 -HoldSec 60 `
    -OutDir E:\tmp\openmir2-m1load\evidence\rust-200
#>
param(
  [int]$Tier = 200,
  [int]$HoldSec = 60,
  [string]$OutDir = 'E:\tmp\openmir2-m1load\evidence\rust-200',
  [string]$StackLogDir = 'E:\tmp\openmir2-m1load\stack\logs',
  [string]$Prefix = 'loadbot',
  [int]$MemSampleSec = 2,
  [int]$RampTimeoutSec = 120,
  [switch]$SkipBuild,
  [switch]$SkipLoad   # 跳过起假人/采样，只重跑「两侧证据计数」（服务端 stdout 被进程持有，读时要共享）
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..')).Path
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$RustProcs = @('login-gate', 'login-srv', 'sel-gate', 'db-srv', 'mysqld')

# 服务端 stdout 日志在运行期被进程持有（写锁）⇒ 必须用 FileShare.ReadWrite 共享读，
# 否则 [System.IO.File]::ReadAllLines 会抛「being used by another process」（实测踩过）。
function Read-SharedLines([string]$Path) {
  if (-not (Test-Path $Path)) { return @() }
  $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
  try {
    $sr = New-Object System.IO.StreamReader($fs)
    try { return ($sr.ReadToEnd() -split "`n") } finally { $sr.Close() }
  } finally { $fs.Close() }
}

function Count-Matches([string]$Path, [string]$Pattern) {
  if (-not (Test-Path $Path)) { return -1 }
  $lines = Read-SharedLines $Path
  $n = 0
  foreach ($line in $lines) { if ($line -match $Pattern) { $n++ } }
  return $n
}

$tierDir = Join-Path $OutDir ("tier-" + $Tier)
$statsPath = Join-Path $tierDir 'load_stats.ndjson'
$reportPath = Join-Path $OutDir 'report-rust.json'

if (-not $SkipLoad) {
# ---- 1) 起内存采样作业（load 期间）----
$memPath = Join-Path $OutDir 'mem-rust.ndjson'
Remove-Item $memPath -Force -ErrorAction SilentlyContinue
$stopFile = Join-Path $OutDir '.mem-stop'
Remove-Item $stopFile -Force -ErrorAction SilentlyContinue
$sampler = Start-Job -ArgumentList $memPath, $stopFile, ($RustProcs -join ','), $MemSampleSec -ScriptBlock {
  param($memPath, $stopFile, $procsCsv, $sampleSec)
  $procs = $procsCsv -split ','
  while (-not (Test-Path $stopFile)) {
    $ts = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
    foreach ($name in $procs) {
      foreach ($p in (Get-Process -Name $name -ErrorAction SilentlyContinue)) {
        $ws = [math]::Round($p.WorkingSet64 / 1MB, 2)
        $pv = [math]::Round($p.PrivateMemorySize64 / 1MB, 2)
        $cpu = [math]::Round($p.TotalProcessorTime.TotalSeconds, 2)
        $line = ('{{"t_ms":{0},"proc":"{1}","pid":{2},"ws_mb":{3},"priv_mb":{4},"cpu_s":{5}}}' -f $ts, $name, $p.Id, $ws, $pv, $cpu)
        [System.IO.File]::AppendAllText($memPath, $line + "`n")
      }
    }
    Start-Sleep -Seconds $sampleSec
  }
}
Write-Output ("SAMPLER_JOB=" + $sampler.Id + " -> " + $memPath)

# ---- 2) 跑假人（只读复用 load_gate.ps1）----
$loadGate = Join-Path $RepoRoot 'tools\botload\load_gate.ps1'
$lgArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $loadGate,
            '-Tiers', "$Tier", '-HoldSec', "$HoldSec", '-Criteria', 'login,conn', '-OutDir', $OutDir,
            '-RampTimeoutSec', "$RampTimeoutSec")
if ($SkipBuild) { $lgArgs += '-SkipBuild' }
$lgLog = Join-Path $OutDir 'load_gate.log'
& powershell @lgArgs 2>&1 | Tee-Object -FilePath $lgLog | ForEach-Object { Write-Output $_ }
$lgRc = $LASTEXITCODE

# ---- 3) 收采样 ----
New-Item -ItemType File -Force -Path $stopFile | Out-Null
Wait-Job $sampler -Timeout 30 | Out-Null
Stop-Job $sampler -ErrorAction SilentlyContinue
Remove-Job $sampler -Force -ErrorAction SilentlyContinue
Remove-Item $stopFile -Force -ErrorAction SilentlyContinue

# ---- 4) 按同一口径归算 Rust 内存曲线 ----
if ((Test-Path $statsPath) -and (Test-Path $memPath)) {
  python (Join-Path $RepoRoot 'tools\botload\load_report.py') summarize --stats $statsPath --mem $memPath `
    --count $Tier --criteria 'login,conn' --out $reportPath | Out-Null
  Write-Output ("REPORT_RUST " + $reportPath)
}
} # end -not $SkipLoad

# ---- 5) 两侧证据计数 ----
$lgsOut = Join-Path $StackLogDir 'login-srv.out.log'
$dbsOut = Join-Path $StackLogDir 'db-srv.out.log'
$sgsOut = Join-Path $StackLogDir 'sel-gate.out.log'
$lgOut  = Join-Path $StackLogDir 'login-gate.out.log'
$botsLog = Join-Path $tierDir 'bots.log'
$counts = [ordered]@{
  tier = $Tier
  hold_sec = $HoldSec
  criteria = 'login,conn'
  load_gate_rc = $lgRc
  bots = [ordered]@{
    log = $botsLog
    query_chr = (Count-Matches $botsLog '查询角色\.')
    select_chr = (Count-Matches $botsLog '选择角色 ')
    enter_game = (Count-Matches $botsLog '准备进入游戏')
    enter_game_ok = (Count-Matches $botsLog '成功进入游戏')
  }
  rust_login_srv = [ordered]@{
    log = $lgsOut
    login_ok = (Count-Matches $lgsOut '\[登录\] 帐号=')
    select_server = (Count-Matches $lgsOut '\[选服\] account=')
  }
  rust_db_srv = [ordered]@{
    log = $dbsOut
    query_chr = (Count-Matches $dbsOut '\[QueryChr\]')
    start_play = (Count-Matches $dbsOut '玩家获取游戏网关信息')
  }
  rust_sel_gate = [ordered]@{ log = $sgsOut; connects = (Count-Matches $sgsOut '开始连接:') }
  rust_login_gate = [ordered]@{ log = $lgOut; connects = (Count-Matches $lgOut '客户端连接') }
  panics = [ordered]@{
    login_gate = (Count-Matches $lgOut 'panicked')
    login_srv = (Count-Matches $lgsOut 'panicked')
    sel_gate = (Count-Matches $sgsOut 'panicked')
    db_srv = (Count-Matches $dbsOut 'panicked')
  }
}
$counts | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $OutDir 'rust-side-counts.json') -Encoding utf8
Write-Output ("COUNTS " + (Join-Path $OutDir 'rust-side-counts.json'))
Write-Output ('LOAD_RC=' + $lgRc)
exit $lgRc
