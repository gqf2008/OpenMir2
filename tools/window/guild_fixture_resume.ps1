# -*- coding: utf-8 -*-
# guild_fixture_resume.ps1 — 行会窗「B 失败形态」归还脚本（卡 openmir2-guild-window-fixture）
#
# 用途：把 guild_fixture_suspend.ps1 改名的 <名>.txt.suspended 改回 <名>.txt，
#       并断言 restored_sha == 状态文件记录的 before_sha。用于窗口收尾。
#
# 安全：目标 <名>.txt 已存在 ⇒ 拒绝覆盖（fail-closed）；.suspended 缺失或 sha 不符 ⇒ 拒绝。
#       与其它窗口脚本同一套真实根护栏：-Selftest 禁止真实根；非 Selftest 命中
#       E:\MirServer 及子孙必须显式 -Force。-WhatIf 只打印计划，不改名。
#
# 用法：
#   pwsh -NoProfile -File tools/window/guild_fixture_resume.ps1 -Selftest
#   pwsh -NoProfile -File tools/window/guild_fixture_resume.ps1 `
#       -Root E:/MirServer/M2GameSvr -GuildName e2e-guild `
#       -BackupDir E:/tmp/guild-window/suspend -Force
#
# 输出：RESTORED_SHA <hex>  BEFORE_SHA <hex>  MATCH <true|false>  SUSPENDED_FILE <path>  STATE_FILE <path>
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

function Read-StateFile([string]$path) {
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "找不到 suspend 状态文件（需与 guild_fixture_suspend.ps1 的同一 -BackupDir）：$path"
  }
  $raw = [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8)
  return ($raw | ConvertFrom-Json)
}

