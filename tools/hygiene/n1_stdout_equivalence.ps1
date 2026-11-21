#requires -Version 5
<#
.SYNOPSIS
  n1_stdout_equivalence.ps1 — N=1「零变化」的可复跑 stdout 等价判据（门禁可登记）。

.DESCRIPTION
  背景：此前多处引用「N=1 归一化 stdout 51 行逐字节」，但**没有任何可复跑的命令**——第三方无法
  复现。本脚本把规则落成命令（卡 `openmir2-n1-stdout-equivalence`，2026-11-22）。

  判据（两条都必须绿）：
    1. **顺序敏感**：两侧 stdout 每行**剥掉开头的 12 位 `HH:MM:SS.mmm` 时间戳**（+ 1 个空格）后，
       按行序拼接的 SHA256 相等；
    2. **行多重集**：同一批行按序数排序后的 SHA256 相等。
  另：两侧都为空 ⇒ **PASS 并打印 `EMPTY_BOTH`**（记 `空文件 sha 相同`），一侧空一侧非空 ⇒ FAIL。
  行数不一致 ⇒ FAIL 并打印两侧行数。

  **覆盖的启动路径**（方法同 P2-3b Step 4）：配置文件解析（`[Server]` 键集校验）、地图注册
  （占位 `WorldMaps`，无 Envir）、`World` 构造、脚本/NPC 装载（若给真 Envir）、固定时长 tick、
  **优雅关服**（命名管道 `\\.\pipe\openmir2-gamesvr-shutdown-<pid>`）、进程退出码。
  **不覆盖**：真实客户端/网关交互、登录与帧生成、DB 读写内容、`-Mode Both` 真机对拍（另有其门）、
  以及任何由外部依赖时序决定的行（DB 重试/退避、端口占用重试）——这类行**不在可对比域**，
  必须用「同一可达 DB 端点 + 同一端口」让它们不出现（见 procedure 文档 §4）。

.PARAMETER BaselineExe
  基线树（改动前）的 game-svr 可执行文件（绝对路径）。
.PARAMETER FeatureExe
  特性树（改动后）的 game-svr 可执行文件（绝对路径）。
.PARAMETER Conf
  两侧**共用**的临时配置副本（必须已把 `GatePort` 改成 `-Port` 指定的非 5000 端口）。
.PARAMETER Port
  本次运行使用的 `GatePort`；必须与 `Conf` 里的值一致且 **不得为 5000**（生产端口，避免与活栈抢）。
.PARAMETER DurationSeconds
  起栈后固定运行时长（默认 20s）；中途自行退出 ⇒ FAIL。
.PARAMETER GracefulTimeoutSeconds
  优雅关服等待上界（默认 30s）；超时 ⇒ FAIL（不静默杀进程冒充通过）。
.PARAMETER OutDir
  产物目录（默认 `E:\tmp\m2-acceptance\n1-stdout\<时间戳>`）。
.PARAMETER Selftest
  离线自测（不起进程、不需要 exe/conf）：纯函数 + 配置守卫 + 判据方向各一组断言。

  **退出码**：0 = 绿；1 = 红（判据不成立，打印原因）；2 = 判不了（用法/文件缺失/超时/提前退出）。

