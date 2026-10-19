// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

//! LoginGate 的**进程级中继自测**：假 LoginSrv + 假客户端，不依赖 MySQL 与整栈。
//!
//! 判据（真判据，不是自洽往返）：
//! 1. 客户端发 `#1…!` ⇒ 网关必须以**内部帧**（6B 头 + MemoryPack）透传给 LoginSrv，
//!    且 `Data` 与客户端原始帧**逐字节相同**（网关不重新编码）；
//! 2. LoginSrv 回一帧 ⇒ 网关必须原样写给对应会话（按 `SocketId` 分派）；
//! 3. 客户端心跳（1 字节 `*`）**不得**被转发（C# 侧收到 1 字节包直接忽略）；
//! 4. 缺配置时进程**非零退出**（M1 验收④）。

use std::time::Duration;

use mir2_protocol::frame::ServerDataPacket;
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// 起一个"假 LoginSrv"：接受一条连接，把收到的第一条 Data 帧原样回给同一 SocketId。
/// 返回 (端口, 收到的帧 Vec)。
async fn fake_login_srv() -> (u16, tokio::task::JoinHandle<Vec<ServerDataMessage>>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let h = tokio::spawn(async move {
        let mut got = Vec::new();
        let (mut sock, _) = l.accept().await.unwrap();
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
                // 原样回帧（模拟 LoginSrv 的应答）
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
    (port, h)
}

/// 起一个"接受后关闭"的假 LoginSrv：读到网关的 Enter 帧后立即断开（模拟 LoginSrv 掉线）。
async fn fake_srv_accept_enter_then_close() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut sock, _) = l.accept().await.unwrap();
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
        // FIN：网关 `downstream_loop` 读到 EOF ⇒ 下行存活标志置 false。
        drop(sock);
    });
    port
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// 写一份指向 (srv=假 LoginSrv, gate=指定端口) 的 config.conf，返回目录。
fn write_conf(gate_port: u16, srv_port: u16) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lg-relay-{}-{}", std::process::id(), gate_port));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("config.conf"),
        format!(
            "[LoginGate]\nServerAddr0=127.0.0.1\nServerPort0={srv_port}\nGateAddr0=127.0.0.1\nGatePort0={gate_port}\n"
        ),
    )
    .unwrap();
    dir
}

