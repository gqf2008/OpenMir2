//! `SessionServer`（5600）—— 1:1 对齐 C# `src/LoginSrv/Services/SessionServer.cs`。
//!
//! 这条链上连的是 DBSrv（`ServerIndex=99`）与 GameSvr：它们上报 `SS_SERVERINFO`
//! （服务器名/序号/在线数/计费模式），账号中心据此回 `SS_KEEPALIVE`；选服下发
//! `SS_OPENSESSION`（进游戏许可）、顶号下发 `SS_CLOSESESSION`。报文是**文本帧** `(code/参数…)`。

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use mir2_protocol::messages;
use mir2_shared::hutil32::{self, ArrestWrite};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Notify};

use crate::gbk;
use crate::log_line;
use crate::login_server::ordinal_ignore_case_eq;
use crate::state::{tick, IndexOutOfRange, ServerSessionInfo, Share};

/// 读循环里延后到**释放锁之后**执行的动作（避免持锁 await 造成自锁）。
enum Action {
    /// `SendServerMsgA`：广播。
    Broadcast(u16, String),
    /// `SendServerMsg`：(ident, 服务器名, 报文)。
    ServerMsg(u16, String, String),
    /// 纯日志。
    Log(&'static str, String),
}

/// 一条 5600 连接的关闭句柄（C# `msgServer.Socket.Close()` 的等价物）。
#[derive(Debug, Clone)]
pub struct ServerHandle {
    /// 通知读任务收工。
    pub notify: Arc<Notify>,
    /// 已关闭标记。
    pub closed: Arc<AtomicBool>,
}

/// 监听 5600。
///
/// # Errors
/// accept 失败时返回错误。
pub async fn run_session_listener(share: Arc<Share>, listener: TcpListener) -> Result<()> {
    let mut seq = 1u64;
    loop {
        let (sock, peer) = listener.accept().await?;
        sock.set_nodelay(true).ok();
        let ip = peer.ip().to_string();
        // C# `Connecting`：不在 `ServerAddr` 白名单内 ⇒ 打日志并立即关闭
        if !share.cfg.server_addrs.iter().any(|a| a == &ip) {
            log_line("WRN", &format!("非法地址连接:{ip}"));
            drop(sock);
            continue;
        }
        let socket_id = format!("srv-{seq}");
        seq += 1;
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let handle = ServerHandle {
            notify: Arc::new(Notify::new()),
            closed: Arc::new(AtomicBool::new(false)),
        };
        share.servers.lock().await.push(ServerSessionInfo {
            socket_id: socket_id.clone(),
            tx,
            endpoint: peer.to_string(),
            server_name: String::new(),
            server_index: 0,
            online_count: 0,
            keepalive_tick: tick(),
            payment_mode: 0,
            session_list: Vec::new(),
            receive_msg: String::new(),
            handle: handle.clone(),
        });
        log_line("DBG", &format!("{peer}链接成功."));
        let share_c = Arc::clone(&share);
        let id = socket_id.clone();
        let h = handle.clone();
        tokio::spawn(async move {
            let (mut rd, mut wr) = sock.into_split();
            let writer = tokio::spawn(async move {
                while let Some(buf) = rx.recv().await {
                    if wr.write_all(&buf).await.is_err() {
                        break;
                    }
                    wr.flush().await.ok();
                }
            });
            let mut buf = vec![0u8; 8192];
            loop {
                tokio::select! {
                    r = rd.read(&mut buf) => match r {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let text = gbk::decode(&buf[..n]);
                            socket_client_read(&share_c, &id, &text).await;
                        }
                    },
                    () = h.notify.notified() => break,
                }
            }
            let removed = {
                let mut servers = share_c.servers.lock().await;
                servers
                    .iter()
                    .position(|s| s.socket_id == id)
                    .map(|i| servers.remove(i))
            };
            writer.abort();
            if let Some(s) = removed {
                let kind = if s.server_index == 99 {
                    "数据库服务器"
                } else {
                    "游戏服务器"
                };
                log_line(
                    "WRN",
                    &format!("[{}]{kind}[{}]断开链接.", s.server_name, s.endpoint),
                );
            }
        });
    }
}

