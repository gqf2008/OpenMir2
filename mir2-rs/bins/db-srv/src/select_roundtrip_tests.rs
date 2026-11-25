//! **card/selectchr-roundtrip**：选角 SQL 往返收敛的牙（DB 级 + 生产派发级）。
//!
//! 跑法（本机 MySQL，库 `mir2_test_selrt_*` 自建自删）：
//! `cargo test -p db-srv --lib select_roundtrip -- --ignored --nocapture`
//!
//! 三条牙：
//! 1. `batch_records_get_by_matches_per_row`：批量 `IN (...)` 与逐条 `GetBy` **逐字段相等**，
//!    且往返计数 1 vs N（N=3）。
//! 2. `select_chr_keeps_db_state_and_uses_four_round_trips`：生产 `dispatch(CM_SELCHR)` 后
//!    选中项 `SelectID=1`、旧选中项清零、其余不动；回帧 `SM_STARTPLAY` 与路由体不变；
//!    往返次数 = 4（1 批量 + 2 更新 + 1 地图查询）。把批量改回逐条 ⇒ 6 ⇒ 红。
//! 3. `select_chr_falls_back_to_per_row_when_batch_query_fails`：池关闭（SQL 全失败）⇒
//!    批量失败回退逐条，最终与旧实现同形：**不回包**、错误上抛（不 panic、不半写）。

use std::sync::Arc;

use mir2_protocol::edcode;
use mir2_protocol::frame::CommandMessage;
use mir2_protocol::messages::{CM_QUERYCHR, CM_SELCHR, SM_QUERYCHR, SM_STARTPLAY};
use mir2_storage::LegacyDsn;

use crate::db::Db;
use crate::testsupport as ts;

const ACCOUNT: &str = "selrtacct";
const SESSION: i32 = 77;

async fn seed(dsn: &LegacyDsn, names: &[&str]) -> Arc<Db> {
    for name in names {
        ts::insert_index(dsn, ACCOUNT, name).await;
        ts::insert_character(dsn, ACCOUNT, name).await;
    }
    Arc::new(ts::open_db(dsn).await)
}

async fn selected_of(db: &Db, name: &str) -> u8 {
    let id = db.record_index(name).await.expect("名字应有索引");
    db.record_get_by(id)
        .await
        .expect("GetBy 不得失败")
        .expect("行必须存在")
        .selected
}

/// **牙①**：批量与逐条逐字段相等；往返 1 vs 3。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_selrt_a（自建自删）"]
async fn batch_records_get_by_matches_per_row() {
    let dsn = ts::setup_db("mir2_test_selrt_a").await;
    let db = seed(&dsn, &["sa1", "sa2", "sa3"]).await;
    let ids: Vec<i64> = {
        let mut v = Vec::new();
        for name in ["sa1", "sa2", "sa3"] {
            v.push(db.record_index(name).await.expect("索引"));
        }
        v
    };
    let batch = db.records_get_by(&ids).await.expect("批量查询失败");
    for id in &ids {
        let row = db
            .record_get_by(*id)
            .await
            .expect("逐条查询失败")
            .expect("行必须存在");
        let got = batch.get(id).expect("批量必须含该行");
        assert_eq!(
            (
                got.id,
                &got.account_text,
                &got.name_text,
                got.selected,
                got.deleted
            ),
            (
                row.id,
                &row.account_text,
                &row.name_text,
                row.selected,
                row.deleted
            ),
            "批量与逐条必须逐字段相等（id={id}）"
        );
    }

    let r0 = db.round_trips();
    let _ = db.records_get_by(&ids).await.expect("批量查询失败");
    assert_eq!(db.round_trips() - r0, 1, "3 个 id 的批量应只 1 次往返");

    let r1 = db.round_trips();
    for id in &ids {
        let _ = db.record_get_by(*id).await.expect("逐条查询失败");
    }
    assert_eq!(
        db.round_trips() - r1,
        ids.len() as u64,
        "逐条路径应 N 次往返（对照）"
    );
    ts::drop_db(&dsn).await;
}

/// **牙②**：生产 SELCHR 后 DB 终态与回帧不变；往返 = 4（批量前为 6）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_selrt_b（自建自删）"]
async fn select_chr_keeps_db_state_and_uses_four_round_trips() {
    let dsn = ts::setup_db("mir2_test_selrt_b").await;
    let db = seed(&dsn, &["sb1", "sb2", "sb3"]).await;
    // 先把 sb1 置为"最近选择"，验证选择 sb2 时会清零它。
    {
        let id = db.record_index("sb1").await.expect("索引");
        let mut row = db
            .record_get_by(id)
            .await
            .expect("GetBy")
            .expect("行必须存在");
        row.selected = 1;
        db.record_update_by(&row).await.expect("置位写库失败");
    }
    let server = ts::test_server("selrt", &dsn);
    server
        .login
        .lock()
        .await
        .add_session(ACCOUNT.as_bytes().to_vec(), SESSION, Vec::new());
    let mut u = ts::gate_user("s1", ACCOUNT.as_bytes());
    u.session_no = SESSION;
    u.chr_queryed = true;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let body = format!("{ACCOUNT}/sb2");
    let r0 = db.round_trips();
    let a0 = db.acquires();
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
    let rounds = db.round_trips() - r0;
    let acquires = db.acquires() - a0;
    assert_eq!(
        rounds, 4,
        "SQL 往返应为 4（1 批量 + 2 更新 + 1 地图）；改回逐条 = 6 ⇒ 红；实际 {rounds}"
    );
    assert_eq!(
        acquires, 1,
        "整条选角必须只 acquire 一次（复用一条连接）；旧实现=4 ⇒ 红；实际 {acquires}"
    );

    let frames = ts::take_frames(&mut rx);
    assert_eq!(frames.len(), 1, "SELCHR 必须回一帧");
    let (head, route) = ts::decode_user_reply(&frames[0]);
    assert_eq!(head.ident, SM_STARTPLAY, "必须回 SM_STARTPLAY");
    assert_eq!(
        route, b"127.0.0.1/7200",
        "路由体必须与改动前逐字节相同（decode_user_reply 已解 EDCode）"
    );
    assert_eq!(selected_of(&db, "sb2").await, 1, "选中项置 1");
    assert_eq!(selected_of(&db, "sb1").await, 0, "旧选中项清零");
    assert_eq!(selected_of(&db, "sb3").await, 0, "其余不动");
    ts::drop_db(&dsn).await;
}

