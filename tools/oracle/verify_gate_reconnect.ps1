#requires -Version 5
<#
.SYNOPSIS
  verify_gate_reconnect.ps1 — S1 缺陷① 的**实机**门禁：网关连接抖动后，真实客户端必须能进世界。

.DESCRIPTION
  被测行为（改前/改后跑同一份）：
    GameGate 与 GameSvr 之间的连接断一次再重连之后，玩家进世界必须仍然成功 ——
    改前：GameSvr 的网关槽位按 TCP accept 序绑定，重连后落到别的槽位，
          世界侧（playObject.GateIdx）寻址到已断开的旧槽位 ⇒ 进世界数据被静默丢弃
          （客户端黑屏）且 WorldServer.ProcessHumans 每 tick 在 SetGateUserList 上抛 NRE。
    改后：槽位随连接释放/回收，世界侧始终寻址到活连接。

  抖动注入方式：**只重启 GameGate**（不动 GameSvr/MySQL/LoginSrv/DBSrv）。
  GameGate 起来后会自己重连 GameSvr，GameSvr 侧就是一次"旧连接断开 + 新连接建立"。

  判据（全部要过）：
    1. 抖动后流程走到 `ingame`（必需阶段齐全）且 `05b_after_notice.png` 非黑占比 >= 1%（进世界真画出来了）；
    2. GameSvr 日志在抖动之后**没有**新增 `SetGateUserList` 的 NullReferenceException；
    3. 抖动后 GameSvr 日志出现新的"游戏网关[...]已打开..."（证明确实重连过）。

  说明：只判"进世界"这一段 —— `mir_flow.ps1` 后面的小退段有十来次固定等待，与本门禁判据无关；
  判据产物（`stages.ndjson` 里的 `ingame` + `05b_after_notice.png`）一齐就提前结束流程进程，
  因此**不**检查流程自身的退出码。

  红检：`-SelfTestRed` 断言一个不存在的阶段，flow 判据必须 RED。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/oracle/verify_gate_reconnect.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/oracle/verify_gate_reconnect.ps1 -SkipBaseline -SkipGateBounce
#>
param(
  [string]$ServerRoot = "E:\MirServer",
  [string]$ClientRunDir = "D:\MirClient-run",
  [string]$RepoRoot = "",
  [string]$OutDir = "",
  [switch]$SkipBaseline,     # 跳过"抖动前"的基线流程（只为省时间；判据不打折）
  [switch]$SkipGateBounce,   # 不动网关（用于只验基线）
  [switch]$SelfTestRed,
  [string[]]$FlowExtraArgs = @(),   # 透传给 tools/client/mir_flow.ps1（例：-SkipReenter 缩短窗口）
  [int]$MaxFlowAttempts = 4,        # 别的线同时跑客户端会互相杀进程；不足就重试（留日志）
  [int]$FlowRetryDelaySec = 20,
  [int]$GateReadyTimeoutSec = 60,
  [int]$FlowTimeoutSec = 180      # 只等"进世界"这一段；产物一齐就提前收工
)

$ErrorActionPreference = "Stop"
if ($RepoRoot -eq "") { $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path }
if ($OutDir -eq "") { $OutDir = Join-Path $RepoRoot ("tests\oracle\gate-reconnect-" + (Get-Date -Format "yyyyMMdd-HHmmss")) }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$LogDir = Join-Path $ServerRoot 'logs'
$GameSvrLog = Join-Path $LogDir 'GameSvr.out.log'
$GateDir = Join-Path $ServerRoot 'RunGate'
$GateExe = Join-Path $GateDir 'GameGate.exe'

# 起长命服务一律走 detached 帮手，别在 PowerShell 层重定向（否则子进程占住本脚本的 stdout 管道，
# 调用方读不到 EOF：脚本跑完了却不回、会话不退）。见 detached.ps1 的实测三档。
. (Join-Path $PSScriptRoot 'detached.ps1')

