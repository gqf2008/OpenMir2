//! **登录入口探针**（`MIR2_DBSEL_ING=1` 门控，关闭态零成本、零语义）。
//!
//! 两层：
//! 1. 连接层：accept → 首个完整帧、入队→派发的等待、四类丢弃计数；
//! 2. 派发循环层：逐条 `service_ms`（含 gates 锁等待/解析/DB 派发）、`idle_ms`、
//!    按 ident 的条数与服务时间分位（近似直方）、`sum_service / wall`、`in_flight`。
//!
//! 只观察：不改超时、不改限速、不改协议、不改任何分支条件。实现约定：
//! - `enabled()` 读一次环境变量（`OnceLock`），关闭时所有钩子第一行就 return；
//! - 队列是**单消费者**，所以"当前条目"是一份全局状态即可（`CURRENT`）；
//! - 计数是进程级 `AtomicU64`，日志采样（每 `EVERY` 条或慢条目）。
//!
//! 输出（`log::info`，与 DBSrv 既有日志同通道）：
//! ```text
//! [DBSEL_ING] conn=1 accept_ms=12 first_frame_ms=37 bytes=204
//! [DBSEL_ING] conn=1 dispatch queue_ms=3 depth=0 ident=103 seq=100
//! [DBSEL_ING] by_ident ident=100 count=57 p50_ms≈12 p90_ms≈31 max_ms=88 sum_ms=940 dropped_no_user=0
//! [DBSEL_ING] loop sum_service_ms=1203 wall_ms=2410 ratio_permille=499 idle_ms=1207 in_flight_max=1
//! [DBSEL_ING] totals accepted_total=1 frames_total=4 dispatched_total=2 dropped_no_gate=0 dropped_no_user=0 dropped_datalen=0 dropped_decode=0
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::log;

const EVERY: u64 = 100;
const SLOW_MS: u128 = 200;

/// 直方图桶上界（ms）；最后一桶 = 溢出。
const BUCKETS: [u64; 19] = [
    1,
    2,
    3,
    5,
    8,
    12,
    20,
    30,
    50,
    80,
    120,
    200,
    500,
    1_000,
    2_000,
    5_000,
    10_000,
    30_000,
    u64::MAX,
];
const NB: usize = BUCKETS.len();
/// 跟踪的 ident：100 QUERYCHR / 101 NEWCHR / 102 DELCHR / 103 SELCHR / 其它。
const TRACKED: [u16; 5] = [100, 101, 102, 103, 0];

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

/// 单消费者循环：逐条服务时间累计、idle 累计、在飞峰值。
static SUM_SERVICE_MS: AtomicU64 = AtomicU64::new(0);
static IDLE_MS: AtomicU64 = AtomicU64::new(0);
static IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
static IN_FLIGHT_MAX: AtomicU64 = AtomicU64::new(0);

struct Conn {
    gate_id: i32,
    accept_at: Instant,
    first_frame_at: Option<Instant>,
    bytes: u64,
}

struct ItemState {
    start: Instant,
    ident: Option<u16>,
    gates_lock_ms: u64,
    pre_dispatch_ms: u64,
}

static CONNS: OnceLock<Mutex<HashMap<usize, Conn>>> = OnceLock::new();
static CURRENT: Mutex<Option<ItemState>> = Mutex::new(None);
static LAST_END: Mutex<Option<Instant>> = Mutex::new(None);

struct IdentStat {
    count: AtomicU64,
    sum_ms: AtomicU64,
    max_ms: AtomicU64,
    dropped_no_user: AtomicU64,
    hist: [AtomicU64; NB],
}

impl IdentStat {
    fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
            sum_ms: AtomicU64::new(0),
            max_ms: AtomicU64::new(0),
            dropped_no_user: AtomicU64::new(0),
            hist: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

static STATS: OnceLock<[IdentStat; 5]> = OnceLock::new();

fn stats() -> &'static [IdentStat; 5] {
    STATS.get_or_init(|| std::array::from_fn(|_| IdentStat::new()))
}

fn slot_of(ident: u16) -> usize {
    TRACKED
        .iter()
        .position(|&t| t == ident)
        .unwrap_or(TRACKED.len() - 1)
}

fn conns() -> &'static Mutex<HashMap<usize, Conn>> {
    CONNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn start() -> &'static Instant {
    START.get_or_init(Instant::now)
}

