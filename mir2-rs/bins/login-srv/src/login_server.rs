//! `LoginServer`（5500 监听 + 单消费者分发）与 `ClientSession`（账号登录/建号/改密/
//! 更新资料/找回密码/选服）—— 1:1 对齐 C# `src/LoginSrv/Services/{LoginServer,ClientSession}.cs`。
//!
//! 已登记的偏离：`mir2-rs/tests/parity/whitelist.md` 的 A-13
//! （畸形帧只跳过该条，不让单消费者循环永久停摆）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use mir2_protocol::frame::{CommandMessage, ServerDataPacket};
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use mir2_protocol::messages;
use mir2_shared::hutil32::{self, to_upper_char};
use mir2_storage::AccountRecord;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::config::GateRoute;
use crate::gbk;
use crate::log_line;
use crate::session_server::{get_server_status, is_not_user_full, send_server_msg};
use crate::state::{tick, LoginGateInfo, SessionConnInfo, Share, UserInfo, VERSION_DATE};
use crate::wire;

/// 网关连接号（C# 用 TouchSocket 的 `client.Id`；这里只需唯一）。
static GATE_SEQ: AtomicU64 = AtomicU64::new(1);

/// C# `AccountCreate`/`AccountUpdateUserInfo` 的
/// `(byte)Math.Ceiling((decimal)(UserEntry.Size(148) * 4) / 3)` = 198。
const ACCOUNT_STR_SIZE: usize = 198;

/// `HUtil32.GetTickCount()` 写入 `int` 字段（`ActionTick` 等）时的取值。
#[must_use]
#[allow(clippy::cast_possible_wrap)] // 与 C# 把 TickCount 存进 int 同语义
pub fn tick_as_i32() -> i32 {
    tick() as i32
}

/// `string.Compare(a, b, OrdinalIgnoreCase) == 0` 的等价判定
/// （.NET 序数忽略大小写＝逐码元的简单大小写折叠，非 culture 语义）。
#[must_use]
pub fn ordinal_ignore_case_eq(a: &str, b: &str) -> bool {
    let mut ia = a.chars();
    let mut ib = b.chars();
    loop {
        match (ia.next(), ib.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if to_upper_char(x) == to_upper_char(y) => {}
            _ => return false,
        }
    }
}

/// 监听 5500，接受 LoginGate 连接；每条连接一个读任务，解析出的内部帧送进**唯一**分发通道。
///
/// # Errors
/// accept 失败时返回错误（由调用方决定进程退出）。
pub async fn run_gate_listener(
    share: Arc<Share>,
    tx: mpsc::UnboundedSender<(String, ServerDataMessage)>,
    listener: TcpListener,
) -> Result<()> {
    loop {
        let (sock, peer) = listener.accept().await?;
        sock.set_nodelay(true).ok();
        let ip = peer.ip().to_string();
        let connection_id = format!("gate-{}", GATE_SEQ.fetch_add(1, Ordering::Relaxed));
        let sipaddr = get_gate_public_addr(&share.cfg.routes, &ip);
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        share.gates.lock().await.insert(
            connection_id.clone(),
            LoginGateInfo {
                sipaddr,
                user_list: Vec::new(),
                keepalive_tick: tick(),
                tx: out_tx,
            },
        );
        log_line("INF", &format!("登录网关[{peer}]已链接..."));
        let share_c = Arc::clone(&share);
        let cid = connection_id.clone();
        let reader_tx = tx.clone();
        tokio::spawn(async move {
            let (rd, mut wr) = sock.into_split();
            let writer = tokio::spawn(async move {
                while let Some(buf) = out_rx.recv().await {
                    if wr.write_all(&buf).await.is_err() {
                        break;
                    }
                    wr.flush().await.ok();
                }
            });
            gate_reader(rd, cid.clone(), reader_tx).await;
            share_c.gates.lock().await.remove(&cid);
            writer.abort();
            log_line("WRN", &format!("登录网关[{ip}:{}]断开链接.", peer.port()));
        });
    }
}

/// 单网关连接：`6B 头 + MemoryPack 体` 逐帧读入分发通道。
async fn gate_reader(
    mut rd: OwnedReadHalf,
    connection_id: String,
    tx: mpsc::UnboundedSender<(String, ServerDataMessage)>,
) {
    loop {
        let mut head = [0u8; ServerDataPacket::SIZE];
        if rd.read_exact(&mut head).await.is_err() {
            return;
        }
        let Some(pkt) = ServerDataPacket::from_bytes(&head) else {
            continue;
        };
        let mut body = vec![0u8; usize::from(pkt.packet_len)];
        if rd.read_exact(&mut body).await.is_err() {
            return;
        }
        match ServerDataMessage::from_bytes(&body) {
            Ok(msg) => {
                if tx.send((connection_id.clone(), msg)).is_err() {
                    return;
                }
            }
            Err(e) => log_line("DBG", &format!("解析登录网关封包出现异常...{e:?}")),
        }
    }
}

/// 单消费者分发（C# `LoginServer.Start` 里的 `_messageQueue` 消费循环）。
pub async fn run_dispatch(
    share: Arc<Share>,
    mut rx: mpsc::UnboundedReceiver<(String, ServerDataMessage)>,
) {
    while let Some((connection_id, msg)) = rx.recv().await {
        if let Err(e) = process_message(&share, &connection_id, msg).await {
            log_line("ERR", &format!("[Exception] 处理网关消息失败: {e:#}"));
        }
    }
}

/// 把分发期间对网关条目副本的修改回写到 `_gateInfos`，**只在条目仍在表内时**覆盖。
///
/// 返回 `true` = 已回写；`false` = 条目在分发期间被读任务删除。
///
/// C# 消费者拿的是 `_clientManager.GetSession(...)` 的**同一对象引用**：读任务 EOF 时
/// `Disconnected`（`LoginServer.cs:71-77`）先把条目摘掉，在途消息仍改那个对象，但改的是
/// **已摘下的对象**，随对象丢弃，绝不会把条目挂回表里。Rust 先 clone 再处理，若回写时
/// 无条件 `insert`，就会把已删除的条目"复活"成幽灵（`user_list` 残留、`tx` 写端已无读者）。
/// 存在性判断与覆盖在同一把锁内完成 ⇒ 不存在"判断时在、写入时已被删"的窗口。
fn commit_gate_update(
    gates: &mut HashMap<String, LoginGateInfo>,
    connection_id: &str,
    info: LoginGateInfo,
) -> bool {
    if let Some(slot) = gates.get_mut(connection_id) {
        *slot = info;
        true
    } else {
        false
    }
}

