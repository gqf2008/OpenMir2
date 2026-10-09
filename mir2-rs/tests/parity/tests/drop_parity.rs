//! 门禁 ④：掉落公式——固定种子下同一怪 100 次击杀的掉落列表逐项相同。
//!
//! 金标准：`golden/drop_*.txt`，由 `csharp/ParityGolden drop` 产出
//! （`MonGetRandomItems` / `LoadMonitems` / 极品加成链均为原 C# 逐字拷贝 +
//! 真实 `RandomNumber.cs`）。
//!
//! 对照组：
//! - `drop_jiangshi1_100.txt`：真实怪物「僵尸1」掉落表 + 真实物品子集，
//!   `MonRandomAddValue=40`（现网 Server.conf 值），种子 42；
//! - `drop_synth_*.txt`：单物品合成掉落表 × 17 分支（覆盖全部
//!   `RandomUpgradeItem` 分支与 `RandomSetUnknownItem` 的 130/131/132 形状），
//!   `MonRandomAddValue=1`（必出极品，逼每条分支生效）。
//!
//! 注意：C# `M2Share.GetItemNumber()` = 计数器 + GetTickCount()（墙钟），
//! 故 `MakeIndex` 不进对拍（金标准输出本就不含该字段）。

use std::path::{Path, PathBuf};

use mir2_data::models::{StdItem, STRING_GOLD_NAME};
use mir2_data::monitems;
use mir2_formula::drop::{mon_get_random_items, ItemNumberCounter, UserItem, VecItemCatalog};
use mir2_shared::rng::RandomNumber;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

#[derive(serde::Deserialize)]
struct ItemFixture {
    name: String,
    std_mode: u8,
    shape: u8,
    dura_max: u16,
}

fn load_items(json_name: &str) -> Vec<StdItem> {
    let text = std::fs::read_to_string(fixture_dir().join(json_name)).unwrap();
    let fixtures: Vec<ItemFixture> = serde_json::from_str(&text).unwrap();
    fixtures
        .into_iter()
        .map(|f| StdItem {
            name: f.name,
            std_mode: f.std_mode,
            shape: f.shape,
            dura_max: f.dura_max,
            ..Default::default()
        })
        .collect()
}

/// 与 C# 金标准输出逐字节一致的格式化。
fn format_drops(kills: usize, outcomes: &[(i32, Vec<UserItem>)]) -> String {
    let mut out = String::new();
    for (k, (gold, items)) in outcomes.iter().enumerate() {
        out.push_str(&format!("KILL {} G {} N {}\n", k + 1, gold, items.len()));
        for it in items {
            out.push_str(&format!("I {} {} {}", it.index, it.dura, it.dura_max));
            for b in it.desc {
                out.push_str(&format!(" {b}"));
            }
            out.push('\n');
        }
    }
    let _ = kills;
    out
}

fn run_drops(
    monitems_rel: &str,
    items_json: &str,
    seed: i32,
    kills: usize,
    mon_random_add_value: i32,
) -> String {
    let drop_list = monitems::load_monitems(&fixture_dir().join(monitems_rel)).unwrap();
    let items = load_items(items_json);
    let catalog = VecItemCatalog { items: &items };
    let mut rng = RandomNumber::with_seed(seed);
    let mut counter = ItemNumberCounter::default();
    let mut outcomes = Vec::with_capacity(kills);
    for _ in 0..kills {
        let out = mon_get_random_items(
            &drop_list,
            &catalog,
            &mut rng,
            mon_random_add_value,
            STRING_GOLD_NAME,
            &mut counter,
        );
        outcomes.push((out.gold, out.items));
    }
    format_drops(kills, &outcomes)
}

fn assert_golden(actual: &str, golden: &str, case: &str) {
    if actual != golden {
        for (i, (la, lb)) in actual.lines().zip(golden.lines()).enumerate() {
            if la != lb {
                panic!(
                    "掉落对拍不一致（{case}）第 {} 行:\n  rust=`{la}`\n  csharp=`{lb}`",
                    i + 1
                );
            }
        }
        panic!(
            "掉落对拍行数不一致（{case}）: rust={} csharp={}",
            actual.lines().count(),
            golden.lines().count()
        );
    }
}

/// 验收 ④ 主用例：真实怪物 100 次击杀。
#[test]
fn drop_jiangshi1_100_kills_match_golden() {
    let actual = run_drops(
        "MonItems/jiangshi1.txt",
        "items_jiangshi1.json",
        42,
        100,
        40,
    );
    let golden = include_str!("../golden/drop_jiangshi1_100.txt");
    assert_golden(&actual, golden, "jiangshi1");
}

/// 分支覆盖：17 个单物品合成掉落表，逼出全部极品/神秘分支。
#[test]
fn drop_synth_branches_match_golden() {
    const CASES: [&str; 17] = [
        "m5", "m6", "m10", "m11", "m15", "m19", "m20", "m21", "m24", "m26", "r22", "r23", "u15",
        "u22", "u23", "u24", "u26",
    ];
    for case in CASES {
        let actual = run_drops(
            &format!("MonItems/synth/{case}.txt"),
            "items_synth.json",
            42,
            100,
            1,
        );
        let golden = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("golden")
                .join(format!("drop_synth_{case}.txt")),
        )
        .unwrap();
        assert_golden(&actual, &golden, case);
    }
}

/// 红检（自动部分）：换种子 → 掉落序列必须不同。
#[test]
fn redcheck_wrong_seed_must_differ() {
    let actual = run_drops(
        "MonItems/jiangshi1.txt",
        "items_jiangshi1.json",
        43,
        100,
        40,
    );
    let golden = include_str!("../golden/drop_jiangshi1_100.txt");
    assert_ne!(actual, golden, "红检失效：错误种子产出了相同掉落序列");
}

/// 红检（自动部分）：掉落表改掉一个概率 → 必须不同。
#[test]
fn redcheck_tampered_drop_table_must_differ() {
    let mut drop_list =
        monitems::load_monitems(&fixture_dir().join("MonItems/jiangshi1.txt")).unwrap();
    drop_list[0].sel_point = 0; // 金币必掉率改掉
    let items = load_items("items_jiangshi1.json");
    let catalog = VecItemCatalog { items: &items };
    let mut rng = RandomNumber::with_seed(42);
    let mut counter = ItemNumberCounter::default();
    let mut outcomes = Vec::new();
    for _ in 0..100 {
        let out = mon_get_random_items(
            &drop_list,
            &catalog,
            &mut rng,
            40,
            STRING_GOLD_NAME,
            &mut counter,
        );
        outcomes.push((out.gold, out.items));
    }
    let actual = format_drops(100, &outcomes);
    let golden = include_str!("../golden/drop_jiangshi1_100.txt");
    assert_ne!(actual, golden, "红检失效：篡改掉落表未改变结果");
}
