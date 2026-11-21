//! 世界分片运行时骨架（P2-1 起；卡 `openmir2-shard-runtime-skeleton` / `openmir2-shard-map-partition`）。
//!
//! 设计依据：`docs/10k-shard-design-20261116.md` §2.1（同进程多 World）与 §4.2；
//! 归属模型见 `docs/shard-mapid-ownership-20261116.md`。本模块放**分片配置与宿主侧运行时**：
//!
//! - [`ShardId`]：分片编号（0 起；[`HOST_SHARD`] = 宿主当前服务的片）；
//! - [`ShardConfig`]：`MIR2_SHARDS`（**默认 1**；非法值 fail-closed）；
//! - [`ShardLayout`]：`map_id → ShardId` 的**静态**归属表（任意分组；P2-3a 起不再要求前缀）；
//!   [`ShardLayout::local_map_sizes`] 返回**全长度**尺寸表（拥有图真实尺寸 + 其它片 1×1 stub），
//!   [`ShardLayout::local_ownership`] 给出 `world.set_map_ownership` 的位图；片 0 另有
//!   `host_*` 兼容别名（P2-1/P2-2 调用点语义不变）；
//! - [`ShardRuntime`]：**每片**运行期句柄（本地片号 + 布局 + 跨片进图/连接点/MapChange 拒绝计数）；
//!   [`ShardRuntime::new`] 造片 0（P2-1/P2-2 宿主语义），[`ShardRuntime::for_shard`] 造任意片（P2-3b）。
//!
//! **`MIR2_SHARDS=1` 零行为变化**：单片布局把每张图都归片 0，
//! [`ShardRuntime::host_owns`] 与 [`ShardRuntime::route_allowed`] 恒真、
//! [`ShardLayout::host_map_sizes`] 原样返回输入、`host_ownership` 全 `true` ⇒ 所有守卫都是空操作，
//! 且本模块在片数为 1 时**不打印任何日志**（stdout 与未设该环境变量时逐字节相同）。
//!
//! 本模块**不做**的事（登记给后续卡）：多 World 实例/线程归属（P2-3b）、
//! 会话路由与协调器（P2-3b/P2-4）、跨片 handoff（P2-5）、确定性守卫（P2-6）。

use std::collections::HashMap;
use std::fmt;

/// 分片数的环境变量名。
pub const ENV_SHARDS: &str = "MIR2_SHARDS";

