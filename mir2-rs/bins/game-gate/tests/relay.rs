// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

//! GameGate 的**进程级中继自测**：假 GameSvr + 假客户端，不依赖 MySQL 与整栈。
//!
//! 判据（对齐协议梳理 §7.5 的实机取证，不是自洽往返）：
//! 1. 客户端一连上，网关发 `GM_OPEN`，体 = 客户端 IP 字符串（`PackLength` 为其长度）、
//!    `Socket` = 客户端 handle、`SessionIndex` = 0，且 `SessionId ∈ 0..=9999`（随机）；
//! 2. 登录通过 ⇒ 网关**先补发一条合成登录包**（`#0…!`，`ClientSession.cs:1199-1224`）；
//! 3. 客户端发 ≥16 字节 body 的命令帧 ⇒ 网关以 `GM_DATA` 转发（**重建后**的 body），`SessionId` 与 GM_OPEN 一致；
//! 4. 20B 头 + 1 字节 body（心跳）**不得**被转发——登录前后各一条（两条 oracle 路径不同，见 `handle_frame`）；
//! 5. body < 16 字节（登录后）⇒ 网关关会话（"异常消息封包"）并补发 `GM_CLOSE`（`session_id=0, socket=handle`）；
//! 6. 缺配置时进程**非零退出**（M1 验收④）。

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

/// 合成登录包（`ClientSession.cs:1207-1209`）以 `#0` 开头；命令路径重建的 body 以**解码态 12B** 开头。
/// 断言"只数命令帧"时要把它排除掉。
fn is_login_packet(body: &[u8]) -> bool {
    body.starts_with(b"#0")
}

