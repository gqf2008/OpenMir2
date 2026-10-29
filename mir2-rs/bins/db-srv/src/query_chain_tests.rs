//! SelGate→DBSrv 查号链牙（卡 `openmir2-selgate-dbsrv-query-chain` A2）。
//!
//! 链路（C# oracle 行号）：`Enter` 开户（`UserService.cs:293-325`）→
//! `Data(CM_QUERYCHR)`（`DeCodeUserMsg`，`UserService.cs:349-361`）→
//! `CheckSession`（`UserService.cs:444`，会话表由 LoginSrv 经 `(100/账号/会话号/…)`
//! 推送，`loginsrv.rs::dispatch` 的 SS_OPENSESSION 分支）→ 命中 ⇒ `SM_QUERYCHR`(520)
//! （`UserService.cs:470-474`）/ 不符 ⇒ `SM_QUERYCHR_FAIL`(527) + `CloseUser`
//! （`UserService.cs:476-481`）。
//!
//! 本模块扮演 **SelGate**：驱动生产 `gate_conn` + `queue_loop` +
//! `login_link_task_with`（真 5600 文本行协议），无测试替身协议实现。
//!
//! 两个用例都 `#[ignore]`（同 lb08 口径：需要本机 MySQL + `mir2_test_db`；标准
//! `cargo test` 不跑）。跑法：
//! `cargo test -p db-srv --lib query_chain_tests -- --ignored --test-threads=1`
//!
//! 红检变异（本人 worktree 重放，证据进卡线程）：
//! - M1（断校验应答）：`handler.rs query_chr` 的 `else`（会话不符）分支改成静默
//!   `Ok(false)`（不发 527、不关会话）⇒ [`querychr_session_miss_replies_fail`] 红；
//! - M2（断成功应答）：成功分支删 `send_user(SM_QUERYCHR)` ⇒
//!   [`querychr_valid_session_replies_520`] 红；
//! - 「断转发」的牙在 sel-gate 侧（`sel-gate/src/forward_tests.rs`：客户端帧必须以
//!   `Data` 内部帧到达假 DBSrv），与本模块互补。
//!
//! A1 结论备查（窗口 RED 断点不在本链）：rust db-srv/sel-gate 窗口日志
//! （`E:/MirServer/logs/mir2-server/`）显示客户端会话从未到达（0 行会话日志）；
//! 530 把探针指到部署路由地址 `10.10.0.102:7100`（非本机地址），本机 FlClash TUN
//! 秒收连接后黑洞；C# 对照（smoke-rollback-final 01:04）给 `127.0.0.1/7100`——
//! 生效 AddrTable（`src/LoginSrv/bin/Release`）与部署树（`E:/MirServer/LoginSrv`）
//! 的数据差异，与 DBSrv AddrTable 卡（`openmir2-selgate-dbsrv-handshake`）同族。

#![allow(clippy::doc_markdown)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mir2_protocol::frame::{
    decode_server_frame_ex, encode_client_frame, CommandMessage, ServerDataPacket,
};
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use mir2_protocol::messages::{CM_QUERYCHR, SM_QUERYCHR, SM_QUERYCHR_FAIL};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

use crate::cache::CacheStore;
use crate::config::{for_test_with_conn, DbConfig};
use crate::db::Db;
use crate::handler::queue_loop;
use crate::loginsrv::login_link_task_with;
use crate::route::Share;
use crate::server::{gate_conn, Server};

/// 部署件同款 AddrTable（无 127.0.0.1；与查号判据无关，只是走生产 `Share::load`）。
const DEPLOYMENT_ADDRTABLE: &str =
    "10.10.0.163\n10.10.0.188\n10.10.0.101\n10.10.0.106\n10.10.0.102\n";

