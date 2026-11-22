//! **L2-5 ⑤**：跨区收件人帧的生产邮箱与排水（设计稿 v3，方案 A / V2）。
//!
//! # 口径（写死）
//!
//! - **单一发射者**：源区只把帧写进目标区邮箱；唯一的 gate `send` 在**收件区线程**（`barrier1`
//!   之后排水 + 本地帧路径）。源区拿不到 gate 发送端。
//! - **按源区号分槽**：目标区邮箱 `slots[dst][src]`，每槽只有一个写者（源区线程）⇒ 无跨源抢锁
//!   交错；收件区按源区号升序排空 ⇒ 跨源序是**声明序**（不是 N=1 的交错，见设计稿 §1.3）。
//! - **恰好 1 tick（V2）**：排水点在收件区 `barrier1` 之后、`phase2` 之前；`barrier1` 时全部区
//!   已完成上一 tick 全部产帧。排水用 `src_tick < 当前 tick` 过滤，把本 tick pre-barrier1 产的
//!   帧留到下一轮 ⇒ 延迟不是 0/1 混。
//! - **有界拒新**：每槽 `max_per_slot`；满 ⇒ 丢新来的帧 + `cross_frame_queue_overflow`。拒新
//!   保留每个源的前缀（丢旧会抽掉多来源最早帧、挖中段）。
//! - **不做丢弃式去重**：结构性恰一次（只有源区产生该事件）；`cross_frame_dup` 只是 canary，
//!   命中时**不丢帧**，正常恒 0。
//! - **N=1 零参与**：邮箱只在 `slicing && mirror` 时构造；线程局部 sink 只在切片线程安装。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use mir2_world_bevy::MirrorRoute;

use crate::perf_counters::{self, CrossRecipientOutcome, CrossRecipientSurface};
use crate::zone::ZoneId;
use crate::zone_router::ZoneRouter;
use crate::SimToGate;

/// **`MAX_FRAMES_PER_SLOT` = 每 `(dst, src)` 槽的单 tick 上限**（**拒新**；初值 16384，按窗口实测峰值调整）。
pub(crate) const MAX_FRAMES_PER_SLOT: usize = 16384;

/// 一条待投递的跨区帧（一个收件人 × 一次广播；`bodies` = 该次广播的全部帧体）。
#[derive(Debug)]
pub(crate) struct CrossZoneFrame {
    /// 产生该帧的源区（= 槽的写者）。
    pub src_zone: ZoneId,
    /// 产生该帧的源 tick（收件区按 `src_tick < 当前 tick` 排空 ⇒ 恰好 1 tick）。
    pub src_tick: u64,
    /// 源区在本 tick 的单调发送序（同源保序）。
    pub seq: u32,
    /// 收件人的网关路由 `(line, socket)`。
    pub dst: MirrorRoute,
    /// 该次广播的全部帧体（逐字节转发，不重编码）。
    pub bodies: Vec<Vec<u8>>,
}

/// 每区一个邮箱；内部按**源区**分槽（`slots[dst][src]`）。
///
/// 每个槽只有一个写者（源区线程）⇒ 无跨源抢锁交错；收件区按源区号升序排空 ⇒ 跨源序是声明序。
pub(crate) struct CrossZoneMailboxes {
    zone_count: usize,
    max_per_slot: usize,
    slots: Vec<Vec<Mutex<VecDeque<CrossZoneFrame>>>>,
}

impl CrossZoneMailboxes {
    /// 按区数与每槽上限构造（启动期一次分配）。
    pub(crate) fn new(zone_count: usize, max_per_slot: usize) -> Self {
        Self {
            zone_count,
            max_per_slot,
            slots: (0..zone_count)
                .map(|_| {
                    (0..zone_count)
                        .map(|_| Mutex::new(VecDeque::new()))
                        .collect()
                })
                .collect(),
        }
    }

