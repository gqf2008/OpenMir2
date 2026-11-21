//! **L2-3 两阶段 tick 的区边界交换 + L2-4 段 5 的区驱动内核**
//!
//! 两阶段驱动（L2-4 的生产接线）在每个 tick 这样做：
//!
//! ```text
//! 每区: tick_phase1() → 发布本区边界快照 → barrier1 → 读邻居快照 → tick_phase2() → barrier2
//! ```
//!
//! - **快照在 phase1 之后取** ⇒ 邻居读到的位置/HP 等是**本 tick 终态**；
//! - 快照槽按 `tick & 1` 双缓冲：barrier2 保证没人还在读上一槽，下一 tick 才能覆盖；
//! - 本模块提供**数据/同步原语 + 区驱动内核**；`run_zone_tick`/`ZoneRuntime` 已由
//!   `run_inner` 的 `MIR2_ZONES` 分支驱动（段 6a）；`OutputAoi` 跨区收件人/投递的记账挂点
//!   在段 6b（本模块不含帧产生点）。
//!
//! 范围（L2-3）：只快照**可见性面**字段（id/位置/视野/名字）；帧字段在 L2-4 扩展。
//! 边界带 = 距区界 ≤ `halo_width`（全局 `view_range` 上界）的已落格实体。
//!
//! **L2-4 段 5**（本模块新增）：
//! - [`run_zone_tick`]：一区的完整阶段序列（`phase1 → snapshot → publish → barrier1 →
//!   读邻居 → phase2 → beat → barrier2`）；段 6 由 `run_inner` 每区一线程驱动。
//! - [`TickBarriers`] 的两次 `wait_*` 在探针启用时返回本次等待 µs（关闭态不取钟、返回 0）；
//!   [`run_zone_tick`] 把**同一区两道屏障之和**交给 `perf_counters`：tick 列
//!   `region_barrier_us` = **各区的最大值**（慢区决定整 tick 墙钟 = Amdahl 代价），
//!   `region_barrier_total_us` = 全部区之和（只用于对账，别当 tick 成本读）。
//! - [`CrossZoneOutbox`]：跨区帧合并与**单一发射者**判据——`submit` 的来源闸
//!   （`src == owner_zone`）+ 每 tick 的 `(tick, owner, recipient, kind)` 去重账本；
//!   `seal_tick` 给出 `sent/recv/dup/never/not_owner`，[`TickLedger::check`] 自动判。
//!   帧的**产生点**（downlink 发射）在段 6 接线。
//! - [`ZoneDriverPlan`]：**N=1 时区驱动零参与**（原单线程路径：不建线程/屏障/心跳/看门狗）。

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use mir2_world_bevy::{TickRecord, World};

use crate::perf_counters;
use crate::tick_watchdog::ZoneHeartbeats;
use crate::zone::{RegionLayout, ZoneId, ZoneRect, ZoneSpan};
use crate::zone_router::{ZoneRouter, HOST_ZONE};

/// 一个边界实体的只读快照（可见性面字段；帧字段 L2-4 扩展）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HaloEntity {
    /// 业务 ActorId（跨区唯一，P2-3b Step 3 已按片分段）。
    pub actor_id: i32,
    /// 地图号（全局下标）。
    pub map_id: usize,
    /// 世界坐标（本 tick 终态）。
    pub x: i32,
    /// 世界坐标（本 tick 终态）。
    pub y: i32,
    /// 观察者视野半径（`PlayObject.cs:904` = 12；非玩家取缺省）。
    pub view_range: i32,
    /// 名字（帧面在 L2-4 需要；本步只用于快照自证）。
    pub name: String,
}

/// 某区某 tick 的边界带快照。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HaloSnapshot {
    /// 产生快照的 tick 序号（读端用它拒绝「上一 tick 的陈旧槽」）。
    pub tick: u64,
    /// 边界带实体（按 World 槽位顺序，确定性）。
    pub entities: Vec<HaloEntity>,
}

/// 从 World 抽某区（`map_id` + `rect`）的边界带快照。
///
/// 过滤：`add_to_mapped`（已落格）+ 地图匹配 + `rect.border_contains(width)`。
/// **必须在 phase1 之后调用**（此时实体状态是本 tick 终态）。
#[must_use]
pub fn snapshot_border(
    world: &World,
    map_id: usize,
    rect: ZoneRect,
    halo_width: i32,
    tick: u64,
) -> HaloSnapshot {
    let entities = world
        .entities
        .iter_ref()
        .filter(|e| {
            e.add_to_mapped && e.map_id == map_id && rect.border_contains(e.x, e.y, halo_width)
        })
        .map(|e| HaloEntity {
            actor_id: e.id,
            map_id: e.map_id,
            x: e.x,
            y: e.y,
            view_range: e.view_range,
            name: e.name.clone(),
        })
        .collect();
    HaloSnapshot { tick, entities }
}

/// 每区两个槽（`tick & 1` 双缓冲）的快照交换。
#[derive(Debug)]
pub struct HaloExchange {
    slots: Vec<Mutex<[Option<HaloSnapshot>; 2]>>,
}

impl HaloExchange {
    /// 按区数构造（每区两槽，启动期一次分配）。
    #[must_use]
    pub fn new(zone_count: usize) -> Self {
        Self {
            slots: (0..zone_count).map(|_| Mutex::new([None, None])).collect(),
        }
    }

    /// 参与交换的区数（槽数）。
    #[must_use]
    pub fn zone_count(&self) -> usize {
        self.slots.len()
    }

    /// owner 区发布自己的本 tick 快照（只写自己的槽）。
    ///
    /// # Panics
    /// `zone` 越界 ⇒ panic（编程错误）。
    pub fn publish(&self, zone: ZoneId, snapshot: HaloSnapshot) {
        let slot = (snapshot.tick as usize) & 1;
        let mut slots = self.slots[zone.index()]
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        slots[slot] = Some(snapshot);
    }

    /// 读某区**本 tick** 的快照；槽里是别的 tick（陈旧）⇒ `None`。
    ///
    /// 读端必须把 `None` 当作错误（邻居没发布/barrier 用错），不得静默当空快照——
    /// 否则「谁也不发」会被伪装成「没有可见实体」。
    #[must_use]
    pub fn read(&self, zone: ZoneId, tick: u64) -> Option<HaloSnapshot> {
        let slot = (tick as usize) & 1;
        let slots = self.slots[zone.index()]
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        slots[slot].clone().filter(|s| s.tick == tick)
    }
}

