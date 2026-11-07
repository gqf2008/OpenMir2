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
//! | 连上打「连接主服务器(tcp://ip:port/)成功...」 | [`connected_line`] |
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

/// 「连接主服务器(tcp://ip:port/)成功...」（`PlanesClient.cs:150`）。
///
/// oracle 拼的是 `client.RemoteIPHost`，实测 `IPHost.ToString()` 带 scheme 与尾斜杠。
#[must_use]
pub fn connected_line(addr: &str, port: i32) -> String {
    format!("连接主服务器(tcp://{addr}:{port}/)成功...")
}

/// 「节点服务器(ip:port)断开连接...」（`PlanesClient.cs:157`；`GetIPPort()` 是 `addr:port`）。
#[must_use]
pub fn disconnected_line(peer: &str) -> String {
    format!("节点服务器({peer})断开连接...")
}

/// 跨服链路句柄。主服角色下 [`start`] 返回 `None`（oracle 不监听）。
#[derive(Debug, Clone)]
pub struct PlanesLink {
    state: Arc<Mutex<Client>>,
    /// 连接建立后由写入任务消费；未连接时为 `None`（对应 oracle `SendSocket` 的 `IsConnect()` 门）。
    outbox: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>>,
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
                // oracle `PlanesClient.cs:132-136`：catch 里**两条** `LogService.Error`——
                // 第一条是常量 `[Exception] TFrmSrvMsg::DecodeSocStr`，第二条是 `ex.StackTrace`。
                // Rust 侧没有 C# 堆栈，第二条改记异常语义（形状仍是两条）。
                crate::glog!("ERR", "[Exception] TFrmSrvMsg::DecodeSocStr");
                crate::glog!(
                    "ERR",
                    "ProcessData 抛异常：ISM_CHANGECASTLEOWNER(211) ⇒ 本批剩余帧与缓冲已丢弃"
                );
            }
        }
    }

    /// oracle `WorldServer.SendServerGroupMsg(code, serverIdx, body)`
    /// （`src/GameSrv/Word/WorldServer.cs:885-896`）。
    ///
    /// 线格式是**明文三段** `(code/serverIdx/body)`（`SendSocket` 负责补括号），不是 SM 二进制帧。
    /// 返回值 = **实际投递的连接数**：主服不监听（§5.2）⇒ 恒 0；分服未连接 ⇒ 0（oracle
    /// `SendSocket` 里 `if (!IsConnect()) return;` 的等价物）。该返回值就是 5b 卡判据③
    /// 要观察的「server-group 发送次数」接缝。
    pub fn send_server_group_msg(&self, ident: i32, server_num: i32, body: &[u8]) -> usize {
        let bytes = mir2_planes::encode_frame(ident, server_num, body);
        let Ok(outbox) = self.outbox.lock() else {
            return 0;
        };
        let Some(tx) = outbox.as_ref() else {
            return 0;
        };
        usize::from(tx.send(bytes).is_ok())
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
/// # Errors
/// 地址不是合法 IP、或端口超出 `0..=65535` 时返回错误（**启动即失败**）：oracle 在
/// `IPHost(IPAddress.Parse(addr), port)` 处抛异常，而 `GameServer.cs:34` 没有 try/catch。
pub fn start(
    server_index: u8,
    master_addr: &str,
    master_port: i32,
    server_number: i32,
) -> anyhow::Result<Option<PlanesLink>> {
    // oracle 在 `PlanesServer.StartPlanesServer` 与 `PlanesClient.Initialize` 里都会
    // `new IPHost(IPAddress.Parse(addr), port)`：地址非法抛 `FormatException`、端口越界抛
    // `UriFormatException`，而 `GameServer.cs:34` 调用 `PlanesService.Start()` 时**没有 try/catch**
    // ⇒ **启动即失败**。这里同样 fail-fast，不静默降级。
    anyhow::ensure!(
        master_addr.parse::<std::net::IpAddr>().is_ok(),
        "MsgSrvAddr 不是合法 IP 地址：{master_addr:?}（oracle `IPAddress.Parse` 会抛异常）"
    );
    let port_u16 = u16::try_from(master_port).map_err(|_| {
        anyhow::anyhow!("MsgSrvPort 不是合法端口：{master_port}（oracle `IPHost` 构造会抛异常）")
    })?;
    match role_of(server_index) {
        Role::Server => Ok(None),
        Role::Client => Ok(Some(spawn_client(
            master_addr.to_owned(),
            master_port,
            server_number,
            port_u16,
        ))),
    }
}

fn spawn_client(addr: String, port: i32, server_number: i32, port_u16: u16) -> PlanesLink {
    let link = PlanesLink {
        state: Arc::new(Mutex::new(Client::new())),
        outbox: Arc::new(Mutex::new(None)),
    };
    let state = Arc::clone(&link.state);
    let outbox = Arc::clone(&link.outbox);
    crate::glog!("INF", "{}", node_mode_line(&addr, port));
    tokio::spawn(async move {
        // **启动即无条件连一次**：oracle `PlanesService.cs:16-17` 的节点分支直接
        // `Initialize(); Start();`，`PlanesClient.Start()`（`:53-71`）在离线时立刻
        // `await ConnectAsync()`——与 `nServerNumber` **无关**，且失败会打「失败./超时.」。
        // 10s 节拍只约束**之后**的 `CheckConnected()` 周期重连。
        //
        // 锚点取**启动时刻**（与首连并发）：`src/GameSvr/TimedService.cs` 的 `StartAsync`
        // 在服务启动时就把 `CheckIntervalTime` 置成当时的 tick。若放在首连之后再取，
        // 黑洞端点（首连走满 5s 超时）会让首次周期检查整体后移 5s。
        let mut last_check = now_ms();
        connect_once(&state, &outbox, &addr, port, port_u16).await;
        let mut timer = tokio::time::interval(TIMER_PERIOD);
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
                connect_once(&state, &outbox, &addr, port, port_u16).await;
            }
        }
    });
    link
}

