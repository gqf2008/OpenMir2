// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

//! SelGate 的**进程级中继自测**：假 DBSrv + 假客户端，不依赖 MySQL 与整栈。
//!
//! 判据（真判据，不是自洽往返）：
//! 1. 客户端发 `#1…!` ⇒ 网关必须以**内部帧**（6B 头 + MemoryPack）透传给 DBSrv，
//!    且 `Data` 与客户端原始帧**逐字节相同**（网关不重新编码）；
//! 2. DBSrv 回一帧 ⇒ 网关必须原样写给对应会话（按 `SocketId` 分派）；
//! 3. 客户端心跳（1 字节 `*`）**不得**被转发（C# 侧收到 1 字节包直接忽略）；
//! 4. 缺配置时进程**非零退出**（M1 验收④）。

use std::time::{Duration, Instant};

use mir2_protocol::frame::ServerDataPacket;
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

/// 起一个"假 DBSrv"：接受一条连接，把收到的第一条 Data 帧原样回给同一 SocketId。
/// 返回 (端口, 收到的帧 Vec)。
async fn fake_login_srv() -> (
    u16,
    tokio::task::JoinHandle<Vec<ServerDataMessage>>,
    oneshot::Receiver<()>,
) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let (accepted_tx, accepted_rx) = oneshot::channel();
    let h = tokio::spawn(async move {
        let mut got = Vec::new();
        let (mut sock, _) = l.accept().await.unwrap();
        let _ = accepted_tx.send(());
        loop {
            let mut head = [0u8; ServerDataPacket::SIZE];
            if sock.read_exact(&mut head).await.is_err() {
                break;
            }
            let pkt = ServerDataPacket::from_bytes(&head).unwrap();
            let mut body = vec![0u8; usize::from(pkt.packet_len)];
            sock.read_exact(&mut body).await.unwrap();
            let msg = ServerDataMessage::from_bytes(&body).unwrap();
            if msg.kind == ServerDataType::Data {
                // 原样回帧（模拟 DBSrv 的应答）
                let reply = ServerDataMessage {
                    kind: ServerDataType::Data,
                    socket_id: msg.socket_id.clone(),
                    data_len: msg.data_len,
                    data: msg.data.clone(),
                };
                sock.write_all(&reply.to_wire()).await.unwrap();
                sock.flush().await.unwrap();
            }
            got.push(msg);
        }
        got
    });
    (port, h, accepted_rx)
}

/// 起一个"接受后关闭"的假 DBSrv：读到网关的 Enter 帧后立即断开（模拟 DBSrv 掉线）。
async fn fake_srv_accept_enter_then_close() -> (u16, oneshot::Receiver<()>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let (accepted_tx, accepted_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (mut sock, _) = l.accept().await.unwrap();
        let _ = accepted_tx.send(());
        loop {
            let mut head = [0u8; ServerDataPacket::SIZE];
            if sock.read_exact(&mut head).await.is_err() {
                return;
            }
            let pkt = ServerDataPacket::from_bytes(&head).unwrap();
            let mut body = vec![0u8; usize::from(pkt.packet_len)];
            if sock.read_exact(&mut body).await.is_err() {
                return;
            }
            let msg = ServerDataMessage::from_bytes(&body).unwrap();
            if msg.kind == ServerDataType::Enter {
                break;
            }
        }
        // FIN：网关读到 EOF ⇒ 下行就绪标志置 false。
        drop(sock);
    });
    (port, accepted_rx)
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// 写一份指向 (srv=假 DBSrv, gate=指定端口) 的 config.conf，返回目录。
fn write_conf(gate_port: u16, srv_port: u16) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lg-relay-{}-{}", std::process::id(), gate_port));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("config.conf"),
        format!(
            "[SelGate]\nServerAddr0=127.0.0.1\nServerPort0={srv_port}\nGateAddr0=127.0.0.1\nGatePort0={gate_port}\n"
        ),
    )
    .unwrap();
    dir
}