/// 地图组配置的环境变量名（P2-2）：`<片号>:<地图名>[,<地图名>...]`，组间 `;` 分隔，
/// `*` 表示「注册表里尚未分配的其余地图（按 `map_id` 升序）」。
///
/// 例：`MIR2_SHARDS=2 MIR2_SHARD_MAP_GROUPS="0:0,0129;1:*"`。
pub const ENV_MAP_GROUPS: &str = "MIR2_SHARD_MAP_GROUPS";

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

    /// **P2-2 地图组划分**：把 `map_names`（注册表逻辑名，下标 = `map_id`）分成 `count` 个
    /// **互不相交且覆盖 100%** 的组。
    ///
    /// `groups_raw` 的语法见 [`ENV_MAP_GROUPS`]；片数为 1 时允许 `None`（= 全归片 0）。
    ///
    /// **P2-3a**：不再要求片 0 是注册表前缀——`map_id` 的全局下标由宿主 World 的
    /// `owned_maps` + 1×1 stub 承载；任意分组都可表达（入口守卫在 world-bevy）。
    ///
    /// # Errors
    /// 语法错、未知地图、重复分配、未覆盖、空片、片号越界 ⇒ [`ShardPlanError`]。
    pub fn partition(
        count: u8,
        map_names: &[String],
        groups_raw: Option<&str>,
    ) -> Result<Self, ShardPlanError> {
        let count = ShardConfig::from_count(count)
            .map_err(ShardPlanError::BadCount)?
            .count();
        let Some(raw) = groups_raw else {
            if count == DEFAULT_SHARD_COUNT {
                return Ok(Self::single(map_names.len()));
            }
            return Err(ShardPlanError::GroupsRequired { count });
        };
        let groups = parse_map_groups(raw)?;
        // 片号必须在 0..count 且不重复（同一片写两次会让「每图唯一归属」失去可读的错因）。
        let mut seen = vec![false; usize::from(count)];
        for group in &groups {
            let idx = group.shard.index();
            if idx >= usize::from(count) {
                return Err(ShardPlanError::ShardOutOfRange {
                    shard: group.shard.get(),
                    count,
                });
            }
            if seen[idx] {
                return Err(ShardPlanError::DuplicateShard(group.shard.get()));
            }
            seen[idx] = true;
        }
        // 地图名按注册表的「ASCII 大小写不敏感」口径匹配（`MapRegistry::id_of` 的 normalize）。
        let mut index: HashMap<String, usize> = HashMap::new();
        for (map_id, name) in map_names.iter().enumerate() {
            index.insert(normalize_map_name(name), map_id);
        }
        let mut assign: Vec<Option<ShardId>> = vec![None; map_names.len()];
        // 先落显式名单（组间顺序无关；重复分配 = 配置错）。
        for group in &groups {
            let GroupMaps::Names(names) = &group.maps else {
                continue;
            };
            for name in names {
                let Some(&map_id) = index.get(&normalize_map_name(name)) else {
                    return Err(ShardPlanError::UnknownMap(name.clone()));
                };
                if assign[map_id].is_some() {
                    return Err(ShardPlanError::MapAlreadyAssigned(name.clone()));
                }
                assign[map_id] = Some(group.shard);
            }
        }
        // 再落 `*`：注册表里尚未分配的其余地图（升序 = `map_id` 序）。
        for group in &groups {
            if group.maps != GroupMaps::All {
                continue;
            }
            for slot in &mut assign {
                if slot.is_none() {
                    *slot = Some(group.shard);
                }
            }
        }
        // 覆盖 100%：没有 `*` 就必须把每张图都显式写出来。
        let missing: Vec<String> = assign
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.is_none())
            .map(|(map_id, _)| map_names[map_id].clone())
            .collect();
        if !missing.is_empty() {
            return Err(ShardPlanError::Unassigned(missing));
        }
        // 每片至少一张图：空片等于一个永不接客的 World，必须在启动时响。
        for idx in 0..usize::from(count) {
            if !assign
                .iter()
                .any(|slot| slot.is_some_and(|shard| shard.index() == idx))
            {
                return Err(ShardPlanError::EmptyShard(idx as u8));
            }
        }
        let table: Vec<ShardId> = assign
            .into_iter()
            .map(|slot| slot.expect("覆盖 100% 已校验"))
            .collect();
        let layout = Self::from_table(count, table).map_err(ShardPlanError::Layout)?;
        // **P2-3a**：不再要求片 0 是注册表前缀——全局下标由宿主 World 的
        // `owned_maps` + 1×1 stub 承载（见 `host_map_sizes` / `host_ownership`）。
        Ok(layout)
    }

    /// 由 `map_to_shard` 表构造（P2-2 的地图组划分用；同一时刻每图恰属一片）。
    ///
    /// # Errors
    /// 表为空 / 长度与 `owned` 不自洽 / 出现 `>= count` 的片号 ⇒ 错误。
    pub(crate) fn from_table(
        count: u8,
        map_to_shard: Vec<ShardId>,
    ) -> Result<Self, ShardLayoutError> {
        if count == 0 || count > MAX_SHARD_COUNT {
            return Err(ShardLayoutError::BadShardCount(count));
        }
        let mut owned = vec![Vec::new(); usize::from(count)];
        for (map_id, shard) in map_to_shard.iter().enumerate() {
            let idx = shard.index();
            if idx >= owned.len() {
                return Err(ShardLayoutError::ShardOutOfRange {
                    map_id,
                    shard: shard.get(),
                    count,
                });
            }
            owned[idx].push(map_id);
        }
        Ok(Self {
            count,
            map_to_shard,
            owned,
        })
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

    /// **某片** World 的 `map_sizes`（**全局下标保序**，P2-3a）：长度 = 注册表地图数。
    ///
    /// - `shard` 拥有的图 ⇒ 取 `all[map_id]` 的真实尺寸；
    /// - 其它图 ⇒ **1×1 stub**（`MapGrid::new` 要求尺寸 > 0），承载全局下标；
    ///   stub 上的实体由 `World::owned_maps` + 换图/落格入口 fail-closed 禁止。
    ///
    /// 单片时片 0 拥有全部图 ⇒ 返回与输入逐元素相同的副本（零行为变化）。
    /// 越界片号返回全 stub（[`ShardRuntime::for_shard`] 已先行 panic；本方法保持纯函数语义）。
    #[must_use]
    pub fn local_map_sizes(&self, shard: ShardId, all: &[(i32, i32)]) -> Vec<(i32, i32)> {
        const STUB: (i32, i32) = (1, 1);
        debug_assert_eq!(
            self.map_count(),
            all.len(),
            "布局地图数 {} 必须等于尺寸表 {}（调用方必须先按同一注册表建布局）",
            self.map_count(),
            all.len()
        );
        let mut sizes = vec![STUB; self.map_count()];
        for &map_id in self.maps_of(shard) {
            if let Some(&size) = all.get(map_id) {
                sizes[map_id] = size;
            }
        }
        sizes
    }

    /// 宿主片（片 0）的 `map_sizes`——P2-1/P2-2 的兼容别名（= `local_map_sizes(HOST_SHARD, …)`）。
    #[must_use]
    pub fn host_map_sizes(&self, all: &[(i32, i32)]) -> Vec<(i32, i32)> {
        self.local_map_sizes(HOST_SHARD, all)
    }

    /// **某片** World 的 `owned_maps` 位图（长度 = 注册表地图数；`true` = 该片拥有）。
    ///
    /// 每个 World 在 `World::with_maps` 之后调用 `world.set_map_ownership(&layout.local_ownership(id))`。
    /// 越界片号返回全 `false`（[`ShardRuntime::for_shard`] 已先行 panic）。
    #[must_use]
    pub fn local_ownership(&self, shard: ShardId) -> Vec<bool> {
        let mut owned = vec![false; self.map_count()];
        for &map_id in self.maps_of(shard) {
            if let Some(slot) = owned.get_mut(map_id) {
                *slot = true;
            }
        }
        owned
    }

    /// 宿主片（片 0）的 `owned_maps` 位图——P2-1/P2-2 的兼容别名。
    #[must_use]
    pub fn host_ownership(&self) -> Vec<bool> {
        self.local_ownership(HOST_SHARD)
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

/// **启动期布局决策（唯一入口）**：把 `MIR2_SHARDS` / `MIR2_SHARD_MAP_GROUPS` 变成静态布局。
///
/// - 片数 1 且未配分组 ⇒ [`ShardLayout::single`]：**不读注册表、不分配名字表**（单片路径零额外开销）；
/// - 其余情形 ⇒ [`ShardLayout::partition`]（无注册表 ⇒ [`ShardPlanError::RegistryRequired`]）。
///
/// `map_names` 只在需要分组时使用（`None` = 无 Envir 注册表的占位形态）；
/// `map_count` = 尺寸表长度（单片快路径与占位形态都靠它）。
///
/// # Errors
/// 见 [`ShardLayout::partition`] 与 [`ShardPlanError::RegistryRequired`]。
pub fn plan_layout(
    cfg: &ShardConfig,
    map_names: Option<&[String]>,
    map_count: usize,
    groups_raw: Option<&str>,
) -> Result<ShardLayout, ShardPlanError> {
    if cfg.is_single() && groups_raw.is_none() {
        return Ok(ShardLayout::single(map_count));
    }
    let Some(names) = map_names else {
        return Err(ShardPlanError::RegistryRequired { count: cfg.count() });
    };
    ShardLayout::partition(cfg.count(), names, groups_raw)
}

/// 片数 >1 的启动日志行（`(级别, 文本)`）；**片数为 1 时返回空**。
///
/// 为什么放在这里而不是 `lib.rs`：`MIR2_SHARDS=1` 的 stdout 必须与 master 逐字节相同，
/// 「1 片不打印分片日志」这条判据要有可单测的形状——函数开头就是早退，
/// 删掉或放宽它都会让 `tests::startup_log_lines_are_empty_for_single_shard` 变红。
#[must_use]
pub fn startup_log_lines(
    cfg: &ShardConfig,
    layout: &ShardLayout,
    groups_raw: Option<&str>,
) -> Vec<(&'static str, String)> {
    if cfg.is_single() {
        return Vec::new();
    }
    let mut lines = vec![
        (
            "INF",
            format!(
                "世界分片：{}={} {}={}（宿主只服务片 0；片 1..{} 的路由在 P2-3，跨片换图在 P2-2 一律拒绝）",
                ENV_SHARDS,
                cfg.count(),
                ENV_MAP_GROUPS,
                groups_raw.unwrap_or("<unset>"),
                cfg.count().saturating_sub(1)
            ),
        ),
        (
            "INF",
            format!(
                "地图组覆盖校验：{}（片 0 = 注册表前缀 0..{}）",
                layout.coverage_report(),
                layout.maps_of(HOST_SHARD).len()
            ),
        ),
    ];
    for shard_id in 1..cfg.count() {
        let id = ShardId::new(shard_id);
        lines.push((
            "WRN",
            format!(
                "shard {} 已划分 {} 张地图，但会话路由未接线（P2-3）⇒ 该片当前 0 会话/0 实体；跨片进入按 fail-closed 拒绝",
                id,
                layout.maps_of(id).len()
            ),
        ));
    }
    lines
}

/// 一个地图组的成员表（`MIR2_SHARD_MAP_GROUPS` 的解析结果）。
#[derive(Clone, Debug, Eq, PartialEq)]
enum GroupMaps {
    /// 注册表里**尚未分配**的其余地图，按 `map_id` 升序。
    All,
    /// 显式地图名（匹配口径与注册表一致：ASCII 大小写不敏感、两端空白忽略）。
    Names(Vec<String>),
}

/// 配置里的一组 `片号:地图…`。
#[derive(Clone, Debug, Eq, PartialEq)]
struct MapGroup {
    shard: ShardId,
    maps: GroupMaps,
}

/// 解析 `MIR2_SHARD_MAP_GROUPS`：`<片号>:<名>[,<名>…]`，组间 `;` 分隔；`*` = 其余地图。
///
/// 空组（如结尾 `;`）直接跳过；**不允许多个 `*`**（否则「其余」没有确定归属）。
fn parse_map_groups(raw: &str) -> Result<Vec<MapGroup>, ShardPlanError> {
    let mut groups = Vec::new();
    let mut wildcards = 0usize;
    for token in raw.split(';') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((id_raw, maps_raw)) = token.split_once(':') else {
            return Err(ShardPlanError::BadGroupSyntax(token.to_string()));
        };
        let id_text = id_raw.trim();
        let shard = id_text
            .parse::<u8>()
            .map_err(|_| ShardPlanError::BadShardId(id_text.to_string()))?;
        let maps_text = maps_raw.trim();
        let maps = if maps_text == "*" {
            wildcards += 1;
            if wildcards > 1 {
                return Err(ShardPlanError::WildcardTwice);
            }
            GroupMaps::All
        } else {
            let names: Vec<String> = maps_text
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect();
            if names.is_empty() {
                return Err(ShardPlanError::EmptyGroupMaps(ShardId::new(shard)));
            }
            GroupMaps::Names(names)
        };
        groups.push(MapGroup {
            shard: ShardId::new(shard),
            maps,
        });
    }
    if groups.is_empty() {
        return Err(ShardPlanError::GroupsEmpty);
    }
    Ok(groups)
}

/// 地图名的匹配口径：与 `MapRegistry::id_of` 相同（ASCII 大小写不敏感，忽略两端空白）。
fn normalize_map_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// 地图组划分错误（fail-closed：任何一条都必须让启动非零退出，不许静默降级）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShardPlanError {
    /// 片数本身非法（转 [`ShardConfigError`]）。
    BadCount(ShardConfigError),
    /// 片数 >1 但没有配置地图组（不猜分组）。
    GroupsRequired {
        /// 请求的片数。
        count: u8,
    },
    /// `MIR2_SHARD_MAP_GROUPS` 是空串。
    GroupsEmpty,
    /// 组缺少 `片号:` 前缀（或整段无法切分）。
    BadGroupSyntax(String),
    /// 组前缀不是 `u8` 片号。
    BadShardId(String),
    /// 出现两个 `*`。
    WildcardTwice,
    /// `片号:` 后面没有地图名（也不是 `*`）。
    EmptyGroupMaps(ShardId),
    /// 同一片被写了两次。
    DuplicateShard(u8),
    /// 片号 `>= MIR2_SHARDS`。
    ShardOutOfRange {
        /// 配置里写的片号。
        shard: u8,
        /// 配置的片数。
        count: u8,
    },
    /// 地图名不在注册表里。
    UnknownMap(String),
    /// 同一张图被分到多个组。
    MapAlreadyAssigned(String),
    /// 没有 `*` 且显式名单没有覆盖全部地图（列出未覆盖的地图名）。
    Unassigned(Vec<String>),
    /// 某片一张图都没有。
    EmptyShard(u8),
    /// 片数 >1 但没有真地图注册表（占位形态无法按名字分组）。
    RegistryRequired {
        /// 请求的片数。
        count: u8,
    },
    /// 布局表最后一道自洽性检查失败。
    Layout(ShardLayoutError),
}

