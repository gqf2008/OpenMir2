//! Lane B 判据（`mir2-rs/tests/parity/m3/design-castle-entities-20261020.md` §6）。
//!
//! 每条用例的「牙」（改坏生产路径必红）写在函数文档里；红检记录见提交报告。

use mir2_data::models::MonsterInfo;
use mir2_envir::castle::{parse_sabuk_w_bytes, SabukWConfig};
use mir2_envir::{MapLoader, MapRegistry};
use mir2_shared::rng::{DotNetRandom, RandomNumber};
use mir2_world::castle::{CastleDefenseSpecs, CastleSiegeConfig, DefenseRole, DefenseUnitSpec};
use mir2_world::mongen::MonGenConfig;

use crate::downlink;
use crate::mongen::{MonGenHost, MonsterTable};

use super::{initialize_castle, specs_from_sabuk};

/// 部署件 `Castle/0/SabukW.txt` 的 GBK 原始字节（hex 证据夹具；与
/// `crates/envir/src/castle.rs` 的 `real_deployed_sabuk_w` 同一份）。
fn real_deployed_sabuk_w() -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/parity/m3/evidence-m3-castle-file-load-20261013/fixtures/SabukW-0.hex");
    let hex = std::fs::read_to_string(&path).expect("证据夹具 fixtures/SabukW-0.hex");
    let digits: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    assert_eq!(digits.len() % 2, 0, "hex 夹具长度必须为偶");
    digits
        .chunks(2)
        .map(|c| {
            let hi = (c[0] as char).to_digit(16).expect("hex 高位") as u8;
            let lo = (c[1] as char).to_digit(16).expect("hex 低位") as u8;
            (hi << 4) | lo
        })
        .collect()
}

/// **C1**：`Castle/0/SabukW.txt` → `CastleDefenseSpecs` 逐字段。
///
/// 牙：① 把 `GUARD_ARRAY_LEN` 改成 4 ⇒ `guards.len()==12` 红；② 把中/右墙名按语义对调
/// ⇒ 墙名断言红；③ 把缺键守卫 HP 默认改 0 ⇒ `guards[4..].hp==2000` 红。
#[test]
fn castle_sabukw_specs_from_deployed_file() {
    let cfg = parse_sabuk_w_bytes(&real_deployed_sabuk_w());
    let specs = specs_from_sabuk(&cfg);

    assert_eq!(specs.map_name, "3");
    assert_eq!(specs.palace_map, "0150");
    assert_eq!(specs.secret_map, "D701");
    assert_eq!(
        (
            specs.main_door.name.as_str(),
            specs.main_door.x,
            specs.main_door.y,
            specs.main_door.hp
        ),
        ("沙巴克城门", 672, 330, 9999)
    );
    assert!(!specs.main_door.status, "MainDoorOpen=0");
    // 中/右墙名互换——照抄不改（硬约束③）。
    assert_eq!(
        (
            specs.left_wall.name.as_str(),
            specs.left_wall.x,
            specs.left_wall.y,
            specs.left_wall.hp
        ),
        ("沙巴克左城墙", 624, 278, 4999)
    );
    assert_eq!(
        (specs.center_wall.name.as_str(), specs.center_wall.hp),
        ("沙巴克右城墙", 4999)
    );
    assert_eq!(
        (specs.right_wall.name.as_str(), specs.right_wall.hp),
        ("沙巴克中城墙", 4999)
    );
    assert_eq!(specs.archers.len(), 12);
    assert_eq!(
        specs.guards.len(),
        12,
        "Guards 数组长度 12（不是雇佣上限 4）"
    );
    assert_eq!(
        (
            specs.archers[0].name.as_str(),
            specs.archers[0].x,
            specs.archers[0].y,
            specs.archers[0].hp
        ),
        ("弓箭手", 662, 333, 2000)
    );
    assert!(specs.archers.iter().all(|a| a.hp == 2000));
    assert!(specs.guards[..4].iter().all(|g| g.hp == 0));
    assert!(specs.guards[4..]
        .iter()
        .all(|g| { g.hp == 2000 && g.x == 0 && g.y == 0 && g.name == "守卫" }));
}

