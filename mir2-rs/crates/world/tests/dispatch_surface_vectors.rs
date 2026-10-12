//! 第十片（DoD③ 第五轴）：`DoSpell` 之外是否另有**魔号效果分派器**——机器判据。
//!
//! 向量来源：`mir2-rs/tests/parity/m3/dispatch-audit-probe`（C#，直读 oracle 源做分派面扫描 +
//! 反射直调 oracle 私有 `PlayObject.DoSpell` / public static `MagicManager.DoSpell`）
//! → `dispatch-audit-probe/vectors/dispatch_audit.{json,tsv}`。
//!
//! **为什么读 TSV 而不是 JSON**：与 `combat_skill_vectors.rs` 同理——`crates/world` 的依赖面
//! 被 `tests/no_parallel.rs` 钉成「内部 crate + bevy_ecs/bevy_app」，加 `serde_json` 会被判红。
//!
//! ## 本片回答的问题
//!
//! `skills_registry` 的 `no_switch_case`（20 行，55–80）与 `magicid_reject`（35 行，>80）
//! 承接原因都写着「待确认是否有第二分派器」。本片把它钉成机器判据：
//!
//! 1. **分派面清单**：`src/M2Server` 内以魔号为键的 `switch` 恰 5 处、`if (…==…)` 比较恰 4 处；
//!    逐站点 `(kind, file, line, case 数)` 与钉死值一致（源漂移 ⇒ 红）。
//! 2. **桶 ∩ case 标签值**：`no_switch_case ∪ magicid_reject` 的 55 个魔号里**只有 56
//!    （`SKILL_REDBANWOL`）** 在别处有 `case`——`PlayObject.Attack.cs:1152` 的攻击路径开关
//!    （`RedHalfMoonOnOff` + `SendSocket("+WID")`）；其余 54 个在 M2Server 内无任何 case。
//! 3. **通用路径真身向量**：55 行 + 8 行 `warr_reject` 对照逐行核对
//!    `(is_warr, spell_point, MP 前后, 返回, 自身出帧)`；Rust 侧用既有原语
//!    `is_warr_skill` / `spell_version_gate` / `get_spell_point` 复算并逐字段比对。
//!
//! 真身环境说明（见探针头注释）：`Envir` 为空图 + 新构造 `PlayObject` 处于隐藏模式
//! （`PlayObject.cs:993`）⇒ `SendRefMsg` 走 `BaseObject.Message.cs:390` 的自身分支，
//! 帧落自身 `MsgQueue`。**帧的 ident/参数是 oracle 原值；收件人集合不在本判据内。**

use std::collections::BTreeSet;
use std::path::PathBuf;

use mir2_data::models::MagicInfo;
use mir2_world::magic::{get_spell_point, is_warr_skill, spell_version_gate};

// ---- 钉死的 oracle 分派面（与探针 `PinnedSites`/`PinnedIdCmps` 同源）---------------------------
const PINNED_SITES: &[(&str, &str, i64, i64)] = &[
    (
        "gate_is_warr_skill",
        "src/M2Server/Magic/MagicManager.cs",
        85,
        9,
    ),
    (
        "effect_dospell_main",
        "src/M2Server/Magic/MagicManager.cs",
        172,
        45,
    ),
    (
        "effect_dospell_inner_amulet",
        "src/M2Server/Magic/MagicManager.cs",
        326,
        7,
    ),
    (
        "attack_skill_toggle",
        "src/M2Server/Player/PlayObject.Attack.cs",
        1118,
        8,
    ),
    (
        "passive_recalc_hitspeed",
        "src/M2Server/Player/PlayObject.cs",
        3685,
        4,
    ),
];

const PINNED_ID_CMPS: &[(&str, &str, i64)] = &[
    (
        "lookup_attack_magic",
        "src/M2Server/Player/PlayObject.Attack.cs",
        345,
    ),
    (
        "lookup_change_magic_key",
        "src/M2Server/Player/PlayObject.Operate.cs",
        779,
    ),
    (
        "lookup_get_magic_info",
        "src/M2Server/Player/PlayObject.cs",
        4179,
    ),
    (
        "cond_see_heal_gauge",
        "src/M2Server/Player/PlayObject.cs",
        5233,
    ),
];

