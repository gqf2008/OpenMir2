//! 卡 `openmir2-m3-quest-persist-observability`（M3⑥-实现 4）判据：**固化 oracle
//! 「任务态不落库」语义 + 门禁**。
//!
//! oracle（`src/`，实测行号）：
//!
//! | 事实 | oracle 锚 |
//! | --- | --- |
//! | MySQL 存档 `UPDATE characters SET …` 的**列清单无任务列** | `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:85-88`（`SaveRecord` `:82`，四条 `AppendLine` 自 `:85` 起，`WHERE ID` 在 `:88`） |
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
//! - **原始串定界（⑥-4 O-2 收口，reviewer-5 `4b283fad18`）**：注释剥离器按 Rust 字面量
//!   规则识别 `r"…"`/`r#"…"#`/`r##"…"##`、字符字面量 `'"'` 与嵌套块注释——原始串内
//!   含引号/`//`/`/*` 不再导致提前收尾而漏检（旧实现只认 `"` 串，§断言 1 的 §字符串面
//!   可能出现假绿）。
//! 2. **正面控制**：`SAVE_CHARACTERS_SQL` 必须真的 `UPDATE characters SET … WHERE ID = ?`
//!    （防止「删掉常量」让扫描恒绿），且 `playdata.rs` 必须真有 `SELECT * FROM characters WHERE ID=?`
//!    读档路径。
//! 3. **会话态仍在**：「不落库」≠「删掉会话任务态」——`crates/script` 的 `QuestBitmap`（3×byte[128]）
//!    与 `crates/storage::playerdata` 的 `quest_flag/quest_unit/quest_unit_open` 必须还在。
//! 4. **已捕获的两侧观测**：M1 跨读基线的 C#/Rust 两侧 `mir2_db.characters_quest.rows` 必须都为 **0**，
//!    且该表**不得进入基线写集差分 `delta`**（若出现则 `added`/`removed` 必须 0/0）；顶层 `delta`
//!    缺失即红（**fail-closed**，不得退化成恒绿的 `if let Some`）。（真机两相快照，见
//!    `mir2-rs/tests/parity/evidence/M1/s4b-slice4-crossread/`。）
//!
//! > 4 的写集差分半边是 reviewer-6 `0a4fee67` **M-1** 的收口：原实现写成
//! > `if let Some(delta) = v.get("delta").and_then(|d| d.get("mir2_db.characters_quest"))`，
//! > 而 M1 基线的 `delta` 只列 `added != 0 || removed != 0` 的表（`characters_quest` 0 行、未被
//! > 触碰），故该分支**从不执行**——断言名不副实。现改为 `quest_delta_violation`：缺 `delta`/
//! > 缺字段一律红，并对该子判据落 `redcheck_quest_delta_subassertion_bites` 强制其五态执行。
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
///
/// 字面量边界按 Rust 规则识别：普通串 `"…"`（含 `\` 转义）、原始串
/// `r"…"`/`r#"…"#`/`r##"…"##`（转义不生效，内部的引号与 `//`/`/*` 都是内容）、
/// 字符字面量 `'"'`，以及嵌套块注释。
///
/// **reviewer-5 `4b283fad18` O-2 收口**：旧实现只认 `"` 串，原始串内含引号时
/// 会在内层 `"` 处提前收尾，其后的 `//`/`/*` 被当注释剥掉 ⇒ 原始串里写的 SQL
/// 任务列被漏检（假绿）。现按原始串定界符（`#` 个数）吃到配平收尾。
fn strip_comments_keep_strings(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            i = skip_line_comment(&chars, i);
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i = skip_block_comment(&chars, i, &mut out);
        } else if c == 'r' && starts_raw_string(&chars, i) {
            i = copy_raw_string(&chars, i, &mut out);
        } else if c == '\'' && is_char_literal(&chars, i) {
            i = copy_char_literal(&chars, i, &mut out);
        } else if c == '"' {
            i = copy_normal_string(&chars, i, &mut out);
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// 行注释：丢弃到行尾（不消费换行，由调用方按普通字符保留）。
fn skip_line_comment(chars: &[char], i: usize) -> usize {
    let mut j = i + 2;
    while j < chars.len() && chars[j] != '\n' {
        j += 1;
    }
    j
}

/// 块注释：Rust 允许嵌套（`/* /* */ */`），按深度吃到配平的 `*/`；内部换行保留。
fn skip_block_comment(chars: &[char], i: usize, out: &mut String) -> usize {
    let mut depth = 1usize;
    let mut j = i + 2;
    while j < chars.len() && depth > 0 {
        if chars[j] == '\n' {
            out.push('\n');
        }
        if chars[j] == '/' && chars.get(j + 1) == Some(&'*') {
            depth += 1;
            j += 2;
        } else if chars[j] == '*' && chars.get(j + 1) == Some(&'/') {
            depth -= 1;
            j += 2;
        } else {
            j += 1;
        }
    }
    j
}

/// `i` 指向 `r` 时判定其是否为原始串起点（`r"…"` / `r#"…"#` / …）。
fn starts_raw_string(chars: &[char], i: usize) -> bool {
    let mut j = i + 1;
    while chars.get(j) == Some(&'#') {
        j += 1;
    }
    chars.get(j) == Some(&'"')
}

/// 复制一段原始串（含定界符）到 `out`，返回其后的下标；转义/注释符都是内容。
fn copy_raw_string(chars: &[char], i: usize, out: &mut String) -> usize {
    let mut hashes = 0usize;
    let mut j = i + 1;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    out.push('r');
    for _ in 0..hashes {
        out.push('#');
    }
    out.push('"');
    j += 1;
    loop {
        if j >= chars.len() {
            break;
        }
        if chars[j] == '"' {
            let mut matched = 0usize;
            while matched < hashes && chars.get(j + 1 + matched) == Some(&'#') {
                matched += 1;
            }
            if matched == hashes {
                out.push('"');
                for _ in 0..hashes {
                    out.push('#');
                }
                j += 1 + hashes;
                break;
            }
        }
        out.push(chars[j]);
        j += 1;
    }
    j
}

/// 字符字面量 `'"'` / `'\''`：整段原样保留，避免其中的引号被当作串起点。
fn copy_char_literal(chars: &[char], i: usize, out: &mut String) -> usize {
    let mut j = i + 1;
    out.push('\'');
    while j < chars.len() {
        out.push(chars[j]);
        if chars[j] == '\\' {
            if let Some(n) = chars.get(j + 1) {
                out.push(*n);
                j += 2;
                continue;
            }
        }
        if chars[j] == '\'' {
            j += 1;
            break;
        }
        j += 1;
    }
    j
}

/// 普通字符串 `"…"`：整段原样保留（`\` 转义，含 `\"`）。
fn copy_normal_string(chars: &[char], i: usize, out: &mut String) -> usize {
    out.push('"');
    let mut j = i + 1;
    while j < chars.len() {
        out.push(chars[j]);
        if chars[j] == '\\' {
            if let Some(n) = chars.get(j + 1) {
                out.push(*n);
                j += 2;
                continue;
            }
        }
        if chars[j] == '"' {
            j += 1;
            break;
        }
        j += 1;
    }
    j
}

/// 判定 `'` 处是否为字符字面量（`'x'` / `'\n'`），而非生命周期（`'a`/`'static`）。
/// 与 `string_literals` 的口径一致。
fn is_char_literal(chars: &[char], idx: usize) -> bool {
    chars.get(idx + 1) == Some(&'\\') || chars.get(idx + 2) == Some(&'\'')
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

/// ④ 的写集差分子判据：M1 基线 `delta`（本次运行 `dbsnap_after - dbsnap` 的逐表差分）中
/// `mir2_db.characters_quest` 必须**不存在**（该表 0 行、未被触碰，故不进入写集）；若存在，
/// `added`/`removed` 必须都是 0。
///
/// **fail-closed**：`delta` 缺失、非对象、或写集条目字段缺失/非整数一律返 `Err`，绝不放行——
/// 否则就退回成 reviewer-6 `0a4fee67` M-1 指出的恒绿 `if let Some`（断言名不副实）。
fn quest_delta_violation(v: &serde_json::Value) -> Result<(), String> {
    const TABLE: &str = "mir2_db.characters_quest";
    let delta = v
        .get("delta")
        .ok_or_else(|| "基线缺顶层 `delta`（写集差分）字段".to_string())?;
    let delta = delta
        .as_object()
        .ok_or_else(|| "基线 `delta` 不是对象".to_string())?;
    let Some(entry) = delta.get(TABLE) else {
        return Ok(()); // 未进入写集 ⇒ 本次运行没有改动该表（M1 基线真实形态）
    };
    let added = entry
        .get("added")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| format!("`delta[\"{TABLE}\"].added` 缺失或非整数"))?;
    let removed = entry
        .get("removed")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| format!("`delta[\"{TABLE}\"].removed` 缺失或非整数"))?;
    if (added, removed) != (0, 0) {
        return Err(format!(
            "任务交互改动了 `{TABLE}` 行（added={added} removed={removed}）"
        ));
    }
    Ok(())
}

