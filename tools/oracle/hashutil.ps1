#requires -Version 5
<#
  hashutil.ps1 — 环境无关的 SHA256（tools/oracle 共用）。

  为什么不用 `Get-FileHash`：它属于 `Microsoft.PowerShell.Utility`。当父进程（如 PS7 lane）
  把 `PSModulePath` 指成 PS7 优先时，5.1 子进程会加载 PS7 的 Utility（该模块**不导出**
  `Get-FileHash`）⇒ 脚本在 `$ErrorActionPreference = 'Stop'` 下抛 `CommandNotFoundException`
  崩溃；同一脚本因而「按环境变色」（本机 lane 实况，见
  `tools/m2-acceptance/evidence/m2svr-preflight-modulepath-20261012/` 与
  `tools/oracle/evidence/m2svr-hash-unify-20261012/`）。

  `[System.Security.Cryptography.SHA256]` 不依赖模块解析，结果与
  `(Get-FileHash -LiteralPath <path> -Algorithm SHA256).Hash` 逐字节相同（同一修法见
  `tools/m2-acceptance/preflight.ps1:126`）。

  红检（`-SelfTestRed`）：`Test-HashUtilSelfTest` 钉住标准 SHA256（对固定 64 字节序列
  0x00..0x3F = fdeab9ac…1108）并验证「同内容同 hash / 翻一位必不同 / 16 位截断 = 前 16 位」。
  任何一条不成立即返回 `$false`（调用方判红）；换成常量、换算法（如 MD5）、或改回
  `Get-FileHash`（在污染环境里）都会红。
#>

function Get-Sha256Hex([string]$Path) {
  if ([string]::IsNullOrEmpty($Path)) { throw 'Get-Sha256Hex: 路径为空' }
  if (-not (Test-Path -LiteralPath $Path)) { throw "Get-Sha256Hex: 文件不存在: $Path" }
  $alg = [System.Security.Cryptography.SHA256]::Create()
  try {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    return ([System.BitConverter]::ToString($alg.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant()
  } finally { $alg.Dispose() }
}

function Get-Sha256Hex16([string]$Path) {
  return (Get-Sha256Hex $Path).Substring(0, 16)
}

# 64 字节 0x00..0x3F 的标准 SHA256 —— 用作「算法/实现没被换掉」的锚点。
$script:Sha256SelfTestAnchor = 'fdeab9acf3710362bd2658cdc9a29e8f9c757fcf9811603a8c447cd1d9151108'

function Test-HashUtilSelfTest {
  $ok = $true
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('hashutil-selftest-' + [System.Guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  try {
    $bytes = (New-Object byte[] 64)
    for ($i = 0; $i -lt 64; $i++) { $bytes[$i] = [byte]$i }
    $a = Join-Path $tmp 'a.bin'
    $b = Join-Path $tmp 'b.bin'
    $c = Join-Path $tmp 'c.bin'
    [System.IO.File]::WriteAllBytes($a, $bytes)
    [System.IO.File]::WriteAllBytes($b, $bytes)
    $bytes[32] = $bytes[32] -bxor 0xff
    [System.IO.File]::WriteAllBytes($c, $bytes)

    $ha = Get-Sha256Hex $a
    $hb = Get-Sha256Hex $b
    $hc = Get-Sha256Hex $c

    if ($ha -ne $script:Sha256SelfTestAnchor) {
      $ok = $false
      Write-Host ("  [RED] 钉住的标准 SHA256 不符: {0} != {1}" -f $ha, $script:Sha256SelfTestAnchor)
    } else {
      Write-Host ("  [OK ] 钉住的标准 SHA256 = {0}" -f $ha)
    }
    if ($ha -ne $hb) {
      $ok = $false
      Write-Host '  [RED] 同内容却算出不同 hash（比较器不可信）'
    } else {
      Write-Host '  [OK ] 同内容同 hash'
    }
    if ($ha -eq $hc) {
      $ok = $false
      Write-Host '  [RED] 翻一位后 hash 不变（对篡改不敏感 ⇒ 门禁失效）'
    } else {
      Write-Host '  [OK ] 翻一位必不同 hash'
    }
    if ((Get-Sha256Hex16 $a) -ne $ha.Substring(0, 16)) {
      $ok = $false
      Write-Host '  [RED] 16 位截断不等于前 16 位'
    } else {
      Write-Host '  [OK ] 16 位截断 = 前 16 位'
    }
  } finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  }
  return $ok
}
