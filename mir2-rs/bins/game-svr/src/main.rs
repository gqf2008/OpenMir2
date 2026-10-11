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
//! - 登录合成包解析后的**认证语义**（C# `DoClientCertification`：建角/取角色/置 `Certification`）——S2；
//! - 会话**进世界**（`World::open_session`/`enter_map`）与命令到世界的映射——S2；
//! - 世界 → 客户端的 **SM_* 出包**（`SendMsgQueue`/编码）——S3；
//! - 真地图/Envir 加载（本片用配置里的占位尺寸建世界）——S2/S3；
//! - 保活定时器（oracle 的 `SendKeepAlive` 消费者在网关侧，本片只维护标志）。
//!
//! **失败要响**：缺配置键、绑不上端口 —— 明确报错并非零退出。

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

mod auth;
mod cert;
mod config;
mod hutil;
mod users;

use std::sync::Arc;

use anyhow::{Context, Result};
use mir2_protocol::frame::ServerMessage;
use mir2_protocol::messages::grobal2;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use users::UserList;

/// C# `CommandMessage.Size`（`src/OpenMir2/Packets/ClientPackets/ClientMesaagePacket.cs:24`）。
const COMMAND_MESSAGE_SIZE: usize = 12;

/// Serilog 同形日志前缀（便于与两侧日志对时间线）。
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

/// I/O 侧 → sim 侧的事件（sim 线程是唯一碰 `World` 的地方）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum GateEvent {
    /// 新用户登记（`GM_OPEN`）。
    Opened { index: i32, ip: String },
    /// 客户端命令帧（`GM_DATA` 里是 12B 命令头 + 可选 EDCode 尾）。
    Command {
        index: i32,
        cmd: CommandMessage,
        tail: Option<String>,
    },
    /// 还**没进世界**的会话收到的第一帧（oracle 在这一支走 `DoClientCertification`，属 S2）。
    CertificationPending { index: i32, ip: String },
    /// `GM_CLOSE`。
    Closed { index: i32 },
    /// 认证通过 ⇒ 该去取角色并进世界（oracle `:323-338` 的 `LoadDBInfo` + `AddToLoadRcdList`）。
    /// **S2b** 的活：本切片只把事件交到 sim 线程并记录。
    LoadDbRequest {
        index: i32,
        account: String,
        chr_name: String,
        session_id: i32,
        gate_id: i32,
        client_version: i32,
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

    // sim 侧：一个专用线程独占 World（§4.2 第 1 条：不许并行 executor 驱动世界）
    let (ev_tx, ev_rx) = std::sync::mpsc::channel::<GateEvent>();
    // 准入会话表（C# `AuthenticationService._sessionList`）：跨网关连接共享；
    // 填充它的是登录链的通知（S2b 的传输），本切片只实现表语义与认证匹配。
    let auth: Arc<std::sync::Mutex<auth::AdmissionStore>> =
        Arc::new(std::sync::Mutex::new(auth::AdmissionStore::new()));
    let maps = cfg.maps.clone();
    let _sim = std::thread::Builder::new()
        .name("m2-sim".to_string())
        .spawn(move || sim_loop(maps, ev_rx))
        .context("起 sim 线程失败")?;

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
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        glog!("INF", "游戏网关连接：{peer}");
        let ev_tx = ev_tx.clone();
        let check_block = cfg.check_block;
        let auth = Arc::clone(&auth);
        tokio::spawn(async move {
            if let Err(e) = gate_conn(sock, ev_tx, check_block, auth).await {
                glog!("WRN", "网关连接结束：{e:#}");
            }
        });
    }
}

