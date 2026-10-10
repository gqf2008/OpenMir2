#requires -Version 5
<#
.SYNOPSIS
  run_bots.ps1 — 假人压测驱动：BotSrv 多开登录/选角/进游戏，断言成功率。

.DESCRIPTION
  用车间里现成的 BotSrv（src/BotSrv，完整客户端协议栈的 headless 机器人）。
  本脚本只做编排：构建 → 写 AppSetting.json → 起进程 → 按日志计数 → 给判据。

  账号规则（BotSrv 内部约定）：账号名 = LoginAccount 前缀 + 递增序号，
  密码 = 账号名。-NewAccount 时机器人先走 CM_ADDNEWUSER 注册再登录。

  判据（默认全过才退出码 0）：
    - 日志中「帐号登录成功！」计数 == -Count
    - 进程在统计窗口内无异常退出
  红检（-SelfTestRed）：故意指向未监听端口，必须报失败退出码 1。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_bots.ps1 -Count 200 -Prefix loadbot
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_bots.ps1 -SelfTestRed
#>
param(
  [int]$Count = 200,
  [string]$Prefix = "loadbot",
  [string]$ServerName = "热血传奇",
  [string]$Address = "127.0.0.1",
  [int]$Port = 7000,
  [switch]$NewAccount,
  [int]$TimeoutSec = 900,
  [switch]$SkipBuild,
  [string]$LogFile = "",          # 默认 <repo>/tests/botload/bots-<时间戳>.log
  [switch]$SelfTestRed
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
$BotProj = Join-Path $RepoRoot "src\BotSrv\BotSrv.csproj"
$BotBinDir = Join-Path $RepoRoot "src\BotSrv\bin\Release"
$BotExe = Join-Path $BotBinDir "BotSrv.exe"

if ($SelfTestRed) {
    # 红检：指向一个必然不通的端口，驱动必须报失败（退出码 1）
    $Count = 3
    $Port = 17999
    $TimeoutSec = 40
    Write-Output "SELFTEST_RED: 指向 127.0.0.1:17999（无监听），预期失败"
}

if ($LogFile -eq "") {
    $outDir = Join-Path $RepoRoot "tests\botload"
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
    $LogFile = Join-Path $outDir ("bots-" + (Get-Date -Format "yyyyMMdd-HHmmss") + ".log")
}

# ---- 1. 构建 ----
if (-not $SkipBuild -or -not (Test-Path $BotExe)) {
    Write-Output "BUILD BotSrv..."
    dotnet build $BotProj -c Release --nologo -v q | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "BotSrv 构建失败" }
}
if (-not (Test-Path $BotExe)) { throw "找不到 $BotExe" }

# ---- 2. 写配置 ----
$cfg = @{
  BotPlay = @{
    ServerName     = $ServerName
    Address        = $Address
    Port           = $Port
    ChrCount       = $Count        # 单批全放（BotSrv 内部还有 3s/个 的软 stagger）
    TotalChrCount  = $Count
    NewAccount     = [bool]$NewAccount
    LoginAccount   = $Prefix
  }
} | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText((Join-Path $BotBinDir "AppSetting.json"), $cfg, (New-Object System.Text.UTF8Encoding($false)))
Write-Output "CONFIG: $Count bots, prefix=$Prefix, target=$Address`:$Port, NewAccount=$NewAccount"

# ---- 3. 起进程 ----
$proc = Start-Process -FilePath $BotExe -WorkingDirectory $BotBinDir -PassThru -WindowStyle Hidden `
  -RedirectStandardOutput $LogFile -RedirectStandardError "$LogFile.err"
Write-Output "BOTSRV_PID=$($proc.Id) LOG=$LogFile"

# ---- 4. 按日志计数 ----
$deadline = (Get-Date).AddSeconds($TimeoutSec)
$success = 0
$spawned = 0
function Read-LogShared([string]$Path) {
  # BotSrv 的 stdout 被 Start-Process 重定向持有：必须以 ReadWrite 共享方式读，否则打开即抛
  if (-not (Test-Path $Path)) { return "" }
  try {
    $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
      $sr = New-Object System.IO.StreamReader($fs)
      return $sr.ReadToEnd()
    } finally { $fs.Close() }
  } catch {
    Write-Output ("WARN: 读日志失败: " + $_.Exception.Message)
    return ""
  }
}
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Seconds 5
  $content = Read-LogShared $LogFile
  $success = ([regex]::Matches($content, "帐号登录成功")).Count
  $spawned = ([regex]::Matches($content, "开始登陆")).Count
  $failed = ([regex]::Matches($content, "此帐号不存在|密码错误|登录失败")).Count
  Write-Output ("PROGRESS spawned=" + $spawned + " login_ok=" + $success + "/" + $Count + " failed=" + $failed)
  if ($success -ge $Count) { break }
  if ($proc.HasExited) { Write-Output "WARN: BotSrv 进程提前退出 code=$($proc.ExitCode)"; break }
}

$rate = if ($Count -gt 0) { [math]::Round(100.0 * $success / $Count, 2) } else { 0 }
$summary = [pscustomobject]@{
  count = $Count; spawned = $spawned; login_ok = $success
  success_rate_pct = $rate; timeout_sec = $TimeoutSec; log = $LogFile
}
$summaryPath = [System.IO.Path]::ChangeExtension($LogFile, ".summary.json")
$summary | ConvertTo-Json | Set-Content -Path $summaryPath -Encoding utf8

if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }

if ($SelfTestRed) {
  if ($success -lt $Count) {
    Write-Output "SELFTEST_RED GREEN: 连不通 => 判失败成立（login_ok=$success/$Count）"
    exit 0   # 红检本身按预期变红 = 自测通过
  }
  Write-Output "SELFTEST_RED RED: 连不通却报成功——判据失效！"
  exit 1
}

if ($success -ge $Count) {
  Write-Output ("GREEN: " + $success + "/" + $Count + " 登录成功 (100%)")
  exit 0
}
Write-Output ("RED: 登录成功 " + $success + "/" + $Count + " (" + $rate + "%) 未达标")
exit 1
