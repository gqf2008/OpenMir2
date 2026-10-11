//! 判据③（M3 逐系统对齐·技能）的「施法前置」补充片：把 oracle 真身探针产出的
//! **武器（护身符）/ 战士技能**向量逐字段核对进 Rust。
//!
//! 向量来源：`mir2-rs/tests/parity/m3/amulet-probe`（C#，直调 oracle 的 public static 入口
//! `MagicManager.IsWarrSkill` 与 `MagicBase.CheckAmulet`，用真 `PlayObject.UseItems` +
//! 真 `GameItemSystem`）→ `amulet-probe/vectors/amulet_vectors.{json,tsv}`。
//!
//! **为什么读 TSV 而不是 JSON**：与 `combat_skill_vectors.rs` 同理——`crates/world` 的依赖面
//! 被 `tests/no_parallel.rs` 钉成「内部 crate + bevy_ecs/bevy_app」，加 `serde_json` 会被判红。
//!
//! 覆盖：
//! - `is_warr_skill`（oracle 反射向量，魔号 0..=110 全表）
//! - `check_amulet`（oracle 直调，24 条场景矩阵：槽位/持久/形状/stdmode/count/type）
//! - `spell_in_range` / `spell_version_gate` / `amulet_requirement`：`DoSpell` 的**内联门**与
//!   `switch` case 标签，oracle 未单独成方法 ⇒ **source-cited**，用「oracle 配置常量 + 逐点断言」
//!   固定语义（`MagicAttackRage` 由探针读出）。

use std::path::PathBuf;

