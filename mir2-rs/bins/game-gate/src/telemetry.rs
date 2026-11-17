//! **入口遥测**（卡 `openmir2-entry-telemetry`）：把「准入链各段的丢失」变成可数。
//!
//! 背景（2026-11-17）：1000 档三段缺口各自无观测——
//!   `1000 bots → 806 拿到 525 → game-svr 记到 774 进世界 → 客户端只算到 759`。
//! 本模块只做**旁路计数**：原子量 + 受控汇总打印（每 N 次事件 / 每 10s 心跳 / 关服），
//! **不改任何协议/回包/业务分支**。
//!
//! 布局：gate 侧覆盖上行（客户端帧/规则丢弃/登录解析失败/上行发送失败）与
//! 下行（`downlink_for_unknown_sid` 与写 socket 失败，按 ident 汇聚）。
//!
//! **没有「首帧 ident」槽位**：该诊断从未接线（`record_first_frame_ident` 曾存在但零调用点），
//! 交付前删除而不是留一个恒为空的字段——空字段会被读成「没有首帧」这种假事实。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};

/// 汇总打印的事件间隔（每 N 次上行帧打印一行；0 = 从不按帧数打印）。
const PRINT_EVERY: u64 = 1000;

/// 上行客户端帧总数（进入 `classify_frame` 之前计数）。
pub static FRAMES_IN: AtomicU64 = AtomicU64::new(0);
/// 上行 `GM_DATA` 成功送出的帧数。
pub static GM_DATA_FORWARDED: AtomicU64 = AtomicU64::new(0);
/// 上行 `GM_DATA` 发送失败次数（原实现是 `let _ =` 静默丢弃）。
pub static SEND_RAW_FAIL: AtomicU64 = AtomicU64::new(0);
/// 登录帧解析失败（`handle_login` 的 Err ⇒ 关会话）次数。
pub static LOGIN_PARSE_KICK: AtomicU64 = AtomicU64::new(0);
/// 下行因「会话不存在」被丢弃的帧数（按 ident 归组另计）。
pub static DOWNLINK_UNKNOWN: AtomicU64 = AtomicU64::new(0);
/// 下行**写客户端 socket 失败**的次数（原实现 `if wr.write_all(...).is_err() { break }` 静默）。
pub static DOWNLINK_WRITE_FAIL: AtomicU64 = AtomicU64::new(0);

/// 规则丢弃分桶：0 = move/run，1 = attack（`DropKick` 同桶），2 = spell，3 = chat。
pub static RULE_DROPPED: [AtomicU64; 4] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// ident 归组表槽位：0 = 下行未知会话，1 = 下行写 socket 失败。
fn ident_map(slot: usize) -> &'static Mutex<BTreeMap<u16, u64>> {
    static MAPS: OnceLock<[Mutex<BTreeMap<u16, u64>>; 2]> = OnceLock::new();
    let maps = MAPS.get_or_init(|| [Mutex::new(BTreeMap::new()), Mutex::new(BTreeMap::new())]);
    &maps[slot]
}

/// 下行写客户端 socket 失败按 ident 归组（slot 1）。
pub fn record_downlink_write_fail(ident: u16) {
    DOWNLINK_WRITE_FAIL.fetch_add(1, Relaxed);
    if let Ok(mut m) = ident_map(1).lock() {
        *m.entry(ident).or_insert(0) += 1;
    }
}

/// 下行「会话不存在」丢弃按 ident 归组（slot 0）。
pub fn record_downlink_unknown(ident: u16) {
    DOWNLINK_UNKNOWN.fetch_add(1, Relaxed);
    if let Ok(mut m) = ident_map(0).lock() {
        *m.entry(ident).or_insert(0) += 1;
    }
}

fn top(map: &BTreeMap<u16, u64>, n: usize) -> String {
    let mut v: Vec<(u16, u64)> = map.iter().map(|(k, c)| (*k, *c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter()
        .take(n)
        .map(|(k, c)| format!("{k}:{c}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// 一行汇总（调用方决定何时打印：每 N 帧 / 每 10s 心跳 / 关服）。
#[must_use]
pub fn summary_line() -> String {
    let unknown = ident_map(0).lock().map(|m| top(&m, 8)).unwrap_or_default();
    let write_fail = ident_map(1).lock().map(|m| top(&m, 8)).unwrap_or_default();
    format!(
        "ENTRY_TELEMETRY frames_in={} gm_data_forwarded={} send_raw_fail={} login_parse_kick={} \
         rule_dropped[move={} attack={} spell={} chat={}] downlink_unknown={} downlink_write_fail={} \
         unknown_idents=[{}] write_fail_idents=[{}] {}",
        FRAMES_IN.load(Relaxed),
        GM_DATA_FORWARDED.load(Relaxed),
        SEND_RAW_FAIL.load(Relaxed),
        LOGIN_PARSE_KICK.load(Relaxed),
        RULE_DROPPED[0].load(Relaxed),
        RULE_DROPPED[1].load(Relaxed),
        RULE_DROPPED[2].load(Relaxed),
        RULE_DROPPED[3].load(Relaxed),
        DOWNLINK_UNKNOWN.load(Relaxed),
        DOWNLINK_WRITE_FAIL.load(Relaxed),
        unknown,
        write_fail,
        // 卡 `openmir2-session-close-attribution`：拆除归因终值（与 `断开链接` 同源计数）。
        crate::close_attr::summary_fragment()
    )
}

/// 第 `frames_total` 帧到达时是否该打印汇总。
///
/// **入参就是调用方 `fetch_add` 的返回值 +1**，不是再读一次全局量：多线程下再读会漏档
/// （1000 被两个线程读到 ⇒ 重复打印；1000 那一档恰好被读成 1001 ⇒ **一行都不打印**）。
#[must_use]
pub fn maybe_summary(frames_total: u64) -> Option<String> {
    if PRINT_EVERY == 0 || frames_total == 0 || !frames_total.is_multiple_of(PRINT_EVERY) {
        return None;
    }
    Some(summary_line())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只有整千帧才打印；0 与半档不打印。退化形态：空输入（0 帧）⇒ None。
    #[test]
    fn maybe_summary_only_on_exact_multiples() {
        assert!(maybe_summary(0).is_none());
        assert!(maybe_summary(PRINT_EVERY - 1).is_none());
        assert!(maybe_summary(PRINT_EVERY).is_some());
        assert!(maybe_summary(PRINT_EVERY + 1).is_none());
        assert!(maybe_summary(PRINT_EVERY * 3).is_some());
    }

    /// 汇总行必须把每个字段名都写出来（缺字段 = 读数时看不出是 0 还是没接）。
    #[test]
    fn summary_line_carries_every_field() {
        let line = summary_line();
        for key in [
            "frames_in=",
            "gm_data_forwarded=",
            "send_raw_fail=",
            "login_parse_kick=",
            "rule_dropped[move=",
            "attack=",
            "spell=",
            "chat=",
            "downlink_unknown=",
            "downlink_write_fail=",
            "unknown_idents=[",
            "write_fail_idents=[",
            // 卡 `openmir2-session-close-attribution`：拆除归因终值必须进同一行。
            "close_total=",
            "close_breakdown[client_fin=",
            "close_errno=[",
        ] {
            assert!(line.contains(key), "汇总行缺字段 {key}: {line}");
        }
    }
}
