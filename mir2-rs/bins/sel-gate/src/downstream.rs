//! SelGate 下行（DBSrv）链路：连接、10s 心跳、断链重连与失败计数日志。
//!
//! 1:1 对齐 C# oracle：
//! - `TimedService.CheckSessionStatus`（`src/SelGate/TimedService.cs:130-152`）：每
//!   `GateShare.CheckServerTimeOutTime`（10s）判一次；
//!   - `boGateReady` ⇒ `SendKeepAlive()` 且 `CheckServerFailCount = 0`；
//!   - 否则当 `now - GateShare.CheckServerTick > 10s`：`CheckServerFail` 为真 ⇒
//!     `Start()` 重连并打 `服务器[...]建立链接.失败次数:[N]`；为假 ⇒ 置真、`Stop()`、
//!     打 `服务器[...]链接超时.失败次数:[N]`（启动后第一次判定的形态）。
//! - `ClientThread`（`Services/ClientThread.cs`）：
//!   - 连接成功 ⇒ `boGateReady = true`、`CheckServerTick = now`、`RestSessionArray()`、
//!     打 `数据库服务器[...]链接成功.`；
//!   - 断开 ⇒ `RestSessionArray()`、`boGateReady = false`、`CheckServerFail = true`、
//!     打 `数据库服务器[...]断开链接.`；
//!   - `SendKeepAlive` 发一条 `ServerDataMessage{Type=KeepAlive}`（SocketId/Data 空）。
//! - `GateShare.CheckServerTick` 只在连接成功时更新；`CheckServerFailCount` 只在
//!   “就绪态 10s tick” 清零（`TimedService.cs:132-136`）。
//!
//! `RestSessionArray`（`Services/ClientThread.cs:231-242`）在 oracle 里**不清空槽位**，
//! 只把每个非空槽位的 `SocketId` 置空；我们的等价物是 [`crate::SessionEntry::idle_eligible`]：
//! 链路每次建立/断开都把**当前**会话标成“不再参与空闲超时”（对应
//! `TimedService.ProcessDelayMsg` 里 `GetSession("") == null` 的分支），但会话本身
//!（客户端连接与 `sessions` 表项）保留。
//!
//! 与 oracle 的两处工程实现差异（不可观测）：
//! 1. oracle 的 `Start()` 是 fire-and-forget，重叠 tick 可能再发一次 `ConnectAsync`；
//!    这里用 `connecting` 闸门避免连接风暴（每个 tick 的失败计数日志仍照打）。
//! 2. 连接尝试超时取 5s（TouchSocket `ConnectAsync` 的内建超时在仓库内没有配置值）。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use mir2_protocol::frame::ServerDataPacket;
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::slots::SlotTable;
use crate::{SessionEntry, Sessions};

/// C# `GateShare.cs:19`：`CheckServerTimeOutTime = 10 * 1000`。
pub const CHECK_INTERVAL: Duration = Duration::from_secs(10);
/// 连接尝试超时（见模块注释“差异 2”）。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// 一次 10s 判定（`TimedService.CheckSessionStatus`）的产物。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickAction {
    /// 时间未到（严格 `>` 未成立）。
    None,
    /// `boGateReady` ⇒ 发 KeepAlive，并把失败计数清零。
    KeepAlive,
    /// `CheckServerFail` 为真 ⇒ `Start()` 重连 + `建立链接.失败次数` 日志。
    Connect { fail_count: u32 },
    /// 首次判定 ⇒ `Stop()` + `链接超时.失败次数` 日志，并置 `CheckServerFail = true`。
    LogTimeout { fail_count: u32 },
}

/// `TimedService.CheckSessionStatus` 的纯状态机（时钟由调用方注入，便于构造级测试）。
#[derive(Debug, Clone, Copy)]
pub struct LinkState {
    ready: bool,
    check_server_fail: bool,
    fail_count: u32,
    /// C# `GateShare.CheckServerTick`：只在连接成功时更新；`None` 等价于静态初值 0。
    check_server_tick_ms: Option<u64>,
}

