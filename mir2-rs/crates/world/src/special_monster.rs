//! 专用怪子类：`BeeQueen(103)` / `SpiderHouseMonster(116)` / `CloneMonster(201)`。
//!
//! 卡：`openmir2-run-gap-monster-specials`（tier=R）。
//!
//! 逐行 oracle 锚点（本片在 `706c95cae` 上实读）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`apply_ctor_fields`] | `BeeQueen.cs:13-21`、`SpiderHouseMonster.cs:13-21`、`Clone.cs:9-12` |
//! | [`special_monster_run`] | `BeeQueen.cs:51-79`、`SpiderHouseMonster.cs:45-73`（两份 `Run` 同形） |
//! | [`make_child`] | `BeeQueen.cs:23-31`、`SpiderHouseMonster.cs:23-31`（`SendRefMsg(RM_HIT)` + 500ms 自投递） |
//! | [`special_spawn_request`] / [`adopt_special_child`] | `BeeQueen.cs:33-44`、`SpiderHouseMonster.cs:33-43`（`Operate(RM_ZEN_BEE)`） |
//! | [`clone_operate`] | `Clone.cs:14-30`（`Operate` 三 ident + 恒 `return false`） |
//!
//! ## `Run` 的同形要点
//!
//! 两份 `Run` 都 **不** 继承 `MonsterObject.Run` 的 `Think`/`AttackTarget`/`GotoTargetXY`/
//! `Wondering`/`CheckRoyalty`/`CrazyMode` 段，只做：
//!
//! ```text
//! if (CanMove())
//!   if ((now - WalkTick) >= WalkSpeed)              // 注意是 >=，不是 >
//!     WalkTick = now;
//!     if ((now - AttackTick) >= NextHitTime)        // 同样是 >=
//!       AttackTick = now;
//!       SearchTarget();
//!       if (TargetCret != null) MakeChildBee()/GenBb();
//!     for (i = Count-1; i >= 0; i--)                // 倒序清理 Death/Ghost
//!       if (bb.Death || bb.Ghost) RemoveAt(i);
//! base.Run();
//! ```
//!
//! 因此本骨架在 [`crate::monai::monster_ai_tick`] 里对这两类怪走
//! [`special_monster_run`] → `animal_run` → `base_object_run_monster`，**不**调用
//! `MonsterObject.Run` 的通用段。
//!
//! ## 自投递（`RM_ZEN_BEE`，500ms）
//!
//! `SendSelfDelayMsg` 走本仓既有的 [`crate::slave::World::send_self_delay_msg`] 队列
//! （`slave::SelfDelayMsg`，`DeliveryTime = now + 500`）。本仓 `deliver_self_delay_msgs`
//! 的语义是「**跳过未到点项、继续扫描后续项**」；oracle `BaseObject.GetMessage`
//! （`BaseObject.Message.cs:334-355`）在**队头**未到点时重新以 `Lowest` 入队并返回
//! `false`（本轮消息段结束）。二者在「延时消息为 `Lowest` 优先级、其它消息先出队」的
//! 生产路径上等价；本卡的 `RM_ZEN_BEE` 是唯一在队的消息时行为相同。**登记**：本仓不做
//! 优先级队列，未到点项不阻塞其它已到点项（既有语义，非本卡新增）。
//!
//! ## 建模差异（登记，不忽略）
//!
//! - `RunTime = 250`（ctor）没有 AI 层消费点（oracle 的 `RunTime` 门在
//!   `ProcessMonsters` 线程的 `RunTick` 上，本仓 `monster_ai_tick` 每 world tick 一跑，
//!   与既有怪物 AI 同口径），登记不落字段。
//! - `RegenMonsterByName` 的**同步造怪 + 取数**在本仓经 [`SpecialSpawner`] 宿主接缝完成：
//!   生产入口是 `bins/game-svr::mongen::WorldSpecialSpawner`（复用
//!   `guardlist::regen_monster_by_name` 的 `CreateMonster` 取数链）；宿主 `sim_loop` 每 tick
//!   经 `tick_world_with_specials` 调 [`World::flush_special_spawns`] 排水
//!   [`World::pending_special_spawns`]。无 spawner 的宿主（测试/其它入口）沿用队列路径。
//! - `SendRefMsg(RM_HIT, …)` 的帧事件落在 [`World::pending_special_ref_msgs`]，由宿主
//!   `drain_special_ref_msgs` 按发帧坐标出 `SM_HIT`（收件人 ±12、排除自己）。
//! - `CloneMonster.Run`（`Clone.cs:46-79`）的追击/闪电攻击段不在本卡范围（卡只要求 ctor +
//!   `Operate`）；Clone 仍走既有通用 `monster_run`，登记为余量。
//! - `CloneMonster.Operate` 的 `RM_SPELL` 入口依赖 `SendRefMsg` 的广播面（oracle 只把
//!   `RM_SPELL` 发给 `Race == Play` 或 `WantRefMsg` 的对象）；本卡把处理体落成
//!   [`World::special_operate_message`]，帧面/广播接线登记为余量。
//! - `Master.WAbil.MP -= x` 是 C# 的 `ushort` 复合赋值（`unchecked` 回绕）⇒ Rust 用
//!   `wrapping_sub`。

