//! 构建期从 oracle C# 源提取 `[Command("出厂名", "描述", "帮助", …)]` 的 desc/help。
//!
//! `gm_list` 的 185 条目录来自 `bins/game-svr/src/gmcmd.rs`（现网名/权限/实现态）与
//! `tests/parity/m3/design-gm-commands-20261023.md` §4.3（族/代码类/效应/帧）。这两处没有
//! 中文描述；oracle 的 `[Command]` 属性（`src/Modules/GameCommand/CommandAttribute.cs`）才有。
//! 本脚本把 `名称 -> {desc, help}` 写成 `OUT_DIR/gm_csharp_attrs.json`，运行期按 **出厂名** 关联。
//!
//! 只读仓库内 C# 源，不写任何仓库文件（产物落 `OUT_DIR`）。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    // mir2-rs/bins/mir2-mcp -> 仓库根
    let src_dir = manifest.join("../../../src/Modules/GameCommand");
    let mut out = Map::new();
    let mut files = Vec::new();
    collect_cs(&src_dir, &mut files);
    files.sort();
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => panic!("读 {} 失败：{e}", path.display()),
        };
        scan_commands(&text, &mut out);
    }
    println!("cargo:rerun-if-changed={}", src_dir.display());

    let target = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("gm_csharp_attrs.json");
    let body = serde_json::to_string(&Value::Object(out)).expect("序列化 C# 属性");
    fs::write(&target, body).expect("写 gm_csharp_attrs.json");
}

fn collect_cs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_cs(&path, out);
        } else if path.extension().is_some_and(|e| e == "cs") {
            out.push(path);
        }
    }
}

/// 扫描一个文件里的全部 `[Command(...)]`，按出厂名写入 `{desc, help}`（同名只取首次）。
fn scan_commands(text: &str, out: &mut Map<String, Value>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut from = 0usize;
    while let Some(rel) = text[from..].find("[Command(") {
        let open = from + rel + "[Command(".len();
        let Some((args, next)) = parse_attr_args(text, open) else {
            from = open;
            continue;
        };
        from = next;
        let Some(name) = args.first().and_then(|a| quoted(a)) else {
            continue;
        };
        let desc = args
            .get(1)
            .and_then(|a| quoted(a))
            .unwrap_or("")
            .to_string();
        // 第 3 参若是字符串则是 Help；若是数字则是 PermissionMin（如 `[Command("AdjustExp", "", 10)]`）。
        let help = args
            .get(2)
            .and_then(|a| quoted(a))
            .unwrap_or("")
            .to_string();
        if out.contains_key(name) {
            continue;
        }
        out.insert(
            name.to_string(),
            Value::Object(Map::from_iter([
                ("desc".to_string(), Value::String(desc)),
                ("help".to_string(), Value::String(help)),
            ])),
        );
    }
}

/// 从 `open`（`[Command(` 之后）扫到配对的 `)`，按顶层逗号切参数。
/// 引号内的 `)`/`,` 不切分（desc 里出现 `(支持权限分配)` 的类必须走这条路径）。
fn parse_attr_args(text: &str, open: usize) -> Option<(Vec<String>, usize)> {
    let bytes = text.as_bytes();
    let mut depth = 1usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut cur = String::new();
    let mut args = Vec::new();
    let mut i = open;
    while i < bytes.len() {
        let ch = text[i..].chars().next()?;
        i += ch.len_utf8();
        if quoted {
            if escaped {
                escaped = false;
                cur.push(ch);
                continue;
            }
            match ch {
                '\\' => {
                    escaped = true;
                    cur.push(ch);
                }
                '"' => {
                    quoted = false;
                    cur.push(ch);
                }
                _ => cur.push(ch),
            }
            continue;
        }
        match ch {
            '"' => {
                quoted = true;
                cur.push(ch);
            }
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    args.push(cur.trim().to_string());
                    return Some((args, i));
                }
                cur.push(ch);
            }
            ',' if depth == 1 => {
                args.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    None
}

/// `"abc"` -> `abc`（不是字符串字面量则 None）。
fn quoted(arg: &str) -> Option<&str> {
    let arg = arg.trim();
    arg.strip_prefix('"')?.strip_suffix('"')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_attribute_with_parentheses_in_desc() {
        let mut out = Map::new();
        scan_commands(
            "[Command(\"Ting\", \"将指定人物随机传送(支持权限分配)\", CommandHelp.GameCommandTingHelpMsg, 10)]",
            &mut out,
        );
        let v = out.get("Ting").expect("Ting");
        assert_eq!(v["desc"], "将指定人物随机传送(支持权限分配)");
        assert_eq!(v["help"], "");
    }

    #[test]
    fn third_numeric_arg_is_permission_not_help() {
        let mut out = Map::new();
        scan_commands("[Command(\"AdjustExp\", \"\", 10)]", &mut out);
        assert_eq!(out["AdjustExp"]["desc"], "");
        assert_eq!(out["AdjustExp"]["help"], "");
    }
}
