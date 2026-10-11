//! Rust 版 **GameGate/RunGate**（M2 入口：顶住 7200，把会话中继进 GameSvr）。
//!
//! 协议面与行为面的取证见 `mir2-rs/docs/M2-GameGate协议梳理.md`（含一次实机取证：假 GameSvr +
//! 真 C# GameGate 的逐帧观测）。本文件按已证实的部分实现，**未证实的字段一律用 `TODO(差分)` 标出**，
//! 不假装等价：
//!
//! - 客户端 face：TCP 7200，**没有定长头**——oracle 的客户端 face 是裸 socket（`ServerService.Initialize()`
//!   的 `TcpService` 没挂任何 `DataHandlingAdapter`）⇒ **一次 read = 一帧**，帧本身是经典 `#…!`
//!   （登录帧 `#1…!`）。**别再按 20B 头去读**：实测（真网关）加头会被当成帧内垃圾前缀 ⇒ `Kicked 1`
//!   （协议梳理 §7.12 / `E:\tmp\openmir2-gamegate-imp\oracle-login-cap\FINDINGS.md`）。
//! - 网关↔GameSvr：同样的 20B 头 + `Grobal2.GM_*` 管理号（GM_OPEN/GM_CLOSE/GM_DATA/...）。
//! - **已由实机取证钉住的字段**：`GM_OPEN{session_id=<随机 4 位>, socket=客户端 handle, session_index=0,
//!   pack_length=IP 串长度} + body=IP 字符串`；客户端断开发 `GM_CLOSE{session_id=0, socket=handle}`。
//! - **`SessionId` 是随机 4 位数字（0..=9999）**，照 `ClientThread.cs:128-160` 的口径生成，
//!   ⇒ 差分/golden 不能按字节比这个字段（协议梳理 §7.5）。
//! - 空转/异常：body 长度 < `GateShare.CommandFixedLength`(=16) ⇒ 关会话（`ClientSession.cs:155-162`）；
//! - **第一帧必须是 `ClientLogin`**（`ClientSession.cs:655-680` / `:1058-`）：网关用 EDCode 解出内层文本
//!   并校验（`login::parse_client_login` 覆盖其 ①②③④ 步），**失败即剔除**；通过后（oracle 的
//!   `HandleLogin = true`，`:1228`）后续帧才按命令转成 `GM_DATA`。登录帧本身**不转发**。
//!   1 字节 body = 心跳 ⇒ 不转发（`ClientSession.cs:157-160`）。
//!
//! **本切片不做**（登记在协议梳理 §6，切片 2/3）：EDCode 解码/重编码与速度规则、`Filters/*`、
//! `Auth/DynamicAuthenticator`(OTP)、`GM_CHECKCLIENT` 周期健康帧、`ClientTimeOutTime` 空闲回收。
//!
//! **失败要响**：缺配置键、绑不上端口、连不上 GameSvr —— 明确报错并非零退出。

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

mod config;
mod gbk;
mod login;
mod rules;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use mir2_protocol::frame::ServerMessage;
use mir2_protocol::messages::grobal2;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

/// C# `GateShare.CommandFixedLength`（`GateShare.cs:18`）：命令帧最小长度。
const COMMAND_FIXED_LENGTH: usize = 16;
/// C# `Packets/ClientPackets/ClientMesaagePacket.cs:24`：`CommandMessage.Size`（解码后的固定头长度）。
const COMMAND_MESSAGE_SIZE: usize = 12;
/// C# `GateShare.MaxSession`（`GateShare.cs:14`）：每条网关→服务器连接的会话上限。
const MAX_SESSION: usize = 10_000;
/// C# `Config.NomClientPacketSize`（现网 `RunGate/config.conf` `[Integer] NomClientPacketSize=400`）：
/// 登录后超过它的帧按 `ClientSession.cs:196-199` **只丢帧、不关会话**。
const NOM_CLIENT_PACKET_SIZE: usize = 400;
/// 单次 read 的缓冲：客户端帧的正常上界远小于它（`NomClientPacketSize=400`；登录帧 <254）。
const MAX_READ: usize = 64 * 1024;