use mir2_shared::rng::RandomNumber;

use crate::entity::Entity;
use crate::monai::{can_move, search_target, set_target_creature};
use crate::world::World;

/// `ActorRace.MonsterBeequeen`（`src/OpenMir2/Enums/Race.cs:85`）。
pub const RACE_BEE_QUEEN: u8 = 103;
/// `CreateMonster` 的 `case 116`（`WorldServer.MonGen.cs:672`）。
pub const RACE_SPIDER_HOUSE: u8 = 116;
/// `CreateMonster` 的 `case 201`（`WorldServer.MonGen.cs:705`）。
pub const RACE_CLONE_MONSTER: u8 = 201;

/// `Messages.RM_ZEN_BEE`（`src/OpenMir2/Messages.cs:652`）。
pub const RM_ZEN_BEE: u16 = 8020;
/// `Messages.RM_HIT`（`src/OpenMir2/Messages.cs:559`）。
pub const RM_HIT: u16 = 10004;
/// `Messages.RM_STRUCK`（`src/OpenMir2/Messages.cs:654`）。
pub const RM_STRUCK: u16 = 8018;
/// `Messages.RM_MAGSTRUCK`（`src/OpenMir2/Messages.cs:671`）。
pub const RM_MAGSTRUCK: u16 = 8044;
/// `Messages.RM_MAGSTRUCK_MINE`（`src/OpenMir2/Messages.cs:662`）。
pub const RM_MAGSTRUCK_MINE: u16 = 8030;
/// `Messages.RM_SPELL`（`src/OpenMir2/Messages.cs:562`）。
pub const RM_SPELL: u16 = 10008;
/// `Messages.RM_REFMESSAGE`（`src/OpenMir2/Messages.cs:578`）——`SendStruckDelayMsg` 的
/// 受击延迟消息。本仓等价投递点 = `AnimalState::pending_struck`（`monai`），Clone 屏蔽已落。
pub const RM_REFMESSAGE: u16 = 10101;
/// `Messages.RM_RANDOMSPACEMOVE`（`src/OpenMir2/Messages.cs:613`；`MapRandomMove`）。
/// 本仓未建模该投递（登记）；Clone 屏蔽由 [`clone_operate`] 的总门覆盖。
pub const RM_RANDOMSPACEMOVE: u16 = 10155;
/// `Messages.RM_DELAYPUSHED`（`src/OpenMir2/Messages.cs:667`；受击者 `CharPushed`）。
/// 本仓未建模该投递（只保留发送侧 `SpellStrikeOutcome::push`）；Clone 屏蔽由
/// [`clone_operate`] 的总门覆盖。
pub const RM_DELAYPUSHED: u16 = 8043;

/// `BeeQueen.MakeChildBee` / `SpiderHouseMonster.GenBb` 的上限：`Count >= 15` 不产。
pub const CHILD_LIMIT: usize = 15;
/// `SendSelfDelayMsg(..., 500)` 的延时。
pub const ZEN_BEE_DELAY_MS: i64 = 500;

/// `GameSvrConf.Bee` 的构造缺省（`GameSvrConf.cs:1316`）。
pub const DEFAULT_BEE_NAME: &str = "蝙蝠";
/// `GameSvrConf.Spider` 的构造缺省（`GameSvrConf.cs:1317`）。
pub const DEFAULT_SPIDER_NAME: &str = "爆裂蜘蛛";

/// `BeeQueen`/`SpiderHouseMonster` ctor 的 `ViewRange = 9`（`BeeQueen.cs:15`、`SpiderHouseMonster.cs:15`）。
pub const CHILD_QUEEN_VIEW_RANGE: i32 = 9;

