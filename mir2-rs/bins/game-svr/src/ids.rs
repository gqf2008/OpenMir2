//! 登录服务器（IDS）的**会话通知链路** —— C# `GameSrv/Services/AuthenticationService.cs` 的对应物。
//!
//! 拓扑（照 oracle）：GameSvr 是 **TcpClient**（`:11`），**主动连** `IDSAddr:IDSPort`
//! （`Initialize` `:22-34`；现网 `Server.conf` 的 `IDSAddr= 127.0.0.1` / `IDSPort=5600`）。
//! 收到字节 → 按 **GBK** 累加成文本（`DataSocketRead` `:56-68` 的 `HUtil32.GetString`）→
//! `Run()`（`:125-204`）按 `(...)` 分帧、把首段当**消息号**（`GetValidStr3(sData, ref sCode, '/' )`），
//! 再分派到各 handler。
//!
//! 本切片做（认证闭环需要的那两条）：
//! 1. `SS_OPENSESSION`(100) → `GetPasswdSuccess`（`:241-265`）→ **填准入表**（`NewSession`）；
//! 2. `SS_CLOSESESSION`(101) → `GetCancelAdmission`（`:267-281`）→ **删准入**（`DelSession`），
//!    并把被删的 `Account/SessionID` 作为 [`KickRequest`] 交给 sim 线程去**账号级顶号**
//!    （oracle `DelSession` 的下一句 `NetChannel.KickUser`，`AuthenticationService.cs:319`）。
//!
//! 断线：`DataSocketDisconnect`（`:427-436`）会 **`ClearSession()`（清空整张表）** 并记错误日志——照抄。
//!
//! **登记不做**（各需世界/配置，属后续切片）：`SS_KEEPALIVE`(104)/`SS_KICKUSER`(111)/`SS_SERVERLOAD`(113)/
//! `ISM_ACCOUNTEXPIRED`(114)/`ISM_QUERYPLAYTIME`(116)、连上后上报在线数（`DataSocketConnect` `:420-425`）、
//! 断线重连（oracle 这里也不重连；重连由别处驱动，未取证）。

/// IDS 链路自己的日志宏（与 main 的 `glog!` 同形，便于两侧日志按前缀过滤）。
macro_rules! glog_ids {
    ($lvl:expr, $($arg:tt)*) => {{
        let now = ::std::time::SystemTime::now()
            .duration_since(::std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let ms = now.subsec_millis();
        let secs = now.as_secs();
        println!(
            "{:02}:{:02}:{:02}.{ms:03} [{}] {}",
            (secs / 3600) % 24,
            (secs / 60) % 60,
            secs % 60,
            $lvl,
            format!($($arg)*)
        );
    }};
}

use std::sync::{Arc, Mutex};

use mir2_protocol::messages::{SS_CLOSESESSION, SS_OPENSESSION};
use mir2_shared::hutil32::{arrest_string_ex_ref, get_valid_str3, str_to_int, ArrestWrite};

use crate::auth::AdmissionStore;
use crate::GateEvent;

/// 一条**账号级顶号**请求（`SS_CLOSESESSION` 删掉的会话）。
///
/// oracle `DelSession`（`AuthenticationService.cs:299-327`）删掉 `AccountSession` 后调
/// `M2Share.NetChannel.KickUser(sAccount, sessionId, sessInfo?.PayMode ?? 0)`；
/// 注意 `sessInfo` 在 `RemoveAt` 后已被置 `null` ⇒ `payMode` **恒为 0**（永远发 `sKickUserMsg`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KickRequest {
    pub account: String,
    pub session_id: i32,
}

/// 字段分隔符：`HUtil32.Backslash = '/'`（`HUtil32.cs:12`）。
const DIV: [char; 1] = ['/'];

/// 把累积文本按 `(...)` 分帧并分派；返回本次处理的消息条数。
///
/// 逐字照 `Run()`（`:149-199`）的循环：取 `(`…`)` 之间那一段；取不到就停；
/// 处理完再看剩余里还有没有 `)`，没有就停；最后把**剩余**放回缓冲（`:190-197`）。
/// 注意 `ArrestStringEx` 的怪癖：闭合标记之后的内容才是"返回的剩余"（见 `hutil` 的注释）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IdsDrain {
    /// 本次处理的会话通知条数（`SS_OPENSESSION`/`SS_CLOSESESSION` 命中数）。
    pub handled: usize,
    /// 账号级顶号请求（`SS_CLOSESESSION` 删到会话时一条）。
    pub kicks: Vec<KickRequest>,
}

