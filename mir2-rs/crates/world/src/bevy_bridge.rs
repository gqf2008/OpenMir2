//! S1：ECS 内核（`crates/world-bevy`）与既有世界（本 crate）的**薄适配层**。
//!
//! 这里只放两类东西：
//! 1. tick 骨架的系统：`collect_tick_order_system` —— 按**显式** [`SlotOrder`]（槽位顺序）
//!    产出本 tick 的处理顺序，**不迭代 `Query` 求顺序**（`Query` 只用于按句柄取组件）；
//! 2. D3 掉落系统原来的 `snapshot_from_world` 适配层（S1 起依赖方向翻转为
//!    `mir2-world -> mir2-world-bevy`，故从 `crates/world-bevy` 迁到这里；行为不变）。

use bevy_ecs::prelude::*;

use mir2_world_bevy::{DropSnapshot, MonsterSnap, SlotOrder, TickOrder};

use crate::entity::Entity;
use crate::world::World;

/// tick 骨架系统：按显式 [`SlotOrder`]（槽位顺序）产出本 tick 的处理顺序（业务 id）。
///
/// **顺序契约**：只按 `order.0` 走；`entities` 查询只用于按句柄取值。
pub fn collect_tick_order_system(
    order: Res<SlotOrder>,
    mut out: ResMut<TickOrder>,
    entities: Query<&Entity>,
) {
    out.0.clear();
    for e in order.0.iter() {
        if let Ok(ent) = entities.get(*e) {
            out.0.push(ent.id);
        }
    }
}

/// 薄适配层：从既有世界（`crates/world`）读"要预算掉落的东西"，拼成快照。
///
/// 只读、不改；顺序按实体 id 升序（= 既有世界的槽位顺序口径）。
pub fn snapshot_from_world(world: &World, seed: i32) -> DropSnapshot {
    let mut monsters: Vec<MonsterSnap> = Vec::new();
    for id in world.entities.ids_sorted() {
        let Some(e) = world.entities.get(id) else {
            continue;
        };
        monsters.push(MonsterSnap {
            id: e.id,
            map_id: e.map_id,
            x: e.x,
            y: e.y,
            drop_table: Vec::new(), // 掉落表由调用方补齐（世界在生成时已消费它）
        });
    }
    DropSnapshot {
        seed,
        mon_random_add_value: world.mon_random_add_value,
        catalog: world.item_catalog.clone(),
        monsters,
    }
}
