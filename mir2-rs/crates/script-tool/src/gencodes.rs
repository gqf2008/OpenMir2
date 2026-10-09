//! `gen-codes`：从 C# 参照源码机械生成 Rust 码表。
//!
//! 数据源（只读，绝不修改）：
//! - `ConditionCode.cs` / `ExecutionCode.cs`：枚举成员 + `[ScriptDefName("名")]`。
//! - `GrobalVarCode.cs`：`sVAR_*` / `tVAR_*` 常量串。
//!
//! 关键语义（已实机验证）：
//! - `typeof(X).GetFields()` 在 .NET 8 下第 0 个字段是 `value__`，枚举成员的
//!   "字段序号" = 声明序号 + 1；C# 解析器字典里存的就是这个字段序号。
//! - 枚举值从首个显式值开始自增（本仓库两个枚举均无中途显式赋值，发现即报错）。
//! - 重名 `ScriptDefName`：C# 保留第一个并告警；生成器同样保留第一个并输出警告。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct CodeDef {
    /// ScriptDefName 中的命令名（脚本里写的名字）
    pub name: String,
    /// 枚举成员名
    pub member: String,
    /// GetFields() 字段序号（含 value__ 占位）
    pub field_index: i32,
    /// 枚举数值
    pub enum_value: i32,
}

#[derive(Debug)]
pub struct GenReport {
    pub condition: Vec<CodeDef>,
    pub execution: Vec<CodeDef>,
    /// GrobalVarCode: (常量名, 值)
    pub grobal_consts: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

fn parse_enum(
    src: &str,
    enum_name: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<CodeDef>, String> {
    let marker = format!("public enum {enum_name}");
    let start = src
        .find(&marker)
        .ok_or_else(|| format!("找不到枚举定义: {enum_name}"))?;
    let body_start = src[start..]
        .find('{')
        .map(|i| start + i + 1)
        .ok_or_else(|| format!("{enum_name} 缺起始花括号"))?;
    // 枚举体内无嵌套花括号，直接找下一个 '}'
    let body_end = src[body_start..]
        .find('}')
        .map(|i| body_start + i)
        .ok_or_else(|| format!("{enum_name} 缺结束花括号"))?;
    let body = &src[body_start..body_end];

    let mut defs: Vec<CodeDef> = Vec::new();
    let mut pending_attr: Option<String> = None;
    let mut next_value: i32 = 0;
    let mut decl_ordinal: i32 = -1;
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("///") || line.starts_with("//") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("[ScriptDefName(\"") {
            let name = rest
                .split('"')
                .next()
                .ok_or_else(|| format!("{enum_name} 属性行无法解析: {line}"))?;
            if pending_attr.is_some() {
                return Err(format!("{enum_name} 连续两个 ScriptDefName 无成员: {line}"));
            }
            pending_attr = Some(name.to_string());
            continue;
        }
        if line.starts_with('[') {
            // 其他属性，忽略但保留 pending
            continue;
        }
        // 成员行：IDENT 或 IDENT = 123，可带逗号与行尾注释
        let code_part = line.split("//").next().unwrap_or("").trim();
        let code_part = code_part.strip_suffix(',').unwrap_or(code_part).trim();
        if code_part.is_empty() {
            continue;
        }
        let (member, explicit) = match code_part.split_once('=') {
            Some((m, v)) => {
                let v: i32 = v
                    .trim()
                    .parse()
                    .map_err(|_| format!("{enum_name} 枚举显式值非整数: {line}"))?;
                (m.trim().to_string(), Some(v))
            }
            None => (code_part.to_string(), None),
        };
        if !member
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(format!("{enum_name} 成员名非法: {member} (行: {line})"));
        }
        decl_ordinal += 1;
        let value = match explicit {
            Some(v) => v,
            None => next_value,
        };
        next_value = value + 1;
        if let Some(name) = pending_attr.take() {
            if defs
                .iter()
                .any(|d: &CodeDef| d.name.eq_ignore_ascii_case(&name))
            {
                // C#: LogService.Warn 后 continue —— 保留第一个
                warnings.push(format!("{enum_name} 重复脚本编码定义[{name}]，保留第一个"));
                continue;
            }
            defs.push(CodeDef {
                name,
                member,
                field_index: decl_ordinal + 1, // value__ 占 0 号位
                enum_value: value,
            });
        }
    }
    Ok(defs)
}

