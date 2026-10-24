# -*- coding: utf-8 -*-
# start-csharp.ps1 — 窗口 C# 私有栈（csharp-root / 15xxx 端口）启动器，**仓库内事实源**。
#
# 背景（卡 openmir2-window-csharp-rng，review 3c3591d8 F1）：
#   `card_window.py` 的 alt profile 原先指向 `E:/tmp/conformance/start-csharp3.ps1`——该脚本
#   没有 `-Seed` / `MIR2_RNG_SEED` / `DOTNET_STARTUP_HOOKS`，oracle 仍按时间种子 ⇒
#   RNG 类窗口项（W-DG-1/2、W-CM-3）在 C# 侧不可验证；且 `E:/tmp/conformance` 会被外部清扫
#   （已发生三次）。本脚本把启动面收进仓库，并在**给 -RngLog（或环境 MIR2_RNG_LOG）时**：
#     * 影子复制 `<Root>/M2GameSvr` → `<Root>/M2GameSvr-hook`（冻结基线不动），
#     * 打开影子 runtimeconfig 的 `System.StartupHookProvider.IsSupported`，
#     * 用 `dotnet build` 构建 `mir2-rs/tests/parity/m3/rng-hook/RngSeedHook.csproj` 并部署 dll，
#     * 只给 GameSvr 子进程注入 `DOTNET_STARTUP_HOOKS` + `MIR2_RNG_SEED` + `MIR2_RNG_LOG`
#       + `MIR2_RNG_SITE=1`（其余角色不注入，行为逐字节不变）。
#   不给 -RngLog 时**不部署钩子、不注种子**（默认行为与旧 start-csharp3.ps1 相同）。
#
# 用法：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/start-csharp.ps1
#   powershell ... -Seed 42 -RngLog E:/tmp/window/run3/login_only/csharp/rng-stream.tsv
#   powershell ... -Plan            # 只打印将执行的命令/env，不启动
#   powershell ... -Stop            # 按 pid 文件停掉本栈
#
# 退出码：0 = 全部角色端口就绪（或 -Plan/-Stop 成功）；1 = 缺件/构建失败/端口未就绪；2 = 参数错误。
param(
  [string]$Root = 'E:\tmp\conformance\csharp-root',
  [string]$LogDir = '',
  [string]$Seed = '42',
  [string]$RngLog = '',
  [string]$HookDll = '',
  [string]$Shadow = '',
  [int]$WaitSeconds = 180,
  [switch]$SkipHookDeploy,
  [switch]$Plan,
  [switch]$Stop
)
$ErrorActionPreference = 'Stop'

$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Repo = (Resolve-Path (Join-Path $Here '..\..')).Path
if ([string]::IsNullOrWhiteSpace($LogDir)) { $LogDir = Join-Path $Root 'logs' }
if ([string]::IsNullOrWhiteSpace($Shadow)) { $Shadow = Join-Path $Root 'M2GameSvr-hook' }
$PidFile = Join-Path $LogDir 'start-csharp.pid.json'
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Test-Port([int]$Port) {
  return [bool](Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)
}

function Log-Step([string]$m) {
  Add-Content -LiteralPath (Join-Path $LogDir 'start-csharp.step.log') -Value ((Get-Date).ToString('HH:mm:ss') + ' ' + $m)
}

# 六角色（端口 = csharp-root 的 alt 私有栈；顺序 = 启动顺序；GameSvr 走影子目录）
$components = @(
  [pscustomobject]@{ Name='DBSrv';     Dir=(Join-Path $Root 'DBSrv');     Exe='DBSrv.exe';     Ports=@(16000,15100,15700) },
  [pscustomobject]@{ Name='LoginSrv';  Dir=(Join-Path $Root 'LoginSrv');  Exe='LoginSrv.exe';  Ports=@(15500,15600) },
  [pscustomobject]@{ Name='GameSvr';   Dir=$Shadow;                       Exe='GameSrv.exe';   Ports=@(15000) },
  [pscustomobject]@{ Name='LoginGate'; Dir=(Join-Path $Root 'LoginGate'); Exe='LoginGate.exe'; Ports=@(17000) },
  [pscustomobject]@{ Name='SelGate';   Dir=(Join-Path $Root 'SelGate');   Exe='SelGate.exe';   Ports=@(17100) },
  [pscustomobject]@{ Name='RunGate';   Dir=(Join-Path $Root 'RunGate');   Exe='GameGate.exe';  Ports=@(17200) }
)
$allPorts = @(15000,15100,15500,15600,15700,16000,17000,17100,17200)