impl LinkState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ready: false,
            check_server_fail: false,
            fail_count: 0,
            check_server_tick_ms: None,
        }
    }

    #[must_use]
    pub const fn ready(&self) -> bool {
        self.ready
    }

    /// C# `ClientSocketConnect`：`boGateReady = true; GateShare.CheckServerTick = now;`
    /// （oracle **不**在这里清 `CheckServerFail`）。
    pub fn on_connected(&mut self, now_ms: u64) {
        self.ready = true;
        self.check_server_tick_ms = Some(now_ms);
    }

    /// C# `ClientSocketDisconnect`：`boGateReady = false; CheckServerFail = true;`
    pub fn on_disconnected(&mut self) {
        self.ready = false;
        self.check_server_fail = true;
    }

    /// C# 收到 `ServerDataType.KeepAlive`：`CheckServerFail = false; boGateReady = true;`
    pub fn on_keepalive(&mut self) {
        self.check_server_fail = false;
        self.ready = true;
    }

    /// 一次 10s tick。判据与 oracle 逐行一致（严格 `>`；首次判定按 `CheckServerTick == 0`
    /// 处理 ⇒ 必超时）。
    pub fn tick(&mut self, now_ms: u64) -> TickAction {
        if self.ready {
            self.fail_count = 0;
            return TickAction::KeepAlive;
        }
        let interval = u64::try_from(CHECK_INTERVAL.as_millis()).unwrap_or(u64::MAX);
        let timed_out = self
            .check_server_tick_ms
            .is_none_or(|tick| now_ms.saturating_sub(tick) > interval);
        if !timed_out {
            return TickAction::None;
        }
        self.fail_count += 1;
        if self.check_server_fail {
            TickAction::Connect {
                fail_count: self.fail_count,
            }
        } else {
            self.check_server_fail = true;
            TickAction::LogTimeout {
                fail_count: self.fail_count,
            }
        }
    }
}

impl Default for LinkState {
    fn default() -> Self {
        Self::new()
    }
}

/// `TimedService.cs:144` 原文：`服务器[{GetEndPoint()}]建立链接.失败次数:[{N}]`。
#[must_use]
pub fn connect_attempt_log(endpoint: &str, fail_count: u32) -> String {
    format!("服务器[{endpoint}]建立链接.失败次数:[{fail_count}]")
}

/// `TimedService.cs:150` 原文：`服务器[{GetEndPoint()}]链接超时.失败次数:[{N}]`。
#[must_use]
pub fn connect_timeout_log(endpoint: &str, fail_count: u32) -> String {
    format!("服务器[{endpoint}]链接超时.失败次数:[{fail_count}]")
}

/// `ClientThread.cs:119` 原文：`数据库服务器[{EndPoint}]链接成功.`。
#[must_use]
pub fn connected_log(endpoint: &str) -> String {
    format!("数据库服务器[{endpoint}]链接成功.")
}

/// `ClientThread.cs:128` 原文：`数据库服务器[{GetIPPort()}]断开链接.`。
#[must_use]
pub fn disconnected_log(endpoint: &str) -> String {
    format!("数据库服务器[{endpoint}]断开链接.")
}

/// 下行链路句柄：客户端会话只依赖它收发，不直接持有 socket。
#[derive(Clone)]
pub struct Downstream(Arc<Inner>);

struct Inner {
    addr: String,
    port: u16,
    /// `"{addr}:{port}"`（C# `IPHost.ToString()` / `GetEndPoint()`）。
    endpoint: String,
    /// C# `ClientThread.boGateReady` / `IsConnected`。
    ready: AtomicBool,
    /// 当前连接代次：旧连接的 EOF/写失败不得影响新连接。
    generation: AtomicU64,
    /// 当前连接的写队列；`None` ⇒ `SendSocket` 的 `Offline` 早退。
    tx: Mutex<Option<(u64, mpsc::UnboundedSender<ServerDataMessage>)>>,
    sessions: Sessions,
    /// C# `ClientThread.SessionArray`（`RestSessionArray` 的等价物由本模块驱动）。
    slots: Arc<Mutex<SlotTable>>,
}

