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

  同序列驱动 = BotSrv 的固定背包/装备序列（`tools/botload/run_bots.ps1 -BagEquipSequence -DisableAutoPlay`，
  白名单 T-22/M1-EXIT-1，夹具专用、默认关）：本假人收到自己的 `SM_LOGON` 后按固定顺序各发一条
  `CM_PICKUP/CM_TAKEONITEM/CM_TAKEOFFITEM/CM_DROPITEM/CM_EAT`，参数从**本假人的客户端视图**取。
  两侧服务端行为相同时，这串 CM_* 字节逐条相同 —— 这就是 M3「两侧同一序列」。
  `-BagEquipSequence` 由 `run_pair.ps1` 自动带上 `-DisableAutoPlay`（两侧同开关；见 baseline.json 的
  `bag_equip_sequence` 记录）——不关挂机则假人随机走位，`characters.CX/CY` 两侧必然不同。

  前置（T-27，owner 2026-10-12 06:40）：Rust game-svr **尚无断线/进世界存档**（`bins/game-svr` 0 处
  请求常量 `DB_SAVEHUMANRCD(101)`；`dbclient.rs` 只有 load 构造器）⇒ 跑下去的 after 快照必是建角
  状态、跨侧 diff 必红（与 T-26 三条**正交**）。`-Side rust` 会先做机判 preflight（未实现 ⇒ PREREQ
  exit 2），避免白占窗口；承接卡 `openmir2-m3-gamesvr-save`（issue `f6893ea2`）。

  Mode：
    -Side csharp|rust [可选参数]     跑一侧基线（复用 run_pair.ps1）。
    -Side rust -PeerSession <C#会话> 跑 Rust 侧并跨侧 diff（= 本卡的正式判据）。
    -DiffOnly -CsharpSession <名> -RustSession <名>  只重比既有产物（**不碰端口**）。
    -SelfCheckNoStack                无栈自检：所有路径都必须 fail-closed（PREREQ/RED），缺栈绝不静默 GREEN。
                                     ⑩ 含 **pin settle+hold 的可判红自检**（owner 2026-10-12 08:20）：故障注入驱动
                                     生产同一函数 `gate_guards.ps1::Invoke-M3PinSettleHold` —— 跳过 settle、
                                     pin 早于异步落库、存档反复覆盖 ⇒ 必判红；并机判 `run_pair -EmitOpsArgs`
                                     的 `pin_instrument`（`settle_seconds>=1` 且 `consecutive>=2`）。

  判红口径（逐步；任一不满足即退出码 != 0）：
    0 入口护栏   -Side rust 而入口监听进程路径仍是 `E:\MirServer\*`（C#）⇒ RED（否则拿 C# 比 C# 恒绿）；
                 -Side csharp 而入口不是 C# ⇒ RED；端口被两种实现混占 ⇒ RED。
    0.5 preflight `-Side rust` 时机判 Rust game-svr 是否已实现断线存档（whitelist T-27）：未实现
                 ⇒ **PREREQ（exit 2）**（否则 after 快照必是建角状态、diff 必红，白占窗口）。
    1 预检       入口端口不全在听 / MySQL 3306 不可读 / 前缀不清新 ⇒ **PREREQ（exit 2）**，不静默通过。
    2 同序列     run_pair -BagEquipSequence 后 bot.log 必须出现**整串** `M3BAGSEQ`（每步一条 + `done`）；
                 缺任一步或没 `done` ⇒ RED（run_pair 内已判，本脚本再核 baseline.json 的 bag_equip_markers）。
    2.5 口径     两侧 baseline.json 的 `bag_equip_sequence` 必须**同一组开关**（step_ms/start_delay_ms/
                 disable_autoplay）——`-BagEquipSequence` 会一并关客户端挂机（`DisableAutoPlay`）；
                 挂机是无种子 RNG 随机走位，不关则 `characters.MapName/CX/CY` 逐字节比**必然**出 diff。
                 旧「开着挂机」产物（如 `csharp-20261012-054849`，无 `disable_autoplay` 记录）不能当
                 同口径 peer ⇒ PREREQ（exit 2），C# 基线必须按新开关重跑。
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
  [switch]$PinStartPose,           # T-14 固定起点变体（透传给 run_pair；owner 86478c55/d7798370）
  [switch]$SkipSavePreflight,      # 仅供调试：跳过「Rust 侧断线存档未实现」前置（默认不跳）
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
# 背包/装备开关口径与 peer 护栏的唯一实现（与 run_m3_gate / selftest_gate 共用，自 dot-source）。
. (Join-Path $Here 'gate_guards.ps1')

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

