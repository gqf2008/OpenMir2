//! Rust 版 **GameSvr 世界宿主**（M2 主体，切片 S1：进程骨架 + 网关链路）。
//!
//! 这一片只做两件事，并把其余登记清楚（不静默跳过）：
//! 1. **网关↔GameSvr 链路**：同 20B `ServerMessage` 头 + `Grobal2.GM_*`
//!    （口径与 `bins/game-gate` 的协议梳理 §3 一致）——`GM_OPEN` 登记用户并回 `GM_SERVERUSERINDEX`、
//!    `GM_DATA` 解析 12B 命令头（+ 可选的 EDCode 尾）并按 `SessionIndex-1`/socket 路由、
//!    `GM_CLOSE` 按 socket 清槽、`GM_CHECKCLIENT`/`GM_RECEIVE_OK` 维护保活/阻塞计数。
//! 2. **单线程 tick 宿主**：`std::thread` 拥有 `crats/world::World`，按 `TICK_INTERVAL_MS` 调 `World::tick`；
//!    I/O（tokio）侧**只发事件**、绝不碰 `World`（设计文档 §4.2 的四条确定性硬约束）。
//!
//! **本切片不做**（登记在卡内与报告，S2/S3/S4 承接）：
//! - 认证链（`DoClientCertification`）已落 `cert.rs`；**取角色/进世界**（`LoadDBInfo` → DBSrv → PlayObject）——S2b-3；
//! - 会话**进世界**（`World::open_session`/`enter_map`）与命令到世界的映射——S2；
//! - 世界 → 客户端的 **SM_* 出包**（`SendMsgQueue`/编码）——S3；
//! - 真地图/Envir 加载（本片用配置里的占位尺寸建世界）——S2/S3；
//! - 保活定时器（消费者是 **M2Server 自己** 的 `TCPNetChannel.cs:435-439`，本片只维护标志）。
//!
//! **失败要响**：缺配置键、绑不上端口 —— 明确报错并非零退出。

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

mod auth;
mod cert;
mod config;
mod dbclient;
mod hutil;
mod ids;
mod load;
mod record;
mod users;

use std::sync::Arc;

use anyhow::{Context, Result};
use game_svr::smframe;
use mir2_protocol::frame::ServerMessage;
use mir2_protocol::messages::grobal2;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

use users::UserList;

/// C# `CommandMessage.Size`（`src/OpenMir2/Packets/ClientPackets/ClientMesaagePacket.cs:24`）。
const COMMAND_MESSAGE_SIZE: usize = 12;

/// Serilog 同形日志前缀（便于与两侧日志对时间线）。
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

/// sim 侧 → I/O 侧的命令（世界/读档流程要动网关连接持有的槽表时用）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum SimToGate {
    /// `NetChannel.CloseUser(gateIdx, socket)`（`TCPNetChannel.cs:221-224`）的对应物：清掉该 socket 的槽。
    KickUser { socket: i32 },
    /// 进世界完成（`World::open_session` + `enter_map` 成功）⇒ 该槽的 `in_world` 置真
    /// （oracle 里是 `gateUser.PlayObject`/`WorldEngine` 挂上，`:201` 的分支门就是它）。
    MarkInWorld { socket: i32 },
    /// S2b-4 续：世界 → 客户端的一条 `SM_*` 帧（`body` = 12B 命令头 + 尾）。
    /// gate 侧按 oracle `ClientSession.cs:897-920` 变成 `'#' + Encode(头12B) + 尾 + '!'` 发客户端。
    SendFrame { socket: i32, body: Vec<u8> },
}

/// I/O 侧 → sim 侧的事件（sim 线程是唯一碰 `World` 的地方）。
#[derive(Debug)]
enum GateEvent {
    /// 新用户登记（`GM_OPEN`）。
    Opened { index: i32, ip: String },
    /// 客户端命令帧（`GM_DATA` 里是 12B 命令头 + 可选 EDCode 尾）。
    Command {
        index: i32,
        cmd: CommandMessage,
        tail: Option<String>,
    },
    /// 没通过认证的会话收到的帧：oracle 在这一支走 `DoClientCertification` 的空转（`Account` 为空才真认证）。
    CertificationPending { index: i32, ip: String },
    /// `GM_CLOSE`。
    Closed { index: i32 },
    /// 认证通过 ⇒ 取角色（oracle `:323-338` 的 `LoadDBInfo` + `AddToLoadRcdList`）。
    /// 事件直接带 `LoadDbInfo`，交给 sim 线程的 `FrontEngine` 队列。
    LoadDbRequest(load::LoadDbInfo),
    /// 网关连接建好：登记它的 `sim → gate` 命令通道（`service_id` = 该连接的线号 = oracle 的 `GateIdx`）。
    RegisterGate {
        service_id: u8,
        tx: mpsc::UnboundedSender<SimToGate>,
    },
}

/// C# `CommandMessage`（12B 解码态布局，`:11-24`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CommandMessage {
    recog: i32,
    ident: u16,
    param: u16,
    tag: u16,
    series: u16,
}

