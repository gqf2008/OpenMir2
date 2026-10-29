// 文档与错误串逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

//! LoginSrv 的**进程级自测**（照 `bins/login-gate/tests/relay.rs` 风格）。
//!
//! 判据（真判据，非自洽往返）：
//! 1. **与 C# 真身逐字节一致**：把 `tests/parity/loginsrv/oracle_capture.py` 采到的
//!    C# 响应帧（`oracle-bytes.json`）当金标准，用同样的假 LoginGate 序列打 Rust 版，
//!    逐帧 `raw_hex` 比对（帧尾 `!$`、12B 头字段、体字节全比）。
//!    —— 这条需要本机 MySQL（`mir2_account` 里有 `loadbot1`/`mir2test`），默认 `#[ignore]`。
//! 2. **失败要响（M1④）**：缺配置键、连不上 MySQL ⇒ 进程非零退出并打印明确错误。

use std::path::{Path, PathBuf};
use std::time::Duration;

use mir2_protocol::edcode;
use mir2_protocol::frame::{CommandMessage, ServerDataPacket};
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use mir2_protocol::messages;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// 无竞态端口 / 就绪门（`next_port()` 与 `game-svr/tests/support/mod.rs` 逐行同口径）。
mod support;

/// 「缺配置 / 缺 AddrTable」子进程的退出上界。依据：本机实测 **0.05s**（读配置即报错退出）；
/// 取 10s = 实测的 ~200× 余量。
const QUICK_EXIT_BOUND: Duration = Duration::from_secs(10);

/// 「连不上 MySQL」子进程的退出上界。依据：本机实测 **30.10s** ——
/// `login-srv` 对必不通的 DSN（`127.0.0.1:3399`）先耗尽 MySQL 连接超时才打印「SQL 连接失败」并退出
/// （本机基线 `login.rs` target 4 条用例合计 **30.22s**，该条占绝大部分）；取 **90s** = 3× 实测，
/// C# 金标准（`oracle_capture.py` 产出）。
const ORACLE: &str = include_str!("../../../tests/parity/loginsrv/oracle-bytes.json");

fn lab_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lsrv-test-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 写一份与 oracle 采集同形的配置（含 GB2312 的 AddrTable/UserLimit）。
fn write_lab(dir: &Path, gate_port: u16, srv_port: u16, dsn: &str) -> PathBuf {
    let conf = format!(
        "[Server]\nReadyServers=0\nEnableMakingID=1\nTestServer=1\n\
         GateAddr=127.0.0.1\nGatePort={gate_port}\nServerAddr=127.0.0.1\nServerPort={srv_port}\n\
         DynamicIPMode=0\nDBServer=127.0.0.1\nDBSPort=6000\n\n[DataBase]\nDbType=MySQL\n\
         ConnctionString={dsn}\n"
    );
    let conf_path = dir.join("logsrv.conf");
    std::fs::write(&conf_path, conf).unwrap();
    std::fs::write(
        dir.join("AddrTable.txt"),
        gbk("热血传奇 Title1 127.0.0.1 127.0.0.1 127.0.0.1:7100\n"),
    )
    .unwrap();
    std::fs::write(dir.join("ServerAddr.txt"), "127.0.0.1\n").unwrap();
    std::fs::write(dir.join("UserLimit.txt"), gbk("热血传奇 热血传奇 10000\n")).unwrap();
    conf_path
}

