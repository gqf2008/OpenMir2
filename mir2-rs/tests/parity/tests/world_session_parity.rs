//! 判据②③（M2 下半程·会话切片）：会话进世界 + 掉落系统的 golden 对拍。
//!
//! 场景 `session.json` 覆盖：进图 → 移动（含被怪物挡住的一步）→ 怪物生成（掉落预算在此消耗 RNG）
//! → 脚本化击杀（掉落落地 + 金币）→ 切图 → 小退 → 再进（新 `ActorId`）。
//! 战斗结算走 `combat.json`（见 `world_combat_parity.rs`）——本切片刻意不含 `attack`，
//! 因此随机流只由掉落预算消耗，把"逐行一致"先钉在一个系统上。
//!
//! 驱动与输出格式见 `common/mod.rs`；金标准由 `csharp/WorldGolden` 产出：
//! ```bash
//! dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- session
//! ```

mod common;

use common::{run_scenario, world_dir};

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
