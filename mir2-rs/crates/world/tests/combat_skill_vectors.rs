//! 判据①②③（M3 逐系统对齐·战斗/技能）：把 oracle 真身探针产出的向量**逐字段**核对进 Rust。
//!
//! 向量来源（两侧同一 RNG 注入路径、同一取数账本）：
//! - `mir2-rs/tests/parity/m3/combat-probe`（C#，反射调 `BaseObject.GetBaseAttackPoewr` / `_Attack`，
//!   攻方是真 `PlayObject` ⇒ 走 `PlayObject.GetAttackPower` override）
//!   → `combat-probe/vectors/combat_vectors.{json,tsv}`
//! - `mir2-rs/tests/parity/m3/skill-probe`（C#，真魔表输入，调 `MagicBase.*` / `GetSpellPoint` / `IsTrainingSkill`）
//!   → `skill-probe/vectors/skill_vectors.{json,tsv}` + `skills_registry.{json,tsv}`
//!
//! **为什么本测试读 TSV 而不是 JSON**：S1 迁移守卫 `tests/no_parallel.rs` 把 `crates/world`
//! 的依赖面钉成「内部 crate + bevy_ecs/bevy_app」（连 `[dev-dependencies]` 也扫）——
//! 加 `serde_json` 会被判红。探针因此同时产出**行式伴生件 `.tsv`**（与 JSON 同一次运行写出），
//! 本用例零依赖解析它；JSON 保留为对外/证据用的向量。
//!
//! Rust 侧被测实现：`crates/world/src/combat.rs`（`attack_with` + `AttackPowerMode::Player`）
//! 与 `crates/world/src/magic.rs`。
//!
//! 红检（自动）：三条 `redcheck_*` 把**同一比较函数**作用在被改坏一个数的向量副本上，
//! 断言必出差异 —— 证明「逐字段一致」不是恒绿。

use std::path::PathBuf;

use mir2_data::models::MagicInfo;
use mir2_shared::rng::RandomNumber;
use mir2_world::magic::{
    can_read_book, get_spell_point, magic_attack_interval, power_defence_area,
    power_get_power_plus_mc, power_get_power_plus_sc, power_heal_get_power_plus_sc2,
    power_holy_shield, power_lightening, power_poison, power_transparent,
};
use mir2_world::{
    attack_with, AttackPowerMode, CombatConfig, Entity, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY,
};

fn repo_parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/m3")
}

fn read_lines(path: &str) -> Vec<Vec<String>> {
    let full = repo_parity_dir().join(path);
    let text = std::fs::read_to_string(&full)
        .unwrap_or_else(|e| panic!("读不到向量 {}: {e}", full.display()));
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect()
}

fn i(fields: &[String], idx: usize) -> i64 {
    fields[idx]
        .parse()
        .unwrap_or_else(|e| panic!("字段 {idx} 不是整数: {:?} ({e})", fields.get(idx)))
}

// ---- 战斗 ---------------------------------------------------------------------------------------

#[derive(Clone)]
struct AtkRow {
    power: i32,
    damage: i32,
    hp_after: u16,
    attacked: bool,
}

#[derive(Clone)]
struct CombatCase {
    name: String,
    dc: i32,
    ahp: u8,
    aup: i32,
    luck: i32,
    power_rate: i32,
    bpi: bool,
    pi: u8,
    tac: u16,
    thp: u16,
    thit: u8,
    tsp: u8,
    tla: u8,
    attacks: Vec<AtkRow>,
    hits: i64,
    dodges: i64,
    attacked: i64,
    damage_sum: i64,
}

