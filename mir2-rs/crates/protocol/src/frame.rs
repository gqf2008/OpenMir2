//! 帧格式 —— Mir2 线缆帧与内部帧。
//!
//! ## 客户端 ↔ 网关（LoginGate:7000 / SelGate:7100 / GameGate:7200）
//!
//! - 客户端→服务端：`'#' '1' <EDCode(头12B) EDCode(体)> '!'`
//!   （Delphi 客户端 `TfrmMain.SendSocket` 的 `Format('#1%s!')`，'1' 是字面量；
//!   头与体**分别编码后拼接**——12 % 3 == 0 使拼接点恰在编码状态机初态，故整体解码等价）。
//! - 服务端→客户端：`'#' <EDCode(头12B)> <已编码的体> '!'`（无 '1'；
//!   体由 GameSvr 侧 `EDCode.EncodeString` 预先编码，网关只编码头后原样拼接，
//!   见 `GameGate/Services/ClientSession.cs` `ProcessServerPacket`）。
//!   **登录跳例外**：LoginSrv 构造的帧尾是 `'!$'`（`LoginSrv/Services/ClientSession.cs:745`
//!   的 `"#" + sMsg + "!$"`），LoginGate 原样转发 ⇒ 客户端在 7000 端口收到的帧以 `!$` 结尾；
//!   而 LoginGate 自己产生的帧（如超时 `SM_OUTOFCONNECTION`）仍是 `'!'`
//!   ⇒ **同一跳两种帧尾并存**，用 [`ServerFrameTail`] 区分，回放必须原样复现。
//! - 心跳：服务端下发单字节 `'*'`，客户端回单字节 `'*'`；网关收到 1 字节包直接忽略。
//! - C# 网关侧**没有显式分帧器**，按 TCP 段直读（`ServerSocketClientRead` 整段即一帧）；
//!   粘包时 C# 会解析出垃圾（既有行为）。Rust 侧提供 [`FrameSplitter`] 显式分帧，
//!   属必要的传输层健壮性差异，不是协议差异。
//!
//! ## 网关 ↔ 引擎/内部服务（TCP，固定头）
//!
//! - [`ServerMessage`]（20B，GameGate↔GameSvr 等）：`PackLength` 为负表示载荷已是
//!   编码完成的文本（GameSvr `SendSocket(string)` 路径），取绝对值为长度。
//! - [`ServerDataPacket`]（6B 头，LoginGate↔LoginSrv 等）。

use crate::edcode;
use std::fmt;

/// C# `Messages.DefBlockSize`（经典 TCmdPack 时代遗留；C# 侧 GateShare.CommandFixedLength
/// 也是 16。**注意它与 [`CommandMessage::SIZE`]=12 不一致**：C# 个别冷门分支按 16 编码
/// 12 字节结构体会越界抛异常（如 SM_EAT_FAIL 限频分支），属 C# 侧既有缺陷，勿据此
/// 推断线上头是 16 字节——线上头以客户端 `TCmdPack`/`CommandMessage` 为准，恒 12 字节）。
pub const DEF_BLOCK_SIZE: usize = 16;

/// 心跳字节（服务端下发 / 客户端应答均为单字节 `'*'`）。
pub const HEARTBEAT: u8 = b'*';

/// 客户端→服务端帧第 2 字节的字面量（`#1...!` 中的 '1'）。
pub const CLIENT_FRAME_TAG: u8 = b'1';

/// 客户端消息头，线上 12 字节小端：
/// `Recog(i32) | Ident(u16) | Param(u16) | Tag(u16) | Series(u16)`。
///
/// 对应 C# `Packets/ClientPackets/ClientMesaagePacket.cs` 的 `CommandMessage`
/// （MemoryPack 对无非托管字段结构体做原始内存序列化，等价 `LayoutKind.Sequential, Pack=1`）。
///
/// 字段语义（`Messages.MakeMessage` / Delphi `MakeDefaultMsg`）：
/// `SendDefMessage(ident, nRecog, nParam, nTag, nSeries)` ⇒ 模式、ID 这类值通常在 `recog`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandMessage {
    /// 承载模式/对象ID 等（`SendDefMessage` 的第 2 参）。
    pub recog: i32,
    /// 消息号（[`crate::messages`] 表内值）。
    pub ident: u16,
    /// 参数字段（常为 X 坐标/低值参数）。
    pub param: u16,
    /// 标签字段（常为 Y 坐标/方向）。
    pub tag: u16,
    /// 序列字段（常为方向/扩展参数）。
    pub series: u16,
}

