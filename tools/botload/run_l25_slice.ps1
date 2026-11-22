# L2-5 ③ 段 4：容量切片冒烟（2 区 1k）+ 1 区基线（RSS 伸缩）——编排骨架
#
# 卡：`openmir2-l25-s3-cross-recipient-count` 段 4。总监 2026-11-22 三条硬要求：
#   1. 摘要必须**按列名**读 perf TSV 的 7 列，`cross_recipient_missing` 合计打印在**首屏**
#      （由 tools/botload/perf_tsv_summary.py 承担，本脚本只负责把产物喂给它）；
#   2. 2 区 1k 冒烟落 TSV/JSON 产物，摘要同时给：有效 tick 率、p50、`region_barrier_us`
#      max/total、`cross_recipient_missing` 六面分解、RSS；
#   3. 额外跑一次 **1 区基线**（同机同时长），给 `RSS(2区) − RSS(1区)` 与每区粗估
#      （`-BaselineJson` 指向 1 区摘要时由 perf_tsv_summary.py --compare 计算）。
#
# 用法（栈由本脚本起停；先跑 2 区，再跑 1 区基线，最后对比）：
#   pwsh -File tools/botload/run_l25_slice.ps1 -Zones 2 -Bots 1000 -HoldSec 120 `
#     -Layout "0:0=0,0-99,199;1:0=100,0-199,199" -ServerExe <release mir2-server.exe> `
#     -OutDir tests/loadgate/l25-slice-2z
#   pwsh -File tools/botload/run_l25_slice.ps1 -Zones 1 -Bots 1000 -HoldSec 120 `
#     -ServerExe <release mir2-server.exe> -OutDir tests/loadgate/l25-slice-1z
#   python -I -B tools/botload/perf_tsv_summary.py --compare `
#     tests/loadgate/l25-slice-2z/summary.json tests/loadgate/l25-slice-1z/summary.json `
#     --labels 2zone 1zone
#
# 骨架状态（2026-11-22）：环境注入 / 起停栈 / 产物喂摘要 / 失败面都已写死；真机运行前需按
# 当次栈目录核对的两处用 `TODO(stack)` 标出（端口 profile、账号前缀）。
# 渲染纪律：一切失败走 rc≠0 + `RED:`/`PREREQ:` 前缀，不静默。

param(
  [ValidateSet(1, 2)][int]$Zones = 2,
  [int]$Bots = 1000,
  [int]$HoldSec = 120,
  # `MIR2_ZONES` 语法（见 docs/issues/l2-4-multi-zone-plan-20261122.md §2；同一张图要么不提、
  # 要么列全且覆盖 100%/不重叠）。切片=1 时**必填**；1 区基线不传。
  [string]$Layout = "",
  [string]$Root = "E:\MirServer",
  [string]$ServerExe = "",
  [string]$RepoMir2Rs = "",
  # **产物一律仓外**（总监 2026-11-22）：压测产物不进仓库工作树（避免被 `git add -A` 收进历史、
  # 也避免污染 `git status`）。根目录可配置；实际目录 = <OutRoot>/<1z|2z>-<时间戳>。
  [string]$OutRoot = "",
  [string]$OutDir = "",
  [string]$AccountPrefix = "loadbot",   # TODO(stack)：与现网账号前缀核对
  [int]$MemSampleSec = 2,
  [int]$WaitSeconds = 60,
  [switch]$ProvisionAccounts,
  [switch]$SkipBuild,
  [switch]$KeepStack,
  [switch]$WhatIf
)

$ErrorActionPreference = 'Stop'

function Fail([string]$msg) { Write-Output ("RED: " + $msg); exit 1 }
function Prereq([string]$msg) { Write-Output ("PREREQ: " + $msg); exit 2 }

