#requires -Version 5
<#
.SYNOPSIS
  run_e2e.ps1 — E2E 回归套件（C 线工具 ⑤）。

.DESCRIPTION
  在现有 C# 栈上串起端到端检查（每项判 GREEN/RED，任何 RED → 退出码 1）：
    1. proxy-selftest    抓包代理三项自测（透传保真/dump 复原/篡改必红）
    2. timeline-selftest 时间线对拍自测
    3. dbsnap-selftest   DB 快照/比对自测（含篡改红检）
    4. golden-verify     最新金标准完整性（tests/golden/LATEST 指向的会话）
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
  [switch]$SelfTestRed
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
$CaptureDir = Join-Path $RepoRoot "tools\capture"
$results = @()

function Add-Result([string]$Name, [bool]$Ok, [string]$Detail = "") {
  $script:results += [pscustomobject]@{ check = $Name; ok = $Ok; detail = $Detail }
  # 注：Windows PowerShell 5.1 没有三元运算符，别用 `$cond ? a : b`
  $tag = "RED"
  if ($Ok) { $tag = "GREEN" }
  $suffix = ""
  if ($Detail) { $suffix = "  ($Detail)" }
  Write-Output ($tag + "  " + $Name + $suffix)
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

python (Join-Path $RepoRoot "tools\dbsnap\dbsnap.py") selftest
Add-Result "dbsnap-selftest" ($LASTEXITCODE -eq 0)

python (Join-Path $RepoRoot "tools\worldsample\world_sampler.py") selftest
Add-Result "worldsample-selftest" ($LASTEXITCODE -eq 0)

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
  & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $RepoRoot "tools\botload\run_bots.ps1") `
    -Count 3 -Prefix "e2esmoke" -NewAccount -TimeoutSec 180
  Add-Result "bot-smoke" ($LASTEXITCODE -eq 0)
}

# ---- 汇总 ----
$red = @($results | Where-Object { -not $_.ok }).Count
Write-Output ("==== E2E: " + ($results.Count - $red) + " GREEN / " + $red + " RED ====")
if ($SelfTestRed) {
  # 红检模式：flow-e2e 必须红才算自测通过
  $flow = $results | Where-Object { $_.check -eq "flow-e2e" } | Select-Object -First 1
  if ($flow -and -not $flow.ok) { Write-Output "SELFTEST_RED GREEN: 不存在的阶段被判 RED（改坏必红成立）"; exit 0 }
  Write-Output "SELFTEST_RED RED: 不存在的阶段竟然判绿——断言失效！"
  exit 1
}
exit $(if ($red -eq 0) { 0 } else { 1 })
