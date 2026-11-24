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
    /// 产出该帧的面（**仅 `frame-src` 构建保留**）：验收件旁路行用它给 A13（WALK 双发）定责——
    /// 「哪条带标记、标记的是哪个 face」⇒ 另一条（无标记）就是第二路。
    #[cfg(feature = "frame-src")]
    pub surface: CrossRecipientSurface,
}

/// **A33**：一次跨区**世界广播**（`SENDMSG 0-3` / 婚礼广播）的请求。
///
/// 帧体不能由源区预构造：`SM_SYSMESSAGE` 的 `Recog` = **收件人自己**的 `ActorId`，源区拿不到
/// 他区 chr→actor 映射 ⇒ 只传「正文 + 颜色 + 发送人」，由收件区用**同一个**
/// `fan_out_world_sysmessage` 就地展开（构造性逐字节一致，不是靠近似比对）。
#[derive(Clone, Debug)]
pub(crate) struct WorldBroadcast {
    /// 广播正文（`SENDMSG 0-3` 的文本，已含脚本侧前缀/替换）。
    pub texts: Vec<String>,
    /// `RM_SYSMESSAGE` 的 F 色（`RedMsgFColor`/`Cust` 色）。
    pub f_color: u8,
    /// `RM_SYSMESSAGE` 的 B 色。
    pub b_color: u8,
    /// 源区发送人：源区已单独发过他那一份（`frames` 本地排水）；收件区按 id 分段无此 id ⇒ no-op。
    pub caller_actor_id: i32,
}

/// **A33**：一个 `[dst][src]` 槽旁挂的世界广播队列（`(src_tick, WorldBroadcast)`，按
/// `src_tick` 单调追加）。独立类型别名只为躲 `clippy::type_complexity`。
type BroadcastSlot = Mutex<VecDeque<(u64, WorldBroadcast)>>;

/// 每区一个邮箱；内部按**源区**分槽（`slots[dst][src]`）。
///
/// 每个槽只有一个写者（源区线程）⇒ 无跨源抢锁交错；收件区按源区号升序排空 ⇒ 跨源序是声明序。
pub(crate) struct CrossZoneMailboxes {
    zone_count: usize,
    max_per_slot: usize,
    slots: Vec<Vec<Mutex<VecDeque<CrossZoneFrame>>>>,
    /// **A33**：每 `[dst][src]` 槽旁挂的世界广播队列（`(src_tick, broadcast)`；同一
    /// `max_per_slot` 拒新）。与帧队列分开只为不改既有 `CrossZoneFrame` 结构；排水点/过滤/屏障
    /// 全部同一条路径。
    broadcasts: Vec<Vec<BroadcastSlot>>,
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
            broadcasts: (0..zone_count)
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

    /// **A33**：把一条世界广播压入 `dst` 区邮箱的 `src` 槽；满 ⇒ `false`（拒新，调用方计 overflow）。
    fn push_broadcast(&self, dst: ZoneId, src: ZoneId, item: (u64, WorldBroadcast)) -> bool {
        let slot = &self.broadcasts[dst.index()][src.index()];
        let mut q = slot.lock().unwrap_or_else(PoisonError::into_inner);
        if q.len() >= self.max_per_slot {
            return false;
        }
        q.push_back(item);
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
            #[cfg(feature = "frame-src")]
            surface,
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

/// **A33**：源区把一条世界广播排进**每个其它区**的槽（`SENDMSG 0-3`/婚礼广播）。
///
/// 无 sink（N=1 / 切片关）⇒ 返回 0 且**零动作**（N=1 的单区字节面因此不变）。返回入队区数。
/// 收件区在 [`drain_zone`] 里用 **plain 版** `fan_out_world_sysmessage` 展开（不回调中继版）。
pub(crate) fn note_world_broadcast(
    texts: &[String],
    f_color: u8,
    b_color: u8,
    caller_actor_id: i32,
) -> usize {
    if texts.is_empty() {
        return 0;
    }
    SINK.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(st) = guard.as_mut() else {
            return 0;
        };
        let zone_count = st.mailboxes.zone_count();
        let mut queued = 0usize;
        for dst_idx in 0..zone_count {
            let dst = ZoneId::new(u32::try_from(dst_idx).unwrap_or(u32::MAX));
            if dst == st.self_zone {
                continue;
            }
            let item = (
                st.tick,
                WorldBroadcast {
                    texts: texts.to_vec(),
                    f_color,
                    b_color,
                    caller_actor_id,
                },
            );
            if st.mailboxes.push_broadcast(dst, st.self_zone, item) {
                perf_counters::note_cross_world_broadcast_queued();
                queued += 1;
            } else {
                perf_counters::note_cross_world_broadcast_overflow();
            }
        }
        queued
    })
}

