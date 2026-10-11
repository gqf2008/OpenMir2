// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

//! GameSvr（S1）的**进程级自测**：假 GameGate + 真 `game-svr` 二进制的 20B 头 `GM_*` 链路。
//!
//! 判据（口径都来自 `src/M2Server/Net/ChannelMessageHandler.cs`）：
//! 1. `GM_OPEN` ⇒ 回 `GM_SERVERUSERINDEX`，`Socket`/`SessionId` 原样、**`SessionIndex` = 下标+1**（`:157/:447-462`）、`PackLength=0`；
//! 2. 槽表 **append-only**：关掉再开，索引继续往后长（`:442-444`）；`GM_CLOSE` 按 **socket** 关（`:161`）；
//! 3. 发送口的阻塞/确认规则（`:50-95`）：累积量到 `CheckBlock*10` 时**先发 `GM_RECEIVE_OK`** 再发正文；
//!    网关回 `GM_RECEIVE_OK` 后计数清零、恢复直接发送；
//! 4. `GM_DATA` 在用户**还没认证**时走 `DoClientCertification` 分支（`:201-225`）⇒ S1 记录为"认证前帧"交给 sim 线程（认证语义属 S2）。

use std::time::Duration;

use mir2_protocol::frame::ServerMessage;
use mir2_protocol::messages::grobal2;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

fn write_conf(dir: &std::path::Path, port: u16, check_block: usize) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("Server.conf"),
        format!(
            "[Server]\nDBPort=6000\nCheckBlock={check_block}\nGateAddr= 127.0.0.1\n\
             GatePort={port}\nWorldMaps=64x64\n"
        ),
    )
    .unwrap();
}

/// 造一帧网关 → GameSvr 的 20B 头 + body。
fn frame(ident: u8, socket: i32, session_id: u16, session_index: i32, body: &[u8]) -> Vec<u8> {
    let head = ServerMessage {
        packet_code: grobal2::PacketCode,
        socket,
        session_id,
        ident: u16::from(ident),
        session_index,
        pack_length: i32::try_from(body.len()).unwrap(),
    };
    let mut v = head.to_bytes().to_vec();
    v.extend_from_slice(body);
    v
}

async fn recv_frame(sock: &mut TcpStream) -> Option<(ServerMessage, Vec<u8>)> {
    let mut head = [0u8; ServerMessage::SIZE];
    if sock.read_exact(&mut head).await.is_err() {
        return None;
    }
    let msg = ServerMessage::from_bytes(&head)?;
    let mut body = vec![0u8; msg.body_len()];
    if !body.is_empty() && sock.read_exact(&mut body).await.is_err() {
        return None;
    }
    Some((msg, body))
}

