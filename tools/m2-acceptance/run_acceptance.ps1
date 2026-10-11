#requires -Version 5
<#
.SYNOPSIS
  run_acceptance.ps1 — M2 验收①②③ 的**一条命令**运行手册（harness）。

.DESCRIPTION
  把「最终验收」从一次人工探险变成一条命令：先跑**前置门**（preflight），再依次跑四项机判，汇总 GREEN/RED/PREREQ，
  任一 PREREQ 或 RED ⇒ 非零退出（**没有栈时明确报「前置未满足」，绝不静默通过**）。

    ⓪ 前置门                  preflight.ps1          （python/dotnet/cargo + 仓库输入 + 金标准身份/schema；缺 ⇒ PREREQ，坏 ⇒ RED）
    ① 端口所有权实拍        check_ports.ps1        （5000/7000/7100/7200 的 PID + 命令行均须为 Rust）
    ③a 同序列出包 diff 导出   export_s2c.ps1        （mir2_proxy dump → segment_frames → GoldenExport → replay）
    ③b 同序列出包 diff 对账   s2c_diff.py           （Rust capture.jsonl vs M0 金标准）
    ② 真客户端 7 张截图       check_screenshots.ps1
    ③c 三路径 DB 字段级 diff  check_db_paths.ps1    （小退/掉线/服务关闭；scoped_diff）

  **本脚本不启动任何服务**（不占端口、不起客户端）——它只对「已经跑着的栈/已经采好的产物」取判据。
  缺栈时每一步都会打印 PREREQ_MISSING 并以对应退出码返回，本脚本据此判整体不通过。

.EXAMPLE
  # 完整验收（栈已起、dump/截图/DB 快照已采）
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/run_acceptance.ps1 `
    -ProxyDir E:\tmp\m2acc\run1\proxy -ShotsDir E:\tmp\m2acc\run1\shots `
    -CsharpDbDir E:\tmp\m2acc\csharp -RustDbDir E:\tmp\m2acc\rust
.EXAMPLE
  # 无栈自检：所有步骤都必须报前置未满足，整体退出码 != 0
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/run_acceptance.ps1 -SelfCheckNoStack
#>
param(
  [string]$ProxyDir = "",
  [string]$ShotsDir = "",
  [string]$CsharpDbDir = "",
  [string]$RustDbDir = "",
  [string]$Golden = "",
  [int[]]$Ports = @(5000, 7000, 7100, 7200),
  [switch]$SkipPorts, [switch]$SkipS2c, [switch]$SkipShots, [switch]$SkipDb,
  [switch]$SkipPreflight,
  [switch]$SelfCheckNoStack
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
if ([string]::IsNullOrEmpty($Golden)) {
  $Golden = Join-Path $RepoRoot "mir2-rs\tests\golden\c-line-baseline-c3-20261010.jsonl"
}
$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$EvidenceDir = Join-Path "E:\tmp\m2-acceptance" ("run-$Stamp")
New-Item -ItemType Directory -Force -Path $EvidenceDir | Out-Null

if ($SelfCheckNoStack) {
  Write-Output "SELFTEST_NO_STACK: 无栈自检——预期每一步都 PREREQ_MISSING，整体非 0 退出"
  # 用一组保证无监听的端口，避免外部已起的栈（如 E:\MirServer）干扰「前置缺失」这条断言
  $Ports = @(17990, 17991, 17992, 17993)
  $ProxyDir = Join-Path $EvidenceDir "nonexistent-proxy"
  $ShotsDir = Join-Path $EvidenceDir "nonexistent-shots"
  $CsharpDbDir = Join-Path $EvidenceDir "nonexistent-csharp"
  $RustDbDir = Join-Path $EvidenceDir "nonexistent-rust"
}

$results = New-Object System.Collections.Generic.List[object]
$script:LastRc = 0
function Invoke-Step([string]$Name, [string]$ScriptPath, [string[]]$ChildArgs) {
  if (-not (Test-Path $ScriptPath)) {
    $results.Add([pscustomobject]@{ name = $Name; state = "RED"; rc = 127; detail = "缺脚本 $ScriptPath" })
    Write-Output ("{0,-22} RED (缺脚本)" -f $Name)
    $script:LastRc = 127
    return
  }
  $prev = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    & powershell -ExecutionPolicy Bypass -File $ScriptPath @ChildArgs
    $script:LastRc = $LASTEXITCODE
  } finally { $ErrorActionPreference = $prev }
  $rc = $script:LastRc
  $state = if ($rc -eq 0) { "GREEN" } elseif ($rc -eq 2) { "PREREQ" } else { "RED" }
  $results.Add([pscustomobject]@{ name = $Name; state = $state; rc = $rc; detail = "$ScriptPath" })
  Write-Output ("{0,-22} {1} (rc={2})" -f $Name, $state, $rc)
}

