//! **入口三段计数**（卡 `openmir2-game-svr-entry-triple`）。
//!
//! 背景（2026-11-17 仪表态 1000 档）：DBSrv 侧 `525_sent=1000 525_fail=0` 已把入口第一段闭合，
//! 客户端 `in_world 975/1000`，剩下 **25** 个卡在「525 之后、进世界之前」。这一段服务端此前
//! **没有任何观测**（`658_sent = enter_ok` 的理由不成立：658 只说明发了 `SM_SENDNOTICE`）。
//!
//! 本模块只做**旁路计数**，三个原子量与三个调用点一一对应：
//!
//! | 计数 | 调用点（唯一） | 语义 |
//! | --- | --- | --- |
//! | [`LOGON_FRAMES_SEEN`] | `GateEvent::LoadDbRequest` 分支 | 网关侧认证通过、取角色进世界的登录事件到达 |
//! | [`ENTER_ATTEMPT`] | `DBR_LOADHUMANRCD(1100) recog==1` 回包分支入口 | 拿到角色数据、开始进世界流程 |
//! | [`ENTER_OK`] | `world.enter_map_at(..)` 返回 `Ok(id)` 分支 | 真正进入世界（会话 + 实体落图） |
//! | [`LOAD_REPLY_OTHER`] | `ident == DBR_LOADHUMANRCD && recog != 1` 的 else 分支 | 取档应答到了但**不是成功码**（修复前该分支完全静默） |
//!
//! 相邻两两之差即缺口：
//! - `logon_frames_seen - enter_attempt - load_reply_other`：**连应答都没等到**（请求没到 DBSvr /
//!   应答在单连接上丢或错配 / DBSvr 侧静默丢弃）——这是卡 `openmir2-entry-load-reply-buckets` 要分开的那一段；
//! - `enter_attempt - enter_ok`：开始进世界但失败（解析失败/准入拒绝/落图失败）。
//!
//! `load_reply_other` 另采样前 [`SAMPLE_LIMIT`] 条原样字段（`ENTRY_SAMPLE load_reply_recog_other …`）。
//!
//! **不改任何协议/回包/业务分支**；打印沿用 gate 的 `ENTRY_TELEMETRY` 单行风格，带 `svc=game-svr`
//! 便于与 gate 行区分。每 `PRINT_EVERY` 次事件打印一行（用 `fetch_add` 的返回值判档，
//! 不要打印时再读全局量——多线程下会漏档或重复打）。
//!
//! 计数语义的一个诚实边界：`RequeueSaving`（角色数据保存中）会把同一次登录**重新入队**，
//! 其后的新一次尝试会让 `enter_attempt` 超出 `logon_frames_seen`。所以
//! `logon ≥ attempt ≥ ok` 只在「无重排队」时成立；若读数违背它，先数日志里的
//! 「角色数据保存中,稍后重新读取」。

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering::Relaxed};

/// 汇总打印的事件间隔（每 N 次事件打印一行；0 = 从不打印）。
const PRINT_EVERY: u64 = 100;

/// 网关侧登录事件到达数（`GateEvent::LoadDbRequest`）。
pub static LOGON_FRAMES_SEEN: AtomicU64 = AtomicU64::new(0);
/// 开始进世界流程的次数（`DBR_LOADHUMANRCD(1100) recog==1` 回包分支入口）。
pub static ENTER_ATTEMPT: AtomicU64 = AtomicU64::new(0);
/// 真正进入世界的次数（`world.enter_map_at(..)` 返回 `Ok`）。
pub static ENTER_OK: AtomicU64 = AtomicU64::new(0);
/// `DBR_LOADHUMANRCD(1100)` 且 **`recog != 1`** 的取档应答数（卡 `openmir2-entry-load-reply-buckets`）。
///
/// 该分支**修复前完全静默**：应答被取走、不打日志、不计 [`ENTER_ATTEMPT`] ⇒
/// 「`logon_frames_seen - enter_attempt` 的缺口」到底是「没回包」还是「回了失败码」分不开。
pub static LOAD_REPLY_OTHER: AtomicU64 = AtomicU64::new(0);

/// **首个登录事件**的会话时钟（`world.clock_ms`）；只写一次（卡 `openmir2-entry-rate-instrument`）。
/// 未发生 = `-1`。速率 = (`last_enter_ms` − `first_enter_ms`) 与条数一起算，不再靠外部日志推。
static FIRST_ENTER_MS: AtomicI64 = AtomicI64::new(-1);
/// **末次进世界成功**的会话时钟；每次成功覆盖（只保留最后一条）。
static LAST_ENTER_MS: AtomicI64 = AtomicI64::new(-1);

