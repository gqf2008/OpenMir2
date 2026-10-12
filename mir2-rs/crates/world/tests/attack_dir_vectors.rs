//! owner 2026-10-12 19:20 裁决②的真身对拍：`PlayObject.AttackDir`（`PlayObject.Attack.cs:25`）
//! 的**完整取数口径**——`GetBaseAttackPoewr`（`PlayObject.GetAttackPower` override，`Luck<=0` 两抽）
//! → `_Attack`（命中门 `RandomByte(target.SpeedPoint)`；命中再 `GetHitStruckDamage`）
//! → `AttackSuccess`（`:225` `Random(5)`；**命中/躲闪都抽**）。
//!
//! 向量来源 = `mir2-rs/tests/parity/m3/attack-dir-probe`（C#，反射直调真身 `AttackDir`，
//! `RecordingRandom` 记账成 `seq|kind|arg|result|site`）→ `vectors/attack_dir_vectors.{json,tsv}`。
//!
//! 本用例用**同一 seed** 跑 Rust [`attack_dir`]（= 世界宿主 `World::attack_once` 的口径），逐击核对
//! `(power, damage, hp_after, hit)`；`no_target_power_only` 用 [`swing_power`] 核对
//! （`World::attack_swing` 的取数段）。
//!
//! **改坏必红**（见 [`redcheck_dropping_attack_success_shifts_sequence`]）：把 `attack_dir` 末尾的
//! `attack_success_weapon_damage` 删掉（= 旧 `attack_with` 口径）⇒ 第 2 击起 RNG 流错位、伤害不符。

use std::path::PathBuf;

use mir2_shared::rng::RandomNumber;
use mir2_world::combat::swing_power;
use mir2_world::{
    attack_dir, attack_with, AttackPowerMode, CombatConfig, Entity, ACTOR_RACE_MONSTER,
    ACTOR_RACE_PLAY,
};

fn vectors_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/parity/m3/attack-dir-probe/vectors/attack_dir_vectors.tsv")
}

const SEED: i32 = 20261012;

#[derive(Debug, Clone)]
struct Atk {
    power: i32,
    damage: i32,
    hp_after: u16,
    hit: bool,
}

#[derive(Debug)]
struct Case {
    name: String,
    dc: i32,
    hit_point: u8,
    ac: i32,
    speed_point: u8,
    power_only: bool,
    draws: usize,
    attacks: Vec<Atk>,
}

fn load() -> Vec<Case> {
    let path = vectors_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读不到真身向量 {}: {e}", path.display()));
    let mut cases: Vec<Case> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "atk" => {
                assert_eq!(f.len(), 7, "atk 列数非法: {line}");
                let name = f[1];
                let case = cases
                    .iter_mut()
                    .find(|c| c.name == name)
                    .unwrap_or_else(|| panic!("atk 行先于 case 行: {line}"));
                case.attacks.push(Atk {
                    power: f[3].parse().expect("power"),
                    damage: f[4].parse().expect("damage"),
                    hp_after: f[5].parse().expect("hp_after"),
                    hit: f[6] == "1",
                });
            }
            "case" => {
                assert_eq!(f.len(), 8, "case 列数非法: {line}");
                cases.push(Case {
                    name: f[1].to_string(),
                    dc: f[2].parse().expect("dc"),
                    hit_point: f[3].parse().expect("hit_point"),
                    ac: f[4].parse().expect("ac"),
                    speed_point: f[5].parse().expect("speed_point"),
                    power_only: f[6] == "1",
                    draws: f[7].parse().expect("draws"),
                    attacks: Vec::new(),
                });
            }
            "draw" => {}
            other => panic!("未知行类型 {other}: {line}"),
        }
    }
    cases
}

fn mk_attacker(dc: i32, hit_point: u8) -> Entity {
    let mut a = Entity::new(101, "attacker", 0, 0);
    a.race = ACTOR_RACE_PLAY;
    a.dc = dc;
    a.hit_point = hit_point;
    a.luck = 0;
    a.power_rate = 100;
    a
}

