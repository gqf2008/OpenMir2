//! **L2-5 ③ 段 2（接线）**：handoff 的生产运行时——offer/ack 邮箱 + tick 顶排水 + 超时回滚。
//!
//! 协议（计划页 §2.2，两阶段、源从不放手）：
//!
//! - **T（源区）**：移动判定把实体挪进邻区 rect ⇒ `Flags.handoff_pending`
//!   （`World::set_zone_bounds` 驱动）；**barrier2 之后**由 [`HandoffRuntime::after_phase2`]
//!   向目标区邮箱投 `HandoffOffer`（全量 `MirrorSpec`）。
//! - **T+1 顶（目标区，phase1 之前）**：[`HandoffRuntime::tick_top`] 排水 offer ⇒
//!   `zone_of` 校验 ⇒ `World::promote_mirror_to_real`（同 id 镜像原地提升）⇒ 回 `HandoffAck`。
//! - **T+1 顶（源区）**：同一函数排 ack：`ok` ⇒ `World::release_entity`（源侧移除）；
//!   `reject`/超时 ⇒ 清 `handoff_pending`（回滚：实体留在源区、不再重发）+ 计数。
//!
//! **默认关闭**：邮箱只在 `MIR2_ZONE_SLICING=1` 时建；`zone_bounds` 也只在切片时设。
//! `slicing=0` ⇒ `handoff_pending` 恒 false（单区/N=1 逐字节不变）。
//!
//! **§2.2 的取舍（显式登记）**：超时/拒绝不做「退回原格」（原格已在另区，回格需再次
//! 跨界）——实体保留在源区的当前坐标继续模拟，并以 `handoff_refused` 计数 + WRN 暴露。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use mir2_world_bevy::{HandoffAccept, MirrorSpec, World};

use crate::glog;

use crate::zone::{RegionLayout, ZoneId};

/// §2.2：offer 后等待 ack 的最大 tick 数（超过 ⇒ 回滚 + `handoff_refused`）。
pub(crate) const HANDOFF_TIMEOUT_TICKS: u64 = 3;

/// **offer 去重表的回收窗口**（总监 2026-11-22 裁定「幂等键 = offer 身份」）。
///
/// 一次 pending epoch 内源区每 tick 重发，寿命受 [`HANDOFF_TIMEOUT_TICKS`] 约束（≤4 tick）；
/// 投递只在内存邮箱里、目标区每 tick 排水 ⇒ 迟到重发最多再晚 ~2 tick。TTL 取 2×寿命，
/// 保证「丢掉一行的时刻，该 epoch 不可能还有重发在路上」（表有界）。
pub(crate) const HANDOFF_DUP_TTL_TICKS: u64 = 2 * (HANDOFF_TIMEOUT_TICKS + 1);

/// 源区 → 目标区的提交申请（全量状态；目标区用它覆盖同 id 镜像）。
#[derive(Debug)]
pub(crate) struct HandoffOffer {
    /// 源区。
    pub from: ZoneId,
    /// 本次**发送**的 tick（诊断用；同一 epoch 的重发会变）。
    pub at_tick: u64,
    /// **offer 身份**：源端该 id 这一次 pending epoch 的起始 tick（同一次跨界的所有重发相同；
    /// 下一次跨界必然更大）。目标区按 `(from, id, epoch)` 去重——迟到重发不是新跨界。
    pub epoch: u64,
    /// 实体全量状态（与快照同一组字段）。
    pub spec: MirrorSpec,
}

/// 目标区 → 源区的回执。
#[derive(Debug, Clone, Copy)]
pub(crate) struct HandoffAck {
    /// 回执面向的源区。
    pub to: ZoneId,
    /// **接受方区**：真正执行提升/落真身的区（= 发回执的那个区）。
    ///
    /// 与 [`Self::to`] 方向相反，**不是**同一个值：`to` 是源区（收件人），本字段是目标区
    /// （A35-b 会话搬迁要按它把会话/路由迁到实体实际所在的区）。把 `to` 当目标区就是
    /// 「区号语义写反」，由 `handoff_tick_top_returns_committed_ids_from_accepting_zone`
    /// 的变异②直接抓。
    pub from: ZoneId,
    /// 实体 id。
    pub id: i32,
    /// `true` = 目标区已提升（源区可移除）。
    pub ok: bool,
}

