//! oracle `BaseObject.HealthSpellChanged()` 的**待发 53**槽模型
//! （`src/M2Server/Actor/BaseObject.cs:700-710` + `BaseObject.Message.cs:283-305`）。
//!
//! # oracle 语义（逐行）
//!
//! ```csharp
//! public void HealthSpellChanged() {
//!     if (Race == ActorRace.Play)
//!         SendUpdateMsg(RM_HEALTHSPELLCHANGED, 0, 0, 0, 0, "");   // 入自己的队列（优先级 68）
//!     if (ShowHp)
//!         SendRefMsg(RM_HEALTHSPELLCHANGED, 0, 0, 0, 0, "");       // 同图 ±range，含源自己
//! }
//! ```
//!
//! `SendUpdateMsg` 只 `TryPeek` **队首**并按 `Count <= i` 预算删除同 ident 旧消息
//! （见 [`crate::msgqueue::send_update_msg`]）；`SendRefMsg` **不去重**，直接给每个收件人
//! （含源自己）入队一条。帧载荷不随消息保存：`PlayObject.Message.cs:1736-1739` 在**出队时**
//! 用当时的 `WAbil.HP/MP/MaxHP` 拼包。
//!
//! # 本模型
//!
//! 每 actor 一个 [`NetPriorityQueue<u16>`]（元素 = `wIdent`，优先级 = `(byte)wIdent`）。
//! 生产点只 `mark`（入队/去重），**组边界**才 [`HealthSpellSlots::take`] 取走条数，
//! 由调用方在出队时刻读当前 `WAbil` 拼帧。
//!
//! **已登记的简化**（不是静默）：
//! - 本模型只装 53，不装 <68 的其它下行（`RM_HIT(20)`/`RM_DEATH(37)`/`RM_ABILITY(67)` 等）。
//!   因此队首 blocker 在**生产路径**上恒不存在（= 每个组至多一条 pending）；纯函数
//!   [`crate::msgqueue::send_update_msg`] 已按 oracle 头-only 语义实现，blocker 层的单测见
//!   `msgqueue::tests::c9_send_update_msg_peeks_head_only`。
//! - 观察者队列各自为政（每个收件人独立 68 元素）；本模型只统计「源 actor 发出几次
//!   `SendRefMsg`」⇒ 每个观察者副本数一致，跨度内不出现「源 WAbil 在两次出队之间变化」。
//! - 跨 tick 携带（G-C 的 pending 被下一 Run 的 P1 恢复替换）未建模；见 `drain_stamina` 的登记。

use crate::msgqueue::NetPriorityQueue;
use std::collections::BTreeMap;

/// `RM_HEALTHSPELLCHANGED`（10052）。
pub const RM_HEALTHSPELLCHANGED: u16 = mir2_protocol::messages::RM_HEALTHSPELLCHANGED;

/// 消息优先级 = `(byte)wIdent`（`BaseObject.Message.cs:94`）：10052 ⇒ **68**。
pub const P_HEALTHSPELLCHANGED: u8 = (RM_HEALTHSPELLCHANGED & 0xFF) as u8;

/// 一个 actor 的待发槽。
#[derive(Default)]
struct Slot {
    /// 待发消息队列（本模型只装 53）。
    queue: NetPriorityQueue<u16>,
    /// `SendRefMsg(53)` 的**观察者**副本数（不含源自己：源自己那份已进 `queue`）。
    ref_copies: u8,
}

/// 每 actor 一个「待发 53」槽；只在组边界 flush（[`Self::take`]）。
#[derive(Default)]
pub struct HealthSpellSlots {
    slots: BTreeMap<i32, Slot>,
}

impl HealthSpellSlots {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `BaseObject.HealthSpellChanged()`（`BaseObject.cs:700-710`）的一次调用：
    /// `Race==Play` ⇒ `SendUpdateMsg(53)`；`ShowHp` ⇒ 再 `SendRefMsg(53)`（含源自己）。
    ///
    /// 同一组内后到的生产者覆盖先到的 pending（oracle 头-only 去重；队首无 <68 blocker 时
    /// `SendUpdateMsg` 恰好删掉那一条）。
    pub fn health_spell_changed(&mut self, actor_id: i32, show_hp: bool) {
        let slot = self.slots.entry(actor_id).or_default();
        crate::msgqueue::send_update_msg(&mut slot.queue, RM_HEALTHSPELLCHANGED);
        if show_hp {
            // `SendRefMsg` 不是 `SendUpdateMsg`：直接入队、不去重；源自己也收一份。
            slot.queue
                .enqueue(RM_HEALTHSPELLCHANGED, P_HEALTHSPELLCHANGED);
            slot.ref_copies = slot.ref_copies.saturating_add(1);
        }
    }

