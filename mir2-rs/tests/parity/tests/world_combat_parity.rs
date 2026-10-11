//! 判据②（M2 下半程·战斗切片）：把 `_Attack` 的 **RNG 面**纳入 golden，与 C# oracle 逐行一致。
//!
//! 场景 `combat.json` 覆盖：
//! - 攻击力 RNG（`GetBaseAttackPoewr` → `Random(power+1)`，DC=0x1E14 ⇒ 20..30）；
//! - 命中判定的 RNG（`RandomByte(目标 SpeedPoint)`，怪物 `speed_point=12` ⇒ 会真的闪避）；
//! - 护甲 RNG 的两条分支（`ac=0` ⇒ 1 次取数；`ac=0x0A05` ⇒ 2 次取数）；
//! - 击杀 → 掉落落地 + 经验结算（`KillFunc` → `KillTargetTrigger` → `GainExp`）；
//! - **经验归属者是"第一个命中者"**（`ExpHitter`，`BaseObject.cs:1753/2429`），
//!   而不是最后一击者（`LastHiter`）——场景里由 mate(5) 补完最后一击，经验仍归 hero(1)；
//! - 攻击自己（`IsProperTarget` 为假）/ 攻击已死目标（`_Attack` 不造成伤害但**仍**登记命中者）。
//!
//! 驱动与输出格式见 `common/mod.rs`；金标准由 `csharp/WorldGolden` 产出：
//! ```bash
//! dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- combat
//! ```

mod common;

use common::{run_scenario, world_dir};

