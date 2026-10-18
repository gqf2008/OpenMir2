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

/// 对应 `MonsterObject.UpdateMonsterVisible(actorId)`（`MonsterObject.cs:136-185`）。
///
/// oracle 逐行：
/// ```text
/// if ((GetTickCount() - SearchTick) <= SearchTime) return;   // 严格 > 才更新
/// SearchTick = GetTickCount();
/// baseObject = ActorMgr.Get(acrotId);
/// if (baseObject.Race == Play || baseObject.Master != null) IsVisibleActive = true;
/// 已在本列表 ⇒ flag = Invisible；否则追加 Show。
/// ```
///
/// **投递点**：`PlayObject.SearchViewRange` 末段（`PlayObject.cs:3129-3139`）向满足距离门的
/// 怪物发 `RM_UPDATEVIEWRANGE`；怪物在自己的 `BaseObject.Run()` 消息段消费
/// （`MonsterObject.cs:177-185` 的 `Operate`）。本函数 = 消费那一刻的语义。
///
/// `viewer_id` = 消息的 `wParam`（玩家 actor id）。oracle 在 `SearchTick = now` **之后**才
/// `ActorMgr.Get`；目标已消失时 oracle 会 NRE（消息队列仍已被消费）。本骨架在
/// `SearchTick = now` 之后按「目标不存在 ⇒ 丢弃」返回 `false`（只在玩家扫描后、目标于同 tick
/// 或下一 tick 前被移除时可达；不外抛 panic）。
///
/// 返回 `true` = 过了 `SearchTime` 门并执行了更新（含已存在 ⇒ 打 `Invisible`）。
#[must_use]
pub fn update_monster_visible(
    store: &mut EntityStore,
    monster_id: i32,
    viewer_id: i32,
    now_ms: i64,
) -> bool {
    // `if ((HUtil32.GetTickCount() - SearchTick) <= SearchTime) return;`
    {
        let Some(monster) = store.get_mut(monster_id) else {
            return false;
        };
        if now_ms - monster.ai.search_tick <= monster.ai.search_time {
            return false;
        }
        monster.ai.search_tick = now_ms;
    }
    // `IActor baseObject = SystemShare.ActorMgr.Get(acrotId);`
    let Some(target) = store.get(viewer_id) else {
        return false;
    };
    let target_is_active = target.race == ACTOR_RACE_PLAY || target.master.is_some();
    let Some(monster) = store.get_mut(monster_id) else {
        return false;
    };
    if target_is_active {
        monster.is_visible_active = true;
    }
    for v in monster.visible_actors.iter_mut() {
        if v.target == viewer_id {
            v.flag = VisibleFlag::Invisible;
            return true;
        }
    }
    monster.visible_actors.push(VisibleBaseObject {
        target: viewer_id,
        flag: VisibleFlag::Show,
    });
    true
}

/// 一趟 `SearchViewRange` 的完整产出。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchViewRangeOutcome {
    /// 本趟离开视野（`Hidden` ⇒ `RemoveAt`）的目标 id（保序）。
    pub removed: Vec<i32>,
    /// 本趟玩家扫描向怪物投递的 `RM_UPDATEVIEWRANGE` 事件：`(怪物 id, 玩家 id)`（扫描序）。
    ///
    /// oracle 投递点 = `PlayObject.SearchViewRange` 的「进入附近怪物视野」段
    /// （`PlayObject.cs:3129-3139`）；只由玩家观察者产生。
    pub view_updates: Vec<(i32, i32)>,
}

