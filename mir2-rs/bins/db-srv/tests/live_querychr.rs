//! **进程级混跑验收**（M1 ①/②/④）：用「伪 LoginSrv + 伪 SelGate」把 DBSrv 单独架起来跑，
//! 逐字节与 C# oracle 捕获的金标准比对。
//!
//! 三个互相独立的场景（都 `#[ignore]`，缺前置即 panic，不放行）：
//! - [`rust_dbsrv_querychr_matches_csharp_golden`]：**Rust db-srv** 顶住 5100 面（④ 进程级自测）；
//! - [`csharp_dbsrv_querychr_matches_csharp_golden`]：**C# DBSrv**（`MIR2_ORACLE_DBSRV_DIR` 指向它的一份
//!   副本目录）跑同样的脚本 ⇒ 两边都等于同一个**外部金标准**，即 ②「跨实现互读 字段级 diff = 0」；
//! - [`csharp_selgate_to_rust_dbsrv`]：**C# SelGate**（`MIR2_ORACLE_SELGATE_DIR`）中继 → Rust db-srv（① 混跑链）。
//!
//! 为什么要「伪 LoginSrv」：DBSrv 的角色列表查询要求会话校验（`CheckSession` 只比账号+会话号），
//! 而会话是 LoginSrv 通过 `(100/账号/会话号/…/IP/秒数)` 推给 DBSrv 的。自己起一个只发这一条报文的
//! 小服务器，就能把 DBSrv 的 5100 面**单独**测透（不依赖 LoginGate/客户端/机器人，也就没有 flaky）。
//!
//! 端口一律**临时分配**（`bind :0` 后取值再释放），且配置写在 temp 目录里 —— 不动 `E:\MirServer`
//! 的冻结部署，也不与并行 agent 的现网端口（5100/7000/7100…）打架。
//!
//! 跑法（需要本机 MySQL 与 `mir2_db`；oracle 场景另需 C# 构建产物副本）：
//! ```powershell
//! $env:MIR2_ORACLE_DBSRV_DIR  = "E:\tmp\a2-dbsrv\oracle-dbsrv"      # DBSrv.exe 所在目录（含 Storeage dll）
//! $env:MIR2_ORACLE_SELGATE_DIR= "E:\tmp\a2-dbsrv\oracle-selgate"    # SelGate.exe 所在目录
//! cargo test -p db-srv --test live_querychr --offline -- --ignored --nocapture
//! ```

// 文档逐字引用 C# 标识符/文件名，不加反引号改造（同 bin 侧口径）。
#![allow(clippy::doc_markdown)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mir2_protocol::edcode;
use mir2_protocol::frame::{encode_server_frame, CommandMessage, ServerDataPacket};
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use mir2_protocol::messages::{
    CM_DELCHR, CM_NEWCHR, CM_QUERYCHR, CM_SELCHR, SM_DELCHR_SUCCESS, SM_NEWCHR_FAIL, SM_QUERYCHR,
    SM_QUERYCHR_FAIL, SM_STARTPLAY,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// 账号与两个角色（`mir2_db` 的测试账号，见设计文档 §2；`aaa` 已选中）。
const ACCOUNT: &str = "mir2test";
/// 会话号：由伪 LoginSrv 推给 DBSrv，测试自己定（因此不依赖任何登录流程）。
const SESSION: i32 = 4242;
/// 期望的角色列表字段（C# oracle 捕获：`*aaa/职业/发型/等级/性别/` + `bbb/…`）。
const EXPECT_CHR_LIST: &[u8] = b"*aaa/0/1/1/0/bbb/2/1/1/0/";
/// 同上但去掉 `*`——用于**归一化前**的首次 `SM_QUERYCHR`（`*` 落哪个角色取决于库里
/// `characters_indexes.SelectID`，是共享可变状态；选角步骤后会归一到 [`EXPECT_CHR_LIST`]）。
const EXPECT_CHR_LIST_BARE: &[u8] = b"aaa/0/1/1/0/bbb/2/1/1/0/";
/// C# oracle 捕获的 `SM_QUERYCHR` 下行帧（`#` + 16 字符头 + 编码体 + `!`），取自 C# DBSrv。
const GOLDEN_QUERYCHR_FRAME_HEX: &str = "236664686b64646a696464696b6464686b5e6d49736d5f58685f613f5f615f58685f6e4a6f6e5f5a685f613f5f615f58685f3c21";

/// 选角（`CM_SELCHR`）成功时 `SM_STARTPLAY`(525) 的**路由体**：夹具 `ServerInfo.txt`
/// 写的是 `127.0.0.1 127.0.0.1 7200`，`MapFile` 缺省为空 ⇒ `MapIndex=0`（与 C# `GateRouteIp` 同）。
const EXPECT_STARTPLAY_BODY: &[u8] = b"127.0.0.1/7200";

// ==== 环境与夹具 ====

/// 缺前置即 panic（RULE_可达性：显式选中的测试缺前置必须 fail loud）。
fn env_or(key: &str, def: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| def.to_string())
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

/// 取一个空闲端口（bind :0 → 取值 → 释放）。
fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("取空闲端口失败");
    l.local_addr().expect("取本地地址失败").port()
}

