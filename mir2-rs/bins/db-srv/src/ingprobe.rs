//! **登录入口探针**（`MIR2_DBSEL_ING=1` 门控，关闭态零成本、零语义）。
//!
//! 回答 a7/W-M 留下的问题：`handle_selchr` 之前的「accept → 首帧完整 → 派发」链路上，
//! 请求有没有被及时读到、在队列里等了多久、以及 1,182 个失败请求是否到达过 DBSrv。
//!
//! 只观察：不改超时、不改限速、不改协议、不改任何分支条件。实现约定：
//! - `enabled()` 读一次环境变量（`OnceLock`），关闭时所有钩子第一行就 return；
//! - 每条连接的状态只放进程内 `HashMap<slot, Conn>`，随连接关闭移除；
//! - 计数是进程级 `AtomicU64`，日志是采样（每 `EVERY` 条或 `queue_ms > SLOW_MS`）。
//!
//! 输出格式（`log::info`，与 DBSrv 既有日志同通道）：
//! ```text
//! [DBSEL_ING] conn=1 accept_ms=12 first_frame_ms=37 bytes=204
//! [DBSEL_ING] conn=1 dispatch queue_ms=3 depth=0 ident=1005 seq=100
//! [DBSEL_ING] totals accepted_total=1 frames_total=4 dispatched_total=2 dropped_no_gate=0 dropped_no_user=0 dropped_datalen=0 dropped_decode=0
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::log;

const EVERY: u64 = 100;
const SLOW_MS: u128 = 200;

static ENABLED: OnceLock<bool> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

static ACCEPTED: AtomicU64 = AtomicU64::new(0);
static FRAMES: AtomicU64 = AtomicU64::new(0);
static DISPATCHED: AtomicU64 = AtomicU64::new(0);
static DROPPED_NO_GATE: AtomicU64 = AtomicU64::new(0);
static DROPPED_NO_USER: AtomicU64 = AtomicU64::new(0);
static DROPPED_DATALEN: AtomicU64 = AtomicU64::new(0);
static DROPPED_DECODE: AtomicU64 = AtomicU64::new(0);
static EVENTS: AtomicU64 = AtomicU64::new(0);

struct Conn {
    gate_id: i32,
    accept_at: Instant,
    first_frame_at: Option<Instant>,
    bytes: u64,
}

static CONNS: OnceLock<Mutex<HashMap<usize, Conn>>> = OnceLock::new();

/// 探针是否打开（进程内只读一次 `MIR2_DBSEL_ING`）。
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var("MIR2_DBSEL_ING").is_ok_and(|v| v == "1"))
}

fn start() -> &'static Instant {
    START.get_or_init(Instant::now)
}

fn conns() -> &'static Mutex<HashMap<usize, Conn>> {
    CONNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ms_since_start(t: Instant) -> u128 {
    t.saturating_duration_since(*start()).as_millis()
}

/// 首帧完整日志行（accept → 首个完整帧）。
#[must_use]
pub fn first_frame_line(conn: i32, accept_ms: u128, first_frame_ms: u128, bytes: u64) -> String {
    format!(
        "[DBSEL_ING] conn={conn} accept_ms={accept_ms} first_frame_ms={first_frame_ms} bytes={bytes}"
    )
}

/// 派发日志行（解析完成 → 进入命令派发前）。
#[must_use]
pub fn dispatch_line(conn: i32, queue_ms: u128, depth: usize, ident: u16, seq: u64) -> String {
    format!(
        "[DBSEL_ING] conn={conn} dispatch queue_ms={queue_ms} depth={depth} ident={ident} seq={seq}"
    )
}

/// 汇总行（计数器原文，供"失败请求是否到达过 DBSrv"对账）。
#[must_use]
pub fn totals_line(
    accepted: u64,
    frames: u64,
    dispatched: u64,
    dropped_no_gate: u64,
    dropped_no_user: u64,
    dropped_datalen: u64,
    dropped_decode: u64,
) -> String {
    format!(
        "[DBSEL_ING] totals accepted_total={accepted} frames_total={frames} dispatched_total={dispatched} dropped_no_gate={dropped_no_gate} dropped_no_user={dropped_no_user} dropped_datalen={dropped_datalen} dropped_decode={dropped_decode}"
    )
}

fn maybe_totals() {
    let seq = EVENTS.fetch_add(1, Ordering::Relaxed) + 1;
    if seq.is_multiple_of(EVERY) {
        log::info(&totals_line(
            ACCEPTED.load(Ordering::Relaxed),
            FRAMES.load(Ordering::Relaxed),
            DISPATCHED.load(Ordering::Relaxed),
            DROPPED_NO_GATE.load(Ordering::Relaxed),
            DROPPED_NO_USER.load(Ordering::Relaxed),
            DROPPED_DATALEN.load(Ordering::Relaxed),
            DROPPED_DECODE.load(Ordering::Relaxed),
        ));
    }
}