/// **M1a（§13.6 的 lite 版）**：跨区帧旁路日志的来源标签。
///
/// M1a 只有 ⑤ 交付边界一处写 `CrossZone`；`ControlReply` 是 M1c 控制邮箱回包处的**预留落点**
/// （M1a 该执行点还不存在，故 `#[allow(dead_code)]`）。
#[cfg(feature = "frame-src")]
#[derive(Clone, Copy, Debug)]
enum FrameSrc {
    CrossZone,
    #[allow(dead_code)]
    ControlReply,
}

/// 旁路日志的环境门（仅 `frame-src` 构建）：设了路径才写。
#[cfg(feature = "frame-src")]
const ENV_CROSS_FRAME_SRC_LOG: &str = "MIR2_CROSS_FRAME_SRC_LOG";

/// **M1a ②**：⑤ 交付边界的 test-only 旁路——每帧一行 JSON `{tick,socket,len,sha256,src}`。
///
/// **默认构建一行不编译**（整段 `#[cfg(feature = "frame-src")]`；验收构建 `--features frame-src`）。
/// `tick` 直接用 `drain_zone` 的参数
/// （= B 段执行时的 `self.ticks`），**不另取** `Instant`/墙钟。
///
/// 外部联表：线上帧 = `'#' + ServerMessage(20B, 小端) + body + '!'`（可选尾部 `$`）
/// ⇒ `body = inner[20..]`；按 `(socket, sha256(body))` 对齐 `frames.ndjson`（tick 只作辅助证据）。
#[cfg(feature = "frame-src")]
fn note_frame_src(
    tick: u64,
    socket: i32,
    body: &[u8],
    src: FrameSrc,
    surface: Option<CrossRecipientSurface>,
) {
    use sha2::Digest as _;
    use std::fmt::Write as _;
    use std::io::Write as _;
    let Some(path) = std::env::var_os(ENV_CROSS_FRAME_SRC_LOG) else {
        return;
    };
    let mut sha = String::with_capacity(64);
    for b in sha2::Sha256::digest(body) {
        let _ = write!(sha, "{b:02x}");
    }
    let surface = surface.map(CrossRecipientSurface::name).unwrap_or("-");
    let line = format!(
        "{{\"tick\":{tick},\"socket\":{socket},\"len\":{},\"sha256\":\"{sha}\",\"src\":\"{src:?}\",\"surface\":\"{surface}\"}}\n",
        body.len()
    );
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut f) => {
            if let Err(e) = f.write_all(line.as_bytes()) {
                crate::glog!("WRN", "跨区帧旁路写失败（{path:?}）：{e}");
            }
        }
        Err(e) => crate::glog!("WRN", "跨区帧旁路打不开（{path:?}）：{e}"),
    }
}