if ($Stop) {
  if (Test-Path -LiteralPath $PidFile) {
    $items = @(Get-Content -LiteralPath $PidFile -Raw | ConvertFrom-Json)
    if ($items.Count -eq 1 -and ($items[0] -is [System.Array])) { $items = @($items[0]) }
    foreach ($e in $items) {
      $p = Get-Process -Id $e.pid -ErrorAction SilentlyContinue
      if ($p) { Stop-Process -Id $e.pid -Force -ErrorAction SilentlyContinue; Write-Output ("STOPPED {0} pid={1}" -f $e.name, $e.pid) }
      else { Write-Output ("ALREADY_GONE {0} pid={1}" -f $e.name, $e.pid) }
    }
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
  } else { Write-Output 'NOTHING_TO_STOP' }
  exit 0
}

# RNG 口径：显式 -RngLog > 环境 MIR2_RNG_LOG > 不落流水（不部署钩子、不注种子）。
$rngLogText = if (-not [string]::IsNullOrWhiteSpace($RngLog)) { $RngLog.Trim() }
              elseif (-not [string]::IsNullOrWhiteSpace($env:MIR2_RNG_LOG)) { $env:MIR2_RNG_LOG.Trim() }
              else { '' }
if ($rngLogText) {
  $rngParent = Split-Path -Parent $rngLogText
  if ([string]::IsNullOrWhiteSpace($rngParent) -or -not (Test-Path -LiteralPath $rngParent -PathType Container)) {
    Write-Output ("RED: -RngLog 的父目录不存在：{0}（先建证据目录，fail-closed）" -f $rngLogText)
    exit 2
  }
  $seedValue = 0
  if (-not [int]::TryParse($Seed, [ref]$seedValue)) {
    Write-Output ("RED: -Seed 不是合法 int：{0}（RNG 流水要求同种子）" -f $Seed)
    exit 2
  }
}

# 忙检（-Plan 也检，便于开窗前自查）
$busy = @($allPorts | Where-Object { Test-Port ([int]$_) })
if ($busy.Count -gt 0) {
  Write-Output ("RED: 端口已被监听 " + ($busy -join ','))
  exit 1
}

# ---- 影子副本 + 钩子部署（仅在需要 RNG 流水时）----
$hookPath = ''
if ($rngLogText) {
  $shadowGs = Join-Path $Shadow 'GameSvr.exe'
  if (-not (Test-Path -LiteralPath $shadowGs)) {
    $src = Join-Path $Root 'M2GameSvr'
    if (-not (Test-Path -LiteralPath (Join-Path $src 'GameSrv.exe'))) {
      Write-Output ("RED: 缺 GameSrv.exe：{0}" -f $src); exit 1
    }
    Write-Output ("SHADOW_COPY {0} -> {1}" -f $src, $Shadow)
    if (-not $Plan) {
      New-Item -ItemType Directory -Force -Path $Shadow | Out-Null
      & robocopy $src $Shadow /E /NFL /NDL /NJH /NJS /NP | Out-Null
      if ($LASTEXITCODE -gt 7) { Write-Output ("RED: robocopy 失败 rc={0}" -f $LASTEXITCODE); exit 1 }
      $rcPath = Join-Path $Shadow 'GameSrv.runtimeconfig.json'
      $rcObj = Get-Content -LiteralPath $rcPath -Raw | ConvertFrom-Json
      $rcObj.runtimeOptions.configProperties.'System.StartupHookProvider.IsSupported' = $true
      $json = $rcObj | ConvertTo-Json -Depth 20
      [System.IO.File]::WriteAllText($rcPath, $json, (New-Object System.Text.UTF8Encoding($false)))
    }
  }
  if (-not [string]::IsNullOrWhiteSpace($HookDll)) {
    $hookPath = $HookDll
  } else {
    if (-not $SkipHookDeploy -and -not $Plan) {
      $hookProj = Join-Path $Repo 'mir2-rs\tests\parity\m3\rng-hook\RngSeedHook.csproj'
      & dotnet build $hookProj -c Release --nologo -v q | Out-Null
      if ($LASTEXITCODE -ne 0) { Write-Output 'RED: RngSeedHook 构建失败'; exit 1 }
    }
    $hookPath = Join-Path $Repo 'mir2-rs\tests\parity\m3\rng-hook\bin\Release\net8.0\RngSeedHook.dll'
  }
  if (-not $Plan) {
    if (-not (Test-Path -LiteralPath $hookPath)) { Write-Output ("RED: 找不到 hook dll：{0}" -f $hookPath); exit 1 }
    Copy-Item -Force $hookPath (Join-Path $Shadow 'RngSeedHook.dll')
    $hookPath = Join-Path $Shadow 'RngSeedHook.dll'
    Write-Output ("HOOK_DEPLOYED {0}" -f $hookPath)
  }
}