.EXAMPLE
  pwsh -NoProfile -ExecutionPolicy Bypass -File tools/hygiene/n1_stdout_equivalence.ps1 `
    -BaselineExe E:\wt\baseline\mir2-rs\target\debug\game-svr.exe `
    -FeatureExe  E:\wt\feature\mir2-rs\target\debug\game-svr.exe `
    -Conf E:\tmp\m2-acceptance\n1-stdout\Server.conf -Port 5077
.EXAMPLE
  pwsh -NoProfile -ExecutionPolicy Bypass -File tools/hygiene/n1_stdout_equivalence.ps1 -Selftest
#>
param(
  [string]$BaselineExe = '',
  [string]$FeatureExe = '',
  [string]$Conf = '',
  [int]$Port = 0,
  [int]$DurationSeconds = 20,
  [int]$GracefulTimeoutSeconds = 30,
  [string]$OutDir = '',
  [switch]$Selftest
)

$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# 纯函数（自测直接覆盖；判据本体）
# ---------------------------------------------------------------------------

# 剥开头的 12 位墙钟时间戳 `HH:MM:SS.mmm` 与其后的 1 个空格（`glog!` = println! + 该前缀）。
function ConvertTo-N1Line {
  param([string]$Line)
  if ($null -eq $Line) { return '' }
  if ($Line.Length -ge 13 -and $Line -match '^\d{2}:\d{2}:\d{2}\.\d{3} ') {
    return $Line.Substring(13)
  }
  return $Line
}

function Get-Sha256Hex {
  param([string]$Text)
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $hash = $sha.ComputeHash($bytes)
    return (($hash | ForEach-Object { $_.ToString('x2') }) -join '')
  } finally {
    $sha.Dispose()
  }
}

# 顺序敏感：按行序拼接（末尾补换行，避免「A,B」与「A\nB」歧义）。
function Get-N1OrderSha {
  param([string[]]$Lines)
  return Get-Sha256Hex ((($Lines -join "`n") + "`n"))
}

# 行多重集：按序数排序后同法求 sha（顺序不敏感；与 OrderSha 一起可区分「换序」与「换内容」）。
function Get-N1MultisetSha {
  param([string[]]$Lines)
  $sorted = @($Lines)
  [Array]::Sort($sorted, [System.StringComparer]::Ordinal)
  return Get-Sha256Hex ((($sorted -join "`n") + "`n"))
}

# 配置守卫：`[Server]` 段必须存在，`GatePort` 必须存在且 == 期望端口，且不得是 5000。
function Test-N1Conf {
  param([string]$Path, [int]$ExpectedPort)
  $result = [pscustomobject]@{ Ok = $false; GatePort = 0; Reason = '' }
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    $result.Reason = "配置文件不存在：$Path"
    return $result
  }
  $lines = @(Get-Content -LiteralPath $Path -Encoding UTF8)
  $inServer = $false
  $gatePort = $null
  foreach ($line in $lines) {
    $trim = $line.Trim()
    if ($trim -match '^\[(.+)\]$') {
      $inServer = ($matches[1].Trim().ToLowerInvariant() -eq 'server')
      continue
    }
    if ($inServer -and $trim -match '^GatePort\s*=\s*(\d+)\s*$') {
      $gatePort = [int]$matches[1]
    }
  }
  if ($gatePort -eq $null) {
    $result.Reason = '配置里找不到 `[Server] GatePort`（本判据要求同一启动命令、显式端口）'
    return $result
  }
  if ($gatePort -eq 5000) {
    $result.Reason = 'GatePort=5000 是生产端口；本判据要求非 5000（避免与活栈抢端口）'
    return $result
  }
  if ($ExpectedPort -gt 0 -and $gatePort -ne $ExpectedPort) {
    $result.Reason = "配置的 GatePort=$gatePort 与 -Port=$ExpectedPort 不一致（两侧必须用同一端口）"
    return $result
  }
  $result.Ok = $true
  $result.GatePort = $gatePort
  return $result
}

# ---------------------------------------------------------------------------
# 自测（离线；不起进程）
# ---------------------------------------------------------------------------
function Invoke-Selftest {
  $script:selftestFail = 0
  function Assert-True([bool]$Cond, [string]$What) {
    if ($Cond) {
      [Console]::WriteLine("PASS $What")
    } else {
      [Console]::WriteLine("FAIL $What")
      $script:selftestFail++
    }
  }

  # ① 时间戳剥离（正例 / 反例 / 退化：空行、无前缀行、时间戳在行中）
  Assert-True ((ConvertTo-N1Line '03:04:05.123 [INF] hello') -ceq '[INF] hello') 'strip leading timestamp'
  Assert-True ((ConvertTo-N1Line ' [INF] no-stamp') -ceq ' [INF] no-stamp') 'no timestamp unchanged'
  Assert-True ((ConvertTo-N1Line '') -ceq '') 'empty line unchanged'
  Assert-True ((ConvertTo-N1Line 'x 03:04:05.123 y') -ceq 'x 03:04:05.123 y') 'mid-line timestamp unchanged'
  Assert-True ((ConvertTo-N1Line '03:04:05.12 [INF] short') -ceq '03:04:05.12 [INF] short') '11-digit stamp unchanged'

  # ② 判定方向：换序 ⇒ OrderSha 变、Multiset 不变；换内容 ⇒ 两者都变
  $a = @('[INF] a', '[INF] b')
  $b = @('[INF] b', '[INF] a')
  Assert-True ((Get-N1OrderSha $a) -ne (Get-N1OrderSha $b)) 'order-sha distinguishes reordering'
  Assert-True ((Get-N1MultisetSha $a) -ceq (Get-N1MultisetSha $b)) 'multiset-sha ignores reordering'
  $c = @('[INF] a', '[INF] c')
  Assert-True ((Get-N1MultisetSha $a) -ne (Get-N1MultisetSha $c)) 'multiset-sha detects content change'
  Assert-True ((Get-N1OrderSha $a) -ceq (Get-N1OrderSha ([string[]]@('[INF] a', '[INF] b')))) 'order-sha stable'

  # ③ 配置守卫：缺 [Server] / GatePort=5000 / 端口不一致 ⇒ 拒；合法 ⇒ 收
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("n1-stdout-selftest-" + [System.Guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  try {
    $noServer = Join-Path $tmp 'no-server.conf'
    Set-Content -LiteralPath $noServer -Value @('[Other]', 'GatePort=5077') -Encoding UTF8
    Assert-True (-not (Test-N1Conf $noServer 5077).Ok) 'conf without [Server] rejected'

    $p5000 = Join-Path $tmp 'port5000.conf'
    Set-Content -LiteralPath $p5000 -Value @('[Server]', 'GateAddr= 127.0.0.1', 'GatePort= 5000') -Encoding UTF8
    Assert-True (-not (Test-N1Conf $p5000 5000).Ok) 'GatePort=5000 rejected'

    $okConf = Join-Path $tmp 'ok.conf'
    Set-Content -LiteralPath $okConf -Value @('[Server]', 'GateAddr= 127.0.0.1', 'GatePort= 5077') -Encoding UTF8
    $check = Test-N1Conf $okConf 5077
    Assert-True ($check.Ok -and $check.GatePort -eq 5077) 'valid conf accepted'
    Assert-True (-not (Test-N1Conf $okConf 5078).Ok) 'port mismatch rejected'
    Assert-True (-not (Test-N1Conf (Join-Path $tmp 'missing.conf') 5077).Ok) 'missing conf rejected'
  } finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
  }

  if ($script:selftestFail -ne 0) {
    [Console]::WriteLine("SELFTEST_FAILED count=$($script:selftestFail)")
    return 1
  }
  [Console]::WriteLine('SELFTEST_OK')
  return 0
}

if ($Selftest) {
  exit (Invoke-Selftest)
}

# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
function Fail-Usage([string]$Why) {
  Write-Output "UNDECIDABLE $Why"
  exit 2
}

if (-not (Test-Path -LiteralPath $BaselineExe -PathType Leaf)) { Fail-Usage "基线 exe 不存在：$BaselineExe" }
if (-not (Test-Path -LiteralPath $FeatureExe -PathType Leaf)) { Fail-Usage "特性 exe 不存在：$FeatureExe" }
if ($Port -le 0 -or $Port -gt 65535) { Fail-Usage "-Port 必须是 1..65535（且非 5000）" }
if ($Port -eq 5000) { Fail-Usage '-Port 不得为 5000（生产端口）' }
if ($DurationSeconds -le 0) { Fail-Usage '-DurationSeconds 必须 > 0' }
$confCheck = Test-N1Conf -Path $Conf -ExpectedPort $Port
if (-not $confCheck.Ok) { Fail-Usage $confCheck.Reason }

if ([string]::IsNullOrEmpty($OutDir)) {
  $OutDir = Join-Path 'E:\tmp\m2-acceptance\n1-stdout' (Get-Date -Format 'yyyyMMdd-HHmmss')
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

function Invoke-N1Run {
  param([string]$Exe, [string]$Tag)
  $stdout = Join-Path $OutDir "$Tag.stdout.txt"
  $stderr = Join-Path $OutDir "$Tag.stderr.txt"
  $exeDir = Split-Path -Parent $Exe
  Write-Output "RUN $Tag exe=$Exe conf=$Conf port=$Port duration=${DurationSeconds}s"
  $confArg = '"' + $Conf + '"'
  $proc = Start-Process -FilePath $Exe -ArgumentList $confArg -WorkingDirectory $exeDir `
    -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -NoNewWindow
  Start-Sleep -Seconds $DurationSeconds
  if ($proc.HasExited) {
    Fail-Usage "$Tag 在 ${DurationSeconds}s 内提前退出（rc=$($proc.ExitCode)）；stderr=$stderr"
  }
  # 优雅关服：连 game-svr 的命名管道并写 8 字节（`spawn_shutdown_pipe_listener`）。
  $pipeName = "openmir2-gamesvr-shutdown-$($proc.Id)"
  $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', $pipeName, [System.IO.Pipes.PipeDirection]::Out)
  try {
    $pipe.Connect([Math]::Max(1000, $GracefulTimeoutSeconds * 1000))
    $payload = [byte[]](0, 1, 2, 3, 4, 5, 6, 7)
    $pipe.Write($payload, 0, $payload.Length)
    $pipe.Flush()
  } catch {
    Fail-Usage "$Tag 关服管道不可用（$pipeName）：$_"
  } finally {
    $pipe.Dispose()
  }
  if (-not $proc.WaitForExit($GracefulTimeoutSeconds * 1000)) {
    try { $proc.Kill() } catch { }
    Fail-Usage "$Tag 优雅关服超时（${GracefulTimeoutSeconds}s）"
  }
  if ($proc.ExitCode -ne 0) {
    Fail-Usage "$Tag 退出码 $($proc.ExitCode)（要求 0）"
  }
  return $stdout
}

