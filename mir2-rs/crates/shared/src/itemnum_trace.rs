//! **M1c-2a**：`MIR2_ITEMNUM_TRACE` —— 统一 `MakeIndex` 计数器的逐次分配旁路（诊断用）。
//!
//! 背景：M1c-2a 把 ①②③ 三个计数器并成**一个** oracle 语义的计数器
//! （`mir2_formula::drop::next_item_number`）。要证明「**单计数器内全局唯一**」
//! （跨区、跨五类造件路径逐值无重复），perf 计数器只有聚合数、帧面/DB 面都掩 `MakeIndex`
//! ⇒ 只能旁路逐值留证（M1c-2 计划 §4.1）。
//!
//! # 开关（默认关、零参与）
//!
//! 环境变量 [`TRACE_ENV`]：
//! - 未设 / 空串 / `0` ⇒ **完全不开启**：不建文件、不分配，热路径上只多一次 `Relaxed`
//!   原子读（值序列与未加旁路逐字节相同）；
//! - `1` ⇒ 写 `<当前目录>/itemnum_trace.tsv`；
//! - 其它值 ⇒ **目标路径**（若是已存在的目录 ⇒ 写 `<dir>/itemnum_trace.tsv`）。
//!
//! 打开失败 ⇒ 单条 `[itemnum-trace][ERROR]` 报错并保持关闭（**fail-open**：诊断通道
//! 不得影响游戏路径）。
//!
//! # 文件格式
//!
//! 首行 `# itemnum_trace v1 zone<TAB>make_index`；其后每次**统一计数器 CAS 成功**一行
//! `<zone>\t<make_index>`。`make_index` = CAS 后的计数值本身，**不带 tick**（T-72 有意
//! 偏离：oracle 返回 `ItemNumber + GetTickCount()`；若将来解 D-1 掩码，必须先补 tick）。
//! **只有一个计数器 ⇒ 行里没有 `counter` 字段**。
//!
//! # 区号从哪来
//!
//! 计数器是进程级共享的，分配点（`crates/formula`、`crates/world-bevy`）不认识「区」⇒
//! 由每个区线程在起线程时 [`set_thread_zone`]（**线程本地**；区线程与 zone 一一对应）。
//! 未设 ⇒ 记 `-1`；N=1 的 sim 线程显式设 `0`。

use std::cell::Cell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// 开关环境变量名（**独立于 rng trace**：`MIR2_RNG_LOG` 是另一条流水）。
pub const TRACE_ENV: &str = "MIR2_ITEMNUM_TRACE";

static ENABLED: AtomicBool = AtomicBool::new(false);
/// 进程内已写出过的 trace 行数（**诊断读数**，默认关时恒 0）。
///
/// 不是 `#[cfg(test)]`：`next_item_number` 在 `crates/formula`，那里的「默认关」牙要用它
/// 断言「跑过造件、写盘 0 行」；crate 的 `cfg(test)` 不会传染给依赖方。
static WRITTEN: AtomicU64 = AtomicU64::new(0);

static SINK: Mutex<Option<std::io::LineWriter<std::fs::File>>> = Mutex::new(None);
/// `enable_from_env` 只读一次环境变量（进程生命周期）。
static INITIALIZED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// 本线程当前服务的区号（`-1` = 未设）。
    static ZONE: Cell<i32> = const { Cell::new(-1) };
}

/// 本线程的区号（未设 ⇒ `-1`）。
#[inline]
pub fn current_zone() -> i32 {
    ZONE.with(Cell::get)
}

/// 标记当前线程服务哪个区（区线程起线程时调用一次）。
pub fn set_thread_zone(zone: u32) {
    ZONE.with(|c| c.set(i32::try_from(zone).unwrap_or(-1)));
}

/// trace 是否已开启。关闭时是热路径上唯一代价（一个 relaxed 原子读）。
#[inline]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// 已写出的 trace 行数（默认关时恒 0；`WRITTEN` 只在 `enabled()` 支里自增）。
#[inline]
pub fn written() -> u64 {
    WRITTEN.load(Ordering::Relaxed)
}

