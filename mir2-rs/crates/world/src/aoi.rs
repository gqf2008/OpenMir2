//! AOI（视野集合）—— 移植源 `src/M2Server/Actor/BaseObject.ViewRange.cs`（逐条照抄）。
//!
//! 规则要点（判据① 钉住的顺序与边界）：
//! 1. `SearchViewRange` 以 `CurrX/CurrY` 为中心扫 **边长 2*ViewRange+1 的正方形**，
//!    **X 外层升序、Y 内层升序**；
//! 2. 每格按 `ObjList` 下标升序；**非玩家观察者**遇到"过期对象"
//!    （`GetTickCount()-AddTime >= 60s`，仅 `ActorObject`）先删该下标，且删后若格内仍有对象则
//!    **不递增下标**（C# 的 `continue`），归零则 `Clear(); break;`。玩家观察者
//!    （`Race == Play`）**跳过**过期块——oracle `PlayObject.SearchViewRange` 把该块注释掉了
//!    （`PlayObject.cs:3114-3123`）；
//! 3. 过滤器（全部满足才可见）：`!Death && !Invisible && !Ghost && !FixedHideMode && !ObMode`
//!    且（观察者 `Master != null || NastyMode || WantRefMsg`，或被观察者 `Master != null` 且
//!    距离 ≤3，或被观察者 `Race == Play`）；
//! 4. 命中后 `UpdateVisibleGay`：已在本列表 ⇒ 标记 `Invisible` 并返回；否则追加 `Show`；
//! 5. 收尾按原下标升序删除仍为 `Hidden` 的条目（保序）。
//!
//! **等价性说明（为什么可以拆成两阶段）**：C# 是"扫描中就地追加/打标"。本实现先按同一顺序
//! 收集候选 id（同时做**同样的**格子过期删除），再按同顺序执行 `UpdateVisibleGay`。
//! 由于 `UpdateVisibleGay` 只改**观察者自己**的 `VisibleActors`，不影响扫描与过滤器所读的
//! 任何字段（过滤器读的是被观察者的 Race/Master/标志位），两阶段与就地等价。

use crate::entity::{
    EntityStore, VisibleBaseObject, VisibleFlag, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY,
};
use crate::map::{CellType, MapGrid};

/// 格子内对象过期阈值（`BaseObject.ViewRange.cs:95`：`60 * 1000` 毫秒）。
pub const CELL_OBJECT_EXPIRE_MS: i64 = 60 * 1000;

/// 对应 `UpdateVisibleGay`（`BaseObject.ViewRange.cs:12`）。
pub fn update_visible_gay(store: &mut EntityStore, viewer_id: i32, target_id: i32) {
    let Some(target) = store.get(target_id) else {
        return;
    };
    let target_is_active = target.race == ACTOR_RACE_PLAY || target.master.is_some();
    let Some(viewer) = store.get_mut(viewer_id) else {
        return;
    };
    if target_is_active {
        viewer.is_visible_active = true;
    }
    for v in viewer.visible_actors.iter_mut() {
        if v.target == target_id {
            v.flag = VisibleFlag::Invisible;
            return;
        }
    }
    viewer.visible_actors.push(VisibleBaseObject {
        target: target_id,
        flag: VisibleFlag::Show,
    });
}

