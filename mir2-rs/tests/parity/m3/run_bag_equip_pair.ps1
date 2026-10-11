#requires -Version 5
<#
.SYNOPSIS
  run_bag_equip_pair.ps1 — M3 T-14「双侧同序列」背包/装备对拍的一条命令（卡 openmir2-m3-bag-equip-alignment）。

.DESCRIPTION
  目标（卡内 DoD ②）：把**同一操作序列**（拾取 → 装备 → 卸下 → 丢弃 → 使用药品，两侧同一串 CM_* 字节）
  分别打在 C# 栈与 Rust 栈上 ⇒ `scoped_diff.py diff --canonical --fixed-newchr` 作用域内字段级 diff = 0。

  本脚本**不新造编排**：一次「侧运行」直接复用 `run_pair.ps1`（快照 before/after + 作用域抽取 + 基线），
  本脚本只加三件：
    ① 入口实现识别护栏（-Side 与 7000/7100/7200/5000 的实际监听进程必须对应）；
    ② `-BagEquipSequence` 透传 + `M3BAGSEQ` 标记判据（序列没跑完整 ⇒ RED，绝不当 GREEN）；
    ③ 跨侧 `scoped_diff.py` 口径与逐步判红口径（见下）。

  同序列驱动 = BotSrv 的固定背包/装备序列（`tools/botload/run_bots.ps1 -BagEquipSequence`，
  白名单 T-22，夹具专用、默认关）：本假人收到自己的 `SM_LOGON` 后按固定顺序各发一条
  `CM_PICKUP/CM_TAKEONITEM/CM_TAKEOFFITEM/CM_DROPITEM/CM_EAT`，参数从**本假人的客户端视图**取。
  两侧服务端行为相同时，这串 CM_* 字节逐条相同 —— 这就是 M3「两侧同一序列」。

  Mode：
    -Side csharp|rust [可选参数]     跑一侧基线（复用 run_pair.ps1）。
    -Side rust -PeerSession <C#会话> 跑 Rust 侧并跨侧 diff（= 本卡的正式判据）。
    -DiffOnly -CsharpSession <名> -RustSession <名>  只重比既有产物（**不碰端口**）。
    -SelfCheckNoStack                无栈自检：所有路径都必须 fail-closed（PREREQ/RED），缺栈绝不静默 GREEN。

  判红口径（逐步；任一不满足即退出码 != 0）：
    0 入口护栏   -Side rust 而入口监听进程路径仍是 `E:\MirServer\*`（C#）⇒ RED（否则拿 C# 比 C# 恒绿）；
                 -Side csharp 而入口不是 C# ⇒ RED；端口被两种实现混占 ⇒ RED。
    1 预检       入口端口不全在听 / MySQL 3306 不可读 / 前缀不清新 ⇒ **PREREQ（exit 2）**，不静默通过。
    2 同序列     run_pair -BagEquipSequence 后 bot.log 必须出现**整串** `M3BAGSEQ`（每步一条 + `done`）；
                 缺任一步或没 `done` ⇒ RED（run_pair 内已判，本脚本再核 baseline.json 的 bag_equip_markers）。
    3 跨侧       `scoped_diff.py diff --canonical --fixed-newchr` ⇒ 非 0 即 RED（差异要么修到 0，要么按 §6.0-3 登记白名单）。

  窗口纪律：本脚本除 -DiffOnly/-SelfCheckNoStack 外**都会连真栈**（5000/7000/7100/7200、MySQL 3306）。
  起两侧栈前先看 `E:\MirServer\port-status.ps1`，与 M2 真机验收严格串行（**别并行起栈**）。
  本脚本**不启动/不停止任何服务**——栈的起停仍由既有脚本/人工负责（与 run_m3_gate.ps1 同口径）。

