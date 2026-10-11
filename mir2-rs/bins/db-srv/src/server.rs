//! 5100 面的**连接与状态层**：SelGate 接入、内部帧收发、会话表（C# `UserService`）。
//!
//! 逐条对齐 C# `DBSrv/Services/Impl/UserService.cs`：
//! - 接入先查 `AddrTable.txt`（`CheckServerIP`），非法地址**直接关闭**；
//! - 每个连接占一个槽位（C# `_gateClients[clientId-1]`，数组长度 20），槽位号＝连接号-1；
//! - 帧是 6B `ServerDataPacket` 头 + MemoryPack `ServerDataMessage`（`crates/protocol::internal`）；
//! - `Enter`/`Leave`/`KeepAlive` **在连接回调里就地处理**，只有 `Data` 进单消费者队列
//!   （C# `Channel<UserGateMessage>`）—— 命令处理必须串行，这里用单 worker 保持同一语义；
//! - 断线清空该槽位的用户列表并置空槽位（C# `Disconnected`）。
//!
//! 与 C# 的一处**必要差异**：C# 的连接回调与消息线程会并发读写同一个 `UserList`
//! （无锁数据竞争）；这里用 `Mutex` 串行化，可观测行为不变（同一张用户表、同一处理顺序）。

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use mir2_protocol::frame::ServerDataPacket;
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use mir2_protocol::messages::grobal2::PacketCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

use crate::config::DbConfig;
use crate::gbk::{arrest, split2_nonempty};
use crate::log;
use crate::loginsrv::LoginState;
use crate::route::Share;

/// C# `_gateClients` 数组长度。
pub const MAX_GATES: usize = 20;

/// 一个角色网关连接上的一个客户端会话（C# `SessionUserInfo`）。
#[derive(Debug, Clone)]
pub struct GateUser {
    /// 网关侧会话号（`ServerDataMessage.SocketId`，字符串）。
    pub socket_id: String,
    /// 账号（`QueryChr` 通过会话校验后才填）。
    pub account: Vec<u8>,
    /// 登录会话号（LoginSrv 发的那个）。
    pub session_no: i32,
    /// 累计的帧文本（客户端帧可能跨包，C# `userInfo.sText`）。
    pub text: Vec<u8>,
    /// 本会话是否已查过角色（C# `boChrQueryed`）。
    pub chr_queryed: bool,
    /// 本会话是否已选过角色（C# `boChrSelected`）。
    pub chr_selected: bool,
    /// 最近一次角色操作的时间戳（毫秒，C# `dwChrTick`）。
    pub chr_tick: u64,
    /// 会话建立时间戳（C# `dwTick34`）。
    pub tick34: u64,
    /// 角色网关 ID（`SelectID.txt` 里的值，建角时写进 `SelectID`）。
    pub sel_gate_id: i16,
    /// 玩家 IP（`Enter` 载荷第 2 段）。
    pub user_ip: Vec<u8>,
    /// 网关 IP（`Enter` 载荷第 3 段起；`DynamicIPMode` 时用于回路由）。
    pub gate_ip: Vec<u8>,
}

/// 一个角色网关连接（C# `SelGateInfo`）。
pub struct Gate {
    /// 连接号（C# `client.Id`，从 1 开始）。
    pub id: i32,
    /// 对端 IP（`GateRouteIp` 用它查游戏网关路由）。
    pub remote_ip: String,
    /// 网关 ID（`SelectID.txt`）。
    pub n_gate_id: i16,
    /// 该连接上的客户端会话列表。
    pub users: Vec<GateUser>,
    /// 写端（每个连接一个写任务）。
    pub tx: mpsc::UnboundedSender<Vec<u8>>,
}

/// 进队列的透传数据（C# `UserGateMessage`）。
pub struct Queued {
    /// 槽位（连接号-1）。
    pub slot: usize,
    /// 原始内部帧。
    pub msg: ServerDataMessage,
}

/// 进程级服务状态。
pub struct Server {
    /// 配置。
    pub cfg: DbConfig,
    /// 只读配置表（AddrTable/ServerInfo/MapInfo/SelectID/DenyChrName）。
    pub share: Arc<Share>,
    /// LoginSrv 会话链。
    pub login: Arc<Mutex<LoginState>>,
    /// 槽位表（长度 `MAX_GATES`）。
    pub gates: Arc<Mutex<Vec<Option<Gate>>>>,
    /// 数据队列写端。
    pub queue_tx: mpsc::UnboundedSender<Queued>,
    next_id: AtomicI32,
}