#[must_use]
pub fn drain_messages(buf: &mut String, store: &mut AdmissionStore) -> IdsDrain {
    let mut text = std::mem::take(buf);
    let mut out = IdsDrain::default();
    loop {
        // 三态照抄 `:153-157`：`ArrestStringEx(sSocketText, "(", ")", ref sData)` ——
        // `Value(空)`（找不到 `(` 的越界缺陷分支）与 `Unchanged`（无闭合 `)`）都让 `sData` 不是"新截出的非空内容"
        // ⇒ 循环中断（注意 `sData` 是 `Run()` 的局部量、只在命中时被写：未命中时它保持上一次的值，
        // 而紧随其后的 `IndexOf(')')` 检查会兜住这种情况，故这里等价地按"中断"处理）。
        let (write, rest) = arrest_string_ex_ref(&text, '(', ')');
        let arrested = match write {
            ArrestWrite::Value(v) if !v.is_empty() => v,
            _ => {
                text = rest;
                break;
            }
        };
        out.handled += usize::from(dispatch(&arrested, store, &mut out.kicks));
        text = rest;
        if !text.contains(')') {
            break;
        }
    }
    *buf = text;
    out
}

/// 单条消息的分派（`:158-184` 的 switch）。返回是否被本切片处理。
fn dispatch(arrested: &str, store: &mut AdmissionStore, kicks: &mut Vec<KickRequest>) -> bool {
    let (code_str, body) = get_valid_str3(arrested, &DIV);
    match u16::try_from(str_to_int(&code_str, 0)).unwrap_or(u16::MAX) {
        SS_OPENSESSION => {
            get_passwd_success(&body, store);
            true
        }
        SS_CLOSESESSION => {
            get_cancel_admission(&body, store, kicks);
            true
        }
        other => {
            glog_ids!("DBG", "IDS: 未处理的消息号 {other}（登记在切片清单里）");
            false
        }
    }
}

/// `GetPasswdSuccess`（`:241-265`）：`account/sessionId/payCost/payMode/ip/playTime`。
///
/// 字段顺序由发送端 `LoginSrv/Services/SessionServer.cs` 决定：
/// `Account + "/" + SessionID + "/" + (PayCost ? 1 : 0) + "/" + nPayMode + "/" + UserIPaddr + "/" + Seconds`。
fn get_passwd_success(body: &str, store: &mut AdmissionStore) {
    let (account, r) = get_valid_str3(body, &DIV);
    let (sid, r) = get_valid_str3(&r, &DIV);
    let (pay_cost, r) = get_valid_str3(&r, &DIV);
    let (pay_mode, r) = get_valid_str3(&r, &DIV);
    let (ipaddr, r) = get_valid_str3(&r, &DIV);
    let (play_time, _) = get_valid_str3(&r, &DIV);
    store.new_session(
        &account,
        &ipaddr,
        str_to_int(&sid, 0),
        str_to_int(&pay_cost, 0),
        str_to_int(&pay_mode, 0),
        i64::from(str_to_int(&play_time, 0)),
    );
    glog_ids!(
        "INF",
        "会话准入登记 Account:{account} SessionId:{} PayCost:{pay_cost} PayMode:{pay_mode} IP:{ipaddr}",
        str_to_int(&sid, 0)
    );
}

/// `GetCancelAdmission`（`:267-281`）：体是 `account/sessionId`，而 C# 取的是 `GetValidStr3` 的**返回值**
/// （= 第二段）当 sessionId（首段给 `sC` 后丢弃）。
///
/// 删到会话后还要接**账号级顶号**（`DelSession` 的下一句 `NetChannel.KickUser`，`:319`）：
/// 把被删的账号/会话号交给 sim 线程去发 `SM_SYSMESSAGE` + 软关旧会话（见 `main.rs::handle_kick_account`）。
fn get_cancel_admission(body: &str, store: &mut AdmissionStore, kicks: &mut Vec<KickRequest>) {
    let (_account, session_id_str) = get_valid_str3(body, &DIV);
    let session_id = str_to_int(&session_id_str, 0);
    let removed = store.del_session(session_id);
    glog_ids!(
        "INF",
        "会话准入注销 SessionId:{session_id}（{}）",
        if removed.is_some() {
            "命中"
        } else {
            "不在表里"
        }
    );
    if let Some(sess) = removed {
        kicks.push(KickRequest {
            account: sess.account,
            session_id: sess.session_id,
        });
    }
}

