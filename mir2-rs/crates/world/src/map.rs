//! 地图与格子存储 —— 移植源：
//! - `src/M2Server/Maps/Envirnoment.cs`（`AddMapObject` / `GetCellInfo` / `MoveToMovingObject` / `CellMatch` / `CellValid`）
//! - `src/Modules/SystemModule/Data/MapCellInfo.cs`、`CellObject.cs`、`CellType.cs`
//!
//! 逐条保真的点（别"顺手修"）：
//! 1. **列主序**：`CellArray[x * Height + y]`（C# `Envirnoment.cs` 的索引方式）；
//! 2. `GetCellInfo` 失败时 C# 返回 `ref CellArray[0]`——于是 `MoveToMovingObject` 在
//!    "越界/不可走"时会读到**第 0 格**的属性与对象表（`cellSuccess=false` 但后续仍有
//!    `if (canMove && cellInfo.Valid)` 分支）。此处原样保留：失败返回 `Grid::cell0`。
//! 3. `Valid` 即 `Attribute == Walk`；`IsAvailable` 即 `ObjList` 非空。
//! 4. `AddMapObject` 只在"格子在界内且 Valid"时追加；首次加入会置 `add_to_mapped`。

/// `src/Modules/SystemModule/Data/CellType.cs`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CellType {
    Event = 1,
    Play = 2,
    Item = 3,
    MapRoute = 4,
    MapEvent = 5,
    Door = 6,
    Roon = 7,
    Merchant = 8,
    Monster = 9,
    SavleMonster = 10,
    CastleDoor = 11,
}

/// `CellObject.cs` 里的 `CellAttribute`（`Valid` 的判据）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CellAttribute {
    /// 可以走动
    Walk = 0,
    HighWall = 1,
    /// 不能走动
    LowWall = 2,
}

impl Default for CellAttribute {
    /// C# `MapCellInfo` 的字段初始值就是 `CellAttribute.Walk`。
    fn default() -> Self {
        CellAttribute::Walk
    }
}

/// 地面物品的类别（world 侧只读分片，owner §25.1 裁决 (A)）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloorItemKind {
    /// 普通物品（`CellObject.cell_obj_id` = `StdItem` 序号）
    Item,
    /// 金币（`CellObject.cell_obj_id` = 落主 `ActorId`）
    Gold,
}

/// 地面物品的**最小身份**（不掉落"完整 `MapItem`"：只记接线层无法可靠反推的字段）。
///
/// 记账点在**显式落格注入**时（`MapGrid::add_floor_item`；oracle 死亡掉落因
/// `Envir.AddItemToMap` 首行 `MapItem.ItemId == 0` 门而恒不落地，故生产路径不落格），
/// 字段来源见 `owner §25.1`：
/// `kind/index/count` 供 `ground::GroundItem` 装配，`item_id` 是删除格子的唯一键，
/// `of_base_object` = oracle `RM_DIEDROPITEM` 的 `wParam`（`BaseObject.cs:2437/2442`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FloorItemMeta {
    pub kind: FloorItemKind,
    /// `StdItem` 序号（1-based）；金币 = 0。
    pub index: u16,
    /// 金币数 / 物品数（物品恒 1）。
    pub count: i32,
    /// 唯一 id（物品 = `UserItem.MakeIndex`；金币 = 落主 `ActorId`）。
    pub item_id: i32,
    /// 掉落归属者（0 = 任何人可拾）。
    pub of_base_object: i32,
}

/// `src/Modules/SystemModule/Data/CellObject.cs`（结构体，按值存放）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellObject {
    /// 唯一 ID（对应 C# 的 `ActorId` / 地图事件 Id）
    pub cell_obj_id: i32,
    /// Cell 类型
    pub cell_type: CellType,
    /// 精灵对象（玩家/怪物/商人）
    pub actor_object: bool,
    /// 添加时间（虚拟时钟，见 `crate::clock`）
    pub add_time: i64,
    /// 地面物品的最小身份（只读分片记账；非物品对象为 `None`）。
    pub item: Option<FloorItemMeta>,
}

