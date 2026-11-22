//! **L2-3 构造级判据**：单图双区的两阶段 tick + 边界 halo。
//!
//! 场景（20×10 图，界 x=9|10）：
//! - 区 A = `[0..9]×[0..9]` 拥有观察者 O(9,5)（`view_range=3`）与静止 E_A(8,5)；
//! - 区 B = `[10..19]×[0..9]` 拥有静止 E_B(10,5) 与移动 M_B(10,7)；
//! - measured tick 3：M_B 移到 (14,7)（仍在 B 内，但距 O 5 格 ⇒ 移出视野）。
//!
//! 正确路径：每区 `phase1 → 取本 tick 快照 → barrier → 读邻居 → phase2 → barrier`。
//! 陈旧路径（负例牙）：快照在 phase1 **之前**取（上一 tick）⇒ 移动后一拍可见集合仍含 M_B。
//!
//! 判据：
//! 1. 正确路径每 tick 的可见集合 == 单 World 全区基线；
//! 2. 每个可见 actor 恰来自一个来源（`dup=0`：本地集 ∩ halo 集 = ∅；`never=0`：并集 == 基线）；
//! 3. 陈旧路径在移动后的一拍**必须**与基线不同（若有人把陈旧路径改成同序 ⇒ 本牙红）；
//! 4. 同一收件人（O）的 appear/disappear 有序序列 == 基线。

use std::sync::mpsc;
use std::sync::Arc;

use game_svr::region_loop::{
    snapshot_border, HaloExchange, HaloFrameSource, HaloSnapshot, TickBarriers,
};
use game_svr::zone::{ZoneId, ZoneRect};
use mir2_world_bevy::{EntitySpawn, World};

const MAP_W: i32 = 20;
const MAP_H: i32 = 10;
const HALO: i32 = 3;
const OBSERVER_ID: i32 = 1;
const OBSERVER_VIEW: i32 = 3;
const E_A_ID: i32 = 2;
const E_B_ID: i32 = 3;
const M_B_ID: i32 = 4;
const MOVE_TICK: u64 = 3;
const TICKS: u64 = 6;

/// 一个 tick 的可见性三源（并集 / 本地 / halo）。
#[derive(Clone, Debug, Eq, PartialEq)]
struct ZoneVisible {
    union: Vec<i32>,
    local: Vec<i32>,
    halo: Vec<i32>,
}

const RECT_A: ZoneRect = ZoneRect::new(0, 0, 9, 9);
const RECT_B: ZoneRect = ZoneRect::new(10, 0, 19, 9);

fn spawn(id: i32, name: &str, x: i32, y: i32, view_range: i32) -> EntitySpawn {
    let mut e = EntitySpawn::new(id, name.to_string(), x, y);
    e.race = 80;
    e.view_range = view_range;
    e
}

fn added(mut world: World, entities: &[EntitySpawn]) -> World {
    for e in entities {
        world.add_entity(e.clone());
    }
    // 一次 tick 处理 `Command::Enter`（落图，`add_to_mapped=true`），与基线对齐时钟。
    world.tick();
    world
}

fn world_a() -> World {
    added(
        World::new(MAP_W, MAP_H),
        &[
            spawn(OBSERVER_ID, "O", 9, 5, OBSERVER_VIEW),
            spawn(E_A_ID, "E_A", 8, 5, 12),
        ],
    )
}

fn world_b() -> World {
    added(
        World::new(MAP_W, MAP_H),
        &[
            spawn(E_B_ID, "E_B", 10, 5, 12),
            spawn(M_B_ID, "M_B", 10, 7, 12),
        ],
    )
}

fn world_baseline() -> World {
    added(
        World::new(MAP_W, MAP_H),
        &[
            spawn(OBSERVER_ID, "O", 9, 5, OBSERVER_VIEW),
            spawn(E_A_ID, "E_A", 8, 5, 12),
            spawn(E_B_ID, "E_B", 10, 5, 12),
            spawn(M_B_ID, "M_B", 10, 7, 12),
        ],
    )
}

fn cheb(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    (ax - bx).abs().max((ay - by).abs())
}

fn observer_xy(world: &World) -> (i32, i32) {
    let o = world.entities.get(OBSERVER_ID).expect("观察者必须在世界内");
    (o.x, o.y)
}

fn visible_local(world: &World) -> Vec<i32> {
    let (ox, oy) = observer_xy(world);
    let mut ids: Vec<i32> = world
        .entities
        .iter_ref()
        .filter(|e| e.id != OBSERVER_ID && cheb(ox, oy, e.x, e.y) <= OBSERVER_VIEW)
        .map(|e| e.id)
        .collect();
    ids.sort_unstable();
    ids
}

fn visible_halo(observer: (i32, i32), halo: &HaloSnapshot) -> Vec<i32> {
    let (ox, oy) = observer;
    let mut ids: Vec<i32> = halo
        .entities
        .iter()
        .filter(|e| e.actor_id != OBSERVER_ID && cheb(ox, oy, e.x, e.y) <= OBSERVER_VIEW)
        .map(|e| e.actor_id)
        .collect();
    ids.sort_unstable();
    ids
}

