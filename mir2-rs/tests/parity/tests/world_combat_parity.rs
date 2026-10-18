//! 判据②（M2 下半程·战斗切片）：把 `_Attack` 的 **RNG 面**纳入 golden，与 C# oracle 逐行一致。
//!
//! 场景 `combat.json` 覆盖：
//! - 攻击力 RNG（玩家攻方走 `PlayObject.GetAttackPower` override：`Luck=0` 时 `Random(power+1)`
//!   + `Random(10)` 两次取数；怪物攻方才是基版 `Random(power+1)`；DC=0x1E14 ⇒ 20..30）；
//! - 命中判定的 RNG（`RandomByte(目标 SpeedPoint)`，幽灵(6) `speed_point=51` ⇒ 会真的闪避）；
//! - 护甲 RNG 的两条分支（`ac=0` ⇒ 1 次取数；`ac=0x0A05` ⇒ 2 次取数）；
//! - 击杀 → 掉落**生成**（oracle `AddItemToMap` 门 ⇒ 不落地）+ 经验结算（`KillFunc` → `KillTargetTrigger` → `GainExp`）；
//! - **经验归属者是"第一个命中者"**（`ExpHitter`，`BaseObject.cs:1753/2429`），
//!   而不是最后一击者（`LastHiter`）——场景里由 mate(4) 补完最后一击，经验仍归 hero(1)；
//! - 攻击自己（`IsProperTarget` 为假）/ 攻击已死目标（`_Attack` 不造成伤害但**仍**登记命中者）。
//! - **击杀经验不按等级差衰减**（`KillTargetTrigger` 把"自己等级"当 `nLevel` 传进 `CalcGetExp`，
//!   `L < L+10` 恒真）——由单独场景 `combat_exp_nofix.json` 钉住，见下方第 2 条测试。
//!
//! 驱动与输出格式见 `common/mod.rs`；金标准由 `csharp/WorldGolden` 产出：
//! ```bash
//! dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- combat
//! dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- combat_exp_nofix
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
/// 场景里 zombie(3) 先被 hero(1) 命中，之后由 mate(4) 连续攻击直到倒下
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

/// 判据（给"`CalcGetExp` 入参＝自己等级"这条口径装牙齿）：`combat_exp_nofix.json`
/// 把 `high_level_kill_mon_fix_exp` 覆写为 **false**，且玩家等级 25 ≥ 怪等级 1 + 10
/// ⇒ 两种实现的数值**可分辨**：
/// - 照 oracle（入参＝自己等级 25，`25 < 35` 恒真）⇒ 不衰减，得 **500**；
/// - 若误把怪物等级 1 当 `nLevel` ⇒ `25 < 11` 为假 ⇒ 衰减成 `500 - round(500/15*14)` = **33**。
///
/// 所以本测试与 golden 比对时，任何"回退到怪物等级"的改动都会红；同时直接断言 500 这个数，
/// 免得 golden 被误重生成后判据一起失效。（审查 reviewer-3 的 M4a 变异即由此覆盖。）
#[test]
fn kill_exp_does_not_decay_even_when_fix_off() {
    let mine = run_scenario("combat_exp_nofix.json");
    let golden = std::fs::read_to_string(world_dir().join("golden/combat_exp_nofix.txt")).unwrap();
    assert_eq!(
        mine, golden,
        "回退到「把怪物等级当 CalcGetExp 入参」会得到 33 而不是 500"
    );
    assert!(
        mine.contains("death 2 0 0 1 500"),
        "fix=false 下击杀经验必须仍是 500（不按等级差衰减）:\n{mine}"
    );
    let hero_line = mine
        .lines()
        .rev()
        .find(|l| l.starts_with("ent 1 "))
        .unwrap();
    // 行格式：`ent <id> <map> <x> <y> <hp> <level> <exp> vis ...`。
    // 玩家 `SearchViewRange` 现在按 1000ms 节流（卡 `openmir2-player-searchviewrange-cadence`），
    // 末 tick 可能没有扫描 ⇒ 不能用 `vis 1:1`（自身 Invisible）当尾锚。
    let exp_field = hero_line.split_whitespace().nth(7).unwrap_or("");
    assert_eq!(exp_field, "500", "hero 的经验应记到 500: {hero_line}");
}

