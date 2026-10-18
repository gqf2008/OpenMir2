//! 怪物 / 动物 AI —— oracle `AnimalObject` / `MonsterObject` 的**确定性子集**（M3④-实现 C 第 ① 片）。
//!
//! oracle 锚点（逐函数，行号复核于本片）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`walk_to`] | `BaseObject.WalkTo`（`src/M2Server/Actor/BaseObject.cs:601-...`） |
//! | [`turn_to`] | `BaseObject.TurnTo`（`BaseObject.cs:1582`） |
//! | [`wondering`] | `AnimalObject.Wondering`（`src/M2Server/Actor/AnimalObject.cs:425-441`） |
//! | [`search_target`] | `AnimalObject.SearchTarget`（`AnimalObject.cs:388-423`） |
//! | [`goto_target_xy`] | `AnimalObject.GotoTargetXY`（`AnimalObject.cs:174-249`） |
//! | [`think`] | `MonsterObject.Think`（`src/M2Server/Monster/MonsterObject.cs:187-215`） |
//! | [`attack_target`] | `MonsterObject.AttackTarget`（`MonsterObject.cs:220-245`）+ `BaseObject.GetAttackDir`（`BaseObject.cs:1264`） |
//! | [`monster_run`] | `MonsterObject.Run`（`MonsterObject.cs:251-360`） |
//!
//! ## RNG 取数次序（Claim B 两侧 site/order 账本的输入；**不许为可测性改序**）
//!
//! - `Wondering`：先 1 次 `Random(20)`；≠0 直接返回（**不再抽**）。==0 时 1 次 `Random(4)`：
//!   ==1 ⇒ 1 次 `RandomByte(8)` 后 `TurnTo`；否则 `WalkTo(Dir)`（`WalkTo`/`TurnTo` 本身不抽）。
//! - `GotoTargetXY`：进入后（目标格与当前格不同）先 `WalkTo(nDir)`，随后**无条件** 1 次
//!   `Random(3)`（用于未移动时的 8 向重试循环；每次重试不再抽）。
//! - `Think`：仅当 `now - ThinkTick > 3000` 时刷新；`DupMode` 支 `WalkTo(RandomByte(8))`。
//! - `AttackTarget`：命中门槛通过且到时间才攻击；攻击取数走 [`crate::world::World::attack_once`]
//!   （= oracle `GetBaseAttackPoewr` + `_Attack` 的怪物路径）。
//!
//! ## 与 oracle 的建模差异（登记，不在本片修）
//!
//! - **线程模型**：oracle 的 `ProcessMonsters` 跑在独立线程（`WorldServer.MonGen.cs:192`），
//!   本 crate 单线程 tick 串行（设计文档 §4.2 硬约束①）。本片把「每 tick 对每只怪跑一次
//!   `Run()`」按**实体槽位顺序**串行执行（顺序来自 `EntityStore` 的显式槽位索引）。
//! - **`BaseObject.Run` 的公共段已分归两处**（`BaseObject.Base.cs:24-316`）：
//!   - [`base_object_run_monster`]：恢复 `HealthTick/SpellTick`（`:39-66`）、体力点段（`:105-110`）、
//!     `TargetCret` 清理（`:111-118`）——每只怪在 [`monster_run`] 之后跑（oracle `MonsterObject.Run`
//!     末尾的 `base.Run()`，`MonsterObject.cs:255/359`）；
//!   - [`crate::world::World`]：`HP==0 ⇒ Die()`（`:70-73`，由 `settle_deaths` 死亡结算等价）、
//!     `LastHiter` 30s / `ExpHiter` 6s 过期（`:119-132`，由 `expire_hitters`）。
//! - **仍未建模**（登记）：`SuperMan`（`:35-38`）、`Master` 跟随/`SlaveRelax` 分支（`:133-215`）、
//!   `ShowHp`/`VerifyTick` 的 `ShowHp` 支（`:217-256`；`VerifyTick` 已接）。死亡支
//!   `MakeGhost`（`:85-101`）已由本片接上（[`make_ghost`]，门见
//!   [`crate::mongen::monster_death_gate`]）。**⑨-3 已接**：`CheckRoyalty()`（`MonsterObject.cs:352`）
//!   在 [`monster_run`] 的两个提前返回口之外复刻调用；宝宝列表清表（`BaseObject.Base.cs:208-217`）
//!   在 [`crate::world::World::tick`] 里集中跑（见 [`crate::slave`]）。
//! - **构造期 RNG**：oracle 在 `new MonsterObject()` 时先抽 `BaseObject` 的 `Random(1500)`+
//!   `Random(2000)`、`AnimalObject` 的 `Random(3000)`×2、`MonsterObject` 的 `Random(2000)`
//!   （`BaseObject.cs:439/441`、`AnimalObject.cs:76/77`、`MonsterObject.cs:65`），**再**由
//!   `CreateMonster` 抽 `RandomByte(8)` 的 `Dir`（`WorldServer.MonGen.cs:740`）。该缺口已由
//!   卡 `openmir2-m3-mongen-load-spawn` 的 F-1 返工闭合：生成路径把构造抽存进
//!   `Entity.ctor_draws`（[`MonsterCtorDraws`]），AI 在宿主接线时只**消费**（
//!   [`AnimalState::seed_from_spawn`](crate::entity::AnimalState::seed_from_spawn)），**不重抽**。
//! - **专用怪子类**（`ATMonster`/`BeeQueen`/... 在各自 `Run()` 里调 `SearchTarget`）与
//!   `Mission`/`RunAway`/`Master`/`CheckRoyalty` 分支**未建模**：默认值下它们
//!   恒不命中，与「普通 `MonsterObject`、无 master、无任务」的 oracle 执行路径一致。
//! - **`CrazyMode`（`MonsterObject.cs:49-57/353-356/465-480`）**：状态与到期已建模
//!   （[`open_crazy_mode`]/[`break_crazy_mode`]/[`crazy_mode_expiry`]）；生产写入点 =
//!   20 诱惑之光的 `OpenCrazyMode(Random(20)+10)`（`MagicManager.cs:797/805`，抽数先由
//!   [`crate::magic_effect::mag_tamming`] 按同序消费并把值随 `crazy_interval` 返回，
//!   宿主 `cast_tamming_spell` 落状态）。`GetChrColor` 的 `crazy_mode ⇒ 0xF9` 色支仍登记未实现。
//! - **`AnimalObject.Run` 的 HolySeize 到期**（`AnimalObject.cs:142-149`）已建模为
//!   [`animal_run`]（严格 `>`），由 [`monster_ai_tick`] 在 `monster_run` 之后、`base.Run` 之前调用
//!   （= oracle 的 `MonsterObject.Run` 尾 `base.Run()` → `AnimalObject.Run`）。`RefNameColor` 帧不在此层。

use crate::combat::is_proper_target;
use crate::entity::{Entity, ACTOR_RACE_PLAY};
use crate::magic::{
    front_position, DIR_DOWN, DIR_DOWN_LEFT, DIR_DOWN_RIGHT, DIR_LEFT, DIR_RIGHT, DIR_UP,
    DIR_UP_LEFT, DIR_UP_RIGHT,
};
use crate::world::World;

/// `BaseObject.WalkTo` 的 `Dir → (dx, dy)`（`BaseObject.cs:601-...` 的 switch）。
///
/// 与 oracle 同构：越界由调用方（`World::try_step_now` 的格子校验）拒绝，本函数只给增量。
#[must_use]
pub fn dir_delta(dir: u8) -> (i32, i32) {
    match dir {
        DIR_UP => (0, -1),
        DIR_UP_RIGHT => (1, -1),
        DIR_RIGHT => (1, 0),
        DIR_DOWN_RIGHT => (1, 1),
        DIR_DOWN => (0, 1),
        DIR_DOWN_LEFT => (-1, 1),
        DIR_LEFT => (-1, 0),
        DIR_UP_LEFT => (-1, -1),
        _ => (0, 0),
    }
}

/// `BaseObject.TurnTo`（`BaseObject.cs:1582`）：只改 `Dir`（帧面 `RM_TURN` 由接线层发）。
pub fn turn_to(world: &mut World, actor_id: i32, dir: u8) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = dir;
    }
}

/// `BaseObject.WalkTo(btDir, boFlag)`（`BaseObject.cs:601`）：**先置 `Dir`**（即使走不动也保留），
/// 再尝试单步移动。返回是否真的移动（oracle 的 `result`）。
///
/// `fearFire`（`BoFearFire`）在本片未建模（普通怪 false；火墙 `CanSafeWalk` 语义不在本片）。
pub fn walk_to(world: &mut World, actor_id: i32, dir: u8) -> bool {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = dir;
    }
    let (dx, dy) = dir_delta(dir);
    if dx == 0 && dy == 0 {
        return false;
    }
    world.try_step_now(actor_id, dx, dy, 1)
}

/// `Envirnoment.GetXYObjCount`（`src/M2Server/Maps/Envirnoment.cs:1062-1093`）的 actor 计数子集：
/// 统计 `(x, y)` 格上 `!Ghost && !Death && !FixedHideMode && !ObMode` 的 actor 对象数。
fn count_at_cell(world: &World, map_id: usize, x: i32, y: i32) -> i32 {
    let Some(map) = world.maps.get(map_id) else {
        return 0;
    };
    let (cell, ok) = map.get_cell_info(x, y);
    if !ok || !cell.is_available() {
        return 0;
    }
    let mut count = 0;
    for o in &cell.obj_list {
        if !o.actor_object {
            continue;
        }
        if world
            .entities
            .get(o.cell_obj_id)
            .is_some_and(|e| !e.ghost && !e.death && !e.fixed_hide_mode && !e.ob_mode)
        {
            count += 1;
        }
    }
    count
}

/// `BaseObject.GetAttackDir`（`BaseObject.cs:1264-1313`）：目标在**八邻格**时给出朝向。
///
/// `None` = 不满足邻接（含同格）；`Some(dir)` = 命中（oracle 返回 true 且 `btDir` 有值）。
#[must_use]
pub fn get_attack_dir(world: &World, actor_id: i32, target_id: i32) -> Option<u8> {
    let a = world.entities.get(actor_id)?;
    let t = world.entities.get(target_id)?;
    let (ax, ay) = (a.x, a.y);
    let (tx, ty) = (t.x, t.y);
    if !(ax - 1 <= tx && ax + 1 >= tx && ay - 1 <= ty && ay + 1 >= ty) || (ax == tx && ay == ty) {
        return None;
    }
    // oracle 的判定顺序：Left, Right, Up, Down, UpLeft, UpRight, DownLeft, DownRight。
    if ax - 1 == tx && ay == ty {
        return Some(DIR_LEFT);
    }
    if ax + 1 == tx && ay == ty {
        return Some(DIR_RIGHT);
    }
    if ax == tx && ay - 1 == ty {
        return Some(DIR_UP);
    }
    if ax == tx && ay + 1 == ty {
        return Some(DIR_DOWN);
    }
    if ax - 1 == tx && ay - 1 == ty {
        return Some(DIR_UP_LEFT);
    }
    if ax + 1 == tx && ay - 1 == ty {
        return Some(DIR_UP_RIGHT);
    }
    if ax - 1 == tx && ay + 1 == ty {
        return Some(DIR_DOWN_LEFT);
    }
    if ax + 1 == tx && ay + 1 == ty {
        return Some(DIR_DOWN_RIGHT);
    }
    None
}

/// `AnimalObject.Wondering`（`AnimalObject.cs:425-441`）。
///
/// 取数：`Random(20)`；非 0 返回。否则 `Random(4)`：==1 ⇒ `RandomByte(8)` + `TurnTo`；
/// 否则 `WalkTo(Dir)`。
pub fn wondering(world: &mut World, actor_id: i32) {
    if world.rng.random_below(20) != 0 {
        return;
    }
    if world.rng.random_below(4) == 1 {
        let dir = world.rng.random_byte(8);
        turn_to(world, actor_id, dir);
    } else {
        let dir = world.entities.get(actor_id).map_or(0, |e| e.ai.dir);
        walk_to(world, actor_id, dir);
    }
}

