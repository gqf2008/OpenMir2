# -*- coding: utf-8 -*-
# guild_fixture.ps1 — 行会窗夹具「预置」脚本（卡 openmir2-guild-window-fixture）
#
# 用途：为真机行会窗预置一个「文件形态」的行会，使 oracle 走「文件存在 ⇒ 保存成功」路径。
# 本脚本只写磁盘文件，不连 DB、不走游戏内建会；真实窗口由 Leader 执行，本卡自测只用 tempdir。
#
# 背景（oracle 事实，勿改）：
#   * 行会数据不在 MySQL：GuildList.txt（名单）+ Guilds/<名>.txt（公告/敌对/联盟/成员）。
#     Rust 侧读侧见 mir2-rs/crates/data/src/guild.rs 模块头；C# 侧见
#     src/M2Server/Guild/GuildInfo.cs:209-346（LoadGuildFile）/ :384-431（SaveGuildFile）。
#   * 编码/换行：GBK（cp936）+ CRLF（StringList.SaveToFile 口径）。
#   * 四段头：公告 / 敌对行会 / 联盟行会 / 行会成员；段间有一行单个空格；成员段以
#     `#<RankNo> <RankName>` 起头，成员行 `+<角色名>`。掌门默认 RankNo=1 / RankName=掌门人。
#   * oracle 保存坑 1：SaveToFile 用 FileMode.Open ⇒ 目标 <名>.txt 不存在时静默失败
#     （SaveGuildFile 的 try/catch 吞掉）。所以本夹具必须先建 <名>.txt，再让服务端装载。
#   * oracle 保存坑 2：<名>.ini 只会被 GuildConf 的 ctor 建成空文件，五键永不落盘。
#   * 重启装载：文件在服务端启动时 LoadGuildInfo/LoadGuild 读取；预置后必须重启 game-svr。
#
# 安全（fail-closed）：
#   * 目标 <名>.txt / <名>.ini 已存在、GuildList.txt 已有同名、参数非法 ⇒ 拒绝（rc=2）。
#   * 受保护真实根（E:\MirServer 及其子孙）在 -Selftest 下无条件拒绝；
#     非 Selftest 时必须显式 -Force 才允许写入。
#   * -WhatIf 只打印计划，不产生任何写（连备份目录都不建）。
#   * 中文段头/封号用码点构造（见 New-Seq），保证脚本以 UTF-8 no BOM 存盘时
#     即使被 Windows PowerShell 5.1 按 ANSI 解析，写出的 GBK 字节仍正确。
#
# 用法：
#   pwsh -NoProfile -File tools/window/guild_fixture.ps1 -Selftest
#   pwsh -NoProfile -File tools/window/guild_fixture.ps1 `
#       -Root E:/MirServer/M2GameSvr -GuildName e2e-guild -Member 测试角色 `
#       -BackupDir E:/tmp/guild-window/backup -Force
#   powershell -NoProfile -File tools/window/guild_fixture.ps1 -WhatIf -Root ... -GuildName ... -Member ...
#
# 输出（机器可读，供窗口取证）：
#   FIXTURE <名>            ROOT <绝对根>
#   BEFORE_SHA <hex|ABSENT> AFTER_SHA <hex|NOT_WRITTEN>
#   LIST_FILE <path>        GUILD_FILE <path>       STATE_FILE <path>
#   WROTE <relpath> ...
# 退出码：0 = 成功/自测全绿；2 = 拒绝或失败（RED: ...）。
#
# 自测（全在系统 temp 目录合成根）：prep→restore 往返字节一致、已存在拒绝、GBK/CRLF
# 字节断言、-WhatIf 零写、真实根硬拒绝，并断言 E:/MirServer/M2GameSvr/GuildBase 自测前后不变。

param(
  [string]$Root = '',
  [string]$GuildName = '',
  [string]$Member = '',
  [string]$BackupDir = '',
  [switch]$WhatIf,
  [switch]$Selftest,
  [switch]$Force
)

$ErrorActionPreference = 'Stop'