/// 一条网关消息的处理（C# `ProcessGateData` → 队列消费体）。
async fn process_message(
    share: &Arc<Share>,
    connection_id: &str,
    msg: ServerDataMessage,
) -> Result<()> {
    let Some(mut info) = share.gates.lock().await.get(connection_id).cloned() else {
        return Ok(());
    };
    match msg.kind {
        ServerDataType::Data => {
            let kick = process_user_message(share, &mut info, &msg).await?;
            if kick {
                info.user_list.retain(|u| u.sock_index != msg.socket_id);
            }
        }
        ServerDataType::KeepAlive => {
            let packet = ServerDataMessage {
                kind: ServerDataType::KeepAlive,
                socket_id: String::new(),
                data_len: 0,
                data: Vec::new(),
            };
            let _ = info.tx.send(packet.to_wire());
            info.keepalive_tick = tick();
        }
        ServerDataType::Enter => receive_open_user(&mut info, &msg),
        ServerDataType::Leave => receive_close_user(share, &mut info, &msg.socket_id).await,
    }
    // C# `_config.sGateIPaddr = gateInfo.sIPaddr;`（每条消息后刷新；即使条目已被摘掉，
    // C# 仍从那个对象读 `sIPaddr`，这里保持同口径）。
    *share.gate_ipaddr.lock().await = info.sipaddr.clone();
    // 回写必须"条目仍在才覆盖"（见 `commit_gate_update`），不得 `insert` 复活已删条目。
    let _ = commit_gate_update(&mut *share.gates.lock().await, connection_id, info);
    Ok(())
}

/// `LoginServer.ReceiveOpenUser`（含 C# "先更新再无条件 Add" 的重复条目语义）。
fn receive_open_user(info: &mut LoginGateInfo, msg: &ServerDataMessage) {
    let sock_index = msg.socket_id.clone();
    let sipaddr = gbk::decode(&msg.data);
    // C# `LoginServer.cs:189`：`string sGateIPaddr = HUtil32.GetValidStr3(sIPaddr,
    // ref sUserIPaddr, '/')` —— 该函数把**第一段写进 dest**、**返回第二段**
    //（`src/OpenMir2/HUtil32.cs:384-389`）⇒ 第一段 = `sUserIPaddr`、返回值 = `sGateIPaddr`。
    // 当前 LoginGate 的 Enter 体是 `{ip}/{ip}`（两段相同），故本订正不改变行为；
    // 仅把命名与 oracle 对齐（SelGate 的 `%K{sid}/{ip}/{ip}$` 经 DBSrv 的 `ArrestStringEx`
    // 走另一条路径，不经过这里）。
    let (s_user_ipaddr, s_gate_ipaddr) = hutil32::get_valid_str3(&sipaddr, &['/']);
    for user in &mut info.user_list {
        if user.sock_index == sock_index {
            user.user_ipaddr.clone_from(&s_user_ipaddr);
            user.gate_ipaddr.clone_from(&s_gate_ipaddr);
            user.account = String::new();
            user.session_id = 0;
            user.client_tick = tick();
            break;
        }
    }
    log_line("DBG", &format!("Open: {s_user_ipaddr}/{s_gate_ipaddr}"));
    let now = tick();
    info.user_list.push(UserInfo {
        account: String::new(),
        user_ipaddr: s_user_ipaddr,
        gate_ipaddr: s_gate_ipaddr,
        sock_index,
        session_id: 0,
        client_tick: now,
        last_create_account_tick: now,
        last_update_account_tick: now,
        last_update_pwd_tick: now,
        last_get_back_pwd_tick: now,
        ..UserInfo::default()
    });
}

/// `LoginServer.ReceiveCloseUser`。
async fn receive_close_user(share: &Arc<Share>, info: &mut LoginGateInfo, sock_index: &str) {
    if let Some(i) = info
        .user_list
        .iter()
        .position(|u| u.sock_index == sock_index)
    {
        let user = info.user_list[i].clone();
        log_line("DBG", &format!("Close: {}", user.user_ipaddr));
        if !user.sel_server {
            share
                .sessions
                .lock()
                .await
                .delete(&user.account, user.session_id);
        }
        info.user_list.remove(i);
    }
}

/// `ClientSession.ProcessUserMessage`；返回 `true` 表示 C# 的 `KickUser` 路径（调用方移除该会话）。
async fn process_user_message(
    share: &Arc<Share>,
    info: &mut LoginGateInfo,
    msg: &ServerDataMessage,
) -> Result<bool> {
    let Some(req) = wire::parse_client_frame(&msg.data) else {
        // C# 在此 `return`（会让消费者循环永久停摆，A-13）；Rust 侧只丢弃这一条。
        return Ok(false);
    };
    let Some(user_index) = info
        .user_list
        .iter()
        .position(|u| u.sock_index == msg.socket_id)
    else {
        return Ok(false);
    };
    let mut user = info.user_list[user_index].clone();
    let outcome = process_user_msg(share, info, &mut user, &req).await;
    if user_index < info.user_list.len() {
        info.user_list[user_index] = user;
    }
    match outcome {
        Ok(kick) => Ok(kick),
        Err(e) => {
            log_line("ERR", "[Exception] LoginService.DecodeUserData");
            log_line("ERR", &format!("{e:#}"));
            Ok(false)
        }
    }
}

/// `ClientSession.ProcessUserMsg` 的 switch；`Ok(true)` = 踢掉该会话（`KickUser`）。
async fn process_user_msg(
    share: &Arc<Share>,
    info: &LoginGateInfo,
    user: &mut UserInfo,
    req: &wire::ClientRequest,
) -> Result<bool> {
    match req.head.ident {
        messages::CM_SELECTSERVER => {
            if !user.sel_server {
                account_select_server(share, info, user, &req.body_enc).await?;
            }
        }
        messages::CM_PROTOCOL => account_check_protocol(info, user, req.head.recog),
        messages::CM_IDPASSWORD => {
            if user.account.is_empty() {
                account_login(share, info, user, &req.body_enc).await?;
            } else {
                // C# `KickUser`：裸字符串 `%+-<sock>$` 直写内部链路（既有形态），并移除会话
                log_line("DBG", &format!("Kick: {}", user.user_ipaddr));
                let _ = info
                    .tx
                    .send(format!("%+-{}$", user.sock_index).into_bytes());
                return Ok(true);
            }
        }
        messages::CM_ADDNEWUSER => {
            if share.cfg.enable_making_id {
                if tick().wrapping_sub(user.last_create_account_tick) > 1000 {
                    user.last_create_account_tick = tick();
                    account_create(share, info, user, &req.body_enc).await?;
                } else {
                    log_line("WRN", &format!("[超速操作] 创建帐号/{}", user.user_ipaddr));
                }
            } else {
                // Recog=99 暂未开放账号注册
                send_msg(
                    info,
                    user,
                    CommandMessage::make(i32::from(messages::SM_NEWID_FAIL), 99, 0, 0, 0),
                    &[],
                );
            }
        }
        messages::CM_CHANGEPASSWORD => {
            if user.account.is_empty() {
                if tick().wrapping_sub(user.last_update_pwd_tick) > 5000 {
                    user.last_update_pwd_tick = tick();
                    account_change_password(share, info, user, &req.body_enc).await?;
                } else {
                    log_line("WRN", &format!("[超速操作] 修改密码 /{}", user.user_ipaddr));
                }
            } else {
                user.account = String::new();
            }
        }
        messages::CM_UPDATEUSER => {
            if tick().wrapping_sub(user.last_update_account_tick) > 5000 {
                user.last_update_account_tick = tick();
                account_update_user_info(share, info, user, &req.body_enc).await?;
            } else {
                log_line("WRN", &format!("[超速操作] 更新帐号 /{}", user.user_ipaddr));
            }
        }
        messages::CM_GETBACKPASSWORD => {
            if tick().wrapping_sub(user.last_get_back_pwd_tick) > 5000 {
                user.last_get_back_pwd_tick = tick();
                account_get_back_password(share, info, user, &req.body_enc).await?;
            } else {
                log_line("WRN", &format!("[超速操作] 找回密码 /{}", user.user_ipaddr));
            }
        }
        _ => {} // C# switch 无 default：未知号静默忽略
    }
    Ok(false)
}

