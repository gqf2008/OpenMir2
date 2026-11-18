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
    AiState, AnimalState, EntityRef, EntityStore, MapBinding, Transform, VisibleBaseObject,
    VisibleEvent, VisibleFlag, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY,
};
use crate::map::{CellType, MapGrid};
use crate::MapsRes;
use bevy_ecs::prelude::Resource;

/// 格子内对象过期阈值（`BaseObject.ViewRange.cs:95`：`60 * 1000` 毫秒）。
pub const CELL_OBJECT_EXPIRE_MS: i64 = 60 * 1000;

/// AOI 扫描所需的实体访问面（S3b）。
///
/// 两个实现：`crates/world-bevy` 的 [`EntityStore`]（facade/测试路径）与 bevy `World` 适配器
/// [`BevyAoiStore`]（`TickStage::OutputAoi` 系统路径）。扫描算法只依赖这三个方法，
/// 因此系统化不复制算法、也不引入第二份真值。
pub trait AoiStore {
    fn get(&self, id: i32) -> Option<EntityRef<'_>>;
    fn ai_mut(&mut self, id: i32) -> Option<&mut AnimalState>;
    fn map_binding_mut(&mut self, id: i32) -> Option<&mut MapBinding>;
}

impl AoiStore for EntityStore {
    fn get(&self, id: i32) -> Option<EntityRef<'_>> {
        EntityStore::get(self, id)
    }
    fn ai_mut(&mut self, id: i32) -> Option<&mut AnimalState> {
        EntityStore::ai_mut(self, id)
    }
    fn map_binding_mut(&mut self, id: i32) -> Option<&mut MapBinding> {
        EntityStore::map_binding_mut(self, id)
    }
}

