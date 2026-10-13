//! **M3⑩-实现 2 判据**：`goldsales` 只读加载 —— 「表里有数据仍加载 0 行」。
//!
//! oracle `MySqlDB.LoadSellOffItemList`（`src/GameSrv/DB/MySqlDB.cs:251-291`）的加载器读
//! `DealChrName`/`BuyChrName`，而 DDL 的列名是 `DealCharName`/`BuyCharName`
//! （`sql/mir2_data.sql:26-27`）⇒ `GetString` 恒空 ⇒ `MySqlDB.cs:270` 的过滤条件把所有行跳过
//! ⇒ **即使表里有数据，也恒加载 0 行**（白名单 `D-5`/`B-5`）。本用例把这条缺陷钉成牙：
//!
//! 1. 在同一会话里 `CREATE TEMPORARY TABLE goldsales`（**遮蔽**真表；连接断开自动消失，
//!    **不改冻结 schema、不持久写任何数据**），按 DDL 建列；
//! 2. 按**正确**列名 `DealCharName`/`BuyCharName` 插一行 ⇒ `SELECT COUNT(*) = 1`（表里确有数据）；
//! 3. 走生产加载器 [`mir2_data::loaders::DataLoaders::load_sell_off_item_list`] ⇒ 断言 **0 行**。
//!
//! ⇒ **改坏必红**：把加载器读的列名改成 DDL 的正确名（`DealCharName`/`BuyCharName`）后，
//! 第 3 步会得到 1 行 ⇒ 本用例 FAILED。
//!
//! # 跑法（需本机 MySQL；非门禁，显式跑）
//!
//! ```text
//! cargo test -p game-svr --test goldsales_load -- --ignored --nocapture
//! ```
//!
//! 连接串取环境变量 `MIR2_DATA_URL`，缺省与现网 `Server.conf` 的 `ConnctionString` 同源
//! （`mysql://root@127.0.0.1:3306/mir2_data`）。
//!
//! **零端口**：不绑任何游戏端口；只连本机已运行的 MySQL（WRN：不起栈）。

use mir2_data::DataLoaders;

/// 按 `sql/mir2_data.sql:24-33` 建 `goldsales`（列名照 DDL：`DealCharName`/`BuyCharName`）。
const DDL: &str = "CREATE TEMPORARY TABLE goldsales (\
     `Id` bigint(20) NOT NULL AUTO_INCREMENT,\
     `DealCharName` varchar(50) NULL DEFAULT NULL,\
     `BuyCharName` varchar(50) NULL DEFAULT NULL,\
     `SellDateTime` datetime NULL DEFAULT NULL,\
     `State` tinyint(1) NULL DEFAULT NULL,\
     `SellGold` int(11) NULL DEFAULT NULL,\
     `UseItems` json NULL,\
     PRIMARY KEY (`Id`)\
     ) ENGINE = InnoDB DEFAULT CHARSET = utf8";

#[tokio::test]
#[ignore = "需要本机 MySQL mir2_data；用会话级 TEMPORARY TABLE 遮蔽真表，零持久写入"]
async fn table_has_data_but_loader_still_reads_zero_rows() {
    let url = std::env::var("MIR2_DATA_URL")
        .unwrap_or_else(|_| "mysql://root@127.0.0.1:3306/mir2_data".to_string());
    // max_connections = 1：临时表是**会话级**的，必须保证建表与加载走同一条连接。
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("连接 MySQL（MIR2_DATA_URL）失败");

    sqlx::query(DDL)
        .execute(&pool)
        .await
        .expect("建 TEMPORARY TABLE goldsales 失败");
    // 用 DDL 的**正确**列名插一行（模拟「表里有数据」；`DealChrName`/`BuyChrName` 是本仓库
    // 加载器读的错列名，Oracle/MySQL 都会把它们当不存在 ⇒ 恒空）。
    sqlx::query(
        "INSERT INTO goldsales (DealCharName, BuyCharName, SellDateTime, State, SellGold, UseItems) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("卖家")
    .bind("买家")
    .bind("2026-10-13 01:02:03")
    .bind(0)
    .bind(500)
    .bind("[]")
    .execute(&pool)
    .await
    .expect("插入 goldsales 行失败");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM goldsales")
        .fetch_one(&pool)
        .await
        .expect("COUNT(*) 失败");
    println!("遮蔽会话里 goldsales 行数 = {count}");
    assert_eq!(count, 1, "前置：表里确实有 1 行（否则本判据空洞）");

    // 生产加载器（D-5/B-5：读错列名 ⇒ 恒 0 行）。
    let loaded = DataLoaders::new(pool, 10)
        .load_sell_off_item_list()
        .await
        .expect("load_sell_off_item_list 失败");
    println!("load_sell_off_item_list 返回行数 = {}", loaded.len());
    assert_eq!(
        loaded.len(),
        0,
        "读错列名 ⇒ 表里有 1 行也必须加载 0 行（D-5/B-5）"
    );
}
