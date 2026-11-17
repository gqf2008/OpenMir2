//! **会话拆除归因**（卡 `openmir2-session-close-attribution`）：把「谁先关」变成可数。
//!
//! 背景（W46 5000 档）：`failed=837`（`timeout 59` + `disconnect 778`）全部发生在进世界之前，
//! gate 的 `断开链接` 601/217/19 条与失败数逐数对齐，但现有日志判不出断开的**发起方**：
//! `断开链接` 两端都记、bot 侧 `decode_errors=0`、game-svr 踢人计数 0。本模块只做旁路观测：
//!
//! - **每会话一行** [`session_line`]（`SESSION_CLOSE ...`），在 `client_loop` 的**唯一退出出口**
//!   （与 `断开链接` 同一处、同一次调用）打印，供逐会话与 bot 侧失败记录对齐；
//! - **按原因聚合的终值计数** [`summary_fragment`]（拼进 `ENTRY_TELEMETRY` 汇总行），供机判；
//! - 恒等式：`Σ 各原因 == close_total`（两个量在同一次 [`record_close`] 里递增，不重复计）。
//!
//! **入口即唯一计数源**：`client_loop` 每个会话恰好退出一次，`record_close` 就在那条路径上
//! 紧随 `断开链接` 日志 ⇒ `close_total` 与 `断开链接` 行数同源（不是另起一套计数）。
//!
//! 原因枚举与 game-svr 侧（`game-svr/src/telemetry.rs` 的同名模块段）**字段名/取值一致**：
//! `client_fin / server_kick / tombstone / timeout / error(errno) / unknown`。
//! 未知原因一律进 `unknown`，**绝不**默认归入 `client_fin`。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};

/// 拆除原因（`repr(u8)` 的取值跨进程稳定；`unknown` = 255 便于日志侧识别）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CloseReason {
    /// 客户端 socket 读到 EOF（`Ok(0)`）——对端先发 FIN。
    ClientFin = 0,
    /// 本进程网关规则/登录解析主动关会话（`Step::Close`）。
    ServerKick = 1,
    /// 下行路由已被 game-svr 的 `GM_CLOSE` 摘除（墓碑化），客户端连接随后才退出。
    Tombstone = 2,
    /// 空闲清理（`TimedService.ClearIdleSession` 等价物）关掉了客户端 socket。
    Timeout = 3,
    /// 读写 socket 出错（桶里另记具体 errno）。
    Error = 4,
    /// 无法分类（**默认桶**；不得并入 `client_fin`）。
    Unknown = 255,
}

impl CloseReason {
    /// 日志/聚合键名（与 game-svr 侧一致）。
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ClientFin => "client_fin",
            Self::ServerKick => "server_kick",
            Self::Tombstone => "tombstone",
            Self::Timeout => "timeout",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }

    /// 稳定数值（`repr(u8)`）。
    #[must_use]
    pub fn code(self) -> u8 {
        self as u8
    }

    /// 数值 → 原因。**未知值落到 `unknown`**（删掉这条兜底 ⇒
    /// `unknown_code_never_folds_into_client_fin` 红）。
    #[must_use]
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Self::ClientFin,
            1 => Self::ServerKick,
            2 => Self::Tombstone,
            3 => Self::Timeout,
            4 => Self::Error,
            _ => Self::Unknown,
        }
    }

    fn slot(self) -> usize {
        match self {
            Self::ClientFin => 0,
            Self::ServerKick => 1,
            Self::Tombstone => 2,
            Self::Timeout => 3,
            Self::Error => 4,
            Self::Unknown => 5,
        }
    }
}

/// 拆除总数（与 `断开链接` 行数同源：每个 `client_loop` 退出恰好 +1）。
pub static CLOSE_TOTAL: AtomicU64 = AtomicU64::new(0);

/// 各原因计数（下标见 [`CloseReason::slot`]）。
static CLOSE_BY_REASON: [AtomicU64; 6] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// errno 桶：`raw_os_error` → 次数（只记 `error` 类）。
fn errno_map() -> &'static Mutex<BTreeMap<i32, u64>> {
    static MAP: OnceLock<Mutex<BTreeMap<i32, u64>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// 记一次拆除：总数与分类桶在同一次调用里递增（恒等式按构造成立）。
