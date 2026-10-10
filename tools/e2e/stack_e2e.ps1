#requires -Version 5
<#
.SYNOPSIS
  stack_e2e.ps1 — 整栈 E2E 一条命令（C6）：起栈 → 工具自测+金标准+客户端流程+假人冒烟 → （可选）压测档位 → 收尾。

.DESCRIPTION
  把"跑一次完整验收"从"人记六步"变成一条命令。每步判 GREEN/RED/SKIP，任一 RED ⇒ 退出码 1。

  步骤：
    1. 前置：停栈（干净基线）→（可选）批量开号 → 起栈（`E:\MirServer\start-all.ps1`）→ 等端口
    2. `tools/e2e/run_e2e.ps1`：工具自测（proxy/timeline/dbsnap/worldsample/segmented/golden-fresh）+ 金标准对账
       + 真实客户端流程 E2E + 3 假人冒烟（可用 -SkipFlow/-SkipBots 关）
    3. （可选）`tools/botload/load_gate.ps1`：压测档位（-LoadTiers 200,500,1000）
    4. 收尾：产物路径清单；-StopStackAtEnd 则停栈

  开号必须在**起 LoginSrv 之前**（它启动时把账号读进内存，之后建的看不到）⇒ 本脚本把"停栈 → 开号 → 起栈"按序做。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/e2e/stack_e2e.ps1 -SkipFlow
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/e2e/stack_e2e.ps1 -ProvisionAccounts -ProvisionCount 1000 -LoadTiers 200,500,1000 -SkipFlow
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/e2e/stack_e2e.ps1 -SelfTestRed -SkipFlow    # 红检：要求一个不存在的阶段，必须 RED
#>
param(
  [int[]]$LoadTiers = @(),
  [switch]$SkipFlow,
  [switch]$SkipBots,
  [switch]$ProvisionAccounts,
  [int]$ProvisionCount = 1000,
  [string]$ProvisionPrefix = "loadbot",
  [switch]$NoStartStack,
  [switch]$StopStackAtEnd,
  [switch]$RestartStackPerTier,
  [int]$HoldSec = 60,
  [switch]$SelfTestRed
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
$StartAll = "E:\MirServer\start-all.ps1"
$GatePort = 7000
$results = New-Object System.Collections.Generic.List[object]

function Add-Result([string]$Name, [string]$State, [string]$Detail = "") {
    $results.Add([pscustomobject]@{ name = $Name; state = $State; detail = $Detail })
    Write-Output ("{0,-22} {1} {2}" -f $Name, $State, $Detail)
}

function Test-Listen([int]$Port) {
    return [bool](Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)
}

function Wait-Port([int]$Port, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        if (Test-Listen $Port) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return (Test-Listen $Port)
}

# ---- 1. 栈生命周期 ----
if (-not $NoStartStack) {
    if (-not (Test-Path $StartAll)) { throw "找不到 $StartAll" }
    Write-Output "STACK stop (干净基线)..."
    & powershell -ExecutionPolicy Bypass -File $StartAll -Stop 2>&1 | Out-Null

    if ($ProvisionAccounts) {
        Write-Output ("PROVISION " + $ProvisionPrefix + " x " + $ProvisionCount + "（必须在起 LoginSrv 之前）...")
        $out = & powershell -ExecutionPolicy Bypass -File (Join-Path $RepoRoot "tools\client\account_provision.ps1") `
            -Prefix $ProvisionPrefix -Count $ProvisionCount 2>&1
        $out | ForEach-Object { Write-Output ("  " + $_) }
        if ($LASTEXITCODE -ne 0) { Add-Result "provision" "RED" "开号失败" } else { Add-Result "provision" "GREEN" "$ProvisionCount 个" }
    }

    Write-Output "STACK start..."
    & powershell -ExecutionPolicy Bypass -File $StartAll 2>&1 | Out-Null
    $portOk = Wait-Port $GatePort 120
    Add-Result "stack-up" ($(if ($portOk) { "GREEN" } else { "RED" })) ("LoginGate:$GatePort")
    if (-not $portOk) {
        Write-Output "RED: 栈没起来，后续步骤没有意义"
        exit 1
    }
    Add-Result "mysql" ($(if (Test-Listen 3306) { "GREEN" } else { "RED" })) "3306"
} else {
    Add-Result "stack-up" "SKIP" "-NoStartStack（假定已在跑）"
}

# ---- 2. 工具自测 + 金标准 + 客户端流程 + 假人冒烟 ----
$e2eArgs = @()
if ($SkipFlow) { $e2eArgs += "-SkipFlow" }
if ($SkipBots) { $e2eArgs += "-SkipBots" }
if ($SelfTestRed) { $e2eArgs += "-SelfTestRed" }
& powershell -ExecutionPolicy Bypass -File (Join-Path $RepoRoot "tools\e2e\run_e2e.ps1") @e2eArgs
$e2eRc = $LASTEXITCODE
if ($SelfTestRed) {
    # 红检：run_e2e -SelfTestRed 要求一个不存在的阶段，必须 RED（rc != 0）；否则判据失效
    if ($e2eRc -ne 0) { Add-Result "run_e2e(red-check)" "GREEN" "按预期变红" }
    else { Add-Result "run_e2e(red-check)" "RED" "该红却绿 —— 判据失效" }
} else {
    Add-Result "run_e2e" ($(if ($e2eRc -eq 0) { "GREEN" } else { "RED" })) ("rc=" + $e2eRc)
}

# ---- 3. 压测档位（可选）----
if ($LoadTiers.Count -gt 0) {
    $lgArgs = @("-Tiers", ($LoadTiers -join ","), "-HoldSec", "$HoldSec")
    if ($RestartStackPerTier) { $lgArgs += "-RestartStackPerTier" }
    & powershell -ExecutionPolicy Bypass -File (Join-Path $RepoRoot "tools\botload\load_gate.ps1") @lgArgs
    $lgRc = $LASTEXITCODE
    Add-Result "load-gate" ($(if ($lgRc -eq 0) { "GREEN" } else { "RED" })) ($LoadTiers -join "/")
}

# ---- 4. 收尾 ----
if ($StopStackAtEnd) {
    & powershell -ExecutionPolicy Bypass -File $StartAll -Stop 2>&1 | Out-Null
    Add-Result "stack-stop" "GREEN" "-StopStackAtEnd"
}

$red = @($results | Where-Object { $_.state -eq "RED" })
Write-Output ""
Write-Output ("==== stack_e2e: " + @($results | Where-Object { $_.state -eq "GREEN" }).Count + " GREEN / " +
              $red.Count + " RED / " + @($results | Where-Object { $_.state -eq "SKIP" }).Count + " SKIP ====")
if ($SelfTestRed) {
    Write-Output "SELFTEST_RED: 见上（红检必须变红才算通过）"
    if ($e2eRc -ne 0) { exit 0 } else { exit 1 }
}
if ($red.Count -gt 0) { exit 1 }
exit 0
