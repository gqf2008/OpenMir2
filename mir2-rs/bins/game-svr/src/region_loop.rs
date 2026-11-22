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
//!
//! **L2-5 ②（镜像 + 跨区可见）**：`HaloEntity` 扩帧面（`dir/death/展示项/玩家 route`）+
//! [`apply_mirrors`]（barrier1 之后、`tick_phase2` 之前用邻区本 tick 快照刷新本区镜像）。
//! 镜像用 owner 的全局 `ActorId` 插入、只读、不参与模拟；`MIR2_ZONE_MIRROR` 关（缺省）⇒
//! 整条路径零参与（段 6a 的 N≥2 行为逐字节不变）。

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use mir2_world_bevy::{CellType, MirrorSpec, MirrorUpsert, TickRecord, World};

use crate::downlink::{ActorDisplay, DisplayTable};
use crate::perf_counters;
use crate::tick_watchdog::ZoneHeartbeats;
use crate::zone::{RegionLayout, ZoneId, ZoneRect, ZoneSpan};
use crate::zone_router::{ZoneRouter, HOST_ZONE};

/// 玩家的网关路由（`line, socket`；只有玩家有）。
///
/// **L2-5 ⑤**：与 `world_bevy::MirrorRoute` 同一类型——镜像的 route 要经
/// `World::set_mirror_route` 存进世界，供六面在「本地会话未命中」时合成跨区收件人。
pub type HaloRoute = mir2_world_bevy::MirrorRoute;

/// 帧面展示字段（= `downlink::ActorDisplay` 的**逐字节**子集；镜像区据此重建展示表条目）。
///
/// `dir` 也在里面（展示表注册值），但出帧实际用的是快照顶层的 [`HaloEntity::dir`]
/// （= 本 tick 的 `BaseObject.Dir`）——与 `drain_aoi_downlink` 的覆盖口径一致。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HaloDisplay {
    pub dir: u8,
    pub light: u8,
    pub feature: i32,
    pub feature_ex: u16,
    pub status: i32,
    pub name_color: u8,
    /// `GetShowName()` 的 GBK 字节（帧 42 / 出现帧的 `show_name`）。
    pub show_name: Vec<u8>,
    /// `SM_MOUNTRIDE(1118)` 的 `mount_id`（0 = 未骑马）。
    pub mount_id: u8,
}

impl HaloDisplay {
    /// 从 owner 区展示表条目取帧面字段（唯一来源；不猜缺省）。
    #[must_use]
    pub fn from_display(d: &ActorDisplay) -> Self {
        Self {
            dir: d.dir,
            light: d.light,
            feature: d.feature,
            feature_ex: d.feature_ex,
            status: d.status,
            name_color: d.name_color,
            show_name: d.show_name.clone(),
            mount_id: d.mount_id,
        }
    }
}

/// 在线路由来源（`OnlineRegistry` 是本 crate 私有类型 ⇒ 用公开 trait 隔离，避免
/// `private_interfaces`：`HaloFrameSource` 要能被集成测试与本仓 `pub` 面使用）。
pub trait HaloRouteSource {
    /// 玩家实体的 `(line, socket)`；非玩家/未登记 ⇒ `None`。
    fn route_of(&self, actor_id: i32) -> Option<HaloRoute>;
}

/// 快照的**帧面来源**（区域里的 `DisplayTable` + 在线路由表）。
///
/// [`Self::none`] 供启动门与单测：`displays=None`、`routes=None`。N=1 不构造任何快照 ⇒
/// 本结构在单区路径上零参与。
#[derive(Clone, Copy, Default)]
pub struct HaloFrameSource<'a> {
    /// AOI 展示表（帧面字段的唯一来源；缺项 ⇒ 镜像侧同样不登记，与基线「缺项跳过」同口径）。
    pub displays: Option<&'a DisplayTable>,
    /// 在线路由表（玩家的 `(line, socket)`；非玩家恒 `None`）。
    pub routes: Option<&'a dyn HaloRouteSource>,
}

impl HaloFrameSource<'static> {
    /// 不带帧面来源（只比 id/坐标的调用方用）。
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }
}

impl<'a> HaloFrameSource<'a> {
    /// 生产/对拍路径：展示表 + 在线路由表。
    #[must_use]
    pub fn new(displays: &'a DisplayTable, routes: &'a dyn HaloRouteSource) -> Self {
        Self {
            displays: Some(displays),
            routes: Some(routes),
        }
    }
}

/// owner 侧的落格类型（与 `world-bevy` 的 `entity_cell_type` 同口径：商人/NPC ⇒ Merchant、
/// 玩家 ⇒ Play、其余 ⇒ Monster）。AOI 扫描对玩家观察者有 `CellType::Monster` 距离门 ⇒ 镜像
/// 必须按 owner 的类型挂格，否则出现帧面不同。
fn halo_cell_type(e: &mir2_world_bevy::EntityRef<'_>) -> CellType {
    if e.merchant.is_some() || e.npc.is_some() {
        CellType::Merchant
    } else if e.race == mir2_world_bevy::ACTOR_RACE_PLAY {
        CellType::Play
    } else {
        CellType::Monster
    }
}

/// 玩家镜像的 `(line, socket)`：按 `ActorId` 查在线表；非玩家/未登记 ⇒ `None`。
fn halo_route(
    source: &HaloFrameSource<'_>,
    e: &mir2_world_bevy::EntityRef<'_>,
) -> Option<HaloRoute> {
    if e.race != mir2_world_bevy::ACTOR_RACE_PLAY {
        return None;
    }
    source.routes?.route_of(e.entity().id)
}