impl Server {
    /// 建状态 + 队列（队列读端交给 `run`）。
    #[must_use]
    pub fn new(cfg: DbConfig, share: Arc<Share>) -> (Arc<Self>, mpsc::UnboundedReceiver<Queued>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut gates = Vec::with_capacity(MAX_GATES);
        for _ in 0..MAX_GATES {
            gates.push(None);
        }
        let server = Arc::new(Self {
            cfg,
            share,
            login: Arc::new(Mutex::new(LoginState::default())),
            gates: Arc::new(Mutex::new(gates)),
            queue_tx: tx,
            next_id: AtomicI32::new(1),
        });
        (server, rx)
    }

    /// 在线会话总数（C# `GetUserCount`，用于上报 LoginSrv）。
    pub async fn user_count(&self) -> usize {
        let gates = self.gates.lock().await;
        gates
            .iter()
            .filter_map(|g| g.as_ref())
            .map(|g| g.users.len())
            .sum()
    }

    /// 监听 5100 并服务（本函数不返回，除非监听失败）。
    ///
    /// # Errors
    /// 绑定失败（端口被占/地址非法）时返回错误（M1④：启动失败要明确报错并给退出码）。
    pub async fn run(
        self: Arc<Self>,
        rx: mpsc::UnboundedReceiver<Queued>,
        db: crate::db::Db,
    ) -> Result<()> {
        let listener = TcpListener::bind((self.cfg.gate_addr.as_str(), self.cfg.gate_port))
            .await
            .with_context(|| {
                format!(
                    "绑定 {}:{} 失败（端口被占？M1④）",
                    self.cfg.gate_addr, self.cfg.gate_port
                )
            })?;
        log::info(&format!(
            "玩家数据网关服务[{}:{}]已启动.等待链接...",
            self.cfg.gate_addr, self.cfg.gate_port
        ));
        tokio::spawn(crate::loginsrv::login_link_task(Arc::clone(&self)));
        tokio::spawn(crate::handler::queue_loop(Arc::clone(&self), rx, db));
        loop {
            let (sock, peer) = listener.accept().await.context("accept 失败")?;
            sock.set_nodelay(true).ok();
            let server = Arc::clone(&self);
            tokio::spawn(async move {
                if let Err(e) = gate_conn(server, sock, peer.to_string()).await {
                    log::warn(&format!("角色网关[{peer}]会话结束: {e:#}"));
                }
            });
        }
    }
}

