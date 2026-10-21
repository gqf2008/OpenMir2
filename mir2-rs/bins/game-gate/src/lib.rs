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
//! **速度/间隔规则已接线**（`rules.rs`，`ClientSession.cs:221-388`）：`CM_WALK`/`CM_RUN`、
//! 攻击族、`CM_SPELL` 的间隔 + 惩罚支；聊天间隔保持原实现不变。登录包按 oracle
//! `SendLoginMessage`（`:1252`）入**延时消息队列**（`DelayTime=1`），三条规则读
//! `GetDelayMsgCount()` —— 登录后 ~0–210ms 内队列非空，超速帧走「丢帧、不 kick」支。
//! 余量：`ItemSpeed`（下行 `SM_CHARSTATUSCHANGED`）与 `SpeedLimit`（登录 `PunishList`）
//! 尚未接线，见 `rules.rs` 模块文档。
//!
//! **已接线（B-5 / B-4，2026-10-20）**：`GM_CHECKCLIENT` 10s 健康帧、`GM_RECEIVE_OK` 回显、
//! `GM_STOP` 停止分配（`ClientThread.cs:431-459`、`:265-300`）；会话槽位按连接生命周期回收
//! （`ServerService.cs:131-209`），槽满时照 oracle 返回最后一个非空槽。
//!
//! **已接线（audit #18，2026-10-20）**：`ClientThread.CheckConnectedState` 的完整状态机
//! （`ClientThread.cs:431-474`：`GateReady` 每 10s 发 `GM_CHECKCLIENT` 并清失败计数；
//! `CheckServerTimeOut` 的 3min 严格 `>` 边界、失败标记/计数/断链/日志；`RunningState.Stop`
//! 的 10min 维护窗口）与 `TimedService.ClearIdleSession` + `ClientThread.ProcessIdleSession`
//! 的 15 天会话清理（硬编码 `GateShare.SessionTimeOutTime`）。
//!
//! **下行重连（2026-10-20，T-119）**：照 oracle `ClientThread.Start()`（`ClientThread.cs:431-460`）
//! 与 SelGate `downstream.rs` 的现成模式实现——运行期断链后按判定周期重连、失败计数、
//! `GateReady`/连接代次、`RestSessionArray()` 的会话摘除，以及 `Stop()` 的全关（读循环由关闭信号
//! 退出）。启动时连不上 GameSvr 仍按 M1④ 失败退出（重连只覆盖运行期断链）。
//!
//! **本切片不做**（登记在协议梳理 §6，切片 2/3）：`Filters/*`、
//! `Auth/DynamicAuthenticator`(OTP)、`ClientTimeOutTime` 空闲回收（死键）。
//!
//! **失败要响**：缺配置键、绑不上端口、连不上 GameSvr —— 明确报错并非零退出。

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

mod config;
mod gbk;
mod login;
mod rules;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::{Context, Result};
use mir2_protocol::frame::ServerMessage;
use mir2_protocol::messages::grobal2;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch, Mutex};

/// C# `GateShare.CommandFixedLength`（`GateShare.cs:18`）：命令帧最小长度。
const COMMAND_FIXED_LENGTH: usize = 16;
/// C# `Packets/ClientPackets/ClientMesaagePacket.cs:24`：`CommandMessage.Size`（解码后的固定头长度）。
const COMMAND_MESSAGE_SIZE: usize = 12;
/// C# `GateShare.MaxSession`（`GateShare.cs:14`）：每条网关→服务器连接的会话上限。
const MAX_SESSION: usize = 10_000;
/// 测试注入：会话槽位数（oracle 恒为 `GateShare.MaxSession`=10000，生产不设该变量）。
const MAX_SESSION_ENV: &str = "MIR2_GAME_GATE_MAX_SESSION";
/// C# `TimedService.KeepAlive`（`TimedService.cs:69-88`）：健康帧周期 10000ms。
const KEEPALIVE_MS: u64 = 10_000;
/// 测试注入：健康帧周期（生产不设该变量）。
const KEEPALIVE_ENV: &str = "MIR2_GAME_GATE_KEEPALIVE_MS";
/// `ClientThread.CheckConnectedState`（`ClientThread.cs:431-441`）：GM_STOP 后 60*10000ms 内不重连。
const MAINTENANCE_MS: i64 = 600_000;
/// C# `GateShare.CheckServerTimeOutTime`（`GateShare.cs:22`）：网关↔GameSvr 3min 无响应判定。
const CHECK_SERVER_TIMEOUT_MS: i64 = 3 * 60 * 1000;
/// 测试注入：3min 无响应阈值（生产不设该变量）。
const CHECK_SERVER_TIMEOUT_ENV: &str = "MIR2_GAME_GATE_CHECK_SERVER_TIMEOUT_MS";
/// C# `GateShare.SessionTimeOutTime`（`GateShare.cs:26`）：会话 15 天无上行即清理。
const SESSION_IDLE_MS: i64 = 15 * 24 * 60 * 60 * 1000;
/// 测试注入：会话空闲阈值（生产不设该变量）。
const SESSION_IDLE_ENV: &str = "MIR2_GAME_GATE_SESSION_IDLE_MS";
/// C# `TimedService.ClearIdleSession`（`TimedService.cs:151-173`）：每 120000ms 扫一次。
const SESSION_SWEEP_MS: u64 = 120_000;
/// 测试注入：空闲会话扫描周期（生产不设该变量）。
const SESSION_SWEEP_ENV: &str = "MIR2_GAME_GATE_SESSION_SWEEP_MS";
/// C# `TimedService` 的 `PeriodicTimer(TimeSpan.FromMilliseconds(10))`（`TimedService.cs:30`）。
const TIMED_POLL_MS: u64 = 10;
/// C# `ushort.MaxValue`：`CheckServerFailCount` 的比较上界（`ClientThread.cs:453/467`）。
const U16_MAX: u32 = 65_535;
/// 单次重连尝试超时（oracle 的 `ConnectAsync` 内建超时在仓库里没有配置值；同 SelGate `downstream.rs`）。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
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

/// 空闲清理登记项（oracle `SessionInfo` 的 `Socket`/`ReceiveTick` 在 `SessionArray` 里的那一格）：
/// 最近一次上行时刻 + 空闲关闭信号。关掉读循环即等价于 `Socket.Close()`（`ClientThread.cs:417-424`）。
///
/// `idle_eligible` 等价于 oracle `ProcessIdleSession` 的 `userSession.Socket != null` 门：
/// `RestSessionArray()`（`ClientThread.cs:332-346`）在链路建立/断开时把每个非空槽的 `Socket`
/// 置 null ⇒ 这些会话**不再参与** 15 天空闲清理（但会话本身保留）。
struct IdleEntry {
    last_recv_ms: Arc<AtomicI64>,
    /// oracle：`SessionInfo.Socket != null` 才进入 `ProcessIdleSession` 的超时判定。
    idle_eligible: bool,
    close: watch::Sender<bool>,
}

/// `ClientThread.SessionArray` 里的非空槽（只登记**占槽**的会话，槽满复用分支不入表，
/// 与 oracle 的 `SessionArray` 口径一致）。
type IdleSessions = Arc<Mutex<HashMap<u16, IdleEntry>>>;

/// 会话在**引擎侧的索引**（`ClientSession.cs:649` 的 `SendMsg.SessionIndex = SvrListIdx`）：
/// 由服务端用 `GM_SERVERUSERINDEX` 下发（`ClientThread.cs:272-275` 存进会话的 `SvrListIdx`），
/// 之后该会话发往 GameSvr 的每一帧都要带上它（未收到时为 0）。
type SessionIndexes = Arc<Mutex<HashMap<u16, i32>>>;

/// 一次出站写（已经拼好的 20B 头 + body）。写任务按到达顺序原样写线上字节。
struct Outbound {
    wire: Vec<u8>,
    /// `Some` ⇒ 调用方在等「已 `write_all` **且** `flush`」的回执（[`Downstream::send_frame_sync`]）：
    /// 离开路径的控制帧必须确认已进入内核发送缓冲，语义对齐 oracle `ClientThread.Send` 的同步
    /// `Socket.Send`（`ClientThread.cs:393-399`）。普通 `GM_DATA` 走 `None`，保持异步入队。
    ack: Option<oneshot::Sender<bool>>,
}

/// 网关→GameSvr 链路句柄：连接由 [`link_loop`] 建立/重建，会话与健康帧只经它发送。
///
/// 与 oracle 的对应关系：
/// - [`Downstream::send_frame`] = `ClientThread.Send`（`ClientThread.cs:391-398`）：
///   `!_clientSocket.Online` 或写端已死时**静默丢弃**这一帧（oracle 的 `Send` 不向会话层抛）；
/// - `generation` 是连接代次：旧连接的 EOF/写失败不得误伤新连接（同 SelGate `downstream.rs`）；
/// - [`Downstream::stop`] = `ClientThread.Stop()`（`:431-434`）：`_clientSocket.Close()` 是**全关**，
///   这里同时给读循环一个关闭信号并摘掉写端 ⇒ 读/写任务都退出、socket 全关。
#[derive(Clone)]
struct Downstream(Arc<DownstreamInner>);

struct DownstreamInner {
    addr: String,
    port: u16,
    /// C# `ClientThread.EndPoint` = `"{ServerAdress}:{ServerPort}"`。
    endpoint: String,
    /// C# `ClientThread.IsConnected` / `_clientSocket.Online`。
    ready: AtomicBool,
    /// 当前连接代次：旧连接的 EOF/写失败不得影响新连接。
    generation: AtomicU64,
    /// 当前连接的写队列；`None` ⇒ `Send` 的 `Offline` 早退。
    tx: StdMutex<Option<(u64, mpsc::UnboundedSender<Outbound>)>>,
    /// 当前连接的“全关”信号（`Stop()` 用）；`None` ⇒ 无活连接。
    close: StdMutex<Option<(u64, watch::Sender<bool>)>>,
}

/// 拼一帧内部消息的线上字节：20B 小端头 + body。
fn server_message_wire(msg: ServerMessage, body: &[u8]) -> Vec<u8> {
    let mut wire = msg.to_bytes().to_vec();
    wire.extend_from_slice(body);
    wire
}

impl Downstream {
    fn new(addr: String, port: u16) -> Self {
        let endpoint = format!("{addr}:{port}");
        Self(Arc::new(DownstreamInner {
            addr,
            port,
            endpoint,
            ready: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            tx: StdMutex::new(None),
            close: StdMutex::new(None),
        }))
    }

    /// C# `ClientThread.IsConnected`。
    fn is_ready(&self) -> bool {
        self.0.ready.load(Ordering::SeqCst)
    }

