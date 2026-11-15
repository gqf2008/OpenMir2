//! 卡 `openmir2-gateway-backpressure-frame-drop`：网关背压的**策略 + 观测层**（总监裁决 A+B）。
//!
//! oracle `ChannelMessageHandler.ProcessBufferSend` 在两个分支**直接 return**、不留任何计数/日志：
//! ① `SendChecked > 0`（等网关回 `GM_RECEIVE_OK`，`:58-66`）——包括「刚超时清零、这一包仍压住」；
//! ② 单包就超 `CheckBlock*10` 且累计为 0（`:76-79`）。`game-svr` 的 `send_decision`/`send_gate_msg`
//! 逐条照搬了这两个丢帧点 ⇒ 高负载下是**静默丢包**（2026-11-15 03:00 窗口：716 条 SM_LOGON 里
//! 330 Send / 386 Hold，bot 侧 `in_world=330` 与 Send 数完全相等）。
//!
//! 总监裁决取 **A+B 混合**（本模块所在的这批实现）：
//!
//! - **A**：`SendChecked > 0` 的帧不再丢，按 FIFO 进 per-gate 有界队列（[`gate_queue`](crate::gate_queue)，
//!   默认 256KiB/4096 帧），回显/超时清零后**按序补发**。
//! - **B**：队列满 ⇒ **拒绝**该帧：计数 + 结构化日志 + 对该会话发 `GM_CLOSE`（客户端可见断开），
//!   不是静默丢（见 [`DropStats::note_overflow`] 与 `send_gate_msg` 的 overflow 分支）。
//! - **仍保持 oracle 的另一个丢点**：单包就超 `CheckBlock*10`（`:76-79`）——这种帧永远无法在
//!   流控前提下发出去，照 oracle 丢弃，但**必须计数 + 日志**（[`DropStats::note_oversize`]）。
//!
//! ## 顺序硬约束（总监裁决，实现见 `lib.rs::send_gate_msg`/`flush_pending`）
//!
//! **flush 只在 `SendChecked` 清零后、任何新帧直发之前完成**；否则共享 FIFO 会让同一会话的
//! `SM_SERVERUSERINDEX → SM_LOGON` 乱序——**乱序比丢帧更难查，因为不报错**。本模块只负责
//! 计数与日志，顺序由 `lib.rs` 的入队/补发路径保证。
//!
//! ## 曝光通道
//!
//! - **入队日志**（默认开）：`WRN`、每条 gate 线最多每 [`LOG_INTERVAL_MS`] 一条，
//!   `event=gate_frame_queued gate=… reason=SendChecked>0 ident=… session=… depth_frames=…`。
//! - **溢出日志**（默认开，独立限速）：`event=gate_frame_overflow gate=… reason=QueueFull
//!   ident=… session=… close_sent=…`——A5 归因「谁被拒了」看这条。
//! - **单包超阈值日志**（默认开，独立限速）：`event=gate_frame_drop reason=Oversize …`。
//! - **周期汇总**（默认开）：同一条 gate 线最早每 [`SUMMARY_INTERVAL_MS`] 在**有事件时**一条
//!   `event=gate_backpressure_summary`，含累计 queued/flushed/overflow/oversize 与队列高水位。
//! - **NDJSON**（仅 `MIR2_GATE_STATS_OUT=<dir>` 时）：汇总追加写 `<dir>/gate_stats.ndjson`，
//!   **累计值**——与 BotSrv `load_stats.ndjson`（`src/BotSrv/LoadMetrics.cs`）同口径：相邻两行
//!   差分即得窗口增量。行内含 `wall_ms`（绝对时钟）便于与窗口归档对齐。未设环境变量 ⇒ 不碰
//!   文件系统（生产默认零副作用）。
//!
//! 直方图键有上限（[`MAX_IDENT_KEYS`] / [`MAX_SESSION_KEYS`]），超出进 overflow 计数，防异常输入
//! 把内存撑爆。直方图统计的是**被拒绝的帧**（overflow + oversize）——即 A5 要归因的失败面；
//! 逐帧日志才是逐帧归因通道（reviewer F2 的口径）。

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// 统计输出目录（NDJSON）。未设 ⇒ 只走日志，不写文件。
pub(crate) const ENV_STATS_OUT: &str = "MIR2_GATE_STATS_OUT";

/// 同一条 gate 线的单类日志最小间隔（1s）。三类事件（queued/overflow/oversize）各自限速。
pub(crate) const LOG_INTERVAL_MS: i64 = 1_000;

