# tools/hygiene/assert_three_way_aligned.ps1
# 三处对齐断言（卡 `openmir2-entry-gap-buckets` 复审；卡 `openmir2-three-way-embedded-rev` 扩判据）：
# **主检出 / 影子 / 本次构建** 三份 DBSrv 产物必须 sha 逐个相等，且**戳 head == 二进制内嵌的
# SourceRevisionId**（每个目录各自自洽、三处戳 head 也相等）。
#
# 为什么要内嵌判据（2026-11-17 botfleet 抓到的「戳在说谎」）：C# 构建把 `SourceRevisionId`
# （= 构建时的 git HEAD）编进程序集；而 MSBuild 增量构建会**跳过源码未变的项目**——曾出现
# 三件二进制内嵌 dcfc57ade/c4c79d7d0 两个不同祖先、戳却统一写 37adb4d39。只比 sha 三处相等
# 抓不到这种「stamp 与二进制脱节」，必须把内嵌值也核进来。
#
# 口径与 `E:\UserFleet\assert_three_way_aligned.py`（botfleet 版，A=sha/B=戳 head 相等/C=戳 head
# 属于内嵌）**统一**：**本脚本是超集**——多查 build 份（三处而非两处）与「每个目录各自的内嵌」；
# botfleet 版另有 `--allow-src-unsynced`（主检出未同步降为 rc=2 的三态），本脚本无此降级（一律 rc=1）。
#
# 退出码：0 = 全对齐；1 = 有红（sha 不等 / 戳缺失 / 戳 head 不等 / 戳 head 不在内嵌里）；
#        2 = 判不了（路径缺失/用法错）——绝不静默通过。
# 机器可读输出：`THREE_WAY_OK <file> sha=<hex> stamp=<head> embedded=ok`
#              / `THREE_WAY_RED <file> reason=<sha_mismatch|stamp_missing|stamp_head_mismatch|embedded_mismatch> ...`
#              / `THREE_WAY_INCOMPLETE <file> missing=<main|shadow|build>`
#
# 用法：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/hygiene/assert_three_way_aligned.ps1 `
#     -Build E:\Users\gxh\Documents\GitHub\OpenMir2\.worktrees\<branch>\src\DBSrv\bin\Release
#   省略 -Build ⇒ 以 -Main 充当「本次构建」（只验 主检出 == 影子，但戳/内嵌判据照跑）。
#   `-Group DBSrv|LoginSrv` 选产物组（默认 DBSrv）：每组自带清单与默认目录；
#   本轮起 **LoginSrv 也纳入**（`runtime/LoginSrv` 曾被漏在 provenance 之外）。
# 自测：`-Selftest`（合成临时目录：1 正例 + 5 反例；含「三处 sha 相同、戳 head 也相同，但那个
#       head 不在二进制内嵌里」——只比 sha 的旧判据会误判成全对齐）。
param(
  [ValidateSet('DBSrv', 'LoginSrv')]
  [string]$Group = 'DBSrv',
  [string]$Main = '',
  [string]$Shadow = '',
  [string]$Build = '',
  [switch]$Selftest
)
# 产物组：每组一份「要比对的二进制清单」与默认的主检出/影子目录（卡 `openmir2-loginsrv-threeway`）。
$Groups = @{
  DBSrv = @{
    Files = @('DBSrv.exe', 'DBSrv.dll', 'DBSrv.Storage.MySQL.dll')
    Main = 'E:\Users\gxh\Documents\GitHub\OpenMir2\src\DBSrv\bin\Release'
    Shadow = 'E:\UserFleet\m1-stack\runtime\DBSrv'
  }
  LoginSrv = @{
    Files = @('LoginSrv.exe', 'LoginSrv.dll')
    Main = 'E:\Users\gxh\Documents\GitHub\OpenMir2\src\LoginSrv\bin\Release'
    Shadow = 'E:\UserFleet\m1-stack\runtime\LoginSrv'
  }
}
if ([string]::IsNullOrWhiteSpace($Main)) { $Main = $Groups[$Group].Main }
if ([string]::IsNullOrWhiteSpace($Shadow)) { $Shadow = $Groups[$Group].Shadow }
$Files = $Groups[$Group].Files
# 内嵌 40-hex 扫描：C# 程序集里字面量可能是 ASCII，也可能是 UTF-16-LE（AssemblyInformationalVersion）
$HexRe = [regex]'[0-9a-f]{40}'

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

