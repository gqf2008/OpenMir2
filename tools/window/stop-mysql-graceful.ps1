#requires -Version 5
<#
.SYNOPSIS
  stop-mysql-graceful.ps1 —— 优雅关闭本机 MariaDB/MySQL（**不强杀**），幂等、有界等待。

.DESCRIPTION
  背景（卡 `openmir2-mirserver-graceful-stop`）：`E:/MirServer/start-all.ps1 -Stop` 的 `Stop-Stack`
  对**每个**进程（含 mysqld）用 `Stop-Process -Force` ⇒ 2026-10-26 22:04:18 与 23:42:51 两次
  `InnoDB: ... crash recovery` 启动，重启前的 DB 证据与重启后不可直接对比。

  本脚本只做一件事：**把 mysqld 优雅关掉**（`mysqladmin shutdown`），并**等它真的退出**。
  它**绝不**调用 `Stop-Process`；超时也只是报错（rc=3）并给出人工处置建议。

  等待退出是**计数式**（卡 `openmir2-stop-mysql-graceful-b1-flake`）：预算 `-TimeoutSec` 秒
  按 200ms 轮询间隔折算成**轮询次数上限**，循环由计数器驱动，不拿墙钟做断言；
  每次轮询的观测（pid_alive / port_open）逐条记录，终态行带 `poll=<k>/<上限>` 与最后观测。

  幂等：没在跑 ⇒ `MYSQL_STOP already_stopped`，rc=0。

  密码：**只从环境变量取**（默认变量名 `MYSQL_ROOT_PASSWORD`，可用 `-PasswordEnv` 改），
  通过子进程的 `MYSQL_PWD` 传递，**不写命令行、不回显、不落盘**。

  输出（全部 Write-Host，不污染返回值/管道）：每行以 `MYSQL_STOP ` 开头
  （`already_stopped` / `plan` / `graceful_sent` / `stopped` / `TIMEOUT` / `ERROR`）。
  `mysqladmin` 的原始输出**落盘**到 `-OutFile`（默认 `$env:TEMP\mysql-shutdown-<ts>.log`），
  **不接管道截取**（管道截输出会让子进程继承 stdout 持有管道，见同卡 B 面）。

  **真实路径的 rc 采集（B1 修复）**：PS 5.1 里 `Start-Process -PassThru -RedirectStandardOutput`
  的 `$p.ExitCode` **恒为 `$null`**（reviewer-21 的最小复现：5.1 得 `$null`、pwsh 7 得 `7`；
  把 `-RedirectStandard*` 去掉后 5.1 也能拿到 7）。所以这里**不靠它**：用 `cmd /c "… > out 2> err"`
  包一层（cmd 透传 ERRORLEVEL），**并且**让包装器把 mysqladmin 的 `$LASTEXITCODE` 写进
  `<OutFile>.rc`、父进程**读那个文件**——两条通道都拿到才算数，`ExitCode` 只作第三兜底。

  退出码：0=已停/优雅停止成功；2=用法错；3=轮询预算耗尽（进程仍在，未强杀）；4=mysqladmin 失败。

.PARAMETER DryRun
  只打印将执行的命令（不含密码）与当前判定，不做任何动作。

.PARAMETER AdminExe
  覆盖 mysqladmin 路径（默认 `<MysqlHome>\bin\mysqladmin.exe`）。自检用它在真实进程路径上
  跑一个临时的 `mysqladmin.cmd`（`exit /b 0|1`）——这是 B1 的牙。

.PARAMETER Selftest
  离线自测：假进程表 + **真实进程路径**（临时 `mysqladmin.cmd`）共 22 条。

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
  [string]$AdminExe = '',
  [switch]$DryRun,
  [switch]$Selftest,
  # 仅供 -Selftest 注入；正常调用不要给
  [hashtable]$TestHooks = $null
)

