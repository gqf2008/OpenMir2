# tools/orchestration/stamp-binaries.ps1
# 二进制构建戳生成 + 校验器（卡 openmir2-csharp-stack-binary-provenance，第 1 步）
#
# 规范（唯一权威）：
#   * docs/binary-provenance-stamp-20261115.md（戳格式、五项通电条件、六条牙）
#   * docs/loadgate-criteria-runbook-20261115.md「栈配置」「preflight 分开记 CODE_HEAD 与 REPO_HEAD」
#
# 为什么需要它：360s bot 窗口实际拉起的是 game-svr/game-gate/login-gate/sel-gate（外加 C# 侧
# DBSrv/LoginSrv），但这些二进制**没有戳**，只能靠 mtime+sha 人工登记；故障发生时无法回答
# 「这套二进制来自哪个提交」（实例：E:/MirServer/M2GameSvr/GameSrv.exe 的 PDB 指向已删除的
# worktree）。本脚本把这些二进制的来源写进 `<exe>.build.json`，并在校验模式下逐条判红。
#
# 格式：复用既有 `mir2-build-stamp/1`，**不另造**（rebuild-server.ps1:89 同格式的超集）。
#   公共字段：format/head/head_subject/built_at/dirty/exe/sha256/size
#   Rust 工具链字段：cargo/rustc
#   C# 工具链等价字段：dotnet（SDK 版本）/runtime（Microsoft.NETCore.App 版本）
#   本工具补充字段：head_source（head 的来源，见下）/mtime（与 built_at 同值，绑文件）/
#                   toolchain_kind（rust|dotnet）/toolchain_probe（ok|skipped）
#   校验器对补充字段**只做存在时的自洽检查**，不要求必须存在 —— 因此 rebuild-server.ps1
#   产出的旧戳（无 mtime/head_source/toolchain_kind）在字段层面仍可被识别。
#
# head_source 取值：
#   explicit / explicit_per_binary —— 调用方用 -Head / -Heads 显式给提交号；
#   inferred_repo_head —— 未给 head，取产戳时 -RepoRoot 的 HEAD（runbook 要求标 inferred）；
#   其它自由串（如 reflog_inferred）—— 调用方用 -HeadSource 显式声明推断方式。
#   注意：head/dirty 取的是**产戳时** -RepoRoot 的状态，不是构建时状态。真实来源必须由
#   构建车道在构建完成时立即产出；事后对构建目录产戳只能得到 inferred 级别的证据。
#
# built_at 的取值：二进制文件的 **mtime**（不是脚本运行时刻）。理由：mtime 是「这份文件
# 何时被写成现在这个样子」的可观测事实；校验器拿戳内 built_at 与文件实时 mtime 比对，
# 用来抓「产戳后被覆盖」（先构建后覆盖）以及手改戳。若用脚本运行时刻，对一个陈旧目录
# 产戳会立即 >5 分钟并判红，且无法区分「旧构建」与「产戳后被换」。
#
# 安全属性：
#   * sha256 由本脚本自己从文件算（.NET SHA256，不依赖 Get-FileHash）；
#   * head 写戳前必须 `git cat-file -e <head>^{commit}` 解析成功（fail-closed，不写假戳）；
#   * 已存在的戳默认**拒绝覆盖**（-Force 才覆盖），防止把更强的来源证据冲掉；
#   * 写戳走 临时文件 + Move + 回读核对，任一步失败 rc=2。
#
# 用法：
#   # 产戳（构建车道在构建完成时跑；BinDir 应是构建输出目录，不是源码目录）
#   powershell -NoProfile -File tools/orchestration/stamp-binaries.ps1 `
#     -BinDir E:/UserTemp/stack-rel-target-w83/release -Head <sha>
#   # 逐二进制指定 head（同目录里各 exe 构建于不同提交时）
#   ... -BinDir <dir> -Heads game-svr=<sha>,game-gate=<sha> -HeadSource reflog_inferred
#   # 校验（第 2 步起栈门禁的判据；现在可独立跑）
#   ... -Check -BinDir <dir> -Names game-svr,game-gate,login-gate,sel-gate -RequireAll
#   # 自测（离线合成夹具；工具门登记项）
#   ... -Selftest
#
# 退出码（与工具门三态一致）：
#   0 = 产戳成功 / 校验全绿
#   1 = 校验判红（戳缺字段、sha 不符、head 解析不到、built_at 与 mtime 差 >5 分钟…），
#       或 -Selftest 有 FAIL
#   2 = 判不了 / 用法错（BinDir 不存在、目录里没有二进制、RepoRoot 不是 git 仓、写戳失败、
#       自测基础设施不可用）——绝不静默通过
#
# 机器可读输出（stdout）：
#   产戳：STAMP_WRITE <stamp> sha256=<hex> size=<n> head=<sha> kind=<rust|dotnet> head_source=<..>
#         SKIP <name> (absent) / SKIP_STAMP_EXISTS <stamp> (use -Force to overwrite) / STAMPED <n>
#   校验：STAMP_OK <exe> / RED <exe> <CODE> [detail] / SKIP <name> (absent) / GREEN <n> stamps verified
#   自测：stamp-binaries selftest: PASS|FAIL <case> [detail] / GREEN|RED (…)

param(
  [string]$BinDir = '',
  [string]$RepoRoot = '',
  [string[]]$Names = @(),
  [string]$Head = '',
  [string[]]$Heads = @(),
  [string]$HeadSource = '',
  [switch]$Check,
  [switch]$RequireAll,
  [switch]$Force,
  [switch]$NoToolchainProbe,
  [switch]$Selftest
)

$ErrorActionPreference = 'Stop'

# 全局兜底：任何未捕获异常都必须 rc=2 + `RED:`，不得以 0 退出留下半成品戳。
trap {
  [Console]::Out.WriteLine("RED: uncaught exception: " + $_.Exception.Message)
  exit 2
}

$script:StampFormat = 'mir2-build-stamp/1'
$script:BaseFields = @('format', 'head', 'head_subject', 'built_at', 'dirty', 'exe', 'sha256', 'size')
$script:DefaultNames = @('game-svr', 'game-gate', 'login-gate', 'sel-gate',
                         'DBSrv', 'LoginSrv', 'GameSrv', 'BotSrv', 'mir2-server')
