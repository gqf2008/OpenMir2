//! **S1 迁移内核 + D3 试验田**：`bevy_ecs` 承载「实体存储 + tick 骨架」（[`ecs`] / [`tick`]），
//! 以及 D3 的「掉落」系统（纯函数式：输入快照 → 输出命令）。
//!
//! 依赖方向（S1 起）：`crates/world` → `crates/world-bevy`（本 crate）。本 crate **不再**依赖
//! `crates/world`；原 `snapshot_from_world` 适配层已迁到 `crates/world`（见
//! `crates/world/src/bevy_bridge.rs`），以保证无环。
//!
//! ## 四条硬约束怎么落实（§4.2.2）
//!
//! 1. **单线程 executor**：显式用 [`bevy_ecs::schedule::SingleThreadedExecutor`]，
//!    绝不使用 `MultiThreadedExecutor`/`bevy_tasks` 并行（守卫：`tests/bevy_drop_parity.rs::no_parallel_guard`）；
//! 2. **遍历顺序显式化**：系统**不迭代任何 `Query`**——处理顺序来自我们自己的
//!    [`OrderIndex`] 资源（实体 id 升序，由调用方给出）。`Query` 迭代序在 bevy 里不是契约
//!    （archetype/table 顺序），本 crate 一处都不用；
//! 3. **确定性守卫升级**：本 crate 自己的 `bevy_drop_parity.rs::no_parallel_guard` 把白名单放宽为
//!    "只允许 `bevy_ecs`/`bevy_app`"（仍旧禁 rayon/crossbeam/bevy_tasks 并行 API），
//!    并保留"tick 必须在同一线程"的运行期断言思路（见测试）；
//! 4. **迁移验收＝原判据不降级**：S1 的实体存储/tick 骨架与原 `EntityStore`/`World::tick` 语义
//!    逐条一致（槽位顺序、空闲槽复用、代际号、时钟/线程断言），三个 golden（basic/session/combat）
//!    逐行不变；D3 的对拍断言「同一输入下，本系统输出的命令与既有实现逐项相同」也保持。
//!
//! ## 为什么第 2 条不是形式主义
//!
//! 掉落预算要按 C# 的**调用顺序**消耗同一个随机源；`Query` 迭代序一变，随机数落到不同
//! 怪物头上 ⇒ 与 C# 的逐行 diff 立刻破。所以顺序只能来自我们自己的索引。

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::SingleThreadedExecutor;

use mir2_data::models::{MonsterDropItem, StdItem, STRING_GOLD_NAME};
use mir2_formula::drop::{mon_get_random_items, ItemNumberCounter, UserItem, VecItemCatalog};
use mir2_shared::rng::RandomNumber;

// ============================ S1 迁移内核（实体存储 + tick 骨架） ============================
//
// 卡片 `openmir2-world-bevy-migration` 的 S1：`crates/world` 的**实体存储**与**tick 骨架**
// 迁到 `bevy_ecs`。内核与 D3 的掉落系统同 crate、同约束：单线程 executor + 自有序索引。
pub mod ecs;
pub mod tick;

pub use ecs::{EcsStore, SlotOrder};
pub use tick::{advance_clock_system, TickClock, TickOrder, TickOwnerThread, TickRunner};

// ============================ 组件 / 资源 ============================

/// 怪物的业务 id（组件）——系统输出命令时带上它，避免依赖 ECS 的 `Entity` 句柄顺序。
#[derive(Component, Clone, Copy, Debug)]
pub struct MonsterId(pub i32);

/// 怪物实体上的掉落表（组件）。
#[derive(Component, Clone, Debug)]
pub struct DropTable(pub Vec<MonsterDropItem>);

/// 怪物在地图上的位置（组件）。
#[derive(Component, Clone, Copy, Debug)]
pub struct Atlas {
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
}

/// 生成时预算出的掉落（组件；系统产出、由适配层写回既有世界）。
#[derive(Component, Clone, Debug, Default)]
pub struct DropBudget {
    pub gold: i32,
    pub items: Vec<UserItem>,
}

/// 随机源（资源）。同种子 ⇒ 与既有实现、与 C# 同流。
#[derive(Resource)]
pub struct RngRes(pub RandomNumber);

/// 物品目录（资源）：掉落生成按名查 `StdItem`。
#[derive(Resource)]
pub struct CatalogRes(pub Vec<StdItem>);

/// `Config.MonRandomAddValue`（资源）。
#[derive(Resource, Clone, Copy)]
pub struct MonRandomAddValue(pub i32);

/// 掉落物品的 `MakeIndex` 计数器（资源，对应 `M2Share.GetItemNumber`）。
#[derive(Resource, Default)]
pub struct ItemNumberRes(pub ItemNumberCounter);

/// **显式处理顺序**（资源）：实体 id 升序。系统只按这个顺序处理，不碰 `Query` 迭代序。
#[derive(Resource, Default)]
pub struct OrderIndex(pub Vec<Entity>);

/// 输出命令（资源）：本系统唯一的产出面。
#[derive(Resource, Default)]
pub struct OutBudgets(pub Vec<(i32, DropBudget)>);