impl CommandMessage {
    /// 线上头部长度：12 字节。
    pub const SIZE: usize = 12;

    /// 等价 C# `Messages.MakeMessage`：参数按 `(ushort)` 截断（既有行为，值域超 u16 会截断）。
    #[must_use]
    // 截断即语义（镜像 C# `(ushort)` 强转），不是疏忽。
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn make(ident: i32, recog: i32, param: i32, tag: i32, series: i32) -> Self {
        Self {
            recog,
            ident: ident as u16,
            param: param as u16,
            tag: tag as u16,
            series: series as u16,
        }
    }

    /// 序列化为线上 12 字节（小端）。
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..4].copy_from_slice(&self.recog.to_le_bytes());
        b[4..6].copy_from_slice(&self.ident.to_le_bytes());
        b[6..8].copy_from_slice(&self.param.to_le_bytes());
        b[8..10].copy_from_slice(&self.tag.to_le_bytes());
        b[10..12].copy_from_slice(&self.series.to_le_bytes());
        b
    }

    /// 从线上字节解析；不足 12 字节返回 `None`。
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < Self::SIZE {
            return None;
        }
        Some(Self {
            recog: i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ident: u16::from_le_bytes([b[4], b[5]]),
            param: u16::from_le_bytes([b[6], b[7]]),
            tag: u16::from_le_bytes([b[8], b[9]]),
            series: u16::from_le_bytes([b[10], b[11]]),
        })
    }
}

/// 解码后的一条客户端消息：12 字节头 + 明文体（体可能为空）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientMessage {
    /// 12 字节消息头。
    pub head: CommandMessage,
    /// 明文体（GB2312 字节流，本层不转码）。
    pub body: Vec<u8>,
}

/// 帧解析错误。EDCode 本身不报错（坏输入产垃圾字节），错误只来自帧定界与长度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// 不是 `#` 开头。
    MissingLead,
    /// 客户端帧缺 `'1'` 标签字节。
    MissingClientTag,
    /// 缺 `!` 结尾。
    MissingTail,
    /// 解码后不足 12 字节消息头。
    ShortHeader {
        /// 实际解码出的字节数。
        decoded_len: usize,
    },
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLead => write!(f, "frame does not start with '#'"),
            Self::MissingClientTag => write!(f, "client frame missing '1' tag byte"),
            Self::MissingTail => write!(f, "frame does not end with '!'"),
            Self::ShortHeader { decoded_len } => {
                write!(f, "decoded payload {decoded_len} < 12-byte header")
            }
        }
    }
}

impl std::error::Error for FrameError {}

/// 解析一帧**客户端→服务端**报文（输入含 `#1` 与 `!` 定界符的完整一帧）。
///
/// 镜像 C# 网关 `destinationSpan[2..^1]` + 整体解码（`EncryptUtil.DecodeSpan`）。
///
/// # Errors
/// 帧定界符缺失或解码后不足 12 字节头时返回 [`FrameError`]。
pub fn decode_client_frame(frame: &[u8]) -> Result<ClientMessage, FrameError> {
    if frame.first() != Some(&b'#') {
        return Err(FrameError::MissingLead);
    }
    if frame.get(1) != Some(&CLIENT_FRAME_TAG) {
        return Err(FrameError::MissingClientTag);
    }
    if frame.last() != Some(&b'!') {
        return Err(FrameError::MissingTail);
    }
    let decoded = edcode::decode(&frame[2..frame.len() - 1]);
    let head = CommandMessage::from_bytes(&decoded).ok_or(FrameError::ShortHeader {
        decoded_len: decoded.len(),
    })?;
    Ok(ClientMessage {
        head,
        body: decoded[CommandMessage::SIZE..].to_vec(),
    })
}

