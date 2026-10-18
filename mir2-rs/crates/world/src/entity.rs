//! 实体存储 —— 对应 C# 的 `SystemShare.ActorMgr`（`ActorId` 稳定 id）+ `BaseObject` 的 AOI 相关字段。
//!
//! 存储形态：连续数组 + 空闲槽回收 + **代际号**（generation），防止"已删除 id 被复用"后
//! 旧引用误命中（C# 侧靠 `ActorMgr.Get(id) == null` 判断，但 id 会复用；这里把复用风险显式化，
//! 对外仍以 C# 风格的 `i32` id 暴露，代际只在内部校验）。
//!
//! 遍历顺序 = 槽位顺序（= 插入顺序 + 回收槽复用），**全库唯一**：
//! tick 的确定性（判据①）依赖"同一组操作产生同一槽位排布"。

/// `src/OpenMir2/Enums/Race.cs`：`ActorRace.Play = 0`、`ActorRace.NPC = 10`、`ActorRace.Monster = 80`。
pub const ACTOR_RACE_PLAY: u8 = 0;
/// **玩家 `SearchViewRange` 节拍**（`PlayObject.cs:900` `SearchTime = 1000`）。
///
/// `BaseObject` ctor 先置 `SearchTime = Random(2000)+2000`（`BaseObject.cs:441`），
/// `PlayObject` ctor 紧接着**常量覆盖**为 1000（`:900`）⇒ 玩家每 `>1000ms` 才扫一次
/// （`WorldServer.cs:663` 的严格大于门）。怪物的 `SearchTime` 由各自 ctor 覆盖
/// （`MonsterObject.cs:65` 起），本常量只用于玩家 ctor 等价物。
pub const PLAYER_SEARCH_TIME: i64 = 1000;
/// 非玩家实体（怪/商人/NPC）`AnimalState` 的 ctor 缺省 `SearchTime` 占位值。
///
/// 真实值由各自构造链落定（`MonsterObject.cs:65` 的 `3000+Random(2000)`、生成路径的
/// [`MonsterCtorDraws::search_time`]）；本常量只用于让 `Entity::new`（玩家 ctor 等价物）
/// **不**把玩家的 1000 常量带到非玩家实体上，保持本片之前非玩家字段逐一不变。
pub const BASE_ANIMAL_SEARCH_TIME: i64 = 3000;
pub const ACTOR_RACE_NPC: u8 = 10;
pub const ACTOR_RACE_MONSTER: u8 = 80;
/// `ActorRace.Animal = 50`（`Race.cs:24`）—— `BaseObject.RecalcAbilitys` 末尾
/// `if (Race >= ActorRace.Animal) ApplySlaveLevelAbilitys();`（`BaseObject.Abil.cs:85-88`）的门。
pub const ACTOR_RACE_ANIMAL: u8 = 50;
/// `ActorRace.WhiteSkeleton = 100`（`Race.cs:61`）—— 变异骷髅，`RecalcAbilitys` 覆写改速度。
pub const ACTOR_RACE_WHITE_SKELETON: u8 = 100;
/// `ActorRace.ElfMonster = 113`（`Race.cs:62`）—— 神兽跟随形态，`RecalcAbilitys` 覆写改速度。
pub const ACTOR_RACE_ELF_MONSTER: u8 = 113;
/// `ActorRace.ElfWarriormon = 114`（`Race.cs:63`）—— 神兽攻击形态，`RecalcAbilitys` 覆写改速度。
pub const ACTOR_RACE_ELF_WARRIORMON: u8 = 114;

/// `src/OpenMir2/Consts/PoisonState.cs`：`StatusTimeArr` 的下标常量子集（B3 用到的五个）。
///
/// `BaseObject.StatusTimeArr` 是 `ushort[15]`（`BaseObject.cs:81`，ctor `:394` 置 0）。
/// oracle 里逐个计时器的递减循环整段被注释（`BaseObject.Base.cs:249-315`）⇒ **不要**实现「每秒减 1」。
pub const POISON_DECHEALTH: usize = 0;
pub const POISON_DAMAGEARMOR: usize = 1;
pub const POISON_STONE: usize = 5;
/// `PoisonState.STATETRANSPARENT = 8`（隐身术/集体隐身术的门，`MagicManager.cs:675/693/1219`）。
pub const POISON_STATETRANSPARENT: usize = 8;
/// `PoisonState.OPENHEATH = 0x2`（`MakeOpenHealth` 置入 `CharStatusEx`，`BaseObject.cs:947`）。
pub const POISON_OPENHEATH: i32 = 0x2;

/// `CharacterObject.UseItems` 的佩戴位数量（`CharacterObject.cs:89`：`UseItems = new UserItem[13]`）。
pub const USE_ITEM_SLOTS: usize = 13;

/// `UserItem` 的战斗子集（`src/OpenMir2/Packets/ClientPackets/ClientUserItem.cs:14`）。
///
/// 本骨架用到 `PlayObject.StruckDamage`（`PlayObject.cs:2954-3036`）的耐久面：
/// 每格 `Index > 0`（= 已装备）各抽一次 `Random(8)`，衣服位先做特判；`Dura` 只做减法。
///
/// **G-06 装载面（`openmir2-persist-useitems-wiring`）**：`DuraMax`/`Desc[14]` 也随槽承载
/// （oracle 的 `UserItem` 是同一个对象，磨损/破零只改 `Index`/`Dura`）。装载源 = 会话
/// `PlayObject.UseItems`，写回目标同源；存档（`MakeSaveRcd`）读 `ToServerItem()` 的耐久字段。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UseItemSlot {
    /// `UserItem.Index`（0 = 空位）
    pub index: u16,
    /// `UserItem.Dura`
    pub dura: u16,
    /// `UserItem.DuraMax`（破零时 oracle **不**清零该字段）
    pub dura_max: u16,
    /// `UserItem.Desc[14]`（破零时 oracle 保留原值；存档 `ToServerItem()` 原样写回）
    pub desc: [u8; 14],
}

/// `src/Modules/SystemModule/Enums/VisibleFlag.cs`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VisibleFlag {
    /// 隐藏（本轮未再看见）
    Hidden = 0,
    /// 不可见（已存在、本轮又看见）
    Invisible = 1,
    /// 可见（本轮新增）
    Show = 2,
}

/// `src/Modules/SystemModule/Data/VisibleBaseObject.cs`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleBaseObject {
    pub target: i32,
    pub flag: VisibleFlag,
}

/// oracle `BaseObject` + `AnimalObject` 两层 ctor 的**唯一**抽数契约（kind + 顺序）。
///
/// 怪物（`MonsterObject: AnimalObject`）、固定 NPC（`NormNpc: AnimalObject`）、商人列表
/// （`Merchant: NormNpc: AnimalObject`）三条构造路径**都经**这条公共链，按同一次序抽同样的 4 个数：
///
/// | # | oracle | 抽法 |
/// | --- | --- | --- |
/// | 0 | `BaseObject.RunTick = GetTickCount() + Random(1500)` | `BaseObject.cs:439` |
/// | 1 | `BaseObject.SearchTime = Random(2000) + 2000` | `BaseObject.cs:441` |
/// | 2 | `AnimalObject.AttackTick = GetTickCount() - Random(3000)` | `AnimalObject.cs:76` |
/// | 3 | `AnimalObject.WalkTick = GetTickCount() - Random(3000)` | `AnimalObject.cs:77` |
///
/// **次序**：C# 基类 ctor 先跑 ⇒ `BaseObject`（1500→2000）先于 `AnimalObject`（3000→3000）。
/// `NormNpc`/`Merchant` 的 ctor 体本身不再抽（`NormNpc.cs:39`、`Merchant.cs:82` 只有字段赋值）；
/// `MonsterObject` 在此之上**另**抽第 5 个 `SearchTime = 3000 + Random(2000)`（`MonsterObject.cs:65`），
/// 属怪物专属、不在这条公共链里（见 [`MonsterCtorDraws::search_time`]）。
///
/// **一处定义、三处消费（防「少抽 2 次」复发）**：本表与 [`BaseAnimalCtorDraws::draw`] 是唯一抽数点；
/// 固定 NPC（[`crate::npc`] 的 `insert_npc`）、商人列表（[`crate::merchant`] 的
/// `merchant_ctor_run_delay`/`insert_merchant`）直接消费；怪物路径（宿主
/// `bins/game-svr/src/mongen.rs::draw_monster_construct`）的公共段按同一表由
/// `bins/game-svr/src/npcs.rs` 的跨路径机判核对。改本表即三处同时改。
pub const BASE_ANIMAL_CTOR_DRAW_MODS: [i32; 4] = [1500, 2000, 3000, 3000];

/// [`BASE_ANIMAL_CTOR_DRAW_MODS`] 定义的一条 `BaseObject`+`AnimalObject` ctor 抽出结果。
///
/// 值语义：`*_delta` 是 oracle `GetTickCount() ± Random(...)` 里的 `Random(...)` 项（绝对值依赖
/// 读取时刻，由消费方用自己的时钟合成）；`base_search_time` 是 `Random(2000)+2000` 的**完整值**
/// （含 `+2000` 偏移），与 [`MonsterCtorDraws::base_search_time`] 同口径。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BaseAnimalCtorDraws {
    /// #0 `BaseObject.RunTick` 的 `Random(1500)`（`BaseObject.cs:439`）。
    pub run_tick_delta: i32,
    /// #1 `BaseObject.SearchTime = Random(2000) + 2000`（`BaseObject.cs:441`）。
    pub base_search_time: i32,
    /// #2 `AnimalObject.AttackTick` 的 `Random(3000)`（`AnimalObject.cs:76`）。
    pub attack_tick_delta: i32,
    /// #3 `AnimalObject.WalkTick` 的 `Random(3000)`（`AnimalObject.cs:77`）。
    pub walk_tick_delta: i32,
}

