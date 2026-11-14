//! **测量 harness（不是判据）**：`SimMonsterAi` 的结构分解 / 可跳过比例 / 超线性 + 逐 tick 分位数。
//!
//! 卡 `openmir2-simmonsterai-profile`（tier L：只加探针与报告，不改 AI 逻辑）。只在 `cfg(test)`
//! 下编译，且默认 `#[ignore]`；`MIR2_SIMMON_BENCH=1` 是二道门。跑法（release）：
//!
//! ```text
//! MIR2_SIMMON_BENCH=1 CARGO_TARGET_DIR=E:/UserTemp/w84-rel \
//!   cargo test --release -p game-svr --lib --offline -- --ignored --nocapture simmonsterai_bench \
//!   > E:/UserTemp/w84-measure/simmon-ai.txt 2>&1
//! ```
//!
//! 可调：`MIR2_SIMMON_ENTITIES=5000,8000,11000`（三档缩放）、`MIR2_SIMMON_TICKS=100`、
//! `MIR2_SIMMON_BOTS=8`（A6 同构档的 bot 数）。
//!
//! # 场景（与 `tick_bench --profile` / `capacity-model-20261114.md` 同源）
//!
//! 200×200 单图、199 列逐行铺满、`race=80` 怪物、MonGen/AI/buff/npc/merchant 全开、固定种子 42。
//! bot（`race=0`）按 stride 均布，用 `add_entity` 落图（**无会话**；AOI 仍会把它们视野内的怪标成
//! `IsVisibleActive`，与 `tick_bench --profile --bots` 同形）。
//!
//! # 三臂（同构造、同种子、同 tick 数）
//!
//! - `none`：不调用宿主探针（无 `HostTickProfile` 句柄），世界探针也不启用；
//! - `off`：调用宿主探针但**关闭态**（`HostTickProfile::default()`，生产缺省形态）；
//! - `on`：宿主探针开启 + `world.enable_tick_profile()`（收集 `SimMonsterAi` 逐 tick 样本与
//!   `MonaiProbe` 子项）。
//!
//! `off vs none` = 关闭态接线的可测开销；`on vs off` = 计时/子探针的扰动。
//!
//! # 口径（与报告 §1 一致）
//!
//! - `HostTickTotal 工作段`（本 harness）= 一轮循环体的墙钟（`world.tick()` 的 head+tail；
//!   本 harness 无 socket/无节拍 sleep）⇒ 等价于生产 `HostTickTotal − HostSleep` 的"工作段"半边。
//! - `SimMonsterAi` 逐 tick 样本来自 `TickProfile::sim_ai_tick_ns`（每个 tick 一个值，
//!   **不是** 50-tick 桶近似）。
//! - 结构分解：`collect`（`order_ids` + 身份门 retain）/ `gate`（只读门判定预扫描）/ `loop`
//!   （门写回 + 全部 AI 体）⇒ `AI 体 ≈ loop − gate`。

use super::*;
use mir2_world_bevy::monai::MonaiProbe;
use std::time::Instant;

const MAP: i32 = 200;
const MONSTER_RACE: u8 = 80;
const WARMUP_TICKS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    None,
    Off,
    On,
}

struct ArmResult {
    /// 每 tick 的循环体墙钟（ns）。
    wall_ns: Vec<u64>,
    /// 逐 tick `HostTickTotal 工作段`（`HostTickProfile::end_tick` 样本；`none` 臂为空）。
    work_ns: Vec<u64>,
    /// 逐 tick `SimMonsterAi`（`on` 臂；其余为空）。
    sim_ai_ns: Vec<u64>,
    stages: Vec<(String, u64, u64)>,
    monai: MonaiProbe,
    monai_ticks: u64,
}

fn env_usize_list(key: &str, default: &[usize]) -> Vec<usize> {
    match std::env::var(key) {
        Ok(v) => v
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect(),
        Err(_) => default.to_vec(),
    }
}

fn grid_xy(i: usize) -> (i32, i32) {
    // 199 列逐行铺满（11k ⇒ 56 行，全在图内；不能沿用 tick_bench::run_tier 的 100 列）。
    (1 + (i as i32 % 199), 1 + (i as i32 / 199))
}

