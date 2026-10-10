//! 判据③（M2 下半程·最小切片）：会话进世界 + 掉落系统的 golden 对拍。
//!
//! 场景 `session.json` 覆盖：进图 → 移动（含被怪物挡住的一步）→ 怪物生成（掉落预算在此消耗）
//! → 脚本化击杀（掉落落地 + 金币）→ 切图 → 小退 → 再进（新 `ActorId`）。
//! 金标准由 `tests/parity/csharp/WorldGolden` 产出（会话/移动/AOI 与掉落链均为 oracle 逐字拷贝）。
//!
//! 本切片**不含战斗结算**（击杀用脚本化 `kill` 触发），因此随机流只由掉落预算消耗——
//! 这是"最小切片"的刻意取舍：把 RNG 调用面收到一个系统，先保证逐行一致。
//!
//! 生成金标准：
//! ```bash
//! dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- session
//! ```

use std::path::{Path, PathBuf};

use mir2_data::models::MonsterDropItem;
use mir2_world::{Command, Entity, World};

fn world_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("world")
}

#[derive(serde::Deserialize)]
struct Scenario {
    maps: Vec<MapSpec>,
    ticks: u64,
    #[serde(default)]
    extended: bool,
    #[serde(default = "default_add_value")]
    mon_random_add_value: i32,
    #[serde(default)]
    item_catalog: Vec<ItemSpec>,
    #[serde(default)]
    sessions: Vec<SessionSpec>,
    #[serde(default)]
    monsters: Vec<MonsterSpec>,
    #[serde(default)]
    commands: Vec<CommandSpec>,
}

fn default_add_value() -> i32 {
    40
}

#[derive(serde::Deserialize)]
struct MapSpec {
    width: i32,
    height: i32,
}

#[derive(serde::Deserialize)]
struct ItemSpec {
    name: String,
    std_mode: u8,
    shape: u8,
    dura_max: u16,
}

#[derive(serde::Deserialize)]
struct SessionSpec {
    chr: String,
    id: i32,
    map: usize,
    x: i32,
    y: i32,
    level: u8,
    hp: u16,
    max_hp: u16,
    dc: i32,
    ac: i32,
    hit_point: u8,
    speed_point: u8,
    job: u8,
    exp: i32,
    max_exp: i32,
    view_range: i32,
    race: u8,
}

impl SessionSpec {
    fn entity(&self, id: i32, x: i32, y: i32) -> Entity {
        let mut e = Entity::new(id, self.chr.clone(), x, y);
        e.map_id = self.map;
        e.level = self.level;
        e.hp = self.hp;
        e.max_hp = self.max_hp;
        e.dc = self.dc;
        e.ac = self.ac;
        e.hit_point = self.hit_point;
        e.speed_point = self.speed_point;
        e.job = self.job;
        e.exp = self.exp;
        e.max_exp = self.max_exp;
        e.view_range = self.view_range;
        e.race = self.race;
        e
    }
}

#[derive(serde::Deserialize)]
struct MonsterSpec {
    id: i32,
    name: String,
    map: usize,
    x: i32,
    y: i32,
    level: u8,
    exp: i32,
    hp: u16,
    dc: u16,
    ac: u16,
    undead: u8,
    drop: Vec<DropSpec>,
}

#[derive(serde::Deserialize)]
struct DropSpec {
    sel_point: i32,
    max_point: i32,
    item_name: String,
    count: i32,
}

#[derive(serde::Deserialize)]
struct CommandSpec {
    tick: u64,
    kind: String,
    #[serde(default)]
    id: i32,
    #[serde(default)]
    dx: i32,
    #[serde(default)]
    dy: i32,
    #[serde(default)]
    chr: String,
    #[serde(default)]
    map: usize,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
}