/// 三种专用怪的身份（由 `CreateMonster` 的 race → 类映射推出）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialKind {
    /// `BeeQueen : AnimalObject`（`BeeQueen.cs:9`）。
    BeeQueen,
    /// `SpiderHouseMonster : AnimalObject`（`SpiderHouseMonster.cs:9`）。
    SpiderHouse,
    /// `CloneMonster : MonsterObject`（`Clone.cs:7`）。
    Clone,
}

impl SpecialKind {
    /// `CreateMonster` 的 race → 类映射（未命中 ⇒ 普通怪）。
    #[must_use]
    pub const fn from_race(race: u8) -> Option<Self> {
        match race {
            RACE_BEE_QUEEN => Some(Self::BeeQueen),
            RACE_SPIDER_HOUSE => Some(Self::SpiderHouse),
            RACE_CLONE_MONSTER => Some(Self::Clone),
            _ => None,
        }
    }

    /// `Run` 覆写族（仅 `BeeQueen`/`SpiderHouseMonster`；`Clone` 的 `Run` 不在本卡）。
    #[must_use]
    pub const fn has_special_run(self) -> bool {
        matches!(self, Self::BeeQueen | Self::SpiderHouse)
    }
}

/// `WorldEngine.RegenMonsterByName`（`WorldServer.MonGen.cs:382-432`）的宿主接缝。
///
/// 与 [`crate::slave::SlaveSpawner`] 同口径：宿主在**同一条** `world.rng` 流上按
/// `CreateMonster` 的次序抽数；本模块只负责调用点/门/落尾。
pub trait SpecialSpawner {
    /// 造一只怪并返回 actor id（oracle `RegenMonsterByName` 返回 `null` ⇒ `None`）。
    fn regen_monster_by_name(
        &mut self,
        world: &mut World,
        rng: &mut RandomNumber,
        map_id: usize,
        name: &str,
        x: i16,
        y: i16,
    ) -> Option<i32>;
}

/// 一条待执行的 `Operate(RM_ZEN_BEE)`（生产队列；见 [`World::flush_special_spawns`]）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecialSpawnRequest {
    /// 产仔者（`BeeQueen`/`SpiderHouseMonster`）actor id。
    pub owner_id: i32,
    pub map_id: usize,
    /// `BeeQueen` = `CurrX`；`SpiderHouseMonster` = `CurrX`（`n08 = CurrX`）。
    pub x: i32,
    /// `BeeQueen` = `CurrY`；`SpiderHouseMonster` = `CurrY + 1`（`n0C = (short)(CurrY + 1)`）。
    pub y: i32,
    /// `Config.Bee` / `Config.Spider`。
    pub name: String,
}

/// 一次 `flush_special_spawns` 的结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecialSpawnOutcome {
    pub owner_id: i32,
    /// 造出的宝宝 id（oracle `bb`；失败 = `None`）。
    pub child_id: Option<i32>,
    pub x: i32,
    pub y: i32,
    pub name: String,
}

/// `SendRefMsg(RM_HIT, Dir, CurrX, CurrY, 0, "")` 的帧事件（host 出 `SM_HIT`）。
///
/// `map_id` 是**发帧时刻**的地图：收件人按 `(map_id, x, y)` 现算（`SendRefMsg` 的
/// `CurrX±12 / CurrY±12` 窗口），不能用宿主 drain 时实体可能已更新的坐标。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecialRefMsg {
    pub actor_id: i32,
    pub ident: u16,
    pub dir: u8,
    pub x: i32,
    pub y: i32,
    pub map_id: usize,
}

