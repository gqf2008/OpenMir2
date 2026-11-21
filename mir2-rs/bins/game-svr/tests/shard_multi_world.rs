//! **P2-3b 构造级判据**：同进程两个 World 的线程归属与地图分片。
//!
//! 钉三件事：
//! 1. 两个 World 在**不同**线程上首次 tick（各自 `TickOwnerThread` / `World.owner_thread`）；
//! 2. 同一个 World 换线程再 tick ⇒ **panic**（线程亲和断言保留，不允许迁移）；
//! 3. 实体按地图分片：各自拥有的图可进、对方图 `MapNotInShard`；tick 不触发
//!    `assert_no_entity_on_unowned_map`（debug）。

use game_svr::shard::{ShardId, ShardLayout, ShardRuntime};
use mir2_world_bevy::{EntitySpawn, SessionError, World};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}

fn spawn(name: &str) -> EntitySpawn {
    EntitySpawn::new(0, name.to_string(), 4, 4)
}

/// 按布局造「某片的 World」：全长度尺寸表（自己的图真实、其它图 1×1 stub）+ 归属位图。
fn build(layout: &ShardLayout, shard: ShardId, all: &[(i32, i32)]) -> World {
    let runtime = ShardRuntime::for_shard(layout.clone(), shard);
    let mut world = World::with_maps(runtime.local_map_sizes(all));
    world.set_map_ownership(&runtime.local_ownership_bitmap());
    world
}

#[test]
fn n2_worlds_are_thread_pinned_and_entities_partition_by_map() {
    let layout = ShardLayout::partition(2, &names(&["A", "B", "C", "D"]), Some("0:A,B;1:*"))
        .expect("合法 N=2 划分");
    let all = vec![(16, 16), (17, 17), (18, 18), (19, 19)];
    let mut w0 = build(&layout, ShardId::new(0), &all);
    let mut w1 = build(&layout, ShardId::new(1), &all);

    // 各自进自己的图。
    w0.open_session("h0");
    assert!(w0.enter_map("h0", spawn("h0"), 0).is_ok(), "片 0 的图 A");
    w1.open_session("h1");
    assert!(w1.enter_map("h1", spawn("h1"), 2).is_ok(), "片 1 的图 C");

    // 对方的图 ⇒ `MapNotInShard`（不是 NoSuchMap：图存在、不归本 World）。
    for (world, name, foreign) in [(&mut w0, "x0", 2usize), (&mut w1, "x1", 0usize)] {
        world.open_session(name);
        let err = world
            .enter_map(name, spawn(name), foreign)
            .expect_err("跨片图必须拒绝");
        assert!(matches!(err, SessionError::MapNotInShard), "{err:?}");
    }

    // 两个线程各自首次 tick ⇒ 线程 id 不同；tick 内不变式（stub 上无实体）不触发。
    let t0 = std::thread::spawn(move || {
        w0.tick();
        (std::thread::current().id(), w0)
    })
    .join()
    .expect("片 0 首次 tick");
    let t1 = std::thread::spawn(move || {
        w1.tick();
        (std::thread::current().id(), w1)
    })
    .join()
    .expect("片 1 首次 tick");
    assert_ne!(t0.0, t1.0, "两个 World 必须在不同 worker 线程上 tick");

    // 同一个 World 换线程再 tick ⇒ panic（`World::tick_head` 的 owner_thread 断言）。
    // 红检：删掉 `World::tick_head` 开头那段 `owner_thread` 断言 ⇒ 本断言红。
    let (_, mut w0) = t0;
    let res = std::thread::spawn(move || {
        w0.tick();
    })
    .join();
    assert!(
        res.is_err(),
        "同一个 World 跨线程 tick 必须 panic（线程亲和）"
    );
}
