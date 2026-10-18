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
        for (id, m, x, y, hp, lv, exp, vis) in world.snapshot_sorted() {
            out.push_str(&format!("ent {id} {m} {x} {y} {hp} {lv} {exp} vis"));
            for (target, flag) in vis {
                out.push_str(&format!(" {target}:{flag}"));
            }
            out.push('\n');
        }
    }
    out
}

/// 逐行比对某场景的 Rust 输出与 C# oracle 金标准。
fn assert_scenario_matches_golden(name: &str) {
    let mine = run_scenario(name);
    let golden_path = parity_dir()
        .join("golden")
        .join(name.replace(".json", ".txt"));
    let golden = std::fs::read_to_string(&golden_path).unwrap();
    if mine != golden {
        for (i, (a, b)) in mine.lines().zip(golden.lines()).enumerate() {
            if a != b {
                panic!(
                    "{name} 位置/可见集与 C# oracle 不一致，首个差异在第 {} 行:\n  rust  = {a}\n  csharp= {b}",
                    i + 1
                );
            }
        }
        panic!(
            "{name} 行数不一致: rust={} csharp={}",
            mine.lines().count(),
            golden.lines().count()
        );
    }
}

/// 玩家 `SearchTime = 1000`（`PlayObject.cs:900`）⇒ 200ms tick 下**首次扫描在第 6 tick**
/// （1200ms，严格大于 1000）。金标准自检要读「首次扫描后」的行，而不是 tick 1。
const FIRST_PLAYER_SCAN_TICK: usize = 6;

/// 取金标准里某实体在**指定 tick** 的行（`ent <id> ...`）。
fn golden_entity_line_at_tick(golden: &str, id: i32, tick_no: usize) -> String {
    let mut current = 0usize;
    for line in golden.lines() {
        if let Some(rest) = line.strip_prefix("tick ") {
            current = rest
                .split_whitespace()
                .next()
                .and_then(|t| t.parse().ok())
                .unwrap_or(0);
            continue;
        }
        if current == tick_no && line.starts_with(&format!("ent {id} ")) {
            return line.to_string();
        }
    }
    panic!("金标准第 {tick_no} tick 里应有 {id} 号实体")
}

#[test]
fn world_positions_and_visible_sets_match_csharp() {
    assert_scenario_matches_golden("basic.json");
}

/// 判据② ⑫：玩家 `ViewRange=12`（`PlayObject.cs:904`）的 AOI 窗口 = **25×25**，与 oracle 金标准逐项相同。
///
/// 这是卡 `openmir2-client-e2e-enter-gaps` 缺口③「同屏没怪」的几何根因所在：
/// 玩家窗口若停在 `BaseObject.cs:383` 的缺省 5（=11×11），Chebyshev **6..12** 这一圈怪/玩家
/// 不进 `VisibleActors` ⇒ 客户端同屏少怪。本场景把环形边界钉死在 **12 可见 / 13 不可见**。
///
/// 金标准由 `csharp/WorldGolden`（逐字拷贝 `BaseObject.ViewRange.cs`）产出：
/// ```bash
/// dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -- aoi_viewrange12.json
/// ```
#[test]
fn world_aoi_viewrange12_ring_matches_csharp() {
    assert_scenario_matches_golden("aoi_viewrange12.json");
}

/// 金标准自检（fail-closed）：`aoi_viewrange12.txt` 第 1 tick 里玩家（1 号）必须**看得见**
/// Chebyshev=12 的怪（4 号 `(44,32)`、6 号对角 `(44,44)`、8 号 `(32,20)`、10 号 `(44,20)`），
/// 且**看不见** Chebyshev=13 的怪（5/7/9 号）。
///
/// ⇒ 若有人把 golden 用 `view_range=5` 重生成（或改坏口算），本用例必红；
/// 它同时给 `world_aoi_viewrange12_ring_matches_csharp` 当**边界正向对照**（防 off-by-one 假绿）。
#[test]
fn golden_viewrange12_ring_boundary_is_pinned() {
    let golden = std::fs::read_to_string(parity_dir().join("golden/aoi_viewrange12.txt")).unwrap();
    let line1 = golden_entity_line_at_tick(&golden, 1, FIRST_PLAYER_SCAN_TICK);
    for id in [4, 6, 8, 10] {
        assert!(
            line1.contains(&format!(" {id}:2")),
            "金标准自检失效：Chebyshev=12 的 {id} 号应可见，实际首 tick 行 = {line1}"
        );
    }
    for id in [5, 7, 9] {
        assert!(
            !line1.contains(&format!(" {id}:")),
            "金标准自检失效：Chebyshev=13 的 {id} 号不应可见，实际首 tick 行 = {line1}"
        );
    }
}