/// 构造期字段（oracle ctor 里**不抽数**的那部分）。
///
/// - `BeeQueen`：`ViewRange = 9`、`SearchTick = GetTickCount()`、`StickMode = true`
///   （`BeeQueen.cs:15-19`）；`SearchTime` 已由生成路径的构造抽提供
///   （`bins/game-svr` 的 `draw_monster_construct`，`Random(1500) + 2500`）。
/// - `SpiderHouseMonster`：`ViewRange = 9`、**`SearchTick = 0`**、`StickMode = true`
///   （`SpiderHouseMonster.cs:15-19`）。
/// - `CloneMonster`：ctor 只覆盖 `SearchTime = Random(1500) + 1500`（已在构造抽里），
///   `SearchTick` 保持 `BaseObject` ctor 的 `GetTickCount()`。
pub fn apply_ctor_fields(e: &mut Entity, now: i64) {
    match SpecialKind::from_race(e.race) {
        Some(SpecialKind::BeeQueen) => {
            e.view_range = CHILD_QUEEN_VIEW_RANGE;
            e.stick_mode = true;
            e.ai.search_tick = now;
        }
        Some(SpecialKind::SpiderHouse) => {
            e.view_range = CHILD_QUEEN_VIEW_RANGE;
            e.stick_mode = true;
            // `SpiderHouseMonster.cs:18`：ctor 显式把 `SearchTick` 置 0（照抄）。
            e.ai.search_tick = 0;
        }
        Some(SpecialKind::Clone) => {
            // `Clone.cs:11`：只改 `SearchTime`（已由构造抽落定）；`SearchTick` 不动。
            e.ai.search_tick = now;
        }
        None => {}
    }
}

/// `MakeChildBee` / `GenBb`：`Count >= 15` 早退；否则 `SendRefMsg(RM_HIT)` +
/// `SendSelfDelayMsg(RM_ZEN_BEE, …, 500)`。
fn make_child(world: &mut World, owner_id: i32) {
    let Some((dir, x, y, map_id, hidden)) = world
        .entities
        .get(owner_id)
        .map(|e| (e.ai.dir, e.x, e.y, e.map_id, e.ob_mode || e.fixed_hide_mode))
    else {
        return;
    };
    let count = world
        .entities
        .get(owner_id)
        .map_or(0, |e| e.special_children.len());
    if count >= CHILD_LIMIT {
        return;
    }
    // `SendRefMsg`（`BaseObject.Message.cs:385-413`）：`ObMode || FixedHideMode` ⇒ 只自收
    // （本骨架不产可见帧）⇒ 不登记广播事件。
    if !hidden {
        world.pending_special_ref_msgs.push(SpecialRefMsg {
            actor_id: owner_id,
            ident: RM_HIT,
            dir,
            x,
            y,
            map_id,
        });
    }
    world.send_self_delay_msg(owner_id, RM_ZEN_BEE, ZEN_BEE_DELAY_MS);
}

/// 倒序遍历 `BeeList`/`_bbList`，移除 `Death || Ghost` 的项（实体已不存在同样视为移除，
/// 等价于 oracle 的强引用已不可达）。
fn cleanup_children(world: &mut World, owner_id: i32) {
    let Some(mut list) = world
        .entities
        .get(owner_id)
        .map(|e| e.special_children.clone())
    else {
        return;
    };
    if list.is_empty() {
        return;
    }
    for i in (0..list.len()).rev() {
        let remove = match world.entities.get(list[i]) {
            Some(child) => child.death || child.ghost,
            None => true,
        };
        if remove {
            list.remove(i);
        }
    }
    if let Some(e) = world.entities.get_mut(owner_id) {
        e.special_children = list;
    }
}

/// `BeeQueen.Run()` / `SpiderHouseMonster.Run()`（两份同形）。
///
/// 返回 `true` = 本实体是这两类之一（调用方**不得**再走通用 `monster_run`）。
pub fn special_monster_run(world: &mut World, actor_id: i32) -> bool {
    let Some(kind) = world
        .entities
        .get(actor_id)
        .and_then(|e| SpecialKind::from_race(e.race))
    else {
        return false;
    };
    if !kind.has_special_run() {
        return false;
    }
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return true;
    };
    if !can_move(e) {
        // oracle：`if (CanMove())` 为假 ⇒ 直接 `base.Run()`（本函数返回后由调用方跑）。
        return true;
    }
    // `(HUtil32.GetTickCount() - WalkTick) >= WalkSpeed` —— **>=**（等于就到点）。
    if now - e.ai.walk_tick < e.ai.walk_speed {
        return true;
    }
    let attack_due = now - e.ai.attack_tick >= e.ai.next_hit_time;
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.walk_tick = now;
    }
    if attack_due {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.attack_tick = now;
        }
        search_target(world, actor_id);
        let has_target = world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ai.target_creature.is_some());
        if has_target {
            make_child(world, actor_id);
        }
    }
    cleanup_children(world, actor_id);
    true
}

