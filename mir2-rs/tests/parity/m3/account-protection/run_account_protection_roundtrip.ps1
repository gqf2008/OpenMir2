#requires -Version 5
<#
.SYNOPSIS
  account_protection_roundtrip 窗口执行器（R 级，audit #14）。

.DESCRIPTION
  跑 `login_server::tests::account_protection_roundtrip`（默认 `#[ignore]`），
  把 stdout 落到证据目录并算 sha256。

  工具自己强制：
    * **库名守卫**：DSN 的 `database`/`db` 必须恰好是 `mir2_test_account`；
      否则在任何 SQL / cargo test 之前拒绝运行（rc=2）。
    * **不连生产库**：`mir2_account` / `mir2_db` / `mir2_data` 一律拒绝。
    * 用例本身自建自删账号 `acctprot01`（account + account_protection 两行）。

.PARAMETER Dsn
  MySQL Connector 风格连接串；缺省取 `$env:MIR2_ACCOUNT_TEST_DSN`。

.PARAMETER OutDir
  证据目录；给了就把 stdout/stderr tee 到 `account_protection_roundtrip.log`
  并写 `*.log.sha256`。

.PARAMETER CheckOnly
  只做库名守卫 + 回显期望列与 cargo 命令，不连库、不跑 cargo（rc=0/2）。

.PARAMETER Selftest
  只跑库名守卫的合成用例（不连库）：接受 `mir2_test_account`，
  拒绝空库名 / 生产库 / 前缀相近的库名。

.EXAMPLE
  $env:MIR2_ACCOUNT_TEST_DSN = "server=127.0.0.1;uid=root;pwd=;database=mir2_test_account;"
  powershell -ExecutionPolicy Bypass -File run_account_protection_roundtrip.ps1 -CheckOnly

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_account_protection_roundtrip.ps1 `
    -OutDir E:/tmp/<window>/account-protection

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_account_protection_roundtrip.ps1 -Selftest
#>
param(
  [string]$Dsn = $env:MIR2_ACCOUNT_TEST_DSN,
  [string]$OutDir = "",
  [switch]$CheckOnly,
  [switch]$Selftest,
  [string]$Cargo = "cargo"
)
$ErrorActionPreference = "Stop"
$ExpectedDb = "mir2_test_account"

function Get-DsnDatabase([string]$Text) {
  $db = $null
  foreach ($seg in ($Text -split ';')) {
    if ($seg -match '^\s*(?i:database|db)\s*=\s*(.*?)\s*$') { $db = $Matches[1] }
  }
  return $db
}

function Assert-DbGuard([string]$Text) {
  if ([string]::IsNullOrWhiteSpace($Text)) {
    throw "库名守卫：DSN 为空；拒绝运行（未执行任何 SQL/cargo test）。"
  }
  $db = Get-DsnDatabase $Text
  if ($db -ne $ExpectedDb) {
    throw ("库名守卫：database 必须恰为 '$ExpectedDb'（收到 '$db'）；" +
           "拒绝运行，未执行任何 SQL/cargo test。")
  }
  return $db
}

if ($Selftest) {
  $cases = @(
    @{ dsn = "server=127.0.0.1;uid=root;pwd=;database=mir2_test_account;"; ok = $true },
    @{ dsn = "Server=127.0.0.1;UID=root;PWD=;Db=mir2_test_account;";              ok = $true },
    @{ dsn = "server=127.0.0.1;uid=root;pwd=;database=mir2_account;";             ok = $false },
    @{ dsn = "server=127.0.0.1;uid=root;pwd=;database=mir2_test_account_extra;";  ok = $false },
    @{ dsn = "server=127.0.0.1;uid=root;pwd=;database=mir2_test_db;";             ok = $false },
    @{ dsn = "";                                                                 ok = $false }
  )
  $fail = 0
  foreach ($c in $cases) {
    $db = if ([string]::IsNullOrWhiteSpace($c.dsn)) { $null } else { Get-DsnDatabase $c.dsn }
    $got = ($db -eq $ExpectedDb)
    if ($got -eq $c.ok) {
      Write-Output ("{0,-72} OK" -f $c.dsn)
    } else {
      Write-Output ("{0,-72} FAIL db={1}" -f $c.dsn, $db)
      $fail++
    }
  }
  if ($fail -gt 0) { exit 1 }
  Write-Output "RESULT: GREEN guard selftest"
  exit 0
}

Assert-DbGuard $Dsn | Out-Null

$workspace = Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")
if (-not (Test-Path (Join-Path $workspace "Cargo.toml"))) {
  throw "找不到 workspace：$workspace\Cargo.toml"
}

$spec = Get-Content (Join-Path $PSScriptRoot "expected_columns.json") -Raw | ConvertFrom-Json
$cargoArgs = @(
  "test", "-p", "login-srv", "--bin", "login-srv",
  "account_protection_roundtrip", "--", "--ignored", "--nocapture"
)

Write-Output "[guard] database=$ExpectedDb OK"
Write-Output ("[expected] INSERT: " + (($spec.insert_columns | ForEach-Object { $_.column }) -join ", "))
Write-Output ("[expected] UPDATE: " + (($spec.update_columns | ForEach-Object { $_.column }) -join ", "))
Write-Output ("[cmd] {0} {1}" -f $Cargo, ($cargoArgs -join " "))

if ($CheckOnly) {
  Write-Output "RESULT: CHECK-ONLY (no DB, no cargo test)"
  exit 0
}

$env:MIR2_ACCOUNT_TEST_DSN = $Dsn
Push-Location $workspace
try {
  if ([string]::IsNullOrWhiteSpace($OutDir)) {
    & $Cargo @cargoArgs
    $rc = $LASTEXITCODE
  } else {
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
    $log = Join-Path $OutDir "account_protection_roundtrip.log"
    & $Cargo @cargoArgs 2>&1 | Tee-Object -FilePath $log
    $rc = $LASTEXITCODE
    $hash = Get-FileHash -Algorithm SHA256 -Path $log
    $line = "{0}  {1}" -f $hash.Hash.ToLowerInvariant(), [IO.Path]::GetFileName($log)
    Set-Content -Path ($log + ".sha256") -Value $line -Encoding ASCII
    Write-Output "[evidence] $log"
    Write-Output "[evidence] $log.sha256"
  }
} finally {
  Pop-Location
}
exit $rc
