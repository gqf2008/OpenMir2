//! 内部链路帧（网关 ↔ 边界服务）—— 与 C# **MemoryPack** 编码逐字节兼容。
//!
//! 与客户端面（EDCode/`#…!`）不同，网关与服务之间走：
//! `ServerDataPacket`(6B 头) + `MemoryPack(ServerDataMessage)` 体，
//! 见 `LoginGate/ClientThread.SendMessage` / `LoginSrv` 的
//! `ServerDataMessageFixedHeaderDataHandlingAdapter`(HeaderLength=6)。
//!
//! 体格式（**实测**自 C# 真身，向量见 `tests/parity/vectors/oracle_vectors.jsonl` 的
//! `kind:"internal"`，git 侧由 `tests/parity/oracle` 生成）：
//!
//! ```text
//! [0x04]                              MemoryPack 对象头（本类型实测恒定；换类型需重测）
//! [type: u8]                          ServerDataType
//! [socket_id: i32 utf16 单元数][utf16le]  …… 注意是 **UTF-16 码元数**，不是 Unicode 标量数
//! [data_len: i16 LE]                  独立字段：调用方写 Data.Length，但**原样保留**（可不等）
//! [data: i32 字节数][bytes]
//! ```
//!
//! 线上帧 = [`frame::ServerDataPacket`]（`PacketCode` = [`crate::messages::grobal2::PacketCode`]、
//! `PacketLen` = 体长度）+ 体。
//!
//! 这些结构只出现在**进程间**链路上（LoginGate↔LoginSrv 5500、SelGate↔DBSrv 5100、
//! SelGate↔LoginSrv 16005 等），客户端看不到，但 M1 混跑（Rust ↔ C#）要求逐字节一致。

use crate::frame::ServerDataPacket;
use crate::messages::grobal2::PacketCode;

/// C# `ServerDataType`（`Packets/ServerPackets/ServerDataMessage.cs`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ServerDataType {
    /// 新会话接入（体是客户端 IP/端口串一类）。
    Enter = 0,
    /// 会话离开。
    Leave = 1,
    /// 透传数据（体是客户端原始帧字节，如 `#1…!`）。
    Data = 2,
    /// 保活。
    KeepAlive = 3,
}

impl ServerDataType {
    /// 从线上字节还原；未知值返回 `None`（C# 侧 switch 无 default，未知即忽略）。
    #[must_use]
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Enter),
            1 => Some(Self::Leave),
            2 => Some(Self::Data),
            3 => Some(Self::KeepAlive),
            _ => None,
        }
    }
}

/// C# `ServerDataMessage` 的 Rust 镜像。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerDataMessage {
    /// 消息类型（Enter/Leave/Data/KeepAlive）。
    pub kind: ServerDataType,
    /// 会话标识（C# 侧是字符串：连接 Id 或数字串）。
    pub socket_id: String,
    /// `DataLen` 字段。**与 `data.len()` 相互独立**（C# 原样写字段，不回填）。
    pub data_len: i16,
    /// 透传载荷。
    pub data: Vec<u8>,
}

/// MemoryPack 对象头字节（实测；本类型恒定）。
const OBJECT_HEADER: u8 = 0x04;

/// 解析错误（内部帧按长度自描述，坏数据即报错，不静默）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InternalError {
    /// 体太短或对象头不符。
    BadHeader,
    /// `ServerDataType` 取值未知。
    BadType(u8),
    /// 长度字段越界。
    Truncated,
}

impl ServerDataMessage {
    /// 构造（`data_len` 由调用方决定，通常 = `data.len()`）。
    #[must_use]
    pub fn new(kind: ServerDataType, socket_id: impl Into<String>, data: Vec<u8>) -> Self {
        let data_len = i16::try_from(data.len()).unwrap_or(i16::MAX);
        Self {
            kind,
            socket_id: socket_id.into(),
            data_len,
            data,
        }
    }

    /// 序列化为 MemoryPack 体。
    ///
    /// # Panics
    /// `socket_id` 的 UTF-16 码元数或 `data` 长度超过 `i32` 时 panic（实际协议不允许，
    /// C# 侧同样会截断/异常）。
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let units: Vec<u16> = self.socket_id.encode_utf16().collect();
        let mut out = Vec::with_capacity(8 + units.len() * 2 + self.data.len());
        out.push(OBJECT_HEADER);
        out.push(self.kind as u8);
        // 字符串：i32 = UTF-16 码元数（C# `string.Length`），后跟 UTF-16LE
        out.extend_from_slice(
            &i32::try_from(units.len())
                .expect("socket_id length fits i32")
                .to_le_bytes(),
        );
        for u in &units {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out.extend_from_slice(&self.data_len.to_le_bytes());
        out.extend_from_slice(
            &i32::try_from(self.data.len())
                .expect("data length fits i32")
                .to_le_bytes(),
        );
        out.extend_from_slice(&self.data);
        out
    }