impl Downstream {
    #[must_use]
    pub fn new(addr: String, port: u16, sessions: Sessions, slots: Arc<Mutex<SlotTable>>) -> Self {
        let endpoint = format!("{addr}:{port}");
        Self(Arc::new(Inner {
            addr,
            port,
            endpoint,
            ready: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            tx: Mutex::new(None),
            sessions,
            slots,
        }))
    }

    /// C# `ClientThread.boGateReady`。
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.0.ready.load(Ordering::SeqCst)
    }

    /// C# `ClientThread.GetEndPoint()`：`RemoteIPHost.ToString()`。
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.0.endpoint
    }

    /// C# `ClientThread.SendSocket`：`!_clientSocket.Online` ⇒ 静默丢弃；
    /// 写失败同样只丢这一帧，**不得**因此结束客户端会话（oracle 的 Send 不抛到会话层）。
    pub fn send(&self, msg: ServerDataMessage) -> bool {
        let Some((generation, tx)) = self.tx_slot().clone() else {
            return false;
        };
        if tx.send(msg).is_ok() {
            return true;
        }
        self.mark_dead(generation);
        false
    }

    fn addr(&self) -> (&str, u16) {
        (&self.0.addr, self.0.port)
    }

    fn next_generation(&self) -> u64 {
        self.0.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn current_generation(&self) -> u64 {
        self.0.generation.load(Ordering::SeqCst)
    }

    fn tx_slot(&self) -> MutexGuard<'_, Option<(u64, mpsc::UnboundedSender<ServerDataMessage>)>> {
        self.0
            .tx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn sessions_guard(&self) -> MutexGuard<'_, std::collections::HashMap<String, SessionEntry>> {
        self.0
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn slots_guard(&self) -> MutexGuard<'_, SlotTable> {
        self.0
            .slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// C# `RestSessionArray()`：保留槽位/会话，只让它们不再参与空闲超时。
    ///
    /// oracle（`ClientThread.cs:231-242`）同时把非空槽的 `SocketId`/`ClientIP` 置空
    /// （槽位**不**置 null）⇒ 这里同步调用 [`SlotTable::rest`]。
    fn detach_all_sessions(&self) {
        for entry in self.sessions_guard().values_mut() {
            entry.idle_eligible = false;
        }
        self.slots_guard().rest();
    }

    fn install(&self, generation: u64, tx: mpsc::UnboundedSender<ServerDataMessage>) {
        *self.tx_slot() = Some((generation, tx));
        self.0.ready.store(true, Ordering::SeqCst);
    }

    fn mark_dead(&self, generation: u64) {
        // 旧代次的 EOF/写失败落在新连接之后 ⇒ 不得把新连接标记为断开。
        if generation != self.current_generation() {
            return;
        }
        {
            let mut slot = self.tx_slot();
            if slot
                .as_ref()
                .is_some_and(|(current, _)| *current == generation)
            {
                *slot = None;
            }
        }
        self.0.ready.store(false, Ordering::SeqCst);
        // C# `ClientSocketDisconnect` 的 `RestSessionArray()`。
        self.detach_all_sessions();
    }
}

/// 后台连接事件（连接任务 / 读任务 / 写任务 → 监督循环）。
enum LinkEvent {
    Connected { generation: u64, stream: TcpStream },
    ConnectFailed,
    ConnectTimedOut,
    KeepAlive(u64),
    Dead(u64),
}