/// 对应 `UpdateVisibleGay`（`BaseObject.ViewRange.cs:12`）。
pub fn update_visible_gay<S: AoiStore>(store: &mut S, viewer_id: i32, target_id: i32) {
    let Some(target) = store.get(target_id) else {
        return;
    };
    let target_is_active = target.race == ACTOR_RACE_PLAY || target.master.is_some();
    let Some(viewer) = store.map_binding_mut(viewer_id) else {
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
pub fn update_monster_visible<S: AoiStore>(
    store: &mut S,
    monster_id: i32,
    viewer_id: i32,
    now_ms: i64,
) -> bool {
    // `if ((HUtil32.GetTickCount() - SearchTick) <= SearchTime) return;`
    {
        let Some(ai) = store.ai_mut(monster_id) else {
            return false;
        };
        if now_ms - ai.search_tick <= ai.search_time {
            return false;
        }
        ai.search_tick = now_ms;
    }
    // `IActor baseObject = SystemShare.ActorMgr.Get(acrotId);`
    let Some(target) = store.get(viewer_id) else {
        return false;
    };
    let target_is_active = target.race == ACTOR_RACE_PLAY || target.master.is_some();
    let Some(monster) = store.map_binding_mut(monster_id) else {
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
pub fn search_view_range<S: AoiStore>(
    map: &mut MapGrid,
    store: &mut S,
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
pub fn search_view_range_with_updates<S: AoiStore>(
    map: &mut MapGrid,
    store: &mut S,
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
        let Some(viewer) = store.map_binding_mut(viewer_id) else {
            return SearchViewRangeOutcome::default();
        };
        viewer.is_visible_active = false;
        for v in viewer.visible_actors.iter_mut() {
            v.flag = VisibleFlag::Hidden;
        }
    }

    // 遥测（默认关闭）：起点覆盖「扫描 + UpdateVisibleGay + 差集收尾」。
    let stats_start = aoi_stats_search_begin();
    let stats_cells = if stats_start.is_some() {
        let side = i64::from(view_range) * 2 + 1;
        u64::try_from(side).map_or(0, |s| s.saturating_mul(s))
    } else {
        0
    };
    let mut candidates: Vec<i32> = Vec::new();
    #[cfg(test)]
    let capture_universe = aoi_universe_capture_enabled();
    #[cfg(test)]
    if capture_universe {
        AOI_LAST_SCAN_UNIVERSE.with(|c| c.borrow_mut().clear());
    }
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
                    #[cfg(test)]
                    if capture_universe {
                        AOI_LAST_SCAN_UNIVERSE.with(|c| c.borrow_mut().push(obj.cell_obj_id));
                    }
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

    let stats_candidates = if stats_start.is_some() {
        candidates.len() as u64
    } else {
        0
    };
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
    if let Some(viewer) = store.map_binding_mut(viewer_id) {
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
    let removed: Vec<i32> = removed
        .into_iter()
        .filter(|t| {
            store
                .get(*t)
                .is_some_and(|e| !e.fixed_hide_mode && !e.ghost)
        })
        .collect();
    if let Some(start) = stats_start {
        let visible = store
            .get(viewer_id)
            .map_or(0, |e| e.visible_actors.len() as u64);
        aoi_stats_record_search(
            start,
            stats_cells,
            stats_candidates,
            visible,
            removed.len() as u64,
        );
    }
    SearchViewRangeOutcome {
        removed,
        view_updates,
    }
}

/// 对应 `PlayObject.UpdateVisibleEvent`（`PlayObject.cs:5464-5486`）。
///
/// oracle 逐行：
/// ```text
/// for (i = 0; i < VisibleEvents.Count; i++)
///     if (VisibleEvents[i] == MapEvent) { VisibleEvents[i].VisibleFlag = Invisible; return; }
/// MapEvent.VisibleFlag = Show; MapEvent.nX = wX; MapEvent.nY = wY; VisibleEvents.Add(MapEvent);
/// ```
///
/// **已在本列表 ⇒ 只打 `Invisible`、不更新坐标**（早退支不写 ⇒ 同一趟后续格子不覆写）。
///
/// **review `39262b65` F3**：`nX/nY` 是**折叠共享对象**的字段——首见支先写共享镜像 `sight`
/// （oracle `PlayObject.cs:5483-5484`，每个观察者首见都覆写同一份），再把该值快照进观察者
/// 条目。快照只服务 **804**（同一趟扫描内写读无间隔 ⇒ 与共享值恒相等）；**805** 读的是发送
/// 时刻的共享值（`:3278`）——它可能已被**后见者**覆写 ⇒ 由 [`search_view_range_events`]
/// 的收尾支读 `sight`，不读条目快照。
pub fn update_visible_event<S: AoiStore>(
    store: &mut S,
    viewer_id: i32,
    event: usize,
    x: i32,
    y: i32,
    sight: &mut (i32, i32),
) {
    let Some(viewer) = store.map_binding_mut(viewer_id) else {
        return;
    };
    for v in viewer.visible_events.iter_mut() {
        if v.event == event {
            v.flag = VisibleFlag::Invisible;
            return;
        }
    }
    // oracle `:5483-5484`：`MapEvent.VisibleFlag = Show; MapEvent.nX = wX; MapEvent.nY = wY;`
    *sight = (x, y);
    viewer.visible_events.push(VisibleEvent {
        event,
        x,
        y,
        flag: VisibleFlag::Show,
    });
}

/// `PlayObject.SearchViewRange` 的 **`CellType.Event` 支**（卡 `openmir2-map-event-frames`）。
///
/// 扫描段（`PlayObject.cs:3186-3194`）：
/// ```text
/// case CellType.Event:
///     MapEvent mapEvent = M2Share.CellObjectMgr.Get<MapEvent>(cellObject.CellObjId); // 恒 0
///     if (mapEvent == null) continue;
///     if (mapEvent.Visible) UpdateVisibleEvent(nX, nY, mapEvent);
/// ```
/// 差集收尾（`:3271-3287`）：`Hidden` ⇒ `SendMsg(RM_HIDEEVENT, 0, mapEvent.Id, mapEvent.nX,
/// mapEvent.nY)` 并 `RemoveAt`；`Show` ⇒ `SendMsg(RM_SHOWEVENT, …)`（帧面由宿主出）。
///
/// 与 actors 的合并实现拆开的**等价性**：oracle 把两段写在同一格子双层循环里，但事件只写
/// 观察者自己的 `VisibleEvents`、actors 只写 `VisibleActors`，互不读写 ⇒ 拆两趟（同一窗口、
/// 同一「X 外层升序、Y 内层升序」序）结果逐字段相同。
///
/// **只有玩家**走这段：`BaseObject.SearchViewRange` / `GuardUnit.SearchViewRange`
/// （`BaseObject.ViewRange.cs`）**没有** `CellType.Event` 分支——`VisibleEvents` 是
/// `PlayObject` 独有字段（`PlayObject.cs:211`）。非玩家调用 ⇒ 空返回。
///
/// **review `39262b65` F3**：`sight` = 折叠对象共享 `nX/nY` 的镜像（`World::event_sight`）。
/// 首见支覆写它；差集收尾的 `RM_HIDEEVENT` 记录读**发送时刻的共享值**（oracle `:3278`）——
/// 即「最后一次首见」的格子，**不是**本观察者条目里的 add 时刻快照（后见者可能已覆写）。
///
/// **`Id == 0` 折叠（1:1 硬约束）**：`events` 里的查找按 oracle `Get<MapEvent>(0)` 的
/// `TryAdd` 语义取**全进程第一条 `registered` 事件**（key 0 第一条胜、`DeleteFromMap` 对
/// id 0 恒不移除 ⇒ 关闭后**不再替换**）；与格子坐标无关（oracle `CellObjectMgr` 是
/// `M2Share` 的静态全局，跨图共享）。给事件编唯一 id ⇒ 每个事件格各解析各的 ⇒ 折叠用例必红。
///
/// 返回本趟「离开视野」的 `(事件下标, nX, nY)`（`Hidden` ⇒ `RemoveAt`，下标升序保序）——
/// 对应 oracle `RM_HIDEEVENT` 的发送对象。
pub fn search_view_range_events<S: AoiStore>(
    map: &MapGrid,
    store: &mut S,
    viewer_id: i32,
    events: &[crate::world::FireBurnEvent],
    sight: &mut (i32, i32),
) -> Vec<(usize, i32, i32)> {
    let Some(viewer) = store.get(viewer_id) else {
        return Vec::new();
    };
    if viewer.race != ACTOR_RACE_PLAY {
        return Vec::new();
    }
    let (cx, cy, view_range) = (viewer.x, viewer.y, viewer.view_range);
    // 置 Hidden（oracle `:3078-3082`：`for (i = Count - 1; i >= 0; i--) … = Hidden;`——
    // 逆序置位与顺序无关，逐条目结果相同）。
    if let Some(viewer) = store.map_binding_mut(viewer_id) {
        for v in viewer.visible_events.iter_mut() {
            v.flag = VisibleFlag::Hidden;
        }
    }
    // 扫描窗口与 actors 同一（`CurrX ± ViewRange` 的正方形，X 外层升序、Y 内层升序）。
    for x in (cx - view_range)..=(cx + view_range) {
        for y in (cy - view_range)..=(cy + view_range) {
            let (cell, success) = map.get_cell_info(x, y);
            if !success || cell.count() == 0 {
                continue; // C#：cellSuccess && cellInfo.IsAvailable
            }
            for obj in &cell.obj_list {
                if obj.cell_type != CellType::Event {
                    continue;
                }
                // `Get<MapEvent>(0)`：Id==0 折叠——任何事件格都解析到**第一条** registered 事件。
                let Some((event_idx, folded)) =
                    events.iter().enumerate().find(|(_, e)| e.registered)
                else {
                    continue; // oracle `mapEvent == null ⇒ continue`（key 0 还空着）
                };
                if !folded.visible {
                    continue; // oracle `if (mapEvent.Visible)`
                }
                update_visible_event(store, viewer_id, event_idx, x, y, sight);
            }
        }
    }
    // 差集收尾（oracle `:3271-3287`）：`Hidden` ⇒ 记移除（宿主发 `SM_HIDEEVENT`）、下标升序保序。
    // review `39262b65` F3：805 坐标 = **发送时刻共享值**（oracle `:3278` 读 `mapEvent.nX/nY`）
    // —— 本趟无首见支（Hidden 才会移除）⇒ sight 未被本趟覆写，即「最后一次首见」的格子。
    let mut removed: Vec<(usize, i32, i32)> = Vec::new();
    if let Some(viewer) = store.map_binding_mut(viewer_id) {
        let mut i = 0;
        while i < viewer.visible_events.len() {
            if viewer.visible_events[i].flag == VisibleFlag::Hidden {
                let ev = viewer.visible_events.remove(i);
                removed.push((ev.event, sight.0, sight.1));
                continue;
            }
            i += 1;
        }
    }
    removed
}

/// 对应 `BaseObject.SearchViewRangeDeath`（`BaseObject.ViewRange.cs:159`）。
///
/// 差异：只做格子过期清理（含物品清理分支），最后**整体清空** `VisibleActors`。
pub fn search_view_range_death<S: AoiStore>(
    map: &mut MapGrid,
    store: &mut S,
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
    if let Some(viewer) = store.map_binding_mut(viewer_id) {
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
    if let Some(viewer) = store.map_binding_mut(viewer_id) {
        viewer.visible_actors.clear();
    }
}

/// 便于对拍/断言：把视野列表规范化成 `(target_id, flag)` 序列（保持列表顺序）。
pub fn visible_snapshot<S: AoiStore>(store: &S, id: i32) -> Vec<(i32, u8)> {
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

// ===================== AOI 搜索遥测（卡 `openmir2-aoi-incremental` phase 1） =====================
//
// **零行为变更**：只读局部量并累加到线程局部状态；不新增/删除/重排 `visible_actors`，
// 不改 `aoi_order`、不改 `SearchTime` 门。
//
// **开关（默认关闭）**：只有 `MIR2_AOI_STATS=1` 开启；未设 / `0` / 空串 / 其它任何值一律关闭。
// 关闭态每次搜索只做一次 `std::sync::OnceLock` 读取判别：**不取 `Instant`、不写线程局部、
// 不打印**。开启态每趟搜索取两次 `Instant`（起/止），把 5 个计数 + 两组 log2 直方图累加进
// 线程局部；`output_aoi_system` 每 [`AOI_STATS_REPORT_TICKS`] tick 向 stderr 打一行
// `[AOI_STATS]`（字段名固定，供报告机读）。
//
// **字段口径**：
// - `searches`：本窗口完成扫描的 `search_view_range_with_updates` 次数（viewer 不存在的
//   早期返回不计——没有发生扫描）；
// - `cells_per_search`：每趟正方形窗口的格数 `(2*ViewRange+1)^2`（实际循环访问数）；
// - `cand_per_search`：每趟通过可见性过滤的候选数（`candidates.len()`）；
// - `vis_per_search`：本趟结束时 `visible_actors.len()` = 新加入 + 既有保持；
// - `ns_p50/ns_p90/ns_max`：每趟「扫描 + UpdateVisibleGay + 差集收尾」的墙钟 ns，分位为
//   log2 直方图近似（桶 `k` 覆盖 `[2^(k-1), 2^k - 1]`，打印桶上限 `2^k`）；
// - `same_map_p50/same_map_p90/same_map_max`：每 tick `by_map` 各图**实体总数**（全部实体，
//   不只观察者）的分布，只统计实体数 > 0 的图（空图会把分位数退化到 0）。
//
// 另累计 `removed`（每趟离开视野的条数，取返回值长度——已经过 `FixedHideMode`/`Ghost`
// 守卫）：卡要求记录，但行内字段固定，不打印（测试经快照读取）。

/// 遥测环境门（**只有字面 `"1"` 开启**；默认关闭）。
pub const ENV_AOI_STATS: &str = "MIR2_AOI_STATS";

/// 汇总窗口默认长度：每 500 tick 打印一行。
pub const AOI_STATS_REPORT_TICKS: u64 = 500;

/// 汇总窗口的**环境变量覆盖**（正整数；缺失/非法一律回落 [`AOI_STATS_REPORT_TICKS`]）。
/// 用途：360 s 跑批在 200 ms/tick 下默认只有 2 行，做不了时间序列；设小值（如 150 ≈ 30 s）
/// 即可按窗口拿到序列。只在开启态解析一次（`OnceLock`），关闭态零成本。
pub const ENV_AOI_STATS_TICKS: &str = "MIR2_AOI_STATS_TICKS";

/// 解析规则（纯函数，便于单测）：缺省/非法/0 ⇒ 默认值。
fn parse_report_ticks(value: Option<&str>) -> u64 {
    match value {
        Some(v) => v
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|n| *n > 0)
            .unwrap_or(AOI_STATS_REPORT_TICKS),
        None => AOI_STATS_REPORT_TICKS,
    }
}

/// 本次进程实际使用的窗口长度（首次调用读一次环境变量）。
#[must_use]
pub fn aoi_stats_report_ticks() -> u64 {
    static OVERRIDE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *OVERRIDE.get_or_init(|| parse_report_ticks(std::env::var(ENV_AOI_STATS_TICKS).ok().as_deref()))
}

/// log2 直方图桶数（`u64` 值域：0 桶 + 1..=64）。
const AOI_HIST_BUCKETS: usize = 65;

/// 值 → log2 直方图桶：`0 => 0`，`v > 0 => floor(log2(v)) + 1`（= `v` 的位宽）。
///
/// 桶 `k >= 1` 覆盖 `[2^(k-1), 2^k - 1]`；[`aoi_hist_upper_bound`] 返回桶上限 `2^k`。
fn aoi_hist_bucket(v: u64) -> usize {
    if v == 0 {
        0
    } else {
        (u64::BITS - v.leading_zeros()) as usize
    }
}

fn aoi_hist_upper_bound(bucket: usize) -> u64 {
    if bucket == 0 {
        0
    } else {
        1u64.checked_shl(bucket as u32).unwrap_or(u64::MAX)
    }
}

/// **ns 直方图：线性桶**（每桶 32 µs；0..=63 覆盖 0–2048 µs，第 64 桶 = ≥2048 µs 溢出）。
///
/// 为什么不用 log2：报告出来会全是 2 的幂边界（131,072 / 524,288 ns），分辨率只有 ±2 倍，
/// 两轮"完全相同"其实只是"落在同一个大桶"，支撑不了收益估算（2026-11-19 AOI 两轮复盘）。
pub const AOI_NS_BUCKET_NS: u64 = 32_000;
/// ns 直方图桶数：64 个线性桶 + 1 个溢出桶。
const AOI_NS_BUCKETS: usize = 65;

fn aoi_ns_bucket(ns: u64) -> usize {
    let idx = ns / AOI_NS_BUCKET_NS;
    usize::try_from(idx)
        .unwrap_or(AOI_NS_BUCKETS - 1)
        .min(AOI_NS_BUCKETS - 1)
}

/// 线性桶的上限（ns）；溢出桶返回 `None`（调用方改用 `ns_max`）。
fn aoi_ns_bucket_upper(bucket: usize) -> Option<u64> {
    if bucket >= AOI_NS_BUCKETS - 1 {
        None
    } else {
        Some((bucket as u64 + 1) * AOI_NS_BUCKET_NS)
    }
}

/// 通用分位：返回覆盖目标分位的最小桶经 `upper` 映射后的值。
fn aoi_percentile(hist: &[u64], pct: u32, upper: impl Fn(usize) -> u64) -> Option<u64> {
    let total: u64 = hist.iter().sum();
    if total == 0 {
        return None;
    }
    let target = (u128::from(total) * u128::from(pct)).div_ceil(100) as u64;
    let mut cumulative = 0u64;
    for (bucket, &count) in hist.iter().enumerate() {
        cumulative = cumulative.saturating_add(count);
        if cumulative >= target {
            return Some(upper(bucket));
        }
    }
    hist.len().checked_sub(1).map(upper)
}

/// `same_map` 用 log2 桶（实体计数跨几个数量级）；分位语义见 [`aoi_percentile`]。
/// 直方图近似分位数：返回覆盖目标分位的最小桶的上限；空直方图 => `None`。
fn aoi_hist_percentile(hist: &[u64; AOI_HIST_BUCKETS], pct: u32) -> Option<u64> {
    aoi_percentile(hist, pct, aoi_hist_upper_bound)
}

/// 本线程一个汇总窗口的累计（**关闭态从不触碰**）。
#[derive(Clone)]
struct AoiStatsState {
    window_ticks: u64,
    /// 本窗口完成的搜索次数（旧字段名 `searches`，行内改名 `searches_in_window`）。
    searches: u64,
    /// **跨窗口累计**的搜索次数（行内 `searches_total`）。与 `searches` 的差值是
    /// 「相邻两行 in_window 之和 == total 增量」这条自洽判据的依据。
    total_searches: u64,
    /// 已输出的窗口序号（1 起；行内 `window_seq`）。
    window_seq: u64,
    /// 累计 tick 序号（1 起；行内 `tick_seq`）——分布要能对齐到 ramp/稳态，光有墙钟不够。
    tick_seq: u64,
    cells: u64,
    candidates: u64,
    visible: u64,
    /// 卡要求记录（行格式固定不打印）；见模块注释。
    #[allow(dead_code)]
    removed: u64,
    ns_max: u64,
    /// ns 样本总和（`ns_avg` 用；桶类指标一律配均值）。
    ns_sum: u64,
    same_map_max: u64,
    /// 非空地图的实体数之和 / 计数 ⇒ `same_map_avg`（桶类分位对一档跳变过敏，均值更稳）。
    same_map_sum: u64,
    same_map_count: u64,
    ns_hist: [u64; AOI_NS_BUCKETS],
    same_map_hist: [u64; AOI_HIST_BUCKETS],
}

impl AoiStatsState {
    const fn new() -> Self {
        Self {
            window_ticks: 0,
            searches: 0,
            total_searches: 0,
            window_seq: 0,
            tick_seq: 0,
            cells: 0,
            candidates: 0,
            visible: 0,
            removed: 0,
            ns_max: 0,
            ns_sum: 0,
            same_map_max: 0,
            same_map_sum: 0,
            same_map_count: 0,
            ns_hist: [0; AOI_NS_BUCKETS],
            same_map_hist: [0; AOI_HIST_BUCKETS],
        }
    }

    /// 窗口边界重置：**只清窗口量**，累计量（`total_searches` / `window_seq` / `tick_seq`）保留。
    fn reset_window(&mut self) {
        let total_searches = self.total_searches;
        let window_seq = self.window_seq;
        let tick_seq = self.tick_seq;
        *self = Self::new();
        self.total_searches = total_searches;
        self.window_seq = window_seq;
        self.tick_seq = tick_seq;
    }

    /// 取一行报告；字段顺序/名称固定（报告机读）。
    ///
    /// 口径（报告必须照抄）：
    /// - `clock_ms` = 世界时钟（`AoiParams.clock_ms`，同一时刻量）；
    /// - `window_seq` / `tick_seq` = **本线程**已输出窗口序号 / 累计 tick 序号（均 1 起）；
    /// - `ticks` = 本窗口 tick 数（正常 = `AOI_STATS_REPORT_TICKS`）；
    /// - `searches_in_window` = 本窗口搜索次数；`searches_total` = 本线程**跨窗口累计**（含本窗口）；
    /// - 自洽判据：相邻两行 `searches_in_window` 之和 == 后一行的 `searches_total` 增量。
    fn report_line(&self, clock_ms: i64, final_line: bool) -> String {
        let searches = self.searches;
        let ticks = self.window_ticks;
        let per_search = |n: u64| {
            if searches == 0 {
                0.0
            } else {
                n as f64 / searches as f64
            }
        };
        let per_tick = if ticks == 0 {
            0.0
        } else {
            searches as f64 / ticks as f64
        };
        let same_map_pct =
            |hist: &[u64; AOI_HIST_BUCKETS], pct: u32| aoi_hist_percentile(hist, pct).unwrap_or(0);
        let ns_max = self.ns_max;
        let ns_pct = |hist: &[u64; AOI_NS_BUCKETS], pct: u32| {
            aoi_percentile(hist, pct, |b| aoi_ns_bucket_upper(b).unwrap_or(ns_max)).unwrap_or(0)
        };
        let ns_avg = if self.searches == 0 {
            0.0
        } else {
            self.ns_sum as f64 / self.searches as f64
        };
        let final_marker = if final_line { "final=1 " } else { "" };
        format!(
            "[AOI_STATS] {final_marker}clock_ms={clock_ms} window_seq={} tick_seq={} ticks={ticks} searches_in_window={searches} searches_total={} searches_per_tick={per_tick:.2} cells_per_search={:.2} cand_per_search={:.2} vis_per_search={:.2} ns_bucket_ns={AOI_NS_BUCKET_NS} ns_avg={ns_avg:.1} ns_p50={} ns_p90={} ns_max={} same_map_p50={} same_map_p90={} same_map_avg={:.2} same_map_max={}",
            self.window_seq,
            self.tick_seq,
            self.total_searches,
            per_search(self.cells),
            per_search(self.candidates),
            per_search(self.visible),
            ns_pct(&self.ns_hist, 50),
            ns_pct(&self.ns_hist, 90),
            self.ns_max,
            same_map_pct(&self.same_map_hist, 50),
            same_map_pct(&self.same_map_hist, 90),
            if self.same_map_count == 0 {
                0.0
            } else {
                self.same_map_sum as f64 / self.same_map_count as f64
            },
            self.same_map_max,
        )
    }
}

/// 环境门的进程级缓存（首次调用时读一次 `MIR2_AOI_STATS`）。
static AOI_STATS_ENV_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

thread_local! {
    /// 本线程窗口累计（关闭态不触碰）。
    static AOI_STATS_STATE: std::cell::RefCell<AoiStatsState> =
        const { std::cell::RefCell::new(AoiStatsState::new()) };
    /// **测试专用**开关覆盖（`None` = 走环境门缓存）。
    #[cfg(test)]
    static AOI_STATS_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
    /// **测试专用**时钟调用计数（证明关闭态根本没取 `Instant`）。
    #[cfg(test)]
    static AOI_STATS_CLOCK_CALLS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
thread_local! {
    /// 测试专用：最近一次扫描实际遍历过的 `cell_obj_id`（**过滤前**的候选全集）。
    static AOI_LAST_SCAN_UNIVERSE: std::cell::RefCell<Vec<i32>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// 测试专用：是否开启 universe 捕获（关闭态零开销）。
    static AOI_UNIVERSE_CAPTURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn aoi_universe_capture_enabled() -> bool {
    AOI_UNIVERSE_CAPTURE.with(|c| c.get())
}

/// 测试专用：打开/关闭 universe 捕获。
#[cfg(test)]
pub(crate) fn aoi_universe_capture_set(enabled: bool) {
    AOI_UNIVERSE_CAPTURE.with(|c| c.set(enabled));
}

/// 测试专用：读最近一次扫描的候选全集（**未排序**，按遍历顺序）。
#[cfg(test)]
pub(crate) fn aoi_last_scan_universe() -> Vec<i32> {
    AOI_LAST_SCAN_UNIVERSE.with(|c| c.borrow().clone())
}

/// 环境值口径：只有先去空白后等于 `"1"` 才开启。
fn aoi_stats_env_enabled(value: Option<&str>) -> bool {
    matches!(value, Some(v) if v.trim() == "1")
}

/// 开关判断（**最外层**，每次搜索/tick 先过这里）：关闭态不取时钟、不写线程局部。
#[inline]
fn aoi_stats_enabled() -> bool {
    #[cfg(test)]
    {
        if let Some(overridden) = AOI_STATS_TEST_OVERRIDE.with(|cell| cell.get()) {
            return overridden;
        }
    }
    *AOI_STATS_ENV_ENABLED
        .get_or_init(|| aoi_stats_env_enabled(std::env::var(ENV_AOI_STATS).ok().as_deref()))
}

/// 遥测路径上唯一的时钟入口（测试构建里顺带数调用次数）。
#[inline]
fn aoi_stats_clock_now() -> std::time::Instant {
    #[cfg(test)]
    {
        AOI_STATS_CLOCK_CALLS.with(|cell| cell.set(cell.get().wrapping_add(1)));
    }
    std::time::Instant::now()
}

/// 每趟搜索起点；关闭态 `None`（**不取 `Instant`**）。
#[inline]
fn aoi_stats_search_begin() -> Option<std::time::Instant> {
    aoi_stats_enabled().then(aoi_stats_clock_now)
}

/// 记一趟搜索（只在起点为 `Some` 时调用）。
fn aoi_stats_record_search(
    start: std::time::Instant,
    cells: u64,
    candidates: u64,
    visible: u64,
    removed: u64,
) {
    // 终点也走计数包装：每趟搜索恰好两次时钟调用（起/止）。
    let ns = aoi_stats_clock_now()
        .duration_since(start)
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64;
    AOI_STATS_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.searches = state.searches.saturating_add(1);
        state.cells = state.cells.saturating_add(cells);
        state.candidates = state.candidates.saturating_add(candidates);
        state.visible = state.visible.saturating_add(visible);
        state.removed = state.removed.saturating_add(removed);
        state.ns_max = state.ns_max.max(ns);
        state.ns_sum = state.ns_sum.saturating_add(ns);
        let bucket = aoi_ns_bucket(ns);
        state.ns_hist[bucket] = state.ns_hist[bucket].saturating_add(1);
    });
}

/// **进程退出时的终值行**（opc-07 要求）：窗口未满也强制出一行，`final=1` 标记。
///
/// 线程局部 ⇒ 必须由**世界线程**在退出前调用；其它线程调用只会看到自己的空窗口（返回 `None`）。
#[must_use]
pub fn aoi_stats_final_line(clock_ms: i64) -> Option<String> {
    if !aoi_stats_enabled() {
        return None;
    }
    AOI_STATS_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.window_ticks == 0 {
            return None;
        }
        state.window_seq = state.window_seq.saturating_add(1);
        state.total_searches = state.total_searches.saturating_add(state.searches);
        let line = state.report_line(clock_ms, true);
        state.reset_window();
        Some(line)
    })
}

/// 每个 `OutputAoi` tick 调用一次：累计分布；到窗口边界返回一行待打印文本并清零。
/// 关闭态直接返回 `None`（**不写线程局部、不消费迭代器**）。
fn aoi_stats_tick(clock_ms: i64, map_entity_counts: impl Iterator<Item = u64>) -> Option<String> {
    if !aoi_stats_enabled() {
        return None;
    }
    let report_ticks = aoi_stats_report_ticks();
    let mut line = None;
    AOI_STATS_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.window_ticks = state.window_ticks.saturating_add(1);
        state.tick_seq = state.tick_seq.saturating_add(1);
        for count in map_entity_counts {
            if count == 0 {
                continue; // 空图无信息量，不进入分布
            }
            state.same_map_sum = state.same_map_sum.saturating_add(count);
            state.same_map_count = state.same_map_count.saturating_add(1);
            let bucket = aoi_hist_bucket(count);
            state.same_map_hist[bucket] = state.same_map_hist[bucket].saturating_add(1);
            state.same_map_max = state.same_map_max.max(count);
        }
        if state.window_ticks >= report_ticks {
            state.window_seq = state.window_seq.saturating_add(1);
            // 先把本窗口计入累计，再出行（行里的 total 含本窗口）。
            state.total_searches = state.total_searches.saturating_add(state.searches);
            line = Some(state.report_line(clock_ms, false));
            state.reset_window();
        }
    });
    line
}

#[cfg(test)]
fn aoi_stats_reset_for_test() {
    AOI_STATS_STATE.with(|cell| *cell.borrow_mut() = AoiStatsState::new());
    AOI_STATS_CLOCK_CALLS.with(|cell| cell.set(0));
}

#[cfg(test)]
fn aoi_stats_snapshot_for_test() -> AoiStatsState {
    AOI_STATS_STATE.with(|cell| cell.borrow().clone())
}

#[cfg(test)]
fn aoi_stats_clock_calls_for_test() -> u64 {
    AOI_STATS_CLOCK_CALLS.with(|cell| cell.get())
}

/// RAII 测试开关：`Drop` 时恢复 `None` 并清零，保证同线程后续测试不受影响。
#[cfg(test)]
struct AoiStatsTestGuard;

#[cfg(test)]
impl AoiStatsTestGuard {
    fn enabled() -> Self {
        Self::set(true)
    }

    fn disabled() -> Self {
        Self::set(false)
    }

    fn set(enabled: bool) -> Self {
        AOI_STATS_TEST_OVERRIDE.with(|cell| cell.set(Some(enabled)));
        aoi_stats_reset_for_test();
        Self
    }
}

#[cfg(test)]
impl Drop for AoiStatsTestGuard {
    fn drop(&mut self) {
        AOI_STATS_TEST_OVERRIDE.with(|cell| cell.set(None));
        aoi_stats_reset_for_test();
    }
}

// ===================== AOI 增量变更馈送（phase 2 契约，卡 openmir2-aoi-incremental） =====================
//
// 增量可见集维护需要**变更事件**：实体跨格、进入/离开世界（或换图）。本结构是 tick 边界上
// 「移动/生命周期系统 → AOI 索引」的唯一接缝，**第一步只定契约**（保守馈送：本 tick 位置或
// 存在性变过的实体全进 feed），不做批量化/缓存优化——语义先拿证，优化另起提交。
//
// 注意：`moved` 只记**跨格**（格坐标变化）；同格内微动不影响可见关系，不产生条目。

/// 实体的格坐标（地图格，不是像素坐标）。
pub type AoiCell = (i32, i32);

/// tick 边界的位置/存在性快照：`id -> 格坐标`（**只收参与世界/存活的实体**）。
///
/// 用 `BTreeMap` 而不是 `HashMap`：迭代顺序确定 ⇒ 派生出的 feed 顺序也确定（差分可复现）。
pub type AoiCellSnapshot = std::collections::BTreeMap<i32, AoiCell>;

/// 与 legacy 扫描**同源**的世界快照：遍历地图格子里所有 `actor_object` 对象
/// （Ghost / 刚死未清 也在内——它们同样是 legacy 候选集的一部分）。
///
/// **对齐判据**（卡 `openmir2-aoi-incremental`，opc-07 要求）：派生 feed 的输入全集必须与
/// legacy 每 tick 实际遍历的候选集相等；否则两侧在解不同的问题，差分绿了也不证明等价。
#[must_use]
pub fn aoi_snapshot_from_map(map: &MapGrid) -> AoiCellSnapshot {
    let mut snap = AoiCellSnapshot::new();
    for x in 0..map.width {
        for y in 0..map.height {
            let (cell, ok) = map.get_cell_info(x, y);
            if !ok {
                continue;
            }
            for obj in &cell.obj_list {
                if obj.actor_object {
                    snap.insert(obj.cell_obj_id, (x, y));
                }
            }
        }
    }
    snap
}

/// **派生 feed**（opc-07 定稿的第一步口径）：由相邻两个 tick 的快照比对得出，
/// 完备性**由构造保证**——不依赖任何调用方"记得喂事件"。
///
/// 语义：
/// - 新出现的 id ⇒ `entered`；
/// - 消失的 id ⇒ `left`；
/// - 两 tick 都在且格坐标变化 ⇒ `moved(id, 旧格, 新格)`；
/// - 同格微动**不产生**条目（不影响可见关系）。
#[must_use]
pub fn derive_aoi_change_feed(prev: &AoiCellSnapshot, curr: &AoiCellSnapshot) -> AoiChangeFeed {
    let mut feed = AoiChangeFeed::default();
    for (id, curr_cell) in curr {
        match prev.get(id) {
            None => feed.entered.push(*id),
            Some(prev_cell) => {
                if prev_cell != curr_cell {
                    feed.moved.push((*id, *prev_cell, *curr_cell));
                }
            }
        }
    }
    for id in prev.keys() {
        if !curr.contains_key(id) {
            feed.left.push(*id);
        }
    }
    feed
}

/// 一个 tick 的 AOI 变更馈送。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AoiChangeFeed {
    /// 跨格移动：`(id, 旧格, 新格)`。
    pub moved: Vec<(i32, AoiCell, AoiCell)>,
    /// 进入世界/进入某图。
    pub entered: Vec<i32>,
    /// 离开世界/离开某图。
    pub left: Vec<i32>,
}

impl AoiChangeFeed {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.moved.is_empty() && self.entered.is_empty() && self.left.is_empty()
    }

    pub fn clear(&mut self) {
        self.moved.clear();
        self.entered.clear();
        self.left.clear();
    }

    /// 总变更条数（日志/仪器用）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.moved.len() + self.entered.len() + self.left.len()
    }
}