impl CommandMessage {
    /// 从解码态字节解析（不足 12B 返回 `None`）。
    #[must_use]
    fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < COMMAND_MESSAGE_SIZE {
            return None;
        }
        Some(Self {
            recog: i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ident: u16::from_le_bytes([b[4], b[5]]),
            param: u16::from_le_bytes([b[6], b[7]]),
            tag: u16::from_le_bytes([b[8], b[9]]),
            series: u16::from_le_bytes([b[10], b[11]]),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let conf = std::env::args()
        .nth(1)
        .map_or_else(config::default_path, std::path::PathBuf::from);
    let cfg = config::load(&conf)?;
    glog!(
        "INF",
        "游戏服务启动：监听 {}:{}（CheckBlock={}，{ } 张地图占位，config={}）",
        cfg.gate_addr,
        cfg.gate_port,
        cfg.check_block,
        cfg.maps.len(),
        conf.display()
    );

    // DB 链路（GameSvr → DBSvr）：oracle 的 `DataQueryServer` 也是 TcpClient 主动连（`:25-33`）。
    // 本切片只**发**取角色请求；收/解析属 S2b-2b。
    let db_state = dbclient::DbLinkState::new();
    let db_replies = std::sync::Arc::new(std::sync::Mutex::new(dbclient::DbReplies::new()));
    let (db_req_tx, db_req_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    {
        let db_state = db_state.clone();
        let db_addr = cfg.db_addr.clone();
        let db_port = cfg.db_port;
        let replies = std::sync::Arc::clone(&db_replies);
        tokio::spawn(async move {
            dbclient::run_db_link(db_addr, db_port, db_state, db_req_rx, replies).await;
        });
    }
    // 查询号：`SystemShare.Config.DBQueryID++`（`PlayerDataService.GetQueryId` `:183-191`，从 1 起、溢出回 1）
    let db = DbWiring {
        state: db_state,
        req_tx: db_req_tx,
        next_query_id: std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0)),
        replies: db_replies,
        outstanding: Vec::new(),
    };

    // sim 侧：一个专用线程独占 World（§4.2 第 1 条：不许并行 executor 驱动世界）
    let (ev_tx, ev_rx) = std::sync::mpsc::channel::<GateEvent>();
    // 准入会话表（C# `AuthenticationService._sessionList`）：跨网关连接共享；
    // 填充它的是登录链的通知（S2b 的传输），本切片只实现表语义与认证匹配。
    let auth: Arc<std::sync::Mutex<auth::AdmissionStore>> =
        Arc::new(std::sync::Mutex::new(auth::AdmissionStore::new()));
    let maps = cfg.maps.clone();
    let _sim = std::thread::Builder::new()
        .name("m2-sim".to_string())
        .spawn(move || sim_loop(maps, ev_rx, Some(db)))
        .context("起 sim 线程失败")?;

    // 登录服务器（IDS）链路：oracle 是 TcpClient 主动连它（`AuthenticationService.cs:22-34`），
    // 会话通知（SS_OPENSESSION/SS_CLOSESESSION）填/删准入表 ⇒ 认证链靠它
    {
        let store = Arc::clone(&auth);
        let ids_addr = cfg.ids_addr.clone();
        let ids_port = cfg.ids_port;
        tokio::spawn(async move { ids::run_ids_link(ids_addr, ids_port, store).await });
    }

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
        "游戏服务[{}:{}]已启动...",
        cfg.gate_addr,
        cfg.gate_port
    );
    // 网关（GameGate）以客户端身份连上来；同一进程可接待多条网关连接，但**各自独立的会话表**。
    // sim 线程随进程存活（`join` 不必要：世界只由它驱动，进程退出即结束）。
    // 本进程只监听一个端点：线号取配置里的第一条（oracle 的 `GateInfo.ServiceId`/`GateIdx`）
    let active_service_id: u8 = 0;
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        glog!("INF", "游戏网关连接：{peer}");
        let ev_tx = ev_tx.clone();
        let check_block = cfg.check_block;
        let auth = Arc::clone(&auth);
        let service_id = active_service_id;
        tokio::spawn(async move {
            if let Err(e) = gate_conn(sock, ev_tx, check_block, auth, service_id).await {
                glog!("WRN", "网关连接结束：{e:#}");
            }
        });
    }
}

/// sim 线程：建世界 → 按 `TICK_INTERVAL_MS` tick → 消费 I/O 侧事件。
///
/// 目前把事件**分类记录**（世界侧的会话/命令映射、取角色进世界是 S2b-3），但 tick 循环与"唯一所有者"这条纪律走真路径。
/// sim 侧用到的 DB 接线（没配 DB 时传 `None` ⇒ 读档恒走"取数失败"支）。
#[derive(Debug)]
struct DbWiring {
    state: dbclient::DbLinkState,
    req_tx: mpsc::UnboundedSender<Vec<u8>>,
    next_query_id: std::sync::Arc<std::sync::atomic::AtomicI32>,
    /// 收侧汇总（分帧 + 签名 + 查询表），由 DB 链路任务与 sim 线程共享。
    replies: std::sync::Arc<std::sync::Mutex<dbclient::DbReplies>>,
    /// 已发出、还没取走的 `(query_id, socket, gate_idx)`——socket 用于回复到达后把该槽标进世界，
    /// `gate_idx` 用于把标记/踢人**路由回该用户的网关线**（oracle 每个 GameGate 连接一套 `SessionUser` 表，
    /// `ChannelMessageHandler.cs:318-322` 的 `gateUser` 属该连接；各网关的 socket 号可重复 ⇒ 不能「随便挑一个」）。
    outstanding: Vec<(i32, i32, i32)>,
}

impl DbWiring {
    /// `GetQueryId()`（`PlayerDataService.cs:183-191`）：先自增，再从 1 起、溢出回 1。
    fn next_query_id(&self) -> i32 {
        use std::sync::atomic::Ordering;
        let q = self.next_query_id.fetch_add(1, Ordering::SeqCst) + 1;
        if q > i32::MAX - 1 {
            self.next_query_id.store(1, Ordering::SeqCst);
            return 1;
        }
        q
    }
}

