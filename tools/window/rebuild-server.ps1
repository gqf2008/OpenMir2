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

function Fail([string]$msg) {
  Write-Host ("RED: " + $msg)
  exit 2
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
$Sha = (Get-FileHash -LiteralPath $Exe -Algorithm SHA256).Hash.ToLowerInvariant()
$Size = (Get-Item -LiteralPath $Exe).Length
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
  $json = ($doc | ConvertTo-Json -Depth 4)
  $utf8 = New-Object System.Text.UTF8Encoding($false)
  $dir = Split-Path -Parent $Stamp
  if (-not (Test-Path -LiteralPath $dir -PathType Container)) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
  }
  [System.IO.File]::WriteAllText($Stamp, $json + "`n", $utf8)
  Write-Host ("BUILD_STAMP " + $Stamp)
}

# ---- 同步 start-rust.ps1 的 -ServerExe 默认值 -----------------------------
$syncResult = 'skipped'
if ($SyncStartRust) {
  if (-not (Test-Path -LiteralPath $StartRustScript -PathType Leaf)) {
    Fail ("-SyncStartRust 但找不到 " + $StartRustScript)
  }
  $text = [System.IO.File]::ReadAllText($StartRustScript)
  $pattern = "(\[string\]\s*\`$\s*ServerExe\s*=\s*')([^']*)(')"
  $hits = [regex]::Matches($text, $pattern, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
  if ($hits.Count -ne 1) {
    Fail ("start-rust.ps1 里 `$ServerExe 默认值匹配 " + $hits.Count + " 处（应恰好 1 处），拒绝改写")
  }
  $current = $hits[0].Groups[2].Value
  $want = $Exe
  $norm = { param($p) $p.Replace('/', '\').TrimEnd('\').ToLowerInvariant() }
  if ((& $norm $current) -eq (& $norm $want)) {
    $syncResult = 'unchanged'
    Write-Host ("START_RUST_SERVER_EXE unchanged (" + $current + ")")
  } elseif ($Plan) {
    $syncResult = 'plan'
    Write-Host ("PLAN: " + $StartRustScript + "  -ServerExe '" + $current + "' -> '" + $want + "'")
  } else {
    $bak = $StartRustScript + '.bak-' + (Get-Date).ToString('yyyyMMdd-HHmmss')
    Copy-Item -LiteralPath $StartRustScript -Destination $bak -Force
    $replacement = [System.Text.RegularExpressions.MatchEvaluator]{
      param($m)
      $m.Groups[1].Value + $want + $m.Groups[3].Value
    }
    $new = [regex]::Replace($text, $pattern, $replacement, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    $utf8n = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($StartRustScript, $new, $utf8n)
    $syncResult = 'updated'
    Write-Host ("START_RUST_SERVER_EXE updated (" + $current + " -> " + $want + ")，备份：" + $bak)
  }
} else {
  Write-Host "START_RUST_SERVER_EXE skipped（未给 -SyncStartRust）"
}

Write-Host ("REBUILD_RESULT " + $(if ($Plan) { 'PLAN' } else { 'OK' }) + " sync=" + $syncResult)
exit 0