// ============================ S3b：OutputAoi 系统 ============================

/// `OutputAoi` 系统的输入快照（时钟 / 掉落清理门 / 事件与 sight 镜像）。
#[derive(Resource, Default)]
pub struct AoiParams {
    pub clock_ms: i64,
    pub clear_drop_on_floor_item_time: i64,
    pub event_sight: (i32, i32),
    pub events: Vec<crate::world::FireBurnEvent>,
}

/// AOI 分组复用缓冲（替代 `World::by_map_scratch`；跨 tick 复用，避免每 tick 分配）。
#[derive(Resource, Default)]
pub struct AoiScratch {
    pub by_map: Vec<Vec<i32>>,
    /// **T10 Top2**：全组件读视图的缓存 `QueryState`（跨 tick 复用）。
    ///
    /// `Option` 的原因：`world.query::<Q>()` 需要 `&mut World`，只能在系统里首次建表；
    /// 之后每 tick 用 `get_manual`（共享借用）按显式句柄取数，不迭代 Query 求顺序。
    pub view: Option<bevy_ecs::query::QueryState<crate::entity::EntityRefData>>,
    /// 分桶用的 `Transform` 只读视图（只取 `map_id`，不构造完整 `EntityRef`）。
    pub map_view: Option<bevy_ecs::query::QueryState<(&'static Transform,)>>,
}

/// `OutputAoi` 系统的输出（facade 跑完 schedule 后转抄进 `TickRecord`）。
#[derive(Resource, Default)]
pub struct AoiOut {
    pub aoi_order: Vec<i32>,
    pub hides: Vec<(i32, i32)>,
    pub hide_observer_kinds: Vec<(i32, i32, crate::world::HideObserverKind)>,
    pub event_hides: Vec<(i32, usize, i32, i32)>,
}

/// bevy `World` 侧的 [`AoiStore`] 实现：按显式句柄取三组件，**不迭代 Query 求顺序**。
struct BevyAoiStore<'a> {
    world: &'a mut bevy_ecs::world::World,
    /// 槽位表（本段期间从 bevy `World` **移出**后借用；段内无结构变更 ⇒ 顺序/解析恒定）。
    ///
    /// 这样 `id -> bevy 实体` 走 [`crate::ecs::SlotTable::entity_of`] 的 O(1)
    /// 稠密索引，不再每 tick 建 11k 项 `HashMap`（T10 Top2）。
    table: &'a crate::ecs::SlotTable,
    /// 全组件读视图（跨 tick 缓存的 `QueryState`；见 [`AoiScratch::view`]）。
    view: &'a bevy_ecs::query::QueryState<crate::entity::EntityRefData>,
}

