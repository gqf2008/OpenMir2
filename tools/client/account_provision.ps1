#requires -Version 5
<#
.SYNOPSIS
  account_provision.ps1 — 测试账号开号（建角流程的 scratch 账号用）。

.DESCRIPTION
  在 mir2_account 库里建一个可用账号，**account 与 account_protection 两张表都要写**：
  设计文档 §8.1 记录过——LoginSrv 用 account INNER JOIN account_protection，
  缺 protection 行时会报「获取账号资料出错」。密码按本工程约定存明文（行为等价优先）。

  幂等：账号已存在时只报告，不重复插入。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File account_provision.ps1 -Account goldprobe
#>
param(
  [Parameter(Mandatory = $true)][string]$Account,
  [string]$Password = "",
  [string]$Birthday = "1986/06/06",
  [string]$Mysql = "D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe"
)
$ErrorActionPreference = "Stop"
if ($Password -eq "") { $Password = $Account }

function Invoke-Mysql([string]$Sql) {
  # mysql.exe 会往 stderr 打横幅/分隔线；ErrorActionPreference=Stop 下会被当成终止错误
  $prev = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $out = & $Mysql -h 127.0.0.1 -u root --batch --skip-column-names -e $Sql 2>&1 |
      Where-Object { $_ -notmatch '^-+$' }
    if ($LASTEXITCODE -ne 0) { throw "mysql 失败: $out" }
    return $out
  } finally { $ErrorActionPreference = $prev }
}

$exists = Invoke-Mysql "SELECT Id FROM mir2_account.account WHERE Account='$Account'"
if ($exists) {
  Write-Output "EXISTS: $Account (Id=$exists) —— 补资料行（Quiz2 等不能为空，否则登录会被要求补填）"
  $id = "$exists".Trim()
  Invoke-Mysql ("UPDATE mir2_account.account_protection SET UserName='$Account', Birthday='$Birthday', " +
                "Quiz1='q1', Answer1='a1', Quiz2='q2', Answer2='a2' WHERE AccountId=$id") | Out-Null
  Write-Output "UPDATED protection: Id=$id"
  exit 0
}

$now = [long]([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())   # 列是 int(11)，存秒（13 是显示宽度不是容量）
Invoke-Mysql ("INSERT INTO mir2_account.account (Account,PassWord,PayMode,Seconds,State,CreateTime,ModifyTime,LastLoginTime) " +
              "VALUES ('$Account','$Password',0,0,0,$now,$now,0)") | Out-Null
$id = (Invoke-Mysql "SELECT Id FROM mir2_account.account WHERE Account='$Account'").Trim()
# account_protection 的 UserName/Quiz2 必须非空：
# LoginSrv AccountLogin 里 UserName 或 Quiz2 为空会回 SM_NEEDUPDATE_ACCOUNT（客户端弹「新的帐户」补填表单），
# 且 §8.1 记录过缺整行会「获取账号资料出错」——两件事一起防。
Invoke-Mysql ("INSERT INTO mir2_account.account_protection (AccountId,UserName,Birthday,Quiz1,Answer1,Quiz2,Answer2) " +
              "VALUES ($id,'$Account','$Birthday','q1','a1','q2','a2')") | Out-Null
Write-Output "CREATED: $Account (Id=$id, 密码=明文'$Password', 已带完整 account_protection 行)"
