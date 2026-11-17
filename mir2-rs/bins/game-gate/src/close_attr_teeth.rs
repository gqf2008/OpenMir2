//! 卡 `openmir2-session-close-attribution` 的红→绿牙。
//!
//! 归因判定是纯函数（`classify_close`），另配真实 socket 的接线牙：每个用例都必须看到
//! **恰好一次** `close_attr::record_close`（总数与 `断开链接` 同源 = 每会话退出一次）。
//!
//! 进程级计数器是全局量 ⇒ 所有摸计数器的用例先取 `close_attr::TEST_LOCK`（串行）。
use super::close_attr::{self, CloseReason};
use super::*;
use std::time::Duration;

/// 本组用例只关心会话拆除，规则值取部署近似值即可（不参与归因判定）。
fn gate_rules() -> rules::GateRules {
    rules::GateRules {
        chat_interval_enabled: true,
        chat_interval_ms: 800,
        speed: rules::SpeedRules::oracle_defaults(),
    }
}

fn close_test_ctx(service_id: u8) -> GateCtx {
    GateCtx {
        gate_addr: "127.0.0.1".into(),
        gate_port: 0,
        service_id,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        route_owner: Arc::new(Mutex::new(HashMap::new())),
        indexes: Arc::new(Mutex::new(HashMap::new())),
        idle: Arc::new(Mutex::new(HashMap::new())),
        downstream: Downstream::new("127.0.0.1".into(), 0),
        gate_rules: gate_rules(),
        nom_client_packet_size: 400,
    }
}

fn link_ctx(ctx: &GateCtx) -> LinkCtx {
    LinkCtx {
        downstream: ctx.downstream.clone(),
        sessions: Arc::clone(&ctx.sessions),
        route_owner: Arc::clone(&ctx.route_owner),
        indexes: Arc::clone(&ctx.indexes),
        idle: Arc::clone(&ctx.idle),
        state: Arc::new(StdMutex::new(GateState {
            running: RunningState::Running,
            gate_ready: true,
            check_server_fail: false,
            check_server_fail_count: 0,
            last_server_tick: now_ms(),
        })),
        local_endpoint: "127.0.0.1:0".into(),
        srv_addr: "127.0.0.1".into(),
        srv_port: 0,
        period: Duration::from_millis(10),
        timeout_ms: 60_000,
        gm_close_delay_ms: None,
    }
}

/// 起一对真实 TCP，返回 `(客户端, client_loop 的 JoinHandle)`。
async fn spawn_client_loop(
    ctx: GateCtx,
    sid: u16,
    handle: i32,
) -> (TcpStream, tokio::task::JoinHandle<Result<()>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let client = TcpStream::connect(addr).await.unwrap();
    let (srv, _) = listener.accept().await.unwrap();
    let slots = Arc::new(StdMutex::new(SessionSlots::new(8)));
    let task = tokio::spawn(client_loop(
        srv,
        sid,
        handle,
        handle,
        None,
        "127.0.0.1:1".into(),
        ctx,
        slots,
    ));
    (client, task)
}

async fn wait_until(mut cond: impl FnMut() -> bool) {
    for _ in 0..500 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    panic!("等待条件超时");
}

async fn join(task: tokio::task::JoinHandle<Result<()>>) {
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("client_loop 未在 5s 内退出")
        .expect("client_loop 任务 panic")
        .expect("client_loop 返回错误");
}

/// 纯判定：每个退出原因 → 唯一分类；变异「把 timeout 归到 client_fin」⇒ 本用例红。
#[test]
fn classifier_maps_every_exit_cause() {
    let none = CloseReason::Unknown.code();
    assert_eq!(
        classify_close(none, &CloseExit::ClientFin),
        (CloseReason::ClientFin, None)
    );
    assert_eq!(
        classify_close(none, &CloseExit::IdleSweep),
        (CloseReason::Timeout, None),
        "timeout 必须归 Timeout（归 client_fin ⇒ 红）"
    );
    assert_eq!(
        classify_close(none, &CloseExit::RuleClose),
        (CloseReason::ServerKick, None)
    );
    assert_eq!(
        classify_close(none, &CloseExit::FrameError),
        (CloseReason::Unknown, None)
    );
    // 墓碑优先：服务端先发起 ⇒ 客户端随后 FIN 也不改判。
    assert_eq!(
        classify_close(CloseReason::Tombstone.code(), &CloseExit::ClientFin),
        (CloseReason::Tombstone, None)
    );
    assert_eq!(
        classify_close(
            CloseReason::Tombstone.code(),
            &CloseExit::ReadError(std::io::Error::from_raw_os_error(10054))
        ),
        (CloseReason::Tombstone, None),
        "墓碑化后即使读错误也是服务端先发起"
    );
}

