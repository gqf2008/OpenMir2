#requires -Version 5
<#
  start-csharp-hook.ps1 — C# oracle 影子栈（带 RNG 同种子钩子）的**仓库内**规范起/停脚本。
  （历史上该脚本是各 lane 仓外自备的 `E:/tmp/*/csharp-stack/start-csharp-hook.ps1`；本片把它收进仓库，
   让「影子 hook + 隔离开关」这条夹具链路可当 patch 评审、可复现。）

  与 `E:\MirServer\start-all.ps1` 同序（DBSrv -> LoginSrv -> GameSvr -> gates），唯一差别：
  GameSvr 用**影子副本**（runtimeconfig 已开 StartupHookProvider + 已放 RngSeedHook.dll），
  并在启动进程环境里设 DOTNET_STARTUP_HOOKS / MIR2_RNG_SEED / MIR2_RNG_LOG / MIR2_RNG_SITE。
  `E:\MirServer\M2GameSvr` 冻结基线一字未动（只读 robocopy 到影子目录）。

  M3②③ 活栈隔离口径（owner 2026-10-12 21:15）：
    -IsolateAttack  给 GameSvr 进程加 MIR2_RNG_ISOLATE_ATTACK=1 / MIR2_RNG_ISOLATE_SITES /
                    MIR2_RNG_ATTACK_LOG（默认关：三个 env 都不设，行为与现在逐字节相同）。
    attack_seed = MIR2_RNG_SEED + 0x9E37（由 hook 自己算；本脚本把 seed/路径/hash 记进
    <LogDir>/csharp_isolation.json 供夹具与审计）。

  窗口纪律：起栈前自查 5000/7000/7100/7200 空闲；收工用 -Stop（本脚本不负责别的 lane 的栈）。
#>
param(
  [int]$Seed = 42,
  [switch]$IsolateAttack,
  [string]$IsolateSites = '',       # 缺省 <LogDir>\rng-isolate-sites.txt
  [string]$AttackStreamLog = '',    # 缺省 <LogDir>\rng-attack-stream.tsv
  [switch]$Stop,
  [switch]$Status,
  [int]$WaitSeconds = 60,
  [string]$Repo = '',
  [string]$Pack = 'E:\MirServer',
  [string]$Shadow = '',
  [string]$LogDir = 'E:\tmp\m3-csharp-hook',
  [switch]$SkipHookDeploy
)
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrEmpty($Repo)) {
  $Here = Split-Path -Parent $MyInvocation.MyCommand.Path
  $Repo = (Resolve-Path (Join-Path $Here '..\..\..\..')).Path
}
if ([string]::IsNullOrEmpty($Shadow)) { $Shadow = Join-Path $Pack 'M2GameSvr-hook' }
if ([string]::IsNullOrEmpty($IsolateSites)) { $IsolateSites = Join-Path $LogDir 'rng-isolate-sites.txt' }
if ([string]::IsNullOrEmpty($AttackStreamLog)) { $AttackStreamLog = Join-Path $LogDir 'rng-attack-stream.tsv' }
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$PidFile = Join-Path $LogDir 'csharp-stack.pid.json'

