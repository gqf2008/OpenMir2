//! 网关侧的**速度/间隔规则**（照 `src/GameGate/Services/ClientSession.cs`）。
//!
//! 本模块只放**纯函数 + 计时状态**，时钟由调用方传进来（`now_ms`）——这样规则本身可以用
//! 显式时间戳单测（可证伪），而生产路径照 oracle 用真实时钟（`HUtil32.GetTickCount`）。
//! 差分/回放若要按金标准 `ts_ms` 推进，只需在调用方喂虚拟时间（协议梳理 §7.10.2 的方案 ②）。

/// `CM_SAY` 的间隔门（`ClientSession.cs:485-497`）。
///
/// 网关侧规则的**打包**（避免给会话函数塞一长串参数）：目前只有聊天间隔，后续 move/attack 等规则
/// 也挂到这里。
#[derive(Debug, Clone, Copy)]
pub struct GateRules {
    /// `Config.IsChatInterval`（`[Switch] ChatInterval`）。
    pub chat_interval_enabled: bool,
    /// `Config.ChatInterval`（`[Integer] ChatInterval`）。
    pub chat_interval_ms: i64,
}

/// `CM_SAY` 的间隔计时（`SessionSpeedRule.SayMsgTick` 的对应物）。
///
/// 逐字语义：`msgContent`（= 解码后 `decodeBuff[12..]`）**以 `@` 开头**（`_commandFlagSpan`，
/// `ClientSession.cs:92`）⇒ **不判间隔**；否则距上次 say 不足 `ChatInterval` ⇒ **丢帧**（`return`），
/// 通过则把 `SayMsgTick` 刷新为当前时间。
#[derive(Debug, Clone, Copy, Default)]
pub struct ChatThrottle {
    /// `SessionSpeedRule.SayMsgTick`（0 = 从未说过 ⇒ 第一条恒通过）。
    pub say_msg_tick: i64,
}

impl ChatThrottle {
    /// `true` = 允许转发；`false` = 按 oracle 丢掉这一帧。
    ///
    /// # Panics
    /// 不会 panic。
    #[must_use]
    pub fn allows(&mut self, now_ms: i64, content: &[u8], interval_ms: i64) -> bool {
        if content.starts_with(b"@") {
            return true; // 命令不走间隔（oracle 的 `!msgContent.StartsWith("@")` 外）
        }
        if now_ms - self.say_msg_tick < interval_ms {
            return false; // :492-495 —— 直接 return，不转发
        }
        self.say_msg_tick = now_ms;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 800ms 间隔（现网 `ChatInterval=800`）：第一条通过、紧接着第二条被丢、跨过间隔后又通过。
    #[test]
    fn say_interval_drops_the_second_message_too_soon() {
        let mut t = ChatThrottle::default();
        assert!(t.allows(1_000, b"hello", 800), "第一条（上次=0）必须通过");
        assert!(!t.allows(1_500, b"again", 800), "500ms 后第二条必须被丢");
        assert!(!t.allows(1_799, b"again", 800), "边界：799ms 仍要丢");
        assert!(
            t.allows(1_800, b"again", 800),
            "恰好 800ms 必须通过（>= interval）"
        );
        // 被丢的那两条**不刷新**计时：所以 1_800 这次通过后，1_801 又被丢
        assert!(!t.allows(1_801, b"x", 800));
    }

    /// `@` 开头的命令内容不走间隔（`ClientSession.cs:489` 的 `!StartsWith("@")` 之外）。
    #[test]
    fn at_prefixed_content_skips_interval_and_does_not_refresh_tick() {
        let mut t = ChatThrottle::default();
        assert!(t.allows(1_000, b"@move 1 2", 800));
        assert!(
            t.allows(1_001, b"@move 1 2", 800),
            "@ 命令不判间隔，连续两条都通过"
        );
        assert_eq!(t.say_msg_tick, 0, "走 @ 分支不刷新 SayMsgTick");
        assert!(t.allows(1_002, b"plain", 800), "普通内容仍按 last=0 通过");
    }

    /// 关掉开关（`[Switch] ChatInterval=0`）时调用方不该调本函数（见 main）；这里是"interval=0"的边界。
    #[test]
    fn zero_interval_never_drops() {
        let mut t = ChatThrottle::default();
        assert!(t.allows(100, b"a", 0));
        assert!(
            t.allows(100, b"b", 0),
            "interval=0 ⇒ now-last=0 不小于 0 ⇒ 通过"
        );
    }
}
