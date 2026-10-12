#requires -Version 5
<#
.SYNOPSIS
  run_pair.ps1 — M3「同库同脚本对拍」夹具编排（D2 线）。

.DESCRIPTION
  一次"侧运行"= 同一份操作序列（假人驱动）打在同一套服务端上，产出可归档的基线：
    1. 预检（客户端入口端口在听 / DB 可读 / mysql 客户端在位）
    2. **全新前缀断言**：本次运行的账号/角色命名空间必须为空——
       这保证"同一初始状态"不必靠破坏性还原（活库里既有角色不受影响）
    2.5 预开号（tools/client/account_provision.ps1，account+account_protection 两表，
        否则 LoginSrv 的 INNER JOIN 取不到资料 —— 设计文档 §8.1）
    3. 快照 BEFORE（tools/dbsnap，逐表 sha256）
    4. 跑操作序列（tools/botload，账号=<Prefix><序号>，-NewAccount 先注册）
    5. 快照 AFTER
    6. 抽取本次命名空间内的行（scoped_diff extract）+ 改动清单（delta）
    7. 落 baseline.json：两侧快照 hash、作用域行数、改动清单、操作序列命令、git 提交、时间线

  操作序列判据：以假人日志标记为准（`帐号登录成功` 与 `成功进入游戏` 各 = N），
  **不看 runner 退出码**——BotSrv 进世界后会崩在 `TMap.CanMove`（工具侧已知阻塞，
  见 m3/README.md），崩溃发生在"进入世界之后"，不影响本段操作与 DB 落库。

  两侧对拍（M3 判据）：
    - 先跑 `-Side csharp` 记基线；Rust 侧同一命令 `-Side rust`（先把 7000/7100/7200 指向 Rust 实现）
    - 然后 `python scoped_diff.py diff <csharp>/scoped_after_norm.jsonl <rust>/scoped_after_norm.jsonl`
      **判据：作用域内字段级 diff = 0**（差异要么 0，要么进 tests/parity/whitelist.md）

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_pair.ps1 -Side csharp -Bots 5
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_pair.ps1 -SkipBots     # 只验夹具机制（不产生新行）
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_pair.ps1 -SelfTestRed # 红检：入口不通必须判失败
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [int]$Bots = 5,
  [string]$Prefix = '',
  [string]$Address = '127.0.0.1',
  [int]$Port = 7000,
  [int]$TimeoutSec = 600,
  [switch]$SkipBots,
  [switch]$KeepSnapshots,
  [switch]$ProvisionSql,
  [switch]$FixedNewChr,            # M3 固定建角场景（卡 openmir2-m3-botsrv-fixed-newchr）
  [int]$NewChrJob = 0,             # 仅 -FixedNewChr 时生效
  [int]$NewChrHair = 2,            # 仅 -FixedNewChr 时生效（2 = 原 Random(1) 恒走的发型）
  [int]$NewChrSex = 0,             # 仅 -FixedNewChr 时生效
  [switch]$BagEquipSequence,       # M3 T-14 夹具：BotSrv 进世界后按固定顺序发背包/装备动作（白名单 T-22）
  [int]$BagEquipStepMs = 1000,     # 仅 -BagEquipSequence 时生效
  [int]$BagEquipStartDelayMs = 3000, # 仅 -BagEquipSequence 时生效
  [switch]$AttackSequence,         # M3②③ 夹具：BotSrv 固定攻击序列（白名单 T-22 扩展）
  [int]$AttackCount = 120,         # 仅 -AttackSequence 时生效：攻击步数
  [int]$AttackStepMs = 500,        # 仅 -AttackSequence 时生效：每步间隔
  [int]$AttackStartDelayMs = 3000, # 仅 -AttackSequence 时生效：进世界到第一步的额外等待
  [switch]$EmitOpsArgs,            # 只打印 run_bots 命令组成（含 -DisableAutoPlay 耦合）后退出；无副作用，供无栈自检
  [switch]$PinStartPose,           # T-14 固定起点变体（owner 86478c55/d7798370）：预热建角 → SQL pin → BEFORE 快照 → 计分跑（选角；不带 -FixedNewChr/-NewAccount）
  [string]$PinMapName = '',   # 攻击变体的 pin 地图（R-1 要求整张零刷怪；空 ⇒ -AttackPinPair 取 $script:M3AttackPinMap，否则 '0'）
  [int]$PinHomeMap = 0,
  [int]$PinHomeX = 289,
  [int]$PinHomeY = 618,
  [int]$PinCX = -1,                # -AttackPinPair 时 <0 ⇒ $script:M3AttackPinCX（0137）；否则 289
  [int]$PinCY = -1,                # -AttackPinPair 时 <0 ⇒ $script:M3AttackPinCY；否则 618
  [int]$PinLevel = 1,
  [int]$PinDir = 0,
  [int]$PinSettleSeconds = 3,      # 仅 -PinStartPose：每轮 pin 后的 settle 窗口（异步掉线存档落库，见 gate_guards.ps1 ⑥）
  [int]$PinMaxAttempts = 5,        # 仅 -PinStartPose：settle+hold 最多轮数（仍 hold 不住 ⇒ throw，fail-closed）
  [switch]$AttackPinPair,          # 攻击变体：两 bot **相邻**固定起点（attacker/victim；配合 -AttackSequence）
  [int]$AttackPinDir = 2,          # 仅 -AttackPinPair：attacker 朝向（默认 2=RIGHT，victim 在右邻格）
  [int]$AttackPinDx = 1,           # 仅 -AttackPinPair：victim 相对 attacker 的 X 偏移（默认 +1）
  [int]$AttackVictimHp = 0,   # 仅 -AttackPinPair：victim（与 attacker）Hp/MaxHP；F-F：0 ⇒ 用 gate_guards 的 oracle 等级派生值（L255/战士=20669）；否则必须与 -AttackVictimLevel 自洽
  [int]$AttackVictimLevel = 0,   # 仅 -AttackPinPair：victim/attacker 钉的等级（byte 0..255）；0 ⇒ gate_guards 的 $script:M3AttackVictimLevel
  [switch]$SelfTestRed
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..')).Path
$Dbsnap = Join-Path $RepoRoot 'tools\dbsnap\dbsnap.py'
$Botload = Join-Path $RepoRoot 'tools\botload\run_bots.ps1'
$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
$Scoped = Join-Path $Here 'scoped_diff.py'
# 背包/装备序列口径（T-14 复跑：两侧同一组开关）的唯一形态在 gate_guards.ps1（与 run_m3_gate/selftest 共用）。
. (Join-Path $Here 'gate_guards.ps1')