/// `src/Modules/SystemModule/Data/MapCellInfo.cs`。
///
/// C# 用 `NativeList<CellObject>`（本仓库内置集合）；语义与 `Vec` 一致（`Remove` 删除首个相等元素、
/// `RemoveAt` 按下标删、`Clear` 清空），故直接用 `Vec`。
#[derive(Clone, Debug, Default)]
pub struct MapCellInfo {
    pub obj_list: Vec<CellObject>,
    pub attribute: CellAttribute,
}

impl MapCellInfo {
    pub fn new() -> Self {
        MapCellInfo {
            obj_list: Vec::new(),
            attribute: CellAttribute::Walk,
        }
    }

    /// 对应 `Count => ObjList == null ? 0 : ObjList.Count`。
    pub fn count(&self) -> usize {
        self.obj_list.len()
    }

    /// 对应 `Valid => Attribute == CellAttribute.Walk`。
    pub fn valid(&self) -> bool {
        self.attribute == CellAttribute::Walk
    }

    /// 对应 `IsAvailable => ObjList?.Count > 0`。
    pub fn is_available(&self) -> bool {
        !self.obj_list.is_empty()
    }

    pub fn add(&mut self, cell: CellObject) {
        self.obj_list.push(cell);
    }

    pub fn remove_at(&mut self, index: usize) {
        self.obj_list.remove(index);
    }

    /// 对应 `Remove(CellObject index)`：删除首个相等元素。
    pub fn remove_value(&mut self, cell: &CellObject) -> bool {
        if let Some(pos) = self.obj_list.iter().position(|c| c == cell) {
            self.obj_list.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.obj_list.clear();
    }

    pub fn set_attribute(&mut self, attribute: CellAttribute) {
        self.attribute = attribute;
    }
}

/// 一次移动请求（把 `MoveToMovingObject` 的多参数收敛成一个结构体，便于调用点自解释）。
#[derive(Clone, Copy, Debug)]
pub struct MoveRequest {
    pub from_x: i32,
    pub from_y: i32,
    pub actor_id: i32,
    pub to_x: i32,
    pub to_y: i32,
    /// 对应 C# 的 `boFlag`：true 时跳过阻挡判定
    pub ignore_block: bool,
    pub add_time: i64,
}

/// 一条地图连接点的**落地目标**（`MapRouteItem` 的 `Envir/X/Y` 三元组）。
///
/// 等价于 oracle 的 `MapRouteItem.Envir`（目标图）+ `.X`/`.Y`（目标坐标）；本仓用 `map_id` 而非
/// `IEnvirnoment` 引用（见 `crates/envir` 的 `MapRegistry`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteHop {
    pub dst_map_id: usize,
    pub dst_x: i32,
    pub dst_y: i32,
}

/// 启动灌入**一条**连接点所需的最小信息（`map_id` 由调用方按注册表下标给出）。
///
/// `crates/world` 不依赖 `crates/envir`（避免反向依赖）⇒ 用本中立结构承载
/// `mir2_envir::ResolvedRoute` 的字段，由宿主（`bins/game-svr`）做映射。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapRouteSpec {
    /// 源图 `map_id`。
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    /// `MapRouteItem.RouteId`（帧不读，仅作格子→hop 的键；唯一即可）。
    pub route_id: i32,
    pub hop: RouteHop,
}

