#requires -Version 5
<#
.SYNOPSIS
  stop_mir2_server.ps1 — 停止 start_mir2_server.ps1 起的单二进制六角色栈。

.DESCRIPTION
  读 `<LogDir>\mir2-server.pid.json`，按其中每条记录的 `stop_order` **降序**停止
  （即启动顺序的严格反向：game-gate -> sel-gate -> login-gate -> game-svr -> login-srv -> db-srv）。
  game-svr 先走 `\\.\pipe\openmir2-gamesvr-shutdown-<pid>` 优雅关服（存档），8s 内未退则 Force；
  其它角色直接 Force。全部停完后校验记录的端口是否 DOWN：
      - 全 DOWN  => 删除 pid 文件，输出 `MIR2_SERVER_STACK_STOPPED`，退出 0；
      - 有端口仍 UP => RED + 保留 pid 文件，退出 1（fail-closed，不静默成功）。
  缺 pid 文件 / 空文件 / 空数组 => 优雅提示 `NOTHING_TO_STOP`，退出 0。
  JSON 损坏 => RED + 退出 2（不猜、不静默）。

  安全：只杀进程名与记录一致（`mir2-server` 或 -ServerExe 的 basename）的进程，
  进程名不符时打印 SKIP（防止 pid 复用误杀别人的进程）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/stop_mir2_server.ps1

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/stop_mir2_server.ps1 -Root E:\MirServer
#>
param(
  [string]$Root = 'E:\MirServer',
  [string]$LogDir = '',
  [string]$PidFile = '',
  [int]$ProcessTimeoutSeconds = 8,
  [int]$PortTimeoutSeconds = 10
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($LogDir)) {
  $LogDir = Join-Path $Root 'logs\mir2-server'
}
if ([string]::IsNullOrWhiteSpace($PidFile)) {
  $PidFile = Join-Path $LogDir 'mir2-server.pid.json'
}

function Test-Port([int]$Port) {
  foreach ($ep in [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()) {
    if ($ep.Port -eq $Port) { return $true }
  }
  return $false
}

function Wait-PortsDown {
  param([int[]]$Ports, [int]$Seconds)
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    if (@($Ports | Where-Object { Test-Port ([int]$_) }).Count -eq 0) { return $true }
    Start-Sleep -Milliseconds 300
  }
  return (@($Ports | Where-Object { Test-Port ([int]$_) }).Count -eq 0)
}

function Stop-Gracefully {
  # game-svr 的进程级优雅关服通道；其它角色没有 => 返回 $false（调用方 Force）。
  param([object]$Entry, [object]$Proc)
  if ($Entry.role -ne 'game-svr') { return $false }
  try {
    $client = [System.IO.Pipes.NamedPipeClientStream]::new(
      '.', ("openmir2-gamesvr-shutdown-{0}" -f $Entry.pid), [System.IO.Pipes.PipeDirection]::Out)
    $client.Connect(1000)
    $bytes = [System.Text.Encoding]::ASCII.GetBytes('SHUTDOWN')
    $client.Write($bytes, 0, $bytes.Length)
    $client.Flush()
    $client.Dispose()
  } catch {
    return $false
  }
  $null = $Proc.WaitForExit($script:ProcessTimeoutSeconds * 1000)
  return -not [bool](Get-Process -Id $Entry.pid -ErrorAction SilentlyContinue)
}

function Get-ExpectedProcessNames {
  param([object]$Entry)
  $names = @('mir2-server')
  if ($Entry -and $Entry.exe) {
    try { $names += [System.IO.Path]::GetFileNameWithoutExtension([string]$Entry.exe) } catch { }
  }
  return @($names | Where-Object { $_ } | Select-Object -Unique)
}

# ---- 缺文件 / 空文件 / 空数组：优雅提示，退出 0。----
if (-not (Test-Path -LiteralPath $PidFile -PathType Leaf)) {
  Write-Output ("NOTHING_TO_STOP（无 pid 文件：{0}）" -f $PidFile)
  exit 0
}

