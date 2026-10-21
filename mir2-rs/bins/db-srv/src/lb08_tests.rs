//! LB-08 / 方案 A 的并发与顺序判据（`design-dbsrv-connection-20261021.md` §8）。
//!
//! 全部是 **R 级** 判据的构造级执行面：走生产函数（`dispatch`/`queue_loop`/
//! `data::load_human_rcd`/`data::store_human_rcd`/`Db::*`），故障注入用外部事务行锁、
//! 小连接池或 `pool.close()`。**默认 `#[ignore]`**，只在显式窗口跑；每个用例自建自删
//! 独立的 `mir2_test_lb8_*` 库（禁止 `mir2_db`）。
//!
//! 跑法（需本机 MySQL）：
//! `cargo test -p db-srv --bins lb08_tests -- --ignored --test-threads=1`
//!
//! 判据 → 用例：
//! - 慢存档不阻塞 QUERYCHR / SELCHR / LOAD → 三个 `slow_save_*`；
//! - 5100 回包全序 → `gateway_queue_preserves_reply_order`；
//! - 建角闸门防重名竞态 → `create_gate_single_winner`；
//! - 索引读写无撕裂 → `index_snapshot_never_tears`；
//! - DB 不可达时每连接独立失败（非 N×超时串行）→ `db_down_requests_fail_in_parallel`；
//! - 关闭路径无泄漏 → `closed_db_leaves_no_arc_or_lock_leak`。

#![allow(clippy::doc_markdown)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use mir2_protocol::edcode;
use mir2_protocol::frame::{encode_server_frame, CommandMessage};
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use mir2_protocol::messages::{
    CM_DELCHR, CM_QUERYCHR, CM_SELCHR, SM_DELCHR_SUCCESS, SM_QUERYCHR, SM_STARTPLAY,
};
use mir2_storage::playerdata::{CharacterDataInfo, LoadCharacterData, MemPack};
use mir2_storage::LegacyDsn;
use sqlx::MySqlConnection;
use tokio::sync::{mpsc, Mutex};

use crate::cache::CacheStore;
use crate::db::Db;
use crate::server::{Gate, GateUser, Queued, Server};
use crate::testsupport as ts;

const ACCOUNT: &str = "lb8acct";
const ACCOUNT2: &str = "lb8acct2";
const SESSION: i32 = 8181;

fn info(name: &str) -> CharacterDataInfo {
    let mut v = CharacterDataInfo::csharp_new();
    v.header.set_name(Some(name.to_string()));
    if let Some(d) = v.data.as_mut() {
        d.chr_name = Some(name.to_string());
    }
    v
}

/// 一条 5100 上行（`#` + 编码头 + 编码体 + `!`）。
fn client_frame(ident: u16, plain: &[u8]) -> Vec<u8> {
    let head = CommandMessage::make(i32::from(ident), 0, 0, 0, 0);
    let mut frame = encode_server_frame(&head, &edcode::encode(plain));
    // 客户端帧在 `#` 后多一个标签字节（`process_data` 会 `frame[1..]` 丢掉它）。
    frame.insert(1, b'1');
    frame
}

fn queued(slot: usize, socket: &str, ident: u16, plain: &[u8]) -> Queued {
    Queued {
        slot,
        msg: ServerDataMessage::new(ServerDataType::Data, socket, client_frame(ident, plain)),
    }
}

/// 外部事务锁住 `characters` 的一行（模拟 InnoDB 行锁/外部 `SELECT ... FOR UPDATE`）。
async fn lock_character_row(dsn: &LegacyDsn, name: &str) -> MySqlConnection {
    let mut c = ts::conn(dsn).await;
    sqlx::query("START TRANSACTION")
        .execute(&mut c)
        .await
        .expect("开外部事务失败");
    let row = sqlx::query("SELECT Id FROM characters WHERE ChrName=? FOR UPDATE")
        .bind(name)
        .fetch_optional(&mut c)
        .await
        .expect("取行锁失败");
    assert!(row.is_some(), "被锁角色行必须存在");
    c
}

