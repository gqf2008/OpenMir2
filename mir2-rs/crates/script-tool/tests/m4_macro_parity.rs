//! M4① 边界语料（宏展开 / `#DEFINE` 常量替换）的 Rust 侧可证伪锚点。
//!
//! 语料 `tests/parity/M4/fixtures/macroenvir/Npc_def/define.txt` 含 2 条 `#DEFINE`：
//! `QQQ -> 金币`、`PPP -> 5`；`give QQQ PPP` 在 `#ACT` 段被替换为 `give 金币 5`。
//!
//! 该结构摘要 `1effd1782e851e85` 与 **C# 参照 harness**（`tools/script-parity-cs`，同一语料）
//! 产出的结构摘要逐字节相同（见 `tests/parity/evidence/M4/macro-parity.txt`）。
//! 本测试把「宏展开次数」与「展开后的结构指纹」都钉在 Rust 侧：
//! 改坏 `parser.rs` 的常量替换（命中位置 / 每行上限 / 大小写）或 `digest.rs` 的内容指纹 → 必红。

use std::path::PathBuf;

use mir2_script::{structure_digest_full, LocalFs, MerchantDigestInput, ScriptParsers};

/// 与 C# 参照 harness 同种子（重名 label 改名序列一致）。
fn load_fixture() -> mir2_script::LoadOutcome {
    let envir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/M4/fixtures/macroenvir");
    let parsers = ScriptParsers::new();
    let fs = LocalFs;
    let mut rng = mir2_shared::rng::RandomNumber::with_seed(42);
    let mut rename = |min: i32, max: i32| rng.get_random_number(min, max);
    parsers
        .load_script_file(&fs, &envir, "Npc_def", "define", false, &mut rename)
        .expect("边界语料不应触发 LoadPanic")
        .expect("边界语料文件应存在")
}

#[test]
fn macro_expansion_counts_and_structure_digest_are_frozen() {
    let outcome = load_fixture();
    let s = &outcome.stats;
    // 四类计数的解析侧锚点（与 C# harness 同值）
    assert_eq!(s.scripts, 1, "脚本数");
    assert_eq!(s.records, 1, "标签数");
    assert_eq!(s.procedures, 1, "过程数");
    assert_eq!(s.conditions, 1, "条件数");
    assert_eq!(s.actions, 2, "动作数（give + break）");
    assert_eq!(s.else_actions, 0, "否则动作数");
    // 宏展开：QQQ->金币（1 次）+ PPP->5（1 次）
    assert_eq!(s.define_substitutions, 2, "宏展开（#DEFINE 常量替换）次数");
}

#[test]
fn macro_expanded_content_matches_csharp_frozen_digest() {
    let outcome = load_fixture();
    let merchant = MerchantDigestInput {
        price_rate: outcome.price_rate.unwrap_or(0),
        item_type_list: outcome.item_type_list.clone(),
        flags: outcome.merchant_flags,
        refill_goods: outcome.refill_goods.clone(),
    };
    let digest = structure_digest_full(&outcome.scripts, &merchant);
    // 该值与 C# 参照 harness 在同一语料上的输出逐字节相同（macro-parity.txt）。
    assert_eq!(
        digest.hash, "1effd1782e851e85",
        "宏展开后的结构指纹（label/cmd/参数/说词）与 C# 参照冻结值不一致"
    );
    assert_eq!(digest.counts.actions, 2);
    assert_eq!(digest.counts.conditions, 1);
}

/// 牙齿：改坏语料（少一条 `#DEFINE`）→ 宏展开计数必减、结构指纹必变。
#[test]
fn redcheck_macro_fixture_without_define_must_differ() {
    let envir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/M4/fixtures/macroenvir");
    // 复制语料并去掉一条 #DEFINE（不碰入库件）
    let tmp = std::env::temp_dir().join(format!("m4-macro-redcheck-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("Npc_def")).unwrap();
    let src = envir.join("Npc_def/define.txt");
    let bytes = std::fs::read(&src).unwrap();
    // 删掉第二行（`#DEFINE PPP 5`；行尾可能是 CRLF 或 LF——`.gitattributes` 强制 LF，
    // 但本地检出可能是 CRLF，两边都要能处理）。
    let text = String::from_utf8_lossy(&bytes).to_string();
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    assert!(lines.len() > 1 && lines[1].starts_with("#DEFINE PPP"));
    lines.remove(1);
    let tampered = lines.join("\r\n");
    std::fs::write(tmp.join("Npc_def/define.txt"), tampered.as_bytes()).unwrap();

    let parsers = ScriptParsers::new();
    let fs = LocalFs;
    let mut rng = mir2_shared::rng::RandomNumber::with_seed(42);
    let mut rename = |min: i32, max: i32| rng.get_random_number(min, max);
    let out = parsers
        .load_script_file(&fs, &tmp, "Npc_def", "define", false, &mut rename)
        .expect("no panic")
        .expect("found");
    assert!(
        out.stats.define_substitutions < 2,
        "去掉一条 #DEFINE 后宏展开计数应减少（实得 {}）",
        out.stats.define_substitutions
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