/// 起 IDS 链路（照 `Start()` `:36-54`：连不上**只记日志、不退出进程**）。
///
/// `ev_tx`：把账号级顶号请求（`SS_CLOSESESSION` 删到会话）交给 sim 线程
/// （`GateEvent::KickAccount` ⇒ `handle_kick_account`）。
pub async fn run_ids_link(
    ids_addr: String,
    ids_port: u16,
    store: Arc<Mutex<AdmissionStore>>,
    ev_tx: std::sync::mpsc::Sender<GateEvent>,
) {
    loop {
        match tokio::net::TcpStream::connect((ids_addr.as_str(), ids_port)).await {
            Ok(sock) => {
                sock.set_nodelay(true).ok();
                glog_ids!("INF", "登录服务器[{ids_addr}:{ids_port}]连接成功...");
                if let Err(e) = read_loop(sock, &store, &ev_tx).await {
                    glog_ids!("ERR", "登录服务器链路结束：{e:#}");
                }
                // `DataSocketDisconnect`（`:427-436`）：**清空整张准入表** + 记错误
                store.lock().expect("准入表锁被毒化").clear();
                glog_ids!("ERR", "登录服务器[{ids_addr}]断开连接...（已清空准入表）");
            }
            Err(e) => {
                glog_ids!("ERR", "链接登录服务器[{ids_addr}:{ids_port}]失败...（{e}）");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

/// 收字节 → GBK 文本 → 累积 → 分帧分派。返回 `Err` 表示链路断了（或读失败）。
async fn read_loop(
    mut sock: tokio::net::TcpStream,
    store: &Arc<Mutex<AdmissionStore>>,
    ev_tx: &std::sync::mpsc::Sender<GateEvent>,
) -> anyhow::Result<()> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::with_capacity(4096);
    let mut text = String::new();
    loop {
        buf.resize(4096, 0);
        let n = sock.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        // `DataSocketRead`（`:56-68`）：`HUtil32.GetString(buffer, 0, len)` = GBK 解码
        let (chunk, _, _) = encoding_rs::GBK.decode(&buf[..n]);
        text.push_str(&chunk);
        let drained = drain_messages(&mut text, &mut store.lock().expect("准入表锁被毒化"));
        if drained.handled > 0 {
            glog_ids!("DBG", "IDS: 处理 {} 条会话通知", drained.handled);
        }
        for k in drained.kicks {
            // 送达 sim 线程；sends 不会阻塞（`std::sync::mpsc::channel` 无界）。
            if ev_tx
                .send(GateEvent::KickAccount {
                    account: k.account,
                    session_id: k.session_id,
                })
                .is_err()
            {
                glog_ids!("WRN", "IDS: sim 事件通道已关闭，顶号请求丢弃");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_session_message_fills_admission() {
        let mut store = AdmissionStore::new();
        let mut buf = "(100/loadbot1/2/0/1/127.0.0.1/3600)".to_string();
        assert_eq!(drain_messages(&mut buf, &mut store).handled, 1);
        assert!(buf.is_empty(), "整条被消费 ⇒ 缓冲清空");
        let a = store.get_admission("loadbot1", 2).expect("应已登记");
        assert_eq!(a.session.ipaddr, "127.0.0.1");
        assert_eq!(a.session.pay_ment, 0, "线上的 PayCost=0");
        assert_eq!(a.pay_ment, 1, "映射 0→1");
        assert!(a.is_admitted());
        assert_eq!(a.session.play_time, 3600);
        assert_eq!(a.session.pay_mode, 1);
    }

    /// `SS_CLOSESESSION` 除了删准入，还要产出**账号级顶号请求**（`DelSession` ⇒ `KickUser`）。
    #[test]
    fn close_session_message_removes_admission_and_emits_kick() {
        let mut store = AdmissionStore::new();
        let mut buf = "(100/loadbot1/2/0/1/ip/0)(101/loadbot1/2)".to_string();
        let d = drain_messages(&mut buf, &mut store);
        assert_eq!(d.handled, 2, "两条一起处理（100 无 kick、101 有 kick）");
        assert!(
            store.get_admission("loadbot1", 2).is_none(),
            "101 应删掉登记"
        );
        // 顶号请求带上被删会话的 account/session_id（顺序：账号/会话号，与 `DelSession` 一致）。
        assert_eq!(
            d.kicks,
            vec![KickRequest {
                account: "loadbot1".to_string(),
                session_id: 2
            }]
        );
        assert!(buf.is_empty());
    }

    /// 未命中的 `SS_CLOSESESSION`（表里没有该 sessionId）不得产出顶号请求。
    #[test]
    fn close_session_miss_emits_no_kick() {
        let mut store = AdmissionStore::new();
        let mut buf = "(101/nobody/99)".to_string();
        assert!(drain_messages(&mut buf, &mut store).kicks.is_empty());
    }

    #[test]
    fn partial_frame_stays_in_buffer() {
        let mut store = AdmissionStore::new();
        let mut buf = "(100/loadbot1/2/0/1/ip/0)(100/loadbo".to_string();
        assert_eq!(drain_messages(&mut buf, &mut store).handled, 1);
        assert_eq!(buf, "(100/loadbo", "未闭合的那段必须留在缓冲里");
        // 补齐后能被处理
        buf.push_str("t2/3/1/0/ip2/60)");
        assert_eq!(drain_messages(&mut buf, &mut store).handled, 1);
        assert!(store.get_admission("loadbot2", 3).is_some());
        assert!(
            store.get_admission("loadbot2", 3).unwrap().is_admitted(),
            "PayCost=1 ⇒ 映射 1→2 ⇒ 通过"
        );
        assert!(buf.is_empty());
    }

    #[test]
    fn unknown_code_and_garbage_are_ignored() {
        let mut store = AdmissionStore::new();
        let mut buf = "(104/whatever)(1/2/3)".to_string();
        assert_eq!(
            drain_messages(&mut buf, &mut store).handled,
            0,
            "未登记的消息号不计入"
        );
        assert!(buf.is_empty());
        // 不含 `(` 的垃圾：照 oracle 的越界分支 ⇒ 整段被丢（`:527-545` 的 catch 语义）
        let mut garbage = "abc".to_string();
        assert_eq!(drain_messages(&mut garbage, &mut store).handled, 0);
        assert!(garbage.is_empty(), "照 oracle 会丢干净");
    }
}