/// `NetChannel` 按 gateIdx 选连接（`TCPNetChannel.cs:221-224`）：取该线号的 `sim → gate` 命令通道。
///
/// 读档流程里的两条 sim→gate 命令（`KickUser`/`MarkInWorld`）**都**走这里 —— 每个 GameGate 连接的
/// `UserList` 是独立的，socket 号在网关之间可重复，因此只能按 `gate_idx` 命中，找不到线号就记 WRN。
fn route_to_gate<'a>(
    gates: &'a std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    gate_idx: i32,
    socket: i32,
    what: &str,
) -> Option<&'a mpsc::UnboundedSender<SimToGate>> {
    match gates.get(&u8::try_from(gate_idx).unwrap_or(0)) {
        Some(tx) => Some(tx),
        None => {
            glog!(
                "WRN",
                "找不到网关线号 {gate_idx}，无法{what} socket={socket}"
            );
            None
        }
    }
}

fn sim_loop(
    maps: Vec<(i32, i32)>,
    ev_rx: std::sync::mpsc::Receiver<GateEvent>,
    mut db: Option<DbWiring>,
) {
    let mut world = mir2_world::World::with_maps(maps);
    // 取角色的加载队列（`FrnEngn` 的双缓冲 + `CharacterDataProcessor` 的读档循环，见 `load.rs`）
    let mut engine = load::FrontEngine::new();
    // `GateIdx -> 该网关连接的 sim→gate 通道`（`NetChannel` 按 gateIdx 选连接，`TCPNetChannel.cs:221-223`）
    let mut gates: std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>> =
        std::collections::HashMap::new();
    glog!(
        "INF",
        "sim 线程就绪：tick={}ms，地图 {} 张",
        mir2_world::TICK_INTERVAL_MS,
        world.maps.len()
    );
    let tick = std::time::Duration::from_millis(
        u64::try_from(mir2_world::TICK_INTERVAL_MS).unwrap_or(200),
    );
    let mut ticks: u64 = 0;
    loop {
        let next = std::time::Instant::now() + tick;
        while let Ok(ev) = ev_rx.try_recv() {
            match ev {
                GateEvent::Opened { index, ip } => {
                    glog!(
                        "DBG",
                        "sim: 新用户登记 index={index} ip={ip}（open_session 属 S2b-3）"
                    );
                }
                GateEvent::Command { index, cmd, tail } => {
                    glog!(
                        "DBG",
                        "sim: index={index} 命令 Recog={} Ident={} Param={} Tag={} Series={} tail={:?}（进世界后才有意义，S2b-3）",
                        cmd.recog,
                        cmd.ident,
                        cmd.param,
                        cmd.tag,
                        cmd.series,
                        tail.as_deref().unwrap_or("")
                    );
                }
                GateEvent::CertificationPending { index, ip } => {
                    glog!(
                        "DBG",
                        "sim: index={index} ip={ip} 收到无 `!` 的帧 ⇒ 认证链返回 Ignored（oracle 的 `if` 不成立）"
                    );
                }
                GateEvent::Closed { index } => {
                    glog!("DBG", "sim: index={index} 关闭（disconnect 属 S2b-3）");
                }
                GateEvent::LoadDbRequest(info) => {
                    glog!(
                        "INF",
                        "sim: 认证通过 ⇒ 取角色进世界 Account:{} ChrName:{} SessionId:{} GateIdx:{} \
                         （读档控制流在 load.rs；DB 请求属 S2b-2b）",
                        info.account,
                        info.chr_name,
                        info.session_id,
                        info.gate_idx
                    );
                    engine.add_to_load_rcd_list(info);
                }
                GateEvent::RegisterGate { service_id, tx } => {
                    gates.insert(service_id, tx);
                    glog!("DBG", "sim: 登记网关线号 {service_id} 的命令通道");
                }
            }
        }
        // 一轮读档：`query` = oracle 的 `QueryCharacterData`（`CharacterDataProcessor.cs:165`）——
        // 已连上 DBSvr ⇒ 发 `DB_LOADHUMANRCD` 请求并返回 query_id；未连上 ⇒ None（照 `IsConnected`，
        // 走 `SendOutConnectMsg`（**oracle 里是"待实现"、不发帧**）+ `CloseUser` 的失败支）。
        // **收/解析**（`LoadPlayerDataPacket` → 取角色 → 进世界）属 S2b-2b 的第二半（已登记）。
        let mut query_fail = false;
        let actions = match &mut db {
            Some(w) => engine.drain_read_storage(|info| {
                if !w.state.is_connected() {
                    return None; // `IsConnected`（`DataQueryServer.cs:60`）
                }
                let qid = w.next_query_id();
                // N1：把该用户的线号一并记下，回复到达后按它路由（见 `route_to_gate`）
                let frame = dbclient::load_human_rcd_request(
                    qid,
                    &info.account,
                    &info.chr_name,
                    &info.ipaddr,
                    info.session_id,
                );
                match w.req_tx.send(frame) {
                    Ok(()) => {
                        w.outstanding.push((qid, info.socket_id, info.gate_idx));
                        Some(qid)
                    }
                    Err(_) => {
                        query_fail = true;
                        None
                    }
                }
            }),
            None => engine.drain_read_storage(|_| None),
        };
        let _ = query_fail;
        for action in actions {
            match action {
                load::LoadAction::RequeueSaving { account, chr_name } => {
                    glog!(
                        "INF",
                        "角色数据保存中,稍后重新读取. Account:{account} ChrName:{chr_name}"
                    );
                }
                load::LoadAction::StartQuery { .. } => {}
                load::LoadAction::KickNoData {
                    gate_idx,
                    socket_id,
                    account,
                    chr_name,
                } => {
                    glog!(
                        "WRN",
                        "读取用户数据失败,踢出用户. Account:{account} ChrName:{chr_name}"
                    );
                    if let Some(tx) = route_to_gate(&gates, gate_idx, socket_id, "踢") {
                        let _ = tx.send(SimToGate::KickUser { socket: socket_id });
                    }
                }
            }
        }
        // 收侧轮询：已发出且应答已到的查询（`GetDataSrvMessage` `:32-48` 命中即移除）
        if let Some(w) = &mut db {
            let replies = std::sync::Arc::clone(&w.replies);
            let mut r = replies.lock().expect("DB 应答表锁被毒化");
            let mut still = Vec::new();
            for (qid, socket, gate_idx) in std::mem::take(&mut w.outstanding) {
                match r.take(qid) {
                    Some(reply) => {
                        let (ident, recog) =
                            dbclient::parse_reply_message(&reply.message).unwrap_or((0, 0));
                        glog!(
                            "INF",
                            "收到角色数据 queryId={qid} Ident={ident} Recog={recog}"
                        );
                        // `PlayerDataService.cs:150`：`nIdent == DBR_LOADHUMANRCD && nRecog == 1 && data.Length > 0`
                        if ident == mir2_protocol::messages::DBR_LOADHUMANRCD && recog == 1 {
                            let payload = mir2_protocol::edcode::decode(&reply.packet);
                            match record::parse_player_data_head(&payload) {
                                Some(head) => {
                                    // 进世界：`World::open_session` + `enter_map`（S2b-3 的骨架；
                                    // 地图名→id 的映射要等真地图加载，本片一律 0 号图并登记）
                                    let _ = world.open_session(&head.chr_name);
                                    let ent = mir2_world::Entity::new(
                                        0,
                                        head.chr_name.clone(),
                                        i32::from(head.cur_x),
                                        i32::from(head.cur_y),
                                    );
                                    match world.enter_map(&head.chr_name, ent, 0) {
                                        Ok(id) => {
                                            glog!(
                                                "INF",
                                                "进世界 ChrName:{} CurMap:{} ({},{}) Dir:{} entity={id}",
                                                head.chr_name,
                                                head.cur_map,
                                                head.cur_x,
                                                head.cur_y,
                                                head.dir
                                            );
                                            // S2b-4 续：进世界后按 oracle `PlayObject.Message.cs:1680-1692`
                                            // 的 `RM_LOGON` 支出入图帧，经**该用户的网关线**下发。
                                            // 输入齐备的现在只有 `SM_NEWMAP`（记录里的坐标 + 地图名）；
                                            // 其余帧的输入依赖真地图（`openmir2-m2-map-loading`）/ 记录字段
                                            // （`openmir2-m2-record-fields`）/ 配置层 ⇒ `EnterWorldInputs` 的
                                            // `Option` 字段留空（登记在卡，绝不猜尾字节）。
                                            // `day_bright` 暂占位 0（`Envir.DayBright()` 属地图层）。
                                            let inputs = smframe::EnterWorldInputs {
                                                actor_id: id,
                                                cur_x: u16::from_ne_bytes(head.cur_x.to_ne_bytes()),
                                                cur_y: u16::from_ne_bytes(head.cur_y.to_ne_bytes()),
                                                dir: head.dir,
                                                map_file_name: Some(
                                                    head.cur_map.as_bytes().to_vec(),
                                                ),
                                                ..smframe::EnterWorldInputs::default()
                                            };
                                            // N1（reviewer-3）：按该用户的 gate_idx 路由回**它自己那条**网关线
                                            // （与上面 KickNoData 的 `route_to_gate` 完全同构）；找不到线号记 WRN，
                                            // 绝不 `gates.values().next()` 静默挑一个。
                                            if let Some(tx) = route_to_gate(
                                                &gates,
                                                gate_idx,
                                                socket,
                                                "标记进世界/下发入图帧",
                                            ) {
                                                let bodies = inputs.frames();
                                                glog!(
                                                    "INF",
                                                    "进世界帧下发：socket={socket} 共 {} 条（输入齐备的）",
                                                    bodies.len()
                                                );
                                                for body in bodies {
                                                    let _ = tx.send(SimToGate::SendFrame {
                                                        socket,
                                                        body,
                                                    });
                                                }
                                                let _ = tx.send(SimToGate::MarkInWorld { socket });
                                            }
                                        }
                                        Err(e) => glog!(
                                            "WRN",
                                            "进世界失败 ChrName:{}：{e:?}",
                                            head.chr_name
                                        ),
                                    }
                                }
                                None => {
                                    glog!("WRN", "角色记录头部解析失败（{} 字节）", payload.len())
                                }
                            }
                        }
                    }
                    None => still.push((qid, socket, gate_idx)),
                }
            }
            w.outstanding = still;
        }
        let rec = world.tick();
        ticks += 1;
        if ticks.is_multiple_of(50) {
            glog!(
                "DBG",
                "sim: tick #{} clock={}ms 处理 {} 项（本地第 {ticks} 次）",
                rec.tick_seq,
                rec.clock_ms,
                rec.processed.len()
            );
        }
        if let Some(d) = next.checked_duration_since(std::time::Instant::now()) {
            std::thread::sleep(d);
        }
    }
}

