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
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
    )
    .expect("读 lib.rs");
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
        .find("game_svr::castle::run_castle_tick(")
        .expect("CastleMgr.Run 必须存在");
    assert!(guild < castle_run, "Guild 先、Castle 后（同一 10s 块）");
    // 10s 门与落盘门都在 `CastleHostGates::tick` 里（严格 `>` 的行为由
    // `persist_tests::host_gates_are_independent_on_fake_tick_sequence` 钉住）。
    assert!(
        main_rs.contains("castle_gates.tick("),
        "两道门必须走 CastleHostGates（SystemProcessor.cs:70-77 / :54-62）"
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

/// **T-131 相位合同**：生产 `initialize_castle`（oracle `CastleMgr.Initialize`，`GameApp.cs:213`，
/// 早于 `GuildMgr.LoadGuildInfo` `:245`）只经 `LoadConfig` 读 `OwnGuild` 名字，**不解析**
/// `MasterGuild`（`UserCastle.cs:231` 的 `FindGuild` 在行会表装载前恒 null）⇒ 启动后
/// `master_guild == None`、`is_master_guild_name(Some(OwnGuild)) == false`（与 oracle 启动态一致）。
///
/// 牙：让 `UserCastle::initialize` 落 `master_guild = Some(..)`（错误相位）⇒ 本用例必红。
#[test]
fn initialize_leaves_master_guild_none_phase_contract() {
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
    let mut specs = default_specs();
    specs.own_guild = "沙巴克".to_string();
    let _ = initialize_castle(
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
    let castle = &world.castles[0];
    assert_eq!(
        castle.own_guild.as_deref(),
        Some("沙巴克"),
        "LoadConfig 只搬名字"
    );
    assert!(
        castle.master_guild.is_none(),
        "启动相位：MasterGuild 在行会表装载前不得解析（oracle GameApp.cs:213 < :245）"
    );
    assert!(
        !castle.is_master_guild_name(Some("沙巴克")),
        "MasterGuild == None ⇒ 名字相等也不得判城主身份"
    );
}

/// **F3 牙（T-132 核心接线）**：`query_user_name_color` 必须用 `world.castle_name_colors`
/// （配置源 → World → 消费者全链），而不是硬编码 `CastleNameColors::default()`。
/// 缺省 World ⇒ PKFlag 目标取 47；`set_castle_name_colors(pk_flag=200)` ⇒ 取 200。
///
/// 红检（改坏必红）：把 `query_user_name_color` 的 `&world.castle_name_colors` 改回
/// `&mir2_world::castle::CastleNameColors::default()`（本卡要修的原始状态）⇒ 200 断言必红。
#[test]
fn query_user_name_color_reads_world_castle_name_colors() {
    let mut world = mir2_world::World::with_maps(vec![(20, 20)]);
    let mut target = mir2_world::Entity::new(1, "p", 10, 10);
    target.pvp_flag = true;
    target.pk_point = 0; // `PvpLevel() = 0 < 2`
    world.add_entity(target);
    // 缺省 World（47/180/69/221）：PKFlag 目标取 47。
    {
        let t = world.entities.get(1).expect("目标存在");
        let c = super::query_user_name_color(&world, None, None, t, 255, false, false)
            .expect("无城堡区");
        assert_eq!(c, 47, "缺省名字色 PKFlag = 0x2F");
    }
    // 配置覆盖（11/22/33/44，**经 setter**）：PKFlag 目标取 11。
    world.set_castle_name_colors(mir2_world::castle::CastleNameColors {
        pk_flag: 11,
        ally_guild: 22,
        war_guild: 33,
        in_free_pk: 44,
    });
    {
        let t = world.entities.get(1).expect("目标存在");
        let c = super::query_user_name_color(&world, None, None, t, 255, false, false)
            .expect("无城堡区");
        assert_eq!(c, 11, "world.castle_name_colors.pk_flag 必须生效");
    }
    // 城堡战区（under_war + 两侧 InGuildWarArea + 无行会）⇒ `in_free_pk=44`。
    let mut castle = mir2_world::castle::UserCastle::new(
        "沙巴克",
        mir2_world::castle::CastleSiegeConfig::default(),
    );
    castle.castle_map_id = Some(0);
    castle.home_x = 10;
    castle.home_y = 10;
    castle.under_war = true;
    world.castles.push(castle);
    let t = world.entities.get(1).expect("目标存在");
    let c = super::query_user_name_color(&world, None, None, t, 255, true, true).expect("战区查询");
    assert_eq!(c, 44, "城堡战区名字色 in_free_pk 必须生效");
}

// ---------------------------------------------------------------------------
// **T-127**（卡 `openmir2-m3-castle-siege-state` 落帧残余）：`UserCastle.Run` 事件的帧合同
// ---------------------------------------------------------------------------
//
// 四条用例分别钉：`Broadcast`（`SendBroadCastMsgExt`）/ `RefreshShowName`（`GetMapRageHuman` +
// `RefShowName`，含逐观察者 `GetChrColor` 的 N1 丢弃）/ `Structure(Ref*)`（`RM_DIGDOWN` 经
// `pending_monster_ref_msgs` → `drain_monster_ref_msgs` 出 `SM_DIGDOWN`）。
//
// 牙（改坏必红，报告里逐条给读数）：
// ① `fan_out_sysmessage_to_all` 改回「按入队序直发」⇒ 四叉堆出队序断言红（3 条同优先级 = `[0,2,1]`）；
// ② `refresh_show_name_in_area` 去掉 `map_id != palace_map_id` 门 ⇒ 宫外玩家多收 2 帧 ⇒ 计数红；
// ③ `queue_structure_ref_msg` 把 `RM_DIGDOWN`/`RM_DIGUP` 对调 ⇒ `SM_DIGDOWN` 逐字节断言红。

fn play_entity(name: &str, x: i32, y: i32) -> mir2_world::Entity {
    let mut e = mir2_world::Entity::new(0, name.to_string(), x, y);
    e.race = mir2_world::ACTOR_RACE_PLAY;
    e
}

fn online_entry(chr: &str, socket: i32, actor_id: i32) -> crate::OnlineEntry {
    crate::OnlineEntry {
        account: chr.to_string(),
        chr_name: chr.to_string(),
        session_id: socket,
        gate_idx: 0,
        socket,
        actor_id,
    }
}

fn drain_frames(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::SimToGate>,
) -> Vec<(i32, Vec<u8>)> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if let crate::SimToGate::SendFrame { socket, body } = ev {
            out.push((socket, body));
        }
    }
    out
}

fn ident_of(body: &[u8]) -> u16 {
    u16::from_le_bytes([body[4], body[5]])
}

fn recog_of(body: &[u8]) -> i32 {
    i32::from_le_bytes([body[0], body[1], body[2], body[3]])
}