/// 下行监督循环：启动即连一次；之后每 10s 心跳或重连，日志串照 oracle。
pub async fn run(downstream: Downstream) {
    let started = Instant::now();
    let now_ms = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let (events, mut events_rx) = mpsc::unbounded_channel::<LinkEvent>();
    let mut state = LinkState::new();
    let mut connecting = false;

    // C# `ClientManager.Start()`：服务启动即发起第一次连接（失败不计数、不退出）。
    spawn_connect(&downstream, &events, &mut connecting);

    let mut ticker = tokio::time::interval(CHECK_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // `interval` 的首个 tick 立即就绪；跳过后第一次判定落在 10s。
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => match state.tick(now_ms()) {
                TickAction::KeepAlive => {
                    // C# `SendKeepAlive()`：`new ServerDataMessage { Type = KeepAlive }`。
                    downstream.send(ServerDataMessage::new(
                        ServerDataType::KeepAlive,
                        "",
                        Vec::new(),
                    ));
                }
                TickAction::Connect { fail_count } => {
                    // oracle 先 `Start()` 再打日志；连接结果由事件处理。
                    crate::glog!(
                        "DBG",
                        "{}",
                        connect_attempt_log(downstream.endpoint(), fail_count)
                    );
                    spawn_connect(&downstream, &events, &mut connecting);
                }
                TickAction::LogTimeout { fail_count } => {
                    // C# 此态下 `Stop()` 关的是已经断开的 socket ⇒ 无额外动作。
                    crate::glog!(
                        "DBG",
                        "{}",
                        connect_timeout_log(downstream.endpoint(), fail_count)
                    );
                }
                TickAction::None => {}
            },
            event = events_rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    LinkEvent::Connected { generation, stream } => {
                        connecting = false;
                        // C# `ClientSocketConnect`：`boGateReady = true; RestSessionArray(); ...`
                        downstream.detach_all_sessions();
                        state.on_connected(now_ms());
                        let (reader_half, writer_half) = stream.into_split();
                        let (tx, rx) = mpsc::unbounded_channel();
                        downstream.install(generation, tx);
                        crate::glog!("INF", "{}", connected_log(downstream.endpoint()));
                        tokio::spawn(writer(
                            rx,
                            writer_half,
                            downstream.clone(),
                            generation,
                            events.clone(),
                        ));
                        tokio::spawn(reader(
                            reader_half,
                            downstream.clone(),
                            generation,
                            events.clone(),
                        ));
                    }
                    LinkEvent::ConnectTimedOut => {
                        connecting = false;
                        // C# `ClientThread.Start` 的 `catch (TimeoutException)`。
                        crate::glog!(
                            "ERR",
                            "链接数据库服务器[{}]超时...",
                            downstream.endpoint()
                        );
                    }
                    LinkEvent::ConnectFailed => {
                        connecting = false;
                        // C# `ClientThread.Start` 的 `catch (Exception)`（日志原文无异常细节）。
                        crate::glog!(
                            "ERR",
                            "链接数据库服务器[{}]失败...",
                            downstream.endpoint()
                        );
                    }
                    LinkEvent::KeepAlive(generation) => {
                        // 写任务可能已先判死（Dead 已处理）；此时不得把状态机拨回就绪，
                        // 否则下一次 tick 会走 KeepAlive 分支而永远不重连。
                        if generation == downstream.current_generation() && downstream.is_ready()
                        {
                            state.on_keepalive();
                        }
                    }
                    LinkEvent::Dead(generation) => {
                        // 同一代次可能同时被读、写任务判死；只处理第一次。
                        if generation == downstream.current_generation() && state.ready() {
                            downstream.mark_dead(generation);
                            state.on_disconnected();
                            crate::glog!("INF", "{}", disconnected_log(downstream.endpoint()));
                        }
                    }
                }
            }
        }
    }
}

fn spawn_connect(
    downstream: &Downstream,
    events: &mpsc::UnboundedSender<LinkEvent>,
    connecting: &mut bool,
) {
    // C# `Start()`：`if (_clientSocket.Online) return;`。
    if downstream.is_ready() || *connecting {
        return;
    }
    *connecting = true;
    let downstream = downstream.clone();
    let events = events.clone();
    tokio::spawn(async move {
        let (addr, port) = downstream.addr();
        match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((addr, port))).await {
            Ok(Ok(stream)) => {
                stream.set_nodelay(true).ok();
                let generation = downstream.next_generation();
                let _ = events.send(LinkEvent::Connected { generation, stream });
            }
            Ok(Err(_)) => {
                let _ = events.send(LinkEvent::ConnectFailed);
            }
            Err(_) => {
                let _ = events.send(LinkEvent::ConnectTimedOut);
            }
        }
    });
}