impl BaseAnimalCtorDraws {
    /// 按 [`BASE_ANIMAL_CTOR_DRAW_MODS`] 的次序从 `rng` 抽这 4 个数（**唯一**抽数点）。
    ///
    /// #1 的 `+ 2000` 偏移与模数同值（oracle 字面 `Random(2000) + 2000`），故复用同一表项。
    #[must_use]
    pub fn draw(rng: &mut mir2_shared::rng::RandomNumber) -> Self {
        Self {
            run_tick_delta: rng.random_below(BASE_ANIMAL_CTOR_DRAW_MODS[0]),
            base_search_time: rng.random_below(BASE_ANIMAL_CTOR_DRAW_MODS[1])
                + BASE_ANIMAL_CTOR_DRAW_MODS[1],
            attack_tick_delta: rng.random_below(BASE_ANIMAL_CTOR_DRAW_MODS[2]),
            walk_tick_delta: rng.random_below(BASE_ANIMAL_CTOR_DRAW_MODS[3]),
        }
    }
}

/// oracle 实体**构造函数链**里抽出的随机数结果（生成路径，卡
/// `openmir2-m3-mongen-load-spawn` 抽取；卡 `openmir2-m3-monster-ai-tick` 的 AI 只消费不抽）。
///
/// oracle `CreateMonster`（`WorldServer.MonGen.cs:498-810`）在 `cert.Dir`（`:740`）之前先
/// `new Xxx()`，而构造链 `BaseObject` → `AnimalObject` → `MonsterObject`/`AtMonster`/… →
/// 派生类会**连续抽随机数**（每层 ctor 一个 `Random(...)`），这些取数发生在
/// `MonGen.rs` 的 `CreateMonster` 回调里、是生成期 RNG 流的一部分；抽出的值必须随生成结果
/// 落到实体上，否则取数完成后无法复算（流位置已前移）。
///
/// 三个 `*_tick_delta` 是 oracle `HUtil32.GetTickCount() ± Random(...)` 里的 `Random(...)` 项
/// （绝对值依赖读取时刻，故只存随机分量，绝对值由消费方用自己的时钟合成）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MonsterCtorDraws {
    /// `BaseObject.RunTick = GetTickCount() + Random(1500)` 的 `Random(1500)`（`BaseObject.cs:439`）。
    pub run_tick_delta: i32,
    /// `BaseObject.SearchTime = Random(2000) + 2000`（`BaseObject.cs:441`）——被派生类覆盖前的中间值。
    pub base_search_time: i32,
    /// `AnimalObject.AttackTick = GetTickCount() - Random(3000)` 的 `Random(3000)`（`AnimalObject.cs:76`）。
    pub attack_tick_delta: i32,
    /// `AnimalObject.WalkTick = GetTickCount() - Random(3000)` 的 `Random(3000)`（`AnimalObject.cs:77`）。
    pub walk_tick_delta: i32,
    /// 构造函数链**最终生效**的 `SearchTime`（最派生类 ctor 的最后一次赋值）。
    pub search_time: i32,
    /// `AnimalObject.MeatQuality`（race 51/52/53 的 `new X {...}` 初始化式；其余 race = 0）。
    pub meat_quality: u16,
    /// `MonsterObject.BoFearFire`（race 95/96/97 ctor 后的 `Random(n)==0`；race 101 初始化式恒 `true`）。
    pub bo_fear_fire: bool,
    /// `ZilKinZombi.ZilKillCount`（race 96 ctor 的 `Random(3)==0 ⇒ Random(3)+1`；未命中 = 0）。
    pub zil_kill_count: i32,
}

/// 怪物/动物 AI 状态 —— oracle `AnimalObject` / `MonsterObject` 的计时器与目标格字段子集。
///
/// 逐字段 oracle 锚点：
/// - `Dir`（`BaseObject.cs:40`，`WalkTo` `:601`/`TurnTo` `:1582` 写它；`CreateMonster` 里
///   `cert.Dir = RandomByte(8)` `WorldServer.MonGen.cs:740`）；
/// - `TargetX/TargetY`（`AnimalObject.cs:15-16`，ctor `TargetX = -1`；`MonsterObject.cs:296-301`
///   无目标时 `TargetX = -1`）；
/// - `TargetCret`（`BaseObject` 的当前攻击目标）、`TargetFocusTick`（`BaseObject.Base.cs:113`）；
/// - `AttackTick`/`WalkTick`（`AnimalObject.cs:77-78`：ctor `= now - Random(3000)`）/
///   `WalkCount`/`WalkStep`/`WalkWait`/`WalkWaitTick`/`WalkWaitLocked`（`AnimalObject.cs:29-38`）；
/// - `WalkSpeed`（`BaseObject.cs:446` = 1400）/`NextHitTime`（`:447` = 2000）；
/// - `SearchTick`/`SearchTime`（`BaseObject.cs:441-442` `Random(2000)+2000`，`MonsterObject.cs:56-57`
///   覆盖为 `3000 + Random(2000)`）/`SearchEnemyTick`（`AnimalObject.cs:23`）；
/// - `ThinkTick`（`MonsterObject.cs:22`）/`DupMode`（`:26`）/`HolySeize`（`AnimalObject.cs:47`）；
/// - `CoolEye`（`BaseObject`，`CreateMonster` `:745-747` 按 `Random(100) < CoolEyeCode` 置位）。
///
/// **本片（C 第 ① 片）不建模**：`Mission`/`MissionX/MissionY`（任务怪路径）、`RunAwayMode` 的
/// 触发点（`RunAway`/`BeeQueen` 等专用怪）；默认值下这些分支恒不命中
/// （与「普通 `MonsterObject`、无 master、无任务」的 oracle 执行路径一致，见 `monai` 模块头）。
/// `Master` 宝宝跟随 / `SlaveRelax` 已由卡 `openmir2-run-gap-monster-master-follow` 接入
/// （见 [`crate::monai::monster_run`] 与 `Entity::slave_relax`）。
#[derive(Clone, Debug)]
pub struct AnimalState {
    /// `BaseObject.Dir`。
    pub dir: u8,
    /// `BaseObject.CoolEye`（`SearchTarget` 的 `!HideMode || CoolEye` 门）。
    pub cool_eye: bool,
    /// `AnimalObject.BoFearFire`（`WalkTo(..., fearFire)`）；专用怪 ctor 置位，普通怪 false。
    pub bo_fear_fire: bool,
    /// `AnimalObject.NoAttackMode`（非攻击模式）。
    pub no_attack_mode: bool,
    /// `MonsterObject` 的当前目标（C# `BaseObject.TargetCret`）。
    pub target_creature: Option<i32>,
    /// `BaseObject.TargetFocusTick`（`BaseObject.Base.cs:113` 的 30s 过期门）。
    pub target_focus_tick: i64,
    /// `AnimalObject.TargetX` / `TargetY`（C# 默认 `TargetX = -1`、`TargetY = 0`）。
    pub target_x: i32,
    pub target_y: i32,
    /// `AnimalObject.AttackTick`。
    pub attack_tick: i64,
    /// `BaseObject.NextHitTime`（= 2000）。
    pub next_hit_time: i64,
    /// `AnimalObject.WalkTick`。
    pub walk_tick: i64,
    /// `AnimalObject.WalkCount` / `WalkStep`。
    pub walk_count: i32,
    pub walk_step: i32,
    /// `AnimalObject.WalkWait` / `WalkWaitTick` / `WalkWaitLocked`。
    pub walk_wait: i64,
    pub walk_wait_tick: i64,
    pub walk_wait_locked: bool,
    /// `BaseObject.WalkSpeed`（= 1400）。
    pub walk_speed: i64,
    /// `BaseObject.SearchTick` / `SearchTime` / `AnimalObject.SearchEnemyTick`。
    pub search_tick: i64,
    pub search_time: i64,
    pub search_enemy_tick: i64,
    /// `MonsterObject.ThinkTick` / `DupMode`。
    pub think_tick: i64,
    pub dup_mode: bool,
    /// `AnimalObject.HolySeize` 系列（困魔咒）。
    pub holy_seize: bool,
    pub holy_seize_tick: i64,
    pub holy_seize_interval: i64,
    /// `MonsterObject.CrazyMode` 系列（`MonsterObject.cs:49-57`：`CrazyMode`/`CrazyModeTick`/
    /// `CrazyModeInterval`）。由 [`crate::monai::open_crazy_mode`] 写入（生产路径 = 20 诱惑之光的
    /// `OpenCrazyMode(Random(20)+10)`），由 [`crate::monai::animal_run`] 尾部的
    /// `CrazyMode` 到期段清除（`MonsterObject.cs:353-356`，严格 `>`）。
    pub crazy_mode: bool,
    pub crazy_mode_tick: i64,
    pub crazy_mode_interval: i64,
    /// `AnimalObject.RunAwayMode` 系列。
    pub run_away_mode: bool,
    pub run_away_start: i64,
    pub run_away_time: i64,
    /// `MonsterObject.CheckRoyaltyTick`（`MonsterObject.cs:45`，ctor `:67` = `GetTickCount()`）
    /// —— `CheckRoyalty`（`MonsterObject.cs:363-397`）的 10s 检查门。
    /// 归 `openmir2-m3-pet-slave-lifecycle`（⑨-3）；由 [`AnimalState::seed_from_spawn`] 置为生成时刻。
    pub check_royalty_tick: i64,
    /// **`RM_STRUCK` 消息队列的等价物**（W4 怪物 AI 攻击支）。
    ///
    /// oracle `_Attack`（`BaseObject.Attack.cs:61-64`）对被命中的 `Race > ActorRace.Play` 目标
    /// `targetObject.SendMsg(targetObject, RM_STRUCK, …)` ⇒ **入队**（`BaseObject.Message.cs:98-183`
    /// 的 `MsgQueue.Enqueue`，非同步）。目标在自己的 `Run()` 里由
    /// `while (GetMessage(...)) Operate(...)`（`BaseObject.Base.cs:26-30`）出队，`Operate(RM_STRUCK)`
    /// （`AnimalObject.cs:118-124`）→ `SetLastHiter` + `Struck`。
    ///
    /// 本骨架无 `MsgQueue`：命中者按**入队序**压入本 `Vec`，由
    /// [`crate::monai::base_object_run_monster`]（= 怪物自己的 `base.Run()`）在开头出队处理，
    /// 使 `Struck` 的取数落在「怪自己的 `base.Run`」这个 site（与 oracle 同），
    /// 而不是攻击方的结算处。
    pub pending_struck: Vec<StruckMsg>,
    /// **`RM_UPDATEVIEWRANGE` 消息队列的等价物**（`MonsterObject.cs:136-185`）。
    ///
    /// oracle `PlayObject.SearchViewRange`（`PlayObject.cs:3130-3139`）对满足
    /// `CellType == Monster` 且两轴 `|Δ| <= ViewRange - target.ViewRange` 的怪物
    /// `SystemShare.ActorMgr.SendMessage(monster.ActorId, RM_UPDATEVIEWRANGE, player.ActorId, …)`
    /// ⇒ 入怪物自己的 `MsgQueue`。怪物在自己的 `BaseObject.Run()` 消息段
    /// （`BaseObject.Base.cs:26-30`）出队 → `MonsterObject.Operate`（`MonsterObject.cs:177-185`）
    /// → `UpdateMonsterVisible`（按 `SearchTime` 节流）。
    ///
    /// 本骨架按**入队序**存玩家 actor id；由 [`crate::monai::base_object_run_monster`] 在
    /// 消息段消费（[`crate::aoi::update_monster_visible`]）。`GuardUnit`/`CloneMonster`
    /// 没有该 `Operate` 支（oracle 丢弃消息）⇒ 投递侧过滤，不入队。
    pub pending_view_updates: Vec<i32>,
    /// `BaseObject.StruckTick`（`AnimalObject.cs:336` 的 `StruckTick = GetTickCount()`）。
    pub struck_tick: i64,
    /// `BaseObject.Animal`（`BaseObject.cs:161`，构造缺省 `false`；`BaseObject.cs:2602` 的
    /// `case 54` 置 `true`）——`Struck` 的肉质扣减门（`AnimalObject.cs:348-355`）。
    /// 本骨架按**怪种分类**等价物读它（[`crate::monai::is_animal_race`] 由 `Entity::race` 推出），
    /// 本字段仅保留作显式覆盖位（当前无写入点，恒 `false`）。
    pub animal: bool,
    /// `AnimalObject.MeatQuality`（`BaseObject.cs:2576` 的 `Random(3500)+3000` 初始化，
    /// `Struck` 里按 `Random(300)` 扣减）。本骨架只落字段与扣减，初始化路径未建模（登记）。
    pub meat_quality: u16,
    /// `DualAxeMonster._mNAttackCount`（`DualAxeMonster.cs:8`，ctor 置 0）。
    ///
    /// 只被 `DualAxeMonster.AttackTarget` 读写（`DualAxeMonster.cs:44-54`）；`AttackMax` 是
    /// 类常量（87⇒2 / 93⇒3 / 104⇒6），由 [`crate::monai`] 按 race 推导，不落字段。
    pub m_n_attack_count: i32,
    /// `ZilKinZombi.ZilKillCount`（`ZilKinZombi.cs:9`）：ctor 抽数由
    /// [`MonsterCtorDraws::zil_kill_count`] 提供（`seed_from_spawn` 落字段），
    /// `Die()` 在 `>0` 时抽复活延迟并**无条件** `-= 1`（`ZilKinZombi.cs:27-32`）。
    pub zil_kill_count: i32,
    /// `ZilKinZombi.ZilkinDieTick`（`ZilKinZombi.cs:10`）：`Die()` 里
    /// `(Random(20)+4)*1000`；`Run` 的复活门 `(now - ReAliveTick) >= ZilkinDieTick`。
    pub zilkin_die_tick: i64,
}