/// **A38**：一次已提交的 handoff（`ack.ok` 且源区实体已移除）的随迁载荷。
///
/// `group` 必须在 `release_entity` **之前**捕获——实体释放后 `GroupState` 组件随之消失
/// （它是组关系在源区唯一的存在处）。捕获顺序由
/// `handoff_tick_top_captures_group_state_before_release` 直接钉住。
#[derive(Debug)]
pub(crate) struct CommittedHandoff {
    /// 实体 id。
    pub id: i32,
    /// 接受方区（= [`HandoffAck::from`]，真正执行提升的区）。
    pub from: ZoneId,
    /// 实体在源区被释放**之前**的组关系（`allow_group`/`allow_group_re_call`/`group_owner`/
    /// `group_members`；`GroupState::default()` = 无组）。
    pub group: mir2_world_bevy::GroupState,
}

/// 一个区的邮箱（offer / ack 各一条队列；区线程互相 push、各自 tick 顶排水）。
pub(crate) type HandoffMailbox = Arc<Mutex<(Vec<HandoffOffer>, Vec<HandoffAck>)>>;

/// 新建一组邮箱（区数 = 区数）。
pub(crate) fn new_mailboxes(zone_count: usize) -> Vec<HandoffMailbox> {
    (0..zone_count)
        .map(|_| Arc::new(Mutex::new((Vec::new(), Vec::new()))))
        .collect()
}

/// **区的单 span 边界**（供 `World::set_zone_bounds`）：多 span 区返回 `None`
/// （动作定义不明确 ⇒ 宁可不开 handoff；调用方记 WRN）。
pub(crate) fn bounds_for(
    layout: &RegionLayout,
    zone: ZoneId,
) -> Option<(usize, (i32, i32, i32, i32))> {
    let mut spans = layout.zone_spans().iter().filter(|s| s.zone == zone);
    let first = spans.next()?;
    if spans.next().is_some() {
        return None;
    }
    let r = first.rect;
    Some((first.map_id, (r.x0, r.y0, r.x1, r.y1)))
}

/// **每区一个的 handoff 运行时**（区线程内持有，无锁竞争）。
pub(crate) struct HandoffRuntime {
    zone: ZoneId,
    layout: Arc<RegionLayout>,
    boxes: Vec<HandoffMailbox>,
    /// 已发 offer 的实体 → 首次发送 tick（超时回滚用）。
    pending_since: HashMap<i32, u64>,
    /// 被拒绝/超时的回滚次数（正常恒 0）。
    refused: u64,
    /// 提升成功次数（本区作为**目标区**的读数）。
    promoted: u64,
    /// **诊断（不改变行为）**：RefusedCell 已打过的 id（同一 id 只打一次）。
    refused_cell_logged: std::collections::HashSet<i32>,
    /// **目标区去重表**（总监 2026-11-22 裁定）：`(源区, 实体 id) → (已处理的最大 offer epoch,
    /// 最近处理的 tick)`。`offer.epoch <= 已处理` ⇒ 迟到重发，直接丢弃。
    /// 按 [`HANDOFF_DUP_TTL_TICKS`] 回收 ⇒ 有界。
    last_offer_epoch: HashMap<(ZoneId, i32), (u64, u64)>,
    /// 本区作为**目标区**丢弃的迟到重发数（诊断/测试）。
    dup_dropped: u64,
}

impl HandoffRuntime {
    pub(crate) fn new(zone: ZoneId, layout: Arc<RegionLayout>, boxes: Vec<HandoffMailbox>) -> Self {
        Self {
            zone,
            layout,
            boxes,
            pending_since: HashMap::new(),
            refused: 0,
            promoted: 0,
            refused_cell_logged: std::collections::HashSet::new(),
            last_offer_epoch: HashMap::new(),
            dup_dropped: 0,
        }
    }

    /// 本区作为**源区**的回滚次数（诊断/测试）。
    #[cfg(test)]
    pub(crate) fn refused(&self) -> u64 {
        self.refused
    }

