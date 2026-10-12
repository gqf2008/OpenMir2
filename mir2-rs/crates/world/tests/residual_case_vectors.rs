//! 第十一片（DoD③ 第六轴）：`DoSpell` switch 里**内联 case** 的端到端真身向量——机器判据。
//!
//! 向量来源：`mir2-rs/tests/parity/m3/residual-case-probe`（C#，反射直调 oracle 私有
//! `PlayObject.DoSpell(UserMagic, short, short, IActor)`）
//! → `residual-case-probe/vectors/residual_case.{json,tsv}`。
//!
//! **为什么读 TSV 而不是 JSON**：与 `combat_skill_vectors.rs` / `dispatch_surface_vectors.rs` 同理——
//! `crates/world` 的依赖面被 `tests/no_parallel.rs` 钉成「内部 crate + bevy_ecs/bevy_app」，
//! 加 `serde_json` 会被判红；探针同一次运行产出行式伴生件，本用例零依赖解析。
//!
//! ## 本片回答的问题
//!
//! 第五片把 108 行魔表分成 6 类；到第十片为止：
//!   * `no_switch_case`（20 行）+ `magicid_reject`（35 行）已有 `DoSpell` 真身向量（第十片）；
//!   * `no_effect_case`（43/49/51/52/53/54，共 6 行）的**共享尾**被第九片 source-cited；
//!   * `context_required` 的内联效果行 40（`SKILL_UNAMYOUNSUL` 清三态计时器）被第七片登记为 `<inline>`。
//!
//! 即：**这 7 行本身没有 `DoSpell` 端到端真身向量**。本片补齐，并逐行用 Rust 原语复算：
//!   * `get_spell_point`（MP 扣减）；
//!   * `is_warr_skill` / `spell_version_gate`（前置）；
//!   * `do_spell_tail`（共享尾：训练门 + `Random(3)+1` + `TrainSkill`/`CheckMagicLevelUp`），
//!     用 [`ReplayRandom`] **强制回放**向量里的取数账本。
//!
//! 红检（自动，≥2）：把同一比较函数作用在被改坏一个数的向量副本上，断言必出差异。

use std::collections::BTreeSet;
use std::path::PathBuf;

use mir2_data::models::MagicInfo;
use mir2_shared::replay::ReplayRandom;
use mir2_world::magic::{
    do_spell_tail, get_spell_point, is_warr_skill, spell_version_gate, SkillTrainState,
};

fn repo_parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/m3")
}

fn read_lines(path: &str) -> Vec<Vec<String>> {
    let full = repo_parity_dir().join(path);
    let text = std::fs::read_to_string(&full)
        .unwrap_or_else(|e| panic!("读不到向量 {}: {e}", full.display()));
    text.lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.is_empty())
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect()
}

fn i(fields: &[String], idx: usize) -> i64 {
    fields[idx]
        .parse::<i64>()
        .unwrap_or_else(|e| panic!("字段 {idx} 非整数: {e}"))
}

/// 状态计时器三元组 `d,a,s` -> `[u16;3]`。
fn triplet(s: &str) -> [i64; 3] {
    let v: Vec<i64> = s
        .split(',')
        .map(|x| x.parse::<i64>().expect("状态三元组"))
        .collect();
    assert_eq!(v.len(), 3, "状态三元组应为 3 项: {s:?}");
    [v[0], v[1], v[2]]
}

/// 取数账本 `1|Next(max)|7|0|max;…` -> `[(arg, value)]`。
fn draws(s: &str) -> Vec<(i64, i64)> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(';')
        .map(|entry| {
            let f: Vec<&str> = entry.split('|').collect();
            assert_eq!(f.len(), 5, "取数账本项格式错: {entry:?}");
            (
                f[2].parse::<i64>().expect("draw arg"),
                f[3].parse::<i64>().expect("draw value"),
            )
        })
        .collect()
}

/// 把取数账本转成 [`ReplayRandom`]（`|` -> 制表符）。
fn replay(s: &str) -> ReplayRandom {
    let mut tsv = String::new();
    if !s.is_empty() {
        for entry in s.split(';') {
            tsv.push_str(&entry.replace('|', "\t"));
            tsv.push('\n');
        }
    }
    ReplayRandom::from_tsv(&tsv).unwrap_or_else(|e| panic!("取数账本非法: {e} ({s})"))
}