function Get-Sha256Hex([string]$Path) {
  if ([string]::IsNullOrEmpty($Path)) { throw 'Get-Sha256Hex: 路径为空' }
  if (-not (Test-Path -LiteralPath $Path)) { throw "Get-Sha256Hex: 文件不存在: $Path" }
  $alg = [System.Security.Cryptography.SHA256]::Create()
  try {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    return ([System.BitConverter]::ToString($alg.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant()
  } finally { $alg.Dispose() }
}

function Test-Port([int]$Port) {
  foreach ($ep in [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()) {
    if ($ep.Port -eq $Port) { return $true }
  }
  return $false
}

function Show-Ports {
  foreach ($p in @(3306,6000,5100,5700,5500,5600,5000,7000,7100,7200)) {
    Write-Output ("  {0,-5} {1}" -f $p, $(if (Test-Port $p) { 'LISTEN' } else { 'DOWN' }))
  }
}

if ($Status) { Show-Ports; exit 0 }

if ($Stop) {
  if (Test-Path $PidFile) {
    foreach ($e in @(Get-Content $PidFile -Raw | ConvertFrom-Json)) {
      $p = Get-Process -Id $e.pid -ErrorAction SilentlyContinue
      if ($p) { Stop-Process -Id $e.pid -Force; Write-Output ("STOPPED {0} pid={1}" -f $e.name, $e.pid) }
      else { Write-Output ("ALREADY_GONE {0} pid={1}" -f $e.name, $e.pid) }
    }
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
  } else { Write-Output 'NOTHING_TO_STOP' }
  Start-Sleep -Seconds 2
  Show-Ports
  exit 0
}

# 目标端口必须空闲（MySQL 3306 可已在）。
$wanted = @(6000,5100,5700,5500,5600,5000,7000,7100,7200)
$busy = @($wanted | Where-Object { Test-Port $_ })
if ($busy.Count -gt 0) { Write-Output ("RED: 端口已被监听 " + ($busy -join ',')); exit 1 }

# ---- 准备影子副本（不存在则 robocopy 冻结基线 + 打开 StartupHookProvider） ----
$shadowGs = Join-Path $Shadow 'GameSrv.exe'
if (-not (Test-Path $shadowGs)) {
  $src = Join-Path $Pack 'M2GameSvr'
  if (-not (Test-Path (Join-Path $src 'GameSrv.exe'))) { Write-Output ("RED: 冻结基线不存在: {0}" -f $src); exit 1 }
  Write-Output ("SHADOW_COPY {0} -> {1}" -f $src, $Shadow)
  New-Item -ItemType Directory -Force -Path $Shadow | Out-Null
  & robocopy $src $Shadow /E /NFL /NDL /NJH /NJS /NP | Out-Null
  if ($LASTEXITCODE -gt 7) { Write-Output ("RED: robocopy 失败 rc={0}" -f $LASTEXITCODE); exit 1 }
  $rcPath = Join-Path $Shadow 'GameSrv.runtimeconfig.json'
  $rcObj = Get-Content $rcPath -Raw | ConvertFrom-Json
  $rcObj.runtimeOptions.configProperties.'System.StartupHookProvider.IsSupported' = $true
  $rcObj | ConvertTo-Json -Depth 10 | Set-Content -Path $rcPath -Encoding utf8
}

# ---- 构建并部署 hook（隔离/记录逻辑都在 hook 里） ----
if (-not $SkipHookDeploy) {
  $hookProj = Join-Path $Repo 'mir2-rs\tests\parity\m3\rng-hook\RngSeedHook.csproj'
  & dotnet build $hookProj -c Release --nologo -v q | Out-Null
  if ($LASTEXITCODE -ne 0) { Write-Output 'RED: RngSeedHook 构建失败'; exit 1 }
  $hookDll = Join-Path $Repo 'mir2-rs\tests\parity\m3\rng-hook\bin\Release\net8.0\RngSeedHook.dll'
  if (-not (Test-Path $hookDll)) { Write-Output ("RED: 找不到 hook 产物 {0}" -f $hookDll); exit 1 }
  Copy-Item -Force $hookDll (Join-Path $Shadow 'RngSeedHook.dll')
  Write-Output ("HOOK_DEPLOYED {0} sha256={1}" -f (Join-Path $Shadow 'RngSeedHook.dll'), (Get-Sha256Hex (Join-Path $Shadow 'RngSeedHook.dll')))
}

$started = @()
function Start-One($Name,$Exe,$Work,$Ports,$StopOrder) {
  $out = Join-Path $LogDir ($Name + '.out.log')
  $err = Join-Path $LogDir ($Name + '.err.log')
  Remove-Item $out,$err -Force -ErrorAction SilentlyContinue
  $p = Start-Process -FilePath $Exe -WorkingDirectory $Work -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput $out -RedirectStandardError $err
  $script:started += [pscustomobject]@{ name=$Name; pid=$p.Id; ports=$Ports; stop_order=$StopOrder; exe=$Exe }
  Write-Output ("START {0} pid={1} ports={2} exe={3}" -f $Name,$p.Id,($Ports -join ','),$Exe)
}

$dbDir = Join-Path $Repo 'src\DBSrv\bin\Release'
$lsDir = Join-Path $Repo 'src\LoginSrv\bin\Release'
Start-One 'DBSrv'    (Join-Path $dbDir 'DBSrv.exe')    $dbDir @(6000,5100,5700) 7
Start-One 'LoginSrv' (Join-Path $lsDir 'LoginSrv.exe') $lsDir @(5500,5600)      6

# GameSvr（影子 + 钩子）
$hook = Join-Path $Shadow 'RngSeedHook.dll'
$env:DOTNET_STARTUP_HOOKS = $hook
$env:MIR2_RNG_SEED = "$Seed"
$env:MIR2_RNG_LOG = Join-Path $LogDir 'rng-stream.tsv'
$env:MIR2_RNG_SITE = '1'
try {
  if ($IsolateAttack) {
    $env:MIR2_RNG_ISOLATE_ATTACK = '1'
    $env:MIR2_RNG_ISOLATE_SITES = $IsolateSites
    $env:MIR2_RNG_ATTACK_LOG = $AttackStreamLog
    Write-Output ("ISOLATE_ATTACK on=true seed={0} sites={1} attack_stream={2}" -f $Seed, $IsolateSites, $AttackStreamLog)
  } else {
    Write-Output 'ISOLATE_ATTACK on=false（默认关；行为逐字节不变）'
  }
  Start-One 'GameSvr' (Join-Path $Shadow 'GameSrv.exe') $Shadow @(5000) 5
} finally {
  Remove-Item Env:DOTNET_STARTUP_HOOKS,Env:MIR2_RNG_SEED,Env:MIR2_RNG_LOG,Env:MIR2_RNG_SITE -ErrorAction SilentlyContinue
  Remove-Item Env:MIR2_RNG_ISOLATE_ATTACK,Env:MIR2_RNG_ISOLATE_SITES,Env:MIR2_RNG_ATTACK_LOG -ErrorAction SilentlyContinue
}

Start-One 'LoginGate' (Join-Path $Pack 'LoginGate\LoginGate.exe') (Join-Path $Pack 'LoginGate') @(7000) 1
Start-One 'SelGate'   (Join-Path $Pack 'SelGate\SelGate.exe')     (Join-Path $Pack 'SelGate')   @(7100) 2
Start-One 'RunGate'   (Join-Path $Pack 'RunGate\GameGate.exe')    (Join-Path $Pack 'RunGate')   @(7200) 3

$started | ConvertTo-Json -Depth 5 | Set-Content -Path $PidFile -Encoding utf8
Write-Output ("PIDFILE " + $PidFile)

$deadline = (Get-Date).AddSeconds($WaitSeconds)
while ((Get-Date) -lt $deadline) {
  if (@($wanted | Where-Object { Test-Port $_ }).Count -eq $wanted.Count) { break }
  Start-Sleep -Milliseconds 500
}
$missing = @($wanted | Where-Object { -not (Test-Port $_) })
if ($missing.Count -gt 0) { Write-Output ("RED: 端口未就绪 " + ($missing -join ',')); Show-Ports; exit 1 }

# 隔离记账（夹具/审计）：seed / attack_seed / dump + sha256。
$attackSeed = 0
$seedSum = [int64]$Seed + [int64]40503
$seedRev = $seedSum -band [int64]4294967295
if ($seedRev -ge [int64]2147483648) { $seedRev -= [int64]4294967296 }
$attackSeed = [int]$seedRev
$sitesSha = if (Test-Path -LiteralPath $IsolateSites) { Get-Sha256Hex $IsolateSites } else { '' }
$attackStreamSha = if (Test-Path -LiteralPath $AttackStreamLog) { Get-Sha256Hex $AttackStreamLog } else { '' }
$isoMeta = [ordered]@{
  isolate_attack = [bool]$IsolateAttack; seed = $Seed; attack_seed = $attackSeed
  sites_dump = $IsolateSites; sites_dump_sha256 = $sitesSha
  attack_stream = $AttackStreamLog; attack_stream_sha256 = $attackStreamSha
  hook_dll_sha256 = (Get-Sha256Hex $hook); ts = (Get-Date).ToString('o')
}
$isoMeta | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $LogDir 'csharp_isolation.json') -Encoding utf8
Write-Output ("ISOLATION_RECORDED {0} isolate={1} attack_seed={2} sites_sha256={3}" -f (Join-Path $LogDir 'csharp_isolation.json'), [bool]$IsolateAttack, $attackSeed, $sitesSha)

Write-Output 'CSHARP_STACK_GREEN'
Show-Ports
