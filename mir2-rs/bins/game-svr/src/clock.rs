//! `MIR2_CLOCK_MS` 可注入时钟（M3 横切前置 `openmir2-m3-clock-injection`，**Rust 半**）。
//!
//! 用途：M3 的 ⑧/⑦/⑤ 家族判据里有大量**时间门**（攻城时限 `CastleWarTime`、门/墙 60s
//! `StruckTick`、金库 10min `SaveTick`、商人 200ms 节奏…）。oracle 侧这些门读
//! `HUtil32.GetTickCount()`（`Environment.TickCount`，`src/OpenMir2/HUtil32.cs:36-39`）与
//! `DateTime.Now`；Rust 侧对应 `bins/game-svr::now_ms()` 与 `crates/world::World::clock_ms`。
//! 两侧若各跑各的墙钟，时间门就**不可判**（只能目测手感）。
//!
//! 本模块把「可注入时钟」的 **Rust 侧半边** 接上，口径与既有 `MIR2_RNG_SEED`（T-30，
//! `seed.rs`）同族：
//!
//! - 变数 [`CLOCK_ENV`]（`MIR2_CLOCK_MS`）：虚拟时钟**起点**（十进制 `i64` 毫秒）。
//! - **未设/空串** ⇒ 不启用：`now_ms()` 仍取墙钟（`SystemTime`）、`world.clock_ms` 仍从 0 起
//!   ⇒ 生产/既有测试**逐字节不变**。
//! - 设了合法值 ⇒ `now_ms() = origin + 自本进程首次取钟起的单调毫秒`，
//!   且 `world.clock_ms` 以 **同一 origin** 为起点（此后仍按 oracle 的 +200ms/tick 推进）
//!   ⇒ 两侧「同起点 + 同推进」。
//! - 非法值（非十进制 `i64`/溢出/含非 UTF-8 字节）⇒ [`ClockError`]（`main` 在起监听之前
//!   `?` 传播 ⇒ **显式非零退出**）。**不许静默降级**：把「以为注入了时钟、其实没有」当
//!   时间门通过，会污染整批对拍结论（与 `seed.rs` 同一条纪律）。
//!
//! **C# 半仍未落地**：oracle 的 `HUtil32.GetTickCount`/`DateTime.Now` 是运行时方法，
//! 现有影子 hook 的「反射换私有字段」手法拦不住（见卡上 `openmir2-m3-clock-injection`
//! worker-5 的盘点 comment：需 owner 在 (a) Mono.Cecil IL patch / (b) 影子源码重建 /
//! (c) 不改 oracle、夹具按墙钟编排 之间裁决）。本模块只定义 **Rust 侧可判的注入契约**，
//! C# 半落地后按同一变数/语义对齐即可。
//!
//! 自证行：[`probe_line`] 产出 `CLOCK_PROBE origin=<n>`，由 `main`（启动处）与 `sim_loop`
//! （世界建好、写入 `world.clock_ms` 之后）各打一条 ⇒ 进程级判据可只读日志。

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// 可注入时钟起点的环境变数名（形如既有 `MIR2_RNG_SEED`）。
pub const CLOCK_ENV: &str = "MIR2_CLOCK_MS";

/// `MIR2_CLOCK_MS` 解析失败（对应 `main` 的 fail-closed 退出）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockError {
    message: String,
}

impl ClockError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ClockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ClockError {}

/// 解析 `MIR2_CLOCK_MS` 的文本（口径照 `seed::parse_seed`）。
///
/// - `None`（变数未设）或 `Some("")`（空串）⇒ `Ok(None)`：**不启用**；
/// - 十进制 `i64`（允许前后空白与 `+`/`-`）⇒ `Ok(Some(origin))`；
/// - 其余（非数字/十六进制/小数/溢出）⇒ `Err`。
///
/// 注意空白串（`"   "`）：空串分支不认它，会走到 `parse` 并失败 ⇒ `Err`（**不是**「空串」）。
pub fn parse_clock(raw: Option<&str>) -> Result<Option<i64>, ClockError> {
    let Some(text) = raw else {
        return Ok(None);
    };
    if text.is_empty() {
        return Ok(None);
    }
    text.trim()
        .parse::<i64>()
        .map(Some)
        .map_err(|_| ClockError::new(format!("{CLOCK_ENV} 不是合法 i64 毫秒: {text}")))
}