async fn connect_once(
    state: &Arc<Mutex<Client>>,
    outbox: &Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>>,
    addr: &str,
    port: i32,
    port_u16: u16,
) {
    let target = format!("{addr}:{port}");
    let attempt = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio::net::TcpStream::connect((addr, port_u16)),
    )
    .await;
    let stream = match attempt {
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
    crate::glog!("INF", "{}", connected_line(addr, port));
    let (mut stream, mut sink) = stream.into_split();
    // 写任务：`send_server_group_msg` 只把字节丢进通道，真正的写在这里。
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    if let Ok(mut slot) = outbox.lock() {
        *slot = Some(tx);
    }
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt as _;
        while let Some(buf) = rx.recv().await {
            if sink.write_all(&buf).await.is_err() {
                break;
            }
        }
    });
    let state = Arc::clone(state);
    let outbox = Arc::clone(outbox);
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
        if let Ok(mut slot) = outbox.lock() {
            *slot = None;
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
            connected_line("10.0.0.7", 4900),
            "连接主服务器(tcp://10.0.0.7:4900/)成功..."
        );
        assert_eq!(
            disconnected_line("10.0.0.7:4900"),
            "节点服务器(10.0.0.7:4900)断开连接..."
        );
    }

    /// **首连是启动即无条件发生的**（`PlanesService.cs:16-17` → `PlanesClient.Start()`），
    /// 与 `nServerNumber` 无关。用真实监听器钉住：2s 内必须出现 accept。
    ///
    /// 这是**下界**断言（"必须发生过"），不卡上界，避免墙钟 flake。
    #[tokio::test]
    async fn client_connects_at_startup_even_when_server_number_is_zero() {
        for server_number in [0, 1] {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let _link = start(1, "127.0.0.1", i32::from(port), server_number)
                .expect("端点合法")
                .expect("分服角色应有句柄");
            // 宽松上界：这是「必须发生过」的下界断言，窗口给足以免慢机 flake。
            let accepted = tokio::time::timeout(Duration::from_secs(10), listener.accept()).await;
            assert!(
                accepted.is_ok(),
                "nServerNumber={server_number}：启动后必须尝试连接一次（10s 窗口）"
            );
        }
    }

    /// 发送口：连上后按 oracle 的明文三段 `(code/serverIdx/body)` 发出，并返回投递数 1。
    #[tokio::test]
    async fn send_server_group_msg_uses_oracle_wire_format() {
        use tokio::io::AsyncReadExt as _;

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let link = start(1, "127.0.0.1", i32::from(port), 0)
            .expect("端点合法")
            .expect("分服角色应有句柄");
        let (mut sock, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
            .await
            .expect("首连必须发生")
            .unwrap();

        // 有界等待 outbox 建立（连接建立后由 connect_once 登记）
        let mut waited = 0;
        while link.send_server_group_msg(205, 0, b"guild/chief") == 0 {
            assert!(waited < 200, "2s 内必须可发送");
            waited += 1;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let mut buf = [0_u8; 64];
        let n = tokio::time::timeout(Duration::from_secs(2), sock.read(&mut buf))
            .await
            .expect("2s 内必须收到字节")
            .unwrap();
        assert_eq!(&buf[..n], b"(205/0/guild/chief)", "明文三段，外层带括号");
    }

    /// 未连接时发送口返回 0（oracle `SendSocket` 的 `if (!IsConnect()) return;`）。
    #[tokio::test]
    async fn send_returns_zero_when_not_connected() {
        // 端口上没有人监听 ⇒ 首连与重连都失败，outbox 恒 None
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let link = start(1, "127.0.0.1", i32::from(port), 0)
            .expect("端点合法")
            .expect("分服角色应有句柄");
        assert_eq!(
            link.send_server_group_msg(205, 0, b"x"),
            0,
            "未连接必须丢弃并返回 0"
        );
    }

    /// 端点非法 ⇒ 启动即失败（oracle `IPHost` 构造抛异常、`GameServer.cs:34` 无 try/catch）。
    #[test]
    fn invalid_endpoint_fails_fast() {
        assert!(
            start(0, "not-an-ip", 4900, 0).is_err(),
            "MsgSrvAddr 非法：主服也必须失败（oracle 同样构造 IPHost）"
        );
        assert!(start(1, "127.0.0.1", 70000, 0).is_err(), "端口越界");
        assert!(start(1, "127.0.0.1", -1, 0).is_err(), "负端口");
        // 合法端点的成功路径要起 tokio 任务，放在 #[tokio::test] 里（见下一条用例），
        // 这里只钉失败路径。
    }

    /// 主服角色**不起监听、也不返回句柄**（设计文档 §5.2 的实测结论）。
    #[test]
    fn server_role_does_not_listen() {
        assert!(start(0, "127.0.0.1", 4900, 0).unwrap().is_none());
        assert!(start(0, "127.0.0.1", 4900, 3).unwrap().is_none());
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
        let outbox: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>> =
            Arc::new(Mutex::new(None));
        connect_once(
            &state,
            &outbox,
            "127.0.0.1",
            i32::from(addr.port()),
            addr.port(),
        )
        .await;
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

    /// 分服角色返回句柄，初始离线（后台任务**启动即无条件首连一次**，之后按 1s/10s 节拍重连）。
    #[tokio::test]
    async fn client_role_returns_offline_handle() {
        let link = start(1, "127.0.0.1", 4900, 0)
            .expect("端点合法")
            .expect("分服角色应有句柄");
        assert!(!link.is_online(), "刚创建时离线");
        // 句柄创建时尚未连上（首连在后台任务里异步进行）；tick 在离线时不解码
        link.tick();
    }
}
