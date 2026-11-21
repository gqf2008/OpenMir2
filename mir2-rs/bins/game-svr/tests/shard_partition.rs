//! **P2-2 / P2-3a 构造级判据**：地图组划分 + 全局 `map_id` 归属模型。
//!
//! 这组用例钉的是「拒绝来自结构，不是运行期逐点守卫」：
//! - 布局给出两个不相交组；
//! - 宿主 World 的 `maps` **全长度保序**：片 0 真实尺寸 + 其它片 1×1 stub；
//! - `owned_maps` 位图由布局驱动；片 1 的 `map_id` 进 `enter_map` ⇒ `MapNotInShard`
//!   （可区分错误，不是 `NoSuchMap`）；
//! - 片 0 的 `map_id` 可正常进入（对照组，证明拒绝不是「所有图都进不去」）。

use game_svr::shard::{ShardId, ShardLayout, ShardRuntime};
use mir2_world_bevy::{EntitySpawn, SessionError, World};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}

fn spawn(name: &str) -> EntitySpawn {
    EntitySpawn::new(0, name.to_string(), 4, 4)
}

#[test]
fn n2_host_world_owns_only_its_map_group_with_stubs_for_others() {
    let maps = names(&["A", "B", "C", "D"]);
    let layout = ShardLayout::partition(2, &maps, Some("0:A,B;1:*")).expect("合法 N=2 划分");
    assert_eq!(layout.maps_of(ShardId::new(0)), &[0, 1]);
    assert_eq!(layout.maps_of(ShardId::new(1)), &[2, 3], "片 1 = 其余地图");

    // 宿主 World 拿**全长度**尺寸表（P2-3a）：片 0 真实尺寸 + 其它片 1×1 stub。
    let all_sizes = vec![(16, 16), (17, 17), (18, 18), (19, 19)];
    let host_sizes = layout.host_map_sizes(&all_sizes);
    assert_eq!(
        host_sizes,
        vec![(16, 16), (17, 17), (1, 1), (1, 1)],
        "全局下标保序；非拥有图 = 1×1 stub"
    );
    let mut world = World::with_maps(host_sizes);
    assert_eq!(world.maps.len(), 4, "宿主 World 保持全长度下标空间");
    world.set_map_ownership(&layout.host_ownership());
    assert!(world.is_map_owned(0) && world.is_map_owned(1));
    assert!(!world.is_map_owned(2) && !world.is_map_owned(3));

    // 片 1 的 map_id 在全局表里存在，但本 World 不拥有 ⇒ 可区分错误 `MapNotInShard`。
    // （`enter_map` 要求先 `open_session`；失败路径在归属校验处返回，不动地图表。）
    for foreign in [2usize, 3] {
        let name = format!("p{foreign}");
        world.open_session(&name);
        let err = world
            .enter_map(&name, spawn(&name), foreign)
            .expect_err("跨片 map_id 必须进不了宿主 World");
        assert!(
            matches!(err, SessionError::MapNotInShard),
            "期望 MapNotInShard（map_id={foreign}），实得 {err:?}"
        );
    }

    // 对照组：片 0 的地图可正常进入。
    world.open_session("hero");
    let ok = world.enter_map("hero", spawn("hero"), 0);
    assert!(ok.is_ok(), "片 0 地图必须可进入：{ok:?}");

    // 全长度（含 stub）地图表上跑一个 tick 不 panic，且不变量断言不触发。
    world.tick();
}

#[test]
fn n2_runtime_guards_reject_cross_shard_routes() {
    let maps = names(&["A", "B", "C"]);
    let layout = ShardLayout::partition(2, &maps, Some("0:A;1:*")).expect("合法 N=2 划分");
    let runtime = ShardRuntime::new(layout);
    assert!(runtime.host_owns(0));
    assert!(!runtime.host_owns(1));
    assert!(runtime.route_allowed(0, 0), "片内连接点放行");
    assert!(!runtime.route_allowed(0, 1), "片 0 → 片 1 的连接点被拒");
    assert!(!runtime.route_allowed(1, 2), "源不在宿主片也不注册");
}

#[test]
fn single_shard_keeps_the_full_map_table() {
    let sizes = vec![(16, 16), (17, 17), (18, 18)];
    let layout = ShardLayout::single(sizes.len());
    assert_eq!(
        layout.host_map_sizes(&sizes),
        sizes,
        "MIR2_SHARDS=1：宿主尺寸表与注册表逐元素相同（零行为变化）"
    );
    let mut world = World::with_maps(layout.host_map_sizes(&sizes));
    world.set_map_ownership(&layout.host_ownership());
    assert_eq!(world.maps.len(), 3, "单片不裁切");
    assert_eq!(
        layout.host_ownership(),
        vec![true, true, true],
        "单片 = 全拥有（零行为变化）"
    );
    for map_id in 0..3 {
        let name = format!("p{map_id}");
        world.open_session(&name);
        assert!(
            world.enter_map(&name, spawn(&name), map_id).is_ok(),
            "单片下每张图都可进入（map_id={map_id}）"
        );
    }
}