/// 同一条 gate 线的汇总最小间隔（5s；与 BotSrv `load_stats.ndjson` 的 5s 采样同节奏）。
pub(crate) const SUMMARY_INTERVAL_MS: i64 = 5_000;

/// ident 直方图键上限（超出计入 `ident_overflow`）。
pub(crate) const MAX_IDENT_KEYS: usize = 48;

/// session 直方图键上限（超出计入 `session_overflow`）。
pub(crate) const MAX_SESSION_KEYS: usize = 48;

/// 已发 `GM_CLOSE` 的会话集合上限（去重用的内存上界；超出后仍发、只是不再去重）。
pub(crate) const MAX_CLOSED_SESSIONS: usize = 4096;

/// 汇总行里 top 列表的长度。
pub(crate) const TOP_N: usize = 5;

/// 溢出时对会话发 `GM_CLOSE` 的结果（落日志，便于证明「明确失败」真的发生）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseOutcome {
    /// 已写出 `GM_CLOSE`（客户端将看到断开）。
    Sent,
    /// 写 `GM_CLOSE` 失败（链路已坏；连接随后会被读错误收掉）。
    Failed,
    /// 该会话本连接内已发过 `GM_CLOSE`（去重）。
    Duplicate,
    /// 帧没有会话号（控制帧），无可关闭对象。
    NoSession,
}

impl CloseOutcome {
    fn sent(self) -> u8 {
        u8::from(self == CloseOutcome::Sent)
    }

    fn failed(self) -> u8 {
        u8::from(self == CloseOutcome::Failed)
    }

    fn as_str(self) -> &'static str {
        match self {
            CloseOutcome::Sent => "sent",
            CloseOutcome::Failed => "failed",
            CloseOutcome::Duplicate => "duplicate",
            CloseOutcome::NoSession => "no_session",
        }
    }
}

/// 一次记账产生的待发日志（由调用方决定落到哪个日志通道）。
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Note {
    /// 限速后的逐帧日志（`WRN`）。
    pub(crate) log: Option<String>,
    /// 周期汇总日志（`WRN`）。
    pub(crate) summary: Option<String>,
    /// 汇总的 NDJSON 形态（仅当配了 [`ENV_STATS_OUT`] 才写文件）。
    pub(crate) summary_json: Option<String>,
}

/// 一条 gate 线（= `service_id` / oracle `GateIdx`）的背压计数。
///
/// 口径：
/// - `hold_frames` = 进入 oracle `SendChecked>0` 分支的帧数（= queued + queued_disabled + overflow）；
/// - `queued_frames` / `flushed_frames` = 入队成功 / 补发成功；
/// - `overflow_frames` = 队列满被拒绝（B）；`oversize_frames` = oracle `:76-79` 的单包超阈值丢弃；
/// - 直方图只统计 `overflow + oversize`（失败面归因）。
#[derive(Debug, Default)]
pub(crate) struct DropStats {
    gate: u8,
    start_ms: i64,
    queue_enabled: bool,
    queue_max_bytes: usize,
    queue_max_frames: usize,
    hold_frames: u64,
    hold_bytes: u64,
    queued_frames: u64,
    queued_bytes: u64,
    queue_disabled_frames: u64,
    flushed_frames: u64,
    flushed_bytes: u64,
    flush_waits: u64,
    timeout_flushes: u64,
    overflow_frames: u64,
    overflow_bytes: u64,
    rejected_sessions: u64,
    close_failures: u64,
    max_depth_frames: usize,
    max_depth_bytes: usize,
    oversize_frames: u64,
    oversize_bytes: u64,
    by_ident: HashMap<u16, u64>,
    ident_overflow: u64,
    by_session: HashMap<u16, u64>,
    session_overflow: u64,
    closed_sessions: HashSet<u16>,
    last_queued_log_ms: Option<i64>,
    queued_suppressed: u64,
    last_overflow_log_ms: Option<i64>,
    overflow_suppressed: u64,
    last_oversize_log_ms: Option<i64>,
    oversize_suppressed: u64,
    last_summary_ms: Option<i64>,
}

impl DropStats {
    /// 新建（`start_ms` 用于日志/NDJSON 的相对时间基线）。
    pub(crate) fn new(gate: u8, start_ms: i64) -> Self {
        Self {
            gate,
            start_ms,
            ..Self::default()
        }
    }