#[tokio::test]
async fn relays_frames_and_swallows_heartbeat() {
    let (srv_port, srv_task) = fake_login_srv().await;
    let gate_port = free_port();
    let dir = write_conf(gate_port, srv_port);
    let conf = dir.join("config.conf");

    // 起网关（本测试编译出的二进制）
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_login-gate"))
        .arg(&conf)
        .kill_on_drop(true)
        .spawn()
        .unwrap();

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

    // 网关回写的应是 LoginSrv 原样回帧
    let mut back = vec![0u8; frame.len()];
    tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut back))
        .await
        .expect("5s 内未收到中继回帧")
        .unwrap();
    assert_eq!(
        back, frame,
        "客户端载荷必须在网关内逐字节不变（网关不重新编码）"
    );

    // 拆下行连接，让假 LoginSrv 收起收到的帧
    child.kill().await.ok();
    drop(client);
    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 LoginSrv 未按时结束")
        .unwrap();
    // Enter 帧体是 C# LoginSrv 的机器判据：`{ClientIP}/{ClientIP}`（无 %K、无端口）。
    let enter = got
        .iter()
        .find(|m| m.kind == ServerDataType::Enter)
        .expect("网关必须向 LoginSrv 发送 Enter 帧");
    assert_eq!(
        enter.data,
        b"127.0.0.1/127.0.0.1".to_vec(),
        "LoginGate Enter 体必须与 C# UserEnter 逐字节一致"
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
        "透传给 LoginSrv 的 Data 必须与原始帧相同"
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
    std::fs::write(&conf, "[LoginGate]\nServerAddr0=127.0.0.1\n").unwrap(); // 缺端口等键
    let out = tokio::process::Command::new(env!("CARGO_BIN_EXE_login-gate"))
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

/// 写一份带 `[Integer] ClientTimeOutTime` 的 config.conf（审计 #12 的超时判据）。
fn write_conf_with_timeout(gate_port: u16, srv_port: u16, timeout_ms: u64) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lg-timeout-{}-{}", std::process::id(), gate_port));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("config.conf"),
        format!(
            "[Integer]\nClientTimeOutTime={timeout_ms}\n\n\
             [LoginGate]\nServerAddr0=127.0.0.1\nServerPort0={srv_port}\n\
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

/// 判据（审计 #12，进程级）：
/// - `ClientTimeOutTime=10000`（oracle 最小值）⇒ 无流量约 10s 后网关先发 C#
///   同字节的 `SM_OUTOFCONNECTION`（16 字节，含其截断缺陷），随后关闭连接；
/// - **收到客户端数据不刷新计时**（LoginGate oracle 的刷新语句全被注释）：
///   t≈8s 发一帧，仍必须在 t≤15s 超时；若被刷新，违约时刻会推到 ~19s ⇒ 本用例红；
/// - 超时路径**不得**向 LoginSrv 发 `Leave`（oracle 先摘会话，`Disconnected` 取不到会话）。
#[tokio::test]
async fn idle_session_times_out_with_oracle_frame_then_close() {
    let (srv_port, srv_task) = fake_login_srv().await;
    let gate_port = free_port();
    let dir = write_conf_with_timeout(gate_port, srv_port, 10_000);
    let conf = dir.join("config.conf");

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_login-gate"))
        .arg(&conf)
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let t0 = tokio::time::Instant::now();
    let mut client = connect_retry(gate_port).await;

    // t≈8s：发一帧。LoginGate 的 oracle 不因客户端数据刷新计时。
    tokio::time::sleep(Duration::from_secs(8)).await;
    client.write_all(b"#1ABCdef!").await.unwrap();
    client.flush().await.unwrap();

    // 该帧被原样回显（假 LoginSrv 的应答），先消费掉。
    let mut echo = vec![0u8; 9];
    tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut echo))
        .await
        .expect("5s 内未收到中继回帧")
        .unwrap();
    assert_eq!(echo, b"#1ABCdef!");

    // 从连接时刻起 15s 内必须超时：正常 ~11s（1s 扫描）；若 8s 的帧刷新了计时，
    // 违约时刻会推到 ~19s，本用例即红。
    let mut frame = vec![0u8; 16];
    tokio::time::timeout_at(t0 + Duration::from_secs(15), client.read_exact(&mut frame))
        .await
        .expect("超时未触发：LoginGate 收到客户端数据不得刷新计时（oracle 语句被注释）")
        .unwrap();
    assert_eq!(
        frame,
        vec![
            0x23, 0x64, 0x64, 0x68, 0x6b, 0x64, 0x68, 0x6a, 0x6b, 0x64, 0x64, 0x68, 0x6b, 0x64,
            0x64, 0x68,
        ],
        "超时帧必须逐字节等于 C# LoginGate 的线上形态（含截断缺陷）"
    );

    // 发帧后关连接（C# `TimedService` 同一扫描 `CloseSession`）。
    let mut one = [0u8; 1];
    let n = tokio::time::timeout(Duration::from_secs(5), client.read(&mut one))
        .await
        .expect("超时帧之后必须关闭连接")
        .unwrap();
    assert_eq!(n, 0, "超时后必须 FIN（read 返回 0）");

    child.kill().await.ok();
    drop(client);
    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 LoginSrv 未按时结束")
        .unwrap();
    assert!(
        !got.iter().any(|m| m.kind == ServerDataType::Leave),
        "超时路径不得给 LoginSrv 发 Leave（oracle 先摘会话）：{got:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 第二轮复验 Important（进程级）：LoginSrv 接受后关闭 ⇒ 客户端**收不到**超时帧。
///
/// oracle：`ClientThread.ClientSocketDisconnect` 把 `SessionArray` 槽位全置 null
/// ⇒ `TimedService.ProcessDelayMsg` 取不到会话 ⇒ 超时动作整体不发生；
/// `ClientSession.SendDefMessage` 另有 `IsConnected` 早退。
///
/// 牙：删掉超时支的 `downstream_alive` 守卫 ⇒ 本用例在 t≈10s 收到 16B 帧 ⇒ 红。
#[tokio::test]
async fn downstream_disconnect_suppresses_idle_timeout_frame() {
    let srv_port = fake_srv_accept_enter_then_close().await;
    let gate_port = free_port();
    let dir = write_conf_with_timeout(gate_port, srv_port, 10_000);
    let conf = dir.join("config.conf");

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_login-gate"))
        .arg(&conf)
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let t0 = tokio::time::Instant::now();
    let mut client = connect_retry(gate_port).await;

    // LoginGate 扫描节奏 1s、阈值 10s：若守卫失效，t≈10s 必发 16B 超时帧并 FIN。
    // 等到 15s 仍无任何字节（既无帧也无 FIN）才算通过。
    let mut buf = [0u8; 32];
    let got = tokio::time::timeout_at(t0 + Duration::from_secs(15), client.read(&mut buf)).await;
    assert!(
        got.is_err(),
        "LoginSrv 断开后不得发生超时动作（不得发帧/不得 FIN）；实得 {got:?}"
    );

    child.kill().await.ok();
    std::fs::remove_dir_all(&dir).ok();
}