/// 发往 6000 面的取档请求数（`req_tx.send` 成功 = 已排进 DB 链路；卡 `openmir2-entry-db-chain-counters`）。
///
/// 链路：`logon_frames_seen` → **`loaddb_request_sent`** → （DBSrv `load_request_seen`）→
/// **`loaddb_reply_seen`** → `enter_attempt` → `enter_ok`，每一段独立对账。
pub static LOADDB_REQUEST_SENT: AtomicU64 = AtomicU64::new(0);
/// 写 6000 面失败次数（`run_db_link` 的 `write_all/flush` 失败 ⇒ 已排队的请求丢了）。
pub static LOADDB_WRITE_ERROR: AtomicU64 = AtomicU64::new(0);
/// **收到 logon 但没发出取档请求**的四条分支（卡 `openmir2-entry-gap-buckets`）：
/// `logon_no_request_disconnected`（DB 链路未连）/ `_send_fail`（req_tx.send 失败）/
/// `_zero_session`（`SessionID == 0` 被读档循环跳过）/ `_requeue`（角色数据保存中 ⇒ 重排队）。
pub static LOGON_NO_REQUEST_DISCONNECTED: AtomicU64 = AtomicU64::new(0);
/// 见 [`LOGON_NO_REQUEST_DISCONNECTED`]：`req_tx.send` 失败。
pub static LOGON_NO_REQUEST_SEND_FAIL: AtomicU64 = AtomicU64::new(0);
/// 见 [`LOGON_NO_REQUEST_DISCONNECTED`]：`SessionID == 0` 跳过。
pub static LOGON_NO_REQUEST_ZERO_SESSION: AtomicU64 = AtomicU64::new(0);
/// 见 [`LOGON_NO_REQUEST_DISCONNECTED`]：保存中重排队。
pub static LOGON_NO_REQUEST_REQUEUE: AtomicU64 = AtomicU64::new(0);
/// **logon 处理函数入口**（`GateEvent::LoadDbRequest` 臂首行）：恒等式
/// `logon_handler_enter == loaddb_request_sent + Σ(logon_no_request_*)`（**无重排队时**成立；
/// `RequeueSaving` 会把同一次 logon 再处理一遍，此时按「每遍处理」计）。
pub static LOGON_HANDLER_ENTER: AtomicU64 = AtomicU64::new(0);
/// 「收到 logon 但没发请求」分支⑤：`db` 装配缺失（`drain_read_storage(|_| None)` 直接踢）。
pub static LOGON_NO_REQUEST_NO_DB_LINK: AtomicU64 = AtomicU64::new(0);
/// 取档循环里**被取走但 ident 不是 `DBR_LOADHUMANRCD`** 的应答数（原静默丢弃）。
pub static REPLY_IDENT_OTHER: AtomicU64 = AtomicU64::new(0);
/// `feed` 时同一 query_id **重复到达**（表按 qid 覆盖 ⇒ 多出的应答消失）的次数。
pub static REPLY_DUPLICATE_QID: AtomicU64 = AtomicU64::new(0);

/// **logon 侧出口恒等式的两条边**（卡 `openmir2-entry-gap-buckets` 第三轮）：
/// `logon_handler_exit_total` 在每个「离开 logon 处理链」的出口 +1（成功发出请求也计入），
/// 断言 `logon_handler_enter == logon_handler_exit_total`；不闭合就说明还有没被计数的出口。
pub static LOGON_HANDLER_EXIT_TOTAL: AtomicU64 = AtomicU64::new(0);
/// logon 出口理由里**未标注**的次数（RAII 守卫 `Uncounted`；>0 即有一条路径没标）。
pub static LOGON_EXIT_UNCOUNTED: AtomicU64 = AtomicU64::new(0);
/// 排水后**仍滞留在读档缓冲**里的 logon 条数（历史最大值；用来给「没有出口」的缺口归位）。
pub static LOGON_PENDING_LISTS_MAX: AtomicU64 = AtomicU64::new(0);
/// 应答**存进表**的次数（只数取档应答）：`reply_seen == reply_stored` 应恒成立。
pub static REPLY_STORED: AtomicU64 = AtomicU64::new(0);
/// 关服时**残留并被记账丢弃**的应答条数（卡 `openmir2-entry-gap-buckets`）：排水窗口后仍在表里的，
/// 不再"消失"而是记在这里。判据：`reply_in_flight == reply_dropped_at_shutdown`（恒等，不是 0）。
pub static REPLY_DROPPED_AT_SHUTDOWN: AtomicU64 = AtomicU64::new(0);
/// `logon_in_flight` 的历史峰值（在**排水结束处**采样；守卫活着 = 该条目正在被处理）。
pub static LOGON_IN_FLIGHT_MAX: AtomicU64 = AtomicU64::new(0);
/// 断线抢占：`FrontEngine::delete_human` 把**尚未排水**的 logon 从 load/temp 两个缓冲里摘掉的条数。
pub static LOGON_EXIT_DISCONNECT_DROP: AtomicU64 = AtomicU64::new(0);
/// **应答表移除**计数（只数 `DBR_LOADHUMANRCD`）：`DbReplies::take` 取走条目时 +1。
pub static REPLY_CONSUMED: AtomicU64 = AtomicU64::new(0);
/// **匹配到等待者**：取档循环里 `take(qid)` 命中后、进入 ident 判断之前 +1。
pub static REPLY_MATCHED: AtomicU64 = AtomicU64::new(0);