$baselineOut = Invoke-N1Run -Exe $BaselineExe -Tag 'baseline'
$featureOut = Invoke-N1Run -Exe $FeatureExe -Tag 'feature'

function Read-N1Lines([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path)) { return @() }
  return @(Get-Content -LiteralPath $Path -Encoding UTF8 | ForEach-Object { ConvertTo-N1Line $_ })
}

$baselineLines = Read-N1Lines $baselineOut
$featureLines = Read-N1Lines $featureOut
$baseNorm = Join-Path $OutDir 'baseline.normalized.txt'
$featNorm = Join-Path $OutDir 'feature.normalized.txt'
Set-Content -LiteralPath $baseNorm -Value $baselineLines -Encoding UTF8
Set-Content -LiteralPath $featNorm -Value $featureLines -Encoding UTF8

$baseOrder = Get-N1OrderSha $baselineLines
$featOrder = Get-N1OrderSha $featureLines
$baseMulti = Get-N1MultisetSha $baselineLines
$featMulti = Get-N1MultisetSha $featureLines

$summary = [pscustomobject]@{
  baseline_exe   = $BaselineExe
  feature_exe    = $FeatureExe
  conf           = $Conf
  port           = $Port
  duration_s     = $DurationSeconds
  baseline_lines = $baselineLines.Count
  feature_lines  = $featureLines.Count
  baseline_order_sha = $baseOrder
  feature_order_sha  = $featOrder
  baseline_multiset_sha = $baseMulti
  feature_multiset_sha  = $featMulti
  result         = ''
}
$summaryPath = Join-Path $OutDir 'summary.json'

