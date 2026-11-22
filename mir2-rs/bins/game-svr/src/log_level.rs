//! **DBG 级别闸**（`MIR2_LOG_LEVEL`）：默认**未设 ⇒ 全打**（与今天逐字节相同）。
//!
//! 起因：`sim_loop` 每个命令事件无条件 `glog!("DBG", …)`，`glog!` 原先没有过滤 ⇒ 1k 走位
//! 2 分钟写出 94MB 日志，并且这些 `println!`（行缓冲 + stdout 锁）吃掉了 `HostInbound` 的
//! 绝大部分墙钟。本模块只加一个**旁路级别判定**：热路径成本 = 一次 relaxed 原子读 + 比较；
//! 不取锁、不做格式化。
//!
//! ## 取值
//!
//! - 未设/空串/**非法值** ⇒ 阈值 4（全打，fail-closed 回默认；绝不静默变成抑制）；
//! - `ERROR`/`ERR` ⇒ 1；`WARN`/`WRN` ⇒ 2；`INFO`/`INF` ⇒ 3；`DEBUG`/`DBG`/`ALL` ⇒ 4。
//! - 大小写不敏感；前后空白 `trim()`。
use std::sync::atomic::{AtomicU8, Ordering};

/// 级别严重度（越小越严重）：ERR=1 < WRN=2 < INF=3 < DBG=4。
#[must_use]
pub fn severity_for(level: &str) -> u8 {
    match level {
        "ERR" => 1,
        "WRN" => 2,
        "INF" => 3,
        _ => 4, // DBG 及其它未登记标签按 DBG 处理
    }
}

/// 解析 `MIR2_LOG_LEVEL` 的**阈值**（>= 阈值的级别被打印）。
///
/// 未设/空串/非法值 ⇒ **4（全打）**：fail-closed，绝不让配置错误变成静默抑制。
#[must_use]
pub fn threshold_from_str(raw: Option<&str>) -> u8 {
    let Some(raw) = raw else { return 4 };
    let v = raw.trim();
    if v.is_empty() {
        return 4;
    }
    let up = v.to_ascii_uppercase();
    match up.as_str() {
        "ERROR" | "ERR" => 1,
        "WARN" | "WRN" => 2,
        "INFO" | "INF" => 3,
        "DEBUG" | "DBG" | "ALL" => 4,
        _ => 4,
    }
}

/// 默认 4 = 全打（未初始化时与今天完全相同）。
static THRESHOLD: AtomicU8 = AtomicU8::new(4);

/// 由进程启动段调用一次；环境变量只在此时读一次。
pub fn init_from_env() {
    let raw = std::env::var(crate::log_level::ENV).ok();
    let threshold = threshold_from_str(raw.as_deref());
    THRESHOLD.store(threshold, Ordering::Relaxed);
}

pub const ENV: &str = "MIR2_LOG_LEVEL";

/// 热路径判定：一次 relaxed 原子读 + 比较。
#[inline]
#[must_use]
pub fn enabled(severity: u8) -> bool {
    severity <= THRESHOLD.load(Ordering::Relaxed)
}

/// 测试期：所有改 `THRESHOLD` 的用例共用一把锁（`THRESHOLD` 是进程级全局，
/// `cargo test` 的并行线程会互相串改阈值 ⇒ 断言时序性翻转）。
///
/// **这一条修复的是既有测试隔离缺陷，不是 HostDiag 闸所需的新装置**：在 `host_diag_*`
/// 引入之前，`log_level::tests` 与 `ids::test_log_gate` 两个用例就已经在同一全局上
/// 无锁互串（opc-27 2026-11-23 复核指出，按缺陷登记）。
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn set_threshold_for_test(v: u8) {
    THRESHOLD.store(v, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_parsing_includes_invalid_falls_back_to_all() {
        assert_eq!(threshold_from_str(None), 4);
        assert_eq!(threshold_from_str(Some("")), 4);
        assert_eq!(threshold_from_str(Some("  ")), 4);
        assert_eq!(threshold_from_str(Some("info")), 3);
        assert_eq!(threshold_from_str(Some(" INFO ")), 3);
        assert_eq!(threshold_from_str(Some("WARN")), 2);
        assert_eq!(threshold_from_str(Some("err")), 1);
        assert_eq!(threshold_from_str(Some("debug")), 4);
        // 非法值 ⇒ 全打（fail-closed）
        assert_eq!(threshold_from_str(Some("verbose")), 4);
        assert_eq!(threshold_from_str(Some("0")), 4);
    }

    #[test]
    fn enabled_gates_by_severity_and_default_is_all() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_threshold_for_test(4);
        for sev in 1..=4 {
            assert!(enabled(sev), "默认（全打）");
        }
        set_threshold_for_test(3); // INFO
        assert!(enabled(3) && enabled(2) && enabled(1));
        assert!(!enabled(4), "INFO 阈值必须抑制 DBG");
        set_threshold_for_test(4);
    }

    #[test]
    fn severity_labels_map_to_expected_order() {
        assert_eq!(severity_for("ERR"), 1);
        assert_eq!(severity_for("WRN"), 2);
        assert_eq!(severity_for("INF"), 3);
        assert_eq!(severity_for("DBG"), 4);
        assert!(severity_for("ERR") < severity_for("WRN"));
        assert!(severity_for("WRN") < severity_for("INF"));
        assert!(severity_for("INF") < severity_for("DBG"));
    }
}
