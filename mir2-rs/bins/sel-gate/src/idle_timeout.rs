//! SelGate 会话空闲超时（审计 #12，oracle 两阶段）。
//!
//! 逐行对齐 C# `src/SelGate`：
//! - `Services/ClientSession.cs:24`：构造时 `_clientTimeOutTick = HUtil32.GetTickCount()`；
//! - `Services/ClientSession.cs:92-118`：`HandleDelayMsg` 两阶段，判据是**严格大于**
//!   `HUtil32.GetTickCount() - _clientTimeOutTick > Config.m_nClientTimeOutTime`；
//! - `Services/ClientSession.cs:45-75`：`HandleUserPacket` **只**对
//!   `CM_QUERYCHR/CM_NEWCHR/CM_DELCHR/CM_SELCHR`（100..=103）刷新计时；其它帧只记
//!   `错误的数据包索引` 日志，**不刷新**；
//! - `TimedService.cs:38-76`：每 `> 20 * 1000` ms 扫描一次，`success=true` 时立即
//!   `_sessionManager.CloseSession` 并摘除 `SessionArray` 槽位（阶段二线上不可达）；
//!   SelGate 的 `SessionManager.CloseSession` **只 `TryRemove`、不调 `ClientSession.CloseSession`**
//!   （`Services/SessionManager.cs:60-67`，与 LoginGate 相反）⇒ 超时后 **socket 保持打开**，
//!   后续客户端数据因 `GetSession == null` 被静默丢弃，直到客户端自己断开；
//! - `Conf/GateConfig.cs:116`：缺省 `m_nClientTimeOutTime = 60 * 1000`；
//!   `Conf/ConfigManager.cs:33-38`：`< 10 * 1000` 夹到 `10 * 1000`。
//!
//! 帧字节照抄 C# `SendDefMessage` 的既有缺陷（见 [`out_of_connection_frame`]）。

use mir2_protocol::edcode;
use mir2_protocol::frame::CommandMessage;
use mir2_protocol::messages::{CM_DELCHR, CM_NEWCHR, CM_QUERYCHR, CM_SELCHR, SM_OUTOFCONNECTION};

/// C# `SelGate/Conf/GateConfig.cs:116` 的缺省值（60s）。
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;
/// C# `SelGate/Conf/ConfigManager.cs:33-38`：低于 10s 一律夹到 10s。
pub const MIN_TIMEOUT_MS: u64 = 10_000;
/// C# `SelGate/TimedService.cs:40`：`> 20 * 1000` ms 一次扫描。
pub const CHECK_INTERVAL_MS: u64 = 20_000;

/// `HandleDelayMsg` 一次轮询的产物。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutAction {
    /// 未超时（严格 `>` 未成立）。
    None,
    /// 第一阶段：`SendDefMessage(SM_OUTOFCONNECTION, 0, 0, 0, 0)` + `_kickFlag = true`。
    /// C# `TimedService` 在**同一次扫描**立即关会话（见模块注释）。
    SendOutOfConnection,
    /// 第二阶段：`_kickFlag` 已置位后再次超时（`CloseClient`）。线上不可达。
    Close,
}

/// 单会话计时状态（纯函数；时钟由调用方传入，便于构造级测试）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleTimer {
    last_tick_ms: u64,
    kick_flag: bool,
}

impl IdleTimer {
    /// C# 构造：`_clientTimeOutTick = HUtil32.GetTickCount()`。
    #[must_use]
    pub fn new(now_ms: u64) -> Self {
        Self {
            last_tick_ms: now_ms,
            kick_flag: false,
        }
    }

    /// C# `HandleUserPacket` 里唯一的计时刷新点（四个角色管理命令）。
    pub fn refresh(&mut self, now_ms: u64) {
        self.last_tick_ms = now_ms;
    }

    /// C# `HandleDelayMsg`：严格 `now - tick > timeout` 才动作；动作时把基准重置为 `now`。
    pub fn poll(&mut self, now_ms: u64, timeout_ms: u64) -> TimeoutAction {
        let elapsed = now_ms.saturating_sub(self.last_tick_ms);
        if !self.kick_flag {
            if elapsed > timeout_ms {
                self.last_tick_ms = now_ms;
                self.kick_flag = true;
                return TimeoutAction::SendOutOfConnection;
            }
            TimeoutAction::None
        } else if elapsed > timeout_ms {
            self.last_tick_ms = now_ms;
            TimeoutAction::Close
        } else {
            TimeoutAction::None
        }
    }
}