/// `ClientSession.AccountCheckProtocol`。
fn account_check_protocol(info: &LoginGateInfo, user: &UserInfo, date: i32) {
    let ident = if date < VERSION_DATE {
        messages::SM_CERTIFICATION_FAIL
    } else {
        messages::SM_CERTIFICATION_SUCCESS
    };
    send_msg(
        info,
        user,
        CommandMessage::make(i32::from(ident), 0, 0, 0, 0),
        &[],
    );
}

/// `ClientSession.AccountLogin`。
#[allow(clippy::too_many_lines)] // 1:1 逐分支照搬 C# `AccountLogin`，拆开反而难对读
async fn account_login(
    share: &Arc<Share>,
    info: &LoginGateInfo,
    user: &mut UserInfo,
    body_enc: &[u8],
) -> Result<()> {
    user.seconds = 0;
    let Some(plain) = wire::decode_body(body_enc) else {
        // C# `EDCode.DeCodeString("")` 抛 ArgumentNullException
        anyhow::bail!("EDCode.DeCodeString: 数据包为空（ArgumentNullException）");
    };
    let text = gbk::decode(&plain);
    let (login_id, password) = hutil32::get_valid_str3(&text, &['/']);
    log_line(
        "DBG",
        &format!(
            "[登录] 帐号=[{login_id}] 密码长度={}",
            password.chars().count()
        ),
    );
    let mut n_code = 0;
    let mut need_update = false;
    let mut record: Option<AccountRecord> = None;
    // C# `_accountStorage.Index(sLoginId)` 返回 **Id**（未命中 -1），判据是 `> 0`
    let account_index = share
        .accounts
        .lock()
        .await
        .index(&login_id)
        .filter(|&i| i > 0);
    if let Some(index) = account_index {
        let got = share.accounts.lock().await.get(index).await?;
        match got {
            // C# `GetAccount` 的 INNER JOIN 缺 protection 行 ⇒ accountRecord 仍为 null ⇒
            // 打日志后再 Get 一次并 return（**不回包**）
            None => {
                log_line("ERR", &format!("获取账号{login_id}资料出错"));
                return Ok(());
            }
            Some(mut rec) => {
                if rec.error_count < 5 || tick_as_i32().wrapping_sub(rec.action_tick) > 60_000 {
                    if ordinal_ignore_case_eq(&rec.password, &password) {
                        rec.error_count = 0;
                        if rec.user_name.is_empty() || rec.quiz2.is_empty() {
                            need_update = true;
                        }
                        n_code = 1;
                    } else {
                        rec.error_count += 1;
                        rec.action_tick = tick_as_i32();
                        n_code = -1;
                    }
                } else {
                    n_code = -2;
                    rec.action_tick = tick_as_i32();
                }
                share
                    .accounts
                    .lock()
                    .await
                    .update_login_record(&rec)
                    .await?;
                record = Some(rec);
            }
        }
    }
    if n_code == 1 && share.sessions.lock().await.is_login_account(&login_id) {
        // 顶号：先通知游戏/角色服关闭旧会话，再立即释放账号占用
        session_kick(share, &login_id).await;
        share.sessions.lock().await.delete_by_account(&login_id);
    }
    if need_update {
        // SM_NEEDUPDATE_ACCOUNT：`EncodeMessage(defMsg) + EncodeBuffer(userEntry)`（两段各自编码）
        let buf = user_entry_buffer(record.as_ref().expect("need_update 必伴随记录"));
        let head = CommandMessage::make(i32::from(messages::SM_NEEDUPDATE_ACCOUNT), 0, 0, 0, 0);
        send_response(info, user, head, &[&buf]);
        user.account = login_id;
        user.session_id = share.next_session_id();
        user.sel_server = false;
        return Ok(());
    }
    if n_code == 1 {
        let Some(rec) = record else {
            return Ok(());
        };
        user.account = login_id;
        user.session_id = share.next_session_id();
        user.sel_server = false;
        if rec.pay_model > 1 && rec.play_time > 0 {
            user.pay_cost = true;
            user.seconds = rec.play_time;
            user.pay_mode = rec.pay_model;
            add_cert_user(share, user).await;
            log_line(
                "DBG",
                &format!(
                    "账号[{}] 登陆IP:[{}] 游戏截止时间:[{}]",
                    user.account,
                    user.user_ipaddr,
                    format_second(user.seconds)
                ),
            );
        } else if rec.pay_model == 0 {
            user.pay_cost = false;
        }
        let server_list = get_server_list_info(share).await;
        let head = CommandMessage::make(
            i32::from(messages::SM_PASSOK_SELECTSERVER),
            0,
            0,
            0,
            i32::try_from(share.cfg.server_names.len()).unwrap_or(0),
        );
        send_response(info, user, head, &[&gbk::encode(&server_list)]);
        session_add(
            share,
            &user.account,
            &user.user_ipaddr,
            user.session_id,
            user.pay_cost,
            rec.pay_model > 0,
        )
        .await;
    } else {
        send_msg(
            info,
            user,
            CommandMessage::make(i32::from(messages::SM_PASSWD_FAIL), n_code, 0, 0, 0),
            &[],
        );
    }
    Ok(())
}

/// `ClientSession.SessionKick`。
async fn session_kick(share: &Arc<Share>, login_id: &str) {
    let Some((account, session_id, server_name, kicked)) = share
        .sessions
        .lock()
        .await
        .get_by_account(login_id)
        .map(|c| {
            (
                c.account.clone(),
                c.session_id,
                c.server_name.clone(),
                c.kicked,
            )
        })
    else {
        return;
    };
    if kicked {
        return;
    }
    send_server_msg(
        share,
        messages::SS_CLOSESESSION,
        &server_name,
        &format!("{account}/{session_id}"),
    )
    .await;
    share.sessions.lock().await.mark_kicked(login_id, tick());
}

/// `ClientSession.SessionAdd`。
async fn session_add(
    share: &Arc<Share>,
    account: &str,
    ipaddr: &str,
    session_id: i32,
    pay_cost: bool,
    payment: bool,
) {
    let now = tick();
    share.sessions.lock().await.add(
        session_id,
        SessionConnInfo {
            account: account.to_string(),
            ipaddr: ipaddr.to_string(),
            server_name: String::new(),
            session_id,
            pay_cost,
            is_payment: payment,
            kick_tick: now,
            start_tick: now,
            kicked: false,
        },
    );
}

/// `ClientSession.AddCertUser`（C# 把可用类型算在 `pUser.AvailableType` 上再拷进 `pCert`）。
async fn add_cert_user(share: &Arc<Share>, user: &mut UserInfo) {
    let mut cert = crate::state::CertUser {
        login_id: user.account.clone(),
        addr: user.user_ipaddr.clone(),
        id_hour: user.seconds,
        ip_day: 0,
        id_day: 0,
        ..crate::state::CertUser::default()
    };
    user.available_type = 5;
    if cert.id_hour > 0 {
        user.available_type = 2;
    }
    if cert.ip_hour > 0 {
        user.available_type = 4;
    }
    if cert.ip_day > 0 {
        user.available_type = 3;
    }
    if cert.id_day > 0 {
        user.available_type = 1;
    }
    cert.certification = user.certification;
    cert.open_time = tick();
    cert.available_type = user.available_type;
    cert.closing = false;
    share.cert_list.lock().await.push(cert);
}