/// 对应 `BaseObject.SearchViewRange`（`BaseObject.ViewRange.cs:57`）。
///
/// `now_ms` 为虚拟时钟（毫秒）。C# 读 `HUtil32.GetTickCount()`；
/// 骨架里时钟由 `crate::world::World` 统一推进（单线程、可复现），**不在模拟逻辑里读墙钟**。
///
/// 返回值 = 本趟**离开视野**（`VisibleFlag::Hidden` ⇒ 被 `RemoveAt`）的观察目标 id，**按移除顺序**
/// （下标升序，保序）。这些正是 oracle `PlayObject.Run()` 的 Hidden 支发 `SM_DISAPPEAR(30)` 的对象
/// （`src/M2Server/Player/PlayObject.cs:3214-3222`）——本函数只管产出事件，帧面由宿主出。
///
/// **守卫**（oracle `:3214`）：只返回 `!FixedHideMode && !Ghost` 的目标（这两类不发消失帧，
/// 防重复占带宽）；`RemoveAt` 本身对全部 Hidden 项照做（与 oracle 同）。
pub fn search_view_range(
    map: &mut MapGrid,
    store: &mut EntityStore,
    viewer_id: i32,
    now_ms: i64,
) -> Vec<i32> {
    let Some(viewer) = store.get(viewer_id) else {
        return Vec::new();
    };
    let (cx, cy, view_range) = (viewer.x, viewer.y, viewer.view_range);
    let viewer_master = viewer.master;
    let viewer_nasty = viewer.nasty_mode;
    let viewer_want_ref = viewer.want_ref_msg;
    // **viewer 分流（oracle 同形）**：`PlayObject.SearchViewRange`（`PlayObject.cs:3073`）把基类
    // 的 60s 过期块**整段注释掉**（`:3114-3123`）⇒ 玩家观察者**不删格子项**；基类
    // `BaseObject.SearchViewRange`（`BaseObject.ViewRange.cs:95`）与 `GuardUnit`（`:155`）仍保留。
    let viewer_is_play = viewer.race == ACTOR_RACE_PLAY;

    // 置位与打标（C#：IsVisibleActive=false；所有 VisibleActors 标 Hidden）
    {
        let Some(viewer) = store.get_mut(viewer_id) else {
            return Vec::new();
        };
        viewer.is_visible_active = false;
        for v in viewer.visible_actors.iter_mut() {
            v.flag = VisibleFlag::Hidden;
        }
    }

    let mut candidates: Vec<i32> = Vec::new();
    for x in (cx - view_range)..=(cx + view_range) {
        for y in (cy - view_range)..=(cy + view_range) {
            let (cell_len, cell_ok) = {
                let (cell, success) = map.get_cell_info(x, y);
                (cell.count(), success)
            };
            if !cell_ok || cell_len == 0 {
                continue; // C#：cellSuccess && cellInfo.IsAvailable
            }
            let mut idx = 0usize;
            loop {
                let (len, obj) = {
                    let (cell, _) = map.get_cell_info(x, y);
                    (cell.count(), cell.obj_list.get(idx).copied())
                };
                let Some(obj) = obj else { break };
                if obj.actor_object {
                    if !viewer_is_play && now_ms - obj.add_time >= CELL_OBJECT_EXPIRE_MS {
                        // C#：Remove(nIdx)；Count>0 ⇒ continue（不递增下标），否则 Clear + break
                        let (cell, _) = map.get_cell_info_mut(x, y);
                        cell.remove_at(idx);
                        if cell.count() > 0 {
                            continue;
                        }
                        cell.clear();
                        break;
                    }
                    if let Some(other) = store.get(obj.cell_obj_id) {
                        let visible = !other.death
                            && !other.invisible
                            && !other.ghost
                            && !other.fixed_hide_mode
                            && !other.ob_mode
                            && (viewer_master.is_some()
                                || viewer_nasty
                                || viewer_want_ref
                                || (other.master.is_some()
                                    && (other.x - cx).abs() <= 3
                                    && (other.y - cy).abs() <= 3)
                                || other.race == ACTOR_RACE_PLAY);
                        if visible {
                            candidates.push(obj.cell_obj_id);
                        }
                    }
                }
                idx += 1;
                if idx >= len {
                    // 下一轮循环会因越界 break；这里显式收敛，语义与 C# 的 while(true) 一致
                    let (cell, _) = map.get_cell_info(x, y);
                    if idx >= cell.count() {
                        break;
                    }
                }
            }
        }
    }

    for target in candidates {
        update_visible_gay(store, viewer_id, target);
    }

    // 收尾：删除仍为 Hidden 的条目（按下标升序、保序），并记录「消失」事件。
    //
    // oracle `PlayObject.Run()`（`PlayObject.cs:3214-3222`）：
    //   if (VisibleFlag == Hidden) {
    //       if (!baseObject.FixedHideMode && !baseObject.Ghost) SendMsg(baseObject, RM_DISAPPEAR, …);
    //       VisibleActors.RemoveAt(n18); Dispose(visibleBaseObject); continue;
    //   }
    // ⇒ 移除顺序 = 列表下标升序；发帧守卫单独判 `FixedHideMode`/`Ghost`（移除**不**受守卫影响）。
    let mut removed: Vec<i32> = Vec::new();
    if let Some(viewer) = store.get_mut(viewer_id) {
        let mut i = 0;
        while i < viewer.visible_actors.len() {
            if viewer.visible_actors[i].flag == VisibleFlag::Hidden {
                removed.push(viewer.visible_actors[i].target);
                viewer.visible_actors.remove(i);
                continue;
            }
            i += 1;
        }
    }
    removed
        .into_iter()
        .filter(|t| {
            store
                .get(*t)
                .is_some_and(|e| !e.fixed_hide_mode && !e.ghost)
        })
        .collect()
}

