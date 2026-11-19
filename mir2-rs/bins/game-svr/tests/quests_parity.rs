//! QT-05..09 动作级对拍（卡 `openmir2-quests-parity`）。
//!
//! 金标准 = **oracle 生成**的 `mir2-rs/tests/parity/m3/quests/vectors/quest_action_ledger.tsv`
//! （`quest-parity-probe --actions`：真 `ScriptEngine.GotoLable` + 真解析器 + 真 `PlayObject` 位图）。
//! 本用例用**同一段脚本正文**在 Rust 侧重放（真 `ScriptParsers` + 真引擎 + 产品 `NpcPlayer.QuestBitmap`），
//! 逐个 case 比三张位图与「解析出的动作条数」。
//!
//! 防空转：缺金标准 ⇒ panic 且消息带 PREREQ（Z2）；空账本 / 0 case ⇒ 红（Z1/Z3）；
//! 全零互证 ⇒ 红（Z4，至少要求某 case 的位图非零）。

use std::path::{Path, PathBuf};

use game_svr::npc::{NpcDef, NpcHost, NpcPlayer};
use mir2_script::{split_lines, ScriptFs, ScriptParsers};

struct MemFs(Vec<(String, String)>);

impl ScriptFs for MemFs {
    fn load_string_list(&self, path: &Path) -> Option<Vec<String>> {
        let key = path.to_string_lossy().replace('\\', "/").to_lowercase();
        self.0
            .iter()
            .find(|(k, _)| k.to_lowercase() == key)
            .map(|(_, v)| split_lines(v))
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 用真解析器 + 真引擎跑一段 `#ACT` 正文，返回（动作条数, 三张位图十六进制）。
fn run_body(body: &str, seed: &str) -> Result<(usize, String, String, String), String> {
    let text = format!("[@main]\n#ACT\n{body}\n");
    let fs = MemFs(vec![("root/npc_def/t.txt".to_string(), text)]);
    let parsers = ScriptParsers::new();
    let mut rename = |_min: i32, _max: i32| 0;
    let out = match parsers.load_script_file(
        &fs,
        Path::new("root"),
        "Npc_def",
        "t",
        false,
        &mut rename,
    ) {
        Ok(Some(o)) => o,
        Ok(None) => return Err("PREREQ 脚本缺失".to_string()),
        Err(p) => return Err(format!("{p}")),
    };
    let actions: usize = out
        .scripts
        .iter()
        .flat_map(|s| s.record_list.iter())
        .flat_map(|r| r.procedure_list.iter())
        .map(|p| p.action_list.len())
        .sum();

    let def = NpcDef {
        chr_name: "T".to_string(),
        file_path: "Npc_def".to_string(),
        script_name: None,
        is_hide: false,
        is_quest: false,
        scripts: out.scripts,
        loaded: true,
        actor_id: 1,
    };
    let host = NpcHost {
        envir_root: PathBuf::from("root"),
        defs: vec![std::rc::Rc::new(def)],
        failures: Vec::new(),
        goto_count_limit: 40,
    };
    let mut rng1 = |_v: i32| 0;
    let mut player = NpcPlayer::empty(&mut rng1);
    for part in seed.split(',').filter(|p| !p.is_empty()) {
        match part.parse::<i32>() {
            Ok(f) => player.quest_flag.set(f, 1),
            Err(_) => return Err(format!("坏 seed: {part}")),
        }
    }
    let _ = host.goto_label("T", &mut player, "@main", false);

    Ok((
        actions,
        hex(player.quest_flag.as_bytes()),
        hex(player.quest_unit.as_bytes()),
        hex(player.quest_unit_open.as_bytes()),
    ))
}

fn ledger_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/parity/m3/quests/vectors/quest_action_ledger.tsv")
}

/// 比较器本体（纯函数，便于 Z1/Z4 直接喂退化输入）。
/// 返回（行数, 不一致明细）。空输入 ⇒ 明细里点名 EMPTY_LEDGER。
fn compare(ledger: &str) -> (usize, Vec<String>) {
    let mut rows = 0usize;
    let mut bad = Vec::new();
    for line in ledger.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 10 {
            bad.push(format!("列数 {} != 10: {line}", f.len()));
            continue;
        }
        let case_id = f[0];
        let parsed: usize = match f[1].parse() {
            Ok(v) => v,
            Err(_) => {
                bad.push(format!("{case_id}: actions_parsed 不是整数: {}", f[1]));
                continue;
            }
        };
        let body = f[8].replace("\\n", "\n");
        let oracle_err = f[6] != "-";
        let seed = if f[9] == "-" {
            String::new()
        } else {
            f[9].to_string()
        };
        let (got_actions, got_flag, got_unit, got_open) = match run_body(&body, &seed) {
            Ok(v) => {
                if oracle_err {
                    bad.push(format!("{case_id}: 金标准侧解析失败 {}，Rust 侧成功", f[6]));
                    rows += 1;
                    continue;
                }
                v
            }
            Err(e) => {
                if !oracle_err {
                    bad.push(format!("{case_id}: Rust 解析失败 {e}，金标准侧正常"));
                } else if !e.contains("IndexOutOfRangeException") {
                    bad.push(format!("{case_id}: 失败类型 {e} 与金标准 {} 不一致", f[6]));
                }
                rows += 1;
                continue;
            }
        };
        if got_actions != parsed {
            bad.push(format!(
                "{case_id}: 解析动作条数 {got_actions} != 金标准 {parsed}（script={body:?}）"
            ));
        }
        if got_flag != f[3] {
            bad.push(format!("{case_id}: flag {got_flag} != 金标准 {}", f[3]));
        }
        if got_unit != f[4] {
            bad.push(format!("{case_id}: unit {got_unit} != 金标准 {}", f[4]));
        }
        if got_open != f[5] {
            bad.push(format!(
                "{case_id}: unit_open {got_open} != 金标准 {}",
                f[5]
            ));
        }
        // 缺参用例：去掉末行重跑，三张位图必须逐字节不变（oracle 侧同口径断言在探针里）
        if f[7] == "1" {
            if let Some(cut) = body.rfind('\n') {
                let Ok((_, pre_flag, pre_unit, pre_open)) = run_body(&body[..cut], &seed) else {
                    bad.push(format!("{case_id}: 前缀重跑解析失败，无法证明 unchanged"));
                    rows += 1;
                    continue;
                };
                if pre_flag != got_flag || pre_unit != got_unit || pre_open != got_open {
                    bad.push(format!(
                        "{case_id}: 缺参动作改动了位图（prefix 与 full 不等）"
                    ));
                }
            }
        }
        rows += 1;
    }
    if rows == 0 {
        bad.push("EMPTY_LEDGER（0 条用例）".to_string());
    }
    (rows, bad)
}