/// 依据 `Operate(RM_ZEN_BEE)` 的规则生成本次产仔请求；门不过 ⇒ `None`。
///
/// - `BeeQueen`：坐标 = `(CurrX, CurrY)`，无 `CanWalk` 门（`BeeQueen.cs:35-41`）。
/// - `SpiderHouseMonster`：坐标 = `(CurrX, CurrY + 1)`，先判
///   `Envir.CanWalk(n08, n0C, true)`（`SpiderHouseMonster.cs:35-42`；`true` = 忽略占位，
///   只要求界内且 `Valid`）。
#[must_use]
pub fn special_spawn_request(world: &World, actor_id: i32) -> Option<SpecialSpawnRequest> {
    let e = world.entities.get(actor_id)?;
    let kind = SpecialKind::from_race(e.race)?;
    let (x, y, name) = match kind {
        SpecialKind::BeeQueen => (e.x, e.y, world.bee_name.clone()),
        SpecialKind::SpiderHouse => {
            let y = e.y + 1;
            let can_walk = world
                .maps
                .get(e.map_id)
                .is_some_and(|m| m.get_cell_info(e.x, y).1);
            if !can_walk {
                return None;
            }
            (e.x, y, world.spider_name.clone())
        }
        SpecialKind::Clone => return None,
    };
    Some(SpecialSpawnRequest {
        owner_id: actor_id,
        map_id: e.map_id,
        x,
        y,
        name,
    })
}

/// `bb.SetTargetCreat(TargetCret); BeeList.Add(bb);`（`BeeQueen.cs:40-41` /
/// `SpiderHouseMonster.cs:39-40`）。
///
/// `SetTargetCreat(null)` 在 oracle 是空操作（只赋值）⇒ 主人当前无目标时只入表。
pub fn adopt_special_child(world: &mut World, owner_id: i32, child_id: i32) -> bool {
    let Some(target) = world
        .entities
        .get(owner_id)
        .and_then(|e| e.ai.target_creature)
    else {
        // 仍要入表（oracle `BeeList.Add(bb)` 无条件）。
        if let Some(e) = world.entities.get_mut(owner_id) {
            e.special_children.push(child_id);
            return true;
        }
        return false;
    };
    if world.entities.get(child_id).is_none() {
        return false;
    }
    set_target_creature(world, child_id, target);
    if let Some(e) = world.entities.get_mut(owner_id) {
        e.special_children.push(child_id);
        return true;
    }
    false
}

/// `CloneMonster.Operate`（`Clone.cs:14-30`）的处理体。
///
/// ```text
/// if (ident == RM_STRUCK || ident == RM_MAGSTRUCK || ident == RM_SPELL)
///   if (Master != null)
///     if (Master.WAbil.MP <= 0) WAbil.HP = 0;
///     if (ident == RM_SPELL) Master.WAbil.MP -= (ushort)nParam3;
///     else                   Master.WAbil.MP -= (ushort)wParam;
/// return false;                       // 三个 ident 之外也是 false（不跑 base.Operate）
/// ```
///
/// 返回值 = oracle `Operate` 的布尔返回（恒 `false`）。
pub fn clone_operate(
    world: &mut World,
    clone_id: i32,
    ident: u16,
    w_param: i32,
    n_param3: i32,
) -> bool {
    let Some(master_id) = world.entities.get(clone_id).and_then(|e| e.master) else {
        return false;
    };
    let Some(master_mp) = world.entities.get(master_id).map(|m| m.mp) else {
        return false;
    };
    if !matches!(ident, RM_STRUCK | RM_MAGSTRUCK | RM_SPELL) {
        return false;
    }
    // 先判 MP（oracle 的次序），再扣。
    if master_mp == 0 {
        if let Some(e) = world.entities.get_mut(clone_id) {
            e.hp = 0;
        }
    }
    // `(ushort)` 截断 + `ushort` 复合赋值的 unchecked 回绕。
    let delta = if ident == RM_SPELL {
        n_param3 as u16
    } else {
        w_param as u16
    };
    if let Some(m) = world.entities.get_mut(master_id) {
        m.mp = m.mp.wrapping_sub(delta);
    }
    false
}