/// 每个 tick 的两道屏障（phase1 之后 / phase2 之后）。
#[derive(Debug)]
pub struct TickBarriers {
    after_phase1: std::sync::Barrier,
    after_phase2: std::sync::Barrier,
}

impl TickBarriers {
    /// 按参与区数构造。
    #[must_use]
    pub fn new(zone_count: usize) -> Self {
        Self {
            after_phase1: std::sync::Barrier::new(zone_count),
            after_phase2: std::sync::Barrier::new(zone_count),
        }
    }

    /// phase1 之后的屏障：所有区发布完自己的快照后才返回（随后可安全读邻居）。
    ///
    /// 返回**本线程**本次等待的 µs（探针关闭 ⇒ 0）；区驱动把两道屏障的返回值相加后交给
    /// `perf_counters::note_region_barrier_us`。
    pub fn wait_after_phase1(&self) -> u64 {
        wait_with_probe(&self.after_phase1)
    }

    /// phase2 之后的屏障：保证没人还在读上一槽，下一 tick 才能覆盖。
    ///
    /// 返回本次等待的 µs（探针关闭 ⇒ 0），与 `wait_after_phase1` 由区驱动求和。
    pub fn wait_after_phase2(&self) -> u64 {
        wait_with_probe(&self.after_phase2)
    }
}

/// 屏障等待计时：探针关闭时**不取钟**、返回 0；启用时返回本次等待的 µs。
///
/// 区驱动（[`run_zone_tick`]）把**同一区的两道屏障等待相加**后交给
/// `perf_counters::note_region_barrier_us`；tick 列 `region_barrier_us` 取各区的**最大值**
/// （Amdahl 代价：决定整 tick 墙钟的是最慢的区），`region_barrier_total_us` 是全部区之和
/// （只用于对账）。
fn wait_with_probe(barrier: &std::sync::Barrier) -> u64 {
    if perf_counters::enabled() {
        let t0 = Instant::now();
        barrier.wait();
        u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX)
    } else {
        barrier.wait();
        0
    }
}

/// **段 5 的形态判定**：N=1 必须仍走原单线程路径（区驱动零参与）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZoneDriverPlan {
    /// N=1：不建区线程、不建屏障、不 beat 心跳、不启动看门狗（`run_inner` 保留原路径）。
    SingleThread,
    /// N>1：每区一线程 + 两道屏障 + 快照交换 + 看门狗。
    Zones {
        /// 区数（= 线程数 = 屏障参与数）。
        zone_count: usize,
    },
}

impl ZoneDriverPlan {
    /// 按区数判形态（`0` 视作单区：空布局在校验期已被拒，这里只求不回退成多区）。
    #[must_use]
    pub fn for_zone_count(zone_count: usize) -> Self {
        if zone_count <= 1 {
            Self::SingleThread
        } else {
            Self::Zones { zone_count }
        }
    }

    /// 区驱动是否参与（N=1 ⇒ `false` ⇒ 生产保留原单线程路径）。
    #[must_use]
    pub fn participates(self) -> bool {
        matches!(self, Self::Zones { .. })
    }
}

/// [`run_zone_tick`] 的错误（fail-closed）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ZoneTickError {
    /// 该区在布局里没有任何 span（布局构造保证非空；出现即程序错误）。
    NoSpans(ZoneId),
    /// barrier1 之后读邻居快照，但该区没有发布**本 tick** 的槽（barrier 用错 / 邻居没跑）。
    ///
    /// 不得静默当空快照——否则「谁也不发」会被伪装成「没有可见实体」。
    StaleNeighbor {
        /// 缺失快照的区。
        zone: ZoneId,
        /// 期望的 tick。
        tick: u64,
    },
}

impl fmt::Display for ZoneTickError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSpans(zone) => write!(f, "区 {zone} 没有任何 span（布局非法或调用方过滤错）"),
            Self::StaleNeighbor { zone, tick } => {
                write!(f, "tick {tick}：区 {zone} 没有发布本 tick 的边界快照")
            }
        }
    }
}

impl std::error::Error for ZoneTickError {}

/// 一区一个 tick 的阶段产物。
#[derive(Clone, Debug)]
pub struct ZoneTickResult {
    /// phase1 产出、phase2 补全的 tick 记录。
    pub record: TickRecord,
    /// 本区发布出去的边界带快照（本 tick）。
    pub border: HaloSnapshot,
    /// barrier1 之后读到的**邻区本 tick** 快照（按区号升序，不含自己）。
    pub neighbors: Vec<HaloSnapshot>,
}

/// 一区的**阶段切面**（段 6 的 `sim_loop` 用；[`run_zone_tick`] 复用同一实现）。
///
/// `sim_loop` 的 tick 是 `tick_head → 宿主 P21/P32 段 → tick_tail`，所以多区模式必须把
/// `tick_tail` 在 AOI 前切开：`tick_tail_before_aoi` → [`Self::after_phase1`]（快照/发布/
/// barrier1/读邻居）→ `tick_tail_aoi`（phase2）→ [`Self::after_phase2`]（beat/barrier2/探针）。
pub struct ZoneRuntime<'a> {
    zone: ZoneId,
    spans: &'a [ZoneSpan],
    halo_width: i32,
    exchange: &'a HaloExchange,
    barriers: &'a TickBarriers,
}

impl<'a> ZoneRuntime<'a> {
    /// 按区构造（`spans` 可含全部区的 span；内部只取本区的）。
    #[must_use]
    pub fn new(
        zone: ZoneId,
        spans: &'a [ZoneSpan],
        halo_width: i32,
        exchange: &'a HaloExchange,
        barriers: &'a TickBarriers,
    ) -> Self {
        Self {
            zone,
            spans,
            halo_width,
            exchange,
            barriers,
        }
    }

    /// phase1 的尾段（`tick_tail_before_aoi`）之后调用：边界快照 → 发布 → barrier1 → 读邻居。
    ///
    /// 返回（本区快照, 邻区快照, barrier1 等待 µs）。
    ///
    /// # Errors
    /// 本区没有 span ⇒ [`ZoneTickError::NoSpans`]；邻区没发布本 tick ⇒
    /// [`ZoneTickError::StaleNeighbor`]。
    pub fn after_phase1(
        &self,
        world: &World,
        tick: u64,
    ) -> Result<(HaloSnapshot, Vec<HaloSnapshot>, u64), ZoneTickError> {
        let border = snapshot_zone_border(world, self.zone, self.spans, self.halo_width, tick)?;
        self.exchange.publish(self.zone, border.clone());
        let wait_phase1_us = self.barriers.wait_after_phase1();
        let mut neighbors = Vec::with_capacity(self.exchange.zone_count().saturating_sub(1));
        for i in 0..self.exchange.zone_count() {
            let other = ZoneId::new(u32::try_from(i).expect("区数 <= u32::MAX"));
            if other == self.zone {
                continue;
            }
            let snap = self
                .exchange
                .read(other, tick)
                .ok_or(ZoneTickError::StaleNeighbor { zone: other, tick })?;
            neighbors.push(snap);
        }
        Ok((border, neighbors, wait_phase1_us))
    }