/// 地图格子网格（`Envirnoment` 的 `CellArray` + `Width`/`Height`）。
///
/// 数组**恒非空**（至少 1 格），以便复刻 `GetCellInfo` 失败时返回 `CellArray[0]` 的行为。
#[derive(Clone, Debug)]
pub struct MapGrid {
    pub width: i32,
    pub height: i32,
    cells: Vec<MapCellInfo>,
    /// `route_id` → 落地目标。替代 oracle 的全局 `M2Share.CellObjectMgr`（帧不读该表）。
    routes: Vec<(i32, RouteHop)>,
    /// `Envirnoment.Flag.boNEEDHOLE`（`MapInfo.txt` 的 `NEEDHOLE` 段）。
    ///
    /// **仅在 `PlayObject.Walk` 的 `MapRoute` 支被读**（`PlayObject.cs:1874`：作为**目标图**的
    /// `mapRoute.Envir.Flag.boNEEDHOLE`）——本图作为 route 的落地目标时才有意义，故由宿主
    /// （`bins/game-svr`）按注册表逐图灌入；缺省 `false`（合成 golden 场景行为不变）。
    need_hole: bool,
}

impl MapGrid {
    pub fn new(width: i32, height: i32) -> Self {
        assert!(
            width > 0 && height > 0,
            "地图尺寸必须为正（C# 侧由 Map 文件给出）"
        );
        let n = (width as usize) * (height as usize);
        let mut cells = Vec::with_capacity(n);
        for _ in 0..n {
            cells.push(MapCellInfo::new());
        }
        MapGrid {
            width,
            height,
            cells,
            routes: Vec::new(),
            need_hole: false,
        }
    }

    /// 设 `Flag.boNEEDHOLE`（route 落地门的输入；宿主按注册表逐图灌入，幂等）。
    pub fn set_need_hole(&mut self, need_hole: bool) {
        self.need_hole = need_hole;
    }

    /// 读 `Flag.boNEEDHOLE`（`PlayObject.cs:1874` 作为**目标图**读）。
    #[must_use]
    pub fn need_hole(&self) -> bool {
        self.need_hole
    }

    /// 对应 `CellMatch(nX, nY) => nX >= 0 && nX < Width && nY >= 0 && nY < Height`。
    pub fn cell_match(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.width && y >= 0 && y < self.height
    }

    /// 对应 `CellValid(nX, nY)`：越界时 C# 返回 `true`（注意这个方向）。
    pub fn cell_valid(&self, x: i32, y: i32) -> bool {
        if self.cell_match(x, y) {
            self.cells[self.index(x, y)].valid()
        } else {
            true
        }
    }

    /// 只读某格属性（**不受 `Valid` 影响**）：越界返回 `None`。
    ///
    /// `get_cell_info` 对「越界或非 Valid」一律返回 `CellArray[0]`，无法读到被门/墙
    /// 掩码置为 `LowWall` 的格子 ⇒ 门/墙通行判据需要这个直读口。
    pub fn cell_attribute(&self, x: i32, y: i32) -> Option<CellAttribute> {
        if self.cell_match(x, y) {
            Some(self.cells[self.index(x, y)].attribute)
        } else {
            None
        }
    }

    /// 对应 `Envirnoment.SetMapXyFlag(nX, nY, boFlag)`（`Envirnoment.cs:1362-1369`）。
    ///
    /// **照搬 oracle 语义**：C# 的 `GetCellInfo` 对「越界**或**非 `Valid`」都返回
    /// `success=false`，而 `SetMapXyFlag` 只在 `success` 时 `SetAttribute` ⇒ 一旦某格被
    /// 置成 `LowWall`，后续 `boFlag=true` 的调用**不会**把它恢复成 `Walk`（门/墙掩码
    /// 的已知 oracle 怪行为，本片照搬，见白名单 **B-15**）。
    ///
    /// 返回 `true` = 实际改了属性（格子当前 `Valid` 且在界内）。
    pub fn set_map_xy_flag(&mut self, x: i32, y: i32, walk: bool) -> bool {
        if self.cell_match(x, y) {
            let idx = self.index(x, y);
            if self.cells[idx].valid() {
                self.cells[idx].set_attribute(if walk {
                    CellAttribute::Walk
                } else {
                    CellAttribute::LowWall
                });
                return true;
            }
        }
        false
    }