/// 基线：单 World 全区，`tick()` 驱动；每 tick 返回 O 的可见集合。
fn run_baseline() -> Vec<Vec<i32>> {
    let mut world = world_baseline();
    let mut out = Vec::new();
    for tick in 1..=TICKS {
        if tick == MOVE_TICK {
            assert!(world.entities.set_xy(M_B_ID, 14, 7), "移动 M_B");
        }
        let _ = world.tick();
        out.push(visible_local(&world));
    }
    out
}

/// 双区：`stale=false` 正确路径；`stale=true` 在 phase1 之前取快照（上一 tick 状态）。
/// 返回每 tick 的三源可见性——用于可见集合与 dup/never 断言。
fn run_split(stale: bool) -> Vec<ZoneVisible> {
    let barriers = Arc::new(TickBarriers::new(2));
    let exchange = Arc::new(HaloExchange::new(2));
    let (tx, rx) = mpsc::channel::<ZoneVisible>();

    let b_barriers = Arc::clone(&barriers);
    let b_exchange = Arc::clone(&exchange);
    let handle_b = std::thread::spawn(move || {
        // **失败快传**：本线程 panic ⇒ 唤醒 A 区，避免测试挂死。
        let abort = Arc::clone(&b_barriers);
        let run = move || {
            let mut world = world_b();
            for tick in 1..=TICKS {
                if stale {
                    b_exchange.publish(
                        ZoneId::new(1),
                        snapshot_border(&world, 0, RECT_B, HALO, tick, &HaloFrameSource::none()),
                    );
                }
                if tick == MOVE_TICK {
                    assert!(world.entities.set_xy(M_B_ID, 14, 7), "移动 M_B");
                }
                let mut record = world.tick_phase1();
                if !stale {
                    b_exchange.publish(
                        ZoneId::new(1),
                        snapshot_border(&world, 0, RECT_B, HALO, tick, &HaloFrameSource::none()),
                    );
                }
                b_barriers.wait_after_phase1();
                let _ = b_exchange.read(ZoneId::new(0), tick);
                world.tick_phase2(&mut record);
                b_barriers.wait_after_phase2();
            }
        };
        if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
            abort.abort();
            std::panic::resume_unwind(payload);
        }
    });

    let a_barriers = Arc::clone(&barriers);
    let a_exchange = Arc::clone(&exchange);
    let handle_a = std::thread::spawn(move || {
        let abort = Arc::clone(&a_barriers);
        let run = move || {
            let mut world = world_a();
            for tick in 1..=TICKS {
                if stale {
                    a_exchange.publish(
                        ZoneId::new(0),
                        snapshot_border(&world, 0, RECT_A, HALO, tick, &HaloFrameSource::none()),
                    );
                }
                let mut record = world.tick_phase1();
                if !stale {
                    a_exchange.publish(
                        ZoneId::new(0),
                        snapshot_border(&world, 0, RECT_A, HALO, tick, &HaloFrameSource::none()),
                    );
                }
                a_barriers.wait_after_phase1();
                let halo_b = a_exchange
                    .read(ZoneId::new(1), tick)
                    .expect("B 区必须在本 tick 发布快照");
                world.tick_phase2(&mut record);
                a_barriers.wait_after_phase2();

                let local = visible_local(&world);
                let halo = visible_halo(observer_xy(&world), &halo_b);
                let mut union: Vec<i32> = local.iter().chain(halo.iter()).copied().collect();
                union.sort_unstable();
                tx.send(ZoneVisible { union, local, halo })
                    .expect("主线程接收");
            }
        };
        if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
            abort.abort();
            std::panic::resume_unwind(payload);
        }
    });

    let out: Vec<ZoneVisible> = (0..TICKS)
        .map(|_| rx.recv().expect("每 tick 必须收到 A 的可见集合"))
        .collect();
    handle_a.join().expect("A 线程");
    handle_b.join().expect("B 线程");
    out
}

#[test]
fn two_phase_halo_matches_single_zone_baseline() {
    let baseline = run_baseline();
    let split = run_split(false);
    assert_eq!(
        split.iter().map(|v| v.union.clone()).collect::<Vec<_>>(),
        baseline,
        "两阶段 + 本 tick 快照的可见集合必须与单区基线逐 tick 相同"
    );
    for (tick, v) in split.iter().enumerate() {
        assert!(
            v.local.iter().all(|id| !v.halo.contains(id)),
            "tick {} 的 dup 非 0：本地集与 halo 集相交 {:?} / {:?}",
            tick + 1,
            v.local,
            v.halo
        );
        assert_eq!(
            v.union.len(),
            v.local.len() + v.halo.len(),
            "tick {} 的并集必须等于两源之和（never/dup 双证）",
            tick + 1
        );
    }
}

#[test]
fn stale_mirror_must_differ_from_baseline_after_the_move() {
    let baseline = run_baseline();
    let stale = run_split(true);
    let diff = (0..TICKS as usize).find(|&i| stale[i].union != baseline[i]);
    assert_eq!(
        diff,
        Some((MOVE_TICK - 1) as usize),
        "陈旧快照必须在移动后的那一拍与基线产生差异；若此处没有差异 ⇒ 牙失效（陈旧路径与正确路径同序）"
    );
    assert!(
        stale[(MOVE_TICK - 1) as usize].union.contains(&M_B_ID),
        "陈旧路径在移动后仍应把 M_B 当作可见（这正是「慢一拍」的表现）"
    );
}
