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

use crate::cache::CacheStore;
use crate::db::Db;
use crate::gbk;
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
pub async fn run(server: Arc<Server>, db: Arc<Db>) -> Result<()> {
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
async fn conn_task(server: Arc<Server>, db: Arc<Db>, sock: TcpStream, peer: String) -> Result<()> {
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
    db: &Db,
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
    db: &Db,
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
pub(crate) async fn load_human_rcd(
    server: &Arc<Server>,
    db: &Db,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    query_id: i32,
    s_data: &[u8],
) -> Result<()> {
    // `ProcessServerMsg` 传来的 `s_data` 已经是 EDCode 解好的体（C# 同）；这里直接反序列化。
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
        if let Some(_n_index) = db.play_data_index_text(&chr_name).await {
            // 注意：`if let` 的临时量活到整个 block 结束 —— 这里的 `lock()` guard 不能写进
            // 条件位置，否则会把缓存锁一直持到下面的 `play_data_get().await`，把 N 条取档
            // 串行化（LB-08 判据 `db_down_requests_fail_in_parallel` 会因此变红）。
            let cached = { server.cache.lock().await.get(&chr_name) };
            if let Some(cached) = cached {
                human_rcd = Some(cached);
            } else {
                // oracle `DataService.cs:177-182`：`_playDataStorage.Get(...)` 返回 false
                // （`Open` 失败 / `GetRecord` 内部吞掉读异常）时只置 `nCheckCode = -2`，
                // **不回包也不上抛**；应答分支只看 `(nCheckCode == 1) || boFoundSession`
                // ⇒ 会话已找到时仍回 `DBR_LOADHUMANRCD(1100) recog=1 series=1` +
                // `HumDataInfo = null`（降级成功帧）。
                // 旧实现用 `?` 上抛 ⇒ MySQL 停机/重启窗内取档**零回包**，GameSvr 永久挂起（LB-01）。
                match db.play_data_get(&chr_name).await {
                    Ok(Some(v)) => human_rcd = Some(v),
                    Ok(None) => n_check_code = -2,
                    Err(e) => {
                        log::error(&format!(
                            "获取玩家[{chr_name}]数据失败（C# 同场景置 nCheckCode=-2）: {e:#}"
                        ));
                        n_check_code = -2;
                    }
                }
            }
        } else {
            n_check_code = -3;
        }
    }
    if n_check_code == 1 || bo_found {
        // oracle `DataService.cs:192`：`EDCode.EncodeString(chrName)` 内部是
        // `HUtil32.GetBytes`（GB2312），**不查库**。旧实现让 MySQL 做
        // `CONVERT(... USING gbk)`，DB 停机时回退 UTF-8 ⇒ 非 ASCII 名字在降级帧上
        // 字节不等（复验 F1）。这里用本地 GBK 编码，与会话/缓存/库状态无关。
        let encoded_name =
            String::from_utf8(edcode::encode(&gbk::encode(&chr_name))).unwrap_or_default();
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
pub(crate) async fn save_human_rcd(
    server: &Arc<Server>,
    db: &Db,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    query_id: i32,
    s_data: &[u8],
) -> Result<()> {
    // `ProcessServerMsg` 传来的 `s_data` 已经是 EDCode 解好的体（C# 同）。
    let Ok(save) = SaveCharacterData::from_bytes(s_data) else {
        log::error("保存玩家数据出错.");
        return Ok(());
    };
    let s_user_id = save.account.clone().unwrap_or_default();
    let s_chr_name = save.chr_name.clone().unwrap_or_default();
    let Some(mut rcd) = save.character_data else {
        // C# `bo21 = humanRcd == null` ⇒ 直接走失败应答（`DBR_LOADHUMANRCD recog=0`）。
        send_request_plain(tx, query_id, save_reply(true));
        return Ok(());
    };
    rcd.header.set_name(Some(s_chr_name.clone()));
    // oracle `DataService.cs:225-234` + `:237`：库侧处置（吞掉 Add/Update 的失败），随后
    // **只要 humanRcd 非 null** 就 `SetSessionSaveRcd`（与是否落库成功无关）。
    let bo21 = store_human_rcd(db, &server.cache, &s_chr_name, &rcd).await;
    server
        .login
        .lock()
        .await
        .set_session_save_rcd(s_user_id.as_bytes());
    send_request_plain(tx, query_id, save_reply(bo21));
    Ok(())
}

/// oracle `DataService.cs:225-234` 的库侧处置，返回 `bo21`。
///
/// `bo21 = true` ⇒ 索引仍缺失（应答 `DBR_LOADHUMANRCD recog=0`）；`false` ⇒ 已进入
/// `Update`（应答 `DBR_SAVEHUMANRCD recog=1`，**与 `Update` 是否成功无关** —— oracle 忽略
/// `_playDataStorage.Update` 的返回值，`DataService.cs:234`）。
///
/// 失败路径（G2/G3）逐条照 oracle：
/// - `Add` 失败：`PlayDataStorage.Add` 内部 `catch` 吞 SQL 异常并返回 false（`Add.cs:78-82`），
///   `DataService.cs:228` **忽略返回值** → `:229` 再 `Index` 仍 <0 ⇒ `bo21` 保持 true；
/// - `Update` 失败：`PlayDataStorage.SaveRecord` 的 `Open` 失败返回 false / `Commit` 抛异常返回
///   false（`PlayDataStorage.Save.cs:50-53/66-74`），`DataService.cs:234` **忽略返回值** ⇒
///   `bo21 = false`（成功应答，GameSvr 侧据此清 in-save）。
///
/// 旧实现两处都用 `?` 上抛 ⇒ 不回包、`save_outstanding`/`begin_save` 永不清（角色永久 in-save）。
pub(crate) async fn store_human_rcd(
    db: &Db,
    cache: &Arc<Mutex<CacheStore>>,
    name: &str,
    rcd: &CharacterDataInfo,
) -> bool {
    let mut n_index = db.play_data_index_text(name).await;
    if n_index.is_none() {
        // oracle `DataService.cs:228`：`_playDataStorage.Add(humanRcd)` 的返回值被忽略。
        if let Err(e) = db.play_data_add(rcd).await {
            log::error(&format!(
                "保存玩家数据出错（自动建档，C# 同场景吞异常）: {e:#}"
            ));
        }
        n_index = db.play_data_index_text(name).await;
    }
    let Some(_n_index) = n_index else {
        return true;
    };
    // oracle 顺序：先 `_cacheStorage.Add`（`:233`）再 `Update`（`:234`）。
    cache.lock().await.add(name, rcd.clone());
    if let Err(e) = db.play_data_update(name, rcd).await {
        log::error(&format!(
            "保存玩家数据出错（Update 失败被吞，C# 同口径仍回成功应答）: {e:#}"
        ));
    }
    false
}

/// oracle `DataService.cs:242/248` 的应答选择：
/// `bo21 == false` ⇒ `DBR_SAVEHUMANRCD recog=1`；否则 `DBR_LOADHUMANRCD recog=0`。
fn save_reply(bo21: bool) -> ServerRequestMessage {
    if bo21 {
        ServerRequestMessage::new(DBR_LOADHUMANRCD, 0, 0, 0, 0)
    } else {
        ServerRequestMessage::new(DBR_SAVEHUMANRCD, 1, 0, 0, 0)
    }
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
pub async fn cache_flush_task(server: Arc<Server>, db: Arc<Db>) {
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
async fn process_cache_storage(server: &Arc<Server>, db: &Db) {
    log::info("同步玩家缓存数据.");
    let items = server.cache.lock().await.query_all();
    for play in items {
        let name = play.header.name.clone().unwrap_or_default();
        let ok = db.play_data_update(&name, &play).await.unwrap_or(false);
        log::debug(&format!("{name}同步{}.", if ok { "成功" } else { "失败" }));
        server.cache.lock().await.delete(&name);
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

    /// **卡 `openmir2-db-error-reply`（G2/G3）构造级判据**：失败路径必须**有应答**，且
    /// `Update` 失败走成功应答（GameSvr `apply_save_reply` 只在 `1102 && recog==1` 时清 in-save）。
    #[test]
    fn save_reply_matches_oracle_failure_paths() {
        // G2：`Update` 的 Err（acquire/begin 失败）在 oracle 里被忽略（`DataService.cs:234`）
        // ⇒ `bo21 = false` ⇒ 成功应答。GameSvr 收到后 `end_save`（清 in-save）。
        let ok = save_reply(false);
        assert_eq!(ok.ident, DBR_SAVEHUMANRCD);
        assert_eq!(ok.recog, 1, "G2：Update 失败仍回成功应答");

        // G3：`Add` 失败后 `Index` 仍 <0 ⇒ `bo21 = true` ⇒ 失败应答
        // （oracle `DataService.cs:248`：`DBR_LOADHUMANRCD recog=0`）。
        let fail = save_reply(true);
        assert_eq!(fail.ident, DBR_LOADHUMANRCD);
        assert_eq!(fail.recog, 0, "G3：自动建档失败回失败应答");
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

#[cfg(test)]
mod save_failure_tests {
    //! 失败注入（G2/G3）：用**关闭连接池**制造 `acquire`/`begin` 失败——这正是 oracle
    //! `StorageContext.Open` 失败/`Update` 抛异常的可达场景（MySQL 瞬断/池耗尽/死锁）。
    //!
    //! 跑法（需本机 MySQL；三个用例各用独立 `mir2_test_*` 库，自建自删）：
    //! `cargo test -p db-srv --bins save_failure_tests -- --ignored --nocapture`

    use super::{save_reply, store_human_rcd, Db, DBR_LOADHUMANRCD, DBR_SAVEHUMANRCD};
    use crate::cache::CacheStore;
    use mir2_storage::playerdata::CharacterDataInfo;
    use mir2_storage::LegacyDsn;
    use sqlx::mysql::MySqlConnection;
    use sqlx::Connection;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    const TABLES: [&str; 10] = [
        "characters_indexes",
        "characters",
        "characters_ablity",
        "characters_bonusability",
        "characters_item",
        "characters_bagitem",
        "characters_storageitem",
        "characters_item_attr",
        "characters_magic",
        "characters_status",
    ];

    fn ddl_for(dump: &str, table: &str) -> String {
        let marker = format!("CREATE TABLE `{table}` ");
        let start = dump
            .find(&marker)
            .unwrap_or_else(|| panic!("{table} 建表语句缺失"));
        let rest = &dump[start..];
        let engine = rest.find(") ENGINE").expect("建表语句缺 ENGINE 尾");
        let semi = rest[engine..].find(';').expect("建表语句缺分号");
        rest[..=engine + semi].to_string()
    }

    async fn setup(database: &str) -> LegacyDsn {
        let dsn = LegacyDsn::parse(&format!(
            "server=127.0.0.1;uid=root;pwd=;database={database};"
        ))
        .expect("测试 DSN 非法");
        assert!(
            dsn.database.starts_with("mir2_test_"),
            "拒绝写非测试库：{}",
            dsn.database
        );
        let admin = LegacyDsn {
            database: "mysql".to_string(),
            ..dsn.clone()
        };
        let mut conn = MySqlConnection::connect(&admin.to_sqlx_url())
            .await
            .expect("连测试 MySQL 失败（本测试需本机 MySQL）");
        sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", dsn.database))
            .execute(&mut conn)
            .await
            .expect("删测试库失败");
        sqlx::query(&format!(
            "CREATE DATABASE `{}` CHARACTER SET utf8 COLLATE utf8_general_ci",
            dsn.database
        ))
        .execute(&mut conn)
        .await
        .expect("建测试库失败");
        let dump = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../sql/mir2_db.sql"
        ))
        .expect("读 sql/mir2_db.sql 失败");
        let mut conn = MySqlConnection::connect(&dsn.to_sqlx_url())
            .await
            .expect("连测试库失败");
        for table in TABLES {
            let ddl = ddl_for(&dump, table);
            sqlx::query(&ddl)
                .execute(&mut conn)
                .await
                .unwrap_or_else(|e| panic!("建表 {table} 失败: {e}"));
        }
        dsn
    }

    fn info(name: &str) -> CharacterDataInfo {
        let mut v = CharacterDataInfo::csharp_new();
        v.header.set_name(Some(name.to_string()));
        if let Some(d) = v.data.as_mut() {
            d.chr_name = Some(name.to_string());
        }
        v
    }

    async fn insert_character(dsn: &LegacyDsn, name: &str) -> i64 {
        let mut conn = MySqlConnection::connect(&dsn.to_sqlx_url())
            .await
            .expect("连测试库失败");
        let res =
            sqlx::query("INSERT INTO characters (LoginID, ChrName, Deleted) VALUES ('acct', ?, 0)")
                .bind(name)
                .execute(&mut conn)
                .await
                .expect("插入 characters 失败");
        #[allow(clippy::cast_possible_wrap)]
        let id = res.last_insert_id() as i64;
        id
    }

    /// G2：索引命中 ⇒ `play_data_update` 的 `acquire` 失败被吞 ⇒ `bo21=false`（成功应答）。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_savefail_g2（自建自删）"]
    async fn update_failure_still_answers_success_for_in_save_release() {
        let dsn = setup("mir2_test_savefail_g2").await;
        insert_character(&dsn, "g2hero").await;
        let mut db = Db::connect(&dsn).await.expect("连测试库失败");
        db.load_quick_lists().await.expect("载入索引失败");
        assert!(
            db.play_data_index_text("g2hero").await.is_some(),
            "索引必须命中"
        );
        // 失败注入：关闭连接池 ⇒ `play_data_update` 的 `pool.acquire()` 返回 Err。
        db.pool.close().await;
        let db = Arc::new(db);
        let cache = Arc::new(Mutex::new(CacheStore::default()));
        let bo21 = store_human_rcd(&db, &cache, "g2hero", &info("g2hero")).await;
        assert!(!bo21, "G2：Update 失败必须按 oracle 口径视为已进入 Update");
        let m = save_reply(bo21);
        assert_eq!((m.ident, m.recog), (DBR_SAVEHUMANRCD, 1));
        let mut conn = MySqlConnection::connect(&dsn.to_sqlx_url())
            .await
            .expect("连测试库失败");
        sqlx::query("DROP DATABASE IF EXISTS `mir2_test_savefail_g2`")
            .execute(&mut conn)
            .await
            .expect("清理测试库失败");
    }

    /// G3：索引缺失 + `Add` 失败（池关闭）⇒ `bo21=true`（失败应答，但**有应答**）。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_savefail_g3（自建自删）"]
    async fn add_failure_still_answers_failure() {
        let dsn = setup("mir2_test_savefail_g3").await;
        let mut db = Db::connect(&dsn).await.expect("连测试库失败");
        db.load_quick_lists().await.expect("载入索引失败");
        assert!(db.play_data_index_text("g3hero").await.is_none());
        db.pool.close().await;
        let db = Arc::new(db);
        let cache = Arc::new(Mutex::new(CacheStore::default()));
        let bo21 = store_human_rcd(&db, &cache, "g3hero", &info("g3hero")).await;
        assert!(bo21, "G3：Add 失败后索引仍缺失");
        let m = save_reply(bo21);
        assert_eq!((m.ident, m.recog), (DBR_LOADHUMANRCD, 0));
        let mut conn = MySqlConnection::connect(&dsn.to_sqlx_url())
            .await
            .expect("连测试库失败");
        sqlx::query("DROP DATABASE IF EXISTS `mir2_test_savefail_g3`")
            .execute(&mut conn)
            .await
            .expect("清理测试库失败");
    }

    /// 成功路径不变：索引命中、库可用 ⇒ `bo21=false`，且 `Update` 真的落库。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_savefail_ok（自建自删）"]
    async fn success_path_still_updates_and_answers_success() {
        let dsn = setup("mir2_test_savefail_ok").await;
        let id = insert_character(&dsn, "okhero").await;
        let mut db = Db::connect(&dsn).await.expect("连测试库失败");
        db.load_quick_lists().await.expect("载入索引失败");
        let db = Arc::new(db);
        let cache = Arc::new(Mutex::new(CacheStore::default()));
        let mut rcd = info("okhero");
        if let Some(d) = rcd.data.as_mut() {
            d.gold = 321;
            d.allow_group = 1;
        }
        let bo21 = store_human_rcd(&db, &cache, "okhero", &rcd).await;
        assert!(!bo21);
        assert_eq!(save_reply(bo21).ident, DBR_SAVEHUMANRCD);
        let mut conn = MySqlConnection::connect(&dsn.to_sqlx_url())
            .await
            .expect("连测试库失败");
        let row = sqlx::query("SELECT Gold FROM characters WHERE Id=?")
            .bind(id)
            .fetch_one(&mut conn)
            .await
            .expect("读 characters 失败");
        let gold: i64 = sqlx::Row::try_get(&row, "Gold").expect("Gold 列");
        assert_eq!(gold, 321, "成功路径必须真的落库");
        sqlx::query("DROP DATABASE IF EXISTS `mir2_test_savefail_ok`")
            .execute(&mut conn)
            .await
            .expect("清理测试库失败");
    }
}

#[cfg(test)]
mod db_fault_tests {
    //! 6000 面故障路径（LB-01 + LB-07 的 6000 侧）：
    //! - `DB_LOADHUMANRCD` 在 `acquire` 失败时必须回 oracle 的**降级成功帧**
    //!   （`1100 recog=1 series=1` + `HumDataInfo=null`），不得零回包；
    //! - 自动建档时 `characters_item`/`characters_bagitem` 的 INSERT 失败必须整次回滚
    //!   （oracle `CreateUseItem`/`CreateBagItem` 在 `AddRecord` 的 try 之外），而
    //!   `characters_storageitem` 的失败被吞并提交（它在 try 内）。
    //!
    //! 跑法（需本机 MySQL；各用例独立 `mir2_test_*` 库，自建自删）：
    //! `cargo test -p db-srv --bins db_fault_tests -- --ignored --nocapture`

    use super::{load_human_rcd, save_reply, store_human_rcd, DBR_LOADHUMANRCD, DBR_SAVEHUMANRCD};
    use crate::cache::CacheStore;
    use crate::testsupport as ts;
    use mir2_protocol::edcode;
    use mir2_protocol::frame::ServerDataPacket;
    use mir2_storage::playerdata::{
        CharacterDataInfo, LoadCharacterData, LoadPlayerDataPacket, MemPack, ServerRequestData,
        ServerRequestMessage,
    };
    use mir2_storage::LegacyDsn;
    use sqlx::Row;
    use std::sync::Arc;
    use tokio::sync::{mpsc, Mutex};

    const SESSION: i32 = 7001;

    fn info(name: &str) -> CharacterDataInfo {
        let mut v = CharacterDataInfo::csharp_new();
        v.header.set_name(Some(name.to_string()));
        if let Some(d) = v.data.as_mut() {
            d.chr_name = Some(name.to_string());
        }
        v
    }

    /// 拆 6B 头 + `ServerRequestData`，返回 `(query_id, 消息头, 业务体解码结果)`。
    fn parse_reply(wire: &[u8]) -> (i32, ServerRequestMessage, Option<Vec<u8>>) {
        let head =
            ServerDataPacket::from_bytes(&wire[..ServerDataPacket::SIZE]).expect("应答 6B 头非法");
        assert_eq!(
            head.packet_len as usize,
            wire.len() - ServerDataPacket::SIZE
        );
        let resp = ServerRequestData::from_bytes(&wire[ServerDataPacket::SIZE..])
            .expect("应答信封解析失败");
        let message = resp.message.as_ref().expect("应答缺 Message");
        let m =
            ServerRequestMessage::from_bytes(&edcode::decode(message)).expect("应答消息头解析失败");
        let body = resp.packet.as_ref().map(|p| edcode::decode(p));
        (resp.query_id, m, body)
    }

    async fn count_where(dsn: &LegacyDsn, table: &str, col: &str, value: &str) -> i64 {
        let mut c = ts::conn(dsn).await;
        let sql = format!("SELECT COUNT(*) AS n FROM {table} WHERE {col}=?");
        let row = sqlx::query(&sql)
            .bind(value)
            .fetch_one(&mut c)
            .await
            .unwrap_or_else(|e| panic!("统计 {table}.{col} 失败: {e}"));
        row.try_get("n").expect("COUNT 列")
    }

    async fn count_all(dsn: &LegacyDsn, table: &str) -> i64 {
        let mut c = ts::conn(dsn).await;
        let sql = format!("SELECT COUNT(*) AS n FROM {table}");
        let row = sqlx::query(&sql)
            .fetch_one(&mut c)
            .await
            .unwrap_or_else(|e| panic!("统计 {table} 失败: {e}"));
        row.try_get("n").expect("COUNT 列")
    }

    /// LB-01 主判据：`pool.close()`（MySQL 停机/重启时 acquire 失败）⇒ oracle 降级成功帧。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_loadfail_a（自建自删）"]
    async fn load_db_acquire_failure_answers_degraded_success_frame() {
        let dsn = ts::setup_db("mir2_test_loadfail_a").await;
        ts::insert_character(&dsn, "acct", "loadhero").await;
        let db = ts::open_db(&dsn).await;
        assert!(
            db.play_data_index_text("loadhero").await.is_some(),
            "内存索引必须命中（否则走 -3 分支）"
        );
        // 故障注入：关闭连接池 ⇒ 后续 `acquire` 返回 Err（评审更正：池占满不可达，
        // 真实触发是 MySQL 停机/重启）。
        db.pool.close().await;
        let db = Arc::new(db);

        let server = ts::test_server("loadfail_a", &dsn);
        server
            .login
            .lock()
            .await
            .add_session(b"acct".to_vec(), SESSION, Vec::new());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let req = LoadCharacterData {
            account: Some("acct".to_string()),
            chr_name: Some("loadhero".to_string()),
            user_addr: Some("127.0.0.1".to_string()),
            session_id: SESSION,
        };
        load_human_rcd(&server, &db, &tx, 7, &req.to_bytes())
            .await
            .expect("故障路径不得上抛（旧实现 `?` 会冒到 conn_task 只记日志）");
        let frames = ts::take_frames(&mut rx);
        assert_eq!(frames.len(), 1, "必须有且只有一帧（旧实现零回包）");
        let (qid, m, packet) = parse_reply(&frames[0]);
        assert_eq!(qid, 7);
        assert_eq!(
            (m.ident, m.recog, m.param, m.tag, m.series),
            (DBR_LOADHUMANRCD, 1, 0, 0, 1),
            "oracle `DataService.cs:190-199`：会话已找到时即使 Get 失败也回成功帧"
        );
        let body = LoadPlayerDataPacket::from_bytes(&packet.expect("成功帧必须带业务体"))
            .expect("LoadPlayerDataPacket 解析失败");
        assert_eq!(body.hum_data_info, None, "oracle 同场景 HumDataInfo=null");
        let expected_name = String::from_utf8(edcode::encode(b"loadhero")).unwrap();
        assert_eq!(body.chr_name.as_deref(), Some(expected_name.as_str()));
        println!("[golden] DBR_LOADHUMANRCD degraded frame hex = {}", {
            use std::fmt::Write as _;
            let mut s = String::new();
            for b in &frames[0] {
                let _ = write!(s, "{b:02x}");
            }
            s
        });
        ts::drop_db(&dsn).await;
    }

    /// **复验 F1 主判据**：降级 1100 帧的 `ChrName` 是 GB2312 字节（oracle `DataService.cs:192`
    /// 的 `HUtil32.GetBytes`），**与 MySQL 可用性无关**；旧实现走 `db.gbk_bytes`，`pool.close()`
    /// 后回退 UTF-8 ⇒ 非 ASCII 名字逐字节不等。
    ///
    /// ASCII 反向对照是上一条用例（`loadhero`，同样 `pool.close()`）。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_loadfail_gbk（自建自删）"]
    async fn load_degraded_frame_chr_name_is_gb2312_not_utf8() {
        let dsn = ts::setup_db("mir2_test_loadfail_gbk").await;
        let name = "测试";
        ts::insert_character(&dsn, "acct", name).await;
        let db = ts::open_db(&dsn).await;
        assert!(
            db.play_data_index_text(name).await.is_some(),
            "内存索引必须命中（否则走 -3 分支）"
        );
        db.pool.close().await;
        let db = Arc::new(db);

        let server = ts::test_server("loadfail_gbk", &dsn);
        server
            .login
            .lock()
            .await
            .add_session(b"acct".to_vec(), SESSION, Vec::new());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let req = LoadCharacterData {
            account: Some("acct".to_string()),
            chr_name: Some(name.to_string()),
            user_addr: Some("127.0.0.1".to_string()),
            session_id: SESSION,
        };
        load_human_rcd(&server, &db, &tx, 9, &req.to_bytes())
            .await
            .expect("DB 停机不得上抛");
        let frames = ts::take_frames(&mut rx);
        assert_eq!(frames.len(), 1, "必须有且只有一帧");
        let (qid, m, packet) = parse_reply(&frames[0]);
        assert_eq!(qid, 9);
        assert_eq!(
            (m.ident, m.recog, m.param, m.tag, m.series),
            (DBR_LOADHUMANRCD, 1, 0, 0, 1),
            "oracle `DataService.cs:190-199`：会话已找到时降级成功帧"
        );
        let body = LoadPlayerDataPacket::from_bytes(&packet.expect("成功帧必须带业务体"))
            .expect("LoadPlayerDataPacket 解析失败");
        assert_eq!(body.hum_data_info, None, "oracle 同场景 HumDataInfo=null");

        // 期望：`EDCode.EncodeString("测试")`，其中 `HUtil32.GetBytes` = GB2312。
        let gbk = [0xB2u8, 0xE2, 0xCA, 0xD4];
        assert_eq!(crate::gbk::encode(name), gbk, "本地编码口径 = GB2312");
        let expected = String::from_utf8(edcode::encode(&gbk)).unwrap();
        assert_eq!(body.chr_name.as_deref(), Some(expected.as_str()));

        // 反向牙：旧实现的 UTF-8 回退字节与期望**不同**（本断言让「改回 UTF-8」必红）。
        let utf8_fallback = String::from_utf8(edcode::encode(name.as_bytes())).unwrap();
        assert_ne!(expected, utf8_fallback, "GBK 与 UTF-8 字节必须可区分");
        ts::drop_db(&dsn).await;
    }

    /// LB-01 反向子断言：会话未命中时仍回 `1100 recog=-1`（不得把失败帧也改成成功帧）。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_loadfail_b（自建自删）"]
    async fn load_session_miss_still_answers_negative_code() {
        let dsn = ts::setup_db("mir2_test_loadfail_b").await;
        ts::insert_character(&dsn, "acct", "loadhero").await;
        let db = Arc::new(ts::open_db(&dsn).await);
        let server = ts::test_server("loadfail_b", &dsn);
        // 不推会话 ⇒ `CheckSessionLoadRcd` 未命中。
        let (tx, mut rx) = mpsc::unbounded_channel();
        let req = LoadCharacterData {
            account: Some("acct".to_string()),
            chr_name: Some("loadhero".to_string()),
            user_addr: Some("127.0.0.1".to_string()),
            session_id: SESSION,
        };
        load_human_rcd(&server, &db, &tx, 8, &req.to_bytes())
            .await
            .expect("会话未命中不得上抛");
        let frames = ts::take_frames(&mut rx);
        assert_eq!(frames.len(), 1);
        let (qid, m, packet) = parse_reply(&frames[0]);
        assert_eq!(qid, 8);
        assert_eq!(
            (m.ident, m.recog, m.series),
            (DBR_LOADHUMANRCD, -1, 0),
            "未命中会话 ⇒ nCheckCode=-1 的失败帧"
        );
        assert_eq!(packet.map_or(0, |p| p.len()), 0, "失败帧不得带业务体");
        ts::drop_db(&dsn).await;
    }

    /// LB-07（6000 侧）：自动建档的 `characters_item` 第 5 条 INSERT 失败 ⇒ 整次建角回滚。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_4k_item（自建自删）"]
    async fn save_auto_create_item_insert_failure_rolls_back_character() {
        let dsn = ts::setup_db("mir2_test_4k_item").await;
        ts::fail_insert_trigger(&dsn, "trg_item_fail", "characters_item", 4).await;
        let db = Arc::new(ts::open_db(&dsn).await);
        let cache = Arc::new(Mutex::new(CacheStore::default()));
        let name = "k6itemhero";
        let bo21 = store_human_rcd(&db, &cache, name, &info(name)).await;
        assert!(bo21, "建角失败 ⇒ 索引仍缺失 ⇒ DBR_LOADHUMANRCD recog=0");
        let m = save_reply(bo21);
        assert_eq!((m.ident, m.recog), (DBR_LOADHUMANRCD, 0));
        assert_eq!(
            count_where(&dsn, "characters", "ChrName", name).await,
            0,
            "oracle：CreateUseItem 在 try 外 ⇒ characters 行一起回滚"
        );
        assert_eq!(count_all(&dsn, "characters_item").await, 0);
        assert!(
            db.play_data_index_text(name).await.is_none(),
            "回滚后不得写内存索引"
        );
        ts::drop_db(&dsn).await;
    }

    /// LB-07 反向子断言：`characters_storageitem` 的 INSERT 失败被吞并照常提交。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_4k_storage（自建自删）"]
    async fn save_auto_create_storage_insert_failure_is_swallowed() {
        let dsn = ts::setup_db("mir2_test_4k_storage").await;
        ts::fail_insert_trigger(&dsn, "trg_storage_fail", "characters_storageitem", -1).await;
        let db = Arc::new(ts::open_db(&dsn).await);
        let cache = Arc::new(Mutex::new(CacheStore::default()));
        let name = "k6storhero";
        let bo21 = store_human_rcd(&db, &cache, name, &info(name)).await;
        assert!(!bo21, "CreateStorageItem 在 try 内 ⇒ 吞错、照常回存档成功");
        let m = save_reply(bo21);
        assert_eq!((m.ident, m.recog), (DBR_SAVEHUMANRCD, 1));
        assert_eq!(
            count_where(&dsn, "characters", "ChrName", name).await,
            1,
            "主表必须提交"
        );
        assert_eq!(count_all(&dsn, "characters_storageitem").await, 0);
        assert_eq!(
            count_all(&dsn, "characters_item").await,
            13,
            "item 表不受 storage 失败影响"
        );
        ts::drop_db(&dsn).await;
    }
}
