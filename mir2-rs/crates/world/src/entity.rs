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

/// `BaseObject` ctor 的 `Dir = 4`（`BaseObject.cs:378`）——`AnimalObject`/`NormNpc`/`Merchant`
/// 均不覆写；只有生成的怪物在 `CreateMonster` 里用 `RandomByte(8)` 重盖（`WorldServer.MonGen.cs:740`）。
pub const BASE_OBJECT_DIR: u8 = 4;
/// `NormNpc` ctor 的 `Light = 2`（`NormNpc.cs:43`）；`Merchant : NormNpc` 继承同值。
pub const NORM_NPC_LIGHT: u8 = 2;
/// `NormNpc` ctor 的 `AntiPoison = 99`（`NormNpc.cs:44`）；`Merchant : NormNpc` 继承同值。
pub const NORM_NPC_ANTI_POISON: u8 = 99;

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
    /// `UserItem.MakeIndex`（`DropUseItems` 的 `DeleteItem.MakeIndex` 与 `SM_DELITEMS` 取它；
    /// 清槽时 oracle **只清 `Index`**、保留本字段）。
    pub make_index: i32,
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

/// C# `PlayObject.VisibleEvents`（`IList<MapEvent>`，`PlayObject.cs:211`）的条目——
/// **卡 `openmir2-map-event-frames`**。
///
/// **BUG-REPLICA（`MapEvent.Id == 0` 折叠）**：oracle `MapEvent.Id` 是 `readonly int` 且赋值行
/// 被注释（`MapEvent.cs:62`）⇒ 恒 0；`CellObjectMgr` 是 `ConcurrentDictionary<int, object>` 的
/// **TryAdd**（`CellObjectMgr.cs:10-16`）⇒ 全进程只有**第一条**成功注册的事件占住 key 0，且
/// `DeleteFromMap` 只在 `CellObjId > 0` 支移除（`Envirnoment.cs:652-667`）⇒ key 0 **永不替换**。
/// 因此 `SearchViewRange` 对**任何**事件格的 `Get<MapEvent>(0)` 都解析到**同一个对象**，
/// 每观察者的 `VisibleEvents` 至多一条（差集按**对象同一性**去重，`PlayObject.cs:5468`
/// `if (mapEvent == MapEvent)` 是引用比较）。`event` 存的是该折叠对象在
/// `World::fire_burn_events` 里的下标——**不得**改成「每事件一个 id」，否则第二次进事件格
/// 会多出一条 `SM_SHOWEVENT`（oracle 不发）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleEvent {
    /// 折叠到的唯一事件：`fire_burn_events` 里**第一条** `registered == true` 的下标。
    pub event: usize,
    /// **add 时刻共享 sight 的快照**（review `39262b65` F3）：oracle 首见支写共享对象的
    /// `nX/nY`（`PlayObject.cs:5483-5484`），同趟扫描内早退支不覆写 ⇒ 804 在扫描后读共享值
    /// 与该快照恒相等。**805 不读这里**——读发送时刻共享值（`World::event_sight`，`aoi` 收尾支）。
    pub x: i32,
    pub y: i32,
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
    /// `ExplosionSpider.explosionMakeTime`（`ExplosionSpider.cs:8`）：
    /// ctor 置 `GetTickCount()`（`:16`），`Run` 每 60s 门刷新（`:83-86`）。
    /// 只有 race 117 读；所有怪在 [`AnimalState::seed_from_spawn`] 按落图时刻置初值。
    pub explosion_make_time: i64,
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
    /// `BaseObject.BodyLeathery`（`src/M2Server/Actor/BaseObject.cs:186`，ctor `:401` 置 **50**；
    /// 生成路径按 race 覆写：`WorldServer.MonGen.cs:521-551` 的 52/53 ⇒ **150**，51 ⇒ 50）。
    ///
    /// **类型是 `byte`** ⇒ 屠宰效果体的 `-= n10`（`PlayObject.Operate.cs:724`）在 C# unchecked
    /// 下按 `(byte)` **回绕**（如 3-5 ⇒ 254），随后的 `<= 0` 判据因此只命中 **`== 0`**。
    /// 唯一读点就是这个 `== 0` 门；写在 [`crate::world::World`] 的屠宰效果体。
    pub body_leathery: u8,
    /// `BaseObject.Skeleton`（`src/M2Server/Actor/BaseObject.cs:182`，ctor `:396` 置 `false`）。
    ///
    /// **唯一 setter** = 屠宰效果体（`PlayObject.Operate.cs:734`，`BodyLeathery` 归零 + race 在
    /// `[Animal=50, Monster=80)`）；**唯一清点** = `ReAliveEx`（`BaseObject.cs:2472`，本仓
    /// [`crate::world::World::revive_monster`]）。`Death && Skeleton` 时 AOI 出现帧发
    /// `RM_SKELETON` 而非 `RM_DEATH`（`PlayObject.cs:3229-3236`；该出现帧支本卡未接，登记 T-206）。
    pub skeleton: bool,
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
        // `ExplosionSpider` ctor `explosionMakeTime = GetTickCount()`（`ExplosionSpider.cs:16`）。
        // 只有 race 117 消费；其余怪该字段无读点（写入无害，与「字段缺省」等价）。
        self.explosion_make_time = now;
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
            explosion_make_time: 0,
            check_royalty_tick: 0,
            pending_struck: Vec::new(),
            pending_view_updates: Vec::new(),
            struck_tick: 0,
            animal: false,
            meat_quality: 0,
            // `BaseObject()` ctor `BodyLeathery = 50` / `Skeleton = false`（`BaseObject.cs:401/396`）。
            body_leathery: 50,
            skeleton: false,
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

/// 实体 → 城堡防御单元的反向绑定（oracle `BaseObject.Castle = this` 的等价物）。
///
/// `UserCastle.Initialize` 成功造出的门 / 墙 / 弓手在 `World.castles[castle_index]` 里各有
/// 一份 [`crate::castle::CastleUnit`]；本结构让每 tick 的 `Run`（race 110/111）与死亡路由
/// 能从实体反查那一份状态机。
///
/// **守卫不绑**：oracle 守卫生成后只写 `WAbil.HP`、不设 `Castle`
/// （`UserCastle.cs:202-206`）⇒ `castle_binding` 恒 `None`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastleBinding {
    /// `World.castles` 的下标。
    pub castle_index: usize,
    /// `MainDoor` / `LeftWall` / `CenterWall` / `RightWall` / `Archer(i)`。
    pub role: crate::castle::DefenseRole,
}

/// C# `BaseObject.Abil` 的**本骨架子集快照**（`Abil.DC` / `Abil.MaxHP`）。
///
/// oracle `RecalcAbilitys` 首行 `WAbil = Abil.Clone()`（`src/M2Server/Actor/BaseObject.Abil.cs:19`）
/// 再按 `Abil` 重算；`MakeClone`（`src/M2Server/Monster/MonsterObject.cs:113`）与
/// `ElfMonster.AppearNow`（`src/M2Server/Monster/Monsters/ElfMonster.cs:28`）/
/// `ElfWarriorMonster.AppearNow`（`src/M2Server/Monster/Monsters/ElfWarriorMonster.cs:20`）
/// 各调一次 `RecalcAbilitys`。第二次必须从 `Abil` 重算（`MonsterObject.cs:115` 已
/// `ElfMon.Abil = OldMon.Abil`），**不能**复合在当前 `WAbil` 上。
///
/// 本骨架只建模 `RecalcAbilitys` 对怪物可观测的两个字段：`DC` 与 `MaxHP`（其余
/// `AC/MAC/MC/SC/Weight*` 等怪物路径未被 `ApplySlaveLevelAbilitys` 改写，登记于
/// `ApplySlaveLevelAbilitys` 文档）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseAbil {
    /// C# `Abil.DC`（`WAbil.DC` 在 `RecalcAbilitys` 内先被 `Abil.Clone()` 覆盖）。
    pub dc: i32,
    /// C# `Abil.MaxHP`（`WAbil.MaxHP = Abil.MaxHP + AddAbil.HP`，怪物 `AddAbil` 为空）。
    pub max_hp: u16,
}

/// **S3b**：位置/视野组件（`x` / `y` / `map_id` / `view_range`）——字段真值在
/// `mir2_world_bevy::Transform`，`Entity` 已删除这四个字段。
///
/// 读：`EntityStore::transform(id)`；写：`EntityStore::transform_mut(id)`（或
/// `set_xy` / `set_map_id` 等受控入口）。格子登记与组件更新必须同一段代码内完成。
pub use mir2_world_bevy::Transform;

/// **S4a**：属性/成长组件的 world 侧实例化（真值在 `mir2_world_bevy` 组件，`Entity` 已删字段）。
pub use mir2_world_bevy::{
    AbilitySnapshot, CombatAttribution, Progression, PvpState, Recovery, Vitals,
};
/// **S4b-1**：状态位/生命周期/外观/类专用状态的 world 侧实例化（`Entity` 已删字段）。
pub use mir2_world_bevy::{Appearance, Flags, LifeCycle, MonsterClassState};
/// **S4b-2**：玩家运行态/组队/复活/Buff 组件的 world 侧实例化（`Entity` 已删字段）。
pub use mir2_world_bevy::{BuffState, GroupState, PlayerRunState, Revival};
use mir2_world_bevy::{EXTRA_ABIL_SLOTS, STATUS_TIME_SLOTS};
/// **S4a**：`Vitals`/`AbilitySnapshot`/`Progression` 三组件的组合类型（`EntitySpawn`/`EntityRef`/`EntityMut` 用）。
pub type Ability = AbilitySnapshot<BaseAbil>;
/// **S4a**：掉落预算组件的 world 侧实例化（`items` 对应迁移前的 `drop_items`）。
pub type DropState = mir2_world_bevy::DropState<crate::combat::DropItem>;
/// **S4a**：生成/归属绑定组件的 world 侧实例化。
pub type SpawnBinding = mir2_world_bevy::SpawnBinding<CastleBinding, MonsterCtorDraws>;

/// **S3b**：AOI/落图绑定组件的 world 侧实例化（6 字段真值）。
pub type MapBinding = mir2_world_bevy::MapBinding<VisibleBaseObject, VisibleEvent>;

/// **生成期载体**（`Entity` + 落图前的 `Transform` / `MapBinding` / S4a 属性组件）。
///
/// 只为保持 约 700 处 `Entity::new(id, name, x, y)` + `e.x = …` 的生成期写法不变；
/// [`EntityStore::insert`] 消费它并把坐标/绑定/属性落成组件。**插入后不再存在第二份真值**：
/// 载体被 move 进 insert，后续读写一律走组件。
#[derive(Debug, Clone)]
pub struct EntitySpawn {
    pub entity: Entity,
    /// 落图前坐标/视野（`insert` 时落成 `Transform` 组件）。
    pub x: i32,
    pub y: i32,
    pub map_id: usize,
    pub view_range: i32,
    /// 落图前 AOI 绑定（`insert` 时落成 `MapBinding` 组件）。
    pub map_binding: MapBinding,
    // ---- S4a：生成期属性/成长（`insert` 时落成 Vitals/AbilitySnapshot/Progression 组件）----
    // 字段平铺在载体上，保持 约 700 处生成期写法（`e.hp = …`）不变；插入后不再有第二份真值。
    pub hp: u16,
    pub max_hp: u16,
    pub mp: u16,
    pub max_mp: u16,
    pub hit_speed: u16,
    pub dc: i32,
    pub ac: i32,
    pub mac: u16,
    pub hit_point: u8,
    pub speed_point: u8,
    pub anti_magic: u16,
    pub anti_poison: u8,
    pub undead_power: i32,
    pub abil: Option<BaseAbil>,
    pub job: u8,
    pub life_attrib: u8,
    pub level: u8,
    pub mon_exp: i32,
    pub exp: i32,
    pub max_exp: i32,
    // ---- S4a-2：生成/归属/掉落/恢复/PK（`insert` 时落成对应组件）----
    pub castle_binding: Option<CastleBinding>,
    pub mon_gen: Option<usize>,
    pub mon_gen_zen_time: i32,
    pub ctor_draws: MonsterCtorDraws,
    pub gold: i32,
    pub drop_items: Vec<crate::combat::DropItem>,
    pub angry_ring: bool,
    pub no_drop_use_item: bool,
    pub no_item: bool,
    pub last_hiter: Option<i32>,
    pub exp_hitter: Option<i32>,
    pub last_hiter_tick: i64,
    pub exp_hitter_tick: i64,
    pub health_tick: i32,
    pub spell_tick: i32,
    pub auto_recovery_tick: i64,
    pub dear_human: Option<i32>,
    pub pk_point: i32,
    pub pvp_flag: bool,
    pub pvp_name_color_tick: i64,
    pub die_in_fight3_zone: bool,
    // ---- S4b-1：状态位/生命周期/外观/类专用状态（`insert` 时落成对应组件）----
    // 生成期写入（如 `e.race = …`）保持平铺写法；插入后真值只在组件里。
    pub invisible: bool,
    pub hide_mode: bool,
    pub fixed_hide_mode: bool,
    pub ob_mode: bool,
    pub nasty_mode: bool,
    pub attack_mode: u8,
    pub in_safe_zone: bool,
    pub admin_mode: bool,
    pub stone_mode: bool,
    pub super_man: bool,
    pub rush_mode: bool,
    pub want_ref_msg: bool,
    pub stick_mode: bool,
    pub death: bool,
    pub ghost: bool,
    pub death_tick: i64,
    pub ghost_tick: i64,
    pub can_re_alive: bool,
    pub re_alive_tick: i64,
    pub death_settled: bool,
    pub die_called: bool,
    pub race: u8,
    pub race_img: u8,
    pub appr: u16,
    pub light: u8,
    pub name_color: u8,
    pub come_out_value: i32,
    pub attack_range: i32,
    pub use_magic: bool,
    pub danger_level: i32,
    pub n550: i32,
    pub class_attack_tick: i64,
    pub bo_is_first: bool,
    pub dig_down_tick: i64,
    pub appear_start_tick: i64,
    pub guard_direction: i8,
    pub siege_lock_count: i32,
    pub jump_time: i64,
    pub crazy_ready_mode: bool,
    pub crazy_king_mode: bool,
    pub crazy_count: i32,
    pub crazy_ready: i64,
    pub crazy_time: i64,
    pub old_hit_time: i64,
    pub old_walk_time: i64,
    // ---- S4b-2：玩家运行态/组队/复活/Buff（`insert` 时落成对应组件）----
    pub luck: i32,
    pub power_rate: i32,
    pub power_rate_time: i32,
    pub kill_mon_exp_rate: i32,
    pub kill_mon_exp_rate_time: i32,
    pub exp_rate_tick: i64,
    pub random_no: i32,
    pub bo_power_item: bool,
    pub power_item: u8,
    pub fire_hit_skill: bool,
    pub latest_fire_hit_tick: i64,
    pub twin_hit_skill: bool,
    pub latest_twin_hit_tick: i64,
    pub allow_group: bool,
    pub allow_group_re_call: bool,
    pub group_owner: i32,
    pub group_members: Vec<i32>,
    pub revival: bool,
    pub revival_tick: i64,
    pub un_revival: bool,
    pub flame_ring: bool,
    pub poisoning_tick: i64,
    pub green_poisoning_point: u8,
    pub char_status: i32,
    pub char_status_ex: i32,
    pub status_time: [u16; STATUS_TIME_SLOTS],
    pub transparent: bool,
    pub show_hp: bool,
    pub show_hp_tick: i64,
    pub show_hp_interval: i32,
    pub magic_shield: bool,
    pub un_magic_shield: bool,
    pub extra_abil: [u16; EXTRA_ABIL_SLOTS],
    pub extra_abil_times: [i64; EXTRA_ABIL_SLOTS],
    pub extra_abil_flag: [u8; EXTRA_ABIL_SLOTS],
}

impl EntitySpawn {
    /// ctor 缺省值（`Entity::new`：`Vitals.hp/max_hp = 100/100`、`Progression.level = 1`；
    /// 其余 0/`None`，见迁移文档 §3.4）。
    ///
    /// **不得**用 `Component::default()` 直接造实体：`hp=0` 在语义上等于「已死」。
    const CTOR_HP: u16 = 100;
    const CTOR_MAX_HP: u16 = 100;
    const CTOR_LEVEL: u8 = 1;

    pub fn new(
        entity: Entity,
        x: i32,
        y: i32,
        map_id: usize,
        view_range: i32,
        map_binding: MapBinding,
    ) -> Self {
        EntitySpawn {
            entity,
            x,
            y,
            map_id,
            view_range,
            map_binding,
            hp: Self::CTOR_HP,
            max_hp: Self::CTOR_MAX_HP,
            mp: 0,
            max_mp: 0,
            hit_speed: 0,
            dc: 0,
            ac: 0,
            mac: 0,
            hit_point: 0,
            speed_point: 0,
            anti_magic: 0,
            anti_poison: 0,
            undead_power: 0,
            abil: None,
            job: 0,
            life_attrib: 0,
            level: Self::CTOR_LEVEL,
            mon_exp: 0,
            exp: 0,
            max_exp: 0,
            castle_binding: None,
            mon_gen: None,
            mon_gen_zen_time: 0,
            ctor_draws: MonsterCtorDraws::default(),
            gold: 0,
            drop_items: Vec::new(),
            angry_ring: false,
            no_drop_use_item: false,
            no_item: false,
            last_hiter: None,
            exp_hitter: None,
            last_hiter_tick: 0,
            exp_hitter_tick: 0,
            health_tick: 0,
            spell_tick: 0,
            auto_recovery_tick: 0,
            dear_human: None,
            pk_point: 0,
            pvp_flag: false,
            pvp_name_color_tick: 0,
            die_in_fight3_zone: false,
            invisible: false,
            hide_mode: false,
            fixed_hide_mode: false,
            ob_mode: false,
            nasty_mode: false,
            attack_mode: 0,
            in_safe_zone: false,
            admin_mode: false,
            stone_mode: false,
            super_man: false,
            rush_mode: false,
            want_ref_msg: false,
            stick_mode: false,
            death: false,
            ghost: false,
            death_tick: 0,
            ghost_tick: 0,
            can_re_alive: false,
            re_alive_tick: 0,
            death_settled: false,
            die_called: false,
            race: ACTOR_RACE_PLAY,
            race_img: 0,
            appr: 0,
            light: 0,
            name_color: 255,
            come_out_value: 0,
            attack_range: 0,
            use_magic: false,
            danger_level: 0,
            n550: 0,
            class_attack_tick: 0,
            bo_is_first: false,
            dig_down_tick: 0,
            appear_start_tick: 0,
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
            luck: 0,
            power_rate: 100,
            power_rate_time: 0,
            kill_mon_exp_rate: 100,
            kill_mon_exp_rate_time: 0,
            exp_rate_tick: 0,
            random_no: 0,
            bo_power_item: false,
            power_item: 0,
            fire_hit_skill: false,
            latest_fire_hit_tick: 0,
            twin_hit_skill: false,
            latest_twin_hit_tick: 0,
            allow_group: false,
            allow_group_re_call: false,
            group_owner: 0,
            group_members: Vec::new(),
            revival: false,
            revival_tick: 0,
            un_revival: false,
            flame_ring: false,
            poisoning_tick: crate::buff::BUFF_EPOCH_MS,
            green_poisoning_point: 0,
            char_status: 0,
            char_status_ex: 0,
            status_time: [0; STATUS_TIME_SLOTS],
            transparent: false,
            show_hp: false,
            show_hp_tick: 0,
            show_hp_interval: 0,
            magic_shield: false,
            un_magic_shield: false,
            extra_abil: [0; EXTRA_ABIL_SLOTS],
            extra_abil_times: [0; EXTRA_ABIL_SLOTS],
            extra_abil_flag: [0; EXTRA_ABIL_SLOTS],
        }
    }