/// `ClientSession.GetServerListInfo`（`ServerName/状态/`，状态取 `ServerStatus` 的**枚举名**）。
async fn get_server_list_info(share: &Arc<Share>) -> String {
    let mut out = String::new();
    for name in &share.cfg.server_names {
        if !name.is_empty() {
            out.push_str(name);
            out.push('/');
            out.push_str(get_server_status(share, name).await);
            out.push('/');
        }
    }
    out
}

/// `ClientSession.AccountSelectServer`。
async fn account_select_server(
    share: &Arc<Share>,
    info: &LoginGateInfo,
    user: &mut UserInfo,
    body_enc: &[u8],
) -> Result<()> {
    let Some(plain) = wire::decode_body(body_enc) else {
        anyhow::bail!("EDCode.DeCodeString: 数据包为空（ArgumentNullException）");
    };
    let server_name = gbk::decode(&plain);
    let gate_ipaddr = share.gate_ipaddr.lock().await.clone();
    let is_login = share.sessions.lock().await.is_login_id(user.session_id);
    log_line(
        "DBG",
        &format!(
            "[选服] account=[{}] server=[{server_name}] gateIP=[{gate_ipaddr}] selServer={} isLogin={is_login} routeCount={}",
            user.account,
            user.sel_server,
            share.cfg.routes.len()
        ),
    );
    if !user.account.is_empty() && !server_name.is_empty() && is_login {
        let (mut s_sel_gate_ip, n_sel_gate_port) =
            get_sel_gate_info(share, &server_name, &gate_ipaddr).await;
        if !s_sel_gate_ip.is_empty() && n_sel_gate_port > 0 {
            if share.cfg.dynamic_ip_mode {
                s_sel_gate_ip = user.gate_ipaddr.clone();
            }
            log_line(
                "INF",
                &format!("Server: {server_name}/{gate_ipaddr}-{s_sel_gate_ip}:{n_sel_gate_port}"),
            );
            user.sel_server = true;
            let n_pay_mode = user.pay_mode;
            // C# `IsNotUserFull` 无 try/catch：NRE 直穿到消息级 catch ⇒ 不回包
            if is_not_user_full(share, &server_name).await? {
                share
                    .sessions
                    .lock()
                    .await
                    .update(user.session_id, &server_name, n_pay_mode > 0);
                send_server_msg(
                    share,
                    messages::SS_OPENSESSION,
                    &server_name,
                    &format!(
                        "{}/{}/{}/{}/{}/{}",
                        user.account,
                        user.session_id,
                        i32::from(user.pay_cost),
                        n_pay_mode,
                        user.user_ipaddr,
                        user.seconds
                    ),
                )
                .await;
                let (recog, pay_mode) = if n_pay_mode > 0 {
                    (i32::try_from(user.seconds).unwrap_or(0), n_pay_mode)
                } else {
                    (user.session_id, 0)
                };
                let head = CommandMessage::make(
                    i32::from(messages::SM_SELECTSERVER_OK),
                    recog,
                    0,
                    i32::from(pay_mode),
                    0,
                );
                let body = format!("{s_sel_gate_ip}/{n_sel_gate_port}/{}", user.session_id);
                send_response(info, user, head, &[&gbk::encode(&body)]);
            } else {
                user.sel_server = false;
                share
                    .sessions
                    .lock()
                    .await
                    .delete(&user.account, user.session_id);
                send_msg(
                    info,
                    user,
                    CommandMessage::make(i32::from(messages::SM_STARTFAIL), 0, 0, 0, 0),
                    &[],
                );
            }
        } else {
            log_line(
                "WRN",
                &format!(
                    "[选服失败] 未匹配到服务器网关路由: server=[{server_name}] gateIP=[{gate_ipaddr}]，请检查 AddrTable.txt 中该服务器的 ServerName/Public 字段是否与登录网关地址一致"
                ),
            );
        }
    } else {
        log_line(
            "WRN",
            &format!(
                "[选服失败] 请求被忽略: account=[{}] server=[{server_name}] isLogin={is_login}",
                user.account
            ),
        );
    }
    Ok(())
}

/// `ClientSession.GetSelGateInfo`（含 `nSelIdx` 轮询游标与"C# 从 `nSelIdx+1` 起找"的既有行为）。
async fn get_sel_gate_info(share: &Arc<Share>, server_name: &str, ipaddr: &str) -> (String, u16) {
    let mut s_sel_gate_ip = String::new();
    let mut n_sel_gate_port = 0u16;
    let mut sel_idx = share.sel_idx.lock().await;
    for (i, route) in share.cfg.routes.iter().enumerate() {
        if share.cfg.dynamic_ip_mode
            || (route.server_name == server_name && route.public_addr == ipaddr)
        {
            let n_gate_count = route
                .gates
                .iter()
                .filter(|g| !g.ipaddr.is_empty() && g.enable)
                .count();
            if n_gate_count == 0 {
                break;
            }
            let cur = sel_idx.get(i).copied().unwrap_or(0);
            let mut selected = false;
            for n in (cur + 1)..=9 {
                if !route.gates[n].ipaddr.is_empty() && route.gates[n].enable {
                    sel_idx[i] = n;
                    selected = true;
                    break;
                }
            }
            if !selected {
                for n in 0..cur {
                    if !route.gates[n].ipaddr.is_empty() && route.gates[n].enable {
                        sel_idx[i] = n;
                        break;
                    }
                }
            }
            let n = sel_idx[i];
            s_sel_gate_ip = route.gates[n].ipaddr.clone();
            n_sel_gate_port = route.gates[n].port;
            break;
        }
    }
    (s_sel_gate_ip, n_sel_gate_port)
}

/// `LsShare.GetGatePublicAddr`。
#[must_use]
pub fn get_gate_public_addr(routes: &[GateRoute], gate_ip: &str) -> String {
    for route in routes {
        if route.remote_addr == gate_ip {
            return route.public_addr.clone();
        }
    }
    gate_ip.to_string()
}

/// `ClientSession.AccountCreate`。
async fn account_create(
    share: &Arc<Share>,
    info: &LoginGateInfo,
    user: &UserInfo,
    body_enc: &[u8],
) -> Result<()> {
    if body_enc.is_empty() {
        log_line("WRN", "[新建账号失败] 数据包为空或数据包长度异常");
        return Ok(());
    }
    if body_enc.len() <= ACCOUNT_STR_SIZE {
        return Ok(());
    }
    let mut account_buff = mir2_protocol::edcode::decode(&body_enc[..ACCOUNT_STR_SIZE]);
    account_buff.extend_from_slice(&mir2_protocol::edcode::decode(
        &body_enc[ACCOUNT_STR_SIZE..],
    ));
    let Some(packet) = parse_user_account_packet(&account_buff) else {
        log_line("WRN", "[新建账号失败] 解析封包出现异常.");
        return Ok(());
    };
    let success = crate::state::verify_account_rule(&packet.account)?;
    let mut n_err_code = -1;
    if success {
        let mut accounts = share.accounts.lock().await;
        let n10 = accounts.index(&packet.account).unwrap_or(-1);
        if n10 <= 0 {
            if !packet.account.is_empty() {
                let rec = record_from_packet(&packet);
                if accounts.add(&rec).await? {
                    n_err_code = 1;
                }
            }
        } else {
            n_err_code = 0;
        }
    } else {
        log_line(
            "WRN",
            &format!("[新建帐号失败] {}/{}", packet.account, packet.quiz2),
        );
    }
    let (ident, recog) = if n_err_code == 1 {
        (messages::SM_NEWID_SUCCESS, 0)
    } else {
        (messages::SM_NEWID_FAIL, n_err_code)
    };
    send_msg(
        info,
        user,
        CommandMessage::make(i32::from(ident), recog, 0, 0, 0),
        &[],
    );
    Ok(())
}

