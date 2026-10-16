//! 地图加载器 + 地图名→`map_id` 注册表。
//!
//! **`map_id` 的来源**：`src/GameSrv/Maps/MapManager.cs` 的 `_mapList` 是
//! `Dictionary<string, IEnvirnoment>(StringComparer.OrdinalIgnoreCase)`，由 `Maps.LoadMapInfo`
//! 按 **MapInfo.txt 的 section 先后顺序**逐个 `AddMapInfo` 填入（**加载成功才入表**）。
//! Rust 世界（`crates/world`）用 `World.maps` 的下标当 `map_id`，因此本模块把
//! 「按 MapInfo.txt 顺序加载成功的地图」的下标定义为 `map_id`；查找大小写不敏感
//! （对应 OrdinalIgnoreCase；地图名实际都是 ASCII，故用 ASCII 小写归一化）。
//!
//! **找不到就报错**：`MapRegistry::require_id` 返回 `MapNameNotFound`，绝不静默退回 0 号占位图。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::EnvirError;
use crate::mapfile::{attach_points, load_map_file, MapFile};
use crate::mapinfo::{
    connect_map_info_arg, parse_map_info_lines, parse_map_routes, MapInfoEntry, MapInfoFlag,
    MapRouteEntry,
};
use crate::paths::{envir_file_path, map_file_path, point_file_path};
use crate::text::decode_text;

/// 一条未能入表的地图（诊断用；对应 C# 的 `LogService.Error`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedMap {
    pub map_name: String,
    pub map_file_name: String,
    pub reason: String,
}

/// 取图配置（`M2Share.BasePath` + `Config.MapDir` / `Config.EnvirDir`）。
#[derive(Clone, Debug)]
pub struct MapLoader {
    pub base_path: PathBuf,
    pub envir_dir: String,
    pub map_dir: String,
}

impl MapLoader {
    #[must_use]
    pub fn new(
        base_path: impl Into<PathBuf>,
        map_dir: impl Into<String>,
        envir_dir: impl Into<String>,
    ) -> Self {
        MapLoader {
            base_path: base_path.into(),
            envir_dir: envir_dir.into(),
            map_dir: map_dir.into(),
        }
    }

    /// `Path.Combine(BasePath, MapDir, 文件名 + ".map")`。
    #[must_use]
    pub fn map_file_path(&self, file_name: &str) -> PathBuf {
        map_file_path(&self.base_path, &self.map_dir, file_name)
    }

    /// `LoadMapData` 里的点位表路径（`.map` 路径 + `.txt`，见 `paths` 模块头）。
    #[must_use]
    pub fn point_file_path(&self, map_file: &Path) -> PathBuf {
        point_file_path(&self.base_path, &self.envir_dir, map_file)
    }

    /// `GetEnvirFilePath("MapInfo.txt")`。
    #[must_use]
    pub fn map_info_path(&self) -> PathBuf {
        envir_file_path(&self.base_path, &self.envir_dir, "", "MapInfo.txt")
    }

    /// 按文件名（不带 `.map`）加载地图 + 同名点位表。
    ///
    /// # Errors
    /// 地图文件缺失/损坏（`EnvirError`）。
    pub fn load(&self, file_name: &str) -> Result<MapFile, EnvirError> {
        self.load_path(&self.map_file_path(file_name))
    }

    /// 按 `.map` 路径加载地图，并按 `point_file_path` 口径补读点位表。
    ///
    /// # Errors
    /// 地图文件缺失/损坏（`EnvirError`）。
    pub fn load_path(&self, map_file: &Path) -> Result<MapFile, EnvirError> {
        let mut map = load_map_file(map_file)?;
        let point_path = self.point_file_path(map_file);
        if let Ok(bytes) = std::fs::read(&point_path) {
            attach_points(&mut map, &decode_text(&bytes));
            map.point_file = Some(point_path);
        }
        Ok(map)
    }

    /// 读 `Envir/MapInfo.txt` 并抽出所有 section（含 `ConnectMapInfo` 子文件展开）。
    ///
    /// 文件不存在时返回空表（对应 C# `if (File.Exists(...))` 为假直接跳过）。
    ///
    /// # Errors
    /// 文件存在但读取失败。
    pub fn load_map_info_entries(&self) -> Result<Vec<MapInfoEntry>, EnvirError> {
        Ok(parse_map_info_lines(&self.load_map_info_lines()?))
    }

    /// `Maps.cs:361-379` 的**地图连接点**行（与 section 同文件集/顺序）。
    ///
    /// 与 [`Self::load_map_info_entries`] 共用 [`Self::load_map_info_lines`]，保证 route 行与
    /// section 行来自**同一份 `loadList`**（`Maps.cs:38-51` 的 `ConnectMapInfo` 展开只做一次）。
    ///
    /// # Errors
    /// MapInfo.txt 存在但读取失败。
    pub fn load_map_routes(&self) -> Result<Vec<MapRouteEntry>, EnvirError> {
        Ok(parse_map_routes(&self.load_map_info_lines()?))
    }