/// `SessionServer.SocketClientRead`（含 C# 把 `sReviceMsg` 写给**所有**条目的覆盖笔误）。
#[allow(clippy::too_many_lines)] // 1:1 照搬 C# 的 if/switch 结构，拆开反而难对读
async fn socket_client_read(share: &Arc<Share>, socket_id: &str, chunk: &str) {
    let (mut server, mut s_revice_msg) = {
        let servers = share.servers.lock().await;
        let Some(pos) = servers.iter().position(|s| s.socket_id == socket_id) else {
            return;
        };
        let server = servers[pos].clone();
        let mut buf = server.receive_msg.clone();
        buf.push_str(chunk);
        (server, buf)
    };
    let mut actions: Vec<Action> = Vec::new();
    let mut sorted_marker: Option<usize> = None;
    while s_revice_msg.find(')').is_some_and(|p| p > 0) {
        let (write, rest) = hutil32::arrest_string_ex_ref(&s_revice_msg, '(', ')');
        s_revice_msg = rest;
        let ArrestWrite::Value(s_msg) = write else {
            continue;
        };
        if s_msg.is_empty() {
            break;
        }
        let (code, rest) = hutil32::get_valid_str3(&s_msg, &['/']);
        let n_code = hutil32::str_to_int(&code, -1);
        match u16::try_from(n_code).unwrap_or(messages::UNKNOWMSG) {
            messages::SS_SOFTOUTSESSION => {
                let (account, rest) = hutil32::get_valid_str3(&rest, &['/']);
                let session_id = hutil32::str_to_int(&rest, 0);
                close_user(&mut server, &account, session_id, &mut actions);
            }
            messages::SS_SERVERINFO => {
                let (server_name, rest) = hutil32::get_valid_str3(&rest, &['/']);
                let (index, rest) = hutil32::get_valid_str3(&rest, &['/']);
                let (online, rest) = hutil32::get_valid_str3(&rest, &['/']);
                let (pay, _) = hutil32::get_valid_str3(&rest, &['/']);
                server.server_name = server_name.clone();
                server.server_index = hutil32::str_to_int(&index, 0);
                server.online_count = hutil32::str_to_int(&online, 0);
                server.payment_mode = hutil32::str_to_int(&pay, 0);
                server.keepalive_tick = tick();
                sorted_marker = Some(0);
                let online_min = get_online_hum_count(share, &server).await;
                share.online_min.store(online_min, Ordering::SeqCst);
                if online_min > share.online_max.load(Ordering::SeqCst) {
                    share.online_max.store(online_min, Ordering::SeqCst);
                }
                actions.push(Action::Broadcast(
                    messages::SS_KEEPALIVE,
                    online_min.to_string(),
                ));
                ref_server_limit(share, &server, &server_name).await;
            }
            messages::ISM_GAMETIMEOFTIMECARDUSER => {
                let (server_name, rest) = hutil32::get_valid_str3(&rest, &['/']);
                let (account, rest) = hutil32::get_valid_str3(&rest, &['/']);
                let game_time = hutil32::str_to_int(&rest, 0);
                if server.payment_mode >= 3 {
                    change_play_time_user(share, &server_name, &account, game_time, &mut actions)
                        .await;
                }
            }
            messages::ISM_QUERYACCOUNTEXPIRETIME => {
                let (account, _) = hutil32::get_valid_str3(&rest, &['/']);
                if server.payment_mode >= 3 {
                    query_play_time(share, &account, &mut actions).await;
                }
            }
            messages::ISM_CHECKTIMEACCOUNT => {
                let (account, _) = hutil32::get_valid_str3(&rest, &['/']);
                if server.payment_mode >= 3 {
                    check_time_account(share, &account, &mut actions).await;
                }
            }
            messages::UNKNOWMSG => {
                actions.push(Action::Broadcast(messages::UNKNOWMSG, rest.clone()));
            }
            _ => actions.push(Action::Log("DBG", n_code.to_string())),
        }
    }
    // 写回：匹配条目更新字段（并按需重排），随后**所有**条目写 `ReceiveMsg`（复刻 C# 笔误）
    {
        let mut servers = share.servers.lock().await;
        if let Some(pos) = servers.iter().position(|s| s.socket_id == socket_id) {
            servers[pos] = server;
            if sorted_marker.is_some() {
                sort_server_list(&mut servers, pos);
            }
        }
        for s in servers.iter_mut() {
            s.receive_msg.clone_from(&s_revice_msg);
        }
    }
    for action in actions {
        match action {
            Action::Broadcast(ident, msg) => send_server_msg_a(share, ident, &msg).await,
            Action::ServerMsg(ident, name, msg) => {
                send_server_msg(share, ident, &name, &msg).await;
            }
            Action::Log(level, msg) => log_line(level, &msg),
        }
    }
}

