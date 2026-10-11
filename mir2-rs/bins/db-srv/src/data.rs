//! DBSrv **6000 `DataService` 面**：GameSvr ↔ DBSrv 的取档/存档链路
//! （C# `src/DBSrv/Services/Impl/DataService.cs`）。
//!
//! 线上形态：6B `ServerDataPacket`（`PacketCode` + `PacketLen`）+ MemoryPack `ServerRequestData`；
//! 体内 `Message`/`Packet`/`Sign` 都是 **EDCode 编码后的字节**，各自再 MemoryPack。
//! 帧格式与记录类型的字节级口径由 `crates/storage::playerdata` 的 C# oracle 向量钉住。
//!
//! 逐条对齐 C#（含既有怪行为）：
//! - 校验码 `HUtil32.MakeLong((ushort)(queryId ^ 170), (ushort)packetLen)`；
//!   `packetLen = Message.Length + Packet.Length + 6`（`Message` 是**编码后**长度）；
//! - `checkCode <= 0` 或 `Sign` 为空时**跳过签名比对直接处理**；签名不匹配则**关闭连接**；
//! - `DB_LOADHUMANRCD`(100)/`DB_SAVEHUMANRCD`(101)/`DB_SAVEHUMANRCDEX`(102)，其余回 `DBR_FAIL`；
//! - 取档必须先过 `ClientSession.CheckSessionLoadRcd`（会话号 + 账号，账号大小写敏感），
//!   成功回 `DBR_LOADHUMANRCD`(1100) recog=1 series=1 + `LoadPlayerDataPacket`，失败回同一 ident + 负码；
//! - 存档成功回 `DBR_SAVEHUMANRCD`(1102) recog=1；失败回 **`DBR_LOADHUMANRCD`(1100) recog=0**（C# 原样）。
//!
//! `EDCode.EncodeBuffer(byte[])` 在 `data.Length >= 10000` 时返回**空数组**（C# `BufferSize`），如实照搬。

use std::sync::Arc;

use anyhow::{Context, Result};
use mir2_protocol::edcode;
use mir2_protocol::frame::{ServerDataPacket, DEF_BLOCK_SIZE};
use mir2_protocol::messages::grobal2::PacketCode;
use mir2_protocol::messages::{
    DBR_FAIL, DBR_LOADHUMANRCD, DBR_SAVEHUMANRCD, DB_LOADHUMANRCD, DB_SAVEHUMANRCD,
    DB_SAVEHUMANRCDEX,
};
use mir2_storage::playerdata::{
    CharacterDataInfo, LoadCharacterData, LoadPlayerDataPacket, MemPack, SaveCharacterData,
    ServerRequestData, ServerRequestMessage,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

use crate::db::Db;
use crate::log;
use crate::server::Server;

/// C# `EDCode.BufferSize`（`EncodeBuffer` 超过即返回空数组）。
const EDCODE_BUFFER_SIZE: usize = 10_000;

/// C# 校验码里的 `queryId ^ 170` 常量（`170 = 0xAA`）。
const CHECK_XOR: i32 = 0xAA;

/// 取 i32 的低 16 位（C# `(ushort)` 强制转换）。
pub(crate) fn low16(v: i32) -> u16 {
    u16::try_from(v & 0xFFFF).unwrap_or(0)
}

/// 处理一条请求后对连接的动作。
pub(crate) enum ConnAction {
    Continue,
    /// C# `client.Close()`（签名不匹配）。
    Close,
}

/// 起 6000 监听并服务（`DataService.Initialize` + `Start`）。
///
/// # Errors
/// 绑定失败（端口被占/地址非法）时返回错误（M1④：启动失败要明确报错并给退出码）。
pub async fn run(server: Arc<Server>, db: Arc<Mutex<Db>>) -> Result<()> {
    let addr = (server.cfg.server_addr.as_str(), server.cfg.server_port);
    let listener = TcpListener::bind(addr).await.with_context(|| {
        format!(
            "绑定 {}:{} 失败（端口被占？M1④）",
            server.cfg.server_addr, server.cfg.server_port
        )
    })?;
    log::info(&format!(
        "玩家数据存储服务[{}:{}]已启动.等待链接...",
        server.cfg.server_addr, server.cfg.server_port
    ));
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        let server = Arc::clone(&server);
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            if let Err(e) = conn_task(server, db, sock, peer.to_string()).await {
                log::warn(&format!("玩家数据服务[{peer}]会话结束: {e:#}"));
            }
        });
    }
}