/// errno 桶：注入一个带 `raw_os_error` 的读错误 ⇒ `error` 类且带具体数值。
///
/// 变异：`classify_close` 的 `ReadError(_) => (Error, None)`（丢 errno）⇒ 红。
#[test]
fn read_error_classifies_error_with_errno() {
    let (reason, errno) = classify_close(
        CloseReason::Unknown.code(),
        &CloseExit::ReadError(std::io::Error::from_raw_os_error(10054)),
    );
    assert_eq!(reason, CloseReason::Error);
    assert_eq!(errno, Some(10054), "errno 桶必须带 raw_os_error 数值");
}

/// handle_frame 的 Err（非 io）落 `unknown`，**不得**落 `client_fin`。
///
/// 变异：把 `CloseExit::FrameError => Unknown` 改成 `ClientFin` ⇒ 红。
#[test]
fn unknown_path_never_folds_into_client_fin() {
    let (reason, _) = classify_close(CloseReason::Unknown.code(), &CloseExit::FrameError);
    assert_eq!(reason, CloseReason::Unknown);
    assert_ne!(reason, CloseReason::ClientFin);
    // 未登记发起方（255）也必须走退出原因，而不是被解释成 client_fin。
    assert_eq!(
        classify_close(255, &CloseExit::IdleSweep).0,
        CloseReason::Timeout
    );
}

/// ① 接线：客户端正常断开（读到 EOF）⇒ `client_fin` 恰 +1，总数恰 +1。
#[tokio::test]
async fn client_fin_is_attributed_once() {
    let _g = close_attr::TEST_LOCK.lock().await;
    let before = close_attr::snapshot();
    let (client, task) = spawn_client_loop(close_test_ctx(1), 101, 0x1001).await;
    drop(client);
    join(task).await;
    let after = close_attr::snapshot();
    assert_eq!(after.total - before.total, 1, "每会话退出恰记一次");
    assert_eq!(
        after.count(CloseReason::ClientFin) - before.count(CloseReason::ClientFin),
        1
    );
    assert_eq!(
        after.count(CloseReason::Unknown),
        before.count(CloseReason::Unknown)
    );
}

/// ④ 退化：登录通过后正常断开 ⇒ 明确 `client_fin`，不进 error/unknown。
#[tokio::test]
async fn normal_login_then_disconnect_is_client_fin() {
    let _g = close_attr::TEST_LOCK.lock().await;
    let before = close_attr::snapshot();
    let (mut client, task) = spawn_client_loop(close_test_ctx(2), 102, 0x1002).await;
    // 合法登录帧：account=acct1(5) char=char1(5) cert=2 ver=120020522 code=1200205220。
    let text = "**acct1/char1/2/120020522/1200205220/";
    let mut frame = b"#1".to_vec();
    frame.extend_from_slice(&mir2_protocol::edcode::encode(text.as_bytes()));
    frame.push(b'!');
    client.write_all(&frame).await.unwrap();
    // 登录帧被读循环消费后再断开（短暂等待近似「已进世界」）。
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(client);
    join(task).await;
    let after = close_attr::snapshot();
    assert_eq!(
        after.count(CloseReason::ClientFin) - before.count(CloseReason::ClientFin),
        1
    );
    assert_eq!(
        after.count(CloseReason::Error),
        before.count(CloseReason::Error)
    );
    assert_eq!(
        after.count(CloseReason::Unknown),
        before.count(CloseReason::Unknown)
    );
    assert_eq!(after.total - before.total, 1);
}

