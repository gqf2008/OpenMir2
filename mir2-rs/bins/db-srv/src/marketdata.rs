//! DBSrv **5700 `MarketService` 面**的 MySQL 存储层：`IMarketStorage`
//! （C# `Storeages/DBSvr.Storage.MySQL/MarketStoageService.cs`）。
//!
//! 逐条 1:1 照搬 C#（含既有 bug 与空桩）：
//! - `QueryMarketItems(groupId)`：`select a.*,b.* from marketItems a left JOIN marketitem b on a.Id=b.MarketId
//!   where a.SellState=1 [and GroupId=@GroupId]`；C# 用 `if (dr.Read())` ⇒ **只读第一行**（行序即
//!   结果），其余行被丢弃；`Desc[i]=Value{i}` 只填 0..13（`Desc[13]` 留 0）；把 `Value{i}` 读到
//!   `Desc[i]`（不是 `Desc[i+1]`）。
//! - `QueryMarketItemsCount`：C# **恒 `return 0`**（不查库），照搬。
//! - `SaveMarketItem`：`INSERT INTO marketitems (...)`；列名/参数与 C# 一字不差，`MarketName` 恒 `"Market"`，
//!   异常 `catch` 后 `return false`（Rust 同：`Err` 记日志后 `Ok(false)`）。
//! - `SearchMarketItems`：四个分支（`SearchMarketNameItems`/`SearchMarketItemNameItems`/
//!   `SearchMarketItemSetItems`/`SearchMarketItemTypeItems`）**全是空桩**（`return new List<>()`）⇒ 恒空。
//!   另有 C# 调用方**实参错位**：调用 `SearchMarketItems(GroupId, MarketName, SearchItem, SearchWho, ...)`
//!   对应形参 `(groupId, marketName, sellWho, itemName, ...)` ⇒ `sellWho` 实际收到 `SearchItem`、
//!   `itemName` 实际收到 `SearchWho`。因全为空桩，行为仍恒空，但保持形参名以便对读。
//!
//! **现网表缺失**：`mir2_db` 里**没有** `marketitems`/`marketitem` 表（已用真身探针核实），
//! 故 C# `QueryMarketItems` 会 SQL 报错 —— 该异常不被 `MarketStoageService` 捕获，冒到
//! `MarketService.Received` 的 try/catch ⇒ 只记日志、不回包。Rust 侧同（`market.rs` 记 `Err`）。
//! 存储 SQL 的**语义**由本模块的 `#[ignore]` 单元测试在**独立临时库**上驱动（不动 `mir2_db`）。

use anyhow::{Context, Result};
use sqlx::{MySqlConnection, Row};

use crate::db::{int_col, resolve_col, Db};

/// 拍卖行条目（C# `OpenMir2.Data.MarketItem`，本实现只保留 5700 存储用到的部分）。
///
/// 注意 C# 侧 `MarketItem : ClientPacket` **没有** `[MemoryPackable]`，只在
/// `MarketSaveDataItem.Item` 上挂了 `[MemoryPackAllowSerialize]`；真身探针证明任何
/// 非空 `MarketItem` 的序列化/反序列化都抛 `MemoryPackSerializationException`。
/// 所以本结构**不参与线上编解码**，仅承载存储层数据。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarketItem {
    /// `MarketItem.Index`（DB 列 `SellIndex`）。
    pub index: i32,
    /// `MarketItem.SellWho`（DB 列 `SellName`）。
    pub sell_who: String,
    /// `MarketItem.SellPrice`（DB 列 `SellPrice`）。
    pub sell_price: i32,
    /// `MarketItem.SellDate`（DB 列 `SellDate`）。
    pub sell_date: String,
    /// `MarketItem.SellState`（DB 列 `SellState`）。
    pub sell_state: u8,
    /// `MarketItem.SellItem.MakeIndex`（DB 列 `MaekeIndex`，C# 拼写如此）。
    pub make_index: i32,
    /// `MarketItem.SellItem.Dura`（DB 列 `Dura`）。
    pub dura: u16,
    /// `MarketItem.SellItem.DuraMax`（DB 列 `DuraMax`）。
    pub dura_max: u16,
    /// `MarketItem.SellItem.Desc`（DB 列 `Value0..Value12`；`Desc[13]` C# 从不赋值）。
    pub desc: [u8; 14],
}