/// 单个 GameSvr 连接（C# `Connecting`/`Received`/`Disconnected`）。
async fn conn_task(
    server: Arc<Server>,
    db: Arc<Mutex<Db>>,
    sock: TcpStream,
    peer: String,
) -> Result<()> {
    let ip = peer
        .rsplit_once(':')
        .map_or(peer.as_str(), |(h, _)| h)
        .to_string();
    if !server.share.check_server_ip(&ip) {
        log::warn(&format!("非法服务器连接: {ip}"));
        log::info(&format!("服务器连接: {ip}"));
        return Ok(());
    }
    log::info(&format!("服务器连接: {ip}"));
    let (mut rd, mut wr) = sock.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let writer = tokio::spawn(async move {
        while let Some(buf) = rx.recv().await {
            if wr.write_all(&buf).await.is_err() {
                break;
            }
            wr.flush().await.ok();
        }
    });
    let mut head = [0u8; ServerDataPacket::SIZE];
    loop {
        if rd.read_exact(&mut head).await.is_err() {
            break;
        }
        let Some(pkt) = ServerDataPacket::from_bytes(&head) else {
            log::error("解析玩家数据封包出现异常...");
            break;
        };
        let mut body = vec![0u8; usize::from(pkt.packet_len)];
        if rd.read_exact(&mut body).await.is_err() {
            break;
        }
        if pkt.packet_code != PacketCode {
            log::error("验证玩家数据封包头出现异常...");
            continue;
        }
        let request = match ServerRequestData::from_bytes(&body) {
            Ok(v) => v,
            Err(e) => {
                log::error(&format!("解析玩家数据封包出现异常... {e:#}"));
                continue;
            }
        };
        match process(&server, &db, &tx, &request).await {
            Ok(ConnAction::Continue) => {}
            Ok(ConnAction::Close) => break,
            Err(e) => log::error(&format!("处理玩家数据失败: {e:#}")),
        }
    }
    writer.abort();
    Ok(())
}

/// C# `ProcessMessagePacket` + `ProcessServerMsg`。
async fn process(
    server: &Arc<Server>,
    db: &Arc<Mutex<Db>>,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    request: &ServerRequestData,
) -> Result<ConnAction> {
    let query_id = request.query_id;
    let Some(message_enc) = request.message.as_ref() else {
        // C#：`requestData.Message.Length` 先 NPE（异常被 TouchSocket 记日志、不回包）
        log::error("玩家数据封包缺少 Message 字段（C# 同场景 NPE）");
        return Ok(ConnAction::Continue);
    };
    let message_bytes = edcode::decode(message_enc);
    let Ok(request_message) = ServerRequestMessage::from_bytes(&message_bytes) else {
        log::error("解析玩家数据消息头失败");
        return Ok(ConnAction::Continue);
    };
    let packet_len = message_enc
        .len()
        .saturating_add(request.packet.as_ref().map_or(0, Vec::len))
        .saturating_add(ServerDataPacket::SIZE);
    if packet_len >= DEF_BLOCK_SIZE && query_id > 0 {
        if let (Some(packet_enc), Some(sign)) = (request.packet.as_ref(), request.sign.as_ref()) {
            let s_data = edcode::decode(packet_enc);
            let check_code = make_long(
                low16(query_id ^ CHECK_XOR),
                u16::try_from(packet_len).unwrap_or(u16::MAX),
            );
            if check_code <= 0 {
                process_server_msg(server, db, tx, query_id, &request_message, &s_data).await?;
                return Ok(ConnAction::Continue);
            }
            if sign.is_empty() {
                process_server_msg(server, db, tx, query_id, &request_message, &s_data).await?;
                return Ok(ConnAction::Continue);
            }
            let signature_id = low16(check_code); // BitConverter.ToInt16 低 16 位
            let sign_dec = edcode::decode(sign);
            let sign_id = if sign_dec.len() >= 2 {
                u16::from_le_bytes([sign_dec[0], sign_dec[1]])
            } else {
                0
            };
            if sign_id == signature_id {
                process_server_msg(server, db, tx, query_id, &request_message, &s_data).await?;
                return Ok(ConnAction::Continue);
            }
            log::error(&format!("关闭错误的任务{query_id}查询请求."));
            return Ok(ConnAction::Close);
        }
    }
    // 包长不足/queryId 非法/缺 Packet/Sign：回 DBR_FAIL（C# 同）。
    send_request_plain(
        tx,
        query_id,
        ServerRequestMessage::new(DBR_FAIL, 0, 0, 0, 0),
    );
    Ok(ConnAction::Continue)
}

