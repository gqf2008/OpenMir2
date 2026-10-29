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
        // 卡 `openmir2-login-srv-illegal-addr-filter`（A1/A2）：参考树 SessionServer.cs:63-89
        // 是「白名单一票否决 + Close」，但运行二进制的**可观测行为**与之不符——同 conf
        // （白名单 = ServerAddr.txt 4 旧网段 IP，无 127.0.0.1）下 C# 生产**接受** 127.0.0.1
        // 服务链入（LoginSrv.out.log 18:13:32.179 `[DBG] 127.0.0.1:64174链接成功`＝boAllowed
        // 分支，且三次 login_only 全链 completed）。以可观测 oracle 为准：白名单不命中仅记
        // 日志，**不断链**（服务注册链路 db-srv⇄login-srv 必经此面）。
        let ip = peer.ip().to_string();
        if !share.cfg.server_addrs.iter().any(|a| a == &ip) {
            log_line(
                "WRN",
                &format!("非法地址连接:{ip}（不在 ServerAddr 白名单；按 oracle 接受，不断链）"),
            );
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

/// `SS_SERVERINFO` 的解析结果：`(103/服务器名/服务器序号/在线数/付费模式)`。
///
/// 发送端是 GameSvr 的 `AuthenticationService.SendOnlineHumCountMsg`（`:113-116`，帧体五段、
/// 四段字段）；本仓发送侧见 `mir2-rs/bins/game-svr/src/ids.rs` 的 `serverinfo_frame`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfoReport {
    /// 服务器名（`SystemShare.Config.ServerName`）。
    pub server_name: String,
    /// 服务器序号（oracle 恒 0；99 = DBSrv）。
    pub server_index: i32,
    /// 在线人数（`WorldEngine.OnlinePlayObject`）。
    pub online_count: i32,
    /// 计费模式（`[Server] PayMentMode`，oracle 缺省 1）。
    pub payment_mode: i32,
}

/// 解析 `(103/…)` 里 `103/` 之后的四段（`HUtil32.GetValidStr3` 逐段；缺段照 C# 取空串/0）。
#[must_use]
pub fn parse_serverinfo(rest: &str) -> ServerInfoReport {
    let (server_name, rest) = hutil32::get_valid_str3(rest, &['/']);
    let (index, rest) = hutil32::get_valid_str3(&rest, &['/']);
    let (online, rest) = hutil32::get_valid_str3(&rest, &['/']);
    let (pay, _) = hutil32::get_valid_str3(&rest, &['/']);
    ServerInfoReport {
        server_name,
        server_index: hutil32::str_to_int(&index, 0),
        online_count: hutil32::str_to_int(&online, 0),
        payment_mode: hutil32::str_to_int(&pay, 0),
    }
}

/// 把 `SS_SERVERINFO` 的解析结果写回在线槽条目（`SocketClientRead` 的 103 分支）。
///
/// 单列成函数让「解析结果是否真的落槽」有可执行判据（T-8：删掉任一行 —— 尤其
/// `server_index` —— 用例必红）。
fn apply_serverinfo(server: &mut ServerSessionInfo, report: &ServerInfoReport) {
    server.server_name.clone_from(&report.server_name);
    server.server_index = report.server_index;
    server.online_count = report.online_count;
    server.payment_mode = report.payment_mode;
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
                let report = parse_serverinfo(&rest);
                apply_serverinfo(&mut server, &report);
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
                ref_server_limit(share, &server, &report.server_name).await;
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

/// C# `ServerStatus`（`SessionServer.cs:726+`）：拼进服务器列表串的是**枚举名**，不是数字。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerStatus {
    /// 空闲（C# `Idle = 1`）。
    Idle,
    /// 良好（C# `General = 2`）。
    General,
    /// 繁忙（C# `Busy = 3`）。
    Busy,
    /// 满员（C# `Full = 4`）。
    Full,
}

