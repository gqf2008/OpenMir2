// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

//! GameSvr（S1）的**进程级自测**：假 GameGate + 真 `game-svr` 二进制的 20B 头 `GM_*` 链路。
//!
//! 判据（口径都来自 `src/M2Server/Net/ChannelMessageHandler.cs`）：
//! 1. `GM_OPEN` ⇒ 回 `GM_SERVERUSERINDEX`，`Socket`/`SessionId` 原样、**`SessionIndex` = 下标+1**（`:157/:447-462`）、`PackLength=0`；
//! 2. 槽表**最小空闲槽复用**（`OpenNewUser` `:434-444`：先找第一个 `null` 槽，找不到才 `Add`）——**不是 append-only**；
//!    `GM_CLOSE` 按 **socket** 关（`:161`）；
//! 3. 发送口的阻塞/确认规则（`:50-95`）：累积量到 `CheckBlock*10` 时**先发 `GM_RECEIVE_OK`** 再发正文；
//!    网关回 `GM_RECEIVE_OK` 后计数清零、恢复直接发送；
//! 4. `GM_DATA` 的分支门是**已进世界**（`:201` 的 `PlayObject/WorldEngine`），不是 `Certification`：
//!    未进世界 ⇒ 走 `DoClientCertification`（已认证的窗口里它 `:307` 空转 ⇒ 丢帧）；无 `!` ⇒ `Ignored`（记录给 sim 线程）。

use std::time::Duration;

use mir2_protocol::frame::ServerMessage;
use mir2_protocol::messages::grobal2;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// 十六进制串 → 字节（把**oracle 实测字节**原样钉进用例）。
fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

