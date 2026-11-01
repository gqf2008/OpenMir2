# -*- coding: utf-8 -*-
# guild_fixture_restore.ps1 — 行会窗夹具「恢复」脚本（卡 openmir2-guild-window-fixture）
#
# 用途：把 guild_fixture.ps1 预置的行会状态恢复到预置前的原字节，用于窗口收尾。
#   * GuildList.txt：原 ABSENT ⇒ 删除文件；否则从备份拷回原字节，
#     并断言 restored_sha == 状态里的 before_sha（不等即 fail-closed，rc=2）。
#   * 删除 Guilds/<名>.txt；删除服务端装载时可能生成的 <名>.ini（仅当预置前不存在）。
#   * 删空 Guilds/ 目录（仅当预置前不存在，避免破坏既有空目录）；同理 GuildBase。
#
# 安全：与 guild_fixture.ps1 同一套真实根护栏：-Selftest 禁止真实根；
#       非 Selftest 命中 E:\MirServer 及子孙必须显式 -Force。恢复同样是写操作。
#       -WhatIf 只打印计划，不写任何文件。
#
# 用法：
#   pwsh -NoProfile -File tools/window/guild_fixture_restore.ps1 -Selftest
#   pwsh -NoProfile -File tools/window/guild_fixture_restore.ps1 `
#       -Root E:/MirServer/M2GameSvr -GuildName e2e-guild `
#       -BackupDir E:/tmp/guild-window/backup -Force
#
# 输出：
#   RESTORED_SHA <hex|ABSENT>  BEFORE_SHA <hex|ABSENT>  MATCH <true|false>
#   DELETED <path> ...         STATE_FILE <path>
# 退出码：0 = 恢复成功/自测全绿；2 = 拒绝或失败。

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
$script:StateFileName = 'guild-fixture-state.json'

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

function Read-StateFile([string]$path) {
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "找不到状态文件（需用 guild_fixture.ps1 的同一 -BackupDir）：$path"
  }
  $raw = [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8)
  return ($raw | ConvertFrom-Json)
}