impl AoiStore for BevyAoiStore<'_> {
    fn get(&self, id: i32) -> Option<EntityRef<'_>> {
        let e = self.table.entity_of(id)?;
        // 一次组合视图取齐（T10 Top2）：旧实现逐组件 `world.get::<C>` = 24 次
        // `TypeId → ComponentId` + archetype 解析；现在一次 `get_manual`。组件缺失的跳过
        // 语义不变（任一列缺失 ⇒ `Err` ⇒ `None`），S4a 的「缺组件是插入期硬断言」意不变。
        let (
            (
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
            ),
            (player_run, group, revival, buff),
            (slave, inventory, merchant, npc, ai),
        ) = self.view.get_manual(&*self.world, e).ok()?;
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
            slave,
            inventory,
            merchant,
            npc,
            ai,
        ))
    }

    fn ai_mut(&mut self, id: i32) -> Option<&mut AnimalState> {
        let e = self.table.entity_of(id)?;
        self.world
            .get_mut::<AiState>(e)
            .map(|m| &mut bevy_ecs::change_detection::Mut::into_inner(m).ai)
    }

    fn map_binding_mut(&mut self, id: i32) -> Option<&mut MapBinding> {
        let e = self.table.entity_of(id)?;
        self.world
            .get_mut::<MapBinding>(e)
            .map(bevy_ecs::change_detection::Mut::into_inner)
    }
}

