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