# 受保护的真实部署根（卡面硬约束：本卡不得对 E:/MirServer 执行任何写入）。
$script:ProtectedRealRoot = 'E:\MirServer'
$script:ListFileName = 'GuildList.txt'
$script:StateFileName = 'guild-fixture-state.json'
$script:BackupListName = 'GuildList.txt.bak'

# ---- 段头 / 封号：用码点构造，避开脚本自身编码对功能字符串的影响 -------------------------
# 公告 / 敌对行会 / 联盟行会 / 行会成员 / 掌门人
$script:S_NOTICE = -join [char[]](0x516C, 0x544A)
$script:S_WAR    = -join [char[]](0x654C, 0x5BF9, 0x884C, 0x4F1A)
$script:S_ALL    = -join [char[]](0x8054, 0x76DF, 0x884C, 0x4F1A)
$script:S_MEMBER = -join [char[]](0x884C, 0x4F1A, 0x6210, 0x5458)
$script:S_CHIEF  = -join [char[]](0x638C, 0x95E8, 0x4EBA)

function Get-GbkEncoding {
  return [System.Text.Encoding]::GetEncoding(936)
}

function Get-Sha256HexBytes([byte[]]$bytes) {
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    return ([BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function Get-Sha256HexText([string]$text) {
  return Get-Sha256HexBytes ([System.Text.Encoding]::UTF8.GetBytes($text))
}

# 优先 Get-FileHash（卡面口径）；但本机 Windows PowerShell 5.1 的 PSModulePath 缺
# Microsoft.PowerShell.Utility 时 Get-FileHash 不存在（见 rebuild-server.ps1 同类注释），
# 故保留 .NET 兜底，保证两种宿主都可跑。
function Get-Sha256Hex([string]$path) {
  try {
    if (Get-Command Get-FileHash -ErrorAction Stop) {
      return (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
  } catch {
    # fall through to .NET
  }
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $stream = [System.IO.File]::OpenRead($path)
    try {
      return ([BitConverter]::ToString($sha.ComputeHash($stream)) -replace '-', '').ToLowerInvariant()
    } finally {
      $stream.Dispose()
    }
  } finally {
    $sha.Dispose()
  }
}

function Get-NormalizedFullPath([string]$path) {
  if ([string]::IsNullOrWhiteSpace($path)) { return '' }
  if (-not [System.IO.Path]::IsPathRooted($path)) {
    throw "路径必须是绝对路径：$path"
  }
  return ([System.IO.Path]::GetFullPath($path)).TrimEnd('\', '/')
}

function Test-IsProtectedPath([string]$path) {
  if ([string]::IsNullOrWhiteSpace($path)) { return $false }
  $full = ([System.IO.Path]::GetFullPath($path)).TrimEnd('\', '/')
  $prot = ([System.IO.Path]::GetFullPath($script:ProtectedRealRoot)).TrimEnd('\', '/')
  if ($full.Equals($prot, [System.StringComparison]::OrdinalIgnoreCase)) { return $true }
  return $full.StartsWith($prot + '\', [System.StringComparison]::OrdinalIgnoreCase)
}

function Assert-SafeRoot([string]$root, [bool]$isSelftest, [bool]$force) {
  if (Test-IsProtectedPath $root) {
    if ($isSelftest) {
      throw "Selftest 禁止使用真实部署根 $($script:ProtectedRealRoot)（收到：$root）；自测一律用 tempdir"
    }
    if (-not $force) {
      throw "根命中受保护的真实部署根 $($script:ProtectedRealRoot)（收到：$root）；如需真机写入必须显式 -Force"
    }
  }
}

function Assert-ValidGuildName([string]$name) {
  if ([string]::IsNullOrWhiteSpace($name)) { throw '缺少 -GuildName' }
  if ($name -ne $name.Trim()) { throw "GuildName 首尾不能有空白：[$name]" }
  if ($name -match '[\\/:*?"<>|]') { throw "GuildName 含非法文件名字符：$name" }
  if ($name -match '[ ,]') { throw "GuildName 不能含空格或逗号（oracle GetValidStr3 分词符）：$name" }
}

function Assert-ValidMember([string]$member) {
  if ([string]::IsNullOrWhiteSpace($member)) { throw '缺少 -Member' }
  if ($member -ne $member.Trim()) { throw "Member 首尾不能有空白：[$member]" }
  if ($member -match '[ ,]') { throw "Member 不能含空格或逗号（oracle GetValidStr3 分词符）：$member" }
}

# 目录树快照（含空目录）：用于 -WhatIf 零写与往返一致性证明。
function Get-TreeSnapshot([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { return 'ABSENT' }
  $rootFull = ([System.IO.Path]::GetFullPath($path)).TrimEnd('\', '/')
  $lines = New-Object System.Collections.Generic.List[string]
  foreach ($d in (Get-ChildItem -LiteralPath $path -Recurse -Force -Directory | Sort-Object FullName)) {
    $rel = $d.FullName.Substring($rootFull.Length).TrimStart('\', '/')
    $lines.Add('D|' + $rel.Replace('\', '/'))
  }
  foreach ($f in (Get-ChildItem -LiteralPath $path -Recurse -Force -File | Sort-Object FullName)) {
    $rel = $f.FullName.Substring($rootFull.Length).TrimStart('\', '/')
    $lines.Add('F|' + $rel.Replace('\', '/') + '|' + (Get-Sha256Hex $f.FullName) + '|' + $f.Length)
  }
  return ($lines -join "`n")
}

function Get-TreeDigest([string]$path) {
  return Get-Sha256HexText (Get-TreeSnapshot $path)
}

# 期望的 <名>.txt 行序列（与 oracle SaveGuildFile 同形）：
#   公告 / ' ' / 敌对行会 / ' ' / 联盟行会 / ' ' / 行会成员 / #1 掌门人 / +<Member>
function Get-GuildFileLines([string]$member) {
  return @(
    $script:S_NOTICE,
    ' ',
    $script:S_WAR,
    ' ',
    $script:S_ALL,
    ' ',
    $script:S_MEMBER,
    ('#1 ' + $script:S_CHIEF),
    ('+' + $member)
  )
}

function Get-GuildFileBytes([string]$member) {
  $text = ((Get-GuildFileLines $member) -join "`r`n") + "`r`n"
  return (Get-GbkEncoding).GetBytes($text)
}

# 解码 GuildList.txt（GBK）为去空白后的名单；文件不存在返回空数组。
function Get-GuildListNames([string]$path) {
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return @() }
  $text = (Get-GbkEncoding).GetString([System.IO.File]::ReadAllBytes($path))
  return @($text -split "`r`n|`n|`r" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' })
}

function New-SequenceBytes([byte[]]$prefix, [byte[]]$suffix) {
  $out = New-Object byte[] ($prefix.Length + $suffix.Length)
  [Array]::Copy($prefix, 0, $out, 0, $prefix.Length)
  [Array]::Copy($suffix, 0, $out, $prefix.Length, $suffix.Length)
  return , $out
}

function Invoke-Prepare {
  param(
    [string]$RootDir,
    [string]$Name,
    [string]$MemberName,
    [string]$BackupDirPath,
    [switch]$PlanOnly
  )

  $guildBase = Join-Path $RootDir 'GuildBase'
  $guildsDir = Join-Path $guildBase 'Guilds'
  $listFile = Join-Path $guildBase $script:ListFileName
  $guildFile = Join-Path $guildsDir ($Name + '.txt')
  $iniFile = Join-Path $guildsDir ($Name + '.ini')

  $beforeExists = Test-Path -LiteralPath $listFile -PathType Leaf
  $beforeBytes = if ($beforeExists) { [System.IO.File]::ReadAllBytes($listFile) } else { $null }
  $beforeSha = if ($beforeExists) { Get-Sha256HexBytes $beforeBytes } else { 'ABSENT' }

  # ---- fail-closed 前置校验（-WhatIf 也照做，保持与真实执行同判） ----
  if (Test-Path -LiteralPath $guildFile -PathType Leaf) {
    throw "目标已存在，拒绝覆盖（fail-closed）：$guildFile"
  }
  if (Test-Path -LiteralPath $iniFile -PathType Leaf) {
    throw "同名 .ini 已存在，拒绝预置（fail-closed）：$iniFile"
  }
  $existingNames = Get-GuildListNames $listFile
  if ($existingNames -contains $Name) {
    throw "GuildList.txt 已含同名行会，拒绝重复预置：$Name"
  }

  $guildBytes = Get-GuildFileBytes $MemberName

  # 追加行：内存中一次构造，避免半写。原文件若末字节不是 LF，先补 CRLF 防粘连。
  $enc = Get-GbkEncoding
  $nameBytes = $enc.GetBytes($Name)
  $crlf = [byte[]](0x0D, 0x0A)
  $suffix = $nameBytes
  if ($beforeExists -and $beforeBytes.Length -gt 0 -and $beforeBytes[$beforeBytes.Length - 1] -ne 0x0A) {
    $suffix = New-SequenceBytes $crlf $nameBytes
  }
  $afterListBytes = if ($beforeExists) {
    New-SequenceBytes $beforeBytes (New-SequenceBytes $suffix $crlf)
  } else {
    New-SequenceBytes $nameBytes $crlf
  }
  $afterSha = Get-Sha256HexBytes $afterListBytes

  $planned = New-Object System.Collections.Generic.List[string]
  $planned.Add('CREATE ' + $guildBase)
  $planned.Add('CREATE ' + $guildsDir)
  $planned.Add('WRITE ' + $guildFile)
  $planned.Add('WRITE ' + $listFile)

  if ($PlanOnly) {
    return [pscustomobject]@{
      Root = $RootDir
      GuildName = $Name
      Member = $MemberName
      GuildBase = $guildBase
      GuildsDir = $guildsDir
      ListFile = $listFile
      GuildFile = $guildFile
      BackupFile = ''
      StateFile = ''
      BeforeSha = $beforeSha
      AfterSha = 'NOT_WRITTEN'
      Planned = $planned
    }
  }

  if ([string]::IsNullOrWhiteSpace($BackupDirPath)) {
    throw '内部错误：未提供 BackupDir'
  }
  if (-not (Test-Path -LiteralPath $BackupDirPath)) {
    [void](New-Item -ItemType Directory -Path $BackupDirPath -Force)
  }
  $backupFile = Join-Path $BackupDirPath $script:BackupListName
  $stateFile = Join-Path $BackupDirPath $script:StateFileName

  # ---- ① 登记 + 备份（原字节 Copy-Item） ----
  $guildBaseExisted = Test-Path -LiteralPath $guildBase
  $guildsDirExisted = Test-Path -LiteralPath $guildsDir
  if ($beforeExists) {
    Copy-Item -LiteralPath $listFile -Destination $backupFile -Force
    $backupSha = Get-Sha256Hex $backupFile
    if ($backupSha -ne $beforeSha) {
      throw "备份字节校验失败：backup_sha=$backupSha before_sha=$beforeSha"
    }
  } else {
    $backupFile = ''
  }

  $state = [ordered]@{
    tool = 'guild_fixture'
    version = 1
    guild_name = $Name
    member = $MemberName
    root = $RootDir
    guild_base = $guildBase
    guilds_dir = $guildsDir
    list_file = $listFile
    guild_file = $guildFile
    guild_ini = $iniFile
    backup_file = $backupFile
    before_sha = $beforeSha
    before_absent = (-not $beforeExists)
    guild_base_existed_before = $guildBaseExisted
    guilds_dir_existed_before = $guildsDirExisted
    guild_ini_existed_before = (Test-Path -LiteralPath $iniFile -PathType Leaf)
    expected_after_sha = $afterSha
    taken_at = (Get-Date).ToString('o')
  }
  $utf8 = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($stateFile, ($state | ConvertTo-Json -Depth 5), $utf8)

  # ---- ②..⑤ 建目录、写 <名>.txt、一次写 GuildList.txt ----
  if (-not $guildBaseExisted) { [void](New-Item -ItemType Directory -Path $guildBase -Force) }
  if (-not $guildsDirExisted) { [void](New-Item -ItemType Directory -Path $guildsDir -Force) }
  [System.IO.File]::WriteAllBytes($guildFile, $guildBytes)
  [System.IO.File]::WriteAllBytes($listFile, $afterListBytes)

  # ---- ⑥ 写后校验 ----
  $actualAfter = Get-Sha256Hex $listFile
  if ($actualAfter -ne $afterSha) {
    throw "after_sha 校验失败：actual=$actualAfter expected=$afterSha"
  }
  $actualGuildSha = Get-Sha256Hex $guildFile
  $expectedGuildSha = Get-Sha256HexBytes $guildBytes
  if ($actualGuildSha -ne $expectedGuildSha) {
    throw "guild file sha 校验失败：actual=$actualGuildSha expected=$expectedGuildSha"
  }

  return [pscustomobject]@{
    Root = $RootDir
    GuildName = $Name
    Member = $MemberName
    GuildBase = $guildBase
    GuildsDir = $guildsDir
    ListFile = $listFile
    GuildFile = $guildFile
    BackupFile = $backupFile
    StateFile = $stateFile
    BeforeSha = $beforeSha
    AfterSha = $actualAfter
    Planned = $planned
  }
}

function Invoke-PrepareCli {
  if ([string]::IsNullOrWhiteSpace($Root)) { throw '缺少 -Root' }
  $rootPath = Get-NormalizedFullPath $Root
  Assert-SafeRoot $rootPath $false $Force.IsPresent
  if (-not (Test-Path -LiteralPath $rootPath -PathType Container)) {
    throw "部署根不存在：$rootPath"
  }
  Assert-ValidGuildName $GuildName
  Assert-ValidMember $Member
  $backupPath = if ([string]::IsNullOrWhiteSpace($BackupDir)) {
    Join-Path ([System.IO.Path]::GetTempPath()) ('guild-fixture-backup-' + (Get-Date).ToString('yyyyMMdd-HHmmss'))
  } else {
    Get-NormalizedFullPath $BackupDir
  }

  $result = Invoke-Prepare -RootDir $rootPath -Name $GuildName -MemberName $Member -BackupDirPath $backupPath -PlanOnly:$WhatIf.IsPresent

  Write-Host ('FIXTURE ' + $result.GuildName)
  Write-Host ('ROOT ' + $result.Root)
  Write-Host ('BEFORE_SHA ' + $result.BeforeSha)
  Write-Host ('AFTER_SHA ' + $result.AfterSha)
  Write-Host ('LIST_FILE ' + $result.ListFile)
  Write-Host ('GUILD_FILE ' + $result.GuildFile)
  Write-Host ('BACKUP_FILE ' + $result.BackupFile)
  Write-Host ('STATE_FILE ' + $result.StateFile)
  foreach ($p in $result.Planned) {
    Write-Host ('PLAN ' + $p)
  }
  if ($WhatIf) { Write-Host 'WHATIF 未写入任何文件' }
}

# ---------------------------------------------------------------- selftest ----
$script:SelFail = 0
function Assert-True([bool]$cond, [string]$msg) {
  if ($cond) {
    Write-Host ('PASS ' + $msg)
  } else {
    $script:SelFail++
    Write-Host ('FAIL ' + $msg)
  }
}
function Assert-Equal($actual, $expected, [string]$msg) {
  Assert-True ([string]$actual -eq [string]$expected) ($msg + ' [actual=' + [string]$actual + ' expected=' + [string]$expected + ']')
}
function Assert-Throws([scriptblock]$block, [string]$msg) {
  $threw = $false
  $err = ''
  try { & $block } catch { $threw = $true; $err = $_.Exception.Message }
  Assert-True $threw ($msg + ' [err=' + $err + ']')
}

function New-TempRoot([string]$tag) {
  $base = Join-Path ([System.IO.Path]::GetTempPath()) ('guild-fixture-selftest-' + $tag + '-' + [guid]::NewGuid().ToString('N'))
  [void](New-Item -ItemType Directory -Path $base -Force)
  return $base
}

function Get-HostExe {
  return (Get-Process -Id $PID).Path
}

function Test-ByteStreamIsCrlf([byte[]]$bytes) {
  for ($i = 0; $i -lt $bytes.Length; $i++) {
    if ($bytes[$i] -eq 0x0A) {
      if ($i -eq 0 -or $bytes[$i - 1] -ne 0x0D) { return $false }
    }
  }
  return $true
}

function Test-ContainsSubsequence([byte[]]$haystack, [byte[]]$needle) {
  if ($needle.Length -eq 0 -or $haystack.Length -lt $needle.Length) { return $false }
  for ($i = 0; $i -le $haystack.Length - $needle.Length; $i++) {
    $ok = $true
    for ($j = 0; $j -lt $needle.Length; $j++) {
      if ($haystack[$i + $j] -ne $needle[$j]) { $ok = $false; break }
    }
    if ($ok) { return $true }
  }
  return $false
}

function Invoke-Selftest {
  $enc = Get-GbkEncoding
  $g = -join [char[]](0x6D4B, 0x8BD5, 0x884C, 0x4F1A)   # 测试行会
  $m = -join [char[]](0x6D4B, 0x8BD5, 0x89D2, 0x8272)   # 测试角色

  # 生产根旁观快照：自测前后必须完全一致（只读，证明零写入）。
  $watchRoot = Join-Path $script:ProtectedRealRoot 'M2GameSvr\GuildBase'
  $watchBefore = if (Test-Path -LiteralPath $watchRoot) { Get-TreeDigest $watchRoot } else { 'ABSENT' }

  Write-Host '--- T1 新建根：prep -> restore 往返 ---'
  $t1 = New-TempRoot 't1'
  try {
    $root = Join-Path $t1 'M2GameSvr'
    [void](New-Item -ItemType Directory -Path (Join-Path $root 'GuildBase') -Force)
    $backup = Join-Path $t1 'backup'
    $guildBase = Join-Path $root 'GuildBase'
    $treeBefore = Get-TreeSnapshot $guildBase
    $r1 = Invoke-Prepare -RootDir $root -Name $g -MemberName $m -BackupDirPath $backup
    Assert-Equal 'ABSENT' $r1.BeforeSha 'T1 原 GuildList 记为 ABSENT'
    Assert-True (Test-Path -LiteralPath $r1.GuildFile -PathType Leaf) 'T1 已写 <名>.txt'
    $expectedList = New-SequenceBytes ((Get-GbkEncoding).GetBytes($g)) ([byte[]](0x0D, 0x0A))
    $actualList = [System.IO.File]::ReadAllBytes($r1.ListFile)
    Assert-Equal (Get-Sha256HexBytes $expectedList) (Get-Sha256HexBytes $actualList) 'T1 GuildList = GBK(名)+CRLF'
    Assert-Equal $r1.AfterSha (Get-Sha256Hex $r1.ListFile) 'T1 after_sha 与磁盘一致'

    $restore = Join-Path $PSScriptRoot 'guild_fixture_restore.ps1'
    $exe = Get-HostExe
    & $exe -NoProfile -ExecutionPolicy Bypass -File $restore -Root $root -GuildName $g -BackupDir $backup
    Assert-Equal 0 $LASTEXITCODE 'T1 restore 退出码 0'
    Assert-True (-not (Test-Path -LiteralPath $r1.ListFile)) 'T1 restore 后 GuildList 已删（原 ABSENT）'
    Assert-True (-not (Test-Path -LiteralPath $r1.GuildFile)) 'T1 restore 后 <名>.txt 已删'
    Assert-True (-not (Test-Path -LiteralPath $r1.GuildsDir)) 'T1 restore 后空 Guilds 目录已删'
    Assert-Equal $treeBefore (Get-TreeSnapshot $guildBase) 'T1 往返后 GuildBase 树一致'
  } finally { Remove-Item -LiteralPath $t1 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- T2 已有 GuildList（带 CRLF / 不带 CRLF）：往返字节一致 ---'
  foreach ($variant in @('with-crlf', 'no-crlf')) {
    $t2 = New-TempRoot ('t2-' + $variant)
    try {
      $root = Join-Path $t2 'M2GameSvr'
      $guildBase = Join-Path $root 'GuildBase'
      [void](New-Item -ItemType Directory -Path $guildBase -Force)
      $old = -join [char[]](0x65E7, 0x884C, 0x4F1A)   # 旧行会
      $seed = if ($variant -eq 'with-crlf') { $enc.GetBytes($old + "`r`n") } else { $enc.GetBytes($old) }
      $listFile = Join-Path $guildBase 'GuildList.txt'
      [System.IO.File]::WriteAllBytes($listFile, $seed)
      $seedSha = Get-Sha256HexBytes $seed
      $treeBefore = Get-TreeSnapshot $guildBase
      $backup = Join-Path $t2 'backup'
      $r = Invoke-Prepare -RootDir $root -Name $g -MemberName $m -BackupDirPath $backup
      Assert-Equal $seedSha $r.BeforeSha ("T2[$variant] before_sha = 原文件")
      $got = Get-GuildListNames $listFile
      Assert-Equal 2 $got.Count ("T2[$variant] 名单含 2 个行会")
      Assert-Equal $old $got[0] ("T2[$variant] 原行会保留")
      Assert-Equal $g $got[1] ("T2[$variant] 新行会追加")

      $restore = Join-Path $PSScriptRoot 'guild_fixture_restore.ps1'
      & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $restore -Root $root -GuildName $g -BackupDir $backup
      Assert-Equal 0 $LASTEXITCODE ("T2[$variant] restore 退出码 0")
      Assert-Equal $seedSha (Get-Sha256Hex $listFile) ("T2[$variant] restored_sha == before_sha")
      Assert-Equal $treeBefore (Get-TreeSnapshot $guildBase) ("T2[$variant] 往返后 GuildBase 树一致")
    } finally { Remove-Item -LiteralPath $t2 -Recurse -Force -ErrorAction SilentlyContinue }
  }

  Write-Host '--- T3 目标已存在 => 拒绝 ---'
  $t3 = New-TempRoot 't3'
  try {
    $root = Join-Path $t3 'M2GameSvr'
    $guildsDir = Join-Path $root 'GuildBase\Guilds'
    [void](New-Item -ItemType Directory -Path $guildsDir -Force)
    $guildFile = Join-Path $guildsDir ($g + '.txt')
    [System.IO.File]::WriteAllBytes($guildFile, $enc.GetBytes('seed'))
    $seedSha = Get-Sha256Hex $guildFile
    $backup = Join-Path $t3 'backup'
    Assert-Throws { Invoke-Prepare -RootDir $root -Name $g -MemberName $m -BackupDirPath $backup } 'T3 已存在目标被拒绝'
    Assert-Equal $seedSha (Get-Sha256Hex $guildFile) 'T3 拒绝后目标文件未被改'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $root 'GuildBase\GuildList.txt'))) 'T3 拒绝后未写 GuildList'
  } finally { Remove-Item -LiteralPath $t3 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- T4 GBK / CRLF 字节级断言 ---'
  $t4 = New-TempRoot 't4'
  try {
    $root = Join-Path $t4 'M2GameSvr'
    [void](New-Item -ItemType Directory -Path (Join-Path $root 'GuildBase') -Force)
    $backup = Join-Path $t4 'backup'
    $r = Invoke-Prepare -RootDir $root -Name $g -MemberName $m -BackupDirPath $backup
    $bytes = [System.IO.File]::ReadAllBytes($r.GuildFile)
    $expectedText = ((Get-GuildFileLines $m) -join "`r`n") + "`r`n"
    $expectedBytes = $enc.GetBytes($expectedText)
    Assert-Equal (Get-Sha256HexBytes $expectedBytes) (Get-Sha256HexBytes $bytes) 'T4 整文件字节 == GBK(四段头+掌门+成员)+CRLF'
    $decoded = $enc.GetString($bytes)
    $lines = $decoded -split "`r`n"
    $expectLines = Get-GuildFileLines $m
    Assert-Equal ($expectLines.Count + 1) $lines.Count 'T4 行数（末行 CRLF 后多一空元素）匹配'
    for ($i = 0; $i -lt $expectLines.Count; $i++) {
      Assert-Equal $expectLines[$i] $lines[$i] ('T4 行[' + $i + '] 内容')
    }
    Assert-Equal '' $lines[$lines.Count - 1] 'T4 末元素为空（证明文件以 CRLF 结尾）'
    Assert-True (Test-ByteStreamIsCrlf $bytes) 'T4 所有 LF 前均有 CR（无裸 LF）'
    Assert-True (-not ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF)) 'T4 无 UTF-8 BOM'
    Assert-True (Test-ContainsSubsequence $bytes ($enc.GetBytes('+' + $m))) 'T4 含 GBK 编码的成员行 +<角色名>'
    Assert-True (Test-ContainsSubsequence $bytes $enc.GetBytes($script:S_NOTICE)) 'T4 含 GBK 编码「公告」'
    $utf8Notice = [System.Text.Encoding]::UTF8.GetBytes($script:S_NOTICE)
    Assert-True (-not (Test-ContainsSubsequence $bytes $utf8Notice)) 'T4 不含「公告」的 UTF-8 字节（确为 GBK）'
  } finally { Remove-Item -LiteralPath $t4 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- T5 -WhatIf 零写 ---'
  $t5 = New-TempRoot 't5'
  try {
    $root = Join-Path $t5 'M2GameSvr'
    [void](New-Item -ItemType Directory -Path (Join-Path $root 'GuildBase') -Force)
    $backup = Join-Path $t5 'backup'
    $digestBefore = Get-TreeDigest $t5
    $r = Invoke-Prepare -RootDir $root -Name $g -MemberName $m -BackupDirPath $backup -PlanOnly
    Assert-Equal 'NOT_WRITTEN' $r.AfterSha 'T5 WhatIf 报 NOT_WRITTEN'
    Assert-True (-not (Test-Path -LiteralPath $r.GuildFile)) 'T5 WhatIf 未写 <名>.txt'
    Assert-True (-not (Test-Path -LiteralPath $r.ListFile)) 'T5 WhatIf 未写 GuildList'
    Assert-True (-not (Test-Path -LiteralPath $backup)) 'T5 WhatIf 未建备份目录'
    Assert-Equal $digestBefore (Get-TreeDigest $t5) 'T5 WhatIf 前后树摘要一致'
  } finally { Remove-Item -LiteralPath $t5 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- T6 真实根护栏 ---'
  Assert-True (Test-IsProtectedPath 'E:/MirServer') 'T6 E:/MirServer 判为受保护'
  Assert-True (Test-IsProtectedPath 'E:/MirServer/M2GameSvr') 'T6 E:/MirServer 子孙判为受保护'
  Assert-True (Test-IsProtectedPath 'E:\MirServer\M2GameSvr\GuildBase') 'T6 反斜杠形态也判为受保护'
  Assert-True (-not (Test-IsProtectedPath ([System.IO.Path]::GetTempPath()))) 'T6 系统 temp 不是受保护根'
  Assert-Throws { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $true $false } 'T6 Selftest 下真实根被拒'
  Assert-Throws { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $false $false } 'T6 非 Selftest 未 -Force 命中真实根被拒'
  $forceOk = $false
  try { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $false $true; $forceOk = $true } catch { $forceOk = $false }
  Assert-True $forceOk 'T6 -Force 下真实根放行（显式）'

  # T7 生产根零触碰（只读旁观）
  $watchAfter = if (Test-Path -LiteralPath $watchRoot) { Get-TreeDigest $watchRoot } else { 'ABSENT' }
  Assert-Equal $watchBefore $watchAfter 'T7 E:/MirServer/M2GameSvr/GuildBase 自测前后一致（零写入）'

  if ($script:SelFail -ne 0) { throw ("SELFTEST FAILED: " + $script:SelFail + " 项失败") }
  Write-Host 'SELFTEST PASS guild_fixture.ps1'
}

# ---------------------------------------------------------------- entrypoint ----
try {
  if ($Selftest) {
    if (-not [string]::IsNullOrWhiteSpace($Root)) {
      Assert-SafeRoot (Get-NormalizedFullPath $Root) $true $false
      Write-Host "WARN Selftest 忽略 -Root（自测一律用 tempdir）：$Root"
    }
    Invoke-Selftest
    exit 0
  }
  Invoke-PrepareCli
  exit 0
} catch {
  Write-Host ('RED: ' + $_.Exception.Message)
  exit 2
}
