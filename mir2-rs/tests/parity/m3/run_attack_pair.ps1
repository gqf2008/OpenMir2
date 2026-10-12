#requires -Version 5
<#
.SYNOPSIS
  run_attack_pair.ps1 — M3②③「双侧同序列」攻击对拍的一条命令
  （卡 openmir2-m3-attack-pair-fixture，issue 76cc08b8）。

.DESCRIPTION
  目标（卡内 DoD）：同一 bot 固定攻击序列在 C# / Rust 两侧各跑一次 ⇒ **逐项伤害序列**
  （≥100 次攻击，按「第 k 次」对齐）+ **命中/躲闪统计** 跨侧比对；期望 diff = 0
  （差异要么修到 0、要么按 §6.0-3 登记 whitelist）。

  本脚本**不新造编排**：一次「侧运行」直接复用 `run_pair.ps1`
  （两相固定起点 pin + settle+hold、会话入库、`scoped_diff` 口径）；本脚本只加三件：
    ① 入口实现识别护栏（-Side 与 5000/7000/7100/7200 的实际监听实现必须对应）；
    ② `-AttackSequence -AttackPinPair` 透传 + `M3ATTSEQ` 标记判据（序列没跑完整 ⇒ RED）；
    ③ 攻击序列跨侧口径：`attack_diff.py align/diff`（逐 k 伤害 + hits/dodges/attacked/sum）。

  度量口径（见卡 comment；**先用 `comment` 写进卡再动手**）：
    - 伤害来源 = 受击方收到的 `SM_STRUCK` 的 `Series`（= oracle
      `MakeMessage(SM_STRUCK, ActorId, HP, MaxHP, wParam)` 的 series=本次伤害；
      `src` = `MessageBodyWL.Tag1` = 命中者 ActorId）。不用 attacker 的 `SM_HIT`（无伤害数）；
      不用 DB 快照（怪物不入库、玩家 HP 只在断线存档落库，拿不到逐次）。
    - 命中/躲闪 = 按「第 k 次」：attacker 第 k 步发出后，受击方在下一步之前收到 `src==attacker`
      的 `SM_STRUCK` ⇒ hit；否则 dodge。比较 hits/dodges/attacked/sum 四项。
    - 两侧同种子（C# 用既有 `m3/rng-hook` 影子注入；Rust game-svr 侧种子为**前置**——不在本卡写集）。

  目标接法：`-AttackPinPair` 把 attacker（账号序号 0）钉在 (PinCX,PinCY) 面向 `AttackPinDir`，
  victim（序号 1）钉在 (PinCX+1,PinCY)；victim HP/MaxHP 抬高（防 100+ 次内死亡）。
  **R-1（owner 2026-10-12 14:30）**：攻击判定取全局 `M2Share.RandomNumber`，被全图怪 AI 消耗 ⇒
  对拍场景必须**除攻击路径外零 RNG 消费者**。最简 = **整张图零刷怪**：pin 地图缺省 = `0137`（竞技场，
  `MonGen.txt` 零行、无 NPC/守卫），落点 `(17,20) dir=2`、victim `(18,20)`（均可走）。不是无怪图就
  `run_pair` 启动时 **fail-closed 拒绝跑**（`gate_guards.ps1::Test-M3MonGenEmpty` 按地图查冻结提取件
  `fixtures/mongen-map<map>.tsv` + `fixtures/mongen-provenance.tsv`，要求该图刷怪行数 == 0）；
  **R-1 refine（owner 2026-10-12 15:05）**：MonGen 之外，守卫/NPC/地图事件/任务触发也可能跑 AI ⇒ 另按
  `fixtures/ai-consumer-map<map>.tsv` + `fixtures/ai-consumer-provenance.tsv` 要求该图在
  GuardList/MerChant/Npcs/MapEvent/MapQuest 里**零引用**（`Test-M3MapNoAiConsumers`，同样 fail-closed）。
  `-PinMapName` 透传给 `run_pair` 的 pin SQL/落点口径，`-SelfCheckNoStack` ⑯/⑰ 做四向红检。
  `-AttackSequence` 让 attacker 固定发 N 次 `CM_HIT`（`Recog=MakeLong(自己坐标)`、`Tag=dir`）。

  判红口径（逐步；任一不满足即退出码 != 0）：
    0 入口护栏   -Side rust 而入口仍是 C#（E:\MirServer）⇒ RED；-Side csharp 而入口不是 C# ⇒ RED。
    0.5 preflight `-Side rust` 时机判 Rust game-svr 是否已实现断线存档（whitelist T-27）；
                 未实现 ⇒ PREREQ（exit 2）。
    0.6 种子注入断言（F-C）  -Side csharp 读 `MIR2_RNG_LOG` 的 `INJECT_OK <seed>`；-Side rust 读 game-svr
                 启动日志的 `SEED_PROBE`（另校 `RUST_SEED_INJECTED`）；未注入/种子不符 ⇒ RED（fail-closed），
                 使「没设种子的不可比红」不再隐身。
    1 预检       入口端口不全在听 / MySQL 3306 不可读 ⇒ PREREQ（exit 2），不静默通过。
    1.5 pin SQL 可执行性（F-B + F-D）  在会话内建 `TEMPORARY TABLE ... LIKE` 并把每条 `UPDATE` 重写到镜像表、
                 以固定探针行真执行一次 ⇒ mysql rc 必须 0；列挂错表（如历史 `a.IncHealth` = ERROR 1054）
                 与越界值（如 `a.Hp=100000` 写 smallint = ERROR 1264）都当场 RED，不再等到 run_pair 半途才炸；
                 临时表会话结束即消失，真实数据零改动。
    2 同序列     run_pair -AttackSequence 后 bot.log 必须出现 `M3ATTSEQ`：`role=attack` k=1..N
                 且 `role=done attacks=N`；缺 ⇒ RED。
    3 跨侧口径   两侧 baseline.json 的 `attack_sequence` / `attack_pose` / `fixed_new_chr` 必须一致；
                 不一致 ⇒ PREREQ（exit 2）。
    3.1 pin 重试账本（owner 2026-10-12 16:30）  两侧 `baseline.json.pin_attempts` 的重试次数必须均为期望值
                 （零额外重试，期望=2 = 「连续两轮 settle」的结构性最小 attempts）；任一侧超出 ⇒
                 PREREQ（exit 2，fail-closed）——重试会推移攻击序列起始的全局 RNG 位置，逐 k 不可比。
    4 逐次对拍   `attack_diff.py align` 两侧 ⇒ `attack_diff.py diff`；非 0 即 RED。
                 另 `scoped_diff.py diff --canonical --fixed-newchr`（DB 侧）非 0 即 RED。

  窗口纪律：除 -DiffOnly/-SelfCheckNoStack 外都会连真栈（5000/7000/7100/7200、MySQL 3306）。
  起栈前先看端口；与 M2 真机验收严格串行（**别并行起栈**）。本脚本不启动/不停止任何服务。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 120 -AttackPinPair
  powershell -ExecutionPolicy Bypass -File run_attack_pair.ps1 -Side rust -Bots 2 -AttackCount 120 -AttackPinPair -PeerSession csharp-...
  powershell -ExecutionPolicy Bypass -File run_attack_pair.ps1 -DiffOnly -CsharpSession csharp-... -RustSession rust-...
  powershell -ExecutionPolicy Bypass -File run_attack_pair.ps1 -SelfCheckNoStack
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [int]$Bots = 2,
  [string]$Prefix = '',
  [string]$Address = '127.0.0.1',
  [int]$Port = 7000,
  [int]$TimeoutSec = 600,
  [int]$AttackCount = 120,
  [int]$AttackStepMs = 500,
  [int]$AttackStartDelayMs = 3000,
  [int]$AttackPinDir = 2,
  [string]$PinMapName = '',        # R-1：攻击 pin 地图（必须整张零刷怪）；空 ⇒ gate_guards 的 $script:M3AttackPinMap = '0137'
  [int]$AttackPinCX = -1,          # R-1：attacker 落点 X（<0 ⇒ 用 gate_guards 的 $script:M3AttackPinCX = 17，0137 竞技场可走空地）
  [int]$AttackPinCY = -1,          # R-1：attacker 落点 Y（<0 ⇒ 用 $script:M3AttackPinCY = 20）
  [int]$AttackVictimHp = 30000,
  [string]$PeerSession = '',
  [string]$CsharpSession = '',
  [string]$RustSession = '',
  [string]$PortsCsv = '',          # 入口端口（逗号分隔）；默认 5000,7000,7100,7200。自检时可指一列空闲端口
  [switch]$AttackPinPair,          # 攻击变体：两 bot 相邻固定起点（attacker/victim）
  [int]$Seed = 42,                 # F-C：两侧必须注入的同一个种子（`MIR2_RNG_SEED`）
  [string]$CsharpSeedLog = '',     # F-C：C# rng-hook 的 `MIR2_RNG_LOG` 文件（含 `INJECT_OK <seed>`）；缺省读 $env:MIR2_RNG_LOG
  [string]$RustSeedLog = '',       # F-C：Rust game-svr 启动日志（含 `SEED_PROBE`）；缺省 <RustStackLogDir>\game-svr.out.log
  [string]$RustStackLogDir = 'E:\tmp\m2-acceptance\stack',  # T-30/`openmir2-m3-rust-stack-seed-plumb` 的默认 -LogDir
  [switch]$SkipSavePreflight,      # 仅供调试：跳过「Rust 侧断线存档未实现」前置
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
$AttackDiff = Join-Path $Here 'attack_diff.py'
$RunBots = Join-Path $RepoRoot 'tools\botload\run_bots.ps1'
$Sessions = Join-Path $Here 'sessions'
$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
# 攻击序列/起点口径与 peer 护栏的唯一实现（与 gate_guards 共用，自 dot-source）。
. (Join-Path $Here 'gate_guards.ps1')

