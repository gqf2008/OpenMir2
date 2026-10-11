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
  # 档位。**故意用 string 收**：`powershell -File` 下 `-LoadTiers 200,500,1000` 会先按
  # 「带千分位分隔符的整数」解析成 2005001000（逗号当分组符 ⇒ 50 万假人、整档 0% 并跑满 600s），
  # 用 string 才能拿到原样文本（`-File` ⇒ "200,500,1000"；
  # `-Command "& … -LoadTiers 200,500,1000"` ⇒ "200 500 1000"，两种都被下面的 `[,\s]+` 拆回数组）。
  # 2026-10-11 实测第三处同源坑，见 tests/loadgate/README.md。
  [string]$LoadTiers = "",
  [switch]$SkipFlow,
  [switch]$SkipBots,
  [switch]$ProvisionAccounts,
  [int]$ProvisionCount = 1000,
  [string]$ProvisionPrefix = "loadbot",
  [switch]$NoStartStack,
  [switch]$StopStackAtEnd,
  [switch]$RestartStackPerTier,
  [int]$HoldSec = 60,
  [int]$RampTimeoutSec = 0,        # 0 = 用 load_gate 默认（600s）
  [int]$Repeat = 1,                # 同一档位同命令连跑次数（owner 裁决 11dbde39 ①：3 连跑取最差）
  [int]$LoginThresholdPct = 0,     # 0 = 按档位取默认（tier>=1000→95，其余 100）
  [int]$LoginResendMs = 0,         # T-11 诊断对照（默认 0=关）
  [switch]$SelfTestRed
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
$StartAll = "E:\MirServer\start-all.ps1"
$GatePort = 7000

# 档位参数归一化 + 防线（与 tools/botload/load_gate.ps1 同源、同口径）：
#   ① 把 string 拆回 [int[]]：`,  空格 都算分隔符 ⇒ `-File`/`-Command` 两种传法都正确；
#   ② 单档数值超出合理上限即**响亮失败**——不再静默把「逗号拼接」当成一个 50 万人的档位跑掉。
$MaxPlausibleTier = 5000
$TierList = @()
if ($LoadTiers.Trim() -ne "") {
    $TierList = @($LoadTiers -split '[,\s]+' | Where-Object { $_ -ne "" } | ForEach-Object { [int]$_ })
}
if ($TierList.Count -eq 1 -and $TierList[0] -gt $MaxPlausibleTier) {
    throw ("档位 " + $TierList[0] + " 超出上限 " + $MaxPlausibleTier +
           "：疑似经 -File 传递时被逗号拼接（200,500,1000 -> 2005001000）。" +
           "请用 -LoadTiers '200,500,1000'（-File 下加引号）或 -Command ""& 'tools\e2e\stack_e2e.ps1' -LoadTiers 200,500,1000""。" +
           "见 tests/loadgate/README.md")
}
if (@($TierList | Where-Object { $_ -le 0 }).Count -gt 0) { throw "档位必须为正整数" }

$results = New-Object System.Collections.Generic.List[object]

function Invoke-Child([string]$ScriptPath, [string[]]$ChildArgs) {
    # 子进程的 stderr 在 $ErrorActionPreference="Stop" 下会变成父进程的终止错误
    # （本轮实测：开号脚本报一句错就把整条 stack_e2e 链打断，表现为"卡住/无输出"）。
    # 统一走这里：临时放宽为 Continue，只看退出码。
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $out = & powershell -ExecutionPolicy Bypass -File $ScriptPath @ChildArgs 2>&1
        return [pscustomobject]@{ rc = $LASTEXITCODE; out = @($out) }
    } finally { $ErrorActionPreference = $prev }
}

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

function Invoke-Stack([string[]]$Extra, [switch]$WaitDone) {
    # 坑（2026-10-11 实测，与 tools/botload/load_gate.ps1 的 Restart-Stack 同源）：
    # `start-all.ps1` 用 Start-Process 起七个服务进程；若把它的输出接成管道（`& powershell ... 2>&1`）
    # 再等它退出，服务进程会继承那个管道的写端 ⇒ 父进程退出后管道仍不关闭，调用方**永久挂住**
    # （表现：日志停在 “STACK start...”，栈其实已起、端口已通，但整条“一条命令”永远走不下去）。
    # 所以栈的启停一律 Start-Process + 重定向到文件，只按端口判就绪，绝不接管道。
    $out = Join-Path $env:TEMP "stack_e2e-startall.out.log"
    $err = Join-Path $env:TEMP "stack_e2e-startall.err.log"
    $args = @('-ExecutionPolicy', 'Bypass', '-File', $StartAll) + $Extra
    if ($WaitDone) {
        Start-Process -FilePath "powershell" -ArgumentList $args -WindowStyle Hidden `
            -RedirectStandardOutput $out -RedirectStandardError $err -Wait | Out-Null
    } else {
        Start-Process -FilePath "powershell" -ArgumentList $args -WindowStyle Hidden `
            -RedirectStandardOutput $out -RedirectStandardError $err | Out-Null
    }
}