/// C# `ProcessServerMsg`。
async fn process_server_msg(
    server: &Arc<Server>,
    db: &Arc<Mutex<Db>>,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    query_id: i32,
    packet: &ServerRequestMessage,
    s_data: &[u8],
) -> Result<()> {
    match packet.ident {
        x if x == DB_LOADHUMANRCD => load_human_rcd(server, db, tx, query_id, s_data).await,
        x if x == DB_SAVEHUMANRCD => save_human_rcd(server, db, tx, query_id, s_data).await,
        x if x == DB_SAVEHUMANRCDEX => save_human_rcd(server, db, tx, query_id, s_data).await,
        _ => {
            send_request_plain(
                tx,
                query_id,
                ServerRequestMessage::new(DBR_FAIL, 0, 0, 0, 0),
            );
            Ok(())
        }
    }
}

/// C# `DataService.LoadHumanRcd`。
async fn load_human_rcd(
    server: &Arc<Server>,
    db: &Arc<Mutex<Db>>,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    query_id: i32,
    s_data: &[u8],
) -> Result<()> {
    // \`ProcessServerMsg\` 传来的 \`s_data\` 已经是 EDCode 解好的体（C# 同）；这里直接反序列化。
    let Ok(req) = LoadCharacterData::from_bytes(s_data) else {
        log::error(&format!(
            "解析 LoadCharacterData 失败（体 {}B）",
            s_data.len()
        ));
        return Ok(());
    };
    if req.session_id <= 0 {
        return Ok(());
    }
    let account = req.account.clone().unwrap_or_default();
    let chr_name = req.chr_name.clone().unwrap_or_default();
    let mut n_check_code = -1;
    let mut bo_found = false;
    if !account.is_empty() && !chr_name.is_empty() {
        let (found, code) = server
            .login
            .lock()
            .await
            .check_session_load_rcd(account.as_bytes(), req.session_id);
        bo_found = found;
        n_check_code = code;
        if n_check_code < 0 || !bo_found {
            log::warn(&format!(
                "[非法请求] 帐号: {account} IP: {} 标识: {}",
                req.user_addr.clone().unwrap_or_default(),
                req.session_id
            ));
        }
    }
    let mut human_rcd: Option<CharacterDataInfo> = None;
    if n_check_code == 1 || bo_found {
        let db = db.lock().await;
        if let Some(_n_index) = db.play_data_index_text(&chr_name) {
            if let Some(cached) = server.cache.lock().await.get(chr_name.as_bytes()) {
                human_rcd = Some(cached);
            } else if let Some(v) = db.play_data_get(&chr_name).await? {
                human_rcd = Some(v);
            } else {
                n_check_code = -2;
            }
        } else {
            n_check_code = -3;
        }
    }
    if n_check_code == 1 || bo_found {
        let gbk = {
            let db = db.lock().await;
            db.gbk_bytes(&chr_name)
                .await
                .unwrap_or_else(|_| chr_name.as_bytes().to_vec())
        };
        let encoded_name = String::from_utf8(edcode::encode(&gbk)).unwrap_or_default();
        let load = LoadPlayerDataPacket {
            chr_name: Some(encoded_name),
            hum_data_info: human_rcd,
        };
        let message = ServerRequestMessage::new(DBR_LOADHUMANRCD, 1, 0, 0, 1);
        send_request_with(tx, query_id, message, &load);
        log::debug(&format!("获取玩家[{chr_name}]数据成功"));
    } else {
        send_request_plain(
            tx,
            query_id,
            ServerRequestMessage::new(DBR_LOADHUMANRCD, n_check_code, 0, 0, 0),
        );
    }
    Ok(())
}

