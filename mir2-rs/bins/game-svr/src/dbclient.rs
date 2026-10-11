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
    server_request_message_with(ident, 0)
}

/// 带 `Recog` 的版本（应答里 DBSvr 会把 `Recog` 置 1，`PlayerDataService.cs:150` 用 `nRecog == 1` 判定「取角色成功」）。
#[must_use]
pub fn server_request_message_with(ident: u16, recog: i32) -> Vec<u8> {
    let mut out = mp_prefix(5);
    out.extend_from_slice(&recog.to_le_bytes()); // Recog
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

/// `DB_SAVEHUMANRCD` 请求（`WorldServer.SaveHumanRcd` → `PlayerDataService.SavePlayerRcd`）：
/// 与取档同一个信封，`Packet` = `record::encode_save_character_data(account, chr_name, rec)`。
///
/// `packet` 是**已序列化的 `SaveCharacterData`**（GameSvr 侧用 `record::Writer` 编，DBSvr 侧用
/// `crates/storage::SaveCharacterData` 解；两侧同 MemoryPack 布局）。
#[must_use]
pub fn save_human_rcd_request(query_id: i32, packet: &[u8]) -> Vec<u8> {
    let msg = server_request_message(mir2_protocol::messages::DB_SAVEHUMANRCD);
    encode_request(query_id, &msg, packet)
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

/// DBSvr → GameSvr 的一帧（与请求**同一个信封**：`ServerDataPacket` + `ServerRequestData`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub query_id: i32,
    /// `ServerRequestData.Message`（**编码态**：`EDCode(ServerRequestMessage)`）。
    pub message: Vec<u8>,
    /// `ServerRequestData.Packet`（**编码态**：`EDCode(LoadPlayerDataPacket)`）。
    pub packet: Vec<u8>,
    /// `ServerRequestData.Sign`（编码态）。
    pub sign: Vec<u8>,
}

/// 收侧分帧（`DataQueryServer.ProcessServerPacket` `:132-186`）：
/// 6B 头 + `ServerRequestData`；`PacketCode` 不对时 oracle 是**偏移 1 字节再试**（`:143-150`），这里照做。
#[derive(Debug, Default)]
pub struct ReplyParser {
    buf: Vec<u8>,
    /// `ServerDataPacket.PacketCode` 不匹配时累计的偏移（oracle 用 `srcOffset`；仅日志用）。
    pub resync_bytes: u64,
}

impl ReplyParser {
    /// 喂入原始字节（等价于 `DataSocketRead` `:104-130` 的累积）。
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// 取下一帧；不足一帧时返回 `None`（剩下留在缓冲里，等价于 oracle 的 `BuffLen`）。
    pub fn next(&mut self) -> Option<Reply> {
        loop {
            if self.buf.len() < SERVER_DATA_FIXED_HEADER_LEN {
                return None;
            }
            let code = u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]);
            if code != PACKET_CODE {
                // `:143-150`：偏移 1 字节继续找
                self.buf.remove(0);
                self.resync_bytes += 1;
                continue;
            }
            let plen = u16::from_le_bytes([self.buf[4], self.buf[5]]);
            let total = SERVER_DATA_FIXED_HEADER_LEN + usize::from(plen);
            if self.buf.len() < total {
                return None; // `:151-155`：不够就等下一批
            }
            let body = self.buf[SERVER_DATA_FIXED_HEADER_LEN..total].to_vec();
            self.buf.drain(..total);
            return parse_request_data(&body);
        }
    }
}

/// 解 `ServerRequestData`（MemoryPack：前缀 4 + QueryId + 3×(i32 长度 + 字节)）。
fn parse_request_data(body: &[u8]) -> Option<Reply> {
    let mut i = 0usize;
    let mut take = |n: usize| -> Option<&[u8]> {
        if i + n > body.len() {
            return None;
        }
        let s = &body[i..i + n];
        i += n;
        Some(s)
    };
    if *take(1)?.first()? != 4 {
        return None; // 成员数前缀（实测 0x04）
    }
    let query_id = i32::from_le_bytes(take(4)?.try_into().ok()?);
    let mut parts: Vec<Vec<u8>> = Vec::with_capacity(3);
    for _ in 0..3 {
        let len = i32::from_le_bytes(take(4)?.try_into().ok()?);
        let len = usize::try_from(len).ok()?;
        parts.push(take(len)?.to_vec());
    }
    let mut it = parts.into_iter();
    Some(Reply {
        query_id,
        message: it.next()?,
        packet: it.next()?,
        sign: it.next()?,
    })
}