/// C# `QueryMarketItems` 的 `select a.*,b.*`（按组）。
const SQL_GROUP: &str =
    "select a.*,b.* from marketItems a left JOIN marketitem b on a.Id=b.MarketId \
                         where a.SellState=1 and GroupId=?";
/// C# `QueryMarketItems` 的 `sSqlStringAll`（`GroupId==0` 查所有组）。
const SQL_ALL: &str =
    "select a.*,b.* from marketItems a left JOIN marketitem b on a.Id=b.MarketId where a.SellState=1";
/// C# `SaveMarketItem` 的 INSERT（`StringBuilder.AppendLine` 原样三行）。
const SQL_INSERT: &str =
    "INSERT INTO marketitems (GroupId, ServerIndex, SellIndex, SellState, MarketName, ItemType, ItemSet, \
     ItemName, MakeIndex, SellName, SellPrice, SellDate) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

fn text_col(row: &sqlx::mysql::MySqlRow, name: &str) -> Result<String> {
    let name = resolve_col(row, name)?;
    row.try_get::<String, _>(name)
        .with_context(|| format!("列 {name} 不是文本类型（表结构与 C# 预期不符）"))
}

fn word_col(row: &sqlx::mysql::MySqlRow, name: &str) -> Result<u16> {
    u16::try_from(int_col(row, name)?)
        .with_context(|| format!("列 {name} 超出 ushort 范围（C# `GetUInt16` 同场景抛异常）"))
}

/// C# `MarketStoageService.QueryMarketItems` 把一行读成 `MarketItem`。
fn row_to_item(row: &sqlx::mysql::MySqlRow) -> Result<MarketItem> {
    let mut item = MarketItem {
        index: i32::try_from(int_col(row, "SellIndex")?).context("SellIndex 越界")?,
        sell_who: text_col(row, "SellName")?,
        sell_price: i32::try_from(int_col(row, "SellPrice")?).context("SellPrice 越界")?,
        sell_date: text_col(row, "SellDate")?,
        sell_state: u8::try_from(int_col(row, "SellState")?).context("SellState 越界")?,
        make_index: i32::try_from(int_col(row, "MaekeIndex")?).context("MaekeIndex 越界")?,
        dura: word_col(row, "Dura")?,
        dura_max: word_col(row, "DuraMax")?,
        ..MarketItem::default()
    };
    for i in 0..13 {
        let name = format!("Value{i}");
        item.desc[i] = u8::try_from(int_col(row, &name)?)
            .with_context(|| format!("列 {name} 超出 byte 范围（C# `GetByte` 同场景抛异常）"))?;
    }
    Ok(item)
}

async fn query_rows(conn: &mut MySqlConnection, group_id: u8) -> Result<Vec<MarketItem>> {
    let rows = if group_id == 0 {
        sqlx::query(SQL_ALL).fetch_all(&mut *conn).await
    } else {
        sqlx::query(SQL_GROUP)
            .bind(group_id)
            .fetch_all(&mut *conn)
            .await
    }
    .context("查询 marketItems 失败（表缺失/列不符与 C# 同）")?;
    // C# `if (dr.Read())`：**只取第一行**。
    rows.first()
        .map_or_else(|| Ok(Vec::new()), |r| Ok(vec![row_to_item(r)?]))
}

impl Db {
    /// C# `MarketStoageService.QueryMarketItems`。
    ///
    /// # Errors
    /// 表缺失/列不符时返回错误（C# 同场景抛、被上层 `Received` 吞成「不回包」）。
    pub async fn market_query_items(&self, group_id: u8) -> Result<Vec<MarketItem>> {
        let mut conn = self.pool.acquire().await.context("取连接失败")?;
        query_rows(&mut conn, group_id).await
    }

    /// C# `MarketStoageService.QueryMarketItemsCount`：**恒 0**（不查库，C# 原样）。
    // 保留 `&self` 以对齐 C# `IMarketStorage` 实例方法（同 `QueryMarketItems`/`SaveMarketItem`）。
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn market_query_items_count(&self, _group_id: u8, _sell_who: &str) -> i32 {
        0
    }