/// **T-127 ①**：`SendBroadCastMsgExt`（`WorldServer.cs:1726-1737`）⇒ 每个 `!Ghost` 在线玩家
/// 一条 `SM_SYSMESSAGE(100)`，`Recog` = **收件人自己**、`Param = MakeWord(RedF,RedB)`、
/// `Tag = 0`、`Series = 1`、尾 = `EncodeString(GBK 原文)`；同优先级按四叉堆 sift 出队
/// （3 条 ⇒ `[0,2,1]`，不是入队序）。
///
/// oracle 锚：`BaseObject.SysMsg` 的非 `Notice` 支（`BaseObject.cs:1656-1675`）、
/// `PlayObject.Message.cs:1715-1718`、`WorldServer.cs:1726-1737`（`!Ghost` 门、无 caller 排除）。
#[test]
fn t127_broadcast_fans_out_to_every_nonghost_player_in_heap_order() {
    let mut world = mir2_world::World::with_maps(vec![(16, 16)]);
    world.open_session("甲");
    let a_id = world
        .enter_map("甲", play_entity("甲", 5, 5), 0)
        .expect("甲 进图");
    world.open_session("乙");
    let mut ghost = play_entity("乙", 6, 5);
    ghost.ghost = true;
    let b_id = world
        .enter_map("乙", ghost, 0)
        .expect("乙 进图（已 Ghost）");

    let (tx0, mut rx0) = tokio::sync::mpsc::unbounded_channel::<crate::SimToGate>();
    let mut gates: std::collections::HashMap<
        u8,
        tokio::sync::mpsc::UnboundedSender<crate::SimToGate>,
    > = std::collections::HashMap::new();
    gates.insert(0, tx0);
    let online: crate::OnlineRegistry =
        vec![online_entry("甲", 5, a_id), online_entry("乙", 6, b_id)];

    let texts = vec![
        "[沙巴克 攻城战已经开始]".to_string(),
        "[沙巴克 攻城战离结束还有10分钟]".to_string(),
        "[沙巴克 攻城战已经结束]".to_string(),
    ];
    let sent = super::fan_out_sysmessage_to_all(&online, &world, &gates, &texts, 0xFC, 0x01);
    assert_eq!(sent, 3, "Ghost 的乙不产帧 ⇒ 只有甲收 3 条");

    let frames = drain_frames(&mut rx0);
    assert_eq!(frames.len(), 3);
    // 同优先级 3 条的 .NET 四叉堆出队序 = `[0,2,1]`（`crates/world/src/msgqueue.rs`）。
    let want = [&texts[0], &texts[2], &texts[1]];
    for (i, ((socket, body), text)) in frames.iter().zip(want).enumerate() {
        assert_eq!(*socket, 5, "第 {i} 帧：只投给甲（乙 Ghost）");
        assert_eq!(
            recog_of(body),
            a_id,
            "第 {i} 帧：Recog = 收件人自己的 ActorId"
        );
        assert_eq!(
            ident_of(body),
            mir2_protocol::messages::SM_SYSMESSAGE,
            "第 {i} 帧 ident"
        );
        assert_eq!(
            u16::from_le_bytes([body[6], body[7]]),
            0x01FC,
            "第 {i} 帧：Param = MakeWord(RedF,RedB)"
        );
        assert_eq!(
            u16::from_le_bytes([body[8], body[9]]),
            0,
            "第 {i} 帧：Tag = 0"
        );
        assert_eq!(
            u16::from_le_bytes([body[10], body[11]]),
            1,
            "第 {i} 帧：Series 恒 1"
        );
        let raw = mir2_protocol::edcode::decode(&body[12..]);
        assert_eq!(
            encoding_rs::GBK.decode(&raw).0,
            text.as_str(),
            "第 {i} 帧：尾 = EncodeString(GBK 原文)"
        );
    }
}

/// **T-127 ②**：`StartWallconquestWar` 的 `RefShowName`——目标 = 皇宫图内 `<=100` 格玩家
/// （`GetMapRageHuman`，`WorldServer.cs:1698-1725`），收件人 = 目标 ±12 格格扫（含自己）⇒
/// `SM_USERNAME(42)`：`Recog` = **被展示者**、`Param` = `GetChrColor`（这里无行会/无战区 ⇒
/// `Entity.name_color`）、尾 = `EncodeString(GetShowName())`。
///
/// 「宫外玩家不进目标集」是本条的核心断言（牙②）。
#[test]
fn t127_ref_show_name_targets_only_palace_area_players() {
    let mut world = mir2_world::World::with_maps(vec![(16, 16), (16, 16)]);
    world.open_session("甲");
    let a_id = world
        .enter_map("甲", play_entity("甲", 5, 5), 0)
        .expect("甲 进皇宫图");
    world.open_session("乙");
    let b_id = world
        .enter_map("乙", play_entity("乙", 6, 5), 0)
        .expect("乙 进皇宫图");
    world.open_session("丙");
    let c_id = world
        .enter_map("丙", play_entity("丙", 5, 5), 1)
        .expect("丙 进另一张图");

    let (tx0, mut rx0) = tokio::sync::mpsc::unbounded_channel::<crate::SimToGate>();
    let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel::<crate::SimToGate>();
    let mut gates: std::collections::HashMap<
        u8,
        tokio::sync::mpsc::UnboundedSender<crate::SimToGate>,
    > = std::collections::HashMap::new();
    gates.insert(0, tx0);
    gates.insert(1, tx1);
    let online: crate::OnlineRegistry = vec![
        online_entry("甲", 5, a_id),
        online_entry("乙", 6, b_id),
        crate::OnlineEntry {
            account: "丙".to_string(),
            chr_name: "丙".to_string(),
            session_id: 7,
            gate_idx: 1,
            socket: 7,
            actor_id: c_id,
        },
    ];
    let displays = downlink::DisplayTable::new();
    let sessions: std::collections::HashMap<String, crate::SessionGround> = [
        ("甲".to_string(), crate::tests::test_session_ground(a_id)),
        ("乙".to_string(), crate::tests::test_session_ground(b_id)),
        ("丙".to_string(), crate::tests::test_session_ground(c_id)),
    ]
    .into_iter()
    .collect();
    let war_states: std::collections::HashMap<i32, mir2_world::castle::CastleWarState> =
        std::collections::HashMap::new();

    let sent = super::refresh_show_name_in_area(
        &online,
        &world,
        &displays,
        &gates,
        &sessions,
        &war_states,
        Some(0), // 皇宫图 = map_id 0
        5,
        5,
        100,
        12,
    );
    assert_eq!(
        sent, 4,
        "甲/乙 各收 2 条（目标甲、乙）；丙不在皇宫图 ⇒ 0 条"
    );

    let frames = drain_frames(&mut rx0);
    assert_eq!(frames.len(), 4);
    // 逐收件人：入队序 = 目标序（甲 → 乙），2 条同优先级 ⇒ 出队 `[0,1]`。
    for (i, target) in [(0usize, a_id), (1, b_id)].into_iter() {
        let body = &frames[i].1;
        assert_eq!(frames[i].0, 5, "甲 的 socket");
        assert_eq!(recog_of(body), target, "甲 的队列：先目标甲、再目标乙");
        assert_eq!(ident_of(body), mir2_protocol::messages::SM_USERNAME);
        assert_eq!(
            u16::from_le_bytes([body[6], body[7]]),
            255,
            "无行会/无战区 ⇒ GetChrColor = Entity.name_color（255）"
        );
    }
    assert_eq!(recog_of(&frames[0].1), a_id, "目标甲的名字帧");
    assert_eq!(recog_of(&frames[1].1), b_id, "目标乙的名字帧");
    assert_eq!(recog_of(&frames[2].1), a_id, "乙 也收到甲的名字帧");
    assert_eq!(recog_of(&frames[3].1), b_id, "乙 也收到乙的名字帧");
    for (_, body) in &frames {
        let raw = mir2_protocol::edcode::decode(&body[12..]);
        let text = encoding_rs::GBK.decode(&raw).0.into_owned();
        assert!(
            text == "甲" || text == "乙",
            "尾 = EncodeString(GetShowName())，实得 {text:?}"
        );
    }
}

