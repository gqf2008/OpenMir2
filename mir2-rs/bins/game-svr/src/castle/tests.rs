//! Lane B 判据（`mir2-rs/tests/parity/m3/design-castle-entities-20261020.md` §6）。
//!
//! 每条用例的「牙」（改坏生产路径必红）写在函数文档里；红检记录见提交报告。

use mir2_data::models::MonsterInfo;
use mir2_envir::castle::{parse_sabuk_w_bytes, SabukWConfig};
use mir2_envir::{MapLoader, MapRegistry};
use mir2_shared::rng::{DotNetRandom, RandomNumber};
use mir2_world::castle::{
    area_state, CastleDate, CastleDefenseSpecs, CastleNow, CastleSiegeConfig, DefenseRole,
    DefenseUnitSpec, RedDieHome,
};
use mir2_world::mongen::MonGenConfig;

use crate::downlink;
use crate::mongen::{MonGenHost, MonsterTable};
use crate::smframe;

use super::{area_state_with, initialize_castle, specs_from_sabuk};

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

/// **C1**：`Castle/0/SabukW.txt` 的 hex 证据夹具 → `CastleDefenseSpecs` 逐字段；
/// A 情景（`Castle/Castle/SabukW.txt`）的部署等价由 `default_specs()` 合成用例对照。
///
/// 牙：① 把 `GUARD_ARRAY_LEN` 改成 4 ⇒ `guards.len()==12` 红；② 把中/右墙名按语义对调
/// ⇒ 墙名断言红；③ 把缺键守卫 HP 默认改 0 ⇒ `guards[4..].hp==2000` 红。
#[test]
fn castle_sabukw_specs_from_fixture() {
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
        own_guild: String::new(),
        map_name: "3".to_string(),
        home_map: "3".to_string(),
        home_x: 644,
        home_y: 290,
        war_range_x: 100,
        war_range_y: 100,
        envir_list: vec!["0151".to_string()],
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
        own_guild: String::new(),
        map_name: "3".to_string(),
        home_map: "3".to_string(),
        home_x: 644,
        home_y: 290,
        war_range_x: 100,
        war_range_y: 100,
        envir_list: vec!["0151".to_string()],
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
        0,
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
        0,
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

// ===========================================================================
// 车道 castlewar：攻城区判据（C2/C4/C5/C8 + 源码序门）
// ===========================================================================

/// C5 `area_state_bit4_encoding`：生产 `area_state_with` 的四向量 + 766 帧字节。
///
/// 牙：位常量 4 改 8 ⇒ 红；`|=` 改 `^=` ⇒ 重复置位 4→0 红；`|=` 改 `=` ⇒ `1|2|4` 变 4 红；
/// 把 n8 写进 `Param` ⇒ 帧字节红。
#[test]
fn c5_area_state_vectors_and_766_frame() {
    let mut flag = mir2_envir::MapInfoFlag::default();
    assert_eq!(area_state_with(&flag, false), 0, "无 FIGHT/SAFE/GW ⇒ 0");
    flag.fight_zone = true;
    assert_eq!(area_state_with(&flag, false), 1);
    flag.safe_area = true;
    assert_eq!(area_state_with(&flag, false), 1 | 2);
    assert_eq!(area_state_with(&flag, true), 1 | 2 | 4);
    let only = mir2_envir::MapInfoFlag {
        safe_area: true,
        ..mir2_envir::MapInfoFlag::default()
    };
    assert_eq!(area_state_with(&only, false), 2);
    assert_eq!(area_state_with(&only, true), 2 | 4);

    // 同一 bit 重复置位幂等（累加器生产实现）。
    let mut n8 = area_state(false, false, true);
    mir2_world::castle::set_area_flag(&mut n8, mir2_world::castle::AREA_STATE_GUILD_WAR, true);
    assert_eq!(n8, 4);

    // 766：Recog = n8，Param/Tag/Series = 0，无尾。
    let body = smframe::sm_areastate(7);
    assert_eq!(&body[0..4], &7i32.to_le_bytes(), "Recog = n8");
    assert_eq!(
        u16::from_le_bytes([body[4], body[5]]),
        mir2_protocol::messages::SM_AREASTATE
    );
    assert_eq!(&body[6..12], &[0u8; 6], "Param/Tag/Series 全 0");
    assert_eq!(body.len(), 12, "无尾");
}

/// C4 + C8：生产 `initialize_castle` → `UserCastle::run` 的 Ghost 回收（Guards 先于 Archers）。
///
/// 牙：只清 Archers ⇒ 红；删回收 ⇒ 红；把实体也删掉 ⇒ 实体数红。
#[test]
fn c8_ghost_reclaim_through_production_run() {
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
    let mut rng = RandomNumber::with_seed(20261020);
    let report = initialize_castle(
        &mut world,
        &f.registry,
        &table,
        &mut host.state,
        &mut displays,
        &mut rng,
        &small_fixture_specs(),
        CastleSiegeConfig::default(),
        0,
    );
    assert_eq!(report.spawned, 16);
    let entities_before = world.entities.iter().count();
    world.castles[0].guards[0] = Some(mir2_world::castle::CastleUnit {
        ghost: true,
        ..mir2_world::castle::CastleUnit::default()
    });
    world.castles[0].archers[0].as_mut().unwrap().ghost = true;
    let events = world.castles[0].run(CastleNow::new(CastleDate::new(2026, 10, 21), 3, 10_000));
    assert!(events.is_empty(), "非开战钟点无事件");
    assert!(world.castles[0].guards[0].is_none(), "Ghost 守卫被回收");
    assert!(world.castles[0].archers[0].is_none(), "Ghost 弓手被回收");
    assert_eq!(
        world.entities.iter().count(),
        entities_before,
        "只清槽位，不删实体（oracle 只清 BaseObject）"
    );
    assert!(
        world.castles[0].guards[1].is_none(),
        "未命中的守卫槽保持 None"
    );
    assert!(world.castles[0].archers[1].is_some(), "非 Ghost 弓手不变");
}

/// 源码序门：进图改写必须在 `shift_enter_position` 之前；每 tick 城堡检查在
/// `tick_world_head_with_specials` 之前；登录 766 覆盖在 `enter::apply` 之后；
/// 10s `CastleMgr.Run` 在 Guild 之后。
///
/// 牙：把任一段挪位/删除 ⇒ 本用例必红（防「函数写了但没被调用」）。
#[test]
fn castle_production_call_order_is_wired() {
    let main_rs = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .expect("读 main.rs");
    let reloc = main_rs
        .find("mir2_world::castle::relocate_on_enter(")
        .expect("进图改写调用必须存在");
    let shift = main_rs
        .find("let shift = mapwire::shift_enter_position(")
        .expect("移位环调用必须存在");
    assert!(reloc < shift, "城堡改写必须在移位环之前");

    let tick = main_rs
        .find("mir2_world::castle::castle_war_tick(")
        .expect("每 tick 城堡检查必须存在");
    let tick_head = main_rs
        .find("let mut rec = tick_world_head_with_specials(")
        .expect("world tick 必须存在");
    assert!(tick < tick_head, "每 tick 城堡检查在 world tick 之前");

    let apply = main_rs
        .find("for w in enter::apply(&mut inputs, &wx)")
        .expect("enter::apply 必须存在");
    let override_area = main_rs
        .find("inputs.area_state = Some(")
        .expect("登录 766 的 bit4 覆盖必须存在");
    assert!(
        apply < override_area,
        "登录 766 必须在 enter::apply 之后装配"
    );

    let guild = main_rs
        .find("let outcome = guild_runtime::step(")
        .expect("GuildMgr.Run 必须存在");
    let castle_run = main_rs
        .find("let castle_events = world.castles[idx].run(now);")
        .expect("CastleMgr.Run 必须存在");
    assert!(guild < castle_run, "Guild 先、Castle 后（同一 10s 块）");
    assert!(
        main_rs.contains("world.clock_ms - castle_run_tick > 10_000"),
        "CastleMgr.Run 必须走严格 `>` 的 10s 门（SystemProcessor.cs:70-77）"
    );
}

/// `specs_from_sabuk` 必须搬运新增的 `OwnGuild/Home*/WarRange/EnvirList`（缺字段 ⇒ 谓词全假）。
#[test]
fn specs_from_sabuk_copies_war_area_fields() {
    let cfg = SabukWConfig {
        own_guild: "沙巴克".to_string(),
        home_x: 644,
        home_y: 290,
        war_range_x: 100,
        war_range_y: 120,
        envir_list: vec!["0151".to_string(), "d701".to_string()],
        ..SabukWConfig::default()
    };
    let specs = specs_from_sabuk(&cfg);
    assert_eq!(specs.own_guild, "沙巴克");
    assert_eq!((specs.home_x, specs.home_y), (644, 290));
    assert_eq!((specs.war_range_x, specs.war_range_y), (100, 120));
    assert_eq!(specs.envir_list, vec!["0151", "d701"]);
}

/// `load_red_die_home` 读部署口径的 `[Setup]`（缺键/缺文件回落源码默认）。
#[test]
fn red_die_home_defaults_and_ini() {
    let missing = std::env::temp_dir().join("gs-castle-no-such-server-conf.conf");
    assert_eq!(super::load_red_die_home(&missing), RedDieHome::default());
    let path = std::env::temp_dir().join(format!("gs-castle-server-{}.conf", std::process::id()));
    std::fs::write(
        &path,
        b"[Setup]\r\nRedDieHomeMap=3\r\nRedDieHomeX=838\r\nRedDieHomeY=668\r\n",
    )
    .unwrap();
    let red = super::load_red_die_home(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        red,
        RedDieHome {
            map_name: "3".to_string(),
            x: 838,
            y: 668
        }
    );
}