impl fmt::Display for ShardPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadCount(e) => write!(f, "{e}"),
            Self::GroupsRequired { count } => write!(
                f,
                "{ENV_SHARDS}={count} 需要同时设置 {ENV_MAP_GROUPS}（例：\"0:地图A,地图B;1:*\"）；不配置时不猜分组（fail-closed）"
            ),
            Self::GroupsEmpty => write!(
                f,
                "{ENV_MAP_GROUPS} 为空串（fail-closed；单片可用 \"0:*\"，多片必须写全分组）"
            ),
            Self::BadGroupSyntax(token) => write!(
                f,
                "{ENV_MAP_GROUPS} 的组 \"{token}\" 缺少 `片号:` 前缀（例：\"0:地图A,地图B\"，组间 `;` 分隔）"
            ),
            Self::BadShardId(text) => write!(
                f,
                "{ENV_MAP_GROUPS} 的片号 \"{text}\" 不是 0..=255 的整数"
            ),
            Self::WildcardTwice => write!(
                f,
                "{ENV_MAP_GROUPS} 出现多个 `*`（`*` = 其余未分配地图，只能出现一次，否则归属不确定）"
            ),
            Self::EmptyGroupMaps(shard) => write!(
                f,
                "{ENV_MAP_GROUPS} 的 shard {shard} 没有地图名（写 `*` 表示其余地图）"
            ),
            Self::DuplicateShard(shard) => write!(
                f,
                "{ENV_MAP_GROUPS} 把 shard {shard} 写了多组（每片只能有一组，否则「每图唯一归属」不再可读）"
            ),
            Self::ShardOutOfRange { shard, count } => write!(
                f,
                "{ENV_MAP_GROUPS} 的 shard {shard} 越界（{ENV_SHARDS}={count} ⇒ 合法片号 0..{}）",
                u16::from(*count).saturating_sub(1)
            ),
            Self::UnknownMap(name) => write!(
                f,
                "{ENV_MAP_GROUPS} 的地图 \"{name}\" 不在 Envir/MapInfo.txt 注册表里（检查拼写；匹配为 ASCII 大小写不敏感）"
            ),
            Self::MapAlreadyAssigned(name) => write!(
                f,
                "{ENV_MAP_GROUPS} 把地图 \"{name}\" 分到了多个组（每张图只能属一片）"
            ),
            Self::Unassigned(names) => write!(
                f,
                "{ENV_MAP_GROUPS} 未覆盖 {} 张地图（{}）；没有 `*` 时必须显式列全，否则分组不完整",
                names.len(),
                preview_names(names)
            ),
            Self::EmptyShard(shard) => write!(
                f,
                "{ENV_MAP_GROUPS} 让 shard {shard} 一张图都没有（空片 = 永不接客的 World，拒绝启动）"
            ),
            Self::RegistryRequired { count } => write!(
                f,
                "{ENV_SHARDS}={count} 需要 Envir 真地图注册表（MapInfo.txt + *.map）：当前是 WorldMaps 占位形态，无法按地图名分组 ⇒ fail-closed"
            ),
            Self::Layout(e) => write!(f, "分片布局构造失败：{e}"),
        }
    }
}

