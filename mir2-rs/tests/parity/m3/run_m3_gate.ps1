#requires -Version 5
<#
.SYNOPSIS
  run_m3_gate.ps1 — M3 正式门禁（一条命令）：同库同脚本对拍 + RNG 基线 + 快照 hash。

.DESCRIPTION
  设计文档 §6 M3 判据（owner 2026-10-10 决策：夹具提升为正式门禁）：
    - DB 侧：作用域内字段级 diff = 0（`scoped_diff.py diff --canonical`；
      只对"每次运行必变"的列（自增 PK/FK、墙钟 datetime/unix 秒）与夹具随机输入的
      建角列（Hair/Sex/Job）置占位，其余列逐字节比；未登记表 fail-closed。
      易变列白名单与理由见 `scoped_diff.py` 的 TABLE_COLUMNS/VOLATILE_COLUMNS）
    - 基线侧：C# 基线一条命令可复跑且 hash 稳定（`rng-baseline.ps1`）
    - 两侧实现必须真的是"两侧"（见下方实现识别护栏）

  五条机判护栏（peer 场景一致性 / sessionDir 空值 / `-Side rust` 已知 Rust 入口 / peer 背包-装备开关一致性 /
  peer 固定起点一致性）的唯一口径在 `gate_guards.ps1`，主流程与 `selftest_gate.ps1` 共用（照 `tools/m2-acceptance/portclassify.ps1` 先例）。
  自检（离线，不起栈/不占端口）：`powershell -ExecutionPolicy Bypass -File selftest_gate.ps1`。

  流程：
    0) **实现识别护栏**：读 7000/7100/7200 的实际监听进程，口径由 `tools/m2-acceptance/portclassify.ps1`
       提供 —— `-Side rust` 时必须三个端口都是**已知 Rust 入口**且挂在本端口期望入口上
       （`Get-M3EntryGuard`，不再只判「不是 C#」）；`-Side csharp` 时必须是 `E:\MirServer` 的 C# 实现。
       不过就立即 RED（否则就是拿 C# 跟自己比 / 已知 Rust 入口接错线，门禁恒绿或假绿）
    1) `run_pair.ps1 -Side <side>`：快照 before/after（13 表 sha256）+ 作用域抽取 + 基线 json
    2) RNG 基线：
       - side=csharp：`rng-baseline.ps1`（种子注入 → 世界 RNG 流 + 掉落/经验序列落盘 + hash）
       - side=rust ：重放 C# 记录流（`cargo test -p mir2-parity-tests --test rng_replay_parity`）
    3) 跨侧比对（给了 `-PeerSession` 时）：先机判「本侧会话目录非空」（`Get-M3SessionDirFault`）
       与「两侧建角场景一致」（`baseline.json.fixed_new_chr`，`Get-M3PeerScenarioGuard`）、
       与「两侧背包/装备开关一致」（`baseline.json.bag_equip_sequence`，`Get-M3PeerBagEquipScopeGuard`），
       再跑 `scoped_diff.py diff` ⇒ 必须 0
    4) 汇总 `M3-gate-report.json`，退出码 0 仅当所有判据都过

.EXAMPLE
  # C# 基线（含两次运行的 hash 稳定性验证）
  powershell -ExecutionPolicy Bypass -File run_m3_gate.ps1 -Side csharp -VerifyRepeat

