//! 判据①③（M3 逐系统对齐·技能，第十片）：把 DoD③ 里 7 条「map-bound」`context_required` 逐条判
//! 「能不能离线做」，并把 (a) 类（只需真 `Envir`/地图数据）的**真身向量**逐字段核对进 Rust。
//!
//! 向量来源：`mir2-rs/tests/parity/m3/map-effect-probe`（C#，真身反射直调
//! `MagicManager.MagPushArround`（`MagicManager.cs:24-52`）/`MagSaceMove`（`:876-891`）/
//! `MagMakeClone`（`:1337-1341`）；合成真格式 `.map` 交给真身 `Envirnoment.LoadMapData`；
//! `System.Random` 覆盖为记录包装），产出 `vectors/map_effect_vectors.{json,tsv}` +
//! 7 行三分类 `vectors/context_reachability.tsv`。
//!
//! **为什么读 TSV 而不是 JSON**：与同目录 `magic_effect_vectors.rs` 同因 ——
//! `tests/no_parallel.rs` 把 `crates/world` 的依赖面钉死，加 `serde_json` 会被判红。
//!
//! 同一 RNG 取数路径：向量把取数账本按 `ReplayRandom` 行式格式带出，本用例用
//! [`ReplayRandom`] **强制回放同值**，再断言「取数序列零分歧（arg/kind 逐条相同）+
//! 消费到底 + 可观测输出逐字段相等」。
//!
//! 红检（自动）：三条 `redcheck_*` 把**同一比较函数**作用在被改坏一个数的向量副本上，
//! 断言必出差异 —— 证明「逐字段一致」不是恒绿。
//!
//! 分类门禁：`context_reachability.tsv` 断言 8/37/21/46 = `a_offline_done`、17/30/41 =
//! `c_real_summon` 且 `missing_prereq` 点明 oracle 行号（不许只写「需要真环境」）。

use mir2_shared::replay::ReplayRandom;
use mir2_world::magic::{
    mag_make_clone, mag_push_arround, mag_sace_move, MakeCloneInput, PushAroundInput, PushTarget,
    SaceMoveInput,
};
use mir2_world::map::{CellAttribute, MapGrid};
use std::path::PathBuf;

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

fn all_walkable_map(w: i32, h: i32) -> MapGrid {
    MapGrid::new(w, h)
}

fn map_with_walls(w: i32, h: i32, walls: &str) -> MapGrid {
    let mut map = MapGrid::new(w, h);
    for w in walls.split(';').filter(|s| !s.is_empty()) {
        let mut it = w.split(',');
        let x: i32 = it.next().unwrap().parse().unwrap();
        let y: i32 = it.next().unwrap().parse().unwrap();
        map.get_cell_info_mut(x, y)
            .0
            .set_attribute(CellAttribute::HighWall);
    }
    map
}

// ---- 8 / 37 MagPushArround ----------------------------------------------------------------------

#[derive(Clone)]
struct PushCase {
    map_w: i32,
    map_h: i32,
    walls: String,
    caster_x: i32,
    caster_y: i32,
    caster_level: u8,
    push_level: i32,
    targets: Vec<PushTarget>,
    draws: String,
    result: i32,
    /// 每个目标调用后的 `(x, y, dir)`（与 `targets` 同序）。
    outcomes: Vec<(i32, i32, u8)>,
}

fn load_push() -> Vec<PushCase> {
    read_lines("map-effect-probe/vectors/map_effect_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "push")
        .map(|f| {
            let targets = f[12]
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|t| {
                    let p: Vec<i64> = t.split(',').map(|v| v.parse().unwrap()).collect();
                    PushTarget {
                        actor_id: p[0] as i32,
                        x: p[1] as i32,
                        y: p[2] as i32,
                        level: p[3] as u8,
                        death: p[4] != 0,
                        stick_mode: p[5] != 0,
                        dir: p[6] as u8,
                        proper_target: p[7] != 0,
                    }
                })
                .collect();
            let outcomes = f[15]
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|o| {
                    let p: Vec<i64> = o.split(',').map(|v| v.parse().unwrap()).collect();
                    (p[0] as i32, p[1] as i32, p[2] as u8)
                })
                .collect();
            PushCase {
                map_w: i(&f, 9) as i32,
                map_h: i(&f, 10) as i32,
                walls: f[11].clone(),
                caster_x: i(&f, 5) as i32,
                caster_y: i(&f, 6) as i32,
                caster_level: i(&f, 7) as u8,
                push_level: i(&f, 8) as i32,
                targets,
                draws: f[13].clone(),
                result: i(&f, 14) as i32,
                outcomes,
            }
        })
        .collect()
}