    /// 记录本连接队列的容量口径（NDJSON/汇总里带上，便于 A5 区分配置）。
    pub(crate) fn set_queue_limits(&mut self, enabled: bool, max_bytes: usize, max_frames: usize) {
        self.queue_enabled = enabled;
        self.queue_max_bytes = max_bytes;
        self.queue_max_frames = max_frames;
    }

    /// 累计进入 `SendChecked>0` 分支的帧数（只给判据/测试读）。
    #[cfg(test)]
    pub(crate) fn hold_frames(&self) -> u64 {
        self.hold_frames
    }

    /// 入队成功帧数（只给判据/测试读）。
    #[cfg(test)]
    pub(crate) fn queued_frames(&self) -> u64 {
        self.queued_frames
    }

    /// 补发成功帧数（只给判据/测试读）。
    #[cfg(test)]
    pub(crate) fn flushed_frames(&self) -> u64 {
        self.flushed_frames
    }

    /// 队列满被拒绝的帧数（只给判据/测试读）。
    #[cfg(test)]
    pub(crate) fn overflow_frames(&self) -> u64 {
        self.overflow_frames
    }

    /// 单包超阈值丢弃帧数（只给判据/测试读）。
    #[cfg(test)]
    pub(crate) fn oversize_frames(&self) -> u64 {
        self.oversize_frames
    }

    /// 超时触发的补发次数（只给判据/测试读）。
    #[cfg(test)]
    pub(crate) fn timeout_flushes(&self) -> u64 {
        self.timeout_flushes
    }

    /// 已发 `GM_CLOSE` 的会话数（只给判据/测试读）。
    #[cfg(test)]
    pub(crate) fn rejected_sessions(&self) -> u64 {
        self.rejected_sessions
    }

    /// 首次为某会话发 `GM_CLOSE` 时返回 true（去重；集合上限 [`MAX_CLOSED_SESSIONS`]，
    /// 超出后仍返回 true 让调用方继续发——明确失败优先于去重内存）。
    pub(crate) fn claim_session_close(&mut self, session: u16) -> bool {
        if self.closed_sessions.contains(&session) {
            return false;
        }
        if self.closed_sessions.len() < MAX_CLOSED_SESSIONS {
            self.closed_sessions.insert(session);
        }
        true
    }