function Test-PathEquals([string]$a, [string]$b) {
  if ([string]::IsNullOrEmpty($a) -or [string]::IsNullOrEmpty($b)) { return $false }
  return ([System.IO.Path]::GetFullPath($a)).TrimEnd('\', '/').Equals(
    ([System.IO.Path]::GetFullPath($b)).TrimEnd('\', '/'), [System.StringComparison]::OrdinalIgnoreCase)
}

function Invoke-Restore {
  param(
    [string]$RootDir,
    [string]$Name,
    [string]$BackupDirPath,
    [switch]$PlanOnly
  )

  if ([string]::IsNullOrWhiteSpace($BackupDirPath)) { throw '缺少 -BackupDir' }
  $stateFile = Join-Path $BackupDirPath $script:StateFileName
  $state = Read-StateFile $stateFile

  if ([string]$state.guild_name -ne $Name) {
    throw "状态文件的行会名不匹配：state=$($state.guild_name) 参数=$Name"
  }
  if (-not (Test-PathEquals ([string]$state.root) $RootDir)) {
    throw "状态文件的 Root 不匹配：state=$($state.root) 参数=$RootDir"
  }

  $guildBase = Join-Path $RootDir 'GuildBase'
  $guildsDir = Join-Path $guildBase 'Guilds'
  $listFile = Join-Path $guildBase 'GuildList.txt'
  $guildFile = Join-Path $guildsDir ($Name + '.txt')
  $iniFile = Join-Path $guildsDir ($Name + '.ini')

  $beforeAbsent = [bool]$state.before_absent
  $beforeSha = [string]$state.before_sha
  $guildBaseExisted = [bool]$state.guild_base_existed_before
  $guildsDirExisted = [bool]$state.guilds_dir_existed_before
  $iniExisted = [bool]$state.guild_ini_existed_before
  $backupFile = [string]$state.backup_file

  # 备份自校验：防止拿错的/被改过的备份恢复。
  if (-not $beforeAbsent) {
    if ([string]::IsNullOrWhiteSpace($backupFile) -or -not (Test-Path -LiteralPath $backupFile -PathType Leaf)) {
      throw "缺少备份文件：$backupFile"
    }
    $backupSha = Get-Sha256Hex $backupFile
    if ($backupSha -ne $beforeSha) {
      throw "备份字节与 before_sha 不符（拒绝恢复）：backup_sha=$backupSha before_sha=$beforeSha"
    }
  }

  $planned = New-Object System.Collections.Generic.List[string]
  if (Test-Path -LiteralPath $guildFile -PathType Leaf) { $planned.Add('DELETE ' + $guildFile) }
  if ((Test-Path -LiteralPath $iniFile -PathType Leaf) -and -not $iniExisted) { $planned.Add('DELETE ' + $iniFile) }
  if ($beforeAbsent) {
    if (Test-Path -LiteralPath $listFile -PathType Leaf) { $planned.Add('DELETE ' + $listFile) }
    $planned.Add('RESTORE ABSENT ' + $listFile)
  } else {
    $planned.Add('RESTORE_BYTES ' + $listFile + ' <- ' + $backupFile)
  }
  if (-not $guildsDirExisted) { $planned.Add('RMDIR_IF_EMPTY ' + $guildsDir) }
  if (-not $guildBaseExisted) { $planned.Add('RMDIR_IF_EMPTY ' + $guildBase) }

  if ($PlanOnly) {
    return [pscustomobject]@{
      StateFile = $stateFile
      BeforeSha = $beforeSha
      RestoredSha = 'NOT_WRITTEN'
      Match = $false
      Deleted = @()
      Planned = $planned
    }
  }

  $deleted = New-Object System.Collections.Generic.List[string]

  # ① 删本夹具创建的行会文件（<名>.txt；<名>.ini 仅在预置前不存在时删）。
  if (Test-Path -LiteralPath $guildFile -PathType Leaf) {
    Remove-Item -LiteralPath $guildFile -Force
    $deleted.Add($guildFile)
  }
  if ((Test-Path -LiteralPath $iniFile -PathType Leaf) -and -not $iniExisted) {
    Remove-Item -LiteralPath $iniFile -Force
    $deleted.Add($iniFile)
  }

  # ② 恢复 GuildList.txt 原字节 / 原 ABSENT。
  if ($beforeAbsent) {
    if (Test-Path -LiteralPath $listFile -PathType Leaf) {
      Remove-Item -LiteralPath $listFile -Force
      $deleted.Add($listFile)
    }
    $restoredSha = 'ABSENT'
  } else {
    Copy-Item -LiteralPath $backupFile -Destination $listFile -Force
    $restoredSha = Get-Sha256Hex $listFile
    if ($restoredSha -ne $beforeSha) {
      throw "restored_sha != before_sha：restored=$restoredSha before=$beforeSha"
    }
  }

  # ③ 删空目录（仅删预置前不存在的，保证往返树一致）。
  if ((-not $guildsDirExisted) -and (Test-Path -LiteralPath $guildsDir)) {
    if (@(Get-ChildItem -LiteralPath $guildsDir -Force).Count -eq 0) {
      Remove-Item -LiteralPath $guildsDir -Force
      $deleted.Add($guildsDir)
    }
  }
  if ((-not $guildBaseExisted) -and (Test-Path -LiteralPath $guildBase)) {
    if (@(Get-ChildItem -LiteralPath $guildBase -Force).Count -eq 0) {
      Remove-Item -LiteralPath $guildBase -Force
      $deleted.Add($guildBase)
    }
  }

  return [pscustomobject]@{
    StateFile = $stateFile
    BeforeSha = $beforeSha
    RestoredSha = $restoredSha
    Match = ((($beforeAbsent) -and ($restoredSha -eq 'ABSENT')) -or ((-not $beforeAbsent) -and ($restoredSha -eq $beforeSha)))
    Deleted = @($deleted)
    Planned = $planned
  }
}

function Invoke-RestoreCli {
  if ([string]::IsNullOrWhiteSpace($Root)) { throw '缺少 -Root' }
  if ([string]::IsNullOrWhiteSpace($GuildName)) { throw '缺少 -GuildName' }
  if ($GuildName -match '[\\/:*?"<>|]') { throw "GuildName 含非法文件名字符：$GuildName" }
  $rootPath = Get-NormalizedFullPath $Root
  Assert-SafeRoot $rootPath $false $Force.IsPresent
  # 状态文件在 -BackupDir 下：-WhatIf 同样必须显式给出，否则连计划都无从读取。
  if ([string]::IsNullOrWhiteSpace($BackupDir)) {
    throw '缺少 -BackupDir（-WhatIf 也必须显式给出：状态文件在 -BackupDir 下，且必须与 guild_fixture.ps1 使用同一目录）'
  }
  $backupPath = Get-NormalizedFullPath $BackupDir
  if (-not (Test-Path -LiteralPath $backupPath -PathType Container)) {
    throw "备份目录不存在：$backupPath"
  }

  $result = Invoke-Restore -RootDir $rootPath -Name $GuildName -BackupDirPath $backupPath -PlanOnly:$WhatIf.IsPresent

  Write-Host ('RESTORED_SHA ' + $result.RestoredSha)
  Write-Host ('BEFORE_SHA ' + $result.BeforeSha)
  Write-Host ('MATCH ' + ([string]$result.Match).ToLowerInvariant())
  Write-Host ('STATE_FILE ' + $result.StateFile)
  foreach ($d in $result.Deleted) { Write-Host ('DELETED ' + $d) }
  foreach ($p in $result.Planned) { Write-Host ('PLAN ' + $p) }
  if ($WhatIf) { Write-Host 'WHATIF 未写入任何文件' }
  if (-not $WhatIf -and -not $result.Match) { throw '恢复后 MATCH=false' }
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
  $base = Join-Path ([System.IO.Path]::GetTempPath()) ('guild-restore-selftest-' + $tag + '-' + [guid]::NewGuid().ToString('N'))
  [void](New-Item -ItemType Directory -Path $base -Force)
  return $base
}
function Get-HostExe { return (Get-Process -Id $PID).Path }
function Get-GbkEncoding { return [System.Text.Encoding]::GetEncoding(936) }

function Invoke-Selftest {
  $enc = Get-GbkEncoding
  $g = -join [char[]](0x6D4B, 0x8BD5, 0x884C, 0x4F1A)   # 测试行会
  $m = -join [char[]](0x6D4B, 0x8BD5, 0x89D2, 0x8272)   # 测试角色
  $fixture = Join-Path $PSScriptRoot 'guild_fixture.ps1'
  $watchRoot = Join-Path $script:ProtectedRealRoot 'M2GameSvr\GuildBase'
  $watchBefore = if (Test-Path -LiteralPath $watchRoot) { Get-TreeDigest $watchRoot } else { 'ABSENT' }

  Write-Host '--- R1 原 ABSENT：fixture 预置后 restore 归还为空 ---'
  $t1 = New-TempRoot 'r1'
  try {
    $root = Join-Path $t1 'M2GameSvr'
    [void](New-Item -ItemType Directory -Path (Join-Path $root 'GuildBase') -Force)
    $backup = Join-Path $t1 'backup'
    $treeBefore = Get-TreeSnapshot (Join-Path $root 'GuildBase')
    & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $fixture -Root $root -GuildName $g -Member $m -BackupDir $backup
    Assert-Equal 0 $LASTEXITCODE 'R1 fixture 退出码 0'
    $r = Invoke-Restore -RootDir $root -Name $g -BackupDirPath $backup
    Assert-Equal 'ABSENT' $r.RestoredSha 'R1 restored_sha=ABSENT'
    Assert-True $r.Match 'R1 MATCH=true'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $root 'GuildBase\GuildList.txt'))) 'R1 GuildList 已删'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $root 'GuildBase\Guilds'))) 'R1 Guilds 目录已删'
    Assert-Equal $treeBefore (Get-TreeSnapshot (Join-Path $root 'GuildBase')) 'R1 往返树一致'
  } finally { Remove-Item -LiteralPath $t1 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- R2 原字节（GBK + CRLF）：restore 断言 restored_sha == before_sha ---'
  $t2 = New-TempRoot 'r2'
  try {
    $root = Join-Path $t2 'M2GameSvr'
    $guildBase = Join-Path $root 'GuildBase'
    [void](New-Item -ItemType Directory -Path $guildBase -Force)
    $old = -join [char[]](0x65E7, 0x884C, 0x4F1A)   # 旧行会
    $seed = $enc.GetBytes($old + "`r`n")
    $listFile = Join-Path $guildBase 'GuildList.txt'
    [System.IO.File]::WriteAllBytes($listFile, $seed)
    $seedSha = Get-Sha256HexBytes $seed
    $treeBefore = Get-TreeSnapshot $guildBase
    $backup = Join-Path $t2 'backup'
    & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $fixture -Root $root -GuildName $g -Member $m -BackupDir $backup
    Assert-Equal 0 $LASTEXITCODE 'R2 fixture 退出码 0'
    $r = Invoke-Restore -RootDir $root -Name $g -BackupDirPath $backup
    Assert-Equal $seedSha $r.RestoredSha 'R2 restored_sha == 原 sha'
    Assert-Equal $seedSha $r.BeforeSha 'R2 before_sha == 原 sha'
    Assert-True $r.Match 'R2 MATCH=true'
    Assert-Equal $seedSha (Get-Sha256Hex $listFile) 'R2 磁盘字节 == 原字节'
    Assert-Equal $treeBefore (Get-TreeSnapshot $guildBase) 'R2 往返树一致'
  } finally { Remove-Item -LiteralPath $t2 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- R3 备份被篡改 => 拒绝恢复 ---'
  $t3 = New-TempRoot 'r3'
  try {
    $root = Join-Path $t3 'M2GameSvr'
    $guildBase = Join-Path $root 'GuildBase'
    [void](New-Item -ItemType Directory -Path $guildBase -Force)
    $listFile = Join-Path $guildBase 'GuildList.txt'
    [System.IO.File]::WriteAllBytes($listFile, $enc.GetBytes('old' + "`r`n"))
    $backup = Join-Path $t3 'backup'
    & (Get-HostExe) -NoProfile -ExecutionPolicy Bypass -File $fixture -Root $root -GuildName $g -Member $m -BackupDir $backup
    Assert-Equal 0 $LASTEXITCODE 'R3 fixture 退出码 0'
    [System.IO.File]::WriteAllBytes((Join-Path $backup 'GuildList.txt.bak'), $enc.GetBytes('tampered' + "`r`n"))
    Assert-Throws { Invoke-Restore -RootDir $root -Name $g -BackupDirPath $backup } 'R3 篡改备份被拒绝'
  } finally { Remove-Item -LiteralPath $t3 -Recurse -Force -ErrorAction SilentlyContinue }

  Write-Host '--- R4 真实根护栏 ---'
  Assert-True (Test-IsProtectedPath 'E:/MirServer/M2GameSvr') 'R4 E:/MirServer 子孙判为受保护'
  Assert-Throws { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $true $false } 'R4 Selftest 下真实根被拒'
  Assert-Throws { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $false $false } 'R4 非 Selftest 未 -Force 命中真实根被拒'
  $forceOk = $false
  try { Assert-SafeRoot 'E:/MirServer/M2GameSvr' $false $true; $forceOk = $true } catch { $forceOk = $false }
  Assert-True $forceOk 'R4 -Force 下真实根放行（显式）'

  Write-Host '--- R5 参数缺失 / 状态不匹配 => 拒绝 ---'
  $t5 = New-TempRoot 'r5'
  try {
    $root = Join-Path $t5 'M2GameSvr'
    [void](New-Item -ItemType Directory -Path (Join-Path $root 'GuildBase') -Force)
    Assert-Throws { Invoke-Restore -RootDir $root -Name $g -BackupDirPath (Join-Path $t5 'nope') } 'R5 缺状态文件被拒绝'
  } finally { Remove-Item -LiteralPath $t5 -Recurse -Force -ErrorAction SilentlyContinue }

  $watchAfter = if (Test-Path -LiteralPath $watchRoot) { Get-TreeDigest $watchRoot } else { 'ABSENT' }
  Assert-Equal $watchBefore $watchAfter 'R6 E:/MirServer/M2GameSvr/GuildBase 自测前后一致（零写入）'

  if ($script:SelFail -ne 0) { throw ("SELFTEST FAILED: " + $script:SelFail + " 项失败") }
  Write-Host 'SELFTEST PASS guild_fixture_restore.ps1'
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
  Invoke-RestoreCli
  exit 0
} catch {
  Write-Host ('RED: ' + $_.Exception.Message)
  exit 2
}
