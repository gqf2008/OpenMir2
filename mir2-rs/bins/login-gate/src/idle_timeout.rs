//! LoginGate 会话空闲超时（审计 #12，oracle 两阶段）。
//!
//! 逐行对齐 C# `src/LoginGate`：
//! - `Services/ClientSession.cs:20`：构造时 `m_dwClientTimeOutTick = HUtil32.GetTickCount()`；
//! - `Services/ClientSession.cs:128-150`：`HandleDelayMsg` 两阶段，判据是**严格大于**
//!   `HUtil32.GetTickCount() - tick > Config.ClientTimeOutTime`；
//! - `TimedService.cs:43-85`：每 `> 1000` ms 扫描一次，`success=true` 时**同一次扫描**
//!   立即 `_sessionManager.CloseSession` 并摘除 `SessionArray` 槽位。
//! - `Conf/GateConfig.cs:63`：缺省 `ClientTimeOutTime = 60 * 1000`；
//!   `Conf/ConfigManager.cs:27-31`：`< 10 * 1000` 一律夹到 `10 * 1000`。
//!
//! **与审计表直觉相反的事实（已复核 oracle）**：LoginGate 的 `ProcessClientPacket` 里
//! 所有 `m_dwClientTimeOutTick = HUtil32.GetTickCount()` 都**被注释掉**
//! （`ClientSession.cs:77-117`）⇒ **收到任何客户端数据都不刷新计时**。计时只在会话构造
//! 与超时触发（两阶段各自重置）时更新。`SelGate` 的刷新规则则只限 4 个角色命令，见其
//! 同名模块。
//!
//! 因为 `TimedService` 在 `success=true` 的同一次扫描就关会话，`HandleDelayMsg` 的
//! `else`（第二阶段）在线上**不可达**；本模块仍 1:1 保留该状态机（构造级判据要测两阶段
//! 顺序），线上则在第一阶段发帧后立即断开。
//!
//! 帧字节照抄 C# `SendDefMessage` 的既有缺陷（见 [`out_of_connection_frame`]）。

use mir2_protocol::edcode;
use mir2_protocol::frame::CommandMessage;
use mir2_protocol::messages::SM_OUTOFCONNECTION;

/// C# `LoginGate/Conf/GateConfig.cs:63` 的缺省值（60s）。
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;
/// C# `LoginGate/Conf/ConfigManager.cs:27-31`：低于 10s 一律夹到 10s。
pub const MIN_TIMEOUT_MS: u64 = 10_000;
/// C# `LoginGate/TimedService.cs:45`：`> 1000` ms 一次扫描。
pub const CHECK_INTERVAL_MS: u64 = 1_000;

/// `HandleDelayMsg` 一次轮询的产物。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutAction {
    /// 未超时（严格 `>` 未成立）。
    None,
    /// 第一阶段：`SendDefMessage(SM_OUTOFCONNECTION, 0, 0, 0, 0)` + `m_KickFlag = true`。
    /// C# `TimedService` 在**同一次扫描**立即关会话（见模块注释）。
    SendOutOfConnection,
    /// 第二阶段：`m_KickFlag` 已置位后再次超时。线上不可达（第一阶段已关会话）。
    Close,
}

/// 单会话计时状态（纯函数；时钟由调用方传入，便于构造级测试）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleTimer {
    last_tick_ms: u64,
    kick_flag: bool,
}