$ErrorActionPreference = 'Stop'
$script:AdminExe = $AdminExe

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
  $admin = if ($script:AdminExe -ne '') { $script:AdminExe } else { Join-Path $mysqlHome 'bin\mysqladmin.exe' }
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
  # 真实路径（B1）：不用 `Start-Process -Redirect* -PassThru` 的 ExitCode（PS 5.1 恒 $null）。
  # 通道①：包装器把 mysqladmin 的 $LASTEXITCODE 写进 `<out>.rc`，父进程**读文件**；
  # 通道②：`cmd /c "… > out 2> err"` 的 ERRORLEVEL 透传；
  # 通道③（兜底）：cmd 进程的 ExitCode。
  $rcPath = $outPath + '.rc'
  $specPath = Join-Path ([IO.Path]::GetTempPath()) ('mysqladmin-spec-' + [guid]::NewGuid().ToString('N') + '.json')
  $runnerPath = Join-Path ([IO.Path]::GetTempPath()) ('mysqladmin-run-' + [guid]::NewGuid().ToString('N') + '.ps1')
  $spec = [ordered]@{ file = $admin; args = @($adminArgs) }
  [IO.File]::WriteAllText($specPath, ($spec | ConvertTo-Json -Depth 4), (New-Object Text.UTF8Encoding($false)))
  $runner = @'
