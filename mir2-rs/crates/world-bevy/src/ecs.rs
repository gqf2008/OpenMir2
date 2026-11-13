//! S1：**ECS 实体存储内核**（`crates/world` → `bevy_ecs` 迁移的第一批）。
//!
//! S2 调优（同一门禁口径下把 ΔP50/ΔP99 拉回绿）集中在两条热路径（不改语义、不改
//! 对外 API）：
//! 1. `get`/`iter` 的组件读取从 `World::get::<T>`（每次 `TypeId` 查 `ComponentId` + 走
//!    `entity→archetype→table row`）换成缓存的 [`QueryState`]（`get_manual`）；
//! 2. `id → 槽位` 从 SipHash `HashMap` 换成 [`IdIndex`]（O(1) 数组定位 + 有界兜底）。
//!
//! 两处均有隔离微基准与 A/B 证据（见 `tests/parity/world/S2调优报告.md`）。
//!
//! 形态：用 [`bevy_ecs::world::World`] 承载实体/组件；槽位索引
//! （`slots` / `free` / `index`）自 S4d-3/P0 起住在 bevy `World` 的 [`crate::ecs::SlotTable`]
//! 资源里（唯一真值），本结构只保留缓存查询与「实体 -> 槽位」序缓存。
//!
//! ## 四条硬约束怎么落实（设计文档 §4.2.2）
//!
//! 1. **单线程**：本内核不建线程、不用并行 API；tick 骨架见 [`crate::tick`]（显式
//!    [`bevy_ecs::schedule::SingleThreadedExecutor`]）；
//! 2. **遍历顺序显式化**：`EcsStore` **一次都不迭代 `Query` 求顺序**——`iter` / `iter_mut` /
//!    `order_index` 全部走 `SlotTable`（槽位顺序 = 插入顺序 + 空闲槽复用）。`Query`（或 `World::get`）
//!    只用于「按显式句柄取组件」。这一点由内核单测 `src/ecs.rs::tests::
//!    order_follows_slots_not_query_order` 与 `crates/world/tests/ecs_migration.rs::
//!    entity_order_follows_slots_not_query_order` 双重钉住（把顺序改回迭代 `Query` 立刻红）。
//! 3. **守卫升级**：`crates/world/tests/no_parallel.rs` 的白名单放宽为
//!    「内部 crate + `bevy_ecs`/`bevy_app`」，仍禁 rayon/crossbeam/`bevy_tasks` 并行/`thread::spawn`；
//! 4. **原判据不降级**：内核对外只暴露与既有 `EntityStore` 同形的语义（槽位顺序、空闲槽复用、
//!    代际号），`crates/world` 的三个 golden 对拍逐行不变。

use std::collections::HashMap;

use bevy_ecs::change_detection::Mut;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryState;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

/// **显式处理顺序索引**（资源）：槽位顺序的 bevy 实体句柄。
///
/// 系统只按它决定处理顺序；`Query` 只用于按句柄取组件。`EcsStore::sync_slot_order`
/// 会在每次 tick 之前刷新它（实体的增删都只改槽位，不改这里的口径）。
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotOrder(pub Vec<Entity>);

/// **id → 槽位索引**（S2 调优）：把热路径上的 `id→槽位` 从 SipHash `HashMap`（本机
/// `ecs_microbench` 实测 ≈5.6ns/次）换成 O(1) 数组定位（同命令实测 ≈0.4–0.5ns/次）。
///
/// 形态：`id ∈ [1, DENSE_CAP]` 走 `dense` 数组（`0` 表示空，否则存 `slot+1`）；范围外
/// （含非正 id）走 `sparse` HashMap 兜底。内存上界 `DENSE_CAP * 4B`。
///
/// 为什么稠密是合理的：C# `ActorMgr.GetNextIdentity` 用 `HUtil32.Sequence()` +
/// `_generateQueue` 在小区间内**循环复用** `ActorId`（`ActorManager.cs:31-39`），世界内
/// 同时存在的 id 稠密且小；`tick_bench` 的 1..=N 即此形态。
const DENSE_CAP: usize = 1 << 20; // 4MiB（u32）；超过则退化为 HashMap（有界降级）