fn build_world(
    entities: usize,
    bots: usize,
    race: u8,
) -> (mir2_world_bevy::World, Vec<i32>, Vec<i32>) {
    let mut world = mir2_world_bevy::World::new(MAP, MAP);
    let mut monster_ids = Vec::with_capacity(entities);
    for i in 0..entities {
        let (x, y) = grid_xy(i);
        let mut e = mir2_world_bevy::EntitySpawn::new((i + 1) as i32, format!("b{i}"), x, y);
        e.race = race;
        monster_ids.push(world.add_entity(e));
    }
    let mut bot_ids = Vec::with_capacity(bots);
    if bots > 0 {
        let stride = entities.checked_div(bots).unwrap_or(1).max(1);
        for k in 0..bots {
            let (x, y) = grid_xy(k * stride);
            let mut p =
                mir2_world_bevy::EntitySpawn::new(1_000_000 + k as i32, format!("bot{k}"), x, y);
            p.race = 0;
            // 高 HP：避免 bot 在测量窗口内被打死后引入死亡怪（改变 AI 路径）。
            p.hp = 60_000;
            p.max_hp = 60_000;
            bot_ids.push(world.add_entity(p));
        }
    }
    // **相位去同步**：真栈的怪由 MonGen 分批生成，`ProcessRunCount` 相位彼此错开；合成世界的
    // 怪全是同一刻创建 ⇒ 计数同步递增 ⇒ **每 3 tick 全体同时 Run**（逐 tick 分位数会出现
    // 1×/18× 的假锯齿）。按 `i % (interval+1)` 预置初值，模拟分批生成的混合相位。
    let period = (world.process_monster_interval.max(0) + 1) as usize;
    if period > 1 {
        for (i, id) in monster_ids.iter().enumerate() {
            world.set_process_run_count(*id, (i % period) as i32);
        }
    }
    world.enable_monster_ai();
    world.enable_buff_tick();
    world.enable_npc_tick();
    world.enable_merchant_tick();
    world.tick(); // 预热：Enter 命令一次性落图，不计入样本
    (world, monster_ids, bot_ids)
}

