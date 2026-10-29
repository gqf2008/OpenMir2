//! SelGate↔DBSrv 准入门牙（卡 `openmir2-selgate-dbsrv-handshake` A2）。
//!
//! 夹具 = **生产同款白名单**：`AddrTable.txt` 逐字取自部署件
//! `E:/MirServer/DBServer/AddrTable.txt`（5 个 10.10.0.x 网段 IP，**无 127.0.0.1**），
//! 连接一律从 127.0.0.1 发起（部署 `SelGate/config.conf` 的 `ServerAddr0=127.0.0.1`）。
//!
//! 背景（A1 已真机复现）：修复前 db-srv 对白名单外连接「一票否决 + Close」⇒ SelGate
//! 按 `TimedService` 10s 周期重连 = 线上观测的「握手每 10 秒断连」；修复后仅记日志、
//! 链路保持（以可观测 oracle 为准，见 server.rs `gate_conn` 同位注释）。
//!
//! 红检变异（在本人 worktree 重放，证据进卡线程）：
//! - M1（恢复一票否决）：`server.rs` `gate_conn` 白名单分支改回 `return Ok(())` ⇒
//!   [`accepts_gate_outside_addrtable_and_stays_open`] 红（连接秒断）；
//! - M2（删心跳回应）：`server.rs` `ServerDataType::KeepAlive` 分支的回帧删除 ⇒
//!   [`keepalive_roundtrip_on_gate_face`] 红（读回执超时）；
//! - M3（坏包杀链回归）：若有人把「PacketCode 不符仅丢帧」改成断链（oracle
//!   `UserService.ProcessGateData` 的 `UserService.cs:203-207` 是 continue）⇒
//!   [`wrong_packet_code_does_not_kill_link`] 红。

#![allow(clippy::doc_markdown)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mir2_protocol::frame::ServerDataPacket;
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::cache::CacheStore;
use crate::config::{for_test as cfg_for_test, DbConfig};
use crate::route::Share;
use crate::server::{gate_conn, Server};

/// 部署件同款 AddrTable（无 127.0.0.1；逐字节对应 E:/MirServer/DBServer/AddrTable.txt）。
const DEPLOYMENT_ADDRTABLE: &str =
    "10.10.0.163\n10.10.0.188\n10.10.0.101\n10.10.0.106\n10.10.0.102\n";

