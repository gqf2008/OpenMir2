//! msgcodegen — 从 C# 源机械生成 Rust 消息号表。
//!
//! 数据源（唯一真源，服务端为准）：
//!   - `src/OpenMir2/Messages.cs`   → 全部 `public const int/byte NAME = VALUE;`
//!   - `src/OpenMir2/Grobal2.cs`    → 仅 GROBAL2_PICK 列出的常量（PacketCode / GM_* / ClientVersionNumber）
//!
//! 用法：
//!   cargo run -p msgcodegen            # 重新生成 crates/protocol/src/messages.rs
//!   cargo run -p msgcodegen -- --check # 漂移门禁：生成结果与已提交文件不一致则 exit 1
//!
//! 注意：值表达式只支持「十进制 / 0x十六进制 / A + B」，遇到其他形态直接报错（不许猜）。

// 文档逐字引用 C# 标识符，不加反引号改造。
#![allow(clippy::doc_markdown)]

use anyhow::{bail, Context, Result};
use regex::Regex;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Grobal2.cs 里属于协议面的常量（白名单，只有这些会被采入，生成到 `grobal2` 子模块）。
const GROBAL2_PICK: &[&str] = &[
    "ClientVersionNumber",
    "PacketCode",
    "GM_OPEN",
    "GM_CLOSE",
    "GM_CHECKSERVER",
    "GM_CHECKCLIENT",
    "GM_DATA",
    "GM_SERVERUSERINDEX",
    "GM_RECEIVE_OK",
    "GM_STOP",
];

struct Entry {
    name: String,
    value: i64,
    /// C# 声明类型（int/byte/uint），决定 Rust 侧类型。
    cs_type: String,
    /// 从 `/// ...` 文档注释里采到的中文说明（可空）。
    doc: Option<String>,
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/mir2-rs/tools/msgcodegen
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repo root must exist")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root must exist")
}

/// 求值常量表达式：十进制 / 0x 十六进制 / 以 `+` 连接的若干项。其他形态报错。
fn eval_expr(expr: &str) -> Result<i64> {
    let mut sum: i64 = 0;
    for part in expr.split('+') {
        let tok = part.trim();
        let v = if let Some(hex) = tok.strip_prefix("0x").or_else(|| tok.strip_prefix("0X")) {
            i64::from_str_radix(hex, 16).with_context(|| format!("bad hex literal: {tok}"))?
        } else {
            tok.parse::<i64>()
                .with_context(|| format!("unsupported const expr token: {tok}"))?
        };
        sum = sum
            .checked_add(v)
            .with_context(|| format!("const expr overflow: {expr}"))?;
    }
    Ok(sum)
}

fn parse_cs(path: &Path, pick: Option<&[&str]>) -> Result<Vec<Entry>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let const_re =
        Regex::new(r"^\s*public const (int|byte|uint)\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([^;]+);")
            .unwrap();
    let doc_re = Regex::new(r"^\s*///\s*(?:<summary>)?(.*?)(?:</summary>)?\s*$").unwrap();

    let mut entries = Vec::new();
    let mut pending_doc: Vec<String> = Vec::new();
    for line in text.lines() {
        if let Some(cap) = const_re.captures(line) {
            let name = cap[2].to_string();
            if let Some(pick_list) = pick {
                if !pick_list.contains(&name.as_str()) {
                    pending_doc.clear();
                    continue;
                }
            }
            let value = eval_expr(cap[3].trim())
                .with_context(|| format!("{}: {}", path.display(), name))?;
            let doc = if pending_doc.is_empty() {
                None
            } else {
                Some(pending_doc.join(" "))
            };
            entries.push(Entry {
                name,
                value,
                cs_type: cap[1].to_string(),
                doc,
            });
            pending_doc.clear();
        } else if let Some(cap) = doc_re.captures(line) {
            let text = cap[1].trim();
            if !text.is_empty() {
                pending_doc.push(text.to_string());
            }
        } else if !line.trim().is_empty() && !line.trim_start().starts_with("//") {
            // 非注释、非空行打断 doc 归属
            pending_doc.clear();
        }
    }
    Ok(entries)
}

