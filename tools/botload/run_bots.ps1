#requires -Version 5
<#
.SYNOPSIS
  run_bots.ps1 — 假人压测驱动：BotSrv 多开登录/选角/进游戏，断言成功率。

.DESCRIPTION
  用车间里现成的 BotSrv（src/BotSrv，完整客户端协议栈的 headless 机器人）。
  本脚本只做编排：构建 → 写 AppSetting.json → 起进程 → 读 BotSrv 的 load_stats.ndjson 计数 → 给判据。

  账号规则（BotSrv 内部约定）：账号名 = LoginAccount 前缀 + 递增序号，
  密码 = 账号名。-NewAccount 时机器人先走 CM_ADDNEWUSER 注册再登录。

  判据（默认全过才退出码 0）：
    - BotSrv 内部计数 load_stats.ndjson 的 login_ok == -Count（**不抓日志行**：
      LoginScene.ClientGetPasswordOk 在「有剩余时长」的两个分支只打「个人帐户的期限」，
      抓「帐号登录成功」会把这类账号误判成登录失败 ⇒ 假红）
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
  [switch]$FixedNewChr,           # M3 夹具：固定建角 Job/Hair/Sex（默认不固定 = 原随机行为）
  [int]$NewChrJob = 0,            # 仅 -FixedNewChr 时生效（写进 AppSetting.json 的 BotPlay 段）
  [int]$NewChrHair = 0,           # 仅 -FixedNewChr 时生效
  [int]$NewChrSex = 0,            # 仅 -FixedNewChr 时生效
  [switch]$BagEquipSequence,      # M3 T-14 夹具：进世界后按固定顺序发背包/装备动作（拾取→装备→卸下→丢弃→用药）
  [int]$BagEquipStepMs = 1000,    # 仅 -BagEquipSequence 时生效：每步间隔
  [int]$BagEquipStartDelayMs = 3000, # 仅 -BagEquipSequence 时生效：进世界到第一步的额外等待
  [switch]$AttackSequence,           # M3②③ 夹具：进世界后按固定攻击序列发 CM_HIT（T-22 扩展）
  [int]$AttackCount = 120,           # 仅 -AttackSequence 时生效：攻击步数
  [int]$AttackStepMs = 500,          # 仅 -AttackSequence 时生效：每步间隔
  [int]$AttackStartDelayMs = 3000,   # 仅 -AttackSequence 时生效：进世界到第一步的额外等待
  [int]$SoftCloseAfterWorldMs = 0,   # M1③ 退出路径夹具：进世界后发 CM_SOFTCLOSE(1009)；0=关（默认）
  [switch]$DisableAutoPlay,          # M1③ 退出路径夹具：关掉自动挂机（默认关=原行为）；见 ClientManager.RunAutoPlay
  [switch]$WriteConfigOnly,       # 只写 AppSetting.json 并退出（不构建/不起进程；供无栈自检与红检用）
  [switch]$RequireWorld,          # F4（T-12）：要求全部假人都有"本会话收到 SM_LOGON"的进世界证据（in_world == Count）才 GREEN
  [switch]$RequireCharCreated,    # 固定起点变体①预热相：等到 N 个「创建角色」标记（只建号，不等进世界）才收工
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