    /// 把载体上的属性字段落成组件值。
    #[must_use]
    pub fn vitals_component(&self) -> Vitals {
        Vitals {
            hp: self.hp,
            max_hp: self.max_hp,
            mp: self.mp,
            max_mp: self.max_mp,
            hit_speed: self.hit_speed,
            dc: self.dc,
            ac: self.ac,
            mac: self.mac,
            hit_point: self.hit_point,
            speed_point: self.speed_point,
            anti_magic: self.anti_magic,
            anti_poison: self.anti_poison,
            undead_power: self.undead_power,
        }
    }

    /// 把载体上的成长字段落成组件值（供「取副本算完再回写」路径：`win_exp` / `get_exp_raw`）。
    #[must_use]
    pub fn progression_component(&self) -> Progression {
        Progression {
            job: self.job,
            life_attrib: self.life_attrib,
            level: self.level,
            mon_exp: self.mon_exp,
            exp: self.exp,
            max_exp: self.max_exp,
        }
    }

    /// 生成/归属绑定组件值。
    #[must_use]
    pub fn spawn_component(&self) -> SpawnBinding {
        SpawnBinding {
            castle_binding: self.castle_binding,
            mon_gen: self.mon_gen,
            mon_gen_zen_time: self.mon_gen_zen_time,
            ctor_draws: self.ctor_draws,
        }
    }

    /// 掉落预算组件值（`drop_items` → 组件字段 `items`，151 字段里唯一的改名）。
    #[must_use]
    pub fn drop_component(&self) -> DropState {
        DropState {
            gold: self.gold,
            items: self.drop_items.clone(),
            angry_ring: self.angry_ring,
            no_drop_use_item: self.no_drop_use_item,
            no_item: self.no_item,
        }
    }

    /// S4b-1：状态位组件值。
    #[must_use]
    pub fn flags_component(&self) -> Flags {
        Flags {
            invisible: self.invisible,
            hide_mode: self.hide_mode,
            fixed_hide_mode: self.fixed_hide_mode,
            ob_mode: self.ob_mode,
            nasty_mode: self.nasty_mode,
            attack_mode: self.attack_mode,
            in_safe_zone: self.in_safe_zone,
            admin_mode: self.admin_mode,
            stone_mode: self.stone_mode,
            super_man: self.super_man,
            rush_mode: self.rush_mode,
            want_ref_msg: self.want_ref_msg,
            stick_mode: self.stick_mode,
        }
    }

    /// S4b-1：生命周期组件值。
    #[must_use]
    pub fn life_cycle_component(&self) -> LifeCycle {
        LifeCycle {
            death: self.death,
            ghost: self.ghost,
            death_tick: self.death_tick,
            ghost_tick: self.ghost_tick,
            can_re_alive: self.can_re_alive,
            re_alive_tick: self.re_alive_tick,
            death_settled: self.death_settled,
            die_called: self.die_called,
        }
    }

    /// S4b-1：外观/行为轴组件值。
    #[must_use]
    pub fn appearance_component(&self) -> Appearance {
        Appearance {
            race: self.race,
            race_img: self.race_img,
            appr: self.appr,
            light: self.light,
            name_color: self.name_color,
        }
    }

    /// S4b-1：类专用状态组件值。
    #[must_use]
    pub fn monster_class_component(&self) -> MonsterClassState {
        MonsterClassState {
            come_out_value: self.come_out_value,
            attack_range: self.attack_range,
            use_magic: self.use_magic,
            danger_level: self.danger_level,
            n550: self.n550,
            class_attack_tick: self.class_attack_tick,
            bo_is_first: self.bo_is_first,
            dig_down_tick: self.dig_down_tick,
            appear_start_tick: self.appear_start_tick,
            guard_direction: self.guard_direction,
            siege_lock_count: self.siege_lock_count,
            jump_time: self.jump_time,
            crazy_ready_mode: self.crazy_ready_mode,
            crazy_king_mode: self.crazy_king_mode,
            crazy_count: self.crazy_count,
            crazy_ready: self.crazy_ready,
            crazy_time: self.crazy_time,
            old_hit_time: self.old_hit_time,
            old_walk_time: self.old_walk_time,
        }
    }

    /// S4b-2：玩家运行态组件值。
    #[must_use]
    pub fn player_run_component(&self) -> PlayerRunState {
        PlayerRunState {
            luck: self.luck,
            power_rate: self.power_rate,
            power_rate_time: self.power_rate_time,
            kill_mon_exp_rate: self.kill_mon_exp_rate,
            kill_mon_exp_rate_time: self.kill_mon_exp_rate_time,
            exp_rate_tick: self.exp_rate_tick,
            random_no: self.random_no,
            bo_power_item: self.bo_power_item,
            power_item: self.power_item,
            fire_hit_skill: self.fire_hit_skill,
            latest_fire_hit_tick: self.latest_fire_hit_tick,
            twin_hit_skill: self.twin_hit_skill,
            latest_twin_hit_tick: self.latest_twin_hit_tick,
        }
    }

    /// S4b-2：组队实体侧组件值。
    #[must_use]
    pub fn group_component(&self) -> GroupState {
        GroupState {
            allow_group: self.allow_group,
            allow_group_re_call: self.allow_group_re_call,
            group_owner: self.group_owner,
            group_members: self.group_members.clone(),
        }
    }

    /// S4b-2：复活戒指/火球戒指粘滞位组件值。
    #[must_use]
    pub fn revival_component(&self) -> Revival {
        Revival {
            revival: self.revival,
            revival_tick: self.revival_tick,
            un_revival: self.un_revival,
            flame_ring: self.flame_ring,
        }
    }

    /// S4b-2：Buff/毒/心灵启示/魔法盾/ExtraAbil 组件值。
    #[must_use]
    pub fn buff_component(&self) -> BuffState {
        BuffState {
            poisoning_tick: self.poisoning_tick,
            green_poisoning_point: self.green_poisoning_point,
            char_status: self.char_status,
            char_status_ex: self.char_status_ex,
            status_time: self.status_time,
            transparent: self.transparent,
            show_hp: self.show_hp,
            show_hp_tick: self.show_hp_tick,
            show_hp_interval: self.show_hp_interval,
            magic_shield: self.magic_shield,
            un_magic_shield: self.un_magic_shield,
            extra_abil: self.extra_abil,
            extra_abil_times: self.extra_abil_times,
            extra_abil_flag: self.extra_abil_flag,
        }
    }

    /// C# `BaseObject.SetLastHiter`（`BaseObject.cs:1753`）的生成期载体版本。
    ///
    /// 与 [`EntityMut::set_last_hiter`] 逐支同义：`LastHiter` 每次刷新；`ExpHitter` **只在为空时**
    /// 认领（第一个命中者粘住），同命中者再次命中才续 `ExpHitterTick`。
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

    /// 落成组件包（`insert` 用）。
    #[must_use]
    fn into_parts(self) -> SpawnParts {
        let progression = self.progression_component();
        let vitals = self.vitals_component();
        let spawn = self.spawn_component();
        let drop = self.drop_component();
        let flags = self.flags_component();
        let life_cycle = self.life_cycle_component();
        let appearance = self.appearance_component();
        let monster_class = self.monster_class_component();
        let player_run = self.player_run_component();
        let group = self.group_component();
        let revival = self.revival_component();
        let buff = self.buff_component();
        SpawnParts {
            entity: self.entity,
            transform: Transform {
                x: self.x,
                y: self.y,
                map_id: self.map_id,
                view_range: self.view_range,
            },
            map_binding: self.map_binding,
            vitals,
            ability: AbilitySnapshot { abil: self.abil },
            progression,
            spawn,
            drop,
            attribution: CombatAttribution {
                last_hiter: self.last_hiter,
                exp_hitter: self.exp_hitter,
                last_hiter_tick: self.last_hiter_tick,
                exp_hitter_tick: self.exp_hitter_tick,
            },
            recovery: Recovery {
                health_tick: self.health_tick,
                spell_tick: self.spell_tick,
                auto_recovery_tick: self.auto_recovery_tick,
            },
            pvp: PvpState {
                dear_human: self.dear_human,
                pk_point: self.pk_point,
                pvp_flag: self.pvp_flag,
                pvp_name_color_tick: self.pvp_name_color_tick,
                die_in_fight3_zone: self.die_in_fight3_zone,
            },
            flags,
            life_cycle,
            appearance,
            monster_class,
            player_run,
            group,
            revival,
            buff,
        }
    }
}

/// `EntitySpawn::into_parts` 的组件包（`EntityStore::insert` 消费）。
#[derive(Debug)]
pub(crate) struct SpawnParts {
    pub entity: Entity,
    pub transform: Transform,
    pub map_binding: MapBinding,
    pub vitals: Vitals,
    pub ability: AbilitySnapshot<BaseAbil>,
    pub progression: Progression,
    pub spawn: SpawnBinding,
    pub drop: DropState,
    pub attribution: CombatAttribution,
    pub recovery: Recovery,
    pub pvp: PvpState,
    pub flags: Flags,
    pub life_cycle: LifeCycle,
    pub appearance: Appearance,
    pub monster_class: MonsterClassState,
    pub player_run: PlayerRunState,
    pub group: GroupState,
    pub revival: Revival,
    pub buff: BuffState,
}

impl std::ops::Deref for EntitySpawn {
    type Target = Entity;
    fn deref(&self) -> &Entity {
        &self.entity
    }
}

impl std::ops::DerefMut for EntitySpawn {
    fn deref_mut(&mut self) -> &mut Entity {
        &mut self.entity
    }
}

/// **只读视图**：一次按 id 取 `Entity` + `Transform` + `MapBinding` 三个组件。
///
/// 为什么要有它：S3b 把 10 个字段搬出 `Entity` 后，绝大多数读取点仍是
/// `store.get(id).map(|e| (e.x, e.y, e.map_id))` 这种**只读快照**。视图把组件值按 id
/// 一次取齐（拷贝 4 个标量 + 借用 6 个绑定字段），因此读取点保持原形，且不会出现
/// 「`get` 之后组件被改」的中间态——视图存活期间 `&EntityStore` 冻结整个 store。
///
/// 它不是组件、不参与存储，**不是双真值**：标量是借用期快照，`Vec` 是只读借用；
/// 写入一律走 `EntityStore::transform_mut` / `map_binding_mut`（或 `get_mut` + 受控写方法）。
#[derive(Clone, Copy, Debug)]
pub struct EntityRef<'a> {
    entity: &'a Entity,
    pub x: i32,
    pub y: i32,
    pub map_id: usize,
    pub view_range: i32,
    pub add_to_mapped: bool,
    pub visible_actors: &'a Vec<VisibleBaseObject>,
    pub visible_events: &'a Vec<VisibleEvent>,
    pub is_visible_active: bool,
    pub processed_tick: u64,
    pub verify_tick: i64,
    // ---- S4a：属性/成长组件的只读投影（`Copy` 组件按值拷贝）----
    pub hp: u16,
    pub max_hp: u16,
    pub mp: u16,
    pub max_mp: u16,
    pub hit_speed: u16,
    pub dc: i32,
    pub ac: i32,
    pub mac: u16,
    pub hit_point: u8,
    pub speed_point: u8,
    pub anti_magic: u16,
    pub anti_poison: u8,
    pub undead_power: i32,
    pub abil: Option<BaseAbil>,
    pub job: u8,
    pub life_attrib: u8,
    pub level: u8,
    pub mon_exp: i32,
    pub exp: i32,
    pub max_exp: i32,
    // ---- S4a-2：生成/掉落/归属/恢复/PK 的只读投影 ----
    pub castle_binding: Option<CastleBinding>,
    pub mon_gen: Option<usize>,
    pub mon_gen_zen_time: i32,
    pub ctor_draws: MonsterCtorDraws,
    pub gold: i32,
    pub drop_items: &'a Vec<crate::combat::DropItem>,
    pub angry_ring: bool,
    pub no_drop_use_item: bool,
    pub no_item: bool,
    pub last_hiter: Option<i32>,
    pub exp_hitter: Option<i32>,
    pub last_hiter_tick: i64,
    pub exp_hitter_tick: i64,
    pub health_tick: i32,
    pub spell_tick: i32,
    pub auto_recovery_tick: i64,
    pub dear_human: Option<i32>,
    pub pk_point: i32,
    pub pvp_flag: bool,
    pub pvp_name_color_tick: i64,
    pub die_in_fight3_zone: bool,
    // ---- S4b-1：状态位/生命周期/外观/类状态的只读投影 ----
    pub invisible: bool,
    pub hide_mode: bool,
    pub fixed_hide_mode: bool,
    pub ob_mode: bool,
    pub nasty_mode: bool,
    pub attack_mode: u8,
    pub in_safe_zone: bool,
    pub admin_mode: bool,
    pub stone_mode: bool,
    pub super_man: bool,
    pub rush_mode: bool,
    pub want_ref_msg: bool,
    pub stick_mode: bool,
    pub death: bool,
    pub ghost: bool,
    pub death_tick: i64,
    pub ghost_tick: i64,
    pub can_re_alive: bool,
    pub re_alive_tick: i64,
    pub death_settled: bool,
    pub die_called: bool,
    pub race: u8,
    pub race_img: u8,
    pub appr: u16,
    pub light: u8,
    pub name_color: u8,
    pub come_out_value: i32,
    pub attack_range: i32,
    pub use_magic: bool,
    pub danger_level: i32,
    pub n550: i32,
    pub class_attack_tick: i64,
    pub bo_is_first: bool,
    pub dig_down_tick: i64,
    pub appear_start_tick: i64,
    pub guard_direction: i8,
    pub siege_lock_count: i32,
    pub jump_time: i64,
    pub crazy_ready_mode: bool,
    pub crazy_king_mode: bool,
    pub crazy_count: i32,
    pub crazy_ready: i64,
    pub crazy_time: i64,
    pub old_hit_time: i64,
    pub old_walk_time: i64,
    // ---- S4b-2：玩家运行态/组队/复活/Buff 的只读投影 ----
    pub luck: i32,
    pub power_rate: i32,
    pub power_rate_time: i32,
    pub kill_mon_exp_rate: i32,
    pub kill_mon_exp_rate_time: i32,
    pub exp_rate_tick: i64,
    pub random_no: i32,
    pub bo_power_item: bool,
    pub power_item: u8,
    pub fire_hit_skill: bool,
    pub latest_fire_hit_tick: i64,
    pub twin_hit_skill: bool,
    pub latest_twin_hit_tick: i64,
    pub allow_group: bool,
    pub allow_group_re_call: bool,
    pub group_owner: i32,
    pub group_members: &'a Vec<i32>,
    pub revival: bool,
    pub revival_tick: i64,
    pub un_revival: bool,
    pub flame_ring: bool,
    pub poisoning_tick: i64,
    pub green_poisoning_point: u8,
    pub char_status: i32,
    pub char_status_ex: i32,
    pub status_time: [u16; STATUS_TIME_SLOTS],
    pub transparent: bool,
    pub show_hp: bool,
    pub show_hp_tick: i64,
    pub show_hp_interval: i32,
    pub magic_shield: bool,
    pub un_magic_shield: bool,
    pub extra_abil: [u16; EXTRA_ABIL_SLOTS],
    pub extra_abil_times: [i64; EXTRA_ABIL_SLOTS],
    pub extra_abil_flag: [u8; EXTRA_ABIL_SLOTS],
}