fn run_arm(
    entities: usize,
    bots: usize,
    ticks: usize,
    arm: Arm,
    dead: usize,
    race: u8,
    inject_targets: bool,
) -> ArmResult {
    let (mut world, monster_ids, bot_ids) = build_world(entities, bots, race);
    // **目标注入**（场景假设，与 AOI harness 的走位注入同性质）：合成世界的 bot 没有会话，
    // `SearchViewRange` 不把怪标成 `visible_active`，怪也就拿不到 `TargetCret`（见报告 §2 偏差）。
    // 这里给「离某个 bot ≤5 格」的怪（= 玩家视野内的那一小撮）直接置
    // `IsVisibleActive + TargetCret + TargetFocusTick`（`TargetFocusTick` 不刷新会在 30s 后
    // 被 `base.Run` 清掉；±15 距离门也在 `clear_target_if_expired` 里）。
    if inject_targets && !bot_ids.is_empty() {
        let now = world.clock_ms;
        let bot_pos: Vec<(i32, i32, i32)> = bot_ids
            .iter()
            .filter_map(|id| world.entities.get(*id).map(|b| (*id, b.x, b.y)))
            .collect();
        for id in &monster_ids {
            let Some((mx, my)) = world.entities.get(*id).map(|e| (e.x, e.y)) else {
                continue;
            };
            let nearest = bot_pos
                .iter()
                .filter(|(_, bx, by)| (bx - mx).abs() + (by - my).abs() <= 5)
                .min_by_key(|(_, bx, by)| (bx - mx).abs() + (by - my).abs())
                .copied();
            let Some((bid, bx, by)) = nearest else {
                continue;
            };
            if let Some(e) = world.entities.get_mut(*id) {
                e.ai.target_creature = Some(bid);
                e.ai.target_x = bx;
                e.ai.target_y = by;
                e.ai.target_focus_tick = now;
            }
            if let Some(b) = world.entities.map_binding_mut(*id) {
                b.is_visible_active = true;
            }
        }
    }
    // 死亡怪臂：把前 `dead` 只标的怪置 `Death` 且 `DeathTick = now`（不会被 MakeGhost 立即回收）；
    // 它们每 tick 走 `base_object_run_monster` 的死亡支 ⇒ `play_object_count` 的全实体扫描（O(dead×N)）。
    if dead > 0 {
        let now = world.clock_ms;
        for id in monster_ids.iter().take(dead) {
            if let Some(e) = world.entities.get_mut(*id) {
                e.life_cycle.death = true;
                e.life_cycle.death_tick = now;
            }
        }
    }
    for _ in 0..WARMUP_TICKS {
        world.tick();
    }
    if arm == Arm::On {
        world.enable_tick_profile();
        world.reset_tick_profile();
    }
    let mut hp = if arm == Arm::Off || arm == Arm::On {
        match arm {
            Arm::On => tick_profile::HostTickProfile::with_enabled(true),
            _ => tick_profile::HostTickProfile::default(),
        }
    } else {
        tick_profile::HostTickProfile::default()
    };
    let mut wall_ns = Vec::with_capacity(ticks);
    for _ in 0..ticks {
        let t0 = Instant::now();
        match arm {
            Arm::None => {
                world.tick();
            }
            Arm::Off | Arm::On => {
                let slot = hp.begin();
                let t = hp.begin();
                world.tick();
                hp.end("HostWorldTick", t);
                hp.end_tick(slot);
            }
        }
        wall_ns.push(t0.elapsed().as_nanos() as u64);
    }
    let mut stages = Vec::new();
    let mut sim_ai_ns = Vec::new();
    let mut monai = MonaiProbe::default();
    let mut monai_ticks = 0;
    if arm == Arm::On {
        if let Some(p) = world.tick_profile() {
            stages = p
                .stages()
                .iter()
                .map(|(s, n, c)| (format!("{s:?}"), *n, *c))
                .collect();
            sim_ai_ns = p.sim_ai_tick_ns.clone();
            monai = p.monai.clone();
            monai_ticks = p.monai_ticks;
        }
    }
    let work_ns = hp.work_samples().to_vec();
    ArmResult {
        wall_ns,
        work_ns,
        sim_ai_ns,
        stages,
        monai,
        monai_ticks,
    }
}

