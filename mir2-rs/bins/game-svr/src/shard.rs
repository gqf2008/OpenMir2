//! 世界分片运行时骨架（卡 `openmir2-shard-runtime-skeleton` / P2-1）。
//!
//! 设计依据：`docs/10k-shard-design-20261116.md` §2.1（同进程多 World）与 §4.2 的 P2-1。
//! 本模块只放**分片配置与宿主侧运行时骨架**，不改 `crates/world-bevy` 的任何语义：
//!
//! - [`ShardId`]：分片编号（0 起；[`HOST_SHARD`] = 宿主当前服务的片）；
//! - [`ShardConfig`]：`MIR2_SHARDS`（**默认 1**；非法值 fail-closed）；
//! - [`ShardLayout`]：`map_id → ShardId` 的**静态**归属表（P2-1 只有单片布局）；
//! - [`ShardRuntime`]：宿主侧运行期句柄（布局 + 跨片拒绝计数）。
//!
//! **`MIR2_SHARDS=1` 零行为变化**：单片布局把每张图都归片 0，
//! [`ShardRuntime::host_owns`] 与 [`ShardRuntime::route_allowed`] 恒真、
//! [`ShardLayout::host_map_sizes`] 原样返回输入 ⇒ 所有守卫都是空操作，
//! 且本模块在片数为 1 时**不打印任何日志**（stdout 与未设该环境变量时逐字节相同）。
//!
//! 本模块**不做**的事（登记给后续卡）：World 实例/线程归属（P2-2/P2-3）、
//! 会话路由与协调器（P2-3/P2-4）、跨片 handoff（P2-5）、确定性守卫（P2-6）。

use std::fmt;

/// 分片数的环境变量名。
pub const ENV_SHARDS: &str = "MIR2_SHARDS";

/// 未配置时的片数（oracle 是单 World；缺省必须与 master 行为一致）。
pub const DEFAULT_SHARD_COUNT: u8 = 1;

/// 片数上界。设计§2.1.2 的甜点是 N=10–12；取 16 只作为**手滑防线**
/// （`MIR2_SHARDS=100` 之类），不是容量承诺。
pub const MAX_SHARD_COUNT: u8 = 16;

/// 宿主当前服务的片号（P2-1/P2-2 阶段只有片 0 承载流量；会话路由在 P2-3）。
pub const HOST_SHARD: ShardId = ShardId::new(0);

/// 分片编号（0 起）。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ShardId(u8);

impl ShardId {
    /// 由编号构造。合法性（`< MIR2_SHARDS`）由布局与调用方校验。
    #[must_use]
    pub const fn new(id: u8) -> Self {
        Self(id)
    }

    /// 编号（`0..MIR2_SHARDS`）。
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// 编号的 `usize` 形态（做下标/索引时用）。
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for ShardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// `MIR2_SHARDS` 的解析错误（fail-closed：任何不认识的值都必须拒绝启动）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShardConfigError {
    /// 环境变量存在但不是合法 Unicode（无法可靠解析）。
    NotUnicode,
    /// 设置了但为空串/全空白。
    Empty,
    /// 不是无符号整数。
    NotANumber(String),
    /// 0 片没有意义。
    Zero,
    /// 超过 [`MAX_SHARD_COUNT`]（手滑防线）。
    TooMany { got: u32, max: u8 },
}

impl fmt::Display for ShardConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUnicode => write!(
                f,
                "{ENV_SHARDS} 不是合法 Unicode，无法解析（fail-closed；不设该变量即默认 {DEFAULT_SHARD_COUNT}）"
            ),
            Self::Empty => write!(
                f,
                "{ENV_SHARDS} 为空（fail-closed；不设该变量即默认 {DEFAULT_SHARD_COUNT}，显式写 1 表示单片）"
            ),
            Self::NotANumber(raw) => write!(
                f,
                "{ENV_SHARDS}={raw} 不是整数（fail-closed；取值 1..={MAX_SHARD_COUNT}）"
            ),
            Self::Zero => write!(
                f,
                "{ENV_SHARDS}=0 没有意义（fail-closed；至少 1 片，缺省 {DEFAULT_SHARD_COUNT}）"
            ),
            Self::TooMany { got, max } => write!(
                f,
                "{ENV_SHARDS}={got} 超过上界 {max}（fail-closed；设计甜点为 N=10–12，见 10k 分片设计 §2.1.2）"
            ),
        }
    }
}

impl std::error::Error for ShardConfigError {}

/// 世界分片配置（P2-1 只有片数；地图组在 P2-2 追加）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShardConfig {
    count: u8,
}

impl ShardConfig {
    /// 缺省单片配置。
    #[must_use]
    pub const fn single() -> Self {
        Self {
            count: DEFAULT_SHARD_COUNT,
        }
    }