/// 收件区在 `barrier1` 返回之后、`phase2` 之前调用：排空本区邮箱并写网关。
///
/// 只排 `src_tick < tick` 的帧（本 tick pre-barrier1 产的帧留到下一轮）⇒ 恰好 1 tick 延迟。
/// 返回本次实际转发到网关的帧数。
pub(crate) fn drain_zone(
    zone: ZoneId,
    tick: u64,
    online: &crate::OnlineRegistry,
    world: &mir2_world_bevy::World,
    gates: &HashMap<u8, tokio::sync::mpsc::UnboundedSender<SimToGate>>,
) -> usize {
    let Some(mailboxes) =
        SINK.with(|cell| cell.borrow().as_ref().map(|st| Arc::clone(&st.mailboxes)))
    else {
        return 0;
    };
    // **A33**：先排世界广播（同一 `src_tick < tick` 过滤）——收件区用 **plain 版**
    // `fan_out_world_sysmessage` 就地展开（`Recog` = 本区收件人自己的 actor ⇒ 与本地构造逐字节
    // 一致）；**不**回调中继包装，防跨区二次中继。次序 = 广播先于逐人帧（已知限制，见设计）。
    let mut sent = 0usize;
    let mut broadcasts: Vec<WorldBroadcast> = Vec::new();
    for src_idx in 0..mailboxes.zone_count() {
        let src = ZoneId::new(u32::try_from(src_idx).unwrap_or(u32::MAX));
        if src == zone {
            continue;
        }
        let slot = &mailboxes.broadcasts[zone.index()][src.index()];
        let mut q = slot.lock().unwrap_or_else(PoisonError::into_inner);
        while q.front().is_some_and(|(src_tick, _)| *src_tick < tick) {
            if let Some((_, b)) = q.pop_front() {
                broadcasts.push(b);
            }
        }
    }
    for b in broadcasts {
        sent += crate::fan_out_world_sysmessage(
            online,
            world,
            gates,
            b.caller_actor_id,
            &b.texts,
            b.f_color,
            b.b_color,
        );
        perf_counters::note_cross_world_broadcast_delivered();
    }
    // 再把符合 `src_tick < tick` 的帧全部搬到本地批，避免持锁发送。
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
        #[cfg(feature = "frame-src")]
        let surface = frame.surface;
        for body in frame.bodies {
            // **M1a ②**：⑤ 交付边界打标（`frame-src` 构建才有；默认构建零参与）。
            // **follow-up**：行里带 `surface`（RefMsg/RunFrames/DepartFrames/…）⇒ A13 定责。
            #[cfg(feature = "frame-src")]
            note_frame_src(tick, socket, &body, FrameSrc::CrossZone, Some(surface));
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

    /// **A33**：既有帧用例的排水 helper（广播展开需要 online/world ⇒ 这里给空玩家）。
    fn drain(zone: ZoneId, tick: u64, g: &Gates) -> usize {
        let online = crate::OnlineRegistry::new();
        let world = mir2_world_bevy::World::with_maps(vec![(10, 10)]);
        drain_zone(zone, tick, &online, &world, g)
    }

    // ----------------------------------------------------------------
    // A33：世界广播（SENDMSG 0-3）跨区 fan-out
    // ----------------------------------------------------------------

    /// **A33**：带显式 actor id 段的测试世界——各「区」独立构造时必须错开基址，
    /// 否则 `sender` 与别的区的首号玩家会撞成同一 id，被 `fan_out_world_sysmessage` 的
    /// 「跳过发送人」误剔（生产里各区 id 段天然不相交）。
    fn test_world(base: i32) -> mir2_world_bevy::World {
        let mut w = mir2_world_bevy::World::with_maps(vec![(20, 20)]);
        w.set_id_base(base);
        w
    }

    fn add_player(world: &mut mir2_world_bevy::World, name: &str) -> i32 {
        world.add_entity(mir2_world_bevy::EntitySpawn::new(0, name, 5, 5))
    }

    fn online_of(players: &[(&str, i32, i32)]) -> crate::OnlineRegistry {
        let mut o = crate::OnlineRegistry::new();
        for (chr, actor, socket) in players {
            o.set(crate::OnlineEntry {
                account: "acct".into(),
                chr_name: (*chr).to_string(),
                session_id: 1,
                gate_idx: 0,
                socket: *socket,
                actor_id: *actor,
            });
        }
        o
    }

    fn expected_sysmessage(actor_id: i32, text: &str) -> Vec<u8> {
        crate::smframe::sm_sysmessage(
            actor_id,
            252,
            192,
            &mir2_script::cp936::encode_gb2312_dotnet(text),
        )
    }

    fn frames_by_socket(rx: &mut mpsc::UnboundedReceiver<SimToGate>) -> HashMap<i32, Vec<Vec<u8>>> {
        let mut out: HashMap<i32, Vec<Vec<u8>>> = HashMap::new();
        while let Ok(SimToGate::SendFrame { socket, body }) = rx.try_recv() {
            out.entry(socket).or_default().push(body);
        }
        out
    }

    /// **A33 牙 1**：区 0 广播 ⇒ 区 1/区 2 **各恰一条**（每收件人 1 帧 = 1 正文），
    /// `Recog` = 收件人自己、帧体与该区本地构造**逐字节一致**；发送人不重复。
    ///
    /// 变异：`note_world_broadcast` 只发给前两个区（区 2 收不到）⇒ 区 2 断言红。
    #[test]
    fn world_broadcast_reaches_every_zone_exactly_once() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(true);
        perf_counters::reset_cross_recipient_missing_for_tests();
        let mb = Arc::new(CrossZoneMailboxes::new(3, 8));
        let router = Arc::new(ZoneRouter::new());
        let _sink = arm_src(&mb, &router, HOST_ZONE, 0);
        let mut sender_world = test_world(100);
        let sender = add_player(&mut sender_world, "sender");
        let sender_online = online_of(&[("sender", sender, 900)]);
        let (g, mut rx) = gates();
        let texts = vec!["世界公告A".to_string()];
        let local = crate::fan_out_world_sysmessage_all_zones(
            &sender_online,
            &sender_world,
            &g,
            sender,
            &texts,
            252,
            192,
        );
        assert_eq!(local, 0, "源区只有发送人 ⇒ 本地 0 帧");
        assert_eq!(
            perf_counters::cross_world_broadcast_queued(),
            2,
            "两个他区各入一条"
        );
        assert_eq!(perf_counters::cross_world_broadcast_overflow(), 0);
        let mut w1 = test_world(200);
        let a1 = add_player(&mut w1, "a1");
        let a2 = add_player(&mut w1, "a2");
        let o1 = online_of(&[("a1", a1, 901), ("a2", a2, 902)]);
        let mut w2 = test_world(300);
        let b1 = add_player(&mut w2, "b1");
        let o2 = online_of(&[("b1", b1, 903)]);
        let n1 = drain_zone(zone(1), 1, &o1, &w1, &g);
        let n2 = drain_zone(zone(2), 1, &o2, &w2, &g);
        assert_eq!((n1, n2), (2, 1), "每区展开一次（帧数 = 收件人数 × 正文数）");
        assert_eq!(perf_counters::cross_world_broadcast_delivered(), 2);
        let got = frames_by_socket(&mut rx);
        assert_eq!(got.len(), 3, "三个收件人（发送人不得再收一份）");
        assert_eq!(
            got.get(&901),
            Some(&vec![expected_sysmessage(a1, "世界公告A")]),
            "区 1 a1：恰一条、recog = 自己、字节与本地构造一致"
        );
        assert_eq!(
            got.get(&902),
            Some(&vec![expected_sysmessage(a2, "世界公告A")])
        );
        assert_eq!(
            got.get(&903),
            Some(&vec![expected_sysmessage(b1, "世界公告A")]),
            "区 2 恰一条（只发前两区的变异 ⇒ 本断言红）"
        );
        assert_eq!(got.get(&900), None, "发送人他自己那份不在本扇出里");
    }

    /// **A33 牙 2**：恰好 1 tick —— tick 0 不排、tick 1 恰一次、同 tick 再排 0（不重发）。
    ///
    /// 变异：① 广播过滤 `< tick` 改 `<= tick` ⇒ tick 0 就发 ⇒ 红；② 只 peek 不 pop ⇒
    /// 第二次 drain 再发一份 ⇒ 红。
    #[test]
    fn world_broadcast_delivers_exactly_at_next_tick_and_never_twice() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(true);
        perf_counters::reset_cross_recipient_missing_for_tests();
        let mb = Arc::new(CrossZoneMailboxes::new(2, 8));
        let router = Arc::new(ZoneRouter::new());
        let _sink = arm_src(&mb, &router, HOST_ZONE, 0);
        let mut sender_world = test_world(100);
        let sender = add_player(&mut sender_world, "sender");
        let sender_online = online_of(&[("sender", sender, 900)]);
        let (g, mut rx) = gates();
        let texts = vec!["公告".to_string()];
        crate::fan_out_world_sysmessage_all_zones(
            &sender_online,
            &sender_world,
            &g,
            sender,
            &texts,
            252,
            192,
        );
        let mut w = test_world(200);
        let p1 = add_player(&mut w, "p1");
        let o = online_of(&[("p1", p1, 777)]);
        assert_eq!(
            drain_zone(zone(1), 0, &o, &w, &g),
            0,
            "src_tick=0 不得在 tick 0 排出"
        );
        assert_eq!(drain_zone(zone(1), 1, &o, &w, &g), 1, "tick 1 恰一次");
        assert_eq!(
            drain_zone(zone(1), 1, &o, &w, &g),
            0,
            "已消费 ⇒ 同 tick 不得再排（只 peek 不 pop 的变异 ⇒ 本断言红）"
        );
        let got = frames_by_socket(&mut rx);
        assert_eq!(got.get(&777).map(Vec::len), Some(1), "收件人恰一帧");
        assert_eq!(perf_counters::cross_world_broadcast_delivered(), 1);
    }

    /// **A33 牙 3**：N=1（无 sink）⇒ 包装版与 plain 版**逐字节相同**。
    ///
    /// 变异：包装版把本地扇出写两遍（无 sink 时也多发一份）⇒ 本断言红。
    #[test]
    fn n1_world_broadcast_bytes_unchanged() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(false);
        uninstall();
        let mut w = test_world(100);
        let a = add_player(&mut w, "a");
        let b = add_player(&mut w, "b");
        let o = online_of(&[("a", a, 900), ("b", b, 901)]);
        let texts = vec!["公告".to_string()];
        let (g1, mut rx1) = gates();
        let s1 = crate::fan_out_world_sysmessage(&o, &w, &g1, a, &texts, 252, 192);
        let (g2, mut rx2) = gates();
        let s2 = crate::fan_out_world_sysmessage_all_zones(&o, &w, &g2, a, &texts, 252, 192);
        assert_eq!(s1, s2, "N=1：包装版与 plain 版帧数相同");
        let f1 = frames_by_socket(&mut rx1);
        let f2 = frames_by_socket(&mut rx2);
        assert_eq!(f1, f2, "N=1：逐字节相同（无 sink 时中继必须零动作）");
        assert_eq!(f1.get(&901), Some(&vec![expected_sysmessage(b, "公告")]));
        assert_eq!(f1.get(&900), None, "发送人自己那份不在本扇出");
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
        assert_eq!(drain(zone(1), 0, &g), 0, "src_tick=0 不得在 tick 0 排出");
        assert_eq!(drain(zone(1), 1, &g), 1, "tick 1 必须排出");
        match rx.try_recv().expect("一帧") {
            SimToGate::SendFrame { socket, body } => {
                assert_eq!(socket, 900);
                assert_eq!(body, vec![7, 7, 7]);
            }
            _ => panic!("意外消息：非 SendFrame"),
        }
        assert!(rx.try_recv().is_err(), "只应有一帧");
    }

    #[cfg(feature = "frame-src")]
    fn sha_hex(bytes: &[u8]) -> String {
        use sha2::Digest as _;
        use std::fmt::Write as _;
        let mut s = String::with_capacity(64);
        for b in sha2::Sha256::digest(bytes) {
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    /// **M1a ② 见证**：⑤ 交付边界写旁路日志（`tick/socket/len/sha256/src`，一行一条）。
    ///
    /// 变异：删 `note_frame_src` 调用 ⇒ 文件不存在/无该行 ⇒ 红。
    /// 只在验收构建跑：`cargo test -p game-svr --lib --features frame-src`。
    #[cfg(feature = "frame-src")]
    #[test]
    fn drain_zone_writes_frame_src_side_log() {
        let _g = perf_counters::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        perf_counters::set_slicing_enabled(true);
        let dir = std::env::temp_dir().join(format!("mir2_frame_src_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("frame_src.ndjson");
        std::env::set_var(ENV_CROSS_FRAME_SRC_LOG, &path);
        let mb = Arc::new(CrossZoneMailboxes::new(2, 8));
        let router = router_with(&[(900, 1)]);
        let _sink = arm_src(&mb, &router, HOST_ZONE, 0);
        note_mirror_recipient(CrossRecipientSurface::RefMsg, route(900), &body(7));
        let (g, _rx) = gates();
        assert_eq!(drain(zone(1), 1, &g), 1);
        std::env::remove_var(ENV_CROSS_FRAME_SRC_LOG);
        let text = std::fs::read_to_string(&path).expect("旁路文件必须存在");
        let sha = sha_hex(&[7, 7, 7]);
        assert!(
            text.lines().any(|l| l.contains("\"tick\":1")
                && l.contains("\"socket\":900")
                && l.contains("\"src\":\"CrossZone\"")
                && l.contains("\"surface\":\"RefMsg\"")
                && l.contains(&sha)),
            "旁路行必须含 tick=1/socket=900/sha256({sha})；实际：{text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
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
        assert_eq!(drain(zone(1), 1, &g), 1, "tick 1 只排 tick 0 的帧");
        assert_eq!(drain(zone(1), 2, &g), 1, "tick 2 排 tick 1 的帧");
        assert_eq!(drain(zone(1), 3, &g), 0);
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
        assert_eq!(drain(zone(1), 1, &g), 2, "前缀两帧");
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
            #[cfg(feature = "frame-src")]
            surface: CrossRecipientSurface::RefMsg,
        };
        assert!(mb.push(zone(1), f()));
        assert!(mb.push(zone(1), f()));
        let (g, mut rx) = gates();
        assert_eq!(drain(zone(1), 1, &g), 2);
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
        assert_eq!(drain(zone(1), 1, &g), 0, "无 sink ⇒ 不排水");
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
        let _ = drain(zone(1), 1, &empty_gates);
        let f = || CrossZoneFrame {
            src_zone: HOST_ZONE,
            src_tick: 0,
            seq: 0,
            dst: MirrorRoute {
                line: 0,
                socket: 900,
            },
            bodies: body(9),
            #[cfg(feature = "frame-src")]
            surface: CrossRecipientSurface::RefMsg,
        };
        assert!(mb.push(zone(1), f()));
        assert!(mb.push(zone(1), f()));
        let _ = drain(zone(1), 1, &empty_gates);
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
