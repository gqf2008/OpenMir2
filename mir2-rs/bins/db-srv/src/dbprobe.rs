//! `[DBSEL]` 选角（`CM_SELCHR`）分步探针——**env 门控、零行为**。
//!
//! 背景：窄带 8000 窗里 `sel_chr` 成批 60 s 超时（SelGate 侧看到"会话不存在、丢弃 44 字节"），
//! 需要把一次选角的耗时拆开：等锁 / 等连接池 / 真查库 / 其余。
//!
//! **门**：只有 `MIR2_DBSEL_PROBE=1`（字面 `"1"`，可含空白）开启；缺省/其它值一律关闭。
//! 关闭态每次选角只读一次 `OnceLock<bool>`：不取 `Instant`、不计数、不打印。
//!
//! **行格式**（字段固定，供报告机读）：
//! `[DBSEL] n=<累计> wait_ms=<入口→调 select_chr> lock_ms=<login 锁+会话检查>
//!  pool_ms=<独立采样的一次池获取> db_ms=<select_chr 内 DB 层累计>
//!  rest_ms=<total−wait−lock−db> total_ms=<入口→回包前> chr=<角色名> ok=<0|1>`
//!
//! **打印频率**：每 `MIR2_DBSEL_PROBE_EVERY`（缺省 50）条一次，或单条 `total_ms ≥
//! `MIR2_DBSEL_PROBE_SLOW_MS`（缺省 1000）时必打。
//!
//! **口径提醒**：`pool_ms` 是**另取一次连接**的等待时间（独立采样，不改查询路径）——
//! 它反映当时的池压力，不等于查询内部各自取连接的耗时。
//!
//! **口径变更日志（card/selectchr-roundtrip，2026-11-26）**：
//! - 选角/查角改为**整条复用一条连接**（`Db::acquire()` 一次）⇒ 新增 `acq_ms`（取连接耗时）
//!   与 `acq_n`（本次请求的 acquire 次数，期望 1）、`sql_n`（本次请求的 SQL 语句数）。
//! - `db_ms` 定义收窄为**仅 SQL execute 之和**（不再含 acquire）；此前 `db_ms` 实际是
//!   Σ(acquire + execute)。`rest_ms = total − wait − lock − acq − db`。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

/// 开关（只有字面 `"1"` 开启）。
pub const ENV_ENABLE: &str = "MIR2_DBSEL_PROBE";
/// 每 N 条打一行（缺省 50；非法/0 ⇒ 50）。
pub const ENV_EVERY: &str = "MIR2_DBSEL_PROBE_EVERY";
/// 单条 `total_ms` 达到该值必打（缺省 1000；非法/0 ⇒ 1000）。
pub const ENV_SLOW_MS: &str = "MIR2_DBSEL_PROBE_SLOW_MS";

/// 缺省的每 N 条采样。
pub const DEFAULT_EVERY: u64 = 50;
/// 缺省的慢请求阈值（毫秒）。
pub const DEFAULT_SLOW_MS: u64 = 1000;

static ENABLED: OnceLock<bool> = OnceLock::new();
static EVERY: OnceLock<u64> = OnceLock::new();
static SLOW_MS: OnceLock<u64> = OnceLock::new();
static COUNT: AtomicU64 = AtomicU64::new(0);

/// 测试专用覆盖：`0` = 未设置（走 env），`1` = 强制开，`2` = 强制关。
#[cfg(test)]
static TEST_GATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// 三个时序分量 + 总时长（毫秒）。
///
/// 字段名直接带单位（`_ms`），与 `[DBSEL]` 行一一对应；clippy 的 `struct_field_names`
/// 只看后缀、不理解"单位进名字"的用意，这里显式放行。
#[allow(clippy::struct_field_names)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Acc {
    /// 分支入口 → 调用 `select_chr` 之前（前置判断 + 调度排队；**已扣除** `lock_ms`）。
    pub wait_ms: u64,
    /// `server.login.lock().await` 取锁 + `check_session` 线性扫。
    pub lock_ms: u64,
    /// 独立采样的一次 `pool.acquire()` 等待（池压力读数）。
    pub pool_ms: u64,
    /// 本次请求取连接（`Db::acquire()`）的耗时。
    pub acq_ms: u64,
    /// 本次请求的 acquire 次数（复用一条连接后恒 1；旧实现=SQL 条数）。
    pub acq_n: u64,
    /// 本次请求的 SQL 语句数（`Db::round_trips()` 的差值）。
    pub sql_n: u64,
    /// 本次请求 DB 层累计：**仅 execute**（不含 acquire）。
    pub db_ms: u64,
    /// 入口 → 回包前。
    pub total_ms: u64,
}

impl Acc {
    /// `rest = total − wait − lock − db`（饱和减法，避免造负数）。
    #[must_use]
    pub fn rest_ms(&self) -> u64 {
        self.total_ms
            .saturating_sub(self.wait_ms)
            .saturating_sub(self.lock_ms)
            .saturating_sub(self.acq_ms)
            .saturating_sub(self.db_ms)
    }
}