/// `ClientSession.AccountChangePassword`。
async fn account_change_password(
    share: &Arc<Share>,
    info: &LoginGateInfo,
    user: &UserInfo,
    body_enc: &[u8],
) -> Result<()> {
    let Some(plain) = wire::decode_body(body_enc) else {
        anyhow::bail!("EDCode.DeCodeString: 数据包为空（ArgumentNullException）");
    };
    let msg = gbk::decode(&plain);
    // C# `new[] { "\09", "\t" }` ⇒ 字符串重载取 [0]，分隔符是 {'\0','\t'}
    let divs = ['\0', '\t'];
    let (login_id, rest) = hutil32::get_valid_str3(&msg, &divs);
    let (old_password, new_password) = hutil32::get_valid_str3(&rest, &divs);
    let mut n_code = 0;
    if new_password.chars().count() >= 3 {
        let accounts = share.accounts.lock().await;
        let n10 = accounts.index(&login_id).unwrap_or(-1);
        if n10 >= 0 {
            if let Some(mut rec) = accounts.get(n10).await? {
                if rec.error_count < 5 || tick_as_i32().wrapping_sub(rec.action_tick) > 180_000 {
                    if ordinal_ignore_case_eq(&rec.password, &old_password) {
                        n_code = i32::from(accounts.change_password(n10, &new_password).await?);
                    } else {
                        rec.error_count += 1;
                        rec.action_tick = tick_as_i32();
                        n_code = -1;
                    }
                } else {
                    n_code = -2;
                    if tick_as_i32() < rec.action_tick {
                        rec.action_tick = tick_as_i32();
                        accounts.update(n10, &rec).await?;
                    }
                }
            }
        }
    }
    let (ident, recog) = if n_code == 1 {
        (messages::SM_CHGPASSWD_SUCCESS, 0)
    } else {
        (messages::SM_CHGPASSWD_FAIL, n_code)
    };
    send_msg(
        info,
        user,
        CommandMessage::make(i32::from(ident), recog, 0, 0, 0),
        &[],
    );
    Ok(())
}

/// `ClientSession.AccountUpdateUserInfo`。
async fn account_update_user_info(
    share: &Arc<Share>,
    info: &LoginGateInfo,
    user: &UserInfo,
    body_enc: &[u8],
) -> Result<()> {
    if body_enc.is_empty() {
        log_line("WRN", "[更新账号失败] 数据包为空或数据包长度异常");
        return Ok(());
    }
    if body_enc.len() <= ACCOUNT_STR_SIZE {
        return Ok(());
    }
    let mut account_buff = mir2_protocol::edcode::decode(&body_enc[..ACCOUNT_STR_SIZE]);
    account_buff.extend_from_slice(&mir2_protocol::edcode::decode(
        &body_enc[ACCOUNT_STR_SIZE..],
    ));
    let Some(packet) = parse_user_account_packet(&account_buff) else {
        anyhow::bail!("[Exception] LoginService.UpdateUserInfo: 解析封包出现异常");
    };
    let mut n_code = -1;
    let rule_ok = crate::state::verify_account_rule(&packet.account).unwrap_or(false);
    if ordinal_ignore_case_eq(&user.account, &packet.account) && rule_ok {
        let accounts = share.accounts.lock().await;
        let account_index = accounts.index(&packet.account).unwrap_or(-1);
        if account_index >= 0 {
            if let Some(mut rec) = accounts.get(account_index).await? {
                apply_packet_to_record(&mut rec, &packet);
                n_code = i32::from(accounts.update_account(account_index, &rec).await?);
            }
        } else {
            n_code = 0;
        }
    }
    let (ident, recog) = if n_code == 1 {
        (messages::SM_UPDATEID_SUCCESS, 0)
    } else {
        (messages::SM_UPDATEID_FAIL, n_code)
    };
    send_msg(
        info,
        user,
        CommandMessage::make(i32::from(ident), recog, 0, 0, 0),
        &[],
    );
    Ok(())
}

/// `ClientSession.AccountGetBackPassword`。
///
/// C# 的 `GetValidStr3(sMsg, ref x, "\09")` 走**字符串**分隔符重载 ⇒ 分隔符是 `'\0'` 与 `'9'`
/// （`"\09"` 是 `\0` + `'9'` 两字符串）。又因 `GetAccount` 不读 `Quiz/Answer/BirthDay`，
/// 这几个字段恒为空串 —— 本函数按同一口径比较。
async fn account_get_back_password(
    share: &Arc<Share>,
    info: &LoginGateInfo,
    user: &UserInfo,
    body_enc: &[u8],
) -> Result<()> {
    let Some(plain) = wire::decode_body(body_enc) else {
        anyhow::bail!("EDCode.DeCodeString: 数据包为空（ArgumentNullException）");
    };
    let mut msg = gbk::decode(&plain);
    let divs = ['\0', '9'];
    let (account, rest) = hutil32::get_valid_str3(&msg, &divs);
    msg = rest;
    let (quest1, rest) = hutil32::get_valid_str3(&msg, &divs);
    msg = rest;
    let (answer1, rest) = hutil32::get_valid_str3(&msg, &divs);
    msg = rest;
    let (quest2, rest) = hutil32::get_valid_str3(&msg, &divs);
    msg = rest;
    let (answer2, rest) = hutil32::get_valid_str3(&msg, &divs);
    msg = rest;
    let (birthday, _) = hutil32::get_valid_str3(&msg, &divs);
    let mut n_code = 0;
    let mut password = String::new();
    if !account.is_empty() {
        let accounts = share.accounts.lock().await;
        let n_index = accounts.index(&account).unwrap_or(-1);
        if n_index >= 0 {
            if let Some(mut rec) = accounts.get(n_index).await? {
                if rec.error_count < 5 || tick_as_i32().wrapping_sub(rec.action_tick) > 180_000 {
                    n_code = -1;
                    // C# 里 `UserEntry.Quiz` 恒为空串 ⇒ 只有空题面才算命中
                    if quest1.is_empty() {
                        n_code = -3;
                        if answer1.is_empty() && birthday.is_empty() {
                            n_code = 1;
                        }
                    }
                    if n_code != 1 && rec.quiz2 == quest2 {
                        n_code = -3;
                        if answer2.is_empty() && birthday.is_empty() {
                            n_code = 1;
                        }
                    }
                    if n_code == 1 {
                        password = rec.password.clone();
                    } else {
                        rec.error_count += 1;
                        rec.action_tick = tick_as_i32();
                        accounts.update(n_index, &rec).await?;
                    }
                } else {
                    n_code = -2;
                    if tick_as_i32() < rec.action_tick {
                        rec.action_tick = tick_as_i32();
                        accounts.update(n_index, &rec).await?;
                    }
                }
            }
        }
    }
    if n_code == 1 {
        let head = CommandMessage::make(i32::from(messages::SM_GETBACKPASSWD_SUCCESS), 0, 0, 0, 0);
        send_response(info, user, head, &[&gbk::encode(&password)]);
    } else {
        send_msg(
            info,
            user,
            CommandMessage::make(i32::from(messages::SM_GETBACKPASSWD_FAIL), n_code, 0, 0, 0),
            &[],
        );
    }
    Ok(())
}