# R-1（owner 2026-10-12 14:30）：攻击对拍场景必须整张图无怪 ⇒ `-AttackPinPair` 的 pin 地图缺省取
# `gate_guards` 的唯一来源常量 `$script:M3AttackPinMap`（0137，MonGen 为空）；背包/装备（非攻击）路径仍缺省 map 0。
if ([string]::IsNullOrEmpty($PinMapName)) { $PinMapName = if ($AttackPinPair) { $script:M3AttackPinMap } else { '0' } }
# 攻击变体的落点缺省也跟随场景图（仅 -AttackPinPair 生效；背包/装备路径保持 289/618 逐字节不变）。
if ($PinCX -lt 0) { $PinCX = if ($AttackPinPair) { $script:M3AttackPinCX } else { 289 } }
if ($PinCY -lt 0) { $PinCY = if ($AttackPinPair) { $script:M3AttackPinCY } else { 618 } }

# F-D（m3cs tick21）+ F-F（owner 2026-10-12 22:35）：攻击 pin 的 `Level/Job/Hp` 必须由 oracle `RecalcLevelAbilitys`
# 自洽（否则进世界被重算回 L1 的 19，受害者早死）。`-AttackVictimHp`/`-AttackVictimLevel` 缺省 0 =
# 用 gate_guards 的唯一口径（L255/战士 ⇒ 20669）；显式传值时必须自洽、且在列域内，否则 fail-closed。
if ($AttackVictimHp -le 0) { $AttackVictimHp = $script:M3AttackVictimHp }
if ($AttackVictimLevel -le 0) { $AttackVictimLevel = $script:M3AttackVictimLevel }
if ($AttackPinPair) {
  if (-not (Test-M3AttackVictimHpRange -VictimHp $AttackVictimHp)) {
    Write-Output ("RED: -AttackVictimHp={0} 越界（characters_ablity.Hp/MaxHP = smallint 有符号，1..32767）——见 F-D" -f $AttackVictimHp)
    exit 1
  }
  $hpConsistency = Test-M3AttackVictimHpSelfConsistent -Level $AttackVictimLevel -Job $script:M3AttackVictimJob -Hp $AttackVictimHp
  if (-not $hpConsistency.ok) {
    Write-Output ("RED: 攻击 pin 的 Level/Hp 与 oracle `RecalcLevelAbilitys` 不自洽（{0}）——进世界会被重算 ⇒ 见 F-F" -f (@($hpConsistency.issues) -join '; '))
    exit 1
  }
}

function Get-M3BotOpPlan {
  param([switch]$ForPrewarm)  # 固定起点变体的①预热相：强制 -FixedNewChr + -NewAccount
  # run_bots 命令组成的**唯一口径**：真实运行与 -EmitOpsArgs 自检共用（避免自检与实际漂移）。
  # 固定起点变体的**计分跑**走「选角」路径（角色已由预热相建出）⇒ 不带 -FixedNewChr/-NewAccount。
  $newAccount = if ($PinStartPose -and -not $ForPrewarm) { $false } else { $true }
  $fixed = if ($ForPrewarm) { $true } elseif ($PinStartPose) { $false } else { [bool]$FixedNewChr }
  $planArgs = @('-Count', $Bots, '-Prefix', $Prefix, '-Address', $Address, '-Port', $Port, '-TimeoutSec', $TimeoutSec)
  # runner 串保持与改动前逐字节一致的形态（-Prefix 后紧跟 -NewAccount 再 -Port），避免基线记录漂移。
  $runner = "run_bots.ps1 -Count $Bots -Prefix $Prefix"
  if ($newAccount) {
    $planArgs += '-NewAccount'
    $runner += ' -NewAccount'
  }
  $runner += " -Port $Port"
  if ($ForPrewarm) {
    # 预热相只需建号：等「创建角色」标记再收工（否则 run_bots 在 login_ok 就到、char 未建）。
    $planArgs += '-RequireCharCreated'
    $runner += ' -RequireCharCreated'
    # 根因修（owner 2026-10-12 19:20）：预热假人若进世界，被 Stop-Process 后 GameSvr
    # `SaveHumanRcd`（WorldServer.cs:711-713 → :1077-1091 异步）会把预热末态写回，覆盖随后的 pin
    # ⇒「首轮只 pin 1/2」的真根因。令 BotSrv 建角后**不进世界**（SelectChrScene 短路 SendSelChr）
    # ⇒ GameSvr 无该 PlayObject ⇒ 断线不触发存档。默认只影响预热相。
    $planArgs += '-StopAfterCharCreated'
    $runner += ' -StopAfterCharCreated'
  }
  if ($fixed) {
    $runner += " -FixedNewChr -NewChrJob $NewChrJob -NewChrHair $NewChrHair -NewChrSex $NewChrSex"
    $planArgs += @('-FixedNewChr', '-NewChrJob', $NewChrJob, '-NewChrHair', $NewChrHair, '-NewChrSex', $NewChrSex)
  }
  if ($BagEquipSequence -and -not $ForPrewarm) {
    # T-14 复跑口径（owner 2026-10-12 06:36）：背包/装备序列**必须**一并关掉客户端挂机。
    # 挂机 = 无种子 RNG 的随机走位（ClientManager.RunAutoPlay）⇒ 两侧各走各的，
    # characters.MapName/CX/CY 逐字节比**必然**出 diff（且不是实现差异）。两侧同开关 ⇒ 序列才可比。
    $runner += " -BagEquipSequence -BagEquipStepMs $BagEquipStepMs -BagEquipStartDelayMs $BagEquipStartDelayMs -DisableAutoPlay"
    $planArgs += @('-BagEquipSequence', '-BagEquipStepMs', $BagEquipStepMs, '-BagEquipStartDelayMs', $BagEquipStartDelayMs, '-DisableAutoPlay')
  }
  if ($AttackSequence -and -not $ForPrewarm) {
    # M3②③ 夹具（卡 openmir2-m3-attack-pair-fixture）：固定攻击序列**必须**一并关挂机
    # （同 T-14 理由：挂机 = 无种子 RNG 随机走位，会改目标/坐标 ⇒ 两侧不可比）。两侧同开关。
    $runner += " -AttackSequence -AttackCount $AttackCount -AttackStepMs $AttackStepMs -AttackStartDelayMs $AttackStartDelayMs -DisableAutoPlay"
    $planArgs += @('-AttackSequence', '-AttackCount', $AttackCount, '-AttackStepMs', $AttackStepMs, '-AttackStartDelayMs', $AttackStartDelayMs, '-DisableAutoPlay')
  }
  return [pscustomobject]@{ runner = $runner; botArgs = $planArgs }
}

function Get-M3StartPoseLabel {
  if (-not $PinStartPose) { return 'none' }
  if ($AttackPinPair) { return (Get-M3AttackPoseLabel) }
  return (Format-M3StartPose -MapName $PinMapName -HomeMap $PinHomeMap -HomeX $PinHomeX -HomeY $PinHomeY `
    -CX $PinCX -CY $PinCY -Level $PinLevel -Dir $PinDir)
}

function Get-M3AttackPoseLabel {
  if (-not $AttackPinPair) { return 'none' }
  return (Format-M3AttackPose -MapName $PinMapName -AX $PinCX -AY $PinCY -Dir $AttackPinDir `
    -VX ($PinCX + $AttackPinDx) -VY $PinCY -Level $AttackVictimLevel -VictimHp $AttackVictimHp)
}

