#requires -Version 5
<#
  m3hashutil.ps1 — M3 夹具共用的环境无关 SHA256（.NET）、**能读被别的进程持有的活文件**，
  以及**生产者侧 LF 写出**（`Write-M3LfText/Write-M3LfLines/Set-M3LfContent` + 覆盖内置 `Set-Content`）。

  三个坑，都已在真机撞过：

  ① `Get-FileHash` 属 `Microsoft.PowerShell.Utility`。本机 PS 5.1 在 `PSModulePath` 被 PS7 污染时
     不导出该 cmdlet ⇒ `$ErrorActionPreference='Stop'` 下 `CommandNotFoundException` 崩溃
     （m3cs 首跑走 `Test-M3MonGenEmpty -VerifySource` 复现）。修法 = 用 .NET SHA256（不依赖模块解析，
     结果与 `(Get-FileHash -Algorithm SHA256).Hash` 逐字节相同；见 tools/oracle/hashutil.ps1、
     tools/m2-acceptance/preflight.ps1 同一修法）。

  ② `[System.IO.File]::ReadAllBytes` 以 `FileShare.Read` 打开；而 C# 影子 GameSvr 用
     `FileShare.ReadWrite` 持有 `rng-attack-stream.tsv` ⇒ Windows 共享模式下读被拒
     （`The process cannot access the file ... because it is being used by another process`；
     `start-csharp-hook.ps1 -IsolateAttack` 在写完 `ISOLATION_RECORDED` 前抛，此时栈已起/端口已就绪，
     m3cs tick22 F1 复现）⇒ 专用流 sha256 记不出来，只能置空。修法 = 显式
     `FileStream(FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete)` 流式读。

  `Test-M3Sha256HotFileSelfTest` 钉住②：holder 以 `FileShare.ReadWrite` 持有临时件 ⇒ 本函数必须算出
  标准 SHA256（锚 = 64 字节 0x00..0x3F = `fdeab9ac…1108`）；而旧的 `File.ReadAllBytes` 必须被拒。
  若「旧读法没被拒」⇒ 说明该环境没复现共享冲突 ⇒ 自检判红（不许假绿）。改回 `ReadAllBytes` ⇒ 自检必红。

  ③ 写出件默认 CRLF（Windows PowerShell 的 `Set-Content`/`WriteAllLines`）而仓库声明 `text eol=lf`
     ⇒ 证据清单「工作树自洽、干净检出不可复现」（EOL 陷阱同族第 5 次，本次正是自检转录件）。
     修法 = 本文件文末的 LF 写出与 `Set-Content` 覆盖；`Test-M3LfWriterSelfTest` 钉住，改回默认 ⇒ 必红。
#>

# 64 字节 0x00..0x3F 的标准 SHA256 —— 用作「算法/实现没被换掉」的锚点。
$script:M3Sha256Anchor = 'fdeab9acf3710362bd2658cdc9a29e8f9c757fcf9811603a8c447cd1d9151108'

