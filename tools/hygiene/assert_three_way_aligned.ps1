# tools/hygiene/assert_three_way_aligned.ps1
# 三处对齐断言（卡 `openmir2-entry-gap-buckets` 复审）：**主检出 / 影子 / 本次构建** 三份 DBSrv 产物
# 必须 sha 逐个相等。判据是 **sha 相等**，不是 `-Check GREEN`——`-Check` 只证明「戳与它旁边的文件自洽」，
# 旧的戳配旧的文件同样会 GREEN（2026-11-17 两次误报「三处对齐」都是这么来的）。
#
# 退出码：0 = 三处逐个相等；1 = 有文件 sha 不等（逐个点名）；2 = 判不了（路径缺失/用法错）——绝不静默通过。
# 机器可读输出：`THREE_WAY_OK <file> sha=<hex>` / `THREE_WAY_RED <file> main=<..> shadow=<..> build=<..>`
#              / `THREE_WAY_INCOMPLETE <file> missing=<main|shadow|build>`
#
# 用法：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/hygiene/assert_three_way_aligned.ps1 `
#     -Build E:\Users\gxh\Documents\GitHub\OpenMir2\.worktrees\<branch>\src\DBSrv\bin\Release
#   省略 -Build ⇒ 以 -Main 充当「本次构建」（只验 主检出 == 影子）。
# 自测：`-Selftest`（合成临时目录：三处相同 ⇒ 0；改一份 ⇒ 1；缺文件 ⇒ 2）。
param(
  [string]$Main = 'E:\Users\gxh\Documents\GitHub\OpenMir2\src\DBSrv\bin\Release',
  [string]$Shadow = 'E:\UserFleet\m1-stack\runtime\DBSrv',
  [string]$Build = '',
  [switch]$Selftest
)
$ErrorActionPreference = 'Stop'
$Files = @('DBSrv.exe', 'DBSrv.dll', 'DBSrv.Storage.MySQL.dll')

function Get-Sha256Hex([string]$path) {
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $null }
  $sha = [System.Security.Cryptography.SHA256]::Create()
  $fs = $null
  try {
    $fs = [System.IO.File]::OpenRead($path)
    $bytes = $sha.ComputeHash($fs)
  } finally {
    if ($null -ne $fs) { $fs.Dispose() }
    $sha.Dispose()
  }
  return (($bytes | ForEach-Object { $_.ToString('x2') }) -join '')
}

function Assert-ThreeWay([string]$MainDir, [string]$ShadowDir, [string]$BuildDir) {
  $problems = 0
  $incomplete = 0
  foreach ($f in $Files) {
    $m = Get-Sha256Hex (Join-Path $MainDir $f)
    $s = Get-Sha256Hex (Join-Path $ShadowDir $f)
    $b = Get-Sha256Hex (Join-Path $BuildDir $f)
    if ($null -eq $m -or $null -eq $s -or $null -eq $b) {
      $missing = @()
      if ($null -eq $m) { $missing += 'main' }
      if ($null -eq $s) { $missing += 'shadow' }
      if ($null -eq $b) { $missing += 'build' }
      [Console]::Out.WriteLine(("THREE_WAY_INCOMPLETE {0} missing={1}" -f $f, ($missing -join ',')))
      $incomplete++
      continue
    }
    if ($m -eq $s -and $s -eq $b) {
      [Console]::Out.WriteLine(("THREE_WAY_OK {0} sha={1}" -f $f, $m))
    } else {
      [Console]::Out.WriteLine(("THREE_WAY_RED {0} main={1} shadow={2} build={3}" -f $f, $m.Substring(0, 12), $s.Substring(0, 12), $b.Substring(0, 12)))
      $problems++
    }
  }
  if ($problems -gt 0) { return 1 }
  if ($incomplete -gt 0) { return 2 }
  return 0
}

if ($Selftest) {
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('three-way-selftest-' + [guid]::NewGuid().ToString('N'))
  try {
    $a = Join-Path $tmp 'main'; $c = Join-Path $tmp 'shadow'; $d = Join-Path $tmp 'build'
    foreach ($p in @($a, $c, $d)) { New-Item -ItemType Directory -Force -Path $p | Out-Null }
    $i = 1
    foreach ($f in $Files) {
      $bytes = New-Object byte[] (1024 * $i)
      for ($k = 0; $k -lt $bytes.Length; $k++) { $bytes[$k] = [byte](($k + $i) % 251) }
      foreach ($p in @($a, $c, $d)) { [System.IO.File]::WriteAllBytes((Join-Path $p $f), $bytes) }
      $i++
    }
    $rc1 = Assert-ThreeWay $a $c $d
    Write-Output ("selftest: aligned rc={0}" -f $rc1)

    # 反例①：改一份 ⇒ 1
    [System.IO.File]::WriteAllBytes((Join-Path $d 'DBSrv.dll'), (New-Object byte[] 77))
    $rc2 = Assert-ThreeWay $a $c $d
    Write-Output ("selftest: one-changed rc={0}" -f $rc2)

    # 反例②：删一份 ⇒ 2（先把上一反例改动还原，否则「不等」会优先于「缺失」返回 1）
    Copy-Item -LiteralPath (Join-Path $a 'DBSrv.dll') -Destination (Join-Path $d 'DBSrv.dll') -Force
    Remove-Item -LiteralPath (Join-Path $c 'DBSrv.exe')
    $rc3 = Assert-ThreeWay $a $c $d
    Write-Output ("selftest: missing rc={0}" -f $rc3)

    # 反例③（本题教训）：**三处各自自洽但彼此不同**——只比 sha 才抓得到；`-Check` 会全绿。
    # 这里用「全不相同」的合成目录直接断言 rc=1，证明判据是相等而不是自洽。
    $e = Join-Path $tmp 'build2'; New-Item -ItemType Directory -Force -Path $e | Out-Null
    $i = 9
    foreach ($f in $Files) {
      $bytes = New-Object byte[] (512 * $i)
      for ($k = 0; $k -lt $bytes.Length; $k++) { $bytes[$k] = [byte](($k * 3 + $i) % 251) }
      [System.IO.File]::WriteAllBytes((Join-Path $e $f), $bytes)
      $i++
    }
    $rc4 = Assert-ThreeWay $a $c $e
    Write-Output ("selftest: three-self-consistent-but-different rc={0}" -f $rc4)

    if ($rc1 -eq 0 -and $rc2 -eq 1 -and $rc3 -eq 2 -and $rc4 -eq 1) {
      Write-Output 'three-way selftest: GREEN (1 positive + 3 negative)'
      exit 0
    }
    Write-Output 'three-way selftest: RED'
    exit 1
  } finally {
    if (Test-Path -LiteralPath $tmp) { try { Remove-Item -LiteralPath $tmp -Recurse -Force } catch { } }
  }
}

if ([string]::IsNullOrWhiteSpace($Build)) { $Build = $Main }
$rc = Assert-ThreeWay $Main $Shadow $Build
exit $rc
