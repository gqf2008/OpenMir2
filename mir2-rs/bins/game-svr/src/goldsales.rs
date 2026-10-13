//! `goldsales` **只读**加载接线（`M2Share.SellOffItemList` 的启动种子）
//! —— M3⑩-实现 2（卡 `openmir2-m3-selloff-goldsales-load-wire`）。
//!
//! # oracle 锚点
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`load_goldsales`] | `src/GameSrv/DB/MySqlDB.cs:251-291`（`LoadSellOffItemList`，`select * from goldsales`） |
//! | 启动调用点 | `src/GameSrv/GameApp.cs:30`（`SellOffItemList = new List<DealOffInfo>()`）+ `:181-182`（`LogService.Info("加载物品寄售系统...")` → `GameShare.DataSource.LoadSellOffItemList()`） |
//! | [`entry_from_loaded`] | `MySqlDB.cs:270-280`（`new DealOffInfo()` + 逐字段赋值 + `JsonSerializer.Deserialize<UserItem[]>(sUseItems)`） |
//! | [`seed_book`] | `MySqlDB.cs:279`（`M2Share.SellOffItemList.Add(DealOffInfo)`） |
//!
//! # 只读 + 缺陷照搬（§1 硬约束③）
//!
//! - **不实现 save**：`SaveSellOffItemList`（`MySqlDB.cs:293-330`）在 oracle 的三个调用点
//!   （`PlayObject.Sale.cs:240/332/577`）**全被注释** ⇒ 永不执行。照搬 = 不实现（登记在证据 README）。
//! - **读错列名照搬**：加载器读 `DealChrName`/`BuyChrName`，DDL 列名是 `DealCharName`/`BuyCharName`
//!   （`sql/mir2_data.sql:26-27`）⇒ 两个名字恒空 ⇒ 过滤条件（`MySqlDB.cs:270`）把**所有**行静默跳过
//!   ⇒ **即使表里有数据，也恒加载 0 行**。该缺陷属 D 线白名单 **D-5/B-5**，加载器本体在
//!   [`mir2_data::loaders::DataLoaders::load_sell_off_item_list`]（本模块不复制它、只接线）。
//! - `UseItems` 的 `UserItem[]` 解码在 D 线口径里被显式推迟（`crates/data/src/models.rs:141-144`
//!   注释），本模块按 `.NET UserItem` 的 JSON 形态补齐这层桥接；因上面「恒 0 行」，该支在
//!   生产路径**不可达**（登记，不假造读数）。
//!
//! # 确定性
//!
//! 纯加载 + 顺序尾追（`Vec::push`），零并行、零 `static`；加载发生在 tokio 侧、装进
//! [`game_svr::selloff::SellOffBook`] 后随 `EnterConfig` 交给独占 `World` 的 sim 线程
//! （§4.2 第 1 条）。

use base64::Engine as _;
use game_svr::selloff::{DealOffInfo as SellOffEntry, SellOffBook};
use mir2_data::loaders::DataLoaders;
use mir2_data::DealOffInfo as LoadedEntry;
use mir2_storage::LegacyDsn;

/// 直连 MySQL 读 `goldsales`（`MySqlDB.LoadSellOffItemList`，`src/GameSrv/DB/MySqlDB.cs:251`）。
///
/// 连接串走 [`LegacyDsn`]（与 `items::load_items` / db-srv / login-srv 同一份解析）。
/// 失败只返回错误 —— 调用方按 oracle `LoadSellOffItemList` 的 `!Open()` 分支
/// （`MySqlDB.cs:254-258`：`LogService.Error` + `return`，列表保持原样）降级，**不 panic**。
///
/// # Errors
/// 连接串解析失败、连库失败、`select * from goldsales` 失败时返回人话错误。
pub async fn load_goldsales(conn: &str) -> Result<Vec<LoadedEntry>, String> {
    let dsn = LegacyDsn::parse(conn).map_err(|e| format!("ConnctionString 解析失败：{e}"))?;
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&dsn.to_sqlx_url())
        .await
        .map_err(|e| format!("连接 MySQL 失败（读 goldsales）：{e}"))?;
    // `monster_power_rate` 只被 `load_monster_db` 用，goldsales 不读它；照 `items.rs` 的口径传 10。
    DataLoaders::new(pool, 10)
        .load_sell_off_item_list()
        .await
        .map_err(|e| format!("加载 goldsales 失败：{e}"))
}