impl<'a> EntityRef<'a> {
    pub fn entity(&self) -> &'a Entity {
        self.entity
    }

    /// 由组件引用构造只读视图（`get` / `iter_ref` / bevy `OutputAoi` 系统用）。
    #[allow(clippy::too_many_arguments)]
    pub fn from_components(
        entity: &'a Entity,
        transform: &'a Transform,
        binding: &'a MapBinding,
        vitals: &'a Vitals,
        ability: &'a AbilitySnapshot<BaseAbil>,
        progression: &'a Progression,
        spawn: &'a SpawnBinding,
        drop: &'a DropState,
        attribution: &'a CombatAttribution,
        recovery: &'a Recovery,
        pvp: &'a PvpState,
        flags: &'a Flags,
        life_cycle: &'a LifeCycle,
        appearance: &'a Appearance,
        monster_class: &'a MonsterClassState,
        player_run: &'a PlayerRunState,
        group: &'a GroupState,
        revival: &'a Revival,
        buff: &'a BuffState,
    ) -> Self {
        EntityRef {
            entity,
            x: transform.x,
            y: transform.y,
            map_id: transform.map_id,
            view_range: transform.view_range,
            add_to_mapped: binding.add_to_mapped,
            visible_actors: &binding.visible_actors,
            visible_events: &binding.visible_events,
            is_visible_active: binding.is_visible_active,
            processed_tick: binding.processed_tick,
            verify_tick: binding.verify_tick,
            hp: vitals.hp,
            max_hp: vitals.max_hp,
            mp: vitals.mp,
            max_mp: vitals.max_mp,
            hit_speed: vitals.hit_speed,
            dc: vitals.dc,
            ac: vitals.ac,
            mac: vitals.mac,
            hit_point: vitals.hit_point,
            speed_point: vitals.speed_point,
            anti_magic: vitals.anti_magic,
            anti_poison: vitals.anti_poison,
            undead_power: vitals.undead_power,
            abil: ability.abil,
            job: progression.job,
            life_attrib: progression.life_attrib,
            level: progression.level,
            mon_exp: progression.mon_exp,
            exp: progression.exp,
            max_exp: progression.max_exp,
            castle_binding: spawn.castle_binding,
            mon_gen: spawn.mon_gen,
            mon_gen_zen_time: spawn.mon_gen_zen_time,
            ctor_draws: spawn.ctor_draws,
            gold: drop.gold,
            drop_items: &drop.items,
            angry_ring: drop.angry_ring,
            no_drop_use_item: drop.no_drop_use_item,
            no_item: drop.no_item,
            last_hiter: attribution.last_hiter,
            exp_hitter: attribution.exp_hitter,
            last_hiter_tick: attribution.last_hiter_tick,
            exp_hitter_tick: attribution.exp_hitter_tick,
            health_tick: recovery.health_tick,
            spell_tick: recovery.spell_tick,
            auto_recovery_tick: recovery.auto_recovery_tick,
            dear_human: pvp.dear_human,
            pk_point: pvp.pk_point,
            pvp_flag: pvp.pvp_flag,
            pvp_name_color_tick: pvp.pvp_name_color_tick,
            die_in_fight3_zone: pvp.die_in_fight3_zone,
            invisible: flags.invisible,
            hide_mode: flags.hide_mode,
            fixed_hide_mode: flags.fixed_hide_mode,
            ob_mode: flags.ob_mode,
            nasty_mode: flags.nasty_mode,
            attack_mode: flags.attack_mode,
            in_safe_zone: flags.in_safe_zone,
            admin_mode: flags.admin_mode,
            stone_mode: flags.stone_mode,
            super_man: flags.super_man,
            rush_mode: flags.rush_mode,
            want_ref_msg: flags.want_ref_msg,
            stick_mode: flags.stick_mode,
            death: life_cycle.death,
            ghost: life_cycle.ghost,
            death_tick: life_cycle.death_tick,
            ghost_tick: life_cycle.ghost_tick,
            can_re_alive: life_cycle.can_re_alive,
            re_alive_tick: life_cycle.re_alive_tick,
            death_settled: life_cycle.death_settled,
            die_called: life_cycle.die_called,
            race: appearance.race,
            race_img: appearance.race_img,
            appr: appearance.appr,
            light: appearance.light,
            name_color: appearance.name_color,
            come_out_value: monster_class.come_out_value,
            attack_range: monster_class.attack_range,
            use_magic: monster_class.use_magic,
            danger_level: monster_class.danger_level,
            n550: monster_class.n550,
            class_attack_tick: monster_class.class_attack_tick,
            bo_is_first: monster_class.bo_is_first,
            dig_down_tick: monster_class.dig_down_tick,
            appear_start_tick: monster_class.appear_start_tick,
            guard_direction: monster_class.guard_direction,
            siege_lock_count: monster_class.siege_lock_count,
            jump_time: monster_class.jump_time,
            crazy_ready_mode: monster_class.crazy_ready_mode,
            crazy_king_mode: monster_class.crazy_king_mode,
            crazy_count: monster_class.crazy_count,
            crazy_ready: monster_class.crazy_ready,
            crazy_time: monster_class.crazy_time,
            old_hit_time: monster_class.old_hit_time,
            old_walk_time: monster_class.old_walk_time,
            luck: player_run.luck,
            power_rate: player_run.power_rate,
            power_rate_time: player_run.power_rate_time,
            kill_mon_exp_rate: player_run.kill_mon_exp_rate,
            kill_mon_exp_rate_time: player_run.kill_mon_exp_rate_time,
            exp_rate_tick: player_run.exp_rate_tick,
            random_no: player_run.random_no,
            bo_power_item: player_run.bo_power_item,
            power_item: player_run.power_item,
            fire_hit_skill: player_run.fire_hit_skill,
            latest_fire_hit_tick: player_run.latest_fire_hit_tick,
            twin_hit_skill: player_run.twin_hit_skill,
            latest_twin_hit_tick: player_run.latest_twin_hit_tick,
            allow_group: group.allow_group,
            allow_group_re_call: group.allow_group_re_call,
            group_owner: group.group_owner,
            group_members: &group.group_members,
            revival: revival.revival,
            revival_tick: revival.revival_tick,
            un_revival: revival.un_revival,
            flame_ring: revival.flame_ring,
            poisoning_tick: buff.poisoning_tick,
            green_poisoning_point: buff.green_poisoning_point,
            char_status: buff.char_status,
            char_status_ex: buff.char_status_ex,
            status_time: buff.status_time,
            transparent: buff.transparent,
            show_hp: buff.show_hp,
            show_hp_tick: buff.show_hp_tick,
            show_hp_interval: buff.show_hp_interval,
            magic_shield: buff.magic_shield,
            un_magic_shield: buff.un_magic_shield,
            extra_abil: buff.extra_abil,
            extra_abil_times: buff.extra_abil_times,
            extra_abil_flag: buff.extra_abil_flag,
        }
    }
}

impl std::ops::Deref for EntityRef<'_> {
    type Target = Entity;
    fn deref(&self) -> &Entity {
        self.entity
    }
}

/// **可变视图**：一次按 id 取 `Entity` 主组件 + S4a 属性/成长组件。
///
/// S4a 把 20 个字段搬出 `Entity` 后，原来 `world.entities.get_mut(id)` 的块仍要
/// 同时读写「剩余字段」（经 `Deref`）与「迁出的属性」（`e.vitals.hp` 等）。视图在
/// **一次 `&mut` 借用**里取齐这些组件，因此不会出现「同一实体的两份可变真值」。
///
/// 它不是组件、不参与存储，**不是双真值**：`vitals`/`ability`/`progression` 都是组件
/// 的可变引用，写回即写给组件。Deref 目标仍是 `Entity`（其余字段照旧）。
pub struct EntityMut<'a> {
    entity: &'a mut Entity,
    pub vitals: &'a mut Vitals,
    pub ability: &'a mut AbilitySnapshot<BaseAbil>,
    pub progression: &'a mut Progression,
    pub spawn: &'a mut SpawnBinding,
    pub drop: &'a mut DropState,
    pub attribution: &'a mut CombatAttribution,
    // ---- S4b-1 ----
    pub flags: &'a mut Flags,
    pub life_cycle: &'a mut LifeCycle,
    pub appearance: &'a mut Appearance,
    pub monster_class: &'a mut MonsterClassState,
    // ---- S4b-2 ----
    pub player_run: &'a mut PlayerRunState,
    pub group: &'a mut GroupState,
    pub revival: &'a mut Revival,
    pub buff: &'a mut BuffState,
}

impl EntityMut<'_> {
    /// 只取主组件（剩余字段）的可变引用（不碰 S4a 属性组件）。
    pub fn entity_mut(&mut self) -> &mut Entity {
        self.entity
    }

    /// C# `BaseObject.SetLastHiter`（`BaseObject.cs:1753`）。
    ///
    /// `LastHiter` 每次都刷新并续 `LastHiterTick`；`ExpHitter` **只在为空时**才认领
    /// （第一个命中者粘住），之后只有同一个命中者再次命中才会续 `ExpHitterTick`
    /// （`BaseObject.cs:1753-1770` 逐支照抄；30s/6s 过期在 `World::expire_hitters`）。
    pub fn set_last_hiter(&mut self, hitter: i32, now: i64) {
        self.attribution.last_hiter = Some(hitter);
        self.attribution.last_hiter_tick = now;
        if self.attribution.exp_hitter.is_none() {
            self.attribution.exp_hitter = Some(hitter);
            self.attribution.exp_hitter_tick = now;
        } else if self.attribution.exp_hitter == Some(hitter) {
            self.attribution.exp_hitter_tick = now;
        }
    }

    /// `BaseObject.Run` 的 `SuperMan` 回满段（`BaseObject.Base.cs:32-36`）：
    /// `if (SuperMan) { WAbil.HP = MaxHP; WAbil.MP = MaxMP; }`。
    ///
    /// 位置契约：在消息泵之后、`if (!Death)` 之前 ⇒ 调用方必须放在自己的 `Run` 消息段
    /// （含 `RM_STRUCK` 出队）之后、死亡/恢复支之前。`SuperMan` 实体因此在 `HP == 0`
    /// 判定（`:70-73`）之前就被回满 ⇒ oracle 的 `Die()` 对它们**不可达**；`Die` 的早退
    /// 语义（`:318-322`）由 `settle_deaths` 在「直接调用 `Die()`」路径等价。
    pub fn run_super_man_refill(&mut self) {
        if self.flags.super_man {
            self.vitals.hp = self.vitals.max_hp;
            self.vitals.mp = self.vitals.max_mp;
        }
    }

    /// 取 `Abil` 快照；未取过时用当前 `dc`/`max_hp` 建立（生成路径 `WAbil == Abil`）。
    ///
    /// 生产路径**唯一**的 `Abil` 写入点是 `monster_from_row` 的库值（`dc`/`max_hp`）；
    /// `MakeClone` 的 `ElfMon.Abil = OldMon.Abil` 由 [`crate::monai`] 直接复制。
    pub fn abil_snapshot(&mut self) -> BaseAbil {
        if let Some(abil) = self.ability.abil {
            return abil;
        }
        let abil = BaseAbil {
            dc: self.vitals.dc,
            max_hp: self.vitals.max_hp,
        };
        self.ability.abil = Some(abil);
        abil
    }
}

impl std::ops::Deref for EntityMut<'_> {
    type Target = Entity;
    fn deref(&self) -> &Entity {
        self.entity
    }
}

/// **只读「基础实体 + PK 状态」视图**：战斗目标判定链（`combat::is_proper_target*` /
/// `is_attack_target_player*` / `is_proper_friend*`）用。
///
/// S4a 把 `PvpState` 搬出 `Entity` 后，这些判定函数既需要 `Entity` 的 `race`/`attack_mode`/
/// `group_owner` 等字段，也需要 PK 字段。`Entity` 自身没有 pvp 了，因此用一个两方法视图：
/// `base()` 给基础字段、`pvp_state()` 给 PK 快照（按值拷贝；调用点在判定期间不写 pvp）。
pub trait PvpView {
    fn base(&self) -> &Entity;
    fn pvp_state(&self) -> PvpState;
    /// S4b-1：行为轴 `race`（外观组件）。
    fn race(&self) -> u8;
    /// S4b-1：死亡位（生命周期组件）。
    fn death(&self) -> bool;
    /// S4b-1：幽灵位（生命周期组件）。
    fn ghost(&self) -> bool;
    /// S4b-1：隐身位（状态位组件）。
    fn invisible(&self) -> bool;
    /// S4b-1：攻击模式（状态位组件）。
    fn attack_mode(&self) -> u8;
    /// S4b-1：完整状态位快照（安全区/admin/石化/nasty 等）。
    fn flags(&self) -> Flags;
    /// S4b-1：名称颜色（外观组件）。
    fn name_color(&self) -> u8;
    /// S4b-2：玩家运行态快照。
    fn player_run(&self) -> PlayerRunState;
    /// S4b-2：组队实体侧快照。
    fn group_state(&self) -> GroupState;
    /// S4b-2：Buff/毒/盾快照。
    fn buff(&self) -> BuffState;
}

impl PvpView for EntitySpawn {
    fn base(&self) -> &Entity {
        &self.entity
    }
    fn pvp_state(&self) -> PvpState {
        PvpState {
            dear_human: self.dear_human,
            pk_point: self.pk_point,
            pvp_flag: self.pvp_flag,
            pvp_name_color_tick: self.pvp_name_color_tick,
            die_in_fight3_zone: self.die_in_fight3_zone,
        }
    }
    fn race(&self) -> u8 {
        self.race
    }
    fn death(&self) -> bool {
        self.death
    }
    fn ghost(&self) -> bool {
        self.ghost
    }
    fn invisible(&self) -> bool {
        self.invisible
    }
    fn attack_mode(&self) -> u8 {
        self.attack_mode
    }
    fn flags(&self) -> Flags {
        self.flags_component()
    }
    fn name_color(&self) -> u8 {
        self.name_color
    }
    fn player_run(&self) -> PlayerRunState {
        self.player_run_component()
    }
    fn group_state(&self) -> GroupState {
        self.group_component()
    }
    fn buff(&self) -> BuffState {
        self.buff_component()
    }
}

impl PvpView for EntityRef<'_> {
    fn base(&self) -> &Entity {
        self.entity
    }
    fn pvp_state(&self) -> PvpState {
        PvpState {
            dear_human: self.dear_human,
            pk_point: self.pk_point,
            pvp_flag: self.pvp_flag,
            pvp_name_color_tick: self.pvp_name_color_tick,
            die_in_fight3_zone: self.die_in_fight3_zone,
        }
    }
    fn race(&self) -> u8 {
        self.race
    }
    fn death(&self) -> bool {
        self.death
    }
    fn ghost(&self) -> bool {
        self.ghost
    }
    fn invisible(&self) -> bool {
        self.invisible
    }
    fn attack_mode(&self) -> u8 {
        self.attack_mode
    }
    fn flags(&self) -> Flags {
        Flags {
            invisible: self.invisible,
            hide_mode: self.hide_mode,
            fixed_hide_mode: self.fixed_hide_mode,
            ob_mode: self.ob_mode,
            nasty_mode: self.nasty_mode,
            attack_mode: self.attack_mode,
            in_safe_zone: self.in_safe_zone,
            admin_mode: self.admin_mode,
            stone_mode: self.stone_mode,
            super_man: self.super_man,
            rush_mode: self.rush_mode,
            want_ref_msg: self.want_ref_msg,
            stick_mode: self.stick_mode,
        }
    }
    fn name_color(&self) -> u8 {
        self.name_color
    }
    fn player_run(&self) -> PlayerRunState {
        PlayerRunState {
            luck: self.luck,
            power_rate: self.power_rate,
            power_rate_time: self.power_rate_time,
            kill_mon_exp_rate: self.kill_mon_exp_rate,
            kill_mon_exp_rate_time: self.kill_mon_exp_rate_time,
            exp_rate_tick: self.exp_rate_tick,
            random_no: self.random_no,
            bo_power_item: self.bo_power_item,
            power_item: self.power_item,
            fire_hit_skill: self.fire_hit_skill,
            latest_fire_hit_tick: self.latest_fire_hit_tick,
            twin_hit_skill: self.twin_hit_skill,
            latest_twin_hit_tick: self.latest_twin_hit_tick,
        }
    }
    fn group_state(&self) -> GroupState {
        GroupState {
            allow_group: self.allow_group,
            allow_group_re_call: self.allow_group_re_call,
            group_owner: self.group_owner,
            group_members: self.group_members.clone(),
        }
    }
    fn buff(&self) -> BuffState {
        BuffState {
            poisoning_tick: self.poisoning_tick,
            green_poisoning_point: self.green_poisoning_point,
            char_status: self.char_status,
            char_status_ex: self.char_status_ex,
            status_time: self.status_time,
            transparent: self.transparent,
            show_hp: self.show_hp,
            show_hp_tick: self.show_hp_tick,
            show_hp_interval: self.show_hp_interval,
            magic_shield: self.magic_shield,
            un_magic_shield: self.un_magic_shield,
            extra_abil: self.extra_abil,
            extra_abil_times: self.extra_abil_times,
            extra_abil_flag: self.extra_abil_flag,
        }
    }
}

impl<T: PvpView + ?Sized> PvpView for &T {
    fn base(&self) -> &Entity {
        (*self).base()
    }
    fn pvp_state(&self) -> PvpState {
        (*self).pvp_state()
    }
    fn race(&self) -> u8 {
        (*self).race()
    }
    fn death(&self) -> bool {
        (*self).death()
    }
    fn ghost(&self) -> bool {
        (*self).ghost()
    }
    fn invisible(&self) -> bool {
        (*self).invisible()
    }
    fn attack_mode(&self) -> u8 {
        (*self).attack_mode()
    }
    fn flags(&self) -> Flags {
        (*self).flags()
    }
    fn name_color(&self) -> u8 {
        (*self).name_color()
    }
    fn player_run(&self) -> PlayerRunState {
        (*self).player_run()
    }
    fn group_state(&self) -> GroupState {
        (*self).group_state()
    }
    fn buff(&self) -> BuffState {
        (*self).buff()
    }
}