#[derive(Debug, Clone, PartialEq)]
struct Row {
    kind: String,
    magic_id: i64,
    name: String,
    um_level: i64,
    abil_level: i64,
    train_lv0: i64,
    status_before: String,
    mp_before: i64,
    spell_point: i64,
    mp_after: i64,
    result: bool,
    draws: String,
    status_after: String,
    tran_before: i64,
    tran_after: i64,
    level_before: i64,
    level_after: i64,
    msg_count: i64,
    msg: String,
    exception: String,
}

fn load() -> Vec<Row> {
    read_lines("residual-case-probe/vectors/residual_case.tsv")
        .into_iter()
        .filter(|f| f[0] == "resid")
        .map(|f| Row {
            kind: f[1].clone(),
            magic_id: i(&f, 2),
            name: f[3].clone(),
            um_level: i(&f, 4),
            abil_level: i(&f, 5),
            train_lv0: i(&f, 6),
            status_before: f[8].clone(),
            mp_before: i(&f, 9),
            spell_point: i(&f, 10),
            mp_after: i(&f, 11),
            result: i(&f, 12) != 0,
            draws: f[13].clone(),
            status_after: f[14].clone(),
            tran_before: i(&f, 15),
            tran_after: i(&f, 16),
            level_before: i(&f, 17),
            level_after: i(&f, 18),
            msg_count: i(&f, 19),
            msg: f.get(20).cloned().unwrap_or_default(),
            exception: f.get(21).cloned().unwrap_or_default(),
        })
        .collect()
}