/// `AnimalObject.SearchTarget`（`AnimalObject.cs:388-423`）。
///
/// 遍历 `VisibleActors`（oracle 的插入序），逐项：死亡/隐身/异图/`|Δx|>15`/`|Δy|>15`
/// ⇒ `ClearTargetCreat`（从可见集移除）；否则 `IsProperTarget && (!HideMode || CoolEye)` 且
/// 曼哈顿距离更近者被选中。最后 `SetTargetCreat(最近者)`。
pub fn search_target(world: &mut World, actor_id: i32) {
    let visible: Vec<i32> = match world.entities.get(actor_id) {
        Some(e) => e.visible_actors.iter().map(|v| v.target).collect(),
        None => return,
    };
    let Some((cx, cy, map_id, cool_eye)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.x, e.y, e.map_id, e.ai.cool_eye))
    else {
        return;
    };
    // oracle `int n10 = 999;`（`AnimalObject.cs:390`）：全部候选距离都 ≥999 时**一个都不选**
    // （±15 窗口内曼哈顿距离上限 30，不可达，但按 1:1 保留该初值）。
    let mut n10 = 999;
    let mut best: Option<i32> = None;
    let mut to_clear: Vec<i32> = Vec::new();
    for target_id in visible {
        let Some(t) = world.entities.get(target_id) else {
            to_clear.push(target_id);
            continue;
        };
        if t.death
            || t.ghost
            || t.map_id != map_id
            || (t.x - cx).abs() > 15
            || (t.y - cy).abs() > 15
        {
            to_clear.push(target_id);
            continue;
        }
        // oracle 在 `ClearTargetCreat` 之后还写了一遍 `if (!baseObject.Death)`（`AnimalObject.cs:402`）：
        // 上面的 `baseObject.Death` 已 `continue` ⇒ 该层不可达（照搬时省略，行为等价）。
        // 隐藏门读的是 `HideMode`（**不是** `FixedHideMode`，后者只用于 `GetXYObjCount`/`Run`）。
        let proper = world
            .entities
            .get(actor_id)
            .is_some_and(|a| is_proper_target(a, t) && (!t.hide_mode || cool_eye));
        if proper {
            let dist = (cx - t.x).abs() + (cy - t.y).abs();
            if dist < n10 {
                n10 = dist;
                best = Some(target_id);
            }
        }
    }
    if !to_clear.is_empty() {
        if let Some(e) = world.entities.get_mut(actor_id) {
            // `BaseObject.ClearTargetCreat`（`BaseObject.ViewRange.cs`）：把该目标从可见集移除。
            e.visible_actors.retain(|v| !to_clear.contains(&v.target));
        }
    }
    if let Some(target_id) = best {
        set_target_creature(world, actor_id, target_id);
    }
}

/// `BaseObject.SetTargetCreat`：置 `TargetCret` 并刷新 `TargetFocusTick`。
pub fn set_target_creature(world: &mut World, actor_id: i32, target_id: i32) {
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.target_creature = Some(target_id);
        e.ai.target_focus_tick = now;
    }
}

/// `BaseObject.DelTargetCreat`（`AnimalObject.cs:151` override 追加 `TargetX/Y = -1`）。
pub fn del_target_creature(world: &mut World, actor_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.target_creature = None;
        e.ai.target_x = -1;
        e.ai.target_y = -1;
    }
}

/// `BaseObject.ClearTargetCreat`（`BaseObject.Base.cs:568-585`）：先把目标从 `VisibleActors`
/// 里删掉（首个匹配，`VisibleActors[i].BaseObject == baseObject` ⇒ Rust 的 `v.target`），
/// 再 `DelTargetCreat()`。
///
/// 与 [`del_target_creature`] 分开：oracle 里 `ClearTargetCreat` 用于 `Run()` 的目标清理段
/// （`BaseObject.Base.cs:111-118`）、`DelTargetCreat` 用于其它路径，两者不同源。
pub fn clear_target_creature(world: &mut World, actor_id: i32, target_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        if let Some(i) = e.visible_actors.iter().position(|v| v.target == target_id) {
            e.visible_actors.remove(i);
        }
        e.ai.target_creature = None;
        e.ai.target_x = -1;
        e.ai.target_y = -1;
    }
}

/// `AnimalObject.GotoTargetXY`（`AnimalObject.cs:174-249`）。
///
/// 目标格 ≠ 当前格时：按 Δ 定主方向 → `WalkTo` → **无条件** `Random(3)` → 最多 8 次
/// （`Dir.Up..=Dir.UpLeft`）在「仍未移动」时按 `n20` 顺时针/逆时针微调方向再 `WalkTo`。
pub fn goto_target_xy(world: &mut World, actor_id: i32) {
    let Some((cx, cy, tx, ty)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.x, e.y, e.ai.target_x, e.ai.target_y))
    else {
        return;
    };
    if cx == tx && cy == ty {
        return;
    }
    let mut n_dir = DIR_DOWN;
    if tx > cx {
        n_dir = DIR_RIGHT;
        if ty > cy {
            n_dir = DIR_DOWN_RIGHT;
        }
        if ty < cy {
            n_dir = DIR_UP_RIGHT;
        }
    } else if tx < cx {
        n_dir = DIR_LEFT;
        if ty > cy {
            n_dir = DIR_DOWN_LEFT;
        }
        if ty < cy {
            n_dir = DIR_UP_LEFT;
        }
    } else if ty > cy {
        n_dir = DIR_DOWN;
    } else if ty < cy {
        n_dir = DIR_UP;
    }
    let (old_x, old_y) = (cx, cy);
    walk_to(world, actor_id, n_dir);
    let n20 = world.rng.random_below(3);
    for _ in DIR_UP..=DIR_UP_LEFT {
        let Some((nx, ny)) = world.entities.get(actor_id).map(|e| (e.x, e.y)) else {
            return;
        };
        if old_x == nx && old_y == ny {
            if n20 != 0 {
                n_dir += 1;
            } else if n_dir > 0 {
                n_dir -= 1;
            } else {
                n_dir = DIR_UP_LEFT;
            }
            if n_dir > DIR_UP_LEFT {
                n_dir = DIR_UP;
            }
            walk_to(world, actor_id, n_dir);
        }
    }
}

/// `AnimalObject.CanMove`（`AnimalObject.cs:157-160`）：`!Ghost && !Death && Stone == 0`。
///
/// 本骨架用 [`Entity::stone_mode`]（`StatusTimeArr[PoisonState.STONE]` 的折叠）替代石化计时。
#[must_use]
pub fn can_move(e: &Entity) -> bool {
    !e.ghost && !e.death && !e.stone_mode
}

/// `MonsterObject.Think`（`MonsterObject.cs:187-215`）：3s 一次的 `DupMode`/目标有效性刷新，
/// `DupMode` 时 `WalkTo(RandomByte(8))`，移动成功即清 `DupMode` 并返回 true。
pub fn think(world: &mut World, actor_id: i32) -> bool {
    let now = world.clock_ms;
    let Some((x, y, map_id)) = world.entities.get(actor_id).map(|e| (e.x, e.y, e.map_id)) else {
        return false;
    };
    if now - world.entities.get(actor_id).map_or(0, |e| e.ai.think_tick) > 3 * 1000 {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.think_tick = now;
        }
        if count_at_cell(world, map_id, x, y) >= 2 {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.dup_mode = true;
            }
        }
        let target = world
            .entities
            .get(actor_id)
            .and_then(|e| e.ai.target_creature);
        if let Some(t) = target {
            let proper = world
                .entities
                .get(actor_id)
                .zip(world.entities.get(t))
                .is_some_and(|(a, b)| is_proper_target(a, b));
            if !proper {
                if let Some(e) = world.entities.get_mut(actor_id) {
                    e.ai.target_creature = None;
                }
            }
        }
    }
    let mut result = false;
    let dup = world.entities.get(actor_id).is_some_and(|e| e.ai.dup_mode);
    if dup {
        if world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ai.holy_seize)
        {
            return false;
        }
        let (old_x, old_y) = match world.entities.get(actor_id) {
            Some(e) => (e.x, e.y),
            None => return false,
        };
        let dir = world.rng.random_byte(8);
        let bo_fear_fire = world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ai.bo_fear_fire);
        let _ = bo_fear_fire; // `WalkTo(..., BoFearFire)` 的 fearFire 子集不在本片
        walk_to(world, actor_id, dir);
        if let Some(e) = world.entities.get(actor_id) {
            if old_x != e.x || old_y != e.y {
                if let Some(e) = world.entities.get_mut(actor_id) {
                    e.ai.dup_mode = false;
                }
                result = true;
            }
        }
    }
    result
}

/// `MonsterObject.AttackTarget`（`MonsterObject.cs:220-245`）。
///
/// 邻接 ⇒ 到点（`now - AttackTick > NextHitTime`）则刷新 `AttackTick`/`TargetFocusTick`、
/// `TurnTo(btDir)`、`AttackDir`；不邻接 ⇒ 同图则 `SetTargetXy(目标格)`，异图则 `DelTargetCreat`。
pub fn attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let now = world.clock_ms;
    if let Some(bt_dir) = get_attack_dir(world, actor_id, target_id) {
        let due = world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
        if due {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.attack_tick = now;
                e.ai.target_focus_tick = now;
                e.ai.holy_seize = false;
            }
            turn_to(world, actor_id, bt_dir);
            // oracle `AnimalObject.Attack`：`GetBaseAttackPoewr()`（怪物 = `AttackPowerMode::Base`）
            // + `AttackDir(target, nPower, nDir)`。`World::attack_once` 的怪物路径逐取数同构
            // （`combat::attack_dir` 对非玩家攻方不加 `AttackSuccess`）。
            //
            // **W4 下行接线**：oracle `AnimalObject.Attack`（`AnimalObject.cs:165-170`）在
            // `AttackDir` 之后 `SendAttackMsg(RM_HIT, …)`；本次结算（`_Attack` 的伤害/命中）
            // 与玩家 `CM_HIT` 攻击走**同一条** `SM_STRUCK` 下行（受击者 `RM_REFMESSAGE` →
            // `SendRefMsg(RM_STRUCK)` → 视野内广播）。因此把结算记进 `pending_ai_attacks`
            // （宿主 `World::tick` 转抄进 `TickRecord::attacks`）⇒ 既有 `drain_combat_downlink`
            // 的 ① 段无需改动即出「怪打玩家」的 `SM_STRUCK` 帧。
            if let Some(outcome) = world.attack_once(actor_id, target_id) {
                world
                    .pending_ai_attacks
                    .push((actor_id, target_id, outcome));
            }
        }
        true
    } else {
        let (same_map, tx, ty) = match (
            world.entities.get(actor_id).map(|e| e.map_id),
            world.entities.get(target_id).map(|t| (t.map_id, t.x, t.y)),
        ) {
            (Some(am), Some((tm, tx, ty))) => (am == tm, tx, ty),
            _ => (false, 0, 0),
        };
        if same_map {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = tx;
                e.ai.target_y = ty;
            }
        } else {
            del_target_creature(world, actor_id);
        }
        false
    }
}

/// `AnimalObject.Struck(hiter)`（`AnimalObject.cs:334-360`）——怪物**被命中**时的索敌与受击后果。
///
/// oracle 调用点 = `Operate(RM_STRUCK)`（`AnimalObject.cs:118-124`）：
/// `SetLastHiter(struckObject); Struck(struckObject); BreakHolySeizeMode(); …`。
/// 本骨架由 [`crate::world::World::attack_once`] 把命中者按入队序压进目标的
/// [`crate::entity::AnimalState::pending_struck`]（= oracle `SendMsg` 的 `MsgQueue.Enqueue`），
/// 再由 [`base_object_run_monster`]（= 目标自己的 `base.Run()` 的消息段）出队调本函数
/// ⇒ `Struck` 的取数落在「怪自己的 `base.Run`」这个 site（与 oracle 同）。
/// `SetLastHiter`/`BreakHolySeizeMode` 不消费随机数，仍折在 `attack_once`（见该处注释）。
///
/// 取数（oracle 逐行，`AnimalObject.cs:334-360`）：
/// - 仅当 `TargetCret != null && !GetAttackDir(TargetCret, btDir)` 时抽 **1 次 `Random(6)`**
///   （短路：无目标或已邻接 ⇒ **不抽**）；
/// - 命中门通过后 `IsProperTarget(hiter)` 为真 ⇒ `SetTargetCreat(hiter)`；
/// - `if (Animal)` ⇒ 再抽 **1 次 `Random(300)`** 扣 `MeatQuality`（本骨架 `Animal` 恒 `false`，登记）；
/// - 尾：`AttackTick += 150 - MIN(130, Abil.Level * 4)`（不抽数）。
///
/// **前置**：调用方保证 `hiter_id` 在场（oracle `Operate` 的 `struckObject != null` 门在出队处）。
pub fn animal_struck(world: &mut World, victim_id: i32, hiter_id: i32) {
    let now = world.clock_ms;
    // `StruckTick = HUtil32.GetTickCount();`（`AnimalObject.cs:336`）
    if let Some(e) = world.entities.get_mut(victim_id) {
        e.ai.struck_tick = now;
    }
    let cur_target = world
        .entities
        .get(victim_id)
        .and_then(|e| e.ai.target_creature);
    // `if (TargetCret == null || GetAttackDir(TargetCret, ref btDir) || M2Share.RandomNumber.Random(6) == 0)`
    let take = match cur_target {
        None => true,
        Some(t) => get_attack_dir(world, victim_id, t).is_some() || world.rng.random_below(6) == 0,
    };
    if take {
        let proper = world
            .entities
            .get(victim_id)
            .zip(world.entities.get(hiter_id))
            .is_some_and(|(a, b)| is_proper_target(a, b));
        if proper {
            set_target_creature(world, victim_id, hiter_id);
        }
    }
    // `if (Animal) { MeatQuality = (ushort)(MeatQuality - Random(300)); if (MeatQuality < 0) MeatQuality = 0; }`
    // `Animal` 由怪种分类决定（[`is_animal_race`]）；`e.ai.animal` 保留为显式覆盖位（恒 `false`）。
    let is_animal = world
        .entities
        .get(victim_id)
        .is_some_and(|e| e.ai.animal || is_animal_race(e.race));
    if is_animal {
        let dec = world.rng.random_below(300);
        if let Some(e) = world.entities.get_mut(victim_id) {
            let v = i32::from(e.ai.meat_quality) - dec;
            e.ai.meat_quality = if v < 0 { 0 } else { v as u16 };
        }
    }
    // `AttackTick = AttackTick + (150 - HUtil32._MIN(130, Abil.Level * 4));`
    let level = world.entities.get(victim_id).map_or(0, |e| e.level);
    let add = 150 - (i32::from(level) * 4).min(130);
    if let Some(e) = world.entities.get_mut(victim_id) {
        e.ai.attack_tick += i64::from(add);
    }
}