impl AnimalState {
    /// 按 oracle 的**生成期取数**初始化一个怪物的 AI 状态（承接卡 `openmir2-m3-monster-ai-tick` 的宿主接线）。
    ///
    /// 这些值的抽取时机在 oracle 里分两处、但都发生在 `CreateMonster` 内、落在实体登记之前：
    /// - 构造链（`new Xxx()`）抽出的 `ctor`（`BaseObject.cs:439/441`、`AnimalObject.cs:76/77`、
    ///   `MonsterObject.cs:65`/各派生类 ctor）；
    /// - `cert.Dir = RandomByte(8)`（`WorldServer.MonGen.cs:740`）与 `CoolEye`（`:745-747`）。
    ///
    /// `now` = oracle 的 `HUtil32.GetTickCount()`（本骨架用 `World::clock_ms` 的确定性注入）。
    /// 本函数**不抽任何随机数**（抽数已在生成路径完成，见 [`MonsterCtorDraws`]）。
    ///
    /// 未建模的构造抽：`BaseObject.RunTick`（`run_tick_delta`）、`MeatQuality`——登记，不假造。
    /// `ZilKillCount` 自 MB-2 起有对应字段（[`AnimalState::zil_kill_count`]），由本节消费。
    pub fn seed_from_spawn(&mut self, dir: u8, cool_eye: bool, ctor: &MonsterCtorDraws, now: i64) {
        self.dir = dir;
        self.cool_eye = cool_eye;
        self.bo_fear_fire = ctor.bo_fear_fire;
        // `AnimalObject.AttackTick/WalkTick = GetTickCount() - Random(3000)`（`AnimalObject.cs:76-77`）。
        self.attack_tick = now - i64::from(ctor.attack_tick_delta);
        self.walk_tick = now - i64::from(ctor.walk_tick_delta);
        // 构造链最终生效的 `SearchTime`（`MonsterObject.cs:57` 覆盖 `BaseObject.cs:441`）。
        self.search_time = i64::from(ctor.search_time);
        // `SearchTick`/`ThinkTick`/`SearchEnemyTick`/`WalkWaitTick`/`RunAwayStart = now`
        // （`BaseObject.cs:442`、`MonsterObject.cs:55-58`、`AnimalObject.cs:78/84/80`）。
        self.search_tick = now;
        self.search_enemy_tick = now;
        self.think_tick = now;
        self.walk_wait_tick = now;
        self.run_away_start = now;
        // `ZilKinZombi` ctor 的 `ZilKillCount`（`ZilKinZombi.cs:17-21`）已由生成路径抽好，
        // 这里只**消费**不重抽（ctor 抽数契约见 `bins/game-svr/src/mongen.rs`）。
        self.zil_kill_count = ctor.zil_kill_count;
        self.zilkin_die_tick = 0;
        // `MonsterObject` ctor `CheckRoyaltyTick = GetTickCount()`（`MonsterObject.cs:67`）。
        self.check_royalty_tick = now;
    }
}

impl Default for AnimalState {
    fn default() -> Self {
        AnimalState {
            dir: 0,
            cool_eye: false,
            bo_fear_fire: false,
            no_attack_mode: false,
            target_creature: None,
            target_focus_tick: 0,
            // oracle ctor 只置 `TargetX = -1`（`AnimalObject.cs:74`），`TargetY` 保持 0。
            target_x: -1,
            target_y: 0,
            attack_tick: 0,
            next_hit_time: 2000,
            walk_tick: 0,
            walk_count: 0,
            walk_step: 0,
            walk_wait: 0,
            walk_wait_tick: 0,
            walk_wait_locked: false,
            walk_speed: 1400,
            search_tick: 0,
            search_time: BASE_ANIMAL_SEARCH_TIME,
            search_enemy_tick: 0,
            think_tick: 0,
            dup_mode: false,
            holy_seize: false,
            holy_seize_tick: 0,
            holy_seize_interval: 0,
            crazy_mode: false,
            crazy_mode_tick: 0,
            crazy_mode_interval: 0,
            run_away_mode: false,
            run_away_start: 0,
            run_away_time: 0,
            check_royalty_tick: 0,
            pending_struck: Vec::new(),
            pending_view_updates: Vec::new(),
            struck_tick: 0,
            animal: false,
            meat_quality: 0,
            m_n_attack_count: 0,
            zil_kill_count: 0,
            zilkin_die_tick: 0,
        }
    }
}

/// `RM_STRUCK` 的入队载荷（oracle `SendMsg(target, RM_STRUCK, nPower, …, ActorId)`）。
///
/// `w_param` = `nPower`（`BaseObject.Attack.cs:61-64`：护甲减免后的实际伤害；未命中/非法目标 = 0）。
/// `CloneMonster.Operate` 的 `Master.WAbil.MP -= (ushort)wParam` 读它
/// （`Clone.cs:24-27`）；普通怪走 `AnimalObject.Operate`（只用 `ActorId` 找命中者）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StruckMsg {
    /// `ProcessMessage.ActorId`（命中者）。
    pub hiter: i32,
    /// `ProcessMessage.wParam`（= `nPower`）。
    pub w_param: i32,
}

