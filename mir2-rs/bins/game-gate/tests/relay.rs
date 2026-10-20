// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

//! GameGate 的**进程级中继自测**：假 GameSvr + 假客户端，不依赖 MySQL 与整栈。
//!
//! 判据（对齐协议梳理 §7.5 的实机取证，不是自洽往返）：
//! 0. **客户端 face 没有定长头**：客户端发给网关的就是裸 `#…!` 帧（一次 write = 一帧）；
//!    加 20B 头会被当成帧内垃圾前缀 ⇒ 照 oracle 剔除（§7.12 的实测 + `header_prefixed_login_is_kicked`）；
//! 1. 客户端一连上，网关发 `GM_OPEN`，体 = 客户端 IP 字符串（`PackLength` 为其长度）、
//!    `Socket` = 客户端 handle、`SessionIndex` = 0，且 `SessionId ∈ 0..=9999`（随机）；
//! 2. 登录通过 ⇒ 网关**先补发一条合成登录包**（`#0…!`，`ClientSession.cs:1199-1224`）；
//! 3. 客户端发 ≥16 字节 body 的命令帧 ⇒ 网关以 `GM_DATA` 转发（**重建后**的 body），`SessionId` 与 GM_OPEN 一致；
//! 4. 20B 头 + 1 字节 body（心跳）**不得**被转发——登录前后各一条（两条 oracle 路径不同，见 `handle_frame`）；
//! 5. body < 16 字节（登录后）⇒ 网关关会话（"异常消息封包"）并补发 `GM_CLOSE`（`session_id=0, socket=handle`）；
//! 6. 缺配置时进程**非零退出**（M1 验收④）。

use std::time::{Duration, Instant};

use mir2_protocol::frame::ServerMessage;
use mir2_protocol::messages::grobal2;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[path = "../../game-svr/tests/support/mod.rs"]
#[allow(dead_code)] // 共用模块：另一个 target 的条目在本 target 未用
mod support;

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

/// 十六进制串 → 字节（用于把 **oracle 实测字节**原样钉进用例）。
fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
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
            // 真客户端是一帧一次 send（我们这边也这样）；关掉 Nagle，免得**测试自己**把多帧合并成一次读
            // —— 那会让网关（照 oracle"一次 read = 一帧"）把两帧当一帧，用例就会假红。
            c.set_nodelay(true).ok();
            return c;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("网关未在预期时间内监听 {port}");
}

/// 双向假 GameSvr：`frames` 收网关→服务端的帧，`tx` 可下发服务端→网关的帧。
///
/// 与 `fake_game_svr` 的区别：后者只收不发、且要等网关断开才返回；B-4/B-5 需要在**同一条活连接**
/// 上收发抖动（GM_STOP/GM_RECEIVE_OK/GM_DATA），所以这里用两个 channel 解耦读写。
struct FakeSvr {
    port: u16,
    frames: mpsc::UnboundedReceiver<(ServerMessage, Vec<u8>)>,
    tx: mpsc::UnboundedSender<(ServerMessage, Vec<u8>)>,
    task: tokio::task::JoinHandle<()>,
}