    /// 参与交换的区数。
    pub(crate) fn zone_count(&self) -> usize {
        self.zone_count
    }

    /// 把一帧压入 `dst` 区邮箱的 `src` 槽；满 ⇒ `false`（拒新，调用方计 overflow）。
    fn push(&self, dst: ZoneId, frame: CrossZoneFrame) -> bool {
        let slot = &self.slots[dst.index()][frame.src_zone.index()];
        let mut q = slot.lock().unwrap_or_else(PoisonError::into_inner);
        if q.len() >= self.max_per_slot {
            return false;
        }
        q.push_back(frame);
        true
    }
}

/// **L2-5 ⑤**：切片 + 镜像同时开才构造邮箱（N=1 / ② 关 ⇒ `None`，零参与）。
///
/// 单独成函数是为了让「构造闸」可被单测直接断言
/// （`cross_zone_mailboxes_absent_without_slicing`）；生产调用点在 `run_inner`。
pub(crate) fn construct_for_run(
    slicing: bool,
    mirror_enabled: bool,
    zone_count: usize,
) -> Option<Arc<CrossZoneMailboxes>> {
    (slicing && mirror_enabled)
        .then(|| Arc::new(CrossZoneMailboxes::new(zone_count, MAX_FRAMES_PER_SLOT)))
}

/// 当前线程的跨区 sink（源区写邮箱用）。线程局部 ⇒ 每区线程各自一份，测试线程互不干扰。
struct SinkState {
    mailboxes: Arc<CrossZoneMailboxes>,
    router: Arc<ZoneRouter>,
    self_zone: ZoneId,
    /// 本 tick 序号（= 入队帧的 `src_tick`）。
    tick: u64,
    /// 本 tick 的源区单调发送序。
    seq: u32,
}

thread_local! {
    static SINK: RefCell<Option<SinkState>> = const { RefCell::new(None) };
}

/// 在本线程安装 sink（区线程启动时调用；`slicing && mirror` 才有邮箱 ⇒ 否则不调用）。
pub(crate) fn install(
    mailboxes: Arc<CrossZoneMailboxes>,
    router: Arc<ZoneRouter>,
    self_zone: ZoneId,
) {
    SINK.with(|cell| {
        *cell.borrow_mut() = Some(SinkState {
            mailboxes,
            router,
            self_zone,
            tick: 0,
            seq: 0,
        });
    });
}

/// 卸载本线程 sink（测试/线程退出）。
#[allow(dead_code)] // 生产退出路径暂不显式卸载（线程结束即释放）；测试用。
pub(crate) fn uninstall() {
    SINK.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// 进入新 tick：刷新 `src_tick` 并复位源区 `seq`。
pub(crate) fn begin_tick(tick: u64) {
    SINK.with(|cell| {
        if let Some(st) = cell.borrow_mut().as_mut() {
            st.tick = tick;
            st.seq = 0;
        }
    });
}

/// 六面在「本地会话未命中且收件人是镜像」处调用：**分类 + 入邮箱**。
///
/// - 已装 sink 且 `route = Some` 且 `ZoneRouter::lookup_registered` 命中别的区 ⇒ 入邮箱（Delivered）；
/// - `route = None` / 路由未登记 / 落在本区 ⇒ Unroutable（残差；后两者另计缺陷信号）；
/// - 槽满 ⇒ Overflow（拒新）。
///
/// 每调用一次**恰好**记一个结果桶（与六面合计同源构造，恒等式由构造保证）。
pub(crate) fn note_mirror_recipient(
    surface: CrossRecipientSurface,
    route: Option<MirrorRoute>,
    bodies: &[Vec<u8>],
) -> CrossRecipientOutcome {
    // **L2-5 ⑤ 成本探针**：只统计「镜像收件人」的整段（含 `bodies.to_vec()`）；关闭态不取钟。
    let t0 = perf_counters::cross_cost_probe_begin();
    let outcome = SINK.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(st) = guard.as_mut() else {
            // 切片关 / 未接线：与旧计数同口径（不投递，记残差）。
            return CrossRecipientOutcome::Unroutable;
        };
        let Some(route) = route else {
            return CrossRecipientOutcome::Unroutable;
        };
        // **禁止**用 `lookup` 的 HOST_ZONE 回落：必须区分「真属区 0」与「未登记」。
        let Some(dst) = st.router.lookup_registered(route.line, route.socket) else {
            perf_counters::note_cross_recipient_zone_unknown();
            return CrossRecipientOutcome::Unroutable;
        };
        if dst == st.self_zone {
            // 本地会话表未命中却路由回本区 —— 一致性缺陷（应恒 0）。
            perf_counters::note_cross_recipient_self_zone_miss();
            return CrossRecipientOutcome::Unroutable;
        }
        let frame = CrossZoneFrame {
            src_zone: st.self_zone,
            src_tick: st.tick,
            seq: st.seq,
            dst: route,
            bodies: bodies.to_vec(),
        };
        st.seq = st.seq.wrapping_add(1);
        if st.mailboxes.push(dst, frame) {
            CrossRecipientOutcome::Delivered
        } else {
            CrossRecipientOutcome::Overflow
        }
    });
    perf_counters::note_cross_recipient_outcome(surface, outcome);
    perf_counters::cross_cost_probe_end(t0);
    outcome
}