/// 返回 oracle 与 Rust 的逐字段差异（空 = 全等）。红检共用。
fn push_mismatches(cases: &[PushCase]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, c) in cases.iter().enumerate() {
        let mut map = map_with_walls(c.map_w, c.map_h, &c.walls);
        let inp = PushAroundInput {
            caster_x: c.caster_x,
            caster_y: c.caster_y,
            caster_level: c.caster_level,
            push_level: c.push_level,
            targets: c.targets.clone(),
        };
        let mut rng = replay(&c.draws);
        let got = mag_push_arround(&mut rng, &mut map, &inp);
        for d in rng.divergences() {
            out.push(format!("push[{n}] 取数分歧: {d}"));
        }
        if rng.remaining() != 0 {
            out.push(format!("push[{n}] 取数未消费完: 剩 {}", rng.remaining()));
        }
        if got.result != c.result {
            out.push(format!(
                "push[{n}] result: got={} want={}",
                got.result, c.result
            ));
        }
        if got.targets.len() != c.outcomes.len() {
            out.push(format!(
                "push[{n}] 目标数: got={} want={}",
                got.targets.len(),
                c.outcomes.len()
            ));
            continue;
        }
        for (k, (t, (x, y, d))) in got.targets.iter().zip(c.outcomes.iter()).enumerate() {
            if t.x != *x || t.y != *y || t.dir != *d {
                out.push(format!(
                    "push[{n}] 目标{k}: got=({},{},{}) want=({},{},{})",
                    t.x, t.y, t.dir, x, y, d
                ));
            }
        }
    }
    out
}

#[test]
fn push_arround_matches_oracle_per_case() {
    let cases = load_push();
    assert!(cases.len() >= 56, "push 用例数 {}", cases.len());
    let bad = push_mismatches(&cases);
    assert!(
        bad.is_empty(),
        "抗拒火环/雷神之怒向量不一致:\n{}",
        bad.join("\n")
    );
    // 非平凡门：确有受推（位移非平凡）、确有墙阻挡（push_count 小于满推）、确有 proper_target 拒绝。
    assert!(
        cases.iter().any(|c| c
            .outcomes
            .iter()
            .zip(c.targets.iter())
            .any(|((x, y, _), t)| (*x, *y) != (t.x, t.y))),
        "无位移用例"
    );
    assert!(
        cases
            .iter()
            .any(|c| c.targets.iter().any(|t| !t.proper_target)),
        "无 IsProperTarget 拒绝用例"
    );
    assert!(cases.iter().any(|c| c.walls.contains(',')), "无墙阻挡用例");
}

/// 红检⑬（技能·push）：把一条用例里某个目标的落点改坏 ⇒ 必出差异。
#[test]
fn redcheck_corrupt_one_push_position_must_fail() {
    let mut cases = load_push();
    let (ci, ti) = cases
        .iter()
        .enumerate()
        .find_map(|(ci, c)| {
            c.outcomes
                .iter()
                .zip(c.targets.iter())
                .position(|((x, y, _), t)| (*x, *y) != (t.x, t.y))
                .map(|ti| (ci, ti))
        })
        .expect("应有目标发生位移");
    cases[ci].outcomes[ti].0 += 1;
    assert!(
        !push_mismatches(&cases).is_empty(),
        "红检失效：改动落点后仍全等"
    );
}

// ---- 21 MagSaceMove -----------------------------------------------------------------------------

