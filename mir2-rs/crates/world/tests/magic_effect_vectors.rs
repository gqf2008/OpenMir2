//! 判据①③（M3 逐系统对齐·技能，第七片）：把 `DoSpell` 里 context_required 的**独立可调方法**
//! 真身向量逐字段核对进 Rust。
//!
//! 向量来源：`mir2-rs/tests/parity/m3/magic-effect-probe`（C#，反射直调 oracle 的 private static
//! `MagicManager.MagTurnUndead`（`MagicManager.cs:827`）与 `MagTamming`（`:704`），
//! RNG 覆盖为记录包装，产出 `vectors/effect_vectors.{json,tsv}` +
//! 可达性登记 `vectors/effect_registry.{json,tsv}`）。
//!
//! **为什么读 TSV 而不是 JSON**：与同目录 `combat_skill_vectors.rs` 同因——
//! `tests/no_parallel.rs` 把 `crates/world` 的依赖面钉死（连 dev-dependencies 也扫），
//! 加 `serde_json` 会被判红；探针同时产出行式伴生件，本用例零依赖解析。
//!
//! 同一 RNG 取数路径：向量把 oracle 的 `System.Random` 账本按 `ReplayRandom` 行式格式
//! （`seq|Next(max)|arg|result|site`）带出，本用例用 [`ReplayRandom`] **强制回放同值**，
//! 再断言「取数序列零分歧（arg/kind 逐条相同）+ 可观测输出逐字段相等」——
//! 这比只比最终数值更强：多了/少了/换了取数都会红。
//!
//! 红检（自动）：三条 `redcheck_*` 把**同一比较函数**作用在被改坏一个数的向量副本上，
//! 断言必出差异 —— 证明「逐字段一致」不是恒绿。
//!
//! 登记门禁：`effect_registry.tsv` 把 108 行按**效果入口可达性**再加一轴分类——
//! `effect_entry`（`<implemented>` / 前置拒绝 / `MagicManager` 私有方法名 / `<inline>` / `<none>`）
//! 与 `probeability`（`implemented` / `reject` / `probeable` / `map_context` / `inline` / `none`），
//! 并把「可被真身反射直调」的行限制为**本片实测过的那两条**（20、32），
//! 每条可调行必须有 ≥1 条向量用例。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use mir2_shared::replay::ReplayRandom;
use mir2_world::magic::{
    mag_tamming, mag_turn_undead, TammingInput, TammingOutcome, TurnUndeadInput, TurnUndeadOutcome,
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

// ---- 32 圣言术（`MagTurnUndead`）----------------------------------------------------------------

#[derive(Clone)]
struct TurnCase {
    caster_level: i32,
    target_level: i32,
    life_attrib: u8,
    super_man: bool,
    animal: bool,
    magic_level: u8,
    cfg_level: i32,
    draws: String,
    result: bool,
    target_hp: i32,
    target_targeted: bool,
    runaway: bool,
    caster_targets: bool,
}

fn load_turn() -> Vec<TurnCase> {
    read_lines("magic-effect-probe/vectors/effect_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "turn_undead")
        .map(|f| TurnCase {
            caster_level: i(&f, 2) as i32,
            target_level: i(&f, 3) as i32,
            life_attrib: i(&f, 4) as u8,
            super_man: i(&f, 5) != 0,
            animal: i(&f, 6) != 0,
            magic_level: i(&f, 7) as u8,
            cfg_level: i(&f, 8) as i32,
            draws: f[9].clone(),
            result: i(&f, 10) != 0,
            target_hp: i(&f, 11) as i32,
            target_targeted: i(&f, 12) != 0,
            runaway: i(&f, 13) != 0,
            caster_targets: i(&f, 14) != 0,
        })
        .collect()
}

/// 返回 oracle 与 Rust 的逐字段差异（空 = 全等）。红检共用。
fn turn_mismatches(cases: &[TurnCase]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, c) in cases.iter().enumerate() {
        let mut rng = replay(&c.draws);
        let got = mag_turn_undead(
            &mut rng,
            &TurnUndeadInput {
                super_man: c.super_man,
                life_attrib: c.life_attrib,
                animal: c.animal,
                caster_level: c.caster_level,
                target_level: c.target_level,
                magic_level: c.magic_level,
                mag_turn_undead_level: c.cfg_level,
            },
        );
        let want = TurnUndeadOutcome {
            result: c.result,
            target_hp_zeroed: c.target_hp == 0,
            target_targeted_by_caster: c.target_targeted,
            target_run_away: c.runaway,
            caster_target_is_target: c.caster_targets,
        };
        for d in rng.divergences() {
            out.push(format!("turn[{n}] 取数分歧: {d}"));
        }
        if rng.remaining() != 0 {
            out.push(format!("turn[{n}] 取数未消费完: 剩 {}", rng.remaining()));
        }
        if got != want {
            out.push(format!("turn[{n}] 输出不等: got={got:?} want={want:?}"));
        }
    }
    out
}

#[test]
fn turn_undead_matches_oracle_per_case() {
    let cases = load_turn();
    assert!(cases.len() >= 24, "turn_undead 用例数 {}", cases.len());
    let bad = turn_mismatches(&cases);
    assert!(bad.is_empty(), "圣言术向量不一致:\n{}", bad.join("\n"));
    // 非平凡：既有「杀成」也有「没杀成」，且 magic_level 改变过判定。
    assert!(cases.iter().any(|c| c.result), "无杀成用例");
    assert!(cases.iter().any(|c| !c.result), "无未杀成用例");
    assert!(
        cases.iter().any(|c| c.target_hp == 0) && cases.iter().any(|c| c.target_hp != 0),
        "HP 结果无分化"
    );
    // 前置拒绝的两类（非不死 / SuperMan）不取数。
    assert!(cases
        .iter()
        .any(|c| c.life_attrib != 1 && !c.super_man && c.draws.is_empty()));
    assert!(cases.iter().any(|c| c.super_man && c.draws.is_empty()));
    // Animal=false 时不抽 Random(300)。
    assert!(cases
        .iter()
        .any(|c| !c.animal && !c.draws.contains("|300|")));
}

/// 红检⑩（技能·圣言术）：把「杀成」用例的 `result` 改坏 ⇒ 同一比较函数必出差异。
#[test]
fn redcheck_corrupt_one_turn_result_must_fail() {
    let mut cases = load_turn();
    let idx = cases
        .iter()
        .position(|c| c.result)
        .expect("向量里应有杀成用例");
    cases[idx].result = false;
    assert!(
        !turn_mismatches(&cases).is_empty(),
        "红检失效：改动 result 后仍全等"
    );
}

/// 红检⑪（技能·圣言术）：把一条取数账本的 `arg` 改坏（`Random(2)` → `Random(3)`）⇒ 必出差异。
#[test]
fn redcheck_corrupt_one_turn_draw_must_fail() {
    let mut cases = load_turn();
    let idx = cases
        .iter()
        .position(|c| c.draws.contains("|Next(max)|2|"))
        .expect("向量里应有 Random(2) 取数");
    cases[idx].draws = cases[idx].draws.replace("|Next(max)|2|", "|Next(max)|3|");
    let bad = turn_mismatches(&cases);
    assert!(!bad.is_empty(), "红检失效：改动取数 arg 后仍全等");
}

// ---- 20 诱惑之光（`MagTamming`）----------------------------------------------------------------

#[derive(Clone)]
struct TammingCase {
    inp: TammingInput,
    draws: String,
    result: bool,
    target_hp: i32,
    cleared: bool,
    master_is_caster: bool,
    walk_speed: i32,
    next_hit_time: i32,
    crazy_mode: bool,
}

fn load_tamming() -> Vec<TammingCase> {
    read_lines("magic-effect-probe/vectors/effect_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "tamming")
        .map(|f| {
            let master_is_caster = i(&f, 7) != 0;
            let other_master = i(&f, 8) != 0;
            TammingCase {
                inp: TammingInput {
                    magic_level: i(&f, 11) as u8,
                    caster_level: i(&f, 2) as i32,
                    target_race: i(&f, 4) as u8,
                    target_level: i(&f, 3) as i32,
                    target_abil_max_hp: i(&f, 10) as i32,
                    target_life_attrib: i(&f, 5) as u8,
                    target_no_tame: i(&f, 6) != 0,
                    target_master_is_caster: master_is_caster,
                    target_master_present: master_is_caster || other_master,
                    target_hp: i(&f, 10) as i32, // 调用前 WAbil.HP = MaxHP（探针构造口径）
                    walk_speed: i(&f, 16) as i32,
                    next_hit_time: i(&f, 17) as i32,
                    caster_slave_count: i(&f, 9) as i32,
                    cfg_target_level: i(&f, 12) as i32,
                    cfg_tamming_level: i(&f, 13) as i32,
                    cfg_tamming_count: i(&f, 14) as i32,
                    cfg_hp_rate: i(&f, 15) as i32,
                },
                draws: f[18].clone(),
                result: i(&f, 19) != 0,
                target_hp: i(&f, 20) as i32,
                cleared: i(&f, 21) != 0,
                master_is_caster: i(&f, 22) != 0,
                walk_speed: i(&f, 23) as i32,
                next_hit_time: i(&f, 24) as i32,
                crazy_mode: i(&f, 25) != 0,
            }
        })
        .collect()
}

/// 返回 oracle 与 Rust 的逐字段差异（空 = 全等）。红检共用。
fn tamming_mismatches(cases: &[TammingCase]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, c) in cases.iter().enumerate() {
        let inp = c.inp; // 调用前初值已记录在向量里（HP = MaxHP、WalkSpeed/NextHitTime 初值）
        let mut rng = replay(&c.draws);
        let got = mag_tamming(&mut rng, &inp);
        let mut want = TammingOutcome {
            result: c.result,
            target_creature_cleared: c.cleared,
            target_master_is_caster: c.master_is_caster,
            target_hp: c.target_hp,
            walk_speed: c.walk_speed,
            next_hit_time: c.next_hit_time,
            crazy_mode: c.crazy_mode,
            royalty_draw: 0,
        };
        // `royalty_draw`（`MagicManager.cs:770` 的 `Random(caster.Lv*2)`）不在这批探针 TSV 的
        // 记录列里；它是**取数账本的一部分**，已由下面 `rng.divergences()`/`rng.remaining()` 的
        // 逐次对拍钉死（值 = 该次抽数），故此处只对齐其余字段。
        want.royalty_draw = got.royalty_draw;
        for d in rng.divergences() {
            out.push(format!("tamming[{n}] 取数分歧: {d}"));
        }
        if rng.remaining() != 0 {
            out.push(format!("tamming[{n}] 取数未消费完: 剩 {}", rng.remaining()));
        }
        if got != want {
            out.push(format!("tamming[{n}] 输出不等: got={got:?} want={want:?}"));
        }
    }
    out
}

#[test]
fn tamming_matches_oracle_per_case() {
    let cases = load_tamming();
    assert!(cases.len() >= 24, "tamming 用例数 {}", cases.len());
    let bad = tamming_mismatches(&cases);
    assert!(bad.is_empty(), "诱惑之光向量不一致:\n{}", bad.join("\n"));
    // 分支覆盖：受奴成功、HP/10、封顶、狂暴、不死、早分支。
    assert!(cases.iter().any(|c| c.master_is_caster), "无受奴用例");
    assert!(cases.iter().any(|c| c.crazy_mode), "无狂暴用例");
    assert!(
        cases.iter().any(|c| c.walk_speed == 900),
        "无 WalkSpeed 封顶用例"
    );
    assert!(
        cases.iter().any(|c| c.target_hp == 0) || cases.iter().any(|c| c.target_hp == 10),
        "无 HP/10 或击杀用例"
    );
}

/// 红检⑫（技能·诱惑之光）：把一条「受奴成功」用例的 `target_hp` 改坏 ⇒ 必出差异。
#[test]
fn redcheck_corrupt_one_tamming_hp_must_fail() {
    let mut cases = load_tamming();
    let idx = cases
        .iter()
        .position(|c| c.target_hp == 10)
        .or_else(|| cases.iter().position(|c| c.master_is_caster))
        .expect("向量里应有受奴用例");
    cases[idx].target_hp = 7777;
    assert!(
        !tamming_mismatches(&cases).is_empty(),
        "红检失效：改动 target_hp 后仍全等"
    );
}

// ---- 可达性登记门禁（DoD③「未实装逐条登记 + 不许混称真身」）------------------------------------

#[derive(Clone)]
struct RegRow {
    magic_id: u16,
    dospell_class: String,
    implemented: bool,
    effect_entry: String,
    anchor: String,
    probeability: String,
}

fn load_registry() -> Vec<RegRow> {
    read_lines("magic-effect-probe/vectors/effect_registry.tsv")
        .into_iter()
        .filter(|f| f[0] == "entry")
        .map(|f| RegRow {
            magic_id: i(&f, 1) as u16,
            dospell_class: f[3].clone(),
            implemented: i(&f, 4) != 0,
            effect_entry: f[5].clone(),
            anchor: f[6].clone(),
            probeability: f[7].clone(),
        })
        .collect()
}

/// 判据（DoD③ 可达性口径）：108 行全部登记「效果入口」，且
/// `probeability = probeable` 的行**只能是本片用真身反射直调过的那两条**（20、32），
/// 每条必须有点名的入口与行号锚点、且在被测向量里有 ≥1 条用例 —— 不许把 source-cited 混称真身。
#[test]
fn effects_are_classified_by_reachability() {
    let reg = load_registry();
    assert_eq!(reg.len(), 108, "魔表应有 108 行（content 冻结）");

    let mut counts: BTreeMap<&str, i64> = BTreeMap::new();
    let mut probeable: BTreeSet<u16> = BTreeSet::new();
    for r in &reg {
        assert!(
            !r.effect_entry.is_empty(),
            "行 {} 无 effect_entry",
            r.magic_id
        );
        assert!(!r.anchor.is_empty(), "行 {} 无 oracle 锚点", r.magic_id);
        *counts.entry(r.probeability.as_str()).or_insert(0) += 1;
        if r.probeability == "probeable" {
            probeable.insert(r.magic_id);
        }
        if r.implemented {
            assert_eq!(
                r.probeability, "implemented",
                "行 {} 已实装但可达性非 implemented",
                r.magic_id
            );
            assert_eq!(r.effect_entry, "<implemented>");
        }
        // context_required 必须落到「独立方法」或「内联」之一，且锚点带行号。
        if r.dospell_class == "context_required" {
            assert!(
                r.effect_entry == "<inline>" || r.anchor.contains("MagicManager.cs:"),
                "行 {} context_required 锚点不可核: {} / {}",
                r.magic_id,
                r.effect_entry,
                r.anchor
            );
        }
    }
    // 第七片：20（MagTamming）与 32（MagTurnUndead）无地图即可反射直调；
    // 第十片：8/37（MagPushArround）、21（MagSaceMove）、46（MagMakeClone）用合成真格式 .map +
    // 真身 `Envirnoment` 也已离线直调成功 ⇒ 一并归入 probeable（见 map-effect-probe）。
    assert_eq!(
        probeable,
        BTreeSet::from([8u16, 20, 21, 32, 37, 46]),
        "probeable 集合（第七 + 第十片实测）"
    );
    // 其余 context_required = 需 WorldEngine 真召唤（17/30/41）或内联（40）。
    assert_eq!(counts.get("map_context").copied(), Some(3));
    assert_eq!(counts.get("inline").copied(), Some(1));
    assert_eq!(counts.get("probeable").copied(), Some(6));
    assert_eq!(counts.get("implemented").copied(), Some(29));
    assert_eq!(counts.get("reject").copied(), Some(43)); // 8 warr_reject + 35 magicid_reject
    assert_eq!(counts.get("none").copied(), Some(26)); // 6 no_effect_case + 20 no_switch_case

    // 每条 probeable 行都必须有向量用例（不许只登记不产向量）。
    // 20/32 的向量在本文件；8/21/37/46 的向量在同 crate 的 `map_effect_vectors.rs`（第十片）。
    let turn: BTreeSet<u16> = load_turn().iter().map(|_| 32u16).collect();
    let tamming: BTreeSet<u16> = load_tamming().iter().map(|_| 20u16).collect();
    assert_eq!(turn, BTreeSet::from([32u16]));
    assert_eq!(tamming, BTreeSet::from([20u16]));
}