/// 服务端→客户端帧的帧尾形态。
///
/// 两种形态在现网并存，**不是同一件事的两种写法**：
/// - [`ServerFrameTail::Bang`]：`'!'` —— GameGate / SelGate 构造的帧，以及 LoginGate 自身
///   产生的那几帧（超时踢人等）；
/// - [`ServerFrameTail::BangDollar`]：`'!''$'` —— LoginSrv 构造、经 LoginGate 原样转发的帧
///   （`LoginSrv/Services/ClientSession.cs` 的 `"#" + sMsg + "!$"`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerFrameTail {
    /// 帧以 `!` 结尾。
    Bang,
    /// 帧以 `!$` 结尾（LoginSrv 形态）。
    BangDollar,
}

impl ServerFrameTail {
    /// 帧尾字节切片。
    #[must_use]
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Bang => b"!",
            Self::BangDollar => b"!$",
        }
    }

    /// 从帧尾字节反推形态（末字节 `!` → [`Self::Bang`]，`$` → [`Self::BangDollar`]）。
    #[must_use]
    pub fn from_last_byte(b: u8) -> Option<Self> {
        match b {
            b'!' => Some(Self::Bang),
            b'$' => Some(Self::BangDollar),
            _ => None,
        }
    }
}

/// 解析一帧**服务端→客户端**报文（`#...!` 或 `#...!$`），丢弃帧尾形态信息。
///
/// 需要原样复现字节（回放/对拍）时用 [`decode_server_frame_ex`]。
///
/// # Errors
/// 帧定界符缺失或解码后不足 12 字节头时返回 [`FrameError`]。
pub fn decode_server_frame(frame: &[u8]) -> Result<ClientMessage, FrameError> {
    decode_server_frame_ex(frame).map(|(msg, _)| msg)
}

/// 解析一帧**服务端→客户端**报文，并返回其帧尾形态。
///
/// # Errors
/// 帧定界符缺失（含无法识别的帧尾）或解码后不足 12 字节头时返回 [`FrameError`]。
pub fn decode_server_frame_ex(
    frame: &[u8],
) -> Result<(ClientMessage, ServerFrameTail), FrameError> {
    if frame.first() != Some(&b'#') {
        return Err(FrameError::MissingLead);
    }
    let Some(tail) = frame
        .last()
        .and_then(|b| ServerFrameTail::from_last_byte(*b))
    else {
        return Err(FrameError::MissingTail);
    };
    // 帧尾 `!$` 占 2 字节，`!` 占 1 字节
    let payload_end = frame.len() - tail.bytes().len();
    if payload_end < 2 {
        return Err(FrameError::MissingTail);
    }
    let decoded = edcode::decode(&frame[1..payload_end]);
    let head = CommandMessage::from_bytes(&decoded).ok_or(FrameError::ShortHeader {
        decoded_len: decoded.len(),
    })?;
    Ok((
        ClientMessage {
            head,
            body: decoded[CommandMessage::SIZE..].to_vec(),
        },
        tail,
    ))
}

/// 构造客户端→服务端帧：`'#' '1' EDCode(头) EDCode(体) '!'`（头体分别编码后拼接）。
#[must_use]
pub fn encode_client_frame(head: &CommandMessage, plain_body: &[u8]) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(3 + edcode::encoded_len(CommandMessage::SIZE + plain_body.len()));
    out.push(b'#');
    out.push(CLIENT_FRAME_TAG);
    out.extend_from_slice(&edcode::encode(&head.to_bytes()));
    if !plain_body.is_empty() {
        out.extend_from_slice(&edcode::encode(plain_body));
    }
    out.push(b'!');
    out
}

/// 构造服务端→客户端帧：`'#' EDCode(头) 已编码体 '!'`。
///
/// `encoded_body` 必须是**已经过 [`edcode::encode`] 的**体字节
/// （镜像 GameSvr 侧 `EDCode.EncodeString` 预编码 + 网关只编码头部的链路）。
#[must_use]
pub fn encode_server_frame(head: &CommandMessage, encoded_body: &[u8]) -> Vec<u8> {
    encode_server_frame_tail(head, encoded_body, ServerFrameTail::Bang)
}