fn run_scenario(name: &str) -> String {
    let text = std::fs::read_to_string(world_dir().join("scenarios").join(name)).unwrap();
    let sc: Scenario = serde_json::from_str(&text).unwrap();

    let sizes: Vec<(i32, i32)> = sc.maps.iter().map(|m| (m.width, m.height)).collect();
    let mut world = World::with_maps(sizes);
    world.mon_random_add_value = sc.mon_random_add_value;
    world.item_catalog = sc
        .item_catalog
        .iter()
        .map(|i| mir2_data::models::StdItem {
            name: i.name.clone(),
            std_mode: i.std_mode,
            shape: i.shape,
            dura_max: i.dura_max,
            ..Default::default()
        })
        .collect();

    for s in &sc.sessions {
        world.open_session(&s.chr);
    }

    // 会话当前地图（切图会改；再进必须用**当前**地图 —— C# 侧同样把当前地图记在会话上）
    let mut cur_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for s in &sc.sessions {
        cur_map.insert(s.chr.clone(), s.map);
    }

    let mut out = String::new();
    for tick in 1..=sc.ticks {
        // 1) 会话操作（立即生效）+ 2) 世界命令（按场景顺序入队）
        for c in sc.commands.iter().filter(|c| c.tick == tick) {
            match c.kind.as_str() {
                "enter" => {
                    let s = sc.sessions.iter().find(|s| s.chr == c.chr).unwrap();
                    world
                        .enter_map(&s.chr, s.entity(s.id, s.x, s.y), s.map)
                        .expect("进图应成功");
                }
                "switch" => {
                    world
                        .switch_map(&c.chr, c.map, c.x, c.y)
                        .expect("切图应成功");
                    cur_map.insert(c.chr.clone(), c.map);
                }
                "softclose" => {
                    world.soft_close(&c.chr).expect("小退应成功");
                }
                "reenter" => {
                    let s = sc.sessions.iter().find(|s| s.chr == c.chr).unwrap();
                    let map = *cur_map.get(&c.chr).unwrap_or(&s.map);
                    world
                        .enter_map(&s.chr, s.entity(c.id, c.x, c.y), map)
                        .expect("再进应成功");
                }
                "spawn" => {
                    let m = sc.monsters.iter().find(|m| m.id == c.id).unwrap();
                    let drop_list: Vec<MonsterDropItem> = m
                        .drop
                        .iter()
                        .map(|d| MonsterDropItem {
                            sel_point: d.sel_point,
                            max_point: d.max_point,
                            item_name: d.item_name.clone(),
                            count: d.count,
                        })
                        .collect();
                    world.enqueue(Command::SpawnMonster {
                        id: m.id,
                        name: m.name.clone(),
                        x: m.x,
                        y: m.y,
                        map_id: m.map,
                        level: m.level,
                        exp: m.exp,
                        hp: m.hp,
                        dc: m.dc,
                        ac: m.ac,
                        undead: m.undead,
                        drop_list,
                    });
                }
                "walk" => world.walk(c.id, c.dx, c.dy),
                "kill" => world.kill(c.id),
                "leave" => world.leave(c.id),
                other => panic!("未知命令 {other}"),
            }
        }
        let rec = world.tick();

        out.push_str(&format!(
            "tick {} clock {}\n",
            world.tick_seq, world.clock_ms
        ));
        for (id, map, x, y, hp, level, exp, vis) in world.snapshot_sorted() {
            out.push_str(&format!("ent {id} {map} {x} {y} {hp} {level} {exp} vis"));
            for (target, flag) in vis {
                out.push_str(&format!(" {target}:{flag}"));
            }
            out.push('\n');
        }
        if sc.extended {
            for (id, placed, gold, killer, exp) in &rec.deaths {
                let k = killer
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-1".to_string());
                out.push_str(&format!("death {id} {placed} {gold} {k} {exp}\n"));
            }
            for (map, x, y, obj) in world.floor_items_sorted() {
                out.push_str(&format!("floor {map} {x} {y} {obj}\n"));
            }
        }
    }
    out
}

#[test]
fn session_and_drops_match_csharp() {
    let mine = run_scenario("session.json");
    let golden = std::fs::read_to_string(world_dir().join("golden/session.txt")).unwrap();
    if mine != golden {
        for (i, (a, b)) in mine.lines().zip(golden.lines()).enumerate() {
            if a != b {
                panic!(
                    "会话+掉落与 C# oracle 不一致，首个差异在第 {} 行:\n  rust  = {a}\n  csharp= {b}",
                    i + 1
                );
            }
        }
        panic!(
            "行数不一致: rust={} csharp={}",
            mine.lines().count(),
            golden.lines().count()
        );
    }
    assert!(mine.contains("floor "), "扩展场景必须产出地面物品行");
    assert!(mine.contains("death 2 "), "扩展场景必须产出死亡结算行");
}

/// 红检（自动）：把掉落的必掉率改掉 ⇒ 地面物品必须变化（证明判据真的在比对掉落）。
#[test]
fn redcheck_drop_change_must_alter_floor() {
    let golden = std::fs::read_to_string(world_dir().join("golden/session.txt")).unwrap();
    let baseline_floors: Vec<&str> = golden.lines().filter(|l| l.starts_with("floor ")).collect();
    // 用"必不掉"的掉落表重跑：地面物品应显著减少
    let text = std::fs::read_to_string(world_dir().join("scenarios/session.json")).unwrap();
    let mut sc: serde_json::Value = serde_json::from_str(&text).unwrap();
    for m in sc["monsters"].as_array_mut().unwrap() {
        for d in m["drop"].as_array_mut().unwrap() {
            d["sel_point"] = serde_json::json!(-1); // 永远不满足 ≤ sel_point
        }
    }
    let path = world_dir().join("scenarios/.session_redcheck.json");
    std::fs::write(&path, serde_json::to_string_pretty(&sc).unwrap()).unwrap();
    let mine = run_scenario(".session_redcheck.json");
    let _ = std::fs::remove_file(&path);
    let floors: Vec<&str> = mine.lines().filter(|l| l.starts_with("floor ")).collect();
    assert!(
        !baseline_floors.is_empty(),
        "金标准里应至少有 1 件地面物品（否则这条红检无意义）"
    );
    assert_ne!(
        floors.len(),
        baseline_floors.len(),
        "红检失效：改掉必掉率后地面物品数没变"
    );
}
