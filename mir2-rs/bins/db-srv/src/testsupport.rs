//! 失败注入测试的共享夹具（`#[cfg(test)]` 专用，不进发布二进制）。
//!
//! 做法沿用 `data.rs::save_failure_tests`：每个用例自建一个 `mir2_test_*` 库
//! （`sql/mir2_db.sql` 的 oracle DDL，自建自删），用 `pool.close()` / `SIGNAL` 触发器做
//! **构造级**故障注入，再直接调用生产函数断言帧/库后态。
//!
//! 跑法（需本机 MySQL）：
//! `cargo test -p db-srv --bins load_fault_tests -- --ignored --nocapture`

#![allow(clippy::doc_markdown)]

use std::path::Path;
use std::sync::Arc;

use mir2_protocol::edcode;
use mir2_protocol::frame::CommandMessage;
use mir2_protocol::frame::ServerDataPacket;
use mir2_protocol::internal::{ServerDataMessage, ServerDataType};
use mir2_storage::LegacyDsn;
use sqlx::mysql::MySqlConnection;
use sqlx::Connection;

use crate::cache::CacheStore;
use crate::config;
use crate::db::Db;
use crate::route::{GateRoute, Share};
use crate::server::{GateUser, Server};

/// `sql/mir2_db.sql` 里本面用到的表（与 `data.rs::save_failure_tests` 同）。
pub const TABLES: [&str; 10] = [
    "characters_indexes",
    "characters",
    "characters_ablity",
    "characters_bonusability",
    "characters_item",
    "characters_bagitem",
    "characters_storageitem",
    "characters_item_attr",
    "characters_magic",
    "characters_status",
];

pub fn ddl_for(dump: &str, table: &str) -> String {
    let marker = format!("CREATE TABLE `{table}` ");
    let start = dump
        .find(&marker)
        .unwrap_or_else(|| panic!("{table} 建表语句缺失"));
    let rest = &dump[start..];
    let engine = rest.find(") ENGINE").expect("建表语句缺 ENGINE 尾");
    let semi = rest[engine..].find(';').expect("建表语句缺分号");
    rest[..=engine + semi].to_string()
}

/// 建 `mir2_test_*` 库 + 10 张表（拒绝写非测试库）。
pub async fn setup_db(database: &str) -> LegacyDsn {
    let dsn = LegacyDsn::parse(&format!(
        "server=127.0.0.1;uid=root;pwd=;database={database};"
    ))
    .expect("测试 DSN 非法");
    assert!(
        dsn.database.starts_with("mir2_test_"),
        "拒绝写非测试库：{}",
        dsn.database
    );
    let admin = LegacyDsn {
        database: "mysql".to_string(),
        ..dsn.clone()
    };
    let mut c = MySqlConnection::connect(&admin.to_sqlx_url())
        .await
        .expect("连测试 MySQL 失败（本测试需本机 MySQL）");
    sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", dsn.database))
        .execute(&mut c)
        .await
        .expect("删测试库失败");
    sqlx::query(&format!(
        "CREATE DATABASE `{}` CHARACTER SET utf8 COLLATE utf8_general_ci",
        dsn.database
    ))
    .execute(&mut c)
    .await
    .expect("建测试库失败");
    let dump = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../sql/mir2_db.sql"
    ))
    .expect("读 sql/mir2_db.sql 失败");
    let mut c = MySqlConnection::connect(&dsn.to_sqlx_url())
        .await
        .expect("连测试库失败");
    for table in TABLES {
        let ddl = ddl_for(&dump, table);
        sqlx::query(&ddl)
            .execute(&mut c)
            .await
            .unwrap_or_else(|e| panic!("建表 {table} 失败: {e}"));
    }
    dsn
}

pub async fn drop_db(dsn: &LegacyDsn) {
    let admin = LegacyDsn {
        database: "mysql".to_string(),
        ..dsn.clone()
    };
    let mut c = MySqlConnection::connect(&admin.to_sqlx_url())
        .await
        .expect("连管理库失败");
    sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", dsn.database))
        .execute(&mut c)
        .await
        .expect("清理测试库失败");
}