#[derive(Default)]
struct IdIndex {
    /// `dense[id] = slot + 1`（0 = 该 id 不存在）。
    dense: Vec<u32>,
    /// 范围外 id 的兜底（非正 id 或 `id > DENSE_CAP`）。
    sparse: HashMap<i32, usize>,
}

impl IdIndex {
    fn insert(&mut self, id: i32, slot: usize) {
        if id >= 1 && (id as usize) < DENSE_CAP {
            let i = id as usize;
            if self.dense.len() <= i {
                self.dense.resize(i + 1, 0);
            }
            self.dense[i] = (slot as u32) + 1;
        } else {
            self.sparse.insert(id, slot);
        }
    }

    fn get(&self, id: i32) -> Option<usize> {
        if id >= 1 && (id as usize) < DENSE_CAP {
            match self.dense.get(id as usize) {
                Some(&v) if v != 0 => Some((v - 1) as usize),
                _ => None,
            }
        } else {
            self.sparse.get(&id).copied()
        }
    }

    fn remove(&mut self, id: i32) -> Option<usize> {
        if id >= 1 && (id as usize) < DENSE_CAP {
            match self.dense.get_mut(id as usize) {
                Some(v) if *v != 0 => {
                    let slot = (*v - 1) as usize;
                    *v = 0;
                    Some(slot)
                }
                _ => None,
            }
        } else {
            self.sparse.remove(&id)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    /// 代际号：空闲槽复用时 +1（与既有 `EntityStore` 的语义一致；对外不可见）。
    generation: u32,
    /// 当前占用该槽的 bevy 实体句柄（空闲为 `None`）。
    entity: Option<Entity>,
    /// 该槽对应业务 id（空闲为 `None`）——按槽位顺序取 id 时不必反查 map。
    key: Option<i32>,
}

/// **S4d-3/P0：槽位表资源**（设计件 `docs/world-ecs-s4d3-entitymut-20261113.md` §3.1 / D1）。
///
/// 槽位表 / 空闲槽 / `id -> 槽位` 索引 / 下一个 id 的**唯一真值**住在 bevy `World` 里。
/// 为什么不是留在 `EcsStore` 的普通字段（D1 的裁决依据）：段系统拿到的是 `EcsStore`
/// 内部那个 bevy `World` 的裸 `&mut`，看不到 wrapper 的字段；而 Command / NPC / 商人 /
/// 宝宝清表四段会在 tick 中增删实体，后续段必须看到**变更后**的集合（槽位序 + id 解析），
/// 否则会漏实体或遍历已删实体，与旧 `iter_ref` 语义不等价。
///
/// 不变量与迁移前逐条相同：槽位顺序 = 插入序 + 空闲槽 LIFO 复用；代际号只在复用时 +1；
/// `next_id` 单调；`id -> 槽位` 只用于查表、不参与遍历。`EcsStore` 的对外方法签名不变，
/// 只是内部改读本资源。
#[derive(Resource)]
pub struct SlotTable {
    slots: Vec<Slot>,
    free: Vec<usize>,
    index: IdIndex,
    next_id: i32,
    /// `EcsStore::iter_mut` 的「实体 -> 槽位」缓存是否需要重建。任何槽位变动都置位。
    order_dirty: bool,
}

impl Default for SlotTable {
    fn default() -> Self {
        SlotTable {
            slots: Vec::new(),
            free: Vec::new(),
            index: IdIndex::default(),
            next_id: 1,
            order_dirty: true,
        }
    }
}

impl std::fmt::Debug for SlotTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlotTable")
            .field("len", &self.len())
            .field("slots", &self.slots.len())
            .field("free", &self.free.len())
            .field("next_id", &self.next_id)
            .finish()
    }
}

impl SlotTable {
    /// 分配一个 id（不插入实体；C# `ActorMgr` 先 `Add` 后填字段）。
    pub fn alloc_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// 业务 id -> bevy 实体句柄（`None` = id 不在槽位表，或该槽已空）。
    pub fn entity_of(&self, id: i32) -> Option<Entity> {
        let slot_idx = self.index.get(id)?;
        self.slots.get(slot_idx)?.entity
    }

