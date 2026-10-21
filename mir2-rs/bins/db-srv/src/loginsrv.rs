//! DBSrv 主动连 LoginSrv 的**会话同步链**（C# `DBSrv/Services/Impl/ClientSession.cs`）。
//!
//! 线上是纯文本行内帧：`(标识/字段/字段…)`，未加密、不帧长（靠 `)` 定界，跨 TCP 段累积）。
//! - `100 SS_OPENSESSION`：`账号/会话号/PayCost/PayMode/玩家IP/秒数` —— 由 LoginSrv 在
//!   **客户端选服**时下发给所有 `ServerIndex==99` 的连接（DBSrv 即此）；
//! - `101 SS_CLOSESESSION`：`账号/会话号`；
//! - `104 SS_KEEPALIVE`：LoginSrv 广播在线数，DBSrv 收到只当续命（C# `ProcessGetOnlineCount` 空实现）；
//! - 下行 `103 SS_SERVERINFO`：DBSrv 每 7s 上报 `(103/服务器名/99/在线数)`；
//! - 下行 `102 SS_SOFTOUTSESSION`：`账号/会话号`（角色离开时通知 LoginSrv 释放会话）。
//!
//! 连接失败**不致命**：C# 只打日志，`TimedService` 每 10s 重连一次（`UseReconnection`）；
//! 这里同样按 10s 重试。
//!
//! **断线不清会话表**（B-6，对齐 C# `ClientSession.LoginSocketDisconnected`，`ClientSession.cs:64-68`
//! 只打日志）：C# 的 `_globaSessionList` 跨链路断线保留，只有 `Stop()`（进程退出，
//! `AppService.cs:58-63`）才逐个置空。因此链路断开期间 `CheckSession` **仍然可能为真**；
//! 只有进程从未收到过 `(100/…)`（或 `Stop()`）时才是假。

use std::sync::Arc;
use std::time::Duration;

use mir2_protocol::messages::{
    SS_CLOSESESSION, SS_KEEPALIVE, SS_OPENSESSION, SS_SERVERINFO, SS_SOFTOUTSESSION,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::gbk::{arrest, split2_nonempty, str_to_int};
use crate::log;
use crate::server::Server;

/// 一条已登录会话（C# `GlobaSessionInfo`）。
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// 账号（原始字节，比较是**大小写敏感**的，与 C# `==` 一致）。
    pub account: Vec<u8>,
    /// 会话号（客户端登录拿到的那个）。
    pub session_id: i32,
    /// 是否已进入游戏（LoginSrv 侧 `StartPlay`）。
    pub start_play: bool,
    /// 本会话是否已取过存档（C# `GlobaSessionInfo.LoadRcd`）。
    ///
    /// 由 6000 面的 `CheckSessionLoadRcd`（取档成功一次后置真）/ `SetSessionSaveRcd`（存档后置假）读写，
    /// 5100 面不读它。
    pub load_rcd: bool,
}

// 注：C# `GlobaSessionInfo.IPaddr` 不被 5100/6000 面读取（`CheckSession`/`CheckSessionLoadRcd`
// 只比账号+会话号，入参 IP 被忽略）⇒ 镜像不保留该字段。

/// 会话表 + 与 LoginSrv 的发送端。
#[derive(Default)]
pub struct LoginState {
    /// 会话列表（顺序＝接入顺序，C# 是 `List`）。
    pub sessions: Vec<SessionInfo>,
    /// 到 LoginSrv 的写端（未连接时为 `None`）。
    pub tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
}

impl LoginState {
    /// C# `CheckSession`：账号与会话号都要对上（账号**大小写敏感**）。
    #[must_use]
    pub fn check_session(&self, account: &[u8], session_id: i32) -> bool {
        self.sessions
            .iter()
            .any(|s| s.account == account && s.session_id == session_id)
    }

    /// C# `SetGlobaSessionNoPlay`。
    pub fn set_no_play(&mut self, session_id: i32) {
        if let Some(s) = self
            .sessions
            .iter_mut()
            .find(|s| s.session_id == session_id)
        {
            s.start_play = false;
        }
    }

    /// C# `SetGlobaSessionPlay`。
    pub fn set_play(&mut self, session_id: i32) {
        if let Some(s) = self
            .sessions
            .iter_mut()
            .find(|s| s.session_id == session_id)
        {
            s.start_play = true;
        }
    }