/// oracle `Operate(processMsg)` 的专用怪分派。
///
/// 返回：
/// - `None` = 不是专用怪（或 Bee/Spider 的非 `RM_ZEN_BEE` 消息）⇒ 调用方跑 base.Operate；
/// - `Some(false)` = `CloneMonster.Operate`（恒不跑 base）；
/// - `Some(true)` = Bee/Spider 的 `RM_ZEN_BEE`（base.Operate 不处理该 ident）。
pub fn operate_special<S: SpecialSpawner + ?Sized>(
    world: &mut World,
    spawner: &mut S,
    actor_id: i32,
    ident: u16,
    w_param: i32,
    n_param3: i32,
) -> Option<bool> {
    let kind = world
        .entities
        .get(actor_id)
        .and_then(|e| SpecialKind::from_race(e.race))?;
    match kind {
        SpecialKind::Clone => Some(clone_operate(world, actor_id, ident, w_param, n_param3)),
        SpecialKind::BeeQueen | SpecialKind::SpiderHouse => {
            if ident != RM_ZEN_BEE {
                return None;
            }
            let Some(req) = special_spawn_request(world, actor_id) else {
                return Some(true);
            };
            let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(0));
            let child = spawner.regen_monster_by_name(
                world,
                &mut rng,
                req.map_id,
                &req.name,
                req.x as i16,
                req.y as i16,
            );
            world.rng = rng;
            if let Some(child) = child {
                adopt_special_child(world, actor_id, child);
            }
            Some(true)
        }
    }
}

impl World {
    /// **`Operate(RM_ZEN_BEE)` 的生产队列排水**：逐条 `RegenMonsterByName` → 成功后
    /// `SetTargetCreat` + 入 `BeeList`/`_bbList`。
    ///
    /// 队列由 [`crate::slave::World::deliver_self_delay_msgs`] 在投递 `RM_ZEN_BEE` 时填充
    /// （无宿主 spawner 的路径）；宿主取数仍在**同一条** `world.rng` 上完成。
    ///
    /// **次序登记**：oracle 在产仔者自己的 `Run()` 消息段内同步 `RegenMonsterByName`
    /// （`ProcessMonsters` 循环中该怪的位置）；本仓 `World::tick` 的自投递段在怪物 AI 之前，
    /// 生产宿主在此后调本方法 ⇒ 同一 tick 内“专用怪产仔抽数”相对其它 AI/NPC/商人抽数的
    /// 位置与 oracle 不同。**无到点 `RM_ZEN_BEE` 的 tick 零抽数**（五批 trace 逐字节不变）；
    /// 有专用怪的场景需真机窗口核抽数次序（R 级余量）。
    pub fn flush_special_spawns<S: SpecialSpawner + ?Sized>(
        &mut self,
        spawner: &mut S,
    ) -> Vec<SpecialSpawnOutcome> {
        let reqs = std::mem::take(&mut self.pending_special_spawns);
        let mut out = Vec::with_capacity(reqs.len());
        for req in reqs {
            let mut rng = std::mem::replace(&mut self.rng, RandomNumber::with_seed(0));
            let child = spawner.regen_monster_by_name(
                self,
                &mut rng,
                req.map_id,
                &req.name,
                req.x as i16,
                req.y as i16,
            );
            self.rng = rng;
            if let Some(child) = child {
                adopt_special_child(self, req.owner_id, child);
            }
            out.push(SpecialSpawnOutcome {
                owner_id: req.owner_id,
                child_id: child,
                x: req.x,
                y: req.y,
                name: req.name,
            });
        }
        out
    }

    /// `Operate` 消息门的**生产入口**（当前用于 `RM_STRUCK`/`RM_MAGSTRUCK` 的 Clone 支，
    /// 以及测试/后续 `RM_SPELL` 帧面接线）。返回 `None` = 非专用怪。
    pub fn special_operate_message<S: SpecialSpawner + ?Sized>(
        &mut self,
        spawner: &mut S,
        actor_id: i32,
        ident: u16,
        w_param: i32,
        n_param3: i32,
    ) -> Option<bool> {
        operate_special(self, spawner, actor_id, ident, w_param, n_param3)
    }

    /// 取走本 tick 的 `RM_HIT` 帧事件（保序）。
    pub fn take_special_ref_msgs(&mut self) -> Vec<SpecialRefMsg> {
        std::mem::take(&mut self.pending_special_ref_msgs)
    }

    /// `GameSvrConf.Bee` / `Spider` 的 `[Names]` 键（`ServerConf.cs:100-101`）。
    pub fn set_special_monster_names(&mut self, bee: &str, spider: &str) {
        self.bee_name = bee.to_string();
        self.spider_name = spider.to_string();
    }
}
