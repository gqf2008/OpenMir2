//! D3 试验田门禁：**bevy_ecs 实现的掉落系统必须与既有实现、与 C# 金标准三方一致**，
//! 且不得依赖 `Query` 迭代序、不得引入并行。
//!
//! 四条断言对应 §4.2.2 的 DoD：
//! 1. `budget_matches_existing_implementation`：同快照下，bevy 系统的预算输出 ==
//!    `crates/formula`（既有实现的核心）直接调用的结果（掉落列表逐项相同）；
//! 2. `floor_placement_matches_csharp_golden`：bevy 系统的落地输出 == C# 金标准
//!    `tests/parity/world/golden/session.txt` 里的 `floor` 行（**与 C# 逐行一致**）；
//! 3. `output_does_not_depend_on_query_order`：把实体按**反序** spawn、只把 [`OrderIndex`]
//!    按 id 升序给系统，输出必须不变 —— 这条直接证明"顺序来自我们自己的索引，不来自 Query 迭代序"；
//! 4. `no_parallel_guard`：直接依赖白名单 + 源码扫描（禁 `MultiThreadedExecutor`/`bevy_tasks`/
//!    rayon/crossbeam 的并行 API）。

use std::fs;
use std::path::{Path, PathBuf};

use mir2_data::models::{MonsterDropItem, StdItem, STRING_GOLD_NAME};
use mir2_formula::drop::{mon_get_random_items, ItemNumberCounter, VecItemCatalog};
use mir2_shared::rng::RandomNumber;
use mir2_world_bevy::{
    plan_drops, plan_floor_placement, DropAt, DropSnapshot, MonsterSnap, PlaceSnapshot,
};

const SEED: i32 = 42;
const MON_RANDOM_ADD_VALUE: i32 = 40;

fn parity_world_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/world")
}

fn catalog() -> Vec<StdItem> {
    vec![StdItem {
        name: "金创药(小量)".to_string(),
        std_mode: 0,
        shape: 0,
        dura_max: 1,
        ..Default::default()
    }]
}

/// 与 `tests/parity/world/scenarios/session.json` 里那只怪物的掉落表一致。
fn drop_table() -> Vec<MonsterDropItem> {
    vec![
        MonsterDropItem {
            sel_point: 9,
            max_point: 10,
            item_name: STRING_GOLD_NAME.to_string(),
            count: 300,
        },
        MonsterDropItem {
            sel_point: 9,
            max_point: 10,
            item_name: "金创药(小量)".to_string(),
            count: 1,
        },
    ]
}

fn snapshot(reverse_insert: bool) -> DropSnapshot {
    let mut monsters = vec![
        MonsterSnap {
            id: 2,
            map_id: 0,
            x: 12,
            y: 10,
            drop_table: drop_table(),
        },
        MonsterSnap {
            id: 5,
            map_id: 0,
            x: 20,
            y: 20,
            drop_table: drop_table(),
        },
    ];
    if reverse_insert {
        // 只改"谁先被 spawn"，不改 OrderIndex（由 plan_drops 按传入顺序建立索引）
        monsters.reverse();
    }
    DropSnapshot {
        seed: SEED,
        mon_random_add_value: MON_RANDOM_ADD_VALUE,
        catalog: catalog(),
        monsters,
    }
}

/// 断言 1：预算输出 == 既有实现（`crates/formula` 的掉落公式）直接调用的结果。
#[test]
fn budget_matches_existing_implementation() {
    let snap = snapshot(false);
    let bevy_out = plan_drops(&snap);

    // 既有实现的核心：同一个种子、同一调用顺序
    let mut rng = RandomNumber::with_seed(SEED);
    let cat = catalog();
    let catalog_ref = VecItemCatalog { items: &cat };
    let mut item_number = ItemNumberCounter::default();
    let mut expected = Vec::new();
    for m in &snap.monsters {
        let out = mon_get_random_items(
            &m.drop_table,
            &catalog_ref,
            &mut rng,
            MON_RANDOM_ADD_VALUE,
            STRING_GOLD_NAME,
            &mut item_number,
        );
        expected.push((m.id, out.gold, out.items));
    }

    assert_eq!(bevy_out.len(), expected.len(), "怪物数量应一致");
    for (got, (id, gold, items)) in bevy_out.iter().zip(expected.iter()) {
        assert_eq!(
            got.id, *id,
            "输出顺序必须是 OrderIndex 的顺序（= 传入顺序）"
        );
        assert_eq!(got.gold, *gold, "金币必须一致（怪物 {id}）");
        assert_eq!(got.items, *items, "掉落列表必须逐项一致（怪物 {id}）");
    }
}

