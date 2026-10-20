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
mod gbk;
mod idle_timeout;
mod slots;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use idle_timeout::{IdleTimer, TimeoutAction, CHECK_INTERVAL_MS};
use mir2_protocol::frame::{FrameSplitter, ServerDataPacket, SplitOut};
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use slots::SlotTable;
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
///
/// `u32`：oracle 的 `ConnectionId` 是 TouchSocket 的每 socket 唯一字符串；
/// Rust 侧用进程内单调 id 等价（客户端不可观测，写进内部帧的 `SocketId` 字符串）。
/// 用 32 位而不是 16 位，避免 65535 次接入后回绕、把新连接撞进**泄漏中的陈旧槽 id**
/// （审计 B-3 的登记项；u16 下这是 Rust-only 差异）。
type Sessions = Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>>;

/// 下行链路存活标志（C# `ClientThread.IsConnected` 的等价物）。
///
/// C# LoginGate 在 `ClientThread.ClientSocketDisconnect` 里把 `SessionArray[i]` 全部
/// 置 null（并 `RestSessionArray()` 清 `ConnectionId`）⇒ `TimedService.ProcessDelayMsg`
/// 取不到会话，超时动作（发帧 + 关会话）整体不再发生；`ClientSession.SendDefMessage`
/// 另有 `_lastLoginSvr == null || !_lastLoginSvr.IsConnected` 的早退。
/// `downstream_loop` 一退出（读到 EOF/错误）就把本标志置 false。
type DownstreamAlive = Arc<AtomicBool>;

/// `downstream_loop` 的析构守卫：任何退出路径（含 panic）都置「下行已断开」。
struct DownstreamAliveGuard(DownstreamAlive);

impl Drop for DownstreamAliveGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// 每个客户端会话要用的共享状态（打包后 `client_loop` 参数保持在 clippy 上限内）。
#[derive(Clone)]
struct GateShared {
    sessions: Sessions,
    downstream_alive: DownstreamAlive,
    /// C# `ClientThread.SessionArray`（单下游 ⇒ 单表）。
    slots: Arc<std::sync::Mutex<SlotTable>>,
}

/// C# `ServerService.ServerSocketClientConnect:65-94` 的三个接入分支。
enum Admission {
    /// `ClientManager.GetClientThread():148` 的 `SessionIsFull()` ⇒ null ⇒
    /// `ServerService.cs:65-70` 打 `获取登陆服务失败。` 后 return（不关 socket、不建会话）。
    Full,
    /// `ServerService.cs:72-77` 的 `!ConnectState` ⇒ `未就绪: {GetIPPort()}` 后 return
    /// （此时**还没有**分配槽位：oracle 的槽位循环在 ConnectState 判定之后）。
    NotReady,
    /// 正常接入：`ServerService.cs:82-94` 线性分配到的空槽下标。
    Slot(usize),
}