    /// 槽位顺序的 `(业务 id, bevy 实体)` 表（处理顺序的唯一契约）。
    pub fn order(&self) -> Vec<(i32, Entity)> {
        self.slots
            .iter()
            .filter_map(|s| match (s.key, s.entity) {
                (Some(k), Some(e)) => Some((k, e)),
                _ => None,
            })
            .collect()
    }

    /// 槽位顺序的 bevy 实体句柄。
    pub fn order_entities(&self) -> Vec<Entity> {
        self.slots.iter().filter_map(|s| s.entity).collect()
    }

    /// 槽位顺序的实体句柄（零分配迭代；`iter`/`iter_mut` 用）。
    pub fn entity_iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.slots.iter().filter_map(|s| s.entity)
    }

    /// 槽位顺序的业务 id。
    pub fn order_index(&self) -> Vec<i32> {
        self.slots.iter().filter_map(|s| s.key).collect()
    }

    /// 按 id 升序收集（用于输出/对拍的稳定顺序）。
    pub fn ids_sorted(&self) -> Vec<i32> {
        let mut v: Vec<i32> = self.slots.iter().filter_map(|s| s.key).collect();
        v.sort_unstable();
        v
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.entity.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 「实体 -> 槽位」缓存是否需要重建。
    pub fn order_dirty(&self) -> bool {
        self.order_dirty
    }

    /// 重建完成后清位。
    pub fn clear_order_dirty(&mut self) {
        self.order_dirty = false;
    }

    /// 取一个槽位（优先 `free.pop()` 的槽并让代际号 +1，否则追加到末尾）。
    fn take_free_slot(&mut self) -> usize {
        if let Some(idx) = self.free.pop() {
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
        }
    }

    /// **P5-pre**：段内结构插入的槽位登记（与 `EcsStore::insert` 的 `take_free_slot` + `bind`
    /// 同序）；由 `crate::tick::StageEntities::insert` 调用（world-bevy 内 `pub(crate)`）。
    pub(crate) fn bind_new(&mut self, id: i32, entity: Entity) {
        let slot_idx = self.take_free_slot();
        self.bind(slot_idx, id, entity);
    }

    /// 把 (id, bevy 实体) 绑到刚取的槽位，并推进 `next_id`。
    fn bind(&mut self, slot_idx: usize, id: i32, entity: Entity) {
        self.slots[slot_idx].entity = Some(entity);
        self.slots[slot_idx].key = Some(id);
        self.index.insert(id, slot_idx);
        if id >= self.next_id {
            self.next_id = id + 1;
        }
        self.order_dirty = true;
    }

    /// 解绑并返回 `(槽位下标, bevy 实体)`；id 不存在或槽已空则 `None`。
    pub(crate) fn unbind(&mut self, id: i32) -> Option<(usize, Entity)> {
        let slot_idx = self.index.remove(id)?;
        let entity = self.slots[slot_idx].entity.take()?;
        self.slots[slot_idx].key = None;
        self.order_dirty = true;
        Some((slot_idx, entity))
    }

    /// 把槽位还回空闲表（LIFO）。
    pub(crate) fn release_slot(&mut self, slot_idx: usize) {
        self.free.push(slot_idx);
    }
}

