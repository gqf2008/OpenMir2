//! 世界 golden 对拍的**共用驱动**：读 `world/scenarios/*.json`，按与 C# 侧（`csharp/WorldGolden`）
//! 逐字对齐的顺序契约跑一遍，产出可逐行比对的文本。
//!
//! 顺序契约（两侧写死，改动必须两侧同批改）：
//! 1. 每 tick：`clock += 200` → 应用本 tick 的**会话操作**（enter/switch/softclose/reenter，立即生效）
//!    → 入队本 tick 的**世界命令**（spawn/walk/kill/attack/leave，按场景顺序 FIFO）→ 处理队列
//!    → 死亡结算（掉落落地 + `KillFunc` 发经验）→ 按插入顺序 AOI → 输出；
//! 2. 怪物掉落在地图**生成时**预算（`WorldServer.MonGen.cs:440` 的时机，RNG 在这里消耗）；
//! 3. `attack`：`GetBaseAttackPoewr()`（攻击力 RNG）→ `_Attack`（命中 + 护甲 RNG）→
//!    若目标 `Race > ActorRace.Play` 则 `SetLastHiter(攻击方)`（折叠 C# 的 `RM_STRUCK` 消息，
//!    见 `crates/world/src/combat.rs` 与 `csharp/WorldGolden/StubsCombat.cs` 的说明）。
//!
//! 输出行（`extended` 场景才有 `atk`/`death`/`floor`）：
//! ```text
//! tick <t> clock <ms>
//! ent <id> <map> <x> <y> <hp> <level> <exp> vis <目标id>:<flag> ...
//! atk <攻击方> <目标> <攻击力> <结算后目标HP>
//! death <id> <落地件数> <金币> <经验归属者|-1> <获得经验>
//! floor <map> <x> <y> <obj_id>
//! ```

use std::path::{Path, PathBuf};

use mir2_data::models::MonsterDropItem;
use mir2_world::{Entity, World};

pub fn world_dir() -> PathBuf {
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
    #[serde(default = "default_fix_exp")]
    high_level_kill_mon_fix_exp: bool,
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

fn default_fix_exp() -> bool {
    true
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
    /// 命中判定两侧；缺省 = `BaseObject` 构造的 5 / 15（`BaseObject.cs:386-387`）
    #[serde(default = "default_mon_hit_point")]
    hit_point: u8,
    #[serde(default = "default_mon_speed_point")]
    speed_point: u8,
    drop: Vec<DropSpec>,
}

fn default_mon_hit_point() -> u8 {
    5
}

fn default_mon_speed_point() -> u8 {
    15
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
    target: i32,
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

/// 跑一遍场景，返回与 C# 侧同格式的文本。
pub fn run_scenario(name: &str) -> String {
    let text = std::fs::read_to_string(world_dir().join("scenarios").join(name)).unwrap();
    let sc: Scenario = serde_json::from_str(&text).unwrap();

    let sizes: Vec<(i32, i32)> = sc.maps.iter().map(|m| (m.width, m.height)).collect();
    let mut world = World::with_maps(sizes);
    world.mon_random_add_value = sc.mon_random_add_value;
    world.combat_config.high_level_kill_mon_fix_exp = sc.high_level_kill_mon_fix_exp;
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
                // 把 actor 置为 Ghost（夹具级状态注入；对应 C# 夹具的 `ApplyGhost`，与 oracle 的
                // `Ghost = true` 同义）。会话操作阶段立即生效，与 C# 侧同序。
                "ghost" => {
                    if let Some(e) = world.entities.get_mut(c.id) {
                        e.ghost = true;
                    }
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
                    world.enqueue(mir2_world::Command::SpawnMonster {
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
                        hit_point: m.hit_point,
                        speed_point: m.speed_point,
                        drop_list,
                    });
                }
                "walk" => world.walk(c.id, c.dx, c.dy),
                "kill" => world.kill(c.id),
                "attack" => world.attack_cmd(c.id, c.target),
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
            for (attacker, target, outcome) in &rec.attacks {
                out.push_str(&format!(
                    "atk {attacker} {target} {} {}\n",
                    outcome.power, outcome.target_hp_after
                ));
            }
            for (id, placed, gold, hitter, exp) in &rec.deaths {
                let k = hitter
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