fn ms_since_start(t: Instant) -> u128 {
    t.saturating_duration_since(*start()).as_millis()
}

fn bucket_of(ms: u64) -> usize {
    BUCKETS.iter().position(|&b| ms <= b).unwrap_or(NB - 1)
}

fn percentile(hist: &[AtomicU64; NB], permille: u64) -> u64 {
    let total: u64 = hist.iter().map(|c| c.load(Ordering::Relaxed)).sum();
    if total == 0 {
        return 0;
    }
    let want = (total * permille).div_ceil(1000).max(1);
    let mut seen = 0u64;
    for (i, c) in hist.iter().enumerate() {
        seen = seen.saturating_add(c.load(Ordering::Relaxed));
        if seen >= want {
            return BUCKETS[i];
        }
    }
    BUCKETS[NB - 1]
}

/// 探针是否打开（进程内只读一次 `MIR2_DBSEL_ING`）。
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var("MIR2_DBSEL_ING").is_ok_and(|v| v == "1"))
}

/// 首帧完整日志行（accept → 首个完整帧）。
#[must_use]
pub fn first_frame_line(conn: i32, accept_ms: u128, first_frame_ms: u128, bytes: u64) -> String {
    format!(
        "[DBSEL_ING] conn={conn} accept_ms={accept_ms} first_frame_ms={first_frame_ms} bytes={bytes}"
    )
}

/// 派发日志行（解码完成 → 进入命令派发前）。
#[must_use]
pub fn dispatch_line(conn: i32, queue_ms: u128, depth: usize, ident: u16, seq: u64) -> String {
    format!(
        "[DBSEL_ING] conn={conn} dispatch queue_ms={queue_ms} depth={depth} ident={ident} seq={seq}"
    )
}

/// 按 ident 的服务时间摘要（分位为桶上界，带 ≈）。
#[must_use]
pub fn by_ident_line(
    ident: u16,
    count: u64,
    p50: u64,
    p90: u64,
    max: u64,
    sum_ms: u64,
    dropped_no_user: u64,
) -> String {
    let name = if ident == 0 {
        "other".to_string()
    } else {
        ident.to_string()
    };
    format!(
        "[DBSEL_ING] by_ident ident={name} count={count} p50_ms≈{p50} p90_ms≈{p90} max_ms={max} sum_ms={sum_ms} dropped_no_user={dropped_no_user}"
    )
}