# ---- 1. 栈生命周期 ----
if (-not $NoStartStack) {
    if (-not (Test-Path $StartAll)) { throw "找不到 $StartAll" }
    Write-Output "STACK stop (干净基线)..."
    Invoke-Stack @("-Stop") -WaitDone

    if ($ProvisionAccounts) {
        # 开号要先连库，而 start-all.ps1 -Stop 会把 MySQL 一起停掉 ⇒ 这里**只单独起 MySQL**
        # （不能先起整栈：LoginSrv 启动时把账号读进内存，之后再建的账号它看不到）
        if (-not (Test-Listen 3306)) {
            Write-Output "PROVISION 前置：单独起 MySQL（先不起 LoginSrv）..."
            Start-Process -FilePath "D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe" `
                -ArgumentList "--datadir=D:\mysql\data", "--port=3306", "--bind-address=127.0.0.1", "--console" `
                -RedirectStandardOutput "E:\MirServer\logs\MySQL.out.log" `
                -RedirectStandardError "E:\MirServer\logs\MySQL.err.log" -WindowStyle Hidden | Out-Null
            if (-not (Wait-Port 3306 40)) { Add-Result "provision" "RED" "MySQL 起不来"; exit 1 }
        }
        Write-Output ("PROVISION " + $ProvisionPrefix + " x " + $ProvisionCount + "（必须在起 LoginSrv 之前）...")
        $r = Invoke-Child (Join-Path $RepoRoot "tools\client\account_provision.ps1") @("-Prefix", $ProvisionPrefix, "-Count", "$ProvisionCount")
        $r.out | ForEach-Object { Write-Output ("  " + $_) }
        if ($r.rc -ne 0) { Add-Result "provision" "RED" ("开号失败 rc=" + $r.rc) } else { Add-Result "provision" "GREEN" "$ProvisionCount 个" }
    }

    Write-Output "STACK start..."
    Invoke-Stack @()
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
$rE2e = Invoke-Child (Join-Path $RepoRoot "tools\e2e\run_e2e.ps1") $e2eArgs
$rE2e.out | ForEach-Object { Write-Output $_ }
$e2eRc = $rE2e.rc
if ($SelfTestRed) {
    # 红检：run_e2e -SelfTestRed 要求一个不存在的阶段，必须 RED（rc != 0）；否则判据失效
    if ($e2eRc -ne 0) { Add-Result "run_e2e(red-check)" "GREEN" "按预期变红" }
    else { Add-Result "run_e2e(red-check)" "RED" "该红却绿 —— 判据失效" }
} else {
    Add-Result "run_e2e" ($(if ($e2eRc -eq 0) { "GREEN" } else { "RED" })) ("rc=" + $e2eRc)
}

# ---- 3. 压测档位（可选）----
if ($TierList.Count -gt 0) {
    # 档位只用 string 传：load_gate.ps1 的 `-Tiers` 是 string，`-File` 下 `-Tiers 200,500,1000`
    # 才不会退化成整数 2005001000（`200,500,1000` 当千分位）。多档连跑必须换干净栈：
    # 上一档结束时成百上千假人同时断连 = 网关抖动（实测过 → GameSvr 网关槽位 UserList 置空
    # → SetGateUserList NRE），所以这里自动带上 -RestartStackPerTier。
    $lgPath = Join-Path $RepoRoot "tools\botload\load_gate.ps1"
    $lgArgs = @('-ExecutionPolicy', 'Bypass', '-File', $lgPath,
                '-Tiers', ($TierList -join ","), '-HoldSec', "$HoldSec",
                '-Repeat', "$Repeat", '-LoginThresholdPct', "$LoginThresholdPct",
                '-LoginResendMs', "$LoginResendMs")
    if ($RampTimeoutSec -gt 0) { $lgArgs += @('-RampTimeoutSec', "$RampTimeoutSec") }
    if ($RestartStackPerTier -or $TierList.Count -gt 1 -or $Repeat -gt 1) { $lgArgs += '-RestartStackPerTier' }
    # 坑（C6 第 4 轮实测，第三处同源）：**不要**用 `& powershell ... 2>&1` 去"收集"子进程输出。
    # `2>&1` 建的是一个**管道**；load_gate 用 Start-Process 起的**长驻服务**（GameSvr/网关/MySQL）
    # 会继承这个管道的写端 ⇒ 父进程读管道要等 EOF，而 EOF 要等所有服务死，于是
    # **整条命令 summary 都已落盘、栈还在跑，却永远不退出**（104822/105943 与上一轮的 12496 都是它）。
    # 改成直接继承 stdout（不建管道）：`& powershell -File` 只等直接子进程退出，长驻孙进程不再挂住父进程。
    $prev = $ErrorActionPreference; $ErrorActionPreference = "Continue"
    try {
        & powershell @lgArgs
        $lgRc = $LASTEXITCODE
    } finally { $ErrorActionPreference = $prev }
    Add-Result "load-gate" ($(if ($lgRc -eq 0) { "GREEN" } else { "RED" })) ($TierList -join "/")
}

# ---- 4. 收尾 ----
if ($StopStackAtEnd) {
    Invoke-Stack @("-Stop") -WaitDone
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