/// C# Serilog 的输出模板同形前缀（便于与两侧日志对时间线）。
#[macro_export]
macro_rules! glog {
    ($lvl:expr, $($arg:tt)*) => {{
        let now = ::std::time::SystemTime::now()
            .duration_since(::std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let ms = now.subsec_millis();
        let secs = now.as_secs();
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        println!("{h:02}:{m:02}:{s:02}.{ms:03} [{}] {}", $lvl, format!($($arg)*));
    }};
}

/// 会话表：`SessionId → 该客户端连接的发送端`（一条网关→服务器连接一张表）。
type Sessions = Arc<Mutex<HashMap<u16, mpsc::UnboundedSender<Vec<u8>>>>>;

/// 会话在**引擎侧的索引**（`ClientSession.cs:649` 的 `SendMsg.SessionIndex = SvrListIdx`）：
/// 由服务端用 `GM_SERVERUSERINDEX` 下发（`ClientThread.cs:272-275` 存进会话的 `SvrListIdx`），
/// 之后该会话发往 GameSvr 的每一帧都要带上它（未收到时为 0）。
type SessionIndexes = Arc<Mutex<HashMap<u16, i32>>>;

#[tokio::main]
async fn main() -> Result<()> {
    let conf = std::env::args()
        .nth(1)
        .map_or_else(config::default_path, std::path::PathBuf::from);
    let cfg = config::load(&conf)?;
    let gate_rules = rules::GateRules {
        chat_interval_enabled: cfg.chat_interval_enabled,
        chat_interval_ms: cfg.chat_interval_ms,
    };
    glog!(
        "INF",
        "游戏网关启动：{} 组端点（config={}）",
        cfg.endpoints.len(),
        conf.display()
    );

    let mut tasks = Vec::new();
    for (i, ep) in cfg.endpoints.into_iter().enumerate() {
        let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
        // 会话→引擎侧索引（`SvrListIdx`；由 `GM_SERVERUSERINDEX` 下发）
        let indexes: SessionIndexes = Arc::new(Mutex::new(HashMap::new()));
        // 下游连接：连不上 GameSvr 即失败退出（M1④）
        let downstream = TcpStream::connect((ep.srv_addr.as_str(), ep.srv_port))
            .await
            .with_context(|| {
                format!(
                    "连接游戏服务 {}:{} 失败（M1④：缺服务要明确报错退出）",
                    ep.srv_addr, ep.srv_port
                )
            })?;
        downstream.set_nodelay(true).ok();
        glog!(
            "INF",
            "游戏服务器[{}:{}]链接成功.",
            ep.srv_addr,
            ep.srv_port
        );
        let (down_rx, down_tx) = downstream.into_split();
        let down_tx = Arc::new(Mutex::new(down_tx));
        tasks.push(tokio::spawn(server_loop(
            down_rx,
            Arc::clone(&sessions),
            Arc::clone(&indexes),
            ep.srv_addr.clone(),
            ep.srv_port,
        )));

        let listener = TcpListener::bind((ep.gate_addr.as_str(), ep.gate_port))
            .await
            .with_context(|| {
                format!("绑定 {}:{} 失败（端口被占？）", ep.gate_addr, ep.gate_port)
            })?;
        glog!(
            "INF",
            "游戏网关[{}:{}]已启动...",
            ep.gate_addr,
            ep.gate_port
        );
        let gate_ctx = GateCtx {
            gate_addr: ep.gate_addr.clone(),
            gate_port: ep.gate_port,
            // `Conf\ConfigManager.cs:18`：`GateList[i].ServiceId = (byte)i`
            // ⇒ 第 i 组端点的线号就是 i（合成登录包的第 7 段用它）。
            service_id: u8::try_from(i).unwrap_or(0),
            sessions,
            indexes,
            down_tx,
            gate_rules,
        };
        tasks.push(tokio::spawn(accept_loop(listener, gate_ctx)));
    }
    for t in tasks {
        let _ = t.await;
    }
    Ok(())
}

/// 一条端点线的共享上下文：把「监听标识 + 下游发送端 + 会话表 + 规则」打包，
/// 免得每个函数都排一长串参数（clippy `too_many_arguments`）。
#[derive(Clone)]
struct GateCtx {
    /// 监听地址（只用于日志与 `SessionId` 播种，见 `gen_session_id`）。
    gate_addr: String,
    /// 监听端口（同上）。
    gate_port: u16,
    /// 该端点的线号（`Conf\ConfigManager.cs:18`：`GateList[i].ServiceId = (byte)i`）——
    /// 合成登录包的第 7 段（`ClientSession.cs:1203`）。
    service_id: u8,
    /// 本线会话表：`SessionId → 该客户端连接的发送端`。
    sessions: Sessions,
    /// 本线会话→引擎侧索引（`SvrListIdx`）。
    indexes: SessionIndexes,
    /// 本线到 GameSvr 的发送端。
    down_tx: Arc<Mutex<OwnedWriteHalf>>,
    /// 本线生效的网关规则（速度/间隔）。
    gate_rules: rules::GateRules,
}

/// 一帧的处置结果（读循环据此决定下一步）。
enum Step {
    /// 处理完毕（含已转发），继续读下一帧。
    Next,
    /// 登录帧通过——已按 `ClientSession.cs:1199-1224` 补发合成登录包。
    LoggedIn,
    /// 按 oracle 剔除/关闭该会话（附日志原因）。
    Close(String),
    /// 静默丢弃（不转发）：聊天间隔未到（`ClientSession.cs:492-495` 的 `return`）。
    Drop,
}

/// 客户端 face：接受连接并为每个会话分配 `SessionId`。
async fn accept_loop(listener: TcpListener, ctx: GateCtx) -> Result<()> {
    let mut used: std::collections::HashSet<u16> = std::collections::HashSet::new();
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        if used.len() >= MAX_SESSION {
            glog!("WRN", "会话已满({MAX_SESSION})，拒绝 {peer}");
            continue;
        }
        let handle = raw_socket_i32(&sock);
        let sid = gen_session_id(&format!("{peer}|{}|{}", ctx.gate_addr, ctx.gate_port));
        used.insert(sid);
        glog!("INF", "开始连接: {peer} (SessionId={sid} socket={handle})");
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if let Err(e) = client_loop(sock, sid, handle, peer.to_string(), ctx).await {
                glog!("WRN", "客户端[{peer}]会话结束: {e:#}");
            }
        });
    }
}