    /// 组装 `Maps.cs` 的 `loadList`：读主 `MapInfo.txt`，再按第一遍扫到的 `ConnectMapInfo`
    /// 把子文件内容**追加到表尾**（`Maps.cs:38-51` + `LoadSubMapInfo`）。
    ///
    /// # Errors
    /// 主 `MapInfo.txt` 存在但读取失败。
    fn load_map_info_lines(&self) -> Result<Vec<String>, EnvirError> {
        let path = self.map_info_path();
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(EnvirError::Io { path, source }),
        };
        let mut lines = split_lines(&decode_text(&bytes));

        // `Maps.cs:38-51`：第一遍扫 `ConnectMapInfo`，把子文件内容**追加到表尾**。
        let mut i = 0;
        while i < lines.len() {
            let Some(sub) = connect_map_info_arg(&lines[i]) else {
                i += 1;
                continue;
            };
            lines.remove(i);
            // `LoadSubMapInfo`：`fileDir = GetEnvirFilePath("MapInfo")`，路径是
            // `fileDir + sFileName`（源码用字符串拼接，没有分隔符——照抄）。
            let file_dir = envir_file_path(&self.base_path, &self.envir_dir, "", "MapInfo");
            let sub_path = PathBuf::from(format!("{}{sub}", file_dir.display()));
            if let Ok(bytes) = std::fs::read(&sub_path) {
                lines.extend(split_lines(&decode_text(&bytes)));
            }
        }
        Ok(lines)
    }

    /// 按 MapInfo.txt 顺序加载所有地图，构建 `map_id` 注册表。
    ///
    /// # Errors
    /// MapInfo.txt 读取失败。
    pub fn load_registry(&self) -> Result<MapRegistry, EnvirError> {
        let entries = self.load_map_info_entries()?;
        Ok(MapRegistry::from_entries(self.clone(), entries))
    }
}

/// 一条**已解析出 `map_id`** 的地图连接点：只有 src/dst 两侧**都加载成功**（等价于 oracle
/// `FindMap` 非空）才保留（`MapManager.AddMapRoute` 的 `sEnvir != null && dEnvir != null` 门）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedRoute {
    /// oracle `MapRouteItem.RouteId`（`ActorMgr.GetNextIdentity()`）。本仓**不等价**：帧不读该表
    /// （见 `game_svr::sessionwire` 注释），这里取 `routes` 下标 + 1（唯一 + 保序）。
    pub route_id: i32,
    pub src_map_id: usize,
    pub src_x: i32,
    pub src_y: i32,
    pub dst_map_id: usize,
    pub dst_x: i32,
    pub dst_y: i32,
}

/// 地图名 → `map_id` 注册表。
#[derive(Clone, Debug)]
pub struct MapRegistry {
    /// **Box 以压 `MapRegistry` 结构尺寸**：`mapwire::Maps::Envir(MapRegistry)` 受
    /// `clippy::large_enum_variant` 门（200B 阈值）约束，route 表两项 Vec 会把裸结构推过阈值；
    /// `MapLoader` 是只读配置（进/出都不改），Box 后行为不变。
    loader: Box<MapLoader>,
    entries: Vec<MapInfoEntry>,
    maps: Vec<MapFile>,
    /// 已入表 section（`map_id` 下标；承载逻辑名 + `Flag` + `MapDesc`，与 `maps` 平行）。
    loaded: Vec<MapInfoEntry>,
    index: HashMap<String, usize>,
    skipped: Vec<SkippedMap>,
    /// 已解析出 `map_id` 的地图连接点（oracle `MapManager.AddMapRoute` 成功者，保序）。
    routes: Vec<ResolvedRoute>,
    /// 因 src/dst 图未加载而被丢弃的连接点行（`AddMapRoute` 返回 false 的等价物，计数登记）。
    unresolved_routes: Vec<MapRouteEntry>,
}