/// sim 线程：建世界 → 按 `TICK_INTERVAL_MS` tick → 消费 I/O 侧事件。
///
/// S1 只把事件**分类记录**（世界侧的会话/命令映射是 S2），但 tick 循环与"唯一所有者"这条纪律现在就走真路径。
fn sim_loop(maps: Vec<(i32, i32)>, ev_rx: std::sync::mpsc::Receiver<GateEvent>) {
    let mut world = mir2_world::World::with_maps(maps);
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
                        "sim: 新用户登记 index={index} ip={ip}（S2 才 open_session）"
                    );
                }
                GateEvent::Command { index, cmd, tail } => {
                    glog!(
                        "DBG",
                        "sim: index={index} 命令 Recog={} Ident={} Param={} Tag={} Series={} tail={:?}（S2 才进世界）",
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
                        "sim: index={index} ip={ip} 收到认证前帧（oracle 走 DoClientCertification，属 S2）"
                    );
                }
                GateEvent::Closed { index } => {
                    glog!("DBG", "sim: index={index} 关闭（S2 才 disconnect）");
                }
                GateEvent::LoadDbRequest {
                    index,
                    account,
                    chr_name,
                    session_id,
                    gate_id,
                    client_version,
                } => {
                    glog!(
                        "INF",
                        "sim: 认证通过 ⇒ 应取角色进世界 Account:{account} ChrName:{chr_name} \
                         SessionId:{session_id} GateIdx:{gate_id} Ver:{client_version} index={index} \
                         （LoadDBInfo/进世界属 S2b）"
                    );
                }
            }
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
) -> Result<()> {
    let (mut rd, wr) = sock.into_split();
    let tx = Arc::new(Mutex::new(wr));
    let mut users = UserList::new();
    let mut state = KeepAlive {
        send_keep_alive: false,
        ..KeepAlive::default()
    };
    loop {
        let mut head = [0u8; ServerMessage::SIZE];
        if rd.read_exact(&mut head).await.is_err() {
            glog!("ERR", "网关连接断开。");
            return Ok(());
        }
        let Some(msg) = ServerMessage::from_bytes(&head) else {
            continue;
        };
        if msg.packet_code != grobal2::PacketCode {
            continue; // `:122-125` 非本协议包直接丢
        }
        let body_len = msg.body_len();
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
    }
}

/// 保活/阻塞计数（C# `GateInfo.SendKeepAlive/SendChecked/SendBlockCount/SendCheckTick`）。
#[derive(Debug, Default)]
struct KeepAlive {
    send_keep_alive: bool,
    send_checked: i32,
    send_block_count: usize,
    send_check_tick: i64,
}

/// `M2Share.SocCheckTimeOut`（现网 2*1000，`ChannelMessageHandler.cs:60`）。
const SOC_CHECK_TIMEOUT_MS: i64 = 2000;

/// 当前毫秒（C# 用 `HUtil32.GetTickCount`；这里只有"差值/超时"参与判定）。
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// 网关发送口的 oracle 规则（`ChannelMessageHandler.cs:50-95` 的 `ProcessBufferSend`）：
/// ① `SendChecked > 0`（等网关回 `GM_RECEIVE_OK`）时**不发**；超过 `SocCheckTimeOut`(2s) 才清零放行（`:58-66`）；
/// ② 累积 `SendBlockCount + len >= CheckBlock*10` ⇒ 先发一条 `GM_RECEIVE_OK` 并置 `SendChecked=1`（`:74-83`）；
///    若 `SendBlockCount == 0 && CheckBlock*10 <= len` ⇒ **直接丢**（`:76-79`）；
/// ③ 发送后 `SendBlockCount += len`（`:89`）。
async fn send_gate_msg(
    tx: &Arc<Mutex<OwnedWriteHalf>>,
    state: &mut KeepAlive,
    check_block: usize,
    wire: &[u8],
) -> Result<()> {
    if state.send_checked > 0 {
        if now_ms() - state.send_check_tick > SOC_CHECK_TIMEOUT_MS {
            state.send_checked = 0;
            state.send_block_count = 0;
        } else {
            return Ok(()); // 网关还没回 GM_RECEIVE_OK，先压住
        }
    }
    let threshold = check_block.saturating_mul(10);
    if state.send_checked == 0 && state.send_block_count + wire.len() >= threshold {
        if state.send_block_count == 0 && threshold <= wire.len() {
            return Ok(()); // 单包就超阈值：oracle 直接丢（`:76-79`）
        }
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
            send_gate_msg(tx, state, check_block, reply.to_bytes().as_slice()).await?;
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
            // `:163-165`：只置标志（消费者是网关侧的定时器，属 S2 取证）
            state.send_keep_alive = true;
            glog!("DBG", "GM_CHECKCLIENT：置保活标志（S2 接定时器）");
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
            if !u.certification {
                // `:201-225`：还没进世界的会话走 `DoClientCertification`（认证链在 `cert.rs`）
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
                        let _ = ev_tx.send(GateEvent::LoadDbRequest {
                            index,
                            account: cert.account,
                            chr_name: cert.chr_name,
                            session_id,
                            gate_id: cert.gate_id,
                            client_version: cert.client_version,
                        });
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