impl IdleTimer {
    /// C# 构造：`m_dwClientTimeOutTick = HUtil32.GetTickCount()`。
    #[must_use]
    pub fn new(now_ms: u64) -> Self {
        Self {
            last_tick_ms: now_ms,
            kick_flag: false,
        }
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

/// LoginGate 自己产生的 `SM_OUTOFCONNECTION` 帧字节（逐字节照抄 C# 既有缺陷）。
///
/// C# `ClientSession.SendDefMessage`（`Services/ClientSession.cs:181-199`）：
/// `sendBuf[0] = '#'`；`iLen = EncryptUtil.Encode(tempBuf, CommandMessage.Size, sendBuf, 1)`
/// 把 16 个编码字符写到 `sendBuf[1..17]` 并返回 `iLen = 16`；随后 `sendBuf[iLen + 1] = '!'`
/// （下标 17）；但 `_serverService.SendMessage(connectionId, sendBuf, iLen)` **只发 `iLen`
/// 字节** ⇒ 线上是 `'#' + 15 个编码字符`（16 字节）：最后一个编码字符与 `'!'` 都没发出去。
///
/// 这是 C# 的既有缺陷，按「行为等价优先于修 bug」1:1 保留（登记见报告）。
#[must_use]
pub fn out_of_connection_frame() -> Vec<u8> {
    let head = CommandMessage::make(i32::from(SM_OUTOFCONNECTION), 0, 0, 0, 0);
    let encoded = edcode::encode(&head.to_bytes());
    let mut out = Vec::with_capacity(encoded.len());
    out.push(b'#');
    // C# 实际发送长度 = encoded.len()（`'#'` 占 1，故编码字符只发出 len-1 个）。
    out.extend_from_slice(&encoded[..encoded.len() - 1]);
    out
}

#[cfg(test)]
mod tests {
    use super::{
        out_of_connection_frame, IdleTimer, TimeoutAction, CHECK_INTERVAL_MS, DEFAULT_TIMEOUT_MS,
        MIN_TIMEOUT_MS,
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
        // 阶段一把基准重置为触发时刻 ⇒ 恰好再过一个阈值不触发（严格 `>`）。
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

    /// 判据 3（阶段二之后继续轮询）：再过一个阈值仍是 Close（C# `else` 支不变），
    /// 不会回到阶段一。
    #[test]
    fn stage_two_is_sticky() {
        let mut t = IdleTimer::new(0);
        assert_eq!(
            t.poll(DEFAULT_TIMEOUT_MS + 1, DEFAULT_TIMEOUT_MS),
            TimeoutAction::SendOutOfConnection
        );
        assert_eq!(
            t.poll(2 * DEFAULT_TIMEOUT_MS + 2, DEFAULT_TIMEOUT_MS),
            TimeoutAction::Close
        );
        assert_eq!(
            t.poll(3 * DEFAULT_TIMEOUT_MS + 3, DEFAULT_TIMEOUT_MS),
            TimeoutAction::Close
        );
    }

    /// 判据 4（检查节奏常量）：C# `TimedService` 是 `> 1000` ms 扫描。
    #[test]
    fn check_interval_matches_oracle() {
        assert_eq!(CHECK_INTERVAL_MS, 1_000);
    }

    /// 判据 5（配置夹取）：`< 10s` 夹到 10s（C# `ConfigManager.cs:27-31`）。
    /// 由 config 模块的用例覆盖读取；此处只钉常量关系。
    #[test]
    fn min_timeout_is_ten_seconds() {
        assert_eq!(MIN_TIMEOUT_MS, 10_000);
        const { assert!(DEFAULT_TIMEOUT_MS > MIN_TIMEOUT_MS) };
    }

    /// 判据 6（帧字节）：线上是 `'#' + EDCode(12B 头)[..15]`，共 16 字节；
    /// **没有** `'!'`，也不含末位编码字符。字节字面量由独立实现（Python 复算）给出。
    /// 牙：把 `dstOffset=1` 的截断「修好」（补 `'!'` / 发满 16 个字符）⇒ 本用例红。
    #[test]
    fn out_of_connection_frame_matches_oracle_bytes() {
        let frame = out_of_connection_frame();
        assert_eq!(
            frame,
            vec![
                0x23, 0x64, 0x64, 0x68, 0x6b, 0x64, 0x68, 0x6a, 0x6b, 0x64, 0x64, 0x68, 0x6b, 0x64,
                0x64, 0x68,
            ],
            "必须逐字节等于 C# LoginGate 的线上形态（含其截断缺陷）"
        );
        assert_eq!(frame.len(), 16);
        assert_eq!(frame[0], b'#');
        assert!(!frame.contains(&b'!'), "C# 的 '!' 从未被发送");
    }

    /// 判据 7（帧头语义）：线上 15 个编码字符 = EDCode(12B 头) 去掉最后 1 个字符
    /// （C# 的截断缺陷），整段解码回去必须是 recog 0 / ident 528 / 其余 0。
    #[test]
    fn out_of_connection_frame_decodes_to_ident_528() {
        use mir2_protocol::frame::CommandMessage;

        let frame = out_of_connection_frame();
        let expected = CommandMessage::make(528, 0, 0, 0, 0);
        let full = mir2_protocol::edcode::encode(&expected.to_bytes());
        assert_eq!(
            &frame[1..],
            &full[..full.len() - 1],
            "线上应只差 C# 丢掉的最后一个编码字符"
        );
        let decoded = mir2_protocol::edcode::decode(&full);
        let head = CommandMessage::from_bytes(&decoded).expect("12B 头");
        assert_eq!(head, expected);
    }
}
