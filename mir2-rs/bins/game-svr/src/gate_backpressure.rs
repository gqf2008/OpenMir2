//! 卡 `openmir2-gateway-backpressure-frame-drop`：网关背压丢帧的**可观测层**（丢帧行为不变）。
//!
//! oracle `ChannelMessageHandler.ProcessBufferSend` 在两个分支**直接 return**、不留任何计数/日志：
//! ① `SendChecked > 0`（等网关回 `GM_RECEIVE_OK`，`:58-66`）——包括「刚超时清零、这一包仍压住」；
//! ② 单包就超 `CheckBlock*10` 且累计为 0（`:76-79`）。`game-svr` 的 `send_decision`/`send_gate_msg`
//! 逐条照搬了这两个丢帧点 ⇒ 高负载下是**静默丢包**（2026-11-15 03:00 窗口：716 条 SM_LOGON 里
//! 330 Send / 386 Hold，bot 侧 `in_world=330` 与 Send 数完全相等）。
//!
//! 本模块**只观测**：计数（按 gate 线 / ident / session）、限速结构化日志、周期汇总 + 可选 NDJSON。
//! [`DropStats::note`] 不碰 `KeepAlive`、不改判定，只在 `send_decision` 已判出 `Hold`/`Drop` 之后调用
//! ⇒ 帧面与丢帧行为逐字节不变。
//!
//! ## 曝光通道
//!
//! - **丢帧日志**（默认开）：`WRN`、每条 gate 线最多每 [`LOG_INTERVAL_MS`] 一条，带被压掉的条数；
//!   形态 `event=gate_frame_drop gate=… reason=SendChecked>0 ident=… session=…`。
//! - **周期汇总**（默认开）：同一条 gate 线最早每 [`SUMMARY_INTERVAL_MS`] 在**发生丢帧时**一条
//!   `event=gate_backpressure_summary`，含累计 `hold_*/oversize_*` 与 top ident/session（有界）。
//! - **NDJSON**（仅 `MIR2_GATE_STATS_OUT=<dir>` 时）：汇总追加写 `<dir>/gate_stats.ndjson`，
//!   **累计值**——与 BotSrv `load_stats.ndjson`（`src/BotSrv/LoadMetrics.cs`）同口径：相邻两行做差分
//!   即得该窗口增量。未设环境变量 ⇒ 不碰文件系统（生产默认零副作用）。
//!
//! 直方图键有上限（[`MAX_IDENT_KEYS`] / [`MAX_SESSION_KEYS`]），超出进 overflow 计数，防异常输入
//! 把内存撑爆（同 BotSrv `LoadMetrics.MaxIdentKeys` 的做法）。

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// 统计输出目录（NDJSON）。未设 ⇒ 只走日志，不写文件。
pub(crate) const ENV_STATS_OUT: &str = "MIR2_GATE_STATS_OUT";

/// 同一条 gate 线的丢帧日志最小间隔（1s）。
pub(crate) const LOG_INTERVAL_MS: i64 = 1_000;

/// 同一条 gate 线的汇总最小间隔（5s；与 BotSrv `load_stats.ndjson` 的 5s 采样同节奏）。
pub(crate) const SUMMARY_INTERVAL_MS: i64 = 5_000;

/// ident 直方图键上限（超出计入 `ident_overflow`）。
pub(crate) const MAX_IDENT_KEYS: usize = 48;

/// session 直方图键上限（超出计入 `session_overflow`）。
pub(crate) const MAX_SESSION_KEYS: usize = 48;

/// 汇总行里 top 列表的长度。
pub(crate) const TOP_N: usize = 5;

/// 丢帧原因——与 oracle `ProcessBufferSend` 的两个 `return` 一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropReason {
    /// `SendChecked > 0`：等网关 `GM_RECEIVE_OK` 期间压住不发（`ChannelMessageHandler.cs:58-66`）。
    SendChecked,
    /// 单包就超 `CheckBlock*10` 且累计为 0（`:76-79`）。
    Oversize,
}

impl DropReason {
    /// 日志里的稳定字面量（判据按它 grep，不要随手改名）。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DropReason::SendChecked => "SendChecked>0",
            DropReason::Oversize => "Oversize",
        }
    }
}

