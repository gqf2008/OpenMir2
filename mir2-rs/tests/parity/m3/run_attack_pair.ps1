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
  **F-F（owner 2026-10-12 22:35）**：光写大 `Hp/MaxHP` 不生效——进世界时 oracle `RecalcLevelAbilitys()`（`PlayObject.cs:5295`）会按 `Abil.Level`+`Job` 重算 `MaxHP` 并把 `HP` 钳回去（L1 战士 ⇒ 19）⇒ pin 必须把 `Level/Job` 一并钉到与目标 HP 自洽的值（本片缺省 L255/战士 ⇒ 20669），并有「pin 后复核 maxhp == 期望」的 fail-closed 门（run 末尾 `VICTIM_HP_POSTPIN` + 自检 ⑥d/⑥e）。
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
    0.7 活栈隔离口径（owner 2026-10-12 21:15，§6.0-3 签字）  `-IsolateAttack` / `$env:MIR2_RNG_ISOLATE_ATTACK`
                 开启时：本侧 rng 流水头必须有 `ISOLATE_ATTACK <attack_seed>`（attack_seed = Seed + 0x9E37，
                 i32 回绕）；C# 侧另机判 `rng-hook/divert-sites.txt` vs `rng-isolate-attack/attack_sites.tsv`
                 （check_predicates.py static + table-vs-csharp，缺一条/有未登记 site ⇒ RED）。会话产物写
                 `attack_isolation.json`（隔离开关 + attack_seed + dump sha256 + 专用流 sha256）；跨侧比前
                 另有护栏：两侧 `isolate_attack` 必须一致（开启时 attack_seed 一致），否则 PREREQ。
                 **默认关**：不构造专用流、RNG 行为逐字节不变（只多写一份 isolate_attack=false 的记账）。
    1 预检       入口端口不全在听 / MySQL 3306 不可读 ⇒ PREREQ（exit 2），不静默通过。
    1.5 pin SQL 可执行性（F-B + F-D）  在会话内建 `TEMPORARY TABLE ... LIKE` 并把每条 `UPDATE` 重写到镜像表、
                 以固定探针行真执行一次 ⇒ mysql rc 必须 0；列挂错表（如历史 `a.IncHealth` = ERROR 1054）
                 与越界值（如 `a.Hp=100000` 写 smallint = ERROR 1264）都当场 RED，不再等到 run_pair 半途才炸；
                 临时表会话结束即消失，真实数据零改动。
    2 同序列     run_pair -AttackSequence 后 bot.log 必须出现 `M3ATTSEQ`：`role=attack` k=1..N
                 且 `role=done attacks=N`；缺 ⇒ RED。
    2.5 pin 后复核受害者 HP（F-F）  会话 `attack_raw.jsonl` 里每条 `role=struck` 的 `maxhp`（= 服务端
                 `WAbil.MaxHP`）必须 == 期望（缺省 20669 = L255/战士）；被 `RecalcLevelAbilitys` 重算回 19
                 ⇒ `VICTIM_HP_POSTPIN` RED（fail-closed）。
    2.6 目标可用性门（owner 23:50 第 ③ 条 / 00:20 两侧都强制）  **两侧都用**服务端权威逐击状态 dump 判
                 （客户端视图看不到 ghost）：dump 最后 -AttackCount 行里，攻方前方格必须有非攻方、
                 !ghost && !death 的玩家族对象（允许 1 击起跑竞态）。C# 半 `-CsharpStateLog` /
                 `start-csharp-hook.ps1 -StateLog`；Rust 半 `-RustStateLog` / `start_rust_stack.ps1 -StateLog`。
                 **任一侧缺 dump ⇒ RED（fail-closed）**；改坏（受害 ghost ⇒ 门点名 k）必红，自检 ⑦c。
    2.7 C# 专用流纯度门（F-N1，owner 22:15 ① / m3cs 2026-10-13）  隔离开启且 `-Side csharp` 时，专用流
                 （`-AttackStreamLog` / `$env:MIR2_RNG_ATTACK_LOG`）必须**只含被测量玩家攻方单元**：首单元必须是
                 `atk_power_a`/`luck_a`，player 单元数 == `-AttackCount`，base 单元数 == 0。
                 调用点级 divert 会把别图怪物/守卫（非玩家攻方）吸进同一条 `Random(attack_seed)` ⇒ 流位置被推移；
                 `attack_stream_gate.py` 当场判红并**拒比**（fail-closed），缺流文件 ⇒ RED。hook 侧已按「被测量攻方
                 线程」收窄（`StartupHook.cs`）。自检 22c：干净绿 / 混入 base 单元·player 数不符 必红。
    3 跨侧口径   两侧 baseline.json 的 `attack_sequence` / `attack_pose` / `fixed_new_chr` 必须一致；
                 不一致 ⇒ PREREQ（exit 2）。
    3.1 pin 记账门（owner 2026-10-12 16:30；**19:20 降级为 informational**）  `baseline.json.pin_attempts`
                 **键在但值空/(off)/形态坏/baseline 坏 JSON ⇒ PREREQ exit 2**（仪器写了字段却没落有效记账）；
                 **旧会话（根本没有该键）⇒ informational、不再拒比**（owner 19:20「降级后旧会话不再被拒」）；
                 attempts 数值差异（> / < / expected 漂移）只打 INFO，**不再拒绝比对**。依据：owner 19:20 判决性
                 实验（attempts 既不充分也不必要——4↔5 与 5↔5 时 attack_seq 都不同；而 c1≡c3 attempts 3/4 却逐字节全等）。
                 另：预热相必须带 `-StopAfterCharCreated`，且预热 BotSrv 日志不得出现进世界标记（防 GameSvr 异步存档覆盖 pin），
                 且必须有建角正证（生产整行 `创建角色 <name>`（`SelectChrScene.cs:200`，**带尾空格**，与建角失败串逐字互斥）/`预热建角完成`）——日志零 marker 也 fail-closed（reviewer-6 M-1）；只见建角失败串而无成功正证 ⇒ `prewarm_char_create_failed` fail-closed（reviewer-5 N-1）
                 ——见 `gate_guards.ps1::Test-M3PrewarmNoWorldEntry`。
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
  [int]$AttackVictimHp = 0,        # F-F：0 ⇒ gate_guards 的 oracle 等级派生值（L255/战士=20669）；显式值必须与 -AttackVictimLevel 自洽
  [int]$AttackVictimLevel = 0,     # F-F：victim/attacker 钉的等级（byte 0..255）；0 ⇒ $script:M3AttackVictimLevel
  [string]$PeerSession = '',
  [string]$CsharpSession = '',
  [string]$RustSession = '',
  [string]$PortsCsv = '',          # 入口端口（逗号分隔）；默认 5000,7000,7100,7200。自检时可指一列空闲端口
  [switch]$AttackPinPair,          # 攻击变体：两 bot 相邻固定起点（attacker/victim）
  [int]$Seed = 42,                 # F-C：两侧必须注入的同一个种子（`MIR2_RNG_SEED`）
  [string]$CsharpSeedLog = '',     # F-C：C# rng-hook 的 `MIR2_RNG_LOG` 文件（含 `INJECT_OK <seed>`）；缺省读 $env:MIR2_RNG_LOG
  [string]$RustSeedLog = '',       # F-C：Rust game-svr 启动日志（含 `SEED_PROBE`）；缺省 <RustStackLogDir>\game-svr.out.log
  [string]$RustRngLog = '',        # 隔离口径：Rust 侧 rng 流水（含 `ISOLATE_ATTACK <seed>`）；缺省 $env:MIR2_RNG_LOG / <RustStackLogDir>\rng-stream.tsv
  [string]$RustStackLogDir = 'E:\tmp\m2-acceptance\stack',  # T-30/`openmir2-m3-rust-stack-seed-plumb` 的默认 -LogDir
  [switch]$SkipSavePreflight,      # 仅供调试：跳过「Rust 侧断线存档未实现」前置
  [switch]$IsolateAttack,          # owner 2026-10-12 21:15：攻击路径专用种子流（C# 栈须以 MIR2_RNG_ISOLATE_ATTACK=1 起）
  [string]$IsolateSites = '',      # C# 静态分流站点 dump（缺省读 $env:MIR2_RNG_ISOLATE_SITES，再由流水头补）
  [string]$AttackStreamLog = '',   # 专用流流水（缺省读 $env:MIR2_RNG_ATTACK_LOG）；夹具记其 sha256
  [string]$CsharpStateLog = '',    # owner 23:50：C# 半服务端权威逐击状态 dump（缺省读 $env:MIR2_RNG_STATE_LOG）
  [string]$RustStateLog = '',      # owner 2026-10-13 00:20：Rust 半同形 dump（m2-svr 管线；缺省读 $env:MIR2_RNG_STATE_LOG / <RustStackLogDir>\rng-server-state.tsv）
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
$ServerStateGate = Join-Path $Here 'server_state_gate.py'
$AttackStreamGate = Join-Path $Here 'attack_stream_gate.py'   # F-N1：C# 专用流「只有被测量攻方」fail-closed 门
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
# F-F（owner 2026-10-12 22:35）：攻击 pin 的 `Level/Job/Hp` 口径归 gate_guards（oracle 等级公式）；
# 缺省 0 ⇒ 用脚本常量（L255/战士 ⇒ 20669），显式值必须自洽（`Get-M3AttackPinSql` 会 fail-closed）。
if ($AttackVictimHp -le 0) { $AttackVictimHp = $script:M3AttackVictimHp }
if ($AttackVictimLevel -le 0) { $AttackVictimLevel = $script:M3AttackVictimLevel }

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