.EXAMPLE
  # M1 顶住 7000/7100/7200 之后的首验（C# 会话作为参照）
  powershell -ExecutionPolicy Bypass -File run_m3_gate.ps1 -Side rust -PeerSession csharp-20261010-140632
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [int]$Bots = 1,
  [int]$Seed = 42,
  [string]$Prefix = '',
  [string]$PeerSession = '',
  [switch]$VerifyRepeat,
  [switch]$SkipRngBaseline,
  [switch]$SkipBots,
  [switch]$FixedNewChr,          # M3 固定建角场景（卡 openmir2-m3-botsrv-fixed-newchr）：
                                 # 跨侧 diff 带 --fixed-newchr ⇒ Job/Hair/Sex 不置占位
  [int]$NewChrJob = 0,
  [int]$NewChrHair = 2,
  [int]$NewChrSex = 0,
  [switch]$BagEquipSequence,     # M3 T-14（T-22）：固定背包/装备序列（拾取→装备→卸下→丢弃→用药）
  [int]$BagEquipStepMs = 1000,
  [int]$BagEquipStartDelayMs = 3000,
  [switch]$PinStartPose           # T-14 固定起点变体（透传给 run_pair；owner 86478c55/d7798370）
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..')).Path
# 五条机判护栏的唯一口径（与 selftest_gate.ps1 共用；gate_guards.ps1 自行 dot-source portclassify.ps1）
. (Join-Path $Here 'gate_guards.ps1')
$LocalScenario = Format-M3Scenario -FixedNewChr ([bool]$FixedNewChr) -NewChrJob $NewChrJob -NewChrHair $NewChrHair -NewChrSex $NewChrSex
# 背包/装备开关（T-14）：-BagEquipSequence 时 run_pair 会一并传 -DisableAutoPlay（两侧同开关）。
$LocalBagEquipScope = Format-M3BagEquipScope -BagEquipSequence ([bool]$BagEquipSequence) -BagEquipStepMs $BagEquipStepMs -BagEquipStartDelayMs $BagEquipStartDelayMs -DisableAutoPlay ([bool]$BagEquipSequence)
$Stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$ReportPath = Join-Path $Here ("sessions\M3-gate-{0}-{1}.json" -f $Side, $Stamp)
$report = [ordered]@{
  side = $Side; bots = $Bots; seed = $Seed; started_at = (Get-Date).ToString('o')
  fixed_new_chr = $LocalScenario
  bag_equip_sequence = $LocalBagEquipScope
  steps = [ordered]@{}
}
$failed = @()

function Get-EntryHolders {
  $list = @()
  foreach ($p in @(7000, 7100, 7200)) {
    $conn = Get-NetTCPConnection -State Listen -LocalPort $p -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $conn) { $list += (New-M3EntryHolderNone -Port $p); continue }
    $proc = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue
    $pname = if ($proc) { $proc.ProcessName } else { '(未知)' }
    $ppath = if ($proc) { $proc.Path } else { '' }
    # impl/entry/verdict 一律走 portclassify.ps1 的口径（与 M2 验收① 同一分类器）
    $list += (New-M3EntryHolder -Port $p -ProcessName $pname -Path $ppath)
  }
  return $list
}

# ---- 0. 实现识别护栏 ----
Write-Output '--- 0) 实现识别护栏 ---'
$holders = Get-EntryHolders
foreach ($h in $holders) {
  Write-Output ("ENTRY {0} <- {1} [impl={2}] entry={3} verdict={4} path={5}" -f $h.port, $h.process, $h.impl, $h.entry, $h.verdict, $h.path)
}
$report.entry_holders = $holders
$entryGuard = Get-M3EntryGuard -Holders $holders -Side $Side
$impls = $entryGuard.impls
if ($impls -notcontains 'none') { $report.entry_impl = $impls -join '+' }
foreach ($m in $entryGuard.messages) { Write-Output $m }
if ($entryGuard.failed -contains 'rust_side_entry_not_known_rust') {
  Write-Output '     先把 7000/7100/7200 交给 M1 的 Rust 实现（login-gate / sel-gate / game-gate 各就各位），再跑本命令'
}
$failed += @($entryGuard.failed)

# 护栏不过就**立刻退出**：继续跑会把 C# 的结果写进名为 rust 的会话目录（比不跑更坏）
if ($failed.Count -gt 0) {
  $report.finished_at = (Get-Date).ToString('o')
  $report.failed = $failed
  $report.pass = $false
  $report | ConvertTo-Json -Depth 8 | Set-Content -Path $ReportPath -Encoding utf8
  Write-Output ('M3_GATE_REPORT ' + $ReportPath)
  Write-Output ('RED: 入口护栏未通过，快速失败: ' + ($failed -join ', '))
  exit 1
}

# ---- 1. DB 对拍夹具（同库同脚本） ----
Write-Output '--- 1) run_pair（快照 + 作用域抽取 + 基线） ---'
$pairArgs = @('-Side', $Side, '-Bots', $Bots)
if ($Prefix -ne '') { $pairArgs += @('-Prefix', $Prefix) }
if ($SkipBots) { $pairArgs += '-SkipBots' }
if ($FixedNewChr) { $pairArgs += @('-FixedNewChr', '-NewChrJob', $NewChrJob, '-NewChrHair', $NewChrHair, '-NewChrSex', $NewChrSex) }
if ($BagEquipSequence) { $pairArgs += @('-BagEquipSequence', '-BagEquipStepMs', $BagEquipStepMs, '-BagEquipStartDelayMs', $BagEquipStartDelayMs) }
if ($PinStartPose) { $pairArgs += '-PinStartPose' }
& powershell -ExecutionPolicy Bypass -File (Join-Path $Here 'run_pair.ps1') @pairArgs | Tee-Object -FilePath (Join-Path $Here ("sessions\gate-{0}-{1}.pair.log" -f $Side, $Stamp))
$pairExit = $LASTEXITCODE
$sideSessionDirs = @(Get-ChildItem (Join-Path $Here 'sessions') -Directory -Filter ("{0}-*" -f $Side) |
                     Where-Object { Test-Path (Join-Path $_.FullName 'baseline.json') } |
                     Sort-Object LastWriteTime -Descending)