/// Rust 侧类型：消息号一律 u16（线上 Ident 字段就是 u16，全部值 < 65536，生成时断言）；
/// byte → u8；超出 u16 的 int/uint（PacketCode、ClientVersionNumber）→ u32。
fn rust_type(e: &Entry) -> Result<&'static str> {
    match e.cs_type.as_str() {
        "byte" => {
            if !(0..=i64::from(u8::MAX)).contains(&e.value) {
                bail!("{} = {} does not fit u8", e.name, e.value);
            }
            Ok("u8")
        }
        "int" | "uint" => {
            if (0..=i64::from(u16::MAX)).contains(&e.value) {
                Ok("u16")
            } else if (0..=i64::from(u32::MAX)).contains(&e.value) {
                Ok("u32")
            } else {
                bail!("{} = {} does not fit u32", e.name, e.value);
            }
        }
        other => bail!("unsupported cs const type {other} for {}", e.name),
    }
}

fn section_of(name: &str) -> &'static str {
    for (prefix, section) in [
        ("CM_", "CM_* — 客户端→服务端"),
        ("SM_", "SM_* — 服务端→客户端"),
        ("RM_", "RM_* — 网关↔引擎内部转译号"),
        ("SS_", "SS_* — 会话/登录服务内部"),
        ("ISM_", "ISM_* — 跨服/互联消息"),
        ("DBR_", "DBR_* — DB 应答"),
        ("DB_", "DB_* — DB 请求"),
        ("GM_", "GM_* — 网关/引擎管理号（Messages.cs 侧）"),
    ] {
        if name.starts_with(prefix) {
            return section;
        }
    }
    "其他"
}

fn write_entry(out: &mut String, e: &Entry, indent: &str) -> Result<()> {
    if let Some(doc) = &e.doc {
        writeln!(out, "{indent}/// {doc}").unwrap();
    }
    let ty = rust_type(e)?;
    writeln!(out, "{indent}pub const {}: {} = {};", e.name, ty, e.value).unwrap();
    Ok(())
}

fn render(entries: &[Entry], grobal2: &[Entry]) -> Result<String> {
    let mut out = String::new();
    out.push_str(
        "//! 消息号表 —— 由 `cargo run -p msgcodegen` 从 C# 源机械生成，**禁止手改**。\n\
         //!\n\
         //! 真源：`src/OpenMir2/Messages.cs` + `src/OpenMir2/Grobal2.cs`（白名单常量，在 [`grobal2`] 子模块）。\n\
         //! 漂移门禁：`cargo run -p msgcodegen -- --check`（与 C# 源不一致时变红）。\n\
         //!\n\
         //! 已知坑（服务端为准）：`SM_ATTACKMODE == 213`；客户端分支曾把 213 占给\n\
         //! `SM_HERODELMAGIC`（已改 546），本表以服务端编号为唯一判据。\n\n\
         // 常量名与 C# 源逐字一致（含 PascalCase/混写），便于双向检索，故豁免命名 lint。\n\
         // 文档注释采自 C# `///`（可空），不强制。\n\
         #![allow(non_upper_case_globals)]\n\
         #![allow(missing_docs)]\n\
         // 字面量保持 C# 源写法（不分位），便于人工对照。\n\
         #![allow(clippy::unreadable_literal)]\n\
         // names_of 是巨型 match（每个值一行），行数即数据量。\n\
         #![allow(clippy::too_many_lines)]\n",
    );

    // 按 section 归组：保持 section 首次出现顺序，组内保持 C# 文件顺序。
    let mut sections: Vec<(&str, Vec<&Entry>)> = Vec::new();
    for e in entries {
        let s = section_of(&e.name);
        match sections.iter_mut().find(|(name, _)| *name == s) {
            Some((_, v)) => v.push(e),
            None => sections.push((s, vec![e])),
        }
    }
    for (section, items) in &sections {
        writeln!(out, "\n// ==== {section} ====\n").unwrap();
        for e in items {
            write_entry(&mut out, e, "")?;
        }
    }

    // Grobal2 白名单进子模块：避免与 Messages.cs 同名常量冲突
    //（Messages.GM_STOP = 21 与 Grobal2.GM_STOP = 8 是两套编号，客观上同名不同值）。
    out.push_str("\n/// `Grobal2.cs` 白名单常量（网关↔引擎管理帧与版本号）。\npub mod grobal2 {\n");
    for e in grobal2 {
        write_entry(&mut out, e, "    ")?;
    }
    out.push_str("}\n");

    // 反向索引：ident 值 → 全部常量名。值冲突客观存在
    //（如 2001 = CM_IDPASSWORD/CM_PASSWORD，116 = ISM_QUERYPLAYTIME/ISM_CHECKTIMEACCOUNT），
    //  grobal2 子模块的名字以 `grobal2::` 前缀区分。
    // 仅用于日志/回放报告的可读化，不参与分派。
    out.push_str(
        "\n/// ident 值 → 全部常量名（值可冲突；按模块边界取舍是调用方的事）。\n\
         /// 仅用于日志/回放报告的可读化，不参与分派。\n\
         #[must_use]\npub fn names_of(ident: u16) -> &'static [&'static str] {\n    match ident {\n",
    );
    let mut by_value: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    for e in entries {
        if matches!(rust_type(e)?, "u16") {
            by_value.entry(e.value).or_default().push(e.name.clone());
        }
    }
    for e in grobal2 {
        if matches!(rust_type(e)?, "u16") {
            by_value
                .entry(e.value)
                .or_default()
                .push(format!("grobal2::{}", e.name));
        }
    }
    for (value, names) in &by_value {
        let list = names
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(out, "        {value} => &[{list}],").unwrap();
    }
    out.push_str("        _ => &[],\n    }\n}\n");
    Ok(out)
}

