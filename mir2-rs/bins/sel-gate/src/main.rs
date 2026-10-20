//! Rust 版 **SelGate**（M1 第一步：顶住 7000，真实客户端能登录）。
//!
//! 1:1 对齐 C# `src/SelGate/**` 的**行为边界**：
//! - 客户端面：TCP 7000，帧形 `#1…!`（EDCode；客户端心跳是 1 字节 `*`）；
//! - 下行链路：连 DBSrv（默认 5500），用 6B `ServerDataPacket` 头 + MemoryPack
//!   `ServerDataMessage`（`crates/protocol::internal` 已字节级对齐 C#）；
//! - 网关**不重新编码客户端载荷**：`Data` 里放的就是客户端原始帧字节（C# 侧同样原样转发），
//!   所以客户端编码的细节在本进程里只是**不透明字节**——这也保证了 `!$` 帧尾等形态天然透传；
//! - 会话多路复用：一个下游连接承载全部客户端会话，靠 `SocketId` 区分（照 C# `ClientThread`）；
//! - `Leave`/`KeepAlive` 的处理与 C# 一致（Leave 关闭该客户端会话，KeepAlive 仅续命）；
//! - 下行链路：断链后每 10s 重连、就绪期每 10s 发 KeepAlive（照 C# `TimedService.CheckSessionStatus`），
//!   会话表跨断线**保留**（oracle 的 `RestSessionArray` 不清空槽位）；
//!
//! **失败要响**（M1 验收④）：缺配置键、绑不上端口 —— 都明确报错并以非零码退出。
//! 下行 DBSrv 连不上**不退出**：oracle `TimedService` 每 10s 重试并记失败次数（B-1 裁决）。

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

mod config;
mod downstream;
mod gbk;
mod idle_timeout;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use downstream::Downstream;
use idle_timeout::{IdleTimer, TimeoutAction, CHECK_INTERVAL_MS};
use mir2_protocol::frame::{decode_client_frame, FrameSplitter, SplitOut};
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

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

/// 会话表项：`SocketId → 发送端 + 是否参与空闲超时`。
pub(crate) struct SessionEntry {
    /// 该客户端连接的发送端（DBSrv 下行按 `SocketId` 分派到这里）。
    pub(crate) tx: mpsc::UnboundedSender<Vec<u8>>,
    /// C# `SessionInfo.SocketId` 非空 ⇔ true。链路每次建立/断开时
    /// `RestSessionArray()` 把 `SocketId` 置空 ⇒ 空闲超时扫描取不到会话；
    /// 会话本身**不**删除（见 `downstream.rs` 模块注释）。
    pub(crate) idle_eligible: bool,
}

/// 会话表：`SocketId → 会话项`（跨下行断线保留，照 oracle `SessionArray`）。
pub(crate) type Sessions = Arc<Mutex<HashMap<u16, SessionEntry>>>;