/// 一个边界实体的只读快照（可见性面 + **帧面**字段；L2-5 ② 扩帧面）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HaloEntity {
    /// 业务 ActorId（跨区唯一，P2-3b Step 3 已按片分段；镜像**不新分配**）。
    pub actor_id: i32,
    /// 地图号（全局下标）。
    pub map_id: usize,
    /// 世界坐标（本 tick 终态）。
    pub x: i32,
    /// 世界坐标（本 tick 终态）。
    pub y: i32,
    /// 观察者视野半径（`PlayObject.cs:904` = 12；非玩家取缺省）。
    pub view_range: i32,
    /// 名字（`Entity.name`）。
    pub name: String,
    /// 行为轴 `Race`（镜像的 `Appearance.race`）。
    pub race: u8,
    /// 外观轴 `RaceImg`（镜像的 `Appearance.race_img`）。
    pub race_img: u8,
    /// 外观 `Appr`。
    pub appr: u16,
    /// `GetMyLight()` 的当前值（镜像 `Appearance.light`）。
    pub light: u8,
    /// 名字颜色（镜像 `Appearance.name_color`）。
    pub name_color: u8,
    /// owner 侧落格类型（镜像挂格用）。
    pub cell_type: CellType,
    /// 本 tick 朝向（oracle `BaseObject.Dir`；出帧用它而不是展示表副本）。
    pub dir: u8,
    /// 本 tick 生死（出帧的死亡/骷髅支用它）。
    pub death: bool,
    /// AOI 可见性/阻挡标志（镜像必须逐项复制；`is_blocking` 与可见性谓词都读它们）。
    pub invisible: bool,
    pub ghost: bool,
    pub fixed_hide_mode: bool,
    pub ob_mode: bool,
    /// 主人（宝宝/分身）；`None` = 无主。
    pub master: Option<i32>,
    /// **格内序（Q1 裁决）**：owner 侧该格 `obj_list` 里的下标（非 actor 对象也计入，保证相对序
    /// 单调）。镜像按它追加 ⇒ 收件人世界的格内序 = owner 侧的「进入该格先后」。
    ///
    /// 为什么不是槽位序：oracle 扫描按 `obj_list` 下标序出帧，而槽位序是实体存储的分配序——
    /// 「后进先出 / 延迟 `Enter`」时两者不等，用槽位序会让同格出现帧相对序与基线不同。
    /// 读不到（未落格/越界）⇒ `usize::MAX`（排最后；正常不会发生）。
    pub cell_pos: usize,
    /// owner 区展示表条目（`None` = owner 侧本就没有 ⇒ 镜像也不登记）。
    pub display: Option<HaloDisplay>,
    /// 玩家网关路由（`None` = 非玩家/未登记）。
    pub route: Option<HaloRoute>,
    /// **L2-5 ③ 段 2（handoff）**：实体已挪进邻区 rect、仍归本区（`Flags.handoff_pending`）。
    ///
    /// 仅用于观测与**定序**（§2.4 第 1 条：收件人区对 `outgoing: true` 的镜像一律追加到格尾、
    /// **忽略** `cell_pos`——那个下标是在源区世界算的，与收件人格内序无关）。
    pub outgoing: bool,
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
/// 过滤：`add_to_mapped`（已落格）+ 地图匹配 + `rect.border_contains(width)` +
/// **`!mirror`**（镜像不得被当成 owner 实体再次发布——否则会形成镜像的镜像）。
/// 入队序 = `iter_ref` 槽位序（确定性）。**必须在 phase1 之后调用**（此时实体状态是本 tick 终态）。
#[must_use]
pub fn snapshot_border(
    world: &World,
    map_id: usize,
    rect: ZoneRect,
    halo_width: i32,
    tick: u64,
    source: &HaloFrameSource<'_>,
) -> HaloSnapshot {
    let entities = world
        .entities
        .iter_ref()
        .filter(|e| {
            // **L2-5 ③ 段 2**：过滤 = rect 边界带 ∪ outgoing 带。
            // outgoing：实体 `handoff_pending`（已挪进邻区 rect、仍归本区）且距本区 rect
            // **≤ HALO**（`within_distance` 把 rect 内部也算进来，两者并集不会漏）。
            let in_band = rect.border_contains(e.x, e.y, halo_width)
                || (e.handoff_pending && rect.within_distance(e.x, e.y, halo_width));
            e.add_to_mapped && !e.mirror && e.map_id == map_id && in_band
        })
        .map(|e| {
            // **合法中间态**：`add_to_mapped` 在生成/登录时置真，进格要等 `Command::Enter` 被
            // 消费（`session.rs`/`monai.rs` 的落图前状态）⇒ 本 tick 的 AOI 也看不见它。
            // 查不到 ⇒ 计数（`mirror_cell_pos_unknown`）+ `usize::MAX` 兜底；②夹具断言计数恒 0。
            let cell_pos = world
                .map(map_id)
                .get_cell_info(e.x, e.y)
                .0
                .obj_list
                .iter()
                .position(|o| o.actor_object && o.cell_obj_id == e.id);
            if cell_pos.is_none() {
                mir2_world_bevy::mirror_metrics::note_mirror_cell_pos_unknown();
            }
            HaloEntity {
                actor_id: e.id,
                map_id: e.map_id,
                x: e.x,
                y: e.y,
                view_range: e.view_range,
                name: e.name.clone(),
                race: e.race,
                race_img: e.race_img,
                appr: e.appr,
                light: e.light,
                name_color: e.name_color,
                cell_type: halo_cell_type(&e),
                dir: e.ai.dir,
                death: e.death,
                invisible: e.invisible,
                ghost: e.ghost,
                fixed_hide_mode: e.fixed_hide_mode,
                ob_mode: e.ob_mode,
                master: e.master,
                cell_pos: cell_pos.unwrap_or(usize::MAX),
                display: source
                    .displays
                    .and_then(|d| d.get(e.id))
                    .map(HaloDisplay::from_display),
                route: halo_route(source, &e),
                outgoing: e.handoff_pending,
            }
        })
        .collect();
    HaloSnapshot { tick, entities }
}