impl FakeSvr {
    async fn start() -> FakeSvr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<(ServerMessage, Vec<u8>)>();
        let task = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let (mut rd, mut wr) = sock.into_split();
            let reader = tokio::spawn(async move {
                loop {
                    let mut head = [0u8; ServerMessage::SIZE];
                    if rd.read_exact(&mut head).await.is_err() {
                        break;
                    }
                    let Some(msg) = ServerMessage::from_bytes(&head) else {
                        break;
                    };
                    let mut body = vec![0u8; msg.body_len()];
                    if !body.is_empty() && rd.read_exact(&mut body).await.is_err() {
                        break;
                    }
                    if frame_tx.send((msg, body)).is_err() {
                        break;
                    }
                }
            });
            while let Some((msg, body)) = out_rx.recv().await {
                if wr.write_all(&msg.to_bytes()).await.is_err() {
                    break;
                }
                if !body.is_empty() && wr.write_all(&body).await.is_err() {
                    break;
                }
                let _ = wr.flush().await;
            }
            reader.abort();
        });
        FakeSvr {
            port,
            frames: frame_rx,
            tx: out_tx,
            task,
        }
    }

    /// 下发一帧仅 20B 头的管理帧（GM_STOP/GM_RECEIVE_OK/GM_CHECKSERVER）。
    fn send(&self, msg: ServerMessage) {
        self.tx.send((msg, Vec::new())).unwrap();
    }

    /// 下发一帧 `GM_DATA`（头 + body）。
    fn send_data(&self, msg: ServerMessage, body: &[u8]) {
        self.tx.send((msg, body.to_vec())).unwrap();
    }

    /// 等一帧满足 `pred` 的网关→服务端帧（超时 5s 返回 `None`）。
    async fn recv_until(
        &mut self,
        pred: impl Fn(&ServerMessage) -> bool,
    ) -> Option<(ServerMessage, Vec<u8>)> {
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some((msg, body)) = self.frames.recv().await {
                if pred(&msg) {
                    return Some((msg, body));
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
    }

    /// 等指定 ident 的一帧（`grobal2::GM_*`）；超时 5s 返回 `None`。
    async fn recv_ident(&mut self, ident: u8) -> Option<(ServerMessage, Vec<u8>)> {
        let ident = u16::from(ident);
        self.recv_until(|m| m.ident == ident).await
    }

    /// 收 `dur` 内到达的全部帧（用于断言"窗口内没有某类帧"）。
    async fn recv_for(&mut self, dur: Duration) -> Vec<(ServerMessage, Vec<u8>)> {
        let mut out = Vec::new();
        let deadline = tokio::time::Instant::now() + dur;
        while let Ok(Some(frame)) = tokio::time::timeout_at(deadline, self.frames.recv()).await {
            out.push(frame);
        }
        out
    }
}

impl Drop for FakeSvr {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// 从客户端 face 读一次（一个 GM_DATA 会以一次 `#…!` write 到达）。
async fn read_some(sock: &mut TcpStream, dur: Duration) -> Option<Vec<u8>> {
    let mut buf = [0u8; 512];
    let n = tokio::time::timeout(dur, sock.read(&mut buf))
        .await
        .ok()?
        .ok()?;
    if n == 0 {
        None
    } else {
        Some(buf[..n].to_vec())
    }
}

/// 断言 socket 仍被对端保持（读超时 = 没数据也没 EOF）；被关闭/有下行都算失败。
async fn assert_still_open(sock: &mut TcpStream, dur: Duration) {
    let mut buf = [0u8; 64];
    match tokio::time::timeout(dur, sock.read(&mut buf)).await {
        Err(_) => {}
        Ok(Ok(0)) => panic!("socket 被对端关闭（EOF）；oracle 在 GM_STOP 后不关新连接"),
        Ok(Ok(n)) => panic!("GM_STOP 后新连接不应有下行，实得 {n} 字节"),
        Ok(Err(e)) => panic!("socket 读错误: {e}"),
    }
}

/// 造一帧 `GM_DATA` 头（body 长度由调用方给）。
fn gm_data(session_id: u16, body_len: i32) -> ServerMessage {
    ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: 0,
        session_id,
        ident: u16::from(grobal2::GM_DATA),
        session_index: 0,
        pack_length: body_len,
    }
}

/// 起一个 `game-gate` 子进程并**等它自己 bind 成功**（就绪门）。
///
/// 卡 `openmir2-m2-flaky-relay-testports`：`free_port()` 的「探测即释放」会让并行用例的
/// `bind(:0)` 抢走端口，网关 bind 失败、客户端却连上别人的监听（owner 实测 `os error 10048`
/// 与假红）。这里端口一律由 `support::next_port()`（进程内唯一、非临时段）给出，且只有看到
/// 子进程在 `TcpListener::bind` 成功之后打印的 `游戏网关[…:{port}]已启动…`（`main.rs:134`）
/// 才返回；没等到就 kill + fail-closed（绝不「先 connect 再赌」）。
struct Gate {
    proc: tokio::process::Child,
    dir: std::path::PathBuf,
    port: u16,
}

impl Gate {
    async fn start(tag: &str, srv_port: u16, gate_port: u16) -> Gate {
        Self::start_with_env(tag, srv_port, gate_port, &[]).await
    }

    /// 同 `start`，但给子进程注入环境变量（B-4/B-5 用小容量/短心跳做进程级判定）。
    async fn start_with_env(
        tag: &str,
        srv_port: u16,
        gate_port: u16,
        envs: &[(&str, &str)],
    ) -> Gate {
        let dir = support::unique_temp_dir(tag);
        write_conf(&dir, srv_port, gate_port);
        let log = dir.join("gate.out.log");
        let out = std::fs::File::create(&log).unwrap();
        let err = out.try_clone().unwrap();
        let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_game-gate"));
        cmd.arg(dir.join("config.conf"))
            .stdout(out)
            .stderr(err)
            .kill_on_drop(true);
        for (key, value) in envs {
            cmd.env(key, value);
        }
        let mut proc = cmd.spawn().unwrap();
        let text =
            support::wait_child_ready(&mut proc, &log, gate_port, Duration::from_secs(15)).await;
        if !text.contains(&format!(":{gate_port}]已启动")) {
            proc.kill().await.ok();
            let _ = std::fs::remove_dir_all(&dir);
        }
        support::require_ready(&text, gate_port);
        Gate {
            proc,
            dir,
            port: gate_port,
        }
    }

    async fn connect(&self) -> TcpStream {
        connect_with_retry(self.port).await
    }

    /// 收工：kill 子进程 + 清临时目录（失败路径也走这里）。
    async fn shutdown(mut self) {
        self.proc.kill().await.ok();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 等网关把上一帧处理完再发下一帧（只影响**测试**的节奏，不改被测算子的语义）。
async fn settle() {
    tokio::time::sleep(Duration::from_millis(60)).await;
}

/// 造一个客户端 face 的**登录帧**：`#1` + EDCode(GBK(文本)) + `!`（照 `ClientSession.cs:166/673`）。
/// **客户端 face 没有定长头**（见文件头判据 0）⇒ 返回值就是直接写到 socket 的字节。
/// 用例里的文本全 ASCII ⇒ GBK 与字节串一致，不必再过编码器。
fn login_frame(text: &str) -> Vec<u8> {
    let mut body = b"#1".to_vec();
    body.extend_from_slice(&mir2_protocol::edcode::encode(text.as_bytes()));
    body.push(b'!');
    body
}

/// 现网形态（账号 8 / 角色名 4 / 版本号 9 / code 10 位）的登录文本。
const GOOD_LOGIN: &str = "**loadbot1/lb1a/1/120020522/1200205220/AABB";

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
    let gate_port = support::next_port();
    let gate = Gate::start("gg-say", srv_port, gate_port).await;
    let mut client = gate.connect().await;
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();
    settle().await; // 等登录帧被处理完（一帧一次 send）
                    // 紧挨着两条聊天：第二条必须被丢
    client.write_all(&say_frame("hello")).await.unwrap();
    client.flush().await.unwrap();
    settle().await;
    client.write_all(&say_frame("hello again")).await.unwrap();
    client.flush().await.unwrap();
    settle().await;
    // `@` 开头的内容不判间隔 ⇒ 这条必须通过
    client.write_all(&say_frame("@move 1 2")).await.unwrap();
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    gate.shutdown().await;
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
}

/// 造一个客户端 face 的**命令帧**：`#1` + EDCode(解码态载荷) + `!`（同样**不加头**）。
/// 登录通过后网关按 `ClientSession.cs:646-669` 重建出帧，所以测试要按这个形态发。
fn command_frame(decoded: &[u8]) -> Vec<u8> {
    let mut body = b"#1".to_vec();
    body.extend_from_slice(&mir2_protocol::edcode::encode(decoded));
    body.push(b'!');
    body
}

/// 判据 6：登录校验失败 ⇒ 剔除（客户端被断开），且 5000 侧除 `GM_OPEN`/`GM_CLOSE` 外没有别的帧。
#[tokio::test]
async fn bad_login_is_kicked_and_nothing_is_forwarded() {
    let (srv_port, srv_task) = fake_game_svr().await;
    let gate_port = support::next_port();
    let gate = Gate::start("gg-kick", srv_port, gate_port).await;
    let mut client = gate.connect().await;

    // 前两位不是 `**` ⇒ 照 oracle 的 Kicked 1（本次实机取证里真网关就是这条）
    client
        .write_all(&login_frame(
            "XXloadbot1\\lb1a\\1\\120020522\\1200205220\\AABB",
        ))
        .await
        .unwrap();
    // 紧接着发一帧"数据"：登录没过，它**不该**被转发
    client.write_all(&[0u8; 32]).await.unwrap();
    client.flush().await.unwrap();

    // 网关应主动断开
    let mut buf = [0u8; 1];
    let n = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf))
        .await
        .expect("5s 内网关应剔除该连接")
        .expect("读应成功（EOF=0 也算）");
    assert_eq!(n, 0, "网关必须剔除（读到 EOF）");
    gate.shutdown().await;
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

    let gate_port = support::next_port();
    let gate = Gate::start("gg-idx", srv_port, gate_port).await;
    let mut client = gate.connect().await;
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await; // 等服务端下发索引
    client.write_all(&[7u8; 32]).await.unwrap();
    client.flush().await.unwrap();

    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未按时结束")
        .unwrap();
    assert_eq!(got, Some(42), "GM_DATA 必须带上服务端下发的 SessionIndex");
    gate.shutdown().await;
}