/// 外部事务锁住 `characters_indexes` 的一行。
async fn lock_index_row(dsn: &LegacyDsn, name: &str) -> MySqlConnection {
    let mut c = ts::conn(dsn).await;
    sqlx::query("START TRANSACTION")
        .execute(&mut c)
        .await
        .expect("开外部事务失败");
    let row = sqlx::query("SELECT Id FROM characters_indexes WHERE ChrName=? FOR UPDATE")
        .bind(name)
        .fetch_optional(&mut c)
        .await
        .expect("取索引行锁失败");
    assert!(row.is_some(), "被锁索引行必须存在");
    c
}

async fn rollback(c: &mut MySqlConnection) {
    sqlx::query("ROLLBACK")
        .execute(&mut *c)
        .await
        .expect("释放外部事务失败");
}

/// 等某个存档事务进入 InnoDB 锁等待（证明它真的被行锁挡住），最多 5s。
///
/// 同时看 `INNODB_TRX.trx_state='LOCK WAIT'` 与 `INNODB_LOCK_WAITS`（MariaDB 两者都在；
/// 单看一个源在慢机器上偶发漏报）。若存档任务已经结束（说明没挡住）返回 false，
/// 调用方按「判据失去意义」处理。
async fn wait_lock_wait(dsn: &LegacyDsn, save: &tokio::task::JoinHandle<bool>) -> bool {
    let mut c = ts::conn(dsn).await;
    for _ in 0..250 {
        if save.is_finished() {
            return false;
        }
        // 主源：MySQL 8/MariaDB 都有 INNODB_TRX；备源 MariaDB 的 INNODB_LOCK_WAITS
        // （MySQL 8 已移除该表，查询失败按 0 处理，不掩盖主源）。
        let n: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.INNODB_TRX WHERE trx_state='LOCK WAIT'",
        )
        .fetch_one(&mut c)
        .await
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!("INNODB_TRX query failed: {e:#}");
                0
            }
        };
        if n > 0 {
            return true;
        }
        let waiting: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.INNODB_LOCK_WAITS")
                .fetch_one(&mut c)
                .await
                .unwrap_or(0);
        if waiting > 0 {
            return true;
        }
        // 第三见证：存档的 `UPDATE characters ...` 在本库上已执行 >1s。外部行锁一直持着，
        // 这条语句只可能是在等锁（MariaDB 偶发不在 INNODB_TRX 里登记该等待）。
        let updating: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.PROCESSLIST              WHERE DB = ? AND STATE = 'Updating' AND INFO LIKE 'UPDATE characters%' AND TIME >= 1",
        )
        .bind(&dsn.database)
        .fetch_one(&mut c)
        .await
        .unwrap_or(0);
        if updating > 0 {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

/// 建 1 个「索引 + 完整 characters 行」的角色（索引走 5100 表，存档行走 6000 生产路径）。
async fn seed(dsn: &LegacyDsn, account: &str, name: &str) -> (Arc<Db>, Arc<Server>, GateUser) {
    ts::insert_index(dsn, account, name).await;
    let db = ts::open_db(dsn).await;
    assert!(
        db.play_data_add(&info(name)).await.expect("建角失败"),
        "6000 生产路径建角必须成功"
    );
    let db = Arc::new(db);
    let server = ts::test_server(name, dsn);
    server
        .login
        .lock()
        .await
        .add_session(account.as_bytes().to_vec(), SESSION, Vec::new());
    let mut u = ts::gate_user("s1", account.as_bytes());
    u.session_no = SESSION;
    (db, server, u)
}

fn empty_cache() -> Arc<Mutex<CacheStore>> {
    Arc::new(Mutex::new(CacheStore::default()))
}

/// 通用骨架：外部锁住角色 A 的 `characters` 行 → 起一条 6000 存档并确认它进了 LOCK WAIT →
/// 执行 `probe` 并返回其耗时。
async fn measure_while_slow_save<F, Fut>(
    db: &Arc<Db>,
    dsn: &LegacyDsn,
    name: &str,
    probe: F,
) -> Duration
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut locker = lock_character_row(dsn, name).await;
    let cache = empty_cache();
    let rcd = info(name);
    let save_name = name.to_string();
    let save_db = Arc::clone(db);
    let save = tokio::spawn(async move {
        crate::data::store_human_rcd(&save_db, &cache, &save_name, &rcd).await
    });
    if !wait_lock_wait(dsn, &save).await {
        let mut c = ts::conn(dsn).await;
        let rows: Vec<(i64, String, i64, String, Option<String>)> = sqlx::query_as(
            "SELECT ID, COMMAND, TIME, STATE, LEFT(INFO,160) FROM information_schema.PROCESSLIST              WHERE DB=? ORDER BY ID",
        )
        .bind(&dsn.database)
        .fetch_all(&mut c)
        .await
        .unwrap_or_default();
        for r in &rows {
            eprintln!("PROCLIST {:?} {:?} {:?} {:?} {:?}", r.0, r.1, r.2, r.3, r.4);
        }
        panic!(
            "慢存档必须真的被行锁挡住（否则判据失去意义；save_finished={})",
            save.is_finished()
        );
    }
    let t0 = Instant::now();
    // 探针必须有上界：回归时若无锁保护，延迟测例会卡到测试超时（50s）甚至永久。
    tokio::time::timeout(Duration::from_secs(2), probe())
        .await
        .expect("探针 2s 未返回（被测路径疑似退化/死锁）");
    let elapsed = t0.elapsed();
    // 释放行锁；存档必须正常收尾（oracle 口径：失败被吞、仍回成功应答）。
    rollback(&mut locker).await;
    let bo21 = tokio::time::timeout(Duration::from_secs(10), save)
        .await
        .expect("行锁释放后存档必须收尾")
        .expect("存档任务 panic");
    assert!(!bo21, "索引命中 ⇒ bo21=false（DBR_SAVEHUMANRCD recog=1）");
    elapsed
}