    /// 本区作为**目标区**的提升次数（诊断/测试）。
    #[cfg(test)]
    pub(crate) fn promoted(&self) -> u64 {
        self.promoted
    }

    /// 本区作为**目标区**丢弃的迟到重发数（诊断/测试）。
    #[cfg(test)]
    pub(crate) fn dup_dropped(&self) -> u64 {
        self.dup_dropped
    }

    fn take_mail(&self) -> (Vec<HandoffOffer>, Vec<HandoffAck>) {
        let Some(b) = self.boxes.get(self.zone.index()) else {
            return (Vec::new(), Vec::new());
        };
        let mut m = b.lock().unwrap_or_else(PoisonError::into_inner);
        (std::mem::take(&mut m.0), std::mem::take(&mut m.1))
    }

    fn send_ack(&self, to: ZoneId, id: i32, ok: bool) {
        if let Some(b) = self.boxes.get(to.index()) {
            let mut m = b.lock().unwrap_or_else(PoisonError::into_inner);
            m.1.push(HandoffAck {
                to,
                from: self.zone,
                id,
                ok,
            });
        }
    }

    /// **tick 顶（phase1 之前）**：排 ack（源侧提交/回滚）+ 排 offer（目标侧提升+回执）。
    ///
    /// 返回本 tick **已提交**（`ack.ok` 且实体已从本区移除）的 [`CommittedHandoff`]：
    /// 接受方区 = [`HandoffAck::from`]（真正执行提升的区），**不是** ack 的收件人 `to`；
    /// `group` = 实体被释放前的组关系（**A38**：跨区的只是位置，组关系跟着人走）。
    pub(crate) fn tick_top(&mut self, world: &mut World, tick: u64) -> Vec<CommittedHandoff> {
        let (offers, acks) = self.take_mail();
        let mut committed: Vec<CommittedHandoff> = Vec::new();
        // 1) acks（本区是源区）。
        for ack in acks {
            if ack.to != self.zone {
                continue;
            }
            // 同一 offer 会每 tick 重发（幂等），ack 也就可能多份：只认第一份（还在待提交表里）。
            if self.pending_since.remove(&ack.id).is_none() {
                continue;
            }
            if ack.ok {
                // **A38**：组关系必须在 `release_entity` **之前**取——释放后 `GroupState` 组件
                // 随实体消失。捕获顺序由 `handoff_tick_top_captures_group_state_before_release`
                // 钉住（把本行挪到 release 之后 ⇒ 拿到 default() ⇒ 该用例红）。
                let group = world.entities.group(ack.id).cloned().unwrap_or_default();
                let removed = world.release_entity(ack.id);
                glog!(
                    "INF",
                    "handoff 提交：区 {} 移除实体 {} (已提升 removed={removed}) tick={tick}",
                    self.zone,
                    ack.id
                );
                committed.push(CommittedHandoff {
                    id: ack.id,
                    from: ack.from,
                    group,
                });
            } else {
                self.refused += 1;
                mir2_world_bevy::mirror_metrics::note_handoff_refused();
                if let Some(e) = world.entities.get_mut(ack.id) {
                    e.flags.handoff_pending = false;
                }
                glog!(
                    "WRN",
                    "handoff 被拒：区 {} 实体 {} 保留本区（回滚 {}）tick={tick}",
                    self.zone,
                    ack.id,
                    self.refused
                );
            }
        }
        // 2) offers（本区是目标区）。
        //
        // **按 offer 身份去重**（总监 2026-11-22 裁定）：`AlreadyHeld` 只能挡住「目标区此刻
        // 还持有该实体」的重复；一旦目标区在提交后把它移除，**迟到重发会被当成新跨界再收下**
        // （`flake_leg_1.log`：往返两次却 `accepted=3`，多出的一次就是这种「传送」）。
        // 因此先按 `(from, id, epoch)` 比 stamp：`epoch <= 已处理` ⇒ 丢弃（不 ack、不动作）。
        //
        // 表回收放在排 offer 之前：任何可能迟到的重发都在 TTL 内，TTL 外的行不可能还会用到。
        self.last_offer_epoch
            .retain(|_, (_, seen)| tick.saturating_sub(*seen) <= HANDOFF_DUP_TTL_TICKS);
        for offer in offers {
            let key = (offer.from, offer.spec.id);
            if let Some(&(last_epoch, _)) = self.last_offer_epoch.get(&key) {
                if offer.epoch <= last_epoch {
                    self.dup_dropped += 1;
                    mir2_world_bevy::mirror_metrics::note_handoff_offer_dup_dropped();
                    continue;
                }
            }
            let owned = self
                .layout
                .zone_of(offer.spec.map_id, offer.spec.x, offer.spec.y)
                == Some(self.zone);
            let ok = if owned {
                match world.handoff_accept(&offer.spec) {
                    HandoffAccept::Promoted => {
                        self.promoted += 1;
                        glog!(
                            "INF",
                            "handoff 提升：区 {} 将镜像 {} 原地升为真实体（来自区 {}，应在 tick {}）",
                            self.zone,
                            offer.spec.id,
                            offer.from,
                            offer.at_tick
                        );
                        true
                    }
                    HandoffAccept::Inserted => {
                        self.promoted += 1;
                        glog!(
                            "INF",
                            "handoff 落真身：区 {} 直接新增实体 {}（目标区当时无镜像，来自区 {}，应在 tick {}）",
                            self.zone,
                            offer.spec.id,
                            offer.from,
                            offer.at_tick
                        );
                        true
                    }
                    HandoffAccept::AlreadyHeld => {
                        // 幂等重发（提升/落真身后源区还没收到 ack）：按已收下应答。
                        // 计在 `World::handoff_accept`（`handoff_already_held`，总监裁定 ①）：
                        // 不为 0 正常，每次跨界收下的结构性上界是 3（首次 offer 被收下 + pending
                        // 寿命 ≤4 tick 的至多 3 次重发），突变要查。
                        true
                    }
                    HandoffAccept::RefusedOccupied => {
                        glog!(
                            "WRN",
                            "handoff 拒绝：区 {} 收到实体 {} 但已被真实体占用，来自区 {}",
                            self.zone,
                            offer.spec.id,
                            offer.from
                        );
                        false
                    }
                    HandoffAccept::RefusedCell => {
                        glog!(
                            "WRN",
                            "handoff 拒绝：区 {} 收到实体 {} 但落格失败（fail-closed），来自区 {}",
                            self.zone,
                            offer.spec.id,
                            offer.from
                        );
                        // **诊断（总监 2026-11-22：先取证不修行为）**：同一 id 只打一次，
                        // 报告 `add_map_object` 的两个失败点（`cell_match` / `cell.valid()`）与
                        // 实体形态（ghost/death/mirror/handoff_pending）。
                        if self.refused_cell_logged.insert(offer.spec.id) {
                            let (match_ok, valid, cell_type, ghost, death, pending, mirror) =
                                match world.sim.maps.get(offer.spec.map_id) {
                                    Some(g) => {
                                        let m = g.cell_match(offer.spec.x, offer.spec.y);
                                        let (cell, _) = g.get_cell_info(offer.spec.x, offer.spec.y);
                                        let e = world.entities.get(offer.spec.id);
                                        (
                                            m,
                                            cell.valid(),
                                            crate::region_loop::mirror_spec_for(
                                                world,
                                                offer.spec.id,
                                            )
                                            .map(|s| s.cell_type),
                                            e.as_ref().is_some_and(|e| e.ghost),
                                            e.as_ref().is_some_and(|e| e.death),
                                            e.as_ref().is_some_and(|e| e.handoff_pending),
                                            e.as_ref().is_some_and(|e| e.mirror),
                                        )
                                    }
                                    None => (false, false, None, false, false, false, false),
                                };
                            glog!(
                                "WRN",
                                "HANDOFF_DIAG RefusedCell：id={} map={} ({},{}) cell_match={} cell_valid={} cell_type={:?} ghost={} death={} pending={} mirror={} 来自区 {} 本区 {}",
                                offer.spec.id,
                                offer.spec.map_id,
                                offer.spec.x,
                                offer.spec.y,
                                match_ok,
                                valid,
                                cell_type,
                                ghost,
                                death,
                                pending,
                                mirror,
                                offer.from,
                                self.zone
                            );
                        }
                        false
                    }
                }
            } else {
                glog!(
                    "WRN",
                    "handoff 拒绝：区 {} 收到不属本区的实体 {} @({},{} map {})",
                    self.zone,
                    offer.spec.id,
                    offer.spec.x,
                    offer.spec.y,
                    offer.spec.map_id
                );
                false
            };
            // 记下已处理到的 epoch（任何结果都记：拒绝已被回执告知源区，重发不再重试）。
            self.last_offer_epoch.insert(key, (offer.epoch, tick));
            self.send_ack(offer.from, offer.spec.id, ok);
        }
        committed
    }

