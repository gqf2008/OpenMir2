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
//! - **`BaseObject.Run` 的公共段**（恢复 `HealthTick/SpellTick`、`HP==0 ⇒ Die()`、`LastHiter/ExpHiter`
//!   过期、`MakeGhost`）已由 [`crate::world::World`] 的 `expire_hitters`/`settle_deaths` 建模，
//!   本模块**不重复**；[`monster_run`] 只做 AI（Think/Walk/Attack/Wondering）。
//! - **构造期 RNG**：oracle 在 `new MonsterObject()` 时先抽 `BaseObject` 的 `Random(1500)`+
//!   `Random(2000)`、`AnimalObject` 的 `Random(3000)`×2、`MonsterObject` 的 `Random(2000)`
//!   （`BaseObject.cs:439/441`、`AnimalObject.cs:76/77`、`MonsterObject.cs:65`），**再**由
//!   `CreateMonster` 抽 `RandomByte(8)` 的 `Dir`（`WorldServer.MonGen.cs:740`）。该缺口已由
//!   卡 `openmir2-m3-mongen-load-spawn` 的 F-1 返工闭合：生成路径把构造抽存进
//!   `Entity.ctor_draws`（[`MonsterCtorDraws`]），AI 在宿主接线时只**消费**（
//!   [`AnimalState::seed_from_spawn`](crate::entity::AnimalState::seed_from_spawn)），**不重抽**。
//! - **专用怪子类**（`ATMonster`/`BeeQueen`/... 在各自 `Run()` 里调 `SearchTarget`）与
//!   `Mission`/`RunAway`/`Master`/`CrazyMode`/`CheckRoyalty` 分支**未建模**：默认值下它们
//!   恒不命中，与「普通 `MonsterObject`、无 master、无任务」的 oracle 执行路径一致。

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
            world.attack_once(actor_id, target_id);
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

/// `MonsterObject.Run`（`MonsterObject.cs:251-360`）的**普通怪路径**（无 master/任务/逃跑/狂暴）。
///
/// 只做 AI；`BaseObject.Run` 的恢复/死亡/过期段由 `World` 统一处理（见模块头）。
pub fn monster_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !(can_move(e) && !e.fixed_hide_mode && !e.stone_mode) {
        return;
    }
    if think(world, actor_id) {
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
}

/// 按**实体槽位顺序**对场上所有怪物跑一次 [`monster_run`]（`ProcessMonsters` 的单线程等价物）。
///
/// 顺序契约：来自 `EntityStore` 的显式槽位索引（`iter`），不迭代 Query。
pub fn monster_ai_tick(world: &mut World) {
    let ids: Vec<i32> = world
        .entities
        .iter()
        .filter(|e| e.race > ACTOR_RACE_PLAY && !e.death && !e.ghost)
        .map(|e| e.id)
        .collect();
    for id in ids {
        monster_run(world, id);
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
}
