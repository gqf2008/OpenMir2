//! **L2 区模型（L2-1 恒等脚手架 / L2-2 区级归属）**
//!
//! "区（zone）" = 运行期单位：一个 World + 一条（普通, 优先）入站队列 + 一个固定 owner 线程
//! （`docs/issues/l2-zone-split-plan-20261122.md` §2.1）。
//!
//! - **L2-1**：`single_host` 单区恒等——所有地图所有坐标都归区 0；队列表长度 = 区数；
//!   N=1 时就是原来那两条 `Receiver`，**运行期零分配、零日志、零额外线程**（启动期 1 次
//!   `Vec<ZoneChannels>` 分配，长度 1）。
//! - **L2-2**：`from_spans` 多矩形切分——区 = `(map_id, 闭区间矩形)` 的集合（区号 = 下标）；
//!   构造时校验「同图矩形不重叠、矩形合法、地图在册」；`zone_of` 给出坐标归属，
//!   `require_owned` 是**宿主侧 fail-closed 守卫**（口径同 `MapNotInShard`，不新造错误码）。
//!   本步**不启多区生产**：`run_inner` 仍只构造 `single_host`；不建线程、不加日志。
//!
//! 几何只保留当前断言需要的原语：`contains`（点）与 `intersects`（矩形不重叠校验）；
//! 等 L2-3 有 halo/边界断言再扩。
//!
//! **L2-4 覆盖校验的等价前提**：`面积和 == 地图面积 + 界内 + 不重叠 ⇒ 并集 = 整图`
//! **只对轴对齐矩形成立**（矩形互斥且都在图内，面积补齐即无缺口）。将来允许**非矩形区**
//! （L2 之后）时，这条等价失效，必须换回逐格计数/扫描——不得沿用面积捷径。

use std::fmt;
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

impl fmt::Display for ZoneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 闭区间格子矩形（`x1`/`y1` **含端点**）。
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

    /// 坐标是否在**边界带**内：在矩形内，且到某条边的格距 ≤ `width`（L2-3 的 halo 范围）。
    #[must_use]
    pub const fn border_contains(&self, x: i32, y: i32, width: i32) -> bool {
        self.contains(x, y)
            && (x - self.x0 <= width
                || self.x1 - x <= width
                || y - self.y0 <= width
                || self.y1 - y <= width)
    }

    /// 两矩形是否相交（闭区间；L2-2 的「不得双属」构造校验用）。
    #[must_use]
    pub const fn intersects(&self, other: &Self) -> bool {
        self.x0 <= other.x1 && other.x0 <= self.x1 && self.y0 <= other.y1 && other.y0 <= self.y1
    }
}

/// 一个区在**一张图**上的矩形（区号 = [`RegionLayout::from_spans`] 的下标）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionSpan {
    pub map_id: usize,
    pub rect: ZoneRect,
}

/// **L2-4**：带显式区号的 span（一区可有多段；区号由 `MIR2_ZONES` 给出）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZoneSpan {
    pub zone: ZoneId,
    pub map_id: usize,
    pub rect: ZoneRect,
}

impl ZoneSpan {
    /// 由区号/地图/矩形构造。
    #[must_use]
    pub const fn new(zone: ZoneId, map_id: usize, rect: ZoneRect) -> Self {
        Self { zone, map_id, rect }
    }
}

impl RegionSpan {
    /// 由地图与矩形构造。
    #[must_use]
    pub const fn new(map_id: usize, rect: ZoneRect) -> Self {
        Self { map_id, rect }
    }
}

