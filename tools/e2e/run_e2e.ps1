#requires -Version 5
<#
.SYNOPSIS
  run_e2e.ps1 — E2E 回归套件（C 线工具 ⑤）。

.DESCRIPTION
  在现有 C# 栈上串起端到端检查（每项判 GREEN/RED，任何 RED → 退出码 1）：
    1. proxy-selftest    抓包代理三项自测（透传保真/dump 复原/篡改必红）
    2. timeline-selftest 时间线对拍自测
    3. dbsnap-selftest   DB 快照/比对自测（含篡改红检）
    3b. worldsample-selftest 世界态采样自测（确定性 + 改坐标必红）
    3c. segmented-check  Segmented 段边界自检（段回编=原字节；seg_len 改错必红）
    4. golden-verify     最新金标准完整性（tests/golden/LATEST 指向的会话）
    4b. golden-fresh     入库金标准新鲜度+登记对账（重导==入库件；登记 sha 一致）
    5. flow-e2e          真实客户端全流程：阶段序列完整 + 截图非空 + 服务日志无新 ERR 暴涨
    6. bot-smoke         3 个假人登录冒烟（-SkipBots 跳过；完整 200 并发用 run_bots.ps1）

  红检：-SelfTestRed 用「要求一个不存在的阶段」跑 flow 断言，必须判 RED。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_e2e.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_e2e.ps1 -SkipFlow -SkipBots   # 只跑纯工具自测
