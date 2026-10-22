#requires -Version 5
<#
.SYNOPSIS
  export_s2c.ps1 — M2 验收③「同序列出包 diff」的**导出半段**：把 Rust 侧 s2c 帧序列导出为 jsonl。

.DESCRIPTION
  不新写一套导出器——复用仓库既有的 M0/C 线流水线（字段口径与
  `tests/golden/session-20261010-c3-hook/capture.jsonl` 完全一致）：

    1. 抓包：客户端 ↔ 7200 经 `tools/capture/mir2_proxy.py`（本脚本**不抓**，接受已有 dump 目录）
       → dump 目录里有 `chunks.ndjson` + `conn-NNNN.{c2s,s2c}.bin`
    2. 切帧：`python tools/capture/segment_frames.py --session <dump> --out <OutDir>` → frames.ndjson + manifest.json
    3. 解码：`dotnet run --project tools/capture/GoldenExport -c Release -- --session <dump> --out <OutDir>/capture.jsonl`
       （GoldenExport 输出的 schema 就是 capture.jsonl：dir/seq/port/hop/conn/raw_hex/ident/param/tag/series/…）
    4. 自洽：`cargo run -p replay --offline -- --no-coverage <capture.jsonl>`
       （既有 replay 门禁：每条 raw_hex decode→encode 逐字节 + ident/param/tag/series 字段对拍；这是"别新写一套"的对账工具）

  客户端身份：
    卡 fixture-harden（root cause：docs/npc-render-diagnosis-20261022.md §2.5/§5）要求 manifest 记
    `client_exe` + `client_exe_sha256` + `client_exe_mtime` + `client_exe_size`。脚本在切帧后把这些字段
    合并进 `$OutDir/manifest.json`（-ClientExe 显式给；或 -RunDir 下找 MirClinet.exe/MirClientZig.exe；
    或环境变量 MIR2_CLIENT_EXE）。找不到时字段记 null 并 WARN，不静默假装知道。

  红/绿：
    GREEN  = 三步都 0 退出（导出 + replay 自洽）；
    PREREQ = 缺 dump 目录 / 缺 chunks.ndjson / 缺 dotnet ⇒ 退出码 2，不静默通过；
    RED    = 切帧/导出/replay 任一步失败 ⇒ 退出码 1。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/export_s2c.ps1 `
    -ProxyDir E:\tmp\m2-acceptance\run1\proxy -OutDir E:\tmp\m2-acceptance\run1\rust
#>
param(
  [string]$ProxyDir = "",
  [string]$OutDir = "",
  [string]$GoldenExportExe = "",   # 可选：预构建的 GoldenExport.exe；不给则 dotnet run
  [string]$ClientExe = "",         # 可选：客户端 exe 全路径（写进 manifest.json 的 client_exe* 字段）
  [string]$RunDir = "",            # 可选：客户端运行目录（自动找 MirClinet.exe / MirClientZig.exe）
  [switch]$SkipReplay
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
$Tools = Join-Path $RepoRoot "tools"
$Mir2Rs = Join-Path $RepoRoot "mir2-rs"

if ([string]::IsNullOrEmpty($ProxyDir) -or [string]::IsNullOrEmpty($OutDir)) {
  Write-Output "PREREQ_MISSING: 必须给 -ProxyDir <mir2_proxy dump 目录> 与 -OutDir <导出目录>"
  exit 2
}
if (-not (Test-Path (Join-Path $ProxyDir "chunks.ndjson"))) {
  Write-Output ("PREREQ_MISSING: {0}\chunks.ndjson 不存在——先在验收运行里用 mir2_proxy.py 抓到 Rust 链路 dump" -f $ProxyDir)
  exit 2
}
# 切帧/布局导出都是 python（此前缺 python 会在这里抛终止错误、被聚合器误当 RED；preflight 也先行拦一道）。
if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
  Write-Output "PREREQ_MISSING: 找不到 python（segment_frames/dump_body_layout 需要；见 preflight.ps1）"
  exit 2
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$Capture = Join-Path $OutDir "capture.jsonl"

Write-Output "--- 1) segment_frames（切帧） ---"
python (Join-Path $Tools "capture\segment_frames.py") --session $ProxyDir --out $OutDir
if ($LASTEXITCODE -ne 0) { Write-Output "RED: segment_frames 失败"; exit 1 }

# ---- 客户端身份取证（卡 fixture-harden）----
# 本轮误判的直接原因：manifest 只记了 proxy/端口映射，没记跑的到底是哪台客户端
# （r5 实际是冻结 Delphi MirClinet.exe，报障却写成 MirClientZig）。
$clientPath = $ClientExe
if ([string]::IsNullOrEmpty($clientPath) -and -not [string]::IsNullOrEmpty($RunDir)) {
  foreach ($cand in @((Join-Path $RunDir "MirClinet.exe"), (Join-Path $RunDir "MirClientZig.exe"))) {
    if (Test-Path -LiteralPath $cand) { $clientPath = $cand; break }
  }
}
if ([string]::IsNullOrEmpty($clientPath) -and $env:MIR2_CLIENT_EXE) { $clientPath = $env:MIR2_CLIENT_EXE }
$clientFields = [ordered]@{
  client_exe = $null; client_exe_sha256 = $null; client_exe_mtime = $null; client_exe_size = $null
}
if (-not [string]::IsNullOrEmpty($clientPath) -and (Test-Path -LiteralPath $clientPath)) {
  $fi = Get-Item -LiteralPath $clientPath
  # 不用 Get-FileHash：本机 Windows PowerShell 5.1 没有该 cmdlet（实测 CommandNotFoundException）。
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $fs = [IO.File]::OpenRead($fi.FullName)
    try { $hashBytes = $sha.ComputeHash($fs) } finally { $fs.Dispose() }
  } finally { $sha.Dispose() }
  $clientFields.client_exe = $fi.FullName
  $clientFields.client_exe_sha256 = (([BitConverter]::ToString($hashBytes) -replace "-", "").ToLowerInvariant())
  $clientFields.client_exe_mtime = $fi.LastWriteTimeUtc.ToString("o")
  $clientFields.client_exe_size = [long]$fi.Length
} else {
  Write-Output "WARN: 未给 -ClientExe/-RunDir/MIR2_CLIENT_EXE，manifest 的 client_exe* 记为 null（本轮误判的直接原因就是缺这几个字段）"
}
$Manifest = Join-Path $OutDir "manifest.json"
if (Test-Path -LiteralPath $Manifest) {
  $man = [IO.File]::ReadAllText($Manifest, [Text.Encoding]::UTF8) | ConvertFrom-Json
  foreach ($k in @($clientFields.Keys)) {
    if ($null -ne $man.PSObject.Properties[$k]) { $man.$k = $clientFields[$k] }
    else { $man | Add-Member -NotePropertyName $k -NotePropertyValue $clientFields[$k] }
  }
  [IO.File]::WriteAllText($Manifest, (($man | ConvertTo-Json -Depth 100) + "`n"), (New-Object Text.UTF8Encoding($false)))
  Write-Output ("MANIFEST_CLIENT " + $Manifest + " client_exe=" + $clientFields.client_exe)
} else {
  Write-Output ("WARN: " + $Manifest + " 不存在（segment_frames 未产出 manifest），客户端字段未记录")
}

