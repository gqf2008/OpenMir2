#requires -Version 5
<#
.SYNOPSIS
  run_m1_load.ps1 — M1⑤②③：对**全 Rust 边界链**跑一层假人（默认 200）登录+选角，
  取 BotSrv 侧计数与 Rust 服务侧日志/计数，并按 `tools/botload/load_report.py` 的同一口径归算内存曲线。

.DESCRIPTION
  - 假人编排**只读复用** `tools/botload/load_gate.ps1`（C6 线的件，本脚本不改它）；
    判据子集固定 `login,conn`（M1⑤ 只要求登录 + 选角，不要求 tick）⇒ 同时传 `-AllowMissingTick`
    （当前 `load_report.py` 的「缺 tick 样本」是**独立开关**，默认判 RED；`--criteria` 不能替代它）。
  - 内存曲线：`load_gate.ps1` 的 `MemProcNames` 是 C# 进程名（GameSrv/LoginSrv/…），拿不到 Rust 进程
    ⇒ 本脚本在 load 期间**另起一个采样作业**采 `login-gate/login-srv/sel-gate/db-srv` 的 WS/私存/CPU
    （schema 与 load_gate 一致：`t_ms/proc/pid/ws_mb/priv_mb/cpu_s`），再用 `load_report.py summarize`
    （**同一口径**）归算出 `report-rust.json`（含 `slope_tail_mb_per_s`）。
  - 两侧证据计数：**单一口径**在 `count_m1_logs.py`（本脚本调它，不重复实现 pattern）。
    入库件可用同一条命令复算（见 `count_m1_logs.py` 头部说明）。

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
  [switch]$SkipLoad   # 跳过起假人/采样，只重跑「两侧证据计数」（日志在活栈里被进程持有；
                      # count_m1_logs.py 用 Python 的共享读，可直接读正在写的 stdout）
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..')).Path
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$RustProcs = @('login-gate', 'login-srv', 'sel-gate', 'db-srv', 'mysqld')

$tierDir = Join-Path $OutDir ("tier-" + $Tier)
$statsPath = Join-Path $tierDir 'load_stats.ndjson'
$reportPath = Join-Path $OutDir 'report-rust.json'
$lgRc = $null

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
            '-RampTimeoutSec', "$RampTimeoutSec", '-AllowMissingTick')
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
  # `--criteria` 固定为 login,conn（M1⑤ 不判 tick）⇒ 必须显式 `--allow-missing-tick`，否则同一轮
  # 的 `load_gate.ps1`（已带 -AllowMissingTick）判 GREEN、而这里归算出的 `report-rust.json` 反判 RED
  # （口径自相矛盾；F-2c 返工时发现）。
  python (Join-Path $RepoRoot 'tools\botload\load_report.py') summarize --stats $statsPath --mem $memPath `
    --count $Tier --criteria 'login,conn' --allow-missing-tick --out $reportPath | Out-Null
  Write-Output ("REPORT_RUST " + $reportPath)
}
} # end -not $SkipLoad

# ---- 5) 两侧证据计数（单一口径：tools/m1-acceptance/count_m1_logs.py）----
# 口径与 pattern 全部在 count_m1_logs.py 里（输出 JSON 的 `patterns` 字段可机读）；本脚本不再自带一份，
# 避免「run 时一套、复算时又一套」的漂移（F-2 教训）。
$lgsOut = Join-Path $StackLogDir 'login-srv.out.log'
$dbsOut = Join-Path $StackLogDir 'db-srv.out.log'
$sgsOut = Join-Path $StackLogDir 'sel-gate.out.log'
$lgOut  = Join-Path $StackLogDir 'login-gate.out.log'
$botsLog = Join-Path $tierDir 'bots.log'
$countJson = Join-Path $OutDir 'rust-side-counts.json'
$countArgs = @((Join-Path $Here 'count_m1_logs.py'),
  '--bots', $botsLog, '--login-srv', $lgsOut, '--db-srv', $dbsOut,
  '--sel-gate', $sgsOut, '--login-gate', $lgOut,
  '--tier', "$Tier", '--hold-sec', "$HoldSec", '--criteria', 'login,conn',
  '--out', $countJson)
if ($null -ne $lgRc) { $countArgs += @('--load-rc', "$lgRc") }
python @countArgs
if ($LASTEXITCODE -ne 0) { throw "count_m1_logs.py rc=$LASTEXITCODE" }
Write-Output ('LOAD_RC=' + $(if ($null -eq $lgRc) { 'SKIPPED' } else { $lgRc }))
if ($null -eq $lgRc) { exit 0 } else { exit $lgRc }
