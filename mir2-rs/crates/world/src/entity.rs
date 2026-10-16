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

/// `CharacterObject.UseItems` 的佩戴位数量（`CharacterObject.cs:89`：`UseItems = new UserItem[13]`）。
pub const USE_ITEM_SLOTS: usize = 13;

/// `UserItem` 的战斗子集（`src/OpenMir2/Packets/ClientPackets/ClientUserItem.cs:14`）。
///
/// 本骨架只用到 `PlayObject.StruckDamage`（`PlayObject.cs:3003-3039`）的耐久循环：
/// 每格 `Index > 0`（= 已装备）各抽一次 `Random(8)`。`Dura` 只做减法，无消息面。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UseItemSlot {
    /// `UserItem.Index`（0 = 空位）
    pub index: u16,
    /// `UserItem.Dura`
    pub dura: u16,
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
/// 触发点（`RunAway`/`BeeQueen` 等专用怪）、`Master` 宝宝跟随之类；默认值下这些分支恒不命中
/// （与「普通 `MonsterObject`、无 master、无任务」的 oracle 执行路径一致，见 `monai` 模块头）。
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
    pub pending_struck: Vec<i32>,
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
    /// 未建模的构造抽：`BaseObject.RunTick`（`run_tick_delta`）、`MeatQuality`、`ZilKillCount`
    /// 无对应字段（AI tick 不消费它们）——登记，不假造。
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
            search_time: 3000,
            search_enemy_tick: 0,
            think_tick: 0,
            dup_mode: false,
            holy_seize: false,
            holy_seize_tick: 0,
            holy_seize_interval: 0,
            run_away_mode: false,
            run_away_start: 0,
            run_away_time: 0,
            check_royalty_tick: 0,
            pending_struck: Vec::new(),
            struck_tick: 0,
            animal: false,
            meat_quality: 0,
        }
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
    pub x: i32,
    pub y: i32,
    /// C# `ViewRange`（`BaseObject` 默认 5）
    pub view_range: i32,
    /// C# `Race`
    pub race: u8,
    /// C# `Master`（宝宝的主人）
    pub master: Option<i32>,
    /// C# `BaseObject.SlaveList`（`BaseObject.cs:141`，懒建于 `:1183`）——主人的宝宝列表（actor id，保序）。
    ///
    /// 归 `openmir2-m3-pet-slave-summon`（⑨-2）建立、`openmir2-m3-pet-slave-lifecycle`（⑨-3）清理。
    pub slave_list: Vec<i32>,
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
    /// C# `BaseObject.GreenPoisoningPoint`（`BaseObject.cs:117`，`MakePosion` `:1963` 置 `(byte)nPoint`）。
    /// DoT 扣血量 = `GreenPoisoningPoint + 1`（`ActorBuffSystem.cs:252`）。
    pub green_poisoning_point: u8,
    /// C# `BaseObject.CharStatus`（`BaseObject.cs:56`）——`GetCharStatus()` 的结果缓存
    /// （`ActorBuffSystem.Operate` 的 `boChg` 支 / `MakePosion` 写它）。
    pub char_status: i32,
    /// C# `BaseObject.CharStatusEx`（`BaseObject.cs:57`，ctor `:393` 置 0）——低 16 位并入 `GetCharStatus`。
    pub char_status_ex: i32,
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
            slave_list: Vec::new(),
            is_slave: false,
            slave_exp_level: 0,
            slave_make_level: 0,
            master_royalty_tick: 0,
            master_tick: 0,
            death: false,
            invisible: false,
            hide_mode: false,
            ghost: false,
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
            // `BaseObject.cs:385`：ctor 初值 `NameColor = 255`。
            name_color: 255,
            // `PlayObject.cs:62/943`：新角色 `Luck` 由 `RecalcAbilitys` 置 0（无 `AddAbil`），
            // `PowerRate = 100`；`BoPowerItem=false` / `PowerItem=0`。
            luck: 0,
            power_rate: 100,
            bo_power_item: false,
            power_item: 0,
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
            green_poisoning_point: 0,
            char_status: 0,
            char_status_ex: 0,
            // `PlayObject.cs:490/494`：`MagicShield`/`UnMagicShield` 构造缺省 `false`。
            magic_shield: false,
            un_magic_shield: false,
            dc: 0,
            ac: 0,
            hit_point: 0,
            speed_point: 0,
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
            ai: AnimalState::default(),
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