fn mean_ms(v: &[u64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().map(|x| *x as f64).sum::<f64>() / v.len() as f64 / 1e6
}

fn q_ms(samples: &[u64], p: f64) -> f64 {
    tick_profile::percentile_ns(samples, p) as f64 / 1e6
}

fn max_ms(samples: &[u64]) -> f64 {
    samples.iter().copied().max().unwrap_or(0) as f64 / 1e6
}

fn print_arm(name: &str, r: &ArmResult, ticks: usize) {
    println!(
        "  arm {name:<4}: wall mean={:.3} ms/tick  n={ticks}",
        mean_ms(&r.wall_ns)
    );
    if !r.work_ns.is_empty() {
        println!(
            "    HostTickTotal 工作段 逐 tick（n={}）：p50 {:.3} / p95 {:.3} / p99 {:.3} / max {:.3} / mean {:.3} ms",
            r.work_ns.len(),
            q_ms(&r.work_ns, 50.0),
            q_ms(&r.work_ns, 95.0),
            q_ms(&r.work_ns, 99.0),
            max_ms(&r.work_ns),
            mean_ms(&r.work_ns)
        );
    }
    if !r.sim_ai_ns.is_empty() {
        println!(
            "    SimMonsterAi 逐 tick（n={}）：p50 {:.3} / p95 {:.3} / p99 {:.3} / max {:.3} / mean {:.3} ms",
            r.sim_ai_ns.len(),
            q_ms(&r.sim_ai_ns, 50.0),
            q_ms(&r.sim_ai_ns, 95.0),
            q_ms(&r.sim_ai_ns, 99.0),
            max_ms(&r.sim_ai_ns),
            mean_ms(&r.sim_ai_ns)
        );
    }
}

fn print_monai_breakdown(on: &ArmResult) {
    if on.monai_ticks == 0 {
        return;
    }
    let mt = on.monai_ticks as f64;
    let m = &on.monai;
    let per = |v: u64| v as f64 / mt;
    println!(
        "    MonaiProbe（{} tick 累计）：collect {:.3} / gate {:.3} / loop {:.3} ms/tick ⇒ AI 体(loop−gate) {:.3} ms/tick",
        on.monai_ticks,
        m.collect_ns as f64 / 1e6 / mt,
        m.gate_ns as f64 / 1e6 / mt,
        m.loop_ns as f64 / 1e6 / mt,
        (m.loop_ns as f64 - m.gate_ns as f64).max(0.0) / 1e6 / mt
    );
    let scanned = per(m.scanned).max(1.0);
    println!(
        "    counts/tick：slot {:.0}，scanned {:.0}（inactive {:.0}），throttle-skip {:.0}（{:.1}%），ran {:.0}（{:.1}%）",
        per(m.slot_total),
        per(m.scanned),
        per(m.scanned_inactive),
        per(m.skipped_throttle),
        per(m.skipped_throttle) / scanned * 100.0,
        per(m.ran),
        per(m.ran) / scanned * 100.0
    );
    println!(
        "    其中 ran：dead {:.0}（{:.1}%），frozen {:.0}（{:.1}%），no-target {:.0}（{:.1}%），idle-wander {:.0}（{:.1}%），walk-due {:.0}（{:.1}%），target {:.0}（{:.1}%）",
        per(m.ran_dead),
        per(m.ran_dead) / scanned * 100.0,
        per(m.ran_frozen),
        per(m.ran_frozen) / scanned * 100.0,
        per(m.ran_no_target),
        per(m.ran_no_target) / scanned * 100.0,
        per(m.ran_idle_wander),
        per(m.ran_idle_wander) / scanned * 100.0,
        per(m.ran_walk_due),
        per(m.ran_walk_due) / scanned * 100.0,
        per(m.ran_target),
        per(m.ran_target) / scanned * 100.0
    );
}

#[test]
#[ignore = "测量 harness：需 MIR2_SIMMON_BENCH=1 + --ignored --nocapture；见 docs/simmonsterai-profile-20261114.md"]
fn simmonsterai_bench_tiers() {
    if std::env::var("MIR2_SIMMON_BENCH").ok().as_deref() != Some("1") {
        eprintln!("MIR2_SIMMON_BENCH != 1 不跑测量（这是 harness，不是判据）");
        return;
    }
    let tiers = env_usize_list("MIR2_SIMMON_ENTITIES", &[5_000, 8_000, 11_000]);
    let ticks = env_usize_list("MIR2_SIMMON_TICKS", &[100])
        .first()
        .copied()
        .unwrap_or(100);
    let bots = env_usize_list("MIR2_SIMMON_BOTS", &[8])
        .first()
        .copied()
        .unwrap_or(8);
    println!(
        "# SimMonsterAi harness：entities={:?} ticks={} bots(A6 档)={}；每档 warmup={} tick",
        tiers, ticks, bots, WARMUP_TICKS
    );
    println!("# 三臂：none=无宿主探针/无世界探针；off=宿主探针关闭态；on=宿主探针+World 探针开启");
    println!();

    // ---- 三档缩放（bots=0，纯怪基线）+ 8k/8bot（A6 同构）----
    let mut rows: Vec<(usize, usize, ArmResult, ArmResult, ArmResult)> = Vec::new();
    for &entities in &tiers {
        for &b in &[0usize, bots] {
            let none = run_arm(entities, b, ticks, Arm::None, 0, MONSTER_RACE, false);
            let off = run_arm(entities, b, ticks, Arm::Off, 0, MONSTER_RACE, false);
            let on = run_arm(entities, b, ticks, Arm::On, 0, MONSTER_RACE, false);
            println!("TIER entities={entities} bots={b} ticks={ticks}");
            print_arm("none", &none, ticks);
            print_arm("off", &off, ticks);
            print_arm("on", &on, ticks);
            let n = mean_ms(&none.wall_ns);
            let o = mean_ms(&off.wall_ns);
            let x = mean_ms(&on.wall_ns);
            println!(
                "    off_vs_none={:+.2}%  on_vs_off={:+.2}%（循环体墙钟均值）",
                (o - n) / n.max(1e-9) * 100.0,
                (x - o) / o.max(1e-9) * 100.0
            );
            if !on.sim_ai_ns.is_empty() {
                let ai_mean = mean_ms(&on.sim_ai_ns);
                println!(
                    "    SimMonsterAi 每实体 = {:.4} µs（{:.3} ms / {} 实体）",
                    ai_mean * 1000.0 / entities as f64,
                    ai_mean,
                    entities
                );
            }
            // 结构分解 + 计数
            print_monai_breakdown(&on);
            // 段表（按 ns 降序）
            let mut st = on.stages.clone();
            st.sort_by_key(|a| std::cmp::Reverse(a.1));
            if !st.is_empty() {
                println!("    World 段（ms/tick，占比按段合计）：");
                let sum: u64 = st.iter().map(|(_, ns, _)| *ns).sum();
                for (name, ns, calls) in st.iter().take(8) {
                    println!(
                        "      {name:<26} {:>9.3} {:>7.2}%  ({} 次/tick)",
                        *ns as f64 / 1e6 / ticks as f64,
                        *ns as f64 / sum.max(1) as f64 * 100.0,
                        calls / ticks as u64
                    );
                }
            }
            println!();
            rows.push((entities, b, none, off, on));
        }
    }

    // ---- 缩放结论 ----
    println!("## 缩放（bots=0）：per-entity SimMonsterAi");
    for (entities, b, _, _, on) in &rows {
        if *b != 0 || on.sim_ai_ns.is_empty() {
            continue;
        }
        let ai = mean_ms(&on.sim_ai_ns);
        println!(
            "  N={entities}: SimMonsterAi={ai:.3} ms ⇒ {:.4} µs/实体",
            ai * 1000.0 / *entities as f64
        );
    }
    println!();

    // ---- 索敌/攻击路径臂（race=81 ATMonster）----
    // race 80 **不在** `is_active_seek_race` ⇒ 怪不会 `SearchTarget`，AI 体只剩 `Wondering`/
    // `base.Run`。真栈的怪是多种 race 的混合，`ATMonster` 类会定期索敌/追打 ⇒ 用 race=81
    // 补一条「取目标/攻击」路径的分解（不给绝对基线，只看份额与计数）。
    let seek_race = env_usize_list("MIR2_SIMMON_SEEK_RACE", &[81])
        .first()
        .copied()
        .unwrap_or(81) as u8;
    println!("## 索敌路径臂（race={seek_race} ATMonster，8000 实体，8 bot，100 tick）");
    {
        let on = run_arm(8_000, bots, 100, Arm::On, 0, seek_race, true);
        print_arm("on", &on, 100);
        print_monai_breakdown(&on);
        if !on.sim_ai_ns.is_empty() {
            let ai = mean_ms(&on.sim_ai_ns);
            println!(
                "    SimMonsterAi 每实体 = {:.4} µs（{:.3} ms / 8000 实体）",
                ai * 1000.0 / 8_000.0,
                ai
            );
        }
        println!();
    }

    // ---- 死亡怪臂：定位 AI 体内唯一的全实体扫描（O(dead×N)）----
    let dead_n = env_usize_list("MIR2_SIMMON_DEAD", &[0, 8, 32, 64]);
    let dead_ticks = 100;
    println!(
        "## 死亡怪臂（N=11000, bots=0, {dead_ticks} tick）：AI 体内 `play_object_count` 全实体扫描的斜率"
    );
    for &d in &dead_n {
        let on = run_arm(11_000, 0, dead_ticks, Arm::On, d, MONSTER_RACE, false);
        let ai = mean_ms(&on.sim_ai_ns);
        println!(
            "  dead={d:>3}: SimMonsterAi={ai:.3} ms/tick ⇒ {:.4} µs/实体（含 dead 的 O(dead×N) 项）",
            ai * 1000.0 / 11_000.0
        );
        print_monai_breakdown(&on);
    }
}