/// 6000 面应答**签名/长度校验不过**的帧数（`ReplyVerdict` 非 Accept 且非 TooShort）。
pub static REPLY_VERIFY_FAIL: AtomicU64 = AtomicU64::new(0);
/// 每轮取档排水后**仍滞留在应答表里**的应答数（取**历史最大值**）：它直接把
/// 「fed 了但没进 attempt」的差值量化出来（qid 对不上/会话已摘）。
pub static REPLY_PENDING_MAX: AtomicU64 = AtomicU64::new(0);

/// 6000 面**断链重连**次数（成功建连但不是第一次；卡 `openmir2-db-link-request-drop`）。
/// 与 `loaddb_request_sent`/`loaddb_write_error` 对读：重连前写成功的请求可能随旧连接一起丢。
pub static DB_LINK_RECONNECT: AtomicU64 = AtomicU64::new(0);
/// 收到 `DBR_LOADHUMANRCD` 应答的次数——**在 `recog` 判断之前**（签名通过即计），
/// 用来区分「应答没回来」与「回来了却没进 attempt」（query 对不上的那 8 个）。
pub static LOADDB_REPLY_SEEN: AtomicU64 = AtomicU64::new(0);

/// 每个样本类别最多打印几条（前 N 条原样字段；之后只计数）。
const SAMPLE_LIMIT: u64 = 5;

/// `recog != 1` 样本的打印序号计数器（与计数无关，只用于截断前 N 条）。
static SAMPLE_LOG_REPLY_OTHER: AtomicU64 = AtomicU64::new(0);

fn bump(counter: &AtomicU64) -> Option<String> {
    let n = counter.fetch_add(1, Relaxed) + 1;
    if PRINT_EVERY == 0 || !n.is_multiple_of(PRINT_EVERY) {
        return None;
    }
    Some(summary_line())
}

/// 网关侧登录事件 +1，并把**首个**事件时刻记进 `FIRST_ENTER_MS`（只写一次，不碰热路径的时钟）；
/// 该次触达整百 ⇒ 返回一行汇总（调用方负责 `glog!`）。
#[must_use]
pub fn record_logon(now_ms: i64) -> Option<String> {
    let _ = FIRST_ENTER_MS.compare_exchange(-1, now_ms, Relaxed, Relaxed);
    bump(&LOGON_FRAMES_SEEN)
}

/// 进世界尝试 +1；该次触达整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_attempt() -> Option<String> {
    bump(&ENTER_ATTEMPT)
}

/// 进世界成功 +1，并把本次时刻记进 `LAST_ENTER_MS`（**末次覆盖**：只留最后一条成功）；
/// 该次触达整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_ok(now_ms: i64) -> Option<String> {
    LAST_ENTER_MS.store(now_ms, Relaxed);
    bump(&ENTER_OK)
}

/// 取档请求已排队 +1；整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_loaddb_request_sent() -> Option<String> {
    bump(&LOADDB_REQUEST_SENT)
}

/// 写 6000 面失败 +1；整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_loaddb_write_error() -> Option<String> {
    bump(&LOADDB_WRITE_ERROR)
}

/// 断线抢占摘掉未排水 logon 的条数（**同时计入 exit_total**：被摘的条目也已离开处理链）。
pub fn record_logon_disconnect_drop(n: u64) {
    if n > 0 {
        LOGON_EXIT_DISCONNECT_DROP.fetch_add(n, Relaxed);
        LOGON_HANDLER_EXIT_TOTAL.fetch_add(n, Relaxed);
    }
}