.EXAMPLE
  # ① C# 基线（C# 栈在听；仅 1 个假人，固定建角 + 固定背包序列）
  powershell -ExecutionPolicy Bypass -File run_bag_equip_pair.ps1 -Side csharp -Bots 1
  # ② 换成 Rust 栈后跑 Rust 侧并与上面的 C# 会话对拍
  powershell -ExecutionPolicy Bypass -File run_bag_equip_pair.ps1 -Side rust -PeerSession csharp-20261012-XXXXXX
  # ③ 只重比既有产物（不碰端口）
  powershell -ExecutionPolicy Bypass -File run_bag_equip_pair.ps1 -DiffOnly -CsharpSession csharp-... -RustSession rust-...
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_bag_equip_pair.ps1 -SelfCheckNoStack
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [int]$Bots = 1,
  [string]$Prefix = '',
  [string]$Address = '127.0.0.1',
  [int]$Port = 7000,
  [int]$TimeoutSec = 600,
  [string]$PeerSession = '',
  [string]$CsharpSession = '',
  [string]$RustSession = '',
  [int]$BagEquipStepMs = 1000,
  [int]$BagEquipStartDelayMs = 3000,
  [string]$PortsCsv = '',          # 入口端口（逗号分隔）；默认 5000,7000,7100,7200。自检时可指一列空闲端口
  [switch]$DiffOnly,
  [switch]$SelfCheckNoStack
)

$ErrorActionPreference = 'Stop'
$Ports = if ([string]::IsNullOrEmpty($PortsCsv)) {
  @(5000, 7000, 7100, 7200)
} else {
  @($PortsCsv.Split(',') | ForEach-Object { [int]$_.Trim() })
}
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..')).Path
$RunPair = Join-Path $Here 'run_pair.ps1'
$Scoped = Join-Path $Here 'scoped_diff.py'
$RunBots = Join-Path $RepoRoot 'tools\botload\run_bots.ps1'
$Sessions = Join-Path $Here 'sessions'
$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'

