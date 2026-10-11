//! 世界 tick 性能门禁（M2 DoD ④ / 设计文档 §4.2.3、§6 M5-3）。
//!
//! ```text
//! cargo run -p mir2-world --release --bin tick_bench -- 500 2000 1000 2000 2000 1000
//! # 参数成对：(实体数, tick 数) ...
//! ```
//!
//! **场景（与 `tests/parity/world/M2骨架报告.md:45-46` 写死的一致，勿改，否则基线不可比）**：
//! 200×200 地图；实体铺在奇数坐标上、相邻 2 格；固定种子 42；每 tick 让 `i % 4 == tick % 4`
//! 的那 1/4 实体走一步，方向每 4 tick 翻转一次（在 2 格带内来回，不撞地图边界）。
//! 首 tick 只做"入图"（`add_entity` 排队的 Enter 命令一次性落图），作为**预热不计入样本**。
//!
//! **输出**：每档一行 `实体数 tick数 P50us P90us P99us maxus P99/预算`（百分位取最近秩
//! `s[ceil(p/100*len)-1]`），与 `tests/parity/world/p99-baseline*.txt` 同格式。
//!
//! **判据（可证伪）**：任一档 P99 > 100ms ⇒ 打印 `RED` 并以退出码 1 结束（100ms = 设计文档
//! §6 M5 的 tick P99 目标）。把 `EntityStore` 的 `id → 槽位` 索引换成线性扫会让 P99 显著变差
//! （M2骨架报告.md 记录过 500/1000/2000 = 4.7×/9.5×/20.2×），故本门禁有牙齿。

use std::time::Instant;

use mir2_world::{Entity, World};

/// M5 的 tick P99 目标（设计文档 §6 M5-3）：超过即判红。
const P99_BUDGET_US: u64 = 100_000;
/// C# `WorldServer.ProcessHumans` 的 200ms 节拍，用于算"占预算比例"。
const TICK_BUDGET_US: f64 = 200_000.0;

const MAP: i32 = 200;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || !args.len().is_multiple_of(2) {
        eprintln!("usage: tick_bench <entities> <ticks> [<entities> <ticks> ...]");
        std::process::exit(2);
    }
    let tiers: Vec<(usize, usize)> = args
        .chunks(2)
        .map(|p| (p[0].parse().unwrap(), p[1].parse().unwrap()))
        .collect();

    println!("# 每 tick 预算 = 200ms（C# WorldServer.ProcessHumans 节拍）");
    println!(
        "{:<10} {:<10} {:>9} {:>9} {:>9} {:>9} {:>11}",
        "实体数", "tick数", "P50us", "P90us", "P99us", "maxus", "P99/预算"
    );

    let mut red = false;
    for (entities, ticks) in tiers {
        let samples = run_tier(entities, ticks);
        let p50 = percentile(&samples, 50.0);
        let p90 = percentile(&samples, 90.0);
        let p99 = percentile(&samples, 99.0);
        let max = *samples.last().unwrap();
        println!(
            "{:<10} {:<10} {:>9} {:>9} {:>9} {:>9} {:>10.3}%",
            entities,
            ticks,
            p50,
            p90,
            p99,
            max,
            p99 as f64 / TICK_BUDGET_US * 100.0
        );
        if over_budget(p99) {
            red = true;
        }
    }
    if red {
        println!("RED: 有档位 P99 > {P99_BUDGET_US}us（M5 目标），门禁不通过");
        std::process::exit(1);
    }
    println!("GREEN: 三档 P99 均在 {P99_BUDGET_US}us 以内");
}

/// 跑一档，返回每 tick 的耗时（µs，升序）。
fn run_tier(entities: usize, ticks: usize) -> Vec<u64> {
    let mut world = World::new(MAP, MAP);
    let mut ids = Vec::with_capacity(entities);
    for i in 0..entities {
        // 奇数坐标、相邻 2 格：x = 1,3,..,199（100 列）；y = 1 + 2*(i/100)
        let x = 1 + 2 * (i as i32 % 100);
        let y = 1 + 2 * (i as i32 / 100);
        let mut e = Entity::new((i + 1) as i32, format!("b{i}"), x, y);
        e.race = 0;
        ids.push(world.add_entity(e));
    }
    world.tick(); // 预热：入图（Enter 命令一次性落图），不计入样本

    let mut samples = Vec::with_capacity(ticks);
    for tick in 1..=ticks {
        let dx = if (tick / 4) % 2 == 0 { 1 } else { -1 };
        for (i, id) in ids.iter().enumerate() {
            if i % 4 == tick % 4 {
                world.walk(*id, dx, 0);
            }
        }
        let t0 = Instant::now();
        world.tick();
        samples.push(t0.elapsed().as_micros() as u64);
    }
    samples.sort_unstable();
    samples
}

/// 最近秩（nearest-rank）：`s[ceil(p/100 * len) - 1]`。
fn percentile(sorted: &[u64], p: f64) -> u64 {
    let len = sorted.len();
    let rank = ((p / 100.0) * len as f64).ceil() as usize;
    sorted[rank.clamp(1, len) - 1]
}

/// 门禁判据（独立成函数，便于断言"哪一侧会红"）。
fn over_budget(p99_us: u64) -> bool {
    p99_us > P99_BUDGET_US
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_is_nearest_rank() {
        let s: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&s, 50.0), 50);
        assert_eq!(percentile(&s, 99.0), 99);
        assert_eq!(percentile(&s, 100.0), 100);
        assert_eq!(percentile(&s, 0.0), 1, "p=0 归到最小样本（秩下限钳到 1）");
    }

    /// 门禁判据本身可断言：150ms > 100ms 预算 ⇒ 必须判红；100ms 恰好不算超。
    #[test]
    fn budget_threshold_flags_over_budget() {
        assert!(!over_budget(50_000));
        assert!(
            !over_budget(100_000),
            "恰好等于 100ms 不算超（M5 目标是 ≤ 100ms）"
        );
        assert!(over_budget(100_001));
        assert!(over_budget(150_000));
    }
}