    /// C# `ClientThread.EndPoint`。
    fn endpoint(&self) -> &str {
        &self.0.endpoint
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

    fn tx_slot(&self) -> std::sync::MutexGuard<'_, Option<(u64, mpsc::UnboundedSender<Outbound>)>> {
        self.0
            .tx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn close_slot(&self) -> std::sync::MutexGuard<'_, Option<(u64, watch::Sender<bool>)>> {
        self.0
            .close
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// C# `ClientThread.Send`：离线/写端已死 ⇒ 静默丢弃并返回 `false`。
    fn send_frame(&self, wire: Vec<u8>) -> bool {
        let Some((generation, tx)) = self.tx_slot().clone() else {
            return false;
        };
        if tx.send(Outbound { wire, ack: None }).is_ok() {
            return true;
        }
        self.mark_dead(generation);
        false
    }

    /// **同步**写一帧：入队后等写任务回执「已 `write_all` + `flush`」。
    ///
    /// oracle 的 `ClientThread.Send` 是同步 `_clientSocket.Send(sendBuffer)`（`ClientThread.cs:393-399`）：
    /// 调用返回时字节已在**内核发送缓冲**里，之后进程即使立刻退出也不会丢帧。Rust 侧普通发送是
    /// 「入队即返回、写任务稍后落盘」，客户端断开后的 `GM_CLOSE` 因此可能在进程收工前还躺在队列里
    /// —— 2026-10-20 全量并行门禁的 `header_prefixed_login_is_kicked` 偶发失败就是这条路径。
    ///
    /// 到达顺序不变：先入队，写任务按 FIFO 写完它前面的帧后才回执。链路已死 ⇒ 返回 `false`
    /// （oracle 的 `!_clientSocket.Online` 静默丢弃）；写失败/写任务被 `Stop` 打断 ⇒ `ack` 发送端
    /// 被丢弃，同样返回 `false`，不会挂住会话任务。
    async fn send_frame_sync(&self, wire: Vec<u8>) -> bool {
        // 标准库互斥量守卫不能跨 await：先取出「代次 + 发送端」再释放。
        let Some((generation, tx)) = self.tx_slot().clone() else {
            return false;
        };
        let (ack_tx, ack_rx) = oneshot::channel();
        if tx
            .send(Outbound {
                wire,
                ack: Some(ack_tx),
            })
            .is_err()
        {
            self.mark_dead(generation);
            return false;
        }
        ack_rx.await.unwrap_or(false)
    }

    /// 发一帧内部消息：20B 小端头 + body。
    fn send_internal(&self, msg: ServerMessage, body: &[u8]) -> bool {
        self.send_frame(server_message_wire(msg, body))
    }

    /// 同 [`Self::send_internal`]，但等写任务确认已写入并 `flush`（控制帧用，见 `send_frame_sync`）。
    async fn send_internal_sync(&self, msg: ServerMessage, body: &[u8]) -> bool {
        self.send_frame_sync(server_message_wire(msg, body)).await
    }

    /// 发一段**已经拼好**的线上字节（命令路径自己拼 20B 头 + 解码头 + 编码尾，见 §7.9）。
    fn send_raw(&self, wire: &[u8]) -> bool {
        self.send_frame(wire.to_vec())
    }

    fn install(
        &self,
        generation: u64,
        tx: mpsc::UnboundedSender<Outbound>,
        close: watch::Sender<bool>,
    ) {
        *self.tx_slot() = Some((generation, tx));
        *self.close_slot() = Some((generation, close));
        self.0.ready.store(true, Ordering::SeqCst);
    }

    /// 断开判死：只认**当前代次**；清发送端（写任务随之退出）与关闭信号。
    fn mark_dead(&self, generation: u64) {
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
        {
            let mut slot = self.close_slot();
            if slot
                .as_ref()
                .is_some_and(|(current, _)| *current == generation)
            {
                *slot = None;
            }
        }
        self.0.ready.store(false, Ordering::SeqCst);
    }

    /// C# `ClientThread.Stop()`：`_clientSocket.Close()` 全关。给读循环一个关闭信号，
    /// 同时摘掉写端发送端 ⇒ 读/写任务都退出（不是只半关写半端）。
    fn stop(&self) {
        let close = {
            let mut slot = self.close_slot();
            slot.take()
        };
        if let Some((_, close)) = close {
            let _ = close.send(true);
        }
        let _ = self.tx_slot().take();
    }
}

/// 后台连接事件（连接任务 / 读任务 / 写任务 → 监督循环）。
#[derive(Debug)]
enum LinkEvent {
    Connected { generation: u64, stream: TcpStream },
    ConnectRefused,
    ConnectTimedOut,
    ConnectFailed,
    Dead(u64),
}

/// 一条端点线的链路监督上下文（连接/重连 + 会话摘除 + 状态机）。
#[derive(Clone)]
struct LinkCtx {
    downstream: Downstream,
    sessions: Sessions,
    indexes: SessionIndexes,
    idle: IdleSessions,
    state: Arc<StdMutex<GateState>>,
    /// C# `ClientThread.LocalEndPoint` = `IPEndPoint(ServerAdress, GatePort)`（日志用）。
    local_endpoint: String,
    srv_addr: String,
    srv_port: u16,
    /// `TimedService.KeepAlive` 的判定周期（生产 10s；测试注入）。
    period: Duration,
    /// `GateShare.CheckServerTimeOutTime`（生产 3min；测试注入）。
    timeout_ms: i64,
}

/// `ClientThread.cs:158`：`游戏网关[{LocalEndPoint}] 游戏引擎[{EndPoint}]链接成功.`
fn connected_log(local: &str, endpoint: &str) -> String {
    format!("游戏网关[{local}] 游戏引擎[{endpoint}]链接成功.")
}

/// `ClientThread.cs:219`：`游戏网关[{LocalEndPoint}] 游戏引擎[{GetEndPoint()}]断开链接.`
fn disconnected_log(local: &str, endpoint: &str) -> String {
    format!("游戏网关[{local}] 游戏引擎[{endpoint}]断开链接.")
}

/// C# `RestSessionArray()`（`ClientThread.cs:332-346`）：会话**保留**，但每个非空槽的
/// `Socket` 置 null ⇒ `ProcessIdleSession`（`:410-427`）的 `Socket != null` 门为假，
/// 这些会话**不再参与** 15 天空闲清理。等价物 = 把所有登记项标成 `idle_eligible = false`。
async fn detach_idle_sessions(idle: &IdleSessions) {
    for entry in idle.lock().await.values_mut() {
        entry.idle_eligible = false;
    }
}

/// `ClientThread.RunningState`（`GateShare.cs:104-114`）：`GM_STOP` 后不再向该引擎分配新用户。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunningState {
    Waiting,
    Running,
    Stop,
}

/// 网关→GameSvr 链路状态（`ClientThread` 的 `GateReady` / `CheckServerFail` /
/// `CheckServerFailCount` / `CheckServerTick` / `RunningState`）。
struct GateState {
    running: RunningState,
    /// `GateReady`：连接建立后才发 `GM_CHECKCLIENT`（`ClientThread.cs:431-459`）。
    gate_ready: bool,
    /// `CheckServerFail`：连接失败/断开后置真，`GM_CHECKSERVER` 清假（`ClientThread.cs:155-191/268-271`）。
    check_server_fail: bool,
    /// `CheckServerFailCount`（oracle 是 `int`；比较上界为 `ushort.MaxValue`）。
    check_server_fail_count: u32,
    /// `CheckServerTick`：最近一次连接成功/收到 `GM_CHECKSERVER` 的时刻。
    last_server_tick: i64,
}

/// 一次 10s tick 的处置（`ClientThread.CheckConnectedState` 的状态判定与 I/O 分开，
/// 便于用注入时钟逐条钉住边界）。
#[derive(Debug, PartialEq, Eq)]
enum KeepaliveTick {
    /// `GateReady` ⇒ 发 `GM_CHECKCLIENT`，并把 `CheckServerFailCount` 清零。
    Send,
    /// GM_STOP 后的 10min 维护窗口已过（oracle 在此调用 `Start()`；本网关无下行重连，只照抄日志）。
    Maintenance,
    /// `CheckServerFail` ⇒ `Start()` 重连尝试（本线只照抄失败日志与计数，见 T-119）。
    RetryConnect { fail_count: u32 },
    /// 失败计数达到 `ushort.MaxValue` 之后（`ClientThread.cs:455-459`）。
    MaxRetries,
    /// `CheckServerTimeOut()` 首判 ⇒ 置 `CheckServerFail`、`Stop()`、计数、日志（`:463-474`）。
    TimeoutStop { fail_count: u32 },
    /// 不动作。
    Idle,
}

/// 一个被占用的会话槽：oracle `SessionInfo` 在本片用到的两个字段
/// （`SckHandle` 与所属会话；`SessionArray` 见 `ServerService.cs:131-147`）。
struct SlotOwner {
    sid: u16,
    handle: i32,
}

/// C# `ClientThread.SessionArray`（`new SessionInfo[GateShare.MaxSession]`）：
/// 槽位在**客户端断开**时回收（`ServerService.cs:166-209`）。
/// 槽满时 oracle **不拒绝新连接**：扫描循环结束后 `userSession` 停在最后一个非空槽，
/// 新会话拿到的是那个陈旧槽的 `SckHandle`（`allocate` 的返回值复刻该语义）。
struct SessionSlots {
    slots: Vec<Option<SlotOwner>>,
}

impl SessionSlots {
    fn new(capacity: usize) -> Self {
        Self {
            slots: (0..capacity).map(|_| None).collect(),
        }
    }

    /// 分配：首个空槽；全满则照 oracle 返回**最后一个非空槽**的 `SckHandle`
    /// 与宿主会话 id（新会话不入槽）。
    fn allocate(&mut self, sid: u16, handle: i32) -> (i32, Option<u16>) {
        if let Some(idx) = self.slots.iter().position(Option::is_none) {
            self.slots[idx] = Some(SlotOwner { sid, handle });
            return (handle, None);
        }
        match self.slots.last().and_then(Option::as_ref) {
            Some(owner) => (owner.handle, Some(owner.sid)),
            // 容量为 0（仅测试注入；oracle 的 10000 不会走这里）。
            None => (handle, None),
        }
    }

    /// 断开回收：只清**属于该会话**的槽（oracle 按 `ConnectionId` 匹配，`ServerService.cs:166-201`；
    /// 槽满分支的新会话从未入槽，因此不得误清宿主槽）。
    fn release(&mut self, sid: u16) {
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|s| matches!(s, Some(o) if o.sid == sid))
        {
            *slot = None;
        }
    }

    #[cfg(test)]
    fn active(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}

/// RAII：`client_loop` 的任何退出路径（含 `?` 早退）都回收槽位（B-4）。
struct SlotGuard {
    slots: Arc<StdMutex<SessionSlots>>,
    sid: u16,
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if let Ok(mut slots) = self.slots.lock() {
            slots.release(self.sid);
        }
    }
}