# R-1（owner 2026-10-12 14:30）：攻击对拍必须落在**整张零刷怪**的图上（除攻击路径外零 RNG 消费者）；
# pin 地图/落点缺省取 gate_guards 的唯一来源常量（0137 竞技场 (17,20)）；调用方可用 -PinMapName/-AttackPinCX/-AttackPinCY 覆盖。
if ([string]::IsNullOrEmpty($PinMapName)) { $PinMapName = $script:M3AttackPinMap }
if ($AttackPinCX -lt 0) { $AttackPinCX = $script:M3AttackPinCX }
if ($AttackPinCY -lt 0) { $AttackPinCY = $script:M3AttackPinCY }

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

# preflight（T-27）：bins/game-svr 是否已有 DB_SAVEHUMANRCD(101) 请求常量（断线存档）。
function Test-RustDisconnectSaveImplemented {
  param([string]$SrcDir = '')
  if ([string]::IsNullOrEmpty($SrcDir)) { $SrcDir = Join-Path $RepoRoot 'mir2-rs\bins\game-svr\src' }
  if (-not (Test-Path $SrcDir)) { return $false }
  $hits = @(Get-ChildItem $SrcDir -Recurse -Filter '*.rs' -ErrorAction SilentlyContinue |
            Select-String -Pattern 'DB_SAVEHUMANRCD' -SimpleMatch -ErrorAction SilentlyContinue)
  return ($hits.Count -gt 0)
}

# 把会话的 attack_raw.jsonl 对齐成 attack_seq.jsonl（幂等；不连网）。
# F-H（m3bag 2026-10-12，第二轮 `-AttackCount 20` 真机冒烟）：函数体里 `& python …` 的 stdout 会混进
# **返回值** ⇒ `$rc = Invoke-AttackAlign` 拿到 @(<python 输出>, $LASTEXITCODE)、`$rc -ne 0` 恒真
# ⇒ C# 侧在 `ATTLIGN_OK` 成功后仍报 `RED: attack_diff align 失败（rc=ALTTIGN_OK …）`。
# 唯一口径：python 输出走 host（不回流），返回值只留 [int]$LASTEXITCODE；离线自检 ⑮ 钉住。
function Invoke-AttackAlign([string]$Dir) {
  $raw = Join-Path $Dir 'attack_raw.jsonl'
  if (-not (Test-Path $raw)) { Write-Output ("PREREQ_MISSING: 会话缺 attack_raw.jsonl -> {0}" -f $raw); return 2 }
  $seq = Join-Path $Dir 'attack_seq.jsonl'
  $out = & python $AttackDiff align $raw --out $seq 2>&1
  foreach ($line in @($out)) { Write-Host $line }
  return [int]$LASTEXITCODE
}

