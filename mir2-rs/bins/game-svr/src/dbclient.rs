//! GameSvr → DBSvr 的**取角色请求**编码 —— 照 `src/GameSrv/Services/DataQueryServer.cs:62-90`
//! （`SendRequest` + `SendMessage`）与 `PlayerDataService.cs:166-181`（`LoadRcd`）。
//!
//! 线上形态（**用部署件的序列化器实测钉住**，不是照直觉推 MemoryPack 布局）：
//! ```text
//! frame = ServerDataPacket{ PacketCode:u32, PacketLen:u16 }        // 6 字节定长头（:80-88）
//!       + ServerRequestData{ QueryId:i32, Message:byte[], Packet:byte[], Sign:byte[] }
//! ```
//! 其中 `Message = EDCode(Serialize(ServerRequestMessage{Recog,Ident,Param,Tag,Series}))`、
//! `Packet = EDCode(Serialize(LoadCharacterData{Account,ChrName,UserAddr,SessionID}))`、
//! `Sign = EDCode(LE32(MakeLong((ushort)(queryId ^ 170), (ushort)(msg.len + pkt.len + 6))))`（`:72-73`）。
//!
//! MemoryPack 的两条布局事实（实测）：
//! 1. **`[MemoryPackable]` 且非"unmanaged"的类型**（`ServerRequestMessage` 是 class、`LoadCharacterData`
//!    含 string）会在最前面写 **成员数字节**（5 / 4）；`ServerDataPacket` 全是值类型 ⇒ **裸布局**（6 字节、无前缀）；
//! 2. `string` = `i32(字符数)` + **UTF-16LE** 字节；`i32` = 小端 4 字节。
//!
//! 基准（部署件 `OpenMir2.dll` 跑出来的，见 `tests/fixtures/db_request_oracle.txt`，
//! 输入 `queryId=7 / ident=100 / account=loadbot1 / chr=lb1a / addr=127.0.0.1 / session=2`）：
//! `msg` 13B、`packet` 59B、`signId=6750381`、`sendBuffer` 120B、`frame` 126B。

use mir2_protocol::edcode;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

/// `Grobal2.PacketCode`（`0xAA9AAA9A`）。
const PACKET_CODE: u32 = 0xAA9A_AA9A;

/// `ServerDataPacket.FixedHeaderLen`（`ServerDataMessage.cs:26`）。
pub const SERVER_DATA_FIXED_HEADER_LEN: usize = 6;

/// MemoryPack 的"成员数前缀"（非 unmanaged 类型才有）。
fn mp_prefix(members: u8) -> Vec<u8> {
    vec![members]
}

/// `string`：`i32(字符数)` + UTF-16LE。
fn mp_string(s: &str) -> Vec<u8> {
    let n = i32::try_from(s.chars().count()).unwrap_or(i32::MAX);
    let mut out = n.to_le_bytes().to_vec();
    for c in s.chars() {
        let mut buf = [0u16; 2];
        for u in c.encode_utf16(&mut buf) {
            out.extend_from_slice(&u.to_le_bytes());
        }
    }
    out
}

