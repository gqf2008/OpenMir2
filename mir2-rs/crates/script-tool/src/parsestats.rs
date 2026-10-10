//! `parse-stats`：全量加载 Envir 脚本并输出对拍统计。
//!
//! 口径定义（C# 侧 harness 必须一致）：
//! - 文件集：Envir 下递归全部 `.txt`，按相对路径（小写）排序；
//! - 加载方式：逐文件走 `LoadScriptFile` 全流程；父目录相对 Envir 根作为 `sPatch`，
//!   文件主名作为 `scriptName`；路径含 `Market_Def` 段时 `boFlag = true`；
//! - 统计：`LoadStats` 全字段 + 文件级异常（对应 C# 未被捕获的越界/重复键异常）。

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mir2_script::hutil32::{get_valid_str3, to_upper};
use mir2_script::{LoadStats, ParseErrorKind, ScriptParsers};

#[derive(Default)]
struct Totals {
    files: usize,
    loaded: usize,
    missing: usize,
    panics: usize,
    stats: LoadStats,
    errors: Vec<serde_json_error>,
    unknown_conditions: BTreeMap<String, usize>,
    unknown_actions: BTreeMap<String, usize>,
    warnings: Vec<String>,
    /// 解码回退字符（U+FFFD）出现文件数——应恒为 0：cp936 的 '?' 回退与 PUA 映射
    /// 已由 gbk_overrides 表精确复刻，出现 U+FFFD 说明解码器与参照不一致
    decode_anomalies: usize,
    /// 逐文件原文摘要（`StringList.Text` 形态的 FNV-1a 64），把解码差异本身纳入对拍
    text_digests: BTreeMap<String, String>,
    /// 逐文件结构摘要（F1 门禁）：relpath → (hash, 计数)
    digests: BTreeMap<String, (String, mir2_script::StructureCounts)>,
    /// 语料命令使用直方图：CmdCode → 次数（条件/动作分开）
    cond_usage: BTreeMap<i32, usize>,
    act_usage: BTreeMap<i32, usize>,
}

#[allow(non_camel_case_types)]
struct serde_json_error {
    file: String,
    line_index: usize,
    kind: String,
    line: String,
}

/// Win32 通配 `*.txt` 的语义：扩展名恰为 3 个字符时，匹配**以 txt 开头**的更长扩展名
/// （`Directory.GetFiles(root, "*.txt", AllDirectories)` 与 `x.txt2` 亦命中）。此处对齐。
fn matches_win32_txt_pattern(p: &Path) -> bool {
    let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    // 用字节前缀比较：ext 以多字节字符开头时 `ext[..3]` 会 panic（非 char 边界）
    ext.len() >= 3 && ext.as_bytes()[..3].eq_ignore_ascii_case(b"txt")
}

/// 命令使用分类：已注册处理器 / ScriptEngine switch 分支 / 未注册（C# 静默忽略）
fn classify(kind_handlers: &[(i32, &str)], kind_switch: &[(i32, &str)], code: i32) -> String {
    if let Some((_, h)) = kind_handlers.iter().find(|(k, _)| *k == code) {
        return format!("handler:{h}");
    }
    if let Some((_, c)) = kind_switch.iter().find(|(k, _)| *k == code) {
        return format!("engine-switch:{c}");
    }
    // ScriptEngine 对未注册 code 走 switch，switch 未命中即静默忽略（结果保持默认）
    "ignored".to_string()
}

fn report_usage(t: &Totals) {
    use mir2_script::handlers::{
        CONDITION_HANDLERS, ENGINE_SWITCH_CONDITIONS, ENGINE_SWITCH_EXECUTIONS, EXECUTION_HANDLERS,
    };
    let mut buckets: BTreeMap<String, (usize, Vec<i32>)> = BTreeMap::new();
    for (code, n) in &t.cond_usage {
        let c = classify(CONDITION_HANDLERS, ENGINE_SWITCH_CONDITIONS, *code);
        let e = buckets.entry(format!("条件/{c}")).or_default();
        e.0 += n;
        e.1.push(*code);
    }
    for (code, n) in &t.act_usage {
        let c = classify(EXECUTION_HANDLERS, ENGINE_SWITCH_EXECUTIONS, *code);
        let e = buckets.entry(format!("动作/{c}")).or_default();
        e.0 += n;
        e.1.push(*code);
    }
    println!("命令派发分类（语料实测）：");
    let mut ignored_total = 0usize;
    for (label, (n, codes)) in &buckets {
        if label.ends_with("/ignored") {
            ignored_total += n;
        }
        println!("  {label}: {n} 次（{} 个 code）", codes.len());
    }
    println!(
        "  未注册（C# 静默忽略）合计: {ignored_total} 次；条件 distinct code {} / 动作 distinct code {}",
        t.cond_usage.len(),
        t.act_usage.len()
    );
}