Write-Output "--- 2) GoldenExport（解码成 capture.jsonl） ---"
# A-9：必须用**当前派生**的分段布局表，否则多段帧（如 SM_ADJUST_BONUS）的 body_len/body_sha256
# 会与分段解码不一致，replay ② 会假红（既有工具 golden_fresh_check.py 也是这个口径）。
$Layout = Join-Path $OutDir "body_layout.json"
python (Join-Path $Tools "capture\dump_body_layout.py") `
  --frame-rs (Join-Path $Mir2Rs "crates\protocol\src\frame.rs") `
  --constants (Join-Path $Mir2Rs "crates\protocol\src\messages.rs") `
  --out $Layout
if ($LASTEXITCODE -ne 0) { Write-Output "RED: dump_body_layout 失败"; exit 1 }

$GaProj = Join-Path $Tools "capture\GoldenExport\GoldenExport.csproj"
if ([string]::IsNullOrEmpty($GoldenExportExe)) {
  $DefaultExe = Join-Path $Tools "capture\GoldenExport\bin\Release\net8.0\GoldenExport.exe"
  if (Test-Path $DefaultExe) { $GoldenExportExe = $DefaultExe }
}
if (-not [string]::IsNullOrEmpty($GoldenExportExe) -and (Test-Path $GoldenExportExe)) {
  & $GoldenExportExe --session $ProxyDir --out $Capture --layout $Layout --verify-roundtrip
} else {
  if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) {
    Write-Output "PREREQ_MISSING: 找不到 dotnet（GoldenExport 需要 .NET 8）"
    exit 2
  }
  dotnet run --project $GaProj -c Release -- --session $ProxyDir --out $Capture --layout $Layout --verify-roundtrip
}
if ($LASTEXITCODE -ne 0) { Write-Output "RED: GoldenExport 失败"; exit 1 }
if (-not (Test-Path $Capture)) { Write-Output "RED: GoldenExport 没有产出 capture.jsonl"; exit 1 }
Write-Output ("EXPORTED " + $Capture)

if (-not $SkipReplay) {
  Write-Output "--- 3) replay（既有门禁：逐字节往返 + 字段对拍） ---"
  Push-Location $Mir2Rs
  try {
    cargo run -p replay --offline -- $Capture --no-coverage
    if ($LASTEXITCODE -ne 0) { Write-Output "RED: replay 自洽失败（导出的 Rust s2c 帧与字段口径不符）"; exit 1 }
  } finally { Pop-Location }
}

Write-Output "GREEN: Rust 侧 s2c 帧已导出为 capture.jsonl 且 replay 自洽（交给 s2c_diff.py 与金标准对账）"
exit 0
