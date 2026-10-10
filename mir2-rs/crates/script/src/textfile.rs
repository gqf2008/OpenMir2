//! `StringList.LoadFromFile` 的文件读取：BOM 嗅探 + GB2312(GBK) 默认解码 + `ReadLine` 切行。
//!
//! 参照 `src/OpenMir2/Common/StringList.cs` 的 `GetEncoding` / `LoadFromFile`：
//! - 有 BOM 按 BOM（UTF-8 / UTF-16LE / UTF-16BE），无 BOM 按 gb2312（.NET 实为 cp936≈GBK）；
//! - 解码失败字节用替换字符（.NET ReplacementFallback 与 encoding_rs 一致）；
//! - `StreamReader.ReadLine` 以 `\r\n` / `\r` / `\n` 切行，文件尾无换行符也返回最后一行。

use std::path::Path;

use crate::parser::ScriptFs;

pub fn decode_bytes(bytes: &[u8]) -> String {
    let text = decode_bytes_raw(bytes);
    // .NET 代码页解码器的默认回退字符是 '?'（cp936 实测：非法字节 → U+003F），
    // encoding_rs 用 U+FFFD —— 统一成 '?' 以对齐参照实现（见 whitelist B-105）。
    text.replace('\u{FFFD}', "?")
}

/// GB2312(cp936) 解码：逐字节状态机 + 差异覆盖表（`gbk_overrides.rs`）。
///
/// cp936 的字节语义（.NET 实测）：
/// - `0x00..=0x7F` → 原样字符；
/// - `0x80` → `U+20AC`（欧元，单字节，不吞后续字节）；
/// - `0xFF` → `U+F8F5`（单字节 PUA，不吞后续字节）；
/// - `0x81..=0xFE` + 后继字节 → 双字节映射（任何后继字节都按一对消耗，非单字符 → `?`）；
///   表内命中用覆盖值，否则用 encoding_rs GBK（WHATWG）逐对解码；
/// - 末位孤立引导字节 → `?`。
fn decode_gb2312(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 {
            out.push(b as char);
            i += 1;
            continue;
        }
        if b == 0x80 {
            out.push('\u{20AC}');
            i += 1;
            continue;
        }
        if b == 0xFF {
            out.push('\u{F8F5}');
            i += 1;
            continue;
        }
        if i + 1 >= bytes.len() {
            out.push('?');
            i += 1;
            continue;
        }
        let key = ((b as u16) << 8) | bytes[i + 1] as u16;
        if let Some((_, c)) = crate::gbk_overrides::GBK_OVERRIDES
            .iter()
            .find(|(k, _)| *k == key)
        {
            out.push(*c);
            i += 2;
            continue;
        }
        let (s, _, _) = encoding_rs::GBK.decode(&bytes[i..i + 2]);
        for c in s.chars() {
            out.push(if c == '\u{FFFD}' { '?' } else { c });
        }
        i += 2;
    }
    out
}

/// `StringList.LoadFromFile` 的解码模型（.NET 实测，2026-10-10）：
/// `GetEncoding` 先按前 4 字节给出编码（无 BOM 时 gb2312），但 `StreamReader` 的
/// `detectEncodingFromByteOrderMarks`（默认 true）**优先级更高**：流以可识别 BOM 开头时
/// 一律按 BOM 解并跳过 BOM。实测判定顺序与结果：
/// - `EF BB BF` → UTF-8；`FF FE` → UTF-16LE（`FF FE 00 00` 也按 UTF-16LE，实测 U+0000 起头）；
/// - `FE FF` → UTF-16BE；`00 00 FE FF` → UTF-32BE；
/// - 其余 → gb2312（cp936，见 `decode_gb2312`）。
fn decode_bytes_raw(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (s, _, _) = encoding_rs::UTF_8.decode(&bytes[3..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (s, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (s, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return decode_utf32_be(&bytes[4..]);
    }
    decode_gb2312(bytes)
}

/// UTF-32BE 解码（.NET `Encoding.UTF32` big-endian 分支）：4 字节一组，非法/残余 → `?`。
fn decode_utf32_be(bytes: &[u8]) -> String {
    let mut out = String::new();
    // 4 字节分组（as_chunks 是切片版 chunks_exact，避免 clippy::chunks_exact_to_as_chunks）
    let (groups, rest) = bytes.as_chunks::<4>();
    for chunk in groups {
        let cp = u32::from_be_bytes(*chunk);
        out.push(char::from_u32(cp).unwrap_or('?'));
    }
    for _ in rest {
        out.push('?');
    }
    out
}

/// `StreamReader.ReadLine` 切行：`\r\n` / `\r` / `\n` 均为行终止符。
pub fn split_lines(text: &str) -> Vec<String> {
    let cs: Vec<char> = text.chars().collect();
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < cs.len() {
        match cs[i] {
            '\r' => {
                if i + 1 < cs.len() && cs[i + 1] == '\n' {
                    i += 1;
                }
                lines.push(std::mem::take(&mut cur));
            }
            '\n' => lines.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
        i += 1;
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// `StringList.Text` 语义：逐行 `AppendLine`（每行后接 CRLF，末行亦然）。
pub fn text_as_string_list_text(lines: &[String]) -> String {
    let mut out = String::new();
    for l in lines {
        out.push_str(l);
        out.push('\r');
        out.push('\n');
    }
    out
}

/// FNV-1a 64（与 digest.rs / C# harness 同算法，用于原文摘要）。
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// 本地文件系统实现：以 Envir 目录为根。
pub struct LocalFs;

impl ScriptFs for LocalFs {
    fn load_string_list(&self, path: &Path) -> Option<Vec<String>> {
        let bytes = std::fs::read(path).ok()?;
        Some(split_lines(&decode_bytes(&bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_lines_readline_semantics() {
        assert_eq!(split_lines("a\r\nb\nc\rd"), vec!["a", "b", "c", "d"]);
        assert_eq!(split_lines("a\r\n"), vec!["a"]);
        assert_eq!(split_lines(""), Vec::<String>::new());
        assert_eq!(split_lines("a"), vec!["a"]);
        assert_eq!(split_lines("\n"), vec![""]);
    }

    #[test]
    fn bom_detection_matches_dotnet() {
        // .NET 实测：StreamReader 按 BOM 判定，优先于 GetEncoding 给出的 gb2312。
        // FF FE FF 41 → 按 UTF-16LE 解出 U+41FF（不是 gb2312 的 U+F8F5 '?' 'A'）。
        assert_eq!(decode_bytes(&[0xFF, 0xFE, 0xFF, 0x41]), "\u{41FF}");
        // FF FE 00 00 仍按 UTF-16LE（实测首字符 U+0000），不是 UTF-32LE
        assert_eq!(decode_bytes(&[0xFF, 0xFE, 0x00, 0x00]), "\u{0000}");
        assert_eq!(decode_bytes(&[0xFF, 0xFE, 0x41, 0x00]), "A");
        assert_eq!(decode_bytes(&[0xFE, 0xFF, 0x00, 0x41]), "A");
        assert_eq!(decode_bytes(&[0xEF, 0xBB, 0xBF, 0x41]), "A");
        // 00 00 FE FF → UTF-32BE
        assert_eq!(
            decode_bytes(&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41]),
            "A"
        );
    }

    #[test]
    fn gbk_decode() {
        // "祈福项链" 的 GBK 字节
        let gbk = [0xC6u8, 0xED, 0xB8, 0xA3, 0xCF, 0xEE, 0xC1, 0xB4];
        assert_eq!(decode_bytes(&gbk), "祈福项链");
    }
}