#[allow(clippy::too_many_lines)] // 端点循环：连接/状态/槽表/两个任务；拆函数只会多传一坨 Arc
async fn run_inner(args: &[String]) -> Result<()> {
    let conf = args
        .first()
        .map_or_else(config::default_path, std::path::PathBuf::from);
    let cfg = config::load(&conf)?;
    let gate_rules = rules::GateRules {
        chat_interval_enabled: cfg.chat_interval_enabled,
        chat_interval_ms: cfg.chat_interval_ms,
        speed: cfg.speed.clone(),
    };
    // 测试注入（生产环境不设这两个变量 ⇒ oracle 口径：10000 槽 / 10s 心跳）。
    let max_session = std::env::var(MAX_SESSION_ENV)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(MAX_SESSION);
    let keepalive_ms = std::env::var(KEEPALIVE_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(KEEPALIVE_MS);
    let check_server_timeout_ms = std::env::var(CHECK_SERVER_TIMEOUT_ENV)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(CHECK_SERVER_TIMEOUT_MS);
    let session_idle_ms = std::env::var(SESSION_IDLE_ENV)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(SESSION_IDLE_MS);
    let session_sweep_ms = std::env::var(SESSION_SWEEP_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(SESSION_SWEEP_MS);
    if max_session != MAX_SESSION
        || keepalive_ms != KEEPALIVE_MS
        || check_server_timeout_ms != CHECK_SERVER_TIMEOUT_MS
        || session_idle_ms != SESSION_IDLE_MS
        || session_sweep_ms != SESSION_SWEEP_MS
    {
        glog!(
            "INF",
            "测试注入：MaxSession={max_session} KeepAlive={keepalive_ms}ms CheckServerTimeOut={check_server_timeout_ms}ms SessionIdle={session_idle_ms}ms Sweep={session_sweep_ms}ms"
        );
    }
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
        // 会话槽表（`ClientThread.SessionArray`）与本线引擎状态（`GateReady`/`RunningState`）。
        let slots = Arc::new(StdMutex::new(SessionSlots::new(max_session)));
        // 空闲清理登记（`SessionArray` 的非空槽 + `ReceiveTick`）。
        let idle: IdleSessions = Arc::new(Mutex::new(HashMap::new()));
        let state = Arc::new(StdMutex::new(GateState {
            running: RunningState::Waiting,
            gate_ready: false,
            check_server_fail: false,
            check_server_fail_count: 0,
            last_server_tick: now_ms(),
        }));
        // 下行链路：启动时连不上 GameSvr 即失败退出（M1④）；运行期断链由 `link_loop` 重连。
        let downstream = Downstream::new(ep.srv_addr.clone(), ep.srv_port);
        let initial = TcpStream::connect((ep.srv_addr.as_str(), ep.srv_port))
            .await
            .with_context(|| {
                format!(
                    "连接游戏服务 {}:{} 失败（M1④：缺服务要明确报错退出）",
                    ep.srv_addr, ep.srv_port
                )
            })?;
        initial.set_nodelay(true).ok();
        let link_ctx = LinkCtx {
            downstream: downstream.clone(),
            sessions: Arc::clone(&sessions),
            indexes: Arc::clone(&indexes),
            idle: Arc::clone(&idle),
            state: Arc::clone(&state),
            // C# `ClientThread.LocalEndPoint = IPEndPoint(ServerAdress, GatePort)`（日志用）。
            local_endpoint: format!("{}:{}", ep.gate_addr, ep.gate_port),
            srv_addr: ep.srv_addr.clone(),
            srv_port: ep.srv_port,
            period: Duration::from_millis(keepalive_ms),
            timeout_ms: check_server_timeout_ms,
        };
        // `ClientThread.ClientSocketConnect`：GateReady/Connected/Runing + RestSessionArray + 日志。
        // 首连在绑定监听**之前**登记，避免 accept 抢在 GateReady 之前（M1④ 的启动门）。
        let (link_events, link_events_rx) = mpsc::unbounded_channel();
        let generation = downstream.next_generation();
        on_connected(&link_ctx, generation, initial, &link_events).await;
        // `TimedService.KeepAlive`（判定周期 + `CheckConnectedState`）与断链重连在本任务里跑。
        tasks.push(tokio::spawn(async move {
            link_loop(link_ctx, link_events, link_events_rx).await;
            Ok::<(), anyhow::Error>(())
        }));
        // `TimedService.ClearIdleSession`（120s 扫描 / 15 天阈值）在本线独立任务里跑。
        let sweep_idle = Arc::clone(&idle);
        tasks.push(tokio::spawn(async move {
            session_sweep_loop(
                sweep_idle,
                Duration::from_millis(session_sweep_ms),
                session_idle_ms,
            )
            .await;
            Ok::<(), anyhow::Error>(())
        }));

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
            idle,
            downstream,
            gate_rules: gate_rules.clone(),
            nom_client_packet_size: cfg.nom_client_packet_size,
        };
        tasks.push(tokio::spawn(accept_loop(listener, gate_ctx, slots, state)));
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
    /// 空闲清理登记（`SessionArray` 的非空槽）。
    idle: IdleSessions,
    /// 本线到 GameSvr 的链路句柄（断链期间发送静默丢弃，照 `ClientThread.Send`）。
    downstream: Downstream,
    /// 本线生效的网关规则（速度/间隔）。
    gate_rules: rules::GateRules,
    /// `[Integer] NomClientPacketSize`（登录后超限帧只丢不关；照 oracle 带默认读取）。
    nom_client_packet_size: usize,
}

/// 一帧的处置结果（读循环据此决定下一步）。
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// 处理完毕（含已转发），继续读下一帧。
    Next,
    /// 登录帧通过——已按 `ClientSession.cs:1199-1224` 补发合成登录包。
    LoggedIn,
    /// 按 oracle 剔除/关闭该会话（附日志原因）。
    Close(String),
    /// 静默丢弃（不转发）：聊天间隔未到（`ClientSession.cs:492-495`）或 move/attack/spell
    /// 速度规则判丢（`ClientSession.cs:243-262` 等分支的 `return`）。
    Drop,
}

/// 客户端 face：接受连接并为每个会话分配 `SessionId`。
#[allow(clippy::too_many_lines)] // 与 oracle 的 Connect 回调一一对应（停服分支/槽满分支/登记）
async fn accept_loop(
    listener: TcpListener,
    ctx: GateCtx,
    slots: Arc<StdMutex<SessionSlots>>,
    state: Arc<StdMutex<GateState>>,
) -> Result<()> {
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        // `ServerService.cs:123-129`：GM_STOP 后 `ServerManager.GetClientThread` 找不到
        // `RunningState.Runing` 的实例 ⇒ 不建会话、不发 `GM_OPEN`，也**不关**客户端连接
        // （socket 留在服务端）。客户端此后的上行同样没有会话可路由（照 oracle 静默丢弃）。
        let stopped = state
            .lock()
            .is_ok_and(|s| s.running != RunningState::Running);
        if stopped {
            glog!(
                "DBG",
                "获取GameSvr服务器实例失败，请确认GameGate和GameSvr是否链接正常。"
            );
            tokio::spawn(async move {
                let mut sock = sock;
                let mut buf = [0u8; 4096];
                loop {
                    match sock.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            });
            continue;
        }
        let handle = raw_socket_i32(&sock);
        let sid = gen_session_id(&format!("{peer}|{}|{}", ctx.gate_addr, ctx.gate_port));
        // `ServerService.cs:131-147`：找首个空槽；全满则返回**最后一个非空槽**的 SckHandle
        // （`donor` 是那个宿主会话，复用其下行路由，见 `client_loop`）。
        let (slot_handle, donor) = match slots.lock() {
            Ok(mut table) => table.allocate(sid, handle),
            Err(_) => (handle, None),
        };
        glog!("INF", "开始连接: {peer} (SessionId={sid} socket={handle})");
        let ctx = ctx.clone();
        let slots = Arc::clone(&slots);
        tokio::spawn(async move {
            if let Err(e) = client_loop(
                sock,
                sid,
                slot_handle,
                handle,
                donor,
                peer.to_string(),
                ctx,
                slots,
            )
            .await
            {
                glog!("WRN", "客户端[{peer}]会话结束: {e:#}");
            }
        });
    }
}