/// 记录一次分配（**统一计数器 CAS 成功后**调用）。
///
/// 关闭时**只做一次原子读**；开启时写一行 `zone\tmake_index`（`Mutex` 串行化写盘，
/// `LineWriter` 逐行 flush ⇒ 强杀进程也不丢已写行）。
#[inline]
pub fn record(make_index: i32) {
    if !enabled() {
        return;
    }
    if let Ok(mut guard) = SINK.lock() {
        if let Some(w) = guard.as_mut() {
            let _ = writeln!(w, "{}\t{make_index}", current_zone());
            let _ = w.flush();
            WRITTEN.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// 显式开启（测试 / 程序化接线用）：`create` 目标文件并写首行格式注释。
///
/// 幂等：已开启直接 `Ok(())`。
pub fn enable(path: &Path) -> std::io::Result<()> {
    if enabled() {
        return Ok(());
    }
    let file = std::fs::File::create(path)?;
    let mut w = std::io::LineWriter::new(file);
    writeln!(w, "# itemnum_trace v1 zone<TAB>make_index")?;
    w.flush()?;
    if let Ok(mut guard) = SINK.lock() {
        *guard = Some(w);
    }
    ENABLED.store(true, Ordering::Relaxed);
    Ok(())
}

/// 关掉旁路（**测试专用**：全局 sink 是进程级的，用例之间必须能复位）。
#[cfg(test)]
pub(crate) fn disable_for_tests() {
    if let Ok(mut guard) = SINK.lock() {
        guard.take();
    }
    ENABLED.store(false, Ordering::Relaxed);
    WRITTEN.store(0, Ordering::Relaxed);
    INITIALIZED.store(false, Ordering::Relaxed);
}

/// 把 env 取值解析成落盘路径（`1` ⇒ 当前目录；已存在目录 ⇒ 目录 + `itemnum_trace.tsv`）。
pub fn resolve_path(raw: &str) -> PathBuf {
    let raw = raw.trim();
    if raw == "1" {
        return PathBuf::from("itemnum_trace.tsv");
    }
    let p = PathBuf::from(raw);
    if p.is_dir() {
        return p.join("itemnum_trace.tsv");
    }
    p
}

/// 从 [`TRACE_ENV`] 读开关并开启（**进程生命周期只生效一次**）。
///
/// 由 `game-svr` 的 `run_inner` 在建区/起线程**之前**调用；未设 ⇒ 零参与。
pub fn enable_from_env() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Ok(raw) = std::env::var(TRACE_ENV) else {
        return;
    };
    let raw = raw.trim();
    if raw.is_empty() || raw == "0" {
        return;
    }
    let path = resolve_path(raw);
    if let Err(e) = enable(&path) {
        // 不静默降级：旁路仪器打不开就点名报错；游戏逻辑不受影响。
        eprintln!(
            "[itemnum-trace][ERROR] {TRACE_ENV}={raw} 打不开 {}（旁路关闭，游戏逻辑不受影响）：{e}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// 全局 sink 是进程级的 ⇒ 开启类用例串行跑。
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "itemnum-trace-{}-{}-{tag}.tsv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn default_off_records_nothing() {
        let _g = TEST_LOCK.lock().unwrap();
        disable_for_tests();
        let before = written();
        for v in 1..=1000 {
            record(v);
        }
        assert!(!enabled(), "默认必须关闭");
        assert_eq!(written(), before, "关闭时不得写盘/计数");
        assert_eq!(current_zone(), -1, "未设区号 ⇒ -1");
    }

    #[test]
    fn resolve_one_and_directory() {
        assert_eq!(resolve_path("1"), PathBuf::from("itemnum_trace.tsv"));
        assert_eq!(
            resolve_path("  /tmp/a.tsv "),
            PathBuf::from("/tmp/a.tsv"),
            "显式路径原样（去首尾空白）"
        );
        let dir = std::env::temp_dir();
        assert_eq!(
            resolve_path(dir.to_str().unwrap()),
            dir.join("itemnum_trace.tsv"),
            "已存在目录 ⇒ 目录 + itemnum_trace.tsv"
        );
    }

    #[test]
    fn enabled_writes_zone_and_value() {
        let _g = TEST_LOCK.lock().unwrap();
        disable_for_tests();
        let path = temp_path("enabled");
        enable(&path).expect("打开 trace");
        set_thread_zone(3);
        record(41);
        record(42);
        set_thread_zone(0);
        assert_eq!(written(), 2);
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "# itemnum_trace v1 zone<TAB>make_index");
        assert_eq!(lines[1], "3\t41");
        assert_eq!(lines[2], "3\t42");
        disable_for_tests();
        std::fs::remove_file(&path).ok();
    }
}