/// ECS 实体存储内核：`bevy_ecs` 存组件，本结构存「谁在哪个槽、按什么顺序遍历」。
///
/// 泛型参数 `T` 是业务实体组件（`crates/world` 的 `Entity`）。内核**不认识** `T` 的业务字段，
/// 只按调用方给的 `id` 建索引，因此可以原样承载任何 `Component`。
pub struct EcsStore<T: Component<Mutability = Mutable>> {
    world: World,
    /// **缓存 `&T` 查询状态**（S2 调优）：`get`/`iter` 走 [`QueryState::get_manual`]。
    ///
    /// 为什么不是 `World::get::<T>`：后者每次调用都要用 `TypeId` 去 `Components` 里
    /// 查 `ComponentId`（一次哈希）再走 `entity → archetype → table row`；`QueryState`
    /// 把 `ComponentId`/表列解析缓存在 `fetch_state` 里，`get_manual` 是纯 O(1) 指针定位。
    /// 可复现的隔离微基准：`cargo run -p mir2-world-bevy --release --bin ecs_microbench`
    ///（本机 2026-10-11 release 三次：`World::get::<T>` ≈5.4–5.9ns vs `QueryState::get_manual`
    /// ≈3.4–3.5ns）；AOI 每 tick 要做约 10 万次 `get`，即此处是 S1 回归的主因。
    read_state: QueryState<&'static T>,
    /// **缓存 `&mut T` 查询状态**：`get_mut` 走 [`QueryState::get_mut`]（同样省掉 `TypeId` 查表）。
    write_state: QueryState<&'static mut T>,
    /// **缓存 `(Entity, &mut T)` 查询状态**（S2 调优）：`iter_mut` 不再每次调用 `world.query`。
    /// `Query` 只用于「安全拿到全部互斥引用」，不决定顺序（顺序仍按 [`Self::order_map`] 排）。
    pairs_state: QueryState<(Entity, &'static mut T)>,
    /// `iter_mut` 的「实体 -> 槽位」序：只在槽位表变动（`SlotTable::order_dirty`）时重建一次，
    /// 不再每次调用都建 `HashMap`（每次调用仍需一次按槽位序的原地排序）。
    order_map: HashMap<Entity, usize>,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T: Component<Mutability = Mutable>> Default for EcsStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Component<Mutability = Mutable>> std::fmt::Debug for EcsStore<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let table = self.world.resource::<SlotTable>();
        f.debug_struct("EcsStore")
            .field("len", &table.len())
            .field("slots", &table.slots.len())
            .field("free", &table.free.len())
            .field("next_id", &table.next_id)
            .finish()
    }
}

impl<T: Component<Mutability = Mutable>> EcsStore<T> {
    pub fn new() -> Self {
        let mut world = World::new();
        // S4d-3/P0（D1）：槽位表的唯一真值进 bevy World 资源。
        world.insert_resource(SlotTable::default());
        // S4d-3/P2 前置：组件注册表 / 段跳过计数 / 段 QueryState 缓存。
        world.insert_resource(crate::tick::ComponentRegistry::default());
        world.insert_resource(crate::tick::StageSkip::default());
        world.insert_resource(crate::tick::StageQueries::default());
        // 三个查询状态各自缓存 `T` 的 `ComponentId`/表列；它们只在本结构里串行使用，
        // 不同时跑，因此读写访问集不冲突（也绝不放进同一个 Schedule 并行）。
        let read_state = world.query::<&T>();
        let write_state = world.query::<&mut T>();
        let pairs_state = world.query::<(Entity, &mut T)>();
        EcsStore {
            world,
            read_state,
            write_state,
            pairs_state,
            order_map: HashMap::new(),
            _marker: std::marker::PhantomData,
        }
    }

    /// **P2 前置**：把一个组件登记进 [`crate::tick::ComponentRegistry`]（段适配器的运行期镜像）。
    ///
    /// 幂等；`EntityStore::new` 会把 25 个组件一次性登记。
    pub fn register_component<C: Component>(&mut self) -> &mut Self {
        self.world
            .resource_mut::<crate::tick::ComponentRegistry>()
            .register::<C>();
        self
    }

    /// 组件注册表（只读；测试断言用）。
    pub fn component_registry(&self) -> &crate::tick::ComponentRegistry {
        self.world.resource::<crate::tick::ComponentRegistry>()
    }

    /// 分配一个 id（不插入实体；C# `ActorMgr` 先 `Add` 后填字段）。
    pub fn alloc_id(&mut self) -> i32 {
        self.world.resource_mut::<SlotTable>().alloc_id()
    }

