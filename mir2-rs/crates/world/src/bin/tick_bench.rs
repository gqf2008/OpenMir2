//! 世界 tick 性能门禁（M2 DoD ④ / 设计文档 §4.2.3、§6 M5-3）。
//!
//! ```text
//! cargo run -p mir2-world --release --bin tick_bench -- 500 2000 1000 2000 2000 1000
//! # 参数成对：(实体数, tick 数) ...
//! cargo run -p mir2-world --release --bin tick_bench -- \
//!     --baseline mir2-rs/tests/parity/world/p99-baseline-tickbench.txt 500 2000 1000 2000 2000 1000
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
//!
//! **`--baseline <file>` 相对门禁**（`P99复测报告.md` §5 的手工口径机判化）：给了基线文件时，
//! 每档跑 **3 轮**、取三轮的 **P50 中位数**与 **P99 中位数**，与基线比：
//! `P50 劣化 > 10%` 或 `P99 中位数劣化 > 30%` ⇒ 打印 `RED` 并 `exit 1`。
//! 不给 `--baseline` 时行为与以前**完全一致**（只跑 1 轮、只判绝对 P99 预算）。
//! 某档不在基线文件里 ⇒ 打印 `SKIP`（不参与相对判定），避免"基线缺档"被静默当成通过。

use std::time::Instant;

use mir2_world::{Entity, World};

/// M5 的 tick P99 目标（设计文档 §6 M5-3）：超过即判红。
const P99_BUDGET_US: u64 = 100_000;
/// C# `WorldServer.ProcessHumans` 的 200ms 节拍，用于算"占预算比例"。
const TICK_BUDGET_US: f64 = 200_000.0;
/// `--baseline` 模式下：相对基线允许的 P50 劣化上限（%）。
const P50_DEGRADE_LIMIT_PCT: f64 = 10.0;
/// `--baseline` 模式下：相对基线允许的 P99 中位数劣化上限（%）。
const P99_DEGRADE_LIMIT_PCT: f64 = 30.0;
/// 给了 `--baseline` 时每档跑几轮（取中位数，压机器噪声）。
const BASELINE_ROUNDS: usize = 3;

const MAP: i32 = 200;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut baseline_path: Option<String> = None;
    if let Some(i) = args.iter().position(|a| a == "--baseline") {
        if i + 1 >= args.len() {
            eprintln!("usage: tick_bench [--baseline <file>] <entities> <ticks> [...]");
            std::process::exit(2);
        }
        baseline_path = Some(args.remove(i + 1));
        args.remove(i);
    }
    if args.is_empty() || !args.len().is_multiple_of(2) {
        eprintln!("usage: tick_bench [--baseline <file>] <entities> <ticks> [...]");
        std::process::exit(2);
    }
    let tiers: Vec<(usize, usize)> = args
        .chunks(2)
        .map(|p| (p[0].parse().unwrap(), p[1].parse().unwrap()))
        .collect();
    let baseline: Vec<BaselineTier> = match &baseline_path {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(text) => parse_baseline(&text),
            Err(e) => {
                eprintln!("RED: 读基线文件失败 {p}: {e}");
                std::process::exit(1);
            }
        },
        None => Vec::new(),
    };
    let rounds = if baseline_path.is_some() {
        BASELINE_ROUNDS
    } else {
        1
    };

    println!("# 每 tick 预算 = 200ms（C# WorldServer.ProcessHumans 节拍）");
    println!(
        "{:<10} {:<10} {:>9} {:>9} {:>9} {:>9} {:>11}",
        "实体数", "tick数", "P50us", "P90us", "P99us", "maxus", "P99/预算"
    );

    let mut red = false;
    for (entities, ticks) in tiers {
        // 每轮取 (p50, p90, p99, max)；`--baseline` 下取三轮的中位数，否则就是那一轮。
        let mut p50s = Vec::with_capacity(rounds);
        let mut p90s = Vec::with_capacity(rounds);
        let mut p99s = Vec::with_capacity(rounds);
        let mut maxes = Vec::with_capacity(rounds);
        for _ in 0..rounds {
            let samples = run_tier(entities, ticks);
            p50s.push(percentile(&samples, 50.0));
            p90s.push(percentile(&samples, 90.0));
            p99s.push(percentile(&samples, 99.0));
            maxes.push(*samples.last().unwrap());
        }
        p50s.sort_unstable();
        p90s.sort_unstable();
        p99s.sort_unstable();
        maxes.sort_unstable();
        let p50 = median(&p50s);
        let p90 = median(&p90s);
        let p99 = median(&p99s);
        let max = median(&maxes);
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
        if baseline_path.is_some() {
            match baseline
                .iter()
                .find(|b| b.entities == entities && b.ticks == ticks)
            {
                Some(b) => {
                    println!(
                        "           基线 P50/P99 = {}/{}，本次三轮中位数 = {}/{}  ⇒ {}",
                        b.p50,
                        b.p99,
                        p50,
                        p99,
                        regression_reason(p50, p99, *b).unwrap_or_else(|| "OK".to_string())
                    );
                    if let Some(reason) = regression_reason(p50, p99, *b) {
                        println!("RED: {entities} 档{reason}");
                        red = true;
                    }
                }
                None => {
                    println!("           基线里没有 {entities}/{ticks} 档 ⇒ SKIP（不参与相对判定）")
                }
            }
        }
    }
    if red {
        println!("RED: 门禁不通过（绝对 P99 预算 和/或 相对基线劣化）");
        std::process::exit(1);
    }
    if baseline_path.is_some() {
        println!(
            "GREEN: 绝对 P99 ≤ {P99_BUDGET_US}us 且相对基线 P50 劣化 ≤ {P50_DEGRADE_LIMIT_PCT}%、P99 中位数劣化 ≤ {P99_DEGRADE_LIMIT_PCT}%"
        );
    } else {
        println!("GREEN: 三档 P99 均在 {P99_BUDGET_US}us 以内");
    }
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

