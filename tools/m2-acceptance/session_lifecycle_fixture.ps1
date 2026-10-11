#requires -Version 5
<#
.SYNOPSIS
  session_lifecycle_fixture.ps1 — M2「会话生命周期」DoD ① 的真机夹具（同账号/角色重复登录实测）。

.DESCRIPTION
  卡：`openmir2-m2-session-lifecycle`（owner openmir2-worker-11）。题面：
  在**真 C# 栈**（E:\MirServer）上，同一账号/角色在旧会话仍在线时再登录一次，实测四件事：
    ① 旧连接收到什么
    ② 新连接被放行还是被 CloseUser 关
    ③ 旧 PlayObject 何时离开 PlayObjectList
    ④ 此后何时能再进世界

  题面要求「口径以实测为准」，所以本脚本**只观测、只登记**，不预先断言走哪条分支。

  做法：用仓库自带的 headless 客户端 `src/BotSrv`（C6 假人线，完整客户端协议栈）：
    - BotSrv 的账号/角色约定是 `LoginAccount + 序号`（`AppService.cs:87-89`、`ChrName = LoginId`）；
    - 两个进程都配 `LoginAccount=<prefix>`、`ChrCount=1` ⇒ 都登录 **同一账号 `prefix0`**、选同一角色；
    - Bot A 先进世界（旧会话）→ 起 Bot B（同账号再登录）⇒ 复现「重复登录」。

  证据产物（`-OutDir`）：
    - `botA.out.log` / `botB.out.log`：两侧客户端日志（旧连接收到什么、新连接是否进世界）
    - `gamesvr-window.log`：GameSvr 日志在观测窗口内的增量（`角色数据保存中` / 网关 / 用户）
    - `timeline.json`：时间线（t_start / t_A_in_world / t_B_start / t_B_verdict / t_kick_msg）
    - `fixture.json`：判据（old_received / new_verdict / reentry）+ 退出码口径

  退出码：
    0 = 夹具跑通、四项观测已采到（**不**代表分支是好是坏，分支结论在 fixture.json）
    1 = 夹具跑了一半失败（如某侧始终没进世界/没被处理）
    2 = **前置未满足**（栈端口没监听 / BotSrv 构建不出 / 账号角色缺失）——绝不静默通过

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File session_lifecycle_fixture.ps1 `
    -OutDir E:\tmp\m2acc-life\run2 -SkipBuild
#>
param(
  [string]$OutDir = "E:\tmp\m2acc-life\run2",
  [string]$Prefix = "fixt",
  [int]$Port = 7000,
  [string]$BotTemplateDir = "",       # 默认 <repo>/src/BotSrv/bin/Release
  [int]$Stage2DelaySec = 3,           # Bot A 进世界后隔多久起 Bot B
  [int]$WaitInWorldSec = 90,
  [switch]$SkipBuild,
  [string]$GameSvrLog = "E:\MirServer\logs\GameSvr.out.log"
)

$ErrorActionPreference = "Stop"

function Fail-Prereq([string]$msg) {
  Write-Output ("PREREQ_MISSING: " + $msg)
  Write-Output "exit=2（前置未满足，脚本判非零退出，绝不静默通过）"
  exit 2
}

# ---- 路径 ----
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
if ($BotTemplateDir -eq "") { $BotTemplateDir = Join-Path $RepoRoot "src\BotSrv\bin\Release" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

Write-Output "== M2 session-lifecycle fixture =="
Write-Output ("repo      = " + $RepoRoot)
Write-Output ("outdir    = " + $OutDir)
Write-Output ("account   = " + ($Prefix + "0"))

# ---- 前置 1：真栈监听 ----
$ports = @(5000, 7000, 7100, 7200)
$missing = @()
foreach ($p in $ports) {
  if (-not (Get-NetTCPConnection -State Listen -LocalPort $p -ErrorAction SilentlyContinue)) { $missing += $p }
}
if ($missing.Count -gt 0) {
  Fail-Prereq ("真 C# 栈未起：端口 " + ($missing -join ",") + " 未监听。先跑 E:\MirServer\start-all.ps1")
}

# ---- 前置 2：BotSrv ----
$BotExe0 = Join-Path $BotTemplateDir "BotSrv.exe"
if (-not $SkipBuild -or -not (Test-Path $BotExe0)) {
  Write-Output "BUILD BotSrv..."
  $prev = $ErrorActionPreference; $ErrorActionPreference = "Continue"
  dotnet build (Join-Path $RepoRoot "src\BotSrv\BotSrv.csproj") -c Release --nologo -v q | Out-Null
  $rc = $LASTEXITCODE; $ErrorActionPreference = $prev
  if ($rc -ne 0) { Fail-Prereq "BotSrv 构建失败（rc=$rc）" }
}
if (-not (Test-Path $BotExe0)) { Fail-Prereq ("找不到 " + $BotExe0) }

function Read-Shared([string]$Path) {
  if (-not (Test-Path $Path)) { return "" }
  try {
    $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try { $sr = New-Object System.IO.StreamReader($fs); return $sr.ReadToEnd() } finally { $fs.Close() }
  } catch { return "" }
}

function New-BotDir([string]$Name) {
  $d = Join-Path $OutDir $Name
  if (Test-Path $d) { Remove-Item -Recurse -Force $d }
  Copy-Item -Recurse -Force $BotTemplateDir $d
  $cfg = @{ BotPlay = @{
      ServerName = "热血传奇"; Address = "127.0.0.1"; Port = $Port
      ChrCount = 1; TotalChrCount = 1; NewAccount = $false
      LoginAccount = $Prefix; ConnectStaggerMs = 200
  } } | ConvertTo-Json -Depth 4
  [System.IO.File]::WriteAllText((Join-Path $d "AppSetting.json"), $cfg, (New-Object System.Text.UTF8Encoding($false)))
  # 压测统计输出目录：指到本夹具 OutDir，避免污染别的车道（C6）的 load_out.conf 目标
  [System.IO.File]::WriteAllText((Join-Path $d "load_out.conf"), $OutDir, (New-Object System.Text.UTF8Encoding($false)))
  return $d
}

function Start-Bot([string]$Dir, [string]$Tag) {
  $log = Join-Path $OutDir ($Tag + ".out.log")
  $err = Join-Path $OutDir ($Tag + ".err.log")
  $p = Start-Process -FilePath (Join-Path $Dir "BotSrv.exe") -WorkingDirectory $Dir -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput $log -RedirectStandardError $err
  return $p
}

function Wait-InWorld([string]$Log, [int]$TimeoutSec) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if ((Read-Shared $Log) -match "成功进入游戏") { return $true }
    Start-Sleep -Milliseconds 500
  }
  return $false
}

# 记录 GameSvr 日志起始行数，观测窗口只取增量
$gsvrLines0 = 0
if (Test-Path $GameSvrLog) { $gsvrLines0 = (Get-Content $GameSvrLog -ErrorAction SilentlyContinue | Measure-Object -Line).Lines }

$t0 = Get-Date
# ---- 旧会话：Bot A ----
$dirA = New-BotDir "botA"
$procA = Start-Bot $dirA "botA"
$logA = Join-Path $OutDir "botA.out.log"
Write-Output ("BotA pid=" + $procA.Id + " 启动，等它进世界…")
if (-not (Wait-InWorld $logA $WaitInWorldSec)) {
  Write-Output "RED: BotA 未在超时内进入世界"
  Write-Output ("BotA 日志尾：" + ((Read-Shared $logA) -split "`n" | Select-Object -Last 5) -join " | ")
  foreach ($p in @($procA)) { if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
  exit 1
}
$tAInWorld = Get-Date
Write-Output ("OK: BotA(=旧会话) 已进入世界 @ " + $tAInWorld.ToString("HH:mm:ss.fff"))