    /// C# `MarketStoageService.SaveMarketItem`。
    ///
    /// # Errors
    /// 连接失败时返回错误；INSERT 失败按 C# 口径记日志并返回 `Ok(false)`。
    pub async fn market_save_item(
        &self,
        market: &MarketItem,
        group_id: u8,
        server_index: u8,
    ) -> Result<bool> {
        let mut conn = self.pool.acquire().await.context("取连接失败")?;
        let res = sqlx::query(SQL_INSERT)
            .bind(group_id)
            .bind(server_index)
            .bind(market.index) // @SellIndex
            .bind(market.sell_state)
            .bind("Market") // @MarketName 恒 "Market"（C# 原样）
            .bind(0_i16) // @ItemType：C# 传 `market.SellItem.Item.ItemType`，本模型无 StdItem
            .bind(0_u16) // @ItemSet：同上
            .bind("") // @ItemName：同上
            .bind(market.make_index) // @MakeIndex
            .bind(market.sell_who.as_str())
            .bind(market.sell_price)
            .bind(market.sell_date.as_str())
            .execute(&mut *conn)
            .await;
        match res {
            Ok(_) => Ok(true),
            Err(e) => {
                // C# `catch` → `LogService.Error` → `return false`。
                crate::log::error(&format!(
                    "[Exception] PlayDataStorage.CreateCharacters: {e:#}"
                ));
                Ok(false)
            }
        }
    }

    /// C# `MarketStoageService.SearchMarketItems`（四个分支全空桩 ⇒ 恒空，且不查库）。
    ///
    /// 形参名按 C# 接口 `(groupId, marketName, sellWho, itemName, itemType, itemSet)`；
    /// 调用方 `MarketService.SearchMarketItem` 实参错位（见模块头注释）。
    // 保留 `&self` 以对齐 C# `IMarketStorage` 实例方法。
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn market_search_items(
        &self,
        _group_id: u8,
        _market_name: &str,
        _sell_who: &str,
        _item_name: &str,
        _item_type: i16,
        _item_set: u8,
    ) -> Vec<MarketItem> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_storage::LegacyDsn;
    use sqlx::MySqlPool;

    fn hex(b: &[u8]) -> String {
        use std::fmt::Write as _;
        let mut s = String::with_capacity(b.len() * 2);
        for x in b {
            let _ = write!(s, "{x:02x}");
        }
        s
    }