/// 单客户端会话：登记会话表 → 发 `GM_OPEN` → 逐帧处置 → 断开发 `GM_CLOSE`。
async fn client_loop(
    sock: TcpStream,
    sid: u16,
    handle: i32,
    peer: String,
    ctx: GateCtx,
) -> Result<()> {
    let (mut rd, mut wr) = sock.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    ctx.sessions.lock().await.insert(sid, tx);
    // 每会话的间隔计时（C# `ClientSession` 里的 `gameSpeed`）
    let mut chat = rules::ChatThrottle::default();

    // 回写任务：把 `server_loop` 已经拼好的**客户端帧**（`#…!`，见 `client_frame`）原样写给客户端
    let writer = tokio::spawn(async move {
        while let Some(buf) = rx.recv().await {
            if wr.write_all(&buf).await.is_err() {
                break;
            }
            let _ = wr.flush().await;
        }
    });

    // 新玩家进游戏：GM_OPEN(session_id, socket=handle, session_index=0, pack_length=IP 串长度) + body=IP
    // （实机取证确认；协议梳理 §7.5）
    let ip_bytes = ip_bytes(&peer);
    let open = ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: handle,
        session_id: sid,
        ident: u16::from(grobal2::GM_OPEN),
        session_index: 0,
        pack_length: i32::try_from(ip_bytes.len()).unwrap_or(i32::MAX),
    };
    send_internal(&ctx.down_tx, open, &ip_bytes).await?;

    // 读循环：**客户端 face 无定长头**，一次 read 就是**一帧**（`#…!`）——照 oracle
    // （`ServerService.Initialize()` 的 TcpService 不挂 DataHandlingAdapter ⇒ `ProcessSessionPacket`
    // 直接吃一次 read 的字节）。帧被 TCP 拆开/合并时的后果与 oracle 一致（都会被剔除）。
    let mut buf = vec![0u8; MAX_READ];
    let mut logged_in = false;
    let mut closed = Ok(());
    loop {
        let n = match rd.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        match handle_frame(&buf[..n], logged_in, &mut chat, &ctx, sid, handle).await {
            Ok(Step::Next | Step::Drop) => {}
            Ok(Step::LoggedIn) => {
                logged_in = true;
                glog!("INF", "客户端登录通过: {peer} (SessionId={sid})");
            }
            Ok(Step::Close(reason)) => {
                glog!("WRN", "{reason} SessionId={sid}");
                break;
            }
            Err(e) => {
                closed = Err(e);
                break;
            }
        }
    }

    // 离开：GM_CLOSE(session_id=0, socket=handle)（实机取证确认）
    ctx.sessions.lock().await.remove(&sid);
    ctx.indexes.lock().await.remove(&sid);
    let leave = ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: handle,
        session_id: 0,
        ident: u16::from(grobal2::GM_CLOSE),
        session_index: 0,
        pack_length: 0,
    };
    let _ = send_internal(&ctx.down_tx, leave, &[]).await;
    writer.abort();
    glog!("INF", "断开链接: {peer} (SessionId={sid})");
    closed
}

