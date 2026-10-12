//! 判据②③（M3 逐系统对齐·战斗/技能，第八片）：`DoSpell` 之外的战士近战技能路径逐字段对拍。
//!
//! 背景：第五片把真魔表里 `IsWarrSkill` 命中的 8 行登记为 `warr_reject`（`DoSpell` 前置即
//! `return false`），reason 写「承接 = 战士技能走 ClientHit/Attack 路径」——那条路径此前没有向量。
//! 本片用 `attack-hit-probe` 把该路径上**可独立反射直调**的两个 oracle 入口调起来：
//! `PlayObject.RecalcHitSpeed`（`src/M2Server/Player/PlayObject.cs:3659-3713`）⇒ [`recalc_hit_speed`]：
//! 基本剑术/精神力战法/攻杀剑术加 `HitPoint`、攻杀剑术设 `HitPlus`、烈火剑法设 `HitDouble`
//! （含攻杀剑术的 `RandomByte` **取数账本**）；`PlayObject.GetAttackPowerHit`
//! （`src/M2Server/Player/PlayObject.Attack.cs:579-625`）⇒ [`get_attack_power_hit`]：攻杀剑术
//! `+= HitPlus`；烈火剑法/狂风斩 `nPower + Round(nPower/100*HitDouble*10)`；目标为 null 时
//! 两个 Hit 分支只清标志、不改威力、不置 `canHit`。
//!
//! **为什么读 TSV 而不是 JSON**：与同目录 `combat_skill_vectors.rs` / `magic_effect_vectors.rs` 同因——
//! `tests/no_parallel.rs` 把 `crates/world` 的依赖面钉死（连 dev-dependencies 也扫），加 `serde_json` 会判红。
//!
//! 同一 RNG 取数路径：向量把 oracle 的 `System.Random` 账本按 `ReplayRandom` 行式格式
//! (`seq|Next(max)|arg|result|site`) 带出，本用例强制回放同值，再断言「取数序列零分歧 +
//! 消费到底 + 可观测输出逐字段相等」。
//!
//! 红检（自动）：两条 `redcheck_*` 把**同一比较函数**作用在被改坏一个数的向量副本上，断言必出差异。
//!
//! 登记门禁：`attack_reach_registry` 把 `warr_reject` 8 行按 `combat_entry`/`probeability` 收口——
//! `probeable` 恰 = {3,4,7,26}，且这些行都**必须**有向量用例；`map_context` 行不得混称真身。

use std::collections::BTreeSet;
use std::path::PathBuf;

use mir2_shared::replay::ReplayRandom;
use mir2_world::{
    get_attack_power_hit, recalc_hit_speed, AttackHitState, HitSpeedInput, HitSpeedOutcome,
};

fn repo_parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/m3")
}

fn read_lines(path: &str) -> Vec<Vec<String>> {
    let full = repo_parity_dir().join(path);
    let text = std::fs::read_to_string(&full)
        .unwrap_or_else(|e| panic!("读不到向量 {}: {e}", full.display()));
    // 注意：**不能** `trim_end()`——行尾的空字段（如空 `draws`/空 `magics`）会被吃掉，列数错位。
    text.lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.is_empty())
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect()
}

fn i(fields: &[String], idx: usize) -> i64 {
    fields[idx]
        .parse()
        .unwrap_or_else(|e| panic!("字段 {idx} 不是整数: {:?} ({e})", fields.get(idx)))
}

/// 把向量里的取数账本（`seq|kind|arg|result|site` 以 `;` 连接）转成 [`ReplayRandom`]。
fn replay(draws: &str) -> ReplayRandom {
    let mut tsv = String::new();
    if !draws.is_empty() {
        for entry in draws.split(';') {
            tsv.push_str(&entry.replace('|', "\t"));
            tsv.push('\n');
        }
    }
    ReplayRandom::from_tsv(&tsv).unwrap_or_else(|e| panic!("取数账本非法: {e} ({draws})"))
}

// ---- `PlayObject.RecalcHitSpeed` 向量 -------------------------------------------------------------

#[derive(Clone)]
struct RecalcRow {
    name: String,
    job: u8,
    bonus_hit: u8,
    bonus_speed: i32,
    tick_hit: u8,
    tick_speed: i32,
    init_hit_point: u8,
    init_speed_point: u8,
    magics: Vec<(u16, u8)>,
    hit_point: u8,
    speed_point: u8,
    hit_plus: u16,
    hit_double: u16,
    attack_skill_count: u8,
    attack_skill_point_count: u8,
    draws: String,
}