    /// 列主序：`x * Height + y`（`Envirnoment.cs`）。
    #[inline]
    fn index(&self, x: i32, y: i32) -> usize {
        (x * self.height + y) as usize
    }

    /// 对应 `GetCellInfo(nX, nY, out bool success)`。
    ///
    /// 返回 `(cell, success)`；失败时按 C# 返回**第 0 格**（不是 None）。
    pub fn get_cell_info(&self, x: i32, y: i32) -> (&MapCellInfo, bool) {
        if self.cell_match(x, y) {
            let cell = &self.cells[self.index(x, y)];
            if cell.valid() {
                return (cell, true);
            }
        }
        (&self.cells[0], false)
    }

    /// 可变版本（`AddMapObject` / `MoveToMovingObject` 需要就地改格子）。
    pub fn get_cell_info_mut(&mut self, x: i32, y: i32) -> (&mut MapCellInfo, bool) {
        if self.cell_match(x, y) {
            let idx = self.index(x, y);
            if self.cells[idx].valid() {
                return (&mut self.cells[idx], true);
            }
        }
        (&mut self.cells[0], false)
    }

    /// 对应 `Envirnoment.AddMapObject`（`Envirnoment.cs:252`）。
    ///
    /// 返回 `true` 表示格子内已追加该对象；`cell_obj_id` 由调用方给出（C# 用 `ActorId`/事件 Id）。
    /// `is_first_add` 由调用方根据实体的 `add_to_mapped` 决定（C# 在这里置位并调用 `AddObject`）。
    pub fn add_map_object(
        &mut self,
        x: i32,
        y: i32,
        cell_type: CellType,
        cell_obj_id: i32,
        add_time: i64,
    ) -> bool {
        self.add_cell_object(x, y, cell_type, cell_obj_id, None, add_time)
    }

    /// 地面物品落格（`CellType::Item`）——与 `add_map_object` 同一格子语义，额外记账
    /// [`FloorItemMeta`]（world 侧只读分片，owner §25.1 (A)）。
    pub fn add_floor_item(
        &mut self,
        x: i32,
        y: i32,
        obj_id: i32,
        meta: FloorItemMeta,
        add_time: i64,
    ) -> bool {
        self.add_cell_object(x, y, CellType::Item, obj_id, Some(meta), add_time)
    }

    /// 对应 `Envirnoment.AddMapRoute(nX, nY, mapRoute)`（`Envirnoment.cs:169-193`）：
    /// `CellMatch` 且在界内 `cell.Valid` 时，往该格 `ObjList` 追加
    /// `CellObject{CellType=MapRoute, CellObjId=route_id}`，并把 `route_id → hop` 记进本图表。
    ///
    /// 与 `add_map_object` 的差别：oracle 的 route `CellObject.ActorObject` 取**结构体默认 `false`**
    /// （`CellObject.cs` 的 `bool ActorObject` 零值）⇒ 这里不能沿用 `add_cell_object`（它硬置
    /// `actor_object = true`，会让 route 格被 `cell_free`/`move_object` 当活体读 `entities`）。
    ///
    /// **红检（变异牙）**：把 `if cell_success && cell.valid()` 支改成恒真（`if true`，即不查格子可否走）
    /// ⇒ 「墙格不落 route」的断言必红。
    pub fn add_map_route(&mut self, x: i32, y: i32, route_id: i32, hop: RouteHop) -> bool {
        // `if (!CellMatch(nX, nY)) return;` —— 注意 oracle 失败时**不**记 route 表。
        if !self.cell_match(x, y) {
            return false;
        }
        let added = {
            let (cell, cell_success) = self.get_cell_info_mut(x, y);
            if cell_success && cell.valid() {
                cell.add(CellObject {
                    cell_obj_id: route_id,
                    cell_type: CellType::MapRoute,
                    actor_object: false,
                    add_time: 0,
                    item: None,
                });
                true
            } else {
                false
            }
        };
        if added {
            // `M2Share.CellObjectMgr.Add(cellObject.CellObjId, mapRoute)`（在 Valid 支内）。
            if !self.routes.iter().any(|(id, _)| *id == route_id) {
                self.routes.push((route_id, hop));
            }
        }
        added
    }

