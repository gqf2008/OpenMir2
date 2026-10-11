//! S2 微基准（**隔离、可复现**）：把本轮两处热路径改动单独量出来。
//! `tests/parity/world/S2调优报告.md` 里的 ns/op 数字**由本命令生成**（不是手抄）。
//!
//! 命令：`cargo run -p mir2-world-bevy --release --bin ecs_microbench`
//!
//! 三段（全部单线程、无并行；只测，不做阈值判定）：
//!
//! 1. `id → 槽位`：`HashMap<i32, usize>`（SipHash，S1 形态）vs 稠密数组 `Vec<u32>`
//!    （`IdIndex` 形态）；
//! 2. 组件读取：`World::get::<T>(entity)`（每次查 `TypeId→ComponentId`，再走
//!    `entity → archetype → table row`）vs 缓存 `QueryState<&T>::get_manual`；
//! 3. 端到端：`EcsStore::get(id)`（= 第 1+2 段组合，即 `crates/world` 的
//!    `EntityStore::get` 实际走的那条路径）。
//!
//! 门禁仍是 `tick_bench` 的同会话 A/B（见 S2 报告 §3）；本 bench 只解释"为什么变快"。
//! 单位 ns/op，越小越好。

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

use bevy_ecs::prelude::*;
use mir2_world_bevy::EcsStore;

#[derive(Component)]
struct Item(u64);

/// 跑 `f` 若干次取平均（ns/op）。`f` 返回一个值交给 `black_box`，防止被优化掉。
fn bench_ns<F: FnMut(u64) -> u64>(iters: u64, mut f: F) -> f64 {
    // 预热
    for i in 0..(iters / 10).max(1) {
        black_box(f(i));
    }
    let t0 = Instant::now();
    let mut acc = 0u64;
    for i in 0..iters {
        acc = acc.wrapping_add(black_box(f(i)));
    }
    black_box(acc);
    t0.elapsed().as_nanos() as f64 / iters as f64
}

fn main() {
    const IDS: usize = 4096;

    // ---------- 1) id → 槽位：SipHash HashMap vs 稠密数组 ----------
    // 访问序列与 bench 里的 `get` 调用同形：伪随机跳（乘一个奇数再取模），
    // 避免"顺序访问把 HashMap 测得太好看"。
    let seq = |i: u64| -> i32 { (((i.wrapping_mul(2_654_435_761)) % IDS as u64) as i32) + 1 };

    let mut map: HashMap<i32, usize> = HashMap::with_capacity(IDS);
    let mut dense: Vec<u32> = vec![0; IDS + 1];
    for i in 0..IDS {
        map.insert(i as i32 + 1, i);
        dense[i + 1] = i as u32 + 1;
    }
    let map_ns = bench_ns(20_000_000, |i| *map.get(&seq(i)).unwrap() as u64);
    let dense_ns = bench_ns(20_000_000, |i| {
        let id = seq(i) as usize;
        (dense.get(id).copied().unwrap_or(0) - 1) as u64
    });

    // ---------- 2) 组件读取：World::get vs QueryState::get_manual ----------
    let mut world = World::new();
    let entities: Vec<Entity> = (0..IDS).map(|i| world.spawn(Item(i as u64)).id()).collect();
    let mut read_state = world.query::<&Item>();
    read_state.update_archetypes(&world);

    let world_get_ns = bench_ns(10_000_000, |i| {
        let e = entities[(i % IDS as u64) as usize];
        world.get::<Item>(e).unwrap().0
    });
    let get_manual_ns = bench_ns(10_000_000, |i| {
        let e = entities[(i % IDS as u64) as usize];
        read_state.get_manual(&world, e).unwrap().0
    });

    // ---------- 3) 端到端：EcsStore::get（S1 形态 = SipHash + World::get）----------
    // S1 形态无法在这个二进制里复原（字段已换掉），所以只报 `EcsStore::get` 的绝对值；
    // "S1 形态"的等价数字由 tick_bench 的同会话 A/B 提供（见 S2 报告 §3）。
    let mut store: EcsStore<Item> = EcsStore::new();
    for i in 0..IDS {
        store.insert(i as i32 + 1, Item(i as u64));
    }
    let store_get_ns = bench_ns(10_000_000, |i| store.get(seq(i)).unwrap().0);

    println!("S2 微基准（release，ns/op；越小越好）");
    println!(
        "{:<42} {:>10}",
        "id→槽位: HashMap<i32,usize> (SipHash)",
        fmt(map_ns)
    );
    println!(
        "{:<42} {:>10}",
        "id→槽位: 稠密数组 Vec<u32> (IdIndex)",
        fmt(dense_ns)
    );
    println!(
        "{:<42} {:>10}",
        "组件读取: World::get::<T>",
        fmt(world_get_ns)
    );
    println!(
        "{:<42} {:>10}",
        "组件读取: QueryState::get_manual",
        fmt(get_manual_ns)
    );
    println!(
        "{:<42} {:>10}",
        "端到端: EcsStore::get (IdIndex+get_manual)",
        fmt(store_get_ns)
    );
    println!(
        "参考: 以 SipHash+World::get 为 S1 形态的组合估算 ≈ {:.2} ns/op",
        map_ns + world_get_ns
    );
}

/// 三位小数打印（ns/op 的量级下第三位已进入噪声，但保留便于对照）。
fn fmt(ns: f64) -> String {
    format!("{ns:.3}")
}