/// LB-08 主判据（QUERYCHR）：慢存档期间 `CM_QUERYCHR` 必须在 500ms 内回包。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_lb8_slowq（自建自删）"]
async fn slow_save_does_not_block_querychr() {
    let dsn = ts::setup_db("mir2_test_lb8_slowq").await;
    let (db, server, mut u) = seed(&dsn, ACCOUNT, "lb8qhero").await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let body = format!("{ACCOUNT}/{SESSION}");
    let elapsed = measure_while_slow_save(&db, &dsn, "lb8qhero", || async {
        crate::handler::dispatch(
            &server,
            &db,
            0,
            &tx,
            "127.0.0.1",
            &mut u,
            &crate::user::ClientPacket {
                head: CommandMessage::make(i32::from(CM_QUERYCHR), 0, 0, 0, 0),
                body: edcode::encode(body.as_bytes()),
            },
        )
        .await
        .expect("QUERYCHR 不得上抛");
    })
    .await;
    assert!(
        elapsed < Duration::from_millis(500),
        "LB-08：慢存档期间 QUERYCHR 耗时 {elapsed:?} ≥ 500ms（队头阻塞）"
    );
    let frames = ts::take_frames(&mut rx);
    assert_eq!(frames.len(), 1, "QUERYCHR 必须回包（不得被慢存档吞掉）");
    let (head, list) = ts::decode_user_reply(&frames[0]);
    assert_eq!(head.ident, SM_QUERYCHR, "必须是角色列表帧");
    assert_eq!(list, b"lb8qhero/0/0/0/0/", "列表内容不得因并发变化");
    ts::drop_db(&dsn).await;
}

/// LB-08（SELCHR）：慢存档期间 `CM_SELCHR` 必须在 500ms 内回 `SM_STARTPLAY`。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_lb8_slows（自建自删）"]
async fn slow_save_does_not_block_selchr() {
    let dsn = ts::setup_db("mir2_test_lb8_slows").await;
    let (db, server, mut u) = seed(&dsn, ACCOUNT, "lb8shero").await;
    u.chr_queryed = true;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let body = format!("{ACCOUNT}/lb8shero");
    let elapsed = measure_while_slow_save(&db, &dsn, "lb8shero", || async {
        crate::handler::dispatch(
            &server,
            &db,
            0,
            &tx,
            "127.0.0.1",
            &mut u,
            &crate::user::ClientPacket {
                head: CommandMessage::make(i32::from(CM_SELCHR), 0, 0, 0, 0),
                body: edcode::encode(body.as_bytes()),
            },
        )
        .await
        .expect("SELCHR 不得上抛");
    })
    .await;
    assert!(
        elapsed < Duration::from_millis(500),
        "LB-08：慢存档期间 SELCHR 耗时 {elapsed:?} ≥ 500ms（队头阻塞）"
    );
    let frames = ts::take_frames(&mut rx);
    assert_eq!(frames.len(), 1, "SELCHR 必须回包");
    let (head, _route) = ts::decode_user_reply(&frames[0]);
    assert_eq!(head.ident, SM_STARTPLAY, "必须进入选角成功帧");
    ts::drop_db(&dsn).await;
}