    /// 读 `route_id` → 落地目标（`M2Share.CellObjectMgr.Get<MapRouteItem>` 的等价物）。
    #[must_use]
    pub fn route_hop(&self, route_id: i32) -> Option<&RouteHop> {
        self.routes
            .iter()
            .find_map(|(id, hop)| (*id == route_id).then_some(hop))
    }

    fn add_cell_object(
        &mut self,
        x: i32,
        y: i32,
        cell_type: CellType,
        cell_obj_id: i32,
        item: Option<FloorItemMeta>,
        add_time: i64,
    ) -> bool {
        if !self.cell_match(x, y) {
            return false;
        }
        let (cell, success) = self.get_cell_info_mut(x, y);
        if success && cell.valid() {
            cell.add(CellObject {
                cell_obj_id,
                cell_type,
                actor_object: true,
                add_time,
                item,
            });
            return true;
        }
        false
    }

    /// 对应 `Envirnoment.MoveToMovingObject(nCx, nCy, cert, nX, nY, boFlag)` 的
    /// **格子簿记部分**（不含 `Walk` 回调与坐标更新，那在 `crate::world`）。
    ///
    /// 语义：目标格有任意"非 Ghost/非 Death/非 FixedHideMode/非 ObMode"的活体 ⇒ 不可移动
    ///（门另见 `CastleDoor.HoldPlace`，本骨架不含门对象，保留分支位置注释）。
    /// 可移动时把 `actor_id` 从源格摘除、追加到目标格。
    ///
    /// 返回 `true` = 格子簿记完成（调用方再更新坐标）。
    pub fn move_object(&mut self, req: MoveRequest, is_blocking: impl Fn(i32) -> bool) -> bool {
        let MoveRequest {
            from_x,
            from_y,
            actor_id,
            to_x,
            to_y,
            ignore_block,
            add_time,
        } = req;
        if !self.cell_match(to_x, to_y) {
            return false;
        }
        let mut can_move = true;
        {
            let (cell, cell_success) = self.get_cell_info(to_x, to_y);
            if !ignore_block && cell_success {
                if cell.valid() && cell.is_available() {
                    for obj in &cell.obj_list {
                        if obj.actor_object && is_blocking(obj.cell_obj_id) {
                            // C# 此处对 CastleDoor 还有 HoldPlace 分支，随后一律 can_move=false; break
                            can_move = false;
                            break;
                        }
                    }
                } else {
                    can_move = true;
                }
            }
        }
        if !can_move {
            return false;
        }
        let (cell, _) = self.get_cell_info(to_x, to_y);
        if !cell.valid() {
            return false;
        }
        // 从源格摘除（C# 按 CellObjId 匹配，命中即 RemoveAt 并处理 Count 归零的 Clear 分支）
        {
            let (old, old_success) = self.get_cell_info_mut(from_x, from_y);
            if old_success && old.is_available() {
                if let Some(pos) = old
                    .obj_list
                    .iter()
                    .position(|o| o.cell_obj_id == actor_id && o.actor_object)
                {
                    old.remove_at(pos);
                    if old.count() == 0 {
                        old.clear();
                    }
                }
            }
        }
        self.add_map_object(to_x, to_y, CellType::Play, actor_id, add_time)
    }