if ($SelfTestRed) {
  $Port = 17998
  $Bots = 2
  Write-Output 'SELFTEST_RED: 入口指向 127.0.0.1:17998（无监听），预期夹具判失败'
}

if ([string]::IsNullOrEmpty($Prefix)) {
  # 账号名上限 10 字符（协议字段）：前缀 + 序号位数必须 <= 10
  $Prefix = 'm3' + (Get-Date -Format 'HHmmss')
}
if ($PinStartPose -and -not $FixedNewChr) {
  throw '-PinStartPose 必须与 -FixedNewChr 同用（预热建角用固定输入；计分跑仍标为固定建角场景以便 --fixed-newchr 比 Job/Hair/Sex）'
}
if ($AttackPinPair -and -not $PinStartPose) {
  throw '-AttackPinPair 必须与 -PinStartPose 同用（攻击变体仍需预热建角 + SQL pin 两相）'
}
# R-1（owner 2026-10-12 14:30）：攻击对拍场景必须**整张图无怪**（除攻击路径外零 RNG 消费者）——
# 按 pin 地图查冻结提取件 + provenance 判「该图刷怪行数 == 0」；**不是无怪图就 fail-closed 拒绝跑**
# （旧口径「离刷怪圈 99.2 格」不算「全图无怪」）。真机启动即判，不等跑出随机红；
# 无栈自检 ⑯ 另做「map 0 必红 + 合成 0/1 行·声明漂移·缺 provenance 必红」四向。
if ($AttackPinPair) {
  $pinMapChk = Test-M3MonGenEmpty -MapName $PinMapName -VerifySource
  if (-not $pinMapChk.ok) {
    Write-Output ("RED: -AttackPinPair 地图 '{0}' 不是整张无怪图（MonGen 刷怪行数 {1}，要求 0）——见 R-1 场景隔离；{2}；请改到 MonGen 为空的地图（缺省 {3}）" -f `
      $PinMapName, $pinMapChk.spawns, $pinMapChk.reason, $script:M3AttackPinMap)
    throw ('R-1: attack pin map not monsterless: {0} spawns={1}' -f $PinMapName, $pinMapChk.spawns)
  }
  Write-Output ("MAP_HAS_NO_MONSTER ok map={0} spawns=0 fixture={1} source_verified={2}" -f $PinMapName, $pinMapChk.fixture, $pinMapChk.source_verified)
  # R-1 refine（owner 2026-10-12 15:05）："严格说不能只看 MonGen"——守卫/NPC/地图事件/任务触发器也可能跑 AI、
  # 消耗全局 RNG ⇒ 除「MonGen 零行」外，还要求 pin 图在 GuardList/MerChant/Npcs/MapEvent/MapQuest 里**零引用**。
  # 同测试件（`fixtures/ai-consumer-map<map>.tsv` + `ai-consumer-provenance.tsv`），非零 ⇒ **fail-closed 拒绝跑**。
  $pinAiChk = Test-M3MapNoAiConsumers -MapName $PinMapName -VerifySource
  if (-not $pinAiChk.ok) {
    Write-Output ("RED: -AttackPinPair 地图 '{0}' 仍有非 MonGen 的 AI 消费者引用（守卫/NPC/地图事件/任务；refs={1}，要求 0）——见 R-1 场景隔离；{2}；请换到无 AI 消费者的图（缺省 {3}）" -f `
      $PinMapName, $pinAiChk.total, $pinAiChk.reason, $script:M3AttackPinMap)
    throw ('R-1: attack pin map has non-mongen AI consumers: {0} refs={1}' -f $PinMapName, $pinAiChk.total)
  }
  Write-Output ("MAP_HAS_NO_AI_CONSUMERS ok map={0} total_refs=0 fixture={1} source_verified={2}" -f $PinMapName, (Split-Path -Leaf $pinAiChk.fixture), $pinAiChk.source_verified)
}
if ($AttackSequence -and $Bots -lt 2) {
  Write-Output ('WARN: -AttackSequence 建议 -Bots 2（attacker+victim）；当前 Bots={0} ⇒ 无目标，struck 恒 0，判据必 RED' -f $Bots)
}
# M-1（reviewer-5 76e433f0，清登记 Minor）：本 WARN 原写在 `if ($EmitOpsArgs)` 块内**且**带
# `-not $EmitOpsArgs` ⇒ 条件恒 false（死代码，任何路径都不触发）。移出该块并去掉自否条件，
# 使「-AttackSequence 未配 -AttackPinPair」在**真实跑与自检**两条路径都可见。
if ($AttackSequence -and -not $AttackPinPair) {
  Write-Output 'WARN: -AttackSequence 未配 -AttackPinPair ⇒ attacker/victim 可能重叠/无目标（真机请配 -PinStartPose -AttackPinPair）'
}