/// `CreateMonster`（`WorldServer.MonGen.cs:507-728`）按 `monsters.Race` 选怪类；本函数给出
/// **`ATMonster` 家族中「未覆写 `Run`」** 的 race 集——这些类直接继承 `ATMonster.Run`
/// （`ATMonster.cs:13-24`），在 `base.Run()` 之前按时钟门调 `SearchTarget()`。
///
/// 由 `CreateMonster` 的 `race → 类` 映射 + 各派生类是否覆写 `Run` 推出：
/// - 本体：`53`（`AnimalWolf`=`new AtMonster`）、`81`/`86`/`88`/`89`（`new AtMonster`）；
/// - `83`（`new SlowAtMonster`，`SlowATMonster.cs:3` `: AtMonster` 无覆写）。
///
/// **不在此列（登记，本片不建模）**：覆写 `Run` 的 `ATMonster` 派生（`90`/`91`/`92`/`96`/`97`/
/// `100`/`105`/`107`/`114`/`130`/`132` 等，见模块头「未建模」），以及非 `ATMonster` 家族但自实现
/// 同样索敌门的类（`BoneKing`/`Clone`/`DigOutZombi`/`Sculture`/`LightingZombi`/…）。
///
/// 改坏必红：去掉本门 ⇒ `active_seek_race_*` 用例里 race 81/83 的怪不再发现玩家。
#[must_use]
pub fn is_active_seek_race(race: u8) -> bool {
    matches!(race, 53 | 81 | 83 | 86 | 88 | 89)
}

/// `AnimalObject.Struck` 的 `if (Animal)` 门（`AnimalObject.cs:350-357`）所读 `BaseObject.Animal`
/// 的**运行时等价物**——`Animal` 是怪种分类的函数（构造期置位，之后不再变）：
/// - `WorldServer.MonGen.cs:518-553` 的对象初始化式：`AnimalChicken(51)`/`AnimalDeer(52)`/`AnimalWolf(53)`；
/// - 派生类 ctor：`SpitSpider(82)`（`SpitSpider.cs:15`）/`Scorpion(84)`（`Scorpion.cs:7`）/
///   `StickMonster(85)`（`StickMonster.cs:23`）/`GasAttackMonster(90)`（`GasAttackMonster.cs:12`）；
/// - **继承**上述 ctor 且不再覆写 `Animal` 的派生：`105`/`106`（`: GasAttackMonster`）、
///   `114`/`118`/`119`（`: SpitSpider`）、`132`（`: StickMonster`）。
///   `CentipedeKingMonster(107)`（`: StickMonster`）ctor 显式 `Animal = false`（`CentipedeKingMonster.cs:17`）⇒ **排除**。
///
/// `BaseObject.ReAliveEx` 的 `case 54`（`BaseObject.cs:2601-2603`）只在**复活**路径置位；本骨架未建模
/// 复活 ⇒ `54` **不计入**（与 [`crate::combat::race_is_animal`] 口径一致）。这里在
/// [`crate::combat::race_is_animal`]（服务 `SKILL_TAMMING` 门）之上补 ctor 继承带来的 race。
///
/// 改坏必红：去门 ⇒ `animal_struck_no_random300_for_non_animal_race` 的对照与
/// `animal_struck_no_random300_for_race_51_vs_80` 必红。
#[must_use]
pub fn is_animal_race(race: u8) -> bool {
    crate::combat::race_is_animal(race) || matches!(race, 105 | 106 | 114 | 118 | 119 | 132)
}

/// 🛡️ `GuardList.txt` 放置的守卫 race 集（`CreateMonster` 的守卫族分支，`WorldServer.MonGen.cs:509-516/659-661`）：
/// `Supreguard(11)`→`SuperGuard`、`Petsupreguard(12)`→`PetSuperGuard`、`ArcherPolice(20)`、`ArcherGuard(112)`。
///
/// 这些类经 `GuardUnit : NormNpc : AnimalObject`（`GuardUnit.cs:14`、`NormNpc.cs:18`），**不继承
/// `MonsterObject.Run`** ⇒ 不走 `Think`/`AttackTarget`/`GotoTargetXY`/`Wondering`（= [`monster_run`]）。
/// `NormNpc.Run`（`NormNpc.cs:1604-1611`）只做「清 master + `base.Run()`」⇒ 守卫**静止**（不游走）；
/// 仍需走 [`base_object_run_monster`]（= `BaseObject.Run` 的恢复/出队/清理段）。
///
/// **登记（本片不建模）**：`SuperGuard.Run`（`SuperGuard.cs:68-115`）/`ArcherGuard.Run`
/// （`ArcherGuard.cs:39-...`）各自另有一套「扫 `VisibleActors` 找 pvp 玩家/怪并攻击」的主动逻辑；
/// 本片只做「不游走/不索敌」的静止等价（安静场景下与 oracle 一致）。
///
/// 改坏必红：去掉本门 ⇒ `guard_race_stays_static_while_normal_monster_wanders` 必红。
#[must_use]
pub fn is_guard_race(race: u8) -> bool {
    matches!(race, 11 | 12 | 20 | 112)
}

/// `MonsterObject.OpenCrazyMode(int nTime)`（`MonsterObject.cs:465-471`）——
/// `CrazyMode = true; CrazyModeTick = now; CrazyModeInterval = nTime * 1000; RefNameColor();`
/// （`RefNameColor` 是帧面，不在 world 层；`GetChrColor` 的 0xF9 支仍登记未实现）。
pub fn open_crazy_mode(world: &mut World, actor_id: i32, interval_seconds: i32) {
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.crazy_mode = true;
        e.ai.crazy_mode_tick = now;
        // C# `int * int` 在 int 上计算；正常取值（`Random(20)+10`）远不溢出。
        e.ai.crazy_mode_interval = i64::from(interval_seconds) * 1000;
    }
}

/// `MonsterObject.BreakCrazyMode()`（`MonsterObject.cs:473-480`）：**只有** `CrazyMode` 为真才清
/// （`RefNameColor` 同属帧面，world 层不产帧）。
pub fn break_crazy_mode(world: &mut World, actor_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        if e.ai.crazy_mode {
            e.ai.crazy_mode = false;
        }
    }
}

/// `AnimalObject.BreakHolySeizeMode()`（`AnimalObject.cs:330-334`）：置 `HolySeize = false`
/// （`RefNameColor` 帧面不在 world 层；与既有 `attack_target`/`tame_adopt` 的写法同口径）。
pub fn break_holy_seize_mode(world: &mut World, actor_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.holy_seize = false;
    }
}

/// `MonsterObject.Run` 尾的 `CrazyMode` 到期段（`MonsterObject.cs:353-356`）。
///
/// **严格大于**：`(now - CrazyModeTick) > CrazyModeInterval`（等于不清）。只在 oracle 落到
/// `if (CanMove() && ...)` **之后**的路径上调用（`Think()`/`AttackTarget()` 的提前返回
/// 会 `base.Run(); return;` ⇒ 跳过本段，见 [`monster_run`]）。
fn crazy_mode_expiry(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let expired = world.entities.get(actor_id).is_some_and(|e| {
        e.ai.crazy_mode && (now - e.ai.crazy_mode_tick) > e.ai.crazy_mode_interval
    });
    if expired {
        break_crazy_mode(world, actor_id);
    }
}

/// `AnimalObject.Run()`（`AnimalObject.cs:142-149`）的到期段：
/// `if (HolySeize && (now - HolySeizeTick) > HolySeizeInterval) BreakHolySeizeMode();` 再 `base.Run()`。
///
/// **严格大于**（等于不清）。oracle 里 `MonsterObject.Run` 的每个出口都调 `base.Run()`
/// （= `AnimalObject.Run`），因此 [`monster_ai_tick`] 对每只怪在 [`monster_run`] 之后、
/// [`base_object_run_monster`] 之前调用本函数（顺序 = `CrazyMode` 到期 → HolySeize 到期 → `BaseObject.Run`）。
/// `NormNpc.Run`（守卫等）同样 `base.Run()` 进 `AnimalObject.Run` ⇒ 守卫分支也调用本函数。
pub fn animal_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let expired = world.entities.get(actor_id).is_some_and(|e| {
        e.ai.holy_seize && (now - e.ai.holy_seize_tick) > e.ai.holy_seize_interval
    });
    if expired {
        break_holy_seize_mode(world, actor_id);
    }
}

/// `MonsterObject.Run`（`MonsterObject.cs:251-360`）的**普通怪路径**（无 master/任务/逃跑/狂暴）。
///
/// 只做 AI；`BaseObject.Run` 的恢复/死亡/过期段由 `World` 统一处理（见模块头）。
pub fn monster_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    // `ATMonster.Run`（`ATMonster.cs:13-24`）：`is_active_seek_race` 的怪在 `base.Run()`
    // （= `MonsterObject.Run`，本函数余下部分）之前先按时钟门索敌。外层门是 `CanMove()`
    // （`AnimalObject.cs:162-165`，= [`can_move`]）；门未到 ⇒ 不搜不抽。
    // `SearchTarget` 本身**不消费随机数**（见其文档）⇒ 只改目标态、不移位 RNG 流水。
    if is_active_seek_race(e.race) && can_move(e) {
        let now = world.clock_ms;
        let enemy_tick = e.ai.search_enemy_tick;
        let has_target = e.ai.target_creature.is_some();
        // `(now - SearchEnemyTick) > 8000 || (now - SearchEnemyTick) > 1000 && TargetCret == null`
        // ——C# 的 `&&` 优先级高于 `||`，故第二项是 `(>1000) && (TargetCret == null)`。
        if now - enemy_tick > 8000 || (now - enemy_tick > 1000 && !has_target) {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.search_enemy_tick = now;
            }
            search_target(world, actor_id);
        }
    }
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !(can_move(e) && !e.fixed_hide_mode && !e.stone_mode) {
        // oracle：外层 `if (CanMove() && ...)` 为假 ⇒ 落到函数尾的 `CheckRoyalty()`
        // （`MonsterObject.cs:352`）与 `CrazyMode` 到期（`:353-356`）。
        crate::slave::check_royalty(world, actor_id);
        crazy_mode_expiry(world, actor_id);
        return;
    }
    if think(world, actor_id) {
        // oracle `:255-258`：`if (Think()) { base.Run(); return; }` ⇒ **跳过** `CheckRoyalty`。
        return;
    }
    let now = world.clock_ms;
    // `if (WalkWaitLocked) { if (now - WalkWaitTick) > WalkWait) WalkWaitLocked = false; }`
    if world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.ai.walk_wait_locked)
        && world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.walk_wait_tick > e.ai.walk_wait)
    {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.walk_wait_locked = false;
        }
    }
    let walk_due = world
        .entities
        .get(actor_id)
        .is_some_and(|e| !e.ai.walk_wait_locked && (now - e.ai.walk_tick) > e.ai.walk_speed);
    if !walk_due {
        // oracle：`if (!WalkWaitLocked && (now - WalkTick) > WalkSpeed)` 为假 ⇒ 落到
        // `CheckRoyalty()` 与 `CrazyMode` 到期（`MonsterObject.cs:352-356`）。
        crate::slave::check_royalty(world, actor_id);
        crazy_mode_expiry(world, actor_id);
        return;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.walk_tick = now;
        e.ai.walk_count += 1;
        if e.ai.walk_count > e.ai.walk_step {
            e.ai.walk_count = 0;
            e.ai.walk_wait_locked = true;
            e.ai.walk_wait_tick = now;
        }
    }
    if !world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.ai.run_away_mode)
    {
        if !world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ai.no_attack_mode)
        {
            let target = world
                .entities
                .get(actor_id)
                .and_then(|e| e.ai.target_creature);
            match target {
                Some(_) => {
                    if attack_target(world, actor_id) {
                        return;
                    }
                }
                None => {
                    if let Some(e) = world.entities.get_mut(actor_id) {
                        // `MonsterObject.cs:296-301`：无目标且 `Mission == false` ⇒ `TargetX = -1`。
                        e.ai.target_x = -1;
                    }
                }
            }
        }
    } else {
        let expired = world.entities.get(actor_id).is_some_and(|e| {
            e.ai.run_away_time > 0 && (now - e.ai.run_away_start) > e.ai.run_away_time
        });
        if expired {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.run_away_mode = false;
                e.ai.run_away_time = 0;
            }
        }
    }
    // `Master` 跟随/`SlaveRelax` 分支未建模（普通怪 master == null ⇒ 恒不命中）。
    if world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.ai.target_x != -1)
    {
        goto_target_xy(world, actor_id);
    } else {
        let has_target = world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ai.target_creature.is_some());
        if !has_target {
            wondering(world, actor_id);
        }
    }
    // `MonsterObject.Run` 尾的 `CheckRoyalty()`（`MonsterObject.cs:352`）。
    // 注：oracle 的 `AttackTarget()` 命中支（`:277-283`）与 `Master.SlaveRelax` 支（`:333-338`）
    // 会 `base.Run(); return;` 跳过本调用；本骨架未建模 `Master.SlaveRelax` 分支
    // （见模块头「未建模」），攻击支的提前返回保持跳过（与 oracle 同）。
    crate::slave::check_royalty(world, actor_id);
    // `MonsterObject.cs:353-356`：CrazyMode 到期（在 `CheckRoyalty()` 之后、`base.Run()` 之前）。
    crazy_mode_expiry(world, actor_id);
}

