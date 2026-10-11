#requires -Version 5
<#
.SYNOPSIS
  start_rust_gates.ps1 — 把 7000/7100/7200 三个入口交给 Rust 实现（`run_m3_gate.ps1 -Side rust` 的前置）。

.DESCRIPTION
  `-Side rust` 的入口护栏只认「7000/7100/7200 的监听进程不在 E:\MirServer 下」；本脚本是这条前置的
  **可复跑** 落地：

    1) 预检：7000/7100/7200 若已被监听 ⇒ 立刻失败退出（不静默混实现）；
    2) 起本仓库构建出的 Rust 三网关，配置直接复用现网 `E:\MirServer\{LoginGate,SelGate,RunGate}\config.conf`
       （键名与端口即现网事实，不改内容）；
    3) 等三个端口就绪；PID 落 `sessions/rust-gates.pid.json`，日志落 `sessions/rust-gates/*.log`。

  下游（MySQL/DBSrv/LoginSrv/GameSvr）**不动**：本脚本只替换入口，这正是 M3「Rust 顶住三端口、
  核心仍是 C# oracle」的形态。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File start_rust_gates.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File start_rust_gates.ps1 -Stop   # 按 PID 停掉本脚本起的三个进程
#>
param(
  [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')).Path,
  [string]$MirServer = 'E:\MirServer',
  [ValidateSet('debug', 'release')][string]$Profile = 'debug',
  [int]$WaitSeconds = 30,
  [switch]$Stop
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$BinDir = Join-Path $RepoRoot ("mir2-rs\target\{0}" -f $Profile)
$SessionDir = Join-Path $Here 'sessions'
$LogDir = Join-Path $SessionDir 'rust-gates'
$PidFile = Join-Path $SessionDir 'rust-gates.pid.json'
$Ports = @(7000, 7100, 7200)
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Test-Port([int]$Port) {
  return [bool](Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)
}

# 三网关：名字 / 可执行 / 现网配置
$Gates = @(
  [pscustomobject]@{ Name = 'login-gate'; Exe = 'login-gate.exe'; Conf = (Join-Path $MirServer 'LoginGate\config.conf'); Port = 7000 }
  [pscustomobject]@{ Name = 'sel-gate';   Exe = 'sel-gate.exe';   Conf = (Join-Path $MirServer 'SelGate\config.conf');   Port = 7100 }
  [pscustomobject]@{ Name = 'game-gate';  Exe = 'game-gate.exe';  Conf = (Join-Path $MirServer 'RunGate\config.conf');   Port = 7200 }
)

if ($Stop) {
  if (-not (Test-Path $PidFile)) { Write-Output 'NOTHING_TO_STOP (无 pid 文件)'; return }
  $saved = Get-Content $PidFile -Raw | ConvertFrom-Json
  # 本脚本写出的 pid 文件是**顶层数组**（`$started | ConvertTo-Json`），不是 `{gates:[…]}`；
  # 原实现取 `$saved.gates` 恒为 $null，`-Stop` 停不掉任何进程（收工停栈实测踩到）。
  # 两种形态都兼容：数组直接用；对象带 `gates` 则取其下的数组。
  $entries = @($saved)
  if ($entries.Count -eq 1 -and ($entries[0].PSObject.Properties.Name -contains 'gates')) {
    $entries = @($entries[0].gates)
  }
  foreach ($g in $entries) {
    if (-not $g -or $null -eq $g.pid) { continue }
    $p = Get-Process -Id $g.pid -ErrorAction SilentlyContinue
    if ($p) { Stop-Process -Id $g.pid -Force; Write-Output ("STOPPED {0} pid={1}" -f $g.name, $g.pid) }
    else { Write-Output ("ALREADY_GONE {0} pid={1}" -f $g.name, $g.pid) }
  }
  Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
  return
}

# ---- 0) 预检：端口必须空着（否则会与别人抢入口 / 混实现） ----
$busy = @($Ports | Where-Object { Test-Port $_ })
if ($busy.Count -gt 0) {
  Write-Output ("RED: 入口端口已被监听 {0} —— 先停掉现网关（C# 用 E:\MirServer\start-all.ps1 -Stop 的第 3 步，或先跑 -Stop）" -f ($busy -join ','))
  exit 1
}

# ---- 1) 起 Rust 三网关 ----
$started = @()
foreach ($g in $Gates) {
  $exe = Join-Path $BinDir $g.Exe
  if (-not (Test-Path $exe)) { throw ("找不到 {0}（先在 mir2-rs 下 cargo build -p {1}）" -f $exe, $g.Name) }
  if (-not (Test-Path $g.Conf)) { throw ("找不到现网配置 {0}" -f $g.Conf) }
  $out = Join-Path $LogDir ($g.Name + '.out.log')
  $err = Join-Path $LogDir ($g.Name + '.err.log')
  $p = Start-Process -FilePath $exe -ArgumentList @($g.Conf) -WorkingDirectory $BinDir `
        -RedirectStandardOutput $out -RedirectStandardError $err -WindowStyle Hidden -PassThru
  $started += [pscustomobject]@{ name = $g.Name; pid = $p.Id; port = $g.Port; conf = $g.Conf }
  Write-Output ("START {0} pid={1} conf={2}" -f $g.Name, $p.Id, $g.Conf)
}

# ---- 2) 等端口就绪；缺一即失败（Rust 网关自身失败会非零退出，这里再看端口兜底） ----
$deadline = (Get-Date).AddSeconds($WaitSeconds)
while ((Get-Date) -lt $deadline) {
  if (@($Ports | Where-Object { Test-Port $_ }).Count -eq $Ports.Count) { break }
  Start-Sleep -Milliseconds 500
}
$missing = @($Ports | Where-Object { -not (Test-Port $_) })
$started | ConvertTo-Json -Depth 4 | Set-Content -Path $PidFile -Encoding utf8
if ($missing.Count -gt 0) {
  Write-Output ("RED: Rust 入口未全部就绪，缺监听 {0}（看 {1}\*.err.log）" -f ($missing -join ','), $LogDir)
  exit 1
}
foreach ($p in $Ports) {
  $conn = Get-NetTCPConnection -State Listen -LocalPort $p | Select-Object -First 1
  $proc = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue
  $impl = if ($proc -and $proc.Path -like 'E:\MirServer\*') { 'csharp' } else { 'rust-or-other' }
  Write-Output ("ENTRY {0} <- {1} [{2}] {3}" -f $p, $proc.ProcessName, $impl, $proc.Path)
}
Write-Output ('RUST_GATES_OK ' + $PidFile)
exit 0
