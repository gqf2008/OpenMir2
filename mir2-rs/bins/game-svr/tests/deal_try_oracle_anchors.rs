//! oracle 锚漂移门（M3⑦-3 · M-1 收口 doc-Nit，卡 `openmir2-m3-party-deal-try-null-pose`）。
//!
//! `ClientDealTry` 路径的内联注释行号必须与 `src/M2Server/Player/PlayObject.Operate.cs`
//! 实测行一致，且 catch 归属 = `ProcessPlayObjectData`。
//!
//! 来源：reviewer-5 `3eb4a6a05` 的 M-1（注释写 `:929`/`:934`/`:955-958`，实为
//! `:933`/`:936`/`:958-960`）与 M-2（catch 归属措辞）。M-2 经复核为**误判**：
//! `WorldServer.cs` 的 `ProcessPlayObjectData` 方法体为 `:625-737`、其 `catch` 在
//! `:732-735`；`:734` 的日志串写作 `WorldServer::ProcessHumans` 是 oracle 自身的复制粘贴
//! 痕迹，不改变包围方法名。本门同时锁死这两点。
//!
//! O-1 收口（reviewer-5 `494ba02e5`）：Rust 侧断言面不再对**整份文件**做 `contains`，而是
//! 收窄到 **deal_try 相关注释块**（块内须含 deal_try 语义标记）——别处注释/字符串出现同串时
//! 既不假绿也不假红；标记漂移时正锚断言 fail-closed 必红。

use std::fs;
use std::path::PathBuf;

/// 仓库根（本测试 = `<repo>/mir2-rs/bins/game-svr/tests/deal_try_oracle_anchors.rs`）。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {} 失败: {e}", p.display()))
}

/// deal_try 注释块的语义标记（小写子串；任一命中即视该注释块为 deal_try 相关）。
const DEAL_TRY_COMMENT_MARKERS: [&str; 6] = [
    "clientdealtry",
    "deal_try",
    "getposecreate",
    "poseobject",
    "processplayobjectdata",
    "dealtry",
];