/// A2（卡 `openmir2-login-srv-illegal-addr-filter`）：**127.0.0.1 连入 5600 不得被
/// 一票否决**。oracle：C# 生产白名单（ServerAddr.txt=4 旧网段 IP）不含 127.0.0.1，
/// 运行二进制仍接受（LoginSrv.out.log 18:13:32.179 `[DBG] 127.0.0.1:64174链接成功`，
/// 三次 login_only 全链 completed）⇒ 白名单不命中仅记日志、不断链。夹具刻意写生产同款
/// 白名单（无 127.0.0.1）以复现否决触发条件。
///
/// 反向牙（A2 变异）：恢复 `drop(sock); continue;` 一票否决 ⇒ 连接被立即关闭 ⇒ 本牙红。
#[tokio::test]
async fn session_face_accepts_loopback_despite_whitelist_miss() {
    let dir = lab_dir("noaddrfilter");
    let gate_port = support::next_port();
    let srv_port = support::next_port();
    let conf = write_lab(
        &dir,
        gate_port,
        srv_port,
        "server=127.0.0.1;port=3399;uid=root;pwd=;database=mir2_account;",
    );
    // 覆写为**生产同款**白名单（无 127.0.0.1）：正是它让旧实现否决 db-srv 的链入。
    std::fs::write(
        dir.join("ServerAddr.txt"),
        "10.10.0.163
10.10.0.102
",
    )
    .unwrap();
    let log = dir.join("login-srv.out.log");
    let out_f = std::fs::File::create(&log).unwrap();
    let err_f = out_f.try_clone().unwrap();
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_login-srv"));
    cmd.arg(&conf)
        .stdout(out_f)
        .stderr(err_f)
        .kill_on_drop(true);
    let mut child = cmd.spawn().expect("启动子进程失败");
    // 5600 面就绪（DSN 失效走 fail-soft，不影响监听）
    support::gate_ready(&log, srv_port, std::time::Duration::from_secs(15)).await;
    // 127.0.0.1 连入（db-srv IDS 链路同型）
    let mut c = tokio::net::TcpStream::connect(("127.0.0.1", srv_port))
        .await
        .expect("127.0.0.1 连入 5600 必须被接受");
    // 白名单外 ⇒ 旧实现记 WRN（oracle 同款日志仍保留）
    let text = support::wait_log_contains(
        &log,
        "非法地址连接:127.0.0.1",
        std::time::Duration::from_secs(5),
    )
    .await;
    assert!(
        text.contains("非法地址连接:127.0.0.1"),
        "白名单外连入必须仍记日志（oracle 保留 Warn），实得：{text}"
    );
    // 核心断言：**不断链**。读 socket：对端关闭 ⇒ Ok(0)（旧实现必如此 ⇒ 红）；
    // 未关闭 ⇒ 读挂起 ⇒ 超时路径 ⇒ ok。
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let mut probe_buf = [0u8; 1];
    let closed = tokio::time::timeout(
        std::time::Duration::from_millis(400),
        c.read(&mut probe_buf),
    )
    .await
    .is_ok_and(|r| matches!(r, Ok(0) | Err(_)));
    assert!(
        !closed,
        "127.0.0.1 连入 5600 不得被一票否决关闭（oracle 接受语义）"
    );
    child.kill().await.ok();
    std::fs::remove_dir_all(&dir).ok();
}

/// GB2312 编码（测试侧只需 ASCII + “热血传奇”）：与生产同一 `.NET cp936` 入口，
/// 避免夹具与线上编码口径分叉。
fn gbk(s: &str) -> Vec<u8> {
    mir2_script::cp936::encode_gb2312_dotnet(s)
}

/// 客户端帧 `#1<enc(头)>[<enc(体)>]!`。
fn client_frame(ident: u16, recog: i32, body: &[u8]) -> Vec<u8> {
    let head = CommandMessage::make(i32::from(ident), recog, 0, 0, 0);
    let mut out = vec![b'#', b'1'];
    out.extend_from_slice(&edcode::encode(&head.to_bytes()));
    if !body.is_empty() {
        out.extend_from_slice(&edcode::encode(body));
    }
    out.push(b'!');
    out
}

/// 内部帧（6B 头 + MemoryPack 体）。
fn sdm(kind: ServerDataType, socket_id: &str, data: &[u8]) -> Vec<u8> {
    ServerDataMessage {
        kind,
        socket_id: socket_id.to_string(),
        data_len: i16::try_from(data.len()).unwrap_or(i16::MAX),
        data: data.to_vec(),
    }
    .to_wire()
}