/// 从进程环境读 `MIR2_CLOCK_MS`（生产路径入口；`main` 在起监听之前调用 ⇒ fail-closed）。
pub fn env_clock() -> Result<Option<i64>, ClockError> {
    match std::env::var_os(CLOCK_ENV) {
        None => Ok(None),
        Some(os) => match os.to_str() {
            Some(text) => parse_clock(Some(text)),
            // 变数是 .NET string（Windows 上 UTF-16）⇒ 非 UTF-8 是异常形态；不当成「没设」静默放行。
            None => Err(ClockError::new(format!(
                "{CLOCK_ENV} 含非 UTF-8 字节: {os:?}"
            ))),
        },
    }
}

/// 启用后的注入时钟：固定起点 + 首次取钟起的单调经过。
#[derive(Debug)]
struct Injected {
    origin: i64,
    start: Instant,
}

impl Injected {
    fn new(origin: i64) -> Self {
        Self {
            origin,
            start: Instant::now(),
        }
    }

    /// `origin + 单调经过毫秒`（单调 ⇒ 不受墙钟回拨影响）。
    fn now_ms(&self) -> i64 {
        let elapsed = i64::try_from(self.start.elapsed().as_millis()).unwrap_or(i64::MAX);
        self.origin.saturating_add(elapsed)
    }
}

/// 进程内唯一注入源（`OnceLock` ⇒ 环境只读一次，且与 `main` 的校验同源）。
static SOURCE: OnceLock<Option<Injected>> = OnceLock::new();

fn source() -> &'static Option<Injected> {
    SOURCE.get_or_init(|| match env_clock() {
        Ok(Some(origin)) => Some(Injected::new(origin)),
        // 非法值已由 `main::env_clock()?` 在起监听之前拦下并非零退出；此处防御性退回「不启用」。
        Ok(None) | Err(_) => None,
    })
}

/// 未注入时的墙钟毫秒（**与改动前 `main::now_ms()` 逐字节相同**）。
fn wall_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// 当前毫秒：注入启用 ⇒ `origin + 单调经过`；未启用 ⇒ 墙钟（行为一字不改）。
#[must_use]
pub fn now_ms() -> i64 {
    match source() {
        Some(injected) => injected.now_ms(),
        None => wall_now_ms(),
    }
}

/// 注入启用的起点；未启用 ⇒ `None`（调用方据此决定是否覆盖 `world.clock_ms`）。
#[must_use]
pub fn injected_origin() -> Option<i64> {
    source().as_ref().map(|s| s.origin)
}

/// 自证行：`CLOCK_PROBE origin=<n>`（进程级判据只读日志即可；未启用 ⇒ 不打）。
#[must_use]
pub fn probe_line(origin: i64) -> String {
    format!("CLOCK_PROBE origin={origin}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_clock_unset_or_empty_is_disabled() {
        assert_eq!(parse_clock(None), Ok(None), "未设变数 ⇒ 不启用");
        assert_eq!(parse_clock(Some("")), Ok(None), "空串 ⇒ 不启用");
    }

    #[test]
    fn parse_clock_accepts_decimal_with_sign_and_whitespace() {
        assert_eq!(parse_clock(Some("0")), Ok(Some(0)));
        assert_eq!(
            parse_clock(Some("1700000000000")),
            Ok(Some(1_700_000_000_000))
        );
        assert_eq!(parse_clock(Some(" +7 ")), Ok(Some(7)), "允许前后空白/正号");
        assert_eq!(parse_clock(Some("-7")), Ok(Some(-7)), "允许负号");
        assert_eq!(parse_clock(Some("9223372036854775807")), Ok(Some(i64::MAX)));
        assert_eq!(
            parse_clock(Some("-9223372036854775808")),
            Ok(Some(i64::MIN))
        );
    }

    #[test]
    fn parse_clock_rejects_non_integer() {
        for text in ["abc", "0x10", "1.5", "9223372036854775808", "   "] {
            assert!(
                parse_clock(Some(text)).is_err(),
                "{text:?} 不是合法 i64 毫秒，必须 Err（fail-closed）"
            );
        }
    }

    /// 构造级判据：注入源从固定 origin 起，且**单调不减**（不是墙钟）。
    #[test]
    fn injected_clock_starts_at_origin_and_is_monotonic() {
        let inj = Injected::new(1_700_000_000_000);
        let first = inj.now_ms();
        assert!(
            first >= 1_700_000_000_000,
            "首个读数必须 >= origin（origin + 经过），实得 {first}"
        );
        assert!(
            first < 1_700_000_000_000 + 60_000,
            "首个读数必须贴近 origin（构造到首读 < 60s），实得 {first}"
        );
        let second = inj.now_ms();
        assert!(second >= first, "单调不减：{first} -> {second}");
        assert!(
            second - first < 60_000,
            "同用例内经过必须很小，实得 {}",
            second - first
        );
    }

    #[test]
    fn probe_line_shape() {
        assert_eq!(probe_line(42), "CLOCK_PROBE origin=42");
    }
}