    /// phase2（`tick_tail_aoi`）之后调用：beat（若给）→ barrier2 → 探针记账。
    pub fn after_phase2(&self, wait_phase1_us: u64, beat: Option<(&ZoneHeartbeats, u64)>) {
        if let Some((heartbeats, now_ms)) = beat {
            heartbeats.completed(self.zone, now_ms);
        }
        let wait_phase2_us = self.barriers.wait_after_phase2();
        perf_counters::note_region_barrier_us(wait_phase1_us + wait_phase2_us);
    }
}

/// 按区过滤 span 并抽边界带快照（一区可多 span；span 互不重叠 ⇒ 实体不会重复）。
///
/// # Errors
/// 该区在 `spans` 里一个 span 都没有 ⇒ [`ZoneTickError::NoSpans`]。
pub fn snapshot_zone_border(
    world: &World,
    zone: ZoneId,
    spans: &[ZoneSpan],
    halo_width: i32,
    tick: u64,
) -> Result<HaloSnapshot, ZoneTickError> {
    let mut entities = Vec::new();
    let mut span_count = 0usize;
    for span in spans.iter().filter(|s| s.zone == zone) {
        span_count += 1;
        entities.extend(snapshot_border(world, span.map_id, span.rect, halo_width, tick).entities);
    }
    if span_count == 0 {
        return Err(ZoneTickError::NoSpans(zone));
    }
    Ok(HaloSnapshot { tick, entities })
}

/// 一区一个 tick 的完整阶段序列（段 5 的构造级入口；段 6 的生产 `sim_loop` 用
/// [`ZoneRuntime`] 的 split 版本，因为宿主段夹在 `tick_head` 与 `tick_tail` 之间）：
///
/// ```text
/// tick_phase1 → snapshot → publish → barrier1 → 读邻居 → tick_phase2 → beat → barrier2
/// ```
///
/// `beat = Some((heartbeats, now_ms))` ⇒ phase2 之后写一次心跳（N>1）；`None` ⇒ 不碰心跳表
/// （N=1 走原单线程路径，区驱动整体不参与，见 [`ZoneDriverPlan`]）。
///
/// # Errors
/// 本区没有 span（[`ZoneTickError::NoSpans`]）或邻居缺本 tick 快照
/// （[`ZoneTickError::StaleNeighbor`]）。
///
/// 早退（`Err`）发生在 barrier1 之后、barrier2 之前：调用方**不得**继续等 barrier2——
/// `std::sync::Barrier` 没有超时接口，本区不再参与会让**其余区永久阻塞在 barrier2**
/// （整机卡死而不是单区失败）。因此段 6 的接线必须在同一事件收口上 **abort 整机**
/// （与既有「任一片失败整机 Abort」同口径），不能静默重试或补等。
#[allow(clippy::too_many_arguments)]
pub fn run_zone_tick(
    world: &mut World,
    zone: ZoneId,
    spans: &[ZoneSpan],
    exchange: &HaloExchange,
    barriers: &TickBarriers,
    halo_width: i32,
    tick: u64,
    beat: Option<(&ZoneHeartbeats, u64)>,
) -> Result<ZoneTickResult, ZoneTickError> {
    // 1) phase1：所有写位置/属性/HP 的系统跑完 ⇒ 实体状态是本 tick 终态。
    let mut record = world.tick_phase1();

    // 2) 快照 → 发布 → barrier1 → 读邻居（缺 ⇒ fail-closed）。
    let runtime = ZoneRuntime::new(zone, spans, halo_width, exchange, barriers);
    let (border, neighbors, wait_phase1_us) = runtime.after_phase1(world, tick)?;

    // 3) phase2（AOI）→ 心跳 → barrier2 → 探针记账。
    world.tick_phase2(&mut record);
    runtime.after_phase2(wait_phase1_us, beat);

    Ok(ZoneTickResult {
        record,
        border,
        neighbors,
    })
}

/// 世界侧收件人（会话的网关坐标；与 `zone_router` 的键同形）。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Recipient {
    /// 网关线号。
    pub line: u8,
    /// 该槽的客户端句柄。
    pub socket: i32,
}

/// 一帧的**确定身份**：同一 `(tick, owner, recipient, kind)` 只允许**一个**发射者。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrameKey {
    /// tick 序号（同 tick 内比较）。
    pub tick: u64,
    /// 产生该帧的实体（= 唯一发射者判定的主体；所属区由布局给出）。
    pub owner: i32,
    /// 收件人。
    pub recipient: Recipient,
    /// 帧类别（`SM_*` 的 ident 或驱动层枚举；同 tick 同实体同收件人下唯一）。
    pub kind: u16,
}

/// 源区产出的一帧（`seq` 只在同 tick 同源区内定序）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XferFrame {
    /// 源区内的发出序。
    pub seq: u32,
    /// 帧身份。
    pub key: FrameKey,
    /// 线上载荷（12B 头 + 尾；本层透传）。
    pub payload: Vec<u8>,
}

/// 已投递的一帧（带源区；目标区按 `(src, seq)` 稳定排序）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveredFrame {
    /// 实体所属区（唯一允许的发射者）。
    pub src: ZoneId,
    /// 源区内的发出序。
    pub seq: u32,
    /// 帧身份。
    pub key: FrameKey,
    /// 线上载荷。
    pub payload: Vec<u8>,
}

/// 跨区 outbox 的拒绝原因（`Err` = 本帧被拒**且已计数**；账本仍可在 `seal_tick` 后查看）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutboxError {
    /// 还没有 `begin_tick`。
    NoActiveTick,
    /// 上一个 tick 还没 `seal_tick` 就开始新 tick。
    Nested {
        /// 未 seal 的 tick。
        active: u64,
        /// 本次尝试的 tick。
        attempt: u64,
    },
    /// 帧的 `key.tick` 与当前 tick 不符。
    TickMismatch {
        /// 当前 tick。
        active: u64,
        /// 帧自称的 tick。
        frame: u64,
    },
    /// **单一发射者违例**：提交方不是实体所属区。
    NotOwner {
        /// 提交方。
        src: ZoneId,
        /// 实体所属区（布局给出）。
        owner_zone: ZoneId,
        /// 实体 id。
        owner: i32,
    },
    /// 同 tick 同 `(owner, recipient, kind)` 已被另一个发射者提交（第二个发射者）。
    Duplicate {
        /// 重复的帧身份。
        key: FrameKey,
    },
    /// 目标区越界（收件人所属区不在本 outbox 的区范围内）⇒ 永远收不到。
    BadZone {
        /// 越界的目标区。
        zone: ZoneId,
        /// outbox 的区数。
        zone_count: usize,
    },
}