fn parse_magics(spec: &str) -> Vec<(u16, u8)> {
    if spec.is_empty() {
        return Vec::new();
    }
    spec.split(';')
        .map(|part| {
            let mut it = part.split(':');
            let id = it.next().unwrap().parse().expect("magic id");
            let lv = it.next().unwrap().parse().expect("magic level");
            (id, lv)
        })
        .collect()
}

fn load_recalc() -> Vec<RecalcRow> {
    read_lines("attack-hit-probe/vectors/attack_hit_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "recalc")
        .map(|f| RecalcRow {
            name: f[1].clone(),
            job: i(&f, 2) as u8,
            bonus_hit: i(&f, 3) as u8,
            bonus_speed: i(&f, 4) as i32,
            tick_hit: i(&f, 5) as u8,
            tick_speed: i(&f, 6) as i32,
            init_hit_point: i(&f, 7) as u8,
            init_speed_point: i(&f, 8) as u8,
            magics: parse_magics(&f[9]),
            hit_point: i(&f, 10) as u8,
            speed_point: i(&f, 11) as u8,
            hit_plus: i(&f, 12) as u16,
            hit_double: i(&f, 13) as u16,
            attack_skill_count: i(&f, 14) as u8,
            attack_skill_point_count: i(&f, 15) as u8,
            draws: f[16].clone(),
        })
        .collect()
}

fn recalc_run(row: &RecalcRow) -> (HitSpeedOutcome, ReplayRandom) {
    let mut rng = replay(&row.draws);
    let got = recalc_hit_speed(
        &mut rng,
        &HitSpeedInput {
            job: row.job,
            bonus_hit: row.bonus_hit,
            bonus_speed: row.bonus_speed,
            tick_hit: row.tick_hit,
            tick_speed: row.tick_speed,
            hit_point: row.init_hit_point,
            speed_point: row.init_speed_point,
            magics: row.magics.clone(),
        },
    );
    (got, rng)
}

/// 返回 oracle 与 Rust 的逐字段差异（空 = 全等）。红检共用。
fn recalc_mismatches(rows: &[RecalcRow]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let (got, rng) = recalc_run(row);
        let want = HitSpeedOutcome {
            hit_point: row.hit_point,
            speed_point: row.speed_point,
            hit_plus: row.hit_plus,
            hit_double: row.hit_double,
            attack_skill_count: row.attack_skill_count,
            attack_skill_point_count: row.attack_skill_point_count,
        };
        for d in rng.divergences() {
            out.push(format!("recalc[{n}] {} 取数分歧: {d}", row.name));
        }
        if rng.remaining() != 0 {
            out.push(format!(
                "recalc[{n}] {} 取数未消费完: 剩 {}",
                row.name,
                rng.remaining()
            ));
        }
        if got != want {
            out.push(format!(
                "recalc[{n}] {} 输出不等: got={got:?} want={want:?}",
                row.name
            ));
        }
    }
    out
}

// ---- `PlayObject.GetAttackPowerHit` 向量 ----------------------------------------------------------

#[derive(Clone)]
struct AphRow {
    name: String,
    hit_mode: i16,
    power: i32,
    has_target: bool,
    hit_plus: u16,
    hit_double: u16,
    power_hit: bool,
    fire_hit: bool,
    twin_hit: bool,
    out_power: i32,
    can_hit: bool,
    post_power_hit: bool,
    post_fire_hit: bool,
    post_twin_hit: bool,
}