#[tokio::test]
async fn relays_frames_and_swallows_heartbeat() {
    let (srv_port, srv_task, _srv_accepted) = fake_login_srv().await;
    let gate_port = free_port();
    let dir = write_conf(gate_port, srv_port);
    let conf = dir.join("config.conf");
    let log = dir.join("gate.log");

    // 起网关并等下行链路安装完成（否则 Enter 可能被“未就绪”丢弃）。
    let mut child = spawn_gate_ready(&conf, &log, srv_port).await;

    // 等端口就绪（不 sleep 猜，直接轮询连得上）
    let mut client = None;
    for _ in 0..100 {
        match TcpStream::connect(("127.0.0.1", gate_port)).await {
            Ok(c) => {
                client = Some(c);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
    let mut client = client.expect("网关未在预期时间内监听");

    // 客户端帧（内容随意：网关应原样透传，不解析）
    let frame = b"#1ABCdef!".to_vec();
    client.write_all(b"*").await.unwrap(); // 心跳：不得被转发
    client.write_all(&frame).await.unwrap();
    client.flush().await.unwrap();

    // 网关回写的应是 DBSrv 原样回帧
    let mut back = vec![0u8; frame.len()];
    tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut back))
        .await
        .expect("5s 内未收到中继回帧")
        .unwrap();
    assert_eq!(
        back, frame,
        "客户端载荷必须在网关内逐字节不变（网关不重新编码）"
    );

    // 拆下行连接，让假 DBSrv 收起收到的帧
    child.kill().await.ok();
    drop(client);
    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 DBSrv 未按时结束")
        .unwrap();
    // Enter 帧体是 C# DBSrv 的机器判据：`%K{SocketId}/{ClientIP}/{ClientIP}$`。
    let enter = got
        .iter()
        .find(|m| m.kind == ServerDataType::Enter)
        .expect("网关必须向 DBSrv 发送 Enter 帧");
    let enter_body = String::from_utf8(enter.data.clone()).unwrap();
    assert_eq!(
        enter_body,
        format!("%K{}/127.0.0.1/127.0.0.1$", enter.socket_id),
        "SelGate Enter 体必须与 C# UserEnter 逐字节一致（含 %K 与尾 $）"
    );
    assert_eq!(
        usize::try_from(enter.data_len).unwrap(),
        enter.data.len(),
        "Enter 的 DataLen 必须等于体长"
    );

    let data_frames: Vec<_> = got
        .iter()
        .filter(|m| m.kind == ServerDataType::Data)
        .collect();
    assert_eq!(
        data_frames.len(),
        1,
        "心跳不得被转发：期望恰好 1 条 Data 帧，实得 {data_frames:?}"
    );
    assert_eq!(
        data_frames[0].data, frame,
        "透传给 DBSrv 的 Data 必须与原始帧相同"
    );
    assert!(
        !data_frames[0].socket_id.is_empty(),
        "Data 帧必须带 SocketId（多路复用靠它）"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn missing_config_exits_nonzero() {
    let dir = std::env::temp_dir().join(format!("lg-noconf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conf = dir.join("config.conf");
    std::fs::write(&conf, "[SelGate]\nServerAddr0=127.0.0.1\n").unwrap(); // 缺端口等键
    let out = tokio::process::Command::new(env!("CARGO_BIN_EXE_sel-gate"))
        .arg(&conf)
        .output()
        .await
        .unwrap();
    assert!(!out.status.success(), "缺配置必须非零退出（M1④）");
    let err =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("配置"), "错误信息应指明配置问题，实得：{err}");
    std::fs::remove_dir_all(&dir).ok();
}

/// 写一份带 `[Integer] ClientTimeOutTime` 的 config.conf（F1 的进程级判据）。
fn write_conf_with_timeout(gate_port: u16, srv_port: u16, timeout_ms: u64) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("sg-timeout-{}-{}", std::process::id(), gate_port));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("config.conf"),
        format!(
            "[Integer]\nClientTimeOutTime={timeout_ms}\n\n\
             [SelGate]\nServerAddr0=127.0.0.1\nServerPort0={srv_port}\n\
             GateAddr0=127.0.0.1\nGatePort0={gate_port}\n"
        ),
    )
    .unwrap();
    dir
}

/// 轮询到网关开始监听（不 sleep 猜）。
async fn connect_retry(gate_port: u16) -> TcpStream {
    for _ in 0..100 {
        match TcpStream::connect(("127.0.0.1", gate_port)).await {
            Ok(c) => return c,
            Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
    panic!("网关未在预期时间内监听");
}

/// F1（进程级，oracle 行为）：SelGate 第一阶段超时后
/// - 客户端收到 18B `SM_OUTOFCONNECTION`（`EDCode(12B 头) + 0x00 + '!'`，无 `'#'` 前导）；
/// - 连接**保持打开**（read 不返回 0，客户端仍可写）；
/// - 服务端继续读并**静默丢弃**超时后的客户端数据（`GetSession == null`，不得再转发给 DBSrv）；
/// - 客户端最终断开时**不发** `Leave`。
///
/// 牙：恢复 `break`（发帧后 writer drop ⇒ 立即 FIN）⇒ 超时后的 `read` 返回 0 ⇒ 本用例红。
#[tokio::test]
async fn idle_timeout_sends_frame_but_keeps_socket_open() {
    let (srv_port, srv_task, _srv_accepted) = fake_login_srv().await;
    let gate_port = free_port();
    let dir = write_conf_with_timeout(gate_port, srv_port, 10_000);
    let conf = dir.join("config.conf");
    let log = dir.join("gate.log");

    // 先等下行链路安装完成：若客户端先接入，`RestSessionArray()` 会摘掉它的
    // `SocketId`（oracle 行为），本用例就永远等不到超时帧。
    let mut child = spawn_gate_ready(&conf, &log, srv_port).await;
    let t0 = tokio::time::Instant::now();
    let mut client = connect_retry(gate_port).await;

    // SelGate 的扫描节奏是 20s（`TimedService.cs:40`），阈值 10s ⇒ 首次动作在 t≈20s。
    let mut frame = vec![0u8; 18];
    tokio::time::timeout_at(t0 + Duration::from_secs(25), client.read_exact(&mut frame))
        .await
        .expect("25s 内必须超时并收到 18B SM_OUTOFCONNECTION")
        .unwrap();
    assert_eq!(
        frame,
        vec![
            0x64, 0x64, 0x68, 0x6b, 0x64, 0x68, 0x6a, 0x6b, 0x64, 0x64, 0x68, 0x6b, 0x64, 0x64,
            0x68, 0x6b, 0x00, 0x21,
        ],
        "超时帧必须逐字节等于 C# SelGate 的线上形态"
    );

    // oracle：SelGate 的 `SessionManager.CloseSession` 只 `TryRemove`，不关 socket ⇒ 连接保持打开。
    let mut one = [0u8; 1];
    let alive = tokio::time::timeout(Duration::from_millis(800), client.read(&mut one)).await;
    assert!(
        alive.is_err(),
        "超时后连接不得 FIN（read 返回 0）；实得 {alive:?}"
    );

    // 客户端仍可写；服务端继续读并丢弃（不得再转发给 DBSrv）。
    client
        .write_all(b"#1ABCdef!")
        .await
        .expect("超时后客户端仍应可写");
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 复验 Minor 3：先关客户端、给网关处理 `Disconnected`/会话离开的时间，**再**杀进程。
    // 旧顺序（先 kill 再 drop）让「不得发 Leave」的断言在进程已死时恒成立，抓不到回归。
    drop(client);
    tokio::time::sleep(Duration::from_millis(500)).await;
    child.kill().await.ok();
    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 DBSrv 未按时结束")
        .unwrap();
    assert!(
        !got.iter().any(|m| m.kind == ServerDataType::Data),
        "超时后客户端数据必须被静默丢弃（GetSession == null）：{got:?}"
    );
    assert!(
        !got.iter().any(|m| m.kind == ServerDataType::Leave),
        "超时路径不得给 DBSrv 发 Leave：{got:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 第二轮复验 Important（进程级）：DBSrv 接受后关闭 ⇒ 客户端**收不到**超时帧。
///
/// oracle：`ClientThread.ClientSocketDisconnect` 的 `RestSessionArray` 清空槽位
/// `SocketId` ⇒ `TimedService.ProcessDelayMsg` 的 `GetSession` 返回 null ⇒ 超时动作
/// 整体不发生；`ClientSession.SendDefMessage` 另有 `IsConnected` 早退。
///
/// 牙：删掉超时支的 `downstream_alive` 守卫 ⇒ 本用例在 t≈20s 收到 18B 帧 ⇒ 红。
#[tokio::test]
async fn downstream_disconnect_suppresses_idle_timeout_frame() {
    let (srv_port, _srv_accepted) = fake_srv_accept_enter_then_close().await;
    let gate_port = free_port();
    let dir = write_conf_with_timeout(gate_port, srv_port, 10_000);
    let conf = dir.join("config.conf");
    let log = dir.join("gate.log");

    // 先等下行链路**安装完成**再接入客户端（否则 Enter 未发出，假 DBSrv 不会断开）。
    let mut child = spawn_gate_ready(&conf, &log, srv_port).await;
    let t0 = tokio::time::Instant::now();
    let mut client = connect_retry(gate_port).await;

    // SelGate 扫描节奏 20s、阈值 10s：若守卫失效，t≈20s 必发 18B 超时帧。
    // 等到 25s 仍无任何字节（既无帧也无 FIN）才算通过。
    let mut buf = [0u8; 32];
    let got = tokio::time::timeout_at(t0 + Duration::from_secs(25), client.read(&mut buf)).await;
    assert!(
        got.is_err(),
        "DBSrv 断开后不得发生超时动作（不得发帧/不得 FIN）；实得 {got:?}"
    );

    child.kill().await.ok();
    std::fs::remove_dir_all(&dir).ok();
}

/// F2（进程级，oracle `RestSessionArray`）：下行断链会让**已有会话**退出空闲超时
/// 集合，且链路恢复也**不回填** ⇒ 之后哪怕计时早已逾期，20s 扫描也不得产生任何动作。
///
/// 时间线：链路就绪 → 客户端接入（建会话）→ 假 DBSrv 掉线 → 网关重连 →
/// 再等 ≥1 个完整 20s 扫描 ⇒ 客户端收不到任何字节（既无 18B 超时帧也无 FIN）。
///
/// 牙：把 `if !idle_eligible { continue; }` 改成 `if false` ⇒ 重连后的扫描会用
/// 过期计时触发 `SendOutOfConnection` ⇒ 客户端收到 18B 帧 ⇒ 本用例红。
#[tokio::test]
async fn downstream_reconnect_keeps_session_out_of_idle_timeout() {
    let (accepted1_tx, _accepted1_rx) = oneshot::channel();
    let (enter_sid_tx, enter_sid_rx) = oneshot::channel();
    let (accepted2_tx, accepted2_rx) = oneshot::channel();
    let (srv_port, srv_task) =
        fake_srv_drop_then_echo(accepted1_tx, enter_sid_tx, accepted2_tx).await;
    let gate_port = free_port();
    let dir = write_conf_with_timeout(gate_port, srv_port, 10_000);
    let conf = dir.join("config.conf");
    let log = dir.join("gate.log");

    // 先等下行链路安装完成，客户端才能建会话（否则命中「获取服务器实例失败。」分支）。
    let mut child = spawn_gate_ready(&conf, &log, srv_port).await;
    let mut client = connect_retry(gate_port).await;

    // 假 DBSrv 读到 Enter 后立即断开 ⇒ oracle `RestSessionArray` 把该会话踢出超时集合。
    tokio::time::timeout(Duration::from_secs(5), enter_sid_rx)
        .await
        .expect("5s 内假 DBSrv 未收到 Enter")
        .unwrap();
    wait_for_log(
        &log,
        &format!("数据库服务器[127.0.0.1:{srv_port}]断开链接."),
        Duration::from_secs(5),
    )
    .await;

    // 等网关重连（≤2 个 10s 周期，与 R1 用例同口径）。
    tokio::time::timeout(Duration::from_secs(27), accepted2_rx)
        .await
        .expect("两个 10s 周期内未重连（重试被关掉或周期改错？）")
        .unwrap();
    wait_for_log_count(
        &log,
        &format!("数据库服务器[127.0.0.1:{srv_port}]链接成功."),
        2,
        Duration::from_secs(5),
    )
    .await;

    // 重连后再等一个完整扫描周期：链路已就绪、计时也早已超过 10s 阈值，
    // 唯一能阻止超时动作的就是 `idle_eligible` 守卫（oracle 不回填 `SocketId`）。
    tokio::time::sleep(Duration::from_secs(21)).await;

    let mut buf = [0u8; 32];
    let got = tokio::time::timeout(Duration::from_millis(800), client.read(&mut buf)).await;
    assert!(
        got.is_err(),
        "重连后该会话仍不得参与空闲超时（不得发 18B 帧/不得 FIN）；实得 {got:?}"
    );

    child.kill().await.ok();
    drop(client);
    srv_task.abort();
    std::fs::remove_dir_all(&dir).ok();
}

/// 起网关并把 stdout 落到日志文件（日志判据要读原文串）。
fn spawn_gate_logged(conf: &std::path::Path, log: &std::path::Path) -> tokio::process::Child {
    let out = std::fs::File::create(log).unwrap();
    tokio::process::Command::new(env!("CARGO_BIN_EXE_sel-gate"))
        .arg(conf)
        .stdout(out)
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

/// 起网关并等到下行链路**已安装**（`ClientSocketConnect` 回调跑完）。
///
/// oracle `AppService.Start()` 先起监听、再异步连 DBSrv：客户端可能先于下行接入，
/// 随后 `RestSessionArray()` 会把该会话的 `SocketId` 清空（空闲超时从此不适用）。
/// 需要“会话正常接入”语义的用例必须先等连接回调完成，否则会撞上这条 oracle 行为。
async fn spawn_gate_ready(
    conf: &std::path::Path,
    log: &std::path::Path,
    srv_port: u16,
) -> tokio::process::Child {
    let child = spawn_gate_logged(conf, log);
    wait_for_log(
        log,
        &format!("数据库服务器[127.0.0.1:{srv_port}]链接成功."),
        Duration::from_secs(5),
    )
    .await;
    child
}

/// 轮询日志文件直到出现 `needle`（不 sleep 猜固定时长）。
async fn wait_for_log(path: &std::path::Path, needle: &str, within: Duration) -> String {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        if text.contains(needle) {
            return text;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{within:?} 内日志未出现 {needle:?}；实得:\n{text}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// 轮询日志文件直到 `needle` 出现至少 `count` 次。
async fn wait_for_log_count(
    path: &std::path::Path,
    needle: &str,
    count: usize,
    within: Duration,
) -> String {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        if text.matches(needle).count() >= count {
            return text;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{within:?} 内日志 {needle:?} 未出现 {count} 次；实得:\n{text}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// 读一条内部帧（6B 头 + MemoryPack 体）。
async fn read_internal_frame(sock: &mut TcpStream) -> std::io::Result<ServerDataMessage> {
    let mut head = [0u8; ServerDataPacket::SIZE];
    sock.read_exact(&mut head).await?;
    let packet = ServerDataPacket::from_bytes(&head)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad header"))?;
    let mut body = vec![0u8; usize::from(packet.packet_len)];
    sock.read_exact(&mut body).await?;
    ServerDataMessage::from_bytes(&body)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad body"))
}

/// 假 DBSrv：第一条连接读到 Enter 后立刻 FIN（模拟 DBSrv 掉线）；第二条连接把
/// `Data` 原样回显。`accepted1` / `enter_sid` / `accepted2` 分别报告首连、Enter 会话号、
/// 重连成功，便于测试消除时序竞态。
async fn fake_srv_drop_then_echo(
    accepted1: oneshot::Sender<()>,
    enter_sid: oneshot::Sender<String>,
    accepted2: oneshot::Sender<()>,
) -> (u16, tokio::task::JoinHandle<Vec<ServerDataMessage>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.unwrap();
        let _ = accepted1.send(());
        loop {
            let Ok(msg) = read_internal_frame(&mut first).await else {
                break;
            };
            if msg.kind == ServerDataType::Enter {
                let _ = enter_sid.send(msg.socket_id.clone());
                break;
            }
        }
        drop(first);
        let (mut second, _) = listener.accept().await.unwrap();
        let _ = accepted2.send(());
        let mut got = Vec::new();
        loop {
            let Ok(msg) = read_internal_frame(&mut second).await else {
                break;
            };
            if msg.kind == ServerDataType::Data {
                let reply = ServerDataMessage {
                    kind: ServerDataType::Data,
                    socket_id: msg.socket_id.clone(),
                    data_len: msg.data_len,
                    data: msg.data.clone(),
                };
                if second.write_all(&reply.to_wire()).await.is_err() {
                    break;
                }
                let _ = second.flush().await;
            }
            got.push(msg);
        }
        got
    });
    (port, task)
}

/// 判据 R1（进程级）：DBSrv 掉线后，网关在 ≤2 个 10s 周期内自动重连；
/// 断链期间客户端连接不崩、帧被丢弃；重连后同一会话的新请求被转发且回包回到原客户端。
///
/// oracle：`TimedService.CheckSessionStatus` 每 10s 判 `CheckServerFail` 并 `Start()`；
/// `ClientSocketDisconnect` 的 `RestSessionArray` 只清 `SocketId`、**不**清会话。
///
/// 牙：
/// - 把重试改成只一次（或周期改错）⇒ `accepted2` 超时 ⇒ 红；
/// - 断链时清空 `sessions`（或写失败即结束客户端）⇒ 回包不到 / 客户端被 FIN ⇒ 红。
#[allow(clippy::too_many_lines)]
// 一条完整时间线：首连 → 断链 → 新客户端无会话 → 重连 → 同会话转发
#[tokio::test]
async fn reconnects_after_downstream_drop_and_keeps_sessions() {
    let (accepted1_tx, _accepted1_rx) = oneshot::channel();
    let (enter_sid_tx, enter_sid_rx) = oneshot::channel();
    let (accepted2_tx, accepted2_rx) = oneshot::channel();
    let (srv_port, srv_task) =
        fake_srv_drop_then_echo(accepted1_tx, enter_sid_tx, accepted2_tx).await;
    let gate_port = free_port();
    let dir = write_conf(gate_port, srv_port);
    let conf = dir.join("config.conf");
    let log = dir.join("gate.log");
    // 等下行链路安装完成再接入客户端（否则 Enter 会被“未就绪”丢弃，假 DBSrv 收不到）。
    let mut child = spawn_gate_ready(&conf, &log, srv_port).await;
    let mut client = connect_retry(gate_port).await;
    let enter_sid = tokio::time::timeout(Duration::from_secs(5), enter_sid_rx)
        .await
        .expect("5s 内假 DBSrv 未收到 Enter")
        .unwrap();

    // 断链期间：客户端仍可写、不得被 FIN；该帧在网关侧丢弃（oracle `未就绪`）。
    client
        .write_all(b"#1OUTAGE!")
        .await
        .expect("断链期间客户端应仍可写");
    client.flush().await.unwrap();
    let mut one = [0u8; 1];
    let alive = tokio::time::timeout(Duration::from_millis(800), client.read(&mut one)).await;
    assert!(
        alive.is_err(),
        "下行断链不得结束客户端连接（不得 FIN/发帧）；实得 {alive:?}"
    );

    // 等网关确认断开（`ClientSocketDisconnect` 已跑完：ServerGateList 已移除）。
    wait_for_log(
        &log,
        &format!("数据库服务器[127.0.0.1:{srv_port}]断开链接."),
        Duration::from_secs(5),
    )
    .await;

    // 断链期间接入的新客户端：oracle `GetClientThread()` 返回 null ⇒ 不建会话、不发 Enter；
    // 链路恢复后也不会补会话（没有回填逻辑）。
    let mut zombie = connect_retry(gate_port).await;
    zombie
        .write_all(b"#1ZOMBIE!")
        .await
        .expect("断链期间新客户端应仍可写");
    zombie.flush().await.unwrap();
    let text = wait_for_log(&log, "获取服务器实例失败。", Duration::from_secs(5)).await;
    assert!(
        text.contains("获取服务器实例失败。"),
        "断链期间接入应走 oracle 的“无服务器实例”分支：
{text}"
    );

    // ≤2 个 10s 周期内重连（oracle 以连接时刻为 CheckServerTick 起点）。
    tokio::time::timeout(Duration::from_secs(27), accepted2_rx)
        .await
        .expect("两个 10s 周期内未重连（重试被关掉或周期改错？）")
        .unwrap();

    let connected = format!("数据库服务器[127.0.0.1:{srv_port}]链接成功.");
    wait_for_log_count(&log, &connected, 2, Duration::from_secs(5)).await;

    // 重连后同一会话的新请求必须被转发，且下行回复按 socket id 回到原客户端。
    let frame = b"#1AFTER!".to_vec();
    client.write_all(&frame).await.unwrap();
    client.flush().await.unwrap();
    let mut back = vec![0u8; frame.len()];
    tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut back))
        .await
        .expect("重连后 5s 内未收到回帧（会话表被清空？）")
        .unwrap();
    assert_eq!(back, frame);

    // 断链期间接入的客户端在重连后仍无会话（oracle 不补 Enter）⇒ 帧不得被服务。
    zombie
        .write_all(b"#1ZOMBIE2!")
        .await
        .expect("zombie 客户端应仍连着");
    zombie.flush().await.unwrap();
    let mut one = [0u8; 1];
    let served = tokio::time::timeout(Duration::from_millis(700), zombie.read(&mut one)).await;
    assert!(
        served.is_err(),
        "断链期间接入的客户端在 oracle 中没有会话，重连后也不得收到回包/FIN；实得 {served:?}"
    );

    let text = std::fs::read_to_string(&log).unwrap_or_default();
    // F1（日志牙）：链路恢复后，无会话客户端的帧必须静默丢弃（oracle `GetSession == null`），
    // 不得再逐帧打「非法攻击」。本用例在断链期与恢复后各写一帧 ⇒ 只允许断链期那一次。
    assert_eq!(
        text.matches("非法攻击").count(),
        1,
        "断链期间接入的客户端只应在链路仍断时打一次「非法攻击」；恢复后必须静默：
{text}"
    );
    child.kill().await.ok();
    drop(client);
    drop(zombie);
    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 DBSrv 未按时结束")
        .unwrap();
    let data: Vec<_> = got
        .iter()
        .filter(|m| m.kind == ServerDataType::Data)
        .collect();
    assert_eq!(
        data.len(),
        1,
        "断链期间的 OUTAGE 帧必须被丢弃，只有重连后的帧到达：{got:?}"
    );
    assert_eq!(data[0].data, frame);
    assert_eq!(
        data[0].socket_id, enter_sid,
        "重连后仍是同一会话（oracle 保留 SessionArray）"
    );
    assert!(
        text.contains(&format!("服务器[127.0.0.1:{srv_port}]建立链接.失败次数:[")),
        "缺 oracle 原文重连日志：\n{text}"
    );
    assert_eq!(
        text.matches(&connected).count(),
        2,
        "应有两次链接成功（首连 + 重连）：\n{text}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 判据 R2（进程级）：启动时 DBSrv 不在 ⇒ **不退出**（oracle `TimedService` 每 10s 重试），
/// 首个 10s tick 打 `链接超时.失败次数:[1]`，下一次重连尝试打 `建立链接.失败次数:[2]`；
/// DBSrv 后来起来后能连上。
///
/// 裁决：B-1 规格要求按 oracle。原 Rust 的 M1④ “连不上 DBSrv 即退出”只适用于
/// **配置/绑定**错误；下行链路按 oracle 无限重试（见报告“裁决”）。
///
/// 牙：恢复启动 fail-loud（连接失败即 `?` 退出）⇒ 本用例红。
#[tokio::test]
async fn startup_without_dbsrv_retries_and_never_exits() {
    let srv_port = free_port();
    let gate_port = loop {
        let candidate = free_port();
        if candidate != srv_port {
            break candidate;
        }
    };
    let dir = write_conf(gate_port, srv_port);
    let conf = dir.join("config.conf");
    let log = dir.join("gate.log");
    let mut child = spawn_gate_logged(&conf, &log);

    // 首次连接失败只记错误、不退出（oracle `ClientThread.Start` 的 catch）。
    let text = wait_for_log(
        &log,
        &format!("链接数据库服务器[127.0.0.1:{srv_port}]失败..."),
        Duration::from_secs(10),
    )
    .await;
    assert!(
        child.try_wait().unwrap().is_none(),
        "DBSrv 不在时网关不得退出：\n{text}"
    );

    // 第一个 10s tick：启动形态 `链接超时.失败次数:[1]`。
    let text = wait_for_log(
        &log,
        &format!("服务器[127.0.0.1:{srv_port}]链接超时.失败次数:[1]"),
        Duration::from_secs(15),
    )
    .await;
    assert!(
        child.try_wait().unwrap().is_none(),
        "首发超时日志后仍不得退出：\n{text}"
    );

    // 起假 DBSrv：下一次重连（`建立链接.失败次数:[2]`）应成功。
    let listener = TcpListener::bind(("127.0.0.1", srv_port)).await.unwrap();
    let srv = tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        while read_internal_frame(&mut sock).await.is_ok() {}
    });
    let text = wait_for_log(
        &log,
        &format!("数据库服务器[127.0.0.1:{srv_port}]链接成功."),
        Duration::from_secs(25),
    )
    .await;
    assert!(
        text.contains(&format!(
            "服务器[127.0.0.1:{srv_port}]建立链接.失败次数:[2]"
        )),
        "第二次尝试应由“建立链接”分支发出：\n{text}"
    );
    child.kill().await.ok();
    srv.abort();
    std::fs::remove_dir_all(&dir).ok();
}

/// 判据 R3（进程级）：连接稳定 ≥2 个周期时，DBSrv 每 10±1s 收到一帧 `KeepAlive`
///（oracle `TimedService.CheckSessionStatus` → `ClientThread.SendKeepAlive`）。
/// wire 只断言 `kind`（A-10 允许空字段 `00000000` vs `ffffffff`）。
///
/// 牙：把心跳周期改错（如 20s）⇒ 22s 内只收到 1 帧 ⇒ 红。
#[tokio::test]
async fn sends_keepalive_every_ten_seconds() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let srv_port = listener.local_addr().unwrap().port();
    let (accepted_tx, accepted_rx) = oneshot::channel();
    let srv = tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let _ = accepted_tx.send(());
        let mut times = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(22);
        loop {
            match tokio::time::timeout_at(deadline, read_internal_frame(&mut sock)).await {
                Err(_) | Ok(Err(_)) => break,
                Ok(Ok(msg)) => {
                    if msg.kind == ServerDataType::KeepAlive {
                        times.push(Instant::now());
                    }
                }
            }
        }
        times
    });
    let gate_port = free_port();
    let dir = write_conf(gate_port, srv_port);
    let conf = dir.join("config.conf");
    let log = dir.join("gate.log");
    let mut child = spawn_gate_logged(&conf, &log);
    accepted_rx.await.expect("下行连接未建立");

    let times = tokio::time::timeout(Duration::from_secs(25), srv)
        .await
        .expect("假 DBSrv 未按时结束")
        .unwrap();
    child.kill().await.ok();
    assert!(
        times.len() >= 2,
        "22s 内应至少收到 2 帧 KeepAlive（周期 10s），实得 {} 帧",
        times.len()
    );
    for pair in times.windows(2) {
        let gap = pair[1].duration_since(pair[0]);
        assert!(
            gap >= Duration::from_secs(9) && gap <= Duration::from_secs(12),
            "KeepAlive 间隔必须为 10±1s，实得 {gap:?}"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}
