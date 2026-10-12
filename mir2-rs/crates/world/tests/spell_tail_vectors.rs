//! 判据②③（M3 逐系统对齐·战斗/技能，第九片）：`DoSpell` 共享尾与技能熟练度/升级逐字段对拍。
//!
//! 背景：`skill-probe` 把 108 行魔表里 `no_effect_case`（6 行：43 `break`；49/51/52/53/54 只置
//! `boTrain`）与 `no_switch_case`（20 行：`DoSpell` switch 无此魔号 case）登记为「效果体为空」。
//! 这两类**唯一的运行时可观测行为**都在共享尾（`src/M2Server/Magic/MagicManager.cs:644-670`）：
//! `boSpellFail` 早退 / `boSpellFire` 发 `RM_MAGICFIRE` / `boTrain` 门内
//! `TrainSkill(…, Random(3)+1)` + `CheckMagicLevelUp` + `RM_MAGIC_LVEXP`。
//!
//! 本片用 `spell-tail-probe` 把尾逻辑调用的**可独立反射直调**的 oracle 入口调起来：
//!   * `PlayObject.TrainSkill`（`PlayObject.cs:4393-4400`）⇒ [`train_skill`]；
//!   * `PlayObject.CheckMagicLevelUp`（`PlayObject.cs:5198-5226`）⇒ [`check_magic_level_up`]；
//!   * `PlayObject.CheckSkillProficiency`（`PlayObject.Attack.cs:200-213`）⇒ [`check_skill_proficiency`]。
//!
//! `DoSpell` 共享尾本身是**内联块** ⇒ 只 source-cited（锚点在探针 `VerifyAnchors`），
//! 由 [`do_spell_tail`] 逐行实现并按门/取数顺序做行为断言，**不冒充真身向量**。
//!
//! **为什么读 TSV 而不是 JSON**：与同目录 `combat_skill_vectors.rs` / `attack_skill_vectors.rs` 同因——
//! `tests/no_parallel.rs` 把 `crates/world` 的依赖面钉死（连 dev-dependencies 也扫），加 `serde_json` 会判红。
//!
//! 同一 RNG 取数路径：攻击路径调用方的 `Random(3)+1` 账本按 `ReplayRandom` 行式格式带出，
//! 本用例强制回放同值（断言取数序列零分歧 + 消费到底 + 抽到的 `n_tran_point` 与向量一致）。
//!
//! 红检（自动）：三条 `redcheck_*` 把**同一比较函数**作用在被改坏一个数的向量副本上，断言必出差异；
//! 另有一条 `reach` 轴的改坏必红。

use std::collections::BTreeSet;
use std::path::PathBuf;

use mir2_shared::replay::ReplayRandom;
use mir2_shared::rng::RandomSource;
use mir2_world::magic::{
    check_magic_level_up, check_skill_proficiency, do_spell_tail, train_skill, SkillTrainState,
};

fn repo_parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/m3")
}

