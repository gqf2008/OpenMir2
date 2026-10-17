//! 宝宝 / 下属（slave）**召唤、召回与生命周期** —— 1:1 复刻 C# `BaseObject.MakeSlave` 一族 +
//! `MagicManager.MagMakeSlave` / `MagMakeSinSuSlave` / `MagMakeAngelSlave` 的**选型与门** +
//! `CheckRoyalty` / 死亡清表 / 主人死幽灵 / 诱惑受奴尾 / `GetChrColor`。
//!
//! 交付卡：`openmir2-m3-pet-slave-summon`（M3⑨-实现 2）+ `openmir2-m3-pet-slave-lifecycle`（⑨-3）。
//! 盘点依据：`mir2-rs/tests/parity/m3/M3⑨英雄宠物现状盘点报告-20261013.md` §1.B/§1.D/§1.F。
//!
//! ## 本模块负责 / 不负责
//!
//! - **负责**：召唤的**纯判定层**——技能选型（`SkeletonArray`/`DragonArray`/`Angel`）、
//!   `SlaveList.Count < nMaxMob` 门、`CheckServerMakeSlave` 的 `RM_10401` 门、`MakeSlave` 的
//!   **落库尾**（主人/`IsSlave`/`MasterRoyaltyTick`/`SlaveMakeLevel`/`SlaveExpLevel`/HP 补半/
//!   入 `SlaveList`）、`RecallSlave` 的**选靶 + `SpaceMoveGetRandXY` 取数**。
//! - **不负责**：真怪物的**构造与生成取数**。`RegenMonsterByName`（`WorldServer.MonGen.cs:382`）
//!   → `CreateMonster` 的 RNG 次序由宿主侧 `create_monster_oracle`（`bins/game-svr::mongen`）
//!   驱动；本模块通过 [`World::make_slave_adopt`] 接收**已生成**的怪 id 并落尾。
//!   ——这与 ④-B 的 MonGen 接线同形（刷怪取数在宿主、状态落 `crates/world`）。
//! - **本模块同时实现 ⑨-3 的生命周期**（卡 `openmir2-m3-pet-slave-lifecycle`）：`SlaveList` 清表
//!   （`BaseObject.Base.cs:208-217`）、`CheckRoyalty` 叛变/到期（`MonsterObject.cs:363-397`）、
//!   主人死/幽灵的 `ProcessSlaveMutiny`/`ProcessSlaveGhost`（`PlayObject.Base.cs:331-370`）、
//!   诱惑受奴落尾（`MagicManager.cs:753-770`）与 `GetChrColor` 色彩（`BaseObject.Base.cs:869-883`）。
//! - **46 分身**（`MagMakeClone`，`MagicManager.cs:1337-1341`）：`new PlayCloneObject(PlayObject)`
//!   造的是一个**玩家型克隆体**（不入 `SlaveList`、不走 `MakeSlave`），与宝宝系统不同轴。
//!   本模块提供 [`spawn_clone`]（实体 + 格占用门 + 静态可观测面），帧面/展示登记归宿主
//!   （`bins/game-svr::slave::cast_clone_spell` + `handle_spell_cmd` 的 46 支）；
//!   克隆体的 tick/AI 生命周期未建模，登记白名单 `T-51`、真机项 `not_verified`。
//!
//! ## oracle 缺陷照搬（`BUG-REPLICA`）
//!
//! 1. **`MagMakeSlave`（骷髅）选型无 `break`**（`MagicManager.cs:1316-1327`）：循环命中后
//!    **继续扫**，于是 `SkeletonArray` 里最后一个 `nHumLevel != 0 && Abil.Level >= nHumLevel`
//!    的项胜出（不是第一个）。`MagMakeSinSuSlave`（神兽，`:1282-1293`）**有** `break`。
//!    本模块逐字保留这个不对称。
//! 2. **`MagMakeSinSuSlave` 的 `nMakeLevel` 算了不用**（`:1274` `HUtil32._MIN(4, userMagic.Level)`
//!    之后从未引用，实际传的是 `userMagic.Level`）。本模块同样不落 `nMakeLevel`。
//!
//! ## 与 oracle 的建模差异（登记）
//!
//! - `CheckServerMakeSlave` 扫的是 `BaseObject.MsgQueue`（自投递消息队列）。本仓无「自投递
//!   `RM_10401`」的产者（跨服换服面未移植）⇒ [`check_server_make_slave`] 以调用方给的
//!   `pending_rm10401` 为输入，宿主今天恒传 `false`（登记：无产者，门恒放行）。
//! - `SpaceMove` 的**出帧**（`RM_CLEAROBJECTS`/`RM_CHANGEMAP`/`RM_SPACEMOVE_SHOW2`）不在此层；
//!   本模块只做**格子移动 + `SpaceMoveGetRandXY` 取数**（帧面归宿主接线片）。
//! - **⑨-3 的帧面不在本层**：`RefNameColor`（`RM_CHANGENAMECOLOR`）/`RefShowName`（`RM_USERNAME`）/
//!   `MakeGhost` 的 `DisappearA`/`RM_MASTERDIEMUTINY`/`RM_MASTERDIEGHOST`/`RM_SPIRITSUITE` 的**出帧**
//!   归 `CM_SPELL`/宿主接线片；本层实现**自投递队列本体 + 主人死触发点**（卡
//!   `openmir2-m3-pet-slave-masterdie-wire`），只改实体状态。
//! - **`MakeGhost` 的 `CanReAlive` 分支未建模**（`BaseObject.Base.cs:484-497`）：召唤物常态
//!   `Ghost=true`；`CanReAlive ⇒ Invisible=true` 变体（需 `MonGen` 字段）登记未实现。
//! - **`CrazyMode` 字段不在骨架**：`MagTamming` 的 `OpenCrazyMode`/`BreakCrazyMode` 与
//!   `MonsterObject.GetChrColor` 的 `CrazyMode/HolySeize` 前置色支登记未实现（`mag_tamming`
//!   已把 `crazy_mode` 作为纯输出给出，见 `crate::magic`）。
//! - **诱惑诱饵到期抽数未表面积化**：`mag_tamming` 已按 oracle 位置消费 `Random(caster.Level*2)`
//!   （RNG 账本不变），但该值未随 `TammingOutcome` 返回 ⇒ [`World::tame_adopt`] 需调用方传
//!   `royalty_draw`；把它接到真实 `CM_SPELL(20)` 接线片（登记）。
//! - **`ApplySlaveLevelAbilitys` 尾的 `WAbil.MAC = 0` 未建模**（`BaseObject.Abil.cs:166`）：
//!   本骨架怪物无 MAC 字段、战斗不消费。`MakeSlave` 尾 `RecalcAbilitys` 的 `WAbil.DC` /
//!   `WAbil.MaxHP` 两支与三派生类速度覆写**已落**（见 [`World::make_slave_adopt`] 第 5 步）。

use mir2_data::pet_slave::{PetSlaveConfig, SLAVE_MAX_LEVEL};
use mir2_shared::hutil;
use mir2_shared::rng::RandomNumber;

use crate::combat::{hi_byte, lo_byte};
use crate::entity::{
    Entity, ACTOR_RACE_ANIMAL, ACTOR_RACE_ELF_MONSTER, ACTOR_RACE_ELF_WARRIORMON, ACTOR_RACE_NPC,
    ACTOR_RACE_PLAY, ACTOR_RACE_WHITE_SKELETON,
};
use crate::magic::{front_position, mag_make_clone, MakeCloneInput};
use crate::world::World;

/// `MagicConst.SKILL_SKELLETON`（`src/Modules/SystemModule/Const/MagicConst.cs:72`）：17 召唤骷髅。
pub const SKILL_SKELLETON: u16 = 17;
/// `MagicConst.SKILL_TAMMING`（`MagicConst.cs:84`）：20 诱惑之光。
pub const SKILL_TAMMING: u16 = 20;
/// `MagicConst.SKILL_SINSU`（`MagicConst.cs:124`）：30 召唤神兽。
pub const SKILL_SINSU: u16 = 30;
/// `MagicConst.SKILL_ANGEL`（`MagicConst.cs:161`）：41 召唤精灵。
pub const SKILL_ANGEL: u16 = 41;
/// `MagicConst.SKILL_46`（`MagicConst.cs:175`）：46 分身（`MagMakeClone`，见模块头「不负责」）。
pub const SKILL_46: u16 = 46;

/// `Messages.RM_MASTERDIEMUTINY`（`src/OpenMir2/Messages.cs:836`）：主人死 1s 后自投递
/// （`PlayObject.Die`/`KickException` 发，`PlayObject.Operate` 收 → `ProcessSlaveMutiny`）。
/// 与 `crates/protocol/src/messages.rs:577` 同值（本层不依赖 `mir2-protocol`，登记数值同源）。
/// `Messages.RM_MAGHEALING`（`src/OpenMir2/Grobal2.cs`；`(byte)8034 = 98`）——
/// 治疗术的**目标自有延时**消息（`MagicManager.cs:216/:66`：`SendSelfDelayMsg(..., nPower, ..., 800)`）。
///
/// P3-B2：载荷 `nParam1 = nPower`（`PlayObject.Message.cs:1619-1629` 累加 `IncHealing`）。
pub const RM_MAGHEALING: u16 = 8034;
/// `Messages.RM_DOOPENHEALTH`（`crates/protocol/src/messages.rs:518`；28 的 1500ms 自投递）。
/// 与 protocol 同值（本层不依赖 `mir2-protocol`）。
pub const RM_DOOPENHEALTH: u16 = 8040;
/// `Messages.RM_TRANSPARENT`（`crates/protocol/src/messages.rs:523`；19 的 800ms 自投递）。
pub const RM_TRANSPARENT: u16 = 8045;
/// `Messages.RM_MAGIC_LVEXP`（`crates/protocol/src/messages.rs:446`；共享尾的 1000ms 自投递）。
pub const RM_MAGIC_LVEXP: u16 = 10123;
/// `Messages.RM_MAKEHOLYSEIZEMODE`（`crates/protocol/src/messages.rs:581`；16 的目标自有队列）。
pub const RM_MAKEHOLYSEIZEMODE: u16 = 60005;
/// **P3-B4** `Messages.RM_DELAYMAGIC`（`crates/protocol/src/messages.rs:510`；8021）。
/// 施法者自有延时消息：`BaseObject.Base.cs:711-729`。载荷
/// `wParam = nPower, nParam1 = MakeLong(tx,ty), nParam2 = rage, nParam3 = target.ActorId`。
pub const RM_DELAYMAGIC: u16 = 8021;
/// **P3-B4** `Messages.RM_POISON`（`crates/protocol/src/messages.rs:517`；8037）。
/// 目标自有延时消息：`BaseObject.Base.cs:740-759`。载荷
/// `wParam = PoisonState, nParam1 = nTime, nParam2 = caster.ActorId, nParam3 = nPoint`。
pub const RM_POISON: u16 = 8037;

pub const RM_MASTERDIEMUTINY: u16 = 60003;
/// `Messages.RM_MASTERDIEGHOST`（`Messages.cs:840`）：主人幽灵 1s 后自投递（→ `ProcessSlaveGhost`）。
/// 与 `crates/protocol/src/messages.rs:579` 同值。
pub const RM_MASTERDIEGHOST: u16 = 60004;
/// `PlayObject.Die()` / `MakeGhost()` 的 `SendSelfDelayMsg(..., 1000)` 延时（`PlayObject.cs:1996/2005/3527`）。
pub const MASTER_DIE_SELF_DELAY_MS: i64 = 1000;

/// `PlayCloneObject.ChrName`（`PlayCloneObject.cs:9`）：克隆体的名字常量。
///
/// `SendRefMsg(RM_TURN, …, ChrName)`（`:17`）据此带出「Clone」名尾。
pub const CLONE_NAME: &str = "Clone";

/// `PlayObject` 构造器置 `ViewRange = 12`（`PlayObject.cs:904`；`BaseObject` 缺省 5，
/// `BaseObject.cs:383`）。
pub const PLAYER_VIEW_RANGE: i32 = 12;

/// `WorldEngine.RegenMonsterByName`（`WorldServer.MonGen.cs:382-432`）的**宿主侧接缝**。
///
/// oracle 在 `BaseObject.MakeSlave`（`BaseObject.cs:1190`）里同步调用 `RegenMonsterByName`
/// → `CreateMonster`，其 RNG 次序（构造链 / `cert.Dir` / 掉落预算 / `CoolEye` / 地形移位）
/// 与 MonGen 刷怪**同一条流**。本 trait 把「造怪 + 取数」留在宿主实现里（`bins/game-svr`
/// 用 `create_monster_oracle`，与 ④-B 同源），[`summon_slave`] 只负责选型/门/落尾。
///
/// 传入 `world` = 允许宿主在同一张地图上查 `CanWalk` 并落实体；传入 `rng` = 宿主必须
/// 用**这条**流抽数（`summon_slave` 从 `world.rng` 移出并原样交回）。
/// `x`/`y` = `GetFrontPosition` 的结果（oracle 的 `short` 口径）。
pub trait SlaveSpawner {
    fn spawn(
        &mut self,
        world: &mut World,
        rng: &mut RandomNumber,
        map_id: usize,
        name: &str,
        x: i16,
        y: i16,
    ) -> Option<i32>;
}

/// [`summon_slave`] 的结果（对应 oracle `MagMakeSlave` 一族的返回值 + 神兽的回召分支）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummonOutcome {
    /// `MakeSlave` 成功 ⇒ `MagMake*Slave` 返回 `true`。
    Spawned(i32),
    /// 神兽（`SKILL_SINSU`）：`MakeSlave` 返回 null ⇒ oracle 改调 `RecallSlave(sMonName)`。
    Recalled(Option<i32>),
    /// `CheckServerMakeSlave()` 为真（有 `RM_10401`）⇒ oracle 整块跳过。
    Blocked,
    /// `SlaveList.Count >= nMaxMob`（或宿主造怪失败）⇒ `MakeSlave` 返回 null。
    Rejected,
    /// 非召唤技能（调用方不该走到这里）。
    NotSummonSkill,
    /// 主人 actor 不在世界。
    NoCaster,
}

/// `MagicManager.MagMakeSlave`/`MagMakeSinSuSlave`/`MagMakeAngelSlave`（`MagicManager.cs:1269-1352`）
/// 的**完整编排**：`CheckServerMakeSlave` 门 → 选型 → `SlaveList.Count < nMaxMob` 门 →
/// `GetFrontPosition` → `RegenMonsterByName`（经 [`SlaveSpawner`]）→ `MakeSlave` 落尾；
/// 神兽另有 `MakeSlave == null ⇒ RecallSlave` 的回召分支。
///
/// **取数次序契约**：本函数从 `world.rng` 移出随机流交给 `spawner`（[`SlaveSpawner::spawn`]），
/// 宿主在其内按 oracle `CreateMonster` 次序抽数；`summon_slave` 自身不抽任何随机数。
/// `RecallSlave` 的 `SpaceMoveGetRandXY` 取数在 [`World::recall_slave`] 里，走同一条流。
///
/// `caster_dir` = `BaseObject.Dir`（`GetFrontPosition` 的入参）。**玩家在宿主侧的权威 `Dir` 在展示表**
/// （`bins/game-svr` 的 `DisplayTable`，客户端指令维护），`Entity.ai.dir` 只对怪物由 AI 维护
/// —— 故这里**显式传入**，不代宿主猜。
pub fn summon_slave<S: SlaveSpawner>(
    world: &mut World,
    caster_id: i32,
    caster_dir: u8,
    magic_id: u16,
    magic_level: u8,
    cfg: &PetSlaveConfig,
    spawner: &mut S,
) -> SummonOutcome {
    // `MagMake*Slave` 首句：`if (!playObject.CheckServerMakeSlave()) { … }`。
    if check_server_make_slave(false) {
        return SummonOutcome::Blocked;
    }
    let Some(caster_level) = world.entities.get(caster_id).map(|e| i32::from(e.level)) else {
        return SummonOutcome::NoCaster;
    };
    let plan = match magic_id {
        SKILL_SKELLETON => mag_make_slave_plan(cfg, caster_level, magic_level),
        SKILL_SINSU => mag_make_sinsu_slave_plan(cfg, caster_level, magic_level),
        SKILL_ANGEL => mag_make_angel_slave_plan(cfg, magic_level),
        _ => return SummonOutcome::NotSummonSkill,
    };
    // `MakeSlave` 首句门：`if (SlaveList.Count < nMaxMob)`。门不过 ⇒ `MakeSlave` 返回 null，
    // 但神兽仍有 `else { RecallSlave(...) }` 分支（`MagicManager.cs:1300-1304`）。
    let gate_ok = make_slave_gate(world.slave_count(caster_id), plan.count);
    let spawned = if gate_ok {
        let (cx, cy, map_id) = {
            let Some(caster) = world.entities.get(caster_id) else {
                return SummonOutcome::NoCaster;
            };
            (caster.x, caster.y, caster.map_id)
        };
        let Some((width, height)) = world.maps.get(map_id).map(|m| (m.width, m.height)) else {
            return SummonOutcome::Rejected;
        };
        // `GetFrontPosition(ref nX, ref nY)`（`BaseObject.cs:1187`）——oracle 是 `short`。
        let (fx, fy) = front_position(cx, cy, caster_dir, width, height);
        let (fx, fy) = (fx as i16, fy as i16);
        // `RegenMonsterByName`：宿主在**同一条** `world.rng` 上造怪（oracle `M2Share.RandomNumber`）。
        let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(0));
        let spawned = spawner.spawn(world, &mut rng, map_id, &plan.mon_name, fx, fy);
        world.rng = rng;
        spawned
    } else {
        None
    };
    let Some(mon_id) = spawned else {
        // oracle `MagMakeSinSuSlave`：`MakeSlave` 返回 null ⇒ `playObject.RecallSlave(sMonName)`；
        // `MagMakeSlave`（骷髅）**没有**这个 else 支（`MagicManager.cs:1300-1304` vs `:1329-1333`）。
        if magic_id == SKILL_SINSU {
            let recalled = world.recall_slave(
                caster_id,
                caster_dir,
                &plan.mon_name,
                &cfg.dragon,
                &cfg.dragon1,
            );
            return SummonOutcome::Recalled(recalled);
        }
        return SummonOutcome::Rejected;
    };
    // `MakeSlave` 落尾（`BaseObject.cs:1192-1208`）。`DwRoyaltySec`（`MagicManager.cs:22`）。
    if world.make_slave_adopt(
        caster_id,
        mon_id,
        plan.make_level,
        plan.exp_level,
        mir2_data::pet_slave::DEFAULT_ROYALTY_SEC,
    ) {
        SummonOutcome::Spawned(mon_id)
    } else {
        SummonOutcome::Rejected
    }
}

