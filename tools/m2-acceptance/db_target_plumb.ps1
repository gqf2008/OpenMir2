#requires -Version 5
<#
.SYNOPSIS
  db_target_plumb.ps1 — 栈启动器「把服务端指向测试库（mir2_test_*）」的纯函数与自检（run sheet C10#8）。

.DESCRIPTION
  `start_rust_stack.ps1` 与 `tools/m5-acceptance/start_full_rust_stack.ps1` 共用本文件。目标只有一个：
  在**不修改** `E:\MirServer`、**不修改**主检出、**不修改**生产库的前提下，把 DBSrv/LoginSrv/game-svr
  的连接串指向克隆库 `mir2_test_*`。

  设计纪律：
    · **未给任何 -Db* 参数时，本文件的函数一律不参与起栈**（调用方零调用）⇒ 默认路径逐字节不变。
    · 改写 INI 用 ISO-8859-1（Latin1）解码/编码：GBK 字节、BOM、CRLF 全部 1:1 保留，只替换
      `[DataBase]` 段里 `ConnctionString` 的**值**（键名/大小写/顺序原样）。缺段/缺键/重复键 ⇒ 抛错
      （fail-closed，调用方打印 RED 并 exit 1），绝不静默写一个「以为指向测试库」的栈。
    · 连接串按 `;` 分段、按 `key=value` 首 `=` 拆；只改给定 key，未给的 key 原样保留；缺 key 时追加。
    · C# DBSrv/LoginSrv 的配置固定从 `AppContext.BaseDirectory` 读（`AppConfigManager.cs:6` /
      `AppServer.cs:29`）⇒ 只有在需要改库时才把整个 Release 目录**复制**到影子目录，再从影子 exe 启动。
    · game-svr 的 BasePath = Server.conf 所在目录（`mapwire.rs:94-102`，还要找 Envir/Map/Notice）⇒ 影子
      目录里 `Map/` 以 Junction **只读**挂载（不复制 ~195MB），Envir/日志/存档等其余目录与顶层文件
      一律复制，Server.conf 复制后改写；生成后 fail-closed 断言除 `Map` 外无 Junction/硬链
      （硬链与源同 inode，写影子 = 写源）。

  公开函数：
    Resolve-Mir2DbTarget            -DbHost/-DbPort/-DbUser/-DbPassword/-DbName → 目标对象（含 Present）
    Expand-Mir2DbName               -Target -Component db|account|data → 具体库名（未给 Name 时 $null）
    Update-Mir2ConnString           -ConnString [-Host][-Port][-User][-Password][-Database] → 新连接串
    Get-Mir2IniConnString           -Path → 现配置里的连接串（只读）
    Set-Mir2IniConnString           -Path -ConnString → 原地字节保真改写
    New-Mir2ShadowAppDir            -SourceDir -DestDir → 复制整个 C# 应用目录
    New-Mir2GameSvrShadowBase       -SourceDir -DestDir -ConnString [-LinkMode ReadOnlyData|Copy]
    Assert-Mir2ShadowNoWritableLinks -DestDir [-ReadOnlyDirs] → 影子内可写入口不得是链接

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/db_target_plumb.ps1 -Selftest
#>
param([switch]$Selftest)

function Get-Mir2Latin1 { return [System.Text.Encoding]::GetEncoding(28591) }

