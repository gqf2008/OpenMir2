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
//! 红检（自动）：八条 `redcheck_*` 把**同一比较函数**作用在被改坏一个数的向量副本上，
//! 断言必出差异 —— 证明「逐字段一致」不是恒绿。
//!
//! 第五片新增：39（`SKILL_GROUPDEDING`→`MagGroupDeDing`）群体物理伤害用例（`group_melee`），
//! 以及 `dospell_classes_are_fully_registered` —— 把真魔表每一行按 oracle `DoSpell` 控制流
//! 归入唯一确定的分派类别（warr/magicid 拒绝、implemented、context_required、no_effect_case、
//! no_switch_case）并逐类钉死行数（DoD③「未实装逐条登记，不许静默漏」的机器门禁）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use mir2_data::models::MagicInfo;
use mir2_shared::rng::{RandomNumber, RandomSource};
use mir2_world::combat::{get_base_attack_power, get_base_attack_power_player};
use mir2_world::magic::{
    can_read_book, get_spell_point, group_melee_damage, magic_attack_interval, power_defence_area,
    power_fire_wall, power_get_power_plus_mc, power_get_power_plus_sc,
    power_heal_get_power_plus_sc2, power_holy_shield, power_lightening, power_poison, power_shield,
    power_show_hp_interval, power_transparent, power_uenhancer, resolve_magic,
};
use mir2_world::{
    attack_with, is_proper_target, AttackPowerMode, CombatConfig, Entity, World,
    ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY, ATTACK_MODE_HAM_PEACE,
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
    eff_time: i32,
    eff_interval: i32,
    eff_melee_damage: i32,
    dc: i32,
    hit_point: u8,
    target_speed_point: u8,
    target_ac: u16,
    target_life_attrib: u8,
}

struct RegRow {
    magic_id: u16,
    name: String,
    implemented: bool,
    reason: String,
    dospell_class: String,
}

fn load_skills() -> (i32, i32, AttackPowerMode, Vec<SkillCase>, Vec<RegRow>) {
    let lines = read_lines("skill-probe/vectors/skill_vectors.tsv");
    let mut seed = 0i32;
    let mut hit_interval = 0i32;
    // `DoSpell` 的施法者是 PlayObject ⇒ 效果威力走 Player override；向量在 seed 行附这四项。
    let mut atk = AttackPowerMode::Player {
        luck: 0,
        power_rate: 100,
        bo_power_item: false,
        power_item: 0,
    };
    let mut cases = Vec::new();
    let mut registry = Vec::new();
    for (n, fields) in lines.iter().enumerate() {
        match fields[0].as_str() {
            "seed" => {
                seed = i(fields, 1) as i32;
                hit_interval = i(fields, 2) as i32;
                atk = AttackPowerMode::Player {
                    luck: i(fields, 3) as i32,
                    power_rate: i(fields, 4) as i32,
                    bo_power_item: i(fields, 5) != 0,
                    power_item: i(fields, 6) as u8,
                };
            }
            "reg" => registry.push(RegRow {
                magic_id: i(fields, 1) as u16,
                name: fields[2].clone(),
                implemented: i(fields, 6) != 0,
                reason: fields[7].clone(),
                dospell_class: fields[8].clone(),
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
                    eff_time: i(fields, 38) as i32,
                    eff_interval: i(fields, 39) as i32,
                    eff_melee_damage: i(fields, 40) as i32,
                    dc: i(fields, 41) as i32,
                    hit_point: i(fields, 42) as u8,
                    target_speed_point: i(fields, 43) as u8,
                    target_ac: i(fields, 44) as u16,
                    target_life_attrib: i(fields, 45) as u8,
                });
            }
            other => panic!("第 {n} 行未知前缀 {other}"),
        }
    }
    (seed, hit_interval, atk, cases, registry)
}