/// 抽出源码中 **deal_try 相关注释块** 的文本（reviewer-5 `494ba02e5` O-1 收口）。
///
/// 规则：只取**整行注释**（`trim_start` 后以 `//` 起头的连续行成块），块内（小写）含
/// [`DEAL_TRY_COMMENT_MARKERS`] 任一标记才计入。这样注释锚断言既不会命中代码/字符串里的
/// 同串，也不会被别处无关注释的 `:929` 之类误判。
///
/// fail-closed：标记若被改名/删走，返回文本将缺锚 ⇒ 调用方正锚断言必红（不会静默恒绿）。
fn deal_try_comment_bytes(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("//") {
            let start = i;
            while i < lines.len() && lines[i].trim_start().starts_with("//") {
                i += 1;
            }
            let block = lines[start..i].join("\n");
            let lower = block.to_lowercase();
            if DEAL_TRY_COMMENT_MARKERS.iter().any(|m| lower.contains(m)) {
                out.push_str(&block);
                out.push('\n');
            }
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn deal_try_oracle_anchors_match_source() {
    // ① oracle 侧：注释声称的每个行号都指向声称的语句（oracle 冻结，行号即契约）。
    let oracle = read("src/M2Server/Player/PlayObject.Operate.cs");
    let lines: Vec<&str> = oracle.lines().collect();
    let at = |n: usize| -> String {
        lines
            .get(n - 1)
            .unwrap_or_else(|| panic!("oracle PlayObject.Operate.cs 第 {n} 行不存在"))
            .trim()
            .to_string()
    };
    for (n, needle) in [
        (932usize, "GetPoseCreate()"),
        (933, "poseObject.Race"),
        (936, "targetPlayObject != this"),
        (938, "GetPoseCreate() == this"),
        (954, "SM_DEALTRY_FAIL"),
        (958, "else"),
        (960, "SM_DEALTRY_FAIL"),
    ] {
        assert!(
            at(n).contains(needle),
            "oracle `PlayObject.Operate.cs:{n}` 应为含 `{needle}` 的语句，实得 `{}`",
            at(n)
        );
    }

    // ② Rust 侧：deal_try 注释必须引用 ① 的行号，且不得回退到旧漂移号码。
    //    O-1：断言面收窄到 deal_try 相关注释块（见 `deal_try_comment_bytes`）。
    let trade = deal_try_comment_bytes(&read("mir2-rs/bins/game-svr/src/trade.rs"));
    let main = deal_try_comment_bytes(&read("mir2-rs/bins/game-svr/src/lib.rs"));
    assert!(
        trade.len() > 200 && main.len() > 200,
        "deal_try 注释块提取过短（trade.rs={} B, lib.rs={} B）——标记漂移会让门禁退化，fail-closed",
        trade.len(),
        main.len()
    );
    for (name, src) in [("trade.rs", &trade), ("lib.rs", &main)] {
        for good in [":932", ":933", ":936", ":938", ":958-960"] {
            assert!(
                src.contains(good),
                "{name} 的 deal_try 注释缺 oracle 锚 `{good}`（reviewer-5 M-1 doc 漂移回归）"
            );
        }
        for bad in [":929", ":928-929", ":934", ":955-958"] {
            assert!(
                !src.contains(bad),
                "{name} 仍残留旧漂移锚 `{bad}`（应为 :932/:933/:936/:938/:958-960）"
            );
        }
        assert!(
            src.contains("WorldServer.cs:732-735"),
            "{name} 的 deal_try 注释应引 `WorldServer.cs:732-735`（catch 站点）"
        );
        // ③ M-2 归属：catch 属 `ProcessPlayObjectData`（方法体 :625-737），不是 `ProcessHumans`。
        assert!(
            src.contains("WorldServer.ProcessPlayObjectData"),
            "{name} 的 deal_try 注释应把 catch 归属写为 `WorldServer.ProcessPlayObjectData`"
        );
    }
}

/// 自动红检（O-1）：注释锚断言必须**只在 deal_try 注释块内**生效。
#[test]
fn redcheck_anchor_scope_is_deal_try_only() {
    // [1] 无关注释里的旧锚 `:929` 不得被当成 deal_try 的锚（不假红/不假绿）。
    let unrelated = "// 别的特性：Operate.cs:929 是另一条路径\nfn f() {}\n";
    assert!(
        !deal_try_comment_bytes(unrelated).contains(":929"),
        "无关注释被误计入 deal_try 断言面: {:?}",
        deal_try_comment_bytes(unrelated)
    );

    // [2] 代码/字符串里的同串不得被计入（原实现整文件 `contains` 会命中）。
    let in_code = "fn deal_try_target_of() { let _s = \":929\"; }\n";
    assert!(
        !deal_try_comment_bytes(in_code).contains(":929"),
        "代码/字符串里的 `:929` 被误计入: {:?}",
        deal_try_comment_bytes(in_code)
    );

    // [3] deal_try 注释块内的回退锚必须被咬住（正锚与反锚都要看得到）。
    let scoped_regression =
        "// oracle `GetPoseCreate()`；旧锚回退 `:929` 与 `:955-958`\nfn deal_try_target_of() {}\n";
    let scoped = deal_try_comment_bytes(scoped_regression);
    assert!(
        scoped.contains(":929") && scoped.contains(":955-958"),
        "deal_try 注释块内的回退锚未被提取，反锚断言将形同虚设: {scoped:?}"
    );

    // [4] 无标记的注释块整体不计入（fail-closed 边界：标记漂移 ⇒ 正锚断言自然红）。
    let unmarked = "// Operate.cs:932-960 但没有任何组队语义标记\n";
    assert!(
        deal_try_comment_bytes(unmarked).is_empty(),
        "无 deal_try 标记的注释块不应计入: {:?}",
        deal_try_comment_bytes(unmarked)
    );
}