/// ① 接线：空闲清理关闭客户端 socket ⇒ 归 `timeout`（不是 `client_fin`）。
#[tokio::test]
async fn idle_sweep_is_attributed_timeout() {
    let _g = close_attr::TEST_LOCK.lock().await;
    let before = close_attr::snapshot();
    let ctx = close_test_ctx(3);
    let idle = Arc::clone(&ctx.idle);
    let sid = 103;
    let (_client, task) = spawn_client_loop(ctx, sid, 0x1003).await;
    wait_until(|| idle.try_lock().is_ok_and(|g| g.contains_key(&sid))).await;
    {
        let guard = idle.lock().await;
        guard[&sid].close.send(true).unwrap();
    }
    join(task).await;
    let after = close_attr::snapshot();
    assert_eq!(
        after.count(CloseReason::Timeout) - before.count(CloseReason::Timeout),
        1
    );
    assert_eq!(
        after.count(CloseReason::ClientFin),
        before.count(CloseReason::ClientFin)
    );
    assert_eq!(after.total - before.total, 1);
}

/// ① 接线：网关规则/登录解析剔除（`Step::Close`）⇒ 归 `server_kick`。
#[tokio::test]
async fn rule_kick_is_attributed_server_kick() {
    let _g = close_attr::TEST_LOCK.lock().await;
    let before = close_attr::snapshot();
    let (mut client, task) = spawn_client_loop(close_test_ctx(4), 104, 0x1004).await;
    // 登录前 ≥4 字节 ⇒ 走 ClientLogin 解析；垃圾文本 ⇒ 解析失败 ⇒ `Step::Close`。
    client.write_all(b"#1garbage-login!").await.unwrap();
    join(task).await;
    let after = close_attr::snapshot();
    assert_eq!(
        after.count(CloseReason::ServerKick) - before.count(CloseReason::ServerKick),
        1
    );
    assert_eq!(
        after.count(CloseReason::ClientFin),
        before.count(CloseReason::ClientFin)
    );
    assert_eq!(after.total - before.total, 1);
}

/// ① 接线：game-svr 的 `GM_CLOSE` 先墓碑化路由 ⇒ 客户端随后断开也归 `tombstone`。
///
/// 变异：`handle_server_message` 里删掉 `initiator.store(Tombstone)` ⇒ 本用例归
/// `client_fin` ⇒ 红。
#[tokio::test]
async fn server_close_is_attributed_tombstone() {
    let _g = close_attr::TEST_LOCK.lock().await;
    let before = close_attr::snapshot();
    let ctx = close_test_ctx(5);
    let sessions = Arc::clone(&ctx.sessions);
    let link = link_ctx(&ctx);
    let sid = 105;
    let (client, task) = spawn_client_loop(ctx, sid, 0x1005).await;
    wait_until(|| sessions.try_lock().is_ok_and(|g| g.contains_key(&sid))).await;
    let close = ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: 0x1005,
        session_id: sid,
        ident: u16::from(grobal2::GM_CLOSE),
        session_index: 0,
        pack_length: 0,
    };
    handle_server_message(&link, close, &[]).await;
    drop(client);
    join(task).await;
    let after = close_attr::snapshot();
    assert_eq!(
        after.count(CloseReason::Tombstone) - before.count(CloseReason::Tombstone),
        1
    );
    assert_eq!(
        after.count(CloseReason::ClientFin),
        before.count(CloseReason::ClientFin)
    );
    assert_eq!(after.total - before.total, 1);
}

/// ③ 接线（真实 RST）：客户端带 `SO_LINGER=0` 关闭 ⇒ 读循环拿 `raw_os_error`，
/// 落 `error` 桶且 errno 非 0。
#[allow(deprecated)] // tokio 对 `set_linger` 的提示只针对阻塞 drop；这里 linger=0 必须发 RST。
#[tokio::test]
async fn reset_socket_is_attributed_error_with_errno() {
    let _g = close_attr::TEST_LOCK.lock().await;
    let before = close_attr::snapshot();
    let (client, task) = spawn_client_loop(close_test_ctx(6), 106, 0x1006).await;
    client.set_linger(Some(Duration::ZERO)).unwrap();
    drop(client);
    join(task).await;
    let after = close_attr::snapshot();
    assert_eq!(
        after.count(CloseReason::Error) - before.count(CloseReason::Error),
        1
    );
    assert_eq!(after.total - before.total, 1);
    let nonzero = |s: &close_attr::CloseSnapshot| {
        s.errno
            .iter()
            .filter(|(code, _)| *code != 0)
            .map(|(_, n)| *n)
            .sum::<u64>()
    };
    assert!(
        nonzero(&after) > nonzero(&before),
        "errno 桶必须带非 0 数值: {after:?}"
    );
}