/// 单客户端会话：登记会话表 → 发 `GM_OPEN` → 逐帧处置 → 断开发 `GM_CLOSE`。
#[allow(clippy::too_many_arguments)] // sid/handle/槽 handle/宿主/对端 都是 oracle 同名概念
async fn client_loop(
    sock: TcpStream,
    sid: u16,
    slot_handle: i32,
    handle: i32,
    donor: Option<u16>,
    peer: String,
    ctx: GateCtx,
    slots: Arc<StdMutex<SessionSlots>>,
) -> Result<()> {
    // 任何退出路径（含下面的 `?` 早退）都回收槽位。
    let _slot_guard = SlotGuard { slots, sid };
    let (mut rd, mut wr) = sock.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    // C# `SessionInfo.ReceiveTick`（`ClientSession.ProcessSessionPacket` 首行刷新）。
    let last_recv_ms = Arc::new(AtomicI64::new(now_ms()));
    // `ProcessIdleSession` 的 `Socket.Close()` 等价物：唤醒读循环 ⇒ 退出时关 socket、发 `GM_CLOSE`。
    let (close_tx, mut close_rx) = watch::channel(false);
    // 槽满复用时，新会话在 oracle 里绑的是**宿主的** `SessionInfo`：下行仍经宿主的
    // `ConnectionId` 发出（`ClientSession.cs:908`）。宿主已断开（槽已释放）⇒ oracle 的
    // `Session.Socket == null` 静默丢弃；这里不登记新键，让 `handle_server_message` 走
    // 「会话不存在，丢弃下行」分支。
    let route_tx = match donor {
        None => Some(tx),
        Some(donor_sid) => ctx.sessions.lock().await.get(&donor_sid).cloned(),
    };
    // 每会话的规则状态：聊天计时 + 三条速度规则的计时 + 延时消息队列
    // （C# `ClientSession` 里的 `gameSpeed` / `SayMsgTick` / `MsgList`）。
    let mut session = SessionRules {
        chat: rules::ChatThrottle::default(),
        // 三个 tick 的初值 = 会话创建时刻（`SessionSpeedRule()` 构造函数）。
        speed: rules::SpeedState::new(now_ms()),
    };

    // 回写任务：把下行读循环已经拼好的**客户端帧**（`#…!`，见 `client_frame`）原样写给客户端
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
        socket: slot_handle,
        session_id: sid,
        ident: u16::from(grobal2::GM_OPEN),
        session_index: 0,
        pack_length: i32::try_from(ip_bytes.len()).unwrap_or(i32::MAX),
    };
    ctx.downstream.send_internal(open, &ip_bytes);
    // oracle 顺序：先 `UserEnter`（发 GM_OPEN）、后 `SessionContainer.AddSession`
    // （`ServerService.cs:147-159`）⇒ 这里同样**在发送之后**登记会话与空闲项；
    // 发送不再可失败（断链时按 `Send` 的 `Online` 门静默丢弃），也就不存在失败早退的泄漏路径。
    if let Some(route_tx) = route_tx {
        ctx.sessions.lock().await.insert(sid, route_tx);
    }
    // 只有**占槽**的会话进入 `SessionArray` 的空闲清理面（槽满复用分支不新增 SessionInfo）。
    if donor.is_none() {
        ctx.idle.lock().await.insert(
            sid,
            IdleEntry {
                last_recv_ms: Arc::clone(&last_recv_ms),
                idle_eligible: true,
                close: close_tx.clone(),
            },
        );
    }

    // 读循环：**客户端 face 无定长头**，一次 read 就是**一帧**（`#…!`）——照 oracle
    // （`ServerService.Initialize()` 的 TcpService 不挂 DataHandlingAdapter ⇒ `ProcessSessionPacket`
    // 直接吃一次 read 的字节）。帧被 TCP 拆开/合并时的后果与 oracle 一致（都会被剔除）。
    let mut buf = vec![0u8; MAX_READ];
    let mut logged_in = false;
    let mut closed = Ok(());
    loop {
        let n = tokio::select! {
            // 空闲超时：oracle `ProcessIdleSession` 直接 `Socket.Close()`。
            _ = close_rx.changed() => break,
            read = rd.read(&mut buf) => match read {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            },
        };
        // C# `ClientSession.ProcessSessionPacket` 第一行：收到任何上行（含心跳/无效帧）都刷新。
        last_recv_ms.store(now_ms(), Ordering::Relaxed);
        match handle_frame(
            &buf[..n],
            logged_in,
            &mut session,
            &ctx,
            sid,
            slot_handle,
            now_ms(),
        )
        .await
        {
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

    // 离开：GM_CLOSE(session_id=0, socket=handle)（实机取证确认）。
    //
    // **同步发送**：oracle `ClientThread.UserLeave` → `SendServerMsg` → `Send` 是同步
    // `_clientSocket.Send`（`ClientThread.cs:384-399`），返回时帧已进内核。这里必须等写任务
    // 回执，否则客户端先看到 EOF、测试/进程收工会把还在队列里的 `GM_CLOSE` 丢掉。
    ctx.sessions.lock().await.remove(&sid);
    ctx.indexes.lock().await.remove(&sid);
    ctx.idle.lock().await.remove(&sid);
    let leave = ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: handle,
        session_id: 0,
        ident: u16::from(grobal2::GM_CLOSE),
        session_index: 0,
        pack_length: 0,
    };
    let _ = ctx.downstream.send_internal_sync(leave, &[]).await;
    writer.abort();
    glog!("INF", "断开链接: {peer} (SessionId={sid})");
    closed
}

/// 会话级可变规则状态（聊天计时 + 速度/延时队列），打包以减少函数参数。
struct SessionRules {
    /// `CM_SAY` 的间隔计时。
    chat: rules::ChatThrottle,
    /// 三条速度规则 + 延时消息队列。
    speed: rules::SpeedState,
}

/// 帧的**纯判定结果**：`handle_frame` 据此做 I/O，测试可直接断言（不经网络）。
#[derive(Debug, PartialEq, Eq)]
enum Classified {
    /// 登录帧（长度 ≥ 3）：调用方走 `handle_login`。
    Login,
    /// 不转发、会话继续：登录前的短帧（oracle 的越界异常被吞）与 1 字节心跳。
    Ignore,
    /// 静默丢弃：超长帧 / 速度规则 Drop / 聊天间隔未到。
    Drop,
    /// 速度规则 DropKick（oracle `SendKickMsg(0)` 只记日志）：同样丢帧、**不关会话**。
    DropKick,
    /// 异常帧：关会话（附原因）。
    Close(String),
    /// 放行：附解码后的命令字节（调用方重建 `GM_DATA` 帧并发送）。
    Forward(Vec<u8>),
}

/// 处置一帧：先做**纯判定**（`classify_frame`），再按结果做 I/O
/// （分支顺序照 `ClientSession.cs:98-` 的 `ProcessSessionPacket`）。
async fn handle_frame(
    frame: &[u8],
    logged_in: bool,
    session: &mut SessionRules,
    ctx: &GateCtx,
    sid: u16,
    handle: i32,
    now: i64,
) -> Result<Step> {
    match classify_frame(
        frame,
        logged_in,
        session,
        &ctx.gate_rules,
        ctx.nom_client_packet_size,
        sid,
        now,
    ) {
        Classified::Login => handle_login(frame, ctx, sid, handle, &mut session.speed, now).await,
        Classified::Ignore => Ok(Step::Next),
        Classified::Drop | Classified::DropKick => Ok(Step::Drop),
        Classified::Close(reason) => Ok(Step::Close(reason)),
        Classified::Forward(decoded) => {
            let payload = &frame[2..frame.len() - 1]; // = C# 的 `tempBuff`
            let session_index = *ctx.indexes.lock().await.get(&sid).unwrap_or(&0);
            let wire = rebuild_command_frame(&decoded, payload, handle, sid, session_index);
            // C# `ClientThread.Send`：离线时静默丢弃，不向会话层抛。
            let _ = ctx.downstream.send_raw(&wire);
            Ok(Step::Next)
        }
    }
}

/// `ProcessSessionPacket`（`ClientSession.cs:98-497`）的分支判定：**纯函数、不碰网络**，
/// 时钟由调用方传入（可测）。
fn classify_frame(
    frame: &[u8],
    logged_in: bool,
    session: &mut SessionRules,
    gate_rules: &rules::GateRules,
    nom_client_packet_size: usize,
    sid: u16,
    now: i64,
) -> Classified {
    if !logged_in {
        // 登录前走 oracle 的 `:671-680` else 分支（直接 `EDCode.DeCodeString`），**没有**
        // 「1 字节 = 心跳」与「< 16 字节 = 异常封包」这两条（那两条在 `HandleLogin` 分支里 `:155-164`）。
        // `packetLen <= 2`（含 0：`ushort` 下溢）会让 C# 的 `destinationSpan[2..(packetLen-1)]` 越界抛
        // ArgumentOutOfRangeException，被 `ServerManager.cs:238` 的 catch 吞掉（只记日志）
        // ⇒ 帧被丢、**会话存活**；这里照此丢弃（不是剔除）。
        if frame.len() < 3 {
            return Classified::Ignore;
        }
        return Classified::Login;
    }
    if frame.len() == 1 {
        // 心跳：`ClientSession.cs:157-160` 直接 `return`（不转发）
        return Classified::Ignore;
    }
    if frame.len() < COMMAND_FIXED_LENGTH {
        // 异常消息封包（`:155-162`）：关会话
        return Classified::Close(format!("异常消息封包（{} 字节），关闭会话...", frame.len()));
    }
    let payload = &frame[2..frame.len() - 1]; // = C# 的 `tempBuff`（去掉 `#1` 与尾 `!`）
    let decoded = mir2_protocol::edcode::decode(payload);
    if decoded.len() < COMMAND_MESSAGE_SIZE {
        // `:170-175`：解码后不足 12 字节 ⇒ 关会话（reviewer-1 的 Minor #3）
        return Classified::Close(format!("解析数据包小于12...解析后长度:{}", decoded.len()));
    }
    if frame.len() > nom_client_packet_size {
        // `:196-199`：超过 `NomClientPacketSize` **只记日志丢帧**（不是关会话）
        glog!(
            "INF",
            "Kick off user,over nom client packet size: {}",
            frame.len()
        );
        return Classified::Drop;
    }
    // `ClientSession.cs:206-388` 的 `switch (clientMessage.Ident)`：move/attack/spell 三条速度规则。
    // 顺序照 oracle：在 `NomClientPacketSize` 之后、转发之前。互斥的 ident 之间先后无影响，
    // 但惩罚分支的"丢帧/延迟"必须在这里发生（不能让帧先转发出去）。
    let ident = u16::from_le_bytes([decoded[4], decoded[5]]);
    let tag = u16::from_le_bytes([decoded[8], decoded[9]]);
    let verdict = match ident {
        mir2_protocol::messages::CM_WALK | mir2_protocol::messages::CM_RUN => {
            session.speed.on_move(now, tag, &gate_rules.speed)
        }
        mir2_protocol::messages::CM_HIT
        | mir2_protocol::messages::CM_HEAVYHIT
        | mir2_protocol::messages::CM_BIGHIT
        | mir2_protocol::messages::CM_POWERHIT
        | mir2_protocol::messages::CM_LONGHIT
        | mir2_protocol::messages::CM_WIDEHIT
        | mir2_protocol::messages::CM_CRSHIT
        | mir2_protocol::messages::CM_FIREHIT => {
            session.speed.on_attack(now, tag, &gate_rules.speed)
        }
        mir2_protocol::messages::CM_SPELL => session.speed.on_spell(now, tag, &gate_rules.speed),
        _ => rules::Verdict::Forward,
    };
    match verdict {
        rules::Verdict::Forward => {}
        rules::Verdict::Drop => {
            glog!(
                "DBG",
                "超速/间隔未到，丢弃该帧（Ident={ident} SessionId={sid}）"
            );
            return Classified::Drop;
        }
        rules::Verdict::DropKick => {
            // oracle `SendKickMsg(0)` 的发送代码整段被注释（`ClientSession.cs:1030-1046`）：
            // 只 `LogService.Debug(OverSpeedSendBack)`，**不发帧、不关会话** ⇒ 这里同样只记日志。
            glog!(
                "DBG",
                "超速累计（oracle SendKickMsg(0)，仅日志）：Ident={ident} SessionId={sid}"
            );
            return Classified::DropKick;
        }
    }
    // `CM_SAY` 的间隔门（`:485-497`）：太快的聊天帧**直接丢**（`return`，不转发）。
    // 内容 = 解码后的 `decodeBuff[12..]`；`@` 开头的内容不判间隔；开关与阈值照现网
    // `[Switch] ChatInterval=1` / `[Integer] ChatInterval=800`。
    if gate_rules.chat_interval_enabled
        && ident == mir2_protocol::messages::CM_SAY
        && !session.chat.allows(
            now,
            &decoded[COMMAND_MESSAGE_SIZE..],
            gate_rules.chat_interval_ms,
        )
    {
        glog!(
            "DBG",
            "聊天间隔未到（{}ms），丢弃该帧（SessionId={sid}）",
            gate_rules.chat_interval_ms
        );
        return Classified::Drop;
    }
    Classified::Forward(decoded)
}

