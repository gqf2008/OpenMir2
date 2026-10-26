#requires -Version 5
<#
.SYNOPSIS
  stop-mysql-graceful.ps1 —— 优雅关闭本机 MariaDB/MySQL（**不强杀**），幂等、有界等待。

.DESCRIPTION
  背景（卡 `openmir2-mirserver-graceful-stop`）：`E:/MirServer/start-all.ps1 -Stop` 的 `Stop-Stack`
  对**每个**进程（含 mysqld）用 `Stop-Process -Force` ⇒ 2026-10-26 22:04:18 MySQL 被强杀，
  下次启动 `logs/MySQL.err.log` 首行 `InnoDB: Starting crash recovery from checkpoint LSN=…`，
  重启前的 DB 证据与重启后不可直接对比。

  本脚本只做一件事：**把 mysqld 优雅关掉**（`mysqladmin shutdown`），并**等它真的退出**。
  它**绝不**调用 `Stop-Process`；超时也只是报错（rc=3）并给出人工处置建议。

  幂等：没在跑 ⇒ `MYSQL_STOP already_stopped`，rc=0。

  密码：**只从环境变量取**（默认变量名 `MYSQL_ROOT_PASSWORD`，可用 `-PasswordEnv` 改），
  通过子进程的 `MYSQL_PWD` 传递，**不写命令行、不回显、不落盘**。

  输出（全部 Write-Host，不污染返回值/管道）：每行以 `MYSQL_STOP ` 开头
  （`already_stopped` / `plan` / `graceful_sent` / `stopped` / `TIMEOUT` / `ERROR`）。
  `mysqladmin` 的原始输出**落盘**到 `-OutFile`（默认 `$env:TEMP\mysql-shutdown-<ts>.log`），
  **不接管道截取**（管道截输出会让子进程继承 stdout 持有管道，见同卡 B 面）。

  退出码：0=已停/优雅停止成功；2=用法错；3=超时（进程仍在，未强杀）；4=mysqladmin 失败。

.PARAMETER DryRun
  只打印将执行的命令（不含密码）与当前判定，不做任何动作。

.PARAMETER Selftest
  离线自测：假进程表 + 假 mysqladmin，跑五个分支（已停 / 优雅成功 / 超时不强杀 / rc 语义 /
  密码不回显与命令行不带口令）。

.EXAMPLE
  $env:MYSQL_ROOT_PASSWORD = '…'      # 若 root 有口令
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/stop-mysql-graceful.ps1

.EXAMPLE
  # 接入 start-all.ps1 -Stop 的方式见 docs/window-runbook-20261023.md（只出 diff 建议，不改部署件）
#>
param(
  [string]$MysqlHome = 'D:\mysql\mariadb-10.11.19-winx64',
  [string]$DbHost = '127.0.0.1',
  [int]$Port = 3306,
  [string]$User = 'root',
  [string]$PasswordEnv = 'MYSQL_ROOT_PASSWORD',
  [int]$TimeoutSec = 60,
  [string]$OutFile = '',
  [switch]$DryRun,
  [switch]$Selftest,
  # 仅供 -Selftest 注入；正常调用不要给
  [hashtable]$TestHooks = $null
)

$ErrorActionPreference = 'Stop'

function Write-Stop([string]$state, [string]$detail = '') {
  # Write-Host：不污染函数返回值，也不会被调用方当成「要 exit 的数」。
  if ($detail) { Write-Host ("MYSQL_STOP " + $state + " " + $detail) }
  else { Write-Host ("MYSQL_STOP " + $state) }
}

function Get-MysqldProcessIds([string]$mysqlHome) {
  if ($TestHooks -and $TestHooks.ContainsKey('ProcessIds')) { return @(& $TestHooks['ProcessIds']) }
  $exe = Join-Path $mysqlHome 'bin\mysqld.exe'
  $ids = @()
  foreach ($p in (Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)) {
    if ($p.ExecutablePath -and ($p.ExecutablePath -ieq $exe)) { $ids += [int]$p.ProcessId }
  }
  return $ids
}

function Test-PortOpen([int]$p) {
  if ($TestHooks -and $TestHooks.ContainsKey('PortOpen')) { return [bool](& $TestHooks['PortOpen'] $p) }
  return [bool](Get-NetTCPConnection -State Listen -LocalPort $p -ErrorAction SilentlyContinue)
}

function Test-AnyPidAlive([int[]]$ids) {
  if ($TestHooks -and $TestHooks.ContainsKey('PidAlive')) { return [bool](& $TestHooks['PidAlive'] $ids) }
  foreach ($id in $ids) {
    if (Get-Process -Id $id -ErrorAction SilentlyContinue) { return $true }
  }
  return $false
}

