#requires -Version 5
<#
.SYNOPSIS
  run_exit_path.ps1 — M1③「退出路径」跨实现 DB 字段级 diff 的**单侧**采集夹具。

.DESCRIPTION
  卡 `openmir2-m1-dbsrv-6000-face`（owner openmir2-worker-17）的 ③ 剩余两条退出路径：
    softclose  客户端发 CM_SOFTCLOSE(1009)（= “小退”）⇒ 服务端 PlayObject.BoSoftClose
               → MakeGhost → SaveHumanRcd（存档）
    shutdown   服务端被优雅关闭（GameSvr 的 host 停机钩子 OnShutdown → SavePlayer）⇒
               对所有在世界里的玩家 SaveHumanRcd

  客户端 = 仓库自带 headless `src/BotSrv`（完整客户端协议栈；不在 oracle 七进程内，T-1）：
    * softclose：BotSrv 收到本假人 SM_LOGON 后等 `-SoftCloseAfterWorldMs`，按客户端
      `AppLogout()` 的线上形状发一条 `CM_SOFTCLOSE`（夹具白名单 M1-EXIT；默认 0=关）。
    * shutdown：BotSrv 进世界后**常驻**（不杀），由本夹具给 GameSrv 的控制台发 CTRL_BREAK
      触发 host 优雅停机（GameSvr 的 `Console.CancelKeyPress` ⇒ cts.Cancel ⇒
      IHostedLifecycleService.StoppingAsync ⇒ OnShutdown → SavePlayer）。

  判据口径（与 M3 对拍同一套）：`tools/dbsnap/dbsnap.py snap` + `scoped_diff.py extract/diff`
  作用域内字段级 diff = 0。两侧各用**独立全新前缀**（账号 = <Prefix><序号>），`extract --normalize`
  把前缀掩成 `<RUN>` 后逐列比（`--canonical` 才掩墙钟/自增列；本夹具只落原始抽取件，
  canonical 对拍由调用方 `compare_exit_paths.py` 做，避免把口径藏进采集步骤）。

  本脚本**不启动服务端栈**（照 m2-acceptance 口径：只对已跑的栈取判据）；起停栈由调用方
  `side_stack.ps1 -Dbsrv csharp|rust -Action start|stop` 负责。缺入口 → PREREQ（exit 2）。

.EXAMPLE
  powershell -File run_exit_path.ps1 -Side csharp -Path softclose -Prefix m1cs0214 -OutDir E:\tmp\..\csharp
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [ValidateSet('softclose', 'shutdown')][string]$Path = 'softclose',
  [string]$Prefix = '',
  [string]$OutDir = '',
  [string]$Address = '127.0.0.1',
  [int]$Port = 7000,
  [int]$SoftCloseAfterWorldMs = 8000,
  [int]$WaitInWorldSec = 150,
  [int]$SettleSec = 10,
  [int]$ShutdownWaitSec = 90,
  [ValidateSet('ctrlbreak', 'closewin')][string]$ShutdownVia = 'ctrlbreak',
  [switch]$NoNewAccount,   # 复用已存在的账号/角色（跨侧互读：同 prefix 两侧各跑一遍）
  [int]$ParkX = 291,       # -NoNewAccount 时：动作前把角色定点到安全坐标（消除随机战斗掉血噪声）
  [int]$ParkY = 666,
  [switch]$SkipBuild,
  [switch]$KeepBot
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..\..\..')).Path
$Dbsnap = Join-Path $RepoRoot 'tools\dbsnap\dbsnap.py'
$Scoped = Join-Path $RepoRoot 'mir2-rs\tests\parity\m3\scoped_diff.py'
$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
$BotProj = Join-Path $RepoRoot 'src\BotSrv\BotSrv.csproj'
$BotBin = Join-Path $RepoRoot 'src\BotSrv\bin\Release'
$CtrlShim = Join-Path $Here 'send_ctrlc.ps1'

if ([string]::IsNullOrEmpty($Prefix)) { throw 'RED: 必须给 -Prefix（账号 = <Prefix><序号>，<=10 字符）' }
if ([string]::IsNullOrEmpty($OutDir)) { throw 'RED: 必须给 -OutDir' }
if (($Prefix.Length + 1) -gt 10) { throw ("RED: 账号名最长 {0} 字符 > 10（协议层静默截断）" -f ($Prefix.Length + 1)) }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