Write-Output ("CSHARP_ROOT {0}" -f $Root)
Write-Output ("CSHARP_LOG_DIR {0}" -f $LogDir)
Write-Output ("SEED: 将向 GameSvr 注入 MIR2_RNG_SEED={0}" -f $Seed)
if ($rngLogText) {
  Write-Output ("RNG_LOG: 将向 GameSvr 注入 MIR2_RNG_LOG={0} + MIR2_RNG_SITE=1 + DOTNET_STARTUP_HOOKS={1}" -f $rngLogText, $hookPath)
} else {
  Write-Output 'RNG_LOG: (none；不部署钩子、不注种子——oracle 行为与旧 start-csharp3.ps1 相同)'
}

if ($Plan) {
  foreach ($c in $components) {
    Write-Output ("PLAN {0} exe={1} ports={2}" -f $c.Name, (Join-Path $c.Dir $c.Exe), (($c.Ports) -join ','))
  }
  exit 0
}

$started = @()
function Start-One($Name, $Dir, $Exe, $Ports) {
  $exe = Join-Path $Dir $Exe
  if (-not (Test-Path -LiteralPath $exe)) { Write-Output ("RED: 缺件 " + $exe); Log-Step ('RED missing ' + $exe); exit 1 }
  $out = Join-Path $LogDir ($Name + '.out.log')
  $err = Join-Path $LogDir ($Name + '.err.log')
  $p = Start-Process -FilePath $exe -WorkingDirectory $Dir -RedirectStandardOutput $out -RedirectStandardError $err -WindowStyle Hidden -PassThru
  $script:started += [pscustomobject]@{ name=$Name; pid=$p.Id; exe=$exe; ports=$Ports }
  Log-Step ('START ' + $Name + ' pid=' + $p.Id)
  Write-Output ("START {0} pid={1} ports={2} exe={3}" -f $Name, $p.Id, (($Ports) -join ','), $exe)
  Start-Sleep -Milliseconds 400
}

Log-Step 'BEGIN roles'
foreach ($c in $components) {
  if ($c.Name -eq 'GameSvr' -and $rngLogText) {
    $env:DOTNET_STARTUP_HOOKS = $hookPath
    $env:MIR2_RNG_SEED = $Seed
    $env:MIR2_RNG_LOG = $rngLogText
    $env:MIR2_RNG_SITE = '1'
    try { Start-One $c.Name $c.Dir $c.Exe $c.Ports }
    finally {
      Remove-Item Env:DOTNET_STARTUP_HOOKS,Env:MIR2_RNG_SEED,Env:MIR2_RNG_LOG,Env:MIR2_RNG_SITE -ErrorAction SilentlyContinue
    }
  } else {
    Start-One $c.Name $c.Dir $c.Exe $c.Ports
  }
}

ConvertTo-Json -InputObject @($started) -Depth 5 | Set-Content -LiteralPath $PidFile -Encoding utf8
Write-Output ("PIDFILE " + $PidFile)

$deadline = (Get-Date).AddSeconds($WaitSeconds)
while ((Get-Date) -lt $deadline) {
  $up = @($allPorts | Where-Object { Test-Port ([int]$_) })
  if ($up.Count -eq $allPorts.Count) { break }
  Start-Sleep -Seconds 2
}
$missing = @($allPorts | Where-Object { -not (Test-Port ([int]$_)) })
Log-Step ('FINAL missing=' + ($missing -join ','))
if ($missing.Count -gt 0) { Write-Output ("RED: 端口未就绪 " + ($missing -join ',')); exit 1 }
Write-Output 'CSHARP_STACK_GREEN'
exit 0