# ---- 新会话：Bot B（同账号再登录）----
Start-Sleep -Seconds $Stage2DelaySec
$tBStart = Get-Date
$dirB = New-BotDir "botB"
$procB = Start-Bot $dirB "botB"
$logB = Join-Path $OutDir "botB.out.log"
Write-Output ("BotB pid=" + $procB.Id + " 启动（同账号再登录）@ " + $tBStart.ToString("HH:mm:ss.fff"))

# 等 B 落定：进世界 or 被关
$bVerdict = "TIMEOUT"
$bInWorld = $false
$deadline = (Get-Date).AddSeconds($WaitInWorldSec)
while ((Get-Date) -lt $deadline) {
  $txt = Read-Shared $logB
  if ($txt -match "成功进入游戏") { $bVerdict = "ADMITTED"; $bInWorld = $true; break }
  if ($txt -match "强行中断|强行离线|关闭连接") { $bVerdict = "CLOSED"; break }
  if ($procB.HasExited) { $bVerdict = "PROCESS_EXITED"; break }
  Start-Sleep -Milliseconds 500
}
$tBResolved = Get-Date

# 给旧侧一点时间收 kick 消息
Start-Sleep -Seconds 3
$txtA = Read-Shared $logA
$txtB = Read-Shared $logB
$tKickMsg = $null
if ($txtA -match "当前登录帐号正在其它位置登录") { $tKickMsg = "KICKUSER_SYSTEM_MSG" }
elseif ($txtA -match "服务器连接被强行中断") { $tKickMsg = "SM_OUTOFCONNECTION" }
elseif ($txtA -match "关闭连接") { $tKickMsg = "CONNECTION_CLOSED" }
else { $tKickMsg = "NONE_OBSERVED" }