fn sandbox_base(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("w84-querychain-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("AddrTable.txt"), DEPLOYMENT_ADDRTABLE).unwrap();
    dir
}

fn db_config(ids_port: u16, database: &str) -> DbConfig {
    for_test_with_conn(
        "127.0.0.1".into(),
        ids_port,
        format!("server=127.0.0.1;uid=root;pwd=;database={database};").as_bytes(),
    )
}

/// stub LoginSrv：接受 DBSrv 的 5600 连接（置位 `connected`），`tx` 供测试向 DBSrv
/// 推送协议行（会话同步走**真** `login_link_task_with`/`feed`/`dispatch` 解析）。
async fn spawn_stub_login_srv() -> (u16, Arc<AtomicBool>, mpsc::UnboundedSender<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let connected = Arc::new(AtomicBool::new(false));
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let connected2 = Arc::clone(&connected);
    tokio::spawn(async move {
        let Ok((sock, _)) = listener.accept().await else {
            return;
        };
        connected2.store(true, Ordering::SeqCst);
        let (mut rd, mut wr) = sock.into_split();
        let reader = tokio::spawn(async move {
            let mut buf = [0u8; 512];
            loop {
                match rd.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });
        // 把测试要推的协议行写给 DBSrv。
        while let Some(line) = rx.recv().await {
            if wr.write_all(&line).await.is_err() {
                break;
            }
        }
        reader.abort();
    });
    (port, connected, tx)
}

async fn send_internal(cli: &mut TcpStream, msg: ServerDataMessage) {
    cli.write_all(&msg.to_wire()).await.unwrap();
}

async fn read_internal(cli: &mut TcpStream) -> ServerDataMessage {
    let mut head = [0u8; ServerDataPacket::SIZE];
    cli.read_exact(&mut head).await.unwrap();
    let pkt = ServerDataPacket::from_bytes(&head).unwrap();
    let mut body = vec![0u8; usize::from(pkt.packet_len)];
    cli.read_exact(&mut body).await.unwrap();
    ServerDataMessage::from_bytes(&body).unwrap()
}

/// 客户端 CM_QUERYCHR 帧：体照抄 2026-10-30 00:52 真机探针 c2s
/// （body_hex=`77696e706c6179302f33` = EDCode("winplay0/3")）。
fn client_querychr_frame() -> Vec<u8> {
    let head = CommandMessage::make(i32::from(CM_QUERYCHR), 0, 0, 0, 0);
    encode_client_frame(&head, b"winplay0/3")
}

struct Chain {
    cli: TcpStream,
}

/// 起链：queue_loop + login_link_task_with（100ms 重试）+ 一条生产 gate_conn。
async fn spawn_chain(tag: &str) -> (Chain, Arc<AtomicBool>, mpsc::UnboundedSender<Vec<u8>>) {
    let base = sandbox_base(tag);
    let share = Arc::new(Share::load(&base, Path::new("mapinfo-not-present.txt")).unwrap());
    let (ids_port, connected, tx) = spawn_stub_login_srv().await;
    let cfg = db_config(ids_port, "mir2_test_db");
    let (srv, rx) = Server::new(
        cfg,
        Arc::clone(&share),
        Arc::new(Mutex::new(CacheStore::default())),
    );
    let dsn = mir2_storage::LegacyDsn::parse(&srv.cfg.conn_string.text()).unwrap();
    let mut db_owned = Db::connect(&dsn).await.unwrap();
    db_owned
        .load_quick_lists()
        .await
        .expect("mir2_test_db 载入索引失败（环境缺表？见用例 doc）");
    let db = Arc::new(db_owned);
    tokio::spawn(queue_loop(Arc::clone(&srv), rx, Arc::clone(&db)));
    tokio::spawn(login_link_task_with(
        Arc::clone(&srv),
        Duration::from_millis(100),
    ));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cli = TcpStream::connect(addr).await.unwrap();
    let (sock, peer) = listener.accept().await.unwrap();
    tokio::spawn(async move {
        let _ = gate_conn(srv, sock, peer.to_string()).await;
    });
    (Chain { cli }, connected, tx)
}

async fn send_enter(cli: &mut TcpStream) {
    // 体格式 = C# UserEnter `%K{sid}/{ip}/{ip}$`（sel-gate/src/lib.rs sel_gate_enter_body）。
    send_internal(
        cli,
        ServerDataMessage::new(
            ServerDataType::Enter,
            "7",
            b"%K7/127.0.0.1/127.0.0.1$".to_vec(),
        ),
    )
    .await;
}

/// T1：会话表**没有**该会话（LoginSrv 从未推送）⇒ 必须回 `SM_QUERYCHR_FAIL`(527)。
/// oracle `UserService.cs:476-481`：不符分支回 FAIL 并 CloseUser，**不允许静默**——
/// 2026-10-30 00:52 窗口探针收到的恰是「什么都没有」，本牙钉住「不符也必须有回帧」。
#[tokio::test]
#[ignore = "需要本机 MySQL + mir2_test_db（同 lb08 口径）"]
async fn querychr_session_miss_replies_fail() {
    let (mut chain, _connected, _tx) = spawn_chain("miss").await;
    send_enter(&mut chain.cli).await;
    send_internal(
        &mut chain.cli,
        ServerDataMessage::new(ServerDataType::Data, "7", client_querychr_frame()),
    )
    .await;
    let reply = tokio::time::timeout(Duration::from_secs(3), read_internal(&mut chain.cli))
        .await
        .expect("会话不符分支无回帧（M1「断校验应答」变异咬红；oracle 必回 527）");
    assert_eq!(reply.kind, ServerDataType::Data, "回帧必须是 Data 内部帧");
    let (decoded, _) = decode_server_frame_ex(&reply.data).expect("载荷必须是可解码客户端帧");
    assert_eq!(
        i32::from(decoded.head.ident),
        i32::from(SM_QUERYCHR_FAIL),
        "必须回 SM_QUERYCHR_FAIL(527)，实际 ident={}",
        decoded.head.ident
    );
}

/// T2：经真 5600 协议推送会话后，CM_QUERYCHR ⇒ `SM_QUERYCHR`(520)。
/// 角色数不影响判据（0 角色也回 520 空表；oracle `UserService.cs:470-474`）。
#[tokio::test]
#[ignore = "需要本机 MySQL + mir2_test_db（同 lb08 口径）"]
async fn querychr_valid_session_replies_520() {
    let (mut chain, connected, tx) = spawn_chain("ok").await;
    send_enter(&mut chain.cli).await;
    // 等 DBSrv 的 5600 链接任务连上 stub，再按 LoginSrv 选服时下发的原样推会话行。
    let deadline = tokio::time::timeout(Duration::from_secs(5), async {
        while !connected.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    deadline.expect("login link 未在 5s 内连上 stub LoginSrv");
    tx.send(b"(100/winplay0/3/0/127.0.0.1/600)".to_vec())
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await; // 给 feed/dispatch 处理时间
    send_internal(
        &mut chain.cli,
        ServerDataMessage::new(ServerDataType::Data, "7", client_querychr_frame()),
    )
    .await;
    let reply = tokio::time::timeout(Duration::from_secs(3), read_internal(&mut chain.cli))
        .await
        .expect("合法会话无 520 回帧（M2「断成功应答」变异咬红）");
    assert_eq!(reply.kind, ServerDataType::Data, "回帧必须是 Data 内部帧");
    let (decoded, _) = decode_server_frame_ex(&reply.data).expect("载荷必须是可解码客户端帧");
    assert_eq!(
        i32::from(decoded.head.ident),
        i32::from(SM_QUERYCHR),
        "必须回 SM_QUERYCHR(520)，实际 ident={}",
        decoded.head.ident
    );
}