    /// C# `GetGlobaSessionStatus`：找不到会话即假。
    #[must_use]
    pub fn get_play(&self, session_id: i32) -> bool {
        self.sessions
            .iter()
            .find(|s| s.session_id == session_id)
            .is_some_and(|s| s.start_play)
    }

    /// C# `CloseSession`：账号与会话号都对上才移除。
    pub fn close_session(&mut self, account: &[u8], session_id: i32) {
        if let Some(i) = self
            .sessions
            .iter()
            .position(|s| s.session_id == session_id && s.account == account)
        {
            self.sessions.remove(i);
        }
    }

    /// C# `ProcessAddSession`：`s18`/`s14` 两段被 C# 忽略，这里也只取账号/会话号/IP。
    pub fn add_session(&mut self, account: Vec<u8>, session_id: i32, _ip: Vec<u8>) {
        // 5100 面不用 IP（`CheckSession` 只比账号+会话号）；载荷里的 IP 由调用方日志打印
        self.sessions.push(SessionInfo {
            account,
            session_id,
            start_play: false,
            load_rcd: false,
        });
    }

    /// C# `CheckSessionLoadRcd`：命中会话即 `found=true`；仅当该会话 `LoadRcd` 还是 false 时
    /// 置真并返回 `1`，否则返回 `-1`。`sIPaddr` 入参在 C# 里被忽略，这里同样忽略。
    #[must_use]
    pub fn check_session_load_rcd(&mut self, account: &[u8], session_id: i32) -> (bool, i32) {
        let mut found = false;
        let mut code = -1;
        for s in &mut self.sessions {
            if s.account == account && s.session_id == session_id {
                found = true;
                if !s.load_rcd {
                    s.load_rcd = true;
                    code = 1;
                }
                break;
            }
        }
        (found, code)
    }

    /// C# `SetSessionSaveRcd`：把该账号名下所有会话的 `LoadRcd` 置假（存档后允许再次取档）。
    pub fn set_session_save_rcd(&mut self, account: &[u8]) -> bool {
        let mut result = false;
        for s in &mut self.sessions {
            if s.account == account {
                s.load_rcd = false;
                result = true;
            }
        }
        result
    }

    /// C# `ProcessDelSession`。
    pub fn del_session(&mut self, account: &[u8], session_id: i32) {
        if let Some(i) = self
            .sessions
            .iter()
            .position(|s| s.session_id == session_id && s.account == account)
        {
            self.sessions.remove(i);
        }
    }

    /// C# `SendSocketMsg`：`(标识/内容)`。
    pub fn send_soft_out(&self, account: &[u8], session_id: i32) {
        if let Some(tx) = &self.tx {
            let mut msg = format!("({SS_SOFTOUTSESSION}/").into_bytes();
            msg.extend_from_slice(account);
            msg.push(b'/');
            msg.extend_from_slice(session_id.to_string().as_bytes());
            msg.push(b')');
            let _ = tx.send(msg);
        }
    }

    /// 解析一段来自 LoginSrv 的文本（C# `ProcessSocketMsg`）。
    pub fn feed(&mut self, text: &mut Vec<u8>) {
        // C# `while (sScoketText.IndexOf(")") > 0)`：`)` 落在**下标 0** 不算（左括号必须在前）。
        while text.iter().position(|b| *b == b')').is_some_and(|i| i > 0) {
            let (data, rest) = arrest(text, b'(', b')');
            *text = rest; // C# 先把 ArrestStringEx 的返回值写回缓冲，**再**判空/分发
            if data.is_empty() {
                break;
            }
            self.dispatch(&data);
        }
    }

