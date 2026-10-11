//! 5100 面的**纯字节层**：请求解析与应答组帧。
//!
//! 逐条对齐 C# `DBSrv/Services/Impl/UserService.cs`：
//! - `ProcessMessage`：把 `ServerDataMessage.Data[0..DataLen]` 当 GB2312 文本，用
//!   `ArrestStringEx(text, "#", "!")` 抠出一帧；**丢掉首字符**（客户端帧的 `1`）
//!   后，前 `Messages.DefBlockSize`(=16) 个字符是**编码后的 12 字节消息头**，其余是编码后的体；
//! - `DeCodeUserMsg`：`sDefMsg = sData[..16]`、`sText = sData[16..]`，各自独立解码
//!   （12 % 3 == 0，所以分开编解码与整体编解码等价，见 `crates/protocol` 的帧注释）；
//! - `SendUserSocket`：回包体是 `"#" + EncodeMessage(msg) + <已编码的体> + "!"`，
//!   正好是 `mir2_protocol::frame::encode_server_frame`（帧尾 `!`）。

use mir2_protocol::edcode;
use mir2_protocol::frame::{encode_server_frame, CommandMessage};
use mir2_protocol::messages::DefBlockSize;

use crate::gbk::split2_nonempty;

/// 客户端消息头编码后的字符数（C# `Messages.DefBlockSize` = 16）。编码字符集是
/// `0x3C..=0x7B`，全是 ASCII ⇒ 字节数等于字符数，可在字节上直接切。
pub const DEF_BLOCK_SIZE: usize = DefBlockSize as usize;

/// C# `HUtil32.GetString(packet.Data, 0, packet.DataLen)` 的长度口径：`DataLen` 是
/// **独立字段**（可与 `Data.len()` 不等）；`DataLen > Data.len()` 或为负在 C# 里抛
/// `ArgumentOutOfRangeException`（被上层吞掉、该条丢弃）⇒ 这里返回 `None` 让调用方跳过。
#[must_use]
pub fn data_len(data_len: i16, cap: usize) -> Option<usize> {
    let n = usize::try_from(data_len).ok()?;
    if n > cap {
        return None;
    }
    Some(n)
}

/// 一条客户端消息：12 字节头（已解码）+ **仍是编码态**的体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientPacket {
    /// 解码后的消息头。
    pub head: CommandMessage,
    /// 体（编码态，按消息号各自的处理方式再解码）。
    pub body: Vec<u8>,
}

/// 解析一条「`#1…!` 抠出、去掉首字符」的载荷（`DeCodeUserMsg` 的前两步）。
///
/// 返回 `Err(len)` 表示载荷不足 `DefBlockSize`（C# 侧上游有同样的长度守卫，等于跳过）。
pub fn decode_client_packet(data: &[u8]) -> Result<ClientPacket, usize> {
    if data.len() < DEF_BLOCK_SIZE {
        return Err(data.len());
    }
    let head_bytes = edcode::decode(&data[..DEF_BLOCK_SIZE]);
    let Some(head) = CommandMessage::from_bytes(&head_bytes) else {
        return Err(data.len());
    };
    Ok(ClientPacket {
        head,
        body: data[DEF_BLOCK_SIZE..].to_vec(),
    })
}

/// 组一条下行帧（`SendUserSocket`：`#` + 编码头 + 已编码体 + `!`）。
#[must_use]
pub fn encode_user_frame(head: &CommandMessage, encoded_body: &[u8]) -> Vec<u8> {
    encode_server_frame(head, encoded_body)
}

/// `SM_QUERYCHR` 请求：体解码后是 `账号/会话号`。
///
/// 会话号按 `HUtil32.StrToInt(..., -2)` 解析（失败＝-2，见 C# `QueryChr`）。
#[must_use]
pub fn query_chr_request(encoded_body: &[u8]) -> (Vec<u8>, i32) {
    let plain = edcode::decode(encoded_body);
    let (account, rest) = split2_nonempty(&plain, b"/");
    (account, crate::gbk::str_to_int(&rest, -2))
}