/// `BaseObject.Run()`（`BaseObject.Base.cs:24-316`）的**怪侧公共段**：恢复 / 体力点 / `TargetCret` 清理。
///
/// oracle 里这三段在 `monster.Run()` 的**末尾**执行：`MonsterObject.Run` 的两个出口
/// （`Think()` 命中 `MonsterObject.cs:255-258` 与普通出口 `:359`）都调 `base.Run()`；
/// 因此 [`monster_ai_tick`] 对每只怪先跑 [`monster_run`]（AI）再跑本函数，与 oracle 同序。
///
/// 覆盖的段（行号 = `src/M2Server/Actor/BaseObject.Base.cs`）：
/// - 恢复段 `:39-66`：`(now - AutoRecoveryTick) / 20` 折成 `HealthTick/SpellTick`；
///   `HP < MaxHP` 且 `HealthTick >= HealthFillTime` ⇒ `HP += MaxHP / 75 + 1`（截断到 `MaxHP`）；
///   `MP < MaxMP` 且 `SpellTick >= SpellFillTime` ⇒ `MP += MaxMP / 18 + 1`；到点后两个 tick 归零。
///   怪物的 `HealthSpellChanged()` 只对 `Race == ActorRace.Play` 发帧（`BaseObject.cs:700-703`）
///   ⇒ **怪侧无帧面**（不产生任何 `SM_*`）。
/// - 体力点段 `:105-110`：`HealthTick < -HealthFillTime && HP > 1` ⇒ `HP -= 1; HealthTick += HealthFillTime`。
///   普通怪的 `HealthTick` 只增不减⇒该支对怪物不可达；照抄是为了同一份 `BaseObject.Run` 语义
///   （将来玩家/消费点接进来时不需要再改）。
/// - `TargetCret` 清理段 `:111-118`：`now - TargetFocusTick > 30000 || TargetCret.Death ||
///   TargetCret.Ghost || 异图 || |Δx| > 15 || |Δy| > 15` ⇒ `ClearTargetCreat`。
///
/// **本函数不消费随机数**（恢复/体力点/过期都是纯状态演进）。
///
/// **未建模（登记，见模块头/证据）**：`SuperMan`（`:35-38`）、`HP == 0 ⇒ Die()`（`:70-73`，
/// 由 [`crate::world::World`] 的 `settle_deaths` 在同一 tick 的死亡结算里等价处理；本段的恢复
/// 只增 HP、体力点段有 `HP > 1` 门⇒此处 `HP == 0` 不可达）、`MakeGhost`（`:86-101`，需
/// `MonGen.ZenTime`/`Config.MakeGhostTime`）、`Master` 跟随/`SlaveRelax`（`:133-215`）、
/// `ShowHp`/`VerifyTick`（`:217-256`）。**⑨-3 已接**：宝宝列表清表（`:208-217`）。
///
/// 锚点复核于 2026-10-13。
/// 改坏必红：把 `HealthFillTime` 门（或 `HP > 1` 门）拆掉⇒`base_recovery_*`/
/// `health_tick_negative_cost_is_clamped` 必红（见 `tests/parity/world/monster-ai-host-20261013`）。
pub fn base_object_run_monster(world: &mut World, actor_id: i32) {
    // `BaseObject.Run` 的**消息段**（`BaseObject.Base.cs:26-30`）：先把入队的 `RM_STRUCK` 出队，
    // 逐个 `Operate(RM_STRUCK) → Struck`（`AnimalObject.cs:118-124`）。本骨架用
    // [`crate::entity::AnimalState::pending_struck`] 承载队列（入队处 = `World::attack_once`）。
    let pending: Vec<i32> = match world.entities.get_mut(actor_id) {
        Some(e) => std::mem::take(&mut e.ai.pending_struck),
        None => return,
    };
    for hiter in pending {
        // oracle `if (processMsg.ActorId == ActorId && struckObject != null)`：命中者已不在场 ⇒ 跳过。
        if world.entities.get(hiter).is_some() {
            animal_struck(world, actor_id, hiter);
        }
    }
    let now = world.clock_ms;
    let state = world.entities.get(actor_id).map(|e| {
        (
            e.death,
            e.can_re_alive,
            e.mon_gen.is_some(),
            e.mon_gen_zen_time,
            e.death_tick,
        )
    });
    let Some((is_death, can_re_alive, has_mon_gen, zen_time, death_tick)) = state else {
        return;
    };
    if is_death {
        // 死亡分支（`BaseObject.Base.cs:85-101`）：
        //
        //   if (CanReAlive && MonGen != null) {
        //       int makeGhostTime = MAX(10s, GetMonstersZenTime(MonGen.ZenTime) - 20s);
        //       if (makeGhostTime > Config.MakeGhostTime) makeGhostTime = Config.MakeGhostTime;
        //       if (GetTickCount() - DeathTick > makeGhostTime) MakeGhost();
        //   } else {
        //       if ((GetTickCount() - DeathTick) > Config.MakeGhostTime) MakeGhost();
        //   }
        //
        // 两条门都是**严格 >**。`GetMonstersZenTime` 读全服 `PlayObjectCount`
        // （`WorldServer.MonGen.cs:168-190`）⇒ 这里数 `race == Play` 的实体（宿主同口径）。
        let play_object_count = world
            .entities
            .iter()
            .filter(|x| x.race == ACTOR_RACE_PLAY)
            .count() as i32;
        // `get_monsters_zen_time` 只读这两项；其余字段用 oracle 构造缺省（不参与计算）。
        let cfg = crate::mongen::MonGenConfig {
            user_full: world.mon_gen_user_full,
            zen_fast_step: world.mon_gen_zen_fast_step,
            ..crate::mongen::MonGenConfig::default()
        };
        let gate = crate::mongen::monster_death_gate(
            can_re_alive,
            has_mon_gen,
            zen_time,
            world.make_ghost_time,
            play_object_count,
            &cfg,
        );
        let now_i32 = i32::try_from(now).unwrap_or(i32::MAX);
        let death_i32 = i32::try_from(death_tick).unwrap_or(i32::MAX);
        if crate::mongen::monster_due_for_ghost(death_i32, gate, now_i32) {
            make_ghost(world, actor_id);
        }
        // 死亡实体不再回血（oracle 同行）。oracle 的 VerifyTick 段在 `if (!Death)` **之外** ⇒
        // 死者同样刷新格子时间（`BaseObject.Base.cs:220-228`）。
        world.verify_map_time(actor_id);
        return;
    }
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let health_fill_time = world.health_fill_time;
    let spell_fill_time = world.spell_fill_time;
    let recovery_tick = ((now - e.auto_recovery_tick) / 20) as i32;
    let mut health_tick = e.health_tick + recovery_tick;
    let mut spell_tick = e.spell_tick + recovery_tick;
    let mut hp = e.hp;
    let mut mp = e.mp;
    let (max_hp, max_mp) = (e.max_hp, e.max_mp);
    // `BaseObject.Base.cs:47-55`：`WAbil.MaxHP / 75 + 1`，`(HP + n) < MaxHP` 否则截断到 `MaxHP`。
    // 加/比在 `int` 上进行（oracle 的 `ushort + ushort` 被提升为 `int`，不会在 65536 处回绕）；
    // 落回 `u16` 时 `sum < max_hp <= 65535`，强转安全。
    if hp < max_hp && health_tick >= health_fill_time {
        let n = i32::from(max_hp) / 75 + 1;
        let sum = i32::from(hp) + n;
        hp = if sum < i32::from(max_hp) {
            sum as u16
        } else {
            max_hp
        };
    }
    // `BaseObject.Base.cs:57-65`：`WAbil.MaxMP / 18 + 1`（同 HP 支在 `int` 上计算）。
    if mp < max_mp && spell_tick >= spell_fill_time {
        let n = i32::from(max_mp) / 18 + 1;
        let sum = i32::from(mp) + n;
        mp = if sum < i32::from(max_mp) {
            sum as u16
        } else {
            max_mp
        };
    }
    // `BaseObject.Base.cs:66-69` 的归零两行（照 oracle 次序：先判恢复、再归零）。
    if health_tick >= health_fill_time {
        health_tick = 0;
    }
    if spell_tick >= spell_fill_time {
        spell_tick = 0;
    }
    // `BaseObject.Base.cs:105-110` 体力点段。
    if health_tick < -health_fill_time && hp > 1 {
        hp -= 1;
        health_tick += health_fill_time;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.health_tick = health_tick;
        e.spell_tick = spell_tick;
        e.hp = hp;
        e.mp = mp;
        e.auto_recovery_tick = now;
    }
    target_cret_cleanup(world, actor_id, now);
    // `BaseObject.Base.cs:223-228`：`(now - VerifyTick) > 30s` ⇒ `Envir.VerifyMapTime(...)`
    // 刷新本格 `CellObject.AddTime`（静止怪/卫士/NPC 不被 60s 过期逐出）。不抽随机数。
    world.verify_map_time(actor_id);
}

/// `BaseObject.Run()` 的 `TargetCret` 清理段（`BaseObject.Base.cs:111-118`）。
fn target_cret_cleanup(world: &mut World, actor_id: i32, now: i64) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let Some(target_id) = e.ai.target_creature else {
        return;
    };
    let (map_id, x, y, focus) = (e.map_id, e.x, e.y, e.ai.target_focus_tick);
    let invalid = match world.entities.get(target_id) {
        None => true,
        Some(t) => {
            (now - focus) > 30_000
                || t.death
                || t.ghost
                || t.map_id != map_id
                || (t.x - x).abs() > 15
                || (t.y - y).abs() > 15
        }
    };
    if invalid {
        clear_target_creature(world, actor_id, target_id);
    }
}

/// `BaseObject.MakeGhost()`（`BaseObject.Base.cs:484-496`）：
///
/// ```csharp
/// if (CanReAlive) { Invisible = true; } else { Ghost = true; }
/// GhostTick = HUtil32.GetTickCount();
/// DisappearA();
/// ```
///
/// `CanReAlive` 的怪**保留尸体**（`Invisible`，可被 `ReAliveEx` 复活）；其余置 `Ghost`
/// （5 分钟后由 [`crate::mongen::process_cert_lists`] 释放）。
/// `DisappearA()` 的格子面 = [`World::disappear_a`]；`RM_DISAPPEAR` 帧由 AOI 视野差集表现。
///
/// **oracle 怪癖照抄**：死亡支每跑一次 `base.Run` 且门仍成立就再调一次 `MakeGhost`
/// （`Invisible` 尸体仍满足 `now - DeathTick > makeGhostTime`）⇒ `GhostTick` 被反复刷新、
/// `DisappearA` 反复执行。本函数同样重复执行（幂等：已摘格时 `disappear_a` 返回 false）。
fn make_ghost(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        if e.can_re_alive {
            // `Invisible = true;`——尸体保留、仍可复活。
            e.invisible = true;
        } else {
            // `Ghost = true;`
            e.ghost = true;
        }
        e.ghost_tick = now;
    }
    world.disappear_a(actor_id);
}