# 统计目录：**必须显式给 BotSrv**，不要让它回落到 exe 同级的 load_out.conf。
# 坑（C6 第 4 轮自查实测）：load_gate.ps1 会把 bin/Release/load_out.conf 写成「最后一档」目录且
# 跑完不清除；run_bots 不设 MIR2_BOT_OUT 时，BotSrv 的 LoadMetrics.ResolveOutDir 会读该 conf ⇒
# 3 假人冒烟用 FileMode.Create 覆写 tests/loadgate/load-*/tier-1000/load_stats.ndjson
#（022821 / 114831 / 121254 三处已发生，见 tests/loadgate/README.md「证据完整性告警」）。
# 设了 MIR2_BOT_OUT 后 AppService 把它当显式参数传给 LoadMetrics.Init，优先级压过 conf。
$statsDir = Join-Path (Split-Path -Parent $LogFile) ([System.IO.Path]::GetFileNameWithoutExtension($LogFile))
New-Item -ItemType Directory -Force -Path $statsDir | Out-Null
$statsPath = Join-Path $statsDir "load_stats.ndjson"
$env:MIR2_BOT_OUT = $statsDir

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
    FixedNewChr    = [bool]$FixedNewChr
    NewChrJob      = $NewChrJob
    NewChrHair     = $NewChrHair
    NewChrSex      = $NewChrSex
    BagEquipSequence    = [bool]$BagEquipSequence
    BagEquipStepMs      = $BagEquipStepMs
    BagEquipStartDelayMs = $BagEquipStartDelayMs
    AttackSequence      = [bool]$AttackSequence
    AttackCount         = $AttackCount
    AttackStepMs        = $AttackStepMs
    AttackStartDelayMs  = $AttackStartDelayMs
    SoftCloseAfterWorldMs = $SoftCloseAfterWorldMs
    DisableAutoPlay     = [bool]$DisableAutoPlay
  }
} | ConvertTo-Json -Depth 4
New-Item -ItemType Directory -Force -Path $BotBinDir | Out-Null
[System.IO.File]::WriteAllText((Join-Path $BotBinDir "AppSetting.json"), $cfg, (New-Object System.Text.UTF8Encoding($false)))
Write-Output "CONFIG: $Count bots, prefix=$Prefix, target=$Address`:$Port, NewAccount=$NewAccount, FixedNewChr=$FixedNewChr"
if ($FixedNewChr) {
    Write-Output "CONFIG_FIXED_NEWCHR Job=$NewChrJob Hair=$NewChrHair Sex=$NewChrSex"
}
if ($BagEquipSequence) {
    Write-Output "CONFIG_BAGEQUIPSEQ step_ms=$BagEquipStepMs start_delay_ms=$BagEquipStartDelayMs"
}
if ($AttackSequence) {
    Write-Output "CONFIG_ATTACKSEQ count=$AttackCount step_ms=$AttackStepMs start_delay_ms=$AttackStartDelayMs"
}
if ($SoftCloseAfterWorldMs -gt 0) {
    Write-Output "CONFIG_SOFTCLOSE after_world_ms=$SoftCloseAfterWorldMs"
}
if ($DisableAutoPlay) {
    Write-Output "CONFIG_DISABLE_AUTOPLAY on"
}

# 无栈自检/红检：只写配置即退（不构建、不起进程）。
if ($WriteConfigOnly) {
    Write-Output ("WRITE_CONFIG_ONLY " + (Join-Path $BotBinDir "AppSetting.json"))
    exit 0
}

# ---- 3. 起进程 ----
$proc = Start-Process -FilePath $BotExe -WorkingDirectory $BotBinDir -PassThru -WindowStyle Hidden `
  -RedirectStandardOutput $LogFile -RedirectStandardError "$LogFile.err"
Write-Output "BOTSRV_PID=$($proc.Id) LOG=$LogFile"

# ---- 4. 读 BotSrv 内部计数（load_stats.ndjson）----
# 判据来源 = BotSrv 自己的 LoadMetrics.LoginOk()（三个分支都计），与 load_gate 同源；
# 不再抓「帐号登录成功」日志行（理由见文件头 .DESCRIPTION）。
$deadline = (Get-Date).AddSeconds($TimeoutSec)
$success = 0
$spawned = 0
$worldOk = 0
# M3 T-14：-BagEquipSequence 开启时，等**整串** `M3BAGSEQ`（含逐个假人的 `M3BAGSEQ done`）落日志后再收工。
# 否则 run_bots 会在 `login_ok == Count` 处立即 break + kill（登录早于进世界/序列），
# 假人没跑完 拾取→装备→卸下→丢弃→用药 就被杀，run_pair 的 bag_equip_markers 判据必红（第九格从未在真栈跑过）。
$bagDone = 0
$charCreated = 0
function Read-SharedText([string]$Path) {
  # BotSrv 的 StreamWriter 以 FileShare.ReadWrite 持有：读侧也必须放行写，否则打开即抛
  if (-not (Test-Path $Path)) { return "" }
  try {
    $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
      $sr = New-Object System.IO.StreamReader($fs)
      return $sr.ReadToEnd()
    } finally { $fs.Close() }
  } catch {
    Write-Output ("WARN: 读统计失败: " + $_.Exception.Message)
    return ""
  }
}
function Get-LastStats([string]$Path) {
  $txt = Read-SharedText $Path
  if ($txt -eq "") { return $null }
  $lines = $txt -split "`n" | Where-Object { $_.Trim() -ne "" }
  if ($lines.Count -eq 0) { return $null }
  try { return ($lines[-1] | ConvertFrom-Json) } catch { return $null }
}
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Seconds 5
  $st = Get-LastStats $statsPath
  if ($st -ne $null) {
    $spawned = [int]$st.spawned
    $success = [int]$st.login_ok
    if ($st.PSObject.Properties.Name -contains "in_world") { $worldOk = [int]$st.in_world }
  }
  if ($BagEquipSequence) {
    # 序列标记由 BotSrv 打到自己的 stdout（= $LogFile），读取同 stats 一样放行写侧。
    $bagDone = ([regex]::Matches((Read-SharedText $LogFile), 'M3BAGSEQ done')).Count
  }
  if ($RequireCharCreated) {
    # 固定起点预热：等「创建角色」标记（BotSrv `SelectChrScene.cs` 的 LogService.Info）
    $charCreated = ([regex]::Matches((Read-SharedText $LogFile), '创建角色')).Count
  }
  $bagNote = if ($BagEquipSequence) { " bag_seq_done=" + $bagDone + "/" + $Count } else { "" }
  Write-Output ("PROGRESS spawned=" + $spawned + " login_ok=" + $success + "/" + $Count + " in_world=" + $worldOk + "/" + $Count + $bagNote + " (src=load_stats.ndjson)")
  if ($BagEquipSequence) {
    # T-14：序列跑完（每个假人一条 done）才算收工；否则继续等（超时上限仍是 TimeoutSec）。
    if ($success -ge $Count -and $bagDone -ge $Count) { break }
  } elseif ($RequireCharCreated) {
    # 固定起点①预热：等到 N 个「创建角色」即 break（不要求进世界）。
    # 另等 load_stats 的 login_ok 到齐：stats 是 BotSrv 每 5s 采样一次，而「创建角色」标记
    # 在登录后立即落日志 ⇒ 只判 charCreated 会在采样边界提前 break，读到「上一拍」的
    # login_ok=0，末尾成功判据（$success -ge $Count）恒假红（T-14 两相 pin 实跑踩到）。
    if ($charCreated -ge $Count -and $success -ge $Count) { break }
  } elseif ($RequireWorld) {
    # F4（T-12）：登录不等于进世界；等"本会话收到 SM_LOGON"的进世界证据到齐。
    if ($worldOk -ge $Count) { break }
  } elseif ($success -ge $Count) { break }
  if ($proc.HasExited) { Write-Output "WARN: BotSrv 进程提前退出 code=$($proc.ExitCode)"; break }
}