/// `CM_SELCHR` / `CM_DELCHR` 请求：解码后按 `账号/角色名` 或 `角色名` 取。
///
/// 返回 `(账号, 角色名)`；`CM_DELCHR` 的请求体没有账号字段（C# 直接整串当角色名），
/// 由调用方自行决定要不要取 `account`。
#[must_use]
pub fn account_and_name(encoded_body: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let plain = edcode::decode(encoded_body);
    let (account, name) = split2_nonempty(&plain, b"/");
    (account, name)
}

/// `CM_NEWCHR` 请求（`NewChr`）：解码后依次是 `账号/角色名/发型/职业/性别`，剩下的余量必须为空。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChrReq {
    /// 账号。
    pub account: Vec<u8>,
    /// 角色名（**未 Trim**，C# 后面才 `Trim()`）。
    pub name: Vec<u8>,
    /// 发型字段原始字符串。
    pub hair: Vec<u8>,
    /// 职业字段原始字符串。
    pub job: Vec<u8>,
    /// 性别字段原始字符串。
    pub sex: Vec<u8>,
    /// 第 5 次切分后的余量（非空 ⇒ C# 判 nCode=0 直接失败）。
    pub rest: Vec<u8>,
}

/// 解析 `CM_NEWCHR` 请求体（连续 5 次 `GetValidStr3(..., '/')`）。
#[must_use]
pub fn new_chr_request(encoded_body: &[u8]) -> NewChrReq {
    let plain = edcode::decode(encoded_body);
    let (account, rest) = split2_nonempty(&plain, b"/");
    let (name, rest) = split2_nonempty(&rest, b"/");
    let (hair, rest) = split2_nonempty(&rest, b"/");
    let (job, rest) = split2_nonempty(&rest, b"/");
    let (sex, rest) = split2_nonempty(&rest, b"/");
    NewChrReq {
        account,
        name,
        hair,
        job,
        sex,
        rest,
    }
}

/// 角色列表里的一项：`(名字的 GBK 字节, 职业, 发型, 等级, 性别, 是否选中)`。
pub type ChrItem = (Vec<u8>, u8, u8, u16, u8, bool);

/// 角色列表应答体（`QueryChr` 拼串）：每项 `名字/职业/发型/等级/性别/`，
/// `SelectID == 1` 的角色前面加 `*`。
#[must_use]
pub fn query_chr_body(items: &[ChrItem]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, job, hair, level, sex, selected) in items {
        if *selected {
            out.push(b'*');
        }
        out.extend_from_slice(name);
        out.push(b'/');
        out.extend_from_slice(job.to_string().as_bytes());
        out.push(b'/');
        out.extend_from_slice(hair.to_string().as_bytes());
        out.push(b'/');
        out.extend_from_slice(level.to_string().as_bytes());
        out.push(b'/');
        out.extend_from_slice(sex.to_string().as_bytes());
        out.push(b'/');
    }
    out
}