/// 签名的判定（`ProcessServerData` `:188-220`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyVerdict {
    /// 接受（`:213` → `PlayerDataService.Enqueue`）
    Accept,
    /// `Message.len + Packet.len + 6 < 12`（`:201` 的 `nLen >= 12` 门外）：
    /// oracle 在这个分支里**什么都不做**（既不累 `LoadDBErrorCount`、也不打日志）⇒ 静默丢。
    TooShort,
    /// `Sign` 为空或 `queryId <= 0`（`:204`）
    EmptySign,
    /// 签名对不上（`:215-218`）
    BadSign,
}

/// **照抄 oracle 的两处怪癖**（别按"更安全"改）：
/// 1. 期望值 `queryId = MakeLong((ushort)(qid ^ 170), (ushort)nLen)`，但比较只用 **低 16 位**
///    （`BitConverter.ToInt16(...)`，`:211`）⇒ `nLen` 那半根本没被校验；
/// 2. 校验失败（`EmptySign`/`BadSign`）只累 `SystemShare.Config.LoadDBErrorCount`、**不记日志**
///    （本实现另加一条 WARN，登记为可观测性差异）；
/// 3. 但 `nLen < 12` 的 `TooShort` 在 `:201` 的 `nLen >= 12` 门**之外**，oracle 连计数都不做。
#[must_use]
pub fn verify_sign(r: &Reply) -> ReplyVerdict {
    let n_len = r.message.len() + r.packet.len() + SERVER_DATA_FIXED_HEADER_LEN;
    if n_len < 12 {
        return ReplyVerdict::TooShort; // `:201`
    }
    let expect = make_long(
        u16::try_from(r.query_id ^ 170).unwrap_or(0),
        u16::try_from(n_len).unwrap_or(u16::MAX),
    );
    if expect <= 0 || r.sign.is_empty() {
        return ReplyVerdict::EmptySign; // `:204`
    }
    let got = edcode::decode(&r.sign);
    if got.len() < 2 {
        return ReplyVerdict::BadSign;
    }
    let want_lo = u16::from_le_bytes([expect.to_le_bytes()[0], expect.to_le_bytes()[1]]);
    let got_lo = u16::from_le_bytes([got[0], got[1]]);
    if want_lo == got_lo {
        ReplyVerdict::Accept
    } else {
        ReplyVerdict::BadSign
    }
}

/// 应答 `Message` 段解出 `(Ident, Recog)`（`GetDataSrvMessage` `:38-44`）。
///
/// `ServerRequestMessage` 的 MemoryPack 布局：前缀 5 + Recog(i32) + Ident/Param/Tag/Series(各 u16)。
#[must_use]
pub fn parse_reply_message(encoded: &[u8]) -> Option<(u16, i32)> {
    let raw = edcode::decode(encoded);
    if raw.first().copied()? != 5 || raw.len() < 13 {
        return None;
    }
    let recog = i32::from_le_bytes([raw[1], raw[2], raw[3], raw[4]]);
    let ident = u16::from_le_bytes([raw[5], raw[6]]);
    Some((ident, recog))
}

/// 收侧汇总：分帧 + 签名判定 + 查询表（`PlayerDataService.QueryMap`）。
#[derive(Debug, Default)]
pub struct DbReplies {
    pub parser: ReplyParser,
    /// `queryId -> 应答`（oracle 的 `QueryMap`）
    map: std::collections::HashMap<i32, Reply>,
    /// `SystemShare.Config.LoadDBErrorCount` 的对应物（`EmptySign`/`BadSign` 的次数；`TooShort` **不**计）
    pub load_db_error_count: u64,
}

