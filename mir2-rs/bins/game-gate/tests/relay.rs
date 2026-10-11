// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

//! GameGate 的**进程级中继自测**：假 GameSvr + 假客户端，不依赖 MySQL 与整栈。
//!
//! 判据（对齐协议梳理 §7.5 的实机取证，不是自洽往返）：
//! 1. 客户端一连上，网关发 `GM_OPEN`，体 = 客户端 IP 字符串（`PackLength` 为其长度）、
//!    `Socket` = 客户端 handle、`SessionIndex` = 0，且 `SessionId ∈ 0..=9999`（随机）；
//! 2. 客户端发 ≥16 字节 body 的帧 ⇒ 网关以 `GM_DATA` 转发，**body 逐字节不变**，`SessionId` 与 GM_OPEN 一致；
//! 3. 20B 头 + 1 字节 body（心跳）**不得**被转发；
//! 4. body < 16 字节 ⇒ 网关关会话（"异常消息封包"）并补发 `GM_CLOSE`（`session_id=0, socket=handle`）；
//! 5. 缺配置时进程**非零退出**（M1 验收④）。

use std::time::Duration;

use mir2_protocol::frame::ServerMessage;
use mir2_protocol::messages::grobal2;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// 假 GameSvr：接受一条连接，把收到的每帧 `(header, body)` 收进 Vec 返回。
async fn fake_game_svr() -> (u16, tokio::task::JoinHandle<Vec<(ServerMessage, Vec<u8>)>>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let h = tokio::spawn(async move {
        let mut got = Vec::new();
        let (mut sock, _) = l.accept().await.unwrap();
        loop {
            let mut head = [0u8; ServerMessage::SIZE];
            if sock.read_exact(&mut head).await.is_err() {
                break;
            }
            let msg = ServerMessage::from_bytes(&head).unwrap();
            let mut body = vec![0u8; msg.body_len()];
            if !body.is_empty() && sock.read_exact(&mut body).await.is_err() {
                break;
            }
            got.push((msg, body));
        }
        got
    });
    (port, h)
}

fn write_conf(dir: &std::path::Path, srv_port: u16, gate_port: u16) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("config.conf"),
        format!(
            "[GameGate]\nCount=1\nServerAddr1=127.0.0.1\nServerPort1={srv_port}\nGatePort1={gate_port}\nGateAddress1=127.0.0.1\n"
        ),
    )
    .unwrap();
}

fn frame(ident: u16, session_id: u16, body: &[u8]) -> Vec<u8> {
    let head = ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: 0,
        session_id,
        ident,
        session_index: 0,
        pack_length: i32::try_from(body.len()).unwrap(),
    };
    let mut v = head.to_bytes().to_vec();
    v.extend_from_slice(body);
    v
}

async fn connect_with_retry(port: u16) -> TcpStream {
    for _ in 0..100 {
        if let Ok(c) = TcpStream::connect(("127.0.0.1", port)).await {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("网关未在预期时间内监听 {port}");
}

#[tokio::test]
async fn relays_open_data_heartbeat_and_close() {
    let (srv_port, srv_task) = fake_game_svr().await;
    let gate_port = free_port();
    let dir = std::env::temp_dir().join(format!("gg-relay-{}-{}", std::process::id(), gate_port));
    write_conf(&dir, srv_port, gate_port);

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-gate"))
        .arg(dir.join("config.conf"))
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let mut client = connect_with_retry(gate_port).await;

    // 判据 2/3：先发心跳（1 字节 body，不得转发），再发一帧真数据（≥16 字节，必须原样转发）
    client.write_all(&frame(11, 0, b"*")).await.unwrap();
    let payload: Vec<u8> = (0..=31u8).collect();
    client.write_all(&frame(11, 0, &payload)).await.unwrap();
    client.flush().await.unwrap();

    // 判据 4：body < 16 ⇒ 网关关会话（并补发 GM_CLOSE）
    client
        .write_all(&frame(11, 0, b"\x01\x02\x03"))
        .await
        .unwrap();
    client.flush().await.unwrap();

    // 等网关看到客户端断开
    let mut buf = [0u8; 1];
    let _ = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf)).await;
    child.kill().await.ok();
    drop(client);

    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未按时结束")
        .unwrap();

    // 判据 1：GM_OPEN 的形状（实机取证：体=IP 串、SessionIndex=0、Socket=handle）
    let (open, open_body) = got.first().expect("必须先收到 GM_OPEN");
    assert_eq!(open.ident, u16::from(grobal2::GM_OPEN));
    assert_eq!(open_body, b"127.0.0.1");
    assert_eq!(open.pack_length, 9, "PackLength 必须等于 IP 串长度");
    assert_eq!(open.session_index, 0);
    assert!(
        (0..10_000).contains(&open.session_id),
        "SessionId 必须是随机 4 位（0..=9999），实得 {}",
        open.session_id
    );
    let sid = open.session_id;

    // 判据 2/3：恰好一条 GM_DATA（心跳没被转发），body 逐字节不变，SessionId 一致
    let datas: Vec<&(ServerMessage, Vec<u8>)> = got
        .iter()
        .filter(|(m, _)| m.ident == u16::from(grobal2::GM_DATA))
        .collect();
    assert_eq!(
        datas.len(),
        1,
        "心跳不得被转发：期望恰好 1 条 GM_DATA，实得 {}",
        datas.len()
    );
    assert_eq!(
        datas[0].1, payload,
        "GM_DATA 的 body 必须与客户端原始 body 逐字节相同"
    );
    assert_eq!(datas[0].0.session_id, sid, "GM_DATA 必须带同一 SessionId");

    // 判据 4：有 GM_CLOSE 且 socket=handle、session_id=0（实机取证）
    let closes: Vec<&(ServerMessage, Vec<u8>)> = got
        .iter()
        .filter(|(m, _)| m.ident == u16::from(grobal2::GM_CLOSE))
        .collect();
    assert_eq!(closes.len(), 1, "断开必须补发一条 GM_CLOSE");
    assert_eq!(closes[0].0.session_id, 0);
    assert_eq!(
        closes[0].0.socket, open.socket,
        "GM_CLOSE 的 socket 必须与 GM_OPEN 的 handle 一致"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn missing_config_exits_nonzero() {
    let dir = std::env::temp_dir().join(format!("gg-noconf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conf = dir.join("config.conf");
    std::fs::write(&conf, "[GameGate]\nCount=1\nServerAddr1=127.0.0.1\n").unwrap(); // 缺端口等键
    let out = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-gate"))
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
