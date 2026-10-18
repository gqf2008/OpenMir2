//! `BaseObject.SpaceMoveGetRandXY` 的独立模块（`mir2-rs` 侧）。
//!
//! **为什么单独成文件（2026-10-17）**：这条取数被**两个** oracle 函数共用——`BaseObject.RecallSlave`
//! （`BaseObject.cs:1850`，魔号 30 回落）与 `BaseObject.MapRandomMove`（`:1214`，商人随机移动）——而
//! RNG 结构一致性门（`rng-trace-probe/consumer_groups.tsv` 的 `groups --require-closed`）按**站点字符串**
//! 归组。若它留在 `slave.rs`，则只能整文件归组，会把**同文件的另一个 oracle 函数**
//! （`PlayObject.ProcessSlaveMutiny`，`PlayObject.Base.cs:344`）的抽数一并吞进同一组 ⇒ 真实机 trace
//! 在该场景会与 C# 侧分组不一致（假红）。单独成文件后，两侧都按**函数**对齐：
//! `space_move` 组 = 本文件；`slave_mutiny` 组 = `slave.rs`。
//!
//! 公开 API 未变：`crate::slave::space_move_get_rand_xy` 仍可用（`slave.rs` 里 `pub use` 再导出）。

use mir2_shared::rng::RandomSource;

use crate::map::CellType;
use crate::world::World;

/// `BaseObject.SpaceMoveGetRandXY`（`BaseObject.cs:1040-1096`）：从 `(x, y)` 起找可走格；
/// 不可走时**按 oracle 的分段步进/随机跳点**推进，最多 201 次尝试（`n14 >= 201` 即放弃）。
///
/// `n18 = Width < 80 ? 3 : 10`；`n1C = Height < 150 ? (Height < 50 ? 2 : 15) : 50`。
/// `can_walk(x, y)` = oracle `Envir.CanWalk(x, y, true)`（`cellSuccess && Valid`）。
///
/// **取数次序是 Claim B 账本输入**：只有进入 `else` 分支（`x` 已到 `Width-n1C-1` 之后）才抽
/// `Random(Width)` / `Random(Height)`，本函数逐字保留分支位置。
pub fn space_move_get_rand_xy<F>(
    rng: &mut impl RandomSource,
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

/// `BaseObject.SpaceMove(sMap, nX, nY, nInt)`（`BaseObject.cs:1103-1180`）的**同服分支**。
///
/// 验收形态是「单 GameSvr 单区单 World」⇒ `M2Share.ServerIndex == envir.ServerIndex` 恒真，
/// 只实现这一支；跨服支（`ChangeSpaceMove`/`KickException`）不建模（登记）。
///
/// 逐句对照：
/// 1. `oldEnvir = Envir; nOldX = CurrX; nOldY = CurrY;`（`:1110-1112`）；
/// 2. `Envir.DeleteFromMap(CurrX, CurrY, CellType, ActorId, this)` ⇒ [`World::disappear_a`]（只摘格）；
/// 3. `Envir = envir; MapName = ...; CurrX = nX; CurrY = nY;`（`:1118-1121`）——
///    **在取数之前**就写落地图/坐标（oracle 同序）；
/// 4. `SpaceMoveGetRandXY(Envir, ref tempX, ref tempY)`（`:1128`）⇒
///    [`space_move_get_rand_xy`]（**唯一取数点**；站点仍在本文件 ⇒ `space_move` 组，
///    与 `BaseObject.SpaceMoveGetRandXY@BaseObject.cs:1040` 对齐）；
/// 5. 成功 ⇒ 写临时坐标、`AddMapObject`（返回 bool 在 oracle 被忽略）、`SpaceMoved = true`；
///    失败（201 轮仍不可走）⇒ 回滚 `Envir/CurrX/CurrY` 并 `AddMapObject` 旧格（`:1147-1153`）；
/// 6. `OnEnvirnomentChanged()`（`:1173`）：`CanReAlive && MonGen.Envir != Envir` ⇒
///    `CanReAlive = false; MonGen = null; ActiveCount--`（`BaseObject.cs:2747-2760`）。
///
/// **登记（未建模）**：
/// - 出帧面：`SendMsg(RM_CLEAROBJECTS/RM_CHANGEMAP)` + `SendRefMsg(RM_SPACEMOVE_SHOW2|SHOW)`
///   （怪物无会话；AOI 由下一 tick 视野差集表现，见 `World::disappear_a` 同款登记）；
/// - 跨服支；
/// - `OnEnvirnomentChanged` 的 `MonGen.ActiveCount--`：`MonGenState` 归宿主持有、不在 `World` 内
///   ⇒ 只复刻实体侧可见效果（`can_re_alive=false; mon_gen=None`）。`mon_gen.is_some()` 期间实体
///   恒在其出生的 MonGen 图上（首次异图移动即清位）⇒ `old_map_id != dest_map_id` 是
///   `MonGen.Envir != Envir` 的等价判据。
///
/// 返回 `SpaceMoved`（落点成功）；`false` 表示已回滚到调用前的地图/坐标。
pub(crate) fn monster_space_move(
    world: &mut World,
    actor_id: i32,
    dest_map_id: usize,
    n_x: i32,
    n_y: i32,
) -> bool {
    let Some(dest_map) = world.maps.get(dest_map_id) else {
        return false;
    };
    let (width, height) = (dest_map.width, dest_map.height);
    let Some((old_map_id, old_x, old_y)) =
        world.entities.get(actor_id).map(|e| (e.map_id, e.x, e.y))
    else {
        return false;
    };

    // 2. 摘旧格（oracle `DeleteFromMap`）。
    world.disappear_a(actor_id);
    // 3. 先写目标图/目标格（oracle 在 `SpaceMoveGetRandXY` 之前写 `CurrX/CurrY`）。
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.map_id = dest_map_id;
        e.x = n_x;
        e.y = n_y;
    }

    // 4. `SpaceMoveGetRandXY(Envir, ref tempX, ref tempY)`——取数点。
    let found = {
        let map = &world.maps[dest_map_id];
        space_move_get_rand_xy(&mut world.rng, width, height, n_x, n_y, |x, y| {
            map.get_cell_info(x, y).1
        })
    };

    let now = world.clock_ms;
    let space_moved = found.is_some();
    let added = match found {
        Some((x, y)) => {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.x = x;
                e.y = y;
            }
            // 5-success. `AddMapObject`（`CellType = Monster`，`AnimalObject.cs:86`）。
            world.maps[dest_map_id].add_map_object(x, y, CellType::Monster, actor_id, now)
        }
        None => {
            // 5-fail. 回滚 `Envir/CurrX/CurrY` 并把对象重新登记到旧格（`BaseObject.cs:1147-1153`）。
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.map_id = old_map_id;
                e.x = old_x;
                e.y = old_y;
            }
            world.maps[old_map_id].add_map_object(old_x, old_y, CellType::Monster, actor_id, now)
        }
    };
    // 本仓内部状态位（oracle `AddToMaped` 不在此处语义内）；保持「已落格」不变量。
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.add_to_mapped = added;
    }

    // 6. `OnEnvirnomentChanged()`（实体侧可见效果；ActiveCount 登记未建模）。
    if space_moved && old_map_id != dest_map_id {
        if let Some(e) = world.entities.get_mut(actor_id) {
            if e.can_re_alive && e.mon_gen.is_some() {
                e.can_re_alive = false;
                e.mon_gen = None;
            }
        }
    }
    space_moved
}
