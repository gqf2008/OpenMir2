//! 玩家 `Run()` 的 **G-B 组**待发队列（P4 地图/饥饿 + P5 复活 + P27 的 `SysMsg`）。
//!
//! # 为什么单独一个组
//!
//! oracle 的 `PlayObject.Run()`（`src/M2Server/Player/PlayObject.Message.cs`）里：
//!
//! - **P4**：`:569-594` 地图 `DECHP/INCHP`、`:596-635` 饥饿系统；
//! - **P27**：`:636-657` 经验/攻击倍率 1s 递减（两条 `SysMsg`）；
//! - **P5**：`:747-763` 复活戒指；
//! - **P6**：`base.Run()` 的消息段（`BaseObject.Base.cs:26-30`）把上面入队的消息**按
//!   `(byte)wIdent` 优先级出队**。
//!
//! 这些生产者在**同一个** `MsgQueue` 里，因此组内后到者可能替换先到者（`SendUpdateMsg`
//! 头-only 去重），出队序由优先级决定：
//!
//! | RM ident | 低字节 | SM 帧 |
//! |---|---:|---|
//! | `RM_HEALTHSPELLCHANGED` (10052) | 68 | `SM_HEALTHSPELLCHANGED` (53) |
//! | `RM_DURACHANGE` (10125) | 141 | `SM_DURACHANGE` (642) |
//! | `RM_MYSTATUS` (8102) | 166 | `SM_MYSTATUS` (708) |
//! | `RM_SYSMESSAGE` (11004) | 252 | `SM_SYSMESSAGE` (100) |
//!
//! # 模型
//!
//! 每 actor 一个 [`NetPriorityQueue`]（元素 = [`GroupFrame`]，优先级 = RM ident 低字节）。
//! **载荷不随帧保存**的字段（`HealthSpell` 的 HP/MP/MaxHP、`MyStatus` 的状态值）在 flush 时
//! 才读当前 `WAbil`/`HungerStatus`——oracle 在 `Operate`（出队）时才拼包
//! （`PlayObject.Message.cs:1736-1739` / `:1949-1950`）。
//!
//! `HealthSpellChanged()`（`BaseObject.cs:700-710`）：
//! 1. `Race == Play` ⇒ `SendUpdateMsg(53)`：头-only 去重后入队一条（见 [`send_update_msg`]）；
//! 2. `ShowHp` ⇒ `SendRefMsg(53)`：**不去重**，源自己 + 观察者各一条——本模型把源自己那份
//!    入同一队列，观察者份数记在 `ref_copies`，由 flush 统一广播。
//!
//! **已登记的简化**（与 [`crate::healthspell`] 同口径，不是静默）：
//! - 观察者的自有队列未建模：观察者副本在生产点计数、flush 时统一发（oracle 在各收件人
//!   出队时刻读源 `WAbil`；同一组内源 `WAbil` 不再变化 ⇒ 载荷一致）。
//! - `< 68` 的队首 blocker（`RM_HIT(20)` 等）不在本模型内；纯函数
//!   [`crate::msgqueue::send_update_msg`] 已按 oracle 头-only 语义实现，blocker 的单测见
//!   `msgqueue::tests::c9_send_update_msg_peeks_head_only`。

use crate::msgqueue::NetPriorityQueue;
use std::collections::BTreeMap;

/// `RM_HEALTHSPELLCHANGED`（10052）。
pub const RM_HEALTHSPELLCHANGED: u16 = mir2_protocol::messages::RM_HEALTHSPELLCHANGED;
/// `RM_DURACHANGE`（10125）。
pub const RM_DURACHANGE: u16 = mir2_protocol::messages::RM_DURACHANGE;
/// `RM_MYSTATUS`（8102）。
pub const RM_MYSTATUS: u16 = mir2_protocol::messages::RM_MYSTATUS;
/// `RM_SYSMESSAGE`（11004）。
pub const RM_SYSMESSAGE: u16 = mir2_protocol::messages::RM_SYSMESSAGE;

