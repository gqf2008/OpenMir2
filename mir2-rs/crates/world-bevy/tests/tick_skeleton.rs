//! S1 tick 骨架门禁：单线程推进 + 换线程 panic。
//!
//! 放在 `tests/`（不在 `src/`）：`no_parallel.rs`/`bevy_drop_parity.rs` 的源码扫描会禁
//! `std::thread::spawn`，而「把骨架挪到别的线程上必须 panic」这条反证需要开线程。

use bevy_ecs::prelude::World;
use mir2_world_bevy::{advance_clock_system, TickClock, TickOwnerThread, TickRunner};

fn pinned_runner() -> TickRunner {
    let mut runner = TickRunner::new();
    runner.add_systems(advance_clock_system);
    runner
}

#[test]
fn clock_advances_and_thread_is_pinned() {
    let mut world = World::new();
    world.insert_resource(TickClock::new(200));
    world.insert_resource(TickOwnerThread::default());
    let mut runner = pinned_runner();

    runner.run(&mut world);
    runner.run(&mut world);
    let clock = *world.resource::<TickClock>();
    assert_eq!(clock.clock_ms, 400);
    assert_eq!(clock.tick_seq, 2);

    // 换线程 ⇒ 断言必然触发（catch_unwind 需要 UnwindSafe，用 AssertUnwindSafe 包住）
    let mut moved_world = world;
    let hit = std::thread::spawn(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pinned_runner().run(&mut moved_world);
        }))
        .is_err()
    })
    .join()
    .unwrap();
    assert!(hit, "tick 骨架在别的线程上必须 panic");
}