/// 应答存进表 +1（「记到」≠「存进表」，分开计）。
pub fn record_reply_stored() {
    REPLY_STORED.fetch_add(1, Relaxed);
}

/// **在飞量**：`enter − exit_total`。>0 = 有守卫还活着（条目正在处理，或处理被卡住）；
/// **不等于丢失**。终值行应把它打到 0（关服后无处理在飞）。
#[must_use]
pub fn logon_in_flight() -> i64 {
    let enter = LOGON_HANDLER_ENTER.load(Relaxed);
    let exit = LOGON_HANDLER_EXIT_TOTAL.load(Relaxed);
    i64::try_from(enter).unwrap_or(i64::MAX) - i64::try_from(exit).unwrap_or(i64::MAX)
}

/// **应答在飞量**：`stored − consumed`（>0 = 应答已入表但还没被取走）。
#[must_use]
pub fn reply_in_flight() -> i64 {
    let stored = REPLY_STORED.load(Relaxed);
    let consumed = REPLY_CONSUMED.load(Relaxed);
    i64::try_from(stored).unwrap_or(i64::MAX) - i64::try_from(consumed).unwrap_or(i64::MAX)
}

/// 在排水结束处采样 `logon_in_flight` 的峰值。
pub fn record_logon_in_flight_sample() {
    let v = logon_in_flight();
    if v <= 0 {
        return;
    }
    let v = u64::try_from(v).unwrap_or(0);
    let mut cur = LOGON_IN_FLIGHT_MAX.load(Relaxed);
    while v > cur {
        match LOGON_IN_FLIGHT_MAX.compare_exchange(cur, v, Relaxed, Relaxed) {
            Ok(_) => break,
            Err(observed) => cur = observed,
        }
    }
}

/// 关服排水窗口结束后仍残留的应答 ⇒ 记账（调用方传入当时的 `reply_in_flight`）。
pub fn record_reply_dropped_at_shutdown(n: u64) {
    REPLY_DROPPED_AT_SHUTDOWN.store(n, Relaxed);
}

/// 记录读档缓冲里的滞留条数（取历史最大值）。
pub fn record_logon_pending(n: u64) {
    let mut cur = LOGON_PENDING_LISTS_MAX.load(Relaxed);
    while n > cur {
        match LOGON_PENDING_LISTS_MAX.compare_exchange(cur, n, Relaxed, Relaxed) {
            Ok(_) => break,
            Err(observed) => cur = observed,
        }
    }
}

/// logon 出口理由（RAII 守卫用）。**未标注 = `Uncounted`**：它在读数里出现就说明有一条路径没标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogonExitReason {
    /// 没有在任何出口显式设置理由（**漏标注**的直接信号）。
    Uncounted,
    /// 成功发出取档请求。
    Success,
    /// `SessionID == 0` 被读档循环跳过。
    ZeroSession,
    /// 角色数据保存中 ⇒ 重排队。
    Requeue,
    /// 读档失败/未连接 ⇒ 踢出。
    Kicked,
}

/// **RAII 出口守卫**（卡 `openmir2-entry-gap-buckets` 第四轮）：入口创建、出口 `set(reason)`，
/// `Drop` 统一 `exit_total += 1`（`Uncounted` 另计）。任何 `return`/`continue`/`?`/panic 展开都必然
/// 经过 `Drop` ⇒ `enter == exit_total` 结构上不可能漏；漏的只可能是**理由**，由 `Uncounted` 暴露。
pub struct LogonExitGuard {
    reason: LogonExitReason,
}

impl LogonExitGuard {
    #[must_use]
    pub fn new() -> Self {
        Self {
            reason: LogonExitReason::Uncounted,
        }
    }

    /// 离开作用域前标明出口理由。
    pub fn set(&mut self, reason: LogonExitReason) {
        self.reason = reason;
    }
}

impl Drop for LogonExitGuard {
    fn drop(&mut self) {
        LOGON_HANDLER_EXIT_TOTAL.fetch_add(1, Relaxed);
        if self.reason == LogonExitReason::Uncounted {
            LOGON_EXIT_UNCOUNTED.fetch_add(1, Relaxed);
        }
    }
}

/// 应答被从表里移除（只数取档应答）。
pub fn record_reply_consumed() {
    REPLY_CONSUMED.fetch_add(1, Relaxed);
}

/// 应答匹配到等待者。
pub fn record_reply_matched() {
    REPLY_MATCHED.fetch_add(1, Relaxed);
}

