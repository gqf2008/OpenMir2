#requires -Version 5
<#
.SYNOPSIS
  account_provision.ps1 — 测试账号开号（建角流程的 scratch 账号用）。

.DESCRIPTION
  在账号库（默认 `mir2_account`）里建一个可用账号，**account 与 account_protection 两张表都要写**：
  设计文档 §8.1 记录过——LoginSrv 用 account INNER JOIN account_protection，
  缺 protection 行时会报「获取账号资料出错」。密码按本工程约定存明文（行为等价优先）。

  幂等：账号已存在时只报告，不重复插入。

  测试库（E1-a，§C10#7）：`-Db mir2_test_account -DbHost 127.0.0.1` 可把**所有** SQL 指向克隆库；
  缺省仍是 `mir2_account` + `127.0.0.1`（库名/SQL 文本与旧版逐字一致，默认行为不变）。
  `-Host` 是 `-DbHost` 的别名（PowerShell 的 `$Host` 是只读自动变量，不能直接用作参数名）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File account_provision.ps1 -Account goldprobe

.EXAMPLE
  # 测试库开号（E1-a）：库名只允许字母/数字/下划线，防止 SQL 拼接被污染
  powershell -ExecutionPolicy Bypass -File account_provision.ps1 -Account g1new -Db mir2_test_account -DbHost 127.0.0.1

.EXAMPLE
  # 批量开号（压测档位用）：loadbot0 .. loadbot1999，幂等、一条 SQL 建完（比逐个调用快）
  powershell -ExecutionPolicy Bypass -File account_provision.ps1 -Prefix loadbot -Count 1000

.EXAMPLE
  # 自检：用临时 mock mysql.exe 跑真实开号路径，断言 SQL/主机参数；不连任何数据库
  powershell -ExecutionPolicy Bypass -File account_provision.ps1 -Selftest
#>
param(
  [string]$Account = "",
  [string]$Password = "",
  [string]$Birthday = "1986/06/06",
  [string]$Mysql = "D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe",
  [string]$Prefix = "",
  [int]$Count = 0,
  [Alias("Host")][string]$DbHost = "127.0.0.1",
  [string]$Db = "mir2_account",
  [switch]$Selftest
)
$ErrorActionPreference = "Stop"
if ($Password -eq "") { $Password = $Account }


function Assert-SafeName([string]$Name, [string]$Kind) {
  # 库名/主机只允许保守字符集：这些值会拼进 SQL / 原生命令行，默认值（mir2_account / 127.0.0.1）不受影响。
  $pattern = if ($Kind -eq "db") { '^[A-Za-z0-9_]+$' } else { '^[A-Za-z0-9._:\-]+$' }
  if ($Name -notmatch $pattern) {
    throw "非法${Kind}: '$Name'（只允许 $(if ($Kind -eq 'db') { '字母/数字/下划线' } else { '字母/数字/点/冒号/下划线/连字符' })）"
  }
  return $Name
}

function Invoke-Mysql([string]$Sql) {
  # mysql.exe 会往 stderr 打横幅/分隔线；ErrorActionPreference=Stop 下会被当成终止错误
  $prev = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $out = & $Mysql -h $DbHost -u root --batch --skip-column-names -e $Sql 2>&1 |
      Where-Object { $_ -notmatch '^-+$' }
    if ($LASTEXITCODE -ne 0) { throw "mysql 失败: $out" }
    return $out
  } finally { $ErrorActionPreference = $prev }
}

function New-MockMysql([string]$Dir) {
  # mock mysql.exe：只记录参数 + 返回开号流程需要的两处读取，不连库。
  $mock = Join-Path $Dir "mock_mysql.cmd"
  $content = @'
@echo off
>>"%MOCK_LOG%" echo ARGS %*
set /a N=0
if exist "%MOCK_CNT%" set /p N=<"%MOCK_CNT%"
set /a N=%N%+1
>"%MOCK_CNT%" echo %N%
echo %* | findstr /C:"SELECT Id FROM" >nul
if errorlevel 1 goto :notid
if %N%==1 goto :eof
echo 7
goto :eof
:notid
echo %* | findstr /C:"SELECT COUNT(*)" >nul
if not errorlevel 1 echo 2
'@
  Set-Content -Path $mock -Value $content -Encoding ASCII
  return $mock
}