pub async fn conn(dsn: &LegacyDsn) -> MySqlConnection {
    MySqlConnection::connect(&dsn.to_sqlx_url())
        .await
        .expect("连测试库失败")
}

/// 建好 `Db`（已载入内存索引）并返回。
pub async fn open_db(dsn: &LegacyDsn) -> Db {
    let mut db = Db::connect(dsn).await.expect("连测试库失败");
    db.load_quick_lists().await.expect("载入索引失败");
    db
}

/// 写一份合法 `dbsvr.conf`（端口只是占位，测试不开监听）。
fn write_conf(dir: &Path, dsn: &LegacyDsn) {
    let body = format!(
        "[Setup]\nShowDebug=0\nServerAddr=127.0.0.1\nServerPort=6000\nGateAddr=0.0.0.0\nGatePort=5100\n\
         ServerName=LegendOfMir2\nDenyChrName=1\nDELMaxLevel=30\nDynamicIPMode=0\n\n\
         [Server]\nIDSAddr=127.0.0.1\nIDSPort=5600\nMarketAddr=127.0.0.1\nMarketPort=5700\n\n\
         [DataBase]\nStoreage=MySQL\nConnctionString=server=127.0.0.1;uid=root;pwd=;database={};\n\n\
         [DBClear]\nInterval=3000\n",
        dsn.database
    );
    std::fs::write(dir.join("dbsvr.conf"), body).expect("写 dbsvr.conf 失败");
}