/// 处置一帧（分支顺序照 `ClientSession.cs:98-` 的 `ProcessSessionPacket`：先判 `HandleLogin`，再看长度）。
async fn handle_frame(
    frame: &[u8],
    logged_in: bool,
    chat: &mut rules::ChatThrottle,
    ctx: &GateCtx,
    sid: u16,
    handle: i32,
) -> Result<Step> {
    if !logged_in {
        // 登录前走 oracle 的 `:671-680` else 分支（直接 `EDCode.DeCodeString`），**没有**
        // 「1 字节 = 心跳」与「< 16 字节 = 异常封包」这两条（那两条在 `HandleLogin` 分支里 `:155-164`）。
        // 1/2 字节会让 C# 的 `destinationSpan[2..(packetLen-1)]` 越界抛
        // ArgumentOutOfRangeException，被 `ServerManager.cs:238` 的 catch 吞掉（只记日志）
        // ⇒ 帧被丢、**会话存活**；这里照此丢弃（不是剔除）。
        if frame.len() < 3 {
            return Ok(Step::Next);
        }
        return handle_login(frame, ctx, sid, handle).await;
    }
    if frame.len() == 1 {
        // 心跳：`ClientSession.cs:157-160` 直接 `return`（不转发）
        return Ok(Step::Next);
    }
    if frame.len() < COMMAND_FIXED_LENGTH {
        // 异常消息封包（`:155-162`）：关会话
        return Ok(Step::Close(format!(
            "异常消息封包（{} 字节），关闭会话...",
            frame.len()
        )));
    }
    let payload = &frame[2..frame.len() - 1]; // = C# 的 `tempBuff`（去掉 `#1` 与尾 `!`）
    let decoded = mir2_protocol::edcode::decode(payload);
    if decoded.len() < COMMAND_MESSAGE_SIZE {
        // `:170-175`：解码后不足 12 字节 ⇒ 关会话（reviewer-1 的 Minor #3）
        return Ok(Step::Close(format!(
            "解析数据包小于12...解析后长度:{}",
            decoded.len()
        )));
    }
    if frame.len() > NOM_CLIENT_PACKET_SIZE {
        // `:196-199`：超过 `NomClientPacketSize` **只记日志丢帧**（不是关会话）
        glog!(
            "INF",
            "Kick off user,over nom client packet size: {}",
            frame.len()
        );
        return Ok(Step::Next);
    }
    // `CM_SAY` 的间隔门（`:485-497`）：太快的聊天帧**直接丢**（`return`，不转发）。
    // 内容 = 解码后的 `decodeBuff[12..]`；`@` 开头的内容不判间隔；开关与阈值照现网
    // `[Switch] ChatInterval=1` / `[Integer] ChatInterval=800`。
    if ctx.gate_rules.chat_interval_enabled
        && u16::from_le_bytes([decoded[4], decoded[5]]) == mir2_protocol::messages::CM_SAY
        && !chat.allows(
            now_ms(),
            &decoded[COMMAND_MESSAGE_SIZE..],
            ctx.gate_rules.chat_interval_ms,
        )
    {
        glog!(
            "DBG",
            "聊天间隔未到（{}ms），丢弃该帧（SessionId={sid}）",
            ctx.gate_rules.chat_interval_ms
        );
        return Ok(Step::Drop);
    }
    // 命令路径的出帧重建（照 `ClientSession.cs:646-669`，**不是搬字节**）：口径见 `rebuild_command_frame`。
    let session_index = *ctx.indexes.lock().await.get(&sid).unwrap_or(&0);
    let wire = rebuild_command_frame(&decoded, payload, sid, session_index);
    send_raw(&ctx.down_tx, &wire).await?;
    Ok(Step::Next)
}

