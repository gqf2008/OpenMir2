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
//!
//! `deployment_wall_census`（卡 `openmir2-mapcell-consumer-teeth`）另读 `Envir/Npcs.txt` /
//! `Envir/Merchant.txt` 做落墙格普查；数字登记在 `bins/game-svr/src/mapwire.rs`。跑法同上（名称过滤可选）：
//! `cargo test -p mir2-envir --test real_maps -- --ignored --nocapture`

use std::path::PathBuf;

use mir2_envir::{CellAttribute, MapFile, MapLoader, MapRegistry};

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

/// **卡 `openmir2-mapcell-consumer-teeth` 真机普查**（依赖 mapcell 装载）：为
/// `bins/game-svr/src/mapwire.rs` 模块头登记的两个真机数字提供可复跑命令——
/// ① 各**注册**图 `(0,0)`（= `MapGrid` 的 `cells[0]`）属性；② `Envir/Npcs.txt` /
/// `Envir/Merchant.txt` 里**配置格落在墙格**的条数（oracle `BaseObject.Initialize` 的
/// `CanWalk(CurrX,CurrY,true)` 失败 ⇒ `MerchantInitialize`/`NpCinitialize` 静默 `RemoveAt`）。
///
/// 数字本身不硬编码断言（本文件是**普查工具**；判据在审查侧）——只做「部署件/路径对」的结构检查。
/// 2026-10-17 实测（`M2GameSvr`）：注册图 **322**（`(0,0)` 为墙 **317**、为 `Walk` **5**：
/// `T2361`/`E702`/`D10031`/`4`/`H009`）；`Npcs.txt` **3** 条 / 落墙 **0**；`Merchant.txt` **131** 条 / 落墙 **0**。
#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_MAP_DIR 显式跑"]
fn deployment_wall_census() {
    let map_dir = map_dir();
    assert!(map_dir.is_dir(), "真图目录不存在：{}", map_dir.display());
    // `MapLoader` 的 base = `M2GameSvr`（`Map` 的父目录），`Envir` 在其下。
    let base = map_dir
        .parent()
        .expect("Map 目录应有父目录（M2GameSvr 根）")
        .to_path_buf();
    let loader = MapLoader::new(base.clone(), "Map", "Envir");
    let entries = loader
        .load_map_info_entries()
        .expect("读 Envir/MapInfo.txt 失败");
    let reg = MapRegistry::from_entries(loader, entries);

    // ① 各注册图 (0,0) 属性（`index(0,0)==0` ⇒ 就是 `cells[0]`）。
    let cell0_walk: Vec<(usize, String)> = reg
        .maps()
        .iter()
        .enumerate()
        .filter(|(_, m)| m.attribute(0, 0) == CellAttribute::Walk)
        .map(|(i, _)| (i, reg.entries()[i].map_file_name.clone()))
        .collect();
    eprintln!(
        "[census] maps_registered={} cell0_walk={} cell0_nonwalk={} cell0_walk_list={cell0_walk:?}",
        reg.maps().len(),
        cell0_walk.len(),
        reg.maps().len() - cell0_walk.len()
    );

    // ② Npcs.txt / Merchant.txt 落墙格。
    let npc_bytes = std::fs::read(base.join("Envir/Npcs.txt")).expect("读 Envir/Npcs.txt 失败");
    let npcs = mir2_envir::npcs::load_npcs(&mir2_envir::decode_text(&npc_bytes))
        .expect("解析 Npcs.txt 失败");
    let mut npc_wall: Vec<String> = Vec::new();
    for n in &npcs {
        let a = reg
            .id_of(&n.map_name)
            .map(|id| reg.maps()[id].attribute(i32::from(n.x), i32::from(n.y)));
        if a != Some(CellAttribute::Walk) {
            npc_wall.push(format!(
                "{} @{} ({},{}) => {a:?}",
                n.chr_name, n.map_name, n.x, n.y
            ));
        }
    }
    let m_bytes =
        std::fs::read(base.join("Envir/MerChant.txt")).expect("读 Envir/MerChant.txt 失败");
    let m_text = mir2_envir::decode_text(&m_bytes);
    let mut m_total = 0usize;
    let mut m_wall: Vec<String> = Vec::new();
    for line in m_text.lines() {
        if let Some(r) = mir2_envir::merchant::parse_merchant_line(line) {
            m_total += 1;
            let a = reg
                .id_of(&r.map_name)
                .map(|id| reg.maps()[id].attribute(i32::from(r.x), i32::from(r.y)));
            if a != Some(CellAttribute::Walk) {
                m_wall.push(format!(
                    "{} @{} ({},{}) => {a:?}",
                    r.chr_name, r.map_name, r.x, r.y
                ));
            }
        }
    }
    eprintln!("[census] npcs_total={} npcs_wall={npc_wall:?}", npcs.len());
    eprintln!("[census] merchant_total={m_total} merchant_wall={m_wall:?}");

    assert!(!reg.maps().is_empty(), "注册表为空 ⇒ 部署件/路径不对");
    assert!(!npcs.is_empty(), "Npcs.txt 解析为 0 条 ⇒ 部署件/路径不对");
    assert!(m_total > 0, "Merchant.txt 解析为 0 条 ⇒ 部署件/路径不对");
}