#[tokio::test]
async fn relays_open_data_heartbeat_and_close() {
    let (srv_port, srv_task) = fake_game_svr().await;
    let gate_port = support::next_port();
    let gate = Gate::start("gg-relay", srv_port, gate_port).await;
    let mut client = gate.connect().await;

    // 判据 4（登录前）：1 字节 body——oracle 走 `:671-680` 的 else 分支、`destinationSpan[2..0]`
    // 越界抛异常被 `ServerManager.cs:238` 吞掉 ⇒ 丢帧、会话存活。**不得**被转发。
    client.write_all(b"*").await.unwrap();
    settle().await;
    // 判据 2：先过登录（登录帧本身不得被转发，但要补发一条合成登录包）
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();
    settle().await; // 等登录处理完
                    // 判据 4（登录后）：心跳走 `:157-160` 的 `return`，同样不得转发
    client.write_all(b"*").await.unwrap();
    settle().await;
    // 判据 3：登录后的命令帧必须按 §7.9 重建后转发。
    // 无尾部命令帧（解码后恰好 12B）：出帧的 body 必须是**解码态**的 12 字节、PackLength=12
    let decoded: Vec<u8> = (0..12u8).collect();
    client.write_all(&command_frame(&decoded)).await.unwrap();
    client.flush().await.unwrap();
    settle().await;

    // 判据 5：body < 16 ⇒ 网关关会话（并补发 GM_CLOSE）
    client.write_all(b"\x01\x02\x03").await.unwrap();
    client.flush().await.unwrap();

    // 等网关看到客户端断开
    let mut buf = [0u8; 1];
    let _ = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf)).await;
    gate.shutdown().await;
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
    // S3 路由：命令帧的 Socket 必须是**客户端 handle**（= GM_OPEN 的 socketId），照
    // `ClientSession.cs:65-70` 的 `SendMsg.Socket = _session.SckHandle`；送 0 会让 GameSvr 的
    // `FindUser`（`ChannelMessageHandler.cs:170-198`）落进「用户不存在」、客户端命令全被丢。
    assert_eq!(
        datas[0].0.socket, open.socket,
        "命令 GM_DATA 的 Socket 必须与 GM_OPEN 的 handle 一致（S3 路由）"
    );

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
    let gate_port = support::next_port();
    let gate = Gate::start("gg-tail", srv_port, gate_port).await;
    let mut client = gate.connect().await;
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();
    settle().await; // 等网关处理完登录（一帧一次 send）
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
    gate.shutdown().await;
}

/// 判据 2（reviewer-1 的 Important #1）：登录通过后**必须**按 `ClientSession.cs:1203-1218` 补发
/// 合成登录包——文本 `**account/charName/cert/clientVer/code/{30 个 0}/{ServiceId}`、`#0…!` 外形、
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

    let gate_port = support::next_port();
    let gate = Gate::start("gg-login", srv_port, gate_port).await;
    let mut client = gate.connect().await;
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();

    let (open, msg, body) = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未按时结束")
        .unwrap();

    // oracle 公式：`**loadbot1/lb1a/1/120020522/1200205220/{30 个 0}/0`
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
    gate.shutdown().await;
}

/// **判据 0 + 判据 2 的 oracle 字节版**：把 M0 金标准里**真 Delphi 客户端**发的 7200 第一帧原样喂进去，
/// 网关吐给 GameSvr 的那条合成登录包必须**逐字节等于真 C# 网关吐出来的那条**。
///
/// 这不是"按公式自证"：两份字节都是**实测**产物——
/// - 输入 = `tests/golden/session-20261010-c3-hook/capture.jsonl` seq 13（真客户端帧，111B）；
/// - 期望 = 把同一帧喂给真 C# GameGate（副本跑 5010/7210，不占现网端口）后 5000 侧抓到的 `GM_DATA`（98B）。
///
/// 证据与夹具：`E:\tmp\openmir2-gamegate-imp\oracle-login-cap\`（harness.py / out-csharp-raw.json / FINDINGS.md）。
#[tokio::test]
async fn oracle_golden_login_frame_yields_oracle_synthetic_packet() {
    /// 真客户端登录帧（111 字节，`#1` + EDCode + `!`）。
    const GOLDEN_LOGIN_FRAME: &str = "23315e5e3c716f6d446f6e6f5478615f3c786e61497b5f623f5f6162586b6062586b61625a675f615a5f60605a6b60615a6a62603f6b6062667663606b756277667761625662626151666161516b7660647977605969757651666361697674615667607656627563516677615b6a21";
    /// 真 C# 网关在同一输入下发给 GameSvr 的合成登录包（98 字节）。
    const ORACLE_LOGIN_PACKET: &str = "23305e5e3c716f6d446f6e6f5478615f3c786e61497b5f623f5f6162586b6062586b61625a675f615a5f60605a6b60615a6a62603f6b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b6060586b5f603f21";

    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let srv_port = l.local_addr().unwrap().port();
    let srv_task = tokio::spawn(async move {
        let (mut sock, _) = l.accept().await.unwrap();
        let open = read_frame(&mut sock).await.expect("GM_OPEN");
        let login = read_until(&mut sock, |m, _| m.ident == u16::from(grobal2::GM_DATA))
            .await
            .expect("合成登录包");
        (open.0, login.0, login.1)
    });

    let gate_port = support::next_port();
    let gate = Gate::start("gg-oracle", srv_port, gate_port).await;
    let mut client = gate.connect().await;
    client.write_all(&hex(GOLDEN_LOGIN_FRAME)).await.unwrap();
    client.flush().await.unwrap();

    let (open, msg, body) = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未按时结束")
        .unwrap();

    let expect = hex(ORACLE_LOGIN_PACKET);
    assert_eq!(body.len(), 98, "合成登录包长度必须 = oracle 的 98");
    assert_eq!(
        body, expect,
        "合成登录包必须逐字节等于真 C# 网关吐出的那条（oracle 字节，非公式自证）"
    );
    assert_eq!(msg.pack_length, 98, "PackLength = encodeLen + 3 = 98");
    assert_eq!(msg.session_id, open.session_id);
    // oracle 这条文本用的是 `/` 分隔（`HUtil32.Backslash='/'`）
    let text = mir2_protocol::edcode::decode(&body[2..body.len() - 1]);
    assert!(
        text.starts_with(b"**loadbot1/lb1a/2/120020522/1200205220/"),
        "文本必须以 `/` 分隔的账号/角色开头，实得 {:?}",
        String::from_utf8_lossy(&text[..text.len().min(48)])
    );
    gate.shutdown().await;
}