impl fmt::Display for OutboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoActiveTick => write!(f, "outbox 还没有 begin_tick"),
            Self::Nested { active, attempt } => {
                write!(f, "tick {active} 未 seal 就尝试 begin_tick({attempt})")
            }
            Self::TickMismatch { active, frame } => {
                write!(f, "帧 tick={frame} 与当前 tick={active} 不符")
            }
            Self::NotOwner {
                src,
                owner_zone,
                owner,
            } => write!(
                f,
                "单一发射者违例：实体 {owner} 属区 {owner_zone}，但由区 {src} 发射"
            ),
            Self::Duplicate { key } => write!(
                f,
                "重复帧：tick={} owner={} recipient=({}, {}) kind={} 已有发射者",
                key.tick, key.owner, key.recipient.line, key.recipient.socket, key.kind
            ),
            Self::BadZone { zone, zone_count } => {
                write!(f, "目标区 {zone} 越界（区数 {zone_count}）⇒ 永远收不到")
            }
        }
    }
}

impl std::error::Error for OutboxError {}

/// 一个 tick 的合并账本。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TickLedger {
    /// 账本对应的 tick。
    pub tick: u64,
    /// 每区收件箱（下标 = 目标区；每箱按 `(src, seq)` 排序）。
    pub inboxes: Vec<Vec<DeliveredFrame>>,
    /// 接受并投递的帧数。
    pub sent: u64,
    /// 收件箱里实际收到的帧数（正常路径 `recv == sent`）。
    pub recv: u64,
    /// 同 tick 重复身份的提交次数（单一发射者违例）。
    pub dup: u64,
    /// 目标区不存在 ⇒ 永远收不到的帧数。
    pub never: u64,
    /// 非所属区提交（单一发射者违例）次数。
    pub not_owner: u64,
}

impl TickLedger {
    /// 某区本 tick 收到的帧（越界区 ⇒ 空）。
    #[must_use]
    pub fn recv_for(&self, zone: ZoneId) -> &[DeliveredFrame] {
        self.inboxes.get(zone.index()).map_or(&[], Vec::as_slice)
    }

    /// **自动判据**：单一发射者 + `sent == recv` + `dup/never/not_owner` 全 0。
    ///
    /// 生产路径（段 6）在每 tick 合并后调用；任一非零 ⇒ 拒绝继续（与「任一片失败整机
    /// Abort」同口径）。
    ///
    /// # Errors
    /// 首个非零计数（`not_owner` → `dup` → `never` → `sent/recv` 失衡）。
    pub fn check(&self) -> Result<(), LedgerViolation> {
        if self.not_owner > 0 {
            return Err(LedgerViolation::NotOwner(self.not_owner));
        }
        if self.dup > 0 {
            return Err(LedgerViolation::Duplicate(self.dup));
        }
        if self.never > 0 {
            return Err(LedgerViolation::Never(self.never));
        }
        if self.sent != self.recv {
            return Err(LedgerViolation::SentRecvMismatch {
                sent: self.sent,
                recv: self.recv,
            });
        }
        Ok(())
    }
}

/// [`TickLedger::check`] 的违例。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LedgerViolation {
    /// 非所属区发射的次数。
    NotOwner(u64),
    /// 重复身份的次数。
    Duplicate(u64),
    /// 目标区不存在的次数。
    Never(u64),
    /// 接受数与投递数不符。
    SentRecvMismatch {
        /// 接受数。
        sent: u64,
        /// 投递数。
        recv: u64,
    },
}

impl fmt::Display for LedgerViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotOwner(n) => write!(f, "单一发射者违例 {n} 次（非所属区发射）"),
            Self::Duplicate(n) => write!(f, "重复帧 {n} 次（同 tick 同身份两个发射者）"),
            Self::Never(n) => write!(f, "永久不可达帧 {n} 次（目标区不存在）"),
            Self::SentRecvMismatch { sent, recv } => {
                write!(f, "sent={sent} != recv={recv}（outbox 内部失衡）")
            }
        }
    }
}

impl std::error::Error for LedgerViolation {}

/// 跨区 outbox：实体所属区是**唯一发射者**，帧按收件人所属区入箱，目标区转发到 gate。
///
/// 单一发射者由两道**自动判据**保证（不是注释）：
/// 1. [`Self::submit`] 的来源闸：`src != owner_zone` ⇒ [`OutboxError::NotOwner`] 并计数；
/// 2. 每 tick 的身份账本：同 `(tick, owner, recipient, kind)` 第二次提交 ⇒
///    [`OutboxError::Duplicate`] 并计数。
///
/// tick 末 [`Self::seal_tick`] 给出 [`TickLedger`]，[`TickLedger::check`] 把
/// `not_owner/dup/never/sent==recv` 一次判完。
#[derive(Debug)]
pub struct CrossZoneOutbox {
    zone_count: usize,
    active: Option<u64>,
    seen: HashMap<FrameKey, ZoneId>,
    inboxes: Vec<Vec<DeliveredFrame>>,
    sent: u64,
    recv: u64,
    dup: u64,
    never: u64,
    not_owner: u64,
}

impl CrossZoneOutbox {
    /// 按区数构造（启动期一次分配）。
    #[must_use]
    pub fn new(zone_count: usize) -> Self {
        Self {
            zone_count,
            active: None,
            seen: HashMap::new(),
            inboxes: (0..zone_count).map(|_| Vec::new()).collect(),
            sent: 0,
            recv: 0,
            dup: 0,
            never: 0,
            not_owner: 0,
        }
    }

    /// 开始一个 tick（上一个必须已 seal）。
    ///
    /// # Errors
    /// 上一个 tick 未 seal ⇒ [`OutboxError::Nested`]。
    pub fn begin_tick(&mut self, tick: u64) -> Result<(), OutboxError> {
        if let Some(active) = self.active {
            return Err(OutboxError::Nested {
                active,
                attempt: tick,
            });
        }
        self.active = Some(tick);
        Ok(())
    }