# 用 Start-Process + 重定向文件跑一个 pwsh 子进程，只等**直接子进程**退出。
# 不能用 `& pwsh ... 2>&1`：PowerShell 的捕获会等**孙进程**（常驻角色）退出才返回
#（实测 8s probe：脚本立刻打印 DONE，捕获却等到孙进程睡醒）⇒ 起栈永远不返回。
function Invoke-PwshToLog {
  param([string[]]$PwshArgs, [string]$LogPath, [string]$ErrPath)
  $sp = Start-Process -FilePath 'pwsh' -ArgumentList $PwshArgs `
    -RedirectStandardOutput $LogPath -RedirectStandardError $ErrPath -WindowStyle Hidden -PassThru
  $sp.WaitForExit()
  return [int]$sp.ExitCode
}

if (-not $OutRoot) {
  # 默认仓外（Windows 下 TEMP 通常是 E:\UserTemp；显式 -OutRoot 可覆盖）。
  $OutRoot = Join-Path ([System.IO.Path]::GetTempPath()) "mir2-l25-slice"
}
if (-not $OutDir) {
  $OutDir = Join-Path $OutRoot ($Zones.ToString() + "z-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$PerfTsv = Join-Path $OutDir "perf_counters.tsv"
$RssNdjson = Join-Path $OutDir "rss.ndjson"
$RunJson = Join-Path $OutDir "run.json"
$SummaryJson = Join-Path $OutDir "summary.json"
$TierDir = Join-Path $OutDir ("tier-" + $Bots)

# ---- 0) 前置（fail-closed） ----
if ($Zones -eq 2 -and [string]::IsNullOrWhiteSpace($Layout)) {
  Prereq "-Zones 2 必须给 -Layout（MIR2_ZONES 语法，例 '0:0=0,0-99,199;1:0=100,0-199,199'）"
}
if ($Zones -eq 2 -and $Layout -notmatch ';') {
  Prereq "-Layout 看不出 >=2 区（需 ';' 分隔且每区非空）：$Layout"
}
if ([string]::IsNullOrWhiteSpace($ServerExe)) {
  Prereq "-ServerExe 必填（release 的 mir2-server.exe；段 4 口径只跑 Rust 栈）"
}
if (-not (Test-Path -LiteralPath $ServerExe)) { Prereq "-ServerExe 不存在：$ServerExe" }

if (-not $RepoMir2Rs) {
  $RepoMir2Rs = Join-Path (Split-Path -Parent $PSScriptRoot) "..\..\mir2-rs"
}
$Summarizer = Join-Path $PSScriptRoot "perf_tsv_summary.py"
if (-not (Test-Path -LiteralPath $Summarizer)) { Prereq "缺摘要工具：$Summarizer" }

$py = Get-Command python -ErrorAction SilentlyContinue
if (-not $py) { Prereq "PATH 里没有 python（摘要工具需要）" }

# 两区必须显式带镜像（plan_slicing 的启动门：slicing=1 要求 MIR2_ZONE_MIRROR=1）。
$childEnv = @{ 'MIR2_PERF_COUNTERS' = $PerfTsv }
if ($Zones -eq 2) {
  $childEnv['MIR2_ZONES'] = $Layout
  $childEnv['MIR2_ZONE_MIRROR'] = '1'
  $childEnv['MIR2_ZONE_SLICING'] = '1'
} else {
  # 1 区基线 = 原单线程路径：三个开关一律不设（N=1 逐字节不变；六面计数也必须全 0）。
  $childEnv.Remove('MIR2_ZONES'); $childEnv.Remove('MIR2_ZONE_MIRROR'); $childEnv.Remove('MIR2_ZONE_SLICING')
}

Write-Output ("PLAN zones={0} bots={1} hold={2}s layout='{3}' out={4}" -f $Zones, $Bots, $HoldSec, $Layout, $OutDir)
Write-Output ("ENV  " + (($childEnv.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ' '))
if ($WhatIf) { Write-Output "WHATIF: 只打印计划，不起栈、不压测"; exit 0 }

# ---- 1) 构建（release；段 4 口径） ----
if (-not $SkipBuild) {
  Push-Location $RepoMir2Rs
  try {
    $env:CARGO_BUILD_JOBS = '2'
    cargo build --release -p mir2-server
    if ($LASTEXITCODE -ne 0) { Fail "cargo build --release -p mir2-server rc=$LASTEXITCODE" }
  } finally { Pop-Location }
}

# ---- 2) 账号（幂等；可按需跳过） ----
if ($ProvisionAccounts) {
  $prov = Join-Path (Split-Path -Parent $PSScriptRoot) "client\account_provision.ps1"
  if (-not (Test-Path -LiteralPath $prov)) { Prereq "缺开号脚本：$prov" }
  & pwsh -NoProfile -File $prov -Prefix $AccountPrefix -Count $Bots
  if ($LASTEXITCODE -ne 0) { Fail "account_provision rc=$LASTEXITCODE（prefix=$AccountPrefix count=$Bots）" }
}

# ---- 3) 起栈（环境经进程继承进 game-svr：见 start_mir2_server.ps1 的 Start-ChildProcess） ----
$Starter = Join-Path (Split-Path -Parent $PSScriptRoot) "m2-acceptance\start_mir2_server.ps1"
$Stopper = Join-Path (Split-Path -Parent $PSScriptRoot) "m2-acceptance\stop_mir2_server.ps1"
if (-not (Test-Path -LiteralPath $Starter)) { Prereq "缺起栈脚本：$Starter" }
if (-not (Test-Path -LiteralPath $Stopper)) { Prereq "缺停栈脚本：$Stopper" }

$prev = @{}
foreach ($k in $childEnv.Keys) {
  $prev[$k] = if (Test-Path -LiteralPath ("Env:" + $k)) { (Get-Item -LiteralPath ("Env:" + $k)).Value } else { $null }
  Set-Item -Path ("Env:" + $k) -Value ([string]$childEnv[$k])
}
$pidFile = $null
$rssProc = $null
try {
  $startLog = Join-Path $OutDir "stack-start.log"
  $startErr = Join-Path $OutDir "stack-start.err.log"
  $startRc = Invoke-PwshToLog -PwshArgs @('-NoProfile','-File',$Starter,'-Root',$Root,'-ServerExe',$ServerExe,'-WaitSeconds',([string]$WaitSeconds)) -LogPath $startLog -ErrPath $startErr
  Get-Content -LiteralPath $startLog -ErrorAction SilentlyContinue | ForEach-Object { Write-Output $_ }
  if ($startRc -ne 0) { Fail "start_mir2_server rc=$startRc（栈未就绪；见 $startLog / $startErr）" }
  $okLine = @(Get-Content -LiteralPath $startLog -ErrorAction SilentlyContinue | Where-Object { "$_" -match '^MIR2_SERVER_STACK_OK\s+(.+)$' })[-1]
  if (-not $okLine) { Fail "起栈输出缺 MIR2_SERVER_STACK_OK（fail-closed）" }
  $pidFile = ([regex]'^MIR2_SERVER_STACK_OK\s+(.+)$').Match("$okLine").Groups[1].Value.Trim()
  if (-not (Test-Path -LiteralPath $pidFile)) { Fail "pid 文件不存在：$pidFile" }
  Write-Output ("STACK pidfile=" + $pidFile)
} finally {
  foreach ($k in $prev.Keys) {
    if ($null -eq $prev[$k]) { Remove-Item -Path ("Env:" + $k) -ErrorAction SilentlyContinue }
    else { Set-Item -Path ("Env:" + $k) -Value ([string]$prev[$k]) }
  }
}

# ---- 4) RSS 采样（独立进程；按 pid 文件里的角色逐个采） ----
$stopFile = Join-Path $OutDir "rss.stop"
Remove-Item -LiteralPath $stopFile -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $RssNdjson -ErrorAction SilentlyContinue
$sampler = Join-Path $PSScriptRoot "rss_sampler.ps1"
if (-not (Test-Path -LiteralPath $sampler)) { Prereq "缺 RSS 采样器：$sampler" }
$rssProc = Start-Process -FilePath 'pwsh' -ArgumentList @('-NoProfile','-File',$sampler,'-PidFile',$pidFile,'-Out',$RssNdjson,'-StopFile',$stopFile,'-EverySec',([string]$MemSampleSec)) -WindowStyle Hidden -PassThru

# ---- 5) 压测档（load_gate；栈已在跑） ----
$Gate = Join-Path $PSScriptRoot "load_gate.ps1"
if (-not (Test-Path -LiteralPath $Gate)) { Prereq "缺 load_gate：$Gate" }
$runStartMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
try {
  $gateLog = Join-Path $OutDir "load_gate.log"
  $gateErr = Join-Path $OutDir "load_gate.err.log"
  $gateRc = Invoke-PwshToLog -PwshArgs @('-NoProfile','-File',$Gate,'-Tiers',([string]$Bots),'-HoldSec',([string]$HoldSec),'-OutDir',$OutDir,'-SkipBuild') -LogPath $gateLog -ErrPath $gateErr
  Get-Content -LiteralPath $gateLog -ErrorAction SilentlyContinue | Select-Object -Last 40 | ForEach-Object { Write-Output $_ }
} finally {
  $runEndMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
  New-Item -ItemType File -Force -Path $stopFile | Out-Null
  if ($null -ne $rssProc) {
    $null = $rssProc.WaitForExit([math]::Max(15000, $MemSampleSec * 3000))
    if (-not $rssProc.HasExited) { Stop-Process -Id $rssProc.Id -Force -ErrorAction SilentlyContinue }
  }
  if (-not $KeepStack) {
    $stopLog = Join-Path $OutDir "stack-stop.log"
    $stopRc = Invoke-PwshToLog -PwshArgs @('-NoProfile','-File',$Stopper,'-Root',$Root) -LogPath $stopLog -ErrPath (Join-Path $OutDir "stack-stop.err.log")
    Get-Content -LiteralPath $stopLog -ErrorAction SilentlyContinue | ForEach-Object { Write-Output $_ }
    if ($stopRc -ne 0) { Write-Output ("RED: stop_mir2_server rc=" + $stopRc) }
  }
}

# **RED 轮也要出摘要/产物**（缺口基线必须在红轮上可读）：先继续做产物与摘要，最后再以 rc=1 退出。
$gateFailed = $gateRc -ne 0
if ($gateFailed) { Write-Output ("RED: load_gate rc=$gateRc（档位未达标；见 $TierDir 的 report.json）——仍继续出摘要") }

# ---- 6) 产物完整性（fail-closed） ----
if (-not (Test-Path -LiteralPath $PerfTsv)) { Prereq "perf TSV 不存在（MIR2_PERF_COUNTERS 未进 game-svr？）：$PerfTsv" }
$reportJson = Join-Path $TierDir "report.json"
if (-not (Test-Path -LiteralPath $reportJson)) { Prereq "缺 load_gate report：$reportJson" }
if (-not (Test-Path -LiteralPath $RssNdjson)) { Prereq "缺 RSS 采样：$RssNdjson" }

@{
  zones = $Zones
  bots = $Bots
  hold_sec = $HoldSec
  layout = $Layout
  server_exe = $ServerExe
  perf_tsv = $PerfTsv
  pid_file = $pidFile
  start_wall_ms = $runStartMs
  end_wall_ms = $runEndMs
  # **本次用哪个开关驱动走位**（总监要求写进产物；空 = 未驱动走位）。
  bot_boundary_walk = [pscustomobject]@{
    enabled = [string]$env:MIR2_BOT_BOUNDARY_WALK
    x0 = [string]$env:MIR2_BOT_BOUNDARY_WALK_X0
    x1 = [string]$env:MIR2_BOT_BOUNDARY_WALK_X1
    step_ms = [string]$env:MIR2_BOT_BOUNDARY_WALK_STEP_MS
  }
} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $RunJson -Encoding utf8

# ---- 7) 摘要（按列名读 7 列；合计在首屏） ----
& python -I -B $Summarizer --perf $PerfTsv --run-json $RunJson --report $reportJson --rss $RssNdjson `
  --bots $Bots --zones $Zones --label ("{0}zone" -f $Zones) --json $SummaryJson
if ($LASTEXITCODE -ne 0) { Prereq "perf_tsv_summary rc=$LASTEXITCODE（输入不可判；见 stderr 的 PREREQ 行）" }

# ---- 8) 若有 1 区基线摘要，直接给内存伸缩 ----
# 基线 = <OutRoot> 下最新的 1z-*/summary.json（与本次无关的旧目录不会误用：只取最新且必须存在）。
$baseline = $null
if ($Zones -eq 2) {
  $baseDir = Get-ChildItem -Path $OutRoot -Directory -Filter "1z-*" -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -First 1
  if ($baseDir) {
    $candidate = Join-Path $baseDir.FullName "summary.json"
    if (Test-Path -LiteralPath $candidate) { $baseline = $candidate }
  }
}
if ($Zones -eq 2 -and $baseline -and (Test-Path -LiteralPath $baseline)) {
  & python -I -B $Summarizer --compare $SummaryJson $baseline --labels 2zone 1zone `
    --json (Join-Path $OutDir "rss_delta.json")
  if ($LASTEXITCODE -ne 0) { Prereq "RSS 对比 rc=$LASTEXITCODE（基线摘要不完整）" }
} elseif ($Zones -eq 2) {
  Write-Output ("NOTE: 未找到 1 区基线摘要（" + $baseline + "）——按段 4 硬要求 3，跑一次 -Zones 1 再 --compare")
}

if ($gateFailed) { Write-Output ("SLICE_SMOKE_OUT=" + $OutDir + "（load_gate RED，摘要已出）"); exit 1 }
Write-Output ("SLICE_SMOKE_OK out=" + $OutDir)
exit 0
