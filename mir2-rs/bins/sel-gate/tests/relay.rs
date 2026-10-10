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

use std::time::Duration;

use mir2_protocol::frame::ServerDataPacket;
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// 起一个"假 DBSrv"：接受一条连接，把收到的第一条 Data 帧原样回给同一 SocketId。
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
    (port, h)
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
    let (srv_port, srv_task) = fake_login_srv().await;
    let gate_port = free_port();
    let dir = write_conf(gate_port, srv_port);
    let conf = dir.join("config.conf");

    // 起网关（本测试编译出的二进制）
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_sel-gate"))
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