fn load_combat() -> (i32, Vec<CombatCase>) {
    let lines = read_lines("combat-probe/vectors/combat_vectors.tsv");
    let mut seed = 0i32;
    let mut cases: Vec<CombatCase> = Vec::new();
    for (n, fields) in lines.iter().enumerate() {
        match fields[0].as_str() {
            "seed" => seed = i(fields, 1) as i32,
            "case" => cases.push(CombatCase {
                name: fields[1].clone(),
                dc: i(fields, 2) as i32,
                ahp: i(fields, 3) as u8,
                aup: i(fields, 4) as i32,
                luck: i(fields, 5) as i32,
                power_rate: i(fields, 6) as i32,
                bpi: i(fields, 7) != 0,
                pi: i(fields, 8) as u8,
                tac: i(fields, 9) as u16,
                thp: i(fields, 10) as u16,
                thit: i(fields, 11) as u8,
                tsp: i(fields, 12) as u8,
                tla: i(fields, 13) as u8,
                attacks: Vec::new(),
                hits: 0,
                dodges: 0,
                attacked: 0,
                damage_sum: 0,
            }),
            "atk" => {
                let case = cases
                    .last_mut()
                    .unwrap_or_else(|| panic!("第 {n} 行 atk 没有前导 case"));
                assert_eq!(i(fields, 2) as usize, case.attacks.len(), "atk i 不连续");
                case.attacks.push(AtkRow {
                    power: i(fields, 3) as i32,
                    damage: i(fields, 4) as i32,
                    hp_after: i(fields, 5) as u16,
                    attacked: i(fields, 6) != 0,
                });
            }
            "stats" => {
                let case = cases
                    .last_mut()
                    .unwrap_or_else(|| panic!("第 {n} 行 stats 没有前导 case"));
                case.hits = i(fields, 2);
                case.dodges = i(fields, 3);
                case.attacked = i(fields, 4);
                case.damage_sum = i(fields, 5);
            }
            other => panic!("第 {n} 行未知前缀 {other}"),
        }
    }
    (seed, cases)
}

/// 战斗向量核对：返回**逐字段差异清单**（空 = 全等）。真实判据与红检共用。
fn combat_mismatches(case: &CombatCase, seed: i32, config: &CombatConfig) -> Vec<String> {
    let mut problems = Vec::new();
    let mut rng = RandomNumber::with_seed(seed);
    let mut attacker = Entity::new(1, "hero", 0, 0);
    attacker.race = ACTOR_RACE_PLAY;
    attacker.dc = case.dc;
    attacker.hit_point = case.ahp;
    attacker.undead_power = case.aup;
    let mut target = Entity::new(2, "monster", 0, 0);
    target.race = ACTOR_RACE_MONSTER;
    target.ac = i32::from(case.tac);
    target.hp = case.thp;
    target.max_hp = case.thp;
    target.hit_point = case.thit;
    target.speed_point = case.tsp;
    target.life_attrib = case.tla;
    let mode = AttackPowerMode::Player {
        luck: case.luck,
        power_rate: case.power_rate,
        bo_power_item: case.bpi,
        power_item: case.pi,
    };

    let mut hits = 0i64;
    let mut dodges = 0i64;
    let mut attacked = 0i64;
    let mut damage_sum = 0i64;
    for (idx, exp) in case.attacks.iter().enumerate() {
        let before = i64::from(target.hp);
        let out = attack_with(&mut rng, &mut attacker, &mut target, None, config, mode);
        if i64::from(out.power) != i64::from(exp.power) {
            problems.push(format!(
                "[{idx}] power rust={} csharp={}",
                out.power, exp.power
            ));
        }
        if i64::from(out.damage) != i64::from(exp.damage) {
            problems.push(format!(
                "[{idx}] damage rust={} csharp={}",
                out.damage, exp.damage
            ));
        }
        if out.target_hp_after != exp.hp_after {
            problems.push(format!(
                "[{idx}] hp_after rust={} csharp={}",
                out.target_hp_after, exp.hp_after
            ));
        }
        if out.hit != (exp.damage > 0) {
            problems.push(format!(
                "[{idx}] hit rust={} csharp(damage>0)={}",
                out.hit,
                exp.damage > 0
            ));
        }
        if (before - i64::from(target.hp)) != i64::from(exp.damage) {
            problems.push(format!(
                "[{idx}] hp 差 rust={} csharp={}",
                before - i64::from(target.hp),
                exp.damage
            ));
        }
        if (out.damage > 0) != exp.attacked {
            problems.push(format!(
                "[{idx}] attacked rust={} csharp={}",
                out.damage > 0,
                exp.attacked
            ));
        }
        if exp.damage > 0 {
            hits += 1;
        } else {
            dodges += 1;
        }
        if exp.attacked {
            attacked += 1;
        }
        damage_sum += i64::from(exp.damage);
    }

    if hits != case.hits {
        problems.push(format!("stats.hits rust={hits} csharp={}", case.hits));
    }
    if dodges != case.dodges {
        problems.push(format!("stats.dodges rust={dodges} csharp={}", case.dodges));
    }
    if attacked != case.attacked {
        problems.push(format!(
            "stats.attacked rust={attacked} csharp={}",
            case.attacked
        ));
    }
    if damage_sum != case.damage_sum {
        problems.push(format!(
            "stats.damage_sum rust={damage_sum} csharp={}",
            case.damage_sum
        ));
    }
    problems
}