/// 红检（自动）③：把玩家 1 号的 `view_range` 从 **12 压回缺省 5**（= 修复前 `new_player_entity`
/// 的窗口口径，见 `bins/game-svr/src/main.rs` 的 `new_player_entity`）⇒ Chebyshev 6..12 的怪
/// 必须从它的可见集消失 ⇒ 与金标准**首次扫描 tick** 行必不同，且 3/4 号（距离 7/12）必须不再出现。
#[test]
fn redcheck_viewrange12_ring_collapsed_to_default_must_differ() {
    let golden = std::fs::read_to_string(parity_dir().join("golden/aoi_viewrange12.txt")).unwrap();
    let golden_line1 = golden_entity_line_at_tick(&golden, 1, FIRST_PLAYER_SCAN_TICK);
    let text =
        std::fs::read_to_string(parity_dir().join("scenarios/aoi_viewrange12.json")).unwrap();
    let sc: Scenario = serde_json::from_str(&text).unwrap();
    let mut world = World::new(sc.width, sc.height);
    for spec in &sc.entities {
        let mut e = Entity::new(spec.id, format!("e{}", spec.id), spec.x, spec.y);
        e.id = spec.id;
        // 除 1 号外逐字照抄场景（含 race/want_ref_msg：这两者决定过滤器是否放行怪物）。
        e.view_range = if spec.id == 1 { 5 } else { spec.view_range };
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
        world.add_entity(e);
    }
    for _ in 0..FIRST_PLAYER_SCAN_TICK {
        world.tick();
    }
    let mine = world
        .snapshot_sorted()
        .into_iter()
        .find(|(id, _, _, _, _, _, _, _)| *id == 1)
        .map(|(id, m, x, y, hp, lv, exp, vis)| {
            let mut l = format!("ent {id} {m} {x} {y} {hp} {lv} {exp} vis");
            for (t, f) in vis {
                l.push_str(&format!(" {t}:{f}"));
            }
            l
        })
        .unwrap();
    assert_ne!(
        mine, golden_line1,
        "红检失效：把玩家 view_range 压回 5 后，1 号首 tick 可见集仍与金标准相同"
    );
    for id in [3, 4, 6] {
        assert!(
            !mine.contains(&format!(" {id}:")),
            "红检失效：view_range=5 时 Chebyshev 6..12 的 {id} 号不应还在可见集里，实际 = {mine}"
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
    for _ in 0..FIRST_PLAYER_SCAN_TICK {
        world.tick();
    }
    let mine: Vec<String> = world
        .snapshot_sorted()
        .into_iter()
        .map(|(id, _m, x, y, _hp, _lv, _exp, vis)| {
            let mut l = format!("ent {id} {_m} {x} {y} {_hp} {_lv} {_exp} vis");
            for (t, f) in vis {
                l.push_str(&format!(" {t}:{f}"));
            }
            l
        })
        .collect();
    let golden_line = golden_entity_line_at_tick(&golden, 1, FIRST_PLAYER_SCAN_TICK);
    let mine_line = mine
        .iter()
        .find(|l| l.starts_with("ent 1 "))
        .expect("1 号实体行");
    assert_ne!(
        mine_line.as_str(),
        golden_line,
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
        .find(|(id, _, _, _, _, _, _, _)| *id == 1)
        .map(|(id, _m, x, y, _hp, _lv, _exp, vis)| {
            let mut l = format!("ent {id} {_m} {x} {y} {_hp} {_lv} {_exp} vis");
            for (t, f) in vis {
                l.push_str(&format!(" {t}:{f}"));
            }
            l
        })
        .unwrap();
    assert_ne!(mine, golden_first, "红检失效：改了坐标但首行与金标准相同");
}