/// 按**实体槽位顺序**对场上所有怪物跑一次 [`monster_run`]（`ProcessMonsters` 的单线程等价物）。
///
/// 顺序契约：来自 `EntityStore` 的显式槽位索引（`iter`），不迭代 Query。
/// 每只怪再跑一次 [`base_object_run_monster`]（`MonsterObject.Run` 末尾的 `base.Run()`）。
pub fn monster_ai_tick(world: &mut World) {
    let ids: Vec<i32> = world
        .entities
        .iter()
        // `race > Play` 会把交易商人也命中（`ActorRace.Merchant == Animal == 50`）⇒
        // 显式排除 `merchant.is_some()` 的实体（M3⑤-实现 2：商人由自己的 tick 驱动）。
        // 固定管理 NPC（`Envir/Npcs.txt`，race = `ActorRace.NPC`）同样排除：oracle 由
        // `ProcessNpcs`（`WorldServer.Merchant.cs:118`）驱动，本仓已由 `World::npc_tick`
        // 建模（同一次序）；此处必须不跑怪物 AI（否则会用与 oracle 不同的取数路径污染
        // RNG 流，且与 `npc_tick` 双驱固定 NPC）。
        .filter(|e| {
            e.race > ACTOR_RACE_PLAY
                && e.merchant.is_none()
                && e.npc.is_none()
                // `Ghost` 怪在 oracle `ProcessMonsters` 里整只跳过（`:320-333` 只做 5min 清理，
                // 由 [`crate::mongen::process_cert_lists`] 承担）⇒ 本 tick 不再驱动。
                // **死亡但未 Ghost** 的怪仍要跑 `base.Run`（死亡支 `MakeGhost`，`:85-101`）⇒ 不过滤。
                && !e.ghost
        })
        .map(|e| e.id)
        .collect();
    for id in ids {
        // `CanMove()`（`AnimalObject.cs:162-165`）对 `Death` 恒 false ⇒ `MonsterObject.Run`
        // 的 AI 体（Think/Walk/Attack）整段跳过，只剩 `base.Run()` 的死亡支与 VerifyTick 段。
        if world.entities.get(id).is_some_and(|e| e.death) {
            base_object_run_monster(world, id);
            continue;
        }
        // `GuardList` 放置的守卫（race 11/12/20/112）不继承 `MonsterObject.Run`
        // （`GuardUnit:NormNpc`，`NormNpc.Run` 只是「清 master + `base.Run()`」）⇒ **不游走/不索敌**，
        // 但仍要跑自己的 `base.Run()` 段（`BaseObject.Run` 的恢复/`RM_STRUCK` 出队/`TargetCret` 清理）。
        if is_guard_race(world.entities.get(id).map_or(0, |e| e.race)) {
            // `NormNpc.Run`（`NormNpc.cs:1604-1611`）→ `base.Run()` = `AnimalObject.Run`
            // （HolySeize 到期）→ `BaseObject.Run`。
            animal_run(world, id);
            base_object_run_monster(world, id);
            continue;
        }
        monster_run(world, id);
        // `MonsterObject.Run` 的末尾 `base.Run()`（`MonsterObject.cs:255/359`）：
        // `AnimalObject.Run`（HolySeize 到期）→ `BaseObject.Run`。
        animal_run(world, id);
        base_object_run_monster(world, id);
    }
}