/// 登录帧（`:673-679` → `ClientLogin`）：校验通过 ⇒ **补发合成登录包**（`:1199-1224`）并置“已登录”；
/// 失败 ⇒ 剔除（`:677-678` 的 `Kick(1)`）。登录帧本身**不转发**。
async fn handle_login(body: &[u8], ctx: &GateCtx, sid: u16, handle: i32) -> Result<Step> {
    let text = gbk::decode(&mir2_protocol::edcode::decode(&body[2..body.len() - 1]));
    match login::parse_client_login(&text, body.len()) {
        Err(reason) => Ok(Step::Close(format!(
            "客户端登陆消息处理失败，剔除链接: {reason}"
        ))),
        Ok(fields) => {
            let login_body = login::synthetic_login_body(&fields, ctx.service_id);
            let head = ServerMessage {
                packet_code: grobal2::PacketCode,
                socket: handle,
                session_id: sid,
                ident: u16::from(grobal2::GM_DATA),
                // `:1217` 用会话当时的 `SessionIndex`（`SvrListIdx`；登录阶段通常还是 0）
                session_index: *ctx.indexes.lock().await.get(&sid).unwrap_or(&0),
                pack_length: i32::try_from(login_body.len()).unwrap_or(i32::MAX),
            };
            send_internal(&ctx.down_tx, head, &login_body).await?;
            Ok(Step::LoggedIn)
        }
    }
}

/// 网关 ↔ GameSvr：收管理帧并分派（`GM_*`，`ClientThread.cs:265-300` 的同构实现）。
async fn server_loop(
    mut rd: tokio::net::tcp::OwnedReadHalf,
    sessions: Sessions,
    indexes: SessionIndexes,
    srv_addr: String,
    srv_port: u16,
) -> Result<()> {
    loop {
        let mut head = [0u8; ServerMessage::SIZE];
        if rd.read_exact(&mut head).await.is_err() {
            glog!("ERR", "游戏服务连接断开，网关停止接收。");
            return Ok(());
        }
        let Some(msg) = ServerMessage::from_bytes(&head) else {
            continue;
        };
        let body_len = msg.body_len();
        let mut body = vec![0u8; body_len];
        if body_len > 0 && rd.read_exact(&mut body).await.is_err() {
            glog!("ERR", "读取游戏服务载荷失败。");
            return Ok(());
        }
        match u8::try_from(msg.ident).unwrap_or(0) {
            grobal2::GM_DATA => {
                let wire = client_frame(msg.pack_length, &body);
                let guard = sessions.lock().await;
                if let Some(tx) = guard.get(&msg.session_id) {
                    let _ = tx.send(wire);
                } else {
                    glog!(
                        "WRN",
                        "会话[{}]不存在，丢弃下行 {} 字节",
                        msg.session_id,
                        body.len()
                    );
                }
            }
            grobal2::GM_CLOSE => {
                sessions.lock().await.remove(&msg.session_id);
                indexes.lock().await.remove(&msg.session_id);
                glog!("INF", "游戏服务要求关闭会话[{}]", msg.session_id);
            }
            grobal2::GM_CHECKSERVER => {}
            grobal2::GM_SERVERUSERINDEX => {
                // `ClientThread.cs:272-275`：把 SessionIndex 记到会话的 `SvrListIdx`，
                // 之后该会话发往 GameSvr 的每帧都带它（`ClientSession.cs:649`）。
                indexes
                    .lock()
                    .await
                    .insert(msg.session_id, msg.session_index);
                glog!(
                    "DBG",
                    "服务端分配会话索引：SessionId={} index={}",
                    msg.session_id,
                    msg.session_index
                );
            }
            grobal2::GM_STOP => {
                glog!(
                    "WRN",
                    "游戏引擎停止服务（{}:{}），网关不再分配用户连接到该实例",
                    srv_addr,
                    srv_port
                );
            }
            other => glog!("DBG", "未处理的 GM_{other}（SessionId={}）", msg.session_id),
        }
    }
}