param([string]$Spec, [string]$RcFile)
$s = Get-Content -LiteralPath $Spec -Raw -Encoding UTF8 | ConvertFrom-Json
& $s.file @($s.args)
$code = $LASTEXITCODE
if ($null -eq $code) { $code = 3 }
Set-Content -LiteralPath $RcFile -Value $code -Encoding ASCII
exit $code
'@
  [IO.File]::WriteAllText($runnerPath, $runner, (New-Object Text.UTF8Encoding($false)))
  $line = ('powershell -NoProfile -ExecutionPolicy Bypass -File "' + $runnerPath + '" -Spec "' + $specPath +
           '" -RcFile "' + $rcPath + '" > "' + $outPath + '" 2> "' + $outPath + '.err"')
  $old = [Environment]::GetEnvironmentVariable('MYSQL_PWD')
  try {
    if ($null -ne $pw) { [Environment]::SetEnvironmentVariable('MYSQL_PWD', $pw) }
    $p = Start-Process cmd.exe -ArgumentList '/c', $line -PassThru -WindowStyle Hidden
    if (-not $p.WaitForExit(30000)) {
      try { taskkill /PID $p.Id /T /F | Out-Null } catch { }
      Write-Stop 'ERROR' 'mysqladmin 30s 未返回（已终止包装器本身，未动 mysqld）'
      return 4
    }
    [void]$p.WaitForExit()
    $rc = $null
    if (Test-Path -LiteralPath $rcPath) {
      $raw = (Get-Content -LiteralPath $rcPath -Raw).Trim()
      if ($raw -match '^-?\d+$') { $rc = [int]$raw }
    }
    if ($null -eq $rc) { $rc = $p.ExitCode }
    if ($null -eq $rc) { $rc = 3 }
    if ($rc -ne 0) {
      Write-Stop 'ERROR' ("mysqladmin rc=" + $rc + "（见 " + $outPath + "）")
      return 4
    }
    return 0
  } finally {
    [Environment]::SetEnvironmentVariable('MYSQL_PWD', $old)
    Remove-Item -LiteralPath $specPath, $runnerPath -Force -ErrorAction SilentlyContinue
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
  # 计数式等待（卡 `openmir2-stop-mysql-graceful-b1-flake`）：不拿墙钟做上界断言——
  # 预算 `TimeoutSec` 秒按 200ms 轮询间隔折算成**轮询次数上限**，循环由计数器驱动；
  # 每次轮询的观测（pid_alive / port_open）逐条记进 `$pollLog`，终态行带 `poll=<k>/<上限>`
  # 与最后一条观测。判据只剩「最终必须退出」（计数器内发生）与「预算耗尽 ⇒ TIMEOUT rc=3」。
  $pollIntervalMs = 200
  $maxPolls = [Math]::Max(1, [int][Math]::Ceiling($TimeoutSec * 1000.0 / $pollIntervalMs))
  $pollLog = @()
  Write-Stop 'graceful_sent' ("mysqladmin shutdown 已发出；等待退出（计数式：间隔 " + $pollIntervalMs +
                              "ms，预算 " + $TimeoutSec + "s ⇒ ≤" + $maxPolls + " 次轮询）")
  for ($poll = 1; $poll -le $maxPolls; $poll++) {
    $pidAlive = Test-AnyPidAlive $pids
    $portOpen = Test-PortOpen $Port
    $pollLog += ('#' + $poll + ':pid_alive=' + $pidAlive + ',port_open=' + $portOpen)
    if (-not $pidAlive -and -not $portOpen) {
      Write-Stop 'stopped' ("已优雅退出（poll=" + $poll + "/" + $maxPolls + "; pid " + ($pids -join ',') +
                            " 不再存在，端口 " + $Port + " 已释放；最后观测 " + $pollLog[-1] + "）")
      return 0
    }
    if ($poll -lt $maxPolls) { Start-Sleep -Milliseconds $pollIntervalMs }
  }
  Write-Stop 'TIMEOUT' ("轮询预算耗尽（poll=" + $maxPolls + "/" + $maxPolls + "；pid 仍活：" + ($pids -join ',') +
                        "；最后观测 " + $pollLog[-1] + "）；**未强杀**。请查 logs/MySQL.err.log，由人决定下一步")
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
  function Run-Script([hashtable]$hooks, [int]$timeout, [string]$pwdEnv = '', [string]$mysqlHomeArg = '', [string]$admin = '') {
    if ($mysqlHomeArg -eq '') { $mysqlHomeArg = $script:tmpHome }
    $p = @{ MysqlHome = $mysqlHomeArg; TimeoutSec = $timeout; TestHooks = $hooks }
    if ($pwdEnv -ne '') { $p['PasswordEnv'] = $pwdEnv }
    if ($admin -ne '') { $p['AdminExe'] = $admin }
    # 6>&1：把 Write-Host 的信息流并进可捕获输出；`exit` 只结束子脚本并设置 $LASTEXITCODE。
    return (@(& $PSCommandPath @p 6>&1) -join "`n")
  }
  $script:tmpHome = Join-Path $env:TEMP ('mysql-stop-selftest-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
  New-Item -ItemType Directory -Force -Path $script:tmpHome | Out-Null
  try {
    # ① 已停 ⇒ already_stopped，rc=0，且**不**调 mysqladmin
    $marker = Join-Path $script:tmpHome 'launched.marker'
    $out = Run-Script @{ ProcessIds = { @() }; PortOpen = { param($p) $false }
                         Launch = { param($a, $b, $c) New-Item -ItemType File -Path $marker -Force | Out-Null; 0 } } 3
    Chk ($out -match 'MYSQL_STOP already_stopped') 'already stopped => already_stopped' $out
    Chk (-not (Test-Path -LiteralPath $marker)) 'already stopped => 不调 mysqladmin' ("marker exists=" + (Test-Path -LiteralPath $marker))
    Chk ($LASTEXITCODE -eq 0) 'already stopped => rc=0' ("rc=" + $LASTEXITCODE)

    # ② 优雅成功（钩子）
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

    # ④ mysqladmin 失败（钩子）⇒ rc=4
    $out = Run-Script @{ ProcessIds = { @(4245) }; PortOpen = { param($p) $false }
                         PidAlive = { param($ids) $true }
                         Launch = { param($a, $b, $c) 4 } } 1
    Chk ($LASTEXITCODE -eq 4) 'mysqladmin 失败(钩子) => rc=4' ("rc=" + $LASTEXITCODE)

    # ⑤ 真实进程路径（**不注入 Launch**）：临时 mysqladmin.cmd 的 exit /b 0|1 —— B1 的牙。
    $realHome = Join-Path $script:tmpHome 'realhome'
    $realBin = Join-Path $realHome 'bin'
    New-Item -ItemType Directory -Force -Path $realBin | Out-Null
    $fakeAdmin = Join-Path $realBin 'mysqladmin.cmd'
    [IO.File]::WriteAllText($fakeAdmin, "@echo off`r`nexit /b 0`r`n", (New-Object Text.UTF8Encoding($false)))
    # PidAlive 一并注入（卡 `openmir2-stop-mysql-graceful-b1-flake`）：pid 4242 是虚构的，
    # 旧版留真打 `Get-Process -Id 4242`——本机恰好有同号 PID 时等待循环空转满预算后 TIMEOUT 判红
    # （2026-10-29 三次观测 2 红 1 绿）。B1 要测的是 mysqladmin 真实路径，不是真进程表。
    $out = Run-Script @{ ProcessIds = { @(4242) }; PortOpen = { param($p) $false }
                         PidAlive = { param($ids) $false } } 5 '' $realHome $fakeAdmin
    Chk ($LASTEXITCODE -eq 0 -and ($out -match 'MYSQL_STOP stopped')) 'B1 真实路径: mysqladmin.cmd exit 0 => rc=0 且 stopped' ("rc=" + $LASTEXITCODE + " out=" + $out)
    [IO.File]::WriteAllText($fakeAdmin, "@echo off`r`nexit /b 1`r`n", (New-Object Text.UTF8Encoding($false)))
    $out = Run-Script @{ ProcessIds = { @(4244) }; PortOpen = { param($p) $false } } 3 '' $realHome $fakeAdmin
    Chk ($LASTEXITCODE -eq 4 -and ($out -match 'MYSQL_STOP ERROR')) 'B1 真实路径: mysqladmin.cmd exit 1 => rc=4 且 ERROR' ("rc=" + $LASTEXITCODE + " out=" + $out)
    Chk ($out -match 'rc=1') 'B1 真实路径: ERROR 行带真实 rc=1（不是空 rc）' $out

    # ⑥ 密码不回显 / 无 -Password 参数 / 实现体不含 Stop-Process（剥行尾注释后再匹配）
    $secret = 'S3cr3t-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
    [Environment]::SetEnvironmentVariable('MYSQL_STOP_SELFTEST_PW', $secret)
    $out = Run-Script @{ ProcessIds = { @(4246) }; PortOpen = { param($p) $false }
                         PidAlive = { param($ids) $false }
                         Launch = { param($a, $b, $c) 0 } } 1 'MYSQL_STOP_SELFTEST_PW'
    [Environment]::SetEnvironmentVariable('MYSQL_STOP_SELFTEST_PW', $null)
    Chk (-not $out.Contains($secret)) '密码不出现在输出' $out
    $src = [IO.File]::ReadAllText($PSCommandPath, [Text.Encoding]::UTF8)
    $markerLine = '# --------------------------------------------------------------------------- selftest'
    $body = if ($src.Contains($markerLine)) { $src.Substring(0, $src.IndexOf($markerLine)) } else { $src }
    $body = [regex]::Replace($body, '(?s)<#.*?#>', '')
    $codeOnly = (($body -split "`n") | ForEach-Object { ($_ -split '#', 2)[0] } | Where-Object { $_.Trim() -ne '' }) -join "`n"
    Chk (-not ($codeOnly -match 'Stop-Process')) '实现体不调用 Stop-Process（不强杀；注释里提到不算）'
    Chk (-not ($codeOnly -match '\[string\]\$Password\b')) '没有 -Password 参数（只走环境变量）'
    Chk (($codeOnly -notmatch 'Stop-Process') -and ([regex]::Replace($src, '(?s)<#.*?#>', '') -match 'Stop-Process')) 'F2: 注释（含行尾）里出现 Stop-Process 不误红（原文命中、剥后不命中）'

    # ⑦ 计数式等待牙（卡 `openmir2-stop-mysql-graceful-b1-flake`）：等待由**轮询计数器**驱动，
    #    不再引用墙钟上界。正向：第 3 次轮询翻转 ⇒ `poll=3/<预算>`；预算耗尽 ⇒ TIMEOUT 且
    #    `poll=<预算>/<预算>`、每次观测都有记录。反向牙（A2）：把计数器→观测的接线改坏
    #    （见变异记录）时这两条必红。
    $script:t3 = 0
    $out = Run-Script @{ ProcessIds = { @(4247) }; PortOpen = { param($p) $false }
                         PidAlive = { param($ids) $script:t3++; return ($script:t3 -le 2) }
                         Launch = { param($a, $b, $c) 0 } } 1
    Chk ($out -match 'MYSQL_STOP stopped') 'B1 计数: 第 3 次轮询翻转 => stopped' $out
    Chk ($out -match 'poll=3/5') 'B1 计数: stopped 记录实际翻转轮次 poll=3/5（计数器接线）' $out
    Chk ($LASTEXITCODE -eq 0) 'B1 计数: 第 3 次轮询翻转 => rc=0' ("rc=" + $LASTEXITCODE)

    $out = Run-Script @{ ProcessIds = { @(4248) }; PortOpen = { param($p) $true }
                         PidAlive = { param($ids) $true }
                         Launch = { param($a, $b, $c) 0 } } 1
    Chk ($out -match 'MYSQL_STOP TIMEOUT') 'B1 计数: 预算耗尽 => TIMEOUT' $out
    Chk ($out -match 'poll=5/5') 'B1 计数: TIMEOUT 恰在预算上限 poll=5/5' $out
    Chk ($LASTEXITCODE -eq 3) 'B1 计数: 预算耗尽 => rc=3' ("rc=" + $LASTEXITCODE)
    Chk ($out -match 'pid_alive=True,port_open=True') 'B1 计数: 每次观测有记录（最后观测进终态行）' $out
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