# owner 2026-10-12 23:50 第 ③ 条：目标可用性 fail-closed 门（服务端权威 dump）。
# 唯一口径见 server_state_gate.py 头（该门咬住「受害者被静默 ghost ⇒ GetPoseCreate()==null」这类
# **客户端视图看不到**的目标消失；旧行为：夹具不判它，m3cs 只能从 RNG 计数事后推断）。
function Invoke-ServerStateGate([string]$StatePath, [int]$Attacks, [int]$AttackerId, [int]$Vx, [int]$Vy, [int]$Tolerance = 1) {
  $out = & python -I $ServerStateGate --state $StatePath --attacks $Attacks --attacker-id $AttackerId --victim-x $Vx --victim-y $Vy --tolerance $Tolerance 2>&1
  foreach ($line in @($out)) { Write-Host $line }
  return [int]$LASTEXITCODE
}

# owner 2026-10-13 00:20（owner 23:50 第 ③ 条的「两侧都强制」）：目标可用性门的**两侧接线**唯一口径。
# C# 半由 `start-csharp-hook.ps1 -StateLog` 注入 `MIR2_RNG_STATE_LOG`；Rust 半由 m2-svr 的
# `start_rust_stack.ps1 -StateLog`（`bins/game-svr` 的 MIR2_RNG_STATE_LOG 管线）注入同名变量。
# 本函数：解析路径（显式 → 环境 → 侧默认）+ **fail-closed**（缺文件 ⇒ rc=1、reason=missing_state_log，
# 不再退化成旧「-Side rust 只打 NOTE 放行」）+ 跑 `server_state_gate.py`。
# **禁止**在这里静默放行：任一侧缺 dump 都必须是 RED——否则这道门只保护一半（owner 00:20 点名的边界）。
function Invoke-M3TargetAvailabilityGate {
  param(
    [ValidateSet('csharp', 'rust')][string]$Side,
    [string]$ExplicitStateLog = '',
    [int]$Attacks,
    [int]$AttackerId,
    [int]$Vx,
    [int]$Vy,
    [int]$Tolerance = 1
  )
  $resolved = ''
  if (-not [string]::IsNullOrEmpty($ExplicitStateLog)) { $resolved = $ExplicitStateLog }
  elseif (-not [string]::IsNullOrEmpty($env:MIR2_RNG_STATE_LOG)) { $resolved = $env:MIR2_RNG_STATE_LOG }
  elseif ($Side -eq 'csharp' -and (Test-Path -LiteralPath 'E:\tmp\m3-csharp-hook\rng-server-state.tsv')) {
    $resolved = 'E:\tmp\m3-csharp-hook\rng-server-state.tsv'
  } elseif ($Side -eq 'rust') {
    $rustDefault = Join-Path $RustStackLogDir 'rng-server-state.tsv'
    if (Test-Path -LiteralPath $rustDefault) { $resolved = $rustDefault }
  }
  if ([string]::IsNullOrEmpty($resolved) -or -not (Test-Path -LiteralPath $resolved)) {
    return [pscustomobject]@{ rc = 1; side = $Side; path = $resolved; reason = 'missing_state_log' }
  }
  $rc = Invoke-ServerStateGate -StatePath $resolved -Attacks $Attacks -AttackerId $AttackerId -Vx $Vx -Vy $Vy -Tolerance $Tolerance
  return [pscustomobject]@{ rc = $rc; side = $Side; path = $resolved; reason = '' }
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
  # ①.5 pin 记账字段存在性护栏（owner 2026-10-12 16:30；**19:20 降级为 informational**）：
  #    只对「字段缺失/不可解析」fail-closed（PREREQ，exit 2；仪器接线断裂）；attempts 数值差异
  #    （重试/期望漂移）只打 INFO，不再拒比（owner 19:20 判决性实验：attempts 既不充分也不必要）。
  $pinGuard = Test-M3PinAttemptsComparable -Dirs @($csharpDir, $rustDir)
  foreach ($m in @($pinGuard.messages)) { Write-Output $m }
  if (@($pinGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: pin 记账字段缺失/不可解析（{0}）——仪器接线断裂，拒绝比对" -f (@($pinGuard.failed) -join ','))
    return
  }
  foreach ($m in @($pinGuard.informational)) { Write-Output $m }
  $pinInfos = @($pinGuard.infos | ForEach-Object { ("{0}:attempts={1}/expected={2}{3}" -f $_.dir, $_.attempts, $_.expected, $(if ($_.retried) { '(informational:retried)' } else { '' })) })
  Write-Output ("PIN_ATTEMPTS_INFO {0}（informational：attempts 数值不拒比）" -f ($pinInfos -join ' '))
  # ①.6 隔离口径护栏（owner 2026-10-12 21:15）：两侧必须同一隔离口径（同开/同关；开启时同 attack_seed）。
  $isoGuard = Get-M3PeerIsolationGuard -PeerDir $csharpDir -LocalDir $rustDir
  foreach ($m in @($isoGuard.messages)) { Write-Output $m }
  if (@($isoGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧隔离口径不一致（{0}）——两相不可比" -f (@($isoGuard.failed) -join ','))
    return
  }
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

  # ②b 预热相「不进世界」开关透传（owner 2026-10-12 19:20 根因修）：-StopAfterCharCreated 必须落配置；默认关。
  if ($off.StopAfterCharCreated -ne $false) { $failures += 'stop_after_char_created_default_not_off' }
  & powershell -ExecutionPolicy Bypass -File $RunBots -StopAfterCharCreated -WriteConfigOnly -SkipBuild | Out-Null
  $onStop = (Get-Content $cfgPath -Raw | ConvertFrom-Json).BotPlay
  if ($onStop.StopAfterCharCreated -ne $true) { $failures += 'stop_after_char_created_not_written_when_on' }
  & powershell -ExecutionPolicy Bypass -File $RunBots -WriteConfigOnly -SkipBuild | Out-Null
  $offStop = (Get-Content $cfgPath -Raw | ConvertFrom-Json).BotPlay
  if ($offStop.StopAfterCharCreated -ne $false) { $failures += 'stop_after_char_created_not_restored_off' }
  Write-Output ("SELFTEST ②b StopAfterCharCreated 透传 on={0}/off={1}（默认关；改坏写入⇒本项必红）" -f $onStop.StopAfterCharCreated, $offStop.StopAfterCharCreated)

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
    $wantPose = ('pair map={0} attacker={1},{2},dir=2 victim={3},{2} level={4} vhp={5}' -f $PinMapName, $AttackPinCX, $AttackPinCY, ($AttackPinCX + 1), $AttackVictimLevel, $AttackVictimHp)
    if ($pair.attack_pose -ne $wantPose) { $failures += ('attack_pose=' + $pair.attack_pose) }
    $all = @($pair.attack_pin_sql_all)
    if ($all.Count -lt 4) { $failures += ('attack_pin_sql_all_count=' + $all.Count) }
    $joined = ($all -join ' || ')
    if ($joined -notmatch 'ChrName=''m3[^'']*0''') { $failures += 'attack_pin_missing_attacker_row' }
    if ($joined -notmatch ('CX=' + ($AttackPinCX + 1) + '[^0-9]')) { $failures += 'attack_pin_missing_victim_cell' }
    if ($joined -notmatch ('a\.Hp={0}, a\.MaxHP={0}' -f $AttackVictimHp)) { $failures += 'attack_pin_missing_victim_hp' }
    # F-F：pin 必须把 `Level`/`Job` 一并钉死（否则进世界被 `RecalcLevelAbilitys` 重算回 19）。
    if ($joined -notmatch ('a\.Level={0}' -f $AttackVictimLevel)) { $failures += ('attack_pin_missing_victim_level=' + $AttackVictimLevel) }
    if ($joined -notmatch ('c\.Job={0}[^0-9]' -f $script:M3AttackVictimJob)) { $failures += ('attack_pin_missing_victim_job=' + $script:M3AttackVictimJob) }
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
    $mutHp = @($all | ForEach-Object { $_ -replace ('a\.Hp={0}, a\.MaxHP={0}' -f $AttackVictimHp), 'a.Hp=100000, a.MaxHP=100000' })
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

  # ⑥d F-F（owner 2026-10-12 22:35）：「pin 真把受害者 HP 抬起来」的离线门 ——
  #    ① oracle 公式锚：L1 战士 = 19（= 真机实测 maxhp），L{AttackVictimLevel} 战士 = {AttackVictimHp}；
  #    ② 自洽门：pin 的 Level/Job 必须能反推出 pin 的 Hp；
  #    ③ 改坏必红：Level 留 255 但 Hp 写回 19 / Hp 写回旧缺省 30000（不是任何≤255 等级的战士 MaxHP）⇒ 必红；
  #    ④ 幸存上界：Hp > AttackCount * (dc_hi + 1)（AC 减免只会更小）。
  $laAnchor = Get-M3AttackVictimLevelAbility -Level 1 -Job 0
  if ($laAnchor.max_hp -ne 19) { $failures += ('f_f_anchor_L1_maxhp=' + $laAnchor.max_hp) }
  $laVictim = Get-M3AttackVictimLevelAbility -Level $AttackVictimLevel -Job $script:M3AttackVictimJob
  if ($laVictim.max_hp -ne $AttackVictimHp) { $failures += ('f_f_derived_maxhp=' + $laVictim.max_hp + ' vs pinned=' + $AttackVictimHp) }
  $selfOk = Test-M3AttackVictimHpSelfConsistent -Level $AttackVictimLevel -Job $script:M3AttackVictimJob -Hp $AttackVictimHp
  if (-not $selfOk.ok) { $failures += ('f_f_self_consistent=' + (@($selfOk.issues) -join ';')) }
  if ((Test-M3AttackVictimHpSelfConsistent -Level $AttackVictimLevel -Job $script:M3AttackVictimJob -Hp 19).ok) { $failures += 'f_f_self_consistent_mutation_recalc19_not_red' }
  if ((Test-M3AttackVictimHpSelfConsistent -Level $AttackVictimLevel -Job $script:M3AttackVictimJob -Hp 30000).ok) { $failures += 'f_f_self_consistent_mutation_legacy30000_not_red' }
  $surv = Test-M3AttackVictimSurvives -Level $AttackVictimLevel -Job $script:M3AttackVictimJob -Hp $AttackVictimHp -AttackCount $AttackCount
  if (-not $surv.ok) { $failures += ('f_f_victim_may_die: hp={0} bound={1}' -f $surv.hp, $surv.bound) }
  Write-Output ("SELFTEST ⑥d F-F oracle 等级公式：L1 战士 MaxHP={0}（锚 19）、L{1} 战士 MaxHP={2}；单击上界 dc_hi+1={3}；{4} 击最坏 {5} < Hp {6}（改坏 Level/Hp 或写回 19/30000 ⇒ 自洽门必红）" -f `
    $laAnchor.max_hp, $AttackVictimLevel, $laVictim.max_hp, ($surv.dc_hi + 1), $AttackCount, $surv.bound, $AttackVictimHp)

  # ⑥e F-F「pin 后复核 HP == 期望值」fail-closed 门（真机产物 = 受害者 SM_STRUCK 落的 attack_raw.jsonl）。
  #    合成三例：maxhp 匹配=绿；maxhp=19（pin 被重算回旧值，= 改坏必红）/ 零 struck 行 = 必红。
  $pinRawOk = Join-Path $tmpRoot 'ff-struck-ok.jsonl'
  $pinRawBad = Join-Path $tmpRoot 'ff-struck-recalc19.jsonl'
  $pinRawEmpty = Join-Path $tmpRoot 'ff-struck-empty.jsonl'
  Set-Content -Path $pinRawOk -Encoding utf8 -Value @(
    '{"role":"attack","k":1,"op":"CM_HIT","x":17,"y":20,"dir":2,"my":1105911852,"id":"a0"}',
    ('{{"role":"struck","dmg":1,"hp":{0},"maxhp":{1},"src":1105911852,"actor":1512982481,"my":1512982481}}' -f ($AttackVictimHp - 1), $AttackVictimHp),
    ('{{"role":"struck","dmg":1,"hp":{0},"maxhp":{1},"src":1105911852,"actor":1512982481,"my":1512982481}}' -f ($AttackVictimHp - 2), $AttackVictimHp)
  )
  Set-Content -Path $pinRawBad -Encoding utf8 -Value @(
    '{"role":"attack","k":1,"op":"CM_HIT","x":17,"y":20,"dir":2,"my":1105911852,"id":"a0"}',
    '{"role":"struck","dmg":1,"hp":18,"maxhp":19,"src":1105911852,"actor":1512982481,"my":1512982481}'
  )
  Set-Content -Path $pinRawEmpty -Encoding utf8 -Value @('{"role":"done","id":"a0","attacks":1}')
  $postOk = Test-M3AttackVictimPostPinHp -RawPath $pinRawOk -ExpectedHp $AttackVictimHp
  if (-not $postOk.ok) { $failures += ('f_f_postpin_ok_case=' + $postOk.reason) }
  $postBad = Test-M3AttackVictimPostPinHp -RawPath $pinRawBad -ExpectedHp $AttackVictimHp
  if ($postBad.ok) { $failures += 'f_f_postpin_recalc19_not_red' }
  $postEmpty = Test-M3AttackVictimPostPinHp -RawPath $pinRawEmpty -ExpectedHp $AttackVictimHp
  if ($postEmpty.ok) { $failures += 'f_f_postpin_no_struck_not_red' }
  Write-Output ("SELFTEST ⑥e F-F pin 后复核 HP：期望 {0} ⇒ 合成的 maxhp 匹配=绿、maxhp=19（被重算）=红、零 struck=红" -f $AttackVictimHp)

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

  # ⑱ pin 记账门（owner 2026-10-12 16:30；**19:20 降级为 informational**）：
  #    唯一 fail-closed = 键在但值空/`(off)`、形态认不出、baseline 坏 JSON；attempts=4 / expected 漂移 /
  #    旧会话（根本没有 `pin_attempts` 键）均**绿**（只 informational——旧 Minor「无字段旧会话被拒比」已收）。
  #    改坏必红（真跑后还原）：把 `pin_attempts` 值清空（=仪器写了字段却没落有效记账）⇒ 必红；还原 ⇒ 绿。
  #    另：断言 `-EmitOpsArgs` 的 `prewarm_bot_args` 含 `-StopAfterCharCreated`（预热不进世界的接线面）。
  $pinExpected = Get-M3PinExpectedAttempts
  if ($pinExpected -lt 1) { $failures += ('pin_expected_attempts=' + $pinExpected) }
  if ($null -ne $pair) {
    if ([string]$pair.pin_instrument -notmatch 'consecutive=2') { $failures += ('pin_instrument_not_consecutive2=' + $pair.pin_instrument) }
    if (@($pair.prewarm_bot_args) -notcontains '-StopAfterCharCreated') { $failures += 'prewarm_stop_after_char_created_not_passed' }
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
  if (@($cmpRetry.failed).Count -ne 0) { $failures += ('pin_attempts_retry_not_informational_green=' + (@($cmpRetry.failed) -join ';')) }
  if (-not (@($cmpRetry.infos | Where-Object { $_.retried }).Count -gt 0)) { $failures += 'pin_attempts_retry_not_flagged_informational' }
  if (@($cmpRetry.informational).Count -eq 0) { $failures += 'pin_attempts_retry_no_info_message' }
  # 旧会话（baseline.json 根本没有 pin_attempts 键）= 绿（informational）——收掉旧登记 Minor「无字段旧会话被拒比」。
  $pinNoField = New-M3PinSession 'nofield' $null
  $pinNoFieldInfo = Get-M3PinAttemptsGuard -PlayerDir $pinNoField
  if (@($pinNoFieldInfo.failed).Count -ne 0) { $failures += ('pin_attempts_nofield_not_green=' + (@($pinNoFieldInfo.failed) -join ';')) }
  if (@($pinNoFieldInfo.informational).Count -eq 0) { $failures += 'pin_attempts_nofield_no_info' }
  # 键在但值为空 / `(off)` = 红（保留的牙：仪器写了字段却没落有效 attempts）。
  $pinEmpty = New-M3PinSession 'empty' ''
  if (-not (@((Get-M3PinAttemptsGuard -PlayerDir $pinEmpty).failed) -contains 'pin_attempts_missing')) { $failures += 'pin_attempts_empty_not_red' }
  $pinOff = New-M3PinSession 'off' '(off)'
  if (-not (@((Get-M3PinAttemptsGuard -PlayerDir $pinOff).failed) -contains 'pin_attempts_missing')) { $failures += 'pin_attempts_off_not_red' }
  $pinInvalid = New-M3PinSession 'invalid' 'attempts=garbage'
  if (-not (@((Get-M3PinAttemptsGuard -PlayerDir $pinInvalid).failed) -contains 'pin_attempts_invalid')) { $failures += 'pin_attempts_invalid_not_red' }
  $pinDrift = New-M3PinSession 'drift' (Format-M3PinAttempts -Attempts 2 -Expected 3 -SettleRounds 2 -MaxAttempts 5)
  if (@((Get-M3PinAttemptsGuard -PlayerDir $pinDrift).failed).Count -ne 0) { $failures += ('pin_attempts_expected_drift_not_informational_green=' + (@((Get-M3PinAttemptsGuard -PlayerDir $pinDrift).failed) -join ';')) }
  $pinNested = New-M3PinSession 'nested' ([pscustomobject]@{ attempts = 2; expected = 2; settle_rounds = 2; max_attempts = 5 })
  if (@((Get-M3PinAttemptsGuard -PlayerDir $pinNested).failed).Count -ne 0) { $failures += ('pin_attempts_nested_not_green=' + (@((Get-M3PinAttemptsGuard -PlayerDir $pinNested).failed) -join ';')) }
  # 改坏必红（真跑 + 还原）：把 `pin_attempts` 值清空（=仪器写了字段却没落有效记账）⇒ 必红；还原 ⇒ 绿。
  $pinMutFile = Join-Path $pinOkA 'baseline.json'
  $pinOrigJson = Get-Content $pinMutFile -Raw
  $pinPreGreen = (@((Get-M3PinAttemptsGuard -PlayerDir $pinOkA).failed).Count -eq 0)
  Set-Content -Path $pinMutFile -Encoding utf8 -Value '{ "session": "pin-ok-a", "side": "csharp", "pin_attempts": "" }'
  $pinMutInfo = Get-M3PinAttemptsGuard -PlayerDir $pinOkA
  $pinMutRed = (@($pinMutInfo.failed) -contains 'pin_attempts_missing')
  Set-Content -Path $pinMutFile -Encoding utf8 -Value $pinOrigJson
  $pinRestoredGreen = (@((Get-M3PinAttemptsGuard -PlayerDir $pinOkA).failed).Count -eq 0)
  if (-not $pinPreGreen) { $failures += 'pin_mutation_pre_not_green' }
  if (-not $pinMutRed) { $failures += 'pin_mutation_empty_value_not_red' }
  if (-not $pinRestoredGreen) { $failures += 'pin_mutation_restore_not_green' }
  Write-Output ("SELFTEST ⑱ pin 记账门（informational，owner 19:20）：attempts=2 绿 / attempts=3(重试) 绿+informational / 旧会话无字段键 绿+informational / 键在但空/(off) 红 / 形态坏 红 / 期望漂移 绿 / 嵌套对象 绿；改坏必红（值清空 红[{0}]）；prewarm -StopAfterCharCreated={1}" -f `
    (@($pinMutInfo.failed) -join ','), (@($pair.prewarm_bot_args) -contains '-StopAfterCharCreated'))

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
    'M3ATTSEQ {"role":"struck","dmg":3,"hp":7,"maxhp":10,"src":11,"actor":22,"my":11}',
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

  # ⑦b 目标可用性门（owner 2026-10-12 23:50 第 ③ 条）：server_state_gate.py 的离线自检。
  # 内建 6 向：生产形态（允许前 1 击起跑竞态）绿 / 受害 ghost ⇒ 红点名 k / 受害移格 ⇒ 红 /
  # 行数不足 ⇒ PREREQ 2 / 非法 JSON ⇒ PREREQ 2 / tolerance=0 首击缺 ⇒ 红。任一向退化为恒绿 ⇒ 必红。
  $stateGateOut = (& python -I $ServerStateGate --selftest 2>&1 | Out-String)
  $stateGateRc = $LASTEXITCODE
  if ($stateGateRc -ne 0) { $failures += ("server_state_gate_selftest_rc=$stateGateRc") }
  if ($stateGateOut -notmatch 'SELFTEST GREEN') { $failures += 'server_state_gate_selftest_not_green' }
  Write-Output ("SELFTEST ⑦b server_state_gate selftest rc={0}" -f $stateGateRc)

  # ⑦c 目标可用性门的**两侧接线**（owner 2026-10-13 00:20「两侧都强制」）：直接跑生产函数
  #    `Invoke-M3TargetAvailabilityGate`（C# 半 + Rust 半），四向有牙：
  #      (1) 合成好 dump（rust）⇒ rc=0；(2) 受害行 ghost=1 ⇒ rc=1（门点名 k）；
  #      (3) 显式路径不存在 ⇒ rc=1 且 reason=missing_state_log（fail-closed，不许退化成旧「只打 NOTE 放行」）；
  #      (4) csharp 与 rust 走同一函数（csharp + 好 dump ⇒ rc=0）。
  #    改坏：把 rust 分支删掉 / 返回恒 0 / 把缺文件降级成放行 ⇒ 本项必 RED（红检原始件见证据）。
  $gateDir = Join-Path $tmpRoot 'f-ssg'
  New-Item -ItemType Directory -Force -Path $gateDir | Out-Null
  $ssgGood = Join-Path $gateDir 'rust-good.tsv'
  $ssgGhost = Join-Path $gateDir 'rust-ghost.tsv'
  $atkJson = '{"id":100,"x":17,"y":20,"dir":2,"ghost":0,"death":0,"hp":19,"maxhp":19,"map":"0137"}'
  $goodLines = New-Object System.Collections.Generic.List[string]
  $ghostLines = New-Object System.Collections.Generic.List[string]
  for ($k = 1; $k -le 5; $k++) {
    $vicGood = '{"id":200,"x":18,"y":20,"dir":6,"ghost":0,"death":0,"hp":18,"maxhp":19,"map":"0137"}'
    $vicGhost = '{"id":200,"x":18,"y":20,"dir":6,"ghost":' + $(if ($k -ge 4) { 1 } else { 0 }) + ',"death":0,"hp":18,"maxhp":19,"map":"0137"}'
    $goodLines.Add('SERVER_STATE ' + $k + ' [' + $atkJson + ',' + $vicGood + ']')
    $ghostLines.Add('SERVER_STATE ' + $k + ' [' + $atkJson + ',' + $vicGhost + ']')
  }
  $ssgUtf8 = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllLines($ssgGood, $goodLines, $ssgUtf8)
  [System.IO.File]::WriteAllLines($ssgGhost, $ghostLines, $ssgUtf8)
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $ssgGoodRes = Invoke-M3TargetAvailabilityGate -Side 'rust' -ExplicitStateLog $ssgGood -Attacks 5 -AttackerId 100 -Vx 18 -Vy 20 -Tolerance 1
  $ssgGhostRes = Invoke-M3TargetAvailabilityGate -Side 'rust' -ExplicitStateLog $ssgGhost -Attacks 5 -AttackerId 100 -Vx 18 -Vy 20 -Tolerance 1
  $ssgMissingRes = Invoke-M3TargetAvailabilityGate -Side 'rust' -ExplicitStateLog (Join-Path $gateDir 'does-not-exist.tsv') -Attacks 5 -AttackerId 100 -Vx 18 -Vy 20 -Tolerance 1
  $ssgCsharpRes = Invoke-M3TargetAvailabilityGate -Side 'csharp' -ExplicitStateLog $ssgGood -Attacks 5 -AttackerId 100 -Vx 18 -Vy 20 -Tolerance 1
  $ErrorActionPreference = $prevEap
  if ($ssgGoodRes.rc -ne 0) { $failures += ('target_avail_rust_good_rc=' + $ssgGoodRes.rc) }
  if ($ssgGhostRes.rc -ne 1) { $failures += ('target_avail_rust_ghost_rc=' + $ssgGhostRes.rc) }
  if ($ssgMissingRes.rc -ne 1 -or $ssgMissingRes.reason -ne 'missing_state_log') { $failures += ('target_avail_missing_not_failclosed=' + $ssgMissingRes.rc + '/' + $ssgMissingRes.reason) }
  if ($ssgCsharpRes.rc -ne 0) { $failures += ('target_avail_csharp_good_rc=' + $ssgCsharpRes.rc) }
  if ($ssgGoodRes -isnot [pscustomobject]) { $failures += 'target_avail_return_not_structured' }
  Write-Output ("SELFTEST ⑦c 目标可用性门两侧接线：rust-good rc={0} / rust-ghost rc={1} / missing rc={2}({3}) / csharp-good rc={4}" -f $ssgGoodRes.rc, $ssgGhostRes.rc, $ssgMissingRes.rc, $ssgMissingRes.reason, $ssgCsharpRes.rc)

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
    if ($k -ne 3) { $events.Add('M3ATTSEQ {"role":"struck","dmg":' + (20 + ($k % 5)) + ',"hp":' + (100 - $k) + ',"maxhp":100,"src":101,"actor":77,"my":101}') }
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

  # ⑲ 残差③（owner 2026-10-12 17:35）：attack 行缺 `my` ⇒ 判据层必须 **fail-closed**（rc=2），
  #    不许静默降级成「不过滤」（旧实现就是这么把同一击算两次的）。真跑生产 `Invoke-AttackAlign`。
  #    改坏（把 attack 行的 `"my":101` 去掉）⇒ `attack_my_missing_not_rc2` 必红。
  $dirC = Join-Path $tmpRoot 'c'
  New-Item -ItemType Directory -Force -Path $dirC | Out-Null
  $rawC = Join-Path $dirC 'attack_raw.jsonl'
  $eventsC = New-Object System.Collections.Generic.List[string]
  for ($k = 1; $k -le 4; $k++) {
    $eventsC.Add('M3ATTSEQ {"role":"attack","k":' + $k + ',"op":"CM_HIT","x":289,"y":618,"dir":2}')
    $eventsC.Add('M3ATTSEQ {"role":"struck","dmg":5,"hp":' + (20 - $k) + ',"maxhp":30,"src":101,"actor":77,"my":101}')
  }
  [System.IO.File]::WriteAllLines($rawC, $eventsC, $utf8NoBom)
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $myMissingRc = Invoke-AttackAlign $dirC
  $ErrorActionPreference = $prevEap
  if ($myMissingRc -isnot [int]) { $failures += ('attack_my_missing_rc_not_int=' + $(if ($null -ne $myMissingRc) { $myMissingRc.GetType().Name } else { 'null' })) }
  elseif ($myMissingRc -ne 2) { $failures += ('attack_my_missing_not_rc2=' + $myMissingRc) }
  Write-Output ("SELFTEST ⑲ attack 行缺 my ⇒ fail-closed（残差③）：rc={0}（期望 2）" -f (@($myMissingRc) -join '|'))

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
  $prodPinSql += Get-M3AttackPinSql -ChrName 'm3fix0' -CX 289 -CY 618 -Dir 2 -MapName '0' -HomeMap 0 -HomeX 289 -HomeY 618 -Level $AttackVictimLevel -Job $script:M3AttackVictimJob -Hp $AttackVictimHp
  $prodVio = @(Test-M3PinSqlColumns -SqlStatements $prodPinSql -SchemaColumns $schemaCols)
  if ($prodVio.Count -ne 0) { $failures += ('pin_sql_column_violations=' + ($prodVio -join ';')) }
  $mutPinSql = @(Get-M3AttackPinSql -ChrName 'm3fix0' -CX 289 -CY 618 -Dir 2 -MapName '0' -HomeMap 0 -HomeX 289 -HomeY 618 -Level $AttackVictimLevel -Job $script:M3AttackVictimJob -Hp $AttackVictimHp |
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

  # 21. owner 2026-10-12 21:15 活栈隔离口径（夹具 + C# 影子 hook 半）：默认关语义、attack_seed 公式、
  #    流水头解析、C# 分流站点 dump vs Rust 谓词表机判（缺一条/未登记 ⇒ 必红）、C# hook 离线自检。
  $isoOff = Test-M3IsolateAttackValue -Value ''
  $isoOff0 = Test-M3IsolateAttackValue -Value '0'
  $isoOffFalse = Test-M3IsolateAttackValue -Value 'false'
  $isoOn1 = Test-M3IsolateAttackValue -Value '1'
  $isoOnYes = Test-M3IsolateAttackValue -Value 'YES'
  $isoBad = Test-M3IsolateAttackValue -Value 'maybe'
  if (-not ($isoOff.ok -and -not $isoOff.on)) { $failures += 'iso_parse_empty_not_off' }
  if (-not ($isoOff0.ok -and -not $isoOff0.on)) { $failures += 'iso_parse_0_not_off' }
  if (-not ($isoOffFalse.ok -and -not $isoOffFalse.on)) { $failures += 'iso_parse_false_not_off' }
  if (-not ($isoOn1.ok -and $isoOn1.on)) { $failures += 'iso_parse_1_not_on' }
  if (-not ($isoOnYes.ok -and $isoOnYes.on)) { $failures += 'iso_parse_YES_not_on' }
  if ($isoBad.ok) { $failures += 'iso_parse_bad_not_red' }
  if ((Get-M3AttackSeed -Seed 42) -ne (42 + 0x9E37)) { $failures += 'iso_attack_seed_42' }
  if ((Get-M3AttackSeed -Seed 0) -ne 0x9E37) { $failures += 'iso_attack_seed_0' }
  if ((Get-M3AttackSeed -Seed 2147483647) -ne -2147443146) { $failures += ('iso_attack_seed_wrap=' + (Get-M3AttackSeed -Seed 2147483647)) }
  # 流水头解析：有 / 无 / 文件缺失 三向。
  $isoTrace = Join-Path $tmpRoot 'iso-trace.tsv'
  [System.IO.File]::WriteAllText($isoTrace, "1`tINJECT_OK`t42`t0`nSEED_PROBE 1 2 3`nISOLATE_ATTACK 40545 isolate_sites=C:\x\dump.txt`n", (New-Object System.Text.UTF8Encoding($false)))
  $isoTracePlain = Join-Path $tmpRoot 'iso-trace-plain.tsv'
  [System.IO.File]::WriteAllText($isoTracePlain, "1`tINJECT_OK`t42`t0`nSEED_PROBE 1 2 3`n", (New-Object System.Text.UTF8Encoding($false)))
  $ti = Get-M3TraceIsolate -LogPath $isoTrace
  if (-not $ti.present -or $ti.seed -ne 40545 -or $ti.sites_path -ne 'C:\x\dump.txt') { $failures += 'iso_trace_parse' }
  if ((Get-M3TraceIsolate -LogPath $isoTracePlain).present) { $failures += 'iso_trace_plain_not_absent' }
  if ((Get-M3TraceIsolate -LogPath (Join-Path $tmpRoot 'nope.tsv')).present) { $failures += 'iso_trace_missing_not_absent' }
  # C# dump vs Rust 谓词表机判（生产文件）：绿；删一条 / 加一条未登记 ⇒ 红。
  $cursor = Get-M3IsolateSitesCursorPath
  $predOk = Test-M3IsolatePredicates -CsharpDump $cursor
  if ($predOk.rc -ne 0) { $failures += ('iso_predicates_prod_rc=' + $predOk.rc) }
  $curLines = @(Get-Content -LiteralPath $cursor | Where-Object { $_ -notmatch '^\s*#' -and $_.Trim().Length -gt 0 })
  $mutDir = Join-Path $tmpRoot 'iso-mut'
  New-Item -ItemType Directory -Force -Path $mutDir | Out-Null
  $mutDrop = Join-Path $mutDir 'drop-one.txt'
  [System.IO.File]::WriteAllLines($mutDrop, @($curLines | Select-Object -Skip 1), (New-Object System.Text.UTF8Encoding($false)))
  $mutExtra = Join-Path $mutDir 'extra.txt'
  [System.IO.File]::WriteAllLines($mutExtra, @($curLines + 'M2Server.Player.PlayObject.Bogus@PlayObject.cs:1'), (New-Object System.Text.UTF8Encoding($false)))
  $predDrop = Test-M3IsolatePredicates -CsharpDump $mutDrop
  if ($predDrop.rc -eq 0) { $failures += 'iso_predicates_drop_not_red' }
  $predExtra = Test-M3IsolatePredicates -CsharpDump $mutExtra
  if ($predExtra.rc -eq 0) { $failures += 'iso_predicates_extra_not_red' }
  # C# hook 离线自检（解析/公式/站点/路由；编译进同一份 StartupHook.cs + divert-sites.txt）。
  $isoSelftestProj = Join-Path $Here 'rng-hook\selftest\RngIsolateSelftest.csproj'
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $isoSelfOut = (& dotnet run --project $isoSelftestProj -c Release 2>&1 | Out-String)
  $isoSelfRc = $LASTEXITCODE
  $ErrorActionPreference = $prevEap
  if ($isoSelfRc -ne 0) { $failures += ('iso_hook_selftest_rc=' + $isoSelfRc) }
  if ($isoSelfOut -notmatch 'SELFTEST GREEN') { $failures += 'iso_hook_selftest_not_green' }
  Write-Output ("SELFTEST 21 活栈隔离口径（owner 21:15）：开关解析 off/on/fail-closed 六向；attack_seed(42)=40545 / wrap=-2147443146；流水头 有/无/缺文件 三向；C# dump vs Rust 谓词表 生产 rc={0} / 删一条 rc={1} / 加未登记 rc={2}；hook 离线自检 rc={3}" -f `
    $predOk.rc, $predDrop.rc, $predExtra.rc, $isoSelfRc)

  # 22. F1（m3cs tick22，m3bag 写集）：SHA256 必须能读「被 GameSvr 以 FileShare.ReadWrite 持有」的活文件。
  #     旧 `[System.IO.File]::ReadAllBytes`（FileShare.Read）在 holder 共享模式下被拒 ⇒ 专用流 sha256
  #     记不出来（m3cs 只能置空）。本项直接调生产实现 `Get-M3Sha256Hex`（gate_guards dot-source 的
  #     m3hashutil.ps1），并断言旧读法在 holder 下必被拒。改回 `ReadAllBytes` ⇒
  #     `hot_hash_mismatch` / `sharing_violation_not_reproduced` 必红。
  $hotHash = Test-M3Sha256HotFileSelfTest
  if (-not $hotHash.ok) { $failures += ('sha256_hot_file_selftest=' + $hotHash.reason) }
  Write-Output ("SELFTEST 22 SHA256 活文件（F1）：holder(FileShare.ReadWrite) 下 hash={0}（锚 {1}）；旧 File.ReadAllBytes 必被拒={2} ok={3}" -f `
    $hotHash.hot_hash, $hotHash.expected, $hotHash.oldway_denied, $hotHash.ok)

  # 22b. F2（m3cs tick22）：`attack_rng_ledger.py` 的 Rust 攻击路径站点表必须与决策片 `rng_ledger_diff.py`
  #      同表（旧表漏 758/628/630/601/609/831/250 ⇒ 同一条 Rust 流水报 attack=2/段）。其 selftest 直接钉住
  #      这些站点必须分类为 attack、非站点必须为 noise（改回旧表 ⇒ selftest 必红）。
  $ledgerPy = Join-Path $Here 'attack_rng_ledger.py'
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $ledgerOut = (& python $ledgerPy selftest 2>&1 | Out-String)
  $ledgerRc = $LASTEXITCODE
  $ErrorActionPreference = $prevEap
  if ($ledgerRc -ne 0) { $failures += ('attack_rng_ledger_selftest_rc=' + $ledgerRc) }
  if ($ledgerOut -notmatch 'SELFTEST GREEN') { $failures += 'attack_rng_ledger_selftest_not_green' }
  Write-Output ("SELFTEST 22b attack_rng_ledger selftest rc={0}（两侧攻击站点表同表 + 新站点分类有牙）" -f $ledgerRc)

  # 22c. F-N1（owner 2026-10-12 22:15 ① / m3cs 2026-10-13）：C# 专用流的「只有被测量攻方」
  #      fail-closed 门。合成**干净流** ⇒ 绿；把一条非玩家攻方（`atk_power_base`）单元混进去
  #      （= 真机实测的污染形态，`evidence-.../provenance/csharp-attack-stream-stack2.tsv` 首条就是它）
  #      ⇒ 必红且点名；player 单元数 < 期望 ⇒ 必红。自检直接调生产脚本 `attack_stream_gate.py`。
  $asg = Join-Path $Here 'attack_stream_gate.py'
  $asgDir = Join-Path $tmpRoot 'asg'
  New-Item -ItemType Directory -Force -Path $asgDir | Out-Null
  $asgClean = Join-Path $asgDir 'clean.tsv'
  $asgPolluted = Join-Path $asgDir 'polluted.tsv'
  $asgCleanRows = @()
  for ($i = 0; $i -lt 3; $i++) {
    $asgCleanRows += "Next(max)`t1`t0`tM2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929"
    $asgCleanRows += "Next(max)`t10`t3`tM2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2934"
    $asgCleanRows += "Next(max)`t15`t7`tM2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41"
    $asgCleanRows += "Next(max)`t5`t2`tM2Server.Player.PlayObject.AttackSuccess@PlayObject.Attack.cs:223"
  }
  [System.IO.File]::WriteAllLines($asgClean, $asgCleanRows, (New-Object System.Text.UTF8Encoding($false)))
  $asgPollutedRows = @("Next(max)`t1`t0`tM2Server.Actor.BaseObject.GetAttackPower@BaseObject.Base.cs:892",
                       "Next(max)`t8`t3`tM2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41") + $asgCleanRows
  [System.IO.File]::WriteAllLines($asgPolluted, $asgPollutedRows, (New-Object System.Text.UTF8Encoding($false)))
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $asgCleanOut = (& python $asg gate --stream $asgClean --expected 3 2>&1 | Out-String)
  $asgCleanRc = $LASTEXITCODE
  $asgBadOut = (& python $asg gate --stream $asgPolluted --expected 3 2>&1 | Out-String)
  $asgBadRc = $LASTEXITCODE
  $asgShortOut = (& python $asg gate --stream $asgClean --expected 4 2>&1 | Out-String)
  $asgShortRc = $LASTEXITCODE
  $asgSelfOut = (& python $asg selftest 2>&1 | Out-String)
  $asgSelfRc = $LASTEXITCODE
  $ErrorActionPreference = $prevEap
  if ($asgCleanRc -ne 0) { $failures += ('asg_clean_not_green_rc=' + $asgCleanRc) }
  if ($asgBadRc -eq 0) { $failures += 'asg_polluted_not_red' }
  if ($asgBadOut -notmatch 'non_player_attacker_unit') { $failures += 'asg_polluted_no_teeth' }
  if ($asgShortRc -eq 0) { $failures += 'asg_short_player_count_not_red' }
  if ($asgSelfRc -ne 0) { $failures += ('asg_selftest_rc=' + $asgSelfRc) }
  if ($asgSelfOut -notmatch 'SELFTEST GREEN') { $failures += 'asg_selftest_not_green' }
  Write-Output ("SELFTEST 22c C# 专用流纯度门（F-N1）：干净 rc={0} / 混入 base 单元 rc={1}(点名={2}) / player 数少 rc={3}；selftest rc={4}" -f $asgCleanRc, $asgBadRc, ($asgBadOut -match 'non_player_attacker_unit'), $asgShortRc, $asgSelfRc)

  # 22d. F-N2（m3cs 2026-10-13）：start-csharp-hook.ps1 传 `-Repo` 时 `$Here` 不得为 null
  #      （回归：F1 修复 44849fa3 把 `$Here` 赋值移进了 `if ($Repo -eq '')`，而它下面用于 dot-source
  #      `m3hashutil.ps1`）。生产 `-Status` 分支在 dot-source **之后** ⇒ 传 `-Repo` 若能打印端口并 rc=0，
  #      就证明 `$Here` 已无条件赋值；改回去 ⇒ 必抛 `Cannot bind argument to parameter 'Path'`（rc!=0）。
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $hookStatusOut = (& powershell -ExecutionPolicy Bypass -File (Join-Path $Here 'start-csharp-hook.ps1') -Repo $RepoRoot -Status 2>&1 | Out-String)
  $hookStatusRc = $LASTEXITCODE
  $ErrorActionPreference = $prevEap
  if ($hookStatusRc -ne 0) { $failures += ('f_n2_repo_status_rc=' + $hookStatusRc) }
  if ($hookStatusOut -match 'Cannot bind argument to parameter') { $failures += 'f_n2_repo_here_null' }
  Write-Output ("SELFTEST 22d start-csharp-hook -Repo（F-N2）：-Status rc={0}（脚本自身目录 `$Here 已无条件赋值，不再 null 绑定崩溃）" -f $hookStatusRc)

  Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  if ($failures.Count -eq 0) {
    Write-Output 'SELFTEST_RED GREEN: 无栈自检全过（攻击配置透传/默认关 + run_pair -DisableAutoPlay 耦合两向 + 攻击两相 pin pose/SQL（-PinMapName 透传含 MapName 断言） + pin SQL 可执行性（含改坏 a.IncHealth 必红 + 改坏 Hp=100000 越界必红） + pin 记账门（owner 19:20 降级 informational：attempts=2/3/漂移/旧会话无字段键 绿 + 键在但空/(off)/形态坏 红 + 改坏必红值清空） + 预热相 -StopAfterCharCreated 透传两向 + 攻击 pin 无怪口径（R-1，按地图查提取件：生产 0137 绿/source_verified + 换回 map 0 必红 + 合成 0/1 行·声明漂移·缺 provenance 四向有牙） + 攻击 pin 非 MonGen AI 消费者口径（R-1 refine 15:05：生产 0137 绿/source_verified + 合成 有引用/声明漂移/缺 provenance/缺提取件 四向必红） + 入口/DiffOnly fail-closed + attack_diff selftest + 改坏 dmg/hit 必红 + Invoke-AttackAlign 返回值形态两处（F-J：⑧ 生产函数直接断言 + ⑮） + scoped_diff 14/14 + M-1 WARN 有牙齿 + M-2 attacker 精确选择 selftest + pin SQL 别名/列校验 + 种子注入断言（两侧 fail-closed）+ 攻击序列收尾等待（F-E） + attack 行缺 my 的 fail-closed（残差③） + 活栈隔离口径（owner 21:15：开关 off/on/fail-closed 六向 + attack_seed 公式含 i32 回绕 + 流水头 有/无/缺文件 + C# dump vs Rust 谓词表机判（生产绿 / 删一条红 / 加未登记红）+ C# hook 离线自检 + SHA256 活文件（F1：holder FileShare.ReadWrite 下读=锚 + 旧 ReadAllBytes 必被拒）+ attack_rng_ledger 站点表与决策片同表（F2）+ 目标可用性门两侧接线（owner 00:20：C#/Rust 同一函数，rust good/ghost/missing 三向 fail-closed） + C# 专用流纯度门（F-N1：干净绿 / 混入 base 单元·player 数不符·污染首条 三向必红）））'
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

Write-Output '--- 0.7) 活栈隔离口径（owner 2026-10-12 21:15；缺省关，行为不变） ---'
$isoEnv = Test-M3IsolateAttackValue -Value $env:MIR2_RNG_ISOLATE_ATTACK
if (-not $isoEnv.ok) {
  Write-Output ("RED: `$env:MIR2_RNG_ISOLATE_ATTACK='{0}' 非法（合法：空/0/false/off/no 或 1/true/on/yes）" -f $env:MIR2_RNG_ISOLATE_ATTACK)
  exit 1
}
$isolateOn = ([bool]$IsolateAttack) -or ([bool]$isoEnv.on)
$attackSeed = Get-M3AttackSeed -Seed $Seed
if ($isolateOn) {
  Write-Output ("ISOLATE_ATTACK_REQUESTED on=true attack_seed={0}（流水头应为 ISOLATE_ATTACK {0}）" -f $attackSeed)
} else {
  Write-Output 'ISOLATE_ATTACK_REQUESTED on=false（默认关；RNG 逐字节行为不变）'
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
               '-PinMapName', $PinMapName, '-PinCX', $AttackPinCX, '-PinCY', $AttackPinCY, '-AttackVictimHp', $AttackVictimHp,
               '-AttackVictimLevel', $AttackVictimLevel)
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
              '-AttackPinDir', $AttackPinDir, '-PinMapName', $PinMapName, '-PinCX', $AttackPinCX, '-PinCY', $AttackPinCY, '-AttackVictimHp', $AttackVictimHp, '-AttackVictimLevel', $AttackVictimLevel)
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

# --- 隔离记账（夹具侧）：会话产物记「隔离是否开启 + 专用流 seed + dump/专用流 sha256」---
# 缺省关时也写（isolate_attack=false），供跨侧护栏判「两侧同一隔离口径」；旧会话无此文件视为 off。
# C# 侧 `ISOLATE_ATTACK` 在 MIR2_RNG_LOG 流水（= -CsharpSeedLog）；Rust 侧在 Rust 侧的同名 rng 流水。
$rustRngLogResolved = if (-not [string]::IsNullOrEmpty($RustRngLog)) { $RustRngLog }
  elseif (-not [string]::IsNullOrEmpty($env:MIR2_RNG_LOG)) { $env:MIR2_RNG_LOG }
  elseif (Test-Path -LiteralPath (Join-Path $RustStackLogDir 'rng-stream.tsv')) { Join-Path $RustStackLogDir 'rng-stream.tsv' }
  else { '' }
$traceForIso = if ($Side -eq 'csharp') { $csharpSeedLogResolved } else { $rustRngLogResolved }
$traceIso = Get-M3TraceIsolate -LogPath $traceForIso
$sitesDumpPath = if (-not [string]::IsNullOrEmpty($IsolateSites)) { $IsolateSites }
  elseif (-not [string]::IsNullOrEmpty($env:MIR2_RNG_ISOLATE_SITES)) { $env:MIR2_RNG_ISOLATE_SITES }
  elseif ($traceIso.present -and -not [string]::IsNullOrEmpty($traceIso.sites_path)) { $traceIso.sites_path }
  else { '' }
$attackStreamPath = if (-not [string]::IsNullOrEmpty($AttackStreamLog)) { $AttackStreamLog }
  elseif (-not [string]::IsNullOrEmpty($env:MIR2_RNG_ATTACK_LOG)) { $env:MIR2_RNG_ATTACK_LOG }
  else { '' }
if ($isolateOn) {
  if (-not $traceIso.present) {
    Write-Output ("RED: 期望隔离口径但流水无 ISOLATE_ATTACK 行（{0}）——栈未开 MIR2_RNG_ISOLATE_ATTACK 或影子 hook 非新版" -f $traceForIso)
    exit 1
  }
  if ($traceIso.seed -ne $attackSeed) {
    Write-Output ("RED: 流水 ISOLATE_ATTACK seed={0} != 期望 {1}（MIR2_RNG_SEED 与 -Seed 不同口径？）" -f $traceIso.seed, $attackSeed)
    exit 1
  }
  if ($Side -eq 'csharp') {
    if ([string]::IsNullOrEmpty($sitesDumpPath)) {
      Write-Output 'RED: 隔离开启但缺 C# 分流站点 dump 路径（-IsolateSites / $env:MIR2_RNG_ISOLATE_SITES / 流水头 isolate_sites=）'
      exit 1
    }
    $pred = Test-M3IsolatePredicates -CsharpDump $sitesDumpPath
    if ($pred.rc -ne 0) {
      Write-Output 'RED: C# 分流站点 vs Rust 谓词表机判失败（缺一条 / 有未登记 site ⇒ fail-closed）'
      Write-Output $pred.output
      exit 1
    }
    Write-Output ("ISOLATE_PREDICATES_OK table-vs-csharp GREEN（dump={0}）" -f $sitesDumpPath)
  }
}
$sitesDumpSha = if (-not [string]::IsNullOrEmpty($sitesDumpPath) -and (Test-Path -LiteralPath $sitesDumpPath)) { Get-M3Sha256Hex -Path $sitesDumpPath } else { '' }
$atkStreamSha = if (-not [string]::IsNullOrEmpty($attackStreamPath) -and (Test-Path -LiteralPath $attackStreamPath)) { Get-M3Sha256Hex -Path $attackStreamPath } else { '' }
$tablePath = Get-M3IsolateSitesTablePath
$cursorPath = Get-M3IsolateSitesCursorPath
$isoMeta = [ordered]@{
  side = $Side; isolate_attack = $isolateOn; attack_seed = $(if ($isolateOn) { $attackSeed } else { $null })
  trace = $traceForIso; trace_isolate = $traceIso.present; trace_isolate_seed = $traceIso.seed
  sites_dump = $sitesDumpPath; sites_dump_sha256 = $sitesDumpSha
  sites_cursor = $cursorPath; sites_cursor_sha256 = $(if (Test-Path -LiteralPath $cursorPath) { Get-M3Sha256Hex -Path $cursorPath } else { '' })
  attack_stream = $attackStreamPath; attack_stream_sha256 = $atkStreamSha
  predicates_table = $tablePath; predicates_table_sha256 = $(if (Test-Path -LiteralPath $tablePath) { Get-M3Sha256Hex -Path $tablePath } else { '' })
  ts = (Get-Date).ToString('o')
}
$isoMeta | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $mine.FullName 'attack_isolation.json') -Encoding utf8
Write-Output ("ISOLATE_EVIDENCE_RECORDED {0} isolate={1} dump_sha256={2} attack_stream_sha256={3}" -f (Join-Path $mine.FullName 'attack_isolation.json'), $isolateOn, $sitesDumpSha, $atkStreamSha)

# F-N1（owner 2026-10-12 22:15 ① / m3cs 2026-10-13）：C# 专用流**纯度** fail-closed 门。
# 调用点级 divert 会把别图怪物/守卫（非玩家攻方）的取数吸进同一条 `Random(attack_seed)` ⇒ 被测量
# 攻方的流位置被整体推移（C# 不可复现、跨侧逐 k≠0）。hook 侧已按「被测量攻方线程」收窄；这里再在
# 下游把「污染流」当场判红、**拒比**（owner 23:50 ③「play 单元数 < 期望 ⇒ 直接拒跑」）。
# 仅限 C# 侧：Rust 的攻击路径由世界层按实体取数（m3cs 实测 base=0），无此污染面。
if ($isolateOn -and $Side -eq 'csharp') {
  $gateStream = $attackStreamPath
  if ([string]::IsNullOrEmpty($gateStream) -or -not (Test-Path -LiteralPath $gateStream)) {
    Write-Output 'RED: 隔离开启但 C# 专用流（-AttackStreamLog / $env:MIR2_RNG_ATTACK_LOG）缺失——专用流纯度门无法判；fail-closed'
    exit 1
  }
  & python $AttackStreamGate gate --stream $gateStream --expected $AttackCount --json (Join-Path $mine.FullName 'attack_stream_gate.json')
  if ($LASTEXITCODE -ne 0) {
    Write-Output ("RED: C# 专用流纯度门未过（非玩家攻方混入 / player 单元数 != {0}）——两相不可比，**拒比**（fail-closed，不放宽判据）" -f $AttackCount)
    exit 1
  }
  Write-Output ("ATTACK_STREAM_PURITY_OK side=csharp player_units=expected={0} base_units=0 stream={1}" -f $AttackCount, (Split-Path -Leaf $gateStream))
}

# 同序列判据：baseline.json 的 attack_markers 必须存在（run_pair 已机判完整串）。
$baseline = Get-Content (Join-Path $mine.FullName 'baseline.json') -Raw | ConvertFrom-Json
$markers = @($baseline.attack_markers)
if ($markers.Count -eq 0 -or -not (Test-Path (Join-Path $mine.FullName 'attack_raw.jsonl'))) {
  Write-Output ("RED: 同序列判据不成立（attack_markers={0} 条 / attack_raw.jsonl 缺失）——驱动没跑完整串" -f $markers.Count)
  exit 1
}
Write-Output ("ATTACKSEQ_OK markers={0} raw=True" -f $markers.Count)

# F-F（owner 2026-10-12 22:35）：「pin 后复核 HP == 期望值」fail-closed —— 受害者收到的 `SM_STRUCK` 的 `maxhp`
# 就是服务端 `WAbil.MaxHP`（oracle `BaseObject.Attack.cs:52`）。pin 若不生效（被 `RecalcLevelAbilitys` 按 L1
# 重算回 19），这里读到 19 != 期望 ⇒ RED（不再等到两侧 diff 才发现「没有目标窗口」）。
$postPin = Test-M3AttackVictimPostPinHp -RawPath (Join-Path $mine.FullName 'attack_raw.jsonl') -ExpectedHp $AttackVictimHp
if (-not $postPin.ok) {
  Write-Output ("RED: pin 后复核受害者 HP 失败——期望 maxhp={0}（Level={1} Job={2}）；{3}" -f $AttackVictimHp, $AttackVictimLevel, $script:M3AttackVictimJob, $postPin.reason)
  exit 1
}
Write-Output ("VICTIM_HP_POSTPIN ok expected={0} struck_lines={1}（Level={2} Job={3}，oracle RecalcLevelAbilitys 自洽）" -f $AttackVictimHp, @($postPin.observed).Count, $AttackVictimLevel, $script:M3AttackVictimJob)

# owner 2026-10-12 23:50 第 ③ 条 + 2026-10-13 00:20「两侧都强制」：**目标可用性 fail-closed 门**。
# 背景：受害者的**服务端** `Ghost` 被某条 oracle 路径静默置 1 时，`Envir.GetMovingObject(..., true)`
# 对 Ghost 直接返回 null ⇒ `GetPoseCreate()==null` ⇒ `_Attack`/`AttackSuccess` 提前返回（C# 只 3/120）。
# 客户端视图（attack_state.jsonl）看不到这个 ghost ⇒ 夹具必须用**服务端权威 dump** 判。两侧同口径：
#   C# 半：`start-csharp-hook.ps1 -StateLog`（缺省 `E:\tmp\m3-csharp-hook\rng-server-state.tsv`）；
#   Rust 半：`start_rust_stack.ps1 -StateLog`（m2-svr 的 MIR2_RNG_STATE_LOG 管线；`-RustStateLog` / 环境 / 默认）。
# 判据：dump 最后 -AttackCount 行里，攻方前方格 (Vx,Vy) 上必须有非攻方、!ghost && !death 的玩家族对象；
# 允许 1 击起跑竞态。**任一侧缺 dump ⇒ RED（fail-closed）**——旧行为「-Side rust 只打 NOTE 放行」已删除。
$atkId = 0
foreach ($l in (Get-Content (Join-Path $mine.FullName 'attack_raw.jsonl'))) {
  if ($l -match '"role"\s*:\s*"attack"' -and $l -match '"my"\s*:\s*(\d+)') { $atkId = [int]$Matches[1]; break }
}
if ($atkId -le 0) { Write-Output 'PREREQ_MISSING: 会话 attack_raw.jsonl 里读不到攻方 ActorId（role=attack.my）'; exit 2 }
$vx = $AttackPinCX + 1
$vy = $AttackPinCY
if ($baseline.attack_pose -match 'victim=(\d+),(\d+)') { $vx = [int]$Matches[1]; $vy = [int]$Matches[2] }
$stateExplicit = if ($Side -eq 'csharp') { $CsharpStateLog } else { $RustStateLog }
$stateFlag = if ($Side -eq 'csharp') { '-CsharpStateLog' } else { '-RustStateLog' }
$gate = Invoke-M3TargetAvailabilityGate -Side $Side -ExplicitStateLog $stateExplicit -Attacks $AttackCount -AttackerId $atkId -Vx $vx -Vy $vy -Tolerance 1
if ($gate.rc -ne 0) {
  if ($gate.reason -eq 'missing_state_log') {
    Write-Output ("RED: 缺服务端权威逐击状态 dump（{0} / 环境变量 MIR2_RNG_STATE_LOG）——目标可用性门无法判；C# 用 start-csharp-hook.ps1 -StateLog，Rust 用 start_rust_stack.ps1 -StateLog（fail-closed，owner 23:50 第 ③ 条 / 00:20 两侧都强制）" -f $stateFlag)
  } else {
    Write-Output ("RED: 目标可用性门未过（rc={0}, side={1}）——受害不在 ({2},{3}) 或被 ghost/death（见 server_state_gate.py）" -f $gate.rc, $Side, $vx, $vy)
  }
  exit 1
}
# 入会话留痕：本侧运行的最后 -AttackCount 行服务端 dump（dump 文件跨多次 run 累加，取尾段）。
@(Get-Content -LiteralPath $gate.path | Select-Object -Last $AttackCount) |
  Set-Content -Path (Join-Path $mine.FullName 'server_state_segment.tsv') -Encoding utf8
Write-Output ("TARGET_AVAILABILITY_OK side={0} state={1} attacker_id={2} victim_cell={3},{4}" -f $Side, (Split-Path -Leaf $gate.path), $atkId, $vx, $vy)

# pin 记账字段存在性护栏（owner 2026-10-12 16:30；**19:20 降级为 informational**）：本侧产物必须至少把
# `pin_attempts` 记账落盘（缺失/不可解析 ⇒ PREREQ exit 2）；attempts 数值只打 INFO（不再拒比）。
$localPin = Get-M3PinAttemptsGuard -PlayerDir $mine.FullName
foreach ($m in @($localPin.messages)) { Write-Output $m }
if (@($localPin.failed).Count -gt 0) {
  Write-Output ("PREREQ_MISSING: 本侧 pin 记账字段缺失/不可解析（{0}）——仪器接线断裂" -f (@($localPin.failed) -join ','))
  exit 2
}
foreach ($m in @($localPin.informational)) { Write-Output $m }
Write-Output ("PIN_ATTEMPTS_INFO local attempts={0}/expected={1} retried={2}" -f $localPin.info.attempts, $localPin.info.expected, $localPin.retried)

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