/// 一条网关连接：20B 头 + body 的收发与 `GM_*` 分派（照 `ChannelMessageHandler.cs:146-230`）。
async fn gate_conn(
    sock: TcpStream,
    ev_tx: std::sync::mpsc::Sender<GateEvent>,
    check_block: usize,
    auth: Arc<std::sync::Mutex<auth::AdmissionStore>>,
    service_id: u8,
) -> Result<()> {
    let (mut rd, wr) = sock.into_split();
    let tx = Arc::new(Mutex::new(wr));
    // sim → gate：世界/读档流程要清某个用户的槽时走这条（`NetChannel.CloseUser(gateIdx, socket)`）
    let (sim_tx, mut sim_rx) = mpsc::unbounded_channel::<SimToGate>();
    let _ = ev_tx.send(GateEvent::RegisterGate {
        service_id,
        tx: sim_tx,
    });
    let mut users = UserList::new();
    let mut state = KeepAlive {
        send_keep_alive: false,
        ..KeepAlive::default()
    };
    loop {
        let mut head = [0u8; ServerMessage::SIZE];
        // sim 侧的命令（清槽等）**不能等下一次入站帧**才处理：oracle 的 `CloseUser` 是调用线程直接清
        // ⇒ 这里用 `select!` 让"下一帧的头"与"sim 命令"任意一个就绪都能推进。
        let head_res = tokio::select! {
            r = rd.read_exact(&mut head) => r,
            Some(cmd) = sim_rx.recv() => {
                if let Err(e) = apply_sim_cmd_async(cmd, &mut users, &tx, &mut state, check_block).await {
                    glog!("WRN", "sim→gate 命令发送失败：{e:#}");
                }
                continue;
            }
        };
        if head_res.is_err() {
            glog!("ERR", "网关连接断开。");
            return Ok(());
        }
        let Some(msg) = ServerMessage::from_bytes(&head) else {
            continue;
        };
        if msg.packet_code != grobal2::PacketCode {
            continue; // `:122-125` 非本协议包直接丢
        }
        // F5（reviewer-2）：`:127-134` 只在 `PackLength > 0` 时当有载荷，否则按 `(null, 0)` 处理
        let body_len = if msg.pack_length > 0 {
            usize::try_from(msg.pack_length).unwrap_or(0)
        } else {
            0
        };
        let mut body = vec![0u8; body_len];
        if body_len > 0 && rd.read_exact(&mut body).await.is_err() {
            glog!("ERR", "读取网关载荷失败。");
            return Ok(());
        }
        dispatch(
            &msg,
            &body,
            &mut users,
            &mut state,
            &tx,
            &ev_tx,
            check_block,
            &auth,
        )
        .await?;
        // 处理与本帧同批到达的 sim 命令（清槽/入图发包等）
        while let Ok(cmd) = sim_rx.try_recv() {
            if let Err(e) = apply_sim_cmd_async(cmd, &mut users, &tx, &mut state, check_block).await
            {
                glog!("WRN", "sim→gate 命令发送失败：{e:#}");
            }
        }
    }
}