// ---- 技能 ---------------------------------------------------------------------------------------

#[derive(Clone)]
struct SkillCase {
    magic: MagicInfo,
    mp_cost: [u16; 4],
    interval: i32,
    pjob: u8,
    plevel: u8,
    is_training: bool,
    can_read: bool,
    mc: i32,
    sc: i32,
    kind: String,
    eff_power: i32,
    eff_vs_undead: i32,
    eff_green: i32,
    eff_red: i32,
}

struct RegRow {
    magic_id: u16,
    implemented: bool,
    reason: String,
}

fn load_skills() -> (i32, i32, Vec<SkillCase>, Vec<RegRow>) {
    let lines = read_lines("skill-probe/vectors/skill_vectors.tsv");
    let mut seed = 0i32;
    let mut hit_interval = 0i32;
    let mut cases = Vec::new();
    let mut registry = Vec::new();
    for (n, fields) in lines.iter().enumerate() {
        match fields[0].as_str() {
            "seed" => {
                seed = i(fields, 1) as i32;
                hit_interval = i(fields, 2) as i32;
            }
            "reg" => registry.push(RegRow {
                magic_id: i(fields, 1) as u16,
                implemented: i(fields, 6) != 0,
                reason: fields[7].clone(),
            }),
            "skill" => {
                let mut train_level = [0u8; 4];
                for (k, t) in train_level.iter_mut().enumerate() {
                    *t = i(fields, 5 + k) as u8;
                }
                let mut max_train = [0i32; 4];
                for (k, t) in max_train.iter_mut().enumerate() {
                    *t = i(fields, 9 + k) as i32;
                }
                let mut mp_cost = [0u16; 4];
                for (k, t) in mp_cost.iter_mut().enumerate() {
                    *t = i(fields, 21 + k) as u16;
                }
                cases.push(SkillCase {
                    magic: MagicInfo {
                        magic_id: i(fields, 1) as u16,
                        magic_name: fields[2].clone(),
                        effect_type: 0,
                        effect: 0,
                        spell: i(fields, 13) as u16,
                        power: i(fields, 15) as u16,
                        max_power: i(fields, 16) as u16,
                        job: i(fields, 3) as u8,
                        train_level,
                        max_train,
                        train_lv: i(fields, 4) as u8,
                        delay_time: i(fields, 19) as i32,
                        def_spell: i(fields, 14) as u8,
                        def_power: i(fields, 17) as u8,
                        def_max_power: i(fields, 18) as u8,
                        desc: String::new(),
                    },
                    mp_cost,
                    interval: i(fields, 20) as i32,
                    pjob: i(fields, 25) as u8,
                    plevel: i(fields, 26) as u8,
                    is_training: i(fields, 29) != 0,
                    can_read: i(fields, 30) != 0,
                    mc: i(fields, 31) as i32,
                    sc: i(fields, 32) as i32,
                    kind: fields[33].clone(),
                    eff_power: i(fields, 34) as i32,
                    eff_vs_undead: i(fields, 35) as i32,
                    eff_green: i(fields, 36) as i32,
                    eff_red: i(fields, 37) as i32,
                });
            }
            other => panic!("第 {n} 行未知前缀 {other}"),
        }
    }
    (seed, hit_interval, cases, registry)
}