function Get-StampHead([string]$dir, [string]$name) {
  $sp = Join-Path $dir ($name + '.build.json')
  if (-not (Test-Path -LiteralPath $sp -PathType Leaf)) { return $null }
  try {
    $json = Get-Content -LiteralPath $sp -Raw -Encoding UTF8 | ConvertFrom-Json
  } catch {
    return $null
  }
  if ($null -eq $json.head) { return $null }
  $h = ([string]$json.head).Trim().ToLowerInvariant()
  if ($h.Length -eq 0) { return $null }
  return $h
}

function Get-EmbeddedRevisions([string]$path) {
  $bytes = [System.IO.File]::ReadAllBytes($path)
  $set = New-Object 'System.Collections.Generic.HashSet[string]'
  # PS 5.1 没有 `[Text.Encoding]::Latin1`（.NET 5+ 才有）；ISO-8859-1(28591) 等价于「字节→字符」一一映射
  $ascii = [System.Text.Encoding]::GetEncoding(28591).GetString($bytes)
  foreach ($m in $HexRe.Matches($ascii)) { [void]$set.Add($m.Value) }
  $utf16 = [System.Text.Encoding]::Unicode.GetString($bytes)
  foreach ($m in $HexRe.Matches($utf16)) { [void]$set.Add($m.Value) }
  return ,$set
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
    if (-not ($m -eq $s -and $s -eq $b)) {
      [Console]::Out.WriteLine(("THREE_WAY_RED {0} reason=sha_mismatch main={1} shadow={2} build={3}" -f $f, $m.Substring(0, 12), $s.Substring(0, 12), $b.Substring(0, 12)))
      $problems++
      continue
    }
    # ② 戳：三处都必须有，且三处 head 相同
    $tm = Get-StampHead $MainDir $f
    $ts = Get-StampHead $ShadowDir $f
    $tb = Get-StampHead $BuildDir $f
    if ($null -eq $tm -or $null -eq $ts -or $null -eq $tb) {
      $noStamp = @()
      if ($null -eq $tm) { $noStamp += 'main' }
      if ($null -eq $ts) { $noStamp += 'shadow' }
      if ($null -eq $tb) { $noStamp += 'build' }
      [Console]::Out.WriteLine(("THREE_WAY_RED {0} reason=stamp_missing dirs={1}" -f $f, ($noStamp -join ',')))
      $problems++
      continue
    }
    if (-not ($tm -eq $ts -and $ts -eq $tb)) {
      [Console]::Out.WriteLine(("THREE_WAY_RED {0} reason=stamp_head_mismatch main={1} shadow={2} build={3}" -f $f, $tm.Substring(0, 12), $ts.Substring(0, 12), $tb.Substring(0, 12)))
      $problems++
      continue
    }
    # ③ 内嵌：每处「戳 head」必须出现在**该处二进制**的内嵌 40-hex 集合里。
    #    只查一处会漏掉「影子没同步、主检出对」；只查三处相等会漏掉「戳与二进制脱节」。
    $embeddedBad = @()
    foreach ($pair in @(@('main', $MainDir), @('shadow', $ShadowDir), @('build', $BuildDir))) {
      $label = $pair[0]
      $dir = $pair[1]
      $set = Get-EmbeddedRevisions (Join-Path $dir $f)
      if (-not $set.Contains($tm)) { $embeddedBad += $label }
    }
    if ($embeddedBad.Count -gt 0) {
      [Console]::Out.WriteLine(("THREE_WAY_RED {0} reason=embedded_mismatch stamp={1} dirs={2}" -f $f, $tm, ($embeddedBad -join ',')))
      $problems++
      continue
    }
    [Console]::Out.WriteLine(("THREE_WAY_OK {0} sha={1} stamp={2} embedded=ok" -f $f, $m, $tm))
  }
  if ($problems -gt 0) { return 1 }
  if ($incomplete -gt 0) { return 2 }
  return 0
}