/// `BaseObject` 中参与本骨架（tick / AOI / 移动）的字段子集。
///
/// S1 起 `Entity` 直接就是 bevy 的**组件**（每个实体一个组件），由
/// `mir2_world_bevy::EcsStore`（bevy `World` + 显式槽位索引）承载。
#[derive(Clone, Debug, bevy_ecs::prelude::Component)]
pub struct Entity {
    /// C# `ActorId`
    pub id: i32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    /// C# `ViewRange`（`BaseObject` 默认 5）
    pub view_range: i32,
    /// C# `Race`
    pub race: u8,
    /// C# `Master`（宝宝的主人）
    pub master: Option<i32>,
    /// C# `BaseObject.SlaveRelax`（`BaseObject.cs:145`）——主人对宝宝的「放松」标志。
    ///
    /// `MonsterObject.Run` 的两处消费（`MonsterObject.cs:320/334`）：
    /// ① 为真时**不**因异图/超距而 `SpaceMove` 追随；② 为真时在 `GotoTargetXY` 之前
    /// `base.Run(); return;` ⇒ 宝宝**停在原地**（不游走/不攻击）。oracle 源码无写入点
    /// （属性缺省 `false`，由外部命令/插件置位）；本骨架同样缺省 `false`，只在显式赋值时生效。
    pub slave_relax: bool,
    /// C# `BaseObject.SlaveList`（`BaseObject.cs:141`，懒建于 `:1183`）——主人的宝宝列表（actor id，保序）。
    ///
    /// 归 `openmir2-m3-pet-slave-summon`（⑨-2）建立、`openmir2-m3-pet-slave-lifecycle`（⑨-3）清理。
    pub slave_list: Vec<i32>,
    /// C# `BaseObject.StickMode`（`BaseObject.cs:173`；ctor `:403` 置 `false`）。
    ///
    /// `BeeQueen`/`SpiderHouseMonster` ctor 显式置 `true`（`BeeQueen.cs:19`、
    /// `SpiderHouseMonster.cs:19`），由 [`crate::special_monster::apply_ctor_fields`] 落字段。
    /// 消费点：44 火焰球的推挤门 `!target.StickMode`（`MagicManager.cs:1048`）。
    pub stick_mode: bool,
    /// C# `BeeQueen.BeeList` / `SpiderHouseMonster._bbList`（`BeeQueen.cs:11` /
    /// `SpiderHouseMonster.cs:11`）——产出的宝宝 actor id 列表（**插入序**）。
    ///
    /// oracle 是 `IList<IActor>` 强引用；本骨架存 id，实体已不在仓库时按「已不可达」
    /// 处理（清理段等价于 `Death || Ghost`）。
    pub special_children: Vec<i32>,
    /// C# `AnimalObject.IsSlave`（`AnimalObject.cs:54`）。
    pub is_slave: bool,
    /// C# `AnimalObject.SlaveExpLevel`（`AnimalObject.cs:27`）——宝宝等级（召唤时/跨服时设定；
    /// `MonsterObject.GainSlaveExp` 是死代码，击杀不涨，见盘点报告 §1.C）。
    pub slave_exp_level: u8,
    /// C# `MonsterObject.SlaveMakeLevel`（`MonsterObject.cs:21`）——召唤时的技能等级。
    pub slave_make_level: u8,
    /// C# `MonsterObject.MasterRoyaltyTick`（`MonsterObject.cs:37`）——叛变时刻。
    pub master_royalty_tick: i64,
    /// C# `MonsterObject.MasterTick`（`MonsterObject.cs:41`）——诱惑到期的起算时刻。
    pub master_tick: i64,
    pub death: bool,
    pub invisible: bool,
    pub ghost: bool,
    /// C# `BaseObject.DeathTick`（`BaseObject.cs:204`，`Die()` 在 `:323` 写 `GetTickCount()`）——
    /// 死亡时刻（毫秒）。`BaseObject.Run` 死亡支的 `now - DeathTick` 门读它
    /// （`BaseObject.Base.cs:90/99`）。
    pub death_tick: i64,
    /// C# `BaseObject.GhostTick`（`BaseObject.cs:207`）——`MakeGhost()` 写（`BaseObject.Base.cs:494`）；
    /// 幽灵怪 5 分钟清理门 `(now - GhostTick) > 5*60*1000`（`WorldServer.MonGen.cs:302`）。
    pub ghost_tick: i64,
    /// C# `BaseObject.CanReAlive`（`BaseObject.cs:220`）——`RegenMonsters` 刷出的怪置 `true`
    /// （`WorldServer.MonGen.cs:832/854`）；换图/魔法清位（`BaseObject.cs:2749-2756`、
    /// `MagicManager.cs:743-745`）。决定 `MakeGhost` 走「留尸体可复活」还是「真 Ghost」。
    pub can_re_alive: bool,
    /// C# `BaseObject.ReAliveTick`（`BaseObject.cs:224`）——复活冷却起点
    /// （`WorldServer.MonGen.cs:833/855` 置创建时刻、`:290` 在复活成功后刷新）。
    pub re_alive_tick: i64,
    /// C# `BaseObject.MonGen`（`BaseObject.cs` 属性）——本实体所属 `MonGenInfo` 在
    /// [`crate::mongen::MonGenState::entries`] 里的下标；`None` = oracle `MonGen == null`。
    pub mon_gen: Option<usize>,
    /// C# `MonGen.ZenTime` 的缓存（死亡支 `GetMonstersZenTime(MonGen.ZenTime)`，
    /// `BaseObject.Base.cs:87`）。仅 [`Entity::mon_gen`] 为 `Some` 时有效。
    /// 缓存不改变语义：`MonGenInfo.ZenTime` 是配置列，运行期不变。
    pub mon_gen_zen_time: i32,
    /// C# `BaseObject.HideMode`（隐身术/隐身戒指的"完全隐身"）。
    ///
    /// 与 [`Entity::invisible`]（oracle `Invisible`，`is_proper_target` 用）和
    /// [`Entity::fixed_hide_mode`]（oracle `FixedHideMode`）是**三个不同字段**：
    /// oracle `AnimalObject.SearchTarget` 的隐藏门读的是 `HideMode`（`AnimalObject.cs:402`），
    /// 只有 `Envirnoment.GetXYObjCount` / `BaseObject.Run` 才读 `FixedHideMode`。
    /// 当前本仓无写入点（隐身术/隐身戒指未接线）⇒ 两侧行为今天相同；接线后在 `search_target`
    /// 的隐藏门上与 oracle 1:1。
    pub hide_mode: bool,
    pub fixed_hide_mode: bool,
    pub ob_mode: bool,
    /// C# `NastyMode`（PK 模式等）
    pub nasty_mode: bool,
    /// C# `PlayObject.AttatckMode`（`AttackMode` 枚举，`src/OpenMir2/Enums/AttackMode.cs`）：
    /// `HAM_ALL=0 / HAM_PEACE=1 / HAM_DEAR=2 / HAM_MASTER=3 / HAM_GROUP=4 / HAM_GUILD=5 / HAM_PKATTACK=6`。
    /// 新角色 `PlayObject.cs:908` 置 `0`（`HAM_ALL`）；`ChangeAttackModeCommand.cs:26` 按 Ctrl-H 循环。
    pub attack_mode: u8,
    /// C# `BaseObject.InSafeZone()`（`BaseObject.cs:1777`）的结果折叠。
    ///
    /// oracle 的 `IsProtectTarget`（`BaseObject.Base.cs:360`）用它做 PvP 门槛；本骨架未建模
    /// 地图 Flag/StartPoint 安全区，由接线层在入图/换图时按 `Envir` 算好后写入；默认 `false`
    /// （= 非安全区，写清了才允许 PvP，与真机 pin 到比奇城东空地一致）。
    pub in_safe_zone: bool,
    /// C# `BaseObject.AdminMode`（`BaseObject.IsAttackTarget` 首判：管理员不可被攻击）。
    pub admin_mode: bool,
    /// C# `BaseObject.StoneMode`（同上，石化不可被攻击）。
    pub stone_mode: bool,
    /// C# `PlayObject.DearHuman`（`PlayObject.IsAttackTarget` 的 `HAM_DEAR` 支：只保护配偶）。
    pub dear_human: Option<i32>,
    /// C# `PlayObject.PkPoint`——`PlayObject.PvpLevel()`（`PlayObject.cs:4352`）= `PkPoint / 100`。
    /// `HAM_PKATTACK` 支用它做红名/白名判定。
    pub pk_point: i32,
    /// C# `PlayObject.PvpFlag`（`PlayObject.cs:4336`）——PK 标记；`SetPkFlag` 置位并触发
    /// `RefNameColor`（⇒ `SM_CHANGENAMECOLOR(656)`）。ctor 缺省 `false`。
    ///
    /// 消费方（P3-B4）：50 的 `playObject.SetPkFlag(targetObject)`（`MagicManager.cs:1249`）与
    /// `RM_POISON` 投递的 `((IPlayerActor)this).SetPkFlag(caster)`（`BaseObject.Base.cs:749`）。
    pub pvp_flag: bool,
    /// C# `PlayObject.PvpNameColorTick`（`PlayObject.cs:243`；接口声明 `IPlayerActor.cs:257`）——`SetPkFlag` 置位时刻
    /// （`PlayObject.cs:4333` 的 `targetObject.PvpNameColorTick = GetTickCount()`）。
    /// `CheckPkStatus`（`:4359`）用 `now - PvpNameColorTick > Config.dwPKFlagTime` 判过期。
    ///
    /// **重复置位也刷新**（oracle 赋值在 `if (!targetObject.PvpFlag)` **之前**）。
    pub pvp_name_color_tick: i64,
    /// C# `PlayObject.Luck`（`PlayObject.cs:62`，`RecalcAbilitys` 里 `Luck = 0 + AddAbil.Luck - AddAbil.UnLuck`）。
    /// 玩家攻方的 [`crate::combat::get_attack_power_player`] override 用它决定取数次数。
    pub luck: i32,
    /// C# `BaseObject.NameColor`（`BaseObject.cs:28/385`，ctor 初值 `255`）。
    ///
    /// `GetNameColor()`（`BaseObject.Base.cs:904-907`）返回它；`GetChrColor`
    /// （`BaseObject.Base.cs:869-883`）的兜底分支与 `MonsterObject`/`PlayObject` 的 override 都读它。
    /// 归 `openmir2-m3-pet-slave-lifecycle`（⑨-3）。
    pub name_color: u8,
    /// C# `PlayObject.PowerRate`（`PlayObject.cs:724`；新角色 `PlayObject.cs:943` 置 `100`）。
    pub power_rate: i32,
    /// C# `PlayObject.BoPowerItem`（`PlayObject.cs:223`）。
    pub bo_power_item: bool,
    /// C# `PlayObject.PowerItem`（`PlayObject.cs:199`）。
    pub power_item: u8,
    /// C# `PlayObject.FireHitSkill` / `LatestFireHitTick`（`PlayObject.cs:181/194`）——
    /// 烈火剑法待发标志与最近一次置位时刻。`PlayObject.Run` 的到期段
    /// （`PlayObject.Message.cs:54-58`）严格 `(now - LatestFireHitTick) > 20*1000` ⇒ 清标志 +
    /// `SpiritsGone` + `SendSocket("+UFIR")`（[`crate::World::expire_player_run_state`]）。
    ///
    /// **写入点登记**：oracle 的 `AllowFireHitSkill`（`PlayObject.Base.cs:653-663`，施放 26 时置位）
    /// 与攻击命中清标志（`PlayObject.Attack.cs:591-593/614-616`）依赖尚未移植的技能层；
    /// 本骨架的生产路径暂无写入者（恒 `false`）⇒ 到期段零动作。字段/到期先落地，接线时无需再改。
    pub fire_hit_skill: bool,
    pub latest_fire_hit_tick: i64,
    /// C# `PlayObject.TwinHitSkill` / `LatestTwinHitTick`（`PlayObject.cs:185/195`）——双龙斩。
    /// 到期段 `PlayObject.Message.cs:60-64`：严格 `> 60*1000` ⇒ 清标志 + `SendSocket("+UTWN")`
    /// （**无** `SysMsg`）。写入点同 [`Entity::fire_hit_skill`] 登记（技能层未移植）。
    pub twin_hit_skill: bool,
    pub latest_twin_hit_tick: i64,
    /// C# `PlayObject.BoDuplication` / `DupStartTick` / `CheckDupObjTick`
    /// （`PlayObject.cs:94/98/258`）——`PlayObject.Run` 的**重叠推挤**状态
    /// （`PlayObject.Message.cs:263-285`，[`crate::World::check_dup_obj`]）：
    ///
    /// - `check_dup_obj_tick`：上一次重叠检查的时刻；ctor `CheckDupObjTick = GetTickCount()`
    ///   （`PlayObject.cs:913`）。本骨架 ctor 等价物（[`Entity::new`]）拿不到世界时钟 ⇒ 由落图点
    ///   （`World::add_entity` / `SessionRegistry::enter_map`）按 `clock_ms` 重盖（同 `verify_tick`）。
    /// - `bo_duplication` / `dup_start_tick`：本格对象数 `>= 2` 的**首次**观测标记与时刻；对象数
    ///   掉回 `< 2` 只清 `bo_duplication`，**不**复位 `dup_start_tick`（照 oracle）。
    pub bo_duplication: bool,
    pub dup_start_tick: i64,
    pub check_dup_obj_tick: i64,
    /// C# `CharacterObject.UseItems`（`CharacterObject.cs:40`，13 格，`CharacterObject.cs:89` 初始化）。
    /// `PlayObject.StruckDamage` 的耐久循环按它取数。
    pub use_items: [UseItemSlot; USE_ITEM_SLOTS],
    /// C# `WantRefMsg`
    pub want_ref_msg: bool,
    /// C# `AddToMaped`（首次入图标记）
    pub add_to_mapped: bool,
    /// C# `VisibleActors`
    pub visible_actors: Vec<VisibleBaseObject>,
    /// C# `IsVisibleActive`
    pub is_visible_active: bool,
    /// 本 tick 是否已被处理（tick 顺序判据用）
    pub processed_tick: u64,
    /// 所在地图（多地图世界用；`World::new` 单图时为 0）
    pub map_id: usize,
    /// `WAbil.HP` / `WAbil.MaxHP`
    pub hp: u16,
    pub max_hp: u16,
    /// `WAbil.MP` / `WAbil.MaxMP`。
    ///
    /// 怪物生成时照 `WorldServer.MonGen.cs:896-897` 的 `ApplyMonsterAbility`：`MP = 0`、
    /// `MaxMP = MonsterInfo.MP`（注意 `MP` 在怪物表里被复用作 `MonsterWeapon` 的形参，见
    /// `:903`；`BaseObject.Run` 的恢复段仍按 `WAbil.MaxMP` 回蓝）。
    pub mp: u16,
    pub max_mp: u16,
    /// C# `BaseObject.PoisoningTick`（`BaseObject.cs:301`，ctor `:443` 置 `GetTickCount()`）——
    /// 毒伤 DoT 的周期门时刻（`ActorBuffSystem.cs:235`，缺省 `Config.PosionDecHealthTime` = 2500ms）。
    /// 缺省 = [`crate::buff::BUFF_EPOCH_MS`]（模拟开机毫秒基）；ctor 拿不到 `World` 时钟 ⇒ 落图时由
    /// [`crate::world::World::add_entity`] 按 `clock_ms + BUFF_EPOCH_MS` 精确重盖，与 oracle ctor
    /// 「对象建立那一刻」取 `GetTickCount()` 对齐。
    pub poisoning_tick: i64,
    /// C# `BaseObject.VerifyTick`（`BaseObject.cs:302`；ctor `:444` 置 `GetTickCount()`）——
    /// `BaseObject.Run` 的 30s 门 `(now - VerifyTick) > 30*1000` 命中时刷新，并调用
    /// `Envir.VerifyMapTime(CurrX, CurrY, this)`（`BaseObject.Base.cs:223-228`）刷新本格
    /// `CellObject.AddTime`，避免静止 actor 的格子项被 60s 过期逐出（`Envirnoment.cs:858-897`）。
    ///
    /// 与 `poisoning_tick` 同口径：ctor 等价物 [`Entity::new`] 拿不到 `World` 时钟 ⇒ 先置 0，
    /// 由落图/生成时刻（`World::add_entity` / `enter_map` / `spawn_monster_*` / `insert_merchant` /
    /// `insert_npc` / 克隆体）按 `clock_ms` 精确重盖。
    pub verify_tick: i64,
    /// C# `BaseObject.GreenPoisoningPoint`（`BaseObject.cs:117`，`MakePosion` `:1963` 置 `(byte)nPoint`）。
    /// DoT 扣血量 = `GreenPoisoningPoint + 1`（`ActorBuffSystem.cs:252`）。
    pub green_poisoning_point: u8,
    /// C# `BaseObject.CharStatus`（`BaseObject.cs:56`）——`GetCharStatus()` 的结果缓存
    /// （`ActorBuffSystem.Operate` 的 `boChg` 支 / `MakePosion` 写它）。
    pub char_status: i32,
    /// C# `BaseObject.CharStatusEx`（`BaseObject.cs:57`，ctor `:393` 置 0）——低 16 位并入 `GetCharStatus`。
    pub char_status_ex: i32,
    /// C# `BaseObject.StatusTimeArr`（`BaseObject.cs:81`，ctor `:394` 置 15 个 0）。
    ///
    /// **P3-B3 新增（`crates/world` 接口变更，须按 §12.4 通知并行车道）**：
    /// 18/19 的隐身门、40 的三个计时器读写、以及 16/18/19 之外的写点（移动破隐 / 透明戒指 /
    /// `Disappear` / `GasMoth` / `FrostTiger` / `Abil`）都落在这一份存储上。
    /// 递减循环被注释（`BaseObject.Base.cs:249-315`）⇒ 只有写入点，没有周期衰减。
    pub status_time: [u16; 15],
    /// C# `BaseObject.Transparent`（`BaseObject.cs:259`；18/19 的 `MagMakePrivateTransparent` 收尾置 true）。
    pub transparent: bool,
    /// C# `BaseObject.ShowHp`（28 心灵启示的 `MakeOpenHealth` 置 true）。
    pub show_hp: bool,
    /// C# `BaseObject.ShowHpTick`（`MagicManager.cs:436` 置当前 tick）。
    pub show_hp_tick: i64,
    /// C# `BaseObject.ShowHpInterval`（`MagicManager.cs:437`：`GetPower13(...) * 1000`）。
    pub show_hp_interval: i32,
    /// C# `BaseObject.SuperMan`（32 圣言术的首门 `target.SuperMan`）。
    pub super_man: bool,
    /// C# `CharacterObject.HitSpeed`（`PlayObject.Message.cs:1846` 的 `SM_CHARSTATUSCHANGED` 末参
    /// 取**收件人自己**的 `this.HitSpeed`）。
    ///
    /// **P3-B3 新增**：`RecalcAbilitys` 的写入点尚未接到实体（登记），普通路径恒 0；
    /// 657 帧改为逐收件人读本字段构造（不再用一条 `hit_speed=0` 的 body 广播）。
    pub hit_speed: u16,
    /// C# `PlayObject.MagicShield`（`PlayObject.cs:490`）——`DamageHealth` 的 MP 抵扣门（`BaseObject.cs:779`）。
    pub magic_shield: bool,
    /// C# `PlayObject.UnMagicShield`（`PlayObject.cs:494`）——`DamageHealth` 首行守卫（`BaseObject.cs:777`）。
    pub un_magic_shield: bool,
    /// `WAbil.DC`（低字节=下限、高字节=上限，与 C# 一致）
    pub dc: i32,
    /// `WAbil.AC`
    pub ac: i32,
    /// `HitPoint` / `SpeedPoint`（命中/敏捷，命中判定用）
    pub hit_point: u8,
    pub speed_point: u8,
    /// C# `BaseObject.AntiMagic`（`BaseObject.cs:113`；ctor `:390` 置 **0**）——
    /// 魔法骰门 `Random(10) >= AntiMagic` 的右操作数（`MagicManager.cs:180/292/334/566/1033`）。
    ///
    /// **普通 `MonGen` 生成路径保持 0**：oracle 只有 `RecalcAbilitys` 会把它置成
    /// `1 + AddAbil.AntiMagic`（`BaseObject.Abil.cs:30` 置 1、`:67` 累加），而普通生成的怪
    /// **不调用** `RecalcAbilitys`；`ElfMonster`/`WhiteSkeleton` 等覆写也不改 `AntiMagic`。
    /// ⇒ 普通怪目标下 `Random(10) >= 0` 恒真，骰门永不失败。
    ///
    /// 本仓的等价 `RecalcAbilitys` 写入点：玩家登录/升级
    /// （`bins/game-svr` 的 `apply_ability_to_entity`）与宝宝召唤
    /// （[`crate::World::make_slave_adopt`]，`BaseObject.cs:1198`），均置 `1 + AddAbil.AntiMagic`
    /// （怪物/玩家的 `AddAbil` 在本骨架内为 0 ⇒ 1）。复活（`BaseObject.cs:2739`）与其余
    /// slave 面（`MakeClone`/`GainSlaveExp`）尚未建模 ⇒ 这些路径保持 0（登记）。
    /// 设计锚：`mir2-rs/tests/parity/m3/design-monster-run-overrides-20261017.md` §4.1.2-5 / §5-2。
    pub anti_magic: u16,
    /// `WAbil.MAC`（`BaseObject` 构造 ctor 缺省 **0**；`RecalcAbilitys` 后 = `Abil.MAC + AddAbil.MAC`
    /// 的逐字节 `MakeWord` 合成，见 `bins/game-svr` 的 `apply_ability_to_entity`）。
    ///
    /// **P3-B2 新增（`crates/world` 接口变更，须按 §12.4 通知并行车道）**：`RM_MAGSTRUCK` 交付段的
    /// `GetMagStruckDamage`（`BaseObject.Base.cs:812-821`）唯一读点：
    /// `Lo(MAC) + Random(|Hi(MAC)-Lo(MAC)|+1)`，再从 `nDamage` 里扣。
    /// 普通 `MonGen` 怪走 ctor 缺省 0（与 `anti_magic` 同口径，生成路径不调 `RecalcAbilitys`）。
    pub mac: u16,
    /// C# `BaseObject.RushMode`——`RM_MAGSTRUCK` 的 `Random(1000)` 门
    /// （`BaseObject.Base.cs:620`：`!RushMode` 才 `WalkTick += 800 + Random(1000)`）。
    ///
    /// **P3-B5 新增**（`crates/world` 接口变更，须按 §12.4 通知并行车道）：
    /// ctor 缺省 `false`（oracle 构造缺省）；本骨架无写入点（冲锋态未建模）⇒ 生产恒 false。
    pub rush_mode: bool,
    /// C# `PlayObject.ExtraAbil[7]`（`PlayObject.cs:201`）——`AttPowerUp` 写槽 0
    /// （`EABIL_DCUP`，`ActorConst.cs:26`）。**P3-B5 新增**。
    pub extra_abil: [u16; 7],
    /// C# `PlayObject.ExtraAbilTimes[7]`（`PlayObject.cs:202`）——到期时刻（sim 毫秒）。
    pub extra_abil_times: [i64; 7],
    /// C# `PlayObject.ExtraAbilFlag[7]`（`PlayObject.cs:203`）——「到期前 10s 提示已发」标记。
    pub extra_abil_flag: [u8; 7],
    /// `Job`（0 战 / 1 法 / 2 道）
    pub job: u8,
    /// `LifeAttrib`（1 = `LA_UNDEAD` 不死系）
    pub life_attrib: u8,
    /// `Abil.Level`
    pub level: u8,
    /// `MonsterInfo.Exp`（怪物经验，死亡时按 `CalcGetExp` 结算）
    pub mon_exp: i32,
    /// `Abil.Exp` / `Abil.MaxExp`
    pub exp: i32,
    pub max_exp: i32,
    /// `AddAbil.UndeadPower`（攻击方对不死系的加成）
    pub undead_power: i32,
    /// `LastHiter`（最后一击者 `ActorId`）
    pub last_hiter: Option<i32>,
    /// C# `ExpHitter`（经验归属者 `ActorId`）——由 `SetLastHiter` 设置：
    /// **第一个**命中者粘住不放（`BaseObject.cs:1753` 的 `if (ExpHitter == null)`），
    /// 死亡时 `KillFunc()`（`BaseObject.cs:2429`）把经验发给它，而不是 `LastHiter`。
    pub exp_hitter: Option<i32>,
    /// `BaseObject.LastHiterTick` / `ExpHitterTick`（`BaseObject.cs:295-297`）。
    /// 由 [`Entity::set_last_hiter`]（`BaseObject.cs:1753-1770`）刷新；`BaseObject.Run`
    /// 的过期段（`BaseObject.Base.cs:119-132`）用它们判 30s / 6s 墙钟过期。
    pub last_hiter_tick: i64,
    pub exp_hitter_tick: i64,
    /// `BaseObject.HealthTick` / `SpellTick` / `AutoRecoveryTick`（`BaseObject.cs:287-306`）。
    ///
    /// 恢复段（`BaseObject.Base.cs:39-66`）按 `(now - AutoRecoveryTick) / 20` 把时间折成
    /// `HealthTick/SpellTick`，到 `HealthFillTime`/`SpellFillTime` 才回 HP/MP。
    pub health_tick: i32,
    pub spell_tick: i32,
    pub auto_recovery_tick: i64,
    /// 掉落预算里的金币（`mon.Gold`）
    pub gold: i32,
    /// 掉落预算里的物品（`mon.ItemList`，生成时算好、死亡时落地）
    pub drop_items: Vec<crate::combat::DropItem>,
    /// 死亡结算是否已做（避免重复发经验/重复掉落）
    pub death_settled: bool,
    /// 构造函数链抽出的随机数结果（见 [`MonsterCtorDraws`]）；玩家/非 MonGen 路径实体为缺省全零。
    pub ctor_draws: MonsterCtorDraws,
    /// C# `StickMonster.ComeOutValue`（`StickMonster.cs:10`；ctor `:19` 置 4）。
    ///
    /// **类常量**：只有 `StickMonster` 家族有该字段（`CentipedeKingMonster.cs:14` = 4、
    /// `SandMobObject.cs:11` = 8）。类由 `MonsterInfo.Race` 决定（`WorldServer.MonGen.cs:504-729`
    /// 的 `new Xxx()`），装载钩子见 [`crate::mongen::apply_monster_class_constants`]。
    /// `Entity` 是各 C# 类的公共载体，非该家族的怪保持 C# 字段缺省 0。
    ///
    /// 消费方：MB-1（`StickMonster.Run` R38）、MB-3（`CentipedeKingMonster.Run` R07）、
    /// MB-8（`SandMobObject.Run` R33）。
    pub come_out_value: i32,
    /// C# `StickMonster.AttackRange`（`StickMonster.cs:11`；ctor `:20` 置 4；
    /// `CentipedeKingMonster.cs:15` 置 6）。缺省 0（非 `StickMonster` 家族无此字段）。
    ///
    /// 消费方：MB-1（R38）、MB-3（R07）、MB-8（R33）。
    pub attack_range: i32,
    /// C# `MagicMonObject.UseMagic`（`MagicMonObject.cs:10`，ctor `:15` = `false`）/
    /// `ElectronicScolpionMon.UseMagic`（`ElectronicScolpionMon.cs:7`，ctor `:12` = `false`）。
    /// 运行期由各自 `Run` 按血量改写（`MagicMonObject.cs:29/33`、`ElectronicScolpionMon.cs:39/43`）。
    ///
    /// 消费方：MB-6（R25 / R14）。
    pub use_magic: bool,
    /// C# `BoneKingMonster.DangerLevel`（`short`，`BoneKingMonster.cs:9`；ctor `:17` = 5）/
    /// `ScultureKingMonster._mNDangerLevel`（`int`，`ScultureKingMonster.cs:11`；ctor `:21` = 5）。
    /// 装载后按类置 5；运行期递减/回满（`BoneKingMonster.cs:56-63`、`ScultureKingMonster.cs:94-101`）。
    ///
    /// 消费方：MB-7（BoneKing R05）、MB-3（ScultureKing R34）。
    pub danger_level: i32,
    /// C# `SoccerBall.N550`（`SoccerBall.cs:9`；ctor `:15` = 0）。
    ///
    /// 消费方：MB-8（SoccerBall R36；`Struck` `:83` 写、`Run` `:24/:70` 读写）。
    pub n550: i32,
    /// C# `GuardUnit.GuardDirection`（`sbyte`，`GuardUnit.cs:16`，无显式初值 ⇒ 0）；
    /// `ArcherGuard` ctor `ArcherGuard.cs:17` 置 **-1**（`ArcherPolice` 继承）。
    /// 运行期城堡守卫由 `UserCastle.cs:187` 改 3（未建模，登记）。
    ///
    /// 消费方：MB-3（ArcherGuard R02）。
    pub guard_direction: i8,
    /// **oracle 无持久字段**：`CowKingMonster.SiegeLockCount()` 是 `Run` 内每次现算的
    /// 「3×3 邻域不可走格数」（`CowKingMonster.cs:96-118`，`>=5` 门在 `:47`）。
    ///
    /// 本字段是 PR-2 按规划落地的**缓存/暂存槽**（无写入点、无读取点 ⇒ 零行为改动）；
    /// 缺省 0 与 C# `int` 字段缺省一致。MB-4 若选择缓存现算结果再用，可写这里；若逐次现算，
    /// 本字段保持 0 即可。
    ///
    /// 消费方：MB-4（CowKing R10）。
    pub siege_lock_count: i32,
    /// C# `CowKingMonster` 的 8 个私有字段（`CowKingMonster.cs:8-15`）——`Run`（`:40-94`）
    /// 的跳跃/狂暴状态。缺省 = C# 字段缺省（`int`/`bool` 全 0/false），
    /// ctor（`:17-25`）与 `Initialize()`（`:33-38`）的赋值由装载钩子落定。
    ///
    /// **勿与 [`AnimalState::crazy_mode`] 混淆**：后者是 `MonsterObject.CrazyMode`
    /// （`OpenCrazyMode` 的通用狂暴），本组是 CowKing 专用状态机。
    ///
    /// 消费方：MB-4（R10）。
    ///
    /// `JumpTime`（`:8`；ctor `:20` = `GetTickCount()`）。
    pub jump_time: i64,
    /// `CrazyReadyMode`（`CowKingMonster.cs:9`；ctor `:23` = false）。
    pub crazy_ready_mode: bool,
    /// `CrazyKingMode`（`CowKingMonster.cs:10`；ctor `:24` = false）。
    pub crazy_king_mode: bool,
    /// `CrazyCount`（`CowKingMonster.cs:11`；ctor `:22` = 0）。
    pub crazy_count: i32,
    /// `CrazyReady`（`CowKingMonster.cs:12`，tick；字段缺省 0，进入 ready 时写）。
    pub crazy_ready: i64,
    /// `CrazyTime`（`CowKingMonster.cs:13`，tick；字段缺省 0，进入 king 时写）。
    pub crazy_time: i64,
    /// `oldHitTime`（`CowKingMonster.cs:14`，tick；字段缺省 0，`Initialize()` `:35` = `NextHitTime`）。
    pub old_hit_time: i64,
    /// `oldWalkTime`（`CowKingMonster.cs:15`，tick；字段缺省 0，`Initialize()` `:36` = `WalkSpeed`）。
    pub old_walk_time: i64,
    /// 怪物/动物 AI 状态（`AnimalObject`/`MonsterObject` 字段子集；玩家不用，保持默认）。
    pub ai: AnimalState,
    /// 商人状态（`Merchant` 字段子集，M3⑤-实现 2 第①片）。`Some(_)` ⇒ 本实体是交易商人：
    /// [`crate::monai::monster_ai_tick`] 显式跳过它（race 50 与 `ActorRace.Animal` 同值，
    /// 靠 race 会误跑 `monster_run`）。
    pub merchant: Option<crate::merchant::MerchantState>,
    /// 固定管理 NPC 状态（`NormNpc` 字段子集 + ctor 抽数，卡 `openmir2-npcs-fixed-loading`）。
    /// `Some(_)` ⇒ 本实体是 `Envir/Npcs.txt` 的固定 NPC（Merchant/GuildOfficial/CastleOfficial）：
    /// [`crate::monai::monster_ai_tick`] 显式跳过它（固定 NPC 由 [`crate::World::npc_tick`]
    /// 驱动，即 oracle `ProcessNpcs` 的建模；`monster_ai_tick` 与 `npc_tick` 不得双驱）；
    /// [`crate::World::merchant_tick`] 只认 `merchant.is_some()` ⇒ 也跳过。
    pub npc: Option<crate::npc::NpcState>,
    /// C# `PlayObject.AllowGroup`（`PlayObject.cs:106` 声明、`:902` 构造初值 `false`）——
    /// 是否允许被拉进小组（`ClientCreateGroup`/`ClientAddGroupMember` 的门）。
    pub allow_group: bool,
    /// C# `PlayObject.AllowGroupReCall`（`PlayObject.cs:224`）——是否允许被组队传送（`@GroupRecall`）。
    /// 构造未显式赋值 ⇒ `bool` 缺省 `false`。[`crate::party`] 不消费它；扩展面（⑦-可选）接线用。
    pub allow_group_re_call: bool,
    /// C# `PlayObject.GroupOwner`（`PlayObject.cs:583`，构造默认 `0` = 无组）——**团长** `ActorId`。
    pub group_owner: i32,
    /// C# `PlayObject.GroupMembers`（`PlayObject.cs:587`，构造 `:1042` `new List`）——
    /// 组内成员 `ActorId` 列表（**含团长自己**，与 oracle 一致）。顺序 = 加入序（oracle 的 `IList`）。
    pub group_members: Vec<i32>,
}