$results = @()
function Add-Result([string]$Name, [bool]$Ok, [string]$Detail = "") {
  $script:results += [pscustomobject]@{ check = $Name; ok = $Ok; detail = $Detail }
  $tag = 'RED'
  if ($Ok) { $tag = 'GREEN' }
  Write-Output ("$tag  $Name  ($Detail)")
}

function Get-ListeningPorts {
  # 探活一律走 netstat 文本：往网关上 connect 一次就会造出幻影用户（工具侧既有结论）
  $text = (netstat -ano | Out-String)
  $ports = @()
  foreach ($line in ($text -split "`r?`n")) {
    if ($line -match 'LISTENING' -and $line -match ':(\d+)\s') { $ports += [int]$Matches[1] }
  }
  return $ports
}

function Test-NotBlackScreen([string]$PngPath, [double]$MinRatio = 0.01) {
  if (-not (Test-Path $PngPath)) { return $false }
  Add-Type -AssemblyName System.Drawing -ErrorAction SilentlyContinue
  try {
    $bmp = New-Object System.Drawing.Bitmap($PngPath)
    $nonBlack = 0; $total = 0
    for ($y = 40; $y -lt $bmp.Height; $y += 8) {
      for ($x = 4; $x -lt $bmp.Width; $x += 8) {
        $c = $bmp.GetPixel($x, $y)
        $total++
        if (($c.R + $c.G + $c.B) -gt 30) { $nonBlack++ }
      }
    }
    $bmp.Dispose()
    if ($total -eq 0) { return $false }
    $ratio = $nonBlack / $total
    # 必须 Write-Host：Write-Output 会混进本函数的返回值（run_e2e.ps1 同款注释）
    Write-Host ("      画面非黑占比 " + [math]::Round($ratio * 100, 2) + "%")
    return ($ratio -ge $MinRatio)
  } catch {
    Write-Host ("      画面取样失败: " + $_.Exception.Message)
    return $false
  }
}