impl std::ops::DerefMut for EntityMut<'_> {
    fn deref_mut(&mut self) -> &mut Entity {
        self.entity
    }
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
    // S3b：`x` / `y` / `view_range` / `map_id` 已迁到 `mir2_world_bevy::Transform` 组件
    //（字段真值，经 `EntityStore::transform` / `transform_mut` 受控访问）。
    /// C# `Race` —— **行为轴**：所有 `IsAttackTarget`/`Struck`/掉落等判定读它。
    ///
    /// 与 [`Entity::race_img`]（外观轴）**不是同一字段**：oracle 商人 `Race = ActorRace.NPC(10)`
    /// （`NormNpc.cs:42` 继承）而 `RaceImg = ActorRace.Merchant(50)`（`Merchant.cs:84`）；两者
    /// 数值在 `Merchant`/`Animal` 上偶合（都是 50），不得混用。
    /// C# `BaseObject.RaceImg`（`BaseObject.cs:101`；ctor `:380` 置 0）——**外观轴**：
    /// 客户端可见帧的形像编号，`GetFeature()` = `MakeMonsterFeature(RaceImg, MonsterWeapon, Appr)`
    /// （`BaseObject.Base.cs:911`）。
    ///
    /// 商人置 `ActorRace.Merchant(50)`（`Merchant.cs:84`），但其行为轴 [`Entity::race`] 仍是
    /// `ActorRace.NPC(10)`（`NormNpc.cs:42`）⇒ **不得**把本字段当 `race` 用（反之亦然）。
    /// C# `BaseObject.Appr`（形像代码）。`ApplyMonsterAbility` 从
    /// `MonsterInfo.Appr` 写入（`WorldServer.MonGen.cs:889`）。
    ///
    /// 唯一的**行为**消费点 = `ElfWarriorMonster.AttackTarget` 的
    /// `Appr == 704/706/708` 形态门（`ElfWarriorMonster.cs:78`）；其余只影响展示层。
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
    /// C# `BeeQueen.BeeList` / `SpiderHouseMonster._bbList`（`BeeQueen.cs:11` /
    /// `SpiderHouseMonster.cs:11`）——产出的宝宝 actor id 列表（**插入序**）。
    ///
    /// oracle 是 `IList<IActor>` 强引用；本骨架存 id，实体已不在仓库时按「已不可达」
    /// 处理（清理段等价于 `Death || Ghost`）。
    pub special_children: Vec<i32>,
    /// C# `AnimalObject.IsSlave`（`AnimalObject.cs:54`）。
    pub is_slave: bool,
    /// **卡 `openmir2-aoi-disappear-observer-scope`（tier=R）**：是否 `PlayCloneObject`
    /// （`MagMakeClone` 造出的玩家型克隆体，`PlayCloneObject.cs:5-23`）。
    ///
    /// 克隆体 `race == PLAY`、`!ghost`，但**没有真实客户端会话**（不进 `OnlineRegistry`）
    /// ⇒ AOI 消失帧的观察者分类必须把它与真玩家区分（否则 `missing_online` 假阳性）。
    /// 纯内存字段：不落库、不进 `EntitySnapshot`、不改协议；随实体移除一起消失。
    pub is_clone: bool,
    /// C# `AnimalObject.SlaveExpLevel`（`AnimalObject.cs:27`）——宝宝等级（召唤时/跨服时设定；
    /// `MonsterObject.GainSlaveExp` 是死代码，击杀不涨，见盘点报告 §1.C）。
    pub slave_exp_level: u8,
    /// C# `MonsterObject.SlaveMakeLevel`（`MonsterObject.cs:21`）——召唤时的技能等级。
    pub slave_make_level: u8,
    /// C# `MonsterObject.MasterRoyaltyTick`（`MonsterObject.cs:37`）——叛变时刻。
    pub master_royalty_tick: i64,
    /// C# `MonsterObject.MasterTick`（`MonsterObject.cs:41`）——诱惑到期的起算时刻。
    pub master_tick: i64,
    /// C# `BaseObject.DeathTick`（`BaseObject.cs:204`，`Die()` 在 `:323` 写 `GetTickCount()`）——
    /// 死亡时刻（毫秒）。`BaseObject.Run` 死亡支的 `now - DeathTick` 门读它
    /// （`BaseObject.Base.cs:90/99`）。
    /// C# `BaseObject.GhostTick`（`BaseObject.cs:207`）——`MakeGhost()` 写（`BaseObject.Base.cs:494`）；
    /// 幽灵怪 5 分钟清理门 `(now - GhostTick) > 5*60*1000`（`WorldServer.MonGen.cs:302`）。
    /// C# `BaseObject.CanReAlive`（`BaseObject.cs:220`）——`RegenMonsters` 刷出的怪置 `true`
    /// （`WorldServer.MonGen.cs:832/854`）；换图/魔法清位（`BaseObject.cs:2749-2756`、
    /// `MagicManager.cs:743-745`）。决定 `MakeGhost` 走「留尸体可复活」还是「真 Ghost」。
    /// C# `BaseObject.ReAliveTick`（`BaseObject.cs:224`）——复活冷却起点
    /// （`WorldServer.MonGen.cs:833/855` 置创建时刻、`:290` 在复活成功后刷新）。
    // **Lane B（城堡实体挂图）**：本实体对应的城堡防御单元（`BaseObject.Castle != null` 的等价物）。
    //
    // 只有 `UserCastle.Initialize` 成功挂图的**门 / 墙 / 弓手**被绑定（守卫不绑，见
    // [`CastleBinding`]）。race 110/111 的 `Run` 与死亡路由靠它反查
    // `World.castles[castle_index]` 的状态机。
    // S4a-2 迁出：`castle_binding` 真值在 components 组件（见迁移文档 S4a）。
    // C# `BaseObject.MonGen`（`BaseObject.cs` 属性）——本实体所属 `MonGenInfo` 在
    // [`crate::mongen::MonGenState::entries`] 里的下标；`None` = oracle `MonGen == null`。
    // S4a-2 迁出：`mon_gen` 真值在 components 组件（见迁移文档 S4a）。
    // C# `MonGen.ZenTime` 的缓存（死亡支 `GetMonstersZenTime(MonGen.ZenTime)`，
    // `BaseObject.Base.cs:87`）。仅 [`Entity::mon_gen`] 为 `Some` 时有效。
    // 缓存不改变语义：`MonGenInfo.ZenTime` 是配置列，运行期不变。
    // S4a-2 迁出：`mon_gen_zen_time` 真值在 components 组件（见迁移文档 S4a）。
    /// C# `BaseObject.HideMode`（隐身术/隐身戒指的"完全隐身"）。
    ///
    /// 与 [`Entity::invisible`]（oracle `Invisible`，`is_proper_target` 用）和
    /// [`Entity::fixed_hide_mode`]（oracle `FixedHideMode`）是**三个不同字段**：
    /// oracle `AnimalObject.SearchTarget` 的隐藏门读的是 `HideMode`（`AnimalObject.cs:402`），
    /// 只有 `Envirnoment.GetXYObjCount` / `BaseObject.Run` 才读 `FixedHideMode`。
    /// 当前本仓无写入点（隐身术/隐身戒指未接线）⇒ 两侧行为今天相同；接线后在 `search_target`
    /// 的隐藏门上与 oracle 1:1。
    /// C# `NastyMode`（PK 模式等）
    /// C# `PlayObject.AttatckMode`（`AttackMode` 枚举，`src/OpenMir2/Enums/AttackMode.cs`）：
    /// `HAM_ALL=0 / HAM_PEACE=1 / HAM_DEAR=2 / HAM_MASTER=3 / HAM_GROUP=4 / HAM_GUILD=5 / HAM_PKATTACK=6`。
    /// 新角色 `PlayObject.cs:908` 置 `0`（`HAM_ALL`）；`ChangeAttackModeCommand.cs:26` 按 Ctrl-H 循环。
    /// C# `BaseObject.InSafeZone()`（`BaseObject.cs:1777`）的结果折叠。
    ///
    /// oracle 的 `IsProtectTarget`（`BaseObject.Base.cs:360`）用它做 PvP 门槛；本骨架未建模
    /// 地图 Flag/StartPoint 安全区。**生产代码当前无写入点（该字段恒 `false`），仅测试夹具赋值**；
    /// 若将来接线层在入图/换图时按 `Envir` 写入，须同步 whitelist T-75。
    /// C# `BaseObject.AdminMode`（`BaseObject.IsAttackTarget` 首判：管理员不可被攻击）。
    /// C# `BaseObject.StoneMode`（同上，石化不可被攻击）。
    // C# `PlayObject.DearHuman`（`PlayObject.IsAttackTarget` 的 `HAM_DEAR` 支：只保护配偶）。
    // S4a-2 迁出：`dear_human` 真值在 components 组件（见迁移文档 S4a）。
    // C# `PlayObject.PkPoint`——`PlayObject.PvpLevel()`（`PlayObject.cs:4352`）= `PkPoint / 100`。
    // `HAM_PKATTACK` 支用它做红名/白名判定。
    // S4a-2 迁出：`pk_point` 真值在 components 组件（见迁移文档 S4a）。
    // C# `PlayObject.PvpFlag`（`PlayObject.cs:4336`）——PK 标记；`SetPkFlag` 置位并触发
    // `RefNameColor`（⇒ `SM_CHANGENAMECOLOR(656)`）。ctor 缺省 `false`。
    //
    // 消费方（P3-B4）：50 的 `playObject.SetPkFlag(targetObject)`（`MagicManager.cs:1249`）与
    // `RM_POISON` 投递的 `((IPlayerActor)this).SetPkFlag(caster)`（`BaseObject.Base.cs:749`）。
    // S4a-2 迁出：`pvp_flag` 真值在 components 组件（见迁移文档 S4a）。
    // C# `PlayObject.PvpNameColorTick`（`PlayObject.cs:243`；接口声明 `IPlayerActor.cs:257`）——`SetPkFlag` 置位时刻
    // （`PlayObject.cs:4333` 的 `targetObject.PvpNameColorTick = GetTickCount()`）。
    // `CheckPkStatus`（`:4359`）用 `now - PvpNameColorTick > Config.dwPKFlagTime` 判过期。
    //
    // **重复置位也刷新**（oracle 赋值在 `if (!targetObject.PvpFlag)` **之前**）。
    // S4a-2 迁出：`pvp_name_color_tick` 真值在 components 组件（见迁移文档 S4a）。
    // C# `PlayObject.DieInFight3Zone`（`PlayObject.cs:522`）：`WorldServer.cs:290` 在
    // Fight3Zone 死亡恢复支置 `true`，`Initialize()`（`PlayObject.cs:891`）清 `false`。
    //
    // 唯一消费点是登录后的 `if (DieInFight3Zone) MapRandomMove(...)`（`PlayObject.cs:1184`）；
    // 该调用点未移植 ⇒ 本骨架只忠实保存该位（登录时由 `rewrite_enter_position` 写回）。
    // S4a-2 迁出：`die_in_fight3_zone` 真值在 components 组件（见迁移文档 S4a）。
    /// C# `PlayObject.Luck`（`PlayObject.cs:62`，`RecalcAbilitys` 里 `Luck = 0 + AddAbil.Luck - AddAbil.UnLuck`）。
    /// 玩家攻方的 [`crate::combat::get_attack_power_player`] override 用它决定取数次数。
    /// C# `BaseObject.NameColor`（`BaseObject.cs:28/385`，ctor 初值 `255`）。
    ///
    /// `GetNameColor()`（`BaseObject.Base.cs:904-907`）返回它；`GetChrColor`
    /// （`BaseObject.Base.cs:869-883`）的兜底分支与 `MonsterObject`/`PlayObject` 的 override 都读它。
    /// 归 `openmir2-m3-pet-slave-lifecycle`（⑨-3）。
    /// C# `PlayObject.PowerRate`（`PlayObject.cs:724`；新角色 `PlayObject.cs:943` 置 `100`）。
    /// C# `PlayObject.PowerRateTime`（`PlayObject.cs:726`；**无 ctor 赋值** ⇒ 0）——
    /// P27 的 1s 递减计时（`PlayObject.Message.cs:646-656`）。`0` = 永久倍率。
    /// C# `PlayObject.KillMonExpRate`（`PlayObject.cs:720`；ctor `:941` 置 `100`）——
    /// **per-player** 经验倍率；唯一读点 = `WinExp`（`PlayObject.Base.cs:140`）。
    /// PL-3/P27：1s 递减到 0 时回 100；脚本 `KILLMONEXPRATE` 写入。
    /// C# `PlayObject.KillMonExpRateTime`（`PlayObject.cs:725`；无 ctor ⇒ 0）——P27 的秒数。
    /// C# `PlayObject.ExpRateTick`（`PlayObject.cs:727`；ctor `:942` = `GetTickCount()`）——
    /// P27 的 1s 门基准。oracle 在对象构造时取钟；本骨架 `Entity::new` 拿不到宿主时钟
    /// ⇒ 缺省 0，生产路径由 `new_player_entity` 用 `now_ms()` 重盖（与 `VerifyTick` 同法）。
    /// C# `PlayObject.RandomNo`（`PlayObject.cs:815`；ctor `:1047`
    /// `RandomNo = Random(999999).ToString()`）——登录批 `new PlayObject()` 的**第 5 抽**。
    ///
    /// 消费面（oracle）：脚本变量 `<$RANDOMNO>`（`NormNpc.cs:439`）、`SETRANDOMNO`
    /// （`ExecutionProcessingSys.cs:2112-2119`）、`CHECKRANDOMNO`（`ScriptEngine.cs:350-353`）。
    /// 本仓脚本层尚未接线（`crates/script` 的 `CHECKRANDOMNO` 仍是 TODO）⇒ 字段先按 oracle
    /// 落值，接线后可直接消费；oracle 存的是十进制字符串，本字段存数值（`to_string()` 同形）。
    /// C# `PlayObject.BoPowerItem`（`PlayObject.cs:223`）。
    /// C# `PlayObject.PowerItem`（`PlayObject.cs:199`）。
    /// C# `PlayObject.FireHitSkill` / `LatestFireHitTick`（`PlayObject.cs:181/194`）——
    /// 烈火剑法待发标志与最近一次置位时刻。`PlayObject.Run` 的到期段
    /// （`PlayObject.Message.cs:54-58`）严格 `(now - LatestFireHitTick) > 20*1000` ⇒ 清标志 +
    /// `SpiritsGone` + `SendSocket("+UFIR")`（[`crate::World::expire_player_run_state`]）。
    ///
    /// **写入点登记**：oracle 的 `AllowFireHitSkill`（`PlayObject.Base.cs:653-663`，施放 26 时置位）
    /// 与攻击命中清标志（`PlayObject.Attack.cs:591-593/614-616`）依赖尚未移植的技能层；
    /// 本骨架的生产路径暂无写入者（恒 `false`）⇒ 到期段零动作。字段/到期先落地，接线时无需再改。
    /// C# `PlayObject.TwinHitSkill` / `LatestTwinHitTick`（`PlayObject.cs:185/195`）——双龙斩。
    /// 到期段 `PlayObject.Message.cs:60-64`：严格 `> 60*1000` ⇒ 清标志 + `SendSocket("+UTWN")`
    /// （**无** `SysMsg`）。写入点同 [`Entity::fire_hit_skill`] 登记（技能层未移植）。
    /// C# `CharacterObject.UseItems`（`CharacterObject.cs:40`，13 格，`CharacterObject.cs:89` 初始化）。
    /// `PlayObject.StruckDamage` 的耐久循环按它取数。
    pub use_items: [UseItemSlot; USE_ITEM_SLOTS],
    // C# `PlayObject.AngryRing`（`PlayObject.cs:2018` 初值 `false`；`RecalcAbilitys` 的
    // `RING_ANGERENERGY_ITEM(117)` 支置真）——`DropUseItems` 的**首门**
    // （`PlayObject.Message.cs:2156`：`if (AngryRing || NoDropUseItem) return;`）。
    //
    // **写入点登记**：本骨架的 `RecalcAbilitys` 等价物（`bins/game-svr/src/ability.rs`）尚未
    // 建模戒指 Shape 117 ⇒ 生产路径恒 `false`（部署件语料下与 oracle 的差异见 whitelist T-81）。
    // S4a-2 迁出：`angry_ring` 真值在 components 组件（见迁移文档 S4a）。
    // C# `PlayObject.NoDropUseItem`（脚本/`RobotPlayer.Conf` 可置；真人玩家默认 `false`）。
    // 同 `DropUseItems` 首门。本骨架无写者 ⇒ 恒 `false`。
    // S4a-2 迁出：`no_drop_use_item` 真值在 components 组件（见迁移文档 S4a）。
    // C# `BaseObject.NoItem`（`IPlayerActor.NoDropItem` 的旧名；脚本可置，真人玩家默认
    // `false`）——`KillTargetTrigger` 第二段 `!NoItem || !Envir.Flag.NoDropItem` 的实体侧门。
    // 本骨架无写者 ⇒ 恒 `false`。
    // S4a-2 迁出：`no_item` 真值在 components 组件（见迁移文档 S4a）。
    /// C# `WantRefMsg`
    // S3b：`add_to_mapped` / `visible_actors` / `visible_events` / `is_visible_active` /
    // `processed_tick` / `verify_tick` 已迁到 [`MapBinding`] 组件（字段真值，经
    // `EntityStore::map_binding` / `map_binding_mut` 受控访问）。`processed_tick` 与
    // `map_id` 的旧字段位置保留在此注释里，源码顺序无契约。
    // `WAbil.HP` / `WAbil.MaxHP`
    // S4a 迁出：`hp` 真值在 components 组件（见迁移文档 S4a）。
    // S4a 迁出：`max_hp` 真值在 components 组件（见迁移文档 S4a）。
    // C# `BaseObject.Abil` 的本骨架子集快照（见 [`BaseAbil`]）。
    //
    // `None` = 尚未取快照：生成路径（`monster_from_row`）直接把库值写进 `dc`/`max_hp`
    // （此时 `WAbil == Abil`，此后未被 `RecalcAbilitys`/伤害改过）⇒ 首次需要时由
    // [`Entity::abil_snapshot`] 以当前 `dc`/`max_hp` 懒取。`MakeClone` 在第一次
    // `RecalcAbilitys()` 之后复制源怪的快照（oracle `MonsterObject.cs:115`），
    // 使随后的 `AppearNow` 从 `OldMon.Abil` 重算（`f(Abil)` 而不是 `f(f(初值))`）。
    // S4a 迁出：`abil` 真值在 components 组件（见迁移文档 S4a）。
    // `WAbil.MP` / `WAbil.MaxMP`。
    //
    // 怪物生成时照 `WorldServer.MonGen.cs:896-897` 的 `ApplyMonsterAbility`：`MP = 0`、
    // `MaxMP = MonsterInfo.MP`（注意 `MP` 在怪物表里被复用作 `MonsterWeapon` 的形参，见
    // `:903`；`BaseObject.Run` 的恢复段仍按 `WAbil.MaxMP` 回蓝）。
    // S4a 迁出：`mp` 真值在 components 组件（见迁移文档 S4a）。
    // S4a 迁出：`max_mp` 真值在 components 组件（见迁移文档 S4a）。
    /// C# `BaseObject.PoisoningTick`（`BaseObject.cs:301`，ctor `:443` 置 `GetTickCount()`）——
    /// 毒伤 DoT 的周期门时刻（`ActorBuffSystem.cs:235`，缺省 `Config.PosionDecHealthTime` = 2500ms）。
    /// 缺省 = [`crate::buff::BUFF_EPOCH_MS`]（模拟开机毫秒基）；ctor 拿不到 `World` 时钟 ⇒ 落图时由
    /// [`crate::world::World::add_entity`] 按 `clock_ms + BUFF_EPOCH_MS` 精确重盖，与 oracle ctor
    /// 「对象建立那一刻」取 `GetTickCount()` 对齐。
    /// C# `BaseObject.GreenPoisoningPoint`（`BaseObject.cs:117`，`MakePosion` `:1963` 置 `(byte)nPoint`）。
    /// DoT 扣血量 = `GreenPoisoningPoint + 1`（`ActorBuffSystem.cs:252`）。
    /// C# `BaseObject.CharStatus`（`BaseObject.cs:56`）——`GetCharStatus()` 的结果缓存
    /// （`ActorBuffSystem.Operate` 的 `boChg` 支 / `MakePosion` 写它）。
    /// C# `BaseObject.CharStatusEx`（`BaseObject.cs:57`，ctor `:393` 置 0）——低 16 位并入 `GetCharStatus`。
    /// C# `BaseObject.StatusTimeArr`（`BaseObject.cs:81`，ctor `:394` 置 15 个 0）。
    ///
    /// **P3-B3 新增（`crates/world` 接口变更，须按 §12.4 通知并行车道）**：
    /// 18/19 的隐身门、40 的三个计时器读写、以及 16/18/19 之外的写点（移动破隐 / 透明戒指 /
    /// `Disappear` / `GasMoth` / `FrostTiger` / `Abil`）都落在这一份存储上。
    /// 递减循环被注释（`BaseObject.Base.cs:249-315`）⇒ 只有写入点，没有周期衰减。
    /// C# `BaseObject.Transparent`（`BaseObject.cs:259`；18/19 的 `MagMakePrivateTransparent` 收尾置 true）。
    /// C# `BaseObject.ShowHp`（28 心灵启示的 `MakeOpenHealth` 置 true）。
    /// C# `BaseObject.ShowHpTick`（`MagicManager.cs:436` 置当前 tick）。
    /// C# `BaseObject.ShowHpInterval`（`MagicManager.cs:437`：`GetPower13(...) * 1000`）。
    /// C# `BaseObject.SuperMan`（32 圣言术的首门 `target.SuperMan`）。
    // C# `CharacterObject.HitSpeed`（`PlayObject.Message.cs:1846` 的 `SM_CHARSTATUSCHANGED` 末参
    // 取**收件人自己**的 `this.HitSpeed`）。
    //
    // **P3-B3 新增**：`RecalcAbilitys` 的写入点尚未接到实体（登记），普通路径恒 0；
    // 657 帧改为逐收件人读本字段构造（不再用一条 `hit_speed=0` 的 body 广播）。
    // S4a 迁出：`hit_speed` 真值在 components 组件（见迁移文档 S4a）。
    /// C# `PlayObject.MagicShield`（`PlayObject.cs:490`）——`DamageHealth` 的 MP 抵扣门（`BaseObject.cs:779`）。
    /// C# `PlayObject.UnMagicShield`（`PlayObject.cs:494`）——`DamageHealth` 首行守卫（`BaseObject.cs:777`）。
    // `WAbil.DC`（低字节=下限、高字节=上限，与 C# 一致）
    // S4a 迁出：`dc` 真值在 components 组件（见迁移文档 S4a）。
    // `WAbil.AC`
    // S4a 迁出：`ac` 真值在 components 组件（见迁移文档 S4a）。
    // `HitPoint` / `SpeedPoint`（命中/敏捷，命中判定用）
    // S4a 迁出：`hit_point` 真值在 components 组件（见迁移文档 S4a）。
    // S4a 迁出：`speed_point` 真值在 components 组件（见迁移文档 S4a）。
    // C# `BaseObject.AntiMagic`（`BaseObject.cs:113`；ctor `:390` 置 **0**）——
    // 魔法骰门 `Random(10) >= AntiMagic` 的右操作数（`MagicManager.cs:180/292/334/566/1033`）。
    //
    // **普通 `MonGen` 生成路径保持 0**：oracle 只有 `RecalcAbilitys` 会把它置成
    // `1 + AddAbil.AntiMagic`（`BaseObject.Abil.cs:30` 置 1、`:67` 累加），而普通生成的怪
    // **不调用** `RecalcAbilitys`；`ElfMonster`/`WhiteSkeleton` 等覆写也不改 `AntiMagic`。
    // ⇒ 普通怪目标下 `Random(10) >= 0` 恒真，骰门永不失败。
    //
    // 本仓的等价 `RecalcAbilitys` 写入点：玩家登录/升级
    // （`bins/game-svr` 的 `apply_ability_to_entity`）与宝宝召唤
    // （[`crate::World::make_slave_adopt`]，`BaseObject.cs:1198`），均置 `1 + AddAbil.AntiMagic`
    // （怪物/玩家的 `AddAbil` 在本骨架内为 0 ⇒ 1）。复活（`BaseObject.cs:2739`）与其余
    // slave 面（`MakeClone`/`GainSlaveExp`）尚未建模 ⇒ 这些路径保持 0（登记）。
    // 设计锚：`mir2-rs/tests/parity/m3/design-monster-run-overrides-20261017.md` §4.1.2-5 / §5-2。
    // S4a 迁出：`anti_magic` 真值在 components 组件（见迁移文档 S4a）。
    /// C# `BaseObject.Light`（`BaseObject.cs:149`；ctor `:384` 置 **0**）——光源强度。
    ///
    /// 消费点全在帧面：`SM_WALK/SM_RUN/SM_TURN/SM_DIGUP/...` 的 `Series = MakeWord(Dir, Light)`
    /// （`PlayObject.Message.cs:1406-1975`）。本骨架只落字段，帧面由宿主 `bins/game-svr` 发。
    /// `NormNpc` ctor 置 **2**（`NormNpc.cs:43`，商人继承）；玩家/普通怪物保持 ctor 缺省 0。
    // C# `BaseObject.AntiPoison`（`BaseObject.cs:109`；ctor `:389` 置 **0**）——毒/麻痹抗性。
    //
    // 消费点：`Random(target.AntiPoison + N)` 的毒/麻痹命中门（`MagicManager.cs:235/914`、
    // `Player.Attack.cs:221`、`GreenMonster.cs:23` 等）。`NormNpc` ctor 置 **99**
    // （`NormNpc.cs:44`，商人继承）；玩家/普通怪物保持 ctor 缺省 0（本骨架尚未接毒/麻痹结算）。
    // S4a 迁出：`anti_poison` 真值在 components 组件（见迁移文档 S4a）。
    // `WAbil.MAC`（`BaseObject` 构造 ctor 缺省 **0**；`RecalcAbilitys` 后 = `Abil.MAC + AddAbil.MAC`
    // 的逐字节 `MakeWord` 合成，见 `bins/game-svr` 的 `apply_ability_to_entity`）。
    //
    // **P3-B2 新增（`crates/world` 接口变更，须按 §12.4 通知并行车道）**：`RM_MAGSTRUCK` 交付段的
    // `GetMagStruckDamage`（`BaseObject.Base.cs:812-821`）唯一读点：
    // `Lo(MAC) + Random(|Hi(MAC)-Lo(MAC)|+1)`，再从 `nDamage` 里扣。
    // 普通 `MonGen` 怪走 ctor 缺省 0（与 `anti_magic` 同口径，生成路径不调 `RecalcAbilitys`）。
    // S4a 迁出：`mac` 真值在 components 组件（见迁移文档 S4a）。
    /// C# `BaseObject.RushMode`——`RM_MAGSTRUCK` 的 `Random(1000)` 门
    /// （`BaseObject.Base.cs:620`：`!RushMode` 才 `WalkTick += 800 + Random(1000)`）。
    ///
    /// **P3-B5 新增**（`crates/world` 接口变更，须按 §12.4 通知并行车道）：
    /// ctor 缺省 `false`（oracle 构造缺省）；本骨架无写入点（冲锋态未建模）⇒ 生产恒 false。
    /// C# `PlayObject.ExtraAbil[7]`（`PlayObject.cs:201`）——`AttPowerUp` 写槽 0
    /// （`EABIL_DCUP`，`ActorConst.cs:26`）。**P3-B5 新增**。
    /// C# `PlayObject.ExtraAbilTimes[7]`（`PlayObject.cs:202`）——到期时刻（sim 毫秒）。
    /// C# `PlayObject.ExtraAbilFlag[7]`（`PlayObject.cs:203`）——「到期前 10s 提示已发」标记。
    // `Job`（0 战 / 1 法 / 2 道）
    // S4a 迁出：`job` 真值在 components 组件（见迁移文档 S4a）。
    // `LifeAttrib`（1 = `LA_UNDEAD` 不死系）
    // S4a 迁出：`life_attrib` 真值在 components 组件（见迁移文档 S4a）。
    // `Abil.Level`
    // S4a 迁出：`level` 真值在 components 组件（见迁移文档 S4a）。
    // `MonsterInfo.Exp`（怪物经验，死亡时按 `CalcGetExp` 结算）
    // S4a 迁出：`mon_exp` 真值在 components 组件（见迁移文档 S4a）。
    // `Abil.Exp` / `Abil.MaxExp`
    // S4a 迁出：`exp` 真值在 components 组件（见迁移文档 S4a）。
    // S4a 迁出：`max_exp` 真值在 components 组件（见迁移文档 S4a）。
    // `AddAbil.UndeadPower`（攻击方对不死系的加成）
    // S4a 迁出：`undead_power` 真值在 components 组件（见迁移文档 S4a）。
    // `LastHiter`（最后一击者 `ActorId`）
    // S4a-2 迁出：`last_hiter` 真值在 components 组件（见迁移文档 S4a）。
    // C# `ExpHitter`（经验归属者 `ActorId`）——由 `SetLastHiter` 设置：
    // **第一个**命中者粘住不放（`BaseObject.cs:1753` 的 `if (ExpHitter == null)`），
    // 死亡时 `KillFunc()`（`BaseObject.cs:2429`）把经验发给它，而不是 `LastHiter`。
    // S4a-2 迁出：`exp_hitter` 真值在 components 组件（见迁移文档 S4a）。
    // `BaseObject.LastHiterTick` / `ExpHitterTick`（`BaseObject.cs:295-297`）。
    // 由 [`Entity::set_last_hiter`]（`BaseObject.cs:1753-1770`）刷新；`BaseObject.Run`
    // 的过期段（`BaseObject.Base.cs:119-132`）用它们判 30s / 6s 墙钟过期。
    // S4a-2 迁出：`last_hiter_tick` 真值在 components 组件（见迁移文档 S4a）。
    // S4a-2 迁出：`exp_hitter_tick` 真值在 components 组件（见迁移文档 S4a）。
    /// C# `PlayObject.Revival`（`PlayObject.cs:284`）——复活戒指生效标志。
    ///
    /// **粘滞锁存**：`RecalcAbilitys()` 只在 Ringr/Ringl 槽的 `Shape == 114`
    /// （`ItemShapeConst.RING_REVIVAL_ITEM`，`PlayObject.cs:2241-2243`）时置 `true`，
    /// **全仓没有任何置 `false` 的写入点**（spike 实测的 oracle bug）⇒ 摘下戒指后仍为 true。
    /// 本字段同口径：调用方按 `entity.revival |= recalc.revival` 落账，绝不重置。
    /// C# `PlayObject.RevivalTick`（`PlayObject.cs:283`）——上次复活时刻（sim 毫秒）。
    ///
    /// P32 的 60s 门：`(now - RevivalTick) > Config.RevivalTime`（`PlayObject.Message.cs:753`）。
    /// C# `PlayObject.UnRevival`（`PlayObject.cs:506`，`bool` 属性缺省 `false`）——
    /// **全仓无写入点**（`grep -rn UnRevival src` 只有声明与 P32 的读点）。
    /// P32 门：`LastHiter` 是玩家且其 `UnRevival == true` ⇒ 不复活
    /// （`PlayObject.Message.cs:751`）。本骨架同缺省 false，只在显式赋值时生效。
    /// C# `PlayObject.FlameRing`（`PlayObject.cs:283` 附近）——火球戒指生效标志。
    ///
    /// **粘滞锁存（oracle bug，照抄）**：`RecalcAbilitys()` 顶部只重置 `RecoveryRing`
    /// （`PlayObject.cs:2017`），全仓唯一写入点是 Ringl/Ringr 的 `Shape == 115` 分支
    /// （`ItemShapeConst.RING_FIREBALL_ITEM`，`:2244-2246`）的 `FlameRing = true`；
    /// **没有任何置 `false` 的写入点**。调用方按 `entity.flame_ring |= recalc.flame_ring` 落账。
    // `BaseObject.HealthTick` / `SpellTick` / `AutoRecoveryTick`（`BaseObject.cs:287-306`）。
    //
    // 恢复段（`BaseObject.Base.cs:39-66`）按 `(now - AutoRecoveryTick) / 20` 把时间折成
    // `HealthTick/SpellTick`，到 `HealthFillTime`/`SpellFillTime` 才回 HP/MP。
    // S4a-2 迁出：`health_tick` 真值在 components 组件（见迁移文档 S4a）。
    // S4a-2 迁出：`spell_tick` 真值在 components 组件（见迁移文档 S4a）。
    // S4a-2 迁出：`auto_recovery_tick` 真值在 components 组件（见迁移文档 S4a）。
    // 掉落预算里的金币（`mon.Gold`）
    // S4a-2 迁出：`gold` 真值在 components 组件（见迁移文档 S4a）。
    // 掉落预算里的物品（`mon.ItemList`，生成时算好、死亡时落地）
    // S4a-2 迁出：`drop_items` 真值在 components 组件（见迁移文档 S4a）。
    /// 死亡结算是否已做（避免重复发经验/重复掉落）
    /// 本 tick 是否收到过**直接 `Die()` 调用**（脚本 `KILLMONSTER` / GM Kill 的
    /// [`crate::world::World::kill`] 入口；oracle 无此字段，是本骨架区分调用来源的一次性标记）。
    ///
    /// 伤害只把 `HP` 打到 0；SuperMan 在 `BaseObject.Run:32-36` 先回满，故伤害路径不会
    /// 执行 `:70-73` 的 `Die()`。只有本标记为 true 时，`settle_deaths` 才执行 SuperMan
    /// 子类覆写体；结算时立即消费，保证两次直接 `Die()` 各执行一次。
    // 构造函数链抽出的随机数结果（见 [`MonsterCtorDraws`]）；玩家/非 MonGen 路径实体为缺省全零。
    // S4a-2 迁出：`ctor_draws` 真值在 components 组件（见迁移文档 S4a）。
    /// C# `StickMonster.ComeOutValue`（`StickMonster.cs:10`；ctor `:19` 置 4）。
    ///
    /// **类常量**：只有 `StickMonster` 家族有该字段（`CentipedeKingMonster.cs:14` = 4、
    /// `SandMobObject.cs:11` = 8）。类由 `MonsterInfo.Race` 决定（`WorldServer.MonGen.cs:504-729`
    /// 的 `new Xxx()`），装载钩子见 [`crate::mongen::apply_monster_class_constants`]。
    /// `Entity` 是各 C# 类的公共载体，非该家族的怪保持 C# 字段缺省 0。
    ///
    /// 消费方：MB-1（`StickMonster.Run` R38）、MB-3（`CentipedeKingMonster.Run` R07）、
    /// MB-8（`SandMobObject.Run` R33）。
    /// C# `StickMonster.AttackRange`（`StickMonster.cs:11`；ctor `:20` 置 4；
    /// `CentipedeKingMonster.cs:15` 置 6）。缺省 0（非 `StickMonster` 家族无此字段）。
    ///
    /// 消费方：MB-1（R38）、MB-3（R07）、MB-8（R33）。
    /// C# `MagicMonObject.UseMagic`（`MagicMonObject.cs:10`，ctor `:15` = `false`）/
    /// `ElectronicScolpionMon.UseMagic`（`ElectronicScolpionMon.cs:7`，ctor `:12` = `false`）。
    /// 运行期由各自 `Run` 按血量改写（`MagicMonObject.cs:29/33`、`ElectronicScolpionMon.cs:39/43`）。
    ///
    /// 消费方：MB-6（R25 / R14）。
    /// C# `BoneKingMonster.DangerLevel`（`short`，`BoneKingMonster.cs:9`；ctor `:17` = 5）/
    /// `ScultureKingMonster._mNDangerLevel`（`int`，`ScultureKingMonster.cs:11`；ctor `:21` = 5）。
    /// 装载后按类置 5；运行期递减/回满（`BoneKingMonster.cs:56-63`、`ScultureKingMonster.cs:94-101`）。
    ///
    /// 消费方：MB-7（BoneKing R05）、MB-3（ScultureKing R34）。
    /// C# `SoccerBall.N550`（`SoccerBall.cs:9`；ctor `:15` = 0）。
    ///
    /// 消费方：MB-8（SoccerBall R36；`Struck` `:83` 写、`Run` `:24/:70` 读写）。
    /// **MB-3（R07）**：`CentipedeKingMonster._attackTick`（`CentipedeKingMonster.cs:9`）——
    /// 类私有攻击节拍，**不是** `WAbil.AttackTick`（后者是 `ai.attack_tick`）。
    /// ctor `:17` 置 `GetTickCount()`，由装载钩子按 `World::clock_ms` 落定；运行期在
    /// `Run`/`AttackTarget` 里刷新（10s/3s 两道门都读它）。
    /// **MB-3（R34）**：`ScultureKingMonster._mSlaveObjectList`（`ScultureKingMonster.cs:12`）——
    /// `CallSlave` 生成成功即入表；`Run` 尾倒序移除 `Death || Ghost` 的子怪。
    /// 与 BeeQueen 的 `special_children`（`BeeList`）是不同 C# 字段，不合并。
    pub class_slave_list: Vec<i32>,
    /// 怪物/动物 AI 状态（`AnimalObject`/`MonsterObject` 字段子集；玩家不用，保持默认）。
    pub ai: AnimalState,
    /// 商人状态（`Merchant` 字段子集，M3⑤-实现 2 第①片）。`Some(_)` ⇒ 本实体是交易商人：
    /// 这是商人的**身份门**——[`crate::monai::monster_ai_tick`]、`entity_cell_type`、
    /// `consumes_view_range_update`、`merchant_ids_sorted` 都按它判定，**不**按 race
    /// （商人行为轴 `race = 10` 与固定 NPC 同值，外观轴 `race_img = 50` 又与动物同值）。
    pub merchant: Option<crate::merchant::MerchantState>,
    /// 固定管理 NPC 状态（`NormNpc` 字段子集 + ctor 抽数，卡 `openmir2-npcs-fixed-loading`）。
    /// `Some(_)` ⇒ 本实体是 `Envir/Npcs.txt` 的固定 NPC（Merchant/GuildOfficial/CastleOfficial）：
    /// [`crate::monai::monster_ai_tick`] 显式跳过它（固定 NPC 由 [`crate::World::npc_tick`]
    /// 驱动，即 oracle `ProcessNpcs` 的建模；`monster_ai_tick` 与 `npc_tick` 不得双驱）；
    /// [`crate::World::merchant_tick`] 只认 `merchant.is_some()` ⇒ 也跳过。
    pub npc: Option<crate::npc::NpcState>,
}