///
/// 返回新的总数。`errno` 只在 `reason == Error` 时进 errno 桶。
pub fn record_close(reason: CloseReason, errno: Option<i32>) -> u64 {
    let total = CLOSE_TOTAL.fetch_add(1, Relaxed) + 1;
    CLOSE_BY_REASON[reason.slot()].fetch_add(1, Relaxed);
    if reason == CloseReason::Error {
        if let Ok(mut m) = errno_map().lock() {
            *m.entry(errno.unwrap_or(0)).or_insert(0) += 1;
        }
    }
    total
}

/// 计数器快照（测试/对账用；生产日志走 [`summary_fragment`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseSnapshot {
    pub total: u64,
    /// 顺序同 [`CloseReason::ALL`]。
    pub by_reason: [u64; 6],
    pub errno: Vec<(i32, u64)>,
}

/// 读当前计数快照。
#[must_use]
pub fn snapshot() -> CloseSnapshot {
    let mut by_reason = [0u64; 6];
    for (i, c) in CLOSE_BY_REASON.iter().enumerate() {
        by_reason[i] = c.load(Relaxed);
    }
    let errno = errno_map()
        .lock()
        .map(|m| m.iter().map(|(k, v)| (*k, *v)).collect())
        .unwrap_or_default();
    CloseSnapshot {
        total: CLOSE_TOTAL.load(Relaxed),
        by_reason,
        errno,
    }
}

impl CloseSnapshot {
    /// 各原因之和（恒等式牙用；生产读汇总片段即可）。
    #[cfg(test)]
    #[must_use]
    pub fn sum_reasons(&self) -> u64 {
        self.by_reason.iter().sum()
    }

    /// 某原因当前计数。
    #[must_use]
    pub fn count(&self, reason: CloseReason) -> u64 {
        self.by_reason[reason.slot()]
    }
}