fn read_lines(path: &str) -> Vec<Vec<String>> {
    let full = repo_parity_dir().join(path);
    let text = std::fs::read_to_string(&full)
        .unwrap_or_else(|e| panic!("读不到向量 {}: {e}", full.display()));
    // 注意：**不能** `trim_end()`——行尾的空字段（如空 `msg`/空 `draws`）会被吃掉，列数错位。
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

fn i32a(fields: &[String], start: usize) -> [i32; 4] {
    [
        i(fields, start) as i32,
        i(fields, start + 1) as i32,
        i(fields, start + 2) as i32,
        i(fields, start + 3) as i32,
    ]
}

fn u8a(fields: &[String], start: usize) -> [u8; 4] {
    [
        i(fields, start) as u8,
        i(fields, start + 1) as u8,
        i(fields, start + 2) as u8,
        i(fields, start + 3) as u8,
    ]
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

/// 期望的升级帧内容：oracle 在 `CheckMagicLevelUp` 里先改 `Level`/`TranPoint` 再 enqueue，
/// 调用方补发同理 ⇒ 帧参数恒为**调用后**的值（`RM_MAGIC_LVEXP = 10123`）。
fn expect_msg(magic_id: u16, level: u8, tran_point: i32) -> String {
    format!("10123:{magic_id}:{level}:{tran_point}")
}

// ---- `PlayObject.TrainSkill` 向量 ---------------------------------------------------------------

#[derive(Clone)]
struct TrainRow {
    name: String,
    fast: bool,
    n: i32,
    init_tp: i32,
    out_tp: i32,
}

fn load_train() -> Vec<TrainRow> {
    read_lines("spell-tail-probe/vectors/spell_tail_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "train")
        .map(|f| TrainRow {
            name: f[1].clone(),
            fast: i(&f, 2) != 0,
            n: i(&f, 3) as i32,
            init_tp: i(&f, 4) as i32,
            out_tp: i(&f, 5) as i32,
        })
        .collect()
}

fn train_mismatches(rows: &[TrainRow]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let got = row.init_tp + train_skill(row.fast, row.n);
        if got != row.out_tp {
            out.push(format!(
                "train[{n}] {} 输出不等: got={got} want={}",
                row.name, row.out_tp
            ));
        }
    }
    out
}

// ---- `PlayObject.CheckMagicLevelUp` 向量 --------------------------------------------------------

#[derive(Clone)]
struct ChkLvRow {
    name: String,
    magic_id: u16,
    train_lv: u8,
    max_train: [i32; 4],
    train_level: [u8; 4],
    init_level: u8,
    init_tp: i32,
    out_result: bool,
    out_level: u8,
    out_tp: i32,
    see_heal: bool,
    msg: String,
}

fn load_chklv() -> Vec<ChkLvRow> {
    read_lines("spell-tail-probe/vectors/spell_tail_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "chklv")
        .map(|f| ChkLvRow {
            name: f[1].clone(),
            magic_id: i(&f, 2) as u16,
            train_lv: i(&f, 3) as u8,
            max_train: i32a(&f, 4),
            train_level: u8a(&f, 8),
            init_level: i(&f, 12) as u8,
            init_tp: i(&f, 13) as i32,
            out_result: i(&f, 14) != 0,
            out_level: i(&f, 15) as u8,
            out_tp: i(&f, 16) as i32,
            see_heal: i(&f, 17) != 0,
            msg: f[18].clone(),
        })
        .collect()
}

fn chklv_state(row: &ChkLvRow) -> SkillTrainState {
    SkillTrainState {
        magic_id: row.magic_id,
        level: row.init_level,
        tran_point: row.init_tp,
        train_lv: row.train_lv,
        train_level: row.train_level,
        max_train: row.max_train,
    }
}

fn chklv_mismatches(rows: &[ChkLvRow]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let mut state = chklv_state(row);
        let o = check_magic_level_up(&mut state);
        if o.result != row.out_result {
            out.push(format!(
                "chklv[{n}] {} result 不等: got={} want={}",
                row.name, o.result, row.out_result
            ));
        }
        if state.level != row.out_level || state.tran_point != row.out_tp {
            out.push(format!(
                "chklv[{n}] {} 状态不等: got=(lv {},tp {}) want=(lv {},tp {})",
                row.name, state.level, state.tran_point, row.out_level, row.out_tp
            ));
        }
        if o.see_heal_gauge != row.see_heal {
            out.push(format!(
                "chklv[{n}] {} see_heal 不等: got={} want={}",
                row.name, o.see_heal_gauge, row.see_heal
            ));
        }
        let want_msg = if o.levelup_msg {
            expect_msg(row.magic_id, state.level, state.tran_point)
        } else {
            String::new()
        };
        if row.msg != want_msg {
            out.push(format!(
                "chklv[{n}] {} 帧不等: got={:?} want={:?}",
                row.name, row.msg, want_msg
            ));
        }
    }
    out
}

// ---- `PlayObject.CheckSkillProficiency` 向量 ----------------------------------------------------

#[derive(Clone)]
struct ProfRow {
    name: String,
    fast: bool,
    abil_level: u8,
    magic_id: u16,
    train_lv: u8,
    max_train: [i32; 4],
    train_level: [u8; 4],
    init_level: u8,
    init_tp: i32,
    n_tran_point: i32,
    out_level: u8,
    out_tp: i32,
    see_heal: bool,
    msg: String,
    draws: String,
}

fn load_prof() -> Vec<ProfRow> {
    read_lines("spell-tail-probe/vectors/spell_tail_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "prof")
        .map(|f| ProfRow {
            name: f[1].clone(),
            fast: i(&f, 2) != 0,
            abil_level: i(&f, 3) as u8,
            magic_id: i(&f, 4) as u16,
            train_lv: i(&f, 5) as u8,
            max_train: i32a(&f, 6),
            train_level: u8a(&f, 10),
            init_level: i(&f, 14) as u8,
            init_tp: i(&f, 15) as i32,
            n_tran_point: i(&f, 16) as i32,
            out_tp: i(&f, 17) as i32,
            out_level: i(&f, 18) as u8,
            see_heal: i(&f, 19) != 0,
            msg: f[20].clone(),
            draws: f[21].clone(),
        })
        .collect()
}

fn prof_state(row: &ProfRow) -> SkillTrainState {
    SkillTrainState {
        magic_id: row.magic_id,
        level: row.init_level,
        tran_point: row.init_tp,
        train_lv: row.train_lv,
        train_level: row.train_level,
        max_train: row.max_train,
    }
}

fn prof_mismatches(rows: &[ProfRow]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        // 攻击路径调用方的 `Random(3)+1`（`AttackSuccess`）——本端强制回放同值。
        if !row.draws.is_empty() {
            let mut rng = replay(&row.draws);
            let drawn = rng.random_below(3) + 1;
            if drawn != row.n_tran_point {
                out.push(format!(
                    "prof[{n}] {} 调用方取数不等: got={drawn} want={}",
                    row.name, row.n_tran_point
                ));
            }
            for d in rng.divergences() {
                out.push(format!("prof[{n}] {} 取数分歧: {d}", row.name));
            }
            if rng.remaining() != 0 {
                out.push(format!(
                    "prof[{n}] {} 取数未消费完: 剩 {}",
                    row.name,
                    rng.remaining()
                ));
            }
        }

        let mut state = prof_state(row);
        let o = check_skill_proficiency(row.fast, row.abil_level, row.n_tran_point, &mut state);
        if state.level != row.out_level || state.tran_point != row.out_tp {
            out.push(format!(
                "prof[{n}] {} 状态不等: got=(lv {},tp {}) want=(lv {},tp {})",
                row.name, state.level, state.tran_point, row.out_level, row.out_tp
            ));
        }
        if o.see_heal_gauge != row.see_heal {
            out.push(format!(
                "prof[{n}] {} see_heal 不等: got={} want={}",
                row.name, o.see_heal_gauge, row.see_heal
            ));
        }
        // `CheckSkillProficiency` 只要进了训练门就恰发一帧（成功在 `CheckMagicLevelUp` 内、
        // 失败由调用方补发），内容都是调用后的 `Level`/`TranPoint`。
        let want_msg = if o.trained {
            expect_msg(row.magic_id, state.level, state.tran_point)
        } else {
            String::new()
        };
        if row.msg != want_msg {
            out.push(format!(
                "prof[{n}] {} 帧不等: got={:?} want={:?}",
                row.name, row.msg, want_msg
            ));
        }
        if o.trained != !row.msg.is_empty() {
            out.push(format!(
                "prof[{n}] {} trained 与帧不一致: trained={} msg={:?}",
                row.name, o.trained, row.msg
            ));
        }
    }
    out
}

// ---- 第四轴登记：no_effect_case / no_switch_case 的尾部行为 -------------------------------------

#[derive(Clone)]
struct ReachRow {
    row_index: i64,
    magic_id: u16,
    name: String,
    class: String,
    tail_effect: String,
}

fn load_reach() -> Vec<ReachRow> {
    read_lines("spell-tail-probe/vectors/spell_tail_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "reach")
        .map(|f| ReachRow {
            row_index: i(&f, 1),
            magic_id: i(&f, 2) as u16,
            name: f[3].clone(),
            class: f[4].clone(),
            tail_effect: f[5].clone(),
        })
        .collect()
}

/// 从已入库的 `skill-probe` 登记读 `no_effect_case`/`no_switch_case` 行（保持装表顺序）。
fn registry_tail_rows() -> Vec<(i64, u16, String, String)> {
    let mut out = Vec::new();
    let mut idx = 0i64;
    for f in read_lines("skill-probe/vectors/skill_vectors.tsv") {
        if f[0] != "reg" {
            continue;
        }
        idx += 1;
        let class = f[8].clone();
        if class == "no_effect_case" || class == "no_switch_case" {
            out.push((idx, i(&f, 1) as u16, f[2].clone(), class));
        }
    }
    out
}

fn reach_mismatches(rows: &[ReachRow]) -> Vec<String> {
    let mut out = Vec::new();
    let expected = registry_tail_rows();
    if rows.len() != expected.len() {
        out.push(format!(
            "reach 行数不等: got={} want={}",
            rows.len(),
            expected.len()
        ));
    }
    for (k, (row, exp)) in rows.iter().zip(expected.iter()).enumerate() {
        if row.row_index != exp.0
            || row.magic_id != exp.1
            || row.name != exp.2
            || row.class != exp.3
        {
            out.push(format!(
                "reach[{k}] 登记不等: got=({},{},{:?},{:?}) want=({},{},{:?},{:?})",
                row.row_index, row.magic_id, row.name, row.class, exp.0, exp.1, exp.2, exp.3
            ));
        }
        let want_effect = if row.class == "no_switch_case" {
            "no_case_no_train"
        } else if row.magic_id == 43 {
            "case_break_no_train"
        } else {
            "case_train"
        };
        if row.tail_effect != want_effect {
            out.push(format!(
                "reach[{k}] 魔号 {} tail_effect 不等: got={:?} want={want_effect}",
                row.magic_id, row.tail_effect
            ));
        }
    }
    out
}

// ---- 用例 --------------------------------------------------------------------------------------

#[test]
fn train_skill_matches_oracle_per_case() {
    let rows = load_train();
    assert_eq!(rows.len(), 5, "向量行数（探针改口径要同步本断言）");
    let bad = train_mismatches(&rows);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    assert_eq!(
        rows.iter()
            .find(|r| r.name == "train_fast_3")
            .unwrap()
            .out_tp,
        109,
        "FastTrain 时 100 + 3*3"
    );
    assert_eq!(
        rows.iter()
            .find(|r| r.name == "train_plain_3")
            .unwrap()
            .out_tp,
        103,
        "非 FastTrain 时 100 + 3"
    );
}

#[test]
fn check_magic_level_up_matches_oracle_per_case() {
    let rows = load_chklv();
    assert_eq!(rows.len(), 9, "向量行数（探针改口径要同步本断言）");
    let bad = chklv_mismatches(&rows);
    assert!(bad.is_empty(), "{}", bad.join("\n"));

    // 非平凡：升级/不升级、单次只升一级、死代码分支、28 的看护法量都覆盖到。
    let find = |name: &str| rows.iter().find(|r| r.name == name).unwrap();
    assert!(find("chklv_exact_lv0").out_result && find("chklv_exact_lv0").out_level == 1);
    assert!(
        find("chklv_one_level_only").out_level == 1 && find("chklv_one_level_only").out_tp == 400
    );
    assert!(
        !find("chklv_maxlevel3").out_result,
        "TrainLv=3 时 Level=3 不再升"
    );
    assert!(
        !find("chklv_level4_gate").out_result,
        "Level>=4 时 nLevel=0 且不升"
    );
    assert!(
        !find("chklv_trainlv_lt_level").out_result,
        "TrainLv<Level 时不升"
    );
    assert!(
        find("chklv_showhp_lv1_to_2").see_heal,
        "28 升到 2 级置看护法量"
    );
    assert!(!find("chklv_showhp_lv0_to_1").see_heal, "28 升到 1 级不置");
}

#[test]
fn check_skill_proficiency_matches_oracle_per_case() {
    let rows = load_prof();
    assert_eq!(rows.len(), 7, "向量行数（探针改口径要同步本断言）");
    let bad = prof_mismatches(&rows);
    assert!(bad.is_empty(), "{}", bad.join("\n"));

    let find = |name: &str| rows.iter().find(|r| r.name == name).unwrap();
    assert!(find("prof_gate_level3").msg.is_empty(), "Level>=3 不训练");
    assert!(
        find("prof_gate_charlevel_low").msg.is_empty(),
        "TrainLevel>Abil.Level 不训练"
    );
    assert_eq!(find("prof_train_levelup").out_level, 1);
    assert_eq!(
        find("prof_fast_train_levelup").out_tp,
        6,
        "FastTrain 3*3 后扣 100"
    );
    assert!(find("prof_showhp_see_gauge").see_heal);
    assert!(
        !find("prof_lowtrainlv").msg.is_empty(),
        "TrainLv=1 但 TrainLevel[1]=0 仍训练且不发升级（CheckMagicLevelUp false）"
    );
}

#[test]
fn no_effect_and_no_switch_rows_are_classified_by_tail_effect() {
    let rows = load_reach();
    assert_eq!(rows.len(), 26, "6 no_effect_case + 20 no_switch_case");
    let bad = reach_mismatches(&rows);
    assert!(bad.is_empty(), "{}", bad.join("\n"));

    let break_set: BTreeSet<u16> = rows
        .iter()
        .filter(|r| r.tail_effect == "case_break_no_train")
        .map(|r| r.magic_id)
        .collect();
    let train_set: BTreeSet<u16> = rows
        .iter()
        .filter(|r| r.tail_effect == "case_train")
        .map(|r| r.magic_id)
        .collect();
    let no_case: BTreeSet<u16> = rows
        .iter()
        .filter(|r| r.tail_effect == "no_case_no_train")
        .map(|r| r.magic_id)
        .collect();
    assert_eq!(break_set, BTreeSet::from([43]), "case 体 break 只有 43");
    assert_eq!(
        train_set,
        BTreeSet::from([49, 51, 52, 53, 54]),
        "只置 boTrain 的 5 行"
    );
    assert_eq!(
        no_case.len(),
        18,
        "20 行 no_switch_case / 18 个不同魔号（66/71 各 2 行）"
    );
}

#[test]
fn do_spell_tail_gate_and_draw_order_source_cited() {
    let state = |tp: i32, train_level0: u8| SkillTrainState {
        magic_id: 1,
        level: 0,
        tran_point: tp,
        train_lv: 3,
        train_level: [train_level0, 9, 11, 0],
        max_train: [100, 200, 300, 400],
    };

    // ① `boSpellFail` ⇒ `return false`，不取数、不发帧、不训练（oracle :644-647）。
    let mut rng = replay("1|Next(max)|3|2|max");
    let mut st = state(100, 7);
    let o = do_spell_tail(true, true, true, false, 40, &mut rng, &mut st);
    assert!(!o.returned && !o.magic_fire && !o.train.trained);
    assert_eq!(rng.remaining(), 1, "早退不得取数");

    // ② `boTrain=false` ⇒ 只有 `RM_MAGICFIRE`（43 / 55-80 的尾部），不取数不训练（oracle :648）。
    let mut rng = replay("1|Next(max)|3|2|max");
    let mut st = state(100, 7);
    let o = do_spell_tail(false, true, false, false, 40, &mut rng, &mut st);
    assert!(o.returned && o.magic_fire && !o.train.trained);
    assert_eq!(rng.remaining(), 1, "boTrain=false 不得取数");

    // ③ `boTrain=true` 且门过 ⇒ **恰取数一次**并累加熟练点（oracle :659-663）；
    //    100 + Random(3)+1 = 103 ≥ MaxTrain[0]=100 ⇒ 同一次调用内还升 1 级并扣点。
    let mut rng = replay("1|Next(max)|3|2|max");
    let mut st = state(100, 7);
    let o = do_spell_tail(false, false, true, false, 40, &mut rng, &mut st);
    assert!(o.train.trained && o.train.level_up && o.returned);
    assert_eq!((st.level, st.tran_point), (1, 3), "100+3 后升 1 级并扣 100");
    assert_eq!(rng.remaining(), 0);
    let divergences = rng.divergences().to_vec();
    assert!(divergences.is_empty(), "取数分歧: {divergences:?}");

    // ④ `boTrain=true` 但 `TrainLevel[0] > Abil.Level` ⇒ 门不过，**不取数**（顺序：先门后抽）。
    let mut rng = replay("1|Next(max)|3|2|max");
    let mut st = state(100, 41);
    let o = do_spell_tail(false, false, true, false, 40, &mut rng, &mut st);
    assert!(!o.train.trained);
    assert_eq!(rng.remaining(), 1, "门不过不得取数");
}

// ---- 红检（自动） --------------------------------------------------------------------------------

#[test]
fn redcheck_corrupt_one_train_point_must_fail() {
    let mut rows = load_train();
    assert!(train_mismatches(&rows).is_empty(), "前置：原向量必须全等");
    rows[3].out_tp = rows[3].out_tp.wrapping_add(1);
    let bad = train_mismatches(&rows);
    assert!(
        bad.iter().any(|m| m.contains("输出不等")),
        "改坏 train 点后必红: {bad:?}"
    );
}

#[test]
fn redcheck_corrupt_one_levelup_result_must_fail() {
    let mut rows = load_chklv();
    assert!(chklv_mismatches(&rows).is_empty(), "前置：原向量必须全等");
    let idx = rows
        .iter()
        .position(|r| r.name == "chklv_exact_lv0")
        .unwrap();
    rows[idx].out_level = rows[idx].out_level.wrapping_add(1);
    let bad = chklv_mismatches(&rows);
    assert!(
        bad.iter().any(|m| m.contains("状态不等")),
        "改坏升级后等级必红: {bad:?}"
    );
}

#[test]
fn redcheck_corrupt_one_prof_point_must_fail() {
    let mut rows = load_prof();
    assert!(prof_mismatches(&rows).is_empty(), "前置：原向量必须全等");
    let idx = rows
        .iter()
        .position(|r| r.name == "prof_train_levelup")
        .unwrap();
    rows[idx].out_tp = rows[idx].out_tp.wrapping_add(1);
    let bad = prof_mismatches(&rows);
    assert!(
        bad.iter().any(|m| m.contains("状态不等")),
        "改坏 prof 熟练点必红: {bad:?}"
    );
}

#[test]
fn redcheck_corrupt_one_tail_effect_must_fail() {
    let mut rows = load_reach();
    assert!(reach_mismatches(&rows).is_empty(), "前置：原向量必须全等");
    let idx = rows.iter().position(|r| r.magic_id == 43).unwrap();
    rows[idx].tail_effect = "case_train".to_string();
    let bad = reach_mismatches(&rows);
    assert!(
        bad.iter().any(|m| m.contains("tail_effect 不等")),
        "改坏 43 的 tail_effect 必红: {bad:?}"
    );
}