/// `HandleUserPacket` 的刷新条件：仅 `CM_QUERYCHR/CM_NEWCHR/CM_DELCHR/CM_SELCHR`。
///
/// 牙：把条件放宽成「任意帧都刷新」（审计表的直觉）或漏掉任一命令 ⇒ 单测红。
#[must_use]
pub fn refreshes_on_ident(ident: u16) -> bool {
    matches!(ident, CM_QUERYCHR | CM_NEWCHR | CM_DELCHR | CM_SELCHR)
}

/// SelGate 自己产生的 `SM_OUTOFCONNECTION` 帧字节（逐字节照抄 C# 既有缺陷）。
///
/// C# `ClientSession.SendDefMessage`（`Services/ClientSession.cs:142-161`）：
/// 先 `SendBuf[0] = '#'`，随后 `iLen = EncryptUtil.Encode(TempBuf, CommandMessage.Size,
/// SendBuf)` 从**下标 0** 开始写，把 `'#'` 覆盖掉；`SendBuf[iLen + 1] = '!'` 落在下标 17，
/// 最后 `_serverService.SendMessage(Session.SocketId, SendBuf, iLen + 2)` 发送 18 字节
/// ⇒ 线上是 `EDCode(12B 头) + 0x00 + '!'`：**没有 `'#'` 前导**，且中间夹一个 0 字节。
///
/// 这是 C# 的既有缺陷，按「行为等价优先于修 bug」1:1 保留（登记见报告）。
#[must_use]
pub fn out_of_connection_frame() -> Vec<u8> {
    let head = CommandMessage::make(i32::from(SM_OUTOFCONNECTION), 0, 0, 0, 0);
    let mut out = edcode::encode(&head.to_bytes());
    // `iLen` = 编码长度；`SendBuf[iLen]` 保持零初始化，`SendBuf[iLen + 1] = '!'`。
    out.push(0);
    out.push(b'!');
    out
}

#[cfg(test)]
mod tests {
    use super::{
        out_of_connection_frame, refreshes_on_ident, IdleTimer, TimeoutAction, CHECK_INTERVAL_MS,
        DEFAULT_TIMEOUT_MS, MIN_TIMEOUT_MS,
    };

    /// 判据 1（严格性）：恰好等于阈值**不**超时；阈值 +1ms 才触发。
    /// 牙：把 `>` 改成 `>=` ⇒ 本用例红。
    #[test]
    fn exactly_threshold_does_not_timeout() {
        let mut t = IdleTimer::new(1_000);
        assert_eq!(
            t.poll(1_000 + DEFAULT_TIMEOUT_MS, DEFAULT_TIMEOUT_MS),
            TimeoutAction::None,
            "C# 判据是严格 `>`：elapsed == timeout 不得触发"
        );
        assert_eq!(
            t.poll(1_000 + DEFAULT_TIMEOUT_MS + 1, DEFAULT_TIMEOUT_MS),
            TimeoutAction::SendOutOfConnection
        );
    }

    /// 判据 2（两阶段顺序与副作用）：阶段一先发帧并置 kick 标志、基准重置；
    /// 再过一整个阈值才进入阶段二 Close，且**不会**再次返回 SendOutOfConnection。
    /// 牙：把两阶段对调（先 Close 或重复发帧）⇒ 本用例红。
    #[test]
    fn two_stages_in_order_with_tick_reset() {
        let mut t = IdleTimer::new(0);
        assert!(!t.kick_flag, "初始不得带 kick 标志");
        assert_eq!(
            t.poll(DEFAULT_TIMEOUT_MS + 1, DEFAULT_TIMEOUT_MS),
            TimeoutAction::SendOutOfConnection
        );
        assert!(t.kick_flag, "阶段一必须置 kick 标志");
        assert_eq!(
            t.poll(2 * DEFAULT_TIMEOUT_MS + 1, DEFAULT_TIMEOUT_MS),
            TimeoutAction::None,
            "阶段一后基准必须重置，否则会在下一轮立即二次动作"
        );
        assert_eq!(
            t.poll(2 * DEFAULT_TIMEOUT_MS + 2, DEFAULT_TIMEOUT_MS),
            TimeoutAction::Close,
            "阶段二必须是 Close（而不是重复发 SM_OUTOFCONNECTION）"
        );
    }