/// `SessionServer.CloseUser`（按账号或会话号移除并回 `SS_CLOSESESSION`）。
fn close_user(
    server: &mut ServerSessionInfo,
    account: &str,
    session_id: i32,
    actions: &mut Vec<Action>,
) {
    let mut i = server.session_list.len();
    while i > 0 {
        i -= 1;
        let c = server.session_list[i].clone();
        if c.account == account || c.session_id == session_id {
            server.session_list.remove(i);
            actions.push(Action::ServerMsg(
                messages::SS_CLOSESESSION,
                c.server_name.clone(),
                format!("{}/{}", c.account, c.session_id),
            ));
        }
    }
}

/// `SessionServer.SortServerList`（含 C# 的插入/去重口径）。
pub(crate) fn sort_server_list(list: &mut Vec<ServerSessionInfo>, n_index: usize) {
    if list.len() <= n_index {
        return;
    }
    let sort = list.remove(n_index);
    let (name, idx) = (sort.server_name.clone(), sort.server_index);
    let mut insert_at: Option<usize> = None;
    let mut drop_at: Option<usize> = None;
    let mut n_c = 0;
    while n_c < list.len() {
        if ordinal_ignore_case_eq(&list[n_c].server_name, &name) {
            if list[n_c].server_index < idx {
                insert_at = Some(n_c);
                break;
            }
            if n_c + 1 < list.len() {
                let mut fallback = n_c + 1;
                let mut n10 = n_c + 1;
                let mut done = false;
                while n10 < list.len() {
                    if ordinal_ignore_case_eq(&list[n10].server_name, &name) {
                        if list[n10].server_index < idx {
                            insert_at = Some(n10);
                            let mut n14 = n10 + 1;
                            while n14 < list.len() {
                                if ordinal_ignore_case_eq(&list[n14].server_name, &name)
                                    && list[n14].server_index == idx
                                {
                                    drop_at = Some(n14);
                                    break;
                                }
                                n14 += 1;
                            }
                            done = true;
                            break;
                        }
                        fallback = n10 + 1;
                    }
                    n10 += 1;
                }
                if done {
                    break;
                }
                insert_at = Some(fallback);
                break;
            }
        }
        n_c += 1;
    }
    match insert_at {
        Some(pos) => {
            list.insert(pos, sort);
            if let Some(drop) = drop_at {
                // 插入先于删除 ⇒ 删除位置顺移 1
                let drop = if drop > pos { drop + 1 } else { drop };
                if drop < list.len() {
                    list.remove(drop);
                }
            }
        }
        None => list.push(sort),
    }
}

/// `SessionServer.GetOnlineHumCount`（排除 `ServerIndex=99` 的 DBSrv）。
async fn get_online_hum_count(share: &Arc<Share>, current: &ServerSessionInfo) -> i32 {
    let servers = share.servers.lock().await;
    servers
        .iter()
        .filter(|s| s.socket_id != current.socket_id && s.server_index != 99)
        .map(|s| s.online_count)
        .sum::<i32>()
        + if current.server_index == 99 {
            0
        } else {
            current.online_count
        }
}