/// 下行读循环：DBSrv → 按 `SocketId` 分派给客户端（`Data`），`Leave` 摘会话。
async fn reader(
    mut reader_half: OwnedReadHalf,
    downstream: Downstream,
    generation: u64,
    events: mpsc::UnboundedSender<LinkEvent>,
) {
    loop {
        let mut head = [0u8; ServerDataPacket::SIZE];
        if reader_half.read_exact(&mut head).await.is_err() {
            break;
        }
        let Some(packet) = ServerDataPacket::from_bytes(&head) else {
            continue;
        };
        let mut body = vec![0u8; usize::from(packet.packet_len)];
        if reader_half.read_exact(&mut body).await.is_err() {
            break;
        }
        let Ok(msg) = ServerDataMessage::from_bytes(&body) else {
            crate::glog!("WRN", "内部帧解析失败（长度 {}）", body.len());
            continue;
        };
        match msg.kind {
            ServerDataType::Data => {
                // oracle 的键是字符串 SocketId（`ConcurrentDictionary<string, ClientSession>`）。
                let sender = downstream
                    .sessions_guard()
                    .get(&msg.socket_id)
                    .map(|entry| entry.tx.clone());
                if let Some(tx) = sender {
                    let _ = tx.send(msg.data);
                } else {
                    crate::glog!(
                        "WRN",
                        "会话[{}]不存在，丢弃下行 {} 字节",
                        msg.socket_id,
                        msg.data.len()
                    );
                }
            }
            ServerDataType::Leave => {
                downstream.sessions_guard().remove(&msg.socket_id);
                crate::glog!("INF", "数据服务要求关闭会话[{}]", msg.socket_id);
            }
            // C# 收 KeepAlive：`CheckServerFail = false; boGateReady = true;`
            ServerDataType::KeepAlive => {
                let _ = events.send(LinkEvent::KeepAlive(generation));
            }
            // Enter 是网关→服务方向；服务回发时 oracle 无动作。
            ServerDataType::Enter => {}
        }
    }
    downstream.mark_dead(generation);
    let _ = events.send(LinkEvent::Dead(generation));
}

/// 下行写循环：一帧一个 `ServerDataMessage`，写失败即判死（在途帧随连接丢弃）。
async fn writer(
    mut rx: mpsc::UnboundedReceiver<ServerDataMessage>,
    mut writer_half: OwnedWriteHalf,
    downstream: Downstream,
    generation: u64,
    events: mpsc::UnboundedSender<LinkEvent>,
) {
    while let Some(msg) = rx.recv().await {
        let wire = msg.to_wire();
        if writer_half.write_all(&wire).await.is_err() {
            break;
        }
        writer_half.flush().await.ok();
    }
    downstream.mark_dead(generation);
    let _ = events.send(LinkEvent::Dead(generation));
}

#[cfg(test)]
mod tests {
    use super::{
        connect_attempt_log, connect_timeout_log, connected_log, disconnected_log, LinkState,
        TickAction, CHECK_INTERVAL,
    };
    use crate::{SessionEntry, Sessions};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// 判据 1（启动形态）：`CheckServerTick == 0` ⇒ 第一个 10s tick 必打
    /// `链接超时.失败次数:[1]`；之后每次 tick 都是 `建立链接.失败次数:[2,3,…]`。
    /// 牙：把失败计数改成每次重置、或把首次分支反过来 ⇒ 本用例红。
    #[test]
    fn startup_ticks_match_oracle_failure_count() {
        let mut state = LinkState::new();
        assert_eq!(state.tick(10_000), TickAction::LogTimeout { fail_count: 1 });
        assert_eq!(state.tick(20_000), TickAction::Connect { fail_count: 2 });
        assert_eq!(state.tick(30_000), TickAction::Connect { fail_count: 3 });
    }

