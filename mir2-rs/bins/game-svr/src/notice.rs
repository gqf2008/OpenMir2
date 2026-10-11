//! 公告文本加载 —— `NoticeMgr.GetNoticeMsg("Notice", ...)` 的 Rust 侧接缝
//! （`openmir2-m2-enter-world-frames` 顺序判据② 的 658 `SM_SENDNOTICE` 接线）。
//!
//! 判据来源是 M0 金标准的第一条 s2c 帧（`port=7200 && ident=658`）：把该帧的
//! 线缆尾用 [`mir2_protocol::edcode::decode`] 解回**公告原文**，就是本模块
//! [`load_notice`] 必须产出的字节（金标准实测 320B）。**不按源码公式重推文本**。
//!
//! 逐字节口径（照 oracle 源码，且已被金标准锚定）：
//! 1. `StringList.LoadFromFile`（`src/OpenMir2/Common/StringList.cs`）用
//!    `StreamReader.ReadLine()` 逐行读 `Notice.txt`（GBK 原文，本层**不转码**）⇒
//!    `\r\n` / `\n` / `\r` 都是行界，**文件末尾的换行不产生多出来的空行**
//!    （`while (sr.Peek() >= 0)`）；
//! 2. `PlayObject.SendNotice`（`src/M2Server/Player/PlayObject.cs:1064-1076`）逐行
//!    `sNoticeMsg = sNoticeMsg + loadList[i] + "\x20\x1B"`，其中字面量的第 3 个字节是
//!    源码里一个**裸 ESC**（该行字节实为 `"\x20\x1B\x1B"`）⇒ 分隔符三字节
//!    [`NOTICE_SEP`]，且**最后一行后也跟一个分隔符**。
//!
//! 文件不存在、或读到的文本为空 ⇒ 返回 `None`（oracle `SendDefMessage` 对空 `sMsg`
//! **不下发**，见 `PlayObject.Socket.cs:27-36` 的 `if (!string.IsNullOrEmpty(sMsg))`）。

use std::path::Path;

/// `PlayObject.SendNotice` 的行分隔符：源码字面量 `"\x20\x1B"` + 裸 ESC = 3 字节。
pub const NOTICE_SEP: [u8; 3] = [0x20, 0x1B, 0x1B];

/// 公告文件名（`NoticeMgr.GetNoticeMsg` 里恒为 `sStr + ".txt"`，调用点是 `"Notice"`）。
pub const NOTICE_FILE: &str = "Notice.txt";

/// 按 C# `StreamReader.ReadLine()` 语义切行。
///
/// `\r\n`、`\n`、`\r` 均为行界；行界**不**进入返回的行；末尾的换行**不**产生额外空行
/// （与 `Peek() >= 0` 循环一致）。空输入 ⇒ 空 `Vec`。
#[must_use]
pub fn split_read_lines(text: &[u8]) -> Vec<Vec<u8>> {
    let mut lines: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b'\r' => {
                lines.push(std::mem::take(&mut cur));
                // `\r\n` 是一个行界而不是两个（`ReadLine` 的 CRLF 处理）。
                i += if text.get(i + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
            }
            b'\n' => {
                lines.push(std::mem::take(&mut cur));
                i += 1;
            }
            b => {
                cur.push(b);
                i += 1;
            }
        }
    }
    // 仅当文件不以行界收尾时，最后一段才是有效行（末尾行界已在循环里 push 过）。
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// 逐行 `line + NOTICE_SEP` 拼接（含最后一行后的分隔符）。
#[must_use]
pub fn join_notice(lines: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(lines.iter().map(Vec::len).sum::<usize>() + lines.len() * 3);
    for line in lines {
        out.extend_from_slice(line);
        out.extend_from_slice(&NOTICE_SEP);
    }
    out
}