fn write_conf(dir: &std::path::Path, port: u16, check_block: usize, ids_port: u16, db_port: u16) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("Server.conf"),
        format!(
            "[Server]\nDBPort=6000\nCheckBlock={check_block}\nGateAddr= 127.0.0.1\n\
             GatePort={port}\nIDSAddr= 127.0.0.1\nIDSPort={ids_port}\nDBAddr=127.0.0.1\nDBPort={db_port}\nWorldMaps=64x64\n"
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
    fn spawn(port: u16, check_block: usize, ids_port: u16, db_port: u16, tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("gs-{tag}-{}-{port}", std::process::id()));
        write_conf(&dir, port, check_block, ids_port, db_port);
        let log = dir.join("server.out.log");
        let out = std::fs::File::create(&log).unwrap();
        let err = out.try_clone().unwrap();
        let proc = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-svr"))
            .arg(dir.join("Server.conf"))
            .stdout(out)
            .stderr(err)
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
    let child = Child::spawn(port, 12288, free_port(), free_port(), "open");
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

/// 判据 2：`GM_CLOSE` 按 socket 关；关掉的槽**被复用**（`OpenNewUser` 先找最小空闲槽，`:434-444`）。
#[tokio::test]
async fn close_frees_slot_for_reuse() {
    let port = free_port();
    let child = Child::spawn(port, 12288, free_port(), free_port(), "close");
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
    // 再开一个：**复用最小空闲槽**（下标 0）⇒ 索引回到 1（oracle :434-441）
    gate.write_all(&frame(grobal2::GM_OPEN, 1003, 3, 0, b"1.1.1.3"))
        .await
        .unwrap();
    let (m3, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(
        m3.session_index, 1,
        "关掉 1 号后，新用户在最小空闲槽（下标 0）⇒ 索引 1（不是 append-only）"
    );
    let log = child.stdout();
    assert!(
        log.contains("用户关闭：socket=1001 index=1"),
        "GM_CLOSE 必须按 socket 关到 index=1；日志实得：{log}"
    );
    child.stop().await;
}

// 判据 3（发送口的阻塞/确认规则）**已改**：F1 之后本切片唯一的出站载荷（`GM_SERVERUSERINDEX`）按 oracle
// 直发（不经 `ProcessBufferSend`），所以这条进程级用例失去前提、已删除；该规则的判据改为 `main.rs` 里
// `send_decision_matches_process_buffer_send` 的**纯函数单测**（含"超时清零后那一包也压住"）。
// S3 的出包接上后，这里要补一条"真正走 ProcessBufferSend 的载荷"的进程级用例（登记在卡上）。

/// 判据 4：`GM_DATA` 在**未进世界**时走 `DoClientCertification`；这条帧连 `!` 都没有 ⇒ oracle 的 `if`
/// 不成立（`Ignored`），事件交给 sim 线程记录。
#[tokio::test]
async fn gm_data_before_certification_goes_to_sim() {
    let port = free_port();
    let child = Child::spawn(port, 12288, free_port(), free_port(), "data");
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
        log.contains("认证链返回 Ignored") && log.contains("index=1"),
        "无 `!` 的帧 ⇒ 认证链返回 Ignored（oracle 的 `if` 不成立）并交给 sim 线程；日志实得：{log}"
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

/// 判据（认证失败路径）：把**真 C# GameGate 实测的那条合成登录包**当 `GM_DATA` 发进来，
/// 准入表里没有对应会话 ⇒ 必须走 oracle 的失败支（`会话验证失败` + `CloseUser`，`ChannelMessageHandler.cs:340-353`），
/// 且槽位被清（后续同会话的 `GM_DATA` 落到"用户不存在"）。
#[tokio::test]
async fn login_packet_without_admission_fails_certification() {
    // 真 C# GameGate 发出的合成登录包（98B；见 E:\tmp\openmir2-gamegate-imp\oracle-login-cap\out-csharp-raw.json）
    const ORACLE_LOGIN_BODY: &str = "23305e5e3c716f6d446f6e6f5478615f3c786e61497b5f623f5f6162586b6062586b61625a675f615a5f60605a6b60615a6a62603f6b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b5f603f21";
    let port = free_port();
    let child = Child::spawn(port, 12288, free_port(), free_port(), "cert");
    let mut gate = connect_with_retry(port).await;
    gate.write_all(&frame(grobal2::GM_OPEN, 900, 42, 0, b"10.0.0.5"))
        .await
        .unwrap();
    let (m, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(m.session_index, 1);
    gate.write_all(&frame(
        grobal2::GM_DATA,
        900,
        42,
        1,
        &hex(ORACLE_LOGIN_BODY),
    ))
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let log = child.stdout();
    // F7（reviewer-2）：断言要能区分"打对了字段"与"只是提到了 loadbot1"——锚到完整形态
    assert!(
        log.contains("会话验证失败.Account:loadbot1 SessionId:2"),
        "无准入记录 ⇒ 必须走认证失败支并打出账号/会话号（完整形态）；日志实得：{log}"
    );
    // 槽位被清：同一会话再发命令帧 ⇒ "用户不存在"
    gate.write_all(&frame(grobal2::GM_DATA, 900, 42, 1, &[0u8; 16]))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        child.stdout().contains("用户不存在"),
        "CloseUser 后同 socket/session 的 GM_DATA 必须落到「用户不存在」"
    );
    child.stop().await;
}

/// **判据（认证闭环，S2a+S2b）**：假 IDS 先发 `(100/loadbot1/2/0/1/127.0.0.1/3600)` 把会话灌进准入表，
/// 假网关再把**真 C# GameGate 实测的 98B 合成登录包**当 `GM_DATA` 发进来 ⇒ 必须走**认证通过**支
/// （sim 线程的「认证通过 ⇒ 应取角色进世界」日志里能看到 loadbot1/lb1a）。
///
/// 这条判据把 S2a（认证链）与 S2b（IDS 通知链路）串成一条端到端链；缺任一半都会红。
#[tokio::test]
async fn ids_notification_enables_login_certification() {
    const ORACLE_LOGIN_BODY: &str = "23305e5e3c716f6d446f6e6f5478615f3c786e61497b5f623f5f6162586b6062586b61625a675f615a5f60605a6b60615a6a62603f6b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b5f603f21";
    // 假 IDS：接受连接后立刻发一条 SS_OPENSESSION(100)
    let ids = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ids_port = ids.local_addr().unwrap().port();
    tokio::spawn(async move {
        if let Ok((mut s, _)) = ids.accept().await {
            let msg = b"(100/loadbot1/2/0/1/127.0.0.1/3600)";
            let _ = tokio::io::AsyncWriteExt::write_all(&mut s, msg).await;
            let _ = tokio::io::AsyncWriteExt::flush(&mut s).await;
            tokio::time::sleep(Duration::from_secs(6)).await; // 别断线（断线会清空准入表，照 oracle）
        }
    });
    let port = free_port();
    let child = Child::spawn(port, 12288, ids_port, free_port(), "ids");
    let mut gate = connect_with_retry(port).await;
    gate.write_all(&frame(grobal2::GM_OPEN, 501, 77, 0, b"127.0.0.1"))
        .await
        .unwrap();
    let (m, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(m.session_index, 1);
    tokio::time::sleep(Duration::from_millis(400)).await; // 等 IDS 通知到达
    gate.write_all(&frame(
        grobal2::GM_DATA,
        501,
        77,
        1,
        &hex(ORACLE_LOGIN_BODY),
    ))
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let log = child.stdout();
    assert!(
        log.contains("认证通过"),
        "IDS 通知 + 真登录包 ⇒ 必须认证通过；日志实得：{log}"
    );
    assert!(
        log.contains("Account:loadbot1") && log.contains("ChrName:lb1a"),
        "认证通过的日志里必须有账号/角色名；日志实得：{log}"
    );
    // S2b-2：认证通过 ⇒ 进加载队列；**没有 DB 链路** ⇒ 照 oracle 的 `QueryCharacterData` 失败支
    // （`SendOutConnectMsg` 在 oracle 里是"待实现"、不发帧）⇒ 清槽 + 日志
    tokio::time::sleep(Duration::from_millis(600)).await;
    let log = child.stdout();
    assert!(
        log.contains("读取用户数据失败,踢出用户. Account:loadbot1 ChrName:lb1a"),
        "无 DB 链路 ⇒ 必须走读档失败支并打出账号/角色名；日志实得：{log}"
    );
    // 槽位被清：同 socket/session 再发一帧 ⇒ "用户不存在"
    gate.write_all(&frame(grobal2::GM_DATA, 501, 77, 1, &[0u8; 16]))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(
        child.stdout().contains("用户不存在"),
        "读档失败清槽后，同 socket/session 的 GM_DATA 必须落到「用户不存在」"
    );
    child.stop().await;
}

/// **判据（S2b-2b 的第一半）**：DB 链路连上时，读档那一步必须把 `DB_LOADHUMANRCD` 请求**按 oracle 的字节**发出去。
///
/// oracle 的字节由部署件序列化器产出（`E:\tmp\openmir2-gamegate-imp\dbgen\db-request-baseline.txt`），
/// 本用例断言"假 DBSvr 收到的帧"等于 `dbclient` 编码器的输出（后者已由 3 条 oracle 字节单测钉住），
/// 并顺带断言：DB 在线 ⇒ **不**走"读取用户数据失败"（与 `ids_notification_...` 那条形成对照）。
#[tokio::test]
async fn db_load_request_is_sent_with_oracle_bytes() {
    const ORACLE_LOGIN_BODY: &str = "23305e5e3c716f6d446f6e6f5478615f3c786e61497b5f623f5f6162586b6062586b61625a675f615a5f60605a6b60615a6a62603f6b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b5f603f21";
    // 假 DBSvr：收一帧就返回
    let dbsrv = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let db_port = dbsrv.local_addr().unwrap().port();
    let dbsrv_task = tokio::spawn(async move {
        let (mut s, _) = dbsrv.accept().await.unwrap();
        let mut buf = vec![0u8; 512];
        let n = tokio::io::AsyncReadExt::read(&mut s, &mut buf)
            .await
            .unwrap_or(0);
        buf.truncate(n);
        buf
    });
    // 假 IDS：灌一条准入
    let ids = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ids_port = ids.local_addr().unwrap().port();
    tokio::spawn(async move {
        if let Ok((mut s, _)) = ids.accept().await {
            let msg = b"(100/loadbot1/2/0/1/127.0.0.1/3600)";
            let _ = tokio::io::AsyncWriteExt::write_all(&mut s, msg).await;
            tokio::time::sleep(Duration::from_secs(6)).await;
        }
    });
    let port = free_port();
    let child = Child::spawn(port, 12288, ids_port, db_port, "dbreq");
    let mut gate = connect_with_retry(port).await;
    gate.write_all(&frame(grobal2::GM_OPEN, 601, 88, 0, b"127.0.0.1"))
        .await
        .unwrap();
    let (m, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(m.session_index, 1);
    tokio::time::sleep(Duration::from_millis(400)).await;
    gate.write_all(&frame(
        grobal2::GM_DATA,
        601,
        88,
        1,
        &hex(ORACLE_LOGIN_BODY),
    ))
    .await
    .unwrap();
    let got = tokio::time::timeout(Duration::from_secs(6), dbsrv_task)
        .await
        .expect("假 DBSvr 未在 6s 内收到请求")
        .unwrap();
    // 逐段核对（结构与内容都对着 oracle 实测件：msg 13B / packet 59B / 帧 126B）
    assert_eq!(got.len(), 126, "oracle 实测帧长 126（6 头 + 120 体）");
    assert_eq!(&got[..4], &0xAA9A_AA9Au32.to_le_bytes(), "帧头 PacketCode");
    assert_eq!(
        u16::from_le_bytes([got[4], got[5]]),
        120,
        "PacketLen = 体长"
    );
    assert_eq!(got[6], 0x04, "ServerRequestData 的成员数前缀");
    assert_eq!(
        i32::from_le_bytes([got[7], got[8], got[9], got[10]]),
        1,
        "QueryId 从 1 起（`GetQueryId`）"
    );
    // Message 段：长度 18 + EDCode(13B 的 ServerRequestMessage)，解出来必须等于 oracle 的 13 字节
    let msg_len = i32::from_le_bytes([got[11], got[12], got[13], got[14]]);
    assert_eq!(msg_len, 18, "EDCode(13) 实测 18 字节");
    let msg = mir2_protocol::edcode::decode(&got[15..15 + 18]);
    assert_eq!(
        msg,
        vec![0x05, 0, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0],
        "ServerRequestMessage（成员数 5 + Recog=0 + Ident=100(DB_LOADHUMANRCD) + 三个 0）"
    );
    // Packet 段：长度 79 + EDCode(59B 的 LoadCharacterData)，解出来等于 oracle 的 59 字节
    let pkt_off = 15 + 18;
    let pkt_len = i32::from_le_bytes([
        got[pkt_off],
        got[pkt_off + 1],
        got[pkt_off + 2],
        got[pkt_off + 3],
    ]);
    assert_eq!(pkt_len, 79, "EDCode(59) 实测 79 字节");
    let oracle_packet = hex("04080000006c006f006100640062006f0074003100040000006c00620031006100090000003100320037002e0030002e0030002e00310002000000");
    assert_eq!(
        mir2_protocol::edcode::decode(&got[pkt_off + 4..pkt_off + 4 + 79]),
        oracle_packet,
        "LoadCharacterData 必须是 oracle 的 59 字节（account=loadbot1 / chr=lb1a / addr=127.0.0.1 / session=2）"
    );
    // Sign 段：长度 6（EDCode(4B)），内容 = EDCode(LE32(MakeLong(queryId^170, msglen+pktlen+6)))
    let sign_off = pkt_off + 4 + 79;
    let sign_len = i32::from_le_bytes([
        got[sign_off],
        got[sign_off + 1],
        got[sign_off + 2],
        got[sign_off + 3],
    ]);
    assert_eq!(sign_len, 6, "EDCode(4) 实测 6 字节");
    let sign_id: u32 = (103u32 << 16) | u32::from(1u16 ^ 170);
    assert_eq!(
        mir2_protocol::edcode::decode(&got[sign_off + 4..sign_off + 4 + 6]),
        sign_id.to_le_bytes().to_vec(),
        "Sign = MakeLong(queryId^170, msg+pkt+6)（hi<<16|lo）"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    let log = child.stdout();
    assert!(
        !log.contains("读取用户数据失败"),
        "DB 在线 ⇒ 不得走失败支；日志实得：{log}"
    );
    child.stop().await;
}

/// **判据（S2b-2b 后半的第一半）**：假 DBSvr 收到请求后**回一帧**（同信封 + 签名）——
/// 好签名 ⇒ 必须被接受（`执行任务Id:1成功` + sim 侧 `收到角色数据 … Ident=101 Recog=1`）；
/// 坏签名 ⇒ 必须被拒（`DB 应答被拒`，且**不**出现 `执行任务Id:1成功`）。
///
/// 应答字节在测试里独立合成（不调被测编码器）：`ServerRequestData` = 前缀 4 + QueryId + 3×(len+bytes)。
#[tokio::test]
async fn db_reply_accepted_with_good_sign_and_rejected_with_bad_sign() {
    const ORACLE_LOGIN_BODY: &str = "23305e5e3c716f6d446f6e6f5478615f3c786e61497b5f623f5f6162586b6062586b61625a675f615a5f60605a6b60615a6a62603f6b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b5f603f21";

    // 合成一帧应答：Message = EDCode(成员数 5 + Recog=1 + Ident=101(DBR_LOADHUMANRCD) + 三个 0)
    fn reply_frame(query_id: i32, packet: &[u8], bad_sign: bool) -> Vec<u8> {
        use mir2_protocol::edcode;
        let mut msg = vec![5u8];
        msg.extend_from_slice(&1i32.to_le_bytes()); // Recog = 1（`:150` 的判据）
        msg.extend_from_slice(&1100u16.to_le_bytes()); // DBR_LOADHUMANRCD（Messages 里是 1100，不是 101）
        msg.extend_from_slice(&[0u8; 6]);
        let m = edcode::encode(&msg);
        let p = edcode::encode(packet);
        let n_len = m.len() + p.len() + 6;
        let lo = (u16::try_from(query_id ^ 170).unwrap()) ^ u16::from(bad_sign);
        let sign_id = (u32::from(u16::try_from(n_len).unwrap()) << 16) | u32::from(lo);
        let s = edcode::encode(&sign_id.to_le_bytes());
        let mut body = vec![4u8];
        body.extend_from_slice(&query_id.to_le_bytes());
        for part in [&m, &p, &s] {
            body.extend_from_slice(&i32::try_from(part.len()).unwrap().to_le_bytes());
            body.extend_from_slice(part);
        }
        let mut frame = 0xAA9A_AA9Au32.to_le_bytes().to_vec();
        frame.extend_from_slice(&u16::try_from(body.len()).unwrap().to_le_bytes());
        frame.extend_from_slice(&body);
        frame
    }

    for bad in [false, true] {
        let dbsrv = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let db_port = dbsrv.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = dbsrv.accept().await.unwrap();
            let mut buf = vec![0u8; 512];
            let _ = tokio::io::AsyncReadExt::read(&mut s, &mut buf).await;
            let frame = reply_frame(1, b"CHAR-RECORD", bad);
            let _ = tokio::io::AsyncWriteExt::write_all(&mut s, &frame).await;
            tokio::time::sleep(Duration::from_secs(4)).await;
        });
        let ids = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ids_port = ids.local_addr().unwrap().port();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = ids.accept().await {
                let _ = tokio::io::AsyncWriteExt::write_all(
                    &mut s,
                    b"(100/loadbot1/2/0/1/127.0.0.1/3600)",
                )
                .await;
                tokio::time::sleep(Duration::from_secs(6)).await;
            }
        });
        let port = free_port();
        let child = Child::spawn(port, 12288, ids_port, db_port, "dbreply");
        let mut gate = connect_with_retry(port).await;
        gate.write_all(&frame(grobal2::GM_OPEN, 701, 99, 0, b"127.0.0.1"))
            .await
            .unwrap();
        let (m, _) = recv_frame(&mut gate).await.unwrap();
        assert_eq!(m.session_index, 1);
        tokio::time::sleep(Duration::from_millis(400)).await;
        gate.write_all(&frame(
            grobal2::GM_DATA,
            701,
            99,
            1,
            &hex(ORACLE_LOGIN_BODY),
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(900)).await;
        let log = child.stdout();
        if bad {
            assert!(
                log.contains("DB 应答被拒") && !log.contains("执行任务Id:1成功"),
                "坏签名必须被拒且不入表；日志实得：{log}"
            );
        } else {
            assert!(
                log.contains("执行任务Id:1成功"),
                "好签名必须被接受并入表；日志实得：{log}"
            );
            assert!(
                log.contains("收到角色数据 queryId=1 Ident=1100 Recog=1"),
                "sim 必须取到该 query 的应答并解出 Ident/Recog；日志实得：{log}"
            );
        }
        child.stop().await;
    }
}

/// **判据（S2b-3 骨架：进世界）**：假 DBSvr 回的载荷是一份**按实测布局合成**的角色记录头部
/// （`LoadPlayerDataPacket` → `CharacterDataInfo` → `RecordHeader` → `CharacterData`，到 `Dir` 为止即可）——
/// ⇒ 必须 parse 出 `ChrName=lb1a / CurMap=0 / (100,200) / Dir=3` 并**进世界**（`open_session` + `enter_map`），
/// 且该槽被标 `in_world`（于是后面的命令帧按**命令**处理，而不是走 F3 那条"已认证未进世界 ⇒ 空转丢帧"）。
///
/// 布局依据：`bins/game-svr/tests/fixtures/player_data_oracle.txt`（部署件序列化样例的头部窗口）。
#[tokio::test]
async fn db_reply_with_player_data_enters_world_and_unlocks_commands() {
    const ORACLE_LOGIN_BODY: &str = "23305e5e3c716f6d446f6e6f5478615f3c786e61497b5f623f5f6162586b6062586b61625a675f615a5f60605a6b60615a6a62603f6b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b5f603f21";

    // 角色记录头部（到 Dir 为止）：前缀 2 + ChrName + 前缀 2 + 前缀 7 + 7 个 Header 字段 + 前缀 52 +
    // ServerIndex + ChrName + CurMap + CurX(i16) + CurY(i16) + Dir(u8)
    fn player_data_head() -> Vec<u8> {
        fn s(v: &str) -> Vec<u8> {
            let mut o = i32::try_from(v.chars().count())
                .unwrap()
                .to_le_bytes()
                .to_vec();
            for c in v.chars() {
                let mut b = [0u16; 2];
                for u in c.encode_utf16(&mut b) {
                    o.extend_from_slice(&u.to_le_bytes());
                }
            }
            o
        }
        let mut o = vec![2u8];
        o.extend_from_slice(&s("lb1a")); // LoadPlayerDataPacket.ChrName
        o.push(2); // CharacterDataInfo 成员数
        o.push(7); // RecordHeader 成员数
        o.extend_from_slice(&s("loadbot1")); // sAccount
        o.extend_from_slice(&s("lb1a")); // Name
        o.extend_from_slice(&5i32.to_le_bytes()); // SelectID
        o.extend_from_slice(&1.5f64.to_le_bytes()); // dCreateDate
        o.push(0); // Deleted
        o.extend_from_slice(&2.5f64.to_le_bytes()); // UpdateDate
        o.extend_from_slice(&3.5f64.to_le_bytes()); // CreateDate
        o.push(52); // CharacterData 成员数（0x34）
        o.push(1); // ServerIndex
        o.extend_from_slice(&s("lb1a")); // Data.ChrName
        o.extend_from_slice(&s("0")); // CurMap
                                      // 入口坐标必须落在占位地图内（本用例的 `WorldMaps=64x64`；真地图加载后无此约束）
        o.extend_from_slice(&10i16.to_le_bytes()); // CurX
        o.extend_from_slice(&20i16.to_le_bytes()); // CurY
        o.push(3); // Dir
        o
    }

    fn reply_frame(query_id: i32, packet: &[u8]) -> Vec<u8> {
        use mir2_protocol::edcode;
        let mut msg = vec![5u8];
        msg.extend_from_slice(&1i32.to_le_bytes()); // Recog = 1
        msg.extend_from_slice(&1100u16.to_le_bytes()); // DBR_LOADHUMANRCD（Messages 里是 1100，不是 101）
        msg.extend_from_slice(&[0u8; 6]);
        let m = edcode::encode(&msg);
        let p = edcode::encode(packet);
        let n_len = m.len() + p.len() + 6;
        let sign_id = (u32::from(u16::try_from(n_len).unwrap()) << 16)
            | u32::from(u16::try_from(query_id ^ 170).unwrap());
        let sgn = edcode::encode(&sign_id.to_le_bytes());
        let mut body = vec![4u8];
        body.extend_from_slice(&query_id.to_le_bytes());
        for part in [&m, &p, &sgn] {
            body.extend_from_slice(&i32::try_from(part.len()).unwrap().to_le_bytes());
            body.extend_from_slice(part);
        }
        let mut frame = 0xAA9A_AA9Au32.to_le_bytes().to_vec();
        frame.extend_from_slice(&u16::try_from(body.len()).unwrap().to_le_bytes());
        frame.extend_from_slice(&body);
        frame
    }

    let dbsrv = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let db_port = dbsrv.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut s, _) = dbsrv.accept().await.unwrap();
        let mut buf = vec![0u8; 512];
        let _ = tokio::io::AsyncReadExt::read(&mut s, &mut buf).await;
        let frame = reply_frame(1, &player_data_head());
        let _ = tokio::io::AsyncWriteExt::write_all(&mut s, &frame).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let ids = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ids_port = ids.local_addr().unwrap().port();
    tokio::spawn(async move {
        if let Ok((mut s, _)) = ids.accept().await {
            let _ =
                tokio::io::AsyncWriteExt::write_all(&mut s, b"(100/loadbot1/2/0/1/127.0.0.1/3600)")
                    .await;
            tokio::time::sleep(Duration::from_secs(7)).await;
        }
    });
    let port = free_port();
    let child = Child::spawn(port, 12288, ids_port, db_port, "dbworld");
    let mut gate = connect_with_retry(port).await;
    gate.write_all(&frame(grobal2::GM_OPEN, 801, 111, 0, b"127.0.0.1"))
        .await
        .unwrap();
    let (m, _) = recv_frame(&mut gate).await.unwrap();
    assert_eq!(m.session_index, 1);
    tokio::time::sleep(Duration::from_millis(400)).await;
    gate.write_all(&frame(
        grobal2::GM_DATA,
        801,
        111,
        1,
        &hex(ORACLE_LOGIN_BODY),
    ))
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(900)).await;
    let log = child.stdout();
    assert!(
        log.contains("进世界 ChrName:lb1a CurMap:0 (10,20) Dir:3"),
        "必须按记录进世界；日志实得：{log}"
    );
    assert!(
        log.contains("进世界标记：socket=801"),
        "进世界后必须把该槽标 in_world；日志实得：{log}"
    );
    // 行为判据：进世界后一条 12B 命令帧必须按**命令**处理（而不是 F3 的"已认证未进世界 ⇒ 空转丢帧"）
    let mut cmd = vec![0u8; 12];
    cmd[4..6].copy_from_slice(&3030u16.to_le_bytes());
    gate.write_all(&frame(grobal2::GM_DATA, 801, 111, 1, &cmd))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let log = child.stdout();
    assert!(
        log.contains("命令 Recog=0 Ident=3030"),
        "进世界后命令帧必须被当命令处理；日志实得：{log}"
    );
    assert!(
        !log.contains("已认证未进世界"),
        "进世界后不得再走空转丢帧那条；日志实得：{log}"
    );
    child.stop().await;
}