fn walk_txt(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(std::result::Result::ok).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk_txt(&p, out);
        } else if matches_win32_txt_pattern(&p) {
            out.push(p);
        }
    }
}

/// 从“脚本错误”行提取命令名（对齐解析器的 opname 拆分后大写）。
fn command_name_of(line: &str) -> String {
    let (first, _) = get_valid_str3(line, &[' ', '\t']);
    let cmd = if first.contains('.') {
        let (_, rest) = get_valid_str3(&first, &['.']);
        rest
    } else {
        first
    };
    to_upper(&cmd)
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn stats_json(t: &Totals) -> String {
    let mut o = String::new();
    let s = &t.stats;
    let _ = writeln!(o, "{{");
    let _ = writeln!(
        o,
        "  \"files\": {}, \"loaded\": {}, \"missing\": {}, \"panics\": {},",
        t.files, t.loaded, t.missing, t.panics
    );
    let _ = writeln!(o, "  \"stats\": {{");
    let _ = writeln!(
        o,
        "    \"scripts\": {}, \"records\": {}, \"procedures\": {},",
        s.scripts, s.records, s.procedures
    );
    let _ = writeln!(
        o,
        "    \"conditions\": {}, \"actions\": {}, \"else_actions\": {},",
        s.conditions, s.actions, s.else_actions
    );
    let _ = writeln!(
        o,
        "    \"define_substitutions\": {}, \"call_loads\": {}, \"call_failures\": {},",
        s.define_substitutions, s.call_loads, s.call_failures
    );
    let _ = writeln!(
        o,
        "    \"include_loads\": {}, \"include_failures\": {}, \"goods\": {}, \"price_rate_lines\": {},",
        s.include_loads, s.include_failures, s.goods, s.price_rate_lines
    );
    let _ = writeln!(
        o,
        "    \"item_type_lines\": {}, \"merchant_cmd_lines\": {}, \"parse_errors\": {},",
        s.item_type_lines, s.merchant_cmd_lines, s.parse_errors
    );
    let _ = writeln!(o, "    \"label_renames\": {}", s.label_renames);
    let _ = writeln!(o, "  }},");
    let dump_map = |o: &mut String, name: &str, m: &BTreeMap<String, usize>, trailing: bool| {
        let _ = writeln!(o, "  \"{name}\": {{");
        for (i, (k, v)) in m.iter().enumerate() {
            let comma = if i + 1 == m.len() { "" } else { "," };
            let _ = writeln!(o, "    \"{}\": {}{comma}", json_escape(k), v);
        }
        let _ = writeln!(o, "  }}{}", if trailing { "," } else { "" });
    };
    dump_map(&mut o, "unknown_conditions", &t.unknown_conditions, true);
    dump_map(&mut o, "unknown_actions", &t.unknown_actions, true);
    let _ = writeln!(o, "  \"structure_digest\": {{");
    for (i, (file, (hash, c))) in t.digests.iter().enumerate() {
        let comma = if i + 1 == t.digests.len() { "" } else { "," };
        let _ = writeln!(
            o,
            "    \"{}\": {{\"hash\": \"{}\", \"s\": {}, \"r\": {}, \"p\": {}, \"c\": {}, \"a\": {}, \"e\": {}}}{comma}",
            json_escape(file), hash, c.scripts, c.records, c.procedures, c.conditions, c.actions, c.else_actions
        );
    }
    let _ = writeln!(o, "  }},");
    let _ = writeln!(o, "  \"text_digest\": {{");
    for (i, (file, h)) in t.text_digests.iter().enumerate() {
        let comma = if i + 1 == t.text_digests.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(o, "    \"{}\": \"{}\"{comma}", json_escape(file), h);
    }
    let _ = writeln!(o, "  }},");
    let _ = writeln!(o, "  \"errors\": [");
    for (i, e) in t.errors.iter().enumerate() {
        let comma = if i + 1 == t.errors.len() { "" } else { "," };
        let _ = writeln!(
            o,
            "    {{\"file\": \"{}\", \"line_index\": {}, \"kind\": \"{}\", \"line\": \"{}\"}}{comma}",
            json_escape(&e.file),
            e.line_index,
            e.kind,
            json_escape(&e.line)
        );
    }
    let _ = writeln!(o, "  ],");
    let _ = writeln!(o, "  \"warnings\": [");
    for (i, w) in t.warnings.iter().enumerate() {
        let comma = if i + 1 == t.warnings.len() { "" } else { "," };
        let _ = writeln!(o, "    \"{}\"{comma}", json_escape(w));
    }
    let _ = writeln!(o, "  ]");
    let _ = writeln!(o, "}}");
    o
}

pub fn run(envir: &Path, json_out: Option<PathBuf>) -> ExitCode {
    if !envir.is_dir() {
        eprintln!("Envir 目录不存在: {}", envir.display());
        return ExitCode::FAILURE;
    }
    let mut files = Vec::new();
    walk_txt(envir, &mut files);
    files.sort_by_key(|p| {
        p.strip_prefix(envir)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
            .to_lowercase()
    });

    let parsers = ScriptParsers::new();
    let fs = mir2_script::LocalFs;
    let mut totals = Totals {
        files: files.len(),
        ..Totals::default()
    };
    // 与 C# harness 同种子同算法（System.Random(42) 复刻），重名 label 改名序列逐位一致
    let mut sys_rng = mir2_shared::rng::RandomNumber::with_seed(42);
    let mut rename_rand = move |min: i32, max: i32| sys_rng.get_random_number(min, max);

    for file in &files {
        let rel = file.strip_prefix(envir).unwrap_or(file);
        let parent = rel
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let stem = file
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let bo_flag = rel.components().any(|c| {
            c.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case("Market_Def")
        });
        // 语料检查：非 BMP 字符会破坏 UTF-16 索引等价假设
        let bytes = std::fs::read(file).unwrap_or_default();
        let text = mir2_script::decode_bytes(&bytes);
        if text.chars().any(|c| c == '\u{FFFD}') {
            totals.decode_anomalies += 1;
            totals.warnings.push(format!(
                "{}: 解码出现 U+FFFD（与 cp936 不一致）",
                rel.display()
            ));
        }
        if text.chars().any(|c| (c as u32) > 0xFFFF) {
            totals.warnings.push(format!(
                "{}: 含非 BMP 字符，UTF-16 索引等价假设不成立",
                rel.display()
            ));
        }
        let result =
            parsers.load_script_file(&fs, envir, &parent, &stem, bo_flag, &mut rename_rand);
        let rel_name = rel.to_string_lossy().replace('\\', "/");
        // 逐文件原文摘要（`StringList.Text` 形态），把解码差异本身纳入对拍
        totals.text_digests.insert(
            rel_name.clone(),
            format!(
                "{:016x}",
                mir2_script::fnv1a64(
                    mir2_script::text_as_string_list_text(&mir2_script::split_lines(&text))
                        .as_bytes()
                )
            ),
        );
        match result {
            Ok(Some(outcome)) => {
                totals.loaded += 1;
                for sc in &outcome.scripts {
                    for rec in &sc.record_list {
                        for proc_ in &rec.procedure_list {
                            for c in &proc_.condition_list {
                                *totals.cond_usage.entry(c.cmd_code).or_insert(0) += 1;
                            }
                            for a in proc_
                                .action_list
                                .iter()
                                .chain(proc_.else_action_list.iter())
                            {
                                *totals.act_usage.entry(a.n_cmd_code).or_insert(0) += 1;
                            }
                        }
                    }
                }
                let merchant = mir2_script::MerchantDigestInput {
                    price_rate: outcome.price_rate.unwrap_or(0),
                    item_type_list: outcome.item_type_list.clone(),
                    flags: outcome.merchant_flags,
                    refill_goods: outcome.refill_goods.clone(),
                };
                let digest = mir2_script::structure_digest_full(&outcome.scripts, &merchant);
                totals
                    .digests
                    .insert(rel_name.clone(), (digest.hash, digest.counts));
                let s = &outcome.stats;
                let t = &mut totals.stats;
                t.scripts += s.scripts;
                t.records += s.records;
                t.procedures += s.procedures;
                t.conditions += s.conditions;
                t.actions += s.actions;
                t.else_actions += s.else_actions;
                t.define_substitutions += s.define_substitutions;
                t.call_loads += s.call_loads;
                t.call_failures += s.call_failures;
                t.include_loads += s.include_loads;
                t.include_failures += s.include_failures;
                t.goods += s.goods;
                t.price_rate_lines += s.price_rate_lines;
                t.item_type_lines += s.item_type_lines;
                t.merchant_cmd_lines += s.merchant_cmd_lines;
                t.parse_errors += s.parse_errors;
                t.label_renames += s.label_renames;
                for e in &outcome.errors {
                    if e.kind == ParseErrorKind::ScriptError {
                        // 按行首指令归入条件/动作未知名单（粗分：含在 #IF 段失败为条件）
                        let name = command_name_of(&e.line);
                        // 无法从错误行直接区分条件/动作，两个表各查一次：
                        let in_cond = mir2_script::codes::CONDITION_CODES
                            .iter()
                            .any(|d| d.name.eq_ignore_ascii_case(&name));
                        let in_exec = mir2_script::codes::EXECUTION_CODES
                            .iter()
                            .any(|d| d.name.eq_ignore_ascii_case(&name));
                        if !in_cond && !in_exec {
                            *totals.unknown_conditions.entry(name.clone()).or_insert(0) += 1;
                            *totals.unknown_actions.entry(name).or_insert(0) += 1;
                        }
                    }
                    totals.errors.push(serde_json_error {
                        file: rel_name.clone(),
                        line_index: e.line_index,
                        kind: format!("{:?}", e.kind),
                        line: e.line.clone(),
                    });
                }
            }
            Ok(None) => {
                totals.missing += 1;
                totals
                    .digests
                    .insert(rel_name.clone(), ("MISSING".into(), Default::default()));
            }
            Err(panic) => {
                totals.panics += 1;
                // 异常文本两侧不同（Rust LoadPanic vs C# 异常消息），故只标记类别
                totals
                    .digests
                    .insert(rel_name.clone(), ("PANIC".into(), Default::default()));
                totals.errors.push(serde_json_error {
                    file: rel_name,
                    line_index: 0,
                    kind: "LoadPanic".to_string(),
                    line: panic.to_string(),
                });
            }
        }
    }

    let json = stats_json(&totals);
    if let Some(path) = json_out {
        if let Err(e) = std::fs::write(&path, &json) {
            eprintln!("写 JSON 失败: {e}");
            return ExitCode::FAILURE;
        }
    }
    let s = &totals.stats;
    println!("Envir: {}", envir.display());
    println!(
        "文件: {} 加载: {} 缺失: {} 异常: {}",
        totals.files, totals.loaded, totals.missing, totals.panics
    );
    println!(
        "脚本 {} 标签 {} 过程 {} 条件 {} 动作 {} 否则动作 {}",
        s.scripts, s.records, s.procedures, s.conditions, s.actions, s.else_actions
    );
    println!(
        "宏展开 {} #CALL成功 {} #CALL失败 {} #INCLUDE {} 商品 {}",
        s.define_substitutions, s.call_loads, s.call_failures, s.include_loads, s.goods
    );
    println!(
        "解析错误: {}；解码异常文件: {}",
        s.parse_errors, totals.decode_anomalies
    );
    report_usage(&totals);

    if !totals.errors.is_empty() {
        println!("错误明细（前 20 条）:");
        for e in totals.errors.iter().take(20) {
            println!("  [{}] {} 行{}: {}", e.kind, e.file, e.line_index, e.line);
        }
    }
    if !totals.warnings.is_empty() {
        println!("警告:");
        for w in &totals.warnings {
            println!("  {w}");
        }
    }
    // 退出码：有解析异常（LoadPanic）→ 非零；解析错误行属语料事实，不影响退出码
    if totals.panics > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