/// 每个客户端会话要用的共享状态（打包后 `client_loop` 参数保持在 clippy 上限内）。
#[derive(Clone)]
struct GateShared {
    sessions: Sessions,
    downstream: Downstream,
}

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
    let downstream = Downstream::new(cfg.srv_addr.clone(), cfg.srv_port, Arc::clone(&sessions));

    // oracle `AppService.Start()` 先起监听（`_serverService.Start()`）再连下游
    // （`_clientManager.Start()`）；DBSrv 不在**不退出**，由 10s 重连循环负责。
    let listener = TcpListener::bind((cfg.gate_addr.as_str(), cfg.gate_port))
        .await
        .with_context(|| {
            format!(
                "绑定 {}:{} 失败（端口被占？）",
                cfg.gate_addr, cfg.gate_port
            )
        })?;
    tokio::spawn(downstream::run(downstream.clone()));
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
        let shared = GateShared {
            sessions: Arc::clone(&sessions),
            downstream: downstream.clone(),
        };
        tokio::spawn(async move {
            if let Err(e) = client_loop(
                sock,
                sid,
                peer.to_string(),
                peer_ip(peer),
                timeout_ms,
                shared,
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
/// - 第一阶段发 `SM_OUTOFCONNECTION` 并在同一次扫描**只摘会话**；C# SelGate 的
///   `SessionManager.CloseSession` 只 `TryRemove`（与 LoginGate 相反，不调 `ClientSession.CloseSession`）
///   ⇒ **socket 保持打开**：后续客户端数据因 `GetSession == null` 被静默丢弃，直到客户端自己断开；
/// - 第二阶段状态机保留但线上不可达（会话已摘，`TimedService` 不会再取到它）；
/// - 超时路径**不发**下行 `Leave`（oracle 在 `Disconnected` 时 `GetSession` 为 null）；
/// - **下行断开守卫**：DBSrv 链路已断时，C# 的 `RestSessionArray` 让
///   `TimedService` 取不到会话 ⇒ 超时动作整段不发生（不发帧、不摘会话、计时也不动）。
///   这里在 `timer.poll` **之前**判下行就绪与会话的 `idle_eligible`，同形复刻。
#[allow(clippy::too_many_lines)] // 与 C# `ServerService` 的接入/收包/超时三段一一对应，拆开反而难对读
async fn client_loop(
    sock: TcpStream,
    sid: u16,
    peer: String,
    ip: String,
    timeout_ms: u64,
    shared: GateShared,
) -> Result<()> {
    let GateShared {
        sessions,
        downstream,
    } = shared;
    let (mut rd, mut wr) = sock.into_split();

    // C# `ClientManager.GetClientThread()`：下行断开时 `GateShare.ServerGateList` 已
    // `Remove(this)` ⇒ 返回 null ⇒ `ServerSocketClientConnect` 打「获取服务器实例失败。」
    // 并直接返回：**不建会话、不发 Enter**。之后每帧都按**当时的**链路状态重判
    //（oracle 的 `ServerSocketClientRead` 每次调用 `GetClientThread(id)`）：
    // - 链路仍断 ⇒ 兜底 `GetClientThread()` 为 null ⇒「非法攻击」并丢弃；
    // - 链路已恢复 ⇒ 兜底拿到活着的 `ClientThread`，但 `GetSession(id)` 为 null
    //   （oracle 从不回填会话）⇒ 静默丢弃，**无日志**。
    if !downstream.is_ready() {
        glog!("INF", "获取服务器实例失败。");
        let mut buf = vec![0u8; 8192];
        loop {
            match rd.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if downstream.is_ready() {
                        continue;
                    }
                    glog!("INF", "非法攻击: {ip}");
                }
            }
        }
        // 断开日志同理（oracle `ServerSocketClientDisconnect`）：链路已恢复时
        // `GetClientThread(id)` 兜底到活着的客户端线程、`boGateReady == true`，
        // 但 `GetSession(id) == null` ⇒ 不打 Info 日志；只有链路仍断才打「断开链接」。
        if !downstream.is_ready() {
            glog!("INF", "断开链接: {peer} (SessionId={sid})");
        }
        return Ok(());
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    // 超时帧与 DBSrv 下行共用同一条回写通道，保证超时帧先于任何后续写出。
    let timeout_tx = tx.clone();

    // 会话登记 + 通知 DBSrv "新会话接入"（C# `UserEnter`：`%K{SocketId}/{ClientIP}/{ClientIP}$`）
    sessions
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            sid,
            SessionEntry {
                tx,
                idle_eligible: true,
            },
        );
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
    // C# `UserEnter()` → `SendSocket()`：下行未就绪时静默丢弃（不结束客户端）。
    downstream.send(enter);

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
                if timed_out {
                    // C# SelGate：`SessionManager.CloseSession` 只摘会话、不关 socket；
                    // 之后 `ProcessReviceMessage` 的 `GetSession` 返回 null ⇒ 字节被静默丢弃，
                    // 连接保持到客户端自己断开（F1）。
                    continue;
                }
                for item in splitter.feed(&buf[..n]) {
                    match item {
                        // 心跳：C# 侧收到 1 字节包直接忽略；这里同样不转发
                        SplitOut::Heartbeat => {}
                        SplitOut::Frame(frame) => {
                            // C# `ServerServiceClientRead`：`!boGateReady` ⇒ 打「未就绪」并
                            // 丢弃整帧（`HandleUserPacket` 不会被调用 ⇒ 空闲计时也不刷新）。
                            if !downstream.is_ready() {
                                glog!("INF", "未就绪: {ip}");
                                continue;
                            }
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
                            // C# `SendSocket()`：写失败只丢帧，不结束会话。
                            downstream.send(msg);
                        }
                    }
                }
            }
            _ = sweep.tick() => {
                if timed_out {
                    // 会话已摘：C# `TimedService` 对 `GetSession == null` 的槽位直接 continue，
                    // 不再调用 `HandleDelayMsg`。
                    continue;
                }
                // oracle 下行存活守卫：`ClientThread` 断开时 `RestSessionArray` 清空槽位
                // `SocketId` ⇒ `GetSession == null` ⇒ 整段跳过（`HandleDelayMsg` 根本不被调用）。
                // 因此这里必须在 `timer.poll` 之前判：计时状态也不得推进（否则 kick 标志
                // 会先被置位，再在下一次扫描里产生动作）。
                if !downstream.is_ready() {
                    continue;
                }
                // `RestSessionArray` 之后该槽位 `SocketId` 为空：即便链路恢复也不参与超时
                //（oracle 从不回填 `SocketId`）。
                let idle_eligible = sessions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get(&sid)
                    .is_some_and(|entry| entry.idle_eligible);
                if !idle_eligible {
                    continue;
                }
                match timer.poll(now_ms(), timeout_ms) {
                    TimeoutAction::None => {}
                    TimeoutAction::SendOutOfConnection => {
                        // C# `TimedService`：`success=true` ⇒ 同一次扫描 `SessionManager.CloseSession`。
                        // SelGate 的 CloseSession 只摘会话（不关 socket）⇒ 这里同样只摘、不 break。
                        let _ = timeout_tx.send(idle_timeout::out_of_connection_frame());
                        glog!("INF", "Client Connect Time Out: {ip}");
                        sessions
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .remove(&sid);
                        timed_out = true;
                    }
                    // 线上不可达（第一阶段已摘会话）；保留以对齐 oracle 状态机。
                    TimeoutAction::Close => {
                        timed_out = true;
                        break;
                    }
                }
            }
        }
    }

    // 会话离开（C# Leave 语义）。超时路径不发：oracle 的 `TimedService` 先摘会话，
    // 客户端真正断开时 `Disconnected` 处理器 `GetSession` 为 null ⇒ 不产生 `UserLeave`。
    sessions
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&sid);
    if !timed_out {
        let leave = ServerDataMessage::new(ServerDataType::Leave, sid.to_string(), Vec::new());
        // C# `UserLeave()` → `SendSocket()`：下行断开时同样静默丢弃。
        downstream.send(leave);
    }
    // 等回写任务把队列（含超时帧）写完再 drop 写半端：C# 是 Send 后 Close。
    drop(timeout_tx);
    let _ = writer.await;
    glog!("INF", "断开链接: {peer} (SessionId={sid})");
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