/// 探针是否开启（首次读 env 后缓存）。
#[must_use]
pub fn enabled() -> bool {
    #[cfg(test)]
    {
        match TEST_GATE.load(Ordering::Relaxed) {
            1 => return true,
            2 => return false,
            _ => {}
        }
    }
    *ENABLED.get_or_init(|| std::env::var(ENV_ENABLE).is_ok_and(|v| v.trim() == "1"))
}

/// 每 N 条打一行（测试可注入）。
fn every() -> u64 {
    *EVERY.get_or_init(|| parse_positive(ENV_EVERY, DEFAULT_EVERY))
}

/// 慢请求阈值（测试可注入）。
fn slow_ms() -> u64 {
    *SLOW_MS.get_or_init(|| parse_positive(ENV_SLOW_MS, DEFAULT_SLOW_MS))
}

/// 正整数解析：缺失 / 非法 / 0 ⇒ 缺省。
fn parse_positive(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

/// 入口计时起点（关闭态不调用）。
#[must_use]
pub fn start() -> Instant {
    Instant::now()
}

/// 自起点以来的毫秒（饱和到 `u64`）。
#[must_use]
pub fn ms_since(t: Instant) -> u64 {
    #[allow(clippy::cast_possible_truncation)]
    let ms = t.elapsed().as_millis() as u64;
    ms
}

/// 纯函数：拼一行（单测直接断言字段齐全）。`chr` 已经过 `decode`。
#[must_use]
pub fn line(n: u64, acc: &Acc, chr: &str, ok: bool) -> String {
    format!(
        "[DBSEL] n={n} wait_ms={} lock_ms={} pool_ms={} acq_ms={} acq_n={} sql_n={} db_ms={} rest_ms={} total_ms={} chr={chr} ok={}",
        acc.wait_ms,
        acc.lock_ms,
        acc.pool_ms,
        acc.acq_ms,
        acc.acq_n,
        acc.sql_n,
        acc.db_ms,
        acc.rest_ms(),
        acc.total_ms,
        u8::from(ok)
    )
}

/// 记一条：计数器 +1，按频率/慢阈值决定是否打印。**关闭态直接返回**（不计数、不取时间）。
pub fn note(acc: &Acc, chr: &str, ok: bool) {
    if !enabled() {
        return;
    }
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if !n.is_multiple_of(every()) && acc.total_ms < slow_ms() {
        return;
    }
    crate::log::info(&line(n, acc, chr, ok));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 两个用例共享进程级 `TEST_GATE`/`COUNT`：并行跑会互相插队（首跑红：
    /// gate_off 读到 gate_on 的 +2）。用同一把锁串行化（测试专用，不改生产路径）。
    static GATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn set_gate(v: u8) {
        TEST_GATE.store(v, Ordering::Relaxed);
    }

    #[test]
    fn gate_off_is_silent_and_does_not_count() {
        let _guard = GATE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_gate(2);
        let before = COUNT.load(Ordering::Relaxed);
        note(&Acc::default(), "hero", true);
        assert_eq!(COUNT.load(Ordering::Relaxed), before, "门关态不得计数");
        assert!(!enabled());
    }

    #[test]
    fn gate_on_counts_and_n_increments() {
        let _guard = GATE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_gate(1);
        let before = COUNT.load(Ordering::Relaxed);
        note(&Acc::default(), "hero", true);
        note(&Acc::default(), "hero", true);
        assert_eq!(COUNT.load(Ordering::Relaxed), before + 2, "门开态每条 +1");
        assert!(enabled());
        set_gate(0);
    }

    #[test]
    fn line_has_all_fields_and_rest_is_saturated() {
        let acc = Acc {
            wait_ms: 7,
            lock_ms: 3,
            pool_ms: 11,
            acq_ms: 2,
            acq_n: 1,
            sql_n: 4,
            db_ms: 19,
            total_ms: 100,
        };
        let s = line(42, &acc, "hero", true);
        for k in [
            "n=42",
            "wait_ms=7",
            "lock_ms=3",
            "pool_ms=11",
            "acq_ms=2",
            "acq_n=1",
            "sql_n=4",
            "db_ms=19",
            "rest_ms=69",
            "total_ms=100",
            "chr=hero",
            "ok=1",
        ] {
            assert!(s.contains(k), "缺字段 {k}: {s}");
        }
        assert!(s.starts_with("[DBSEL] "), "行必须以 [DBSEL] 开头: {s}");
        // 负例：分量之和超过 total ⇒ rest 饱和为 0，不得下溢
        let over = Acc {
            wait_ms: 50,
            lock_ms: 50,
            pool_ms: 1,
            acq_ms: 50,
            acq_n: 1,
            sql_n: 4,
            db_ms: 50,
            total_ms: 100,
        };
        assert_eq!(over.rest_ms(), 0, "rest_ms 必须饱和，不得下溢");
    }

    #[test]
    fn parse_positive_falls_back_on_garbage_and_zero() {
        // 只测纯函数行为（不写 env，避免影响其它用例）
        assert_eq!(parse_positive("MIR2_DBSEL_PROBE__NO_SUCH_KEY", 50), 50);
    }
}