/// `TickStage::OutputAoi` 的系统：AOI 刷新的唯一生产实现。
///
/// 输入（资源，facade 在 tick 边界一次性移入）：[`AoiParams`] + [`MapsRes<MapGrid>`] +
/// [`AoiScratch`]；输出：[`AoiOut`]。扫描顺序 = **本段自己借用的 [`SlotTable`] 槽位顺序**
///（T10 Top2：此前是 facade 每 tick `slot_order()` 克隆进 `AoiIds` 资源），
/// **从不迭代 `Query` 求顺序**。
pub fn output_aoi_system(world: &mut bevy_ecs::world::World) {
    // S3b 评审 P3：输入资源由 facade 在 tick 边界**无条件**移入（见 `World::tick` 的
    // `OutputAoi` 段）⇒ 缺失是接线错误，用 `expect` 立即失败，而不是静默按空表跑
    //（`unwrap_or_default` 会让 AOI 静默不出/漏出，golden 仍可能绿）。
    let mut params = world
        .remove_resource::<AoiParams>()
        .expect("OutputAoi 前必须插入 AoiParams（facade tick 边界）");
    let mut maps = world
        .remove_resource::<MapsRes<MapGrid>>()
        .expect("OutputAoi 前必须插入 MapsRes（facade tick 边界）");
    // `AoiScratch` 是本段**跨 tick 复用**的资源：系统每 tick 把它从 `World` 取出、段尾原样
    // 放回（资源本身跨 tick 存活，`by_map` 缓冲与两个 `QueryState` 都复用）。缺省构造是
    // 合法初始态（含两个未建表的视图缓存）⇒ 保持 `unwrap_or_default`（首 tick 建表）。
    let mut scratch = world.remove_resource::<AoiScratch>().unwrap_or_default();
    // **T10 Top2/Top4**：槽位表就是「id → 实体 + 槽位序」的唯一真值。本段把它从 bevy
    // `World` 移出后借用：既得到 O(1) 的 `entity_of`（替代每 tick 重建的 11k 项
    // `HashMap`），又直接按槽位序迭代（替代 facade 每 tick 的 `slot_order()` 克隆）。
    // 段内无结构变更（AOI 只改组件），段尾原样放回。
    let table = world
        .remove_resource::<crate::ecs::SlotTable>()
        .expect("OutputAoi 前必须已插入 SlotTable（ECS 内核不变量）");
    let mut out = AoiOut::default();

    // 两个缓存视图：建表（首次）与 archetype 刷新（结构变更发生在其它段）。
    scratch
        .view
        .get_or_insert_with(|| world.query::<crate::entity::EntityRefData>())
        .update_archetypes(world);
    scratch
        .map_view
        .get_or_insert_with(|| world.query::<(&'static Transform,)>())
        .update_archetypes(world);
    let view = scratch
        .view
        .as_ref()
        .expect("view 刚由 get_or_insert_with 建立");
    let map_view = scratch
        .map_view
        .as_ref()
        .expect("map_view 刚由 get_or_insert_with 建立");

    let mut by_map = std::mem::take(&mut scratch.by_map);
    by_map.clear();
    by_map.resize_with(maps.0.len(), Vec::new);
    for v in by_map.iter_mut() {
        v.clear();
    }
    // 分桶只读 `Transform`（地图 id）——旧实现为拿 `map_id` 构造完整 24 组件 `EntityRef`。
    for (id, entity) in table.order_iter() {
        if let Ok((transform,)) = map_view.get_manual(&*world, entity) {
            if let Some(v) = by_map.get_mut(transform.map_id) {
                v.push(id);
            }
        }
    }
    {
        let mut store = BevyAoiStore {
            world: &mut *world,
            table: &table,
            view,
        };
        for (map_id, map_ids) in by_map.iter().enumerate() {
            let Some(map) = maps.0.get_mut(map_id) else {
                continue;
            };
            for id in map_ids {
                // **T10 Top2**：death/race/ghost/搜索门在同一快照里取一次（旧实现为这三项
                // 各构造一次完整 `EntityRef`；三项都是只读字段，且本段内 race/death 不变）。
                let snap = match store.get(*id) {
                    Some(e) => (
                        e.death,
                        e.race,
                        e.ghost,
                        e.ai.search_tick,
                        e.ai.search_time,
                        e.is_clone,
                        crate::monai::monster_self_scans(&e),
                    ),
                    None => continue,
                };
                let (death, race, ghost, search_tick, search_time, is_clone, self_scans) = snap;
                if death {
                    search_view_range_death(
                        map,
                        &mut store,
                        *id,
                        params.clock_ms,
                        params.clear_drop_on_floor_item_time,
                    );
                } else {
                    let (scan, observer_kind) = if race == crate::entity::ACTOR_RACE_PLAY {
                        (
                            !ghost && params.clock_ms - search_tick > search_time,
                            if is_clone {
                                crate::world::HideObserverKind::Clone
                            } else {
                                crate::world::HideObserverKind::Player
                            },
                        )
                    } else if !ghost && self_scans {
                        (
                            params.clock_ms - search_tick > search_time,
                            crate::world::HideObserverKind::NonPlayer,
                        )
                    } else {
                        (false, crate::world::HideObserverKind::Player)
                    };
                    if !scan {
                        out.aoi_order.push(*id);
                        continue;
                    }
                    if let Some(ai) = store.ai_mut(*id) {
                        ai.search_tick = params.clock_ms;
                    }
                    let outcome =
                        search_view_range_with_updates(map, &mut store, *id, params.clock_ms);
                    for target in outcome.removed {
                        out.hides.push((*id, target));
                        out.hide_observer_kinds.push((*id, target, observer_kind));
                    }
                    for (monster_id, player_id) in outcome.view_updates {
                        if store
                            .get(monster_id)
                            .is_some_and(|e| crate::monai::consumes_view_range_update(&e))
                        {
                            if let Some(ai) = store.ai_mut(monster_id) {
                                ai.pending_view_updates.push(player_id);
                            }
                        }
                    }
                    if race == crate::entity::ACTOR_RACE_PLAY {
                        for (idx, x, y) in search_view_range_events(
                            map,
                            &mut store,
                            *id,
                            &params.events,
                            &mut params.event_sight,
                        ) {
                            out.event_hides.push((*id, idx, x, y));
                        }
                    }
                }
                out.aoi_order.push(*id);
            }
        }
    }
    // AOI 搜索遥测（默认关闭）：每 500 tick 汇总一行 `[AOI_STATS]`（stderr）。
    if let Some(line) = aoi_stats_tick(params.clock_ms, by_map.iter().map(|v| v.len() as u64)) {
        eprintln!("{line}");
    }
    scratch.by_map = by_map;
    world.insert_resource(table);
    world.insert_resource(maps);
    world.insert_resource(params);
    world.insert_resource(scratch);
    world.insert_resource(out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{EntitySpawn, EntityStore};

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
        let mut viewer = crate::EntitySpawn::new(1, "p", 10, 10);
        viewer.race = ACTOR_RACE_PLAY;
        viewer.want_ref_msg = true;
        viewer.view_range = 5;
        store.insert(viewer);
        put(&mut map, 1, 10, 10, 60_000);
        let mut target = crate::EntitySpawn::new(2, "guard", 11, 10);
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
        let mut viewer = crate::EntitySpawn::new(1, "guard", 10, 10);
        viewer.race = ACTOR_RACE_MONSTER;
        viewer.view_range = 5;
        store.insert(viewer);
        put(&mut map, 1, 10, 10, 60_000);
        let mut target = crate::EntitySpawn::new(2, "stale", 11, 10);
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

    fn play_viewer(id: i32, x: i32, y: i32, view_range: i32) -> EntitySpawn {
        let mut e = crate::EntitySpawn::new(id, "p", x, y);
        e.race = ACTOR_RACE_PLAY;
        // `PlayObject` ctor `WantRefMsg = true`（`PlayObject.cs:889`）。
        e.want_ref_msg = true;
        e.view_range = view_range;
        e
    }

    fn monster_entity(id: i32, x: i32, y: i32, view_range: i32) -> EntitySpawn {
        let mut e = crate::EntitySpawn::new(id, "m", x, y);
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
        let mut baby = crate::EntitySpawn::new(3, "baby", 10, 11);
        baby.race = ACTOR_RACE_MONSTER;
        baby.master = Some(1);
        store.insert(baby);
        let mut plain = crate::EntitySpawn::new(4, "plain", 10, 12);
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
        let mut guard = crate::EntitySpawn::new(1, "g", 10, 10);
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

    /// **M-3 A3 ②：ghost 不进他人 AOI 可见集**（oracle `BaseObject.ViewRange.cs:105-112` 的
    /// `!baseObject.Ghost`，对应当前 `search_view_range_with_updates` 的 `!other.ghost`）。
    ///
    /// 牙：删掉可见判定里的 `&& !other.ghost` ⇒ ghost 进可见集 ⇒ 红。
    /// 规则 19 退化对照：把同一目标清掉 ghost 位、重扫 ⇒ 立刻可见。
    #[test]
    fn m3_ghost_player_is_not_in_others_view_set() {
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        let mut viewer = crate::EntitySpawn::new(1, "p", 10, 10);
        viewer.race = ACTOR_RACE_PLAY;
        viewer.want_ref_msg = true;
        viewer.view_range = 5;
        store.insert(viewer);
        put(&mut map, 1, 10, 10, 0);
        let mut ghost = crate::EntitySpawn::new(2, "g", 11, 10);
        ghost.race = ACTOR_RACE_PLAY;
        ghost.ghost = true;
        store.insert(ghost);
        put(&mut map, 2, 11, 10, 0);

        let _ = search_view_range(&mut map, &mut store, 1, 1_000);
        let snap = visible_snapshot(&store, 1);
        assert!(
            !snap.iter().any(|(id, _)| *id == 2),
            "ghost 不得进他人可见集：{snap:?}"
        );

        // 退化对照：清 ghost 位 ⇒ 同一夹具立刻可见。
        store.get_mut(2).unwrap().life_cycle.ghost = false;
        let _ = search_view_range(&mut map, &mut store, 1, 1_000);
        let snap = visible_snapshot(&store, 1);
        assert!(
            snap.iter().any(|(id, _)| *id == 2),
            "非 ghost ⇒ 必进可见集（门只挡 ghost）：{snap:?}"
        );
    }

    // ---------------------------------------------------------------------------------------
    // 卡 `openmir2-aoi-incremental` phase 1：AOI 搜索遥测（tier=R）。
    // 牙①关闭态零写/无行；②已知搜索计数；③行为等价（可见集 + aoi_order）；④直方图分位。
    // ---------------------------------------------------------------------------------------

    /// 牙①：默认关闭（只有 `MIR2_AOI_STATS=1` 开启）。关闭态：不取时钟、搜索不计数、
    /// 500 tick 不出行。
    ///
    /// 红检：把 `aoi_stats_enabled` 写死 `true`（无视覆盖/环境）=> 本用例第一个快照
    /// `searches == 1` 或时钟调用数 > 0 或 500 tick 后拿到 `Some(line)` => 红。
    #[test]
    fn aoi_stats_disabled_has_zero_counters_and_no_report() {
        // 环境门口径：只有字面 "1" 开启。
        assert!(!aoi_stats_env_enabled(None));
        assert!(!aoi_stats_env_enabled(Some("0")));
        assert!(!aoi_stats_env_enabled(Some("")));
        assert!(!aoi_stats_env_enabled(Some("true")));
        assert!(aoi_stats_env_enabled(Some("1")));
        assert!(aoi_stats_env_enabled(Some(" 1 ")));

        let _guard = AoiStatsTestGuard::disabled();
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        store.insert(play_viewer(1, 10, 10, 5));
        store.insert(monster_entity(2, 11, 10, 5));
        put_typed(&mut map, 2, 11, 10, CellType::Monster, 0);
        let _ = search_view_range_with_updates(&mut map, &mut store, 1, 0);

        let snap = aoi_stats_snapshot_for_test();
        assert_eq!(snap.searches, 0, "关闭态不得累加搜索");
        assert_eq!(snap.window_ticks, 0, "关闭态不得累加 tick");
        assert!(snap.ns_hist.iter().all(|&count| count == 0));
        assert_eq!(
            aoi_stats_clock_calls_for_test(),
            0,
            "关闭态不得取 Instant（零开销）"
        );
        for _ in 0..AOI_STATS_REPORT_TICKS {
            assert!(
                aoi_stats_tick(0, std::iter::empty()).is_none(),
                "关闭态 500 tick 也不得产生 `[AOI_STATS]` 行"
            );
        }
        assert_eq!(aoi_stats_snapshot_for_test().window_ticks, 0);
    }

    /// 牙②：已知搜索的计数正确性（小地图、ViewRange=5、3 个候选）。
    ///
    /// 红检：把 `stats_cells` 写成 `2*R+1`（漏平方）或候选数从 `candidates.len()` 改成
    /// 其它量 => 断言必红。
    #[test]
    fn aoi_stats_counts_known_search() {
        let _guard = AoiStatsTestGuard::enabled();
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        store.insert(play_viewer(1, 10, 10, 5));
        for (id, x, y) in [(2, 11, 10), (3, 9, 10), (4, 10, 11)] {
            store.insert(monster_entity(id, x, y, 5));
            put_typed(&mut map, id, x, y, CellType::Monster, 0);
        }
        let outcome = search_view_range_with_updates(&mut map, &mut store, 1, 0);
        assert!(outcome.removed.is_empty());

        let snap = aoi_stats_snapshot_for_test();
        assert_eq!(snap.searches, 1);
        assert_eq!(snap.cells, 121, "(2*5+1)^2 = 121");
        assert_eq!(snap.candidates, 3, "三只可见怪");
        assert_eq!(snap.visible, 3, "新加入 3 条");
        assert_eq!(snap.removed, 0);
        assert!(snap.ns_max > 0, "每趟搜索耗时必须 > 0 ns");
        assert_eq!(snap.ns_hist.iter().sum::<u64>(), 1);
        assert_eq!(
            aoi_stats_clock_calls_for_test(),
            2,
            "每趟搜索两次 Instant（起/止）"
        );
        // 行内平均口径 = 窗口总和 / 搜索数。
        let line = snap.report_line(0, false);
        assert!(
            line.contains(" cells_per_search=121.00 cand_per_search=3.00 vis_per_search=3.00 "),
            "报告行字段/均值不符：{line}"
        );
    }

    /// 牙③：行为等价——同一输入（实体 + 移动）开/关遥测各跑一次，可见集逐项相等、
    /// `aoi_order` 与 `hides` 相同。
    #[test]
    fn aoi_stats_behavior_equivalent() {
        type VisibleRows = Vec<(i32, Vec<(i32, u8)>)>;

        fn run() -> (Vec<crate::TickRecord>, VisibleRows) {
            let mut w = crate::World::new(32, 32);
            let mut player = crate::EntitySpawn::new(1, "p", 10, 10);
            player.race = ACTOR_RACE_PLAY;
            player.want_ref_msg = true;
            player.view_range = 8;
            w.add_entity(player);
            for (id, x, y) in [(2, 12, 10), (3, 14, 12), (4, 20, 20)] {
                let mut monster = crate::EntitySpawn::new(id, "m", x, y);
                monster.race = ACTOR_RACE_MONSTER;
                monster.view_range = 5;
                w.add_entity(monster);
            }
            let mut records = Vec::new();
            // 玩家 SearchTime=1000ms、tick=200ms => 第 6 tick(1200ms) 才发生首趟搜索。
            for _ in 0..8 {
                w.walk(1, 1, 0);
                records.push(w.tick());
            }
            let visible = w
                .entities
                .order_index()
                .iter()
                .map(|id| (*id, visible_snapshot(&w.entities, *id)))
                .collect();
            (records, visible)
        }

        let (on_records, on_visible) = {
            let _guard = AoiStatsTestGuard::enabled();
            let out = run();
            let snap = aoi_stats_snapshot_for_test();
            assert!(
                snap.searches > 0,
                "开启态必须真的记录到搜索（否则等价性没有意义）"
            );
            out
        };
        let (off_records, off_visible) = {
            let _guard = AoiStatsTestGuard::disabled();
            let out = run();
            assert_eq!(aoi_stats_snapshot_for_test().searches, 0);
            out
        };

        assert_eq!(
            on_visible, off_visible,
            "可见集（含 flag/顺序）必须逐项相等"
        );
        let on_order: Vec<Vec<i32>> = on_records.iter().map(|r| r.aoi_order.clone()).collect();
        let off_order: Vec<Vec<i32>> = off_records.iter().map(|r| r.aoi_order.clone()).collect();
        assert_eq!(on_order, off_order, "aoi_order 必须相同");
        let on_hides: Vec<Vec<(i32, i32)>> = on_records.iter().map(|r| r.hides.clone()).collect();
        let off_hides: Vec<Vec<(i32, i32)>> = off_records.iter().map(|r| r.hides.clone()).collect();
        assert_eq!(on_hides, off_hides, "hides 必须相同");
    }

    /// 牙④：log2 直方图分位——已知样本 1..=1000 落在正确桶。
    #[test]
    fn aoi_stats_histogram_percentiles_land_in_expected_buckets() {
        assert_eq!(aoi_hist_bucket(0), 0);
        assert_eq!(aoi_hist_bucket(1), 1);
        assert_eq!(aoi_hist_bucket(2), 2);
        assert_eq!(aoi_hist_bucket(3), 2);
        assert_eq!(aoi_hist_bucket(500), 9);
        assert_eq!(aoi_hist_bucket(1000), 10);

        // ns 用**线性桶**（每桶 32 µs）：边界必须落在正确桶。
        assert_eq!(aoi_ns_bucket(0), 0);
        assert_eq!(aoi_ns_bucket(AOI_NS_BUCKET_NS - 1), 0);
        assert_eq!(aoi_ns_bucket(AOI_NS_BUCKET_NS), 1);
        assert_eq!(
            aoi_ns_bucket(2_048_000),
            AOI_NS_BUCKETS - 1,
            "≥2048µs 进溢出桶"
        );
        assert_eq!(aoi_ns_bucket_upper(0), Some(AOI_NS_BUCKET_NS));
        assert_eq!(
            aoi_ns_bucket_upper(AOI_NS_BUCKETS - 1),
            None,
            "溢出桶无上限"
        );

        let mut state = AoiStatsState::new();
        state.window_ticks = 1;
        for ns in 1..=1000u64 {
            state.searches += 1;
            state.ns_sum += ns;
            state.ns_max = state.ns_max.max(ns);
            state.ns_hist[aoi_ns_bucket(ns)] += 1;
        }
        // 1000 个样本都 < 32 µs ⇒ 全在桶 0；p50/p90 都报桶 0 上限 32,000 ns；
        // 均值 = (1+…+1000)/1000 = 500.5。
        let line = state.report_line(0, false);
        assert!(
            line.contains(
                " ns_bucket_ns=32000 ns_avg=500.5 ns_p50=32000 ns_p90=32000 ns_max=1000 "
            ),
            "ns 线性桶/均值不符：{line}"
        );
        assert!(
            line.contains(" same_map_p50=0 same_map_p90=0 same_map_avg=0.00 same_map_max=0"),
            "same_map 无样本应为 0：{line}"
        );

        // 退化形态：空样本 => 分位 None、行内打 0（不 panic、不除零）。
        let empty = AoiStatsState::new();
        assert_eq!(aoi_percentile(&empty.ns_hist, 50, |_b| 0), None);
        let empty_line = empty.report_line(0, false);
        assert!(
            empty_line.contains(
                "clock_ms=0 window_seq=0 tick_seq=0 ticks=0 searches_in_window=0 searches_total=0 searches_per_tick=0.00"
            ),
            "空窗口行格式不符：{empty_line}"
        );
    }
    /// 牙⑤（opc-07 要求）：**累计口径自洽** + 序号单调 + `clock_ms` 透传。
    /// - 相邻两行 `searches_in_window` 之和 == 后一行 `searches_total`；
    /// - `window_seq` 每窗 +1、`tick_seq` 每 tick +1（满窗 = 500）；
    /// - 行内 `clock_ms` 就是调用方传入的世界时钟（不是墙钟/Instant）。
    #[test]
    fn aoi_stats_totals_and_sequences_are_self_consistent() {
        let _guard = AoiStatsTestGuard::enabled();

        for _ in 0..2 {
            let start = aoi_stats_search_begin().expect("开启态必须有起点");
            aoi_stats_record_search(start, 25, 3, 3, 0);
        }
        let mut line1 = None;
        for _ in 0..AOI_STATS_REPORT_TICKS {
            if let Some(l) = aoi_stats_tick(1_000, std::iter::empty::<u64>()) {
                line1 = Some(l);
            }
        }
        let line1 = line1.expect("满一窗必须出行");
        assert!(
            line1.contains(
                " clock_ms=1000 window_seq=1 tick_seq=500 ticks=500 searches_in_window=2 searches_total=2 "
            ),
            "第一窗口径不符：{line1}"
        );

        for _ in 0..3 {
            let start = aoi_stats_search_begin().expect("开启态必须有起点");
            aoi_stats_record_search(start, 25, 3, 3, 0);
        }
        let mut line2 = None;
        for _ in 0..AOI_STATS_REPORT_TICKS {
            if let Some(l) = aoi_stats_tick(2_000, std::iter::empty::<u64>()) {
                line2 = Some(l);
            }
        }
        let line2 = line2.expect("第二窗必须出行");
        assert!(
            line2.contains(
                " clock_ms=2000 window_seq=2 tick_seq=1000 ticks=500 searches_in_window=3 searches_total=5 "
            ),
            "第二窗口径不符（total 必须 = 前窗 2 + 本窗 3）：{line2}"
        );
        assert!(line2.contains(" searches_total=5 "), "累计自洽：2 + 3 == 5");
    }

    /// 牙⑥：`MIR2_AOI_STATS_TICKS` 解析——缺省/非法/0 一律回落默认值，合法正整数生效。
    #[test]
    fn aoi_stats_report_ticks_env_parse_is_safe() {
        assert_eq!(parse_report_ticks(None), AOI_STATS_REPORT_TICKS);
        assert_eq!(parse_report_ticks(Some("100")), 100);
        assert_eq!(parse_report_ticks(Some(" 100 ")), 100);
        for bad in ["", "0", "-5", "abc", "1.5"] {
            assert_eq!(
                parse_report_ticks(Some(bad)),
                AOI_STATS_REPORT_TICKS,
                "非法值 {bad:?} 必须回落默认"
            );
        }
    }

    /// 牙⑦：`same_map_avg` 是**非空地图**的实体数均值；终值行带 `final=1` 且窗口未满也出行。
    #[test]
    fn aoi_stats_same_map_avg_and_final_line() {
        let _guard = AoiStatsTestGuard::enabled();
        // 两 tick：第一 tick 两张图（1、3 个实体），第二 tick 一张图（0 个 = 空图跳过）。
        assert!(aoi_stats_tick(500, [1u64, 3].into_iter()).is_none());
        assert!(aoi_stats_tick(1_000, [0u64].into_iter()).is_none());
        let line = aoi_stats_final_line(1_500).expect("窗口未满也必须出终值行");
        assert!(line.contains("final=1 "), "终值行必须带 final 标记：{line}");
        assert!(
            line.contains(" same_map_avg=2.00 "),
            "非空地图均值 (1+3)/2 = 2.00：{line}"
        );
        assert!(line.contains(" clock_ms=1500 "), "clock 透传：{line}");
        // 终值行后窗口已清空，再取一次应为 None。
        assert!(aoi_stats_final_line(2_000).is_none());
    }

    // ================= 差分骨架（phase 2 前置，卡 openmir2-aoi-incremental） =================
    //
    // 目的：给"增量实现 vs legacy 全量扫描"提供**可复用的快照 + 比较器**，并先用两颗前置牙证明
    // 骨架本身不假绿：
    //   ① 确定性牙：同一实现对同一输入跑两遍，逐项相等；
    //   ② 检出力牙：比较器必须能**自己报出**"哪一 tick、哪个观察者、少了谁"。
    // 快照里的可见集/离开集都是**有序**的——顺序本身是语义（出帧顺序受影响）。

    /// 一个 (tick, viewer) 的可观测快照。
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct TickSnapshot {
        tick: usize,
        viewer: i32,
        /// 观察者当前可见集（**有序**，来自 `visible_actors.target`）。
        visible: Vec<i32>,
        /// 本趟离开视野（**有序**）。
        removed: Vec<i32>,
        /// 本趟 `RM_UPDATEVIEWRANGE` 投递（扫描序）。
        view_updates: Vec<(i32, i32)>,
    }

    fn visible_of(store: &EntityStore, viewer: i32) -> Vec<i32> {
        match store.get(viewer) {
            Some(e) => e.visible_actors.iter().map(|v| v.target).collect(),
            None => Vec::new(),
        }
    }

    /// 比较器：**自己**产出带上下文的分歧描述（不靠 `assert!` 的裸消息）。
    ///
    /// **检查顺序（写死，保证"报第一条分歧"可复现）**：
    /// 1. `SNAPSHOT_COUNT_MISMATCH` —— **必须最先**：长度不等时 zip 会静默截断，
    ///    "先集合后计数"的实现会把"少了一整个快照"错报成集合差异；
    /// 2. `TICK_VIEWER_MISMATCH`（快照对齐校验）；
    /// 3. `VISIBLE_SET_MISMATCH`（集合成员：missing / extra）——集合先于顺序；
    /// 4. `VISIBLE_ORDER_MISMATCH`（集合相同、顺序不同）；
    /// 5. `REMOVED_SET_MISMATCH`；
    /// 6. `VIEW_UPDATES_MISMATCH`。
    /// 顺序一旦改动，同一对输入可能报出不同类别 ⇒ 报告不可复现，故写进本注释并由测试钉住。
    fn compare_snapshots(a: &[TickSnapshot], b: &[TickSnapshot]) -> Result<(), String> {
        if a.len() != b.len() {
            return Err(format!(
                "SNAPSHOT_COUNT_MISMATCH a={} b={}",
                a.len(),
                b.len()
            ));
        }
        for (x, y) in a.iter().zip(b.iter()) {
            if x.tick != y.tick || x.viewer != y.viewer {
                return Err(format!(
                    "TICK_VIEWER_MISMATCH a=({}, {}) b=({}, {})",
                    x.tick, x.viewer, y.tick, y.viewer
                ));
            }
            if x.visible != y.visible {
                let missing: Vec<i32> = x
                    .visible
                    .iter()
                    .copied()
                    .filter(|e| !y.visible.contains(e))
                    .collect();
                let extra: Vec<i32> = y
                    .visible
                    .iter()
                    .copied()
                    .filter(|e| !x.visible.contains(e))
                    .collect();
                if missing.is_empty() && extra.is_empty() {
                    return Err(format!(
                        "VISIBLE_ORDER_MISMATCH tick={} viewer={} a={:?} b={:?}",
                        x.tick, x.viewer, x.visible, y.visible
                    ));
                }
                return Err(format!(
                    "VISIBLE_SET_MISMATCH tick={} viewer={} missing={:?} extra={:?}",
                    x.tick, x.viewer, missing, extra
                ));
            }
            if x.removed != y.removed {
                return Err(format!(
                    "REMOVED_SET_MISMATCH tick={} viewer={} a={:?} b={:?}",
                    x.tick, x.viewer, x.removed, y.removed
                ));
            }
            if x.view_updates != y.view_updates {
                return Err(format!(
                    "VIEW_UPDATES_MISMATCH tick={} viewer={} a={:?} b={:?}",
                    x.tick, x.viewer, x.view_updates, y.view_updates
                ));
            }
        }
        Ok(())
    }

    /// 确定性场景：3 个玩家（view_range=5）+ 一片怪；每 tick 加一只怪、切一只怪的 Ghost 标记，
    /// 再让每个玩家扫一遍。全部输入由 tick 决定 ⇒ 同实现两遍必须完全一致。
    fn run_diff_scenario() -> Vec<TickSnapshot> {
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        store.insert(play_viewer(1, 10, 10, 5));
        store.insert(play_viewer(2, 14, 10, 5));
        store.insert(play_viewer(3, 28, 28, 5));
        let mut next_id = 100;
        for dy in -3..=3 {
            for dx in -3..=3 {
                let (x, y) = (10 + dx, 10 + dy);
                store.insert(monster_entity(next_id, x, y, 5));
                put_typed(&mut map, next_id, x, y, CellType::Monster, 0);
                next_id += 1;
            }
        }
        let mut snaps = Vec::new();
        for tick in 0..12i64 {
            // 新怪逐步进入视野（位置随 tick 变化）。
            let x = 10 + i32::try_from(tick % 4).unwrap_or(0);
            let y = 13;
            store.insert(monster_entity(next_id, x, y, 5));
            put_typed(&mut map, next_id, x, y, CellType::Monster, tick);
            next_id += 1;
            // Ghost 标记来回切（影响可见性过滤）。
            let ghost_id = 100 + i32::try_from(tick % 5).unwrap_or(0);
            if let Some(e) = store.get_mut(ghost_id) {
                e.life_cycle.ghost = tick % 2 == 0;
            }
            for viewer in [1, 2, 3] {
                let out = search_view_range_with_updates(&mut map, &mut store, viewer, tick * 1000);
                snaps.push(TickSnapshot {
                    tick: usize::try_from(tick).unwrap_or(0),
                    viewer,
                    visible: visible_of(&store, viewer),
                    removed: out.removed.clone(),
                    view_updates: out.view_updates.clone(),
                });
            }
        }
        snaps
    }

    /// 固定种子 fuzz：LCG 生成操作序列（加怪 / 切 Ghost / 什么都不做），逐步重扫全部玩家。
    /// 返回 (快照, 操作日志)——**日志用于分歧时复现**。
    fn run_fuzz(seed: u64, steps: usize) -> (Vec<TickSnapshot>, Vec<String>) {
        let mut state = seed;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u32
        };
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        store.insert(play_viewer(1, 10, 10, 5));
        store.insert(play_viewer(2, 15, 12, 5));
        let mut next_id = 200;
        let mut ops = Vec::new();
        let mut snaps = Vec::new();
        for step in 0..steps {
            let op = next() % 3;
            match op {
                0 => {
                    let x = 6 + i32::try_from(next() % 12).unwrap_or(0);
                    let y = 6 + i32::try_from(next() % 12).unwrap_or(0);
                    store.insert(monster_entity(next_id, x, y, 5));
                    put_typed(&mut map, next_id, x, y, CellType::Monster, step as i64);
                    ops.push(format!("step={step} add id={next_id} at=({x},{y})"));
                    next_id += 1;
                }
                1 => {
                    let id = 200 + i32::try_from(next() % 8).unwrap_or(0);
                    let flag = next() % 2 == 0;
                    if let Some(e) = store.get_mut(id) {
                        e.life_cycle.ghost = flag;
                        ops.push(format!("step={step} ghost id={id} flag={flag}"));
                    } else {
                        ops.push(format!("step={step} ghost id={id} skipped(absent)"));
                    }
                }
                _ => ops.push(format!("step={step} nop")),
            }
            for viewer in [1, 2] {
                let out = search_view_range_with_updates(
                    &mut map,
                    &mut store,
                    viewer,
                    i64::try_from(step).unwrap_or(0) * 1000,
                );
                snaps.push(TickSnapshot {
                    tick: step,
                    viewer,
                    visible: visible_of(&store, viewer),
                    removed: out.removed.clone(),
                    view_updates: out.view_updates.clone(),
                });
            }
        }
        (snaps, ops)
    }

    /// 牙①（确定性）：同一实现对同一场景跑两遍 ⇒ 逐项相等。
    #[test]
    fn aoi_diff_skeleton_is_deterministic() {
        let a = run_diff_scenario();
        let b = run_diff_scenario();
        match compare_snapshots(&a, &b) {
            Ok(()) => println!(
                "[AOI_DIFF] DETERMINISM_OK snapshots={} sample={:?}",
                a.len(),
                a.get(a.len() / 2)
            ),
            Err(e) => panic!("确定性牙红（同实现两遍不一致）：{e}"),
        }
        assert!(a.len() >= 30, "场景至少 12 tick × 3 viewer");
        assert!(
            a.iter().any(|s| s.visible.len() >= 2),
            "场景必须产生非平凡可见集（否则检出力牙没有靶子）"
        );
    }

    /// 牙②（检出力）：一侧故意丢一个可见实体 ⇒ 比较器必须报出 tick / viewer / 少了谁。
    #[test]
    fn aoi_diff_harness_detects_a_dropped_entity() {
        let a = run_diff_scenario();
        let mut b = run_diff_scenario();
        let (tick, viewer, dropped) = {
            let target = b
                .iter_mut()
                .find(|s| s.visible.len() >= 2)
                .expect("场景里必须有一个可见集 ≥2 的快照");
            let dropped = target.visible.remove(0);
            (target.tick, target.viewer, dropped)
        };
        let err = compare_snapshots(&a, &b).expect_err("丢掉一个可见实体后比较器必须报红");
        println!(
            "[AOI_DIFF] DETECTION_RED tick={} viewer={} dropped={} msg={}",
            tick, viewer, dropped, err
        );
        assert!(err.contains("VISIBLE_SET_MISMATCH"), "{err}");
        assert!(err.contains(&format!("tick={tick}")), "{err}");
        assert!(err.contains(&format!("viewer={viewer}")), "{err}");
        assert!(err.contains(&format!("missing=[{dropped}]")), "{err}");
    }

    /// 牙③：**顺序**类分歧必须被抓到（集合相同、顺序不同）。
    #[test]
    fn aoi_diff_harness_detects_order_mismatch() {
        let a = run_diff_scenario();
        let mut b = run_diff_scenario();
        let (tick, viewer) = {
            let target = b
                .iter_mut()
                .find(|s| s.visible.len() >= 2)
                .expect("需要可见集 ≥2 的快照");
            target.visible.swap(0, 1);
            (target.tick, target.viewer)
        };
        let err = compare_snapshots(&a, &b).expect_err("顺序不同必须报红");
        println!("[AOI_DIFF] ORDER_RED tick={tick} viewer={viewer} msg={err}");
        assert!(err.contains("VISIBLE_ORDER_MISMATCH"), "{err}");
    }

    /// 牙④：**removed** 分歧必须被抓到。
    #[test]
    fn aoi_diff_harness_detects_removed_mismatch() {
        let a = run_diff_scenario();
        let mut b = run_diff_scenario();
        let idx = b.len() / 2;
        b[idx].removed.push(999_999);
        let err = compare_snapshots(&a, &b).expect_err("removed 不同必须报红");
        println!(
            "[AOI_DIFF] REMOVED_RED tick={} viewer={} msg={err}",
            b[idx].tick, b[idx].viewer
        );
        assert!(err.contains("REMOVED_SET_MISMATCH"), "{err}");
    }

    /// 牙⑤：**view_updates** 分歧必须被抓到。
    #[test]
    fn aoi_diff_harness_detects_view_updates_mismatch() {
        let a = run_diff_scenario();
        let mut b = run_diff_scenario();
        let idx = b.len() / 3;
        b[idx].view_updates.push((-1, -1));
        let err = compare_snapshots(&a, &b).expect_err("view_updates 不同必须报红");
        println!(
            "[AOI_DIFF] VIEW_UPDATES_RED tick={} viewer={} msg={err}",
            b[idx].tick, b[idx].viewer
        );
        assert!(err.contains("VIEW_UPDATES_MISMATCH"), "{err}");
    }

    /// 牙⑥：**快照数量**分歧必须被抓到（少一个快照）。
    #[test]
    fn aoi_diff_harness_detects_count_mismatch() {
        let a = run_diff_scenario();
        let mut b = run_diff_scenario();
        b.pop();
        let err = compare_snapshots(&a, &b).expect_err("快照数量不同必须报红");
        println!("[AOI_DIFF] COUNT_RED msg={err}");
        assert!(err.contains("SNAPSHOT_COUNT_MISMATCH"), "{err}");
    }

    /// 牙⑦：固定种子 fuzz——**同种子两遍完全一致**（确定性延伸）；分歧时 dump 操作日志。
    #[test]
    fn aoi_diff_fuzz_same_seed_is_deterministic() {
        const SEED: u64 = 0x5EED_1234;
        const STEPS: usize = 120;
        let (a, ops_a) = run_fuzz(SEED, STEPS);
        let (b, ops_b) = run_fuzz(SEED, STEPS);
        assert_eq!(ops_a, ops_b, "同种子的操作序列必须一致");
        match compare_snapshots(&a, &b) {
            Ok(()) => println!(
                "[AOI_DIFF] FUZZ_OK seed=0x{SEED:X} steps={STEPS} snapshots={} ops={}",
                a.len(),
                ops_a.len()
            ),
            Err(e) => panic!(
                "fuzz 同种子两遍不一致：{e}\n操作日志（前 20 条）：{:?}",
                &ops_a[..ops_a.len().min(20)]
            ),
        }
        assert!(a.len() == STEPS * 2, "每 step 两个观察者各一条快照");
    }

    // ================= 覆盖矩阵（设计 §4.2 六类）：先只验"场景良构 + 确定性" =================
    //
    // 这些场景函数同时是增量实现的对照输入：现阶段的绿只证明"同实现两遍一致"，
    // 等增量实现接上后，同一批场景必须 legacy-vs-incremental 也全绿（等价性的唯一标准）。

    /// 从地图格子里摘掉一个对象（按 `cell_obj_id` 定位，保留其余对象顺序）。
    fn remove_from_cell(map: &mut MapGrid, id: i32, x: i32, y: i32) {
        let (cell, ok) = map.get_cell_info_mut(x, y);
        if ok {
            if let Some(i) = cell.obj_list.iter().position(|c| c.cell_obj_id == id) {
                cell.remove_at(i);
            }
        }
    }

    /// 每 tick、每观察者做一次**集合对齐**（opc-07 要求"随矩阵走"）：
    /// legacy 实际遍历的候选 id 集合 必须 == `aoi_snapshot_from_map` 在该观察者窗口内的 id 集合。
    /// 不相等 ⇒ 该场景的差分结论无效，**立即停**。
    fn assert_candidate_universe_aligned(
        scenario: &str,
        map: &MapGrid,
        store: &EntityStore,
        viewer: i32,
        tick: usize,
    ) {
        let (vx, vy, _) = store.pos(viewer).expect("观察者必须在 store 里");
        let range = store.get(viewer).map_or(0, |e| e.view_range);
        let mut got = aoi_last_scan_universe();
        got.sort_unstable();
        got.dedup();
        let snap = aoi_snapshot_from_map(map);
        let mut expected: Vec<i32> = snap
            .iter()
            .filter(|(_, (x, y))| (*x - vx).abs() <= range && (*y - vy).abs() <= range)
            .map(|(id, _)| *id)
            .collect();
        expected.sort_unstable();
        if got != expected {
            let snap_only: Vec<i32> = expected
                .iter()
                .copied()
                .filter(|i| !got.contains(i))
                .collect();
            let legacy_only: Vec<i32> = got
                .iter()
                .copied()
                .filter(|i| !expected.contains(i))
                .collect();
            panic!(
                "CANDIDATE_SET_MISMATCH tick={tick} scenario={scenario} viewer={viewer} snapshot_only={snap_only:?} legacy_only={legacy_only:?}"
            );
        }
    }

    /// 通用驱动：每 tick 先执行 `step(tick)` 变更世界，再让每个观察者扫一遍、记快照，
    /// 并**逐 tick 做集合对齐断言**（对齐不过直接 panic，该场景结论作废）。
    fn drive(
        scenario: &str,
        ticks: usize,
        viewers: &[i32],
        mut step: impl FnMut(&mut MapGrid, &mut EntityStore, usize),
    ) -> (Vec<TickSnapshot>, usize) {
        aoi_universe_capture_set(true);
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        for viewer in viewers {
            store.insert(play_viewer(*viewer, 10, 10, 5));
        }
        let mut snaps = Vec::new();
        let mut aligned = 0usize;
        for tick in 0..ticks {
            step(&mut map, &mut store, tick);
            for viewer in viewers {
                let out = search_view_range_with_updates(
                    &mut map,
                    &mut store,
                    *viewer,
                    i64::try_from(tick).unwrap_or(0) * 1000,
                );
                assert_candidate_universe_aligned(scenario, &map, &store, *viewer, tick);
                aligned += 1;
                snaps.push(TickSnapshot {
                    tick,
                    viewer: *viewer,
                    visible: visible_of(&store, *viewer),
                    removed: out.removed.clone(),
                    view_updates: out.view_updates.clone(),
                });
            }
        }
        aoi_universe_capture_set(false);
        (snaps, aligned)
    }

    fn assert_scenario_deterministic(name: &str, run: impl Fn() -> (Vec<TickSnapshot>, usize)) {
        let (a, aligned_a) = run();
        let (b, aligned_b) = run();
        match compare_snapshots(&a, &b) {
            Ok(()) => println!(
                "[AOI_MATRIX] {name} OK snapshots={} aligned_checks={} nonzero_visible={}",
                a.len(),
                aligned_a,
                a.iter().filter(|s| !s.visible.is_empty()).count()
            ),
            Err(e) => panic!("[AOI_MATRIX] {name} 非确定：{e}"),
        }
        assert_eq!(aligned_a, aligned_b, "{name}: 对齐检查次数必须一致");
        assert!(aligned_a >= a.len(), "{name}: 每个快照都应对齐检查过");
        assert!(!a.is_empty(), "{name}: 场景必须产生快照");
    }

    /// 矩阵①：实体**进入格子**（窗内中心 / 边界格 x=15 / 窗外 x=20 三种位置各来一只）。
    #[test]
    fn aoi_matrix_entity_enters_cell() {
        assert_scenario_deterministic("entity_enters_cell", || {
            drive("entity_enters_cell", 6, &[1], |map, store, tick| {
                if (2..=4).contains(&tick) {
                    let x = 10 + i32::try_from(tick).unwrap_or(0) + 2;
                    let id = 300 + i32::try_from(tick).unwrap_or(0);
                    store.insert(monster_entity(id, x, 10, 5));
                    put_typed(map, id, x, 10, CellType::Monster, tick as i64);
                }
            })
        });
    }

    /// 矩阵②：实体**离开格子**（先在场、随后从 store 与地图同时摘除）。
    #[test]
    fn aoi_matrix_entity_leaves_cell() {
        assert_scenario_deterministic("entity_leaves_cell", || {
            drive("entity_leaves_cell", 6, &[1], |map, store, tick| {
                if tick == 0 {
                    store.insert(monster_entity(400, 13, 10, 5));
                    put_typed(map, 400, 13, 10, CellType::Monster, 0);
                }
                if tick == 3 {
                    remove_from_cell(map, 400, 13, 10);
                    store.remove(400);
                }
            })
        });
    }

    /// 矩阵③：**观察者移动**（set_xy 平移；窗口随之平移）。
    #[test]
    fn aoi_matrix_viewer_moves() {
        assert_scenario_deterministic("viewer_moves", || {
            drive("viewer_moves", 6, &[1, 2], |map, store, tick| {
                if tick == 0 {
                    for id in 500..510 {
                        let x = 12 + (id - 500);
                        store.insert(monster_entity(id, x, 10, 5));
                        put_typed(map, id, x, 10, CellType::Monster, 0);
                    }
                }
                let _ = store.set_xy(1, 10 + i32::try_from(tick).unwrap_or(0), 10);
            })
        });
    }

    /// 矩阵④：**状态变化**（Ghost 来回切）。
    #[test]
    fn aoi_matrix_state_toggle() {
        assert_scenario_deterministic("state_toggle", || {
            drive("state_toggle", 6, &[1], |map, store, tick| {
                if tick == 0 {
                    for id in 600..606 {
                        store.insert(monster_entity(id, 11 + (id - 600), 11, 5));
                        put_typed(map, id, 11 + (id - 600), 11, CellType::Monster, 0);
                    }
                }
                if let Some(e) = store.get_mut(600 + i32::try_from(tick % 3).unwrap_or(0)) {
                    e.life_cycle.ghost = tick % 2 == 0;
                }
            })
        });
    }

    /// 矩阵⑤：**上下线**（候选实体上线/下线 + 观察者中途加入）。
    #[test]
    fn aoi_matrix_online_offline() {
        assert_scenario_deterministic("online_offline", || {
            drive("online_offline", 6, &[1, 2], |map, store, tick| {
                if tick == 1 {
                    store.insert(monster_entity(700, 12, 12, 5));
                    put_typed(map, 700, 12, 12, CellType::Monster, 1);
                }
                if tick == 3 {
                    remove_from_cell(map, 700, 12, 12);
                    store.remove(700);
                }
            })
        });
    }

    /// 矩阵⑥：**触发时机**（搜索间隔门）：同 now_ms 连续两趟扫描必须记录可复现的快照序列。
    #[test]
    fn aoi_matrix_search_gate() {
        assert_scenario_deterministic("search_gate", || {
            let mut map = MapGrid::new(32, 32);
            let mut store = EntityStore::new();
            store.insert(play_viewer(1, 10, 10, 5));
            store.insert(monster_entity(800, 12, 10, 5));
            put_typed(&mut map, 800, 12, 10, CellType::Monster, 0);
            aoi_universe_capture_set(true);
            let mut snaps = Vec::new();
            let mut aligned = 0usize;
            // 同一 now_ms 连扫三趟（间隔门未到期）——逐趟记录并逐趟做集合对齐。
            for pass in 0..3usize {
                let out = search_view_range_with_updates(&mut map, &mut store, 1, 0);
                assert_candidate_universe_aligned("search_gate", &map, &store, 1, pass);
                aligned += 1;
                snaps.push(TickSnapshot {
                    tick: pass,
                    viewer: 1,
                    visible: visible_of(&store, 1),
                    removed: out.removed.clone(),
                    view_updates: out.view_updates.clone(),
                });
            }
            aoi_universe_capture_set(false);
            (snaps, aligned)
        });
    }

    /// phase 2 契约：`AoiChangeFeed` 的判空/清空/计数语义（实现与馈送方共同依赖）。
    #[test]
    fn aoi_change_feed_basic_contract() {
        let mut feed = AoiChangeFeed::default();
        assert!(feed.is_empty());
        assert_eq!(feed.len(), 0);
        feed.moved.push((7, (1, 2), (1, 3)));
        feed.entered.push(8);
        feed.left.push(9);
        assert!(!feed.is_empty());
        assert_eq!(feed.len(), 3);
        feed.clear();
        assert!(feed.is_empty());
        assert_eq!(feed.len(), 0);
    }

    /// 派生 feed 的完备性语义：进/出/跨格/同格微动四类各一条，且顺序确定。
    #[test]
    fn aoi_derived_feed_covers_all_transitions() {
        let mut prev = AoiCellSnapshot::new();
        prev.insert(1, (0, 0));
        prev.insert(2, (1, 1));
        prev.insert(4, (5, 5));
        let mut curr = AoiCellSnapshot::new();
        curr.insert(1, (0, 1)); // 跨格
        curr.insert(3, (2, 2)); // 新进
        curr.insert(4, (5, 5)); // 同格微动（不在快照层面体现）
        let feed = derive_aoi_change_feed(&prev, &curr);
        assert_eq!(feed.moved, vec![(1, (0, 0), (0, 1))]);
        assert_eq!(feed.entered, vec![3]);
        assert_eq!(feed.left, vec![2]);
        assert_eq!(feed.len(), 3);

        // 空对空 ⇒ 空 feed（退化形态）。
        let empty = derive_aoi_change_feed(&AoiCellSnapshot::new(), &AoiCellSnapshot::new());
        assert!(empty.is_empty());
    }

    /// **集合对齐牙**（opc-07 要求，卡 `openmir2-aoi-incremental`）：
    /// 派生 feed 的输入全集（`aoi_snapshot_from_map`）必须与 legacy 每 tick 实际遍历的候选集相等。
    /// 场景特意包含 **Ghost**、**已死未清（store 已移除但格子里还在）**——这两类最容易被"只收存活"
    /// 的快照漏掉，进而让两侧解不同的问题。
    #[test]
    fn aoi_candidate_universe_aligns_with_legacy_scan() {
        aoi_universe_capture_set(true);
        let mut map = MapGrid::new(32, 32);
        let mut store = EntityStore::new();
        store.insert(play_viewer(1, 10, 10, 5));
        store.insert(monster_entity(900, 12, 10, 5));
        put_typed(&mut map, 900, 12, 10, CellType::Monster, 0);
        store.insert(monster_entity(901, 13, 10, 5));
        put_typed(&mut map, 901, 13, 10, CellType::Monster, 0);
        store.get_mut(901).expect("ghost 怪").life_cycle.ghost = true;
        store.insert(monster_entity(902, 14, 10, 5));
        put_typed(&mut map, 902, 14, 10, CellType::Monster, 0);
        store.remove(902); // 已死未清：地图里还在

        let _ = search_view_range_with_updates(&mut map, &mut store, 1, 0);
        let mut got = aoi_last_scan_universe();
        got.sort_unstable();

        let snap = aoi_snapshot_from_map(&map);
        let in_window = |x: i32, y: i32| (5..=15).contains(&x) && (5..=15).contains(&y);
        let mut expected: Vec<i32> = snap
            .iter()
            .filter(|(_, (x, y))| in_window(*x, *y))
            .map(|(id, _)| *id)
            .collect();
        expected.sort_unstable();

        if got != expected {
            let snap_only: Vec<i32> = expected
                .iter()
                .copied()
                .filter(|i| !got.contains(i))
                .collect();
            let legacy_only: Vec<i32> = got
                .iter()
                .copied()
                .filter(|i| !expected.contains(i))
                .collect();
            panic!(
                "CANDIDATE_SET_MISMATCH tick=0 viewer=1 snapshot_only={snap_only:?} legacy_only={legacy_only:?}"
            );
        }
        assert!(got.contains(&901), "Ghost 必须在候选全集里");
        assert!(
            got.contains(&902),
            "已死未清对象必须仍在候选全集里（legacy 会遍历到）"
        );
        println!("[AOI_ALIGN] OK viewer=1 universe={got:?}");
        aoi_universe_capture_set(false);
    }
}
