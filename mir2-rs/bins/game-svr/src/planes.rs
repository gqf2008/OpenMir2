//! 跨服链路（G11 / A3-2b）：把 `mir2-planes` 的客户端状态机挂到 tokio 传输上。
//!
//! # 为什么只有客户端
//!
//! 设计文档 §5.2 用 C# 探针实测过：oracle `PlanesServer.StartPlanesServer()` 只调
//! `Setup(cfg)`、**没有 `Start()`** ⇒ 主服从不监听（`ServerState=None`，客户端连接超时）。
//! 本模块**如实移植**：`ServerIndex == 0` 时什么都不做，连日志都不打——oracle 里那行
//! `节点数据服务[...]已启动` 本身就是注释。
//!
//! # 客户端行为（逐条对 oracle）
//!
//! | oracle | 这里 |
//! |---|---|
//! | `PlanesService.cs:18` 非 0 分支打「节点运行模式…」 | [`node_mode_line`] |
//! | `PlanesClient.Start()` 的 `ConnectAsync` | [`connect_once`] 的 `TcpStream::connect` + 5s 超时 |
//! | 失败/超时两条 catch | [`connect_failed_line`] / [`connect_timeout_line`] |
//! | 连上打「连接主服务器(ip:port)成功...」 | [`connected_line`] |
//! | 断开打「节点服务器(ip:port)断开连接...」 | [`disconnected_line`] |
//! | `src/GameSrv/TimedService.cs:63-71`：1s 定时器、距上次检查 > 10s 才检查 | [`check_due`] |
//! | `CheckConnected()`：`nServerNumber > 0` 且离线才重连 | [`mir2_planes::client::Client::should_reconnect`] |
//!
//! **心跳不实现**：oracle 全仓没有发送点（设计文档 §8）。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mir2_planes::client::{Client, Event};
use mir2_planes::service::{role_of, Role};

/// oracle `TimedService` 的定时器周期（`src/GameSrv/TimedService.cs`：`PeriodicTimer(1s)`）。
const TIMER_PERIOD: Duration = Duration::from_secs(1);
/// oracle `src/GameSrv/TimedService.cs:63` 的检查阈值：`(currentTick - CheckIntervalTime) > 10 * 1000`。
const CHECK_THRESHOLD_MS: u64 = 10_000;
/// `ConnectAsync` 的超时。oracle 没显式设置（TouchSocket 缺省值不在本仓），
/// 这里取 5s，只为把「超时」与「其它失败」分成 oracle 的两条 catch 分支。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// 「距上次检查超过 10s」——oracle `src/GameSrv/TimedService.cs:63` 的条件（**严格大于**）。
#[must_use]
pub fn check_due(now_ms: u64, last_check_ms: u64) -> bool {
    now_ms.saturating_sub(last_check_ms) > CHECK_THRESHOLD_MS
}

/// 「节点运行模式...主机端口:[addr:port]」（`PlanesService.cs:18`）。
#[must_use]
pub fn node_mode_line(addr: &str, port: i32) -> String {
    format!("节点运行模式...主机端口:[{addr}:{port}]")
}

/// 「链接主分区服务器[addr:port]失败.」（`PlanesClient.cs:69`）。
#[must_use]
pub fn connect_failed_line(addr: &str, port: i32) -> String {
    format!("链接主分区服务器[{addr}:{port}]失败.")
}

/// 「链接主分区服务器[addr:port]超时.」（`PlanesClient.cs:65`）。
#[must_use]
pub fn connect_timeout_line(addr: &str, port: i32) -> String {
    format!("链接主分区服务器[{addr}:{port}]超时.")
}

/// 「连接主服务器(ip:port)成功...」（`PlanesClient.cs:139`）。
#[must_use]
pub fn connected_line(peer: &str) -> String {
    format!("连接主服务器({peer})成功...")
}

/// 「节点服务器(ip:port)断开连接...」（`PlanesClient.cs:146`）。
#[must_use]
pub fn disconnected_line(peer: &str) -> String {
    format!("节点服务器({peer})断开连接...")
}