/// 一条待发的下行（payload 在 flush 时按 oracle 出队时刻补齐）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupFrame {
    /// `RM_HEALTHSPELLCHANGED` → `SM_HEALTHSPELLCHANGED`（53）。
    HealthSpell,
    /// `RM_DURACHANGE` → `SM_DURACHANGE`（642）。`slot` 是 `ItemLocation`（0..12）。
    DuraChange { slot: u8, dura: u16, dura_max: u16 },
    /// `RM_MYSTATUS` → `SM_MYSTATUS`（708）；状态值在 flush 时读 `HungerStatus`。
    MyStatus,
    /// `RM_SYSMESSAGE` → `SM_SYSMESSAGE`（100）。`text` 是 GBK 字节。
    SysMessage {
        f_color: u16,
        b_color: u16,
        text: Vec<u8>,
    },
}

impl GroupFrame {
    /// oracle 入队时的 `wIdent`（优先级 = 低字节）。
    #[must_use]
    pub fn rm_ident(&self) -> u16 {
        match self {
            Self::HealthSpell => RM_HEALTHSPELLCHANGED,
            Self::DuraChange { .. } => RM_DURACHANGE,
            Self::MyStatus => RM_MYSTATUS,
            Self::SysMessage { .. } => RM_SYSMESSAGE,
        }
    }
}

/// 一个 actor 的 G-B 待发槽。
#[derive(Default)]
struct Slot {
    queue: NetPriorityQueue<GroupFrame>,
    /// `SendRefMsg(53)` 的观察者副本数（不含源自己：源自己那份在 `queue` 里）。
    ref_copies: u8,
}

/// 每 actor 一个 G-B 队列；只在组边界 [`Self::take`]。
#[derive(Default)]
pub struct RunFrames {
    slots: BTreeMap<i32, Slot>,
}

impl RunFrames {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `BaseObject.HealthSpellChanged()`（`BaseObject.cs:700-710`）的一次调用。
    pub fn health_spell_changed(&mut self, actor_id: i32, show_hp: bool) {
        let slot = self.slots.entry(actor_id).or_default();
        send_update_msg(&mut slot.queue, RM_HEALTHSPELLCHANGED);
        if show_hp {
            // `SendRefMsg` 不是 `SendUpdateMsg`：直接入队、不去重；源自己也收一份。
            slot.queue.enqueue(
                GroupFrame::HealthSpell,
                (RM_HEALTHSPELLCHANGED & 0xFF) as u8,
            );
            slot.ref_copies = slot.ref_copies.saturating_add(1);
        }
    }

    /// `SendMsg(actor, ident, …)` 入队（不去重）。
    pub fn enqueue(&mut self, actor_id: i32, frame: GroupFrame) {
        let prio = (frame.rm_ident() & 0xFF) as u8;
        self.slots
            .entry(actor_id)
            .or_default()
            .queue
            .enqueue(frame, prio);
    }

    /// 组边界：按优先级取走该 actor 的全部待发帧 + 观察者副本数。
    #[must_use]
    pub fn take(&mut self, actor_id: i32) -> (Vec<GroupFrame>, u8) {
        let Some(slot) = self.slots.remove(&actor_id) else {
            return (Vec::new(), 0);
        };
        let mut queue = slot.queue;
        let mut frames = Vec::with_capacity(queue.len());
        while let Some(frame) = queue.dequeue() {
            frames.push(frame);
        }
        (frames, slot.ref_copies)
    }

    /// 当前有 pending 的 actor（`BTreeMap` 升序，不依赖 `HashMap` 迭代序）。
    pub fn pending_actors(&self) -> impl Iterator<Item = i32> + '_ {
        self.slots.keys().copied()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// `BaseObject.SendUpdateMsg`（`BaseObject.Message.cs:283-305`）在 [`GroupFrame`] 队列上的等价物：
/// 只 `TryPeek` 队首并删同 ident 旧消息（`Count <= i` 预算），再入队一条新的。
fn send_update_msg(queue: &mut NetPriorityQueue<GroupFrame>, w_ident: u16) {
    let mut i = 0_usize;
    loop {
        if queue.len() <= i {
            break;
        }
        if queue.peek().is_some_and(|head| head.rm_ident() == w_ident) {
            let _ = queue.dequeue();
        }
        i += 1;
    }
    queue.enqueue(GroupFrame::HealthSpell, (w_ident & 0xFF) as u8);
}

#[cfg(test)]
mod tests {
    use super::{
        GroupFrame, RunFrames, RM_DURACHANGE, RM_HEALTHSPELLCHANGED, RM_MYSTATUS, RM_SYSMESSAGE,
    };