fn load_aph() -> Vec<AphRow> {
    read_lines("attack-hit-probe/vectors/attack_hit_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "aph")
        .map(|f| AphRow {
            name: f[1].clone(),
            hit_mode: i(&f, 2) as i16,
            power: i(&f, 3) as i32,
            has_target: i(&f, 4) != 0,
            hit_plus: i(&f, 5) as u16,
            hit_double: i(&f, 6) as u16,
            power_hit: i(&f, 7) != 0,
            fire_hit: i(&f, 8) != 0,
            twin_hit: i(&f, 9) != 0,
            out_power: i(&f, 10) as i32,
            can_hit: i(&f, 11) != 0,
            post_power_hit: i(&f, 12) != 0,
            post_fire_hit: i(&f, 13) != 0,
            post_twin_hit: i(&f, 14) != 0,
        })
        .collect()
}

fn aph_mismatches(rows: &[AphRow]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let mut state = AttackHitState {
            power_hit: row.power_hit,
            fire_hit_skill: row.fire_hit,
            twin_hit_skill: row.twin_hit,
            hit_plus: row.hit_plus,
            hit_double: row.hit_double,
        };
        let (power, can_hit) =
            get_attack_power_hit(row.hit_mode, row.power, row.has_target, &mut state);
        if power != row.out_power || can_hit != row.can_hit {
            out.push(format!(
                "aph[{n}] {} 输出不等: got=({power},{can_hit}) want=({},{})",
                row.name, row.out_power, row.can_hit
            ));
        }
        if state.power_hit != row.post_power_hit
            || state.fire_hit_skill != row.post_fire_hit
            || state.twin_hit_skill != row.post_twin_hit
        {
            out.push(format!(
                "aph[{n}] {} 标志不等: got=({},{},{}) want=({},{},{})",
                row.name,
                state.power_hit,
                state.fire_hit_skill,
                state.twin_hit_skill,
                row.post_power_hit,
                row.post_fire_hit,
                row.post_twin_hit
            ));
        }
    }
    out
}

// ---- 用例 ----------------------------------------------------------------------------------------

#[test]
fn recalc_hit_speed_matches_oracle_per_case() {
    let rows = load_recalc();
    assert_eq!(rows.len(), 10, "向量行数（探针改口径要同步本断言）");
    let bad = recalc_mismatches(&rows);
    assert!(bad.is_empty(), "{}", bad.join("\n"));

    // 非平凡：三条技能轴都真的被观测到，且攻杀剑术的 `RandomByte` 取数在账本里。
    assert!(
        rows.iter().any(|r| r.hit_plus > 0),
        "无攻杀剑术 HitPlus 样本"
    );
    assert!(
        rows.iter().any(|r| r.hit_double > 0),
        "无烈火剑法 HitDouble 样本"
    );
    assert!(
        rows.iter()
            .any(|r| r.attack_skill_count > 0 && !r.draws.is_empty()),
        "无攻杀剑术取数样本"
    );
    let yedo = rows.iter().find(|r| r.name == "warr_yedo_lv3").unwrap();
    let (got, _) = recalc_run(yedo);
    assert_eq!(
        got.attack_skill_point_count, 1,
        "攻杀剑术 RandomByte(4)=1（种子钉住）"
    );
    // 道士分支不改 HitPoint（oracle 只写 SpeedPoint）——防止把 HitPoint 也重算。
    let taos = rows.iter().find(|r| r.name == "taos_init_hp_kept").unwrap();
    let (got, _) = recalc_run(taos);
    assert_eq!(got.hit_point, 7, "道士初始 HitPoint 应保持");
    assert_eq!(got.speed_point, 20, "道士 SpeedPoint = 15 + 60/30 + 3");
}

#[test]
fn attack_power_hit_matches_oracle_per_case() {
    let rows = load_aph();
    assert_eq!(rows.len(), 16, "向量行数（探针改口径要同步本断言）");
    let bad = aph_mismatches(&rows);
    assert!(bad.is_empty(), "{}", bad.join("\n"));

    // 非平凡：三种模式都有真的倍率生效，且 null 目标分支确实不同。
    let find = |name: &str| rows.iter().find(|r| r.name == name).unwrap();
    assert_eq!(
        find("mode3_powerhit_t").out_power,
        34,
        "攻杀剑术 25+HitPlus(9)"
    );
    assert_eq!(
        find("mode7_fire_high_t").out_power,
        410,
        "烈火剑法 250+Round(250/100*8*10)"
    );
    assert_eq!(
        find("mode9_twin_high_t").out_power,
        693,
        "狂风斩 333+Round(333/100*12*10)"
    );
    assert!(!find("mode3_nopowerhit_t").can_hit, "未充能时不得 canHit");
    // null 目标：mode 7/9 只清标志、不改威力、不置 canHit（oracle 的另一个分支）。
    let null7 = find("mode7_fire_high_n");
    assert_eq!(null7.out_power, 250, "null 目标 mode7 不改威力");
    assert!(!null7.can_hit, "null 目标 mode7 不置 canHit");
    assert!(!null7.post_fire_hit, "null 目标 mode7 仍清标志");
    let null9 = find("mode9_twin_high_n");
    assert_eq!(null9.out_power, 333, "null 目标 mode9 不改威力");
    assert!(!null9.post_twin_hit, "null 目标 mode9 仍清标志");
    // mode0 不碰任何标志。
    let m0 = find("mode0_default_t");
    assert_eq!(m0.out_power, 77);
    assert!(!m0.can_hit);
    assert!(m0.post_power_hit && m0.post_fire_hit && m0.post_twin_hit);
}