impl ServerStatus {
    /// `Enum.ToString()` 的结果（状态 0 无名字时由调用方拼 `"0"`）。
    #[must_use]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::General => "General",
            Self::Busy => "Busy",
            Self::Full => "Full",
        }
    }
}

/// `GetServerStatus` 的四档阈值（`SessionServer.cs:645-665`），**顺序与 C# 整数除法照抄**：
/// `Min <= Max/2 ⇒ Idle`；`Min <= Max - Max/5 ⇒ General`；`Min < Max ⇒ Busy`；否则 `Full`。
#[must_use]
pub(crate) const fn limit_status(min: i32, max: i32) -> ServerStatus {
    if min <= max / 2 {
        ServerStatus::Idle
    } else if min <= max - max / 5 {
        ServerStatus::General
    } else if min < max {
        ServerStatus::Busy
    } else {
        ServerStatus::Full
    }
}

/// `GetServerStatus` 的在线判定（`SessionServer.cs:625-643`）：
/// `_serverList` 里存在同名且 `ServerIndex != 99`（99 = DBSrv）的条目即在线。
#[must_use]
pub(crate) fn is_server_online(servers: &[ServerSessionInfo], server_name: &str) -> bool {
    servers
        .iter()
        .any(|s| s.server_index != 99 && s.server_name == server_name)
}

/// `GetServerStatus` 的纯判定体；离线或 `UserLimit` 无同名槽 ⇒ `"0"`。
///
/// `None` 槽复刻 C# 的 NRE：`UserLimit[i]` 为 null 时 `try/catch` 吞掉异常并返回当时的 `status`（0）。
#[must_use]
pub(crate) fn resolve_server_status(
    server_name: &str,
    online: bool,
    limits: &[Option<crate::config::LimitServerUserInfo>],
) -> &'static str {
    if !online {
        return "0";
    }
    for slot in limits {
        let Some(e) = slot else {
            return "0";
        };
        if e.server_name == server_name {
            return limit_status(e.limit_count_min, e.limit_count_max).name();
        }
    }
    "0"
}

/// `SessionServer.GetServerStatus` → C# 把枚举拼进字符串 ⇒ 取**枚举名**（0 无名字即 "0"）。
pub async fn get_server_status(share: &Arc<Share>, server_name: &str) -> &'static str {
    let online = {
        let servers = share.servers.lock().await;
        is_server_online(servers.as_slice(), server_name)
    };
    let limits = share.user_limit.lock().await;
    resolve_server_status(server_name, online, limits.as_slice())
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

#[cfg(test)]
mod tests {
    use super::{
        apply_serverinfo, is_server_online, limit_status, parse_serverinfo, resolve_server_status,
        ServerHandle, ServerInfoReport, ServerStatus,
    };
    use crate::config::LimitServerUserInfo;
    use crate::gbk;
    use crate::state::ServerSessionInfo;
    use mir2_protocol::messages;
    use mir2_shared::hutil32::{self, ArrestWrite};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn lim(name: &str, min: i32, max: i32) -> LimitServerUserInfo {
        LimitServerUserInfo {
            server_name: name.into(),
            name: "g".into(),
            limit_count_min: min,
            limit_count_max: max,
        }
    }

    fn srv(name: &str, index: i32) -> ServerSessionInfo {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        ServerSessionInfo {
            socket_id: format!("s-{index}-{name}"),
            tx,
            endpoint: "127.0.0.1:1".into(),
            server_name: name.into(),
            server_index: index,
            online_count: 0,
            keepalive_tick: 0,
            payment_mode: 0,
            session_list: Vec::new(),
            receive_msg: String::new(),
            handle: ServerHandle {
                notify: Arc::new(tokio::sync::Notify::new()),
                closed: Arc::new(AtomicBool::new(false)),
            },
        }
    }