    /// 组边界 flush：取走该 actor 的 `(自见帧数, 观察者副本数)`；无槽 ⇒ `(0, 0)`。
    ///
    /// 自见帧数 = oracle 队列里 53 的条数（`SendUpdateMsg` 一条 + `ShowHp` 的 `SendRefMsg`
    /// 自发一条；多次调用时按 `ceil(N/2)` 预算演化，稳态 2–3 条）。
    #[must_use]
    pub fn take(&mut self, actor_id: i32) -> (usize, u8) {
        let Some(slot) = self.slots.remove(&actor_id) else {
            return (0, 0);
        };
        let mut queue = slot.queue;
        let mut self_frames = 0_usize;
        while let Some(ident) = queue.dequeue() {
            if ident == RM_HEALTHSPELLCHANGED {
                self_frames += 1;
            }
        }
        (self_frames, slot.ref_copies)
    }

    /// 当前有 pending 的 actor（按 `BTreeMap` 的 actor id 升序，不依赖 `HashMap` 迭代序）。
    pub fn pending_actors(&self) -> impl Iterator<Item = i32> + '_ {
        self.slots.keys().copied()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::HealthSpellSlots;

    /// **C8**：`ShowHp=true` 时**一次** `HealthSpellChanged()` 会给自己入 **2 条**
    /// （`SendUpdateMsg` + `SendRefMsg` 自发副本），观察者各 1 条；同一字节。
    ///
    /// 牙（改坏必红）：去掉 `SendRefMsg` 的自发副本（或把 `SendUpdateMsg` 去重成 1 条）⇒ 断言红。
    #[test]
    fn c8_show_hp_single_call_emits_two_self_frames_and_one_observer_copy() {
        let mut slots = HealthSpellSlots::new();
        slots.health_spell_changed(7, true);
        let (self_frames, ref_copies) = slots.take(7);
        assert_eq!(self_frames, 2, "SendUpdateMsg 1 + SendRefMsg 自发 1");
        assert_eq!(ref_copies, 1, "观察者 1 份");
        assert!(slots.is_empty(), "flush 必须清槽");
    }

    /// 头-only 去重的**组内覆盖**：同一 actor 同组两次 `HealthSpellChanged()`、`ShowHp=false`
    /// ⇒ pending 仍 1 条（后到者覆盖）。
    ///
    /// 牙：把 `send_update_msg` 改成「追加不去重」⇒ 断言红。
    #[test]
    fn group_later_producer_overwrites_pending() {
        let mut slots = HealthSpellSlots::new();
        slots.health_spell_changed(3, false);
        slots.health_spell_changed(3, false);
        assert_eq!(slots.take(3), (1, 0));
    }

    /// `ShowHp` 多次调用的堆序累积：每次 1 条 `SendUpdateMsg` + 1 条自发 `SendRefMsg`；
    /// `SendUpdateMsg` 只删队首一半 ⇒ 稳态 2–3 条（oracle 缺陷照抄，不修）。
    ///
    /// 牙：把 `send_update_msg` 改成「全队列扫描去重」⇒ 每次稳定 1 条自发副本、断言红。
    #[test]
    fn show_hp_repeated_calls_accumulate_two_to_three_frames() {
        let mut slots = HealthSpellSlots::new();
        slots.health_spell_changed(9, true); // 2
        slots.health_spell_changed(9, true); // SendUpdateMsg 删 1 加 1 ⇒ 2，+自发 ⇒ 3
        slots.health_spell_changed(9, true); // 删 1 加 1 ⇒ 3，+自发 ⇒ 4? 见下一断言
        let (self_frames, _) = slots.take(9);
        // 逐次演算：2 →（删队首 1、加 1、加自发 1）= 3 →（删队首 1、加 1、加自发 1）= 4。
        // oracle 的 `Count <= i` 预算在「队首连续 3 条」时删 2 条：
        //   N=3：i=0 peek/删 → 2；i=1 peek/删 → 1；i=2 Count(1)<=2 停 ⇒ 1；再加 1 ⇒ 2；再加自发 ⇒ 3。
        // 故稳态为 2–3，而不是 4。本模型逐位照抄该预算。
        assert_eq!(self_frames, 3, "第 3 次调用后 pending=3（ceil 预算）");
    }
}