#[tokio::main]
#[allow(clippy::too_many_lines)] // 启动顺序照 `AppService.Start`：先连下游、再监听，错误分支都是 fail-loud
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
    let downstream_alive: DownstreamAlive = Arc::new(AtomicBool::new(true));
    let slots: Arc<std::sync::Mutex<SlotTable>> = Arc::new(std::sync::Mutex::new(SlotTable::new()));

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
    tokio::spawn(downstream_loop(
        down_rx,
        Arc::clone(&sessions),
        Arc::clone(&downstream_alive),
        Arc::clone(&slots),
    ));

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

    let next_sid = Arc::new(AtomicU32::new(1));
    let timeout_ms = cfg.client_timeout_ms;
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        let sid = next_sid.fetch_add(1, Ordering::Relaxed);
        // C# `ServerService.ServerSocketClientConnect:65-94` 的准入顺序：
        // ① `ClientManager.GetClientThread():148` → `SessionIsFull()`；满 ⇒ `获取登陆服务失败。`
        //    （null 分支，socket 留开）；② `!clientThread.ConnectState` ⇒ `未就绪`；
        // ③ 线性找空槽并登记会话。
        let admission = {
            let mut table = slots
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            admit(&mut table, sid, downstream_alive.load(Ordering::SeqCst))
        };
        let shared = GateShared {
            sessions: Arc::clone(&sessions),
            downstream_alive: Arc::clone(&downstream_alive),
            slots: Arc::clone(&slots),
        };
        let down_tx = Arc::clone(&down_tx);
        let ip = peer_ip(peer);
        tokio::spawn(async move {
            let result = match admission {
                Admission::Full => {
                    // LogService.Debug("获取登陆服务失败。")；随后 return。
                    glog!("DBG", "获取登陆服务失败。");
                    rejected_loop(sock, ip).await
                }
                Admission::NotReady => {
                    // LogService.Info("未就绪: " + sRemoteAddress)；随后 return。
                    glog!("INF", "未就绪: {peer}");
                    rejected_loop(sock, ip).await
                }
                Admission::Slot(slot_index) => {
                    glog!("INF", "开始连接: {peer}");
                    client_loop(
                        sock,
                        sid,
                        peer.to_string(),
                        ip,
                        timeout_ms,
                        slot_index,
                        shared,
                        down_tx,
                    )
                    .await
                }
            };
            if let Err(e) = result {
                glog!("WRN", "客户端[{peer}]会话结束: {e:#}");
            }
        });
    }
}

/// C# `ServerService.ServerSocketClientConnect:65-94` 的准入判定（纯函数，构造级可判）。
///
/// 顺序照 oracle：`ClientManager.GetClientThread():130-149` 的**判满最先行**（满 ⇒ null
/// ⇒ `获取登陆服务失败。`），随后才判 `ConnectState`（未就绪 ⇒ 不分配槽位）。
fn admit(table: &mut SlotTable, sid: u32, downstream_alive: bool) -> Admission {
    if table.is_full() {
        Admission::Full
    } else if !downstream_alive {
        Admission::NotReady
    } else {
        Admission::Slot(table.alloc(sid).expect("is_full() == false ⇒ 必有空槽"))
    }
}

/// C# 接入被拒（`Full` / `NotReady`）后的客户端形态：socket **保持打开**。
///
/// oracle 没有任何会话登记；后续每个接收块在 `ServerSocketClientRead` 入队，
/// `ServerManager.ProcessLoginMessage:76-83` 因 `GetSession == null` 打
/// `非法攻击: {ClientIP}`（Warn）后丢弃 ⇒ 不产生任何上行字节、也不发生空闲超时
/// （`TimedService` 只扫槽位，无槽位 ⇒ 不动作）。
async fn rejected_loop(mut sock: TcpStream, ip: String) -> Result<()> {
    let mut buf = vec![0u8; 8192];
    loop {
        match sock.read(&mut buf).await {
            Ok(0) => break,
            Ok(_) => glog!("WRN", "非法攻击: {ip}"),
            Err(e) => return Err(e).context("读客户端失败"),
        }
    }
    Ok(())
}

/// TouchSocket `ITcpClientBase.IP` / `SystemNetExtension.GetIP` 口径：只取对端 IP，
/// **不含**客户端临时端口；IPv6 带方括号（实测 `[::1]`，与 C# `client.IP` 一致）。
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

/// C# `src/LoginGate/Services/ClientSession.cs:204-212` 的 `UserEnter` 帧体：
/// `$"{ClientIP}/{ClientIP}"`（**无** `%K`）。
fn login_gate_enter_body(ip: &str) -> Vec<u8> {
    format!("{ip}/{ip}").into_bytes()
}

