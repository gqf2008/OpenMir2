//! `StringList.LoadFromFile` 的文件读取：BOM 嗅探 + GB2312(GBK) 默认解码 + `ReadLine` 切行。
//!
//! 参照 `src/OpenMir2/Common/StringList.cs` 的 `GetEncoding` / `LoadFromFile`：
//! - 有 BOM 按 BOM（UTF-8 / UTF-16LE / UTF-16BE），无 BOM 按 gb2312（.NET 实为 cp936≈GBK）；
//! - 解码失败字节用替换字符（.NET ReplacementFallback 与 encoding_rs 一致）；
//! - `StreamReader.ReadLine` 以 `\r\n` / `\r` / `\n` 切行，文件尾无换行符也返回最后一行。

use std::path::Path;

use crate::parser::ScriptFs;

pub fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (s, _, _) = encoding_rs::UTF_8.decode(&bytes[3..]);
        return s.into_owned();
    }
    // C# GetEncoding：UTF-16LE BOM 分支带 `byte3 != 0xFF` 守卫（FF FE FF 会回落 gb2312）
    if bytes.starts_with(&[0xFF, 0xFE]) && bytes.get(3) != Some(&0xFF) {
        let (s, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (s, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        return s.into_owned();
    }
    let (s, _, _) = encoding_rs::GBK.decode(bytes);
    s.into_owned()
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
    fn gbk_decode() {
        // "祈福项链" 的 GBK 字节
        let gbk = [0xC6u8, 0xED, 0xB8, 0xA3, 0xCF, 0xEE, 0xC1, 0xB4];
        assert_eq!(decode_bytes(&gbk), "祈福项链");
    }
}
