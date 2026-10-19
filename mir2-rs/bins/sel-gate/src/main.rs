//! Rust 版 **SelGate**（M1 第一步：顶住 7000，真实客户端能登录）。
//!
//! 1:1 对齐 C# `src/SelGate/**` 的**行为边界**：
//! - 客户端面：TCP 7000，帧形 `#1…!`（EDCode；客户端心跳是 1 字节 `*`）；
//! - 下行链路：连 DBSrv（默认 5500），用 6B `ServerDataPacket` 头 + MemoryPack
//!   `ServerDataMessage`（`crates/protocol::internal` 已字节级对齐 C#）；
//! - 网关**不重新编码客户端载荷**：`Data` 里放的就是客户端原始帧字节（C# 侧同样原样转发），
//!   所以客户端编码的细节在本进程里只是**不透明字节**——这也保证了 `!$` 帧尾等形态天然透传；
//! - 会话多路复用：一个下游连接承载全部客户端会话，靠 `SocketId` 区分（照 C# `ClientThread`）；
//! - `Leave`/`KeepAlive` 的处理与 C# 一致（Leave 关闭该客户端会话，KeepAlive 仅续命）。
//!
//! **失败要响**（M1 验收④）：缺配置键、绑不上端口、连不上 DBSrv —— 都明确报错并以非零码退出。

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

mod config;
mod gbk;
mod idle_timeout;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use idle_timeout::{IdleTimer, TimeoutAction, CHECK_INTERVAL_MS};
use mir2_protocol::frame::{decode_client_frame, FrameSplitter, ServerDataPacket, SplitOut};
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
        "角色网关启动：监听 {}:{} → 数据服务 {}:{}",
        cfg.gate_addr,
        cfg.gate_port,
        cfg.srv_addr,
        cfg.srv_port
    );

    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));

    // 下行连接：连不上 DBSrv 即失败退出（M1④）
    let downstream = TcpStream::connect((cfg.srv_addr.as_str(), cfg.srv_port))
        .await
        .with_context(|| {
            format!(
                "连接数据服务 {}:{} 失败（M1④：缺服务要明确报错退出）",
                cfg.srv_addr, cfg.srv_port
            )
        })?;
    downstream.set_nodelay(true).ok();
    glog!(
        "INF",
        "数据服务[{}:{}]链接成功.",
        cfg.srv_addr,
        cfg.srv_port
    );
    let (down_rx, down_tx) = downstream.into_split();
    let down_tx = Arc::new(Mutex::new(down_tx));

    // 下行读取循环（DBSrv → 各客户端）
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
        "角色网关[{}:{}]已启动...",
        cfg.gate_addr,
        cfg.gate_port
    );

    let next_sid = Arc::new(AtomicU16::new(1));
    let timeout_ms = cfg.client_timeout_ms;
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        let sid = next_sid.fetch_add(1, Ordering::Relaxed);
        glog!("INF", "开始连接: {peer}");
        let sessions = Arc::clone(&sessions);
        let down_tx = Arc::clone(&down_tx);
        tokio::spawn(async move {
            if let Err(e) = client_loop(
                sock,
                sid,
                peer.to_string(),
                peer_ip(peer),
                timeout_ms,
                sessions,
                down_tx,
            )
            .await
            {
                glog!("WRN", "客户端[{peer}]会话结束: {e:#}");
            }
        });
    }
}

/// C# `RemoteEndPoint.GetIP()`（TouchSocket `SystemNetExtension.GetIP`）口径：只取对端 IP，
/// **不含**客户端临时端口；IPv6 带方括号（实测 `[::1]`）。
fn peer_ip(peer: SocketAddr) -> String {
    match peer {
        SocketAddr::V4(v4) => v4.ip().to_string(),
        SocketAddr::V6(v6) => {
            let ip = v6.ip();
            // .NET `IPAddress.ToString()` 对 link-local 会带 `%scope`；TouchSocket 原样包进方括号。
            match v6.scope_id() {
                0 => format!("[{ip}]"),
                scope => format!("[{ip}%{scope}]"),
            }
        }
    }
}

/// C# `src/SelGate/Services/ClientSession.cs:166-175` 的 `UserEnter` 帧体：
/// `$"%K{SocketId}/{ClientIP}/{ClientIP}$"`（含前导 `%K` 与尾部 `$`）。
fn sel_gate_enter_body(socket_id: &str, ip: &str) -> Vec<u8> {
    format!("%K{socket_id}/{ip}/{ip}$").into_bytes()
}