/// 单个角色网关连接：注册槽位 → 读内部帧 → 处理 Enter/Leave/KeepAlive、转发 Data。
async fn gate_conn(server: Arc<Server>, sock: TcpStream, peer: String) -> Result<()> {
    let ip = peer
        .rsplit_once(':')
        .map_or(peer.as_str(), |(h, _)| h)
        .to_string();
    if !server.share.check_server_ip(&ip) {
        log::warn(&format!("非法服务器连接: {peer}"));
        return Ok(());
    }
    let id = server.next_id.fetch_add(1, Ordering::Relaxed);
    let Ok(slot) = usize::try_from(id - 1) else {
        return Ok(());
    };
    if slot >= MAX_GATES {
        // C# `_gateClients[clientId-1]` 会越界抛异常（连接不被登记）⇒ 这里显式拒绝
        log::error(&format!("角色网关连接数超过 {MAX_GATES}，拒绝 {peer}"));
        return Ok(());
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    {
        let mut gates = server.gates.lock().await;
        gates[slot] = Some(Gate {
            id,
            remote_ip: ip.clone(),
            n_gate_id: server.share.get_gate_id(&ip),
            users: Vec::new(),
            tx,
        });
    }
    log::info(&format!("角色网关[{id}]({peer})已打开..."));
    let (mut rd, mut wr) = sock.into_split();
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
            log::debug("解析角色网关封包出现异常...");
            break;
        };
        if pkt.packet_code != PacketCode {
            log::debug("解析角色网关封包出现异常...");
            continue;
        }
        let mut body = vec![0u8; usize::from(pkt.packet_len)];
        if rd.read_exact(&mut body).await.is_err() {
            break;
        }
        let Ok(msg) = ServerDataMessage::from_bytes(&body) else {
            log::debug("解析角色网关封包出现异常...");
            continue;
        };
        match msg.kind {
            ServerDataType::KeepAlive => {
                // C# `SendKeepAlivePacket`：回一条空的 KeepAlive
                let reply = ServerDataMessage::new(ServerDataType::KeepAlive, "", Vec::new());
                write_gate(&server, slot, &reply.to_wire()).await;
                log::debug("响应角色网关心跳...");
            }
            ServerDataType::Enter => match crate::user::data_len(msg.data_len, msg.data.len()) {
                None => log::debug("Enter 报文 DataLen 越界（C# 会抛异常 ⇒ 丢弃）"),
                Some(n) => {
                    let (between, _) = arrest(&msg.data[..n], b'%', b'$');
                    let (_, sdata) = split2_nonempty(&between, b"/");
                    let socket_id = msg.socket_id.clone();
                    open_user(&server, slot, &socket_id, &sdata).await;
                }
            },
            ServerDataType::Leave => {
                close_user(&server, slot, &msg.socket_id).await;
            }
            ServerDataType::Data => {
                let _ = server.queue_tx.send(Queued { slot, msg });
            }
        }
    }
    {
        let mut gates = server.gates.lock().await;
        if let Some(g) = gates[slot].as_mut() {
            log::info(&format!("角色网关[{}]({})已关闭...", g.id, g.remote_ip));
            g.users.clear();
        }
        gates[slot] = None;
    }
    writer.abort();
    Ok(())
}

/// 把一帧内部报文写到指定槽位（槽位不在即丢弃，同 C# `SendPacket` 的存在性检查）。
pub async fn write_gate(server: &Arc<Server>, slot: usize, wire: &[u8]) {
    let gates = server.gates.lock().await;
    if let Some(Some(g)) = gates.get(slot) {
        let _ = g.tx.send(wire.to_vec());
    }
}

/// C# `OpenUser`：`Enter` 载荷里第 2 段是玩家 IP、第 3 段起是网关 IP（第 1 段被 C# 丢弃）。
async fn open_user(server: &Arc<Server>, slot: usize, socket_id: &str, s_ip: &[u8]) {
    let (user_ip, gate_ip) = split2_nonempty(s_ip, b"/");
    let mut gates = server.gates.lock().await;
    let Some(Some(g)) = gates.get_mut(slot) else {
        return;
    };
    if g.users.iter().any(|u| u.socket_id == socket_id) {
        return;
    }
    let now = crate::handler::now_ms();
    log::debug(&format!(
        "新会话 socket_id={socket_id} user_ip={} gate_ip={} gateId={} tick={now}",
        String::from_utf8_lossy(&user_ip),
        String::from_utf8_lossy(&gate_ip),
        g.n_gate_id
    ));
    g.users.push(GateUser {
        socket_id: socket_id.to_string(),
        account: Vec::new(),
        session_no: 0,
        text: Vec::new(),
        chr_queryed: false,
        chr_selected: false,
        chr_tick: now,
        tick34: now,
        sel_gate_id: g.n_gate_id,
        user_ip,
        gate_ip,
    });
}

/// C# `CloseUser`：未进游戏则通知 LoginSrv 软下线并释放会话，然后摘掉这个客户端。
async fn close_user(server: &Arc<Server>, slot: usize, socket_id: &str) {
    let mut gates = server.gates.lock().await;
    let Some(Some(g)) = gates.get_mut(slot) else {
        return;
    };
    let Some(idx) = g.users.iter().position(|u| u.socket_id == socket_id) else {
        return;
    };
    let u = g.users[idx].clone();
    log::debug(&format!(
        "会话关闭 socket_id={} 存活={}ms",
        u.socket_id,
        crate::handler::now_ms().saturating_sub(u.tick34)
    ));
    let mut login = server.login.lock().await;
    if !login.get_play(u.session_no) {
        login.send_soft_out(&u.account, u.session_no);
        login.close_session(&u.account, u.session_no);
    }
    g.users.remove(idx);
}
