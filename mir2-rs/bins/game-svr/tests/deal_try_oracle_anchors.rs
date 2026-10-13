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
    let trade = read("mir2-rs/bins/game-svr/src/trade.rs");
    let main = read("mir2-rs/bins/game-svr/src/main.rs");
    for (name, src) in [("trade.rs", &trade), ("main.rs", &main)] {
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