    /// 记录一次 **A：入队成功**。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn note_queued(
        &mut self,
        ident: u16,
        session: u16,
        bytes: usize,
        depth_frames: usize,
        depth_bytes: usize,
        send_block_count: usize,
        now: i64,
    ) -> Note {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        self.hold_frames += 1;
        self.hold_bytes = self.hold_bytes.saturating_add(bytes);
        self.queued_frames += 1;
        self.queued_bytes = self.queued_bytes.saturating_add(bytes);
        self.max_depth_frames = self.max_depth_frames.max(depth_frames);
        self.max_depth_bytes = self.max_depth_bytes.max(depth_bytes);

        let mut out = Note::default();
        let due = due(self.last_queued_log_ms, now);
        if due {
            let mut line = String::new();
            let _ = write!(
                line,
                "event=gate_frame_queued gate={} reason=SendChecked>0 ident={ident} session={session} \
                 bytes={bytes} depth_frames={depth_frames} depth_bytes={depth_bytes} \
                 queue_max_bytes={} queue_max_frames={} send_block_count={send_block_count} \
                 suppressed_since_last={} hold_frames={} queued_frames={} overflow_frames={} uptime_ms={}",
                self.gate,
                self.queue_max_bytes,
                self.queue_max_frames,
                self.queued_suppressed,
                self.hold_frames,
                self.queued_frames,
                self.overflow_frames,
                now.saturating_sub(self.start_ms),
            );
            out.log = Some(line);
            self.last_queued_log_ms = Some(now);
            self.queued_suppressed = 0;
        } else {
            self.queued_suppressed += 1;
        }
        self.summarize(&mut out, now);
        out
    }

    /// 记录一次 **回退开关禁用**（`MIR2_GATE_SEND_QUEUE=0`）：按 oracle 直接丢，但**不静默**。
    pub(crate) fn note_dropped_hold(
        &mut self,
        ident: u16,
        session: u16,
        bytes: usize,
        now: i64,
    ) -> Note {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        self.hold_frames += 1;
        self.hold_bytes = self.hold_bytes.saturating_add(bytes);
        self.queue_disabled_frames += 1;
        bump(
            &mut self.by_ident,
            &mut self.ident_overflow,
            ident,
            MAX_IDENT_KEYS,
        );
        bump(
            &mut self.by_session,
            &mut self.session_overflow,
            session,
            MAX_SESSION_KEYS,
        );

        let mut out = Note::default();
        let due = due(self.last_oversize_log_ms, now);
        if due {
            let mut line = String::new();
            let _ = write!(
                line,
                "event=gate_frame_drop gate={} reason=SendChecked>0 queue=disabled ident={ident} \
                 session={session} bytes={bytes} suppressed_since_last={} hold_frames={} \
                 queue_disabled_frames={} uptime_ms={}",
                self.gate,
                self.oversize_suppressed,
                self.hold_frames,
                self.queue_disabled_frames,
                now.saturating_sub(self.start_ms),
            );
            out.log = Some(line);
            self.last_oversize_log_ms = Some(now);
            self.oversize_suppressed = 0;
        } else {
            self.oversize_suppressed += 1;
        }
        self.summarize(&mut out, now);
        out
    }

    /// 记录一次 **B：队列满拒绝**（计数 + 明确失败的结果）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn note_overflow(
        &mut self,
        ident: u16,
        session: u16,
        bytes: usize,
        depth_frames: usize,
        depth_bytes: usize,
        close: CloseOutcome,
        now: i64,
    ) -> Note {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        self.hold_frames += 1;
        self.hold_bytes = self.hold_bytes.saturating_add(bytes);
        self.overflow_frames += 1;
        self.overflow_bytes = self.overflow_bytes.saturating_add(bytes);
        self.max_depth_frames = self.max_depth_frames.max(depth_frames);
        self.max_depth_bytes = self.max_depth_bytes.max(depth_bytes);
        if close == CloseOutcome::Sent {
            self.rejected_sessions += 1;
        } else if close == CloseOutcome::Failed {
            self.close_failures += 1;
        }
        bump(
            &mut self.by_ident,
            &mut self.ident_overflow,
            ident,
            MAX_IDENT_KEYS,
        );
        bump(
            &mut self.by_session,
            &mut self.session_overflow,
            session,
            MAX_SESSION_KEYS,
        );

        let mut out = Note::default();
        let due = due(self.last_overflow_log_ms, now);
        if due {
            let mut line = String::new();
            let _ = write!(
                line,
                "event=gate_frame_overflow gate={} reason=QueueFull ident={ident} session={session} \
                 bytes={bytes} depth_frames={depth_frames} depth_bytes={depth_bytes} \
                 queue_max_bytes={} queue_max_frames={} close={} close_sent={} close_failed={} \
                 suppressed_since_last={} overflow_frames={} overflow_bytes={} rejected_sessions={} \
                 uptime_ms={}",
                self.gate,
                self.queue_max_bytes,
                self.queue_max_frames,
                close.as_str(),
                close.sent(),
                close.failed(),
                self.overflow_suppressed,
                self.overflow_frames,
                self.overflow_bytes,
                self.rejected_sessions,
                now.saturating_sub(self.start_ms),
            );
            out.log = Some(line);
            self.last_overflow_log_ms = Some(now);
            self.overflow_suppressed = 0;
        } else {
            self.overflow_suppressed += 1;
        }
        self.summarize(&mut out, now);
        out
    }

    /// 记录一次 **oracle 单包超阈值丢弃**（`:76-79`；保持 oracle 语义，只加观测）。
    pub(crate) fn note_oversize(
        &mut self,
        ident: u16,
        session: u16,
        bytes: usize,
        now: i64,
    ) -> Note {
        let bytes_u64 = u64::try_from(bytes).unwrap_or(u64::MAX);
        self.oversize_frames += 1;
        self.oversize_bytes = self.oversize_bytes.saturating_add(bytes_u64);
        bump(
            &mut self.by_ident,
            &mut self.ident_overflow,
            ident,
            MAX_IDENT_KEYS,
        );
        bump(
            &mut self.by_session,
            &mut self.session_overflow,
            session,
            MAX_SESSION_KEYS,
        );

        let mut out = Note::default();
        let due = due(self.last_oversize_log_ms, now);
        if due {
            let mut line = String::new();
            let _ = write!(
                line,
                "event=gate_frame_drop gate={} reason=Oversize ident={ident} session={session} \
                 bytes={bytes} suppressed_since_last={} oversize_frames={} oversize_bytes={} \
                 uptime_ms={}",
                self.gate,
                self.oversize_suppressed,
                self.oversize_frames,
                self.oversize_bytes,
                now.saturating_sub(self.start_ms),
            );
            out.log = Some(line);
            self.last_oversize_log_ms = Some(now);
            self.oversize_suppressed = 0;
        } else {
            self.oversize_suppressed += 1;
        }
        self.summarize(&mut out, now);
        out
    }

    /// 记录一次补发：`waited=true` 表示这一帧发完后撞了阈值、已发 `GM_RECEIVE_OK` 并等待回显。
    pub(crate) fn note_flushed(&mut self, bytes: usize, waited: bool) {
        self.flushed_frames += 1;
        self.flushed_bytes = self
            .flushed_bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        if waited {
            self.flush_waits += 1;
        }
    }

    /// 记录一次「因 50ms 超时（网关未回显）而清零并补发」。
    pub(crate) fn note_timeout_flush(&mut self) {
        self.timeout_flushes += 1;
    }

    /// 是否发生过任何值得落终值汇总的事件。
    fn has_activity(&self) -> bool {
        self.hold_frames > 0 || self.oversize_frames > 0
    }

    fn summarize(&mut self, out: &mut Note, now: i64) {
        if !due_after(self.last_summary_ms, now, SUMMARY_INTERVAL_MS) {
            return;
        }
        out.summary = Some(self.format_summary(now));
        out.summary_json = Some(self.format_summary_json(now));
        self.last_summary_ms = Some(now);
    }

    /// 连接结束时补一条汇总（有事件才产出）：避免「最后 5s 内的事件只在限速日志里、NDJSON 没落」。
    pub(crate) fn final_summary(&self, now: i64) -> Option<String> {
        self.has_activity().then(|| self.format_summary(now))
    }

    /// 连接结束时的 NDJSON 终值（写文件用；调用方负责环境变量与 I/O）。
    pub(crate) fn final_summary_json(&self, now: i64) -> Option<String> {
        self.has_activity().then(|| self.format_summary_json(now))
    }

    fn format_summary(&self, now: i64) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "event=gate_backpressure_summary gate={} queue_enabled={} queue_max_bytes={} \
             queue_max_frames={} hold_frames={} hold_bytes={} queued_frames={} queued_bytes={} \
             queue_disabled_frames={} flushed_frames={} flushed_bytes={} flush_waits={} \
             timeout_flushes={} overflow_frames={} overflow_bytes={} oversize_frames={} \
             oversize_bytes={} rejected_sessions={} close_failures={} max_depth_frames={} \
             max_depth_bytes={} top_idents={} top_sessions={} overflow_idents={} \
             overflow_sessions={} uptime_ms={}",
            self.gate,
            u8::from(self.queue_enabled),
            self.queue_max_bytes,
            self.queue_max_frames,
            self.hold_frames,
            self.hold_bytes,
            self.queued_frames,
            self.queued_bytes,
            self.queue_disabled_frames,
            self.flushed_frames,
            self.flushed_bytes,
            self.flush_waits,
            self.timeout_flushes,
            self.overflow_frames,
            self.overflow_bytes,
            self.oversize_frames,
            self.oversize_bytes,
            self.rejected_sessions,
            self.close_failures,
            self.max_depth_frames,
            self.max_depth_bytes,
            top_pairs(&self.by_ident),
            top_pairs(&self.by_session),
            self.ident_overflow,
            self.session_overflow,
            now.saturating_sub(self.start_ms),
        );
        out
    }

    fn format_summary_json(&self, now: i64) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"wall_ms\":{now},\"t_ms\":{},\"gate\":{},\"queue_enabled\":{},\
             \"queue_max_bytes\":{},\"queue_max_frames\":{},\"hold_frames\":{},\"hold_bytes\":{},\
             \"queued_frames\":{},\"queued_bytes\":{},\"queue_disabled_frames\":{},\
             \"flushed_frames\":{},\"flushed_bytes\":{},\"flush_waits\":{},\"timeout_flushes\":{},\
             \"overflow_frames\":{},\"overflow_bytes\":{},\"oversize_frames\":{},\
             \"oversize_bytes\":{},\"rejected_sessions\":{},\"close_failures\":{},\
             \"max_depth_frames\":{},\"max_depth_bytes\":{},\"top_idents\":\"{}\",\
             \"top_sessions\":\"{}\",\"overflow_idents\":{},\"overflow_sessions\":{}}}",
            now.saturating_sub(self.start_ms),
            self.gate,
            u8::from(self.queue_enabled),
            self.queue_max_bytes,
            self.queue_max_frames,
            self.hold_frames,
            self.hold_bytes,
            self.queued_frames,
            self.queued_bytes,
            self.queue_disabled_frames,
            self.flushed_frames,
            self.flushed_bytes,
            self.flush_waits,
            self.timeout_flushes,
            self.overflow_frames,
            self.overflow_bytes,
            self.oversize_frames,
            self.oversize_bytes,
            self.rejected_sessions,
            self.close_failures,
            self.max_depth_frames,
            self.max_depth_bytes,
            top_pairs(&self.by_ident),
            top_pairs(&self.by_session),
            self.ident_overflow,
            self.session_overflow,
        );
        out
    }
}