/// `MagicManager.MagMakeSlave` / `MagMakeSinSuSlave` / `MagMakeAngelSlave` 选出的召唤计划。
///
/// 字段名对应 oracle：`sMonName` / `nExpLevel` / `nCount`（+ 统一传 `userMagic.Level` 的 `nMakeLevel`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MakeSlavePlan {
    /// `sMonName`（`Config.Skeleton`/`Dragon`/`Angel` 或数组项；C# `null` 表示空串）。
    pub mon_name: String,
    /// `nMakeLevel`（三个技能都传 `userMagic.Level`）。
    pub make_level: u8,
    /// `nExpLevel`（`SlaveExpLevel` 初值）。
    pub exp_level: i32,
    /// `nCount`（`MakeSlave` 的 `nMaxMob`）。
    pub count: i32,
}

/// `MagicManager.MagMakeSlave`（`MagicManager.cs:1308-1335`）：召唤骷髅（`SKILL_SKELLETON=17`）。
///
/// 逐字：
/// 1. `sMonName = Config.Skeleton`、`nExpLevel = userMagic.Level`、`nCount = Config.SkeletonCount`；
/// 2. `for i in 0..SkeletonArray.Length`：
///    - `if (SkeletonArray[i].nHumLevel == 0) break;`
///    - `if (Abil.Level >= SkeletonArray[i].nHumLevel) { 覆盖三项 }` —— **无 `break`**（BUG-REPLICA ①）；
/// 3. 返回 `(sMonName, userMagic.Level, nExpLevel, nCount)`。
#[must_use]
pub fn mag_make_slave_plan(
    cfg: &PetSlaveConfig,
    player_level: i32,
    magic_level: u8,
) -> MakeSlavePlan {
    let mut mon_name = cfg.skeleton.clone();
    let mut exp_level = i32::from(magic_level);
    let mut count = cfg.skeleton_count;
    for e in &cfg.skeleton_array {
        if e.n_hum_level == 0 {
            break;
        }
        if player_level >= e.n_hum_level {
            mon_name = e.s_mon_name.clone();
            exp_level = e.n_level;
            count = e.n_count;
            // BUG-REPLICA ①：oracle 此处**没有** `break`。
        }
    }
    MakeSlavePlan {
        mon_name,
        make_level: magic_level,
        exp_level,
        count,
    }
}

/// `MagicManager.MagMakeSinSuSlave`（`MagicManager.cs:1269-1306`）：召唤神兽（`SKILL_SINSU=30`）。
///
/// 逐字：
/// 1. `sMonName = Config.Dragon`、`nExpLevel = userMagic.Level`、`nMakeLevel = MIN(4, userMagic.Level)`
///    （**算了不用**，BUG-REPLICA ②）、`if (nExpLevel > 4) sMonName = Dragon + ((nExpLevel/4)+4)`、
///    `nCount = Config.DragonCount`；
/// 2. `for i in 0..DragonArray.Length`：`break on nHumLevel==0`；命中 `Abil.Level >= nHumLevel`
///    覆盖三项并 **`break`**（与骷髅相反）；
/// 3. 返回计划的 `nMaxMob = nCount`。
#[must_use]
pub fn mag_make_sinsu_slave_plan(
    cfg: &PetSlaveConfig,
    player_level: i32,
    magic_level: u8,
) -> MakeSlavePlan {
    let mut mon_name = cfg.dragon.clone();
    let mut exp_level = i32::from(magic_level);
    // BUG-REPLICA ②：oracle 的 `nMakeLevel` 只算不传（`MakeSlave` 的第二个实参是 `userMagic.Level`）。
    let _n_make_level = i32::from(magic_level).min(4);
    if exp_level > 4 {
        mon_name = format!("{}{}", cfg.dragon, (exp_level / 4) + 4);
    }
    let mut count = cfg.dragon_count;
    for e in &cfg.dragon_array {
        if e.n_hum_level == 0 {
            break;
        }
        if player_level >= e.n_hum_level {
            mon_name = e.s_mon_name.clone();
            exp_level = e.n_level;
            count = e.n_count;
            break;
        }
    }
    MakeSlavePlan {
        mon_name,
        make_level: magic_level,
        exp_level,
        count,
    }
}

/// `MagicManager.MagMakeAngelSlave`（`MagicManager.cs:1343-1352`）：召唤精灵（`SKILL_ANGEL=41`）。
///
/// `MakeSlave(Config.Angel, userMagic.Level, userMagic.Level, 1, DwRoyaltySec)` —— `nMaxMob` 恒 `1`。
#[must_use]
pub fn mag_make_angel_slave_plan(cfg: &PetSlaveConfig, magic_level: u8) -> MakeSlavePlan {
    MakeSlavePlan {
        mon_name: cfg.angel.clone(),
        make_level: magic_level,
        exp_level: i32::from(magic_level),
        count: 1,
    }
}

/// `BaseObject.MakeSlave` 的 `SlaveList.Count < nMaxMob` 门（`BaseObject.cs:1183-1185`）。
///
/// `SlaveList` 懒建（`:1181-1183`）⇒ 空表计数为 0；`nMaxMob` 来自选型计划的 `nCount`
/// （现网默认 `1`）。
#[must_use]
pub fn make_slave_gate(slave_count: usize, n_max_mob: i32) -> bool {
    // oracle `if (SlaveList.Count < nMaxMob)`：`Count` 是 `int`，`nMaxMob` 也是 `int`。
    i32::try_from(slave_count).unwrap_or(i32::MAX) < n_max_mob
}

/// `BaseObject.CheckServerMakeSlave`（`BaseObject.cs:1979-1994`）：`MsgQueue` 里是否有 `RM_10401`
/// （正跨服）⇒ 有则**不许**本服召唤。
///
/// 本仓无 `RM_10401` 产者（跨服换服未移植）⇒ 宿主恒传 `pending_rm10401=false`（门恒放行）；
/// 本函数保留完整判定以便 ⑨ 跨服片接入。
#[must_use]
pub fn check_server_make_slave(pending_rm10401: bool) -> bool {
    pending_rm10401
}

/// `BaseObject.RecallSlave`（`BaseObject.cs:1850-1876`）的**选靶**：
/// 从 `SlaveList` **末尾往前**找第一个匹配者。
///
/// `nFlag = (sSlaveName == Config.Dragon, OrdinalIgnoreCase) ? 1 : -1`；
/// - `nFlag==1`：命中 `ChrName == Config.Dragon` **或** `ChrName == Config.Dragon1`（`==`，区分大小写）；
/// - 否则：命中 `ChrName == sSlaveName`（区分大小写）。
///
/// `slaves` 按 `SlaveList` 原序（[`World`] 的 `Entity::slave_list` 同序）；返回命中的 actor id。
#[must_use]
pub fn recall_slave_target(
    slaves: &[(i32, String)],
    slave_name: &str,
    dragon: &str,
    dragon1: &str,
) -> Option<i32> {
    // C# `string.Compare(a, b, StringComparison.OrdinalIgnoreCase) == 0`。
    let dragon_mode = eq_ordinal_ignore_case(slave_name, dragon);
    for (id, name) in slaves.iter().rev() {
        if dragon_mode {
            if name == dragon || name == dragon1 {
                return Some(*id);
            }
        } else if name == slave_name {
            return Some(*id);
        }
    }
    None
}