/// `SessionServer.RefServerLimit`。
async fn ref_server_limit(share: &Arc<Share>, current: &ServerSessionInfo, server_name: &str) {
    let n_count: i32 = {
        let servers = share.servers.lock().await;
        servers
            .iter()
            .filter(|s| s.socket_id != current.socket_id && s.server_index != 99)
            .filter(|s| s.server_name == server_name)
            .map(|s| s.online_count)
            .sum::<i32>()
            + if current.server_index == 99 || current.server_name != server_name {
                0
            } else {
                current.online_count
            }
    };
    let mut limits = share.user_limit.lock().await;
    for e in limits.iter_mut().flatten() {
        if e.server_name == server_name {
            e.limit_count_min = n_count;
            break;
        }
    }
}

/// `SessionServer.GetLimitName`（C# 无 null 检查 ⇒ 尾部空槽必 NRE，这里以 `Err` 表达）。
///
/// # Errors
/// 遍历到空槽（`UserLimit` 未填满 100 项）且此前未命中同名时返回 [`IndexOutOfRange`]。
pub async fn get_limit_name(
    share: &Arc<Share>,
    server_name: &str,
) -> Result<String, IndexOutOfRange> {
    let limits = share.user_limit.lock().await;
    for slot in limits.iter() {
        let Some(e) = slot else {
            return Err(IndexOutOfRange);
        };
        if ordinal_ignore_case_eq(&e.server_name, server_name) {
            return Ok(e.name.clone());
        }
    }
    Ok(String::new())
}

/// `SessionServer.IsNotUserFull`。
///
/// # Errors
/// 同 C# 的 NRE：遍历到空槽且未命中同名 ⇒ 异常直穿（调用方不回包）。
pub async fn is_not_user_full(
    share: &Arc<Share>,
    server_name: &str,
) -> Result<bool, IndexOutOfRange> {
    let limits = share.user_limit.lock().await;
    for slot in limits.iter() {
        let Some(e) = slot else {
            return Err(IndexOutOfRange);
        };
        if e.server_name == server_name {
            return Ok(e.limit_count_min <= e.limit_count_max);
        }
    }
    Ok(true)
}

/// `SessionServer.GetServerStatus` → C# 把枚举拼进字符串 ⇒ 取**枚举名**（0 无名字即 "0"）。
pub async fn get_server_status(share: &Arc<Share>, server_name: &str) -> &'static str {
    let online = {
        let servers = share.servers.lock().await;
        servers
            .iter()
            .any(|s| s.server_index != 99 && s.server_name == server_name)
    };
    if !online {
        return "0";
    }
    let limits = share.user_limit.lock().await;
    for slot in limits.iter() {
        let Some(e) = slot else {
            return "0"; // C# try/catch：空槽 NRE 被吞 ⇒ 状态 0
        };
        if e.server_name == server_name {
            let max = e.limit_count_max;
            let min = e.limit_count_min;
            if min <= max / 2 {
                return "Idle";
            }
            if min <= max - max / 5 {
                return "General";
            }
            if min < max {
                return "Busy";
            }
            return "Full";
        }
    }
    "0"
}

/// `SessionServer.SendServerMsg`。
pub async fn send_server_msg(share: &Arc<Share>, ident: u16, server_name: &str, msg: &str) {
    let limit_name = match get_limit_name(share, server_name).await {
        Ok(name) => name,
        Err(e) => {
            // C# 的 try/catch：NRE 时什么都不发
            log_line(
                "ERR",
                &format!("[Exception] SessionServer.SendServerMsg: {e}"),
            );
            return;
        }
    };
    let text = format!("({ident}/{msg})").into_bytes();
    let servers = share.servers.lock().await;
    for s in servers.iter() {
        if s.tx.is_closed() {
            continue;
        }
        if limit_name.is_empty()
            || s.server_name.is_empty()
            || ordinal_ignore_case_eq(&s.server_name, &limit_name)
            || s.server_index == 99
        {
            let _ = s.tx.send(text.clone());
        }
    }
}