impl Entity {
    pub fn new(id: i32, name: impl Into<String>, x: i32, y: i32) -> Self {
        Entity {
            id,
            name: name.into(),
            x,
            y,
            view_range: 5, // BaseObject.cs:383
            race: ACTOR_RACE_PLAY,
            master: None,
            slave_relax: false,
            slave_list: Vec::new(),
            // `BaseObject.cs:403`：ctor `StickMode = false`；两类虫后由 ctor 等价点置 true。
            stick_mode: false,
            special_children: Vec::new(),
            is_slave: false,
            slave_exp_level: 0,
            slave_make_level: 0,
            master_royalty_tick: 0,
            master_tick: 0,
            death: false,
            invisible: false,
            hide_mode: false,
            ghost: false,
            death_tick: 0,
            ghost_tick: 0,
            can_re_alive: false,
            re_alive_tick: 0,
            mon_gen: None,
            mon_gen_zen_time: 0,
            fixed_hide_mode: false,
            ob_mode: false,
            nasty_mode: false,
            // `PlayObject.cs:908` 新角色 `AttatckMode = 0 = HAM_ALL`；未接 `CM_ATTACKMODE` 前恒为 0。
            attack_mode: 0,
            in_safe_zone: false,
            admin_mode: false,
            stone_mode: false,
            dear_human: None,
            pk_point: 0,
            pvp_flag: false,
            pvp_name_color_tick: 0,
            // `BaseObject.cs:385`：ctor 初值 `NameColor = 255`。
            name_color: 255,
            // `PlayObject.cs:62/943`：新角色 `Luck` 由 `RecalcAbilitys` 置 0（无 `AddAbil`），
            // `PowerRate = 100`；`BoPowerItem=false` / `PowerItem=0`。
            luck: 0,
            power_rate: 100,
            bo_power_item: false,
            power_item: 0,
            // `PlayObject.cs:181/185/194/195`：ctor 缺省 `FireHitSkill/TwinHitSkill = false`、
            // 两个 tick 为 0（C# `int` 缺省）；真实置位来自技能层（未移植，见字段文档）。
            fire_hit_skill: false,
            latest_fire_hit_tick: 0,
            twin_hit_skill: false,
            latest_twin_hit_tick: 0,
            // `PlayObject.cs:94/98`：ctor 缺省 `BoDuplication=false` / `DupStartTick=0`；
            // `CheckDupObjTick` 由 `World::add_entity` / `SessionRegistry::enter_map` 按落图时钟重盖
            // （`PlayObject.cs:913` 的 `GetTickCount()`）。
            bo_duplication: false,
            dup_start_tick: 0,
            check_dup_obj_tick: 0,
            // `CharacterObject.cs:89`：13 个空位（`Index = 0`）。
            use_items: [UseItemSlot::default(); USE_ITEM_SLOTS],
            want_ref_msg: false,
            add_to_mapped: false,
            visible_actors: Vec::new(),
            is_visible_active: false,
            processed_tick: 0,
            map_id: 0,
            // 玩家默认满血（WAbil.HP 的默认值不能是 0：0 在语义上等于「已死」，
            // 会让死亡结算把刚进图的玩家当成死者——实测踩过）
            hp: 100,
            max_hp: 100,
            mp: 0,
            max_mp: 0,
            // `BaseObject.cs:391`/`:393`：`GreenPoisoningPoint = 0`、`CharStatusEx = 0`；
            // `CharStatus` 未显式赋值 ⇒ int 缺省 0；`PoisoningTick` ctor 置 `GetTickCount()`
            // （本骨架用模拟开机基 `BUFF_EPOCH_MS`；精确落图时刻由 `World::add_entity` 重盖）。
            poisoning_tick: crate::buff::BUFF_EPOCH_MS,
            // `BaseObject.cs:444` ctor `VerifyTick = GetTickCount()`：落图时刻由 `World` 重盖。
            verify_tick: 0,
            green_poisoning_point: 0,
            char_status: 0,
            char_status_ex: 0,
            // `BaseObject.cs:81/394`：`StatusTimeArr = new ushort[15]`（全 0）；
            // `Transparent`/`ShowHp`/`ShowHpTick`/`ShowHpInterval`/`SuperMan` 构造缺省。
            status_time: [0; 15],
            transparent: false,
            show_hp: false,
            show_hp_tick: 0,
            show_hp_interval: 0,
            super_man: false,
            // `CharacterObject.HitSpeed`（`RecalcAbilitys` 才赋值）⇒ ctor 缺省 0。
            hit_speed: 0,
            // `PlayObject.cs:490/494`：`MagicShield`/`UnMagicShield` 构造缺省 `false`。
            magic_shield: false,
            un_magic_shield: false,
            dc: 0,
            ac: 0,
            hit_point: 0,
            speed_point: 0,
            // `BaseObject.cs:390`：ctor 置 `AntiMagic = 0`（普通生成路径不调 `RecalcAbilitys`）。
            anti_magic: 0,
            // `BaseObject.WAbil.MAC` 构造缺省 0（`RecalcAbilitys` 才写 `Abil.MAC + AddAbil.MAC`）。
            mac: 0,
            // `BaseObject.RushMode` 构造缺省 false（`BaseObject.cs` ctor）。
            rush_mode: false,
            // `PlayObject.cs:201-203`：三个 7 槽数组构造缺省全 0。
            extra_abil: [0; 7],
            extra_abil_times: [0; 7],
            extra_abil_flag: [0; 7],
            job: 0,
            life_attrib: 0,
            level: 1,
            mon_exp: 0,
            exp: 0,
            max_exp: 0,
            undead_power: 0,
            last_hiter: None,
            exp_hitter: None,
            last_hiter_tick: 0,
            exp_hitter_tick: 0,
            health_tick: 0,
            spell_tick: 0,
            auto_recovery_tick: 0,
            gold: 0,
            drop_items: Vec::new(),
            death_settled: false,
            ctor_draws: MonsterCtorDraws::default(),
            // 专用怪 ctor 类常量（PR-2）：`Entity` 是 C# 各怪物类的公共载体，这里落
            // **C# 字段缺省**（`int`/`sbyte` 0、`bool` false）；派生类 ctor 的实际常量
            // （StickMonster 4/4、CentipedeKing 4/6、SandMob 8/4、BoneKing/ScultureKing 5、
            // ArcherGuard -1 等）由装载钩子 `crate::mongen::apply_monster_class_constants`
            // 按 race 覆盖（oracle `new Xxx()` 的位置）。无抽数。
            come_out_value: 0,
            attack_range: 0,
            use_magic: false,
            danger_level: 0,
            n550: 0,
            guard_direction: 0,
            siege_lock_count: 0,
            jump_time: 0,
            crazy_ready_mode: false,
            crazy_king_mode: false,
            crazy_count: 0,
            crazy_ready: 0,
            crazy_time: 0,
            old_hit_time: 0,
            old_walk_time: 0,
            // `PlayObject` ctor（`PlayObject.cs:899-901`）在 `BaseObject` ctor 之后立即覆盖
            // `SearchTime = 1000` 与 `SearchTick = GetTickCount()`；本函数是玩家的 ctor
            // 等价物（`race` 缺省 = `ActorRace.Play`），故这里落 1000（常量、不抽数）。
            // `SearchTick` 拿不到世界时钟 ⇒ 由落图点（`World::add_entity`/`enter_map`）按
            // `clock_ms` 重盖（同 `verify_tick` 口径）。
            ai: AnimalState {
                search_time: PLAYER_SEARCH_TIME,
                ..AnimalState::default()
            },
            merchant: None,
            npc: None,
            // 组队态构造初值：`PlayObject.cs:902` `AllowGroup=false`、`:583` `GroupOwner=0`、
            // `:1042` `GroupMembers=new List()`、`AllowGroupReCall`（`:224`）bool 缺省 `false`。
            allow_group: false,
            allow_group_re_call: false,
            group_owner: 0,
            group_members: Vec::new(),
        }
    }