/// 读一帧（20B 头 + body）；连接断开返回 `None`。
async fn read_frame(sock: &mut TcpStream) -> Option<(ServerMessage, Vec<u8>)> {
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

/// 一直读到满足 `pred` 的帧（断开返回 `None`）。
async fn read_until(
    sock: &mut TcpStream,
    pred: impl Fn(&ServerMessage, &[u8]) -> bool,
) -> Option<(ServerMessage, Vec<u8>)> {
    while let Some((m, b)) = read_frame(sock).await {
        if pred(&m, &b) {
            return Some((m, b));
        }
    }
    None
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

/// 造一个客户端 face 的**登录帧**：body = `#1` + EDCode(GBK(文本)) + `!`（照 `ClientSession.cs:166/673`）。
/// 用例里的文本全 ASCII ⇒ GBK 与字节串一致，不必再过编码器。
fn login_frame(text: &str) -> Vec<u8> {
    let mut body = b"#1".to_vec();
    body.extend_from_slice(&mir2_protocol::edcode::encode(text.as_bytes()));
    body.push(b'!');
    frame(0, 0, &body)
}

/// 现网形态（账号 8 / 角色名 4 / 版本号 9 / code 10 位）的登录文本。
const GOOD_LOGIN: &str = "**loadbot1\\lb1a\\1\\120020522\\1200205220\\AABB";

/// 造一个 `CM_SAY` 命令帧（解码态 = 12B 固定头，`Ident = CM_SAY` = 3030，后面跟聊天内容）。
fn say_frame(content: &str) -> Vec<u8> {
    let mut decoded = vec![0u8; 12];
    decoded[4..6].copy_from_slice(&mir2_protocol::messages::CM_SAY.to_le_bytes());
    decoded.extend_from_slice(content.as_bytes());
    command_frame(&decoded)
}

/// 判据 9：`CM_SAY` 的间隔门（`ClientSession.cs:485-497`，现网 `ChatInterval=800`）——
/// 800ms 内的第二条聊天**必须被丢**（不转发）；`@` 开头的内容不判间隔。
#[tokio::test]
async fn say_interval_drops_the_second_chat_frame() {
    let (srv_port, srv_task) = fake_game_svr().await;
    let gate_port = free_port();
    let dir = std::env::temp_dir().join(format!("gg-say-{}-{}", std::process::id(), gate_port));
    write_conf(&dir, srv_port, gate_port);
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-gate"))
        .arg(dir.join("config.conf"))
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut client = connect_with_retry(gate_port).await;
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();
    // 紧挨着两条聊天：第二条必须被丢
    client.write_all(&say_frame("hello")).await.unwrap();
    client.flush().await.unwrap();
    client.write_all(&say_frame("hello again")).await.unwrap();
    client.flush().await.unwrap();
    // `@` 开头的内容不判间隔 ⇒ 这条必须通过
    client.write_all(&say_frame("@move 1 2")).await.unwrap();
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    child.kill().await.ok();
    drop(client);

    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未按时结束")
        .unwrap();
    let datas: Vec<&(ServerMessage, Vec<u8>)> = got
        .iter()
        .filter(|(m, b)| m.ident == u16::from(grobal2::GM_DATA) && !is_login_packet(b))
        .collect();
    assert_eq!(
        datas.len(),
        2,
        "800ms 内的第二条普通聊天必须被丢；`@` 开头的第三条必须通过（实得 {} 条命令帧；\
         合成登录包不计入）",
        datas.len()
    );
    // 两条能到 5000 的body 必须**不同**（分别是 hello 与 @move 那条）。
    // 注意：body 里**不是明文**——按 §7.9 的出帧构造，尾部抄的是**输入的编码态**字节，
    // 所以这里不做 `ends_with(b"hello")` 这类明文断言（那属于 `command_frame_with_tail_is_rebuilt`
    // 的职责：它按 `payload[16..]` 的编码态比对）。
    assert_ne!(
        datas[0].1, datas[1].1,
        "两条通过的聊天帧 body 必须不同（hello vs @move）"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 造一个客户端 face 的**命令帧**：body = `#1` + EDCode(解码态载荷) + `!`。
/// 登录通过后网关按 `ClientSession.cs:646-669` 重建出帧，所以测试要按这个形态发。
fn command_frame(decoded: &[u8]) -> Vec<u8> {
    let mut body = b"#1".to_vec();
    body.extend_from_slice(&mir2_protocol::edcode::encode(decoded));
    body.push(b'!');
    frame(11, 0, &body)
}

/// 判据 6：登录校验失败 ⇒ 剔除（客户端被断开），且 5000 侧除 `GM_OPEN`/`GM_CLOSE` 外没有别的帧。
#[tokio::test]
async fn bad_login_is_kicked_and_nothing_is_forwarded() {
    let (srv_port, srv_task) = fake_game_svr().await;
    let gate_port = free_port();
    let dir = std::env::temp_dir().join(format!("gg-kick-{}-{}", std::process::id(), gate_port));
    write_conf(&dir, srv_port, gate_port);
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-gate"))
        .arg(dir.join("config.conf"))
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut client = connect_with_retry(gate_port).await;

    // 前两位不是 `**` ⇒ 照 oracle 的 Kicked 1（本次实机取证里真网关就是这条）
    client
        .write_all(&login_frame(
            "XXloadbot1\\lb1a\\1\\120020522\\1200205220\\AABB",
        ))
        .await
        .unwrap();
    // 紧接着发一帧"数据"：登录没过，它**不该**被转发
    client.write_all(&frame(11, 0, &[0u8; 32])).await.unwrap();
    client.flush().await.unwrap();

    // 网关应主动断开
    let mut buf = [0u8; 1];
    let n = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf))
        .await
        .expect("5s 内网关应剔除该连接")
        .expect("读应成功（EOF=0 也算）");
    assert_eq!(n, 0, "网关必须剔除（读到 EOF）");
    child.kill().await.ok();
    drop(client);

    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未结束")
        .unwrap();
    let idents: Vec<u16> = got.iter().map(|(m, _)| m.ident).collect();
    assert_eq!(
        idents,
        vec![u16::from(grobal2::GM_OPEN), u16::from(grobal2::GM_CLOSE)],
        "登录失败时除 GM_OPEN/GM_CLOSE 外不得有别的帧（尤其不能有 GM_DATA）"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 判据：合成登录包**在 `GM_SERVERUSERINDEX` 之前**发出（`SessionIndex` 还是 0），
/// 而服务端下发索引后的命令帧必须带上它。
///
/// 依据：`ClientThread.cs:272-275`（把 `SessionIndex` 记到会话的 `SvrListIdx`）+
/// `ClientSession.cs:649`（`SendMsg.SessionIndex = SvrListIdx`）+
/// `ClientSession.cs:1217`（登录包的 `SessionIndex` 用会话当时的 `SvrListIdx`）。
///
/// 时序是**确定**的：假 GameSvr 只在**收到合成登录包之后**才下发索引，所以登录包必然带 0、
/// 之后的命令帧必然带 42——不依赖睡眠竞速。
#[tokio::test]
async fn server_user_index_is_echoed_on_gm_data() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let srv_port = l.local_addr().unwrap().port();
    let srv_task = tokio::spawn(async move {
        let (mut sock, _) = l.accept().await.unwrap();
        // 先收 GM_OPEN，拿到网关生成的 SessionId
        let open = read_frame(&mut sock).await.expect("GM_OPEN");
        assert_eq!(open.0.ident, u16::from(grobal2::GM_OPEN));
        let sid = open.0.session_id;
        // 等合成登录包（登录通过才会来），记下它的 SessionIndex，然后才下发 SvrListIdx=42
        let login = read_until(&mut sock, |m, b| {
            m.ident == u16::from(grobal2::GM_DATA) && is_login_packet(b)
        })
        .await
        .expect("合成登录包");
        assert_eq!(login.0.session_index, 0, "索引下发前登录包必须是 0");
        let idx = ServerMessage {
            packet_code: grobal2::PacketCode,
            socket: 0,
            session_id: sid,
            ident: u16::from(grobal2::GM_SERVERUSERINDEX),
            session_index: 42,
            pack_length: 0,
        };
        sock.write_all(&idx.to_bytes()).await.unwrap();
        sock.flush().await.unwrap();
        // 再收命令帧，看它带没带 42
        let cmd = read_until(&mut sock, |m, b| {
            m.ident == u16::from(grobal2::GM_DATA) && !is_login_packet(b)
        })
        .await
        .expect("命令帧");
        Some(cmd.0.session_index)
    });

    let gate_port = free_port();
    let dir = std::env::temp_dir().join(format!("gg-idx-{}-{}", std::process::id(), gate_port));
    write_conf(&dir, srv_port, gate_port);
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-gate"))
        .arg(dir.join("config.conf"))
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut client = connect_with_retry(gate_port).await;
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await; // 等服务端下发索引
    client.write_all(&frame(11, 0, &[7u8; 32])).await.unwrap();
    client.flush().await.unwrap();

    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未按时结束")
        .unwrap();
    assert_eq!(got, Some(42), "GM_DATA 必须带上服务端下发的 SessionIndex");
    child.kill().await.ok();
    std::fs::remove_dir_all(&dir).ok();
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

    // 判据 4（登录前）：1 字节 body——oracle 走 `:671-680` 的 else 分支、`destinationSpan[2..0]`
    // 越界抛异常被 `ServerManager.cs:238` 吞掉 ⇒ 丢帧、会话存活。**不得**被转发。
    client.write_all(&frame(11, 0, b"*")).await.unwrap();
    // 判据 2：先过登录（登录帧本身不得被转发，但要补发一条合成登录包）
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();
    // 判据 4（登录后）：心跳走 `:157-160` 的 `return`，同样不得转发
    client.write_all(&frame(11, 0, b"*")).await.unwrap();
    // 判据 3：登录后的命令帧必须按 §7.9 重建后转发。
    // 无尾部命令帧（解码后恰好 12B）：出帧的 body 必须是**解码态**的 12 字节、PackLength=12
    let decoded: Vec<u8> = (0..12u8).collect();
    client.write_all(&command_frame(&decoded)).await.unwrap();
    client.flush().await.unwrap();

    // 判据 5：body < 16 ⇒ 网关关会话（并补发 GM_CLOSE）
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

    // 判据 2：登录通过时必须补发**恰好一条**合成登录包（reviewer-1 的 Important #1）
    let logins: Vec<&(ServerMessage, Vec<u8>)> = got
        .iter()
        .filter(|(m, b)| m.ident == u16::from(grobal2::GM_DATA) && is_login_packet(b))
        .collect();
    assert_eq!(
        logins.len(),
        1,
        "登录通过必须补发一条合成登录包（实得 {} 条）",
        logins.len()
    );
    assert_eq!(
        logins[0].0.socket, open.socket,
        "合成登录包的 Socket 必须是客户端 handle（ClientSession.cs:1215）"
    );
    assert_eq!(logins[0].0.session_id, sid);
    assert_eq!(
        usize::try_from(logins[0].0.pack_length).unwrap(),
        logins[0].1.len(),
        "PackLength = encodeLen + 3 = body 长度（:1218）"
    );

    // 判据 3：恰好一条**命令** GM_DATA（两条心跳都没被转发），body = 解码态 12B，SessionId 一致
    let datas: Vec<&(ServerMessage, Vec<u8>)> = got
        .iter()
        .filter(|(m, b)| m.ident == u16::from(grobal2::GM_DATA) && !is_login_packet(b))
        .collect();
    assert_eq!(
        datas.len(),
        1,
        "心跳不得被转发：期望恰好 1 条命令 GM_DATA，实得 {}",
        datas.len()
    );
    assert_eq!(
        datas[0].1, decoded,
        "无尾部命令帧：GM_DATA 的 body 必须是**解码态**的 12 字节（oracle 抄 decodeBuff，不是原样字节）"
    );
    assert_eq!(
        datas[0].0.pack_length, 12,
        "无尾部时 PackLength = CommandMessage.Size = 12"
    );
    assert_eq!(datas[0].0.session_id, sid, "GM_DATA 必须带同一 SessionId");

    // 判据 5：有 GM_CLOSE 且 socket=handle、session_id=0（实机取证）
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

/// 判据 8：**有尾部**的命令帧按 `ClientSession.cs:646-669` 重建——
/// `PackLength = 12 + tLen + 1`（`tLen` = 尾部重编码后的长度），body = 解码态 12B + 输入尾部（编码态）
/// + 1 字节（oracle 那句是池内存未初始化字节，我们送 0，见协议梳理 §7.9）。
#[tokio::test]
async fn command_frame_with_tail_is_rebuilt() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let srv_port = l.local_addr().unwrap().port();
    // 解码态载荷：12B 固定头 + 8B 尾部
    let decoded: Vec<u8> = (0..20u8).collect();
    let expect_tlen = mir2_protocol::edcode::encode(&decoded[12..]).len();
    let srv_task = tokio::spawn(async move {
        let (mut sock, _) = l.accept().await.unwrap();
        loop {
            let mut head = [0u8; ServerMessage::SIZE];
            if sock.read_exact(&mut head).await.is_err() {
                return None;
            }
            let m = ServerMessage::from_bytes(&head).unwrap();
            let mut body = vec![0u8; m.body_len()];
            if !body.is_empty() && sock.read_exact(&mut body).await.is_err() {
                return None;
            }
            if m.ident == u16::from(grobal2::GM_DATA) && !is_login_packet(&body) {
                return Some((m.pack_length, body));
            }
        }
    });
    let gate_port = free_port();
    let dir = std::env::temp_dir().join(format!("gg-tail-{}-{}", std::process::id(), gate_port));
    write_conf(&dir, srv_port, gate_port);
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-gate"))
        .arg(dir.join("config.conf"))
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut client = connect_with_retry(gate_port).await;
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();
    client.write_all(&command_frame(&decoded)).await.unwrap();
    client.flush().await.unwrap();

    let (pack_length, body) = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未按时结束")
        .unwrap()
        .expect("应收到 GM_DATA");
    assert_eq!(
        usize::try_from(pack_length).unwrap(),
        12 + expect_tlen + 1,
        "PackLength = 12 + tLen + 1"
    );
    assert_eq!(
        &body[..12],
        &decoded[..12],
        "固定头必须是**解码态**的 12 字节"
    );
    // 尾部抄的是**输入的编码态**字节：本用例的输入 body = `#1` + encode(decoded) + `!`，
    // 网关视角的 payload = encode(decoded) ⇒ 尾部 = payload[16..]（oracle 抄的是 tempBuff[16..]）。
    let enc = mir2_protocol::edcode::encode(&decoded);
    let sent_tail = &body[12..body.len() - 1];
    assert_eq!(
        sent_tail,
        &enc[16..],
        "尾部必须逐字节等于输入的编码态尾部（oracle 抄 tempBuff[16..]）"
    );
    assert_eq!(
        body[body.len() - 1],
        0,
        "oracle 的声明长度比实际内容多 1 字节（池内存未初始化）；Rust 侧固定送 0 —— 已登记差异"
    );
    child.kill().await.ok();
    std::fs::remove_dir_all(&dir).ok();
}

/// 判据 2（reviewer-1 的 Important #1）：登录通过后**必须**按 `ClientSession.cs:1203-1218` 补发
/// 合成登录包——文本 `**account/charName/cert/clientVer/code/{32 个 0}/{ServiceId}`、`#0…!` 外形、
/// `PackLength = encodeLen + 3`。缺了这条，GameSvr 永远等不到该会话的登录。
///
/// 这里**独立**按 oracle 公式重算期望字节（不复用被测函数），并把 body 解回明文复核文本。
#[tokio::test]
async fn login_success_sends_synthetic_login_packet() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let srv_port = l.local_addr().unwrap().port();
    let srv_task = tokio::spawn(async move {
        let (mut sock, _) = l.accept().await.unwrap();
        let open = read_frame(&mut sock).await.expect("GM_OPEN");
        let login = read_until(&mut sock, |m, b| {
            m.ident == u16::from(grobal2::GM_DATA) && is_login_packet(b)
        })
        .await
        .expect("合成登录包");
        (open.0, login.0, login.1)
    });

    let gate_port = free_port();
    let dir = std::env::temp_dir().join(format!("gg-login-{}-{}", std::process::id(), gate_port));
    write_conf(&dir, srv_port, gate_port);
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-gate"))
        .arg(dir.join("config.conf"))
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut client = connect_with_retry(gate_port).await;
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();

    let (open, msg, body) = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未按时结束")
        .unwrap();

    // oracle 公式：`**loadbot1/lb1a/1/120020522/1200205220/{32 个 0}/0`
    //（ServiceId = 第 1 组端点 = 0；hardware 因现网 ProcClientCount=0 全 0）
    let text = "**loadbot1/lb1a/1/120020522/1200205220/\
                000000000000000000000000000000/0";
    let mut expect = b"#0".to_vec();
    expect.extend_from_slice(&mir2_protocol::edcode::encode(text.as_bytes()));
    expect.push(b'!');
    assert_eq!(body, expect, "合成登录包必须逐字节等于 oracle 公式");
    assert_eq!(
        usize::try_from(msg.pack_length).unwrap(),
        body.len(),
        "PackLength = encodeLen + 3"
    );
    assert_eq!(msg.session_id, open.session_id, "必须带同一 SessionId");
    assert_eq!(
        msg.socket, open.socket,
        "Socket 必须是客户端 handle（:1215）"
    );
    assert_eq!(
        msg.session_index, 0,
        "登录阶段 SessionIndex 还是 0（:1217）"
    );
    // 解回明文复核（不是用同一个 format! 自证）
    let back = String::from_utf8(mir2_protocol::edcode::decode(&body[2..body.len() - 1])).unwrap();
    assert_eq!(back, text);
    child.kill().await.ok();
    std::fs::remove_dir_all(&dir).ok();
}