/// `AccountCreate`/`UpdateUserInfo` 共用的封包解析（`ClientPacket.ToPacket<UserAccountPacket>`）。
struct UserAccountPacket {
    account: String,
    password: String,
    user_name: String,
    ss_no: String,
    quiz: String,
    answer: String,
    email: String,
    quiz2: String,
    answer2: String,
    birthday: String,
    mobile_phone: String,
}

/// 把新建账号封包映射成待写库记录（C# `accountRecord.UserEntry = userAccount.UserEntry`
/// + `UserEntryAdd`，见 `ClientSession.AccountCreate`）。
fn record_from_packet(packet: &UserAccountPacket) -> AccountRecord {
    AccountRecord {
        account: packet.account.clone(),
        password: packet.password.clone(),
        user_name: packet.user_name.clone(),
        quiz2: packet.quiz2.clone(),
        id_card: packet.ss_no.clone(),
        birthday: packet.birthday.clone(),
        email: packet.email.clone(),
        quiz: packet.quiz.clone(),
        answer: packet.answer.clone(),
        mobile_phone: packet.mobile_phone.clone(),
        answer2: packet.answer2.clone(),
        ..AccountRecord::default()
    }
}

/// 把改资料封包逐字段覆盖到已有记录（C# `accountRecord.UserEntry/UserEntryAdd = userAccount.*`，
/// `ClientSession.AccountUpdateUserInfo`）。
fn apply_packet_to_record(rec: &mut AccountRecord, packet: &UserAccountPacket) {
    rec.user_name.clone_from(&packet.user_name);
    rec.quiz2.clone_from(&packet.quiz2);
    rec.id_card.clone_from(&packet.ss_no);
    rec.birthday.clone_from(&packet.birthday);
    rec.email.clone_from(&packet.email);
    rec.quiz.clone_from(&packet.quiz);
    rec.answer.clone_from(&packet.answer);
    rec.mobile_phone.clone_from(&packet.mobile_phone);
    rec.answer2.clone_from(&packet.answer2);
}

/// `BinaryReader` + `ReadPascalString(size)`：读长度字节，再读 `max(size, len)` 字节，取前 `len` 字节。
fn parse_user_account_packet(buf: &[u8]) -> Option<UserAccountPacket> {
    let mut cur = 0usize;
    let mut take = |size: usize| -> Option<String> {
        let len = *buf.get(cur)? as usize;
        let size = size.max(len);
        let start = cur + 1;
        let end = start.checked_add(size)?;
        let bytes = buf.get(start..end)?;
        cur = end;
        Some(gbk::decode(bytes.get(..len)?))
    };
    let account = take(10)?;
    let password = take(10)?;
    let user_name = take(20)?;
    let ss_no = take(14)?;
    let _phone = take(14)?;
    let quiz = take(20)?;
    let answer = take(12)?;
    let email = take(40)?;
    let quiz2 = take(20)?;
    let answer2 = take(12)?;
    let birthday = take(10)?;
    let mobile_phone = take(13)?;
    let _memo = take(20)?;
    let _memo2 = take(20)?;
    Some(UserAccountPacket {
        account,
        password,
        user_name,
        ss_no,
        quiz,
        answer,
        email,
        quiz2,
        answer2,
        birthday,
        mobile_phone,
    })
}

/// `BinaryWriterExtension.WriteAsciiString(value, defaultSize)`。
fn write_ascii_string(out: &mut Vec<u8>, value: &str, size: usize) {
    let mut buffer = if value.is_empty() {
        vec![0u8; size]
    } else {
        gbk::encode(value)
    };
    if buffer.len() > size {
        buffer.truncate(size);
        out.push(u8::try_from(size).unwrap_or(u8::MAX));
    } else {
        // C#：`string.IsNullOrEmpty(value)` ⇒ 长度字节写 0（不是 defaultSize）
        out.push(if value.is_empty() {
            0
        } else {
            u8::try_from(buffer.len()).unwrap_or(0)
        });
        buffer.resize(size, 0);
    }
    out.extend_from_slice(&buffer);
}

/// `UserEntry.GetBuffer()`（148 字节；`GetAccount` 只填 Account/Password/UserName）。
fn user_entry_buffer(rec: &AccountRecord) -> Vec<u8> {
    let mut out = Vec::with_capacity(148);
    write_ascii_string(&mut out, &rec.account, 10);
    write_ascii_string(&mut out, &rec.password, 10);
    write_ascii_string(&mut out, &rec.user_name, 20);
    write_ascii_string(&mut out, "", 14); // SSNo
    write_ascii_string(&mut out, "", 14); // Phone
    write_ascii_string(&mut out, "", 20); // Quiz
    write_ascii_string(&mut out, "", 12); // Answer
    write_ascii_string(&mut out, "", 40); // EMail
    out
}

/// `ClientSession.SendGateMsg`（`"#" + sMsg + "!$"`，帧尾 `!$`）。
fn send_response(info: &LoginGateInfo, user: &UserInfo, head: CommandMessage, segments: &[&[u8]]) {
    let frame = wire::build_response(head, segments);
    let packet = ServerDataMessage {
        kind: ServerDataType::Data,
        socket_id: user.sock_index.clone(),
        data_len: i16::try_from(frame.len()).unwrap_or(i16::MAX),
        data: frame,
    };
    if info.tx.send(packet.to_wire()).is_err() {
        log_line("ERR", "登陆网关链接断开，消息发送失败");
    }
}

/// `ClientSession.SendGateMsg` 的简写：`MakeMessage` 造头 + 文本体段。
fn send_msg(info: &LoginGateInfo, user: &UserInfo, cmd: CommandMessage, segments: &[&[u8]]) {
    send_response(info, user, cmd, segments);
}