$rate = if ($Count -gt 0) { [math]::Round(100.0 * $success / $Count, 2) } else { 0 }
$worldRate = if ($Count -gt 0) { [math]::Round(100.0 * $worldOk / $Count, 2) } else { 0 }
$summary = [pscustomobject]@{
  count = $Count; spawned = $spawned; login_ok = $success; in_world = $worldOk
  bag_seq_done = $bagDone; bag_equip_sequence = [bool]$BagEquipSequence
  char_created = $charCreated; require_char_created = [bool]$RequireCharCreated
  success_rate_pct = $rate; world_rate_pct = $worldRate; require_world = [bool]$RequireWorld
  timeout_sec = $TimeoutSec; log = $LogFile; stats = $statsPath
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

if ($success -ge $Count -and (-not $RequireWorld -or $worldOk -ge $Count) -and (-not $BagEquipSequence -or $bagDone -ge $Count) -and (-not $RequireCharCreated -or $charCreated -ge $Count)) {
  Write-Output ("GREEN: " + $success + "/" + $Count + " 登录成功 (100%)" + $(if ($RequireWorld) { "；in_world=" + $worldOk + "/" + $Count } else { "" }) + $(if ($BagEquipSequence) { "；bag_seq_done=" + $bagDone + "/" + $Count } else { "" }) + $(if ($RequireCharCreated) { "；char_created=" + $charCreated + "/" + $Count } else { "" }))
  exit 0
}
if ($BagEquipSequence -and $bagDone -lt $Count) {
  Write-Output ("RED: " + $Count + " 个假人的背包/装备序列只跑完 " + $bagDone + "/" + $Count + "（缺 M3BAGSEQ done）——序列不完整，不能当『同一序列』")
  exit 1
}
if ($RequireWorld -and $worldOk -lt $Count) {
  Write-Output ("RED: 进世界证据 in_world=" + $worldOk + "/" + $Count + "（" + $worldRate + "%）未达标（-RequireWorld：要求每个假人本会话都收到 SM_LOGON）")
  exit 1
}
if ($RequireCharCreated -and $charCreated -lt $Count) {
  Write-Output ("RED: 建角证据 char_created=" + $charCreated + "/" + $Count + " 未达标（-RequireCharCreated：预热建角没完成）")
  exit 1
}
Write-Output ("RED: 登录成功 " + $success + "/" + $Count + " (" + $rate + "%) 未达标")
exit 1