    /// 从 MemoryPack 体解析。
    ///
    /// # Errors
    /// 头字节/类型未知或长度字段越界时返回 [`InternalError`]。
    ///
    /// # Panics
    /// 长度字段读取处的定长切片转换理论不可能失败；不构成实际 panic 面。
    pub fn from_bytes(b: &[u8]) -> Result<Self, InternalError> {
        let mut cur = 0usize;
        let take = |cur: &mut usize, n: usize| -> Result<&[u8], InternalError> {
            let end = cur.checked_add(n).ok_or(InternalError::Truncated)?;
            if end > b.len() {
                return Err(InternalError::Truncated);
            }
            let s = &b[*cur..end];
            *cur = end;
            Ok(s)
        };
        if take(&mut cur, 1)?[0] != OBJECT_HEADER {
            return Err(InternalError::BadHeader);
        }
        let kind =
            ServerDataType::from_u8(take(&mut cur, 1)?[0]).ok_or(InternalError::BadType(b[1]))?;
        let n_units = i32::from_le_bytes(take(&mut cur, 4)?.try_into().unwrap());
        let n_units = usize::try_from(n_units).map_err(|_| InternalError::Truncated)?;
        let raw = take(&mut cur, n_units * 2)?;
        let mut units = Vec::with_capacity(n_units);
        for chunk in raw.as_chunks::<2>().0 {
            units.push(u16::from_le_bytes(*chunk));
        }
        let socket_id = String::from_utf16(&units).map_err(|_| InternalError::BadHeader)?;
        let data_len = i16::from_le_bytes(take(&mut cur, 2)?.try_into().unwrap());
        let n_data = i32::from_le_bytes(take(&mut cur, 4)?.try_into().unwrap());
        let n_data = usize::try_from(n_data).map_err(|_| InternalError::Truncated)?;
        let data = take(&mut cur, n_data)?.to_vec();
        Ok(Self {
            kind,
            socket_id,
            data_len,
            data,
        })
    }

    /// 线上帧：6B `ServerDataPacket` 头 + 体。
    #[must_use]
    pub fn to_wire(&self) -> Vec<u8> {
        let body = self.to_bytes();
        let head = ServerDataPacket {
            packet_code: PacketCode,
            packet_len: u16::try_from(body.len()).unwrap_or(u16::MAX),
        };
        let mut out = head.to_bytes().to_vec();
        out.extend_from_slice(&body);
        out
    }

    /// 解析线上帧（6B 头 + 体）。
    ///
    /// # Errors
    /// 头缺失/包标识码不符/体解析失败时返回 [`InternalError`]。
    pub fn from_wire(b: &[u8]) -> Result<Self, InternalError> {
        if b.len() < ServerDataPacket::SIZE {
            return Err(InternalError::Truncated);
        }
        let head = ServerDataPacket::from_bytes(b).ok_or(InternalError::Truncated)?;
        if head.packet_code != PacketCode {
            return Err(InternalError::BadHeader);
        }
        Self::from_bytes(&b[ServerDataPacket::SIZE..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edcode;

    /// 金标准字面量：取自 C# 真身生成的 oracle 向量（`kind:"internal"`）。
    /// 自洽往返对格式改动是盲的（见 `RULE_规则可执行性.md` 第 5 条），故必须钉外部字面量。
    #[test]
    fn memorypack_body_matches_csharp_golden() {
        // type=Data, socket_id="7", DataLen=10, data=Gb2312("#1abcdefg!")（10B）
        let m = ServerDataMessage {
            kind: ServerDataType::Data,
            socket_id: "7".into(),
            data_len: 10,
            data: b"#1abcdefg!".to_vec(),
        };
        assert_eq!(
            hex::encode(m.to_bytes()),
            "04020100000037000a000a00000023316162636465666721"
        );
        // 线上帧 = 9aaa9aaa + 长度 + 体
        assert_eq!(
            hex::encode(m.to_wire()),
            "9aaa9aaa180004020100000037000a000a00000023316162636465666721"
        );
    }

    /// 非 BMP 字符按 **UTF-16 码元数**计（C# `string.Length` 语义）：
    /// "a😀b" → 4 个单元（a + 代理对 2 + b）。
    #[test]
    fn string_length_counts_utf16_units() {
        let m = ServerDataMessage::new(ServerDataType::Enter, "a\u{1F600}b", Vec::new());
        assert_eq!(
            hex::encode(m.to_bytes()),
            "04000400000061003dd800de6200000000000000"
        );
        assert_eq!(m.socket_id.encode_utf16().count(), 4);
    }

    /// `DataLen` 与 `data.len()` 独立：C# 原样写字段，Rust 也必须原样保留。
    #[test]
    fn data_len_field_is_independent() {
        let m = ServerDataMessage {
            kind: ServerDataType::Data,
            socket_id: "9".into(),
            data_len: 99,
            data: b"#1x!".to_vec(),
        };
        assert_eq!(
            hex::encode(m.to_bytes()),
            "040201000000390063000400000023317821"
        );
        let back = ServerDataMessage::from_bytes(&m.to_bytes()).unwrap();
        assert_eq!(back, m);
    }

    /// 往返 + 坏数据必须报错（不静默）。
    #[test]
    fn roundtrip_and_reject_garbage() {
        for (kind, sid, data) in [
            (ServerDataType::KeepAlive, "", vec![]),
            (ServerDataType::Enter, "1234", edcode::encode(b"x")),
            (ServerDataType::Data, "中文会话号", vec![0x00, 0xFF, 0x7F]),
        ] {
            let m = ServerDataMessage::new(kind, sid, data);
            assert_eq!(ServerDataMessage::from_bytes(&m.to_bytes()).unwrap(), m);
            assert_eq!(ServerDataMessage::from_wire(&m.to_wire()).unwrap(), m);
        }
        assert_eq!(
            ServerDataMessage::from_bytes(&[0x05, 0x02]),
            Err(InternalError::BadHeader)
        );
        assert_eq!(
            ServerDataMessage::from_bytes(&[0x04, 0x09]),
            Err(InternalError::BadType(9))
        );
        assert_eq!(
            ServerDataMessage::from_bytes(&[0x04, 0x02, 0xFF]),
            Err(InternalError::Truncated)
        );
        assert_eq!(
            ServerDataMessage::from_wire(&[0x00; 6]),
            Err(InternalError::BadHeader)
        );
    }
}
