//! Rust 版 **LoginGate**（M1 第一步：顶住 7000，真实客户端能登录）。
//!
//! 1:1 对齐 C# `src/LoginGate/**` 的**行为边界**：
//! - 客户端面：TCP 7000，帧形 `#1…!`（EDCode；客户端心跳是 1 字节 `*`）；
//! - 下行链路：连 LoginSrv（默认 5500），用 6B `ServerDataPacket` 头 + MemoryPack
//!   `ServerDataMessage`（`crates/protocol::internal` 已字节级对齐 C#）；
//! - 网关**不重新编码客户端载荷**：`Data` 里放的就是客户端原始帧字节（C# 侧同样原样转发），
//!   所以客户端编码的细节在本进程里只是**不透明字节**——这也保证了 `!$` 帧尾等形态天然透传；
//! - 会话多路复用：一个下游连接承载全部客户端会话，靠 `SocketId` 区分（照 C# `ClientThread`）；
//! - `Leave`/`KeepAlive` 的处理与 C# 一致（Leave 关闭该客户端会话，KeepAlive 仅续命）。
//!
//! **失败要响**（M1 验收④）：缺配置键、绑不上端口、连不上 LoginSrv —— 都明确报错并以非零码退出。

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

mod config;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use mir2_protocol::frame::{FrameSplitter, ServerDataPacket, SplitOut};
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

/// C# Serilog 的输出模板是 `{Timestamp:HH:mm:ss.fff} [{Level:u3}] {Message:lj}`；
/// 这里用同形前缀，便于 M1 验收①的"两侧日志时间线"对齐。
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

/// 会话表：`SocketId → 该客户端连接的发送端`。
type Sessions = Arc<Mutex<HashMap<u16, mpsc::UnboundedSender<Vec<u8>>>>>;

#[tokio::main]
async fn main() -> Result<()> {
    // 配置路径：可选 argv[1] 覆盖（默认取可执行文件同目录，与 C# BaseDirectory 一致）
    let conf = std::env::args()
        .nth(1)
        .map_or_else(config::default_path, std::path::PathBuf::from);
    let cfg = config::load(&conf)?;
    glog!(
        "INF",
        "登录网关启动：监听 {}:{} → 登录服务 {}:{}",
        cfg.gate_addr,
        cfg.gate_port,
        cfg.srv_addr,
        cfg.srv_port
    );

    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));

    // 下行连接：连不上 LoginSrv 即失败退出（M1④）
    let downstream = TcpStream::connect((cfg.srv_addr.as_str(), cfg.srv_port))
        .await
        .with_context(|| {
            format!(
                "连接登录服务 {}:{} 失败（M1④：缺服务要明确报错退出）",
                cfg.srv_addr, cfg.srv_port
            )
        })?;
    downstream.set_nodelay(true).ok();
    glog!(
        "INF",
        "账号服务器[{}:{}]链接成功.",
        cfg.srv_addr,
        cfg.srv_port
    );
    let (down_rx, down_tx) = downstream.into_split();
    let down_tx = Arc::new(Mutex::new(down_tx));

    // 下行读取循环（LoginSrv → 各客户端）
    tokio::spawn(downstream_loop(down_rx, Arc::clone(&sessions)));

    let listener = TcpListener::bind((cfg.gate_addr.as_str(), cfg.gate_port))
        .await
        .with_context(|| {
            format!(
                "绑定 {}:{} 失败（端口被占？）",
                cfg.gate_addr, cfg.gate_port
            )
        })?;
    glog!(
        "INF",
        "登录网关[{}:{}]已启动...",
        cfg.gate_addr,
        cfg.gate_port
    );

    let next_sid = Arc::new(AtomicU16::new(1));
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        let sid = next_sid.fetch_add(1, Ordering::Relaxed);
        glog!("INF", "开始连接: {peer}");
        let sessions = Arc::clone(&sessions);
        let down_tx = Arc::clone(&down_tx);
        tokio::spawn(async move {
            if let Err(e) = client_loop(sock, sid, peer.to_string(), sessions, down_tx).await {
                glog!("WRN", "客户端[{peer}]会话结束: {e:#}");
            }
        });
    }
}