#[derive(Clone)]
struct SaceCase {
    level: i32,
    map_found: bool,
    same_envir: bool,
    map_w: i32,
    map_h: i32,
    result: bool,
    space_moved: bool,
    is_time_recall: bool,
    x: i32,
    y: i32,
    draws: String,
}

fn load_sace() -> Vec<SaceCase> {
    read_lines("map-effect-probe/vectors/map_effect_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "sace")
        .map(|f| SaceCase {
            level: i(&f, 3) as i32,
            map_found: i(&f, 4) != 0,
            same_envir: i(&f, 5) != 0,
            map_w: i(&f, 6) as i32,
            map_h: i(&f, 7) as i32,
            x: i(&f, 8) as i32,
            y: i(&f, 9) as i32,
            draws: f[10].clone(),
            result: i(&f, 11) != 0,
            space_moved: i(&f, 12) != 0,
            is_time_recall: i(&f, 13) != 0,
        })
        .collect()
}

fn sace_mismatches(cases: &[SaceCase]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, c) in cases.iter().enumerate() {
        let map = all_walkable_map(c.map_w, c.map_h);
        let inp = SaceMoveInput {
            level: c.level,
            map_found: c.map_found,
            same_envir: c.same_envir,
            map_width: c.map_w,
            map_height: c.map_h,
            x: c.x,
            y: c.y,
            is_time_recall: true, // 探针调用前置 true（MagSaceMove 只在换图时置 false）
        };
        let mut rng = replay(&c.draws);
        let got = mag_sace_move(&mut rng, &map, &inp);
        for d in rng.divergences() {
            out.push(format!("sace[{n}] 取数分歧: {d}"));
        }
        if rng.remaining() != 0 {
            out.push(format!("sace[{n}] 取数未消费完: 剩 {}", rng.remaining()));
        }
        if got.result != c.result
            || got.moved != c.space_moved
            || got.is_time_recall != c.is_time_recall
            || got.x != c.x
            || got.y != c.y
        {
            out.push(format!(
                "sace[{n}] 输出不等: got={got:?} want=({}, {}, {}, {:?})",
                c.result,
                c.space_moved,
                c.is_time_recall,
                (c.x, c.y)
            ));
        }
    }
    out
}

#[test]
fn sace_move_matches_oracle_per_case() {
    let cases = load_sace();
    assert!(cases.len() >= 96, "sace 用例数 {}", cases.len());
    let bad = sace_mismatches(&cases);
    assert!(bad.is_empty(), "瞬息移动向量不一致:\n{}", bad.join("\n"));
    // 分支覆盖：门未过、同图、换图（IsTimeRecall=false）、HomeMap 缺失。
    assert!(cases.iter().any(|c| !c.result), "无门未过用例");
    assert!(cases.iter().any(|c| c.space_moved), "无移动用例");
    assert!(
        cases.iter().any(|c| !c.is_time_recall),
        "无换图 IsTimeRecall=false 用例"
    );
    assert!(cases.iter().any(|c| !c.map_found), "无 HomeMap 缺失用例");
}

/// 红检⑭（技能·瞬息移动）：把一条「换图」用例的 `is_time_recall` 改坏 ⇒ 必出差异。
#[test]
fn redcheck_corrupt_one_sace_flag_must_fail() {
    let mut cases = load_sace();
    let idx = cases
        .iter()
        .position(|c| !c.is_time_recall)
        .expect("应有换图用例");
    cases[idx].is_time_recall = true;
    assert!(
        !sace_mismatches(&cases).is_empty(),
        "红检失效：改动 is_time_recall 后仍全等"
    );
}

// ---- 46 MagMakeClone ----------------------------------------------------------------------------

#[derive(Clone)]
struct CloneCase {
    caster_dir: u8,
    x: i32,
    y: i32,
    magic_result: bool,
    created: i64,
    name: String,
    clone_dir: u8,
    same_envir: bool,
}