/// logon 处理函数入口 +1。
#[must_use]
pub fn record_logon_handler_enter() -> Option<String> {
    let out = bump(&LOGON_HANDLER_ENTER);
    // **峰值在增量处采样**（排水结束处采样会错过瞬时峰值：那时多半已排空）。
    let v = logon_in_flight();
    if v > 0 {
        if let Ok(v) = u64::try_from(v) {
            let mut cur = LOGON_IN_FLIGHT_MAX.load(Relaxed);
            while v > cur {
                match LOGON_IN_FLIGHT_MAX.compare_exchange(cur, v, Relaxed, Relaxed) {
                    Ok(_) => break,
                    Err(observed) => cur = observed,
                }
            }
        }
    }
    out
}

/// 「收到 logon 但没发请求」分支⑤：`db` 未装配。
#[must_use]
pub fn record_logon_no_db_link() -> Option<String> {
    bump(&LOGON_NO_REQUEST_NO_DB_LINK)
}

/// 取走但 ident 不是取档应答 +1。
#[must_use]
pub fn record_reply_ident_other() -> Option<String> {
    bump(&REPLY_IDENT_OTHER)
}

/// 同一 qid 的重复应答 +1。
#[must_use]
pub fn record_reply_duplicate_qid() -> Option<String> {
    bump(&REPLY_DUPLICATE_QID)
}

/// 「收到 logon 但没发请求」的四条分支各记一次；整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_logon_no_request(bucket: LogonNoRequest) -> Option<String> {
    match bucket {
        LogonNoRequest::Disconnected => bump(&LOGON_NO_REQUEST_DISCONNECTED),
        LogonNoRequest::SendFail => bump(&LOGON_NO_REQUEST_SEND_FAIL),
        LogonNoRequest::ZeroSession => bump(&LOGON_NO_REQUEST_ZERO_SESSION),
        LogonNoRequest::Requeue => bump(&LOGON_NO_REQUEST_REQUEUE),
    }
}

/// [`record_logon_no_request`] 的分支枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogonNoRequest {
    /// DB 链路未连接（`drain_read_storage` 的闭包直接 None）。
    Disconnected,
    /// `req_tx.send` 失败（链路已断）。
    SendFail,
    /// `SessionID == 0`（读档循环跳过）。
    ZeroSession,
    /// 角色数据保存中 ⇒ 重排队。
    Requeue,
}

/// 应答签名/长度校验不过 +1；整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_reply_verify_fail() -> Option<String> {
    bump(&REPLY_VERIFY_FAIL)
}

/// 记录「排水后仍滞留在应答表里的应答数」（取历史最大值；>0 时返回一行汇总供日志）。
#[must_use]
pub fn record_reply_pending(pending: u64) -> Option<String> {
    let mut cur = REPLY_PENDING_MAX.load(Relaxed);
    while pending > cur {
        match REPLY_PENDING_MAX.compare_exchange(cur, pending, Relaxed, Relaxed) {
            Ok(_) => break,
            Err(observed) => cur = observed,
        }
    }
    if pending == 0 {
        None
    } else {
        Some(summary_line())
    }
}

/// 6000 面重连 +1；整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_db_link_reconnect() -> Option<String> {
    bump(&DB_LINK_RECONNECT)
}

/// 收到 `DBR_LOADHUMANRCD` 应答（recog 判断之前）+1；整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_loaddb_reply_seen() -> Option<String> {
    bump(&LOADDB_REPLY_SEEN)
}

/// `recog != 1` 的取档应答 +1；整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_load_reply_other() -> Option<String> {
    bump(&LOAD_REPLY_OTHER)
}

/// 该 `recog != 1` 应答是否该打样本行（前 [`SAMPLE_LIMIT`] 条为真，其余为假）。
#[must_use]
pub fn sample_load_reply_other() -> bool {
    SAMPLE_LOG_REPLY_OTHER.fetch_add(1, Relaxed) < SAMPLE_LIMIT
}

/// **终值行只打一次**（信号处理路径与关服分支共用）：首次返回 `Some`，其后恒 `None`。
/// 判据「关服后日志里有且仅有一条终值行」就靠它——重复调用不会多打，漏调用则由日志缺失暴露。
#[must_use]
pub fn final_line_once() -> Option<String> {
    static ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ONCE.swap(true, Relaxed) {
        return None;
    }
    Some(summary_line())
}