if ($baselineLines.Count -eq 0 -and $featureLines.Count -eq 0) {
  $summary.result = 'EMPTY_BOTH'
  $summary | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $summaryPath -Encoding UTF8
  Write-Output "EMPTY_BOTH 两侧 stdout 都为空（空文件 sha 相同=$baseOrder）；stderr 行数作为旁证："
  foreach ($tag in @('baseline', 'feature')) {
    $errPath = Join-Path $OutDir "$tag.stderr.txt"
    $errLines = @(Get-Content -LiteralPath $errPath -Encoding UTF8 -ErrorAction SilentlyContinue)
    Write-Output ("  {0}.stderr lines={1}" -f $tag, $errLines.Count)
    if ($errLines.Count -gt 0) {
      Write-Output ("    首行: " + $errLines[0])
      Write-Output ("    末行: " + $errLines[$errLines.Count - 1])
    }
  }
  exit 0
}

$orderOk = ($baseOrder -eq $featOrder)
$multiOk = ($baseMulti -eq $featMulti)
if ($orderOk -and $multiOk) {
  $summary.result = 'GREEN'
  $summary | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $summaryPath -Encoding UTF8
  Write-Output ("GREEN lines={0}/{1} order_sha={2} multiset_sha={3}" -f `
      $baselineLines.Count, $featureLines.Count, $baseOrder, $baseMulti)
  exit 0
}

$summary.result = 'RED'
$summary | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $summaryPath -Encoding UTF8
Write-Output ("RED baseline_lines={0} feature_lines={1} order_equal={2} multiset_equal={3}" -f `
    $baselineLines.Count, $featureLines.Count, $orderOk, $multiOk)
if (-not $orderOk) {
  $max = [Math]::Min($baselineLines.Count, $featureLines.Count)
  for ($i = 0; $i -lt $max; $i++) {
    if ($baselineLines[$i] -cne $featureLines[$i]) {
      Write-Output ("FIRST_DIFF line={0} baseline='{1}' feature='{2}'" -f `
          ($i + 1), $baselineLines[$i], $featureLines[$i])
      break
    }
  }
}
Write-Output "产物：$OutDir"
exit 1