function Invoke-Selftest {
  $tmpRoot = Join-Path ([IO.Path]::GetTempPath()) ("acct-provision-selftest-" + [Guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null
  $script:stFail = 0
  $scriptPath = $MyInvocation.MyCommand.Path
  if (-not $scriptPath) { $scriptPath = $PSCommandPath }

  function Assert-That([string]$Name, [bool]$Cond, [string]$Detail) {
    if ($Cond) {
      Write-Output ("{0,-58} OK" -f $Name)
    } else {
      Write-Output ("{0,-58} FAIL {1}" -f $Name, $Detail)
      $script:stFail++
    }
  }

  function Invoke-Child([string]$Case, [string[]]$ExtraArgs) {
    $dir = Join-Path $tmpRoot $Case
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $log = Join-Path $dir "args.log"
    $cnt = Join-Path $dir "calls.txt"
    $out = Join-Path $dir "stdout.txt"
    $mock = New-MockMysql $dir
    $env:MOCK_LOG = $log
    $env:MOCK_CNT = $cnt
    $childArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $scriptPath,
                   "-Account", "probe", "-Password", "probe", "-Mysql", $mock) + $ExtraArgs
    # 子进程的 stderr（非法库名那条 throw）在 EAP=Stop 下会变成终止错误；这里临时降级，
    # 只取 rc/输出做断言，不吞掉失败（rc!=0 仍由用例判）。
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { & powershell @childArgs > $out 2>&1; $rc = $LASTEXITCODE }
    finally { $ErrorActionPreference = $prevEap }
    $lines = if (Test-Path $log) { Get-Content $log -Raw } else { "" }
    return [pscustomobject]@{ rc = $rc; log = $lines; out = (Get-Content $out -Raw) }
  }

  # 1) 默认行为：SQL 仍写 mir2_account，主机仍 127.0.0.1
  $r = Invoke-Child "default" @()
  Assert-That "默认：rc=0" ($r.rc -eq 0) ("rc=" + $r.rc + " out=" + $r.out)
  Assert-That "默认：-h 127.0.0.1" ($r.log -match '-h 127\.0\.0\.1') $r.log
  Assert-That "默认：INSERT mir2_account.account" ($r.log -match 'INSERT INTO mir2_account\.account ') $r.log
  Assert-That "默认：INSERT mir2_account.account_protection" ($r.log -match 'INSERT INTO mir2_account\.account_protection ') $r.log

  # 2) 测试库 + 自定义主机（E1-a，§C10#7）
  $r = Invoke-Child "testdb" @("-Db", "mir2_test_account", "-DbHost", "127.0.0.2")
  Assert-That "测试库：rc=0" ($r.rc -eq 0) ("rc=" + $r.rc + " out=" + $r.out)
  Assert-That "测试库：-h 127.0.0.2" ($r.log -match '-h 127\.0\.0\.2') $r.log
  Assert-That "测试库：INSERT mir2_test_account.account" ($r.log -match 'INSERT INTO mir2_test_account\.account ') $r.log
  Assert-That "测试库：INSERT mir2_test_account.account_protection" ($r.log -match 'INSERT INTO mir2_test_account\.account_protection ') $r.log
  Assert-That "测试库：不再出现生产库名" ($r.log -notmatch 'mir2_account\.') $r.log

  # 3) -Host 别名（PowerShell $Host 只读自动变量，别名必须映射到 DbHost）
  $r = Invoke-Child "hostalias" @("-Host", "127.0.0.3")
  Assert-That "别名 -Host：rc=0" ($r.rc -eq 0) ("rc=" + $r.rc + " out=" + $r.out)
  Assert-That "别名 -Host：-h 127.0.0.3" ($r.log -match '-h 127\.0\.0\.3') $r.log

  # 4) 批量模式：seq 表与目标库都跟随 -Db
  $r = Invoke-Child "bulk" @("-Prefix", "sel", "-Count", "2", "-Db", "mir2_test_account")
  Assert-That "批量：rc=0" ($r.rc -eq 0) ("rc=" + $r.rc + " out=" + $r.out)
  Assert-That "批量：FROM mir2_test_account.seq_1_to_2" ($r.log -match 'FROM mir2_test_account\.seq_1_to_2') $r.log
  Assert-That "批量：INSERT mir2_test_account.account" ($r.log -match 'INSERT INTO mir2_test_account\.account ') $r.log

  # 5) 非法库名必须在任何 mysql 调用前被拒（fail-closed，不给 SQL 拼接留口）
  $r = Invoke-Child "baddb" @("-Db", "x;drop")
  Assert-That "非法库名：rc!=0" ($r.rc -ne 0) ("rc=" + $r.rc + " out=" + $r.out)
  Assert-That "非法库名：未调用 mysql" ([string]::IsNullOrWhiteSpace($r.log)) $r.log

  Remove-Item Env:MOCK_LOG -ErrorAction SilentlyContinue
  Remove-Item Env:MOCK_CNT -ErrorAction SilentlyContinue
  Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  if ($script:stFail -gt 0) {
    Write-Output ("SELFTEST RED ({0} FAILED)" -f $script:stFail)
    exit 1
  }
  Write-Output "SELFTEST GREEN"
  exit 0
}

if ($Selftest) { Invoke-Selftest }

$Db = Assert-SafeName $Db "db"
$DbHost = Assert-SafeName $DbHost "host"

# ---- 批量模式：-Prefix + -Count（压测档位用）----
# MariaDB 的 sequence 引擎（seq_1_to_N）一条 SQL 建 N 个账号。
# **幂等必须用 WHERE NOT EXISTS，不能用 INSERT IGNORE**（2026-10-11 实测根因）：
# `account.Account` 上**没有唯一索引**（只有按名建的非唯一索引 _WA_Sys_FLD_LOGINID），
# 所以 `INSERT IGNORE` 不会去重——每跑一轮就再插 1000 行副本，loadbot0 累积到 6 行、
# 全表 11354 行 / 7151 个不同名。LoginSrv 的 account⋈account_protection 在重复行上会产生歧义
# ⇒ 反复跑压测时登录爬坡越来越差（tier-500 78%、tier-1000 38.6%）。
# 内容冻结（§1 硬约束②）不允许给 oracle 表加唯一索引，所以幂等只能在开号 SQL 里做。
# 注意：**账号必须在 LoginSrv 启动前建好**（它在启动时把账号读进内存，之后新建的看不到）。
if ($Prefix -ne "" -and $Count -gt 0) {
  $now = [long]([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())
  # 序号从 0 起（与 BotSrv 的 LoginAccount 前缀+序号约定一致：loadbot0, loadbot1, ...）
  Invoke-Mysql ("INSERT INTO $Db.account " +
                "(Account,PassWord,PayMode,Seconds,State,CreateTime,ModifyTime,LastLoginTime) " +
                "SELECT CONCAT('$Prefix', s.seq-1), CONCAT('$Prefix', s.seq-1), 0,0,0,$now,$now,0 " +
                "FROM $Db.seq_1_to_$Count s " +
                "WHERE NOT EXISTS (SELECT 1 FROM $Db.account a WHERE a.Account = CONCAT('$Prefix', s.seq-1))") | Out-Null
  # account_protection：UserName/Quiz1/2 非空（否则 LoginSrv 回 SM_NEEDUPDATE_ACCOUNT）
  Invoke-Mysql ("INSERT INTO $Db.account_protection " +
                "(AccountId,UserName,Birthday,Quiz1,Answer1,Quiz2,Answer2) " +
                "SELECT a.Id, a.Account, '$Birthday', 'q1','a1','q2','a2' FROM $Db.account a " +
                "LEFT JOIN $Db.account_protection p ON p.AccountId = a.Id " +
                "WHERE p.AccountId IS NULL AND a.Account LIKE '$Prefix%'") | Out-Null
  $n = (Invoke-Mysql "SELECT COUNT(*) FROM $Db.account WHERE Account LIKE '$Prefix%'").Trim()
  $np = (Invoke-Mysql ("SELECT COUNT(*) FROM $Db.account a JOIN $Db.account_protection p " +
                       "ON p.AccountId=a.Id WHERE a.Account LIKE '$Prefix%'")).Trim()
  Write-Output "BULK_OK prefix=$Prefix count=$Count -> account=$n protection=$np（幂等：已存在的不重复建）"
  exit 0
}
if ($Account -eq "") { throw "要么给 -Account，要么给 -Prefix + -Count" }
$exists = Invoke-Mysql "SELECT Id FROM $Db.account WHERE Account='$Account'"
if ($exists) {
  Write-Output "EXISTS: $Account (Id=$exists) —— 补资料行（Quiz2 等不能为空，否则登录会被要求补填）"
  $id = "$exists".Trim()
  Invoke-Mysql ("UPDATE $Db.account_protection SET UserName='$Account', Birthday='$Birthday', " +
                "Quiz1='q1', Answer1='a1', Quiz2='q2', Answer2='a2' WHERE AccountId=$id") | Out-Null
  Write-Output "UPDATED protection: Id=$id"
  exit 0
}

$now = [long]([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())   # 列是 int(11)，存秒（13 是显示宽度不是容量）
Invoke-Mysql ("INSERT INTO $Db.account (Account,PassWord,PayMode,Seconds,State,CreateTime,ModifyTime,LastLoginTime) " +
              "VALUES ('$Account','$Password',0,0,0,$now,$now,0)") | Out-Null
$id = (Invoke-Mysql "SELECT Id FROM $Db.account WHERE Account='$Account'").Trim()
# account_protection 的 UserName/Quiz2 必须非空：
# LoginSrv AccountLogin 里 UserName 或 Quiz2 为空会回 SM_NEEDUPDATE_ACCOUNT（客户端弹「新的帐户」补填表单），
# 且 §8.1 记录过缺整行会「获取账号资料出错」——两件事一起防。
Invoke-Mysql ("INSERT INTO $Db.account_protection (AccountId,UserName,Birthday,Quiz1,Answer1,Quiz2,Answer2) " +
              "VALUES ($id,'$Account','$Birthday','q1','a1','q2','a2')") | Out-Null
Write-Output "CREATED: $Account (Id=$id, 密码=明文'$Password', 已带完整 account_protection 行)"