/// 服务端→客户端帧的**体分段布局**。
///
/// 线上事实（C2 金标准 222 帧实测 + C# 源核对）：服务端某些消息的体**不是单段编码**，
/// 而是把若干段**各自独立编码后拼接**（每段各自从编码器初态开始）。
/// 单段编码的可产长度为 `n + ceil(n/3)`；实测不符帧的 payload 长度全为 `≡1 (mod 4)`，
/// 而该长度**不可能**由任何单段编码产出 ⇒ 必然是多段拼接。例：
/// - `SM_TURN` 家族（`6/7/9/10`）：`EncodePacket(CharDesc{int,int}=8B) + EncodeString($"{{Msg}}/{{color}}")`
///   （`M2Server/Player/PlayObject.Message.cs:1541-1552`）；
/// - `ident=811`（`SM_ADJUST_BONUS`）等其它多段形态待逐消息号补表（见 whitelist A-8）。
///
/// 判据仍是**真判据**：分段由本表决定（不是从字节里搜出来的），选错段长就编不出原字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyLayout {
    /// 体是单段编码（默认）。
    Single,
    /// 体 = `编码(前 N 字节结构)` + `[编码(其余字节)]`（两段各自独立编码）。
    StructThenRest {
        /// 首段结构字节数（SM_TURN 家族为 `CharDesc` = 8）。
        struct_len: usize,
    },
}

/// 按消息号给出体分段布局（表驱动；未登记的消息号按 [`BodyLayout::Single`]）。
#[must_use]
pub fn body_layout(ident: u16) -> BodyLayout {
    use crate::messages;
    match ident {
        // SM_RUSH(6) / SM_RUSHKUNG(7) / SM_BACKSTEP(9) / SM_TURN(10)：CharDesc + 可选文本
        messages::SM_RUSH | messages::SM_RUSHKUNG | messages::SM_BACKSTEP | messages::SM_TURN => {
            BodyLayout::StructThenRest { struct_len: 8 }
        }
        _ => BodyLayout::Single,
    }
}

/// 按布局把线上 payload（已去掉 `#`/`!`）**分段解码**成 12B 头 + 明文体。
///
/// 为什么必须分段解：多段帧的段边界不在 4 字符编码周期上，整段解码会错位
/// （C# 侧同样如此 —— 它先按 `DEFBLOCKSIZE=16` 取头，再按约定切体）。
#[must_use]
pub fn decode_server_payload(payload: &[u8]) -> Option<(CommandMessage, Vec<u8>)> {
    const HEAD_BLOCK: usize = 16; // enc(12B) 恒为 16 字符
    if payload.len() < HEAD_BLOCK {
        return None;
    }
    let head = CommandMessage::from_bytes(&edcode::decode(&payload[..HEAD_BLOCK]))?;
    let rest = &payload[HEAD_BLOCK..];
    let body = match body_layout(head.ident) {
        BodyLayout::Single => edcode::decode(rest),
        BodyLayout::StructThenRest { struct_len } => {
            let first = edcode::encoded_len(struct_len);
            if rest.len() <= first {
                edcode::decode(rest)
            } else {
                let mut out = edcode::decode(&rest[..first]);
                out.extend_from_slice(&edcode::decode(&rest[first..]));
                out
            }
        }
    };
    Some((head, body))
}

/// 按布局把**已解码的体字节**重新编码为线上体（各段独立编码后拼接）。
#[must_use]
pub fn encode_body(decoded_body: &[u8], layout: BodyLayout) -> Vec<u8> {
    match layout {
        BodyLayout::Single => edcode::encode(decoded_body),
        BodyLayout::StructThenRest { struct_len } => {
            if decoded_body.len() <= struct_len {
                // 短于首段结构：无第二段（C# 侧 `EncodePacket` 之后不拼文本）
                return edcode::encode(decoded_body);
            }
            let mut out = edcode::encode(&decoded_body[..struct_len]);
            out.extend_from_slice(&edcode::encode(&decoded_body[struct_len..]));
            out
        }
    }
}

/// 构造服务端→客户端帧，按消息号的体布局分段编码（推荐入口）。
#[must_use]
pub fn encode_server_frame_layout(
    head: &CommandMessage,
    decoded_body: &[u8],
    tail: ServerFrameTail,
) -> Vec<u8> {
    encode_server_frame_tail(
        head,
        &encode_body(decoded_body, body_layout(head.ident)),
        tail,
    )
}

