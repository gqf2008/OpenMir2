//! 门数据装载判据（卡 `openmir2-mapdoor-derive-plumbing`）：`crates/envir` 从 `.map` 解出的门表
//! 经 `MapFile::door_tables()`（生产宿主 `game_svr::mapdoor::apply_map_doors` 的同一映射）进
//! `crates/world` 的 `MapGrid`。
//!
//! 判据分级：本文件是**构造级**证据（L 级可离线收口）；真机窗口项 W-R1 仍按 `whitelist.md` T-65
//! （门相邻 route 格的换图被门控挡住）另跑。
//!
//! 真图用例**刻意 `#[ignore]`**（读仓外 `E:\MirServer`，常驻会在他机恒红；与
//! `crates/envir/tests/real_maps.rs` 同惯例）。显式跑：
//! ```text
//! $env:MIR2_MAP_DIR='E:\MirServer\M2GameSvr\Map'
//! cargo test -p game-svr --test mapdoor_derive -- --ignored --nocapture
//! ```
//!
//! 红检（变异牙，见报告）：① `door_tables` 返回空门表 ⇒ 计数断言必红；② 映射里 `x`/`y` 互换
//! ⇒ 逐条坐标断言必红；③ 丢掉 `door_status`（返回空表）⇒ 状态计数/`status_id` 解析断言必红。

use std::path::PathBuf;

use mir2_envir::{
    DoorStatus as EnvirDoorStatus, MapCell, MapDoor as EnvirMapDoor, MapFile, MapLoader,
};
use mir2_world::map::MapGrid;

fn envir_door(x: i16, y: i16, n08: u8, status_id: usize) -> EnvirMapDoor {
    EnvirMapDoor {
        x,
        y,
        n08,
        door_id: 0,
        status_id,
    }
}

/// 合成一张只带门表的 `MapFile`（属性测试不关心格数据；`to_grid` 对空格返回 `Walk`）。
fn map_file_with_doors(
    width: i32,
    height: i32,
    doors: Vec<EnvirMapDoor>,
    door_status: Vec<EnvirDoorStatus>,
) -> MapFile {
    MapFile {
        map_name: "synthetic".to_string(),
        width,
        height,
        cells: vec![MapCell::default(); (width * height) as usize],
        door_status,
        doors,
        points: Vec::new(),
        map_file: PathBuf::new(),
        point_file: None,
        first_wall_index: None,
    }
}

/// **构造级判据 1**：逐字段搬运（坐标/`n08`/`door_id`/`status_id` 保序保身份）+ 状态可读。
///
/// 红检：把 `door_tables` 的 `x: d.x` 改成 `x: d.y`（坐标错）⇒ 本测必红。
#[test]
fn door_tables_moves_fields_and_keeps_status_identity() {
    let envir_doors = vec![
        envir_door(3, 4, 5, 0),
        envir_door(4, 4, 5, 0), // 与上一条共享 status 0（同组门）
        envir_door(8, 8, 6, 1),
        envir_door(1, 1, 7, 2),
    ];
    let envir_status = vec![
        EnvirDoorStatus {
            opened: false,
            open_tick: 0,
        },
        EnvirDoorStatus {
            opened: true,
            open_tick: 12_345,
        },
        EnvirDoorStatus::default(),
    ];
    let f = map_file_with_doors(12, 12, envir_doors.clone(), envir_status.clone());

    let (doors, status) = f.door_tables();
    assert_eq!(doors.len(), 4, "门条数 = envir 解析条数");
    assert_eq!(status.len(), 3, "DoorStatus 实例条数（同组共享）");
    for (got, want) in doors.iter().zip(&envir_doors) {
        assert_eq!(
            (got.x, got.y, got.n08, got.door_id, got.status_id),
            (want.x, want.y, want.n08, want.door_id, want.status_id),
            "逐字段同型 + 保序 + 保 status_id 身份"
        );
    }
    for (got, want) in status.iter().zip(&envir_status) {
        assert_eq!(
            (got.opened, got.open_tick),
            (want.opened, want.open_tick),
            "Opened/open_tick 逐字段搬运（门状态可读）"
        );
    }

    // 灌进 `MapGrid` 后状态仍按 `status_id` 解析：同组两条门读到同一实例。
    let mut g = MapGrid::new(12, 12);
    g.set_doors(doors, status);
    assert_eq!(g.get_door(3, 4).unwrap().status_id, 0);
    assert_eq!(g.get_door(4, 4).unwrap().status_id, 0);
    assert_eq!(g.get_door(8, 8).unwrap().status_id, 1);
    assert_eq!(g.get_door(1, 1).unwrap().status_id, 2);
    // 门未开 ⇒ ±1 邻域挡住；门开 ⇒ 放行；距离 2 不影响。
    assert!(!g.arround_door_opened(3, 3), "未开门 (±1) 挡路");
    assert!(g.arround_door_opened(8, 9), "已开门放行");
    assert!(g.arround_door_opened(1, 3), "距离 2 的门不影响");
}

/// **构造级判据 2（阴性对照）**：无门地图 ⇒ `MapGrid` 门表为空，`arround_door_opened` 恒 `true`。
///
/// 红检：把 `set_doors` 的门表默认塞一条假门 ⇒ 本测必红。
#[test]
fn no_doors_gives_empty_table() {
    let f = map_file_with_doors(5, 5, Vec::new(), Vec::new());
    let (doors, status) = f.door_tables();
    assert!(doors.is_empty());
    assert!(status.is_empty());
    let mut g = MapGrid::new(5, 5);
    g.set_doors(doors, status);
    assert!(g.doors().is_empty());
    assert_eq!(g.door_status_len(), 0);
    for x in 0..5 {
        for y in 0..5 {
            assert!(g.arround_door_opened(x, y), "无门图 ⇒ 恒 true");
            assert_eq!(g.get_door(x, y), None);
        }
    }
}

