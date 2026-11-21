//! **L2-1 区模型 + 每宿主区队列（恒等脚手架）**
//!
//! "区（zone）" = 运行期单位：一个 World + 一条（普通, 优先）入站队列 + 一个固定 owner 线程
//! （`docs/issues/l2-zone-split-plan-20261122.md` §2.1）。本步只落**区模型与队列表**：
//!
//! - `MIR2_SHARDS=1`（以及任何未开启 L2 的构建）只有一个区，`zone_of` 恒为 [`ZoneId::new(0)`]；
//! - 队列表长度 = 区数；N=1 时就是原来那两条 `Receiver`，**运行期零分配、零日志、零额外线程**
//!   （启动期一次 `Vec<ZoneChannels>` 分配，长度 1）；
//! - 几何（`map_id` + 矩形）在 L2-2 才进入守卫；本步 [`RegionLayout::single_host`] 只记录地图数。
//!
//! 为什么先落这个：L2-4 的「每区队列 + 入站 demux」需要一个稳定的类型接缝；先以恒等形式接进
//! `run_inner`，后续步骤只改「区数/路由」，不动热路径的机器。

use std::sync::mpsc::Receiver;

use crate::GateEvent;

/// 运行期区号（0 起；L2 未开启时恒为 0）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ZoneId(u32);

impl ZoneId {
    /// 由编号构造。
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// 编号。
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// 编号的 `usize` 形态（做下标/索引时用）。
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// 闭区间格子矩形（`x1`/`y1` **含端点**；L2-2 起用于区级归属）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZoneRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl ZoneRect {
    /// 由四角构造（闭区间）。
    #[must_use]
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// 坐标是否落在本矩形内（闭区间）。
    #[must_use]
    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

/// 区布局（L2-1：单区、恒等）。
///
/// L2-2 起扩展为「每张图若干矩形 → 区」；本步只表达「所有地图的所有格子都归区 0」。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionLayout {
    map_count: usize,
    zone_count: usize,
}

impl RegionLayout {
    /// 单宿主区：所有地图的所有格子都归 [`ZoneId::new(0)`]（L2 未开启的唯一形态）。
    #[must_use]
    pub const fn single_host(map_count: usize) -> Self {
        Self {
            map_count,
            zone_count: 1,
        }
    }

    /// 区数。
    #[must_use]
    pub const fn zone_count(&self) -> usize {
        self.zone_count
    }

    /// 布局覆盖的地图数（= 注册表地图数）。
    #[must_use]
    pub const fn map_count(&self) -> usize {
        self.map_count
    }

    /// 某坐标所属区；**越界地图 ⇒ `None`**（与既有 `NoSuchMap` 同口径，不制造新错误）。
    #[must_use]
    pub fn zone_of(&self, map_id: usize, _x: i32, _y: i32) -> Option<ZoneId> {
        if map_id < self.map_count {
            Some(ZoneId::new(0))
        } else {
            None
        }
    }

    /// 某区是否拥有该坐标（L2-1 单区恒等）。
    #[must_use]
    pub fn owns(&self, zone: ZoneId, map_id: usize, x: i32, y: i32) -> bool {
        zone == ZoneId::new(0) && self.zone_of(map_id, x, y).is_some()
    }
}

/// 一个区的两条入站通道（普通, 优先）。`GateEvent` 是 crate 私有 ⇒ 本类型也 `pub(crate)`。
#[derive(Debug)]
pub(crate) struct ZoneChannels {
    normal: Receiver<GateEvent>,
    priority: Receiver<GateEvent>,
}

impl ZoneChannels {
    /// 包装现有两条通道（N=1 时就是 `sim_loop` 原来收到的两条）。
    #[must_use]
    pub(crate) fn new(normal: Receiver<GateEvent>, priority: Receiver<GateEvent>) -> Self {
        Self { normal, priority }
    }

    /// 拆回两条通道（L2-1 的恒等路径；L2-4 改为按区取用）。
    #[must_use]
    pub(crate) fn into_parts(self) -> (Receiver<GateEvent>, Receiver<GateEvent>) {
        (self.normal, self.priority)
    }
}