/// 发一帧内部消息：20B 小端头 + body。
async fn send_internal(
    tx: &Arc<Mutex<OwnedWriteHalf>>,
    msg: ServerMessage,
    body: &[u8],
) -> Result<()> {
    let mut wire = msg.to_bytes().to_vec();
    wire.extend_from_slice(body);
    send_raw(tx, &wire).await
}

/// 发一段**已经拼好**的线上字节（命令路径要自己拼 20B 头 + 解码头 + 编码尾，见 §7.9）。
async fn send_raw(tx: &Arc<Mutex<OwnedWriteHalf>>, wire: &[u8]) -> Result<()> {
    let mut w = tx.lock().await;
    w.write_all(wire).await.context("写游戏服务失败")?;
    w.flush().await.ok();
    Ok(())
}

/// 命令帧的出帧重建（照 `ClientSession.cs:646-669`；口径见协议梳理 §7.8/§7.9）：
/// - 无尾部（解码后 ≤ `COMMAND_MESSAGE_SIZE`=12）：`PackLength = 12`，body = **解码态**字节；
/// - 有尾部：`PackLength = 12 + tLen + 1`（`tLen` = 尾部重编码后的长度），
///   body = 解码态 12B + **输入的编码态尾部**（`payload[16..]`）+ 1 字节；
/// - 那 1 字节在 oracle 里是 `BytePool.Rent` 的**未初始化内存**，这里固定送 `0`（已知的不可比对差异）。
fn rebuild_command_frame(decoded: &[u8], payload: &[u8], sid: u16, session_index: i32) -> Vec<u8> {
    let mut wire: Vec<u8> = Vec::with_capacity(ServerMessage::SIZE + decoded.len() + 1);
    let (pack_length, tail): (i32, &[u8]) = if decoded.len() > COMMAND_MESSAGE_SIZE {
        let t_len = mir2_protocol::edcode::encode(&decoded[COMMAND_MESSAGE_SIZE..]).len();
        let pl = i32::try_from(COMMAND_MESSAGE_SIZE + t_len + 1).unwrap_or(i32::MAX);
        let tail = if payload.len() > COMMAND_FIXED_LENGTH {
            &payload[COMMAND_FIXED_LENGTH..]
        } else {
            &[]
        };
        (pl, tail)
    } else {
        (i32::try_from(COMMAND_MESSAGE_SIZE).unwrap_or(i32::MAX), &[])
    };
    let head = ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: 0,
        session_id: sid,
        ident: u16::from(grobal2::GM_DATA),
        session_index,
        pack_length,
    };
    wire.extend_from_slice(&head.to_bytes());
    if decoded.len() > COMMAND_MESSAGE_SIZE {
        wire.extend_from_slice(&decoded[..COMMAND_MESSAGE_SIZE]);
        wire.extend_from_slice(tail);
        wire.push(0); // oracle 的未初始化字节（我们送 0）
    } else {
        wire.extend_from_slice(decoded);
    }
    wire
}