function Resolve-Mir2DbTarget {
  param(
    [string]$DbHost = '',
    [string]$DbPort = '',
    [string]$DbUser = '',
    [string]$DbPassword = '',
    [string]$DbName = ''
  )
  $present = @(@($DbHost, $DbPort, $DbUser, $DbPassword, $DbName) |
               Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count -gt 0
  $t = [pscustomobject]@{
    Present      = $present
    Host         = $null
    Port         = $null
    User         = $null
    Password     = $null
    NameTemplate = $null
  }
  if (-not $present) { return $t }

  foreach ($pair in @(@('Host', $DbHost), @('User', $DbUser), @('Password', $DbPassword))) {
    $field = $pair[0]
    $value = $pair[1]
    if ([string]::IsNullOrWhiteSpace($value)) { continue }
    $value = $value.Trim()
    if ($value -match '[;\r\n]') {
      throw ("-Db{0} 取值含 ';' 或换行 —— 无法安全写入 MySQL 连接串（fail-closed）" -f $field)
    }
    $t.$field = $value
  }

  if (-not [string]::IsNullOrWhiteSpace($DbPort)) {
    $p = 0
    if (-not [int]::TryParse($DbPort.Trim(), [ref]$p) -or $p -lt 1 -or $p -gt 65535) {
      throw ("-DbPort={0} 非法（必须是 1..65535 的整数）" -f $DbPort)
    }
    $t.Port = [string]$p
  }

  if (-not [string]::IsNullOrWhiteSpace($DbName)) {
    $n = $DbName.Trim()
    if ($n -match '[\s;=/\r\n]') {
      throw ("-DbName={0} 含空白或非法字符（只允许字母/数字/下划线，可用一个尾部 * 作模板）" -f $DbName)
    }
    if ($n -match '^([A-Za-z0-9_]+)\*$') {
      $template = $n
    } elseif ($n -match '^(?i)(.+)_(db|account|data)$') {
      # 允许传具体库名（如 mir2_test_db）：剥掉组件后缀，仍按 <前缀>_* 展开三库。
      $template = $Matches[1] + '_*'
    } elseif ($n -match '^[A-Za-z0-9_]+$') {
      $template = $n + '_*'
    } else {
      throw ("-DbName={0} 非法：只允许 <前缀>、<前缀>_* 或 <前缀>_db|_account|_data" -f $DbName)
    }
    $longest = $template.Replace('*', 'account')
    if ($longest.Length -gt 64) {
      throw ("-DbName={0} 展开后超过 MySQL 库名上限 64 字符（{1}）" -f $DbName, $longest)
    }
    $t.NameTemplate = $template
  }
  return $t
}

function Expand-Mir2DbName {
  param($Target, [string]$Component)
  if ($null -eq $Target -or -not $Target.Present) { return $null }
  if ([string]::IsNullOrEmpty($Target.NameTemplate)) { return $null }
  return $Target.NameTemplate.Replace('*', $Component)
}

function Update-Mir2ConnString {
  param(
    [string]$ConnString,
    [Alias('Host')][string]$ServerHost,
    [string]$Port,
    [string]$User,
    [string]$Password,
    [string]$Database
  )
  if ($null -eq $ConnString) { throw '连接串为空（fail-closed）' }
  $map = @{
    'server'   = $ServerHost
    'port'     = $Port
    'uid'      = $User
    'pwd'      = $Password
    'database' = $Database
  }
  $any = @(@($ServerHost, $Port, $User, $Password, $Database) |
           Where-Object { -not [string]::IsNullOrEmpty($_) }).Count -gt 0
  if (-not $any) { return $ConnString }
  foreach ($k in $map.Keys) {
    if ($null -ne $map[$k] -and $map[$k] -match '[\r\n;]') {
      throw ("连接串 {0} 取值含 ';' 或换行（fail-closed）" -f $k)
    }
  }

  $parts = $ConnString.Split(';')
  $body = New-Object System.Collections.ArrayList
  $seen = @{}
  $lastIsEmpty = ($parts.Count -gt 0 -and $parts[$parts.Count - 1] -eq '')
  for ($i = 0; $i -lt $parts.Count; $i++) {
    $seg = $parts[$i]
    if ($lastIsEmpty -and $i -eq $parts.Count - 1) { continue }
    $eq = $seg.IndexOf('=')
    if ($eq -lt 0) {
      [void]$body.Add($seg)
      continue
    }
    $key = $seg.Substring(0, $eq).Trim().ToLowerInvariant()
    if ($map.ContainsKey($key) -and -not [string]::IsNullOrEmpty($map[$key])) {
      [void]$body.Add($seg.Substring(0, $eq + 1) + $map[$key])
      $seen[$key] = $true
    } else {
      [void]$body.Add($seg)
    }
  }
  foreach ($k in @('server', 'port', 'uid', 'pwd', 'database')) {
    if (-not [string]::IsNullOrEmpty($map[$k]) -and -not $seen.ContainsKey($k)) {
      [void]$body.Add(("{0}={1}" -f $k, $map[$k]))
    }
  }
  $out = ($body -join ';')
  if ($lastIsEmpty) { $out += ';' }
  return $out
}

function Get-Mir2DataBaseSection {
  param([string]$Text, [string]$Path)
  $m = [regex]::Match($Text, '(?im)^[ \t]*\[DataBase\][ \t]*\r?$')
  if (-not $m.Success) {
    throw ("{0}: 缺 [DataBase] 段（fail-closed）" -f $Path)
  }
  $start = $m.Index + $m.Length
  $rest = $Text.Substring($start)
  $next = [regex]::Match($rest, '(?m)^[ \t]*\[')
  $end = if ($next.Success) { $start + $next.Index } else { $Text.Length }
  return [pscustomobject]@{ Start = $start; End = $end }
}

# 读 INI：Latin1 解码**前**显式剥离 UTF-8 BOM（EF BB BF）。BOM 字节在 Latin1 下会变成
# 「ï»¿」（U+00EF U+00BB U+00BF），`^[ \t]*\[DataBase\]` 吃不到 ⇒ 生产 `Server.conf`
# （BOM 紧贴首行 `[DataBase]`）会误判缺段。返回 Text（已剥离）与 HasBom（写回时原样补回）。
function Read-Mir2IniText {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) { throw ("配置文件不存在：{0}" -f $Path) }
  $enc = Get-Mir2Latin1
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  $offset = 0
  $hasBom = $bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF
  if ($hasBom) { $offset = 3 }
  return [pscustomobject]@{
    Text   = $enc.GetString($bytes, $offset, $bytes.Length - $offset)
    HasBom = $hasBom
  }
}