impl std::error::Error for ShardPlanError {}

/// 日志里最多列 8 个地图名，其余用 `…` 省略（避免一行刷屏）。
fn preview_names(names: &[String]) -> String {
    const LIMIT: usize = 8;
    let mut out = names
        .iter()
        .take(LIMIT)
        .cloned()
        .collect::<Vec<String>>()
        .join(",");
    if names.len() > LIMIT {
        out.push_str(",…");
    }
    out
}

/// [`ShardLayout::from_table`] 的构造错误（P2-2 的划分引擎会先做完整校验，这里是最后一道自洽性检查）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShardLayoutError {
    /// 片数不在 `1..=MAX_SHARD_COUNT`。
    BadShardCount(u8),
    /// 布局表里出现了 `>= count` 的片号。
    ShardOutOfRange { map_id: usize, shard: u8, count: u8 },
}

impl fmt::Display for ShardLayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadShardCount(count) => {
                write!(f, "分片数 {count} 非法（取值 1..={MAX_SHARD_COUNT}）")
            }
            Self::ShardOutOfRange {
                map_id,
                shard,
                count,
            } => write!(f, "map_id={map_id} 归属 shard {shard} 越界（片数 {count}）"),
        }
    }
}

impl std::error::Error for ShardLayoutError {}

/// 一条地图连接点（走路换图）在**宿主片视角**下的分类。
///
/// `cross_shard_route_refused` 与 `foreign_shard_route_refused` 分开累计：
/// 前者是「从宿主片走向其它片」（P2-5 handoff 要处理的），后者是「整条连接点都不属宿主片」。
/// 两者都拒绝注册，但日志里必须能分别核对（follow-up：旧日志把 foreign 条数写成累计之外）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteClass {
    /// 源与目标都在宿主片 ⇒ 正常注册。
    Keep,
    /// 源在宿主片、目标在其它片 ⇒ 跨片换图（拒绝，计入 `cross_shard_route_refused`）。
    CrossShard,
    /// 源在其它片 ⇒ 整条连接点不属宿主片（拒绝，计入 `foreign_shard_route_refused`）。
    ForeignShard,
}

/// 跨片拒绝计数（P2-2 起由宿主在拒绝点累加；日志里带累计值）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShardCounters {
    /// 登录/进图目标图不属宿主片 ⇒ 拒绝进图的次数。
    pub cross_shard_entry_refused: u64,
    /// 源在宿主片、目标在其它片的地图连接点条数（P2-5 handoff 的对象）。
    pub cross_shard_route_refused: u64,
    /// 源在其它片、整条不属宿主片的地图连接点条数（与上一条**分开**累计）。
    pub foreign_shard_route_refused: u64,
    /// **P2-3a**：宿主侧看到「目标图不归本片」的 `MapChange` 条数（防御性；正常恒 0）。
    pub cross_shard_map_change_refused: u64,
}

/// 每片运行期句柄：**本地片号** + 布局 + 拒绝计数。
///
/// P2-1 只定义与单测覆盖；P2-2 起 `sim_loop` 持有它做 map 组守卫；
/// **P2-3b** 起每片一个实例（[`Self::for_shard`]），[`Self::new`] 是片 0 的兼容入口。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShardRuntime {
    layout: ShardLayout,
    /// **P2-3b**：本实例服务的片。P2-1/P2-2 只有一个实例（片 0）。
    local: ShardId,
    counters: ShardCounters,
}

impl ShardRuntime {
    /// 用给定布局构造**片 0** 的运行时（P2-1/P2-2 宿主语义；计数归零）。
    #[must_use]
    pub fn new(layout: ShardLayout) -> Self {
        Self::for_shard(layout, HOST_SHARD)
    }

    /// 为指定片构造运行时（P2-3b：每片一个实例，各驱动自己的 World）。
    ///
    /// `shard` 必须落在 `0..layout.count()`：两处由同一份配置构造，越界是**编程错误**
    /// （不是用户配置错误）⇒ 立即 panic，免得静默起一个永远没有地图的 World。
    #[must_use]
    pub fn for_shard(layout: ShardLayout, shard: ShardId) -> Self {
        assert!(
            shard.index() < usize::from(layout.count()),
            "本地片 {shard} 越界：布局只有 {} 片",
            layout.count()
        );
        Self {
            layout,
            local: shard,
            counters: ShardCounters::default(),
        }
    }

    /// 静态归属表。
    #[must_use]
    pub fn layout(&self) -> &ShardLayout {
        &self.layout
    }

    /// 本地片号（该 runtime 服务的片）。
    #[must_use]
    pub const fn local_shard(&self) -> ShardId {
        self.local
    }

    /// 累计拒绝计数。
    #[must_use]
    pub fn counters(&self) -> ShardCounters {
        self.counters
    }

    /// `map_id` 是否归**本地片**。单片布局恒真。
    ///
    /// **P2-3b**：这是主谓词（每个 World 只关心自己片）；[`Self::host_owns`] 是片 0 之上的
    /// 兼容别名。P2-3b 的每片 runtime 必须经本方法判归属——否则非 0 片会把自己的图当外来图。
    #[must_use]
    pub fn local_owns(&self, map_id: usize) -> bool {
        self.layout.owns(self.local, map_id)
    }

    /// `map_id` 是否归宿主片（片 0）。
    ///
    /// 兼容别名：P2-1/P2-2 的调用点都建在「本地片 = 片 0」的 runtime 上（[`Self::new`]）
    /// ⇒ 与 [`Self::local_owns`] 等价。非 0 片的 runtime 请用 `local_owns`。
    #[must_use]
    pub fn host_owns(&self, map_id: usize) -> bool {
        self.local_owns(map_id)
    }

    /// 一条地图连接点（走路换图）是否允许注册：**源与目标都必须在本地片内**。
    /// 单片布局恒真。
    #[must_use]
    pub fn local_route_allowed(&self, src_map: usize, dst_map: usize) -> bool {
        self.local_owns(src_map) && self.local_owns(dst_map)
    }

    /// 宿主片（片 0）的连接点判定——[`Self::new`] 上的兼容别名。
    #[must_use]
    pub fn route_allowed(&self, src_map: usize, dst_map: usize) -> bool {
        self.local_route_allowed(src_map, dst_map)
    }

    /// 地图连接点分类（**唯一分类口径**；`sim_loop` 的注册过滤与日志都走这里）。
    ///
    /// 顺序是契约：先看源（整条是否属本地片），再看目标（是否跨片）。
    #[must_use]
    pub fn classify_route(&self, src_map: usize, dst_map: usize) -> RouteClass {
        if !self.local_owns(src_map) {
            RouteClass::ForeignShard
        } else if !self.local_owns(dst_map) {
            RouteClass::CrossShard
        } else {
            RouteClass::Keep
        }
    }

    /// 记一次「跨片进图被拒绝」，返回累计值。
    pub fn note_cross_shard_entry_refused(&mut self) -> u64 {
        self.counters.cross_shard_entry_refused += 1;
        self.counters.cross_shard_entry_refused
    }