/// 汇总片段（拼进 `ENTRY_TELEMETRY` 汇总行；字段名固定供机判）。
#[must_use]
pub fn summary_fragment() -> String {
    let s = snapshot();
    let errno = s
        .errno
        .iter()
        .map(|(code, n)| format!("{code}:{n}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "close_total={} close_breakdown[client_fin={} server_kick={} tombstone={} timeout={} \
         error={} unknown={}] close_errno=[{errno}]",
        s.total,
        s.count(CloseReason::ClientFin),
        s.count(CloseReason::ServerKick),
        s.count(CloseReason::Tombstone),
        s.count(CloseReason::Timeout),
        s.count(CloseReason::Error),
        s.count(CloseReason::Unknown),
    )
}

/// 每会话拆除行（与 `断开链接` 同一出口打印，保证一行一会话）。
#[must_use]
pub fn session_line(
    ts_ms: i64,
    gate_line: u8,
    socket: i32,
    sid: u16,
    reason: CloseReason,
    errno: Option<i32>,
) -> String {
    let errno = errno.map_or_else(|| "-".to_string(), |e| e.to_string());
    format!(
        "SESSION_CLOSE ts_ms={ts_ms} svc=game-gate gate_line={gate_line} socket={socket} sid={sid} \
         cause={} errno={errno}",
        reason.name()
    )
}

/// 进程级计数器测试串行锁（统计量是全局的，测试必须串行取快照差值）。
#[cfg(test)]
pub static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests {
    use super::*;

    /// 五类各构造一次 ⇒ 五类各 +1、合计 == 总数（差值口径，不受其它用例影响）。
    ///
    /// 变异：把 `record_close` 里 `reason.slot()` 写死成某桶 ⇒ 分类计数与合计对不上 ⇒ 红。
    #[tokio::test]
    async fn five_reasons_each_count_once_and_sum_to_total() {
        let _g = TEST_LOCK.lock().await;
        let before = snapshot();
        for reason in [
            CloseReason::ClientFin,
            CloseReason::ServerKick,
            CloseReason::Tombstone,
            CloseReason::Timeout,
            CloseReason::Error,
        ] {
            record_close(reason, None);
        }
        let after = snapshot();
        assert_eq!(after.total - before.total, 5, "五次记录，总数必须恰 +5");
        for reason in [
            CloseReason::ClientFin,
            CloseReason::ServerKick,
            CloseReason::Tombstone,
            CloseReason::Timeout,
            CloseReason::Error,
        ] {
            assert_eq!(
                after.count(reason) - before.count(reason),
                1,
                "{} 必须恰 +1",
                reason.name()
            );
        }
        assert_eq!(
            after.sum_reasons() - before.sum_reasons(),
            after.total - before.total,
            "Σ各原因 != 拆除总数（恒等式破坏）"
        );
        assert_eq!(
            after.count(CloseReason::Unknown),
            before.count(CloseReason::Unknown),
            "五类构造不得进 unknown 桶"
        );
    }

    /// 未知数值必须落 `unknown`，**不得**默认进 `client_fin`。
    ///
    /// 变异：把 `from_code` 的 `_ => Self::Unknown` 改成 `_ => Self::ClientFin` ⇒ 本用例红。
    #[test]
    fn unknown_code_never_folds_into_client_fin() {
        assert_eq!(CloseReason::from_code(200), CloseReason::Unknown);
        assert_eq!(CloseReason::from_code(255), CloseReason::Unknown);
        assert_ne!(CloseReason::from_code(200), CloseReason::ClientFin);
        for (code, want) in [
            (0, CloseReason::ClientFin),
            (1, CloseReason::ServerKick),
            (2, CloseReason::Tombstone),
            (3, CloseReason::Timeout),
            (4, CloseReason::Error),
        ] {
            assert_eq!(CloseReason::from_code(code), want);
            assert_eq!(want.code(), code, "code 必须与 repr 一致");
        }
    }

    /// errno 桶必须带具体数值；非 error 类不进 errno 桶。
    ///
    /// 变异：`record_close` 里删掉 `errno` 入表 ⇒ 本用例红。
    #[tokio::test]
    async fn errno_bucket_carries_value() {
        let _g = TEST_LOCK.lock().await;
        let before = snapshot();
        record_close(CloseReason::Error, Some(10054));
        let after = snapshot();
        let got = |s: &CloseSnapshot, code: i32| {
            s.errno
                .iter()
                .find(|(c, _)| *c == code)
                .map_or(0, |(_, n)| *n)
        };
        assert_eq!(got(&after, 10054) - got(&before, 10054), 1);
        assert_ne!(got(&after, 10054), 0, "errno 桶必须是非 0 值");
        assert!(
            summary_fragment().contains("10054:"),
            "汇总行必须带 errno 数值: {}",
            summary_fragment()
        );
    }

    /// 汇总片段必须写全所有字段（缺字段 = 读数时看不出是 0 还是没接）。
    #[test]
    fn summary_fragment_carries_every_field() {
        let line = summary_fragment();
        for key in [
            "close_total=",
            "close_breakdown[client_fin=",
            "server_kick=",
            "tombstone=",
            "timeout=",
            "error=",
            "unknown=",
            "close_errno=[",
        ] {
            assert!(line.contains(key), "汇总片段缺字段 {key}: {line}");
        }
    }

    /// 每会话行字段固定（gate 线号/socket/sid/原因/errno）。
    #[test]
    fn session_line_carries_identity_and_reason() {
        let line = session_line(
            1_700_000_000_123,
            2,
            4242,
            777,
            CloseReason::Error,
            Some(10054),
        );
        for key in [
            "SESSION_CLOSE ",
            "ts_ms=1700000000123",
            "svc=game-gate",
            "gate_line=2",
            "socket=4242",
            "sid=777",
            "cause=error",
            "errno=10054",
        ] {
            assert!(line.contains(key), "拆除行缺字段 {key}: {line}");
        }
        let no_errno = session_line(1, 0, 1, 2, CloseReason::ClientFin, None);
        assert!(no_errno.contains("errno=-"), "{no_errno}");
    }
}