/// C# `string.Compare(a, b, StringComparison.OrdinalIgnoreCase) == 0` 的 ASCII 等价
/// （中文无大小写 ⇒ 与 `==` 同）。
fn eq_ordinal_ignore_case(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// `BaseObject.SpaceMoveGetRandXY`：**已移入 [`crate::space_move`]**（独立模块——理由见该文件头：
/// 该取数被召回落与商人随机移动两个 oracle 函数共用，按站点归组需要函数对齐；本文件只剩
/// `PlayObject.ProcessSlaveMutiny` 的抽数 ⇒ 归 `slave_mutiny` 组）。
pub use crate::space_move::space_move_get_rand_xy;

/// `HUtil32.MakeWord(ushort bLow, ushort bHigh)`（`src/OpenMir2/HUtil32.cs:66-69`）：
/// `(ushort)(bLow | bHigh << 8)` —— 先按 `int` 左移再截 16 位 ⇒ `bHigh` 只有低 8 位生效。
///
/// 注意与 [`mir2_shared::hutil::make_word`]（`MakeWord(byte, byte)`）是**不同重载**：
/// `ApplySlaveLevelAbilitys` 里 `(ushort)` 强转过的 `High` 走的是本重载。
fn make_word_u16(low: u16, high: u16) -> u16 {
    (u32::from(low) | (u32::from(high) << 8)) as u16
}

/// `BaseObject.ApplySlaveLevelAbilitys`（`src/M2Server/Actor/BaseObject.Abil.cs:133-170`）——
/// oracle 在 `BaseObject.RecalcAbilitys()` 末尾 `if (Race >= ActorRace.Animal)`
/// （`BaseObject.Abil.cs:85-88`）调用。
///
/// **为什么 `MakeSlave` 尾的 `RecalcAbilitys` 不是 no-op**：`RecalcAbilitys`（`Abil.cs:15-94`）
/// 先 `WAbil = Abil.Clone()` 再把 `WAbil.HP/MP` 还原为调用前的值（`:19-23`），
/// `AddAbil = new AddAbility()` 全 0 ⇒ `WAbil.MaxHP = Abil.MaxHP + 0`（`:70`）、
/// `WAbil.DC = Abil.DC + 0`（`:75`），最后由本函数按 `SlaveExpLevel` 改写 `WAbil.DC` /
/// `WAbil.MaxHP`。于是 `MakeSlave` 尾的 HP 补半（`WAbil.HP < WAbil.MaxHP ⇒
/// HP += (MaxHP - HP) / 2`，`BaseObject.cs:1198-1201`）用的是**改过**的 `MaxHP`。
/// 本骨架曾按「基类怪 = `WAbil=Abil.Clone()` ⇒ no-op」处理（⑨-2 `slave.rs` 旧注释），
/// 与 oracle 分叉 —— 即 reviewer-5 ⑨-2 `M-1` 所指缺口（⑨-3 未真落）。
///
/// `Abil` 取「当前 `dc` / `max_hp`」：召唤路径上被落尾的怪是 `RegenMonsterByName` 刚造的
/// （`WorldServer.MonGen.cs:742` `cert.WAbil = cert.Abil`，此后未被任何
/// `RecalcAbilitys`/伤害改过），故当前态即 `Abil`，等价。
/// **未建模**：else 支尾部的 `WAbil.MAC = 0` —— 本骨架怪物无 MAC 字段、战斗不消费（登记，
/// 不假造）；`RecalcAbilitys` 其余字段（`AntiPoison=0`/`AntiMagic=1`/`Weight*`…）本骨架亦
/// 无对应怪物字段。
fn apply_slave_level_abilitys(mon: &mut Entity) {
    // `if (Race >= ActorRace.Animal)`（`BaseObject.Abil.cs:85-88`，`Animal = 50`）。
    if mon.race < ACTOR_RACE_ANIMAL {
        return;
    }
    let exp = i32::from(mon.slave_exp_level);
    // `Abil.MaxHP` / `Abil.DC` = 当前 `max_hp` / `dc`（见上）。
    let abil_max_hp = i32::from(mon.max_hp);
    let abil_dc = mon.dc;
    if matches!(
        mon.race,
        ACTOR_RACE_WHITE_SKELETON | ACTOR_RACE_ELF_MONSTER | ACTOR_RACE_ELF_WARRIORMON
    ) {
        // `WAbil.DC = MakeWord(LoByte(WAbil.DC), HiByte(Abil.DC));` —— 此刻两者相等 ⇒ 恒等。
        // `WAbil.DC = MakeWord(LoByte(WAbil.DC), (ushort)(int)Math.Round(HiByte(WAbil.DC)
        //   + (3 * (0.3 + slaveExpLevel * 0.1) * slaveExpLevel)));`
        let hi = (f64::from(hi_byte(abil_dc))
            + 3.0 * (0.3 + f64::from(exp) * 0.1) * f64::from(exp))
        .round_ties_even();
        // C# `(int)` 截断再 `(ushort)`。
        let hi = hi as i32 as u16;
        let new_dc = make_word_u16(u16::from(lo_byte(abil_dc)), hi);
        // `chp = (ushort)(chp + Math.Round(Abil.MaxHP * (0.3 + slaveExpLevel * 0.1)) * slaveExpLevel);`
        // （`chp` 初值 0）
        let chp = (f64::from(abil_max_hp) * (0.3 + f64::from(exp) * 0.1)).round_ties_even()
            * f64::from(exp);
        // C# `(ushort)double`（unchecked：截断 + 取模）；再 `chp = (ushort)(Abil.MaxHP + chp);`
        let chp = chp as i64 as u16;
        let chp = (i64::from(abil_max_hp) + i64::from(chp)) as u16;
        mon.dc = i32::from(new_dc);
        // `if (slaveExpLevel > 0) WAbil.MaxHP = chp; else WAbil.MaxHP = Abil.MaxHP;`
        if exp > 0 {
            mon.max_hp = chp;
        }
    } else if mon.master.is_some() {
        // `if (Master != null) { ... }`：
        // `WAbil.DC = MakeWord(LoByte(WAbil.DC), (ushort)Math.Abs(HiByte(WAbil.DC)
        //   + (2 * slaveExpLevel)));`
        let hi = i32::from(hi_byte(abil_dc)) + 2 * exp;
        let new_dc = make_word_u16(u16::from(lo_byte(abil_dc)), i32::abs(hi) as u16);
        // `chp = (ushort)(chp + Math.Round(Abil.MaxHP * 0.15) * slaveExpLevel);`（`chp = Abil.MaxHP`）
        let chp = (f64::from(abil_max_hp) * 0.15).round_ties_even() * f64::from(exp);
        let chp = (i64::from(abil_max_hp) + chp as i64) as u16;
        // `WAbil.MaxHP = (ushort)HUtil32._MIN(Abil.MaxHP + 60 * slaveExpLevel, chp);`
        let capped = (abil_max_hp + 60 * exp).min(i32::from(chp));
        mon.dc = i32::from(new_dc);
        mon.max_hp = capped as u16;
        // `WAbil.MAC = 0;` —— 未建模（登记，见函数文档）。
    }
}

/// 三个派生召唤物 `RecalcAbilitys` 覆写的尾巴（oracle `base.RecalcAbilitys()` 之后调
/// `ResetElfMon()`）：
/// - `WhiteSkeleton.RecalcAbilitys`（`Monster/Monsters/WhiteSkeleton.cs:21-27`）：
///   `NextHitTime = 3000 - SlaveMakeLevel*600`、`WalkSpeed = 1200 - SlaveMakeLevel*250`、
///   `WalkTick = GetTickCount() + 2000`；
/// - `ElfMonster.RecalcAbilitys` → `ResetElfMon`（`ElfMonster.cs:31-41`）：
///   `WalkSpeed = 500 - SlaveMakeLevel*50`、`WalkTick = GetTickCount() + 2000`；
/// - `ElfWarriorMonster.RecalcAbilitys` → `ResetElfMon`（`ElfWarriorMonster.cs:35-39,105-110`）：
///   `NextHitTime = 1500 - SlaveMakeLevel*100`、`WalkSpeed = 500 - SlaveMakeLevel*50`、
///   `WalkTick = GetTickCount() + 2000`。
///
/// 顺序：oracle 先 `base.RecalcAbilitys()`（含 [`apply_slave_level_abilitys`]），再本覆写。
fn apply_derived_slave_recalc(mon: &mut Entity, now: i64) {
    let lvl = i64::from(mon.slave_make_level);
    match mon.race {
        ACTOR_RACE_WHITE_SKELETON => {
            mon.ai.next_hit_time = 3000 - lvl * 600;
            mon.ai.walk_speed = 1200 - lvl * 250;
            mon.ai.walk_tick = now + 2000;
        }
        ACTOR_RACE_ELF_MONSTER => {
            mon.ai.walk_speed = 500 - lvl * 50;
            mon.ai.walk_tick = now + 2000;
        }
        ACTOR_RACE_ELF_WARRIORMON => {
            mon.ai.next_hit_time = 1500 - lvl * 100;
            mon.ai.walk_speed = 500 - lvl * 50;
            mon.ai.walk_tick = now + 2000;
        }
        _ => {}
    }
}

impl World {
    /// `BaseObject.SlaveList.Count`（`BaseObject.cs:141`）——懒建列表的空表 = 0。
    #[must_use]
    pub fn slave_count(&self, caster_id: i32) -> usize {
        self.entities
            .get(caster_id)
            .map_or(0, |e| e.slave_list.len())
    }

    /// `BaseObject.MakeSlave`（`BaseObject.cs:1179-1209`）的**落库尾**。
    ///
    /// 调用于宿主侧 `RegenMonsterByName` 成功之后（`mon_id` = 新生成的怪）：
    /// 1. `monObj.Master = this`；2. `monObj.IsSlave = true`；
    /// 3. `monObj.MasterRoyaltyTick = GetTickCount() + dwRoyaltySec*1000`；
    /// 4. `monObj.SlaveMakeLevel = (byte)nMakeLevel`、`monObj.SlaveExpLevel = (byte)nExpLevel`；
    /// 5. `monObj.RecalcAbilitys()`（`BaseObject.cs:1197`）：base 的 `WAbil = Abil.Clone()`
    ///    部分对本骨架是恒等（`dc`/`max_hp` 即 `Abil`），但末尾
    ///    `if (Race >= ActorRace.Animal)` 的 [`apply_slave_level_abilitys`]
    ///    （`BaseObject.Abil.cs:133-170`）会按 `SlaveExpLevel` 改 `WAbil.DC`/`WAbil.MaxHP`；
    ///    三个派生召唤物（变异骷髅/神兽两形态）再由 [`apply_derived_slave_recalc`]
    ///    改 `WalkSpeed`/`NextHitTime`/`WalkTick`；
    /// 6. `if (WAbil.HP < WAbil.MaxHP) HP += (MaxHP - HP) / 2`（用第 5 步改过的 `MaxHP`）；
    /// 7. `RefNameColor()`（`BaseObject.cs:730`，发 `RM_CHANGENAMECOLOR`；色彩归 ⑨-3）；
    /// 8. `SlaveList.Add(monObj)`。
    ///
    /// 返回 `true` = 落尾完成（oracle 返回非 null）。
    pub fn make_slave_adopt(
        &mut self,
        caster_id: i32,
        mon_id: i32,
        make_level: u8,
        exp_level: i32,
        royalty_sec: i32,
    ) -> bool {
        let Some(caster) = self.entities.get(caster_id) else {
            return false;
        };
        if caster.race != ACTOR_RACE_PLAY {
            // oracle 里 `MakeSlave` 是 `BaseObject` 方法，实参 playObject 恒为玩家；本骨架只
            // 允许玩家持有 SlaveList（与 `PlayObject.SlaveList` 的语义一致）。
            return false;
        }
        let now = self.clock_ms;
        let Some(mon) = self.entities.get_mut(mon_id) else {
            return false;
        };
        mon.master = Some(caster_id);
        mon.is_slave = true;
        // `MasterRoyaltyTick = GetTickCount() + (dwRoyaltySec * 1000)`（`BaseObject.cs:1193`）。
        mon.master_royalty_tick = now + i64::from(royalty_sec) * 1000;
        mon.slave_make_level = make_level;
        // C# `(byte)nExpLevel` 截断。
        mon.slave_exp_level = exp_level as u8;
        // 5. `monObj.RecalcAbilitys()`：base（`WAbil = Abil.Clone()` 的恒等部分 +
        //    `ApplySlaveLevelAbilitys`）→ 派生覆写速度。顺序与 oracle 一致。
        apply_slave_level_abilitys(mon);
        apply_derived_slave_recalc(mon, now);
        // `BaseObject.RecalcAbilitys`（`BaseObject.Abil.cs:30` 置 1、`:67` 累加
        // `AddAbil.AntiMagic`）：宝宝/召唤物在此刻才离开 ctor 的 0，变成 `1 + AddAbil.AntiMagic`。
        // 怪物 `AddAbil` 在本骨架内为空 ⇒ **1**。普通 `MonGen` 生成路径不调 `RecalcAbilitys`
        // ⇒ 那边保持 0（`BaseObject.cs:390`）。
        mon.anti_magic = 1;
        // 6. `if (monObj.WAbil.HP < monObj.WAbil.MaxHP) HP += (MaxHP - HP) / 2;`（`:1198-1201`）。
        if mon.hp < mon.max_hp {
            let delta = (i32::from(mon.max_hp) - i32::from(mon.hp)) / 2;
            mon.hp = mon.hp.saturating_add(u16::try_from(delta).unwrap_or(0));
        }
        // `SlaveList.Add(monObj)`（`:1207`）。
        self.entities
            .get_mut(caster_id)
            .expect("caster 已确认存在")
            .slave_list
            .push(mon_id);
        true
    }

    /// `BaseObject.RecallSlave`（`BaseObject.cs:1850-1876`）+ `SpaceMove`（`:1103-1166`）的
    /// **状态/取数**部分：选靶 → `GetFrontPosition` → `SpaceMoveGetRandXY`（消耗 `world.rng`）
    /// → 把目标移格。
    ///
    /// 返回被移动的 slave id（`None` = `SlaveList` 无匹配，oracle 什么都做）。
    /// `RecalcAbilitys`/出帧不在此层（见模块头）。
    ///
    /// `caster_dir` = `BaseObject.Dir`（玩家权威 `Dir` 在宿主展示表，见 [`summon_slave`] 的说明）。
    pub fn recall_slave(
        &mut self,
        caster_id: i32,
        caster_dir: u8,
        slave_name: &str,
        dragon: &str,
        dragon1: &str,
    ) -> Option<i32> {
        let (cx, cy, map_id, slaves) = {
            let caster = self.entities.get(caster_id)?;
            let slaves: Vec<(i32, String)> = caster
                .slave_list
                .iter()
                .filter_map(|id| self.entities.get(*id).map(|e| (*id, e.name.clone())))
                .collect();
            (caster.x, caster.y, caster.map_id, slaves)
        };
        let target = recall_slave_target(&slaves, slave_name, dragon, dragon1)?;
        let (width, height) = {
            let map = self.maps.get(map_id)?;
            (map.width, map.height)
        };
        let (fx, fy) = front_position(cx, cy, caster_dir, width, height);
        // `SpaceMoveGetRandXY(Envir, ref nX, ref nY)`（`BaseObject.cs:1128`）。
        let map = self.maps.get(map_id)?;
        let found = space_move_get_rand_xy(&mut self.rng, width, height, fx, fy, |x, y| {
            map.get_cell_info(x, y).1
        });
        let (nx, ny) = found?;
        // `Envir.DeleteFromMap` + `Envir.AddMapObject`（`BaseObject.cs:1114/1132`）。
        let moved = {
            let map = self.maps.get_mut(map_id)?;
            map.move_object(
                crate::map::MoveRequest {
                    from_x: self.entities.get(target)?.x,
                    from_y: self.entities.get(target)?.y,
                    actor_id: target,
                    to_x: nx,
                    to_y: ny,
                    ignore_block: true,
                    add_time: self.clock_ms,
                },
                |_| false,
            )
        };
        if !moved {
            return None;
        }
        if let Some(e) = self.entities.get_mut(target) {
            e.x = nx;
            e.y = ny;
            e.map_id = map_id;
        }
        Some(target)
    }
}

/// 宝宝实体判据（供 ⑨-3 / 宿主出名字色用）：`AnimalObject.IsSlave`（`AnimalObject.cs:54`）。
#[must_use]
pub fn is_slave(e: &Entity) -> bool {
    e.is_slave
}

/// `BaseObject.Base.cs:877` 的色表上限（`SlaveExpLevel <= Grobal2.SlaveMaxLevel`）。
#[must_use]
pub fn slave_color_level_index(exp_level: u8) -> Option<usize> {
    if exp_level <= SLAVE_MAX_LEVEL {
        Some(usize::from(exp_level))
    } else {
        None
    }
}

// =====================================================================================
// 生命周期（卡 `openmir2-m3-pet-slave-lifecycle`，⑨-3）
//
// 覆盖 oracle：
// - 死亡/幽灵/换主清表：`BaseObject.Base.cs:208-217`（`BaseObject.Run` 尾）；
// - 叛变到期：`MonsterObject.CheckRoyalty`（`MonsterObject.cs:363-397`，`Run` 尾 `:352` 调）；
// - 主人死 ⇒ `ProcessSlaveMutiny`（`PlayObject.Base.cs:331-357`）；
// - 主人幽灵 ⇒ `ProcessSlaveGhost`（`PlayObject.Base.cs:360-370`）；
// - 诱惑受奴落尾：`MagicManager.MagTamming`（`MagicManager.cs:704-825` 受奴支 `:753-770`）；
// - 名字颜色：`GetChrColor`（`BaseObject.Base.cs:869-883`）。
//
// ## oracle 缺陷照搬（`BUG-REPLICA`）
//
// 1. **色表越界门**：`GetChrColor`（`BaseObject.Base.cs:877-881`）用门 `slaveExpLevel <= SlaveMaxLevel(50)`
//    去索引 15 项色表 `SlaveColor[slaveExpLevel]` ⇒ `15..=50` 会 `IndexOutOfRangeException`（latent）。
//    Rust `get_chr_color` 用 `slice::get` 落到 `GetNameColor` 兜底（见函数注释）；正常数据不可达
//    （召唤 `SlaveExpLevel` 来自 `SkeletonArray/DragonArray.nLevel`；`ProcessSlaveMutiny` 置 15 但
//    同时把 `Master=null` ⇒ `GetChrColor` 走不到色表支）。
// 2. **`MasterDieMutinySpeed == 0` 除零**：`ProcessSlaveMutiny`（`PlayObject.Base.cs:350`）
//    `WalkSpeed = WalkSpeed / Config.MasterDieMutinySpeed`。C# 配置缺陷（`ServerConf.cs:521` 读的是
//    `MasterDieMutinyPower` 键）下若 `MasterDieMutinyPower=0` ⇒ 除零异常；Rust 侧 `speed != 0` 才除
//    （不可达分支，登记不冒充已实现）。
// 3. **`GainSlaveExp` 是死代码**（`MonsterObject.cs:71-84`，全仓无调用点）⇒ 本片**不实现**宝宝击杀涨级，
//    照搬「击杀不涨 `SlaveExpLevel`」。
// 4. **`RM_MASTERDIEGHOST` 恒不可达**（卡 `openmir2-m3-pet-slave-masterdie-wire`）：
//    `PlayObject.MakeGhost()`（`PlayObject.cs:3524-3527`）在 `base.MakeGhost()`
//    （`BaseObject.Base.cs:484-497`）**之后**判 `if (Ghost) SendSelfDelayMsg(RM_MASTERDIEGHOST,…,1000)`；
//    而 `base.MakeGhost()` 已把 `Ghost = true`（玩家 `CanReAlive == false`），`SendSelfDelayMsg`
//    首行 `if (!Ghost) return;`（`BaseObject.Message.cs:190`）随即拦截 ⇒ **消息永不入队**、
//    `ProcessSlaveGhost`（`PlayObject.Base.cs:361-369`）在 oracle 里是**死代码**。
//    Rust `World::send_master_die_ghost` 逐字保留两道门（进队为空），`deliver_self_delay_msgs`
//    保留 `RM_MASTERDIEGHOST` 支以与 oracle 同形（唯 `RM_MASTERDIEMUTINY` 可达）。

/// `MonsterObject.CheckRoyalty` 的检查间隔（`MonsterObject.cs:365` `> 10000`）。
pub const CHECK_ROYALTY_INTERVAL: i64 = 10_000;

/// `CheckRoyalty` 的诱饵到期（`MonsterObject.cs:390` `12 * 60 * 60 * 1000`）。
pub const MASTER_MUTINY_DEATH_MS: i64 = 12 * 60 * 60 * 1000;

/// `BaseObject.Run` 的宝宝列表清理（`BaseObject.Base.cs:208-217`）：从列表**末尾往前**删掉
/// `Death || Ghost || Master != this` 的项。
///
/// 本骨架 `SlaveList` 存的是 actor id（`Entity::slave_list`）；`entities` 里已不存在的 id
/// 等价于 oracle 的「对象已不是本主人的宝宝」⇒ 一并移除（登记：C# 持有强引用，实体离图仍在表里；
/// 本骨架 `EntityStore::remove` 后 id 可能被代际复用于**另一实体**，故缺失即视为换主）。
impl World {
    /// 对单个主人跑一次清表（空表立即返回）。
    pub fn slave_list_cleanup(&mut self, master_id: i32) {
        let Some(master) = self.entities.get(master_id) else {
            return;
        };
        if master.slave_list.is_empty() {
            return;
        }
        let mut list = master.slave_list.clone();
        for i in (0..list.len()).rev() {
            let sid = list[i];
            let remove = match self.entities.get(sid) {
                Some(s) => s.death || s.ghost || s.master != Some(master_id),
                None => true,
            };
            if remove {
                list.remove(i);
            }
        }
        if let Some(m) = self.entities.get_mut(master_id) {
            m.slave_list = list;
        }
    }

    /// `MagTamming` 受奴落尾（`MagicManager.cs:753-770`）。
    ///
    /// `royalty_draw` = oracle `Random(playObject.Abil.Level * 2)` 的抽数结果（诱饵到期时刻的随机分量）。
    /// 该抽数的**位置**已在 [`crate::magic::mag_tamming`] 里逐字保留（RNG 账本不变）；把它的值
    /// 表面积化并接到本函数归 `CM_SPELL` 接线片（本卡只做状态落尾，见证据 README「未接」）。
    ///
    /// 逐条对应 oracle：`Master = playObject` → `MasterRoyaltyTick` → `SlaveMakeLevel = magicLevel` →
    /// `if (MasterTick == 0) MasterTick = now` → `BreakHolySeizeMode()` → `WalkSpeed/NextHitTime` 封顶
    /// → `RefShowName()`（帧，不在此层）→ `playObject.SlaveList.Add(targetObject)`。
    /// `CanReAlive`/`MonGen.ActiveCount`/`BreakCrazyMode` 未建模（登记）。
    pub fn tame_adopt(
        &mut self,
        caster_id: i32,
        target_id: i32,
        magic_level: u8,
        royalty_draw: i32,
    ) -> bool {
        let now = self.clock_ms;
        let Some(caster) = self.entities.get(caster_id) else {
            return false;
        };
        if caster.race != ACTOR_RACE_PLAY {
            return false;
        }
        let caster_level = i32::from(caster.level);
        let Some(t) = self.entities.get(target_id) else {
            return false;
        };
        // `if (targetObject.Master != null) HP = HP / 10;`（`:756-759`）。
        let mut hp = t.hp;
        if t.master.is_some() {
            hp /= 10;
        }
        // `if (1500 - magicLevel * 200 < WalkSpeed) WalkSpeed = ...`（`:765-772`）。
        let walk_speed = t.ai.walk_speed.min(1500 - i64::from(magic_level) * 200);
        let next_hit_time = t.ai.next_hit_time.min(2000 - i64::from(magic_level) * 200);
        let master_tick = if t.master_tick == 0 {
            now
        } else {
            t.master_tick
        };
        let royalty_tick = tame_royalty_tick(caster_level, magic_level, royalty_draw, now);
        if let Some(t) = self.entities.get_mut(target_id) {
            t.master = Some(caster_id);
            t.hp = hp;
            t.master_royalty_tick = royalty_tick;
            // oracle 只写 `SlaveMakeLevel`（**不写 `IsSlave`**，与 `MakeSlave` 不同）。
            t.slave_make_level = magic_level;
            t.master_tick = master_tick;
            t.ai.walk_speed = walk_speed;
            t.ai.next_hit_time = next_hit_time;
            t.ai.holy_seize = false; // `BreakHolySeizeMode()`
        }
        self.entities
            .get_mut(caster_id)
            .expect("caster 已确认存在")
            .slave_list
            .push(target_id);
        true
    }
}

/// 对每个**持有非空 `SlaveList`** 的实体跑一次 [`World::slave_list_cleanup`]（槽位顺序）。
///
/// oracle 里清表在各自主人的 `Run()` 里跑；本骨架玩家无 `Run` ⇒ 在 [`World`] 的 tick 里集中跑。
/// 空表零动作（既有 golden 逐字节不变；无随机数）。
pub fn slave_list_cleanup_all(world: &mut World) {
    let ids: Vec<i32> = world
        .entities
        .iter()
        .filter(|e| !e.slave_list.is_empty())
        .map(|e| e.id)
        .collect();
    for id in ids {
        world.slave_list_cleanup(id);
    }
}

/// `MonsterObject.CheckRoyalty`（`MonsterObject.cs:363-397`）。
///
/// 每 ≥10s（`CheckRoyaltyTick`）检查一次；`Master != null` 时：
/// ① `M2Share.SpiritMutinyTick > now && SlaveExpLevel < 5` ⇒ `MasterRoyaltyTick = 0`；
/// ② `now > MasterRoyaltyTick` ⇒ 从 `Master.SlaveList` 移除（首个匹配）+ `Master = null` + `HP /= 10` + `RefShowName`；
/// ③ `MasterTick > 0 && now - MasterTick > 12h` ⇒ `HP = 0`。
///
/// **不消费随机数**。`RefShowName`（重发 `RM_USERNAME`）是帧面，不在此层。
/// 调用点（`MonsterObject.cs:352`）由 [`crate::monai::monster_run`] 在 oracle 的两个提前返回口之外复刻。
pub fn check_royalty(world: &mut World, mon_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(mon_id) else {
        return;
    };
    if (now - e.ai.check_royalty_tick) <= CHECK_ROYALTY_INTERVAL {
        return;
    }
    let exp_level = e.slave_exp_level;
    let master = e.master;
    let master_tick = e.master_tick;
    // oracle 先刷新 `CheckRoyaltyTick`，再判 `Master`。
    if let Some(m) = world.entities.get_mut(mon_id) {
        m.ai.check_royalty_tick = now;
    }
    let Some(master_id) = master else {
        return;
    };
    if world.spirit_mutiny_tick > now && exp_level < 5 {
        if let Some(m) = world.entities.get_mut(mon_id) {
            m.master_royalty_tick = 0;
        }
    }
    let royalty_deadline = world
        .entities
        .get(mon_id)
        .map_or(0, |m| m.master_royalty_tick);
    if now > royalty_deadline {
        if let Some(master) = world.entities.get_mut(master_id) {
            if let Some(i) = master.slave_list.iter().position(|id| *id == mon_id) {
                master.slave_list.remove(i);
            }
        }
        if let Some(m) = world.entities.get_mut(mon_id) {
            m.master = None;
            m.hp /= 10;
        }
    }
    // 注意：`Master` 已可能置 null，但 `MasterTick` 是自身字段 ⇒ 仍判。
    if master_tick > 0 && (now - master_tick) > MASTER_MUTINY_DEATH_MS {
        if let Some(m) = world.entities.get_mut(mon_id) {
            m.hp = 0;
        }
    }
}

/// `MagTamming` 的 `MasterRoyaltyTick` 公式（`MagicManager.cs:759`）：
/// `(Random(caster.Level * 2) + (magicLevel << 2) * 5 + 20) * 60 * 1000 + now`。
#[must_use]
pub fn tame_royalty_tick(caster_level: i32, magic_level: u8, draw: i32, now: i64) -> i64 {
    let _ = caster_level; // 抽取范围由调用方在 `mag_tamming` 里消费；此处只用 draw 值。
    (i64::from(draw) + (i64::from(magic_level) << 2) * 5 + 20) * 60 * 1000 + now
}

/// `PlayObject.ProcessSlaveMutiny`（`PlayObject.Base.cs:331-357`）：主人死亡后逐个宝宝掷骰。
///
/// `!Death || !Config.MasterDieMutiny` ⇒ 直接返回（且**不抽数**）。否则从列表**末尾往前**：
/// `Master.LastHiter != null && Random(MasterDieMutinyRate) == 0` ⇒ 叛变（`Master=null`、
/// `SlaveExpLevel = SlaveColor.Length`、`DC × MasterDieMutinyPower`、`WalkSpeed /= MasterDieMutinySpeed`）；
/// 否则 `HP = 0`。
///
/// **取数次序**：`LastHiter == null` 时 C# 短路 ⇒ 不抽；否则每宝宝恰抽 1 次
/// `Random(MasterDieMutinyRate)`（Claim B 账本输入，不许改序）。不返回 `RefNameColor`/`RefShowName` 帧。
///
/// **照搬的 oracle 怪行为（BUG-REPLICA ④）**：`WAbil.DC = (ushort)MakeLong((short)(LoByte*Power),
/// (short)(HiByte*Power))`（`PlayObject.Base.cs:349`）——外层 `(ushort)` 把 `MakeLong` 的高 16 位（= 高字节
/// 缩放值）**截掉** ⇒ `DC` 实际只等于 `(ushort)(short)(LoByte(DC) * Power)`，高字节归零。本实现逐位保留这
/// 个截断（含红检钉住）。
pub fn process_slave_mutiny(world: &mut World, master_id: i32, cfg: &PetSlaveConfig) -> Vec<i32> {
    let Some(master) = world.entities.get(master_id) else {
        return Vec::new();
    };
    if !master.death || !cfg.master_die_mutiny {
        return Vec::new();
    }
    let last_hiter = master.last_hiter;
    let slave_ids: Vec<i32> = master.slave_list.clone();
    let rate = cfg.master_die_mutiny_rate;
    let power = cfg.master_die_mutiny_power;
    let speed = cfg.master_die_mutiny_speed;
    let color_len = cfg.slave_color.len() as u8; // C# `SlaveColor.Length`（现网 15）
    let mut mutinied = Vec::new();
    for sid in slave_ids.iter().rev() {
        let roll = if last_hiter.is_some() {
            world.rng.random_below(rate)
        } else {
            1 // `&&` 短路：不抽数、必不走叛变支
        };
        if last_hiter.is_some() && roll == 0 {
            if let Some(s) = world.entities.get_mut(*sid) {
                s.master = None;
                s.slave_exp_level = color_len;
                // `RecalcAbilitys()` 对基类怪 = `WAbil = Abil.Clone()`（本骨架无额外字段）⇒ no-op。
                let low = (crate::combat::lo_byte(s.dc) as i32 * power) as i16;
                let high = (crate::combat::hi_byte(s.dc) as i32 * power) as i16;
                let dc = mir2_shared::hutil::make_long_signed(low, high);
                // C# `(ushort)MakeLong(...)` 截断低 16 位 ⇒ 高字节缩放值被丢掉（BUG-REPLICA ④）。
                s.dc = (dc as u32 & 0xFFFF) as i32;
                // BUG-REPLICA ②：速度 0 时 C# 除零；Rust 跳过除法（不可达分支）。
                if speed != 0 {
                    s.ai.walk_speed /= i64::from(speed);
                }
            }
            mutinied.push(*sid);
        } else if let Some(s) = world.entities.get_mut(*sid) {
            s.hp = 0;
        }
    }
    mutinied
}

/// `PlayObject.ProcessSlaveGhost`（`PlayObject.Base.cs:360-370`）：`Ghost` 主人逐个宝宝 `MakeGhost()`。
///
/// `!Ghost` ⇒ 直接返回。返回被处理的宝宝 id（末尾往前序）。`MakeGhost` 的 `CanReAlive` 变体未建模（登记）。
pub fn process_slave_ghost(world: &mut World, master_id: i32) -> Vec<i32> {
    let Some(master) = world.entities.get(master_id) else {
        return Vec::new();
    };
    if !master.ghost {
        return Vec::new();
    }
    let slave_ids: Vec<i32> = master.slave_list.clone();
    let mut out = Vec::new();
    for sid in slave_ids.iter().rev() {
        if let Some(s) = world.entities.get_mut(*sid) {
            // `MakeGhost`（`BaseObject.Base.cs:484-497`）：`CanReAlive => Invisible=true` 否则 `Ghost=true`。
            // 本骨架未建模 `CanReAlive`/`DisappearA` ⇒ 按召唤物常态置 `ghost`（登记）。
            s.ghost = true;
        }
        out.push(*sid);
    }
    out
}

/// `BaseObject.SendSelfDelayMsg`（`BaseObject.Message.cs:188-206`）入队的一条自身延时消息。
///
/// `deliver_at_ms` = oracle 的 `DeliveryTime = GetTickCount() + dwDelay`；`BaseObject.GetMessage`
/// （`:334-352`）在 `GetTickCount() < DeliveryTime` 时重新入队，`now >= DeliveryTime` 时投递
/// ⇒ 本骨架的到点门为 `world.clock_ms >= deliver_at_ms`（同一个 `>=` 边界）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfDelayMsg {
    /// 收件人（自投递 ⇒ 发件人本人）的 actor id（oracle `SendMessage.ActorId`）。
    pub actor_id: i32,
    /// `SendMessage.wIdent`（`RM_MASTERDIEMUTINY` / `RM_MASTERDIEGHOST` / `RM_MAGHEALING`）。
    pub ident: u16,
    /// `SendMessage.wParam`（B2 扩载荷；当前生产仅 `RM_MAGHEALING` 用 0）。
    pub w_param: i32,
    /// `SendMessage.nParam1`（B2 扩载荷：`RM_MAGHEALING` 的 `nPower`；B3 的 `RM_TRANSPARENT`
    /// 用它传 `nHTime`）。
    pub n_param1: i32,
    /// `SendMessage.nParam2`（B3：`RM_MAGIC_LVEXP` 的 `Level`）。
    pub n_param2: i32,
    /// `SendMessage.nParam3`（B3：`RM_MAGIC_LVEXP` 的 `TranPoint`；B4：`RM_POISON` 的 `nPoint`）。
    pub n_param3: i32,
    /// `SendMessage.Msg`（`BaseObject.Message.cs:188-206` 的 `string strMsg`）。
    /// B4 的 `RM_POISON`/`RM_DELAYMAGIC` 都传 `""`；保留字段以与 oracle 结构同形（合同 §2.6）。
    pub msg: String,
    /// 到点时刻（sim 毫秒）。
    pub deliver_at_ms: i64,
}