/// **T-127 ③**：`GetChrColor` 的 N1 分支（`PlayObject.cs:3656-3659` 的 `castle.IsAttackGuild`）——
/// 观察者有行会 + 双方都在攻城区 + `UnderWar` ⇒ oracle 在收件人的消息处理里抛异常被吞 ⇒
/// 本帧不投递。同一夹具去掉行会 ⇒ 帧恢复，`Param = in_free_pk`（战区分支先写的那一色）。
///
/// 牙④：把 `Err` 分支从 `continue` 改成「照发」⇒ 第一段断言（0 帧）红。
#[test]
fn t127_ref_show_name_n1_drops_frame_for_guilded_viewer_in_war_area() {
    let mut world = mir2_world::World::with_maps(vec![(16, 16)]);
    world.open_session("甲");
    let a_id = world
        .enter_map("甲", play_entity("甲", 5, 5), 0)
        .expect("甲 进图");
    world.open_session("乙");
    let b_id = world
        .enter_map("乙", play_entity("乙", 6, 5), 0)
        .expect("乙 进图");
    let mut castle = mir2_world::castle::UserCastle::new(
        "沙巴克",
        mir2_world::castle::CastleSiegeConfig::default(),
    );
    // `CastlePlaceMap` 命中 ⇒ `InCastleWarArea` 恒真（`UserCastle.cs:674-678` 的皇宫/密道支）。
    castle.palace_map_id = Some(0);
    castle.under_war = true;
    world.castles.push(castle);
    let in_free_pk = world.castle_name_colors.in_free_pk;

    let (tx0, mut rx0) = tokio::sync::mpsc::unbounded_channel::<crate::SimToGate>();
    let mut gates: std::collections::HashMap<
        u8,
        tokio::sync::mpsc::UnboundedSender<crate::SimToGate>,
    > = std::collections::HashMap::new();
    gates.insert(0, tx0);
    let online: crate::OnlineRegistry =
        vec![online_entry("甲", 5, a_id), online_entry("乙", 6, b_id)];
    let displays = downlink::DisplayTable::new();
    let mut war_states: std::collections::HashMap<i32, mir2_world::castle::CastleWarState> =
        std::collections::HashMap::new();
    for id in [a_id, b_id] {
        let mut st = mir2_world::castle::CastleWarState::new(0);
        st.in_guild_war_area = true;
        war_states.insert(id, st);
    }
    let mut guilded: std::collections::HashMap<String, crate::SessionGround> = [
        ("甲".to_string(), crate::tests::test_session_ground(a_id)),
        ("乙".to_string(), crate::tests::test_session_ground(b_id)),
    ]
    .into_iter()
    .collect();
    for name in ["甲", "乙"] {
        guilded.get_mut(name).unwrap().guild.guild_name = Some("攻方".to_string());
    }

    let dropped = super::refresh_show_name_in_area(
        &online,
        &world,
        &displays,
        &gates,
        &guilded,
        &war_states,
        Some(0),
        5,
        5,
        100,
        12,
    );
    assert_eq!(
        dropped, 0,
        "有会观察者在攻城区命中 N1 ⇒ oracle 抛异常被吞 ⇒ 不投递"
    );
    assert!(drain_frames(&mut rx0).is_empty());

    // 去掉行会 ⇒ N1 不可达 ⇒ 帧恢复，色 = `InFreePKAreaNameColor`（`PlayObject.cs:3618`）。
    let bare: std::collections::HashMap<String, crate::SessionGround> = [
        ("甲".to_string(), crate::tests::test_session_ground(a_id)),
        ("乙".to_string(), crate::tests::test_session_ground(b_id)),
    ]
    .into_iter()
    .collect();
    let sent = super::refresh_show_name_in_area(
        &online,
        &world,
        &displays,
        &gates,
        &bare,
        &war_states,
        Some(0),
        5,
        5,
        100,
        12,
    );
    assert_eq!(sent, 4);
    for (_, body) in drain_frames(&mut rx0) {
        assert_eq!(ident_of(&body), mir2_protocol::messages::SM_USERNAME);
        assert_eq!(
            u16::from_le_bytes([body[6], body[7]]),
            u16::from(in_free_pk),
            "战区分支先写 InFreePKAreaNameColor，无会观察者直接返回该色"
        );
    }
}

/// **T-127 ④**：`MainDoorControl(true)` 的 `CastleDoor.Close()` ⇒ `RM_DIGDOWN`（`CastleDoor.cs:60-76`）
/// 经 `queue_structure_ref_msg` 重新入队成 `MonsterRefMsg`，由 `drain_monster_ref_msgs` 出
/// `SM_DIGDOWN(21)`（头 `(ActorId, X, Y, 0)`、无尾），收件人 = 门 ±12 格内的玩家。
///
/// 牙③：把 `RM_DIGDOWN`/`RM_DIGUP` 对调 ⇒ ident 断言红。
#[test]
fn t127_door_close_structure_event_drains_to_sm_digdown() {
    let mut world = mir2_world::World::with_maps(vec![(16, 16)]);
    let mut door = mir2_world::Entity::new(0, "沙巴克城门".to_string(), 5, 5);
    door.race = 110;
    let door_id = world.add_entity(door);
    world.open_session("甲");
    let a_id = world
        .enter_map("甲", play_entity("甲", 6, 5), 0)
        .expect("甲 进图");
    let mut castle = mir2_world::castle::UserCastle::new(
        "沙巴克",
        mir2_world::castle::CastleSiegeConfig::default(),
    );
    castle.main_door = Some(mir2_world::castle::CastleUnit {
        entity_id: Some(door_id),
        x: 5,
        y: 5,
        hp: 9_999,
        max_hp: 9_999,
        ..mir2_world::castle::CastleUnit::default()
    });
    world.castles.push(castle);

    // 坐标对不上任何门/墙 ⇒ 不入队。
    assert!(!super::queue_structure_ref_msg(
        &mut world,
        0,
        &mir2_world::castle::StructureEvent::RefDigDown {
            dir: 0,
            x: 99,
            y: 99
        }
    ));
    assert!(world.pending_monster_ref_msgs.is_empty());

    assert!(super::queue_structure_ref_msg(
        &mut world,
        0,
        &mir2_world::castle::StructureEvent::RefDigDown { dir: 0, x: 5, y: 5 }
    ));
    assert_eq!(world.pending_monster_ref_msgs.len(), 1);
    let queued = &world.pending_monster_ref_msgs[0];
    assert_eq!(queued.actor_id, door_id);
    assert_eq!(queued.ident, mir2_world::monai::RM_DIGDOWN);
    assert_eq!(queued.recipients, vec![a_id], "收件人 = 门 ±12 格内玩家");

    let (tx0, mut rx0) = tokio::sync::mpsc::unbounded_channel::<crate::SimToGate>();
    let mut gates: std::collections::HashMap<
        u8,
        tokio::sync::mpsc::UnboundedSender<crate::SimToGate>,
    > = std::collections::HashMap::new();
    gates.insert(0, tx0);
    let online: crate::OnlineRegistry = vec![online_entry("甲", 5, a_id)];
    let displays = downlink::DisplayTable::new();
    crate::drain_monster_ref_msgs(&mut world, &displays, &online, &gates);

    let frames = drain_frames(&mut rx0);
    assert_eq!(frames.len(), 1, "SM_DIGDOWN 单帧");
    let body = &frames[0].1;
    assert_eq!(recog_of(body), door_id, "Recog = 门 ActorId");
    assert_eq!(ident_of(body), mir2_protocol::messages::SM_DIGDOWN);
    assert_eq!(u16::from_le_bytes([body[6], body[7]]), 5, "Param = X");
    assert_eq!(u16::from_le_bytes([body[8], body[9]]), 5, "Tag = Y");
    assert_eq!(
        u16::from_le_bytes([body[10], body[11]]),
        0,
        "Series = 0（RM_DIGDOWN 支）"
    );
    assert_eq!(body.len(), 12, "RM_DIGDOWN 无尾（SendSocket(ClientMsg)）");
}

