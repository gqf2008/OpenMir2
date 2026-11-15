//! 卡 `openmir2-gateway-backpressure-frame-drop`：**per-gate 有界发送队列**（总监裁决 A+B 的 A 部分）。
//!
//! 背景：oracle `ChannelMessageHandler.ProcessBufferSend` 在 `SendChecked > 0`（等网关回
//! `GM_RECEIVE_OK`，`:58-66`）时**无条件 `return`**——帧直接丢、不排队、无计数。2026-11-15
//! 03:00 窗口 716 条 `SM_LOGON` 里 330 Send / 386 Hold，bot 侧 `in_world=330`，即 386 个会话
//! 因背压永远进不了世界。总监裁决取 **A+B**：
//!
//! - **A（本模块）**：`Hold` 的帧按 FIFO 入本 gate 线的有界队列，等 `GM_RECEIVE_OK` 回显或
//!   `SocCheckTimeOut`(50ms) 清零后按序补发；稳态队列深度接近 0，只有登录洪峰才短暂堆积。
//! - **B（溢出）**：队列满（字节数或帧数）时**拒绝**该帧——计数 + 结构化日志 + 对该会话发
//!   `GM_CLOSE`（客户端可见断开），**不是静默丢**（见 `send_gate_msg` 的 overflow 分支）。
//!
//! ## 顺序硬约束（总监裁决）
//!
//! **flush 只在 `SendChecked` 清零后、任何新帧直发之前完成**。原因：队列是所有会话共享的
//! 一条 gate FIFO，若清零后先直发新帧、再补发积压帧，同一会话的 `SM_SERVERUSERINDEX →
//! SM_LOGON` 会乱序——**乱序比丢帧更难查，因为不报错**。落地方式见 `send_gate_msg`：
//! 队列非空时新帧**一律先入队**，再由 `flush_pending` 统一按 FIFO 写出；直发路径只在队列
//! 为空时可达。`lib.rs` 的实现与注释都重复这条约束，防止后人把「队列是否为空」的判断挪错。
//!
//! ## 内存上界
//!
//! 默认 [`DEFAULT_QUEUE_BYTES`] = **256 KiB/gate**（≥ 触发阈值 `CheckBlock*10` ≈ 120KB 的 2 倍），
//! 帧数上限 [`DEFAULT_QUEUE_FRAMES`] = 4096/gate。`GATE_SLOT_CAPACITY` = 20 条 gate 线 ⇒
//! 最坏 **5 MiB**。两个上限都可用环境变量收紧/放宽（见 [`ENV_QUEUE_BYTES`] /
//! [`ENV_QUEUE_FRAMES`]，解析失败回落默认值，不 panic）。
//!
//! ## 回退（D-* 登记项）
//!
//! `MIR2_GATE_SEND_QUEUE=0`（或任意一个容量设 0）⇒ 本队列**禁用**：`Hold` 帧恢复 oracle 的
//! 「直接丢」，但仍走既有观测面（计数 + 限速日志），因此不会退回「静默丢帧」。
//! 帧面/协议不变，改动可整体 revert。

use std::collections::VecDeque;

/// 发送队列开关（`0`/`false`/`off`/`no` ⇒ 关闭，回退 oracle 直接丢）。默认开。
pub(crate) const ENV_QUEUE_ENABLED: &str = "MIR2_GATE_SEND_QUEUE";

/// per-gate 队列字节上限（默认 [`DEFAULT_QUEUE_BYTES`]；`0` ⇒ 关闭队列）。
pub(crate) const ENV_QUEUE_BYTES: &str = "MIR2_GATE_QUEUE_BYTES";

/// per-gate 队列帧数上限（默认 [`DEFAULT_QUEUE_FRAMES`]；`0` ⇒ 关闭队列）。
pub(crate) const ENV_QUEUE_FRAMES: &str = "MIR2_GATE_QUEUE_FRAMES";

/// 默认字节上限：256 KiB/gate（20 条线最坏 5 MiB；见模块文档）。
pub(crate) const DEFAULT_QUEUE_BYTES: usize = 256 * 1024;