async fn connect_with_retry(port: u16) -> TcpStream {
    for _ in 0..200 {
        if let Ok(s) = TcpStream::connect(("127.0.0.1", port)).await {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("Rust login-srv 未在预期时间内监听 {port}");
}

/// 读一坨下行帧（内部帧），空闲 150ms 即认为读完。
async fn read_frames(sock: &mut TcpStream) -> Vec<ServerDataMessage> {
    let mut out = Vec::new();
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        match tokio::time::timeout(Duration::from_millis(150), sock.read(&mut chunk)).await {
            Ok(Ok(n)) if n > 0 => buf.extend_from_slice(&chunk[..n]),
            _ => break,
        }
        while buf.len() >= ServerDataPacket::SIZE {
            let pkt = ServerDataPacket::from_bytes(&buf).unwrap();
            let total = ServerDataPacket::SIZE + usize::from(pkt.packet_len);
            if buf.len() < total {
                break;
            }
            let msg = ServerDataMessage::from_bytes(&buf[ServerDataPacket::SIZE..total]).unwrap();
            out.push(msg);
            buf.drain(..total);
        }
    }
    out
}

/// 一次假网关会话：Enter → 逐条 Data（每条读一次下行）。
async fn session(port: u16, sid: &str, calls: &[&[u8]]) -> Vec<Vec<u8>> {
    let mut sock = connect_with_retry(port).await;
    sock.write_all(&sdm(ServerDataType::Enter, sid, b"127.0.0.1/127.0.0.1"))
        .await
        .unwrap();
    let mut got = Vec::new();
    for call in calls {
        sock.write_all(&sdm(ServerDataType::Data, sid, call))
            .await
            .unwrap();
        sock.flush().await.unwrap();
        for m in read_frames(&mut sock).await {
            if m.kind == ServerDataType::Data {
                got.push(m.data);
            }
        }
    }
    drop(sock);
    got
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn hex_to_bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}

/// 把金标准 JSON 里某个 case 的响应 `raw_hex` 列表取出来。
fn oracle_frames(doc: &serde_json::Value, case: &str) -> Vec<String> {
    doc["cases"][case]
        .as_array()
        .unwrap_or_else(|| panic!("金标准缺 case {case}"))
        .iter()
        .flat_map(|c| {
            c["frames"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|f| f["raw_hex"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// **主判据**：Rust LoginSrv 的响应帧必须与 C# 真身逐字节相同（含错误分支）。
///
/// 需要本机 MySQL（`mir2_account`：`loadbot1/loadbot1`、`mir2test/mir2pass`）：
/// ```text
/// cargo test -p login-srv --test login -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "需要本机 MySQL mir2_account；显式 --ignored 运行"]
#[allow(clippy::too_many_lines)] // 就绪门 + 逐帧 oracle 比对同在一个用例里（与 db-srv live_* 同口径）
async fn responses_match_csharp_oracle_bytes() {
    let doc: serde_json::Value = serde_json::from_str(ORACLE).unwrap();
    let server_name = gbk("热血传奇");
    let gate = support::next_port();
    let srv = support::next_port();
    let dir = lab_dir("oracle");
    let conf = write_lab(
        &dir,
        gate,
        srv,
        "server=127.0.0.1;uid=root;pwd=;database=mir2_account;",
    );
    // stdout 落夹具文件：就绪门要读 `账号登陆服务[...]已启动.`（bind 成功之后才打印）。
    let log = dir.join("logsrv.out.log");
    let out = std::fs::File::create(&log).unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_login-srv"))
        .arg(&conf)
        .stdout(out)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    // 就绪门（fail-closed）：gate 面 bind 成功才发流量；未起来 ⇒ panic 并带全文日志。
    support::gate_ready(&log, gate, Duration::from_secs(20)).await;
    // 等 5500（本例是 gate 端口）就绪：先连一次再关
    let probe = connect_with_retry(gate).await;
    drop(probe);

    // 心跳（真身字节，2026-10-11 从运行中的 C# LoginGate 直抓）：
    // `SocketId=""` + `Data=null`（MemoryPack 长度 -1）。修前该帧被当 Truncated 拒绝 ⇒
    // 网关 30s 后判"账号服务器长时间没有响应"并断开（LoginGate 只连一次、不重连 ⇒ 整跳死掉）。
    {
        let mut sock = connect_with_retry(gate).await;
        sock.write_all(&sdm(
            ServerDataType::Enter,
            "keepalive",
            b"127.0.0.1/127.0.0.1",
        ))
        .await
        .unwrap();
        sock.write_all(&hex_to_bytes("9aaa9aaa0c000403000000000000ffffffff"))
            .await
            .unwrap();
        sock.flush().await.unwrap();
        let frames = read_frames(&mut sock).await;
        let hb = frames
            .iter()
            .find(|m| m.kind == ServerDataType::KeepAlive)
            .expect("C# 心跳帧必须被接受并回 KeepAlive");
        assert_eq!(hb.socket_id, "");
        assert!(hb.data.is_empty());
        assert_eq!(
            hex(&hb.to_wire()),
            "9aaa9aaa0c00040300000000000000000000",
            "回帧口径：null 字段按空编码（C# 侧是 null；接收方只读 Type，行为等价）"
        );
    }

    let cases: [(&str, Vec<Vec<u8>>); 6] = [
        (
            "protocol",
            vec![client_frame(messages::CM_PROTOCOL, 20_011_006, b"")],
        ),
        (
            "login_ok",
            vec![client_frame(
                messages::CM_IDPASSWORD,
                0,
                b"loadbot1/loadbot1",
            )],
        ),
        (
            "select_server",
            vec![
                client_frame(messages::CM_IDPASSWORD, 0, b"loadbot1/loadbot1"),
                client_frame(messages::CM_SELECTSERVER, 0, &server_name),
            ],
        ),
        (
            "login_bad_password",
            vec![client_frame(
                messages::CM_IDPASSWORD,
                0,
                b"loadbot1/wrong-password",
            )],
        ),
        (
            "login_unknown_account",
            vec![client_frame(messages::CM_IDPASSWORD, 0, b"nosuchacct/x")],
        ),
        (
            "login_needupdate",
            vec![client_frame(
                messages::CM_IDPASSWORD,
                0,
                b"mir2test/mir2pass",
            )],
        ),
    ];

    for (sid, (case, calls)) in (1..).zip(cases.iter()) {
        let refs: Vec<&[u8]> = calls.iter().map(Vec::as_slice).collect();
        let got = session(gate, &sid.to_string(), &refs).await;
        let want = oracle_frames(&doc, case);
        let got_hex: Vec<String> = got.iter().map(|f| hex(f)).collect();
        assert_eq!(
            got_hex, want,
            "case={case}：Rust 响应帧与 C# 金标准不一致\n  期望 {want:?}\n  实得 {got_hex:?}"
        );
    }
    child.kill().await.ok();
    std::fs::remove_dir_all(&dir).ok();
}

/// 缺关键配置键 ⇒ 非零退出（M1④）。
#[tokio::test]
async fn missing_config_exits_nonzero() {
    let dir = lab_dir("noconf");
    std::fs::write(dir.join("logsrv.conf"), "[Server]\nGatePort=0\n").unwrap();
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_login-srv"));
    cmd.arg(dir.join("logsrv.conf"));
    let out = support::output_bounded(&mut cmd, "login-srv 缺配置非零退出", QUICK_EXIT_BOUND).await;
    assert!(!out.status.success(), "缺配置必须非零退出（M1④）");
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("端口为 0 非法"),
        "错误应指明配置问题，实得：{text}"
    );
    // 缺 AddrTable.txt（选服路由来源）同样必须报错退出
    let dir2 = lab_dir("noaddr");
    let conf2 = dir2.join("logsrv.conf");
    std::fs::write(
        &conf2,
        "[Server]\nGatePort=5598\nServerPort=5698\n[DataBase]\nConnctionString=x\n",
    )
    .unwrap();
    std::fs::write(dir2.join("UserLimit.txt"), gbk("热血传奇 g 10000\n")).unwrap();
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_login-srv"));
    cmd.arg(&conf2);
    let out2 = support::output_bounded(
        &mut cmd,
        "login-srv 缺 AddrTable 非零退出",
        QUICK_EXIT_BOUND,
    )
    .await;
    assert!(
        !out2.status.success(),
        "缺 AddrTable 必须非零退出（A-11/M1④）"
    );
    let text2 =
        String::from_utf8_lossy(&out2.stdout).to_string() + &String::from_utf8_lossy(&out2.stderr);
    assert!(
        text2.contains("AddrTable"),
        "错误应指明缺 AddrTable，实得：{text2}"
    );
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&dir2).ok();
}

/// 连不上 MySQL ⇒ **fail-soft 常驻**（卡 `openmir2-login-srv-dsn-source`）：
/// 两监听照常就绪、日志含「SQL 连接失败」、进程**不退出**（oracle `Initialization`
/// catch-and-continue；旧 M1④/A-6「必须报错退出」的授权偏离已按 oracle 撤销，
/// 本牙即新语义的反向钉：谁把「退出」改回来谁红）。
#[tokio::test]
async fn mysql_unreachable_fail_soft_and_stays_up() {
    let dir = lab_dir("nodbmysql");
    let gate_port = support::next_port();
    let srv_port = support::next_port();
    // 配置齐全，但 DSN 指向一个必不通的端口
    let conf = write_lab(
        &dir,
        gate_port,
        srv_port,
        "server=127.0.0.1;port=3399;uid=root;pwd=;database=mir2_account;",
    );
    let log = dir.join("login-srv.out.log");
    let out_f = std::fs::File::create(&log).unwrap();
    let err_f = out_f.try_clone().unwrap();
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_login-srv"));
    cmd.arg(&conf)
        .stdout(out_f)
        .stderr(err_f)
        .kill_on_drop(true);
    let mut child = cmd.spawn().expect("启动子进程失败");
    // 两监听就绪（bind 与 SQL 连接无关）
    support::gate_ready(&log, gate_port, std::time::Duration::from_secs(15)).await;
    support::gate_ready(&log, srv_port, std::time::Duration::from_secs(15)).await;
    // SQL 失败日志（C# Initialization 的 catch 文本同款）
    let text =
        support::wait_log_contains(&log, "SQL 连接失败", std::time::Duration::from_secs(10)).await;
    assert!(
        text.contains("SQL 连接失败"),
        "必须记录 SQL 连接失败（oracle catch 文本），实得：{text}"
    );
    // fail-soft 核心断言：进程仍常驻，不得退出
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    assert!(
        matches!(child.try_wait(), Ok(None)),
        "DSN 失效 ⇒ 服务必须常驻（oracle fail-soft），不得退出；子进程日志：
{}",
        std::fs::read_to_string(&log).unwrap_or_default()
    );
    child.kill().await.ok();
    std::fs::remove_dir_all(&dir).ok();
}