/// 一行汇总（调用方决定何时打印（字段名固定，供日志侧机判）。
#[must_use]
pub fn summary_line() -> String {
    format!(
        "ENTRY_TELEMETRY svc=game-svr logon_frames_seen={} enter_attempt={} enter_ok={}          load_reply_other={} first_enter_ms={} last_enter_ms={}          loaddb_request_sent={} loaddb_reply_seen={} loaddb_write_error={} db_link_reconnect={}          logon_handler_enter={} logon_handler_exit_total={} logon_exit_uncounted={} logon_disconnect_drop={} logon_pending_lists_max={} logon_in_flight={} logon_in_flight_max={} reply_in_flight={} reply_dropped_at_shutdown={} logon_no_request[disconnected={} send_fail={} zero_session={} requeue={} no_db_link={}] reply_ident_other={} reply_duplicate_qid={} reply_stored={} reply_consumed={} reply_matched={}          reply_verify_fail={} reply_pending_max={}",
        LOGON_FRAMES_SEEN.load(Relaxed),
        ENTER_ATTEMPT.load(Relaxed),
        ENTER_OK.load(Relaxed),
        LOAD_REPLY_OTHER.load(Relaxed),
        FIRST_ENTER_MS.load(Relaxed),
        LAST_ENTER_MS.load(Relaxed),
        LOADDB_REQUEST_SENT.load(Relaxed),
        LOADDB_REPLY_SEEN.load(Relaxed),
        LOADDB_WRITE_ERROR.load(Relaxed),
        DB_LINK_RECONNECT.load(Relaxed),
        LOGON_HANDLER_ENTER.load(Relaxed),
        LOGON_HANDLER_EXIT_TOTAL.load(Relaxed),
        LOGON_EXIT_UNCOUNTED.load(Relaxed),
        LOGON_EXIT_DISCONNECT_DROP.load(Relaxed),
        LOGON_PENDING_LISTS_MAX.load(Relaxed),
        logon_in_flight(),
        LOGON_IN_FLIGHT_MAX.load(Relaxed),
        reply_in_flight(),
        REPLY_DROPPED_AT_SHUTDOWN.load(Relaxed),
        LOGON_NO_REQUEST_DISCONNECTED.load(Relaxed),
        LOGON_NO_REQUEST_SEND_FAIL.load(Relaxed),
        LOGON_NO_REQUEST_ZERO_SESSION.load(Relaxed),
        LOGON_NO_REQUEST_REQUEUE.load(Relaxed),
        LOGON_NO_REQUEST_NO_DB_LINK.load(Relaxed),
        REPLY_IDENT_OTHER.load(Relaxed),
        REPLY_DUPLICATE_QID.load(Relaxed),
        REPLY_STORED.load(Relaxed),
        REPLY_CONSUMED.load(Relaxed),
        REPLY_MATCHED.load(Relaxed),
        REPLY_VERIFY_FAIL.load(Relaxed),
        REPLY_PENDING_MAX.load(Relaxed)
    )
}

/// 测试专用：复位两个时间戳（生产路径不调用；计数器本身单调不减，只按差值断言）。
#[cfg(test)]
pub(crate) fn reset_timestamps_for_tests() {
    FIRST_ENTER_MS.store(-1, Relaxed);
    LAST_ENTER_MS.store(-1, Relaxed);
}

