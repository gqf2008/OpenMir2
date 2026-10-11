//! S1 迁移门禁（集成测试）：ECS 内核的**顺序契约**与 tick 骨架的**线程断言**。
//!
//! 放在 `tests/`（不在 `src/`）是有意的：`src/tests/no_parallel.rs` 的源码扫描会禁
//! `std::thread::spawn`，而「把 world 挪到别的线程上跑必须 panic」这条反证**需要**开线程。

use mir2_world::{Entity, World};

/// 判据③：tick 必须在同一线程上串行——把 world 挪到别的线程上跑必须 panic。
#[test]
fn tick_refuses_to_run_on_another_thread() {
    let mut w = World::new(8, 8);
    w.tick();
    let handle = std::thread::spawn(move || {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.tick()));
        r.is_err()
    });
    assert!(
        handle.join().unwrap(),
        "tick 在别的线程上必须 panic（世界只能单线程串行）"
    );
}

/// 判据②（可证伪）：实体处理顺序来自**自己的槽位索引**，不是 ECS 的 Query/生成顺序。
///
/// 反例场景：插入 1/2/3 → 删 1 → 插入 4（复用 1 的槽）。槽位顺序是 `[4,2,3]`；
/// 而 bevy 的 archetype/table 顺序（swap-remove 后）通常是 `[3,2,4]`。
/// 若有人把 `EcsStore::iter`/`order_index` 改成迭代 `Query`，本测试立刻红。
#[test]
fn entity_order_follows_slots_not_query_order() {
    let mut store = mir2_world::EntityStore::new();
    for id in [1, 2, 3] {
        store.insert(Entity::new(id, format!("e{id}"), 0, 0));
    }
    store.remove(1);
    store.insert(Entity::new(4, "e4", 0, 0));

    assert_eq!(
        store.order_index(),
        vec![4, 2, 3],
        "处理顺序必须来自槽位索引（插入顺序 + 空闲槽 LIFO 复用）"
    );
    assert_eq!(
        store.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![4, 2, 3],
        "iter 必须与 order_index 同序（Query 迭代序不是契约）"
    );
}

/// tick 的 AOI 处理顺序同样来自槽位索引：删一个再补一个后，处理顺序按复用后的槽位排。
#[test]
fn tick_aoi_order_follows_slot_index_after_slot_reuse() {
    let mut w = World::new(16, 16);
    let a = w.add_entity(Entity::new(1, "a", 1, 1));
    w.add_entity(Entity::new(2, "b", 2, 1));
    w.add_entity(Entity::new(3, "c", 3, 1));
    w.tick(); // a/b/c 入图
    w.leave(a);
    w.tick(); // a 离开世界 ⇒ 槽位 0 释放
    let d = w.add_entity(Entity::new(4, "d", 4, 1)); // 复用槽位 0
    assert_eq!(d, 4);
    let rec = w.tick();
    assert_eq!(
        rec.aoi_order,
        vec![4, 2, 3],
        "AOI 处理顺序 = 槽位顺序（不是 ECS 内部顺序）"
    );
}

/// tick 骨架的系统（S2 起承载 AOI 顺序）按**显式 SlotOrder** 产出处理顺序：
/// 删一个再补一个后，顺序按复用后的槽位排（不迭代 Query 求顺序）。
#[test]
fn tick_order_skeleton_collects_slot_order() {
    let mut w = World::new(16, 16);
    let a = w.add_entity(Entity::new(1, "a", 1, 1));
    w.add_entity(Entity::new(2, "b", 2, 1));
    w.add_entity(Entity::new(3, "c", 3, 1));
    w.tick(); // 1/2/3 入图
    w.leave(a);
    w.tick(); // 1 离开世界 ⇒ 槽位 0 释放
    w.add_entity(Entity::new(4, "d", 4, 1)); // 复用槽位 0
    w.tick(); // 4 入图

    w.entities.sync_slot_order();
    let mut runner = mir2_world_bevy::TickRunner::new();
    runner.add_systems(mir2_world::collect_tick_order_system);
    runner.run(w.entities.world_mut());
    let order = w.entities.world().resource::<mir2_world_bevy::TickOrder>();
    assert_eq!(order.0, vec![4, 2, 3], "顺序来自槽位索引（空闲槽复用）");
}

/// tick 骨架（单线程 Schedule）推进虚拟时钟：与原实现同为 200ms/次、序号从 1 开始。
#[test]
fn tick_clock_is_advanced_by_single_threaded_skeleton() {
    let mut w = World::new(8, 8);
    let r1 = w.tick();
    assert_eq!((r1.tick_seq, r1.clock_ms), (1, 200));
    let r2 = w.tick();
    assert_eq!((r2.tick_seq, r2.clock_ms), (2, 400));
    assert_eq!((w.tick_seq, w.clock_ms), (2, 400));
}