/// 对应 `BaseObject.SearchViewRangeDeath`（`BaseObject.ViewRange.cs:159`）。
///
/// 差异：只做格子过期清理（含物品清理分支），最后**整体清空** `VisibleActors`。
pub fn search_view_range_death(
    map: &mut MapGrid,
    store: &mut EntityStore,
    viewer_id: i32,
    now_ms: i64,
    clear_drop_on_floor_item_time: i64,
) {
    let Some(viewer) = store.get(viewer_id) else {
        return;
    };
    if viewer.visible_actors.is_empty() {
        return;
    }
    let (cx, cy, view_range, death, race) = (
        viewer.x,
        viewer.y,
        viewer.view_range,
        viewer.death,
        viewer.race,
    );
    if let Some(viewer) = store.get_mut(viewer_id) {
        viewer.is_visible_active = false;
        for v in viewer.visible_actors.iter_mut() {
            v.flag = VisibleFlag::Hidden;
        }
    }
    for x in (cx - view_range)..=(cx + view_range) {
        for y in (cy - view_range)..=(cy + view_range) {
            let (cell_len, cell_ok) = {
                let (cell, success) = map.get_cell_info(x, y);
                (cell.count(), success)
            };
            if !cell_ok || cell_len == 0 {
                continue;
            }
            let mut i = 0usize;
            loop {
                let (len, obj) = {
                    let (cell, _) = map.get_cell_info(x, y);
                    (cell.count(), cell.obj_list.get(i).copied())
                };
                let Some(obj) = obj else { break };
                if obj.actor_object && now_ms - obj.add_time >= CELL_OBJECT_EXPIRE_MS {
                    let (cell, _) = map.get_cell_info_mut(x, y);
                    cell.remove_at(i);
                    if cell.count() > 0 {
                        continue;
                    }
                    cell.clear();
                    break;
                }
                if obj.cell_type == CellType::Item
                    && !death
                    && race > ACTOR_RACE_MONSTER
                    && now_ms - obj.add_time > clear_drop_on_floor_item_time
                {
                    let (cell, _) = map.get_cell_info_mut(x, y);
                    cell.remove_at(i);
                    if cell.count() > 0 {
                        continue;
                    }
                    cell.clear();
                }
                i += 1;
                if i >= len {
                    let (cell, _) = map.get_cell_info(x, y);
                    if i >= cell.count() {
                        break;
                    }
                }
            }
        }
    }
    if let Some(viewer) = store.get_mut(viewer_id) {
        viewer.visible_actors.clear();
    }
}

/// 便于对拍/断言：把视野列表规范化成 `(target_id, flag)` 序列（保持列表顺序）。
pub fn visible_snapshot(store: &EntityStore, id: i32) -> Vec<(i32, u8)> {
    store
        .get(id)
        .map(|e| {
            e.visible_actors
                .iter()
                .map(|v| (v.target, v.flag as u8))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, EntityStore};

    fn put(map: &mut MapGrid, id: i32, x: i32, y: i32, add_time: i64) {
        assert!(map.add_map_object(x, y, CellType::Play, id, add_time));
    }

    /// **判据④ 牙①**：玩家观察者**不执行** 60s 过期（oracle `PlayObject.cs:3114-3123` 注释块）。
    ///
    /// 格子项已过期（`now - add_time >= 60s`），但 viewer 是玩家 ⇒ 项仍在、仍可见。
    /// 红检：把 `!viewer_is_play &&` 去掉（恢复基类过期）⇒ 本用例必红（格子被清、可见集空）。
    #[test]
    fn player_viewer_skips_cell_expiry() {
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        let mut viewer = Entity::new(1, "p", 10, 10);
        viewer.race = ACTOR_RACE_PLAY;
        viewer.want_ref_msg = true;
        viewer.view_range = 5;
        store.insert(viewer);
        put(&mut map, 1, 10, 10, 60_000);
        let mut target = Entity::new(2, "guard", 11, 10);
        target.race = ACTOR_RACE_MONSTER;
        store.insert(target);
        put(&mut map, 2, 11, 10, 0); // 过期（now - 0 >= 60s）

        let _ = search_view_range(&mut map, &mut store, 1, 60_000);

        assert_eq!(
            map.cell(11, 10).count(),
            1,
            "玩家 viewer 不得删除过期格子项（oracle 注释块）"
        );
        let snap = visible_snapshot(&store, 1);
        assert!(
            snap.contains(&(2, VisibleFlag::Show as u8)),
            "过期项仍应进入玩家可见集：{snap:?}"
        );
    }

    /// 非玩家观察者（怪物/守卫/NPC）**保留**基类的 60s 过期（`BaseObject.ViewRange.cs:95`，
    /// `GuardUnit.cs:155`）。红检：把 `!viewer_is_play` 去掉后玩家分支也会清理 ⇒ 本用例不受影响；
    /// 把过期块整段删掉 ⇒ 本用例红（格子项未被删）。
    #[test]
    fn non_player_viewer_still_expires_cell_objects() {
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        let mut viewer = Entity::new(1, "guard", 10, 10);
        viewer.race = ACTOR_RACE_MONSTER;
        viewer.view_range = 5;
        store.insert(viewer);
        put(&mut map, 1, 10, 10, 60_000);
        let mut target = Entity::new(2, "stale", 11, 10);
        target.race = ACTOR_RACE_MONSTER;
        store.insert(target);
        put(&mut map, 2, 11, 10, 0);

        let _ = search_view_range(&mut map, &mut store, 1, 60_000);

        assert_eq!(map.cell(11, 10).count(), 0, "非玩家 viewer 必须删除过期项");
        assert!(visible_snapshot(&store, 1).is_empty());
    }
}