    /// 当前朝向（C# `BaseObject.Dir`），供接线层发 `SM_TURN`/`RM_TURN` 等帧面用。
    #[must_use]
    pub fn dir(&self) -> u8 {
        self.ai.dir
    }

    /// C# `BaseObject.SetLastHiter`（`BaseObject.cs:1753`）。
    ///
    /// `LastHiter` 每次都刷新并续 `LastHiterTick`；`ExpHitter` **只在为空时**才认领
    /// （第一个命中者粘住），之后只有同一个命中者再次命中才会续 `ExpHitterTick`
    /// （`BaseObject.cs:1753-1770` 逐支照抄；30s/6s 过期在 `World::expire_hitters`）。
    pub fn set_last_hiter(&mut self, hitter: i32, now: i64) {
        self.last_hiter = Some(hitter);
        self.last_hiter_tick = now;
        if self.exp_hitter.is_none() {
            self.exp_hitter = Some(hitter);
            self.exp_hitter_tick = now;
        } else if self.exp_hitter == Some(hitter) {
            self.exp_hitter_tick = now;
        }
    }
}

/// 实体仓库（`ActorMgr` 等价物）——S1 起是 **bevy_ecs 内核的薄适配层**。
///
/// 存储与遍历顺序都在 [`mir2_world_bevy::EcsStore`]（bevy `World` + 显式槽位索引）：
/// - 槽位顺序 = 插入顺序 + 空闲槽 LIFO 复用，`iter`/`iter_mut`/`ids_sorted`/`order_index` 全按它；
/// - `get`/`get_mut` 仍是 O(1) 查表；代际号在内核里维护（对外不可见）。
///
/// **顺序契约**：本类型**不迭代 Query 求顺序**——`iter`/`iter_mut` 走内核的显式槽位索引，
/// `Query`（内核里的 `world.query`）只用于「按句柄安全拿到互斥引用」，之后仍按槽位排序。
#[derive(Debug)]
pub struct EntityStore {
    inner: mir2_world_bevy::EcsStore<Entity>,
}