/// 技能向量核对：返回**逐字段差异清单**（空 = 全等）。红检共用。
fn skill_mismatches(case: &SkillCase, seed: i32, hit_interval: i32) -> Vec<String> {
    let mut problems = Vec::new();
    let magic = &case.magic;
    let magic_id = i64::from(magic.magic_id);

    for (lv, exp) in case.mp_cost.iter().enumerate() {
        let got = get_spell_point(magic, lv as u8);
        if got != *exp {
            problems.push(format!("mp_cost[{lv}] rust={got} csharp={exp}"));
        }
    }

    if magic.delay_time != case.interval - hit_interval {
        problems.push(format!(
            "delay_time rust={} csharp={}",
            magic.delay_time,
            case.interval - hit_interval
        ));
    }
    let got_interval = magic_attack_interval(magic.delay_time, hit_interval);
    if got_interval != case.interval {
        problems.push(format!(
            "interval rust={got_interval} csharp={}",
            case.interval
        ));
    }

    let got_book = can_read_book(magic, case.pjob, case.plevel, case.is_training);
    if got_book != case.can_read {
        problems.push(format!(
            "can_read_book rust={got_book} csharp={}",
            case.can_read
        ));
    }

    // 效果威力（每技能独立子种子，与探针同序）
    let sub_seed = seed ^ (magic_id as i32);
    let level = 1u8;
    let mut rng = RandomNumber::with_seed(sub_seed);
    match case.kind.as_str() {
        "damage" | "heal" => {
            let got = match magic_id {
                1 | 5 => power_get_power_plus_mc(&mut rng, magic, level, case.mc),
                13 => power_get_power_plus_sc(&mut rng, magic, level, case.sc),
                2 => power_heal_get_power_plus_sc2(&mut rng, magic, level, case.sc),
                11 => {
                    let (p, undead) = power_lightening(&mut rng, magic, level, case.mc);
                    if i64::from(undead) != i64::from(case.eff_vs_undead) {
                        problems.push(format!(
                            "vs_undead rust={undead} csharp={}",
                            case.eff_vs_undead
                        ));
                    }
                    p
                }
                other => panic!("未登记的伤害技能 id {other}"),
            };
            if i64::from(got) != i64::from(case.eff_power) {
                problems.push(format!("effect.power rust={got} csharp={}", case.eff_power));
            }
        }
        "poison" => {
            let green = power_poison(&mut rng, magic, level, case.sc, 40);
            let mut rng2 = RandomNumber::with_seed(sub_seed);
            let red = power_poison(&mut rng2, magic, level, case.sc, 30);
            if i64::from(green) != i64::from(case.eff_green) {
                problems.push(format!(
                    "power_green rust={green} csharp={}",
                    case.eff_green
                ));
            }
            if i64::from(red) != i64::from(case.eff_red) {
                problems.push(format!("power_red rust={red} csharp={}", case.eff_red));
            }
        }
        "buff_ac" => {
            let got = power_defence_area(&mut rng, magic, level, case.sc);
            if i64::from(got) != i64::from(case.eff_power) {
                problems.push(format!("effect.power rust={got} csharp={}", case.eff_power));
            }
        }
        "buff_holy" => {
            let got = power_holy_shield(&mut rng, magic, level, case.sc);
            if i64::from(got) != i64::from(case.eff_power) {
                problems.push(format!("effect.power rust={got} csharp={}", case.eff_power));
            }
        }
        "buff_transparent" => {
            let got = power_transparent(&mut rng, magic, level, case.sc);
            if i64::from(got) != i64::from(case.eff_power) {
                problems.push(format!("effect.power rust={got} csharp={}", case.eff_power));
            }
        }
        other => panic!("未登记的效果类别 {other}"),
    }
    problems
}

