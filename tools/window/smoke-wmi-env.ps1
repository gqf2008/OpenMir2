# WMI 派生链 env-pin 冒烟（卡 `openmir2-window-wmi-env-forwarding`，tier=L）。
#
# 验什么：53 分诊实锤的缺口——`Win32_Process.Create` 派生不继承调用方环境，
# `MIR2_CLIENT_ABILITY_LAYOUT` 等起栈链丢失（game-svr.out.log 05:57:06.620
# `CLIENT_ABILITY_LAYOUT layout=client-58（MIR2_CLIENT_ABILITY_LAYOUT=<unset>）`）。
# 本冒烟走**修复后的派生链**（start-stack-wmi.ps1 的
# Win32_ProcessStartup.EnvironmentVariables 全量转发）把**真服务端**（mir2-server
# 的 game-svr 角色，**单进程**、不起完整栈）拉起来，然后两层校验：
#   ① 内层脚本等 server 日志出现 `CLIENT_ABILITY_LAYOUT` 自证行（≤ -WaitSec），到点杀进程；
#   ② 包装器 -PinChecks/-PinLog 自证：日志必须出现 `KEY=VAL` 且生效布局 token 对上
#      （0|false ⇒ layout=oracle-40，否则 ⇒ layout=client-58），不一致 rc=1（fail-closed）。
#
# 用法：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/smoke-wmi-env.ps1 `
#     [-Root E:/tmp/conformance/rust-root] [-Pin MIR2_CLIENT_ABILITY_LAYOUT=0] [-OutDir E:/tmp/w75-smoke-run]
#
# 前置：-Root 下有 M2GameSvr\Server.conf（内容部署已就位）；仓内已 `cargo build --release`
#       （或显式 -ServerExe）；Server.conf 的 GatePort 空闲（被占 ⇒ 拒跑，不撞别人的栈）。
#       本脚本不写库、不改配置、不动 E:/tmp/conformance 的窗口证据目录（日志全落 -OutDir）。
# 退出码：0 = WMI 派生 + pin 自证通过；1 = 自证失败 / server 没打出自证行；2 = 前置不满足。
param(
  [string]$ServerExe = '',
  [string]$Root = 'E:/tmp/conformance/rust-root',
  [string]$Pin = 'MIR2_CLIENT_ABILITY_LAYOUT=0',
  [string]$OutDir = 'E:/tmp/w75-smoke-run',
  [string]$Wrapper = '',
  [int]$WaitSec = 90
)

$ErrorActionPreference = 'Stop'
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

# ---- 前置：wrapper / server exe / pin / conf ---------------------------------
if ([string]::IsNullOrWhiteSpace($Wrapper)) {
  $Wrapper = Join-Path $RepoRoot 'tools\window\start-stack-wmi.ps1'
}
if (-not (Test-Path -LiteralPath $Wrapper -PathType Leaf)) {
  Write-Output ('RED: 找不到 WMI 包装：' + $Wrapper)
  exit 2
}
if ([string]::IsNullOrWhiteSpace($ServerExe)) {
  $releaseExe = Join-Path $RepoRoot 'mir2-rs\target\release\mir2-server.exe'
  $debugExe = Join-Path $RepoRoot 'mir2-rs\target\debug\mir2-server.exe'
  if (Test-Path -LiteralPath $releaseExe -PathType Leaf) { $ServerExe = $releaseExe }
  else { $ServerExe = $debugExe }
}
if (-not (Test-Path -LiteralPath $ServerExe -PathType Leaf)) {
  Write-Output ('RED: mir2-server 不存在：' + $ServerExe + '（先 cargo build --release -p mir2-server 或显式 -ServerExe）')
  exit 2
}
$eq = $Pin.IndexOf('=')
if ($eq -lt 1 -or $eq -ge ($Pin.Length - 1)) {
  Write-Output ('RED: -Pin 非法（应为 KEY=VAL）：' + $Pin)
  exit 2
}
$pinKey = $Pin.Substring(0, $eq)
$pinVal = $Pin.Substring($eq + 1)

$conf = Join-Path $Root 'M2GameSvr\Server.conf'
if (-not (Test-Path -LiteralPath $conf -PathType Leaf)) {
  Write-Output ('RED: 缺配置：' + $conf)
  exit 2
}
$confText = [System.IO.File]::ReadAllText($conf)
$gatePort = 0
$m = [regex]::Match($confText, '(?m)^\s*GatePort\s*=\s*(\d+)')
if ($m.Success) { $gatePort = [int]$m.Groups[1].Value }
if ($gatePort -le 0) {
  Write-Output ('RED: ' + $conf + ' 里没有 GatePort（conf 不合预期）')
  exit 2
}