/// 一条自身延时消息**到点投递**（`PlayObject.Operate`）的效果记录。
///
/// 帧面（`RefNameColor` ⇒ `RM_CHANGENAMECOLOR` / `RefShowName` ⇒ `RM_USERNAME`）不在本层，
/// 由宿主据本记录出帧（登记）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelfDelayOutcome {
    /// 收件人 id。
    pub actor_id: i32,
    /// 被投递的消息号（当前仅 `RM_MASTERDIEMUTINY` / `RM_MASTERDIEGHOST`）。
    pub ident: u16,
    /// `ProcessSlaveMutiny` 的叛变宝宝 id（末尾往前序；非该支恒空）。
    pub mutinied: Vec<i32>,
    /// `ProcessSlaveGhost` 置 Ghost 的宝宝 id（末尾往前序；非该支恒空）。
    pub ghosted: Vec<i32>,
    /// `RM_MAGHEALING` 的 `nParam1`（`nPower`；非该支恒 `None`）。
    ///
    /// 宿主据它执行 `PlayObject.Message.cs:1619-1629` 的
    /// `if (IncHealing + n < 300) { IncHealing += n; PerHealing = 5; } else { IncHealing = 300; }`
    /// （`PerHealing = 5` **只在 if 分支**）。
    pub mag_healing: Option<i32>,
    /// `RM_TRANSPARENT` 到点投递的 `MagMakePrivateTransparent` 结果（非该支 `None`）。
    pub transparent: Option<TransparentDelivery>,
    /// `RM_DOOPENHEALTH` 到点投递的 `MakeOpenHealth` 结果（非该支 `None`）。
    pub open_health: Option<OpenHealthDelivery>,
    /// `RM_MAGIC_LVEXP` 到点投递的载荷（非该支 `None`）。
    pub magic_lvexp: Option<MagicLvExpDelivery>,
    /// `RM_MAKEHOLYSEIZEMODE` 到点投递的 `RefNameColor` 结果（非该支 `None`）。
    pub name_color: Option<NameColorDelivery>,
    /// **P3-B4**：`RM_DELAYMAGIC` 到点投递的观察值（非该支 `None`）。
    pub delay_magic: Option<DelayMagicDelivery>,
    /// **P3-B4**：`RM_POISON` 到点投递的观察值（非该支 `None`）。
    pub poison: Option<PoisonDelivery>,
}

/// **P3-B4**：`RM_DELAYMAGIC` 投递（`BaseObject.Base.cs:711-729`）的观察值。
///
/// 投递后目标收到的 `RM_MAGSTRUCK` 只**入** [`crate::world::World::pending_mag_struck`]，
/// 由宿主随后调 `World::deliver_mag_struck` 完整处理（目标侧 `R(1000)`/MSD/`StruckDamage`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DelayMagicDelivery {
    /// 处理该延时消息的 actor（oracle 的 `this` = `processMsg.ActorId`）。
    pub caster_id: i32,
    pub target_id: i32,
    /// `(ushort)wParam`（`nPower` 初值）。
    pub power: i32,
    /// 投递者侧 `GetMagStruckDamage(this, nPower)` 的结果（`<= 0` ⇒ 不投递）。
    pub msd: i32,
    /// 是否向目标投递了 `RM_MAGSTRUCK`（距离门 + MSD>0）。
    pub struck: bool,
}

/// **P3-B4**：`RM_POISON` 投递（`BaseObject.Base.cs:740-759` → `BaseObject.cs:1941-1973`）的观察值。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoisonDelivery {
    pub target_id: i32,
    /// `wParam`（`PoisonState`：0=绿毒、1=红毒、5=STONE）。
    pub poison_type: u8,
    /// `(ushort)nParam1`（`MakePosion` 的 `nTime` —— 注意这是**时长**，不是中毒点）。
    pub time: i32,
    /// `nParam3`（`MakePosion` 的 `nPoint`）。
    pub point: i32,
    /// `nOldCharStatus != CharStatus` ⇒ `StatusChanged()`（⇒ 657）。
    pub status_changed: bool,
    /// 投递后的 `GetCharStatus()`（657 的 `nParam1`）。
    pub status: i32,
    /// 目标 `Race == ActorRace.Play` ⇒ `SysMsg(YouPoisoned)`（⇒ 100）。
    pub is_player: bool,
    /// `SetPkFlag(caster)` 是否触发 `RefNameColor`（⇒ 656，由 **caster** 发）。
    pub name_color: Option<NameColorDelivery>,
}

/// `RM_TRANSPARENT` 的投递结果（`MagicManager.cs:673-699` 的 `MagMakePrivateTransparent`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransparentDelivery {
    pub target_id: i32,
    /// helper 是否走到收尾（`StatusTimeArr[STATETRANSPARENT] > 0` ⇒ 早退 ⇒ **无 657**）。
    pub status_changed: bool,
    /// `GetCharStatus()`（657 的 `nParam1`；本仓不读 `StatusTimeArr` ⇒ 通常不变）。
    pub status: i32,
    /// `RM_TRANSPARENT.nParam1`（投递时按 `(ushort)` 截断）。
    pub n_h_time: u16,
}

/// `RM_DOOPENHEALTH` 的投递结果（`BaseObject.cs:944-950` 的 `MakeOpenHealth`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenHealthDelivery {
    pub target_id: i32,
    pub hp: u16,
    pub max_hp: u16,
}

/// `RM_MAGIC_LVEXP` 的投递载荷（`MagicManager.cs:666` 的 `SendSelfDelayMsg`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MagicLvExpDelivery {
    pub magic_id: u16,
    pub level: u8,
    pub tran_point: i32,
}

/// `RM_MAKEHOLYSEIZEMODE` 的投递结果（`AnimalObject.cs:322-328`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NameColorDelivery {
    pub target_id: i32,
    /// 接收者侧 `PlayObject.GetChrColor(baseObject)`（`BaseObject.GetChrColor` ⇒ `GetNameColor()`，
    /// 怪的 `name_color` 构造缺省 255）。**不是** `MonsterObject.GetChrColor` 的 0x7D 覆写
    /// （该分支只在怪物自己算色时可达，见 `World::open_holy_seize_mode` 注释）。
    pub color: u8,
}

impl World {
    /// `BaseObject.SendSelfDelayMsg`（`BaseObject.Message.cs:188-206`）：**先判 `!Ghost`**——
    /// `Ghost == true` 的实体不入队（见「生命周期」节 BUG-REPLICA 第 4 条：`RM_MASTERDIEGHOST`
    /// 恒不可达）。
    ///
    /// 入队优先级一律 `MessagePriority.Lowest`（`:204`）；本骨架以**插入序**代替优先队列。
    pub fn send_self_delay_msg(&mut self, actor_id: i32, ident: u16, delay_ms: i64) {
        self.send_self_delay_msg_with(actor_id, ident, 0, 0, delay_ms);
    }

    /// 带 `wParam`/`nParam1` 载荷的 [`World::send_self_delay_msg`]（P3-B2 扩载荷）：
    /// `RM_MAGHEALING` 需要 `nParam1 = nPower`（`MagicManager.cs:216/:66`）。
    ///
    /// oracle `BaseObject.Message.cs:188-206` 的 `!Ghost` 门照旧。
    pub fn send_self_delay_msg_with(
        &mut self,
        actor_id: i32,
        ident: u16,
        w_param: i32,
        n_param1: i32,
        delay_ms: i64,
    ) {
        self.send_self_delay_msg_full(actor_id, ident, w_param, n_param1, 0, 0, delay_ms);
    }

    /// **P3-B3**：带 `nParam2`/`nParam3` 的 `SendSelfDelayMsg`（`RM_MAGIC_LVEXP` 需要
    /// `nParam1=MagicId, nParam2=Level, nParam3=TranPoint`，`MagicManager.cs:666`）。
    ///
    /// oracle `!Ghost` 门照旧（`BaseObject.Message.cs:190`）。
    #[allow(clippy::too_many_arguments)]
    pub fn send_self_delay_msg_full(
        &mut self,
        actor_id: i32,
        ident: u16,
        w_param: i32,
        n_param1: i32,
        n_param2: i32,
        n_param3: i32,
        delay_ms: i64,
    ) {
        let ghost = self.entities.get(actor_id).is_some_and(|e| e.ghost);
        if ghost {
            return;
        }
        let deliver_at_ms = self.clock_ms + delay_ms;
        self.self_delay_msgs.push(SelfDelayMsg {
            actor_id,
            ident,
            w_param,
            n_param1,
            n_param2,
            n_param3,
            msg: String::new(),
            deliver_at_ms,
        });
    }

    /// **P3-B3**：`BaseObject.UpdateDelayMsg`（`BaseObject.Message.cs:257-278`）的等价物。
    ///
    /// oracle 先遍历 `MsgQueue` 删除 `wIdent == ident && nParam1 == lParam1` 的待投递项，
    /// 再 `SendStruckDelayMsg(..., dwDelay)`（末行 `MsgQueue.Enqueue(..., Lowest)`）。
    /// 本骨架的延迟队列按插入序保存 ⇒ 等价的「删除同 `(actor, ident, nParam1)` 待投递项」。
    ///
    /// **登记**：oracle 的循环只在**队首**（最小优先级）连续匹配时删除；队首不匹配即退出该次
    /// 扫描（`i` 自增到 `Count` 结束），因此它是「删队首连续同键」而非「删全队列同键」。
    /// 本骨架未建模跨对象的整条优先级堆，按任务口径直接删除全部同键待投递项。
    #[allow(clippy::too_many_arguments)]
    pub fn update_self_delay_msg_full(
        &mut self,
        actor_id: i32,
        ident: u16,
        w_param: i32,
        n_param1: i32,
        n_param2: i32,
        n_param3: i32,
        delay_ms: i64,
    ) {
        self.self_delay_msgs
            .retain(|m| !(m.actor_id == actor_id && m.ident == ident && m.n_param1 == n_param1));
        self.send_self_delay_msg_full(
            actor_id, ident, w_param, n_param1, n_param2, n_param3, delay_ms,
        );
    }

    /// **P3-B3**：`BaseObject.SendMsg(int wIdent, int wParam, …)`（`BaseObject.Message.cs:34-96`）
    /// 的**即时入队**等价物。
    ///
    /// `:80` 的 `if (boSend && !Ghost)` 与 `SendSelfDelayMsg`（`:190`）、`SendMsg(IActor)`
    /// （`:144`）同门 ⇒ **Ghost 实体不入队**（P3-B3 must_fix：原注释「无 `!Ghost` 门」有误；
    /// 只是 16 的调用面前置了 `mag_objects_in_range` 的 `!ghost`，把它掩盖了）。
    ///
    /// 16 的 `targetObject.SendMsg(RM_MAKEHOLYSEIZEMODE, nPower*1000, 0,0,0)`（`MagicManager.cs:1168`）
    /// 用它落到目标自己的队列（投递时刻 = 当前时钟，按插入序）。
    pub fn send_msg_now(
        &mut self,
        actor_id: i32,
        ident: u16,
        w_param: i32,
        n_param1: i32,
        n_param2: i32,
        n_param3: i32,
    ) {
        // `:80` `boSend && !Ghost`；实体缺失（oracle 不可达）按不入队处理。
        let blocked = match self.entities.get(actor_id) {
            None => true,
            Some(e) => e.ghost,
        };
        if blocked {
            return;
        }
        self.self_delay_msgs.push(SelfDelayMsg {
            actor_id,
            ident,
            w_param,
            n_param1,
            n_param2,
            n_param3,
            msg: String::new(),
            deliver_at_ms: self.clock_ms,
        });
    }

    /// `PlayObject.Die()` / `KickException()` 尾（`PlayObject.cs:1996/2005`）：
    /// `SendSelfDelayMsg(RM_MASTERDIEMUTINY, 0,0,0,0,"",1000)`。
    pub fn send_master_die_mutiny(&mut self, master_id: i32) {
        self.send_self_delay_msg(master_id, RM_MASTERDIEMUTINY, MASTER_DIE_SELF_DELAY_MS);
    }

    /// `PlayObject.MakeGhost()` 尾（`PlayObject.cs:3524-3527`）：`base.MakeGhost()` 后若 `Ghost`
    /// 则 `SendSelfDelayMsg(RM_MASTERDIEGHOST, …, 1000)`。
    ///
    /// **照搬的 oracle 缺陷（生命周期 BUG-REPLICA 第 4 条）**：`base.MakeGhost()`（`BaseObject.Base.cs:484-497`）
    /// 已把 `Ghost = true`（`CanReAlive == false`，玩家恒如此），而 `SendSelfDelayMsg` 首行的
    /// `if (!Ghost)` 门（`BaseObject.Message.cs:190`）随即拦截 ⇒ `RM_MASTERDIEGHOST` **永不入队**、
    /// `ProcessSlaveGhost` 是死代码。本方法逐字保留两道门（`Ghost` 已是 true ⇒ 入队为空）。
    pub fn send_master_die_ghost(&mut self, master_id: i32) {
        self.send_self_delay_msg(master_id, RM_MASTERDIEGHOST, MASTER_DIE_SELF_DELAY_MS);
    }