/// LB-08（LOAD）：慢存档期间另一 6000 连接的 `DB_LOADHUMANRCD` 必须在 500ms 内回包。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_lb8_slowl（自建自删）"]
async fn slow_save_does_not_block_load() {
    let dsn = ts::setup_db("mir2_test_lb8_slowl").await;
    let (db, server, _u) = seed(&dsn, ACCOUNT, "lb8lhero").await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let req = LoadCharacterData {
        account: Some(ACCOUNT.to_string()),
        chr_name: Some("lb8lhero".to_string()),
        user_addr: Some("127.0.0.1".to_string()),
        session_id: SESSION,
    };
    let bytes = req.to_bytes();
    let elapsed = measure_while_slow_save(&db, &dsn, "lb8lhero", || async {
        crate::data::load_human_rcd(&server, &db, &tx, 41, &bytes)
            .await
            .expect("LOAD 不得上抛");
    })
    .await;
    assert!(
        elapsed < Duration::from_millis(500),
        "LB-08：慢存档期间 LOAD 耗时 {elapsed:?} ≥ 500ms（队头阻塞）"
    );
    let frames = ts::take_frames(&mut rx);
    assert_eq!(frames.len(), 1, "LOAD 必须回包");
    ts::drop_db(&dsn).await;
}

/// 5100 全序：单消费者队列必须按入队顺序回包。
///
/// 第一条 `CM_DELCHR` 被外部行锁挡住 ~700ms，第二条 `CM_QUERYCHR` 只读
/// `characters_indexes` 的另一账号行（MVCC 不阻塞）。**若队列改成每消息
/// `tokio::spawn`**，QUERYCHR 会先回包 ⇒ 断言红。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_lb8_order（自建自删）"]
async fn gateway_queue_preserves_reply_order() {
    let dsn = ts::setup_db("mir2_test_lb8_order").await;
    ts::insert_index(&dsn, ACCOUNT, "ordaa").await;
    ts::insert_index(&dsn, ACCOUNT2, "ordbb").await;
    let db = ts::open_db(&dsn).await;
    assert!(db.play_data_add(&info("ordaa")).await.expect("建 a"));
    assert!(db.play_data_add(&info("ordbb")).await.expect("建 b"));
    let db = Arc::new(db);
    let server = ts::test_server("order", &dsn);
    {
        let mut login = server.login.lock().await;
        login.add_session(ACCOUNT.as_bytes().to_vec(), SESSION, Vec::new());
        login.add_session(ACCOUNT2.as_bytes().to_vec(), SESSION, Vec::new());
    }
    // `now_ms` 的 START 是懒初始化的：先触发一次再等过 1000ms 限频窗（否则本用例可能
    // 是该进程里第一个调用，`chr_tick=0` 且 `now≈0` ⇒ DELCHR 被当作 Hacker Attack 丢掉）。
    let _ = crate::handler::now_ms();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let mut u1 = ts::gate_user("s-a", ACCOUNT.as_bytes());
    u1.session_no = SESSION;
    u1.chr_tick = 0;
    let mut u2 = ts::gate_user("s-b", ACCOUNT2.as_bytes());
    u2.session_no = SESSION;
    let (gtx, mut grx) = mpsc::unbounded_channel();
    {
        let mut gates = server.gates.lock().await;
        gates[0] = Some(Gate {
            id: 1,
            remote_ip: "127.0.0.1".to_string(),
            n_gate_id: 0,
            users: vec![u1, u2],
            tx: gtx,
        });
    }
    let (qtx, qrx) = mpsc::unbounded_channel();
    let qserver = Arc::clone(&server);
    let qdb = Arc::clone(&db);
    let worker = tokio::spawn(crate::handler::queue_loop(qserver, qrx, qdb));

    let mut locker = lock_index_row(&dsn, "ordaa").await;
    qtx.send(queued(0, "s-a", CM_DELCHR, b"ordaa"))
        .expect("入队 DELCHR");
    // 第二条故意选**只读**的 QUERYCHR：若改成每消息 spawn，它会在 DELCHR 被行锁挡住时
    // 先回包（SELCHR 会写 characters_indexes，同样被表级行锁挡住，测不出乱序）。
    qtx.send(queued(
        0,
        "s-b",
        CM_QUERYCHR,
        format!("{ACCOUNT2}/{SESSION}").as_bytes(),
    ))
    .expect("入队 QUERYCHR");
    drop(qtx);

    // 700ms 后放锁：DELCHR 收尾，随后才轮到 QUERYCHR。
    tokio::time::sleep(Duration::from_millis(700)).await;
    rollback(&mut locker).await;

    let first = tokio::time::timeout(Duration::from_secs(5), grx.recv())
        .await
        .expect("等第一帧超时")
        .expect("通道关闭");
    let second = tokio::time::timeout(Duration::from_secs(5), grx.recv())
        .await
        .expect("等第二帧超时")
        .expect("通道关闭");
    let (h1, _) = ts::decode_user_reply(&first);
    let (h2, _) = ts::decode_user_reply(&second);
    assert_eq!(h1.ident, SM_DELCHR_SUCCESS, "第一帧必须是先入队的 DELCHR");
    assert_eq!(h2.ident, SM_QUERYCHR, "第二帧必须是后入队的 QUERYCHR");
    tokio::time::timeout(Duration::from_secs(2), worker)
        .await
        .expect("queue_loop 必须随队列关闭退出")
        .expect("queue_loop panic");
    ts::drop_db(&dsn).await;
}