/// **判据 0 的红检**：客户端 face **没有定长头**——给登录帧套一个 20B `ServerMessage` 头，
/// 会变成"帧内垃圾前缀"⇒ 照 oracle 必须**剔除**（且 5000 侧不得出现任何 `GM_DATA`）。
///
/// 这条用例的牙齿：谁把入站 reading 改回"读 20B 头"，它就会变红。
///
/// 同时钉死**离开路径的同步性**：客户端读到 EOF 就立刻 `shutdown()` 杀网关进程，中间不 sleep、
/// 不等任何队列。oracle 的 `ClientThread.Send` 是同步 `Socket.Send`（`ClientThread.cs:393-399`），
/// 所以网关必须在让客户端看到 EOF 之前把 `GM_CLOSE` 写进内核；若有人把离开路径改回归普通
/// 异步入队，这条用例会在高负载并行门禁下偶发丢 `GM_CLOSE`（2026-10-20 的实际红因）。
#[tokio::test]
async fn header_prefixed_login_is_kicked() {
    const GOLDEN_LOGIN_FRAME: &str = "23315e5e3c716f6d446f6e6f5478615f3c786e61497b5f623f5f6162586b6062586b61625a675f615a5f60605a6b60615a6a62603f6b6062667663606b756277667761625662626151666161516b7660647977605969757651666361697674615667607656627563516677615b6a21";
    let frame = hex(GOLDEN_LOGIN_FRAME);
    let mut head = ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: 0,
        session_id: 0,
        ident: 11,
        session_index: 0,
        pack_length: i32::try_from(frame.len()).unwrap(),
    }
    .to_bytes()
    .to_vec();
    head.extend_from_slice(&frame);

    let (srv_port, srv_task) = fake_game_svr().await;
    let gate_port = support::next_port();
    let gate = Gate::start("gg-hdr", srv_port, gate_port).await;
    let mut client = gate.connect().await;
    client.write_all(&head).await.unwrap();
    client.flush().await.unwrap();

    // 网关应主动断开（读到 EOF）
    let mut buf = [0u8; 1];
    let n = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf))
        .await
        .expect("5s 内网关应剔除该连接")
        .expect("读应成功（EOF=0 也算）");
    assert_eq!(n, 0, "带 20B 头的登录帧必须被剔除（oracle 实测如此）");
    gate.shutdown().await;
    drop(client);

    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未结束")
        .unwrap();
    let idents: Vec<u16> = got.iter().map(|(m, _)| m.ident).collect();
    assert_eq!(
        idents,
        vec![u16::from(grobal2::GM_OPEN), u16::from(grobal2::GM_CLOSE)],
        "剔除路径不得出现 GM_DATA；且 GM_CLOSE 必须已在客户端 EOF 前写入"
    );
    // `GM_CLOSE` 的字段照实机取证：session_id=0、socket=客户端 handle、无 body。
    let (open, open_body) = &got[0];
    let (close, close_body) = &got[1];
    assert_eq!(
        open_body, b"127.0.0.1",
        "GM_OPEN 的 body 必须是客户端 IP 串"
    );
    assert_eq!(close.packet_code, grobal2::PacketCode);
    assert_eq!(close.session_id, 0, "GM_CLOSE 的 SessionId 必须是 0");
    assert_eq!(
        close.socket, open.socket,
        "GM_CLOSE 的 Socket 必须与 GM_OPEN 的客户端 handle 一致"
    );
    assert_eq!(close.session_index, 0, "GM_CLOSE 的 SessionIndex 必须是 0");
    assert_eq!(close.pack_length, 0, "GM_CLOSE 无 body ⇒ PackLength=0");
    assert!(close_body.is_empty(), "GM_CLOSE 不得带 body");
}

/// **判据（下行）**：网关 → 客户端的两种出帧形态必须等于 oracle 实测字节。
///
/// oracle 实测（真 C# 网关，见 `oracle-login-cap/FINDINGS.md`）：
/// - `PackLength=+16`、body `10 11 … 1f` ⇒ 客户端收到 `#` + Encode(前 12B) + 余下 4B + `!`；
/// - `PackLength=-4`、body `de ad be ef` ⇒ 客户端收到 `#` + body 原样 + `!`。
#[tokio::test]
async fn downlink_frames_match_oracle_bytes() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let srv_port = l.local_addr().unwrap().port();
    let srv_task = tokio::spawn(async move {
        let (mut sock, _) = l.accept().await.unwrap();
        let open = read_frame(&mut sock).await.expect("GM_OPEN");
        // 等登录通过（合成登录包），再按 oracle 的两种形态各回一帧
        let _login = read_until(&mut sock, |m, _| m.ident == u16::from(grobal2::GM_DATA))
            .await
            .expect("合成登录包");
        let sid = open.0.session_id;
        let body_a: Vec<u8> = (0x10u8..0x20).collect(); // 16B
        let head_a = ServerMessage {
            packet_code: grobal2::PacketCode,
            socket: 0,
            session_id: sid,
            ident: u16::from(grobal2::GM_DATA),
            session_index: 0,
            pack_length: i32::try_from(body_a.len()).unwrap(),
        };
        let mut wire = head_a.to_bytes().to_vec();
        wire.extend_from_slice(&body_a);
        sock.write_all(&wire).await.unwrap();
        sock.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let body_b = [0xDEu8, 0xAD, 0xBE, 0xEF]; // 4B 小包
        let head_b = ServerMessage {
            packet_code: grobal2::PacketCode,
            socket: 0,
            session_id: sid,
            ident: u16::from(grobal2::GM_DATA),
            session_index: 0,
            pack_length: -i32::try_from(body_b.len()).unwrap(),
        };
        let mut wire = head_b.to_bytes().to_vec();
        wire.extend_from_slice(&body_b);
        sock.write_all(&wire).await.unwrap();
        sock.flush().await.unwrap();
    });

    let gate_port = support::next_port();
    let gate = Gate::start("gg-down", srv_port, gate_port).await;
    let mut client = gate.connect().await;
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();

    let mut got = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut buf = [0u8; 256];
    while got.len() < 22 + 6 && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(2), client.read(&mut buf)).await {
            Ok(Ok(0) | Err(_)) | Err(_) => break,
            Ok(Ok(n)) => got.extend_from_slice(&buf[..n]),
        }
    }
    gate.shutdown().await;
    let _ = tokio::time::timeout(Duration::from_secs(2), srv_task).await;

    // 实测字节：`#` + Encode(10..1b) + `1c 1d 1e 1f` + `!`，然后是 `#` + `de ad be ef` + `!`
    let expect_a = hex("2368697a6b6b68756a6a6b7066696a73611c1d1e1f21");
    let expect_b = hex("23deadbeef21");
    let mut expect = expect_a.clone();
    expect.extend_from_slice(&expect_b);
    assert_eq!(
        got, expect,
        "下行两帧必须等于 oracle 实测字节（+16 ⇒ 编码头+原样尾；-4 ⇒ 小包原样）"
    );
}