    /// 独立临时库名（进程号 + 调用计数，避免并行用例互踩）。
    fn scratch_db() -> String {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        format!(
            "mir2_dbsrv6k_mkt_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        )
    }

    /// 真 **MySQL** 存储层验收（`#[ignore]`，需本机 MySQL；用独立临时库，**不碰 `mir2_db`**）。
    ///
    /// 建最小 `marketitems`/`marketitem` 表（列名取 C# SQL 实际读写的列），种 2 行在售数据，
    /// 断言：`QueryMarketItems` **只回第一行**（C# `if (dr.Read())` 怪癖）、`SaveMarketItem` 落库、
    /// `QueryMarketItemsCount` 恒 0、`SearchMarketItems` 恒空。
    ///
    /// 跑法：`cargo test -p db-srv --offline -- --ignored marketdata::`
    #[tokio::test]
    #[ignore = "需本机 MySQL（独立临时库 mir2_dbsrv6k_mkt_*，不碰 mir2_db）"]
    async fn market_storage_matches_csharp_quirks() {
        let db_name = scratch_db();
        let dsn = LegacyDsn {
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: String::new(),
            database: "mysql".into(),
        };
        let admin = MySqlPool::connect(&dsn.to_sqlx_url())
            .await
            .expect("连接 MySQL 失败");
        sqlx::query(&format!("CREATE DATABASE {db_name}"))
            .execute(&admin)
            .await
            .expect("建临时库失败");
        let scratch_dsn = LegacyDsn {
            database: db_name.clone(),
            ..dsn
        };
        let db = Db::connect(&scratch_dsn).await.expect("连临时库失败");
        sqlx::query(
            "CREATE TABLE marketitems (\
                Id INT AUTO_INCREMENT PRIMARY KEY, GroupId INT NOT NULL, ServerIndex INT NOT NULL, \
                SellIndex INT NOT NULL, SellState INT NOT NULL, MarketName VARCHAR(64), ItemType SMALLINT, \
                ItemSet SMALLINT, ItemName VARCHAR(64), MakeIndex INT, SellName VARCHAR(64), \
                SellPrice INT, SellDate VARCHAR(32))",
        )
        .execute(&db.pool)
        .await
        .expect("建 marketitems 失败");
        sqlx::query(
            "CREATE TABLE marketitem (\
                MarketId INT NOT NULL, MaekeIndex INT, Dura SMALLINT, DuraMax SMALLINT, \
                Value0 TINYINT, Value1 TINYINT, Value2 TINYINT, Value3 TINYINT, Value4 TINYINT, \
                Value5 TINYINT, Value6 TINYINT, Value7 TINYINT, Value8 TINYINT, Value9 TINYINT, \
                Value10 TINYINT, Value11 TINYINT, Value12 TINYINT)",
        )
        .execute(&db.pool)
        .await
        .expect("建 marketitem 失败");
        // 两行在售：C# 只取第一行（`Id=1`）。
        for (id, sell_index, name) in [(1_i32, 11_i32, "first"), (2, 22, "second")] {
            sqlx::query("INSERT INTO marketitems (Id, GroupId, ServerIndex, SellIndex, SellState, MarketName, ItemType, ItemSet, ItemName, MakeIndex, SellName, SellPrice, SellDate) VALUES (?, 2, 1, ?, 1, 'Market', 0, 0, '', ?, ?, 1000, '0312311210')")
                .bind(id)
                .bind(sell_index)
                .bind(300 + sell_index)
                .bind(name)
                .execute(&db.pool)
                .await
                .expect("种 marketitems 失败");
            sqlx::query("INSERT INTO marketitem (MarketId, MaekeIndex, Dura, DuraMax, Value0, Value1, Value2, Value3, Value4, Value5, Value6, Value7, Value8, Value9, Value10, Value11, Value12) VALUES (?, 700, 5, 6, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8)")
                .bind(id)
                .execute(&db.pool)
                .await
                .expect("种 marketitem 失败");
        }

        let items = db.market_query_items(2).await.expect("QueryMarketItems");
        assert_eq!(items.len(), 1, "C# `if (dr.Read())` ⇒ 只回第一行");
        let first = &items[0];
        assert_eq!(first.index, 11);
        assert_eq!(first.sell_who, "first");
        assert_eq!(first.make_index, 700);
        assert_eq!(first.dura, 5);
        assert_eq!(first.dura_max, 6);
        assert_eq!(first.desc[0], 9);
        assert_eq!(first.desc[12], 8);
        assert_eq!(first.desc[13], 0, "C# 只填 Value0..Value12");

        // 无匹配组 ⇒ 空（C# 同）。
        assert!(db.market_query_items(9).await.unwrap().is_empty());

        // 保存一行，再查第一行应变成新行（Id 递增 ⇒ 若 C# 只取第一行，会看不到新行）。
        let save = MarketItem {
            index: 33,
            sell_who: "saver".into(),
            sell_price: 2,
            sell_date: "0401010101".into(),
            sell_state: 1,
            make_index: 900,
            dura: 7,
            dura_max: 8,
            ..MarketItem::default()
        };
        assert!(db.market_save_item(&save, 2, 1).await.unwrap());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM marketitems")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 3, "INSERT 落库");

        // 恒 0 / 恒空（C# 空桩）。
        assert_eq!(db.market_query_items_count(2, "first"), 0);
        assert!(db
            .market_search_items(2, "Market", "first", "", 1, 0)
            .is_empty());

        // 收尾：删临时库（失败不 panic，避免掩盖上面的断言）。
        let _ = sqlx::query(&format!("DROP DATABASE {db_name}"))
            .execute(&admin)
            .await;
        eprintln!(
            "[marketdata] scratch db dropped: {db_name} hex={}",
            hex(db_name.as_bytes())
        );
    }
}