function Get-Mir2IniConnString {
  param([string]$Path)
  $doc = Read-Mir2IniText -Path $Path
  $text = $doc.Text
  $range = Get-Mir2DataBaseSection -Text $text -Path $Path
  $body = $text.Substring($range.Start, $range.End - $range.Start)
  $m = [regex]::Match($body, '(?im)^[ \t]*ConnctionString[ \t]*=[ \t]*([^\r\n]*)')
  if (-not $m.Success) {
    throw ("{0}: [DataBase] 段缺 ConnctionString（fail-closed）" -f $Path)
  }
  return $m.Groups[1].Value
}

function Set-Mir2IniConnString {
  param([string]$Path, [string]$ConnString)
  if ($null -eq $ConnString -or $ConnString -match '[\r\n]') {
    throw '连接串为空或含换行（fail-closed）'
  }
  $doc = Read-Mir2IniText -Path $Path
  $enc = Get-Mir2Latin1
  $text = $doc.Text
  $range = Get-Mir2DataBaseSection -Text $text -Path $Path
  $body = $text.Substring($range.Start, $range.End - $range.Start)
  $rx = [regex]'(?im)^([ \t]*ConnctionString[ \t]*=[ \t]*)[^\r\n]*'
  $count = $rx.Matches($body).Count
  if ($count -eq 0) {
    throw ("{0}: [DataBase] 段缺 ConnctionString（fail-closed）" -f $Path)
  }
  if ($count -gt 1) {
    throw ("{0}: [DataBase] 段有 {1} 条 ConnctionString（歧义，fail-closed）" -f $Path, $count)
  }
  $evaluator = [System.Text.RegularExpressions.MatchEvaluator]{
    param([System.Text.RegularExpressions.Match]$mm)
    return $mm.Groups[1].Value + $ConnString
  }
  $newBody = $rx.Replace($body, $evaluator, 1)
  $newText = $text.Substring(0, $range.Start) + $newBody + $text.Substring($range.End)
  $outBytes = $enc.GetBytes($newText)
  if ($doc.HasBom) {
    $withBom = New-Object byte[] ($outBytes.Length + 3)
    $withBom[0] = 0xEF; $withBom[1] = 0xBB; $withBom[2] = 0xBF
    [Array]::Copy($outBytes, 0, $withBom, 3, $outBytes.Length)
    $outBytes = $withBom
  }
  [System.IO.File]::WriteAllBytes($Path, $outBytes)
}