/// 布局构造错误（fail-closed：任何一条都必须让调用方拒绝启动/拒绝该布局）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionLayoutError {
    /// 一个区都没有。
    Empty,
    /// `map_id` 超出注册表地图数。
    MapOutOfRange {
        /// 配置里写的地图号。
        map_id: usize,
        /// 布局覆盖的地图数。
        map_count: usize,
    },
    /// 矩形非法（负起点或 `x0 > x1` / `y0 > y1`）。
    InvalidRect {
        /// 该 span 的地图号。
        map_id: usize,
        /// 非法矩形。
        rect: ZoneRect,
    },
    /// 同图两个矩形相交（会导致一个坐标双属 ⇒ 归属不确定）。
    Overlap {
        /// 相交所在的地图号。
        map_id: usize,
        /// 前一个 span 的下标。
        a: usize,
        /// 后一个 span 的下标。
        b: usize,
    },
    /// 区号越界。
    ZoneOutOfRange { zone: u32, zone_count: usize },
    /// 某区没有任何 span。
    EmptyZone(usize),
    /// 矩形越出地图范围。
    RectOutOfBounds { map_id: usize, rect: ZoneRect },
    /// 某图的 span 并集未覆盖整图。
    MapNotCovered {
        map_id: usize,
        covered: i64,
        expected: i64,
    },
}

impl fmt::Display for RegionLayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "区布局为空（至少一个区）"),
            Self::MapOutOfRange { map_id, map_count } => {
                write!(f, "map_id={map_id} 越界（布局覆盖 {map_count} 张图）")
            }
            Self::InvalidRect { map_id, rect } => write!(
                f,
                "map_id={map_id} 的矩形非法（负起点或 x0>x1/y0>y1）：({}, {})..=({}, {})",
                rect.x0, rect.y0, rect.x1, rect.y1
            ),
            Self::Overlap { map_id, a, b } => write!(
                f,
                "map_id={map_id} 的区 {a} 与区 {b} 矩形相交（一个坐标不得双属）"
            ),
            Self::ZoneOutOfRange { zone, zone_count } => {
                write!(f, "区号 {zone} 越界（配置 {zone_count} 个区）")
            }
            Self::EmptyZone(z) => write!(f, "区 {z} 一个 span 都没有"),
            Self::RectOutOfBounds { map_id, rect } => write!(
                f,
                "map_id={map_id} 的矩形越界：({}, {})..=({}, {})",
                rect.x0, rect.y0, rect.x1, rect.y1
            ),
            Self::MapNotCovered {
                map_id,
                covered,
                expected,
            } => write!(
                f,
                "map_id={map_id} 未覆盖整图（已覆盖 {covered} 格 / 共 {expected} 格）"
            ),
        }
    }
}

impl std::error::Error for RegionLayoutError {}

/// 宿主侧 fail-closed 结果：坐标不属于给定区。
///
/// **口径同 `MapNotInShard`**：可区分错误、可计数；本步只在宿主侧提供判定，L2-4 在
/// world 边界把它映射成既有拒绝路径（**不新造协议错误码**）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionRefusal {
    /// 期望的区。
    pub zone: ZoneId,
    /// 地图号。
    pub map_id: usize,
    /// 坐标。
    pub x: i32,
    /// 坐标。
    pub y: i32,
}

impl fmt::Display for RegionRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "坐标 map_id={} ({}, {}) 不属于区 {}（fail-closed；口径同 MapNotInShard）",
            self.map_id, self.x, self.y, self.zone
        )
    }
}

impl std::error::Error for RegionRefusal {}

/// 区布局。
///
/// - [`Self::single_host`]：L2 未开启的唯一形态——单区、任意坐标归区 0；
/// - [`Self::from_spans`]：L2-2 的多矩形切分——区号 = span 下标。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionLayout {
    map_count: usize,
    zone_count: usize,
    kind: LayoutKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LayoutKind {
    /// 单宿主区：所有地图所有坐标都归区 0。
    WholeHost,
    /// 多区：每个 span 带显式区号。
    Spans(Vec<ZoneSpan>),
}

impl RegionLayout {
    /// 单宿主区：所有地图的所有格子都归 [`ZoneId::new(0)`]（L2 未开启的唯一形态）。
    #[must_use]
    pub const fn single_host(map_count: usize) -> Self {
        Self {
            map_count,
            zone_count: 1,
            kind: LayoutKind::WholeHost,
        }
    }