# 已知 C# 服务（name→dotnet）。名字按无扩展名、小写比较；game-gate(Rust) 与 GameGate(C#) 不冲突。
$script:DotnetNames = @('dbsrv', 'loginsrv', 'gamesrv', 'botsrv', 'chatsrv',
                        'logdatasrv', 'gamegate', 'logingate')
$script:ToleranceMinutes = 5
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Emit([string]$text) { [Console]::Out.WriteLine($text) }

function Fail([string]$msg) {
  [Console]::Out.WriteLine("RED: " + $msg)
  exit 2
}

# SHA256 用 .NET 直接算：Windows PowerShell 5.1 在 PSModulePath 缺 Microsoft.PowerShell.Utility
# 时 `Get-FileHash` 不存在（2026-10-27 现场）。任何异常上抛，由调用点转 Fail。
function Get-Sha256Hex([string]$path) {
  $sha = [System.Security.Cryptography.SHA256]::Create()
  $stream = $null
  try {
    $stream = [System.IO.File]::Open(
      $path,
      [System.IO.FileMode]::Open,
      [System.IO.FileAccess]::Read,
      [System.IO.FileShare]::Read
    )
    $bytes = $sha.ComputeHash($stream)
  } finally {
    if ($null -ne $stream) { $stream.Dispose() }
    $sha.Dispose()
  }
  if ($null -eq $bytes -or $bytes.Length -ne 32) {
    throw ("ComputeHash returned " + $(if ($null -eq $bytes) { 'null' } else { $bytes.Length.ToString() }) + " bytes (expected 32)")
  }
  return (($bytes | ForEach-Object { $_.ToString('x2') }) -join '')
}

# ---- git 辅助 -------------------------------------------------------------
# 注意：Windows PowerShell 5.1 在 $ErrorActionPreference='Stop' 下会把**原生命令的 stderr**
# 当成终止错误（git 在 rc≠0 时会写 `fatal: …`）⇒ 必须在调用点临时放宽，否则
# 「head 不存在」这种正常要判红的路径会变成脚本崩溃。PS 7 不受影响，但统一写法。
function Invoke-Git([string]$Repo, [string[]]$GitArgs) {
  $prev = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    $out = @(& git -C $Repo @GitArgs 2>&1)
    $rc = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $prev
  }
  return @{ Rc = [int]$rc; Out = $out }
}

# 自测专用：清掉调用方可能导出的 GIT_*（GIT_DIR 污染会把临时仓库的 init/commit 打到真实仓库，
# 见卡 openmir2 的 GIT_DIR 事故记录）。只在 -Selftest 的临时仓库操作里用。
function Invoke-GitSanitized([string]$Repo, [string[]]$GitArgs) {
  $keys = @('GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_OBJECT_DIRECTORY',
            'GIT_COMMON_DIR', 'GIT_NAMESPACE', 'GIT_CEILING_DIRECTORIES',
            'GIT_ALTERNATE_OBJECT_DIRECTORIES', 'GIT_CONFIG', 'GIT_CONFIG_GLOBAL',
            'GIT_CONFIG_SYSTEM')
  $saved = @{}
  foreach ($k in $keys) {
    $v = [Environment]::GetEnvironmentVariable($k)
    if ($null -ne $v) {
      $saved[$k] = $v
      Remove-Item ("Env:" + $k) -ErrorAction SilentlyContinue
    }
  }
  $prev = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    $out = @(& git -C $Repo @GitArgs 2>&1)
    $rc = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $prev
    foreach ($k in $saved.Keys) { Set-Item ("Env:" + $k) $saved[$k] }
  }
  return @{ Rc = [int]$rc; Out = $out }
}

function Test-CommitExists([string]$Repo, [string]$Sha) {
  if ($Sha -notmatch '^[0-9a-f]{7,40}$') { return $false }
  $r = Invoke-Git $Repo @('cat-file', '-e', ($Sha + '^{commit}'))
  return ($r.Rc -eq 0)
}

function Get-GitCommitSubject([string]$Repo, [string]$Sha) {
  $r = Invoke-Git $Repo @('show', '-s', '--format=%s', $Sha)
  if ($r.Rc -ne 0) { return $null }
  return (($r.Out | ForEach-Object { "$_" }) -join "`n").Trim()
}

function Get-GitDirtyText([string]$Repo) {
  $r = Invoke-Git $Repo @('status', '--porcelain')
  if ($r.Rc -ne 0) { Fail ("git status --porcelain failed in " + $Repo) }
  $lines = @($r.Out | ForEach-Object { "$_" } | Where-Object { $_ -and $_.Trim() })
  return (($lines -join "`n"))
}

# ---- 名字 / 工具链 --------------------------------------------------------
function Find-Exe([string]$Dir, [string]$Name) {
  $base = $Name
  if ($base.ToLowerInvariant().EndsWith('.exe')) { $base = $base.Substring(0, $base.Length - 4) }
  $direct = Join-Path $Dir ($base + '.exe')
  if (Test-Path -LiteralPath $direct -PathType Leaf) {
    return (Resolve-Path -LiteralPath $direct).Path
  }
  $target = ($base + '.exe').ToLowerInvariant()
  $hit = Get-ChildItem -LiteralPath $Dir -File -ErrorAction SilentlyContinue |
         Where-Object { $_.Name.ToLowerInvariant() -eq $target } |
         Select-Object -First 1
  if ($hit) { return $hit.FullName }
  return $null
}

function Get-ToolchainKind([string]$ExePath) {
  $dir = Split-Path -Parent $ExePath
  $base = [System.IO.Path]::GetFileNameWithoutExtension($ExePath)
  if ($script:DotnetNames -contains $base.ToLowerInvariant()) { return 'dotnet' }
  if (Test-Path -LiteralPath (Join-Path $dir ($base + '.d')) -PathType Leaf) { return 'rust' }
  if (Test-Path -LiteralPath (Join-Path $dir ($base + '.deps.json')) -PathType Leaf) { return 'dotnet' }
  Emit ("WARN toolchain-unknown " + $ExePath + " => assume rust")
  return 'rust'
}

$script:ToolchainCache = @{}

