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
//! **判据（可证伪）**：
//!
//! - **不给 `--baseline`**：任一档 P99 > 100ms ⇒ 打印 `RED`、`exit 1`（100ms = 设计文档 §6 M5
//!   的 tick P99 目标；行为与历史版本**完全一致**）。把 `EntityStore` 的 `id → 槽位` 索引换成线性
//!   扫会让 P99 显著变差（M2骨架报告.md 记录过 500/1000/2000 = 4.7×/9.5×/20.2×），故本门禁有牙齿。
//! - **给 `--baseline <file>`**：只判**相对量**——每档跑 `BASELINE_ROUNDS` 轮并取各档中位数，与基线文件比
//!   `P50 劣化 > 10%` 或 `P99 中位数劣化 > 30%` ⇒ `RED` / `exit 1`。
//!   **绝对 P99 只作为报告值打印**（M5 的 100ms 目标是"空载窗口里的验收目标"，不是并行环境下的合并门禁）。
//!   轮次**跨档交错**（先各档跑第 1 轮、再回头跑第 2 轮……），不是"先把一档跑满再跑下一档"。
//! - **负载守卫**：`--baseline` 模式下若机器处于高负载窗口（CPU ≥ [`LOAD_PCT_SKIP_THRESHOLD`]% 或
//!   rustc/cargo/cargo-clippy/msbuild ≥ [`HEAVY_PROC_SKIP_THRESHOLD`] 个），相对门禁不可信 ⇒ 打印
//!   `SKIP-UNDER-LOAD` 并 `exit 0`（**不把负载导致的 RED 当作者的锅**）。但**数量级**劣化
//!   （P50 相对劣化 > [`GROSS_P50_DEGRADE_PCT`]% 或 P99 中位数 > [`GROSS_P99_DEGRADE_PCT`]%）不是负载
//!   能解释的，仍判 `RED`——门禁在负载下也保留牙齿。
//! - 某档不在基线文件里 ⇒ 打印 `SKIP`（不静默当成通过）；读基线文件失败 ⇒ `RED`。
//!
//! 口径来源：owner 裁决 `5e1daee`（同次运行内交错测量 + 只判相对量 + 绝对量降级为报告值）及其可接受的
//! 退路「检测到高负载 ⇒ SKIP-UNDER-LOAD 且 exit 0」。为什么"真正的同进程 A/B"在静态数字基线 + 本卡
//! 写集下不可实现，见 `tests/parity/world/P99复测报告.md` §5。

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use mir2_world::{Entity, World};

