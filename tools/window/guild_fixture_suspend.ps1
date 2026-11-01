# -*- coding: utf-8 -*-
# guild_fixture_suspend.ps1 — 行会窗「B 失败形态」制造脚本（卡 openmir2-guild-window-fixture）
#
# 用途：窗口中间把 Guilds/<名>.txt 改名成 <名>.txt.suspended，制造「文件不存在」形态，
#       让 oracle 的下一次行会保存走 SaveToFile(FileMode.Open) 的静默失败分支
#       （GuildInfo.cs:384-431 的 try/catch 吞掉异常，只在日志留 ERR「保存行会信息失败!!!」）。
#       由 guild_fixture_resume.ps1 改回并断言 sha 还原。
#
# 只改名，不删、不改写文件内容；before_sha 写入状态文件（restore 用它断言还原）。
#
# 安全：与 guild_fixture.ps1 同一套护栏：-Selftest 禁止真实根；非 Selftest 命中
#       E:\MirServer 及子孙必须显式 -Force。-WhatIf 只打印计划，不改名、不建备份目录。
#
# 用法：
#   pwsh -NoProfile -File tools/window/guild_fixture_suspend.ps1 -Selftest
#   pwsh -NoProfile -File tools/window/guild_fixture_suspend.ps1 `
#       -Root E:/MirServer/M2GameSvr -GuildName e2e-guild `
#       -BackupDir E:/tmp/guild-window/suspend -Force
#
# 输出：SUSPENDED_FILE <path>  BEFORE_SHA <hex>  SUSPENDED_SHA <hex>  STATE_FILE <path>
# 退出码：0 = 成功/自测全绿；2 = 拒绝或失败。

param(
  [string]$Root = '',
  [string]$GuildName = '',
  [string]$BackupDir = '',
  [switch]$WhatIf,
  [switch]$Selftest,
  [switch]$Force
)

$ErrorActionPreference = 'Stop'

$script:ProtectedRealRoot = 'E:\MirServer'
$script:SuspendStateName = 'guild-fixture-suspend.json'