/// `ServerRequestMessage`（`Packets/ServerPackets/ServerRequestMessage.cs:11-30`）：
/// 成员数 5 + Recog(i32) + Ident/Param/Tag/Series(各 u16)。
#[must_use]
pub fn server_request_message(ident: u16) -> Vec<u8> {
    let mut out = mp_prefix(5);
    out.extend_from_slice(&0i32.to_le_bytes()); // Recog
    out.extend_from_slice(&ident.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // Param
    out.extend_from_slice(&0u16.to_le_bytes()); // Tag
    out.extend_from_slice(&0u16.to_le_bytes()); // Series
    out
}

/// `LoadCharacterData`（`Packets/ServerPackets/PlayerDataInfo.cs:46-52`）：
/// 成员数 4 + 三个 string + SessionID(i32)。
#[must_use]
pub fn load_character_data(
    account: &str,
    chr_name: &str,
    user_addr: &str,
    session_id: i32,
) -> Vec<u8> {
    let mut out = mp_prefix(4);
    out.extend_from_slice(&mp_string(account));
    out.extend_from_slice(&mp_string(chr_name));
    out.extend_from_slice(&mp_string(user_addr));
    out.extend_from_slice(&session_id.to_le_bytes());
    out
}

/// `HUtil32.MakeLong(lo, hi)`：实测 `signId = (hi << 16) | lo`（`ServerRequestData` 那行算出来的 6750381 反推）。
#[must_use]
pub fn make_long(lo: u16, hi: u16) -> i32 {
    i32::from_ne_bytes((u32::from(hi) << 16 | u32::from(lo)).to_ne_bytes())
}

/// 一帧 `ServerRequestData`（`DataQueryServer.cs:62-90` 的逐行复刻），返回**含 6 字节头**的完整线上字节。
#[must_use]
pub fn encode_request(query_id: i32, message: &[u8], packet: &[u8]) -> Vec<u8> {
    let msg_enc = edcode::encode(message);
    let pkt_enc = edcode::encode(packet);
    let sign_id = make_long(
        u16::try_from(query_id ^ 170).unwrap_or(0),
        u16::try_from(msg_enc.len() + pkt_enc.len() + SERVER_DATA_FIXED_HEADER_LEN)
            .unwrap_or(u16::MAX),
    );
    let sign_enc = edcode::encode(&sign_id.to_le_bytes());

    // `ServerRequestData`：成员数 4 + QueryId + Message + Packet + Sign（每个 byte[] = i32 长度 + 字节）
    let mut body = mp_prefix(4);
    body.extend_from_slice(&query_id.to_le_bytes());
    for part in [&msg_enc, &pkt_enc, &sign_enc] {
        body.extend_from_slice(&i32::try_from(part.len()).unwrap_or(i32::MAX).to_le_bytes());
        body.extend_from_slice(part);
    }

    // `ServerDataPacket`：裸布局（无成员数前缀）
    let mut frame = Vec::with_capacity(SERVER_DATA_FIXED_HEADER_LEN + body.len());
    frame.extend_from_slice(&PACKET_CODE.to_le_bytes());
    frame.extend_from_slice(&u16::try_from(body.len()).unwrap_or(u16::MAX).to_le_bytes());
    frame.extend_from_slice(&body);
    frame
}

/// `DB_LOADHUMANRCD` 请求（`PlayerDataService.LoadRcd` `:166-181`）。
#[must_use]
pub fn load_human_rcd_request(
    query_id: i32,
    account: &str,
    chr_name: &str,
    user_addr: &str,
    session_id: i32,
) -> Vec<u8> {
    let msg = server_request_message(mir2_protocol::messages::DB_LOADHUMANRCD);
    let pkt = load_character_data(account, chr_name, user_addr, session_id);
    encode_request(query_id, &msg, &pkt)
}

/// DB 链路的"已连接"标志（C# `DataQueryServer.IsConnected`，`:60`）—— sim 侧据此选
/// `QueryCharacterData` 的真/假分支（`CharacterDataProcessor.cs:165`）。
#[derive(Debug, Clone)]
pub struct DbLinkState(Arc<AtomicBool>);

impl DbLinkState {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    fn set(&self, up: bool) {
        self.0.store(up, Ordering::SeqCst);
    }

    /// 是否已连上 DBSvr（对应 `_tcpClient.Online`）。
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl Default for DbLinkState {
    fn default() -> Self {
        Self::new()
    }
}

/// 起 DB 链路（照 `DataQueryServer` 的 `Start` `:35-53`：连不上**只记日志、不退出**；
/// 断线在 `DataSocketDisconnected`（`:92-96`）也只是记错误日志）。
///
/// 本切片只**发**请求；**收**（`LoadPlayerDataPacket` → 取角色 → 进世界）属 S2b-2b 的第二半（已登记）。
pub async fn run_db_link(
    db_addr: String,
    db_port: u16,
    state: DbLinkState,
    mut req_rx: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    loop {
        match tokio::net::TcpStream::connect((db_addr.as_str(), db_port)).await {
            Ok(sock) => {
                sock.set_nodelay(true).ok();
                state.set(true);
                crate::glog!("INF", "数据库服务器[{db_addr}:{db_port}]连接成功...");
                // 收半边：本切片只把字节读掉（丢弃），避免对端写满；解析属 S2b-2b
                let (mut rd, mut wr) = sock.into_split();
                let drain = tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = [0u8; 4096];
                    while let Ok(n) = rd.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                    }
                });
                while let Some(frame) = req_rx.recv().await {
                    if wr.write_all(&frame).await.is_err() || wr.flush().await.is_err() {
                        crate::glog!("ERR", "写数据库服务器失败（连接断开？）");
                        break;
                    }
                }
                state.set(false);
                drain.abort();
                crate::glog!("ERR", "数据库服务器[{db_addr}]断开连接...");
            }
            Err(e) => {
                crate::glog!("ERR", "链接数据库服务器[{db_addr}:{db_port}]失败...（{e}）");
                state.set(false);
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_protocol::messages::DB_LOADHUMANRCD;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// **oracle 字节基准**（部署件 `OpenMir2.dll` 跑的，见 `tests/fixtures/db_request_oracle.txt`）：
    /// 输入 `queryId=7 / ident=DB_LOADHUMANRCD(100) / account=loadbot1 / chr=lb1a / addr=127.0.0.1 / session=2`。
    const ORACLE_MSG: &str = "05000000006400000000000000";
    const ORACLE_PACKET: &str = "04080000006c006f006100640062006f0074003100040000006c00620031006100090000003100320037002e0030002e0030002e00310002000000";
    const ORACLE_FRAME: &str = "9aaa9aaa7800040700000012000000656468676464447b6464686b6464686b643f4f0000006464686564643c7b646f68686d64447b646e686b6f64546f6461686b64646867646c68686e64596b646d686b656468636461686b6264576b645e686860643e6b6460686b5e64595f6466686b64644b060000003d64476f643f";

    #[test]
    fn server_request_message_matches_oracle_bytes() {
        assert_eq!(
            server_request_message(DB_LOADHUMANRCD),
            hex(ORACLE_MSG),
            "ServerRequestMessage 的 MemoryPack 布局（成员数前缀 5 + Recog/Ident/Param/Tag/Series）"
        );
    }

    #[test]
    fn load_character_data_matches_oracle_bytes() {
        assert_eq!(
            load_character_data("loadbot1", "lb1a", "127.0.0.1", 2),
            hex(ORACLE_PACKET),
            "LoadCharacterData 的 MemoryPack 布局（成员数前缀 4 + 3×string + i32）"
        );
    }

    #[test]
    fn whole_request_frame_matches_oracle_bytes() {
        let got = load_human_rcd_request(7, "loadbot1", "lb1a", "127.0.0.1", 2);
        assert_eq!(
            got.len(),
            126,
            "帧长必须等于 oracle 的 126（6 头 + 120 体）"
        );
        assert_eq!(
            got,
            hex(ORACLE_FRAME),
            "整帧必须逐字节等于 oracle 的合成结果"
        );
    }

    #[test]
    fn make_long_is_hi_shifted_lo() {
        assert_eq!(
            make_long(173, 103),
            6_750_381,
            "queryId=7 时 oracle 的 signId"
        );
    }
}
