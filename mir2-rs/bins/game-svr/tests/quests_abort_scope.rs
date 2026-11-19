//! 中止范围对拍（卡 `openmir2-quests-parity` §10.7 ①）：
//! 清单里混入一条会让 oracle 解析器越界的坏脚本时，**两侧最终可用的前缀集合必须逐条相等**
//! （含失败脚本身份）。
//!
//! 金标准 = oracle 探针 `--abort-scope` 产出的
//! `mir2-rs/tests/parity/m3/quests/vectors/quest_abort_scope_ledger.tsv`（真
//! `ScriptParsers.LoadScriptFile`、2 好 1 坏夹具、单一顶层 catch 只记录不逐条吞）。
//! Rust 侧走**生产函数** `NpcHost::load`（同一批夹具、同一顺序），断言：
//! `NpcLoadAbort.partial.defs`（前缀）逐条相等、`failing.chr_name` 相等、异常类型同源。
//!
//! 防空转：缺金标准/缺夹具 ⇒ panic 且带 PREREQ（Z2）；空账本 ⇒ 红（Z1）；
//! 前缀全空 ⇒ 红（「整批中止」与「全都没装」不可区分，Z3/Z4 同族）。

use std::path::PathBuf;

use game_svr::npc::NpcHost;
use mir2_envir::mapquest::QuestNpcRecord;

struct Row {
    case_id: String,
    scripts: Vec<String>,
    loaded: Vec<String>,
    failing: String,
    exception: String,
}

/// 解析金标准（纯函数，Z1 直接喂退化输入）。
///
/// 空账本（只有注释/空行）⇒ `Err("EMPTY_LEDGER")`；列数 ≠ 5 ⇒ `Err`。
fn parse_ledger(text: &str) -> Result<Row, String> {
    let rows: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .collect();
    if rows.is_empty() {
        return Err("EMPTY_LEDGER".to_string());
    }
    if rows.len() != 1 {
        return Err(format!("ROWS={} != 1", rows.len()));
    }
    let f: Vec<&str> = rows[0].split('\t').collect();
    if f.len() != 5 {
        return Err(format!("COLS={} != 5: {rows:?}", f.len()));
    }
    let split = |v: &str| -> Vec<String> {
        if v == "-" {
            Vec::new()
        } else {
            v.split(',').map(str::to_string).collect()
        }
    };
    Ok(Row {
        case_id: f[0].to_string(),
        scripts: split(f[1]),
        loaded: split(f[2]),
        failing: f[3].to_string(),
        exception: f[4].to_string(),
    })
}

fn quests_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/m3/quests")
}

#[test]
fn empty_ledger_is_red() {
    assert!(parse_ledger("# 只有表头\n").is_err());
    assert!(parse_ledger("").is_err());
}

/// ①前缀集合逐条相等（含失败脚本名与异常类型）。
#[test]
fn abort_scope_prefix_set_matches_oracle_ledger() {
    let ledger_path = quests_root().join("vectors/quest_abort_scope_ledger.tsv");
    let text = std::fs::read_to_string(&ledger_path)
        .unwrap_or_else(|e| panic!("PREREQ 缺金标准 {ledger_path:?}: {e}"));
    let row = parse_ledger(&text).unwrap_or_else(|e| panic!("金标准不可用：{e}"));
    assert_eq!(row.scripts.len(), 3, "夹具必须是 2 好 1 坏");

    // 真文件路径 + 真解析器：把仓库夹具逐字拷进临时 Envir。
    let root = std::env::temp_dir().join(format!("gs-quest-abort-{}", std::process::id()));
    let dir = root.join("Envir").join("AbortScope");
    std::fs::create_dir_all(&dir).unwrap();
    let fixtures = quests_root().join("fixtures");
    for name in &row.scripts {
        let src = fixtures.join(format!("{name}.txt"));
        let dst = dir.join(format!("{name}.txt"));
        std::fs::copy(&src, &dst).unwrap_or_else(|e| panic!("PREREQ 缺夹具 {src:?}: {e}"));
    }
    let rec = |name: &str| QuestNpcRecord {
        map_name: "0".to_string(),
        curr_x: 0,
        curr_y: 0,
        chr_name: name.to_string(),
        npc_flag: 0,
        appr: 0,
        file_path: "AbortScope".to_string(),
        script_name: None,
        is_hide: true,
        is_quest: false,
    };
    let records: Vec<QuestNpcRecord> = row.scripts.iter().map(|n| rec(n)).collect();
    let mut rename = |_min: i32, _max: i32| 1;
    let abort = NpcHost::load(&root.join("Envir"), &records, &[], 40, &mut rename)
        .expect_err("坏脚本必须让整批中止（oracle 同输入异常穿透初始化链）");

    // ①-a 前缀集合逐条相等（顺序敏感：装载序 = oracle 的构造序）。
    let rust_loaded: Vec<&str> = abort
        .partial
        .defs
        .iter()
        .map(|d| d.chr_name.as_str())
        .collect();
    let golden_loaded: Vec<&str> = row.loaded.iter().map(String::as_str).collect();
    assert_eq!(
        rust_loaded, golden_loaded,
        "{}: 前缀集合必须与 oracle 逐条相等",
        row.case_id
    );

    // ①-b 失败脚本身份相等。
    assert_eq!(
        abort.failing.chr_name, row.failing,
        "{}: 失败脚本名",
        row.case_id
    );
    assert!(
        abort.failing.reason.contains(&row.exception),
        "{}: 异常类型必须同源（Rust 显示文本 {:?} 不含 {:?}）",
        row.case_id,
        abort.failing.reason,
        row.exception
    );

    // 防空转：前缀非空，且坏脚本之后的 good 不得装载。
    assert!(
        !rust_loaded.is_empty(),
        "前缀不得为空（否则与「全都没装」不可区分）"
    );
    assert_eq!(row.scripts.len(), 3);
    assert_eq!(
        row.scripts[1], row.failing,
        "中间那条必须是坏脚本（夹具口径）"
    );
    assert!(
        !rust_loaded.contains(&row.scripts[2].as_str()),
        "坏脚本之后的 good 不得装载"
    );

    std::fs::remove_dir_all(&root).ok();
}