#[test]
fn warr_reject_rows_are_classified_by_combat_entry() {
    // ① `warr_reject` 集合以已入库的 `skill_vectors.tsv` 为准（第五片口径）。
    let reg = read_lines("skill-probe/vectors/skill_vectors.tsv");
    let warr: BTreeSet<i64> = reg
        .iter()
        .filter(|f| f[0] == "reg" && f[8] == "warr_reject")
        .map(|f| i(f, 1))
        .collect();
    assert_eq!(
        warr,
        BTreeSet::from([3, 4, 7, 12, 25, 26, 27, 34]),
        "warr_reject 行（IsWarrSkill∩魔表）"
    );

    // ② 本片登记必须覆盖同一集合。
    let reach = read_lines("attack-hit-probe/vectors/attack_hit_vectors.tsv");
    let mut reach_ids = BTreeSet::new();
    let mut probeable = BTreeSet::new();
    let mut map_context = BTreeSet::new();
    for f in reach.iter().filter(|f| f[0] == "reach") {
        let id = i(f, 1);
        assert!(reach_ids.insert(id), "reach 行重复: {id}");
        match f[4].as_str() {
            "probeable" => {
                probeable.insert(id);
            }
            "map_context" => {
                map_context.insert(id);
            }
            other => panic!("未登记的可达性 {other}（行 {id}）"),
        }
    }
    assert_eq!(reach_ids, warr, "登记集合必须与 warr_reject 一致");

    // ③ `probeable` 恰 = {3,4,7,26}，且每行都必须在向量里有真身用例。
    assert_eq!(
        probeable,
        BTreeSet::from([3, 4, 7, 26]),
        "本片实测可直调集合"
    );
    assert_eq!(map_context, BTreeSet::from([12, 25, 27, 34]));

    let recalc = load_recalc();
    let recalc_magics: BTreeSet<u16> = recalc
        .iter()
        .flat_map(|r| r.magics.iter().map(|(id, _)| *id))
        .collect();
    let aph = load_aph();
    let aph_modes: BTreeSet<i16> = aph.iter().map(|r| r.hit_mode).collect();
    // 3/4/26 只在 RecalcHitSpeed 里被观测；7 在两处（RecalcHitSpeed + GetAttackPowerHit mode3）。
    assert_eq!(
        recalc_magics,
        BTreeSet::from([3, 4, 7, 26]),
        "recalc 向量覆盖的魔号"
    );
    assert!(aph_modes.contains(&3) && aph_modes.contains(&7) && aph_modes.contains(&9));
    for id in &map_context {
        assert!(
            !recalc_magics.contains(&(*id as u16)),
            "map_context 行 {id} 不得混称有真身向量"
        );
    }
}

// ---- 红检（自动） --------------------------------------------------------------------------------

#[test]
fn redcheck_corrupt_one_recalc_hit_point_must_fail() {
    let mut rows = load_recalc();
    assert!(recalc_mismatches(&rows).is_empty(), "前置：原向量必须全等");
    rows[0].hit_point = rows[0].hit_point.wrapping_add(1);
    let bad = recalc_mismatches(&rows);
    assert!(
        bad.iter().any(|m| m.contains("输出不等")),
        "改坏 recalc hit_point 后必红: {bad:?}"
    );
}

#[test]
fn redcheck_corrupt_one_aph_power_must_fail() {
    let mut rows = load_aph();
    assert!(aph_mismatches(&rows).is_empty(), "前置：原向量必须全等");
    let idx = rows
        .iter()
        .position(|r| r.name == "mode7_fire_high_t")
        .unwrap();
    rows[idx].out_power += 1;
    let bad = aph_mismatches(&rows);
    assert!(
        bad.iter().any(|m| m.contains("输出不等")),
        "改坏 aph out_power 后必红: {bad:?}"
    );
}