// ---------------------------------------------------------------------------
// **T-127 返工 F1**（review `ab1ac11753`）：跨源帧序必须按 oracle `MsgQueue` 优先级升序
// ---------------------------------------------------------------------------

/// oracle 的 `MsgQueue` 优先级 = `(byte)wIdent`（`BaseObject.Message.cs:181`）：
/// `RM_USERNAME=8101 ⇒ 165`、`RM_DIGUP=10200 ⇒ 216`、`RM_DIGDOWN=10201 ⇒ 217`、
/// `RM_SYSMESSAGE=11004 ⇒ 252`（`src/OpenMir2/Messages.cs:614/615/646/699`）。
///
/// 牙：把任一常量换成 SM 侧号码或写成 `ident` 全值 ⇒ 断言红（防止再用「120/122」这类
/// 非源码字面量）。
#[test]
fn t127_frame_priorities_are_low_byte_of_rm_ident() {
    use mir2_protocol::messages as m;
    let pri = |v: u16| (v & 0xFF) as u8;
    assert_eq!(m::RM_USERNAME, 8101);
    assert_eq!(pri(m::RM_USERNAME), 165);
    assert_eq!(m::RM_DIGUP, 10200);
    assert_eq!(pri(m::RM_DIGUP), 216);
    assert_eq!(m::RM_DIGDOWN, 10201);
    assert_eq!(pri(m::RM_DIGDOWN), 217);
    assert_eq!(m::RM_SYSMESSAGE, 11004);
    assert_eq!(pri(m::RM_SYSMESSAGE), 252);
    assert!(pri(m::RM_USERNAME) < pri(m::RM_DIGUP));
    assert!(pri(m::RM_DIGUP) < pri(m::RM_DIGDOWN));
    assert!(pri(m::RM_DIGDOWN) < pri(m::RM_SYSMESSAGE));
}

/// **跨源合同（F1 的判据）**：同一收件人在**同一个城堡 tick** 里同时收到
/// `RefShowName`(165)、门 `RM_DIGDOWN`(217)、全服广播(252) ⇒ 线缆序必须是
/// `[SM_USERNAME, SM_DIGDOWN, SM_SYSMESSAGE]`（= 优先级升序），
/// **不是**入队序、也不是上一版实现的 `[SYSMESSAGE, USERNAME, DIGDOWN]`。
///
/// 驱动的是三个**生产排水函数**（`refresh_show_name_in_area` → `drain_monster_ref_msgs` →
/// `fan_out_sysmessage_to_all`），调用序 = `sim_loop` 的三相位
/// （165 立即 / 216-217 走 tick head / 252 在 tick head 之后；见 `lib.rs` 的同名源码序门）。
///
/// 牙：把 `fan_out_sysmessage_to_all` 挪回 username 之前（上一版实现）⇒ 首帧断言红。
#[test]
fn t127_cross_source_frames_follow_priority_order() {
    let mut world = mir2_world::World::with_maps(vec![(16, 16)]);
    let mut door = mir2_world::Entity::new(0, "沙巴克城门".to_string(), 5, 5);
    door.race = 110;
    let door_id = world.add_entity(door);
    world.open_session("甲");
    let a_id = world
        .enter_map("甲", play_entity("甲", 5, 5), 0)
        .expect("甲 进图");
    let mut castle = mir2_world::castle::UserCastle::new(
        "沙巴克",
        mir2_world::castle::CastleSiegeConfig::default(),
    );
    // `CastlePlaceMap` = map 0 ⇒ `InCastleWarArea` 为真（`UserCastle.cs:674-678` 皇宫支）。
    castle.palace_map_id = Some(0);
    castle.main_door = Some(mir2_world::castle::CastleUnit {
        entity_id: Some(door_id),
        x: 5,
        y: 5,
        hp: 9_999,
        max_hp: 9_999,
        ..mir2_world::castle::CastleUnit::default()
    });
    world.castles.push(castle);

    let (tx0, mut rx0) = tokio::sync::mpsc::unbounded_channel::<crate::SimToGate>();
    let mut gates: std::collections::HashMap<
        u8,
        tokio::sync::mpsc::UnboundedSender<crate::SimToGate>,
    > = std::collections::HashMap::new();
    gates.insert(0, tx0);
    let online: crate::OnlineRegistry = vec![online_entry("甲", 5, a_id)];
    let displays = downlink::DisplayTable::new();
    let sessions: std::collections::HashMap<String, crate::SessionGround> =
        [("甲".to_string(), crate::tests::test_session_ground(a_id))]
            .into_iter()
            .collect();
    let war_states: std::collections::HashMap<i32, mir2_world::castle::CastleWarState> =
        std::collections::HashMap::new();

    // 相位①（165）：`StartWallconquestWar` 的 `RefShowName`。
    let n1 = super::refresh_show_name_in_area(
        &online,
        &world,
        &displays,
        &gates,
        &sessions,
        &war_states,
        Some(0),
        5,
        5,
        100,
        12,
    );
    // 相位②（217）：`MainDoorControl(true)` 的 `CastleDoor.Close()`。
    assert!(super::queue_structure_ref_msg(
        &mut world,
        0,
        &mir2_world::castle::StructureEvent::RefDigDown { dir: 0, x: 5, y: 5 }
    ));
    crate::drain_monster_ref_msgs(&mut world, &displays, &online, &gates);
    // 相位③（252）：`SendBroadCastMsgExt`。
    let n3 = super::fan_out_sysmessage_to_all(
        &online,
        &world,
        &gates,
        &["[沙巴克 攻城战已经开始]".to_string()],
        0xFC,
        0x01,
    );
    assert!(n1 > 0 && n3 > 0, "三个相位都必须产帧（n1={n1} n3={n3}）");

    let frames = drain_frames(&mut rx0);
    let idents: Vec<u16> = frames.iter().map(|(_, b)| ident_of(b)).collect();
    assert_eq!(
        idents,
        vec![
            mir2_protocol::messages::SM_USERNAME,
            mir2_protocol::messages::SM_DIGDOWN,
            mir2_protocol::messages::SM_SYSMESSAGE,
        ],
        "跨源帧序必须是优先级升序 165 < 217 < 252"
    );
}