/// 构造服务端→客户端帧，显式指定帧尾形态（回放时按抓到的原样复现）。
#[must_use]
pub fn encode_server_frame_tail(
    head: &CommandMessage,
    encoded_body: &[u8],
    tail: ServerFrameTail,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        2 + edcode::encoded_len(CommandMessage::SIZE) + encoded_body.len() + tail.bytes().len(),
    );
    out.push(b'#');
    out.extend_from_slice(&edcode::encode(&head.to_bytes()));
    out.extend_from_slice(encoded_body);
    out.extend_from_slice(tail.bytes());
    out
}

/// 流式分帧器（服务端→客户端方向语义：按 `'#'` 起、`'!'` 止；`'*'` 心跳单独上报）。
///
/// C# 客户端用 `ArrestStringEx(BufferStr, '#', '!', data)` 从累计缓冲里逐帧抠出，
/// 并把所有 `'*'` 从流中剔除后回 1 字节应答。本分帧器等价实现该语义；
/// 也可用于客户端→服务端方向（帧形如 `#1...!`，多一个标签字节，解析照旧）。
#[derive(Default)]
pub struct FrameSplitter {
    buf: Vec<u8>,
}

/// 分帧产出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitOut {
    /// 完整一帧（含首尾定界符）。
    Frame(Vec<u8>),
    /// 心跳字节 `'*'`（已从流中剔除；接收方应回 1 字节 `'*'`）。
    Heartbeat,
}

impl FrameSplitter {
    /// 新建空分帧器。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入字节流，返回本次抠出的全部产物（帧/心跳），按出现顺序。
    pub fn feed(&mut self, data: &[u8]) -> Vec<SplitOut> {
        let mut out = Vec::new();
        for &b in data {
            if b == HEARTBEAT {
                // 客户端语义：'*' 从流中剔除并应答；不在帧内才这样处理
                //（编码字符集是 0x3C..=0x7B，'*' = 0x2A 不在其中，帧内不可能出现）。
                out.push(SplitOut::Heartbeat);
                continue;
            }
            if b == b'#' {
                // 上一帧没收尾就被新帧头打断：丢弃残留（与客户端 ArrestStringEx 的容错一致）。
                self.buf.clear();
                self.buf.push(b);
            } else if !self.buf.is_empty() {
                if b == b'!' {
                    self.buf.push(b);
                    out.push(SplitOut::Frame(std::mem::take(&mut self.buf)));
                } else {
                    self.buf.push(b);
                }
            }
            // 帧外垃圾字节：丢弃（客户端行为：只认 '#...!'）。
        }
        out
    }
}

/// 网关↔引擎内部帧头（20B 小端），对应 C# `Packets/ServerPackets/ServerMessage.cs`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerMessage {
    /// 恒为 [`crate::messages::grobal2::PacketCode`]。
    pub packet_code: u32,
    /// SocketID（C# 侧是 socket handle 低 32 位）。
    pub socket: i32,
    /// 会话 ID。
    pub session_id: u16,
    /// `Grobal2.GM_*` 管理号（GM_OPEN/GM_CLOSE/GM_DATA/...）。
    pub ident: u16,
    /// 会话在引擎侧的索引（`GM_SERVERUSERINDEX` 分配）。
    pub session_index: i32,
    /// 载荷长度；**负值**表示载荷已是 EDCode 编码完成的文本（绝对值为长度）。
    pub pack_length: i32,
}

impl ServerMessage {
    /// 头部长度：20 字节。
    pub const SIZE: usize = 20;

    /// 序列化为线上 20 字节（小端）。
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..4].copy_from_slice(&self.packet_code.to_le_bytes());
        b[4..8].copy_from_slice(&self.socket.to_le_bytes());
        b[8..10].copy_from_slice(&self.session_id.to_le_bytes());
        b[10..12].copy_from_slice(&self.ident.to_le_bytes());
        b[12..16].copy_from_slice(&self.session_index.to_le_bytes());
        b[16..20].copy_from_slice(&self.pack_length.to_le_bytes());
        b
    }

    /// 从线上字节解析；不足 20 字节返回 `None`。
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < Self::SIZE {
            return None;
        }
        Some(Self {
            packet_code: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            socket: i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            session_id: u16::from_le_bytes([b[8], b[9]]),
            ident: u16::from_le_bytes([b[10], b[11]]),
            session_index: i32::from_le_bytes([b[12], b[13], b[14], b[15]]),
            pack_length: i32::from_le_bytes([b[16], b[17], b[18], b[19]]),
        })
    }

    /// 实际载荷长度（取绝对值）。
    #[must_use]
    pub fn body_len(self) -> usize {
        self.pack_length.unsigned_abs() as usize
    }

    /// 载荷是否已是编码完成形态（`PackLength < 0`）。
    #[must_use]
    pub fn body_is_pre_encoded(self) -> bool {
        self.pack_length < 0
    }
}