/// 限速判定：首次或距上次 ≥ [`LOG_INTERVAL_MS`]。
fn due(last: Option<i64>, now: i64) -> bool {
    due_after(last, now, LOG_INTERVAL_MS)
}

/// 通用的「首次或距上次 ≥ `interval_ms`」判定（汇总用 [`SUMMARY_INTERVAL_MS`]）。
fn due_after(last: Option<i64>, now: i64, interval_ms: i64) -> bool {
    last.is_none_or(|last| now.saturating_sub(last) >= interval_ms)
}

/// 直方图 +1；键数到上限后只记 overflow（与 BotSvr `LoadMetrics` 的键上限同思路）。
fn bump(map: &mut HashMap<u16, u64>, overflow: &mut u64, key: u16, cap: usize) {
    if let Some(count) = map.get_mut(&key) {
        *count += 1;
    } else if map.len() < cap {
        map.insert(key, 1);
    } else {
        *overflow += 1;
    }
}

/// `k1:c1,k2:c2,...`（按计数降序、键升序打破平局；最多 [`TOP_N`] 项）。
fn top_pairs(map: &HashMap<u16, u64>) -> String {
    let mut pairs: Vec<(u16, u64)> = map.iter().map(|(&k, &v)| (k, v)).collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut out = String::new();
    for (i, (key, count)) in pairs.iter().take(TOP_N).enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{key}:{count}");
    }
    out
}

