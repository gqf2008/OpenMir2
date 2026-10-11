//! D3 成本对比 bench：**同一个掉落预算任务，三条路径的单次调用耗时**（P50/P90/P99/max）。
//!
//! 为什么入库：`D3试验田报告.md` 的成本表（+30µs/tick、≈2.4×）与设计文档 §4.2.3 的
//! owner 裁决都引用本命令，但此 bin 的源文件**从未进版本管理**（`crates/world-bevy/src/bin/`
//! 一直被 `mir2-rs/.gitignore` 的 `**/bin/` 规则吞掉）⇒ 结论不可复现。
//! 卡片：`openmir2-bevy-dropbench`（验收 (a) 重建入库）。
//!
//! ## 口径（写死；改一处基线即不可比）
//!
//! - **32 只怪 / 次，2000 次**（与历史 `tests/parity/world/d3-drop-bench.txt` 的表头一致）；
//! - 掉落表 / 物品目录 = `tests/bevy_drop_parity.rs` 里那份（金币 300 + 金创药(小量) ×1，
//!   `sel=9 / max=10`；目录只有金创药(小量)，`dura_max=1`）；怪物 id 1..=32；
//! - 固定种子 42、`MonRandomAddValue = 40`；
//! - 每条路径**先预热 100 次**（不计入样本），再计时 2000 次；百分位取最近秩
//!   `s[ceil(p/100*n)-1]`；单位 µs；
//! - 三条路径（`bevy_ecs 含建 world` / `bevy_ecs 常驻 world` / `既有实现直接调用`）在同一批
//!   样本上跑，**输出必须逐项相同**（本 bench 自检；不一致 ⇒ exit 1）；
//! - `--redcheck`：额外测一条"每只怪建一次 world"的**错误用法**，用来证明本 bench 有分辨力
//!   （不是噪声里看热闹）。
//!
//! 命令：`cargo run -p mir2-world-bevy --release --bin drop_bench [-- --redcheck]`

use std::time::Instant;

use mir2_data::models::{MonsterDropItem, StdItem, STRING_GOLD_NAME};
use mir2_formula::drop::{mon_get_random_items, ItemNumberCounter, VecItemCatalog};
use mir2_shared::rng::RandomNumber;
use mir2_world_bevy::{
    build_world, plan_drops, plan_drops_in_world, DropPlanOut, DropSnapshot, MonsterSnap,
};

const MONSTERS: usize = 32;
const CALLS: usize = 2000;
const WARMUP: usize = 100;
const SEED: i32 = 42;
const MON_RANDOM_ADD_VALUE: i32 = 40;

fn catalog() -> Vec<StdItem> {
    vec![StdItem {
        name: "金创药(小量)".to_string(),
        std_mode: 0,
        shape: 0,
        dura_max: 1,
        ..Default::default()
    }]
}

fn drop_table() -> Vec<MonsterDropItem> {
    vec![
        MonsterDropItem {
            sel_point: 9,
            max_point: 10,
            item_name: STRING_GOLD_NAME.to_string(),
            count: 300,
        },
        MonsterDropItem {
            sel_point: 9,
            max_point: 10,
            item_name: "金创药(小量)".to_string(),
            count: 1,
        },
    ]
}

fn snapshot(n: usize) -> DropSnapshot {
    DropSnapshot {
        seed: SEED,
        mon_random_add_value: MON_RANDOM_ADD_VALUE,
        catalog: catalog(),
        monsters: (0..n)
            .map(|i| MonsterSnap {
                id: i as i32 + 1,
                map_id: 0,
                x: 1 + (i as i32 % 100) * 2,
                y: 1 + (i as i32 / 100) * 2,
                drop_table: drop_table(),
            })
            .collect(),
    }
}