// ---------------------------------------------------------------------------
// **卡 `openmir2-castle-official-design`（CastleOfficial 门，tier R）**
// ---------------------------------------------------------------------------
//
// 钉两层：
// ① `CastleOfficial.GetVariableText`（`CastleOfficial.cs:33-91`）+ `NormNpc` 的 `$DATETIME/
//    $CASTLEWARDATE/$LISTOFWAR/$CASTLEGETDAYS` 展开（含 `Castle == null ⇒ 整行 "????"` 与
//    `$CASTLEDOORSTATE` 无门 ⇒ NRE 中止）；
// ② 宿主 `castle_vars_for`（`CastleMgr.InCastleWarArea(this)`，`NormNpc.cs:1674-1678`）。
//
// 牙（改坏必红，读数见报告 §5）：① `$CASTLEDOORSTATE` 缺门时改成返回 "closed" ⇒ A2 红；
// ② `Castle == null` 分支不整行覆盖 ⇒ A3 红；③ `$CASTLEWARDATE` 的整行替换改回 token 替换 ⇒ A4 红。

fn castle_vars_fixture() -> game_svr::npc::CastleVars {
    game_svr::npc::CastleVars {
        name: "沙巴克".to_string(),
        own_guild: String::new(),
        lord: None,
        total_gold: 1234,
        today_income: 56,
        main_door_state: Some("closed"),
        under_war: false,
        war_date_text: String::new(),
        war_list_text: String::new(),
        get_days: Some(7),
        change_date_text: Some("1998-01-01 00:00:00".to_string()),
    }
}

fn official_player<'r, 'g>(rng: &'r mut dyn FnMut(i32) -> i32) -> game_svr::npc::NpcPlayer<'r, 'g> {
    use game_svr::npc::{CastleVars, NpcPlayer};
    let mut p = NpcPlayer::empty(rng);
    p.vars.castle_official = true;
    p.vars.castle = Some(CastleVars {
        ..castle_vars_fixture()
    });
    p.vars.wall_clock = "2026-10-25 20:00:00".to_string();
    p.vars.repair_door_price = 2_000_000;
    p.vars.repair_wall_price = 500_000;
    p.vars.hire_guard_price = 300_000;
    p.vars.hire_archer_price = 300_000;
    p
}

/// **A1**：`@main` 页用到的 11 个 token 一次展开（部署值：OWNERGUILD 空 ⇒ 游戏管理、
/// LORD 无城主 ⇒ 管理员、金币 1234 / 收入 56 / 天数 7 / 价格 2000000/500000/300000/300000、
/// `$GUARDRULE` 常量「无效」、`$DATETIME` 宿主注入）。
#[test]
fn castle_official_main_page_expands_all_tokens() {
    use mir2_script::engine::ScriptPlayer;
    let mut rng = |_v: i32| 0;
    let mut p = official_player(&mut rng);
    let page = "<$OWNERGUILD>|<$LORD>|<$CASTLEGOLD>|<$TODAYINCOME>|<$DATETIME>|\
                <$CASTLEGETDAYS>|<$REPAIRDOORGOLD>|<$REPAIRWALLGOLD>|<$ARCHERFEE>|\
                <$GUARDFEE>|<$GUARDRULE>|<$CASTLEDOORSTATE>";
    assert_eq!(
        p.line_variable_text(page),
        "游戏管理|管理员|1234|56|2026-10-25 20:00:00|7|2000000|500000|300000|300000|无效|closed"
    );
    assert!(!p.take_say_abort(), "有门实体 ⇒ 不中止");
}

/// **A2（牙①）**：`$CASTLEDOORSTATE` 在 `MainDoor.BaseObject == null`（= 本快照
/// `main_door_state == None`）时 oracle **无 null 检查** ⇒ `NullReferenceException` ⇒ 整页中止：
/// 展开器置 `say_abort`，引擎在发说词前取走并 `return`（`crates/script` 的三处说词点）。
#[test]
fn castle_official_door_state_without_door_aborts_page() {
    use mir2_script::engine::ScriptPlayer;
    let mut rng = |_v: i32| 0;
    let mut p = official_player(&mut rng);
    p.vars.castle.as_mut().unwrap().main_door_state = None;
    let _ = p.line_variable_text("城门当前状态为:<$CASTLEDOORSTATE>\\");
    assert!(p.take_say_abort(), "缺门 ⇒ NRE 等价 ⇒ 必须置中止信号");
    assert!(!p.take_say_abort(), "中止信号是一次性的（引擎取走即清零）");
}

/// **A3（牙②）**：`CastleOfficial.cs:36-40`——`Castle == null` ⇒ **整行** `"????"`
/// （不是只换该 token；base 已替换的其它变量一起丢）。
#[test]
fn castle_official_without_castle_replaces_whole_line() {
    use mir2_script::engine::ScriptPlayer;
    let mut rng = |_v: i32| 0;
    let mut p = official_player(&mut rng);
    p.vars.castle = None;
    assert_eq!(p.line_variable_text("甲<$CASTLEGOLD>乙"), "????");
    // 非 CastleOfficial 类没有这层覆写 ⇒ `$CASTLEGOLD` 原样保留。
    let mut q = official_player(&mut rng);
    q.vars.castle_official = false;
    q.vars.castle = None;
    assert_eq!(
        q.line_variable_text("甲<$CASTLEGOLD>乙"),
        "甲<$CASTLEGOLD>乙"
    );
}

/// **A4（牙③）**：`$CASTLEWARDATE` / `$LISTOFWAR` 的**整行替换**（`NormNpc.cs:805-877`）：
/// 申请表空 ⇒ `"暂时没有行会攻城 .\\ \\<返回/@main>"`；战时 ⇒ `"正在攻城当中.\\ \\<返回/@main>"`。
#[test]
fn castle_official_aboutwar_whole_line_rules() {
    use mir2_script::engine::ScriptPlayer;
    let mut rng = |_v: i32| 0;
    let mut p = official_player(&mut rng);
    assert_eq!(
        p.line_variable_text("下一次战役记录在:<$CASTLEWARDATE> , 20:00开始。"),
        "暂时没有行会攻城 .\\ \\<返回/@main>"
    );
    assert_eq!(
        p.line_variable_text("<$LISTOFWAR> \\ <返回/@aboutwar>"),
        "暂时没有行会攻城 .\\ \\<返回/@main>"
    );
    p.vars.castle.as_mut().unwrap().under_war = true;
    assert_eq!(
        p.line_variable_text("下一次战役记录在:<$CASTLEWARDATE>"),
        "正在攻城当中.\\ \\<返回/@main>"
    );
}

