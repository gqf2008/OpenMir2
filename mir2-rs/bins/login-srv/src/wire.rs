//! 客户端面帧的解析与构造 —— LoginSrv 视角（`src/LoginSrv/Services/ClientSession.cs`）。
//!
//! 上行（网关透传的客户端原始帧）：`#1<EDCode(12B 头)><EDCode(体)>!`
//! C# 的解析链是「GB2312 解码整帧 → `EndsWith("!")` → `ArrestStringEx("#","!")`
//! → `sMsg[1..]`（丢掉字面量 `'1'`）→ 前 16 字符为头、其余为**仍编码的体**」；
//! 编码字符集恒在 `0x3C..=0x7B`（ASCII 域），故下面直接在**字节**上做同样的事。
//!
//! 下行：`'#' + EncodeMessage(头12B) + EncodeString(体) + "!$"`（帧尾 `!$` 是登录跳特有，
//! 见 `crates/protocol::frame` 的 A-4 说明）。`SM_NEEDUPDATE_ACCOUNT` 是**两段各自编码**
//! （`EncodeMessage + EncodeBuffer(userEntry)`），所以构造器按「段」入参。

use mir2_protocol::edcode;
use mir2_protocol::frame::CommandMessage;
use mir2_protocol::messages::DefBlockSize;

/// 解码后的一条上行请求：12B 头 + **仍是编码串**的体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientRequest {
    /// 12 字节头。
    pub head: CommandMessage,
    /// 体（`sData`，`EDCode.DeCodeString` 前的编码字符）。
    pub body_enc: Vec<u8>,
}

/// 解析上行帧（镜像 C# `ClientSession.ProcessUserMessage` 的裁剪链）。
///
/// 返回 `None` 表示该条**按 C# 语义直接丢弃**：帧尾不是 `!`、`#…!` 之间为空、
/// 或裁剪后不足 `DefBlockSize`（16）。
#[must_use]
pub fn parse_client_frame(raw: &[u8]) -> Option<ClientRequest> {
    if raw.last() != Some(&b'!') {
        return None;
    }
    let hash = raw.iter().position(|&b| b == b'#')?;
    let after = &raw[hash + 1..];
    let bang = after.iter().position(|&b| b == b'!')?;
    let inner = &after[..bang];
    if inner.is_empty() || inner.len() < usize::from(DefBlockSize) {
        return None;
    }
    // C# `sMsg = sMsg[1..]`：丢掉客户端帧的字面量 '1'
    let msg = &inner[1..];
    if msg.len() < usize::from(DefBlockSize) {
        return None;
    }
    let head_bytes = edcode::decode(&msg[..usize::from(DefBlockSize)]);
    let head = CommandMessage::from_bytes(&head_bytes)?;
    Some(ClientRequest {
        head,
        body_enc: msg[usize::from(DefBlockSize)..].to_vec(),
    })
}

/// 解码体（`EDCode.DeCodeString(sData)` → 明文 GB2312 字节）
///
/// C# 对**空串**会抛 `ArgumentNullException`（`EDCode.DeCodeString` 的 `IsNullOrEmpty` 检查），
/// 由调用方的 try/catch 吞噬 ⇒ 表现为"该条无响应"。这里返回 `None` 由调用方同样处理。
#[must_use]
pub fn decode_body(body_enc: &[u8]) -> Option<Vec<u8>> {
    if body_enc.is_empty() {
        return None;
    }
    Some(edcode::decode(body_enc))
}

/// 构造下行帧：`#` + `EncodeMessage(头)` + 各段 `EncodeString` + `!$`。
#[must_use]
pub fn build_response(head: CommandMessage, segments: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.push(b'#');
    out.extend_from_slice(&edcode::encode(&head.to_bytes()));
    for seg in segments {
        out.extend_from_slice(&edcode::encode(seg));
    }
    out.extend_from_slice(b"!$");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_protocol::messages;

    fn client_frame(ident: u16, recog: i32, body: &[u8]) -> Vec<u8> {
        let head = CommandMessage::make(i32::from(ident), recog, 0, 0, 0);
        let mut out = vec![b'#', b'1'];
        out.extend_from_slice(&edcode::encode(&head.to_bytes()));
        if !body.is_empty() {
            out.extend_from_slice(&edcode::encode(body));
        }
        out.push(b'!');
        out
    }

    /// 上级请求解析：头字段与体（体仍是编码串）都要对上。
    #[test]
    fn parses_client_frame_like_csharp() {
        let frame = client_frame(messages::CM_IDPASSWORD, 0, b"mir2test/mir2pass");
        let req = parse_client_frame(&frame).expect("应能解析");
        assert_eq!(req.head.ident, messages::CM_IDPASSWORD);
        assert_eq!(decode_body(&req.body_enc).unwrap(), b"mir2test/mir2pass");
    }

    /// 丢弃条件：非 `!` 结尾 / 空内容 / 不足 16 字节（C# 的 `return` 分支）。
    #[test]
    fn rejects_short_or_unterminated() {
        assert!(parse_client_frame(b"#1abc").is_none());
        assert!(parse_client_frame(b"#1!").is_none()); // 裁剪后不足 16
        assert!(parse_client_frame(b"#!").is_none());
        assert!(parse_client_frame(b"").is_none());
    }

    /// 下行帧字面量：与 C# 真身（`tests/parity/loginsrv/oracle-bytes.json`）逐字节相同。
    #[test]
    fn response_frame_matches_csharp_golden() {
        // SM_PASSWD_FAIL(503) recog=-1，空体
        let f = build_response(CommandMessage::make(503, -1, 0, 0, 0), &[]);
        assert_eq!(hex(&f), "2353534f4c5353695e6464686b6464686b2124");
        // SM_CERTIFICATION_SUCCESS(500)，空体
        let f = build_response(CommandMessage::make(500, 0, 0, 0, 0), &[]);
        assert_eq!(hex(&f), "236464686b6450696a6464686b6464686b2124");
        // SM_PASSOK_SELECTSERVER(529) series=1，体 = "热血传奇/0/"（GB2312 字节）
        let body = [b"\xc8\xc8\xd1\xaa\xb4\xab\xc6\xe6".as_slice(), b"/0/"].concat();
        let f = build_response(CommandMessage::make(529, 0, 0, 0, 1), &[&body]);
        assert_eq!(
            hex(&f),
            "236464686b64696a6b6464686b6465686b545479513e404342564e3f66605f482124"
        );
        // SM_SELECTSERVER_OK(530) recog=3，体 = "127.0.0.1/7100/3"
        let f = build_response(
            CommandMessage::make(530, 3, 0, 0, 0),
            &[b"127.0.0.1/7100/3"],
        );
        assert_eq!(
            hex(&f),
            "236764686b646a6a6b6464686b6464686b6162576b5e603e5f605e59685f63595e60603f6b633f2124"
        );
    }

    fn hex(b: &[u8]) -> String {
        use std::fmt::Write as _;
        b.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:02x}");
            s
        })
    }
}
