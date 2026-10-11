//! Rust 版 **GameGate/RunGate**（M2 入口：顶住 7200，把会话中继进 GameSvr）。
//!
//! 协议面与行为面的取证见 `mir2-rs/docs/M2-GameGate协议梳理.md`（含一次实机取证：假 GameSvr +
//! 真 C# GameGate 的逐帧观测）。本文件按已证实的部分实现，**未证实的字段一律用 `TODO(差分)` 标出**，
//! 不假装等价：
//!
//! - 客户端 face：TCP 7200，帧是 **20B `ServerMessage` 头 + body**（`crates/protocol::frame::ServerMessage`
//!   已字节级对齐 C#）。body 里是经典 `#1…!` EDCode 帧——**本切片原样透传，不解码**（C# 会解码/重编码，
//!   见协议梳理 §2 的 ⚠️，差分确认前不算等价）。
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
    glog!(
        "INF",
        "游戏网关启动：{} 组端点（config={}）",
        cfg.endpoints.len(),
        conf.display()
    );

    let mut tasks = Vec::new();
    for ep in cfg.endpoints {
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
        let gate_addr = ep.gate_addr.clone();
        tasks.push(tokio::spawn(accept_loop(
            listener,
            gate_addr,
            ep.gate_port,
            sessions,
            indexes,
            down_tx,
        )));
    }
    for t in tasks {
        let _ = t.await;
    }
    Ok(())
}

/// 客户端 face：接受连接并为每个会话分配 `SessionId`。
async fn accept_loop(
    listener: TcpListener,
    gate_addr: String,
    gate_port: u16,
    sessions: Sessions,
    indexes: SessionIndexes,
    down_tx: Arc<Mutex<OwnedWriteHalf>>,
) -> Result<()> {
    let mut used: std::collections::HashSet<u16> = std::collections::HashSet::new();
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        if used.len() >= MAX_SESSION {
            glog!("WRN", "会话已满({MAX_SESSION})，拒绝 {peer}");
            continue;
        }
        let handle = raw_socket_i32(&sock);
        let sid = gen_session_id(&format!("{peer}|{gate_addr}|{gate_port}"));
        used.insert(sid);
        glog!("INF", "开始连接: {peer} (SessionId={sid} socket={handle})");
        let sessions = Arc::clone(&sessions);
        let indexes = Arc::clone(&indexes);
        let down_tx = Arc::clone(&down_tx);
        tokio::spawn(async move {
            if let Err(e) = client_loop(
                sock,
                sid,
                handle,
                peer.to_string(),
                sessions,
                indexes,
                down_tx,
            )
            .await
            {
                glog!("WRN", "客户端[{peer}]会话结束: {e:#}");
            }
        });
    }
}

/// 单客户端会话：登记会话表 → 发 `GM_OPEN` → 逐帧搬进 `GM_DATA` → 断开发 `GM_CLOSE`。
async fn client_loop(
    sock: TcpStream,
    sid: u16,
    handle: i32,
    peer: String,
    sessions: Sessions,
    indexes: SessionIndexes,
    down_tx: Arc<Mutex<OwnedWriteHalf>>,
) -> Result<()> {
    let (mut rd, mut wr) = sock.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    sessions.lock().await.insert(sid, tx);

    // 回写任务：把 GameSvr 发来的 body 原样写给客户端（TODO(差分)：C# 会重新编码，见协议梳理 §2 ⚠️）
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
    send_internal(&down_tx, open, &ip_bytes).await?;

    // 读循环：20B 定长头 + body
    let mut logged_in = false;
    let mut closed = Ok(());
    loop {
        let mut head = [0u8; ServerMessage::SIZE];
        if rd.read_exact(&mut head).await.is_err() {
            break;
        }
        let Some(msg) = ServerMessage::from_bytes(&head) else {
            continue;
        };
        let body_len = msg.body_len();
        if body_len == 1 {
            // 心跳：C# 侧收到 1 字节包直接忽略（ClientSession.cs:157-160）
            let mut hb = [0u8; 1];
            let _ = rd.read_exact(&mut hb).await;
            continue;
        }
        if body_len < COMMAND_FIXED_LENGTH {
            // 异常消息封包，关闭会话（ClientSession.cs:155-162）
            glog!(
                "WRN",
                "异常消息封包（{body_len} 字节），关闭会话... SessionId={sid}"
            );
            closed = Ok(());
            break;
        }
        let mut body = vec![0u8; body_len];
        if rd.read_exact(&mut body).await.is_err() {
            break;
        }
        if !logged_in {
            // 登录帧（切片 2）：照 ClientSession.cs:166（跳过 `#1` 与尾 `!`）→ :673 EDCode 解码 →
            // :674 ClientLogin 校验；失败即剔除（:677-678），通过则等价于 oracle 的 `HandleLogin = true`（:1228）。
            if body.len() < 3 {
                glog!(
                    "WRN",
                    "登录帧过短（{} 字节），剔除会话 SessionId={sid}",
                    body.len()
                );
                break;
            }
            let payload = &body[2..body.len() - 1];
            let text = gbk::decode(&mir2_protocol::edcode::decode(payload));
            match login::parse_client_login(&text, body.len()) {
                Ok(()) => {
                    logged_in = true;
                    glog!("INF", "客户端登录通过: {peer} (SessionId={sid})");
                }
                Err(reason) => {
                    glog!("INF", "客户端登陆消息处理失败，剔除链接: {reason}");
                    break;
                }
            }
            continue; // 登录帧不转发给 GameSvr（oracle 的 else 分支也不发）
        }
        // 命令路径的出帧重建（照 `ClientSession.cs:646-669`，**不是搬字节**）：
        // - 固定头 12B（`CommandMessage.Size`）用**解码态**字节；
        // - 有尾部时：`PackLength = 12 + tLen + 1`，尾部字节直接抄**输入的编码态**（oracle 里
        //   `sendBuffer` 算了却没用，抄的是 `tempBuff[16..]`）；
        // - 末尾那 1 字节 oracle 是**未初始化的池内存**（`bodyBuffer` 来自 `BytePool.Rent`），
        //   这里送 0 —— **已知的不可比对差异**，见协议梳理 §7.9。
        // - `SessionIndex` 取本会话的 `SvrListIdx`（`:649`）。
        let session_index = *indexes.lock().await.get(&sid).unwrap_or(&0);
        let wire = rebuild_command_frame(&body, sid, session_index);
        send_raw(&down_tx, &wire).await?;
    }

    // 离开：GM_CLOSE(session_id=0, socket=handle)（实机取证确认）
    sessions.lock().await.remove(&sid);
    indexes.lock().await.remove(&sid);
    let leave = ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: handle,
        session_id: 0,
        ident: u16::from(grobal2::GM_CLOSE),
        session_index: 0,
        pack_length: 0,
    };
    let _ = send_internal(&down_tx, leave, &[]).await;
    writer.abort();
    glog!("INF", "断开链接: {peer} (SessionId={sid})");
    closed
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
                let guard = sessions.lock().await;
                if let Some(tx) = guard.get(&msg.session_id) {
                    // TODO(差分)：C# 在此会重新编码（§2 ⚠️）；本切片原样写 body
                    let _ = tx.send(body);
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
fn rebuild_command_frame(body: &[u8], sid: u16, session_index: i32) -> Vec<u8> {
    let payload = &body[2..body.len() - 1]; // = C# 的 `tempBuff`（去掉 `#1` 与尾 `!`）
    let decoded = mir2_protocol::edcode::decode(payload);
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
        wire.extend_from_slice(&decoded);
    }
    wire
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
