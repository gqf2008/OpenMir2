//! 判据②：同一地图、同一批实体、跑 N tick 后**位置与可见集**与 C# oracle 逐项相同。
//!
//! 金标准由 `tests/parity/csharp/WorldGolden` 产出：它把 oracle 的
//! `BaseObject.ViewRange.cs`（`SearchViewRange` / `SearchViewRangeDeath` / `UpdateVisibleGay`）与
//! `Envirnoment.cs`（`GetCellInfo` / `AddMapObject` / `MoveToMovingObject` / `CellMatch`）
//! **逐字拷贝**成桩，时钟桩成虚拟时钟（两侧都不读墙钟），再按同一顺序驱动。
//!
//! 生成命令：
//! ```bash
//! dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -- \
//!   mir2-rs/tests/parity/world/scenarios/basic.json \
//!   mir2-rs/tests/parity/world/golden/basic.txt
//! ```

use std::path::{Path, PathBuf};

use mir2_world::{Entity, World};

fn parity_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("world")
}

#[derive(serde::Deserialize)]
struct Scenario {
    width: i32,
    height: i32,
    ticks: u64,
    entities: Vec<EntitySpec>,
    commands: Vec<CommandSpec>,
}

#[derive(serde::Deserialize)]
struct EntitySpec {
    id: i32,
    x: i32,
    y: i32,
    #[serde(default = "default_view_range")]
    view_range: i32,
    #[serde(default)]
    race: u8,
    #[serde(default)]
    ghost: bool,
    #[serde(default)]
    death: bool,
    #[serde(default)]
    invisible: bool,
    #[serde(default)]
    fixed_hide_mode: bool,
    #[serde(default)]
    ob_mode: bool,
    #[serde(default = "no_master")]
    master: i32,
    #[serde(default)]
    nasty_mode: bool,
    #[serde(default)]
    want_ref_msg: bool,
}

fn default_view_range() -> i32 {
    5
}

fn no_master() -> i32 {
    -1
}

#[derive(serde::Deserialize)]
struct CommandSpec {
    tick: u64,
    kind: String,
    id: i32,
    #[serde(default)]
    dx: i32,
    #[serde(default)]
    dy: i32,
}

/// 与 C# `WorldGolden/Program.cs` **逐字一致**的输出格式。
fn run_scenario(name: &str) -> String {
    let text = std::fs::read_to_string(parity_dir().join("scenarios").join(name)).unwrap();
    let sc: Scenario = serde_json::from_str(&text).unwrap();

    let mut world = World::new(sc.width, sc.height);
    for spec in &sc.entities {
        let mut e = Entity::new(spec.id, format!("e{}", spec.id), spec.x, spec.y);
        e.id = spec.id;
        e.view_range = spec.view_range;
        e.race = spec.race;
        e.ghost = spec.ghost;
        e.death = spec.death;
        e.invisible = spec.invisible;
        e.fixed_hide_mode = spec.fixed_hide_mode;
        e.ob_mode = spec.ob_mode;
        e.master = if spec.master < 0 {
            None
        } else {
            Some(spec.master)
        };
        e.nasty_mode = spec.nasty_mode;
        e.want_ref_msg = spec.want_ref_msg;
        world.add_entity(e); // 入队 Enter（与 C# 驱动一致）
    }

    let mut out = String::new();
    for tick in 1..=sc.ticks {
        for c in sc.commands.iter().filter(|c| c.tick == tick) {
            match c.kind.as_str() {
                "walk" => world.walk(c.id, c.dx, c.dy),
                "leave" => world.leave(c.id),
                other => panic!("未知命令 {other}"),
            }
        }
        world.tick();

        out.push_str(&format!(
            "tick {} clock {}\n",
            world.tick_seq, world.clock_ms
        ));
        for (id, x, y, vis) in world.snapshot_sorted() {
            out.push_str(&format!("ent {id} {x} {y} vis"));
            for (target, flag) in vis {
                out.push_str(&format!(" {target}:{flag}"));
            }
            out.push('\n');
        }
    }
    out
}

#[test]
fn world_positions_and_visible_sets_match_csharp() {
    let mine = run_scenario("basic.json");
    let golden = std::fs::read_to_string(parity_dir().join("golden/basic.txt")).unwrap();
    if mine != golden {
        for (i, (a, b)) in mine.lines().zip(golden.lines()).enumerate() {
            if a != b {
                panic!(
                    "位置/可见集与 C# oracle 不一致，首个差异在第 {} 行:\n  rust  = {a}\n  csharp= {b}",
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
}

/// 红检（自动）①：把 1 号的 view_range +3 ⇒ 它的可见集必须与金标准不同。
#[test]
fn redcheck_view_range_change_must_alter_visible_set() {
    let golden = std::fs::read_to_string(parity_dir().join("golden/basic.txt")).unwrap();
    let text = std::fs::read_to_string(parity_dir().join("scenarios/basic.json")).unwrap();
    let sc: Scenario = serde_json::from_str(&text).unwrap();
    let mut world = World::new(sc.width, sc.height);
    for spec in &sc.entities {
        let mut e = Entity::new(spec.id, format!("e{}", spec.id), spec.x, spec.y);
        e.id = spec.id;
        e.view_range = if spec.id == 1 {
            spec.view_range + 3
        } else {
            spec.view_range
        };
        world.add_entity(e);
    }
    world.tick();
    let mine: Vec<String> = world
        .snapshot_sorted()
        .into_iter()
        .map(|(id, x, y, vis)| {
            let mut l = format!("ent {id} {x} {y} vis");
            for (t, f) in vis {
                l.push_str(&format!(" {t}:{f}"));
            }
            l
        })
        .collect();
    let golden_tick1: Vec<&str> = golden
        .lines()
        .skip(1)
        .take_while(|l| l.starts_with("ent "))
        .collect();
    let mine_line1 = &mine[0];
    assert_ne!(
        mine_line1.as_str(),
        golden_tick1[0],
        "红检失效：改了 view_range 但 1 号的可见集与金标准相同"
    );
}

/// 红检（自动）②：把 1 号位置改一格 ⇒ 第 1 tick 的位置行必须与金标准不同。
#[test]
fn redcheck_position_change_must_differ() {
    let golden = std::fs::read_to_string(parity_dir().join("golden/basic.txt")).unwrap();
    let golden_first = golden
        .lines()
        .find(|l| l.starts_with("ent 1 "))
        .expect("金标准里应有 1 号实体");
    let text = std::fs::read_to_string(parity_dir().join("scenarios/basic.json")).unwrap();
    let sc: Scenario = serde_json::from_str(&text).unwrap();
    let mut world = World::new(sc.width, sc.height);
    for spec in &sc.entities {
        let dx = if spec.id == 1 { 1 } else { 0 };
        let mut e = Entity::new(spec.id, format!("e{}", spec.id), spec.x + dx, spec.y);
        e.id = spec.id;
        world.add_entity(e);
    }
    world.tick();
    let mine = world
        .snapshot_sorted()
        .into_iter()
        .find(|(id, _, _, _)| *id == 1)
        .map(|(id, x, y, vis)| {
            let mut l = format!("ent {id} {x} {y} vis");
            for (t, f) in vis {
                l.push_str(&format!(" {t}:{f}"));
            }
            l
        })
        .unwrap();
    assert_ne!(mine, golden_first, "红检失效：改了坐标但首行与金标准相同");
}