    /// 提交一帧。`src` = 提交方区，`owner_zone` = 布局算出的实体所属区，`dst` = 收件人所属区。
    ///
    /// `Err` 表示本帧被拒**且已计入账本**（`not_owner` / `dup` / `never`）；调用方在 tick 末
    /// 用 [`TickLedger::check`] 统一判。
    ///
    /// # Errors
    /// 见 [`OutboxError`]（`NoActiveTick` / `TickMismatch` / `NotOwner` / `Duplicate` /
    /// `BadZone`）。
    pub fn submit(
        &mut self,
        src: ZoneId,
        owner_zone: ZoneId,
        dst: ZoneId,
        frame: XferFrame,
    ) -> Result<(), OutboxError> {
        let Some(active) = self.active else {
            return Err(OutboxError::NoActiveTick);
        };
        if frame.key.tick != active {
            return Err(OutboxError::TickMismatch {
                active,
                frame: frame.key.tick,
            });
        }
        if src != owner_zone {
            self.not_owner += 1;
            return Err(OutboxError::NotOwner {
                src,
                owner_zone,
                owner: frame.key.owner,
            });
        }
        if dst.index() >= self.zone_count {
            self.never += 1;
            return Err(OutboxError::BadZone {
                zone: dst,
                zone_count: self.zone_count,
            });
        }
        match self.seen.entry(frame.key) {
            Entry::Occupied(_) => {
                self.dup += 1;
                return Err(OutboxError::Duplicate { key: frame.key });
            }
            Entry::Vacant(slot) => {
                slot.insert(src);
            }
        }
        self.inboxes[dst.index()].push(DeliveredFrame {
            src,
            seq: frame.seq,
            key: frame.key,
            payload: frame.payload,
        });
        self.sent += 1;
        self.recv += 1;
        Ok(())
    }

    /// 结束本 tick：每区收件箱按 `(src, seq)` 定序，返回账本并复位。
    ///
    /// # Errors
    /// 没有活动 tick ⇒ [`OutboxError::NoActiveTick`]。
    pub fn seal_tick(&mut self) -> Result<TickLedger, OutboxError> {
        let Some(tick) = self.active.take() else {
            return Err(OutboxError::NoActiveTick);
        };
        let mut inboxes = std::mem::take(&mut self.inboxes);
        for inbox in &mut inboxes {
            inbox.sort_by_key(|f| (f.src.get(), f.seq));
        }
        let ledger = TickLedger {
            tick,
            inboxes,
            sent: self.sent,
            recv: self.recv,
            dup: self.dup,
            never: self.never,
            not_owner: self.not_owner,
        };
        self.inboxes = (0..self.zone_count).map(|_| Vec::new()).collect();
        self.seen.clear();
        self.sent = 0;
        self.recv = 0;
        self.dup = 0;
        self.never = 0;
        self.not_owner = 0;
        Ok(ledger)
    }
}

/// **生产 AOI 出帧的单一发射者记账**（段 6）。
///
/// 每 tick 一个：`drain_aoi_downlink` 对每一帧调 [`Self::note`]，tick 末调用方 `seal_tick`
/// 后用 [`TickLedger::check`] 判。**只记 AOI 出现帧**——v1 不存在跨区帧（收件人都是本区
/// 会话），其它帧源不记账；全帧记账需要逐发射点改签名，超出本段。
pub struct AoIFrameLedger<'a> {
    outbox: &'a mut CrossZoneOutbox,
    layout: &'a RegionLayout,
    router: &'a ZoneRouter,
    zone: ZoneId,
    tick: u64,
    seq: u32,
}

impl<'a> AoIFrameLedger<'a> {
    /// 开一个 tick 的账（调用方已 `begin_tick`）。
    pub fn new(
        outbox: &'a mut CrossZoneOutbox,
        layout: &'a RegionLayout,
        router: &'a ZoneRouter,
        zone: ZoneId,
        tick: u64,
    ) -> Self {
        Self {
            outbox,
            layout,
            router,
            zone,
            tick,
            seq: 0,
        }
    }