#>
param(
  [switch]$SkipFlow,
  [switch]$SkipBots,
  [switch]$SelfTestRed,
  [switch]$FailOnSkip          # 严格模式：SKIP（前置不满足）也算失败
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
$CaptureDir = Join-Path $RepoRoot "tools\capture"
$results = @()

function Add-Result([string]$Name, [bool]$Ok, [string]$Detail = "", [string]$State = "") {
  # State 可显式给 "SKIP"（前置不满足）；默认按 Ok 判 GREEN/RED。
  # SKIP 在汇总里单独计数（不许静默当 GREEN），-FailOnSkip 时折算成 RED。
  if ($State -eq "") { if ($Ok) { $State = "GREEN" } else { $State = "RED" } }
  $script:results += [pscustomobject]@{ check = $Name; ok = $Ok; state = $State; detail = $Detail }
  # 注：Windows PowerShell 5.1 没有三元运算符，别用 `$cond ? a : b`
  $suffix = ""
  if ($Detail) { $suffix = "  ($Detail)" }
  Write-Output ($State + "  " + $Name + $suffix)
}

function Test-FlowStages([string]$StagesFile, [string[]]$Required) {
  # 阶段序列断言：必需阶段必须按序出现；返回 $true/$false
  if (-not (Test-Path $StagesFile)) { return $false }
  $stages = @()
  foreach ($line in (Get-Content $StagesFile)) {
    try { $stages += ($line | ConvertFrom-Json).stage } catch { }
  }
  $pos = 0
  foreach ($req in $Required) {
    $found = $false
    for ($i = $pos; $i -lt $stages.Count; $i++) {
      if ($stages[$i] -eq $req) { $found = $true; $pos = $i + 1; break }
    }
    if (-not $found) { Write-Host "  缺阶段(或乱序): $req"; return $false }   # Write-Host：别污染返回值
  }
  return $true
}

function Test-NotBlackScreen([string]$PngPath, [double]$MinNonBlackRatio = 0.01) {
  # 进世界成功必须"画出来了"：全黑画面 = 没进世界（本轮踩过的实际症状）。
  # 抽样统计非黑像素占比，低于阈值即判黑屏——这条让 flow 门禁可证伪。
  if (-not (Test-Path $PngPath)) { return $false }
  Add-Type -AssemblyName System.Drawing -ErrorAction SilentlyContinue
  try {
    $bmp = New-Object System.Drawing.Bitmap($PngPath)
    $w = $bmp.Width; $h = $bmp.Height
    $nonBlack = 0; $total = 0
    for ($y = 40; $y -lt $h; $y += 8) {
      for ($x = 4; $x -lt $w; $x += 8) {
        $c = $bmp.GetPixel($x, $y)
        $total++
        if (($c.R + $c.G + $c.B) -gt 30) { $nonBlack++ }
      }
    }
    $bmp.Dispose()
    if ($total -eq 0) { return $false }
    $ratio = $nonBlack / $total
    Write-Host ("  画面非黑占比 " + [math]::Round($ratio * 100, 2) + "% (" + $PngPath + ")")   # 必须 Write-Host：Write-Output 会混进返回值
    return ($ratio -ge $MinNonBlackRatio)
  } catch {
    Write-Host ("  画面取样失败: " + $_.Exception.Message)
    return $false
  }
}


python (Join-Path $CaptureDir "selftest_proxy.py")
Add-Result "proxy-selftest" ($LASTEXITCODE -eq 0)

python (Join-Path $RepoRoot "tools\logdiff\timeline.py") selftest
Add-Result "timeline-selftest" ($LASTEXITCODE -eq 0)

# dbsnap 自测要连实库：MySQL 没起时判 SKIP（前置不满足）而不是 RED ——
# 免得"环境没起"被读成"代码坏了"；-FailOnSkip 严格模式下 SKIP 也算失败。
$mysqlUp = [bool](Get-NetTCPConnection -State Listen -LocalPort 3306 -ErrorAction SilentlyContinue)
if (-not $mysqlUp) {
    Write-Host "SKIP dbsnap-selftest (前置: MySQL 3306 未监听)"
    if ($FailOnSkip) { Add-Result "dbsnap-selftest" $false "MySQL 未起（-FailOnSkip 严格模式）" }
    else { Add-Result "dbsnap-selftest" $true "MySQL 未起" "SKIP" }
} else {
    python (Join-Path $RepoRoot "tools\dbsnap\dbsnap.py") selftest
    Add-Result "dbsnap-selftest" ($LASTEXITCODE -eq 0)
}

python (Join-Path $RepoRoot "tools\worldsample\world_sampler.py") selftest
Add-Result "worldsample-selftest" ($LASTEXITCODE -eq 0)

# Segmented 段边界自检（C4）：用已入库的 C3 裸 dump 的真实帧 + A 线冻结契约。
# 需要 GoldenExport 可执行体（缺则现构建）；改错 seg_len 必须 rc=4（改错必红）。
$gaProj = Join-Path $RepoRoot "tools\capture\GoldenExport\GoldenExport.csproj"
$gaExe  = Join-Path $RepoRoot "tools\capture\GoldenExport\bin\Debug\net8.0\GoldenExport.exe"
if (-not (Test-Path $gaExe)) {
  dotnet build $gaProj -v q --nologo 2>&1 | Out-Null
}
python (Join-Path $RepoRoot "tools\capture\segmented_check.py")
Add-Result "segmented-check" ($LASTEXITCODE -eq 0)

# 入库金标准"新鲜度 + 登记"对账（C4 续）：每份入库件都必须等于**当前**导出器从来源裸 dump 的重导结果，
# 且 README 登记行的 sha256 等于实际文件 —— "旧 artifact + 新下游"这类假红/假绿在这里自动暴露。
python (Join-Path $RepoRoot "tools\capture\golden_fresh_check.py")
Add-Result "golden-fresh" ($LASTEXITCODE -eq 0)

# 同上的红检（改坏必红）：入库件与当前口径不符 / 只改登记行 / registry 缺项 —— 三类都必须在临时目录里判红
python (Join-Path $RepoRoot "tools\capture\golden_fresh_check.py") --selftest
Add-Result "golden-fresh-red" ($LASTEXITCODE -eq 0)

# ---- 4. 金标准完整性 ----
$latestFile = Join-Path $RepoRoot "tests\golden\LATEST"
if (Test-Path $latestFile) {
  $latest = (Get-Content $latestFile -Raw).Trim()
  if (-not [System.IO.Path]::IsPathRooted($latest)) { $latest = Join-Path $RepoRoot $latest }
  python (Join-Path $CaptureDir "verify_golden.py") --golden $latest
  Add-Result "golden-verify" ($LASTEXITCODE -eq 0) $latest
} else {
  Add-Result "golden-verify" $false "tests/golden/LATEST 不存在（先跑 capture_baseline.ps1）"
}

# ---- 5. 真实客户端流程 ----
if (-not $SkipFlow) {
  $flowOut = Join-Path $RepoRoot ("tests\e2e\flow-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
  $required = @("start","login_id","login_pwd","login_submit","charsel","enter_game","ingame",
                "notice_dismiss","logout_alt_x","logout_confirm","back_charsel","reenter","reentered","done")
  if ($SelfTestRed) { $required += "stage_that_does_not_exist" }   # 红检：必缺

  # 记录服务端 err 日志水位，跑完对比是否暴涨
  $errMark = @{}
  foreach ($f in (Get-ChildItem "E:\MirServer\logs" -Filter *.err.log -ErrorAction SilentlyContinue)) {
    $errMark[$f.FullName] = $f.Length
  }
  & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $RepoRoot "tools\client\mir_flow.ps1") `
    -OutDir $flowOut | Out-Null
  $flowOk = ($LASTEXITCODE -eq 0)
  $stagesOk = Test-FlowStages (Join-Path $flowOut "stages.ndjson") $required
  $shots = Get-ChildItem $flowOut -Filter *.png -ErrorAction SilentlyContinue
  $shotsOk = ($shots.Count -ge 5) -and (($shots | Measure-Object Length -Minimum).Minimum -gt 10000)
  $errGrowth = 0
  foreach ($f in $errMark.Keys) {
    if (Test-Path $f) { $errGrowth += ((Get-Item $f).Length - $errMark[$f]) }
  }
  $ok = $flowOk -and $stagesOk -and $shotsOk
  Add-Result "flow-e2e" $ok ("stages=$stagesOk shots=$($shots.Count) errlog+${errGrowth}B")
  # 单独一条：进世界必须真画出画面（黑屏 = 没进世界，服务端侧问题）
  $renderOk = Test-NotBlackScreen (Join-Path $flowOut "05b_after_notice.png")
  Add-Result "flow-world-render" $renderOk "05b_after_notice.png 非黑占比 >=1%"
}

# ---- 6. 假人冒烟 ----
if (-not $SkipBots) {
  # 就绪：整栈七进程里 LoginGate:7000 最早起，登录链路（LoginSrv 5500/SelGate 7100）可能还没跟上，
  # 冷栈第一跑偶发 2/3（2026-10-11 实测）⇒ 先等所有 oracle 端口都 listen，再跑冒烟。
  $readyPorts = @(3306,5000,5100,5500,5600,5700,6000,7000,7100,7200)
  $deadline = (Get-Date).AddSeconds(120)
  while ((Get-Date) -lt $deadline) {
    $missing = @($readyPorts | Where-Object { -not (Get-NetTCPConnection -State Listen -LocalPort $_ -ErrorAction SilentlyContinue) })
    if ($missing.Count -eq 0) { break }
    Start-Sleep -Seconds 2
  }
  if ($missing.Count -gt 0) { Write-Output ("  等栈就绪：仍缺端口 " + ($missing -join ",")) }
  # 冒烟是门禁不是正确性 oracle：冷栈/网关抖动会偶发个别假人没登上，允许重试一次（重试仍失败才算红）。
  $smokeOk = $false
  for ($attempt = 1; $attempt -le 2 -and -not $smokeOk; $attempt++) {
    if ($attempt -gt 1) { Write-Output "  bot-smoke 重试（首次可能赶在栈完全就绪前）" }
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $RepoRoot "tools\botload\run_bots.ps1") `
      -Count 3 -Prefix "e2esmoke" -NewAccount -TimeoutSec 180
    $smokeOk = ($LASTEXITCODE -eq 0)
  }
  Add-Result "bot-smoke" $smokeOk ($(if ($smokeOk) { "3/3" } else { "重试 2 次仍失败" }))
}

# ---- 汇总 ----
$redChecks = @($results | Where-Object { $_.state -eq "RED" })
$skipChecks = @($results | Where-Object { $_.state -eq "SKIP" })
if ($FailOnSkip -and $skipChecks.Count -gt 0) {
  $redChecks = @($redChecks + $skipChecks)
  $skipChecks = @()
  Write-Output ("严格模式(-FailOnSkip)：把 " + $results.Count + " 项里的 SKIP 折算成 RED")
}
$red = $redChecks.Count
Write-Output ("==== E2E: " + ($results.Count - $red - $skipChecks.Count) + " GREEN / " + $red +
              " RED / " + $skipChecks.Count + " SKIP ====")
if ($SelfTestRed) {
  # 红检模式：flow-e2e 必须红才算自测通过
  $flow = $results | Where-Object { $_.check -eq "flow-e2e" } | Select-Object -First 1
  if ($flow -and -not $flow.ok) { Write-Output "SELFTEST_RED GREEN: 不存在的阶段被判 RED（改坏必红成立）"; exit 0 }
  Write-Output "SELFTEST_RED RED: 不存在的阶段竟然判绿——断言失效！"
  exit 1
}
exit $(if ($red -eq 0) { 0 } else { 1 })