/// 区宿主计划：布局 + 每区队列表（**长度 = 区数**）。`pub(crate)`（含 `GateEvent` 通道）。
#[derive(Debug)]
pub(crate) struct ZoneHostPlan {
    layout: RegionLayout,
    channels: Vec<ZoneChannels>,
}

impl ZoneHostPlan {
    /// 单宿主区计划（L2 未开启）：一个区，队列表 1 项。
    #[must_use]
    pub(crate) fn single_host(map_count: usize, channels: ZoneChannels) -> Self {
        Self {
            layout: RegionLayout::single_host(map_count),
            channels: vec![channels],
        }
    }

    /// 区布局。
    #[must_use]
    pub(crate) fn layout(&self) -> &RegionLayout {
        &self.layout
    }

    /// 队列表长度（= 区数）。
    #[must_use]
    pub(crate) fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// 取单区通道（L2-1 恒等路径）。
    ///
    /// # Panics
    /// 区数 ≠ 1 ⇒ panic。**防 L2-4 误用**：多区必须走 L2-4 的 demux，不得塞进单区恒等路径。
    #[must_use]
    pub(crate) fn into_single(
        mut self,
    ) -> (RegionLayout, (Receiver<GateEvent>, Receiver<GateEvent>)) {
        assert_eq!(
            self.channels.len(),
            1,
            "L2-1 恒等路径要求恰一个区；区数={}",
            self.channels.len()
        );
        let channels = self.channels.pop().expect("长度已断言为 1");
        (self.layout, channels.into_parts())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn chans() -> ZoneChannels {
        let (normal_tx, normal_rx) = channel();
        let (priority_tx, priority_rx) = channel();
        drop((normal_tx, priority_tx));
        ZoneChannels::new(normal_rx, priority_rx)
    }

    /// **L2-1 牙**：单宿主布局把任意坐标都归区 0；越界地图返回 `None`（不制造新错误）。
    #[test]
    fn single_host_maps_every_coordinate_to_zone_zero() {
        let layout = RegionLayout::single_host(3);
        assert_eq!(layout.zone_count(), 1);
        for map_id in 0..3 {
            assert_eq!(layout.zone_of(map_id, 0, 0), Some(ZoneId::new(0)));
            assert_eq!(
                layout.zone_of(map_id, 9_999, 12_345),
                Some(ZoneId::new(0)),
                "L2-1 不按几何细分"
            );
        }
        assert_eq!(layout.zone_of(3, 0, 0), None, "越界地图不制造新错误");
        assert!(layout.owns(ZoneId::new(0), 2, 1, 1));
        assert!(!layout.owns(ZoneId::new(1), 2, 1, 1));
    }

    /// **矩形牙**：闭区间含端点；宽高按格数算（L2-2 的边界判据依赖这一点）。
    #[test]
    fn rect_is_a_closed_interval() {
        let rect = ZoneRect::new(10, 20, 19, 29);
        assert!(rect.contains(10, 20) && rect.contains(19, 29));
        assert!(!rect.contains(9, 20));
        assert!(!rect.contains(20, 20));
        assert!(!rect.contains(10, 30));
    }

    /// **恒等牙**：单区计划的队列表长度 = 1，拆回的两条通道就是原通道（N=1 零行为差）。
    #[test]
    fn host_plan_single_is_identity() {
        let plan = ZoneHostPlan::single_host(4, chans());
        assert_eq!(plan.channel_count(), 1);
        assert_eq!(plan.layout().zone_count(), 1);
        let (layout, _channels) = plan.into_single();
        assert_eq!(layout.map_count(), 4);
    }

    /// **负例牙（防 L2-4 误用）**：多区走恒等路径 ⇒ panic。
    ///
    /// 红检：删掉 `into_single` 的 `assert_eq!` ⇒ 本用例不再 panic ⇒ 红。
    #[test]
    #[should_panic(expected = "L2-1 恒等路径要求恰一个区")]
    fn host_plan_rejects_multi_zone_in_identity_path() {
        let mut plan = ZoneHostPlan::single_host(1, chans());
        plan.channels.push(chans());
        let _ = plan.into_single();
    }
}