    /// 由片数构造（`1..=MAX_SHARD_COUNT`）。
    ///
    /// # Errors
    /// 0 或超过上界 ⇒ [`ShardConfigError`]。
    pub fn from_count(count: u8) -> Result<Self, ShardConfigError> {
        if count == 0 {
            return Err(ShardConfigError::Zero);
        }
        if count > MAX_SHARD_COUNT {
            return Err(ShardConfigError::TooMany {
                got: u32::from(count),
                max: MAX_SHARD_COUNT,
            });
        }
        Ok(Self { count })
    }

    /// 从进程环境读 `MIR2_SHARDS`（未设 ⇒ 默认 1）。
    ///
    /// # Errors
    /// 值非 Unicode / 空 / 非整数 / 0 / 超上界 ⇒ [`ShardConfigError`]。
    pub fn from_env() -> Result<Self, ShardConfigError> {
        match std::env::var_os(ENV_SHARDS) {
            None => Ok(Self::single()),
            Some(raw) => {
                let raw = raw
                    .into_string()
                    .map_err(|_| ShardConfigError::NotUnicode)?;
                Self::from_raw(Some(&raw))
            }
        }
    }

    /// 解析 `MIR2_SHARDS` 的原始值（`None` = 未设置）。纯函数，供单测直接覆盖。
    ///
    /// # Errors
    /// 空 / 非整数 / 0 / 超上界 ⇒ [`ShardConfigError`]。
    pub fn from_raw(raw: Option<&str>) -> Result<Self, ShardConfigError> {
        let Some(raw) = raw else {
            return Ok(Self::single());
        };
        let text = raw.trim();
        if text.is_empty() {
            return Err(ShardConfigError::Empty);
        }
        let got: u32 = text
            .parse()
            .map_err(|_| ShardConfigError::NotANumber(raw.to_string()))?;
        if got == 0 {
            return Err(ShardConfigError::Zero);
        }
        let count = u8::try_from(got).map_err(|_| ShardConfigError::TooMany {
            got,
            max: MAX_SHARD_COUNT,
        })?;
        Self::from_count(count)
    }

    /// 片数。
    #[must_use]
    pub const fn count(&self) -> u8 {
        self.count
    }

    /// 是否单片（`MIR2_SHARDS` 缺省或 1）。
    #[must_use]
    pub const fn is_single(&self) -> bool {
        self.count == DEFAULT_SHARD_COUNT
    }
}

/// `map_id → ShardId` 的静态归属表。
///
/// `map_id` 与 `crates/envir` 的 `MapRegistry` 下标同源（`mapwire::resolve_map_id`），
/// 所以布局的 `map_count` 必须等于注册表地图数。P2-1 只构造单片布局。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShardLayout {
    count: u8,
    map_to_shard: Vec<ShardId>,
    owned: Vec<Vec<usize>>,
}

impl ShardLayout {
    /// 单片布局：所有地图归 [`HOST_SHARD`]（`MIR2_SHARDS=1` 的唯一形态）。
    #[must_use]
    pub fn single(map_count: usize) -> Self {
        Self {
            count: DEFAULT_SHARD_COUNT,
            map_to_shard: vec![HOST_SHARD; map_count],
            owned: vec![(0..map_count).collect()],
        }
    }

    /// 片数。
    #[must_use]
    pub const fn count(&self) -> u8 {
        self.count
    }

    /// 布局覆盖的地图数（= 注册表地图数）。
    #[must_use]
    pub fn map_count(&self) -> usize {
        self.map_to_shard.len()
    }

    /// `map_id` 的归属片；越界返回 `None`。
    #[must_use]
    pub fn shard_of_map(&self, map_id: usize) -> Option<ShardId> {
        self.map_to_shard.get(map_id).copied()
    }

    /// 某片拥有的 `map_id`（升序）。
    #[must_use]
    pub fn maps_of(&self, shard: ShardId) -> &[usize] {
        self.owned
            .get(shard.index())
            .map_or(&[], std::vec::Vec::as_slice)
    }

    /// `map_id` 是否归 `shard`。
    #[must_use]
    pub fn owns(&self, shard: ShardId, map_id: usize) -> bool {
        self.shard_of_map(map_id) == Some(shard)
    }