/// 默认帧数上限：防「大量小帧」只按字节统计撑爆计数（与 256KiB 一起构成双上限）。
pub(crate) const DEFAULT_QUEUE_FRAMES: usize = 4096;

/// 一次入队的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnqueueOutcome {
    /// 已入队（调用方随后应在安全点 `flush`）。
    Queued,
    /// 队列被开关/零容量禁用 ⇒ 调用方按 oracle「直接丢」处理（仍计数/日志）。
    Disabled,
    /// 队列已满（字节或帧数）⇒ 调用方按 B 策略拒绝（计数 + 明确失败）。
    Overflow,
}

/// 有界 FIFO。**单线程持有**（`gate_conn` 是每条 gate 线唯一的写者）⇒ 无需锁。
#[derive(Debug)]
pub(crate) struct PendingQueue {
    frames: VecDeque<Vec<u8>>,
    bytes: usize,
    max_bytes: usize,
    max_frames: usize,
    enabled: bool,
}

impl Default for PendingQueue {
    /// 生产默认值（**不读环境变量**；`gate_conn` 用 [`Self::from_env`] 覆盖）。
    /// 单测直接构造时行为确定，不受测试进程环境干扰。
    fn default() -> Self {
        Self::new(true, DEFAULT_QUEUE_BYTES, DEFAULT_QUEUE_FRAMES)
    }
}

impl PendingQueue {
    /// 显式构造（`enabled=false` 或任一容量为 0 ⇒ 等价禁用）。
    pub(crate) fn new(enabled: bool, max_bytes: usize, max_frames: usize) -> Self {
        let enabled = enabled && max_bytes > 0 && max_frames > 0;
        Self {
            frames: VecDeque::new(),
            bytes: 0,
            max_bytes,
            max_frames,
            enabled,
        }
    }

    /// 从环境变量构造（非法值回落默认；`0` ⇒ 禁用）。
    pub(crate) fn from_env() -> Self {
        let enabled = std::env::var_os(ENV_QUEUE_ENABLED)
            .as_deref()
            .and_then(parse_flag)
            .unwrap_or(true);
        let max_bytes = std::env::var_os(ENV_QUEUE_BYTES)
            .as_deref()
            .and_then(parse_usize)
            .unwrap_or(DEFAULT_QUEUE_BYTES);
        let max_frames = std::env::var_os(ENV_QUEUE_FRAMES)
            .as_deref()
            .and_then(parse_usize)
            .unwrap_or(DEFAULT_QUEUE_FRAMES);
        Self::new(enabled, max_bytes, max_frames)
    }

    /// 入队（FIFO）。`Overflow` 时**不改变队列**，帧由调用方按 B 策略拒绝。
    pub(crate) fn enqueue(&mut self, wire: Vec<u8>) -> EnqueueOutcome {
        if !self.enabled {
            return EnqueueOutcome::Disabled;
        }
        if self.frames.len() >= self.max_frames
            || self.bytes.saturating_add(wire.len()) > self.max_bytes
        {
            return EnqueueOutcome::Overflow;
        }
        self.bytes += wire.len();
        self.frames.push_back(wire);
        EnqueueOutcome::Queued
    }

    /// 取出队首（补发成功后调用；调用方负责 `send_raw`）。
    pub(crate) fn pop_front(&mut self) -> Option<Vec<u8>> {
        let frame = self.frames.pop_front()?;
        self.bytes = self.bytes.saturating_sub(frame.len());
        Some(frame)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.frames.len()
    }

    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    pub(crate) fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    pub(crate) fn max_frames(&self) -> usize {
        self.max_frames
    }

    /// 是否启用（回退开关关闭时 `false`）。
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }
}