function Test-Port([int]$Port) {
  foreach ($ep in [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()) {
    if ($ep.Port -eq $Port) { return $true }
  }
  return $false
}
if (Test-Port $gatePort) {
  Write-Output ('RED: GatePort ' + $gatePort + ' 已被监听——先停现有栈再冒烟（本脚本不撞别人的栈）')
  exit 2
}

# ---- 产物目录 + 内层脚本（具体路径烤进脚本里；自身产物落 -OutDir）------------
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$workDir = Split-Path -Parent $conf
$outLog = Join-Path $OutDir 'game-svr.out.log'
$errLog = Join-Path $OutDir 'game-svr.err.log'
$resultFile = Join-Path $OutDir 'inner-result.txt'
$pidFile = Join-Path $OutDir 'inner-server-pid.txt'
$innerPath = Join-Path $OutDir 'inner-game-svr.ps1'
Remove-Item -LiteralPath $outLog, $errLog, $resultFile, $pidFile -Force -ErrorAction SilentlyContinue

$inner = @'
$ErrorActionPreference = 'Continue'
$proc = Start-Process -FilePath '__EXE__' -ArgumentList @('game-svr', '__CONF__') -WorkingDirectory '__WORK__' -RedirectStandardOutput '__OUT__' -RedirectStandardError '__ERR__' -WindowStyle Hidden -PassThru
Set-Content -LiteralPath '__PID__' -Value ([string]$proc.Id)
$deadline = (Get-Date).AddSeconds(__WAITSEC__)
$found = $false; $line = ''
while ((Get-Date) -lt $deadline) {
  if (Test-Path -LiteralPath '__OUT__' -PathType Leaf) {
    $m = [regex]::Match([System.IO.File]::ReadAllText('__OUT__'), 'CLIENT_ABILITY_LAYOUT[^
]{0,120}')
    if ($m.Success) { $found = $true; $line = $m.Value; break }
  }
  if ($proc.HasExited) { break }
  Start-Sleep -Milliseconds 500
}
Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Set-Content -LiteralPath '__RES__' -Value ('found=' + $found + ' line=' + $line + ' exited=' + $proc.HasExited)
if ($found) { exit 0 } else { exit 1 }
'@
$inner = $inner.Replace('__EXE__', $ServerExe).Replace('__CONF__', $conf).Replace('__WORK__', $workDir).Replace('__OUT__', $outLog).Replace('__ERR__', $errLog).Replace('__RES__', $resultFile).Replace('__PID__', $pidFile).Replace('__WAITSEC__', [string]$WaitSec)
$inner | Set-Content -LiteralPath $innerPath -Encoding ASCII

# ---- 设 pin → 经修复后的 WMI 派生链起 server --------------------------------
Set-Item -Path ('Env:' + $pinKey) -Value $pinVal
Write-Output ('PIN ' + $pinKey + '=' + $pinVal)
Write-Output ('SERVER_EXE ' + $ServerExe)
Write-Output ('CONF ' + $conf + ' (GatePort=' + $gatePort + ')')

$out = & powershell -NoProfile -ExecutionPolicy Bypass -File $Wrapper `
  -Script $innerPath -ReadyPorts 0 -SkipPortWait `
  -PinChecks $Pin -PinLog $outLog -PinWaitSec $WaitSec 2>&1
$wrapperRc = $LASTEXITCODE
foreach ($l in @($out)) { Write-Output ('wrapper| ' + $l) }

$innerRc = -1
if (Test-Path -LiteralPath $resultFile -PathType Leaf) {
  Write-Output ('inner| ' + ([System.IO.File]::ReadAllText($resultFile)).Trim())
  $innerRc = 0
}
if (Test-Path -LiteralPath $outLog -PathType Leaf) {
  Write-Output ('log| ' + ((Get-Content -LiteralPath $outLog -TotalCount 3) -join ' / '))
}

# ---- 清场：万一内层脚本没杀干净，兜底杀一次 --------------------------------
if (Test-Path -LiteralPath $pidFile -PathType Leaf) {
  $leftPid = 0
  if ([int]::TryParse(([System.IO.File]::ReadAllText($pidFile)).Trim(), [ref]$leftPid)) {
    Stop-Process -Id $leftPid -Force -ErrorAction SilentlyContinue
  }
}

if ($wrapperRc -ne 0) {
  Write-Output ('SMOKE FAIL (wrapper rc=' + $wrapperRc + ')')
  exit 1
}
Write-Output 'SMOKE OK (WMI 派生 + pin 自证通过；server 日志可见 pin 生效)'
exit 0