// ============================ 纯函数式系统 ============================

/// 掉落预算系统：按 [`OrderIndex`] 顺序，为每个带 [`DropTable`] 的实体算出 [`DropBudget`]。
///
/// 随机数消耗顺序与 C# `WorldServer.MonGen.cs::MonGetRandomItems` 逐条一致
/// （公式本体来自 `crates/formula`，已与 C# 对拍）。
// bevy 系统的参数列表 = 它声明的数据访问（resource/query/commands），
// 参数多是 ECS 的常态而不是坏味道；clippy 的默认阈值不适用于 System。
#[allow(clippy::too_many_arguments)]
pub fn drop_budget_system(
    mut out: ResMut<OutBudgets>,
    order: Res<OrderIndex>,
    catalog: Res<CatalogRes>,
    add_value: Res<MonRandomAddValue>,
    mut rng: ResMut<RngRes>,
    mut item_number: ResMut<ItemNumberRes>,
    mut commands: Commands,
    tables: Query<(&MonsterId, &DropTable, Option<&DropBudget>)>,
) {
    // 注意：这里**只**用 `order.0` 决定顺序；`tables` 查询只用于按 Entity 取组件，
    // 不迭代它（`Query` 迭代序不是契约）。
    for entity in order.0.iter() {
        let Ok((monster_id, table, existing)) = tables.get(*entity) else {
            continue;
        };
        if existing.is_some() {
            continue; // 已有预算则不重复（幂等）
        }
        let catalog_ref = VecItemCatalog { items: &catalog.0 };
        let result = mon_get_random_items(
            &table.0,
            &catalog_ref,
            &mut rng.0,
            add_value.0,
            STRING_GOLD_NAME,
            &mut item_number.0,
        );
        let budget = DropBudget {
            gold: result.gold,
            items: result.items,
        };
        out.0.push((monster_id.0, budget.clone()));
        commands.entity(*entity).insert(budget);
    }
}

// ============================ 输入快照 / 输出命令 ============================

/// 输入快照（纯数据，不含任何 ECS 类型）：一次 tick 里所有需要预算掉落的怪物。
#[derive(Clone, Debug, Default)]
pub struct DropSnapshot {
    pub seed: i32,
    pub mon_random_add_value: i32,
    pub catalog: Vec<StdItem>,
    /// **顺序即处理顺序**（调用方给，通常 = 实体 id 升序）
    pub monsters: Vec<MonsterSnap>,
}

#[derive(Clone, Debug)]
pub struct MonsterSnap {
    pub id: i32,
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    pub drop_table: Vec<MonsterDropItem>,
}

/// 输出命令：预算结果（对应既有实现的 `Entity.gold` / `Entity.drop_items`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropPlanOut {
    pub id: i32,
    pub gold: i32,
    pub items: Vec<UserItem>,
}

/// 输出命令：把掉落物落到地上（对应既有实现的 `place_floor_item`）。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FloorPlaceOut {
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    pub obj_id: i32,
}

/// **纯函数式入口（预算）**：输入快照 → 输出命令。
///
/// 内部构造一个 `bevy_ecs::World`，用**单线程** `Schedule` 跑一次 [`drop_budget_system`]。
/// 除随机源外无任何隐藏状态：同快照 + 同种子 ⇒ 同输出。
///
/// **处理顺序**：按**实体 id 升序**建立 [`OrderIndex`]（与快照里数组的先后无关）。
/// 这一点是硬要求：掉落消耗同一条随机流，顺序一变结果就变（与 C# 的逐行 diff 会立刻破）。
/// 因此"ECS 的 spawn 顺序 / `Query` 迭代序"对本函数的输出**没有任何影响**。
pub fn plan_drops(snapshot: &DropSnapshot) -> Vec<DropPlanOut> {
    let mut world = World::new();
    world.insert_resource(RngRes(RandomNumber::with_seed(snapshot.seed)));
    world.insert_resource(CatalogRes(snapshot.catalog.clone()));
    world.insert_resource(MonRandomAddValue(snapshot.mon_random_add_value));
    world.insert_resource(ItemNumberRes::default());
    world.insert_resource(OutBudgets::default());

    // spawn 顺序 = 快照里的顺序（无所谓）；处理顺序 = 实体 id 升序（我们的索引说了算）
    let mut entity_of: Vec<(i32, Entity)> = Vec::with_capacity(snapshot.monsters.len());
    for m in &snapshot.monsters {
        let e = world
            .spawn((
                MonsterId(m.id),
                DropTable(m.drop_table.clone()),
                Atlas {
                    map_id: m.map_id,
                    x: m.x,
                    y: m.y,
                },
            ))
            .id();
        entity_of.push((m.id, e));
    }
    let mut sorted = entity_of.clone();
    sorted.sort_by_key(|(id, _)| *id);
    world.insert_resource(OrderIndex(sorted.into_iter().map(|(_, e)| e).collect()));

    let mut schedule = Schedule::default();
    schedule.set_executor(SingleThreadedExecutor::default());
    schedule.add_systems(drop_budget_system);
    schedule.run(&mut world);

    let out = world.resource::<OutBudgets>();
    out.0
        .iter()
        .map(|(id, budget)| DropPlanOut {
            id: *id,
            gold: budget.gold,
            items: budget.items.clone(),
        })
        .collect()
}