/// 单客户端会话：读客户端字节 → 拆帧 → 透传给 DBSrv；并把该会话登记到会话表。
///
/// 空闲超时（审计 #12）照 C# `ClientSession.HandleDelayMsg` + `TimedService`：
/// - 计时起点 = 会话构造；
/// - **只有** `CM_QUERYCHR/CM_NEWCHR/CM_DELCHR/CM_SELCHR`（100..=103）刷新计时
///   （C# `HandleUserPacket` 的 4 个 `case`）；其它帧只记日志、不刷新；
/// - 每 20s 扫描一次，严格 `> timeout` 才动作；
/// - 第一阶段发 `SM_OUTOFCONNECTION` 并在同一次扫描关会话；第二阶段状态机保留但线上不可达；
/// - 超时关会话时**不发**下行 `Leave`（oracle 先摘会话，`Disconnected` 取不到会话）。
async fn client_loop(
    sock: TcpStream,
    sid: u16,
    peer: String,
    ip: String,
    timeout_ms: u64,
    sessions: Sessions,
    down_tx: Arc<Mutex<OwnedWriteHalf>>,
) -> Result<()> {
    let (mut rd, mut wr) = sock.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    // 超时帧与 DBSrv 下行共用同一条回写通道，保证「先发帧、后断开」的字节次序。
    let timeout_tx = tx.clone();

    // 会话登记 + 通知 DBSrv "新会话接入"（C# `UserEnter`：`%K{SocketId}/{ClientIP}/{ClientIP}$`）
    sessions.lock().await.insert(sid, tx);
    let sid_text = sid.to_string();
    let body = sel_gate_enter_body(&sid_text, &ip);
    // C# `accountPacket.DataLen = (short)body.Length`（unchecked 截断）。
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let data_len = body.len() as i16;
    let enter = ServerDataMessage {
        kind: ServerDataType::Enter,
        socket_id: sid_text,
        data_len,
        data: body,
    };
    send_internal(&down_tx, &enter).await?;

    // 回写任务：把 DBSrv 发来的原始帧字节写给客户端
    let writer = tokio::spawn(async move {
        while let Some(buf) = rx.recv().await {
            if wr.write_all(&buf).await.is_err() {
                break;
            }
            let _ = wr.flush().await;
        }
    });

    // 读循环：拆帧（含 `*` 心跳）后原样透传；同时按 C# `TimedService` 的节奏轮询超时。
    let mut splitter = FrameSplitter::new();
    let mut buf = vec![0u8; 8192];
    let clock_start = Instant::now();
    let now_ms = || u64::try_from(clock_start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut timer = IdleTimer::new(now_ms());
    let mut sweep = tokio::time::interval(Duration::from_millis(CHECK_INTERVAL_MS));
    sweep.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut timed_out = false;
    loop {
        tokio::select! {
            // 不用 `biased`：持续有客户端流量时也不能饿死超时扫描
            //（C# 的定时服务是独立线程，收包流量不会阻塞它）。
            read = rd.read(&mut buf) => {
                let n = read.context("读客户端失败")?;
                if n == 0 {
                    break;
                }
                for item in splitter.feed(&buf[..n]) {
                    match item {
                        // 心跳：C# 侧收到 1 字节包直接忽略；这里同样不转发
                        SplitOut::Heartbeat => {}
                        SplitOut::Frame(frame) => {
                            // C# `HandleUserPacket`：解码成功且 `Ident` 是四个角色命令之一
                            // 才 `_clientTimeOutTick = GetTickCount()`。解码失败/其它 ident
                            // 走 `default`（只记日志）⇒ 不刷新。
                            if let Ok(msg) = decode_client_frame(&frame) {
                                if idle_timeout::refreshes_on_ident(msg.head.ident) {
                                    timer.refresh(now_ms());
                                }
                            }
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
            _ = sweep.tick() => {
                match timer.poll(now_ms(), timeout_ms) {
                    TimeoutAction::None => {}
                    TimeoutAction::SendOutOfConnection => {
                        // C# `TimedService`：`success=true` ⇒ 同一次扫描立即 CloseSession。
                        let _ = timeout_tx.send(idle_timeout::out_of_connection_frame());
                        glog!("INF", "Client Connect Time Out: {ip}");
                        timed_out = true;
                        break;
                    }
                    // 线上不可达（第一阶段已关会话）；保留以对齐 oracle 状态机。
                    TimeoutAction::Close => {
                        timed_out = true;
                        break;
                    }
                }
            }
        }
    }

    // 会话离开（C# Leave 语义）。超时路径不发：oracle 的 TimedService 先摘会话再关
    // socket，`Disconnected` 处理器 `GetSession` 为 null ⇒ 不产生 `UserLeave`。
    sessions.lock().await.remove(&sid);
    if !timed_out {
        let leave = ServerDataMessage::new(ServerDataType::Leave, sid.to_string(), Vec::new());
        let _ = send_internal(&down_tx, &leave).await;
    }
    // 等回写任务把队列（含超时帧）写完再 drop 写半端：C# 是 Send 后 Close。
    drop(timeout_tx);
    let _ = writer.await;
    glog!("INF", "断开链接: {peer} (SessionId={sid})");
    Ok(())
}

/// 下行读取：DBSrv → 按 `SocketId` 分派给对应客户端（`Data`），`Leave` 关闭会话。
async fn downstream_loop(mut rd: OwnedReadHalf, sessions: Sessions) -> Result<()> {
    loop {
        let mut head = [0u8; ServerDataPacket::SIZE];
        if rd.read_exact(&mut head).await.is_err() {
            glog!("ERR", "数据服务连接断开，网关停止接收。");
            return Ok(());
        }
        let Some(pkt) = ServerDataPacket::from_bytes(&head) else {
            continue;
        };
        let mut body = vec![0u8; usize::from(pkt.packet_len)];
        if rd.read_exact(&mut body).await.is_err() {
            glog!("ERR", "读取数据服务载荷失败。");
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
                glog!("INF", "数据服务要求关闭会话[{sid}]");
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
    w.write_all(&wire).await.context("写数据服务失败")?;
    w.flush().await.ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{peer_ip, sel_gate_enter_body};

    /// 判据 1：SelGate Enter 体逐字节 == `"%K{sid}/{ip}/{ip}$"`（含尾 `$`）。
    #[test]
    fn enter_body_matches_oracle_ipv4() {
        let body = sel_gate_enter_body("7", "10.0.0.5");
        assert_eq!(body, b"%K7/10.0.0.5/10.0.0.5$".to_vec());
    }

    /// 判据 2：体里没有端口，且 DBSrv 口径能切出 3 段（`%K{sid}` / ip / `{ip}$`）。
    #[test]
    fn enter_body_has_three_arrest_segments_and_no_port() {
        let peer = "10.1.2.3:53123".parse().expect("SocketAddr");
        let body = String::from_utf8(sel_gate_enter_body("7", &peer_ip(peer))).unwrap();
        assert_eq!(body, "%K7/10.1.2.3/10.1.2.3$");
        assert!(!body.contains(':'), "体不得带端口，实得 {body}");
        let inner = body
            .strip_prefix("%K")
            .and_then(|s| s.strip_suffix('$'))
            .expect("体必须由 %K 开头、$ 结尾");
        let parts: Vec<&str> = inner.split('/').collect();
        assert_eq!(parts.len(), 3, "DBSrv ArrestStringEx 口径应得到 3 段");
        assert_eq!(parts[0], "7");
        assert_eq!(parts[1], "10.1.2.3");
        assert_eq!(parts[2], "10.1.2.3");
    }

    /// 判据 3：`RemoteEndPoint.GetIP()` 口径 —— IPv6 带方括号。
    #[test]
    fn peer_ip_matches_touchsocket_get_ip() {
        assert_eq!(
            peer_ip("192.168.1.42:53123".parse().unwrap()),
            "192.168.1.42"
        );
        assert_eq!(peer_ip("[::1]:53123".parse().unwrap()), "[::1]");
        assert_eq!(peer_ip("[2001:db8::1]:1".parse().unwrap()), "[2001:db8::1]");
        assert_eq!(peer_ip("[fe80::1%12]:1".parse().unwrap()), "[fe80::1%12]");
    }

    /// 判据 4：`DataLen` 口径 == C# `(short)body.Length`。
    #[test]
    fn enter_data_len_uses_oracle_short_cast() {
        let body = sel_gate_enter_body("7", "10.1.2.3");
        assert_eq!(body.len(), 22);
        assert_eq!(i16::try_from(body.len()).unwrap(), 22);
    }
}