    fn dispatch(&mut self, data: &[u8]) {
        let (code_b, body) = split2_nonempty(data, b"/");
        let code = str_to_int(&code_b, 0);
        if code == i32::from(SS_OPENSESSION) {
            // 账号/会话号/PayCost/PayMode/IP/秒数
            let (account, rest) = split2_nonempty(&body, b"/");
            let (sid_b, rest) = split2_nonempty(&rest, b"/");
            let (_pay_cost, rest) = split2_nonempty(&rest, b"/");
            let (_pay_mode, rest) = split2_nonempty(&rest, b"/");
            let (ip, _) = split2_nonempty(&rest, b"/");
            let sid = str_to_int(&sid_b, 0);
            log::debug(&format!(
                "同步账号服务[{}]同步会话消息... (SessionID={sid} ip={})",
                String::from_utf8_lossy(&account),
                String::from_utf8_lossy(&ip)
            ));
            self.add_session(account, sid, ip);
        } else if code == i32::from(SS_CLOSESESSION) {
            let (account, rest) = split2_nonempty(&body, b"/");
            let sid = str_to_int(&rest, 0);
            self.del_session(&account, sid);
        } else if code == i32::from(SS_KEEPALIVE) {
            // 104 SS_KEEPALIVE：C# `ProcessGetOnlineCount` 是空实现
        } else {
            log::debug(&format!("登录服务未知消息号：{code}"));
        }
    }
}

/// C# `TimedService.ExecuteAsync` 的重连节拍（10s）。
const RECONNECT_DELAY: Duration = Duration::from_secs(10);

/// LoginSrv 连接任务：连不上就每 10s 重试；连上后按行解析 + 每 7s 上报在线数。
pub async fn login_link_task(server: Arc<Server>) {
    login_link_task_with(server, RECONNECT_DELAY).await;
}