    /// 到点投递自身延时消息（oracle `PlayObject.Message.cs:360-364` 的消息段）：按**插入序**扫描，
    /// 对 `clock_ms >= deliver_at_ms` 的项出队并按 `ident` 调生产效果体。
    ///
    /// 借道 oracle `Operate`（`PlayObject.Message.cs:2116-2121`）：`RM_MASTERDIEMUTINY` ⇒
    /// [`process_slave_mutiny`]，`RM_MASTERDIEGHOST` ⇒ [`process_slave_ghost`]。未到点的项**保留**
    /// 在队里（oracle `GetMessage` 重新入队等价）。
    pub fn deliver_self_delay_msgs(&mut self) -> Vec<SelfDelayOutcome> {
        let now = self.clock_ms;
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.self_delay_msgs.len() {
            if self.self_delay_msgs[i].deliver_at_ms > now {
                i += 1;
                continue;
            }
            let msg = self.self_delay_msgs.remove(i);
            // B2：`RM_MAGHEALING` 不受「宝宝自投递」开关影响（它由治疗效果体入队）；
            // 两个主人死族消息仍按 `slave_self_delay_enabled` 门（关闭时保持入队）。
            if matches!(msg.ident, RM_MASTERDIEMUTINY | RM_MASTERDIEGHOST)
                && !self.slave_self_delay_enabled()
            {
                self.self_delay_msgs.insert(i, msg);
                i += 1;
                continue;
            }
            match msg.ident {
                RM_MAGHEALING => {
                    // `PlayObject.Message.cs:1619-1629`：交付段 0 抽，只把 `nPower` 交宿主累加
                    // `IncHealing`（`SessionGround.recovery`）。
                    out.push(SelfDelayOutcome {
                        actor_id: msg.actor_id,
                        ident: msg.ident,
                        mutinied: Vec::new(),
                        ghosted: Vec::new(),
                        mag_healing: Some(msg.n_param1),
                        ..SelfDelayOutcome::default()
                    });
                }
                RM_MASTERDIEMUTINY => {
                    // 只在真正有到点的叛变消息时才克隆配置（避免每 tick 无谓 clone）。
                    let cfg = self.pet_slave_cfg.clone();
                    let mutinied = process_slave_mutiny(self, msg.actor_id, &cfg);
                    out.push(SelfDelayOutcome {
                        actor_id: msg.actor_id,
                        ident: msg.ident,
                        mutinied,
                        ghosted: Vec::new(),
                        mag_healing: None,
                        ..SelfDelayOutcome::default()
                    });
                }
                RM_MASTERDIEGHOST => {
                    let ghosted = process_slave_ghost(self, msg.actor_id);
                    out.push(SelfDelayOutcome {
                        actor_id: msg.actor_id,
                        ident: msg.ident,
                        mutinied: Vec::new(),
                        ghosted,
                        mag_healing: None,
                        ..SelfDelayOutcome::default()
                    });
                }
                // **P3-B3**：19 的 800ms 投递——投递时才重读 `StatusTimeArr[8]` 并逐怪抽 `R(2)`。
                RM_TRANSPARENT => {
                    let d = self.mag_make_private_transparent(msg.actor_id, msg.n_param1 as u16);
                    out.push(SelfDelayOutcome {
                        actor_id: msg.actor_id,
                        ident: msg.ident,
                        transparent: Some(d),
                        ..SelfDelayOutcome::default()
                    });
                }
                // **P3-B3**：28 的 1500ms 投递——`MakeOpenHealth()`。
                RM_DOOPENHEALTH => {
                    let h = self.make_open_health(msg.actor_id);
                    out.push(SelfDelayOutcome {
                        actor_id: msg.actor_id,
                        ident: msg.ident,
                        open_health: Some(h),
                        ..SelfDelayOutcome::default()
                    });
                }
                // **P3-B3**：共享尾的 1000ms 训练帧
                // （`nParam1=MagicId, nParam2=Level, nParam3=TranPoint`）。
                RM_MAGIC_LVEXP => {
                    out.push(SelfDelayOutcome {
                        actor_id: msg.actor_id,
                        ident: msg.ident,
                        magic_lvexp: Some(MagicLvExpDelivery {
                            magic_id: u16::try_from(msg.n_param1).unwrap_or(0),
                            level: u8::try_from(msg.n_param2).unwrap_or(0),
                            tran_point: msg.n_param3,
                        }),
                        ..SelfDelayOutcome::default()
                    });
                }
                // **P3-B3**：16 的 `RM_MAKEHOLYSEIZEMODE`（`wParam = nPower*1000`）。
                RM_MAKEHOLYSEIZEMODE => {
                    let n = self.open_holy_seize_mode(msg.actor_id, msg.w_param);
                    out.push(SelfDelayOutcome {
                        actor_id: msg.actor_id,
                        ident: msg.ident,
                        name_color: Some(n),
                        ..SelfDelayOutcome::default()
                    });
                }
                // **P3-B4**：`RM_DELAYMAGIC`（`BaseObject.Base.cs:711-729`）——施法者侧
                // 1 抽 MSD，命中则把 `RM_MAGSTRUCK` 入**目标**队列（由宿主随后
                // `World::deliver_mag_struck` 完整处理）。
                RM_DELAYMAGIC => {
                    let d = self.deliver_delay_magic(&msg);
                    out.push(SelfDelayOutcome {
                        actor_id: msg.actor_id,
                        ident: msg.ident,
                        delay_magic: Some(d),
                        ..SelfDelayOutcome::default()
                    });
                }
                // **P3-B4**：`RM_POISON`（`BaseObject.Base.cs:740-759` → `MakePosion`）。
                // 投毒本身**零 RNG**；只落 buff / CharStatus / 帧事件。
                RM_POISON => {
                    let d = self.deliver_poison(&msg);
                    out.push(SelfDelayOutcome {
                        actor_id: msg.actor_id,
                        ident: msg.ident,
                        poison: Some(d),
                        ..SelfDelayOutcome::default()
                    });
                }
                // 本层只承载这些自投递（登记：其他 `RM_*` 自投递未建模）。
                _ => {}
            }
        }
        out
    }

    /// **P3-B4**：`RM_DELAYMAGIC` 到点投递（`BaseObject.Base.cs:711-729`）。
    ///
    /// 逐行：
    /// ```text
    /// :712 nPower = (ushort)wParam; tx = LoWord(nParam1); ty = HiWord(nParam1); rage = nParam2
    /// :716 target = ActorMgr.Get(nParam3)
    /// :717 if (target != null && target.GetMagStruckDamage(this, nPower) > 0)   ← 1 抽
    /// :719   SetTargetCreat(target)
    /// :720   if (target.Race >= Animal) nPower = Round(nPower / 1.2)
    /// :724   if (|tx-target.CurrX| <= rage && |ty-target.CurrY| <= rage)
    /// :726     target.SendMsg(this, RM_MAGSTRUCK, 0, nPower, 0, 0)
    /// ```
    ///
    /// **`GetMagStruckDamage(this, …)`** 的 `baseObject` 是 `this`（施法者）⇒ 不死系加成生效
    /// （`BaseObject.Base.cs:816`）。与目标处理 `RM_MAGSTRUCK` 时传 `null` 的那一抽不同。
    fn deliver_delay_magic(&mut self, msg: &SelfDelayMsg) -> DelayMagicDelivery {
        let n_power = i32::from(msg.w_param as u16);
        let tx = i32::from((msg.n_param1 & 0xFFFF) as u16 as i16);
        let ty = i32::from(((msg.n_param1 >> 16) & 0xFFFF) as u16 as i16);
        let rage = msg.n_param2;
        let target_id = msg.n_param3;
        let mut out = DelayMagicDelivery {
            caster_id: msg.actor_id,
            target_id,
            power: n_power,
            msd: 0,
            struck: false,
        };
        let Some(t) = self.entities.get(target_id).cloned() else {
            return out;
        };
        // `this` 在 oracle 里恒存在；缺失时按 0 处理（不改变取数次数）。
        let undead = self
            .entities
            .get(msg.actor_id)
            .map_or(0, |c| c.undead_power);
        let msd = crate::combat::get_mag_struck_damage(&mut self.rng, t.mac, n_power, Some(undead));
        out.msd = msd;
        if msd > 0 {
            if let Some(c) = self.entities.get_mut(msg.actor_id) {
                c.ai.target_creature = Some(target_id);
            }
            let mut power = n_power;
            if t.race >= crate::entity::ACTOR_RACE_ANIMAL {
                power = hutil::round(f64::from(power) / 1.2);
            }
            if (tx - t.x).abs() <= rage && (ty - t.y).abs() <= rage {
                self.pending_mag_struck
                    .push(crate::world::PendingMagStruck {
                        target_id,
                        caster_id: msg.actor_id,
                        power,
                    });
                out.struck = true;
            }
        }
        out
    }

    /// **P3-B4**：`RM_POISON` 到点投递（`BaseObject.Base.cs:740-759` + `MakePosion`
    /// `BaseObject.cs:1941-1973`）。
    ///
    /// ```text
    /// :741 caster = ActorMgr.Get(nParam2)
    /// :742 if (caster != null) {
    /// :744   if (IsProperTarget(caster)) { SetTargetCreat(caster);
    /// :747     if (this.Race==Play && caster.Race==Play) SetPkFlag(caster);  // ⇒ 656（由 caster 发）
    /// :751     SetLastHiter(caster); }
    /// :753   MakePosion(wParam, (ushort)nParam1, nParam3);
    /// :757 } else MakePosion(...)
    /// ```
    ///
    /// **零 RNG**。`nOldCharStatus != CharStatus` ⇒ `StatusChanged()`（⇒ 657）；
    /// `Race == Play` ⇒ `SysMsg(YouPoisoned)`（⇒ 100）。两者都由宿主据本记录出帧。
    fn deliver_poison(&mut self, msg: &SelfDelayMsg) -> PoisonDelivery {
        let target_id = msg.actor_id;
        let caster_id = msg.n_param2;
        let poison_type = msg.w_param as u8;
        let time = i32::from((msg.n_param1 & 0xFFFF) as u16);
        let point = msg.n_param3;
        let mut out = PoisonDelivery {
            target_id,
            poison_type,
            time,
            point,
            ..PoisonDelivery::default()
        };
        let target = self.entities.get(target_id).cloned();
        let caster = self.entities.get(caster_id).cloned();
        if let (Some(t), Some(c)) = (target.as_ref(), caster.as_ref()) {
            // `IsProperTarget` 的 `this` 是**中毒目标** t ⇒ `IsGroupMember` 读 **t 的团长**
            // （`PlayObject.cs:5158-5175`）的成员表，不是 t 自己的 `group_members`（非团长恒空）。
            if crate::combat::is_proper_target_grouped(t, c, &self.attacker_group_members(t)) {
                if let Some(tm) = self.entities.get_mut(target_id) {
                    tm.ai.target_creature = Some(caster_id);
                }
                if t.race == crate::entity::ACTOR_RACE_PLAY
                    && c.race == crate::entity::ACTOR_RACE_PLAY
                {
                    out.name_color = self.set_pk_flag(target_id, caster_id);
                }
                let now = self.clock_ms;
                if let Some(tm) = self.entities.get_mut(target_id) {
                    tm.set_last_hiter(caster_id, now);
                }
            }
        }
        let Some(t) = target else {
            return out;
        };
        let old = self.get_char_status(target_id);
        let ok = self.make_posion(target_id, poison_type, time, point);
        let new = self.get_char_status(target_id);
        out.status = new;
        out.status_changed = ok && old != new;
        out.is_player = t.race == crate::entity::ACTOR_RACE_PLAY;
        out
    }
}

/// `BaseObject.GetChrColor`（`BaseObject.Base.cs:869-883`）的**宝宝色支** + 兜底：
/// ① `Race == NPC` ⇒ `npn_name_color`；② `Master != null` 且 `SlaveExpLevel <= SlaveMaxLevel`
/// ⇒ `slave_color[SlaveExpLevel]`；③ 否则 `GetNameColor()`（= `Entity::name_color`）。
///
/// `MonsterObject.GetChrColor`（`MonsterObject.cs:482-492`）的 `CrazyMode`/`HolySeize` 前置支
/// 未建模（`CrazyMode` 字段不在骨架里，登记）；调用方可先判这两支再落本函数。
#[must_use]
pub fn get_chr_color(e: &Entity, slave_color: &[u8], npc_name_color: u8) -> u8 {
    if e.race == ACTOR_RACE_NPC {
        return npc_name_color;
    }
    if e.master.is_some() {
        let lvl = e.slave_exp_level;
        if lvl <= SLAVE_MAX_LEVEL {
            if let Some(c) = slave_color.get(usize::from(lvl)) {
                return *c;
            }
            // BUG-REPLICA ①：oracle 15 项表 + 门 `<= 50` ⇒ 越界异常；Rust 落到兜底。
        }
    }
    e.name_color
}

/// [`spawn_clone`] 的结果（宿主据此登记展示表并发 `RM_TURN`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloneSpawn {
    pub clone_id: i32,
    pub x: i32,
    pub y: i32,
    /// `PlayCloneObject.Dir = GetBackDir(playObject.Dir)`（`PlayCloneObject.cs:12`）。
    pub dir: u8,
    /// `Envir.AddMapObject` 的返回（格子已追加该对象）；oracle 侧返回值不上抛。
    pub created: bool,
}