/// 单客户端会话：读客户端字节 → 拆帧 → 透传给 LoginSrv；并把该会话登记到会话表。
///
/// 空闲超时（审计 #12）照 C# `ClientSession.HandleDelayMsg` + `TimedService`：
/// - 计时起点 = 会话构造；
/// - **收到客户端数据不刷新**（oracle 的刷新语句全被注释，见 `idle_timeout` 模块文档）；
/// - 每 `CHECK_INTERVAL_MS` 扫描一次，严格 `> timeout` 才动作；
/// - 第一阶段发 `SM_OUTOFCONNECTION` 并在同一次扫描关会话（C# `success=true` 支），
///   第二阶段状态机保留但线上不可达；
/// - 超时关会话时**不发**下行 `Leave`（C# 先从 `_sessionMap` 摘除，`Disconnected`
///   处理器取不到会话，故 oracle 不发 `UserLeave`）；
/// - **下行断开守卫**：LoginSrv 链路已断时，`ClientThread.ClientSocketDisconnect` 把
///   `SessionArray` 槽位全置 null ⇒ `TimedService` 取不到会话 ⇒ 超时动作整段不发生
///   （不发帧、不关会话、计时也不动）。这里在 `timer.poll` **之前**判
///   [`DownstreamAlive`]，同形复刻。
#[allow(clippy::too_many_arguments)] // 与 C# `ServerService` 的接入/收包/超时三段一一对应
async fn client_loop(
    sock: TcpStream,
    sid: u32,
    peer: String,
    ip: String,
    timeout_ms: u64,
    slot_index: usize,
    shared: GateShared,
    down_tx: Arc<Mutex<OwnedWriteHalf>>,
) -> Result<()> {
    let GateShared {
        sessions,
        downstream_alive,
        slots,
    } = shared;
    let (mut rd, mut wr) = sock.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    // 超时帧与 LoginSrv 下行共用同一条回写通道，保证「先发帧、后断开」的字节次序。
    let timeout_tx = tx.clone();

    // 会话登记 + 通知 LoginSrv "新会话接入"（C# `UserEnter`：`{ClientIP}/{ClientIP}`，无 `%K`）
    sessions.lock().await.insert(sid, tx);
    let body = login_gate_enter_body(&ip);
    // C# `accountPacket.DataLen = (byte)body.Length`（unchecked 截断到 8 位，再隐式扩回 short）。
    #[allow(clippy::cast_possible_truncation)]
    let data_len = i16::from(body.len() as u8);
    let enter = ServerDataMessage {
        kind: ServerDataType::Enter,
        socket_id: sid.to_string(),
        data_len,
        data: body,
    };
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
                            // LoginGate：C# 的 `m_dwClientTimeOutTick = ...` 全被注释 ⇒
                            // 收到任何客户端数据都不刷新计时。
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
                // oracle 下行存活守卫：`ClientThread` 断开时槽位被置 null ⇒
                // `TimedService` 的 `GetSession` 返回 null ⇒ 整段跳过
                // （`HandleDelayMsg` 根本不被调用）。必须在 `timer.poll` 之前判，
                // 保证计时状态与 kick 标志都不推进。
                if !downstream_alive.load(Ordering::SeqCst) {
                    continue;
                }
                match timer.poll(now_ms(), timeout_ms) {
                    TimeoutAction::None => {}
                    TimeoutAction::SendOutOfConnection => {
                        // C# `TimedService`：`success=true` ⇒ 同一次扫描立即 CloseSession。
                        let _ = timeout_tx.send(idle_timeout::out_of_connection_frame());
                        glog!("DBG", "Client Connect TimeOut: {ip}");
                        timed_out = true;
                        // `TimedService.ProcessDelayMsg:79-80`：`SessionArray[j] = null`。
                        slots
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .clear(slot_index);
                        break;
                    }
                    // 线上不可达（第一阶段已关会话）；保留以对齐 oracle 状态机。
                    TimeoutAction::Close => {
                        timed_out = true;
                        slots
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .clear(slot_index);
                        break;
                    }
                }
            }
        }
    }

    // 会话离开（C# Leave 语义）。超时路径不发：oracle 的 TimedService 先摘会话再关
    // socket，`Disconnected` 处理器 `GetSession` 为 null ⇒ 不产生 `UserLeave`。
    //
    // 槽位**故意不清**：C# `ServerSocketClientDisconnect:113-127` 只 `CloseSession`，
    // 从不触碰 `SessionArray` ⇒ 正常断开泄漏一个槽（oracle 缺陷照抄；见 `slots` 模块）。
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

