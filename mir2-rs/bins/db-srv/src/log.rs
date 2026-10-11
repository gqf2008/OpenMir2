//! 控制台日志。前缀与 C# Serilog 模板 `{Timestamp:HH:mm:ss.fff} [{Level:u3}] {Message:lj}`
//! 同形，便于 M1 验收①里"两侧日志时间线对齐"。

use std::time::{SystemTime, UNIX_EPOCH};

static SHOW_DEBUG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 打开/关闭调试输出（对应 C# `SettingsModel.ShowDebug`：部署里 `ShowDebug` 缺省为 0 ⇒ 不打
/// `LogService.Debug`；混跑取证时把它置 1 可拿到完整时间线）。
pub fn set_debug(on: bool) {
    SHOW_DEBUG.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn stamp() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    // 本地时间是 UTC+8（现网部署），日志只为对齐时间线用
    let local = secs + 8 * 3600;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        (local / 3600) % 24,
        (local / 60) % 60,
        local % 60,
        d.subsec_millis()
    )
}

/// 信息级。
pub fn info(msg: &str) {
    println!("{} [INF] {msg}", stamp());
}

/// 调试级（C# `LogService.Debug`：`blnShowDebug` 打开时才输出）。
pub fn debug(msg: &str) {
    if SHOW_DEBUG.load(std::sync::atomic::Ordering::Relaxed) {
        println!("{} [DBG] {msg}", stamp());
    }
}

/// 警告级。
pub fn warn(msg: &str) {
    println!("{} [WRN] {msg}", stamp());
}

/// 错误级。
pub fn error(msg: &str) {
    println!("{} [ERR] {msg}", stamp());
}
