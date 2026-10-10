#requires -Version 5
<#
.SYNOPSIS
  run_m3_gate.ps1 — M3 正式门禁（一条命令）：同库同脚本对拍 + RNG 基线 + 快照 hash。

.DESCRIPTION
  设计文档 §6 M3 判据（owner 2026-10-10 决策：夹具提升为正式门禁）：
    - DB 侧：作用域内字段级 diff = 0（`scoped_diff.py diff`）
    - 基线侧：C# 基线一条命令可复跑且 hash 稳定（`rng-baseline.ps1`）
    - 两侧实现必须真的是"两侧"（见下方实现识别护栏）

  流程：
    0) **实现识别护栏**：读 7000/7100/7200 的实际监听进程
       —— `-Side rust` 时若入口仍是 C#（路径在 `E:\MirServer` 下）⇒ 立即 RED
       （否则就是拿 C# 跟自己比，门禁恒绿）
    1) `run_pair.ps1 -Side <side>`：快照 before/after（13 表 sha256）+ 作用域抽取 + 基线 json
    2) RNG 基线：
       - side=csharp：`rng-baseline.ps1`（种子注入 → 世界 RNG 流 + 掉落/经验序列落盘 + hash）
       - side=rust ：重放 C# 记录流（`cargo test -p mir2-parity-tests --test rng_replay_parity`）
    3) 跨侧比对（给了 `-PeerSession` 时）：`scoped_diff.py diff` ⇒ 必须 0
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
  [switch]$SkipBots
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..')).Path
$Stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$ReportPath = Join-Path $Here ("sessions\M3-gate-{0}-{1}.json" -f $Side, $Stamp)
$report = [ordered]@{
  side = $Side; bots = $Bots; seed = $Seed; started_at = (Get-Date).ToString('o')
  steps = [ordered]@{}
}
$failed = @()

function Get-EntryHolders {
  $list = @()
  foreach ($p in @(7000, 7100, 7200)) {
    $conn = Get-NetTCPConnection -State Listen -LocalPort $p -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $conn) {
      $list += [pscustomobject]@{ port = $p; process = '(无监听)'; path = ''; impl = 'none' }
      continue
    }
    $proc = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue
    $impl = 'rust-or-other'
    if ($proc -and $proc.Path -like 'E:\MirServer\*') { $impl = 'csharp' }
    $pname = if ($proc) { $proc.ProcessName } else { '(未知)' }
    $ppath = if ($proc) { $proc.Path } else { '' }
    $list += [pscustomobject]@{ port = $p; process = $pname; path = $ppath; impl = $impl }
  }
  return $list
}

# ---- 0. 实现识别护栏 ----
Write-Output '--- 0) 实现识别护栏 ---'
$holders = Get-EntryHolders
foreach ($h in $holders) { Write-Output ("ENTRY {0} <- {1} [{2}] {3}" -f $h.port, $h.process, $h.impl, $h.path) }
$report.entry_holders = $holders
$impls = @($holders | ForEach-Object { $_.impl } | Sort-Object -Unique)
if ($impls -contains 'none') {
  Write-Output 'RED: 入口端口不全在听——服务端没起全，门禁不成立'
  $failed += 'entry_ports_not_listening'
} else {
  $report.entry_impl = $impls -join '+'
  if ($Side -eq 'rust' -and ($impls -ne @('rust-or-other'))) {
    Write-Output 'RED: -Side rust 但入口仍是 C# 实现（E:\MirServer）——这是拿 C# 跟自己比，门禁会恒绿'
    Write-Output '     先把 7000/7100/7200 交给 M1 的 Rust 实现，再跑本命令'
    $failed += 'rust_side_but_csharp_holds_entry'
  }
  if ($Side -eq 'csharp' -and ($impls -ne @('csharp'))) {
    Write-Output 'RED: -Side csharp 但入口不是 E:\MirServer 的 C# 实现'
    $failed += 'csharp_side_but_entry_is_not_csharp'
  }
  if ($impls -contains 'csharp' -and $impls -contains 'rust-or-other') {
    Write-Output 'RED: 入口端口被两种实现混着占（部分端口 C#、部分非 C#）——先统一'
    $failed += 'mixed_implementations_on_entries'
  }
}

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
& powershell -ExecutionPolicy Bypass -File (Join-Path $Here 'run_pair.ps1') @pairArgs | Tee-Object -FilePath (Join-Path $Here ("sessions\gate-{0}-{1}.pair.log" -f $Side, $Stamp))
$pairExit = $LASTEXITCODE
$sessionDir = (Get-ChildItem (Join-Path $Here 'sessions') -Directory -Filter ("{0}-*" -f $Side) |
               Sort-Object LastWriteTime -Descending | Select-Object -First 1)
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
  $mine = Join-Path $sessionDir 'scoped_after_norm.jsonl'
  $peer = Join-Path (Join-Path $Here 'sessions') (Join-Path $PeerSession 'scoped_after_norm.jsonl')
  if (-not (Test-Path $peer)) {
    Write-Output ("RED: 参照会话缺 scoped_after_norm.jsonl: {0}" -f $peer)
    $failed += 'peer_session_missing'
  } else {
    & python (Join-Path $Here 'scoped_diff.py') diff $peer $mine
    $diffExit = $LASTEXITCODE
    $report.steps.cross_side_diff = [ordered]@{ peer = $PeerSession; exit = $diffExit }
    if ($diffExit -ne 0) { $failed += 'cross_side_diff_nonzero' }
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