    /// **barrier2 之后**：把本区所有 `handoff_pending` 实体投给目标区；
    /// 同时把超过 [`HANDOFF_TIMEOUT_TICKS`] 未回执的回滚。
    ///
    /// **A43**：返回本次 `iter_ref` 全表扫的**真实计数**（`scans ≡ 1`、`pending` = 扫前
    /// `handoff_pending` 实体数），供 `HostTickProfile` 的窗口行统计空载浪费。
    pub(crate) fn after_phase2(&mut self, world: &mut World, tick: u64) -> HandoffAfterStats {
        // 超时回滚（目标区已死/邮箱丢失）。
        let expired: Vec<i32> = self
            .pending_since
            .iter()
            .filter(|(_, since)| tick.saturating_sub(**since) > HANDOFF_TIMEOUT_TICKS)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.pending_since.remove(&id);
            self.refused += 1;
            mir2_world_bevy::mirror_metrics::note_handoff_refused();
            if let Some(e) = world.entities.get_mut(id) {
                e.flags.handoff_pending = false;
            }
            glog!(
                "WRN",
                "handoff 超时：区 {} 实体 {} 在 {} tick 内未回执 ⇒ 回滚（累计 {}）",
                self.zone,
                id,
                HANDOFF_TIMEOUT_TICKS,
                self.refused
            );
        }
        // 发 offer（只对真实实体；镜像不参与 handoff）。
        let ids: Vec<i32> = world
            .entities
            .iter_ref()
            .filter(|e| !e.mirror && e.handoff_pending)
            .map(|e| e.id)
            .collect();
        let stats = HandoffAfterStats {
            scans: 1,
            pending: ids.len(),
        };
        for id in ids {
            let Some(spec) = crate::region_loop::mirror_spec_for(world, id) else {
                continue;
            };
            let Some(target) = self.layout.zone_of(spec.map_id, spec.x, spec.y) else {
                // 不属任何区（越界坐标）⇒ 回滚，不投。
                self.refused += 1;
                mir2_world_bevy::mirror_metrics::note_handoff_refused();
                if let Some(e) = world.entities.get_mut(id) {
                    e.flags.handoff_pending = false;
                }
                glog!(
                    "WRN",
                    "handoff 拒绝：区 {} 实体 {} 落点不属任何区（已回滚）",
                    self.zone,
                    id
                );
                continue;
            };
            if target == self.zone {
                // 回到自己区（rect 内）⇒ 取消待提交。
                if let Some(e) = world.entities.get_mut(id) {
                    e.flags.handoff_pending = false;
                }
                self.pending_since.remove(&id);
                continue;
            }
            // epoch = 这次 pending 的起点：同一次跨界的所有重发共享它，下一次跨界必然更大。
            let epoch = *self.pending_since.entry(id).or_insert(tick);
            if let Some(b) = self.boxes.get(target.index()) {
                let mut m = b.lock().unwrap_or_else(PoisonError::into_inner);
                m.0.push(HandoffOffer {
                    from: self.zone,
                    at_tick: tick,
                    epoch,
                    spec,
                });
                mir2_world_bevy::mirror_metrics::note_handoff_offered();
            }
        }
        stats
    }
}

/// **A43**：`HandoffRuntime::after_phase2` 的扫描统计（真实计数）。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct HandoffAfterStats {
    /// 本 tick 的全表扫次数（`iter_ref` 一次 ⇒ 恒 1；空载也有）。
    pub scans: u64,
    /// 扫到的 `handoff_pending` 实体数（空载恒 0）。
    pub pending: usize,
}