/// 登录帧（`:673-679` → `ClientLogin`）：校验通过 ⇒ **补发合成登录包**（`:1199-1224`）并置“已登录”；
/// 失败 ⇒ 剔除（`:677-678` 的 `Kick(1)`）。登录帧本身**不转发**。
async fn handle_login(
    body: &[u8],
    ctx: &GateCtx,
    sid: u16,
    handle: i32,
    speed: &mut rules::SpeedState,
    now: i64,
) -> Result<Step> {
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
            ctx.downstream.send_internal(head, &login_body);
            // `ClientSession.cs:1252` 的 `SendLoginMessage` → `SendDelayMsg(..., 1)`：
            // 登录包入 `MsgList`（`DelayTime = now + 1`），`HandleLogin=true` 之后的三条
            // 速度规则会看到非空队列（`GetDelayMsgCount()`）。**发送本身仍照当前实现立即完成**
            // （oracle 实际会推迟到 drain 时才 `ClientThread.Send`；该延迟不在本片范围，见报告登记）。
            speed.delays.send_delay_msg(now, 1);
            Ok(Step::LoggedIn)
        }
    }
}

/// 网关 ↔ GameSvr：收管理帧并分派（`GM_*`，`ClientThread.cs:265-300` 的同构实现）。
async fn handle_server_message(ctx: &LinkCtx, msg: ServerMessage, body: &[u8]) {
    match u8::try_from(msg.ident).unwrap_or(0) {
        grobal2::GM_DATA => {
            let wire = client_frame(msg.pack_length, body);
            let guard = ctx.sessions.lock().await;
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
            ctx.sessions.lock().await.remove(&msg.session_id);
            ctx.indexes.lock().await.remove(&msg.session_id);
            glog!("INF", "游戏服务要求关闭会话[{}]", msg.session_id);
        }
        grobal2::GM_CHECKSERVER => {
            // `ClientThread.cs:268-271`：`CheckServerFail=false; CheckServerTick=now`。
            // oracle **不**恢复 `GateReady`（只有 `ClientSocketConnect` 置真）⇒ 这个分支只清
            // 失败标志、刷新 tick，不会把 3min 超时判定“救活”。
            if let Ok(mut st) = ctx.state.lock() {
                st.check_server_fail = false;
                st.last_server_tick = now_ms();
            }
        }
        grobal2::GM_RECEIVE_OK => {
            // `ClientThread.cs:279-280`：收到即回发同 ident 的 20B 头（其余字段全 0）。
            // GameSvr 的 `ProcessBufferSend` 在 `SendChecked>0` 时暂停下行，只有收到回显
            // 才复位（`ChannelMessageHandler.cs:166-169`）；否则要等 `SocCheckTimeOut`(50ms)。
            let echo = ServerMessage {
                packet_code: grobal2::PacketCode,
                socket: 0,
                session_id: 0,
                ident: u16::from(grobal2::GM_RECEIVE_OK),
                session_index: 0,
                pack_length: 0,
            };
            if !ctx.downstream.send_internal(echo, &[]) {
                glog!(
                    "WRN",
                    "GM_RECEIVE_OK 回显未发出（链路离线，按 oracle `Send` 的 Online 门静默丢弃）"
                );
            }
        }
        grobal2::GM_SERVERUSERINDEX => {
            // `ClientThread.cs:272-275`：把 SessionIndex 记到会话的 `SvrListIdx`，
            // 之后该会话发往 GameSvr 的每帧都带它（`ClientSession.cs:649`）。
            ctx.indexes
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
            // `ClientThread.cs:265-267`：置 `RunningState.Stop`；此后
            // `ServerManager.GetClientThread` 只挑 `Runing` 实例（`ServerManager.cs:161-163`），
            // 新客户端不再拿到 `GM_OPEN`（`ServerService.cs:123-129`），已有会话不动。
            if let Ok(mut st) = ctx.state.lock() {
                st.running = RunningState::Stop;
            }
            glog!(
                "WRN",
                "游戏引擎停止服务（{}:{}），网关不再分配用户连接到该实例",
                ctx.srv_addr,
                ctx.srv_port
            );
        }
        other => glog!("DBG", "未处理的 GM_{other}（SessionId={}）", msg.session_id),
    }
}

/// 下行读循环：逐帧读 20B 头 + body；EOF / 关闭信号 / 读失败 ⇒ 判死并通知监督循环。
async fn reader(
    mut rd: OwnedReadHalf,
    ctx: LinkCtx,
    generation: u64,
    mut close_rx: watch::Receiver<bool>,
    events: mpsc::UnboundedSender<LinkEvent>,
) {
    loop {
        let mut head = [0u8; ServerMessage::SIZE];
        let read = tokio::select! {
            // `Stop()` 的全关信号；oracle 的 `_clientSocket.Close()` 会同时终止读循环。
            _ = close_rx.changed() => break,
            r = rd.read_exact(&mut head) => r,
        };
        if read.is_err() {
            break;
        }
        let Some(msg) = ServerMessage::from_bytes(&head) else {
            continue;
        };
        let body_len = msg.body_len();
        let mut body = vec![0u8; body_len];
        if body_len > 0 {
            let read = tokio::select! {
                _ = close_rx.changed() => break,
                r = rd.read_exact(&mut body) => r,
            };
            if read.is_err() {
                break;
            }
        }
        handle_server_message(&ctx, msg, &body).await;
    }
    // 读端先判死；写任务可能随后也发 Dead（监督循环只认第一次）。
    ctx.downstream.mark_dead(generation);
    let _ = events.send(LinkEvent::Dead(generation));
}

/// 下行写循环：一帧一个完整线上帧；写失败 / 关闭信号 ⇒ 判死并通知监督循环。
async fn writer(
    mut rx: mpsc::UnboundedReceiver<Outbound>,
    mut wr: OwnedWriteHalf,
    downstream: Downstream,
    generation: u64,
    mut close_rx: watch::Receiver<bool>,
    events: mpsc::UnboundedSender<LinkEvent>,
) {
    loop {
        let out = tokio::select! {
            _ = close_rx.changed() => break,
            item = rx.recv() => match item {
                Some(out) => out,
                None => break,
            },
        };
        let written = wr.write_all(&out.wire).await.is_ok() && wr.flush().await.is_ok();
        // 同步发送方（离开路径的 `GM_CLOSE`）在等这个回执：只有确认写进 socket 才放它返回。
        if let Some(ack) = out.ack {
            let _ = ack.send(written);
        }
        if !written {
            break;
        }
    }
    downstream.mark_dead(generation);
    let _ = events.send(LinkEvent::Dead(generation));
}

/// C# `ClientThread.Start()`（`ClientThread.cs:431-460`）：`if (_clientSocket.Online) return;`
/// 后 `ConnectAsync()`。oracle 是 fire-and-forget；这里用 `connecting` 闸门避免重叠 tick 的连接风暴
/// （失败计数/日志仍按 tick 照打）。
fn spawn_connect(ctx: &LinkCtx, events: &mpsc::UnboundedSender<LinkEvent>, connecting: &mut bool) {
    if ctx.downstream.is_ready() || *connecting {
        return;
    }
    *connecting = true;
    let ctx = ctx.clone();
    let events = events.clone();
    tokio::spawn(async move {
        let (addr, port) = ctx.downstream.addr();
        match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((addr, port))).await {
            Ok(Ok(stream)) => {
                stream.set_nodelay(true).ok();
                let generation = ctx.downstream.next_generation();
                let _ = events.send(LinkEvent::Connected { generation, stream });
            }
            Ok(Err(e)) => {
                let event = match e.kind() {
                    std::io::ErrorKind::ConnectionRefused => LinkEvent::ConnectRefused,
                    std::io::ErrorKind::TimedOut => LinkEvent::ConnectTimedOut,
                    _ => LinkEvent::ConnectFailed,
                };
                let _ = events.send(event);
            }
            Err(_) => {
                let _ = events.send(LinkEvent::ConnectTimedOut);
            }
        }
    });
}

fn install_connection(
    ctx: &LinkCtx,
    generation: u64,
    stream: TcpStream,
    events: &mpsc::UnboundedSender<LinkEvent>,
) {
    let (rd, wr) = stream.into_split();
    let (tx, rx) = mpsc::unbounded_channel::<Outbound>();
    let (close_tx, close_rx) = watch::channel(false);
    ctx.downstream.install(generation, tx, close_tx);
    tokio::spawn(writer(
        rx,
        wr,
        ctx.downstream.clone(),
        generation,
        close_rx.clone(),
        events.clone(),
    ));
    tokio::spawn(reader(
        rd,
        ctx.clone(),
        generation,
        close_rx,
        events.clone(),
    ));
}

/// C# `ClientSocketConnect`（`ClientThread.cs:152-161`）：`GateReady=true; CheckServerTick=now;`
/// `Connected=true; RunningState=Runing; RestSessionArray();` + 链接成功日志。
async fn on_connected(
    ctx: &LinkCtx,
    generation: u64,
    stream: TcpStream,
    events: &mpsc::UnboundedSender<LinkEvent>,
) {
    if let Ok(mut st) = ctx.state.lock() {
        st.gate_ready = true;
        st.last_server_tick = now_ms();
        st.running = RunningState::Running;
    }
    // C# 在连接回调里也调 `RestSessionArray()`：链路每次建立都把当前会话摘出空闲清理面。
    detach_idle_sessions(&ctx.idle).await;
    install_connection(ctx, generation, stream, events);
    glog!(
        "INF",
        "{}",
        connected_log(&ctx.local_endpoint, ctx.downstream.endpoint())
    );
}

