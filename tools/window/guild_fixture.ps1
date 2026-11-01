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
#   * 多成员形态（serializer：mir2-rs/crates/data/src/guild.rs::serialize_guild_file:643-648）：
#     逐 rank 输出 `#<n> <rank名>` + 该 rank 下每个成员的 `+<name>`；本夹具的映射是
#     第一个成员 ⇒ rank#1（掌门），其余成员 ⇒ 同一条 rank#2（普通成员）。
#     段名缺省取部署 String.conf:289-291 的实际值：GuildChief=掌门人 / GuildMemberRank=行会成员。
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
#   # 两成员（第一个=掌门 rank#1，其余=rank#2 普通成员）：-Member 仍等价单成员旧用法
#   pwsh -NoProfile -File tools/window/guild_fixture.ps1 `
#       -Root E:/MirServer/M2GameSvr -GuildName e2e-guild -Members winplay0,aaa `
#       -BackupDir E:/tmp/guild-window/backup -Force
#   # 自定义段名（缺省即上面 String.conf:289-291 的值）
#   pwsh -NoProfile -File tools/window/guild_fixture.ps1 ... -Members a,b -ChiefRank 掌门人 -MemberRank 行会成员
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
# 字节断言、两成员逐字节断言与往返 restore、单成员路径逐字节回归（钉住旧版基线 sha）、
# -Member/-Members 参数解析、-WhatIf 零写、真实根硬拒绝，
# 并断言 E:/MirServer/M2GameSvr/GuildBase 自测前后不变。