/// M5 的 tick P99 目标（设计文档 §6 M5-3）：不给 `--baseline` 时超过即判红。
const P99_BUDGET_US: u64 = 100_000;
/// C# `WorldServer.ProcessHumans` 的 200ms 节拍，用于算"占预算比例"。
const TICK_BUDGET_US: f64 = 200_000.0;
/// `--baseline` 模式下：相对基线允许的 P50 劣化上限（%）。
const P50_DEGRADE_LIMIT_PCT: f64 = 10.0;
/// `--baseline` 模式下：相对基线允许的 P99 中位数劣化上限（%）。
const P99_DEGRADE_LIMIT_PCT: f64 = 30.0;
/// 给了 `--baseline` 时每档跑几轮（取中位数，压机器噪声）。
const BASELINE_ROUNDS: usize = 3;
/// 高负载窗口判据：CPU 占用 ≥ 该值（%）⇒ 相对门禁降级为 `SKIP-UNDER-LOAD`。
/// 取 40%（本机多 agent 并行时常驻 40%~70%）：宁可多 SKIP，也不让并行负载制造假 RED。
const LOAD_PCT_SKIP_THRESHOLD: u8 = 40;
/// 高负载窗口判据：`rustc`/`cargo`/`cargo-clippy`/`msbuild` 进程数 ≥ 该值 ⇒ `SKIP-UNDER-LOAD`。
const HEAVY_PROC_SKIP_THRESHOLD: usize = 4;
/// 高负载窗口下仍能判红的**数量级**劣化阈值（P50，%）。
const GROSS_P50_DEGRADE_PCT: f64 = 150.0;
/// 高负载窗口下仍能判红的**数量级**劣化阈值（P99 中位数，%）。
const GROSS_P99_DEGRADE_PCT: f64 = 400.0;

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
    let relative_mode = baseline_path.is_some();
    let rounds = if relative_mode { BASELINE_ROUNDS } else { 1 };

    let load_before = if relative_mode {
        probe_load_window()
    } else {
        None
    };

    // 轮次**跨档交错**：先各档跑第 1 轮，再回头跑第 2 轮……避免"一档连续占用机器"放大单档偏置。
    let mut per_tier: Vec<TierRounds> = tiers.iter().map(|_| TierRounds::default()).collect();
    for _ in 0..rounds {
        for (idx, (entities, ticks)) in tiers.iter().enumerate() {
            let samples = run_tier(*entities, *ticks);
            per_tier[idx].push(&samples);
        }
    }

    let load = if relative_mode {
        combine_load(load_before, probe_load_window())
    } else {
        None
    };
    let loaded = should_skip_under_load(load);

    println!("# 每 tick 预算 = 200ms（C# WorldServer.ProcessHumans 节拍）");
    if relative_mode {
        println!(
            "# 时间戳 {}；负载探针 = {}；重型进程阈值 = {}；CPU 阈值 = {}%",
            unix_now(),
            load_describe(load),
            HEAVY_PROC_SKIP_THRESHOLD,
            LOAD_PCT_SKIP_THRESHOLD
        );
    }
    println!(
        "{:<10} {:<10} {:>9} {:>9} {:>9} {:>9} {:>11}",
        "实体数", "tick数", "P50us", "P90us", "P99us", "maxus", "P99/预算"
    );

    let mut red = false;
    let mut skip_under_load = false;
    let mut skip_missing = false;
    for (idx, (entities, ticks)) in tiers.iter().enumerate() {
        let (p50, p90, p99, max) = per_tier[idx].medians();
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

        if !relative_mode {
            // 历史行为：只判绝对 P99 预算。
            if over_budget(p99) {
                red = true;
            }
            continue;
        }

        // 相对模式：绝对 P99 降级为报告值（仅在空载窗口才代表 M5 目标）。
        println!(
            "           绝对 P99 = {p99}us（报告值；M5 目标 {P99_BUDGET_US}us，仅空载窗口可作为判据）"
        );

        let Some(b) = baseline
            .iter()
            .find(|b| b.entities == *entities && b.ticks == *ticks)
        else {
            println!("           基线里没有 {entities}/{ticks} 档 ⇒ SKIP（不参与相对判定）");
            skip_missing = true;
            continue;
        };
        println!(
            "           基线 P50/P99 = {}/{}，本次中位数 = {}/{} ⇒ ΔP50 {:+.1}%、ΔP99 {:+.1}%",
            b.p50,
            b.p99,
            p50,
            p99,
            degrade_pct(p50, b.p50),
            degrade_pct(p99, b.p99)
        );
        if loaded {
            if let Some(reason) = gross_degrade_reason(p50, p99, *b) {
                println!("RED: {entities} 档{reason}（超出负载可解释范围，负载下仍判红）");
                red = true;
            } else {
                println!("           高负载窗口内相对量不可信 ⇒ 本档 SKIP-UNDER-LOAD（不判红）");
                skip_under_load = true;
            }
        } else if let Some(reason) = regression_reason(p50, p99, *b) {
            println!("RED: {entities} 档{reason}");
            red = true;
        } else {
            println!("           OK（相对门禁在阈值内）");
        }
    }

    if red {
        println!("RED: 门禁不通过（相对基线劣化，或数量级劣化超出负载可解释范围）");
        std::process::exit(1);
    }
    if relative_mode {
        if skip_under_load {
            println!(
                "SKIP-UNDER-LOAD: {}，相对门禁不可信 ⇒ 退出码 0（不是通过、也不是失败）",
                load_describe(load)
            );
        } else if skip_missing {
            println!("SKIP: 基线缺档（见上）⇒ 不判通过也不判失败，退出码 0");
        } else {
            println!(
                "GREEN: 相对基线 P50 劣化 ≤ {P50_DEGRADE_LIMIT_PCT}%、P99 中位数劣化 ≤ {P99_DEGRADE_LIMIT_PCT}%"
            );
        }
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

/// 一档的多轮统计（每轮 4 个百分位；中位数在 [`Self::medians`] 里取）。
#[derive(Default)]
struct TierRounds {
    p50: Vec<u64>,
    p90: Vec<u64>,
    p99: Vec<u64>,
    max: Vec<u64>,
}

impl TierRounds {
    fn push(&mut self, samples: &[u64]) {
        self.p50.push(percentile(samples, 50.0));
        self.p90.push(percentile(samples, 90.0));
        self.p99.push(percentile(samples, 99.0));
        self.max.push(*samples.last().unwrap());
    }

    fn medians(&self) -> (u64, u64, u64, u64) {
        let med = |v: &[u64]| {
            let mut s = v.to_vec();
            s.sort_unstable();
            median(&s)
        };
        (
            med(&self.p50),
            med(&self.p90),
            med(&self.p99),
            med(&self.max),
        )
    }
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
fn median(sorted: &[u64]) -> u64 {
    sorted[(sorted.len() - 1) / 2]
}

/// 相对基线的劣化百分比（可为负 = 变快）。
fn degrade_pct(cur: u64, base: u64) -> f64 {
    if base == 0 {
        0.0
    } else {
        (cur as f64 - base as f64) / base as f64 * 100.0
    }
}

/// 相对基线判回归：`P50 劣化 > 10%` 或 `P99 中位数劣化 > 30%` ⇒ 返回原因；否则 `None`。
fn regression_reason(cur_p50: u64, cur_p99: u64, base: BaselineTier) -> Option<String> {
    let d50 = degrade_pct(cur_p50, base.p50);
    let d99 = degrade_pct(cur_p99, base.p99);
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

/// 高负载窗口下仍能判红的"数量级"劣化（负载不可能解释的幅度）。
fn gross_degrade_reason(cur_p50: u64, cur_p99: u64, base: BaselineTier) -> Option<String> {
    let d50 = degrade_pct(cur_p50, base.p50);
    let d99 = degrade_pct(cur_p99, base.p99);
    if d50 > GROSS_P50_DEGRADE_PCT {
        return Some(format!(" P50 劣化 {d50:.1}% > {GROSS_P50_DEGRADE_PCT}%"));
    }
    if d99 > GROSS_P99_DEGRADE_PCT {
        return Some(format!(
            " P99 中位数劣化 {d99:.1}% > {GROSS_P99_DEGRADE_PCT}%"
        ));
    }
    None
}

/// 机器负载探针（`--baseline` 模式下用于把"负载导致的相对劣化"降级为 SKIP）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoadProbe {
    cpu_pct: u8,
    heavy_procs: usize,
}

/// 高负载窗口 ⇒ 相对门禁不可信（返回 `true`）。探针不可用（非 Windows / PowerShell 失败）⇒ 不降级。
fn should_skip_under_load(probe: Option<LoadProbe>) -> bool {
    probe.is_some_and(|p| {
        p.cpu_pct >= LOAD_PCT_SKIP_THRESHOLD || p.heavy_procs >= HEAVY_PROC_SKIP_THRESHOLD
    })
}

/// 用前后两次探针的较大值（取运行区间内的峰值负载）。
fn combine_load(a: Option<LoadProbe>, b: Option<LoadProbe>) -> Option<LoadProbe> {
    match (a, b) {
        (Some(x), Some(y)) => Some(LoadProbe {
            cpu_pct: x.cpu_pct.max(y.cpu_pct),
            heavy_procs: x.heavy_procs.max(y.heavy_procs),
        }),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

/// 解析探针输出 `"<cpu_pct> <heavy_procs>"`（取最后一行非空文本）。
fn parse_load_probe(stdout: &str) -> Option<LoadProbe> {
    let line = stdout.lines().rev().find(|l| !l.trim().is_empty())?;
    let mut it = line.split_whitespace();
    Some(LoadProbe {
        cpu_pct: it.next()?.parse().ok()?,
        heavy_procs: it.next()?.parse().ok()?,
    })
}

fn load_describe(load: Option<LoadProbe>) -> String {
    match load {
        Some(p) => format!("CPU {}% / 重型进程 {} 个", p.cpu_pct, p.heavy_procs),
        None => "不可用（探针失败 ⇒ 不做负载守卫）".to_string(),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 采集当前 CPU 占用与重型编译进程数（Windows：CIM，一次 PowerShell 调用）。
#[cfg(windows)]
fn probe_load_window() -> Option<LoadProbe> {
    const SCRIPT: &str = "$c=(Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average; $p=@(Get-Process -Name rustc,cargo,cargo-clippy,msbuild -ErrorAction SilentlyContinue).Count; Write-Output ('{0} {1}' -f [int]$c, $p)";
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_load_probe(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(windows))]
fn probe_load_window() -> Option<LoadProbe> {
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

    #[test]
    fn degrade_pct_is_signed_and_guards_zero_base() {
        assert!((degrade_pct(110, 100) - 10.0).abs() < f64::EPSILON);
        assert!((degrade_pct(90, 100) + 10.0).abs() < f64::EPSILON);
        assert_eq!(degrade_pct(123, 0), 0.0, "零基线下不制造无穷大");
    }

    /// 负载守卫的两个触发条件：CPU 过高、或重型编译进程过多；探针不可用时不降级。
    #[test]
    fn load_guard_triggers_on_cpu_or_heavy_processes() {
        assert!(!should_skip_under_load(None), "无探针 ⇒ 不做负载守卫");
        assert!(!should_skip_under_load(Some(LoadProbe {
            cpu_pct: LOAD_PCT_SKIP_THRESHOLD - 1,
            heavy_procs: HEAVY_PROC_SKIP_THRESHOLD - 1,
        })));
        assert!(
            should_skip_under_load(Some(LoadProbe {
                cpu_pct: LOAD_PCT_SKIP_THRESHOLD,
                heavy_procs: 0,
            })),
            "CPU 达到阈值即 SKIP"
        );
        assert!(
            should_skip_under_load(Some(LoadProbe {
                cpu_pct: 0,
                heavy_procs: HEAVY_PROC_SKIP_THRESHOLD,
            })),
            "重型进程达到阈值即 SKIP"
        );
    }

    /// 数量级劣化：负载导致的抖动（实测最高 P50 +116%、P99 +288%）不足以触发；
    /// 线性扫这类真回归（+500%~+2771%）必然判红——即使在高负载窗口。
    #[test]
    fn gross_thresholds_only_fire_on_order_of_magnitude() {
        let base = BaselineTier {
            entities: 500,
            ticks: 2000,
            p50: 400,
            p99: 600,
        };
        assert!(
            gross_degrade_reason(864, 2328, base).is_none(),
            "负载实测上限（P50 +116% / P99 +288%）不触发数量级判红"
        );
        assert!(
            gross_degrade_reason(2400, 3600, base)
                .unwrap()
                .contains("P50"),
            "+500% 必须判数量级劣化"
        );
        assert!(
            gross_degrade_reason(400, 6000, base)
                .unwrap()
                .contains("P99"),
            "+900% P99 必须判数量级劣化"
        );
    }

    #[test]
    fn combine_load_takes_peak_of_two_probes() {
        let a = LoadProbe {
            cpu_pct: 30,
            heavy_procs: 1,
        };
        let b = LoadProbe {
            cpu_pct: 70,
            heavy_procs: 5,
        };
        assert_eq!(
            combine_load(Some(a), Some(b)),
            Some(LoadProbe {
                cpu_pct: 70,
                heavy_procs: 5
            })
        );
        assert_eq!(combine_load(Some(a), None), Some(a));
        assert_eq!(combine_load(None, Some(b)), Some(b));
        assert_eq!(combine_load(None, None), None);
    }

    #[test]
    fn parse_load_probe_reads_last_non_empty_line() {
        assert_eq!(
            parse_load_probe("WARNING: something\n17 2\n"),
            Some(LoadProbe {
                cpu_pct: 17,
                heavy_procs: 2
            })
        );
        assert_eq!(
            parse_load_probe("0 0"),
            Some(LoadProbe {
                cpu_pct: 0,
                heavy_procs: 0
            })
        );
        assert_eq!(parse_load_probe("garbage"), None);
        assert_eq!(parse_load_probe(""), None);
    }
}
