//! 物品模板表（`stditems`）的**加载段**——oracle `GameApp.Initialize()` 直连 MySQL 读 `stditems`
//! （`MySqlDB.LoadItemsDB`，`src/GameSrv/DB/MySqlDB.cs:13`）。
//!
//! # 查表口径已收敛到 `game_svr::ability`
//!
//! 本模块**只保留 DB 载入**：`load_items` 返回 `SELECT * FROM stditems` 的**顺序表**，
//! `catalog_from` 把它变成 [`game_svr::ability::StdItemCatalog`]（= oracle `GameItemSystem`）。
//! `Index → StdItem.Shape` / `GetStdItem` 的查表与 1-based 语义**只有一份**，在 `ability.rs`
//! （`openmir2-m3-bag-equip-alignment` 的能力接缝，owner 裁决为保留方；本片按 `brief-m2svr-next.md`
//! §18 把 m2-svr 侧原来的第二份查表删除）。
//!
//! # 1-based 口径（为什么 `catalog_from` 下标从 1 起）
//!
//! `GameItemSystem.GetStdItem(ushort nItemIdx)`（`GameItemSystem.cs:20-33`）`nItemIdx -= 1` 后索引
//! `StdItemList`；而 `AddItem` 赋的 `Index = i + 1`（`GameItemSystem.cs:133`）⇒ **Index = 表内位置 + 1**。
//! 空名记录按 oracle 的 `if (string.IsNullOrEmpty(result.Name)) result = null`（`:28-31`）**不放进目录**
//! （`StdItemCatalog::get` 不做空名判定，见 `ability.rs`）⇒ 入目录前先滤掉空名，位置仍按原下标 + 1。
//!
//! 加载失败 / 表为空 ⇒ `catalog` 空 ⇒ `GetStdItem` 恒 `null`（oracle `LoadItemsDB` 失败分支，
//! `GameApp.cs:111-115`）；调用方据此降级 + WRN，**不伪造**任何 Shape。

use game_svr::ability::StdItemCatalog;
use mir2_data::loaders::DataLoaders;
use mir2_data::StdItem;
use mir2_storage::LegacyDsn;

/// oracle `GameSvrConf.ConnctionString` 的源码缺省（`GameSvrConf.cs:1921`）。
pub const DEFAULT_CONN_STRING: &str = "server=127.0.0.1;uid=root;pwd=;database=mir2;";

/// 直连 MySQL 读 `stditems`（`MySqlDB.LoadItemsDB`，`src/GameSrv/DB/MySqlDB.cs:13`）。
///
/// 连接串走 [`LegacyDsn`]（与 db-srv/login-srv 同一份解析，勿再手写一份）。失败只返回错误，
/// **不 panic** —— 调用方按 oracle 的「加载失败 ⇒ 空物品系统」降级。
///
/// # Errors
/// 连接串解析失败、连库失败、`SELECT * FROM stditems` 失败时返回人话错误。
pub async fn load_items(conn: &str) -> Result<Vec<StdItem>, String> {
    let dsn = LegacyDsn::parse(conn).map_err(|e| format!("ConnctionString 解析失败：{e}"))?;
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&dsn.to_sqlx_url())
        .await
        .map_err(|e| format!("连接 MySQL 失败（读 stditems）：{e}"))?;
    // `monster_power_rate` 只被 `load_monster_db` 用，物品表不读它；照 tests 的口径传 10。
    DataLoaders::new(pool, 10)
        .load_items_db()
        .await
        .map_err(|e| format!("加载 stditems 失败：{e}"))
}

/// 把 `LoadItemsDB` 的**顺序表**装成 oracle `GameItemSystem` 的查表目录。
///
/// - **1-based**：表内位置 `i` ⇒ `Index = i + 1`（`GameItemSystem.cs:133` 的 `item.Index = (ushort)(i + 1)`）；
/// - **空名按缺**：`Name` 为空的记录不进目录（oracle `GetStdItem` 的空名分支，`GameItemSystem.cs:28-31`）；
/// - 超过 `u16` 表（`Index` 溢出）的尾部记录一律丢弃（oracle 侧 `Index` 也是 `ushort`）。
#[must_use]
pub fn catalog_from(items: &[StdItem]) -> StdItemCatalog {
    let entries = items.iter().enumerate().filter_map(|(i, item)| {
        if item.name.is_empty() {
            return None;
        }
        let index = u16::try_from(i + 1).ok()?;
        Some((index, item.clone()))
    });
    StdItemCatalog::from_items(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_svr::ability::ItemCatalog;

    fn item(name: &str, shape: u8) -> StdItem {
        StdItem {
            name: name.to_string(),
            shape,
            ..StdItem::default()
        }
    }

    /// 1-based + 空名按缺：`Index=1` ⇒ `items[0]`；空名记录处的位置仍占号。
    #[test]
    fn catalog_is_one_based_and_skips_empty_names() {
        let table = vec![item("木剑", 5), item("", 9), item("布衣", 3)];
        let catalog = catalog_from(&table);
        assert_eq!(catalog.shape(1), Some(5), "Index=1 ⇒ items[0]（木剑）");
        assert_eq!(catalog.shape(2), None, "空名记录按 oracle GetStdItem 计缺");
        assert_eq!(
            catalog.shape(3),
            Some(3),
            "空名记录仍占位置：Index=3 ⇒ items[2]"
        );
        assert_eq!(catalog.shape(0), None, "Index=0 越界 ⇒ None");
        assert_eq!(catalog.shape(4), None, "越界 ⇒ None");
        assert_eq!(catalog_from(&[]).shape(1), None, "空表 ⇒ 恒 None");
    }
}