/// 建角闸门：5100 `data_add` 与 6000 `play_data_add` 对同名并发 ⇒ 只允许一个赢家；
/// 两个并发 `record_add` 同理（索引表只 1 行）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_lb8_gate（自建自删）"]
async fn create_gate_single_winner() {
    let dsn = ts::setup_db("mir2_test_lb8_gate").await;
    let db = ts::open_db(&dsn).await;
    let db = Arc::new(db);
    let mut conn = ts::conn(&dsn).await;
    for i in 0..5 {
        let name = format!("race{i}");
        let idx_name = format!("raceidx{i}");
        let d1 = Arc::clone(&db);
        let d2 = Arc::clone(&db);
        let n1 = name.clone();
        let i1 = info(&name);
        let (a, b) = tokio::join!(
            async move { d1.data_add(&n1, ACCOUNT, 1, 0, 0).await },
            async move { d2.play_data_add(&i1).await },
        );
        let a = a.expect("data_add 不得上抛");
        let b = b.expect("play_data_add 不得上抛");
        assert!(
            a.is_some() ^ b,
            "同名并发建角必须恰好一个赢家（data_add={a:?} play_data_add={b}）"
        );
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM characters WHERE ChrName=?")
            .bind(&name)
            .fetch_one(&mut conn)
            .await
            .expect("统计 characters 失败");
        assert_eq!(rows, 1, "同名角色只允许 1 行（否则是持久化污染）");
        assert!(
            db.data_index(&name).await.is_some(),
            "赢家必须写 name_quick"
        );
        assert!(
            db.play_data_index_text(&name).await.is_some(),
            "赢家必须写 name_text"
        );

        let d3 = Arc::clone(&db);
        let d4 = Arc::clone(&db);
        let in1 = idx_name.clone();
        let in2 = idx_name.clone();
        let (x, y) = tokio::join!(
            async move { d3.record_add(&in1, ACCOUNT, 0).await },
            async move { d4.record_add(&in2, ACCOUNT, 0).await },
        );
        let x = x.expect("record_add 不得上抛");
        let y = y.expect("record_add 不得上抛");
        assert!(x.is_some() ^ y.is_some(), "同名并发插索引必须恰好一个赢家");
        let idx_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM characters_indexes WHERE ChrName=?")
                .bind(&idx_name)
                .fetch_one(&mut conn)
                .await
                .expect("统计 characters_indexes 失败");
        assert_eq!(idx_rows, 1, "同名索引只允许 1 行");
    }
    // 闸门不是全局锁：持住 race0 的闸门时，另一个落在不同分片的名字必须立刻可建。
    // （同名仍由同一分片互斥；不同名撞同一分片是可接受的残余，见 whitelist T-120。）
    let held_name = "race0";
    let other = (0..10_000)
        .map(|i| format!("gatename{i}"))
        .find(|n| crate::db::create_gate_shard(n) != crate::db::create_gate_shard(held_name))
        .expect("64 分片下必须能找到不同分片的名字");
    let gate = db.create_gate(held_name).lock().await;
    assert!(
        tokio::time::timeout(Duration::from_secs(1), db.create_gate(&other).lock())
            .await
            .is_ok(),
        "不同名字不得被 race0 的建角闸门阻塞（闸门必须是按名分片，不是全局）"
    );
    drop(gate);
    ts::drop_db(&dsn).await;
}