/// `goldsales.UseItems` 里的一项（`.NET UserItem` 的 `System.Text.Json` 形态）。
///
/// `Desc` 在 .NET 是 `byte[]`，默认序列化成 **base64 字符串**；其余是 `MakeIndex`/`Index`/
/// `Dura`/`DuraMax` 四个整数属性（`src/OpenMir2/Packets/ClientPackets/ClientUserItem.cs:19-27`）。
/// 未知字段（`ColorR/G/B`/`Prefix` 及基类属性）**忽略**（`.NET` 反序列化同样按属性名匹配）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedUserItem {
    /// `UserItem.MakeIndex`（`int`）。
    pub make_index: i32,
    /// `UserItem.Index`（`ushort`）。
    pub index: u16,
    /// `UserItem.Dura`（`ushort`）。
    pub dura: u16,
    /// `UserItem.DuraMax`（`ushort`）。
    pub dura_max: u16,
    /// `UserItem.Desc`（`byte[]`，已从 base64 解出；长度保原样）。
    pub desc: Vec<u8>,
}

/// 把加载器的一行 `goldsales` 记录转成世界态条目（`M2Share.SellOffItemList` 的元素）。
///
/// 逐字段对应 `MySqlDB.cs:271-280`：
/// - `sDealChrName`/`sBuyChrName`/`nSellGold`/`Flag` 直接搬；
/// - `dSellDateTime`（`DateTime`）→ 毫秒（`NaiveDateTime` 视为 UTC；该值仅用于展示，
///   且生产路径不可达 —— 见模块头）；
/// - `UseItems` = `JsonSerializer.Deserialize<UserItem[]>`：`null` 槽保留为 `None`
///   （这是 [`crate::selloff`] 里「遇空槽中止」缺陷①的前提）；数组**长度保原样**（不补到 9）。
#[must_use]
pub fn entry_from_loaded(row: &LoadedEntry) -> SellOffEntry {
    SellOffEntry {
        deal_chr_name: row.deal_chr_name.clone(),
        buy_chr_name: row.buy_chr_name.clone(),
        sell_datetime_ms: row.sell_date_time.and_utc().timestamp_millis(),
        sell_gold: i32::from(row.sell_gold),
        use_items: decode_use_items(&row.use_items),
        flag: row.flag,
    }
}

/// `JsonSerializer.Deserialize<UserItem[]>(sUseItems)` 的 Rust 对应（`MySqlDB.cs:277`）。
///
/// `serde_json::Value` 里的每个元素：`null` ⇒ `None`（空槽），对象 ⇒ 取四个整数字段 +
/// `Desc`（base64）。数组之外的形态（`null`/非数组/畸形对象）⇒ 空表（生产路径不可达；
/// oracle 在该形态抛异常，见模块头登记）。
#[must_use]
fn decode_use_items(raw: &serde_json::Value) -> Vec<Option<game_svr::bag::BagItem>> {
    decode_slots(raw)
        .into_iter()
        .map(|slot| {
            slot.map(|it| {
                let mut desc = [0u8; 14];
                let n = it.desc.len().min(14);
                desc[..n].copy_from_slice(&it.desc[..n]);
                game_svr::bag::BagItem {
                    make_index: it.make_index,
                    index: it.index,
                    dura: it.dura,
                    dura_max: it.dura_max,
                    desc,
                }
            })
        })
        .collect()
}

/// 元素级解码（把 `Value` 变成 [`LoadedUserItem`]；`BagItem` 组装在 [`decode_use_items`]）。
#[must_use]
fn decode_slots(raw: &serde_json::Value) -> Vec<Option<LoadedUserItem>> {
    let Some(arr) = raw.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .map(|v| {
            if v.is_null() {
                return None;
            }
            Some(LoadedUserItem {
                make_index: json_i32(v, "MakeIndex"),
                index: json_u16(v, "Index"),
                dura: json_u16(v, "Dura"),
                dura_max: json_u16(v, "DuraMax"),
                desc: json_desc(v),
            })
        })
        .collect()
}

/// 属性名匹配的整数字段（`.NET` 按属性名，缺字段 ⇒ 0）。
fn json_i32(v: &serde_json::Value, key: &str) -> i32 {
    v.get(key)
        .and_then(serde_json::Value::as_i64)
        .map_or(0, |n| i32::try_from(n).unwrap_or(0))
}

/// 同上，`ushort` 口径。
fn json_u16(v: &serde_json::Value, key: &str) -> u16 {
    v.get(key)
        .and_then(serde_json::Value::as_u64)
        .map_or(0, |n| u16::try_from(n).unwrap_or(0))
}

/// `Desc`（`byte[]` ⇒ base64 字符串）；缺/非字符串/null ⇒ 空数组。
fn json_desc(v: &serde_json::Value) -> Vec<u8> {
    let Some(s) = v.get("Desc").and_then(serde_json::Value::as_str) else {
        return Vec::new();
    };
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .unwrap_or_default()
}