/// 断言 2：落地输出 == C# 金标准的 `floor` 行（**与 C# 逐行一致**）。
#[test]
fn floor_placement_matches_csharp_golden() {
    let golden = fs::read_to_string(parity_world_dir().join("golden/session.txt")).unwrap();
    // 金标准里第 6 tick 起出现掉落（此后各 tick 持续存在，取首个 tick 的 floor 行）
    let first_tick_floors: Vec<(usize, i32, i32, i32)> = golden
        .lines()
        .skip_while(|l| !l.starts_with("death "))
        .take_while(|l| !l.starts_with("tick "))
        .filter(|l| l.starts_with("floor "))
        .map(|l| {
            let f: Vec<i64> = l.split(' ').skip(1).map(|v| v.parse().unwrap()).collect();
            (f[0] as usize, f[1] as i32, f[2] as i32, f[3] as i32)
        })
        .collect();
    assert!(
        !first_tick_floors.is_empty(),
        "金标准里应至少有 1 件地面物品（否则这条断言没有意义）"
    );

    // 死亡时的输入：怪物在 (12,10)，掉落顺序 = 先物品（Index）、后金币（怪物 id）
    let mut obj_ids: Vec<i32> = vec![1]; // 物品 Index = 1（金创药）
    obj_ids.push(2); // 金币用怪物 id
    let snap = PlaceSnapshot {
        // 死亡前已被占的格子：玩家 (11,10)、怪物自己 (12,10)
        occupied: vec![(0, 11, 10), (0, 12, 10)],
        drops: vec![DropAt {
            id: 2,
            map_id: 0,
            x: 12,
            y: 10,
            obj_ids,
        }],
    };
    let mut got: Vec<(usize, i32, i32, i32)> = plan_floor_placement(&snap)
        .into_iter()
        .map(|p| (p.map_id, p.x, p.y, p.obj_id))
        .collect();
    got.sort_unstable();
    let mut want = first_tick_floors.clone();
    want.sort_unstable();
    assert_eq!(
        got, want,
        "bevy 系统的落地结果必须与 C# 金标准的 floor 行逐项一致"
    );
}

/// 断言 3：**输出不依赖 ECS 的 spawn 顺序 / `Query` 迭代序**。
///
/// 把同一批怪物按反序 spawn（改变 ECS 内部实体/archetype 顺序），输出必须**完全一致**：
/// 处理顺序来自我们自己的 [`OrderIndex`]（实体 id 升序），与 ECS 内部顺序无关。
/// 反过来说：如果哪天有人把系统的顺序改回"迭代 Query"，这条断言会立刻红。
#[test]
fn output_does_not_depend_on_query_order() {
    let forward = plan_drops(&snapshot(false));
    let reversed = plan_drops(&snapshot(true));
    assert_eq!(
        forward, reversed,
        "spawn 顺序变化后输出必须逐项相同（顺序应只由我们的 OrderIndex 决定）"
    );
    assert_eq!(forward[0].id, 2, "输出顺序 = 实体 id 升序");
}

/// 断言 4：禁并行守卫（直接依赖白名单 + 源码扫描）。
#[test]
fn no_parallel_guard() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let allowed = [
        "bevy_ecs",
        "bevy_app",
        "mir2-shared",
        "mir2-data",
        "mir2-formula",
        "mir2-world",
    ];
    let mut in_deps = false;
    for line in cargo.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t == "[dependencies]";
            continue;
        }
        if in_deps && !t.is_empty() && !t.starts_with('#') {
            if let Some((name, _)) = t.split_once('=') {
                let name = name.trim();
                assert!(
                    allowed.contains(&name),
                    "试验田只允许 {allowed:?}；实际出现 {name}"
                );
            }
        }
    }
    for bad in ["rayon", "crossbeam", "bevy_tasks"] {
        assert!(
            !cargo.contains(&format!("{bad} =")),
            "试验田不得直接依赖 {bad}"
        );
    }

    // 源码扫描（只看代码，跳过注释 —— 文档里出现"禁止某 API"的字样是正常的）
    let lib_raw = fs::read_to_string(root.join("src/lib.rs")).unwrap();
    let lib: String = lib_raw
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("/*") && !t.starts_with('*')
        })
        .collect::<Vec<_>>()
        .join(
            "
",
        );
    for bad in [
        "MultiThreadedExecutor",
        "bevy_tasks",
        "rayon::",
        "crossbeam",
        "thread::spawn",
    ] {
        assert!(!lib.contains(bad), "试验田源码不得出现并行 API：{bad}");
    }
    // 正面断言：显式使用单线程执行器
    assert!(
        lib.contains("SingleThreadedExecutor"),
        "试验田必须显式用 SingleThreadedExecutor（§4.2.2 硬约束①）"
    );
}
