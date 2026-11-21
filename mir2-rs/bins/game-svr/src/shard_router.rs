//! **P2-3b 事件路由**：单条输入通道 → 每片 sim 的（普通, 优先）双通道。
//!
//! 本模块只放**可单测的纯逻辑**（目录 + 判定）；真正的转发线程在 `lib.rs`
//! （它要复用 `EventDrain` 的预算/结转语义，且事件类型是 crate 私有的）。
//!
//! # 路由规则（唯一口径，见 [`route_target`]）
//!
//! - `RegisterGate` / `UnregisterGate` ⇒ **广播**：每个 World 都要向该网关线发帧；
//! - 带角色名的事件（`Command` / `Closed` / `KickAccount` / `AdminGm`）⇒ 查
//!   [`ShardDirectory`]（进世界时写入、离开时移除），**未知（未登录/未进世界）一律片 0**；
//! - 其余（`Opened` / `CertificationPending` / `LoadDbRequest`）⇒ 片 0。
//!
//! # 为什么不能按 `index`/`socket` 轮转分发
//!
//! `GateEvent::Command.index` 是**该网关连接自己的** `UserList` 槽位号，`socket` 在不同
//! 网关线上会重复（见 `lib.rs` 的 `GateEvent::Command` 文档：`ChannelMessageHandler.cs`
//! 先按 `SessionIndex-1` 且 socket 相符解出说话者，否则线性扫 socket）——两者都不是
//! 全局唯一会话键。轮转分发会把 A 连接的命令投到 B 连接所在的片。⇒ 本批**登录与进世界
//! 永远落在片 0**；跨片进图仍由既有 guard fail-closed。handoff（P2-5）需要先给事件加
//! 全局会话键（网关线号 + 会话号）再改这里。
//!
//! # N=1
//!
//! `MIR2_SHARDS=1` 时 `sim_loop` 传 `None` 目录、不建路由线程——本模块的函数全部不执行，
//! stdout / 行为与 master 逐字节相同。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::shard::{ShardId, HOST_SHARD};
use crate::GateEvent;

/// `chr_name → ShardId` 目录：进世界写入、离开移除，跨线程共享。
///
/// 只作**路由提示**：条目缺失一律回落到片 0（fail-closed 到主机），不会把事件丢进虚空。
/// 本批所有会话都在片 0（无 handoff），因此目录内容恒为 `{chr: 片 0}`——它的价值在
/// 接线与测试：P2-5 把会话搬进非 0 片后，路由自动跟着目录走。
#[derive(Clone, Debug, Default)]
pub(crate) struct ShardDirectory(Arc<Mutex<HashMap<String, ShardId>>>);

impl ShardDirectory {
    /// 空目录。
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 记「`chr_name` 现在在 `shard`」。
    pub(crate) fn set(&self, chr_name: &str, shard: ShardId) {
        let mut map = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.insert(chr_name.to_string(), shard);
    }

    /// 记「`chr_name` 已离开世界」（幂等）。
    pub(crate) fn remove(&self, chr_name: &str) {
        let mut map = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.remove(chr_name);
    }

    /// `chr_name` 当前所在片；`None` = 未登录/未进世界/已离开。
    #[must_use]
    pub(crate) fn shard_of(&self, chr_name: &str) -> Option<ShardId> {
        let map = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.get(chr_name).copied()
    }

    /// 目录条目数（测试/诊断用）。
    #[cfg(test)]
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        let map = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.len()
    }
}

/// 一条事件的投递目标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RouteTarget {
    /// 投给**全部**片的 sim（网关线注册/注销）。
    Broadcast,
    /// 投给一个片（未知角色回落 [`HOST_SHARD`]）。
    One(ShardId),
}

/// 路由判定（纯函数；`dir` 只在带角色名的事件上被查询）。
#[must_use]
pub(crate) fn route_target(ev: &GateEvent, dir: &ShardDirectory) -> RouteTarget {
    match ev {
        GateEvent::RegisterGate { .. } | GateEvent::UnregisterGate { .. } => RouteTarget::Broadcast,
        GateEvent::Command { chr_name, .. } | GateEvent::Closed { chr_name, .. } => {
            RouteTarget::One(dir.shard_of(chr_name).unwrap_or(HOST_SHARD))
        }
        GateEvent::KickAccount { account, .. } => {
            RouteTarget::One(dir.shard_of(account).unwrap_or(HOST_SHARD))
        }
        GateEvent::AdminGm { chr, .. } => RouteTarget::One(dir.shard_of(chr).unwrap_or(HOST_SHARD)),
        // `Opened` / `CertificationPending` / `LoadDbRequest`：没有全局唯一会话键，
        // 只能进主机（登录链在片 0 完成；见模块头）。
        _ => RouteTarget::One(HOST_SHARD),
    }
}

/// 进世界时记目录（`None` ⇒ 空操作：N=1）。
pub(crate) fn note_entered(dir: Option<&ShardDirectory>, chr_name: &str, shard: ShardId) {
    if let Some(dir) = dir {
        dir.set(chr_name, shard);
    }
}