/// 55 个目标桶魔号里唯一在别处有 `case` 的例外：56 `SKILL_REDBANWOL`（攻击路径开关）。
const REDBANWOL_HIT: (&str, i64, &str, i64) = (
    "src/M2Server/Player/PlayObject.Attack.cs",
    1152,
    "SKILL_REDBANWOL",
    56,
);

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

// ---- 向量装载 ---------------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq)]
struct SiteRow {
    kind: String,
    file: String,
    line: i64,
    case_count: i64,
    subject: String,
}

#[derive(Debug, Clone, PartialEq)]
struct IdCmpRow {
    kind: String,
    file: String,
    line: i64,
    subject: String,
}

#[derive(Debug, Clone, PartialEq)]
struct CaseIdRow {
    file: String,
    line: i64,
    name: String,
    value: i64,
}

#[derive(Debug, Clone, PartialEq)]
struct GenericRow {
    row_index: i64,
    magic_id: i64,
    name: String,
    class: String,
    is_warr: bool,
    spell_point: i64,
    mp_before: i64,
    mp_after: i64,
    result: bool,
    direct_result: bool,
    direct_exception: String,
    msg_count: i64,
    msg: String,
}

#[derive(Debug, Clone)]
struct Audit {
    sites: Vec<SiteRow>,
    id_cmps: Vec<IdCmpRow>,
    case_ids: Vec<CaseIdRow>,
    bucket_case_hits: Vec<CaseIdRow>,
    generic: Vec<GenericRow>,
}

fn load_audit() -> Audit {
    let lines = read_lines("dispatch-audit-probe/vectors/dispatch_audit.tsv");
    let mut a = Audit {
        sites: Vec::new(),
        id_cmps: Vec::new(),
        case_ids: Vec::new(),
        bucket_case_hits: Vec::new(),
        generic: Vec::new(),
    };
    for fields in &lines {
        match fields[0].as_str() {
            "cfg" => {}
            "site" => a.sites.push(SiteRow {
                kind: fields[1].clone(),
                file: fields[2].clone(),
                line: i(fields, 3),
                case_count: i(fields, 4),
                subject: fields[5].clone(),
            }),
            "idcmp" => a.id_cmps.push(IdCmpRow {
                kind: fields[1].clone(),
                file: fields[2].clone(),
                line: i(fields, 3),
                subject: fields[4].clone(),
            }),
            "caseid" => a.case_ids.push(CaseIdRow {
                file: fields[1].clone(),
                line: i(fields, 2),
                name: fields[3].clone(),
                value: i(fields, 4),
            }),
            "bucketcase" => a.bucket_case_hits.push(CaseIdRow {
                file: fields[1].clone(),
                line: i(fields, 2),
                name: fields[3].clone(),
                value: i(fields, 4),
            }),
            "generic" => a.generic.push(GenericRow {
                row_index: i(fields, 1),
                magic_id: i(fields, 2),
                name: fields[3].clone(),
                class: fields[4].clone(),
                is_warr: i(fields, 5) != 0,
                spell_point: i(fields, 6),
                mp_before: i(fields, 7),
                mp_after: i(fields, 8),
                result: i(fields, 9) != 0,
                direct_result: i(fields, 10) != 0,
                direct_exception: fields[11].clone(),
                msg_count: i(fields, 12),
                msg: fields[13].clone(),
            }),
            other => panic!("未知的 dispatch_audit.tsv 行类型 {other:?}"),
        }
    }
    a
}