/// **A5**：宿主 `castle_vars_for` = `CastleMgr.InCastleWarArea(this)`（`NormNpc.cs:1674-1678`）：
/// 皇宫图上的实体 ⇒ 命中该城并按 `CastleOfficial.cs:33-91` 取快照；图外实体 ⇒ `None`
/// （= oracle `Castle == null`）。
///
/// 牙④：把 `castle_war_area_at` 换成「取 castles[0]」⇒ 图外实体也命中 ⇒ 断言红。
#[test]
fn castle_vars_for_resolves_by_war_area_not_by_index() {
    let mut world = mir2_world::World::with_maps(vec![(16, 16), (16, 16)]);
    let mut castle = mir2_world::castle::UserCastle::new(
        "沙巴克",
        mir2_world::castle::CastleSiegeConfig::default(),
    );
    castle.palace_map_id = Some(0); // 皇宫图
    castle.total_gold = 777;
    castle.today_income = 88;
    castle.change_date = Some("2026-10-20 19:00:00".to_string());
    castle.own_guild = Some(String::new());
    castle.main_door = Some(mir2_world::castle::CastleUnit {
        entity_id: Some(1),
        x: 5,
        y: 5,
        death: false,
        opened: true,
        ..mir2_world::castle::CastleUnit::default()
    });
    world.castles.push(castle);
    world.open_session("甲");
    let in_palace = world
        .enter_map("甲", play_entity("甲", 5, 6), 0)
        .expect("皇宫图实体");
    world.open_session("乙");
    let elsewhere = world
        .enter_map("乙", play_entity("乙", 5, 6), 1)
        .expect("图外实体");

    let v = super::castle_vars_for(&world, in_palace, (2026, 10, 25)).expect("皇宫图命中该城");
    assert_eq!(v.name, "沙巴克");
    assert_eq!(v.own_guild, "");
    assert!(v.lord.is_none(), "R-0：MasterGuild 恒 None ⇒ $LORD=管理员");
    assert_eq!(v.total_gold, 777);
    assert_eq!(v.today_income, 88);
    assert_eq!(v.main_door_state, Some("opened"), "IsOpened ⇒ opened");
    assert!(!v.under_war);
    assert!(v.war_date_text.is_empty(), "申请表空 ⇒ GetWarDate() 空");
    assert_eq!(v.get_days, Some(5), "2026-10-20 → 2026-10-25 = 5 天");
    assert!(
        super::castle_vars_for(&world, elsewhere, (2026, 10, 25)).is_none(),
        "图外 ⇒ Castle == null"
    );
}

/// **F1（review `6c510095`）**：`$CASTLEWARDATE` 在 `Castle == null` 时取 `CastleMgr.GetCastle(0)`
/// **并写回**（`NormNpc.cs:805-818` 是字段赋值，跨次对话保留）⇒ 部署的 `老人-3`（`@aboutwar`）与
/// `比奇国王-0122` 这类「不在战区」的 NPC 也要展开成整行替换，而不是把字面 token 留在页面上。
///
/// 牙：去掉 `castle_zero` 回退（只保留 `if let Some(c) = castle`）⇒ 首断言红。
#[test]
fn castle_wardate_falls_back_to_castle_zero_and_writes_back() {
    use mir2_script::engine::ScriptPlayer;
    let mut rng = |_v: i32| 0;
    let mut p = official_player(&mut rng);
    p.vars.castle_official = false;
    p.vars.castle = None;
    p.vars.npc_actor_id = 42;
    let mut zero = castle_vars_fixture();
    zero.under_war = false;
    zero.war_date_text = String::new();
    p.vars.castle_zero = Some(zero);
    assert_eq!(
        p.line_variable_text("下一次战役记录在:<$CASTLEWARDATE> , 20:00开始。"),
        "暂时没有行会攻城 .\\ \\<返回/@main>",
        "Castle==null ⇒ GetCastle(0) ⇒ 申请表空 ⇒ 整行替换"
    );
    assert!(p.vars.castle.is_some(), "回退必须写回 NPC 的 Castle 字段");
    assert!(
        p.vars.castle_zero_sticky.borrow().contains(&42),
        "写回要按 NPC ActorId 记账（跨次对话粘滞）"
    );

    // 没有 0 号城 ⇒ oracle 的 `if (Castle != null)` 为假 ⇒ **不替换**（token 原样）。
    let mut q = official_player(&mut rng);
    q.vars.castle_official = false;
    q.vars.castle = None;
    q.vars.castle_zero = None;
    assert_eq!(
        q.line_variable_text("记录:<$CASTLEWARDATE>"),
        "记录:<$CASTLEWARDATE>"
    );
}

/// **F3（review `6c510095`）**：`$CASTLECHANGEDATE`（`NormNpc.cs:856-870`，部署 `老人-3`
/// `[@WarStatus]` 用到）：`Castle != null` ⇒ `ChangeDate.ToString("yyyy-MM-dd HH:mm:ss")`，
/// 否则 `"????"`（**无** 0 号城回退）。
#[test]
fn castle_changedate_formats_or_question_marks() {
    use mir2_script::engine::ScriptPlayer;
    let mut rng = |_v: i32| 0;
    let mut p = official_player(&mut rng);
    p.vars.castle_official = false;
    assert_eq!(
        p.line_variable_text("占领日期:<$CASTLECHANGEDATE>"),
        "占领日期:1998-01-01 00:00:00"
    );
    p.vars.castle.as_mut().unwrap().change_date_text = None;
    assert_eq!(
        p.line_variable_text("占领日期:<$CASTLECHANGEDATE>"),
        "占领日期:????"
    );
}

/// **Minor ③（返工 2）**：`$CASTLECHANGEDATE` 的**非零时分秒**必须原样（`ChangeDate` 是
/// `DateTime`，oracle `ToString("yyyy-MM-dd HH:mm:ss")`）——钉住「去掉 `{h:02}` 补零也绿」的缺口。
#[test]
fn castle_changedate_keeps_nonzero_time_components() {
    use mir2_script::engine::ScriptPlayer;
    let mut rng = |_v: i32| 0;
    let mut p = official_player(&mut rng);
    p.vars.castle_official = false;
    p.vars.castle.as_mut().unwrap().change_date_text = Some("1998-01-01 05:06:07".to_string());
    assert_eq!(
        p.line_variable_text("占领日期:<$CASTLECHANGEDATE>"),
        "占领日期:1998-01-01 05:06:07"
    );
}

/// **Minor ③ 的第二半**：宿主格式化 `Castle.ChangeDate` —— 日期补零 + 缺时间补 `00:00:00`
/// + 已有时间按 `HH:MM:SS` 补零（`CastleConf.cs:19-21` 的 `DateTime.Parse` 语义）。
#[test]
fn change_date_text_pads_date_and_time() {
    assert_eq!(
        super::change_date_text("1998-1-2 5:6:7"),
        Some("1998-01-02 05:06:07".to_string())
    );
    assert_eq!(
        super::change_date_text("1998-01-02"),
        Some("1998-01-02 00:00:00".to_string())
    );
    assert_eq!(super::change_date_text("not-a-date"), None);
}

// ---------------------------------------------------------------------------
// **卡 `openmir2-castle-official-actions`（tier R，构造级）**：动作 switch
// ---------------------------------------------------------------------------

fn action_world() -> (mir2_world::World, i32, i32) {
    let mut world = mir2_world::World::with_maps(vec![(16, 16)]);
    let mut door = mir2_world::Entity::new(0, "沙巴克城门".to_string(), 5, 5);
    door.race = 110;
    let door_id = world.add_entity(door);
    world.open_session("甲");
    let a_id = world
        .enter_map("甲", play_entity("甲", 6, 5), 0)
        .expect("甲 进图");
    let mut castle = mir2_world::castle::UserCastle::new(
        "沙巴克",
        mir2_world::castle::CastleSiegeConfig::default(),
    );
    castle.castle_map_id = Some(0);
    castle.total_gold = 3_000_000;
    castle.main_door = Some(mir2_world::castle::CastleUnit {
        entity_id: Some(door_id),
        x: 5,
        y: 5,
        hp: 9_999,
        max_hp: 9_999,
        ..mir2_world::castle::CastleUnit::default()
    });
    world.castles.push(castle);
    (world, door_id, a_id)
}