/// 小地图夹具（20×20 全平地；`map_id = 0`，逻辑名 `"3"`）。
struct Fixture {
    base: std::path::PathBuf,
    registry: MapRegistry,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn fixture(map_name: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "gs-castle-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let (w, h) = (20_i16, 20_i16);
    let mut bytes = vec![0u8; mir2_envir::HEADER_BYTES];
    bytes[0..2].copy_from_slice(&w.to_le_bytes());
    bytes[2..4].copy_from_slice(&h.to_le_bytes());
    bytes.resize(
        mir2_envir::HEADER_BYTES
            + usize::try_from(w).unwrap() * usize::try_from(h).unwrap() * mir2_envir::CELL_BYTES,
        0,
    );
    std::fs::create_dir_all(base.join("Map")).unwrap();
    std::fs::write(base.join("Map").join("0.map"), &bytes).unwrap();
    let loader = MapLoader::new(base.clone(), "Map", "Envir");
    // 皇宫/密道也注册（oracle 部署件三图都在）⇒ `FindMap` 成功后只剩城堡块自身的告警。
    let registry = MapRegistry::from_entries(
        loader,
        vec![
            mir2_envir::MapInfoEntry {
                map_name: map_name.to_string(),
                map_file_name: "0".to_string(),
                desc: String::new(),
                server_index: 0,
                flag: mir2_envir::MapInfoFlag::default(),
            },
            mir2_envir::MapInfoEntry {
                map_name: "0150".to_string(),
                map_file_name: "0".to_string(),
                desc: String::new(),
                server_index: 0,
                flag: mir2_envir::MapInfoFlag::default(),
            },
            mir2_envir::MapInfoEntry {
                map_name: "D701".to_string(),
                map_file_name: "0".to_string(),
                desc: String::new(),
                server_index: 0,
                flag: mir2_envir::MapInfoFlag::default(),
            },
        ],
    );
    assert_eq!(registry.id_of(map_name), Some(0));
    Fixture { base, registry }
}

fn unit(name: &str, x: i16, y: i16, hp: u16) -> DefenseUnitSpec {
    DefenseUnitSpec {
        name: name.to_string(),
        x,
        y,
        status: false,
        hp,
    }
}

/// 合成部署口径（`Castle/Castle/SabukW.txt`）：默认名 + `MainDoorOpen=1` + 弓手/守卫 HP=0。
fn default_specs() -> CastleDefenseSpecs {
    CastleDefenseSpecs {
        map_name: "3".to_string(),
        palace_map: "0150".to_string(),
        secret_map: "D701".to_string(),
        main_door: DefenseUnitSpec {
            name: "MainDoor".to_string(),
            x: 672,
            y: 330,
            status: true,
            hp: 2000,
        },
        left_wall: unit("LeftWall", 624, 278, 2000),
        center_wall: unit("CenterWall", 627, 278, 2000),
        right_wall: unit("RightWall", 634, 271, 2000),
        archers: (0..12).map(|_| unit("弓箭手", 0, 0, 0)).collect(),
        guards: (0..12).map(|_| unit("守卫", 0, 0, 0)).collect(),
    }
}

/// `Castle/0` 夹具口径的**小坐标**版本（RNG/绑定判据用；怪物名与 HP 保持真值）。
fn small_fixture_specs() -> CastleDefenseSpecs {
    CastleDefenseSpecs {
        map_name: "3".to_string(),
        palace_map: "0150".to_string(),
        secret_map: "D701".to_string(),
        main_door: unit("沙巴克城门", 5, 5, 9999),
        left_wall: unit("沙巴克左城墙", 2, 2, 4999),
        // 中/右墙按部署件互换。
        center_wall: unit("沙巴克右城墙", 3, 3, 4999),
        right_wall: unit("沙巴克中城墙", 4, 4, 4999),
        archers: (0..12)
            .map(|i| unit("弓箭手", 2 + i as i16, 10, 2000))
            .collect(),
        guards: (0..12)
            .map(|i| unit("守卫", 0, 0, if i < 4 { 0 } else { 2000 }))
            .collect(),
    }
}

fn row(name: &str, race: u8, hp: u16) -> MonsterInfo {
    MonsterInfo {
        name: name.to_string(),
        race,
        hp,
        ..MonsterInfo::default()
    }
}

fn castle_table() -> MonsterTable {
    MonsterTable::new(vec![
        row("沙巴克城门", 110, 10000),
        row("沙巴克左城墙", 111, 5000),
        row("沙巴克右城墙", 111, 5000),
        row("沙巴克中城墙", 111, 5000),
        row("弓箭手", 112, 2000),
        // `守卫` 故意缺行：`race_of` = -1 ⇒ 8 次失败，0 抽。
    ])
}

/// **C1/A 情景**：部署实读文件（默认名、弓手/守卫 HP=0、`MainDoorOpen=1`）⇒ 0 实体、4 条告警。
///
/// 牙：给 `弓箭手` 一个非 0 HP ⇒ 12 条额外 regen 调用 ⇒ 告警数/实体数红。
#[test]
fn castle_deployment_defaults_zero_entities_four_warns() {
    let f = fixture("3");
    let table = MonsterTable::new(vec![row("弓箭手", 112, 2000)]);
    let mut world = mir2_world::World::with_maps(vec![(20, 20)]);
    let mut host = MonGenHost::start(
        Vec::new(),
        &table,
        false,
        MonGenConfig::default(),
        &mut world.rng,
    );
    let mut displays = downlink::DisplayTable::new();
    let mut rng = RandomNumber::with_seed(20261020);
    let specs = default_specs();
    let report = initialize_castle(
        &mut world,
        &f.registry,
        &table,
        &mut host.state,
        &mut displays,
        &mut rng,
        &specs,
        CastleSiegeConfig::default(),
    );

    assert_eq!(report.spawned, 0, "部署口径 0 实体");
    assert_eq!(world.entities.iter().count(), 0, "世界内无门/墙/弓手");
    assert_eq!(report.warns.len(), 4, "仅 4 条告警：{:?}", report.warns);
    assert!(report.warns[0].contains("MainDoor.UnitObj = nil"));
    assert!(report.warns[1].ends_with("MainDoor"));
    assert!(report.warns[2].ends_with("LeftWall"));
    assert!(report.warns[3].ends_with("CenterWall"));
    // 名字全部未命中 ⇒ 未消耗任何随机数。
    assert_eq!(
        rng.random(),
        RandomNumber::with_seed(20261020).random(),
        "未知名路径 0 抽"
    );
}

/// **C2/C3/C4 合并**：`Castle/0` 夹具口径 ⇒ 16 实体、24 次 regen、8 条告警；逐只 6 抽。
///
/// 牙：① 改用 `draw_monster_object`（多 1 抽）⇒ 手算对照红；② 把 `CoolEye` 抽错位 ⇒ 对应
/// 实体的 `cool_eye`/后续流水红；③ 失败名字也抽 ⇒ 最终 `rng.random()` 与手算流水不等。
#[test]
fn castle_fixture_spawns_sixteen_with_six_draws_each() {
    let f = fixture("3");
    let table = castle_table();
    let mut world = mir2_world::World::with_maps(vec![(20, 20)]);
    let mut host = MonGenHost::start(
        Vec::new(),
        &table,
        false,
        MonGenConfig::default(),
        &mut world.rng,
    );
    let mut displays = downlink::DisplayTable::new();
    const SEED: i32 = 20261020;
    let mut rng = RandomNumber::with_seed(SEED);
    let specs = small_fixture_specs();
    let report = initialize_castle(
        &mut world,
        &f.registry,
        &table,
        &mut host.state,
        &mut displays,
        &mut rng,
        &specs,
        CastleSiegeConfig::default(),
    );

    assert_eq!(report.spawned, 16, "1 门 + 3 墙 + 12 弓手");
    assert_eq!(report.warns.len(), 8, "8 个守卫名未命中");
    assert_eq!(world.entities.iter().count(), 16);

    // ---- 逐只 6 抽：与独立 `DotNetRandom(SEED)` 的手算序列逐值对照 ----
    let mut hand = DotNetRandom::new(SEED);
    let mut hand_entity = |world: &mir2_world::World, id: i32, dir_override: Option<u8>| {
        let v1 = hand.next_max(1500);
        let v2 = hand.next_max(2000) + 2000;
        let v3 = hand.next_max(3000);
        let v4 = hand.next_max(3000);
        let v5 = hand.next_max(8);
        let v6 = hand.next_max(100);
        let e = world.entities.get(id).expect("spawned entity");
        assert_eq!(e.ctor_draws.run_tick_delta, v1, "BaseObject ctor 第 1 抽");
        assert_eq!(e.ctor_draws.base_search_time, v2, "BaseObject ctor 第 2 抽");
        assert_eq!(
            e.ctor_draws.attack_tick_delta, v3,
            "AnimalObject ctor 第 1 抽"
        );
        assert_eq!(
            e.ctor_draws.walk_tick_delta, v4,
            "AnimalObject ctor 第 2 抽"
        );
        assert_eq!(
            e.ai.dir,
            dir_override.unwrap_or_else(|| u8::try_from(v5).unwrap()),
            "Dir = RandomByte(8)（墙被 WallStructure.Initialize 覆写为 0）"
        );
        assert_eq!(e.ai.cool_eye, v6 < 0, "CoolEye（DB=0 ⇒ 恒 false，但仍抽）");
    };

    // 调用序 = 门 → 左/中/右墙 → 弓手 0..11（守卫 8 次失败）。
    let main_door = world.castles[0].main_door.expect("main door");
    let door_id = main_door.entity_id.expect("door actor id");
    hand_entity(&world, door_id, None);
    for role in [
        DefenseRole::LeftWall,
        DefenseRole::CenterWall,
        DefenseRole::RightWall,
    ] {
        let unit = world.castles[0].defense_unit(role).expect("wall");
        hand_entity(&world, unit.entity_id.expect("wall actor id"), Some(0));
    }
    for i in 0..12 {
        let unit = world.castles[0].archers[i].expect("archer");
        hand_entity(&world, unit.entity_id.expect("archer actor id"), None);
    }
    assert_eq!(
        rng.random(),
        hand.next_value(),
        "整段 16×6 = 96 抽后，两岸流水必须同步（未知名 = 0 抽）"
    );

    // ---- C3：实体字段 / 绑定 ----
    let door = world.entities.get(door_id).unwrap();
    assert_eq!(door.race, 110);
    assert_eq!(
        (door.hp, door.max_hp),
        (9999, 10000),
        "HP=配置值，MaxHP=模板"
    );
    assert_eq!(
        door.castle_binding,
        Some(mir2_world::entity::CastleBinding {
            castle_index: 0,
            role: DefenseRole::MainDoor
        })
    );
    assert_eq!(
        (main_door.opened, main_door.stone_mode),
        (false, false),
        "MainDoorOpen=0"
    );
    let wall = world.castles[0].left_wall.expect("left wall");
    let wall_e = world
        .entities
        .get(wall.entity_id.expect("wall actor"))
        .unwrap();
    assert_eq!(wall_e.race, 111);
    assert_eq!((wall_e.hp, wall_e.max_hp), (4999, 5000));
    assert_eq!(wall_e.ai.dir, 0, "WallStructure.Initialize 的 Dir=0");
    assert!(wall_e.castle_binding.is_some());
    let archer = world.castles[0].archers[0].expect("archer");
    let archer_e = world
        .entities
        .get(archer.entity_id.expect("archer actor"))
        .unwrap();
    assert_eq!(archer_e.race, 112);
    assert_eq!((archer_e.hp, archer_e.max_hp), (2000, 2000));
    assert_eq!(archer_e.guard_direction, 3, "GuardDirection=3");
    assert!(archer_e.castle_binding.is_some());
    assert!(
        world.castles[0].guards.iter().all(Option::is_none),
        "守卫全部未命中模板（且即使成功也不绑 Castle）"
    );
}

/// `specs_from_sabuk` 逐字段搬运（含默认 `SabukWConfig` 的默认名）。
#[test]
fn specs_from_sabuk_copies_defaults_verbatim() {
    let cfg = SabukWConfig::default();
    let specs = specs_from_sabuk(&cfg);
    assert_eq!(specs.main_door.name, "MainDoor");
    assert_eq!(specs.left_wall.name, "LeftWall");
    assert_eq!(specs.center_wall.name, "CenterWall");
    assert_eq!(specs.right_wall.name, "RightWall");
    assert_eq!(specs.archers.len(), 12);
    assert_eq!(specs.guards.len(), 12);
}
