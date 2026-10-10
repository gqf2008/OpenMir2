#requires -Version 5
<#
.SYNOPSIS
  run_dispatch_regression.ps1 —— 脚本派发回归门禁（whitelist B-8 / T-4）。

.DESCRIPTION
  用 tools/oracle/cond-crash-probe 直接跑生产类型（ConditionProcessingSys / ExecutionProcessingSystem），
  对 dispatch-regression-120.tsv 这台"旧位移口径下必抛"的语料做两条断言：
    ① 现行口径下**一行都不许再抛** IndexOutOfRangeException（修复的验收线）；
    ② 旧位移口径下**仍然抛**（语料仍有牙齿）+ 语料 sha256 与 expected.json 一致（防误改/防放宽）。
  另跑轴审计（两个枚举「字段序号 == 枚举值」）—— 那是"改解析器是唯一有效落点"的前提。

  红检：-SelfTestRed 用 --force-legacy（把现行口径当成旧位移评估，不碰源码）跑同一套判据，必须判红。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/script-dispatch/run_dispatch_regression.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/script-dispatch/run_dispatch_regression.ps1 -SelfTestRed
#>
param(
  [string]$RepoRoot = "",
  [switch]$SelfTestRed,
  [switch]$Quiet,
  # 协调者已裁定 Rust 侧同批翻转（设计文档 §17）。翻转前本开关故意默认关闭（否则门禁会因"别人还没做"
  # 而红）；B 线翻转完成后把它打开，Rust 口径就成了硬判据。
  [switch]$RequireRustFlipped
)

$ErrorActionPreference = "Stop"
if ($RepoRoot -eq "") {
  # script-dispatch → parity → tests → mir2-rs → 仓库根，共 4 层
  $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")).Path
}
$Corpus = Join-Path $PSScriptRoot 'dispatch-regression-120.tsv'
$Expected = Join-Path $PSScriptRoot 'expected.json'
$ProbeProj = Join-Path $RepoRoot 'tools\oracle\cond-crash-probe\CondCrashProbe.csproj'
$ProbeExe = Join-Path $RepoRoot 'tools\oracle\cond-crash-probe\bin\Release\CondCrashProbe.exe'

if (-not (Test-Path $Corpus)) { Write-Output "RED  语料不存在: $Corpus"; exit 1 }
if (-not (Test-Path $Expected)) { Write-Output "RED  期望值文件不存在: $Expected"; exit 1 }
$exp = Get-Content -LiteralPath $Expected -Raw | ConvertFrom-Json

Write-Output "== 脚本派发回归门禁（B-8 / T-4）"
Write-Output "   语料: $Corpus"

# 1) 语料哈希（防误改：改了语料就必须同步改 expected.json，并在提交说明里写清原因）
$hash = (Get-FileHash -LiteralPath $Corpus -Algorithm SHA256).Hash.ToLower()
$hashOk = ($hash -eq $exp.corpus_sha256.ToLower())
Write-Output ("{0}  corpus-sha256  ({1}，期望 {2})" -f $(if ($hashOk) { "PASS" } else { "FAIL" }), $hash.Substring(0, 16), $exp.corpus_sha256.Substring(0, 16))

# 2) 构建探针（增量；失败要把编译输出打出来，别让"构建失败"变成一个没有下文的状态）
$buildOut = & dotnet build $ProbeProj -c Release -v q --nologo 2>&1
if ($LASTEXITCODE -ne 0) {
  Write-Output "RED  探针构建失败（exit $LASTEXITCODE）："
  $buildOut | Where-Object { $_ -match 'error|错误' } | Select-Object -First 8 | ForEach-Object { Write-Output ("      " + $_) }
  exit 1
}

# 3) 跑门禁（-SelfTestRed 时用 --force-legacy 做阳性对照）
$gateArgs = @('--gate', $Corpus)
if ($SelfTestRed) { $gateArgs += '--force-legacy' }
$out = & $ProbeExe @gateArgs 2>&1
$gateRc = $LASTEXITCODE
if (-not $Quiet) { $out | ForEach-Object { Write-Output ("      " + $_) } }

# 4) Rust 侧当前口径（默认只是信息行：裁定要求两边一起翻，但"Rust 还没翻"不该让本门禁变红；
#    翻转完成后用 -RequireRustFlipped 把它变成硬判据）
$rustFlipped = $null
$rustParser = Join-Path $RepoRoot 'mir2-rs\crates\script\src\parser.rs'
if (Test-Path $rustParser) {
  $rustFlipped = -not (Select-String -LiteralPath $rustParser -Pattern 'field_index\s*-\s*1' -Quiet)
  if ($rustFlipped) {
    Write-Output "   [信息] Rust 已翻转（field_index）⇒ 请把本行改成硬判据（-RequireRustFlipped）并同步 T-4/README"
  } else {
    Write-Output "   [信息] Rust 仍是旧位移（field_index - 1）：协调者裁定要求同批翻转（设计文档 §17），**待 B 线执行**"
  }
} else {
  Write-Output "   [信息] 找不到 $rustParser（跳过 Rust 口径检查）"
}
$rustOk = ($rustFlipped -ne $false) -or (-not $RequireRustFlipped)

# 5) 汇总
if ($SelfTestRed) {
  if ($gateRc -ne 0) { Write-Output "== SELFTEST_RED GREEN：--force-legacy 下门禁判红（阳性对照成立）"; exit 0 }
  Write-Output "== SELFTEST_RED RED：阳性对照没判红 —— 门禁失效！"
  exit 1
}
if ($hashOk -and $gateRc -eq 0 -and $rustOk) { Write-Output "== gate GREEN（语料哈希一致 + 5 条判据全过$(if ($RequireRustFlipped) { ' + Rust 已翻转' } else { '' })）"; exit 0 }
Write-Output "== gate RED（hash_ok=$hashOk, gate_rc=$gateRc, rust_ok=$rustOk）"
exit 1