function Fail-Prereq([string]$msg) { Write-Output ('PREREQ_MISSING: ' + $msg); exit 2 }

# ---- 前置 ----
if (-not (Get-Command python -ErrorAction SilentlyContinue)) { Fail-Prereq '找不到 python' }
if (-not (Test-Path $MysqlExe)) { Fail-Prereq ('找不到 mysql 客户端 ' + $MysqlExe) }
if (-not (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)) {
  Fail-Prereq ("客户端入口 {0}:{1} 未监听（先 side_stack.ps1 -Action start）" -f $Address, $Port)
}
try {
  $c = New-Object System.Net.Sockets.TcpClient
  $iar = $c.BeginConnect('127.0.0.1', 3306, $null, $null)
  if (-not $iar.AsyncWaitHandle.WaitOne(1500)) { throw 'timeout' }
  $c.Close()
} catch { Fail-Prereq 'MySQL 3306 不可达' }

if (-not $SkipBuild -or -not (Test-Path (Join-Path $BotBin 'BotSrv.exe'))) {
  Write-Output 'BUILD BotSrv...'
  $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
  dotnet build $BotProj -c Release --nologo -v q | Out-Null
  $rc = $LASTEXITCODE; $ErrorActionPreference = $prev
  if ($rc -ne 0) { Fail-Prereq ('BotSrv 构建失败 rc=' + $rc) }
}

# ---- 运行态归属（基线必须写清是哪几个二进制产出的） ----
$runtimeProcs = @()
foreach ($n in @('mysqld', 'DBSrv', 'db-srv', 'LoginSrv', 'GameSrv', 'LoginGate', 'SelGate', 'GameGate')) {
  foreach ($proc in @(Get-Process -Name $n -ErrorAction SilentlyContinue)) {
    $runtimeProcs += [pscustomobject]@{ name = $proc.ProcessName; path = $proc.Path; pid = $proc.Id; start = $proc.StartTime.ToString('o') }
  }
}
foreach ($rp in $runtimeProcs) { Write-Output ('RUNTIME {0} pid={1} <- {2}' -f $rp.name, $rp.pid, $rp.path) }

# ---- BotSrv 工作目录（每次独立；写 AppSetting.json + load_out.conf） ----
$botDir = Join-Path $OutDir ('bot-' + $Path)
if (Test-Path $botDir) { Remove-Item -Recurse -Force $botDir }
Copy-Item -Recurse -Force $BotBin $botDir
$cfg = @{ BotPlay = @{
    ServerName = '热血传奇'; Address = $Address; Port = $Port
    ChrCount = 1; TotalChrCount = 1; NewAccount = $(-not $NoNewAccount); ConnectStaggerMs = 200
    LoginAccount = $Prefix
    FixedNewChr = $true; NewChrJob = 1; NewChrHair = 2; NewChrSex = 0
    SoftCloseAfterWorldMs = $(if ($Path -eq 'softclose') { $SoftCloseAfterWorldMs } else { 0 })
    DisableAutoPlay = $true   # 挂机走位不可复现（客户端无种子 RNG）⇒ 对拍必须静止；见 ClientManager.RunAutoPlay
} } | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText((Join-Path $botDir 'AppSetting.json'), $cfg, (New-Object System.Text.UTF8Encoding($false)))
[System.IO.File]::WriteAllText((Join-Path $botDir 'load_out.conf'), $OutDir, (New-Object System.Text.UTF8Encoding($false)))
$env:MIR2_BOT_OUT = $OutDir
$statsPath = Join-Path $OutDir 'load_stats.ndjson'
$botLog = Join-Path $OutDir ('bot-' + $Path + '.out.log')
$botErr = Join-Path $OutDir ('bot-' + $Path + '.err.log')
Remove-Item -Force $statsPath -ErrorAction SilentlyContinue
Remove-Item -Force $botLog, $botErr -ErrorAction SilentlyContinue

