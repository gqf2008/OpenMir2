//! S1：**ECS 实体存储内核**（`crates/world` → `bevy_ecs` 迁移的第一批）。
//!
//! 形态：用 [`bevy_ecs::world::World`] 承载实体/组件，用本结构**自己的槽位索引**
//! （`slots` / `slot_keys`）决定遍历与处理顺序。
//!
//! ## 四条硬约束怎么落实（设计文档 §4.2.2）
//!
//! 1. **单线程**：本内核不建线程、不用并行 API；tick 骨架见 [`crate::tick`]（显式
//!    [`bevy_ecs::schedule::SingleThreadedExecutor`]）；
//! 2. **遍历顺序显式化**：`EcsStore` **一次都不迭代 `Query` 求顺序**——`iter` / `iter_mut` /
//!    `order_index` 全部走 `slots`（槽位顺序 = 插入顺序 + 空闲槽复用）。`Query`（或 `World::get`）
//!    只用于「按显式句柄取组件」。这一点由 `tests/ecs_kernel.rs::order_follows_slots_not_query_order`
//!    钉住（把 `order_index()` 改回迭代 `Query` 立刻红）。
//! 3. **守卫升级**：`crates/world/tests/no_parallel.rs` 的白名单放宽为
//!    「内部 crate + `bevy_ecs`/`bevy_app`」，仍禁 rayon/crossbeam/`bevy_tasks` 并行/`thread::spawn`；
//! 4. **原判据不降级**：内核对外只暴露与既有 `EntityStore` 同形的语义（槽位顺序、空闲槽复用、
//!    代际号），`crates/world` 的三个 golden 对拍逐行不变。

use std::collections::HashMap;

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

/// **显式处理顺序索引**（资源）：槽位顺序的 bevy 实体句柄。
///
/// 系统只按它决定处理顺序；`Query` 只用于按句柄取组件。`EcsStore::sync_slot_order`
/// 会在每次 tick 之前刷新它（实体的增删都只改槽位，不改这里的口径）。
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotOrder(pub Vec<Entity>);

#[derive(Debug, Clone, Copy)]
struct Slot {
    /// 代际号：空闲槽复用时 +1（与既有 `EntityStore` 的语义一致；对外不可见）。
    generation: u32,
    /// 当前占用该槽的 bevy 实体句柄（空闲为 `None`）。
    entity: Option<Entity>,
    /// 该槽对应业务 id（空闲为 `None`）——按槽位顺序取 id 时不必反查 map。
    key: Option<i32>,
}

/// ECS 实体存储内核：`bevy_ecs` 存组件，本结构存「谁在哪个槽、按什么顺序遍历」。
///
/// 泛型参数 `T` 是业务实体组件（`crates/world` 的 `Entity`）。内核**不认识** `T` 的业务字段，
/// 只按调用方给的 `id` 建索引，因此可以原样承载任何 `Component`。
pub struct EcsStore<T: Component<Mutability = Mutable>> {
    world: World,
    /// 槽位表（顺序即处理顺序）。
    slots: Vec<Slot>,
    /// 空闲槽（LIFO：与既有 `EntityStore.free.pop()` 完全一致）。
    free: Vec<usize>,
    /// 业务 id → 槽位（只用于查表，**不参与遍历**）。
    index: HashMap<i32, usize>,
    /// 下一个分配的 id（C# `ActorMgr` 单调递增语义）。
    next_id: i32,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T: Component<Mutability = Mutable>> Default for EcsStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Component<Mutability = Mutable>> std::fmt::Debug for EcsStore<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EcsStore")
            .field("len", &self.len())
            .field("slots", &self.slots.len())
            .field("free", &self.free.len())
            .field("next_id", &self.next_id)
            .finish()
    }
}

impl<T: Component<Mutability = Mutable>> EcsStore<T> {
    pub fn new() -> Self {
        EcsStore {
            world: World::new(),
            slots: Vec::new(),
            free: Vec::new(),
            index: HashMap::new(),
            next_id: 1,
            _marker: std::marker::PhantomData,
        }
    }

    /// 分配一个 id（不插入实体；C# `ActorMgr` 先 `Add` 后填字段）。
    pub fn alloc_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 插入实体，返回其业务 id（复用调用方给的 `id`）。
    ///
    /// 槽位分配与既有 `EntityStore::insert` 逐条一致：优先复用 `free.pop()` 的槽
    /// （代际号 +1），否则追加到末尾。
    pub fn insert(&mut self, id: i32, value: T) -> i32 {
        let slot_idx = if let Some(idx) = self.free.pop() {
            self.slots[idx].generation = self.slots[idx].generation.wrapping_add(1);
            self.slots[idx].entity = None;
            self.slots[idx].key = None;
            idx
        } else {
            self.slots.push(Slot {
                generation: 0,
                entity: None,
                key: None,
            });
            self.slots.len() - 1
        };
        let bevy_entity = self.world.spawn(value).id();
        self.slots[slot_idx].entity = Some(bevy_entity);
        self.slots[slot_idx].key = Some(id);
        self.index.insert(id, slot_idx);
        if id >= self.next_id {
            self.next_id = id + 1;
        }
        id
    }

