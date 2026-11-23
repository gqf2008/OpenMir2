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

# C14：golden 时间桶（与 `mir2-rs/bins/game-svr/src/enter.rs` 的 `game_time_from_hour` 同口径）。
# 桶值只由**本地小时**决定；N=1 的 `世界时钟变化` 行只在「世界初值 0 → 桶值 ≠ 0」时出现。
function Get-N1GameTimeBucket {
  param([int]$Hour = -1)
  if ($Hour -lt 0) { $Hour = (Get-Date).Hour }
  switch ($Hour) {
    { $_ -in 5..10 } { return 1 }
    { $_ -in 16..19 } { return 1 }
    21 { return 1 }
    { $_ -in 11, 20, 23 } { return 2 }
    { $_ -in 4, 15 } { return 0 }
    default { return 3 }
  }
}


# ---------------------------------------------------------------------------
# 纯函数（自测直接覆盖；判据本体）
# ---------------------------------------------------------------------------

# 剥开头的 12 位墙钟时间戳 `HH:MM:SS.mmm` 与其后的 1 个空格（`glog!` = println! + 该前缀）。
function ConvertTo-N1Line {
  param([string]$Line)
  if ($null -eq $Line) { return '' }
  if ($Line.Length -ge 13 -and $Line -match '^\d{2}:\d{2}:\d{2}\.\d{3} ') {
    $Line = $Line.Substring(13)
  }
  # 关服命名管道行带进程 pid（`...shutdown-<pid>`）：每次运行必然不同，不是产品差异 ⇒ 归一化。
  $Line = [regex]::Replace($Line, 'openmir2-gamesvr-shutdown-\d+', 'openmir2-gamesvr-shutdown-<pid>')
  # `世界时钟变化：Bright=<n>`：昼夜由**墙钟日期**派生 ⇒ 跨时段两次运行必然不同（1 vs 2），
  # 不是产品差异 ⇒ 归一化为 `Bright=<n>`（字面取值与总监代修一致，两边 sha 可对账）。
  $Line = [regex]::Replace($Line, 'Bright=\d+', 'Bright=<n>')
  # `城堡 N 日期串刷新：… （墙钟 YYYY-MM-DD HH:MM:SS）`：消息体里嵌**墙钟时刻**（每次启动
  # 的首轮刷新必然不同）⇒ 只归一化时刻本身；`IncomeToday/WarDate` 是语义位，**不**归一化
  # （两者不同仍判 RED）。
  return [regex]::Replace($Line, '（墙钟 \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}）', '（墙钟 <wallclock>）')
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

# 关服 payload：产品侧 `spawn_shutdown_pipe_listener` **只认 8 字节 ASCII `SHUTDOWN`**，
# 其它 8 字节会被记 `WRN 关服命名管道收到未知请求（忽略）` 并继续运行 ⇒ 脚本必须发这一串。
function Get-N1ShutdownPayload {
  return ,[System.Text.Encoding]::ASCII.GetBytes('SHUTDOWN')
}

# **退出码读数的判据分类**（2026-11-22 缺陷修复）。
#
# 现象：`Start-Process -PassThru -RedirectStandardOutput/-RedirectStandardError` 返回的 Process 对象
# `.ExitCode` 可能是 `$null`（本机最小复现：`Start-Process cmd.exe '/c exit 3' -PassThru
# -NoNewWindow -Redirect*` ⇒ `WaitForExit(5000)=True` 但 `.ExitCode=[]`；补一句
# `$null = $p.Handle` 再等 ⇒ `.ExitCode=3`）。
# 旧判据 `if ($proc.ExitCode -ne 0)` 对 `$null` 为**真** ⇒ 正常退出被报成「退出码（空）」的失败，
# 把「读不到」当成「不为 0」——违反「判不了 ≠ 红」。
#
# 分类：0 ⇒ `OK`；其它整数 ⇒ `FAIL`（产品非零）；`$null`/非整数 ⇒ `HARNESS`（**判据脚本自身
# 缺陷，不是产品失败**，调用方以 rc=2「判不了」退出）。
function Get-N1ExitVerdict {
  param([object]$Code)
  if ($null -eq $Code) { return 'HARNESS' }
  if ($Code -is [int] -or $Code -is [long]) {
    if ($Code -eq 0) { return 'OK' }
    return 'FAIL'
  }
  return 'HARNESS'
}

function Get-N1RunOutputPath {
  param([string]$Dir, [string]$Tag)
  return (Join-Path $Dir "$Tag.stdout.txt")
}

# 单值守卫：`Invoke-N1Run` 一旦用 `Write-Output` 打诊断行，返回值就会变成**数组**（此前真实发生：
# 两侧各 9600 字节的 stdout 被判 0 行 ⇒ EMPTY_BOTH 假绿）。所有读路径入口都先过这道守卫。
function Assert-N1SinglePath {
  param([object]$Path)
  return ($Path -is [string] -and -not [string]::IsNullOrEmpty($Path))
}

# 文件字节数；不存在 ⇒ -1（判不了，不当作空文件）。
function Get-N1FileBytes {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return -1 }
  return [int](Get-Item -LiteralPath $Path).Length
}