/// 建 temp 目录（含 `AddrTable.txt` / `ServerInfo.txt`，与 C# DBSrv 的部署一致）。
fn fixture_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("a2-dbsrv-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("建 temp 目录失败");
    std::fs::write(d.join("AddrTable.txt"), "127.0.0.1\n").expect("写 AddrTable.txt 失败");
    std::fs::write(d.join("ServerInfo.txt"), "127.0.0.1 127.0.0.1 7200\n")
        .expect("写 ServerInfo.txt 失败");
    d
}

/// 递归复制目录（影子副本用；不动原目录）。
fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("建影子目录失败");
    for entry in std::fs::read_dir(src).expect("读源目录失败") {
        let entry = entry.expect("读目录项失败");
        let to = dst.join(entry.file_name());
        if entry.file_type().expect("取文件类型失败").is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("复制文件失败");
        }
    }
}

/// 写 DBSrv 配置（`dbsvr.conf`；`GatePort` 是本测试要连的 5100 面，`IDSPort` 指向伪 LoginSrv）。
///
/// `show_debug`：② 的两个场景用 `false`（与现网 bin 版 conf 一致 —— C# 侧 `LogService.Debug` 还会被
/// `appsettings.json` 的 Serilog `MinimumLevel=Information` 滤掉，关掉才能让两侧日志行集逐行对齐）；
/// ① 的场景用 `true`，以便断言"心跳被应答"（`响应角色网关心跳...` 是 Debug 级，与 C# 同源同文本）。
fn write_conf(dir: &Path, gate_port: u16, ids_port: u16, show_debug: bool) {
    let body = format!(
        "[Setup]\nShowDebug={}\nServerAddr=127.0.0.1\nServerPort={}\nGateAddr=0.0.0.0\nGatePort={gate_port}\n\
         ServerName=LegendOfMir2\nDenyChrName=1\nDELMaxLevel=30\nDynamicIPMode=0\n\n\
         [Server]\nIDSAddr=127.0.0.1\nIDSPort={ids_port}\nMarketAddr=127.0.0.1\nMarketPort={}\n\n\
         [DataBase]\nStoreage=MySQL\nConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_db;\n\n\
         [DBClear]\nInterval=3000\n",
        u8::from(show_debug),
        free_port(),
        free_port()
    );
    std::fs::write(dir.join("dbsvr.conf"), body).expect("写 dbsvr.conf 失败");
}

