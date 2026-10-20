#requires -Version 5
<#
.SYNOPSIS
  selftest_stack_db_target.ps1 — C10#8 的启动器自检（离线；不连库、不起栈、不占端口）。

.DESCRIPTION
  用临时夹具树分别驱动两个起栈器：
    · `tools/m2-acceptance/start_rust_stack.ps1`
    · `tools/m5-acceptance/start_full_rust_stack.ps1`
  只跑 `-EmitPlan`（打印进程计划后退出；不查端口、不起进程）。断言三件事：
    1. **默认参数 ⇒ 命令行/生成配置逐字节不变**（旧路径、旧 dbsvr.conf 字节；game-svr 仍用真 Server.conf）。
    2. **给了 -Db\* ⇒ 只改 StackRoot/LogDir 里的影子副本**：DBSrv/LoginSrv/game-svr 分别指向
       `mir2_test_db/_account/_data`，host/port/user/password 覆盖生效；**源配置 sha256 不变**。
    3. **判据有牙**：非法端口、配置缺 `[DataBase]/ConnctionString` ⇒ 非 0 退出且打印 `RED`。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/selftest_stack_db_target.ps1
#>
param([string]$OutDir = '')

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..')).Path
$StartRust = Join-Path $RepoRoot 'tools\m2-acceptance\start_rust_stack.ps1'
$StartFull = Join-Path $RepoRoot 'tools\m5-acceptance\start_full_rust_stack.ps1'
. (Join-Path $Here 'db_target_plumb.ps1')