/// 施放一条 sim → gate 的**槽操作**（`NetChannel.CloseUser(gateIdx, socket)` 的落地）。
///
/// `SendFrame` 是异步发包，由 [`apply_sim_cmd_async`] 处理；本函数只做纯槽表动作
/// （单测直接调它，不必起 tokio 运行时）。
fn apply_sim_cmd(cmd: SimToGate, users: &mut UserList) {
    match cmd {
        SimToGate::MarkInWorld { socket } => {
            if users.mark_in_world(socket) {
                glog!("INF", "进世界标记：socket={socket}");
            }
        }
        SimToGate::KickUser { socket } => {
            if let Some(index) = users.close_by_socket(socket) {
                glog!(
                    "WRN",
                    "按世界侧要求清槽：socket={socket} index={index} 在线={} 槽={}",
                    users.online(),
                    users.slots()
                );
            }
        }
        // 异步发包，不在纯槽操作里处理。
        SimToGate::SendFrame { .. } => {}
    }
}

/// sim → gate 命令的异步落地：`SendFrame` 按 oracle `PlayObject.Socket.cs:101-116` 拼
/// `GM_DATA`（20B 头 + 12B 命令头 + 尾）并经 [`send_gate_msg`]（= `ProcessBufferSend` 规则）上路；
/// 其余命令走纯槽操作。
async fn apply_sim_cmd_async(
    cmd: SimToGate,
    users: &mut UserList,
    tx: &Arc<Mutex<OwnedWriteHalf>>,
    state: &mut KeepAlive,
    check_block: usize,
) -> Result<()> {
    match cmd {
        SimToGate::SendFrame { socket, body } => {
            // oracle `messageHead.Socket = SocketId` / `SessionId = SocketIdx`（`PlayObject.Socket.cs:104-115`）；
            // 按 socket 找回该槽（`SessionIndex` 在出包方向上 oracle 不设，保持 0）。
            let Some(u) = users.find(0, socket) else {
                glog!(
                    "WRN",
                    "下发 SM_* 帧时 socket={socket} 的槽不存在，丢弃 {} 字节",
                    body.len()
                );
                return Ok(());
            };
            let head = ServerMessage {
                packet_code: grobal2::PacketCode,
                socket: u.socket,
                session_id: u.session_id,
                ident: u16::from(grobal2::GM_DATA),
                session_index: 0,
                pack_length: i32::try_from(body.len()).unwrap_or(0),
            };
            let mut wire = head.to_bytes().to_vec();
            wire.extend_from_slice(&body);
            send_gate_msg(tx, state, check_block, &wire).await
        }
        other => {
            apply_sim_cmd(other, users);
            Ok(())
        }
    }
}

/// 保活/阻塞计数（C# `GateInfo.SendKeepAlive/SendChecked/SendBlockCount/SendCheckTick`）。
#[derive(Debug, Default)]
struct KeepAlive {
    send_keep_alive: bool,
    send_checked: i32,
    send_block_count: usize,
    /// `SendCheckTick`：`send_decision` 读它判超时（S2b-4 后 `SM_*` 入图帧已走生产路径 ⇒
    /// 整套阻塞/确认逻辑不再是纸面代码）。
    send_check_tick: i64,
}