/// [`ENV_STATS_OUT`] 指向的目录（仅字面非空；相对路径按进程 cwd 解释）。
pub(crate) fn stats_out_dir() -> Option<PathBuf> {
    std::env::var_os(ENV_STATS_OUT)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// 追加一行到 `<dir>/gate_stats.ndjson`（目录不存在会创建）。
pub(crate) fn append_ndjson(dir: &Path, line: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("gate_stats.ndjson"))?;
    writeln!(file, "{line}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> DropStats {
        let mut s = DropStats::new(3, 0);
        s.set_queue_limits(true, 256 * 1024, 4096);
        s
    }

    fn queued(s: &mut DropStats, ident: u16, session: u16, now: i64) -> Note {
        s.note_queued(ident, session, 120, 1, 120, 0, now)
    }

    /// 判据 A/A+（总监裁决）：`SendChecked>0` 的帧入队必须计数，并产出含 ident/session/深度
    /// 的结构化日志；`hold_frames = queued_frames`。删掉计数或日志分支 ⇒ 本用例必红。
    #[test]
    fn held_frame_is_queued_and_logged() {
        let mut s = stats();
        let note = queued(&mut s, 50, 42, 1_000);
        assert_eq!(s.hold_frames, 1, "Hold 必须计数");
        assert_eq!(s.queued_frames, 1, "入队必须计数");
        assert_eq!(s.queued_bytes, 120);
        assert_eq!(s.overflow_frames, 0);
        let log = note.log.expect("首次入队必须产出行");
        for needle in [
            "event=gate_frame_queued",
            "gate=3",
            "reason=SendChecked>0",
            "ident=50",
            "session=42",
            "depth_frames=1",
            "queue_max_bytes=262144",
            "hold_frames=1",
            "queued_frames=1",
        ] {
            assert!(log.contains(needle), "入队日志缺 {needle}：{log}");
        }
        let summary = note.summary.expect("首次事件必须产出汇总");
        assert!(summary.contains("event=gate_backpressure_summary"));
        assert!(summary.contains("queued_frames=1"));
        assert!(summary.contains("queue_enabled=1"));
    }

    /// 入队日志 1s 限速，但计数继续长；第 1s 的那条带 `suppressed_since_last`。
    #[test]
    fn queued_log_is_rate_limited_but_counters_grow() {
        let mut s = stats();
        assert!(queued(&mut s, 50, 1, 10_000).log.is_some());
        assert!(queued(&mut s, 50, 1, 10_010).log.is_none(), "1s 内必须限速");
        assert_eq!(s.queued_frames, 2);
        let log = queued(&mut s, 50, 1, 11_000)
            .log
            .expect("到 1s 必须再出一行");
        assert!(log.contains("suppressed_since_last=1"), "{log}");
        assert!(log.contains("queued_frames=3"));
        assert!(queued(&mut s, 50, 1, 11_001).log.is_none());
    }

    /// 判据 B（总监裁决）：队列满 ⇒ 拒绝帧计数 +1、日志 `reason=QueueFull`，
    /// 且 `close=sent` / `rejected_sessions=1`（明确失败发生了）。删掉该分支 ⇒ 本用例必红。
    #[test]
    fn overflow_is_counted_and_logged_with_close() {
        let mut s = stats();
        let note = s.note_overflow(51, 43, 900, 4096, 262_144, CloseOutcome::Sent, 2_000);
        assert_eq!(s.overflow_frames, 1);
        assert_eq!(s.overflow_bytes, 900);
        assert_eq!(s.rejected_sessions, 1);
        assert_eq!(s.queued_frames, 0);
        assert_eq!(s.by_ident.get(&51), Some(&1), "拒绝帧要按 ident 归因");
        assert_eq!(s.by_session.get(&43), Some(&1), "拒绝帧要按 session 归因");
        let log = note.log.expect("溢出必须立即出行（独立限速）");
        for needle in [
            "event=gate_frame_overflow",
            "reason=QueueFull",
            "ident=51",
            "session=43",
            "depth_frames=4096",
            "queue_max_bytes=262144",
            "close=sent",
            "close_sent=1",
            "overflow_frames=1",
            "rejected_sessions=1",
        ] {
            assert!(log.contains(needle), "溢出日志缺 {needle}：{log}");
        }
        assert_eq!(s.max_depth_frames, 4096);
        assert_eq!(s.max_depth_bytes, 262_144);
    }

    /// 溢出日志与入队日志**各自限速**：同一秒内先入队再溢出，溢出那条仍要出行（不允许被吞）。
    #[test]
    fn overflow_log_not_suppressed_by_queued_log() {
        let mut s = stats();
        assert!(queued(&mut s, 50, 1, 5_000).log.is_some());
        let log = s
            .note_overflow(50, 2, 10, 1, 10, CloseOutcome::Sent, 5_010)
            .log
            .expect("溢出日志不得被入队日志限速吞掉");
        assert!(log.contains("event=gate_frame_overflow"), "{log}");
    }

    /// `GM_CLOSE` 去重：同一会话只 true 一次；不同会话各 true。
    #[test]
    fn close_is_sent_once_per_session() {
        let mut s = stats();
        assert!(s.claim_session_close(42));
        assert!(!s.claim_session_close(42), "同一会话不得重复关闭");
        assert!(s.claim_session_close(43));
        assert_eq!(s.closed_sessions.len(), 2);
    }

    /// 回退开关路径：`Hold` 帧按 oracle 直接丢，但必须计数 + 日志（不静默）。
    #[test]
    fn disabled_queue_drop_is_still_observed() {
        let mut s = DropStats::new(9, 0);
        s.set_queue_limits(false, 0, 0);
        let note = s.note_dropped_hold(50, 7, 333, 1_000);
        assert_eq!(s.hold_frames, 1);
        assert_eq!(s.queue_disabled_frames, 1);
        assert_eq!(s.queued_frames, 0);
        let log = note.log.expect("回退路径也必须出行");
        assert!(log.contains("reason=SendChecked>0"), "{log}");
        assert!(log.contains("queue=disabled"), "{log}");
        assert!(log.contains("queue_disabled_frames=1"), "{log}");
        let summary = note.summary.unwrap();
        assert!(summary.contains("queue_enabled=0"), "{summary}");
    }

    /// oracle 单包超阈值丢点：单独计数、reason 不同、仍进失败直方图。
    #[test]
    fn oversize_drop_is_counted_separately() {
        let mut s = stats();
        let note = s.note_oversize(50, 7, 999, 0);
        assert_eq!(s.oversize_frames, 1);
        assert_eq!(s.hold_frames, 0);
        assert_eq!(s.oversize_bytes, 999);
        assert_eq!(s.by_ident.get(&50), Some(&1));
        let log = note.log.unwrap();
        assert!(log.contains("reason=Oversize"), "{log}");
        assert!(log.contains("ident=50"));
        assert!(log.contains("session=7"));
    }

    /// 补发计数：`flush_waits` 只在撞阈值等待回显时 +1；超时补发单独计数。
    #[test]
    fn flush_counters() {
        let mut s = stats();
        s.note_flushed(100, false);
        s.note_flushed(100, true);
        s.note_flushed(50, false);
        assert_eq!(s.flushed_frames, 3);
        assert_eq!(s.flushed_bytes, 250);
        assert_eq!(s.flush_waits, 1);
        s.note_timeout_flush();
        s.note_timeout_flush();
        assert_eq!(s.timeout_flushes, 2);
    }

    /// 退化输入：零字节 / ident=session=0 也必须计数并出行（不允许空集静默）。
    #[test]
    fn zero_sized_frame_is_still_observed() {
        let mut s = stats();
        let note = s.note_queued(0, 0, 0, 1, 0, 0, 0);
        assert_eq!(s.queued_frames, 1);
        let log = note.log.unwrap();
        assert!(log.contains("ident=0") && log.contains("session=0") && log.contains("bytes=0"));
    }

    /// 直方图键上限：第 49 个 ident / session 进 overflow，不无限长。
    #[test]
    fn histograms_are_bounded() {
        let mut s = stats();
        for i in 0..100u16 {
            let _ = s.note_overflow(i, u16::MAX - i, 1, 1, 1, CloseOutcome::Duplicate, 0);
        }
        assert_eq!(s.by_ident.len(), MAX_IDENT_KEYS);
        assert_eq!(
            s.ident_overflow,
            u64::from(100 - u16::try_from(MAX_IDENT_KEYS).unwrap())
        );
        assert_eq!(s.by_session.len(), MAX_SESSION_KEYS);
        assert_eq!(
            s.session_overflow,
            u64::from(100 - u16::try_from(MAX_SESSION_KEYS).unwrap())
        );
    }

    /// NDJSON：字段可被下行脚本按 JSON 解析（键名/数值形态），含绝对 `wall_ms` 与队列口径。
    /// 直方图只统计**被拒绝**的帧（overflow + oversize），故这里同时制造一次溢出。
    #[test]
    fn ndjson_summary_is_valid_json_and_appends() {
        let mut s = DropStats::new(4, 1_000);
        s.set_queue_limits(true, 262_144, 4096);
        let _ = queued(&mut s, 50, 9, 2_000);
        let _ = s.note_overflow(50, 9, 10, 1, 130, CloseOutcome::Sent, 2_001);
        let json = s.final_summary_json(6_000).expect("有事件就要有终值");
        for key in [
            "\"wall_ms\":6000",
            "\"t_ms\":5000",
            "\"gate\":4",
            "\"queue_enabled\":1",
            "\"queue_max_bytes\":262144",
            "\"hold_frames\":2",
            "\"queued_frames\":1",
            "\"flushed_frames\":0",
            "\"overflow_frames\":1",
            "\"rejected_sessions\":1",
            "\"top_idents\":\"50:1\"",
            "\"top_sessions\":\"9:1\"",
        ] {
            assert!(json.contains(key), "NDJSON 缺 {key}：{json}");
        }
        assert!(!json.contains(' '), "NDJSON 不许带空白：{json}");
        assert!(json.starts_with('{') && json.ends_with('}'));
        // 追加写：两行都能读回。
        let dir = std::env::temp_dir().join(format!(
            "gate-backpressure-ndjson-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        append_ndjson(&dir, &json).unwrap();
        append_ndjson(&dir, &json).unwrap();
        let text = std::fs::read_to_string(dir.join("gate_stats.ndjson")).unwrap();
        assert_eq!(text.lines().count(), 2, "两行累计值：{text}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 没有任何事件 ⇒ 不产出终值汇总（不制造噪声）。
    #[test]
    fn final_summary_is_absent_without_events() {
        let s = DropStats::new(0, 0);
        assert!(s.final_summary(10_000).is_none());
        assert!(s.final_summary_json(10_000).is_none());
    }
}