use mir2_world::magic::{
    amulet_requirement, check_amulet, is_warr_skill, spell_in_range, spell_version_gate,
    AmuletSlot, ITEM_LOCATION_ARM_RINGL, ITEM_LOCATION_BUJUK,
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

// ---- 解析 ---------------------------------------------------------------------------------------

#[derive(Clone)]
struct AmuletCase {
    id: String,
    count: u8,
    amulet_type: u8,
    armringl: Option<AmuletSlot>,
    bujuk: Option<AmuletSlot>,
    ok: bool,
    idx: i64,
}

struct Vectors {
    magic_attack_rage: i32,
    magic_hit_interval_time: i32,
    warr_skill_sweep_max: i32,
    warr: Vec<(i32, bool)>,
    amulet: Vec<AmuletCase>,
}

fn slot(idx: i64, std_mode: i64, shape: i64, dura: i64) -> Option<AmuletSlot> {
    if idx < 0 {
        None
    } else {
        Some(AmuletSlot {
            index: idx as i32,
            std_mode: std_mode as u8,
            shape: shape as u8,
            dura: dura as i32,
        })
    }
}

fn load_vectors() -> Vectors {
    let lines = read_lines("amulet-probe/vectors/amulet_vectors.tsv");
    let meta = &lines[0];
    assert_eq!(meta[0], "meta", "首行必须是 meta");
    assert_eq!(meta[1], "magic_attack_rage");
    assert_eq!(meta[3], "magic_hit_interval_time");
    assert_eq!(meta[5], "warr_skill_sweep_max");
    let mut v = Vectors {
        magic_attack_rage: i(meta, 2) as i32,
        magic_hit_interval_time: i(meta, 4) as i32,
        warr_skill_sweep_max: i(meta, 6) as i32,
        warr: Vec::new(),
        amulet: Vec::new(),
    };
    for fields in &lines[1..] {
        match fields[0].as_str() {
            "warr" => v.warr.push((i(fields, 1) as i32, i(fields, 2) != 0)),
            "amulet" => v.amulet.push(AmuletCase {
                id: fields[1].clone(),
                count: i(fields, 2) as u8,
                amulet_type: i(fields, 3) as u8,
                armringl: slot(i(fields, 4), i(fields, 5), i(fields, 6), i(fields, 7)),
                bujuk: slot(i(fields, 8), i(fields, 9), i(fields, 10), i(fields, 11)),
                ok: i(fields, 12) != 0,
                idx: i(fields, 13),
            }),
            other => panic!("未知行类型: {other}"),
        }
    }
    v
}

// ---- 逐字段比较（红检复用同一函数）-------------------------------------------------------------

fn warr_mismatches(rows: &[(i32, bool)]) -> Vec<String> {
    rows.iter()
        .filter(|(mag_idx, want)| is_warr_skill(*mag_idx) != *want)
        .map(|(mag_idx, want)| {
            format!(
                "mag_idx={mag_idx}: rust={} csharp={want}",
                is_warr_skill(*mag_idx)
            )
        })
        .collect()
}

fn amulet_mismatches(cases: &[AmuletCase]) -> Vec<String> {
    cases
        .iter()
        .filter_map(|c| {
            let got = check_amulet(c.armringl, c.bujuk, c.count, c.amulet_type);
            let want = if c.ok { Some(c.idx as u8) } else { None };
            if got != want {
                Some(format!(
                    "{}: rust={got:?} csharp={want:?} (count={} type={})",
                    c.id, c.count, c.amulet_type
                ))
            } else {
                None
            }
        })
        .collect()
}

// ---- 用例 ---------------------------------------------------------------------------------------

#[test]
fn warr_skill_matches_csharp() {
    let v = load_vectors();
    assert_eq!(
        v.warr.len() as i32,
        v.warr_skill_sweep_max + 1,
        "warr 全表行数应与 sweep_max 一致"
    );
    let mismatches = warr_mismatches(&v.warr);
    assert!(
        mismatches.is_empty(),
        "战士技能表不等价:\n{}",
        mismatches.join("\n")
    );
    let truthy: Vec<i32> = v.warr.iter().filter(|(_, t)| *t).map(|(m, _)| *m).collect();
    assert_eq!(
        truthy,
        vec![3, 4, 7, 12, 25, 26, 27, 34, 38],
        "oracle 判为战士技能的魔号集合应恰为 MagicConst 的 9 个"
    );
}

#[test]
fn check_amulet_matches_csharp() {
    let v = load_vectors();
    assert!(v.amulet.len() >= 20, "护身符场景矩阵应覆盖 ≥20 条");
    let mismatches = amulet_mismatches(&v.amulet);
    assert!(
        mismatches.is_empty(),
        "护身符前置不等价:\n{}",
        mismatches.join("\n")
    );
    // 结果必须真的非平凡：既有命中也有未命中，且两个槽位都被覆盖过。
    assert!(v.amulet.iter().any(|c| c.ok), "矩阵应含命中场景");
    assert!(v.amulet.iter().any(|c| !c.ok), "矩阵应含未命中场景");
    assert!(v
        .amulet
        .iter()
        .any(|c| c.idx == i64::from(ITEM_LOCATION_ARM_RINGL)));
    assert!(v
        .amulet
        .iter()
        .any(|c| c.idx == i64::from(ITEM_LOCATION_BUJUK)));
}

#[test]
fn amulet_requirement_is_source_cited_table() {
    // 逐条 = MagicManager.cs 的 CheckAmulet 调用点（见 magic.rs 注释）。
    assert_eq!(amulet_requirement(6), Some((1, 2))); // 施毒术 :229
    for id in [13, 14, 15, 16, 17, 18, 19] {
        assert_eq!(amulet_requirement(id), Some((1, 1)), "魔号 {id} :323");
    }
    assert_eq!(amulet_requirement(30), Some((5, 1))); // 召唤神兽 :452
    assert_eq!(amulet_requirement(41), Some((2, 1))); // 天使 :464
    assert_eq!(amulet_requirement(36), Some((1, 1))); // 无极真气 :620
    assert_eq!(amulet_requirement(48), Some((1, 2))); // 群体施毒术 :908
    assert_eq!(amulet_requirement(1), None, "火球术不需护身符");
    // 需求数量必须落在场景矩阵覆盖过的 (count,type) 组合里（否则该表未被 oracle 行为背书）。
    let v = load_vectors();
    for id in 0..=110 {
        if let Some((count, ty)) = amulet_requirement(id) {
            assert!(
                v.amulet
                    .iter()
                    .any(|c| c.count == count && c.amulet_type == ty),
                "魔号 {id} 的需求 {count}/{ty} 不在探针矩阵里"
            );
        }
    }
}

#[test]
fn spell_in_range_uses_oracle_rage() {
    let v = load_vectors();
    let r = v.magic_attack_rage;
    assert_eq!(
        r, 8,
        "oracle MagicAttackRage 默认值应为 8（GameSvrConf.cs:1752）"
    );
    assert!(
        v.magic_hit_interval_time > 0,
        "oracle MagicHitIntervalTime 应为正数（冷却口径的加项，见第一片）"
    );
    // 边界：恰好等于 rage 通过；超 1 格即失败；正负方向对称。
    assert!(spell_in_range(0, 0, r as i16, r as i16, r));
    assert!(!spell_in_range(0, 0, (r + 1) as i16, 0, r));
    assert!(!spell_in_range(0, 0, 0, (r + 1) as i16, r));
    assert!(!spell_in_range(0, 0, -(r + 1) as i16, 0, r));
    assert!(spell_in_range(10, 10, 10 - r as i16, 10 + r as i16, r));
    assert!(!spell_in_range(0, 0, 0, 0, -1), "rage<0 时任意位移都失败");
}

#[test]
fn spell_version_gate_source_cited() {
    // `(SoftVersionDateEx == 0 && ClientTick == 0) || MagicId > 80` ⇒ false。
    assert!(!spell_version_gate(0, 0, 1), "双 0 ⇒ 拒");
    assert!(spell_version_gate(1, 0, 1), "版本非 0 ⇒ 过");
    assert!(spell_version_gate(0, 1, 1), "tick 非 0 ⇒ 过");
    assert!(!spell_version_gate(1, 1, 81), "魔号 > 80 ⇒ 拒");
    assert!(spell_version_gate(1, 1, 80), "魔号 80 ⇒ 过");
    assert!(!spell_version_gate(0, 0, 81), "双 0 + 魔号 > 80 ⇒ 拒");
}

// ---- 红检（证明比较函数有牙齿）------------------------------------------------------------------

/// 红检①：改坏 `warr` 全表里**一个数** ⇒ 战士技能对拍必 RED。
#[test]
fn redcheck_corrupt_one_warr_result_must_fail() {
    let v = load_vectors();
    assert!(
        warr_mismatches(&v.warr).is_empty(),
        "前提：未改坏前必须全等"
    );
    let mut corrupt = v.warr.clone();
    corrupt[0].1 = !corrupt[0].1; // mag_idx=0 本应 false
    assert!(
        !warr_mismatches(&corrupt).is_empty(),
        "红检失效：翻转 mag_idx=0 的判定后仍判等"
    );
}

/// 红检②：改坏护身符矩阵里**一个结果** ⇒ 该场景必 RED。
#[test]
fn redcheck_corrupt_one_amulet_result_must_fail() {
    let v = load_vectors();
    assert!(
        amulet_mismatches(&v.amulet).is_empty(),
        "前提：未改坏前必须全等"
    );
    let mut corrupt = v.amulet.clone();
    let row = corrupt
        .iter_mut()
        .find(|c| c.ok && c.idx == i64::from(ITEM_LOCATION_ARM_RINGL))
        .expect("矩阵里应有 ArmRingl 命中场景");
    row.ok = false;
    assert!(
        !amulet_mismatches(&corrupt).is_empty(),
        "红检失效：把护身符命中场景改成未命中后仍判等"
    );
}

/// 红检③：改坏护身符的**持久**输入一个数（把命中边界推下去）⇒ 该场景必 RED。
#[test]
fn redcheck_corrupt_one_amulet_durability_must_fail() {
    let v = load_vectors();
    let mut corrupt = v.amulet.clone();
    let row = corrupt
        .iter_mut()
        .find(|c| c.ok && c.armringl.map(|s| s.shape == 5) == Some(true))
        .expect("矩阵里应有 type1 Shape==5 的命中场景");
    let slot = row.armringl.as_mut().unwrap();
    slot.dura = 0; // 持久归零后 oracle 必不命中
    assert!(
        !amulet_mismatches(&corrupt).is_empty(),
        "红检失效：把护身符持久清零后仍判等"
    );
}