# C# 的 runtime 字段 = **二进制自己的目标框架版本**（构建产物里的 <base>.runtimeconfig.json 的
# runtimeOptions.framework.version，如 8.0.0），不是「本机安装的第一个 runtime」。
# 实例（2026-11-15）：本机 --list-runtimes 第一条是 6.0.36，但 DBSrv 二进制目标为 8.0.0；
# 只取第一条会把工具链版本写成与二进制无关的值。文件缺失时回落到 --list-runtimes。
function Get-DotnetTargetRuntime([string]$ExePath) {
  $dir = Split-Path -Parent $ExePath
  $base = [System.IO.Path]::GetFileNameWithoutExtension($ExePath)
  $cfg = Join-Path $dir ($base + '.runtimeconfig.json')
  if (Test-Path -LiteralPath $cfg -PathType Leaf) {
    try {
      $j = [System.IO.File]::ReadAllText($cfg, $utf8) | ConvertFrom-Json
      if ($null -ne $j.runtimeOptions -and $null -ne $j.runtimeOptions.framework -and
          $null -ne $j.runtimeOptions.framework.version) {
        $v = [string]$j.runtimeOptions.framework.version
        if ($v -ne '') { return $v }
      }
    } catch { }
  }
  return ''
}

function Get-ToolchainVersionFields([string]$Kind, [string]$ExePath) {
  $fields = [ordered]@{}
  if ($Kind -eq 'rust') {
    if (-not $script:ToolchainCache.ContainsKey('rust')) {
      $cargo = ''
      $rustc = ''
      if (-not $NoToolchainProbe) {
        try { $cargo = ((@(& cargo --version 2>&1) | ForEach-Object { "$_" }) -join ' ').Trim() } catch { $cargo = '' }
        try { $rustc = ((@(& rustc --version 2>&1) | ForEach-Object { "$_" }) -join ' ').Trim() } catch { $rustc = '' }
      }
      $script:ToolchainCache['rust'] = @{ cargo = $cargo; rustc = $rustc }
    }
    $fields['cargo'] = $script:ToolchainCache['rust'].cargo
    $fields['rustc'] = $script:ToolchainCache['rust'].rustc
  } else {
    if (-not $script:ToolchainCache.ContainsKey('dotnet')) {
      $dotnet = ''
      $fallbackRuntime = ''
      if (-not $NoToolchainProbe) {
        try { $dotnet = ((@(& dotnet --version 2>&1) | ForEach-Object { "$_" }) -join ' ').Trim() } catch { $dotnet = '' }
        try {
          $runtimes = @(& dotnet --list-runtimes 2>&1)
          foreach ($line in $runtimes) {
            if ("$line" -match '^Microsoft\.NETCore\.App\s+([0-9][^\s]*)') { $fallbackRuntime = $Matches[1]; break }
          }
        } catch { $fallbackRuntime = '' }
      }
      $script:ToolchainCache['dotnet'] = @{ dotnet = $dotnet; fallbackRuntime = $fallbackRuntime }
    }
    $fields['dotnet'] = $script:ToolchainCache['dotnet'].dotnet
    $target = Get-DotnetTargetRuntime $ExePath
    if ($target -eq '') { $target = $script:ToolchainCache['dotnet'].fallbackRuntime }
    $fields['runtime'] = $target
  }
  if ($NoToolchainProbe) { $fields['toolchain_probe'] = 'skipped' } else { $fields['toolchain_probe'] = 'ok' }
  return $fields
}