/// 下行读取：LoginSrv → 按 `SocketId` 分派给对应客户端（`Data`），`Leave` 关闭会话。
async fn downstream_loop(
    mut rd: OwnedReadHalf,
    sessions: Sessions,
    downstream_alive: DownstreamAlive,
    slots: Arc<std::sync::Mutex<SlotTable>>,
) -> Result<()> {
    let mark_dead = DownstreamAliveGuard(downstream_alive);
    let reason;
    loop {
        let mut head = [0u8; ServerDataPacket::SIZE];
        if rd.read_exact(&mut head).await.is_err() {
            reason = "登录服务连接断开，网关停止接收。";
            break;
        }
        let Some(pkt) = ServerDataPacket::from_bytes(&head) else {
            continue;
        };
        let mut body = vec![0u8; usize::from(pkt.packet_len)];
        if rd.read_exact(&mut body).await.is_err() {
            reason = "读取登录服务载荷失败。";
            break;
        }
        let Ok(msg) = ServerDataMessage::from_bytes(&body) else {
            glog!("WRN", "内部帧解析失败（长度 {}）", body.len());
            continue;
        };
        match msg.kind {
            ServerDataType::Data => {
                let Ok(sid) = msg.socket_id.parse::<u32>() else {
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
                let Ok(sid) = msg.socket_id.parse::<u32>() else {
                    continue;
                };
                sessions.lock().await.remove(&sid);
                glog!("INF", "登录服务要求关闭会话[{sid}]");
            }
            // KeepAlive 仅续命（C# 侧同样不转发）；Enter 是网关→服务方向，服务回发时无动作
            ServerDataType::KeepAlive | ServerDataType::Enter => {}
        }
    }
    // C# `ClientThread.ClientSocketDisconnect:118-130`：所有槽位置 null。
    // （LoginGate 不自动重连，链路断开是终态；此后 `TimedService` 再也取不到会话。）
    slots
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .reset();
    // 先让 `DownstreamAlive=false` 落定再打日志：日志出现即代表后续连接必走
    // `未就绪` 分支（进程级用例据此消除竞态）。
    drop(mark_dead);
    glog!("ERR", "{reason}");
    Ok(())
}

/// 发一帧内部消息：`[PacketCode u32][PacketLen u16]` + MemoryPack 体。
async fn send_internal(tx: &Arc<Mutex<OwnedWriteHalf>>, msg: &ServerDataMessage) -> Result<()> {
    let wire = msg.to_wire();
    let mut w = tx.lock().await;
    w.write_all(&wire).await.context("写登录服务失败")?;
    w.flush().await.ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex as StdMutex};
    use std::time::Duration;

    use tokio::io::AsyncReadExt;
    use tokio::net::tcp::OwnedWriteHalf;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::Mutex;

    use super::{
        admit, client_loop, downstream_loop, login_gate_enter_body, peer_ip, Admission,
        DownstreamAlive, GateShared, SlotTable,
    };

    /// 判据 1：LoginGate Enter 体逐字节 == `"{ip}/{ip}"`，且**无** `%K`。
    #[test]
    fn enter_body_matches_oracle_ipv4() {
        let body = login_gate_enter_body("192.168.1.42");
        assert_eq!(body, b"192.168.1.42/192.168.1.42".to_vec());
        assert!(
            !body.windows(2).any(|w| w == b"%K"),
            "LoginGate Enter 体不得含 %K（oracle 无该前缀）"
        );
    }

    /// 判据 2：体里没有端口（oracle `_session.ClientIP` 是 `client.IP`，不含 `:port`）。
    #[test]
    fn enter_body_has_no_transient_port() {
        let peer = "10.1.2.3:53123".parse().expect("SocketAddr");
        let body = String::from_utf8(login_gate_enter_body(&peer_ip(peer))).unwrap();
        assert_eq!(body, "10.1.2.3/10.1.2.3");
        assert!(!body.contains(':'), "体不得带端口，实得 {body}");
    }

    /// 判据 3：`client.IP` 口径 —— IPv4 无括号，IPv6 带方括号（TouchSocket 实测）。
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

    /// 判据 5（准入三分支，cap=4 构造级）：未就绪 ⇒ `NotReady` 且**不占槽**；
    /// 满 ⇒ `Full` 优先于 `NotReady`（oracle 的 `SessionIsFull` 在 `ConnectState` 之前）；
    /// 正常 ⇒ 按下标分配，第 cap 个之后满。
    /// 牙：把判满与就绪顺序对调、或 `NotReady` 也分配槽 ⇒ 本用例红。
    #[test]
    fn admission_matches_oracle_branch_order() {
        use super::{admit, Admission};
        use crate::slots::SlotTable;

        let mut table = SlotTable::with_capacity(4);
        // 未就绪：不分配槽位（oracle 的槽位循环在 ConnectState 判定之后）。
        assert!(matches!(admit(&mut table, 1, false), Admission::NotReady));
        assert_eq!(table.capacity(), 4);
        // 正常接入：下标 0..3。
        for sid in 1..=4 {
            match admit(&mut table, sid, true) {
                Admission::Slot(index) => {
                    assert_eq!(index, usize::try_from(sid).unwrap() - 1);
                }
                Admission::Full | Admission::NotReady => panic!("期望 Slot，实得拒接入"),
            }
        }
        assert!(table.is_full());
        // 满 + 未就绪：oracle 先判满 ⇒ Full（不是 NotReady）。
        assert!(matches!(admit(&mut table, 5, false), Admission::Full));
        assert!(matches!(admit(&mut table, 5, true), Admission::Full));
    }

    /// 判据 6（泄漏照抄）：cap=4 churn 5 次“连接→正常断开”后第 5 次仍被拒。
    /// 牙：在正常断开路径调用 `clear` ⇒ 第 5 次会拿到槽位 ⇒ 本用例红。
    #[test]
    fn churn_after_normal_disconnects_hits_cap() {
        use super::{admit, Admission};
        use crate::slots::SlotTable;

        let mut table = SlotTable::with_capacity(4);
        for round in 1..=4 {
            assert!(matches!(admit(&mut table, round, true), Admission::Slot(_)));
            // 客户端正常断开：oracle 只 CloseSession，不碰 SessionArray ⇒ 无 clear。
        }
        // 第 5 次接入：oracle `SessionIsFull()` ⇒ null ⇒ `获取登陆服务失败。`
        assert!(matches!(admit(&mut table, 5, true), Admission::Full));
    }

    /// 判据 4：`DataLen` 口径 == C# `(byte)body.Length`。
    #[test]
    fn enter_data_len_uses_oracle_byte_cast() {
        let body = login_gate_enter_body("10.1.2.3");
        assert_eq!(body.len(), 17);
        assert_eq!(i16::from(u8::try_from(body.len()).unwrap()), 17);
    }

    // ---- B-3 复验补牙：全部走**生产函数**（`client_loop` / `downstream_loop`），
    // ---- 不是只测 `SlotTable`。回环 socket 只替代真实监听器，断开/超时/下游 EOF
    // ---- 都由生产分支处理。

    /// 假下行：交出一条 `OwnedWriteHalf`（网关→LoginSrv 方向），对端持续排空。
    async fn dummy_downstream() -> (Arc<Mutex<OwnedWriteHalf>>, DownstreamAlive) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let gateway = TcpStream::connect(addr).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (mut server_rd, _server_wr) = server.into_split();
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            while matches!(server_rd.read(&mut buf).await, Ok(n) if n > 0) {}
        });
        let (_, wr) = gateway.into_split();
        (Arc::new(Mutex::new(wr)), Arc::new(AtomicBool::new(true)))
    }

    /// 一对回环 TCP：（客户端端，网关端）。丢弃客户端端 = 客户端正常断开（FIN）。
    async fn client_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        let (gateway, _) = listener.accept().await.unwrap();
        (client, gateway)
    }

    /// 判据 B-3-1（生产断开路径）：客户端**正常断开**时 `client_loop` 不得清槽。
    ///
    /// cap=2：A 占 0 号槽并正常断开（泄漏 1 槽）⇒ 新连接 B 只能拿 1 号槽，C 被拒。
    /// 若把「正常断开不清槽」改成断开即清，A 断开后 0 号槽空出，B/C 会分别拿到
    /// 0/1 号槽（C 不再 `Full`）⇒ 本用例红。
    #[tokio::test]
    async fn production_normal_disconnect_keeps_slot() {
        let (down_tx, alive) = dummy_downstream().await;
        let slots = Arc::new(StdMutex::new(SlotTable::with_capacity(2)));
        {
            let mut table = slots.lock().unwrap();
            assert!(matches!(admit(&mut table, 1, true), Admission::Slot(0)));
        }
        let shared = GateShared {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            downstream_alive: alive,
            slots: Arc::clone(&slots),
        };
        let (client, gateway) = client_pair().await;
        let task = tokio::spawn(client_loop(
            gateway,
            1,
            "127.0.0.1:40000".to_string(),
            "127.0.0.1".to_string(),
            60_000,
            0,
            shared,
            down_tx,
        ));
        // 客户端正常断开：oracle `ServerSocketClientDisconnect` 只 CloseSession。
        drop(client);
        let joined = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("正常断开后 client_loop 未退出")
            .expect("client_loop 任务 panic");
        assert!(joined.is_ok(), "正常断开不应报错：{joined:?}");

        let mut table = slots.lock().unwrap();
        assert_eq!(
            table.get(0),
            Some(1),
            "正常断开后槽位必须仍被占用（oracle 泄漏照抄）"
        );
        assert_eq!(table.occupied(), 1, "正常断开不得回收槽位");
        assert!(
            matches!(admit(&mut table, 2, true), Admission::Slot(1)),
            "泄漏 1 槽后仍应能接纳下一个连接（拿剩下的空槽）"
        );
        assert!(
            matches!(admit(&mut table, 3, true), Admission::Full),
            "若正常断开清了槽，第三个连接会被接纳（拿到 0 号槽）⇒ 本用例红"
        );
    }

    /// 判据 B-3-2a（生产超时路径）：超时**只清命中的槽**，不得清别的会话。
    ///
    /// cap=2：A（0 号槽，timeout=10ms）在首个 1s 扫描超时；B（1 号槽，timeout=60s）
    /// 仍在跑。A 的 `client_loop` 返回后 0 号槽必须为 null、1 号槽必须仍为 `Some(2)`。
    /// 若把 `clear(slot_index)` 改成 `reset()`（全清）⇒ 1 号槽也变 null ⇒ 本用例红。
    #[tokio::test]
    async fn production_timeout_clears_only_hit_slot() {
        let (down_tx, alive) = dummy_downstream().await;
        let slots = Arc::new(StdMutex::new(SlotTable::with_capacity(2)));
        {
            let mut table = slots.lock().unwrap();
            assert!(matches!(admit(&mut table, 1, true), Admission::Slot(0)));
            assert!(matches!(admit(&mut table, 2, true), Admission::Slot(1)));
        }
        let shared = GateShared {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            downstream_alive: alive,
            slots: Arc::clone(&slots),
        };
        let (client_a, gateway_a) = client_pair().await;
        let task_a = tokio::spawn(client_loop(
            gateway_a,
            1,
            "127.0.0.1:40001".to_string(),
            "127.0.0.1".to_string(),
            10,
            0,
            shared.clone(),
            Arc::clone(&down_tx),
        ));
        let (client_b, gateway_b) = client_pair().await;
        let task_b = tokio::spawn(client_loop(
            gateway_b,
            2,
            "127.0.0.1:40002".to_string(),
            "127.0.0.1".to_string(),
            60_000,
            1,
            shared,
            down_tx,
        ));

        // A 在第一个扫描 tick（CHECK_INTERVAL_MS=1000）超时并清自己的槽。
        let joined = tokio::time::timeout(Duration::from_secs(5), task_a)
            .await
            .expect("A 的超时路径未在 5s 内触发")
            .expect("client_loop A 任务 panic");
        assert!(joined.is_ok(), "超时路径不应报错：{joined:?}");

        {
            let table = slots.lock().unwrap();
            assert_eq!(table.get(0), None, "命中的槽位必须被清");
            assert_eq!(
                table.get(1),
                Some(2),
                "未命中的槽位不得被清（若改成 reset 全清 ⇒ 本用例红）"
            );
            assert_eq!(table.occupied(), 1);
        }

        // 收尾：B 正常断开（同样不清槽）。
        drop(client_a);
        drop(client_b);
        let joined_b = tokio::time::timeout(Duration::from_secs(5), task_b)
            .await
            .expect("B 未在 5s 内退出")
            .expect("client_loop B 任务 panic");
        assert!(joined_b.is_ok(), "B 正常断开不应报错：{joined_b:?}");
    }

    /// 判据 B-3-2b（生产下游断开路径）：LoginSrv 链路断开 ⇒ `downstream_loop` 清**全部**槽。
    ///
    /// 先把 cap=2 的两个槽占满；假 LoginSrv 关闭连接触发 `downstream_loop` 的 EOF 分支
    /// （`ClientThread.ClientSocketDisconnect:118-130`）。返回后 `occupied()==0`。
    /// 若把 `reset()` 改成只清一个槽 ⇒ `occupied()==1` ⇒ 本用例红。
    #[tokio::test]
    async fn production_downstream_disconnect_clears_all_slots() {
        let slots = Arc::new(StdMutex::new(SlotTable::with_capacity(2)));
        {
            let mut table = slots.lock().unwrap();
            assert!(matches!(admit(&mut table, 1, true), Admission::Slot(0)));
            assert!(matches!(admit(&mut table, 2, true), Admission::Slot(1)));
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let gateway = TcpStream::connect(addr).await.unwrap();
        let (srv, _) = listener.accept().await.unwrap();
        let (down_rd, down_wr) = gateway.into_split();
        drop(down_wr);
        let alive = Arc::new(AtomicBool::new(true));
        let sessions: super::Sessions = Arc::new(Mutex::new(HashMap::new()));
        let task = tokio::spawn(downstream_loop(
            down_rd,
            sessions,
            Arc::clone(&alive),
            Arc::clone(&slots),
        ));
        // 假 LoginSrv 断开：EOF ⇒ 生产 reset 分支。
        drop(srv);
        let joined = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("downstream_loop 未在 5s 内退出")
            .expect("downstream_loop 任务 panic");
        assert!(joined.is_ok(), "downstream_loop 返回 Err：{joined:?}");
        assert!(!alive.load(Ordering::SeqCst), "下行存活标志必须置 false");

        let table = slots.lock().unwrap();
        assert_eq!(
            table.occupied(),
            0,
            "下游断开必须清全部槽（若只清一个 ⇒ 本用例红）"
        );
        assert!(!table.is_full());
    }
}