if ([string]::IsNullOrEmpty($OutDir)) {
  $OutDir = Join-Path ([System.IO.Path]::GetTempPath()) ("mir2-stack-dbtarget-selftest-" + [Guid]::NewGuid().ToString('N'))
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$script:ok = $true
function Assert-True([string]$Name, [bool]$Cond, [string]$Detail = '') {
  if ($Cond) { Write-Output ("PASS  {0}" -f $Name) }
  else {
    Write-Output ("FAIL  {0}{1}" -f $Name, $(if ($Detail) { "（" + $Detail + "）" } else { '' }))
    $script:ok = $false
  }
}

function Invoke-Stack([string]$ScriptPath, [string[]]$Arguments) {
  $lines = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $ScriptPath @Arguments 2>&1
  return [pscustomobject]@{ Code = $LASTEXITCODE; Text = (($lines | ForEach-Object { "$_" }) -join "`n") }
}

function New-Fixture([string]$Root) {
  $cs = Join-Path $Root 'cs'
  $ms = Join-Path $Root 'ms'
  $bin = Join-Path $Root 'bin'
  $logs = Join-Path $Root 'logs'
  $stack = Join-Path $Root 'stack'
  $dbs = Join-Path $cs 'src\DBSrv\bin\Release'
  $lsv = Join-Path $cs 'src\LoginSrv\bin\Release'
  foreach ($d in @($dbs, $lsv, $bin, $logs, $stack,
                   (Join-Path $ms 'M2GameSvr\Envir'), (Join-Path $ms 'M2GameSvr\Map'),
                   (Join-Path $ms 'LoginSrv'), (Join-Path $ms 'LoginGate'),
                   (Join-Path $ms 'SelGate'), (Join-Path $ms 'RunGate'))) {
    New-Item -ItemType Directory -Force -Path $d | Out-Null
  }
  $enc = New-Object System.Text.ASCIIEncoding
  $dbConf = @'
[Setup]
ShowDebug=1
ServerAddr=0.0.0.0
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
'@
  [System.IO.File]::WriteAllText((Join-Path $dbs 'dbsvr.conf'), $dbConf, $enc)
  [System.IO.File]::WriteAllText((Join-Path $dbs 'DBSrv.exe'), 'stub', $enc)
  [System.IO.File]::WriteAllText((Join-Path $lsv 'logsrv.conf'),
    "[DataBase]`nConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_account;`n", $enc)
  [System.IO.File]::WriteAllText((Join-Path $lsv 'AddrTable.txt'), "127.0.0.1`r`n", $enc)
  [System.IO.File]::WriteAllText((Join-Path $lsv 'LoginSrv.exe'), 'stub', $enc)
  [System.IO.File]::WriteAllText((Join-Path $ms 'M2GameSvr\Server.conf'),
    "[DataBase]`nConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_data;`n[Share]`nEnvirDir=Envir`n", $enc)
  [System.IO.File]::WriteAllText((Join-Path $ms 'M2GameSvr\Envir\MapInfo.txt'), "[0|0]`r`n", $enc)
  [System.IO.File]::WriteAllText((Join-Path $ms 'M2GameSvr\Map\0.map'), 'map', $enc)
  [System.IO.File]::WriteAllText((Join-Path $ms 'LoginSrv\UserLimit.txt'), "10`r`n", $enc)
  foreach ($g in @('LoginGate', 'SelGate', 'RunGate')) {
    [System.IO.File]::WriteAllText((Join-Path $ms ($g + '\config.conf')), "[Server]`nPort=1`n", $enc)
  }
  foreach ($e in @('db-srv', 'login-srv', 'game-svr', 'login-gate', 'sel-gate', 'game-gate')) {
    [System.IO.File]::WriteAllText((Join-Path $bin ($e + '.exe')), 'stub', $enc)
  }
  return [pscustomobject]@{ Root = $Root; Cs = $cs; Ms = $ms; Bin = $bin; Logs = $logs; Stack = $stack; DbConf = $dbConf }
}

function Get-Sha([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash }
function Find-Proc($Plan, [string]$Name) { return @($Plan.procs | Where-Object { $_.name -eq $Name })[0] }

try {
  $fx = New-Fixture (Join-Path $OutDir 'fixture')
  $srcDbConf = Join-Path $fx.Cs 'src\DBSrv\bin\Release\dbsvr.conf'
  $srcLsConf = Join-Path $fx.Cs 'src\LoginSrv\bin\Release\logsrv.conf'
  $srcGsConf = Join-Path $fx.Ms 'M2GameSvr\Server.conf'
  $shaDb0 = Get-Sha $srcDbConf
  $shaLs0 = Get-Sha $srcLsConf
  $shaGs0 = Get-Sha $srcGsConf

  # -------------------------------------------------------------------------
  # 1) start_rust_stack：默认 plan（旧命令行逐项钉死）
  # -------------------------------------------------------------------------
  $r = Invoke-Stack $StartRust @('-EmitPlan', '-CsharpRepo', $fx.Cs, '-MirServer', $fx.Ms,
                                 '-RustBinDir', $fx.Bin, '-LogDir', $fx.Logs)
  Assert-True 'start_rust_stack 默认：rc=0' ($r.Code -eq 0) $r.Text
  $plan = $null
  try { $plan = $r.Text | ConvertFrom-Json } catch { }
  Assert-True 'start_rust_stack 默认：输出是 JSON plan' ($null -ne $plan -and $plan.mode -eq 'start_rust_stack') $r.Text
  if ($plan) {
    Assert-True 'start_rust_stack 默认：db_target.present=false' ($plan.db_target.present -eq $false)
    $names = @($plan.procs | ForEach-Object { $_.name })
    Assert-True 'start_rust_stack 默认：进程序 MySQL,DBSrv,LoginSrv,game-svr,三网关' (
      ($names -join ',') -eq 'MySQL,DBSrv,LoginSrv,game-svr,login-gate,sel-gate,game-gate') ($names -join ',')
    $p = Find-Proc $plan 'DBSrv'
    Assert-True 'start_rust_stack 默认：DBSrv 用源 Release + 无参数 + 源 workdir' (
      $p.exe -eq (Join-Path $fx.Cs 'src\DBSrv\bin\Release\DBSrv.exe') -and
      @($p.args).Count -eq 0 -and
      $p.workdir -eq (Join-Path $fx.Cs 'src\DBSrv\bin\Release')) ($p | ConvertTo-Json -Compress)
    $p = Find-Proc $plan 'LoginSrv'
    Assert-True 'start_rust_stack 默认：LoginSrv 用源 Release + 无参数 + 源 workdir' (
      $p.exe -eq (Join-Path $fx.Cs 'src\LoginSrv\bin\Release\LoginSrv.exe') -and
      @($p.args).Count -eq 0 -and
      $p.workdir -eq (Join-Path $fx.Cs 'src\LoginSrv\bin\Release')) ($p | ConvertTo-Json -Compress)
    $p = Find-Proc $plan 'game-svr'
    Assert-True 'start_rust_stack 默认：game-svr 读真 Server.conf（无影子）' (
      @($p.args).Count -eq 1 -and $p.args[0] -eq (Join-Path $fx.Ms 'M2GameSvr\Server.conf') -and
      $p.workdir -eq (Join-Path $fx.Ms 'M2GameSvr')) ($p | ConvertTo-Json -Compress)
    $p = Find-Proc $plan 'game-gate'
    Assert-True 'start_rust_stack 默认：game-gate 参数/workdir 不变' (
      @($p.args).Count -eq 1 -and $p.args[0] -eq (Join-Path $fx.Ms 'RunGate\config.conf') -and
      $p.workdir -eq (Join-Path $fx.Ms 'RunGate')) ($p | ConvertTo-Json -Compress)
  }
  Assert-True 'start_rust_stack 默认：不生成 db-target 影子目录' (-not (Test-Path -LiteralPath (Join-Path $fx.Logs 'db-target')))

  # -------------------------------------------------------------------------
  # 2) start_full_rust_stack：默认生成的 dbsvr.conf 字节 == 旧字面量
  # -------------------------------------------------------------------------
  $r = Invoke-Stack $StartFull @('-EmitPlan', '-CsharpRepo', $fx.Cs, '-MirServer', $fx.Ms,
                                 '-RustBinDir', $fx.Bin, '-StackRoot', $fx.Stack, '-LogDir', (Join-Path $fx.Root 'logs-full'))
  Assert-True 'start_full 默认：rc=0' ($r.Code -eq 0) $r.Text
  $planF = $null
  try { $planF = $r.Text | ConvertFrom-Json } catch { }
  Assert-True 'start_full 默认：输出是 JSON plan' ($null -ne $planF -and $planF.mode -eq 'start_full_rust_stack') $r.Text
  $genDb = Join-Path $fx.Stack 'db-srv\dbsvr.conf'
  $expected = $fx.DbConf
  Assert-True 'start_full 默认：生成的 dbsvr.conf 逐字节等于旧版字面量' (
    (Test-Path -LiteralPath $genDb) -and
    ([System.IO.File]::ReadAllText($genDb, [System.Text.Encoding]::ASCII) -ceq $expected)) 'README: 默认配置漂移'
  Assert-True 'start_full 默认：db_target.present=false' ($planF -and $planF.db_target.present -eq $false)
  $p = Find-Proc $planF 'db-srv'
  Assert-True 'start_full 默认：db-srv 用 StackRoot 的 dbsvr.conf' (
    @($p.args).Count -eq 1 -and $p.args[0] -eq (Join-Path $fx.Stack 'db-srv\dbsvr.conf')) ($p | ConvertTo-Json -Compress)
  $p = Find-Proc $planF 'game-svr'
  Assert-True 'start_full 默认：game-svr 仍用真 Server.conf（无影子）' (
    @($p.args).Count -eq 1 -and $p.args[0] -eq (Join-Path $fx.Ms 'M2GameSvr\Server.conf')) ($p | ConvertTo-Json -Compress)
  Assert-True 'start_full 默认：不生成 game-svr 影子目录' (-not (Test-Path -LiteralPath (Join-Path $fx.Stack 'game-svr')))

  # -------------------------------------------------------------------------
  # 3) start_rust_stack + 全套 -Db*：影子指向测试库，源配置不动
  # -------------------------------------------------------------------------
  $logsDb = Join-Path $fx.Root 'logs-db'
  $r = Invoke-Stack $StartRust @('-EmitPlan', '-CsharpRepo', $fx.Cs, '-MirServer', $fx.Ms,
                                 '-RustBinDir', $fx.Bin, '-LogDir', $logsDb,
                                 '-DbName', 'mir2_test', '-DbHost', '10.1.2.3', '-DbPort', '3307',
                                 '-DbUser', 'tester', '-DbPassword', 'secret')
  Assert-True 'start_rust_stack +Db*：rc=0' ($r.Code -eq 0) $r.Text
  $planDb = $null
  try { $planDb = $r.Text | ConvertFrom-Json } catch { }
  Assert-True 'start_rust_stack +Db*：db_target 三库展开正确' (
    $planDb -and $planDb.db_target.db_db -eq 'mir2_test_db' -and
    $planDb.db_target.db_account -eq 'mir2_test_account' -and
    $planDb.db_target.db_data -eq 'mir2_test_data') ($r.Text)
  $shadow = Join-Path $logsDb 'db-target'
  $p = Find-Proc $planDb 'DBSrv'
  Assert-True 'start_rust_stack +Db*：DBSrv 从影子 exe 起' (
    $p.exe -eq (Join-Path $shadow 'DBSrv\DBSrv.exe') -and $p.workdir -eq (Join-Path $shadow 'DBSrv')) ($p | ConvertTo-Json -Compress)
  $p = Find-Proc $planDb 'LoginSrv'
  Assert-True 'start_rust_stack +Db*：LoginSrv 从影子 exe 起' (
    $p.exe -eq (Join-Path $shadow 'LoginSrv\LoginSrv.exe') -and $p.workdir -eq (Join-Path $shadow 'LoginSrv')) ($p | ConvertTo-Json -Compress)
  $p = Find-Proc $planDb 'game-svr'
  Assert-True 'start_rust_stack +Db*：game-svr 读影子 Server.conf' (
    @($p.args).Count -eq 1 -and $p.args[0] -eq (Join-Path $shadow 'M2GameSvr\Server.conf')) ($p | ConvertTo-Json -Compress)
  $want = 'server=10.1.2.3;uid=tester;pwd=secret;database=mir2_test_db;port=3307;'
  Assert-True 'start_rust_stack +Db*：DBSrv 影子连接串改写' (
    (Get-Mir2IniConnString -Path (Join-Path $shadow 'DBSrv\dbsvr.conf')) -eq $want)
  Assert-True 'start_rust_stack +Db*：LoginSrv 影子连接串改写' (
    (Get-Mir2IniConnString -Path (Join-Path $shadow 'LoginSrv\logsrv.conf')) -eq
      'server=10.1.2.3;uid=tester;pwd=secret;database=mir2_test_account;port=3307;')
  Assert-True 'start_rust_stack +Db*：game-svr 影子连接串改写' (
    (Get-Mir2IniConnString -Path (Join-Path $shadow 'M2GameSvr\Server.conf')) -eq
      'server=10.1.2.3;uid=tester;pwd=secret;database=mir2_test_data;port=3307;')
  Assert-True 'start_rust_stack +Db*：game-svr 影子仍能看到 Envir/Map 数据' (
    (Test-Path -LiteralPath (Join-Path $shadow 'M2GameSvr\Envir\MapInfo.txt')) -and
    (Test-Path -LiteralPath (Join-Path $shadow 'M2GameSvr\Map\0.map')))
  Assert-True 'start_rust_stack +Db*：源 dbsvr.conf 未被改' ((Get-Sha $srcDbConf) -eq $shaDb0)
  Assert-True 'start_rust_stack +Db*：源 logsrv.conf 未被改' ((Get-Sha $srcLsConf) -eq $shaLs0)
  Assert-True 'start_rust_stack +Db*：源 Server.conf 未被改' ((Get-Sha $srcGsConf) -eq $shaGs0)

  # 3b) 只给 -DbName：host/user/pwd 沿用各组件原配置
  $logsName = Join-Path $fx.Root 'logs-dbname'
  $r = Invoke-Stack $StartRust @('-EmitPlan', '-CsharpRepo', $fx.Cs, '-MirServer', $fx.Ms,
                                 '-RustBinDir', $fx.Bin, '-LogDir', $logsName, '-DbName', 'mir2_test')
  Assert-True 'start_rust_stack 只给 -DbName：rc=0' ($r.Code -eq 0) $r.Text
  Assert-True 'start_rust_stack 只给 -DbName：换库不换 host/user/pwd' (
    (Get-Mir2IniConnString -Path (Join-Path $logsName 'db-target\DBSrv\dbsvr.conf')) -eq
      'server=127.0.0.1;uid=root;pwd=;database=mir2_test_db;')

  # 3c) 默认路径不受「上一次留下的影子目录」影响
  $r = Invoke-Stack $StartRust @('-EmitPlan', '-CsharpRepo', $fx.Cs, '-MirServer', $fx.Ms,
                                 '-RustBinDir', $fx.Bin, '-LogDir', $logsDb)
  $planAgain = $r.Text | ConvertFrom-Json
  $p = Find-Proc $planAgain 'DBSrv'
  Assert-True 'start_rust_stack 默认：忽略遗留 db-target（仍用源 exe）' (
    $p.exe -eq (Join-Path $fx.Cs 'src\DBSrv\bin\Release\DBSrv.exe'))

  # -------------------------------------------------------------------------
  # 4) start_full_rust_stack + 全套 -Db*：影子配置三库改写，源文件不动
  # -------------------------------------------------------------------------
  $stack2 = Join-Path $fx.Root 'stack-db'
  $r = Invoke-Stack $StartFull @('-EmitPlan', '-CsharpRepo', $fx.Cs, '-MirServer', $fx.Ms,
                                 '-RustBinDir', $fx.Bin, '-StackRoot', $stack2, '-LogDir', (Join-Path $fx.Root 'logs-full-db'),
                                 '-DbName', 'mir2_test_*', '-DbHost', '10.1.2.3', '-DbPort', '3307',
                                 '-DbUser', 'tester', '-DbPassword', 'secret')
  Assert-True 'start_full +Db*：rc=0' ($r.Code -eq 0) $r.Text
  $planF2 = $null
  try { $planF2 = $r.Text | ConvertFrom-Json } catch { }
  Assert-True 'start_full +Db*：db-srv 连接串指向 mir2_test_db' (
    (Get-Mir2IniConnString -Path (Join-Path $stack2 'db-srv\dbsvr.conf')) -eq
      'server=10.1.2.3;uid=tester;pwd=secret;database=mir2_test_db;port=3307;')
  Assert-True 'start_full +Db*：login-srv 连接串指向 mir2_test_account' (
    (Get-Mir2IniConnString -Path (Join-Path $stack2 'login-srv\logsrv.conf')) -eq
      'server=10.1.2.3;uid=tester;pwd=secret;database=mir2_test_account;port=3307;')
  Assert-True 'start_full +Db*：game-svr 连接串指向 mir2_test_data' (
    (Get-Mir2IniConnString -Path (Join-Path $stack2 'game-svr\Server.conf')) -eq
      'server=10.1.2.3;uid=tester;pwd=secret;database=mir2_test_data;port=3307;')
  $p = Find-Proc $planF2 'game-svr'
  Assert-True 'start_full +Db*：game-svr 从影子 conf 起' (
    @($p.args).Count -eq 1 -and $p.args[0] -eq (Join-Path $stack2 'game-svr\Server.conf')) ($p | ConvertTo-Json -Compress)
  Assert-True 'start_full +Db*：game-svr 影子仍能看到 Envir/Map' (
    (Test-Path -LiteralPath (Join-Path $stack2 'game-svr\Envir\MapInfo.txt')) -and
    (Test-Path -LiteralPath (Join-Path $stack2 'game-svr\Map\0.map')))
  Assert-True 'start_full +Db*：源 logsrv.conf 未被改' ((Get-Sha $srcLsConf) -eq $shaLs0)
  Assert-True 'start_full +Db*：源 Server.conf 未被改' ((Get-Sha $srcGsConf) -eq $shaGs0)

  # -------------------------------------------------------------------------
  # 5) 负面：非法参数 / 配置缺 ConnctionString ⇒ RED（非 0）
  # -------------------------------------------------------------------------
  $r = Invoke-Stack $StartRust @('-EmitPlan', '-CsharpRepo', $fx.Cs, '-MirServer', $fx.Ms,
                                 '-RustBinDir', $fx.Bin, '-LogDir', (Join-Path $fx.Root 'logs-bad'),
                                 '-DbPort', '0')
  Assert-True '非法 -DbPort 0 ⇒ rc=1 + RED' ($r.Code -eq 1 -and $r.Text -match 'RED:') $r.Text

  $badRoot = Join-Path $OutDir 'bad-fixture'
  Copy-Item -LiteralPath $fx.Cs -Destination (Join-Path $badRoot 'cs') -Recurse -Force
  Copy-Item -LiteralPath $fx.Ms -Destination (Join-Path $badRoot 'ms') -Recurse -Force
  Copy-Item -LiteralPath $fx.Bin -Destination (Join-Path $badRoot 'bin') -Recurse -Force
  [System.IO.File]::WriteAllText((Join-Path $badRoot 'cs\src\DBSrv\bin\Release\dbsvr.conf'),
    "[Setup]`nServerPort=6000`n", (New-Object System.Text.ASCIIEncoding))
  $r = Invoke-Stack $StartRust @('-EmitPlan', '-CsharpRepo', (Join-Path $badRoot 'cs'), '-MirServer', (Join-Path $badRoot 'ms'),
                                 '-RustBinDir', (Join-Path $badRoot 'bin'), '-LogDir', (Join-Path $badRoot 'logs'),
                                 '-DbName', 'mir2_test')
  Assert-True 'DB 配置缺 [DataBase]/ConnctionString ⇒ rc=1 + RED（fail-closed）' (
    $r.Code -eq 1 -and $r.Text -match 'RED: 测试库影子生成失败') $r.Text
} finally {
  if ($script:ok) { Remove-Item -LiteralPath $OutDir -Recurse -Force -ErrorAction SilentlyContinue }
}

Write-Output ("SELFTEST {0}" -f $(if ($script:ok) { 'GREEN' } else { 'RED' }))
if ($script:ok) { exit 0 } else { exit 1 }