    /// 只读遍历辅助（AOI 扫描需要按 X 外/Y 内访问格子）。
    pub fn cell(&self, x: i32, y: i32) -> &MapCellInfo {
        self.get_cell_info(x, y).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_major_indexing() {
        // 列主序：x*Height + y。取 2x3 地图，(1,0) 与 (0,1) 必须落在不同格。
        let mut g = MapGrid::new(2, 3);
        g.add_map_object(1, 0, CellType::Play, 7, 0);
        assert_eq!(g.cell(1, 0).count(), 1);
        assert_eq!(g.cell(0, 1).count(), 0);
        assert_eq!(g.cell(1, 1).count(), 0);
    }

    #[test]
    fn get_cell_info_returns_cell0_on_failure() {
        // 复刻 C#：越界/不可走时返回 CellArray[0]（这是"看起来像 bug 的行为"，必须一致）
        let mut g = MapGrid::new(4, 4);
        g.get_cell_info_mut(0, 0).0.add(CellObject {
            cell_obj_id: 999,
            cell_type: CellType::Play,
            actor_object: true,
            add_time: 0,
            item: None,
        });
        let (cell, success) = g.get_cell_info(99, 99);
        assert!(!success);
        assert_eq!(cell.count(), 1, "失败时应返回第 0 格（含其对象）");
        assert_eq!(cell.obj_list[0].cell_obj_id, 999);
    }

    #[test]
    fn move_blocked_by_any_actor_in_target_cell() {
        let mut g = MapGrid::new(8, 8);
        g.add_map_object(2, 2, CellType::Play, 1, 0);
        g.add_map_object(3, 2, CellType::Play, 2, 0);
        let blocked = |_id: i32| true;
        let moved = g.move_object(
            MoveRequest {
                from_x: 2,
                from_y: 2,
                actor_id: 1,
                to_x: 3,
                to_y: 2,
                ignore_block: false,
                add_time: 0,
            },
            blocked,
        );
        assert!(!moved, "目标格有活体 ⇒ 不可移动");
        assert_eq!(g.cell(2, 2).count(), 1);
        assert_eq!(g.cell(3, 2).count(), 1);
    }

    #[test]
    fn move_ignores_block_when_flag_set() {
        let mut g = MapGrid::new(8, 8);
        g.add_map_object(2, 2, CellType::Play, 1, 0);
        g.add_map_object(3, 2, CellType::Play, 2, 0);
        let blocked = |_id: i32| true;
        let moved = g.move_object(
            MoveRequest {
                from_x: 2,
                from_y: 2,
                actor_id: 1,
                to_x: 3,
                to_y: 2,
                ignore_block: true,
                add_time: 0,
            },
            blocked,
        );
        assert!(moved, "boFlag=true 时跳过阻挡判定");
        assert_eq!(g.cell(3, 2).count(), 2);
    }

    /// **L1 TransMap 世界侧落格**：`AddMapRoute` 的 `cell.valid` 门照抄——
    /// 墙格（`LowWall`）与越界格**不落** route，可走格落 `CellType::MapRoute` 且记进 route 表。
    ///
    /// 红检（变异牙）：去掉 `add_map_route` 的 `success && cell.valid()` 门 ⇒ 墙格/越界断言必红。
    #[test]
    fn add_map_route_requires_valid_in_bounds_cell() {
        let mut g = MapGrid::new(8, 8);
        g.set_map_xy_flag(1, 1, false); // → LowWall（不可走）
        let hop = RouteHop {
            dst_map_id: 0,
            dst_x: 7,
            dst_y: 9,
        };
        assert!(!g.add_map_route(1, 1, 7, hop), "墙格不落 route");
        assert!(!g.add_map_route(-1, 0, 8, hop), "越界不落 route");
        assert!(g.add_map_route(2, 2, 9, hop), "可走格落 route");
        assert_eq!(g.cell(2, 2).count(), 1);
        assert_eq!(g.cell(2, 2).obj_list[0].cell_type, CellType::MapRoute);
        assert_eq!(g.cell(2, 2).obj_list[0].cell_obj_id, 9);
        assert!(
            !g.cell(2, 2).obj_list[0].actor_object,
            "route CellObject 不是活体（oracle ActorObject 默认 false）"
        );
        assert_eq!(g.route_hop(9), Some(&hop));
        assert_eq!(g.route_hop(7), None, "被丢弃的 route 不入表");
    }
}