function Invoke-Flow([string]$Tag, [string[]]$RequiredStages) {
  # 判据只看"进世界"这一段（enter_game / ingame / 05b_after_notice.png）。
  # mir_flow 后面的小退段有十来次固定等待（数分钟），对本门禁无用 —— 判据产物一齐就收工，
  # 免得把整栈按住 8 分钟。**注意：本函数所有日志必须 Write-Host**，
  # Write-Output 会混进返回值（PowerShell 函数返回"所有未捕获输出"）。
  #
  # 为什么带重试：`mir_flow.ps1` 启动时会 `Get-Process MirClinet | Stop-Process -Force` ——
  # 别的线（C3）同时跑同一个流程会互相把客户端杀掉，症状是流程在 notice_poll 前后"自己退出"、
  # 阶段不全。这不是被测行为的证据，重试即可；每次尝试都留日志与目录，别把噪声读成结论。
  for ($attempt = 1; $attempt -le $MaxFlowAttempts; $attempt++) {
    $flowOut = Join-Path $OutDir "$Tag-attempt$attempt"
    New-Item -ItemType Directory -Force -Path $flowOut | Out-Null
    $renderShot = Join-Path $flowOut '05b_after_notice.png'
    $stagesFile = Join-Path $flowOut 'stages.ndjson'
    $stdoutFile = Join-Path $flowOut 'flow.out.log'
    $flowArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $RepoRoot 'tools\client\mir_flow.ps1'), '-OutDir', $flowOut) + $FlowExtraArgs
    $proc = Start-Process -FilePath 'powershell' -ArgumentList $flowArgs -PassThru -WindowStyle Hidden `
      -RedirectStandardOutput $stdoutFile -RedirectStandardError ($stdoutFile + '.err')
    # 这里的 PowerShell 级重定向**可以留**：流程是短命进程，而且每次用完都会
    # Stop-ProcessTree 连树收掉 ⇒ 它不会活过本脚本、也就不会一直占着本脚本的 stdout 管道。
    # 反过来，长命服务（GameGate/GameSvr）绝不能这么起 —— 见 detached.ps1。

    $deadline = (Get-Date).AddSeconds($FlowTimeoutSec)
    $reached = $false
    $exitedEarly = $false
    while ((Get-Date) -lt $deadline) {
      if ($proc.HasExited) { $exitedEarly = $true; break }
      if ((Test-Path $renderShot) -and (Test-Path $stagesFile)) {
        $text = Get-Content -LiteralPath $stagesFile -Raw -ErrorAction SilentlyContinue
        if ($text -and $text -match '"stage":"ingame"') { $reached = $true; break }
      }
      Start-Sleep -Milliseconds 500
    }
    if (-not $proc.HasExited) {
      try { Stop-ProcessTree -Id $proc.Id } catch { }   # 连子进程树一起收：留着的孩子也会占住句柄/管道
      Write-Host "      （判据产物已齐/超时，结束流程进程）"
    }
    Get-Process MirClinet -ErrorAction SilentlyContinue | Stop-Process -Force

    $stages = @()
    if (Test-Path $stagesFile) {
      foreach ($line in (Get-Content $stagesFile)) {
        try { $stages += ($line | ConvertFrom-Json).stage } catch { }
      }
    }
    $renderOk = Test-NotBlackScreen $renderShot
    if ($reached -or $attempt -eq $MaxFlowAttempts) {
      if ($exitedEarly) { Write-Host "      第 $attempt 次尝试：流程提前退出（多半是别的线也在跑客户端），stages=$($stages.Count)" }
      $stagesOk = $true
      $pos = 0
      foreach ($req in $RequiredStages) {
        $found = $false
        for ($i = $pos; $i -lt $stages.Count; $i++) { if ($stages[$i] -eq $req) { $found = $true; $pos = $i + 1; break } }
        if (-not $found) { Write-Host "      缺阶段(或乱序): $req"; $stagesOk = $false; break }
      }
      return [pscustomobject]@{ Dir = $flowOut; Stages = $stages; StagesOk = $stagesOk; RenderOk = $renderOk; Reached = $reached }
    }
    Write-Host "      第 $attempt 次尝试没走到 ingame（多半是别的线同时跑客户端把 MirClinet 杀了），${FlowRetryDelaySec}s 后重试"
    Start-Sleep -Seconds $FlowRetryDelaySec
  }
  return [pscustomobject]@{ Dir = $OutDir; Stages = @(); StagesOk = $false; RenderOk = $false; Reached = $false }
}

function Get-LogText([string]$Path) {
  if (-not (Test-Path $Path)) { return '' }
  return (Get-Content -LiteralPath $Path -Raw -Encoding UTF8)
}

# ---------------- 0. 前置：整栈在跑 ----------------
$listening = Get-ListeningPorts
foreach ($p in @(5000, 7000, 7100, 7200)) {
  if ($listening -notcontains $p) {
    Write-Output "RED  前置端口 $p 未监听 —— 先起整栈（E:\MirServer\start-all.cmd）"
    exit 1
  }
}
if (-not (Test-Path (Join-Path $ClientRunDir 'run-release.cmd'))) {
  Write-Output "RED  客户端目录不可用: $ClientRunDir"
  exit 1
}
Write-Output "== verify_gate_reconnect：前置通过（5000/7000/7100/7200 均在监听）"

# 必需阶段取 mir_flow.ps1 实际会写出的那些（`Stage "…"` 全表实测）。
# 注：tools/e2e/run_e2e.ps1 的 flow-e2e 断言里还要求 notice_dismiss，而 mir_flow 从没写过这个阶段
# ⇒ 那条断言恒红；属 C 线工具侧既有缺陷，S1 不擅自改别人的门禁，只登记（见报告"跨线发现"）。
$required = @('start', 'login_id', 'login_pwd', 'login_submit', 'charsel', 'enter_game', 'ingame')
if ($SelfTestRed) { $required += 'stage_that_does_not_exist' }

# ---------------- 1. 抖动前基线（可跳过） ----------------
if (-not $SkipBaseline) {
  Write-Output "== 步骤 1：抖动前基线流程"
  $base = Invoke-Flow 'baseline' $required
  Add-Result 'baseline-flow-stages' $base.StagesOk ("stages=" + $base.Stages.Count)
  Add-Result 'baseline-flow-reached-world' $base.Reached '流程走到 ingame 且出了 05b_after_notice.png'
  Add-Result 'baseline-world-render' ([bool]$base.RenderOk) '05b_after_notice.png 非黑 >=1%'
}

# ---------------- 2. 抖动注入：只重启 GameGate ----------------
$gateOpenedBefore = ([regex]::Matches((Get-LogText $GameSvrLog), '游戏网关\[[^\]]+\]已打开')).Count
if (-not $SkipGateBounce) {
  Write-Output "== 步骤 2：重启 GameGate（不动 GameSvr/MySQL），制造一次网关重连"
  $gates = @(Get-Process GameGate -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $GateExe })
  foreach ($g in $gates) { Write-Output ("      停 GameGate pid=" + $g.Id); Stop-Process -Id $g.Id -Force }
  Start-Sleep -Seconds 2
  Start-DetachedProcess -Exe $GateExe -WorkDir $GateDir `
    -OutLog (Join-Path $LogDir 'RunGate.out.log') -ErrLog (Join-Path $LogDir 'RunGate.err.log')

  $deadline = (Get-Date).AddSeconds($GateReadyTimeoutSec)
  $reconnected = $false
  while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 2
    $now = ([regex]::Matches((Get-LogText $GameSvrLog), '游戏网关\[[^\]]+\]已打开')).Count
    if ($now -gt $gateOpenedBefore) { $reconnected = $true; break }
  }
  Add-Result 'gate-reconnected' $reconnected ("已打开 $gateOpenedBefore -> " + ([regex]::Matches((Get-LogText $GameSvrLog), '游戏网关\[[^\]]+\]已打开')).Count)
} else {
  Add-Result 'gate-reconnected' $true '已按 -SkipGateBounce 跳过抖动注入'
}