impl Entity {
    /// ctor 等价物：返回**生成期载体** [`EntitySpawn`]（`Entity` + 落图前的
    /// [`Transform`] / [`MapBinding`]）。坐标在 `insert` 时落成组件；插入后真值只在组件里。
    ///
    /// 保留 `(x, y)` 参数是为了让 约 700 处生成期赋值保持原形（`e.x = …` 写的是
    /// `EntitySpawn.transform`），不改变任何构造点的可读性。
    // `new` 返回生成期载体（`EntitySpawn` 而不是 `Self`），以保持 约 700 处生成期写法；
    // 名字沿用 `Entity::new`（oracle ctor 等价物）。
    #[allow(clippy::new_ret_no_self)]
    pub fn new(id: i32, name: impl Into<String>, x: i32, y: i32) -> EntitySpawn {
        let entity = Entity {
            id,
            name: name.into(),
            master: None,
            slave_relax: false,
            slave_list: Vec::new(),
            // `BaseObject.cs:403`：ctor `StickMode = false`；两类虫后由 ctor 等价点置 true。
            special_children: Vec::new(),
            is_slave: false,
            is_clone: false,
            slave_exp_level: 0,
            slave_make_level: 0,
            master_royalty_tick: 0,
            master_tick: 0,
            // `PlayObject.cs:908` 新角色 `AttatckMode = 0 = HAM_ALL`；未接 `CM_ATTACKMODE` 前恒为 0。
            // `PlayObject.cs:891`：`Initialize()` 置 `DieInFight3Zone = false`。
            // `BaseObject.cs:385`：ctor 初值 `NameColor = 255`。
            // `PlayObject.cs:62/943`：新角色 `Luck` 由 `RecalcAbilitys` 置 0（无 `AddAbil`），
            // `PowerRate = 100`；`BoPowerItem=false` / `PowerItem=0`。
            // `PlayObject.cs:725-727`：`PowerRateTime`/`KillMonExpRateTime` 无 ctor 赋值 ⇒ 0；
            // `KillMonExpRate = 100`（`:941`）；`ExpRateTick` 的 ctor 取钟由落图点重盖。
            // `PlayObject.cs:1047` 的 `RandomNo` 由登录构造点抽数后写入；`Entity::new` 缺省 0。
            // `PlayObject.cs:181/185/194/195`：ctor 缺省 `FireHitSkill/TwinHitSkill = false`、
            // 两个 tick 为 0（C# `int` 缺省）；真实置位来自技能层（未移植，见字段文档）。
            // 重叠推挤状态（`BoDuplication` / `DupStartTick` / `CheckDupObjTick`）已于 S3a
            // 迁入 `DupState` 组件：`Entity::new` 的同值缺省 = `DupState::default`（false/0/0），
            // 由 `EntityStore::insert` 统一插入；`CheckDupObjTick` 的 ctor/落图时刻由
            // `World::add_entity` / `SessionRegistry::enter_map_at` 经 `EntityStore::dup_state_mut` 重盖。
            // `CharacterObject.cs:89`：13 个空位（`Index = 0`）。
            use_items: [UseItemSlot::default(); USE_ITEM_SLOTS],
            // `PlayObject.cs:2018`：ctor 缺省 `AngryRing = false`；`NoDropUseItem` 亦无 ctor 赋值。
            // 玩家默认满血（WAbil.HP 的默认值不能是 0：0 在语义上等于「已死」，
            // 会让死亡结算把刚进图的玩家当成死者——实测踩过）
            // `BaseObject.cs:391`/`:393`：`GreenPoisoningPoint = 0`、`CharStatusEx = 0`；
            // `CharStatus` 未显式赋值 ⇒ int 缺省 0；`PoisoningTick` ctor 置 `GetTickCount()`
            // （本骨架用模拟开机基 `BUFF_EPOCH_MS`；精确落图时刻由 `World::add_entity` 重盖）。
            // `BaseObject.cs:81/394`：`StatusTimeArr = new ushort[15]`（全 0）；
            // `Transparent`/`ShowHp`/`ShowHpTick`/`ShowHpInterval`/`SuperMan` 构造缺省。
            // `CharacterObject.HitSpeed`（`RecalcAbilitys` 才赋值）⇒ ctor 缺省 0。
            // `PlayObject.cs:490/494`：`MagicShield`/`UnMagicShield` 构造缺省 `false`。
            // 生成路径不调 `RecalcAbilitys` ⇒ 快照未取（首次需要时懒取，见 `abil_snapshot`）。
            // `BaseObject.cs:390`：ctor 置 `AntiMagic = 0`（普通生成路径不调 `RecalcAbilitys`）。
            // `BaseObject.cs:384/:389`：ctor 置 `Light = 0`、`AntiPoison = 0`。
            // `NormNpc`/`Merchant` 的 ctor 在各自创建点覆盖为 2/99（`NormNpc.cs:43-44`）。
            // `BaseObject.WAbil.MAC` 构造缺省 0（`RecalcAbilitys` 才写 `Abil.MAC + AddAbil.MAC`）。
            // `BaseObject.RushMode` 构造缺省 false（`BaseObject.cs` ctor）。
            // `PlayObject.cs:201-203`：三个 7 槽数组构造缺省全 0。
            class_slave_list: Vec::new(),
            // MB-7：神兽两形态 ctor 置 `BoIsFirst = true`（`apply_monster_class_constants`）；
            // 字段缺省照 C# `bool` = false。`DigDownTick` 是 `int` 缺省 0。
            // 专用怪 ctor 类常量（PR-2）：`Entity` 是 C# 各怪物类的公共载体，这里落
            // **C# 字段缺省**（`int`/`sbyte` 0、`bool` false）；派生类 ctor 的实际常量
            // （StickMonster 4/4、CentipedeKing 4/6、SandMob 8/4、BoneKing/ScultureKing 5、
            // ArcherGuard -1 等）由装载钩子 `crate::mongen::apply_monster_class_constants`
            // 按 race 覆盖（oracle `new Xxx()` 的位置）。无抽数。
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
        };
        EntitySpawn {
            entity,
            x,
            y,
            // `BaseObject.cs:383` ctor `ViewRange = 5`；`map_id` ctor 缺省 0（落图点重盖/登记）。
            map_id: 0,
            view_range: 5,
            map_binding: MapBinding::default(),
            // S4a ctor 缺省（迁移文档 §3.4）：玩家默认满血（`hp=0` 语义上等于「已死」），
            // 等级 1；`abil: None` = 生成路径不调 `RecalcAbilitys`，首次需要时懒取快照。
            hp: EntitySpawn::CTOR_HP,
            max_hp: EntitySpawn::CTOR_MAX_HP,
            mp: 0,
            max_mp: 0,
            hit_speed: 0,
            dc: 0,
            ac: 0,
            mac: 0,
            hit_point: 0,
            speed_point: 0,
            anti_magic: 0,
            anti_poison: 0,
            undead_power: 0,
            abil: None,
            job: 0,
            life_attrib: 0,
            level: EntitySpawn::CTOR_LEVEL,
            mon_exp: 0,
            exp: 0,
            max_exp: 0,
            // S4a-2 ctor 缺省（与迁移前 `Entity::new` 逐字段一致）。
            castle_binding: None,
            mon_gen: None,
            mon_gen_zen_time: 0,
            ctor_draws: MonsterCtorDraws::default(),
            gold: 0,
            drop_items: Vec::new(),
            angry_ring: false,
            no_drop_use_item: false,
            no_item: false,
            last_hiter: None,
            exp_hitter: None,
            last_hiter_tick: 0,
            exp_hitter_tick: 0,
            health_tick: 0,
            spell_tick: 0,
            auto_recovery_tick: 0,
            dear_human: None,
            pk_point: 0,
            pvp_flag: false,
            pvp_name_color_tick: 0,
            die_in_fight3_zone: false,
            // S4b-1 ctor 缺省（与迁移前 `Entity::new` 逐字段一致；`name_color=255`、
            // `race=ACTOR_RACE_PLAY` 是 ctor 覆盖值，见迁移文档 §3.4）。
            invisible: false,
            hide_mode: false,
            fixed_hide_mode: false,
            ob_mode: false,
            nasty_mode: false,
            attack_mode: 0,
            in_safe_zone: false,
            admin_mode: false,
            stone_mode: false,
            super_man: false,
            rush_mode: false,
            want_ref_msg: false,
            stick_mode: false,
            death: false,
            ghost: false,
            death_tick: 0,
            ghost_tick: 0,
            can_re_alive: false,
            re_alive_tick: 0,
            death_settled: false,
            die_called: false,
            race: ACTOR_RACE_PLAY,
            race_img: 0,
            appr: 0,
            light: 0,
            name_color: 255,
            come_out_value: 0,
            attack_range: 0,
            use_magic: false,
            danger_level: 0,
            n550: 0,
            class_attack_tick: 0,
            bo_is_first: false,
            dig_down_tick: 0,
            appear_start_tick: 0,
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
            luck: 0,
            power_rate: 100,
            power_rate_time: 0,
            kill_mon_exp_rate: 100,
            kill_mon_exp_rate_time: 0,
            exp_rate_tick: 0,
            random_no: 0,
            bo_power_item: false,
            power_item: 0,
            fire_hit_skill: false,
            latest_fire_hit_tick: 0,
            twin_hit_skill: false,
            latest_twin_hit_tick: 0,
            allow_group: false,
            allow_group_re_call: false,
            group_owner: 0,
            group_members: Vec::new(),
            revival: false,
            revival_tick: 0,
            un_revival: false,
            flame_ring: false,
            poisoning_tick: crate::buff::BUFF_EPOCH_MS,
            green_poisoning_point: 0,
            char_status: 0,
            char_status_ex: 0,
            status_time: [0; STATUS_TIME_SLOTS],
            transparent: false,
            show_hp: false,
            show_hp_tick: 0,
            show_hp_interval: 0,
            magic_shield: false,
            un_magic_shield: false,
            extra_abil: [0; EXTRA_ABIL_SLOTS],
            extra_abil_times: [0; EXTRA_ABIL_SLOTS],
            extra_abil_flag: [0; EXTRA_ABIL_SLOTS],
        }
    }