/// C# `DataService.SaveHumanRcd`（`SaveHumanRcdEx` 亦走此函数）。
async fn save_human_rcd(
    server: &Arc<Server>,
    db: &Arc<Mutex<Db>>,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    query_id: i32,
    s_data: &[u8],
) -> Result<()> {
    // \`ProcessServerMsg\` 传来的 \`s_data\` 已经是 EDCode 解好的体（C# 同）。
    let Ok(save) = SaveCharacterData::from_bytes(s_data) else {
        log::error("保存玩家数据出错.");
        return Ok(());
    };
    let s_user_id = save.account.clone().unwrap_or_default();
    let s_chr_name = save.chr_name.clone().unwrap_or_default();
    let Some(mut rcd) = save.character_data else {
        // C# `bo21 = humanRcd == null` ⇒ 失败回包用的是 `DBR_LOADHUMANRCD` (C# 原样)。
        send_request_plain(
            tx,
            query_id,
            ServerRequestMessage::new(DBR_LOADHUMANRCD, 0, 0, 0, 0),
        );
        return Ok(());
    };
    let mut bo21 = true;
    rcd.header.set_name(Some(s_chr_name.clone()));
    {
        let mut db = db.lock().await;
        let mut n_index = db.play_data_index_text(&s_chr_name);
        if n_index.is_none() {
            let _ = db.play_data_add(&rcd).await?;
            n_index = db.play_data_index_text(&s_chr_name);
        }
        if n_index.is_some() {
            server
                .cache
                .lock()
                .await
                .add(s_chr_name.as_bytes(), rcd.clone());
            let _ = db.play_data_update(&s_chr_name, &rcd).await?;
            bo21 = false;
        }
    }
    server
        .login
        .lock()
        .await
        .set_session_save_rcd(s_user_id.as_bytes());
    let message = if bo21 {
        ServerRequestMessage::new(DBR_LOADHUMANRCD, 0, 0, 0, 0)
    } else {
        ServerRequestMessage::new(DBR_SAVEHUMANRCD, 1, 0, 0, 0)
    };
    send_request_plain(tx, query_id, message);
    Ok(())
}

// ==== 信封编解码（C# `SendRequest`/`SendRequest<T>`/`GetCheckCode`/`SendMessage`） ====

/// C# `HUtil32.MakeLong(ushort, ushort)`。
pub(crate) fn make_long(low: u16, high: u16) -> i32 {
    (u32::from(low) | (u32::from(high) << 16)).cast_signed()
}

/// C# `EDCode.EncodeBuffer(byte[])`：≥ 10000 字节返回空数组。
pub(crate) fn encode_buffer(data: &[u8]) -> Vec<u8> {
    if data.len() >= EDCODE_BUFFER_SIZE {
        return Vec::new();
    }
    edcode::encode(data)
}

/// C# `GetCheckCode`（6000/5700 面共用；`SendRequest` 的校验码口径）。
pub(crate) fn get_check_code(query_id: i32, packet: &ServerRequestData) -> i32 {
    let len = packet
        .message
        .as_ref()
        .map_or(0, Vec::len)
        .saturating_add(packet.packet.as_ref().map_or(0, Vec::len))
        .saturating_add(ServerDataPacket::SIZE);
    make_long(
        low16(query_id ^ CHECK_XOR),
        u16::try_from(len).unwrap_or(u16::MAX),
    )
}