/// 建 `Server`（登录会话由调用方 `add_session`）。
///
/// `ServerInfo.txt` 的行表构造为一条路由：`127.0.0.1 → 127.0.0.1:7200`
/// （选角用例断言 `SM_STARTPLAY` 的路由体 `127.0.0.1/7200`，与 `live_querychr.rs` 同）。
pub fn test_server(tag: &str, dsn: &LegacyDsn) -> Arc<Server> {
    let dir = std::env::temp_dir().join(format!("a2-dbsrv-ut-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建 temp 目录失败");
    write_conf(&dir, dsn);
    let cfg = config::load(&dir.join("dbsvr.conf")).expect("解析测试配置失败");
    let share = Share {
        server_ips: Vec::new(),
        gate_ids: Vec::new(),
        routes: vec![GateRoute {
            sel_gate_ip: b"127.0.0.1".to_vec(),
            game_gate_ip: vec![b"127.0.0.1".to_vec()],
            game_gate_port: vec![7200],
        }],
        map_list: Vec::new(),
        deny_names: Vec::new(),
    };
    let cache = Arc::new(tokio::sync::Mutex::new(CacheStore::default()));
    let (server, _rx) = Server::new(cfg, Arc::new(share), cache);
    server
}

/// 一条测试用会话（C# `SessionUserInfo`）。
pub fn gate_user(socket_id: &str, account: &[u8]) -> GateUser {
    GateUser {
        socket_id: socket_id.to_string(),
        account: account.to_vec(),
        session_no: 0,
        text: Vec::new(),
        chr_queryed: false,
        chr_selected: false,
        chr_tick: 0,
        tick34: 0,
        sel_gate_id: 0,
        user_ip: b"127.0.0.1".to_vec(),
        gate_ip: b"127.0.0.1".to_vec(),
    }
}

/// 解析一条 5100 下行（内部 `ServerDataMessage` wire → `#`+头+体+`!`）。
pub fn decode_user_reply(wire: &[u8]) -> (CommandMessage, Vec<u8>) {
    // `ServerDataMessage::to_wire()` = 6B `ServerDataPacket` 头 + MemoryPack 体。
    let pkt = mir2_protocol::frame::ServerDataPacket::from_bytes(&wire[..ServerDataPacket::SIZE])
        .expect("内部帧 6B 头解析失败");
    let msg =
        ServerDataMessage::from_bytes(&wire[ServerDataPacket::SIZE..]).expect("内部帧解析失败");
    assert_eq!(pkt.packet_len as usize, wire.len() - ServerDataPacket::SIZE);
    assert_eq!(msg.kind, ServerDataType::Data, "必须是 Data 帧");
    let f = &msg.data;
    assert_eq!(f.first().copied(), Some(b'#'), "下行帧必须以 # 开头");
    assert_eq!(f.last().copied(), Some(b'!'), "下行帧必须以 ! 结尾");
    let payload = &f[1..f.len() - 1];
    assert!(payload.len() >= 16, "下行帧载荷不足 16 字节");
    let head =
        CommandMessage::from_bytes(&edcode::decode(&payload[..16])).expect("下行消息头解析失败");
    let body = edcode::decode(&payload[16..]);
    (head, body)
}

/// 收完当前通道里已产生的所有下行帧。
pub fn take_frames(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    while let Ok(v) = rx.try_recv() {
        out.push(v);
    }
    out
}

/// 插一行 `characters_indexes`（`IsDeleted=0`），返回 `Id`。
pub async fn insert_index(dsn: &LegacyDsn, account: &str, name: &str) -> i64 {
    let mut c = conn(dsn).await;
    let res = sqlx::query(
        "INSERT INTO characters_indexes (Account, ChrName, SelectID, IsDeleted, CreateDate, ModifyDate) \
         VALUES (?, ?, 0, 0, now(), now())",
    )
    .bind(account)
    .bind(name)
    .execute(&mut c)
    .await
    .expect("插入 characters_indexes 失败");
    i64::try_from(res.last_insert_id()).expect("Id 越界")
}

/// 插一行 `characters`（最小列），返回 `Id`。
pub async fn insert_character(dsn: &LegacyDsn, account: &str, name: &str) -> i64 {
    let mut c = conn(dsn).await;
    let res = sqlx::query(
        "INSERT INTO characters (LoginID, ChrName, MapName, Level, Deleted, CREATEDATE, LASTUPDATE) \
         VALUES (?, ?, '0', 1, 0, now(), now())",
    )
    .bind(account)
    .bind(name)
    .execute(&mut c)
    .await
    .expect("插入 characters 失败");
    i64::try_from(res.last_insert_id()).expect("Id 越界")
}

/// 在 `table` 上挂一个「`Position = at` 时 SIGNAL」的 BEFORE INSERT 触发器。
///
/// 复刻「第 N 条空槽 INSERT 失败」：`CreateUseItem`/`CreateBagItem` 从 0 递增，
/// `Position=4` 即第 5 条。`at < 0` 表示每条都失败。
pub async fn fail_insert_trigger(dsn: &LegacyDsn, trigger: &str, table: &str, at: i32) {
    let mut c = conn(dsn).await;
    let body = if at < 0 {
        "SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'inject_insert'".to_string()
    } else {
        format!(
            "BEGIN IF NEW.Position = {at} THEN SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'inject_insert'; END IF; END"
        )
    };
    let sql = format!("CREATE TRIGGER {trigger} BEFORE INSERT ON {table} FOR EACH ROW {body}");
    sqlx::query(&sql)
        .execute(&mut c)
        .await
        .unwrap_or_else(|e| panic!("建触发器 {trigger} 失败: {e}"));
}

/// 在 `table` 上挂一个「任何 UPDATE 都 SIGNAL」的 BEFORE UPDATE 触发器。
pub async fn fail_update_trigger(dsn: &LegacyDsn, trigger: &str, table: &str) {
    let mut c = conn(dsn).await;
    let sql = format!(
        "CREATE TRIGGER {trigger} BEFORE UPDATE ON {table} FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'inject_update'"
    );
    sqlx::query(&sql)
        .execute(&mut c)
        .await
        .unwrap_or_else(|e| panic!("建触发器 {trigger} 失败: {e}"));
}
