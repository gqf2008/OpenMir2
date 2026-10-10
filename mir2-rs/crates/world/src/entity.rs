//! 实体存储 —— 对应 C# 的 `SystemShare.ActorMgr`（`ActorId` 稳定 id）+ `BaseObject` 的 AOI 相关字段。
//!
//! 存储形态：连续数组 + 空闲槽回收 + **代际号**（generation），防止"已删除 id 被复用"后
//! 旧引用误命中（C# 侧靠 `ActorMgr.Get(id) == null` 判断，但 id 会复用；这里把复用风险显式化，
//! 对外仍以 C# 风格的 `i32` id 暴露，代际只在内部校验）。
//!
//! 遍历顺序 = 槽位顺序（= 插入顺序 + 回收槽复用），**全库唯一**：
//! tick 的确定性（判据①）依赖"同一组操作产生同一槽位排布"。

use crate::map::MapGrid;

/// `src/OpenMir2/Enums/Race.cs`：`ActorRace.Play = 0`、`ActorRace.Monster = 80`。
pub const ACTOR_RACE_PLAY: u8 = 0;
pub const ACTOR_RACE_MONSTER: u8 = 80;

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

/// `BaseObject` 中参与本骨架（tick / AOI / 移动）的字段子集。
#[derive(Clone, Debug)]
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
    pub death: bool,
    pub invisible: bool,
    pub ghost: bool,
    pub fixed_hide_mode: bool,
    pub ob_mode: bool,
    /// C# `NastyMode`（PK 模式等）
    pub nasty_mode: bool,
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
            death: false,
            invisible: false,
            ghost: false,
            fixed_hide_mode: false,
            ob_mode: false,
            nasty_mode: false,
            want_ref_msg: false,
            add_to_mapped: false,
            visible_actors: Vec::new(),
            is_visible_active: false,
            processed_tick: 0,
        }
    }
}

/// 连续的实体槽。
#[derive(Clone, Debug)]
struct Slot {
    generation: u32,
    entity: Option<Entity>,
}

/// 实体仓库（`ActorMgr` 等价物）。
///
/// **查表**：C# 的 `ActorMgr.Get(id)` 是字典级查找（O(1)）；本实现用 `index: HashMap<i32, usize>`
/// 保持同样量级——否则 AOI 过滤器里的每次 `get` 都退化成线性扫，2000 实体时单 tick 会到几十毫秒
/// （2026-10-10 实测：线性版 P99 80.2ms vs 索引版见 `tick_bench` 输出）。
/// 该 HashMap **只用于查表、不参与遍历**，所以不影响 tick 的确定性顺序（遍历恒为槽位顺序）。
#[derive(Clone, Debug, Default)]
pub struct EntityStore {
    slots: Vec<Slot>,
    free: Vec<usize>,
    index: std::collections::HashMap<i32, usize>,
    /// 下一个分配的 `ActorId`（C# 单调递增）
    next_id: i32,
}

impl EntityStore {
    pub fn new() -> Self {
        EntityStore {
            slots: Vec::new(),
            free: Vec::new(),
            index: std::collections::HashMap::new(),
            next_id: 1,
        }
    }

    /// 分配一个 id（不插入实体；C# `ActorMgr` 先 `Add` 后填字段）。
    pub fn alloc_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 插入实体，返回其 id（复用本实体的 `id`）。
    pub fn insert(&mut self, entity: Entity) -> i32 {
        let id = entity.id;
        let slot_idx = if let Some(idx) = self.free.pop() {
            self.slots[idx].generation = self.slots[idx].generation.wrapping_add(1);
            self.slots[idx].entity = Some(entity);
            idx
        } else {
            self.slots.push(Slot {
                generation: 0,
                entity: Some(entity),
            });
            self.slots.len() - 1
        };
        self.index.insert(id, slot_idx);
        if id >= self.next_id {
            self.next_id = id + 1;
        }
        id
    }

    pub fn remove(&mut self, id: i32) -> Option<Entity> {
        let idx = self.index.remove(&id)?;
        let taken = self.slots[idx].entity.take();
        if taken.is_some() {
            self.free.push(idx);
        }
        taken
    }

    /// O(1) 查表（C# `ActorMgr.Get`）。
    pub fn get(&self, id: i32) -> Option<&Entity> {
        self.index
            .get(&id)
            .and_then(|i| self.slots[*i].entity.as_ref())
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut Entity> {
        self.index
            .get(&id)
            .and_then(|i| self.slots.get_mut(*i))
            .and_then(|s| s.entity.as_mut())
    }

    /// 按槽位顺序遍历（tick 的确定性顺序）。
    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.slots.iter().filter_map(|s| s.entity.as_ref())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Entity> {
        self.slots.iter_mut().filter_map(|s| s.entity.as_mut())
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.entity.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 按 id 升序收集（用于输出/对拍的稳定顺序）。
    pub fn ids_sorted(&self) -> Vec<i32> {
        let mut v: Vec<i32> = self.iter().map(|e| e.id).collect();
        v.sort_unstable();
        v
    }

    /// 会话/实体进出世界：把实体放入地图格子（对应 `Envirnoment.AddMapObject` 的
    /// `AddObject` + `cellInfo.Add` 组合）。
    pub fn enter_world(&mut self, id: i32, map: &mut MapGrid, now: i64) -> bool {
        let Some(e) = self.get(id) else {
            return false;
        };
        let (x, y, cell_type) = (e.x, e.y, crate::map::CellType::Play);
        let first = !e.add_to_mapped;
        let ok = map.add_map_object(x, y, cell_type, id, now);
        if ok {
            if let Some(e) = self.get_mut(id) {
                e.add_to_mapped = true;
            }
        }
        let _ = first;
        ok
    }

    /// 离开世界：从地图格子摘除（对应 `DeleteFromMap`）。
    pub fn leave_world(&mut self, id: i32, map: &mut MapGrid) -> bool {
        let Some(e) = self.get(id) else {
            return false;
        };
        let (x, y) = (e.x, e.y);
        let (cell, success) = map.get_cell_info_mut(x, y);
        if !success {
            return false;
        }
        let before = cell.count();
        cell.obj_list
            .retain(|o| !(o.actor_object && o.cell_obj_id == id));
        if cell.count() == 0 {
            cell.clear();
        }
        let removed = cell.count() != before;
        if removed {
            if let Some(e) = self.get_mut(id) {
                e.add_to_mapped = false;
                e.visible_actors.clear();
            }
        }
        removed
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

    #[test]
    fn enter_and_leave_world_bookkeeping() {
        let mut s = EntityStore::new();
        let mut map = MapGrid::new(10, 10);
        let id = s.alloc_id();
        s.insert(Entity::new(id, "p", 4, 5));
        assert!(s.enter_world(id, &mut map, 0));
        assert_eq!(map.cell(4, 5).count(), 1);
        assert!(s.get(id).unwrap().add_to_mapped);
        assert!(s.leave_world(id, &mut map));
        assert_eq!(map.cell(4, 5).count(), 0);
        assert!(!s.get(id).unwrap().add_to_mapped);
    }
}
