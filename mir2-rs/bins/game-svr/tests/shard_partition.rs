//! **P2-2 构造级判据**（卡 `openmir2-shard-map-partition`）：地图组划分之后，
//! 宿主 World **物理上只有片 0 的注册表前缀**，跨片 `map_id` 由既有 `NoSuchMap` 门拒绝。
//!
//! 这组用例钉的是「拒绝来自结构，不是运行期逐点守卫」：
//! - 布局给出两个不相交组；
//! - 宿主 World 的 `maps.len()` 只到片 0 的前缀；
//! - 片 1 的 `map_id` 进 `enter_map` ⇒ `NoSuchMap`；
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
fn n2_host_world_holds_only_the_host_map_prefix() {
    let maps = names(&["A", "B", "C", "D"]);
    let layout = ShardLayout::partition(2, &maps, Some("0:A,B;1:*")).expect("合法 N=2 划分");
    assert_eq!(layout.maps_of(ShardId::new(0)), &[0, 1]);
    assert_eq!(layout.maps_of(ShardId::new(1)), &[2, 3], "片 1 = 其余地图");

    // 宿主 World 只拿片 0 的尺寸表（前缀裁切；与 `sim_loop` 的接线同一条路径）。
    let all_sizes = vec![(16, 16), (17, 17), (18, 18), (19, 19)];
    let host_sizes = layout.host_map_sizes(&all_sizes);
    assert_eq!(host_sizes, vec![(16, 16), (17, 17)]);
    let mut world = World::with_maps(host_sizes);
    assert_eq!(world.maps.len(), 2, "宿主 World 只装片 0 的两张图");

    // 片 1 的 map_id 在布局里存在，但宿主 World 没有 ⇒ 既有 `NoSuchMap` 门拒绝。
    // （`enter_map` 要求先 `open_session`；失败路径在 map 校验处返回，不改地图表。）
    for foreign in [2usize, 3] {
        let name = format!("p{foreign}");
        world.open_session(&name);
        let err = world
            .enter_map(&name, spawn(&name), foreign)
            .expect_err("跨片 map_id 必须进不了宿主 World");
        assert!(
            matches!(err, SessionError::NoSuchMap),
            "期望 NoSuchMap（map_id={foreign}），实得 {err:?}"
        );
    }

    // 对照组：片 0 的地图可正常进入。
    world.open_session("hero");
    let ok = world.enter_map("hero", spawn("hero"), 0);
    assert!(ok.is_ok(), "片 0 地图必须可进入：{ok:?}");

    // 前缀地图表上跑一个 tick 不 panic（tick 不依赖全注册表）。
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
    assert_eq!(world.maps.len(), 3, "单片不裁切");
    for map_id in 0..3 {
        let name = format!("p{map_id}");
        world.open_session(&name);
        assert!(
            world.enter_map(&name, spawn(&name), map_id).is_ok(),
            "单片下每张图都可进入（map_id={map_id}）"
        );
    }
}
