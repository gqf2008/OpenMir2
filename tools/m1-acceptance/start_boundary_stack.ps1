#requires -Version 5
<#
.SYNOPSIS
  start_boundary_stack.ps1 — 起一条**全 Rust** 的 M1 边界链（M1⑤①）。

.DESCRIPTION
  进程构成（端口以 `mir2-rs/docs/M1-边界服务移植计划.md` §1 链路表为准）：

    MySQL       3306  （本机 MariaDB；已在跑则复用，本脚本不重复起）
    login-srv   5500  （LoginGate → 本进程）+ 5600（DBSrv → 本进程，会话同步）  **Rust** `bins/login-srv`
    db-srv      5100  （SelGate → 本进程，UserService 面）                      **Rust** `bins/db-srv`
    login-gate  7000  （客户端 → 本进程 → 5500）                                **Rust** `bins/login-gate`
    sel-gate    7100  （客户端 → 本进程 → 5100）                                **Rust** `bins/sel-gate`

  **不含 6000**：那是 DBSrv 的 `DataService` 面（GameSvr 取档），Rust 侧还没实现，留给卡
  `openmir2-m1-dbsrv-6000-face`。本卡只验「登录 + 选角」，不照 6000 起栈。

  **不碰 `E:\MirServer`**：所有配置写到 `-StackRoot` 的影子副本里（照 `tools/m2-acceptance/start_rust_stack.ps1`
  的做法）。其中 GB2312 的 `AddrTable.txt`/`UserLimit.txt` **整体复制**（服务器名是 GBK 字节，不能重建）。

  判据/退出码：
    `-Stop`   按 pid 文件停掉本脚本起的进程（**不碰 MySQL**）；缺 pid 文件 ⇒ `NOTHING_TO_STOP`。
    `-Status` 只按端口实况打印，不改状态。
    起栈：任一步端口未就绪 ⇒ 打印 `RED` 并 exit 1；缺 exe/配置 ⇒ `PREREQ_MISSING` 并 exit 2；
          启动前若目标端口已被监听 ⇒ exit 1（**不静默混实现**）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m1-acceptance/start_boundary_stack.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m1-acceptance/start_boundary_stack.ps1 -Stop