/// **P3-1 牙（follow-up）**：`sim_loop` 的登录进图路径必须在 `world.enter_map_at` **之前**
/// 调用跨片 guard。删掉新增的 guard 臂 ⇒ 生产段里 `guard_cross_shard_enter(` 消失，本用例红。
///
/// 为什么需要这条：`NoSuchMap` 与 `route_allowed` 的断言只钉「World 没有这张图」，
/// 删掉 guard 后世界仍会返回 `NoSuchMap` ⇒ 假绿；这里钉的是「guard 真的接在进图之前」。
#[test]
fn sim_loop_enter_path_calls_cross_shard_guard_before_world_enter() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .expect("读 lib.rs");
    let prod_end = src.find("mod tests {").expect("`mod tests {` 行");
    let prod = &src[..prod_end];
    let guard = prod
        .find("guard_cross_shard_enter(")
        .expect("跨片登图 guard 必须存在（删掉 guard 臂 ⇒ 本条红）");
    let enter = prod
        .find("world.enter_map_at(")
        .expect("登录进图 call（world.enter_map_at）必须存在");
    assert!(
        guard < enter,
        "guard 必须在 world.enter_map_at 之前注册（guard={guard} enter={enter}）"
    );
    let arm = prod[..guard]
        .rfind("Ok((final_map_name, map_id))")
        .expect("guard 必须写在登录 Ok 臂内");
    assert!(
        guard - arm < 2000,
        "guard 必须紧邻登录 Ok 臂（arm={arm} guard={guard}）"
    );
}

/// **P2-3b Step 1**：非 0 片 runtime + World——自己的图可进入、别人的图 `MapNotInShard`。
///
/// 红检：让 `ShardRuntime::for_shard` 恒用片 0（忽略参数）⇒ 尺寸表/归属断言与进图断言全红。
#[test]
fn shard_one_world_uses_local_ownership_and_refuses_foreign_maps() {
    let maps = names(&["A", "B", "C", "D"]);
    let layout = ShardLayout::partition(2, &maps, Some("0:A,B;1:*")).expect("N=2 划分");
    let runtime = ShardRuntime::for_shard(layout.clone(), ShardId::new(1));
    assert_eq!(runtime.local_shard(), ShardId::new(1));
    let sizes = layout.local_map_sizes(ShardId::new(1), &[(16, 16), (17, 17), (18, 18), (19, 19)]);
    assert_eq!(
        sizes,
        vec![(1, 1), (1, 1), (18, 18), (19, 19)],
        "片 1 的世界：片 0 的图是 stub，自己的图取真实尺寸"
    );
    let mut world = World::with_maps(sizes);
    world.set_map_ownership(&layout.local_ownership(ShardId::new(1)));
    assert!(!world.is_map_owned(0) && !world.is_map_owned(1));
    assert!(world.is_map_owned(2) && world.is_map_owned(3));

    // 片 0 的图 ⇒ `MapNotInShard`（片 1 的 World）。
    for foreign in [0usize, 1] {
        let name = format!("s1-{foreign}");
        world.open_session(&name);
        let err = world
            .enter_map(&name, spawn(&name), foreign)
            .expect_err("片 0 的图不属于片 1");
        assert!(matches!(err, SessionError::MapNotInShard), "{err:?}");
    }
    // 片 1 自己的图可进入；全长度（含 stub）表上 tick 不触发不变式。
    world.open_session("s1-hero");
    assert!(world.enter_map("s1-hero", spawn("s1-hero"), 2).is_ok());
    world.tick();
}

/// **P2-3b Step 1**：单片布局下 `for_shard(0)` 与 `new` 完全相同（零行为变化）。
#[test]
fn single_shard_for_shard_zero_is_identity() {
    let sizes = vec![(16, 16), (17, 17)];
    let layout = ShardLayout::single(sizes.len());
    let runtime = ShardRuntime::for_shard(layout.clone(), ShardId::new(0));
    assert_eq!(runtime, ShardRuntime::new(layout.clone()));
    assert_eq!(layout.local_map_sizes(ShardId::new(0), &sizes), sizes);
    assert_eq!(layout.local_ownership(ShardId::new(0)), vec![true, true]);
}