/// `SessionServer.SendServerMsgA`（广播，无名称过滤）。
pub async fn send_server_msg_a(share: &Arc<Share>, ident: u16, msg: &str) {
    let text = format!("({ident}/{msg})").into_bytes();
    let servers = share.servers.lock().await;
    for s in servers.iter() {
        if !s.tx.is_closed() {
            let _ = s.tx.send(text.clone());
        }
    }
}

/// `SessionServer.QueryPlayTime`。
async fn query_play_time(share: &Arc<Share>, account: &str, actions: &mut Vec<Action>) {
    if account.is_empty() {
        return;
    }
    let seconds = share
        .accounts
        .lock()
        .await
        .play_time(account)
        .await
        .unwrap_or(0);
    let certs = share.cert_list.lock().await.clone();
    for c in certs {
        if ordinal_ignore_case_eq(&c.login_id, account)
            && !c.free_mode
            && !c.closing
            && (c.available_type == 2 || (6..=10).contains(&c.available_type))
        {
            actions.push(Action::ServerMsg(
                messages::ISM_QUERYPLAYTIME,
                c.server_name.clone(),
                format!("{}/{}", c.login_id, seconds),
            ));
            actions.push(Action::Log(
                "DBG",
                format!(
                    "[GameServer/Send] ISM_QUERYPLAYTIME : {} PlayTime: ({seconds})",
                    c.login_id
                ),
            ));
        }
    }
}

/// `SessionServer.CheckTimeAccount`。
async fn check_time_account(share: &Arc<Share>, account: &str, actions: &mut Vec<Action>) {
    if account.is_empty() {
        return;
    }
    let seconds = share
        .accounts
        .lock()
        .await
        .play_time(account)
        .await
        .unwrap_or(0);
    if seconds != 0 {
        return;
    }
    let certs = share.cert_list.lock().await.clone();
    for c in certs {
        if ordinal_ignore_case_eq(&c.login_id, account)
            && !c.free_mode
            && !c.closing
            && (c.available_type == 2 || (6..=10).contains(&c.available_type))
        {
            actions.push(Action::ServerMsg(
                messages::ISM_ACCOUNTEXPIRED,
                c.server_name.clone(),
                format!("{}/{}", c.login_id, c.certification),
            ));
        }
    }
}

/// `SessionServer.ChanggePlayTimeUser`。
async fn change_play_time_user(
    share: &Arc<Share>,
    server_name: &str,
    account: &str,
    game_time: i32,
    actions: &mut Vec<Action>,
) {
    let mut seconds = share
        .accounts
        .lock()
        .await
        .play_time(account)
        .await
        .unwrap_or(0);
    if seconds > 0 {
        seconds -= 60;
        let _ = share
            .accounts
            .lock()
            .await
            .update_play_time(account, seconds)
            .await;
        actions.push(Action::Log(
            "DBG",
            format!("账号:[{account}] 数据库时间:{seconds} 引擎时间:[{game_time}]"),
        ));
        if seconds < i64::from(game_time) {
            actions.push(Action::Log("DBG", format!("账号[{account}]游戏时间异常.")));
        } else {
            seconds = 0;
        }
    }
    if seconds == 0 {
        let certs = share.cert_list.lock().await.clone();
        for c in certs {
            if ordinal_ignore_case_eq(&c.login_id, account)
                && !c.free_mode
                && !c.closing
                && (c.available_type == 2 || (6..=10).contains(&c.available_type))
            {
                actions.push(Action::ServerMsg(
                    messages::SS_CLOSESESSION,
                    server_name.to_string(),
                    format!("{}/{}", c.login_id, c.certification),
                ));
                actions.push(Action::Log(
                    "DBG",
                    format!(
                        "[GameServer/Send] ISM_CANCELADMISSION : {} TO ({})",
                        c.login_id, c.addr
                    ),
                ));
            }
        }
    } else {
        actions.push(Action::ServerMsg(
            messages::ISM_QUERYPLAYTIME,
            server_name.to_string(),
            format!("{account}/{seconds}"),
        ));
        actions.push(Action::Log(
            "DBG",
            format!("[GameServer/Send] ISM_QUERYPLAYTIME : {account} PlayTime: ({seconds})"),
        ));
    }
}