# **顺序噪声类**：DB / IDS 连接**重试环**的失败行。两条环各自按墙钟退避重试，两侧**内容相同**但
# 相对顺序由重试时刻决定 ⇒ 只在**顺序比较**时剔除；**行多重集仍用全量**（内容漂移必须红）。
# 注意：**不要用 `^` 锚定行首**——剥掉时间戳后真实行仍带 `[ERR] ` 前缀（`[ERR] 链接数据库服务器[…]
# 失败…`）；锚定写法会一条都匹配不到，噪声类形同虚设（2026-11-22 实测）。
$script:N1NoisePatterns = @(
  '链接数据库服务器\[.*\]失败',
  '链接登录服务器\[.*\]失败'
)

function Test-N1NoiseLine {
  param([string]$Line)
  foreach ($p in $script:N1NoisePatterns) {
    if ($Line -match $p) { return $true }
  }
  return $false
}

# 判决矩阵（纯函数）：**空文件**（字节 0）才算 EMPTY_BOTH；文件非空却解析 0 行 ⇒ UNDECIDABLE，
# 绝不因为「没比」而 PASS。
function Get-N1Verdict {
  param(
    [int]$BaselineBytes,
    [int]$FeatureBytes,
    [int]$BaselineLines,
    [int]$FeatureLines,
    [bool]$OrderOk,
    [bool]$MultisetOk
  )
  if ($BaselineBytes -lt 0 -or $FeatureBytes -lt 0) { return 'UNDECIDABLE' }
  if ($BaselineBytes -gt 0 -and $BaselineLines -eq 0) { return 'UNDECIDABLE' }
  if ($FeatureBytes -gt 0 -and $FeatureLines -eq 0) { return 'UNDECIDABLE' }
  if ($BaselineBytes -eq 0 -and $FeatureBytes -eq 0) { return 'EMPTY_BOTH' }
  if ($BaselineBytes -eq 0 -or $FeatureBytes -eq 0) { return 'RED' }
  if ($BaselineLines -ne $FeatureLines) { return 'RED' }
  if ($OrderOk -and $MultisetOk) { return 'GREEN' }
  return 'RED'
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

  # ④ 关服 payload：必须是 ASCII `SHUTDOWN`（8 字节），且**不是**旧的 0..7 字节模式
  $payload = Get-N1ShutdownPayload
  $expected = [System.Text.Encoding]::ASCII.GetBytes('SHUTDOWN')
  Assert-True ($payload.Length -eq 8) 'shutdown payload is 8 bytes'
  $same = $true
  for ($i = 0; $i -lt 8; $i++) { if ($payload[$i] -ne $expected[$i]) { $same = $false } }
  Assert-True $same 'shutdown payload equals ASCII SHUTDOWN'
  $oldWrong = [byte[]](0, 1, 2, 3, 4, 5, 6, 7)
  $equalsOld = $true
  for ($i = 0; $i -lt 8; $i++) { if ($payload[$i] -ne $oldWrong[$i]) { $equalsOld = $false } }
  Assert-True (-not $equalsOld) 'shutdown payload is not the old 0..7 byte pattern'

  # ⑤ 假绿回归：文件非空却解析 0 行 ⇒ UNDECIDABLE（不是 EMPTY_BOTH/PASS）
  $v = { param($bb, $fb, $bl, $fl, $o, $m) Get-N1Verdict -BaselineBytes $bb -FeatureBytes $fb `
      -BaselineLines $bl -FeatureLines $fl -OrderOk $o -MultisetOk $m }
  Assert-True ((& $v 0 0 0 0 $true $true) -eq 'EMPTY_BOTH') 'verdict: both empty files -> EMPTY_BOTH'
  Assert-True ((& $v 100 100 0 0 $true $true) -eq 'UNDECIDABLE') 'verdict: non-empty but 0 lines -> UNDECIDABLE (no false green)'
  Assert-True ((& $v 10 0 1 0 $false $false) -eq 'RED') 'verdict: one side empty -> RED'
  Assert-True ((& $v 100 100 5 5 $true $true) -eq 'GREEN') 'verdict: equal sha -> GREEN'
  Assert-True ((& $v 100 100 5 5 $false $true) -eq 'RED') 'verdict: order differs -> RED'
  Assert-True ((& $v 100 100 5 6 $true $true) -eq 'RED') 'verdict: line count differs -> RED'
  Assert-True ((& $v -1 100 0 0 $false $false) -eq 'UNDECIDABLE') 'verdict: missing artifact -> UNDECIDABLE'

  # ⑥ 输出污染防线（真实事故：Write-Output 让返回值变数组 ⇒ 9600 字节被判 0 行）
  Assert-True (Assert-N1SinglePath 'E:/x/a.txt') 'single path guard accepts string'
  Assert-True (-not (Assert-N1SinglePath @('RUN x', 'E:/x/a.txt'))) 'single path guard rejects array'
  Assert-True (-not (Assert-N1SinglePath '')) 'single path guard rejects empty'
  Assert-True ((Get-N1RunOutputPath 'E:/x' 'baseline') -is [string]) 'run output path is a string'

  # ⑦ 关服管道 pid 归一化（每次运行必然不同，不是产品差异）
  Assert-True ((ConvertTo-N1Line '00:00:00.000 [INF] x openmir2-gamesvr-shutdown-12345') -ceq `
      '[INF] x openmir2-gamesvr-shutdown-<pid>') 'shutdown pipe pid normalized'

  # ⑦b 昼夜值归一化（`Bright=<n>` 由墙钟日期派生，跨时段会变）
  Assert-True ((ConvertTo-N1Line '00:00:00.000 [INF] 世界时钟变化：Bright=1 ⇒ 向 0 个在线玩家各发一条 SM_DAYCHANGING(46)') -ceq `
      '[INF] 世界时钟变化：Bright=<n> ⇒ 向 0 个在线玩家各发一条 SM_DAYCHANGING(46)') 'clock Bright value normalized'

  # ⑦c 城堡日期串刷新：只归一化消息体里的墙钟时刻（语义位保留 ⇒ 仍能抓真差异）
  Assert-True ((ConvertTo-N1Line '00:00:00.000 [INF] 城堡 0 日期串刷新：IncomeToday=true WarDate=false（墙钟 2026-11-22 14:29:50）') -ceq `
      '[INF] 城堡 0 日期串刷新：IncomeToday=true WarDate=false（墙钟 <wallclock>）') 'castle wall-clock normalized'
  Assert-True ((ConvertTo-N1Line '00:00:00.000 [INF] 城堡 0 日期串刷新：IncomeToday=true WarDate=false（墙钟 2026-11-22 14:30:10）') -ceq `
      (ConvertTo-N1Line '00:00:00.000 [INF] 城堡 0 日期串刷新：IncomeToday=true WarDate=false（墙钟 2026-11-22 14:29:50）')) 'castle wall-clock: two runs equal'
  Assert-True ((ConvertTo-N1Line '00:00:00.000 [INF] 城堡 0 日期串刷新：IncomeToday=false WarDate=false（墙钟 2026-11-22 14:30:10）') -cne `
      (ConvertTo-N1Line '00:00:00.000 [INF] 城堡 0 日期串刷新：IncomeToday=true WarDate=false（墙钟 2026-11-22 14:29:50）')) 'castle wall-clock: semantic bit still differs'

  # ⑦d 退出码分类（缺陷 #N1-EXITCODE：$null 曾被当成「不为 0」⇒ 正常退出报失败）
  Assert-True ((Get-N1ExitVerdict 0) -ceq 'OK') 'exit verdict: 0 -> OK'
  Assert-True ((Get-N1ExitVerdict 3) -ceq 'FAIL') 'exit verdict: 3 -> FAIL'
  Assert-True ((Get-N1ExitVerdict $null) -ceq 'HARNESS') 'exit verdict: $null -> HARNESS (not product failure)'
  Assert-True ((Get-N1ExitVerdict 'x') -ceq 'HARNESS') 'exit verdict: non-integer -> HARNESS'

  # ⑧ DB/IDS 重试环属顺序噪声类：顺序比较剔除、多重集全量
  Assert-True (Test-N1NoiseLine '[ERR] 链接数据库服务器[127.0.0.1:3306]失败...（e）') 'noise class: db retry line (real shape with level prefix)'
  Assert-True (Test-N1NoiseLine '[ERR] 链接登录服务器[127.0.0.1:5600]失败...（e）') 'noise class: ids retry line (real shape with level prefix)'
  Assert-True (-not (Test-N1NoiseLine '[INF] 链接数据库服务器就绪')) 'noise class: normal line not matched'
  $noisyA = @('[INF] a', '[ERR] 链接数据库服务器[1.2.3.4:3306]失败...（e）', '[ERR] 链接登录服务器[1.2.3.4:5600]失败...（e）', '[INF] b')
  $noisyB = @('[INF] a', '[ERR] 链接登录服务器[1.2.3.4:5600]失败...（e）', '[ERR] 链接数据库服务器[1.2.3.4:3306]失败...（e）', '[INF] b')
  Assert-True ((Get-N1OrderSha $noisyA) -ne (Get-N1OrderSha $noisyB)) 'premise: noise reorder changes full order-sha'
  $fA = @($noisyA | Where-Object { -not (Test-N1NoiseLine $_) })
  $fB = @($noisyB | Where-Object { -not (Test-N1NoiseLine $_) })
  Assert-True ((Get-N1OrderSha $fA) -eq (Get-N1OrderSha $fB)) 'noise reorder ignored by noise-filtered order-sha'
  Assert-True ((Get-N1MultisetSha $noisyA) -eq (Get-N1MultisetSha $noisyB)) 'noise reorder keeps full multiset equal'

  # ⑨ PowerShell 逐条执行：函数必须**先定义后调用**（曾实测 `术语 'Assert-N1RunPath' 不会被识别`）。
  # 「首个调用」只扫 `if ($Selftest) {` 之后的主流程区（自测区自身含函数名，不能计入）。
  $selfLines = @(Get-Content -LiteralPath $PSCommandPath -Encoding UTF8)
  $dispatchAt = 0
  for ($li = 0; $li -lt $selfLines.Count; $li++) {
    if ($selfLines[$li].StartsWith('if ($Selftest)')) {
      $dispatchAt = $li + 1
      break
    }
  }
  foreach ($fn in @('Assert-N1RunPath', 'Read-N1Lines', 'Get-N1Verdict')) {
    $defAt = 0
    for ($li = 0; $li -lt $selfLines.Count; $li++) {
      if ($selfLines[$li].StartsWith("function $fn ") -or $selfLines[$li].StartsWith("function $fn(")) {
        $defAt = $li + 1
        break
      }
    }
    $callAt = 0
    for ($li = $dispatchAt; $li -lt $selfLines.Count; $li++) {
      $isDef = $selfLines[$li].StartsWith("function $fn ") -or $selfLines[$li].StartsWith("function $fn(")
      if ($selfLines[$li].Contains($fn) -and -not $isDef) {
        $callAt = $li + 1
        break
      }
    }
    Assert-True ($dispatchAt -gt 0 -and $defAt -gt 0 -and $callAt -gt 0 -and $defAt -lt $callAt) ("function {0} defined before first call (def={1}, call={2})" -f $fn, $defAt, $callAt)
  }

  if ($script:selftestFail -ne 0) {
    [Console]::WriteLine("SELFTEST_FAILED count=$($script:selftestFail)")
    return 1
  }
  # ⑦ C14：game_time 桶映射（与 enter::game_time_from_hour 同口径）
  Assert-True ((Get-N1GameTimeBucket -Hour 4) -eq 0) 'bucket: hour 4 -> 0'
  Assert-True ((Get-N1GameTimeBucket -Hour 15) -eq 0) 'bucket: hour 15 -> 0'
  Assert-True ((Get-N1GameTimeBucket -Hour 5) -eq 1) 'bucket: hour 5 -> 1'
  Assert-True ((Get-N1GameTimeBucket -Hour 21) -eq 1) 'bucket: hour 21 -> 1'
  Assert-True ((Get-N1GameTimeBucket -Hour 11) -eq 2) 'bucket: hour 11 -> 2'
  Assert-True ((Get-N1GameTimeBucket -Hour 23) -eq 2) 'bucket: hour 23 -> 2'
  Assert-True ((Get-N1GameTimeBucket -Hour 3) -eq 3) 'bucket: hour 3 -> 3'
  Assert-True ((Get-N1GameTimeBucket -Hour 12) -eq 3) 'bucket: hour 12 -> 3'

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

# 诊断：打印文件末尾 N 行到 stdout（本次定位「payload 错 ⇒ 服务端 30s 不退」就是靠 stdout 里那条
# `WRN 关服命名管道收到未知请求（忽略）`）。
function Show-Tail {
  param([string]$Path, [string]$Label, [int]$Count = 5)
  if (-not (Test-Path -LiteralPath $Path)) { return }
  $lines = @(Get-Content -LiteralPath $Path -Encoding UTF8 -ErrorAction SilentlyContinue)
  $start = [Math]::Max(0, $lines.Count - $Count)
  for ($i = $start; $i -lt $lines.Count; $i++) {
    [Console]::WriteLine(("DIAG {0}[{1}] {2}" -f $Label, ($i + 1), $lines[$i]))
  }
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

function Read-N1Lines([object]$Path) {
  if (-not (Assert-N1SinglePath $Path)) {
    Fail-Usage ("只接受单个路径字符串（实得 {0}：函数输出被污染/返回值被诊断行污染）" -f `
        $(if ($null -eq $Path) { 'null' } else { $Path.GetType().FullName }))
  }
  if (-not (Test-Path -LiteralPath $Path)) { return @() }
  return @(Get-Content -LiteralPath $Path -Encoding UTF8 | ForEach-Object { ConvertTo-N1Line $_ })
}

function Assert-N1RunPath([object]$Path, [string]$Tag) {
  if (-not (Assert-N1SinglePath $Path)) {
    Fail-Usage "$Tag 运行返回的不是单个路径（函数输出被污染：诊断行必须走 Console/Write-Host）"
  }
}

function Invoke-N1Run {
  param([string]$Exe, [string]$Tag)
  $stdout = Get-N1RunOutputPath -Dir $OutDir -Tag $Tag
  $stderr = Join-Path $OutDir "$Tag.stderr.txt"
  $exeDir = Split-Path -Parent $Exe
  [Console]::WriteLine("RUN $Tag exe=$Exe conf=$Conf port=$Port duration=${DurationSeconds}s")
  $confArg = '"' + $Conf + '"'
  $proc = Start-Process -FilePath $Exe -ArgumentList $confArg -WorkingDirectory $exeDir `
    -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -NoNewWindow
  # **2026-11-22 缺陷修复**：立刻碰一次 `.Handle`（缓存 OS 句柄）。不碰时 Process 对象与句柄的
  # 关联是惰性的，重定向 + `-PassThru` 的组合下 `.ExitCode` 可能在进程退出后仍读到 `$null`。
  $null = $proc.Handle
  Start-Sleep -Seconds $DurationSeconds
  if ($proc.HasExited) {
    Fail-Usage "$Tag 在 ${DurationSeconds}s 内提前退出（rc=$($proc.ExitCode)）；stderr=$stderr"
  }
  # 优雅关服：连 game-svr 的命名管道并写 8 字节（`spawn_shutdown_pipe_listener`）。
  $pipeName = "openmir2-gamesvr-shutdown-$($proc.Id)"
  $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', $pipeName, [System.IO.Pipes.PipeDirection]::Out)
  try {
    $pipe.Connect([Math]::Max(1000, $GracefulTimeoutSeconds * 1000))
    $payload = Get-N1ShutdownPayload
    $pipe.Write($payload, 0, $payload.Length)
    $pipe.Flush()
  } catch {
    Fail-Usage "$Tag 关服管道不可用（$pipeName）：$_"
  } finally {
    $pipe.Dispose()
  }
  if (-not $proc.WaitForExit($GracefulTimeoutSeconds * 1000)) {
    try { $proc.Kill() } catch { }
    [Console]::WriteLine(
      "DIAG $Tag 优雅关服超时：管道已连上，但服务端 ${GracefulTimeoutSeconds}s 未退出（先看末尾是否 `WRN 未知请求` ⇒ payload 不是 ASCII SHUTDOWN）")
    Show-Tail $stdout 'stdout' 5
    Show-Tail $stderr 'stderr' 5
    Fail-Usage "$Tag 优雅关服超时（${GracefulTimeoutSeconds}s；stdout=$stdout）"
  }
  # **判据分类**：`$null`/非整数 = 「退出码不可读」（HARNESS，rc=2「判不了」，**不是产品失败**）；
  # 非零整数 = 产品失败（沿用既有的 Fail-Usage 口径）。两侧 stdout 对比只在两侧都读到时才有意义。
  $exitVerdict = Get-N1ExitVerdict $proc.ExitCode
  if ($exitVerdict -eq 'HARNESS') {
    [Console]::WriteLine(
      "HARNESS $Tag 退出码不可读（ExitCode=$($proc.ExitCode)；Process 对象未拿到句柄）——**非产品失败**；stdout=$stdout")
    exit 2
  }
  if ($exitVerdict -eq 'FAIL') {
    Fail-Usage "$Tag 退出码 $($proc.ExitCode)（要求 0）"
  }
  return $stdout
}

$baselineOut = Invoke-N1Run -Exe $BaselineExe -Tag 'baseline'
Assert-N1RunPath $baselineOut 'baseline'
$featureOut = Invoke-N1Run -Exe $FeatureExe -Tag 'feature'
Assert-N1RunPath $featureOut 'feature'

$baselineLines = Read-N1Lines $baselineOut
$featureLines = Read-N1Lines $featureOut
$baseBytes = Get-N1FileBytes $baselineOut
$featBytes = Get-N1FileBytes $featureOut
$baseNorm = Join-Path $OutDir 'baseline.normalized.txt'
$featNorm = Join-Path $OutDir 'feature.normalized.txt'
Set-Content -LiteralPath $baseNorm -Value $baselineLines -Encoding UTF8
Set-Content -LiteralPath $featNorm -Value $featureLines -Encoding UTF8

# **顺序比较剔除顺序噪声类**（DB/IDS 重试环）；**多重集仍用全量**（内容漂移必须红）。
$baselineOrderLines = @($baselineLines | Where-Object { -not (Test-N1NoiseLine $_) })
$featureOrderLines = @($featureLines | Where-Object { -not (Test-N1NoiseLine $_) })
$noiseBase = $baselineLines.Count - $baselineOrderLines.Count
$noiseFeat = $featureLines.Count - $featureOrderLines.Count
$baseOrder = Get-N1OrderSha $baselineOrderLines
$featOrder = Get-N1OrderSha $featureOrderLines
$baseMulti = Get-N1MultisetSha $baselineLines
$featMulti = Get-N1MultisetSha $featureLines
$orderOk = ($baseOrder -eq $featOrder)
$multiOk = ($baseMulti -eq $featMulti)

$verdict = Get-N1Verdict -BaselineBytes $baseBytes -FeatureBytes $featBytes `
  -BaselineLines $baselineLines.Count -FeatureLines $featureLines.Count `
  -OrderOk $orderOk -MultisetOk $multiOk

$startHour = (Get-Date).Hour
$bucket = Get-N1GameTimeBucket -Hour $startHour
$summary = [pscustomobject]@{
  start_hour     = $startHour
  game_time_bucket = $bucket
  game_time_bucket_source = 'enter::game_time_from_hour 同口径（本地小时）'
  baseline_exe   = $BaselineExe
  feature_exe    = $FeatureExe
  conf           = $Conf
  port           = $Port
  duration_s     = $DurationSeconds
  baseline_bytes = $baseBytes
  feature_bytes  = $featBytes
  baseline_lines = $baselineLines.Count
  feature_lines  = $featureLines.Count
  baseline_order_lines = $baselineOrderLines.Count
  feature_order_lines  = $featureOrderLines.Count
  noise_lines_baseline = $noiseBase
  noise_lines_feature  = $noiseFeat
  baseline_order_sha = $baseOrder
  feature_order_sha  = $featOrder
  baseline_multiset_sha = $baseMulti
  feature_multiset_sha  = $featMulti
  result         = $verdict
}
$summaryPath = Join-Path $OutDir 'summary.json'
$summary | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $summaryPath -Encoding UTF8

if ($verdict -eq 'UNDECIDABLE') {
  Write-Output ("UNDECIDABLE 判不了：baseline_bytes={0} feature_bytes={1} baseline_lines={2} feature_lines={3}" -f `
      $baseBytes, $featBytes, $baselineLines.Count, $featureLines.Count)
  Write-Output '  （产物缺失 / 非空文件却解析 0 行 / 函数输出被污染——一律不判绿；末尾 5 行见下）'
  Show-Tail $baselineOut 'baseline.stdout' 5
  Show-Tail $featureOut 'feature.stdout' 5
  exit 2
}

if ($verdict -eq 'EMPTY_BOTH') {
  Write-Output ("EMPTY_BOTH 两侧 stdout 文件长度都为 0（空文件 sha 相同={0}）；stderr 行数作为旁证：" -f $baseOrder)
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

if ($verdict -eq 'GREEN') {
  Write-Output ("GREEN lines={0}/{1} bytes={2}/{3} order_sha={4} multiset_sha={5}" -f `
      $baselineLines.Count, $featureLines.Count, $baseBytes, $featBytes, $baseOrder, $baseMulti)
  exit 0
}

Write-Output ("RED baseline_lines={0} feature_lines={1} bytes={2}/{3} order_equal={4} multiset_equal={5}" -f `
    $baselineLines.Count, $featureLines.Count, $baseBytes, $featBytes, $orderOk, $multiOk)
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