impl DbReplies {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂字节并处理所有完整帧；返回"被接受并入表"的 query_id 列表（调用方照 `Enqueue` 记日志）。
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<i32> {
        self.parser.push(bytes);
        let mut accepted = Vec::new();
        while let Some(reply) = self.parser.next() {
            match verify_sign(&reply) {
                ReplyVerdict::Accept => {
                    let qid = reply.query_id;
                    self.map.insert(qid, reply);
                    accepted.push(qid);
                }
                // N3（reviewer-3）：照 `DataQueryServer.cs:199-208`，`nLen < 12` 落在 `nLen >= 12` 门**之外**
                // ⇒ 什么都不做（不累 `LoadDBErrorCount`、不打日志）。
                ReplyVerdict::TooShort => {}
                other => {
                    self.load_db_error_count += 1;
                    crate::glog!(
                        "WRN",
                        "DB 应答被拒（{other:?}）queryId={} —— 照 oracle 只累 LoadDBErrorCount（本条日志是本实现加的可观测性差异）",
                        reply.query_id
                    );
                }
            }
        }
        accepted
    }

    /// 取走某个 query 的应答（`GetDataSrvMessage` `:32-48`：命中即从表里移除）。
    #[must_use]
    pub fn take(&mut self, query_id: i32) -> Option<Reply> {
        self.map.remove(&query_id)
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
    replies: Arc<std::sync::Mutex<DbReplies>>,
) {
    loop {
        match tokio::net::TcpStream::connect((db_addr.as_str(), db_port)).await {
            Ok(sock) => {
                sock.set_nodelay(true).ok();
                state.set(true);
                crate::glog!("INF", "数据库服务器[{db_addr}:{db_port}]连接成功...");
                // 收半边：分帧 + 签名判定 + 入查询表（`DataQueryServer.ProcessServerPacket/ProcessServerData`）
                let (mut rd, mut wr) = sock.into_split();
                let replies_for_task = Arc::clone(&replies);
                let drain = tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = [0u8; 4096];
                    loop {
                        match rd.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                let mut r = replies_for_task.lock().expect("DB 应答表锁被毒化");
                                for qid in r.feed(&buf[..n]) {
                                    // 照 `PlayerDataService.Enqueue` `:20-24` 的调试日志
                                    crate::glog!("DBG", "执行任务Id:{qid}成功");
                                }
                            }
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

    /// 造一帧应答（与请求同信封；`bad_sign` 时把签名改成错的低 16 位）。
    fn reply_frame(
        query_id: i32,
        ident: u16,
        recog: i32,
        packet: &[u8],
        bad_sign: bool,
    ) -> Vec<u8> {
        let message = server_request_message_with(ident, recog);
        let msg_enc = edcode::encode(&message);
        let pkt_enc = edcode::encode(packet);
        let n_len = msg_enc.len() + pkt_enc.len() + SERVER_DATA_FIXED_HEADER_LEN;
        let sign_id = make_long(
            u16::try_from(query_id ^ 170).unwrap_or(0) ^ if bad_sign { 1 } else { 0 },
            u16::try_from(n_len).unwrap_or(u16::MAX),
        );
        let sign_enc = edcode::encode(&sign_id.to_le_bytes());
        let mut body = mp_prefix(4);
        body.extend_from_slice(&query_id.to_le_bytes());
        for part in [&msg_enc, &pkt_enc, &sign_enc] {
            body.extend_from_slice(&i32::try_from(part.len()).unwrap().to_le_bytes());
            body.extend_from_slice(part);
        }
        let mut frame = PACKET_CODE.to_le_bytes().to_vec();
        frame.extend_from_slice(&u16::try_from(body.len()).unwrap().to_le_bytes());
        frame.extend_from_slice(&body);
        frame
    }

    #[test]
    fn reply_round_trip_and_sign_verdicts() {
        let mut r = DbReplies::new();
        let good = reply_frame(7, 101, 1, b"PAYLOAD", false);
        let accepted = r.feed(&good);
        assert_eq!(accepted, vec![7], "签名对 ⇒ 入表（Enqueue）");
        let reply = r.take(7).expect("取走");
        assert_eq!(reply.query_id, 7);
        assert_eq!(reply.packet, edcode::encode(b"PAYLOAD"));
        let (ident, recog) = parse_reply_message(&reply.message).unwrap();
        assert_eq!((ident, recog), (101, 1), "应答 Message 解出 Ident/Recog");
        // 取走后再取：None（`:48` 命中即移除）
        assert!(r.take(7).is_none());

        // 签名错 ⇒ 拒 + 错误计数 +1
        let mut r2 = DbReplies::new();
        assert!(r2.feed(&reply_frame(7, 101, 1, b"P", true)).is_empty());
        assert_eq!(r2.load_db_error_count, 1);
        assert!(r2.take(7).is_none());
    }

    /// N3（reviewer-3）：`nLen < 12` 的应答照 `DataQueryServer.cs:199-208` **什么都不做**
    /// （落在 `nLen >= 12` 门之外）⇒ 不累 `LoadDBErrorCount`、不入表。
    #[test]
    fn too_short_reply_is_silently_dropped_without_error_count() {
        // nLen = encoded_len(1) + encoded_len(1) + 6 = 2 + 2 + 6 = 10 < 12
        let msg_enc = edcode::encode(b"M");
        let pkt_enc = edcode::encode(b"P");
        let sign_enc = edcode::encode(b"S");
        let mut body = mp_prefix(4);
        body.extend_from_slice(&7i32.to_le_bytes());
        for part in [&msg_enc, &pkt_enc, &sign_enc] {
            body.extend_from_slice(&i32::try_from(part.len()).unwrap().to_le_bytes());
            body.extend_from_slice(part);
        }
        let mut frame = PACKET_CODE.to_le_bytes().to_vec();
        frame.extend_from_slice(&u16::try_from(body.len()).unwrap().to_le_bytes());
        frame.extend_from_slice(&body);

        let mut r = DbReplies::new();
        assert!(r.feed(&frame).is_empty(), "TooShort 不入表");
        assert_eq!(
            r.load_db_error_count, 0,
            "oracle 在 nLen<12 时什么都不做 ⇒ 不累 LoadDBErrorCount"
        );
        assert!(r.take(7).is_none());
    }

    #[test]
    fn reply_parser_resyncs_on_bad_packet_code() {
        let mut p = ReplyParser::default();
        let mut junk = vec![0x00, 0x11];
        junk.extend_from_slice(&reply_frame(3, 100, 1, b"X", false));
        p.push(&junk);
        let got = p.next().expect("偏移后应能解出帧");
        assert_eq!(got.query_id, 3);
        assert_eq!(p.resync_bytes, 2, "两个坏字节各偏移一次");
    }

    #[test]
    fn reply_parser_waits_for_full_frame() {
        let mut p = ReplyParser::default();
        let frame = reply_frame(9, 100, 1, b"XY", false);
        p.push(&frame[..frame.len() - 3]);
        assert!(p.next().is_none(), "不足一帧 ⇒ 等下一批（BuffLen 语义）");
        p.push(&frame[frame.len() - 3..]);
        assert_eq!(p.next().unwrap().query_id, 9);
    }

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

    /// 存档请求：与取档同信封，`Message` 的 `Ident = DB_SAVEHUMANRCD(101)`、`Packet` = 传入的
    /// `SaveCharacterData` 编码。用**收侧解析器**解回（不按公式重推）。
    #[test]
    fn save_request_carries_db_savehumanrcd_and_packet() {
        let req = save_human_rcd_request(9, b"SAVEBODY");
        let mut p = ReplyParser::default();
        p.push(&req);
        let r = p.next().expect("存档请求帧应可被同信封解析器解出");
        assert_eq!(r.query_id, 9);
        assert_eq!(
            parse_reply_message(&r.message),
            Some((mir2_protocol::messages::DB_SAVEHUMANRCD, 0)),
            "Message.Ident 必须是 DB_SAVEHUMANRCD(101)"
        );
        assert_eq!(
            r.packet,
            edcode::encode(b"SAVEBODY"),
            "Packet 原样携带 SaveCharacterData 编码"
        );
        assert!(p.next().is_none());
    }
}
