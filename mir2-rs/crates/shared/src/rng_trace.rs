//! **M3②③ 决策片（owner 2026-10-12 18:00）**：opt-in 的**逐次 RNG trace**（D2 口径）。
//!
//! 背景：`attack_rng_ledger.py` 能把 C# 半的 `MIR2_RNG_LOG` 流水按「每次攻击恰一次的站点」逐 k
//! 分段，但 **Rust 生产路径此前只有 `SEED_PROBE`（8 项自证）**，没有 per-call trace ⇒ 两侧账本
//! 逐 k `diff` 跑不了（m3bag 2026-10-12 已登记，转 m3cs）。本模块补上 Rust 半：
//!
//! - 落点：**全局随机源本身**（[`crate::rng::RandomNumber`] 的每个取数入口），不在 `crates/world`
//!   各调用点手工打点 ⇒ 只要生产路径走 `RandomNumber` 就一定被记录，不会漏消费者；
//! - 形态：与 C# 钩子（`mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs`）**逐列同形**：
//!     `seq \t kind \t max/arg \t result \t site`
//!   头两行同形：`1\tINJECT_OK\t<seed>\t0` 与 `SEED_PROBE <8 项>`；`site = 调用点 file:line`；
//! - 开关：环境变量 [`TRACE_ENV`]（沿用 C# 的 `MIR2_RNG_LOG`）：**未设/空串 ⇒ 完全不开启**
//!   （每次取数只多一个 `AtomicBool` 读，值序列与改动前逐字节相同）；设了 ⇒ 落该路径的 TSV；
//! - **不引入新的随机源**：trace 只是旁路记录，绝不消耗/改动 `System.Random` 的状态。
//!
//! `#[track_caller]` 链：`RandomNumber` 的 inherent 方法 + `RandomSource` 的 trait 方法都标了
//! `#[track_caller]`，这样 `crates/world` 里 `rng.random_range(...)` 记到的是**世界层调用点**
//! （如 `crates/world/src/combat.rs:105`），而不是 `crates/shared/src/rng.rs` 的包装层——否则
//! 所有取数的 site 都长一样，账本无法把「攻击路径取数」与「其它消费者」分开。
//!
//! 线程：C# 侧是全局单例 + 多线程；Rust 侧 `RandomNumber` 按持有者分（`World.rng`）。本 sink 用
//! `Mutex` 串行化写盘，`seq` 全局单调 ⇒ 与 C# 的 `RecordingRandom` 同口径（记录的是「写盘顺序」，
//! 亦即实际取数顺序）。

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

/// 开启 trace 的环境变量名（**与 C# 钩子同名**：`StartupHook.cs` 的 `MIR2_RNG_LOG`）。
pub const TRACE_ENV: &str = "MIR2_RNG_LOG";

/// `SEED_PROBE` 行长度（照 C# 钩子：`new Random(seed).Next()` 取 8 项）。
pub const PROBE_LEN: usize = 8;

static ENABLED: AtomicBool = AtomicBool::new(false);
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static SEQ: AtomicI64 = AtomicI64::new(0);

type Sink = Mutex<std::io::LineWriter<std::fs::File>>;
static SINK: OnceLock<Sink> = OnceLock::new();

/// trace 是否已开启。关闭时是热路径上唯一代价（一个 relaxed 原子读）。
#[inline]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// 显式开启（测试 / 程序化接线用）：打开 `path`，写与 C# 钩子同形的两行头。
///
/// 幂等：已开启则直接 `Ok(())`。用 `LineWriter` 逐行 flush，保证进程被 `Stop-Process -Force`
/// 强杀时已写出的行不丢（C# `StreamWriter.AutoFlush = true` 同口径）。
pub fn enable(path: &Path, seed: i32) -> std::io::Result<()> {
    if enabled() {
        return Ok(());
    }
    let file = std::fs::File::create(path)?;
    let mut w = std::io::LineWriter::new(file);
    writeln!(w, "1\tINJECT_OK\t{seed}\t0")?;
    // 参照序列用**独立实例**（`new Random(seed).Next()`），不消耗游戏随机流——照 C# 钩子。
    let mut probe = crate::rng::DotNetRandom::new(seed);
    let vals = (0..PROBE_LEN)
        .map(|_| probe.next_value().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    writeln!(w, "SEED_PROBE {vals}")?;
    w.flush()?;
    let _ = SINK.set(Mutex::new(w));
    ENABLED.store(true, Ordering::Relaxed);
    Ok(())
}

/// 从环境读 [`TRACE_ENV`]（`MIR2_RNG_LOG`）并在设了非空路径时开启。**只生效一次**（进程生命周期）。
///
/// 头部 `INJECT_OK <seed>` / `SEED_PROBE` 的种子取自**同一个 `MIR2_RNG_SEED`**（T-30 注入口）⇒
/// 即使 `crates/world::World::with_maps` 先用缺省 seed 42 构造过随机源，头部也不会把注入口
/// 写错。两个变数缺一（或种子非法）⇒ **不开启**并点名报告（不静默降级；trace 是旁路仪器，
/// 不开启不影响游戏逻辑）。
///
/// 由 [`crate::rng::RandomNumber::with_seed`] 调用 ⇒ 生产路径（`bins/game-svr`）无需任何改动，
/// 只要把 `MIR2_RNG_LOG`/`MIR2_RNG_SEED` 透传进 `game-svr` 子进程环境即可
/// （`start_rust_stack.ps1` 的 `Start-Process` 默认继承调用方环境 ⇒ 在起栈 shell 里设一次就够）。
pub fn enable_from_env() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Ok(raw) = std::env::var(TRACE_ENV) else {
        return;
    };
    if raw.is_empty() {
        return;
    }
    // 种子与 `bins/game-svr/src/seed.rs::parse_seed` 同口径（`IsNullOrEmpty` + `TryParse`）。
    let seed = match std::env::var("MIR2_RNG_SEED") {
        Ok(s) if !s.is_empty() => match s.trim().parse::<i32>() {
            Ok(v) => v,
            Err(_) => {
                eprintln!(
                    "[rng-trace][ERROR] {TRACE_ENV}={raw} 但 MIR2_RNG_SEED 不是合法 int: {s}（trace 关闭）"
                );
                return;
            }
        },
        _ => {
            eprintln!(
                "[rng-trace][ERROR] {TRACE_ENV}={raw} 但 MIR2_RNG_SEED 未设/为空 ⇒ 无法自证种子（trace 关闭）"
            );
            return;
        }
    };
    if let Err(e) = enable(Path::new(&raw), seed) {
        // 不静默降级：trace 是旁路仪器，打不开就点名报错，但不影响游戏逻辑（值序列不变）。
        eprintln!("[rng-trace][ERROR] 无法打开 {raw}: {e}（trace 关闭，游戏逻辑不受影响）");
    }
}

/// 记录一次取数（C# `RecordingRandom` 的同形）。
///
/// `kind` / `arg` 对应 C# 的调用形态与参数：`Next()`→arg=0、`Next(max)`→arg=max、
/// `Next(min,max)`→arg=`(min<<32)|max`、`NextDouble()`→arg=0。
#[inline]
pub fn record(kind: &str, arg: i64, result: i64, file: &'static str, line: u32) {
    if !enabled() {
        return;
    }
    if let Some(sink) = SINK.get() {
        if let Ok(mut w) = sink.lock() {
            let seq = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = writeln!(w, "{seq}\t{kind}\t{arg}\t{result}\t{file}:{line}");
        }
    }
}