/// **判据（上限计数）**：登录后超过 `NomClientPacketSize`(400) 的帧**只丢不关**（`ClientSession.cs:196-199`）——
/// 丢完之后同一个会话的命令帧仍应被转发（会话存活）。
#[tokio::test]
async fn oversize_frame_is_dropped_without_kick() {
    let (srv_port, srv_task) = fake_game_svr().await;
    let gate_port = support::next_port();
    let gate = Gate::start("gg-big", srv_port, gate_port).await;
    let mut client = gate.connect().await;
    client.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    client.flush().await.unwrap();
    settle().await; // 等登录处理完
                    // 401 字节的帧（解码后 < 12 会先被 deCodeLen<12 关掉，所以这里用一条**能解码**的大帧：
                    // `#1` + EDCode(401 字节载荷) + `!` ⇒ 帧长 > 400）
    let big = command_frame(&vec![7u8; 401]);
    assert!(
        big.len() > 400,
        "构造的帧必须真的超过 400（实得 {}）",
        big.len()
    );
    client.write_all(&big).await.unwrap();
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    // 会话必须还活着：紧跟一条正常命令帧，它必须被转发
    let ok = command_frame(&[1u8; 12]);
    client.write_all(&ok).await.unwrap();
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    gate.shutdown().await;
    drop(client);

    let got = tokio::time::timeout(Duration::from_secs(5), srv_task)
        .await
        .expect("假 GameSvr 未结束")
        .unwrap();
    let datas: Vec<&(ServerMessage, Vec<u8>)> = got
        .iter()
        .filter(|(m, b)| m.ident == u16::from(grobal2::GM_DATA) && !is_login_packet(b))
        .collect();
    assert_eq!(
        datas.len(),
        1,
        "超限帧必须被丢（不转发），而它后面的正常帧必须转发（会话存活）：期望 1 条命令帧，实得 {}",
        datas.len()
    );
}

/// **B-4 判据 1/2**：槽位按连接生命周期回收；全满时照 oracle 返回最后一个非空槽。
///
/// 旧实现 `used: HashSet` 只 insert 不 remove ⇒ 超过可注入容量后新连接落到「槽满」分支；
/// 这里用**下行可达性**做判据（不依赖 socket handle 是否被 OS 复用）：
/// 槽满分支的下行走宿主 `ConnectionId`（`ClientSession.cs:908`），新客户端收不到自己的数据。
#[tokio::test]
async fn session_slots_release_and_full_branch_follow_oracle() {
    let mut srv = FakeSvr::start().await;
    let gate_port = support::next_port();
    let gate = Gate::start_with_env(
        "gg-b4",
        srv.port,
        gate_port,
        &[("MIR2_GAME_GATE_MAX_SESSION", "2")],
    )
    .await;

    // 两个并发会话占满两个槽。
    let client_a = gate.connect().await;
    let open_a = srv
        .recv_ident(grobal2::GM_OPEN)
        .await
        .expect("A 的 GM_OPEN");
    let mut client_b = gate.connect().await;
    let open_b = srv
        .recv_ident(grobal2::GM_OPEN)
        .await
        .expect("B 的 GM_OPEN");
    assert_ne!(open_a.0.socket, open_b.0.socket, "两个并发会话各占一槽");

    // 槽满：oracle 返回**最后一个非空槽**（B）的 SckHandle，新连接不被关闭、也不入槽。
    let mut client_c = gate.connect().await;
    let open_c = srv
        .recv_ident(grobal2::GM_OPEN)
        .await
        .expect("槽满时的 GM_OPEN（oracle 不干净拒绝）");
    assert_eq!(
        open_c.0.socket, open_b.0.socket,
        "槽满时 GM_OPEN 必须带最后一个非空槽（B）的 handle（ServerService.cs:131-147）"
    );
    assert_ne!(
        open_c.0.session_id, open_b.0.session_id,
        "槽满分支仍分配新 SessionId"
    );
    // 下行按宿主 ConnectionId 路由 ⇒ B 收到发给 C 的数据，C 收不到。
    srv.send_data(gm_data(open_c.0.session_id, 12), &[9u8; 12]);
    let to_b = read_some(&mut client_b, Duration::from_secs(2))
        .await
        .expect("槽满分支的下行必须走宿主（B）的连接");
    assert!(to_b.starts_with(b"#"));
    assert!(
        read_some(&mut client_c, Duration::from_millis(300))
            .await
            .is_none(),
        "槽满分支的新客户端不得收到发给自己的下行（oracle 绑定的是宿主 SessionInfo）"
    );

    // 断开 A/B ⇒ 槽位回收；新连接 D 的下行必须回到 D 自己（泄漏槽时 D 会落槽满分支、收不到）。
    drop(client_a);
    drop(client_b);
    for _ in 0..2 {
        srv.recv_until(|m| m.ident == u16::from(grobal2::GM_CLOSE))
            .await
            .expect("断开必须补发 GM_CLOSE");
    }
    let mut client_d = gate.connect().await;
    let open_d = srv
        .recv_ident(grobal2::GM_OPEN)
        .await
        .expect("D 的 GM_OPEN");
    srv.send_data(gm_data(open_d.0.session_id, 12), &[7u8; 12]);
    let to_d = read_some(&mut client_d, Duration::from_secs(2))
        .await
        .expect("释放槽位后，新会话必须收到自己的下行");
    assert!(to_d.starts_with(b"#"));
    drop(client_d);
    drop(client_c);

    // 顺序 churn：容量 2 下 8 次连/断全部正常（旧 `used` 泄漏会从第 3 次起走下坡）。
    for i in 0..8 {
        let mut client_x = gate.connect().await;
        let open_x = srv
            .recv_ident(grobal2::GM_OPEN)
            .await
            .expect("churn 的 GM_OPEN");
        srv.send_data(
            gm_data(open_x.0.session_id, 12),
            &[u8::try_from(i).unwrap(); 12],
        );
        assert!(
            read_some(&mut client_x, Duration::from_secs(2))
                .await
                .is_some(),
            "第 {i} 次 churn 的下行必须可达（槽位已回收）"
        );
        drop(client_x);
        srv.recv_until(|m| m.ident == u16::from(grobal2::GM_CLOSE))
            .await
            .expect("churn 的 GM_CLOSE");
    }

    gate.shutdown().await;
}