fn mk_target(ac: i32, speed_point: u8) -> Entity {
    let mut t = Entity::new(900001, "monster", 0, 0);
    t.race = ACTOR_RACE_MONSTER;
    t.ac = ac;
    t.hp = 60000;
    t.max_hp = 60000;
    t.hit_point = 10;
    t.speed_point = speed_point;
    t
}

#[test]
fn attack_dir_matches_oracle_vectors() {
    let config = CombatConfig::default();
    let cases = load();
    assert_eq!(cases.len(), 4, "真身向量 4 例，实得 {}", cases.len());
    let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
    for need in [
        "no_target_power_only",
        "target_hit_ac0",
        "target_hit_ac2",
        "target_miss",
    ] {
        assert!(names.contains(&need), "真身向量缺少用例 {need}");
    }

    for case in &cases {
        let player = AttackPowerMode::Player {
            luck: 0,
            power_rate: 100,
            bo_power_item: false,
            power_item: 0,
        };
        let mut rng = RandomNumber::with_seed(SEED); // 探针每 case 重新注种子
        if case.power_only {
            for (i, want) in case.attacks.iter().enumerate() {
                let got = swing_power(&mut rng, case.dc, player);
                assert_eq!(
                    got, want.power,
                    "[{}#{}] 无目标挥砍攻击力与 oracle 不一致",
                    case.name, i
                );
            }
            continue;
        }
        let mut attacker = mk_attacker(case.dc, case.hit_point);
        let mut target = mk_target(case.ac, case.speed_point);
        for (i, want) in case.attacks.iter().enumerate() {
            let got = attack_dir(&mut rng, &mut attacker, &mut target, None, &config);
            assert_eq!(
                (
                    i32::from(got.damage),
                    got.target_hp_after,
                    i32::from(got.damage) > 0
                ),
                (want.damage, want.hp_after, want.hit),
                "[{}#{}] AttackDir 逐击结算与 oracle 不一致（取数流错位？）",
                case.name,
                i
            );
        }

        // 牙齿：`attack_success` 的 `Random(5)` 每击一次（`:225`）——命中/躲闪都有。
        // 用 `CountingRng` 之外的最简口径：期望取数条数 = 每击 (2 攻击力 + 命中门 + 护甲 1~2 + 1)。
        let per_attack = case.draws / case.attacks.len();
        let expected = if case.name == "target_miss" {
            4
        } else if case.name == "target_hit_ac2" {
            6
        } else {
            5
        };
        assert_eq!(
            per_attack, expected,
            "[{}] 每击取数条数与 oracle 不一致（少一次 AttackSuccess 的 Random(5) 会少 1）",
            case.name
        );
    }
}

/// 改坏必红：把 `attack_dir` 末尾的 `AttackSuccess` 那一抽去掉（旧 `attack_with` 口径）
/// ⇒ 第 2 击起 RNG 流整体前移、结算与 oracle 不符。
#[test]
fn redcheck_dropping_attack_success_shifts_sequence() {
    let config = CombatConfig::default();
    let cases = load();
    let case = cases
        .iter()
        .find(|c| c.name == "target_hit_ac0")
        .expect("缺 target_hit_ac0");

    let mut rng = RandomNumber::with_seed(SEED);
    let mut attacker = mk_attacker(case.dc, case.hit_point);
    let mut target = mk_target(case.ac, case.speed_point);
    let player = AttackPowerMode::Player {
        luck: 0,
        power_rate: 100,
        bo_power_item: false,
        power_item: 0,
    };
    let first = attack_with(&mut rng, &mut attacker, &mut target, None, &config, player);
    assert_eq!(
        i32::from(first.damage),
        case.attacks[0].damage,
        "第 1 击在 AttackSuccess 之前 ⇒ 与 oracle 相同（前提）"
    );
    let second = attack_with(&mut rng, &mut attacker, &mut target, None, &config, player);
    assert_ne!(
        i32::from(second.damage),
        case.attacks[1].damage,
        "红检失效：去掉 AttackSuccess 的 Random(5) 后第 2 击仍与 oracle 相同 ⇒ 本条没有牙齿"
    );
}