/// 跨服链路句柄。主服角色下 [`start`] 返回 `None`（oracle 不监听）。
#[derive(Debug, Clone)]
pub struct PlanesLink {
    state: Arc<Mutex<Client>>,
}

impl PlanesLink {
    /// 对应 oracle `SystemProcessor.cs:42-49` 每轮的 `PlanesClient.Run()`：**只在在线时解码**。
    ///
    /// `ProcessData` 在本 oracle 里没有副作用（设计文档 §4.2），所以本函数除「211 抛异常」
    /// 时记一条客户端日志外什么也不做——保留调用点，A4 真正接线时语义位置不变。
    pub fn tick(&self) {
        let events = match self.state.lock() {
            Ok(mut st) => st.tick(),
            Err(_) => return,
        };
        for e in events {
            if e == Event::Aborted {
                // oracle `PlanesClient.cs:132-136`：catch 里 LogService.Error 两次。
                crate::glog!(
                    "ERR",
                    "[Exception] TFrmSrvMsg::DecodeSocStr（211 抛异常，本批剩余帧与缓冲已丢弃）"
                );
            }
        }
    }

    /// 当前是否在线（测试与排障用）。
    #[must_use]
    pub fn is_online(&self) -> bool {
        self.state.lock().map(|s| s.is_online()).unwrap_or(false)
    }
}

/// 按角色启动跨服链路。
///
/// - `server_index == 0` ⇒ `None`（**不监听**，见模块文档与设计文档 §5.2）；
/// - 否则起一个客户端任务，按 oracle 的 1s/10s 节拍重连。
///
/// `master_port` 在 oracle 里是 `int`；超出 `u16` 时 oracle 会在 `IPHost` 构造抛异常。
/// 这里不 panic：记一条 `连接…失败.` 后不再尝试。
#[must_use]
pub fn start(
    server_index: u8,
    master_addr: &str,
    master_port: i32,
    server_number: i32,
) -> Option<PlanesLink> {
    match role_of(server_index) {
        Role::Server => None,
        Role::Client => Some(spawn_client(
            master_addr.to_owned(),
            master_port,
            server_number,
        )),
    }
}

fn spawn_client(addr: String, port: i32, server_number: i32) -> PlanesLink {
    let link = PlanesLink {
        state: Arc::new(Mutex::new(Client::new())),
    };
    let state = Arc::clone(&link.state);
    crate::glog!("INF", "{}", node_mode_line(&addr, port));
    tokio::spawn(async move {
        let Ok(port_u16) = u16::try_from(port) else {
            // oracle：`new IPHost(addr, port)` 端口越界抛异常，进程起不来。这里记 ERR 后放弃。
            crate::glog!("ERR", "{}", connect_failed_line(&addr, port));
            return;
        };
        let mut timer = tokio::time::interval(TIMER_PERIOD);
        // oracle `src/GameSvr/TimedService.cs` 的 `StartAsync` 把 `CheckIntervalTime`
        // 初始化成**启动时的 tick** ⇒ 首次检查要等 >10s（1s 定时器下约第 11 秒）。
        // 若初始化成 0，会在启动第 1 秒就抢连一次，是**可观察的时序差异**。
        let mut last_check: u64 = now_ms();
        loop {
            timer.tick().await;
            let now = now_ms();
            if !check_due(now, last_check) {
                continue;
            }
            last_check = now;
            let should = state
                .lock()
                .map(|s| s.should_reconnect(server_number))
                .unwrap_or(false);
            if should {
                connect_once(&state, &addr, port, port_u16).await;
            }
        }
    });
    link
}