/// ④ 已捕获的两侧观测：C# / Rust 两侧 `mir2_db.characters_quest` 均为 0 行、且未进入写集差分。
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
        quest_delta_violation(&v).unwrap_or_else(|e| {
            panic!("{side} 侧 M1 基线写集差分子判据失败（fail-closed，不得恒绿）: {e}")
        });
    }
}

/// 自动红检：④ 的写集差分子判据必须**真的执行**且 fail-closed——用合成基线覆盖
/// 「净改动 ⇒ 红 / 显式 0/0 ⇒ 绿 / 未进写集 ⇒ 绿 / `delta` 缺失 ⇒ 红 / 条目缺字段 ⇒ 红」
/// 五态，防止再退回惰性分支（reviewer-6 `0a4fee67` M-1）。
#[test]
fn redcheck_quest_delta_subassertion_bites() {
    use serde_json::json;

    let dirty = json!({"delta": {"mir2_db.characters_quest": {"added": 1, "removed": 0}}});
    assert!(
        quest_delta_violation(&dirty).is_err(),
        "改坏必红失效：非零 added 的任务表写集差分被放行"
    );

    let clean = json!({"delta": {"mir2_db.characters_quest": {"added": 0, "removed": 0}}});
    assert!(
        quest_delta_violation(&clean).is_ok(),
        "净零差分被误判为改动：{:?}",
        quest_delta_violation(&clean)
    );

    let absent = json!({"delta": {"mir2_db.characters": {"added": 1, "removed": 0}}});
    assert!(
        quest_delta_violation(&absent).is_ok(),
        "未进入写集的表被误判为改动：{:?}",
        quest_delta_violation(&absent)
    );

    let no_delta = json!({"dbsnap": {}});
    assert!(
        quest_delta_violation(&no_delta).is_err(),
        "fail-closed 失效：缺顶层 `delta` 的基线被放行（恒绿漏洞未堵）"
    );

    let malformed = json!({"delta": {"mir2_db.characters_quest": {"added": 0}}});
    assert!(
        quest_delta_violation(&malformed).is_err(),
        "fail-closed 失效：写集条目缺 `removed` 被放行"
    );
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

/// 自动红检（reviewer-5 `4b283fad18` O-2）：注释剥离器必须识别 Rust **原始串**的全部
/// 定界形态。旧实现只把 `"` 当串边界，原始串内含引号时会在内层 `"` 提前收尾，其后的
/// `//`/`/*` 被当注释剥掉 ⇒ 原始串里写的 SQL 任务列被漏检（假绿）。
#[test]
fn redcheck_comment_stripper_handles_raw_strings_with_comment_markers() {
    // [1] 行注释符 `//` 在原始串内（且在**奇数个**内层引号之后）不得吞掉串尾 SQL：
    // 内层引号数为偶时旧实现也会因重新开串而碰巧保住内容，必须用奇数引号才构成区分度。
    let line = "const S: &str = r#\"say \" // UPDATE characters SET QuestFlag = ?\"#;";
    let hits = sql_quest_hits(&strip_comments_keep_strings(line));
    assert!(
        hits.iter().any(|(_, t)| *t == "questflag"),
        "红检失效：原始串内引号后的 `//` 把 SQL 任务列吞掉了（O-2 漏检面）: {hits:?}"
    );

    // [2] 块注释符 `/* */` 同理（同样是奇数内层引号）。
    let block = "const S: &str = r#\"say \" /* UPDATE characters SET QuestUnit = ? */\"#;";
    let hits = sql_quest_hits(&strip_comments_keep_strings(block));
    assert!(
        hits.iter().any(|(_, t)| *t == "questunit"),
        "红检失效：原始串内引号后的 `/* */` 把 SQL 任务列吞掉了: {hits:?}"
    );

    // [3] 多 `#` 定界的原始串（`#` 个数即收尾定界）整段保留；同样用奇数内层引号。
    let multi = "const S: &str = r##\"a \" // DELETE FROM characters_quest\"##;";
    let stripped = strip_comments_keep_strings(multi).to_lowercase();
    assert!(
        stripped.contains(TABLE_TOKEN),
        "红检失效：r##\"…\"## 里的表名被剥掉"
    );

    // [4] 对照：真正的行注释仍被剥掉（收紧不能以放松注释剥离为代价）。
    let commented = "// r#\"UPDATE characters SET QuestFlag = ?\"# 只是注释\nlet x = 1;";
    assert!(
        sql_quest_hits(&strip_comments_keep_strings(commented)).is_empty(),
        "对照失效：注释里的任务列被误报"
    );
}
