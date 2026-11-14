//! **测量 harness（不是判据）**：`HostAoiDownlink` 子探针的三档缩放 + 7.9k 单点分解。
//!
//! 卡 `openmir2-host-aoi-downlink-profile`。只在 `cfg(test)` 下编译，且默认 `#[ignore]`
//! （`cargo test` 不跑，避免拖慢套件）。跑法（release；`MIR2_HOST_AOI_BENCH=1` 是二道门，
//! 防止误跑）：
//!
//! ```text
//! CARGO_TARGET_DIR=E:/UserTemp/w84-rel cargo test --release -p game-svr --lib \
//!     -- --ignored --nocapture host_aoi_bench
//! ```
//!
//! # 场景（与 `docs/capacity-model-20261114.md` 同源，两处已登记偏差）
//!
//! - 200x200 单图、`race=80` 怪物、AI/buff/npc/merchant 全开、固定种子；实体数 11,000 或 7,908。
//! - **偏差 1（横向隔格）**：容量模型用 199 列逐行铺满；本 harness 用 `x = 1 + 2*(i % 99)`
//!   （横向隔一格）。原因：`try_step_impl` 的目标格被活着的 actor 占据就不移动，铺满时
//!   **任何**单格走位都被挡，`drain_monster_walk_downlink` 恒零，测不到真栈 MonGen 怪的持续
//!   游走。隔格后单格位移可落地；代价是玩家 11x11 视野内的可见实体约少一半。
//! - **偏差 2（走位注入）**：像 `tick_bench::run_tier` 那样每 tick 让 `i % 4 == tick % 4` 的
//!   怪物 `world.walk(±1, 0)`（方向每 4 tick 翻转）。真栈里这是怪物 AI/MonGen 的自然游走；
//!   合成世界没有目标可追，只能显式注入。
//! - 玩家（bot）= **真会话**：`open_session` + `enter_map`（`race=0`、`view_range=5`），
//!   并登记 `online` 与 gate 通道，1000ms 首扫门、出现帧与 `drain_stamina` 都真的跑。
//!   bot 位置按 `stride` 均布在怪物网格上（允许与怪物同格，与 `tick_bench --bots` 同形）。
//!
//! # 三臂（同种子、同世界构造、同 tick 数）
//!
//! - `none`：不传探针句柄（`None`），即「没有探针接线」的基线；
//! - `off`：传**关闭态** profile（`with_enabled(false)`），生产缺省形态；
//! - `on`：传开启态 profile，收集子探针。
//!
//! `none` 与 `off` 的每 tick 中位数差 = 关闭态接线的开销；`off` 与 `on` = 计时开销。

use super::*;
use std::collections::HashMap;
use std::time::Instant;

const MAP: i32 = 200;
const MONSTER_RACE: u8 = 80;
const VIEW_RANGE: i32 = 5;
const WARMUP_TICKS: usize = 12;
const SEND_RANGE: i32 = 12;
const HEALTH_FILL: i32 = 3000;
const SPELL_FILL: i32 = 3000;
/// 走位注入的cohort：每 tick 让 `i % WALK_DIV == tick % WALK_DIV` 的怪物走一格。
///
/// **不能取 4**（`tick_bench::run_tier` 的节拍）：那是压力场景，11k 实体会变成每 tick
/// ~2,750 个走位者，而 `broadcast_ref_msg` 的收件人扫描是 O(全实体) ⇒ 单 tick 数秒，
/// 远超真栈量级。取 40（每 tick 2.5% 实体）≈ 275 走位者/11k，接近真栈 MonGen 怪的自然游走率；
/// 真栈的实际走位者数由 `HostAoiWalkFanout` 的 calls/tick 直接观测，两份数据可相乘。
const WALK_DIV: usize = 40;

/// 一档测量的所有可变状态。
struct Rig {
    world: mir2_world_bevy::World,
    displays: downlink::DisplayTable,
    online: OnlineRegistry,
    gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    rx: mpsc::UnboundedReceiver<SimToGate>,
    sessions: HashMap<String, SessionGround>,
    seen: HashMap<i32, (usize, i32, i32)>,
    monster_ids: Vec<i32>,
}

fn fake_display(name: &str) -> downlink::ActorDisplay {
    downlink::ActorDisplay::new(0, 0, 0, 0, 0, 0, name.as_bytes().to_vec())
}

fn grid_xy(i: usize) -> (i32, i32) {
    (1 + 2 * (i as i32 % 99), 1 + (i as i32 / 99))
}