#[test]
fn quest_actions_match_oracle_ledger() {
    let path = ledger_path();
    if !path.exists() {
        panic!(
            "PREREQ 缺金标准 {}（先跑 quest-parity-probe --actions 并拷进 vectors/）",
            path.display()
        );
    }
    let text = std::fs::read_to_string(&path).expect("读不到金标准");
    let (rows, bad) = compare(&text);
    assert!(bad.is_empty(), "动作对拍不一致：{bad:#?}");
    assert!(rows >= 20, "用例数 {rows} 少于预期 20 —— 金标准可能被截断");

    // Z4：至少一个 case 的位图非零（全零互证不算绿）
    let non_zero = text.lines().any(|l| {
        let f: Vec<&str> = l.split('\t').collect();
        f.len() == 10 && (f[3].chars().any(|c| c != '0') || f[4].chars().any(|c| c != '0'))
    });
    assert!(non_zero, "金标准全是零位图 ⇒ 假绿（Z4）");
}

#[test]
fn quest_actions_comparator_rejects_degenerate_input() {
    // Z1：空账本
    let (rows, bad) = compare("");
    assert_eq!(rows, 0);
    assert!(bad.iter().any(|m| m.contains("EMPTY_LEDGER")), "{bad:?}");

    // 篡改：把某行的 flag 值改掉 ⇒ 必须报不一致（证明比较器不恒真）
    let text = std::fs::read_to_string(ledger_path()).unwrap_or_default();
    let first_row = text
        .lines()
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or("QT05-01\t1\tc1/n5,1/s[5][1]\t00\t00\t00\t-\t-\tSET [5] 1");
    let mut f: Vec<String> = first_row.split('\t').map(|s| s.to_string()).collect();
    if f.len() == 10 {
        f[3] = "ff".to_string();
        let (_, bad) = compare(&f.join("\t"));
        assert!(!bad.is_empty(), "篡改 flag 后比较器仍报绿 ⇒ 假绿");
    }
}