function Test-PathEquals([string]$a, [string]$b) {
  if ([string]::IsNullOrEmpty($a) -or [string]::IsNullOrEmpty($b)) { return $false }
  return ([System.IO.Path]::GetFullPath($a)).TrimEnd('\', '/').Equals(
    ([System.IO.Path]::GetFullPath($b)).TrimEnd('\', '/'), [System.StringComparison]::OrdinalIgnoreCase)
}

function Invoke-Resume {
  param(
    [string]$RootDir,
    [string]$Name,
    [string]$BackupDirPath,
    [switch]$PlanOnly
  )

  if ([string]::IsNullOrWhiteSpace($BackupDirPath)) { throw '缺少 -BackupDir' }
  $stateFile = Join-Path $BackupDirPath $script:SuspendStateName
  $state = Read-StateFile $stateFile

  if ([string]$state.guild_name -ne $Name) {
    throw "状态文件的行会名不匹配：state=$($state.guild_name) 参数=$Name"
  }
  if (-not (Test-PathEquals ([string]$state.root) $RootDir)) {
    throw "状态文件的 Root 不匹配：state=$($state.root) 参数=$RootDir"
  }

  $guildFile = [string]$state.guild_file
  $suspendedFile = [string]$state.suspended_file
  $beforeSha = [string]$state.before_sha

  # 期望的 suspended 路径以 Root 重算，防止状态文件被挪根。
  $expectedGuildFile = Join-Path (Join-Path (Join-Path $RootDir 'GuildBase') 'Guilds') ($Name + '.txt')
  if (-not (Test-PathEquals $guildFile $expectedGuildFile)) {
    throw "状态文件的 guild_file 与 Root 不一致：state=$guildFile 期望=$expectedGuildFile"
  }
  $expectedSuspended = Get-SuspendedPath $expectedGuildFile
  if (-not (Test-PathEquals $suspendedFile $expectedSuspended)) {
    throw "状态文件的 suspended_file 与 Root 不一致：state=$suspendedFile 期望=$expectedSuspended"
  }

  if (-not (Test-Path -LiteralPath $suspendedFile -PathType Leaf)) {
    throw ".suspended 文件不存在，无法归还：$suspendedFile"
  }
  $suspendedSha = Get-Sha256Hex $suspendedFile
  if ($suspendedSha -ne $beforeSha) {
    throw ".suspended 的 sha 与 before_sha 不符（拒绝归还）：suspended=$suspendedSha before=$beforeSha"
  }
  if (Test-Path -LiteralPath $guildFile) {
    throw "目标行会文件已存在，拒绝覆盖（fail-closed）：$guildFile"
  }

  $planned = @(('MOVE ' + $suspendedFile + ' -> ' + $guildFile))

  if ($PlanOnly) {
    return [pscustomobject]@{
      StateFile = $stateFile
      GuildFile = $guildFile
      SuspendedFile = $suspendedFile
      BeforeSha = $beforeSha
      RestoredSha = 'NOT_MOVED'
      Match = $false
      Planned = $planned
    }
  }

  Move-Item -LiteralPath $suspendedFile -Destination $guildFile -Force

  if (Test-Path -LiteralPath $suspendedFile) { throw "归还后 .suspended 仍存在：$suspendedFile" }
  if (-not (Test-Path -LiteralPath $guildFile -PathType Leaf)) { throw "归还后未找到原文件：$guildFile" }
  $restoredSha = Get-Sha256Hex $guildFile
  if ($restoredSha -ne $beforeSha) {
    throw "restored_sha != before_sha：restored=$restoredSha before=$beforeSha"
  }

  return [pscustomobject]@{
    StateFile = $stateFile
    GuildFile = $guildFile
    SuspendedFile = $suspendedFile
    BeforeSha = $beforeSha
    RestoredSha = $restoredSha
    Match = ($restoredSha -eq $beforeSha)
    Planned = $planned
  }
}

function Invoke-ResumeCli {
  if ([string]::IsNullOrWhiteSpace($Root)) { throw '缺少 -Root' }
  if ([string]::IsNullOrWhiteSpace($GuildName)) { throw '缺少 -GuildName' }
  $rootPath = Get-NormalizedFullPath $Root
  Assert-SafeRoot $rootPath $false $Force.IsPresent
  # 状态文件在 -BackupDir 下：-WhatIf 同样必须显式给出，否则连计划都无从读取。
  if ([string]::IsNullOrWhiteSpace($BackupDir)) {
    throw '缺少 -BackupDir（-WhatIf 也必须显式给出：状态文件在 -BackupDir 下，且必须与 guild_fixture_suspend.ps1 使用同一目录）'
  }
  $backupPath = Get-NormalizedFullPath $BackupDir
  if (-not (Test-Path -LiteralPath $backupPath -PathType Container)) {
    throw "备份目录不存在：$backupPath"
  }

  $r = Invoke-Resume -RootDir $rootPath -Name $GuildName -BackupDirPath $backupPath -PlanOnly:$WhatIf.IsPresent
  Write-Host ('RESTORED_SHA ' + $r.RestoredSha)
  Write-Host ('BEFORE_SHA ' + $r.BeforeSha)
  Write-Host ('MATCH ' + ([string]$r.Match).ToLowerInvariant())
  Write-Host ('SUSPENDED_FILE ' + $r.SuspendedFile)
  Write-Host ('STATE_FILE ' + $r.StateFile)
  foreach ($p in $r.Planned) { Write-Host ('PLAN ' + $p) }
  if ($WhatIf) { Write-Host 'WHATIF 未改名任何文件' }
  if (-not $WhatIf -and -not $r.Match) { throw '归还后 MATCH=false' }
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
  $base = Join-Path ([System.IO.Path]::GetTempPath()) ('guild-resume-selftest-' + $tag + '-' + [guid]::NewGuid().ToString('N'))
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
  $suspend = Join-Path $PSScriptRoot 'guild_fixture_suspend.ps1'
  $watchRoot = Join-Path $script:ProtectedRealRoot 'M2GameSvr\GuildBase'
  $watchBefore = if (Test-Path -LiteralPath $watchRoot) { Get-TreeDigest $watchRoot } else { 'ABSENT' }

  Write-Host '--- U1 suspend 后 resume 归还，sha 一致 ---'
  $t1 = New-TempRoot 'u1'
  try {
    $root = Join-Path $t1 'M2GameSvr'
    $seed = $enc.GetBytes('seed' + "`r`n")
    $guildFile = New-SeedGuild $root $g $seed
    $seedSha = Get-Sha256HexBytes $seed
    $backup = Join-Path $t1 'backup'
    & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $suspend -Root $root -GuildName $g -BackupDir $backup
    Assert-Equal 0 $LASTEXITCODE 'U1 suspend 退出码 0'
    Assert-True (-not (Test-Path -LiteralPath $guildFile)) 'U1 已进入 B 形态（原文件不在）'
    $r = Invoke-Resume -RootDir $root -Name $g -BackupDirPath $backup
    Assert-Equal $seedSha $r.BeforeSha 'U1 before_sha = 原 sha'
    Assert-Equal $seedSha $r.RestoredSha 'U1 restored_sha == before_sha'
    Assert-True $r.Match 'U1 MATCH=true'
    Assert-Equal $seedSha (Get-Sha256Hex $guildFile) 'U1 磁盘文件 sha == 原 sha'
    Assert-True (-not (Test-Path -LiteralPath $r.SuspendedFile)) 'U1 .suspended 已消失'
  } finally { Remove-Item -LiteralPath $t1 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- U2 .suspended 被篡改 => 拒绝归还 ---'
  $t2 = New-TempRoot 'u2'
  try {
    $root = Join-Path $t2 'M2GameSvr'
    $guildFile = New-SeedGuild $root $g ($enc.GetBytes('seed'))
    $backup = Join-Path $t2 'backup'
    & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $suspend -Root $root -GuildName $g -BackupDir $backup
    Assert-Equal 0 $LASTEXITCODE 'U2 suspend 退出码 0'
    [System.IO.File]::WriteAllBytes((Get-SuspendedPath $guildFile), $enc.GetBytes('tampered'))
    Assert-Throws { Invoke-Resume -RootDir $root -Name $g -BackupDirPath $backup } 'U2 篡改 .suspended 被拒绝'
    Assert-True (-not (Test-Path -LiteralPath $guildFile)) 'U2 拒绝后原文件仍不在（未误放行）'
  } finally { Remove-Item -LiteralPath $t2 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- U3 目标已存在 => 拒绝覆盖 ---'
  $t3 = New-TempRoot 'u3'
  try {
    $root = Join-Path $t3 'M2GameSvr'
    $guildFile = New-SeedGuild $root $g ($enc.GetBytes('seed'))
    $backup = Join-Path $t3 'backup'
    & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $suspend -Root $root -GuildName $g -BackupDir $backup
    Assert-Equal 0 $LASTEXITCODE 'U3 suspend 退出码 0'
    [System.IO.File]::WriteAllBytes($guildFile, $enc.GetBytes('new-content'))
    Assert-Throws { Invoke-Resume -RootDir $root -Name $g -BackupDirPath $backup } 'U3 目标已存在被拒绝'
    Assert-Equal (Get-Sha256HexBytes ($enc.GetBytes('new-content'))) (Get-Sha256Hex $guildFile) 'U3 拒绝后目标未被覆盖'
  } finally { Remove-Item -LiteralPath $t3 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- U4 -WhatIf 零写 ---'
  $t4 = New-TempRoot 'u4'
  try {
    $root = Join-Path $t4 'M2GameSvr'
    $guildFile = New-SeedGuild $root $g ($enc.GetBytes('seed'))
    $backup = Join-Path $t4 'backup'
    & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $suspend -Root $root -GuildName $g -BackupDir $backup
    Assert-Equal 0 $LASTEXITCODE 'U4 suspend 退出码 0'
    $digestBefore = Get-TreeDigest $root
    $r = Invoke-Resume -RootDir $root -Name $g -BackupDirPath $backup -PlanOnly
    Assert-Equal 'NOT_MOVED' $r.RestoredSha 'U4 WhatIf 报 NOT_MOVED'
    Assert-True (Test-Path -LiteralPath $r.SuspendedFile) 'U4 WhatIf .suspended 仍在'
    Assert-True (-not (Test-Path -LiteralPath $guildFile)) 'U4 WhatIf 原文件仍不在'
    Assert-Equal $digestBefore (Get-TreeDigest $root) 'U4 WhatIf 前后树摘要一致'
  } finally { Remove-Item -LiteralPath $t4 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- U5 真实根护栏 ---'
  Assert-True (Test-IsProtectedPath 'E:/MirServer/M2GameSvr') 'U5 E:/MirServer 子孙判为受保护'
  Assert-Throws { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $true $false } 'U5 Selftest 下真实根被拒'
  Assert-Throws { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $false $false } 'U5 非 Selftest 未 -Force 命中真实根被拒'
  $forceOk = $false
  try { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $false $true; $forceOk = $true } catch { $forceOk = $false }
  Assert-True $forceOk 'U5 -Force 下真实根放行（显式）'

  $watchAfter = if (Test-Path -LiteralPath $watchRoot) { Get-TreeDigest $watchRoot } else { 'ABSENT' }
  Assert-Equal $watchBefore $watchAfter 'U6 E:/MirServer/M2GameSvr/GuildBase 自测前后一致（零写入）'

  if ($script:SelFail -ne 0) { throw ("SELFTEST FAILED: " + $script:SelFail + " 项失败") }
  Write-Host 'SELFTEST PASS guild_fixture_resume.ps1'
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
  Invoke-ResumeCli
  exit 0
} catch {
  Write-Host ('RED: ' + $_.Exception.Message)
  exit 2
}