/// **L2-5 ③ 段 2（接线）**：从本区 World 取某实体的全量镜像状态（handoff offer 的载荷）。
///
/// 与 [`snapshot_border`] 的字段集合一致（同一批字段、同一 `halo_cell_type` 口径）；镜像本身
/// 不参与 handoff（调用方先过滤 `!mirror`）。
pub(crate) fn mirror_spec_for(world: &World, id: i32) -> Option<MirrorSpec> {
    let e = world.entities.get(id)?;
    Some(MirrorSpec {
        id: e.id,
        name: e.name.clone(),
        map_id: e.map_id,
        x: e.x,
        y: e.y,
        view_range: e.view_range,
        race: e.race,
        race_img: e.race_img,
        appr: e.appr,
        light: e.light,
        name_color: e.name_color,
        cell_type: halo_cell_type(&e),
        dir: e.ai.dir,
        death: e.death,
        invisible: e.invisible,
        ghost: e.ghost,
        fixed_hide_mode: e.fixed_hide_mode,
        ob_mode: e.ob_mode,
        master: e.master,
    })
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
///
/// **失败快传**：任一线程 panic 后调用 [`Self::abort`]，其余线程立刻从屏障上醒来并 panic。
/// `std::sync::Barrier` 没有超时接口——不做快传会让「一区 panic ⇒ 其余区永久阻塞」把整机或
/// 整测试挂死（实测：区线程 panic 后 `l2_two_zone` 卡 180s+，进程 CPU=0）。
#[derive(Debug)]
pub struct TickBarriers {
    after_phase1: FailFastBarrier,
    after_phase2: FailFastBarrier,
}

impl TickBarriers {
    /// 按参与区数构造。
    #[must_use]
    pub fn new(zone_count: usize) -> Self {
        Self {
            after_phase1: FailFastBarrier::new(zone_count),
            after_phase2: FailFastBarrier::new(zone_count),
        }
    }

    /// **失败快传**：区线程在 `catch_unwind` 的 `Err` 分支调用（应在 `resume_unwind` 之前）。
    /// 唤醒两道屏障上的所有等待者并让它们 panic。
    pub fn abort(&self) {
        self.after_phase1.abort();
        self.after_phase2.abort();
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

/// 一通屏障。语义同 [`std::sync::Barrier`]（N 方到达才放行、代际复用），另加 `failed` 标志。
#[derive(Debug)]
struct FailFastBarrier {
    parties: usize,
    /// `(arrived, generation)`。
    state: Mutex<(usize, u64)>,
    cv: std::sync::Condvar,
    failed: std::sync::atomic::AtomicBool,
}

impl FailFastBarrier {
    fn new(parties: usize) -> Self {
        Self {
            parties,
            state: Mutex::new((0, 0)),
            cv: std::sync::Condvar::new(),
            failed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn abort(&self) {
        self.failed.store(true, std::sync::atomic::Ordering::SeqCst);
        let _guard = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        self.cv.notify_all();
    }

    fn is_failed(&self) -> bool {
        self.failed.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn wait(&self) {
        let mut st = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if self.is_failed() {
            drop(st);
            fail_fast_panic();
        }
        let generation = st.1;
        st.0 += 1;
        if st.0 >= self.parties {
            st.0 = 0;
            st.1 = st.1.wrapping_add(1);
            self.cv.notify_all();
            drop(st);
            if self.is_failed() {
                fail_fast_panic();
            }
            return;
        }
        while st.1 == generation {
            st = self.cv.wait(st).unwrap_or_else(|e| e.into_inner());
            if self.is_failed() {
                drop(st);
                fail_fast_panic();
            }
        }
        drop(st);
        if self.is_failed() {
            fail_fast_panic();
        }
    }
}

/// 失败快传的 panic 文本（测试据此判「快传」而不是超时）。
fn fail_fast_panic() -> ! {
    panic!("多区屏障：另一区已失败（panic）⇒ 快速失败，避免永久阻塞在 Barrier 上")
}

/// 屏障等待计时：探针关闭时**不取钟**、返回 0；启用时返回本次等待的 µs。
///
/// 区驱动（[`run_zone_tick`]）把**同一区的两道屏障等待相加**后交给
/// `perf_counters::note_region_barrier_us`；tick 列 `region_barrier_us` 取各区的**最大值**
/// （Amdahl 代价：决定整 tick 墙钟的是最慢的区），`region_barrier_total_us` 是全部区之和
/// （只用于对账）。
fn wait_with_probe(barrier: &FailFastBarrier) -> u64 {
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
    /// 镜像写入被拒（fail-closed）：id 撞真实实体（区 id 段保证不应发生）或格不可用。
    MirrorRefused {
        /// 被拒的实体 id。
        id: i32,
        /// `World::upsert_mirror` 的结果。
        outcome: MirrorUpsert,
    },
}

impl fmt::Display for ZoneTickError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSpans(zone) => write!(f, "区 {zone} 没有任何 span（布局非法或调用方过滤错）"),
            Self::StaleNeighbor { zone, tick } => {
                write!(f, "tick {tick}：区 {zone} 没有发布本 tick 的边界快照")
            }
            Self::MirrorRefused { id, outcome } => {
                write!(
                    f,
                    "镜像 {id} 写入被拒（{outcome:?}）：id 撞真实实体或格不可用"
                )
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
        source: &HaloFrameSource<'_>,
    ) -> Result<(HaloSnapshot, Vec<HaloSnapshot>, u64), ZoneTickError> {
        let border = self.snapshot_publish(world, tick, source)?;
        let (neighbors, wait_phase1_us) = self.await_neighbors(tick)?;
        Ok((border, neighbors, wait_phase1_us))
    }

    /// `after_phase1` 的**前半**：边界快照 → 发布（止于 barrier1 之前）。
    ///
    /// **L2-5 ③ 段 1b** 的锁纪律用：切片区的世界/展示/在线表在 `Mutex` 后，屏障等待期间
    /// **不得持锁**（区 0 线程登录落区会拿同一把锁；持锁等屏障 = 死锁）。拆成
    /// 「持锁段 = 快照/发布」+「无锁段 = 等屏障/读邻居」两步，合起来与 `after_phase1` 同序。
    ///
    /// # Errors
    /// 本区没有 span ⇒ [`ZoneTickError::NoSpans`]。
    pub fn snapshot_publish(
        &self,
        world: &World,
        tick: u64,
        source: &HaloFrameSource<'_>,
    ) -> Result<HaloSnapshot, ZoneTickError> {
        let border =
            snapshot_zone_border(world, self.zone, self.spans, self.halo_width, tick, source)?;
        self.exchange.publish(self.zone, border.clone());
        Ok(border)
    }

    /// `after_phase1` 的**后半**：barrier1 → 读邻居。返回（邻区快照, barrier1 等待 µs）。
    ///
    /// # Errors
    /// 邻区没发布本 tick ⇒ [`ZoneTickError::StaleNeighbor`]。
    pub fn await_neighbors(&self, tick: u64) -> Result<(Vec<HaloSnapshot>, u64), ZoneTickError> {
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
        Ok((neighbors, wait_phase1_us))
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
    source: &HaloFrameSource<'_>,
) -> Result<HaloSnapshot, ZoneTickError> {
    let mut entities = Vec::new();
    let mut span_count = 0usize;
    for span in spans.iter().filter(|s| s.zone == zone) {
        span_count += 1;
        entities.extend(
            snapshot_border(world, span.map_id, span.rect, halo_width, tick, source).entities,
        );
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
    source: &HaloFrameSource<'_>,
    mirrors: bool,
    beat: Option<(&ZoneHeartbeats, u64)>,
) -> Result<ZoneTickResult, ZoneTickError> {
    // 1) phase1：所有写位置/属性/HP 的系统跑完 ⇒ 实体状态是本 tick 终态。
    let mut record = world.tick_phase1();

    // 2) 快照 → 发布 → barrier1 → 读邻居（缺 ⇒ fail-closed）。
    let runtime = ZoneRuntime::new(zone, spans, halo_width, exchange, barriers);
    let (border, neighbors, wait_phase1_us) = runtime.after_phase1(world, tick, source)?;

    // 2b) 镜像刷新：barrier1 之后（邻区快照已是本 tick 终态）、phase2（AOI）之前。
    //     `mirrors == false` ⇒ 零参与（N=1 与 ②第 1 步的默认态；生产 N≥2 由
    //     `MIR2_ZONE_MIRROR` decide，见 `ZoneHost::mirror_enabled`）。
    if mirrors {
        // 最小区没有展示表/在线表 ⇒ `None`（镜像帧面由区 0 一侧负责；本区暂无会话）。
        let _stats = apply_mirrors(world, &neighbors, None)?;
    }

    // 3) phase2（AOI）→ 心跳 → barrier2 → 探针记账。
    world.tick_phase2(&mut record);
    runtime.after_phase2(wait_phase1_us, beat);

    Ok(ZoneTickResult {
        record,
        border,
        neighbors,
    })
}

/// 一区的镜像应用读数（②对拍/诊断用；不参与判定）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MirrorStats {
    /// 新插入的镜像数。
    pub inserted: usize,
    /// 原地刷新的镜像数。
    pub updated: usize,
    /// 本 tick 消失并移除的镜像数。
    pub removed: usize,
    /// **L2-5 ③ 段 2**：镜像被本区真实体占位而跳过的条数（**不区分** handoff 窗口内与静态图
    /// 实体撞名；两区各装 322 张图时静态 NPC/怪同 id ⇒ 稳态也非 0，**不得**当红线）。
    pub shadowed_by_local: usize,
    /// **L2-5 ③ 段 4（总监裁决）**：镜像落格失败（`RefusedCell`）**被丢弃**的条数（不再 abort）。
    pub mirror_write_refused_cell: usize,
}

/// **L2-5 ②**：用 barrier1 之后读到的**邻区本 tick** 快照刷新本区镜像。
///
/// 时序：`after_phase1`（快照/发布/barrier1/读邻居）之后、`tick_phase2`（AOI）之前。
/// 邻居槽由 `HaloExchange::read(tick)` 保证是本 tick（陈旧 ⇒ `StaleNeighbor`，调用方 fail-closed）。
///
/// - 邻区快照里的实体 ⇒ [`World::upsert_mirror`]（**同一全局 id，不新分配**）；
/// - 本 tick 没有任何邻区快照包含的镜像 ⇒ 移除（消失帧由下一次 AOI 扫描的 `hides` 自然覆盖，
///   不需要单独实现跨区 disappear）；
/// - `displays` = **收件人区**的展示表：镜像条目逐字段取快照的 `display`；快照没有展示项
///   （owner 侧本来就没有）⇒ 摘掉（与基线「展示表缺项 ⇒ 跳过出帧」同口径）。
///
/// # 拒绝口径（L2-5 ③ 段 4，总监 2026-11-22 裁决）
/// `upsert_mirror` 的两类拒绝**都不 abort 整机**：`RefusedOccupied` ⇒ 跳过 + `mirror_shadowed_by_local`；
/// `RefusedCell` ⇒ **丢弃该条镜像** + `mirror_write_refused_cell` + 限流 WRN（缺口不许静默，
/// 该计数进 perf TSV，跨界验收必须展示）。abort 只留给不可能状态。
pub fn apply_mirrors(
    world: &mut World,
    neighbors: &[HaloSnapshot],
    mut displays: Option<&mut DisplayTable>,
) -> Result<MirrorStats, ZoneTickError> {
    let mut stats = MirrorStats::default();
    let mut seen: std::collections::HashSet<i32> = std::collections::HashSet::new();
    // **同格序（Q1 裁决 / §9.1.2）**：先按 `(map, x, y, cell_pos, id)` 定序，再逐条 upsert。
    //
    // **L2-5 ③ 段 2（§2.4 第 1 条）**：`outgoing: true` 的镜像一律**追加到格尾**、**忽略**
    // `cell_pos`——基线里跨界那一步是 `move_object` 的 push（追加到目标格尾部），而 `cell_pos`
    // 是在**源区**世界算的（混有源区的镜像），与收件人格内序无关。排序键把 outgoing 排在同格
    // 非 outgoing 之后；outgoing 组内退化为 `(id)`。
    // 镜像的格子登记是「追加」⇒ 只有按 owner 侧格内序追加，收件人世界的 `obj_list` 序才与基线
    // 一致（否则出现帧相对序会不同）。跨格定序只影响镜像槽位序（确定性），不影响扫描（按坐标）。
    let mut ordered: Vec<&HaloEntity> = neighbors
        .iter()
        .flat_map(|snap| snap.entities.iter())
        .collect();
    ordered.sort_by_key(|e| {
        (
            e.map_id,
            e.x,
            e.y,
            u8::from(e.outgoing),
            if e.outgoing { 0 } else { e.cell_pos },
            e.actor_id,
        )
    });
    for e in ordered {
        {
            seen.insert(e.actor_id);
            let spec = MirrorSpec {
                id: e.actor_id,
                name: e.name.clone(),
                map_id: e.map_id,
                x: e.x,
                y: e.y,
                view_range: e.view_range,
                race: e.race,
                race_img: e.race_img,
                appr: e.appr,
                light: e.light,
                name_color: e.name_color,
                cell_type: e.cell_type,
                dir: e.dir,
                death: e.death,
                invisible: e.invisible,
                ghost: e.ghost,
                fixed_hide_mode: e.fixed_hide_mode,
                ob_mode: e.ob_mode,
                master: e.master,
            };
            // **L2-5 ④b 诊断**：`upsert_mirror` 消费 `spec`；先取采样所需的只读字段（只 clone 名字）。
            let diag = (
                spec.id,
                spec.map_id,
                spec.x,
                spec.y,
                spec.race,
                spec.name.clone(),
            );
            match world.upsert_mirror(spec) {
                MirrorUpsert::Inserted => {
                    stats.inserted += 1;
                    // **L2-5 ⑤**：route 与镜像同生命周期（快照 `HaloEntity::route` = 本 tick 邻区
                    // 在线登记；非玩家/未登记 ⇒ None）。写入失败的两支下面清掉，陈旧镜像的
                    // route 随 `remove_mirror` 一起消失。
                    world.set_mirror_route(e.actor_id, e.route);
                }
                MirrorUpsert::Updated => {
                    stats.updated += 1;
                    world.set_mirror_route(e.actor_id, e.route);
                }
                // **L2-5 ③ 段 2（兜底，防御不是牙）**：该 id 已是本区**真实**实体 ⇒ handoff 收敛
                // 的正常面（目标区先提升、源区快照里还有它一 tick 的残留）。**跳过 + 计数**，
                // 不 abort、不删本地实体（§2.3 必带要求 3）。时序正确时该计数恒 0。
                MirrorUpsert::RefusedOccupied => {
                    // **L2-5 ⑤**：路由清理是 `World::upsert_mirror` 的不变量（拒绝支清），
                    // 这里不再重复。
                    // 该 id 在本区已是**真实体**（玩家 / 静态 NPC / 怪，均可能）⇒ 跳过。
                    // **L2-5 ④b 诊断（2026-11-22）**：先按本地真实体类别拆两列（静态 / 玩家）
                    // 并限流采样——两处文档对「非 0 区是否有静态实体」自相矛盾，先用读数裁决，
                    // 再决定「静态归属」怎么修（不许先选一个说法往下写）。
                    let local = world.entities.get(diag.0);
                    let local_race = local.as_ref().map(|l| l.race);
                    if local_race == Some(mir2_world_bevy::ACTOR_RACE_PLAY) {
                        mir2_world_bevy::mirror_metrics::note_mirror_shadowed_handoff();
                    } else {
                        mir2_world_bevy::mirror_metrics::note_mirror_shadowed_static();
                    }
                    mir2_world_bevy::mirror_metrics::note_mirror_shadowed_by_local();
                    mir2_world_bevy::mirror_metrics::note_mirror_write_refused_occupied();
                    stats.shadowed_by_local += 1;
                    static SHADOW_LOGGED: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(0);
                    let n = SHADOW_LOGGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if n <= 8 || n.is_multiple_of(1000) {
                        let seg = |id: i32| id / crate::shard::ACTOR_ID_SEGMENT;
                        let (lm, lx, ly, lp, lname) = local
                            .as_ref()
                            .map_or((usize::MAX, 0, 0, false, String::new()), |l| {
                                (l.map_id, l.x, l.y, l.handoff_pending, l.name.clone())
                            });
                        let (did, dmap, dx, dy, drace, dname) = diag;
                        crate::glog!(
                            "WRN",
                            "MIRROR_SHADOW id={} 段={} 本地race={:?} 名={} 图={} 格=({},{}) pending={} | 镜像 图={} ({},{}) race={} name={}；累计 {}",
                            did,
                            seg(did),
                            local_race,
                            lname,
                            lm,
                            lx,
                            ly,
                            lp,
                            dmap,
                            dx,
                            dy,
                            drace,
                            dname,
                            n
                        );
                    }
                }
                MirrorUpsert::RefusedCell => {
                    // **段 4 裁决**：单条镜像写失败不 abort 整机 ⇒ 丢弃 + 计数 + 限流 WRN。
                    // 原因通常是该条镜像的坐标在本区地图上不是可走格（`cell.valid()` 只认
                    // `CellAttribute::Walk`）；实体本体不受影响。
                    mir2_world_bevy::mirror_metrics::note_mirror_write_refused_cell();
                    // **L2-5 ⑤**：route 清理在 `World::upsert_mirror` 的拒绝支（不变量），不重复。
                    stats.mirror_write_refused_cell += 1;
                    static REFUSED_CELL_LOGGED: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(0);
                    let n =
                        REFUSED_CELL_LOGGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if n <= 8 || n.is_multiple_of(1000) {
                        crate::glog!(
                            "WRN",
                            "镜像写失败（已丢弃，不 abort）：id={} map={} ({},{}) cell_type={:?} 原因=cell（不可走/越界）；累计 {}",
                            e.actor_id,
                            e.map_id,
                            e.x,
                            e.y,
                            e.cell_type,
                            n
                        );
                    }
                }
            }
            if let Some(d) = displays.as_deref_mut() {
                match &e.display {
                    Some(hd) => d.set(
                        e.actor_id,
                        ActorDisplay {
                            dir: hd.dir,
                            light: hd.light,
                            feature: hd.feature,
                            feature_ex: hd.feature_ex,
                            status: hd.status,
                            name_color: hd.name_color,
                            show_name: hd.show_name.clone(),
                            mount_id: hd.mount_id,
                        },
                    ),
                    None => d.remove(e.actor_id),
                }
            }
        }
    }
    // 陈旧镜像清扫：`mirror_ids` 是槽位序（确定性），不在本 tick 任何邻区快照里 ⇒ 移除。
    for id in world.mirror_ids() {
        if !seen.contains(&id) {
            if world.remove_mirror(id) {
                stats.removed += 1;
            }
            if let Some(d) = displays.as_deref_mut() {
                d.remove(id);
            }
        }
    }
    Ok(stats)
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

/// **L2-5 §7：单一发射者判据的口径**（两种并存，旧形态保留给 L2-4 用例/对照）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmitterCriterion {
    /// **L2-4 形态（已被 §7 取代，仅保留对照/旧用例）**：发射者 = 实体坐标所属区。
    OwnerByRect,
    /// **L2-5 形态（镜像模型）**：发射者 = **收件人所在区**——镜像占同一格、扫描序可复现，
    /// 出现帧必须由收件人区发出；否则镜像模型恒红（§7 的约 90% 反例）。
    RecipientZone,
}

/// 跨区 outbox：**唯一发射者**（口径见 [`EmitterCriterion`]），帧按收件人所属区入箱，
/// 目标区转发到 gate。
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
    /// §7 的发射者口径（`new` = 旧形态；镜像 rig 用 [`Self::with_criterion`] 切到收件人区）。
    criterion: EmitterCriterion,
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
        Self::with_criterion(zone_count, EmitterCriterion::OwnerByRect)
    }

    /// 指定 §7 口径构造（镜像模型用 [`EmitterCriterion::RecipientZone`]）。
    #[must_use]
    pub fn with_criterion(zone_count: usize, criterion: EmitterCriterion) -> Self {
        Self {
            zone_count,
            criterion,
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
        // §7：旧口径看「实体坐标所属区」，新口径看「收件人所在区」。
        let allowed = match self.criterion {
            EmitterCriterion::OwnerByRect => owner_zone,
            EmitterCriterion::RecipientZone => dst,
        };
        if src != allowed {
            self.not_owner += 1;
            return Err(OutboxError::NotOwner {
                src,
                owner_zone: allowed,
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
            &HaloFrameSource::none(),
            false,
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
                run_zone_tick(
                    &mut world,
                    zone,
                    &spans,
                    &exchange,
                    &barriers,
                    3,
                    1,
                    &HaloFrameSource::none(),
                    false,
                    None,
                )
                .map(|r| {
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
            &HaloFrameSource::none(),
            false,
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
            &HaloFrameSource::none(),
            false,
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
            &HaloFrameSource::none(),
            false,
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
            &HaloFrameSource::none(),
            false,
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

    /// **§7 新口径（镜像模型）双向负例**：发射者 = **收件人所在区**；旧口径保留为对照。
    ///
    /// ① 收件人区发（src = dst）、实体坐标在别区 ⇒ 绿；
    /// ② 非收件人区发 ⇒ `NotOwner`；
    /// ③ 旧口径（`CrossZoneOutbox::new`）下同一帧 ⇒ 仍红（旧用例语义不变）。
    ///
    /// 红检：把 `submit` 的口径分派删掉（恒按坐标）⇒ ① 红；恒按收件人区 ⇒ ③ 红——
    /// 两个方向都要有，防「闸被拆掉后两边都绿」。
    #[test]
    fn emitter_criterion_recipient_zone_has_two_way_teeth() {
        let z0 = ZoneId::new(0);
        let z1 = ZoneId::new(1);

        // ① src = 收件人区（0），实体坐标在区 1 ⇒ 新口径绿。
        let mut outbox = CrossZoneOutbox::with_criterion(2, EmitterCriterion::RecipientZone);
        outbox.begin_tick(1).expect("begin");
        outbox
            .submit(z0, z1, z0, xfer(0, frame_key(1, 7, 0, 900, 10)))
            .expect("收件人区发帧必须放行");
        assert!(outbox.seal_tick().expect("seal").check().is_ok());

        // ② 非收件人区发 ⇒ NotOwner。
        let mut outbox = CrossZoneOutbox::with_criterion(2, EmitterCriterion::RecipientZone);
        outbox.begin_tick(1).expect("begin");
        let err = outbox
            .submit(z0, z1, z1, xfer(0, frame_key(1, 7, 0, 900, 10)))
            .expect_err("非收件人区必须被拒");
        assert!(
            matches!(err, OutboxError::NotOwner { src, owner_zone, .. } if src == z0 && owner_zone == z1),
            "{err}"
        );
        assert_eq!(
            outbox.seal_tick().expect("seal").check(),
            Err(LedgerViolation::NotOwner(1))
        );

        // ③ 旧口径对照：同一帧仍红（旧用例语义不变）。
        let mut outbox = CrossZoneOutbox::new(2);
        outbox.begin_tick(1).expect("begin");
        assert!(
            outbox
                .submit(z0, z1, z0, xfer(0, frame_key(1, 7, 0, 900, 10)))
                .is_err(),
            "旧口径（坐标所属区）必须仍拒"
        );
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
        let snap = snapshot_border(&world, 0, rect, 3, 7, &HaloFrameSource::none());
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
                    race: 80,
                    race_img: 0,
                    appr: 0,
                    light: 0,
                    name_color: 255,
                    cell_type: CellType::Monster,
                    dir: 0,
                    death: false,
                    invisible: false,
                    ghost: false,
                    fixed_hide_mode: false,
                    ob_mode: false,
                    master: None,
                    cell_pos: 0,
                    display: None,
                    route: None,
                    outgoing: false,
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

    // ---- L2-5 ② 第 1 步：镜像实体（骨架；闸关 ⇒ 零变化） ----

    fn mirror_spec(id: i32, x: i32, y: i32) -> MirrorSpec {
        MirrorSpec {
            id,
            name: format!("m{id}"),
            map_id: 0,
            x,
            y,
            view_range: 5,
            race: 80,
            race_img: 0,
            appr: 1,
            light: 0,
            name_color: 255,
            cell_type: CellType::Monster,
            dir: 3,
            death: false,
            invisible: false,
            ghost: false,
            fixed_hide_mode: false,
            ob_mode: false,
            master: None,
        }
    }

    fn halo_entity(id: i32, x: i32, y: i32) -> HaloEntity {
        HaloEntity {
            actor_id: id,
            map_id: 0,
            x,
            y,
            view_range: 5,
            name: format!("h{id}"),
            race: 80,
            race_img: 0,
            appr: 1,
            light: 0,
            name_color: 255,
            cell_type: CellType::Monster,
            dir: 1,
            death: false,
            invisible: false,
            ghost: false,
            fixed_hide_mode: false,
            ob_mode: false,
            master: None,
            cell_pos: 0,
            display: None,
            route: None,
            outgoing: false,
        }
    }

    fn cell_has(world: &World, x: i32, y: i32, id: i32) -> bool {
        let (cell, ok) = world.map(0).get_cell_info(x, y);
        ok && cell
            .obj_list
            .iter()
            .any(|o| o.actor_object && o.cell_obj_id == id)
    }

    /// **镜像 API 往返**：插入（同一 id + 同步挂格）→ 原地刷新 → 换位 → 移除（摘格）。
    ///
    /// 红检：`upsert_mirror` 去掉 `add_to_mapped`/`add_map_object` ⇒ 首条挂格断言红；
    /// 换位时不摘旧格 ⇒ 「旧格必须摘掉」红；`remove_mirror` 不摘格 ⇒ 末条红。
    #[test]
    fn mirror_upsert_refresh_move_and_remove_round_trip() {
        let mut world = World::new(20, 20);
        assert_eq!(
            world.upsert_mirror(mirror_spec(900, 4, 5)),
            MirrorUpsert::Inserted
        );
        assert!(world.is_mirror(900), "镜像标志");
        assert!(cell_has(&world, 4, 5, 900), "插入即挂格（不走 Enter 队列）");
        assert_eq!(world.mirror_ids(), vec![900]);
        assert_eq!(world.entities.get(900).map(|e| e.dir()), Some(3));

        // 原地刷新（同坐标）：Updated，格不变，帧面字段按快照更新。
        let mut spec = mirror_spec(900, 4, 5);
        spec.dir = 5;
        spec.death = true;
        assert_eq!(world.upsert_mirror(spec), MirrorUpsert::Updated);
        let e = world.entities.get(900).expect("镜像在");
        assert_eq!(e.dir(), 5, "朝向按快照刷新");
        assert!(e.death, "death 按快照刷新");
        assert!(cell_has(&world, 4, 5, 900), "原地刷新不动格");

        // 换位：新格有、旧格无。
        assert_eq!(
            world.upsert_mirror(mirror_spec(900, 7, 9)),
            MirrorUpsert::Updated
        );
        assert!(cell_has(&world, 7, 9, 900), "新格必须有");
        assert!(!cell_has(&world, 4, 5, 900), "旧格必须摘掉");

        assert!(world.remove_mirror(900));
        assert!(!world.is_mirror(900));
        assert!(world.entities.get(900).is_none());
        assert!(!cell_has(&world, 7, 9, 900), "移除必须摘格");
        assert!(world.mirror_ids().is_empty());
    }

    /// **镜像两条 fail-closed**：id 撞真实实体 ⇒ `RefusedOccupied`（不改真实实体）；
    /// 越界格 ⇒ `RefusedCell`（不留半实体）。
    ///
    /// 红检：删 `RefusedOccupied` 分支 ⇒ 首条红；挂格失败不回滚 `entities.remove` ⇒ 末条红。
    #[test]
    fn mirror_upsert_refuses_occupied_id_and_bad_cell() {
        let mut world = World::new(20, 20);
        world.add_entity(entity(7, "real", 3, 3));
        world.tick();
        assert_eq!(
            world.upsert_mirror(mirror_spec(7, 4, 4)),
            MirrorUpsert::RefusedOccupied
        );
        assert!(!world.is_mirror(7), "真实实体不得被改成镜像");
        assert_eq!(
            world.upsert_mirror(mirror_spec(901, 100, 100)),
            MirrorUpsert::RefusedCell
        );
        assert!(world.entities.get(901).is_none(), "挂格失败不得留半实体");
    }

    /// **正向牙（总监条件③）**：镜像进**本地 AOI 扫描**结果——否则「快照过滤 mirror」会把
    /// 可见性一起屏蔽、帧直接消失。**反向牙**：镜像不出现在 owner 侧发布快照里；
    /// 真实实体照旧在（防「过滤过头」）。
    ///
    /// 红检（正向）：`upsert_mirror` 不挂格 ⇒ 扫描看不到 ⇒ 红；
    /// 红检（反向）：删 `snapshot_border` 的 `!e.mirror` ⇒ 反向红。
    #[test]
    fn mirror_is_scanned_by_local_aoi_and_excluded_from_snapshot() {
        let mut world = World::new(20, 20);
        let mut p = entity(1, "O", 5, 5);
        p.race = mir2_world_bevy::ACTOR_RACE_PLAY;
        p.view_range = 5;
        world.add_entity(p);
        world.tick(); // 落图（Enter 在 tick_head 处理）
        assert_eq!(
            world.upsert_mirror(mirror_spec(900, 6, 5)),
            MirrorUpsert::Inserted
        );
        // 玩家扫描门 = `clock - search_tick > 1000`（SearchTime=1000）⇒ 跑到门开的那一拍。
        let mut seen = false;
        for _ in 0..10 {
            world.tick();
            if let Some(o) = world.entities.get(1) {
                if o.visible_actors
                    .iter()
                    .any(|v| v.target == 900 && v.flag == mir2_world_bevy::VisibleFlag::Show)
                {
                    seen = true;
                    break;
                }
            }
        }
        assert!(
            seen,
            "镜像必须出现在本地 AOI 扫描结果里（否则过滤会把可见性一起屏蔽）"
        );

        let snap = snapshot_border(
            &world,
            0,
            ZoneRect::new(0, 0, 19, 19),
            12,
            1,
            &HaloFrameSource::none(),
        );
        assert!(
            !snap.entities.iter().any(|e| e.actor_id == 900),
            "镜像不得被再次发布成 owner 实体（快照过滤 mirror）"
        );
        assert!(
            snap.entities.iter().any(|e| e.actor_id == 1),
            "真实实体仍在快照里（过滤不过头）"
        );
    }

    /// **段 0 牙（Q4）**：快照的 5 个可见性/阻挡标志逐项落到镜像（插入与刷新两条路径）。
    /// 缺一项 ⇒ 拆区会多出可见/挡路的实体（C7 与 A6 都会红）。
    ///
    /// 红检：删 `update_mirror_frame_fields` 任一标志回填（如 `invisible`）⇒ 对应断言红；
    /// 删插入路径的 `spawn.invisible` ⇒ 首条断言红。
    #[test]
    fn mirror_copies_aoi_and_blocking_flags() {
        let mut world = World::new(20, 20);
        let mut h = halo_entity(902, 4, 5);
        h.invisible = true;
        h.ghost = true;
        h.fixed_hide_mode = true;
        h.ob_mode = true;
        h.master = Some(77);
        let snap = HaloSnapshot {
            tick: 1,
            entities: vec![h],
        };
        apply_mirrors(&mut world, std::slice::from_ref(&snap), None).expect("apply");
        let e = world.entities.get(902).expect("镜像在");
        assert!(
            e.invisible && e.ghost && e.fixed_hide_mode && e.ob_mode,
            "4 个状态位必须逐项复制：invisible={} ghost={} fixed_hide={} ob={}",
            e.invisible,
            e.ghost,
            e.fixed_hide_mode,
            e.ob_mode
        );
        assert_eq!(e.master, Some(77), "master 必须复制");

        // 刷新路径（同坐标）同样逐项更新：清回 false/None。
        let h2 = halo_entity(902, 4, 5);
        let snap2 = HaloSnapshot {
            tick: 2,
            entities: vec![h2],
        };
        apply_mirrors(&mut world, std::slice::from_ref(&snap2), None).expect("apply2");
        let e = world.entities.get(902).expect("镜像在");
        assert!(
            !e.invisible && !e.ghost && !e.fixed_hide_mode && !e.ob_mode,
            "刷新必须把状态位清回 false"
        );
        assert_eq!(e.master, None, "刷新必须清 master");
    }

    /// **`apply_mirrors`**：邻区快照 ⇒ 插入 + 登记展示项；下一 tick 快照不含它 ⇒ 移除镜像
    /// 与展示项。消失帧由下次 AOI 扫描的 `hides` 自然覆盖（本用例只钉镜像/展示表状态）。
    ///
    /// 红检：删 `mirror_ids` 清扫循环 ⇒ 「移除」断言红；删 `displays.set` ⇒ 展示断言红。
    #[test]
    fn apply_mirrors_inserts_refreshes_removes_and_syncs_displays() {
        let mut world = World::new(20, 20);
        let mut displays = DisplayTable::new();
        let mut halo = halo_entity(900, 4, 5);
        halo.display = Some(HaloDisplay {
            dir: 2,
            light: 1,
            feature: 0x1234,
            feature_ex: 0,
            status: 0,
            name_color: 255,
            show_name: b"hero".to_vec(),
            mount_id: 0,
        });
        let snap1 = HaloSnapshot {
            tick: 1,
            entities: vec![halo],
        };
        let stats = apply_mirrors(
            &mut world,
            std::slice::from_ref(&snap1),
            Some(&mut displays),
        )
        .expect("apply");
        assert_eq!((stats.inserted, stats.updated, stats.removed), (1, 0, 0));
        assert!(world.is_mirror(900));
        assert_eq!(
            displays.get(900).map(|d| (d.feature, d.show_name.clone())),
            Some((0x1234, b"hero".to_vec()))
        );

        // 下一 tick：快照不再包含它 ⇒ 移除镜像 + 展示项。
        let empty = HaloSnapshot {
            tick: 2,
            entities: Vec::new(),
        };
        let stats = apply_mirrors(
            &mut world,
            std::slice::from_ref(&empty),
            Some(&mut displays),
        )
        .expect("apply empty");
        assert_eq!((stats.inserted, stats.updated, stats.removed), (0, 0, 1));
        assert!(!world.is_mirror(900));
        assert!(displays.get(900).is_none(), "展示项必须随镜像移除");
    }

    /// **C10（②第 2 步）**：镜像消失帧时序——owner 走出边界带 ⇒ 镜像移除；基线的真实实体
    /// 走出视野。两者的消失帧必须同 tick（判据允许晚 1 tick）。
    ///
    /// 夹具：O(8,5) `view_range=3`（视野 x∈[5,11]）；eb 在 (10,5)（区 1，距边界 0）；`halo=3`。
    /// - 基线：tick T 把 eb 向东走 5 格到 (15,5)（距 O 7 > 3 ⇒ 出视野，**实体仍在表里**）；
    /// - 拆区：tick T 发空快照（owner 已在带外）⇒ 镜像被 `apply_mirrors` 移除。
    ///
    /// 红检：把 `run_zone_tick` 的 `apply_mirrors` 挪到 `tick_phase2` 之后 ⇒ 拆区消失帧晚到
    /// 下一次扫描（+6 tick）⇒ 超窗红。
    #[test]
    fn step2_disappear_tick_window() {
        // 玩家扫描门：`clock - search_tick > SearchTime(1000)`，200ms/tick ⇒ 下一次扫描 +6 tick。
        const SCAN_GAP: u64 = 6;
        const HALO: i32 = 3;
        let spans = vec![ZoneSpan::new(ZoneId::new(0), 0, ZoneRect::new(0, 0, 9, 9))];
        let exchange = HaloExchange::new(2);
        let barriers = TickBarriers::new(1);
        let mut world = World::new(20, 10);
        let mut p = entity(1, "O", 8, 5);
        p.race = mir2_world_bevy::ACTOR_RACE_PLAY;
        p.view_range = 3;
        world.add_entity(p);
        let eb = halo_entity(21, 10, 5);

        // 阶段 A：跑到 O 首次看到镜像（Show）。
        let mut seen_at = None;
        let mut tick = 0u64;
        while tick < 20 && seen_at.is_none() {
            exchange.publish(
                ZoneId::new(1),
                HaloSnapshot {
                    tick,
                    entities: vec![eb.clone()],
                },
            );
            let out = run_zone_tick(
                &mut world,
                ZoneId::new(0),
                &spans,
                &exchange,
                &barriers,
                HALO,
                tick,
                &HaloFrameSource::none(),
                true,
                None,
            )
            .expect("区 tick（镜像开）");
            assert!(out.record.hides.is_empty(), "镜像在场时不得有消失帧");
            if world.entities.get(1).is_some_and(|o| {
                o.visible_actors
                    .iter()
                    .any(|v| v.target == 21 && v.flag == mir2_world_bevy::VisibleFlag::Show)
            }) {
                seen_at = Some(tick);
            }
            tick += 1;
        }
        let seen_at = seen_at.expect("20 tick 内必须首次看见镜像");
        let remove_at = seen_at + SCAN_GAP;

        // 阶段 B：继续到 remove_at；该 tick 发空快照（镜像在 phase2 之前被移除）。
        let mut first_hide = None;
        while tick <= remove_at {
            let entities = if tick < remove_at {
                vec![eb.clone()]
            } else {
                Vec::new()
            };
            exchange.publish(ZoneId::new(1), HaloSnapshot { tick, entities });
            let out = run_zone_tick(
                &mut world,
                ZoneId::new(0),
                &spans,
                &exchange,
                &barriers,
                HALO,
                tick,
                &HaloFrameSource::none(),
                true,
                None,
            )
            .expect("区 tick（移除 tick）");
            if first_hide.is_none() && out.record.hides.iter().any(|(o, t)| *o == 1 && *t == 21) {
                first_hide = Some(tick);
            }
            tick += 1;
        }
        // ---- 基线臂：同一个 O + 真实 eb；T 时走出视野（实体保留在实体表）----
        let mut base = World::new(20, 10);
        let mut bp = entity(1, "O", 8, 5);
        bp.race = mir2_world_bevy::ACTOR_RACE_PLAY;
        bp.view_range = 3;
        base.add_entity(bp);
        let real = base.add_entity(entity(21, "eb", 10, 5));

        let mut b_seen = None;
        let mut bt = 0u64;
        while bt < 20 && b_seen.is_none() {
            let mut rec = base.tick_phase1();
            base.tick_phase2(&mut rec);
            if base.entities.get(1).is_some_and(|o| {
                o.visible_actors
                    .iter()
                    .any(|v| v.target == 21 && v.flag == mir2_world_bevy::VisibleFlag::Show)
            }) {
                b_seen = Some(bt);
            }
            bt += 1;
        }
        let b_seen = b_seen.expect("基线 20 tick 内必须首次看见");
        assert_eq!(b_seen, seen_at, "两臂的首扫 tick 必须相同（同一夹具）");
        let b_remove = b_seen + SCAN_GAP;
        let mut b_hide = None;
        while bt <= b_remove {
            if bt == b_remove {
                // 走出视野：5 步东移（(10,5) → (15,5)），实体仍在实体表里。
                for _ in 0..5 {
                    base.walk(real, 1, 0);
                }
            }
            let mut rec = base.tick_phase1();
            base.tick_phase2(&mut rec);
            if b_hide.is_none() && rec.hides.iter().any(|(o, t)| *o == 1 && *t == 21) {
                b_hide = Some(bt);
            }
            bt += 1;
        }
        assert_eq!(
            b_hide,
            Some(b_remove),
            "基线：走出视野的真实实体必须在同一 tick 出消失帧"
        );
        assert_eq!(
            first_hide,
            Some(remove_at),
            "镜像消失帧必须在下一次扫描的同一 tick 出（baseline={:?}）",
            b_hide
        );
        assert!(
            first_hide.unwrap() <= b_hide.unwrap() + 1,
            "镜像消失帧允许晚 1 tick：split={:?} baseline={:?}",
            first_hide,
            b_hide
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