fn sandbox_base(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("w84-gateaccept-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("AddrTable.txt"), DEPLOYMENT_ADDRTABLE).unwrap();
    dir
}

/// 起 `gate_conn`（真实生产函数）+ 一个 127.0.0.1 客户端连接；返回客户端流。
async fn spawn_gate_conn(tag: &str) -> TcpStream {
    let base = sandbox_base(tag);
    let share = Arc::new(Share::load(&base, Path::new("mapinfo-not-present.txt")).unwrap());
    let cfg: DbConfig = cfg_for_test("127.0.0.1".into(), 16601);
    let (srv, _rx) = Server::new(
        cfg,
        Arc::clone(&share),
        Arc::new(Mutex::new(CacheStore::default())),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cli = TcpStream::connect(addr).await.unwrap();
    let (sock, peer) = listener.accept().await.unwrap();
    tokio::spawn(async move {
        // gate_conn 正常路径不返回；任务随测试进程结束。
        let _ = gate_conn(srv, sock, peer.to_string()).await;
    });
    cli
}

/// 读一条完整内部帧（6B 头 + 载荷）。
async fn read_frame(cli: &mut TcpStream) -> (ServerDataPacket, ServerDataMessage) {
    let mut head = [0u8; ServerDataPacket::SIZE];
    cli.read_exact(&mut head).await.unwrap();
    let pkt = ServerDataPacket::from_bytes(&head).unwrap();
    let mut body = vec![0u8; usize::from(pkt.packet_len)];
    cli.read_exact(&mut body).await.unwrap();
    (pkt, ServerDataMessage::from_bytes(&body).unwrap())
}

/// A2 主牙：白名单外的 127.0.0.1 网关连接**必须被接受**且链路保持（旧一票否决 ⇒ 秒断）。
#[tokio::test]
async fn accepts_gate_outside_addrtable_and_stays_open() {
    let mut cli = spawn_gate_conn("accept").await;
    // 读不到任何字节且连接未关 = 链路保持。修复前这里立刻读到 EOF（连接被关）。
    let mut buf = [0u8; 1];
    let outcome = tokio::time::timeout(Duration::from_millis(400), cli.read(&mut buf)).await;
    match outcome {
        Err(_elapsed) => { /* 连接仍开：预期 */ }
        Ok(Ok(0)) => panic!(
            "连接被对端立即关闭：回到「一票否决 + Close」旧行为（= 卡 A1 复现的每 10 秒断连根因）"
        ),
        Ok(Ok(n)) => panic!("意外收到 {n} 字节"),
        Ok(Err(e)) => panic!("读错误: {e}"),
    }
}

/// 心跳回应牙：SelGate 的 KeepAlive 必须得到 KeepAlive 回应（oracle
/// `UserService.cs:212-213`；SelGate `ClientThread.cs:181-185` 靠它确认链路）。
#[tokio::test]
async fn keepalive_roundtrip_on_gate_face() {
    let mut cli = spawn_gate_conn("keepalive").await;
    let req = ServerDataMessage::new(ServerDataType::KeepAlive, "", Vec::new());
    cli.write_all(&req.to_wire()).await.unwrap();
    let (_, reply) = tokio::time::timeout(Duration::from_secs(2), read_frame(&mut cli))
        .await
        .expect("KeepAlive 无回应（删回帧变异 M2 咬红）");
    assert_eq!(
        reply.kind,
        ServerDataType::KeepAlive,
        "回应类型必须是 KeepAlive"
    );
}

/// 坏包不杀链：PacketCode 不符的帧被丢弃后，后续合法帧照常处理（oracle
/// `UserService.cs:203-207` 语义；防「错握手包 ⇒ 断链」回归）。
#[tokio::test]
async fn wrong_packet_code_does_not_kill_link() {
    let mut cli = spawn_gate_conn("badcode").await;
    let mut bad = ServerDataMessage::new(ServerDataType::KeepAlive, "", Vec::new()).to_wire();
    bad[0] ^= 0xFF; // 只破坏 PacketCode；长度头保持可解析 ⇒ oracle 走「丢弃该帧」分支
    cli.write_all(&bad).await.unwrap();
    let good = ServerDataMessage::new(ServerDataType::KeepAlive, "", Vec::new());
    cli.write_all(&good.to_wire()).await.unwrap();
    let (_, reply) = tokio::time::timeout(Duration::from_secs(2), read_frame(&mut cli))
        .await
        .expect("坏包后链路死了（「错握手包 ⇒ 断链」回归；oracle 只丢帧不断链）");
    assert_eq!(reply.kind, ServerDataType::KeepAlive);
}

/// 防退化哨兵：白名单命中的连接照旧工作（本测试用部署件里真实存在的 10.10.0.163 伪装
/// 源 IP 需要改对端地址，不易构造 ⇒ 用「同一连接上第二条心跳仍被回应」替代锚定
/// 「接受后处理路径未变」）。
#[tokio::test]
async fn accepted_link_processes_repeated_keepalives() {
    let mut cli = spawn_gate_conn("repeat").await;
    let msg = ServerDataMessage::new(ServerDataType::KeepAlive, "", Vec::new());
    for _ in 0..3 {
        cli.write_all(&msg.to_wire()).await.unwrap();
        let (_, reply) = tokio::time::timeout(Duration::from_secs(2), read_frame(&mut cli))
            .await
            .expect("第二条起的心跳无回应");
        assert_eq!(reply.kind, ServerDataType::KeepAlive);
    }
}
