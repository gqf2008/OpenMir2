//! 宝宝 / 下属（slave）召唤与召回 —— 1:1 复刻 C# `BaseObject.MakeSlave` 一族 +
//! `MagicManager.MagMakeSlave` / `MagMakeSinSuSlave` / `MagMakeAngelSlave` 的**选型与门**。
//!
//! 交付卡：`openmir2-m3-pet-slave-summon`（M3⑨-实现 2）。
//! 盘点依据：`mir2-rs/tests/parity/m3/M3⑨英雄宠物现状盘点报告-20261013.md` §1.B/§1.F。
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
//! - **不负责**：等级色彩 `GetChrColor`/`SlaveColor`、叛变 `CheckRoyalty`、死亡清表
//!   （归 `openmir2-m3-pet-slave-lifecycle`，⑨-3）；本模块只把 `SlaveExpLevel`/`MasterRoyaltyTick`
//!   等**写在实体上**供 ⑨-3 消费。
//! - **46 分身**（`MagMakeClone`，`MagicManager.cs:1337-1341`）：`new PlayCloneObject(PlayObject)`
//!   造的是一个**玩家型克隆体**（不入 `SlaveList`、不走 `MakeSlave`），与宝宝系统不同轴；
//!   本片**登记未实现**（见 [`CLONE_DEFERRED`]）。
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

use mir2_data::pet_slave::{PetSlaveConfig, SLAVE_MAX_LEVEL};
use mir2_shared::rng::RandomNumber;

use crate::entity::{Entity, ACTOR_RACE_PLAY};
use crate::magic::front_position;
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

/// `MagicConst.SKILL_46` 的登记（本片未实现，理由见模块头）。
pub const CLONE_DEFERRED: &str =
    "46 分身 = `new PlayCloneObject(PlayObject)`（造玩家型克隆体，不入 SlaveList）；\
    与宝宝系统不同轴 ⇒ 归后续片（登记，不冒充已实现）";

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

/// `BaseObject.SpaceMoveGetRandXY`（`BaseObject.cs:1040-1096`）：从 `(x, y)` 起找可走格；
/// 不可走时**按 oracle 的分段步进/随机跳点**推进，最多 201 次尝试（`n14 >= 201` 即放弃）。
///
/// `n18 = Width < 80 ? 3 : 10`；`n1C = Height < 150 ? (Height < 50 ? 2 : 15) : 50`。
/// `can_walk(x, y)` = oracle `Envir.CanWalk(x, y, true)`（`cellSuccess && Valid`）。
///
/// **取数次序是 Claim B 账本输入**：只有进入 `else` 分支（`x` 已到 `Width-n1C-1` 之后）才抽
/// `Random(Width)` / `Random(Height)`，本函数逐字保留分支位置。
pub fn space_move_get_rand_xy<F>(
    rng: &mut RandomNumber,
    width: i32,
    height: i32,
    mut x: i32,
    mut y: i32,
    mut can_walk: F,
) -> Option<(i32, i32)>
where
    F: FnMut(i32, i32) -> bool,
{
    let n18 = if width < 80 { 3 } else { 10 };
    let n1c = if height < 150 {
        if height < 50 {
            2
        } else {
            15
        }
    } else {
        50
    };
    let mut attempts = 0;
    loop {
        if can_walk(x, y) {
            return Some((x, y));
        }
        if x < (width - n1c - 1) {
            x += n18;
        } else {
            x = rng.random_below(width);
            if y < (height - n1c - 1) {
                y += n18;
            } else {
                y = rng.random_below(height);
            }
        }
        attempts += 1;
        if attempts >= 201 {
            break;
        }
    }
    None
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
    /// 5. `RecalcAbilitys()`（`BaseObject.Abil.cs:15-…` 对**基类**怪只把 `WAbil = Abil.Clone()`
    ///    并保留 HP/MP ⇒ 本骨架已存 `hp/max_hp`，此处等价 no-op；派生类
    ///    `WhiteSkeleton`/`ElfMonster` 的速度改写归 ⑨-3）；
    /// 6. `if (WAbil.HP < WAbil.MaxHP) HP += (MaxHP - HP) / 2`；
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
        // `if (monObj.WAbil.HP < monObj.WAbil.MaxHP) HP += (MaxHP - HP) / 2;`（`:1198-1201`）。
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
        // (100-30)/2 = 35 ⇒ HP 65。
        assert_eq!(m.hp, 65);
        assert_eq!(w.entities.get(caster).unwrap().slave_list, vec![mon]);
        assert_eq!(w.slave_count(caster), 1);
    }

    /// HP 已满 ⇒ 不补（oracle `if (HP < MaxHP)`）。
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
        assert!(w.make_slave_adopt(caster, mon, 1, 1, 1));
        assert_eq!(w.entities.get(mon).unwrap().hp, 100);
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
        assert_eq!(m.hp, 40 + (100 - 40) / 2, "HP 补半");
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
}