/// **牙③**：SQL 全失败（池关闭）⇒ 批量失败回退逐条，仍与旧实现同形：不回包、错误上抛。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_selrt_c（自建自删）"]
async fn select_chr_falls_back_to_per_row_when_batch_query_fails() {
    let dsn = ts::setup_db("mir2_test_selrt_c").await;
    let db = seed(&dsn, &["sc1"]).await;
    let server = ts::test_server("selrtc", &dsn);
    server
        .login
        .lock()
        .await
        .add_session(ACCOUNT.as_bytes().to_vec(), SESSION, Vec::new());
    let mut u = ts::gate_user("s1", ACCOUNT.as_bytes());
    u.session_no = SESSION;
    u.chr_queryed = true;
    // 先把 sc1 置成「最近选择」，关闭池后断言它**没有被改动**（A 案的护栏牙：
    // 池关闭 ⇒ 无回包 + SelectID 不变。此牙在新旧实现下都绿，不是等价性红证）。
    {
        let id = db.record_index("sc1").await.expect("索引");
        let mut row = db
            .record_get_by(id)
            .await
            .expect("GetBy")
            .expect("行必须存在");
        row.selected = 1;
        db.record_update_by(&row).await.expect("置位写库失败");
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    // 关池：唯一 acquire 失败 ⇒ 一条语句都不执行。
    db.pool.close().await;
    let body = format!("{ACCOUNT}/sc1");
    let err = crate::handler::dispatch(
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
    .expect_err("SQL 全失败时旧实现同样以 Err 终止（不回包）");
    let text = format!("{err:#}");
    assert!(
        text.contains("取数据库连接失败") || text.contains("characters"),
        "错误必须来自取连接或 characters/索引查询：{text}"
    );
    assert!(ts::take_frames(&mut rx).is_empty(), "错误路径不得回帧");
    // 原 pool 已关闭 ⇒ 用同一个库开一条新连接读后态（不是重新走业务路径）。
    let db2 = crate::testsupport::open_db(&dsn).await;
    assert_eq!(
        selected_of(&db2, "sc1").await,
        1,
        "acquire 失败 ⇒ SelectID 不得被改动（A 案护栏牙；新旧实现都绿）"
    );
    ts::drop_db(&dsn).await;
}

/// **牙④（card/selectchr-roundtrip 增量）**：生产 `CM_QUERYCHR` 同样整条复用一条连接。
///
/// 旧实现每条 SQL 各取一次连接：3 个角色 ⇒ 3 次 `GetBy` + 至多 2 次 `GetQryChar` = 5 次
/// acquire；复用后 `acquires == 1`，SQL 条数仍为 5（`round_trips` 不变）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "需要本机 MySQL；库名 mir2_test_selrt_q（自建自删）"]
async fn query_chr_reuses_one_connection() {
    let dsn = ts::setup_db("mir2_test_selrt_q").await;
    let db = seed(&dsn, &["qa1", "qa2", "qa3"]).await;
    let server = ts::test_server("selrtq", &dsn);
    server
        .login
        .lock()
        .await
        .add_session(ACCOUNT.as_bytes().to_vec(), SESSION, Vec::new());
    let mut u = ts::gate_user("s1", ACCOUNT.as_bytes());
    u.session_no = SESSION;
    u.chr_tick = 0;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    // 前置：内存索引（characters_indexes）与 name_quick（characters）都能查到这三个名字。
    for name in ["qa1", "qa2", "qa3"] {
        let idx = db.record_index(name).await;
        assert!(idx.is_some(), "前置：characters_indexes 应有 {name}");
        let row = db.records_get_by(&[idx.unwrap()]).await.expect("批量查询");
        assert!(
            row.values().all(|r| !r.deleted),
            "前置：{name} 不得是 deleted"
        );
        assert!(
            db.data_index(name).await.is_some(),
            "前置：characters 的 name_quick 应有 {name}"
        );
    }
    // QUERYCHR 有 200ms 限频门：先触发一次 now_ms，避免 chr_tick=0 与 now≈0 误判。
    let _ = crate::handler::now_ms();
    let body = format!("{ACCOUNT}/{SESSION}");
    let a0 = db.acquires();
    let r0 = db.round_trips();
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
    let acquires = db.acquires() - a0;
    let rounds = db.round_trips() - r0;
    assert_eq!(
        acquires, 1,
        "整条 QUERYCHR 必须只 acquire 一次；旧实现=5 ⇒ 红；实际 {acquires}"
    );
    assert_eq!(
        rounds, 5,
        "3 个角色 + 至多 2 次 GetQryChar = 5 条 SQL；实际 {rounds}"
    );
    let frames = ts::take_frames(&mut rx);
    assert_eq!(frames.len(), 1, "QUERYCHR 必须回一帧");
    let (head, _body) = ts::decode_user_reply(&frames[0]);
    assert_eq!(head.ident, SM_QUERYCHR, "必须回 SM_QUERYCHR");
    ts::drop_db(&dsn).await;
}