/// **A-O1**：`@openmaindoor`/`@closemaindoor`（`CastleOfficial.cs:199-206`）⇒
/// `MainDoorControl(false/true)` ⇒ `CastleDoor.Open/Close` 的 `SM_DIGUP`/`SM_DIGDOWN`
/// 发给 ±12 格内玩家，且 `SetMapXyFlag` 落到该城地图；两次都要求回 `@main`。
///
/// 牙：把 `close` 取反 ⇒ 第一条帧 ident 断言红；把 `say_main` 改回 `true` ⇒ 本用例红（F1 的钉）。
#[test]
fn castle_official_action_open_close_emits_door_frames() {
    let (mut world, door, a_id) = action_world();
    // **F1（review `e7f076f4`）**：展示层非零 —— `RM_DIGUP` 的 `Series=MakeWord(Dir,Light)`、
    // 尾 `Param1=GetFeature` 必须取展示层，不得写死 0。
    let mut displays = downlink::DisplayTable::new();
    let d = downlink::ActorDisplay {
        dir: 0,
        light: 9,
        feature: 0x1234,
        feature_ex: 0,
        status: 0,
        name_color: 255,
        show_name: Vec::new(),
    };
    displays.set(door, d);
    let out = super::castle_official_action_exec(
        &mut world,
        &displays,
        0,
        0,
        "@openmaindoor",
        (2_000_000, 500_000),
        (0xDB, 0xFF),
        (0xFF, 0x38),
    )
    .expect("动作已识别");
    assert!(
        !out.say_main,
        "F1：open/close 不得回 @main（@main 页由脚本 goto 产生，回了就重复）"
    );
    assert!(
        out.sysmsg.is_none(),
        "open/close 无 SysMsg（脚本 MESSAGEBOX 负责文案）"
    );
    let digup = out
        .frames
        .iter()
        .find(|(r, b)| {
            *r == a_id && u16::from_le_bytes([b[4], b[5]]) == mir2_protocol::messages::SM_DIGUP
        })
        .map(|(_, b)| b.clone())
        .expect("开门必须有 SM_DIGUP");
    // 尾是 `MessageBodyWL` 的 **EDCode 编码**（`sm_digup` → `edcode::encode(&wl)`）⇒ 先解码再读。
    let tail = mir2_protocol::edcode::decode(&digup[12..]);
    assert_eq!(
        i32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]),
        0x1234,
        "F1：尾 Param1 = GetFeature（展示层），不得写死 0"
    );
    assert_eq!(
        u16::from_le_bytes([digup[10], digup[11]]),
        u16::from(7u8) | (u16::from(9u8) << 8),
        "F1：Series = MakeWord(Dir=7, Light=9)，Light 不得写死 0"
    );
    assert!(world.castles[0].main_door.as_ref().expect("门").opened);

    let out = super::castle_official_action_exec(
        &mut world,
        &displays,
        0,
        0,
        "@closemaindoor",
        (2_000_000, 500_000),
        (0xDB, 0xFF),
        (0xFF, 0x38),
    )
    .expect("动作已识别");
    assert!(!out.say_main, "F1：close 也不得回 @main");
    assert!(out.frames.iter().any(|(r, b)| *r == a_id
        && u16::from_le_bytes([b[4], b[5]]) == mir2_protocol::messages::SM_DIGDOWN));
    assert!(!world.castles[0].main_door.as_ref().expect("门").opened);
}

/// **A-O2**：`@repairdoornow`/`@repairwallnowN`（`:207-226`）——`official_repair_door/wall` 的价格门
/// 与 60s 严格 `>` 门；成功才扣 `TotalGold`，文案绿/红分明。
///
/// 牙：把 `res.message.starts_with("城内资金不足")` 的颜色判定取反 ⇒ 红/绿断言红。
#[test]
fn castle_official_action_repair_gold_and_messages() {
    let (mut world, _door, _) = action_world();
    let displays = downlink::DisplayTable::new();
    // 钱不足 ⇒ 红字、不扣款。
    world.castles[0].total_gold = 10;
    let out = super::castle_official_action_exec(
        &mut world,
        &displays,
        0,
        100_000,
        "@repairdoornow",
        (2_000_000, 500_000),
        (0xDB, 0xFF),
        (0xFF, 0x38),
    )
    .expect("动作已识别");
    assert_eq!(out.sysmsg, Some("城内资金不足!!!"));
    assert_eq!(out.sysmsg_color, (0xFF, 0x38), "资金不足用红");
    assert_eq!(world.castles[0].total_gold, 10);

    // 钱够 + 门已满血 ⇒ 「城门不需要修理!!!」（绿）、不扣款。
    world.castles[0].total_gold = 3_000_000;
    let out = super::castle_official_action_exec(
        &mut world,
        &displays,
        0,
        100_000,
        "@repairdoornow",
        (2_000_000, 500_000),
        (0xDB, 0xFF),
        (0xFF, 0x38),
    )
    .expect("动作已识别");
    assert_eq!(out.sysmsg, Some("城门不需要修理!!!"));
    assert_eq!(out.sysmsg_color, (0xDB, 0xFF), "不需要修也用绿");
    assert_eq!(world.castles[0].total_gold, 3_000_000);
}

/// **A-O3**：N5（T-40）`@hirearchernowN`/`@hireguardnowN` ⇒ oracle `Substring` 恒越界被吞
/// ⇒ **零帧、零状态、零提示**，只登记日志；未命中标签 ⇒ `None`。
///
/// 牙：让 `HireUnreachable` 产出 `say_main=true` ⇒ 断言红。
#[test]
fn castle_official_action_hire_is_n5_noop() {
    let (mut world, _door, _) = action_world();
    let displays = downlink::DisplayTable::new();
    for label in ["@hirearchernow1", "@hirearchernow12", "@hireguardnow3"] {
        let out = super::castle_official_action_exec(
            &mut world,
            &displays,
            0,
            0,
            label,
            (2_000_000, 500_000),
            (0xDB, 0xFF),
            (0xFF, 0x38),
        )
        .expect("N5 支已识别");
        assert!(out.frames.is_empty(), "{label} 不得产帧");
        assert!(out.sysmsg.is_none(), "{label} 不得有提示");
        assert!(!out.say_main, "{label} 不回 @main");
        assert_eq!(out.log, Some("N5"), "{label} 只登记");
    }
    assert!(
        super::castle_official_action_exec(
            &mut world,
            &displays,
            0,
            0,
            "@main",
            (2_000_000, 500_000),
            (0xDB, 0xFF),
            (0xFF, 0x38)
        )
        .is_none(),
        "非动作标签 ⇒ None（oracle switch 无该分支）"
    );
}