function Invoke-MysqladminShutdown([string]$mysqlHome, [string]$h, [int]$port, [string]$user,
                                   [string]$pwdEnv, [string]$outPath) {
  $admin = Join-Path $mysqlHome 'bin\mysqladmin.exe'
  $pw = [Environment]::GetEnvironmentVariable($pwdEnv)
  $adminArgs = @('-h', $h, '-P', $port, '-u', $user, 'shutdown')
  if ($DryRun) {
    Write-Stop 'plan' ($admin + " " + ($adminArgs -join ' ') + "（密码取自环境变量 " + $pwdEnv + "，不回显）")
    return 0
  }
  if ($TestHooks -and $TestHooks.ContainsKey('Launch')) {
    return [int](& $TestHooks['Launch'] $admin $adminArgs $outPath)
  }
  if (-not (Test-Path -LiteralPath $admin)) {
    Write-Stop 'ERROR' ("缺 mysqladmin：" + $admin)
    return 4
  }
  $old = [Environment]::GetEnvironmentVariable('MYSQL_PWD')
  try {
    if ($null -ne $pw) { [Environment]::SetEnvironmentVariable('MYSQL_PWD', $pw) }
    $p = Start-Process -FilePath $admin -ArgumentList $adminArgs -PassThru -WindowStyle Hidden `
          -RedirectStandardOutput $outPath -RedirectStandardError ($outPath + '.err')
    if (-not $p.WaitForExit(30000)) {
      try { taskkill /PID $p.Id /T /F | Out-Null } catch { }
      Write-Stop 'ERROR' 'mysqladmin 30s 未返回（已终止 mysqladmin 本身，未动 mysqld）'
      return 4
    }
    if ($p.ExitCode -ne 0) {
      Write-Stop 'ERROR' ("mysqladmin rc=" + $p.ExitCode + "（见 " + $outPath + "）")
      return 4
    }
    return 0
  } finally {
    [Environment]::SetEnvironmentVariable('MYSQL_PWD', $old)
  }
}

function Invoke-Stop {
  if ($OutFile -eq '') {
    $OutFile = Join-Path $env:TEMP ('mysql-shutdown-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.log')
  }
  $pids = @(Get-MysqldProcessIds $MysqlHome)
  $portOpen = Test-PortOpen $Port
  if ($pids.Count -eq 0 -and -not $portOpen) {
    Write-Stop 'already_stopped' ("mysqld 未在跑（" + (Join-Path $MysqlHome 'bin\mysqld.exe') +
                                  "；端口 " + $Port + " 未监听）")
    return 0
  }
  if ($DryRun) {
    Write-Stop 'plan' ("将优雅关闭 pid=" + ($pids -join ',') + " port=" + $Port + " timeout=" + $TimeoutSec + "s")
    [void](Invoke-MysqladminShutdown $MysqlHome $DbHost $Port $User $PasswordEnv $OutFile)
    return 0
  }
  $rc = Invoke-MysqladminShutdown $MysqlHome $DbHost $Port $User $PasswordEnv $OutFile
  if ($rc -ne 0) { return $rc }
  Write-Stop 'graceful_sent' ("mysqladmin shutdown 已发出；等待退出（≤" + $TimeoutSec + "s）")
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if (-not (Test-AnyPidAlive $pids) -and -not (Test-PortOpen $Port)) {
      Write-Stop 'stopped' ("已优雅退出（pid " + ($pids -join ',') + "；端口 " + $Port + " 已释放）")
      return 0
    }
    Start-Sleep -Milliseconds 200
  }
  Write-Stop 'TIMEOUT' ("≤" + $TimeoutSec + "s 内未退出（pid 仍活：" + ($pids -join ',') +
                        "）；**未强杀**。请查 logs/MySQL.err.log，由人决定下一步")
  return 3
}

# --------------------------------------------------------------------------- selftest
function Invoke-Selftest {
  $script:spass = 0
  $script:sfail = 0
  function Chk([bool]$ok, [string]$name, [string]$detail = '') {
    if ($ok) { $script:spass++; Write-Host ("PASS " + $name) }
    else { $script:sfail++; Write-Host ("FAIL " + $name + "  " + $detail) }
  }
  function Run-Script([hashtable]$hooks, [int]$timeout, [string]$pwdEnv = '') {
    # 具名参数必须用**哈希表 splat**（数组 splat 是位置参数，会把 -TimeoutSec 当成 Port 的值）。
    $p = @{ MysqlHome = $script:tmpHome; TimeoutSec = $timeout; TestHooks = $hooks }
    if ($pwdEnv -ne '') { $p['PasswordEnv'] = $pwdEnv }
    # 6>&1：把 Write-Host 的信息流并进可捕获输出；`exit` 只结束子脚本并设置 $LASTEXITCODE。
    return (@(& $PSCommandPath @p 6>&1) -join "`n")
  }
  $script:tmpHome = Join-Path $env:TEMP ('mysql-stop-selftest-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
  New-Item -ItemType Directory -Force -Path $script:tmpHome | Out-Null
  try {
    # ① 已停 ⇒ already_stopped，rc=0，且**不**调 mysqladmin
    # 计数器用**文件标记**：钩子在子脚本里执行，脚本作用域的计数器跨不过去（那样牙会是空的）。
    $marker = Join-Path $script:tmpHome 'launched.marker'
    $out = Run-Script @{ ProcessIds = { @() }; PortOpen = { param($p) $false }
                         Launch = { param($a, $b, $c) New-Item -ItemType File -Path $marker -Force | Out-Null; 0 } } 3
    Chk ($out -match 'MYSQL_STOP already_stopped') 'already stopped => already_stopped' $out
    Chk (-not (Test-Path -LiteralPath $marker)) 'already stopped => 不调 mysqladmin' ("marker exists=" + (Test-Path -LiteralPath $marker))
    Chk ($LASTEXITCODE -eq 0) 'already stopped => rc=0' ("rc=" + $LASTEXITCODE)

    # ② 优雅成功：开始 alive，mysqladmin 成功后被判为已退出
    $script:alive = $true
    $out = Run-Script @{ ProcessIds = { @(4242) }; PortOpen = { param($p) $false }
                         PidAlive = { param($ids) $script:alive }
                         Launch = { param($a, $b, $c) $script:alive = $false; 0 } } 5
    Chk ($out -match 'MYSQL_STOP stopped') 'graceful => stopped' $out
    Chk ($LASTEXITCODE -eq 0) 'graceful => rc=0' ("rc=" + $LASTEXITCODE)

    # ③ 超时：进程一直活 ⇒ TIMEOUT + rc=3
    $out = Run-Script @{ ProcessIds = { @(4243) }; PortOpen = { param($p) $false }
                         PidAlive = { param($ids) $true }
                         Launch = { param($a, $b, $c) 0 } } 1
    Chk ($out -match 'MYSQL_STOP TIMEOUT') 'timeout => TIMEOUT' $out
    Chk ($LASTEXITCODE -eq 3) 'timeout => rc=3' ("rc=" + $LASTEXITCODE)

    # ④ mysqladmin 失败 ⇒ rc=4（不是 0、也不是 3）
    $out = Run-Script @{ ProcessIds = { @(4245) }; PortOpen = { param($p) $false }
                         PidAlive = { param($ids) $true }
                         Launch = { param($a, $b, $c) 4 } } 1
    Chk ($LASTEXITCODE -eq 4) 'mysqladmin 失败 => rc=4' ("rc=" + $LASTEXITCODE)

    # ⑤ 密码不回显 / 命令行不接 -Password / 正文不含 Stop-Process
    $secret = 'S3cr3t-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
    [Environment]::SetEnvironmentVariable('MYSQL_STOP_SELFTEST_PW', $secret)
    $out = Run-Script @{ ProcessIds = { @(4244) }; PortOpen = { param($p) $false }
                         PidAlive = { param($ids) $false }
                         Launch = { param($a, $b, $c) 0 } } 1 'MYSQL_STOP_SELFTEST_PW'
    [Environment]::SetEnvironmentVariable('MYSQL_STOP_SELFTEST_PW', $null)
    Chk (-not $out.Contains($secret)) '密码不出现在输出' $out
    # 只查**实现体**（selftest 分节之前），且去掉注释行——否则会匹配到本自检自己的断言文字。
    $src = [IO.File]::ReadAllText($PSCommandPath, [Text.Encoding]::UTF8)
    $marker = '# --------------------------------------------------------------------------- selftest'
    $body = if ($src.Contains($marker)) { $src.Substring(0, $src.IndexOf($marker)) } else { $src }
    # 注释块 <# … #> 也要剔掉：文档串里会写「不要用 Stop-Process -Force」这种反例。
    $body = [regex]::Replace($body, '(?s)<#.*?#>', '')
    $codeOnly = (($body -split "`n") | Where-Object { -not $_.TrimStart().StartsWith('#') }) -join "`n"
    Chk (-not ($codeOnly -match 'Stop-Process\s+-')) '实现体不调用 Stop-Process（不强杀；文档串里的名字不算）'
    Chk (-not ($codeOnly -match '\[string\]\$Password\b')) '没有 -Password 参数（只走环境变量）'
  } finally {
    Remove-Item -LiteralPath $script:tmpHome -Recurse -Force -ErrorAction SilentlyContinue
  }
  Write-Host ("MYSQL_STOP_SELFTEST_PASS=" + $script:spass + " FAIL=" + $script:sfail)
  if ($script:sfail -gt 0) { return 1 }
  Write-Host 'MYSQL_STOP_SELFTEST_OK'
  return 0
}

if ($Selftest) { exit (Invoke-Selftest) }
if ($TimeoutSec -lt 1) { Write-Host 'MYSQL_STOP USAGE -TimeoutSec 必须 ≥1'; exit 2 }
exit (Invoke-Stop)
