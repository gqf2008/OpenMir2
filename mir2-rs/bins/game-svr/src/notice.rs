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
//! **R1-1（oracle 口径修正）**：文件不存在、或读到的文本为空 ⇒ 返回**空字节**（不是 `None`）。
//! oracle `SendDefMessage(..., sMsg)` 的 `if (!string.IsNullOrEmpty(sMsg))` **只决定是否追加尾**：
//! `sMsg` 为空时走 `else SendSocket(ClientMsg)`（`PlayObject.Socket.cs:38-41`），
//! 而 `SendSocket(CommandMessage)` 照常写 12B 命令头（`:69-88`）⇒ **仍下发一条 header-only 的 658**。
//! 原先「空 ⇒ 不下发帧」把「不追加尾」误读成了「不发帧」，是口径误述（reviewer-1 `4f5005f3`）。
//!
//! **R1-2（1:1 搬 oracle 笔误）**：`SendNotice` 末尾是
//! `sNoticeMsg.Replace("/r/n/r/n ", "")`——**字面量正斜杠**（oracle 的笔误，本意应是 `\r\n\r\n `）。
//! 真实 `Notice.txt` 无该子串 ⇒ 逐字节不变；仍按「连 bug 一起搬」补上，见 [`NOTICE_REPLACE_FROM`]。

use std::path::Path;

/// `PlayObject.SendNotice` 的行分隔符：源码字面量 `"\x20\x1B"` + 裸 ESC = 3 字节。
pub const NOTICE_SEP: [u8; 3] = [0x20, 0x1B, 0x1B];

/// oracle `sNoticeMsg.Replace("/r/n/r/n ", "")` 的**字面量**（正斜杠，非 `\r\n`）。
///
/// `PlayObject.cs:1076` 原文如此，属 oracle 的笔误；本层 1:1 搬入（行为等价优先）。
/// 真实 `E:/MirServer/M2GameSvr/Notice/Notice.txt` 与金标准 #1 尾巴都**不含**该子串 ⇒ 该替换为 no-op。
pub const NOTICE_REPLACE_FROM: &[u8] = b"/r/n/r/n ";

/// 公告文件名（`NoticeMgr.GetNoticeMsg` 里恒为 `sStr + ".txt"`，调用点是 `"Notice"`）。
pub const NOTICE_FILE: &str = "Notice.txt";

/// 逐行滚动公告文件名（`GameApp.cs:204 M2Share.LoadLineNotice(GetNoticeFilePath("LineNotice.txt"))`）。
pub const LINE_NOTICE_FILE: &str = "LineNotice.txt";

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

/// 1:1 复刻 oracle `sNoticeMsg.Replace(NOTICE_REPLACE_FROM, "")`（字面量正斜杠；左到右、不重叠）。
#[must_use]
pub fn apply_notice_replace(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i..].starts_with(NOTICE_REPLACE_FROM) {
            i += NOTICE_REPLACE_FROM.len();
        } else {
            out.push(text[i]);
            i += 1;
        }
    }
    out
}

/// 原始 `Notice.txt` 字节 → 公告原文；空/缺 ⇒ **空字节**（oracle 仍发 header-only 658，见模块头 R1-1）。
#[must_use]
pub fn notice_from_bytes(text: &[u8]) -> Vec<u8> {
    apply_notice_replace(&join_notice(&split_read_lines(text)))
}

/// 读 `<base>/<notice_dir>/Notice.txt` 并产出公告原文；文件不存在/读失败 ⇒ 空字节。
///
/// `base` 等价 oracle `M2Share.BasePath`（生产下是 `Server.conf` 所在目录，
/// 见 `mapwire::base_path`）；`notice_dir` 是 `[Share] NoticeDir`（oracle 缺省 `Notice`）。
/// 读不到 ⇒ 空字节——与 oracle `LoadFromFile` 里 `if (!File.Exists) return;` 同构：
/// 读不到就当没有公告文本，而不是报错；但**帧仍要发**（空尾）。
#[must_use]
pub fn load_notice(base: &Path, notice_dir: &str) -> Vec<u8> {
    let path = base.join(notice_dir).join(NOTICE_FILE);
    match std::fs::read(&path) {
        Ok(text) => notice_from_bytes(&text),
        Err(_) => Vec::new(),
    }
}