// ---- 判据 ---------------------------------------------------------------------------------------

/// 判据①：固定种子下同一怪 100 次攻击的伤害序列逐项相同（4 个 case，含命中/躲闪与不死系分支）。
#[test]
fn combat_damage_sequence_matches_csharp() {
    let (seed, cases) = load_combat();
    let config = CombatConfig::default();
    assert!(!cases.is_empty(), "战斗向量为空");
    for case in &cases {
        let problems = combat_mismatches(case, seed, &config);
        assert!(
            problems.is_empty(),
            "战斗序列与 C# oracle 不一致（case={}），差异 {} 项：\n{}",
            case.name,
            problems.len(),
            problems.join("\n")
        );
    }
}

/// 判据②：命中/躲闪统计两侧一致；且序列里真的同时出现命中与闪避（否则统计是平凡值）。
#[test]
fn combat_hit_dodge_stats_match_and_are_non_trivial() {
    let (seed, cases) = load_combat();
    let config = CombatConfig::default();
    let mut total_hits = 0i64;
    let mut total_dodges = 0i64;
    for case in &cases {
        total_hits += case.hits;
        total_dodges += case.dodges;
        assert_eq!(
            case.hits + case.dodges,
            case.attacks.len() as i64,
            "hits + dodges 必须等于攻击次数（case={}）",
            case.name
        );
        assert!(
            combat_mismatches(case, seed, &config).is_empty(),
            "统计核对失败（case={}）",
            case.name
        );
    }
    assert!(total_hits > 0, "向量里必须真的出现命中");
    assert!(
        total_dodges > 0,
        "向量里必须真的出现闪避（否则命中骰分支未被覆盖）"
    );
}

/// 判据③：每个已实装技能一条用例，逐项核前置/MP/效果/冷却。
#[test]
fn skills_match_csharp_per_skill() {
    let (seed, hit_interval, cases, _) = load_skills();
    assert!(
        cases.len() >= 10,
        "已实装技能用例数 {} 少于 10（少登记了技能？）",
        cases.len()
    );
    for case in &cases {
        let problems = skill_mismatches(case, seed, hit_interval);
        assert!(
            problems.is_empty(),
            "技能用例与 C# oracle 不一致（{}），差异 {} 项：\n{}",
            case.magic.magic_name,
            problems.len(),
            problems.join("\n")
        );
    }
}

/// 登记判据（DoD③「未实装技能逐条登记，不许静默漏」）：
/// 真魔表每一行都登记，且 implemented=false 的行必须有承接说明。
#[test]
fn unimplemented_skills_are_registered() {
    let (_, _, cases, registry) = load_skills();
    assert!(
        registry.len() >= 100,
        "魔表登记行数 {} 过少（解析漏行？）",
        registry.len()
    );
    let mut implemented = 0;
    for row in &registry {
        assert!(
            !row.reason.is_empty(),
            "魔表行 {} 没有承接说明",
            row.magic_id
        );
        if row.implemented {
            implemented += 1;
        }
    }
    assert_eq!(
        implemented,
        cases.len(),
        "登记为已实装的技能数必须等于用例数"
    );
}