/// LoginGate↔LoginSrv 等链路的 6 字节头，对应 C# `ServerDataPacket`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerDataPacket {
    /// 封包标识码（`grobal2::PacketCode`）。
    pub packet_code: u32,
    /// 封包总长度（头部之后载荷字节数，C# `PacketLen`）。
    pub packet_len: u16,
}

impl ServerDataPacket {
    /// 头部长度：6 字节。
    pub const SIZE: usize = 6;

    /// 序列化为线上 6 字节（小端）。
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..4].copy_from_slice(&self.packet_code.to_le_bytes());
        b[4..6].copy_from_slice(&self.packet_len.to_le_bytes());
        b
    }

    /// 从线上字节解析；不足 20 字节返回 `None`。
    /// 从线上字节解析；不足 6 字节返回 `None`。
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < Self::SIZE {
            return None;
        }
        Some(Self {
            packet_code: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            packet_len: u16::from_le_bytes([b[4], b[5]]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_message_layout() {
        let m = CommandMessage {
            recog: 0x1122_3344,
            ident: 3011,
            param: 10,
            tag: 20,
            series: 3,
        };
        let b = m.to_bytes();
        assert_eq!(b.len(), 12);
        assert_eq!(&b[0..4], &0x1122_3344i32.to_le_bytes());
        assert_eq!(&b[4..6], &3011u16.to_le_bytes());
        assert_eq!(CommandMessage::from_bytes(&b), Some(m));
    }

    #[test]
    fn client_frame_roundtrip() {
        let head = CommandMessage::make(3011, 12345, 10, 20, 3);
        let body = b"walk-body";
        let frame = encode_client_frame(&head, body);
        assert_eq!(frame[0], b'#');
        assert_eq!(frame[1], b'1');
        assert_eq!(*frame.last().unwrap(), b'!');
        let got = decode_client_frame(&frame).unwrap();
        assert_eq!(got.head, head);
        assert_eq!(got.body, body);
    }

    #[test]
    fn server_frame_accepts_both_tails() {
        let head = CommandMessage::make(529, 0, 0, 0, 0);
        let body = crate::edcode::encode("热血传奇".as_bytes());
        for tail in [ServerFrameTail::Bang, ServerFrameTail::BangDollar] {
            let f = encode_server_frame_tail(&head, &body, tail);
            let (got, got_tail) = decode_server_frame_ex(&f).unwrap();
            assert_eq!(got.head, head);
            assert_eq!(got.body, "热血传奇".as_bytes());
            assert_eq!(got_tail, tail);
            // 旧入口（丢弃帧尾）对两种形态都可用
            assert_eq!(decode_server_frame(&f).unwrap().head, head);
        }
        // 帧尾缺失 → 红
        let f = encode_server_frame(&head, &body);
        assert_eq!(
            decode_server_frame_ex(&f[..f.len() - 1]),
            Err(FrameError::MissingTail)
        );
    }

    /// C# 生成的帧尾金标准（字面量取自 `tests/parity/vectors/oracle_vectors.jsonl`，
    /// 由 oracle 生成器用真身 `"#" + EDCode.EncodeMessage(cmd) + "!$"/"!"` 产出）。
    /// 逐字节回放对帧尾自洽，改尾不可见 ⇒ 必须用外部字面量钉住编码器。
    #[test]
    fn server_frame_tails_match_csharp_golden() {
        let head = CommandMessage::make(502, 0, 0, 0, 0); // SM_ID_NOTFOUND，空体
        assert_eq!(
            hex::encode(encode_server_frame_tail(
                &head,
                b"",
                ServerFrameTail::BangDollar
            )),
            "236464686b6452696a6464686b6464686b2124"
        );
        assert_eq!(
            hex::encode(encode_server_frame(&head, b"")),
            "236464686b6452696a6464686b6464686b21"
        );
        // 带体：SM_PASSOK_SELECTSERVER + "1/热血传奇/127.0.0.1/7100"（GB2312 字节！
        // 线上文本一律 GBK：`"热血传奇"` 是 C8 C8 D1 AA B4 AB C6 E6，8 字节，
        // 用 UTF-8 会得 12 字节且字节全错 —— 本用例正是被下面的金标准字面量逮住的）
        let head = CommandMessage::make(529, 0, 0, 0, 0);
        let mut body_bytes = b"1/".to_vec();
        body_bytes.extend_from_slice(&[0xC8, 0xC8, 0xD1, 0xAA, 0xB4, 0xAB, 0xC6, 0xE6]);
        body_bytes.extend_from_slice(b"/127.0.0.1/7100");
        let body = crate::edcode::encode(&body_bytes);
        assert_eq!(
            hex::encode(encode_server_frame_tail(
                &head,
                &body,
                ServerFrameTail::BangDollar
            )),
            "236464686b64696a6b6464686b6464686b615f605854594243403f66554e5f596462633e6a605e58685e613f5f63615867603f2124"
        );
        // 解码方向同样对齐（字面量 → 字段）
        let raw = hex::decode("236464686b6452696a6464686b6464686b2124").unwrap();
        let (msg, tail) = decode_server_frame_ex(&raw).unwrap();
        assert_eq!(msg.head.ident, 502);
        assert_eq!(tail, ServerFrameTail::BangDollar);
    }

    #[test]
    fn loginsrv_tail_is_bang_dollar() {
        // LoginSrv: "#" + sMsg + "!$"（ClientSession.cs:745）
        let head = CommandMessage::make(2001, 0, 0, 0, 0);
        let f = encode_server_frame_tail(&head, b"", ServerFrameTail::BangDollar);
        assert_eq!(*f.last().unwrap(), b'$');
        assert_eq!(f[f.len() - 2], b'!');
        assert_eq!(
            ServerFrameTail::from_last_byte(b'$'),
            Some(ServerFrameTail::BangDollar)
        );
        assert_eq!(
            ServerFrameTail::from_last_byte(b'!'),
            Some(ServerFrameTail::Bang)
        );
        assert_eq!(ServerFrameTail::from_last_byte(b'#'), None);
    }

    #[test]
    fn server_frame_roundtrip() {
        let head = CommandMessage::make(11, 777, 1, 2, 3);
        let body = crate::edcode::encode("血量变动".as_bytes());
        let frame = encode_server_frame(&head, &body);
        assert_eq!(frame[0], b'#');
        assert_eq!(*frame.last().unwrap(), b'!');
        let got = decode_server_frame(&frame).unwrap();
        assert_eq!(got.head, head);
        assert_eq!(got.body, "血量变动".as_bytes());
    }

    #[test]
    fn splitter_frames_and_heartbeat() {
        let mut sp = FrameSplitter::new();
        let f1 = encode_client_frame(&CommandMessage::make(3011, 1, 0, 0, 0), b"");
        let f2 = encode_server_frame(&CommandMessage::make(11, 2, 0, 0, 0), b"");
        // 粘包：两帧 + 心跳一次到达
        let mut stream = Vec::new();
        stream.extend_from_slice(&f1);
        stream.push(b'*');
        stream.extend_from_slice(&f2);
        let out = sp.feed(&stream);
        assert_eq!(
            out,
            vec![
                SplitOut::Frame(f1),
                SplitOut::Heartbeat,
                SplitOut::Frame(f2)
            ]
        );
    }

    #[test]
    fn splitter_partial_delivery() {
        let mut sp = FrameSplitter::new();
        let f = encode_client_frame(&CommandMessage::make(100, 9, 0, 0, 0), b"abc");
        let mid = f.len() / 2;
        assert!(sp.feed(&f[..mid]).is_empty());
        assert_eq!(sp.feed(&f[mid..]), vec![SplitOut::Frame(f)]);
    }

    #[test]
    fn make_message_truncates_like_csharp() {
        // C# MakeMessage 的 (ushort) 截断：SM_EXCHGTAKEON_OK=65023 仍 fit；超 u16 的截断行为锚定。
        let m = CommandMessage::make(0x1_0005, 0, 0, 0, 0);
        assert_eq!(m.ident, 5);
    }
}