    /// 两张图是否属**不同**片（任一侧越界 ⇒ `false`，越界由既有 `NoSuchMap` 路径拒绝）。
    #[must_use]
    pub fn is_cross_shard(&self, from_map: usize, to_map: usize) -> bool {
        match (self.shard_of_map(from_map), self.shard_of_map(to_map)) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        }
    }

    /// 宿主 World 的 `map_sizes`：片 0 拥有的**注册表前缀**。
    ///
    /// 单片时 `owned[0].len() == all.len()` ⇒ 返回与输入逐元素相同的副本（零行为变化）。
    #[must_use]
    pub fn host_map_sizes<T: Copy>(&self, all: &[T]) -> Vec<T> {
        let owned = self.maps_of(HOST_SHARD).len();
        debug_assert_eq!(
            owned.min(all.len()),
            owned,
            "布局地图数 {} 大于尺寸表 {}（调用方必须先按同一注册表建布局）",
            self.map_count(),
            all.len()
        );
        all[..owned.min(all.len())].to_vec()
    }

    /// 覆盖校验输出（启动日志用）：`shards/maps/assigned/per_shard`。
    #[must_use]
    pub fn coverage_report(&self) -> String {
        let assigned: usize = self.owned.iter().map(Vec::len).sum();
        let per: Vec<String> = self
            .owned
            .iter()
            .map(|maps| maps.len().to_string())
            .collect();
        format!(
            "shards={} maps={} assigned={} unassigned={} per_shard=[{}]",
            self.count,
            self.map_count(),
            assigned,
            self.map_count().saturating_sub(assigned),
            per.join(",")
        )
    }
}

/// 跨片拒绝计数（P2-2 起由宿主在拒绝点累加；日志里带累计值）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShardCounters {
    /// 登录/进图目标图不属宿主片 ⇒ 拒绝进图的次数。
    pub cross_shard_entry_refused: u64,
    /// 启动装载时被拒绝注册的跨片地图连接点条数。
    pub cross_shard_route_refused: u64,
}

/// 宿主侧分片运行时句柄：布局 + 拒绝计数。
///
/// P2-1 只定义与单测覆盖；P2-2 起 `sim_loop` 持有它做 map 组守卫，
/// P2-3 起由会话路由器按 `shard_of_map` 分发事件。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShardRuntime {
    layout: ShardLayout,
    counters: ShardCounters,
}

impl ShardRuntime {
    /// 用给定布局构造（计数归零）。
    #[must_use]
    pub fn new(layout: ShardLayout) -> Self {
        Self {
            layout,
            counters: ShardCounters::default(),
        }
    }

    /// 静态归属表。
    #[must_use]
    pub fn layout(&self) -> &ShardLayout {
        &self.layout
    }

    /// 累计拒绝计数。
    #[must_use]
    pub fn counters(&self) -> ShardCounters {
        self.counters
    }

    /// `map_id` 是否归宿主片（片 0）。单片布局恒真。
    #[must_use]
    pub fn host_owns(&self, map_id: usize) -> bool {
        self.layout.owns(HOST_SHARD, map_id)
    }

    /// 一条地图连接点（走路换图）是否允许注册：**源与目标都必须在宿主片内**。
    /// 单片布局恒真。
    #[must_use]
    pub fn route_allowed(&self, src_map: usize, dst_map: usize) -> bool {
        self.host_owns(src_map) && self.host_owns(dst_map)
    }

    /// 记一次「跨片进图被拒绝」，返回累计值。
    pub fn note_cross_shard_entry_refused(&mut self) -> u64 {
        self.counters.cross_shard_entry_refused += 1;
        self.counters.cross_shard_entry_refused
    }

    /// 记一次「跨片连接点被拒绝」（`n` 条由启动装载一次性拒绝），返回累计值。
    pub fn note_cross_shard_routes_refused(&mut self, n: u64) -> u64 {
        self.counters.cross_shard_route_refused += n;
        self.counters.cross_shard_route_refused
    }