#>
param(
  [string]$MirServer  = 'E:\MirServer',
  # C# 构建输出所在检出（DBSrv/LoginSrv 的 bin/Release）。默认主检出——本 worktree 没有 bin 产物。
  [string]$CsharpRepo = 'E:\Users\gxh\Documents\GitHub\OpenMir2',
  [string]$RustBinDir = 'E:\tmp\openmir2-m1load-target\debug',
  [string]$StackRoot  = 'E:\tmp\openmir2-m1load\stack',
  [string]$LogDir     = 'E:\tmp\openmir2-m1load\stack\logs',
  [int]$WaitSeconds   = 60,
  [switch]$Stop,
  [switch]$Status,
  [switch]$Force
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..')).Path
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$PidFile = Join-Path $LogDir 'boundary-stack.pid.json'
$Ports = @(7000, 5500, 7100, 5100, 5600)

function Test-Port([int]$Port) {
  foreach ($ep in [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()) {
    if ($ep.Port -eq $Port) { return $true }
  }
  return $false
}

function Get-ListenPids {
  $map = @{}
  foreach ($line in (& netstat -ano 2>$null)) {
    if ($line -match '^\s*TCP\s+\S+:(\d+)\s+\S+\s+LISTENING\s+(\d+)\s*$') {
      $p = [int]$Matches[1]; $procId = [int]$Matches[2]
      if (-not $map.ContainsKey($p)) { $map[$p] = $procId }
    }
  }
  return $map
}

function Get-Owner([int]$Port) {
  $map = Get-ListenPids
  if (-not $map.ContainsKey($Port)) { return $null }
  return (Get-CimInstance Win32_Process -Filter ("ProcessId=$($map[$Port])") -ErrorAction SilentlyContinue)
}

function Show-Status {
  Write-Output '=== M1 全 Rust 边界链 端口实况 ==='
  foreach ($p in $Ports) {
    $o = Get-Owner $p
    if ($o) { Write-Output ("  {0,-5} <- pid={1} {2}  {3}" -f $p, $o.ProcessId, $o.Name, $o.ExecutablePath) }
    else    { Write-Output ("  {0,-5} DOWN" -f $p) }
  }
}

if ($Stop) {
  if (-not (Test-Path $PidFile)) { Write-Output 'NOTHING_TO_STOP (无 pid 文件)'; exit 0 }
  $saved = Get-Content $PidFile -Raw | ConvertFrom-Json
  $entries = @($saved)
  if ($entries.Count -eq 1 -and ($entries[0].PSObject.Properties.Name -contains 'procs')) { $entries = @($entries[0].procs) }
  # 逆启动序停：网关 → db-srv → login-srv（MySQL 不动）
  foreach ($g in ($entries | Sort-Object { [int]$_.stop_order })) {
    if (-not $g -or $null -eq $g.pid) { continue }
    $p = Get-Process -Id $g.pid -ErrorAction SilentlyContinue
    if ($p) { Stop-Process -Id $g.pid -Force; Write-Output ("STOPPED {0} pid={1}" -f $g.name, $g.pid) }
    else { Write-Output ("ALREADY_GONE {0} pid={1}" -f $g.name, $g.pid) }
  }
  Start-Sleep -Seconds 2
  Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
  Show-Status
  exit 0
}

if ($Status) { Show-Status; exit 0 }

# ---- 0) 前置：端口必须空（不静默混实现）----
if (-not $Force) {
  $busy = @($Ports | Where-Object { Test-Port $_ })
  if ($busy.Count -gt 0) {
    Write-Output ("RED: 目标端口已被监听 {0} —— 先停掉现有栈（tools/m1-acceptance/start_boundary_stack.ps1 -Stop 或 C# 栈的 start-all.ps1 -Stop）再起" -f ($busy -join ','))
    Show-Status
    exit 1
  }
}

# ---- 1) 生成影子配置（不碰 E:\MirServer）----
# LoginSrv 的部署配置（logsrv.conf / AddrTable.txt）从 C# 构建输出目录取；缺则回落 E:\MirServer\LoginSrv。
$srcLoginSrv = Join-Path $CsharpRepo 'src\LoginSrv\bin\Release'
if (-not (Test-Path (Join-Path $srcLoginSrv 'logsrv.conf'))) { $srcLoginSrv = Join-Path $MirServer 'LoginSrv' }
$lgsDir = Join-Path $StackRoot 'login-gate'
$sgsDir = Join-Path $StackRoot 'sel-gate'
$lsvDir = Join-Path $StackRoot 'login-srv'
$dbsDir = Join-Path $StackRoot 'db-srv'
foreach ($d in @($lgsDir, $sgsDir, $lsvDir, $dbsDir)) { New-Item -ItemType Directory -Force -Path $d | Out-Null }

$missing = @()
$need = @(
  (Join-Path $RustBinDir 'login-srv.exe'), (Join-Path $RustBinDir 'db-srv.exe'),
  (Join-Path $RustBinDir 'login-gate.exe'), (Join-Path $RustBinDir 'sel-gate.exe'),
  (Join-Path $MirServer 'LoginGate\config.conf'), (Join-Path $MirServer 'SelGate\config.conf'),
  (Join-Path $srcLoginSrv 'logsrv.conf'), (Join-Path $srcLoginSrv 'AddrTable.txt'),
  (Join-Path $MirServer 'LoginSrv\UserLimit.txt')
)
foreach ($p in $need) { if (-not (Test-Path $p)) { $missing += $p } }
if ($missing.Count -gt 0) {
  Write-Output 'PREREQ_MISSING: 缺以下 exe/配置：'
  $missing | ForEach-Object { Write-Output ("  " + $_) }
  exit 2
}

# login-gate：现网 config.conf（ServerPort0=5500 / GatePort0=7000），GBK 整体复制
Copy-Item (Join-Path $MirServer 'LoginGate\config.conf') (Join-Path $lgsDir 'config.conf') -Force
# sel-gate：现网 config.conf（ServerPort0=5100 / GatePort0=7100），GBK 整体复制
Copy-Item (Join-Path $MirServer 'SelGate\config.conf')  (Join-Path $sgsDir 'config.conf')  -Force
# login-srv：logsrv.conf + AddrTable.txt（GBK，含 127.0.0.1:7100 路由）+ UserLimit.txt
Copy-Item (Join-Path $srcLoginSrv 'logsrv.conf')    (Join-Path $lsvDir 'logsrv.conf')    -Force
Copy-Item (Join-Path $srcLoginSrv 'AddrTable.txt')  (Join-Path $lsvDir 'AddrTable.txt')  -Force
Copy-Item (Join-Path $MirServer 'LoginSrv\UserLimit.txt') (Join-Path $lsvDir 'UserLimit.txt') -Force
# ServerAddr.txt：5600 允许接入的 IP 白名单（db-srv 从 127.0.0.1 连）
[System.IO.File]::WriteAllText((Join-Path $lsvDir 'ServerAddr.txt'), "127.0.0.1`r`n", (New-Object System.Text.ASCIIEncoding))

# db-srv：dbsvr.conf（ShowDebug=1 以便取「选角成功」日志）+ AddrTable/ServerInfo/SelectID
$dbsConf = @"
[Setup]
ShowDebug=1
ServerAddr=127.0.0.1
ServerPort=6000
GateAddr=0.0.0.0
GatePort=5100
ServerName=LegendOfMir2
DenyChrName=1
DELMaxLevel=30
DynamicIPMode=0
MapFile=

[Server]
IDSAddr=127.0.0.1
IDSPort=5600
MarketAddr=127.0.0.1
MarketPort=5700
PushMarketInterval=30000

[DataBase]
Storeage=MySQL
ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_db;

[DBClear]
Interval=3000
"@
[System.IO.File]::WriteAllText((Join-Path $dbsDir 'dbsvr.conf'), $dbsConf, (New-Object System.Text.ASCIIEncoding))
# AddrTable.txt：5100 允许接入的角色网关 IP
[System.IO.File]::WriteAllText((Join-Path $dbsDir 'AddrTable.txt'), "127.0.0.1`r`n", (New-Object System.Text.ASCIIEncoding))
# ServerInfo.txt：角色网关 IP → 游戏网关路由（本地 127.0.0.1:7200）
[System.IO.File]::WriteAllText((Join-Path $dbsDir 'ServerInfo.txt'), "127.0.0.1 127.0.0.1 7200`r`n", (New-Object System.Text.ASCIIEncoding))
# SelectID.txt：IP → 网关 ID
[System.IO.File]::WriteAllText((Join-Path $dbsDir 'SelectID.txt'), "127.0.0.1 1`r`n", (New-Object System.Text.ASCIIEncoding))
$denySrc = Join-Path $MirServer 'DBServer\DenyChrName.txt'
if (Test-Path $denySrc) { Copy-Item $denySrc (Join-Path $dbsDir 'DenyChrName.txt') -Force }
else { [System.IO.File]::WriteAllText((Join-Path $dbsDir 'DenyChrName.txt'), "", (New-Object System.Text.ASCIIEncoding)) }

# ---- 2) MySQL（已在跑则复用）----
if (-not (Test-Port 3306)) {
  $mysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe'
  $mysqlData = 'D:\mysql\data'
  if (-not (Test-Path $mysqlExe)) { Write-Output ("PREREQ_MISSING: " + $mysqlExe); exit 2 }
  Write-Output 'START MySQL'
  Start-Process -FilePath $mysqlExe -ArgumentList @("--datadir=$mysqlData", '--port=3306', '--bind-address=127.0.0.1', '--console') `
    -WorkingDirectory 'D:\mysql\mariadb-10.11.19-winx64' -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $LogDir 'MySQL.out.log') -RedirectStandardError (Join-Path $LogDir 'MySQL.err.log') | Out-Null
} else { Write-Output 'MYSQL already up (3306)' }

$started = @()
function Start-One([string]$Name, [string]$Exe, [string]$ConfPath, [int[]]$Ports, [int]$StopOrder) {
  $out = Join-Path $LogDir ($Name + '.out.log')
  $err = Join-Path $LogDir ($Name + '.err.log')
  Remove-Item $out, $err -Force -ErrorAction SilentlyContinue
  $sp = @{ FilePath = $Exe; WorkingDirectory = (Split-Path $ConfPath -Parent); WindowStyle = 'Hidden'; PassThru = $true
           RedirectStandardOutput = $out; RedirectStandardError = $err; ArgumentList = @($ConfPath) }
  $p = Start-Process @sp
  $script:started += [pscustomobject]@{ name = $Name; pid = $p.Id; ports = $Ports; stop_order = $StopOrder; exe = $Exe }
  Write-Output ("START {0} pid={1} ports={2}" -f $Name, $p.Id, ($Ports -join ','))
}

# 启动顺序：login-srv(5500/5600) → db-srv(5100) → login-gate(7000) → sel-gate(7100)
# （网关启动时就建下游连接，所以下游先起）
Start-One -Name 'login-srv'  -Exe (Join-Path $RustBinDir 'login-srv.exe')  -ConfPath (Join-Path $lsvDir 'logsrv.conf')   -Ports @(5500, 5600) -StopOrder 4
Start-One -Name 'db-srv'     -Exe (Join-Path $RustBinDir 'db-srv.exe')     -ConfPath (Join-Path $dbsDir 'dbsvr.conf')    -Ports @(5100)        -StopOrder 3
Start-One -Name 'login-gate' -Exe (Join-Path $RustBinDir 'login-gate.exe') -ConfPath (Join-Path $lgsDir 'config.conf')   -Ports @(7000)        -StopOrder 1
Start-One -Name 'sel-gate'   -Exe (Join-Path $RustBinDir 'sel-gate.exe')   -ConfPath (Join-Path $sgsDir 'config.conf')   -Ports @(7100)        -StopOrder 2

$started | ConvertTo-Json -Depth 5 | Set-Content -Path $PidFile -Encoding utf8
Write-Output ("PIDFILE " + $PidFile)

function Wait-Ports([int[]]$Want) {
  $deadline = (Get-Date).AddSeconds($WaitSeconds)
  while ((Get-Date) -lt $deadline) {
    if (@($Want | Where-Object { Test-Port $_ }).Count -eq $Want.Count) { return $true }
    Start-Sleep -Milliseconds 500
  }
  return (@($Want | Where-Object { Test-Port $_ }).Count -eq $Want.Count)
}

Start-Sleep -Seconds 3
$expect = @(
  @{ name = 'login-srv'; ports = @(5500, 5600) }
  @{ name = 'db-srv'; ports = @(5100) }
  @{ name = 'login-gate'; ports = @(7000) }
  @{ name = 'sel-gate'; ports = @(7100) }
)
$fail = @()
foreach ($e in $expect) {
  if (-not (Wait-Ports $e.ports)) {
    $miss = @($e.ports | Where-Object { -not (Test-Port $_) })
    $fail += ("{0} 缺 {1}" -f $e.name, ($miss -join ','))
    Write-Output ("RED: {0} 端口未就绪（{1}）看 {2}" -f $e.name, ($miss -join ','), (Join-Path $LogDir ($e.name + '.err.log')))
  } else {
    Write-Output ("CORE_UP {0} ports={1}" -f $e.name, ($e.ports -join ','))
  }
}

Show-Status
if ($fail.Count -gt 0) {
  Write-Output ("RED: 边界链未就绪：" + ($fail -join '; '))
  exit 1
}
Write-Output ('BOUNDARY_STACK_OK ' + $PidFile)
exit 0