/// 离开世界时清目录（`None` ⇒ 空操作：N=1）。
pub(crate) fn note_left(dir: Option<&ShardDirectory>, chr_name: &str) {
    if let Some(dir) = dir {
        dir.remove(chr_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AdminGmReply;

    fn cmd(chr: &str) -> GateEvent {
        GateEvent::Command {
            index: 0,
            chr_name: chr.to_string(),
            socket: 1,
            cmd: crate::CommandMessage {
                recog: 0,
                ident: 0,
                param: 0,
                tag: 0,
                series: 0,
            },
            tail: None,
        }
    }

    fn s1() -> ShardId {
        ShardId::new(1)
    }

    fn dir_with(entries: &[(&str, ShardId)]) -> ShardDirectory {
        let dir = ShardDirectory::new();
        for (chr, shard) in entries {
            dir.set(chr, *shard);
        }
        dir
    }

    /// **广播牙**：网关线的注册/注销必须投给**每一片**（否则非 0 片无法回帧）。
    ///
    /// 红检：把 `route_target` 的 `RegisterGate|UnregisterGate` 臂删掉（落到 `_` 的片 0）
    /// ⇒ 本用例红。
    #[test]
    fn gate_line_events_broadcast() {
        let dir = ShardDirectory::new();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let ev = GateEvent::RegisterGate { service_id: 3, tx };
        assert_eq!(route_target(&ev, &dir), RouteTarget::Broadcast);
        let ev = GateEvent::UnregisterGate { service_id: 3 };
        assert_eq!(route_target(&ev, &dir), RouteTarget::Broadcast);
    }

    /// **单播牙**：目录命中的角色事件只进该片；目录缺失一律回落片 0。
    ///
    /// 红检：把 `Command` 臂里的 `dir.shard_of(...)` 换成恒 `HOST_SHARD` ⇒ 已知角色那条红。
    #[test]
    fn character_events_follow_directory_and_default_to_host() {
        let dir = dir_with(&[("hero", s1())]);
        assert_eq!(route_target(&cmd("hero"), &dir), RouteTarget::One(s1()));
        assert_eq!(
            route_target(&cmd("nobody"), &dir),
            RouteTarget::One(HOST_SHARD),
            "未登录/未进世界 ⇒ 片 0"
        );
        let closed = GateEvent::Closed {
            index: 0,
            chr_name: "hero".to_string(),
            socket: 1,
        };
        assert_eq!(route_target(&closed, &dir), RouteTarget::One(s1()));
        let kick = GateEvent::KickAccount {
            account: "hero".to_string(),
            session_id: 9,
        };
        assert_eq!(route_target(&kick, &dir), RouteTarget::One(s1()));
        let (reply, _rx) = tokio::sync::oneshot::channel::<AdminGmReply>();
        let admin = GateEvent::AdminGm {
            chr: "hero".to_string(),
            cmd: "@who".to_string(),
            reply,
        };
        assert_eq!(route_target(&admin, &dir), RouteTarget::One(s1()));
    }

    /// **片 0 兜底牙**：无角色名的事件（`Opened`/`CertificationPending`）只进片 0。
    ///
    /// 红检：把 `_` 臂改成 `Broadcast` 或按 index 轮转 ⇒ 本用例红。
    #[test]
    fn pre_login_events_go_to_host() {
        let dir = ShardDirectory::new();
        let opened = GateEvent::Opened {
            index: 7,
            ip: "127.0.0.1".to_string(),
        };
        assert_eq!(route_target(&opened, &dir), RouteTarget::One(HOST_SHARD));
        let pending = GateEvent::CertificationPending {
            index: 7,
            ip: "127.0.0.1".to_string(),
        };
        assert_eq!(route_target(&pending, &dir), RouteTarget::One(HOST_SHARD));
    }

    /// **关服信号牙**：N=1 的 `Receiver` 与 N>1 的共享 `Flag` 语义相同（`try_recv().is_ok()`）。
    ///
    /// 红检：把 `ShutdownSignal::requested` 的 `Flag` 支写成恒 `false` ⇒ 第二条断言红。
    #[test]
    fn shutdown_signal_receiver_and_flag_have_same_semantics() {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let sig: crate::ShutdownSignal = rx.into();
        assert!(!sig.requested(), "未发信号 ⇒ 不关服（= 原 try_recv Err）");
        tx.send(()).expect("发送关服信号");
        assert!(sig.requested(), "收到信号 ⇒ 关服（= 原 try_recv Ok）");

        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sig = crate::ShutdownSignal::Flag(std::sync::Arc::clone(&flag));
        assert!(!sig.requested());
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(sig.requested());
    }

    /// **目录生命周期牙**：进世界写入、离开移除（幂等）；`None` 目录全空操作。
    #[test]
    fn directory_lifecycle_and_optional_noop() {
        let dir = ShardDirectory::new();
        note_entered(Some(&dir), "hero", s1());
        assert_eq!(dir.len(), 1);
        assert_eq!(dir.shard_of("hero"), Some(s1()));
        note_left(Some(&dir), "hero");
        note_left(Some(&dir), "hero");
        assert_eq!(dir.len(), 0);
        assert_eq!(dir.shard_of("hero"), None);

        note_entered(None, "hero", s1());
        note_left(None, "hero");
    }
}
