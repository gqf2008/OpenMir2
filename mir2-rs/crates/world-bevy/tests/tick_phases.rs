//! **L2-3 恒等牙**：`World::tick()` 与「显式 `tick_phase1() + tick_phase2()`」必须
//! **逐字段**产生同一个 `TickRecord`（断言，不是打印比对）。
//!
//! 两阶段拆分若把任一阶段挪出/挪进（或改了阶段序），本用例必红。

use mir2_world_bevy::{EntitySpawn, World};

fn make_world() -> World {
    let mut world = World::new(20, 20);
    for (id, name, x, y) in [(1, "m1", 3, 3), (2, "m2", 10, 10)] {
        let mut e = EntitySpawn::new(id, name.to_string(), x, y);
        e.race = 80;
        world.add_entity(e);
    }
    world.enable_monster_ai();
    world
}

#[test]
fn tick_equals_phase1_plus_phase2_record_by_record() {
    let mut combined = make_world();
    let mut split = make_world();
    for i in 1..=3 {
        let expected = combined.tick();
        let mut actual = split.tick_phase1();
        split.tick_phase2(&mut actual);
        assert_eq!(
            expected, actual,
            "第 {i} 个 tick：tick() 与 phase1+phase2 的 TickRecord 必须逐字段相等"
        );
        assert_eq!(
            combined.last_tick, split.last_tick,
            "第 {i} 个 tick 的 last_tick 必须相等"
        );
    }
}
