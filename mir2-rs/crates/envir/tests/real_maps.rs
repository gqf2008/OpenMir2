//! 真图判据（卡 `openmir2-m2-map-loading` §3）——**刻意 `#[ignore]`，不进常驻测试**：
//! 本仓有教训（`crates/world-bevy` 那批），常驻测试读仓外 `E:\MirServer` 会在 worktree 恒红。
//!
//! 由审查者/owner 在真机上显式跑（PowerShell）：
//! ```text
//! $env:MIR2_MAP_DIR='E:\MirServer\M2GameSvr\Map'
//! cargo test -p mir2-envir --test real_maps -- --ignored --nocapture
//! ```
//! 部署件参照读数（本仓 owner 侧夹具）：
//! ```text
//! dotnet E:\tmp\openmir2-gamegate-imp\mapgen\bin\Release\net9.0\mapgen.dll probe E:\MirServer\M2GameSvr\Map\0.map
//! ```
//! 环境变量缺失时**不静默 skip**：直接报错，提醒这是需要真机数据的门禁。

use std::path::PathBuf;

use mir2_envir::{CellAttribute, MapFile, MapLoader};

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
        "真图目录不存在：{}（用 MIR2_MAP_DIR 指定，例如 E:\\MirServer\\M2GameSvr\\Map）",
        dir.display()
    );
    let loader = MapLoader::new(&dir, "Map", "Envir");
    loader
        .load_path(&dir.join(name))
        .unwrap_or_else(|e| panic!("加载 {name} 失败: {e}"))
}

fn report(name: &str, m: &MapFile) {
    eprintln!(
        "{}: W={} H={} high={} low={} walk={} doors={} door_status={} first_wall={:?}",
        name,
        m.width,
        m.height,
        m.high_count(),
        m.low_count(),
        m.walk_count(),
        m.doors.len(),
        m.door_status.len(),
        m.first_wall_index,
    );
}

#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_MAP_DIR 显式跑"]
fn map_0_matches_deployment() {
    let m = probe("0.map");
    report("0.map", &m);
    assert_eq!((m.width, m.height), (700, 700));
    assert_eq!(m.high_count(), 162711);
    assert_eq!(m.low_count(), 109);
    assert_eq!(m.walk_count(), 327180);
    // high+low+walk == 700*700。
    assert_eq!(m.high_count() + m.low_count() + m.walk_count(), 490_000);
    // 门：部署件 probe 报 DoorList count=79，21 个不同 StatusRef。
    assert_eq!(m.doors.len(), 79);
    assert_eq!(m.door_status.len(), 21);
    // 真图 ObjList 全 NULL ⇒ 首个墙格在 k=0。
    assert_eq!(m.first_wall_index, Some(0));
    assert!(!m.obj_list_allocated_at(0));
}

#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_MAP_DIR 显式跑"]
fn map_0129_matches_deployment() {
    let m = probe("0129.map");
    report("0129.map", &m);
    assert_eq!((m.width, m.height), (7, 10));
    assert_eq!(m.high_count(), 59);
    assert_eq!(m.low_count(), 0);
    assert_eq!(m.walk_count(), 11);
    assert_eq!(m.doors.len(), 0);
    assert_eq!(m.first_wall_index, Some(0));
}

#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_MAP_DIR 显式跑"]
fn map_0100_matches_deployment() {
    let m = probe("0100.map");
    report("0100.map", &m);
    assert_eq!((m.width, m.height), (15, 18));
    assert_eq!(m.high_count(), 0);
    assert_eq!(m.low_count(), 223);
    assert_eq!(m.walk_count(), 47);
    assert_eq!(m.doors.len(), 0);
    assert_eq!(m.first_wall_index, Some(0));
}

/// 单点抽查：真图 (0,0) 的属性值必须与文件 bit15 一致（防止整表偏移错位）。
#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_MAP_DIR 显式跑"]
fn map_0_cell_zero_attribute_is_consistent() {
    let m = probe("0.map");
    let c = m.cells[0];
    let expected = if c.low_wall() {
        CellAttribute::LowWall
    } else if c.high_wall() {
        CellAttribute::HighWall
    } else {
        CellAttribute::Walk
    };
    assert_eq!(m.attribute(0, 0), expected);
}