/// `MagMakeClone`（`MagicManager.cs:1337-1341`）`new PlayCloneObject(playObject)`
/// （`PlayCloneObject.cs:5-23`）的骨架等价物：造一个**玩家型**克隆体实体并落格。
///
/// 逐字段锚点：
/// - `ChrName = "Clone"`（`:9`）⇒ [`CLONE_NAME`]；
/// - `CurrX/CurrY = playObject.CurrX/CurrY`（`:10-11`）⇒ 与主人同格；
/// - `Dir = GetBackDir(playObject.Dir)`（`:12`）⇒ 取 `caster_dir` 的背面（[`mag_make_clone`]
///   内的 `back_dir` 落，`GetBackDir` = `(dir + 4) % 8`）；
/// - `Envir = playObject.Envir`（`:13`）⇒ `map_id` 取主人所在图；
/// - `Gender`/`Hair = playObject.Gender/Hair`（`:14-15`）：骨架 `Entity` 不存这两个展示层字段
///   （它们编码在 `bins/game-svr::downlink::ActorDisplay.feature` 的位域里）⇒ 宿主登记展示时
///   从主人的 `feature` 反解 `gender = byte1 & 1` / `hair = byte2 >> 1`（见 `clone_display`）；
/// - `Envir.AddMapObject(CurrX, CurrY, CellType.Play, ActorId, this)`（`:16`）⇒
///   [`mag_make_clone`] 的 `created`（格占用门）；
/// - `SendRefMsg(RM_TURN, Dir, CurrX, CurrY, 0, ChrName)`（`:17`）⇒ 帧面归宿主。
///
/// **不入 `SlaveList`**（oracle `PlayCloneObject` 非 `MonsterObject`）；**不抽随机数**
/// （oracle 构造器无 `Random`，RNG 账本不变）。
///
/// oracle 里 `PlayCloneObject` 是完整 `PlayObject` 子类，会走基类 `Run`/AOI/buff/tick；
/// 本骨架的克隆体是**静态可观测实体**（存在/名字/坐标/朝向/格占用/展示），tick 生命周期
/// 登记白名单 `T-51`、`not_verified`。
///
/// 主人不在世界 / 地图 id 越界 ⇒ `None`（oracle 侧这两个前置在 `PlayObject`/`Envir` 上不可达）。
pub fn spawn_clone(world: &mut World, caster_id: i32, caster_dir: u8) -> Option<CloneSpawn> {
    let (x, y, map_id) = {
        let c = world.entities.get(caster_id)?;
        (c.x, c.y, c.map_id)
    };
    if map_id >= world.maps.len() {
        return None;
    }
    let clone_id = world.entities.alloc_id();
    // `Envir.AddMapObject(...)`：`mag_make_clone` 已把「格子能否放下」这一门落成 `created`。
    let out = mag_make_clone(
        &mut world.maps[map_id],
        &MakeCloneInput {
            x,
            y,
            dir: caster_dir,
            clone_id,
        },
    );
    let mut e = Entity::new(0, CLONE_NAME.to_string(), x, y);
    e.id = clone_id;
    e.race = ACTOR_RACE_PLAY;
    e.map_id = map_id;
    e.view_range = PLAYER_VIEW_RANGE;
    // `PlayObject` 构造器置 `WantRefMsg = true`（`PlayObject.cs:889`）。
    e.want_ref_msg = true;
    // oracle `AddMapObject`（`Envirnoment.cs:279-284`）：成功才 `AddObject` + 置 `AddToMaped`；
    // 失败时对象仍在 `ActorMgr`（= 本骨架 `EntityStore`）但不在图上 ⇒ `add_to_mapped` 跟 `created`。
    e.add_to_mapped = out.created;
    world.entities.insert(e);
    Some(CloneSpawn {
        clone_id,
        x,
        y,
        dir: out.dir,
        created: out.created,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_data::pet_slave::RecallMigic;

    fn recall(hum: i32, name: &str, count: i32, level: i32) -> RecallMigic {
        RecallMigic {
            n_hum_level: hum,
            s_mon_name: name.to_string(),
            n_count: count,
            n_level: level,
        }
    }

    /// 按需改 `PetSlaveConfig`（避开 clippy `field_reassign_with_default`）。
    fn pet_cfg(edit: impl FnOnce(&mut PetSlaveConfig)) -> PetSlaveConfig {
        let mut c = PetSlaveConfig::default();
        edit(&mut c);
        c
    }

    /// 红检①（骷髅选型无 `break`）：多个命中项时取**最后一个**（`MagicManager.cs:1316-1327`）。
    #[test]
    fn skeleton_selection_keeps_last_match_no_break() {
        let cfg = pet_cfg(|c| {
            c.skeleton = "变异骷髅".into();
            c.skeleton_count = 1;
            c.skeleton_array[0] = recall(1, "骷髅A", 2, 1);
            c.skeleton_array[1] = recall(1, "骷髅B", 3, 2);
            c.skeleton_array[2] = recall(1, "骷髅C", 4, 3);
            c.skeleton_array[3] = recall(0, "", 0, 0);
        });
        let plan = mag_make_slave_plan(&cfg, 10, 3);
        assert_eq!(plan.mon_name, "骷髅C", "无 break ⇒ 最后一个命中项胜出");
        assert_eq!(plan.count, 4);
        assert_eq!(plan.exp_level, 3);
        assert_eq!(plan.make_level, 3);
    }

    /// 骷髅选型的 `nHumLevel == 0` 早停：0 之后的项不参与（oracle `break`）。
    #[test]
    fn skeleton_selection_stops_at_zero_hum_level() {
        let cfg = pet_cfg(|c| {
            c.skeleton_array[0] = recall(1, "骷髅A", 2, 1);
            c.skeleton_array[1] = recall(0, "", 0, 0);
            c.skeleton_array[2] = recall(1, "骷髅C", 4, 3);
        });
        let plan = mag_make_slave_plan(&cfg, 99, 1);
        assert_eq!(plan.mon_name, "骷髅A");
        assert_eq!(plan.count, 2);
    }

    /// 神兽选型**有** `break`（取第一个命中），并保留 `nExpLevel>4` 的 Dragon 名拼接。
    #[test]
    fn sinsu_selection_breaks_on_first_match() {
        let cfg = pet_cfg(|c| {
            c.dragon = "神兽".into();
            c.dragon_count = 1;
            c.dragon_array[0] = recall(1, "神兽A", 2, 1);
            c.dragon_array[1] = recall(1, "神兽B", 3, 2);
            c.dragon_array[2] = recall(0, "", 0, 0);
        });
        let plan = mag_make_sinsu_slave_plan(&cfg, 10, 3);
        assert_eq!(plan.mon_name, "神兽A", "有 break ⇒ 第一个命中项胜出");
        assert_eq!(plan.count, 2);
        assert_eq!(plan.exp_level, 1);
        assert_eq!(
            plan.make_level, 3,
            "oracle 传 userMagic.Level（nMakeLevel 算了不用）"
        );
    }

    /// 神兽选型无命中：回 `Config.Dragon`/`DragonCount`；`nExpLevel>4` 拼后缀。
    #[test]
    fn sinsu_selection_falls_back_to_dragon_name() {
        let cfg = pet_cfg(|c| {
            c.dragon = "神兽".into();
            c.dragon_count = 7;
            c.dragon_array[0] = recall(0, "", 0, 0);
        });
        let plan = mag_make_sinsu_slave_plan(&cfg, 10, 5);
        assert_eq!(plan.mon_name, "神兽5", "exp_level=5 ⇒ Dragon + ((5/4)+4)=5");
        assert_eq!(plan.count, 7);
    }

    /// 精灵：`nMaxMob` 恒 1、`SlaveExpLevel = SlaveMakeLevel = userMagic.Level`。
    #[test]
    fn angel_uses_magic_level_for_both() {
        let cfg = PetSlaveConfig::default();
        let plan = mag_make_angel_slave_plan(&cfg, 2);
        assert_eq!(plan.mon_name, cfg.angel);
        assert_eq!(plan.count, 1);
        assert_eq!(plan.exp_level, 2);
        assert_eq!(plan.make_level, 2);
    }

    /// 上限门：`SlaveList.Count < nMaxMob` 才放行（`BaseObject.cs:1185`）。
    #[test]
    fn make_slave_gate_is_strictly_less() {
        assert!(make_slave_gate(0, 1));
        assert!(!make_slave_gate(1, 1), "count == nMaxMob ⇒ 拒绝");
        assert!(!make_slave_gate(2, 1));
        assert!(make_slave_gate(4, 5));
        assert!(!make_slave_gate(5, 5));
    }

    /// `RM_10401` 门（`BaseObject.cs:1979-1994`）。
    #[test]
    fn check_server_make_slave_blocks_only_on_pending() {
        assert!(!check_server_make_slave(false));
        assert!(check_server_make_slave(true));
    }

    /// `RecallSlave` 选靶：普通名区分大小写、从末尾往前；Dragon 模式匹配 `Dragon`/`Dragon1`。
    #[test]
    fn recall_slave_target_scans_from_tail() {
        let slaves = vec![
            (1, "神兽".to_string()),
            (2, "骷髅".to_string()),
            (3, "神兽1".to_string()),
        ];
        assert_eq!(
            recall_slave_target(&slaves, "骷髅", "神兽", "神兽1"),
            Some(2)
        );
        // 末尾优先：两个同名时取靠后的那个。
        let dup = vec![(1, "骷髅".to_string()), (2, "骷髅".to_string())];
        assert_eq!(recall_slave_target(&dup, "骷髅", "神兽", "神兽1"), Some(2));
        // Dragon 模式：匹配 Dragon 或 Dragon1，末尾优先（神兽1 在神兽之后）。
        assert_eq!(
            recall_slave_target(&slaves, "神兽", "神兽", "神兽1"),
            Some(3)
        );
        // 无匹配 ⇒ None。
        assert_eq!(recall_slave_target(&slaves, "天使", "神兽", "神兽1"), None);
    }

    /// `SpaceMoveGetRandXY`：起点可走 ⇒ 不抽任何数（`BaseObject.cs:1067-1071`）。
    #[test]
    fn space_move_returns_start_without_rng_when_walkable() {
        let mut rng = RandomNumber::with_seed(7);
        let before = rng.random_below(1000);
        let mut rng2 = RandomNumber::with_seed(7);
        let got = space_move_get_rand_xy(&mut rng2, 300, 300, 5, 5, |_, _| true);
        assert_eq!(got, Some((5, 5)));
        // 同种子的下一抽应与「先抽一次再抽一次」的路径不同：这里直接断言未消费。
        let _ = before;
        let next_via_loop = rng2.random_below(1000);
        let mut rng3 = RandomNumber::with_seed(7);
        let expected = rng3.random_below(1000);
        assert_eq!(
            next_via_loop, expected,
            "起点可走 ⇒ SpaceMoveGetRandXY 零取数"
        );
    }

    /// `SpaceMoveGetRandXY`：不可走时按 `x += n18` 前进直到可走（不抽数）。
    #[test]
    fn space_move_steps_by_n18_when_blocked() {
        let mut rng = RandomNumber::with_seed(1);
        // 300x300 ⇒ n18=10、n1C=50；只在 x >= 25 时放行。
        let got = space_move_get_rand_xy(&mut rng, 300, 300, 5, 5, |x, _| x >= 25);
        assert_eq!(got, Some((25, 5)));
    }

    /// `World::make_slave_adopt` 落尾：master/is_slave/royalty/levels/HP 补半/入表。
    #[test]
    fn make_slave_adopt_writes_oracle_tail() {
        let mut w = World::new(32, 32);
        let caster = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
        let mon = {
            let mut e = Entity::new(0, "变异骷髅".to_string(), 11, 9);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            e.hp = 30;
            e.max_hp = 100;
            w.add_entity(e)
        };
        w.clock_ms = 5_000;
        assert!(w.make_slave_adopt(caster, mon, 3, 4, 86_400));
        let m = w.entities.get(mon).unwrap();
        assert_eq!(m.master, Some(caster));
        assert!(m.is_slave);
        assert_eq!(m.slave_make_level, 3);
        assert_eq!(m.slave_exp_level, 4);
        assert_eq!(m.master_royalty_tick, 5_000 + 86_400_000);
        // `RecalcAbilitys`（`BaseObject.Abil.cs:30/67`）：宝宝离开 ctor 的 0 ⇒ 1（AddAbil 空）。
        assert_eq!(m.anti_magic, 1, "宝宝召唤走 RecalcAbilitys ⇒ anti_magic=1");
        // `ApplySlaveLevelAbilitys`（race=80、exp=4）：
        // `MaxHP = _MIN(100 + 60*4, 100 + round(100*0.15)*4) = _MIN(340, 160) = 160`
        // ⇒ HP 补半 `30 + (160-30)/2 = 95`（旧实现按 no-op 会给 65）。
        assert_eq!(m.max_hp, 160, "宝宝级 MaxHP（非三类支）");
        assert_eq!(m.hp, 95, "HP 补半用改过的 MaxHP");
        assert_eq!(w.entities.get(caster).unwrap().slave_list, vec![mon]);
        assert_eq!(w.slave_count(caster), 1);
    }

    /// HP 已满且 `SlaveExpLevel == 0`（`MaxHP` 不变）⇒ 不补（oracle `if (HP < MaxHP)`）。
    #[test]
    fn make_slave_adopt_keeps_full_hp() {
        let mut w = World::new(32, 32);
        let caster = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
        let mon = {
            let mut e = Entity::new(0, "怪".to_string(), 11, 9);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            e.hp = 100;
            e.max_hp = 100;
            w.add_entity(e)
        };
        // exp=0 ⇒ `ApplySlaveLevelAbilitys` 的 `MaxHP` 不变（非三类支 `chp = 100 + 15*0`）。
        assert!(w.make_slave_adopt(caster, mon, 1, 0, 1));
        assert_eq!(w.entities.get(mon).unwrap().max_hp, 100);
        assert_eq!(w.entities.get(mon).unwrap().hp, 100);
    }

    /// `ApplySlaveLevelAbilitys` **非三类支**（`BaseObject.Abil.cs:151-167`，`Master != null`）：
    /// `DC` 高字节 `+= 2*SlaveExpLevel`、`MaxHP = _MIN(Abil.MaxHP + 60*exp, Abil.MaxHP + round(Abil.MaxHP*0.15)*exp)`。
    #[test]
    fn make_slave_adopt_applies_slave_level_abilitys_else_branch() {
        let mut w = World::new(32, 32);
        let caster = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
        let mon = {
            let mut e = Entity::new(0, "怪".to_string(), 11, 9);
            e.race = crate::entity::ACTOR_RACE_MONSTER; // 80 ≥ Animal(50)
            e.hp = 100;
            e.max_hp = 100;
            e.dc = i32::from(make_word_u16(5, 15)); // DC=5，MaxDC=15
            w.add_entity(e)
        };
        assert!(w.make_slave_adopt(caster, mon, 3, 4, 1));
        let m = w.entities.get(mon).unwrap();
        // 高字节 = 15 + 2*4 = 23（低字节不变）。
        assert_eq!(m.dc, i32::from(make_word_u16(5, 23)), "DC 高字节 +2*exp");
        // `round(100*0.15) = 15`（精确）⇒ chp = 100 + 15*4 = 160；上界 100 + 240 = 340。
        assert_eq!(m.max_hp, 160);
        assert_eq!(m.hp, 100 + (160 - 100) / 2);
    }

    /// `Math.Round(double)` 是**银行家舍入**（`MidpointRounding.ToEven`）：`round(110*0.15) =
    /// round(16.5) = 16`（不是 17）。`MaxHP = 110 + 16*1 = 126`（若按四舍五入会得 127）。
    #[test]
    fn make_slave_adopt_uses_bankers_rounding() {
        let mut w = World::new(32, 32);
        let caster = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
        let mon = {
            let mut e = Entity::new(0, "怪".to_string(), 11, 9);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            e.hp = 110;
            e.max_hp = 110;
            w.add_entity(e)
        };
        assert!(w.make_slave_adopt(caster, mon, 1, 1, 1));
        assert_eq!(w.entities.get(mon).unwrap().max_hp, 126);
    }

    /// `Race < ActorRace.Animal(50)` ⇒ `ApplySlaveLevelAbilitys` 不跑（oracle `:85-88` 的门）。
    #[test]
    fn make_slave_adopt_skips_slave_level_abilitys_below_animal() {
        let mut w = World::new(32, 32);
        let caster = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
        let mon = {
            let mut e = Entity::new(0, "NPC".to_string(), 11, 9);
            e.race = 49; // < Animal
            e.hp = 100;
            e.max_hp = 100;
            e.dc = i32::from(make_word_u16(5, 15));
            w.add_entity(e)
        };
        assert!(w.make_slave_adopt(caster, mon, 3, 4, 1));
        let m = w.entities.get(mon).unwrap();
        assert_eq!(m.max_hp, 100, "门不过 ⇒ MaxHP 不动");
        assert_eq!(m.dc, i32::from(make_word_u16(5, 15)), "门不过 ⇒ DC 不动");
        assert_eq!(m.hp, 100);
    }

    /// 变异骷髅（race=100）：`ApplySlaveLevelAbilitys` 三类支（DC/MaxHP 公式）+ `WhiteSkeleton.RecalcAbilitys`
    /// 的 `NextHitTime/WalkSpeed/WalkTick`（`WhiteSkeleton.cs:21-27`）。
    #[test]
    fn make_slave_adopt_white_skeleton_branch_and_speed() {
        let mut w = World::new(32, 32);
        let caster = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
        let mon = {
            let mut e = Entity::new(0, "变异骷髅".to_string(), 11, 9);
            e.race = crate::entity::ACTOR_RACE_WHITE_SKELETON;
            e.hp = 100;
            e.max_hp = 100;
            e.dc = i32::from(make_word_u16(5, 15));
            w.add_entity(e)
        };
        w.clock_ms = 7_000;
        assert!(w.make_slave_adopt(caster, mon, 3, 2, 1));
        let m = w.entities.get(mon).unwrap();
        // 高字节 = round(15 + 3*(0.3+0.2)*2) = round(18) = 18。
        assert_eq!(m.dc, i32::from(make_word_u16(5, 18)));
        // MaxHP = 100 + round(100*0.5)*2 = 200。
        assert_eq!(m.max_hp, 200);
        assert_eq!(m.hp, 100 + (200 - 100) / 2);
        assert_eq!(m.ai.next_hit_time, 3000 - 3 * 600);
        assert_eq!(m.ai.walk_speed, 1200 - 3 * 250);
        assert_eq!(m.ai.walk_tick, 7_000 + 2000);
    }

    /// 神兽跟随形态（race=113）：`ElfMonster.RecalcAbilitys`→`ResetElfMon` 只改 `WalkSpeed`/`WalkTick`
    /// （`ElfMonster.cs:31-41`）；`exp=0` ⇒ MaxHP/DC 不变。
    #[test]
    fn make_slave_adopt_elf_monster_speed() {
        let mut w = World::new(32, 32);
        let caster = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
        let mon = {
            let mut e = Entity::new(0, "神兽".to_string(), 11, 9);
            e.race = crate::entity::ACTOR_RACE_ELF_MONSTER;
            e.hp = 100;
            e.max_hp = 100;
            e.dc = i32::from(make_word_u16(5, 15));
            w.add_entity(e)
        };
        w.clock_ms = 9_000;
        assert!(w.make_slave_adopt(caster, mon, 4, 0, 1));
        let m = w.entities.get(mon).unwrap();
        assert_eq!(m.ai.walk_speed, 500 - 4 * 50);
        assert_eq!(m.ai.walk_tick, 9_000 + 2000);
        // `ElfMonster` 不改 `NextHitTime`（保持 ctor 默认 2000）。
        assert_eq!(m.ai.next_hit_time, 2000);
        assert_eq!(m.max_hp, 100, "exp=0 ⇒ MaxHP 不变");
        assert_eq!(m.dc, i32::from(make_word_u16(5, 15)), "exp=0 ⇒ DC 不变");
    }

    /// 神兽攻击形态（race=114）：`ElfWarriorMonster.RecalcAbilitys`→`ResetElfMon` 改
    /// `NextHitTime`/`WalkSpeed`/`WalkTick`（`ElfWarriorMonster.cs:35-39,105-110`）。
    #[test]
    fn make_slave_adopt_elf_warrior_speed() {
        let mut w = World::new(32, 32);
        let caster = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
        let mon = {
            let mut e = Entity::new(0, "强化神兽".to_string(), 11, 9);
            e.race = crate::entity::ACTOR_RACE_ELF_WARRIORMON;
            e.hp = 100;
            e.max_hp = 100;
            w.add_entity(e)
        };
        w.clock_ms = 11_000;
        assert!(w.make_slave_adopt(caster, mon, 3, 0, 1));
        let m = w.entities.get(mon).unwrap();
        assert_eq!(m.ai.next_hit_time, 1500 - 3 * 100);
        assert_eq!(m.ai.walk_speed, 500 - 3 * 50);
        assert_eq!(m.ai.walk_tick, 11_000 + 2000);
    }

    /// 红检②（上限/入表）：`make_slave_gate` 用 `<=` ⇒ 本用例必红。
    #[test]
    fn redcheck_gate_must_be_strict() {
        // 直接断言 oracle 语义：count==nMaxMob ⇒ false。改成 <= 会让本断言失败。
        assert!(!make_slave_gate(1, 1), "门必须是严格小于；改成 <= 即红");
    }

    /// `World::recall_slave`：找到末位同名 slave 并把它移到主人前一格。
    #[test]
    fn recall_slave_moves_tail_match_to_front() {
        let mut w = World::new(300, 300);
        let caster = {
            let mut e = Entity::new(0, "主人".to_string(), 10, 10);
            e.ai.dir = crate::magic::DIR_RIGHT;
            w.add_entity(e)
        };
        let slave = {
            let mut e = Entity::new(0, "骷髅".to_string(), 50, 50);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            w.add_entity(e)
        };
        assert!(w.make_slave_adopt(caster, slave, 1, 1, 1));
        let moved = w.recall_slave(caster, crate::magic::DIR_RIGHT, "骷髅", "神兽", "神兽1");
        assert_eq!(moved, Some(slave));
        let s = w.entities.get(slave).unwrap();
        assert_eq!((s.x, s.y), (11, 10), "GetFrontPosition(dir=右) ⇒ (11,10)");
    }

    /// `World::recall_slave` 无匹配 ⇒ `None` 且不消耗 RNG（oracle 循环直接结束）。
    #[test]
    fn recall_slave_no_match_is_noop() {
        fn world_with_slave() -> (World, i32) {
            let mut w = World::new(300, 300);
            w.rng = RandomNumber::with_seed(0);
            let caster = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
            let slave = {
                let mut e = Entity::new(0, "骷髅".to_string(), 50, 50);
                e.race = crate::entity::ACTOR_RACE_MONSTER;
                w.add_entity(e)
            };
            assert!(w.make_slave_adopt(caster, slave, 1, 1, 1));
            (w, caster)
        }
        let (mut a, caster) = world_with_slave();
        let (mut b, _) = world_with_slave();
        assert_eq!(
            a.recall_slave(caster, crate::magic::DIR_RIGHT, "没有", "神兽", "神兽1"),
            None
        );
        // 无匹配 ⇒ 一次数都不抽：与未调用的对照 world 的下一抽相同。
        assert_eq!(a.rng.random_below(1000), b.rng.random_below(1000));
    }

    // ---- `summon_slave` 编排（构造级，用 mock spawner 代替 `RegenMonsterByName`）----------------

    /// mock 宿主：按 oracle `MakeSlave` 的入参造一只 HP<MaxHP 的怪并落库（不抽数）。
    #[derive(Default)]
    struct MockSpawner {
        last: Option<(String, i16, i16)>,
        fail: bool,
    }

    impl SlaveSpawner for MockSpawner {
        fn spawn(
            &mut self,
            world: &mut World,
            _rng: &mut RandomNumber,
            _map_id: usize,
            name: &str,
            x: i16,
            y: i16,
        ) -> Option<i32> {
            self.last = Some((name.to_string(), x, y));
            if self.fail {
                return None;
            }
            let map_id = 0; // `World::new(32,32)` 单图；测试主人与宝宝同图。
            let mut e = Entity::new(0, name.to_string(), i32::from(x), i32::from(y));
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            e.map_id = map_id;
            e.hp = 40;
            e.max_hp = 100;
            Some(world.add_entity(e))
        }
    }

    fn summon_world() -> (World, i32) {
        let mut w = World::new(32, 32);
        let caster = {
            let mut e = Entity::new(0, "主人".to_string(), 10, 10);
            e.race = crate::entity::ACTOR_RACE_PLAY;
            e.level = 20;
            e.ai.dir = crate::magic::DIR_RIGHT;
            w.add_entity(e)
        };
        (w, caster)
    }

    /// 骷髅召唤：选型 + 前排位置 + 落尾（master/royalty/HP 补半/入表）。
    #[test]
    fn summon_slave_skeleton_end_to_end() {
        let (mut w, caster) = summon_world();
        let cfg = pet_cfg(|c| {
            c.skeleton = "变异骷髅".into();
            c.skeleton_count = 1;
            c.skeleton_array[0] = recall(10, "强化骷髅", 1, 2);
        });
        let mut sp = MockSpawner::default();
        w.clock_ms = 1_000;
        let out = summon_slave(
            &mut w,
            caster,
            crate::magic::DIR_RIGHT,
            SKILL_SKELLETON,
            3,
            &cfg,
            &mut sp,
        );
        let SummonOutcome::Spawned(mon) = out else {
            panic!("应召唤成功，得 {out:?}");
        };
        assert_eq!(
            sp.last,
            Some(("强化骷髅".to_string(), 11, 10)),
            "RegenMonsterByName 用 GetFrontPosition(dir=右)=(11,10)"
        );
        let m = w.entities.get(mon).unwrap();
        assert_eq!(m.master, Some(caster));
        assert!(m.is_slave);
        assert_eq!(m.slave_make_level, 3);
        assert_eq!(m.slave_exp_level, 2, "SkeletonArray 的 nLevel 覆盖初值");
        assert_eq!(
            m.master_royalty_tick,
            1_000 + i64::from(mir2_data::pet_slave::DEFAULT_ROYALTY_SEC) * 1000
        );
        // `ApplySlaveLevelAbilitys`（mock race=80、exp=2）：`MaxHP = _MIN(100+120, 100+15*2) = 130`
        // ⇒ HP 补半 `40 + (130-40)/2 = 85`。
        assert_eq!(m.max_hp, 130);
        assert_eq!(m.hp, 85, "HP 补半用宝宝级 MaxHP");
    }

    /// 上限门：已有 1 只、`nCount=1` ⇒ 第二次召唤被拒（且不调 spawner）。
    #[test]
    fn summon_slave_rejected_when_slave_list_full() {
        let (mut w, caster) = summon_world();
        let cfg = PetSlaveConfig::default();
        let mut sp = MockSpawner::default();
        assert!(matches!(
            summon_slave(
                &mut w,
                caster,
                crate::magic::DIR_RIGHT,
                SKILL_SKELLETON,
                1,
                &cfg,
                &mut sp
            ),
            SummonOutcome::Spawned(_)
        ));
        sp.last = None;
        let out = summon_slave(
            &mut w,
            caster,
            crate::magic::DIR_RIGHT,
            SKILL_SKELLETON,
            1,
            &cfg,
            &mut sp,
        );
        assert_eq!(out, SummonOutcome::Rejected);
        assert!(sp.last.is_none(), "门拒 ⇒ 不调 RegenMonsterByName");
    }

    /// 神兽回召分支：spawner 失败 ⇒ `RecallSlave(plan.mon_name)`（骷髅无此分支）。
    #[test]
    fn summon_slave_sinsu_falls_back_to_recall() {
        let (mut w, caster) = summon_world();
        let cfg = pet_cfg(|c| {
            c.dragon = "神兽".into();
            c.dragon_array[0] = recall(10, "神兽", 1, 1);
        });
        // 先真召一只（spawner 成功），作为回召目标。
        let mut ok = MockSpawner::default();
        let SummonOutcome::Spawned(slave) = summon_slave(
            &mut w,
            caster,
            crate::magic::DIR_RIGHT,
            SKILL_SINSU,
            1,
            &cfg,
            &mut ok,
        ) else {
            panic!("首次神兽召唤应成功");
        };
        // 把目标挪远，再用失败的 spawner 触发回召。
        w.entities.get_mut(slave).unwrap().x = 20;
        w.entities.get_mut(slave).unwrap().y = 20;
        let mut fail = MockSpawner {
            fail: true,
            ..Default::default()
        };
        let out = summon_slave(
            &mut w,
            caster,
            crate::magic::DIR_RIGHT,
            SKILL_SINSU,
            1,
            &cfg,
            &mut fail,
        );
        assert_eq!(
            out,
            SummonOutcome::Recalled(Some(slave)),
            "神兽 null ⇒ RecallSlave"
        );
        let s = w.entities.get(slave).unwrap();
        assert_eq!((s.x, s.y), (11, 10), "回召到主人前排");
    }

    // ---- 装载后**新增可达**路径：`RecallSlave` → `SpaceMoveGetRandXY` 移位支（R 级：RNG 流水） ----
    //
    // 触发条件（oracle 与 Rust 同构）：魔号 30（`SKILL_SINSU`）且 `MakeSlave` 返回 null
    // （宝宝数已满 `SlaveList.Count >= nMaxMob`，或宿主 `RegenMonsterByName` 失败）**且**
    // 主人正前方 `(fx,fy)` 是墙格。装载前每格皆 `Walk` ⇒ `CanWalk(fx,fy,true)` 恒真 ⇒
    // **首轮返回、零抽数**；装载后进移位支 ⇒ 消耗 `world.rng` 且落点变。
    //
    // 逐行同构核对（本次在 9121f5936 上复读行号）：
    //
    // | oracle | 语义 | Rust（`crates/world/src/slave.rs`） |
    // |---|---|---|
    // | `BaseObject.RecallSlave`（`BaseObject.cs:1850-1876`） | `GetFrontPosition` → `nFlag=(sSlaveName==Config.Dragon, OrdinalIgnoreCase)?1:-1` → 从 `SlaveList` **末尾往前**找（Dragon 模式匹配 `Dragon`/`Dragon1`，普通名匹配 `sSlaveName`）→ `SpaceMove(Envir.MapName, nX, nY, 1)` | `World::recall_slave`（`slave.rs:584`）+ `recall_slave_target`（`:362`）|
    // | `BaseObject.GetFrontPosition`（`:977-1038`） | 按 `Dir` 取前一格；越界时**原样保留**（clamp） | `magic::front_position`（`crates/world/src/magic.rs:963-989`）|
    // | `BaseObject.SpaceMove`（`:1103-1166`）本地图支 | `tempX/tempY = 正前方`；`SpaceMoveGetRandXY(Envir, ref tempX, ref tempY)` 成功 ⇒ `CurrX/Y = tempX/tempY` + `AddMapObject`；失败 ⇒ 回滚 | `recall_slave`（`:653-684`）：`found?`（失败 ⇒ `None`）；成功才 `move_object` 并回填实体 `x/y/map_id` |
    // | `BaseObject.SpaceMoveGetRandXY`（`:1040-1096`） | `n18 = Width<80 ? 3 : 10`；`n1C = Height<150 ? (Height<50 ? 2 : 15) : 50`；循环首行 `CanWalk(nX,nY,true)` 命中即返回；`nX < Width-n1C-1` ⇒ `nX += n18`，否则 `nX = Random(Width)`，且 `nY` 同段推进/`Random(Height)`；`n14++ >= 201` 放弃 | `crate::space_move::space_move_get_rand_xy`（`crates/world/src/space_move.rs:23`）逐句同构（含**自增后**判 201；2026-10-17 由 slave.rs 迁入独立模块）|
    // | `Envirnoment.CanWalk(x,y,true)`（`Envirnoment.cs:442-486`） | `cellSuccess && cellInfo.Valid`（`boFlag=true` 不查格上对象） | `map.get_cell_info(x,y).1`（`map.rs:331-339`）：**界内且 `valid()`** 才返回 `true`，与 oracle 的 `success` 同义 |
    //
    // 取数式：oracle `M2Share.RandomNumber.Random(Width/Height)` = `.NET Random.Next(int)` =
    // Rust `RandomNumber::random_below(width/height)`。
    // **首轮成功 ⇒ 不进移位支**：两边都是循环首行判 `CanWalk`，命中即返回 ⇒ 零抽数。
    //
    // 期望值**独立复算**（不引用实现）：按 .NET Framework `System.Random(int)` 的
    // Knuth 减法算法 + 上述 oracle 顺序，seed=42 的 `Next(10)` 序列 = `[6, 1, 1, 5, …]`。
    // ⇒ 10x10 图（`n18=3`、`n1C=2`）从 `(2,8)` 起、`(2,8)/(5,8)/(8,8)` 为墙时：
    //   probe (2,8) 墙 → x=5；probe (5,8) 墙 → x=8；probe (8,8) 墙 →
    //   `8 < 10-2-1=7` 假 ⇒ `x = Next(10) = 6`（第 1 抽）；`8 < 7` 假 ⇒ `y = Next(10) = 1`（第 2 抽）；
    //   probe (6,1) 可走 ⇒ 落点 `(6,1)`；第 3 抽 = `1`（序列第 3 项）。

    /// 10x10 世界：主人 `(1,8)`（面朝右 ⇒ 正前方 `(2,8)`）+ 已在 `SlaveList` 的神兽宝宝 `(0,0)`。
    /// `walls` 中的格子置 `LowWall`，其余保持默认 `Walk`。
    fn wall_recall_world(walls: &[(i32, i32)]) -> (World, i32, i32) {
        let mut w = World::new(10, 10);
        let caster = {
            let mut e = Entity::new(0, "主人".to_string(), 1, 8);
            e.race = crate::entity::ACTOR_RACE_PLAY;
            e.level = 20;
            w.add_entity(e)
        };
        let slave = {
            let mut e = Entity::new(0, "神兽".to_string(), 0, 0);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            w.add_entity(e)
        };
        w.entities.get_mut(caster).unwrap().slave_list.push(slave);
        for &(x, y) in walls {
            assert!(
                w.maps[0].set_map_xy_flag(x, y, false),
                "置墙角失败 ({x},{y})"
            );
        }
        (w, caster, slave)
    }

    /// **阳性（宝宝数已满）**：`gate_ok == false` ⇒ `MakeSlave` 返回 null ⇒ 神兽回召；
    /// 主人正前方是墙 ⇒ **进入移位支**，`world.rng` 逐项抽数为 oracle 期望 `[6, 1]`，落点 `(6,1)`。
    #[test]
    fn recall_shift_when_front_wall_and_slave_list_full() {
        let (mut w, caster, slave) = wall_recall_world(&[(2, 8), (5, 8), (8, 8)]);
        assert!(
            !w.maps[0].get_cell_info(2, 8).1,
            "前置：主人正前方 (2,8) 必须是墙格（否则首轮即返回）"
        );
        // `PetSlaveConfig::default()`：dragon=神兽、dragon_count=1；已有 1 只 ⇒ 门拒。
        let cfg = PetSlaveConfig::default();
        let mut sp = MockSpawner::default();
        let out = summon_slave(
            &mut w,
            caster,
            crate::magic::DIR_RIGHT,
            SKILL_SINSU,
            1,
            &cfg,
            &mut sp,
        );
        assert_eq!(
            out,
            SummonOutcome::Recalled(Some(slave)),
            "宝宝满 ⇒ MakeSlave null ⇒ RecallSlave"
        );
        assert!(sp.last.is_none(), "门拒 ⇒ 不调 RegenMonsterByName");
        let s = w.entities.get(slave).unwrap();
        assert_eq!(
            (s.x, s.y),
            (6, 1),
            "oracle 期望落点 = (Next(10)=6, Next(10)=1)"
        );
        // 前两抽已由落点逐项核过。**证「恰好 2 抽」必须看随后连续三抽**：只断第 3 抽 = 1 **不具判别力**
        // （seed=42 的 `Next(10)` 在索引 1/2/4/8/21/28/32 上都是 1 ⇒ 多抽 2 次 / 多抽 6 次的实现仍会通过；
        // 独立复验用变异实测过：落点不变、共 4 抽时旧断言全绿）。
        // `Next(10)` 序列：[6, 1, **1, 5, 1**, …] ⇒ 已耗 2 抽 ⇒ 随后三抽 = [1, 5, 1]。
        assert_eq!(
            (
                w.rng.random_below(10),
                w.rng.random_below(10),
                w.rng.random_below(10)
            ),
            (1, 5, 1),
            "恰好 2 抽：随后三抽 = [1,5,1]"
        );
    }

    /// **阳性（造怪失败）**：门放行（1 < 2）但宿主 spawner 失败 ⇒ 同样走神兽回召；
    /// `MockSpawner` 失败不抽数 ⇒ 期望与阳性①完全一致（`[6, 1]`、落点 `(6,1)`）。
    #[test]
    fn recall_shift_when_front_wall_and_spawn_fails() {
        let (mut w, caster, slave) = wall_recall_world(&[(2, 8), (5, 8), (8, 8)]);
        let cfg = pet_cfg(|c| c.dragon_count = 2); // 1 < 2 ⇒ 门放行，走到 spawner
        let mut fail = MockSpawner {
            fail: true,
            ..Default::default()
        };
        let out = summon_slave(
            &mut w,
            caster,
            crate::magic::DIR_RIGHT,
            SKILL_SINSU,
            1,
            &cfg,
            &mut fail,
        );
        assert_eq!(
            out,
            SummonOutcome::Recalled(Some(slave)),
            "spawn 失败 ⇒ MakeSlave null ⇒ RecallSlave"
        );
        assert!(
            fail.last.is_some(),
            "spawner 确实被调用（证明是失败支而非门拒）"
        );
        let s = w.entities.get(slave).unwrap();
        assert_eq!((s.x, s.y), (6, 1), "期望落点 (Next(10)=6, Next(10)=1)");
        // 同阳性①：随后三抽 = [1,5,1]（只断第 3 抽不具判别力，见上）。
        assert_eq!(
            (
                w.rng.random_below(10),
                w.rng.random_below(10),
                w.rng.random_below(10)
            ),
            (1, 5, 1),
            "恰好 2 抽：随后三抽 = [1,5,1]"
        );
    }

    /// **阴性对照**：正前方可走 ⇒ oracle `SpaceMoveGetRandXY` 首轮命中、**不进移位支、零抽数**。
    /// 落点 = 正前方 `(2,8)`；下一抽仍是 seed=42 的第 1 抽 `6`（未被消耗）。
    #[test]
    fn recall_first_round_walkable_consumes_zero_draws() {
        let (mut w, caster, slave) = wall_recall_world(&[]);
        assert!(w.maps[0].get_cell_info(2, 8).1, "前置：正前方可走");
        let cfg = PetSlaveConfig::default();
        let mut sp = MockSpawner::default();
        let out = summon_slave(
            &mut w,
            caster,
            crate::magic::DIR_RIGHT,
            SKILL_SINSU,
            1,
            &cfg,
            &mut sp,
        );
        assert_eq!(out, SummonOutcome::Recalled(Some(slave)));
        let s = w.entities.get(slave).unwrap();
        assert_eq!((s.x, s.y), (2, 8), "首轮 CanWalk 真 ⇒ 落点 = 正前方");
        // 零抽数 ⇒ 随后三抽 = seed=42 `Next(10)` 的最初三项 [6, 1, 1]（比只断第 1 项更有判别力）。
        assert_eq!(
            (
                w.rng.random_below(10),
                w.rng.random_below(10),
                w.rng.random_below(10)
            ),
            (6, 1, 1),
            "零抽数 ⇒ 随后三抽 = [6,1,1]"
        );
    }

    /// 非召唤技能 ⇒ `NotSummonSkill`（不抽数、不造怪）。
    #[test]
    fn summon_slave_ignores_other_skills() {
        let (mut w, caster) = summon_world();
        let cfg = PetSlaveConfig::default();
        let mut sp = MockSpawner::default();
        assert_eq!(
            summon_slave(
                &mut w,
                caster,
                crate::magic::DIR_RIGHT,
                SKILL_TAMMING,
                1,
                &cfg,
                &mut sp
            ),
            SummonOutcome::NotSummonSkill
        );
        assert!(sp.last.is_none());
    }

    // ---- ⑨-3 生命周期：清表 / CheckRoyalty / 主人死幽灵 / 诱惑受奴 / 色彩 -----------------------

    /// 主人 + 一只宝宝的合成世界（返回 (world, master_id, slave_id)）。
    fn slave_world(hp: u16) -> (World, i32, i32) {
        let mut w = World::new(32, 32);
        let master = {
            let mut e = Entity::new(0, "主人".to_string(), 10, 10);
            e.race = crate::entity::ACTOR_RACE_PLAY;
            w.add_entity(e)
        };
        let slave = {
            let mut e = Entity::new(0, "变异骷髅".to_string(), 11, 10);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            e.master = Some(master);
            e.is_slave = true;
            e.hp = hp;
            e.max_hp = 100;
            w.add_entity(e)
        };
        w.entities.get_mut(master).unwrap().slave_list.push(slave);
        (w, master, slave)
    }

    /// `CheckRoyalty` 到期：从主人表移除 + `Master=null` + `HP/=10`（`MonsterObject.cs:374-383`）。
    #[test]
    fn check_royalty_removes_and_halves_hp() {
        let (mut w, master, slave) = slave_world(90);
        w.clock_ms = 20_000; // > 10s 门；> MasterRoyaltyTick=0（未设）
        check_royalty(&mut w, slave);
        assert!(w.entities.get(master).unwrap().slave_list.is_empty());
        let s = w.entities.get(slave).unwrap();
        assert_eq!(s.master, None);
        assert_eq!(s.hp, 9, "HP/10");
    }

    /// `CheckRoyalty` 10s 门未到 ⇒ 不做任何事（也不刷新 `CheckRoyaltyTick`）。
    #[test]
    fn check_royalty_respects_interval() {
        let (mut w, master, slave) = slave_world(90);
        w.clock_ms = 5_000;
        w.entities.get_mut(slave).unwrap().ai.check_royalty_tick = 0;
        check_royalty(&mut w, slave); // now - 0 = 5000 ≤ 10000 ⇒ 早退
        assert_eq!(w.entities.get(master).unwrap().slave_list.len(), 1);
        assert_eq!(w.entities.get(slave).unwrap().master, Some(master));
    }

    /// 神之祈祷窗口内且 `SlaveExpLevel < 5` ⇒ `MasterRoyaltyTick=0` ⇒ 立即叛变
    /// （`MonsterObject.cs:370-372`）。
    #[test]
    fn check_royalty_spirit_window_forces_zero_deadline() {
        let (mut w, master, slave) = slave_world(50);
        w.clock_ms = 20_000;
        let s = w.entities.get_mut(slave).unwrap();
        s.slave_exp_level = 3;
        s.master_royalty_tick = w.clock_ms + 1_000_000; // 未到期
        s.ai.check_royalty_tick = 0;
        w.spirit_mutiny_tick = w.clock_ms + 60_000; // 窗口未过期
        check_royalty(&mut w, slave);
        assert!(w.entities.get(master).unwrap().slave_list.is_empty());
        assert_eq!(w.entities.get(slave).unwrap().master, None);
    }

    /// 神之祈祷窗口内但 `SlaveExpLevel >= 5` ⇒ 不置零、未到期 ⇒ 不动（`< 5` 门）。
    #[test]
    fn check_royalty_spirit_window_gated_by_level() {
        let (mut w, master, slave) = slave_world(50);
        w.clock_ms = 20_000;
        let s = w.entities.get_mut(slave).unwrap();
        s.slave_exp_level = 5;
        s.master_royalty_tick = w.clock_ms + 1_000_000;
        s.ai.check_royalty_tick = 0;
        w.spirit_mutiny_tick = w.clock_ms + 60_000;
        check_royalty(&mut w, slave);
        assert_eq!(w.entities.get(master).unwrap().slave_list.len(), 1);
    }

    /// `MasterTick > 0` 且超 12h ⇒ `HP=0`（`MonsterObject.cs:387-393`，诱饵到期）。
    #[test]
    fn check_royalty_master_tick_12h_kills() {
        let (mut w, master, slave) = slave_world(80);
        w.clock_ms = 100_000_000;
        let s = w.entities.get_mut(slave).unwrap();
        s.master_royalty_tick = w.clock_ms + 1_000_000; // 未到期
        s.master_tick = w.clock_ms - (MASTER_MUTINY_DEATH_MS + 1);
        s.ai.check_royalty_tick = 0;
        check_royalty(&mut w, slave);
        let s = w.entities.get(slave).unwrap();
        assert_eq!(s.hp, 0, "12h 诱饵到期 ⇒ HP=0");
        assert_eq!(s.master, Some(master), "未到期 ⇒ 主人不变");
    }

    /// 清表（`BaseObject.Base.cs:208-217`）：删死亡/幽灵/换主，保序保留合法宝宝。
    #[test]
    fn slave_list_cleanup_removes_dead_ghost_and_mismaster() {
        let mut w = World::new(32, 32);
        let master = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
        let other = w.add_entity(Entity::new(0, "别人".to_string(), 12, 12));
        let mut add = |dead: bool, ghost: bool, owner: i32| {
            let mut e = Entity::new(0, "宝宝".to_string(), 11, 10);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            e.death = dead;
            e.ghost = ghost;
            e.master = Some(owner);
            w.add_entity(e)
        };
        let dead = add(true, false, master);
        let ghost = add(false, true, master);
        let stray = add(false, false, other);
        let alive = add(false, false, master);
        w.entities.get_mut(master).unwrap().slave_list = vec![dead, ghost, stray, alive];
        w.slave_list_cleanup(master);
        assert_eq!(w.entities.get(master).unwrap().slave_list, vec![alive]);
    }

    /// tick 集成：死亡结算后清表把死亡宝宝从主人表里移除。
    #[test]
    fn tick_cleanup_drops_dead_slave() {
        let (mut w, master, slave) = slave_world(10);
        w.kill(slave);
        w.tick();
        assert!(
            !w.entities.get(master).unwrap().slave_list.contains(&slave),
            "死亡宝宝应在同 tick 清表后消失"
        );
    }

    /// `ProcessSlaveMutiny`：`Random(rate)==0` ⇒ 叛变（Master=null / exp=色表长 / DC×power / 速度/speed）。
    #[test]
    fn process_slave_mutiny_mutinies_on_zero_roll() {
        let (mut w, master, slave) = slave_world(100);
        {
            let s = w.entities.get_mut(slave).unwrap();
            // dc 低字节=10、高字节=20。
            s.dc = 10 | (20 << 8);
            s.ai.walk_speed = 400;
        }
        w.entities.get_mut(master).unwrap().death = true;
        w.entities.get_mut(master).unwrap().last_hiter = Some(slave);
        let cfg = pet_cfg(|c| {
            c.master_die_mutiny = true;
            c.master_die_mutiny_rate = 1; // Random(1) 恒 0 ⇒ 必叛变
            c.master_die_mutiny_power = 2;
            c.master_die_mutiny_speed = 2;
        });
        let out = process_slave_mutiny(&mut w, master, &cfg);
        assert_eq!(out, vec![slave]);
        let s = w.entities.get(slave).unwrap();
        assert_eq!(s.master, None);
        assert_eq!(s.slave_exp_level, 15, "SlaveColor.Length");
        assert_eq!(
            s.dc, 20,
            "oracle (ushort) 截断丢掉高字节 ⇒ 只剩 LoByte(10)*power"
        );
        assert_eq!(s.ai.walk_speed, 200, "WalkSpeed / speed");
    }

    /// `ProcessSlaveMutiny` 无 `LastHiter`：`&&` 短路 ⇒ 不抽数、全 `HP=0`。
    #[test]
    fn process_slave_mutiny_no_rng_without_last_hiter() {
        let (mut a, master_a, _) = slave_world(100);
        a.entities.get_mut(master_a).unwrap().death = true;
        let cfg = pet_cfg(|c| {
            c.master_die_mutiny = true;
            c.master_die_mutiny_rate = 7;
        });
        let out_a = process_slave_mutiny(&mut a, master_a, &cfg);
        assert!(out_a.is_empty(), "无 LastHiter ⇒ 无叛变");
        // 未抽数：与同种子对照世界的下一抽相同。
        let mut b = World::new(32, 32);
        assert_eq!(a.rng.random_below(1000), b.rng.random_below(1000));
    }

    /// `ProcessSlaveMutiny` 提前返回（非死亡 / 开关关）⇒ 不抽数、不置 HP。
    #[test]
    fn process_slave_mutiny_early_return() {
        let (mut w, master, slave) = slave_world(100);
        let cfg = pet_cfg(|c| {
            c.master_die_mutiny = true;
            c.master_die_mutiny_rate = 1;
        });
        assert!(process_slave_mutiny(&mut w, master, &cfg).is_empty()); // Death = false
        w.entities.get_mut(master).unwrap().death = true;
        let off = pet_cfg(|c| c.master_die_mutiny = false);
        assert!(process_slave_mutiny(&mut w, master, &off).is_empty());
        assert_eq!(w.entities.get(slave).unwrap().hp, 100);
    }

    /// `ProcessSlaveGhost`：`Ghost` 主人 ⇒ 逐宝宝 `MakeGhost`（末尾往前序）。
    #[test]
    fn process_slave_ghost_marks_all() {
        let mut w = World::new(32, 32);
        let master = w.add_entity(Entity::new(0, "主人".to_string(), 10, 10));
        let mut mk = || {
            let mut e = Entity::new(0, "宝宝".to_string(), 11, 10);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            e.master = Some(master);
            w.add_entity(e)
        };
        let s1 = mk();
        let s2 = mk();
        w.entities.get_mut(master).unwrap().slave_list = vec![s1, s2];
        // 主人未幽灵 ⇒ 无动作。
        assert!(process_slave_ghost(&mut w, master).is_empty());
        w.entities.get_mut(master).unwrap().ghost = true;
        assert_eq!(process_slave_ghost(&mut w, master), vec![s2, s1]);
        assert!(w.entities.get(s1).unwrap().ghost);
        assert!(w.entities.get(s2).unwrap().ghost);
    }

    /// 卡 `openmir2-m3-pet-slave-masterdie-wire`：主人死 ⇒ `SendSelfDelayMsg(...,1000)`（
    /// `PlayObject.cs:1996`）⇒ **+1000ms（虚拟时钟）**才到点投递 ⇒ 倒序逐宝宝、`Random(rate)==0`
    /// 支叛变（`PlayObject.Base.cs:331-357`）。
    #[test]
    fn master_die_self_delays_1000ms_then_mutinies_in_reverse() {
        let (mut w, master, s1) = slave_world(100);
        let s2 = {
            let mut e = Entity::new(0, "变异骷髅2".to_string(), 12, 10);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            e.master = Some(master);
            e.is_slave = true;
            e.hp = 100;
            e.max_hp = 100;
            e.dc = 10 | (20 << 8);
            e.ai.walk_speed = 400;
            w.add_entity(e)
        };
        for id in [s1, s2] {
            let s = w.entities.get_mut(id).unwrap();
            s.dc = 10 | (20 << 8);
            s.ai.walk_speed = 400;
        }
        w.entities.get_mut(master).unwrap().slave_list = vec![s1, s2];
        // oracle `SlaveList[i].Master.LastHiter != null` 门：置个活着的命中者。
        w.entities.get_mut(master).unwrap().last_hiter = Some(s1);
        w.enable_slave_self_delay(pet_cfg(|c| {
            c.master_die_mutiny = true;
            c.master_die_mutiny_rate = 1; // Random(1) 恒 0 ⇒ 必叛变
            c.master_die_mutiny_power = 2;
            c.master_die_mutiny_speed = 2;
        }));

        w.kill(master);
        let rec = w.tick(); // clock 0 -> 200：死亡结算 + 入队
        assert!(
            rec.deaths.iter().any(|(id, ..)| *id == master),
            "死亡应已结算"
        );
        assert!(w.entities.get(master).unwrap().death);
        assert_eq!(w.self_delay_msgs.len(), 1, "Die() 尾应入队一条自投递");
        assert_eq!(w.self_delay_msgs[0].ident, RM_MASTERDIEMUTINY);
        assert_eq!(
            w.self_delay_msgs[0].deliver_at_ms,
            200 + MASTER_DIE_SELF_DELAY_MS,
            "DeliveryTime = GetTickCount() + 1000"
        );
        assert!(
            rec.slave_self_deliveries.is_empty(),
            "本 tick 未到点 ⇒ 不投递"
        );

        // 到点前 4 个 tick（200ms 步进 ⇒ clock 400/600/800/1000）仍未投递。
        for _ in 0..4 {
            let r = w.tick();
            assert!(
                r.slave_self_deliveries.is_empty(),
                "clock {} 未到点",
                r.clock_ms
            );
        }
        assert_eq!(w.clock_ms, 1_000);
        assert_eq!(
            w.entities.get(s1).unwrap().master,
            Some(master),
            "未到点不变"
        );

        // 第 5 个 tick：clock = 1200 == deliver_at ⇒ 到点（`>=` 门，与 `GetMessage` 同）。
        let r = w.tick();
        assert_eq!(r.clock_ms, 1_200);
        assert_eq!(r.slave_self_deliveries.len(), 1, "到点应投递一条");
        let d = &r.slave_self_deliveries[0];
        assert_eq!(d.ident, RM_MASTERDIEMUTINY);
        assert_eq!(d.mutinied, vec![s2, s1], "倒序遍历 SlaveList");
        assert!(w.self_delay_msgs.is_empty(), "到点后出队");
        for id in [s1, s2] {
            let s = w.entities.get(id).unwrap();
            assert_eq!(s.master, None);
            assert_eq!(s.slave_exp_level, 15, "SlaveColor.Length");
            assert_eq!(s.dc, 20, "(ushort) 截断 ⇒ 只剩 LoByte(10)*power");
            assert_eq!(s.ai.walk_speed, 200, "WalkSpeed / speed");
        }
    }

    /// 主人死但 `LastHiter == null` ⇒ `&&` 短路：不抽数、逐宝宝 `HP = 0`（else 支）。
    #[test]
    fn master_die_self_delay_else_branch_zeros_hp() {
        let (mut w, master, slave) = slave_world(100);
        w.entities.get_mut(master).unwrap().last_hiter = None;
        w.enable_slave_self_delay(pet_cfg(|c| {
            c.master_die_mutiny = true;
            c.master_die_mutiny_rate = 1;
        }));
        w.kill(master);
        w.tick(); // 死亡 tick（clock 200）
        for _ in 0..5 {
            w.tick(); // 到点 tick（clock 1200）
        }
        let s = w.entities.get(slave).unwrap();
        assert_eq!(s.hp, 0, "else 支置 HP=0");
        assert_eq!(s.master, Some(master), "未叛变 ⇒ Master 不变");
    }

    /// BUG-REPLICA 第 4 条：`MakeGhost` 后 `Ghost == true`，`SendSelfDelayMsg` 首行的 `!Ghost` 门
    /// 随即拦截 ⇒ `RM_MASTERDIEGHOST` **永不入队**（`ProcessSlaveGhost` 在 oracle 里是死代码）。
    /// 本测试同时反证「投递支本身已接线」（手工入队可触发 `ProcessSlaveGhost`）。
    #[test]
    fn master_die_ghost_self_delay_is_blocked_by_ghost_gate() {
        let (mut w, master, slave) = slave_world(100);
        w.enable_slave_self_delay(pet_cfg(|_| {}));
        // `base.MakeGhost()` 的结果（玩家 `CanReAlive == false` ⇒ `Ghost = true`）。
        w.entities.get_mut(master).unwrap().ghost = true;
        w.send_master_die_ghost(master);
        assert!(
            w.self_delay_msgs.is_empty(),
            "Ghost 实体不入队（oracle `if (!Ghost)` 门）"
        );
        // 反证：绕过入口门手工入队，到点后 `Operate` 支确实调 `ProcessSlaveGhost`。
        w.self_delay_msgs.push(SelfDelayMsg {
            actor_id: master,
            ident: RM_MASTERDIEGHOST,
            w_param: 0,
            n_param1: 0,
            n_param2: 0,
            n_param3: 0,
            msg: String::new(),
            deliver_at_ms: w.clock_ms,
        });
        let r = w.tick();
        assert_eq!(r.slave_self_deliveries.len(), 1);
        assert_eq!(r.slave_self_deliveries[0].ghosted, vec![slave]);
        assert!(w.entities.get(slave).unwrap().ghost);
    }

    /// **P3-B3 must_fix 6**：`BaseObject.Message.cs:80` 的 `SendMsg` 同样有 `if (boSend && !Ghost)`
    /// ⇒ [`World::send_msg_now`]（即时入队）对 Ghost 实体不入队（与 `SendSelfDelayMsg` 的 `:190` 同门）。
    #[test]
    fn send_msg_now_is_blocked_by_ghost_gate() {
        let (mut w, _master, slave) = slave_world(100);
        w.entities.get_mut(slave).unwrap().ghost = true;
        w.send_msg_now(slave, RM_MAKEHOLYSEIZEMODE, 1000, 0, 0, 0);
        assert!(w.self_delay_msgs.is_empty(), "Ghost 实体不入队（`:80`）");
        // 对照：非 Ghost 照常入队（投递时刻 = 当前时钟）。
        w.entities.get_mut(slave).unwrap().ghost = false;
        w.send_msg_now(slave, RM_MAKEHOLYSEIZEMODE, 1000, 0, 0, 0);
        assert_eq!(w.self_delay_msgs.len(), 1);
        assert_eq!(w.self_delay_msgs[0].deliver_at_ms, w.clock_ms);
        assert_eq!(w.self_delay_msgs[0].w_param, 1000);
    }

    /// 诱惑受奴落尾（`MagicManager.cs:753-770`）。
    #[test]
    fn tame_adopt_writes_oracle_tail() {
        let mut w = World::new(32, 32);
        let caster = {
            let mut e = Entity::new(0, "道士".to_string(), 10, 10);
            e.race = crate::entity::ACTOR_RACE_PLAY;
            e.level = 10;
            w.add_entity(e)
        };
        let target = {
            let mut e = Entity::new(0, "鸡".to_string(), 11, 10);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            e.hp = 100;
            e.ai.walk_speed = 2000;
            e.ai.next_hit_time = 3000;
            e.ai.holy_seize = true;
            w.add_entity(e)
        };
        w.clock_ms = 1_000;
        assert!(w.tame_adopt(caster, target, 2, 5));
        let t = w.entities.get(target).unwrap();
        assert_eq!(t.master, Some(caster));
        assert_eq!(t.hp, 100, "原本无主 ⇒ 不 /10");
        assert_eq!(t.slave_make_level, 2);
        assert!(!t.is_slave, "诱惑不置 IsSlave（与 MakeSlave 不同）");
        assert_eq!(t.master_tick, 1_000, "MasterTick==0 ⇒ 记 now");
        assert_eq!(t.master_royalty_tick, tame_royalty_tick(10, 2, 5, 1_000));
        assert_eq!(t.ai.walk_speed, 1100, "min(2000, 1500-400)");
        assert_eq!(t.ai.next_hit_time, 1600, "min(3000, 2000-400)");
        assert!(!t.ai.holy_seize, "BreakHolySeizeMode");
        assert_eq!(w.entities.get(caster).unwrap().slave_list, vec![target]);
    }

    /// 诱惑受奴：目标已有别的 `Master` ⇒ 先 `HP/10`；`MasterTick` 已非 0 ⇒ 不覆盖。
    #[test]
    fn tame_adopt_halves_hp_and_keeps_master_tick() {
        let mut w = World::new(32, 32);
        let caster = w.add_entity(Entity::new(0, "道士".to_string(), 10, 10));
        let other = w.add_entity(Entity::new(0, "旧主".to_string(), 12, 12));
        let target = {
            let mut e = Entity::new(0, "半兽人".to_string(), 11, 10);
            e.race = crate::entity::ACTOR_RACE_MONSTER;
            e.master = Some(other);
            e.hp = 100;
            e.master_tick = 555;
            w.add_entity(e)
        };
        assert!(w.tame_adopt(caster, target, 3, 0));
        let t = w.entities.get(target).unwrap();
        assert_eq!(t.hp, 10);
        assert_eq!(t.master_tick, 555);
    }

    /// `MasterRoyaltyTick` 公式（`MagicManager.cs:759`）。
    #[test]
    fn tame_royalty_tick_formula() {
        // (5 + (2<<2)*5 + 20) * 60000 + 1000 = 65 * 60000 + 1000
        assert_eq!(tame_royalty_tick(10, 2, 5, 1_000), 3_901_000);
    }

    /// `GetChrColor`：NPC/宝宝色表/兜底（含 oracle latent 越界回落到兜底）。
    #[test]
    fn get_chr_color_branches() {
        let slave_color = [0xFF, 0xFE, 0x93, 0x9A];
        let mut s = Entity::new(0, "宝宝".to_string(), 0, 0);
        s.race = crate::entity::ACTOR_RACE_MONSTER;
        s.master = Some(1);
        s.slave_exp_level = 2;
        s.name_color = 200;
        assert_eq!(get_chr_color(&s, &slave_color, 77), 0x93);
        // 非宝宝 ⇒ 兜底 NameColor。
        s.master = None;
        assert_eq!(get_chr_color(&s, &slave_color, 77), 200);
        // NPC ⇒ NpcNameColor。
        s.race = ACTOR_RACE_NPC;
        assert_eq!(get_chr_color(&s, &slave_color, 77), 77);
        // 宝宝但等级越色表界（oracle latent OOB）⇒ 兜底，不 panic。
        s.race = crate::entity::ACTOR_RACE_MONSTER;
        s.master = Some(1);
        s.slave_exp_level = 20;
        assert_eq!(get_chr_color(&s, &slave_color, 77), 200);
    }

    /// **⑨-2 残余判据（46 分身）**：`spawn_clone` 造玩家型克隆体并落格——名字/坐标/
    /// 朝向=`GetBackDir(主人)`/格被占/不入 `SlaveList`/不抽 RNG。
    ///
    /// 红检（改坏必红）：① `dir` 改成主人的正向 `caster_dir`（不用 `back_dir`）⇒ 朝向断言必红；
    /// ② 不 `insert` 实体（只落格）⇒ `entities.get` 必红；③ 把 `e.race` 改成 `MONSTER`
    /// ⇒ race 断言必红。
    #[test]
    fn spawn_clone_creates_player_clone_and_occupies_cell() {
        let mut w = World::new(32, 32);
        let caster = {
            let mut e = Entity::new(0, "主人".to_string(), 7, 9);
            e.race = ACTOR_RACE_PLAY;
            w.add_entity(e)
        };
        w.tick(); // 让 `Enter` 生效（主人落图）。
        let mut fresh = w.rng.clone();
        let out = spawn_clone(&mut w, caster, 2).expect("主人在世界 ⇒ 应造出克隆体");
        // `Dir = GetBackDir(2)` = (2 + 4) % 8 = 6。
        assert_eq!(out.dir, 6, "PlayCloneObject.Dir = GetBackDir(主人.Dir)");
        assert_eq!((out.x, out.y), (7, 9), "坐标 = 主人坐标");
        assert!(out.created, "同格可放 ⇒ AddMapObject 成功");
        let c = w.entities.get(out.clone_id).expect("克隆体实体应存在");
        assert_eq!(c.name, CLONE_NAME);
        assert_eq!(c.race, ACTOR_RACE_PLAY, "玩家型克隆体");
        assert_eq!((c.x, c.y), (7, 9));
        assert!(c.add_to_mapped, "格占用成功 ⇒ AddToMaped");
        assert!(c.slave_list.is_empty(), "克隆体不入 SlaveList");
        assert!(
            w.entities.get(caster).unwrap().slave_list.is_empty(),
            "主人的 SlaveList 不被触碰"
        );
        // 格占用：同格 cell 里有该克隆体对象。
        let (cell, ok) = w.maps[0].get_cell_info(7, 9);
        assert!(ok);
        assert!(
            cell.obj_list
                .iter()
                .any(|o| o.cell_obj_id == out.clone_id && o.actor_object),
            "克隆体应进同格 obj_list"
        );
        // 不抽随机数：两流下一次取数相同。
        assert_eq!(w.rng.random(), fresh.random(), "构造克隆体不消费 RNG");
    }

    /// **⑨-2 残余判据（46 分身）**：主人不在世界 ⇒ `None`（不造实体、不抽数）。
    #[test]
    fn spawn_clone_absent_caster_returns_none() {
        let mut w = World::new(8, 8);
        assert!(spawn_clone(&mut w, 999, 0).is_none());
    }
}