/// C# `SendRequest`（无业务体）：设 QueryId → 补空 Packet → 算 Sign → 发。
pub(crate) fn send_request_plain(
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    query_id: i32,
    message: ServerRequestMessage,
) {
    let mut resp = ServerRequestData {
        query_id,
        message: Some(encode_buffer(&message.to_bytes())),
        packet: None,
        sign: None,
    };
    let check = if resp.packet.is_some() {
        get_check_code(query_id, &resp)
    } else {
        resp.packet = Some(Vec::new());
        make_long(
            low16(query_id ^ CHECK_XOR),
            u16::try_from(
                resp.message
                    .as_ref()
                    .map_or(0, Vec::len)
                    .saturating_add(ServerDataPacket::SIZE),
            )
            .unwrap_or(u16::MAX),
        )
    };
    resp.sign = Some(encode_buffer(&check.to_le_bytes()));
    send_message(tx, &resp.to_bytes());
}

/// C# `SendRequest<T>`（带业务体）。
pub(crate) fn send_request_with<T: MemPack>(
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    query_id: i32,
    message: ServerRequestMessage,
    body: &T,
) {
    send_request_bytes(tx, query_id, message, &body.to_bytes());
}

/// C# `SendRequest`（带**已序列化**业务体）：5700 面用它发送非 `mir2_storage` 的 MemoryPack 体。
pub(crate) fn send_request_bytes(
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    query_id: i32,
    message: ServerRequestMessage,
    body: &[u8],
) {
    let mut resp = ServerRequestData {
        query_id,
        message: Some(encode_buffer(&message.to_bytes())),
        packet: Some(encode_buffer(body)),
        sign: None,
    };
    let check = get_check_code(query_id, &resp);
    resp.sign = Some(encode_buffer(&check.to_le_bytes()));
    send_message(tx, &resp.to_bytes());
}

/// C# `SendMessage`：6B `ServerDataPacket` + 体。
pub(crate) fn send_message(tx: &mpsc::UnboundedSender<Vec<u8>>, body: &[u8]) {
    let header = ServerDataPacket {
        packet_code: PacketCode,
        packet_len: u16::try_from(body.len()).unwrap_or(u16::MAX),
    };
    let mut wire = header.to_bytes().to_vec();
    wire.extend_from_slice(body);
    let _ = tx.send(wire);
}

// ==== TimedService 的缓存回写（C# `TimedService.ProcessCacheStorage`） ====

/// C# `TimedService` 的 5 分钟缓存回写循环。
///
/// C# `TimedService` 另有两个周期（10s 重连 LoginSrv、7s 上报 `(103/…)`）已由
/// [`crate::loginsrv::login_link_task`] 承担（同一 `ClientSession`/`LoginState` 语义），这里不重复。
pub async fn cache_flush_task(server: Arc<Server>, db: Arc<Mutex<Db>>) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
    let mut last_flush = tokio::time::Instant::now();
    loop {
        tick.tick().await;
        if last_flush.elapsed() >= std::time::Duration::from_secs(300) {
            last_flush = tokio::time::Instant::now();
            process_cache_storage(&server, &db).await;
        }
    }
}