if ($Selftest) {
  # 两组各跑一遍（卡 `openmir2-loginsrv-threeway`）：每组都是 1 正例 + 5 反例，
  # 反例与 DBSrv 组同构——证明「清单换了判据不松」，不是只对 DBSrv 有牙。
  $allOk = $true
  foreach ($g in @('DBSrv', 'LoginSrv')) {
    $Files = $Groups[$g].Files
    $f0 = $Files[0]
    $f1 = $Files[1]
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('three-way-selftest-' + $g + '-' + [guid]::NewGuid().ToString('N'))
    try {
      function New-FixtureDir([string]$path, [string]$tag, [string]$head) {
        New-Item -ItemType Directory -Force -Path $path | Out-Null
        foreach ($f in $Files) {
          $bytes = [System.Text.Encoding]::ASCII.GetBytes("BIN $tag $f $head")
          [System.IO.File]::WriteAllBytes((Join-Path $path $f), $bytes)
          $stamp = @{ format = 'mir2-build-stamp/1'; head = $head } | ConvertTo-Json -Compress
          [System.IO.File]::WriteAllText((Join-Path $path ($f + '.build.json')), $stamp)
        }
      }
      $headA = ('a' * 40)
      $a = Join-Path $tmp 'main'
      $c = Join-Path $tmp 'shadow'
      $d = Join-Path $tmp 'build'
      New-FixtureDir $a 'A' $headA
      New-FixtureDir $c 'A' $headA
      New-FixtureDir $d 'A' $headA
      $rc1 = Assert-ThreeWay $a $c $d
      Write-Output ("selftest[{0}]: aligned rc={1}" -f $g, $rc1)

      # 反例①：改一份（sha 不等）⇒ 1
      [System.IO.File]::WriteAllBytes((Join-Path $d $f1), (New-Object byte[] 77))
      $rc2 = Assert-ThreeWay $a $c $d
      Write-Output ("selftest[{0}]: one-changed rc={1}" -f $g, $rc2)
      Copy-Item -LiteralPath (Join-Path $a $f1) -Destination (Join-Path $d $f1) -Force

      # 反例②：删一份 ⇒ 2（「缺失」优先于其他判据：先还原上一步的改动）
      Remove-Item -LiteralPath (Join-Path $c $f0)
      $rc3 = Assert-ThreeWay $a $c $d
      Write-Output ("selftest[{0}]: missing rc={1}" -f $g, $rc3)
      New-FixtureDir $c 'A' $headA

      # 反例③：三处各自自洽（sha 相等、戳与内嵌一致）但彼此不同 ⇒ 1
      $e = Join-Path $tmp 'build2'
      New-FixtureDir $e 'E' ('e' * 40)
      $rc4 = Assert-ThreeWay $a $c $e
      Write-Output ("selftest[{0}]: three-self-consistent-but-different rc={1}" -f $g, $rc4)

      # 反例④（本题教训，2026-11-17「戳在说谎」）：三处 sha 全相同、三处戳 head 也相同，但那个
      # head **不在任何一处二进制内嵌里** ⇒ 1。只比 sha 的旧判据会误判成全对齐。
      $notEmbedded = ('b' * 40)
      $stampBad = @{ format = 'mir2-build-stamp/1'; head = $notEmbedded } | ConvertTo-Json -Compress
      foreach ($dir in @($a, $c, $d)) {
        foreach ($f in $Files) {
          [System.IO.File]::WriteAllText((Join-Path $dir ($f + '.build.json')), $stampBad)
        }
      }
      $rc5 = Assert-ThreeWay $a $c $d
      Write-Output ("selftest[{0}]: stamp-head-not-embedded rc={1}" -f $g, $rc5)
      foreach ($dir in @($a, $c, $d)) {
        New-FixtureDir $dir 'A' $headA
      }

      # 反例⑤：戳缺失 ⇒ 1（缺戳就是红，不当作「判不了」）
      Remove-Item -LiteralPath (Join-Path $c ($f1 + '.build.json'))
      $rc6 = Assert-ThreeWay $a $c $d
      Write-Output ("selftest[{0}]: stamp-missing rc={1}" -f $g, $rc6)

      $groupOk = ($rc1 -eq 0 -and $rc2 -eq 1 -and $rc3 -eq 2 -and $rc4 -eq 1 -and $rc5 -eq 1 -and $rc6 -eq 1)
      Write-Output ("selftest[{0}]: {1}" -f $g, $(if ($groupOk) { 'GREEN' } else { 'RED' }))
      $allOk = $allOk -and $groupOk
    } finally {
      if (Test-Path -LiteralPath $tmp) { try { Remove-Item -LiteralPath $tmp -Recurse -Force } catch { } }
    }
  }
  if ($allOk) {
    Write-Output 'three-way selftest: GREEN (DBSrv + LoginSrv，各 1 正 + 5 负)'
    exit 0
  }
  Write-Output 'three-way selftest: RED'
  exit 1
}
if ([string]::IsNullOrWhiteSpace($Build)) { $Build = $Main }
$rc = Assert-ThreeWay $Main $Shadow $Build
exit $rc