/// 网关 → 客户端的出帧（照 `ClientSession.cs:891-924` 的 `ProcessServerPacket`）：
///
/// - `PackLength < 0`（"小包"：走路/攻击等）⇒ `#` + body **原样** + `!`（**不编码**）；
/// - 否则 ⇒ `#` + `Encode(body[..12])` + `body[12..]` **原样** + `!`。
///
/// 实测（真 C# 网关 + 真客户端夹具，`oracle-login-cap/FINDINGS.md`）：`PackLength=+16` 时客户端收到
/// `23` + `68697a6b6b68756a6a6b7066696a7361` + `1c1d1e1f` + `21`（= `#`+Encode(12B)+余下+`!`）；
/// `PackLength=-4` 时收到 `23 deadbeef 21`。
///
/// 已知不可等价的角落：`0 <= len < 12` 的正数包——oracle `:917` 固定编码 **12** 字节（读越界），
/// 这里按实际长度编码；真实服务端不会发这种包，`:926-928` 的 `bufferLen > 8` 分支还会在
/// `Deserialize<CommandMessage>` 时抛异常被 `ClientThread` 吞掉（实测客户端收不到任何东西）。
fn client_frame(pack_length: i32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.push(b'#');
    if pack_length < 0 {
        out.extend_from_slice(body);
    } else if body.len() >= COMMAND_MESSAGE_SIZE {
        out.extend_from_slice(&mir2_protocol::edcode::encode(
            &body[..COMMAND_MESSAGE_SIZE],
        ));
        out.extend_from_slice(&body[COMMAND_MESSAGE_SIZE..]);
    } else {
        out.extend_from_slice(&mir2_protocol::edcode::encode(body));
    }
    out.push(b'!');
    out
}

/// 客户端 socket 句柄（C# 用 `Socket.Handle.ToInt32()`；Windows 下取 raw socket 低 32 位，
/// 其它平台给 0——该字段的用途仍属 `TODO(差分)`，见协议梳理 §7.3）。
#[cfg(windows)]
fn raw_socket_i32(sock: &TcpStream) -> i32 {
    use std::os::windows::io::AsRawSocket;
    // C# 用 `Socket.Handle.ToInt32()`（截断到低 32 位，本字段语义见协议梳理 §7.3）
    let low = u32::try_from(sock.as_raw_socket()).unwrap_or(0);
    i32::from_ne_bytes(low.to_ne_bytes())
}

#[cfg(not(windows))]
fn raw_socket_i32(_sock: &TcpStream) -> i32 {
    0
}

/// 取客户端 IP 的字节（`GM_OPEN` 体；C# 侧 `HUtil32.GetBytes(IP字符串)`，ASCII 下与字节串一致）。
fn ip_bytes(peer: &str) -> Vec<u8> {
    let ip = peer.rsplit_once(':').map_or(peer, |(h, _)| h);
    ip.as_bytes().to_vec()
}

/// 当前毫秒时间。C# 侧是 `HUtil32.GetTickCount()`（开机以来毫秒）；这里用 epoch 毫秒，
/// **只有差值与间隔比较参与判定**，基准不同不影响语义（协议梳理 §7.10.2 已说明差分需要喂虚拟时间）。
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// C# `ClientThread.GetSessionId`（`ClientThread.cs:128-160`）的等价实现：
/// 取"连接标识"哈希最低 8 位，与 4 个随机字节逐字节异或、每个 `% 10`、拼接成 4 位数字串再 `TryParse`
/// ⇒ **取值 0..=9999 且随机**（C# 侧也没有唯一性检查）。这里用系统时间纳秒 + 入参做种子，
/// 保持"随机 4 位"这一可观测形态；**差分/golden 必须归一化该字段**。
fn gen_session_id(seed_src: &str) -> u16 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    seed_src.hash(&mut h);
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos())
        .hash(&mut h);
    let hash_lo = (h.finish() & 0xFF) as u8;
    let mut state = h.finish() ^ 0x9E37_79B9_7F4A_7C15;
    let mut digits = String::with_capacity(4);
    for _ in 0..4 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let rnd = u8::try_from((state >> 33) & 0xFF).unwrap_or(0);
        digits.push(char::from(b'0' + (rnd ^ hash_lo) % 10));
    }
    digits.parse::<u16>().unwrap_or(0)
}
