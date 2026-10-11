//! **进程级验收**（M1 S4b ③/④）：用「伪 LoginSrv + 伪 GameSvr」把 Rust db-srv 的 **5700 `MarketService` 面**
//! 单独架起来跑，对**独立临时库**（非 `mir2_db`）验证拍卖行四个 ident 的应答/不回包行为。
//!
//! 覆盖点（逐条对齐 C# `DBSrv/Services/Impl/MarketService.cs`）：
//! - 信封：6B `ServerDataPacket` + MemoryPack `ServerRequestData`；校验码 `MakeLong(queryId^170, len)`；
//! - 5700 与 6000 的**校验码语义分叉**：`packetLen<16`/`Sign` 缺失 ⇒ 5700 回 `DBR_FAIL`(2000)；
//! - `DB_LOADMARKET`：空表 ⇒ `DB_LOADMARKETFAIL`(106) `param=1`；有行 ⇒ C# `MarketItem` 未注册
//!   MemoryPack ⇒ 序列化抛 ⇒ **不回包**（本实现同）；
//! - `DB_SEARCHMARKET`：恒 `DB_SRARCHMARKETFAIL`(104)（`GroupId==0` `param=0`，否则 `param=1`）；
//! - `DB_LOADUSERMARKET`：`GroupId!=0` ⇒ `DB_LOADUSERMARKETSUCCESS`(110)（查询号**恒 1**）+ 12B 体；
//! - `DB_SAVEMARKET` / 未知 ident / 非法 Register：不回包；
//! - 签名不匹配 ⇒ 关连接（C# `client.Close()`）。
//!
//! 端口一律临时分配、库名唯一（`mir2_dbsrv6k_mktlive_<pid>`，跑完 DROP）—— 不动 `E:\MirServer`、
//! 不动共享 `mir2_db`、不与并行 agent 的现网端口打架。
//!
//! 跑法（需本机 MySQL）：
//! ```powershell
//! cargo test -p db-srv --test live_market --offline -- --ignored --nocapture
//! ```

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mir2_protocol::edcode;
use mir2_protocol::frame::ServerDataPacket;
use mir2_protocol::messages::grobal2::PacketCode;
use mir2_protocol::messages::{
    DBR_FAIL, DB_LOADMARKET, DB_LOADMARKETFAIL, DB_LOADUSERMARKET, DB_LOADUSERMARKETSUCCESS,
    DB_SAVEMARKET, DB_SEARCHMARKET, DB_SRARCHMARKETFAIL,
};
use mir2_storage::playerdata::{MemPack, ServerRequestData, ServerRequestMessage};
use sqlx::MySqlPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const ACCOUNT: &str = "mir2test6k";

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("取空闲端口失败");
    l.local_addr().expect("取本地地址失败").port()
}

fn scratch_db() -> String {
    format!("mir2_dbsrv6k_mktlive_{}", std::process::id())
}

fn admin_pool_url() -> String {
    mir2_storage::LegacyDsn::parse("server=127.0.0.1;uid=root;pwd=;database=mysql;")
        .expect("解析 DSN 失败")
        .to_sqlx_url()
}

fn fixture_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("a2-dbsrv-5700-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("建 temp 目录失败");
    std::fs::write(d.join("AddrTable.txt"), "127.0.0.1\n").expect("写 AddrTable.txt 失败");
    std::fs::write(d.join("ServerInfo.txt"), "127.0.0.1 127.0.0.1 7200\n")
        .expect("写 ServerInfo.txt 失败");
    d
}

fn write_conf(
    dir: &Path,
    gate_port: u16,
    server_port: u16,
    ids_port: u16,
    market_port: u16,
    db: &str,
) {
    let body = format!(
        "[Setup]\nShowDebug=1\nServerAddr=127.0.0.1\nServerPort={server_port}\nGateAddr=0.0.0.0\nGatePort={gate_port}\n\
         ServerName=LegendOfMir2\nDenyChrName=1\nDELMaxLevel=30\nDynamicIPMode=0\n\n\
         [Server]\nIDSAddr=127.0.0.1\nIDSPort={ids_port}\nMarketAddr=127.0.0.1\nMarketPort={market_port}\n\n\
         [DataBase]\nStoreage=MySQL\nConnctionString=server=127.0.0.1;uid=root;pwd=;database={db};\n\n\
         [DBClear]\nInterval=3000\n"
    );
    std::fs::write(dir.join("dbsvr.conf"), body).expect("写 dbsvr.conf 失败");
}