/// 原始 `Notice.txt` 字节 → 公告原文；空 ⇒ `None`（不下发）。
#[must_use]
pub fn notice_from_bytes(text: &[u8]) -> Option<Vec<u8>> {
    let joined = join_notice(&split_read_lines(text));
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// 读 `<base>/<notice_dir>/Notice.txt` 并产出公告原文。
///
/// `base` 等价 oracle `M2Share.BasePath`（生产下是 `Server.conf` 所在目录，
/// 见 `mapwire::base_path`）；`notice_dir` 是 `[Share] NoticeDir`（oracle 缺省 `Notice`）。
/// 文件不存在（或读失败）⇒ `None`——这与 oracle `LoadFromFile` 里 `if (!File.Exists) return;`
/// 同构：读不到就当没有公告，而不是报错。
#[must_use]
pub fn load_notice(base: &Path, notice_dir: &str) -> Option<Vec<u8>> {
    let path = base.join(notice_dir).join(NOTICE_FILE);
    let text = std::fs::read(&path).ok()?;
    notice_from_bytes(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 金标准第一条 s2c 帧（`port=7200 && ident=658`）的原始线缆字节。
    fn golden_658_raw() -> Vec<u8> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/golden/session-20261010-c3-hook/capture.jsonl"
        );
        let capture =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("金标准不存在：{path}（{e}）"));
        for line in capture.lines() {
            if !line.contains("\"dir\":\"s2c\"")
                || !line.contains("\"port\":7200")
                || !line.contains("\"ident\":658")
            {
                continue;
            }
            let i = line.find("\"raw_hex\":\"").expect("缺 raw_hex") + 11;
            let j = line[i..].find('"').expect("raw_hex 未闭合") + i;
            return (0..j - i)
                .step_by(2)
                .map(|k| u8::from_str_radix(&line[i + k..i + k + 2], 16).unwrap())
                .collect();
        }
        panic!("金标准里没有 7200/s2c 的 ident=658 帧");
    }

    /// 顺序判据② 的 658 锚点：`Notice.txt` 原文 → 公告原文 → 与金标准解出的尾巴逐字节相等。
    ///
    /// 同时回证「文件 → 文本」与「文本 → 金标准」两段口径：
    /// 1. 把金标准尾巴按 [`NOTICE_SEP`] 切回 19 行、用 `\r\n` 还原成 `Notice.txt` 内容；
    /// 2. [`notice_from_bytes`] 必须把该内容还原成**与金标准尾巴完全相同**的 320B。
    #[test]
    fn notice_roundtrip_matches_golden_658_tail() {
        let raw = golden_658_raw();
        assert_eq!(raw[0], b'#', "帧首必须是 '#'");
        assert_eq!(raw[raw.len() - 1], b'!', "帧尾必须是 '!'");
        let plain = mir2_protocol::edcode::decode(&raw[17..raw.len() - 1]);
        assert_eq!(plain.len(), 320, "金标准 658 的公告原文长度");
        assert!(
            plain.ends_with(&NOTICE_SEP),
            "公告原文必须以分隔符收尾（最后一行后也有分隔符）"
        );

        // 还原 Notice.txt：按分隔符切行（丢掉收尾分隔符产生的空尾巴），再用 CRLF 拼回。
        let mut chunks: Vec<&[u8]> = Vec::new();
        let mut start = 0usize;
        while start + NOTICE_SEP.len() <= plain.len() {
            let end = start
                + plain[start..]
                    .windows(NOTICE_SEP.len())
                    .position(|w| w == NOTICE_SEP)
                    .expect("公告原文里应有分隔符");
            chunks.push(&plain[start..end]);
            start = end + NOTICE_SEP.len();
        }
        assert!(start == plain.len(), "分隔符切分必须恰好覆盖全文");
        let mut file: Vec<u8> = Vec::new();
        for line in &chunks {
            file.extend_from_slice(line);
            file.extend_from_slice(b"\r\n");
        }

        let got = notice_from_bytes(&file).expect("有内容必须产出公告");
        assert_eq!(
            got, plain,
            "Notice.txt 原文 → 公告原文 必须与金标准 658 尾巴逐字节相等"
        );
    }

    /// `StreamReader.ReadLine` 语义锚点：CRLF/LF/CR 都是行界、末尾换行不产生空行。
    #[test]
    fn readline_semantics() {
        assert_eq!(split_read_lines(b""), Vec::<Vec<u8>>::new());
        assert_eq!(
            split_read_lines(b"a\r\nb\nc\rd"),
            vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]
        );
        assert_eq!(
            split_read_lines(b"a\r\n"),
            vec![b"a".to_vec()],
            "末尾 CRLF 不产生额外空行"
        );
        assert_eq!(
            split_read_lines(b"\r\n"),
            vec![Vec::<u8>::new()],
            "文件以 CRLF 开头 ⇒ 一个空行（现网 Notice.txt 形态）"
        );
        assert_eq!(split_read_lines(b"a\n\n"), vec![b"a".to_vec(), Vec::new()]);
    }

    /// 空文本不下发（oracle `SendDefMessage` 对空 `sMsg` 直接返回）。
    #[test]
    fn empty_notice_is_not_sent() {
        assert_eq!(notice_from_bytes(b""), None);
        assert_eq!(join_notice(&[]), Vec::<u8>::new());
        assert!(notice_from_bytes(b"\r\n").is_some(), "非空行仍要下发");
    }

    /// 缺 `Notice.txt` ⇒ 不报错、返回 `None`（照 oracle `LoadFromFile` 的 `File.Exists` 早退）。
    #[test]
    fn missing_notice_file_is_none() {
        let dir = std::env::temp_dir().join(format!("gs-notice-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Notice")).unwrap();
        assert_eq!(load_notice(&dir, "Notice"), None);
        std::fs::write(dir.join("Notice").join("Notice.txt"), b"hello\r\n").unwrap();
        assert_eq!(
            load_notice(&dir, "Notice"),
            Some(b"hello\x20\x1b\x1b".to_vec())
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