/// **A-O4（F2 + Minor）**：`@repairwallnow1` 对**受损的左墙**（`hp<max_hp`、`struck_tick` 已过 60s）
/// ⇒ 成功扣款 + **墙的 `RefStatus` 帧**（`SM_ALIVE`，`Recog` = **墙**的 ActorId）。
///
/// oracle：`UserCastle.cs:981/991` 从**墙**的 `RefStatus` 出帧；本实现经
/// [`super::castle_unit_entity_at`] 在门 + 三面墙里按坐标找绑定实体。
/// 牙：把该查找换回「只查 `main_door`」⇒ 帧集为空 ⇒ 本用例红（正是 review `4a165328` F2 的漏检点）。
#[test]
fn castle_official_action_repair_wall_emits_wall_frames_after_60s() {
    let (mut world, _door, a_id) = action_world();
    let displays = downlink::DisplayTable::new();
    // **Minor①（review `e7f076f4`）**：门与墙必须是**两个不同实体**，否则「帧的 Recog 必须是墙」
    // 分不出门/墙；墙坐标 (4,5) 与主门 (5,5) 不同 ⇒ 只查 main_door 的变异会找不到实体。
    let door_actor = world.castles[0]
        .main_door
        .as_ref()
        .and_then(|d| d.entity_id)
        .expect("门实体");
    let mut wall = mir2_world::Entity::new(0, "沙巴克左城墙".to_string(), 4, 5);
    wall.race = 111;
    let wall_actor = world.add_entity(wall);
    assert_ne!(wall_actor, door_actor, "Minor①：门与墙必须是不同实体");
    world.castles[0].left_wall = Some(mir2_world::castle::CastleUnit {
        entity_id: Some(wall_actor),
        x: 4,
        y: 5,
        hp: 5,
        max_hp: 10,
        struck_tick: 0,
        ..mir2_world::castle::CastleUnit::default()
    });
    let out = super::castle_official_action_exec(
        &mut world,
        &displays,
        0,
        100_000,
        "@repairwallnow1",
        (2_000_000, 500_000),
        (0xDB, 0xFF),
        (0xFF, 0x38),
    )
    .expect("动作已识别");
    assert_eq!(out.sysmsg, Some("修理成功。"));
    assert_eq!(out.sysmsg_color, (0xDB, 0xFF), "成功用绿");
    assert!(
        out.say_main,
        "repair 系列回 @main（`:207-226` 的 `GotoLable(@main)`）"
    );
    assert_eq!(
        world.castles[0].total_gold,
        3_000_000 - 500_000,
        "成功才扣 RepairWallPrice"
    );
    assert!(
        !out.frames.is_empty(),
        "F2：墙修复必须有帧（只查 main_door 会整帧丢失）"
    );
    assert!(
        out.frames
            .iter()
            .any(|(r, b)| *r == a_id && i32::from_le_bytes([b[0], b[1], b[2], b[3]]) == wall_actor),
        "帧的 Recog 必须是**墙**的 ActorId（不是门 {door_actor}）"
    );
}

/// **N2（merge_result `29e240b9` Minor）**：emit 的 `SM_ALIVE`（墙修复的 `RefStatus` 帧）
/// `CharDesc.Status` 与 `drain_monster_ref_msgs` 的 `RM_ALIVE` 支**同一来源**——展示层
/// `d.status`（无登记 ⇒ 0），不是实体 `char_status`。两格，且故意让实体源非零（`0x1234`）
/// 使两个源可分：
/// ① 展示层登记 `status=0x77` ⇒ 帧尾 Status=`0x77`；
/// ② 完全不登记展示层 ⇒ Status=`0`（不得回落实体 `char_status`）。
///
/// 牙：把 RM_ALIVE 支的 status 改回实体 `char_status` ⇒ 两格都 RED（N2 的钉）。
#[test]
fn castle_official_action_alive_status_uses_display_layer() {
    // —— 格①：展示层登记了 status（与实体源可分）——
    let (mut world, _door, a_id) = action_world();
    let mut wall = mir2_world::Entity::new(0, "沙巴克左城墙".to_string(), 4, 5);
    wall.race = 111;
    wall.char_status = 0x1234; // 实体源故意非零 ⇒ 改回实体源的变异必被识破
    let wall_actor = world.add_entity(wall);
    world.castles[0].left_wall = Some(mir2_world::castle::CastleUnit {
        entity_id: Some(wall_actor),
        x: 4,
        y: 5,
        hp: 5,
        max_hp: 10,
        struck_tick: 0,
        ..mir2_world::castle::CastleUnit::default()
    });
    let mut displays = downlink::DisplayTable::new();
    displays.set(
        wall_actor,
        downlink::ActorDisplay {
            dir: 0,
            light: 0,
            feature: 0x55,
            feature_ex: 0,
            status: 0x77,
            name_color: 255,
            show_name: Vec::new(),
        },
    );
    let out = super::castle_official_action_exec(
        &mut world,
        &displays,
        0,
        100_000,
        "@repairwallnow1",
        (2_000_000, 500_000),
        (0xDB, 0xFF),
        (0xFF, 0x38),
    )
    .expect("动作已识别");
    let alive = out
        .frames
        .iter()
        .find(|(r, b)| {
            *r == a_id
                && i32::from_le_bytes([b[0], b[1], b[2], b[3]]) == wall_actor
                && u16::from_le_bytes([b[4], b[5]]) == mir2_protocol::messages::SM_ALIVE
        })
        .map(|(_, b)| b.clone())
        .expect("墙修复必须出 SM_ALIVE");
    let tail = mir2_protocol::edcode::decode(&alive[12..]);
    assert_eq!(
        i32::from_le_bytes([tail[4], tail[5], tail[6], tail[7]]),
        0x77,
        "N2：SM_ALIVE 的 CharDesc.Status 取展示层 d.status（与 drain_monster_ref_msgs 同源）"
    );
    assert_eq!(
        i32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]),
        0x55,
        "Feature 仍取展示层（F1 口径不变）"
    );

    // —— 格②：展示层未登记 ⇒ Status=0（不得回落实体 char_status）——
    let (mut world2, _door2, a_id2) = action_world();
    let mut wall2 = mir2_world::Entity::new(0, "沙巴克左城墙".to_string(), 4, 5);
    wall2.race = 111;
    wall2.char_status = 0x1234;
    let wall_actor2 = world2.add_entity(wall2);
    world2.castles[0].left_wall = Some(mir2_world::castle::CastleUnit {
        entity_id: Some(wall_actor2),
        x: 4,
        y: 5,
        hp: 5,
        max_hp: 10,
        struck_tick: 0,
        ..mir2_world::castle::CastleUnit::default()
    });
    let out2 = super::castle_official_action_exec(
        &mut world2,
        &downlink::DisplayTable::new(),
        0,
        100_000,
        "@repairwallnow1",
        (2_000_000, 500_000),
        (0xDB, 0xFF),
        (0xFF, 0x38),
    )
    .expect("动作已识别");
    let alive2 = out2
        .frames
        .iter()
        .find(|(r, b)| {
            *r == a_id2
                && i32::from_le_bytes([b[0], b[1], b[2], b[3]]) == wall_actor2
                && u16::from_le_bytes([b[4], b[5]]) == mir2_protocol::messages::SM_ALIVE
        })
        .map(|(_, b)| b.clone())
        .expect("无展示层登记也要出 SM_ALIVE");
    let tail2 = mir2_protocol::edcode::decode(&alive2[12..]);
    assert_eq!(
        i32::from_le_bytes([tail2[4], tail2[5], tail2[6], tail2[7]]),
        0,
        "N2 退化格：展示层未登记 ⇒ Status=0，不得回落实体 char_status"
    );
}