/// 同 [`login_link_task`]，但重连节拍可注入（单元测试用；生产恒为 10s）。
pub(crate) async fn login_link_task_with(server: Arc<Server>, retry: Duration) {
    let cfg = server.cfg.clone();
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let addr = format!("{}:{}", cfg.ids_addr, cfg.ids_port);
        let stream = match TcpStream::connect(&addr).await {
            Ok(s) => s,
            Err(e) => {
                log::error(&format!("账号服务器[{addr}]链接失败({e})...10s 后重试"));
                {
                    let mut st = server.login.lock().await;
                    st.tx = None;
                }
                tokio::time::sleep(retry).await;
                continue;
            }
        };
        stream.set_nodelay(true).ok();
        log::info(&format!("账号服务器[{addr}]链接成功."));
        let (mut rd, mut wr) = stream.into_split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        {
            let mut st = server.login.lock().await;
            st.tx = Some(tx);
        }
        buf.clear();
        let mut last_keep = tokio::time::Instant::now();
        let mut tick = tokio::time::interval(Duration::from_millis(1000));
        loop {
            tokio::select! {
                read = rd.read_buf(&mut buf) => {
                    match read {
                        Ok(0) | Err(_) => {
                            log::error(&format!("账号服务器[{addr}]断开链接."));
                            break;
                        }
                        Ok(_) => {
                            let mut st = server.login.lock().await;
                            let mut text = std::mem::take(&mut buf);
                            st.feed(&mut text);
                            buf = text;
                        }
                    }
                }
                out = rx.recv() => {
                    if let Some(m) = out {
                        if wr.write_all(&m).await.is_err() {
                            log::error(&format!("账号服务器[{addr}]写失败."));
                            break;
                        }
                        wr.flush().await.ok();
                    }
                }
                _ = tick.tick() => {
                    // C# `TimedService`：每 7000ms 发一次 `(103/服务器名/99/在线数)`
                    if last_keep.elapsed() >= Duration::from_millis(7000) {
                        last_keep = tokio::time::Instant::now();
                        let users = server.user_count().await;
                        let mut m = format!("({SS_SERVERINFO}/").into_bytes();
                        m.extend_from_slice(&cfg.server_name.wire_bytes());
                        m.extend_from_slice(b"/99/");
                        m.extend_from_slice(users.to_string().as_bytes());
                        m.push(b')');
                        if wr.write_all(&m).await.is_err() {
                            break;
                        }
                        wr.flush().await.ok();
                    }
                }
            }
        }
        // C# `LoginSocketDisconnected`（`ClientSession.cs:64-68`）只打日志：`_globaSessionList`
        // **跨断线保留**，只有进程 `Stop()` 才清（`AppService.cs:58-63`）。重连由
        // `TimedService` 每 10s `Start()` 完成，新会话只追加、不重放旧会话。
        {
            let mut st = server.login.lock().await;
            st.tx = None;
        }
        tokio::time::sleep(retry).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    fn test_server(port: u16) -> (Arc<Server>, mpsc::UnboundedReceiver<crate::server::Queued>) {
        Server::new(
            crate::config::for_test("127.0.0.1".to_string(), port),
            Arc::new(crate::route::Share::default()),
            Arc::new(tokio::sync::Mutex::new(crate::cache::CacheStore::default())),
        )
    }

    async fn wait_until<F, Fut>(what: &str, mut cond: F)
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        for _ in 0..1000 {
            if cond().await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("等待超时：{what}");
    }

    /// 牙 1（B-6）：链路断开**不得清会话表**（C# `LoginSocketDisconnected` 只打日志，
    /// `_globaSessionList` 跨断线保留，只有进程 `Stop()` 才清）。
    /// 旧实现在断链收尾里 `sessions.clear()`，本用例必红。
    #[tokio::test]
    async fn disconnect_keeps_session_table() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (server, _queue_rx) = test_server(port);
        let task = tokio::spawn(login_link_task_with(
            Arc::clone(&server),
            Duration::from_millis(20),
        ));

        let (mut link, _) = listener.accept().await.unwrap();
        link.write_all(b"(100/acc/7/0/0/1.2.3.4/0)").await.unwrap();
        wait_until("首链会话入表", || async {
            server.login.lock().await.check_session(b"acc", 7)
        })
        .await;

        drop(link);
        wait_until("断链后发送端释放", || async {
            server.login.lock().await.tx.is_none()
        })
        .await;

        let st = server.login.lock().await;
        assert!(
            st.check_session(b"acc", 7),
            "B-6：链路断开不得清空会话表（C# ClientSession.cs:64-68 只打日志）"
        );
        assert_eq!(st.sessions.len(), 1, "断线不得清表（C# 只在 Stop() 清）");
        drop(st);
        task.abort();
    }

    /// 牙 2（B-6）：重连后旧会话仍在、新会话只追加一次（不重放、不重复），
    /// 且旧会话仍可被 `(101/…)` 正常关闭。
    #[tokio::test]
    async fn reconnect_appends_without_replaying_or_duplicating() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (server, _queue_rx) = test_server(port);
        let task = tokio::spawn(login_link_task_with(
            Arc::clone(&server),
            Duration::from_millis(20),
        ));

        let (mut link, _) = listener.accept().await.unwrap();
        link.write_all(b"(100/acc/7/0/0/1.2.3.4/0)").await.unwrap();
        wait_until("首链会话入表", || async {
            server.login.lock().await.check_session(b"acc", 7)
        })
        .await;
        drop(link);
        wait_until("断链后发送端释放", || async {
            server.login.lock().await.tx.is_none()
        })
        .await;

        let (mut link2, _) = listener.accept().await.unwrap();
        link2
            .write_all(b"(100/acc2/8/0/0/5.6.7.8/0)")
            .await
            .unwrap();
        wait_until("重连后新会话入表", || async {
            server.login.lock().await.check_session(b"acc2", 8)
        })
        .await;

        {
            let st = server.login.lock().await;
            assert_eq!(
                st.sessions.len(),
                2,
                "重连只追加新会话：不得重放/重复旧会话，也不得丢掉旧会话"
            );
            assert!(st.check_session(b"acc", 7));
            assert!(st.check_session(b"acc2", 8));
        }

        // 旧会话在新链路上仍可被 LoginSrv 的 `(101/…)` 关闭。
        link2.write_all(b"(101/acc/7)").await.unwrap();
        wait_until("旧会话可关闭", || async {
            !server.login.lock().await.check_session(b"acc", 7)
        })
        .await;
        let st = server.login.lock().await;
        assert_eq!(st.sessions.len(), 1);
        assert!(st.check_session(b"acc2", 8));
        drop(st);
        task.abort();
    }

    /// 牙 3（B-6）：断链窗口内 `send_soft_out` 只丢队列（`tx=None`），不清会话；
    /// 重连后发送端被重新接线，旧会话的软下线能真正写到新链路。
    #[tokio::test]
    async fn soft_out_rewires_to_new_link_and_keeps_sessions() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (server, _queue_rx) = test_server(port);
        let task = tokio::spawn(login_link_task_with(
            Arc::clone(&server),
            Duration::from_millis(20),
        ));

        let (mut link, _) = listener.accept().await.unwrap();
        link.write_all(b"(100/acc/7/0/0/1.2.3.4/0)").await.unwrap();
        wait_until("首链会话入表", || async {
            server.login.lock().await.check_session(b"acc", 7)
        })
        .await;
        drop(link);
        wait_until("断链后发送端释放", || async {
            server.login.lock().await.tx.is_none()
        })
        .await;
        {
            let st = server.login.lock().await;
            // 断链窗口：无发送端 ⇒ 静默丢弃，不得 panic 或清表。
            st.send_soft_out(b"acc", 7);
            assert!(st.check_session(b"acc", 7));
        }

        let (mut link2, _) = listener.accept().await.unwrap();
        wait_until("重连后发送端接线", || async {
            server.login.lock().await.tx.is_some()
        })
        .await;
        server.login.lock().await.send_soft_out(b"acc", 7);

        let mut buf = [0u8; 64];
        let n = tokio::time::timeout(Duration::from_secs(2), link2.read(&mut buf))
            .await
            .expect("重连后应收到软下线报文")
            .unwrap();
        assert_eq!(
            &buf[..n],
            b"(102/acc/7)",
            "C# `SendSocketMsg`：`(标识/内容)`"
        );
        task.abort();
    }

    /// LoginSrv 的三条报文（形态取自 `SessionServer.SendServerMsg`）。
    #[test]
    fn parses_login_srv_messages() {
        let mut st = LoginState::default();
        st.feed(&mut b"(103/LegendOfMir2/99/0)".to_vec());
        st.feed(&mut b"(100/mir2test/4242/0/0/127.0.0.1/0)".to_vec());
        assert!(st.check_session(b"mir2test", 4242));
        assert!(!st.check_session(b"MIR2TEST", 4242), "账号比较大小写敏感");
        assert!(!st.check_session(b"mir2test", 1));
        assert!(!st.get_play(4242));
        st.set_play(4242);
        assert!(st.get_play(4242));
        st.set_no_play(4242);
        assert!(!st.get_play(4242));
        st.feed(&mut b"(104/1)".to_vec());
        assert!(st.check_session(b"mir2test", 4242));
        st.feed(&mut b"(101/mir2test/4242)".to_vec());
        assert!(!st.check_session(b"mir2test", 4242));
    }

    /// 跨 TCP 段的拆包（C# 靠 `)` 定界累积）。
    #[test]
    fn accumulates_across_chunks() {
        let mut st = LoginState::default();
        let mut text = b"(100/acc/7/0/0/1.2.3.4".to_vec();
        st.feed(&mut text);
        assert!(!text.is_empty(), "缺 ')' 不消费");
        text.extend_from_slice(b"/0)(100/acc2/8/0/0/5.6.7.8/0)");
        st.feed(&mut text);
        assert!(st.check_session(b"acc", 7));
        assert!(st.check_session(b"acc2", 8));
        assert!(text.is_empty());
    }

    /// C# 的边界口径：`)` 在下标 0 不算（`IndexOf(")") > 0`）；空载荷 `()` 会把 `(` 之前的前缀
    /// **消费掉**（`ArrestStringEx` 的返回值先写回缓冲，再判空 break），不会卡在同一段文本上。
    #[test]
    fn feed_matches_csharp_edge_cases() {
        let mut st = LoginState::default();
        let mut junk = b")garbage".to_vec();
        st.feed(&mut junk);
        assert_eq!(junk, b")garbage".to_vec(), "下标 0 的 ')' 不消费");
        let mut text = b"()".to_vec();
        st.feed(&mut text);
        assert!(text.is_empty(), "空载荷也要消费掉括号");
        let mut text2 = b"(100/acc/5/0/0/1.1.1.1/0)".to_vec();
        st.feed(&mut text2);
        assert!(text2.is_empty());
        assert!(st.check_session(b"acc", 5));
    }

    /// 离线时软下线报文只丢进队列（无连接即丢弃）。
    #[test]
    fn soft_out_requires_link() {
        let mut st = LoginState::default();
        st.add_session(b"acc".to_vec(), 3, b"1.1.1.1".to_vec());
        st.send_soft_out(b"acc", 3); // 无 tx：不 panic
        let (tx, mut rx) = mpsc::unbounded_channel();
        st.tx = Some(tx);
        st.send_soft_out(b"acc", 3);
        assert_eq!(rx.try_recv().unwrap(), b"(102/acc/3)".to_vec());
    }
}
