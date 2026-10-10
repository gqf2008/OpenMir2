//! `gen-gbk-overrides`：比对 .NET cp936 探测表与 encoding_rs(WHATWG GBK) 的差异，
//! 生成 Rust 侧覆盖表。数据源：`cp936-pairs.txt`（由 C# 探针 dump，见 tools/gbk-probe）。
//!
//! 用途：OpenMir2 用 `Encoding.GetEncoding("gb2312")`（cp936）读脚本；cp936 比 WHATWG GBK
//! 多定义若干字节对（映射到 PUA 等），Rust 的 encoding_rs 会解成 U+FFFD。语料已实测命中
//! （`GuildRankNameFilter.txt` 的 `A8BF` → cp936 U+E7C8）。

use std::fmt::Write as _;
use std::path::Path;

/// 解析探测表：`XXXX\tHEX[,HEX...]`（值可为 `003F` = '?' 回退、或成对/单字符序列）
pub fn parse_probe(path: &Path) -> Result<Vec<(u16, Vec<char>)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("读取探测表失败: {e}"))?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (key, val) = line
            .split_once('\t')
            .ok_or_else(|| format!("第 {} 行格式错误: {line}", i + 1))?;
        let key = u16::from_str_radix(key, 16).map_err(|_| format!("第 {} 行字节对非法", i + 1))?;
        let mut chars = Vec::new();
        if !val.is_empty() {
            for cp in val.split(',') {
                let n =
                    u32::from_str_radix(cp, 16).map_err(|_| format!("第 {} 行码点非法", i + 1))?;
                chars.push(
                    char::from_u32(n).ok_or_else(|| format!("第 {} 行非法码点 {cp}", i + 1))?,
                );
            }
        }
        out.push((key, chars));
    }
    Ok(out)
}

/// 生成覆盖表：仅收录「cp936 结果 != encoding_rs 结果」的字节对。
pub fn generate_overrides(probe: &[(u16, Vec<char>)]) -> Vec<(u16, char)> {
    let mut out = Vec::new();
    for (key, expected) in probe {
        let bytes = [(*key >> 8) as u8, (*key & 0xFF) as u8];
        let (decoded, _, _) = encoding_rs::GBK.decode(&bytes);
        // 与 decode_bytes 相同的回退归一：U+FFFD → '?'（.NET cp936 的回退字符）
        let ours: Vec<char> = decoded
            .chars()
            .map(|c| if c == '\u{FFFD}' { '?' } else { c })
            .collect();
        if ours != *expected {
            // cp936 单字符结果才可安全覆盖（多字符结果交由主解码器处理）
            if expected.len() == 1 {
                out.push((*key, expected[0]));
            }
        }
    }
    out
}

pub fn emit_rust(overrides: &[(u16, char)]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "//! 由 `script-tool gen-gbk-overrides` 从 .NET cp936 探测表机械生成，请勿手改。"
    );
    let _ = writeln!(
        out,
        "//! 数据源: tools/gbk-probe (C#) → target/cp936-pairs.txt"
    );
    let _ = writeln!(
        out,
        "//! 重跑: cargo run -p mir2-script-tool -- gen-gbk-overrides --probe <探测表>"
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "/// cp936（.NET `gb2312`）比 WHATWG GBK 多定义的字节对 → 字符（{} 条）。",
        overrides.len()
    );
    let _ = writeln!(out, "#[rustfmt::skip]");
    let _ = writeln!(out, "pub static GBK_OVERRIDES: &[(u16, char)] = &[");
    for (k, c) in overrides {
        let _ = writeln!(out, "    (0x{k:04X}, '\\u{{{:04X}}}'),", *c as u32);
    }
    let _ = writeln!(out, "];");
    out
}