fn parse_grobal_consts(src: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for raw_line in src.lines() {
        let line = raw_line.trim();
        let Some(rest) = line.strip_prefix("public const string ") else {
            continue;
        };
        let Some((name, value_part)) = rest.split_once('=') else {
            return Err(format!("GrobalVarCode 常量行无法解析: {line}"));
        };
        let name = name.trim().to_string();
        let value_part = value_part.trim().trim_end_matches(';').trim();
        if !value_part.starts_with('"') || !value_part.ends_with('"') {
            return Err(format!("GrobalVarCode 常量值非字符串字面量: {line}"));
        }
        // 仅处理简单转义（\" 与 \\），C# 此处无复杂字面量
        let inner = &value_part[1..value_part.len() - 1];
        let mut value = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some('r') => value.push('\r'),
                    Some(other) => value.push(other),
                    None => return Err(format!("GrobalVarCode 转义不完整: {line}")),
                }
            } else {
                value.push(c);
            }
        }
        out.push((name, value));
    }
    Ok(out)
}

pub fn generate(consts_dir: &Path) -> Result<GenReport, String> {
    let mut warnings = Vec::new();
    let read = |name: &str| -> Result<String, String> {
        let p: PathBuf = consts_dir.join(name);
        // C# 源带 BOM，直接按 UTF-8 读（内容除注释外为 ASCII）
        std::fs::read_to_string(&p).map_err(|e| format!("读取 {} 失败: {e}", p.display()))
    };
    let condition = parse_enum(&read("ConditionCode.cs")?, "ConditionCode", &mut warnings)?;
    let execution = parse_enum(&read("ExecutionCode.cs")?, "ExecutionCode", &mut warnings)?;
    let grobal_consts = parse_grobal_consts(&read("GrobalVarCode.cs")?)?;
    Ok(GenReport {
        condition,
        execution,
        grobal_consts,
        warnings,
    })
}

fn rust_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn emit_rust(report: &GenReport) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "//! 由 `script-tool gen-codes` 从 C# 参照源码机械生成，请勿手改。"
    );
    let _ = writeln!(out, "//! 数据源: src/Modules/ScriptEngine/Consts/{{ConditionCode,ExecutionCode,GrobalVarCode}}.cs");
    let _ = writeln!(out, "//! 重跑: cargo run -p mir2-script-tool -- gen-codes");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "/// 脚本命令码定义：命令名 → (GetFields 字段序号, 枚举值)。"
    );
    let _ = writeln!(out, "#[derive(Debug, Clone, Copy, PartialEq, Eq)]");
    let _ = writeln!(out, "pub struct CodeDef {{");
    let _ = writeln!(out, "    /// ScriptDefName 命令名");
    let _ = writeln!(out, "    pub name: &'static str,");
    let _ = writeln!(out, "    /// 枚举成员名");
    let _ = writeln!(out, "    pub member: &'static str,");
    let _ = writeln!(
        out,
        "    /// typeof(X).GetFields() 字段序号（value__ 占 0 号位）"
    );
    let _ = writeln!(out, "    pub field_index: i32,");
    let _ = writeln!(out, "    /// 枚举数值");
    let _ = writeln!(out, "    pub enum_value: i32,");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out);
    let emit_table = |out: &mut String, label: &str, defs: &[CodeDef]| {
        let _ = writeln!(out, "/// {label}（{} 条）", defs.len());
        let _ = writeln!(
            out,
            "#[rustfmt::skip]
pub static {}: &[CodeDef] = &[",
            label
        );
        for d in defs {
            let _ = writeln!(
                out,
                "    CodeDef {{ name: {}, member: {}, field_index: {}, enum_value: {} }},",
                rust_str(&d.name),
                rust_str(&d.member),
                d.field_index,
                d.enum_value
            );
        }
        let _ = writeln!(out, "];");
        let _ = writeln!(out);
    };
    emit_table(&mut out, "CONDITION_CODES", &report.condition);
    emit_table(&mut out, "EXECUTION_CODES", &report.execution);
    // GrobalVarCode: sVAR_* / tVAR_* 配对
    let _ = writeln!(
        out,
        "/// GrobalVarCode 的 sVAR_/tVAR_ 配对表：(脚本变量名, 替换文本)。"
    );
    let _ = writeln!(
        out,
        "#[rustfmt::skip]
pub static GROBAL_VAR_PAIRS: &[(&str, &str)] = &["
    );
    let mut pairs = 0usize;
    for (name, value) in &report.grobal_consts {
        let Some(suffix) = name.strip_prefix("sVAR_") else {
            continue;
        };
        let tname = format!("tVAR_{suffix}");
        let Some((_, tvalue)) = report.grobal_consts.iter().find(|(n, _)| *n == tname) else {
            continue;
        };
        let _ = writeln!(out, "    ({}, {}),", rust_str(value), rust_str(tvalue));
        pairs += 1;
    }
    let _ = writeln!(out, "];");
    let _ = writeln!(out, "// GROBAL_VAR_PAIRS 条数（对账用）：{pairs}");
    out
}