/// 新连接接入（槽位登记之后）。
pub fn on_accept(slot: usize, gate_id: i32) {
    if !enabled() {
        return;
    }
    ACCEPTED.fetch_add(1, Ordering::Relaxed);
    let now = Instant::now();
    conns().lock().ok().map(|mut m| {
        m.insert(
            slot,
            Conn {
                gate_id,
                accept_at: now,
                first_frame_at: None,
                bytes: 0,
            },
        );
    });
    maybe_totals();
}

/// 收到一条 `Data` 报文（进入队列之前）。
pub fn on_data(slot: usize, bytes: usize) {
    if !enabled() {
        return;
    }
    if let Ok(mut m) = conns().lock() {
        if let Some(c) = m.get_mut(&slot) {
            c.bytes = c.bytes.saturating_add(bytes as u64);
        }
    }
}

/// 队列入队时间戳（关闭态返回 `None`，调用方无需分支）。
#[must_use]
pub fn enqueued() -> Option<Instant> {
    enabled().then(Instant::now)
}

/// 一个完整帧解析完成（`process_data` 抠出 `#…!` 并解码成功之后）。
pub fn on_frame(slot: usize) {
    if !enabled() {
        return;
    }
    FRAMES.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut m) = conns().lock() {
        if let Some(c) = m.get_mut(&slot) {
            if c.first_frame_at.is_none() {
                let now = Instant::now();
                c.first_frame_at = Some(now);
                log::info(&first_frame_line(
                    c.gate_id,
                    ms_since_start(c.accept_at),
                    ms_since_start(now),
                    c.bytes,
                ));
            }
        }
    }
    maybe_totals();
}

/// 进入命令派发前（当前只在 `CM_SELCHR` 调用；`queue_ms` = 入队→派发的等待）。
pub fn on_dispatch(slot: usize, queue_ms: u128, depth: usize, ident: u16) {
    if !enabled() {
        return;
    }
    let n = DISPATCHED.fetch_add(1, Ordering::Relaxed) + 1;
    if n.is_multiple_of(EVERY) || queue_ms >= SLOW_MS {
        let gate_id = conns()
            .lock()
            .ok()
            .and_then(|m| m.get(&slot).map(|c| c.gate_id))
            .unwrap_or(-1);
        log::info(&dispatch_line(gate_id, queue_ms, depth, ident, n));
    }
    maybe_totals();
}

/// 连接关闭。
pub fn on_close(slot: usize) {
    if !enabled() {
        return;
    }
    if let Ok(mut m) = conns().lock() {
        m.remove(&slot);
    }
}

/// 丢弃分类（`process_data` 的四条早退/失败路）。
#[derive(Clone, Copy)]
pub enum Drop {
    NoGate,
    NoUser,
    DataLen,
    Decode,
}

pub fn on_drop(kind: Drop) {
    if !enabled() {
        return;
    }
    let c = match kind {
        Drop::NoGate => &DROPPED_NO_GATE,
        Drop::NoUser => &DROPPED_NO_USER,
        Drop::DataLen => &DROPPED_DATALEN,
        Drop::Decode => &DROPPED_DECODE,
    };
    c.fetch_add(1, Ordering::Relaxed);
    maybe_totals();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_carry_the_fields_the_report_needs() {
        let l = first_frame_line(7, 12, 37, 204);
        assert_eq!(
            l,
            "[DBSEL_ING] conn=7 accept_ms=12 first_frame_ms=37 bytes=204"
        );
        let d = dispatch_line(7, 3, 0, 103, 100);
        assert_eq!(
            d,
            "[DBSEL_ING] conn=7 dispatch queue_ms=3 depth=0 ident=103 seq=100"
        );
        let t = totals_line(1, 4, 2, 0, 0, 0, 0);
        assert!(t.contains("accepted_total=1"));
        assert!(t.contains("frames_total=4"));
        assert!(t.contains("dispatched_total=2"));
        assert!(t.contains("dropped_decode=0"));
    }

    #[test]
    fn disabled_probe_allocates_nothing() {
        // 测试进程不设 MIR2_DBSEL_ING ⇒ 门控为关，钩子必须是 no-op、不触连接表。
        if std::env::var_os("MIR2_DBSEL_ING").is_some() {
            return;
        }
        assert!(!enabled());
        assert!(enqueued().is_none());
        on_accept(0, 1);
        on_data(0, 10);
        on_frame(0);
        on_dispatch(0, 1, 0, 103);
        on_drop(Drop::NoUser);
        on_close(0);
    }
}