/// 按 C# `StreamReader.ReadLine()` 语义在**已解码文本**上切行。
///
/// `\r\n`、`\n`、`\r` 均为行界；行界不进入返回的行；末尾的行界**不**产生额外空行。
/// 空输入 ⇒ 空 `Vec`。（与 [`split_read_lines`] 同语义，但作用在 `&str` 上——
/// `LineNotice` 走 `StringList.LoadFromFile` 解码后逐行 `.Trim()`，需先整文件解码再切行。）
fn split_read_lines_str(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                out.push(&text[start..i]);
                i += if bytes.get(i + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                start = i;
            }
            b'\n' => {
                out.push(&text[start..i]);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        out.push(&text[start..]);
    }
    out
}

/// 1:1 复刻 `M2Share.LoadLineNotice`（`src/M2Server/M2Share.cs:213-238`）的**文本半**：
/// 逐行 `Trim()`，**空行丢弃**（原实现 `LoadList.RemoveAt(i); continue;` 等价于过滤空行），
/// 其余按序保留。`result = true` 由「文件存在」决定（见 [`load_line_notice`]）。
///
/// 注：oracle 用 `StringList.LoadFromFile` 的 `StreamReader.ReadLine` 切行，
/// 故先对**整文件**做 BOM/GBK 判定再切行（见 [`load_line_notice`]）。
#[must_use]
pub fn parse_line_notice(text: &str) -> Vec<String> {
    split_read_lines_str(text)
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// 读 `<base>/<notice_dir>/LineNotice.txt` 并产出逐行公告（`GameApp.cs:204`）。
///
/// 文件不存在/读失败 ⇒ 空 `Vec`（oracle `File.Exists` 假 ⇒ 返回 `false`、不装载）。
/// 编码走 `StringList.LoadFromFile` 的判定（GBK 缺省，BOM 覆盖）——与 [`load_notice`] 同口径。
///
/// 消费点（**世界 tick 扩容首批 T3 · ③ 已接线**）：`WorldServer.cs:669-696` 的逐玩家定时滚动公告
/// —— `line_notice::roll` 按 per-session `ShowLineNoticeTick`/`ShowLineNoticeIdx` 推进，
/// 经 `BaseObject.SysMsg(sMsg, color, MsgType.Notice)` 的 `[`/`<`/`{`/`R`/`G`/`B` 分支出帧；
/// 宿主 `sim_loop` 每 tick 调 `drain_line_notice`（`ShowLineNoticeTime` 门 + `Idx` 回绕）。
/// `GetLineVariableText` 的 `<$…>` 变量替换未建模（登记见 `line_notice` 模块头）。
#[must_use]
pub fn load_line_notice(base: &Path, notice_dir: &str) -> Vec<String> {
    let path = base.join(notice_dir).join(LINE_NOTICE_FILE);
    match std::fs::read(&path) {
        Ok(bytes) => parse_line_notice(&mir2_envir::decode_text(&bytes)),
        Err(_) => Vec::new(),
    }
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

        let got = notice_from_bytes(&file);
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

    /// **R1-1**：空文本产出**空字节**（不是 `None`）——上层据此仍发 header-only 658。
    #[test]
    fn empty_notice_is_empty_bytes() {
        assert_eq!(notice_from_bytes(b""), Vec::<u8>::new());
        assert_eq!(join_notice(&[]), Vec::<u8>::new());
        assert_eq!(
            notice_from_bytes(b"\r\n"),
            b"\x20\x1b\x1b".to_vec(),
            "非空行仍要下发"
        );
    }

    /// **R1-2**：1:1 搬 oracle 的 `.Replace("/r/n/r/n ", "")`（**字面量正斜杠**，非 `\r\n`）。
    ///
    /// 真实 `Notice.txt` 无该子串 ⇒ 该用例是**唯一**能证明「搬了 bug」的判据：
    /// 去掉 [`apply_notice_replace`] ⇒ 本用例必红（红检见 `pi-m2svr-r1-*.log`）。
    #[test]
    fn literal_slash_r_n_replace_is_carried_over() {
        // `join_notice` 先按行 + `\x20\x1B\x1B` 拼，再整体 Replace。
        assert_eq!(
            notice_from_bytes(b"aa/r/n/r/n bb\r\n"),
            b"aabb\x20\x1b\x1b".to_vec(),
            "字面量 `/r/n/r/n `（带尾空格）必须被删掉"
        );
        // 只删「带尾空格的 9 字节」；无尾空格的字面量 `\r\n\r\n `（真实 CRLF）不动。
        // 注意：行尾分隔符首字节就是 0x20，所以「line 恰为 `/r/n/r/n`」会与分隔符拼成
        // `/r/n/r/n ` 而被删除——oracle 在同一拼接串上做 Replace，行为相同。
        assert_eq!(
            notice_from_bytes(b"/r/n/r/n\r\n"),
            b"\x1b\x1b".to_vec(),
            "分隔符首字节 0x20 被字面量吃掉 ⇒ 剩下裸 ESC\nESC（与 oracle 同）"
        );
        assert_eq!(
            apply_notice_replace(b"\r\n\r\n x"),
            b"\r\n\r\n x".to_vec(),
            "真实 CRLF 序列不在删除集中"
        );
    }

    /// 缺 `Notice.txt` ⇒ 不报错、返回**空字节**（oracle `File.Exists` 早退，但帧仍发）。
    #[test]
    fn missing_notice_file_is_empty() {
        let dir = std::env::temp_dir().join(format!("gs-notice-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Notice")).unwrap();
        assert_eq!(load_notice(&dir, "Notice"), Vec::<u8>::new());
        std::fs::write(dir.join("Notice").join("Notice.txt"), b"hello\r\n").unwrap();
        assert_eq!(load_notice(&dir, "Notice"), b"hello\x20\x1b\x1b".to_vec());
        std::fs::remove_dir_all(&dir).ok();
    }

    // ---- LineNotice（R0-4）：`M2Share.LoadLineNotice`（`M2Share.cs:213-238`）----

    /// `StringList.LoadFromFile` 的 ReadLine 语义（`\r\n`/`\n`/`\r` 均行界、末尾行界不产生空行）。
    #[test]
    fn line_notice_readline_semantics() {
        assert_eq!(split_read_lines_str(""), Vec::<&str>::new());
        assert_eq!(
            split_read_lines_str("a\r\nb\nc\rd"),
            vec!["a", "b", "c", "d"]
        );
        assert_eq!(
            split_read_lines_str("a\r\n"),
            vec!["a"],
            "末尾行界不产生空行"
        );
        assert_eq!(split_read_lines_str("a\r\n\r\n"), vec!["a", ""]);
    }

    /// 文本半：逐行 `Trim()` + 丢空行 + 保序（`M2Share.cs:226-235`）。
    #[test]
    fn line_notice_trims_and_drops_blank_lines() {
        let got = parse_line_notice("  甲  \r\n\r\n\t乙\t\r\n\n丙");
        assert_eq!(got, vec!["甲", "乙", "丙"], "trim + 丢空行");
        assert_eq!(parse_line_notice(""), Vec::<String>::new());
        assert_eq!(
            parse_line_notice("\r\n\r\n"),
            Vec::<String>::new(),
            "全空行 ⇒ 空"
        );
    }

    /// 文件半：GBK 字节解码（部署件形态：`[公告]X` 行）+ 缺文件 ⇒ 空。
    #[test]
    fn load_line_notice_decodes_gbk_and_missing_is_empty() {
        let dir = std::env::temp_dir().join(format!("gs-linenotice-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Notice")).unwrap();
        // 缺文件 ⇒ 空（oracle `File.Exists` 假 ⇒ 返回 false）。
        assert_eq!(load_line_notice(&dir, "Notice"), Vec::<String>::new());
        // `[公告]甲\r\n[公告]乙\r\n[公告]丙` 的 GBK 字节（无 BOM，同部署件）。
        let mut body: Vec<u8> = Vec::new();
        for tail in [[0xBCu8, 0xD7], [0xD2, 0xD2], [0xB1, 0xFB]] {
            body.extend_from_slice(&[0x5B, 0xB9, 0xAB, 0xB8, 0xE6, 0x5D]);
            body.extend_from_slice(&tail);
            body.extend_from_slice(b"\r\n");
        }
        std::fs::write(dir.join("Notice").join("LineNotice.txt"), &body).unwrap();
        assert_eq!(
            load_line_notice(&dir, "Notice"),
            vec!["[公告]甲", "[公告]乙", "[公告]丙"],
            "GBK 解码 + ReadLine 切行"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 文件半：UTF-8 BOM 覆盖（`StringList.GetEncoding` 的 BOM 判定）⇒ UTF-8 文本原样。
    #[test]
    fn load_line_notice_honours_utf8_bom() {
        let dir = std::env::temp_dir().join(format!("gs-linenotice-bom-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Notice")).unwrap();
        let mut body: Vec<u8> = vec![0xEF, 0xBB, 0xBF];
        body.extend_from_slice("[公告]测试\r\n".as_bytes());
        std::fs::write(dir.join("Notice").join("LineNotice.txt"), &body).unwrap();
        assert_eq!(load_line_notice(&dir, "Notice"), vec!["[公告]测试"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