$sessionDir = if ($sideSessionDirs.Count -gt 0) { $sideSessionDirs[0] } else { $null }
$report.steps.run_pair = [ordered]@{ exit = $pairExit; session = if ($sessionDir) { $sessionDir.Name } else { $null } }
if ($pairExit -ne 0) {
  Write-Output 'RED: run_pair 未通过，M3 门禁不成立'
  $failed += 'run_pair_failed'
}

# ---- 2. RNG 基线 / 记录回放 ----
Write-Output '--- 2) RNG 基线 ---'
if ($SkipRngBaseline) {
  $report.steps.rng = [ordered]@{ skipped = $true }
  Write-Output 'SKIP: 复用已有 C# 基线（-SkipRngBaseline）'
} elseif ($Side -eq 'csharp') {
  $rngArgs = @('-Seed', $Seed, '-Session', ("m3rng-gate-{0}" -f $Stamp))
  if ($VerifyRepeat) { $rngArgs += '-VerifyRepeat' }
  & powershell -ExecutionPolicy Bypass -File (Join-Path $Here 'rng-baseline.ps1') @rngArgs |
    Tee-Object -FilePath (Join-Path $Here ("sessions\gate-{0}-{1}.rng.log" -f $Side, $Stamp))
  $rngExit = $LASTEXITCODE
  $rngSession = Join-Path $Here ("sessions\m3rng-gate-{0}" -f $Stamp)
  $product = Join-Path $rngSession 'product.json'
  $report.steps.rng = [ordered]@{
    exit = $rngExit
    session = ("m3rng-gate-{0}" -f $Stamp)
    product = if (Test-Path $product) { (Get-Content $product -Raw | ConvertFrom-Json) } else { $null }
  }
  if ($rngExit -ne 0) { $failed += 'rng_baseline_failed' }
} else {
  # rust 侧：重放 C# 记录流（形态/顺序/次数逐条对拍）
  & cargo test --manifest-path (Join-Path $RepoRoot 'mir2-rs/Cargo.toml') -p mir2-parity-tests --test rng_replay_parity |
    Tee-Object -FilePath (Join-Path $Here ("sessions\gate-{0}-{1}.replay.log" -f $Side, $Stamp))
  $replayExit = $LASTEXITCODE
  $report.steps.rng_replay = [ordered]@{ exit = $replayExit }
  if ($replayExit -ne 0) { $failed += 'rng_replay_failed' }
}