/// 判据（给"命中者 **ghost 化** 后不再认领经验"装牙齿）：场景 `combat_exp_hitter_ghost.json`
/// 让 hero(1) 先命中僵尸（`ExpHitter=hero`），**同一 tick 内** hero 变 ghost 且 mate(4) 打死它
/// ⇒ 清理步（两边夹具同序：命令之后、死亡结算之前）把 `ExpHitter` 清空，死亡结算时**无人认领**：
/// `death 2 0 0 -1 0`（killer=-1、经验 0），hero/mate 经验都停在 0。
///
/// 若把清理步去掉（或把它挪到死亡结算之后），本条会红；若把击杀挪到 ghost 的**下一 tick**，
/// mate 会合法地成为新的 `ExpHitter`（`SetLastHiter` 在 `ExpHitter` 为空时认领）⇒ killer=4——
/// 那是另一条同样正确的行为，场景刻意只钉"同 tick"这一条。
#[test]
fn ghosted_hitter_loses_exp_credit_in_same_tick() {
    let mine = run_scenario("combat_exp_hitter_ghost.json");
    let golden =
        std::fs::read_to_string(world_dir().join("golden/combat_exp_hitter_ghost.txt")).unwrap();
    assert_eq!(
        mine, golden,
        "ghost 化的命中者必须失去经验认领（期望 death 2 0 0 -1 0）"
    );
    assert!(
        mine.contains("death 2 0 0 -1 0"),
        "ghost 化后同 tick 击杀必须判为无人认领（killer=-1、经验 0）:\n{mine}"
    );
    assert!(
        !mine.contains(" 500 vis"),
        "hero/mate 的经验都必须停在 0（谁也不该拿这 500 经验）:\n{mine}"
    );
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
    // 真闪避的判据：**同一目标**相邻两次 `atk` 行里 `hp_after` 不变且 > 0。
    // 不能拿 `prev_tick_hp` 判：同一 tick 内可能打多次，且「攻击自己」（`IsProperTarget` 假、
    // 目标 HP 不变）会被误计成闪避——那正是 B-1 重基准后这条断言一度假绿的原因。
    let mut last_atk_hp: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
    let mut dodges = 0;
    let mut dodge_targets: std::collections::HashSet<i32> = std::collections::HashSet::new();
    let mut no_roll_target_hits = 0;
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
            if let Some(&prev) = last_atk_hp.get(&target) {
                if after == prev && after > 0 {
                    dodges += 1; // 命中骰判闪避 ⇒ 血量不变
                    dodge_targets.insert(target);
                }
            }
            let before = *prev_tick_hp.get(&target).unwrap_or(&after);
            if target == 5 && after < before {
                // 稻草人 HitPoint == 0 ⇒ C# 整段跳过命中骰（RNG 少消费一次）仍照常掉血
                no_roll_target_hits += 1;
            }
            last_atk_hp.insert(target, after);
        }
    }
    assert!(
        dodges >= 1,
        "金标准必须至少覆盖一次真闪避（否则离散步进未验证）"
    );
    assert!(
        dodge_targets.contains(&6),
        "幽灵(6) `speed_point=51` ⇒ 固定种子流下必须真的闪过（B-1 按 oracle 重基准后 \
         原 speed=40 已不再产生闪避，scenario 特意调到会闪的值以保住这条覆盖）"
    );
    assert!(
        no_roll_target_hits >= 1,
        "金标准必须覆盖『目标 HitPoint == 0 ⇒ 不掷命中骰』分支"
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