# -EmitOpsArgs：只把 run_bots 的命令组成打印成 JSON 供无栈自检核（-DisableAutoPlay 是否随
# -BagEquipSequence 一并传下去；固定起点变体的预热/计分两相），**不建会话目录/不预检/不取快照**。
if ($EmitOpsArgs) {
  $plan = Get-M3BotOpPlan
  $obj = [ordered]@{
    ops_runner        = $plan.runner
    bot_args          = $plan.botArgs
    bag_equip_scope   = Format-M3BagEquipScope -BagEquipSequence ([bool]$BagEquipSequence) -BagEquipStepMs $BagEquipStepMs -BagEquipStartDelayMs $BagEquipStartDelayMs -DisableAutoPlay ([bool]$BagEquipSequence)
    attack_sequence   = Format-M3AttackScope -AttackSequence ([bool]$AttackSequence) -AttackCount $AttackCount -AttackStepMs $AttackStepMs -AttackStartDelayMs $AttackStartDelayMs -DisableAutoPlay ([bool]$AttackSequence)
    start_pose        = (Get-M3StartPoseLabel)
    attack_pose       = (Get-M3AttackPoseLabel)
  }
  if ($PinStartPose) {
    $pre = Get-M3BotOpPlan -ForPrewarm
    $obj['prewarm_runner'] = $pre.runner
    $obj['prewarm_bot_args'] = $pre.botArgs
    $obj['pin_sql'] = Format-M3StartPoseSql -ChrName "${Prefix}0" -MapName $PinMapName -HomeMap $PinHomeMap -HomeX $PinHomeX -HomeY $PinHomeY `
      -CX $PinCX -CY $PinCY -Level $PinLevel -Dir $PinDir
    $obj['pin_sql_all'] = @(Get-M3StartPosePinSql -ChrName "${Prefix}0")
    # pin 仪器形态（owner 2026-10-12 08:20）：无栈自检据此机判生产接线是否被削弱（settle_seconds>=1、consecutive>=2）。
    $obj['pin_instrument'] = (Format-M3PinInstrument -SettleSeconds $PinSettleSeconds -MaxAttempts $PinMaxAttempts -Consecutive 2)
    # pin 重试账本的期望值（owner 2026-10-12 16:30）：自检断言它与仪器 consecutive 同源（均为 2）。
    $obj['pin_attempts_expected'] = (Get-M3PinExpectedAttempts)
    if ($AttackPinPair) {
      $attackSql = New-Object System.Collections.Generic.List[string]
      foreach ($st in (Get-M3AttackPinSql -ChrName "${Prefix}0" -CX $PinCX -CY $PinCY -Dir $AttackPinDir -MapName $PinMapName -HomeMap $PinHomeMap -HomeX $PinHomeX -HomeY $PinHomeY -Level $AttackVictimLevel -Job $script:M3AttackVictimJob -Hp $AttackVictimHp)) { $attackSql.Add($st) }
      foreach ($st in (Get-M3AttackPinSql -ChrName "${Prefix}1" -CX ($PinCX + $AttackPinDx) -CY $PinCY -Dir 6 -MapName $PinMapName -HomeMap $PinHomeMap -HomeX $PinHomeX -HomeY $PinHomeY -Level $AttackVictimLevel -Job $script:M3AttackVictimJob -Hp $AttackVictimHp)) { $attackSql.Add($st) }
      $obj['attack_pin_sql_all'] = $attackSql.ToArray()
    }
  }
  [pscustomobject]$obj | ConvertTo-Json -Depth 4
  exit 0
}

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$SessionDir = Join-Path $RepoRoot ("mir2-rs\tests\parity\m3\sessions\{0}-{1}" -f $Side, $stamp)
New-Item -ItemType Directory -Force -Path $SessionDir | Out-Null
Write-Output ("SESSION " + $SessionDir)
Write-Output ("SIDE={0} PREFIX={1} BOTS={2} ENTRY={3}:{4}" -f $Side, $Prefix, $Bots, $Address, $Port)

function Invoke-Mysql([string]$Sql) {
  $out = & $MysqlExe -h 127.0.0.1 -P 3306 -u root '--password=' --batch --skip-column-names -e $Sql
  if ($LASTEXITCODE -ne 0) { throw "mysql 执行失败: $Sql" }
  return $out
}

function Git-Rev {
  Push-Location $RepoRoot
  try { return (git rev-parse HEAD).Trim() } finally { Pop-Location }
}

# ---- 1. 预检 ----
if (-not (Test-Path $MysqlExe)) { throw "找不到 mysql 客户端: $MysqlExe" }
if ($SkipBots) {
  Write-Output 'PREFLIGHT 跳过入口探测（-SkipBots）'
} else {
  $listening = [bool](Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)
  if (-not $listening) {
    if ($SelfTestRed) {
      Write-Output ("SELFTEST_RED GREEN: 入口 {0}:{1} 无监听 => 夹具判失败成立" -f $Address, $Port)
      exit 0
    }
    Write-Output ("RED: 入口 {0}:{1} 没有监听——夹具无法产生操作序列" -f $Address, $Port)
    exit 1
  }
  Write-Output ("PREFLIGHT 入口 {0}:{1} 在听" -f $Address, $Port)
}
$dbOk = Invoke-Mysql 'SELECT 1'
if ($dbOk -notmatch '1') { throw 'DB 不可读' }

# ---- 1.8 运行态记录（放进基线：基线必须写清是哪几个二进制产出的） ----
# 实测（A2 勘察 + D2 复核）：运行的 LoginSrv/DBSrv 来自**仓库构建输出**
# `src/*/bin/Release/`，而 LoginGate/SelGate/GameGate/GameSvr 来自 `E:\MirServer\*`。
# 计划"整栈重启"时必须以这里的实际 exe 为准；LoginSrv 读的配置是
# `src/LoginSrv/bin/Release/logsrv.conf`（LogSrv.ini/config.conf 是遗留副本，不读）。
$runtimeProcs = @()
foreach ($n in @('mysqld', 'DBSrv', 'LoginSrv', 'GameSrv', 'LoginGate', 'SelGate', 'GameGate')) {
  $ps = Get-Process -Name $n -ErrorAction SilentlyContinue
  foreach ($proc in $ps) {
    $runtimeProcs += [pscustomobject]@{
      name = $proc.ProcessName
      path = $proc.Path
      start = $proc.StartTime.ToString('o')
    }
  }
}
$runtimeProcs | ForEach-Object { Write-Output ("RUNTIME {0} <- {1}" -f $_.name, $_.path) }

# ---- 1.9 账号名长度护栏（实测：超 10 字符被协议层截断，登录会报"帐号不存在"） ----
$maxAcct = 10
$longest = $Prefix.Length + ([string]($Bots - 1)).Length
if ($longest -gt $maxAcct) {
  Write-Output ("RED: 账号名最长 {0} 字符 > 上限 {1}（前缀 {2} + 序号）——协议层会静默截断，" -f $longest, $maxAcct, $Prefix)
  Write-Output '     表现为"帐号创建成功"但登录"此帐号不存在"。缩短 -Prefix。'
  exit 1
}
Write-Output ("ACCT_LEN_OK 最长账号名 {0} 字符（上限 {1}）" -f $longest, $maxAcct)

# ---- 2. 全新前缀断言 ----
$accHits = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_account.account WHERE Account LIKE '{0}%'" -f $Prefix)
$chrHits = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters WHERE ChrName LIKE '{0}%'" -f $Prefix)
Write-Output ("FRESH_CHECK account={0} character={1}" -f $accHits, $chrHits)
if ([int]$accHits -ne 0 -or [int]$chrHits -ne 0) {
  Write-Output ("RED: 前缀 {0} 已存在同名账号/角色——换前缀，或先清理（见 M3 对拍夹具.md 的清理 SQL）" -f $Prefix)
  exit 1
}

# ---- 2.5 固定起点变体（T-14，owner 86478c55/d7798370）：预热建角 → SQL pin → 再取 BEFORE 快照 ----
#   注：本机的 ORACLE C# 侧是冻结部署件（不能注入种子）⇒ 只关挂机仍不够；这一相把角色的
#   MapName/HomeMap/HomeX/Y/CX/CY 钉成固定值，使两侧走「记录里有图有坐标」的路径。
# pin 重试账本（owner 2026-10-12 16:30）：把本轮 pin 的 attempts/settle_rounds 落成结构化字段
# `baseline.json.pin_attempts`（判据层据此 fail-closed 拒绝「重试次数不同」的两次运行，见
# `gate_guards.ps1` ⑥b）。未走 pin 的运行保持 `(off)`，逐字节等价旧路径。
$pinAttempts = 0
$pinSettleRounds = 0
$pinAttemptsExpected = Get-M3PinExpectedAttempts
$pinHoldLines = @()
# 预热相「不进世界」门的结果（owner 2026-10-12 19:20 根因修）：写入 baseline 供跨侧/事后核查。
$prewarmNoWorldEntry = $null
if ($PinStartPose) {
  Write-Output '--- 2.5) 固定起点：预热建角 + SQL pin ---'
  if ($SkipBots) { throw '-PinStartPose 不能与 -SkipBots 同用' }
  $prePlan = Get-M3BotOpPlan -ForPrewarm
  Write-Output ('PREWARM ' + $prePlan.runner)
  # BotSrv 日志显式落会话目录（run_bots -LogFile）⇒ 可直接机判「预热相有没有进世界」。
  $prewarmBotLog = Join-Path $SessionDir 'prewarm-bot.log'
  & powershell -ExecutionPolicy Bypass -File $Botload @($prePlan.botArgs) -LogFile $prewarmBotLog 2>&1 |
    Tee-Object -FilePath (Join-Path $SessionDir 'prewarm.log')
  if ($LASTEXITCODE -ne 0) { throw '预热建角失败（run_bots -RequireCharCreated 非 0）' }
  $chrCnt = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters WHERE ChrName LIKE '{0}%'" -f $Prefix)
  if ([int]$chrCnt -ne $Bots) { throw ("预热后 characters 行数={0}，期望 {1}——预热建角没成功，pin 无目标" -f $chrCnt, $Bots) }
  # ⑥c 根因门（owner 2026-10-12 19:20）：预热相假人不得进世界。
  #   进世界 ⇒ Stop-Process 会被 GameSvr 当成真实下线 ⇒ `WorldServer.SaveHumanRcd` 异步落库
  #   （src/GameSrv/Word/WorldServer.cs:711-713 → :1077-1091）覆盖随后的 pin（真机原件：
  #   `bots-20261012-150104.log` 里 选择角色/进入游戏/成功进入游戏）⇒「首轮只 pin 1/2」的真根因。
  #   BotSrv `-StopAfterCharCreated` 在建角后短路 SendSelChr ⇒ 永不进世界。
  $prewarmNoWorld = Test-M3PrewarmNoWorldEntry -LogPath $prewarmBotLog
  if (-not $prewarmNoWorld.ok) {
    if ($prewarmNoWorld.reason -eq 'prewarm_entered_world') {
      throw ("预热相违规：BotSrv 日志 {0} 出现进世界标记 [{1}]（{2}）——GameSvr 会在断线时异步 SaveHumanRcd 覆盖 pin；请确认 -StopAfterCharCreated 接线（owner 19:20 根因修）" -f $prewarmBotLog, (@($prewarmNoWorld.markers) -join ','), $prewarmNoWorld.reason)
    }
    if ($prewarmNoWorld.reason -eq 'prewarm_char_create_failed') {
      throw ("预热相建角失败：BotSrv 日志 {0} 只出现建角失败串 [{1}]、无成功正证——预热未拿到角色（reviewer-5 N-1）；请检查账号/角色名冲突（账号或角色名已被占用）" -f $prewarmBotLog, (@($prewarmNoWorld.failures) -join ','))
    }
    # reviewer-6 M-1：日志缺失/不可读/零 marker（无建角正证）都 fail-closed——否则「预热未跑到建角」会被误当合格。
    throw ("预热相无合格证据：BotSrv 日志 {0} reason={1} positive=[{2}]——预热必须带「创建角色 <name>」/「预热建角完成」正证且无进世界标记（reviewer-6 M-1；reviewer-5 N-1）" -f $prewarmBotLog, $prewarmNoWorld.reason, (@($prewarmNoWorld.positive) -join ','))
  }
  $prewarmNoWorldEntry = $true
  Write-Output ("PREWARM_NO_WORLD_ENTRY ok log={0} markers=0" -f $prewarmBotLog)
  # 预热相的掉线存档是**异步**的（BotSrv 被杀 ⇒ GameSvr `SaveHumanRcd` → FrontEngine 异步落库）：
  # 实测（2026-10-12 T-14 复跑）pin 后立刻进图，C# 侧拿到的是**预热末态**（`CX/CY=285,622`、
  # 物品 `MakeIndex=36685073…`）而不是 pin 的 `289,618`/`90000001…`——预热存档晚于 pin 才落库、把 pin
  # 覆盖了（与 §5 AFTER 快照前的 settle 同因，注释见下）。
  # 注（owner 2026-10-12 19:20）：上面 ⑥c 已用 `-StopAfterCharCreated` 从**上游根除**这个竞态
  # （预热假人不再进世界 ⇒ 无断线存档）；下面这套 settle+hold 仍保留作**防御**（仍能咬住任何残留写者）。
  # 判据统一到 `gate_guards.ps1::Invoke-M3PinSettleHold`（唯一口径，owner 2026-10-12 08:20）：
  #   ① 每轮 pin 后必须**真实 settle**（`Start-Sleep` 后返回 $true，计入 settle 轮数）；
  #   ② **连续两轮** settle 后的复验都到位才算 hold（单次观测挡不住「存档随后覆盖」的假 GREEN）。
  #   ⇒ 跳过 settle 或存档反复覆盖 ⇒ 必判红（throw，fail-closed）。可判红自检见
  #   `run_bag_equip_pair.ps1 -SelfCheckNoStack` ⑩（故障注入驱动同一函数）。
  $pinCtx = [pscustomobject]@{
    Prefix = $Prefix; Bots = $Bots; SettleSeconds = $PinSettleSeconds
    MapName = $PinMapName; HomeMap = $PinHomeMap; HomeX = $PinHomeX; HomeY = $PinHomeY
    CX = $PinCX; CY = $PinCY; Level = $(if ($AttackPinPair) { $AttackVictimLevel } else { $PinLevel }); Dir = $PinDir
  }
  $pinResult = Invoke-M3PinSettleHold -Bots $Bots -MaxAttempts $PinMaxAttempts -Context $pinCtx -BagPerBot $(if ($AttackPinPair) { 0 } else { 4 }) `
    -Pin {
      param($ctx, $attempt)
      if ($AttackPinPair) {
        # 攻击变体：attacker=bot0 @ (CX,CY) 面向 AttackPinDir；victim=bot1 @ (CX+Dx,CY) 面向左(6)。
        $specs = @(
          @{ i = 0; x = [int]$ctx.CX; y = [int]$ctx.CY; dir = [int]$AttackPinDir },
          @{ i = 1; x = [int]$ctx.CX + [int]$AttackPinDx; y = [int]$ctx.CY; dir = 6 }
        )
        foreach ($spec in $specs) {
          foreach ($st in (Get-M3AttackPinSql -ChrName "$($ctx.Prefix)$($spec.i)" -CX $spec.x -CY $spec.y -Dir $spec.dir -MapName $ctx.MapName -HomeMap $ctx.HomeMap -HomeX $ctx.HomeX -HomeY $ctx.HomeY -Level $ctx.Level -Hp $AttackVictimHp)) {
            Invoke-Mysql $st | Out-Null
            Write-Output ('PIN_CHAR ' + $st)
          }
        }
      } else {
        for ($i = 0; $i -lt $ctx.Bots; $i++) {
          $pinChar = Format-M3StartPoseSql -ChrName "$($ctx.Prefix)$i" -MapName $ctx.MapName -HomeMap $ctx.HomeMap -HomeX $ctx.HomeX -HomeY $ctx.HomeY `
            -CX $ctx.CX -CY $ctx.CY -Level $ctx.Level -Dir $ctx.Dir
          Invoke-Mysql $pinChar | Out-Null
          # 连「初始背包 + Abil」一起 provision（owner 643459a7）：否则五步无对象可操作。
          foreach ($st in (Get-M3StartPosePinSql -ChrName "$($ctx.Prefix)$i")) { Invoke-Mysql $st | Out-Null }
          Write-Output ('PIN_CHAR ' + $pinChar)
        }
      }
    } `
    -Settle { param($ctx, $attempt) Start-Sleep -Seconds ([int]$ctx.SettleSeconds); return $true } `
    -Observe {
      param($ctx, $attempt)
      if ($AttackPinPair) {
        # 攻击变体：两个 bot 各自在自己格子（attacker 面向 AttackPinDir；victim 面向 6）+ Abil Hp 抬高。
        $pinnedA = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters WHERE ChrName='{0}0' AND MapName='{1}' AND CX={2} AND CY={3} AND Level={4} AND Dir={5}" -f $ctx.Prefix, $ctx.MapName, $ctx.CX, $ctx.CY, $ctx.Level, $AttackPinDir)
        $pinnedV = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters WHERE ChrName='{0}1' AND MapName='{1}' AND CX={2} AND CY={3} AND Level={4} AND Dir=6" -f $ctx.Prefix, $ctx.MapName, ([int]$ctx.CX + [int]$AttackPinDx), $ctx.CY, $ctx.Level)
        $pinned = [int]$pinnedA + [int]$pinnedV
        $abilPinned = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id WHERE c.ChrName LIKE '{0}%' AND a.Hp={1} AND a.MaxHP={1}" -f $ctx.Prefix, $AttackVictimHp)
        return [pscustomobject]@{ Pinned = $pinned; BagPinned = 0; AbilPinned = [int]$abilPinned }
      }
      # pin 后复验：图/坐标/Level + 4 件初始物品 + Abil Level（三项都到位才算本轮 match）。
      $pinned = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters WHERE ChrName LIKE '{0}%' AND MapName='{1}' AND CX={2} AND CY={3} AND Level={4}" -f $ctx.Prefix, $ctx.MapName, $ctx.CX, $ctx.CY, $ctx.Level)
      $bagPinned = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters_bagitem b JOIN mir2_db.characters c ON b.PlayerId=c.Id WHERE c.ChrName LIKE '{0}%' AND b.StdIndex<>0" -f $ctx.Prefix)
      $abilPinned = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id WHERE c.ChrName LIKE '{0}%' AND a.Level={1}" -f $ctx.Prefix, $M3PinAbilLevel)
      return [pscustomobject]@{ Pinned = [int]$pinned; BagPinned = [int]$bagPinned; AbilPinned = [int]$abilPinned }
    }
  $pinAttempts = [int]$pinResult.Attempts
  $pinSettleRounds = [int]$pinResult.SettleRounds
  $pinHoldLines = @()
  foreach ($v in $pinResult.Verdicts) {
    $line = if ($v.hold) { "PIN_HOLD attempt={0}（连续两轮 settle 复验均 hold 住）" -f $v.attempt }
            else { "PIN_RETRY attempt={0} {1}（{2}）" -f $v.attempt, $v.detail, (@($v.failed) -join ',') }
    $pinHoldLines += $line
    Write-Output $line
  }
  # 原始 pin 记账落会话目录（自包含，便于事后复算 attempts；与 baseline.json 的结构化字段互证）。
  Set-Content -Path (Join-Path $SessionDir 'pin_hold.txt') -Value $pinHoldLines -Encoding utf8
  Write-Output ('PIN_ATTEMPTS ' + (Format-M3PinAttempts -Attempts $pinAttempts -Expected $pinAttemptsExpected -SettleRounds $pinSettleRounds -MaxAttempts $PinMaxAttempts))
  if (-not $pinResult.Hold) {
    $lastV = @($pinResult.Verdicts)[-1]
    throw ("pin 未 hold 住（跳过 settle 或预热掉线存档反复覆盖）：{0}" -f $lastV.detail)
  }
  Write-Output ('PIN_START_POSE ' + (Get-M3StartPoseLabel))
  if ($AttackPinPair) { Write-Output ('ATTACK_POSE ' + (Get-M3AttackPoseLabel)) }
}

# ---- 3. 快照 BEFORE ----
$snapBefore = Join-Path $SessionDir 'snap_before'
python $Dbsnap snap --out $snapBefore | Tee-Object -FilePath (Join-Path $SessionDir 'dbsnap_before.log')
if ($LASTEXITCODE -ne 0) { throw 'snap_before 失败' }

# ---- 3.5 账号准备 ----
# 默认走**服务器自建号**（假人 -NewAccount 注册，LoginSrv 的账号表在线更新，
# account_protection 行由服务端一并写入 —— 实测可登录）。
# `-ProvisionSql` 改为 SQL 直开号，但**必须重启 LoginSrv 后才有用**：
# `AccountStorage.Index()` 查的是启动时加载的内存账号表（实测：SQL 新开的号登录报"此帐号不存在"）。
$provLog = Join-Path $SessionDir 'provision.log'
$Provision = Join-Path $RepoRoot 'tools/client/account_provision.ps1'
if ($SkipBots) {
  'SKIPPED (-SkipBots)' | Set-Content -Path $provLog -Encoding utf8
} elseif ($ProvisionSql) {
  if (-not (Test-Path $Provision)) { throw "找不到开号脚本: $Provision" }
  for ($i = 0; $i -lt $Bots; $i++) {
    $acct = "$Prefix$i"
    & powershell -ExecutionPolicy Bypass -File $Provision -Account $acct 2>&1 |
      Tee-Object -FilePath $provLog -Append
    if ($LASTEXITCODE -ne 0) { throw "开号失败: $acct" }
  }
  Write-Output 'PROVISION_SQL 已 SQL 开号 —— 注意：需重启 LoginSrv（内存账号表）后才可登录'
} else {
  'SERVER-SIDE (-NewAccount during ops)' | Set-Content -Path $provLog -Encoding utf8
}

# ---- 4. 操作序列 ----
$opsLog = Join-Path $SessionDir 'ops.log'
$opsRunner = 'none'
$opsExit = 0
$bagSeqMarkers = @()
$attackMarkers = @()
$attackRawPath = Join-Path $SessionDir 'attack_raw.jsonl'
if (-not $SkipBots) {
  $plan = Get-M3BotOpPlan
  $opsRunner = $plan.runner
  $botArgs = $plan.botArgs
  if ($FixedNewChr) {
    Write-Output ("OPS_FIXED_NEWCHR Job={0} Hair={1} Sex={2}" -f $NewChrJob, $NewChrHair, $NewChrSex)
  }
  if ($BagEquipSequence) {
    Write-Output ("OPS_BAGEQUIPSEQ step_ms={0} start_delay_ms={1} disable_autoplay=True" -f $BagEquipStepMs, $BagEquipStartDelayMs)
  }
  if ($AttackSequence) {
    Write-Output ("OPS_ATTACKSEQ count={0} step_ms={1} start_delay_ms={2} disable_autoplay=True" -f $AttackCount, $AttackStepMs, $AttackStartDelayMs)
  }
  Write-Output ("OPS " + $opsRunner)
  & powershell -ExecutionPolicy Bypass -File $Botload @botArgs 2>&1 |
    Tee-Object -FilePath $opsLog
  $opsExit = $LASTEXITCODE
  Write-Output ("OPS_EXIT " + $opsExit)
  if ($SelfTestRed) {
    if ($opsExit -ne 0) { Write-Output 'SELFTEST_RED GREEN: 入口不通 => 夹具判失败成立'; exit 0 }
    Write-Output 'SELFTEST_RED RED: 入口不通却报成功——判据失效'; exit 1
  }
  # 判据取日志标记而不是 runner 退出码：BotSrv 进入世界后崩溃在 TMap.CanMove
  #（工具侧已知问题，见 m3/README.md「已知阻塞」），但目前"登录+建角+进世界"这段是可靠的。
  $botLog = (Get-ChildItem (Join-Path $RepoRoot 'tests/botload') -Filter 'bots-*.log' |
             Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
  Copy-Item $botLog (Join-Path $SessionDir 'bot.log') -Force
  $botText = Get-Content $botLog -Raw
  $acct = ([regex]::Matches($botText, '您的帐号创建成功|帐号登录成功')).Count
  $login = ([regex]::Matches($botText, '帐号登录成功')).Count
  $charc = ([regex]::Matches($botText, '创建角色')).Count
  $enter = ([regex]::Matches($botText, '成功进入游戏')).Count
  Write-Output ("OPS_MARKERS login_ok={0}/{1} char_created={2}/{1} enter_world={3}/{1} bot_log={4}" -f $login, $Bots, $charc, $enter, (Split-Path $botLog -Leaf))
  # M3 T-14（T-22）：固定背包/装备序列的执行证据（每一步一条 `M3BAGSEQ` 标记，落 bot.log）。
  # 判据：开了 -BagEquipSequence 就必须看到整串（含 `done`）——否则驱动没跑完，
  # 两侧“同一序列”不成立，绝不能当 GREEN（否则拿一个空序列去 diff 会恒绿）。
  $bagSeqMarkers = @([regex]::Matches($botText, 'M3BAGSEQ [^\r\n]*') | ForEach-Object { $_.Value })
  Write-Output ("OPS_BAGSEQ_MARKERS count={0}" -f $bagSeqMarkers.Count)
  foreach ($m in $bagSeqMarkers) { Write-Output ("BAGSEQ " + $m) }
  if ($BagEquipSequence) {
    $doneCount = ([regex]::Matches($botText, 'M3BAGSEQ done')).Count
    if ($bagSeqMarkers.Count -eq 0) {
      Write-Output 'RED: -BagEquipSequence 开了但 bot.log 里一条 M3BAGSEQ 标记都没有 —— 固定序列没跑（驱动未生效/假人未进世界）'
      exit 1
    }
    if ($doneCount -lt $Bots) {
      Write-Output ("RED: -BagEquipSequence 开了但只有 {0}/{1} 个假人跑完整串（缺 M3BAGSEQ done）——序列不完整，不能当同一序列" -f $doneCount, $Bots)
      exit 1
    }
  }
  # M3②③（T-22 扩展）：固定攻击序列的执行证据（每步一条 `M3ATTSEQ {json}` 标记，落 bot.log）。
  # 判据：开了 -AttackSequence 就必须看到 `role=attack` 从 k=1 到 k=AttackCount（每侧至少一个假人跑完整串）。
  $attackMarkers = @([regex]::Matches($botText, 'M3ATTSEQ [^\r\n]*') | ForEach-Object { $_.Value })
  Write-Output ("OPS_ATTACKSEQ_MARKERS count={0}" -f $attackMarkers.Count)
  if ($AttackSequence) {
    $jsonObjs = @([regex]::Matches($botText, 'M3ATTSEQ (\{[^{}]*\})') | ForEach-Object { $_.Groups[1].Value })
    if ($jsonObjs.Count -eq 0) {
      Write-Output 'RED: -AttackSequence 开了但 bot.log 里一条 M3ATTSEQ 标记都没有 —— 固定攻击序列没跑（驱动未生效/假人未进世界）'
      exit 1
    }
    # 落自包含原始事件流（每行一个 JSON 对象），供 attack_diff.py align/diff。
    Set-Content -Path $attackRawPath -Value $jsonObjs -Encoding utf8
    $attackSteps = @($jsonObjs | Where-Object { $_ -match '"role"\s*:\s*"attack"' }).Count
    $doneCount = @($jsonObjs | Where-Object { $_ -match '"role"\s*:\s*"done"' }).Count
    Write-Output ("OPS_ATTACKSEQ attacks={0} done={1} raw={2}" -f $attackSteps, $doneCount, (Split-Path $attackRawPath -Leaf))
    if ($doneCount -lt 1) {
      Write-Output 'RED: -AttackSequence 开了但没有 `M3ATTSEQ done` —— 序列未收尾（驱动中途断了）'
      exit 1
    }
    # 每个假人应发 AttackCount 步；至少一个假人跑满（多假人时只要求 max == AttackCount）。
    $maxAttack = 0
    foreach ($g in ($jsonObjs | Where-Object { $_ -match '"role"\s*:\s*"done"' })) {
      if ($g -match '"attacks"\s*:\s*(\d+)') { $maxAttack = [Math]::Max($maxAttack, [int]$Matches[1]) }
    }
    if ($maxAttack -lt $AttackCount) {
      Write-Output ("RED: -AttackSequence 只跑了 {0} 步 < -AttackCount {1} —— 序列不完整" -f $maxAttack, $AttackCount)
      exit 1
    }
  }
  # 判据取"可靠段"：账号创建 + 登录成功 = N（实测稳定）。
  # 进世界之后的段位今日不可靠：BotSrv 会在 `TMap.CanMove` 崩溃并带走整个进程
  #（工具侧已知阻塞，见 m3/README.md「已知阻塞」），崩溃点与时序相关（有时停在"确认游戏公告"）。
  # 因此 enter_world 只记录、不判红；M3 要用"打怪/掉落/小退"操作序列前必须先修该崩溃。
  if ($login -lt $Bots) {
    Write-Output ("RED: 假人登录成功 {0}/{1} 未达标，基线不成立" -f $login, $Bots)
    exit 1
  }
  if ($enter -lt $Bots) {
    Write-Output ("WARN: 进世界 {0}/{1} —— BotSrv 崩溃（工具侧已知阻塞），本次基线只覆盖到「登录+建角」" -f $enter, $Bots)
  }
} else {
  ('SKIPPED (-SkipBots)') | Set-Content -Path $opsLog -Encoding utf8
}

# ---- 5. 快照 AFTER ----
# M3 T-14：run_bots 收工时 kill BotSrv ⇒ 服务端从掉线路径走 `SaveHumanRcd`（WorldServer 的非活跃分支），
# 把本段 in-world 改动（装备/卸下/用药）落库。给保存线程一点时间再取 AFTER 快照，否则会漏掉刚发生的改动。
# 仅 -BagEquipSequence 时等待；其余路径逐字节不变（不加额外等待）。
if ($BagEquipSequence -or $AttackSequence) {
  Write-Output 'SETTLE_SEQ 等掉线存档落库（3s）'
  Start-Sleep -Seconds 3
}
$snapAfter = Join-Path $SessionDir 'snap_after'
python $Dbsnap snap --out $snapAfter | Tee-Object -FilePath (Join-Path $SessionDir 'dbsnap_after.log')
if ($LASTEXITCODE -ne 0) { throw 'snap_after 失败' }

# ---- 6. 作用域抽取 + 改动清单 ----
$scopedBefore = Join-Path $SessionDir 'scoped_before.jsonl'
$scopedAfter = Join-Path $SessionDir 'scoped_after.jsonl'
$scopedAfterNorm = Join-Path $SessionDir 'scoped_after_norm.jsonl'
$deltaJson = Join-Path $SessionDir 'delta.json'

python $Scoped extract $snapBefore $Prefix $scopedBefore --allow-empty | Tee-Object -FilePath (Join-Path $SessionDir 'extract_before.log')
if ($LASTEXITCODE -ne 0) { throw 'extract_before 失败' }
python $Scoped extract $snapAfter $Prefix $scopedAfter | Tee-Object -FilePath (Join-Path $SessionDir 'extract_after.log')
if ($LASTEXITCODE -ne 0) { throw 'extract_after 失败' }
python $Scoped extract $snapAfter $Prefix $scopedAfterNorm --normalize | Out-Null
python $Scoped delta $scopedBefore $scopedAfter --out $deltaJson | Tee-Object -FilePath (Join-Path $SessionDir 'delta.log')

# ---- 7. 基线清单 ----
function Snap-Hashes([string]$Dir) {
  $man = Get-Content (Join-Path $Dir 'manifest.json') -Raw | ConvertFrom-Json
  $t = @{}
  foreach ($e in $man.tables) { $t[$e.table] = [pscustomobject]@{ rows = $e.rows; sha256 = $e.sha256 } }
  return $t
}
$beforeHashes = Snap-Hashes $snapBefore
$afterHashes = Snap-Hashes $snapAfter
$delta = Get-Content $deltaJson -Raw | ConvertFrom-Json

$baseline = [pscustomobject]@{
  session      = (Split-Path $SessionDir -Leaf)
  side         = $Side
  prefix       = $Prefix
  bots         = $Bots
  entry        = "$Address`:$Port"
  ops_runner   = $opsRunner
  ops_exit     = $opsExit
  fixed_new_chr = if ($FixedNewChr) { "Job=$NewChrJob Hair=$NewChrHair Sex=$NewChrSex" } else { "(off)" }
  start_pose = (Get-M3StartPoseLabel)
  bag_equip_sequence = (Format-M3BagEquipScope -BagEquipSequence ([bool]$BagEquipSequence) -BagEquipStepMs $BagEquipStepMs -BagEquipStartDelayMs $BagEquipStartDelayMs -DisableAutoPlay ([bool]$BagEquipSequence))
  bag_equip_markers = $bagSeqMarkers
  attack_sequence = (Format-M3AttackScope -AttackSequence ([bool]$AttackSequence) -AttackCount $AttackCount -AttackStepMs $AttackStepMs -AttackStartDelayMs $AttackStartDelayMs -DisableAutoPlay ([bool]$AttackSequence))
  attack_pose = (Get-M3AttackPoseLabel)
  pin_attempts = $(if ($pinAttempts -ge 1) { Format-M3PinAttempts -Attempts $pinAttempts -Expected $pinAttemptsExpected -SettleRounds $pinSettleRounds -MaxAttempts $PinMaxAttempts } else { '(off)' })
  prewarm_no_world_entry = $prewarmNoWorldEntry
  attack_markers = $attackMarkers
  attack_raw = $(if (Test-Path $attackRawPath) { (Split-Path $attackRawPath -Leaf) } else { '' })
  git_rev      = (Git-Rev)
  dbsnap       = $beforeHashes
  dbsnap_after = $afterHashes
  delta        = $delta
  scoped_rows  = @{
    before = (Get-Content $scopedBefore | Measure-Object -Line).Lines
    after  = (Get-Content $scopedAfter | Measure-Object -Line).Lines
  }
  runtime_procs = $runtimeProcs
  finished_at  = (Get-Date).ToString('o')
}
$baselinePath = Join-Path $SessionDir 'baseline.json'
$baseline | ConvertTo-Json -Depth 8 | Set-Content -Path $baselinePath -Encoding utf8

# 快照 TSV 体积大（活库全表，单次 ~36MB）且可随时重生：默认只留 manifest（sha256 判据本体）。
# 需要原始 TSV 做人工核查时加 -KeepSnapshots。
if (-not $KeepSnapshots) {
  foreach ($d in @($snapBefore, $snapAfter)) {
    Copy-Item (Join-Path $d 'manifest.json') (Join-Path (Split-Path $d -Parent) ((Split-Path $d -Leaf) + '.manifest.json')) -Force
    Remove-Item -Recurse -Force $d -ErrorAction SilentlyContinue
  }
  Write-Output 'SNAPSHOT_TRIMMED 已把 snap_before/snap_after 瘦身为 *.manifest.json（-KeepSnapshots 可保留原始 TSV）'
}

Write-Output ('BASELINE_OK ' + $baselinePath)
Write-Output ('SNAPSHOT_HASHES_BEFORE tables={0}; AFTER tables={1}' -f $beforeHashes.Count, $afterHashes.Count)
exit 0