$script:AttackDiffRc = 2
function Invoke-AttackCrossSideDiff([string]$csharpDir, [string]$rustDir, [string]$LocalScope, [string]$LocalPose, [string]$LocalScenario) {
  $script:AttackDiffRc = 2
  # ① 场景/开关/起点口径护栏（两侧同一输入才有仲裁力）。
  $scenarioGuard = Get-M3PeerScenarioGuard -PeerDir $csharpDir -LocalScenario $LocalScenario
  foreach ($m in $scenarioGuard.messages) { Write-Output $m }
  if (@($scenarioGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧建角场景不同口径（{0}）" -f (@($scenarioGuard.failed) -join ','))
    return
  }
  $scopeGuard = Get-M3PeerAttackScopeGuard -PeerDir $csharpDir -LocalScope $LocalScope
  foreach ($m in $scopeGuard.messages) { Write-Output $m }
  if (@($scopeGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧攻击序列开关不同口径（{0}）" -f (@($scopeGuard.failed) -join ','))
    return
  }
  Write-Output ("ATTACK_SCOPE_OK peer='{0}' local='{1}'" -f $scopeGuard.peer_scope, $LocalScope)
  $poseGuard = Get-M3PeerAttackPoseGuard -PeerDir $csharpDir -LocalPose $LocalPose
  foreach ($m in $poseGuard.messages) { Write-Output $m }
  if (@($poseGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧攻击起点不同口径（{0}）" -f (@($poseGuard.failed) -join ','))
    return
  }
  Write-Output ("ATTACK_POSE_OK peer='{0}' local='{1}'" -f $poseGuard.peer_pose, $LocalPose)
  # ①.5 pin 重试账本护栏（owner 2026-10-12 16:30）：两侧的运行 pin 重试次数必须都是期望值（零额外重试），
  #    否则攻击序列起始的全局 RNG 位置不同 ⇒ 逐 k 不可比（c2-vs-c3 那次即被误判为「实现差异」）。
  #    fail-closed（PREREQ，exit 2）：拒绝把它当「同一序列」继续比，而不是静默继续。
  $pinGuard = Test-M3PinAttemptsComparable -Dirs @($csharpDir, $rustDir)
  foreach ($m in @($pinGuard.messages)) { Write-Output $m }
  if (@($pinGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧 pin 重试账本不可比（{0}）——拒绝把它当「同一序列」继续比" -f (@($pinGuard.failed) -join ','))
    return
  }
  $pinInfos = @($pinGuard.infos | ForEach-Object { ("{0}:attempts={1}/expected={2}" -f $_.dir, $_.attempts, $_.expected) })
  Write-Output ("PIN_ATTEMPTS_OK {0}（两侧均为零额外重试）" -f ($pinInfos -join ' '))
  # ② 两侧各自 align（若尚未生成）。
  foreach ($d in @($csharpDir, $rustDir)) {
    $rc = Invoke-AttackAlign $d
    if ($rc -ne 0) { Write-Output ("PREREQ_MISSING: align 失败（rc={0}）: {1}" -f $rc, $d); return }
  }
  # ③ 逐次对拍（判据本体）。
  Write-Output '--- 跨侧攻击序列比对（attack_diff.py diff） ---'
  & python $AttackDiff diff (Join-Path $csharpDir 'attack_seq.jsonl') (Join-Path $rustDir 'attack_seq.jsonl')
  if ($LASTEXITCODE -ne 0) {
    Write-Output 'RED: 跨侧攻击序列 diff != 0（差异要么修到 0，要么按 §6.0-3 登记 tests/parity/whitelist.md）'
    $script:AttackDiffRc = 1
    return
  }
  # ④ DB 侧作用域 diff（复用既有判据）。
  Write-Output '--- 跨侧作用域字段级 diff（--canonical --fixed-newchr） ---'
  & python $Scoped diff (Join-Path $csharpDir 'scoped_after_norm.jsonl') (Join-Path $rustDir 'scoped_after_norm.jsonl') --canonical --fixed-newchr
  if ($LASTEXITCODE -eq 0) {
    Write-Output 'GREEN: 跨侧攻击序列 diff = 0 且作用域字段级 diff = 0'
    $script:AttackDiffRc = 0
    return
  }
  Write-Output 'RED: 跨侧作用域字段级 diff != 0'
  $script:AttackDiffRc = 1
}

# ===========================================================================
# Mode: SelfCheckNoStack —— 无栈自检（机器可判，不碰真端口）
# ===========================================================================
if ($SelfCheckNoStack) {
  Write-Output 'SELFTEST_NO_STACK: 无栈自检——所有路径都必须 fail-closed（缺失绝不静默 GREEN）'
  $failures = @()
  $tmpRoot = Join-Path $env:TEMP ('m3attack-selftest-' + (Get-Date -Format 'HHmmssfff'))
  New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

  # ① 配置透传（T-22 扩展）：-AttackSequence/-AttackCount/... 必须落进 AppSetting.json；默认关。
  $cfgPath = Join-Path $RepoRoot 'src\BotSrv\bin\Release\AppSetting.json'
  & powershell -ExecutionPolicy Bypass -File $RunBots -AttackSequence -AttackCount 120 -AttackStepMs 500 -AttackStartDelayMs 3000 -WriteConfigOnly -SkipBuild | Out-Null
  if ($LASTEXITCODE -ne 0) {
    $failures += 'attackseq_config_on_writerc=non-zero'
  } else {
    $on = (Get-Content $cfgPath -Raw | ConvertFrom-Json).BotPlay
    if ($on.AttackSequence -ne $true) { $failures += 'attack_sequence_not_written_when_on' }
    if ($on.AttackCount -ne 120) { $failures += ('attack_count_on=' + $on.AttackCount) }
    Write-Output ("SELFTEST ① 配置透传 on=true? AttackSequence={0} AttackCount={1} StepMs={2}" -f $on.AttackSequence, $on.AttackCount, $on.AttackStepMs)
  }
  & powershell -ExecutionPolicy Bypass -File $RunBots -WriteConfigOnly -SkipBuild | Out-Null
  $off = (Get-Content $cfgPath -Raw | ConvertFrom-Json).BotPlay
  if ($off.AttackSequence -ne $false) { $failures += 'attack_sequence_default_not_off' }
  Write-Output ("SELFTEST ② 默认关 off=false? AttackSequence={0}" -f $off.AttackSequence)

  # ③ 无栈入口护栏：一列空闲端口 ⇒ 必须 PREREQ（exit 2），不能静默 GREEN。
  & powershell -ExecutionPolicy Bypass -File $MyInvocation.MyCommand.Path -Side rust -PortsCsv 17981,17982,17983,17984 | Out-Null
  $rcPort = $LASTEXITCODE
  if ($rcPort -ne 2) { $failures += ("no_stack_port_guard_rc=$rcPort(expect 2)") }
  Write-Output ("SELFTEST ③ 无栈入口护栏 rc={0}（期望 2=PREREQ）" -f $rcPort)

  # ④ -DiffOnly 缺会话 ⇒ PREREQ（exit 2）。
  & powershell -ExecutionPolicy Bypass -File $MyInvocation.MyCommand.Path -DiffOnly -CsharpSession '__none__' -RustSession '__none__' | Out-Null
  $rcDiff = $LASTEXITCODE
  if ($rcDiff -ne 2) { $failures += ("diffonly_missing_sessions_rc=$rcDiff(expect 2)") }
  Write-Output ("SELFTEST ④ -DiffOnly 缺会话 rc={0}（期望 2=PREREQ）" -f $rcDiff)

  # ⑤ run_pair 的开关耦合：-AttackSequence 必须一并传 -DisableAutoPlay；默认不得出现。
  $emitOnRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -AttackSequence -AttackCount 120 -AttackStepMs 500 -AttackStartDelayMs 3000
  if ($LASTEXITCODE -ne 0) { $failures += 'emit_on_rc=non-zero' }
  else {
    # M-1 修复后「未配 -AttackPinPair」会真打一行 WARN（stdout，JSON 之前）⇒ 解析前先滤掉 WARN 行。
    $emitOnText = ((@($emitOnRaw) | Where-Object { $_ -notmatch '^\s*(WARN:|PIN_NO_MONSTER|MAP_HAS_NO_MONSTER|MAP_HAS_NO_AI_CONSUMERS)' }) -join "`n").Trim()
    $emitOn = ($emitOnText | ConvertFrom-Json)
    $has = (@($emitOn.bot_args) -contains '-DisableAutoPlay')
    if (-not $has) { $failures += 'attack_sequence_does_not_pass_disable_autoplay' }
    $wantScope = 'on count=120 step_ms=500 start_delay_ms=3000 disable_autoplay=True'
    if ($emitOn.attack_sequence -ne $wantScope) { $failures += ('attack_scope_on=' + $emitOn.attack_sequence) }
    Write-Output ("SELFTEST ⑤a run_pair -AttackSequence 传 -DisableAutoPlay? {0}（scope={1}）" -f $has, $emitOn.attack_sequence)
  }
  $emitOffRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs
  if ($LASTEXITCODE -ne 0) { $failures += 'emit_off_rc=non-zero' }
  else {
    $emitOff = (($emitOffRaw | Out-String).Trim() | ConvertFrom-Json)
    if (@($emitOff.bot_args) -contains '-DisableAutoPlay') { $failures += 'default_path_unexpectedly_passes_disable_autoplay' }
    if ($emitOff.attack_sequence -ne '(off)') { $failures += ('attack_scope_off=' + $emitOff.attack_sequence) }
    Write-Output ("SELFTEST ⑤b 默认路径不带 -DisableAutoPlay 且 scope=(off)：{0}" -f $emitOff.attack_sequence)
  }

  # ⑥ 攻击变体两相 pin：-AttackPinPair 的 attack_pose + 每 bot SQL（attacker/victim 相邻）。
  #    R-1：pin 地图/落点取无怪图（gate_guards $script:M3AttackPinMap='0137' + CX/CY）；本项断言 emit 出的
  #    pose 与该落点/map 逐字节相同，且 pin SQL 真把 MapName 落到该图（-PinMapName 透传有牙齿）。
  $emitPairRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -FixedNewChr -PinStartPose -AttackPinPair -AttackSequence -Bots 2 -PinMapName $PinMapName -PinCX $AttackPinCX -PinCY $AttackPinCY
  if ($LASTEXITCODE -ne 0) { $failures += 'emit_pair_rc=non-zero' }
  else {
    $pair = (((@($emitPairRaw) | Where-Object { $_ -notmatch '^\s*(WARN:|PIN_NO_MONSTER|MAP_HAS_NO_MONSTER|MAP_HAS_NO_AI_CONSUMERS)' }) -join "`n").Trim() | ConvertFrom-Json)
    $wantPose = ('pair map={0} attacker={1},{2},dir=2 victim={3},{2} level=1 vhp=30000' -f $PinMapName, $AttackPinCX, $AttackPinCY, ($AttackPinCX + 1))
    if ($pair.attack_pose -ne $wantPose) { $failures += ('attack_pose=' + $pair.attack_pose) }
    $all = @($pair.attack_pin_sql_all)
    if ($all.Count -lt 4) { $failures += ('attack_pin_sql_all_count=' + $all.Count) }
    $joined = ($all -join ' || ')
    if ($joined -notmatch 'ChrName=''m3[^'']*0''') { $failures += 'attack_pin_missing_attacker_row' }
    if ($joined -notmatch ('CX=' + ($AttackPinCX + 1) + '[^0-9]')) { $failures += 'attack_pin_missing_victim_cell' }
    if ($joined -notmatch 'a\.Hp=30000, a\.MaxHP=30000') { $failures += 'attack_pin_missing_victim_hp' }
    # F-D（m3cs tick21）：探针行真执行才能咬住越界值；旧缺省 100000 > smallint(32767) ⇒ pin 真机必 ERROR 1264。
    if ($joined -match 'a\.Hp=100000') { $failures += 'attack_pin_victim_hp_out_of_range' }
    # R-1：-PinMapName 必须真落进攻击 pin SQL（改坏透传 ⇒ 本项必红）。
    $mapRe = "MapName='" + [regex]::Escape($PinMapName) + "'"
    if ($joined -notmatch $mapRe) { $failures += ('attack_pin_missing_map=' + $PinMapName) }
    Write-Output ("SELFTEST ⑥ -AttackPinPair map={0} pose='{1}'；pin_sql_all={2} 条" -f $PinMapName, $pair.attack_pose, $all.Count)

    # ⑥b F-B/F-D：pin SQL 的「可执行性」门（m3cs tick16/tick21 真机 finding）。
    #    字符串匹配永远穿得过「列名挂错表」与「越界值」⇒ 必须真送进 MySQL。
    #    生产 4 条 SQL（镜像临时表 + 探针行）⇒ rc=0；
    #    把 `c.IncHealth` 改回历史 bug `a.IncHealth` ⇒ 必 RED（ERROR 1054）；
    #    把 Hp/MaxHP 改回越界的 100000 ⇒ 必 RED（ERROR 1264）。
    $pinExec = Test-M3PinSqlExecutable -SqlStatements $all -MysqlExe $MysqlExe
    if ($pinExec.rc -ne 0) { $failures += ('pin_sql_not_executable_rc=' + $pinExec.rc + ':' + ($pinExec.failures -join ';')) }
    $mutAll = @($all | ForEach-Object { $_ -replace 'c\.IncHealth=0, c\.IncSpell=0', 'a.IncHealth=0, a.IncSpell=0' })
    $mutExec = Test-M3PinSqlExecutable -SqlStatements $mutAll -MysqlExe $MysqlExe
    if ($mutExec.rc -eq 0) { $failures += 'pin_sql_exec_mutation_not_red' }
    if (-not (($mutExec.failures -join ';') -match 'a\.IncHealth')) { $failures += 'pin_sql_exec_mutation_not_diff_shaped' }
    $mutHp = @($all | ForEach-Object { $_ -replace 'a\.Hp=30000, a\.MaxHP=30000', 'a.Hp=100000, a.MaxHP=100000' })
    $mutHpExec = Test-M3PinSqlExecutable -SqlStatements $mutHp -MysqlExe $MysqlExe
    if ($mutHpExec.rc -eq 0) { $failures += 'pin_sql_range_mutation_not_red' }
    if (-not (($mutHpExec.failures -join ';') -match 'Out of range|1264')) { $failures += 'pin_sql_range_mutation_not_diff_shaped' }
    Write-Output ("SELFTEST ⑥b pin SQL 可执行性（镜像临时表 + 探针行真执行）：生产 rc={0} failures={1}；改坏 a.IncHealth rc={2} [{3}]；改坏 Hp=100000 rc={4} [{5}]" -f `
      $pinExec.rc, $pinExec.failures.Count, $mutExec.rc, (($mutExec.failures -join ';') | Select-Object -First 1), $mutHpExec.rc, (($mutHpExec.failures -join ';') | Select-Object -First 1))
  }

  # ⑥c F-D 范围护栏（m3cs tick21）：本脚本自己的缺省 value 必须落在
  #    `characters_ablity.Hp/MaxHP`（smallint 有符号，1..32767）内；越界值必须被判红。
  #    改坏（把本文件 :71-ish 的 `-AttackVictimHp` 缺省改回 100000）⇒ `attack_victim_hp_default_out_of_range` 必红。
  if (-not (Test-M3AttackVictimHpRange -VictimHp $AttackVictimHp)) {
    $failures += ('attack_victim_hp_default_out_of_range=' + $AttackVictimHp)
  }
  if (Test-M3AttackVictimHpRange -VictimHp 100000) { $failures += 'attack_victim_hp_range_guard_not_red' }
  Write-Output ("SELFTEST ⑥c -AttackVictimHp 范围护栏：缺省 {0} 合规（≤32767）；100000 必判红" -f $AttackVictimHp)

  # ⑯ R-1（owner 2026-10-12 14:30）：攻击 pin 场景必须**整张图无怪**——按地图查冻结提取件
  #    `fixtures/mongen-map<map>.tsv` + `fixtures/mongen-provenance.tsv`，判「该图刷怪行数 == 0」。
  #    两向红检：① **换回 map 0**（有 53 个刷怪点）必须红；② 合成提取件（0 行/1 行·声明漂移·缺 provenance）
  #    判定必须有牙齿（改坏 Test-M3MonGenEmpty 的「==0」判断 ⇒ 本项必红）。
  $pinChk = Test-M3MonGenEmpty -MapName $PinMapName
  if (-not $pinChk.ok) { $failures += ('attack_pin_map_not_monsterless=' + $PinMapName + ':' + $pinChk.reason) }
  if ($pinChk.spawns -ne 0) { $failures += ('attack_pin_map_spawns=' + $pinChk.spawns) }
  $pinChkV = Test-M3MonGenEmpty -MapName $PinMapName -VerifySource
  if (-not $pinChkV.ok) { $failures += ('attack_pin_map_source_recheck_red=' + $pinChkV.reason) }
  if (-not $pinChkV.source_verified) { $failures += 'attack_pin_map_source_not_verified' }
  $map0Chk = Test-M3MonGenEmpty -MapName '0'
  if ($map0Chk.ok) { $failures += 'attack_pin_map0_not_red' }
  if ($map0Chk.spawns -ne 53) { $failures += ('map0_spawns_unexpected=' + $map0Chk.spawns) }
  # 合成牙齿：临时 fixture + provenance，覆盖「0 行=绿 / 1 行=红 / 行数!=声明=红 / 缺 provenance=红」。
  $synthDir = Join-Path $tmpRoot 'mongen-synth'
  New-Item -ItemType Directory -Force -Path $synthDir | Out-Null
  $synthProv = Join-Path $tmpRoot 'mongen-provenance-synth.tsv'
  [System.IO.File]::WriteAllText((Join-Path $synthDir 'mongen-mapE.tsv'), "# empty`n", (New-Object System.Text.UTF8Encoding($false)))
  [System.IO.File]::WriteAllText((Join-Path $synthDir 'mongen-mapF.tsv'), "1`t1`t1`n", (New-Object System.Text.UTF8Encoding($false)))
  [System.IO.File]::WriteAllText((Join-Path $synthDir 'mongen-mapG.tsv'), "2`t2`t2`n", (New-Object System.Text.UTF8Encoding($false)))
  $synthLines = @("E`tmongen-mapE.tsv`t0`t`t", "F`tmongen-mapF.tsv`t1`t`t", "G`tmongen-mapG.tsv`t0`t`t")
  [System.IO.File]::WriteAllLines($synthProv, $synthLines, (New-Object System.Text.UTF8Encoding($false)))
  $synthE = Test-M3MonGenEmpty -MapName 'E' -FixtureDir $synthDir -ProvenancePath $synthProv
  if (-not $synthE.ok) { $failures += ('synth_empty_not_ok=' + $synthE.reason) }
  $synthF = Test-M3MonGenEmpty -MapName 'F' -FixtureDir $synthDir -ProvenancePath $synthProv
  if ($synthF.ok) { $failures += 'synth_nonempty_not_red' }
  $synthG = Test-M3MonGenEmpty -MapName 'G' -FixtureDir $synthDir -ProvenancePath $synthProv
  if ($synthG.ok) { $failures += 'synth_rowcount_drift_not_red' }
  $synthH = Test-M3MonGenEmpty -MapName 'H' -FixtureDir $synthDir -ProvenancePath $synthProv
  if ($synthH.ok) { $failures += 'synth_missing_provenance_not_red' }
  Write-Output ("SELFTEST ⑯ 攻击 pin 无怪口径（R-1，按地图查提取件）：生产 map={0} spawns={1} source_verified={2}；map 0 spawns={3} 必红；合成 0/1 行·声明漂移·缺 provenance 四向有牙" -f `
    $PinMapName, $pinChk.spawns, $pinChkV.source_verified, $map0Chk.spawns)

  # ⑰ R-1 refine（owner 2026-10-12 15:05）："严格说不能只看 MonGen" —— 守卫/NPC/地图事件/任务触发
  #    也可能跑 AI、消耗全局 RNG ⇒ 按 pin 地图查冻结提取件 `fixtures/ai-consumer-map<map>.tsv` +
  #    `fixtures/ai-consumer-provenance.tsv`，要求该图在 GuardList/MerChant/Npcs/MapEvent/MapQuest
  #    里的引用行数**全为 0**。红检：合成「有 1 行引用 / 声明漂移 / 缺 provenance / 缺提取件」四向必红；
  #    改坏 Test-M3MapNoAiConsumers 的「declared_total == 0」判断 ⇒ 本项必红。
  $aiChk = Test-M3MapNoAiConsumers -MapName $PinMapName
  if (-not $aiChk.ok) { $failures += ('attack_pin_ai_consumers_not_zero=' + $PinMapName + ':' + $aiChk.reason) }
  if ($aiChk.total -ne 0) { $failures += ('attack_pin_ai_consumers_total=' + $aiChk.total) }
  $aiChkV = Test-M3MapNoAiConsumers -MapName $PinMapName -VerifySource
  if (-not $aiChkV.ok) { $failures += ('attack_pin_ai_consumers_source_recheck_red=' + $aiChkV.reason) }
  if (-not $aiChkV.source_verified) { $failures += 'attack_pin_ai_consumers_source_not_verified' }
  $aiSynthDir = Join-Path $tmpRoot 'ai-consumer-synth'
  New-Item -ItemType Directory -Force -Path $aiSynthDir | Out-Null
  $aiSynthProv = Join-Path $tmpRoot 'ai-consumer-provenance-synth.tsv'
  $aiZero = "GuardList.txt`t0`nMerChant.txt`t0`nNpcs.txt`t0`nMapEvent.txt`t0`nMapQuest.txt`t0`n"
  $aiOneG = "GuardList.txt`t1`nMerChant.txt`t0`nNpcs.txt`t0`nMapEvent.txt`t0`nMapQuest.txt`t0`n"
  [System.IO.File]::WriteAllText((Join-Path $aiSynthDir 'ai-consumer-mapA.tsv'), $aiZero, (New-Object System.Text.UTF8Encoding($false)))
  [System.IO.File]::WriteAllText((Join-Path $aiSynthDir 'ai-consumer-mapB.tsv'), $aiOneG, (New-Object System.Text.UTF8Encoding($false)))
  [System.IO.File]::WriteAllText((Join-Path $aiSynthDir 'ai-consumer-mapC.tsv'), $aiOneG, (New-Object System.Text.UTF8Encoding($false)))
  $aiSynthLines = @("A`tai-consumer-mapA.tsv`t0`t`t", "B`tai-consumer-mapB.tsv`t1`t`t", "C`tai-consumer-mapC.tsv`t0`t`t", "E`tai-consumer-mapE.tsv`t0`t`t")
  [System.IO.File]::WriteAllLines($aiSynthProv, $aiSynthLines, (New-Object System.Text.UTF8Encoding($false)))
  $aiSynthA = Test-M3MapNoAiConsumers -MapName 'A' -FixtureDir $aiSynthDir -ProvenancePath $aiSynthProv
  if (-not $aiSynthA.ok) { $failures += ('synth_ai_empty_not_ok=' + $aiSynthA.reason) }
  $aiSynthB = Test-M3MapNoAiConsumers -MapName 'B' -FixtureDir $aiSynthDir -ProvenancePath $aiSynthProv
  if ($aiSynthB.ok) { $failures += 'synth_ai_ref_not_red' }
  $aiSynthC = Test-M3MapNoAiConsumers -MapName 'C' -FixtureDir $aiSynthDir -ProvenancePath $aiSynthProv
  if ($aiSynthC.ok) { $failures += 'synth_ai_declared_drift_not_red' }
  $aiSynthD = Test-M3MapNoAiConsumers -MapName 'D' -FixtureDir $aiSynthDir -ProvenancePath $aiSynthProv
  if ($aiSynthD.ok) { $failures += 'synth_ai_missing_provenance_not_red' }
  $aiSynthE = Test-M3MapNoAiConsumers -MapName 'E' -FixtureDir $aiSynthDir -ProvenancePath $aiSynthProv
  if ($aiSynthE.ok) { $failures += 'synth_ai_missing_fixture_not_red' }
  Write-Output ("SELFTEST ⑰ 攻击 pin 非 MonGen AI 消费者口径（R-1 refine 15:05）：生产 map={0} total={1} source_verified={2}；合成 有引用/声明漂移/缺 provenance/缺提取件 四向必红" -f `
    $PinMapName, $aiChk.total, $aiChkV.source_verified)

  # ⑱ pin 重试账本（owner 2026-10-12 16:30）：会话产物必须把本轮 pin 的 attempts 落成结构化字段；
  #    判据层对「attempts 超过期望」fail-closed（拒绝把重试过的运行当「同一序列」继续比）。
  #    期望值口径 = 2（`Get-M3PinHoldVerdict` 的「连续两轮 settle」结构性最小 attempts；见 gate_guards ⑥b）。
  #    本项：① 断言仪器/常量同源；② 五向构造输入（retried/missing/invalid/expected漂移/嵌套对象兼容）；
  #    ③ **改坏必红（真跑后还原）**：同一会话 attempts=2 ⇒ 绿；写成 attempts=3（= 人为多一次重试）⇒ 必红；还原 ⇒ 绿。
  $pinExpected = Get-M3PinExpectedAttempts
  if ($pinExpected -ne 2) { $failures += ('pin_expected_attempts=' + $pinExpected) }
  if ($null -ne $pair) {
    if ($pair.pin_attempts_expected -ne 2) { $failures += ('emit_pin_attempts_expected=' + $pair.pin_attempts_expected) }
    if ([string]$pair.pin_instrument -notmatch 'consecutive=2') { $failures += ('pin_instrument_not_consecutive2=' + $pair.pin_instrument) }
  }
  function New-M3PinSession([string]$Tag, $PinAttemptsValue) {
    $d = Join-Path $tmpRoot ('pin-' + $Tag)
    New-Item -ItemType Directory -Force -Path $d | Out-Null
    $obj = [ordered]@{ session = (Split-Path $d -Leaf); side = 'csharp' }
    if ($null -ne $PinAttemptsValue) { $obj['pin_attempts'] = $PinAttemptsValue }
    ([pscustomobject]$obj | ConvertTo-Json -Depth 4) | Set-Content -Path (Join-Path $d 'baseline.json') -Encoding utf8
    return $d
  }
  $pinOkA = New-M3PinSession 'ok-a' (Format-M3PinAttempts -Attempts 2 -SettleRounds 2 -MaxAttempts 5)
  $pinOkB = New-M3PinSession 'ok-b' (Format-M3PinAttempts -Attempts 2 -SettleRounds 2 -MaxAttempts 5)
  $cmpOk = Test-M3PinAttemptsComparable -Dirs @($pinOkA, $pinOkB)
  if (@($cmpOk.failed).Count -ne 0) { $failures += ('pin_attempts_ok_not_green=' + (@($cmpOk.failed) -join ';')) }
  $pinRetry = New-M3PinSession 'retry-b' (Format-M3PinAttempts -Attempts 3 -SettleRounds 3 -MaxAttempts 5)
  $cmpRetry = Test-M3PinAttemptsComparable -Dirs @($pinOkA, $pinRetry)
  if (-not (@($cmpRetry.failed) -contains 'pin_attempts_retried')) { $failures += 'pin_attempts_retry_not_red' }
  $pinMissing = New-M3PinSession 'missing' $null
  if (-not (@((Get-M3PinAttemptsGuard -PlayerDir $pinMissing).failed) -contains 'pin_attempts_missing')) { $failures += 'pin_attempts_missing_not_red' }
  $pinInvalid = New-M3PinSession 'invalid' 'attempts=garbage'
  if (-not (@((Get-M3PinAttemptsGuard -PlayerDir $pinInvalid).failed) -contains 'pin_attempts_invalid')) { $failures += 'pin_attempts_invalid_not_red' }
  $pinDrift = New-M3PinSession 'drift' (Format-M3PinAttempts -Attempts 2 -Expected 3 -SettleRounds 2 -MaxAttempts 5)
  if (-not (@((Get-M3PinAttemptsGuard -PlayerDir $pinDrift).failed) -contains 'pin_attempts_expected_mismatch')) { $failures += 'pin_attempts_expected_drift_not_red' }
  $pinNested = New-M3PinSession 'nested' ([pscustomobject]@{ attempts = 2; expected = 2; settle_rounds = 2; max_attempts = 5 })
  if (@((Get-M3PinAttemptsGuard -PlayerDir $pinNested).failed).Count -ne 0) { $failures += ('pin_attempts_nested_not_green=' + (@((Get-M3PinAttemptsGuard -PlayerDir $pinNested).failed) -join ';')) }
  # 改坏必红（真跑 + 还原）：把同一会话的 attempts 从 2 改成 3 ⇒ 必红；还原 ⇒ 绿。
  $pinMutFile = Join-Path $pinOkA 'baseline.json'
  $pinOrigJson = Get-Content $pinMutFile -Raw
  $pinPreGreen = (@((Get-M3PinAttemptsGuard -PlayerDir $pinOkA).failed).Count -eq 0)
  Set-Content -Path $pinMutFile -Encoding utf8 -Value '{ "session": "pin-ok-a", "side": "csharp", "pin_attempts": "attempts=3 expected=2 settle_rounds=3 max_attempts=5" }'
  $pinMutRed = $false
  $pinMutInfo = Get-M3PinAttemptsGuard -PlayerDir $pinOkA
  $pinMutRed = (@($pinMutInfo.failed) -contains 'pin_attempts_retried')
  Set-Content -Path $pinMutFile -Encoding utf8 -Value $pinOrigJson
  $pinRestoredGreen = (@((Get-M3PinAttemptsGuard -PlayerDir $pinOkA).failed).Count -eq 0)
  if (-not $pinPreGreen) { $failures += 'pin_mutation_pre_not_green' }
  if (-not $pinMutRed) { $failures += 'pin_mutation_retry_not_red' }
  if (-not $pinRestoredGreen) { $failures += 'pin_mutation_restore_not_green' }
  Write-Output ("SELFTEST ⑱ pin 重试账本（owner 16:30）：期望 attempts={0}（已与仪器 consecutive 同源）；五向构造输入 + 改坏必红真跑（attempts=2 绿 / 改成 3 红[{1}] / 还原 绿[{2}]）" -f `
    $pinExpected, (@($pinMutInfo.failed) -join ','), $pinRestoredGreen)

  # ⑭ F-E（m3cs tick21）：驱动收尾等待——run_bots.ps1 的攻击序列「等 M3ATTSEQ done」离线自检
  #    （合成日志计 attack done 2/1/0 + bag done 2/0；改坏计数正则 ⇒ RED）。真机 120/120 由 m3cs 下一轮实跑覆盖。
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $botsSelfOut = (& powershell -ExecutionPolicy Bypass -File (Join-Path $RepoRoot 'tools\botload\run_bots.ps1') -SelfCheckNoStack 2>&1 | Out-String)
  $botsSelfRc = $LASTEXITCODE
  $ErrorActionPreference = $prevEap
  if ($botsSelfRc -ne 0) { $failures += ('attack_wait_selftest_rc=' + $botsSelfRc) }
  if ($botsSelfOut -notmatch 'SELFTEST_RED GREEN') { $failures += 'attack_wait_selftest_not_green' }
  Write-Output ("SELFTEST ⑭ 攻击序列收尾等待（run_bots -SelfCheckNoStack）rc={0}" -f $botsSelfRc)

  # ⑮ F-H（m3bag 2026-10-12）：`Invoke-AttackAlign` 的**返回值类型**必须是 int（只含退出码），
  #    不得混入 python stdout（否则 `-ne 0` 恒真 ⇒ C# 侧 align 成功后仍报 RED）。真跑生产函数。
  #    改坏（把函数体改回 `& python …; return $LASTEXITCODE`）⇒ `attack_align_rc_not_int=Object[]` 必红。
  $alignDir = Join-Path $tmpRoot 'f-h-align'
  New-Item -ItemType Directory -Force -Path $alignDir | Out-Null
  $fRaw = Join-Path $alignDir 'attack_raw.jsonl'
  $fLines = @(
    'M3ATTSEQ {"role":"attack","k":1,"op":"CM_HIT","x":1,"y":1,"dir":2,"my":11}',
    'M3ATTSEQ {"role":"struck","dmg":3,"hp":7,"maxhp":10,"src":11,"actor":22}',
    'M3ATTSEQ {"role":"done","attacks":1}'
  )
  [System.IO.File]::WriteAllLines($fRaw, $fLines, (New-Object System.Text.UTF8Encoding($false)))
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $alignRc2 = Invoke-AttackAlign $alignDir
  $ErrorActionPreference = $prevEap
  if ($alignRc2 -isnot [int]) { $failures += ('attack_align_rc_not_int=' + $(if ($null -ne $alignRc2) { $alignRc2.GetType().Name } else { 'null' })) }
  elseif ($alignRc2 -ne 0) { $failures += ('attack_align_rc=' + $alignRc2) }
  if (-not (Test-Path (Join-Path $alignDir 'attack_seq.jsonl'))) { $failures += 'attack_align_seq_missing' }
  Write-Output ("SELFTEST ⑮ Invoke-AttackAlign 返回值类型/退出码（F-H）：rc={0} type={1}" -f (@($alignRc2) -join '|'), $(if ($null -ne $alignRc2) { $alignRc2.GetType().Name } else { 'null' }))

  # ⑩ Minor M-1 回归（reviewer-5 76e433f0）：`-AttackSequence` 未配 `-AttackPinPair` ⇒ WARN 必须**真**出现；
  #    配了 `-AttackPinPair` ⇒ 不得出现。旧实现把 WARN 写在 `if ($EmitOpsArgs)` 块内且带 `-not $EmitOpsArgs`
  #    ⇒ 恒 false 死代码，本项必 RED（这就是「改坏必红」：把 WARN 挪回块内 ⇒ m1_warn_missing 必红）。
  $warnPat = 'WARN: -AttackSequence 未配 -AttackPinPair'
  $warnOn = @(@($emitOnRaw) | Where-Object { $_ -match $warnPat }).Count
  $warnPair = @(@($emitPairRaw) | Where-Object { $_ -match $warnPat }).Count
  if ($warnOn -lt 1) { $failures += 'm1_warn_missing_when_no_pinpair' }
  if ($warnPair -ne 0) { $failures += 'm1_warn_present_when_pinpair' }
  Write-Output ("SELFTEST ⑩ M-1 WARN 有牙齿：无 -AttackPinPair={0} 条（期望≥1），有 -AttackPinPair={1} 条（期望 0）" -f $warnOn, $warnPair)

  # ⑪ Minor M-2 回归（reviewer-5 76e433f0）：attacker 选择必须是**精确相等**（生产函数级，非源码扫描）。
  #    `BotSrv.exe --selftest-attackrole` 覆盖 `-Bots>10`（10/20… 不得命中 attacker 序号 0）；
  #    把 IsAttackAttacker 改回 EndsWith ⇒ 必 RED（红检原始件见证据 mutation-red-minors.txt）。
  $botProj = Join-Path $RepoRoot 'src\BotSrv\BotSrv.csproj'
  $botExe = Join-Path $RepoRoot 'src\BotSrv\bin\Release\BotSrv.exe'
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $botBuildOut = (& dotnet build $botProj -c Release --nologo -v q 2>&1 | Out-String)
  $botBuildRc = $LASTEXITCODE
  if ($botBuildRc -ne 0) {
    $failures += ('m2_botsrv_build_rc=' + $botBuildRc)
    Write-Output '  --- BotSrv build 输出 ---'; Write-Output $botBuildOut
  } else {
    $roleOut = (& $botExe --selftest-attackrole 2>&1 | Out-String)
    $roleRc = $LASTEXITCODE
    if ($roleRc -ne 0) { $failures += ('m2_attackrole_selftest_rc=' + $roleRc) }
    if ($roleOut -notmatch 'ATTACKROLE SELFTEST GREEN') { $failures += 'm2_attackrole_selftest_not_green' }
    Write-Output ("SELFTEST ⑪ M-2 attacker 精确选择 selftest rc={0}" -f $roleRc)
    if ($roleRc -ne 0) { Write-Output $roleOut }
  }
  $ErrorActionPreference = $prevEap

  # ⑦ attack_diff selftest：同序列绿 + 改坏 dmg/hit 必红 + 非平凡门 + 空序列。
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $attackStOut = (& python $AttackDiff selftest 2>&1 | Out-String)
  $attackStRc = $LASTEXITCODE
  $ErrorActionPreference = $prevEap
  if ($attackStRc -ne 0) { $failures += ("attack_diff_selftest_rc=$attackStRc") }
  if ($attackStOut -notmatch 'SELFTEST GREEN') { $failures += 'attack_diff_selftest_not_green' }
  Write-Output ("SELFTEST ⑦ attack_diff selftest rc={0}" -f $attackStRc)

  # ⑧ 生产函数级「改坏必红」：合成两侧 raw → align → diff 绿；改坏一次 dmg ⇒ 红；改坏一次 hit ⇒ 红。
  #    F-J（m3cs tick22，2026-10-12 13:45）：align **必须走生产函数 `Invoke-AttackAlign`**，
  #    不再 `& python … align … | Out-Null` 把待测函数的返回值吞掉——旧写法正好绕过了
  #    「返回值被 python stdout 污染」这个 bug。这里直接断言两个返回值都是 `[int]` 且为 0；
  #    把函数体改回 `& python …; return $LASTEXITCODE` ⇒ `align_synthetic_rc_not_int=Object[]`（本项 + ⑮）必红。
  $dirA = Join-Path $tmpRoot 'a'
  $dirB = Join-Path $tmpRoot 'b'
  New-Item -ItemType Directory -Force -Path $dirA, $dirB | Out-Null
  $rawA = Join-Path $dirA 'attack_raw.jsonl'
  $rawB = Join-Path $dirB 'attack_raw.jsonl'
  $events = New-Object System.Collections.Generic.List[string]
  for ($k = 1; $k -le 10; $k++) {
    $events.Add('M3ATTSEQ {"role":"attack","k":' + $k + ',"op":"CM_HIT","x":289,"y":618,"dir":2,"my":101}')
    if ($k -ne 3) { $events.Add('M3ATTSEQ {"role":"struck","dmg":' + (20 + ($k % 5)) + ',"hp":' + (100 - $k) + ',"maxhp":100,"src":101,"actor":77}') }
  }
  $events.Add('M3ATTSEQ {"role":"done","attacks":10}')
  $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllLines($rawA, $events, $utf8NoBom)
  [System.IO.File]::WriteAllLines($rawB, $events, $utf8NoBom)
  $seqA = Join-Path $dirA 'attack_seq.jsonl'
  $seqB = Join-Path $dirB 'attack_seq.jsonl'
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $alignRc = Invoke-AttackAlign $dirA
  $alignRcB = Invoke-AttackAlign $dirB
  $ErrorActionPreference = $prevEap
  if ($alignRc -isnot [int]) { $failures += ('align_synthetic_rc_not_int=' + $(if ($null -ne $alignRc) { $alignRc.GetType().Name } else { 'null' })) }
  elseif ($alignRc -ne 0) { $failures += ('align_synthetic_rc=' + $alignRc) }
  if ($alignRcB -isnot [int]) { $failures += ('align_synthetic_rc_b_not_int=' + $(if ($null -ne $alignRcB) { $alignRcB.GetType().Name } else { 'null' })) }
  elseif ($alignRcB -ne 0) { $failures += ('align_synthetic_rc_b=' + $alignRcB) }
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $greenOut = (& python $AttackDiff diff $seqA $seqB 2>&1 | Out-String)
  $ErrorActionPreference = $prevEap
  $greenRc = $LASTEXITCODE
  if ($greenRc -ne 0) { $failures += ('synthetic_identical_rc=' + $greenRc) }
  # 改坏 b 侧某一次 dmg（走生产的 `mutate` 子命令，不靠字符串替换）⇒ 必红
  & python $AttackDiff mutate $seqB --k 5 --field dmg --value 999 | Out-Null
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $dmgRedOut = (& python $AttackDiff diff $seqA $seqB 2>&1 | Out-String)
  $ErrorActionPreference = $prevEap
  $dmgRedRc = $LASTEXITCODE
  if ($dmgRedRc -ne 1) { $failures += ('dmg_mutation_not_red_rc=' + $dmgRedRc) }
  # 重生成 b.seq（清掉上一步），再改坏 k=5 的 hit（hit→dodge）⇒ 必红
  & python $AttackDiff align $rawB --out $seqB | Out-Null
  & python $AttackDiff mutate $seqB --k 5 --field hit --value 0 | Out-Null
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $hitRedOut = (& python $AttackDiff diff $seqA $seqB 2>&1 | Out-String)
  $ErrorActionPreference = $prevEap
  $hitRedRc = $LASTEXITCODE
  if ($hitRedRc -ne 1) { $failures += ('hit_mutation_not_red_rc=' + $hitRedRc) }
  # 绿的那次必须真的一行差异都没有（证明「必红」不是语法/BOM 引起的假红）
  if ($greenOut -notmatch 'GREEN') { $failures += 'synthetic_green_not_green' }
  if ($dmgRedOut -notmatch 'dmg 不等') { $failures += 'dmg_mutation_not_diff_shaped' }
  if ($hitRedOut -notmatch 'hit 不等') { $failures += 'hit_mutation_not_diff_shaped' }
  Write-Output 'SELFTEST ⑧ 合成两侧 → align/diff：'
  Write-Output ('  [绿] rc={0}: {1}' -f $greenRc, (($greenOut.Trim() -split "`n")[-1]))
  Write-Output ('  [改坏 dmg] rc={0}: {1}' -f $dmgRedRc, (($dmgRedOut.Trim() -split "`n") | Where-Object { $_ -match 'dmg 不等' } | Select-Object -First 1))
  Write-Output ('  [改坏 hit] rc={0}: {1}' -f $hitRedRc, (($hitRedOut.Trim() -split "`n") | Where-Object { $_ -match 'hit 不等' } | Select-Object -First 1))
  if ($greenRc -ne 0 -or $dmgRedRc -ne 1 -or $hitRedRc -ne 1) {
    Write-Output '  --- 原始输出（绿） ---'; Write-Output $greenOut
    Write-Output '  --- 原始输出（改坏 dmg） ---'; Write-Output $dmgRedOut
    Write-Output '  --- 原始输出（改坏 hit） ---'; Write-Output $hitRedOut
  }

  # ⑨ scoped_diff selftest 仍须绿（DB 侧判据未被削弱）。
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $scopedOut = (& python $Scoped selftest 2>&1 | Out-String)
  $scopedRc = $LASTEXITCODE
  $ErrorActionPreference = $prevEap
  if ($scopedRc -ne 0) { $failures += ("scoped_diff_selftest_rc=$scopedRc") }
  if ($scopedOut -notmatch 'selftest 14/14') { $failures += 'scoped_diff_selftest_missing' }
  Write-Output ("SELFTEST ⑨ scoped_diff selftest rc={0}（14/14）" -f $scopedRc)

  # ⑫ pin SQL 的「别名/列存在性」无栈校验（m3cs tick16 真机 finding，2026-10-12 11:42）：
  #    `Get-M3AttackPinSql` 曾把 `IncHealth/IncSpell` 挂在 `characters_ablity` 别名 `a` 上
  #    ⇒ 真机 `ERROR 1054 Unknown column 'a.IncHealth'`、run_pair pin 步 RED、对拍发不出攻击。
  #    两向：生产 SQL（背包 pin + 攻击 pin）必须 0 violation；把 `c.IncHealth` 改回历史 bug 的
  #    `a.IncHealth` 必须被检出（红检有牙齿）。不连真库、不起栈。
  $schemaCols = Get-M3SchemaColumns -SchemaPath (Join-Path $RepoRoot 'sql\mir2_db.sql')
  $prodPinSql = @()
  $prodPinSql += Get-M3StartPosePinSql -ChrName 'm3fix0'
  $prodPinSql += Get-M3AttackPinSql -ChrName 'm3fix0' -CX 289 -CY 618 -Dir 2 -MapName '0' -HomeMap 0 -HomeX 289 -HomeY 618 -Level 1 -Hp 30000
  $prodVio = @(Test-M3PinSqlColumns -SqlStatements $prodPinSql -SchemaColumns $schemaCols)
  if ($prodVio.Count -ne 0) { $failures += ('pin_sql_column_violations=' + ($prodVio -join ';')) }
  $mutPinSql = @(Get-M3AttackPinSql -ChrName 'm3fix0' -CX 289 -CY 618 -Dir 2 -MapName '0' -HomeMap 0 -HomeX 289 -HomeY 618 -Level 1 -Hp 30000 |
    ForEach-Object { $_ -replace 'c\.IncHealth=0, c\.IncSpell=0', 'a.IncHealth=0, a.IncSpell=0' })
  $mutVio = @(Test-M3PinSqlColumns -SqlStatements $mutPinSql -SchemaColumns $schemaCols)
  if ($mutVio.Count -eq 0) { $failures += 'pin_sql_column_mutation_not_red' }
  if (-not (($mutVio -join ';') -match 'a\.IncHealth')) { $failures += 'pin_sql_column_mutation_not_diff_shaped' }
  Write-Output ("SELFTEST ⑫ pin SQL 别名/列校验：生产 statements={0} violations={1}；改坏 a.IncHealth violations={2}" -f $prodPinSql.Count, $prodVio.Count, $mutVio.Count)

  # ⑬ F-C：种子注入证据断言（离线；不起栈/不连库）。两向 + 种子不符 + 空路径 fail-closed。
  #    C# = `INJECT_OK <seed>`；Rust = `SEED_PROBE` 行（+ 可选 `RUST_SEED_INJECTED`）。
  $csGood = Join-Path $tmpRoot 'f-c-cs-good.tsv'
  Set-Content -LiteralPath $csGood -Encoding ascii -Value "1`tINJECT_OK`t42`t0"
  $csBad = Join-Path $tmpRoot 'f-c-cs-bad.tsv'
  Set-Content -LiteralPath $csBad -Encoding ascii -Value "123`tNext(max)`t20`t5"
  $csWrong = Join-Path $tmpRoot 'f-c-cs-wrong.tsv'
  Set-Content -LiteralPath $csWrong -Encoding ascii -Value "1`tINJECT_OK`t7`t0"
  $rsGood = Join-Path $tmpRoot 'f-c-rs-good.log'
  Set-Content -LiteralPath $rsGood -Encoding utf8 -Value '12:00:00 [INF] RNG 同种子注入：seed=42 参照序列=SEED_PROBE 1434747710 302596119'
  $rsBad = Join-Path $tmpRoot 'f-c-rs-bad.log'
  Set-Content -LiteralPath $rsBad -Encoding utf8 -Value '12:00:00 [INF] 世界已加载'
  $rsMismatch = Join-Path $tmpRoot 'f-c-rs-mismatch.log'
  Set-Content -LiteralPath $rsMismatch -Encoding utf8 -Value "RUST_SEED_INJECTED=7`nRNG 同种子注入：seed=42 参照序列=SEED_PROBE 1434747710"
  $evChecks = @(
    @{ n = 'csharp/INJECT_OK 42=绿';  o = (Test-M3SeedInjected -Side csharp -LogPath $csGood -ExpectSeed 42); want = $true },
    @{ n = 'csharp/无 INJECT_OK=红';    o = (Test-M3SeedInjected -Side csharp -LogPath $csBad -ExpectSeed 42); want = $false },
    @{ n = 'csharp/种子 7!=42=红';      o = (Test-M3SeedInjected -Side csharp -LogPath $csWrong -ExpectSeed 42); want = $false },
    @{ n = 'csharp/空路径=红(fail-closed)'; o = (Test-M3SeedInjected -Side csharp -LogPath '' -ExpectSeed 42); want = $false },
    @{ n = 'rust/SEED_PROBE 42=绿';    o = (Test-M3SeedInjected -Side rust -LogPath $rsGood -ExpectSeed 42); want = $true },
    @{ n = 'rust/无 SEED_PROBE=红';    o = (Test-M3SeedInjected -Side rust -LogPath $rsBad -ExpectSeed 42); want = $false },
    @{ n = 'rust/启动器行 7!=42=红';    o = (Test-M3SeedInjected -Side rust -LogPath $rsMismatch -ExpectSeed 42); want = $false }
  )
  foreach ($c in $evChecks) {
    if ($c.o.ok -ne $c.want) { $failures += ('seed_assert_' + ($c.n -replace '[^A-Za-z0-9]', '_')) }
  }
  Write-Output ("SELFTEST ⑬ 种子注入断言（离线）：{0}/{1} 向符合预期（csharp INJECT_OK / rust SEED_PROBE / 种子不符必红 / 空路径 fail-closed）" -f (@($evChecks | Where-Object { $_.o.ok -eq $_.want }).Count), $evChecks.Count)

  Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  if ($failures.Count -eq 0) {
    Write-Output 'SELFTEST_RED GREEN: 无栈自检全过（攻击配置透传/默认关 + run_pair -DisableAutoPlay 耦合两向 + 攻击两相 pin pose/SQL（-PinMapName 透传含 MapName 断言） + pin SQL 可执行性（含改坏 a.IncHealth 必红 + 改坏 Hp=100000 越界必红） + pin 重试账本（owner 16:30：期望 attempts=2 与仪器同源 + 五向构造输入 + 改坏必红真跑还原） + 攻击 pin 无怪口径（R-1，按地图查提取件：生产 0137 绿/source_verified + 换回 map 0 必红 + 合成 0/1 行·声明漂移·缺 provenance 四向有牙） + 攻击 pin 非 MonGen AI 消费者口径（R-1 refine 15:05：生产 0137 绿/source_verified + 合成 有引用/声明漂移/缺 provenance/缺提取件 四向必红） + 入口/DiffOnly fail-closed + attack_diff selftest + 改坏 dmg/hit 必红 + Invoke-AttackAlign 返回值形态两处（F-J：⑧ 生产函数直接断言 + ⑮） + scoped_diff 14/14 + M-1 WARN 有牙齿 + M-2 attacker 精确选择 selftest + pin SQL 别名/列校验 + 种子注入断言（两侧 fail-closed）+ 攻击序列收尾等待（F-E））'
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
  $localScope = Get-M3SessionAttackScope -Dir $rd
  if ($null -eq $localScope) { Write-Output ("PREREQ_MISSING: Rust 会话读不出攻击序列开关: {0}" -f $rd); exit 2 }
  $localPose = Get-M3SessionAttackPose -Dir $rd
  if ($null -eq $localPose) { Write-Output ("PREREQ_MISSING: Rust 会话读不出攻击起点: {0}" -f $rd); exit 2 }
  $localScenario = (Get-Content (Join-Path $rd 'baseline.json') -Raw | ConvertFrom-Json).fixed_new_chr
  Invoke-AttackCrossSideDiff $cd $rd $localScope $localPose $localScenario
  exit $script:AttackDiffRc
}

# ===========================================================================
# Mode: 侧运行（csharp / rust）
# ===========================================================================
if ([string]::IsNullOrEmpty($Prefix)) {
  $Prefix = 'm3a' + (Get-Date -Format 'HHmmss')
}
if (-not $AttackPinPair) {
  Write-Output 'PREREQ_MISSING: 本脚本要求 -AttackPinPair（attacker/victim 必须相邻固定起点）——见卡 comment 度量口径'
  exit 2
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
  Write-Output 'RED: 入口端口被两种实现混占——先统一'
  exit 1
}

# F-D（m3cs tick21）：pin 抬高值范围守卫——越界就 fail-closed，不等真机 MySQL `ERROR 1264`。
# 放在入口护栏之后（-SelfCheckNoStack / -DiffOnly / 无栈入口已各自 fail-closed），只卡真实侧运行。
if (-not (Test-M3AttackVictimHpRange -VictimHp $AttackVictimHp)) {
  Write-Output ("RED: -AttackVictimHp={0} 越界（characters_ablity.Hp/MaxHP = smallint 有符号，1..32767）——见 F-D" -f $AttackVictimHp)
  exit 1
}

Write-Output '--- 0.5) preflight：Rust 断线存档（T-27） ---'
if ($Side -eq 'rust' -and -not $SkipSavePreflight) {
  if (-not (Test-RustDisconnectSaveImplemented)) {
    Write-Output 'PREREQ_MISSING: Rust game-svr 未实现断线/进世界存档（无 DB_SAVEHUMANRCD）——after 快照必是建角状态；见 whitelist T-27'
    exit 2
  }
  Write-Output 'PREFLIGHT Rust 断线存档：已实现'
} else {
  Write-Output 'PREFLIGHT Rust 断线存档：跳过'
}

Write-Output '--- 0.6) 种子注入断言（F-C，两侧同种子的前提；缺失 ⇒ fail-closed 红） ---'
$csharpSeedLogResolved = if (-not [string]::IsNullOrEmpty($CsharpSeedLog)) { $CsharpSeedLog }
  elseif (-not [string]::IsNullOrEmpty($env:MIR2_RNG_LOG)) { $env:MIR2_RNG_LOG }
  else { '' }
$rustSeedLogResolved = if (-not [string]::IsNullOrEmpty($RustSeedLog)) { $RustSeedLog }
  else { Join-Path $RustStackLogDir 'game-svr.out.log' }
if ($Side -eq 'csharp') {
  $seedEv = Test-M3SeedInjected -Side csharp -LogPath $csharpSeedLogResolved -ExpectSeed $Seed
  if (-not $seedEv.ok) {
    Write-Output ("RED: C# 种子注入断言失败（期望 seed={0}）——{1}" -f $Seed, $seedEv.reason)
    Write-Output '     修法：用带 rng-hook 的 C# 栈（MIR2_RNG_SEED/MIR2_RNG_LOG）起栈后，传 -CsharpSeedLog <rng-stream.tsv> 或设 $env:MIR2_RNG_LOG。'
    exit 1
  }
  Write-Output ("SEED_INJECT_OK csharp seed={0} <- {1}" -f $seedEv.seed, $csharpSeedLogResolved)
} else {
  $seedEv = Test-M3SeedInjected -Side rust -LogPath $rustSeedLogResolved -ExpectSeed $Seed
  if (-not $seedEv.ok) {
    Write-Output ("RED: Rust 种子注入断言失败（期望 seed={0}）——{1}" -f $Seed, $seedEv.reason)
    Write-Output '     修法：用 start_rust_stack.ps1 -Seed <n> 起栈（openmir2-m3-rust-stack-seed-plumb 的种子管线），或传 -RustSeedLog <game-svr 启动日志>。'
    exit 1
  }
  Write-Output ("SEED_INJECT_OK rust seed={0} <- {1}" -f $seedEv.seed, $rustSeedLogResolved)
}

Write-Output '--- 1) 预检：MySQL / 前缀 ---'
if (-not (Test-MysqlTcp)) {
  Write-Output 'PREREQ_MISSING: MySQL 127.0.0.1:3306 不可达'
  exit 2
}
Write-Output 'PREFLIGHT MySQL 在听'

# ---- 1.5) pin SQL 可执行性（F-B）：真执行一次，列挂错表（如历史 `a.IncHealth`）不再等到 run_pair 半途才炸 ----
# 复用 run_pair 的 `-EmitOpsArgs`（pin SQL 的唯一口径）取 attack_pin_sql_all；每条以不存在角色名执行，0 行改动。
$probeArgs = @('-EmitOpsArgs', '-FixedNewChr', '-PinStartPose', '-AttackPinPair', '-AttackSequence',
               '-Bots', $Bots, '-Prefix', $Prefix, '-AttackCount', $AttackCount, '-AttackStepMs', $AttackStepMs,
               '-AttackStartDelayMs', $AttackStartDelayMs, '-AttackPinDir', $AttackPinDir,
               '-PinMapName', $PinMapName, '-PinCX', $AttackPinCX, '-PinCY', $AttackPinCY, '-AttackVictimHp', $AttackVictimHp)
$probeRaw = @(& powershell -ExecutionPolicy Bypass -File $RunPair @probeArgs)
if ($LASTEXITCODE -ne 0) {
  Write-Output 'RED: 取 pin SQL 失败（run_pair -EmitOpsArgs 非 0）——无法做可执行性预检'
  exit 1
}
$probeText = ((@($probeRaw) | Where-Object { $_ -notmatch '^\s*(WARN:|PIN_NO_MONSTER|MAP_HAS_NO_MONSTER|MAP_HAS_NO_AI_CONSUMERS)' }) -join "`n").Trim()
$probeObj = $probeText | ConvertFrom-Json
try { $probeSql = @($probeObj.attack_pin_sql_all) } catch { $probeSql = @() }
if ($probeSql.Count -lt 1) {
  Write-Output 'RED: pin SQL 预检取不到 attack_pin_sql_all（run_pair 未产出）'
  exit 1
}
$probeExec = Test-M3PinSqlExecutable -SqlStatements $probeSql -MysqlExe $MysqlExe
if ($probeExec.rc -ne 0) {
  Write-Output ("RED: pin SQL 不可执行（prepare 期错误）——{0}" -f ($probeExec.failures -join ';'))
  exit 1
}
Write-Output ("PREFLIGHT pin SQL 可执行性：{0} 条 UPDATE 在镜像临时表上以探针行真执行，mysql rc=0（prepare 期 + 范围期）" -f $probeSql.Count)

Write-Output '--- 2) 侧运行（复用 run_pair.ps1；同序列 = -FixedNewChr + -AttackSequence + -AttackPinPair） ---'
$pairArgs = @('-Side', $Side, '-Bots', $Bots, '-Prefix', $Prefix, '-Address', $Address, '-Port', $Port,
              '-TimeoutSec', $TimeoutSec, '-FixedNewChr', '-PinStartPose', '-AttackPinPair',
              '-AttackSequence', '-AttackCount', $AttackCount, '-AttackStepMs', $AttackStepMs, '-AttackStartDelayMs', $AttackStartDelayMs,
              '-AttackPinDir', $AttackPinDir, '-PinMapName', $PinMapName, '-PinCX', $AttackPinCX, '-PinCY', $AttackPinCY, '-AttackVictimHp', $AttackVictimHp)
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

# F-C 留痕：把本侧种子注入证据写进会话目录（两侧种子的可追溯锚，供跨侧审计/diff 前提核对）。
$seedLogUsed = if ($Side -eq 'csharp') { $csharpSeedLogResolved } else { $rustSeedLogResolved }
$seedMeta = [ordered]@{
  side = $Side; seed = $Seed; ok = $true; expect_seed = $Seed
  evidence_log = $seedLogUsed; matched = $seedEv.matched; ts = (Get-Date).ToString('o')
}
$seedMeta | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $mine.FullName 'attack_seed.json') -Encoding utf8
Write-Output ("SEED_EVIDENCE_RECORDED {0}" -f (Join-Path $mine.FullName 'attack_seed.json'))

# 同序列判据：baseline.json 的 attack_markers 必须存在（run_pair 已机判完整串）。
$baseline = Get-Content (Join-Path $mine.FullName 'baseline.json') -Raw | ConvertFrom-Json
$markers = @($baseline.attack_markers)
if ($markers.Count -eq 0 -or -not (Test-Path (Join-Path $mine.FullName 'attack_raw.jsonl'))) {
  Write-Output ("RED: 同序列判据不成立（attack_markers={0} 条 / attack_raw.jsonl 缺失）——驱动没跑完整串" -f $markers.Count)
  exit 1
}
Write-Output ("ATTACKSEQ_OK markers={0} raw=True" -f $markers.Count)

# pin 重试账本（owner 2026-10-12 16:30）：本侧自己的运行也必须零额外重试，否则这份基线不可比
# （即使 C# 侧尚未跑）。fail-closed（PREREQ，exit 2）——拒绝把被重试污染的运行当「同一序列」。
$localPin = Get-M3PinAttemptsGuard -PlayerDir $mine.FullName
foreach ($m in @($localPin.messages)) { Write-Output $m }
if (@($localPin.failed).Count -gt 0) {
  Write-Output ("PREREQ_MISSING: 本侧 pin 重试超期望（{0}）——本基线不可比，需重跑至零额外重试" -f (@($localPin.failed) -join ','))
  exit 2
}
Write-Output ("PIN_ATTEMPTS_OK local attempts={0}/expected={1}" -f $localPin.info.attempts, $localPin.info.expected)

$alignRc = Invoke-AttackAlign $mine.FullName
if ($alignRc -ne 0) {
  Write-Output ("RED: attack_diff align 失败（rc={0}）" -f $alignRc)
  exit 1
}

if ($Side -eq 'rust' -and -not [string]::IsNullOrEmpty($PeerSession)) {
  $peer = Join-Path $Sessions $PeerSession
  if (-not (Test-Path $peer)) { Write-Output ("PREREQ_MISSING: 找不到 C# 对照会话 {0}" -f $peer); exit 2 }
  $localScope = Format-M3AttackScope -AttackSequence $true -AttackCount $AttackCount -AttackStepMs $AttackStepMs -AttackStartDelayMs $AttackStartDelayMs -DisableAutoPlay $true
  $ownScope = Get-M3SessionAttackScope -Dir $mine.FullName
  if ($ownScope -ne $localScope) {
    Write-Output ("RED: 本侧 baseline.json 记录的开关与实际不一致——baseline='{0}' vs 实际='{1}'" -f $ownScope, $localScope)
    exit 1
  }
  $localPose = Get-M3SessionAttackPose -Dir $mine.FullName
  if ($null -eq $localPose) { Write-Output ("PREREQ_MISSING: 本侧会话读不出攻击起点: {0}" -f $mine.FullName); exit 2 }
  $localScenario = (Get-Content (Join-Path $mine.FullName 'baseline.json') -Raw | ConvertFrom-Json).fixed_new_chr
  Invoke-AttackCrossSideDiff $peer $mine.FullName $localScope $localPose $localScenario
  exit $script:AttackDiffRc
}

if ($Side -eq 'csharp') {
  Write-Output ("NEXT: 换 Rust 栈后跑  run_attack_pair.ps1 -Side rust -Bots {0} -AttackCount {1} -AttackPinPair -PeerSession {2}" -f $Bots, $AttackCount, $mine.Name)
} else {
  Write-Output 'NOTE: 未给 -PeerSession，跳过跨侧 diff（-Side rust 的正式判据要 C# 对照会话）'
}
Write-Output 'GREEN: 侧运行与同序列判据通过'
exit 0