/// [`search_view_range_with_updates`] 的兼容包装：只返回「离开视野」的目标（旧签名）。
///
/// 非玩家自我扫描（卫士/宝宝，`BaseObject.SearchViewRange`/`GuardUnit.SearchViewRange`）
/// 不使用 `view_updates` 面。
#[must_use]
pub fn search_view_range(
    map: &mut MapGrid,
    store: &mut EntityStore,
    viewer_id: i32,
    now_ms: i64,
) -> Vec<i32> {
    search_view_range_with_updates(map, store, viewer_id, now_ms).removed
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
pub fn search_view_range_with_updates(
    map: &mut MapGrid,
    store: &mut EntityStore,
    viewer_id: i32,
    now_ms: i64,
) -> SearchViewRangeOutcome {
    let Some(viewer) = store.get(viewer_id) else {
        return SearchViewRangeOutcome::default();
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
            return SearchViewRangeOutcome::default();
        };
        viewer.is_visible_active = false;
        for v in viewer.visible_actors.iter_mut() {
            v.flag = VisibleFlag::Hidden;
        }
    }

    let mut candidates: Vec<i32> = Vec::new();
    let mut view_updates: Vec<(i32, i32)> = Vec::new();
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
                        // **viewer 分流（oracle 两份 `SearchViewRange` 的过滤器不同）**：
                        // - `PlayObject.SearchViewRange`（`PlayObject.cs:3125-3129`）：
                        //   `!Invisible && !Ghost && !FixedHideMode && !ObMode`，且左项
                        //   `Race < ActorRace.Animal || Master != null || WantRefMsg` —— 玩家
                        //   `Race = 0 < Animal(50)` 恒真；**没有 `Death` 门** ⇒ 尸体仍可见，
                        //   也照走 `RM_UPDATEVIEWRANGE` 投递（`:3130-3139`）与出帧
                        //   （`SM_DEATH`/`SM_SKELETON`，`:3218-3242`）。
                        // - `BaseObject.SearchViewRange`（`BaseObject.ViewRange.cs:105-112`）：
                        //   多一层 `!baseObject.Death`，左项是
                        //   `Master != null || NastyMode || WantRefMsg`。
                        // 其余两支（`baseObject.Master != null && 距离 <= 3`、
                        // `baseObject.Race == Play`）两份相同。
                        let base_pass = viewer_is_play
                            || viewer_master.is_some()
                            || viewer_nasty
                            || viewer_want_ref;
                        let visible = !other.invisible
                            && !other.ghost
                            && !other.fixed_hide_mode
                            && !other.ob_mode
                            && (viewer_is_play || !other.death)
                            && (base_pass
                                || (other.master.is_some()
                                    && (other.x - cx).abs() <= 3
                                    && (other.y - cy).abs() <= 3)
                                || other.race == ACTOR_RACE_PLAY);
                        if visible {
                            candidates.push(obj.cell_obj_id);
                            // **`PlayObject.SearchViewRange` 的「进入附近怪物视野」段**
                            // （`PlayObject.cs:3130-3139`）：
                            // ```text
                            // if (baseObject.CellType == CellType.Monster && !ObMode && !FixedHideMode)
                            //   if (|Δx| <= (ViewRange - baseObject.ViewRange)
                            //    && |Δy| <= (ViewRange - baseObject.ViewRange))
                            //     SendMessage(baseObject.ActorId, RM_UPDATEVIEWRANGE, this.ActorId, …);
                            // ```
                            // `ObMode/FixedHideMode` 已在外层可见性过滤里排除了。`CellType` 是
                            // 格子登记类型（`CellObject.CellType`），不是 `Race`；玩家观察者恒进入
                            // 该段（`Race(0) < ActorRace.Animal(50)`）。两轴都满足才投递。
                            if viewer_is_play && obj.cell_type == CellType::Monster {
                                let span = view_range - other.view_range;
                                if (other.x - cx).abs() <= span && (other.y - cy).abs() <= span {
                                    view_updates.push((obj.cell_obj_id, viewer_id));
                                }
                            }
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
    let removed = removed
        .into_iter()
        .filter(|t| {
            store
                .get(*t)
                .is_some_and(|e| !e.fixed_hide_mode && !e.ghost)
        })
        .collect();
    SearchViewRangeOutcome {
        removed,
        view_updates,
    }
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

    // ---------------------------------------------------------------------------------------
    // 卡 `openmir2-monster-view-throttle`（tier=R）：`RM_UPDATEVIEWRANGE` 投递/消费。
    // oracle：`PlayObject.cs:3129-3139`、`MonsterObject.cs:136-185`。
    // ---------------------------------------------------------------------------------------

    fn put_typed(map: &mut MapGrid, id: i32, x: i32, y: i32, t: CellType, add_time: i64) {
        assert!(map.add_map_object(x, y, t, id, add_time));
    }

    fn play_viewer(id: i32, x: i32, y: i32, view_range: i32) -> Entity {
        let mut e = Entity::new(id, "p", x, y);
        e.race = ACTOR_RACE_PLAY;
        // `PlayObject` ctor `WantRefMsg = true`（`PlayObject.cs:889`）。
        e.want_ref_msg = true;
        e.view_range = view_range;
        e
    }

    fn monster_entity(id: i32, x: i32, y: i32, view_range: i32) -> Entity {
        let mut e = Entity::new(id, "m", x, y);
        e.race = ACTOR_RACE_MONSTER;
        e.view_range = view_range;
        e
    }

    /// **判据③：距离门 `|Δ| <= ViewRange(玩家) - ViewRange(怪)` 的精确边界（差 1 格）**。
    ///
    /// 两轴是**矩形门**（逐轴比较，不是欧氏/Chebyshev 半径）；目标 ViewRange 大于观察者时
    /// 差为负 ⇒ 同格也不投递。
    ///
    /// 牙：把 `<= span` 写成 `< span`（或删掉任一轴）⇒ 7 格通过侧/对角通过侧必红。
    #[test]
    fn player_scan_view_update_distance_gate_exact_boundary() {
        let run = |dx: i32, dy: i32, viewer_range: i32, target_range: i32| {
            let mut map = MapGrid::new(32, 32);
            let mut store = EntityStore::new();
            store.insert(play_viewer(1, 10, 10, viewer_range));
            store.insert(monster_entity(2, 10 + dx, 10 + dy, target_range));
            put_typed(&mut map, 2, 10 + dx, 10 + dy, CellType::Monster, 0);
            search_view_range_with_updates(&mut map, &mut store, 1, 0).view_updates
        };
        // `ViewRange(12) - ViewRange(5) = 7`。
        assert_eq!(run(7, 0, 12, 5), vec![(2, 1)], "x 轴恰好 7 格：通过");
        assert!(run(8, 0, 12, 5).is_empty(), "x 轴 8 格：超出 1 格");
        assert_eq!(run(0, 7, 12, 5), vec![(2, 1)], "y 轴恰好 7 格：通过");
        assert!(run(0, 8, 12, 5).is_empty(), "y 轴 8 格：超出 1 格");
        assert_eq!(run(7, 7, 12, 5), vec![(2, 1)], "两轴同时恰好 7 格：通过");
        assert!(run(8, 7, 12, 5).is_empty(), "任一轴超出：不投递");
        assert!(
            run(0, 0, 5, 12).is_empty(),
            "目标 ViewRange > 观察者 ⇒ span 为负 ⇒ 同格也不投递"
        );
    }

    /// **判据①/②：`SearchTime` 严格大于门 + 已存在项打 `Invisible`**。
    ///
    /// `(now - SearchTick) <= SearchTime` ⇒ 丢弃且**不写** `SearchTick`（oracle return 在赋值前）。
    ///
    /// 牙：去掉门 ⇒ 3000ms 那条必红；把 `<=` 改 `<` ⇒ 3000ms 会错误更新。
    #[test]
    fn monster_view_update_throttle_strictly_greater() {
        let mut store = EntityStore::new();
        store.insert(play_viewer(1, 10, 10, 12));
        let mut m = monster_entity(2, 10, 11, 5);
        m.ai.search_tick = 0;
        m.ai.search_time = 3000;
        store.insert(m);

        assert!(
            !update_monster_visible(&mut store, 2, 1, 3000),
            "3000 - 0 <= 3000 ⇒ 丢弃"
        );
        assert_eq!(store.get(2).unwrap().ai.search_tick, 0);
        assert!(store.get(2).unwrap().visible_actors.is_empty());

        assert!(
            update_monster_visible(&mut store, 2, 1, 3001),
            "3001 - 0 > 3000 ⇒ 更新"
        );
        assert_eq!(store.get(2).unwrap().ai.search_tick, 3001);
        assert_eq!(
            visible_snapshot(&store, 2),
            vec![(1, VisibleFlag::Show as u8)]
        );

        assert!(
            !update_monster_visible(&mut store, 2, 1, 6001),
            "6001 - 3001 <= 3000 ⇒ 丢弃"
        );
        assert_eq!(store.get(2).unwrap().ai.search_tick, 3001);
        assert!(update_monster_visible(&mut store, 2, 1, 6002));
        assert_eq!(store.get(2).unwrap().ai.search_tick, 6002);
        assert_eq!(
            visible_snapshot(&store, 2),
            vec![(1, VisibleFlag::Invisible as u8)],
            "已在列表 ⇒ 只打 Invisible，不重复追加"
        );
    }

    /// **判据④：`IsVisibleActive` 置位条件**（`MonsterObject.cs:145-149`）：
    /// `viewer.Race == Play || viewer.Master != null`。
    #[test]
    fn monster_view_update_sets_visible_active_for_player_and_baby() {
        let mut store = EntityStore::new();
        store.insert(play_viewer(1, 10, 10, 12));
        let mut baby = Entity::new(3, "baby", 10, 11);
        baby.race = ACTOR_RACE_MONSTER;
        baby.master = Some(1);
        store.insert(baby);
        let mut plain = Entity::new(4, "plain", 10, 12);
        plain.race = ACTOR_RACE_MONSTER;
        store.insert(plain);

        for id in 5..=7 {
            let mut m = monster_entity(id, 15, 10, 5);
            m.ai.search_tick = 0;
            m.ai.search_time = 0;
            store.insert(m);
        }
        assert!(update_monster_visible(&mut store, 5, 1, 1));
        assert!(
            store.get(5).unwrap().is_visible_active,
            "玩家 ⇒ IsVisibleActive = true"
        );
        assert!(update_monster_visible(&mut store, 6, 3, 2));
        assert!(
            store.get(6).unwrap().is_visible_active,
            "宝宝（Master != null）⇒ true"
        );
        assert!(update_monster_visible(&mut store, 7, 4, 3));
        assert!(
            !store.get(7).unwrap().is_visible_active,
            "普通怪（无 Master）⇒ false"
        );
    }

    /// **判据⑥：死亡目标仍投递（`PlayObject` 无 `Death` 门），Ghost 目标被过滤器排除。**
    #[test]
    fn player_scan_updates_dead_monster_but_skips_ghost() {
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        store.insert(play_viewer(1, 10, 10, 12));
        let mut dead = monster_entity(2, 11, 10, 5);
        dead.death = true;
        store.insert(dead);
        let mut ghost = monster_entity(3, 9, 10, 5);
        ghost.ghost = true;
        store.insert(ghost);
        put_typed(&mut map, 2, 11, 10, CellType::Monster, 0);
        put_typed(&mut map, 3, 9, 10, CellType::Monster, 0);

        let out = search_view_range_with_updates(&mut map, &mut store, 1, 0);
        assert_eq!(
            out.view_updates,
            vec![(2, 1)],
            "死亡怪收到投递；Ghost 怪被 `!Ghost` 过滤"
        );
        let snap = visible_snapshot(&store, 1);
        assert!(snap.iter().any(|(t, _)| *t == 2), "尸体可见：{snap:?}");
        assert!(!snap.iter().any(|(t, _)| *t == 3), "Ghost 不可见：{snap:?}");
    }

    /// **投递只由玩家观察者产生**：非玩家自扫（`BaseObject.SearchViewRange`）没有
    /// `SendMessage(RM_UPDATEVIEWRANGE, …)` 段（`PlayObject.cs:3130-3139` 仅在 override 里）。
    #[test]
    fn view_range_update_only_emitted_by_player_viewer() {
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        let mut guard = Entity::new(1, "g", 10, 10);
        guard.race = 11;
        guard.view_range = 5;
        guard.want_ref_msg = true;
        store.insert(guard);
        store.insert(monster_entity(2, 11, 10, 5));
        put_typed(&mut map, 2, 11, 10, CellType::Monster, 0);

        let out = search_view_range_with_updates(&mut map, &mut store, 1, 0);
        assert!(out.view_updates.is_empty(), "非玩家观察者不投递");
        assert!(
            visible_snapshot(&store, 1).iter().any(|(t, _)| *t == 2),
            "但其自身可见集照常更新"
        );
    }

    /// oracle 在 `SearchTick = now` **之后**才 `ActorMgr.Get`：目标已消失时 `SearchTick`
    /// 仍被推进（消息已消费），只是不产生可见项。本骨架不抛 NRE，按丢弃处理。
    #[test]
    fn update_with_missing_viewer_still_refreshes_search_tick() {
        let mut store = EntityStore::new();
        let mut m = monster_entity(2, 10, 11, 5);
        m.ai.search_tick = 0;
        m.ai.search_time = 0;
        store.insert(m);
        assert!(!update_monster_visible(&mut store, 2, 999, 42));
        assert_eq!(
            store.get(2).unwrap().ai.search_tick,
            42,
            "oracle 先写 SearchTick 再 ActorMgr.Get"
        );
        assert!(store.get(2).unwrap().visible_actors.is_empty());
    }
}
