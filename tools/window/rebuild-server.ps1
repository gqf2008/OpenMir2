# -*- coding: utf-8 -*-
# rebuild-server.ps1 — 窗口轮次的 release 二进制重建 + head/sha256 对齐（卡 openmir2-window-run2-prep）
#
# 为什么需要它（window-run-1 §0 的「旧二进制假绿」纪律）：
#   * 真机证据里的 Rust sha256 必须指向**本次窗口、当前 head** 构建出来的
#     `mir2-rs/target/release/mir2-server.exe`；拿旧构建跑出来的 PASS 不可追溯。
#   * run-2 的私栈启动脚本 `E:/tmp/conformance/start-rust.ps1` 把默认 `-ServerExe`
#     钉在 `E:\tmp\wt-worker-73\...`（该 worktree 已删除）；而 `card_window.py` 起栈后
#     会用 `mir2-server.pid.json` 的 `roles[].exe` 与 profile 的 binaries 对账 —— 不修
#     直接 rc=2。`-SyncStartRust` 把默认值改到**本检出**的 release 二进制（原文件先备份）。
#
# 用法：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/rebuild-server.ps1
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/rebuild-server.ps1 -SyncStartRust
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/rebuild-server.ps1 -Plan   # 只打印，不构建/不改文件
#
# 产物（都在 target/ 下，不入库）：
#   mir2-rs/target/release/mir2-server.exe
#   mir2-rs/target/release/mir2-server.build.json   ← format=mir2-build-stamp/1
#     { head, head_subject, built_at, dirty, exe, sha256, size, cargo, rustc }
#
# 退出码：0 = 构建成功且 stamp 写出；2 = 任何前置/构建/写 stamp/同步失败（fail-closed）。
# 机器可读输出（供 batch/preflight 解析）：
#   BUILD_HEAD <sha>          BUILD_DIRTY <0|1>
#   SERVER_EXE <path>         SERVER_EXE_SHA256 <hex>
#   BUILD_STAMP <path>        START_RUST_SERVER_EXE <path|unchanged|skipped>

param(
  [string]$RepoRoot = '',
  [string]$StartRustScript = 'E:/tmp/conformance/start-rust.ps1',
  [switch]$SyncStartRust,
  [switch]$SkipBuild,
  [switch]$RequireClean,
  [switch]$Plan
)

$ErrorActionPreference = 'Stop'

# 全局兜底：任何未捕获的终止错误（含将来有人删掉某段 try/catch）都必须 rc=2 + `RED:`，
# 不得让脚本以 0 退出却留下旧 stamp。
trap {
  Write-Host ("RED: 未捕获异常：" + $_.Exception.Message)
  exit 2
}

function Fail([string]$msg) {
  Write-Host ("RED: " + $msg)
  exit 2
}

# SHA256 用 .NET 直接算，**不依赖 `Get-FileHash`**：
#   * Windows PowerShell 5.1 在本机 PSModulePath 缺 `Microsoft.PowerShell.Utility` 时会报
#     `Get-FileHash is not recognized`（2026-10-27 窗口重建现场：构建成功但 stamp 没写成）；
#   * `[System.Security.Cryptography.SHA256]::Create()` + `[IO.File]::Open` 在 5.1 与 7 都可用。
# 任何异常都向上抛，由调用点转成 `Fail`（fail-closed），不得吞掉后继续写 stamp。
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
    throw ("ComputeHash 返回 " + $(if ($null -eq $bytes) { 'null' } else { $bytes.Length.ToString() + ' 字节' }) + "（应为 32）")
  }
  return (($bytes | ForEach-Object { $_.ToString('x2') }) -join '')
}

if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
  # $PSScriptRoot = <repo>/tools/window ⇒ 上两级 = 检出根（worktree 也一样）
  $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
}
$RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path
$Exe = Join-Path $RepoRoot 'mir2-rs\target\release\mir2-server.exe'
$Stamp = Join-Path $RepoRoot 'mir2-rs\target\release\mir2-server.build.json'
$RustRoot = Join-Path $RepoRoot 'mir2-rs'

if (-not (Test-Path -LiteralPath (Join-Path $RustRoot 'Cargo.toml') -PathType Leaf)) {
  Fail ("找不到 cargo 工作区：$RustRoot\Cargo.toml（RepoRoot 解析错？）")
}