fn load_clone() -> Vec<CloneCase> {
    read_lines("map-effect-probe/vectors/map_effect_vectors.tsv")
        .into_iter()
        .filter(|f| f[0] == "clone")
        .map(|f| CloneCase {
            caster_dir: i(&f, 3) as u8,
            x: i(&f, 4) as i32,
            y: i(&f, 5) as i32,
            magic_result: i(&f, 6) != 0,
            created: i(&f, 7),
            name: f[8].clone(),
            clone_dir: i(&f, 11) as u8,
            same_envir: i(&f, 12) != 0,
        })
        .collect()
}

fn clone_mismatches(cases: &[CloneCase]) -> Vec<String> {
    let mut out = Vec::new();
    for (n, c) in cases.iter().enumerate() {
        let mut map = all_walkable_map(20, 20);
        let got = mag_make_clone(
            &mut map,
            &MakeCloneInput {
                x: c.x,
                y: c.y,
                dir: c.caster_dir,
                clone_id: 990000 + n as i32,
            },
        );
        if !c.magic_result {
            out.push(format!("clone[{n}] 真身 MagMakeClone 未返回 true"));
        }
        if c.name != "Clone" {
            out.push(format!("clone[{n}] 名字应为 Clone: {}", c.name));
        }
        if !c.same_envir {
            out.push(format!("clone[{n}] clone.Envir 应等于施法者 Envir"));
        }
        if (got.created as i64) != c.created
            || got.dir != c.clone_dir
            || got.x != c.x
            || got.y != c.y
        {
            out.push(format!(
                "clone[{n}] 输出不等: got=({}, {}, {}@{},{}) want=(created={}, dir={}@{},{})",
                got.created, got.x, got.y, got.x, got.y, c.created, c.clone_dir, c.x, c.y
            ));
        }
    }
    out
}

#[test]
fn make_clone_matches_oracle_per_case() {
    let cases = load_clone();
    assert_eq!(cases.len(), 8, "clone 用例数应为 8 个方向");
    let bad = clone_mismatches(&cases);
    assert!(bad.is_empty(), "分身术向量不一致:\n{}", bad.join("\n"));
    // 8 个方向各一，覆盖 GetBackDir 全表。
    let dirs: std::collections::BTreeSet<u8> = cases.iter().map(|c| c.caster_dir).collect();
    assert_eq!(dirs, (0u8..8).collect(), "应覆盖 8 个方向");
}

/// 红检⑮（技能·分身术）：把一条用例的 `clone_dir` 改坏 ⇒ 必出差异。
#[test]
fn redcheck_corrupt_one_clone_dir_must_fail() {
    let mut cases = load_clone();
    cases[0].clone_dir ^= 0x7;
    assert!(
        !clone_mismatches(&cases).is_empty(),
        "红检失效：改动 clone_dir 后仍全等"
    );
}

// ---- 7 行离线可行性三分类门禁 -------------------------------------------------------------------

#[test]
fn context_rows_are_classified_offline_or_summon() {
    let rows: Vec<Vec<String>> = read_lines("map-effect-probe/vectors/context_reachability.tsv")
        .into_iter()
        .filter(|f| f[0] == "row" && f.get(1).map(String::as_str) != Some("magic_id"))
        .collect();
    assert_eq!(rows.len(), 7, "应有 7 条 map-bound context_required 行");
    let mut classes = std::collections::BTreeMap::new();
    for f in &rows {
        let id: i32 = i(f, 1) as i32;
        let class = f[5].clone();
        let missing = f[6].clone();
        if class == "c_real_summon" {
            assert!(
                missing.contains("WorldEngine") && missing.contains(".cs:"),
                "行 {id} c_real_summon 的 missing_prereq 必须点明 oracle 前置: {missing}"
            );
        }
        classes.insert(id, class);
    }
    for id in [8, 37, 21, 46] {
        assert_eq!(
            classes.get(&id).map(String::as_str),
            Some("a_offline_done"),
            "行 {id} 应为 a_offline_done"
        );
    }
    for id in [17, 30, 41] {
        assert_eq!(
            classes.get(&id).map(String::as_str),
            Some("c_real_summon"),
            "行 {id} 应为 c_real_summon"
        );
    }
}