/// `skill-probe/vectors/skill_vectors.tsv` 的 `reg` 行（装表顺序）。
fn registry_rows() -> Vec<(i64, i64, String, String)> {
    let lines = read_lines("skill-probe/vectors/skill_vectors.tsv");
    let mut rows = Vec::new();
    let mut idx = 0;
    for fields in &lines {
        if fields[0] != "reg" {
            continue;
        }
        idx += 1;
        rows.push((idx, i(fields, 1), fields[2].clone(), fields[8].clone()));
    }
    rows
}

fn bucket_row_indices(class: &str) -> Vec<i64> {
    registry_rows()
        .into_iter()
        .filter(|(_, _, _, c)| c == class)
        .map(|(row_index, _, _, _)| row_index)
        .collect()
}

fn bucket_ids(class: &str) -> BTreeSet<i64> {
    registry_rows()
        .into_iter()
        .filter(|(_, _, _, c)| c == class)
        .map(|(_, id, _, _)| id)
        .collect()
}

// ---- 判据 A：分派面钉死（源漂移 ⇒ 红）---------------------------------------------------------
#[test]
fn dispatch_surface_matches_pinned_inventory() {
    let a = load_audit();
    assert_eq!(
        a.sites.len(),
        PINNED_SITES.len(),
        "M2Server 魔号 switch 站点数漂移"
    );
    for (row, (kind, file, line, cases)) in a.sites.iter().zip(PINNED_SITES.iter()) {
        assert_eq!(
            (&row.kind, &row.file, row.line, row.case_count),
            (&kind.to_string(), &file.to_string(), *line, *cases),
            "分派站点漂移（子句: {}）",
            row.subject
        );
    }
    // 站点语义自检：主 switch 的 subject 与：85/:3685/:1118 的 subject 各不相同。
    assert_eq!(a.sites[0].subject, "switch (wMagIdx)");
    assert_eq!(a.sites[1].subject, "switch (userMagic.Magic.MagicId)");
    assert_eq!(a.sites[2].subject, "switch (userMagic.Magic.MagicId)");
    assert_eq!(a.sites[3].subject, "switch (UserMagic.MagIdx)");
    assert_eq!(a.sites[4].subject, "switch (userMagic.MagIdx)");

    assert_eq!(
        a.id_cmps.len(),
        PINNED_ID_CMPS.len(),
        "M2Server 魔号比较站点数漂移"
    );
    for (row, (kind, file, line)) in a.id_cmps.iter().zip(PINNED_ID_CMPS.iter()) {
        assert_eq!(
            (&row.kind, &row.file, row.line),
            (&kind.to_string(), &file.to_string(), *line),
            "魔号比较站点漂移（子句: {}）",
            row.subject
        );
    }
}

// ---- 判据 B：桶 ∩ case 标签值 = {56} ---------------------------------------------------------
#[test]
fn only_red_banwol_of_the_buckets_has_a_second_dispatcher() {
    let a = load_audit();
    let bucket: BTreeSet<i64> = bucket_ids("no_switch_case")
        .union(&bucket_ids("magicid_reject"))
        .copied()
        .collect();
    // 行数（content 冻结：20 + 35 = 55 行；其中重号行使 distinct 魔号少于行数）。
    assert_eq!(bucket_row_indices("no_switch_case").len(), 20);
    assert_eq!(bucket_row_indices("magicid_reject").len(), 35);
    assert_eq!(
        bucket.len(),
        43,
        "目标桶 distinct 魔号应为 43（55 行中 12 行为重号）"
    );

    // oracle 源里所有 `case MagicConst.<name>` 的数值集合。
    let case_values: BTreeSet<i64> = a.case_ids.iter().map(|c| c.value).collect();
    let hits: Vec<i64> = case_values.intersection(&bucket).copied().collect();
    assert_eq!(
        hits,
        vec![56],
        "目标桶里只允许 56 在别处有 case（实得 {hits:?}）"
    );

    // 探针显式登记的命中项必须恰为该处。
    assert_eq!(a.bucket_case_hits.len(), 1, "桶命中项应恰 1 条");
    let hit = &a.bucket_case_hits[0];
    assert_eq!(
        (hit.file.as_str(), hit.line, hit.name.as_str(), hit.value),
        REDBANWOL_HIT,
        "56 的第二分派器锚点漂移"
    );
    // 56 的反面：55 与 57 不在 case 集合里（防止把整桶误报为“都有 case”）。
    assert!(!case_values.contains(&55));
    assert!(!case_values.contains(&57));
    // 8 个 warr_reject 行则**都**应在 case 集合里（IsWarrSkill/攻击开关/被动加成）。
    assert_eq!(bucket_row_indices("warr_reject").len(), 8);
    for id in &bucket_ids("warr_reject") {
        assert!(
            case_values.contains(id),
            "warr_reject 魔号 {id} 应在 case 集合里"
        );
    }
}

