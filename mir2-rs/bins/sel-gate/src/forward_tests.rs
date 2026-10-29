//! SelGate 客户端帧转发牙（卡 `openmir2-selgate-dbsrv-query-chain` A2「断转发」）。
//!
//! 场景：真 `run_inner` 起 sel-gate（临时 conf），下游接**假 DBSrv**（测试内监听器）；
//! 一个客户端连 7100 面后发一帧——SelGate 必须先发 `Enter`（开户），再把客户端帧
//! 以 `Data` 内部帧透传给 DBSrv（oracle `ClientSession.HandleUserPacket` →
//! `ServerDataMessage{Type=Data}` 原样转发，`src/SelGate/Services/ClientSession.cs`）。
//!
//! 牙：`forwards_client_frames_as_data`。
//! 变异 M-F（断转发，本人 worktree 重放）：把 `lib.rs` `client_loop` 里
//! `ServerDataType::Data` 分支的 `downstream.send(msg)` 删掉 ⇒ 本牙红（假 DBSrv
//! 等不到 Data，超时）。Enter 牙同时红（把 Enter 的 `downstream.send(enter)` 删除
//! ⇒ `delivers_enter_then_data` 红在第一步）。
//!
//! 离线合成：不连真 DBSrv、不连 MySQL，标准 `cargo test -p sel-gate` 即跑。

#![allow(clippy::doc_markdown)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mir2_protocol::frame::ServerDataPacket;
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// 假 DBSrv 收到的内部帧记录。
#[derive(Debug, Clone)]
struct Frame {
    kind: ServerDataType,
    socket_id: String,
    data: Vec<u8>,
}

struct FakeDbsrv {
    port: u16,
    frames: Arc<Mutex<Vec<Frame>>>,
    entered: Arc<AtomicBool>,
}

/// 起假 DBSrv：按内部帧协议（6B 头 + MemoryPack 体）累积收帧。
async fn spawn_fake_dbsrv() -> FakeDbsrv {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let frames: Arc<Mutex<Vec<Frame>>> = Arc::new(Mutex::new(Vec::new()));
    let entered = Arc::new(AtomicBool::new(false));
    let f2 = Arc::clone(&frames);
    let e2 = Arc::clone(&entered);
    tokio::spawn(async move {
        let Ok((sock, _)) = listener.accept().await else {
            return;
        };
        let (mut rd, _wr) = sock.into_split();
        loop {
            let mut head = [0u8; ServerDataPacket::SIZE];
            if rd.read_exact(&mut head).await.is_err() {
                break;
            }
            let Some(pkt) = ServerDataPacket::from_bytes(&head) else {
                continue;
            };
            let mut body = vec![0u8; usize::from(pkt.packet_len)];
            if rd.read_exact(&mut body).await.is_err() {
                break;
            }
            let Ok(msg) = ServerDataMessage::from_bytes(&body) else {
                continue;
            };
            if msg.kind == ServerDataType::Enter {
                e2.store(true, Ordering::SeqCst);
            }
            f2.lock().unwrap().push(Frame {
                kind: msg.kind,
                socket_id: msg.socket_id,
                data: msg.data,
            });
        }
    });
    FakeDbsrv {
        port,
        frames,
        entered,
    }
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

fn write_conf(path: &Path, dbsrv_port: u16, gate_port: u16) {
    std::fs::write(
        path,
        format!(
            "[SelGate]\nTitle=Test\nCount=1\nServerAddr0=127.0.0.1\nServerPort0={dbsrv_port}\nGatePort0={gate_port}\n\nShowLogLevel=1\nShowDebugLog=1\nMaxConnOfIPaddr=20\nBlockMethod=0\nKeepConnectTimeOut=60000\n\nAttackLevel=49\nMinimize=0\n"
        ),
    )
    .unwrap();
}

use std::path::Path;

/// 主牙：Enter 先到、客户端帧以 Data 内部帧原样到达假 DBSrv。
#[tokio::test]
async fn delivers_enter_then_client_frame_as_data() {
    let dbsrv = spawn_fake_dbsrv().await;
    let dir = std::env::temp_dir().join(format!("w84-selgate-fwd-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let gate_port = free_port();
    let conf = dir.join("config.conf");
    write_conf(&conf, dbsrv.port, gate_port);

    // 起 sel-gate（生产入口；accept 循环常驻，任务随测试进程结束）。
    let args = vec![conf.to_string_lossy().into_owned()];
    tokio::spawn(async move {
        let _ = crate::run_inner(&args).await;
    });

    // 等客户端面可连（sel-gate 监听器起来），连上后等 Enter 到达假 DBSrv。
    let cli = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(c) = TcpStream::connect(("127.0.0.1", gate_port)).await {
                return c;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("3s 内 7100 面不可连（sel-gate 未起？）");

    // 等待 Enter 出现（客户端连接建立后 sel-gate 发出）。
    tokio::time::timeout(Duration::from_secs(3), async {
        while !dbsrv.entered.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("未收到 Enter（M-Enter 变异：删 downstream.send(enter) 必红）");

    // 客户端发一帧（内容任意——sel-gate 对客户端载荷透明）。
    let mut cli = cli;
    let payload = b"#1forward-tooth-frame!";
    cli.write_all(payload).await.unwrap();

    // 断言：Data 内部帧到达假 DBSrv，载荷 == 客户端原帧。
    let data = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let hit = {
                let f = dbsrv.frames.lock().unwrap();
                f.iter()
                    .find(|x| x.kind == ServerDataType::Data && x.data == payload)
                    .cloned()
            };
            if let Some(f) = hit {
                return f;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("客户端帧未以 Data 到达假 DBSrv（M-F「断转发」变异咬红）");
    assert!(!data.socket_id.is_empty(), "Data 必须带会话 SocketId");
}