/// 便于测试/接线：当前朝向下的前一格（oracle `GetFrontPosition`，越界 clamp）。
#[must_use]
pub fn front_of(world: &World, actor_id: i32, dir: u8, map_id: usize) -> Option<(i32, i32)> {
    let e = world.entities.get(actor_id)?;
    let map = world.maps.get(map_id)?;
    Some(front_position(e.x, e.y, dir, map.width, map.height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{VisibleBaseObject, VisibleFlag, ACTOR_RACE_MONSTER};
    use crate::world::World;
    use mir2_shared::rng::RandomNumber;

    fn monster(world: &mut World, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "稻草人", x, y);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 60;
        e.max_hp = 60;
        world.add_entity(e)
    }

    fn player(world: &mut World, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "m2r", x, y);
        e.hp = 100;
        e.max_hp = 100;
        world.add_entity(e)
    }

    /// 找一个「首抽 `Random(20)` 为 `want_zero`、且（可选）次抽 `Random(4)` 为 `want4`」的种子。
    fn seed_with_gate(want_zero: bool, want4: Option<i32>) -> i32 {
        for seed in 0..100_000 {
            let mut r = RandomNumber::with_seed(seed);
            let first = r.random_below(20);
            if (first == 0) != want_zero {
                continue;
            }
            if let Some(w4) = want4 {
                if r.random_below(4) != w4 {
                    continue;
                }
            }
            return seed;
        }
        unreachable!("应能在 10 万种子里找到目标门取值");
    }

    /// oracle `Wondering`：`Random(20) != 0` ⇒ **只抽这一次**、不动不转（`AnimalObject.cs:425-428`）。
    #[test]
    fn wondering_gate_nonzero_consumes_exactly_one_draw_no_move() {
        let seed = seed_with_gate(false, None);
        let mut world = World::new(30, 30);
        let id = monster(&mut world, 10, 10);
        world.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        let first = probe.random_below(20);
        assert_ne!(first, 0);
        let probe_next = probe.random();
        let before = {
            let e = world.entities.get(id).unwrap();
            (e.x, e.y, e.ai.dir)
        };
        wondering(&mut world, id);
        let after = {
            let e = world.entities.get(id).unwrap();
            (e.x, e.y, e.ai.dir)
        };
        assert_eq!(before, after, "非 0 支不应移动/转向");
        assert_eq!(
            world.rng.random(),
            probe_next,
            "非 0 支必须只消耗一次 Random(20)"
        );
    }

    /// oracle `Wondering`：`Random(20)==0 && Random(4)==1` ⇒ `RandomByte(8)` 后 `TurnTo`
    /// （`AnimalObject.cs:430-435`）。取数次序：Random(20) → Random(4) → RandomByte(8)。
    #[test]
    fn wondering_turn_branch_matches_oracle_draw_order() {
        let seed = seed_with_gate(true, Some(1));
        let mut world = World::new(30, 30);
        let id = monster(&mut world, 10, 10);
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        assert_eq!(expect.random_below(20), 0);
        assert_eq!(expect.random_below(4), 1);
        let dir = expect.random_byte(8);
        let (x, y) = {
            let e = world.entities.get(id).unwrap();
            (e.x, e.y)
        };
        wondering(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.ai.dir, dir, "应 TurnTo(RandomByte(8))");
        assert_eq!((e.x, e.y), (x, y), "TurnTo 分支不移动");
        assert_eq!(
            world.rng.random(),
            expect.random(),
            "取数次序须为 Random(20)→Random(4)→RandomByte(8)"
        );
    }

    /// oracle `Wondering`：`Random(20)==0 && Random(4)!=1` ⇒ `WalkTo(Dir)`（`AnimalObject.cs:437`）。
    #[test]
    fn wondering_walk_branch_moves_and_keeps_dir() {
        let seed = seed_with_gate(true, Some(0));
        let mut world = World::new(30, 30);
        let id = monster(&mut world, 10, 10);
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        assert_eq!(expect.random_below(20), 0);
        assert_eq!(expect.random_below(4), 0);
        let before_dir = world.entities.get(id).unwrap().ai.dir;
        wondering(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.ai.dir, before_dir, "WalkTo(Dir) 不转向");
        // dir=0（Up）⇒ y 减一（地图空旷）。
        assert_eq!((e.x, e.y), (10, 9), "WalkTo(Up) 应向北走一格");
        assert_eq!(
            world.rng.random(),
            expect.random(),
            "WalkTo 分支不再抽随机数"
        );
    }

    /// `SearchTarget`：可见集里按曼哈顿距离选最近、且被 `IsProperTarget` 承认的目标；
    /// 死亡/超距者从 `VisibleActors` 清除（`AnimalObject.cs:388-423`）。
    #[test]
    fn search_target_picks_nearest_and_clears_invalid() {
        let mut world = World::new(60, 60);
        let mid = monster(&mut world, 10, 10);
        let near = player(&mut world, 12, 10);
        let far = player(&mut world, 20, 10);
        let dead = player(&mut world, 11, 10);
        world.entities.get_mut(dead).unwrap().death = true;
        {
            let e = world.entities.get_mut(mid).unwrap();
            e.visible_actors = vec![
                VisibleBaseObject {
                    target: far,
                    flag: VisibleFlag::Show,
                },
                VisibleBaseObject {
                    target: dead,
                    flag: VisibleFlag::Show,
                },
                VisibleBaseObject {
                    target: far,
                    flag: VisibleFlag::Show,
                },
                VisibleBaseObject {
                    target: near,
                    flag: VisibleFlag::Show,
                },
            ];
        }
        search_target(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(near),
            "应选曼哈顿距离最近的合法目标"
        );
        let vis = world.entities.get(mid).unwrap().visible_actors.clone();
        assert!(vis.iter().all(|v| v.target != dead), "死亡目标应被清除");
        assert_eq!(
            vis.iter().filter(|v| v.target == far).count(),
            2,
            "重复可见项保留"
        );
    }

    /// **reviewer-6 的 Important 收口（`AnimalObject.cs:402`）**：`SearchTarget` 的隐藏门读的是
    /// `HideMode`，**不是** `FixedHideMode`。
    ///
    /// - `hide_mode=true && cool_eye=false` ⇒ 不得被选为目标（oracle 的 `(!HideMode || CoolEye)` 不成立）；
    /// - `hide_mode=false && fixed_hide_mode=true` ⇒ **可以**被选为（oracle 该函数不看 `FixedHideMode`）；
    /// - `hide_mode=true && cool_eye=true` ⇒ 可以被选（`CoolEye` 破隐身）。
    ///
    /// 红检：把门改回 `fixed_hide_mode` ⇒ 第 1/3 条断言必红。
    #[test]
    fn search_target_hide_gate_reads_hide_mode_not_fixed_hide_mode() {
        fn run(hide_mode: bool, fixed: bool, cool_eye: bool) -> (i32, Option<i32>) {
            let mut world = World::new(60, 60);
            let mid = monster(&mut world, 10, 10);
            world.entities.get_mut(mid).unwrap().ai.cool_eye = cool_eye;
            let target = player(&mut world, 12, 10);
            {
                let t = world.entities.get_mut(target).unwrap();
                t.hide_mode = hide_mode;
                t.fixed_hide_mode = fixed;
            }
            world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
                target,
                flag: VisibleFlag::Show,
            }];
            search_target(&mut world, mid);
            (target, world.entities.get(mid).unwrap().ai.target_creature)
        }
        assert_eq!(run(true, true, false).1, None, "HideMode 挡住：不得选中");
        let (target, picked) = run(false, true, false);
        assert_eq!(
            picked,
            Some(target),
            "FixedHideMode 不属该门 ⇒ 照选（与 oracle 1:1）"
        );
        let (target, picked) = run(true, true, true);
        assert_eq!(picked, Some(target), "CoolEye 破 HideMode");
    }

    /// `GotoTargetXY`：进函数先 `WalkTo(主方向)`，**无条件**抽一次 `Random(3)`；
    /// 移动成功 ⇒ 不再走重试循环（`AnimalObject.cs:174-249`）。
    #[test]
    fn goto_target_xy_walks_and_consumes_one_random3() {
        let mut world = World::new(30, 30);
        let id = monster(&mut world, 10, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.target_x = 10;
            e.ai.target_y = 15; // 正下方 ⇒ 主方向 Down(4)
        }
        let mut expect = RandomNumber::with_seed(1234);
        world.rng = RandomNumber::with_seed(1234);
        let _ = expect.random_below(3);
        goto_target_xy(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!((e.x, e.y), (10, 11), "应由 Down 走到 (10,11)");
        assert_eq!(e.ai.dir, DIR_DOWN);
        // 移动成功后循环体不再抽（oracle 的 Random(3) 只在进函数时抽一次）。
        assert_eq!(world.rng.random(), expect.random());
    }

    /// `Think`：3s 门未到 ⇒ 不抽不判；到点且同格对象 ≥2 ⇒ `DupMode=true`，
    /// 且同一次调用里的 `DupMode` 支 `WalkTo(RandomByte(8))` 被边界拦住（不移动）⇒ 返回 false。
    #[test]
    fn think_sets_dup_mode_when_cell_has_two_objects() {
        // 1×1 地图：四周越界 ⇒ `WalkTo` 不可能成功，`DupMode` 保持。
        let mut world = World::new(1, 1);
        let id = monster(&mut world, 0, 0);
        let _other = player(&mut world, 0, 0);
        assert!(world.enter_world(id));
        assert!(world.enter_world(_other));
        world.entities.get_mut(id).unwrap().ai.think_tick = 0;
        world.clock_ms = 5000;
        assert!(
            !think(&mut world, id),
            "边界拦住的 DupMode 移动不应返回 true"
        );
        assert!(world.entities.get(id).unwrap().ai.dup_mode);
    }

    /// 顺序判据：`monster_ai_tick` 按实体槽位顺序处理，同种子两次世界状态逐字节一致。
    #[test]
    fn monster_ai_tick_is_slot_ordered_and_deterministic() {
        fn run() -> Vec<(i32, i32, i32, u8)> {
            let mut world = World::new(30, 30);
            let _p = player(&mut world, 5, 5);
            for (i, (x, y)) in [(10, 10), (12, 12), (20, 20)].into_iter().enumerate() {
                let mut e = Entity::new(0, format!("m{i}"), x, y);
                e.race = ACTOR_RACE_MONSTER;
                e.hp = 60;
                e.max_hp = 60;
                world.add_entity(e);
            }
            world.rng = RandomNumber::with_seed(99);
            for _ in 0..20 {
                world.clock_ms += 200;
                monster_ai_tick(&mut world);
            }
            let mut out: Vec<(i32, i32, i32, u8)> = world
                .entities
                .iter()
                .map(|e| (e.id, e.x, e.y, e.ai.dir))
                .collect();
            out.sort_unstable();
            out
        }
        assert_eq!(run(), run(), "同种子两次运行必须逐字节一致");
    }

    /// **缺省关闭**：`World::tick` 在未 `enable_monster_ai()` 时不得驱动 AI、不得消耗额外随机数；
    /// 显式开启后必须推进随机流（gate 是 load-bearing，也是既有 golden 不被移位的前提）。
    #[test]
    fn monster_ai_is_opt_in_and_off_by_default() {
        fn run(enable: bool) -> (Vec<(i32, i32, u8)>, i32) {
            let mut world = World::new(30, 30);
            let _id = monster(&mut world, 10, 10);
            world.rng = RandomNumber::with_seed(99);
            if enable {
                world.enable_monster_ai();
            }
            assert_eq!(world.monster_ai_enabled(), enable);
            for _ in 0..30 {
                world.tick();
            }
            let state = world
                .entities
                .iter()
                .map(|e| (e.x, e.y, e.ai.dir))
                .collect();
            (state, world.rng.random())
        }
        let (off_state, off_rng) = run(false);
        let (on_state, on_rng) = run(true);
        assert_eq!(off_state, vec![(10, 10, 0)], "默认关 ⇒ 怪物不被 AI 移动");
        assert_ne!(
            on_rng, off_rng,
            "开启 AI 必须消耗/推进随机流（否则 gate 形同虚设）"
        );
        let _ = on_state;
    }

    /// `BaseObject.Run` 恢复段（`BaseObject.Base.cs:39-66`）：`HealthTick` 到 `HealthFillTime`
    /// ⇒ `HP += MaxHP / 75 + 1`（截断到 `MaxHP`）并把 `HealthTick` 归零。
    /// 改坏必红：去掉 `health_tick >= health_fill_time` 门（无条件回血）⇒ 本用例第一个断言红。
    #[test]
    fn base_recovery_heals_hp_at_fill_time_and_resets_tick() {
        let mut world = World::new(32, 32);
        world.set_recovery_fill_times(300, 800);
        let mut e = Entity::new(0, "稻草人", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 10;
        e.max_hp = 75;
        e.health_tick = 0;
        e.auto_recovery_tick = 0;
        let id = world.add_entity(e);
        // 未到点：`(5980 - 0) / 20 = 299` ⇒ `HealthTick = 299 < 300` ⇒ 不回血。
        world.clock_ms = 5_980;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.hp, 10, "未到 HealthFillTime 不得回血");
        assert_eq!(e.health_tick, 299, "HealthTick 仍按时间累加");
        // 到点：`(6000 - 5980) / 20 = 1` ⇒ `HealthTick = 300 >= 300` ⇒ `HP += 75/75+1 = 2` 后归零。
        world.clock_ms = 6_000;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.hp, 12, "回血量 = MaxHP/75 + 1");
        assert_eq!(e.health_tick, 0, "到点后 HealthTick 归零");
    }

    /// 恢复段截断：`(HP + n) < MaxHP` 不成立 ⇒ 跳到 `MaxHP`（`BaseObject.Base.cs:51-54`）。
    #[test]
    fn base_recovery_caps_at_max_hp() {
        let mut world = World::new(32, 32);
        world.set_recovery_fill_times(300, 800);
        let mut e = Entity::new(0, "稻草人", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 74;
        e.max_hp = 75;
        e.health_tick = 300;
        e.auto_recovery_tick = 0;
        let id = world.add_entity(e);
        base_object_run_monster(&mut world, id);
        assert_eq!(world.entities.get(id).unwrap().hp, 75, "HP 截断到 MaxHP");
    }

    /// 恢复段在 `u16` 边界上的加法必须按 oracle 的 `int` 语义（`BaseObject.Base.cs:47-55/57-65`：
    /// C# 的 `ushort + ushort` 提升为 `int` ⇒ 不会在 65536 处回绕/panic）。HP/MP 两支都盖。
    /// 改坏必红：把本段改回 `hp + n`（u16 加）⇒ 本用例 debug `attempt to add with overflow`
    /// panic / release 回绕为 0。
    #[test]
    fn base_recovery_high_max_hp_does_not_overflow_u16() {
        let mut world = World::new(32, 32);
        world.set_recovery_fill_times(300, 800);
        let mut e = Entity::new(0, "魔龙教主", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.max_hp = 65_535;
        // n = 65535/75 + 1 = 874 ⇒ hp + n = 66374 > u16::MAX（旧 u16 写法在此回绕/panic）。
        e.hp = 65_500;
        e.health_tick = 300;
        e.auto_recovery_tick = 0;
        let id = world.add_entity(e);
        base_object_run_monster(&mut world, id);
        assert_eq!(
            world.entities.get(id).unwrap().hp,
            65_535,
            "溢出带内应截断到 MaxHP 而不是回绕为 0"
        );
        // MP 支同理：n = 65535/18 + 1 = 3641 ⇒ mp + n = 69141 > u16::MAX。
        let mut e = Entity::new(1, "魔龙教主", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.max_mp = 65_535;
        e.mp = 65_500;
        e.spell_tick = 800;
        e.auto_recovery_tick = 0;
        let id2 = world.add_entity(e);
        base_object_run_monster(&mut world, id2);
        assert_eq!(
            world.entities.get(id2).unwrap().mp,
            65_535,
            "MP 溢出带内应截断到 MaxMP"
        );
    }

    /// 恢复段 MP 支（`BaseObject.Base.cs:57-65`）：`MaxMP/18 + 1`。
    #[test]
    fn base_recovery_heals_mp_at_spell_fill_time() {
        let mut world = World::new(32, 32);
        world.set_recovery_fill_times(300, 800);
        let mut e = Entity::new(0, "稻草人", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 75;
        e.max_hp = 75;
        e.mp = 0;
        e.max_mp = 18;
        e.spell_tick = 795;
        e.auto_recovery_tick = 0;
        let id = world.add_entity(e);
        world.clock_ms = 100; // recovery = 5 ⇒ SpellTick = 800 >= 800
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.mp, 2, "回蓝量 = MaxMP/18 + 1");
        assert_eq!(e.spell_tick, 0, "到点后 SpellTick 归零");
    }

    /// 体力点段（`BaseObject.Base.cs:105-110`）：`HealthTick < -HealthFillTime && HP > 1`
    /// ⇒ `HP -= 1` 且 `HealthTick += HealthFillTime`；`HP == 1` 时被 `HP > 1` 门挡住。
    /// 改坏必红：拆掉 `hp > 1` 门 ⇒ 第二个断言红。
    #[test]
    fn health_tick_negative_costs_one_hp_and_is_gated() {
        let mut world = World::new(32, 32);
        world.set_recovery_fill_times(300, 800);
        let mut e = Entity::new(0, "稻草人", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 5;
        e.max_hp = 60;
        e.health_tick = -301;
        let id = world.add_entity(e);
        base_object_run_monster(&mut world, id); // clock 0 ⇒ recovery 0
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.hp, 4, "负体力点扣 1 点 HP");
        assert_eq!(e.health_tick, -1, "HealthTick 回填 HealthFillTime");
        // `HP == 1` ⇒ `HP > 1` 门挡住。
        let mut e = Entity::new(0, "稻草人", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 1;
        e.max_hp = 60;
        e.health_tick = -301;
        let id2 = world.add_entity(e);
        base_object_run_monster(&mut world, id2);
        let e = world.entities.get(id2).unwrap();
        assert_eq!(e.hp, 1, "HP == 1 时不得再扣");
        assert_eq!(e.health_tick, -301, "被门挡住 ⇒ HealthTick 不回填");
    }

    /// `BaseObject.Run` 的 `TargetCret` 清理段（`BaseObject.Base.cs:111-118`）四种失效条件。
    #[test]
    fn target_cret_cleanup_covers_age_death_map_and_distance() {
        let build = |clock: i64, tx: i32, ty: i32, kill: bool, tmap: usize| {
            let mut world = World::new(64, 64);
            world.clock_ms = 0;
            let id = monster(&mut world, 10, 10);
            let tid = player(&mut world, tx, ty);
            if tmap != 0 {
                world.entities.get_mut(tid).unwrap().map_id = tmap;
            }
            if kill {
                world.entities.get_mut(tid).unwrap().death = true;
            }
            set_target_creature(&mut world, id, tid);
            world.clock_ms = clock;
            base_object_run_monster(&mut world, id);
            world.entities.get(id).unwrap().ai.target_creature
        };
        assert!(
            build(0, 11, 10, false, 0).is_some(),
            "同图近距且未过期 ⇒ 保留"
        );
        assert!(build(30_001, 11, 10, false, 0).is_none(), "30s 过期 ⇒ 清理");
        assert!(build(0, 11, 10, true, 0).is_none(), "目标死亡 ⇒ 清理");
        assert!(build(0, 11, 10, false, 1).is_none(), "异图 ⇒ 清理");
        assert!(build(0, 26, 10, false, 0).is_none(), "|Δx| > 15 ⇒ 清理");
        assert!(build(0, 11, 26, false, 0).is_none(), "|Δy| > 15 ⇒ 清理");
    }

    /// M-1：清理段走 oracle 的 `ClearTargetCreat`（`BaseObject.Base.cs:568-585`）——
    /// 除 `DelTargetCreat` 外还要把目标从 `VisibleActors` 移除。
    /// 改坏必红：把 `clear_target_creature` 换回 `del_target_creature` ⇒ 第二个断言红。
    #[test]
    fn target_cret_cleanup_also_removes_visible_actor() {
        let mut world = World::new(64, 64);
        world.clock_ms = 0;
        let id = monster(&mut world, 10, 10);
        let tid = player(&mut world, 11, 10);
        world.entities.get_mut(id).unwrap().visible_actors = vec![
            VisibleBaseObject {
                target: tid,
                flag: VisibleFlag::Show,
            },
            VisibleBaseObject {
                target: 999,
                flag: VisibleFlag::Show,
            },
        ];
        set_target_creature(&mut world, id, tid);
        world.clock_ms = 30_001; // 过期 ⇒ 清理
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(e.ai.target_creature.is_none(), "目标已清理");
        assert!(
            !e.visible_actors.iter().any(|v| v.target == tid),
            "目标应从 VisibleActors 移除（ClearTargetCreat）"
        );
        assert!(
            e.visible_actors.iter().any(|v| v.target == 999),
            "其它可见体不受影响"
        );
    }

    // ------------------------------------------------------------------
    // W4：怪物 → 玩家攻击判定链（AnimalObject.Struck / MonsterObject.AttackTarget）
    //     oracle：AnimalObject.cs:334-360、MonsterObject.cs:220-245、BaseObject.Attack.cs:37-64
    // ------------------------------------------------------------------

    /// 找一个「首抽 `Random(6)` 为 `want_zero`」的种子。
    fn seed_random6(want_zero: bool) -> i32 {
        for seed in 0..100_000 {
            let mut r = RandomNumber::with_seed(seed);
            if (r.random_below(6) == 0) == want_zero {
                return seed;
            }
        }
        unreachable!("应能在 10 万种子里找到 Random(6) 门取值");
    }

    /// W4-A：`AnimalObject.Struck`（`AnimalObject.cs:334-360`）的**索敌支**——
    /// `TargetCret == null` ⇒ 短路**不抽** `Random(6)`；`IsProperTarget(hiter)` 成立即认领；
    /// 尾 `AttackTick += 150 - MIN(130, Level*4)`。
    /// 改坏必红：删掉认领支（或 `base_object_run_monster` 的队列出队）⇒ 第一个断言红。
    #[test]
    fn animal_struck_acquires_hitter_without_target_and_bumps_attack_tick() {
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        world.entities.get_mut(mid).unwrap().level = 1;
        let pid = player(&mut world, 11, 10);
        world.rng = RandomNumber::with_seed(1234);
        let mut probe = RandomNumber::with_seed(1234);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, pid);
        let e = world.entities.get(mid).unwrap();
        assert_eq!(
            e.ai.target_creature,
            Some(pid),
            "无目标 ⇒ 认领命中者（Struck 索敌支）"
        );
        assert_eq!(e.ai.attack_tick, 146, "AttackTick += 150 - min(130, 1*4)");
        assert_eq!(
            world.rng.random(),
            probe_next,
            "TargetCret==null ⇒ 不得抽 Random(6)"
        );
    }

    /// W4-B：`TargetCret != null` 且**邻接** ⇒ `GetAttackDir` 真 ⇒ 短路**不抽** `Random(6)`。
    #[test]
    fn animal_struck_adjacent_target_takes_without_drawing() {
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        let old = player(&mut world, 11, 10); // 邻接
        let hitter = player(&mut world, 12, 10);
        set_target_creature(&mut world, mid, old);
        world.rng = RandomNumber::with_seed(7);
        let mut probe = RandomNumber::with_seed(7);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, hitter);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(hitter)
        );
        assert_eq!(world.rng.random(), probe_next, "邻接支不得抽 Random(6)");
    }

    /// W4-C：`TargetCret != null` 且**不邻接** ⇒ 恰好抽 **1 次** `Random(6)`：
    /// `== 0` 认领命中者、`!= 0` 保持旧目标。取数次序/次数须与 oracle 逐次一致。
    /// 改坏必红：把 `get_attack_dir(...).is_some() || ...` 短路改反（无条件抽/不抽）⇒ 次数断言红。
    #[test]
    fn animal_struck_draws_random6_only_when_target_present_and_far() {
        // Random(6) == 0 ⇒ take=true ⇒ 目标换成命中者，恰好抽 1 次。
        let seed = seed_random6(true);
        let mut world = World::new(40, 40);
        let mid = monster(&mut world, 10, 10);
        let far = player(&mut world, 20, 20); // 非邻接
        let hitter = player(&mut world, 11, 10);
        set_target_creature(&mut world, mid, far);
        world.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        assert_eq!(probe.random_below(6), 0);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, hitter);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(hitter),
            "Random(6)==0 ⇒ 认领命中者"
        );
        assert_eq!(
            world.rng.random(),
            probe_next,
            "非邻接支恰好抽 1 次 Random(6)"
        );

        // Random(6) != 0 ⇒ take=false ⇒ 保持旧目标，仍恰好抽 1 次。
        let seed = seed_random6(false);
        let mut world = World::new(40, 40);
        let mid = monster(&mut world, 10, 10);
        let far = player(&mut world, 20, 20);
        let hitter = player(&mut world, 11, 10);
        set_target_creature(&mut world, mid, far);
        world.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        assert_ne!(probe.random_below(6), 0);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, hitter);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(far),
            "Random(6)!=0 ⇒ 保持旧目标"
        );
        assert_eq!(
            world.rng.random(),
            probe_next,
            "非邻接支恰好抽 1 次 Random(6)"
        );
    }

    /// W4-D：`IsProperTarget(hiter)` 门（`AnimalObject.cs:344-347`）——已死命中者不得被认领。
    /// 改坏必红：删掉 `is_proper_target` 门 ⇒ 断言红。
    #[test]
    fn animal_struck_rejects_improper_hitter() {
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        let dead = player(&mut world, 11, 10);
        world.entities.get_mut(dead).unwrap().death = true;
        world.rng = RandomNumber::with_seed(3);
        let mut probe = RandomNumber::with_seed(3);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, dead);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            None,
            "已死命中者不得被认领"
        );
        assert_eq!(world.rng.random(), probe_next, "TargetCret==null 支不抽数");
    }

    /// W4-E：`RM_STRUCK` 队列（`AnimalState::pending_struck`）在怪物自己的 `base.Run()`
    /// （[`base_object_run_monster`]）里按**入队序**出队；已消失的命中者跳过。
    /// 改坏必红：删掉出队循环 ⇒ 第一个断言红。
    #[test]
    fn base_object_run_monster_drains_struck_queue_in_order() {
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        let gone = player(&mut world, 12, 10);
        let live = player(&mut world, 11, 10);
        world.entities.remove(gone);
        world.entities.get_mut(mid).unwrap().ai.pending_struck = vec![gone, live];
        base_object_run_monster(&mut world, mid);
        let e = world.entities.get(mid).unwrap();
        assert!(e.ai.pending_struck.is_empty(), "队列应被出队清空");
        assert_eq!(
            e.ai.target_creature,
            Some(live),
            "已消失的命中者跳过，活的命中者被认领"
        );
    }

    /// W4-F：`MonsterObject.AttackTarget`（`MonsterObject.cs:220-245`）——邻接且到点
    /// （`now - AttackTick > NextHitTime`）才对目标 `_Attack`；结算落进 `pending_ai_attacks`
    /// （宿主转抄 `TickRecord::attacks` ⇒ `drain_combat_downlink` ① 出 `SM_STRUCK`）。
    /// 改坏必红：去攻击间隔门 ⇒ 首个断言（未到点不得出记录）红；去记录支 ⇒ 末条断言红。
    #[test]
    fn monster_attack_target_gates_on_interval_and_records_attack() {
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        {
            let m = world.entities.get_mut(mid).unwrap();
            m.dc = 10 | (10 << 8); // lo=10,hi=10 ⇒ power 10
            m.level = 1;
            m.ai.attack_tick = 0;
        }
        let pid = player(&mut world, 11, 10);
        set_target_creature(&mut world, mid, pid);
        world.clock_ms = 1000;
        assert!(attack_target(&mut world, mid), "邻接 ⇒ 返回 true");
        assert!(
            world.pending_ai_attacks.is_empty(),
            "到点门未过（1000-0=1000 <= 2000）⇒ 不得出攻击记录"
        );
        world.clock_ms = 2200;
        assert!(attack_target(&mut world, mid), "邻接 ⇒ 返回 true");
        let m = world.entities.get(mid).unwrap();
        assert_eq!(m.ai.dir, DIR_RIGHT, "先 TurnTo(GetAttackDir)");
        assert_eq!(m.ai.attack_tick, 2200, "到点后刷新 AttackTick");
        let rec = world.pending_ai_attacks.clone();
        assert_eq!(rec.len(), 1, "恰 1 条 AI 攻击记录");
        assert_eq!(
            (rec[0].0, rec[0].1),
            (mid, pid),
            "记录 (攻方=怪, 受击方=玩家)"
        );
        assert!(rec[0].2.hit, "怪对玩家的攻击应命中");
        assert!(rec[0].2.damage > 0, "护甲 0 ⇒ 伤害应 > 0（下游才出帧）");
    }

    /// W4-G（端到端）：玩家打怪 ⇒ 怪被 `RM_STRUCK` ⇒ 怪在后续 AI 里**反击玩家**，
    /// 且反击结算落进 `TickRecord::attacks`（= 宿主「怪打玩家」帧的数据源）。固定种子两次一致。
    /// 改坏必红：删掉 `attack_target` 的攻击支（或 `pending_struck` 的出队）⇒ 找不到记录。
    #[test]
    fn player_hit_triggers_monster_retaliation_via_tick_and_records_frame() {
        fn run() -> Option<(i32, i32)> {
            let mut world = World::new(40, 40);
            let mid = monster(&mut world, 10, 10);
            {
                let m = world.entities.get_mut(mid).unwrap();
                m.dc = 10 | (10 << 8);
                m.level = 1;
            }
            let pid = player(&mut world, 11, 10);
            world.rng = RandomNumber::with_seed(2024);
            world.enable_monster_ai();
            // 玩家攻击怪（= CM_HIT 的结算）⇒ 怪被命中 ⇒ 入队 `RM_STRUCK`。
            let o = world.attack_once(pid, mid).expect("玩家攻怪应结算");
            assert!(o.hit, "玩家应命中怪");
            let mut found = None;
            for _ in 0..40 {
                let rec = world.tick();
                for (a, t, _) in &rec.attacks {
                    if *a == mid && *t == pid {
                        found = Some((*a, *t));
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            found
        }
        let a = run();
        assert!(a.is_some(), "怪应在被命中后的某 tick 反击玩家");
        assert_eq!(a, run(), "固定种子两次运行必须一致（确定性）");
    }

    // ==================================================================
    // W9：① 主动索敌（ATMonster.Run）・② Animal 肉质扣减・③ 守卫静止
    //     oracle：ATMonster.cs:13-24、AnimalObject.cs:334-360、GuardUnit.cs/NormNpc.cs:1604-1611
    // ==================================================================

    /// 造一只指定 race 的怪；`walk_speed` 顶高以把 `monster_run` 隔离在索敌段
    /// （`now - walk_tick > walk_speed` 恒假 ⇒ 不进游走/攻击段）。
    fn race_monster(world: &mut World, race: u8, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "m", x, y);
        e.race = race;
        e.hp = 60;
        e.max_hp = 60;
        e.ai.walk_speed = 1_000_000;
        world.add_entity(e)
    }

    /// ①-A：`ATMonster.Run`（`ATMonster.cs:17`）的时钟门四态——race 81/83 的怪只在
    /// `(now-SearchEnemyTick)>8000 || ((now-SearchEnemyTick)>1000 && TargetCret==null)` 时索敌。
    /// 改坏必红：去掉 `search_target` 调用 ⇒ 段 3/4（应认领却空）断言红。
    #[test]
    fn active_seek_race_searches_only_under_clock_gate() {
        let mut world = World::new(60, 60);
        let mid = race_monster(&mut world, 83, 10, 10); // SlowAtMonster：继承 ATMonster.Run
        let pid = player(&mut world, 12, 10);
        world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
            target: pid,
            flag: VisibleFlag::Show,
        }];
        // 段1：`now - SearchEnemyTick <= 1000` ⇒ 不搜。
        world.clock_ms = 1000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            None,
            "<=1000 不索敌"
        );
        // 段2：`>1000` 但已有目标 ⇒ 不搜（保持哨兵）。
        set_target_creature(&mut world, mid, pid);
        world.clock_ms = 5000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(pid),
            ">1000 但有目标 ⇒ 不重搜"
        );
        // 段3：`>1000` 且无目标 ⇒ 搜（认领最近可见玩家）。
        world.entities.get_mut(mid).unwrap().ai.target_creature = None;
        world.clock_ms = 5000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(pid),
            ">1000 且无目标 ⇒ 索敌"
        );
        // 段4：`>8000` 即使有目标也重搜（超距者被清、取最近合法者）。
        let far = player(&mut world, 40, 10);
        world
            .entities
            .get_mut(mid)
            .unwrap()
            .visible_actors
            .push(VisibleBaseObject {
                target: far,
                flag: VisibleFlag::Show,
            });
        set_target_creature(&mut world, mid, far);
        world.clock_ms = 9000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(pid),
            ">8000 重搜 ⇒ 取更近者（far 超距被清）"
        );
    }

    /// ①-B：`SearchTarget` **不消费随机数**（RNG 流水不变）；race 80（非索敌族）即使有可见玩家也不索敌。
    /// 改坏必红：给 SearchTarget 加抽数 ⇒ 流水断言红；把门放开到全体 ⇒ race 80 断言红。
    #[test]
    fn active_seek_is_rng_free_and_non_seek_race_never_searches() {
        // race 81：门到点、索敌成功，但 RNG 流水不动。
        let mut world = World::new(60, 60);
        let mid = race_monster(&mut world, 81, 10, 10);
        let pid = player(&mut world, 12, 10);
        world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
            target: pid,
            flag: VisibleFlag::Show,
        }];
        world.clock_ms = 5000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        world.rng = RandomNumber::with_seed(777);
        let mut probe = RandomNumber::with_seed(777);
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(pid),
            "race 81 应索敌"
        );
        assert_eq!(
            world.rng.random(),
            probe.random(),
            "SearchTarget 不得消费随机数"
        );

        // race 80（MonsterObject 本体）：同样有可见玩家，但**不**自动索敌。
        let mut world = World::new(60, 60);
        let mid = race_monster(&mut world, 80, 10, 10);
        let pid = player(&mut world, 12, 10);
        world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
            target: pid,
            flag: VisibleFlag::Show,
        }];
        world.clock_ms = 5000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            None,
            "race 80 不索敌"
        );
    }

    /// ①-C：`is_active_seek_race` 的分类（含 `AtMonster` 本体 + `SlowAtMonster`；排除覆写 `Run` 的派生）。
    #[test]
    fn active_seek_race_classification_matches_oracle() {
        for r in [53u8, 81, 83, 86, 88, 89] {
            assert!(is_active_seek_race(r), "race {r} 应为主动索敌族");
        }
        // race 80（MonsterObject）与覆写 Run 的派生（82/84/90/105/107/114/132）不在本片族内。
        for r in [80u8, 82, 84, 85, 90, 105, 106, 107, 114, 118, 119, 132] {
            assert!(!is_active_seek_race(r), "race {r} 不应为主动索敌族");
        }
    }

    /// ②-A：`AnimalObject.Struck`（`AnimalObject.cs:350-357`）——`Animal` 为真 ⇒ 恰抽 1 次
    /// `Random(300)` 扣 `MeatQuality`。固定种子断言取数与流水位置（在索敌支之后）。
    /// 改坏必红：门恒 false（去掉 `is_animal_race`）⇒ 断言红。
    #[test]
    fn animal_struck_consumes_random300_and_reduces_meat_quality() {
        let mut world = World::new(30, 30);
        let mid = race_monster(&mut world, 52, 10, 10); // 鹿：AnimalDeer
        world.entities.get_mut(mid).unwrap().ai.meat_quality = 5000;
        let pid = player(&mut world, 11, 10);
        world.clock_ms = 100;
        world.rng = RandomNumber::with_seed(4242);
        let mut probe = RandomNumber::with_seed(4242);
        // 无目标 ⇒ 索敌支短路**不抽** `Random(6)`；随后 `Animal` 支抽 1 次 `Random(300)`。
        let draw = probe.random_below(300);
        animal_struck(&mut world, mid, pid);
        let e = world.entities.get(mid).unwrap();
        assert_eq!(
            e.ai.meat_quality,
            5000 - draw as u16,
            "MeatQuality -= Random(300)"
        );
        assert_eq!(
            world.rng.random(),
            probe.random(),
            "恰抽 1 次 Random(300)，位置在索敌支之后"
        );
    }

    /// ②-B：`Animal` 为假（race 80）⇒ **不**抽 `Random(300)`、`MeatQuality` 不变。
    /// 改坏必红：门恒真 ⇒ 断言红。
    #[test]
    fn animal_struck_no_random300_for_non_animal_race() {
        let mut world = World::new(30, 30);
        let mid = race_monster(&mut world, 80, 10, 10);
        world.entities.get_mut(mid).unwrap().ai.meat_quality = 5000;
        let pid = player(&mut world, 11, 10);
        world.clock_ms = 100;
        world.rng = RandomNumber::with_seed(4242);
        let mut probe = RandomNumber::with_seed(4242);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, pid);
        let e = world.entities.get(mid).unwrap();
        assert_eq!(e.ai.meat_quality, 5000, "非 Animal 不扣肉质");
        assert_eq!(
            world.rng.random(),
            probe_next,
            "恰抽 0 次随机数（无目标支 + 非 Animal）"
        );
    }

    /// ②-C：`MeatQuality` 下限 0（`AnimalObject.cs:353-356`）——`u16` 不得回绕。
    /// 改坏必红：去掉 `< 0 ⇒ 0` 夹取 ⇒ `0u16 - Random(300)` 回绕成 ~65236。
    #[test]
    fn animal_struck_clamps_meat_quality_at_zero() {
        let mut world = World::new(30, 30);
        let mid = race_monster(&mut world, 53, 10, 10); // 狼：AnimalWolf（AtMonster 系）
        world.entities.get_mut(mid).unwrap().ai.meat_quality = 0;
        let pid = player(&mut world, 11, 10);
        world.rng = RandomNumber::with_seed(5);
        animal_struck(&mut world, mid, pid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.meat_quality,
            0,
            "扣减后夹到 0（不回绕）"
        );
    }

    /// ②-D：`is_animal_race` 分类与 oracle 一致（含 ctor 继承与 `CentipedeKing` 的显式排除）。
    #[test]
    fn animal_race_classification_matches_oracle() {
        for r in [51u8, 52, 53, 82, 84, 85, 90, 105, 106, 114, 118, 119, 132] {
            assert!(is_animal_race(r), "race {r} 应为 Animal");
        }
        // 54（ReAliveEx 才置位，复活未建模）・107（CentipedeKing ctor 显式 false）・非动物。
        for r in [54u8, 80, 81, 83, 86, 88, 89, 107, 11, 112] {
            assert!(!is_animal_race(r), "race {r} 不应为 Animal");
        }
    }

    /// 造一只有可见玩家的怪，跑 `ticks` 个 `monster_ai_tick`，返回末位置 `(x, y)`。
    fn run_ai_ticks(race: u8, ticks: u32) -> (i32, i32) {
        let mut world = World::new(60, 60);
        let mut m = Entity::new(0, "m", 20, 20);
        m.race = race;
        m.hp = 6000;
        m.max_hp = 6000;
        let mid = world.add_entity(m);
        let pid = player(&mut world, 22, 20);
        world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
            target: pid,
            flag: VisibleFlag::Show,
        }];
        world.rng = RandomNumber::with_seed(20261016);
        for _ in 0..ticks {
            world.clock_ms += 200;
            monster_ai_tick(&mut world);
        }
        let e = world.entities.get(mid).unwrap();
        (e.x, e.y)
    }

    /// ③-A：race 11（守卫）静止不动；race 80（普通怪）照常游走。
    /// 改坏必红：去掉 `monster_ai_tick` 的守卫跳过 ⇒ 守卫游走 ⇒ 首个断言红。
    #[test]
    fn guard_race_stays_static_while_normal_monster_wanders() {
        assert_eq!(
            run_ai_ticks(11, 800),
            (20, 20),
            "守卫（race 11）必须静止（不游走/不索敌）"
        );
        assert_ne!(
            run_ai_ticks(80, 800),
            (20, 20),
            "普通怪（race 80）应照常游走"
        );
    }

    /// ③-B：守卫虽跳过 `monster_run`，仍要跑 `base.Run()` 段——`RM_STRUCK` 队列照常出队。
    /// 改坏必红：把守卫改成 `continue` 前不出队（或整只跳过 AI）⇒ 队列不为空。
    #[test]
    fn guard_still_drains_struck_queue() {
        let mut world = World::new(30, 30);
        let gid = race_monster(&mut world, 11, 10, 10);
        let hitter = player(&mut world, 11, 10);
        world.entities.get_mut(gid).unwrap().ai.pending_struck = vec![hitter];
        world.clock_ms = 200;
        monster_ai_tick(&mut world);
        let e = world.entities.get(gid).unwrap();
        assert!(
            e.ai.pending_struck.is_empty(),
            "守卫的 RM_STRUCK 队列应被出队"
        );
        assert_eq!(
            e.ai.target_creature,
            Some(hitter),
            "守卫被击应认领命中者（GuardUnit.Struck 同 AnimalObject.Struck）"
        );
        assert_eq!((e.x, e.y), (10, 10), "处理受击也不移动");
    }

    /// ③-C：`is_guard_race` 分类（`CreateMonster` 的守卫族；不含普通怪）。
    #[test]
    fn guard_race_classification_matches_oracle() {
        for r in [11u8, 12, 20, 112] {
            assert!(is_guard_race(r), "race {r} 应为守卫族");
        }
        for r in [0u8, 10, 50, 80, 81, 83, 110, 111] {
            assert!(!is_guard_race(r), "race {r} 不应为守卫族");
        }
    }

    // ---- 尸体 / MakeGhost 门（卡 openmir2-run-gap-corpse-ghost-revive） ----------------

    /// 造一只已死怪（race 80 = 普通 `MonsterObject`，非 Animal）。
    fn corpse(
        world: &mut World,
        can_re_alive: bool,
        has_mon_gen: bool,
        zen_time: i32,
        death_tick: i64,
    ) -> i32 {
        let mut e = Entity::new(0, "尸体", 10, 10);
        e.race = 80;
        e.max_hp = 60;
        e.hp = 0;
        e.max_mp = 10;
        e.mp = 0;
        e.death = true;
        e.death_tick = death_tick;
        e.can_re_alive = can_re_alive;
        e.mon_gen = if has_mon_gen { Some(0) } else { None };
        e.mon_gen_zen_time = zen_time;
        world.add_entity(e)
    }

    /// 判据 a：`ZenTime` 足够大 ⇒ 门 = `ZenTime - 20s`（未触截断）：
    /// `DeathTick + gate`（等于）**不** Ghost，`+1` Ghost。两条门都是严格 `>`。
    ///
    /// **牙①**：去掉可复活支的 `- 20 * 1000` ⇒ 门变 120000，`+100001` 不 Ghost ⇒ 本用例红。
    #[test]
    fn make_ghost_gate_uses_zen_time_minus_20s() {
        let mut world = World::new(64, 64);
        world.set_corpse_config(180_000, 5000, 300);
        // gate = max(10000, 120000 - 20000) = 100000（< MakeGhostTime ⇒ 不截断）。
        let id = corpse(&mut world, true, true, 120_000, 1_000);

        world.clock_ms = 1_000 + 99_999;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(!e.invisible && !e.ghost, "gate-1 不得 MakeGhost");

        world.clock_ms = 1_000 + 100_000;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(
            !e.invisible && !e.ghost,
            "恰等于 gate 不得 MakeGhost（严格 >）"
        );

        world.clock_ms = 1_000 + 100_001;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(e.invisible, "gate+1 可复活怪 ⇒ Invisible");
        assert!(!e.ghost, "可复活怪保留尸体，不置 Ghost");
        assert_eq!(e.ghost_tick, world.clock_ms, "GhostTick = GetTickCount()");
    }

    /// 判据 b：`ZenTime` 小 ⇒ `MAX(10s)` 下限生效（`25000-20000=5000 ⇒ 10000`）。
    ///
    /// **牙②**：去掉 `MAX(10 * 1000, ...)` ⇒ 门变 5000，`+10001` 时仍 Ghost，但
    /// `+10000` 处就已经 Ghost ⇒ 第一条断言红。
    #[test]
    fn make_ghost_gate_floors_at_10s() {
        let mut world = World::new(64, 64);
        world.set_corpse_config(180_000, 5000, 300);
        let id = corpse(&mut world, true, true, 25_000, 1_000);

        world.clock_ms = 1_000 + 10_000;
        base_object_run_monster(&mut world, id);
        assert!(
            !world.entities.get(id).unwrap().invisible,
            "MAX(10s) 下限：恰等 10000 不 Ghost"
        );
        world.clock_ms = 1_000 + 10_001;
        base_object_run_monster(&mut world, id);
        assert!(
            world.entities.get(id).unwrap().invisible,
            "MAX(10s) 下限：10001 必须 Ghost"
        );
    }

    /// 截断步：`ZenTime - 20s > Config.MakeGhostTime` ⇒ 取 `Config.MakeGhostTime`。
    #[test]
    fn make_ghost_gate_truncates_to_config_make_ghost_time() {
        let mut world = World::new(64, 64);
        world.set_corpse_config(90_000, 5000, 300);
        // 2_000_000 - 20_000 = 1_980_000 > 90_000 ⇒ 截到 90_000。
        let id = corpse(&mut world, true, true, 2_000_000, 1_000);
        world.clock_ms = 1_000 + 90_000;
        base_object_run_monster(&mut world, id);
        assert!(
            !world.entities.get(id).unwrap().invisible,
            "恰等截断值不 Ghost"
        );
        world.clock_ms = 1_000 + 90_001;
        base_object_run_monster(&mut world, id);
        assert!(
            world.entities.get(id).unwrap().invisible,
            "截断值 +1 必须 Ghost"
        );
    }

    /// 判据 c：`CanReAlive=false` ⇒ 走普适支 `Config.MakeGhostTime`（90s）。
    ///
    /// **牙③**：把普适支的 `>` 改成 `>=` ⇒ 恰等 90000 时 Ghost ⇒ 第二条断言红。
    #[test]
    fn make_ghost_gate_non_revivable_is_config_value() {
        let mut world = World::new(64, 64);
        world.set_corpse_config(90_000, 5000, 300);
        // 即使 `MonGen != null`，`CanReAlive == false` 也必须走普适支。
        let id = corpse(&mut world, false, true, 120_000, 1_000);

        world.clock_ms = 1_000 + 89_999;
        base_object_run_monster(&mut world, id);
        assert!(!world.entities.get(id).unwrap().invisible);

        world.clock_ms = 1_000 + 90_000;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(
            !e.invisible && !e.ghost,
            "普适支严格 >：恰等 90000 不 Ghost"
        );

        world.clock_ms = 1_000 + 90_001;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(e.ghost, "普适支：90001 ⇒ Ghost=true");
        assert!(!e.invisible, "普适支不得保留 Invisible 尸体");
    }

    /// 判据 f（阴性对照）：活怪不受尸体门影响；且死亡怪在门未到前也不改变状态。
    #[test]
    fn live_monster_is_not_affected_by_corpse_gates() {
        let mut world = World::new(64, 64);
        world.set_corpse_config(10_000, 5000, 300);
        let id = monster(&mut world, 10, 10);
        world.clock_ms = 10_000_000;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(!e.death && !e.ghost && !e.invisible);
        assert_eq!(e.ghost_tick, 0, "活怪不得写 GhostTick");

        // 刚死（门未到）的怪同样不动。
        let now = world.clock_ms;
        let dead = corpse(&mut world, false, false, 0, now);
        base_object_run_monster(&mut world, dead);
        let e = world.entities.get(dead).unwrap();
        assert!(!e.ghost && !e.invisible, "门未到不得 MakeGhost");
    }

    /// 死亡时刻由 `settle_deaths` 写 `death_tick`（`BaseObject.cs:323`）。
    #[test]
    fn death_settlement_records_death_tick() {
        let mut world = World::new(32, 32);
        let id = monster(&mut world, 10, 10);
        world.clock_ms = 12_345;
        world.entities.get_mut(id).unwrap().hp = 0;
        world.tick();
        let e = world.entities.get(id).unwrap();
        assert!(e.death);
        // `tick()` 先推进时钟（+200ms）再结算 ⇒ `DeathTick` = 结算时刻。
        assert_eq!(e.death_tick, world.clock_ms);
        assert_eq!(e.death_tick, 12_545);
    }
}