// ---- 判据 C：通用路径真身向量逐字段 -----------------------------------------------------------
fn expected_frames(row: &GenericRow) -> (i64, String) {
    if row.is_warr {
        return (0, String::new());
    }
    let spell = format!("10008:0:0:{};10052:0:0:0", row.magic_id);
    match row.class.as_str() {
        "magicid_reject" => (2, spell),
        "no_switch_case" => (3, format!("{spell};10120:257:0:0")),
        other => panic!("generic 行出现未登记的类别 {other}"),
    }
}

fn expected_result(row: &GenericRow) -> bool {
    // 逐行复刻 oracle 决策树：warr 门 → 距离门（探针恒过）→ 版本门 → switch。
    if row.is_warr {
        return false;
    }
    if !spell_version_gate(1, 1, row.magic_id as i32) {
        return false; // MagicId > 80 ⇒ :167 早退
    }
    // 通过版本门后：没有 case 的魔号（no_switch_case）落到共享尾 `return true`；
    // 桶内其余（magicid_reject）不可能到达这里。
    row.class == "no_switch_case"
}

#[test]
fn generic_path_matches_oracle_vectors() {
    let a = load_audit();
    let mut expected_rows: Vec<i64> = bucket_row_indices("no_switch_case");
    expected_rows.extend(bucket_row_indices("magicid_reject"));
    expected_rows.extend(bucket_row_indices("warr_reject"));
    expected_rows.sort_unstable();
    assert_eq!(expected_rows.len(), 63, "覆盖行数应为 20 + 35 + 8 = 63");

    let got_rows: Vec<i64> = a.generic.iter().map(|r| r.row_index).collect();
    assert_eq!(
        got_rows, expected_rows,
        "generic 向量覆盖行与登记类别不一致"
    );
    assert_eq!(a.generic.len(), 63, "generic 行数应为 63（含重复魔号行）");

    let probe_magic = MagicInfo {
        spell: 20,
        def_spell: 0,
        ..Default::default()
    };

    for row in &a.generic {
        let label = format!("id={} {} ({})", row.magic_id, row.name, row.class);
        assert_eq!(
            row.spell_point,
            i64::from(get_spell_point(&probe_magic, 0)),
            "{label}: GetSpellPoint 与 Rust `get_spell_point` 不一致"
        );
        assert_eq!(
            row.is_warr,
            is_warr_skill(row.magic_id as i32),
            "{label}: IsWarrSkill 与 Rust `is_warr_skill` 不一致"
        );
        assert_eq!(
            row.result,
            expected_result(row),
            "{label}: 返回与复算不一致"
        );
        assert_eq!(
            row.direct_result, row.result,
            "{label}: 直调与包装返回不一致"
        );
        assert_eq!(row.direct_exception, "", "{label}: 直调不应抛异常");

        let want_mp_after = if row.is_warr || row.spell_point == 0 {
            row.mp_before
        } else {
            row.mp_before - row.spell_point
        };
        assert_eq!(row.mp_after, want_mp_after, "{label}: MP 扣减不一致");

        let (want_count, want_msg) = expected_frames(row);
        assert_eq!(row.msg_count, want_count, "{label}: 自身出帧数不一致");
        assert_eq!(row.msg, want_msg, "{label}: 自身出帧内容/顺序不一致");
    }

    // 非平凡断言：三类各自至少一条，且三类结果/帧确实不同（不是同一个值被抄三遍）。
    assert!(a
        .generic
        .iter()
        .any(|r| r.class == "no_switch_case" && r.result));
    assert!(a
        .generic
        .iter()
        .any(|r| r.class == "magicid_reject" && !r.result));
    assert!(a
        .generic
        .iter()
        .any(|r| r.class == "warr_reject" && !r.result && r.msg_count == 0));
}