impl MapRegistry {
    /// 按条目顺序加载；加载成功且名字不重复的进入 `maps`（`map_id = maps.len()` 时刻）。
    #[must_use]
    pub fn from_entries(loader: MapLoader, entries: Vec<MapInfoEntry>) -> Self {
        let mut reg = MapRegistry {
            loader: Box::new(loader),
            entries,
            maps: Vec::new(),
            loaded: Vec::new(),
            index: HashMap::new(),
            skipped: Vec::new(),
            routes: Vec::new(),
            unresolved_routes: Vec::new(),
        };
        let entries = reg.entries.clone();
        for e in entries {
            if e.map_name.is_empty() {
                continue;
            }
            let key = normalize_name(&e.map_name);
            match reg.loader.load(&e.map_file_name) {
                Ok(mut map) => {
                    if reg.index.contains_key(&key) {
                        reg.skipped.push(SkippedMap {
                            map_name: e.map_name,
                            map_file_name: e.map_file_name,
                            reason: "地图名称重复（MapManager.AddMapInfo）".to_string(),
                        });
                        continue;
                    }
                    map.map_name = e.map_name.clone();
                    let id = reg.maps.len();
                    reg.index.insert(key, id);
                    reg.loaded.push(e);
                    reg.maps.push(map);
                }
                Err(err) => reg.skipped.push(SkippedMap {
                    map_name: e.map_name,
                    map_file_name: e.map_file_name,
                    reason: err.to_string(),
                }),
            }
        }
        // `Maps.cs:361-379` 的连接点：与 section 同文件集/顺序；`MapManager.AddMapRoute`
        // 只在 src/dst 两侧都能 `FindMap` 时入表，否则返回 false（本仓登记为 `unresolved_routes`）。
        // `from_entries` 已持有 `loader` 且已做文件 I/O（逐图 `load`），这里一并取 route，使注册表
        // 自洽（`mapwire::load_registry` 无需改动）。读取失败 ⇒ 空表（不 panic）。
        let route_entries = reg.loader.load_map_routes().unwrap_or_default();
        for e in route_entries {
            match (reg.id_of(&e.src_map_name), reg.id_of(&e.dst_map_name)) {
                (Some(s), Some(d)) => {
                    let route_id = (reg.routes.len() + 1) as i32;
                    reg.routes.push(ResolvedRoute {
                        route_id,
                        src_map_id: s,
                        src_x: e.src_x,
                        src_y: e.src_y,
                        dst_map_id: d,
                        dst_x: e.dst_x,
                        dst_y: e.dst_y,
                    });
                }
                // oracle 的 `AddMapRoute` 返回 false（源或目标图缺失）⇒ 该 route 不入表。
                _ => reg.unresolved_routes.push(e),
            }
        }
        reg
    }

    #[must_use]
    pub fn loader(&self) -> &MapLoader {
        self.loader.as_ref()
    }

    /// MapInfo.txt 的全部 section（含未加载成功者）。
    #[must_use]
    pub fn entries(&self) -> &[MapInfoEntry] {
        &self.entries
    }

    /// 未能入表的地图（重复名 / 文件缺失 / 解析失败）。
    #[must_use]
    pub fn skipped(&self) -> &[SkippedMap] {
        &self.skipped
    }

    /// 已解析 `map_id` 的地图连接点（oracle `MapManager.AddMapRoute` 成功者，保序）。
    #[must_use]
    pub fn routes(&self) -> &[ResolvedRoute] {
        &self.routes
    }

    /// 因 src/dst 图未加载而被丢弃的连接点行（计数登记）。
    #[must_use]
    pub fn unresolved_routes(&self) -> &[MapRouteEntry] {
        &self.unresolved_routes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.maps.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.maps.is_empty()
    }

    /// 已加载地图（下标即 `map_id`）。
    #[must_use]
    pub fn maps(&self) -> &[MapFile] {
        &self.maps
    }

    /// `map_id` → 逻辑名。
    #[must_use]
    pub fn name_of(&self, map_id: usize) -> Option<&str> {
        self.loaded.get(map_id).map(|e| e.map_name.as_str())
    }

    /// `map_id` → `Envirnoment.Flag`（帧 766/54/46 的 `Envir.Flag` 输入）。
    #[must_use]
    pub fn flag_of(&self, map_id: usize) -> Option<&MapInfoFlag> {
        self.loaded.get(map_id).map(|e| &e.flag)
    }

    /// `map_id` → `Envirnoment.MapDesc`（帧 54 的尾巴）。
    #[must_use]
    pub fn map_desc_of(&self, map_id: usize) -> Option<&str> {
        self.loaded.get(map_id).map(|e| e.desc.as_str())
    }

    /// 逻辑名 → `map_id`（ASCII 大小写不敏感）。
    #[must_use]
    pub fn id_of(&self, map_name: &str) -> Option<usize> {
        self.index.get(&normalize_name(map_name)).copied()
    }

    /// 同 `id_of`，但找不到时**报错**（集成点禁止静默用占位图）。
    ///
    /// # Errors
    /// 名字不在注册表内 ⇒ `EnvirError::MapNameNotFound`。
    pub fn require_id(&self, map_name: &str) -> Result<usize, EnvirError> {
        self.id_of(map_name)
            .ok_or_else(|| EnvirError::MapNameNotFound(map_name.to_string()))
    }

    #[must_use]
    pub fn get(&self, map_id: usize) -> Option<&MapFile> {
        self.maps.get(map_id)
    }

    #[must_use]
    pub fn map_by_name(&self, map_name: &str) -> Option<&MapFile> {
        self.id_of(map_name).and_then(|id| self.maps.get(id))
    }
}

/// `StringComparer.OrdinalIgnoreCase` 的 ASCII 归一化（地图名实际都是 ASCII）。
fn normalize_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// `StreamReader.ReadLine` 分行（\r\n / \n / \r）。
fn split_lines(text: &str) -> Vec<String> {
    text.split(['\n', '\r'])
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
        // 去掉 `\r\n` 被拆出的空行；但保留真正的空行（C# ReadLine 不会产生空行）。
        .filter(|l| !l.is_empty())
        .collect()
}
