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

use std::collections::BTreeMap;
use std::path::PathBuf;

use mir2_data::models::MagicInfo;
use mir2_shared::rng::RandomNumber;
use mir2_world::magic::{
    can_read_book, get_spell_point, group_melee_damage, magic_attack_interval, power_defence_area,
    power_fire_wall, power_get_power_plus_mc, power_get_power_plus_sc,
    power_heal_get_power_plus_sc2, power_holy_shield, power_lightening, power_poison, power_shield,
    power_show_hp_interval, power_transparent, power_uenhancer,
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
/// - `warr_reject`：`IsWarrSkill(MagIdx)` ⇒ `return false`（`:82-105,151`）
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