# ---- head / dirty ---------------------------------------------------------
$Head = (& git -C $RepoRoot rev-parse HEAD 2>$null)
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($Head)) {
  Fail "git rev-parse HEAD 失败（不是 git 检出？）：$RepoRoot"
}
$Head = $Head.Trim()
$HeadSubject = (& git -C $RepoRoot show -s --format=%s HEAD 2>$null)
if ($LASTEXITCODE -ne 0) { $HeadSubject = '' }
$DirtyLines = @(& git -C $RepoRoot status --porcelain 2>$null)
$Dirty = (($DirtyLines | Where-Object { $_ -and $_.Trim() }) -join "`n")

Write-Host ("BUILD_HEAD " + $Head)
Write-Host ("BUILD_DIRTY " + $(if ($Dirty) { 1 } else { 0 }))
if ($Dirty) {
  Write-Host ("WARNING: 工作树不干净，构建不可追溯到 HEAD：`n" + $Dirty)
  if ($RequireClean) { Fail "工作树不干净；提交/清理后重跑（或去掉 -RequireClean）" }
}

# ---- 构建 ----------------------------------------------------------------
if (-not $SkipBuild) {
  if ($Plan) {
    Write-Host ("PLAN: pushd " + $RustRoot + " ; cargo build --release -p mir2-server")
  } else {
    Push-Location $RustRoot
    try {
      & cargo build --release -p mir2-server
      $rc = $LASTEXITCODE
    } finally {
      Pop-Location
    }
    if ($rc -ne 0) { Fail ("cargo build --release -p mir2-server rc=" + $rc) }
  }
} else {
  Write-Host "SKIP_BUILD（-SkipBuild：只重算 sha256/stamp）"
}

if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) {
  Fail ("构建后仍缺 release 二进制：" + $Exe)
}

# ---- sha256 / stamp ------------------------------------------------------
# fail-closed：size / sha256 任一失败 ⇒ rc=2 + `RED:`，**不写 stamp**（旧 stamp 留着也只能被
# rc=2 显式判红，绝不允许「哈希失败但 exit 0」这种失效模式复活）。
try {
  $Size = [long](Get-Item -LiteralPath $Exe -ErrorAction Stop).Length
} catch {
  Fail ("读二进制大小失败：" + $Exe + " :: " + $_.Exception.Message)
}
if ($Size -le 0) {
  Fail ("二进制大小为 0（不可作为构建产物）：" + $Exe)
}
try {
  $Sha = Get-Sha256Hex $Exe
} catch {
  Fail ("SHA256 计算失败：" + $Exe + " :: " + $_.Exception.Message)
}
if ($Sha -notmatch '^[0-9a-f]{64}$') {
  Fail ("SHA256 不是 64 位小写 hex（实际='" + $Sha + "'）：" + $Exe)
}
$CargoVer = ''
$RustcVer = ''
try { $CargoVer = ((& cargo --version 2>$null) -join ' ') } catch {}
try { $RustcVer = ((& rustc --version 2>$null) -join ' ') } catch {}

Write-Host ("SERVER_EXE " + $Exe)
Write-Host ("SERVER_EXE_SHA256 " + $Sha)
Write-Host ("SERVER_EXE_SIZE " + $Size)

if ($Plan) {
  Write-Host ("PLAN: write stamp " + $Stamp)
} else {
  $doc = [ordered]@{
    format       = 'mir2-build-stamp/1'
    head         = $Head
    head_subject = ([string]$HeadSubject).Trim()
    built_at     = (Get-Date).ToString('o')
    dirty        = $Dirty
    exe          = $Exe.Replace('\', '/')
    sha256       = $Sha
    size         = $Size
    cargo        = $CargoVer
    rustc        = $RustcVer
  }
  # fail-closed 写 stamp：先写临时文件再替换，写后回读核对；任一步失败 ⇒ rc=2。
  # 绝不 swallow 后 exit 0（2026-10-27 失效模式：stamp 留旧 head/sha 却被当成功）。
  $tmp = $Stamp + '.tmp-' + $PID
  try {
    $json = ($doc | ConvertTo-Json -Depth 4)
    if ([string]::IsNullOrWhiteSpace($json)) { throw 'ConvertTo-Json 产出空串' }
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    $dir = Split-Path -Parent $Stamp
    if (-not (Test-Path -LiteralPath $dir -PathType Container)) {
      New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }
    [System.IO.File]::WriteAllText($tmp, $json + "`n", $utf8)
    Move-Item -LiteralPath $tmp -Destination $Stamp -Force
    $check = [System.IO.File]::ReadAllText($Stamp, $utf8) | ConvertFrom-Json
    if ([string]$check.head -ne $Head) { throw ("回读 head='{0}' 与 git HEAD='{1}' 不一致" -f $check.head, $Head) }
    if ([string]$check.sha256 -ne $Sha) { throw ("回读 sha256='{0}' 与计算值不一致" -f $check.sha256) }
    if ([long]$check.size -ne $Size) { throw ("回读 size='{0}' 与文件字节={1} 不一致" -f $check.size, $Size) }
  } catch {
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue }
    Fail ("写 stamp 失败：" + $Stamp + " :: " + $_.Exception.Message)
  }
  Write-Host ("BUILD_STAMP " + $Stamp)
}