/// 基线文件里的一档（与 `tests/parity/world/p99-baseline*.txt` 同格式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BaselineTier {
    entities: usize,
    ticks: usize,
    p50: u64,
    p99: u64,
}

/// 解析基线文件：跳过 `#` 注释与表头行，取 `实体数 tick数 P50 P90 P99 max P99/预算`（后两列忽略）。
#[must_use]
fn parse_baseline(text: &str) -> Vec<BaselineTier> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("实体数") {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 {
            continue;
        }
        let (Ok(entities), Ok(ticks), Ok(p50), Ok(p99)) = (
            cols[0].parse::<usize>(),
            cols[1].parse::<usize>(),
            cols[2].parse::<u64>(),
            cols[4].parse::<u64>(),
        ) else {
            continue;
        };
        out.push(BaselineTier {
            entities,
            ticks,
            p50,
            p99,
        });
    }
    out
}

/// 中位数（输入需已升序；偶数个取偏小那个——确定性优先，够压机器噪声）。
#[must_use]
fn median(sorted: &[u64]) -> u64 {
    sorted[(sorted.len() - 1) / 2]
}

/// 相对基线判回归：`P50 劣化 > 10%` 或 `P99 中位数劣化 > 30%` ⇒ 返回原因；否则 `None`。
#[must_use]
fn regression_reason(cur_p50: u64, cur_p99: u64, base: BaselineTier) -> Option<String> {
    let pct = |cur: u64, b: u64| -> f64 {
        if b == 0 {
            0.0
        } else {
            (cur as f64 - b as f64) / b as f64 * 100.0
        }
    };
    let d50 = pct(cur_p50, base.p50);
    let d99 = pct(cur_p99, base.p99);
    if d50 > P50_DEGRADE_LIMIT_PCT {
        return Some(format!(
            " P50 劣化 {d50:.1}% > {P50_DEGRADE_LIMIT_PCT}%（基线 {}us，本次 {cur_p50}us）",
            base.p50
        ));
    }
    if d99 > P99_DEGRADE_LIMIT_PCT {
        return Some(format!(
            " P99 中位数劣化 {d99:.1}% > {P99_DEGRADE_LIMIT_PCT}%（基线 {}us，本次 {cur_p99}us）",
            base.p99
        ));
    }
    None
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

    /// 基线解析：能读真文件（注释 + 表头 + 三档），并跳过不合法行。
    #[test]
    fn parse_baseline_reads_committed_format() {
        let text = "# 注释\n实体数        tick数          P50us     P90us     P99us     maxus      P99/预算\n\
                    500        2000             377       403       576       677      0.288%\n\
                    garbage line\n\
                    1000       2000             840       870      1195      1338      0.598%\n";
        let tiers = parse_baseline(text);
        assert_eq!(tiers.len(), 2, "注释/表头/垃圾行都要跳过：{tiers:?}");
        assert_eq!(
            tiers[0],
            BaselineTier {
                entities: 500,
                ticks: 2000,
                p50: 377,
                p99: 576
            }
        );
        assert_eq!(tiers[1].p99, 1195);
    }

    #[test]
    fn median_takes_lower_middle_of_even_count() {
        assert_eq!(median(&[5]), 5);
        assert_eq!(median(&[3, 7, 9]), 7);
        assert_eq!(median(&[3, 7]), 3, "偶数个取偏小那个（确定性优先）");
    }

    /// 相对门禁的两个阈值：恰好 +10%/+30% 不算劣化，超过才判红（可证伪）。
    #[test]
    fn regression_limits_are_exact() {
        let base = BaselineTier {
            entities: 500,
            ticks: 2000,
            p50: 400,
            p99: 600,
        };
        assert!(
            regression_reason(440, 780, base).is_none(),
            "恰好 +10% / +30% 不算劣化"
        );
        assert!(
            regression_reason(441, 600, base)
                .unwrap()
                .contains("P50 劣化"),
            "+10.25% 必须判 P50 劣化"
        );
        assert!(
            regression_reason(400, 781, base).unwrap().contains("P99"),
            "+30.2% 必须判 P99 劣化"
        );
        assert!(regression_reason(300, 400, base).is_none(), "变快不算劣化");
    }
}