/// `skill-probe/vectors/skill_vectors.tsv` 的 `reg` 行：`(id, dospell_class)`。
fn registry_classes() -> Vec<(i64, String)> {
    read_lines("skill-probe/vectors/skill_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "reg")
        .map(|f| (i(&f, 1), f[8].clone()))
        .collect()
}

fn probe_magic() -> MagicInfo {
    MagicInfo {
        spell: 20,
        def_spell: 0,
        ..Default::default()
    }
}

/// 共享尾 `boTrain`：40 仅当状态清除门命中才置真；49/51/52/53/54 恒真；43 恒假。
fn bo_train(row: &Row) -> bool {
    match row.kind.as_str() {
        "inline_effect_cleared" => true,
        "inline_effect_not_cleared" | "no_effect_break" => false,
        "no_effect_train" | "no_effect_train_level3" => true,
        other => panic!("未知的 residual 行类别 {other}"),
    }
}

fn train_state(row: &Row) -> SkillTrainState {
    SkillTrainState {
        magic_id: row.magic_id as u16,
        level: row.um_level as u8,
        tran_point: row.tran_before as i32,
        train_lv: 3,
        train_level: [row.train_lv0 as u8, 9, 11, 0],
        max_train: [100, 200, 300, 400],
    }
}

/// 返回 oracle 与 Rust（原语复算）的逐字段差异（空 = 全等）。红检共用。
fn mismatches(rows: &[Row]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let label = format!(
            "[{n}] {} id={} {} (lv={} abil={})",
            row.kind, row.magic_id, row.name, row.um_level, row.abil_level
        );

        // 前置：非战士技能、版本门通过 ⇒ 到达 switch。
        if is_warr_skill(row.magic_id as i32) {
            out.push(format!("{label}: is_warr_skill 不应为真"));
        }
        if !spell_version_gate(1, 1, row.magic_id as i32) {
            out.push(format!("{label}: spell_version_gate 不应拒绝"));
        }

        // MP 扣减 = GetSpellPoint（Rust 原语复算）。
        let want_sp = i64::from(get_spell_point(&probe_magic(), row.um_level as u8));
        if row.spell_point != want_sp {
            out.push(format!(
                "{label}: spell_point got={} want={want_sp}",
                row.spell_point
            ));
        }
        if row.mp_after != row.mp_before - want_sp {
            out.push(format!(
                "{label}: mp_after got={} want={}",
                row.mp_after,
                row.mp_before - want_sp
            ));
        }
        if !row.result {
            out.push(format!("{label}: 返回应为 true"));
        }
        if !row.exception.is_empty() {
            out.push(format!("{label}: 不应抛异常 {}", row.exception));
        }

        // 共享尾：用向量里的取数账本强制回放，Rust 原语复算训练/升级。
        // 40 的 case 先抽过 `Random(7)`，尾账本要剥掉首条；其余行账本即尾账本。
        let tail_draws = if row.kind.starts_with("inline_effect_") {
            let d = draws(&row.draws);
            if d.is_empty() {
                String::new()
            } else {
                // 重建「去掉第一条」的账本串。
                let s: Vec<String> = row.draws.split(';').skip(1).map(str::to_string).collect();
                s.join(";")
            }
        } else {
            row.draws.clone()
        };
        let mut rng = replay(&tail_draws);
        let mut state = train_state(row);
        let tail = do_spell_tail(
            false,
            true,
            bo_train(row),
            false,
            row.abil_level as u8,
            &mut rng,
            &mut state,
        );
        if !tail.returned {
            out.push(format!("{label}: 共享尾应 return true"));
        }
        if !tail.magic_fire {
            out.push(format!("{label}: 共享尾应发 RM_MAGICFIRE"));
        }
        let tail_trained = !tail_draws.is_empty();
        if tail.train.trained != tail_trained {
            out.push(format!(
                "{label}: 训练门 got={} want={tail_trained}",
                tail.train.trained
            ));
        }
        for d in rng.divergences() {
            out.push(format!("{label}: 尾取数分歧 {d}"));
        }
        if rng.remaining() != 0 {
            out.push(format!("{label}: 尾取数未消费完 剩 {}", rng.remaining()));
        }
        if state.tran_point != row.tran_after as i32 {
            out.push(format!(
                "{label}: tran_after got={} want={}",
                row.tran_after, state.tran_point
            ));
        }
        if state.level != row.level_after as u8 {
            out.push(format!(
                "{label}: level_after got={} want={}",
                row.level_after, state.level
            ));
        }

        // 40 内联效果：状态门 `Random(7) - (Level+1) < 0`。
        if row.kind.starts_with("inline_effect_") {
            let before = triplet(&row.status_before);
            let after = triplet(&row.status_after);
            let d = draws(&row.draws);
            if d.is_empty() {
                out.push(format!("{label}: 应至少抽一次 Random(7)"));
            } else {
                if d[0].0 != 7 {
                    out.push(format!("{label}: 首发取数应为 Random(7)，实为 {}", d[0].0));
                }
                let cleared = d[0].1 - (row.um_level + 1) < 0;
                let want_after: [i64; 3] = if cleared {
                    [
                        if before[0] != 0 { 1 } else { 0 },
                        if before[1] != 0 { 1 } else { 0 },
                        if before[2] != 0 { 1 } else { 0 },
                    ]
                } else {
                    before
                };
                if after != want_after {
                    out.push(format!(
                        "{label}: status_after got={after:?} want={want_after:?} (cleared={cleared})"
                    ));
                }
                if cleared != (row.kind == "inline_effect_cleared") {
                    out.push(format!("{label}: cleared 与 kind 不一致"));
                }
            }
        }

        // 自身出帧（帧内容为 oracle 原值；40 的 targetObject=自身 ⇒ ActorId=910040）。
        let prefix = format!("10008:0:0:{};10052:0:0:0", row.magic_id);
        if row.kind == "inline_effect_cleared" {
            let want = format!(
                "{prefix};10120:257:0:910040;10123:{}:0:{}",
                row.magic_id, row.tran_after
            );
            if row.msg != want {
                out.push(format!("{label}: 帧不等 got={} want={want}", row.msg));
            }
            if row.msg_count != 4 {
                out.push(format!("{label}: msg_count got={} want=4", row.msg_count));
            }
        } else if row.kind == "inline_effect_not_cleared" {
            let want = format!("{prefix};10120:257:0:910040");
            if row.msg != want {
                out.push(format!("{label}: 帧不等 got={} want={want}", row.msg));
            }
            if row.msg_count != 3 {
                out.push(format!("{label}: msg_count got={} want=3", row.msg_count));
            }
        } else if !tail_draws.is_empty() {
            // 训练尾见效 ⇒ 多一帧 `RM_MAGIC_LVEXP`（内容为调用后的 TranPoint）。
            let want = format!(
                "{prefix};10120:257:0:0;10123:{}:0:{}",
                row.magic_id, row.tran_after
            );
            if row.msg != want {
                out.push(format!("{label}: 帧不等 got={} want={want}", row.msg));
            }
            if row.msg_count != 4 {
                out.push(format!("{label}: msg_count got={} want=4", row.msg_count));
            }
        } else {
            let want = format!("{prefix};10120:257:0:0");
            if row.msg != want {
                out.push(format!("{label}: 帧不等 got={} want={want}", row.msg));
            }
            if row.msg_count != 3 {
                out.push(format!("{label}: msg_count got={} want=3", row.msg_count));
            }
        }
    }
    out
}