/// 判据（登记未接线）：`PlayObject.GetAttackPower` override（Luck/PowerRate/BoPowerItem）
/// 会让**取数次数**与 `BaseObject.GetAttackPower` 不同（Luck=0 时每次多抽一次 `Random(10)`）。
/// 世界宿主 `World::attack_once` 仍走 [`AttackPowerMode::Base`]（M2 既有 golden 口径），
/// 是**已登记的未接线项**（承接：M3 后续批次，需同步更新 M2 `WorldGolden` 桩，本卡写集外）。
///
/// 本测试不比对两个实现，而是断言二者**确为不同实现**（同种子同输入下取数账本不同），
/// 免得有人误以为 `AttackMode::Base` 已覆盖玩家路径。
#[test]
fn player_attack_power_is_a_registered_distinct_path() {
    let config = CombatConfig::default();
    let mut attacker = Entity::new(1, "hero", 0, 0);
    attacker.dc = 0x1E14;
    attacker.hit_point = 20;
    let mut base_target = Entity::new(2, "monster", 0, 0);
    base_target.race = ACTOR_RACE_MONSTER;
    base_target.ac = 5;
    base_target.hp = 60000;
    base_target.max_hp = 60000;
    let mut player_target = base_target.clone();

    let mut rng_base = RandomNumber::with_seed(7);
    let out_base = attack_with(
        &mut rng_base,
        &mut attacker,
        &mut base_target,
        None,
        &config,
        AttackPowerMode::Base,
    );
    let mut rng_player = RandomNumber::with_seed(7);
    let out_player = attack_with(
        &mut rng_player,
        &mut attacker,
        &mut player_target,
        None,
        &config,
        AttackPowerMode::Player {
            luck: 0,
            power_rate: 100,
            bo_power_item: false,
            power_item: 0,
        },
    );
    assert_ne!(
        (out_base.power, out_base.damage),
        (out_player.power, out_player.damage),
        "两种攻方口径在同种子下应产生不同序列（否则登记项已被覆盖，可销账）"
    );
}

// ---- 红检 ---------------------------------------------------------------------------------------

/// 红检①：改坏伤害序列里的**一个数** ⇒ 战斗对拍必 RED。
#[test]
fn redcheck_corrupt_one_damage_value_must_fail() {
    let (seed, cases) = load_combat();
    let config = CombatConfig::default();
    let case = &cases[0];
    assert!(
        combat_mismatches(case, seed, &config).is_empty(),
        "前提：未改坏前必须全等"
    );
    let mut corrupt = case.clone();
    let row = corrupt
        .attacks
        .iter_mut()
        .find(|a| a.damage > 0)
        .expect("向量里应有伤害 > 0 的一击");
    let old = row.damage;
    row.damage = old + 1;
    assert!(
        !combat_mismatches(&corrupt, seed, &config).is_empty(),
        "红检失效：把一击伤害 {old} 改成 {} 后仍判等（比较函数没有牙齿）",
        old + 1
    );
}

/// 红检②：改坏某技能的 **MP 消耗**一个值 ⇒ 该技能用例必 RED。
#[test]
fn redcheck_corrupt_one_mp_cost_must_fail() {
    let (seed, hit_interval, cases, _) = load_skills();
    let case = &cases[0];
    assert!(
        skill_mismatches(case, seed, hit_interval).is_empty(),
        "前提：未改坏前必须全等"
    );
    let mut corrupt = case.clone();
    let old = corrupt.mp_cost[1];
    corrupt.mp_cost[1] = old + 1;
    assert!(
        !skill_mismatches(&corrupt, seed, hit_interval).is_empty(),
        "红检失效：把技能 {} 的 MP 消耗 {old} 改成 {} 后仍判等",
        corrupt.magic.magic_id,
        old + 1
    );
}

/// 红检③（技能·冷却）：改坏 `MagicAttackInterval` 一个值 ⇒ 该技能用例必 RED。
#[test]
fn redcheck_corrupt_one_cooldown_must_fail() {
    let (seed, hit_interval, cases, _) = load_skills();
    let case = &cases[0];
    let mut corrupt = case.clone();
    let old = corrupt.interval;
    corrupt.interval = old + 1;
    assert!(
        !skill_mismatches(&corrupt, seed, hit_interval).is_empty(),
        "红检失效：把 MagicAttackInterval {old} 改成 {} 后仍判等",
        old + 1
    );
}