/// 链路监督：`TimedService` 每 10ms 看一次、严格 `>` 过周期才跑一轮
/// `CheckConnectedState`；连接事件驱动重连与断开状态复位。
#[allow(clippy::too_many_lines)] // 状态机 + 事件分派一一对应 oracle 分支；拆函数只会多传一坨 Clone 上下文
async fn link_loop(
    ctx: LinkCtx,
    events: mpsc::UnboundedSender<LinkEvent>,
    mut events_rx: mpsc::UnboundedReceiver<LinkEvent>,
) {
    let period_ms = i64::try_from(ctx.period.as_millis()).unwrap_or(i64::MAX);
    let mut connecting = false;
    let mut link_up = ctx.downstream.is_ready();
    let mut last_check = now_ms();
    let mut poll = tokio::time::interval(Duration::from_millis(TIMED_POLL_MS));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    poll.tick().await; // `interval` 首 tick 立即就绪；跳过 ⇒ 第一次判定落在 `period` 之后。

    loop {
        tokio::select! {
            _ = poll.tick() => {
                let now = now_ms();
                if !keepalive_due(now, last_check, period_ms) {
                    continue;
                }
                last_check = now;
                let action = match ctx.state.lock() {
                    Ok(mut st) => connected_state_action(&mut st, now, ctx.timeout_ms),
                    Err(_) => continue,
                };
                match action {
                    KeepaliveTick::Send => {
                        // `SendServerMsg(GM_CHECKCLIENT, 0, 0, 0, "", 0)`：20B 头，其余字段全 0。
                        let head = ServerMessage {
                            packet_code: grobal2::PacketCode,
                            socket: 0,
                            session_id: 0,
                            ident: u16::from(grobal2::GM_CHECKCLIENT),
                            session_index: 0,
                            pack_length: 0,
                        };
                        ctx.downstream.send_internal(head, &[]);
                    }
                    KeepaliveTick::Maintenance => {
                        // oracle 先 `_ = Start()`（在线时立即返回）、后打日志。
                        glog!(
                            "DBG",
                            "游戏引擎维护时间结束,重新连接游戏引擎[{}].",
                            ctx.downstream.endpoint()
                        );
                        spawn_connect(&ctx, &events, &mut connecting);
                    }
                    KeepaliveTick::RetryConnect { fail_count } => {
                        // `_ = Start(); CheckServerFailCount++; log(新计数)`（`:449-455`）。
                        glog!(
                            "DBG",
                            "链接服务器[{}:{}] 失败次数[{fail_count}]",
                            ctx.srv_addr,
                            ctx.srv_port
                        );
                        spawn_connect(&ctx, &events, &mut connecting);
                    }
                    KeepaliveTick::MaxRetries => glog!(
                        "DBG",
                        "超过最大重试次数，请重启程序后再次确认链接是否正常。"
                    ),
                    KeepaliveTick::TimeoutStop { fail_count } => {
                        // `Stop()`：oracle 的 `_clientSocket.Close()` 全关；本实现给读循环发关闭信号
                        // 并摘掉写端（读/写任务都退出，不是只半关写半端）。
                        ctx.downstream.stop();
                        glog!(
                            "DBG",
                            "服务器[{}:{}]长时间没有响应,断开链接.失败次数:[{fail_count}]",
                            ctx.srv_addr,
                            ctx.srv_port
                        );
                    }
                    KeepaliveTick::Idle => {}
                }
            }
            event = events_rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    LinkEvent::Connected { generation, stream } => {
                        connecting = false;
                        on_connected(&ctx, generation, stream, &events).await;
                        link_up = true;
                    }
                    LinkEvent::ConnectRefused => {
                        connecting = false;
                        if let Ok(mut st) = ctx.state.lock() {
                            // `ClientThread.ClientSocketError`：GateReady=false; CheckServerFail=true。
                            st.gate_ready = false;
                            st.check_server_fail = true;
                        }
                        glog!(
                            "WRN",
                            "游戏网关[{}] 链接游戏引擎[{}]拒绝链接...",
                            ctx.local_endpoint,
                            ctx.downstream.endpoint()
                        );
                    }
                    LinkEvent::ConnectTimedOut => {
                        connecting = false;
                        if let Ok(mut st) = ctx.state.lock() {
                            st.gate_ready = false;
                            st.check_server_fail = true;
                        }
                        glog!(
                            "INF",
                            "游戏网关[{}] 链接游戏引擎[{}]超时...",
                            ctx.local_endpoint,
                            ctx.downstream.endpoint()
                        );
                    }
                    LinkEvent::ConnectFailed => {
                        connecting = false;
                        if let Ok(mut st) = ctx.state.lock() {
                            st.gate_ready = false;
                            st.check_server_fail = true;
                        }
                        glog!(
                            "INF",
                            "游戏网关[{}] 链接游戏引擎[{}]失败...",
                            ctx.local_endpoint,
                            ctx.downstream.endpoint()
                        );
                    }
                    LinkEvent::Dead(generation) => {
                        // 同一代次读/写任务都可能判死；只处理第一次。
                        if generation == ctx.downstream.current_generation() && link_up {
                            link_up = false;
                            ctx.downstream.mark_dead(generation);
                            // `ClientSocketDisconnect` 的 `RestSessionArray()`。
                            detach_idle_sessions(&ctx.idle).await;
                            if let Ok(mut st) = ctx.state.lock() {
                                st.gate_ready = false;
                                st.check_server_fail = true;
                            }
                            glog!(
                                "INF",
                                "{}",
                                disconnected_log(&ctx.local_endpoint, ctx.downstream.endpoint())
                            );
                        }
                    }
                }
            }
        }
    }
}

/// `TimedService.KeepAlive`（`TimedService.cs:69-88`）的 10s 门：严格 `>`。
fn keepalive_due(now: i64, last_check: i64, period_ms: i64) -> bool {
    now - last_check > period_ms
}

/// `ClientThread.CheckConnectedState`（`ClientThread.cs:431-474`）的完整状态机：
/// 判定 + 计数变更 + 选动作，I/O 由调用方做。严格按 oracle 的分支顺序。
fn connected_state_action(state: &mut GateState, now: i64, timeout_ms: i64) -> KeepaliveTick {
    if state.running == RunningState::Stop {
        // 停止服务后暂时停止心跳检查；`> 60 * 10000` 才 `Start()`。
        if now - state.last_server_tick > MAINTENANCE_MS {
            return KeepaliveTick::Maintenance;
        }
        return KeepaliveTick::Idle;
    }
    if state.gate_ready {
        // 先发健康帧、再把失败计数清零（oracle `:443-448`）。
        state.check_server_fail_count = 0;
        return KeepaliveTick::Send;
    }
    if state.check_server_fail && state.check_server_fail_count <= U16_MAX {
        // `_ = Start(); CheckServerFailCount++; log(新计数)`（`:449-455`）。
        state.check_server_fail_count += 1;
        return KeepaliveTick::RetryConnect {
            fail_count: state.check_server_fail_count,
        };
    }
    if state.check_server_fail_count >= U16_MAX {
        return KeepaliveTick::MaxRetries;
    }
    // `CheckServerTimeOut()`（`:463-472`）：严格 `>`；触发后 `CheckServerFail=true; Stop(); count++`。
    if now - state.last_server_tick > timeout_ms && state.check_server_fail_count <= U16_MAX {
        state.check_server_fail = true;
        state.check_server_fail_count += 1;
        return KeepaliveTick::TimeoutStop {
            fail_count: state.check_server_fail_count,
        };
    }
    KeepaliveTick::Idle
}

/// `ClientThread.ProcessIdleSession`（`ClientThread.cs:410-427`）的判定：严格 `>`。
fn session_idle_expired(now: i64, last_recv: i64, timeout_ms: i64) -> bool {
    now - last_recv > timeout_ms
}

/// `TimedService.ClearIdleSession`（`TimedService.cs:151-173`）：每 `sweep` 扫一遍
/// `SessionArray` 的非空槽，超时者关客户端 socket（读循环退出 ⇒ 发 `GM_CLOSE`）。
async fn session_sweep_loop(idle: IdleSessions, sweep: Duration, idle_ms: i64) {
    loop {
        tokio::time::sleep(sweep).await;
        let now = now_ms();
        let stale: Vec<watch::Sender<bool>> = {
            let guard = idle.lock().await;
            guard
                .values()
                .filter(|entry| {
                    // oracle `ProcessIdleSession`：`userSession.Socket != null` 才参与超时判定；
                    // `RestSessionArray()` 摘除过的会话（`idle_eligible == false`）永不在这里关闭。
                    entry.idle_eligible
                        && session_idle_expired(
                            now,
                            entry.last_recv_ms.load(Ordering::Relaxed),
                            idle_ms,
                        )
                })
                .map(|entry| entry.close.clone())
                .collect()
        };
        for close in &stale {
            let _ = close.send(true);
        }
        // oracle 在清完整轮后打 Debug（没有任何会话也打）。
        glog!("DBG", "清理空闲或无效客户端会话...");
    }
}

/// 命令帧的出帧重建（照 `ClientSession.cs:646-669`；口径见协议梳理 §7.8/§7.9）：
/// - 无尾部（解码后 ≤ `COMMAND_MESSAGE_SIZE`=12）：`PackLength = 12`，body = **解码态**字节；
/// - 有尾部：`PackLength = 12 + tLen + 1`（`tLen` = 尾部重编码后的长度），
///   body = 解码态 12B + **输入的编码态尾部**（`payload[16..]`）+ 1 字节；
/// - 那 1 字节在 oracle 里是 `BytePool.Rent` 的**未初始化内存**，这里固定送 `0`（已知的不可比对差异）。
///
/// **S3 路由修复**：头里的 `Socket` 必须是**该客户端连接的 socket handle**——oracle 的
/// `SendMsg` 在构造时就把 `Socket = _session.SckHandle`（`ClientSession.cs:65-70`），
/// 与 `GM_OPEN` 的 `socketId`（`ClientThread.cs:378` 的 `UserEnter` 传 `userSession.SckHandle`）
/// 是同一个值。GameSvr 侧收 `GM_DATA` 时**先按 `SessionIndex-1` 找槽、且要求 `Socket` 相同**，
/// 否则线性扫 `Socket`（`ChannelMessageHandler.cs:170-198`）⇒ 送 0 会让每个已进世界的会话都
/// 落进「用户不存在」分支、客户端命令（走路/攻击/说话）全被丢。真机症状：
/// `[WRN] SessionIndex=1 socket=0 的用户不存在，丢弃 12/28/32/35 字节`。
fn rebuild_command_frame(
    decoded: &[u8],
    payload: &[u8],
    handle: i32,
    sid: u16,
    session_index: i32,
) -> Vec<u8> {
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
        // 命令帧的 Socket = 客户端 handle（= GM_OPEN 的 socketId），照 `SendMsg.Socket = _session.SckHandle`
        // （`ClientSession.cs:65-70`）；送 0 会让 GameSvr 的按 socket 查找全部落空（见函数文档）。
        socket: handle,
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

/// 旧二进制入口（`game-gate.exe`）：argv 语义与改造前一致（`env::args().skip(1)`）。
///
/// 单测/`mir2-server` 都走 [`run_with_args`]，本函数只负责取进程 argv。
#[must_use]
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run_with_args(&args)
}