    /// 插入实体，返回其业务 id（复用调用方给的 `id`）。
    ///
    /// 槽位分配与既有 `EntityStore::insert` 逐条一致：优先复用 `free.pop()` 的槽
    /// （代际号 +1），否则追加到末尾。
    pub fn insert(&mut self, id: i32, value: T) -> i32 {
        // 先取槽位（可能复用空闲槽），再 spawn（两次 `resource_mut` 之间不跨借用），
        // 最后把 (id, 实体) 绑上去 —— 与迁移前「先定槽、再填 entity/key/index/next_id」
        // 的顺序逐条一致。
        let slot_idx = self.world.resource_mut::<SlotTable>().take_free_slot();
        let bevy_entity = self.world.spawn(value).id();
        self.world
            .resource_mut::<SlotTable>()
            .bind(slot_idx, id, bevy_entity);
        // 结构性变更后刷新缓存查询状态的 archetype 匹配集（`&T` 的 archetype 在首次
        // 插入时创建；之后同组件集的 spawn 复用同一 archetype）。`remove` 同理兜底。
        self.refresh_archetypes();
        id
    }

    /// 移除实体，返回其组件（不存在返回 `None`）。空闲槽按 LIFO 回收。
    pub fn remove(&mut self, id: i32) -> Option<T> {
        let (slot_idx, bevy_entity) = self.world.resource_mut::<SlotTable>().unbind(id)?;
        let taken = self.world.entity_mut(bevy_entity).take::<T>();
        self.world.despawn(bevy_entity);
        if taken.is_some() {
            self.world
                .resource_mut::<SlotTable>()
                .release_slot(slot_idx);
        }
        self.refresh_archetypes();
        taken
    }

    /// O(1) 查表（C# `ActorMgr.Get`）；只按显式句柄取组件，不迭代 `Query`。
    ///
    /// 走缓存查询状态（`get_manual`）而非 `World::get::<T>`：省掉每次的 `TypeId` 查表，
    /// 是本轮把 ΔP50 拉回门禁内的关键路径。
    #[inline]
    pub fn get(&self, id: i32) -> Option<&T> {
        let bevy_entity = self.world.resource::<SlotTable>().entity_of(id)?;
        self.read_state.get_manual(&self.world, bevy_entity).ok()
    }

    /// 业务 id → bevy 实体句柄（显式句柄，不迭代 `Query`；供上层缓存多组件查询）。
    #[inline]
    pub fn entity_of(&self, id: i32) -> Option<Entity> {
        self.world.resource::<SlotTable>().entity_of(id)
    }

    #[inline]
    pub fn get_mut(&mut self, id: i32) -> Option<&mut T> {
        let bevy_entity = self.world.resource::<SlotTable>().entity_of(id)?;
        self.write_state
            .get_mut(&mut self.world, bevy_entity)
            .ok()
            .map(Mut::into_inner)
    }

    /// **可选组件原语（S3a）**：在既有实体上追加/覆盖一个组件。
    ///
    /// 与主组件 `T` 正交：**不触碰槽位表**，因此不改变 `order_index`/`iter` 的顺序契约。
    /// 组件插入是 archetype 迁移，之后必须刷新三个缓存查询状态的匹配集（否则 `T` 的
    /// `get`/`iter` 会跳过刚迁移的实体）。返回 `false` 表示该业务 id 不存在。
    ///
    /// 热路径提醒：本方法每次做一次 archetype 迁移 + 查询状态刷新，只用于「实体的组件
    /// 集合在生成/切片迁移时变化」，**不得**放进逐 tick 的内层循环（设计件风险 R10）。
    pub fn insert_component<C: Component<Mutability = Mutable>>(
        &mut self,
        id: i32,
        component: C,
    ) -> bool {
        let Some(bevy_entity) = self.world.resource::<SlotTable>().entity_of(id) else {
            return false;
        };
        self.world.entity_mut(bevy_entity).insert(component);
        self.refresh_archetypes();
        true
    }

    /// 读取某个实体上的可选组件（不存在/该实体没有该组件 ⇒ `None`）。
    ///
    /// 只按显式 id 取，不迭代 `Query`；顺序契约与是否有该组件无关。
    pub fn get_component<C: Component>(&self, id: i32) -> Option<&C> {
        let bevy_entity = self.world.resource::<SlotTable>().entity_of(id)?;
        self.world.get::<C>(bevy_entity)
    }

    /// 可变读取某个实体上的可选组件。
    pub fn get_component_mut<C: Component<Mutability = Mutable>>(
        &mut self,
        id: i32,
    ) -> Option<&mut C> {
        let bevy_entity = self.world.resource::<SlotTable>().entity_of(id)?;
        self.world.get_mut::<C>(bevy_entity).map(Mut::into_inner)
    }