/// 索引无撕裂：写者建索引的同时，读者反复取 `by_account` 快照并逐项回查 `quick`。
/// 单调只增 ⇒ 任何被看到的 `Quick` 都必须已在 `quick` 里（同一写临界区更新）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_lb8_tear（自建自删）"]
async fn index_snapshot_never_tears() {
    let dsn = ts::setup_db("mir2_test_lb8_tear").await;
    let db = ts::open_db(&dsn).await;
    let db = Arc::new(db);
    let writer = {
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            for i in 0..60 {
                let name = format!("tear{i}");
                db.record_add(&name, "tearacct", 0)
                    .await
                    .expect("record_add 失败");
                tokio::task::yield_now().await;
            }
        })
    };
    // 读者采样必须够密：写者的撕裂窗口只有「两次写临界区之间」的几微秒。
    // 原 4×300 轮在变异（M4 拆写临界区）下 8 次只红 2 次；这里提高到 8×4000，
    // 且**不在快照与回查之间让出**（让出会给写者补完 quick 的机会，反而漏检）。
    let mut readers = Vec::new();
    for _ in 0..8 {
        let db = Arc::clone(&db);
        readers.push(tokio::spawn(async move {
            for _ in 0..4000 {
                let list = db.find_by_account(b"tearacct").await;
                for q in list {
                    assert!(
                        db.record_index(&q.name_text).await.is_some(),
                        "索引撕裂：by_account 可见 name={:?}，但 quick 查不到",
                        q.name_text
                    );
                }
                tokio::task::yield_now().await;
            }
        }));
    }
    writer.await.expect("写者 panic");
    for r in readers {
        r.await.expect("读者断言失败");
    }
    assert_eq!(
        db.find_by_account(b"tearacct").await.len(),
        60,
        "60 条索引必须全部就位"
    );
    ts::drop_db(&dsn).await;
}