/// 角色分派入口：`args` = **角色名之后**的剩余 argv（与旧二进制的 `env::args().skip(1)` 等长同序，
/// 含首个可选配置文件路径）。启动参数解析、日志、端口、配置读取全部沿用 [`run_inner`]，
/// 退出码口径与 `fn main() -> Result<()>` 相同（成功 0，失败 1）。
///
/// # Panics
///
/// 仅在 tokio 运行期构建失败时 panic（与 `#[tokio::main]` 展开的行为一致）。
#[must_use]
pub fn run_with_args(args: &[String]) -> i32 {
    use std::io::Write as _;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed building the Runtime");
    let result = rt.block_on(run_inner(args));
    let code = match result {
        Ok(()) => 0,
        Err(err) => {
            // 与 `fn main() -> Result<()>` 的 `Termination` 实现同形：`Error: {err:?}` 进 stderr。
            let _ = writeln!(std::io::stderr(), "Error: {err:?}");
            1
        }
    };
    // `process::exit` 跳过运行期退出清理，显式刷新，保持旧二进制"退出前输出可见"。
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployed_speed() -> rules::SpeedRules {
        rules::SpeedRules {
            move_interval: 366,
            attack_interval: 500,
            ..rules::SpeedRules::oracle_defaults()
        }
    }

    fn gate_rules() -> rules::GateRules {
        rules::GateRules {
            chat_interval_enabled: true,
            chat_interval_ms: 800,
            speed: deployed_speed(),
        }
    }

    /// 构造客户端线缆帧：`#1` + Encode(12B CommandMessage) + `!`（客户端 face 无定长头）。
    fn walk_frame(direction: u16) -> Vec<u8> {
        let mut cmd = vec![0u8; COMMAND_MESSAGE_SIZE];
        cmd[4..6].copy_from_slice(&mir2_protocol::messages::CM_WALK.to_le_bytes());
        cmd[8..10].copy_from_slice(&direction.to_le_bytes());
        let mut frame = Vec::new();
        frame.extend_from_slice(b"#1");
        frame.extend_from_slice(&mir2_protocol::edcode::encode(&cmd));
        frame.push(b'!');
        assert!(
            frame.len() >= COMMAND_FIXED_LENGTH,
            "编码后必须过 <16 字节门"
        );
        frame
    }

    /// F5：经 relay 的帧判定入口注入 `CM_WALK`。登录包已入队（`DelayTime=1`）⇒ 60ms 处
    /// 队列非空、间隔未到 ⇒ **丢帧**（不是 DropKick）；drain 后同一会话按正常间隔规则
    /// （DelaySend 空队列支）**放行**。
    #[test]
    fn relay_walk_dropped_while_login_delay_pending_then_forwarded() {
        let rules = gate_rules();
        let mut session = SessionRules {
            chat: rules::ChatThrottle::default(),
            speed: rules::SpeedState::new(1_000),
        };
        session.speed.delays.send_delay_msg(1_000, 1); // = handle_login 的 SendDelayMsg(..., 1)
        assert_eq!(
            classify_frame(&walk_frame(1), true, &mut session, &rules, 400, 7, 1_060),
            Classified::Drop,
            "登录后 60ms：队列非空 ⇒ 丢帧，且**不** DropKick"
        );
        match classify_frame(&walk_frame(2), true, &mut session, &rules, 400, 7, 1_260) {
            Classified::Forward(decoded) => {
                assert_eq!(
                    u16::from_le_bytes([decoded[4], decoded[5]]),
                    mir2_protocol::messages::CM_WALK
                );
                assert_eq!(
                    u16::from_le_bytes([decoded[8], decoded[9]]),
                    2,
                    "转发的是第二帧"
                );
            }
            other => panic!("drain 后应放行，实际 {other:?}"),
        }
    }

    /// F5（端到端）：同一帧经 `handle_frame`（relay 的会话入口）注入：首帧被规则丢弃，
    /// **不向下游写任何字节**；第二帧被转发成 20B `GM_DATA` 头 + 12B 解码命令。
    #[tokio::test]
    async fn relay_session_entry_drops_then_forwards_walk_frame() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let downstream = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut head = [0u8; ServerMessage::SIZE];
            sock.read_exact(&mut head).await.unwrap();
            let msg = ServerMessage::from_bytes(&head).unwrap();
            let mut body = vec![0u8; msg.body_len()];
            sock.read_exact(&mut body).await.unwrap();
            (msg, body)
        });
        let stream = TcpStream::connect(addr).await.unwrap();
        let (_rd, mut wr) = stream.into_split();
        // 生产里写任务由 `link_loop` 的 `install_connection` 起；这里装同一条写通路。
        let down = Downstream::new("127.0.0.1".into(), 0);
        let (tx, mut rx) = mpsc::unbounded_channel::<Outbound>();
        let (close_tx, _close_rx) = watch::channel(false);
        down.install(1, tx, close_tx);
        let _writer = tokio::spawn(async move {
            while let Some(out) = rx.recv().await {
                let written = wr.write_all(&out.wire).await.is_ok() && wr.flush().await.is_ok();
                if let Some(ack) = out.ack {
                    let _ = ack.send(written);
                }
                if !written {
                    break;
                }
            }
        });
        let ctx = GateCtx {
            gate_addr: "127.0.0.1".into(),
            gate_port: 0,
            service_id: 0,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            indexes: Arc::new(Mutex::new(HashMap::new())),
            idle: Arc::new(Mutex::new(HashMap::new())),
            downstream: down,
            gate_rules: gate_rules(),
            nom_client_packet_size: 400,
        };
        let mut session = SessionRules {
            chat: rules::ChatThrottle::default(),
            speed: rules::SpeedState::new(1_000),
        };
        session.speed.delays.send_delay_msg(1_000, 1);
        assert_eq!(
            handle_frame(&walk_frame(1), true, &mut session, &ctx, 7, 0x1234, 1_060)
                .await
                .unwrap(),
            Step::Drop,
            "队列非空 ⇒ 规则丢帧，下游零字节"
        );
        assert_eq!(
            handle_frame(&walk_frame(2), true, &mut session, &ctx, 7, 0x1234, 1_260)
                .await
                .unwrap(),
            Step::Next,
            "drain 后放行"
        );
        let (head, body) = downstream.await.unwrap();
        assert_eq!(head.session_id, 7);
        assert_eq!(head.ident, u16::from(grobal2::GM_DATA));
        assert_eq!(
            head.pack_length,
            i32::try_from(COMMAND_MESSAGE_SIZE).unwrap()
        );
        assert_eq!(body.len(), COMMAND_MESSAGE_SIZE, "12B 解码态命令");
        assert_eq!(
            u16::from_le_bytes([body[8], body[9]]),
            2,
            "转发的是第二帧（第一帧必须没写下游）"
        );
    }

    /// 离开路径的控制帧必须**同步**落地（oracle `ClientThread.Send` 的同步 `Socket.Send`，
    /// `ClientThread.cs:393-399`）：`send_frame_sync` 在写任务回执之前**不得**返回；
    /// 回执成功返回 `true`。写任务消失（链路死/`Stop`）时 `ack` 被丢弃 ⇒ 立即返回 `false`，
    /// 不得挂住会话任务。
    ///
    /// 判据不靠 sleep 碰运气：`timeout(50ms, &mut fut)` 只有「未来已就绪」才会是 `Ok`，
    /// 所以提前返回必定红；机器负载只会让超时更容易成立。
    #[tokio::test]
    async fn sync_control_frame_waits_for_writer_ack() {
        let down = Downstream::new("127.0.0.1".into(), 0);
        let (tx, mut rx) = mpsc::unbounded_channel::<Outbound>();
        let (close_tx, _close_rx) = watch::channel(false);
        down.install(1, tx, close_tx);

        // 1) 写任务还没确认 ⇒ 同步发送必须挂住。
        let mut send = Box::pin(down.send_frame_sync(vec![7, 8, 9]));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut send)
                .await
                .is_err(),
            "写任务确认前 send_frame_sync 不得返回"
        );
        let out = rx.recv().await.expect("同步帧必须入队");
        assert_eq!(out.wire, vec![7, 8, 9], "入队字节不得被改写");
        // 2) 写任务回执 true ⇒ 同步发送返回 true。
        out.ack
            .expect("同步帧必须带回执")
            .send(true)
            .expect("回执接收端仍在等");
        assert!(send.await, "写任务确认成功 ⇒ true");

        // 3) 回执端被丢弃（写任务退出/Stop）⇒ false，且不得挂住。
        //    先 poll 一次让 future 真的入队（async fn 不 poll 不执行）。
        let mut send = Box::pin(down.send_frame_sync(vec![1]));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut send)
                .await
                .is_err(),
            "写任务确认前 send_frame_sync 不得返回"
        );
        let out = rx.recv().await.expect("同步帧必须入队");
        drop(out);
        assert!(!send.await, "写任务消失 ⇒ false（不得一直等）");
    }

    /// B-4：槽位按「首个空槽」分配；全满时照 oracle 返回**最后一个非空槽**的 handle，
    /// 且不吞掉新连接（`ServerService.cs:131-147`）。
    #[test]
    fn session_slots_allocate_release_and_full_branch() {
        let mut slots = SessionSlots::new(2);
        assert_eq!(slots.allocate(11, 100), (100, None));
        assert_eq!(slots.allocate(22, 200), (200, None));
        assert_eq!(slots.active(), 2);
        assert_eq!(
            slots.allocate(33, 300),
            (200, Some(22)),
            "槽满：返回最后一个非空槽（22）的 SckHandle，宿主 sid=22"
        );
        assert_eq!(slots.active(), 2, "槽满不新增槽");
        // 释放后同一 sid 可再用（旧 `used: HashSet` 泄漏的回归牙）。
        slots.release(11);
        assert_eq!(slots.active(), 1);
        assert_eq!(slots.allocate(11, 111), (111, None));
        // 回收必须按宿主匹配：槽满分支的新会话从未入槽，断开它不得误清宿主槽。
        slots.release(33);
        assert_eq!(slots.active(), 2, "33 从未入槽，release 不得清掉宿主槽");
    }

    // ------------------------------------------------------------------
    // audit #18：GameGate↔GameSvr 保活/超时（tier=R）的构造级判据
    // ------------------------------------------------------------------

    fn link_state() -> GateState {
        GateState {
            running: RunningState::Running,
            gate_ready: false,
            check_server_fail: false,
            check_server_fail_count: 0,
            last_server_tick: 1_000,
        }
    }

    /// C1：`TimedService.KeepAlive` 的 10s 门是严格 `>`（`TimedService.cs:71`）。
    #[test]
    fn keepalive_period_is_strictly_greater() {
        assert!(!keepalive_due(10_000, 0, 10_000), "恰好 10000ms 不得触发");
        assert!(keepalive_due(10_001, 0, 10_000), "10001ms 必须触发");
        assert_eq!(KEEPALIVE_MS, 10_000);
    }

    /// C2：`CheckServerTimeOut` 的 3min 边界是严格 `>`，触发即
    /// `CheckServerFail=true` + `Stop()`(动作) + 计数 +1（`ClientThread.cs:463-472`）。
    #[test]
    fn check_server_timeout_boundary_marks_fail_and_counts() {
        assert_eq!(CHECK_SERVER_TIMEOUT_MS, 3 * 60 * 1000);
        let mut st = link_state();
        assert_eq!(
            connected_state_action(&mut st, 1_000 + 180_000, CHECK_SERVER_TIMEOUT_MS),
            KeepaliveTick::Idle,
            "恰好 3min 不得超时（oracle 是 `>`）"
        );
        assert!(!st.check_server_fail);
        assert_eq!(st.check_server_fail_count, 0);

        assert_eq!(
            connected_state_action(&mut st, 1_000 + 180_001, CHECK_SERVER_TIMEOUT_MS),
            KeepaliveTick::TimeoutStop { fail_count: 1 },
            "3min+1ms 必须走 CheckServerTimeOut"
        );
        assert!(st.check_server_fail, "超时后 CheckServerFail 必须置真");
        assert_eq!(st.check_server_fail_count, 1);
    }

    /// C5：超时后的动作/顺序——失败计数逐 tick +1，到 `ushort.MaxValue` 的下一轮转
    /// `MaxRetries`（`ClientThread.cs:449-459`）。
    #[test]
    fn timeout_then_retry_until_max_retries() {
        let mut st = link_state();
        assert_eq!(
            connected_state_action(&mut st, 181_001, CHECK_SERVER_TIMEOUT_MS),
            KeepaliveTick::TimeoutStop { fail_count: 1 }
        );
        for expected in 2..=U16_MAX {
            assert_eq!(
                connected_state_action(&mut st, 181_010, CHECK_SERVER_TIMEOUT_MS),
                KeepaliveTick::RetryConnect {
                    fail_count: expected
                },
                "CheckServerFail 分支每 tick 计数 +1"
            );
        }
        assert_eq!(
            connected_state_action(&mut st, 181_010, CHECK_SERVER_TIMEOUT_MS),
            KeepaliveTick::RetryConnect { fail_count: 65_536 },
            "65535 <= ushort.MaxValue 仍走重连分支"
        );
        assert_eq!(
            connected_state_action(&mut st, 181_010, CHECK_SERVER_TIMEOUT_MS),
            KeepaliveTick::MaxRetries
        );
        assert_eq!(U16_MAX, u32::from(u16::MAX));
    }

    /// audit #18 ② 的可达性：`TimeoutStop`（3min）分支要求 `!gate_ready && !check_server_fail`。
    ///
    /// 生产路径里**所有**置 `gate_ready = false` 的地方（读 EOF / 写失败判死）都同时置
    /// `check_server_fail = true` ⇒ 普通断链走 `RetryConnect`，3min 分支不可达。唯一能让
    /// `check_server_fail` 留在 `false` 的是窄竞态：写失败判死先把 `gate_ready` 置假，随后
    /// 读循环处理到在途的 `GM_CHECKSERVER` 又清零 fail（并刷新 tick）。本用例钉住这两条
    /// 状态序列的判定结果（构造级；进程级竞态无法确定性注入）。
    #[test]
    fn timeout_branch_needs_the_write_failure_race() {
        // 普通断链：两标志同时置真 ⇒ 每 tick 都是重连，不会进 3min 分支。
        let mut st = link_state();
        st.gate_ready = false;
        st.check_server_fail = true;
        assert_eq!(
            connected_state_action(
                &mut st,
                1_000 + CHECK_SERVER_TIMEOUT_MS + 1,
                CHECK_SERVER_TIMEOUT_MS
            ),
            KeepaliveTick::RetryConnect { fail_count: 1 },
            "普通断链必须先走 Start() 重连分支"
        );

        // 窄竞态后：`!gate_ready && !check_server_fail` ⇒ 3min 严格 `>` 才 TimeoutStop。
        let mut st = link_state();
        st.gate_ready = false; // 写失败判死（`writer` → Dead → GateReady=false）
        st.check_server_fail = false; // 随后到达的 GM_CHECKSERVER 清零 fail
        st.last_server_tick = 1_000;
        assert_eq!(
            connected_state_action(
                &mut st,
                1_000 + CHECK_SERVER_TIMEOUT_MS,
                CHECK_SERVER_TIMEOUT_MS
            ),
            KeepaliveTick::Idle,
            "恰好 3min 不得触发（严格 `>`）"
        );
        assert_eq!(
            connected_state_action(
                &mut st,
                1_000 + CHECK_SERVER_TIMEOUT_MS + 1,
                CHECK_SERVER_TIMEOUT_MS
            ),
            KeepaliveTick::TimeoutStop { fail_count: 1 }
        );
    }

    /// audit #18 ③ + `RestSessionArray`：链路建立/断开把会话摘出空闲清理面
    /// （oracle `SessionInfo.Socket = null`），但会话登记本身**保留**（不清表）。
    /// 牙：`detach_idle_sessions` 改成 `idle.clear()` ⇒ `len()` 变 0 ⇒ 红；把
    /// `session_sweep_loop` 的 `idle_eligible` 门删掉 ⇒ 进程级
    /// `downstream_drop_detaches_existing_sessions_from_idle_close` 红。
    #[tokio::test]
    async fn rest_session_array_detaches_idle_but_keeps_sessions() {
        let idle: IdleSessions = Arc::new(Mutex::new(HashMap::new()));
        let (close, _rx) = watch::channel(false);
        idle.lock().await.insert(
            7,
            IdleEntry {
                last_recv_ms: Arc::new(AtomicI64::new(1_000)),
                idle_eligible: true,
                close,
            },
        );
        detach_idle_sessions(&idle).await;
        {
            let guard = idle.lock().await;
            assert_eq!(guard.len(), 1, "RestSessionArray 不得清空会话登记");
            assert!(!guard[&7].idle_eligible, "断链后会话必须不再参与空闲清理");
        }
        // 幂等：重连再次 RestSessionArray 不会改变结论。
        detach_idle_sessions(&idle).await;
        assert!(!idle.lock().await[&7].idle_eligible);
    }

    /// Moderate 2（`Stop()` 全关）：oracle `ClientThread.Stop()` 的 `_clientSocket.Close()`
    /// 同时终止读/写。本实现用关闭信号唤醒读循环、并摘掉写端——**两个任务都必须退出**
    /// （旧实现只对写半端 `shutdown()`，读循环没有关闭信号）。真实 TCP 对端同时必须读到 EOF。
    #[tokio::test]
    async fn stop_closes_the_downstream_socket_fully() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let peer = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1];
            let eof = matches!(
                tokio::time::timeout(Duration::from_millis(300), sock.read(&mut buf)).await,
                Ok(Ok(0))
            );
            // **不要立刻关对端**：否则对端关闭会反过来唤醒网关读循环，掩盖“只半关写端”。
            tokio::time::sleep(Duration::from_millis(800)).await;
            eof
        });
        let stream = TcpStream::connect(addr).await.unwrap();
        let down = Downstream::new("127.0.0.1".into(), addr.port());
        let ctx = LinkCtx {
            downstream: down.clone(),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            indexes: Arc::new(Mutex::new(HashMap::new())),
            idle: Arc::new(Mutex::new(HashMap::new())),
            state: Arc::new(StdMutex::new(GateState {
                running: RunningState::Running,
                gate_ready: true,
                check_server_fail: false,
                check_server_fail_count: 0,
                last_server_tick: now_ms(),
            })),
            local_endpoint: "127.0.0.1:0".into(),
            srv_addr: "127.0.0.1".into(),
            srv_port: addr.port(),
            period: Duration::from_millis(10),
            timeout_ms: 60_000,
        };
        let (events, mut events_rx) = mpsc::unbounded_channel();
        let generation = down.next_generation();
        install_connection(&ctx, generation, stream, &events);
        down.stop();
        // 读、写任务各发一次 Dead：两个都到 = 读循环也被关闭信号唤醒（不是只半关写端）。
        let mut dead = 0;
        for _ in 0..2 {
            match tokio::time::timeout(Duration::from_millis(500), events_rx.recv()).await {
                Ok(Some(LinkEvent::Dead(g))) if g == generation => dead += 1,
                other => panic!("Stop() 后读写任务必须都退出并报 Dead，实得 {other:?}"),
            }
        }
        assert_eq!(dead, 2, "读/写任务都必须退出");
        assert!(peer.await.unwrap(), "全关后对端必须读到 EOF");
    }

    /// C4：`GateReady` 时只发健康帧并清零计数；GM_STOP 后停发，10min 维护窗口严格 `>`。
    #[test]
    fn gate_ready_sends_and_stop_uses_maintenance_window() {
        let mut st = link_state();
        st.gate_ready = true;
        st.check_server_fail_count = 42;
        assert_eq!(
            connected_state_action(&mut st, 200_000, CHECK_SERVER_TIMEOUT_MS),
            KeepaliveTick::Send
        );
        assert_eq!(st.check_server_fail_count, 0, "就绪态必须清零失败计数");

        let mut stopped = link_state();
        stopped.running = RunningState::Stop;
        assert_eq!(
            connected_state_action(
                &mut stopped,
                1_000 + MAINTENANCE_MS,
                CHECK_SERVER_TIMEOUT_MS
            ),
            KeepaliveTick::Idle,
            "恰好 10min 不得重连"
        );
        assert_eq!(
            connected_state_action(
                &mut stopped,
                1_000 + MAINTENANCE_MS + 1,
                CHECK_SERVER_TIMEOUT_MS
            ),
            KeepaliveTick::Maintenance,
            "10min+1ms 才走维护结束分支"
        );
        assert!(!stopped.gate_ready, "Stop 分支不得发健康帧");
    }

    /// C3：15 天会话超时（`GateShare.SessionTimeOutTime`）与 120s 扫描周期；
    /// 判定是严格 `>`（`ClientThread.cs:418`）。
    #[test]
    fn session_idle_timeout_is_strictly_greater() {
        assert_eq!(SESSION_IDLE_MS, 15 * 24 * 60 * 60 * 1000);
        assert_eq!(SESSION_SWEEP_MS, 120_000);
        assert!(
            !session_idle_expired(SESSION_IDLE_MS, 0, SESSION_IDLE_MS),
            "恰好 15 天不得清理"
        );
        assert!(
            session_idle_expired(SESSION_IDLE_MS + 1, 0, SESSION_IDLE_MS),
            "15 天 +1ms 必须清理"
        );
        assert!(!session_idle_expired(5_000, 1_000, SESSION_IDLE_MS));
        assert!(session_idle_expired(
            1_000 + SESSION_IDLE_MS + 1,
            1_000,
            SESSION_IDLE_MS
        ));
    }

    /// B-4：`GateShare.MaxSession`(=10000) 全满再分配不 panic，且返回最后一个非空槽。
    #[test]
    fn session_slots_full_at_oracle_max_does_not_panic() {
        let mut slots = SessionSlots::new(MAX_SESSION);
        for i in 0..MAX_SESSION {
            let sid = u16::try_from(i % 10_000).unwrap();
            let handle = i32::try_from(i).unwrap();
            assert_eq!(slots.allocate(sid, handle), (handle, None));
        }
        assert_eq!(
            slots.allocate(1, 42),
            (
                i32::try_from(MAX_SESSION - 1).unwrap(),
                Some(u16::try_from(MAX_SESSION - 1).unwrap())
            ),
            "第 10001 个连接返回最后一个槽（9999）的 handle，不 panic"
        );
    }
}