    /// **L2-2**：由 `(map_id, 矩形)` span 列表构造多区布局（区号 = 下标）。
    ///
    /// 校验（任一条失败 ⇒ 拒绝，不静默降级）：
    /// - 至少一个区；
    /// - `map_id < map_count`；
    /// - 矩形非负且 `x0 <= x1`、`y0 <= y1`；
    /// - **同一张图内任意两个矩形不相交**（一个坐标不得双属）。
    ///
    /// **不校验覆盖 100%**：未覆盖的格子 `zone_of` 返回 `None` ⇒ `require_owned` 拒绝
    /// （fail-closed）；L2-4 的布局文件会在启动时另行做覆盖校验。
    ///
    /// # Errors
    /// 见 [`RegionLayoutError`] 各变体。
    pub fn from_spans(map_count: usize, spans: Vec<RegionSpan>) -> Result<Self, RegionLayoutError> {
        if spans.is_empty() {
            return Err(RegionLayoutError::Empty);
        }
        for (i, span) in spans.iter().enumerate() {
            if span.map_id >= map_count {
                return Err(RegionLayoutError::MapOutOfRange {
                    map_id: span.map_id,
                    map_count,
                });
            }
            let r = span.rect;
            if r.x0 < 0 || r.y0 < 0 || r.x0 > r.x1 || r.y0 > r.y1 {
                return Err(RegionLayoutError::InvalidRect {
                    map_id: span.map_id,
                    rect: r,
                });
            }
            for (j, other) in spans.iter().enumerate().skip(i + 1) {
                if span.map_id == other.map_id && span.rect.intersects(&other.rect) {
                    return Err(RegionLayoutError::Overlap {
                        map_id: span.map_id,
                        a: i,
                        b: j,
                    });
                }
            }
        }
        let zone_count = spans.len();
        let spans = spans
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                ZoneSpan::new(
                    ZoneId::new(u32::try_from(i).expect("区数 <= u32::MAX")),
                    s.map_id,
                    s.rect,
                )
            })
            .collect();
        Ok(Self {
            map_count,
            zone_count,
            kind: LayoutKind::Spans(spans),
        })
    }

    /// **L2-4**：由显式区号的 span 列表 + 地图尺寸构造多区布局（启动期校验）。
    ///
    /// 覆盖等价见模块头注释（**只对轴对齐矩形成立**）：界内 + 不重叠 + 面积和 == 地图面积
    /// ⇒ 并集 = 整图。未被任何 span 提到的图由 `default_zone` 的整图 span 兜底。
    ///
    /// # Errors
    /// 区号越界 / 空区 / 非法矩形 / 越界 / 同图相交 / 覆盖不足 ⇒ 指名 `map_id` 与矩形。
    pub fn from_zone_spans(
        zone_count: usize,
        map_sizes: &[(i32, i32)],
        mut spans: Vec<ZoneSpan>,
        default_zone: ZoneId,
    ) -> Result<Self, RegionLayoutError> {
        if zone_count == 0 {
            return Err(RegionLayoutError::Empty);
        }
        for (map_id, &(w, h)) in map_sizes.iter().enumerate() {
            if !spans.iter().any(|s| s.map_id == map_id) {
                spans.push(ZoneSpan::new(
                    default_zone,
                    map_id,
                    ZoneRect::new(0, 0, w - 1, h - 1),
                ));
            }
        }
        if spans.is_empty() {
            return Err(RegionLayoutError::Empty);
        }
        let mut per_zone = vec![0usize; zone_count];
        for (i, span) in spans.iter().enumerate() {
            let z = span.zone.index();
            if z >= zone_count {
                return Err(RegionLayoutError::ZoneOutOfRange {
                    zone: span.zone.get(),
                    zone_count,
                });
            }
            per_zone[z] += 1;
            let r = span.rect;
            if r.x0 < 0 || r.y0 < 0 || r.x0 > r.x1 || r.y0 > r.y1 {
                return Err(RegionLayoutError::InvalidRect {
                    map_id: span.map_id,
                    rect: r,
                });
            }
            let (w, h) =
                map_sizes
                    .get(span.map_id)
                    .copied()
                    .ok_or(RegionLayoutError::MapOutOfRange {
                        map_id: span.map_id,
                        map_count: map_sizes.len(),
                    })?;
            if r.x1 >= w || r.y1 >= h {
                return Err(RegionLayoutError::RectOutOfBounds {
                    map_id: span.map_id,
                    rect: r,
                });
            }
            for (j, other) in spans.iter().enumerate().skip(i + 1) {
                if span.map_id == other.map_id && span.rect.intersects(&other.rect) {
                    return Err(RegionLayoutError::Overlap {
                        map_id: span.map_id,
                        a: i,
                        b: j,
                    });
                }
            }
        }
        if let Some((z, _)) = per_zone.iter().enumerate().find(|(_, n)| **n == 0) {
            return Err(RegionLayoutError::EmptyZone(z));
        }
        for (map_id, &(w, h)) in map_sizes.iter().enumerate() {
            let covered: i64 = spans
                .iter()
                .filter(|s| s.map_id == map_id)
                .map(|s| {
                    i64::from(s.rect.x1 - s.rect.x0 + 1) * i64::from(s.rect.y1 - s.rect.y0 + 1)
                })
                .sum();
            let expected = i64::from(w) * i64::from(h);
            if covered != expected {
                return Err(RegionLayoutError::MapNotCovered {
                    map_id,
                    covered,
                    expected,
                });
            }
        }
        Ok(Self {
            map_count: map_sizes.len(),
            zone_count,
            kind: LayoutKind::Spans(spans),
        })
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

    /// 某坐标所属区。
    ///
    /// - `WholeHost`：地图在册 ⇒ 区 0；
    /// - `Spans`：命中唯一 span ⇒ 该区；未覆盖/越界地图/负坐标 ⇒ `None`（fail-closed）。
    #[must_use]
    pub fn zone_of(&self, map_id: usize, x: i32, y: i32) -> Option<ZoneId> {
        match &self.kind {
            LayoutKind::WholeHost => (map_id < self.map_count).then_some(ZoneId::new(0)),
            LayoutKind::Spans(spans) => spans
                .iter()
                .find(|s| s.map_id == map_id && s.rect.contains(x, y))
                .map(|s| s.zone),
        }
    }

    /// 某区是否拥有该坐标（`zone_of == Some(zone)`）。
    #[must_use]
    pub fn owns(&self, zone: ZoneId, map_id: usize, x: i32, y: i32) -> bool {
        self.zone_of(map_id, x, y) == Some(zone)
    }

    /// **宿主侧 fail-closed 守卫（L2-2）**：坐标不属于 `zone` ⇒ [`RegionRefusal`]。
    ///
    /// 调用方（L2-4）把失败映射成既有的拒绝路径（口径同 `MapNotInShard`），并累计计数；
    /// 本步只提供判定，不改 world 错误枚举、不加日志。
    ///
    /// # Errors
    /// 坐标不在本区（含未覆盖、越界、负坐标）⇒ [`RegionRefusal`]。
    pub fn require_owned(
        &self,
        zone: ZoneId,
        map_id: usize,
        x: i32,
        y: i32,
    ) -> Result<(), RegionRefusal> {
        if self.owns(zone, map_id, x, y) {
            Ok(())
        } else {
            Err(RegionRefusal { zone, map_id, x, y })
        }
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

    fn two_zone_of_map_zero() -> RegionLayout {
        RegionLayout::from_spans(
            1,
            vec![
                RegionSpan::new(0, ZoneRect::new(0, 0, 9, 9)),
                RegionSpan::new(0, ZoneRect::new(10, 0, 19, 9)),
            ],
        )
        .expect("合法双区切分")
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

    /// **矩形牙**：闭区间含端点；相交判定同为闭区间（L2-2 的双属校验依赖它）。
    #[test]
    fn rect_is_a_closed_interval_and_intersects() {
        let rect = ZoneRect::new(10, 20, 19, 29);
        assert!(rect.contains(10, 20) && rect.contains(19, 29));
        assert!(!rect.contains(9, 20));
        assert!(!rect.contains(20, 20));
        assert!(!rect.contains(10, 30));
        assert!(
            rect.intersects(&ZoneRect::new(19, 29, 30, 40)),
            "端点相接 = 相交"
        );
        assert!(
            !rect.intersects(&ZoneRect::new(20, 20, 30, 30)),
            "相邻但不重叠（19 与 20 不交）"
        );
        // L2-3 边界带：含边、带内为真，带深一格为假。
        let zone = ZoneRect::new(0, 0, 19, 19);
        assert!(zone.border_contains(0, 5, 3) && zone.border_contains(19, 5, 3));
        assert!(zone.border_contains(3, 10, 3), "距左边 3 = 带内（含）");
        assert!(!zone.border_contains(4, 10, 3), "距边 4 > width ⇒ 带外");
        assert!(!zone.border_contains(20, 5, 3), "矩形外 ⇒ 带外");
    }

    /// **L2-2 牙**：双区边界端点各归一侧、**不得双属**；区外/未覆盖 ⇒ `None`。
    ///
    /// 红检：把 `zone_of` 改成「第一个 `x >= x0` 的区」或让闭区间变半开 ⇒ 端点断言红。
    #[test]
    fn two_zone_split_assigns_boundary_points_to_exactly_one_zone() {
        let layout = two_zone_of_map_zero();
        assert_eq!(layout.zone_count(), 2);
        assert_eq!(layout.zone_of(0, 0, 0), Some(ZoneId::new(0)));
        assert_eq!(layout.zone_of(0, 9, 9), Some(ZoneId::new(0)), "左区右端点");
        assert_eq!(layout.zone_of(0, 10, 0), Some(ZoneId::new(1)), "右区左端点");
        assert_eq!(layout.zone_of(0, 19, 9), Some(ZoneId::new(1)));
        assert_eq!(layout.zone_of(0, 20, 0), None, "区外 ⇒ fail-closed");
        assert_eq!(layout.zone_of(0, 5, 10), None, "y 越界 ⇒ fail-closed");
        assert_eq!(layout.zone_of(1, 0, 0), None, "其它图未配置 ⇒ fail-closed");
        assert_eq!(layout.zone_of(0, -1, 0), None, "负坐标 ⇒ fail-closed");
        assert!(layout.owns(ZoneId::new(0), 0, 9, 9));
        assert!(!layout.owns(ZoneId::new(1), 0, 9, 9), "不得双属");
    }

    /// **L2-2 牙（守卫）**：`require_owned` 对区外坐标给出可区分错误，文本点名坐标与区。
    ///
    /// 红检：把 `require_owned` 改成恒 `Ok(())` ⇒ 本用例红。
    #[test]
    fn require_owned_fails_closed_outside_the_zone() {
        let layout = two_zone_of_map_zero();
        assert_eq!(layout.require_owned(ZoneId::new(0), 0, 9, 9), Ok(()));
        let refusal = layout
            .require_owned(ZoneId::new(0), 0, 10, 0)
            .expect_err("右区坐标不属于左区");
        assert_eq!(
            refusal,
            RegionRefusal {
                zone: ZoneId::new(0),
                map_id: 0,
                x: 10,
                y: 0
            }
        );
        let text = refusal.to_string();
        for needle in ["map_id=0", "(10, 0)", "区 0", "MapNotInShard"] {
            assert!(text.contains(needle), "错误文本缺 `{needle}`：{text}");
        }
        assert!(
            layout.require_owned(ZoneId::new(1), 0, 10, 0).is_ok(),
            "坐标属于右区"
        );
        assert!(
            layout.require_owned(ZoneId::new(0), 0, 20, 0).is_err(),
            "未覆盖坐标必须拒绝"
        );
    }

    /// **L2-2 负例牙**：同图矩形相交 ⇒ 构造拒绝（一个坐标不得双属）。
    ///
    /// 红检：删掉 `from_spans` 里的 `intersects` 校验 ⇒ 本用例从 Err 变 Ok ⇒ 红。
    #[test]
    fn overlapping_spans_are_rejected() {
        let err = RegionLayout::from_spans(
            1,
            vec![
                RegionSpan::new(0, ZoneRect::new(0, 0, 9, 9)),
                RegionSpan::new(0, ZoneRect::new(5, 5, 14, 14)),
            ],
        )
        .expect_err("相交必须拒绝");
        assert_eq!(
            err,
            RegionLayoutError::Overlap {
                map_id: 0,
                a: 0,
                b: 1
            }
        );
        assert!(err.to_string().contains("不得双属"));
    }

    /// **L2-2 负例牙**：空布局、越界地图、非法矩形（负起点 / x0>x1）全部拒绝。
    #[test]
    fn invalid_layouts_are_rejected_fail_closed() {
        assert_eq!(
            RegionLayout::from_spans(1, Vec::new()),
            Err(RegionLayoutError::Empty)
        );
        assert_eq!(
            RegionLayout::from_spans(1, vec![RegionSpan::new(1, ZoneRect::new(0, 0, 1, 1))]),
            Err(RegionLayoutError::MapOutOfRange {
                map_id: 1,
                map_count: 1
            })
        );
        assert_eq!(
            RegionLayout::from_spans(1, vec![RegionSpan::new(0, ZoneRect::new(5, 0, 4, 0))]),
            Err(RegionLayoutError::InvalidRect {
                map_id: 0,
                rect: ZoneRect::new(5, 0, 4, 0)
            })
        );
        assert_eq!(
            RegionLayout::from_spans(1, vec![RegionSpan::new(0, ZoneRect::new(-1, 0, 4, 0))]),
            Err(RegionLayoutError::InvalidRect {
                map_id: 0,
                rect: ZoneRect::new(-1, 0, 4, 0)
            })
        );
    }

    /// **L2-2 牙**：区号是**全局 span 下标**（跨地图不重置），避免多图时区号撞车。
    #[test]
    fn zone_ids_are_global_span_indices_across_maps() {
        let layout = RegionLayout::from_spans(
            2,
            vec![
                RegionSpan::new(0, ZoneRect::new(0, 0, 9, 9)),
                RegionSpan::new(0, ZoneRect::new(10, 0, 19, 9)),
                RegionSpan::new(1, ZoneRect::new(0, 0, 4, 4)),
            ],
        )
        .expect("跨图三区合法");
        assert_eq!(layout.zone_count(), 3);
        assert_eq!(layout.zone_of(0, 9, 9), Some(ZoneId::new(0)));
        assert_eq!(layout.zone_of(0, 10, 0), Some(ZoneId::new(1)));
        assert_eq!(layout.zone_of(1, 4, 4), Some(ZoneId::new(2)));
        assert_eq!(layout.zone_of(1, 5, 0), None);
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

    // ---------- L2-4：一区多 span + 覆盖 100% ----------

    fn zs(zone: u32, map: usize, r: ZoneRect) -> ZoneSpan {
        ZoneSpan::new(ZoneId::new(zone), map, r)
    }

    #[test]
    fn zone_spans_split_one_map_into_two_zones() {
        let l = RegionLayout::from_zone_spans(
            2,
            &[(20, 10)],
            vec![
                zs(0, 0, ZoneRect::new(0, 0, 9, 9)),
                zs(1, 0, ZoneRect::new(10, 0, 19, 9)),
            ],
            ZoneId::new(0),
        )
        .expect("合法双区");
        assert_eq!(l.zone_count(), 2);
        assert_eq!(l.zone_of(0, 9, 9), Some(ZoneId::new(0)));
        assert_eq!(l.zone_of(0, 10, 0), Some(ZoneId::new(1)));
        assert_eq!(l.zone_of(0, 20, 0), None);
    }

    #[test]
    fn zone_spans_default_covers_unmentioned_map() {
        let l = RegionLayout::from_zone_spans(
            2,
            &[(20, 10), (4, 4), (2, 2)],
            vec![
                zs(0, 0, ZoneRect::new(0, 0, 9, 9)),
                zs(1, 0, ZoneRect::new(10, 0, 19, 9)),
                zs(1, 1, ZoneRect::new(0, 0, 3, 3)),
            ],
            ZoneId::new(0),
        )
        .expect("map 2 未提到 ⇒ default 区 0 兜底");
        assert_eq!(l.zone_of(2, 1, 1), Some(ZoneId::new(0)));
        assert_eq!(l.zone_of(1, 3, 3), Some(ZoneId::new(1)));
    }

    #[test]
    fn zone_spans_reject_overlap_within_a_zone() {
        let err = RegionLayout::from_zone_spans(
            1,
            &[(20, 10)],
            vec![
                zs(0, 0, ZoneRect::new(0, 0, 9, 9)),
                zs(0, 0, ZoneRect::new(5, 5, 14, 9)),
            ],
            ZoneId::new(0),
        )
        .expect_err("同区重叠必须拒绝");
        assert!(
            matches!(
                err,
                RegionLayoutError::Overlap {
                    map_id: 0,
                    a: 0,
                    b: 1
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn zone_spans_reject_overlap_across_zones() {
        let err = RegionLayout::from_zone_spans(
            2,
            &[(20, 10)],
            vec![
                zs(0, 0, ZoneRect::new(0, 0, 10, 9)),
                zs(1, 0, ZoneRect::new(10, 0, 19, 9)),
            ],
            ZoneId::new(0),
        )
        .expect_err("跨区重叠必须拒绝");
        assert!(
            matches!(err, RegionLayoutError::Overlap { map_id: 0, .. }),
            "{err}"
        );
    }

    #[test]
    fn zone_spans_reject_area_short_by_one() {
        let err = RegionLayout::from_zone_spans(
            2,
            &[(20, 10)],
            vec![
                zs(0, 0, ZoneRect::new(0, 0, 19, 8)),
                zs(1, 0, ZoneRect::new(0, 9, 18, 9)),
            ],
            ZoneId::new(0),
        )
        .expect_err("面积差 1 必须拒绝");
        assert_eq!(
            err,
            RegionLayoutError::MapNotCovered {
                map_id: 0,
                covered: 199,
                expected: 200
            }
        );
        let text = err.to_string();
        for needle in ["map_id=0", "199", "200"] {
            assert!(text.contains(needle), "错误文本缺 `{needle}`：{text}");
        }
    }

    #[test]
    fn zone_spans_reject_out_of_bounds_rect() {
        let err = RegionLayout::from_zone_spans(
            1,
            &[(20, 10)],
            vec![zs(0, 0, ZoneRect::new(0, 0, 20, 9))],
            ZoneId::new(0),
        )
        .expect_err("越界必须拒绝");
        assert_eq!(
            err,
            RegionLayoutError::RectOutOfBounds {
                map_id: 0,
                rect: ZoneRect::new(0, 0, 20, 9)
            }
        );
        let text = err.to_string();
        for needle in ["map_id=0", "(0, 0)..=(20, 9)"] {
            assert!(text.contains(needle), "错误文本缺 `{needle}`：{text}");
        }
    }

    #[test]
    fn zone_spans_reject_half_map_gap() {
        let err = RegionLayout::from_zone_spans(
            1,
            &[(20, 10)],
            vec![zs(0, 0, ZoneRect::new(0, 0, 9, 9))],
            ZoneId::new(0),
        )
        .expect_err("半图缺口必须拒绝");
        assert_eq!(
            err,
            RegionLayoutError::MapNotCovered {
                map_id: 0,
                covered: 100,
                expected: 200
            }
        );
    }
}
