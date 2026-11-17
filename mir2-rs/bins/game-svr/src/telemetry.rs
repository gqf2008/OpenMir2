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

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};

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
/// 终值行一次性开关（`final_line_once` 用；测试可复位）。
static FINAL_LINE_TAKEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
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
/// 应答表读数 ①：**排水后残留峰值**（取**历史最大值**）。语义是「**真滞留**」——
/// 本轮 `take` 扫过一遍后仍没人取（qid 对不上 / 会话已摘），也就是「fed 了但没进 attempt」。
///
/// **口径（旧语义，历史可比）**：2026-11-17 那轮 649 的报告里 `reply_pending_max=0` 就是本字段，
/// 按「排水后残留」读；修复前后的窗口读数可以直接横向对比。
///
/// 与 [`REPLY_TABLE_MAX`]（顶部=到达积压）**不是一个量**，两者都要：
/// 排水后的这个数才是诊断「649 条躺在表里没人取」的判据；只说「表里有多少」会把本轮
/// 本来就会被取走的新到达也算成滞留（2000 档每轮到达几十~上百条，那个数会常态很大）。
pub static REPLY_PENDING_MAX: AtomicU64 = AtomicU64::new(0);
/// 应答表读数 ②：**每轮开头表内条数峰值**（取**历史最大值**）。语义是「**到达积压**」——
/// 本轮开头表里有多少条待处理（含本轮排水就会取走的新到达），度量的是消费端活性，
/// **不度量滞留**。它放在 tick 循环顶部 ⇒ 即使事件排水被预算截断也**每轮必采**。
///
/// **口径（本轮新增）**：没有历史值可比，**不与旧窗口的 `reply_pending_max` 横比**——
/// 两者量的不是一件事（见上）。与 [`REPLY_PENDING_MAX`] 对读：顶部大而排水后小 = 到达多
/// 但都在被消费；两者都大 = 真的有滞留。只留一个读数就会像今晚一样被误读。
pub static REPLY_TABLE_MAX: AtomicU64 = AtomicU64::new(0);

/// **事件排水累计取走条数**（卡 `openmir2-tick-starvation`）：每轮 tick 从 `ev_rx` 取走一条 +1。
/// 与 [`DRAIN_BUDGET_HIT`] 对读：没有预算时它会在**同一次 tick 内**无限增长（tick 永不返回）。
pub static DRAIN_EVENTS_DRAINED: AtomicU64 = AtomicU64::new(0);
/// **事件排水因预算被截断的轮次数**（卡 `openmir2-tick-starvation`）：>0 = 本轮还有事件留在
/// 通道里、被延后到下一轮。它与 `world.clock_ms` 对读：截断而时钟仍前进 = 预算生效。
pub static DRAIN_BUDGET_HIT: AtomicU64 = AtomicU64::new(0);

/// 6000 面**断链重连**次数（成功建连但不是第一次；卡 `openmir2-db-link-request-drop`）。
/// 与 `loaddb_request_sent`/`loaddb_write_error` 对读：重连前写成功的请求可能随旧连接一起丢。
pub static DB_LINK_RECONNECT: AtomicU64 = AtomicU64::new(0);
/// 收到 `DBR_LOADHUMANRCD` 应答的次数——**在 `recog` 判断之前**（签名通过即计），
/// 用来区分「应答没回来」与「回来了却没进 attempt」（query 对不上的那 8 个）。
pub static LOADDB_REPLY_SEEN: AtomicU64 = AtomicU64::new(0);

/// **下发时目标槽已不存在**而丢弃的帧数（进程级；卡 `openmir2-missing-socket-warn-storm`）。
///
/// 每次落空 **+1**，**与是否已告警无关**——告警节流只压日志，不压计数。窗口 37（5000 档）
/// 这一个口子打了 2,303,294 条 WRN（占全部 WRN 99.7%，日志 238MB）：客户端 60s play 超时先于
/// 取档排队（尾部 p90 85s）⇒ 断开后世界仍保留会话每 tick 广播，每次落空逐条同步打一行。
/// 该字段进 [`summary_line`]（`ENTRY_TELEMETRY`），降噪后的落空量仍可对账。
pub static DOWNLINK_DEAD_SOCKET_TOTAL: AtomicU64 = AtomicU64::new(0);