# ---------------- 3. 抖动后再进世界 ----------------
$nreBefore = ([regex]::Matches((Get-LogText $GameSvrLog), 'SetGateUserList')).Count
Write-Output "== 步骤 3：抖动后跑真实客户端进阶流程"
$after = Invoke-Flow 'after-bounce' $required
Add-Result 'after-bounce-flow-stages' $after.StagesOk ("stages=" + $after.Stages.Count)
Add-Result 'after-bounce-flow-reached-world' $after.Reached '流程走到 ingame 且出了 05b_after_notice.png'
Add-Result 'after-bounce-world-render' ([bool]$after.RenderOk) '05b_after_notice.png 非黑 >=1%'
$nreAfter = ([regex]::Matches((Get-LogText $GameSvrLog), 'SetGateUserList')).Count
Add-Result 'no-setgateuserlist-nre' ($nreAfter -eq $nreBefore) ("SetGateUserList 出现次数 $nreBefore -> $nreAfter")

# ---------------- 汇总 ----------------
$red = @($results | Where-Object { -not $_.ok }).Count
Write-Output ("==== verify_gate_reconnect: " + ($results.Count - $red) + " GREEN / " + $red + " RED；产物 " + $OutDir)
if ($SelfTestRed) {
  $flow = $results | Where-Object { $_.check -eq 'after-bounce-flow-stages' } | Select-Object -First 1
  if ($flow -and -not $flow.ok) { Write-Output 'SELFTEST_RED GREEN: 不存在的阶段被判 RED（断言可证伪）'; exit 0 }
  Write-Output 'SELFTEST_RED RED: 不存在的阶段竟然判绿——断言失效！'
  exit 1
}
exit $(if ($red -eq 0) { 0 } else { 1 })