function New-Mir2ShadowAppDir {
  param([string]$SourceDir, [string]$DestDir)
  if (-not (Test-Path -LiteralPath $SourceDir)) { throw ("源应用目录不存在：{0}" -f $SourceDir) }
  if (Test-Path -LiteralPath $DestDir) { Remove-Item -LiteralPath $DestDir -Recurse -Force }
  New-Item -ItemType Directory -Force -Path $DestDir | Out-Null
  Get-ChildItem -Force -LiteralPath $SourceDir | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $DestDir -Recurse -Force
  }
}

# fail-closed：影子内除声明的只读输入目录外，任何条目都不许是 Junction/硬链 ——
# 硬链与源是同一 inode，服务端写影子里的同名文件（日志/ini/存档）会直接写穿到 `E:\MirServer`。
function Assert-Mir2ShadowNoWritableLinks {
  param([string]$DestDir, [string[]]$ReadOnlyDirs = @('Map'))
  foreach ($e in Get-ChildItem -Force -LiteralPath $DestDir) {
    if ($ReadOnlyDirs -contains $e.Name) {
      if (-not ($e.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
        throw ("影子只读输入目录未链接（预期 Junction）：{0}（fail-closed）" -f $e.FullName)
      }
      continue
    }
    if (-not [string]::IsNullOrEmpty($e.LinkType)) {
      throw ("影子内可写入口指向源（{0}: {1}）—— 写影子会写穿源目录（fail-closed）" -f $e.LinkType, $e.FullName)
    }
  }
}

function New-Mir2GameSvrShadowBase {
  param(
    [string]$SourceDir,
    [string]$DestDir,
    [string]$ConnString,
    [ValidateSet('ReadOnlyData', 'Copy')][string]$LinkMode = 'ReadOnlyData'
  )
  if (-not (Test-Path -LiteralPath (Join-Path $SourceDir 'Server.conf'))) {
    throw ("game-svr 配置目录缺 Server.conf：{0}" -f $SourceDir)
  }
  if (Test-Path -LiteralPath $DestDir) { Remove-Item -LiteralPath $DestDir -Recurse -Force }
  New-Item -ItemType Directory -Force -Path $DestDir | Out-Null
  # `ReadOnlyData`（默认）：只把**只读输入目录**做 Junction（Map 约 195MB，不复制）；其余目录与
  # 顶层文件一律复制 ⇒ 影子完全独立。绝不硬链可写文件（硬链写穿源，见 Assert-* 注释）。
  $readOnlyDirs = @('Map')
  foreach ($e in Get-ChildItem -Force -LiteralPath $SourceDir) {
    if ($e.Name -ieq 'Server.conf') { continue }
    $dest = Join-Path $DestDir $e.Name
    $link = ($LinkMode -eq 'ReadOnlyData' -and $e.PSIsContainer -and ($readOnlyDirs -contains $e.Name))
    if ($link) {
      New-Item -ItemType Junction -Path $dest -Target $e.FullName | Out-Null
    } elseif ($e.PSIsContainer) {
      Copy-Item -LiteralPath $e.FullName -Destination $dest -Recurse -Force
    } else {
      Copy-Item -LiteralPath $e.FullName -Destination $dest -Force
    }
  }
  $conf = Join-Path $DestDir 'Server.conf'
  Copy-Item -LiteralPath (Join-Path $SourceDir 'Server.conf') -Destination $conf -Force
  Set-Mir2IniConnString -Path $conf -ConnString $ConnString
  $linked = if ($LinkMode -eq 'ReadOnlyData') { $readOnlyDirs } else { @() }
  Assert-Mir2ShadowNoWritableLinks -DestDir $DestDir -ReadOnlyDirs $linked
}

# ---------------------------------------------------------------------------
# Selftest（纯函数/临时目录；不连库、不起栈、不碰 E:\MirServer）
# ---------------------------------------------------------------------------
function Invoke-Mir2DbTargetSelftest {
  $script:dbSelftestOk = $true
  function Test-Case([string]$Name, [scriptblock]$Body) {
    try {
      $r = & $Body
      if ($r -eq $true) { Write-Output ("PASS  {0}" -f $Name) }
      else { Write-Output ("FAIL  {0}" -f $Name); $script:dbSelftestOk = $false }
    } catch {
      Write-Output ("FAIL  {0}（异常：{1}）" -f $Name, $_.Exception.Message)
      $script:dbSelftestOk = $false
    }
  }
  $root = Join-Path ([System.IO.Path]::GetTempPath()) ("mir2-dbtarget-selftest-" + [Guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path $root | Out-Null
  try {
    $enc = Get-Mir2Latin1

    Test-Case '默认（无参数）Present=false 且不改任何字段' {
      $t = Resolve-Mir2DbTarget
      return ((-not $t.Present) -and ($null -eq $t.Host) -and ($null -eq $t.NameTemplate))
    }
    Test-Case 'DbName=mir2_test 展开三库' {
      $t = Resolve-Mir2DbTarget -DbName 'mir2_test'
      return ((Expand-Mir2DbName $t 'db') -eq 'mir2_test_db') -and
             ((Expand-Mir2DbName $t 'account') -eq 'mir2_test_account') -and
             ((Expand-Mir2DbName $t 'data') -eq 'mir2_test_data')
    }
    Test-Case 'DbName=mir2_test_* 模板展开' {
      $t = Resolve-Mir2DbTarget -DbName 'mir2_test_*'
      return ((Expand-Mir2DbName $t 'db') -eq 'mir2_test_db')
    }
    Test-Case 'DbName=mir2_test_db 剥后缀后仍按前缀展开三库' {
      $t = Resolve-Mir2DbTarget -DbName 'mir2_test_db'
      return ((Expand-Mir2DbName $t 'account') -eq 'mir2_test_account')
    }
    Test-Case 'DbPort=3307 保留为字符串；DbHost/User/Password 去空白' {
      $t = Resolve-Mir2DbTarget -DbPort '3307' -DbHost ' 10.0.0.2 ' -DbUser 'u' -DbPassword 'p'
      return ($t.Port -eq '3307') -and ($t.Host -eq '10.0.0.2') -and ($t.User -eq 'u') -and ($t.Password -eq 'p')
    }
    Test-Case '非法端口 0/70000/abc 抛错' {
      $bad = 0
      foreach ($v in @('0', '70000', 'abc')) {
        try { Resolve-Mir2DbTarget -DbPort $v | Out-Null } catch { $bad++ }
      }
      return $bad -eq 3
    }
    Test-Case 'DbPassword 含 ; 抛错' {
      try { Resolve-Mir2DbTarget -DbPassword 'a;b' | Out-Null; return $false } catch { return $true }
    }
    Test-Case 'DbName 含空格/; 抛错' {
      $bad = 0
      foreach ($v in @('mir2 test', 'mir2;test')) {
        try { Resolve-Mir2DbTarget -DbName $v | Out-Null } catch { $bad++ }
      }
      return $bad -eq 2
    }

    $base = 'server=127.0.0.1;uid=root;pwd=;database=mir2_data;'
    Test-Case '连接串只改 database（顺序/空 pwd/尾 ; 全保留）' {
      $s = Update-Mir2ConnString -ConnString $base -Database 'mir2_test_data'
      return $s -eq 'server=127.0.0.1;uid=root;pwd=;database=mir2_test_data;'
    }
    Test-Case '连接串改 host/user/pwd/port（port 追加；其余原序）' {
      $s = Update-Mir2ConnString -ConnString $base -Host '10.1.2.3' -User 'tester' -Password 'secret' -Port '3307'
      return $s -eq 'server=10.1.2.3;uid=tester;pwd=secret;database=mir2_data;port=3307;'
    }
    Test-Case '连接串无任何覆盖 ⇒ 原样返回' {
      return ((Update-Mir2ConnString -ConnString $base) -eq $base)
    }
    Test-Case '连接串已含 port ⇒ 原位替换不追加' {
      $s = Update-Mir2ConnString -ConnString 'server=h;port=3306;uid=u;pwd=;database=d;' -Port '3308'
      return $s -eq 'server=h;port=3308;uid=u;pwd=;database=d;'
    }

    $ini = Join-Path $root 'dbsvr.conf'
    $prefix = "[Setup]`r`nServerPort=6000`r`n`r`n[DataBase]`r`nStoreage=MySQL`r`nConnctionString=$base`r`n`r`n[DBClear]`r`nInterval=3000`r`n"
    # BOM + 一段 GBK 字节（0xC4 0xE3 = 你），证明 Latin1 往返不吞非 UTF-8 字节。
    $bytes = [byte[]](0xEF, 0xBB, 0xBF) + $enc.GetBytes($prefix) + [byte[]](0x23, 0x20, 0xC4, 0xE3, 0x0D, 0x0A)
    [System.IO.File]::WriteAllBytes($ini, $bytes)
    $sha0 = (Get-FileHash -LiteralPath $ini -Algorithm SHA256).Hash

    Test-Case 'INI 读连接串（BOM/GBK 共存）' {
      return ((Get-Mir2IniConnString -Path $ini) -eq $base)
    }
    Test-Case 'BOM 紧贴 [DataBase] 段头（生产形态 LF）：读/写均成立且 BOM 保留' {
      $p = Join-Path $root 'bom-first.conf'
      $body = "[DataBase]`nConnctionString=$base`n[Server]`nX=1`n"
      [System.IO.File]::WriteAllBytes($p, [byte[]](0xEF, 0xBB, 0xBF) + $enc.GetBytes($body))
      $read0 = (Get-Mir2IniConnString -Path $p)
      Set-Mir2IniConnString -Path $p -ConnString 'server=127.0.0.1;uid=root;pwd=;database=mir2_test_db;'
      $after = [System.IO.File]::ReadAllBytes($p)
      $bomOk = ($after.Length -ge 3 -and $after[0] -eq 0xEF -and $after[1] -eq 0xBB -and $after[2] -eq 0xBF)
      $readOk = ((Get-Mir2IniConnString -Path $p) -eq 'server=127.0.0.1;uid=root;pwd=;database=mir2_test_db;')
      return ($read0 -eq $base) -and $bomOk -and $readOk
    }
    Test-Case 'INI 写同值 ⇒ 字节完全不变（sha256 相同）' {
      Set-Mir2IniConnString -Path $ini -ConnString $base
      return ((Get-FileHash -LiteralPath $ini -Algorithm SHA256).Hash -eq $sha0)
    }
    Test-Case 'INI 改写 database ⇒ 只改值字节，BOM/GBK/CRLF/段序保留' {
      Set-Mir2IniConnString -Path $ini -ConnString 'server=127.0.0.1;uid=root;pwd=;database=mir2_test_db;'
      $after = [System.IO.File]::ReadAllBytes($ini)
      $text = $enc.GetString($after)
      $bomOk = ($after[0] -eq 0xEF -and $after[1] -eq 0xBB -and $after[2] -eq 0xBF)
      $gbkOk = ($text.Contains([string][char]0xC4 + [string][char]0xE3))
      $dbOk = ($text.Contains('ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_test_db;'))
      $tailOk = ($text.Contains("[DBClear]`r`nInterval=3000`r`n"))
      return ($bomOk -and $gbkOk -and $dbOk -and $tailOk)
    }
    Test-Case 'INI 缺 [DataBase] 段 ⇒ 抛错' {
      $p = Join-Path $root 'no-section.conf'
      [System.IO.File]::WriteAllText($p, "[Setup]`r`nX=1`r`n", $enc)
      try { Set-Mir2IniConnString -Path $p -ConnString $base; return $false } catch { return $true }
    }
    Test-Case 'INI 重复 ConnctionString ⇒ 抛错（歧义 fail-closed）' {
      $p = Join-Path $root 'dup.conf'
      [System.IO.File]::WriteAllText($p, "[DataBase]`r`nConnctionString=$base`r`nConnctionString=$base`r`n", $enc)
      try { Set-Mir2IniConnString -Path $p -ConnString $base; return $false } catch { return $true }
    }

    Test-Case '影子应用目录：文件逐字节复制（含 GBK 配置）' {
      $src = Join-Path $root 'app-src'
      New-Item -ItemType Directory -Force -Path $src | Out-Null
      [System.IO.File]::WriteAllBytes((Join-Path $src 'dbsvr.conf'), $bytes)
      [System.IO.File]::WriteAllText((Join-Path $src 'DBSrv.exe'), 'stub', $enc)
      $dst = Join-Path $root 'app-dst'
      New-Mir2ShadowAppDir -SourceDir $src -DestDir $dst
      $same = ((Get-FileHash -LiteralPath (Join-Path $dst 'dbsvr.conf') -Algorithm SHA256).Hash -eq $sha0)
      $exeOk = (Test-Path -LiteralPath (Join-Path $dst 'DBSrv.exe'))
      return ($same -and $exeOk)
    }
    Test-Case 'game-svr 影子 base：Server.conf 改写，Envir/Map 可见，源目录不被改' {
      $src = Join-Path $root 'gsvr-src'
      New-Item -ItemType Directory -Force -Path (Join-Path $src 'Envir') | Out-Null
      New-Item -ItemType Directory -Force -Path (Join-Path $src 'Map') | Out-Null
      [System.IO.File]::WriteAllText((Join-Path $src 'Envir\MapInfo.txt'), "[0|0]`r`n", $enc)
      [System.IO.File]::WriteAllText((Join-Path $src 'Map\0.map'), 'map', $enc)
      $confPath = Join-Path $src 'Server.conf'
      [System.IO.File]::WriteAllBytes($confPath, $bytes)
      $srcSha = (Get-FileHash -LiteralPath $confPath -Algorithm SHA256).Hash
      $dst = Join-Path $root 'gsvr-dst'
      New-Mir2GameSvrShadowBase -SourceDir $src -DestDir $dst `
        -ConnString 'server=127.0.0.1;uid=root;pwd=;database=mir2_test_data;' -LinkMode Copy
      $srcStill = ((Get-FileHash -LiteralPath $confPath -Algorithm SHA256).Hash -eq $srcSha)
      $dbOk = ((Get-Mir2IniConnString -Path (Join-Path $dst 'Server.conf')) -eq 'server=127.0.0.1;uid=root;pwd=;database=mir2_test_data;')
      $envOk = (Test-Path -LiteralPath (Join-Path $dst 'Envir\MapInfo.txt'))
      $mapOk = (Test-Path -LiteralPath (Join-Path $dst 'Map\0.map'))
      return ($srcStill -and $dbOk -and $envOk -and $mapOk)
    }
    Test-Case 'game-svr 影子（默认 ReadOnlyData）：写影子日志/配置不改源，可写入口无链接' {
      $src = Join-Path $root 'gsvr-write-src'
      New-Item -ItemType Directory -Force -Path (Join-Path $src 'Envir') | Out-Null
      New-Item -ItemType Directory -Force -Path (Join-Path $src 'Map') | Out-Null
      New-Item -ItemType Directory -Force -Path (Join-Path $src 'logs') | Out-Null
      [System.IO.File]::WriteAllText((Join-Path $src 'Envir\MapInfo.txt'), "[0|0]`r`n", $enc)
      [System.IO.File]::WriteAllText((Join-Path $src 'Map\0.map'), 'map', $enc)
      [System.IO.File]::WriteAllText((Join-Path $src 'logs\old.log'), 'old', $enc)
      [System.IO.File]::WriteAllText((Join-Path $src 'setting.conf'), 'x=1', $enc)
      [System.IO.File]::WriteAllBytes((Join-Path $src 'Server.conf'), $bytes)
      $dst = Join-Path $root 'gsvr-write-dst'
      New-Mir2GameSvrShadowBase -SourceDir $src -DestDir $dst -ConnString $base
      Add-Content -LiteralPath (Join-Path $dst 'logs\old.log') -Value 'shadow'
      Add-Content -LiteralPath (Join-Path $dst 'Envir\MapInfo.txt') -Value 'shadow'
      Add-Content -LiteralPath (Join-Path $dst 'setting.conf') -Value 'shadow'
      $srcLogOk = ([System.IO.File]::ReadAllText((Join-Path $src 'logs\old.log'), $enc) -ceq 'old')
      $srcEnvirOk = (-not [System.IO.File]::ReadAllText((Join-Path $src 'Envir\MapInfo.txt'), $enc).Contains('shadow'))
      $srcSettingOk = ([System.IO.File]::ReadAllText((Join-Path $src 'setting.conf'), $enc) -ceq 'x=1')
      $dstLogOk = ([System.IO.File]::ReadAllText((Join-Path $dst 'logs\old.log'), $enc).Contains('shadow'))
      $mapLink = ((Get-Item -LiteralPath (Join-Path $dst 'Map') -Force).LinkType -eq 'Junction')
      $envirCopied = [string]::IsNullOrEmpty((Get-Item -LiteralPath (Join-Path $dst 'Envir') -Force).LinkType)
      return ($srcLogOk -and $srcEnvirOk -and $srcSettingOk -and $dstLogOk -and $mapLink -and $envirCopied)
    }
    Test-Case 'game-svr 影子：可写入口被换成硬链 ⇒ 断言 fail-closed' {
      $src = Join-Path $root 'gsvr-link-src'
      New-Item -ItemType Directory -Force -Path (Join-Path $src 'Envir') | Out-Null
      New-Item -ItemType Directory -Force -Path (Join-Path $src 'Map') | Out-Null
      [System.IO.File]::WriteAllText((Join-Path $src 'setting.conf'), 'x=1', $enc)
      [System.IO.File]::WriteAllBytes((Join-Path $src 'Server.conf'), $bytes)
      $dst = Join-Path $root 'gsvr-link-dst'
      New-Mir2GameSvrShadowBase -SourceDir $src -DestDir $dst -ConnString $base
      Remove-Item -LiteralPath (Join-Path $dst 'setting.conf') -Force
      New-Item -ItemType HardLink -Path (Join-Path $dst 'setting.conf') -Target (Join-Path $src 'setting.conf') | Out-Null
      try { Assert-Mir2ShadowNoWritableLinks -DestDir $dst | Out-Null; return $false } catch { return $true }
    }
  } finally {
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
  }
  Write-Output ("SELFTEST {0}" -f $(if ($script:dbSelftestOk) { 'GREEN' } else { 'RED' }))
}

if ($Selftest) {
  Invoke-Mir2DbTargetSelftest
  if ($script:dbSelftestOk) { exit 0 } else { exit 1 }
}