fn log_file(dir: &Path, name: &str) -> std::fs::File {
    std::fs::File::create(dir.join(name)).expect("建日志文件失败")
}

struct Proc(Option<Child>);

impl Drop for Proc {
    fn drop(&mut self) {
        if let Some(mut c) = self.0.take() {
            c.kill().ok();
            c.wait().ok();
        }
    }
}

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

async fn fake_login_srv() -> (u16, Arc<AtomicBool>) {
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
                let msg = format!("(100/{ACCOUNT}/6000/0/0/127.0.0.1/0)");
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

async fn wait_listen(port: u16) {
    for _ in 0..120 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("等端口 {port} 监听超时");
}

async fn wait_session_link(flag: &AtomicBool) {
    for _ in 0..150 {
        if flag.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // MarketService 不依赖会话链；未连上也继续（仅提示）。
    eprintln!("[warn] db-srv 未连上伪 LoginSrv（5700 面不依赖它）");
}

// ==== 5700 信封（C# GameSvr `MarketService.SendRequest<T>` 形态） ====

fn low16(v: i32) -> u16 {
    u16::try_from(v & 0xFFFF).unwrap_or(0)
}

fn make_long(low: u16, high: u16) -> i32 {
    (u32::from(low) | (u32::from(high) << 16)).cast_signed()
}

fn build_request(query_id: i32, ident: u16, recog: i32, body: &[u8]) -> Vec<u8> {
    let message = edcode::encode(&ServerRequestMessage::new(ident, recog, 0, 0, 0).to_bytes());
    let packet = edcode::encode(body);
    let len = message.len() + packet.len() + ServerDataPacket::SIZE;
    let check = make_long(low16(query_id ^ 0xAA), u16::try_from(len).unwrap());
    let sign = edcode::encode(&check.to_le_bytes());
    let request = ServerRequestData {
        query_id,
        message: Some(message),
        packet: Some(packet),
        sign: Some(sign),
    };
    frame(&request.to_bytes())
}

fn frame(body_bytes: &[u8]) -> Vec<u8> {
    let header = ServerDataPacket {
        packet_code: PacketCode,
        packet_len: u16::try_from(body_bytes.len()).unwrap(),
    };
    let mut wire = header.to_bytes().to_vec();
    wire.extend_from_slice(body_bytes);
    wire
}

/// 造一条**签名错误**的请求（校验码低 16 位改 1 位）。
fn build_request_bad_sign(query_id: i32, ident: u16, body: &[u8]) -> Vec<u8> {
    let message = edcode::encode(&ServerRequestMessage::new(ident, 0, 0, 0, 0).to_bytes());
    let packet = edcode::encode(body);
    let len = message.len() + packet.len() + ServerDataPacket::SIZE;
    let check = make_long(low16(query_id ^ 0xAA), u16::try_from(len).unwrap()) ^ 1;
    let sign = edcode::encode(&check.to_le_bytes());
    let request = ServerRequestData {
        query_id,
        message: Some(message),
        packet: Some(packet),
        sign: Some(sign),
    };
    frame(&request.to_bytes())
}

/// 读一条应答，返回 `(ServerRequestMessage, packet 体)`。
async fn read_reply(sock: &mut TcpStream) -> (ServerRequestMessage, Option<Vec<u8>>) {
    let mut head = [0u8; ServerDataPacket::SIZE];
    tokio::time::timeout(Duration::from_secs(10), sock.read_exact(&mut head))
        .await
        .expect("等 5700 应答超时")
        .expect("读 5700 应答头失败");
    let pkt = ServerDataPacket::from_bytes(&head).expect("5700 应答头非法");
    assert_eq!(pkt.packet_code, PacketCode);
    let mut body = vec![0u8; usize::from(pkt.packet_len)];
    sock.read_exact(&mut body)
        .await
        .expect("读 5700 应答体失败");
    let resp = ServerRequestData::from_bytes(&body).expect("5700 应答信封非法");
    let msg = ServerRequestMessage::from_bytes(&edcode::decode(resp.message.as_ref().unwrap()))
        .expect("5700 应答消息头非法");
    let decoded = resp.packet.as_ref().map(|p| edcode::decode(p));
    (msg, decoded)
}

/// 发一条请求并读回应答。
async fn roundtrip(
    sock: &mut TcpStream,
    query_id: i32,
    ident: u16,
    body: &[u8],
) -> (ServerRequestMessage, Option<Vec<u8>>) {
    sock.write_all(&build_request(query_id, ident, 0, body))
        .await
        .expect("发 5700 请求失败");
    sock.flush().await.ok();
    read_reply(sock).await
}

/// 断言「不回包」：500ms 内读不到任何字节，且连接**未**被关闭。
async fn expect_no_reply(sock: &mut TcpStream, label: &str) {
    let mut buf = [0u8; 64];
    match tokio::time::timeout(Duration::from_millis(500), sock.read(&mut buf)).await {
        Err(_) => {}
        Ok(Ok(0)) => panic!("{label}: 连接被关闭（期望保持且不回包）"),
        Ok(Ok(n)) => panic!("{label}: 收到 {n} 字节（期望不回包）"),
        Ok(Err(e)) => panic!("{label}: 读失败 {e}"),
    }
}

// ==== 市场消息 MemoryPack 编码（测试侧最小实现，与 C# 真身声明序一致） ====

fn push_str(out: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.extend_from_slice(&i32::try_from(units.len()).unwrap().to_le_bytes());
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
}

fn market_register(server_index: u8, server_name: &str, group_id: u8, token: &str) -> Vec<u8> {
    let mut o = vec![4u8, server_index];
    push_str(&mut o, server_name);
    o.push(group_id);
    push_str(&mut o, token);
    o
}

fn market_search(group_id: u8, market_npc: i32) -> Vec<u8> {
    let mut o = vec![10u8, 1, group_id];
    push_str(&mut o, "u");
    push_str(&mut o, "Market");
    push_str(&mut o, "w");
    push_str(&mut o, "i");
    o.extend_from_slice(&(-1_i16).to_le_bytes());
    o.push(0);
    o.extend_from_slice(&1_i32.to_le_bytes());
    o.extend_from_slice(&market_npc.to_le_bytes());
    o
}

// ==== 临时库夹具 ====

async fn setup_scratch(admin: &MySqlPool, db: &str) {
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {db}"))
        .execute(admin)
        .await;
    sqlx::query(&format!("CREATE DATABASE {db}"))
        .execute(admin)
        .await
        .expect("建临时库失败");
    let dsn =
        mir2_storage::LegacyDsn::parse(&format!("server=127.0.0.1;uid=root;pwd=;database={db};"))
            .unwrap();
    let pool = MySqlPool::connect(&dsn.to_sqlx_url())
        .await
        .expect("连临时库失败");
    for sql in [
        "CREATE TABLE characters_indexes (Id INT AUTO_INCREMENT PRIMARY KEY, Account VARCHAR(64), \
         ChrName VARCHAR(64), SelectID INT, IsDeleted INT)",
        "CREATE TABLE characters (Id INT AUTO_INCREMENT PRIMARY KEY, LoginID VARCHAR(64), \
         ChrName VARCHAR(64), Deleted INT)",
        "CREATE TABLE marketitems (Id INT AUTO_INCREMENT PRIMARY KEY, GroupId INT NOT NULL, \
         ServerIndex INT NOT NULL, SellIndex INT NOT NULL, SellState INT NOT NULL, MarketName VARCHAR(64), \
         ItemType SMALLINT, ItemSet SMALLINT, ItemName VARCHAR(64), MakeIndex INT, SellName VARCHAR(64), \
         SellPrice INT, SellDate VARCHAR(32))",
        "CREATE TABLE marketitem (MarketId INT NOT NULL, MaekeIndex INT, Dura SMALLINT, DuraMax SMALLINT, \
         Value0 TINYINT, Value1 TINYINT, Value2 TINYINT, Value3 TINYINT, Value4 TINYINT, Value5 TINYINT, \
         Value6 TINYINT, Value7 TINYINT, Value8 TINYINT, Value9 TINYINT, Value10 TINYINT, Value11 TINYINT, \
         Value12 TINYINT)",
    ] {
        sqlx::query(sql).execute(&pool).await.expect("建表失败");
    }
}

async fn seed_market_row(db: &str) {
    let dsn =
        mir2_storage::LegacyDsn::parse(&format!("server=127.0.0.1;uid=root;pwd=;database={db};"))
            .unwrap();
    let pool = MySqlPool::connect(&dsn.to_sqlx_url()).await.unwrap();
    sqlx::query(
        "INSERT INTO marketitems (Id, GroupId, ServerIndex, SellIndex, SellState, MarketName, ItemType, \
         ItemSet, ItemName, MakeIndex, SellName, SellPrice, SellDate) VALUES \
         (1, 2, 1, 11, 1, 'Market', 0, 0, '', 300, 'seller', 1000, '0312311210')",
    )
    .execute(&pool)
    .await
    .expect("种 marketitems 失败");
    sqlx::query("INSERT INTO marketitem (MarketId, MaekeIndex, Dura, DuraMax, Value0, Value1, Value2, Value3, Value4, Value5, Value6, Value7, Value8, Value9, Value10, Value11, Value12) VALUES (1, 700, 5, 6, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8)")
        .execute(&pool)
        .await
        .expect("种 marketitem 失败");
}

#[tokio::test]
#[ignore = "需要本机 MySQL，会起真正的 db-srv 进程（独立临时库 mir2_dbsrv6k_mktlive_*）"]
#[allow(clippy::too_many_lines)]
async fn rust_dbsrv_5700_market_faces_csharp_behaviour() {
    let admin = MySqlPool::connect(&admin_pool_url())
        .await
        .expect("连 MySQL(admin) 失败");
    let db = scratch_db();
    setup_scratch(&admin, &db).await;

    let dir = fixture_dir("market");
    let (ids_port, accepted) = fake_login_srv().await;
    let server_port = free_port();
    let market_port = free_port();
    let gate_port = free_port();
    write_conf(&dir, gate_port, server_port, ids_port, market_port, &db);

    let dbsrv = start_rust_dbsrv(&dir);
    wait_listen(market_port).await;
    wait_session_link(&accepted).await;

    let mut sock = TcpStream::connect(("127.0.0.1", market_port))
        .await
        .expect("连 5700 面失败");
    sock.set_nodelay(true).ok();

    // 1) DB_SEARCHMARKET：GroupId==0 ⇒ 104 recog=0 param=0，查询号回显。
    let (msg, body) = roundtrip(
        &mut sock,
        7,
        u16::from(DB_SEARCHMARKET),
        &market_search(0, 0),
    )
    .await;
    assert_eq!(msg.ident, u16::from(DB_SRARCHMARKETFAIL));
    assert_eq!((msg.recog, msg.param), (0, 0), "GroupId==0 分支");
    // C# `SendFailMessage` 的 `Packet` 经 `SendRequest` 补成 `Array.Empty<byte>()` ⇒ 线缆上是非 null 空体。
    assert!(body.unwrap_or_default().is_empty(), "失败应答体应为空");
    println!("[search gid=0] {msg:?}");

    // 2) DB_SEARCHMARKET：GroupId!=0 ⇒ 104 param=1（Search* 全空桩）。
    let (msg, _) = roundtrip(
        &mut sock,
        8,
        u16::from(DB_SEARCHMARKET),
        &market_search(2, 0),
    )
    .await;
    assert_eq!(msg.ident, u16::from(DB_SRARCHMARKETFAIL));
    assert_eq!((msg.recog, msg.param), (0, 1), "非 0 组分支");
    println!("[search gid=2] {msg:?}");

    // 3) DB_LOADUSERMARKET：GroupId==0 ⇒ 104 param=0。
    let (msg, _) = roundtrip(
        &mut sock,
        9,
        u16::from(DB_LOADUSERMARKET),
        &market_search(0, 7),
    )
    .await;
    assert_eq!(msg.ident, u16::from(DB_SRARCHMARKETFAIL));
    assert_eq!((msg.recog, msg.param), (0, 0));

    // 4) DB_LOADUSERMARKET：GroupId!=0 ⇒ 110（查询号**恒 1**，recog=请求 recog）+ 12B MarkerUserLoadMessage。
    let req = build_request(10, u16::from(DB_LOADUSERMARKET), 99, &market_search(2, 7));
    sock.write_all(&req).await.unwrap();
    sock.flush().await.ok();
    let (msg, body) = read_reply(&mut sock).await;
    assert_eq!(msg.ident, u16::from(DB_LOADUSERMARKETSUCCESS));
    assert_eq!(
        (msg.recog, msg.param),
        (99, 0),
        "SendSuccessMessage 回显 actorId"
    );
    // MarkerUserLoadMessage：全非托管 struct ⇒ 原始 12B（SellCount, MarketNPC, IsBusy + 3 pad）。
    assert_eq!(
        body.expect("个人拍卖行应答缺体"),
        vec![0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0],
        "SellCount=0 MarketNPC=7 IsBusy=0"
    );
    println!("[user gid=2] {msg:?}");

    // 5) DB_LOADMARKET：非法 Register（Token/ServerName 空）⇒ 不回包。
    sock.write_all(&build_request(
        11,
        u16::from(DB_LOADMARKET),
        0,
        &market_register(1, "", 2, ""),
    ))
    .await
    .unwrap();
    sock.flush().await.ok();
    expect_no_reply(&mut sock, "load reject(empty token)").await;

    // 6) DB_LOADMARKET：合法 Register + 空表 ⇒ 106 recog=0 param=1。
    let (msg, _) = roundtrip(
        &mut sock,
        12,
        u16::from(DB_LOADMARKET),
        &market_register(1, "LegendOfMir2", 2, "tok"),
    )
    .await;
    assert_eq!(msg.ident, u16::from(DB_LOADMARKETFAIL));
    assert_eq!((msg.recog, msg.param), (0, 1));
    println!("[load empty] {msg:?}");

    // 7) 种一行在售 ⇒ DB_LOADMARKET 命中 ⇒ C# MarketItem 未注册 ⇒ 不回包。
    seed_market_row(&db).await;
    sock.write_all(&build_request(
        13,
        u16::from(DB_LOADMARKET),
        0,
        &market_register(1, "LegendOfMir2", 2, "tok"),
    ))
    .await
    .unwrap();
    sock.flush().await.ok();
    expect_no_reply(&mut sock, "load with items (C# serialize throws)").await;

    // 8) DB_SAVEMARKET：空对象头（GroupId=0）⇒ 不回包；含 Item 成员 ⇒ C# 反序列化抛 ⇒ 不回包。
    sock.write_all(&build_request(14, u16::from(DB_SAVEMARKET), 0, &[0x00]))
        .await
        .unwrap();
    sock.flush().await.ok();
    expect_no_reply(&mut sock, "save empty-object").await;
    sock.write_all(&build_request(
        15,
        u16::from(DB_SAVEMARKET),
        0,
        &[0x04, 0xff, 1, 0, 2, 0],
    ))
    .await
    .unwrap();
    sock.flush().await.ok();
    expect_no_reply(&mut sock, "save with item (C# deserialize throws)").await;

    // 9) 未知 ident ⇒ 不回包。
    sock.write_all(&build_request(16, 999, 0, &[0x00]))
        .await
        .unwrap();
    sock.flush().await.ok();
    expect_no_reply(&mut sock, "unknown ident").await;

    // 10) `Sign` 缺失（Message/Packet 均非空）⇒ DBR_FAIL(2000) param=0。
    let message = edcode::encode(
        &ServerRequestMessage::new(u16::from(DB_SEARCHMARKET), 0, 0, 0, 0).to_bytes(),
    );
    let packet = edcode::encode(&market_search(2, 0));
    let no_sign = ServerRequestData {
        query_id: 17,
        message: Some(message),
        packet: Some(packet),
        sign: None,
    };
    sock.write_all(&frame(&no_sign.to_bytes())).await.unwrap();
    sock.flush().await.ok();
    let (msg, _) = read_reply(&mut sock).await;
    assert_eq!(msg.ident, DBR_FAIL);
    assert_eq!((msg.recog, msg.param), (0, 0));
    println!("[no sign] {msg:?}");

    // 11) 签名不匹配 ⇒ 关连接（C# `client.Close()`）。
    sock.write_all(&build_request_bad_sign(
        18,
        u16::from(DB_SEARCHMARKET),
        &market_search(2, 0),
    ))
    .await
    .unwrap();
    sock.flush().await.ok();
    let mut buf = [0u8; 16];
    let r = tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf))
        .await
        .expect("等关连接超时");
    assert!(matches!(r, Ok(0)), "签名不匹配应关闭连接, got {r:?}");
    println!("[bad sign] connection closed");

    drop(dbsrv);
    // 收尾：删临时库。
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {db}"))
        .execute(&admin)
        .await;
    eprintln!("[fixture] scratch db dropped: {db}");
}