impl Default for EntityStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityStore {
    pub fn new() -> Self {
        let mut inner = mir2_world_bevy::EcsStore::new();
        // 内核资源：显式处理顺序索引 + 本 tick 顺序（tick 骨架的系统按它们处理）。
        inner
            .world_mut()
            .insert_resource(mir2_world_bevy::SlotOrder::default());
        inner
            .world_mut()
            .insert_resource(mir2_world_bevy::TickOrder::default());
        EntityStore { inner }
    }

    /// 分配一个 id（不插入实体；C# `ActorMgr` 先 `Add` 后填字段）。
    pub fn alloc_id(&mut self) -> i32 {
        self.inner.alloc_id()
    }

    /// 插入实体，返回其 id（复用本实体的 `id`）。
    pub fn insert(&mut self, entity: Entity) -> i32 {
        self.inner.insert(entity.id, entity)
    }

    pub fn remove(&mut self, id: i32) -> Option<Entity> {
        self.inner.remove(id)
    }

    /// O(1) 查表（C# `ActorMgr.Get`）。
    pub fn get(&self, id: i32) -> Option<&Entity> {
        self.inner.get(id)
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut Entity> {
        self.inner.get_mut(id)
    }

    /// 按槽位顺序遍历（tick 的确定性顺序）。
    pub fn iter(&self) -> impl Iterator<Item = &Entity> + '_ {
        self.inner.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Entity> + '_ {
        self.inner.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// 按 id 升序收集（用于输出/对拍的稳定顺序）。
    pub fn ids_sorted(&self) -> Vec<i32> {
        self.inner.ids_sorted()
    }

    /// **显式处理顺序索引**：槽位顺序的业务 id（`crates/world` 的处理顺序契约）。
    pub fn order_index(&self) -> Vec<i32> {
        self.inner.order_index()
    }

    /// 把当前槽位顺序写入 ECS 资源 [`mir2_world_bevy::SlotOrder`]（tick 骨架处理前调用）。
    pub fn sync_slot_order(&mut self) {
        self.inner.sync_slot_order();
    }

    /// 暴露内核的 bevy `World`（`TickRunner` 跑单线程 Schedule 用）。
    pub fn world(&self) -> &bevy_ecs::world::World {
        self.inner.world()
    }

    pub fn world_mut(&mut self) -> &mut bevy_ecs::world::World {
        self.inner.world_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PR-2：`Entity::new` 是 C# 各怪物类的公共载体——新落的专用怪类常量必须是
    /// **C# 字段缺省**（`int`/`sbyte` 0、`bool` false）；派生类 ctor 的实际常量由装载钩子
    /// （[`crate::mongen::apply_monster_class_constants`]）按 race 覆盖。
    ///
    /// 牙①：把 `attack_range: 0` 改成 `4`（把 `StickMonster` ctor 常量误当公共缺省）⇒ 本用例红。
    #[test]
    fn new_entity_class_constants_default_to_csharp_field_defaults() {
        let e = Entity::new(1, "x", 0, 0);
        assert_eq!(e.come_out_value, 0, "ComeOutValue int 缺省");
        assert_eq!(e.attack_range, 0, "AttackRange int 缺省");
        assert!(!e.use_magic, "UseMagic bool 缺省");
        assert_eq!(e.danger_level, 0, "DangerLevel short/int 缺省");
        assert_eq!(e.n550, 0, "N550 int 缺省");
        assert_eq!(e.guard_direction, 0, "GuardDirection sbyte 缺省");
        assert_eq!(e.siege_lock_count, 0, "SiegeLockCount 缓存槽缺省");
        assert_eq!(e.jump_time, 0, "JumpTime int 缺省");
        assert!(!e.crazy_ready_mode, "CrazyReadyMode bool 缺省");
        assert!(!e.crazy_king_mode, "CrazyKingMode bool 缺省");
        assert_eq!(e.crazy_count, 0, "CrazyCount int 缺省");
        assert_eq!(e.crazy_ready, 0, "CrazyReady int 缺省");
        assert_eq!(e.crazy_time, 0, "CrazyTime int 缺省");
        assert_eq!(e.old_hit_time, 0, "oldHitTime int 缺省");
        assert_eq!(e.old_walk_time, 0, "oldWalkTime int 缺省");
    }

    #[test]
    fn ids_are_stable_and_order_is_slot_order() {
        let mut s = EntityStore::new();
        let a = s.alloc_id();
        let b = s.alloc_id();
        s.insert(Entity::new(a, "a", 1, 1));
        s.insert(Entity::new(b, "b", 2, 2));
        assert_eq!(
            s.iter().map(|e| e.name.clone()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        s.remove(a);
        let c = s.alloc_id();
        s.insert(Entity::new(c, "c", 3, 3));
        // 回收槽复用：c 落在 a 的槽位（顺序 = 槽位顺序，判据①钉住这一点）
        assert_eq!(
            s.iter().map(|e| e.name.clone()).collect::<Vec<_>>(),
            vec!["c", "b"]
        );
    }
}