# ---- 3. 跨侧比对（有参照会话时） ----
if ($PeerSession -ne '') {
  Write-Output '--- 3) 跨侧作用域 diff ---'
  # ② 本侧会话目录空值防线：run_pair 中途失败时目录可能已建但未写 baseline.json ⇒ $sessionDir = $null。
  # 旧实现直接 `Join-Path $sessionDir.FullName …`，在 $ErrorActionPreference='Stop' 下抛异常中断、
  # 不产出 RED 报告（fail-open）。先判空：记 failed，让流程走到第 4 步落 RED 报告。
  $sessionDirFault = Get-M3SessionDirFault -SessionDir $sessionDir
  if ($sessionDirFault) {
    Write-Output 'RED: 本侧会话目录缺失（run_pair 未产出带 baseline.json 的会话）——跨侧比对无输入，门禁不成立'
    $failed += $sessionDirFault
    $report.steps.cross_side_diff = [ordered]@{ peer = $PeerSession; exit = $null; fault = $sessionDirFault }
  } else {
    # 注意：`$sessionDir` 是 DirectoryInfo，`Join-Path` 会调 `ToString()`（只给目录名）⇒ 必须显式 FullName，
    # 否则 `scoped_diff.py` 会拿到相对路径并 `FileNotFoundError`（首次跨侧跑实测踩到）。
    $mine = Join-Path $sessionDir.FullName 'scoped_after_norm.jsonl'
    $peerDir = Join-Path (Join-Path $Here 'sessions') $PeerSession

    # ① 参照会话建角场景一致性（机判）：场景不一致时两侧不是同一输入序列，diff 结果无仲裁力。
    $scenarioGuard = Get-M3PeerScenarioGuard -PeerDir $peerDir -LocalScenario $LocalScenario
    foreach ($m in $scenarioGuard.messages) { Write-Output $m }
    $failed += @($scenarioGuard.failed)
    $report.steps.peer_scenario = [ordered]@{
      peer = $PeerSession; peer_fixed_new_chr = $scenarioGuard.peer_scenario; local_fixed_new_chr = $LocalScenario
    }

    # ④ 参照会话背包/装备开关一致性（T-14 复跑口径）：两侧必须同一组开关
    #    （step_ms/start_delay_ms/disable_autoplay）。旧产物「开着挂机」跑（无 disable_autoplay 记录）
    #    ⇒ characters.MapName/CX/CY 是随机走位值、跨侧必然出 diff ⇒ 不能当同口径 peer。
    $bagScopeGuard = Get-M3PeerBagEquipScopeGuard -PeerDir $peerDir -LocalScope $LocalBagEquipScope
    foreach ($m in $bagScopeGuard.messages) { Write-Output $m }
    $failed += @($bagScopeGuard.failed)
    $report.steps.peer_bag_equip_scope = [ordered]@{
      peer = $PeerSession; peer_scope = $bagScopeGuard.peer_scope; local_scope = $LocalBagEquipScope
    }

    # ⑤ 参照会话固定起点一致性（T-14 固定起点变体，owner 86478c55/d7798370）：两侧 pin 值必须相同
    #    （本侧钉在 baseline.json.start_pose 上；旧产物无该字段 ⇒ 本侧非 none 时判红）。
    $localStartPose = Get-M3SessionStartPose -Dir $sessionDir.FullName
    $poseGuard = Get-M3PeerStartPoseGuard -PeerDir $peerDir -LocalPose $localStartPose
    foreach ($m in $poseGuard.messages) { Write-Output $m }
    $failed += @($poseGuard.failed)
    $report.steps.peer_start_pose = [ordered]@{
      peer = $PeerSession; peer_start_pose = $poseGuard.peer_pose; local_start_pose = $localStartPose
    }

    $peer = Join-Path $peerDir 'scoped_after_norm.jsonl'
    if (-not (Test-Path $peer)) {
      Write-Output ("RED: 参照会话缺 scoped_after_norm.jsonl: {0}" -f $peer)
      $failed += 'peer_session_missing'
    } elseif ($scenarioGuard.failed.Count -gt 0 -or $bagScopeGuard.failed.Count -gt 0 -or $poseGuard.failed.Count -gt 0) {
      # 场景/开关/起点不一致 ⇒ 跳过 diff（比出来的差异不算数），失败项已记。
      Write-Output 'SKIP: 参照会话场景、背包/装备开关或固定起点不一致，跳过 scoped_diff（差异无仲裁力）'
      $report.steps.cross_side_diff = [ordered]@{ peer = $PeerSession; exit = $null; skipped = 'peer_scope_mismatch' }
    } else {
      $diffArgs = @((Join-Path $Here 'scoped_diff.py'), 'diff', $peer, $mine, '--canonical')
      if ($FixedNewChr) { $diffArgs += '--fixed-newchr' }
      & python @diffArgs
      $diffExit = $LASTEXITCODE
      $report.steps.cross_side_diff = [ordered]@{ peer = $PeerSession; exit = $diffExit; fixed_new_chr = [bool]$FixedNewChr }
      if ($diffExit -ne 0) { $failed += 'cross_side_diff_nonzero' }
    }
  }
} else {
  Write-Output 'SKIP: 未给 -PeerSession，跳过跨侧比对（单侧基线可独立跑）'
}

# ---- 4. 汇总 ----
$report.finished_at = (Get-Date).ToString('o')
$report.failed = $failed
$report.pass = ($failed.Count -eq 0)
$report | ConvertTo-Json -Depth 8 | Set-Content -Path $ReportPath -Encoding utf8
Write-Output ('M3_GATE_REPORT ' + $ReportPath)
if ($failed.Count -eq 0) {
  Write-Output 'GREEN: M3 门禁通过'
  exit 0
}
Write-Output ('RED: M3 门禁未通过，失败项: ' + ($failed -join ', '))
exit 1