function Get-M3Sha256Hex([string]$Path) {
  if ([string]::IsNullOrEmpty($Path)) { throw 'Get-M3Sha256Hex: 路径为空' }
  if (-not (Test-Path -LiteralPath $Path)) { throw "Get-M3Sha256Hex: 文件不存在: $Path" }
  $alg = [System.Security.Cryptography.SHA256]::Create()
  $share = ([System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete)
  $fs = $null
  try {
    # 关键：读活文件（被 GameSvr 以 FileShare.ReadWrite 持有）必须显式允许读写共享。
    $fs = [System.IO.FileStream]::new($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, $share)
    return ([System.BitConverter]::ToString($alg.ComputeHash($fs)) -replace '-', '').ToLowerInvariant()
  } finally {
    if ($fs) { $fs.Dispose() }
    $alg.Dispose()
  }
}

function Test-M3Sha256HotFileSelfTest {
  $result = [ordered]@{
    ok            = $true
    reason        = ''
    hot_hash      = ''
    expected      = $script:M3Sha256Anchor
    oldway_denied = $false
  }
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('m3hash-hot-' + [System.Guid]::NewGuid().ToString('N'))
  $holder = $null
  try {
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    $path = Join-Path $tmp 'hot-stream.tsv'
    $bytes = New-Object byte[] 64
    for ($i = 0; $i -lt 64; $i++) { $bytes[$i] = [byte]$i }
    [System.IO.File]::WriteAllBytes($path, $bytes)

    # holder 模拟 live GameSvr：以 FileShare.ReadWrite 持有（可读可写可共享）。
    $holder = [System.IO.FileStream]::new($path, [System.IO.FileMode]::Open,
      [System.IO.FileAccess]::ReadWrite,
      ([System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete))

    $result.hot_hash = Get-M3Sha256Hex -Path $path
    if ($result.hot_hash -ne $result.expected) {
      $result.ok = $false
      $result.reason = 'hot_hash_mismatch'
    }

    # 改坏必红：旧的 FileShare.Read 读法（File.ReadAllBytes）在 holder 的共享模式下必须被拒。
    $denied = $false
    try { [void][System.IO.File]::ReadAllBytes($path) } catch { $denied = $true }
    $result.oldway_denied = $denied
    if (-not $denied -and $result.ok) {
      $result.ok = $false
      $result.reason = 'sharing_violation_not_reproduced'
    }
  } catch {
    $result.ok = $false
    $result.reason = ('exception:' + $_.Exception.Message)
  } finally {
    if ($holder) { $holder.Dispose() }
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  }
  return [pscustomobject]$result
}

# --------------------------------------------------------------------------- #
# LF 写出 —— **生产者侧**根因修（owner 2026-10-13 01:15；EOL 陷阱同族第 5 次）
#
# 同族坑：夹具/自检脚本在 Windows PowerShell 下用默认 `Set-Content` / `[IO.File]::WriteAllLines`
# 写出的文本是 CRLF，而 `mir2-rs/.gitattributes` 对 `*.txt/*.json/*.jsonl/*.log/*.md/*.cs/*.rs/...`
# 声明 `text eol=lf` ⇒ 提交进仓库的是 LF、`SHA256SUMS.txt` 若按盘上 CRLF 记就**只在作者工作树自洽**，
# 干净检出 / 仓库字节口径必红（历史 5 次：C6 F9-1 / M4 N-1 / lifecycle N-1 / `170a51b9` /
# m3cs 的 `window/00-selfcheck-nostack.txt`——而后者正是 `run_attack_pair.ps1 -SelfCheckNoStack` 的产物）。
#
# 修法 = 让**生产者**写 LF，一处收口：
#   ① `Write-M3LfText` / `Write-M3LfLines` / `Set-M3LfContent`：UTF-8 无 BOM、行尾统一 LF；
#   ② 在本文件里**显式覆盖内置 `Set-Content`**：凡 dot-source 本文件的夹具（全部 m3 脚本经由
#      `gate_guards.ps1` / 直接 dot-source 拿到）写出的文本一律 LF，**不必逐个调用点手改**，
#      也防止将来又写回 CRLF。这是**有意**覆盖（PowerShell 命令优先级：函数 > cmdlet）；
#      只覆盖夹具实际用到的 `-Path/-LiteralPath/-Value/-Encoding` 与管道输入。`-Encoding` 仅为
#      兼容既有调用点保留，实现固定 UTF-8 无 BOM（纯 ASCII 内容字节不变，且比 PS 5.1 的 utf8-BOM 更安全）。
#   ③ `[System.IO.File]::WriteAllLines`（用 Environment.NewLine，绕过覆盖）在夹具里改用 `Write-M3LfLines`。
#   ④ `Test-M3LfWriterSelfTest` 钉住：写出件不含 0x0D 且与 LF 归一逐字节相同；把覆盖改回默认（CRLF）
#      ⇒ 无栈自检与 `verify_sha256sums.py eolcheck` 必红。
# --------------------------------------------------------------------------- #
$script:M3Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function ConvertTo-M3LfText([string]$Text) {
  if ($null -eq $Text) { return '' }
  return (($Text -replace "`r`n", "`n") -replace "`r", "`n")
}

function Write-M3LfText {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Path,
    [Parameter(Position = 1)][AllowNull()][string]$Text
  )
  $lf = ConvertTo-M3LfText $Text
  $dir = Split-Path -Parent $Path
  if ($dir -and -not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
  [System.IO.File]::WriteAllText($Path, $lf, $script:M3Utf8NoBom)
}

function Write-M3LfLines {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Path,
    [Parameter(Position = 1)][AllowNull()][AllowEmptyCollection()]$Lines
  )
  $arr = @($Lines)
  $text = if ($arr.Count -eq 0) { '' } else { (($arr | ForEach-Object { [string]$_ }) -join "`n") + "`n" }
  Write-M3LfText -Path $Path -Text $text
}

# 显式名（供新代码直接调用；语义 = 原 `Set-Content`，但 UTF-8 无 BOM + LF）。
function Set-M3LfContent {
  [CmdletBinding(DefaultParameterSetName = 'Path')]
  param(
    [Parameter(Mandatory = $true, Position = 0, ParameterSetName = 'Path')][string[]]$Path,
    [Parameter(Mandatory = $true, ParameterSetName = 'LiteralPath')][string[]]$LiteralPath,
    [Parameter(ValueFromPipeline = $true, Position = 1)][AllowNull()][AllowEmptyCollection()]$Value,
    [string]$Encoding = 'utf8'
  )
  begin { $buf = New-Object System.Collections.ArrayList }
  process {
    if ($null -ne $Value) { foreach ($v in @($Value)) { [void]$buf.Add([string]$v) } }
  }
  end {
    $targets = if ($PSCmdlet.ParameterSetName -eq 'LiteralPath') { $LiteralPath } else { $Path }
    $text = if ($buf.Count -eq 0) { '' } else { (($buf) -join "`n") + "`n" }
    foreach ($t in @($targets)) { Write-M3LfText -Path $t -Text $text }
  }
}

# 生产者侧统一收口：覆盖内置 `Set-Content`（函数优先于 cmdlet）。
# 注意：**不得**在本函数内再调 `Set-Content`（会递归）；实现只走 `Write-M3LfText`。
function Set-Content {
  [CmdletBinding(DefaultParameterSetName = 'Path')]
  param(
    [Parameter(Mandatory = $true, Position = 0, ParameterSetName = 'Path')][string[]]$Path,
    [Parameter(Mandatory = $true, ParameterSetName = 'LiteralPath')][string[]]$LiteralPath,
    [Parameter(ValueFromPipeline = $true, Position = 1)][AllowNull()][AllowEmptyCollection()]$Value,
    [string]$Encoding = 'utf8'
  )
  begin { $buf = New-Object System.Collections.ArrayList }
  process {
    if ($null -ne $Value) { foreach ($v in @($Value)) { [void]$buf.Add([string]$v) } }
  }
  end {
    $targets = if ($PSCmdlet.ParameterSetName -eq 'LiteralPath') { $LiteralPath } else { $Path }
    $text = if ($buf.Count -eq 0) { '' } else { (($buf) -join "`n") + "`n" }
    foreach ($t in @($targets)) { Write-M3LfText -Path $t -Text $text }
  }
}

function Test-M3LfWriterSelfTest {
  $result = [ordered]@{
    ok          = $false
    reason      = ''
    has_cr      = $false
    override_cr = $false
    text        = ''
  }
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('m3lf-' + [System.Guid]::NewGuid().ToString('N'))
  try {
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    $p1 = Join-Path $tmp 'helper.txt'
    Set-M3LfContent -Path $p1 -Value @('alpha', 'beta')
    $result.has_cr = ([System.IO.File]::ReadAllBytes($p1) -contains [byte]13)
    $result.text = [System.IO.File]::ReadAllText($p1)
    if ($result.has_cr) { $result.reason = 'helper_wrote_crlf'; return [pscustomobject]$result }
    if ($result.text -ne "alpha`nbeta`n") { $result.reason = 'helper_text_mismatch'; return [pscustomobject]$result }
    # 覆盖生效：裸 `Set-Content` 也必须写 LF（连 CRLF 输入都被归一）。改回默认 ⇒ 本项必红。
    $p2 = Join-Path $tmp 'override.txt'
    Set-Content -Path $p2 -Value "gamma`r`ndelta" -Encoding utf8
    $result.override_cr = ([System.IO.File]::ReadAllBytes($p2) -contains [byte]13)
    if ($result.override_cr) { $result.reason = 'setcontent_override_not_lf'; return [pscustomobject]$result }
    if ([System.IO.File]::ReadAllText($p2) -ne "gamma`ndelta`n") { $result.reason = 'override_text_mismatch'; return [pscustomobject]$result }
    $result.ok = $true
  } catch {
    $result.reason = 'exception:' + $_.Exception.Message
  } finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  }
  return [pscustomobject]$result
}