/// 最近秩百分位（与 `tick_bench` 同口径）：输入会被排序。
fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (p / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

/// 路径 C：既有实现（`crates/formula`）直接调用——按 id 升序、每次调用从头开一条随机流。
fn direct_call(snap: &DropSnapshot) -> Vec<DropPlanOut> {
    let mut order: Vec<&MonsterSnap> = snap.monsters.iter().collect();
    order.sort_by_key(|m| m.id);
    let mut rng = RandomNumber::with_seed(snap.seed);
    let mut counter = ItemNumberCounter::default();
    let cat = VecItemCatalog {
        items: &snap.catalog,
    };
    order
        .into_iter()
        .map(|m| {
            let out = mon_get_random_items(
                &m.drop_table,
                &cat,
                &mut rng,
                snap.mon_random_add_value,
                STRING_GOLD_NAME,
                &mut counter,
            );
            DropPlanOut {
                id: m.id,
                gold: out.gold,
                items: out.items,
            }
        })
        .collect()
}

/// 红检路径：**每只怪建一次 world**（错误用法）——同一个工作量，被 ECS 建 world 的开销吃掉。
fn redcheck_one_world_per_monster(snap: &DropSnapshot) -> Vec<DropPlanOut> {
    let mut out = Vec::with_capacity(snap.monsters.len());
    for m in &snap.monsters {
        let one = DropSnapshot {
            seed: snap.seed,
            mon_random_add_value: snap.mon_random_add_value,
            catalog: snap.catalog.clone(),
            monsters: vec![m.clone()],
        };
        out.extend(plan_drops(&one));
    }
    out
}

fn sample<F: FnMut() -> Vec<DropPlanOut>>(mut f: F) -> (Vec<u64>, Vec<DropPlanOut>) {
    for _ in 0..WARMUP {
        std::hint::black_box(f());
    }
    let mut samples = Vec::with_capacity(CALLS);
    let mut last = Vec::new();
    for _ in 0..CALLS {
        let t = Instant::now();
        last = f();
        samples.push(t.elapsed().as_micros() as u64);
    }
    (samples, last)
}

fn report(name: &str, samples: &mut [u64]) -> (u64, u64, u64) {
    samples.sort_unstable();
    let p50 = percentile(samples, 50.0);
    let p90 = percentile(samples, 90.0);
    let p99 = percentile(samples, 99.0);
    let max = *samples.last().unwrap_or(&0);
    println!("{name:<34} {p50:>7} {p90:>7} {p99:>7} {max:>7}");
    (p50, p90, p99)
}

fn checksum(out: &[DropPlanOut]) -> String {
    let gold: i64 = out.iter().map(|o| o.gold as i64).sum();
    let items: usize = out.iter().map(|o| o.items.len()).sum();
    let midx: i64 = out
        .iter()
        .flat_map(|o| o.items.iter())
        .map(|i| i.make_index as i64)
        .sum();
    format!("gold={gold} items={items} make_index_sum={midx}")
}

fn main() {
    let redcheck = std::env::args().any(|a| a == "--redcheck");
    let snap = snapshot(MONSTERS);

    // ---- 自检：三条路径对同一快照必须逐项相同（否则本 bench 测的不是同一件事）----
    let fresh = plan_drops(&snap);
    let mut resident = build_world(&snap);
    let resident_out = plan_drops_in_world(&mut resident, &snap);
    let direct = direct_call(&snap);
    if fresh != resident_out || fresh != direct {
        println!("RED: 三条路径输出不一致 ⇒ bench 无效（含建 world / 常驻 world / 直接调用）");
        std::process::exit(1);
    }

    println!(
        "# D3 试验田：掉落预算路径成本（{MONSTERS} 只怪/次，{CALLS} 次；预热 {WARMUP} 次不计入；种子 {SEED}）"
    );
    println!(
        "{:<34} {:>7} {:>7} {:>7} {:>7}",
        "路径", "P50us", "P90us", "P99us", "maxus"
    );

    let mut a = sample(|| plan_drops(&snap));
    let (a50, _, _) = report("bevy_ecs（含建 world）", &mut a.0);

    let mut world = build_world(&snap);
    let mut b = sample(|| plan_drops_in_world(&mut world, &snap));
    let (b50, _, _) = report("bevy_ecs（常驻 world）", &mut b.0);

    let mut c = sample(|| direct_call(&snap));
    let (c50, _, _) = report("既有实现（直接调用）", &mut c.0);

    if redcheck {
        let mut r = sample(|| redcheck_one_world_per_monster(&snap));
        let (r50, _, _) = report("redcheck（每只怪建一次 world）", &mut r.0);
        println!(
            "  『每只怪建一次 world』P50 = {r50} µs vs 『每次建一次 world』P50 = {a50} µs（{:.1}×）",
            r50 as f64 / a50.max(1) as f64
        );
    }

    println!("# 输出一致性：三路径逐项相同（{}）", checksum(&fresh));
    println!(
        "# 相对比：常驻 world / 直接调用 = {:.2}× ；含建 world / 直接调用 = {:.2}×",
        b50 as f64 / c50.max(1) as f64,
        a50 as f64 / c50.max(1) as f64
    );
}