    /// 当前朝向（C# `BaseObject.Dir`），供接线层发 `SM_TURN`/`RM_TURN` 等帧面用。
    #[must_use]
    pub fn dir(&self) -> u8 {
        self.ai.dir
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
/// 读路径缓存查询：`Entity` + 位置/绑定 + S4a 属性/成长组件。
type ReadViewQuery = bevy_ecs::query::QueryState<(
    &'static Entity,
    &'static Transform,
    &'static MapBinding,
    &'static Vitals,
    &'static AbilitySnapshot<BaseAbil>,
    &'static Progression,
    &'static SpawnBinding,
    &'static DropState,
    &'static CombatAttribution,
    &'static Recovery,
    &'static PvpState,
    &'static Flags,
    &'static LifeCycle,
    &'static Appearance,
    &'static MonsterClassState,
)>;

/// 读路径的第二缓存查询（S4b-2）：bevy `QueryData` 元组上限 15，S4b-2 追加 4 个组件后
/// 与 [`ReadViewQuery`] 分两次 `get_manual(&World, …)` 取（两次都是共享借用，不改变读写顺序）。
type ReadView2Query = bevy_ecs::query::QueryState<(
    &'static PlayerRunState,
    &'static GroupState,
    &'static Revival,
    &'static BuffState,
)>;

/// 写路径缓存查询：`Entity` 主组件 + S4a 属性/成长组件 + S4b-1 状态组件（[`EntityMut`] 用）。
type MutViewQuery = bevy_ecs::query::QueryState<(
    &'static mut Entity,
    &'static mut Vitals,
    &'static mut AbilitySnapshot<BaseAbil>,
    &'static mut Progression,
    &'static mut SpawnBinding,
    &'static mut DropState,
    &'static mut CombatAttribution,
    &'static mut Flags,
    &'static mut LifeCycle,
    &'static mut Appearance,
    &'static mut MonsterClassState,
    &'static mut PlayerRunState,
    &'static mut GroupState,
    &'static mut Revival,
    &'static mut BuffState,
)>;

pub struct EntityStore {
    inner: mir2_world_bevy::EcsStore<Entity>,
    /// 读路径的缓存查询（S3b/S4a）：一次按句柄取齐 `Entity` + 位置/绑定 + 属性/成长组件，
    /// 避免每个读取点逐次 `TypeId` 查表。只按显式句柄 `get_manual`，不迭代求顺序。
    view_state: ReadViewQuery,
    /// 读路径第二缓存查询（S4b-2）。
    view2_state: ReadView2Query,
    /// 写路径的缓存查询（S4a/S4b）：一次按句柄取齐可变主组件与属性/成长/状态组件（[`EntityMut`] 用）。
    mut_view_state: MutViewQuery,
}

impl std::fmt::Debug for EntityStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityStore")
            .field("len", &self.len())
            .finish()
    }
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
        let view_state = inner.world_mut().query::<(
            &Entity,
            &Transform,
            &MapBinding,
            &Vitals,
            &AbilitySnapshot<BaseAbil>,
            &Progression,
            &SpawnBinding,
            &DropState,
            &CombatAttribution,
            &Recovery,
            &PvpState,
            &Flags,
            &LifeCycle,
            &Appearance,
            &MonsterClassState,
        )>();
        let view2_state = inner
            .world_mut()
            .query::<(&PlayerRunState, &GroupState, &Revival, &BuffState)>();
        let mut_view_state = inner.world_mut().query::<(
            &mut Entity,
            &mut Vitals,
            &mut AbilitySnapshot<BaseAbil>,
            &mut Progression,
            &mut SpawnBinding,
            &mut DropState,
            &mut CombatAttribution,
            &mut Flags,
            &mut LifeCycle,
            &mut Appearance,
            &mut MonsterClassState,
            &mut PlayerRunState,
            &mut GroupState,
            &mut Revival,
            &mut BuffState,
        )>();
        EntityStore {
            inner,
            view_state,
            view2_state,
            mut_view_state,
        }
    }

    /// 结构变更后刷新视图查询的 archetype 匹配集。
    fn refresh_view(&mut self) {
        self.view_state.update_archetypes(self.inner.world());
        self.view2_state.update_archetypes(self.inner.world());
        self.mut_view_state.update_archetypes(self.inner.world());
    }

    /// 分配一个 id（不插入实体；C# `ActorMgr` 先 `Add` 后填字段）。
    pub fn alloc_id(&mut self) -> i32 {
        self.inner.alloc_id()
    }

    /// 插入实体（生成期载体），返回其 id（复用本实体的 `id`）。
    ///
    /// **S3a/S3b/S4a 组件集合完整性**：每个实体插入时都带 `DupState`（S3a）、`Transform` 与
    /// `MapBinding`（S3b）、`Vitals` / `AbilitySnapshot` / `Progression`（S4a）组件。
    /// 坐标/绑定/属性在 `insert` 这一刻从生成期载体落成组件，其后**不再有第二份真值**
    ///（`Entity` 没有这些字段）。
    ///
    /// `insert_component` 对「刚由同一 store 插入的 id」不可能失败，因此用**硬断言**
    /// （release 也保持行为）：宁可立即 panic，也不允许半实体被 AOI/重叠检查静默跳过
    ///（S3a 评审 P3-3）。
    pub fn insert(&mut self, spawn: EntitySpawn) -> i32 {
        self.insert_parts(spawn.into_parts())
    }

    /// 组件已拆好的插入入口（克隆/迁移路径用，避免再拼一个 `EntitySpawn`）。
    ///
    /// **整 bundle 插入**：任一可选组件缺失都立即 panic（半实体不得进入 AOI/重叠检查）。
    fn insert_parts(&mut self, parts: SpawnParts) -> i32 {
        let id = self.inner.insert(parts.entity.id, parts.entity);
        assert!(
            self.inner.insert_component(id, parts.transform),
            "刚插入的实体必须能挂 Transform 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.map_binding),
            "刚插入的实体必须能挂 MapBinding 组件（id={id}）"
        );
        assert!(
            self.inner
                .insert_component(id, mir2_world_bevy::DupState::default()),
            "刚插入的实体必须能挂 DupState 组件（id={id}）；缺组件会让 check_dup_obj 静默跳过"
        );
        assert!(
            self.inner.insert_component(id, parts.vitals),
            "刚插入的实体必须能挂 Vitals 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.ability),
            "刚插入的实体必须能挂 AbilitySnapshot 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.progression),
            "刚插入的实体必须能挂 Progression 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.spawn),
            "刚插入的实体必须能挂 SpawnBinding 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.drop),
            "刚插入的实体必须能挂 DropState 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.attribution),
            "刚插入的实体必须能挂 CombatAttribution 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.recovery),
            "刚插入的实体必须能挂 Recovery 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.pvp),
            "刚插入的实体必须能挂 PvpState 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.flags),
            "刚插入的实体必须能挂 Flags 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.life_cycle),
            "刚插入的实体必须能挂 LifeCycle 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.appearance),
            "刚插入的实体必须能挂 Appearance 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.monster_class),
            "刚插入的实体必须能挂 MonsterClassState 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.player_run),
            "刚插入的实体必须能挂 PlayerRunState 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.group),
            "刚插入的实体必须能挂 GroupState 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.revival),
            "刚插入的实体必须能挂 Revival 组件（id={id}）"
        );
        assert!(
            self.inner.insert_component(id, parts.buff),
            "刚插入的实体必须能挂 BuffState 组件（id={id}）"
        );
        self.refresh_view();
        id
    }

    /// 受控读取位置/视野组件（S3b 起真值在组件，不在 `Entity`）。
    pub fn transform(&self, id: i32) -> Option<&Transform> {
        self.inner.get_component(id)
    }

    /// 受控可变访问位置/视野组件。**调用方负责**与 `MapGrid` 格子登记保持原子
    ///（本方法不动地图格子）。
    pub fn transform_mut(&mut self, id: i32) -> Option<&mut Transform> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取 AOI/落图绑定组件（S3b 起真值在组件，不在 `Entity`）。
    pub fn map_binding(&self, id: i32) -> Option<&MapBinding> {
        self.inner.get_component(id)
    }

    pub fn map_binding_mut(&mut self, id: i32) -> Option<&mut MapBinding> {
        self.inner.get_component_mut(id)
    }

    /// 覆盖观察者可见集（测试/AOI 重置路径；等价于 `map_binding_mut(id).visible_actors = v`）。
    pub fn set_visible_actors(&mut self, id: i32, actors: Vec<VisibleBaseObject>) -> bool {
        match self.map_binding_mut(id) {
            Some(b) => {
                b.visible_actors = actors;
                true
            }
            None => false,
        }
    }

    pub fn set_visible_events(&mut self, id: i32, events: Vec<VisibleEvent>) -> bool {
        match self.map_binding_mut(id) {
            Some(b) => {
                b.visible_events = events;
                true
            }
            None => false,
        }
    }

    /// 常用只读投影（AOI/日志/宿主帧面用；避免各处重复 `transform(...).map(...)`）。
    #[inline]
    pub fn x(&self, id: i32) -> Option<i32> {
        self.transform(id).map(|t| t.x)
    }

    #[inline]
    pub fn y(&self, id: i32) -> Option<i32> {
        self.transform(id).map(|t| t.y)
    }

    #[inline]
    pub fn map_id(&self, id: i32) -> Option<usize> {
        self.transform(id).map(|t| t.map_id)
    }

    #[inline]
    pub fn view_range(&self, id: i32) -> Option<i32> {
        self.transform(id).map(|t| t.view_range)
    }

    #[inline]
    pub fn xy(&self, id: i32) -> Option<(i32, i32)> {
        self.transform(id).map(|t| (t.x, t.y))
    }

    #[inline]
    pub fn pos(&self, id: i32) -> Option<(i32, i32, usize)> {
        self.transform(id).map(|t| (t.x, t.y, t.map_id))
    }

    /// 受控写入位置（**不**动地图格子；调用方按 `MapGrid` 契约另行登记/摘除）。
    pub fn set_xy(&mut self, id: i32, x: i32, y: i32) -> bool {
        match self.transform_mut(id) {
            Some(t) => {
                t.x = x;
                t.y = y;
                true
            }
            None => false,
        }
    }

    pub fn set_map_id(&mut self, id: i32, map_id: usize) -> bool {
        match self.transform_mut(id) {
            Some(t) => {
                t.map_id = map_id;
                true
            }
            None => false,
        }
    }

    pub fn set_view_range(&mut self, id: i32, view_range: i32) -> bool {
        match self.transform_mut(id) {
            Some(t) => {
                t.view_range = view_range;
                true
            }
            None => false,
        }
    }

    /// 观察者当前可见目标 id（保序；`visible_actors` 的常用投影）。
    pub fn visible_targets(&self, id: i32) -> Option<Vec<i32>> {
        self.map_binding(id)
            .map(|m| m.visible_actors.iter().map(|v| v.target).collect())
    }

    #[inline]
    pub fn is_visible_active(&self, id: i32) -> Option<bool> {
        self.map_binding(id).map(|m| m.is_visible_active)
    }

    /// 受控读取实体的重叠推挤状态（S3a 起真值在 `DupState` 组件，不在 `Entity`）。
    pub fn dup_state(&self, id: i32) -> Option<&mir2_world_bevy::DupState> {
        self.inner.get_component(id)
    }

    /// 受控可变访问：tick 段/落图点从这里读写，不暴露裸 `Query`。
    pub fn dup_state_mut(&mut self, id: i32) -> Option<&mut mir2_world_bevy::DupState> {
        self.inner.get_component_mut(id)
    }

    /// 覆盖式插入 `DupState`（切片初始化/迁移工具用；常规路径由 [`Self::insert`] 给默认值）。
    pub fn insert_dup_state(&mut self, id: i32, state: mir2_world_bevy::DupState) -> bool {
        self.inner.insert_component(id, state)
    }

    pub fn remove(&mut self, id: i32) -> Option<Entity> {
        let taken = self.inner.remove(id);
        if taken.is_some() {
            self.refresh_view();
        }
        taken
    }

    /// O(1) 查表（C# `ActorMgr.Get`），返回组件只读视图 [`EntityRef`]。
    pub fn get(&self, id: i32) -> Option<EntityRef<'_>> {
        let bevy_entity = self.inner.entity_of(id)?;
        let (
            entity,
            transform,
            binding,
            vitals,
            ability,
            progression,
            spawn,
            drop,
            attribution,
            recovery,
            pvp,
            flags,
            life_cycle,
            appearance,
            monster_class,
        ) = self
            .view_state
            .get_manual(self.inner.world(), bevy_entity)
            .ok()?;
        let (player_run, group, revival, buff) = self
            .view2_state
            .get_manual(self.inner.world(), bevy_entity)
            .ok()?;
        Some(EntityRef::from_components(
            entity,
            transform,
            binding,
            vitals,
            ability,
            progression,
            spawn,
            drop,
            attribution,
            recovery,
            pvp,
            flags,
            life_cycle,
            appearance,
            monster_class,
            player_run,
            group,
            revival,
            buff,
        ))
    }

    /// 可变访问：`Entity` 剩余字段 + S4a 属性/成长组件（[`EntityMut`]）。
    ///
    /// 坐标/绑定请用 `transform_mut` / `map_binding_mut`。
    pub fn get_mut(&mut self, id: i32) -> Option<EntityMut<'_>> {
        let bevy_entity = self.inner.entity_of(id)?;
        self.mut_view_of_entity(bevy_entity)
    }

    /// 受控读取属性组件（S4a 起真值在组件，不在 `Entity`）。
    pub fn vitals(&self, id: i32) -> Option<&Vitals> {
        self.inner.get_component(id)
    }

    /// 受控可变访问属性组件。
    pub fn vitals_mut(&mut self, id: i32) -> Option<&mut Vitals> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取成长/等级组件（S4a）。
    pub fn progression(&self, id: i32) -> Option<&Progression> {
        self.inner.get_component(id)
    }

    pub fn progression_mut(&mut self, id: i32) -> Option<&mut Progression> {
        self.inner.get_component_mut(id)
    }

    /// 覆盖式写回成长组件（「取副本算完再回写」路径用）。
    pub fn set_progression(&mut self, id: i32, progression: Progression) -> bool {
        match self.inner.get_component_mut::<Progression>(id) {
            Some(p) => {
                *p = progression;
                true
            }
            None => false,
        }
    }

    /// 把生成期载体的**全部真值**写回实体：主组件 + `Vitals` + `AbilitySnapshot` +
    /// `Progression`（不含 `Transform`/`MapBinding`，位置请走 `set_xy`/`map_binding_mut`）。
    ///
    /// 「`spawn_cloned` → 改副本 → 回写」路径必须用它，**不能**只写 `*e = spawn.entity`：
    /// S4a 起迁移字段不在 `Entity` 里，只写主组件会丢 HP/MP/经验等组件值（S4a 回归教训）。
    pub fn write_back(&mut self, id: i32, spawn: EntitySpawn) -> bool {
        let vitals = spawn.vitals_component();
        let progression = spawn.progression_component();
        let ability = AbilitySnapshot { abil: spawn.abil };
        let spawn_binding = spawn.spawn_component();
        let drop = spawn.drop_component();
        let attribution = CombatAttribution {
            last_hiter: spawn.last_hiter,
            exp_hitter: spawn.exp_hitter,
            last_hiter_tick: spawn.last_hiter_tick,
            exp_hitter_tick: spawn.exp_hitter_tick,
        };
        let recovery = Recovery {
            health_tick: spawn.health_tick,
            spell_tick: spawn.spell_tick,
            auto_recovery_tick: spawn.auto_recovery_tick,
        };
        let pvp = PvpState {
            dear_human: spawn.dear_human,
            pk_point: spawn.pk_point,
            pvp_flag: spawn.pvp_flag,
            pvp_name_color_tick: spawn.pvp_name_color_tick,
            die_in_fight3_zone: spawn.die_in_fight3_zone,
        };
        let flags = spawn.flags_component();
        let life_cycle = spawn.life_cycle_component();
        let appearance = spawn.appearance_component();
        let monster_class = spawn.monster_class_component();
        let player_run = spawn.player_run_component();
        let group = spawn.group_component();
        let revival = spawn.revival_component();
        let buff = spawn.buff_component();
        // `Recovery`/`PvpState` 不在 [`EntityMut`] 视图内（bevy 15 元组上限，见 `MutViewQuery`），
        // 仍由**同一个**组件实体承载，这里经受控入口整包写回（单真值，无影子副本）。
        if let Some(r) = self.inner.get_component_mut::<Recovery>(id) {
            *r = recovery;
        }
        if let Some(p) = self.inner.get_component_mut::<PvpState>(id) {
            *p = pvp;
        }
        let Some(view) = self.mut_view_of_entity_by_id(id) else {
            return false;
        };
        *view.entity = spawn.entity;
        *view.vitals = vitals;
        *view.ability = ability;
        *view.progression = progression;
        *view.spawn = spawn_binding;
        *view.drop = drop;
        *view.attribution = attribution;
        *view.flags = flags;
        *view.life_cycle = life_cycle;
        *view.appearance = appearance;
        *view.monster_class = monster_class;
        *view.player_run = player_run;
        *view.group = group;
        *view.revival = revival;
        *view.buff = buff;
        true
    }

    fn mut_view_of_entity_by_id(&mut self, id: i32) -> Option<EntityMut<'_>> {
        let bevy_entity = self.inner.entity_of(id)?;
        self.mut_view_of_entity(bevy_entity)
    }

    /// 按槽位顺序对每个实体执行 `f`（`&mut` 视图，含 S4a 组件）。
    ///
    /// 顺序仍来自槽位索引（`iter_mut` 的语义）；`f` 每次只拿一个 [`EntityMut`]，
    /// 因此不会同时可变借用同一实体。
    pub fn for_each_mut(&mut self, mut f: impl FnMut(EntityMut<'_>)) {
        for e in self.inner.order_entities() {
            if let Some(view) = self.mut_view_of_entity(e) {
                f(view);
            }
        }
    }

    /// 按 bevy 句柄构造 [`EntityMut`]（`for_each_mut` 用）。
    fn mut_view_of_entity(
        &mut self,
        bevy_entity: bevy_ecs::entity::Entity,
    ) -> Option<EntityMut<'_>> {
        let (
            entity,
            vitals,
            ability,
            progression,
            spawn,
            drop,
            attribution,
            flags,
            life_cycle,
            appearance,
            monster_class,
            player_run,
            group,
            revival,
            buff,
        ) = self
            .mut_view_state
            .get_mut(self.inner.world_mut(), bevy_entity)
            .ok()?;
        Some(EntityMut {
            entity: bevy_ecs::change_detection::Mut::into_inner(entity),
            vitals: bevy_ecs::change_detection::Mut::into_inner(vitals),
            ability: bevy_ecs::change_detection::Mut::into_inner(ability),
            progression: bevy_ecs::change_detection::Mut::into_inner(progression),
            spawn: bevy_ecs::change_detection::Mut::into_inner(spawn),
            drop: bevy_ecs::change_detection::Mut::into_inner(drop),
            attribution: bevy_ecs::change_detection::Mut::into_inner(attribution),
            flags: bevy_ecs::change_detection::Mut::into_inner(flags),
            life_cycle: bevy_ecs::change_detection::Mut::into_inner(life_cycle),
            appearance: bevy_ecs::change_detection::Mut::into_inner(appearance),
            monster_class: bevy_ecs::change_detection::Mut::into_inner(monster_class),
            player_run: bevy_ecs::change_detection::Mut::into_inner(player_run),
            group: bevy_ecs::change_detection::Mut::into_inner(group),
            revival: bevy_ecs::change_detection::Mut::into_inner(revival),
            buff: bevy_ecs::change_detection::Mut::into_inner(buff),
        })
    }

    /// 受控读取 `Abil` 快照组件（S4a）。
    pub fn ability(&self, id: i32) -> Option<&AbilitySnapshot<BaseAbil>> {
        self.inner.get_component(id)
    }

    pub fn ability_mut(&mut self, id: i32) -> Option<&mut AbilitySnapshot<BaseAbil>> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取状态位组件（S4b-1 起真值在组件，不在 `Entity`）。
    pub fn flags(&self, id: i32) -> Option<&Flags> {
        self.inner.get_component(id)
    }

    pub fn flags_mut(&mut self, id: i32) -> Option<&mut Flags> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取生命周期组件（S4b-1）。
    pub fn life_cycle(&self, id: i32) -> Option<&LifeCycle> {
        self.inner.get_component(id)
    }

    pub fn life_cycle_mut(&mut self, id: i32) -> Option<&mut LifeCycle> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取外观/行为轴组件（S4b-1）。
    pub fn appearance(&self, id: i32) -> Option<&Appearance> {
        self.inner.get_component(id)
    }

    pub fn appearance_mut(&mut self, id: i32) -> Option<&mut Appearance> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取类专用状态组件（S4b-1）。
    pub fn monster_class(&self, id: i32) -> Option<&MonsterClassState> {
        self.inner.get_component(id)
    }

    pub fn monster_class_mut(&mut self, id: i32) -> Option<&mut MonsterClassState> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取玩家运行态组件（S4b-2）。
    pub fn player_run(&self, id: i32) -> Option<&PlayerRunState> {
        self.inner.get_component(id)
    }

    pub fn player_run_mut(&mut self, id: i32) -> Option<&mut PlayerRunState> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取组队实体侧组件（S4b-2；组队服务仍是 actor）。
    pub fn group(&self, id: i32) -> Option<&GroupState> {
        self.inner.get_component(id)
    }

    pub fn group_mut(&mut self, id: i32) -> Option<&mut GroupState> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取复活粘滞位组件（S4b-2）。
    pub fn revival(&self, id: i32) -> Option<&Revival> {
        self.inner.get_component(id)
    }

    pub fn revival_mut(&mut self, id: i32) -> Option<&mut Revival> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取 Buff/毒/盾组件（S4b-2）。
    pub fn buff(&self, id: i32) -> Option<&BuffState> {
        self.inner.get_component(id)
    }

    pub fn buff_mut(&mut self, id: i32) -> Option<&mut BuffState> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取 HP/MP 恢复计时组件（S4a；不在 [`EntityMut`] 视图内，单独入口）。
    pub fn recovery(&self, id: i32) -> Option<&Recovery> {
        self.inner.get_component(id)
    }

    pub fn recovery_mut(&mut self, id: i32) -> Option<&mut Recovery> {
        self.inner.get_component_mut(id)
    }

    /// 受控读取 PK 状态组件（S4a；不在 [`EntityMut`] 视图内，单独入口）。
    pub fn pvp_state(&self, id: i32) -> Option<&PvpState> {
        self.inner.get_component(id)
    }

    pub fn pvp_state_mut(&mut self, id: i32) -> Option<&mut PvpState> {
        self.inner.get_component_mut(id)
    }

    /// 只取 `Entity` 主组件（剩余字段）的引用；需要「克隆一份再改」的路径用它
    ///（`get` 返回的 [`EntityRef`] 是只读视图，不携带可写 `Entity`）。
    pub fn entity(&self, id: i32) -> Option<&Entity> {
        self.inner.get(id)
    }

    /// 只取 `Entity` 主组件（剩余字段）的可变引用。
    ///
    /// 只适用于**不碰 S4a 迁移字段**的路径（如商人物品列表、AI 计时器）；需要 `vitals`/
    /// `progression` 的路径用 [`Self::get_mut`]（[`EntityMut`]）。
    pub fn entity_mut(&mut self, id: i32) -> Option<&mut Entity> {
        self.inner.get_mut(id)
    }

    /// 克隆 `Entity` 主组件（剩余字段；不含 Transform/MapBinding）。
    pub fn entity_cloned(&self, id: i32) -> Option<Entity> {
        self.inner.get(id).cloned()
    }

    /// 克隆「实体 + 位置 + 绑定 + S4a 属性组件」到生成期载体（供“取一份本地副本再算/回写”的路径）。
    ///
    /// 本地副本的 `x/y/map_id/view_range`/属性写的是载体字段，**不**自动回写组件；需要回写的
    /// 调用方必须显式走 `transform_mut` / `vitals_mut` 等。
    pub fn spawn_cloned(&self, id: i32) -> Option<EntitySpawn> {
        let bevy_entity = self.inner.entity_of(id)?;
        let (
            entity,
            transform,
            binding,
            vitals,
            ability,
            progression,
            spawn,
            drop,
            attribution,
            recovery,
            pvp,
            flags,
            life_cycle,
            appearance,
            monster_class,
        ) = self
            .view_state
            .get_manual(self.inner.world(), bevy_entity)
            .ok()?;
        let (player_run, group, revival, buff) = self
            .view2_state
            .get_manual(self.inner.world(), bevy_entity)
            .ok()?;
        Some(EntitySpawn {
            entity: entity.clone(),
            x: transform.x,
            y: transform.y,
            map_id: transform.map_id,
            view_range: transform.view_range,
            map_binding: binding.clone(),
            hp: vitals.hp,
            max_hp: vitals.max_hp,
            mp: vitals.mp,
            max_mp: vitals.max_mp,
            hit_speed: vitals.hit_speed,
            dc: vitals.dc,
            ac: vitals.ac,
            mac: vitals.mac,
            hit_point: vitals.hit_point,
            speed_point: vitals.speed_point,
            anti_magic: vitals.anti_magic,
            anti_poison: vitals.anti_poison,
            undead_power: vitals.undead_power,
            abil: ability.abil,
            job: progression.job,
            life_attrib: progression.life_attrib,
            level: progression.level,
            mon_exp: progression.mon_exp,
            exp: progression.exp,
            max_exp: progression.max_exp,
            castle_binding: spawn.castle_binding,
            mon_gen: spawn.mon_gen,
            mon_gen_zen_time: spawn.mon_gen_zen_time,
            ctor_draws: spawn.ctor_draws,
            gold: drop.gold,
            drop_items: drop.items.clone(),
            angry_ring: drop.angry_ring,
            no_drop_use_item: drop.no_drop_use_item,
            no_item: drop.no_item,
            last_hiter: attribution.last_hiter,
            exp_hitter: attribution.exp_hitter,
            last_hiter_tick: attribution.last_hiter_tick,
            exp_hitter_tick: attribution.exp_hitter_tick,
            health_tick: recovery.health_tick,
            spell_tick: recovery.spell_tick,
            auto_recovery_tick: recovery.auto_recovery_tick,
            dear_human: pvp.dear_human,
            pk_point: pvp.pk_point,
            pvp_flag: pvp.pvp_flag,
            pvp_name_color_tick: pvp.pvp_name_color_tick,
            die_in_fight3_zone: pvp.die_in_fight3_zone,
            invisible: flags.invisible,
            hide_mode: flags.hide_mode,
            fixed_hide_mode: flags.fixed_hide_mode,
            ob_mode: flags.ob_mode,
            nasty_mode: flags.nasty_mode,
            attack_mode: flags.attack_mode,
            in_safe_zone: flags.in_safe_zone,
            admin_mode: flags.admin_mode,
            stone_mode: flags.stone_mode,
            super_man: flags.super_man,
            rush_mode: flags.rush_mode,
            want_ref_msg: flags.want_ref_msg,
            stick_mode: flags.stick_mode,
            death: life_cycle.death,
            ghost: life_cycle.ghost,
            death_tick: life_cycle.death_tick,
            ghost_tick: life_cycle.ghost_tick,
            can_re_alive: life_cycle.can_re_alive,
            re_alive_tick: life_cycle.re_alive_tick,
            death_settled: life_cycle.death_settled,
            die_called: life_cycle.die_called,
            race: appearance.race,
            race_img: appearance.race_img,
            appr: appearance.appr,
            light: appearance.light,
            name_color: appearance.name_color,
            come_out_value: monster_class.come_out_value,
            attack_range: monster_class.attack_range,
            use_magic: monster_class.use_magic,
            danger_level: monster_class.danger_level,
            n550: monster_class.n550,
            class_attack_tick: monster_class.class_attack_tick,
            bo_is_first: monster_class.bo_is_first,
            dig_down_tick: monster_class.dig_down_tick,
            appear_start_tick: monster_class.appear_start_tick,
            guard_direction: monster_class.guard_direction,
            siege_lock_count: monster_class.siege_lock_count,
            jump_time: monster_class.jump_time,
            crazy_ready_mode: monster_class.crazy_ready_mode,
            crazy_king_mode: monster_class.crazy_king_mode,
            crazy_count: monster_class.crazy_count,
            crazy_ready: monster_class.crazy_ready,
            crazy_time: monster_class.crazy_time,
            old_hit_time: monster_class.old_hit_time,
            old_walk_time: monster_class.old_walk_time,
            luck: player_run.luck,
            power_rate: player_run.power_rate,
            power_rate_time: player_run.power_rate_time,
            kill_mon_exp_rate: player_run.kill_mon_exp_rate,
            kill_mon_exp_rate_time: player_run.kill_mon_exp_rate_time,
            exp_rate_tick: player_run.exp_rate_tick,
            random_no: player_run.random_no,
            bo_power_item: player_run.bo_power_item,
            power_item: player_run.power_item,
            fire_hit_skill: player_run.fire_hit_skill,
            latest_fire_hit_tick: player_run.latest_fire_hit_tick,
            twin_hit_skill: player_run.twin_hit_skill,
            latest_twin_hit_tick: player_run.latest_twin_hit_tick,
            allow_group: group.allow_group,
            allow_group_re_call: group.allow_group_re_call,
            group_owner: group.group_owner,
            group_members: group.group_members.clone(),
            revival: revival.revival,
            revival_tick: revival.revival_tick,
            un_revival: revival.un_revival,
            flame_ring: revival.flame_ring,
            poisoning_tick: buff.poisoning_tick,
            green_poisoning_point: buff.green_poisoning_point,
            char_status: buff.char_status,
            char_status_ex: buff.char_status_ex,
            status_time: buff.status_time,
            transparent: buff.transparent,
            show_hp: buff.show_hp,
            show_hp_tick: buff.show_hp_tick,
            show_hp_interval: buff.show_hp_interval,
            magic_shield: buff.magic_shield,
            un_magic_shield: buff.un_magic_shield,
            extra_abil: buff.extra_abil,
            extra_abil_times: buff.extra_abil_times,
            extra_abil_flag: buff.extra_abil_flag,
        })
    }

    /// 按槽位顺序遍历（tick 的确定性顺序）。
    pub fn iter(&self) -> impl Iterator<Item = &Entity> + '_ {
        self.inner.iter()
    }
    /// 按槽位顺序遍历**只读视图**（含 Transform/MapBinding/S4a 属性字段；宿主帧面/日志用）。
    pub fn iter_ref(&self) -> impl Iterator<Item = EntityRef<'_>> + '_ {
        self.inner
            .order_entities()
            .into_iter()
            .filter_map(|bevy_entity| {
                let (
                    entity,
                    transform,
                    binding,
                    vitals,
                    ability,
                    progression,
                    spawn,
                    drop,
                    attribution,
                    recovery,
                    pvp,
                    flags,
                    life_cycle,
                    appearance,
                    monster_class,
                ) = self
                    .view_state
                    .get_manual(self.inner.world(), bevy_entity)
                    .ok()?;
                let (player_run, group, revival, buff) = self
                    .view2_state
                    .get_manual(self.inner.world(), bevy_entity)
                    .ok()?;
                Some(EntityRef::from_components(
                    entity,
                    transform,
                    binding,
                    vitals,
                    ability,
                    progression,
                    spawn,
                    drop,
                    attribution,
                    recovery,
                    pvp,
                    flags,
                    life_cycle,
                    appearance,
                    monster_class,
                    player_run,
                    group,
                    revival,
                    buff,
                ))
            })
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

    /// 槽位顺序的 bevy 实体句柄（与 [`Self::order_index`] 逐项对齐）。
    pub fn order_entities(&self) -> Vec<bevy_ecs::entity::Entity> {
        self.inner.order_entities()
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

    /// **S4a 回归牙（总监要求）**：`spawn_cloned` 的副本里属性真值来自**组件**，回写必须整包
    /// （`EntityStore::write_back`），只写 `*e = spawn.entity` 会丢 HP/MP/exp。
    ///
    /// 牙：把 `write_back` 缩回「只写主组件」⇒ 第二条断言取到 37/11 而非 5/3 ⇒ 红。
    #[test]
    fn write_back_preserves_spawn_cloned_vitals() {
        let mut s = EntityStore::new();
        let mut e = Entity::new(1, "hero", 0, 0);
        e.hp = 37;
        e.mp = 11;
        e.exp = 5;
        s.insert(e);

        let mut t = s.spawn_cloned(1).expect("实体在场");
        assert_eq!(
            (t.hp, t.mp, t.exp),
            (37, 11, 5),
            "`spawn_cloned` 必须把组件里的 HP/MP/exp 带进生成期载体"
        );
        t.hp = 5;
        t.mp = 3;
        t.exp = 9;
        assert!(s.write_back(1, t), "写回必须命中已存在实体");

        let r = s.get(1).expect("实体在场");
        assert_eq!(
            (r.hp, r.mp, r.exp),
            (5, 3, 9),
            "`write_back` 必须把属性/成长组件整包写回（只写主组件会丢 HP/MP/exp）"
        );
    }

    /// **S3b 评审 P3（`insert_parts` 误用防护）**：`insert_parts` 的唯一入口是
    /// `EntitySpawn::into_parts`（本 crate 内无其他调用方）。若实现改用 `Transform::default()` /
    /// `Vitals::default()` / `Progression::default()` 造组件，`view_range` 会变 0、`hp` 变 0、
    /// `level` 变 0 ⇒ 本用例红。
    ///
    /// 牙：把 `insert` 里的 `spawn.into_parts()` 换成 `Component::default()` 语义 ⇒
    /// `(5,100,100,1)` 断言红。
    #[test]
    fn insert_uses_spawn_ctor_bundle_not_component_defaults() {
        let mut s = EntityStore::new();
        let id = s.insert(Entity::new(1, "x", 0, 0));
        let r = s.get(id).expect("实体在场");
        assert_eq!(
            r.view_range, 5,
            "ctor ViewRange=5（`Transform::default()` 是 0）"
        );
        assert_eq!(
            (r.hp, r.max_hp),
            (100, 100),
            "ctor HP/MaxHP=100（Default 是 0）"
        );
        assert_eq!(r.level, 1, "ctor Level=1（Default 是 0）");
        assert_eq!(r.abil, None, "ctor 不取 Abil 快照");
    }

    /// **S4a/S4b 评审 P3**：`write_back` 必须整包写回**所有**已迁移组件，不得只覆盖
    /// HP/MP/exp。此用例把 16 个迁移组件逐字段置非缺省，再 `spawn_cloned → 改副本 →
    /// write_back`，最后经 `get` 只读投影逐组断言。
    ///
    /// 牙：`write_back` 漏写任一组件（尤其 attribution/pvp/recovery/drop/spawn/ability）⇒
    /// 该组断言读到 ctor 缺省 ⇒ 红。
    #[test]
    fn write_back_covers_all_migrated_components() {
        let mut s = EntityStore::new();
        let id = s.insert(Entity::new(7, "wb-all", 0, 0));
        let mut c = s.spawn_cloned(id).expect("副本");
        // Vitals + Ability + Progression
        c.hp = 11;
        c.max_hp = 12;
        c.mp = 13;
        c.max_mp = 14;
        c.hit_speed = 15;
        c.dc = 16;
        c.ac = 17;
        c.mac = 18;
        c.hit_point = 19;
        c.speed_point = 20;
        c.anti_magic = 21;
        c.anti_poison = 22;
        c.undead_power = 23;
        c.abil = Some(BaseAbil { dc: 24, max_hp: 25 });
        c.job = 2;
        c.life_attrib = 3;
        c.level = 4;
        c.mon_exp = 26;
        c.exp = 27;
        c.max_exp = 28;
        // SpawnBinding + DropState
        c.mon_gen = Some(5);
        c.mon_gen_zen_time = 29;
        c.ctor_draws = MonsterCtorDraws {
            run_tick_delta: 30,
            base_search_time: 31,
            attack_tick_delta: 32,
            walk_tick_delta: 33,
            search_time: 34,
            meat_quality: 35,
            bo_fear_fire: true,
            zil_kill_count: 36,
        };
        c.gold = 37;
        c.angry_ring = true;
        c.no_drop_use_item = true;
        c.no_item = true;
        // CombatAttribution + Recovery + PvpState
        c.last_hiter = Some(38);
        c.exp_hitter = Some(39);
        c.last_hiter_tick = 40;
        c.exp_hitter_tick = 41;
        c.health_tick = 42;
        c.spell_tick = 43;
        c.auto_recovery_tick = 44;
        c.dear_human = Some(45);
        c.pk_point = 46;
        c.pvp_flag = true;
        c.pvp_name_color_tick = 47;
        c.die_in_fight3_zone = true;
        // Flags + LifeCycle + Appearance + MonsterClassState
        c.invisible = true;
        c.hide_mode = true;
        c.fixed_hide_mode = true;
        c.ob_mode = true;
        c.nasty_mode = true;
        c.attack_mode = 6;
        c.in_safe_zone = true;
        c.admin_mode = true;
        c.stone_mode = true;
        c.super_man = true;
        c.rush_mode = true;
        c.want_ref_msg = true;
        c.stick_mode = true;
        c.death = true;
        c.ghost = true;
        c.death_tick = 48;
        c.ghost_tick = 49;
        c.can_re_alive = true;
        c.re_alive_tick = 50;
        c.death_settled = true;
        c.die_called = true;
        c.race = 80;
        c.race_img = 51;
        c.appr = 52;
        c.light = 53;
        c.name_color = 54;
        c.come_out_value = 55;
        c.attack_range = 56;
        c.use_magic = true;
        c.danger_level = 57;
        c.n550 = 58;
        c.class_attack_tick = 59;
        c.bo_is_first = true;
        c.dig_down_tick = 60;
        c.appear_start_tick = 61;
        c.guard_direction = -1;
        c.siege_lock_count = 62;
        c.jump_time = 63;
        c.crazy_ready_mode = true;
        c.crazy_king_mode = true;
        c.crazy_count = 64;
        c.crazy_ready = 65;
        c.crazy_time = 66;
        c.old_hit_time = 67;
        c.old_walk_time = 68;
        // PlayerRunState + GroupState + Revival + BuffState
        c.luck = 69;
        c.power_rate = 70;
        c.power_rate_time = 71;
        c.kill_mon_exp_rate = 72;
        c.kill_mon_exp_rate_time = 73;
        c.exp_rate_tick = 74;
        c.random_no = 75;
        c.bo_power_item = true;
        c.power_item = 76;
        c.fire_hit_skill = true;
        c.latest_fire_hit_tick = 77;
        c.twin_hit_skill = true;
        c.latest_twin_hit_tick = 78;
        c.allow_group = true;
        c.allow_group_re_call = true;
        c.group_owner = 79;
        c.group_members = vec![80, 81];
        c.revival = true;
        c.revival_tick = 82;
        c.un_revival = true;
        c.flame_ring = true;
        c.poisoning_tick = 83;
        c.green_poisoning_point = 4;
        c.char_status = 84;
        c.char_status_ex = 85;
        c.status_time = [1; STATUS_TIME_SLOTS];
        c.transparent = true;
        c.show_hp = true;
        c.show_hp_tick = 86;
        c.show_hp_interval = 87;
        c.magic_shield = true;
        c.un_magic_shield = true;
        c.extra_abil = [2; EXTRA_ABIL_SLOTS];
        c.extra_abil_times = [88; EXTRA_ABIL_SLOTS];
        c.extra_abil_flag = [3; EXTRA_ABIL_SLOTS];
        assert!(s.write_back(id, c), "write_back 必须命中实体");

        let r = s.get(id).expect("实体在场");
        assert_eq!(
            (
                r.hp,
                r.max_hp,
                r.mp,
                r.max_mp,
                r.hit_speed,
                r.dc,
                r.ac,
                r.mac,
                r.hit_point
            ),
            (11, 12, 13, 14, 15, 16, 17, 18, 19),
            "Vitals 全字段（HP/MP/DC/AC/MAC/HitSpeed/命中）"
        );
        assert_eq!(
            (r.speed_point, r.anti_magic, r.anti_poison, r.undead_power),
            (20, 21, 22, 23),
            "Vitals 全字段（敏捷/抗性/不死）"
        );
        assert_eq!(
            r.abil,
            Some(BaseAbil { dc: 24, max_hp: 25 }),
            "AbilitySnapshot"
        );
        assert_eq!(
            (r.job, r.life_attrib, r.level, r.mon_exp, r.exp, r.max_exp),
            (2, 3, 4, 26, 27, 28),
            "Progression"
        );
        assert_eq!(
            (r.mon_gen, r.mon_gen_zen_time, r.ctor_draws.zil_kill_count),
            (Some(5), 29, 36),
            "SpawnBinding"
        );
        assert_eq!(
            (r.gold, r.angry_ring, r.no_drop_use_item, r.no_item),
            (37, true, true, true),
            "DropState"
        );
        assert_eq!(
            (
                r.last_hiter,
                r.exp_hitter,
                r.last_hiter_tick,
                r.exp_hitter_tick
            ),
            (Some(38), Some(39), 40, 41),
            "CombatAttribution"
        );
        assert_eq!(
            (r.health_tick, r.spell_tick, r.auto_recovery_tick),
            (42, 43, 44),
            "Recovery"
        );
        assert_eq!(
            (
                r.dear_human,
                r.pk_point,
                r.pvp_flag,
                r.pvp_name_color_tick,
                r.die_in_fight3_zone
            ),
            (Some(45), 46, true, 47, true),
            "PvpState"
        );
        assert!(
            r.invisible
                && r.hide_mode
                && r.fixed_hide_mode
                && r.ob_mode
                && r.nasty_mode
                && r.in_safe_zone
                && r.admin_mode
                && r.stone_mode
                && r.super_man
                && r.rush_mode
                && r.want_ref_msg
                && r.stick_mode
                && r.attack_mode == 6,
            "Flags 全字段"
        );
        assert!(
            r.death
                && r.ghost
                && r.can_re_alive
                && r.death_settled
                && r.die_called
                && r.death_tick == 48
                && r.ghost_tick == 49
                && r.re_alive_tick == 50,
            "LifeCycle 全字段"
        );
        assert_eq!(
            (r.race, r.race_img, r.appr, r.light, r.name_color),
            (80, 51, 52, 53, 54),
            "Appearance"
        );
        assert!(
            r.come_out_value == 55
                && r.attack_range == 56
                && r.use_magic
                && r.danger_level == 57
                && r.n550 == 58
                && r.class_attack_tick == 59
                && r.bo_is_first
                && r.dig_down_tick == 60
                && r.appear_start_tick == 61
                && r.guard_direction == -1
                && r.siege_lock_count == 62
                && r.jump_time == 63
                && r.crazy_ready_mode
                && r.crazy_king_mode
                && r.crazy_count == 64
                && r.crazy_ready == 65
                && r.crazy_time == 66
                && r.old_hit_time == 67
                && r.old_walk_time == 68,
            "MonsterClassState 全字段"
        );
        assert!(
            r.luck == 69
                && r.power_rate == 70
                && r.power_rate_time == 71
                && r.kill_mon_exp_rate == 72
                && r.kill_mon_exp_rate_time == 73
                && r.exp_rate_tick == 74
                && r.random_no == 75
                && r.bo_power_item
                && r.power_item == 76
                && r.fire_hit_skill
                && r.latest_fire_hit_tick == 77
                && r.twin_hit_skill
                && r.latest_twin_hit_tick == 78,
            "PlayerRunState 全字段"
        );
        assert!(
            r.allow_group
                && r.allow_group_re_call
                && r.group_owner == 79
                && *r.group_members == vec![80, 81],
            "GroupState 全字段"
        );
        assert!(
            r.revival && r.un_revival && r.flame_ring && r.revival_tick == 82,
            "Revival 全字段"
        );
        assert!(
            r.poisoning_tick == 83
                && r.green_poisoning_point == 4
                && r.char_status == 84
                && r.char_status_ex == 85
                && r.status_time == [1; STATUS_TIME_SLOTS]
                && r.transparent
                && r.show_hp
                && r.show_hp_tick == 86
                && r.show_hp_interval == 87
                && r.magic_shield
                && r.un_magic_shield
                && r.extra_abil == [2; EXTRA_ABIL_SLOTS]
                && r.extra_abil_times == [88; EXTRA_ABIL_SLOTS]
                && r.extra_abil_flag == [3; EXTRA_ABIL_SLOTS],
            "BuffState 全字段"
        );
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