param(
  [string]$Root = '',
  [string]$GuildName = '',
  # -Member（单成员，兼容旧用法）与 -Members（逗号分隔）二选一：
  #   -Members 的第一个 = 掌门（rank#1），其余全部 = rank#2 普通成员。
  [string]$Member = '',
  [string[]]$Members = @(),
  # 可选段名；缺省 = 部署 String.conf:289-291 的 GuildChief / GuildMemberRank 实际值。
  [string]$ChiefRank = '',
  [string]$MemberRank = '',
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
# 段名缺省值 = 部署 String.conf:289-291 的实际值（GBK 解码）：
#   289 GuildMember=行会成员 / 290 GuildMemberRank=行会成员 / 291 GuildChief=掌门人。
$script:DEFAULT_CHIEF_RANK  = $script:S_CHIEF
$script:DEFAULT_MEMBER_RANK = $script:S_MEMBER

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

function Assert-ValidMember([string]$member, [string]$optName = '-Member') {
  if ([string]::IsNullOrWhiteSpace($member)) { throw "缺少 $optName" }
  if ($member -ne $member.Trim()) { throw "$optName 首尾不能有空白：[$member]" }
  if ($member -match '[ ,]') { throw "$optName 不能含空格或逗号（oracle GetValidStr3 分词符）：$member" }
}

# 解析成员表：-Member（单元素）与 -Members（逗号分隔）二选一。
# -Members 每个元素会再按逗号拆，故 `-Members a,b` 与 `-Members "a,b"` 等价。
# 返回 [string[]]（调用方直接赋值即可，勿再套 @()，否则会多包一层）。
function Resolve-Members([string]$single, [string[]]$many) {
  $hasSingle = -not [string]::IsNullOrWhiteSpace($single)
  $list = New-Object System.Collections.Generic.List[string]
  foreach ($raw in @($many)) {
    if ([string]::IsNullOrWhiteSpace($raw)) { continue }
    foreach ($part in @($raw -split ',')) {
      $t = $part.Trim()
      if ($t -eq '') { throw "Members 含空项（逗号分隔的名单不能有空元素）：[$raw]" }
      $list.Add($t)
    }
  }
  if ($hasSingle -and $list.Count -gt 0) { throw '不能同时给 -Member 与 -Members（二选一）' }
  $resolved = if ($list.Count -gt 0) { [string[]]$list.ToArray() } elseif ($hasSingle) { [string[]]@($single) } else { throw '缺少 -Member 或 -Members' }
  foreach ($n in $resolved) { Assert-ValidMember $n '-Members' }
  $seen = @{}
  foreach ($n in $resolved) {
    if ($seen.ContainsKey($n)) { throw "成员重复：$n（oracle 会写出重复 + 行）" }
    $seen[$n] = $true
  }
  return ,([string[]]$resolved)
}

# 校验自定义段名：非空、无首尾空白、无换行；长度按 oracle 装载上限
# （GuildInfo.cs:314 `GuildRankNameLen`，部署默认 16，见 GameSvrConf.cs:1720）。
function Resolve-RankName([string]$value, [string]$defaultValue, [string]$optName) {
  if ([string]::IsNullOrWhiteSpace($value)) { return $defaultValue }
  if ($value -ne $value.Trim()) { throw "$optName 首尾不能有空白：[$value]" }
  if ($value -match "[`r`n]") { throw "$optName 不能含换行：[$value]" }
  if ($value.Length -gt 16) { throw "$optName 过长（oracle GuildRankNameLen=16）：$value" }
  return $value
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

# 期望的 <名>.txt 行序列（与 oracle SaveGuildFile / serialize_guild_file:643-648 同形）：
#   公告 / ' ' / 敌对行会 / ' ' / 联盟行会 / ' ' / 行会成员 /
#   #1 <chiefRank> / +<成员1> [/ #2 <memberRank> / +<成员2> ...]
# 单成员 ⇒ 只有 rank#1 段（与旧版逐字节一致）；多成员 ⇒ 其余全部落在同一条 rank#2 段。
function Get-GuildFileLines([string[]]$members, [string]$chiefRank, [string]$memberRank) {
  $lines = @(
    $script:S_NOTICE,
    ' ',
    $script:S_WAR,
    ' ',
    $script:S_ALL,
    ' ',
    $script:S_MEMBER,
    ('#1 ' + $chiefRank),
    ('+' + $members[0])
  )
  if ($members.Count -gt 1) {
    $lines += ('#2 ' + $memberRank)
    for ($i = 1; $i -lt $members.Count; $i++) { $lines += ('+' + $members[$i]) }
  }
  return $lines
}

function Get-GuildFileBytes([string[]]$members, [string]$chiefRank, [string]$memberRank) {
  $text = ((Get-GuildFileLines $members $chiefRank $memberRank) -join "`r`n") + "`r`n"
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
    [string[]]$MemberNames,
    [string]$ChiefRankName = '',
    [string]$MemberRankName = '',
    [string]$BackupDirPath,
    [switch]$PlanOnly
  )

  if ($null -eq $MemberNames -or $MemberNames.Count -lt 1) { throw '内部错误：MemberNames 为空' }
  if ([string]::IsNullOrWhiteSpace($ChiefRankName)) { $ChiefRankName = $script:DEFAULT_CHIEF_RANK }
  if ([string]::IsNullOrWhiteSpace($MemberRankName)) { $MemberRankName = $script:DEFAULT_MEMBER_RANK }

  $guildBase = Join-Path $RootDir 'GuildBase'
  $guildsDir = Join-Path $guildBase 'Guilds'
  $listFile = Join-Path $guildBase $script:ListFileName
  $guildFile = Join-Path $guildsDir ($Name + '.txt')
  $iniFile = Join-Path $guildsDir ($Name + '.ini')

  $beforeExists = Test-Path -LiteralPath $listFile -PathType Leaf
  # 不能写成 `$beforeBytes = if (...) { ReadAllBytes(...) } else { ... }`：if 表达式会把 byte[]
  # 展开进管道——0 字节文件变成 $null（Get-Sha256HexBytes/ComputeHash 报「多个不明确的重载」），
  # 单字节文件变成标量 [byte]（.Length 恒 1、末字节判断失真）。先置空数组再**直接**赋值，保住
  # byte[] 语义。触发输入：既有 GuildList.txt 为 0 字节（部署现网即如此）或单字节。
  [byte[]]$beforeBytes = @()
  if ($beforeExists) { $beforeBytes = [System.IO.File]::ReadAllBytes($listFile) }
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

  $guildBytes = Get-GuildFileBytes $MemberNames $ChiefRankName $MemberRankName

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
      Member = $MemberNames[0]
      Members = $MemberNames
      ChiefRank = $ChiefRankName
      MemberRank = $MemberRankName
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
    member = $MemberNames[0]
    members = @($MemberNames)
    chief_rank = $ChiefRankName
    member_rank = $MemberRankName
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
    Member = $MemberNames[0]
    Members = $MemberNames
    ChiefRank = $ChiefRankName
    MemberRank = $MemberRankName
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
  $memberNames = Resolve-Members -Single $Member -Many $Members
  $chiefRankName = Resolve-RankName $ChiefRank $script:DEFAULT_CHIEF_RANK '-ChiefRank'
  $memberRankName = Resolve-RankName $MemberRank $script:DEFAULT_MEMBER_RANK '-MemberRank'
  $backupPath = if ([string]::IsNullOrWhiteSpace($BackupDir)) {
    Join-Path ([System.IO.Path]::GetTempPath()) ('guild-fixture-backup-' + (Get-Date).ToString('yyyyMMdd-HHmmss'))
  } else {
    Get-NormalizedFullPath $BackupDir
  }

  $result = Invoke-Prepare -RootDir $rootPath -Name $GuildName -MemberNames $memberNames `
    -ChiefRankName $chiefRankName -MemberRankName $memberRankName `
    -BackupDirPath $backupPath -PlanOnly:$WhatIf.IsPresent

  Write-Host ('FIXTURE ' + $result.GuildName)
  Write-Host ('ROOT ' + $result.Root)
  Write-Host ('MEMBERS ' + ($result.Members -join ','))
  Write-Host ('CHIEF_RANK ' + $result.ChiefRank)
  Write-Host ('MEMBER_RANK ' + $result.MemberRank)
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
$script:SelPass = 0
function Assert-True([bool]$cond, [string]$msg) {
  if ($cond) {
    $script:SelPass++
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
    $r1 = Invoke-Prepare -RootDir $root -Name $g -MemberNames @($m) -BackupDirPath $backup
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
      $r = Invoke-Prepare -RootDir $root -Name $g -MemberNames @($m) -BackupDirPath $backup
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

  Write-Host '--- T2b 既有 GuildList 为 0 字节 / 单字节（无尾 LF）：prep -> restore 往返 ---'
  # 回归牙：if 表达式会把 byte[] 展开进管道（0 字节 -> $null，单字节 -> 标量 [byte]），
  # 旧实现在 0 字节既有文件上 prep 必崩（ComputeHash 多个不明确的重载）。
  $t2bCases = @(
    @{ tag = 'empty'; seed = [byte[]]::new(0) },
    @{ tag = 'one-byte'; seed = [byte[]](0x41) }
  )
  foreach ($case in $t2bCases) {
    $t2b = New-TempRoot ('t2b-' + $case.tag)
    try {
      $root = Join-Path $t2b 'M2GameSvr'
      $guildBase = Join-Path $root 'GuildBase'
      [void](New-Item -ItemType Directory -Path $guildBase -Force)
      $listFile = Join-Path $guildBase 'GuildList.txt'
      [System.IO.File]::WriteAllBytes($listFile, $case.seed)
      $seedSha = Get-Sha256HexBytes $case.seed
      $treeBefore = Get-TreeSnapshot $guildBase
      $backup = Join-Path $t2b 'backup'
      $r = Invoke-Prepare -RootDir $root -Name $g -MemberNames @($m) -BackupDirPath $backup
      Assert-Equal $seedSha $r.BeforeSha ('T2b[' + $case.tag + '] before_sha = 原字节')
      $suffix = $enc.GetBytes($g)
      if ($case.seed.Length -gt 0 -and $case.seed[$case.seed.Length - 1] -ne 0x0A) {
        $suffix = New-SequenceBytes ([byte[]](0x0D, 0x0A)) $suffix
      }
      $expectedList = New-SequenceBytes $case.seed (New-SequenceBytes $suffix ([byte[]](0x0D, 0x0A)))
      Assert-Equal (Get-Sha256HexBytes $expectedList) $r.AfterSha ('T2b[' + $case.tag + '] after_sha = 原字节+GBK(名)+CRLF')
      Assert-Equal (Get-Sha256HexBytes $expectedList) (Get-Sha256Hex $listFile) ('T2b[' + $case.tag + '] 磁盘字节与期望一致')
      # Get-GuildListNames 返回单元素时会被管道展开成标量字符串，必须 @() 包裹后再取下标。
      $got = @(Get-GuildListNames $listFile)
      $expectedCount = if ($case.seed.Length -gt 0) { 2 } else { 1 }
      Assert-Equal $expectedCount $got.Count ('T2b[' + $case.tag + '] 名单条数')
      Assert-Equal $g $got[$got.Count - 1] ('T2b[' + $case.tag + '] 新行会在名单末尾')

      $restore = Join-Path $PSScriptRoot 'guild_fixture_restore.ps1'
      $out = & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $restore -Root $root -GuildName $g -BackupDir $backup 2>&1
      $restoreRc = $LASTEXITCODE
      Assert-Equal 0 $restoreRc ('T2b[' + $case.tag + '] restore 退出码 0')
      Assert-True (($out | Out-String) -match ('RESTORED_SHA ' + $seedSha)) ('T2b[' + $case.tag + '] restore 输出 restored_sha = 原 sha')
      Assert-True (Test-Path -LiteralPath $listFile -PathType Leaf) ('T2b[' + $case.tag + '] restore 后 GuildList 仍在（原有文件不当 ABSENT）')
      Assert-Equal $seedSha (Get-Sha256Hex $listFile) ('T2b[' + $case.tag + '] restored_sha == before_sha')
      Assert-Equal $case.seed.Length ([System.IO.File]::ReadAllBytes($listFile)).Length ('T2b[' + $case.tag + '] 恢复后字节数一致')
      Assert-Equal $treeBefore (Get-TreeSnapshot $guildBase) ('T2b[' + $case.tag + '] 往返后 GuildBase 树一致')
    } finally { Remove-Item -LiteralPath $t2b -Recurse -Force -ErrorAction SilentlyContinue }
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
    Assert-Throws { Invoke-Prepare -RootDir $root -Name $g -MemberNames @($m) -BackupDirPath $backup } 'T3 已存在目标被拒绝'
    Assert-Equal $seedSha (Get-Sha256Hex $guildFile) 'T3 拒绝后目标文件未被改'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $root 'GuildBase\GuildList.txt'))) 'T3 拒绝后未写 GuildList'
  } finally { Remove-Item -LiteralPath $t3 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- T4 GBK / CRLF 字节级断言 ---'
  $t4 = New-TempRoot 't4'
  try {
    $root = Join-Path $t4 'M2GameSvr'
    [void](New-Item -ItemType Directory -Path (Join-Path $root 'GuildBase') -Force)
    $backup = Join-Path $t4 'backup'
    $r = Invoke-Prepare -RootDir $root -Name $g -MemberNames @($m) -BackupDirPath $backup
    $bytes = [System.IO.File]::ReadAllBytes($r.GuildFile)
    $expectedText = ((Get-GuildFileLines @($m) $script:DEFAULT_CHIEF_RANK $script:DEFAULT_MEMBER_RANK) -join "`r`n") + "`r`n"
    $expectedBytes = $enc.GetBytes($expectedText)
    Assert-Equal (Get-Sha256HexBytes $expectedBytes) (Get-Sha256HexBytes $bytes) 'T4 整文件字节 == GBK(四段头+掌门+成员)+CRLF'
    $decoded = $enc.GetString($bytes)
    $lines = $decoded -split "`r`n"
    $expectLines = Get-GuildFileLines @($m) $script:DEFAULT_CHIEF_RANK $script:DEFAULT_MEMBER_RANK
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
    $r = Invoke-Prepare -RootDir $root -Name $g -MemberNames @($m) -BackupDirPath $backup -PlanOnly
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

  Write-Host '--- T8 两成员（掌门 rank#1 + 普通成员 rank#2）：字节断言 + 往返 restore ---'
  $t8 = New-TempRoot 't8'
  try {
    $root = Join-Path $t8 'M2GameSvr'
    $guildBase = Join-Path $root 'GuildBase'
    [void](New-Item -ItemType Directory -Path $guildBase -Force)
    $backup = Join-Path $t8 'backup'
    $treeBefore = Get-TreeSnapshot $guildBase
    $chief = 'winplay0'
    $plain = 'aaa'
    $r = Invoke-Prepare -RootDir $root -Name $g -MemberNames @($chief, $plain) -BackupDirPath $backup
    Assert-Equal 'ABSENT' $r.BeforeSha 'T8 原 GuildList 记为 ABSENT'
    # 独立字节期望（不调用被测的 Get-GuildFileLines）：
    # GBK(四段头 + `#1 掌门人/+winplay0` + `#2 行会成员/+aaa`) + 每行 CRLF。
    $twoMemberHex = 'b9abb8e60d0a200d0ab5d0b6d4d0d0bbe10d0a200d0ac1aac3cbd0d0bbe10d0a200d0ad0d0bbe1b3c9d4b10d0a233120d5c6c3c5c8cb0d0a2b77696e706c6179300d0a233220d0d0bbe1b3c9d4b10d0a2b6161610d0a'
    $bytes8 = [System.IO.File]::ReadAllBytes($r.GuildFile)
    Assert-Equal $twoMemberHex (([BitConverter]::ToString($bytes8) -replace '-', '').ToLowerInvariant()) 'T8 两成员文件逐字节断言'
    Assert-Equal 86 $bytes8.Length 'T8 文件字节数 86'
    $lines8 = $enc.GetString($bytes8) -split "`r`n"
    Assert-Equal 12 $lines8.Count 'T8 行数（11 行内容 + 末 CRLF 后空元素）'
    Assert-Equal ('#1 ' + $script:DEFAULT_CHIEF_RANK) $lines8[7] 'T8 行[7] = #1 掌门人'
    Assert-Equal ('+' + $chief) $lines8[8] 'T8 行[8] = +winplay0'
    Assert-Equal ('#2 ' + $script:DEFAULT_MEMBER_RANK) $lines8[9] 'T8 行[9] = #2 行会成员'
    Assert-Equal ('+' + $plain) $lines8[10] 'T8 行[10] = +aaa'
    Assert-Equal '' $lines8[11] 'T8 末尾为空（文件以 CRLF 结尾）'
    Assert-True (Test-ByteStreamIsCrlf $bytes8) 'T8 全部 LF 前均有 CR'
    Assert-True (-not ($bytes8.Length -ge 3 -and $bytes8[0] -eq 0xEF -and $bytes8[1] -eq 0xBB -and $bytes8[2] -eq 0xBF)) 'T8 无 UTF-8 BOM'
    Assert-Equal (Get-Sha256HexBytes (New-SequenceBytes $enc.GetBytes($g) ([byte[]](0x0D, 0x0A)))) $r.AfterSha 'T8 GuildList = GBK(名)+CRLF'
    $restore = Join-Path $PSScriptRoot 'guild_fixture_restore.ps1'
    & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $restore -Root $root -GuildName $g -BackupDir $backup
    Assert-Equal 0 $LASTEXITCODE 'T8 restore 退出码 0'
    Assert-True (-not (Test-Path -LiteralPath $r.ListFile)) 'T8 restore 后 GuildList 已删（原 ABSENT）'
    Assert-True (-not (Test-Path -LiteralPath $r.GuildFile)) 'T8 restore 后 <名>.txt 已删'
    Assert-True (-not (Test-Path -LiteralPath $r.GuildsDir)) 'T8 restore 后空 Guilds 目录已删'
    Assert-Equal $treeBefore (Get-TreeSnapshot $guildBase) 'T8 往返后 GuildBase 树一致'
  } finally { Remove-Item -LiteralPath $t8 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- T8b 自定义 -ChiefRank / -MemberRank 落盘 ---'
  $t8b = New-TempRoot 't8b'
  try {
    $root = Join-Path $t8b 'M2GameSvr'
    $guildBase = Join-Path $root 'GuildBase'
    [void](New-Item -ItemType Directory -Path $guildBase -Force)
    $backup = Join-Path $t8b 'backup'
    $customChief = -join [char[]](0x4F1A, 0x957F)    # 会长
    $customMember = -join [char[]](0x6210, 0x5458)   # 成员
    $r = Invoke-Prepare -RootDir $root -Name $g -MemberNames @('u1', 'u2') `
      -ChiefRankName $customChief -MemberRankName $customMember -BackupDirPath $backup
    $linesB = $enc.GetString([System.IO.File]::ReadAllBytes($r.GuildFile)) -split "`r`n"
    Assert-Equal ('#1 ' + $customChief) $linesB[7] 'T8b 自定义掌门段名落盘'
    Assert-Equal ('#2 ' + $customMember) $linesB[9] 'T8b 自定义成员段名落盘'
    Assert-Equal '+u2' $linesB[10] 'T8b 第二个成员在 rank#2 段内'
  } finally { Remove-Item -LiteralPath $t8b -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- T9 单成员路径逐字节回归（钉住旧版基线） ---'
  # 基线来自扩能前的实现（单成员 测试角色），sha256 282b71a4…4fd8；本卡不得改变单成员语义。
  $singleHex = 'b9abb8e60d0a200d0ab5d0b6d4d0d0bbe10d0a200d0ac1aac3cbd0d0bbe10d0a200d0ad0d0bbe1b3c9d4b10d0a233120d5c6c3c5c8cb0d0a2bb2e2cad4bdc7c9ab0d0a'
  $singleBytes = Get-GuildFileBytes @($m) $script:DEFAULT_CHIEF_RANK $script:DEFAULT_MEMBER_RANK
  Assert-Equal $singleHex (([BitConverter]::ToString($singleBytes) -replace '-', '').ToLowerInvariant()) 'T9 单成员文件逐字节 == 旧版基线'
  Assert-Equal '282b71a4ab410950f8312fcc0134569183ff3e18a39f2d9632a0f7ef4d414fd8' (Get-Sha256HexBytes $singleBytes) 'T9 单成员 sha256 == 旧版基线'
  $singleLines = @(Get-GuildFileLines @($m) $script:DEFAULT_CHIEF_RANK $script:DEFAULT_MEMBER_RANK)
  Assert-Equal 9 $singleLines.Count 'T9 单成员只有 rank#1 段（9 行）'
  Assert-True (-not ((($singleLines -join "`r`n")) -match '#2 ')) 'T9 单成员不含 #2 段'

  Write-Host '--- T10 成员参数解析（-Member / -Members 二选一） ---'
  $rm1 = Resolve-Members -Single $m -Many @()
  Assert-Equal 1 $rm1.Count 'T10 -Member 等价单元素'
  $rm2 = Resolve-Members -Single '' -Many @('a,b')
  Assert-Equal $rm2.Count 2 'T10 -Members 逗号分隔拆成 2'
  $rm3 = Resolve-Members -Single '' -Many @('a,b,c')
  Assert-Equal $rm3.Count 3 'T10 -Members 三成员'
  Assert-Equal $rm2[1] 'b' 'T10 顺序保持（第二个 = b）'
  Assert-Throws { Resolve-Members -Single 'x' -Many @('y') } 'T10 同时给 -Member 与 -Members 被拒'
  Assert-Throws { Resolve-Members -Single '' -Many @('a,,b') } 'T10 空项被拒'
  Assert-Throws { Resolve-Members -Single '' -Many @('dup','dup') } 'T10 重复成员被拒'

  # T7 生产根零触碰（只读旁观）
  $watchAfter = if (Test-Path -LiteralPath $watchRoot) { Get-TreeDigest $watchRoot } else { 'ABSENT' }
  Assert-Equal $watchBefore $watchAfter 'T7 E:/MirServer/M2GameSvr/GuildBase 自测前后一致（零写入）'

  # 元断言：断言条数只增不减（防新增用例被静默跳过而假绿）。基线 79 + T8/T8b/T9/T10 31 = 110。
  Assert-True ($script:SelPass -ge 109) ('TM 自测断言条数 >= 109（当前 ' + $script:SelPass + '）')

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