    /// 移除实体，返回其组件（不存在返回 `None`）。空闲槽按 LIFO 回收。
    pub fn remove(&mut self, id: i32) -> Option<T> {
        let slot_idx = self.index.remove(&id)?;
        let bevy_entity = self.slots[slot_idx].entity.take()?;
        self.slots[slot_idx].key = None;
        let taken = self.world.entity_mut(bevy_entity).take::<T>();
        self.world.despawn(bevy_entity);
        if taken.is_some() {
            self.free.push(slot_idx);
        }
        taken
    }

    /// O(1) 查表（C# `ActorMgr.Get`）；只按显式句柄取组件，不迭代 `Query`。
    pub fn get(&self, id: i32) -> Option<&T> {
        let slot_idx = *self.index.get(&id)?;
        let bevy_entity = self.slots[slot_idx].entity?;
        self.world.get::<T>(bevy_entity)
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut T> {
        let slot_idx = *self.index.get(&id)?;
        let bevy_entity = self.slots[slot_idx].entity?;
        self.world
            .get_mut::<T>(bevy_entity)
            .map(bevy_ecs::change_detection::Mut::into_inner)
    }

    /// 按槽位顺序遍历（处理顺序的唯一来源）。
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        self.slots
            .iter()
            .filter_map(|s| s.entity)
            .filter_map(|e| self.world.get::<T>(e))
    }

    /// 按槽位顺序可变遍历（同样**不**用 `Query` 定序：先取到互斥引用，再按槽位索引排序）。
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> + '_ {
        // 取到所有 (Entity, &mut T)（Query 只负责「安全拿到互斥引用」），
        // 再按我们的槽位索引排序后产出 —— 顺序仍来自 slots，不来自 Query。
        let mut pairs: Vec<(Entity, &mut T)> = {
            let mut state = self.world.query::<(Entity, &mut T)>();
            state
                .iter_mut(&mut self.world)
                .map(|(e, m)| (e, m.into_inner()))
                .collect()
        };
        let order: HashMap<Entity, usize> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.entity.map(|e| (e, i)))
            .collect();
        pairs.sort_by_key(|(e, _)| order.get(e).copied().unwrap_or(usize::MAX));
        pairs.into_iter().map(|(_, v)| v)
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.entity.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 按 id 升序收集（用于输出/对拍的稳定顺序）。
    pub fn ids_sorted(&self) -> Vec<i32> {
        let mut v: Vec<i32> = self.slots.iter().filter_map(|s| s.key).collect();
        v.sort_unstable();
        v
    }

    /// **显式处理顺序索引**：槽位顺序的业务 id（处理顺序的唯一契约）。
    pub fn order_index(&self) -> Vec<i32> {
        self.slots.iter().filter_map(|s| s.key).collect()
    }

    /// 槽位顺序的 bevy 实体句柄（供系统按 [`SlotOrder`] 处理）。
    pub fn order_entities(&self) -> Vec<Entity> {
        self.slots.iter().filter_map(|s| s.entity).collect()
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// 把当前槽位顺序写入 [`SlotOrder`] 资源（每个 tick 处理前调用一次）。
    pub fn sync_slot_order(&mut self) {
        let order = self.order_entities();
        self.world.insert_resource(SlotOrder(order));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Debug, PartialEq, Eq)]
    struct Item(i32);

    fn store_with(ids: &[i32]) -> EcsStore<Item> {
        let mut s = EcsStore::new();
        for id in ids {
            s.insert(*id, Item(*id));
        }
        s
    }

    #[test]
    fn order_follows_slots_not_query_order() {
        let mut s = store_with(&[1, 2, 3]);
        s.remove(1);
        s.insert(4, Item(4)); // 复用 1 的槽位
        assert_eq!(
            s.order_index(),
            vec![4, 2, 3],
            "处理顺序必须来自槽位索引（插入顺序 + 空闲槽复用），不是 ECS 的 Query/生成顺序"
        );
        assert_eq!(
            s.iter().map(|i| i.0).collect::<Vec<_>>(),
            vec![4, 2, 3],
            "iter 必须与 order_index 同序"
        );
        assert_eq!(s.ids_sorted(), vec![2, 3, 4]);
    }

    #[test]
    fn free_slots_are_reused_lifo() {
        let mut s = store_with(&[1, 2, 3]);
        s.remove(1);
        s.remove(2);
        s.insert(4, Item(4)); // 复用 2 的槽（LIFO）
        s.insert(5, Item(5)); // 复用 1 的槽
        assert_eq!(s.order_index(), vec![5, 4, 3]);
    }

    #[test]
    fn get_mut_sees_component_changes() {
        let mut s = store_with(&[7]);
        if let Some(item) = s.get_mut(7) {
            item.0 = 70;
        }
        assert_eq!(s.get(7), Some(&Item(70)));
        // iter_mut 也按槽位顺序
        let vals: Vec<i32> = s.iter_mut().map(|i| i.0).collect();
        assert_eq!(vals, vec![70]);
    }

    #[test]
    fn sync_slot_order_matches_order_entities() {
        let mut s = store_with(&[1, 2]);
        s.remove(1);
        s.insert(3, Item(3));
        s.sync_slot_order();
        let order = s.world().resource::<SlotOrder>();
        assert_eq!(order.0, s.order_entities());
        assert_eq!(order.0.len(), 2);
    }
}