#[test]
fn combat_attacks_and_exp_match_csharp() {
    let mine = run_scenario("combat.json");
    let golden = std::fs::read_to_string(world_dir().join("golden/combat.txt")).unwrap();
    if mine != golden {
        for (i, (a, b)) in mine.lines().zip(golden.lines()).enumerate() {
            if a != b {
                panic!(
                    "战斗结算与 C# oracle 不一致，首个差异在第 {} 行:\n  rust  = {a}\n  csharp= {b}",
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
    assert!(mine.contains("atk "), "战斗场景必须产出攻击结算行");
    assert!(mine.contains("death 3 "), "战斗场景必须产出击杀结算行");
}

/// 语义判据（不是行数一致就能证明的）：经验归属者 = **第一个**命中者。
///
/// 场景里 zombie(3) 先被 hero(1) 命中，之后由 mate(5) 连续攻击直到倒下
/// ⇒ "死亡行前最后一击"必须是 4，而 `death` 行的归属者必须是 1。
/// 若哪天有人把 `ExpHitter` 改回 `LastHiter`，这条会红。
#[test]
fn exp_goes_to_first_hitter_not_last() {
    let golden = std::fs::read_to_string(world_dir().join("golden/combat.txt")).unwrap();
    let mut last_attacker_of_3: Option<i32> = None;
    for line in golden.lines() {
        if let Some(rest) = line.strip_prefix("atk ") {
            let f: Vec<&str> = rest.split_whitespace().collect();
            if f[1] == "3" {
                last_attacker_of_3 = Some(f[0].parse().unwrap());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("death 3 ") {
            let f: Vec<&str> = rest.split_whitespace().collect();
            let killer: i32 = f[2].parse().unwrap();
            assert_eq!(
                last_attacker_of_3,
                Some(4),
                "场景前提被破坏：zombie(3) 的最后一击应来自 mate(4)，实际 {last_attacker_of_3:?}"
            );
            assert_eq!(
                killer, 1,
                "经验归属者必须是第一个命中者 hero(1)（ExpHitter），不是最后一击者 mate(4)"
            );
            return;
        }
    }
    panic!("金标准里没有 zombie(3) 的死亡行");
}

/// 覆盖判据：金标准里必须真的出现**闪避**（`hp` 没掉）与**目标 HitPoint == 0 时不掷命中骰**
/// 两条分支，否则"逐行一致"只是覆盖了常规路径。
#[test]
fn golden_covers_dodge_and_no_hit_roll_branch() {
    let golden = std::fs::read_to_string(world_dir().join("golden/combat.txt")).unwrap();
    // `ent` 行打印的是**本 tick 结束**的状态，攻击发生在同一 tick 的 `atk` 行
    // ⇒ 判断"掉没掉血"要用**上一个 tick** 的 `ent` 值作为 before。
    let mut prev_tick_hp: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
    let mut cur_tick_hp: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
    let mut dodges = 0;
    let mut no_roll_target_hits = 0;
    let mut last_attack_of_6: Option<i32> = None;
    for line in golden.lines() {
        if line.starts_with("tick ") {
            prev_tick_hp = std::mem::take(&mut cur_tick_hp);
            continue;
        }
        if let Some(rest) = line.strip_prefix("ent ") {
            let f: Vec<&str> = rest.split_whitespace().collect();
            cur_tick_hp.insert(f[0].parse().unwrap(), f[4].parse().unwrap());
            continue;
        }
        if let Some(rest) = line.strip_prefix("atk ") {
            let f: Vec<&str> = rest.split_whitespace().collect();
            let (target, after): (i32, i32) = (f[1].parse().unwrap(), f[3].parse().unwrap());
            let before = *prev_tick_hp.get(&target).unwrap_or(&after);
            if after == before && after > 0 {
                dodges += 1; // 命中骰判闪避 ⇒ 血量不变
            }
            if target == 5 && after < before {
                // 稻草人 HitPoint == 0 ⇒ C# 整段跳过命中骰（RNG 少消费一次）仍照常掉血
                no_roll_target_hits += 1;
            }
            if target == 6 {
                last_attack_of_6 = Some(after);
            }
        }
    }
    assert!(
        dodges >= 1,
        "金标准必须至少覆盖一次闪避（否则离散步进未验证）"
    );
    assert!(
        no_roll_target_hits >= 1,
        "金标准必须覆盖『目标 HitPoint == 0 ⇒ 不掷命中骰』分支"
    );
    assert_eq!(
        last_attack_of_6,
        Some(50),
        "幽灵(6) hp=100 且 speed_point=40 ⇒ 6 次攻击后应还剩 50（证明闪避真的发生过）"
    );
}

/// 红检（自动）：把怪物的敏捷归零 ⇒ 命中判定不再走闪避分支，`atk` 行必须变化。
/// 证明"逐行一致"确实在比对 `RandomByte(SpeedPoint)` 这条 RNG 面，而不是恒绿。
#[test]
fn redcheck_speed_point_change_must_alter_attacks() {
    let golden = std::fs::read_to_string(world_dir().join("golden/combat.txt")).unwrap();
    let text = std::fs::read_to_string(world_dir().join("scenarios/combat.json")).unwrap();
    let mut sc: serde_json::Value = serde_json::from_str(&text).unwrap();
    for m in sc["monsters"].as_array_mut().unwrap() {
        m["speed_point"] = serde_json::json!(0);
    }
    let path = world_dir().join("scenarios/.combat_redcheck.json");
    std::fs::write(&path, serde_json::to_string_pretty(&sc).unwrap()).unwrap();
    let mine = run_scenario(".combat_redcheck.json");
    let _ = std::fs::remove_file(&path);
    let mine_atk: Vec<&str> = mine.lines().filter(|l| l.starts_with("atk ")).collect();
    let golden_atk: Vec<&str> = golden.lines().filter(|l| l.starts_with("atk ")).collect();
    assert!(!golden_atk.is_empty(), "金标准里应至少有 1 条 atk 行");
    assert_ne!(
        mine_atk, golden_atk,
        "红检失效：把怪物敏捷归零后 atk 行没变"
    );
}

/// 红检（自动）：把稻草人的 `hit_point` 从 0 改成 5 ⇒ 命中骰**重新开始消费 RNG**，
/// 之后所有取数的对位都会平移，`atk` 行必须变化。
/// 证明判据覆盖了"目标 HitPoint == 0 时整段跳过命中骰"这条分支口径。
#[test]
fn redcheck_hit_point_zero_branch_consumes_no_rng() {
    let golden = std::fs::read_to_string(world_dir().join("golden/combat.txt")).unwrap();
    let text = std::fs::read_to_string(world_dir().join("scenarios/combat.json")).unwrap();
    let mut sc: serde_json::Value = serde_json::from_str(&text).unwrap();
    for m in sc["monsters"].as_array_mut().unwrap() {
        if m["id"] == serde_json::json!(5) {
            m["hit_point"] = serde_json::json!(5);
        }
    }
    let path = world_dir().join("scenarios/.combat_redcheck_hitpoint.json");
    std::fs::write(&path, serde_json::to_string_pretty(&sc).unwrap()).unwrap();
    let mine = run_scenario(".combat_redcheck_hitpoint.json");
    let _ = std::fs::remove_file(&path);
    assert_ne!(
        mine, golden,
        "红检失效：稻草人 hit_point 0→5 后整份输出没变"
    );
}
