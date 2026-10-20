//! `StringList.LoadFromFile` 的文件读取：BOM 嗅探 + GB2312(GBK) 默认解码 + `ReadLine` 切行。
//!
//! 参照 `src/OpenMir2/Common/StringList.cs` 的 `GetEncoding` / `LoadFromFile`：
//! - 有 BOM 按 BOM（UTF-8 / UTF-16LE / UTF-16BE / UTF-32BE），无 BOM 按 gb2312
//!   （.NET 实为 cp936）；**本路径没有 UTF-32LE 分支**：`FF FE 00 00` 是 UTF-16LE
//!   （`GetEncoding` 先给 `Encoding.Unicode`，`StreamReader` 不再升级；见
//!   [`crate::cp936::decode_string_list`]）；实现统一在 [`crate::cp936`]；
//! - `StreamReader.ReadLine` 以 `\r\n` / `\r` / `\n` 切行，文件尾无换行符也返回最后一行。

use std::path::Path;

use crate::parser::ScriptFs;

pub fn decode_bytes(bytes: &[u8]) -> String {
    // 唯一实现见 `crate::cp936::decode_string_list`（B-105 / E-4 的统一入口）：
    // BOM 模型是 `StringList.LoadFromFile` 的 GetEncoding + StreamReader 组合
    // （`FF FE 00 00` = UTF-16LE，不是 UTF-32LE）；`U+FFFD → '?'` 的归一
    // **只适用于 cp936 分支**。
    crate::cp936::decode_string_list(bytes)
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

/// `StringList.SaveToFile` 的编码口径：`Encoding.GetEncoding("gb2312")`（cp936≈GBK，**无 BOM**），
/// 逐行 `WriteLine`（每行后接 CRLF，由调用方拼好）。
///
/// 逐 UTF-16 码元编码：不可映射码元回退单字节 `'?'`（非 BMP 字符 → 两个 `?`），
/// 与 .NET cp936 编码器一致；
/// **不用** one-shot `Encoding::encode` 的整串替换（它会写 `&#NNNN;` 数值字符引用，会把列表写坏）。
pub fn encode_gb2312(text: &str) -> Vec<u8> {
    // 唯一实现见 `crate::cp936::encode_gb2312_dotnet`（E-1 / E-2）：逐 UTF-16 码元查
    // .NET cp936 表，不可映射 → `?`；绝不写 `&#NNNN;` 数字字符引用。
    crate::cp936::encode_gb2312_dotnet(text)
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
        // FF FE 00 00 在 StringList 路径是 UTF-16LE（GetEncoding 给 Encoding.Unicode，
        // StreamReader 不再升级 UTF-32LE）：首字符 U+0000，41 00 00 00 → A, U+0000。
        assert_eq!(decode_bytes(&[0xFF, 0xFE, 0x00, 0x00]), "\u{0000}");
        assert_eq!(
            decode_bytes(&[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00]),
            "\u{0000}A\u{0000}"
        );
        // UTF-16LE 尾部单字节 → 单个 U+FFFD（不是 ?）
        assert_eq!(
            decode_bytes(&[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00, 0x42]),
            "\u{0000}A\u{0000}\u{FFFD}"
        );
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
    fn invalid_sequence_fallback_per_branch_matches_dotnet() {
        // .NET 实测（StringList.LoadFromFile）：
        // UTF-8 BOM + 0xFF → U+FFFD U+0041（不是 '?'）
        assert_eq!(decode_bytes(&[0xEF, 0xBB, 0xBF, 0xFF, 0x41]), "\u{FFFD}A");
        // UTF-8 编码的代理码位 → 逐字节 U+FFFD（三个）
        assert_eq!(
            decode_bytes(&[0xEF, 0xBB, 0xBF, 0xED, 0xA0, 0x80]),
            "\u{FFFD}\u{FFFD}\u{FFFD}"
        );
        // UTF-16LE/BE 孤立高代理 → U+FFFD + 'A'
        assert_eq!(
            decode_bytes(&[0xFF, 0xFE, 0x00, 0xD8, 0x41, 0x00]),
            "\u{FFFD}A"
        );
        assert_eq!(
            decode_bytes(&[0xFE, 0xFF, 0xD8, 0x00, 0x00, 0x41]),
            "\u{FFFD}A"
        );
        // gb2312 分支：非法字节对 → 单个 '?'（成对消耗）+ 'A'
        assert_eq!(decode_bytes(&[0x81, 0x20, 0x41]), "?A");
    }

    #[test]
    fn gbk_decode() {
        // "祈福项链" 的 GBK 字节
        let gbk = [0xC6u8, 0xED, 0xB8, 0xA3, 0xCF, 0xEE, 0xC1, 0xB4];
        assert_eq!(decode_bytes(&gbk), "祈福项链");
    }
}