    /// 四档逐档断言 + `Max/2`、`Max - Max/5` 边界（C# 整数除法口径）。
    #[test]
    fn four_tiers_and_boundaries() {
        // 偶数 Max=3000：Max/2=1500，Max-Max/5=2400
        assert_eq!(limit_status(0, 3000).name(), "Idle");
        assert_eq!(limit_status(1500, 3000).name(), "Idle"); // 边界 ≤
        assert_eq!(limit_status(1501, 3000).name(), "General");
        assert_eq!(limit_status(2400, 3000).name(), "General"); // 边界 ≤
        assert_eq!(limit_status(2401, 3000).name(), "Busy");
        assert_eq!(limit_status(2999, 3000).name(), "Busy");
        assert_eq!(limit_status(3000, 3000).name(), "Full"); // Min >= Max
        assert_eq!(limit_status(4000, 3000).name(), "Full"); // Min > Max 也是 Full

        // 奇数 Max=3001：Max/2=1500（整除截断），Max-Max/5=3001-600=2401
        assert_eq!(limit_status(1500, 3001).name(), "Idle");
        assert_eq!(limit_status(1501, 3001).name(), "General");
        assert_eq!(limit_status(2401, 3001).name(), "General");
        assert_eq!(limit_status(2402, 3001).name(), "Busy");
    }

    /// 在线判定：`ServerIndex == 99`（DBSrv）同名不算在线。
    #[test]
    fn online_requires_non_db_server_with_same_name() {
        let db_only = vec![srv("热血传奇", 99)];
        assert!(!is_server_online(&db_only, "热血传奇"));
        let both = vec![srv("LegendOfMir2", 99), srv("热血传奇", 0)];
        assert!(is_server_online(&both, "热血传奇"));
        assert!(!is_server_online(&both, "其它服"));
    }

    /// 纯判定体：离线 / 无同名槽 / 空槽 ⇒ "0"；在线且命中 ⇒ 枚举名。
    #[test]
    fn resolve_status_cases() {
        let limits = [Some(lim("热血传奇", 0, 3000))];
        assert_eq!(resolve_server_status("热血传奇", false, &limits), "0");
        assert_eq!(resolve_server_status("热血传奇", true, &limits), "Idle");
        assert_eq!(resolve_server_status("其它服", true, &limits), "0");
        // 空槽在命中项之后不影响；命中项之前出现空槽则复刻 C# NRE ⇒ "0"
        let after = [Some(lim("热血传奇", 0, 3000)), None];
        assert_eq!(resolve_server_status("热血传奇", true, &after), "Idle");
        let before = [None, Some(lim("热血传奇", 0, 3000))];
        assert_eq!(resolve_server_status("热血传奇", true, &before), "0");
    }

    /// 拼进列表串的是枚举名（不是数字）。
    #[test]
    fn status_names_are_enum_names() {
        assert_eq!(ServerStatus::Idle.name(), "Idle");
        assert_eq!(ServerStatus::General.name(), "General");
        assert_eq!(ServerStatus::Busy.name(), "Busy");
        assert_eq!(ServerStatus::Full.name(), "Full");
    }

    /// 在线判定必须是**序数精确相等**（C# `string ==` → `StringComparison.Ordinal`）：
    /// 大小写变体、前缀/子串、后缀扩展都不算同一个名字。
    ///
    /// 牙：把 `==` 换成 `eq_ignore_ascii_case` 或 `starts_with`/`contains`，本用例立刻红。
    #[test]
    fn is_server_online_requires_exact_ordinal_match() {
        let both = vec![srv("legendofmir2", 1), srv("热血传奇", 0)];
        assert!(is_server_online(&both, "legendofmir2"));
        assert!(is_server_online(&both, "热血传奇"));
        // 大小写变体
        assert!(!is_server_online(&both, "LegendOfMir2"));
        assert!(!is_server_online(&both, "LEGENDOFMIR2"));
        // 前缀 / 子串 / 后缀扩展
        assert!(!is_server_online(&both, "legend"));
        assert!(!is_server_online(&both, "热血"));
        assert!(!is_server_online(&both, "热血传奇2"));
    }