// ---- 判据 A：覆盖行 = 本片声明的 7 行 ---------------------------------------------------------
#[test]
fn residual_rows_cover_declared_switch_inline_cases() {
    let rows = load();
    assert_eq!(
        rows.len(),
        15,
        "residual 向量行数应为 2 + 2 + (5×2) + 1 = 15"
    );

    let ids: BTreeSet<i64> = rows.iter().map(|r| r.magic_id).collect();
    let want: BTreeSet<i64> = [40, 43, 49, 51, 52, 53, 54].into_iter().collect();
    assert_eq!(ids, want, "覆盖魔号集合应为 40 ∪ no_effect_case(6)");

    // 与 skill-probe 的登记类别一致：40 是 context_required，其余 6 行是 no_effect_case。
    let classes: std::collections::BTreeMap<i64, String> = registry_classes().into_iter().collect();
    assert_eq!(
        classes.get(&40).map(String::as_str),
        Some("context_required"),
        "40 在登记里应为 context_required"
    );
    let no_effect: BTreeSet<i64> = classes
        .iter()
        .filter(|(_, c)| c.as_str() == "no_effect_case")
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(
        no_effect,
        [43, 49, 51, 52, 53, 54].into_iter().collect(),
        "no_effect_case 集合应为 {{43,49,51,52,53,54}}"
    );
    // 覆盖行数上界：本片只碰这 7 行，不动 no_switch_case/magicid_reject/warr_reject/implemented。
    assert!(!ids.contains(&42), "42 已实装，不应在本片向量里");
    assert!(
        !ids.contains(&55),
        "55 no_switch_case 属第十片，不应在本片向量里"
    );
    assert!(
        !ids.contains(&81),
        "81 magicid_reject 属第十片，不应在本片向量里"
    );
}

// ---- 判据 B：逐字段全等（oracle 向量 vs Rust 原语复算）----------------------------------------
#[test]
fn residual_cases_match_oracle_per_case() {
    let rows = load();
    let bad = mismatches(&rows);
    assert!(bad.is_empty(), "residual 向量不一致:\n{}", bad.join("\n"));

    // 非平凡断言：清成/未清成两态、训练见效/不见效、训练等级门都真的出现过。
    assert!(rows.iter().any(|r| r.kind == "inline_effect_cleared"));
    assert!(rows.iter().any(|r| r.kind == "inline_effect_not_cleared"));
    assert!(rows
        .iter()
        .any(|r| !draws(&r.draws).is_empty() && r.kind == "no_effect_train"));
    assert!(rows
        .iter()
        .any(|r| draws(&r.draws).is_empty() && r.kind == "no_effect_train"));
    assert!(rows.iter().any(|r| r.kind == "no_effect_train_level3"));
}

// ---- 判据 C：改坏必红（3 条）------------------------------------------------------------------
#[test]
fn redcheck_corrupt_inline_status_after_must_fail() {
    let mut rows = load();
    let idx = rows
        .iter()
        .position(|r| r.kind == "inline_effect_cleared")
        .expect("应有清成行");
    rows[idx].status_after = rows[idx].status_before.clone();
    assert!(!mismatches(&rows).is_empty(), "改坏 40 的状态结果必须判红");
}

#[test]
fn redcheck_corrupt_training_draw_must_fail() {
    let mut rows = load();
    // 找一条训练见效行（尾账本 `Next(3)`），把账本值 +1 ⇒ TranPoint 复算必不一致。
    let idx = rows
        .iter()
        .position(|r| r.kind == "no_effect_train" && !draws(&r.draws).is_empty())
        .expect("应有训练见效行");
    let (arg, val) = draws(&rows[idx].draws)[0];
    rows[idx].draws = format!("1|Next(max)|{arg}|{}|max", val + 1);
    assert!(!mismatches(&rows).is_empty(), "改坏训练取数账本必须判红");
}

#[test]
fn redcheck_corrupt_self_frame_must_fail() {
    let mut rows = load();
    let idx = rows
        .iter()
        .position(|r| r.kind == "inline_effect_not_cleared")
        .expect("应有未清成行");
    rows[idx].msg = rows[idx].msg.replace("10120:257:0:910040", "10120:257:0:1");
    assert!(!mismatches(&rows).is_empty(), "改坏自身出帧必须判红");
}