/// **常驻 world 形态**（真实集成用的形态）：world 建一次，之后每批只重置状态并重跑系统。
///
/// 与 [`plan_drops`] 的区别只在"是否重建 world"；数值输出必须与 [`plan_drops`] 逐项相同
/// （由 `bevy_drop_parity::persistent_world_matches_fresh_world` 断言）。
pub fn plan_drops_in_world(world: &mut World, snapshot: &DropSnapshot) -> Vec<DropPlanOut> {
    // 重置随机源/计数器/输出（每次预算都从同一种子重来 ⇒ 与 plan_drops 同流）
    world.insert_resource(RngRes(RandomNumber::with_seed(snapshot.seed)));
    world.insert_resource(ItemNumberRes::default());
    world.insert_resource(OutBudgets::default());
    // 清掉上一轮的预算组件（幂等语义：系统只在没有 DropBudget 时计算）
    let order: Vec<Entity> = world.resource::<OrderIndex>().0.clone();
    for e in order {
        world.entity_mut(e).remove::<DropBudget>();
    }
    let mut schedule = Schedule::default();
    schedule.set_executor(SingleThreadedExecutor::default());
    schedule.add_systems(drop_budget_system);
    schedule.run(world);
    let out = world.resource::<OutBudgets>();
    out.0
        .iter()
        .map(|(id, b)| DropPlanOut {
            id: *id,
            gold: b.gold,
            items: b.items.clone(),
        })
        .collect()
}

/// 建一个常驻 world（把快照里的怪物一次装进去）；之后用 [`plan_drops_in_world`] 反复跑。
pub fn build_world(snapshot: &DropSnapshot) -> World {
    let mut world = World::new();
    world.insert_resource(CatalogRes(snapshot.catalog.clone()));
    world.insert_resource(MonRandomAddValue(snapshot.mon_random_add_value));
    world.insert_resource(RngRes(RandomNumber::with_seed(snapshot.seed)));
    world.insert_resource(ItemNumberRes::default());
    world.insert_resource(OutBudgets::default());
    let mut entity_of: Vec<(i32, Entity)> = Vec::with_capacity(snapshot.monsters.len());
    for m in &snapshot.monsters {
        let e = world
            .spawn((
                MonsterId(m.id),
                DropTable(m.drop_table.clone()),
                Atlas {
                    map_id: m.map_id,
                    x: m.x,
                    y: m.y,
                },
            ))
            .id();
        entity_of.push((m.id, e));
    }
    entity_of.sort_by_key(|(id, _)| *id);
    world.insert_resource(OrderIndex(entity_of.into_iter().map(|(_, e)| e).collect()));
    world
}

/// 落地快照：把预算好的掉落物分散到空格子。
#[derive(Clone, Debug, Default)]
pub struct PlaceSnapshot {
    /// 初始被占用的格子（来自既有世界的地图；`(map_id, x, y)`）
    pub occupied: Vec<(usize, i32, i32)>,
    /// 顺序即处理顺序
    pub drops: Vec<DropAt>,
}

#[derive(Clone, Debug)]
pub struct DropAt {
    pub id: i32,
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    /// 依次尝试落地的对象 id（物品用 `index`、金币用怪物 id —— 与既有实现一致）
    pub obj_ids: Vec<i32>,
}

/// **纯函数式入口（落地）**：输入快照 → 输出命令。
///
/// 扫描顺序完全照 `BaseObject.GetDropPosition` 的子集：`for i in 0..range { for ii in -i..=i { for iii in -i..=i } }`，
/// 取第一个空格子；每落一件就把它记为已占（同一批内后续件不会再选同一格）。
pub fn plan_floor_placement(snapshot: &PlaceSnapshot) -> Vec<FloorPlaceOut> {
    let mut occupied: std::collections::HashSet<(usize, i32, i32)> =
        snapshot.occupied.iter().copied().collect();
    let mut out = Vec::new();
    for d in &snapshot.drops {
        for obj_id in &d.obj_ids {
            let mut placed_at = None;
            'scan: for i in 0..3 {
                for ii in -i..=i {
                    for iii in -i..=i {
                        let px = d.x + iii + 1;
                        let py = d.y + ii + 1;
                        if !occupied.contains(&(d.map_id, px, py)) {
                            placed_at = Some((px, py));
                            break 'scan;
                        }
                    }
                }
            }
            if let Some((px, py)) = placed_at {
                occupied.insert((d.map_id, px, py));
                out.push(FloorPlaceOut {
                    map_id: d.map_id,
                    x: px,
                    y: py,
                    obj_id: *obj_id,
                });
            }
        }
    }
    out
}

// 注：D3 里的 `snapshot_from_world(&mir2_world::World, seed)` 适配层需要 `mir2-world`，
// 而 S1 起依赖方向翻转为 `mir2-world -> mir2-world-bevy`（避免环）。该函数已迁到
// `crates/world/src/bevy_bridge.rs::snapshot_from_world`（行为不变，仅换 crate）。