/// 循环层摘要：`sum_service / wall` 与 idle。
#[must_use]
pub fn loop_line(sum_service_ms: u64, wall_ms: u64, idle_ms: u64, in_flight_max: u64) -> String {
    // 不变式：墙钟不可能短于累计服务时间（`Instant` 边界/取整会让它差几毫秒）；
    // 这里把 `wall` 夹到 `sum` 之上，保证 `ratio_permille ∈ [0, 1000]`。
    let wall = wall_ms.max(sum_service_ms);
    let ratio = if wall == 0 {
        0
    } else {
        sum_service_ms.saturating_mul(1000) / wall
    };
    format!(
        "[DBSEL_ING] loop sum_service_ms={sum_service_ms} wall_ms={wall} ratio_permille={ratio} idle_ms={idle_ms} in_flight_max={in_flight_max}"
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

fn emit_totals() {
    log::info(&totals_line(
        ACCEPTED.load(Ordering::Relaxed),
        FRAMES.load(Ordering::Relaxed),
        DISPATCHED.load(Ordering::Relaxed),
        DROPPED_NO_GATE.load(Ordering::Relaxed),
        DROPPED_NO_USER.load(Ordering::Relaxed),
        DROPPED_DATALEN.load(Ordering::Relaxed),
        DROPPED_DECODE.load(Ordering::Relaxed),
    ));
    for (i, &ident) in TRACKED.iter().enumerate() {
        let st = &stats()[i];
        let count = st.count.load(Ordering::Relaxed);
        if count == 0 && st.dropped_no_user.load(Ordering::Relaxed) == 0 {
            continue;
        }
        log::info(&by_ident_line(
            ident,
            count,
            percentile(&st.hist, 500),
            percentile(&st.hist, 900),
            st.max_ms.load(Ordering::Relaxed),
            st.sum_ms.load(Ordering::Relaxed),
            st.dropped_no_user.load(Ordering::Relaxed),
        ));
    }
    log::info(&loop_line(
        SUM_SERVICE_MS.load(Ordering::Relaxed),
        start().elapsed().as_millis() as u64,
        IDLE_MS.load(Ordering::Relaxed),
        IN_FLIGHT_MAX.load(Ordering::Relaxed),
    ));
}

fn maybe_totals() {
    let seq = EVENTS.fetch_add(1, Ordering::Relaxed) + 1;
    if seq.is_multiple_of(EVERY) {
        emit_totals();
    }
}

/// 新连接接入（槽位登记之后）。
pub fn on_accept(slot: usize, gate_id: i32) {
    if !enabled() {
        return;
    }
    ACCEPTED.fetch_add(1, Ordering::Relaxed);
    let now = Instant::now();
    if let Ok(mut m) = conns().lock() {
        m.insert(
            slot,
            Conn {
                gate_id,
                accept_at: now,
                first_frame_at: None,
                bytes: 0,
            },
        );
    }
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
    let line = {
        let Ok(mut m) = conns().lock() else {
            return;
        };
        let Some(c) = m.get_mut(&slot) else {
            return;
        };
        if c.first_frame_at.is_some() {
            None
        } else {
            let now = Instant::now();
            c.first_frame_at = Some(now);
            Some(first_frame_line(
                c.gate_id,
                ms_since_start(c.accept_at),
                ms_since_start(now),
                c.bytes,
            ))
        }
    };
    if let Some(line) = line {
        log::info(&line);
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

/// 队列出队：开始计时一条（含 idle 统计）。
pub fn begin_item() {
    if !enabled() {
        return;
    }
    let now = Instant::now();
    if let Ok(mut last) = LAST_END.lock() {
        if let Some(prev) = last.take() {
            let idle = now.saturating_duration_since(prev).as_millis() as u64;
            IDLE_MS.fetch_add(idle, Ordering::Relaxed);
        }
    }
    let n = IN_FLIGHT.fetch_add(1, Ordering::Relaxed) + 1;
    IN_FLIGHT_MAX.fetch_max(n, Ordering::Relaxed);
    if let Ok(mut cur) = CURRENT.lock() {
        *cur = Some(ItemState {
            start: now,
            ident: None,
            gates_lock_ms: 0,
            pre_dispatch_ms: 0,
        });
    }
}

/// gates 锁等待时长（`process_data` 取会话表那一次）。
pub fn note_gates_lock(ms: u64) {
    if !enabled() {
        return;
    }
    if let Ok(mut cur) = CURRENT.lock() {
        if let Some(c) = cur.as_mut() {
            c.gates_lock_ms = c.gates_lock_ms.saturating_add(ms);
        }
    }
}

/// 解码出 ident 时登记（供按 ident 统计；同时记"到派发前"的耗时）。
pub fn mark_ident(ident: u16) {
    if !enabled() {
        return;
    }
    if let Ok(mut cur) = CURRENT.lock() {
        if let Some(c) = cur.as_mut() {
            c.ident = Some(ident);
            c.pre_dispatch_ms = c.start.elapsed().as_millis() as u64;
        }
    }
}

/// 慢条目行（`service_ms ≥ SLOW_MS`）：把 service 拆成 gates 锁等待与到派发前的耗时。
#[must_use]
pub fn slow_line(ident: Option<u16>, service_ms: u64, gates_lock_ms: u64, pre_dispatch_ms: u64) -> String {
    let ident = ident.map_or_else(|| "other".to_string(), |i| i.to_string());
    format!(
        "[DBSEL_ING] slow ident={ident} service_ms={service_ms} gates_lock_ms={gates_lock_ms} pre_dispatch_ms={pre_dispatch_ms}"
    )
}

/// 一条处理结束（无论成功/失败/丢弃），结算 service_ms 与按 ident 统计。
pub fn end_item() {
    if !enabled() {
        return;
    }
    let taken = CURRENT.lock().ok().and_then(|mut c| c.take());
    let Some(ItemState {
        start: t0,
        ident,
        gates_lock_ms,
        pre_dispatch_ms,
    }) = taken
    else {
        return;
    };
    IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
    let service = t0.elapsed().as_millis() as u64;
    if u128::from(service) >= SLOW_MS {
        log::info(&slow_line(ident, service, gates_lock_ms, pre_dispatch_ms));
    }
    SUM_SERVICE_MS.fetch_add(service, Ordering::Relaxed);
    let slot = slot_of(ident.unwrap_or(0));
    let st = &stats()[slot];
    st.count.fetch_add(1, Ordering::Relaxed);
    st.sum_ms.fetch_add(service, Ordering::Relaxed);
    st.max_ms.fetch_max(service, Ordering::Relaxed);
    st.hist[bucket_of(service)].fetch_add(1, Ordering::Relaxed);
    if let Ok(mut last) = LAST_END.lock() {
        *last = Some(Instant::now());
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

/// `NoUser` 丢弃时按**尽力解析出的 ident** 分桶（帧可能跨包，解析不出就只记总数）。
pub fn on_drop_no_user(data: &[u8]) {
    if !enabled() {
        return;
    }
    DROPPED_NO_USER.fetch_add(1, Ordering::Relaxed);
    if let Some(ident) = hint_ident(data) {
        stats()[slot_of(ident)]
            .dropped_no_user
            .fetch_add(1, Ordering::Relaxed);
    }
    maybe_totals();
}

/// 尽力从原始 Data 里取一个完整 `#…!` 帧的 ident（只读；失败返回 `None`）。
fn hint_ident(data: &[u8]) -> Option<u16> {
    let start = data.iter().position(|b| *b == b'#')?;
    let end = data[start..].iter().position(|b| *b == b'!')? + start;
    let frame = &data[start..=end];
    crate::user::decode_client_packet(&frame[1..])
        .ok()
        .map(|p| p.head.ident)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_carry_the_fields_the_report_needs() {
        assert_eq!(
            first_frame_line(7, 12, 37, 204),
            "[DBSEL_ING] conn=7 accept_ms=12 first_frame_ms=37 bytes=204"
        );
        assert_eq!(
            dispatch_line(7, 3, 0, 103, 100),
            "[DBSEL_ING] conn=7 dispatch queue_ms=3 depth=0 ident=103 seq=100"
        );
        let b = by_ident_line(100, 57, 12, 31, 88, 940, 0);
        assert!(b.contains("ident=100"));
        assert!(b.contains("count=57"));
        assert!(b.contains("p90_ms≈31"));
        let l = loop_line(1203, 2410, 1207, 1);
        assert!(l.contains("ratio_permille=499"), "{l}");
        // 牙：`wall >= sum_service` 且 `ratio_permille ∈ [0,1000]`（含时钟边界导致的
        // `sum > wall` 输入）。
        for (sum, wall) in [(1203u64, 2410u64), (1203, 500), (0, 0), (5, 5)] {
            let line = loop_line(sum, wall, 0, 1);
            let get = |k: &str| -> u64 {
                line.split_whitespace()
                    .find_map(|f| f.strip_prefix(k).and_then(|v| v.parse().ok()))
                    .unwrap_or(u64::MAX)
            };
            let w = get("wall_ms=");
            let r = get("ratio_permille=");
            assert!(w >= sum, "wall>=sum: {line}");
            assert!(r <= 1000, "ratio<=1000: {line}");
        }
        let t = totals_line(1, 4, 2, 0, 0, 0, 0);
        assert!(t.contains("accepted_total=1"));
        assert!(t.contains("dispatched_total=2"));
    }

    #[test]
    fn buckets_and_percentiles_are_monotone() {
        let hist: [AtomicU64; NB] = std::array::from_fn(|_| AtomicU64::new(0));
        hist[bucket_of(1)].fetch_add(5, Ordering::Relaxed);
        hist[bucket_of(30)].fetch_add(5, Ordering::Relaxed);
        assert!(percentile(&hist, 500) <= percentile(&hist, 900));
    }

    #[test]
    fn disabled_probe_allocates_nothing() {
        if std::env::var_os("MIR2_DBSEL_ING").is_some() {
            return;
        }
        assert!(!enabled());
        assert!(enqueued().is_none());
        on_accept(0, 1);
        on_data(0, 10);
        on_frame(0);
        on_dispatch(0, 1, 0, 103);
        begin_item();
        note_gates_lock(1);
        mark_ident(103);
        end_item();
        on_drop(Drop::NoUser);
        on_drop_no_user(b"#x!");
        on_close(0);
    }
}