/// 一次 [`DropStats::note`] 产生的待发日志（由调用方决定落到哪个日志通道）。
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Note {
    /// 限速后的逐帧丢帧日志（`WRN`）。
    pub(crate) log: Option<String>,
    /// 周期汇总日志（`WRN`）。
    pub(crate) summary: Option<String>,
    /// 汇总的 NDJSON 形态（仅当配了 [`ENV_STATS_OUT`] 才写文件）。
    pub(crate) summary_json: Option<String>,
}

/// 一条 gate 线（= `service_id` / oracle `GateIdx`）的丢帧观测计数。
#[derive(Debug, Default)]
pub(crate) struct DropStats {
    gate: u8,
    start_ms: i64,
    hold_frames: u64,
    hold_bytes: u64,
    oversize_frames: u64,
    oversize_bytes: u64,
    by_ident: HashMap<u16, u64>,
    ident_overflow: u64,
    by_session: HashMap<u16, u64>,
    session_overflow: u64,
    last_log_ms: Option<i64>,
    suppressed: u64,
    last_summary_ms: Option<i64>,
}

impl DropStats {
    /// 新建（`start_ms` 只用于 NDJSON 的 `t_ms` 基线）。
    pub(crate) fn new(gate: u8, start_ms: i64) -> Self {
        Self {
            gate,
            start_ms,
            ..Self::default()
        }
    }

    /// 累计 Hold 丢帧数（只给判据/测试读；生产走日志与 NDJSON）。
    #[cfg(test)]
    pub(crate) fn hold_frames(&self) -> u64 {
        self.hold_frames
    }

    /// 累计单包超阈值丢帧数（只给判据/测试读；生产走日志与 NDJSON）。
    #[cfg(test)]
    pub(crate) fn oversize_frames(&self) -> u64 {
        self.oversize_frames
    }

    /// 累计丢帧数（Hold + Oversize）。
    pub(crate) fn total_drops(&self) -> u64 {
        self.hold_frames + self.oversize_frames
    }