/// `M2Share.SocCheckTimeOut`（`ChannelMessageHandler.cs:60` 比较用的那个静态）。
///
/// **是字面量 50**：`src/M2Server/M2Share.cs:180` 的静态默认值就是 50，而且它**没有**被配置赋值
/// （`ServerConf.cs:60` 只写 `SystemShare.SocCheckTimeOut`，那是另一个静态；现网 `Server.conf:48`
/// `SocCheckTimeOut=50` 恰好也是 50，容易误以为是"从配置读 2000"——原注释抄了 `:60` 行尾那句过时的
/// `// 2 * 1000`，reviewer-2 的 F2 已指出）。
const SOC_CHECK_TIMEOUT_MS: i64 = 50;

/// 当前毫秒（C# 用 `HUtil32.GetTickCount`；这里只有"差值/超时"参与判定）。
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// 发送口的判定结果（`ProcessBufferSend` `:58-90` 的纯函数版，便于单测）。
#[derive(Debug, PartialEq, Eq)]
enum SendDecision {
    /// `SendChecked > 0`（等网关回 `GM_RECEIVE_OK`）：**包括刚超时清零的那一次也压住不发**（`:58-66` 的无条件 `return`）
    Hold,
    /// 累积到阈值 ⇒ 先发一条 `GM_RECEIVE_OK` 再发正文（`:74-83`）
    CheckFirst,
    /// 直接发（`:84-90`）
    Send,
    /// 单包就超阈值 ⇒ 丢弃（`:76-79`）
    Drop,
}

/// 判定的纯函数（副作用只有超时清零，照 `:58-66`）。
fn send_decision(state: &mut KeepAlive, check_block: usize, len: usize, now: i64) -> SendDecision {
    if state.send_checked > 0 {
        if now - state.send_check_tick > SOC_CHECK_TIMEOUT_MS {
            state.send_checked = 0;
            state.send_block_count = 0;
        }
        // 注意：oracle 这里是**无条件 return**（`:65`）——刚清零的那一包也要丢
        return SendDecision::Hold;
    }
    let threshold = check_block.saturating_mul(10);
    if state.send_block_count + len >= threshold {
        if state.send_block_count == 0 && threshold <= len {
            return SendDecision::Drop; // `:76-79`
        }
        return SendDecision::CheckFirst;
    }
    SendDecision::Send
}