/// C# `TimedService.ProcessCacheStorage`：把缓存里的存档刷回库并从缓存删除。
///
/// 由于 [`crate::cache::CacheStore`] 的 `Add` 只在键已存在时替换（C# 既有行为），
/// 缓存实际恒为空 —— 这个循环跑起来是**空转**，但日志两条与 C# 一致。
async fn process_cache_storage(server: &Arc<Server>, db: &Arc<Mutex<Db>>) {
    log::info("同步玩家缓存数据.");
    let items = server.cache.lock().await.query_all();
    for play in items {
        let name = play.header.name.clone().unwrap_or_default();
        let ok = db
            .lock()
            .await
            .play_data_update(&name, &play)
            .await
            .unwrap_or(false);
        log::debug(&format!("{name}同步{}.", if ok { "成功" } else { "失败" }));
        server.cache.lock().await.delete(name.as_bytes());
    }
    log::info("同步玩家缓存数据完成.");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MakeLong` 与 C# 一致（高位进 bit16+，负数来自高 16 位）。
    #[test]
    fn make_long_matches_csharp() {
        assert_eq!(make_long(0x0001, 0x0002), 0x0002_0001);
        assert_eq!(make_long(0xFFFF, 0xFFFF), -1);
        assert_eq!(make_long(0x0000, 0x8000), i32::MIN);
    }

    /// `checkCode <= 0` 的边界：`MakeLong` 的高 16 位最高位为 1 时为负（= `packetLen >= 0x8000`）。
    #[test]
    fn check_code_sign_boundary() {
        assert!(make_long(0, 0x8000) <= 0);
        assert!(make_long(0xFFFF, 0x7FFF) > 0);
        assert!(make_long(1u16, 0) > 0);
    }

    /// `EncodeBuffer` 的 10000 字节阈值（C# `BufferSize`）。
    #[test]
    fn encode_buffer_threshold() {
        assert_eq!(encode_buffer(&[0u8; 9999]).len(), edcode::encoded_len(9999));
        assert!(encode_buffer(&vec![0u8; 10_000]).is_empty());
    }

    /// 空业务体的 `DBR_FAIL` 信封可被解析回 `ServerRequestMessage`。
    #[test]
    fn fail_envelope_roundtrips() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        send_request_plain(&tx, 7, ServerRequestMessage::new(DBR_FAIL, 0, 0, 0, 0));
        let wire = rx.try_recv().unwrap();
        assert!(wire.len() >= 6);
        let pkt = ServerDataPacket::from_bytes(&wire[..6]).unwrap();
        assert_eq!(pkt.packet_code, PacketCode);
        assert_eq!(usize::from(pkt.packet_len), wire.len() - 6);
        let resp = ServerRequestData::from_bytes(&wire[6..]).unwrap();
        assert_eq!(resp.query_id, 7);
        let msg = ServerRequestMessage::from_bytes(&edcode::decode(resp.message.as_ref().unwrap()))
            .unwrap();
        assert_eq!(msg.ident, DBR_FAIL);
    }

    /// 请求侧信封：`Message`/`Packet` 都经 EDCode 往返后仍能解出原结构（回归测试里的“解析 LoadCharacterData 失败”）。
    #[test]
    fn request_envelope_roundtrips_load_character_data() {
        let load = LoadCharacterData {
            account: Some("mir2test6k".into()),
            chr_name: Some("zz6k30840".into()),
            user_addr: Some("127.0.0.1".into()),
            session_id: 6000,
        };
        let message =
            edcode::encode(&ServerRequestMessage::new(DB_LOADHUMANRCD, 0, 0, 0, 0).to_bytes());
        let packet = edcode::encode(&load.to_bytes());
        let request = ServerRequestData {
            query_id: 1,
            message: Some(message.clone()),
            packet: Some(packet.clone()),
            sign: Some(edcode::encode(&[0u8; 4])),
        };
        let back = ServerRequestData::from_bytes(&request.to_bytes()).unwrap();
        let msg = ServerRequestMessage::from_bytes(&edcode::decode(back.message.as_ref().unwrap()))
            .expect("Message 解不出");
        assert_eq!(msg.ident, DB_LOADHUMANRCD);
        let s_data = edcode::decode(back.packet.as_ref().unwrap());
        assert_eq!(
            s_data,
            load.to_bytes(),
            "Packet 解码后应等于原始 MemoryPack"
        );
        let l2 = LoadCharacterData::from_bytes(&s_data).expect("LoadCharacterData 解不出");
        assert_eq!(l2, load);
    }
}
