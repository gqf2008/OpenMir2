//! 主地图门（MapDoor）数据接线 —— `crates/envir` 已从 `.map` 解出的门表 → `crates/world` 的 `MapGrid`。
//!
//! 移植源与口径（逐条照 oracle，别"顺手修"）：
//! - `Envirnoment.LoadMapData`（`src/M2Server/Maps/Envirnoment.cs:934-991`）在加载 `.map` 时就把每格
//!   `btDoorIndex`（bit7 置位且低 7 位 > 0）建成 `MapDoor` 并 `DoorList.Add`；同图内
//!   `|dx| <= 10 && |dy| <= 10 && n08 相同` 的门**共享同一个 `DoorStatus` 实例**。解析已在
//!   `crates/envir/src/mapfile.rs` 完成（真图 `0.map` = 79 门 / 21 status）；本模块只把
//!   `MapFile::door_tables()` 的逐字段镜像灌进 `world.maps[map_id]`（两侧结构逐字段同型，
//!   `crates/world` 不依赖 `crates/envir`，故在 world 侧镜像定义）。
//! - `MapManager.LoadMapDoor()`（`src/GameSrv/Maps/MapManager.cs:182-189`，调用点 `GameApp.cs:226`）
//!   只是把已解出的门 `AddDoorToMap`，**不读独立内容件**（`Envir/MapDoor.txt` 不存在）。
//! - **不实现真开/关门**：oracle `CM_OPENDOOR` 是结构性 no-op（`PlayObject.Operate.cs:363-376` 的
//!   `if (Envir.GetDoor(nX, nY, ref door)) { return; }` 守卫写反 + `WorldServer.OpenDoor` 要求有门），
//!   ⇒ 门恒 `Opened=false`、`ProcessMapDoor`/`SendDoorStatus` 不可达。本模块只搬「加载态」，**不提供**
//!   `open_door`/`process_map_door`（照搬 no-op；见 `mir2-rs/tests/parity/whitelist.md` T-65）。
//! - `ArroundDoorOpened` 的消费点（`PlayObject.cs:1872` 的 `MapRoute` 支）在
//!   `crates/world/src/world.rs::route_enter`；本模块只让门表/状态可读，不参与路由判定。

use mir2_envir::MapRegistry;
use mir2_world::World;

/// 把注册表每图的门表灌进 `world.maps[map_id]`，返回门总数（日志用）。
///
/// 等价 oracle `Maps.LoadMapInfo` 之后由 `MapManager.LoadMapDoor()` 逐图 `AddDoorToMap`。
/// 缺省（无注册表 / 无门）⇒ 每图空表，`MapGrid::arround_door_opened` 恒 `true`（改动前行为不变）。
pub fn apply_map_doors(reg: &MapRegistry, world: &mut World) -> usize {
    let mut total = 0usize;
    for (map_id, f) in reg.maps().iter().enumerate() {
        let (doors, status) = f.door_tables();
        total += doors.len();
        world.set_map_doors(map_id, doors, status);
    }
    total
}
