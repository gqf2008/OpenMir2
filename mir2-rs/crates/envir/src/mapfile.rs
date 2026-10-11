//! Envir `*.map` 加载器 —— 格式由卡 `openmir2-m2-map-loading` 的 comment `862396f8` 在**部署件**上定案。
//!
//! 定案九条（此处逐条落地，别再自己猜）：
//! 1. 文件头 **52 字节**；`LoadMapData` 只读偏移 `0..3` 的 `int16 Width/Height`（小端），随后 `seek(52)`；
//!    文件长度 `<= 52` ⇒ 加载失败。
//! 2. 每格 **12 字节**：`wBkImg u16`、`wMidImg u16`、`wFrImg u16`、`btDoorIndex u8`、`btDoorOffset u8`、
//!    `btAniFrame u8`、`btAniTick u8`、`btArea u8`、`btLight u8`。
//! 3. 文件序 ↔ 坐标：`k = x * Height + y`（**X 主序**；`nW` 外层、`nH` 内层）。
//! 4. `wBkImg` bit15 ⇒ `HighWall`；`wFrImg` bit15 ⇒ `LowWall`；**`wMidImg` 忽略**。
//! 5. **先判 High 再判 Low ⇒ Low 覆盖 High**（两者都置位 ⇒ `LowWall`）。
//! 6. 门：`btDoorIndex` bit7 置位且 `(btDoorIndex & 0x7F) > 0` ⇒ 记一条门；`0x80`（point=0）不记。
//! 7. 门 Status 合并：同图内 `|dx| <= 10 && |dy| <= 10 && n08 相同` 的门**共享同一个 DoorStatus 实例**
//!    （`DoorList` 里仍是各自独立条目）。Rust 用「`door_status` 数组下标」表示实例身份。
//! 8. `ObjList` 只在「首个墙格之前」的格被分配（源码 `isInitialize` 只置 false 不复位）；
//!    真图 `0.map` / `0129.map` 全部为 NULL。**处置**：Rust 的 `MapGrid` 每格恒有一个空 `Vec`
//!    （`crates/world`），因为 `ObjList == null` 与「空表」在**可观测层面等价**
//!    （`IsAvailable => ObjList?.Count > 0` 两者都是 false，`AddMapObject` 遇 null 也会**懒分配**，
//!    `Envirnoment.cs:261-264`）——所以不引入 `Option`；本模块仍保留 `obj_list_allocated()`
//!    以便把「首个墙格」这一源码事实作为证据输出。
//! 9. 有效性与可用性：`Valid == (Attribute == Walk)`；`IsAvailable == ObjList 非空且 Count > 0`。

use std::path::{Path, PathBuf};

use mir2_world::{CellAttribute, MapGrid};

use crate::error::EnvirError;
use crate::points::{parse_points, PointInfo};

/// 文件头长度（`fileStream.Position = 52`）。
pub const HEADER_BYTES: usize = 52;
/// 每格字节数（3×u16 + 6×u8）。
pub const CELL_BYTES: usize = 12;
/// 阻挡位（WORD bit15）。
pub const WALL_BIT: u16 = 0x8000;
/// 门索引的第 7 位（`btDoorIndex & 0x80`）。
pub const DOOR_BIT: u8 = 0x80;
/// 门索引低 7 位掩码（`btDoorIndex & 0x7F`）。
pub const DOOR_POINT_MASK: u8 = 0x7F;
/// 门 Status 合并的坐标半径（源码 `<= 10`）。
pub const DOOR_MERGE_RANGE: i32 = 10;

/// 一格的原始 12 字节（`private record struct MapUnitInfo`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapCell {
    pub bk_img: u16,
    pub mid_img: u16,
    pub fr_img: u16,
    pub door_index: u8,
    pub door_offset: u8,
    pub ani_frame: u8,
    pub ani_tick: u8,
    pub area: u8,
    pub light: u8,
}

impl MapCell {
    /// `wBkImg` bit15。
    #[must_use]
    pub const fn high_wall(&self) -> bool {
        self.bk_img & WALL_BIT != 0
    }

    /// `wFrImg` bit15。
    #[must_use]
    pub const fn low_wall(&self) -> bool {
        self.fr_img & WALL_BIT != 0
    }

    /// 复刻 `Envirnoment.cs:945-951`：先 `wBkImg`(High) 再 `wFrImg`(Low)，**后者覆盖前者**；
    /// `wMidImg` 不参与（定案③）。
    #[must_use]
    pub const fn attribute(&self) -> CellAttribute {
        if self.low_wall() {
            CellAttribute::LowWall
        } else if self.high_wall() {
            CellAttribute::HighWall
        } else {
            CellAttribute::Walk
        }
    }
}