/// 单客户端会话：读客户端字节 → 拆帧 → 透传给 LoginSrv；并把该会话登记到会话表。
async fn client_loop(
    sock: TcpStream,
    sid: u16,
    peer: String,
    sessions: Sessions,
    down_tx: Arc<Mutex<OwnedWriteHalf>>,
) -> Result<()> {
    let (mut rd, mut wr) = sock.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // 会话登记 + 通知 LoginSrv "新会话接入"（C# 的 Enter 语义：体为客户端 IP，此处给 IP:port）
    sessions.lock().await.insert(sid, tx);
    let enter = ServerDataMessage::new(
        ServerDataType::Enter,
        sid.to_string(),
        peer.as_bytes().to_vec(),
    );
    send_internal(&down_tx, &enter).await?;

    // 回写任务：把 LoginSrv 发来的原始帧字节写给客户端
    let writer = tokio::spawn(async move {
        while let Some(buf) = rx.recv().await {
            if wr.write_all(&buf).await.is_err() {
                break;
            }
            let _ = wr.flush().await;
        }
    });

    // 读循环：拆帧（含 `*` 心跳）后原样透传
    let mut splitter = FrameSplitter::new();
    let mut buf = vec![0u8; 8192];
    loop {
        let n = rd.read(&mut buf).await.context("读客户端失败")?;
        if n == 0 {
            break;
        }
        for item in splitter.feed(&buf[..n]) {
            match item {
                // 心跳：C# 侧收到 1 字节包直接忽略；这里同样不转发
                SplitOut::Heartbeat => {}
                SplitOut::Frame(frame) => {
                    let msg = ServerDataMessage {
                        kind: ServerDataType::Data,
                        socket_id: sid.to_string(),
                        data_len: i16::try_from(frame.len()).unwrap_or(i16::MAX),
                        data: frame,
                    };
                    send_internal(&down_tx, &msg).await?;
                }
            }
        }
    }

    // 会话离开（C# Leave 语义）
    sessions.lock().await.remove(&sid);
    let leave = ServerDataMessage::new(ServerDataType::Leave, sid.to_string(), Vec::new());
    let _ = send_internal(&down_tx, &leave).await;
    writer.abort();
    glog!("INF", "断开链接: {peer} (SessionId={sid})");
    Ok(())
}

/// 下行读取：LoginSrv → 按 `SocketId` 分派给对应客户端（`Data`），`Leave` 关闭会话。
async fn downstream_loop(mut rd: OwnedReadHalf, sessions: Sessions) -> Result<()> {
    loop {
        let mut head = [0u8; ServerDataPacket::SIZE];
        if rd.read_exact(&mut head).await.is_err() {
            glog!("ERR", "登录服务连接断开，网关停止接收。");
            return Ok(());
        }
        let Some(pkt) = ServerDataPacket::from_bytes(&head) else {
            continue;
        };
        let mut body = vec![0u8; usize::from(pkt.packet_len)];
        if rd.read_exact(&mut body).await.is_err() {
            glog!("ERR", "读取登录服务载荷失败。");
            return Ok(());
        }
        let Ok(msg) = ServerDataMessage::from_bytes(&body) else {
            glog!("WRN", "内部帧解析失败（长度 {}）", body.len());
            continue;
        };
        match msg.kind {
            ServerDataType::Data => {
                let Ok(sid) = msg.socket_id.parse::<u16>() else {
                    continue;
                };
                let guard = sessions.lock().await;
                if let Some(tx) = guard.get(&sid) {
                    let _ = tx.send(msg.data);
                } else {
                    glog!("WRN", "会话[{sid}]不存在，丢弃下行 {} 字节", msg.data.len());
                }
            }
            ServerDataType::Leave => {
                let Ok(sid) = msg.socket_id.parse::<u16>() else {
                    continue;
                };
                sessions.lock().await.remove(&sid);
                glog!("INF", "登录服务要求关闭会话[{sid}]");
            }
            // KeepAlive 仅续命（C# 侧同样不转发）；Enter 是网关→服务方向，服务回发时无动作
            ServerDataType::KeepAlive | ServerDataType::Enter => {}
        }
    }
}

/// 发一帧内部消息：`[PacketCode u32][PacketLen u16]` + MemoryPack 体。
async fn send_internal(tx: &Arc<Mutex<OwnedWriteHalf>>, msg: &ServerDataMessage) -> Result<()> {
    let wire = msg.to_wire();
    let mut w = tx.lock().await;
    w.write_all(&wire).await.context("写登录服务失败")?;
    w.flush().await.ok();
    Ok(())
}