# ---- 写戳 ----------------------------------------------------------------
# 返回 $true=已写 / $false=已有戳且未 -Force（跳过）。
function Write-StampFile {
  param(
    [string]$ExePath,
    [string]$Name,
    [string]$HeadSha,
    [string]$HeadSrc,
    [string]$DirtyText,
    [bool]$DoForce
  )
  $item = Get-Item -LiteralPath $ExePath -ErrorAction Stop
  $size = [long]$item.Length
  if ($size -le 0) { Fail ("binary size is 0 (not a build artifact): " + $ExePath) }
  try { $sha = Get-Sha256Hex $ExePath } catch { Fail ("SHA256 failed: " + $ExePath + " :: " + $_.Exception.Message) }
  if ($sha -notmatch '^[0-9a-f]{64}$') { Fail ("SHA256 is not 64 lowercase hex: " + $ExePath) }
  $builtAt = $item.LastWriteTime.ToString('o')
  $subject = Get-GitCommitSubject $RepoRoot $HeadSha
  if ($null -eq $subject -or $subject -eq '') {
    Fail ("cannot read head subject for " + $HeadSha + " (RepoRoot=" + $RepoRoot + ")")
  }
  $kind = Get-ToolchainKind $ExePath
  $versions = Get-ToolchainVersionFields $kind $ExePath

  $doc = [ordered]@{
    format         = $script:StampFormat
    head           = $HeadSha
    head_source    = $HeadSrc
    head_subject   = $subject
    built_at       = $builtAt
    mtime          = $builtAt
    dirty          = $DirtyText
    exe            = ([System.IO.Path]::GetFullPath($ExePath).Replace('\', '/'))
    sha256         = $sha
    size           = $size
    toolchain_kind = $kind
  }
  foreach ($k in $versions.Keys) { $doc[$k] = $versions[$k] }

  $stampPath = $ExePath + '.build.json'
  if ((Test-Path -LiteralPath $stampPath -PathType Leaf) -and (-not $DoForce)) {
    Emit ("SKIP_STAMP_EXISTS " + $stampPath + " (use -Force to overwrite)")
    return $false
  }
  $tmp = $stampPath + '.tmp-' + $PID
  try {
    $json = $doc | ConvertTo-Json -Depth 5
    if ([string]::IsNullOrWhiteSpace($json)) { throw 'ConvertTo-Json produced empty output' }
    [System.IO.File]::WriteAllText($tmp, $json + "`n", $utf8)
    Move-Item -LiteralPath $tmp -Destination $stampPath -Force
    # 写后回读：任一处不一致都不得 exit 0（旧失效模式：戳留旧值却被当成功）。
    $check = [System.IO.File]::ReadAllText($stampPath, $utf8) | ConvertFrom-Json
    if ([string]$check.format -ne $script:StampFormat) { throw 'format readback mismatch' }
    if ([string]$check.head -ne $HeadSha) { throw 'head readback mismatch' }
    if ([string]$check.sha256 -ne $sha) { throw 'sha256 readback mismatch' }
    if ([long]$check.size -ne $size) { throw 'size readback mismatch' }
  } catch {
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue }
    Fail ("write stamp failed: " + $stampPath + " :: " + $_.Exception.Message)
  }
  Emit ("STAMP_WRITE " + $stampPath + " sha256=" + $sha + " size=" + $size +
        " head=" + $HeadSha + " kind=" + $kind + " head_source=" + $HeadSrc)
  return $true
}

# ---- 校验 ----------------------------------------------------------------
function Test-HasField($Obj, [string]$Name) {
  if ($null -eq $Obj) { return $false }
  return (@($Obj.PSObject.Properties.Name) -contains $Name)
}

function Get-FieldText($Obj, [string]$Name) {
  if (-not (Test-HasField $Obj $Name)) { return $null }
  $v = $Obj.$Name
  if ($null -eq $v) { return '' }
  return ([string]$v)
}

function Parse-IsoInstant([string]$Text) {
  return [DateTimeOffset]::Parse($Text, [System.Globalization.CultureInfo]::InvariantCulture)
}

# 返回错误的 List[string]（空 List = 该戳健康）。
function Test-OneStamp([string]$ExePath, [string]$StampPath, [string]$Repo) {
  $errors = New-Object System.Collections.Generic.List[string]
  if (-not (Test-Path -LiteralPath $StampPath -PathType Leaf)) {
    $errors.Add('MISSING_STAMP')
    return ,$errors
  }
  $raw = ''
  try { $raw = [System.IO.File]::ReadAllText($StampPath, $utf8) } catch { $raw = '' }
  $obj = $null
  try { $obj = $raw | ConvertFrom-Json } catch { $obj = $null }
  if ($null -eq $obj) {
    $errors.Add('BAD_JSON')
    return ,$errors
  }

  foreach ($field in $script:BaseFields) {
    if (-not (Test-HasField $obj $field)) { $errors.Add('MISSING_FIELD ' + $field) }
  }

  $format = Get-FieldText $obj 'format'
  if ($null -ne $format -and $format -ne $script:StampFormat) { $errors.Add('BAD_FORMAT') }

  $head = Get-FieldText $obj 'head'
  if ($null -ne $head) {
    if ($head -notmatch '^[0-9a-f]{7,40}$') {
      $errors.Add('BAD_HEAD_FORMAT')
    } elseif (-not (Test-CommitExists $Repo $head)) {
      $errors.Add('HEAD_NOT_FOUND')
    }
  }

  $subject = Get-FieldText $obj 'head_subject'
  if ($null -ne $subject -and $subject -eq '') { $errors.Add('EMPTY_HEAD_SUBJECT') }

  if (Test-HasField $obj 'head_source') {
    $src = Get-FieldText $obj 'head_source'
    if ($src -eq '') { $errors.Add('EMPTY_HEAD_SOURCE') }
  }

  $exeField = Get-FieldText $obj 'exe'
  if ($null -ne $exeField) {
    $wantName = [System.IO.Path]::GetFileName($ExePath).ToLowerInvariant()
    $gotName = ''
    try { $gotName = [System.IO.Path]::GetFileName($exeField.Replace('/', '\')).ToLowerInvariant() } catch { $gotName = '' }
    if ($gotName -ne $wantName) { $errors.Add('EXE_MISMATCH') }
  }

  $shaField = Get-FieldText $obj 'sha256'
  if ($null -ne $shaField) {
    if ($shaField -notmatch '^[0-9a-f]{64}$') {
      $errors.Add('BAD_SHA256_FORMAT')
    } else {
      $actualSha = ''
      try { $actualSha = Get-Sha256Hex $ExePath } catch { $errors.Add('SHA256_UNREADABLE') }
      if ($actualSha -ne '' -and $actualSha -ne $shaField) { $errors.Add('SHA256_MISMATCH') }
    }
  }

  $item = Get-Item -LiteralPath $ExePath -ErrorAction Stop
  if (Test-HasField $obj 'size') {
    $stampSize = -1
    try { $stampSize = [long]$obj.size } catch { $errors.Add('BAD_SIZE') }
    if ($stampSize -ge 0 -and $stampSize -ne [long]$item.Length) { $errors.Add('SIZE_MISMATCH') }
  }

  $fileInstant = [DateTimeOffset]$item.LastWriteTime
  $builtText = Get-FieldText $obj 'built_at'
  if ($null -ne $builtText -and $builtText -ne '') {
    $built = $null
    try { $built = Parse-IsoInstant $builtText } catch { $errors.Add('BAD_BUILT_AT') }
    if ($null -ne $built) {
      $delta = [Math]::Abs(($built.ToUniversalTime() - $fileInstant.ToUniversalTime()).TotalMinutes)
      if ($delta -gt $script:ToleranceMinutes) { $errors.Add('BUILT_AT_SKEW') }
    }
  }

  # mtime 是本工具的补充字段：存在就必须与文件 mtime 自洽（同一 5 分钟容差）。
  if (Test-HasField $obj 'mtime') {
    $mtimeText = Get-FieldText $obj 'mtime'
    if ($mtimeText -ne '') {
      $mto = $null
      try { $mto = Parse-IsoInstant $mtimeText } catch { $errors.Add('BAD_MTIME') }
      if ($null -ne $mto) {
        $delta2 = [Math]::Abs(($mto.ToUniversalTime() - $fileInstant.ToUniversalTime()).TotalMinutes)
        if ($delta2 -gt $script:ToleranceMinutes) { $errors.Add('MTIME_SKEW') }
      }
    }
  }

  $kind = ''
  if (Test-HasField $obj 'toolchain_kind') {
    $kind = (Get-FieldText $obj 'toolchain_kind').ToLowerInvariant()
    if ($kind -ne 'rust' -and $kind -ne 'dotnet') { $errors.Add('BAD_TOOLCHAIN_KIND') }
  }
  if ($kind -eq 'rust') {
    foreach ($f in @('cargo', 'rustc')) {
      if (-not (Test-HasField $obj $f)) { $errors.Add('MISSING_FIELD ' + $f) }
    }
  } elseif ($kind -eq 'dotnet') {
    foreach ($f in @('dotnet', 'runtime')) {
      if (-not (Test-HasField $obj $f)) { $errors.Add('MISSING_FIELD ' + $f) }
    }
  } elseif ($kind -eq '') {
    # 旧戳没有 toolchain_kind：按字段存在性推断，必须能认出至少一种工具链。
    $hasRust = (Test-HasField $obj 'cargo') -or (Test-HasField $obj 'rustc')
    $hasDotnet = (Test-HasField $obj 'dotnet') -or (Test-HasField $obj 'runtime')
    if ($hasRust) {
      foreach ($f in @('cargo', 'rustc')) {
        if (-not (Test-HasField $obj $f)) { $errors.Add('MISSING_FIELD ' + $f) }
      }
    } elseif ($hasDotnet) {
      foreach ($f in @('dotnet', 'runtime')) {
        if (-not (Test-HasField $obj $f)) { $errors.Add('MISSING_FIELD ' + $f) }
      }
    } else {
      $errors.Add('MISSING_TOOLCHAIN_FIELDS')
    }
  }

  return ,$errors
}

# ---- 路径 / 参数解析 ------------------------------------------------------
$ScriptPath = $PSCommandPath
if ([string]::IsNullOrWhiteSpace($ScriptPath)) { $ScriptPath = $MyInvocation.MyCommand.Path }
$Here = Split-Path -Parent $ScriptPath
if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
  $RepoRoot = (Resolve-Path (Join-Path $Here '..\..')).Path
}
try { $RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path } catch {
  Fail ("RepoRoot not found: " + $RepoRoot)
}

# 名字集合（保持顺序、大小写不敏感去重）。
# 注意：`powershell -File script.ps1 -Names a b c` 的 `-File` 绑定**不会**让数组参数吃掉
# 后续 token（b/c 会落到下一个参数）⇒ 一律接受逗号分隔形式：`-Names a,b,c`。
$wantedList = New-Object System.Collections.Generic.List[string]
if ($Names -and @($Names).Count -gt 0) {
  foreach ($chunk in @($Names)) {
    foreach ($n in ([string]$chunk).Split(',')) {
      if ($n -and $n.Trim()) { $wantedList.Add($n.Trim()) }
    }
  }
} else {
  foreach ($n in $script:DefaultNames) { $wantedList.Add($n) }
}
$seenNames = @{}
$wanted = New-Object System.Collections.Generic.List[string]
foreach ($n in $wantedList) {
  $key = $n.ToLowerInvariant()
  if (-not $seenNames.ContainsKey($key)) {
    $seenNames[$key] = $true
    $wanted.Add($n)
  }
}
if ($wanted.Count -eq 0) { Fail "no binary names to process (-Names is empty)" }

# git 必须在 PATH 上：产戳要解析 head、校验要 cat-file。缺 git ⇒ 判不了（rc=2），
# 绝不把「无法验证来源」当成健康。
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
  Fail "git not found on PATH (required to resolve/verify head)"
}

# ---- 自测（离线合成夹具；不依赖真实仓库状态）-----------------------------
if ($Selftest) {
  $script:SelfFails = New-Object System.Collections.Generic.List[string]
  $script:SelfCases = 0
  $script:SelfInfra = ''

  function SAssert([bool]$cond, [string]$name, [string]$detail) {
    if ($cond) {
      Emit ("stamp-binaries selftest: PASS " + $name)
    } else {
      Emit ("stamp-binaries selftest: FAIL " + $name + " :: " + $detail)
      $script:SelfFails.Add($name)
    }
  }

  $psExe = ''
  try { $psExe = (Get-Process -Id $PID).Path } catch { $psExe = '' }
  if ([string]::IsNullOrWhiteSpace($psExe)) { $psExe = 'powershell' }

  function Invoke-Child([string[]]$ChildArgs) {
    $out = @(& $psExe -NoProfile -ExecutionPolicy Bypass -File $ScriptPath @ChildArgs 2>&1)
    $rc = $LASTEXITCODE
    return @{ Rc = [int]$rc; Text = (($out | ForEach-Object { "$_" }) -join "`n") }
  }

  function New-Fixture([string]$Root, [string]$Sub, [string[]]$ExeNames) {
    $dir = Join-Path $Root $Sub
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $i = 1
    foreach ($n in $ExeNames) {
      $p = Join-Path $dir ($n + '.exe')
      $bytes = New-Object byte[] (1024 * (2 + $i))
      for ($b = 0; $b -lt $bytes.Length; $b++) {
        $bytes[$b] = [byte](($b * 31 + $i * 7) % 251)
      }
      [System.IO.File]::WriteAllBytes($p, $bytes)
      if ($n -eq 'DBSrv') {
        # 合成 runtimeconfig：让 C# 等价字段 runtime 的取值**不依赖本机装了什么运行时**。
        $cfg = '{"runtimeOptions":{"tfm":"net8.0","framework":{"name":"Microsoft.NETCore.App","version":"8.0.0"}}}'
        [System.IO.File]::WriteAllText((Join-Path $dir ($n + '.runtimeconfig.json')), $cfg, $utf8)
      }
      $i++
    }
    return $dir
  }

  function Read-StampJson([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
    try { return ([System.IO.File]::ReadAllText($Path, $utf8) | ConvertFrom-Json) } catch { return $null }
  }

  function Write-StampJson($Obj, [string]$Path) {
    [System.IO.File]::WriteAllText($Path, (($Obj | ConvertTo-Json -Depth 5) + "`n"), $utf8)
  }

  function Write-FixtureStamps([string]$Dir, [string]$Repo, [string]$Sha, [string[]]$ExeNames) {
    $childArgs = @('-BinDir', $Dir, '-RepoRoot', $Repo, '-Head', $Sha, '-NoToolchainProbe',
                   '-Names', ($ExeNames -join ','))
    return (Invoke-Child $childArgs)
  }

  function Invoke-SelfCases([string]$Tmp, [string]$Repo, [string]$HeadSha, [string[]]$ExeNames) {
    # Case 1：正例（产戳 + 校验全绿；C# 等价字段存在；sha 与独立计算一致）
    $script:SelfCases++
    $bin = New-Fixture $Tmp 'pos' $ExeNames
    $w = Write-FixtureStamps $bin $Repo $HeadSha $ExeNames
    SAssert ($w.Rc -eq 0) 'positive: write stamps rc=0' ("rc=" + $w.Rc + " out=" + $w.Text)
    $stampCount = @(Get-ChildItem -LiteralPath $bin -Filter '*.build.json' -File -ErrorAction SilentlyContinue).Count
    SAssert ($stampCount -eq $ExeNames.Count) 'positive: one stamp per binary' ("stamps=" + $stampCount + " expected=" + $ExeNames.Count)

    $rustStamp = Read-StampJson (Join-Path $bin 'game-svr.exe.build.json')
    $indepSha = ''
    $rustExe = Join-Path $bin 'game-svr.exe'
    if (Test-Path -LiteralPath $rustExe) { $indepSha = Get-Sha256Hex $rustExe }
    SAssert (($null -ne $rustStamp) -and ((Get-FieldText $rustStamp 'sha256') -eq $indepSha)) `
      'positive: stamp sha256 equals independent file hash' `
      ("stamp=" + (Get-FieldText $rustStamp 'sha256') + " indep=" + $indepSha)
    SAssert (($null -ne $rustStamp) -and (Test-HasField $rustStamp 'cargo') -and (Test-HasField $rustStamp 'rustc') -and (Test-HasField $rustStamp 'format')) `
      'positive: Rust stamp carries cargo/rustc/format' `
      ("fields=" + (($rustStamp.PSObject.Properties.Name) -join ','))
    $dotnetStamp = Read-StampJson (Join-Path $bin 'DBSrv.exe.build.json')
    SAssert (($null -ne $dotnetStamp) -and (Test-HasField $dotnetStamp 'dotnet') -and (Test-HasField $dotnetStamp 'runtime') -and ((Get-FieldText $dotnetStamp 'toolchain_kind') -eq 'dotnet')) `
      'positive: C# stamp carries dotnet/runtime and kind=dotnet' `
      ("kind=" + (Get-FieldText $dotnetStamp 'toolchain_kind') + " fields=" + (($dotnetStamp.PSObject.Properties.Name) -join ','))
    SAssert (($null -ne $dotnetStamp) -and ((Get-FieldText $dotnetStamp 'runtime') -eq '8.0.0')) `
      'positive: C# runtime comes from binary runtimeconfig target' `
      ("runtime=" + (Get-FieldText $dotnetStamp 'runtime'))
    $c = Invoke-Child @('-Check', '-BinDir', $bin, '-RepoRoot', $Repo, '-RequireAll',
                        '-Names', ($ExeNames -join ','))
    SAssert ($c.Rc -eq 0) 'positive: check healthy dir rc=0' ("rc=" + $c.Rc + " out=" + $c.Text)
    SAssert ($c.Text.IndexOf(('GREEN ' + $ExeNames.Count), [StringComparison]::Ordinal) -ge 0) `
      'positive: check reports GREEN <n>' ("out=" + $c.Text)

    # Case 2：缺字段（删 sha256 ⇒ MISSING_FIELD sha256，rc=1）
    $script:SelfCases++
    $bin2 = New-Fixture $Tmp 'missing' @('game-svr')
    $w2 = Write-FixtureStamps $bin2 $Repo $HeadSha @('game-svr')
    SAssert ($w2.Rc -eq 0) 'negative-setup: missing-field fixture written' ("rc=" + $w2.Rc + " out=" + $w2.Text)
    $p2 = Join-Path $bin2 'game-svr.exe.build.json'
    $o2 = Read-StampJson $p2
    if ($null -eq $o2) { $script:SelfInfra = 'cannot parse fixture stamp for missing-field case'; return }
    $o2.PSObject.Properties.Remove('sha256')
    Write-StampJson $o2 $p2
    $c2 = Invoke-Child @('-Check', '-BinDir', $bin2, '-RepoRoot', $Repo, '-Names', 'game-svr')
    SAssert ($c2.Rc -eq 1) 'negative: missing field rc=1' ("rc=" + $c2.Rc + " out=" + $c2.Text)
    SAssert ($c2.Text.IndexOf('MISSING_FIELD sha256', [StringComparison]::Ordinal) -ge 0) `
      'negative: missing field names sha256' ("out=" + $c2.Text)

    # Case 3：篡改戳内 sha256 ⇒ SHA256_MISMATCH
    $script:SelfCases++
    $bin3 = New-Fixture $Tmp 'sha' @('game-svr')
    $w3 = Write-FixtureStamps $bin3 $Repo $HeadSha @('game-svr')
    SAssert ($w3.Rc -eq 0) 'negative-setup: sha-mismatch fixture written' ("rc=" + $w3.Rc)
    $p3 = Join-Path $bin3 'game-svr.exe.build.json'
    $o3 = Read-StampJson $p3
    if ($null -eq $o3) { $script:SelfInfra = 'cannot parse fixture stamp for sha-mismatch case'; return }
    $o3.sha256 = ('0' * 64)
    Write-StampJson $o3 $p3
    $c3 = Invoke-Child @('-Check', '-BinDir', $bin3, '-RepoRoot', $Repo, '-Names', 'game-svr')
    SAssert ($c3.Rc -eq 1) 'negative: tampered sha256 rc=1' ("rc=" + $c3.Rc + " out=" + $c3.Text)
    SAssert ($c3.Text.IndexOf('SHA256_MISMATCH', [StringComparison]::Ordinal) -ge 0) `
      'negative: tampered sha256 reports SHA256_MISMATCH' ("out=" + $c3.Text)

    # Case 4：head 改成不存在的提交 ⇒ HEAD_NOT_FOUND（cat-file -e 失败）
    $script:SelfCases++
    $bin4 = New-Fixture $Tmp 'head' @('game-svr')
    $w4 = Write-FixtureStamps $bin4 $Repo $HeadSha @('game-svr')
    SAssert ($w4.Rc -eq 0) 'negative-setup: head-missing fixture written' ("rc=" + $w4.Rc)
    $p4 = Join-Path $bin4 'game-svr.exe.build.json'
    $o4 = Read-StampJson $p4
    if ($null -eq $o4) { $script:SelfInfra = 'cannot parse fixture stamp for head case'; return }
    $o4.head = ('0' * 40)
    Write-StampJson $o4 $p4
    $c4 = Invoke-Child @('-Check', '-BinDir', $bin4, '-RepoRoot', $Repo, '-Names', 'game-svr')
    SAssert ($c4.Rc -eq 1) 'negative: nonexistent head rc=1' ("rc=" + $c4.Rc + " out=" + $c4.Text)
    SAssert ($c4.Text.IndexOf('HEAD_NOT_FOUND', [StringComparison]::Ordinal) -ge 0) `
      'negative: nonexistent head reports HEAD_NOT_FOUND' ("out=" + $c4.Text)

    # Case 5：built_at 与 mtime 差 10 分钟 ⇒ BUILT_AT_SKEW
    $script:SelfCases++
    $bin5 = New-Fixture $Tmp 'skew' @('game-svr')
    $w5 = Write-FixtureStamps $bin5 $Repo $HeadSha @('game-svr')
    SAssert ($w5.Rc -eq 0) 'negative-setup: skew fixture written' ("rc=" + $w5.Rc)
    $p5 = Join-Path $bin5 'game-svr.exe.build.json'
    $o5 = Read-StampJson $p5
    if ($null -eq $o5) { $script:SelfInfra = 'cannot parse fixture stamp for skew case'; return }
    $exe5 = Join-Path $bin5 'game-svr.exe'
    $mtime5 = (Get-Item -LiteralPath $exe5).LastWriteTime
    $skew = ([DateTimeOffset]$mtime5).AddMinutes(10)
    $o5.built_at = $skew.ToString('o')
    Write-StampJson $o5 $p5
    $c5 = Invoke-Child @('-Check', '-BinDir', $bin5, '-RepoRoot', $Repo, '-Names', 'game-svr')
    SAssert ($c5.Rc -eq 1) 'negative: built_at/mtime skew rc=1' ("rc=" + $c5.Rc + " out=" + $c5.Text)
    SAssert ($c5.Text.IndexOf('BUILT_AT_SKEW', [StringComparison]::Ordinal) -ge 0) `
      'negative: built_at/mtime skew reports BUILT_AT_SKEW' ("out=" + $c5.Text)

    # Case 6：空目录 / 无二进制 ⇒ 明确 rc=2 + NO_BINARIES（不得静默通过）
    $script:SelfCases++
    $empty = Join-Path $Tmp 'empty'
    New-Item -ItemType Directory -Force -Path $empty | Out-Null
    $c6 = Invoke-Child @('-Check', '-BinDir', $empty, '-RepoRoot', $Repo)
    SAssert ($c6.Rc -eq 2) 'negative: empty dir rc=2' ("rc=" + $c6.Rc + " out=" + $c6.Text)
    SAssert ($c6.Text.IndexOf('NO_BINARIES', [StringComparison]::Ordinal) -ge 0) `
      'negative: empty dir reports NO_BINARIES' ("out=" + $c6.Text)

    # Case 7：产戳后二进制被覆盖 ⇒ SHA256_MISMATCH（证明 sha 是现算的，不是抄报告）
    $script:SelfCases++
    $bin7 = New-Fixture $Tmp 'overwrite' @('game-svr')
    $w7 = Write-FixtureStamps $bin7 $Repo $HeadSha @('game-svr')
    SAssert ($w7.Rc -eq 0) 'negative-setup: overwrite fixture written' ("rc=" + $w7.Rc)
    $exe7 = Join-Path $bin7 'game-svr.exe'
    $fs = [System.IO.File]::Open($exe7, [System.IO.FileMode]::Append, [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
    try { $fs.WriteByte([byte]0x5a) } finally { $fs.Dispose() }
    $c7 = Invoke-Child @('-Check', '-BinDir', $bin7, '-RepoRoot', $Repo, '-Names', 'game-svr')
    SAssert ($c7.Rc -eq 1) 'negative: binary overwritten after stamp rc=1' ("rc=" + $c7.Rc + " out=" + $c7.Text)
    SAssert ($c7.Text.IndexOf('SHA256_MISMATCH', [StringComparison]::Ordinal) -ge 0) `
      'negative: binary overwrite reports SHA256_MISMATCH' ("out=" + $c7.Text)

    # Case 8：产戳时 head 解析不到 ⇒ 拒绝写戳（rc=2，且不产生戳文件）
    $script:SelfCases++
    $bin8 = New-Fixture $Tmp 'badhead' @('game-svr')
    $badChild = @('-BinDir', $bin8, '-RepoRoot', $Repo, '-Head', ('0' * 40), '-NoToolchainProbe', '-Names', 'game-svr')
    $w8 = Invoke-Child $badChild
    SAssert ($w8.Rc -eq 2) 'negative: write refuses nonexistent head rc=2' ("rc=" + $w8.Rc + " out=" + $w8.Text)
    $stamp8 = Join-Path $bin8 'game-svr.exe.build.json'
    SAssert (-not (Test-Path -LiteralPath $stamp8)) 'negative: write refused to create stamp' ("stamp=" + $stamp8)
    SAssert ($w8.Text.IndexOf('does not resolve', [StringComparison]::Ordinal) -ge 0) `
      'negative: write names the unresolvable head' ("out=" + $w8.Text)
  }

  $tmpRoot = [System.IO.Path]::GetTempPath()
  $tmp = Join-Path $tmpRoot ('stamp-binaries-selftest-' + [guid]::NewGuid().ToString('N'))
  $cleanupNote = ''
  try {
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    $repo = Join-Path $tmp 'repo'
    New-Item -ItemType Directory -Force -Path $repo | Out-Null
    $gi = Invoke-GitSanitized $repo @('init', '-q')
    if ($gi.Rc -ne 0) { $script:SelfInfra = 'git init failed: ' + (($gi.Out | ForEach-Object { "$_" }) -join ' ') }
    if ($script:SelfInfra -eq '') {
      $gc = Invoke-GitSanitized $repo @('-c', 'user.name=stamp-selftest', '-c', 'user.email=stamp-selftest@invalid',
                                        '-c', 'commit.gpgsign=false', 'commit', '--allow-empty', '-m', 'stamp-selftest')
      if ($gc.Rc -ne 0) { $script:SelfInfra = 'git commit failed: ' + (($gc.Out | ForEach-Object { "$_" }) -join ' ') }
    }
    $headSha = ''
    if ($script:SelfInfra -eq '') {
      $hr = Invoke-GitSanitized $repo @('rev-parse', 'HEAD')
      if ($hr.Rc -ne 0) { $script:SelfInfra = 'git rev-parse HEAD failed' }
      else { $headSha = (($hr.Out | ForEach-Object { "$_" }) -join '').Trim() }
    }
    if ($script:SelfInfra -eq '' -and ($headSha -notmatch '^[0-9a-f]{40}$')) {
      $script:SelfInfra = 'temp repo HEAD is not 40 lowercase hex: ' + $headSha
    }
    if ($script:SelfInfra -eq '') {
      Invoke-SelfCases $tmp $repo $headSha @('game-svr', 'game-gate', 'DBSrv')
    }
  } finally {
    # 只清理本自测自己创建的唯一临时目录（创建方清理；不是项目产物清理）。
    if (Test-Path -LiteralPath $tmp) {
      $guard = $tmp.Replace('\', '/').ToLowerInvariant()
      if ($guard.StartsWith($tmpRoot.Replace('\', '/').ToLowerInvariant()) -and $guard.Contains('stamp-binaries-selftest-')) {
        try { Remove-Item -LiteralPath $tmp -Recurse -Force } catch { $cleanupNote = 'cleanup failed: ' + $_.Exception.Message }
      } else {
        $cleanupNote = 'cleanup skipped: temp dir failed ownership guard'
      }
    }
  }
  if ($cleanupNote -ne '') { Emit ("WARN " + $cleanupNote + " (tmp=" + $tmp + ")") }
  if ($script:SelfInfra -ne '') {
    Emit ("SELFTEST_INFRA " + $script:SelfInfra)
    exit 2
  }
  SAssert ($script:SelfCases -eq 8) 'meta: expected case count executed' ("cases=" + $script:SelfCases)
  if ($script:SelfFails.Count -gt 0) {
    Emit ("stamp-binaries selftest: RED (" + $script:SelfFails.Count + " failed)")
    exit 1
  }
  Emit "stamp-binaries selftest: GREEN (1 positive + 7 negative)"
  exit 0
}

# ---- 前置校验（非自测路径）-----------------------------------------------
$gitProbe = Invoke-Git $RepoRoot @('rev-parse', '--git-dir')
if ($gitProbe.Rc -ne 0) {
  Fail ("RepoRoot is not a git repository: " + $RepoRoot)
}

if ($Check) {
  if ([string]::IsNullOrWhiteSpace($BinDir)) { Fail "-BinDir is required with -Check" }
  if (-not (Test-Path -LiteralPath $BinDir -PathType Container)) { Fail ("BinDir not found: " + $BinDir) }
  $binRoot = (Resolve-Path -LiteralPath $BinDir).Path
  $found = 0
  $problems = 0
  foreach ($name in $wanted) {
    $exe = Find-Exe $binRoot $name
    if ($null -eq $exe) {
      Emit ("SKIP " + $name + " (absent)")
      if ($RequireAll) {
        Emit ("RED " + $name + " MISSING_BINARY")
        $problems++
      }
      continue
    }
    $found++
    $stampPath = $exe + '.build.json'
    $errs = Test-OneStamp $exe $stampPath $RepoRoot
    if ($errs.Count -eq 0) {
      Emit ("STAMP_OK " + $exe)
    } else {
      foreach ($e in $errs) {
        Emit ("RED " + $exe + " " + $e)
        $problems++
      }
    }
  }
  if ($found -eq 0) {
    Emit ("RED NO_BINARIES " + $binRoot)
    exit 2
  }
  if ($problems -gt 0) {
    Emit ("RED " + $problems + " problem(s) across " + $found + " binaries")
    exit 1
  }
  Emit ("GREEN " + $found + " stamps verified")
  exit 0
}

# ---- 产戳 ----------------------------------------------------------------
if ([string]::IsNullOrWhiteSpace($BinDir)) { Fail "-BinDir is required (or -Check / -Selftest)" }
if (-not (Test-Path -LiteralPath $BinDir -PathType Container)) { Fail ("BinDir not found: " + $BinDir) }
$binRoot = (Resolve-Path -LiteralPath $BinDir).Path

$headFromParam = $Head.Trim().ToLowerInvariant()
$headSourceText = $HeadSource.Trim()
$headMap = @{}
$headPairs = New-Object System.Collections.Generic.List[string]
foreach ($chunk in @($Heads)) {
  foreach ($p in ([string]$chunk).Split(',')) {
    if ($p -and $p.Trim()) { $headPairs.Add($p.Trim()) }
  }
}
foreach ($pair in $headPairs) {
  if ([string]::IsNullOrWhiteSpace($pair)) { continue }
  $idx = $pair.IndexOf('=')
  if ($idx -le 0 -or $idx -eq ($pair.Length - 1)) { Fail ("bad -Heads entry (want name=sha): " + $pair) }
  $k = $pair.Substring(0, $idx).Trim().ToLowerInvariant()
  $v = $pair.Substring($idx + 1).Trim().ToLowerInvariant()
  if ($headMap.ContainsKey($k)) { Fail ("duplicate -Heads entry for " + $k) }
  if (-not $seenNames.ContainsKey($k)) { Fail ("-Heads names an entry not in -Names: " + $k) }
  if (-not (Test-CommitExists $RepoRoot $v)) { Fail ("head does not resolve to a commit: " + $v + " (" + $k + ")") }
  $headMap[$k] = $v
}
if ($headFromParam -ne '' -and -not (Test-CommitExists $RepoRoot $headFromParam)) {
  Fail ("head does not resolve to a commit: " + $headFromParam)
}

$repoHead = ''
if ($headFromParam -eq '' -and $headMap.Count -eq 0) {
  $r = Invoke-Git $RepoRoot @('rev-parse', '--verify', 'HEAD')
  if ($r.Rc -ne 0) { Fail ("cannot resolve HEAD in " + $RepoRoot) }
  $repoHead = (($r.Out | ForEach-Object { "$_" }) -join '').Trim()
  if ($repoHead -notmatch '^[0-9a-f]{40}$') { Fail ("HEAD is not 40 lowercase hex: " + $repoHead) }
}

$dirty = Get-GitDirtyText $RepoRoot
$present = 0
$stamped = 0
foreach ($name in $wanted) {
  $exe = Find-Exe $binRoot $name
  if ($null -eq $exe) {
    Emit ("SKIP " + $name + " (absent)")
    continue
  }
  $present++
  $key = ([System.IO.Path]::GetFileNameWithoutExtension($exe)).ToLowerInvariant()
  if ($headMap.ContainsKey($key)) {
    $h = $headMap[$key]
    if ($headSourceText -ne '') { $src = $headSourceText } else { $src = 'explicit_per_binary' }
  } elseif ($headFromParam -ne '') {
    $h = $headFromParam
    if ($headSourceText -ne '') { $src = $headSourceText } else { $src = 'explicit' }
  } else {
    $h = $repoHead
    if ($headSourceText -ne '') { $src = $headSourceText } else { $src = 'inferred_repo_head' }
  }
  if (Write-StampFile -ExePath $exe -Name $key -HeadSha $h -HeadSrc $src -DirtyText $dirty -DoForce:$([bool]$Force)) {
    $stamped++
  }
}
if ($present -eq 0) {
  Emit ("RED NO_BINARIES " + $binRoot)
  exit 2
}
Emit ("STAMPED " + $stamped + " (" + $present + " binaries present)")
exit 0