// ---- 判据 D：改坏必红（4 条）-----------------------------------------------------------------
fn generic_mismatches(rows: &[GenericRow]) -> Vec<String> {
    let mut out = Vec::new();
    for row in rows {
        let label = format!("id={} {}", row.magic_id, row.name);
        if row.result != expected_result(row) {
            out.push(format!(
                "{label}: 返回不等 got={} want={}",
                row.result,
                expected_result(row)
            ));
        }
        let (want_count, want_msg) = expected_frames(row);
        if row.msg != want_msg || row.msg_count != want_count {
            out.push(format!("{label}: 帧不等 got={} want={}", row.msg, want_msg));
        }
        if row.spell_point != 5 {
            out.push(format!(
                "{label}: spell_point 不等 got={} want=5",
                row.spell_point
            ));
        }
    }
    out
}

fn site_mismatches(sites: &[SiteRow]) -> Vec<String> {
    let mut out = Vec::new();
    if sites.len() != PINNED_SITES.len() {
        out.push(format!("站点数 {} != {}", sites.len(), PINNED_SITES.len()));
    }
    for (row, (kind, file, line, cases)) in sites.iter().zip(PINNED_SITES.iter()) {
        if row.kind != *kind || row.file != *file || row.line != *line || row.case_count != *cases {
            out.push(format!("站点漂移 {}:{}", row.file, row.line));
        }
    }
    out
}

fn bucket_case_hit_mismatches(hits: &[CaseIdRow]) -> Vec<String> {
    if hits.len() != 1 {
        return vec![format!("桶命中项应恰 1 条，实得 {}", hits.len())];
    }
    let h = &hits[0];
    if (h.file.as_str(), h.line, h.name.as_str(), h.value) != REDBANWOL_HIT {
        return vec![format!("桶命中锚点漂移 {}({})", h.file, h.line)];
    }
    Vec::new()
}

#[test]
fn redcheck_corrupt_one_generic_result_must_fail() {
    let mut a = load_audit();
    a.generic[0].result = !a.generic[0].result;
    assert!(
        !generic_mismatches(&a.generic).is_empty(),
        "改坏一条返回必须判红"
    );
}

#[test]
fn redcheck_corrupt_one_spell_point_must_fail() {
    let mut a = load_audit();
    a.generic[10].spell_point += 1;
    assert!(
        !generic_mismatches(&a.generic).is_empty(),
        "改坏一条 spell_point 必须判红"
    );
}

#[test]
fn redcheck_extra_dispatcher_site_must_fail() {
    let mut a = load_audit();
    a.sites.push(SiteRow {
        kind: "rogue".to_string(),
        file: "src/M2Server/Player/PlayObject.cs".to_string(),
        line: 1,
        case_count: 1,
        subject: "switch (magic.MagicId)".to_string(),
    });
    assert!(
        !site_mismatches(&a.sites).is_empty(),
        "多出一个分派站点必须判红"
    );
}

#[test]
fn redcheck_unregistered_bucket_case_hit_must_fail() {
    let mut a = load_audit();
    a.bucket_case_hits.push(CaseIdRow {
        file: "src/M2Server/Magic/MagicManager.cs".to_string(),
        line: 1,
        name: "SKILL_XX".to_string(),
        value: 57,
    });
    assert!(
        !bucket_case_hit_mismatches(&a.bucket_case_hits).is_empty(),
        "未登记的桶命中必须判红"
    );
}