# preflight（owner 2026-10-12 06:40 / whitelist T-27）：bins/game-svr 目前只 load 不 save
# （dbclient.rs 只有 load_human_rcd_request）⇒ 断线后 after 快照仍是建角状态、跨侧 diff 必红。
# 机判：game-svr 源码里必须出现**请求**常量 DB_SAVEHUMANRCD(101)（DBR_ 回包不算，前缀不是 DB_）。
# 承接卡 openmir2-m3-gamesvr-save（issue f6893ea2）。
function Test-RustDisconnectSaveImplemented {
  param([string]$SrcDir = '')
  if ([string]::IsNullOrEmpty($SrcDir)) { $SrcDir = Join-Path $RepoRoot 'mir2-rs\bins\game-svr\src' }
  if (-not (Test-Path $SrcDir)) { return $false }
  $hits = @(Get-ChildItem $SrcDir -Recurse -Filter '*.rs' -ErrorAction SilentlyContinue |
            Select-String -Pattern 'DB_SAVEHUMANRCD' -SimpleMatch -ErrorAction SilentlyContinue)
  return ($hits.Count -gt 0)
}

# CrossSideDiff 的退出码（脚本作用域）：不能把带 Write-Output 的函数直接写进 `exit (...)`——
# 括起来的子表达式会把输出流全部吃掉，exit 拿到数组/字符串 ⇒ 退出码变 0、消息也不打印（实跑踩到）。
$script:CrossSideDiffRc = 2
function Invoke-CrossSideDiff([string]$csharpDir, [string]$rustDir, [string]$LocalScope, [string]$LocalPose) {
  $script:CrossSideDiffRc = 2
  $a = Join-Path $csharpDir 'scoped_after_norm.jsonl'
  $b = Join-Path $rustDir 'scoped_after_norm.jsonl'
  if (-not (Test-Path $a)) { Write-Output ("PREREQ_MISSING: C# 会话缺 scoped_after_norm.jsonl -> {0}" -f $a); return }
  if (-not (Test-Path $b)) { Write-Output ("PREREQ_MISSING: Rust 会话缺 scoped_after_norm.jsonl -> {0}" -f $b); return }
  # peer 口径护栏（T-14 复跑前提）：两侧必须**同一组开关**（step_ms/start_delay_ms/disable_autoplay）。
  # 旧产物（如 csharp-20261012-054849）是「开着挂机」跑的——无 `disable_autoplay=` 记录 ⇒
  # 其 characters.MapName/CX/CY 是随机走位值，不能当同口径 peer。
  $scopeGuard = Get-M3PeerBagEquipScopeGuard -PeerDir $csharpDir -LocalScope $LocalScope
  foreach ($m in $scopeGuard.messages) { Write-Output $m }
  if (@($scopeGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧背包/装备开关不同口径（{0}）——先按同一组开关重跑基线（-BagEquipSequence 会自动带 -DisableAutoPlay）" -f (@($scopeGuard.failed) -join ','))
    return
  }
  Write-Output ("BAGEQUIP_SCOPE_OK peer='{0}' local='{1}'" -f $scopeGuard.peer_scope, $LocalScope)
  # 固定起点护栏（T-14 固定起点变体）：两侧 pin 值必须逐字节相同（否则位置列 diff 无仲裁力）。
  $poseGuard = Get-M3PeerStartPoseGuard -PeerDir $csharpDir -LocalPose $LocalPose
  foreach ($m in $poseGuard.messages) { Write-Output $m }
  if (@($poseGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧固定起点不同口径（{0}）——先按同一组 pin 值重跑基线" -f (@($poseGuard.failed) -join ','))
    return
  }
  Write-Output ("STARTPOSE_OK peer='{0}' local='{1}'" -f $poseGuard.peer_pose, $LocalPose)
  Write-Output '--- 跨侧作用域字段级 diff（--canonical --fixed-newchr） ---'
  & python $Scoped diff $a $b --canonical --fixed-newchr
  if ($LASTEXITCODE -eq 0) { Write-Output 'GREEN: 跨侧作用域内字段级 diff = 0'; $script:CrossSideDiffRc = 0; return }
  Write-Output 'RED: 跨侧 diff != 0（差异要么修到 0，要么按 §6.0-3 登记 tests/parity/whitelist.md）'
  $script:CrossSideDiffRc = 1
}

# ===========================================================================
# Mode: SelfCheckNoStack —— 无栈自检（机器可判，不碰真端口）
#   必须全绿（= 每条 fail-closed 路径都按预期失败），否则说明判据失效。
# ===========================================================================
if ($SelfCheckNoStack) {
  Write-Output 'SELFTEST_NO_STACK: 无栈自检——所有路径都必须 fail-closed（缺失绝不静默 GREEN）'
  $failures = @()

  # ① 配置透传（T-22 + M1-EXIT-1）：-BagEquipSequence/-DisableAutoPlay 必须落进 AppSetting.json。
  $cfgPath = Join-Path $RepoRoot 'src\BotSrv\bin\Release\AppSetting.json'
  & powershell -ExecutionPolicy Bypass -File $RunBots -BagEquipSequence -DisableAutoPlay -WriteConfigOnly -SkipBuild | Out-Null
  if ($LASTEXITCODE -ne 0) {
    $failures += 'config_on_writerc=non-zero'
  } else {
    $on = (Get-Content $cfgPath -Raw | ConvertFrom-Json).BotPlay
    if ($on.BagEquipSequence -ne $true) { $failures += 'bag_equip_sequence_not_written_when_on' }
    if ($on.DisableAutoPlay -ne $true) { $failures += 'disable_autoplay_not_written_when_on' }
    Write-Output ("SELFTEST ① 配置透传 on=true? BagEquipSequence={0} DisableAutoPlay={1}" -f $on.BagEquipSequence, $on.DisableAutoPlay)
  }
  & powershell -ExecutionPolicy Bypass -File $RunBots -WriteConfigOnly -SkipBuild | Out-Null
  $off = (Get-Content $cfgPath -Raw | ConvertFrom-Json).BotPlay
  if ($off.BagEquipSequence -ne $false) { $failures += 'bag_equip_sequence_default_not_off' }
  if ($off.DisableAutoPlay -ne $false) { $failures += 'disable_autoplay_default_not_off' }
  Write-Output ("SELFTEST ② 默认关 off=false? BagEquipSequence={0} DisableAutoPlay={1}" -f $off.BagEquipSequence, $off.DisableAutoPlay)

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

  # ⑤ run_pair 的开关耦合（T-14 复跑口径，owner 2026-10-12 06:36）：
  #    -BagEquipSequence 必须**一并传 -DisableAutoPlay**；默认（不带该开关）时**不得**出现。
  #    这是「默认关 ⇒ 原路径逐字节不变」的两向自检（run_pair 的 -EmitOpsArgs 无副作用）。
  $emitOnRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -BagEquipSequence -BagEquipStepMs 1000 -BagEquipStartDelayMs 3000
  if ($LASTEXITCODE -ne 0) { $failures += 'emit_on_rc=non-zero' }
  else {
    $emitOn = (($emitOnRaw | Out-String).Trim() | ConvertFrom-Json)
    $has = (@($emitOn.bot_args) -contains '-DisableAutoPlay')
    if (-not $has) { $failures += 'bag_equip_sequence_does_not_pass_disable_autoplay' }
    if ($emitOn.ops_runner -notmatch '-DisableAutoPlay') { $failures += 'ops_runner_missing_disable_autoplay' }
    if ($emitOn.bag_equip_scope -ne 'on step_ms=1000 start_delay_ms=3000 disable_autoplay=True') { $failures += ('bag_equip_scope_on=' + $emitOn.bag_equip_scope) }
    Write-Output ("SELFTEST ⑤a run_pair -BagEquipSequence 传 -DisableAutoPlay? {0}（scope={1}）" -f $has, $emitOn.bag_equip_scope)
  }
  $emitOffRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs
  if ($LASTEXITCODE -ne 0) { $failures += 'emit_off_rc=non-zero' }
  else {
    $emitOff = (($emitOffRaw | Out-String).Trim() | ConvertFrom-Json)
    $hasOff = (@($emitOff.bot_args) -contains '-DisableAutoPlay')
    if ($hasOff) { $failures += 'default_path_unexpectedly_passes_disable_autoplay' }
    if ($emitOff.bag_equip_scope -ne '(off)') { $failures += ('bag_equip_scope_off=' + $emitOff.bag_equip_scope) }
    Write-Output ("SELFTEST ⑤b 默认路径不带 -DisableAutoPlay? {0}（scope={1}）" -f (-not $hasOff), $emitOff.bag_equip_scope)
  }

  # ⑥ peer 口径护栏有牙齿（断言，不止说明）：入库的「开着挂机」C# 基线 csharp-20261012-054849
  #    （只有 `on step_ms=1000 start_delay_ms=3000`、无 disable_autoplay 记录）**不能**当同口径 peer。
  $legacyPeer = Join-Path $Sessions 'csharp-20261012-054849'
  if (-not (Test-Path (Join-Path $legacyPeer 'baseline.json'))) {
    $failures += 'legacy_peer_session_missing'
  } else {
    $g = Get-M3PeerBagEquipScopeGuard -PeerDir $legacyPeer -LocalScope 'on step_ms=1000 start_delay_ms=3000 disable_autoplay=True'
    $codes = @($g.failed)
    if (-not ($codes -contains 'peer_bag_equip_autoplay_on')) { $failures += ('legacy_peer_not_rejected_codes=' + ($codes -join ',')) }
    Write-Output ("SELFTEST ⑥ 旧挂机 peer 被判红? {0}（{1}）" -f ($codes -contains 'peer_bag_equip_autoplay_on'), ($codes -join ','))
  }

  # ⑦ 断言「characters.MapName/CX/CY 逐字节比」——这就是 T-14 复跑必须关挂机的原因。
  #    scoped_diff.py selftest 14/14 里有一条改坏 CX 即判红的用例，本条把该牙齿纳入无栈自检。
  #    注意：selftest 有意把红检用例的 `RED:` 打到 stderr ⇒ 此处临时 Continue，避免 Stop 把它当终止错误。
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $scopedOut = (& python $Scoped selftest 2>&1 | Out-String)
  $scopedRc = $LASTEXITCODE
  $ErrorActionPreference = $prevEap
  if ($scopedRc -ne 0) { $failures += ("scoped_diff_selftest_rc=$scopedRc") }
  if ($scopedOut -notmatch 'selftest 14/14') { $failures += 'scoped_diff_selftest_cx_case_missing' }
  Write-Output ("SELFTEST ⑦ scoped_diff selftest rc={0}（含 MapName/CX/CY 逐字节比断言）" -f $scopedRc)

  # ⑧ preflight（T-27）有牙齿：空目录 ⇒ false；写了请求常量 DB_SAVEHUMANRCD ⇒ true；
  #    只有回包常量 DBR_SAVEHUMANRCD ⇒ 不算（前缀不是 DB_）。
  $tmpSrc = Join-Path $env:TEMP ('m3bag-save-probe-' + (Get-Date -Format 'HHmmssfff'))
  New-Item -ItemType Directory -Force -Path $tmpSrc | Out-Null
  if (Test-RustDisconnectSaveImplemented -SrcDir $tmpSrc) { $failures += 'save_preflight_empty_dir_true' }
  Set-Content -Path (Join-Path $tmpSrc 'x.rs') -Value 'const DB_SAVEHUMANRCD: u16 = 101;' -Encoding utf8
  if (-not (Test-RustDisconnectSaveImplemented -SrcDir $tmpSrc)) { $failures += 'save_preflight_hit_false' }
  Set-Content -Path (Join-Path $tmpSrc 'x.rs') -Value 'const DBR_SAVEHUMANRCD: u16 = 1102;' -Encoding utf8
  if (Test-RustDisconnectSaveImplemented -SrcDir $tmpSrc) { $failures += 'save_preflight_dbr_false_positive' }
  Remove-Item -Recurse -Force $tmpSrc -ErrorAction SilentlyContinue
  Write-Output 'SELFTEST ⑧ preflight(T-27)：空目录 false / DB_SAVEHUMANRCD true / DBR_ 回包不算'

  # ⑨ 固定起点变体（T-14，owner 86478c55/d7798370）：预热/计分两相命令 + pin SQL + 起点护栏，离线可判。
  $posedRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -FixedNewChr -BagEquipSequence -PinStartPose
  if ($LASTEXITCODE -ne 0) { $failures += 'emit_pin_rc=non-zero' }
  else {
    $posed = (($posedRaw | Out-String).Trim() | ConvertFrom-Json)
    $pre = @($posed.prewarm_bot_args)
    $score = @($posed.bot_args)
    if ($pre -notcontains '-RequireCharCreated') { $failures += 'pin_prewarm_missing_require_char_created' }
    if ($pre -notcontains '-FixedNewChr') { $failures += 'pin_prewarm_missing_fixed_newchr' }
    if ($pre -notcontains '-NewAccount') { $failures += 'pin_prewarm_missing_newaccount' }
    if ($score -contains '-FixedNewChr') { $failures += 'pin_scoring_should_not_pass_fixed_newchr' }
    if ($score -contains '-NewAccount') { $failures += 'pin_scoring_should_not_pass_newaccount' }
    if ($score -notcontains '-DisableAutoPlay') { $failures += 'pin_scoring_missing_disable_autoplay' }
    $wantPose = 'map=0 home=0,289,618 pos=289,618 level=1 dir=0 bag=48@0,97@1,207@2,3@3 abil=1'
    if ($posed.start_pose -ne $wantPose) { $failures += ('pin_start_pose=' + $posed.start_pose) }
    if ($posed.pin_sql -notmatch "^UPDATE mir2_db\.characters SET MapName='0', CX=289, CY=618, HomeMap=0, HomeX=289, HomeY=618, Level=1, Dir=0 WHERE ChrName='") { $failures += ('pin_sql=' + $posed.pin_sql) }
    # 初始背包/Abil 必须一起 provision（owner 643459a7）：pin_sql_all 至少 7 条，含 bagitem/ablity/4 件
    $all = @($posed.pin_sql_all)
    if ($all.Count -lt 7) { $failures += ('pin_sql_all_count=' + $all.Count) }
    $joined = ($all -join ' || ')
    if ($joined -notmatch 'characters_bagitem') { $failures += 'pin_sql_all_missing_bagitem' }
    if ($joined -notmatch 'characters_item') { $failures += 'pin_sql_all_missing_item' }
    if ($joined -notmatch 'characters_ablity') { $failures += 'pin_sql_all_missing_ablity' }
    if ($joined -notmatch 'StdIndex=48, b\.Dura=8000') { $failures += 'pin_sql_all_missing_candle' }
    if ($joined -notmatch 'a\.Level=1') { $failures += 'pin_sql_all_missing_abil_level' }
    Write-Output ("SELFTEST ⑨ 固定起点两相：prewarm+RequireCharCreated={0}；scoring 无 -FixedNewChr/-NewAccount={1}；pin_sql_all={2} 条（含初始背包 4 件 + Abil）" -f ($pre -contains '-RequireCharCreated'), (($score -notcontains '-FixedNewChr') -and ($score -notcontains '-NewAccount')), $all.Count)
  }
  # 起点护栏有牙齿：入库旧 peer（无 start_pose）+ 本侧 pin ⇒ 必红 missing
  if (Test-Path (Join-Path $Sessions 'csharp-20261012-054849')) {
    $pg = Get-M3PeerStartPoseGuard -PeerDir (Join-Path $Sessions 'csharp-20261012-054849') -LocalPose 'map=0 home=0,289,618 pos=289,618 level=1 dir=0 bag=48@0,97@1,207@2,3@3 abil=1'
    if (-not (@($pg.failed) -contains 'peer_start_pose_missing')) { $failures += ('legacy_peer_start_pose_not_rejected_codes=' + (@($pg.failed) -join ',')) }
    Write-Output ("SELFTEST ⑨b 旧 peer 无 start_pose + 本侧 pin 被判红? {0}" -f (@($pg.failed) -contains 'peer_start_pose_missing'))
  }

  # ⑩ pin settle+hold 仪器的**可判红自检**（owner 2026-10-12 08:20；审阅者会问「仪器有没有被削弱」）：
  #    故障注入驱动**生产同一函数** `gate_guards.ps1::Invoke-M3PinSettleHold`，断言：
  #    (a) 跳过 settle 直接 pin（Settle 返回 $false）⇒ Hold=false 且带 pin_settle_skipped；
  #    (b) pin 早于异步落库——存档在首轮复验后落库覆盖 pin ⇒ 首轮虽 match 但 hold=false
  #        （单次观测挡不住假 GREEN），重 pin 后连续两轮到位才 hold（attempt=4）；
  #    (c) 存档反复覆盖 ⇒ 5 轮 hold 不住 ⇒ Hold=false（fail-closed，起点守卫 RED）；
  #    (d) 正常路径（settle + 三项到位）⇒ attempt=2 hold（连续两轮）。
  $emptyPin = { param($ctx, $attempt) }
  $okObserve = { param($ctx, $attempt) return [pscustomobject]@{ Pinned = 1; BagPinned = 4; AbilPinned = 1 } }
  # (a) 跳过 settle 直接 pin：Settle 报告「没等」⇒ 每轮都判 pin_settle_skipped，永不 hold（假 GREEN 被拦）
  $rSkip = Invoke-M3PinSettleHold -Bots 1 -MaxAttempts 3 -Pin $emptyPin `
    -Settle { param($ctx, $attempt) return $false } -Observe $okObserve
  $skipCodes = @($rSkip.Verdicts | ForEach-Object { @($_.failed) } | Sort-Object -Unique)
  if ($rSkip.Hold) { $failures += 'pin_hold_should_fail_when_settle_skipped' }
  if (-not ($skipCodes -contains 'pin_settle_skipped')) { $failures += ('pin_settle_skipped_missing=' + ($skipCodes -join ',')) }
  Write-Output ("SELFTEST ⑩a 跳过 settle 直接 pin ⇒ Hold={0}（期望 False；代码={1}）" -f $rSkip.Hold, ($skipCodes -join ','))
  # (b) pin 早于异步落库：首轮 match 但不得 hold；重 pin 后连续两轮到齐 ⇒ attempt=4
  $rAsync = Invoke-M3PinSettleHold -Bots 1 -MaxAttempts 5 -Pin $emptyPin `
    -Settle { param($ctx, $attempt) Start-Sleep -Milliseconds 1; return $true } `
    -Observe {
      param($ctx, $attempt)
      if ($attempt -eq 1) { return [pscustomobject]@{ Pinned = 1; BagPinned = 4; AbilPinned = 1 } }   # 首轮：pin 刚写，存档未落
      if ($attempt -eq 2) { return [pscustomobject]@{ Pinned = 0; BagPinned = 0; AbilPinned = 0 } }   # 存档落库覆盖了 pin
      return [pscustomobject]@{ Pinned = 1; BagPinned = 4; AbilPinned = 1 }                            # 重 pin 后稳定
    }
  $firstHold = @($rAsync.Verdicts)[0].hold
  if ($firstHold) { $failures += 'pin_hold_single_observation_false_green' }
  if (-not ($rAsync.Hold -and $rAsync.Attempts -eq 4)) { $failures += ('pin_async_overwrite_not_converged=' + $rAsync.Attempts) }
  Write-Output ("SELFTEST ⑩b pin 早于落库：首轮 hold={0}（期望 False）；收敛 attempt={1}（期望 4）Hold={2}" -f $firstHold, $rAsync.Attempts, $rAsync.Hold)
  # (c) 存档反复覆盖 ⇒ fail-closed（绝不静默 GREEN）
  $rRepeat = Invoke-M3PinSettleHold -Bots 1 -MaxAttempts 5 -Pin $emptyPin `
    -Settle { param($ctx, $attempt) Start-Sleep -Milliseconds 1; return $true } `
    -Observe { param($ctx, $attempt) return [pscustomobject]@{ Pinned = 0; BagPinned = 0; AbilPinned = 0 } }
  if ($rRepeat.Hold) { $failures += 'pin_hold_should_fail_when_save_keeps_overwriting' }
  Write-Output ("SELFTEST ⑩c 存档反复覆盖 ⇒ Hold={0}（期望 False，fail-closed）attempts={1}" -f $rRepeat.Hold, $rRepeat.Attempts)
  # (d) 正常路径：settle + 三项到位 ⇒ 连续两轮 ⇒ attempt=2
  $rOk = Invoke-M3PinSettleHold -Bots 1 -MaxAttempts 5 -Pin $emptyPin `
    -Settle { param($ctx, $attempt) Start-Sleep -Milliseconds 1; return $true } -Observe $okObserve
  if (-not ($rOk.Hold -and $rOk.Attempts -eq 2 -and $rOk.SettleRounds -eq 2)) { $failures += ('pin_normal_path_not_hold_at_2=' + $rOk.Attempts) }
  Write-Output ("SELFTEST ⑩d 正常路径：Hold={0} attempt={1} settle_rounds={2}（期望 True/2/2）" -f $rOk.Hold, $rOk.Attempts, $rOk.SettleRounds)
  # (e) 生产接线未被削弱：run_pair -EmitOpsArgs -PinStartPose 的 pin_instrument 必须 settle_seconds>=1 且 consecutive>=2
  $instRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -FixedNewChr -BagEquipSequence -PinStartPose
  if ($LASTEXITCODE -ne 0) { $failures += 'emit_pin_instrument_rc=non-zero' }
  else {
    $inst = (($instRaw | Out-String).Trim() | ConvertFrom-Json).pin_instrument
    if ($inst -notmatch 'settle_seconds=(\d+)') { $failures += 'pin_instrument_missing_settle' }
    elseif ([int]$Matches[1] -lt 1) { $failures += ('pin_instrument_settle_too_small=' + $Matches[1]) }
    if ($inst -notmatch 'consecutive=(\d+)') { $failures += 'pin_instrument_missing_consecutive' }
    elseif ([int]$Matches[1] -lt 2) { $failures += ('pin_instrument_consecutive_too_small=' + $Matches[1]) }
    Write-Output ("SELFTEST ⑩e 生产 pin_instrument='{0}'（要求 settle_seconds>=1 且 consecutive>=2）" -f $inst)
  }

  # ⑪ pin SQL 的「别名/列存在性」无栈校验（与攻击夹具共用 gate_guards.ps1 同一口径；
  #    m3cs tick16 真机 finding：`a.IncHealth` 不属于 characters_ablity ⇒ ERROR 1054）。
  #    两向：本文件的生产 pin SQL（背包 pin）必须 0 violation；把 `c.IncHealth` 改坏成
  #    `a.IncHealth` 必须被检出。不连真库、不起栈。
  $schemaCols = Get-M3SchemaColumns -SchemaPath (Join-Path $RepoRoot 'sql\mir2_db.sql')
  $bagPinSql = @()
  $bagPinSql += Get-M3StartPosePinSql -ChrName 'm3bagfix0'
  $bagVio = @(Test-M3PinSqlColumns -SqlStatements $bagPinSql -SchemaColumns $schemaCols)
  if ($bagVio.Count -ne 0) { $failures += ('bag_pin_sql_column_violations=' + ($bagVio -join ';')) }
  $bagMut = @(Get-M3AttackPinSql -ChrName 'm3bagfix0' -CX 289 -CY 618 -Dir 2 -MapName '0' -HomeMap 0 -HomeX 289 -HomeY 618 -Level 1 -Hp 30000 |
    ForEach-Object { $_ -replace 'c\.IncHealth=0, c\.IncSpell=0', 'a.IncHealth=0, a.IncSpell=0' })
  $bagMutVio = @(Test-M3PinSqlColumns -SqlStatements $bagMut -SchemaColumns $schemaCols)
  if ($bagMutVio.Count -eq 0) { $failures += 'bag_pin_sql_column_mutation_not_red' }
  if (-not (($bagMutVio -join ';') -match 'a\.IncHealth')) { $failures += 'bag_pin_sql_column_mutation_not_diff_shaped' }
  Write-Output ("SELFTEST ⑪ pin SQL 别名/列校验：生产 statements={0} violations={1}；改坏 a.IncHealth violations={2}" -f $bagPinSql.Count, $bagVio.Count, $bagMutVio.Count)

  if ($failures.Count -eq 0) {
    Write-Output 'SELFTEST_RED GREEN: 无栈自检全过（配置透传有/无两向 + run_pair 关挂机耦合两向 + 固定起点两相 + pin settle+hold 可判红自检（跳过 settle / 早于落库 / 反复覆盖 / 正常收敛）+ 旧挂机 peer/旧起点 peer 必红 + CX/CY 逐字节比断言 + 断线存档 preflight + 入口护栏/DiffOnly fail-closed + pin SQL 别名/列校验（含改坏 a.IncHealth 必红））'
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
  # DiffOnly 没有活开关：以 Rust（本侧）会话基线里记录的开关为准，再和 C# peer 对口径。
  $localScope = Get-M3SessionBagEquipScope -Dir $rd
  if ($null -eq $localScope) { Write-Output ("PREREQ_MISSING: Rust 会话缺/坏 baseline.json，读不出背包-装备开关: {0}" -f $rd); exit 2 }
  $localPose = Get-M3SessionStartPose -Dir $rd
  if ($null -eq $localPose) { Write-Output ("PREREQ_MISSING: Rust 会话缺/坏 baseline.json，读不出固定起点: {0}" -f $rd); exit 2 }
  Invoke-CrossSideDiff $cd $rd $localScope $localPose
  exit $script:CrossSideDiffRc
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

# --- 0.5) preflight（whitelist T-27）：Rust 侧断线存档是否已实现 ---
#   Rust game-svr 现在只 load 不 save（bins/game-svr 0 处 DB_SAVEHUMANRCD）⇒ 断线后 after 快照
#   恒为建角状态、跨侧 diff 必然非 0（与 T-26 三条正交）。未实现就直接 PREREQ，别白占窗口。
Write-Output '--- 0.5) preflight：Rust 断线存档（T-27） ---'
if ($Side -eq 'rust' -and -not $SkipSavePreflight) {
  if (-not (Test-RustDisconnectSaveImplemented)) {
    Write-Output 'PREREQ_MISSING: Rust game-svr 未实现断线/进世界存档（bins/game-svr 0 处请求常量 DB_SAVEHUMANRCD(101)）——跑下去 after 快照必是建角状态、跨侧 diff 必红（whitelist T-27；承接卡 openmir2-m3-gamesvr-save issue f6893ea2；-SkipSavePreflight 仅调试用）'
    exit 2
  }
  Write-Output 'PREFLIGHT Rust 断线存档：已实现（game-svr 命中 DB_SAVEHUMANRCD）'
} else {
  Write-Output 'PREFLIGHT Rust 断线存档：跳过（-Side csharp 或 -SkipSavePreflight）'
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
if ($PinStartPose) { $pairArgs += '-PinStartPose' }
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
  # 本脚本总是 -FixedNewChr + -BagEquipSequence；run_pair 会一并传 -DisableAutoPlay（两侧同开关）。
  $localBagScope = Format-M3BagEquipScope -BagEquipSequence $true -BagEquipStepMs $BagEquipStepMs -BagEquipStartDelayMs $BagEquipStartDelayMs -DisableAutoPlay $true
  # 自查：本侧 baseline.json 记录的开关必须与实际开关一致（否则 run_pair 没把开关落进基线）。
  $ownScope = Get-M3SessionBagEquipScope -Dir $mine.FullName
  if ($ownScope -ne $localBagScope) {
    Write-Output ("RED: 本侧 baseline.json 记录的开关与实际不一致——baseline='{0}' vs 实际='{1}'（run_pair 未把开关落进基线？）" -f $ownScope, $localBagScope)
    exit 1
  }
  $localPose = Get-M3SessionStartPose -Dir $mine.FullName
  if ($null -eq $localPose) { Write-Output ("PREREQ_MISSING: 本侧会话 baseline.json 缺/坏，读不出固定起点: {0}" -f $mine.FullName); exit 2 }
  Invoke-CrossSideDiff $peer $mine.FullName $localBagScope $localPose
  exit $script:CrossSideDiffRc
}

if ($Side -eq 'csharp') {
  Write-Output ("NEXT: 换 Rust 栈后跑  run_bag_equip_pair.ps1 -Side rust -PeerSession {0}" -f $mine.Name)
} else {
  Write-Output 'NOTE: 未给 -PeerSession，跳过跨侧 diff（-Side rust 的正式判据要 C# 对照会话）'
}
Write-Output 'GREEN: 侧运行与同序列判据通过'
exit 0