function Get-Sha256HexBytes([byte[]]$bytes) {
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try { return ([BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant() }
  finally { $sha.Dispose() }
}
function Get-Sha256HexText([string]$text) {
  return Get-Sha256HexBytes ([System.Text.Encoding]::UTF8.GetBytes($text))
}
function Get-Sha256Hex([string]$path) {
  try {
    if (Get-Command Get-FileHash -ErrorAction Stop) {
      return (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
  } catch { }
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $stream = [System.IO.File]::OpenRead($path)
    try { return ([BitConverter]::ToString($sha.ComputeHash($stream)) -replace '-', '').ToLowerInvariant() }
    finally { $stream.Dispose() }
  } finally { $sha.Dispose() }
}
function Get-NormalizedFullPath([string]$path) {
  if ([string]::IsNullOrWhiteSpace($path)) { return '' }
  if (-not [System.IO.Path]::IsPathRooted($path)) { throw "路径必须是绝对路径：$path" }
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
function Get-TreeDigest([string]$path) { return Get-Sha256HexText (Get-TreeSnapshot $path) }

function Get-SuspendedPath([string]$guildFile) { return $guildFile + '.suspended' }

function Invoke-Suspend {
  param(
    [string]$RootDir,
    [string]$Name,
    [string]$BackupDirPath,
    [switch]$PlanOnly
  )

  if ([string]::IsNullOrWhiteSpace($Name)) { throw '缺少 -GuildName' }
  if ($Name -match '[\\/:*?"<>|]') { throw "GuildName 含非法文件名字符：$Name" }

  $guildBase = Join-Path $RootDir 'GuildBase'
  $guildsDir = Join-Path $guildBase 'Guilds'
  $guildFile = Join-Path $guildsDir ($Name + '.txt')
  $suspendedFile = Get-SuspendedPath $guildFile

  if (-not (Test-Path -LiteralPath $guildFile -PathType Leaf)) {
    throw "目标行会文件不存在，无法制造 B 形态（fail-closed）：$guildFile"
  }
  if (Test-Path -LiteralPath $suspendedFile) {
    throw "已存在 .suspended 文件，拒绝覆盖（fail-closed）：$suspendedFile"
  }

  $beforeSha = Get-Sha256Hex $guildFile
  $planned = @(
    ('MOVE ' + $guildFile + ' -> ' + $suspendedFile),
    ('STATE ' + $script:SuspendStateName)
  )

  if ($PlanOnly) {
    return [pscustomobject]@{
      Root = $RootDir
      GuildName = $Name
      GuildFile = $guildFile
      SuspendedFile = $suspendedFile
      StateFile = ''
      BeforeSha = $beforeSha
      SuspendedSha = 'NOT_MOVED'
      Planned = $planned
    }
  }

  if ([string]::IsNullOrWhiteSpace($BackupDirPath)) { throw '内部错误：未提供 BackupDir' }
  if (-not (Test-Path -LiteralPath $BackupDirPath)) {
    [void](New-Item -ItemType Directory -Path $BackupDirPath -Force)
  }
  $stateFile = Join-Path $BackupDirPath $script:SuspendStateName

  $state = [ordered]@{
    tool = 'guild_fixture_suspend'
    version = 1
    guild_name = $Name
    root = $RootDir
    guild_file = $guildFile
    suspended_file = $suspendedFile
    before_sha = $beforeSha
    suspended_at = (Get-Date).ToString('o')
  }
  $utf8 = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($stateFile, ($state | ConvertTo-Json -Depth 5), $utf8)

  Move-Item -LiteralPath $guildFile -Destination $suspendedFile -Force

  if (Test-Path -LiteralPath $guildFile) { throw "改名后原文件仍存在：$guildFile" }
  if (-not (Test-Path -LiteralPath $suspendedFile -PathType Leaf)) { throw "改名后未找到 suspended 文件：$suspendedFile" }
  $suspendedSha = Get-Sha256Hex $suspendedFile
  if ($suspendedSha -ne $beforeSha) {
    throw "改名后 sha 变化：before=$beforeSha suspended=$suspendedSha"
  }

  return [pscustomobject]@{
    Root = $RootDir
    GuildName = $Name
    GuildFile = $guildFile
    SuspendedFile = $suspendedFile
    StateFile = $stateFile
    BeforeSha = $beforeSha
    SuspendedSha = $suspendedSha
    Planned = $planned
  }
}

function Invoke-SuspendCli {
  if ([string]::IsNullOrWhiteSpace($Root)) { throw '缺少 -Root' }
  if ([string]::IsNullOrWhiteSpace($GuildName)) { throw '缺少 -GuildName' }
  $rootPath = Get-NormalizedFullPath $Root
  Assert-SafeRoot $rootPath $false $Force.IsPresent
  if (-not (Test-Path -LiteralPath $rootPath -PathType Container)) { throw "部署根不存在：$rootPath" }
  $backupPath = if ([string]::IsNullOrWhiteSpace($BackupDir)) {
    Join-Path ([System.IO.Path]::GetTempPath()) ('guild-suspend-' + (Get-Date).ToString('yyyyMMdd-HHmmss'))
  } else { Get-NormalizedFullPath $BackupDir }

  $r = Invoke-Suspend -RootDir $rootPath -Name $GuildName -BackupDirPath $backupPath -PlanOnly:$WhatIf.IsPresent
  Write-Host ('SUSPENDED_FILE ' + $r.SuspendedFile)
  Write-Host ('BEFORE_SHA ' + $r.BeforeSha)
  Write-Host ('SUSPENDED_SHA ' + $r.SuspendedSha)
  Write-Host ('STATE_FILE ' + $r.StateFile)
  foreach ($p in $r.Planned) { Write-Host ('PLAN ' + $p) }
  if ($WhatIf) { Write-Host 'WHATIF 未改名任何文件' }
}

# ---------------------------------------------------------------- selftest ----
$script:SelFail = 0
function Assert-True([bool]$cond, [string]$msg) {
  if ($cond) { Write-Host ('PASS ' + $msg) } else { $script:SelFail++; Write-Host ('FAIL ' + $msg) }
}
function Assert-Equal($actual, $expected, [string]$msg) {
  Assert-True ([string]$actual -eq [string]$expected) ($msg + ' [actual=' + [string]$actual + ' expected=' + [string]$expected + ']')
}
function Assert-Throws([scriptblock]$block, [string]$msg) {
  $threw = $false; $err = ''
  try { & $block } catch { $threw = $true; $err = $_.Exception.Message }
  Assert-True $threw ($msg + ' [err=' + $err + ']')
}
function New-TempRoot([string]$tag) {
  $base = Join-Path ([System.IO.Path]::GetTempPath()) ('guild-suspend-selftest-' + $tag + '-' + [guid]::NewGuid().ToString('N'))
  [void](New-Item -ItemType Directory -Path $base -Force)
  return $base
}
function Get-HostExe { return (Get-Process -Id $PID).Path }
function Get-GbkEncoding { return [System.Text.Encoding]::GetEncoding(936) }

function New-SeedGuild([string]$root, [string]$name, [byte[]]$bytes) {
  $guildsDir = Join-Path $root 'GuildBase\Guilds'
  [void](New-Item -ItemType Directory -Path $guildsDir -Force)
  $guildFile = Join-Path $guildsDir ($name + '.txt')
  [System.IO.File]::WriteAllBytes($guildFile, $bytes)
  return $guildFile
}

function Invoke-Selftest {
  $enc = Get-GbkEncoding
  $g = -join [char[]](0x6D4B, 0x8BD5, 0x884C, 0x4F1A)   # 测试行会
  $resume = Join-Path $PSScriptRoot 'guild_fixture_resume.ps1'
  $watchRoot = Join-Path $script:ProtectedRealRoot 'M2GameSvr\GuildBase'
  $watchBefore = if (Test-Path -LiteralPath $watchRoot) { Get-TreeDigest $watchRoot } else { 'ABSENT' }

  Write-Host '--- S1 改名 -> resume 归还，sha 一致 ---'
  $t1 = New-TempRoot 's1'
  try {
    $root = Join-Path $t1 'M2GameSvr'
    $seed = $enc.GetBytes('seed' + "`r`n")
    $guildFile = New-SeedGuild $root $g $seed
    $seedSha = Get-Sha256HexBytes $seed
    $backup = Join-Path $t1 'backup'
    $r = Invoke-Suspend -RootDir $root -Name $g -BackupDirPath $backup
    Assert-Equal $seedSha $r.BeforeSha 'S1 before_sha = 原文件'
    Assert-True (-not (Test-Path -LiteralPath $guildFile)) 'S1 原文件已改名'
    Assert-True (Test-Path -LiteralPath $r.SuspendedFile -PathType Leaf) 'S1 .suspended 存在'
    Assert-Equal $seedSha $r.SuspendedSha 'S1 suspended sha == before_sha'

    & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $resume -Root $root -GuildName $g -BackupDir $backup
    Assert-Equal 0 $LASTEXITCODE 'S1 resume 退出码 0'
    Assert-True (Test-Path -LiteralPath $guildFile -PathType Leaf) 'S1 原文件已归还'
    Assert-True (-not (Test-Path -LiteralPath $r.SuspendedFile)) 'S1 .suspended 已消失'
    Assert-Equal $seedSha (Get-Sha256Hex $guildFile) 'S1 归还后 sha == before_sha'
  } finally { Remove-Item -LiteralPath $t1 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- S2 目标缺失 => 拒绝 ---'
  $t2 = New-TempRoot 's2'
  try {
    $root = Join-Path $t2 'M2GameSvr'
    [void](New-Item -ItemType Directory -Path (Join-Path $root 'GuildBase\Guilds') -Force)
    Assert-Throws { Invoke-Suspend -RootDir $root -Name $g -BackupDirPath (Join-Path $t2 'backup') } 'S2 缺目标文件被拒绝'
  } finally { Remove-Item -LiteralPath $t2 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- S3 已存在 .suspended => 拒绝（fail-closed） ---'
  $t3 = New-TempRoot 's3'
  try {
    $root = Join-Path $t3 'M2GameSvr'
    $guildFile = New-SeedGuild $root $g ($enc.GetBytes('seed'))
    [System.IO.File]::WriteAllBytes((Get-SuspendedPath $guildFile), $enc.GetBytes('stale'))
    Assert-Throws { Invoke-Suspend -RootDir $root -Name $g -BackupDirPath (Join-Path $t3 'backup') } 'S3 已存在 .suspended 被拒绝'
    Assert-True (Test-Path -LiteralPath $guildFile) 'S3 拒绝后原文件仍在'
  } finally { Remove-Item -LiteralPath $t3 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- S4 -WhatIf 零写 ---'
  $t4 = New-TempRoot 's4'
  try {
    $root = Join-Path $t4 'M2GameSvr'
    $guildFile = New-SeedGuild $root $g ($enc.GetBytes('seed'))
    $backup = Join-Path $t4 'backup'
    $digestBefore = Get-TreeDigest $t4
    $r = Invoke-Suspend -RootDir $root -Name $g -BackupDirPath $backup -PlanOnly
    Assert-Equal 'NOT_MOVED' $r.SuspendedSha 'S4 WhatIf 报 NOT_MOVED'
    Assert-True (Test-Path -LiteralPath $guildFile) 'S4 WhatIf 原文件仍在'
    Assert-True (-not (Test-Path -LiteralPath $r.SuspendedFile)) 'S4 WhatIf 未生成 .suspended'
    Assert-True (-not (Test-Path -LiteralPath $backup)) 'S4 WhatIf 未建备份目录'
    Assert-Equal $digestBefore (Get-TreeDigest $t4) 'S4 WhatIf 前后树摘要一致'
  } finally { Remove-Item -LiteralPath $t4 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- S5 真实根护栏 ---'
  Assert-True (Test-IsProtectedPath 'E:/MirServer/M2GameSvr') 'S5 E:/MirServer 子孙判为受保护'
  Assert-Throws { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $true $false } 'S5 Selftest 下真实根被拒'
  Assert-Throws { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $false $false } 'S5 非 Selftest 未 -Force 命中真实根被拒'
  $forceOk = $false
  try { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $false $true; $forceOk = $true } catch { $forceOk = $false }
  Assert-True $forceOk 'S5 -Force 下真实根放行（显式）'

  $watchAfter = if (Test-Path -LiteralPath $watchRoot) { Get-TreeDigest $watchRoot } else { 'ABSENT' }
  Assert-Equal $watchBefore $watchAfter 'S6 E:/MirServer/M2GameSvr/GuildBase 自测前后一致（零写入）'

  if ($script:SelFail -ne 0) { throw ("SELFTEST FAILED: " + $script:SelFail + " 项失败") }
  Write-Host 'SELFTEST PASS guild_fixture_suspend.ps1'
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
  Invoke-SuspendCli
  exit 0
} catch {
  Write-Host ('RED: ' + $_.Exception.Message)
  exit 2
}