    /// 出队序按 RM ident 低字节：53(68) → DuraChange(141) → MyStatus(166) → SysMsg(252)，
    /// 与入队序无关。
    ///
    /// 牙：把 `enqueue` 的优先级改成 `rm_ident()` 全值（或插入序）⇒ 断言红。
    #[test]
    fn flush_orders_by_rm_ident_low_byte() {
        let mut f = RunFrames::new();
        f.enqueue(
            7,
            GroupFrame::SysMessage {
                f_color: 0xFF,
                b_color: 0x38,
                text: b"x".to_vec(),
            },
        );
        f.enqueue(7, GroupFrame::MyStatus);
        f.enqueue(
            7,
            GroupFrame::DuraChange {
                slot: 3,
                dura: 4,
                dura_max: 5,
            },
        );
        f.health_spell_changed(7, false);
        let (frames, refs) = f.take(7);
        assert_eq!(refs, 0);
        let idents: Vec<u16> = frames.iter().map(GroupFrame::rm_ident).collect();
        assert_eq!(
            idents,
            vec![
                RM_HEALTHSPELLCHANGED,
                RM_DURACHANGE,
                RM_MYSTATUS,
                RM_SYSMESSAGE
            ]
        );
        assert!(f.is_empty(), "take 必须清槽");
    }

    /// G-B 同组 `HealthSpellChanged` 头-only 去重：同 actor 连续两次（`ShowHp=false`）
    /// 只留 1 条 pending。
    ///
    /// 牙：把 `send_update_msg` 改成「追加」⇒ 断言红；改成「全队列扫描」在多条 53 时也会红
    /// （见 `msgqueue` 的 C9）。
    #[test]
    fn same_group_healthspell_overwrites_pending() {
        let mut f = RunFrames::new();
        f.health_spell_changed(3, false);
        f.health_spell_changed(3, false);
        let (frames, refs) = f.take(3);
        assert_eq!(refs, 0);
        assert_eq!(frames, vec![GroupFrame::HealthSpell]);
    }

    /// `ShowHp`：一次调用给自己 2 条（`SendUpdateMsg` + `SendRefMsg` 自发），观察者 1 份。
    ///
    /// 牙：去掉自发副本或把去重做成「1 条」⇒ 断言红。
    #[test]
    fn show_hp_repeated_calls_accumulate_like_healthspell_slots() {
        let mut f = RunFrames::new();
        f.health_spell_changed(9, true);
        let (first, refs) = f.take(9);
        assert_eq!(first.len(), 2);
        assert_eq!(refs, 1);

        // 与 `healthspell::HealthSpellSlots` 同口径：重复调用按 ceil 预算收敛到 2–3 条。
        f.health_spell_changed(9, true);
        f.health_spell_changed(9, true);
        f.health_spell_changed(9, true);
        let (frames, refs) = f.take(9);
        assert_eq!(frames.len(), 3, "ceil 预算：第 3 次调用后 pending=3");
        assert_eq!(refs, 3);
    }

    /// P4 的 53 与 P26 跨档 `MyStatus` 同组 ⇒ 帧序 53 → 708（按 ident 低字节）。
    #[test]
    fn p25_healthspell_precedes_p26_mystatus() {
        let mut f = RunFrames::new();
        f.enqueue(5, GroupFrame::MyStatus);
        f.health_spell_changed(5, false);
        let (frames, _) = f.take(5);
        let idents: Vec<u16> = frames.iter().map(GroupFrame::rm_ident).collect();
        assert_eq!(idents, vec![RM_HEALTHSPELLCHANGED, RM_MYSTATUS]);
    }
}