/// `MapFile::to_grid()` 也带上门表（消除「`to_grid` 只拷属性、丢弃 doors」的旧缺口）。
#[test]
fn to_grid_carries_door_table() {
    let doors = vec![envir_door(2, 2, 9, 0)];
    let f = map_file_with_doors(
        6,
        6,
        doors,
        vec![EnvirDoorStatus {
            opened: false,
            open_tick: 0,
        }],
    );
    let g = f.to_grid();
    assert_eq!(g.doors().len(), 1);
    assert_eq!(g.door_status_len(), 1);
    assert_eq!(g.get_door(2, 2).map(|d| d.n08), Some(9));
    assert!(!g.arround_door_opened(2, 2), "加载态门恒未开");
}

fn map_dir() -> PathBuf {
    std::env::var("MIR2_MAP_DIR").map_or_else(
        |_| PathBuf::from(r"E:\MirServer\M2GameSvr\Map"),
        PathBuf::from,
    )
}

fn probe(name: &str) -> MapFile {
    let dir = map_dir();
    assert!(
        dir.is_dir(),
        "真图目录不存在：{}（用 MIR2_MAP_DIR 指定）",
        dir.display()
    );
    MapLoader::new(&dir, "Map", "Envir")
        .load_path(&dir.join(name))
        .unwrap_or_else(|e| panic!("加载 {name} 失败: {e}"))
}

/// **真图判据（79 门 / 21 状态）**：装载 `0.map`（700×700）→ `MapGrid` 门表与 envir 解析结果
/// 逐条一致（数量 / 坐标 / `n08` / `status_id` / 状态值）。
#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_MAP_DIR 显式跑"]
fn real_map_0_doors_match_envir() {
    let f = probe("0.map");
    assert_eq!((f.width, f.height), (700, 700), "真图尺寸");
    assert_eq!(f.doors.len(), 79, "envir 解析：0.map 门 79 条");
    assert_eq!(
        f.door_status.len(),
        21,
        "envir 解析：0.map DoorStatus 21 个"
    );

    let (doors, status) = f.door_tables();
    let mut g = MapGrid::new(f.width, f.height);
    g.set_doors(doors, status);

    // ① 数量。
    assert_eq!(g.doors().len(), 79, "MapGrid 门表条数 = 79");
    assert_eq!(g.door_status_len(), 21, "MapGrid 状态实例数 = 21");
    // ② 逐条坐标/字段（保序）。
    for (i, (got, want)) in g.doors().iter().zip(&f.doors).enumerate() {
        assert_eq!(
            (
                i32::from(got.x),
                i32::from(got.y),
                got.n08,
                got.door_id,
                got.status_id
            ),
            (
                i32::from(want.x),
                i32::from(want.y),
                want.n08,
                want.door_id,
                want.status_id
            ),
            "第 {i} 条门与 envir 解析不一致"
        );
    }
    // 坐标抽查（真图实物）：首门 (106,315)/n08=55/status 0，`(265,629)` 是 status 1 组，
    // 末门 (630,645)/n08=52/status 20。
    assert_eq!(
        g.get_door(106, 315).map(|d| (d.n08, d.status_id)),
        Some((55, 0)),
        "首门坐标/组号"
    );
    assert_eq!(
        g.get_door(265, 629).map(|d| (d.n08, d.status_id)),
        Some((62, 1)),
        "第二组首门坐标/组号"
    );
    assert_eq!(
        g.get_door(630, 645).map(|d| (d.n08, d.status_id)),
        Some((52, 20)),
        "末门坐标/组号"
    );
    assert_eq!(
        g.get_door(630, 644).map(|d| d.status_id),
        Some(20),
        "同组共享 status 20（(629,644)/(630,644)/(630,645) 三条同组）"
    );
    // ③ 状态可读：每条门的 status_id 都能解析；生产不变式 = 门恒未开（`CM_OPENDOOR` no-op）。
    for d in g.doors() {
        assert!(
            g.door_status(d.status_id).is_some(),
            "status_id {} 越界",
            d.status_id
        );
    }
    assert!(
        (0..g.door_status_len()).all(|i| g
            .door_status(i)
            .is_some_and(|s| !s.opened && s.open_tick == 0)),
        "门恒 Opened=false / OpenTick=0（oracle CM_OPENDOOR 结构性 no-op）"
    );
    // 门未开 ⇒ 门格自身与 ±1 邻域被 `ArroundDoorOpened` 挡住（唯一活耦合）。
    assert!(!g.arround_door_opened(106, 315), "门格自身（未开）");
    assert!(!g.arround_door_opened(107, 316), "±1 邻域（未开）");
}

/// **阴性对照（真图）**：无门真图 ⇒ 空表。
#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_MAP_DIR 显式跑"]
fn real_maps_without_doors_give_empty_table() {
    for name in ["0129.map", "0100.map"] {
        let f = probe(name);
        assert_eq!(f.doors.len(), 0, "{name} 无门");
        let (doors, status) = f.door_tables();
        let mut g = MapGrid::new(f.width, f.height);
        g.set_doors(doors, status);
        assert!(g.doors().is_empty(), "{name} ⇒ MapGrid 空门表");
        assert_eq!(g.door_status_len(), 0, "{name} ⇒ 空状态表");
        assert!(g.arround_door_opened(0, 0), "{name} ⇒ 无门恒放行");
    }
}