/// 技能向量核对：返回**逐字段差异清单**（空 = 全等）。红检共用。
fn skill_mismatches(
    case: &SkillCase,
    seed: i32,
    hit_interval: i32,
    atk: AttackPowerMode,
) -> Vec<String> {
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
                1 | 5 | 9 | 10 | 23 | 24 | 33 | 44 | 47 | 50 => {
                    power_get_power_plus_mc(&mut rng, magic, level, case.mc, atk)
                }
                13 => power_get_power_plus_sc(&mut rng, magic, level, case.sc, atk),
                2 | 29 => power_heal_get_power_plus_sc2(&mut rng, magic, level, case.sc, atk),
                11 | 42 | 45 => {
                    let (p, undead) = power_lightening(&mut rng, magic, level, case.mc, atk);
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
            let got = power_defence_area(&mut rng, magic, level, case.sc, atk);
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
        "fire_wall" => {
            let (power, time) = power_fire_wall(&mut rng, magic, level, case.mc, atk);
            if i64::from(power) != i64::from(case.eff_power) {
                problems.push(format!(
                    "fire_wall.power rust={power} csharp={}",
                    case.eff_power
                ));
            }
            if i64::from(time) != i64::from(case.eff_time) {
                problems.push(format!(
                    "fire_wall.time rust={time} csharp={}",
                    case.eff_time
                ));
            }
        }
        "show_hp" => {
            let interval = power_show_hp_interval(&mut rng, magic, level, case.sc);
            if i64::from(interval) != i64::from(case.eff_interval) {
                problems.push(format!(
                    "show_hp.interval rust={interval} csharp={}",
                    case.eff_interval
                ));
            }
        }
        "shield" => {
            let power = power_shield(&mut rng, magic, level, case.mc);
            if i64::from(power) != i64::from(case.eff_power) {
                problems.push(format!(
                    "shield.power rust={power} csharp={}",
                    case.eff_power
                ));
            }
        }
        "uenhancer" => {
            let (power, time) = power_uenhancer(&mut rng, magic, level, case.sc, atk);
            if i64::from(power) != i64::from(case.eff_power) {
                problems.push(format!(
                    "uenhancer.power rust={power} csharp={}",
                    case.eff_power
                ));
            }
            if i64::from(time) != i64::from(case.eff_time) {
                problems.push(format!(
                    "uenhancer.time rust={time} csharp={}",
                    case.eff_time
                ));
            }
        }
        "group_melee" => {
            // 39（`MagGroupDeDing`）：攻方走 PlayObject override，目标带 SpeedPoint/AC/LifeAttrib。
            let mut attacker = Entity::new(1, "caster", 0, 0);
            attacker.race = ACTOR_RACE_PLAY;
            attacker.dc = case.dc;
            attacker.hit_point = case.hit_point;
            let mut target = Entity::new(2, "target", 0, 0);
            target.race = ACTOR_RACE_MONSTER;
            target.ac = i32::from(case.target_ac);
            target.speed_point = case.target_speed_point;
            target.life_attrib = case.target_life_attrib;
            let (power, damage) = group_melee_damage(&mut rng, &attacker, &target, atk);
            if i64::from(power) != i64::from(case.eff_power) {
                problems.push(format!(
                    "group_melee.power rust={power} csharp={}",
                    case.eff_power
                ));
            }
            if i64::from(damage) != i64::from(case.eff_melee_damage) {
                problems.push(format!(
                    "group_melee.damage rust={damage} csharp={}",
                    case.eff_melee_damage
                ));
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
    let (seed, hit_interval, atk, cases, _) = load_skills();
    assert!(
        cases.len() >= 20,
        "已实装技能用例数 {} 少于 20（少登记了技能？）",
        cases.len()
    );
    // 第二批已实装技能的存在性硬约束：这些魔号必须有用例（防登记漏）
    for id in [9u16, 10, 22, 23, 24, 28, 29, 31, 33, 36, 45, 47, 50] {
        assert!(
            cases.iter().any(|c| c.magic.magic_id == id),
            "第二批技能 {id} 缺少用例"
        );
    }
    // 第三批（群体变体，复用同式表达式）：这些魔号必须有用例。
    for id in [42u16, 44, 48] {
        assert!(
            cases.iter().any(|c| c.magic.magic_id == id),
            "第三批技能 {id} 缺少用例"
        );
    }
    // 第五批（`MagGroupDeDing` 群体物理）：39 必须有用例，且效果类别为 group_melee。
    let melee = cases
        .iter()
        .find(|c| c.magic.magic_id == 39)
        .expect("第五批技能 39 缺少用例");
    assert_eq!(melee.kind, "group_melee", "39 的效果类别应为 group_melee");
    for case in &cases {
        let problems = skill_mismatches(case, seed, hit_interval, atk);
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
    let (_, _, _, cases, registry) = load_skills();
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
        assert!(
            !row.dospell_class.is_empty(),
            "魔表行 {} 没有 DoSpell 分派分类",
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

/// 登记判据（DoD③「未实装逐条登记」的机器门禁）：
/// 真魔表每一行按 oracle `DoSpell` 的控制流归入**唯一确定**的分派类别；
/// 类别分布逐类钉死（内容冻结 ⇒ 行数稳定），新增/漏登魔表行都会判红。
///
/// 类别口径（`src/M2Server/Magic/MagicManager.cs`）：
/// - `warr_reject`：`IsWarrSkill(MagIdx)` ⇒ `return false`（`:82-100,151`）
/// - `magicid_reject`：`MagicId > 80` ⇒ `return false`（`:167`）
/// - `implemented`：本片有用例（四种字段齐备）
/// - `context_required`：有 case，但效果需地图/目标/可见集/召唤/状态
/// - `no_effect_case`：有 case，但体内无副作用（`break` / 只置 `boTrain`）
/// - `no_switch_case`：switch 无此魔号分支
#[test]
fn dospell_classes_are_fully_registered() {
    let (_, _, _, cases, registry) = load_skills();
    assert_eq!(registry.len(), 108, "魔表应有 108 行（content 冻结）");

    let mut counts: BTreeMap<&str, i64> = BTreeMap::new();
    for row in &registry {
        match row.dospell_class.as_str() {
            "implemented" | "context_required" | "no_effect_case" | "no_switch_case"
            | "warr_reject" | "magicid_reject" => {}
            other => panic!("魔表行 {} 出现未登记的分派类别 {:?}", row.magic_id, other),
        }
        *counts.entry(row.dospell_class.as_str()).or_insert(0) += 1;
        if row.implemented {
            assert_eq!(
                row.dospell_class, "implemented",
                "魔表行 {} implemented=true 但类别为 {}",
                row.magic_id, row.dospell_class
            );
        }
    }
    assert_eq!(counts.get("implemented").copied(), Some(cases.len() as i64));
    // 内容冻结下的逐类分布（oracle DoSpell switch + IsWarrSkill + MagicId>80 唯一决定）。
    assert_eq!(counts.get("implemented").copied(), Some(29));
    assert_eq!(counts.get("context_required").copied(), Some(10));
    assert_eq!(counts.get("no_effect_case").copied(), Some(6));
    assert_eq!(counts.get("no_switch_case").copied(), Some(20));
    assert_eq!(counts.get("warr_reject").copied(), Some(8));
    assert_eq!(counts.get("magicid_reject").copied(), Some(35));
}

/// 判据（DoD③ 的可达性口径）：`WorldServer.FindMagic(int)` 的**首匹配**语义。
///
/// 真魔表 108 行里有 13 个 `MagID` 各出现 2 次（`Idx` 序先入表者胜）：`FindMagic` 取首行，
/// 后一行被遮蔽。本测试把该结构钉成机器门禁（content 冻结 ⇒ 数字稳定），并点名 48。
#[test]
fn duplicate_magic_ids_resolve_to_first_row_like_oracle() {
    let (_, _, _, _, registry) = load_skills();
    let magics: Vec<MagicInfo> = registry
        .iter()
        .map(|r| MagicInfo {
            magic_id: r.magic_id,
            magic_name: r.name.clone(),
            ..Default::default()
        })
        .collect();
    assert_eq!(magics.len(), 108, "真魔表行数（content 冻结）");

    let mut count: BTreeMap<u16, usize> = BTreeMap::new();
    for m in &magics {
        *count.entry(m.magic_id).or_insert(0) += 1;
    }
    assert_eq!(count.len(), 95, "不同 MagID 数（108 行 − 13 个重复）");
    let dups: Vec<u16> = count
        .iter()
        .filter(|(_, &n)| n > 1)
        .map(|(&id, _)| id)
        .collect();
    assert_eq!(
        dups,
        vec![48u16, 66, 71, 88, 89, 90, 91, 92, 93, 94, 96, 97, 98],
        "重复 MagID 集合（按魔号升序）"
    );
    for &id in &dups {
        assert_eq!(count[&id], 2, "重复 MagID {id} 应恰有 2 行");
        let first = magics
            .iter()
            .find(|m| m.magic_id == id)
            .expect("重复 MagID 必有首行");
        let resolved = resolve_magic(&magics, id).expect("按魔号必能解析");
        assert_eq!(
            resolved.magic_name, first.magic_name,
            "MagID {id} 必须解析为先入表的 {}",
            first.magic_name
        );
    }
    // 点名：48（`MagicConst.SKILL_GROUPAMYOUNSUL = 48`）先入表的是「群体施毒术」，
    // `Idx=48` 的「气功波」被遮蔽 ⇒ 运行时按魔号永远到不了「气功波」。
    assert_eq!(
        resolve_magic(&magics, 48).expect("48").magic_name,
        "群体施毒术"
    );
    let shadowed48: Vec<&str> = magics
        .iter()
        .filter(|m| m.magic_id == 48 && m.magic_name != "群体施毒术")
        .map(|m| m.magic_name.as_str())
        .collect();
    assert_eq!(shadowed48, vec!["气功波"], "48 的遮蔽行点名");
}

/// 判据（DoD③ 的计数口径）：`implemented` 必须按**可达魔号**计。
///
/// 探针按真魔表**行**产用例（108 行里 29 行 implemented），其中「48 气功波」是遮蔽行，
/// 不构成一个独立的已实装技能 ⇒ 可达魔号为 28。遮蔽行必须逐条登记、点名，不许静默漏。
#[test]
fn implemented_cases_are_counted_per_reachable_magic_id() {
    let (_, _, _, cases, registry) = load_skills();
    // 首行 = 可达；其余 = 遮蔽。
    let mut seen: BTreeSet<u16> = BTreeSet::new();
    let mut impl_effective = 0usize;
    let mut impl_shadowed: Vec<(u16, String)> = Vec::new();
    for row in &registry {
        let is_first_for_id = seen.insert(row.magic_id);
        if row.implemented {
            if is_first_for_id {
                impl_effective += 1;
            } else {
                impl_shadowed.push((row.magic_id, row.name.clone()));
            }
        }
    }
    assert_eq!(
        registry.iter().filter(|r| r.implemented).count(),
        cases.len(),
        "implemented 行数 = 用例数（探针按行产用例）"
    );
    assert_eq!(impl_effective, 28, "已实装**可达魔号**数");
    assert_eq!(
        impl_shadowed,
        vec![(48u16, "气功波".to_string())],
        "遮蔽的 implemented 行必须逐条登记且点名"
    );
    // 用例侧也点名 phantom，承认它是按行产出、不属于可达魔号。
    let phantom: Vec<&SkillCase> = cases
        .iter()
        .filter(|c| c.magic.magic_id == 48 && c.magic.magic_name == "气功波")
        .collect();
    assert_eq!(phantom.len(), 1, "48 的遮蔽行仍有一份按行用例（phantom）");
}

/// 红检⑨（技能·可达性）：`resolve_magic` 若被改成 **last-wins**（`HashMap` 覆盖 / 逆序取），
/// `FindMagic(48)` 会变成遮蔽行「气功波」⇒ 与 oracle 首匹配语义不一致。
/// 本测试把「首匹配 ≠ 末匹配」钉成可判红：若两值相同 ⇒ 首匹配语义没有牙齿。
#[test]
fn redcheck_duplicate_resolution_is_order_sensitive_must_fail() {
    let (_, _, _, _, registry) = load_skills();
    let magics: Vec<MagicInfo> = registry
        .iter()
        .map(|r| MagicInfo {
            magic_id: r.magic_id,
            magic_name: r.name.clone(),
            ..Default::default()
        })
        .collect();
    let first = resolve_magic(&magics, 48).expect("48").magic_name.clone();
    let last = magics
        .iter()
        .rev()
        .find(|m| m.magic_id == 48)
        .expect("48")
        .magic_name
        .clone();
    assert_eq!(first, "群体施毒术");
    assert_eq!(last, "气功波");
    assert_ne!(
        first, last,
        "红检失效：48 的首匹配与末匹配相同 ⇒ 首匹配语义被覆盖（Map/last-wins）"
    );
}

/// 判据（B-1 已接线）：`PlayObject.GetAttackPower` override（Luck/PowerRate/BoPowerItem）
/// 会让**取数次数**与 `BaseObject.GetAttackPower` 不同（Luck=0 时每次多抽一次 `Random(10)`）。
/// 世界宿主 `World::attack_once` 自 B-1 起对 `Race == Play` 的攻方走 [`AttackPowerMode::Player`]
/// （oracle：`BaseObject.Attack.cs:25` 的虚调用 → `PlayObject.cs:2909` override），
/// 怪物攻方仍走 `Base`（`BaseObject.Base.cs:886`）。
///
/// 本测试断言两种口径在同种子下**确为不同取数账本**；世界宿主的分派与取数次数分别由
/// `world_host_player_attacker_uses_playobject_override`、
/// `player_target_equipped_items_draw_random8_per_slot` 钉死。
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

/// 计数随机源：把 [`RandomSource`] 委托给同种子 [`RandomNumber`]，并记录每次 `random_below` 的入参。
/// 用来把「取数**次数与参数**」本身钉成机器可判（oracle 的 RNG 面就是顺序 + 次数）。
/// 注意 `get_hit_struck_damage` 走 `random_range`，不计入 `below`。
struct CountingRng {
    inner: RandomNumber,
    below: Vec<i32>,
    ranges: Vec<(i32, i32)>,
}

impl CountingRng {
    fn new(seed: i32) -> Self {
        Self {
            inner: RandomNumber::with_seed(seed),
            below: Vec::new(),
            ranges: Vec::new(),
        }
    }

    fn count(&self, arg: i32) -> usize {
        self.below.iter().filter(|&&v| v == arg).count()
    }

    /// 总取数次数：`random_below` + `random_range` 都算（两个入口都是 C# 的 `Random(n)`）。
    fn draws(&self) -> usize {
        self.below.len() + self.ranges.len()
    }
}

impl RandomSource for CountingRng {
    fn random(&mut self) -> i32 {
        self.inner.random()
    }

    fn random_below(&mut self, value: i32) -> i32 {
        self.below.push(value);
        self.inner.random_below(value)
    }

    fn random_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        self.ranges.push((min_value, max_value));
        self.inner.random_range(min_value, max_value)
    }

    fn next_double(&mut self) -> f64 {
        self.inner.next_double()
    }
}

/// B-1 判据（世界宿主分派 + oracle 行号）：玩家攻方必须走 `PlayObject.GetAttackPower` override
///（`PlayObject.cs:2909`：`Luck <= 0` 多一次 `Random(10)`），而不是 `BaseObject.GetAttackPower`
///（`BaseObject.Base.cs:886`）。
///
/// 做法：在 `World` 内连打两击，与 [`attack_with`] 的 `Player` 口径逐项核；并以 `Base` 口径作
/// **反例**（若 `world.rs` 回退成 Base，本条必红）；再用 [`CountingRng`] 把“Player 比 Base 多一次
/// `Random(10)`”钉死。
#[test]
fn world_host_player_attacker_uses_playobject_override() {
    const SEED: i32 = 20261012;
    const DC: i32 = 0x1E14;

    let mut world = World::new(32, 32);
    world.open_session("hero");
    let mut hero = Entity::new(1, "hero", 10, 10);
    hero.race = ACTOR_RACE_PLAY;
    hero.dc = DC;
    hero.hit_point = 20;
    hero.luck = 0;
    hero.power_rate = 100;
    hero.hp = 200;
    hero.max_hp = 200;
    let hero_id = world.enter_map("hero", hero, 0).expect("进图");
    let mut mon = Entity::new(2, "mon", 11, 10);
    mon.race = ACTOR_RACE_MONSTER;
    mon.ac = 0;
    mon.hp = 60000;
    mon.max_hp = 60000;
    mon.hit_point = 0; // 不掷闪避，聚焦攻方口径
    let mon_id = world.add_entity(mon);
    world.tick(); // 处理 Enter 落图

    // 聚焦攻击路径：把种子重置到攻击前（与 WorldGolden / combat-probe 同一种子注入口径）。
    world.rng = RandomNumber::with_seed(SEED);
    world.attack_cmd(hero_id, mon_id);
    world.attack_cmd(hero_id, mon_id);
    let rec = world.tick();
    assert_eq!(rec.attacks.len(), 2, "两次攻击都应结算");
    let world_powers = [rec.attacks[0].2.power, rec.attacks[1].2.power];

    let config = CombatConfig::default();
    let mk = || {
        let mut a = Entity::new(1, "hero", 0, 0);
        a.race = ACTOR_RACE_PLAY;
        a.dc = DC;
        a.hit_point = 20;
        let mut t = Entity::new(2, "mon", 0, 0);
        t.race = ACTOR_RACE_MONSTER;
        t.ac = 0;
        t.hp = 60000;
        t.max_hp = 60000;
        t.hit_point = 0;
        (a, t)
    };
    let player_mode = AttackPowerMode::Player {
        luck: 0,
        power_rate: 100,
        bo_power_item: false,
        power_item: 0,
    };
    let (mut a, mut t) = mk();
    let mut rng_p = RandomNumber::with_seed(SEED);
    let p = [
        attack_with(&mut rng_p, &mut a, &mut t, None, &config, player_mode).power,
        attack_with(&mut rng_p, &mut a, &mut t, None, &config, player_mode).power,
    ];
    let (mut a, mut t) = mk();
    let mut rng_b = RandomNumber::with_seed(SEED);
    let b = [
        attack_with(
            &mut rng_b,
            &mut a,
            &mut t,
            None,
            &config,
            AttackPowerMode::Base,
        )
        .power,
        attack_with(
            &mut rng_b,
            &mut a,
            &mut t,
            None,
            &config,
            AttackPowerMode::Base,
        )
        .power,
    ];
    assert_eq!(
        world_powers, p,
        "世界宿主玩家攻方必须与 PlayObject override 口径逐项相同（oracle：PlayObject.cs:2909）"
    );
    assert_ne!(
        p, b,
        "红检失效：Player 与 Base 两口径在同种子下不可分辨 ⇒ 本判据没有牙齿"
    );

    let mut rng_player = CountingRng::new(SEED);
    let _ = get_base_attack_power_player(&mut rng_player, DC, 0, 100, false, 0);
    assert_eq!(
        rng_player.draws(),
        2,
        "Luck=0 的 Player override 应抽 2 次：Random(power+1) + Random(10)"
    );
    assert_eq!(
        rng_player.count(10),
        1,
        "Player override 的第二次抽数就是 Random(10)（PlayObject.cs:2917）"
    );
    let mut rng_base = CountingRng::new(SEED);
    let _ = get_base_attack_power(&mut rng_base, DC);
    assert_eq!(
        rng_base.draws(),
        1,
        "BaseObject.GetAttackPower 只抽 1 次（BaseObject.Base.cs:886）"
    );
}

/// B-2 判据：`PlayObject.StruckDamage`（`PlayObject.cs:2954-3039`）对**玩家目标**在
/// `base.StruckDamage` 之后无条件抽一次 `Random(10)`（`:2957`），并逐格对 `Index > 0` 的
/// 已装备物品各抽一次 `Random(8)`（`:2998-3039`）。
///
/// 用 [`CountingRng`] 直接数：3 个已装备格 ⇒ `random_below(8)` 恰好 3 次；
/// 空装备位（`Index == 0`）**不抽**（C# 的 `&&` 短路），并把这一条作为反例。
/// 若谁把逐格循环删掉（或改成给空位也抽/给已装备位漏抽），本条必红。
#[test]
fn player_target_equipped_items_draw_random8_per_slot() {
    let config = CombatConfig::default();
    let mk_attacker = || {
        let mut a = Entity::new(1, "hero", 0, 0);
        a.race = ACTOR_RACE_PLAY;
        a.dc = 0x1E14;
        a.hit_point = 20;
        a
    };
    let mk_target = || {
        let mut t = Entity::new(2, "victim", 0, 0);
        t.race = ACTOR_RACE_PLAY; // 玩家目标 ⇒ 走 PlayObject.StruckDamage override
        t.ac = 0;
        t.hp = 60000;
        t.max_hp = 60000;
        t.hit_point = 0; // 不掷闪避
        t
    };

    let mut attacker = mk_attacker();
    let mut target = mk_target();
    // 三个已装备格：0（衣服 · 循环里也会各抽一次 R(8)）、1、7。
    for slot in [0usize, 1, 7] {
        target.use_items[slot].index = 100 + slot as u16;
        target.use_items[slot].dura = 60000;
    }
    let mut rng = CountingRng::new(20261012);
    let out = attack_with(
        &mut rng,
        &mut attacker,
        &mut target,
        None,
        &config,
        AttackPowerMode::Base,
    );
    assert!(
        out.damage > 0,
        "前提：必须命中并造成伤害，否则 StruckDamage 不跑"
    );
    assert_eq!(
        rng.count(8),
        3,
        "3 个已装备格 ⇒ 逐格循环恰好 3 次 Random(8)（PlayObject.cs:2998-3039）"
    );
    assert_eq!(
        rng.count(10),
        1,
        "玩家目标的 StruckDamage 无条件抽 1 次 Random(10)（PlayObject.cs:2957）"
    );

    // 反例：空装备位不抽 Random(8)。
    let mut attacker = mk_attacker();
    let mut empty = mk_target();
    let mut rng_empty = CountingRng::new(20261012);
    let out_empty = attack_with(
        &mut rng_empty,
        &mut attacker,
        &mut empty,
        None,
        &config,
        AttackPowerMode::Base,
    );
    assert!(out_empty.damage > 0, "前提：空装备也要能命中");
    assert_eq!(
        rng_empty.count(8),
        0,
        "空装备位（Index == 0）不得抽 Random(8)（C# 的 `&&` 短路）"
    );
    assert_eq!(rng_empty.count(10), 1, "空装备也要抽那条无条件 Random(10)");
}

/// 定向随机源：`random_below(arg)` 命中 `force` 表时返回**指定值**（用来构造耐久“恰好破零”/
/// “短耐久回绕”这类 C# 的 `ushort` 语义角落），其余入口委托同种子 [`RandomNumber`]；
/// 同时记录每次 `random_below` 的入参。
struct TargetedRng {
    inner: RandomNumber,
    below: Vec<i32>,
    force: BTreeMap<i32, i32>,
}

impl TargetedRng {
    fn new(seed: i32, force: [(i32, i32); 2]) -> Self {
        Self {
            inner: RandomNumber::with_seed(seed),
            below: Vec::new(),
            force: force.into_iter().collect(),
        }
    }

    fn count(&self, arg: i32) -> usize {
        self.below.iter().filter(|&&v| v == arg).count()
    }
}

impl RandomSource for TargetedRng {
    fn random(&mut self) -> i32 {
        self.inner.random()
    }

    fn random_below(&mut self, value: i32) -> i32 {
        self.below.push(value);
        if let Some(&v) = self.force.get(&value) {
            return v;
        }
        self.inner.random_below(value)
    }

    fn random_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        self.inner.random_range(min_value, max_value)
    }

    fn next_double(&mut self) -> f64 {
        self.inner.next_double()
    }
}

/// 构造一个“必定命中、必定造成伤害”的玩家/玩家对，并把若干装备位设成给定 `(slot, index, dura)`。
fn equipped_target(slots: &[(usize, u16, u16)]) -> (Entity, Entity) {
    let mut attacker = Entity::new(1, "hero", 0, 0);
    attacker.race = ACTOR_RACE_PLAY;
    attacker.dc = 0x1E14;
    attacker.hit_point = 20;
    let mut target = Entity::new(2, "victim", 0, 0);
    target.race = ACTOR_RACE_PLAY;
    target.ac = 0;
    target.hp = 60000;
    target.max_hp = 60000;
    target.hit_point = 0; // 不掷闪避
    for &(slot, index, dura) in slots {
        target.use_items[slot].index = index;
        target.use_items[slot].dura = dura;
    }
    (attacker, target)
}

/// 衣服位（`ItemLocation.Dress = 0`）**恰好破零**（`Dura == nDam`）⇒ 清 `Index`
/// ⇒ 同一击的逐格循环**跳过** slot 0 的 `Random(8)`（`PlayObject.cs:2965-2996` + `:2998-3039`）。
///
/// 破零必红手法：把 `wear_durability` 里的 `slot.index = 0` 去掉（或删掉衣服位特判）⇒
/// `rng.count(8)` 从 1 变 2，本条 FAILED。
#[test]
fn dress_break_clears_index_and_skips_random8_in_same_attack() {
    let config = CombatConfig::default();
    // force: Random(10)=0 ⇒ nDam=5；Random(8)=7 ⇒ 循环里的每一格都不扣耐久（只数取数）。
    let mut rng = TargetedRng::new(20261012, [(10, 0), (8, 7)]);
    // 衣服 Dura=5 == nDam ⇒ 破零；slot 1 正常装备。
    let (mut attacker, mut target) = equipped_target(&[(0, 100, 5), (1, 101, 60000)]);
    let out = attack_with(
        &mut rng,
        &mut attacker,
        &mut target,
        None,
        &config,
        AttackPowerMode::Base,
    );
    assert!(
        out.damage > 0,
        "前提：必须命中并造成伤害，否则 StruckDamage 不跑"
    );
    assert_eq!(
        target.use_items[0].index, 0,
        "衣服破零后 Index 必被清 0（PlayObject.cs:2982）"
    );
    assert_eq!(
        target.use_items[0].dura, 0,
        "衣服破零后 Dura 必为 0（PlayObject.cs:2985）"
    );
    assert_eq!(
        target.use_items[1].dura, 60000,
        "Random(8)=7 ⇒ 逐格循环不扣耐久"
    );
    assert_eq!(
        rng.count(8),
        1,
        "衣服在特判里破零 ⇒ 同一击逐格循环只给 slot 1 抽 Random(8)（不是 2）"
    );
}

/// 衣服位**耐久 < nDam**（`Dura=3 < nDam=5`）：C# `ushort nDura -= nDam` **回绕**成大正数
/// ⇒ **不破零**、`Index` 保留、逐格循环照抽 `Random(8)`（`PlayObject.cs:2969-2970`）。
///
/// 破零必红手法：把 `wear_durability` 的 `wrapping_sub` 改成 `saturating_sub`（或“<= nDam 就破零”）
/// ⇒ `count(8)` 从 2 变 1 且 `Index` 被清 ⇒ 本条 FAILED。
#[test]
fn dress_durability_below_damage_wraps_without_breaking() {
    let config = CombatConfig::default();
    let mut rng = TargetedRng::new(20261012, [(10, 0), (8, 7)]);
    let (mut attacker, mut target) = equipped_target(&[(0, 100, 3), (1, 101, 60000)]);
    let out = attack_with(
        &mut rng,
        &mut attacker,
        &mut target,
        None,
        &config,
        AttackPowerMode::Base,
    );
    assert!(out.damage > 0, "前提：必须命中并造成伤害");
    assert_eq!(
        target.use_items[0].index, 100,
        "耐久短于 nDam 时按 ushort 回绕 ⇒ 不破零、Index 保留"
    );
    assert_eq!(
        target.use_items[0].dura, 65534,
        "3u16.wrapping_sub(5) == 65534（unchecked ushort 语义）"
    );
    assert_eq!(
        rng.count(8),
        2,
        "衣服未破零 ⇒ 同一击里 slot 0 + slot 1 各抽一次 Random(8)"
    );
}

/// 逐格循环里的破零同样清 `Index`（`PlayObject.cs:3008`/`:3012`）：`Random(8)==0` 且
/// `Dura == nDam` 的格子被清位。影响的是**后续攻击**的取数（本击的抽数已发生）。
///
/// 破零必红手法：让循环只改 `dura` 不清 `index` ⇒ 第二击仍会给 slot1 抽 `Random(8)`，
/// `rng.count(8)` 变 2，本条 FAILED。
#[test]
fn loop_break_clears_index_and_affects_next_attack_draws() {
    let config = CombatConfig::default();
    // 第一次攻击：衣服 Dura==nDam 破零；slot1 也 Dura==nDam 且 Random(8)=0 ⇒ 破零。
    let mut rng = TargetedRng::new(20261012, [(10, 0), (8, 0)]);
    let (mut attacker, mut target) = equipped_target(&[(0, 100, 5), (1, 101, 5)]);
    let out1 = attack_with(
        &mut rng,
        &mut attacker,
        &mut target,
        None,
        &config,
        AttackPowerMode::Base,
    );
    assert!(out1.damage > 0, "前提：第一击必须造成伤害");
    assert_eq!(target.use_items[0].index, 0, "衣服在第一击破零");
    assert_eq!(
        target.use_items[1].index, 0,
        "slot1 在循环里 Random(8)==0 + Dura==nDam ⇒ 破零"
    );
    assert_eq!(
        rng.count(8),
        1,
        "第一击只给 slot1 抽 Random(8)（slot0 已被特判清位）"
    );
    // 第二次攻击：两格都被清位 ⇒ 逐格循环 0 抽。
    let out2 = attack_with(
        &mut rng,
        &mut attacker,
        &mut target,
        None,
        &config,
        AttackPowerMode::Base,
    );
    assert!(out2.damage > 0, "前提：第二击必须造成伤害");
    assert_eq!(
        rng.count(8),
        1,
        "第二击两格都已清位 ⇒ 不再抽 Random(8)（全局流位置与 oracle 同步的关键）"
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
    let (seed, hit_interval, atk, cases, _) = load_skills();
    let case = &cases[0];
    assert!(
        skill_mismatches(case, seed, hit_interval, atk).is_empty(),
        "前提：未改坏前必须全等"
    );
    let mut corrupt = case.clone();
    let old = corrupt.mp_cost[1];
    corrupt.mp_cost[1] = old + 1;
    assert!(
        !skill_mismatches(&corrupt, seed, hit_interval, atk).is_empty(),
        "红检失效：把技能 {} 的 MP 消耗 {old} 改成 {} 后仍判等",
        corrupt.magic.magic_id,
        old + 1
    );
}

/// 红检③（技能·冷却）：改坏 `MagicAttackInterval` 一个值 ⇒ 该技能用例必 RED。
#[test]
fn redcheck_corrupt_one_cooldown_must_fail() {
    let (seed, hit_interval, atk, cases, _) = load_skills();
    let case = &cases[0];
    let mut corrupt = case.clone();
    let old = corrupt.interval;
    corrupt.interval = old + 1;
    assert!(
        !skill_mismatches(&corrupt, seed, hit_interval, atk).is_empty(),
        "红检失效：把 MagicAttackInterval {old} 改成 {} 后仍判等",
        old + 1
    );
}

/// 红检④（技能·效果威力）：改坏魔法盾的 `power` 一个值 ⇒ 该技能用例必 RED。
#[test]
fn redcheck_corrupt_one_shield_power_must_fail() {
    let (seed, hit_interval, atk, cases, _) = load_skills();
    let case = cases
        .iter()
        .find(|c| c.kind == "shield")
        .expect("向量里应有魔法盾（shield）用例");
    assert!(
        skill_mismatches(case, seed, hit_interval, atk).is_empty(),
        "前提：未改坏前魔法盾必须全等"
    );
    let mut corrupt = case.clone();
    let old = corrupt.eff_power;
    corrupt.eff_power = old + 1;
    assert!(
        !skill_mismatches(&corrupt, seed, hit_interval, atk).is_empty(),
        "红检失效：把魔法盾威力 {old} 改成 {} 后仍判等",
        old + 1
    );
}

/// 红检⑤（技能·效果时长）：改坏火墙的 `time` 一个值 ⇒ 该技能用例必 RED。
#[test]
fn redcheck_corrupt_one_fire_wall_time_must_fail() {
    let (seed, hit_interval, atk, cases, _) = load_skills();
    let case = cases
        .iter()
        .find(|c| c.kind == "fire_wall")
        .expect("向量里应有火墙（fire_wall）用例");
    assert!(
        skill_mismatches(case, seed, hit_interval, atk).is_empty(),
        "前提：未改坏前火墙必须全等"
    );
    let mut corrupt = case.clone();
    let old = corrupt.eff_time;
    corrupt.eff_time = old + 1;
    assert!(
        !skill_mismatches(&corrupt, seed, hit_interval, atk).is_empty(),
        "红检失效：把火墙时长 {old} 改成 {} 后仍判等",
        old + 1
    );
}

/// 红检⑥（技能·效果威力）：改坏 42（`SKILL_GROUPLIGHTENING`）的不死系倍率一个值 ⇒ 该技能用例必 RED。
#[test]
fn redcheck_corrupt_one_group_lightening_vs_undead_must_fail() {
    let (seed, hit_interval, atk, cases, _) = load_skills();
    let case = cases
        .iter()
        .find(|c| c.magic.magic_id == 42)
        .expect("向量里应有 42（SKILL_GROUPLIGHTENING）用例");
    assert!(
        skill_mismatches(case, seed, hit_interval, atk).is_empty(),
        "前提：未改坏前 42 必须全等"
    );
    let mut corrupt = case.clone();
    let old = corrupt.eff_vs_undead;
    corrupt.eff_vs_undead = old + 1;
    assert!(
        !skill_mismatches(&corrupt, seed, hit_interval, atk).is_empty(),
        "红检失效：把 42 的不死系倍率 {old} 改成 {} 后仍判等",
        old + 1
    );
}

/// 红检⑦（技能·效果威力）：改坏 48（`SKILL_GROUPAMYOUNSUL`）的绿毒威力一个值 ⇒ 该技能用例必 RED。
#[test]
fn redcheck_corrupt_one_group_poison_green_must_fail() {
    let (seed, hit_interval, atk, cases, _) = load_skills();
    let case = cases
        .iter()
        .find(|c| c.magic.magic_id == 48)
        .expect("向量里应有 48（SKILL_GROUPAMYOUNSUL）用例");
    assert!(
        skill_mismatches(case, seed, hit_interval, atk).is_empty(),
        "前提：未改坏前 48 必须全等"
    );
    let mut corrupt = case.clone();
    let old = corrupt.eff_green;
    corrupt.eff_green = old + 1;
    assert!(
        !skill_mismatches(&corrupt, seed, hit_interval, atk).is_empty(),
        "红检失效：把 48 的绿毒威力 {old} 改成 {} 后仍判等",
        old + 1
    );
}

/// 红检⑧（技能·效果威力）：改坏 39（`SKILL_GROUPDEDING`→`MagGroupDeDing`）的护甲减免后伤害一个值 ⇒ 必 RED。
#[test]
fn redcheck_corrupt_one_group_melee_damage_must_fail() {
    let (seed, hit_interval, atk, cases, _) = load_skills();
    let case = cases
        .iter()
        .find(|c| c.magic.magic_id == 39)
        .expect("向量里应有 39（SKILL_GROUPDEDING）用例");
    assert!(
        skill_mismatches(case, seed, hit_interval, atk).is_empty(),
        "前提：未改坏前 39 必须全等"
    );
    let mut corrupt = case.clone();
    let old = corrupt.eff_melee_damage;
    corrupt.eff_melee_damage = old + 1;
    assert!(
        !skill_mismatches(&corrupt, seed, hit_interval, atk).is_empty(),
        "红检失效：把 39 的群体物理伤害 {old} 改成 {} 后仍判等",
        old + 1
    );
}

// -------------------------------------------------------------------------------------------
// F-K / R-1：`IsProperTarget` 判定链 + 零 RNG 消费者 PvP 攻击序列
//   （`mir2-rs/tests/parity/m3/proper-target-probe` → `vectors/proper_target_vectors.tsv`）
// -------------------------------------------------------------------------------------------

/// `pt` 行 = oracle `IsProperTarget` 的一个边界输入 + 期望。
#[derive(Clone, Debug)]
pub struct ProperCase {
    pub name: String,
    pub attacker_is_play: bool,
    pub attacker_race: u8,
    pub target_race: u8,
    pub attack_mode: u8,
    pub attacker_safe: bool,
    pub target_safe: bool,
    pub target_admin: bool,
    pub target_stone: bool,
    pub target_has_master: bool,
    pub master_is_attacker: bool,
    pub dear_is_target: bool,
    pub attacker_pk: i32,
    pub target_pk: i32,
    pub attacker_nasty: bool,
    pub expected: bool,
}

/// `pvp` 行 = 同一种子下第 i 次 `_Attack` 的真身结果。
#[derive(Clone, Debug)]
pub struct PvpRow {
    pub i: i64,
    pub power: i32,
    pub damage: i32,
    pub hp_after: u16,
    pub hit: bool,
}

#[derive(Clone, Debug)]
pub struct PvpSeq {
    pub name: String,
    pub rows: Vec<PvpRow>,
    pub hits: i64,
    pub dodges: i64,
    pub attacked: i64,
    pub damage_sum: i64,
}

/// 读 `proper_target_vectors.tsv`：`(seed, proper cases, pvp seqs)`。
pub fn load_proper_target(seed: &mut i32, cases: &mut Vec<ProperCase>, seqs: &mut Vec<PvpSeq>) {
    let lines = read_lines("proper-target-probe/vectors/proper_target_vectors.tsv");
    for (n, fields) in lines.iter().enumerate() {
        match fields[0].as_str() {
            "seed" => *seed = i(fields, 1) as i32,
            "pt" => cases.push(ProperCase {
                name: fields[1].clone(),
                attacker_is_play: i(fields, 2) != 0,
                attacker_race: i(fields, 3) as u8,
                target_race: i(fields, 4) as u8,
                attack_mode: i(fields, 5) as u8,
                attacker_safe: i(fields, 6) != 0,
                target_safe: i(fields, 7) != 0,
                target_admin: i(fields, 8) != 0,
                target_stone: i(fields, 9) != 0,
                target_has_master: i(fields, 10) != 0,
                master_is_attacker: i(fields, 11) != 0,
                dear_is_target: i(fields, 12) != 0,
                attacker_pk: i(fields, 13) as i32,
                target_pk: i(fields, 14) as i32,
                attacker_nasty: i(fields, 15) != 0,
                expected: i(fields, 16) != 0,
            }),
            "pvp" => {
                let name = fields[1].clone();
                if seqs.last().map(|s| s.name.as_str()) != Some(name.as_str()) {
                    seqs.push(PvpSeq {
                        name,
                        rows: Vec::new(),
                        hits: 0,
                        dodges: 0,
                        attacked: 0,
                        damage_sum: 0,
                    });
                }
                let seq = seqs.last_mut().unwrap();
                assert_eq!(i(fields, 2) as usize, seq.rows.len(), "pvp i 不连续");
                seq.rows.push(PvpRow {
                    i: i(fields, 2),
                    power: i(fields, 3) as i32,
                    damage: i(fields, 4) as i32,
                    hp_after: i(fields, 5) as u16,
                    hit: i(fields, 6) != 0,
                });
            }
            "pvpstats" => {
                let seq = seqs
                    .last_mut()
                    .unwrap_or_else(|| panic!("第 {n} 行 pvpstats 没有前导 pvp 序列"));
                seq.hits = i(fields, 2);
                seq.dodges = i(fields, 3);
                seq.attacked = i(fields, 4);
                seq.damage_sum = i(fields, 5);
            }
            other => panic!("第 {n} 行未知前缀 {other}"),
        }
    }
}

/// 由向量行构造 Rust 侧两侧实体，跑一次 `is_proper_target`；返回与期望的差异清单。
fn proper_mismatches(case: &ProperCase) -> Vec<String> {
    let mut problems = Vec::new();
    let mut attacker = Entity::new(101, "attacker", 0, 0);
    attacker.race = case.attacker_race;
    attacker.attack_mode = case.attack_mode;
    attacker.in_safe_zone = case.attacker_safe;
    attacker.pk_point = case.attacker_pk;
    attacker.nasty_mode = case.attacker_nasty;
    // 向量里 `attacker_is_play` 与 `race == Play` 必须一致（oracle 用对象类型分派，这里用 race）。
    assert_eq!(
        case.attacker_is_play,
        case.attacker_race == ACTOR_RACE_PLAY,
        "{}: attacker_is_play 与 race 不一致",
        case.name
    );
    let mut target = Entity::new(202, "target", 0, 0);
    target.race = case.target_race;
    target.in_safe_zone = case.target_safe;
    target.admin_mode = case.target_admin;
    target.stone_mode = case.target_stone;
    target.pk_point = case.target_pk;
    if case.target_has_master {
        target.master = Some(if case.master_is_attacker { 101 } else { 303 });
    }
    if case.dear_is_target {
        attacker.dear_human = Some(target.id);
    }
    let got = is_proper_target(&attacker, &target);
    if got != case.expected {
        problems.push(format!(
            "{}: proper rust={got} csharp={}",
            case.name, case.expected
        ));
    }
    problems
}

/// PvP 序列核对：同种子逐 k 复现 oracle；返回差异清单。
fn pvp_mismatches(seq: &PvpSeq, seed: i32, config: &CombatConfig) -> Vec<String> {
    let mut problems = Vec::new();
    let mut rng = RandomNumber::with_seed(seed);
    let mut attacker = Entity::new(101, "attacker", 0, 0);
    attacker.race = ACTOR_RACE_PLAY;
    attacker.dc = 0x1E14;
    attacker.hit_point = 20;
    attacker.speed_point = 12;
    let mut target = Entity::new(202, "victim", 0, 0);
    target.race = ACTOR_RACE_PLAY;
    target.ac = 0x0A05;
    target.hp = 30000;
    target.max_hp = 30000;
    target.hit_point = 20;
    target.speed_point = 20;
    let mode = AttackPowerMode::Player {
        luck: 0,
        power_rate: 100,
        bo_power_item: false,
        power_item: 0,
    };

    let mut hits = 0i64;
    let mut dodges = 0i64;
    let mut attacked = 0i64;
    let mut damage_sum = 0i64;
    for (idx, exp) in seq.rows.iter().enumerate() {
        let out = attack_with(&mut rng, &mut attacker, &mut target, None, config, mode);
        if i64::from(out.power) != i64::from(exp.power) {
            problems.push(format!(
                "[{}] power rust={} csharp={}",
                exp.i, out.power, exp.power
            ));
        }
        if i64::from(out.damage) != i64::from(exp.damage) {
            problems.push(format!(
                "[{}] damage rust={} csharp={}",
                exp.i, out.damage, exp.damage
            ));
        }
        if out.target_hp_after != exp.hp_after {
            problems.push(format!(
                "[{}] hp rust={} csharp={}",
                exp.i, out.target_hp_after, exp.hp_after
            ));
        }
        if out.hit != exp.hit {
            problems.push(format!(
                "[{}] hit rust={} csharp={}",
                exp.i, out.hit, exp.hit
            ));
        }
        if out.hit {
            hits += 1;
        } else {
            dodges += 1;
        }
        if out.damage > 0 {
            attacked += 1;
        }
        damage_sum += i64::from(out.damage);
        assert_eq!(idx as i64, exp.i, "pvp 行序不等于 k");
    }
    if hits != seq.hits {
        problems.push(format!("hits rust={hits} csharp={}", seq.hits));
    }
    if dodges != seq.dodges {
        problems.push(format!("dodges rust={dodges} csharp={}", seq.dodges));
    }
    if damage_sum != seq.damage_sum {
        problems.push(format!(
            "damage_sum rust={damage_sum} csharp={}",
            seq.damage_sum
        ));
    }
    if attacked != seq.attacked {
        problems.push(format!("attacked rust={attacked} csharp={}", seq.attacked));
    }
    // 非平凡门（oracle 侧 `attacked` 应在向量生成时已 ≥1）。
    if seq.attacked < 1 {
        problems.push("attacked<1（非平凡门）".to_string());
    }
    problems
}

/// F-K：`IsProperTarget` 判定链逐个边界与 oracle 一致（含 HAM_ALL PvP、安全区、自己宝宝、攻击模式）。
#[test]
fn proper_target_matches_csharp_on_boundaries() {
    let mut seed = 0i32;
    let mut cases = Vec::new();
    let mut seqs = Vec::new();
    load_proper_target(&mut seed, &mut cases, &mut seqs);
    assert!(cases.len() >= 20, "pt 向量太少: {}", cases.len());
    // 覆盖度自检：F-K 核心（HAM_ALL 允许 PvP）与安全区拒绝必须真的在向量里。
    let names: BTreeSet<&str> = cases.iter().map(|c| c.name.as_str()).collect();
    for must in [
        "pvp_ham_all_nosafe",
        "pvp_ham_all_atk_safe",
        "pvp_ham_all_tgt_safe",
        "own_pet_ham_all",
        "own_pet_ham_peace",
    ] {
        assert!(names.contains(must), "缺边界向量 {must}");
    }
    let mut problems = Vec::new();
    for c in &cases {
        problems.extend(proper_mismatches(c));
    }
    assert!(
        problems.is_empty(),
        "IsProperTarget 差异:\n{}",
        problems.join("\n")
    );
}

/// R-1：零 RNG 消费者（无地图/无怪 AI）下，同种子 PvP 攻击序列逐 k 与 oracle 相同。
#[test]
fn pvp_sequence_matches_csharp_per_k() {
    let mut seed = 0i32;
    let mut cases = Vec::new();
    let mut seqs = Vec::new();
    load_proper_target(&mut seed, &mut cases, &mut seqs);
    assert_eq!(seqs.len(), 1, "应有且仅有 1 条 pvp 序列");
    let seq = &seqs[0];
    assert!(
        seq.rows.len() >= 100,
        "pvp 序列应 ≥100 次: {}",
        seq.rows.len()
    );
    // 非平凡门：至少有一次命中且伤害 > 0（防恒 0 比较器）。
    assert!(seq.hits >= 1, "pvp 序列一次都没命中 ⇒ 判据无仲裁力");
    assert!(seq.damage_sum > 0, "pvp 序列伤害全为 0 ⇒ 判据无仲裁力");
    let problems = pvp_mismatches(seq, seed, &CombatConfig::default());
    assert!(
        problems.is_empty(),
        "PvP 序列差异:\n{}",
        problems.join("\n")
    );
}

/// 红检⑨（F-K）：把 `pvp_ham_all_nosafe` 的攻击模式改成 `HAM_PEACE`（PvP 本应被拒）
/// ⇒ 该向量必与期望不符（证明 `proper_mismatches` 有牙齿，不是恒绿）。
#[test]
fn redcheck_ham_all_pvp_boundary_must_fail() {
    let mut seed = 0i32;
    let mut cases = Vec::new();
    let mut seqs = Vec::new();
    load_proper_target(&mut seed, &mut cases, &mut seqs);
    let case = cases
        .iter()
        .find(|c| c.name == "pvp_ham_all_nosafe")
        .expect("向量里应有 pvp_ham_all_nosafe");
    assert!(proper_mismatches(case).is_empty(), "前提：未改坏前必须全等");
    let mut corrupt = case.clone();
    corrupt.attack_mode = ATTACK_MODE_HAM_PEACE;
    assert!(
        !proper_mismatches(&corrupt).is_empty(),
        "红检失效：把 HAM_ALL 改成 HAM_PEACE 后仍判等"
    );
}

/// 红检⑩（R-1）：改坏 PvP 序列里一个伤害值 ⇒ 逐 k 比对必 RED。
#[test]
fn redcheck_corrupt_one_pvp_damage_must_fail() {
    let mut seed = 0i32;
    let mut cases = Vec::new();
    let mut seqs = Vec::new();
    load_proper_target(&mut seed, &mut cases, &mut seqs);
    let seq = &seqs[0];
    assert!(
        pvp_mismatches(seq, seed, &CombatConfig::default()).is_empty(),
        "前提：未改坏前必须全等"
    );
    let mut corrupt = seq.clone();
    let old = corrupt.rows[0].damage;
    corrupt.rows[0].damage = old + 1;
    assert!(
        !pvp_mismatches(&corrupt, seed, &CombatConfig::default()).is_empty(),
        "红检失效：把 PvP k=0 伤害 {old} 改成 {} 后仍判等",
        old + 1
    );
}

/// 红检⑪（F-K）：把自己宝宝的 HAM_ALL 允许改成拒绝（用 `HAM_PEACE` 模拟）⇒ 必 RED。
#[test]
fn redcheck_own_pet_boundary_must_fail() {
    let mut seed = 0i32;
    let mut cases = Vec::new();
    let mut seqs = Vec::new();
    load_proper_target(&mut seed, &mut cases, &mut seqs);
    let case = cases
        .iter()
        .find(|c| c.name == "own_pet_ham_all")
        .expect("向量里应有 own_pet_ham_all");
    assert!(proper_mismatches(case).is_empty(), "前提：未改坏前必须全等");
    let mut corrupt = case.clone();
    corrupt.attack_mode = ATTACK_MODE_HAM_PEACE;
    assert!(
        !proper_mismatches(&corrupt).is_empty(),
        "红检失效：把自己宝宝改成非 HAM_ALL 后仍判等"
    );
}