/// `ClientSession.FormatSecond`。
#[must_use]
pub fn format_second(second: i64) -> String {
    let days = second / 86400;
    let hours = (second % 86400) / 3600;
    let minutes = ((second % 86400) % 3600) / 60;
    let seconds = ((second % 86400) % 3600) % 60;
    format!("{days}天{hours}小时{minutes}分钟{seconds}秒")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TGateNet;
    use crate::session_server::ServerHandle;
    use crate::state::ServerSessionInfo;
    use std::sync::atomic::AtomicBool;

    fn srv(id: &str, name: &str, index: i32, online: i32) -> ServerSessionInfo {
        let (tx, _rx) = mpsc::unbounded_channel();
        ServerSessionInfo {
            socket_id: id.into(),
            tx,
            endpoint: "127.0.0.1:1".into(),
            server_name: name.into(),
            server_index: index,
            online_count: online,
            keepalive_tick: 0,
            payment_mode: 0,
            session_list: Vec::new(),
            receive_msg: String::new(),
            handle: ServerHandle {
                notify: std::sync::Arc::new(tokio::sync::Notify::new()),
                closed: std::sync::Arc::new(AtomicBool::new(false)),
            },
        }
    }

    /// `OrdinalIgnoreCase` 形状（ASCII 折叠、长度不等即假）。
    #[test]
    fn ordinal_ignore_case_matches_dotnet_shape() {
        assert!(ordinal_ignore_case_eq("mir2pass", "MIR2PASS"));
        assert!(ordinal_ignore_case_eq("", ""));
        assert!(ordinal_ignore_case_eq("中文", "中文"));
        assert!(!ordinal_ignore_case_eq("abc", "abcd"));
        assert!(!ordinal_ignore_case_eq("pass", "pas"));
    }

    /// `WriteAsciiString` 的三个分支（截断 / 填充 / 空串）。
    #[test]
    fn write_ascii_string_layout() {
        let mut out = Vec::new();
        write_ascii_string(&mut out, "abc", 10);
        assert_eq!(out.len(), 11);
        assert_eq!(out[0], 3);
        assert_eq!(&out[1..4], b"abc");
        assert!(out[4..].iter().all(|&b| b == 0));

        let mut out = Vec::new();
        write_ascii_string(&mut out, "", 14);
        assert_eq!(out.len(), 15);
        assert_eq!(out[0], 0);

        let mut out = Vec::new();
        write_ascii_string(&mut out, "0123456789ABCDEF", 10);
        assert_eq!(out.len(), 11);
        assert_eq!(out[0], 10);
        assert_eq!(&out[1..], b"0123456789");
    }

    /// `UserEntry.GetBuffer()` 必须正好 148 字节（C# `UserEntry.Size`）。
    #[test]
    fn user_entry_buffer_is_148_bytes() {
        let rec = AccountRecord {
            account: "mir2test".into(),
            password: "mir2pass".into(),
            user_name: "mir2test".into(),
            quiz2: "q2".into(),
            ..AccountRecord::default()
        };
        let buf = user_entry_buffer(&rec);
        assert_eq!(buf.len(), 148);
        assert_eq!(buf[0], 8);
        assert_eq!(&buf[1..9], b"mir2test");
        assert_eq!(buf[11], 8);
        assert_eq!(&buf[12..20], b"mir2pass");
        // UserName 段：偏移 22 是长度字节，23..31 是内容
        assert_eq!(buf[22], 8);
        assert_eq!(&buf[23..31], b"mir2test");
    }

    /// `ToPacket<UserAccountPacket>`：148B（UserEntry）+ 101B（UserEntryAdd）→ 取 5 个字段。
    #[test]
    fn parses_user_account_packet() {
        let mut buf = Vec::new();
        write_ascii_string(&mut buf, "newacct", 10);
        write_ascii_string(&mut buf, "pwd123", 10);
        write_ascii_string(&mut buf, "nickname", 20);
        write_ascii_string(&mut buf, "ssn1", 14); // SSNo
        write_ascii_string(&mut buf, "", 14); // Phone（oracle SQL 不取该字段）
        write_ascii_string(&mut buf, "q1", 20); // Quiz
        write_ascii_string(&mut buf, "a1", 12); // Answer
        write_ascii_string(&mut buf, "e@x", 40); // EMail
        assert_eq!(buf.len(), 148);
        write_ascii_string(&mut buf, "q2", 20);
        write_ascii_string(&mut buf, "a2", 12);
        write_ascii_string(&mut buf, "1970-01-01", 10);
        write_ascii_string(&mut buf, "13800000000", 13);
        write_ascii_string(&mut buf, "", 20);
        write_ascii_string(&mut buf, "", 20);
        let p = parse_user_account_packet(&buf).expect("应能解析");
        assert_eq!(p.account, "newacct");
        assert_eq!(p.password, "pwd123");
        assert_eq!(p.user_name, "nickname");
        assert_eq!(p.ss_no, "ssn1");
        assert_eq!(p.quiz, "q1");
        assert_eq!(p.answer, "a1");
        assert_eq!(p.email, "e@x");
        assert_eq!(p.quiz2, "q2");
        assert_eq!(p.answer2, "a2");
        assert_eq!(p.birthday, "1970-01-01");
        assert_eq!(p.mobile_phone, "13800000000");
        // 缓冲不足 ⇒ C# 抛 EndOfStream 被 ToPacket 吞掉返回 null ⇒ Rust 返回 None
        assert!(parse_user_account_packet(&buf[..30]).is_none());
        // 只到 UserEntry（148B）也不够：C# 还要读 UserEntryAdd 的 6 段
        assert!(parse_user_account_packet(&buf[..148]).is_none());
        // 少了 Memo2 最后一段同样失败
        assert!(parse_user_account_packet(&buf[..buf.len() - 1]).is_none());
    }

    /// 解析结果 → 建号记录：protection 13 列的数据源必须全部落到 `AccountRecord`
    /// （C# `AccountCreate` 里 `accountRecord.UserEntry/UserEntryAdd = userAccount.*`）。
    #[test]
    fn record_from_packet_fills_all_protection_fields() {
        let packet = UserAccountPacket {
            account: "acct".into(),
            password: "pwd".into(),
            user_name: "uname".into(),
            ss_no: "idcard".into(),
            quiz: "q1".into(),
            answer: "a1".into(),
            email: "e@x".into(),
            quiz2: "q2".into(),
            answer2: "a2".into(),
            birthday: "1980-01-02".into(),
            mobile_phone: "13800000000".into(),
        };
        let rec = record_from_packet(&packet);
        assert_eq!(rec.user_name, "uname");
        assert_eq!(rec.id_card, "idcard");
        assert_eq!(rec.birthday, "1980-01-02");
        assert_eq!(rec.email, "e@x");
        assert_eq!(rec.quiz, "q1");
        assert_eq!(rec.answer, "a1");
        assert_eq!(rec.quiz2, "q2");
        assert_eq!(rec.mobile_phone, "13800000000");
        assert_eq!(rec.answer2, "a2");
    }

    /// 改资料：封包必须**覆盖**（不是保留旧值）全部 protection 字段。
    #[test]
    fn apply_packet_overwrites_all_protection_fields() {
        let mut rec = AccountRecord {
            user_name: "old".into(),
            quiz2: "oldq2".into(),
            id_card: "oldid".into(),
            birthday: "oldbd".into(),
            email: "olde".into(),
            quiz: "oldq1".into(),
            answer: "olda1".into(),
            mobile_phone: "oldm".into(),
            answer2: "olda2".into(),
            ..AccountRecord::default()
        };
        let packet = UserAccountPacket {
            account: "acct".into(),
            password: "pwd".into(),
            user_name: "newun".into(),
            ss_no: "newid".into(),
            quiz: "newq1".into(),
            answer: "newa1".into(),
            email: "newe".into(),
            quiz2: "newq2".into(),
            answer2: "newa2".into(),
            birthday: "newbd".into(),
            mobile_phone: "newm".into(),
        };
        apply_packet_to_record(&mut rec, &packet);
        assert_eq!(rec.user_name, "newun");
        assert_eq!(rec.id_card, "newid");
        assert_eq!(rec.birthday, "newbd");
        assert_eq!(rec.email, "newe");
        assert_eq!(rec.quiz, "newq1");
        assert_eq!(rec.answer, "newa1");
        assert_eq!(rec.quiz2, "newq2");
        assert_eq!(rec.mobile_phone, "newm");
        assert_eq!(rec.answer2, "newa2");
    }

    /// `GetGatePublicAddr`：AddrTable `RemoteAddr` 命中即换成 `PublicAddr`，否则原样。
    #[test]
    fn gate_public_addr_lookup() {
        let route = GateRoute {
            server_name: "热血传奇".into(),
            title: "Title1".into(),
            remote_addr: "127.0.0.1".into(),
            public_addr: "10.0.0.1".into(),
            gates: std::array::from_fn(|_| TGateNet::default()),
        };
        assert_eq!(get_gate_public_addr(&[route], "127.0.0.1"), "10.0.0.1");
        assert_eq!(get_gate_public_addr(&[], "192.168.1.2"), "192.168.1.2");
    }

    /// `SortServerList`：把 idx 更大的同名项插到前面（C# 的插入口径，逐例手推）。
    #[test]
    fn sort_server_list_orders_same_name_by_desc_index() {
        let mut list = vec![srv("a", "A", 0, 0), srv("b", "A", 5, 0)];
        crate::session_server::sort_server_list(&mut list, 1);
        assert_eq!(
            list.iter().map(|s| s.server_index).collect::<Vec<_>>(),
            vec![5, 0]
        );
        // 已在正确位置时不改变顺序；长度不变（不丢/不重）
        let mut list = vec![srv("a", "A", 5, 0), srv("b", "A", 0, 0)];
        crate::session_server::sort_server_list(&mut list, 1);
        assert_eq!(
            list.iter().map(|s| s.server_index).collect::<Vec<_>>(),
            vec![5, 0]
        );
        assert_eq!(list.len(), 2);
        // 名字不同 ⇒ 追加到末尾
        let mut list = vec![srv("a", "A", 5, 0), srv("z", "Z", 99, 0)];
        crate::session_server::sort_server_list(&mut list, 1);
        assert_eq!(list[1].server_name, "Z");
        assert_eq!(list.len(), 2);
    }

    /// `FormatSecond` 文案（C# 拼接形状）。
    #[test]
    fn format_second_shape() {
        assert_eq!(format_second(0), "0天0小时0分钟0秒");
        assert_eq!(format_second(3661), "0天1小时1分钟1秒");
        assert_eq!(format_second(90061), "1天1小时1分钟1秒");
    }

    fn gate(tx: mpsc::UnboundedSender<Vec<u8>>) -> LoginGateInfo {
        LoginGateInfo {
            sipaddr: "10.0.0.9".to_string(),
            user_list: Vec::new(),
            keepalive_tick: 0,
            tx,
        }
    }

    fn user(sock: &str) -> UserInfo {
        UserInfo {
            sock_index: sock.to_string(),
            ..UserInfo::default()
        }
    }

    /// 牙 1（B-7）：分发期间条目被读任务删除 ⇒ 回写不得复活幽灵条目，且旧 `tx` 随对象释放。
    /// 旧实现（无条件 `insert`）下本用例必红：`gates` 会重新出现该 id。
    #[test]
    fn removed_gate_is_not_resurrected_by_inflight_writeback() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut gates = HashMap::new();
        gates.insert("gate-1".to_string(), gate(tx.clone()));

        // 分发开始：C# `GetSession`（这里是 clone）。
        let mut info = gates.get("gate-1").unwrap().clone();
        info.user_list.push(user("s1"));
        info.keepalive_tick = 42;

        // 读任务 EOF：`Disconnected` → `_clientManager.Delete`。
        gates.remove("gate-1");

        // 分发结束：回写。
        assert!(
            !commit_gate_update(&mut gates, "gate-1", info),
            "条目已被删除，回写必须报告未写入"
        );
        assert!(
            !gates.contains_key("gate-1"),
            "已删除的网关条目不得被在途消息写回（幽灵条目：user_list 残留 + tx 无读者）"
        );
        // `commit_gate_update` 在未回写时丢弃 `info`（连同它的 `tx` 克隆）；`tx` 也随之
        // 成为最后一个持有者 ⇒ 释放后接收端应观察到断开。
        drop(tx);
        assert!(
            matches!(rx.try_recv(), Err(mpsc::error::TryRecvError::Disconnected)),
            "幽灵 tx 不应被任何存活条目持有"
        );
    }

    /// 牙 2（B-7）：条目仍在表内时，分发期间的修改必须照常落地（防"干脆不回写"的过度修正）。
    #[test]
    fn present_gate_still_receives_mutations() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut gates = HashMap::new();
        gates.insert("gate-1".to_string(), gate(tx));

        let mut info = gates.get("gate-1").unwrap().clone();
        info.user_list.push(user("s1"));
        info.keepalive_tick = 9;

        assert!(commit_gate_update(&mut gates, "gate-1", info));
        let got = &gates["gate-1"];
        assert_eq!(got.user_list.len(), 1);
        assert_eq!(got.user_list[0].sock_index, "s1");
        assert_eq!(got.keepalive_tick, 9);
    }

    /// 牙 3（B-7）：两种交错顺序（先回写后删 / 先删后回写）终态都必须无幽灵条目。
    #[test]
    fn both_interleavings_converge_to_absent() {
        // 顺序 A：回写先于删除。
        {
            let (tx, _rx) = mpsc::unbounded_channel();
            let mut gates = HashMap::new();
            gates.insert("gate-1".to_string(), gate(tx));
            let mut info = gates.get("gate-1").unwrap().clone();
            info.user_list.push(user("s1"));
            assert!(commit_gate_update(&mut gates, "gate-1", info));
            gates.remove("gate-1");
            assert!(!gates.contains_key("gate-1"));
        }
        // 顺序 B：删除先于回写（含"判断时在、写入时已被删"的窗口——本实现里判断与写入同锁）。
        {
            let (tx, _rx) = mpsc::unbounded_channel();
            let mut gates = HashMap::new();
            gates.insert("gate-1".to_string(), gate(tx));
            let mut info = gates.get("gate-1").unwrap().clone();
            info.user_list.push(user("s1"));
            gates.remove("gate-1");
            assert!(!commit_gate_update(&mut gates, "gate-1", info));
            assert!(!gates.contains_key("gate-1"));
        }
    }

    /// 反例牙：把回写改回旧的"无条件 insert"，上面牙 1 的断言确实会红（证明判别力）。
    #[test]
    fn unconditional_insert_would_resurrect_removed_gate() {
        fn buggy_insert(
            gates: &mut HashMap<String, LoginGateInfo>,
            connection_id: &str,
            info: LoginGateInfo,
        ) {
            gates.insert(connection_id.to_string(), info);
        }

        let (tx, _rx) = mpsc::unbounded_channel();
        let mut gates = HashMap::new();
        gates.insert("gate-1".to_string(), gate(tx));
        let mut info = gates.get("gate-1").unwrap().clone();
        info.user_list.push(user("s1"));
        gates.remove("gate-1");
        buggy_insert(&mut gates, "gate-1", info);
        assert!(
            gates.contains_key("gate-1"),
            "旧实现必然复活条目——这正是本卡要消除的幽灵"
        );
    }
}