# ---- 定点（仅复用已有角色时）：消除“随机战斗掉血”这个夹具噪声 ----
# 依据：同一 C# GameSvr 下，角色出生点是 GetRandHomeX/Y 随机的；地图上随机挨打会让存档 Hp
# 在 16/15 之间跳（实测）。Rust db-srv 自身对 Hp 的取/存是忠实的（diag 证据见 README），
# 差异只是夹具噪声 ⇒ 把角色定点到比奇省安全区（291,666），两侧同起点、无战斗。
if ($NoNewAccount) {
  $sql = "UPDATE mir2_db.characters SET MapName='0', CX=$ParkX, CY=$ParkY WHERE ChrName LIKE '$Prefix%';"
  $sql += " UPDATE mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id SET a.HP=a.MaxHP, a.MP=a.MaxMP WHERE c.ChrName LIKE '$Prefix%';"
  & $MysqlExe -h 127.0.0.1 -P 3306 -u root --batch -e $sql | Out-Null
  if ($LASTEXITCODE -ne 0) { throw 'RED: 定点 SQL 失败' }
  Write-Output ("PARKED prefix={0} -> ({1},{2}) HP=MaxHP MP=MaxMP" -f $Prefix, $ParkX, $ParkY)
}

function Read-SharedText([string]$p) {
  if (-not (Test-Path $p)) { return '' }
  try {
    $fs = [System.IO.File]::Open($p, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try { $sr = New-Object System.IO.StreamReader($fs); return $sr.ReadToEnd() } finally { $fs.Close() }
  } catch { return '' }
}
function Get-LastStats([string]$p) {
  $txt = Read-SharedText $p
  if ($txt -eq '') { return $null }
  $lines = $txt -split "`n" | Where-Object { $_.Trim() -ne '' }
  if ($lines.Count -eq 0) { return $null }
  try { return ($lines[-1] | ConvertFrom-Json) } catch { return $null }
}

# ---- BEFORE 快照 ----
$snapBefore = Join-Path $OutDir ('snap-' + $Path + '-before')
python $Dbsnap snap --out $snapBefore | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'RED: dbsnap before 失败' }
$extractBefore = Join-Path $OutDir ($Path + '-before.jsonl')
python $Scoped extract $snapBefore $Prefix $extractBefore --normalize --allow-empty | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'RED: scoped extract before 失败' }

# ---- 起 BotSrv ----
$proc = Start-Process -FilePath (Join-Path $botDir 'BotSrv.exe') -WorkingDirectory $botDir -PassThru -WindowStyle Hidden `
  -RedirectStandardOutput $botLog -RedirectStandardError $botErr
Write-Output ('BOTSRV_PID=' + $proc.Id + ' LOG=' + $botLog)

# ---- 等进世界 ----
$deadline = (Get-Date).AddSeconds($WaitInWorldSec)
$inWorld = 0
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Seconds 3
  $st = Get-LastStats $statsPath
  if ($st -ne $null -and ($st.PSObject.Properties.Name -contains 'in_world')) { $inWorld = [int]$st.in_world }
  if ($inWorld -ge 1) { break }
  if ($proc.HasExited) { break }
}
if ($inWorld -lt 1) {
  Write-Output ('RED: BotSrv 未在 {0}s 内进世界（in_world={1}）' -f $WaitInWorldSec, $inWorld)
  Write-Output ((Read-SharedText $botLog) -split "`n" | Select-Object -Last 8)
  if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
  exit 1
}
Write-Output ('BOTSRV_IN_WORLD in_world=' + $inWorld)