/// `M2Share.SellOffItemList.Add(DealOffInfo)`（`MySqlDB.cs:279`）：顺序尾追。
///
/// 顺序 = `select * from goldsales` 的行序（oracle `while (dr.Read())`）；零并行。
pub fn seed_book(book: &mut SellOffBook, rows: &[LoadedEntry]) {
    for row in rows {
        book.entries.push(entry_from_loaded(row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use game_svr::selloff::DEAL_OFF_ITEMS;

    fn loaded(
        deal: &str,
        buy: &str,
        gold: i16,
        state: u8,
        use_items: serde_json::Value,
    ) -> LoadedEntry {
        LoadedEntry {
            deal_chr_name: deal.to_string(),
            buy_chr_name: buy.to_string(),
            sell_date_time: NaiveDate::from_ymd_opt(2026, 10, 13)
                .unwrap()
                .and_hms_opt(1, 2, 3)
                .unwrap(),
            sell_gold: gold,
            use_items,
            flag: state,
        }
    }

    /// 判据（构造级）：一行 `goldsales` 记录逐字段落进世界态条目 +
    /// `UseItems` 的 `null` 空槽（缺陷①的前提）保留 + 长度保原样。
    #[test]
    fn entry_from_loaded_maps_fields_and_keeps_null_slots() {
        let raw = serde_json::json!([
            {
                "MakeIndex": 123,
                "Index": 456,
                "Dura": 7,
                "DuraMax": 9,
                "Desc": "AAECAw=="
            },
            null
        ]);
        let row = loaded("卖家", "买家", 500, 1, raw);
        let e = entry_from_loaded(&row);
        assert_eq!(e.deal_chr_name, "卖家");
        assert_eq!(e.buy_chr_name, "买家");
        assert_eq!(e.sell_gold, 500);
        assert_eq!(e.flag, 1);
        assert_eq!(e.sell_datetime_ms, 1_791_853_323_000);
        assert_eq!(
            e.use_items.len(),
            2,
            "数组长度保原样（不补到 DEAL_OFF_ITEMS）"
        );
        let slot = e.use_items[0].as_ref().expect("0 号槽非空");
        assert_eq!(slot.make_index, 123);
        assert_eq!(slot.index, 456);
        assert_eq!(slot.dura, 7);
        assert_eq!(slot.dura_max, 9);
        assert_eq!(&slot.desc[..4], &[0, 1, 2, 3]);
        assert!(e.use_items[1].is_none(), "null 槽 = None（缺陷①前提）");
        assert_eq!(DEAL_OFF_ITEMS, 9);
    }

    /// 判据（构造级）：`seed_book` 顺序尾追（`MySqlDB.cs:279`），且从空 book 起。
    #[test]
    fn seed_book_appends_in_row_order() {
        let rows = vec![
            loaded("a", "b", 1, 0, serde_json::json!([])),
            loaded("c", "d", 2, 3, serde_json::json!([])),
        ];
        let mut book = SellOffBook::default();
        assert!(book.entries.is_empty());
        seed_book(&mut book, &rows);
        assert_eq!(book.entries.len(), 2);
        assert_eq!(book.entries[0].deal_chr_name, "a");
        assert_eq!(book.entries[1].deal_chr_name, "c");
        // 再 seed 一次 = 再追加（oracle 每个 `Add` 都是一次 push）。
        seed_book(&mut book, &rows[..1]);
        assert_eq!(book.entries.len(), 3);
    }

    /// 空表 ⇒ book 保持空（oracle `LoadSellOffItemList` 的 0 行分支）。
    #[test]
    fn seed_book_with_no_rows_keeps_book_empty() {
        let mut book = SellOffBook::default();
        seed_book(&mut book, &[]);
        assert!(book.entries.is_empty());
    }

    /// `Desc` 非 base64 / 缺字段 ⇒ 空（生产路径不可达，登记形态）。
    #[test]
    fn malformed_item_json_degrades_to_zero_fields() {
        let raw = serde_json::json!({ "MakeIndex": 1 });
        assert!(decode_slots(&raw).is_empty(), "非数组 ⇒ 空表");
        let raw = serde_json::json!([{ "Index": 5, "Desc": "!!!not-base64!!!" }]);
        let slots = decode_slots(&raw);
        assert_eq!(slots.len(), 1);
        let it = slots[0].as_ref().unwrap();
        assert_eq!(it.index, 5);
        assert_eq!(it.make_index, 0);
        assert!(it.desc.is_empty());
    }
}