async fn connect_once(state: &Arc<Mutex<Client>>, addr: &str, port: i32, port_u16: u16) {
    let target = format!("{addr}:{port}");
    let attempt = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio::net::TcpStream::connect((addr, port_u16)),
    )
    .await;
    let mut stream = match attempt {
        Err(_) => {
            crate::glog!("ERR", "{}", connect_timeout_line(addr, port));
            return;
        }
        Ok(Err(_)) => {
            crate::glog!("ERR", "{}", connect_failed_line(addr, port));
            return;
        }
        Ok(Ok(s)) => s,
    };
    if let Ok(mut st) = state.lock() {
        st.set_online(true);
    }
    crate::glog!("INF", "{}", connected_line(&target));
    let state = Arc::clone(state);
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt as _;
        let mut buf = [0_u8; 8192];
        loop {
            match stream.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Ok(mut st) = state.lock() {
                        st.received(&buf[..n]);
                    }
                }
            }
        }
        if let Ok(mut st) = state.lock() {
            st.set_online(false);
        }
        crate::glog!("ERR", "{}", disconnected_line(&target));
    });
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_due_is_strictly_greater_than_ten_seconds() {
        assert!(!check_due(10_000, 0), "恰好 10s 不触发（oracle 是 >）");
        assert!(check_due(10_001, 0));
        assert!(!check_due(9_999, 5_000));
        assert!(check_due(20_000, 5_000));
        // 时钟回退：不触发而不是溢出
        assert!(!check_due(1_000, 5_000));
    }

    #[test]
    fn log_lines_match_oracle_wording() {
        assert_eq!(
            node_mode_line("127.0.0.1", 4900),
            "节点运行模式...主机端口:[127.0.0.1:4900]"
        );
        assert_eq!(
            connect_failed_line("10.0.0.7", 4900),
            "链接主分区服务器[10.0.0.7:4900]失败."
        );
        assert_eq!(
            connect_timeout_line("10.0.0.7", 4900),
            "链接主分区服务器[10.0.0.7:4900]超时."
        );
        assert_eq!(
            connected_line("10.0.0.7:4900"),
            "连接主服务器(10.0.0.7:4900)成功..."
        );
        assert_eq!(
            disconnected_line("10.0.0.7:4900"),
            "节点服务器(10.0.0.7:4900)断开连接..."
        );
    }

    /// 主服角色**不起监听、也不返回句柄**（设计文档 §5.2 的实测结论）。
    #[test]
    fn server_role_does_not_listen() {
        assert!(start(0, "127.0.0.1", 4900, 0).is_none());
        assert!(start(0, "127.0.0.1", 4900, 3).is_none());
    }

    /// 真传输：连上本机监听器 ⇒ 在线；收到字节 ⇒ 进缓冲；对端关闭 ⇒ 离线。
    ///
    /// 只做真实 socket 往返，不卡时间上界（用有界轮询，避免墙钟 flake）。
    #[tokio::test]
    async fn connect_once_marks_online_feeds_bytes_and_clears_on_eof() {
        use tokio::io::AsyncWriteExt as _;

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            sock.write_all(b"(201/9/a)").await.unwrap();
            // 让客户端先读到字节，再断开
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(sock);
        });

        let state = Arc::new(Mutex::new(Client::new()));
        connect_once(&state, "127.0.0.1", i32::from(addr.port()), addr.port()).await;
        assert!(state.lock().unwrap().is_online(), "连上后必须在线");

        // 有界等待字节进入缓冲
        let mut waited = 0;
        let got = loop {
            if !state.lock().unwrap().buffered().is_empty() {
                break true;
            }
            if waited >= 100 {
                break false;
            }
            waited += 1;
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert!(got, "1s 内必须收到字节");
        assert_eq!(state.lock().unwrap().buffered(), b"(201/9/a)");

        // 对端关闭后有界等待离线
        let mut waited = 0;
        let offline = loop {
            if !state.lock().unwrap().is_online() {
                break true;
            }
            if waited >= 100 {
                break false;
            }
            waited += 1;
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert!(offline, "1s 内必须转为离线");
        server.await.unwrap();
    }

    /// 分服角色返回句柄，初始离线（连接由后台任务异步进行，10s 后才首次尝试）。
    #[tokio::test]
    async fn client_role_returns_offline_handle() {
        let link = start(1, "127.0.0.1", 4900, 0).expect("分服角色应有句柄");
        assert!(!link.is_online(), "刚创建时离线");
        // 时钟未到 10s 检查点 ⇒ 不应有任何连接尝试；tick 在离线时不解码
        link.tick();
    }
}