$raw = Get-Content -LiteralPath $PidFile -Raw -ErrorAction Stop
if ([string]::IsNullOrWhiteSpace($raw)) {
  Write-Output ("NOTHING_TO_STOP（pid 文件为空：{0}）" -f $PidFile)
  Remove-Item -LiteralPath $PidFile -Force -ErrorAction SilentlyContinue
  exit 0
}

try {
  $parsed = $raw | ConvertFrom-Json
} catch {
  Write-Output ("RED: pid 文件不是合法 JSON：{0}（{1}）—— 不猜、不静默" -f $PidFile, $_.Exception.Message)
  exit 2
}

$entries = @()
if ($null -ne $parsed) {
  if ($parsed.PSObject.Properties.Name -contains 'roles') { $entries = @($parsed.roles) }
  elseif ($parsed -is [array]) { $entries = @($parsed) }
  else { $entries = @($parsed) }
}
$entries = @($entries | Where-Object { $_ -and ($_.PSObject.Properties.Name -contains 'role') -and ($_.PSObject.Properties.Name -contains 'pid') })
if ($entries.Count -eq 0) {
  Write-Output ("NOTHING_TO_STOP（pid 文件无角色条目：{0}）" -f $PidFile)
  Remove-Item -LiteralPath $PidFile -Force -ErrorAction SilentlyContinue
  exit 0
}

Write-Output ("PIDFILE {0}（{1} 个角色，停止顺序 = stop_order 降序）" -f $PidFile, $entries.Count)

# ---- 按 stop_order 降序停止。----
foreach ($e in @($entries | Sort-Object { if ($null -ne $_.stop_order) { [int]$_.stop_order } else { 0 } } -Descending)) {
  $procId = 0
  if (-not [int]::TryParse([string]$e.pid, [ref]$procId)) {
    Write-Output ("SKIP {0} pid 非法：{1}" -f $e.role, $e.pid)
    continue
  }
  $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
  if (-not $p) {
    Write-Output ("ALREADY_GONE {0} pid={1}" -f $e.role, $procId)
    continue
  }
  $expected = Get-ExpectedProcessNames $e
  if ($expected -notcontains $p.ProcessName) {
    Write-Output ("SKIP {0} pid={1} 进程名={2} 与记录（{3}）不符 —— 防 pid 复用误杀，不结束" -f $e.role, $procId, $p.ProcessName, ($expected -join '|'))
    continue
  }
  if (Stop-Gracefully $e $p) {
    Write-Output ("STOPPED {0} pid={1} graceful" -f $e.role, $procId)
    continue
  }
  Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
  $null = $p.WaitForExit($script:ProcessTimeoutSeconds * 1000)
  if (Get-Process -Id $procId -ErrorAction SilentlyContinue) {
    Write-Output ("RED: {0} pid={1} Force 后仍存活" -f $e.role, $procId)
  } else {
    Write-Output ("STOPPED {0} pid={1} force" -f $e.role, $procId)
  }
}

# ---- 校验端口 DOWN（fail-closed）。----
$stillUp = @()
foreach ($e in $entries) {
  $ports = @()
  if ($e.PSObject.Properties.Name -contains 'ports' -and $null -ne $e.ports) {
    $ports = @($e.ports | ForEach-Object { [int]$_ })
  }
  if ($ports.Count -eq 0) { continue }
  if (-not (Wait-PortsDown -Ports $ports -Seconds $PortTimeoutSeconds)) {
    $up = @($ports | Where-Object { Test-Port ([int]$_) })
    $stillUp += ("{0}:{1}" -f $e.role, ($up -join ','))
  } else {
    Write-Output ("PORT_DOWN {0} ports={1}" -f $e.role, ($ports -join ','))
  }
}

if ($stillUp.Count -gt 0) {
  Write-Output ("RED: 停止后端口仍 UP：{0} —— 保留 pid 文件 {1}，请人工核查" -f ($stillUp -join '; '), $PidFile)
  exit 1
}

Remove-Item -LiteralPath $PidFile -Force -ErrorAction SilentlyContinue
Write-Output ('MIR2_SERVER_STACK_STOPPED ' + $PidFile)
exit 0