    /// **跨片登图守卫**：目标图归宿主片 ⇒ `None`（放行）；否则累计计数并返回要打印的 WRN 行。
    ///
    /// 把「判定 + 计数 + 日志文本」放在一个纯函数里，才有可变异/可断言的牙：
    /// `sim_loop` 只负责调用并 `glog!`。删掉 `sim_loop` 的 guard 臂时，接线用例
    /// （`tests/shard_partition.rs`）必须红。
    #[must_use = "返回的日志行必须被打印，否则跨片拒绝不可见"]
    pub fn guard_cross_shard_enter(
        &mut self,
        chr_name: &str,
        map_name: &str,
        map_id: usize,
    ) -> Option<String> {
        if self.local_owns(map_id) {
            return None;
        }
        let count = self.note_cross_shard_entry_refused();
        Some(format!(
            "拒绝进图（跨片 fail-closed，P2-2）：ChrName:{chr_name} CurMap:{map_name} map_id={map_id} 属其它片（handoff 在 P2-5）；累计跨片进图拒绝 {count}"
        ))
    }

    /// 记一次「跨片连接点被拒绝」（`n` 条由启动装载一次性拒绝），返回累计值。
    pub fn note_cross_shard_routes_refused(&mut self, n: u64) -> u64 {
        self.counters.cross_shard_route_refused += n;
        self.counters.cross_shard_route_refused
    }

    /// 记一次「源在其它片、整条连接点不属宿主片」（与跨片计数**分开**累计），返回累计值。
    pub fn note_foreign_shard_routes_skipped(&mut self, n: u64) -> u64 {
        self.counters.foreign_shard_route_refused += n;
        self.counters.foreign_shard_route_refused
    }