/// `TimedService.SystemStatus`（20s 一次的状态汇总日志）。
pub async fn system_status(share: &Arc<Share>) {
    let servers = share.servers.lock().await;
    let mut builder = String::new();
    for s in servers.iter() {
        if s.server_name.is_empty() {
            builder.push_str("-/-/-/-;");
            continue;
        }
        let _ = write!(
            builder,
            "{}/{}/{}/",
            s.server_name, s.server_index, s.online_count
        );
        if s.server_index == 99 {
            builder.push_str("DBServer/");
        } else {
            builder.push_str("GameServer/");
            builder.push_str(match s.payment_mode {
                0 => "免费/",
                1 => "试玩/",
                2 => "测试/",
                3 => "付费/",
                _ => "",
            });
        }
        let _ = write!(builder, "Online:{}/", s.online_count);
        if tick().wrapping_sub(s.keepalive_tick) < 30_000 {
            builder.push_str("正常");
        } else {
            builder.push_str("超时");
        }
    }
    if !builder.is_empty() {
        log_line("DBG", &builder);
    }
}

/// `TimedService.CheckServerStatus`（10s 一次；>20s 无心跳即关闭连接）。
pub async fn check_server_status(share: &Arc<Share>) {
    let dead: Vec<ServerHandle> = {
        let servers = share.servers.lock().await;
        servers
            .iter()
            .filter(|s| !s.server_name.is_empty() && tick().wrapping_sub(s.keepalive_tick) > 20_000)
            .map(|s| {
                let kind = if s.server_index == 99 {
                    "数据库服务器"
                } else {
                    "游戏服务器"
                };
                log_line(
                    "WRN",
                    &format!("[{}]{kind}响应超时,关闭链接.", s.server_name),
                );
                s.handle.clone()
            })
            .collect()
    };
    for h in dead {
        h.closed.store(true, Ordering::SeqCst);
        h.notify.notify_waiters();
    }
}

/// `LoginServer.SessionClearKick`（5s 宽限后清会话）。
pub async fn session_clear_kick(share: &Arc<Share>) {
    let snapshot = share.sessions.lock().await.snapshot();
    for c in snapshot {
        if c.kicked && tick().wrapping_sub(c.kick_tick) > 5 * 1000 {
            share.sessions.lock().await.delete(&c.account, c.session_id);
        }
    }
}

/// `SessionServer.SessionClearNoPayMent`（付费服 1 小时未付费即踢）。
pub async fn session_clear_no_payment(share: &Arc<Share>, test_server: bool) {
    if test_server {
        return;
    }
    let mut actions: Vec<Action> = Vec::new();
    {
        let mut servers = share.servers.lock().await;
        for s in servers.iter_mut() {
            if s.payment_mode != 3 {
                continue;
            }
            let mut i = s.session_list.len();
            while i > 0 {
                i -= 1;
                let c = s.session_list[i].clone();
                if !c.kicked && !c.is_payment && tick().wrapping_sub(c.start_tick) > 60 * 60 * 1000
                {
                    s.session_list.remove(i);
                    actions.push(Action::ServerMsg(
                        messages::SS_KICKUSER,
                        c.server_name.clone(),
                        format!("{}/{}", c.account, c.session_id),
                    ));
                }
            }
        }
    }
    for action in actions {
        if let Action::ServerMsg(ident, name, msg) = action {
            send_server_msg(share, ident, &name, &msg).await;
        }
    }
}
