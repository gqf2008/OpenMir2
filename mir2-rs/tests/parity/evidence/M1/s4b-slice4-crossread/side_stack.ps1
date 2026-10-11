#requires -Version 5
<#
  side_stack.ps1 — M1③ 跨实现互读：把整栈起在一个指定的 DBSrv 实现上。
  栈构成（其余固定 = C# oracle）：
    C# GameSvr(5000) + <DBSrv>(5100/6000/5700) + C# LoginSrv(5500/5600)
    + C# LoginGate(7000) + C# SelGate(7100) + C# RunGate(7200) + MySQL(3306)
  用法： -Dbsrv rust|csharp  -Action start|stop|status
  只读 E:\MirServer（不写它的配置/数据）；Rust db-srv 的配置写在影子目录。
#>
param(
  [ValidateSet('rust','csharp')][string]$Dbsrv = 'rust',
  [ValidateSet('start','stop','status')][string]$Action = 'start'
)
$ErrorActionPreference = 'Stop'
$MirServer = 'E:\MirServer'
$CsharpRepo = 'E:\Users\gxh\Documents\GitHub\OpenMir2'
$StackRoot = 'E:\tmp\openmir2-dbsrv6k\stack'
$LogDir = Join-Path $StackRoot 'logs'
$RustBin = 'E:\tmp\openmir2-dbsrv6k-target\debug'
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$PidFile = Join-Path $StackRoot 'side-stack.pid.json'
$Ports = @(5000,5100,5500,5600,5700,6000,7000,7100,7200)

function Test-Port([int]$p) {
  foreach ($ep in [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()) { if ($ep.Port -eq $p) { return $true } }
  return $false
}
function Show-Status {
  Write-Output "=== ports ($Dbsrv side) ==="
  $map = @{}
  foreach ($line in (& netstat -ano 2>$null)) {
    if ($line -match '^\s*TCP\s+\S+:(\d+)\s+\S+\s+LISTENING\s+(\d+)\s*$') { $map[[int]$Matches[1]] = [int]$Matches[2] }
  }
  foreach ($p in $Ports) {
    if ($map.ContainsKey($p)) {
      $pr = Get-Process -Id $map[$p] -ErrorAction SilentlyContinue
      $nm = if ($pr) { $pr.ProcessName } else { '?' }
      Write-Output ("  {0,-5} pid={1} {2}" -f $p, $map[$p], $nm)
    } else { Write-Output ("  {0,-5} DOWN" -f $p) }
  }
}

function Stop-All {
  foreach ($n in @('LoginGate','SelGate','GameGate','GameSrv','LoginSrv','DBSrv','db-srv')) {
    Get-Process -Name $n -ErrorAction SilentlyContinue | ForEach-Object {
      try { Stop-Process -Id $_.Id -Force; Write-Output ("STOP {0} pid={1}" -f $n, $_.Id) } catch {}
    }
  }
  Start-Sleep -Seconds 2
}

if ($Action -eq 'status') { Show-Status; exit 0 }
if ($Action -eq 'stop') { Stop-All; Show-Status; exit 0 }

Stop-All

if (-not (Test-Port 3306)) {
  Start-Process -FilePath 'D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe' -ArgumentList '--datadir=D:\mysql\data','--port=3306','--bind-address=127.0.0.1','--console' -WorkingDirectory 'D:\mysql\mariadb-10.11.19-winx64' -WindowStyle Hidden -RedirectStandardOutput (Join-Path $LogDir 'mysql.out.log') -RedirectStandardError (Join-Path $LogDir 'mysql.err.log') | Out-Null
  Start-Sleep -Seconds 6
}
if (-not (Test-Port 3306)) { Write-Output 'RED: MySQL 3306 未就绪'; exit 1 }

# DBSrv
if ($Dbsrv -eq 'rust') {
  $exe = Join-Path $RustBin 'db-srv.exe'
  $dir = Join-Path $StackRoot 'db-srv'
  $args = @((Join-Path $dir 'dbsvr.conf'))
} else {
  $exe = Join-Path $CsharpRepo 'src\DBSrv\bin\Release\DBSrv.exe'
  $dir = Split-Path $exe -Parent
  $args = @()
}
if (-not (Test-Path $exe)) { Write-Output ("RED: 缺 exe " + $exe); exit 1 }
$sp = @{ FilePath = $exe; WorkingDirectory = $dir; WindowStyle = 'Hidden'; PassThru = $true;
         RedirectStandardOutput = (Join-Path $LogDir 'DBSrv.out.log'); RedirectStandardError = (Join-Path $LogDir 'DBSrv.err.log') }
if ($args.Count -gt 0) { $sp.ArgumentList = $args }
$p = Start-Process @sp
Write-Output ("START DBSrv({0}) pid={1}" -f $Dbsrv, $p.Id)

Start-Sleep -Seconds 3
# LoginSrv
$ls = Join-Path $CsharpRepo 'src\LoginSrv\bin\Release'
$p = Start-Process -FilePath (Join-Path $ls 'LoginSrv.exe') -WorkingDirectory $ls -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $LogDir 'LoginSrv.out.log') -RedirectStandardError (Join-Path $LogDir 'LoginSrv.err.log')
Write-Output ("START LoginSrv pid=" + $p.Id)
Start-Sleep -Seconds 4
# GameSvr (C# oracle)
$p = Start-Process -FilePath (Join-Path $MirServer 'M2GameSvr\GameSrv.exe') -WorkingDirectory (Join-Path $MirServer 'M2GameSvr') -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $LogDir 'GameSvr.out.log') -RedirectStandardError (Join-Path $LogDir 'GameSvr.err.log')
Write-Output ("START GameSvr pid=" + $p.Id)
Start-Sleep -Seconds 8
# gates
$gates = @(
  @{n='LoginGate'; exe=(Join-Path $MirServer 'LoginGate\LoginGate.exe'); dir=(Join-Path $MirServer 'LoginGate')},
  @{n='SelGate';   exe=(Join-Path $MirServer 'SelGate\SelGate.exe');     dir=(Join-Path $MirServer 'SelGate')},
  @{n='GameGate';  exe=(Join-Path $MirServer 'RunGate\GameGate.exe');    dir=(Join-Path $MirServer 'RunGate')}
)
foreach ($g in $gates) {
  $p = Start-Process -FilePath $g.exe -WorkingDirectory $g.dir -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $LogDir ($g.n+'.out.log')) -RedirectStandardError (Join-Path $LogDir ($g.n+'.err.log'))
  Write-Output ("START {0} pid={1}" -f $g.n, $p.Id)
  Start-Sleep -Seconds 2
}
Start-Sleep -Seconds 3
Show-Status
$missing = @($Ports | Where-Object { -not (Test-Port $_) })
if ($missing.Count -gt 0) { Write-Output ("RED: 端口未就绪: " + ($missing -join ',')); exit 1 }
Write-Output ("SIDE_STACK_OK DBSrv=" + $Dbsrv)
exit 0