/// `HUtil32.IsEnglishStr`：C# 里是"**存在**任一英文/数字字符即为真"（循环里命中就 break），
/// 只在 `[Setup] EnglishNameOnly=1` 时生效。
#[must_use]
pub fn is_english_str(name: &[u8]) -> bool {
    name.iter()
        .any(|b| b.is_ascii_alphabetic() || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_protocol::messages::CM_QUERYCHR;

    /// 造一条客户端帧：`#1` + `enc(12B 头)` + `enc(体)` + `!`（客户端形态）。
    fn client_frame(head: CommandMessage, body: &[u8]) -> Vec<u8> {
        mir2_protocol::frame::encode_client_frame(&head, body)
    }

    /// `DeCodeUserMsg` 的切分口径：抠出的内容去掉首字符 `1` 后按 16/其余切。
    #[test]
    fn splits_head_and_body_like_csharp() {
        let head = CommandMessage::make(i32::from(CM_QUERYCHR), 0, 0, 0, 0);
        let body = edcode::encode(b"mir2test/1234");
        let frame = client_frame(head, b"mir2test/1234");
        assert_eq!(frame[0], b'#');
        assert_eq!(frame[1], b'1');
        assert_eq!(*frame.last().unwrap(), b'!');
        // C# `ArrestStringEx(sText, "#", "!")` 取的是 '#' 与 '!' **之间**（含首字节 '1'），
        // 随后 `sData = sData[1..]` 丢掉那个 '1' ⇒ 载荷 = 编码头 + 编码体
        let between = &frame[1..frame.len() - 1];
        let payload = &between[1..];
        let pkt = decode_client_packet(payload).unwrap();
        assert_eq!(pkt.head.ident, CM_QUERYCHR);
        assert_eq!(pkt.body, body);
        let (account, session) = query_chr_request(&pkt.body);
        assert_eq!(account, b"mir2test".to_vec());
        assert_eq!(session, 1234);
    }

    /// 不足 16 字符的载荷按 C# 的长度守卫丢弃。
    #[test]
    fn short_payload_is_rejected() {
        assert!(decode_client_packet(&[b'a'; 15]).is_err());
        assert!(decode_client_packet(&[]).is_err());
    }

    /// `CM_SELCHR` 体是 `账号/角色名`；`CM_NEWCHR` 体是 5 段 + 余量。
    #[test]
    fn request_bodies() {
        let (acct, name) = account_and_name(&edcode::encode(b"mir2test/aaa"));
        assert_eq!(acct, b"mir2test".to_vec());
        assert_eq!(name, b"aaa".to_vec());
        assert!(
            decode_client_packet(&[0u8; 16]).is_ok(),
            "16 字节恰好可解（头解出全 0）"
        );
        let r = new_chr_request(&edcode::encode(b"mir2test/new1/1/2/0"));
        assert_eq!(r.account, b"mir2test".to_vec());
        assert_eq!(r.name, b"new1".to_vec());
        assert_eq!(r.hair, b"1".to_vec());
        assert_eq!(r.job, b"2".to_vec());
        assert_eq!(r.sex, b"0".to_vec());
        assert!(r.rest.is_empty());
    }

    /// 角色列表体拼串（含 `*` 前导与字段顺序）。
    #[test]
    fn query_chr_body_layout() {
        let items = vec![
            (b"aaa".to_vec(), 0u8, 1u8, 1u16, 0u8, true),
            (b"bbb".to_vec(), 2, 1, 1, 0, false),
        ];
        assert_eq!(
            query_chr_body(&items),
            b"*aaa/0/1/1/0/bbb/2/1/1/0/".to_vec()
        );
        assert!(query_chr_body(&[]).is_empty());
    }

    /// 下行帧形态：`#` + 16 字符头 + 体 + `!`（与 C# `SendUserSocket` 一字不差）。
    #[test]
    fn response_frame_layout() {
        let head = CommandMessage::make(520, 2, 0, 1, 0);
        let body = edcode::encode(b"*aaa/0/1/1/0/bbb/2/1/1/0/");
        let frame = encode_user_frame(&head, &body);
        assert_eq!(frame[0], b'#');
        assert_eq!(*frame.last().unwrap(), b'!');
        let content = &frame[1..frame.len() - 1];
        assert_eq!(content.len(), 16 + body.len());
        // 头 16 字符解回原消息头
        assert_eq!(
            CommandMessage::from_bytes(&edcode::decode(&content[..16])),
            Some(head)
        );
        assert_eq!(
            edcode::decode(&content[16..]),
            b"*aaa/0/1/1/0/bbb/2/1/1/0/".to_vec()
        );
    }

    /// 选服/选角请求里 `Trim` 与 `IsEnglishStr` 的既有口径。
    #[test]
    fn name_helpers() {
        assert_eq!(crate::gbk::trim_name(b"  blood  "), b"blood".to_vec());
        assert_eq!(crate::gbk::trim_name(b"\xa1\xa1ab\xa1\xa1"), b"ab".to_vec());
        assert!(is_english_str(b"1a"));
        assert!(!is_english_str(b""));
        assert!(!is_english_str(&[0xD1, 0xAA]));
    }
}