/// 收件区在 `barrier1` 返回之后、`phase2` 之前调用：排空本区邮箱并写网关。
///
/// 只排 `src_tick < tick` 的帧（本 tick pre-barrier1 产的帧留到下一轮）⇒ 恰好 1 tick 延迟。
/// 返回本次实际转发到网关的帧数。
pub(crate) fn drain_zone(
    zone: ZoneId,
    tick: u64,
    gates: &HashMap<u8, tokio::sync::mpsc::UnboundedSender<SimToGate>>,
) -> usize {
    let Some(mailboxes) =
        SINK.with(|cell| cell.borrow().as_ref().map(|st| Arc::clone(&st.mailboxes)))
    else {
        return 0;
    };
    // 先把符合 `src_tick < tick` 的帧全部搬到本地批，避免持锁发送。
    let mut batch: Vec<CrossZoneFrame> = Vec::new();
    for src_idx in 0..mailboxes.zone_count() {
        let src = ZoneId::new(u32::try_from(src_idx).unwrap_or(u32::MAX));
        if src == zone {
            continue;
        }
        let slot = &mailboxes.slots[zone.index()][src.index()];
        let mut q = slot.lock().unwrap_or_else(PoisonError::into_inner);
        // 槽按 `src_tick` 单调（每个源每 tick 追加）⇒ front-scan 到 `src_tick == tick` 即停。
        while q.front().is_some_and(|f| f.src_tick < tick) {
            if let Some(f) = q.pop_front() {
                batch.push(f);
            }
        }
    }
    let mut sent = 0usize;
    let mut seen: HashSet<(ZoneId, u64, u32, u8, i32)> = HashSet::new();
    for frame in batch {
        let key = (
            frame.src_zone,
            frame.src_tick,
            frame.seq,
            frame.dst.line,
            frame.dst.socket,
        );
        if !seen.insert(key) {
            // canary：**不丢帧**，只计数（重复帧若出现就是缺陷）。
            perf_counters::note_cross_frame_dup();
        }
        let socket = frame.dst.socket;
        let Some(tx) = crate::route_to_gate(gates, i32::from(frame.dst.line), socket, "跨区帧排水")
        else {
            perf_counters::note_cross_frame_drop_closed();
            continue;
        };
        for body in frame.bodies {
            let _ = tx.send(SimToGate::SendFrame { socket, body });
        }
        sent += 1;
    }
    sent
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zone_router::HOST_ZONE;
    use tokio::sync::mpsc;

    fn zone(i: u32) -> ZoneId {
        ZoneId::new(i)
    }

    fn router_with(assign: &[(i32, u32)]) -> Arc<ZoneRouter> {
        let r = ZoneRouter::new();
        for (socket, z) in assign {
            r.assign(0, *socket, zone(*z)).expect("assign");
        }
        Arc::new(r)
    }

    type Gates = HashMap<u8, mpsc::UnboundedSender<SimToGate>>;

    fn gates() -> (Gates, mpsc::UnboundedReceiver<SimToGate>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut g: Gates = HashMap::new();
        g.insert(0u8, tx);
        (g, rx)
    }

    struct SinkGuard;

    impl Drop for SinkGuard {
        fn drop(&mut self) {
            uninstall();
        }
    }

    fn arm_src(
        mb: &Arc<CrossZoneMailboxes>,
        router: &Arc<ZoneRouter>,
        z: ZoneId,
        tick: u64,
    ) -> SinkGuard {
        install(Arc::clone(mb), Arc::clone(router), z);
        begin_tick(tick);
        SinkGuard
    }

    fn body(n: u8) -> Vec<Vec<u8>> {
        vec![vec![n, n, n]]
    }

    fn route(socket: i32) -> Option<MirrorRoute> {
        Some(MirrorRoute { line: 0, socket })
    }

    /// 恰好 1 tick：tick 0 入队，tick 1 才排；收件人收到原样帧体。
    #[test]
    fn delivers_exactly_at_next_tick() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(true);
        perf_counters::reset_cross_recipient_missing_for_tests();
        let mb = Arc::new(CrossZoneMailboxes::new(2, 8));
        let router = router_with(&[(900, 1)]);
        let _sink = arm_src(&mb, &router, HOST_ZONE, 0);
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(7));
        assert_eq!(
            perf_counters::cross_recipient_outcome(CrossRecipientOutcome::Delivered),
            1
        );
        let (g, mut rx) = gates();
        assert_eq!(
            drain_zone(zone(1), 0, &g),
            0,
            "src_tick=0 不得在 tick 0 排出"
        );
        assert_eq!(drain_zone(zone(1), 1, &g), 1, "tick 1 必须排出");
        match rx.try_recv().expect("一帧") {
            SimToGate::SendFrame { socket, body } => {
                assert_eq!(socket, 900);
                assert_eq!(body, vec![7, 7, 7]);
            }
            _ => panic!("意外消息：非 SendFrame"),
        }
        assert!(rx.try_recv().is_err(), "只应有一帧");
    }

    /// src_tick 过滤：tick 1 pre-barrier1 产的帧留到 tick 2（不是 0 tick）。
    #[test]
    fn src_tick_filter_makes_delay_exactly_one_tick() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(true);
        perf_counters::reset_cross_recipient_missing_for_tests();
        let mb = Arc::new(CrossZoneMailboxes::new(2, 8));
        let router = router_with(&[(900, 1)]);
        let _sink = arm_src(&mb, &router, HOST_ZONE, 0);
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(1));
        begin_tick(1);
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(2));
        let (g, mut rx) = gates();
        assert_eq!(drain_zone(zone(1), 1, &g), 1, "tick 1 只排 tick 0 的帧");
        assert_eq!(drain_zone(zone(1), 2, &g), 1, "tick 2 排 tick 1 的帧");
        assert_eq!(drain_zone(zone(1), 3, &g), 0);
        let a = rx.try_recv().expect("tick0 帧");
        let b = rx.try_recv().expect("tick1 帧");
        match (a, b) {
            (SimToGate::SendFrame { body: ba, .. }, SimToGate::SendFrame { body: bb, .. }) => {
                assert_eq!(ba, vec![1, 1, 1]);
                assert_eq!(bb, vec![2, 2, 2]);
            }
            _ => panic!("形状不对"),
        }
    }

    /// 拒新保前缀：第 3 帧 overflow，前两帧按序排出。
    #[test]
    fn overflow_rejects_new_and_preserves_prefix() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(true);
        perf_counters::reset_cross_recipient_missing_for_tests();
        let mb = Arc::new(CrossZoneMailboxes::new(2, 2));
        let router = router_with(&[(900, 1)]);
        let _sink = arm_src(&mb, &router, HOST_ZONE, 0);
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(1));
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(2));
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(3));
        assert_eq!(
            perf_counters::cross_recipient_outcome(CrossRecipientOutcome::Delivered),
            2
        );
        assert_eq!(
            perf_counters::cross_recipient_outcome(CrossRecipientOutcome::Overflow),
            1
        );
        let (g, mut rx) = gates();
        assert_eq!(drain_zone(zone(1), 1, &g), 2, "前缀两帧");
        let a = rx.try_recv().expect("1");
        let b = rx.try_recv().expect("2");
        match (a, b) {
            (SimToGate::SendFrame { body: ba, .. }, SimToGate::SendFrame { body: bb, .. }) => {
                assert_eq!(ba, vec![1, 1, 1]);
                assert_eq!(bb, vec![2, 2, 2]);
            }
            _ => panic!("形状不对"),
        }
        assert!(rx.try_recv().is_err());
    }

    /// 无 sink / route=None ⇒ Unroutable，不投递。
    #[test]
    fn no_sink_or_no_route_is_unroutable() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(true);
        perf_counters::reset_cross_recipient_missing_for_tests();
        uninstall();
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(1));
        assert_eq!(
            perf_counters::cross_recipient_outcome(CrossRecipientOutcome::Unroutable),
            1
        );
        let mb = Arc::new(CrossZoneMailboxes::new(2, 8));
        let router = router_with(&[(900, 1)]);
        let _sink = arm_src(&mb, &router, HOST_ZONE, 0);
        note_mirror_recipient(CrossRecipientSurface::RefMsg, None, &body(2));
        assert_eq!(
            perf_counters::cross_recipient_outcome(CrossRecipientOutcome::Unroutable),
            2
        );
    }

    /// canary：同 key 两条帧 ⇒ 计数 +1，但**两条都发出**（不丢帧）。
    #[test]
    fn dup_canary_counts_without_dropping() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(true);
        perf_counters::reset_cross_recipient_missing_for_tests();
        let mb = Arc::new(CrossZoneMailboxes::new(2, 8));
        let router = router_with(&[(900, 1)]);
        let _sink = arm_src(&mb, &router, HOST_ZONE, 0);
        // 直接 push 两条同 key 的帧（绕过 seq 递增，模拟重发缺陷）。
        let f = || CrossZoneFrame {
            src_zone: HOST_ZONE,
            src_tick: 0,
            seq: 0,
            dst: MirrorRoute {
                line: 0,
                socket: 900,
            },
            bodies: body(9),
        };
        assert!(mb.push(zone(1), f()));
        assert!(mb.push(zone(1), f()));
        let (g, mut rx) = gates();
        assert_eq!(drain_zone(zone(1), 1, &g), 2);
        assert_eq!(perf_counters::cross_frame_dup(), 1);
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_ok(), "重复帧不得被丢");
    }

    /// **L2-5 ⑤ I4-1**：构造闸 = `slicing && mirror`；任一关 ⇒ `None`（N=1/② 零参与）。
    #[test]
    fn cross_zone_mailboxes_absent_without_slicing() {
        assert!(
            construct_for_run(false, true, 2).is_none(),
            "slicing=0 不得构造邮箱"
        );
        assert!(
            construct_for_run(true, false, 2).is_none(),
            "mirror=0 不得构造邮箱"
        );
        assert!(construct_for_run(false, false, 2).is_none());
        assert!(
            construct_for_run(true, true, 2).is_some(),
            "slicing+mirror 同时开必须构造"
        );
    }

    /// **L2-5 ⑤ I4-4**：无 sink（切片关的形态）⇒ 排水 0、gates 收不到帧。
    #[test]
    fn cross_drain_absent_without_slicing() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(false);
        perf_counters::reset_cross_recipient_missing_for_tests();
        uninstall();
        let (g, mut rx) = gates();
        assert_eq!(drain_zone(zone(1), 1, &g), 0, "无 sink ⇒ 不排水");
        assert!(rx.try_recv().is_err(), "无 sink ⇒ gates 不得收帧");
        perf_counters::set_slicing_enabled(true);
    }

    /// **L2-5 ⑤ I4-3**：切片关 ⇒ 三桶 + 诊断列**各自 0**（即使错误地装了 sink）。
    ///
    /// 覆盖：delivered / unroutable / overflow / dup / drop_closed / zone_unknown。
    #[test]
    fn slicing_off_counts_nothing_three_buckets() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(false);
        perf_counters::reset_cross_recipient_missing_for_tests();
        let mb = Arc::new(CrossZoneMailboxes::new(2, 4));
        let router = router_with(&[(900, 1)]);
        // 错误配置：切片关但装了 sink；计数器闸必须让所有桶保持 0。
        let _sink = arm_src(&mb, &router, HOST_ZONE, 0);
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(1)); // delivered 路径
        note_mirror_recipient(CrossRecipientSurface::RefMsg, None, &body(2)); // unroutable 路径
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(999), &body(3)); // zone_unknown 路径
        for _ in 0..3 {
            note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(4));
        }
        // 槽上限 4：上面已入 4 帧，再来 1 帧走 overflow。
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(5));
        // dup / drop_closed：清空槽（空 gates 强制 drop_closed），再压两条同 key 帧。
        let empty_gates: Gates = HashMap::new();
        let _ = drain_zone(zone(1), 1, &empty_gates);
        let f = || CrossZoneFrame {
            src_zone: HOST_ZONE,
            src_tick: 0,
            seq: 0,
            dst: MirrorRoute {
                line: 0,
                socket: 900,
            },
            bodies: body(9),
        };
        assert!(mb.push(zone(1), f()));
        assert!(mb.push(zone(1), f()));
        let _ = drain_zone(zone(1), 1, &empty_gates);
        assert_eq!(
            perf_counters::cross_recipient_outcome(CrossRecipientOutcome::Delivered),
            0,
            "slicing=0 ⇒ delivered 必须 0"
        );
        assert_eq!(
            perf_counters::cross_recipient_outcome(CrossRecipientOutcome::Unroutable),
            0,
            "slicing=0 ⇒ unroutable 必须 0"
        );
        assert_eq!(
            perf_counters::cross_recipient_outcome(CrossRecipientOutcome::Overflow),
            0,
            "slicing=0 ⇒ overflow 必须 0"
        );
        assert_eq!(
            perf_counters::cross_frame_dup(),
            0,
            "slicing=0 ⇒ dup 必须 0"
        );
        assert_eq!(
            perf_counters::cross_frame_drop_closed(),
            0,
            "slicing=0 ⇒ drop_closed 必须 0"
        );
        assert_eq!(
            perf_counters::cross_recipient_zone_unknown(),
            0,
            "slicing=0 ⇒ zone_unknown 必须 0"
        );
        let (us, events) = perf_counters::cross_enqueue_cost();
        assert_eq!(
            (us, events),
            (0, 0),
            "slicing=0 ⇒ 成本探针不得取钟/计数（双闸）"
        );
        perf_counters::set_slicing_enabled(true);
    }

    /// N=1 / 切片关：sink 不装 ⇒ 不投递、不计数。
    #[test]
    fn slicing_off_counts_nothing() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(false);
        perf_counters::reset_cross_recipient_missing_for_tests();
        uninstall();
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(1));
        assert_eq!(
            perf_counters::cross_recipient_outcome(CrossRecipientOutcome::Unroutable),
            0
        );
        perf_counters::set_slicing_enabled(true);
    }
}