    /// **P2-3a**：宿主侧 `MapChange` 可路由判定。
    ///
    /// 目标图归本片 ⇒ `true`；否则累计 `cross_shard_map_change_refused` 并返回 `false`
    /// （handoff 归 P2-5）。正常恒 `true`：world 已在所有换图入口 fail-closed，这里是宿主侧防御。
    pub fn accept_map_change(&mut self, map_id: usize) -> bool {
        if self.local_owns(map_id) {
            return true;
        }
        self.counters.cross_shard_map_change_refused += 1;
        false
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
                foreign_shard_route_refused: 0,
                cross_shard_map_change_refused: 0,
            }
        );
    }

    /// follow-up 牙：foreign-only 连接点必须进 `foreign_shard_route_refused`，
    /// **不能**被算进 `cross_shard_route_refused`（旧实现的累计漏掉 foreign）。
    ///
    /// 红检：把 `classify_route` 的 `ForeignShard` 支改成 `CrossShard`，或让
    /// `sim_loop` 只累计 cross ⇒ 本用例的 `cross==0 / foreign==N` 断言红。
    #[test]
    fn route_classification_and_counters_split_cross_from_foreign() {
        let maps = map_names(&["A", "B", "C"]);
        let layout = ShardLayout::partition(2, &maps, Some("0:A;1:*")).expect("两片划分");
        let mut runtime = ShardRuntime::new(layout);
        // 宿主片 = {0}；片 1 = {1,2}。
        assert_eq!(runtime.classify_route(0, 0), RouteClass::Keep);
        assert_eq!(runtime.classify_route(0, 1), RouteClass::CrossShard);
        assert_eq!(
            runtime.classify_route(1, 2),
            RouteClass::ForeignShard,
            "源不在宿主片 ⇒ 整条不属宿主片"
        );
        assert_eq!(runtime.classify_route(9, 0), RouteClass::ForeignShard);
        assert_eq!(runtime.classify_route(0, 9), RouteClass::CrossShard);

        // foreign-only 场景：两条连接点都源在片 1（不跨片，但整条不属宿主）。
        let mut cross = 0u64;
        let mut foreign = 0u64;
        for (src, dst) in [(1usize, 2usize), (2, 1)] {
            match runtime.classify_route(src, dst) {
                RouteClass::Keep => {}
                RouteClass::CrossShard => cross += 1,
                RouteClass::ForeignShard => foreign += 1,
            }
        }
        assert_eq!((cross, foreign), (0, 2), "foreign-only 不得混进 cross");
        assert_eq!(runtime.note_cross_shard_routes_refused(cross), 0);
        assert_eq!(runtime.note_foreign_shard_routes_skipped(foreign), 2);
        assert_eq!(runtime.counters().cross_shard_route_refused, 0);
        assert_eq!(runtime.counters().foreign_shard_route_refused, 2);

        // mixed：cross 与 foreign 各 1，累计互不污染。
        let mut runtime =
            ShardRuntime::new(ShardLayout::partition(2, &maps, Some("0:A;1:*")).unwrap());
        assert_eq!(runtime.classify_route(0, 2), RouteClass::CrossShard);
        assert_eq!(runtime.classify_route(2, 1), RouteClass::ForeignShard);
        runtime.note_cross_shard_routes_refused(1);
        runtime.note_foreign_shard_routes_skipped(1);
        assert_eq!(runtime.counters().cross_shard_route_refused, 1);
        assert_eq!(runtime.counters().foreign_shard_route_refused, 1);
    }

    /// follow-up 牙：跨片登图 guard 必须同时给出**日志行**与**计数**；本片进图零副作用。
    ///
    /// 红检：删掉 `guard_cross_shard_enter` 的 `host_owns` 早退 ⇒ 本片进图也返回 Some（红）；
    /// 删掉 `note_cross_shard_entry_refused()` ⇒ 累计断言红；把返回值改成纯 bool ⇒ 日志断言红。
    #[test]
    fn cross_shard_enter_guard_returns_log_line_and_counts() {
        let maps = map_names(&["A", "B"]);
        let layout = ShardLayout::partition(2, &maps, Some("0:A;1:*")).expect("两片划分");
        let mut runtime = ShardRuntime::new(layout);
        assert_eq!(
            runtime.guard_cross_shard_enter("hero", "A", 0),
            None,
            "宿主片进图不拒绝、不计数"
        );
        assert_eq!(runtime.counters().cross_shard_entry_refused, 0);
        let line = runtime
            .guard_cross_shard_enter("hero", "B", 1)
            .expect("其它片必须拒绝");
        assert!(line.contains("跨片 fail-closed"), "{line}");
        assert!(line.contains("map_id=1"), "{line}");
        assert!(line.contains("累计跨片进图拒绝 1"), "{line}");
        assert_eq!(runtime.counters().cross_shard_entry_refused, 1);
        let line2 = runtime
            .guard_cross_shard_enter("hero2", "B", 1)
            .expect("重复拒绝也要计数");
        assert!(line2.contains("累计跨片进图拒绝 2"), "{line2}");
        assert_eq!(runtime.counters().cross_shard_entry_refused, 2);
    }

    /// **P2-3a 牙**：宿主侧 `MapChange` 路由判定——本片放行、其它片拒绝并计数。
    ///
    /// 红检：把 `accept_map_change` 改成恒 `true` ⇒ 第二条断言红；删掉计数 ⇒ 累计断言红。
    #[test]
    fn accept_map_change_rejects_and_counts_foreign_targets() {
        let maps = map_names(&["A", "B"]);
        let layout = ShardLayout::partition(2, &maps, Some("0:A;1:*")).expect("两片划分");
        let mut runtime = ShardRuntime::new(layout);
        assert!(runtime.accept_map_change(0), "本片地图放行");
        assert_eq!(runtime.counters().cross_shard_map_change_refused, 0);
        assert!(!runtime.accept_map_change(1), "其它片必须拒绝");
        assert!(!runtime.accept_map_change(1));
        assert_eq!(runtime.counters().cross_shard_map_change_refused, 2);
    }

    #[test]
    fn layout_table_rejects_out_of_range_shard() {
        let err = ShardLayout::from_table(2, vec![ShardId::new(0), ShardId::new(2)])
            .expect_err("片号 >= count 必须拒绝");
        assert_eq!(
            err,
            ShardLayoutError::ShardOutOfRange {
                map_id: 1,
                shard: 2,
                count: 2
            }
        );
        let err = ShardLayout::from_table(0, Vec::new()).expect_err("0 片必须拒绝");
        assert_eq!(err, ShardLayoutError::BadShardCount(0));
    }

    // ---------- P2-2：地图组划分 ----------

    fn map_names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn single_shard_partition_is_identical_to_single_layout() {
        let maps = map_names(&["0", "0129", "0100"]);
        assert_eq!(
            ShardLayout::partition(1, &maps, None).expect("单片无需分组"),
            ShardLayout::single(3)
        );
        // 单片也接受显式分组，但必须把全部地图归片 0。
        assert_eq!(
            ShardLayout::partition(1, &maps, Some("0:*")).expect("0:* 覆盖全部"),
            ShardLayout::single(3)
        );
        assert_eq!(
            ShardLayout::partition(1, &maps, Some("0:0,0129,0100")).expect("显式列全"),
            ShardLayout::single(3)
        );
    }

    #[test]
    fn two_shard_partition_loads_two_disjoint_groups() {
        let maps = map_names(&["0", "0129", "0100", "3"]);
        let layout = ShardLayout::partition(2, &maps, Some("0:0,0129;1:*")).expect("合法 N=2 划分");
        assert_eq!(layout.count(), 2);
        assert_eq!(layout.maps_of(HOST_SHARD), &[0, 1], "片 0 = 前缀 [0,1]");
        assert_eq!(layout.maps_of(ShardId::new(1)), &[2, 3], "片 1 = 其余");
        // 两个组不相交。
        assert!(layout.owns(HOST_SHARD, 0));
        assert!(!layout.owns(HOST_SHARD, 2));
        assert!(layout.owns(ShardId::new(1), 2));
        assert!(layout.is_cross_shard(1, 2), "跨组 = 跨片");
        assert!(!layout.is_cross_shard(0, 1), "组内不跨片");
        // 覆盖校验输出（100% + 每图唯一归属由构造保证）。
        let report = layout.coverage_report();
        assert!(report.contains("shards=2"), "{report}");
        assert!(report.contains("maps=4"), "{report}");
        assert!(report.contains("assigned=4"), "{report}");
        assert!(report.contains("unassigned=0"), "{report}");
        assert!(report.contains("per_shard=[2,2]"), "{report}");
        // 宿主 World 拿**全长度**尺寸表：片 0 的真实尺寸 + 其它片的 1×1 stub（全局下标保序）。
        assert_eq!(
            layout.host_map_sizes(&[(1, 1), (2, 2), (3, 3), (4, 4)]),
            vec![(1, 1), (2, 2), (1, 1), (1, 1)]
        );
        assert_eq!(layout.host_ownership(), vec![true, true, false, false]);
        // 运行期守卫：片 0 → 片 1 的连接点被拒；片内放行。
        let runtime = ShardRuntime::new(layout);
        assert!(!runtime.route_allowed(0, 2));
        assert!(!runtime.route_allowed(2, 3), "源不在宿主片也不注册");
        assert!(runtime.route_allowed(0, 1));
    }

    #[test]
    fn three_shard_partition_covers_all_shards() {
        let maps = map_names(&["A", "B", "C", "D"]);
        let layout = ShardLayout::partition(3, &maps, Some("0:A;1:C,B;2:*")).expect("三片划分");
        assert_eq!(layout.maps_of(HOST_SHARD), &[0]);
        assert_eq!(
            layout.maps_of(ShardId::new(1)),
            &[1, 2],
            "组内顺序无关，按 map_id 升序"
        );
        assert_eq!(layout.maps_of(ShardId::new(2)), &[3]);
        assert!(layout.coverage_report().contains("per_shard=[1,2,1]"));
    }

    #[test]
    fn map_name_matching_is_ascii_case_insensitive() {
        let maps = map_names(&["Zero", "0129"]);
        let layout = ShardLayout::partition(2, &maps, Some("0:zero;1:*")).expect("大小写不敏感");
        assert_eq!(layout.maps_of(HOST_SHARD), &[0]);
        assert_eq!(layout.maps_of(ShardId::new(1)), &[1]);
    }

    #[test]
    fn multi_shard_requires_groups() {
        let maps = map_names(&["A", "B"]);
        assert_eq!(
            ShardLayout::partition(2, &maps, None),
            Err(ShardPlanError::GroupsRequired { count: 2 })
        );
    }

    /// **P2-3a**：任意非前缀分组现在可表达（全局下标 + stub + ownership），不再拒绝。
    ///
    /// 宿主尺寸表仍是全长度：片 0 拥有的图取真实尺寸，其它片填 1×1 stub；
    /// 对应 `owned_maps` 位图逐位正确。
    #[test]
    fn non_prefix_groups_are_supported_with_stub_sizes() {
        let maps = map_names(&["A", "B", "C", "D"]);
        // 片 0 拥有 B、D（map_id 1、3）；片 1 拥有 A、C（0、2）——既非前缀、也不连续。
        let layout = ShardLayout::partition(2, &maps, Some("0:B,D;1:*")).expect("非前缀分组合法");
        assert_eq!(layout.maps_of(HOST_SHARD), &[1, 3]);
        assert_eq!(layout.maps_of(ShardId::new(1)), &[0, 2]);
        assert_eq!(
            layout.host_map_sizes(&[(10, 10), (11, 11), (12, 12), (13, 13)]),
            vec![(1, 1), (11, 11), (1, 1), (13, 13)],
            "stub = (1,1)，全局下标不移动"
        );
        assert_eq!(layout.host_ownership(), vec![false, true, false, true]);
        // 运行期守卫按 host_owns：片 0 的图放行，片 1 的图拒绝（连接点分类不变）。
        let runtime = ShardRuntime::new(layout);
        assert!(runtime.host_owns(1) && runtime.host_owns(3));
        assert!(!runtime.host_owns(0) && !runtime.host_owns(2));
    }

    #[test]
    fn partition_rejects_unknown_and_duplicate_maps() {
        let maps = map_names(&["A", "B", "C"]);
        assert_eq!(
            ShardLayout::partition(2, &maps, Some("0:A,不存在;1:*")),
            Err(ShardPlanError::UnknownMap("不存在".to_string()))
        );
        assert_eq!(
            ShardLayout::partition(2, &maps, Some("0:A,C;1:A")),
            Err(ShardPlanError::MapAlreadyAssigned("A".to_string()))
        );
        assert_eq!(
            ShardLayout::partition(2, &maps, Some("0:A,A;1:*")),
            Err(ShardPlanError::MapAlreadyAssigned("A".to_string()))
        );
    }

    #[test]
    fn partition_requires_full_coverage_without_wildcard() {
        let maps = map_names(&["A", "B", "C"]);
        let err = ShardLayout::partition(2, &maps, Some("0:A;1:B")).expect_err("漏掉 C");
        assert_eq!(err, ShardPlanError::Unassigned(vec!["C".to_string()]));
        assert!(err.to_string().contains("C"));
        // 显式列全（不用 `*`）也可以。
        assert!(ShardLayout::partition(2, &maps, Some("0:A;1:B,C")).is_ok());
    }

    #[test]
    fn partition_rejects_empty_shards() {
        let maps = map_names(&["A", "B"]);
        assert_eq!(
            ShardLayout::partition(2, &maps, Some("0:A,B;1:*")),
            Err(ShardPlanError::EmptyShard(1))
        );
        // `*` 不因书写顺序抢走显式名单：`0:*;1:A` 里 A 先被分给片 1，片 0 只剩 B
        // ⇒ 两片都非空（P2-3a 起非前缀分组合法）。
        let layout = ShardLayout::partition(2, &maps, Some("0:*;1:A")).expect("两片非空");
        assert_eq!(layout.maps_of(HOST_SHARD), &[1]);
        assert_eq!(layout.maps_of(ShardId::new(1)), &[0]);
        assert_eq!(layout.host_ownership(), vec![false, true]);
    }

    #[test]
    fn partition_reports_syntax_and_range_errors() {
        let maps = map_names(&["A", "B"]);
        assert_eq!(
            ShardLayout::partition(2, &maps, Some("0")),
            Err(ShardPlanError::BadGroupSyntax("0".to_string()))
        );
        assert_eq!(
            ShardLayout::partition(2, &maps, Some("x:A;1:*")),
            Err(ShardPlanError::BadShardId("x".to_string()))
        );
        assert_eq!(
            ShardLayout::partition(2, &maps, Some("0:;1:*")),
            Err(ShardPlanError::EmptyGroupMaps(ShardId::new(0)))
        );
        assert_eq!(
            ShardLayout::partition(2, &maps, Some(";;")),
            Err(ShardPlanError::GroupsEmpty)
        );
        assert_eq!(
            ShardLayout::partition(2, &maps, Some("0:*;1:*")),
            Err(ShardPlanError::WildcardTwice)
        );
        assert_eq!(
            ShardLayout::partition(2, &maps, Some("0:A;0:B")),
            Err(ShardPlanError::DuplicateShard(0))
        );
        assert_eq!(
            ShardLayout::partition(2, &maps, Some("0:A;1:B;2:*")),
            Err(ShardPlanError::ShardOutOfRange { shard: 2, count: 2 })
        );
    }

    #[test]
    fn partition_errors_name_the_env_var() {
        for err in [
            ShardPlanError::GroupsRequired { count: 2 },
            ShardPlanError::GroupsEmpty,
            ShardPlanError::WildcardTwice,
            ShardPlanError::UnknownMap("A".to_string()),
        ] {
            assert!(
                err.to_string().contains(ENV_MAP_GROUPS),
                "错误必须点名 {ENV_MAP_GROUPS}：{err}"
            );
        }
    }

    // ---------- 启动决策（`MIR2_SHARDS=1` 零变化的反向牙） ----------

    /// **变异牙**：把「片数为 1 走单 World」的早退改坏，本用例必须红。
    ///
    /// 红检（逐个试）：
    /// 1. 删掉 `plan_layout` 的 `if cfg.is_single() && groups_raw.is_none()` 早退（让 1 片也走分组路径）
    ///    ⇒ 第一条断言拿到 `GroupsRequired{count:1}`，红；
    /// 2. 把早退条件改成 `cfg.count() <= MAX_SHARD_COUNT`（等价「1 片也按多片处理」）⇒ 同上红；
    /// 3. 把早退的返回值改成 `partition(...)` 且忽略 `groups_raw` ⇒ 无注册表那条断言红。
    #[test]
    fn plan_layout_single_shard_never_needs_groups_or_registry() {
        let cfg = ShardConfig::from_raw(Some("1")).expect("单片配置");
        // 无注册表 + 无分组：单片布局（`WorldMaps` 占位形态也必须能起）。
        assert_eq!(
            plan_layout(&cfg, None, 7, None).expect("单片无需注册表/分组"),
            ShardLayout::single(7)
        );
        // 有注册表、无分组：同一条快路径（不读名字表）。
        let names = map_names(&["A", "B"]);
        assert_eq!(
            plan_layout(&cfg, Some(&names), 2, None).expect("单片 + 注册表"),
            ShardLayout::single(2)
        );
        // 单片配了分组：必须真的校验（无注册表 ⇒ 报错，不静默忽略）。
        assert_eq!(
            plan_layout(&cfg, None, 2, Some("0:*")),
            Err(ShardPlanError::RegistryRequired { count: 1 })
        );
        assert_eq!(
            plan_layout(&cfg, Some(&names), 2, Some("0:*")).expect("0:* 覆盖全部"),
            ShardLayout::single(2)
        );
    }

    /// **变异牙**：把多片分支改成也返回单片布局（或多片不校验分组），本用例必须红。
    ///
    /// 红检：把 `plan_layout` 的末尾 `ShardLayout::partition(...)` 换成 `ShardLayout::single(map_count)`
    /// ⇒ 下面的 `count()==2` / `maps_of` 断言红；把 `GroupsRequired` 支去掉 ⇒ 第二条断言红。
    #[test]
    fn plan_layout_multi_shard_requires_groups_and_registry() {
        let cfg = ShardConfig::from_raw(Some("2")).expect("两片配置");
        let names = map_names(&["A", "B", "C"]);
        assert_eq!(
            plan_layout(&cfg, None, 3, Some("0:A;1:*")),
            Err(ShardPlanError::RegistryRequired { count: 2 })
        );
        assert_eq!(
            plan_layout(&cfg, Some(&names), 3, None),
            Err(ShardPlanError::GroupsRequired { count: 2 })
        );
        let layout = plan_layout(&cfg, Some(&names), 3, Some("0:A;1:*")).expect("合法两片划分");
        assert_eq!(layout.count(), 2);
        assert_eq!(layout.maps_of(HOST_SHARD), &[0]);
        assert_eq!(layout.maps_of(ShardId::new(1)), &[1, 2]);
    }

    /// **变异牙**：删掉 `startup_log_lines` 开头的 `if cfg.is_single()` 早退
    /// （让 1 片也打印分片日志）⇒ 本用例红（`MIR2_SHARDS=1` 的 stdout 就不再逐字节相同）。
    #[test]
    fn startup_log_lines_are_empty_for_single_shard() {
        let cfg = ShardConfig::from_raw(Some("1")).expect("单片配置");
        assert!(
            startup_log_lines(&cfg, &ShardLayout::single(5), None).is_empty(),
            "1 片必须零新增日志行"
        );
        // 显式 `MIR2_SHARDS=1` + 空布局/带分组也一样零行。
        assert!(startup_log_lines(&cfg, &ShardLayout::single(0), Some("0:*")).is_empty());
    }

    #[test]
    fn startup_log_lines_report_coverage_for_multi_shard() {
        let cfg = ShardConfig::from_raw(Some("2")).expect("两片配置");
        let names = map_names(&["A", "B", "C"]);
        let layout = ShardLayout::partition(2, &names, Some("0:A;1:*")).expect("两片划分");
        let lines = startup_log_lines(&cfg, &layout, Some("0:A;1:*"));
        assert_eq!(lines.len(), 3, "2 条 INF + 每片 1 条 WRN");
        assert_eq!(lines[0].0, "INF");
        assert!(lines[1].1.contains("shards=2"), "{:?}", lines[1]);
        assert!(lines[1].1.contains("per_shard=[1,2]"), "{:?}", lines[1]);
        assert_eq!(lines[2].0, "WRN");
        assert!(lines[2].1.contains("shard 1"), "{:?}", lines[2]);
    }

    // ---------- P2-3b：本地片参数化 ----------

    /// **P2-3b 牙**：同一份布局造出的每片 runtime 只认自己的图（片 1 视角与片 0 互补）。
    ///
    /// 红检：把 `for_shard` 忽略 `shard`（恒用 HOST_SHARD）⇒ 片 1 的 `local_owns(1)` 变 false、
    /// `local_owns(0)` 变 true ⇒ 前四条断言红；把 `classify_route` 的 `local_owns` 改回
    /// `layout.owns(HOST_SHARD, …)` ⇒ 分类三条红。
    #[test]
    fn for_shard_keeps_from_its_own_perspective() {
        let maps = map_names(&["A", "B", "C"]);
        let layout = ShardLayout::partition(2, &maps, Some("0:A;1:*")).expect("两片划分");
        let mut s1 = ShardRuntime::for_shard(layout, ShardId::new(1));
        assert_eq!(s1.local_shard(), ShardId::new(1));
        assert!(s1.local_owns(1) && s1.local_owns(2), "片 1 拥有 B/C");
        assert!(!s1.local_owns(0), "片 1 不拥有 A");
        assert_eq!(s1.classify_route(2, 1), RouteClass::Keep, "片内");
        assert_eq!(
            s1.classify_route(2, 0),
            RouteClass::CrossShard,
            "片 1 → 片 0"
        );
        assert_eq!(
            s1.classify_route(0, 1),
            RouteClass::ForeignShard,
            "源不在片 1"
        );
        // guard 以本地片为准：片 1 的图放行、片 0 的图拒绝并计数。
        assert_eq!(s1.guard_cross_shard_enter("hero", "B", 1), None);
        let line = s1
            .guard_cross_shard_enter("hero", "A", 0)
            .expect("片 0 的图对片 1 是跨片");
        assert!(line.contains("map_id=0"), "{line}");
        assert_eq!(s1.counters().cross_shard_entry_refused, 1);
        assert!(!s1.accept_map_change(0) && s1.accept_map_change(2));
    }

    /// **P2-3b 牙**：`new` 与 `for_shard(…, 0)` 逐字段相等（兼容别名不改片 0 语义）。
    ///
    /// 红检：让 `new` 直接填 `local: ShardId::new(1)`（或跳过 `for_shard` 的越界断言）⇒ 相等断言红。
    #[test]
    fn new_is_for_shard_zero() {
        let layout =
            ShardLayout::partition(2, &map_names(&["A", "B"]), Some("0:A;1:*")).expect("两片划分");
        assert_eq!(
            ShardRuntime::new(layout.clone()),
            ShardRuntime::for_shard(layout, HOST_SHARD)
        );
    }

    /// **P2-3b 牙**：每张图在**恰好一片**的 `local_map_sizes` 里是真实尺寸，其余片是 1×1 stub；
    /// `local_ownership` 与之逐位一致。
    ///
    /// 红检：把 `local_map_sizes` 里的 `self.maps_of(shard)` 改回 `maps_of(HOST_SHARD)`
    /// ⇒ 片 1 的尺寸表变成与片 0 相同 ⇒ 第二条与「恰属一片」断言红。
    #[test]
    fn local_sizes_and_ownership_partition_every_map_exactly_once() {
        let maps = map_names(&["A", "B", "C", "D"]);
        let layout = ShardLayout::partition(2, &maps, Some("0:B,D;1:*")).expect("非前缀划分");
        let all = vec![(10, 10), (11, 11), (12, 12), (13, 13)];
        let s0 = layout.local_map_sizes(HOST_SHARD, &all);
        let s1 = layout.local_map_sizes(ShardId::new(1), &all);
        assert_eq!(s0, vec![(1, 1), (11, 11), (1, 1), (13, 13)]);
        assert_eq!(s1, vec![(10, 10), (1, 1), (12, 12), (1, 1)]);
        for map_id in 0..maps.len() {
            let real = [s0[map_id] != (1, 1), s1[map_id] != (1, 1)];
            assert_eq!(
                real.iter().filter(|&&x| x).count(),
                1,
                "map_id={map_id} 必须恰属一片"
            );
        }
        assert_eq!(
            layout.local_ownership(HOST_SHARD),
            vec![false, true, false, true]
        );
        assert_eq!(
            layout.local_ownership(ShardId::new(1)),
            vec![true, false, true, false]
        );
    }

    /// 越界片号是编程错误 ⇒ `for_shard` panic（不静默起一个永远没有地图的 World）。
    ///
    /// 红检：删掉 `for_shard` 开头的 `assert!` ⇒ 本用例从 panic 变成正常返回 ⇒ 红。
    #[test]
    #[should_panic(expected = "越界")]
    fn for_shard_panics_on_out_of_range_shard() {
        let layout =
            ShardLayout::partition(2, &map_names(&["A", "B"]), Some("0:A;1:*")).expect("两片划分");
        let _ = ShardRuntime::for_shard(layout, ShardId::new(2));
    }
}