/// **墓碑命中、在入队前就被放弃的帧批次**（进程级；卡 `openmir2-world-keeps-dead-session`）。
///
/// 与 [`DOWNLINK_DEAD_SOCKET_TOTAL`] **分开计**：后者是「帧一路排到最后一公里、到网关侧
/// 才发现槽已不存在」，本字段是「世界侧路由时就知道该 `(线号, socket)` 已死、根本没产生帧」。
/// 一个降一个升各自说明修复面的位置——混进同一个字段就分不开「帧没发」与「发了才发现没人收」。
///
/// 计数点在**生产者路由**（`route_to_gate`）：多数调用点一次路由 = 一帧；同一次路由后成批
/// 入队的调用点按**批**计（真实帧数由最后一公里的 `downlink_dead_socket_total` 对账）。
pub static DOWNLINK_SKIPPED_BY_TOMBSTONE_TOTAL: AtomicU64 = AtomicU64::new(0);

/// 已告警过的 `(网关线号, socket)`。键**必须带线号**：socket 号只在一条网关线内唯一
/// （不同网关线的同号 socket 是不同会话，各自都该有自己的首告警）。
static DEAD_SOCKET_WARNED: OnceLock<Mutex<HashSet<(u8, i32)>>> = OnceLock::new();

/// 两次「落空聚合行」之间的最小间隔（毫秒）：非首次落空**最多 1 行/秒**，取代逐帧 WRN。
const DEAD_SOCKET_AGG_INTERVAL_MS: i64 = 1_000;
/// 自上次聚合行以来被抑制（未逐条 WRN）的落空次数。
static DEAD_SOCKET_SUPPRESSED: AtomicU64 = AtomicU64::new(0);
/// 上次聚合行的时钟（`i64::MIN` = 从未打过 ⇒ 第一条被抑制的落空立即聚合成行）。
static DEAD_SOCKET_LAST_AGG_MS: AtomicI64 = AtomicI64::new(i64::MIN);

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

/// 记录「**排水后残留峰值**」（取历史最大值；>0 时返回一行汇总供日志）。
///
/// 口径与 [`REPLY_PENDING_MAX`] 一致：调用点必须在**应答排水块结束处**（`w.outstanding = still;`
/// 之后、用 `r.len()`），否则读数就不是「真滞留」。历史可比（649 那轮就是它）。
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

/// 记录「**每轮开头应答表内条数峰值**」（取历史最大值；>0 时返回一行汇总供日志）。
///
/// 口径与 [`REPLY_TABLE_MAX`] 一致：调用点必须在 **tick 循环顶部**（每轮必采）；它度量的是
/// 到达积压/消费端活性，**不是**滞留。本轮新增、无历史值可比（不与旧窗口横比）。
#[must_use]
pub fn record_reply_table(len: u64) -> Option<String> {
    let mut cur = REPLY_TABLE_MAX.load(Relaxed);
    while len > cur {
        match REPLY_TABLE_MAX.compare_exchange(cur, len, Relaxed, Relaxed) {
            Ok(_) => break,
            Err(observed) => cur = observed,
        }
    }
    if len == 0 {
        None
    } else {
        Some(summary_line())
    }
}

/// 事件排水从通道取走一条 +1（调用点唯一：sim 侧带预算排水器的取事件处）。
/// 无打印：它每 tick 都要动，汇总行里给累计值即可。
pub fn record_drain_event_drained() {
    DRAIN_EVENTS_DRAINED.fetch_add(1, Relaxed);
}