/// DB 不可达：N 个独立 6000 请求各自超时失败，总墙钟 ≈ 1× 超时（不是 N×）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_lb8_par（自建自删）"]
async fn db_down_requests_fail_in_parallel() {
    let dsn = ts::setup_db("mir2_test_lb8_par").await;
    ts::insert_index(&dsn, ACCOUNT, "parhero").await;
    ts::insert_character(&dsn, ACCOUNT, "parhero").await;
    // 小池 + 2s 超时：把 N 条连接全部占住 ⇒ 后续 acquire 以超时失败（等价 MySQL 停机）。
    let mut db = Db::connect_with(&dsn, 2, Duration::from_secs(2))
        .await
        .expect("连测试库失败");
    db.load_quick_lists().await.expect("载入索引失败");
    assert!(
        db.play_data_index_text("parhero").await.is_some(),
        "内存索引必须命中（否则走 -3 分支、不碰连接池）"
    );
    let db = Arc::new(db);
    let c1 = db.pool.acquire().await.expect("占住连接 1");
    let c2 = db.pool.acquire().await.expect("占住连接 2");
    let server = ts::test_server("par", &dsn);
    server
        .login
        .lock()
        .await
        .add_session(ACCOUNT.as_bytes().to_vec(), SESSION, Vec::new());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let req = LoadCharacterData {
        account: Some(ACCOUNT.to_string()),
        chr_name: Some("parhero".to_string()),
        user_addr: Some("127.0.0.1".to_string()),
        session_id: SESSION,
    };
    let bytes = req.to_bytes();
    let t0 = Instant::now();
    let mut handles = Vec::new();
    for qid in 0..4 {
        let db = Arc::clone(&db);
        let server = Arc::clone(&server);
        let tx = tx.clone();
        let bytes = bytes.clone();
        handles.push(tokio::spawn(async move {
            crate::data::load_human_rcd(&server, &db, &tx, qid, &bytes)
                .await
                .expect("降级路径不得上抛");
        }));
    }
    for h in handles {
        h.await.expect("LOAD 任务 panic");
    }
    let total = t0.elapsed();
    drop(c1);
    drop(c2);
    assert!(
        total < Duration::from_secs(4),
        "DB 不可达时 4 条请求总墙钟 {total:?} ≥ 2×单请求超时（被串行化）"
    );
    let frames = ts::take_frames(&mut rx);
    assert_eq!(frames.len(), 4, "4 条请求必须各有 1 帧降级成功应答");
    for f in &frames {
        let head =
            mir2_protocol::frame::ServerDataPacket::from_bytes(&f[..6]).expect("6B 头解析失败");
        assert_eq!(head.packet_len as usize, f.len() - 6);
    }
    ts::drop_db(&dsn).await;
}

/// 关闭路径无泄漏：并发请求结束后 `Arc<Db>` 强引用回到 1，索引/闸门锁都无人持有。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_lb8_leak（自建自删）"]
async fn closed_db_leaves_no_arc_or_lock_leak() {
    let dsn = ts::setup_db("mir2_test_lb8_leak").await;
    ts::insert_index(&dsn, ACCOUNT, "leakhero").await;
    let db = ts::open_db(&dsn).await;
    db.pool.close().await;
    let db = Arc::new(db);
    let server = ts::test_server("leak", &dsn);
    server
        .login
        .lock()
        .await
        .add_session(ACCOUNT.as_bytes().to_vec(), SESSION, Vec::new());
    let (tx, _rx) = mpsc::unbounded_channel();
    let req = LoadCharacterData {
        account: Some(ACCOUNT.to_string()),
        chr_name: Some("leakhero".to_string()),
        user_addr: Some("127.0.0.1".to_string()),
        session_id: SESSION,
    };
    let bytes = req.to_bytes();
    let mut handles = Vec::new();
    for qid in 0..8 {
        let db = Arc::clone(&db);
        let server = Arc::clone(&server);
        let tx = tx.clone();
        let bytes = bytes.clone();
        handles.push(tokio::spawn(async move {
            crate::data::load_human_rcd(&server, &db, &tx, qid, &bytes)
                .await
                .expect("池关闭后仍须回降级帧");
        }));
    }
    for h in handles {
        h.await.expect("LOAD 任务 panic");
    }
    drop(tx);
    drop(server);
    assert_eq!(
        Arc::strong_count(&db),
        1,
        "所有请求结束后不得残留 Arc<Db> 克隆（任务泄漏）"
    );
    for (i, gate) in db.create_gates.iter().enumerate() {
        assert!(
            tokio::time::timeout(Duration::from_secs(1), gate.lock())
                .await
                .is_ok(),
            "建角闸门分片 {i} 不得被残留 guard 占住"
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_secs(1), db.idx.write())
            .await
            .is_ok(),
        "索引锁不得被残留 guard 占住"
    );
    assert!(Arc::try_unwrap(db).is_ok(), "Db 必须可独占析构");
    ts::drop_db(&dsn).await;
}

// 静态断言：方案 A 的核心前提 —— `Db` 本身可跨任务共享，不再需要外层 `Arc<Mutex<..>>`。
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Db>();
};
