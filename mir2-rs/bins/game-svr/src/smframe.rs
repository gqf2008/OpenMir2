//! M2 **S2b-4 第一片**：GameSvr → 客户端的 `SM_*` 入图帧**编码层**（`openmir2-m2-enter-world-frames`）。
//!
//! 判据来源是 M0 金标准 `tests/golden/session-20261010-c3-hook/capture.jsonl`（真 C# 服务端 + 真客户端，
//! `port=7200 && dir=s2c` 共 329 条）。这里先把**两条字节流水线**逐字节钉住，不掺任何"按公式重推"：
//!
//! ```text
//! GameSvr 侧（oracle PlayObject.Socket.cs:101-116 的 SendSocket(defMsg, sMsg)）：
//!   GM_DATA.body = CommandMessage(12B, 解码态) ++ 尾
//!   其中"尾"由各消息自己决定：多为 EDCode 编码后的 role body，少数是原样字面量（如 "<<<<<<"）。
//! 网关侧（oracle GameGate ClientSession.cs:897-920 的 ProcessServerPacket）：
//!   client = '#' ++ Encode(头 12B) ++ 尾(原样) ++ '!'
//! ```
//!
//! **为什么 `Encode(头)` 与"尾原样"能拼**：`encoded_len(12) == 16` 且 `16 % 4 == 0`，
//! 所以 `Encode` 的状态在头边界复位，网关只编码头、尾原样追加，客户端整段解码后仍然正确
//! （`mir2-rs/crates/protocol/src/edcode.rs` 的 `header_encodes_to_multiple_of_4` 锚这条）。
//!
//! **本片已逐字节对上的 5 条**（金标准帧号）：SM_NEWMAP(51, #2)、SM_LOGON(50, #3)、
//! SM_FEATURECHANGED(41, #4)、SM_ATTACKMODE(213, #5)、SM_VERSION_FAIL(1106, #11)。
//! **本片未做**（登记在卡）：其余 11 条（658/11029/42/766/54/5008/52/752/811/46/10）——
//! 它们的尾取自玩家记录/地图/配置（`MessageBodyWL` 之外还有 Abil/物品/技能等），
//! 要等真地图（`openmir2-m2-map-loading`）与记录字段解析补齐；接线进 sim 循环同样登记。
#![allow(dead_code)] // S2b-4 后续接线前先入库（本文件 #[cfg(test)] 已在用）

use mir2_protocol::{edcode, messages};

/// `HUtil32.MakeWord(low, high)`（`src/OpenMir2/HUtil32.cs:61-64`）：`(high << 8) | low`。
#[must_use]
pub fn make_word(low: u8, high: u8) -> u16 {
    (u16::from(high) << 8) | u16::from(low)
}

/// `CommandMessage`（`ClientMesaagePacket.cs:11-24`）的 12B 解码态布局：
/// `Recog(i32) + Ident(u16) + Param(u16) + Tag(u16) + Series(u16)`，小端。
#[must_use]
pub fn command_header(recog: i32, ident: u16, param: u16, tag: u16, series: u16) -> [u8; 12] {
    let mut b = [0u8; 12];
    b[0..4].copy_from_slice(&recog.to_le_bytes());
    b[4..6].copy_from_slice(&ident.to_le_bytes());
    b[6..8].copy_from_slice(&param.to_le_bytes());
    b[8..10].copy_from_slice(&tag.to_le_bytes());
    b[10..12].copy_from_slice(&series.to_le_bytes());
    b
}

/// `GM_DATA` 的 body = 12B 命令头 ++ 尾（尾的编码口径由各 `sm_*` 决定，见文件头）。
#[must_use]
pub fn gm_data_body(header: [u8; 12], tail: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(12 + tail.len());
    v.extend_from_slice(&header);
    v.extend_from_slice(tail);
    v
}

/// 网关把 `GM_DATA` body 变成客户端线缆字节（`GameGate/Services/ClientSession.cs:897-920`）：
/// `'#' ++ Encode(头 12B) ++ 尾(原样) ++ '!'`。
///
/// # Panics
/// body 不足 12B 时 panic（调用方是自家构造器，恒 >= 12）。
#[must_use]
pub fn client_frame(gm_data: &[u8]) -> Vec<u8> {
    assert!(gm_data.len() >= 12, "GM_DATA body 必须含 12B 命令头");
    let mut out = Vec::with_capacity(gm_data.len() + 6);
    out.push(b'#');
    out.extend_from_slice(&edcode::encode(&gm_data[..12]));
    out.extend_from_slice(&gm_data[12..]);
    out.push(b'!');
    out
}