fn build_rig(entities: usize, bots: usize) -> Rig {
    let mut world = mir2_world_bevy::World::new(MAP, MAP);
    let mut displays = downlink::DisplayTable::new();
    let mut monster_ids = Vec::with_capacity(entities);
    for i in 0..entities {
        let (x, y) = grid_xy(i);
        let mut e = mir2_world_bevy::EntitySpawn::new((i + 1) as i32, format!("b{i}"), x, y);
        e.race = MONSTER_RACE;
        e.view_range = VIEW_RANGE;
        let id = world.add_entity(e);
        monster_ids.push(id);
        displays.set(id, fake_display("mon"));
    }
    let (tx, rx) = mpsc::unbounded_channel::<SimToGate>();
    let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
    gates.insert(0u8, tx);
    let mut online: OnlineRegistry = Vec::new();
    let mut sessions: HashMap<String, SessionGround> = HashMap::new();
    if bots > 0 {
        let stride = entities.checked_div(bots).unwrap_or(0).max(1);
        for k in 0..bots {
            let (x, y) = grid_xy(k * stride);
            let actor_id = 1_000_000 + k as i32;
            let name = format!("bot{k}");
            world.open_session(&name);
            let mut p = mir2_world_bevy::EntitySpawn::new(actor_id, name.clone(), x, y);
            p.race = 0;
            p.view_range = VIEW_RANGE;
            let id = world.enter_map(&name, p, 0).expect("bot 进图");
            displays.set(id, fake_display(&name));
            online.push(OnlineEntry {
                account: name.clone(),
                chr_name: name.clone(),
                session_id: k as i32 + 1,
                gate_idx: 0,
                socket: 9_000 + k as i32,
                actor_id: id,
            });
            let mut sg = crate::tests::test_session_ground(id);
            sg.stamina = game_svr::stamina::Stamina::new(now_ms());
            sg.recovery = game_svr::recover::Recovery::new(now_ms());
            sessions.insert(name, sg);
        }
    }
    world.enable_monster_ai();
    world.enable_buff_tick();
    world.enable_npc_tick();
    world.enable_merchant_tick();
    world.tick();
    Rig {
        world,
        displays,
        online,
        gates,
        rx,
        sessions,
        seen: HashMap::new(),
        monster_ids,
    }
}

/// 一次宿主 tick：注入走位，世界 tick（不计时），**只计时** `drain_run_downlinks_with_magic`。
/// 返回 `(宿主段 ns, 本 tick rec.hides.len(), gate 通道取走的帧数)`。
fn tick_once(
    rig: &mut Rig,
    tick: usize,
    prof: &mut Option<&mut tick_profile::HostTickProfile>,
    catalog: &dyn game_svr::ability::ItemCatalog,
) -> (u64, usize, usize) {
    let dx = if (tick / 4).is_multiple_of(2) { 1 } else { -1 };
    let dir = if dx > 0 {
        mir2_world_bevy::magic::DIR_RIGHT
    } else {
        mir2_world_bevy::magic::DIR_LEFT
    };
    let ids = rig.monster_ids.clone();
    for (i, id) in ids.iter().enumerate() {
        if i % WALK_DIV == tick % WALK_DIV {
            // 先置 Dir（oracle `Walk` 的 `Dir = btDir` 在移动之前）；否则
            // `drain_monster_walk_downlink` 的方向门 `(dx,dy) == dir_delta(ai.dir)` 基本恒假，
            // 注入的走位会被当成「不是走位」，扇出成本测不到。
            if let Some(e) = rig.world.entities.get_mut(*id) {
                e.ai.dir = dir;
            }
            rig.world.walk(*id, dx, 0);
        }
    }
    let mut rec = rig.world.tick_head();
    rig.world.tick_tail(&mut rec);
    let hides = rec.hides.len();
    let t0 = Instant::now();
    let _ = drain_run_downlinks_with_magic(
        &rec,
        &mut rig.world,
        &rig.displays,
        &rig.online,
        &mut rig.sessions,
        &rig.gates,
        catalog,
        SEND_RANGE,
        HEALTH_FILL,
        SPELL_FILL,
        false,
        None,
        prof.as_deref_mut(),
        None,
        Some(&mut rig.seen),
    );
    let ns = t0.elapsed().as_nanos() as u64;
    let mut frames = 0usize;
    while rig.rx.try_recv().is_ok() {
        frames += 1;
    }
    (ns, hides, frames)
}

#[derive(Clone, Copy)]
enum Arm {
    None,
    Off,
    On,
}