    /// 判据 3（收到数据的刷新规则，oracle 事实）：
    /// - 四个角色管理命令刷新；
    /// - 其它 ident（含 0）**不**刷新；
    /// - 刷新后必须把计时推后（阈值内不再触发）。
    ///
    /// 牙：漏一个命令、或把条件改成「任意帧刷新」⇒ 本用例红。
    #[test]
    fn refresh_only_on_the_four_character_commands() {
        for ident in [100u16, 101, 102, 103] {
            assert!(
                refreshes_on_ident(ident),
                "C# 的 HandleUserPacket 必须对 CM ident={ident} 刷新计时"
            );
        }
        for ident in [0u16, 99, 104, 105, 200, 528] {
            assert!(
                !refreshes_on_ident(ident),
                "C# 的 default 支（ident={ident}）只记日志，不得刷新计时"
            );
        }

        // 刷新语义：59s 时收到 CM_NEWCHR ⇒ 再等 59s 仍不超时。
        let mut t = IdleTimer::new(0);
        t.refresh(59_000);
        assert_eq!(
            t.poll(59_000 + DEFAULT_TIMEOUT_MS, DEFAULT_TIMEOUT_MS),
            TimeoutAction::None
        );
        assert_eq!(
            t.poll(59_000 + DEFAULT_TIMEOUT_MS + 1, DEFAULT_TIMEOUT_MS),
            TimeoutAction::SendOutOfConnection
        );
    }

    /// 判据 4（刷新不解除已置位的 kick 标志）：C# 的刷新只在 `HandleUserPacket`，
    /// 而阶段一之后会话已被 TimedService 摘下 ⇒ 刷新不可能「救回」；这里钉住状态机
    /// 不把 kick 标志清掉（否则阶段二永远不回 Close）。
    #[test]
    fn refresh_does_not_clear_kick_flag() {
        let mut t = IdleTimer::new(0);
        assert_eq!(
            t.poll(DEFAULT_TIMEOUT_MS + 1, DEFAULT_TIMEOUT_MS),
            TimeoutAction::SendOutOfConnection
        );
        t.refresh(10_000);
        assert!(t.kick_flag);
        assert_eq!(
            t.poll(10_000 + DEFAULT_TIMEOUT_MS + 1, DEFAULT_TIMEOUT_MS),
            TimeoutAction::Close
        );
    }

    /// 判据 5（检查节奏常量）：C# `TimedService` 是 `> 20 * 1000` ms 扫描。
    #[test]
    fn check_interval_matches_oracle() {
        assert_eq!(CHECK_INTERVAL_MS, 20_000);
    }

    /// 判据 6（配置夹取）：`< 10s` 夹到 10s（C# `ConfigManager.cs:33-38`）。
    #[test]
    fn min_timeout_is_ten_seconds() {
        assert_eq!(MIN_TIMEOUT_MS, 10_000);
        const { assert!(DEFAULT_TIMEOUT_MS > MIN_TIMEOUT_MS) };
    }

    /// 判据 7（帧字节）：线上是 `EDCode(12B 头) + 0x00 + '!'`，共 18 字节；
    /// **没有** `'#'` 前导（C# 先写 `'#'` 又被 Encode 覆盖）。
    /// 字节字面量由独立实现（Python 复算）给出。
    /// 牙：补上 `'#'`、去掉 0 字节、或改发 `'!$'` ⇒ 本用例红。
    #[test]
    fn out_of_connection_frame_matches_oracle_bytes() {
        let frame = out_of_connection_frame();
        assert_eq!(
            frame,
            vec![
                0x64, 0x64, 0x68, 0x6b, 0x64, 0x68, 0x6a, 0x6b, 0x64, 0x64, 0x68, 0x6b, 0x64, 0x64,
                0x68, 0x6b, 0x00, 0x21,
            ],
            "必须逐字节等于 C# SelGate 的线上形态（无 '#' 前导 + 0 字节）"
        );
        assert_eq!(frame.len(), 18);
        assert_ne!(frame[0], b'#', "C# 的 '#' 被 Encode 覆盖，线上没有前导 '#'");
        assert_eq!(frame[frame.len() - 1], b'!');
    }

    /// 判据 8（帧头语义）：编码前的 12B 头 = recog 0 / ident 528 / 其余 0。
    #[test]
    fn out_of_connection_frame_decodes_to_ident_528() {
        let frame = out_of_connection_frame();
        let decoded = mir2_protocol::edcode::decode(&frame[..16]);
        let head = mir2_protocol::frame::CommandMessage::from_bytes(&decoded).expect("12B 头");
        assert_eq!(head.ident, 528);
        assert_eq!(head.recog, 0);
        assert_eq!((head.param, head.tag, head.series), (0, 0, 0));
    }
}