async fn connect_with_retry(port: u16) -> TcpStream {
    for _ in 0..100 {
        if let Ok(c) = TcpStream::connect(("127.0.0.1", port)).await {
            c.set_nodelay(true).ok();
            return c;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("game-svr 未在预期时间内监听 {port}");
}

struct Child {
    proc: tokio::process::Child,
    dir: std::path::PathBuf,
    log: std::path::PathBuf,
}

impl Child {
    fn spawn(port: u16, check_block: usize, tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("gs-{tag}-{}-{port}", std::process::id()));
        write_conf(&dir, port, check_block);
        let log = dir.join("server.out.log");
        let out = std::fs::File::create(&log).unwrap();
        let proc = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-svr"))
            .arg(dir.join("Server.conf"))
            .stdout(out)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        Self { proc, dir, log }
    }

    fn stdout(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    async fn stop(mut self) {
        self.proc.kill().await.ok();
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// 判据 1：`GM_OPEN` ⇒ `GM_SERVERUSERINDEX`（索引 1 起、Socket/SessionId 原样、PackLength=0）。
#[tokio::test]
async fn gm_open_gets_server_user_index_one_based() {
    let port = free_port();
    let child = Child::spawn(port, 12288, "open");
    let mut gate = connect_with_retry(port).await;
    gate.write_all(&frame(grobal2::GM_OPEN, 1001, 4242, 0, b"192.168.1.7"))
        .await
        .unwrap();
    let (msg, body) = tokio::time::timeout(Duration::from_secs(5), recv_frame(&mut gate))
        .await
        .expect("5s 内应回 GM_SERVERUSERINDEX")
        .expect("连接不该断");
    assert_eq!(msg.ident, u16::from(grobal2::GM_SERVERUSERINDEX));
    assert_eq!(msg.socket, 1001, "Socket 必须原样回（:455）");
    assert_eq!(msg.session_id, 4242, "SessionId 必须原样回（:456）");
    assert_eq!(msg.session_index, 1, "SessionIndex = 下标+1（:157/:458）");
    assert_eq!(msg.pack_length, 0);
    assert!(body.is_empty());
    child.stop().await;
}

/// 判据 2：索引 append-only（关了也不回收）；`GM_CLOSE` 按 socket 关。
#[tokio::test]
async fn index_is_append_only_and_close_is_by_socket() {
    let port = free_port();
    let child = Child::spawn(port, 12288, "close");
    let mut gate = connect_with_retry(port).await;
    gate.write_all(&frame(grobal2::GM_OPEN, 1001, 1, 0, b"1.1.1.1"))
        .await
        .unwrap();
    let (m1, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(m1.session_index, 1);
    gate.write_all(&frame(grobal2::GM_OPEN, 1002, 2, 0, b"1.1.1.2"))
        .await
        .unwrap();
    let (m2, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(m2.session_index, 2);
    // 按 socket 关掉第一个
    gate.write_all(&frame(grobal2::GM_CLOSE, 1001, 0, 0, &[]))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    // 再开一个：索引必须是 3（不回收下标）
    gate.write_all(&frame(grobal2::GM_OPEN, 1003, 3, 0, b"1.1.1.3"))
        .await
        .unwrap();
    let (m3, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(
        m3.session_index, 3,
        "槽表 append-only（oracle :442-444 的 userList.Add），关了也不回收"
    );
    let log = child.stdout();
    assert!(
        log.contains("用户关闭：socket=1001 index=1"),
        "GM_CLOSE 必须按 socket 关到 index=1；日志实得：{log}"
    );
    child.stop().await;
}

/// 判据 3：发送口阻塞/确认（`CheckBlock*10` 阈值 ⇒ 先发 `GM_RECEIVE_OK`；收到网关的 `GM_RECEIVE_OK` 后恢复）。
///
/// `CheckBlock=3` ⇒ 阈值 = 30 字节；每条 `GM_SERVERUSERINDEX` 是 20 字节 ⇒ 第二条起会先撞阈值。
#[tokio::test]
async fn blocked_send_sends_receive_ok_then_resumes() {
    let port = free_port();
    let child = Child::spawn(port, 3, "block");
    let mut gate = connect_with_retry(port).await;
    // 第一条：0 + 20 < 30 ⇒ 直接回索引
    gate.write_all(&frame(grobal2::GM_OPEN, 1, 1, 0, b"a"))
        .await
        .unwrap();
    let (m1, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(m1.ident, u16::from(grobal2::GM_SERVERUSERINDEX));
    assert_eq!(m1.session_index, 1);
    // 第二条：20 + 20 >= 30 ⇒ 先 GM_RECEIVE_OK，再索引
    gate.write_all(&frame(grobal2::GM_OPEN, 2, 2, 0, b"b"))
        .await
        .unwrap();
    let (a1, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(
        a1.ident,
        u16::from(grobal2::GM_RECEIVE_OK),
        "撞阈值必须先发 GM_RECEIVE_OK（:80）"
    );
    let (m2, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(m2.ident, u16::from(grobal2::GM_SERVERUSERINDEX));
    assert_eq!(m2.session_index, 2);
    // 网关确认（清零计数）后，第三条又能直接回
    gate.write_all(&frame(grobal2::GM_RECEIVE_OK, 0, 0, 0, &[]))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    gate.write_all(&frame(grobal2::GM_OPEN, 3, 3, 0, b"c"))
        .await
        .unwrap();
    let (m3, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(m3.ident, u16::from(grobal2::GM_SERVERUSERINDEX));
    assert_eq!(m3.session_index, 3);
    child.stop().await;
}

/// 判据 4：`GM_DATA` 在用户未认证时走"认证前"分支（oracle `:201-225` 的 `DoClientCertification`），
/// 并把事件交给 sim 线程（S1 只记录；认证语义属 S2）。
#[tokio::test]
async fn gm_data_before_certification_goes_to_sim() {
    let port = free_port();
    let child = Child::spawn(port, 12288, "data");
    let mut gate = connect_with_retry(port).await;
    gate.write_all(&frame(grobal2::GM_OPEN, 777, 555, 0, b"10.0.0.9"))
        .await
        .unwrap();
    let (m, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(m.session_index, 1);
    // 12B 命令头 + 4B 尾（尾按 EDCode 编码；oracle 侧尾是 `DeCodeString(msgBuff[12..^1])`）
    let mut body = vec![0u8; 12];
    body[4..6].copy_from_slice(&3030u16.to_le_bytes()); // CM_SAY
    body.extend_from_slice(&mir2_protocol::edcode::encode(b"hi"));
    gate.write_all(&frame(grobal2::GM_DATA, 777, 555, 1, &body))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let log = child.stdout();
    assert!(
        log.contains("认证前帧") && log.contains("index=1") && log.contains("ip=10.0.0.9"),
        "未认证的 GM_DATA 必须走认证前分支并交给 sim 线程；日志实得：{log}"
    );
    child.stop().await;
}

/// 缺配置必须非零退出（M1④"失败要响"）。
#[tokio::test]
async fn missing_config_exits_nonzero() {
    let dir = std::env::temp_dir().join(format!("gs-noconf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conf = dir.join("Server.conf");
    std::fs::write(&conf, "[Server]\nGateAddr=127.0.0.1\n").unwrap(); // 缺 GatePort/CheckBlock/WorldMaps
    let out = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .output()
        .await
        .unwrap();
    assert!(!out.status.success(), "缺配置必须非零退出（M1④）");
    let err =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("缺键"), "错误信息应指明缺键，实得：{err}");
    std::fs::remove_dir_all(&dir).ok();
}