/// `OpenMir2.Data.DoorStatus`（可被同组多门共享的可变状态）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DoorStatus {
    pub opened: bool,
    pub open_tick: i32,
}

/// `src/Modules/SystemModule/Data/MapDoor.cs` 的一条门。
///
/// `status_id` 就是 C# `MapDoor.Status` 的对象身份的 Rust 表示：两条门 `status_id` 相同
/// ⇔ C# 里它们是**同一个** `DoorStatus` 实例。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapDoor {
    pub x: i16,
    pub y: i16,
    /// C# `n08 = btDoorIndex & 0x7F`（> 0）。
    pub n08: u8,
    /// `LoadMapData` 里恒为 0（`MapDoor.DoorId` 默认值）。
    pub door_id: i32,
    pub status_id: usize,
}

/// 一张已加载的地图（`Envirnoment` 的可观测状态子集 + 原始格数据）。
#[derive(Clone, Debug)]
pub struct MapFile {
    /// 逻辑地图名（`MapManager._mapList` 的键 / 角色记录 `CurMap`）；由 `MapRegistry` 填入。
    pub map_name: String,
    pub width: i32,
    pub height: i32,
    /// 按文件序（`k = x * Height + y`）排列的原始格。
    pub cells: Vec<MapCell>,
    /// 门的 Status 实例表（对应 C# 各 `DoorStatus` 对象，按需创建）。
    pub door_status: Vec<DoorStatus>,
    pub doors: Vec<MapDoor>,
    pub points: Vec<PointInfo>,
    /// 实际读到的 `.map` 路径（`MapLoader` 填入）。
    pub map_file: PathBuf,
    /// 实际读到的点位表路径（读到时填）。
    pub point_file: Option<PathBuf>,
    /// 首个墙格的文件下标；`None` = 整图无墙（`isInitialize` 从未置 false）。
    pub first_wall_index: Option<usize>,
}

impl MapFile {
    /// 文件序索引（X 主序）：`x * Height + y`。
    #[must_use]
    pub fn index(&self, x: i32, y: i32) -> Option<usize> {
        if self.cell_match(x, y) {
            Some((x * self.height + y) as usize)
        } else {
            None
        }
    }

    /// 对应 `CellMatch`。
    #[must_use]
    pub fn cell_match(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.width && y >= 0 && y < self.height
    }

    /// 该格的最终属性。
    #[must_use]
    pub fn attribute(&self, x: i32, y: i32) -> CellAttribute {
        self.index(x, y)
            .map_or(CellAttribute::Walk, |k| self.cells[k].attribute())
    }

    /// 该格是否在源码里被分配了 `ObjList`（文件下标 < 首个墙格下标）。
    #[must_use]
    pub fn obj_list_allocated_at(&self, index: usize) -> bool {
        self.first_wall_index.is_none_or(|f| index < f)
    }

    /// 最终属性为 `HighWall` 的格数（已被 Low 覆盖的不计）。
    #[must_use]
    pub fn high_count(&self) -> usize {
        self.cells
            .iter()
            .filter(|c| c.attribute() == CellAttribute::HighWall)
            .count()
    }

    /// 最终属性为 `LowWall` 的格数。
    #[must_use]
    pub fn low_count(&self) -> usize {
        self.cells
            .iter()
            .filter(|c| c.attribute() == CellAttribute::LowWall)
            .count()
    }

    /// 最终属性为 `Walk` 的格数。
    #[must_use]
    pub fn walk_count(&self) -> usize {
        self.cells
            .iter()
            .filter(|c| c.attribute() == CellAttribute::Walk)
            .count()
    }

    /// 转成 `crates/world` 的 `MapGrid`（属性 + 空对象表）。
    ///
    /// **对象表处置**：`MapGrid` 每格恒有一个空 `Vec`，不带 `Option`——见模块头第 8 条，
    /// null 与空表在可观测层面等价。`MapGrid::new` 要求 W/H 均为正（本模块已保证 > 1）。
    #[must_use]
    pub fn to_grid(&self) -> MapGrid {
        let mut grid = MapGrid::new(self.width, self.height);
        for x in 0..self.width {
            for y in 0..self.height {
                let attr = self.attribute(x, y);
                let (cell, success) = grid.get_cell_info_mut(x, y);
                // 每格只访问一次且初始为 Walk（valid），必命中真实格。
                debug_assert!(success, "MapGrid 初始格应为 Walk: ({x},{y})");
                cell.set_attribute(attr);
            }
        }
        grid
    }
}