    /// 判据 2（就绪态 + 计数清零）：就绪 tick 发 KeepAlive 并清零失败计数；
    /// 断链后未满 10s（相对连接时刻）不动作，超过后才 `Connect{fail_count:1}`。
    /// 牙：不在 KeepAlive 时清零 ⇒ 断链后的计数会是 3 而不是 1 ⇒ 本用例红。
    #[test]
    fn ready_ticks_send_keepalive_and_reset_count() {
        let mut state = LinkState::new();
        assert_eq!(state.tick(10_000), TickAction::LogTimeout { fail_count: 1 });
        assert_eq!(state.tick(20_000), TickAction::Connect { fail_count: 2 });
        // 第 2 次重连成功（t=20s）；t=30s 就绪 tick ⇒ KeepAlive + 计数清零。
        state.on_connected(20_000);
        assert_eq!(state.tick(30_000), TickAction::KeepAlive);
        assert_eq!(state.tick(40_000), TickAction::KeepAlive);
        state.on_disconnected();
        // 严格 `>`：30_000-20_000 == 10_000 不动作。
        assert_eq!(state.tick(30_000), TickAction::None);
        // 计数已清零 ⇒ 第一次重连是 1（不是 3）。
        assert_eq!(state.tick(40_000), TickAction::Connect { fail_count: 1 });
    }

    /// 判据 3（严格边界）：`elapsed == 10_000` 不动作，`10_001` 才动作。
    /// 牙：`>` 改 `>=` ⇒ 本用例红。
    #[test]
    fn timeout_is_strictly_greater_than_interval() {
        let mut state = LinkState::new();
        state.on_connected(0);
        state.on_disconnected();
        let interval = u64::try_from(CHECK_INTERVAL.as_millis()).unwrap();
        assert_eq!(state.tick(interval), TickAction::None);
        assert_eq!(
            state.tick(interval + 1),
            TickAction::Connect { fail_count: 1 }
        );
    }

    /// 判据 4（日志原文）：四个串逐字等于 oracle。
    #[test]
    fn log_strings_are_oracle_verbatim() {
        assert_eq!(
            connected_log("127.0.0.1:5100"),
            "数据库服务器[127.0.0.1:5100]链接成功."
        );
        assert_eq!(
            disconnected_log("127.0.0.1:5100"),
            "数据库服务器[127.0.0.1:5100]断开链接."
        );
        assert_eq!(
            connect_attempt_log("127.0.0.1:5100", 7),
            "服务器[127.0.0.1:5100]建立链接.失败次数:[7]"
        );
        assert_eq!(
            connect_timeout_log("127.0.0.1:5100", 1),
            "服务器[127.0.0.1:5100]链接超时.失败次数:[1]"
        );
    }

    /// 判据 5（RestSessionArray）：会话表**保留**，但全部变成不参与空闲超时。
    /// 牙：把实现改成 `sessions.clear()` ⇒ `len()` 变 0 ⇒ 本用例红。
    #[test]
    fn rest_session_array_keeps_sessions_but_detaches_idle() {
        let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
        let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();
        {
            let mut guard = sessions.lock().unwrap();
            guard.insert(
                "1".to_string(),
                SessionEntry {
                    tx: tx1,
                    idle_eligible: true,
                },
            );
            guard.insert(
                "2".to_string(),
                SessionEntry {
                    tx: tx2,
                    idle_eligible: true,
                },
            );
        }
        let slots = Arc::new(Mutex::new(crate::slots::SlotTable::new()));
        let downstream =
            super::Downstream::new("127.0.0.1".into(), 5100, Arc::clone(&sessions), slots);
        downstream.detach_all_sessions();
        let guard = sessions.lock().unwrap();
        assert_eq!(guard.len(), 2, "RestSessionArray 不得清空会话");
        assert!(guard.values().all(|entry| !entry.idle_eligible));
    }
}
