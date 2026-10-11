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
//! 2. `SS_CLOSESESSION`(101) → `GetCancelAdmission`（`:267-281`）→ **删准入**（`DelSession`）。
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
use mir2_shared::hutil32::get_valid_str3;

use crate::auth::AdmissionStore;
use crate::hutil::{arrest_string_ex, str_to_int};

/// 字段分隔符：`HUtil32.Backslash = '/'`（`HUtil32.cs:12`）。
const DIV: [char; 1] = ['/'];

/// 把累积文本按 `(...)` 分帧并分派；返回本次处理的消息条数。
///
/// 逐字照 `Run()`（`:149-199`）的循环：取 `(`…`)` 之间那一段；取不到就停；
/// 处理完再看剩余里还有没有 `)`，没有就停；最后把**剩余**放回缓冲（`:190-197`）。
/// 注意 `ArrestStringEx` 的怪癖：闭合标记之后的内容才是"返回的剩余"（见 `hutil` 的注释）。
#[must_use]
pub fn drain_messages(buf: &mut String, store: &mut AdmissionStore) -> usize {
    let mut text = std::mem::take(buf);
    let mut handled = 0usize;
    loop {
        let (rest, arrested) = arrest_string_ex(&text, "(", ")");
        if arrested.is_empty() {
            text = rest;
            break;
        }
        handled += usize::from(dispatch(&arrested, store));
        text = rest;
        if !text.contains(')') {
            break;
        }
    }
    *buf = text;
    handled
}

/// 单条消息的分派（`:158-184` 的 switch）。返回是否被本切片处理。
fn dispatch(arrested: &str, store: &mut AdmissionStore) -> bool {
    let (code_str, body) = get_valid_str3(arrested, &DIV);
    match u16::try_from(str_to_int(&code_str, 0)).unwrap_or(u16::MAX) {
        SS_OPENSESSION => {
            get_passwd_success(&body, store);
            true
        }
        SS_CLOSESESSION => {
            get_cancel_admission(&body, store);
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
fn get_cancel_admission(body: &str, store: &mut AdmissionStore) {
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
}

/// 起 IDS 链路（照 `Start()` `:36-54`：连不上**只记日志、不退出进程**）。
pub async fn run_ids_link(ids_addr: String, ids_port: u16, store: Arc<Mutex<AdmissionStore>>) {
    loop {
        match tokio::net::TcpStream::connect((ids_addr.as_str(), ids_port)).await {
            Ok(sock) => {
                sock.set_nodelay(true).ok();
                glog_ids!("INF", "登录服务器[{ids_addr}:{ids_port}]连接成功...");
                if let Err(e) = read_loop(sock, &store).await {
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
        let handled = drain_messages(&mut text, &mut store.lock().expect("准入表锁被毒化"));
        if handled > 0 {
            glog_ids!("DBG", "IDS: 处理 {handled} 条会话通知");
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
        assert_eq!(drain_messages(&mut buf, &mut store), 1);
        assert!(buf.is_empty(), "整条被消费 ⇒ 缓冲清空");
        let a = store.get_admission("loadbot1", 2).expect("应已登记");
        assert_eq!(a.session.ipaddr, "127.0.0.1");
        assert_eq!(a.session.pay_ment, 0, "线上的 PayCost=0");
        assert_eq!(a.pay_ment, 1, "映射 0→1");
        assert!(a.is_admitted());
        assert_eq!(a.session.play_time, 3600);
        assert_eq!(a.session.pay_mode, 1);
    }

    #[test]
    fn close_session_message_removes_admission() {
        let mut store = AdmissionStore::new();
        let mut buf = "(100/loadbot1/2/0/1/ip/0)(101/loadbot1/2)".to_string();
        assert_eq!(drain_messages(&mut buf, &mut store), 2, "两条一起处理");
        assert!(buf.is_empty());
        assert!(
            store.get_admission("loadbot1", 2).is_none(),
            "101 应删掉登记"
        );
    }

    #[test]
    fn partial_frame_stays_in_buffer() {
        let mut store = AdmissionStore::new();
        let mut buf = "(100/loadbot1/2/0/1/ip/0)(100/loadbo".to_string();
        assert_eq!(drain_messages(&mut buf, &mut store), 1);
        assert_eq!(buf, "(100/loadbo", "未闭合的那段必须留在缓冲里");
        // 补齐后能被处理
        buf.push_str("t2/3/1/0/ip2/60)");
        assert_eq!(drain_messages(&mut buf, &mut store), 1);
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
            drain_messages(&mut buf, &mut store),
            0,
            "未登记的消息号不计入"
        );
        assert!(buf.is_empty());
        // 不含 `(` 的垃圾：照 oracle 的越界分支 ⇒ 整段被丢（`:527-545` 的 catch 语义）
        let mut garbage = "abc".to_string();
        assert_eq!(drain_messages(&mut garbage, &mut store), 0);
        assert!(garbage.is_empty(), "照 oracle 会丢干净");
    }
}