Write-Output ("M2 验收运行手册  evidence=" + $EvidenceDir)

if (-not $SkipPreflight) {
  Invoke-Step "preflight(⓪)" (Join-Path $Here "preflight.ps1") @("-GoldenPath", $Golden, "-OutFile", (Join-Path $EvidenceDir "preflight.json"))
}

if (-not $SkipPorts) {
  Invoke-Step "ports(①)" (Join-Path $Here "check_ports.ps1") @("-OutFile", (Join-Path $EvidenceDir "ports.json"), "-PortsCsv", ($Ports -join ","))
}

if (-not $SkipS2c) {
  $s2cDir = Join-Path $EvidenceDir "s2c"
  Invoke-Step "s2c-export(③a)" (Join-Path $Here "export_s2c.ps1") @("-ProxyDir", $ProxyDir, "-OutDir", $s2cDir)
  $rcExport = $script:LastRc
  if ($rcExport -eq 0) {
    $capture = Join-Path $s2cDir "capture.jsonl"
    python (Join-Path $Here "s2c_diff.py") `
      --rust $capture --golden $Golden --out (Join-Path $EvidenceDir "s2c-diff.json")
    $rcDiff = $LASTEXITCODE
    $state = if ($rcDiff -eq 0) { "GREEN" } elseif ($rcDiff -eq 2) { "PREREQ" } else { "RED" }
    $results.Add([pscustomobject]@{ name = "s2c-diff(③b)"; state = $state; rc = $rcDiff; detail = "s2c_diff.py" })
    Write-Output ("{0,-22} {1} (rc={2})" -f "s2c-diff(③b)", $state, $rcDiff)
  } else {
    $results.Add([pscustomobject]@{ name = "s2c-diff(③b)"; state = "PREREQ"; rc = 2; detail = "导出未成功，跳过对账" })
    Write-Output ("{0,-22} SKIP (导出未成功)" -f "s2c-diff(③b)")
  }
}

if (-not $SkipShots) {
  Invoke-Step "shots(②)" (Join-Path $Here "check_screenshots.ps1") @("-ShotsDir", $ShotsDir, "-OutFile", (Join-Path $EvidenceDir "shots.json"))
}

if (-not $SkipDb) {
  Invoke-Step "db-paths(③c)" (Join-Path $Here "check_db_paths.ps1") @("-Mode", "diff", "-CsharpDir", $CsharpDbDir, "-RustDir", $RustDbDir, "-OutFile", (Join-Path $EvidenceDir "db-paths.json"))
}

$green = @($results | Where-Object { $_.state -eq "GREEN" }).Count
$red = @($results | Where-Object { $_.state -eq "RED" }).Count
$prereq = @($results | Where-Object { $_.state -eq "PREREQ" }).Count
$report = [ordered]@{
  tool = "m2-acceptance/run_acceptance.ps1"
  ts = (Get-Date).ToString("o")
  self_check_no_stack = [bool]$SelfCheckNoStack
  evidence_dir = $EvidenceDir
  green = $green; red = $red; prereq = $prereq
  results = $results
  pass = ($red -eq 0 -and $prereq -eq 0 -and $results.Count -ge 4)
}
$reportPath = Join-Path $EvidenceDir "acceptance.json"
$report | ConvertTo-Json -Depth 6 | Set-Content -Path $reportPath -Encoding utf8
Write-Output ("ACCEPTANCE_REPORT " + $reportPath)
Write-Output ("SUMMARY green={0} red={1} prereq={2}" -f $green, $red, $prereq)

if ($report.pass) { Write-Output "GREEN: M2 验收①②③ 全部机判通过"; exit 0 }
if ($red -gt 0) { Write-Output "RED: 有步骤判红，验收不通过"; exit 1 }
Write-Output "PREREQ_MISSING: 有步骤的前置未满足（栈/产物未就位），验收不成立"; exit 2