/// 生成结果过一遍 rustfmt（stdin→stdout），保证已提交文件与 `cargo fmt --check` 零漂移。
fn rustfmt(text: &str) -> Result<String> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2021", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn rustfmt（需要 rustfmt 在 PATH 中；rustup component add rustfmt）")?;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(text.as_bytes())
        .context("write rustfmt stdin")?;
    let out = child.wait_with_output().context("wait rustfmt")?;
    if !out.status.success() {
        bail!("rustfmt failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    String::from_utf8(out.stdout).context("rustfmt output not utf-8")
}

fn main() -> Result<()> {
    let check = std::env::args().any(|a| a == "--check");
    let root = repo_root();

    let entries = parse_cs(&root.join("src/OpenMir2/Messages.cs"), None)?;
    let grobal2 = parse_cs(&root.join("src/OpenMir2/Grobal2.cs"), Some(GROBAL2_PICK))?;
    if grobal2.len() != GROBAL2_PICK.len() {
        bail!(
            "Grobal2.cs 白名单采不全：期望 {} 条，实际 {} 条（源文件改了？）",
            GROBAL2_PICK.len(),
            grobal2.len()
        );
    }

    let rendered = rustfmt(&render(&entries, &grobal2)?)?;
    let out_path = workspace_root().join("crates/protocol/src/messages.rs");

    if check {
        let existing = std::fs::read_to_string(&out_path)
            .with_context(|| format!("read {}", out_path.display()))?;
        if existing != rendered {
            eprintln!(
                "msgcodegen --check FAILED: {} 与 C# 源（src/OpenMir2/Messages.cs）漂移。\n\
                 跑 `cargo run -p msgcodegen` 重新生成后再提交。",
                out_path.display()
            );
            std::process::exit(1);
        }
        println!(
            "msgcodegen --check OK: {} 条常量（Messages.cs {} 条 + Grobal2 白名单 {} 条）与 C# 源一致。",
            entries.len() + grobal2.len(),
            entries.len(),
            grobal2.len()
        );
        return Ok(());
    }

    std::fs::write(&out_path, &rendered)
        .with_context(|| format!("write {}", out_path.display()))?;
    println!(
        "generated {} ({} entries: {} from Messages.cs, {} from Grobal2.cs)",
        out_path.display(),
        entries.len() + grobal2.len(),
        entries.len(),
        grobal2.len()
    );
    Ok(())
}