/// 解析 `.map` 字节（不读点位表；点位表由 `MapLoader` 另读）。
///
/// # Errors
/// 文件 <= 52 字节、尺寸 <= 1、数据截断，都返回 `Err`（对应 C# `LoadMapData` 的 `result=false`）。
pub fn parse_map_bytes(data: &[u8]) -> Result<MapFile, EnvirError> {
    if data.len() <= HEADER_BYTES {
        return Err(EnvirError::FileTooSmall {
            size: data.len() as u64,
        });
    }
    let width = i32::from(i16::from_le_bytes([data[0], data[1]]));
    let height = i32::from(i16::from_le_bytes([data[2], data[3]]));
    if width <= 1 || height <= 1 {
        return Err(EnvirError::InvalidSize {
            w: width,
            h: height,
        });
    }
    let count = (width as usize)
        .checked_mul(height as usize)
        .ok_or(EnvirError::InvalidSize {
            w: width,
            h: height,
        })?;
    let need = HEADER_BYTES as u64 + (count as u64) * (CELL_BYTES as u64);
    if (data.len() as u64) < need {
        return Err(EnvirError::Truncated {
            w: width,
            h: height,
            need,
            actual: data.len() as u64,
        });
    }

    let mut cells: Vec<MapCell> = Vec::with_capacity(count);
    let mut doors: Vec<MapDoor> = Vec::new();
    let mut door_status: Vec<DoorStatus> = Vec::new();
    let mut first_wall_index: Option<usize> = None;
    let mut is_initialize = true;

    for nw in 0..width {
        for nh in 0..height {
            let k = cells.len();
            let off = HEADER_BYTES + k * CELL_BYTES;
            let cell = MapCell {
                bk_img: u16::from_le_bytes([data[off], data[off + 1]]),
                mid_img: u16::from_le_bytes([data[off + 2], data[off + 3]]),
                fr_img: u16::from_le_bytes([data[off + 4], data[off + 5]]),
                door_index: data[off + 6],
                door_offset: data[off + 7],
                ani_frame: data[off + 8],
                ani_tick: data[off + 9],
                area: data[off + 10],
                light: data[off + 11],
            };

            // `isInitialize` 在任何墙格处置 false（源码两次 if 都会置 false）。
            if is_initialize && (cell.high_wall() || cell.low_wall()) {
                is_initialize = false;
                first_wall_index = Some(k);
            }

            // 门（定案⑥⑦）。
            if cell.door_index & DOOR_BIT != 0 {
                let point = cell.door_index & DOOR_POINT_MASK;
                if point > 0 {
                    let x = nw as i16;
                    let y = nh as i16;
                    let mut status_id = None;
                    for d in &doors {
                        if (i32::from(d.x) - i32::from(x)).abs() <= DOOR_MERGE_RANGE
                            && (i32::from(d.y) - i32::from(y)).abs() <= DOOR_MERGE_RANGE
                            && d.n08 == point
                        {
                            status_id = Some(d.status_id);
                            break;
                        }
                    }
                    let status_id = match status_id {
                        Some(id) => id,
                        None => {
                            door_status.push(DoorStatus::default());
                            door_status.len() - 1
                        }
                    };
                    doors.push(MapDoor {
                        x,
                        y,
                        n08: point,
                        door_id: 0,
                        status_id,
                    });
                }
            }

            cells.push(cell);
        }
    }

    Ok(MapFile {
        map_name: String::new(),
        width,
        height,
        cells,
        door_status,
        doors,
        points: Vec::new(),
        map_file: PathBuf::new(),
        point_file: None,
        first_wall_index,
    })
}

/// 解析点位表文本并写入地图（`width`/`height` 取地图尺寸）。
pub fn attach_points(map: &mut MapFile, text: &str) {
    map.points = parse_points(text, map.width, map.height);
}

/// 便捷入口：从磁盘读 `.map` + 同名点位表。
///
/// 点位表路径按 `paths::point_file_path` 口径推导；文件不存在时 `points` 为空、
/// `point_file` 为 `None`（C# 里 `File.Exists` 为假则跳过，不算错误）。
///
/// # Errors
/// 地图文件不存在或解析失败时返回 `Err`。
pub fn load_map_file(map_file: &Path) -> Result<MapFile, EnvirError> {
    let data = std::fs::read(map_file).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            EnvirError::FileNotFound(map_file.to_path_buf())
        } else {
            EnvirError::Io {
                path: map_file.to_path_buf(),
                source,
            }
        }
    })?;
    let mut map = parse_map_bytes(&data)?;
    map.map_file = map_file.to_path_buf();
    Ok(map)
}