/// **B-5 判据 1**：稳定连接后按周期收 `GM_CHECKCLIENT`，头字段除 PacketCode/Ident 外全 0、
/// 无 body（`ClientThread.cs:112-142` + `TimedService.cs:69-88`）。
#[tokio::test]
async fn gm_checkclient_is_sent_every_keepalive_period() {
    let mut srv = FakeSvr::start().await;
    let gate_port = support::next_port();
    let gate = Gate::start_with_env(
        "gg-b5-ka",
        srv.port,
        gate_port,
        &[("MIR2_GAME_GATE_KEEPALIVE_MS", "150")],
    )
    .await;

    let mut got = Vec::new();
    let start = Instant::now();
    for _ in 0..3 {
        let frame = srv
            .recv_until(|m| m.ident == u16::from(grobal2::GM_CHECKCLIENT))
            .await
            .expect("每周期必须有一帧 GM_CHECKCLIENT");
        got.push((start.elapsed(), frame));
    }
    for (_, (msg, body)) in &got {
        assert_eq!(msg.packet_code, grobal2::PacketCode);
        assert_eq!(msg.ident, u16::from(grobal2::GM_CHECKCLIENT));
        assert_eq!(msg.socket, 0);
        assert_eq!(msg.session_id, 0);
        assert_eq!(msg.session_index, 0);
        assert_eq!(msg.pack_length, 0);
        assert!(
            body.is_empty(),
            "GM_CHECKCLIENT 是 20B 纯头（SendServerMsg 无 body）"
        );
    }
    let gap1 = got[1].0.checked_sub(got[0].0).expect("elapsed 单调递增");
    let gap2 = got[2].0.checked_sub(got[1].0).expect("elapsed 单调递增");
    for gap in [gap1, gap2] {
        assert!(
            gap >= Duration::from_millis(140),
            "心跳间隔不得短于注入周期（150ms），实得 {gap:?}"
        );
        assert!(
            gap <= Duration::from_secs(2),
            "心跳间隔不得漂移到 2s 以上，实得 {gap:?}"
        );
    }
    gate.shutdown().await;
}

/// **B-5 判据 2**：`GM_RECEIVE_OK` 收到即回显同 ident 的 20B 头（其余字段全 0）；
/// 回显必须由收帧触发（< GameSvr 的 `SocCheckTimeOut`=50ms），不能等周期任务。
#[tokio::test]
async fn gm_receive_ok_is_echoed_immediately_with_zeroed_fields() {
    let mut srv = FakeSvr::start().await;
    let gate_port = support::next_port();
    let gate = Gate::start_with_env(
        "gg-b5-ok",
        srv.port,
        gate_port,
        &[("MIR2_GAME_GATE_KEEPALIVE_MS", "60000")],
    )
    .await;

    let mut best = Duration::from_secs(5);
    for _ in 0..5 {
        let sent_at = Instant::now();
        srv.send(ServerMessage {
            packet_code: grobal2::PacketCode,
            socket: 0,
            session_id: 0,
            ident: u16::from(grobal2::GM_RECEIVE_OK),
            session_index: 0,
            pack_length: 0,
        });
        let (echo, body) = srv
            .recv_until(|m| m.ident == u16::from(grobal2::GM_RECEIVE_OK))
            .await
            .expect("GM_RECEIVE_OK 必须被回显（ClientThread.cs:279-280）");
        best = best.min(sent_at.elapsed());
        assert_eq!(echo.packet_code, grobal2::PacketCode);
        assert_eq!(echo.socket, 0);
        assert_eq!(echo.session_id, 0);
        assert_eq!(echo.session_index, 0);
        assert_eq!(echo.pack_length, 0);
        assert!(body.is_empty(), "回显必须是 20B 纯头");
    }
    assert!(
        best < Duration::from_millis(50),
        "回显必须是收到即发（GameSvr 的 SocCheckTimeOut=50ms），最快一轮 {best:?}"
    );
    gate.shutdown().await;
}

/// **B-5 判据 3**：`GM_STOP` 后不再分配新用户（不发 `GM_OPEN`、socket 不关、已有会话不动），
/// 同时停发 `GM_CHECKCLIENT`（`ClientThread.cs:116-123,265-267`；`ServerService.cs:123-129`）。
#[tokio::test]
async fn gm_stop_stops_assignment_but_keeps_existing_sessions() {
    let mut srv = FakeSvr::start().await;
    let gate_port = support::next_port();
    let gate = Gate::start_with_env(
        "gg-b5-stop",
        srv.port,
        gate_port,
        &[("MIR2_GAME_GATE_KEEPALIVE_MS", "150")],
    )
    .await;

    let mut a = gate.connect().await;
    let open_a = srv
        .recv_until(|m| m.ident == u16::from(grobal2::GM_OPEN))
        .await
        .expect("A 的 GM_OPEN");
    // 先完成登录，让后续命令帧走上行转发路径（登录帧本身也会产生一条合成登录 GM_DATA，
    // 下面的 `recv_until(GM_CHECKCLIENT)` 会顺带把它排掉）。
    a.write_all(&login_frame(GOOD_LOGIN)).await.unwrap();
    a.flush().await.unwrap();
    settle().await;
    srv.recv_until(|m| m.ident == u16::from(grobal2::GM_CHECKCLIENT))
        .await
        .expect("GM_STOP 前健康帧必须已经在发");

    srv.send(ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: 0,
        session_id: 0,
        ident: u16::from(grobal2::GM_STOP),
        session_index: 0,
        pack_length: 0,
    });
    // 排掉停机瞬间可能在途的心跳，再看一个完整静默窗。
    tokio::time::sleep(Duration::from_millis(400)).await;
    let _ = srv.recv_for(Duration::from_millis(100)).await;
    let quiet = srv.recv_for(Duration::from_millis(500)).await;
    assert!(
        quiet
            .iter()
            .all(|(m, _)| m.ident != u16::from(grobal2::GM_CHECKCLIENT)),
        "GM_STOP 后必须停发健康帧（`CheckConnectedState` 的 Stop 分支直接 return）"
    );

    // 新客户端：不建会话、不发 GM_OPEN，socket 留在服务端。
    let mut b = gate.connect().await;
    let after_stop = srv.recv_for(Duration::from_millis(600)).await;
    assert!(
        after_stop
            .iter()
            .all(|(m, _)| m.ident != u16::from(grobal2::GM_OPEN)),
        "GM_STOP 后新连接不得拿到 GM_OPEN（ServerManager 只分配 Runing 实例）"
    );
    b.write_all(&login_frame(GOOD_LOGIN))
        .await
        .expect("GM_STOP 后新连接的 socket 不得被网关关闭");
    assert_still_open(&mut b, Duration::from_millis(300)).await;

    // 已有会话照常工作：服务端下行仍能到达 A，A 的上行仍能带原 handle 转发。
    srv.send_data(gm_data(open_a.0.session_id, 12), &[5u8; 12]);
    let to_a = read_some(&mut a, Duration::from_secs(2))
        .await
        .expect("GM_STOP 不得影响已有会话的下行");
    assert!(to_a.starts_with(b"#"));
    a.write_all(&command_frame(&[1u8; 12])).await.unwrap();
    a.flush().await.unwrap();
    let up = srv
        .recv_until(|m| {
            m.ident == u16::from(grobal2::GM_DATA) && m.session_id == open_a.0.session_id
        })
        .await
        .expect("GM_STOP 不得影响已有会话的上行");
    assert_eq!(up.0.socket, open_a.0.socket);

    gate.shutdown().await;
}