# ---- 执行退出动作 ----
$actionEvidence = ''
if ($Path -eq 'softclose') {
  $mDeadline = (Get-Date).AddSeconds($ShutdownWaitSec)
  $sent = $false
  while ((Get-Date) -lt $mDeadline) {
    if ((Read-SharedText $botLog) -match 'M1SOFTCLOSE sent') { $sent = $true; break }
    if ($proc.HasExited) { break }
    Start-Sleep -Milliseconds 500
  }
  if (-not $sent) { Write-Output 'RED: 未看到 M1SOFTCLOSE sent 标记'; Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue; exit 1 }
  Write-Output 'M1SOFTCLOSE sent ident=1009'
  $actionEvidence = 'M1SOFTCLOSE sent ident=1009（BotSrv 日志）'
  Start-Sleep -Seconds $SettleSec
} else {
  # shutdown：BotSrv 常驻世界；给 GameSrv 控制台发 CTRL_BREAK（优雅停机）
  $gs = Get-Process -Name 'GameSrv' -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $gs) { Write-Output 'RED: 找不到 GameSrv 进程'; Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue; exit 1 }
  $gsPid = $gs.Id
  Write-Output ('SHUTDOWN target GameSrv pid=' + $gsPid + ' via=' + $ShutdownVia)
  if ($ShutdownVia -eq 'closewin') {
    $ok = $gs.CloseMainWindow()
    Write-Output ('CLOSEMAINWINDOW ok=' + $ok)
  } else {
    & powershell -NoProfile -ExecutionPolicy Bypass -File $CtrlShim -TargetPid $gsPid -Event 1 2>&1 | ForEach-Object { Write-Output $_ }
  }
  $gDeadline = (Get-Date).AddSeconds($ShutdownWaitSec)
  while ((Get-Date) -lt $gDeadline) {
    if (-not (Get-Process -Id $gsPid -ErrorAction SilentlyContinue)) { break }
    Start-Sleep -Milliseconds 500
  }
  if (Get-Process -Id $gsPid -ErrorAction SilentlyContinue) { Write-Output ('RED: GameSrv pid={0} 在 {1}s 内未退出' -f $gsPid, $ShutdownWaitSec); Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue; exit 1 }
  Write-Output ('SHUTDOWN GameSrv pid={0} 优雅退出' -f $gsPid)
  $actionEvidence = ('GameSvr pid={0} 收到 CTRL_BREAK 优雅退出（OnShutdown → SavePlayer）' -f $gsPid)
  Start-Sleep -Seconds 3
}

# ---- 收 BotSrv ----
if (-not $KeepBot -and $proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
Start-Sleep -Seconds 2

# ---- AFTER 快照 ----
$snapAfter = Join-Path $OutDir ('snap-' + $Path + '-after')
python $Dbsnap snap --out $snapAfter | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'RED: dbsnap after 失败' }
$extractAfter = Join-Path $OutDir ($Path + '-after.jsonl')
python $Scoped extract $snapAfter $Prefix $extractAfter --normalize | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'RED: scoped extract after 失败' }

$afterLines = 0
if (Test-Path $extractAfter) { $afterLines = (Get-Content $extractAfter | Measure-Object -Line).Lines }
$beforeLines = 0
if (Test-Path $extractBefore) { $beforeLines = (Get-Content $extractBefore | Measure-Object -Line).Lines }

# 落 baseline.json（原始快照目录瘦身：只留 manifest）
foreach ($d in @($snapBefore, $snapAfter)) {
  Copy-Item (Join-Path $d 'manifest.json') (Join-Path (Split-Path $d -Parent) ((Split-Path $d -Leaf) + '.manifest.json')) -Force
  Remove-Item -Recurse -Force $d -ErrorAction SilentlyContinue
}

$baseline = [pscustomobject]@{
  schema        = 'm1-exit-path/1'
  side          = $Side
  path          = $Path
  prefix        = $Prefix
  reuse_account = [bool]$NoNewAccount
  entry         = "$Address`:$Port"
  fixed_new_chr = 'Job=1 Hair=2 Sex=0'
  softclose_after_world_ms = $(if ($Path -eq 'softclose') { $SoftCloseAfterWorldMs } else { 0 })
  action_evidence = $actionEvidence
  scoped_rows   = @{ before = $beforeLines; after = $afterLines }
  runtime_procs = $runtimeProcs
  bot_stats     = (Get-LastStats $statsPath)
  finished_at   = (Get-Date).ToString('o')
}
$baseline | ConvertTo-Json -Depth 8 | Set-Content -Path (Join-Path $OutDir 'baseline.json') -Encoding utf8
Write-Output ('BASELINE_OK ' + (Join-Path $OutDir 'baseline.json'))
Write-Output ('SCOPED_ROWS before={0} after={1}' -f $beforeLines, $afterLines)
exit 0