/// 起一个「伪 LoginSrv」：接受 DBSrv 的连接，**周期性**推 `(100/账号/会话号/0/0/IP/0)`，其余报文吞掉。
///
/// 周期性重推的理由：`SM_QUERYCHR_FAIL` 那条分支会让 DBSrv 主动 `CloseSession`（C# 与 Rust 同），
/// 之后若不再推，后续用例会全部落到 `SM_OUTOFCONNECTION`（第一版就踩过）。
/// 返回 `(port, accepted 标志)`；`accepted` 置位表示 DBSrv 已连上（测试据此等会话就绪）。
async fn fake_login_srv(account: &'static str) -> (u16, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("伪 LoginSrv 绑定失败");
    let port = listener.local_addr().unwrap().port();
    let accepted = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&accepted);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let flag = Arc::clone(&flag);
            tokio::spawn(async move {
                let msg = format!("(100/{account}/{SESSION}/0/0/127.0.0.1/0)");
                let mut tick = tokio::time::interval(Duration::from_millis(300));
                let mut buf = [0u8; 256];
                loop {
                    tokio::select! {
                        _ = tick.tick() => {
                            if sock.write_all(msg.as_bytes()).await.is_err() {
                                break;
                            }
                            sock.flush().await.ok();
                            flag.store(true, Ordering::SeqCst);
                        }
                        r = sock.read(&mut buf) => {
                            // DBSrv 的 `(103/服务器名/99/在线数)` 保活：读了就丢
                            if !matches!(r, Ok(n) if n > 0) {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    (port, accepted)
}

/// 起 Rust 版 db-srv（cargo 集成测试里可直接拿到 bin 路径）。
///
/// 日志**重定向到夹具目录的文件**（不继承 stdout：子进程一旦成为孤儿会一直持有管道，
/// 让调用方的管线等不到 EOF；文件也便于事后当证据留档）。
fn start_rust_dbsrv(dir: &Path) -> Proc {
    let exe = env!("CARGO_BIN_EXE_db-srv");
    let child = Command::new(exe)
        .arg(dir.join("dbsvr.conf"))
        .current_dir(dir)
        .stdout(Stdio::from(log_file(dir, "dbsrv.out.log")))
        .stderr(Stdio::from(log_file(dir, "dbsrv.err.log")))
        .spawn()
        .expect("起 Rust db-srv 失败");
    eprintln!(
        "[fixture] Rust db-srv pid={} dir={}",
        child.id(),
        dir.display()
    );
    Proc(Some(child))
}

/// 起 C# 版 DBSrv（目录必须含 `DBSrv.exe` 与 `DBSrv.Storage.MySQL.dll`）。
fn start_csharp_dbsrv(exe_dir: &Path) -> Proc {
    let exe = exe_dir.join("DBSrv.exe");
    assert!(exe.exists(), "找不到 {}", exe.display());
    let child = Command::new(&exe)
        .current_dir(exe_dir)
        .stdout(Stdio::from(log_file(exe_dir, "dbsrv.out.log")))
        .stderr(Stdio::from(log_file(exe_dir, "dbsrv.err.log")))
        .spawn()
        .expect("起 C# DBSrv 失败");
    eprintln!(
        "[fixture] C# DBSrv pid={} dir={}",
        child.id(),
        exe_dir.display()
    );
    Proc(Some(child))
}

/// 起 C# 版 SelGate（配置文件已指向被测 DBSrv 的 5100 面端口）。
fn start_csharp_selgate(exe_dir: &Path) -> Proc {
    let exe = exe_dir.join("SelGate.exe");
    assert!(exe.exists(), "找不到 {}", exe.display());
    let child = Command::new(&exe)
        .current_dir(exe_dir)
        .stdout(Stdio::from(log_file(exe_dir, "selgate.out.log")))
        .stderr(Stdio::from(log_file(exe_dir, "selgate.err.log")))
        .spawn()
        .expect("起 C# SelGate 失败");
    eprintln!(
        "[fixture] C# SelGate pid={} dir={}",
        child.id(),
        exe_dir.display()
    );
    Proc(Some(child))
}

/// 打开夹具目录里的日志文件（截断写）。
fn log_file(dir: &Path, name: &str) -> std::fs::File {
    std::fs::File::create(dir.join(name)).expect("建日志文件失败")
}

/// 子进程守卫：`Drop` 时杀掉并回收 —— 断言失败/超时 panic 时也不会留孤儿进程与僵尸。
struct Proc(Option<Child>);

/// 把夹具目录里两侧子进程的日志尾部打进测试输出 —— 验收①要求"给日志/输出"，
/// 这样 C# SelGate 与 Rust db-srv 的时间线就和断言结果在同一条证据里。
fn dump_child_logs(dir: &Path, tag: &str) {
    for name in [
        "dbsrv.out.log",
        "dbsrv.err.log",
        "selgate.out.log",
        "selgate.err.log",
    ] {
        let Ok(text) = std::fs::read_to_string(dir.join(name)) else {
            continue;
        };
        let tail: Vec<&str> = text.lines().rev().take(200).collect();
        println!("---- [{tag}] {name}（尾部 {} 行） ----", tail.len());
        for line in tail.into_iter().rev() {
            println!("{line}");
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        if let Some(mut c) = self.0.take() {
            c.kill().ok();
            c.wait().ok();
        }
    }
}

/// 等 `gate_port` 开始监听（最多 10s）。
async fn wait_listen(port: u16) {
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("等端口 {port} 监听超时（看两侧日志）");
}

/// 等伪 LoginSrv 报告 DBSrv 已连上（最多 15s）。
async fn wait_session_link(flag: &AtomicBool) {
    for _ in 0..150 {
        if flag.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(200)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("DBSrv 未连上伪 LoginSrv（看 stdout 里 DBSrv 的日志）");
}

// ==== 伪 SelGate ====

/// 连 DBSrv 的 5100 面并发 `Enter`（C# SelGate `UserEnter` 的报文形态）。
async fn pretend_selgate(port: u16, socket_id: &str) -> TcpStream {
    let mut s = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("连 DBSrv 5100 面失败");
    s.set_nodelay(true).ok();
    let ip = "127.0.0.1";
    let enter = ServerDataMessage::new(
        ServerDataType::Enter,
        socket_id,
        format!("%K{socket_id}/{ip}/{ip}$").into_bytes(),
    );
    s.write_all(&enter.to_wire()).await.expect("发 Enter 失败");
    s.flush().await.ok();
    s
}

/// 伪 SelGate 发一条客户端帧（作为 `Data` 透传）并收一条下行业务帧。
async fn gate_roundtrip(sock: &mut TcpStream, socket_id: &str, frame: &[u8]) -> Vec<u8> {
    let msg = ServerDataMessage::new(ServerDataType::Data, socket_id, frame.to_vec());
    sock.write_all(&msg.to_wire()).await.expect("发 Data 失败");
    sock.flush().await.ok();
    let mut head = [0u8; ServerDataPacket::SIZE];
    tokio::time::timeout(Duration::from_secs(10), sock.read_exact(&mut head))
        .await
        .expect("等下行业务帧超时（限频？会话校验失败？）")
        .expect("读内部帧头失败");
    let pkt = ServerDataPacket::from_bytes(&head).expect("内部帧头非法");
    let mut body = vec![0u8; usize::from(pkt.packet_len)];
    sock.read_exact(&mut body).await.expect("读内部帧体失败");
    let msg = ServerDataMessage::from_bytes(&body).expect("内部帧体非法");
    assert_eq!(msg.kind, ServerDataType::Data, "不是透传帧：{msg:?}");
    msg.data
}

/// 造一条客户端帧（`#1` + 编码头 + 编码体 + `!`）。
///
/// 注意口径：`encode_client_frame` 自己会**分别编码**头与体，所以这里传的是**明文体**
/// （传已编码的体会被二次编码 —— 第一版就踩过：帧能发出但服务端解不出会话号）。
fn client_frame(ident: u16, body: &[u8]) -> Vec<u8> {
    mir2_protocol::frame::encode_client_frame(
        &CommandMessage::make(i32::from(ident), 0, 0, 0, 0),
        body,
    )
}

/// 解析一帧下行 `#…!`（DBSrv → 客户端方向）。
fn parse_down(frame: &[u8]) -> (CommandMessage, Vec<u8>) {
    assert_eq!(frame.first(), Some(&b'#'), "下行帧不是 '#' 开头");
    assert_eq!(frame.last(), Some(&b'!'), "下行帧不是 '!' 结尾");
    let content = &frame[1..frame.len() - 1];
    let head = CommandMessage::from_bytes(&edcode::decode(&content[..16])).expect("下行头不足 16");
    (head, edcode::decode(&content[16..]))
}

/// `SM_STARTPLAY` 下行帧的期望字节（帧布局由 `crates/protocol` 冻结，路由体按夹具常量）。
fn expected_startplay_frame() -> Vec<u8> {
    encode_server_frame(
        &CommandMessage::make(i32::from(SM_STARTPLAY), 0, 0, 0, 0),
        &edcode::encode(EXPECT_STARTPLAY_BODY),
    )
}

/// 走一遍 5100 面的四个命令并逐条断言（两种实现都必须给出同一组字节/字段）。
async fn exercise_5100_face(port: u16) {
    // 1) CM_QUERYCHR #1：先只核字段（`*` 落在哪个角色取决于库里 `SelectID`——共享可变状态）。
    let mut sock = pretend_selgate(port, "7").await;
    let frame = client_frame(CM_QUERYCHR, format!("{ACCOUNT}/{SESSION}").as_bytes());
    let down = gate_roundtrip(&mut sock, "7", &frame).await;
    let (head, chr_list) = parse_down(&down);
    println!(
        "[querychr#1] head={head:?} body={:?}\n  raw={}",
        String::from_utf8_lossy(&chr_list),
        hex(&down)
    );
    assert_eq!(head.ident, SM_QUERYCHR, "不是 SM_QUERYCHR：{head:?}");
    assert_eq!(head.recog, 2, "角色数应为 2（aaa+bbb）");
    assert_eq!((head.param, head.tag, head.series), (0, 1, 0));
    let bare: Vec<u8> = chr_list.iter().copied().filter(|b| *b != b'*').collect();
    assert_eq!(bare, EXPECT_CHR_LIST_BARE.to_vec(), "字段级内容不符");

    // 1b) CM_SELCHR：进选角前必须已查过角色（C# `boChrQueryed`）⇒ 回 `SM_STARTPLAY`(525)、
    //     体为 `游戏网关IP/端口+MapIndex`。**本卡修的缺陷**：Rust 之前 `query_chr` 成功不置
    //     `chr_queryed` ⇒ 这里回 `SM_STARTFAIL` 且日志打 `Double send SELCHR`。
    //     选角会写 `characters_indexes.SelectID`（`aaa` 选中、其余置 0）⇒ 顺带把库归一到金标准态。
    let sel = client_frame(CM_SELCHR, format!("{ACCOUNT}/aaa").as_bytes());
    let down = gate_roundtrip(&mut sock, "7", &sel).await;
    let (head, route) = parse_down(&down);
    println!(
        "[selchr] head={head:?} body={:?}\n  raw={}",
        String::from_utf8_lossy(&route),
        hex(&down)
    );
    assert_eq!(head.ident, SM_STARTPLAY, "选角应回 SM_STARTPLAY：{head:?}");
    assert_eq!(
        (head.recog, head.param, head.tag, head.series),
        (0, 0, 0, 0)
    );
    assert_eq!(route, EXPECT_STARTPLAY_BODY.to_vec(), "选角路由体不符");
    assert_eq!(
        down,
        expected_startplay_frame(),
        "SM_STARTPLAY 下行帧字节与冻结布局不符"
    );

    // 1c) CM_QUERYCHR #2：选角已把 `SelectID` 归一到金标准态（`aaa` 选中）⇒ 再逐字节对金标准。
    //     过 200ms 限频（`CM_QUERYCHR` 的 `dwChrTick`）即可重查。
    tokio::time::sleep(Duration::from_millis(250)).await;
    let down = gate_roundtrip(&mut sock, "7", &frame).await;
    let (head, chr_list) = parse_down(&down);
    println!(
        "[querychr#2] head={head:?} body={:?}\n  raw={}",
        String::from_utf8_lossy(&chr_list),
        hex(&down)
    );
    assert_eq!(head.ident, SM_QUERYCHR);
    assert_eq!(
        chr_list,
        EXPECT_CHR_LIST.to_vec(),
        "字段级内容不符（归一化后）"
    );
    assert!(
        GOLDEN_QUERYCHR_FRAME_HEX != "PLACEHOLDER",
        "金标准未填：把上面 `raw=` 的值填进 GOLDEN_QUERYCHR_FRAME_HEX"
    );
    assert_eq!(
        hex(&down),
        GOLDEN_QUERYCHR_FRAME_HEX,
        "下行帧与 C# oracle 金标准字节不等"
    );

    // 2) CM_NEWCHR 的三条**不写库**拒绝分支：0 名字太短 / 2 重名 / 3 账号已满 2 个。
    //    必须**复用同一个会话**（socket 7）：C# 侧 `userInfo.sAccount` 只在 `QueryChr` 成功时填，
    //    新连接上的 NEWCHR 会因 `sAccount` 为空直接回 `SM_OUTOFCONNECTION`（第一版就踩过）。
    for (name, want) in [(&b"ab"[..], 0i32), (&b"aaa"[..], 2), (&b"zzz"[..], 3)] {
        tokio::time::sleep(Duration::from_millis(1100)).await; // 过 NEWCHR 的 1000ms 限频
        let mut body = format!("{ACCOUNT}/").into_bytes();
        body.extend_from_slice(name);
        body.extend_from_slice(b"/1/2/0");
        let new = client_frame(CM_NEWCHR, &body);
        let (head, _) = parse_down(&gate_roundtrip(&mut sock, "7", &new).await);
        println!("[newchr {}] head={head:?}", String::from_utf8_lossy(name));
        assert_eq!(head.ident, SM_NEWCHR_FAIL, "建角应当是失败");
        assert_eq!(head.recog, want, "失败码不符");
    }

    // 3) CM_DELCHR：账号不匹配 ⇒ C# 只回 SUCCESS、**不写库**（bocheck 语义，1:1 复刻）
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let del = client_frame(CM_DELCHR, b"gm01"); // 属于 admin，不属于 mir2test
    let (head, _) = parse_down(&gate_roundtrip(&mut sock, "7", &del).await);
    println!("[delchr gm01] head={head:?}");
    assert_eq!(head.ident, SM_DELCHR_SUCCESS);

    // 4) 会话校验失败：伪会话号 ⇒ SM_QUERYCHR_FAIL（且该登录会话被摘掉；放最后，免影响前面用例）
    let mut sock2 = pretend_selgate(port, "8").await;
    let bad = client_frame(CM_QUERYCHR, format!("{ACCOUNT}/99999999").as_bytes());
    let (head, _) = parse_down(&gate_roundtrip(&mut sock2, "8", &bad).await);
    println!("[querychr-bad-session] head={head:?}");
    assert_eq!(head.ident, SM_QUERYCHR_FAIL);
    assert_eq!(
        (head.recog, head.param, head.tag, head.series),
        (0, 0, 1, 0)
    );
}

// ==== 三个场景 ====

#[tokio::test]
#[ignore = "需要本机 MySQL(mir2_db)，会起真正的 db-srv 进程"]
async fn rust_dbsrv_querychr_matches_csharp_golden() {
    let dir = fixture_dir("rust");
    let gate_port = free_port();
    let (ids_port, accepted) = fake_login_srv(ACCOUNT).await;
    write_conf(&dir, gate_port, ids_port, false);
    let dbsrv = start_rust_dbsrv(&dir);
    wait_listen(gate_port).await;
    wait_session_link(&accepted).await;
    exercise_5100_face(gate_port).await;
    dump_child_logs(&dir, "dbsrv");
    // 先收子进程（否则它仍持有夹具目录里的日志文件句柄 ⇒ Windows 上删目录会失败留残）
    drop(dbsrv);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
#[ignore = "需要本机 MySQL(mir2_db) + MIR2_ORACLE_DBSRV_DIR（C# DBSrv 产物目录）"]
async fn csharp_dbsrv_querychr_matches_csharp_golden() {
    let oracle_dir = PathBuf::from(env_or(
        "MIR2_ORACLE_DBSRV_DIR",
        "E:\\tmp\\a2-dbsrv\\oracle-dbsrv",
    ));
    assert!(
        oracle_dir.join("DBSrv.exe").exists(),
        "缺 MIR2_ORACLE_DBSRV_DIR（C# DBSrv 构建产物副本目录）：{}",
        oracle_dir.display()
    );
    // 影子副本：把 C# 产物复制到 temp 再改端口，绝不动原目录
    let dir = fixture_dir("csharp");
    copy_dir(&oracle_dir, &dir);
    let gate_port = free_port();
    let (ids_port, accepted) = fake_login_srv(ACCOUNT).await;
    write_conf(&dir, gate_port, ids_port, false);
    let dbsrv = start_csharp_dbsrv(&dir);
    wait_listen(gate_port).await;
    wait_session_link(&accepted).await;
    exercise_5100_face(gate_port).await;
    dump_child_logs(&dir, "dbsrv");
    // 先收子进程（否则它仍持有夹具目录里的日志文件句柄 ⇒ Windows 上删目录会失败留残）
    drop(dbsrv);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
#[ignore = "需要本机 MySQL(mir2_db) + MIR2_ORACLE_SELGATE_DIR（C# SelGate 产物目录）"]
async fn csharp_selgate_to_rust_dbsrv() {
    let selgate_dir = PathBuf::from(env_or(
        "MIR2_ORACLE_SELGATE_DIR",
        "E:\\tmp\\a2-dbsrv\\oracle-selgate",
    ));
    assert!(
        selgate_dir.join("SelGate.exe").exists(),
        "缺 MIR2_ORACLE_SELGATE_DIR（C# SelGate 产物副本目录）：{}",
        selgate_dir.display()
    );
    // 被测：Rust db-srv（默认占 5100 —— 与验收①「Rust DBSrv(5100)」一致）
    let dbsrv_dir = fixture_dir("rust-selgate");
    let gate_port = match std::env::var("MIR2_DBSRV_PORT") {
        Ok(p) => p.parse::<u16>().expect("MIR2_DBSRV_PORT 非法"),
        Err(_) => 5100,
    };
    assert!(
        port_is_free(gate_port),
        "端口 {gate_port} 已被占用（并行 agent 的 C# DBSrv？）：本用例要求独占 5100，稍后重跑"
    );
    // C# SelGate 的 listen 端口是**编译期常量**（`SelGate/GateShare.cs: GatePort = 7100`），
    // 影子副本也改不了 ⇒ 本用例要求 7100 空闲。
    let gate_listen = 7100u16;
    assert!(
        port_is_free(gate_listen),
        "端口 {gate_listen} 已被占用（并行 agent 的 C# SelGate？）：本用例要求独占 7100，稍后重跑"
    );
    let (ids_port, accepted) = fake_login_srv(ACCOUNT).await;
    write_conf(&dbsrv_dir, gate_port, ids_port, true);
    let dbsrv = start_rust_dbsrv(&dbsrv_dir);
    wait_listen(gate_port).await;
    wait_session_link(&accepted).await;

    // C# SelGate 影子副本：只把监听口换成临时端口、下游指向被测 db-srv
    let sel_dir = fixture_dir("selgate");
    copy_dir(&selgate_dir, &sel_dir);
    let conf = format!(
        "[SelGate]\nTitle=热血传奇\nCount=1\nServerAddr0=127.0.0.1\nServerPort0={gate_port}\n\
         GatePort0={gate_listen}\n\nShowLogLevel=1\nShowDebugLog=1\nKeepConnectTimeOut=60000\n\
         LsvServer=127.0.0.1\nLsvServerPort={}\n",
        free_port()
    );
    std::fs::write(sel_dir.join("config.conf"), conf).expect("写 SelGate config.conf 失败");
    let selgate = start_csharp_selgate(&sel_dir);
    wait_listen(gate_listen).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 客户端侧：`#1…!` 帧经 C# SelGate 中继到 Rust db-srv
    let mut s = TcpStream::connect(("127.0.0.1", gate_listen))
        .await
        .expect("连 C# SelGate 失败");
    s.set_nodelay(true).ok();
    s.write_all(&client_frame(
        CM_QUERYCHR,
        format!("{ACCOUNT}/{SESSION}").as_bytes(),
    ))
    .await
    .expect("发客户端帧失败");
    s.flush().await.ok();
    let (head, chr_list, frame) = read_one_client_frame(&mut s).await;
    println!(
        "[csharp-selgate→rust-dbsrv] head={head:?}\n  body={:?}\n  raw={}",
        String::from_utf8_lossy(&chr_list),
        hex(&frame)
    );
    assert_eq!(head.ident, SM_QUERYCHR);
    assert_eq!(head.recog, 2);
    assert_eq!(chr_list, EXPECT_CHR_LIST.to_vec());
    assert!(
        GOLDEN_QUERYCHR_FRAME_HEX != "PLACEHOLDER",
        "金标准未填（先跑前两个场景）"
    );
    assert_eq!(
        hex(&frame),
        GOLDEN_QUERYCHR_FRAME_HEX,
        "经 SelGate 的字节与金标准不等"
    );

    after_keepalive_roundtrip(&mut s, &dbsrv_dir).await;

    dump_child_logs(&sel_dir, "selgate");
    dump_child_logs(&dbsrv_dir, "dbsrv");
    drop(selgate);
    std::fs::remove_dir_all(&sel_dir).ok();
    drop(dbsrv);
    std::fs::remove_dir_all(&dbsrv_dir).ok();
}

/// 长连接面：C# SelGate 每 10s 往上游发一次 KeepAlive（`TimedService.CheckSessionStatus → SendKeepAlive`），
/// 等过一个心跳周期后再查一次角色列表 —— 既证明链路不被心跳打断，也把心跳帧的解析暴露到日志里。
///
/// 心跳帧是 **MemoryPack null**（`SocketId`/`Data` 均 null ⇒ `04 03 ffffffff 0000 ffffffff`），
/// 必须被解析并应答（`响应角色网关心跳...`）；修前那一版这里会出现"解析角色网关封包出现异常..."且不应答。
async fn after_keepalive_roundtrip(s: &mut TcpStream, dbsrv_dir: &Path) {
    tokio::time::sleep(Duration::from_secs(12)).await;
    s.write_all(&client_frame(
        CM_QUERYCHR,
        format!("{ACCOUNT}/{SESSION}").as_bytes(),
    ))
    .await
    .expect("二次发客户端帧失败");
    s.flush().await.ok();
    let (head, chr_list, frame) = read_one_client_frame(s).await;
    println!(
        "[csharp-selgate→rust-dbsrv][+12s] head={head:?} body={:?}",
        String::from_utf8_lossy(&chr_list)
    );
    assert_eq!(head.ident, SM_QUERYCHR, "心跳周期后链路失效");
    assert_eq!(chr_list, EXPECT_CHR_LIST.to_vec());
    assert_eq!(hex(&frame), GOLDEN_QUERYCHR_FRAME_HEX);
    let log = std::fs::read_to_string(dbsrv_dir.join("dbsrv.out.log")).unwrap_or_default();
    assert!(
        log.contains("响应角色网关心跳..."),
        "DBSrv 没有应答 SelGate 心跳（证据目录：{}）",
        dbsrv_dir.display()
    );
    assert!(
        !log.contains("解析角色网关封包出现异常"),
        "心跳帧仍被当成坏帧丢弃（证据目录：{}）",
        dbsrv_dir.display()
    );
}

/// 端口是否空闲（试着绑一次再放开）。
fn port_is_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// 读一帧服务端下行 `#…!`（含 `!$` 形态）。
async fn read_one_client_frame(s: &mut TcpStream) -> (CommandMessage, Vec<u8>, Vec<u8>) {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        if let Some(pos) = buf.iter().position(|b| *b == b'!') {
            let end = if buf.get(pos + 1) == Some(&b'$') {
                pos + 2
            } else {
                pos + 1
            };
            let frame = buf[..end].to_vec();
            let (head, body) = parse_down(&frame);
            return (head, body, frame);
        }
        let n = tokio::time::timeout(Duration::from_secs(10), s.read(&mut tmp))
            .await
            .expect("等客户端下行超时")
            .expect("读客户端连接失败");
        assert!(n != 0, "对端关闭了连接（buf={buf:02X?}）");
        buf.extend_from_slice(&tmp[..n]);
    }
}