/// 解析布尔开关：`1/true/on/yes` ⇒ true，`0/false/off/no` ⇒ false，其余 ⇒ `None`（回落默认）。
fn parse_flag(raw: &std::ffi::OsStr) -> Option<bool> {
    let s = raw.to_str()?.trim().to_ascii_lowercase();
    match s.as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

/// 解析非负整数（非法 ⇒ `None` 回落默认，不 panic）。
fn parse_usize(raw: &std::ffi::OsStr) -> Option<usize> {
    raw.to_str()?.trim().parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(max_bytes: usize, max_frames: usize) -> PendingQueue {
        PendingQueue::new(true, max_bytes, max_frames)
    }

    /// 基本 FIFO：入队顺序 = 出队顺序；字节数随入队/出队增减。
    #[test]
    fn fifo_order_and_byte_accounting() {
        let mut q = q(1_000, 10);
        assert_eq!(q.enqueue(vec![1]), EnqueueOutcome::Queued);
        assert_eq!(q.enqueue(vec![2, 2, 2]), EnqueueOutcome::Queued);
        assert_eq!((q.len(), q.bytes()), (2, 4));
        assert_eq!(q.pop_front(), Some(vec![1]));
        assert_eq!((q.len(), q.bytes()), (1, 3));
        assert_eq!(q.pop_front(), Some(vec![2, 2, 2]));
        assert!(q.is_empty());
        assert_eq!(q.pop_front(), None);
    }

    /// 字节上限是**闭区间**：正好填满可以，再多 1 字节就 Overflow；溢出帧不入队。
    #[test]
    fn byte_limit_is_inclusive_and_overflow_keeps_queue() {
        let mut q = q(10, 10);
        assert_eq!(q.enqueue(vec![0; 10]), EnqueueOutcome::Queued);
        assert_eq!(q.enqueue(vec![0; 1]), EnqueueOutcome::Overflow);
        assert_eq!(q.len(), 1, "溢出帧不得入队");
        assert_eq!(q.bytes(), 10);
        assert_eq!(q.pop_front().map(|f| f.len()), Some(10));
        assert_eq!(
            q.enqueue(vec![0; 10]),
            EnqueueOutcome::Queued,
            "腾出空间后可再入队"
        );
    }

    /// 帧数上限：第 N+1 帧溢出（即使字节还有空间）。
    #[test]
    fn frame_limit_bounds_small_frames() {
        let mut q = q(1_000_000, 2);
        assert_eq!(q.enqueue(vec![0]), EnqueueOutcome::Queued);
        assert_eq!(q.enqueue(vec![0]), EnqueueOutcome::Queued);
        assert_eq!(q.enqueue(vec![0]), EnqueueOutcome::Overflow);
        assert_eq!(q.len(), 2);
    }

    /// 回退开关：禁用后一律 `Disabled`（调用方按 oracle 直接丢 + 计数），队列永不生长。
    #[test]
    fn disabled_queue_never_enqueues() {
        let mut q = PendingQueue::new(false, 1_000, 10);
        assert_eq!(q.enqueue(vec![1, 2, 3]), EnqueueOutcome::Disabled);
        assert!(q.is_empty());
        // 容量 0 也等价禁用（避免「零容量 ⇒ 每帧都拒绝+断开」的脚枪）。
        let mut zero = PendingQueue::new(true, 0, 10);
        assert_eq!(zero.enqueue(vec![1]), EnqueueOutcome::Disabled);
        assert!(!zero.enabled());
    }

    /// 退化输入：零字节帧仍可入队/出队（判据不能靠「长度>0」隐式跳过）。
    #[test]
    fn zero_sized_frame_is_queued() {
        let mut q = q(10, 10);
        assert_eq!(q.enqueue(Vec::new()), EnqueueOutcome::Queued);
        assert_eq!(q.bytes(), 0);
        assert_eq!(q.len(), 1);
        assert_eq!(q.pop_front(), Some(Vec::new()));
    }

    /// 环境变量解析的纯函数（不 set_var，避免测试进程环境污染）。
    #[test]
    fn env_parsers_fall_back_on_garbage() {
        use std::ffi::OsStr;
        assert_eq!(parse_flag(OsStr::new("0")), Some(false));
        assert_eq!(parse_flag(OsStr::new(" OFF ")), Some(false));
        assert_eq!(parse_flag(OsStr::new("1")), Some(true));
        assert_eq!(parse_flag(OsStr::new("Yes")), Some(true));
        assert_eq!(parse_flag(OsStr::new("maybe")), None);
        assert_eq!(parse_usize(OsStr::new("262144")), Some(262_144));
        assert_eq!(parse_usize(OsStr::new("-1")), None);
        assert_eq!(parse_usize(OsStr::new("garbage")), None);
    }
}