/// 网关发送口的 oracle 规则（`ChannelMessageHandler.cs:50-95` 的 `ProcessBufferSend`）：
/// ① `SendChecked > 0`（等网关回 `GM_RECEIVE_OK`）时**不发**；超过 `SocCheckTimeOut`(50ms) 清零后**仍 Hold**
///    （oracle `:58-66` 无条件 `return`，刚清零的那一包也丢）；
/// ② 累积 `SendBlockCount + len >= CheckBlock*10` ⇒ 先发一条 `GM_RECEIVE_OK` 并置 `SendChecked=1`（`:74-83`）；
///    若 `SendBlockCount == 0 && CheckBlock*10 <= len` ⇒ **直接丢**（`:76-79`）；
/// ③ 发送后 `SendBlockCount += len`（`:89`）。
async fn send_gate_msg(
    tx: &Arc<Mutex<OwnedWriteHalf>>,
    state: &mut KeepAlive,
    check_block: usize,
    wire: &[u8],
) -> Result<()> {
    match send_decision(state, check_block, wire.len(), now_ms()) {
        SendDecision::Hold | SendDecision::Drop => return Ok(()),
        SendDecision::CheckFirst => {}
        SendDecision::Send => {}
    }
    if state.send_checked == 0
        && state.send_block_count + wire.len() >= check_block.saturating_mul(10)
    {
        let check = ServerMessage {
            packet_code: grobal2::PacketCode,
            socket: 0,
            session_id: 0,
            ident: u16::from(grobal2::GM_RECEIVE_OK),
            session_index: 0,
            pack_length: 0,
        };
        send_raw(tx, check.to_bytes().as_slice()).await?;
        state.send_checked = 1;
        state.send_check_tick = now_ms();
    }
    send_raw(tx, wire).await?;
    state.send_block_count += wire.len();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn dispatch(
    msg: &ServerMessage,
    body: &[u8],
    users: &mut UserList,
    state: &mut KeepAlive,
    tx: &Arc<Mutex<OwnedWriteHalf>>,
    ev_tx: &std::sync::mpsc::Sender<GateEvent>,
    check_block: usize,
    auth: &Arc<std::sync::Mutex<auth::AdmissionStore>>,
) -> Result<()> {
    match u8::try_from(msg.ident).unwrap_or(0) {
        grobal2::GM_OPEN => {
            // `:154-159`：体 = IP 字符串；登记后回 `GM_SERVERUSERINDEX`（索引 = 下标+1）
            let ip = gbk_decode(body);
            let index = users.open(msg.socket, msg.session_id, ip.clone());
            let reply = ServerMessage {
                packet_code: grobal2::PacketCode,
                socket: msg.socket,
                session_id: msg.session_id,
                ident: u16::from(grobal2::GM_SERVERUSERINDEX),
                session_index: index,
                pack_length: 0,
            };
            // F1（reviewer-2）：oracle 的 `SendNewUserMsg` 是**直发** `socket.Send(data)`（`:461`），
            // 完全不经过 `:50-95` 的 `ProcessBufferSend` ⇒ 这里也必须直发（不计 SendBlockCount、不受 SendChecked 拦截）
            send_raw(tx, reply.to_bytes().as_slice()).await?;
            // 该分支（GM_OPEN）无其它出站载荷；`SM_*` 出包走 `apply_sim_cmd_async` → `send_gate_msg`。
            let _ = (state, check_block);
            glog!(
                "INF",
                "新用户链接... socket={} SessionId={} index={} ip={ip} 在线={} 槽={}",
                msg.socket,
                msg.session_id,
                index,
                users.online(),
                users.slots()
            );
            let _ = ev_tx.send(GateEvent::Opened { index, ip });
        }
        grobal2::GM_CLOSE => {
            // `:160-162`：**按 socket** 关
            if let Some(index) = users.close_by_socket(msg.socket) {
                glog!(
                    "INF",
                    "用户关闭：socket={} index={index} 在线={} 槽={}",
                    msg.socket,
                    users.online(),
                    users.slots()
                );
                let _ = ev_tx.send(GateEvent::Closed { index });
            }
        }
        grobal2::GM_CHECKCLIENT => {
            // `:163-165`：只置标志。**消费者在本进程**：`src/M2Server/Net/TCPNetChannel.cs:435-439`
            // 见 `SendKeepAlive` 就发 `SendCheck(GM_CHECKSERVER)`；网关侧只是反向（`GameGate/Services/ClientThread.cs:444`
            // 发 `GM_CHECKCLIENT`、`:268-271` 收 `GM_CHECKSERVER`）。本切片不做这个定时器（S2b-3 登记）。
            state.send_keep_alive = true;
            glog!("DBG", "GM_CHECKCLIENT：置保活标志（消费者是 M2Server/Net/TCPNetChannel.cs:435-439，属 S2b-3）");
        }
        grobal2::GM_RECEIVE_OK => {
            // `:166-169`：清零阻塞计数
            state.send_checked = 0;
            state.send_block_count = 0;
        }
        grobal2::GM_DATA => {
            let Some(u) = users.find(msg.session_index, msg.socket) else {
                glog!(
                    "WRN",
                    "SessionIndex={} socket={} 的用户不存在，丢弃 {} 字节",
                    msg.session_index,
                    msg.socket,
                    body.len()
                );
                return Ok(());
            };
            let index = u.index;
            let ip = u.ip.clone();
            // `:201-225`：还没进世界的会话走 `DoClientCertification`（认证语义属 S2）；
            // 已进世界的才把 12B 命令头（+ EDCode 尾）当命令处理。
            if !u.in_world {
                // `:201-225` 的分支门是「**挂上 PlayObject/WorldEngine**」= 已进世界，**不是** `Certification`（F3）。
                // 已认证但尚未进世界的窗口里，oracle 会进 `DoClientCertification`，而那里 `:307` 见 `Account` 非空
                // **直接空转** ⇒ 这一帧被丢弃（不是当命令处理）。
                if u.certification {
                    glog!(
                        "DBG",
                        "index={index} 已认证未进世界 ⇒ 照 oracle 空转丢帧（{} 字节）",
                        body.len()
                    );
                    return Ok(());
                }
                // 未认证 ⇒ 认证链（`cert.rs`）
                let outcome = {
                    let store = auth.lock().expect("准入表锁被毒化");
                    cert::do_client_certification(body, &store)
                };
                match outcome {
                    cert::CertOutcome::Ok {
                        cert,
                        session_id,
                        pay_ment,
                    } => {
                        users.certify(msg.socket, &cert.account, &cert.chr_name, session_id);
                        glog!(
                            "INF",
                            "会话验证通过 Account:{} ChrName:{} SessionId:{} PayMent:{} index={index}",
                            cert.account,
                            cert.chr_name,
                            session_id,
                            pay_ment
                        );
                        // `:323-337` 的 `LoadDBInfo`（字段逐个对应；`SocketId` = 客户端句柄、`GSocketIdx` = 网关 SessionId）
                        let _ = ev_tx.send(GateEvent::LoadDbRequest(load::LoadDbInfo {
                            account: cert.account.clone(),
                            chr_name: cert.chr_name.clone(),
                            ipaddr: ip.clone(),
                            session_id,
                            soft_version_date: cert.client_version,
                            pay_ment,
                            pay_mode: 0,
                            socket_id: msg.socket,
                            gate_socket_idx: msg.session_id,
                            gate_idx: cert.gate_id,
                            new_user_tick: now_ms(),
                            play_time: 0,
                        }));
                    }
                    cert::CertOutcome::Failed {
                        account,
                        game_session_id: game_sid,
                    } => {
                        // `:348-353` / `:340-346`：标 *disable* + 关用户（槽位置空；客户端由网关侧后续处理）
                        glog!(
                            "WRN",
                            "会话验证失败.Account:{account} SessionId:{game_sid} SessionId(gate)={} Address:{ip}",
                            msg.session_id
                        );
                        users.close_by_socket(msg.socket);
                        let _ = ev_tx.send(GateEvent::Closed { index });
                    }
                    cert::CertOutcome::Ignored => {
                        let _ = ev_tx.send(GateEvent::CertificationPending { index, ip });
                    }
                }
                return Ok(());
            }
            let Some(cmd) = CommandMessage::from_bytes(body) else {
                glog!("WRN", "index={index} 的帧不足 12B（{}），丢弃", body.len());
                return Ok(());
            };
            // `:211-218`：`nMsgLen == 12` ⇒ 尾为 None；否则 `EDCode.DeCodeString(msgBuff[12..^1])`
            let tail = if body.len() == COMMAND_MESSAGE_SIZE {
                None
            } else {
                Some(mir2_protocol::edcode::decode(
                    &body[COMMAND_MESSAGE_SIZE..body.len() - 1],
                ))
            };
            let tail = tail.map(|bytes| gbk_decode(&bytes));
            let _ = ev_tx.send(GateEvent::Command { index, cmd, tail });
        }
        other => {
            glog!("DBG", "未处理的 GM_{other}（socket={}）", msg.socket);
        }
    }
    Ok(())
}

/// GBK 解码（现网账号/角色名是 GBK 字节；展示层才解码，协议层按原始字节处理）。
fn gbk_decode(bytes: &[u8]) -> String {
    let (s, _, _) = encoding_rs::GBK.decode(bytes);
    s.into_owned()
}

async fn send_raw(tx: &Arc<Mutex<OwnedWriteHalf>>, wire: &[u8]) -> Result<()> {
    let mut w = tx.lock().await;
    w.write_all(wire).await.context("写网关失败")?;
    w.flush().await.ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// F2（reviewer-2）的判据：`ProcessBufferSend` `:58-90` 的四条分支，**含"刚超时清零的那一包也要压住"**。
    #[test]
    fn send_decision_matches_process_buffer_send() {
        let mut st = KeepAlive::default();
        // ① 正常：未到阈值 ⇒ Send
        assert_eq!(send_decision(&mut st, 12288, 20, 0), SendDecision::Send);
        // ② 累积到阈值 ⇒ 先发 check
        st.send_block_count = 122_880 - 10; // CheckBlock*10 = 122880
        assert_eq!(
            send_decision(&mut st, 12288, 20, 0),
            SendDecision::CheckFirst
        );
        // ③ 单包就超阈值（且累计数为 0）⇒ 丢
        let mut st2 = KeepAlive::default();
        assert_eq!(send_decision(&mut st2, 1, 20, 0), SendDecision::Drop);
        // ④ SendChecked>0 且未超时 ⇒ Hold（不发、不清）
        let mut st3 = KeepAlive {
            send_keep_alive: false,
            send_checked: 1,
            send_check_tick: 1000,
            send_block_count: 7,
        };
        assert_eq!(send_decision(&mut st3, 12288, 20, 1040), SendDecision::Hold);
        assert_eq!(st3.send_checked, 1, "未超时不清零");
        // ⑤ SendChecked>0 且**已超时**：清零后仍 Hold（oracle `:58-66` 的无条件 return）
        let mut st4 = KeepAlive {
            send_keep_alive: false,
            send_checked: 1,
            send_check_tick: 1000,
            send_block_count: 7,
        };
        assert_eq!(send_decision(&mut st4, 12288, 20, 1060), SendDecision::Hold);
        assert_eq!(st4.send_checked, 0, "超时清零");
        assert_eq!(st4.send_block_count, 0);
        // 清零后的下一包才正常发
        assert_eq!(send_decision(&mut st4, 12288, 20, 1061), SendDecision::Send);
    }

    /// N1（reviewer-3）的红检：`MarkInWorld` 必须按该用户的 `gate_idx` 路由，不能「随便挑一个网关」。
    ///
    /// 造两条假网关线并**故意让 socket 号重复**：标记应只投到 1 号线，0 号线的同号槽**不得**被标。
    /// 两个 `same_channel` 断言锁死「按线号取通道」——把实现改回 `gates.values().next()`（对同一张表
    /// 两次调用必返回同一条）时，这两个断言至少有一个必红。
    #[test]
    fn mark_in_world_routes_by_gate_idx_not_arbitrary_gate() {
        use std::collections::HashMap;
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let tx0_keep = tx0.clone();
        let tx1_keep = tx1.clone();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);

        // 路由选择：按 gate_idx 取到的必须是**对应那条**
        assert!(
            route_to_gate(&gates, 1, 5, "标记进世界")
                .expect("1 号线存在")
                .same_channel(&tx1_keep),
            "gate_idx=1 必须路由到 1 号线"
        );
        assert!(
            route_to_gate(&gates, 0, 5, "标记进世界")
                .expect("0 号线存在")
                .same_channel(&tx0_keep),
            "gate_idx=0 必须路由到 0 号线"
        );

        // 两条线各自的槽表里都有 **同一个** socket=5（各网关各自分配，号可重复）
        let mut users0 = UserList::new();
        users0.open(5, 111, "a".into());
        let mut users1 = UserList::new();
        users1.open(5, 222, "b".into());

        // 该用户的 gate_idx=1 ⇒ 标记只投 1 号线
        route_to_gate(&gates, 1, 5, "标记进世界")
            .unwrap()
            .send(SimToGate::MarkInWorld { socket: 5 })
            .unwrap();
        assert!(
            matches!(rx0.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "0 号线不得收到该标记"
        );
        let cmd = rx1.try_recv().expect("1 号线应收到该标记");
        apply_sim_cmd(cmd, &mut users1);
        assert!(
            users1.find(1, 5).unwrap().in_world,
            "1 号线的槽应被标 in_world"
        );
        assert!(
            !users0.find(1, 5).unwrap().in_world,
            "0 号线的同号 socket **不得**被标（N1 的错标场景）"
        );
    }

    /// N1 的负向：线号不在表里 ⇒ 不投递（返回 `None`），而不是静默挑一个。
    #[test]
    fn mark_in_world_missing_gate_idx_does_not_pick_arbitrary() {
        use std::collections::HashMap;
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx2, mut rx2) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(2, tx2);
        assert!(
            route_to_gate(&gates, 1, 5, "标记进世界").is_none(),
            "1 号线不在表里 ⇒ 不许挑 0/2 号"
        );
        assert!(matches!(
            rx0.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        assert!(matches!(
            rx2.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }
}