/// **audit #18 C1（进程级）**：`GM_CHECKCLIENT` 的 10s 门是严格 `>`——首个周期之前**不出现**
/// 任何健康帧，周期到点才发（`TimedService.cs:69-88`）；帧形状仍是 20B 纯头。
#[tokio::test]
async fn gm_checkclient_waits_for_the_period_before_first_frame() {
    let mut srv = FakeSvr::start().await;
    let gate_port = support::next_port();
    let started = Instant::now();
    let gate = Gate::start_with_env(
        "gg-ka-first",
        srv.port,
        gate_port,
        &[("MIR2_GAME_GATE_KEEPALIVE_MS", "2000")],
    )
    .await;

    // 周期 2000ms、轮询 10ms：前 500ms 内（从子进程启动算起）不得有健康帧。
    let early = srv.recv_for(Duration::from_millis(500)).await;
    assert!(
        early
            .iter()
            .all(|(m, _)| m.ident != u16::from(grobal2::GM_CHECKCLIENT)),
        "首个健康帧必须等满一个周期（严格 `>`），提前收到: {early:?}"
    );
    let (frame, body) = srv
        .recv_until(|m| m.ident == u16::from(grobal2::GM_CHECKCLIENT))
        .await
        .expect("周期到点必须发 GM_CHECKCLIENT");
    // 首帧 elapsed 必须 ≥ 注入周期（容差 100ms）：只测“前 500ms 无帧”无法钉住严格 `>` 的
    // 边界（周期被改成 100ms 时窗口仍会绿）。
    let first_at = started.elapsed();
    assert!(
        first_at >= Duration::from_millis(1900),
        "首个健康帧必须等满注入周期 2000ms（严格 `>`，容差 100ms），实得 {first_at:?}"
    );
    assert!(
        first_at <= Duration::from_secs(5),
        "首个健康帧不得漂移到 5s 以上，实得 {first_at:?}"
    );
    assert_eq!(frame.packet_code, grobal2::PacketCode);
    assert_eq!(frame.socket, 0);
    assert_eq!(frame.session_id, 0);
    assert_eq!(frame.session_index, 0);
    assert_eq!(frame.pack_length, 0);
    assert!(body.is_empty(), "GM_CHECKCLIENT 是 20B 纯头");
    gate.shutdown().await;
}

/// **audit #18 C3（进程级）**：15 天无上行的会话被清理（`ClientThread.ProcessIdleSession`）：
/// 客户端 socket 被关（EOF），网关补发 `GM_CLOSE`。注入阈值 400ms / 扫描 100ms；
/// 严格 `>` 意味着 250ms 时仍必须存活。
#[tokio::test]
async fn idle_session_without_traffic_is_closed_after_timeout() {
    let mut srv = FakeSvr::start().await;
    let gate_port = support::next_port();
    let gate = Gate::start_with_env(
        "gg-idle-close",
        srv.port,
        gate_port,
        &[
            ("MIR2_GAME_GATE_KEEPALIVE_MS", "60000"),
            ("MIR2_GAME_GATE_SESSION_IDLE_MS", "400"),
            ("MIR2_GAME_GATE_SESSION_SWEEP_MS", "100"),
        ],
    )
    .await;
    let started = Instant::now();
    let mut client = gate.connect().await;
    let _ = srv
        .recv_until(|m| m.ident == u16::from(grobal2::GM_OPEN))
        .await
        .expect("GM_OPEN");

    let mut buf = [0u8; 16];
    let n = tokio::time::timeout(Duration::from_secs(3), client.read(&mut buf))
        .await
        .expect("空闲超时后必须关闭客户端 socket")
        .expect("读客户端 socket 失败");
    assert_eq!(n, 0, "空闲清理必须关 socket（客户端读到 EOF）");
    // 严格 `>` + 扫描粒度：关闭必须发生在注入阈值 400ms **之后**（容差 20ms），而不是
    // 只在 250ms 窗口里“还活着”就算过关。
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(380),
        "注入阈值 400ms（严格 `>`）之前不得清理，实得 {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_secs(2),
        "空闲清理不得漂移到 2s 以上，实得 {elapsed:?}"
    );
    srv.recv_until(|m| m.ident == u16::from(grobal2::GM_CLOSE))
        .await
        .expect("空闲清理后网关必须补发 GM_CLOSE（照断开回调）");
    gate.shutdown().await;
}