// ---- QT-02 CHECK 条件对拍 ----

/// 用真解析器 + 真引擎跑一条 `CHECK` 条件，返回「走的是 #SAY（真支）还是 #ELSESAY（假支）」。
fn run_check(line: &str, flag: i32, status: i32) -> Result<bool, String> {
    let text = format!("[@main]\n#IF\n{line}\n#SAY\nyes\n#ELSESAY\nno\n");
    let fs = MemFs(vec![("root/npc_def/t.txt".to_string(), text)]);
    let parsers = ScriptParsers::new();
    let mut rename = |_min: i32, _max: i32| 0;
    let out = match parsers.load_script_file(
        &fs,
        Path::new("root"),
        "Npc_def",
        "t",
        false,
        &mut rename,
    ) {
        Ok(Some(o)) => o,
        Ok(None) => return Err("PREREQ 脚本缺失".to_string()),
        Err(p) => return Err(format!("{p}")),
    };
    let def = NpcDef {
        chr_name: "T".to_string(),
        file_path: "Npc_def".to_string(),
        script_name: None,
        is_hide: false,
        is_quest: false,
        scripts: out.scripts,
        loaded: true,
        actor_id: 1,
    };
    let host = NpcHost {
        envir_root: PathBuf::from("root"),
        defs: vec![std::rc::Rc::new(def)],
        failures: Vec::new(),
        goto_count_limit: 40,
    };
    let mut rng1 = |_v: i32| 0;
    let mut player = NpcPlayer::empty(&mut rng1);
    if status != 0 {
        player.quest_flag.set(flag, 1);
    }
    let says = host
        .goto_label("T", &mut player, "@main", false)
        .unwrap_or_default();
    Ok(says.iter().any(|m| m.raw_msg.contains("yes")))
}

#[test]
fn quest_check_conditions_match_oracle_ledger() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/parity/m3/quests/vectors/quest_check_ledger.tsv");
    if !path.exists() {
        panic!(
            "PREREQ 缺金标准 {}（先跑 quest-parity-probe --checks）",
            path.display()
        );
    }
    let text = std::fs::read_to_string(&path).expect("读不到金标准");
    let mut rows = 0usize;
    let mut bad = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 5 {
            bad.push(format!("列数 {} != 5: {line}", f.len()));
            continue;
        }
        let case_id = f[0];
        let status: i32 = f[2].parse().unwrap_or(0);
        let expect_true = f[3] == "1";
        // 最后一列是 `exc|line`：真正的脚本行在 `|` 之后
        let script = match f[4].split_once('|') {
            Some((_, s)) => s,
            None => {
                bad.push(format!("{case_id}: script 列格式错: {}", f[4]));
                continue;
            }
        };
        // flag 取自解析结果 `c{cmd}/n{flag},{value}/s[...]`，取不到就按 0
        let flag: i32 = f[1]
            .split_once("/n")
            .and_then(|(_, rest)| rest.split_once(','))
            .and_then(|(a, _)| a.parse().ok())
            .unwrap_or(0);
        match run_check(script, flag, status) {
            Ok(got) => {
                if got != expect_true {
                    bad.push(format!(
                        "{case_id}: Rust 走 {} != 金标准 {}（{script}, flag={flag}, status={status}）",
                        if got { "真支" } else { "假支" },
                        if expect_true { "真支" } else { "假支" }
                    ));
                }
            }
            Err(e) => bad.push(format!("{case_id}: 解析失败 {e}")),
        }
        rows += 1;
    }
    assert!(rows > 0, "EMPTY_LEDGER（0 条用例）");
    assert!(bad.is_empty(), "CHECK 对拍不一致：{bad:#?}");
}