struct ArmResult {
    mean_ns: u64,
    median_ns: u64,
    p90_ns: u64,
    max_ns: u64,
    probes: Vec<(&'static str, u64, u64)>,
    hides_per_tick: f64,
    frames_per_tick: f64,
}

fn run_arm(entities: usize, bots: usize, ticks: usize, arm: Arm) -> ArmResult {
    let mut rig = build_rig(entities, bots);
    let catalog = game_svr::ability::StdItemCatalog::new();
    for i in 0..WARMUP_TICKS {
        let mut none: Option<&mut tick_profile::HostTickProfile> = None;
        let _ = tick_once(&mut rig, i, &mut none, &catalog);
    }
    let mut profile = tick_profile::HostTickProfile::with_enabled(matches!(arm, Arm::On));
    let mut samples: Vec<u64> = Vec::with_capacity(ticks);
    let mut hides = 0usize;
    let mut frames = 0usize;
    for i in 0..ticks {
        let tick = WARMUP_TICKS + i;
        let (ns, h, f) = match arm {
            Arm::None => {
                let mut none: Option<&mut tick_profile::HostTickProfile> = None;
                tick_once(&mut rig, tick, &mut none, &catalog)
            }
            Arm::Off | Arm::On => {
                let mut some: Option<&mut tick_profile::HostTickProfile> = Some(&mut profile);
                tick_once(&mut rig, tick, &mut some, &catalog)
            }
        };
        samples.push(ns);
        hides += h;
        frames += f;
    }
    let mean_ns = samples.iter().copied().sum::<u64>() / samples.len() as u64;
    samples.sort_unstable();
    let rank = |p: f64| {
        let n = samples.len();
        let r = ((p / 100.0) * n as f64).ceil() as usize;
        samples[r.clamp(1, n) - 1]
    };
    ArmResult {
        mean_ns,
        median_ns: samples[samples.len() / 2],
        p90_ns: rank(90.0),
        max_ns: samples.last().copied().unwrap_or(0),
        probes: profile.probes().to_vec(),
        hides_per_tick: hides as f64 / ticks as f64,
        frames_per_tick: frames as f64 / ticks as f64,
    }
}

#[test]
#[ignore = "测量 harness：需 MIR2_HOST_AOI_BENCH=1 + --ignored --nocapture；见 docs/host-aoi-profile-20261114.md"]
fn host_aoi_bench_tiers() {
    if std::env::var("MIR2_HOST_AOI_BENCH").ok().as_deref() != Some("1") {
        eprintln!("MIR2_HOST_AOI_BENCH != 1 不跑测量（这是 harness，不是判据）");
        return;
    }
    // (实体数, bot 数, 测量 tick 数)。1000 bot 档每 5 tick 一次首扫、hide 路径 O(H*H)，
    // 20 tick（4 个扫描周期）已足够，且不让测量窗口拖到分钟级。
    let tiers: &[(usize, usize, usize)] = &[
        (7_908, 0, 40),
        (11_000, 0, 40),
        (11_000, 1, 40),
        (11_000, 100, 30),
        (11_000, 1_000, 20),
    ];
    for &(entities, bots, ticks) in tiers {
        let mut arms = Vec::new();
        for arm in [Arm::None, Arm::Off, Arm::On] {
            arms.push(run_arm(entities, bots, ticks, arm));
        }
        println!("TIER entities={entities} bots={bots} ticks={ticks}");
        for (name, r) in ["none", "off", "on"].iter().zip(arms.iter()) {
            println!(
                "  arm {name}: mean={:.3} p50={:.3} p90={:.3} max={:.3} ms/tick  hides/tick={:.1} frames/tick={:.1}",
                r.mean_ns as f64 / 1e6,
                r.median_ns as f64 / 1e6,
                r.p90_ns as f64 / 1e6,
                r.max_ns as f64 / 1e6,
                r.hides_per_tick,
                r.frames_per_tick
            );
        }
        let none_ms = arms[0].mean_ns as f64 / 1e6;
        let off_ms = arms[1].mean_ns as f64 / 1e6;
        let on_ms = arms[2].mean_ns as f64 / 1e6;
        println!(
            "  off_vs_none={:+.2}% on_vs_off={:+.2}% (mean)",
            (off_ms - none_ms) / none_ms * 100.0,
            (on_ms - off_ms) / off_ms.max(1e-9) * 100.0
        );
        let on = &arms[2];
        let mut ps = on.probes.clone();
        ps.sort_by_key(|p| std::cmp::Reverse(p.1));
        println!("  probes (mean ms/tick, % of on-arm mean host-drain tick, calls/tick):");
        for (n, ns, calls) in ps {
            println!(
                "    {n:<26} {:>9.3} {:>7.2}%  {:.1}",
                ns as f64 / 1e6 / ticks as f64,
                ns as f64 / 1e6 / ticks as f64 / on_ms.max(1e-9) * 100.0,
                calls as f64 / ticks as f64
            );
        }
        // 扇出的**单价**（与走位者数/实体数解耦，便于和真栈观测相乘）：
        // `HostAoiWalkFanout` 每次调用 = 一个走位者的一次 `broadcast_ref_msg`
        // （`chat::hear_targets` 对**全实体**做 race==PLAY ±range 过滤）。
        let fanout = on.probes.iter().find(|p| p.0 == "HostAoiWalkFanout");
        let walk_total = on.probes.iter().find(|p| p.0 == "HostAoiMonsterWalk");
        if let Some((_, ns, calls)) = fanout {
            if *calls > 0 {
                println!(
                    "  walk_fanout_unit: {:.3} us/walker (实体数 {entities}, 走位者 {:.1}/tick)",
                    *ns as f64 / *calls as f64 / 1000.0,
                    *calls as f64 / ticks as f64
                );
            }
        }
        if let (Some((_, wt, _)), Some((_, ft, _))) = (walk_total, fanout) {
            println!(
                "  walk_scan_frame: {:.3} ms/tick (= HostAoiMonsterWalk - HostAoiWalkFanout)",
                wt.saturating_sub(*ft) as f64 / 1e6 / ticks as f64
            );
        }
        println!();
    }
}