/// **audit #18 C4（存活时零帧）**：有上行（1 字节心跳/短帧）的会话不进入空闲清理；
/// 存活期间空闲面**不产生任何下行帧**（健康帧周期注入 60s，窗口内也不会出现）。
/// 停止上行后再超时会被清理 ⇒ 证明 `ReceiveTick` 确实随上行刷新，而不是永不清理。
#[tokio::test]
async fn live_session_with_traffic_stays_open_and_emits_no_frames() {
    let mut srv = FakeSvr::start().await;
    let gate_port = support::next_port();
    let gate = Gate::start_with_env(
        "gg-idle-live",
        srv.port,
        gate_port,
        &[
            ("MIR2_GAME_GATE_KEEPALIVE_MS", "60000"),
            ("MIR2_GAME_GATE_SESSION_IDLE_MS", "400"),
            ("MIR2_GAME_GATE_SESSION_SWEEP_MS", "100"),
        ],
    )
    .await;
    let mut client = gate.connect().await;
    let _ = srv
        .recv_until(|m| m.ident == u16::from(grobal2::GM_OPEN))
        .await
        .expect("GM_OPEN");

    // ~720ms 持续上行（每 90ms 一帧）：远过 400ms 的注入阈值。
    for _ in 0..8 {
        client.write_all(&[0u8]).await.unwrap();
        client.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(90)).await;
    }
    // 停止上行后的 200ms（< 400ms 阈值）内仍不得有下行帧/关闭。
    let frames = srv.recv_for(Duration::from_millis(200)).await;
    assert!(
        frames
            .iter()
            .all(|(m, _)| m.ident != u16::from(grobal2::GM_CLOSE)),
        "存活会话不得被空闲清理（也不得有其它下行）: {frames:?}"
    );
    assert!(
        frames.is_empty(),
        "存活期间空闲面必须零帧（健康帧周期 60s）: {frames:?}"
    );

    // 停止上行：阈值后必须清理（牙：把 last_recv 刷新删掉，上面的存活断言会先红）。
    let mut buf = [0u8; 16];
    let n = tokio::time::timeout(Duration::from_secs(3), client.read(&mut buf))
        .await
        .expect("停止上行后必须触发空闲清理")
        .expect("读客户端 socket 失败");
    assert_eq!(n, 0, "停止上行后应读 EOF");
    srv.recv_until(|m| m.ident == u16::from(grobal2::GM_CLOSE))
        .await
        .expect("空闲清理后必须补发 GM_CLOSE");
    gate.shutdown().await;
}

/// **T-119 进程级（下行重连）**：网关→GameSvr 断链后按判定周期调用 `Start()` 重连
/// （`ClientThread.cs:449-455`），重连成功后恢复 `GM_CHECKCLIENT`。
///
/// 夹具顺序是确定的：首连接**先收到一帧健康帧**再被主动断开 ⇒ 断链时链路确实就绪过；
/// 监听 socket 仍在 ⇒ 重连必然能建立。注入 150ms 周期；5s 内未重连即失败。
#[tokio::test]
async fn reconnects_after_downstream_drop_and_resumes_keepalive() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let srv_port = listener.local_addr().unwrap().port();
    let (done_tx, mut done_rx) = mpsc::unbounded_channel::<()>();
    let srv = tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.expect("初始连接");
        let (msg, _body) = read_frame(&mut first).await.expect("首连接的首帧");
        assert_eq!(
            msg.ident,
            u16::from(grobal2::GM_CHECKCLIENT),
            "断开前链路必须已就绪（已发健康帧）"
        );
        drop(first); // 模拟 GameSvr 重启/断链
        let (mut second, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .expect("网关必须在 5s 内重连 GameSvr")
            .expect("accept 重连");
        loop {
            let (msg, _body) = read_frame(&mut second).await.expect("重连后的帧");
            if msg.ident == u16::from(grobal2::GM_CHECKCLIENT) {
                done_tx.send(()).unwrap();
                break;
            }
        }
        // 保持第二条连接直到用例结束（drop 只影响本任务）。
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let gate_port = support::next_port();
    let gate = Gate::start_with_env(
        "gg-reconnect",
        srv_port,
        gate_port,
        &[("MIR2_GAME_GATE_KEEPALIVE_MS", "150")],
    )
    .await;
    tokio::time::timeout(Duration::from_secs(5), done_rx.recv())
        .await
        .expect("重连后必须恢复健康帧（Start() 未接线时这里会超时）")
        .expect("夹具通道不得提前关闭");
    gate.shutdown().await;
    srv.abort();
}

/// **audit #18 ③ + `RestSessionArray`（进程级）**：下行断链后，既有会话被 `RestSessionArray()`
/// 的 `Socket = null` 摘出空闲清理面（`ClientThread.cs:186/208/332-346`；`ProcessIdleSession`
/// 只看 `Socket != null`，`:410-427`）⇒ 注入的 15 天阈值过去后客户端**不会**被空闲清理关闭。
///
/// 牙：删掉 `IdleEntry.idle_eligible` 门（或断链时不摘除）⇒ 扫描在 1000ms 关掉客户端 ⇒ 红。
/// 断开时机由 channel 同步（客户端会话已建立后才断链），避免与启动竞速。
#[tokio::test]
async fn downstream_drop_detaches_existing_sessions_from_idle_close() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let srv_port = listener.local_addr().unwrap().port();
    let (drop_tx, mut drop_rx) = mpsc::unbounded_channel::<()>();
    let srv = tokio::spawn(async move {
        let (first, _) = listener.accept().await.expect("初始连接");
        let _ = drop_rx.recv().await;
        drop(first);
        // 重连也保持住：`ClientSocketConnect` 会再跑一次 RestSessionArray。
        if let Ok(Ok((second, _))) =
            tokio::time::timeout(Duration::from_secs(5), listener.accept()).await
        {
            tokio::time::sleep(Duration::from_secs(5)).await;
            drop(second);
        }
    });

    let gate_port = support::next_port();
    let gate = Gate::start_with_env(
        "gg-idle-rest",
        srv_port,
        gate_port,
        &[
            ("MIR2_GAME_GATE_KEEPALIVE_MS", "150"),
            ("MIR2_GAME_GATE_SESSION_IDLE_MS", "1000"),
            ("MIR2_GAME_GATE_SESSION_SWEEP_MS", "100"),
        ],
    )
    .await;
    let started = Instant::now();
    let mut client = gate.connect().await;
    settle().await; // 确保客户端会话与 IdleEntry 已登记（GM_OPEN 之后登记）
    drop_tx.send(()).unwrap();
    // 跨过 1000ms 阈值 + 多个 100ms 扫描周期。
    tokio::time::sleep(Duration::from_millis(1600)).await;
    let mut buf = [0u8; 16];
    match tokio::time::timeout(Duration::from_millis(200), client.read(&mut buf)).await {
        Err(_) => {} // 无数据、无 EOF ⇒ 会话仍存活（正确：已被 RestSessionArray 摘除）
        Ok(Ok(0)) => panic!(
            "下行断链后的既有会话被空闲清理关闭（elapsed={:?}）；oracle 的 RestSessionArray \
             会把 SessionInfo.Socket 置 null、从而跳过 ProcessIdleSession",
            started.elapsed()
        ),
        Ok(Ok(n)) => panic!("不应有下行数据，实得 {n} 字节"),
        Ok(Err(e)) => panic!("客户端读错误: {e}"),
    }
    gate.shutdown().await;
    srv.abort();
}