    /// **I-2③**：login-srv 侧——生产字节 gb2312 `(103/热血传奇/0/0/1)` → GBK 解码 → `(...)`
    /// 分帧 → 取消息号 → [`parse_serverinfo`] → [`is_server_online`]（99 门）→
    /// [`resolve_server_status`] = `Idle`。复用本模块既有 helper 与四档用例，不重复定义。
    ///
    /// 牙：`is_server_online` 的 `server_index != 99` 改 `== 99`、或四档阈值改错 ⇒ 必红。
    #[test]
    fn serverinfo_report_makes_server_status_idle() {
        let raw = [
            b"(103/".as_slice(),
            &[0xC8, 0xC8, 0xD1, 0xAA, 0xB4, 0xAB, 0xC6, 0xE6],
            b"/0/0/1)".as_slice(),
        ]
        .concat();
        let text = gbk::decode(&raw);
        let (write, rest) = hutil32::arrest_string_ex_ref(&text, '(', ')');
        assert!(rest.is_empty(), "整帧被消费");
        let ArrestWrite::Value(msg) = write else {
            panic!("分帧失败")
        };
        let (code, body) = hutil32::get_valid_str3(&msg, &['/']);
        assert_eq!(
            hutil32::str_to_int(&code, -1),
            i32::from(messages::SS_SERVERINFO)
        );
        assert_eq!(
            parse_serverinfo(&body),
            ServerInfoReport {
                server_name: "热血传奇".to_string(),
                server_index: 0,
                online_count: 0,
                payment_mode: 1, // I-1：oracle 缺省 1（不是 0）
            }
        );
        // 在线判定：GameSvr（index 0）算在线；DBSrv（index 99）不算。
        let game = srv("热血传奇", 0);
        let db = srv("热血传奇", 99);
        assert!(is_server_online(std::slice::from_ref(&game), "热血传奇"));
        assert!(
            !is_server_online(std::slice::from_ref(&db), "热血传奇"),
            "ServerIndex=99 是 DBSrv ⇒ 不算游戏服在线"
        );
        // 部署 `UserLimit.txt`：limit_count_max = 10000（`Server.conf`/部署件同口径）。
        let slots = [Some(lim("热血传奇", 0, 10_000)), None];
        assert_eq!(resolve_server_status("热血传奇", true, &slots), "Idle");
        assert_eq!(resolve_server_status("热血传奇", false, &slots), "0");
        // 四档阈值（max = 10000）：5000 Idle / 8000 General / 9999 Busy / 10000 Full。
        assert_eq!(
            resolve_server_status("x", true, &[Some(lim("x", 5_000, 10_000))]),
            "Idle"
        );
        assert_eq!(
            resolve_server_status("x", true, &[Some(lim("x", 8_000, 10_000))]),
            "General"
        );
        assert_eq!(
            resolve_server_status("x", true, &[Some(lim("x", 9_999, 10_000))]),
            "Busy"
        );
        assert_eq!(
            resolve_server_status("x", true, &[Some(lim("x", 10_000, 10_000))]),
            "Full"
        );
        // 尾部空槽（C# NRE 被吞）在未命中时 ⇒ "0"。
        assert_eq!(resolve_server_status("热血传奇", true, &[None]), "0");
    }

    /// **T-8**：`SS_SERVERINFO` 的解析结果必须真的**写回在线槽条目**（四列逐项）。
    ///
    /// 牙：删掉 [`apply_serverinfo`] 的任一行（尤其 `server_index`）⇒ 本用例必红。
    #[test]
    fn serverinfo_report_is_applied_to_online_slot() {
        let mut slot = srv("旧服", 99);
        let report = ServerInfoReport {
            server_name: "热血传奇".to_string(),
            server_index: 0,
            online_count: 7,
            payment_mode: 1,
        };
        apply_serverinfo(&mut slot, &report);
        assert_eq!(slot.server_name, "热血传奇");
        assert_eq!(slot.server_index, 0, "T-8：server_index 必须写回槽位");
        assert_eq!(slot.online_count, 7);
        assert_eq!(slot.payment_mode, 1);
    }
}