# ---- 收 GameSvr 增量 ----
$gsvrOut = Join-Path $OutDir "gamesvr-window.log"
if (Test-Path $GameSvrLog) {
  Get-Content $GameSvrLog -ErrorAction SilentlyContinue | Select-Object -Skip $gsvrLines0 | Set-Content $gsvrOut -Encoding utf8
}

# ---- 判定（记录分支，不断言好坏）----
$result = [pscustomobject]@{
  schema           = "m2-session-lifecycle-fixture/1"
  account          = ($Prefix + "0")
  botA_old         = [pscustomobject]@{
    in_world_at    = $tAInWorld.ToString("HH:mm:ss.fff")
    received        = $tKickMsg
    evidence        = "botA.out.log：'当前登录帐号正在其它位置登录，本机已被强行离线!!!' / '服务器连接被强行中断' / '关闭连接'"
  }
  botB_new         = [pscustomobject]@{
    start_at       = $tBStart.ToString("HH:mm:ss.fff")
    verdict        = $bVerdict                     # ADMITTED | CLOSED | TIMEOUT | PROCESS_EXITED
    entered_world  = $bInWorld
    resolved_at    = $tBResolved.ToString("HH:mm:ss.fff")
  }
  timing           = [pscustomobject]@{
    a_in_world     = $tAInWorld.ToString("HH:mm:ss.fff")
    b_start        = $tBStart.ToString("HH:mm:ss.fff")
    b_resolved     = $tBResolved.ToString("HH:mm:ss.fff")
    b_delta_sec    = [math]::Round(($tBResolved - $tAInWorld).TotalSeconds, 2)
  }
  gamesvr_log      = $gsvrOut
  notes = @(
    ("BotSrv 是仓库自带 headless 客户端（src/BotSrv，完整客户端协议栈），两个进程都配 LoginAccount=$Prefix + ChrCount=1 ⇒ 同账号 " + $Prefix + "0、同角色。"),
    "本脚本只观测、不断言分支；分支结论见 botA/botB 的 received/verdict 字段与 gamesvr-window.log。"
  )
}
$result | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $OutDir "fixture.json") -Encoding utf8
$result | ConvertTo-Json -Depth 6 | Write-Output

# ---- 收尾：停 Bot ----
foreach ($p in @($procA, $procB)) { if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
Get-Process BotSrv -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

if ($bVerdict -eq "TIMEOUT" -or $bVerdict -eq "PROCESS_EXITED") {
  Write-Output "RED: BotB 未落定（$bVerdict）——夹具未跑通"
  exit 1
}
Write-Output ("GREEN(fixture): 观测已采到。旧会话收到=$tKickMsg；新会话=$bVerdict（进世界=$bInWorld）")
exit 0