/// 本轮事件排水**因条数/墙钟预算被截断** +1；整百 ⇒ 返回一行汇总（调用方按 WRN 打）。
///
/// 用 [`bump`] 的节流：预算被触达可能是常态（高负载档每轮都触达），逐轮打会刷屏。
#[must_use]
pub fn record_drain_budget_hit() -> Option<String> {
    bump(&DRAIN_BUDGET_HIT)
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

fn dead_socket_warned() -> &'static Mutex<HashSet<(u8, i32)>> {
    DEAD_SOCKET_WARNED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// 记一次「下发时槽不存在」并判定**该不该打 WRN**（卡 `openmir2-missing-socket-warn-storm`）。
///
/// - 计数 [`DOWNLINK_DEAD_SOCKET_TOTAL`] **总是 +1**（节流只压日志，不压计数）；
/// - 返回 `true` = 该 `(网关线号, socket)` **首次**落空 ⇒ 调用方打**一条** WRN；
///   同一键此后恒 `false`（不再逐条 WRN，后续量走 [`aggregate_downlink_dead_socket`] 的
///   按秒聚合行 + 汇总行里的累计字段）。
#[must_use]
pub fn record_downlink_dead_socket(gate_line: u8, socket: i32) -> bool {
    DOWNLINK_DEAD_SOCKET_TOTAL.fetch_add(1, Relaxed);
    let mut warned = dead_socket_warned()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    warned.insert((gate_line, socket))
}

/// 墓碑命中一次（入队前丢弃）：+1 并返回累计值。与 [`DOWNLINK_DEAD_SOCKET_TOTAL`] 分开。
#[must_use]
pub fn record_downlink_skipped_by_tombstone() -> u64 {
    DOWNLINK_SKIPPED_BY_TOMBSTONE_TOTAL.fetch_add(1, Relaxed) + 1
}

/// 读「墓碑跳过」累计值（测试/窗口对账用；生产日志走 [`summary_line`]）。
#[cfg_attr(not(test), allow(dead_code))]
#[must_use]
pub fn downlink_skipped_by_tombstone_total() -> u64 {
    DOWNLINK_SKIPPED_BY_TOMBSTONE_TOTAL.load(Relaxed)
}

/// 非首次落空的**按秒聚合**：`Some(行)` = 距上次聚合 ≥ [`DEAD_SOCKET_AGG_INTERVAL_MS`]，
/// 行内带这段时间被抑制（未逐条 WRN）的条数与进程累计值。
///
/// **前缀必须是 `DOWNLINK_DEAD_SOCKET_AGG`，不得以 `ENTRY_TELEMETRY` 开头**：本项目口径是
/// 「日志里**最后一条** `ENTRY_TELEMETRY` 行 = 终值」，舰队归档/停止脚本与窗口复核都按
/// `Select-String 'ENTRY_TELEMETRY' | Select-Object -Last 1` 取它；风暴恰在窗口末段，若聚合行
/// 复用该前缀，最后一条终值会被只有两个字段的聚合行顶掉、所有计数丢失
/// （卡 `openmir2-missing-socket-warn-storm` P1 返工）。
///
/// 逐帧只做一次原子加、一次读：不到 1s 不分配、不打印 ⇒ 高负载下这个口子最多 1 行/秒。
pub fn aggregate_downlink_dead_socket(now_ms: i64) -> Option<String> {
    DEAD_SOCKET_SUPPRESSED.fetch_add(1, Relaxed);
    let last = DEAD_SOCKET_LAST_AGG_MS.load(Relaxed);
    if now_ms.saturating_sub(last) < DEAD_SOCKET_AGG_INTERVAL_MS {
        return None;
    }
    // 只有抢到窗口的那一次打印（并发调用不会各打一行）。
    if DEAD_SOCKET_LAST_AGG_MS
        .compare_exchange(last, now_ms, Relaxed, Relaxed)
        .is_err()
    {
        return None;
    }
    let suppressed = DEAD_SOCKET_SUPPRESSED.swap(0, Relaxed);
    Some(format!(
        "DOWNLINK_DEAD_SOCKET_AGG svc=game-svr suppressed={suppressed} total={}",
        DOWNLINK_DEAD_SOCKET_TOTAL.load(Relaxed)
    ))
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
    if FINAL_LINE_TAKEN.swap(true, Relaxed) {
        return None;
    }
    // **唯一记账出口**（卡 `openmir2-entry-gap-buckets`）：所有退出路径（正常/超时/信号/管道）都经过这里。
    // 语义：`reply_dropped_at_shutdown` = **关服时仍未被消费的残留**的定义值，**不是"被处理过"**。
    let inflight = reply_in_flight();
    if inflight > 0 {
        if let Ok(n) = u64::try_from(inflight) {
            record_reply_dropped_at_shutdown(n);
            crate::glog!(
                "WRN",
                "{} replies unconsumed at shutdown (reply_in_flight={})",
                n,
                inflight
            );
        }
    }
    Some(summary_line())
}

/// 测试专用：复位终值行的一次性开关（生产路径不调用）。
#[cfg(test)]
pub(crate) fn reset_final_line_once_for_tests() {
    FINAL_LINE_TAKEN.store(false, Relaxed);
}

/// 一行汇总（调用方决定何时打印（字段名固定，供日志侧机判）。
///
/// 应答表两个读数**不要混读**：`reply_pending_max` = **排水后残留峰值**（旧语义，历史可比）；
/// `reply_table_max` = **每轮开头表内条数峰值**（本轮新增，不与旧值横比）。
#[must_use]
pub fn summary_line() -> String {
    format!(
        "ENTRY_TELEMETRY svc=game-svr logon_frames_seen={} enter_attempt={} enter_ok={}          load_reply_other={} first_enter_ms={} last_enter_ms={}          loaddb_request_sent={} loaddb_reply_seen={} loaddb_write_error={} db_link_reconnect={}          logon_handler_enter={} logon_handler_exit_total={} logon_exit_uncounted={} logon_disconnect_drop={} logon_pending_lists_max={} logon_in_flight={} logon_in_flight_max={} reply_in_flight={} reply_dropped_at_shutdown={} logon_no_request[disconnected={} send_fail={} zero_session={} requeue={} no_db_link={}] reply_ident_other={} reply_duplicate_qid={} reply_stored={} reply_consumed={} reply_matched={}          reply_verify_fail={} reply_pending_max={}          drain_events_drained={} drain_budget_hit={}          reply_table_max={} downlink_dead_socket_total={} downlink_skipped_by_tombstone_total={}",
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
        REPLY_PENDING_MAX.load(Relaxed),
        DRAIN_EVENTS_DRAINED.load(Relaxed),
        DRAIN_BUDGET_HIT.load(Relaxed),
        REPLY_TABLE_MAX.load(Relaxed),
        DOWNLINK_DEAD_SOCKET_TOTAL.load(Relaxed),
        DOWNLINK_SKIPPED_BY_TOMBSTONE_TOTAL.load(Relaxed)
    )
}

/// 测试专用：复位两个时间戳（生产路径不调用；计数器本身单调不减，只按差值断言）。
#[cfg(test)]
pub(crate) fn reset_timestamps_for_tests() {
    FIRST_ENTER_MS.store(-1, Relaxed);
    LAST_ENTER_MS.store(-1, Relaxed);
}

/// 测试专用：清空「已告警键」与按秒聚合的状态（生产路径不调用；累计计数只按差值断言）。
#[cfg(test)]
pub(crate) fn reset_dead_socket_state_for_tests() {
    dead_socket_warned()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    DEAD_SOCKET_SUPPRESSED.store(0, Relaxed);
    DEAD_SOCKET_LAST_AGG_MS.store(i64::MIN, Relaxed);
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
            "drain_events_drained=",
            "drain_budget_hit=",
            "reply_table_max=",
            "downlink_dead_socket_total=",
            "downlink_skipped_by_tombstone_total=",
        ] {
            assert!(line.contains(key), "汇总行缺字段 {key}: {line}");
        }
    }

    /// 墓碑跳过计数必须落进 `ENTRY_TELEMETRY` 汇总行（窗口对账读最后一条该行）。
    /// 变异：把 `downlink_skipped_by_tombstone_total={}` 从 `summary_line` 的格式串删掉 ⇒ 本用例红。
    #[test]
    fn skipped_by_tombstone_counter_lands_in_summary_line() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = downlink_skipped_by_tombstone_total();
        let after = record_downlink_skipped_by_tombstone();
        assert!(
            after > before,
            "墓碑命中必须逐次 +1（before={before} after={after}）"
        );
        let line = summary_line();
        let needle = format!(
            "downlink_skipped_by_tombstone_total={}",
            downlink_skipped_by_tombstone_total()
        );
        assert!(line.contains(&needle), "汇总行缺/错值：{needle} @ {line}");
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

    /// 顶部积压读数 `reply_table_max`：与 `reply_pending_max` 同构（历史最大值、>0 才打印），
    /// 但**两个计数器互不串**——更新 table 不碰 pending，反之亦然。
    /// 退化形态：0 不打印且不清零历史峰值。
    #[test]
    fn reply_table_tracks_its_own_max_and_does_not_alias_pending() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        REPLY_TABLE_MAX.store(0, Ordering::Relaxed);
        REPLY_PENDING_MAX.store(0, Ordering::Relaxed);
        assert!(record_reply_table(0).is_none(), "0 不应打印");
        assert!(record_reply_table(5).is_some(), ">0 应打印");
        assert!(record_reply_table(3).is_some(), ">0 应打印");
        assert_eq!(
            REPLY_TABLE_MAX.load(Ordering::Relaxed),
            5,
            "必须保留历史最大值，不得回落"
        );
        assert_eq!(
            REPLY_PENDING_MAX.load(Ordering::Relaxed),
            0,
            "更新 table 不得碰 pending（串源 ⇒ 本用例红）"
        );
        assert!(record_reply_pending(9).is_some(), ">0 应打印");
        assert_eq!(REPLY_PENDING_MAX.load(Ordering::Relaxed), 9);
        assert_eq!(
            REPLY_TABLE_MAX.load(Ordering::Relaxed),
            5,
            "更新 pending 不得碰 table（串源 ⇒ 本用例红）"
        );
    }

    /// **事件排水两个计数**（卡 `openmir2-tick-starvation`）：
    /// - 退化形态「空集」（0 次调用）：两个计数都不变、不打印；
    /// - `drain_events_drained` 每次调用严格 +1，且从不打印（打印由汇总行承担）；
    /// - `drain_budget_hit` 按 [`PRINT_EVERY`] 节流：任意连续 `PRINT_EVERY` 次里恰好一行
    ///   （与起始相位无关），即预算每轮触达也不会逐轮刷屏。
    #[test]
    fn drain_counters_count_and_throttle() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 退化形态：不调用 ⇒ 读数不变（任何「初始化即非零」的接线都会在这里露馅）。
        let before_drained = DRAIN_EVENTS_DRAINED.load(Ordering::Relaxed);
        let before_hit = DRAIN_BUDGET_HIT.load(Ordering::Relaxed);
        assert_eq!(DRAIN_EVENTS_DRAINED.load(Ordering::Relaxed), before_drained);
        assert_eq!(DRAIN_BUDGET_HIT.load(Ordering::Relaxed), before_hit);

        // 单条 +1（不打印：函数返回 ()，没有日志出口）。
        record_drain_event_drained();
        assert_eq!(
            DRAIN_EVENTS_DRAINED.load(Ordering::Relaxed),
            before_drained + 1,
            "每取走一条必须 +1"
        );

        // 节流：连续 PRINT_EVERY 次里恰好一次返回汇总行。
        let mut printed = 0;
        for _ in 0..PRINT_EVERY {
            if record_drain_budget_hit().is_some() {
                printed += 1;
            }
        }
        assert_eq!(printed, 1, "每 {PRINT_EVERY} 次截断只许一行");
        assert_eq!(
            DRAIN_BUDGET_HIT.load(Ordering::Relaxed),
            before_hit + PRINT_EVERY,
            "每次截断都必须计数（打印可以节流，计数不能）"
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
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_final_line_once_for_tests();
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

    /// **唯一记账出口**：终值行生成时若仍有残留，必须把快照写进 `reply_dropped_at_shutdown`，
    /// 且终值行带该字段（删记账 ⇒ 红；把记账移到某一条路径上 ⇒ 其它路径的用例红）。
    #[test]
    fn final_line_accounts_leftover_replies() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_final_line_once_for_tests();
        REPLY_STORED.store(3, Ordering::Relaxed);
        REPLY_CONSUMED.store(0, Ordering::Relaxed);
        REPLY_DROPPED_AT_SHUTDOWN.store(0, Ordering::Relaxed);
        let line = final_line_once().expect("终值行");
        assert!(
            line.contains("reply_dropped_at_shutdown=3"),
            "终值行必须带残留记账字段: {line}"
        );
        REPLY_STORED.store(0, Ordering::Relaxed);
        REPLY_CONSUMED.store(0, Ordering::Relaxed);
        REPLY_DROPPED_AT_SHUTDOWN.store(0, Ordering::Relaxed);
        reset_final_line_once_for_tests();
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

    /// 卡 `openmir2-missing-socket-warn-storm` 牙①：**同一死槽连发 10,000 帧** ⇒
    /// 生产路径只在该函数返回 `true` 时打 WRN ⇒ **恰 1 条 WRN**；
    /// 计数**恰好 +10,000**（节流只压日志，不能压计数）。
    ///
    /// 退化形态（空输入）：先断言 0 次调用不改计数，再开始循环（本用例内也覆盖「空集」）。
    /// 变异：删掉 `HashSet::insert` 的返回值判断（恒 `true`）⇒ `firsts == 1` 红；
    /// 把计数改成只在 `true` 时 +1 ⇒ `before + 10_000` 红。
    #[test]
    fn dead_socket_warns_once_but_counts_every_drop() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_dead_socket_state_for_tests();
        let before = DOWNLINK_DEAD_SOCKET_TOTAL.load(Ordering::Relaxed);
        // 空集形态：一次落空都没有 ⇒ 读数不变。
        assert_eq!(DOWNLINK_DEAD_SOCKET_TOTAL.load(Ordering::Relaxed), before);
        let mut firsts = 0u64;
        for _ in 0..10_000 {
            if record_downlink_dead_socket(3, 77) {
                firsts += 1;
            }
        }
        assert_eq!(
            firsts, 1,
            "同一 (网关线号, socket) 只许首告警一次（10,000 帧必须仍是 1 条 WRN）"
        );
        assert_eq!(
            DOWNLINK_DEAD_SOCKET_TOTAL.load(Ordering::Relaxed),
            before + 10_000,
            "计数必须逐帧 +1，不受告警节流影响"
        );
    }

    /// 牙②：**10 个不同死槽各 1 帧** ⇒ 10 条 WRN、计数 +10（不是全局只告警一次）；
    /// 且键含**网关线号**——另一条网关线上的同号 socket 是新会话，必须各自告警。
    #[test]
    fn distinct_dead_sockets_each_warn_once_and_key_includes_gate_line() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_dead_socket_state_for_tests();
        let before = DOWNLINK_DEAD_SOCKET_TOTAL.load(Ordering::Relaxed);
        let mut firsts = 0u64;
        for socket in 0..10 {
            if record_downlink_dead_socket(0, 1_000 + socket) {
                firsts += 1;
            }
        }
        assert_eq!(firsts, 10, "10 个不同死槽必须各告警一次");
        assert_eq!(
            DOWNLINK_DEAD_SOCKET_TOTAL.load(Ordering::Relaxed),
            before + 10
        );
        assert!(
            record_downlink_dead_socket(1, 1_000),
            "键必须含网关线号：另一条线的同号 socket 是新会话，仍须首告警"
        );
        assert_eq!(
            DOWNLINK_DEAD_SOCKET_TOTAL.load(Ordering::Relaxed),
            before + 11
        );
    }

    /// 后续落空**按秒聚合**：1s 内只出 1 行（行内带被抑制条数），满 1s 再出 1 行；
    /// 聚合行是独立 `ENTRY_TELEMETRY` 行，不冒充 WRN（牙①的「恰 1 条 WRN」不受影响）。
    #[test]
    fn suppressed_dead_sockets_aggregate_at_most_once_per_second() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_dead_socket_state_for_tests();
        // 首次告警不计入抑制量。
        assert!(record_downlink_dead_socket(0, 5));
        let first = aggregate_downlink_dead_socket(10_000).expect("首次聚合行");
        assert!(
            first.contains("suppressed=1"),
            "首行带本窗口被抑制条数: {first}"
        );
        for i in 1..1_000 {
            assert!(
                aggregate_downlink_dead_socket(10_000 + i).is_none(),
                "1s 窗口内不得再出行（i={i}）"
            );
        }
        // 窗口内共 1 + 999 次抑制调用；满 1s 的这一次触发第二行，swap 前已含本次。
        let next = aggregate_downlink_dead_socket(11_001).expect("满 1s 必须出行");
        assert!(
            next.contains("suppressed=1000"),
            "第二行必须带整秒窗口的抑制条数: {next}"
        );
    }

    /// 卡 `openmir2-missing-socket-warn-storm` P1 返工牙：**聚合行不得顶掉终值行**。
    ///
    /// 项目口径「日志里最后一条 `ENTRY_TELEMETRY` 行 = 终值」（舰队归档/停止脚本与窗口复核
    /// 都取它）。构造「先打汇总行、再触发一次聚合行」的日志序列，按同一口径取最后一条
    /// `ENTRY_TELEMETRY` 行：必须仍是汇总行（带 `logon_frames_seen=`），聚合行只能有自己的
    /// 独立前缀。
    ///
    /// 变异：把聚合行前缀改回 `ENTRY_TELEMETRY` ⇒ 最后一条 `ENTRY_TELEMETRY` 行变成聚合行，
    /// `logon_frames_seen=` 缺失 ⇒ 本用例红。
    #[test]
    fn aggregate_line_does_not_shadow_last_entry_telemetry_line() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_dead_socket_state_for_tests();
        let summary = summary_line();
        assert!(
            summary.starts_with("ENTRY_TELEMETRY svc=game-svr "),
            "前提：汇总行必须以 ENTRY_TELEMETRY 开头: {summary}"
        );
        // 风暴发生在窗口末段：汇总行之后紧跟聚合行是常态。
        let agg = aggregate_downlink_dead_socket(10_000).expect("首次聚合行");
        assert!(
            !agg.starts_with("ENTRY_TELEMETRY"),
            "聚合行不得复用 ENTRY_TELEMETRY 前缀（会顶掉终值行）: {agg}"
        );
        assert!(
            agg.starts_with("DOWNLINK_DEAD_SOCKET_AGG "),
            "聚合行必须用独立前缀: {agg}"
        );
        // 按舰队口径取「最后一条 ENTRY_TELEMETRY 行」。
        let lines = [summary, agg];
        let last_entry_line = lines
            .iter()
            .rev()
            .find(|l| l.starts_with("ENTRY_TELEMETRY"))
            .expect("日志里必须有 ENTRY_TELEMETRY 终值行");
        assert!(
            last_entry_line.contains("logon_frames_seen="),
            "最后一条 ENTRY_TELEMETRY 行必须仍是汇总终值（聚合行顶掉 ⇒ 缺 logon_frames_seen=）: {last_entry_line}"
        );
    }
}