/// `SM_NEWMAP`（`PlayObject.Message.cs:1681` 的 `RM_LOGON` 支）：
/// `MakeMessage(SM_NEWMAP, ActorId, CurrX, CurrY, DayBright())` + 尾 `EDCode.EncodeString(MapFileName)`。
#[must_use]
pub fn sm_newmap(
    recog: i32,
    cur_x: u16,
    cur_y: u16,
    day_bright: u16,
    map_file_name: &str,
) -> Vec<u8> {
    let header = command_header(recog, messages::SM_NEWMAP, cur_x, cur_y, day_bright);
    gm_data_body(header, &edcode::encode(map_file_name.as_bytes()))
}

/// `SM_LOGON`（`PlayObject.cs:1130-1140` 的 `SendLogon`）：
/// `MakeMessage(SM_LOGON, ActorId, CurrX, CurrY, MakeWord(Dir, Light))` +
/// 尾 `EDCode.EncodePacket(MessageBodyWL)`（4×i32 = 16B）。
#[must_use]
pub fn sm_logon(
    recog: i32,
    cur_x: u16,
    cur_y: u16,
    dir: u8,
    light: u8,
    body_wl: [u8; 16],
) -> Vec<u8> {
    let header = command_header(
        recog,
        messages::SM_LOGON,
        cur_x,
        cur_y,
        make_word(dir, light),
    );
    gm_data_body(header, &edcode::encode(&body_wl))
}

/// `SM_FEATURECHANGED`（`PlayObject.cs:1145`）：
/// `MakeMessage(SM_FEATURECHANGED, ActorId, LoWord(nRecog), HiWord(nRecog), GetFeatureEx())`，无尾。
#[must_use]
pub fn sm_featurechanged(recog: i32, n_recog: i32, feature_ex: u16) -> Vec<u8> {
    let header = command_header(
        recog,
        messages::SM_FEATURECHANGED,
        lo_word(n_recog),
        hi_word(n_recog),
        feature_ex,
    );
    gm_data_body(header, &[])
}

/// `SM_ATTACKMODE`（`PlayObject.cs:1146`）：`SendDefMessage(SM_ATTACKMODE, (byte)AttatckMode, 0, 0, 0)`，无尾。
#[must_use]
pub fn sm_attackmode(recog: i32, mode: u8) -> Vec<u8> {
    let header = command_header(recog, messages::SM_ATTACKMODE, u16::from(mode), 0, 0);
    gm_data_body(header, &[])
}

/// `SM_VERSION_FAIL`（`PlayObject.Message.cs:1690-1692`）：
/// `MakeMessage(SM_VERSION_FAIL, ClientFile1_CRC, LoWord(ClientFile2_CRC), HiWord(ClientFile2_CRC), 0)`
/// + 尾 `"<<<<<<"`（**原样字面量**，不是 EDCode 编码——金标准里捕获工具把它解码成了垃圾，但这 6 字节在线缆上就是原样）。
#[must_use]
pub fn sm_version_fail(recog: i32, crc2: i32) -> Vec<u8> {
    let header = command_header(
        recog,
        messages::SM_VERSION_FAIL,
        lo_word(crc2),
        hi_word(crc2),
        0,
    );
    gm_data_body(header, b"<<<<<<")
}

/// `HUtil32.LoWord`（低 16 位）。
#[must_use]
pub fn lo_word(v: i32) -> u16 {
    u16::try_from(v & 0xFFFF).unwrap_or(0)
}

