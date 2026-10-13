//! 卡 `openmir2-m3-quest-persist-observability`（M3⑥-实现 4）判据：**固化 oracle
//! 「任务态不落库」语义 + 门禁**。
//!
//! oracle（`src/`，实测行号）：
//!
//! | 事实 | oracle 锚 |
//! | --- | --- |
//! | MySQL 存档 `UPDATE characters SET …` 的**列清单无任务列** | `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:81-86` |
//! | `SaveQuest` **只 `DELETE FROM characters_quest`，从不 INSERT** | `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:493`（DELETE `:495`） |
//! | `SaveQuest` **全仓无调用点**（唯一下文是自身的异常日志 `:505`） | 同上 |
//! | MySQL 读档 `GetChrRecord` = `SELECT * FROM characters WHERE ID=@ID`，**不读任务数组** | `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs:192`（SQL `:194`） |
//! | 任务数组只走 **6000 面会话记录**（`QuestFlag/QuestUnit/QuestUnitOpen = new byte[128]`） | `src/OpenMir2/Packets/ServerPackets/PlayerDataInfo.cs:101-103`（构造 `:112-114`） |
//!
//! 结论：**oracle 下任务标志只存在于会话内 `CharacterData`，下线即丢**；这不是缺口，是参照
//! 实现的行为。本门禁把「两侧都不写 `characters_quest`、`characters` 无任务列」钉死，防止
//! 后续有人在 Rust 侧偷偷加一条任务落库路径（规格变更必须 owner 签字另立卡）。
//!
//! 断言分层，任一被破坏即红：
//!
//! 1. **源码扫描（扫代码 + SQL 字符串，跳过注释）**：`mir2-rs/**/*.rs` 不得出现表名
//!    `characters_quest`（当前全仓 0 引用）；`bins/db-srv/src/playdata.rs` 不得出现任何任务列
//!    （`QUESTFLAG`/`QUESTUNIT`/`QUESTUNITOPEN`/`QUESTOPENINDEX`/`QUESTFININDEX`）。
//!
//! - **工作区级 SQL 字面量扫描（⑥-4 M-1 收口，reviewer-5 `3dc49a83`）**：任何 `.rs` 的
//!   **SQL 字符串字面量**（含 `INSERT`/`UPDATE`/`DELETE`/`SELECT`）都不得提及任务列/表——
//!   表名已由 1 的全仓扫描覆盖，五个任务列在这里工作区级钉死（原实现只在
//!   `bins/db-srv/src/playdata.rs` 单文件断言，换到别的 crate/别的表就会漏）。判定只作用于
//!   字符串字面量，故展示串（如 `expect("QuestFlag")`）与注释不误报。
//! 2. **正面控制**：`SAVE_CHARACTERS_SQL` 必须真的 `UPDATE characters SET … WHERE ID = ?`
//!    （防止「删掉常量」让扫描恒绿），且 `playdata.rs` 必须真有 `SELECT * FROM characters WHERE ID=?`
//!    读档路径。
//! 3. **会话态仍在**：「不落库」≠「删掉会话任务态」——`crates/script` 的 `QuestBitmap`（3×byte[128]）
//!    与 `crates/storage::playerdata` 的 `quest_flag/quest_unit/quest_unit_open` 必须还在。
//! 4. **已捕获的两侧观测**：M1 跨读基线的 C#/Rust 两侧 `mir2_db.characters_quest.rows` 必须都为 **0**
//!    （真机两相快照，见 `mir2-rs/tests/parity/evidence/M1/s4b-slice4-crossread/`）。
//!
//! 真机复现（本卡 `DoD` 的进程级判据）需要独占窗口：任务交互后两侧 `characters_quest = 0 行` +
//! `characters` 无任务列变更（`run_pair.ps1` + `scoped_diff.py --canonical`，`characters_quest`
//! 已在 `scoped_diff.py` 的 `TABLES` 作用域内）。本片离线门禁覆盖其静态/证据半边。

use std::fs;
use std::path::{Path, PathBuf};