    /// 是否单片（P2-1 的全部守卫在此为真时都是空操作）。
    #[must_use]
    pub fn is_single_shard(&self) -> bool {
        self.layout.count() == DEFAULT_SHARD_COUNT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_config_defaults_to_one_when_unset() {
        let cfg = ShardConfig::from_raw(None).expect("未设置 ⇒ 默认 1 片");
        assert_eq!(cfg.count(), 1);
        assert!(cfg.is_single());
        assert_eq!(ShardConfig::single(), cfg);
    }

    #[test]
    fn shard_config_accepts_explicit_one_without_behavior_change() {
        let cfg = ShardConfig::from_raw(Some("1")).expect("显式 1 合法");
        assert_eq!(cfg, ShardConfig::single());
        assert!(cfg.is_single());
        // 空白/制表符与裸数字等价（环境变量里常见）。
        assert_eq!(
            ShardConfig::from_raw(Some(" 1\t")).expect("空白可修剪"),
            ShardConfig::single()
        );
    }

    #[test]
    fn shard_config_parses_multi_shard_counts() {
        for n in [2_u8, 4, 10, 12, MAX_SHARD_COUNT] {
            let cfg = ShardConfig::from_raw(Some(&n.to_string())).expect("合法片数");
            assert_eq!(cfg.count(), n);
            assert!(!cfg.is_single());
        }
    }

    #[test]
    fn shard_config_rejects_invalid_values_fail_closed() {
        assert_eq!(
            ShardConfig::from_raw(Some("")),
            Err(ShardConfigError::Empty)
        );
        assert_eq!(
            ShardConfig::from_raw(Some("   ")),
            Err(ShardConfigError::Empty)
        );
        assert_eq!(
            ShardConfig::from_raw(Some("two")),
            Err(ShardConfigError::NotANumber("two".to_string()))
        );
        assert_eq!(
            ShardConfig::from_raw(Some("1.5")),
            Err(ShardConfigError::NotANumber("1.5".to_string()))
        );
        assert_eq!(
            ShardConfig::from_raw(Some("-1")),
            Err(ShardConfigError::NotANumber("-1".to_string())),
            "负数走「不是无符号整数」而不是 0 支"
        );
        assert_eq!(
            ShardConfig::from_raw(Some("0")),
            Err(ShardConfigError::Zero)
        );
        assert_eq!(
            ShardConfig::from_raw(Some("17")),
            Err(ShardConfigError::TooMany {
                got: 17,
                max: MAX_SHARD_COUNT
            })
        );
        assert_eq!(
            ShardConfig::from_raw(Some("999999999")),
            Err(ShardConfigError::TooMany {
                got: 999_999_999,
                max: MAX_SHARD_COUNT
            })
        );
    }

    #[test]
    fn shard_config_errors_name_the_env_var() {
        let text = ShardConfigError::Empty.to_string();
        assert!(
            text.contains(ENV_SHARDS),
            "错误消息必须点名环境变量：{text}"
        );
        let text = ShardConfigError::TooMany {
            got: 99,
            max: MAX_SHARD_COUNT,
        }
        .to_string();
        assert!(text.contains("99"));
    }

    #[test]
    fn single_layout_maps_every_map_to_host_shard() {
        let layout = ShardLayout::single(5);
        assert_eq!(layout.count(), 1);
        assert_eq!(layout.map_count(), 5);
        assert_eq!(layout.maps_of(HOST_SHARD), &[0, 1, 2, 3, 4]);
        for map_id in 0..5 {
            assert!(layout.owns(HOST_SHARD, map_id));
            assert_eq!(layout.shard_of_map(map_id), Some(HOST_SHARD));
        }
        assert_eq!(layout.shard_of_map(5), None, "越界无归属");
        assert!(!layout.is_cross_shard(0, 4), "单片内永不跨片");
        assert!(
            !layout.is_cross_shard(9, 0),
            "越界不算跨片（交给 NoSuchMap）"
        );
    }

    #[test]
    fn single_layout_host_map_sizes_is_identity() {
        let sizes = vec![(64, 64), (100, 80), (20, 20)];
        assert_eq!(
            ShardLayout::single(sizes.len()).host_map_sizes(&sizes),
            sizes
        );
        // 空注册表（占位/测试形态）也必须是恒等。
        let empty: Vec<(i32, i32)> = Vec::new();
        assert_eq!(ShardLayout::single(0).host_map_sizes(&empty), empty);
    }

    #[test]
    fn coverage_report_denominates_all_maps_for_single_layout() {
        let report = ShardLayout::single(207).coverage_report();
        assert!(report.contains("shards=1"), "{report}");
        assert!(report.contains("maps=207"), "{report}");
        assert!(report.contains("assigned=207"), "{report}");
        assert!(report.contains("unassigned=0"), "{report}");
        assert!(report.contains("per_shard=[207]"), "{report}");
    }

    #[test]
    fn single_shard_runtime_guards_are_no_ops() {
        let mut runtime = ShardRuntime::new(ShardLayout::single(3));
        assert!(runtime.is_single_shard());
        for map_id in 0..3 {
            assert!(runtime.host_owns(map_id), "单片必须全部拥有");
        }
        for (src, dst) in [(0, 1), (1, 2), (2, 0)] {
            assert!(runtime.route_allowed(src, dst), "单片连接点全放行");
            assert!(!runtime.layout().is_cross_shard(src, dst));
        }
        // 计数初始为 0（单片路径不该产生任何拒绝）。
        assert_eq!(runtime.counters(), ShardCounters::default());
        // 计数 API 本身可用（P2-2 接线前的骨架自证）。
        assert_eq!(runtime.note_cross_shard_entry_refused(), 1);
        assert_eq!(runtime.note_cross_shard_routes_refused(2), 2);
        assert_eq!(
            runtime.counters(),
            ShardCounters {
                cross_shard_entry_refused: 1,
                cross_shard_route_refused: 2,
            }
        );
    }
}