    /// 记一帧：`owner` 实体的坐标所属区是唯一发射者；`dst` 由收件人 `(line, socket)` 查
    /// [`ZoneRouter`]（未命中 = 区 0）。被拒的帧已在 outbox 内计数，`seal` 后的 `check`
    /// 会报；这里不 panic。
    // 一帧的身份就是这几个字段（owner 的三元组 + 收件人 + kind）⇒ 不拆结构体。
    #[allow(clippy::too_many_arguments)]
    pub fn note(
        &mut self,
        owner: i32,
        owner_map: usize,
        owner_x: i32,
        owner_y: i32,
        line: u8,
        socket: i32,
        kind: u16,
    ) {
        let owner_zone = self
            .layout
            .zone_of(owner_map, owner_x, owner_y)
            .unwrap_or(HOST_ZONE);
        let dst = self.router.lookup(line, socket);
        let key = FrameKey {
            tick: self.tick,
            owner,
            recipient: Recipient { line, socket },
            kind,
        };
        let frame = XferFrame {
            seq: self.seq,
            key,
            payload: Vec::new(),
        };
        self.seq = self.seq.wrapping_add(1);
        let _ = self.outbox.submit(self.zone, owner_zone, dst, frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_world_bevy::{EntitySpawn, World};
    use std::sync::Arc;

    // ---- L2-4 段 5：区驱动 / 跨区 outbox / 探针 / 心跳 ----

    /// 探针守卫：与 `zone_router` 的计数器用例共用 `TEST_LOCK`（都开关全局 `ENABLED`），
    /// **drop 时复位**——中途 panic 也不把探针留给别的测试。
    struct Probe {
        _guard: std::sync::MutexGuard<'static, ()>,
        path: std::path::PathBuf,
    }

    impl Probe {
        fn on(tag: &str) -> Self {
            let guard = perf_counters::TEST_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let path = std::env::temp_dir().join(format!(
                "l2_region_loop_{}_{}.tsv",
                std::process::id(),
                tag
            ));
            let _ = std::fs::remove_file(&path);
            // 先归零（`init_with_path` 不清计数）：本文件的探针断言是绝对值。
            perf_counters::reset_for_tests();
            perf_counters::init_with_path(&path).expect("探针文件可建");
            Self {
                _guard: guard,
                path,
            }
        }
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            perf_counters::reset_for_tests();
            let _ = std::fs::remove_file(&self.path);
        }
    }

    /// **N=1 零参与牙**：单区布局的驱动形态是 `SingleThread`（原单线程路径）；单区协议本身
    /// 仍可独立跑（neighbors 为空）——但生产 N=1 不会走到这里。
    ///
    /// 红检：把 `for_zone_count` 的 `<= 1` 改成 `< 1` ⇒ 第一条断言红。
    #[test]
    fn n_eq_one_plan_is_zero_participation() {
        assert_eq!(
            ZoneDriverPlan::for_zone_count(0),
            ZoneDriverPlan::SingleThread
        );
        assert_eq!(
            ZoneDriverPlan::for_zone_count(1),
            ZoneDriverPlan::SingleThread
        );
        assert!(
            !ZoneDriverPlan::for_zone_count(1).participates(),
            "N=1 ⇒ 区驱动零参与"
        );
        assert_eq!(
            ZoneDriverPlan::for_zone_count(2),
            ZoneDriverPlan::Zones { zone_count: 2 }
        );
        assert!(ZoneDriverPlan::for_zone_count(2).participates());

        let mut world = World::new(20, 10);
        world.add_entity(entity(1, "solo", 3, 3));
        let spans = vec![ZoneSpan::new(ZoneId::new(0), 0, ZoneRect::new(0, 0, 19, 9))];
        let out = run_zone_tick(
            &mut world,
            ZoneId::new(0),
            &spans,
            &HaloExchange::new(1),
            &TickBarriers::new(1),
            3,
            1,
            None,
        )
        .expect("单区 tick");
        assert!(out.neighbors.is_empty());
        assert_eq!(out.border.tick, 1);
        assert_eq!(
            out.border
                .entities
                .iter()
                .map(|e| e.actor_id)
                .collect::<Vec<_>>(),
            vec![1]
        );
    }

    /// **阶段牙**：两个区各跑一 tick（各自线程），barrier1 之后互相读到**本 tick**的边界快照。
    ///
    /// 红检：删掉 `run_zone_tick` 里的 `wait_after_phase1`（或 publish）⇒ tick=1 首次读取必为
    /// `StaleNeighbor` ⇒ 红。
    #[test]
    fn two_zones_exchange_this_tick_border_snapshots() {
        let spans = vec![
            ZoneSpan::new(ZoneId::new(0), 0, ZoneRect::new(0, 0, 9, 9)),
            ZoneSpan::new(ZoneId::new(1), 0, ZoneRect::new(10, 0, 19, 9)),
        ];
        let exchange = Arc::new(HaloExchange::new(2));
        let barriers = Arc::new(TickBarriers::new(2));
        let mut handles = Vec::new();
        for (zone, id, x) in [(0u32, 7i32, 9i32), (1u32, 8i32, 10i32)] {
            let spans = spans.clone();
            let exchange = Arc::clone(&exchange);
            let barriers = Arc::clone(&barriers);
            handles.push(std::thread::spawn(move || {
                let mut world = World::new(20, 10);
                world.add_entity(entity(id, "bot", x, 5));
                let zone = ZoneId::new(zone);
                run_zone_tick(&mut world, zone, &spans, &exchange, &barriers, 3, 1, None).map(|r| {
                    (
                        zone,
                        r.border
                            .entities
                            .iter()
                            .map(|e| e.actor_id)
                            .collect::<Vec<_>>(),
                        r.neighbors
                            .iter()
                            .map(|n| {
                                (
                                    n.tick,
                                    n.entities.iter().map(|e| e.actor_id).collect::<Vec<_>>(),
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                })
            }));
        }
        let mut got = Vec::new();
        for h in handles {
            got.push(h.join().expect("区线程").expect("区 tick"));
        }
        got.sort_by_key(|(zone, ..)| zone.get());
        assert_eq!(got[0].1, vec![7]);
        assert_eq!(got[1].1, vec![8]);
        assert_eq!(got[0].2, vec![(1, vec![8])], "区 0 读到区 1 的本 tick 快照");
        assert_eq!(got[1].2, vec![(1, vec![7])], "区 1 读到区 0 的本 tick 快照");
    }

    /// **陈旧邻区牙**：本 tick 没有邻居发布 ⇒ `StaleNeighbor`（fail-closed，不当空快照）。
    ///
    /// 红检：把 `ok_or(StaleNeighbor)` 改成 `unwrap_or_default()` ⇒ 本用例红。
    #[test]
    fn missing_neighbor_snapshot_is_an_error() {
        let spans = vec![ZoneSpan::new(ZoneId::new(0), 0, ZoneRect::new(0, 0, 9, 9))];
        let exchange = HaloExchange::new(2);
        let barriers = TickBarriers::new(1); // 不等待：邻居永远不会发布
        let mut world = World::new(20, 10);
        world.add_entity(entity(1, "solo", 5, 5));
        let err = run_zone_tick(
            &mut world,
            ZoneId::new(0),
            &spans,
            &exchange,
            &barriers,
            3,
            1,
            None,
        )
        .expect_err("区 1 没发布 ⇒ 必须报错");
        assert_eq!(
            err,
            ZoneTickError::StaleNeighbor {
                zone: ZoneId::new(1),
                tick: 1
            }
        );
    }

    /// **心跳牙**：`beat = Some(..)` 时 phase2 之后写心跳；`None` 时不动心跳表。
    ///
    /// 红检：把 `heartbeats.completed` 挪到 phase2 之前（或删掉）⇒ 第一条断言在
    /// 「phase2 之后」的实现上仍可能过——**真正的红检是删掉 `completed`** ⇒ 第一条红。
    #[test]
    fn heartbeat_is_written_after_phase2_only_when_supplied() {
        let spans = vec![ZoneSpan::new(ZoneId::new(0), 0, ZoneRect::new(0, 0, 19, 9))];
        let zone = ZoneId::new(0);
        let mut world = World::new(20, 10);
        let hb = ZoneHeartbeats::new(1, 111);
        let _ran = run_zone_tick(
            &mut world,
            zone,
            &spans,
            &HaloExchange::new(1),
            &TickBarriers::new(1),
            3,
            1,
            Some((&hb, 12_345)),
        )
        .expect("带心跳 tick");
        assert_eq!(hb.last_completed_ms(zone), 12_345, "phase2 后必须 beat");

        let hb2 = ZoneHeartbeats::new(1, 111);
        run_zone_tick(
            &mut world,
            zone,
            &spans,
            &HaloExchange::new(1),
            &TickBarriers::new(1),
            3,
            2,
            None,
        )
        .expect("不带心跳 tick");
        assert_eq!(hb2.last_completed_ms(zone), 111, "不参与时不得改心跳");
    }

    /// **探针牙 ①**：探针启用时 `wait_*` 返回本次等待 µs（5ms 邻居延迟保证可测）；
    /// 关闭态不取钟、返回 0。
    ///
    /// 红检：删掉 `wait_with_probe` 的计时分支（直接 `barrier.wait(); 0`）⇒ 第一条断言红。
    #[test]
    fn barrier_wait_returns_measured_us_when_enabled() {
        let _probe = Probe::on("barrier-us");
        let barriers = Arc::new(TickBarriers::new(2));
        let b = Arc::clone(&barriers);
        let sleeper = std::thread::spawn(move || {
            // 先睡再等：主线程先到屏障并阻塞 ⇒ 测到的等待 ≈ sleep（100ms 余量，断言取
            // 20ms 下限，留 5× 调度抖动余量）。
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = b.wait_after_phase1();
        });
        let us = barriers.wait_after_phase1();
        sleeper.join().expect("延迟线程");
        assert!(us >= 20_000, "≈100ms 的等待必须返回 µs（实测 {us}）");
    }

    /// **探针牙 ②（口径）**：`region_barrier_us` = 各区「两道屏障之和」的**最大值**，
    /// `region_barrier_total_us` = 全部区之和（对账列）。
    ///
    /// 红检：把 `note_region_barrier_us` 的 `fetch_max` 改成 `fetch_add` ⇒ max 断言红
    /// （300 变 600）；删掉 total 的 `fetch_add` ⇒ total 断言红。
    #[test]
    fn region_barrier_probe_keeps_max_and_total() {
        let _probe = Probe::on("barrier-max");
        use std::sync::atomic::Ordering;
        perf_counters::note_region_barrier_us(100);
        perf_counters::note_region_barrier_us(300);
        perf_counters::note_region_barrier_us(200);
        assert_eq!(
            perf_counters::REGION_BARRIER_US.load(Ordering::Relaxed),
            300,
            "列 = 各区的最大值（Amdahl 代价），不是和"
        );
        assert_eq!(
            perf_counters::REGION_BARRIER_TOTAL_US.load(Ordering::Relaxed),
            600,
            "total 列 = 全部区之和（只对账）"
        );
    }

    /// **探针牙 ③（接线）**：`run_zone_tick` 把**同一区两道屏障之和**交给探针。
    ///
    /// 本用例让「邻居」在 barrier1 与 barrier2 **之间**睡 100ms：区 0 的 barrier2 等待必然
    /// ≈100ms（不受两线程启动先后影响），因此 `region_barrier_us`（max）必须跟上。
    ///
    /// 红检：删掉 `run_zone_tick` 里的 `note_region_barrier_us` ⇒ max 恒 0 ⇒ 红。
    #[test]
    fn two_zones_probe_max_tracks_slowest_zone() {
        let _probe = Probe::on("barrier-wiring");
        let exchange = HaloExchange::new(2);
        // 邻居（区 1）先发布本 tick 快照：区 0 读邻居不会走 StaleNeighbor。
        exchange.publish(
            ZoneId::new(1),
            HaloSnapshot {
                tick: 1,
                entities: Vec::new(),
            },
        );
        let barriers = Arc::new(TickBarriers::new(2));
        let b = Arc::clone(&barriers);
        let peer = std::thread::spawn(move || {
            let _ = b.wait_after_phase1();
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = b.wait_after_phase2();
        });
        let spans = vec![ZoneSpan::new(ZoneId::new(0), 0, ZoneRect::new(0, 0, 19, 9))];
        let mut world = World::new(20, 10);
        run_zone_tick(
            &mut world,
            ZoneId::new(0),
            &spans,
            &exchange,
            &barriers,
            3,
            1,
            None,
        )
        .expect("区 tick");
        peer.join().expect("邻居线程");
        use std::sync::atomic::Ordering;
        let max = perf_counters::REGION_BARRIER_US.load(Ordering::Relaxed);
        let total = perf_counters::REGION_BARRIER_TOTAL_US.load(Ordering::Relaxed);
        assert!(
            max >= 20_000,
            "区 0 等邻居的 ≈100ms 必须进 max（实测 {max}µs）"
        );
        assert!(
            total >= max,
            "total 是全部区之和，必不小于 max（实测 {total}/{max}）"
        );
    }

    fn frame_key(tick: u64, owner: i32, line: u8, socket: i32, kind: u16) -> FrameKey {
        FrameKey {
            tick,
            owner,
            recipient: Recipient { line, socket },
            kind,
        }
    }

    fn xfer(seq: u32, key: FrameKey) -> XferFrame {
        XferFrame {
            seq,
            key,
            payload: Vec::new(),
        }
    }

    /// **outbox 正例**：按收件人所属区入箱、按 `(src, seq)` 定序、`sent == recv`。
    ///
    /// 红检：把 `submit` 里的 `dst` 固定成 `src` ⇒ 收件箱断言红；删掉 `sort_by_key` ⇒ 定序红。
    #[test]
    fn outbox_routes_to_recipient_zone_and_keeps_sent_eq_recv() {
        let mut outbox = CrossZoneOutbox::new(2);
        outbox.begin_tick(4).expect("begin");
        let z0 = ZoneId::new(0);
        let z1 = ZoneId::new(1);
        outbox
            .submit(z0, z0, z1, xfer(5, frame_key(4, 7, 1, 700, 12)))
            .expect("帧 1");
        outbox
            .submit(z0, z0, z1, xfer(1, frame_key(4, 7, 1, 700, 13)))
            .expect("帧 2");
        outbox
            .submit(z0, z0, z0, xfer(0, frame_key(4, 8, 1, 701, 12)))
            .expect("本区帧");
        let ledger = outbox.seal_tick().expect("seal");
        assert_eq!(
            (
                ledger.sent,
                ledger.recv,
                ledger.dup,
                ledger.never,
                ledger.not_owner
            ),
            (3, 3, 0, 0, 0)
        );
        assert!(ledger.check().is_ok());
        let to_z1: Vec<u32> = ledger.recv_for(z1).iter().map(|f| f.seq).collect();
        assert_eq!(to_z1, vec![1, 5], "目标区按 (src, seq) 定序");
        assert_eq!(ledger.recv_for(z0).len(), 1);
        assert!(ledger.recv_for(ZoneId::new(9)).is_empty(), "越界区 ⇒ 空");
    }

    /// **单一发射者反例**：非所属区提交 ⇒ `NotOwner` 计数；同身份二次提交 ⇒ `Duplicate`。
    ///
    /// 红检：删来源闸 ⇒ 第一条红；删身份账本 ⇒ 第二条红。
    #[test]
    fn outbox_counts_not_owner_and_duplicate() {
        let z0 = ZoneId::new(0);
        let z1 = ZoneId::new(1);
        // ① 非所属区发射。
        let mut outbox = CrossZoneOutbox::new(2);
        outbox.begin_tick(1).expect("begin");
        let err = outbox
            .submit(z1, z0, z0, xfer(0, frame_key(1, 7, 2, 1, 12)))
            .expect_err("非所属区必须被拒");
        assert!(matches!(
            err,
            OutboxError::NotOwner { src, owner_zone, owner }
                if src == z1 && owner_zone == z0 && owner == 7
        ));
        let ledger = outbox.seal_tick().expect("seal");
        assert_eq!((ledger.sent, ledger.recv, ledger.not_owner), (0, 0, 1));
        assert_eq!(ledger.check(), Err(LedgerViolation::NotOwner(1)));

        // ② 同身份两个发射者（同一所属区重复发）。
        let mut outbox = CrossZoneOutbox::new(2);
        outbox.begin_tick(1).expect("begin");
        outbox
            .submit(z0, z0, z0, xfer(0, frame_key(1, 7, 2, 1, 12)))
            .expect("首次");
        let err = outbox
            .submit(z0, z0, z0, xfer(1, frame_key(1, 7, 2, 1, 12)))
            .expect_err("重复身份必须被拒");
        assert!(matches!(err, OutboxError::Duplicate { .. }));
        let ledger = outbox.seal_tick().expect("seal");
        assert_eq!((ledger.sent, ledger.recv, ledger.dup), (1, 1, 1));
        assert_eq!(ledger.check(), Err(LedgerViolation::Duplicate(1)));
    }

    /// **never 反例**：目标区越界 ⇒ 投不进任何箱、计数 `never`。
    ///
    /// 红检：删 `dst.index() >= zone_count` 的判断 ⇒ 越界下标 panic（或串箱）⇒ 红。
    #[test]
    fn outbox_counts_never_for_out_of_range_destination() {
        let z0 = ZoneId::new(0);
        let mut outbox = CrossZoneOutbox::new(2);
        outbox.begin_tick(9).expect("begin");
        let err = outbox
            .submit(z0, z0, ZoneId::new(2), xfer(0, frame_key(9, 1, 0, 0, 1)))
            .expect_err("越界目标区");
        assert!(matches!(err, OutboxError::BadZone { zone_count: 2, .. }));
        let ledger = outbox.seal_tick().expect("seal");
        assert_eq!((ledger.sent, ledger.never), (0, 1));
        assert_eq!(ledger.check(), Err(LedgerViolation::Never(1)));
    }

    /// **生命周期牙**：未 begin 不能提交；未 seal 不能再 begin；帧 tick 必须等于当前 tick。
    #[test]
    fn outbox_tick_lifecycle_is_enforced() {
        let z0 = ZoneId::new(0);
        let mut outbox = CrossZoneOutbox::new(1);
        assert_eq!(
            outbox.submit(z0, z0, z0, xfer(0, frame_key(1, 1, 0, 0, 1))),
            Err(OutboxError::NoActiveTick)
        );
        outbox.begin_tick(1).expect("begin");
        assert_eq!(
            outbox.begin_tick(2),
            Err(OutboxError::Nested {
                active: 1,
                attempt: 2
            })
        );
        assert_eq!(
            outbox.submit(z0, z0, z0, xfer(0, frame_key(2, 1, 0, 0, 1))),
            Err(OutboxError::TickMismatch {
                active: 1,
                frame: 2
            })
        );
        outbox.seal_tick().expect("seal");
        assert_eq!(outbox.seal_tick(), Err(OutboxError::NoActiveTick));
    }

    fn entity(id: i32, name: &str, x: i32, y: i32) -> EntitySpawn {
        let mut e = EntitySpawn::new(id, name.to_string(), x, y);
        e.race = 80;
        e
    }

    /// **快照牙**：边界带只含「已落格 + 正确地图 + 距边 ≤ width」的实体；坐标是本 tick 值。
    #[test]
    fn snapshot_only_includes_this_zone_border_band() {
        let mut world = World::new(20, 20);
        world.add_entity(entity(1, "edge", 9, 5)); // 带内（距右边界 0）
        world.add_entity(entity(2, "deep", 4, 5)); // 距边 5 > width 3 ⇒ 带外
        world.add_entity(entity(3, "outside", 10, 5)); // 矩形外（rect 到 9）
                                                       // `add_entity` 只入队 `Command::Enter`；跑一个 tick 才落图（`add_to_mapped=true`）。
        world.tick();
        let rect = ZoneRect::new(0, 0, 9, 19);
        let snap = snapshot_border(&world, 0, rect, 3, 7);
        assert_eq!(snap.tick, 7);
        assert_eq!(
            snap.entities.iter().map(|e| e.actor_id).collect::<Vec<_>>(),
            vec![1],
            "只含带内已落格实体"
        );
    }

    /// **交换牙**：按 tick 双缓冲；读别的 tick ⇒ `None`（不得把陈旧槽伪装成空快照）。
    #[test]
    fn exchange_is_double_buffered_and_rejects_stale_ticks() {
        let ex = HaloExchange::new(2);
        let zone = ZoneId::new(1);
        ex.publish(
            zone,
            HaloSnapshot {
                tick: 4,
                entities: vec![HaloEntity {
                    actor_id: 9,
                    map_id: 0,
                    x: 1,
                    y: 2,
                    view_range: 12,
                    name: "m".to_string(),
                }],
            },
        );
        assert!(ex.read(zone, 4).is_some(), "本 tick 可读");
        assert!(ex.read(zone, 5).is_none(), "其它 tick 的槽必须拒绝");
        assert!(ex.read(ZoneId::new(0), 4).is_none(), "未发布区 ⇒ None");
        // tick 5 发布会写另一个槽；tick 4 的槽仍在（双缓冲）。
        ex.publish(
            zone,
            HaloSnapshot {
                tick: 5,
                entities: vec![],
            },
        );
        assert!(ex.read(zone, 4).is_some(), "tick 4 槽未被 tick 5 覆盖");
        assert_eq!(
            ex.read(zone, 5),
            Some(HaloSnapshot {
                tick: 5,
                entities: vec![]
            })
        );
    }

    /// **屏障牙**：2 区内两道屏障各放行 2 个参与者（不死锁）。
    #[test]
    fn barriers_release_all_participants_twice() {
        let barriers = std::sync::Arc::new(TickBarriers::new(2));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let b = std::sync::Arc::clone(&barriers);
            handles.push(std::thread::spawn(move || {
                b.wait_after_phase1();
                b.wait_after_phase2();
            }));
        }
        for h in handles {
            h.join().expect("屏障线程");
        }
    }
}