    /// 移除并返回某个实体上的可选组件（不存在 ⇒ `None`）。
    ///
    /// 同样不改槽位表；移除成功时刷新缓存查询状态（archetype 迁移）。
    pub fn remove_component<C: Component<Mutability = Mutable>>(&mut self, id: i32) -> Option<C> {
        let bevy_entity = self.world.resource::<SlotTable>().entity_of(id)?;
        let taken = self.world.entity_mut(bevy_entity).take::<C>();
        if taken.is_some() {
            self.refresh_archetypes();
        }
        taken
    }

    /// 结构变更（spawn/despawn/组件集合变化）后刷新三个缓存查询状态的 archetype 匹配集。
    ///
    /// **P5-pre**：公开给 `crate::world::EntityStore::refresh_views()`——段内结构变更（只经
    /// `StageEntities` 的 bevy `World` 裸句柄）绕过了本结构的 `insert`/`remove`，facade 必须在
    /// 段后调用本方法，否则 `get`/`iter` 的缓存会漏掉新实体/仍见已删实体。
    pub fn refresh_archetypes(&mut self) {
        self.read_state.update_archetypes(&self.world);
        self.write_state.update_archetypes(&self.world);
        self.pairs_state.update_archetypes(&self.world);
    }

    /// 按槽位顺序遍历（处理顺序的唯一来源）。
    ///
    /// 顺序来自 `SlotTable`；`read_state` 只用于「按句柄取引用」，不参与定序。
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        let table = self.world.resource::<SlotTable>();
        table
            .entity_iter()
            .filter_map(move |e| self.read_state.get_manual(&self.world, e).ok())
    }

    /// 按槽位顺序可变遍历（顺序同样来自 `slots`；`Query` 只用于「安全拿到全部互斥引用」）。
    ///
    /// S2 调优：不再「每次调用都 `world.query` 建查询 + 建 HashMap + 排序」，而是复用缓存的
    /// [`Self::pairs_state`] / [`Self::order_map`]；`order_map` 只在槽位表变动后重建一次。
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> + '_ {
        if self.world.resource::<SlotTable>().order_dirty() {
            {
                let table = self.world.resource::<SlotTable>();
                self.order_map.clear();
                self.order_map
                    .extend(table.entity_iter().enumerate().map(|(i, e)| (e, i)));
            }
            self.world.resource_mut::<SlotTable>().clear_order_dirty();
        }
        // 取到所有 (Entity, &mut T)（Query 只负责「安全拿到互斥引用」），
        // 再按我们的槽位索引排序后产出 —— 顺序仍来自 slots，不来自 Query。
        let mut pairs: Vec<(Entity, &mut T)> = self
            .pairs_state
            .iter_mut(&mut self.world)
            .map(|(e, m)| (e, m.into_inner()))
            .collect();
        pairs.sort_by_key(|(e, _)| self.order_map.get(e).copied().unwrap_or(usize::MAX));
        pairs.into_iter().map(|(_, v)| v)
    }

    pub fn len(&self) -> usize {
        self.world.resource::<SlotTable>().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 按 id 升序收集（用于输出/对拍的稳定顺序）。
    pub fn ids_sorted(&self) -> Vec<i32> {
        self.world.resource::<SlotTable>().ids_sorted()
    }

    /// **显式处理顺序索引**：槽位顺序的业务 id（处理顺序的唯一契约）。
    pub fn order_index(&self) -> Vec<i32> {
        self.world.resource::<SlotTable>().order_index()
    }

    /// 槽位顺序的 bevy 实体句柄（供系统按 [`SlotOrder`] 处理）。
    pub fn order_entities(&self) -> Vec<Entity> {
        self.world.resource::<SlotTable>().order_entities()
    }

    /// 槽位顺序的 `(业务 id, bevy 实体)` 表（S4d-3：段适配器与 `TickEntityIds` 的入口）。
    pub fn slot_order(&self) -> Vec<(i32, Entity)> {
        self.world.resource::<SlotTable>().order()
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

    /// 可选组件（S3a）：与主组件 `T` 正交。
    #[derive(Component, Debug, PartialEq, Eq)]
    struct Extra(i32);

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

    /// S3a：可选组件的 `insert/get/get_mut/remove` 闭环。
    ///
    /// 牙：把 `insert_component` 改成不插/插错实体 ⇒ 读回值断言红；把 `remove_component`
    /// 改成只置默认不摘除 ⇒ 第二次 `get_component` 仍 `Some` ⇒ 红。
    #[test]
    fn optional_component_round_trip() {
        let mut s = store_with(&[1, 2]);
        // 只给 1 号插可选组件（2 号没有 ⇒ None，证「可选」而非「默认全有」）
        assert!(s.insert_component(1, Extra(11)));
        assert_eq!(s.get_component::<Extra>(1), Some(&Extra(11)));
        assert_eq!(s.get_component::<Extra>(2), None);
        // 不存在的 id ⇒ false
        assert!(!s.insert_component(99, Extra(0)));
        assert_eq!(s.get_component::<Extra>(99), None);
        // 可变写入
        if let Some(extra) = s.get_component_mut::<Extra>(1) {
            extra.0 += 1;
        }
        assert_eq!(s.get_component::<Extra>(1), Some(&Extra(12)));
        // 覆盖插入 = archetype 不变（同组件集），值被替换
        assert!(s.insert_component(1, Extra(20)));
        assert_eq!(s.get_component::<Extra>(1), Some(&Extra(20)));
        // 移除并返回值
        assert_eq!(s.remove_component::<Extra>(1), Some(Extra(20)));
        assert_eq!(s.get_component::<Extra>(1), None);
        assert_eq!(s.remove_component::<Extra>(1), None, "二次移除返回 None");
    }

    /// S3a 顺序契约：插/取/移除可选组件**不得**改变槽位顺序（主组件 `T` 的遍历来源）。
    ///
    /// 牙：若组件操作误动了 `slots`/`free`（例如把实体重排或补进空闲槽）⇒ 顺序断言红；
    /// 若 archetype 迁移后没刷新 `QueryState` ⇒ `iter` 会静默漏掉迁移过的实体。
    #[test]
    fn optional_components_do_not_change_slot_order() {
        let mut s = store_with(&[1, 2, 3]);
        s.remove(1);
        s.insert(4, Item(4)); // 复用 1 的槽：槽位序 [4, 2, 3]
        let before = s.order_index();
        assert_eq!(before, vec![4, 2, 3]);

        assert!(s.insert_component(4, Extra(40)));
        assert!(s.insert_component(2, Extra(20)));
        assert_eq!(s.order_index(), before, "插入可选组件不得改槽位顺序");
        assert_eq!(
            s.iter().map(|i| i.0).collect::<Vec<_>>(),
            vec![4, 2, 3],
            "archetype 迁移后主组件 iter 不得漏实体/换序"
        );
        // `pairs_state`（iter_mut）与 `write_state`（get_mut）也要各自钉一刀。
        //
        // 说明（S3a 评审 P3-1）：bevy 0.19 的 `iter_mut`/`get_mut` 内部走 `query_mut`，会
        // 自动刷新 archetype，因此这两条断言钉的是**行为**（迁移后不得漏实体/换序），而不是
        // `refresh_archetypes()` 里 `pairs_state`/`write_state` 那两次刷新本身；后者属于
        // 「删掉也不会红」的冗余保险（真正承重的是 `read_state`：`get`/`iter` 走
        // `get_manual`，不自动刷新）。保留刷新是为了与 `read_state` 同构、不依赖 bevy 内部实现。
        assert_eq!(
            s.iter_mut().map(|i| i.0).collect::<Vec<_>>(),
            vec![4, 2, 3],
            "archetype 迁移后主组件 iter_mut 不得漏实体/换序"
        );
        assert_eq!(
            s.get_mut(2).map(|i| i.0),
            Some(2),
            "archetype 迁移后 write_state.get_mut 必须能按显式句柄取到组件"
        );

        assert_eq!(s.remove_component::<Extra>(4), Some(Extra(40)));
        assert_eq!(s.order_index(), before, "移除可选组件不得改槽位顺序");
        assert_eq!(
            s.iter().map(|i| i.0).collect::<Vec<_>>(),
            vec![4, 2, 3],
            "移除组件后主组件 iter 仍按槽位序"
        );
        assert_eq!(
            s.iter_mut().map(|i| i.0).collect::<Vec<_>>(),
            vec![4, 2, 3],
            "移除组件后主组件 iter_mut 仍按槽位序（pairs_state）"
        );
        // 主组件读写不受可选组件影响
        assert_eq!(s.get(2), Some(&Item(2)));
        s.get_mut(2).unwrap().0 = 22;
        assert_eq!(s.get(2), Some(&Item(22)));
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

    /// `IdIndex` 的稠密/兜底两条路径都要正确（含边界：范围外 id 与负 id）。
    #[test]
    fn id_index_dense_and_sparse_paths_agree() {
        let mut s = EcsStore::new();
        let small = 7;
        let negative = -3;
        let beyond = i32::try_from(DENSE_CAP).unwrap();
        // sparse 区插入
        s.insert(negative, Item(negative));
        s.insert(beyond, Item(beyond));
        // dense 区插入
        s.insert(small, Item(small));
        assert_eq!(s.get(negative).map(|i| i.0), Some(negative));
        assert_eq!(s.get(beyond).map(|i| i.0), Some(beyond));
        assert_eq!(s.get(small).map(|i| i.0), Some(small));
        // 两条路径的 remove 各自生效，且不影响另一条
        assert_eq!(s.remove(small).map(|i| i.0), Some(small));
        assert!(s.get(small).is_none());
        assert!(s.get(beyond).is_some(), "移除 dense 项不得动到 sparse 项");
        assert_eq!(s.remove(beyond).map(|i| i.0), Some(beyond));
        assert_eq!(s.remove(negative).map(|i| i.0), Some(negative));
        assert!(s.get(negative).is_none());
    }

    /// **P0/C1 确定性对照**：id 分配 / 槽位复用序列必须与 **master 上记录的向量**逐项相等。
    ///
    /// 记录方式：改动前在 master 代码（`95fdbb68a`）上跑同一序列并打印（本卡 P0 报告附原始输出）。
    /// 牙：把 `take_free_slot` 改成「总是追加」⇒ `ins7` 预期复用槽位、实际变成 `[2, 7]` ⇒ 红；
    /// 把 `next_id` 推进从 `id + 1` 改成 `id` ⇒ 末尾 `alloc:100` 变成 `alloc:99` ⇒ 红。
    #[test]
    fn id_alloc_sequence_matches_master_vector() {
        let mut s = EcsStore::<Item>::new();
        let mut trace: Vec<String> = Vec::new();
        for _ in 0..3 {
            trace.push(format!("alloc:{}", s.alloc_id()));
        }
        s.insert(10, Item(10));
        trace.push(format!("after10:{:?}", s.order_index()));
        s.insert(2, Item(2));
        trace.push(format!("after2:{:?}", s.order_index()));
        s.remove(10);
        trace.push(format!("rm10:{:?}", s.order_index()));
        s.insert(7, Item(7));
        trace.push(format!("ins7:{:?}", s.order_index()));
        trace.push(format!("alloc:{}", s.alloc_id()));
        s.remove(3);
        trace.push(format!("rm3:{:?}", s.order_index()));
        s.insert(99, Item(99));
        trace.push(format!("ins99:{:?}", s.order_index()));
        trace.push(format!("alloc:{}", s.alloc_id()));
        trace.push(format!("ids_sorted:{:?}", s.ids_sorted()));
        // master `95fdbb68a` 记录向量（逐项相等，顺序也是判据）。
        let master: Vec<&str> = vec![
            "alloc:1",
            "alloc:2",
            "alloc:3",
            "after10:[10]",
            "after2:[10, 2]",
            "rm10:[2]",
            "ins7:[7, 2]",
            "alloc:11",
            "rm3:[7, 2]",
            "ins99:[7, 2, 99]",
            "alloc:100",
            "ids_sorted:[2, 7, 99]",
        ];
        assert_eq!(
            trace, master,
            "id 分配/槽位复用序列必须与 master 记录向量逐项相等"
        );
    }
}