/// 进程级计数器被并行用例共享 ⇒ 会互相改数的用例必须串行（否则断言「另一个计数不变」会被
/// 同模块/别模块的用例 bump 掉，表现为 flaky 而不是真红）。接线牙不碰计数器，无需加锁。
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    /// 整百才打印：0 / 半档不打印，第 100 次触达返回 Summary；计数器本身每次 +1。
    #[test]
    fn print_only_on_exact_multiples() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 计数器是进程级全局量：用「差值」而不是绝对值（其它用例/并行测试也会动它们）。
        let before = LOGON_FRAMES_SEEN.load(Ordering::Relaxed);
        let mut printed = 0;
        for i in 0..PRINT_EVERY {
            if record_logon(1_000 + i as i64).is_some() {
                printed += 1;
            }
        }
        let after = LOGON_FRAMES_SEEN.load(Ordering::Relaxed);
        assert_eq!(after, before + PRINT_EVERY, "每次调用必须 +1");
        assert_eq!(
            printed, 1,
            "PRINT_EVERY 次里只该在第 {PRINT_EVERY} 次打印一行（打印判档必须用 fetch_add 的返回值）"
        );
    }

    /// 汇总行把三个字段名都写出来（缺字段 = 读数时看不出是 0 还是没接）。
    #[test]
    fn summary_line_carries_every_field() {
        let line = summary_line();
        assert!(line.starts_with("ENTRY_TELEMETRY svc=game-svr "), "{line}");
        for key in [
            "logon_frames_seen=",
            "enter_attempt=",
            "enter_ok=",
            "load_reply_other=",
            "first_enter_ms=",
            "last_enter_ms=",
            "loaddb_request_sent=",
            "loaddb_reply_seen=",
            "loaddb_write_error=",
            "db_link_reconnect=",
            "logon_handler_enter=",
            "logon_handler_exit_total=",
            "logon_exit_uncounted=",
            "logon_disconnect_drop=",
            "logon_pending_lists_max=",
            "logon_in_flight=",
            "logon_in_flight_max=",
            "reply_in_flight=",
            "reply_dropped_at_shutdown=",
            "logon_no_request[",
            "no_db_link=",
            "reply_ident_other=",
            "reply_duplicate_qid=",
            "reply_stored=",
            "reply_consumed=",
            "reply_matched=",
            "reply_verify_fail=",
            "reply_pending_max=",
        ] {
            assert!(line.contains(key), "汇总行缺字段 {key}: {line}");
        }
    }

    /// 样本只打前 `SAMPLE_LIMIT` 条：第 1..=N 条为真、之后的都是假（空集形态：第一次调用必须为真）。
    #[test]
    fn sample_cap_is_exactly_first_n() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = SAMPLE_LOG_REPLY_OTHER.load(Ordering::Relaxed);
        assert!(sample_load_reply_other(), "第一条必须打样本");
        for _ in 1..SAMPLE_LIMIT {
            assert!(sample_load_reply_other(), "前 {SAMPLE_LIMIT} 条都要打");
        }
        assert!(
            !sample_load_reply_other(),
            "第 {} 条及以后不得再打（截断牙）",
            SAMPLE_LIMIT + 1
        );
        assert_eq!(
            SAMPLE_LOG_REPLY_OTHER.load(Ordering::Relaxed),
            before + SAMPLE_LIMIT + 1,
            "采样计数器必须逐次 +1（与业务计数分开）"
        );
    }

    /// 首时间戳只记**第一次** logon：后续 logon 不得改写（删 `compare_exchange` ⇒ 本用例红）。
    #[test]
    fn first_enter_ms_records_only_first_logon() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_timestamps_for_tests();
        let _ = record_logon(1_000);
        let _ = record_logon(2_500);
        assert_eq!(
            FIRST_ENTER_MS.load(Ordering::Relaxed),
            1_000,
            "首个时间戳必须是第一次 logon 的时钟值"
        );
    }

    /// 末时间戳**末次覆盖**：两次 enter_ok 之后必须等于后一次（删 `store` 或改成只写一次 ⇒ 红）。
    #[test]
    fn last_enter_ms_takes_last_enter_ok() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_timestamps_for_tests();
        let _ = record_ok(5_000);
        let _ = record_ok(9_000);
        assert_eq!(
            LAST_ENTER_MS.load(Ordering::Relaxed),
            9_000,
            "末时间戳必须跟随最后一次进世界成功"
        );
        assert_eq!(
            FIRST_ENTER_MS.load(Ordering::Relaxed),
            -1,
            "enter_ok 不得写首时间戳（两者基准不同）"
        );
    }

    /// 滞留应答取**历史最大值**：5 之后来 3 不回落；0 不打印。
    #[test]
    fn reply_pending_tracks_max_and_prints_only_when_positive() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        REPLY_PENDING_MAX.store(0, Ordering::Relaxed);
        assert!(record_reply_pending(0).is_none(), "0 不应打印");
        assert!(record_reply_pending(5).is_some(), ">0 应打印");
        assert!(record_reply_pending(3).is_some(), ">0 应打印");
        assert_eq!(
            REPLY_PENDING_MAX.load(Ordering::Relaxed),
            5,
            "必须保留历史最大值，不得回落"
        );
    }

    /// **RAII 守卫**：没 `set` 就 Drop ⇒ `exit_total` 与 `Uncounted` 各 +1；`set` 过 ⇒ 只 +total。
    /// 这保证「漏标路径」一定可见（删掉 Drop 里的计数 ⇒ 本用例红）。
    #[test]
    fn exit_guard_counts_and_flags_unlabelled() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before_total = LOGON_HANDLER_EXIT_TOTAL.load(Ordering::Relaxed);
        let before_unc = LOGON_EXIT_UNCOUNTED.load(Ordering::Relaxed);
        {
            let _guard = LogonExitGuard::new();
        }
        assert_eq!(
            LOGON_HANDLER_EXIT_TOTAL.load(Ordering::Relaxed),
            before_total + 1
        );
        assert_eq!(
            LOGON_EXIT_UNCOUNTED.load(Ordering::Relaxed),
            before_unc + 1,
            "未标注必须计入 Uncounted"
        );
        {
            let mut guard = LogonExitGuard::new();
            guard.set(LogonExitReason::Success);
        }
        assert_eq!(
            LOGON_HANDLER_EXIT_TOTAL.load(Ordering::Relaxed),
            before_total + 2
        );
        assert_eq!(
            LOGON_EXIT_UNCOUNTED.load(Ordering::Relaxed),
            before_unc + 1,
            "标注过的出口不得计入 Uncounted"
        );
    }

    /// **在飞量 = enter − exit_total / stored − consumed**：直接是差值（删掉减法或写死 0 ⇒ 红）。
    #[test]
    fn in_flight_is_derived_from_counters() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        LOGON_HANDLER_ENTER.store(10, Ordering::Relaxed);
        LOGON_HANDLER_EXIT_TOTAL.store(7, Ordering::Relaxed);
        assert_eq!(
            logon_in_flight(),
            3,
            "logon_in_flight 必须是 enter − exit_total"
        );
        REPLY_STORED.store(5, Ordering::Relaxed);
        REPLY_CONSUMED.store(2, Ordering::Relaxed);
        assert_eq!(
            reply_in_flight(),
            3,
            "reply_in_flight 必须是 stored − consumed"
        );
        // 复位，别污染其它用例
        LOGON_HANDLER_ENTER.store(0, Ordering::Relaxed);
        LOGON_HANDLER_EXIT_TOTAL.store(0, Ordering::Relaxed);
        REPLY_STORED.store(0, Ordering::Relaxed);
        REPLY_CONSUMED.store(0, Ordering::Relaxed);
    }

    /// **终值行只打一次**：第一次 `Some`、第二次 `None`（改成恒返回 ⇒ 红）。
    #[test]
    fn final_line_once_emits_exactly_once() {
        let first = final_line_once();
        assert!(first.is_some(), "首次调用必须返回终值行");
        assert!(first.unwrap().starts_with("ENTRY_TELEMETRY svc=game-svr "));
        assert!(
            final_line_once().is_none(),
            "第二次及以后必须为 None（只打一条）"
        );
    }

    /// **峰值在增量处采样**：enter 之后 `logon_in_flight_max` 必须跟上（只在排水后采样会漏 ⇒ 红）。
    #[test]
    fn in_flight_max_updates_on_enter() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        LOGON_HANDLER_ENTER.store(0, Ordering::Relaxed);
        LOGON_HANDLER_EXIT_TOTAL.store(0, Ordering::Relaxed);
        LOGON_IN_FLIGHT_MAX.store(0, Ordering::Relaxed);
        let _ = record_logon_handler_enter();
        let _ = record_logon_handler_enter();
        LOGON_HANDLER_EXIT_TOTAL.fetch_add(1, Ordering::Relaxed);
        assert!(
            LOGON_IN_FLIGHT_MAX.load(Ordering::Relaxed) >= 2,
            "峰值必须 ≥ 增量处观测到的在飞数"
        );
        LOGON_HANDLER_ENTER.store(0, Ordering::Relaxed);
        LOGON_HANDLER_EXIT_TOTAL.store(0, Ordering::Relaxed);
        LOGON_IN_FLIGHT_MAX.store(0, Ordering::Relaxed);
    }

    /// 关服残留记账：`record_reply_dropped_at_shutdown(n)` 写入后汇总行必须带该值。
    #[test]
    fn reply_dropped_at_shutdown_is_recorded() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        record_reply_dropped_at_shutdown(48);
        let line = summary_line();
        assert!(line.contains("reply_dropped_at_shutdown=48"), "{line}");
        record_reply_dropped_at_shutdown(0);
    }

    /// 三个计数器互相独立：动一个不许动另一个（同族「删任一计数 ⇒ 对应断言红」的前提）。
    #[test]
    fn counters_are_independent() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = LOGON_FRAMES_SEEN.load(Ordering::Relaxed);
        let b = ENTER_ATTEMPT.load(Ordering::Relaxed);
        let c = ENTER_OK.load(Ordering::Relaxed);
        let _ = record_attempt();
        assert_eq!(
            LOGON_FRAMES_SEEN.load(Ordering::Relaxed),
            a,
            "attempt 不得动 logon"
        );
        assert_eq!(ENTER_ATTEMPT.load(Ordering::Relaxed), b + 1);
        assert_eq!(ENTER_OK.load(Ordering::Relaxed), c, "attempt 不得动 ok");
    }
}