# ---- 同步 start-rust.ps1 的两个默认值 -------------------------------------
#   -ServerExe : 本检出的 release 二进制（run-2 遗留 wt-worker-73 路径会 pid/exe 对账红）
#   $starter   : 本检出的 tools/m2-acceptance/start_mir2_server.ps1（避免主检出旧启动器）
$syncResult = 'skipped'
if ($SyncStartRust) {
  if (-not (Test-Path -LiteralPath $StartRustScript -PathType Leaf)) {
    Fail ("-SyncStartRust 但找不到 " + $StartRustScript)
  }
  $text = [System.IO.File]::ReadAllText($StartRustScript)
  $norm = { param($p) $p.Replace('/', '\').TrimEnd('\').ToLowerInvariant() }
  $starterPath = Join-Path $RepoRoot 'tools\m2-acceptance\start_mir2_server.ps1'
  $specs = @(
    @{ Label='ServerExe'; Pattern="(\[string\]\s*\`$\s*ServerExe\s*=\s*')([^']*)(')"; Want=$Exe },
    @{ Label='starter';   Pattern="(\`$starter\s*=\s*')([^']*)(')"; Want=$starterPath }
  )
  $changes = @()
  foreach ($spec in $specs) {
    $hits = [regex]::Matches($text, $spec.Pattern, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    if ($hits.Count -ne 1) {
      Fail ("start-rust.ps1 里 " + $spec.Label + " 匹配 " + $hits.Count + " 处（应恰好 1 处），拒绝改写")
    }
    $current = $hits[0].Groups[2].Value
    if ((& $norm $current) -ne (& $norm $spec.Want)) {
      $changes += ,@{ Label=$spec.Label; Pattern=$spec.Pattern; Want=$spec.Want; Current=$current }
    }
  }
  if ($changes.Count -eq 0) {
    $syncResult = 'unchanged'
    Write-Host ("START_RUST_SERVER_EXE unchanged (" + $Exe + ")")
    Write-Host ("START_RUST_STARTER unchanged (" + $starterPath + ")")
  } elseif ($Plan) {
    $syncResult = 'plan'
    foreach ($c in $changes) {
      Write-Host ("PLAN: " + $StartRustScript + "  " + $c.Label + " '" + $c.Current + "' -> '" + $c.Want + "'")
    }
  } else {
    $bak = $StartRustScript + '.bak-' + (Get-Date).ToString('yyyyMMdd-HHmmss')
    Copy-Item -LiteralPath $StartRustScript -Destination $bak -Force
    $new = $text
    foreach ($c in $changes) {
      $want = $c.Want
      $replacement = [System.Text.RegularExpressions.MatchEvaluator]{
        param($m)
        $m.Groups[1].Value + $want + $m.Groups[3].Value
      }
      $new = [regex]::Replace($new, $c.Pattern, $replacement, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
      Write-Host ("START_RUST_" + $c.Label.ToUpperInvariant() + " updated (" + $c.Current + " -> " + $c.Want + ")")
    }
    $utf8n = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($StartRustScript, $new, $utf8n)
    $syncResult = 'updated'
    Write-Host ("备份：" + $bak)
  }
} else {
  Write-Host "START_RUST_SERVER_EXE skipped（未给 -SyncStartRust）"
}

Write-Host ("REBUILD_RESULT " + $(if ($Plan) { 'PLAN' } else { 'OK' }) + " sync=" + $syncResult)
exit 0