/// `mir2-rs` 工作区根（本测试 = `<root>/bins/db-srv/tests/quest_no_persist.rs`）。
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// 任务列/表名（小写匹配）。`characters_quest` 是 oracle 里唯一涉及任务态的表。
const QUEST_TOKENS: [&str; 6] = [
    "characters_quest",
    "questflag",
    "questunit",
    "questunitopen",
    "questopenindex",
    "questfinindex",
];

const TABLE_TOKEN: &str = "characters_quest";

/// SQL 语句关键字（小写）。字面量须**同时**含其一与某个任务 token 才算命中，
/// 这样 `expect("QuestFlag")` 这类展示串不会被判红。
const SQL_KEYWORDS: [&str; 4] = ["insert", "update", "delete", "select"];

/// **只去注释、保留字符串**：SQL 是字符串字面量，必须留在扫描面里；
/// 但注释里写 `characters_quest` 只删不写不得误报（这是 `no_parallel` 踩过的坑）。
fn strip_comments_keep_strings(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && chars.peek() == Some(&'/') {
            for sc in chars.by_ref() {
                if sc == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut prev = '\0';
            for sc in chars.by_ref() {
                if sc == '\n' {
                    out.push('\n');
                }
                if prev == '*' && sc == '/' {
                    break;
                }
                prev = sc;
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 命中的任务列/表名（对去注释后的代码做小写子串匹配）。
fn quest_tokens_in(code: &str) -> Vec<&'static str> {
    let lower = code.to_lowercase();
    QUEST_TOKENS
        .iter()
        .copied()
        .filter(|t| lower.contains(t))
        .collect()
}

/// 从**去注释后的源码**里抽出字符串字面量内容（标准串 + 原始串 `r"…"`/`r#"…"#`）。
/// 目的：把「不得把任务列写进 SQL」限定在**字符串字面量**上，让展示串与注释不误报。
fn string_literals(code: &str) -> Vec<String> {
    let chars: Vec<char> = code.chars().collect();
    let mut out = Vec::new();
    let mut idx = 0usize;
    while idx < chars.len() {
        let cur = chars[idx];
        // 原始串：r"…" / r#"…"# / r##"…"##（转义不生效）。
        if cur == 'r' {
            let mut after_r = idx + 1;
            let mut hashes = 0usize;
            while after_r < chars.len() && chars[after_r] == '#' {
                hashes += 1;
                after_r += 1;
            }
            if after_r < chars.len() && chars[after_r] == '"' {
                let mut scan = after_r + 1;
                let mut buf = String::new();
                while scan < chars.len() {
                    if chars[scan] == '"' {
                        let mut matched = 0usize;
                        while matched < hashes
                            && scan + 1 + matched < chars.len()
                            && chars[scan + 1 + matched] == '#'
                        {
                            matched += 1;
                        }
                        if matched == hashes {
                            scan += 1 + hashes;
                            break;
                        }
                    }
                    buf.push(chars[scan]);
                    scan += 1;
                }
                out.push(buf);
                idx = scan;
                continue;
            }
        }
        // 字符字面量：跳过，避免 `'"'` 里的引号被当成字符串起点；生命周期 `'a` 不在此列。
        if cur == '\'' {
            let is_char_lit =
                chars.get(idx + 1) == Some(&'\\') || chars.get(idx + 2) == Some(&'\'');
            if is_char_lit {
                let mut scan = idx + 1;
                while scan < chars.len() {
                    if chars[scan] == '\\' {
                        scan += 2;
                        continue;
                    }
                    if chars[scan] == '\'' {
                        scan += 1;
                        break;
                    }
                    scan += 1;
                }
                idx = scan;
                continue;
            }
        }
        if cur == '"' {
            let mut scan = idx + 1;
            let mut buf = String::new();
            while scan < chars.len() {
                let ch = chars[scan];
                if ch == '\\' {
                    if let Some(next) = chars.get(scan + 1) {
                        buf.push(*next);
                        scan += 2;
                        continue;
                    }
                }
                if ch == '"' {
                    scan += 1;
                    break;
                }
                buf.push(ch);
                scan += 1;
            }
            out.push(buf);
            idx = scan;
            continue;
        }
        idx += 1;
    }
    out
}

/// SQL 字面量里提及的任务 token（`(字面量, token)`）——字面量须同时含一个 SQL 关键字。
fn sql_quest_hits(code: &str) -> Vec<(String, &'static str)> {
    let mut hits = Vec::new();
    for lit in string_literals(code) {
        let lower = lit.to_lowercase();
        if !SQL_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
            continue;
        }
        for t in QUEST_TOKENS {
            if lower.contains(t) {
                hits.push((lit.clone(), t));
            }
        }
    }
    hits
}

/// 收集一个目录下所有 `.rs`（递归；跳过 `target`/`.git`）。
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if name == "target" || name == ".git" {
                continue;
            }
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn read_rel(root: &Path, rel: &str) -> String {
    let p = root.join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {} 失败: {e}", p.display()))
}

/// ③ 源码扫描：`mir2-rs` 下任何 `.rs` 都不得引用 `characters_quest`（含 SQL 字符串）。
#[test]
fn workspace_rust_sources_never_reference_characters_quest() {
    let root = workspace_root();
    assert!(root.is_dir(), "工作区根不存在: {}", root.display());
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    assert!(
        files.len() > 50,
        "只扫到 {} 个 .rs——路径漂移会让门禁恒绿，fail-closed",
        files.len()
    );

    let mut hits = Vec::new();
    for f in &files {
        // 本门禁自身以字符串字面量列出被禁表名/任务列（扫描规则 + 红检夹具），必须跳过它自己，
        // 否则会把自己判红。只跳这一个文件，不放松对其余源码的扫描。
        if f.ends_with("tests/quest_no_persist.rs") || f.ends_with("tests\\quest_no_persist.rs") {
            continue;
        }
        let raw = fs::read_to_string(f).unwrap_or_default();
        let code = strip_comments_keep_strings(&raw).to_lowercase();
        if code.contains(TABLE_TOKEN) {
            hits.push(f.clone());
        }
    }
    assert!(
        hits.is_empty(),
        "Rust 侧不得引用 `characters_quest`（oracle 该表只删不写、无调用点；落库属规格变更，需 owner 另立卡）；命中: {hits:#?}"
    );
}

/// ①b 工作区级 SQL 字面量扫描：`mir2-rs` 下任何 `.rs` 的 SQL 字面量不得提及任务列/表。
///
/// ⑥-4 M-1（reviewer-5 `3dc49a83`）收口：原实现只在 `bins/db-srv/src/playdata.rs` 单文件断言
/// 五个任务列，漏『别的 crate 把任务列写进别的表』。这里把扫描面扩到**全仓 SQL 字面量**，
/// 与表名全文件扫描互补（表名 ⊆ 任务 token，故本用例同时也扫到表名，保留显式断言作双证）。
#[test]
fn workspace_sql_literals_never_mention_quest_columns() {
    let root = workspace_root();
    assert!(root.is_dir(), "工作区根不存在: {}", root.display());
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    assert!(
        files.len() > 50,
        "只扫到 {} 个 .rs——路径漂移会让门禁恒绿，fail-closed",
        files.len()
    );

    let mut hits = Vec::new();
    for f in &files {
        // 同 ③：只跳过门禁自身（它以字符串字面量列出扫描规则/红检夹具）。
        if f.ends_with("tests/quest_no_persist.rs") || f.ends_with("tests\\quest_no_persist.rs") {
            continue;
        }
        let raw = fs::read_to_string(f).unwrap_or_default();
        let code = strip_comments_keep_strings(&raw);
        for (lit, token) in sql_quest_hits(&code) {
            hits.push(format!("{} :: `{}`（token `{token}`）", f.display(), lit));
        }
    }
    assert!(
        hits.is_empty(),
        "工作区任何 SQL 字面量都不得提及任务列/表（oracle 只把任务态放在会话内 6000 面，\
         落库属规格变更，需 owner 另立卡）；命中: {hits:#?}"
    );
}

/// ①② db-srv 存档/读档面不得出现任务列，且存档 SQL 必须仍是 `UPDATE characters SET …`。
#[test]
fn db_srv_save_and_load_sql_have_no_quest_columns() {
    let root = workspace_root();
    let code = strip_comments_keep_strings(&read_rel(&root, "bins/db-srv/src/playdata.rs"));
    let lower = code.to_lowercase();

    // ② 正面控制：SQL 常量没被删、存档目标仍是 characters，且带 WHERE ID 占位符。
    assert!(
        lower.contains("update characters set"),
        "`SAVE_CHARACTERS_SQL` 必须仍更新 `characters` 表（防门禁被『删常量』绕过）"
    );
    assert!(
        lower.contains("where id = ?"),
        "`SAVE_CHARACTERS_SQL` 必须仍有 `WHERE ID = ?`"
    );
    assert!(
        lower.contains("gold = ?"),
        "`SAVE_CHARACTERS_SQL` 应含非任务列（如 `Gold = ?`）做正面控制"
    );
    assert!(
        lower.contains("select * from characters where id=?"),
        "`GetChrRecord` 的读档路径必须仍是 `SELECT * FROM characters WHERE ID=?`（oracle Query.cs:192/194）"
    );

    // ① 反面：存档/读档代码里不得出现任务列。
    let hits = quest_tokens_in(&code);
    assert!(
        hits.is_empty(),
        "bins/db-srv/src/playdata.rs 不得出现任务列/表 {hits:?}——oracle 的 `characters` UPDATE 列清单无任务列，\
         任务数组只走 6000 面会话记录（PlayerDataInfo.cs:101-103）"
    );
}

/// ③ 正面控制：「不落库」不等于「删掉会话任务态」——会话侧建模必须在。
#[test]
fn session_quest_state_model_is_still_present() {
    let root = workspace_root();
    let qs = read_rel(&root, "crates/script/src/engine/quest_state.rs");
    assert!(
        qs.contains("pub struct QuestBitmap") && qs.contains("QUEST_ARRAY_LEN"),
        "crates/script 必须仍持有会话任务态 `QuestBitmap`（3×byte[128]）——不落库的对象是 DB，不是会话态"
    );

    let recorder = read_rel(&root, "crates/storage/src/playerdata.rs");
    for field in ["quest_flag", "quest_unit", "quest_unit_open"] {
        assert!(
            recorder.contains(field),
            "6000 面会话记录必须仍编解码 `{field}`（PlayerDataInfo.cs:101-103）"
        );
    }

    let npc = read_rel(&root, "bins/game-svr/src/npc.rs");
    assert!(
        npc.contains("QuestBitmap"),
        "bins/game-svr 的脚本玩家必须仍持有会话任务态（`QuestBitmap`）"
    );
}

/// ④ 已捕获的两侧观测：C# / Rust 两侧 `mir2_db.characters_quest` 均为 0 行。
#[test]
fn captured_cross_side_baselines_show_zero_characters_quest_rows() {
    let root = workspace_root();
    let cases = [
        (
            "csharp",
            "tests/parity/evidence/M1/s4b-slice4-crossread/03-csharp-dbsrv-baseline.json",
        ),
        (
            "rust",
            "tests/parity/evidence/M1/s4b-slice4-crossread/04-rust-dbsrv-baseline.json",
        ),
    ];
    for (side, rel) in cases {
        let p = root.join(rel);
        let text = fs::read_to_string(&p).unwrap_or_else(|e| {
            panic!(
                "缺 {side} 侧 M1 跨读基线 {}（门禁 fail-closed，勿让路径漂移变恒绿）: {e}",
                p.display()
            )
        });
        let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .unwrap_or_else(|e| panic!("{} 非法 JSON: {e}", p.display()));
        let rows = v["dbsnap"]["mir2_db.characters_quest"]["rows"]
            .as_i64()
            .unwrap_or_else(|| {
                panic!("{side} 基线缺 `dbsnap[\"mir2_db.characters_quest\"][\"rows\"]`")
            });
        assert_eq!(
            rows, 0,
            "{side} 侧 `mir2_db.characters_quest` 必须为 0 行（oracle MySQL 不持久任务态）"
        );
        if let Some(delta) = v
            .get("delta")
            .and_then(|d| d.get("mir2_db.characters_quest"))
        {
            let added = delta
                .get("added")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            let removed = delta
                .get("removed")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            assert_eq!(
                (added, removed),
                (0, 0),
                "{side} 侧任务交互不得改动 `characters_quest` 行"
            );
        }
    }
}

/// 自动红检：扫描器必须能咬住注入的任务落库，且注释里的字样不得误报。
#[test]
fn redcheck_scanner_bites_injected_quest_persistence() {
    let injected =
        "async fn f() { sqlx::query(\"DELETE FROM characters_quest WHERE PlayerId=?\").execute(&mut c).await; }";
    assert!(
        quest_tokens_in(&strip_comments_keep_strings(injected)).contains(&"characters_quest"),
        "红检失效：注入的 `characters_quest` 写入未被识别"
    );

    let injected_col =
        "const S: &str = \"UPDATE characters SET QuestFlag = ?, Gold = ? WHERE ID = ?\";";
    assert!(
        !quest_tokens_in(&strip_comments_keep_strings(injected_col)).is_empty(),
        "红检失效：注入的任务列未被识别"
    );

    let benign =
        "// oracle: characters_quest 只删不写、无调用点（Save.cs:493）\nlet s = \"quest-free\";\n";
    let code = strip_comments_keep_strings(benign);
    assert!(
        quest_tokens_in(&code).is_empty(),
        "注释里的 `characters_quest` 被误判为代码：{}",
        code.trim()
    );
}

/// 自动红检：工作区级 SQL 扫描必须咬住注入的任务列/表，且展示串/非任务 SQL 不误报。
#[test]
fn redcheck_sql_scanner_bites_injected_quest_columns() {
    // [1] 注入到**别的表**的任务列：旧门禁（只禁 `characters_quest`）漏，本门禁必须咬住。
    let injected_other_table =
        "const S: &str = \"UPDATE some_other_table SET QuestFlag = ?, Gold = ? WHERE ID = ?\";";
    let hits = sql_quest_hits(&strip_comments_keep_strings(injected_other_table));
    assert!(
        hits.iter().any(|(_, t)| *t == "questflag"),
        "红检失效：别的表里的任务列未被 SQL 扫描咬住（本门禁的目标场景）"
    );

    // [2] 任务表名：字面量里出现 ⇒ 两条扫描都被咬住。
    let injected_table = "sqlx::query(\"DELETE FROM characters_quest WHERE PlayerId = ?\");";
    let stripped = strip_comments_keep_strings(injected_table);
    assert!(
        stripped.to_lowercase().contains(TABLE_TOKEN),
        "红检失效：全文件扫描未咬住注入的 `characters_quest`"
    );
    assert!(
        sql_quest_hits(&stripped)
            .iter()
            .any(|(_, t)| *t == TABLE_TOKEN),
        "红检失效：SQL 字面量扫描未咬住注入的 `characters_quest`"
    );

    // [3] 展示串（非 SQL）+ 注释里的任务字样不得误报（本仓真实形态：record.rs expect("QuestFlag")）。
    let benign = "let qf = d.quest_flag.as_ref().expect(\"QuestFlag\");\n\
                  // 会话态: QuestUnit / QuestUnitOpen / QuestFlag\n\
                  let s = \"quest-free\";\n";
    let code = strip_comments_keep_strings(benign);
    assert!(
        sql_quest_hits(&code).is_empty(),
        "展示串/注释被误判为 SQL 落库: {:?}",
        sql_quest_hits(&code)
    );

    // [4] 不含任务列的其他 SQL（真实 `SAVE_CHARACTERS_SQL` 形态）不得误报。
    let other_sql =
        "const S: &str = \"UPDATE characters SET Gold = ?, BodyLuck = ? WHERE ID = ?\";";
    assert!(
        sql_quest_hits(&strip_comments_keep_strings(other_sql)).is_empty(),
        "非任务列 SQL 被误报: {:?}",
        sql_quest_hits(&strip_comments_keep_strings(other_sql))
    );

    // [5] 原始串也在扫描面内（`r#"..."#` 里写 SQL）。
    let raw_sql = "const S: &str = r#\"UPDATE characters SET QuestUnit = ? WHERE ID = ?\"#;";
    assert!(
        sql_quest_hits(&strip_comments_keep_strings(raw_sql))
            .iter()
            .any(|(_, t)| *t == "questunit"),
        "红检失效：原始串里的任务列未被 SQL 扫描咬住"
    );
}