    /// 记录一次丢帧；到了限速窗口就产出日志/汇总文本（纯函数，不打印、不写文件）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn note(
        &mut self,
        reason: DropReason,
        ident: u16,
        session: u16,
        bytes: usize,
        send_checked_at_entry: i32,
        send_checked_now: i32,
        send_block_count: usize,
        now: i64,
    ) -> Note {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        match reason {
            DropReason::SendChecked => {
                self.hold_frames += 1;
                self.hold_bytes = self.hold_bytes.saturating_add(bytes);
            }
            DropReason::Oversize => {
                self.oversize_frames += 1;
                self.oversize_bytes = self.oversize_bytes.saturating_add(bytes);
            }
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
        let due = self
            .last_log_ms
            .is_none_or(|last| now.saturating_sub(last) >= LOG_INTERVAL_MS);
        if due {
            out.log = Some(self.format_drop(
                reason,
                ident,
                session,
                bytes,
                send_checked_at_entry,
                send_checked_now,
                send_block_count,
                now,
            ));
            self.last_log_ms = Some(now);
            self.suppressed = 0;
        } else {
            self.suppressed += 1;
        }
        let summary_due = self
            .last_summary_ms
            .is_none_or(|last| now.saturating_sub(last) >= SUMMARY_INTERVAL_MS);
        if summary_due {
            out.summary = Some(self.format_summary(now));
            out.summary_json = Some(self.format_summary_json(now));
            self.last_summary_ms = Some(now);
        }
        out
    }

    /// 连接结束时补一条汇总（有丢帧才产出）：避免「最后 5s 内的丢帧只在限速日志里、NDJSON 没落」。
    pub(crate) fn final_summary(&self, now: i64) -> Option<String> {
        (self.total_drops() > 0).then(|| self.format_summary(now))
    }

    /// 连接结束时的 NDJSON 终值（写文件用；调用方负责环境变量与 I/O）。
    pub(crate) fn final_summary_json(&self, now: i64) -> Option<String> {
        (self.total_drops() > 0).then(|| self.format_summary_json(now))
    }

    #[allow(clippy::too_many_arguments)]
    fn format_drop(
        &self,
        reason: DropReason,
        ident: u16,
        session: u16,
        bytes: u64,
        send_checked_at_entry: i32,
        send_checked_now: i32,
        send_block_count: usize,
        now: i64,
    ) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "event=gate_frame_drop gate={} reason={} ident={ident} session={session} bytes={bytes} \
             send_checked_at_entry={send_checked_at_entry} send_checked_now={send_checked_now} \
             send_block_count={send_block_count} suppressed_since_last={} \
             hold_frames={} hold_bytes={} oversize_frames={} oversize_bytes={} uptime_ms={}",
            self.gate,
            reason.as_str(),
            self.suppressed,
            self.hold_frames,
            self.hold_bytes,
            self.oversize_frames,
            self.oversize_bytes,
            now.saturating_sub(self.start_ms),
        );
        out
    }

    fn format_summary(&self, now: i64) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "event=gate_backpressure_summary gate={} hold_frames={} hold_bytes={} \
             oversize_frames={} oversize_bytes={} top_idents={} top_sessions={} \
             overflow_idents={} overflow_sessions={} uptime_ms={}",
            self.gate,
            self.hold_frames,
            self.hold_bytes,
            self.oversize_frames,
            self.oversize_bytes,
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
            "{{\"t_ms\":{},\"gate\":{},\"hold_frames\":{},\"hold_bytes\":{},\
             \"oversize_frames\":{},\"oversize_bytes\":{},\"top_idents\":\"{}\",\
             \"top_sessions\":\"{}\",\"overflow_idents\":{},\"overflow_sessions\":{}}}",
            now.saturating_sub(self.start_ms),
            self.gate,
            self.hold_frames,
            self.hold_bytes,
            self.oversize_frames,
            self.oversize_bytes,
            top_pairs(&self.by_ident),
            top_pairs(&self.by_session),
            self.ident_overflow,
            self.session_overflow,
        );
        out
    }
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

    fn hold(stats: &mut DropStats, ident: u16, session: u16, now: i64) -> Note {
        stats.note(DropReason::SendChecked, ident, session, 120, 1, 1, 0, now)
    }

    /// 判据 A1/A4：`SendChecked>0` 的丢帧必须**计数 +1**，且产出含 ident/session/reason/当前
    /// `SendChecked` 的结构化日志行。删掉计数或日志分支 ⇒ 本用例必红。
    #[test]
    fn send_checked_drop_is_counted_and_logged() {
        let mut stats = DropStats::new(3, 0);
        let note = hold(&mut stats, 50, 42, 1_000);
        assert_eq!(stats.hold_frames, 1, "Hold 必须计数");
        assert_eq!(stats.hold_bytes, 120);
        assert_eq!(stats.by_ident.get(&50), Some(&1), "按 ident 维度计数");
        assert_eq!(stats.by_session.get(&42), Some(&1), "按 session 维度计数");
        let log = note.log.expect("首次丢帧必须产出日志行");
        for needle in [
            "event=gate_frame_drop",
            "gate=3",
            "reason=SendChecked>0",
            "ident=50",
            "session=42",
            "send_checked_at_entry=1",
            "send_checked_now=1",
            "hold_frames=1",
        ] {
            assert!(log.contains(needle), "日志行缺 {needle}：{log}");
        }
        let summary = note.summary.expect("首次丢帧必须产出汇总");
        assert!(summary.contains("event=gate_backpressure_summary"));
        assert!(summary.contains("hold_frames=1"));
        assert!(summary.contains("top_idents=50:1"));
        assert!(summary.contains("top_sessions=42:1"));
    }

    /// 限速：1s 内第二条只计数不出行；到 1s 的那条把被压掉的条数带出来。
    #[test]
    fn log_is_rate_limited_but_counters_keep_growing() {
        let mut stats = DropStats::new(0, 0);
        assert!(hold(&mut stats, 50, 1, 10_000).log.is_some());
        assert!(
            hold(&mut stats, 50, 1, 10_010).log.is_none(),
            "1s 内必须限速"
        );
        assert_eq!(stats.hold_frames, 2);
        let log = hold(&mut stats, 50, 1, 11_000)
            .log
            .expect("到 1s 必须再出一行");
        assert!(
            log.contains("suppressed_since_last=1"),
            "要带被压条数：{log}"
        );
        assert!(log.contains("hold_frames=3"));
        assert!(hold(&mut stats, 50, 1, 11_001).log.is_none());
    }

    /// 汇总限速：5s 内不重复汇总；到点后汇总里的累计值是最新的。
    #[test]
    fn summary_is_rate_limited() {
        let mut stats = DropStats::new(1, 0);
        assert!(hold(&mut stats, 50, 1, 1_000).summary.is_some());
        assert!(hold(&mut stats, 50, 1, 1_100).summary.is_none());
        let summary = hold(&mut stats, 50, 1, 6_000)
            .summary
            .expect("到 5s 必须汇总");
        assert!(summary.contains("hold_frames=3"), "{summary}");
        assert!(summary.contains("gate=1"));
    }

    /// 第二个丢帧点（单包超阈值）必须单独计数、reason 不同。
    #[test]
    fn oversize_drop_is_counted_separately() {
        let mut stats = DropStats::new(2, 0);
        let note = stats.note(DropReason::Oversize, 50, 7, 999, 0, 0, 0, 0);
        assert_eq!(stats.oversize_frames, 1);
        assert_eq!(stats.hold_frames, 0);
        assert_eq!(stats.oversize_bytes, 999);
        let log = note.log.unwrap();
        assert!(log.contains("reason=Oversize"), "{log}");
        assert!(log.contains("ident=50"));
        assert!(log.contains("session=7"));
    }

    /// 退化输入：空/零字节、ident/session 为 0 也必须计数并出行（不允许空集静默）。
    #[test]
    fn zero_sized_frame_is_still_observed() {
        let mut stats = DropStats::new(0, 0);
        let note = stats.note(DropReason::SendChecked, 0, 0, 0, 1, 1, 0, 0);
        assert_eq!(stats.hold_frames, 1);
        let log = note.log.unwrap();
        assert!(log.contains("ident=0") && log.contains("session=0") && log.contains("bytes=0"));
    }

    /// 直方图键上限：第 49 个 ident / session 进 overflow，不无限长。
    #[test]
    fn histograms_are_bounded() {
        let mut stats = DropStats::new(0, 0);
        for i in 0..100u16 {
            let _ = hold(&mut stats, i, u16::MAX - i, 0);
        }
        assert_eq!(stats.by_ident.len(), MAX_IDENT_KEYS);
        assert_eq!(
            stats.ident_overflow,
            u64::from(100 - u16::try_from(MAX_IDENT_KEYS).unwrap())
        );
        assert_eq!(stats.by_session.len(), MAX_SESSION_KEYS);
        assert_eq!(
            stats.session_overflow,
            u64::from(100 - u16::try_from(MAX_SESSION_KEYS).unwrap())
        );
    }

    /// NDJSON：字段可被下行脚本按 JSON 解析（键名/数值形态），累计值单调。
    #[test]
    fn ndjson_summary_is_valid_json_and_appends() {
        let mut stats = DropStats::new(4, 1_000);
        let _ = hold(&mut stats, 50, 9, 2_000);
        let json = stats.final_summary_json(6_000).expect("有丢帧就要有终值");
        // 只做字段形态断言（不引 serde 依赖）：键全在、值全数字、无空白。
        for key in [
            "\"t_ms\":5000",
            "\"gate\":4",
            "\"hold_frames\":1",
            "\"hold_bytes\":120",
            "\"oversize_frames\":0",
            "\"top_idents\":\"50:1\"",
            "\"top_sessions\":\"9:1\"",
            "\"overflow_idents\":0",
            "\"overflow_sessions\":0",
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

    /// 没有丢帧 ⇒ 不产出终值汇总（不制造噪声）。
    #[test]
    fn final_summary_is_absent_without_drops() {
        let stats = DropStats::new(0, 0);
        assert!(stats.final_summary(10_000).is_none());
        assert!(stats.final_summary_json(10_000).is_none());
    }
}