function Get-EntryHolders {
  $list = @()
  foreach ($p in $Ports) {
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

function Test-MysqlTcp {
  try {
    $c = New-Object System.Net.Sockets.TcpClient
    $iar = $c.BeginConnect('127.0.0.1', 3306, $null, $null)
    $ok = $iar.AsyncWaitHandle.WaitOne(1500)
    $c.Close()
    return $ok
  } catch { return $false }
}

function Newest-Session([string]$side) {
  $dirs = @(Get-ChildItem $Sessions -Directory -Filter ("{0}-*" -f $side) -ErrorAction SilentlyContinue |
            Where-Object { Test-Path (Join-Path $_.FullName 'baseline.json') } |
            Sort-Object LastWriteTime -Descending)
  if ($dirs.Count -gt 0) { return $dirs[0] } else { return $null }
}

function Invoke-CrossSideDiff([string]$csharpDir, [string]$rustDir) {
  $a = Join-Path $csharpDir 'scoped_after_norm.jsonl'
  $b = Join-Path $rustDir 'scoped_after_norm.jsonl'
  if (-not (Test-Path $a)) { Write-Output ("PREREQ_MISSING: C# 会话缺 scoped_after_norm.jsonl -> {0}" -f $a); return 2 }
  if (-not (Test-Path $b)) { Write-Output ("PREREQ_MISSING: Rust 会话缺 scoped_after_norm.jsonl -> {0}" -f $b); return 2 }
  Write-Output '--- 跨侧作用域字段级 diff（--canonical --fixed-newchr） ---'
  & python $Scoped diff $a $b --canonical --fixed-newchr
  if ($LASTEXITCODE -eq 0) { Write-Output 'GREEN: 跨侧作用域内字段级 diff = 0'; return 0 }
  Write-Output 'RED: 跨侧 diff != 0（差异要么修到 0，要么按 §6.0-3 登记 tests/parity/whitelist.md）'
  return 1
}

# ===========================================================================
# Mode: SelfCheckNoStack —— 无栈自检（机器可判，不碰真端口）
#   必须全绿（= 每条 fail-closed 路径都按预期失败），否则说明判据失效。
# ===========================================================================
if ($SelfCheckNoStack) {
  Write-Output 'SELFTEST_NO_STACK: 无栈自检——所有路径都必须 fail-closed（缺失绝不静默 GREEN）'
  $failures = @()

  # ① 配置透传（T-22）：-BagEquipSequence 必须落进 AppSetting.json；不带开关时必须为 false。
  $cfgPath = Join-Path $RepoRoot 'src\BotSrv\bin\Release\AppSetting.json'
  & powershell -ExecutionPolicy Bypass -File $RunBots -BagEquipSequence -WriteConfigOnly -SkipBuild | Out-Null
  if ($LASTEXITCODE -ne 0) {
    $failures += 'config_on_writerc=non-zero'
  } else {
    $on = (Get-Content $cfgPath -Raw | ConvertFrom-Json).BotPlay.BagEquipSequence
    if ($on -ne $true) { $failures += 'bag_equip_sequence_not_written_when_on' }
    Write-Output ("SELFTEST ① 配置透传 on=true? {0}" -f $on)
  }
  & powershell -ExecutionPolicy Bypass -File $RunBots -WriteConfigOnly -SkipBuild | Out-Null
  $off = (Get-Content $cfgPath -Raw | ConvertFrom-Json).BotPlay.BagEquipSequence
  if ($off -ne $false) { $failures += 'bag_equip_sequence_default_not_off' }
  Write-Output ("SELFTEST ② 默认关 off=false? {0}" -f $off)

  # ③ 入口护栏 fail-closed：一列空闲端口（无监听）⇒ 必须 PREREQ（exit 2），不能静默 GREEN。
  & powershell -ExecutionPolicy Bypass -File $MyInvocation.MyCommand.Path -Side rust -PortsCsv 17991,17992,17993,17994 | Out-Null
  $rcPort = $LASTEXITCODE
  if ($rcPort -ne 2) { $failures += ("no_stack_port_guard_rc=$rcPort(expect 2)") }
  Write-Output ("SELFTEST ③ 无栈入口护栏 rc={0}（期望 2=PREREQ）" -f $rcPort)

  # ④ 只重比既有产物：会话不存在 ⇒ 必须 PREREQ（exit 2）。
  & powershell -ExecutionPolicy Bypass -File $MyInvocation.MyCommand.Path -DiffOnly -CsharpSession '__none__' -RustSession '__none__' | Out-Null
  $rcDiff = $LASTEXITCODE
  if ($rcDiff -ne 2) { $failures += ("diffonly_missing_sessions_rc=$rcDiff(expect 2)") }
  Write-Output ("SELFTEST ④ -DiffOnly 缺会话 rc={0}（期望 2=PREREQ）" -f $rcDiff)

  if ($failures.Count -eq 0) {
    Write-Output 'SELFTEST_RED GREEN: 无栈自检全过（配置透传有/无两向 + 入口护栏 fail-closed + DiffOnly fail-closed）'
    exit 0
  }
  Write-Output ('SELFTEST_RED RED: ' + ($failures -join ', '))
  exit 1
}

# ===========================================================================
# Mode: DiffOnly —— 只重比既有产物（不碰端口/库）
# ===========================================================================
if ($DiffOnly) {
  if ([string]::IsNullOrEmpty($CsharpSession) -or [string]::IsNullOrEmpty($RustSession)) {
    Write-Output 'PREREQ_MISSING: -DiffOnly 需要 -CsharpSession <名> 与 -RustSession <名>'
    exit 2
  }
  $cd = Join-Path $Sessions $CsharpSession
  $rd = Join-Path $Sessions $RustSession
  if (-not (Test-Path $cd)) { Write-Output ("PREREQ_MISSING: 找不到 C# 会话目录 {0}" -f $cd); exit 2 }
  if (-not (Test-Path $rd)) { Write-Output ("PREREQ_MISSING: 找不到 Rust 会话目录 {0}" -f $rd); exit 2 }
  exit (Invoke-CrossSideDiff $cd $rd)
}

# ===========================================================================
# Mode: 侧运行（csharp / rust）
# ===========================================================================
if ([string]::IsNullOrEmpty($Prefix)) {
  $Prefix = 'm3b' + (Get-Date -Format 'HHmmss')
}

Write-Output '--- 0) 入口实现识别护栏 ---'
$holders = Get-EntryHolders
foreach ($h in $holders) { Write-Output ("ENTRY {0} <- {1} [{2}] {3}" -f $h.port, $h.process, $h.impl, $h.path) }
$impls = @($holders | ForEach-Object { $_.impl } | Sort-Object -Unique)
if ($impls -contains 'none') {
  Write-Output 'PREREQ_MISSING: 入口端口不全在听（5000/7000/7100/7200）——先起全栈；本脚本不起服务'
  exit 2
}
if ($Side -eq 'rust' -and ($impls -ne @('rust-or-other'))) {
  Write-Output 'RED: -Side rust 但入口仍是 C#（E:\MirServer）——这是拿 C# 跟 C# 比，门禁恒绿'
  exit 1
}
if ($Side -eq 'csharp' -and ($impls -ne @('csharp'))) {
  Write-Output 'RED: -Side csharp 但入口不是 E:\MirServer 的 C# 实现'
  exit 1
}
if ($impls -contains 'csharp' -and $impls -contains 'rust-or-other') {
  Write-Output 'RED: 入口端口被两种实现混占（部分 C#、部分非 C#）——先统一'
  exit 1
}

Write-Output '--- 1) 预检：MySQL / 前缀 ---'
if (-not (Test-MysqlTcp)) {
  Write-Output 'PREREQ_MISSING: MySQL 127.0.0.1:3306 不可达——dbsnap 取不到快照'
  exit 2
}
Write-Output 'PREFLIGHT MySQL 在听'

Write-Output '--- 2) 侧运行（复用 run_pair.ps1；同序列 = -FixedNewChr + -BagEquipSequence） ---'
$pairArgs = @('-Side', $Side, '-Bots', $Bots, '-Prefix', $Prefix, '-Address', $Address, '-Port', $Port,
              '-TimeoutSec', $TimeoutSec, '-FixedNewChr', '-BagEquipSequence',
              '-BagEquipStepMs', $BagEquipStepMs, '-BagEquipStartDelayMs', $BagEquipStartDelayMs)
& powershell -ExecutionPolicy Bypass -File $RunPair @pairArgs
$pairExit = $LASTEXITCODE
if ($pairExit -ne 0) {
  Write-Output ("RED: run_pair（{0} 侧）未通过（rc={1}）——同序列/快照/抽取未成立" -f $Side, $pairExit)
  exit 1
}

$mine = Newest-Session $Side
if ($null -eq $mine) {
  Write-Output ("RED: 找不到 {0} 侧新会话目录（缺 baseline.json）" -f $Side)
  exit 1
}
Write-Output ("SESSION_{0} {1}" -f $Side.ToUpper(), $mine.Name)

# 同序列判据复核：baseline.json 的 bag_equip_markers 必须含整串（含 done）。
$baseline = Get-Content (Join-Path $mine.FullName 'baseline.json') -Raw | ConvertFrom-Json
$markers = @($baseline.bag_equip_markers)
$hasDone = @($markers | Where-Object { $_ -match 'M3BAGSEQ done' }).Count -ge $Bots
if ($markers.Count -eq 0 -or -not $hasDone) {
  Write-Output ("RED: 同序列判据不成立（bag_equip_markers={0} 条，done={1}）——驱动没跑完整串" -f $markers.Count, $hasDone)
  exit 1
}
Write-Output ("BAGSEQ_OK markers={0} done=True" -f $markers.Count)

# 跨侧 diff
if ($Side -eq 'rust' -and -not [string]::IsNullOrEmpty($PeerSession)) {
  $peer = Join-Path $Sessions $PeerSession
  if (-not (Test-Path $peer)) { Write-Output ("PREREQ_MISSING: 找不到 C# 对照会话 {0}" -f $peer); exit 2 }
  exit (Invoke-CrossSideDiff $peer $mine.FullName)
}

if ($Side -eq 'csharp') {
  Write-Output ("NEXT: 换 Rust 栈后跑  run_bag_equip_pair.ps1 -Side rust -PeerSession {0}" -f $mine.Name)
} else {
  Write-Output 'NOTE: 未给 -PeerSession，跳过跨侧 diff（-Side rust 的正式判据要 C# 对照会话）'
}
Write-Output 'GREEN: 侧运行与同序列判据通过'
exit 0
