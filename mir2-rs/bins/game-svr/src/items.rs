//! 物品模板表（`stditems`）加载 + `Index → StdItem.Shape` 查表——帧 `50 SM_LOGON` / `41 SM_FEATURECHANGED`
//! 的装备形态输入（`openmir2-m2-enter-world-frames` 的 §15 缺口收口）。
//!
//! # oracle 链（逐行对齐）
//!
//! ```text
//! GameApp.Initialize()                                   src/GameSrv/GameApp.cs:110
//!   └─ GameShare.DataSource.LoadItemsDB()                src/GameSrv/DB/MySqlDB.cs:13
//!        └─ SystemShare.ItemSystem.Add(StdItem)          GameItemSystem.cs:66
//! PlayObject.GetFeature(null)                            PlayObject.cs:3395
//!   └─ SystemShare.ItemSystem.GetStdItem(Index)          GameItemSystem.cs:20
//!        └─ nItemIdx -= 1; if (0 <= nItemIdx < Count) …  （**1-based**：Index=1 取列表[0]）
//!             if (string.IsNullOrEmpty(item.Name)) null  （**空名也算取不到**）
//! ```
//!
//! 关键口径（都会改变结果，别"顺手简化"）：
//! 1. `GetStdItem` 是 **1-based**：`Index=1` ⇒ `StdItemList[0]`；`Index=0` 或越界 ⇒ `null`
//!    （`GameItemSystem.cs:20-33`）。调用方 `GetFeature` 另有 `Index > 0` 前置门。
//! 2. 命中记录若 `Name` 为空串 ⇒ 同样按 `null`（`GameItemSystem.cs:28-31`）。
//! 3. 加载失败 / 表为空 ⇒ `GetStdItem` 恒 `null` ⇒ 形态计 0（oracle `LoadItemsDB` 失败分支，
//!    `GameApp.cs:111-115`）；Rust 侧据此降级 + WRN，**不伪造**任何 Shape。
//!
//! 本模块只做查表与加载；`Shape → feature 位` 的位运算在 `enter::feature_shapes`（调用
//! `game_svr::feature::human_feature`），不在这里重复。

use mir2_data::loaders::DataLoaders;
use mir2_data::StdItem;
use mir2_storage::LegacyDsn;

/// `GameItemSystem.GetStdItem(ushort nItemIdx)`（`src/M2Server/Items/GameItemSystem.cs:20-35`）。
///
/// **1-based**：`Index=1` ⇒ `items[0]`；`Index=0`/越界/命中项 `Name` 为空 ⇒ `None`。
#[must_use]
pub fn get_std_item(items: &[StdItem], index: u16) -> Option<&StdItem> {
    // C# `nItemIdx -= 1`（ushort 下溢到 65535）后 `Count > nItemIdx` 恒假 ⇒ `Index==0` 也走 None。
    let zero_based = index.checked_sub(1)? as usize;
    let item = items.get(zero_based)?;
    if item.name.is_empty() {
        return None;
    }
    Some(item)
}

/// `UseItems[slot].Index` → `StdItem.Shape`（`PlayObject.GetFeature()` 的两段输入，`PlayObject.cs:3399/3411`）。
///
/// 未命中（含空表）⇒ `None`，由调用方按 oracle 的 `GetStdItem == null` 分支计 0。
#[must_use]
pub fn shape_of(items: &[StdItem], index: u16) -> Option<u8> {
    get_std_item(items, index).map(|it| it.shape)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, shape: u8) -> StdItem {
        StdItem {
            name: name.to_string(),
            shape,
            ..StdItem::default()
        }
    }

    /// `GameItemSystem.GetStdItem` 的 **1-based** 口径：`Index=1` ⇒ `items[0]`。
    #[test]
    fn get_std_item_is_one_based() {
        let items = vec![item("木剑", 2), item("布衣", 5)];
        assert_eq!(
            get_std_item(&items, 1).map(|i| i.name.as_str()),
            Some("木剑")
        );
        assert_eq!(
            get_std_item(&items, 2).map(|i| i.name.as_str()),
            Some("布衣")
        );
        // 越界 / 0 号 ⇒ None（C# `nItemIdx -= 1` 后越界）。
        assert!(get_std_item(&items, 0).is_none());
        assert!(get_std_item(&items, 3).is_none());
        assert!(get_std_item(&[], 1).is_none());
    }

    /// 命中记录 `Name` 为空串 ⇒ 同 `GetStdItem == null`（`GameItemSystem.cs:28-31`）。
    #[test]
    fn empty_name_is_treated_as_missing() {
        let items = vec![item("", 7)];
        assert!(get_std_item(&items, 1).is_none());
        assert_eq!(shape_of(&items, 1), None);
    }

    /// `shape_of` 返回**原始 Shape**（`*2` 与 gender 加法在 `feature::human_feature` 里，不在这里）。
    #[test]
    fn shape_of_returns_raw_shape() {
        let items = vec![item("布衣", 3), item("木剑", 5)];
        assert_eq!(shape_of(&items, 1), Some(3));
        assert_eq!(shape_of(&items, 2), Some(5));
        assert_eq!(shape_of(&items, 9), None);
    }
}