/// `HUtil32.HiWord`（高 16 位）。
#[must_use]
pub fn hi_word(v: i32) -> u16 {
    u16::try_from((v >> 16) & 0xFFFF).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// 金标准 `capture.jsonl`（7200/s2c）里这 5 条的**原始客户端线缆字节**（含 `#`/`!`）。
    /// 这里按原样钉进用例；`golden_constants_are_present_in_capture` 再回读文件核对，防止抄错/漂移。
    const G_NEWMAP: &str = "23486158617063685f6b65426f6664686b603f21";
    const G_LOGON: &str =
        "23486158617062685f6b65426f6664686b64646a6b6464686b6464686b6464686b6464686b643f21";
    const G_FEATURECHANGED: &str = "2348615861705d685d64646a6b6464686b21";
    const G_ATTACKMODE: &str = "236464686b6459686a6464686b6464686b21";
    const G_VERSION_FAIL: &str = "236464686b647a646b6464686b6464686b3c3c3c3c3c3c21";

    /// 该会话的 `ActorId`（金标准里 5 条帧的 `Recog` 都一样，属"会话相关随机 Id"，归一化时固定成这个值）。
    const G_ACTOR_ID: i32 = 0x7c30_3998;

    /// ① 一条端到端（编码层）：`SM_NEWMAP` 与金标准 #2 逐字节相等。
    #[test]
    fn sm_newmap_matches_golden_frame_2() {
        // 金标准：CurrX=287 / CurrY=618 / DayBright=0 / MapFileName="0"
        let body = sm_newmap(G_ACTOR_ID, 287, 618, 0, "0");
        assert_eq!(body.len(), 12 + edcode::encoded_len(1));
        assert_eq!(client_frame(&body), hex(G_NEWMAP));
    }

    /// ①：`SM_LOGON` 与金标准 #3 逐字节相等（`MessageBodyWL` = 金标准解出的 16B）。
    #[test]
    fn sm_logon_matches_golden_frame_3() {
        let wl: [u8; 16] = hex("00000200000000000000000000000000").try_into().unwrap();
        let body = sm_logon(G_ACTOR_ID, 287, 618, 0, 0, wl);
        assert_eq!(body.len(), 12 + edcode::encoded_len(16));
        assert_eq!(client_frame(&body), hex(G_LOGON));
    }

    /// `SM_FEATURECHANGED` 与金标准 #4 逐字节相等（`nRecog = GetFeatureToLong() = 0x0002_0000`）。
    #[test]
    fn sm_featurechanged_matches_golden_frame_4() {
        let body = sm_featurechanged(G_ACTOR_ID, 0x0002_0000, 0);
        assert_eq!(body.len(), 12, "无尾 ⇒ PackLength 恰好 12");
        assert_eq!(client_frame(&body), hex(G_FEATURECHANGED));
    }

    /// `SM_ATTACKMODE` 与金标准 #5 逐字节相等（AttatckMode=0）。
    #[test]
    fn sm_attackmode_matches_golden_frame_5() {
        let body = sm_attackmode(0, 0);
        assert_eq!(body.len(), 12);
        assert_eq!(client_frame(&body), hex(G_ATTACKMODE));
    }

    /// `SM_VERSION_FAIL` 与金标准 #11 逐字节相等：尾是原样 `"<<<<<<"`（6B），不是 EDCode 编码。
    #[test]
    fn sm_version_fail_matches_golden_frame_11() {
        let body = sm_version_fail(0, 0);
        assert_eq!(body.len(), 12 + 6);
        assert_eq!(client_frame(&body), hex(G_VERSION_FAIL));
        // 钉住"尾是原样"：客户端线缆里应出现 6 个 0x3c，而不是它们的 EDCode 编码。
        assert!(client_frame(&body).windows(6).any(|w| w == b"<<<<<<"));
    }

    /// 头 12B 的字节序锚点（改坏字段顺序必红）：`Recog` 小端在前 4B，接着 Ident/Param/Tag/Series。
    #[test]
    fn command_header_layout_is_little_endian() {
        let h = command_header(0x0102_0304, 0x0506, 0x0708, 0x090A, 0x0B0C);
        assert_eq!(h.as_slice(), hex("04030201060508070a090c0b").as_slice());
    }

    /// 回读 M0 金标准文件，核对上面 5 个常量确实是其原始字节（防抄错、防漂移）。
    ///
    /// 文件路径从 `CARGO_MANIFEST_DIR`（`mir2-rs/bins/game-svr`）上溯到仓库根的 `tests/golden/...`。
    #[test]
    fn golden_constants_are_present_in_capture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/golden/session-20261010-c3-hook/capture.jsonl"
        );
        let Ok(capture) = std::fs::read_to_string(path) else {
            panic!("金标准不存在：{path}（本测试要求它在仓库内，不许静默跳过）");
        };
        for (name, raw) in [
            ("SM_NEWMAP", G_NEWMAP),
            ("SM_LOGON", G_LOGON),
            ("SM_FEATURECHANGED", G_FEATURECHANGED),
            ("SM_ATTACKMODE", G_ATTACKMODE),
            ("SM_VERSION_FAIL", G_VERSION_FAIL),
        ] {
            assert!(
                capture.contains(&format!("\"raw_hex\":\"{raw}\"")),
                "{name} 的常量 {raw} 不在 M0 金标准里（抄错或金标准已漂移）"
            );
        }
    }
}
