//! `src/GameSrv/DB/MySqlDB.cs` 四个加载函数的逐条移植（只读）。
//!
//! **关键**：C# 读列走的是 `src/OpenMir2/Extensions/DataReaderExtension.cs`
//! 的扩展方法，不是 `IDataReader` 原生读取。语义逐条对齐如下
//! （`GetOrdinal` 找不到列时 idx=-1，同样给默认值）：
//!
//! | C# 扩展方法 | NULL / 缺列 | 解析失败 | Rust |
//! | --- | --- | --- | --- |
//! | `GetByte` | 0 | 0（`byte.TryParse` 失败保持 0；负值/超 255 → 0） | [`get_u8`] |
//! | `GetUInt16` | 0 | 0（同上） | [`get_u16`] |
//! | `GetInt32` | -1 | `Convert.ToInt32`（整型列总能成功） | [`get_i32`] |
//! | `GetInt16` | -1 | `Convert.ToInt16` | [`get_i16`] |
//! | `GetSByte` | **抛 InvalidCastException** | 同左 | [`get_i8`]（NULL → Err） |
//! | `GetString` | `""` | `Convert.ToString` | [`get_string`] |
//! | `GetDateTime` | **抛 InvalidCastException** | 同左 | NULL → Err |
//!
//! `(byte)GetInt32(col)` 这类强转在 GetInt32 返回 -1（NULL）时得 255，照原样。

use sqlx::mysql::MySqlRow;
use sqlx::{MySqlPool, Row};

use mir2_shared::hutil;

use crate::models::{
    DealOffInfo, MagicInfo, MonsterInfo, StdItem, ACTOR_RACE_SABUK_DOOR, ACTOR_RACE_SABUK_WALL,
};

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("数据库错误: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("列 `{column}` 为 NULL，C# `Convert.ToSByte/ToDateTime(DBNull)` 在此抛 InvalidCastException")]
    NullConvert { column: &'static str },
    /// 对应 C# `LoadItemsDB` 的 `result = -100` 分支
    /// （`ItemSystem.ItemCount <= idx` 不满足时加载失败并中断）。
    #[error("加载物品(Idx:{idx} Name:{name})数据失败!!!")]
    ItemIndexRule { idx: i32, name: String },
    #[error("列 `{column}` JSON 解析失败（C# `JsonSerializer.Deserialize` 在此抛异常）: {reason}")]
    Json {
        column: &'static str,
        reason: String,
    },
}

/// 读原始整数值；NULL 或列不存在 → None（对应 C# `IsDBNull` / `GetOrdinal = -1`）。
///
/// 类型口径（2026-10-10 实测，sqlx 0.8）：`Option<i64>` 能读 MySQL 的
/// tinyint / smallint / int / bigint，**只有读成 `u8` 会 mismatched types 失败**
/// （且这类失败容易被 `unwrap_or` 吞掉 —— 别用 u8 读这些列）。
fn raw_i64(row: &MySqlRow, col: &'static str) -> Option<i64> {
    match row.try_get::<Option<i64>, _>(col) {
        Ok(v) => v,
        Err(sqlx::Error::ColumnNotFound(_)) => None,
        // 字符串/其他类型列不在本加载器路径上；其他错误按 None 处理会掩盖问题，
        // 但 SELECT * 下不会发生，保持 panic 以便暴露。
        Err(e) => panic!("读取列 {col} 失败: {e}"),
    }
}

/// C# `DataReaderExtension.GetByte`：NULL/缺列/解析失败 → 0；负值、超 255 → 0。
fn get_u8(row: &MySqlRow, col: &'static str) -> u8 {
    match raw_i64(row, col) {
        Some(v) => u8::try_from(v).unwrap_or(0),
        None => 0,
    }
}

/// C# `DataReaderExtension.GetUInt16`：NULL/缺列/解析失败 → 0；负值、超 65535 → 0。
fn get_u16(row: &MySqlRow, col: &'static str) -> u16 {
    match raw_i64(row, col) {
        Some(v) => u16::try_from(v).unwrap_or(0),
        None => 0,
    }
}

/// C# `DataReaderExtension.GetInt32`：NULL/缺列 → -1。
fn get_i32(row: &MySqlRow, col: &'static str) -> i32 {
    match raw_i64(row, col) {
        Some(v) => i32::try_from(v).unwrap_or(-1),
        None => -1,
    }
}

/// C# `DataReaderExtension.GetInt16`：NULL/缺列 → -1。
fn get_i16(row: &MySqlRow, col: &'static str) -> i16 {
    match raw_i64(row, col) {
        Some(v) => i16::try_from(v).unwrap_or(-1),
        None => -1,
    }
}

/// C# `DataReaderExtension.GetSByte`：NULL → 抛 InvalidCastException（Err）；
/// 缺列 → -1。
fn get_i8(row: &MySqlRow, col: &'static str) -> Result<i8, DataError> {
    match row.try_get::<Option<i64>, _>(col) {
        Ok(Some(v)) => Ok(i8::try_from(v).unwrap_or(-1)),
        Ok(None) => Err(DataError::NullConvert { column: col }),
        Err(sqlx::Error::ColumnNotFound(_)) => Ok(-1),
        Err(e) => panic!("读取列 {col} 失败: {e}"),
    }
}

/// C# `DataReaderExtension.GetString`：NULL/缺列 → ""。
fn get_string(row: &MySqlRow, col: &'static str) -> String {
    match row.try_get::<Option<String>, _>(col) {
        Ok(v) => v.unwrap_or_default(),
        Err(sqlx::Error::ColumnNotFound(_)) => String::new(),
        Err(e) => panic!("读取列 {col} 失败: {e}"),
    }
}

/// C# `(byte)GetInt32(col)` 强转：NULL → GetInt32 得 -1 → (byte)(-1) = 255。
fn cast_u8(row: &MySqlRow, col: &'static str) -> u8 {
    get_i32(row, col) as u8
}

/// 四表加载器。对应 C# `GameShare.DataSource`（`MySqlDB`）。
pub struct DataLoaders {
    pool: MySqlPool,
    /// C# `SystemShare.Config.MonsterPowerRate`（运行中 Server.conf = 10）。
    pub monster_power_rate: i32,
}

impl DataLoaders {
    pub fn new(pool: MySqlPool, monster_power_rate: i32) -> Self {
        DataLoaders {
            pool,
            monster_power_rate,
        }
    }

    /// C# `MySqlDB.LoadItemsDB`（`src/GameSrv/DB/MySqlDB.cs:13`）。
    ///
    /// 返回成功加载的物品表（对应 `ItemSystem` 内容）；`usize` 下标 + 1 即 C# 的物品 Index。
    /// 加载规则照 C#：`ItemCount <= idx` 才允许加入，否则整体失败（C# 返回 -100）。
    pub async fn load_items_db(&self) -> Result<Vec<StdItem>, DataError> {
        let rows = sqlx::query("SELECT * FROM stditems")
            .fetch_all(&self.pool)
            .await?;
        let mut items: Vec<StdItem> = Vec::with_capacity(rows.len());
        for row in &rows {
            let idx = get_i32(row, "Id");
            let item = StdItem {
                name: get_string(row, "Name"),
                std_mode: get_u8(row, "StdMode"),
                shape: get_u8(row, "Shape"),
                weight: get_u8(row, "Weight"),
                ani_count: get_u8(row, "AniCount"),
                special_pwr: get_i8(row, "Source")?,
                item_desc: get_u8(row, "Reserved"),
                looks: get_u16(row, "ImgIndex"),
                dura_max: get_u16(row, "DuraMax"),
                ac: hutil::make_word(get_u16(row, "Ac"), get_u16(row, "AcMax")),
                mac: hutil::make_word(get_u16(row, "Mac"), get_u16(row, "MacMax")),
                dc: hutil::make_word(get_u16(row, "Dc"), get_u16(row, "DcMax")),
                mc: hutil::make_word(get_u16(row, "Mc"), get_u16(row, "McMax")),
                sc: hutil::make_word(get_u16(row, "Sc"), get_u16(row, "ScMax")),
                need: get_u8(row, "Need"),
                need_level: get_u8(row, "NeedLevel"),
                // C# 加载时恒为 0（MySqlDB.cs:47）
                need_identify: 0,
                price: get_i32(row, "Price"),
                stock: get_i32(row, "Stock"),
                atk_spd: get_u8(row, "Atkspd"),
                agility: get_u8(row, "AgilIty"),
                accurate: get_u8(row, "Accurate"),
                mg_avoid: get_u8(row, "Mgavoid"),
                strong: get_u8(row, "Strong"),
                undead: get_u8(row, "Undead"),
                hp_add: get_i32(row, "HpAdd"),
                mp_add: get_i32(row, "MpAdd"),
                exp_add: get_i32(row, "ExpAdd"),
                eff_type1: get_u8(row, "Efftype1"),
                eff_rate1: get_u8(row, "Effrate1"),
                eff_value1: get_u8(row, "Effvalue1"),
                eff_type2: get_u8(row, "Efftype2"),
                eff_rate2: get_u8(row, "Effrate2"),
                eff_value2: get_u8(row, "Effvalue2"),
                slowdown: get_u8(row, "SlowDown"),
                tox: get_u8(row, "Tox"),
                tox_avoid: get_u8(row, "ToxAvoid"),
                unique_item: get_u8(row, "UniqueItem"),
                overlap_item: get_u8(row, "OverlapItem"),
                light: get_u8(row, "LIGHT"),
                item_type: get_u8(row, "ItemType"),
                item_set: get_u16(row, "ItemSet"),
                reference: get_string(row, "Reference"),
            };
            // C#: if (SystemShare.ItemSystem.ItemCount <= idx) AddItem else 失败(-100)
            if (items.len() as i32) <= idx {
                items.push(item);
            } else {
                return Err(DataError::ItemIndexRule {
                    idx,
                    name: item.name,
                });
            }
        }
        Ok(items)
    }

    /// C# `MySqlDB.LoadMagicDB`（`src/GameSrv/DB/MySqlDB.cs:105`）。
    ///
    /// `MagicId == 0` 的行跳过不计（C# `AddMagicList` 只在 > 0 时调用）。
    pub async fn load_magic_db(&self) -> Result<Vec<MagicInfo>, DataError> {
        let rows = sqlx::query("select * from magics")
            .fetch_all(&self.pool)
            .await?;
        let mut magics = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut magic = MagicInfo {
                magic_id: get_u16(row, "MagID"),
                magic_name: get_string(row, "MagName"),
                effect_type: cast_u8(row, "EffectType"),
                effect: cast_u8(row, "Effect"),
                spell: get_u16(row, "Spell"),
                power: get_u16(row, "Power"),
                max_power: get_u16(row, "MaxPower"),
                job: cast_u8(row, "Job"),
                train_level: [0; 4],
                max_train: [0; 4],
                train_lv: 3, // C# 加载时恒为 3（MySqlDB.cs:139）
                delay_time: get_i32(row, "Delay"),
                def_spell: cast_u8(row, "DefSpell"),
                def_power: cast_u8(row, "DefPower"),
                def_max_power: cast_u8(row, "DefMaxPower"),
                desc: get_string(row, "Descr"),
            };
            magic.train_level[0] = cast_u8(row, "NeedL1");
            magic.train_level[1] = cast_u8(row, "NeedL2");
            magic.train_level[2] = cast_u8(row, "NeedL3");
            // C# 原样：train_level[3] 也取 NeedL3（没有 NeedL4 列，MySqlDB.cs:134）
            magic.train_level[3] = cast_u8(row, "NeedL3");
            magic.max_train[0] = get_i32(row, "L1Train");
            magic.max_train[1] = get_i32(row, "L2Train");
            magic.max_train[2] = get_i32(row, "L3Train");
            // C# 原样：max_train[3] 复制 max_train[2]（MySqlDB.cs:138）
            magic.max_train[3] = magic.max_train[2];
            if magic.magic_id > 0 {
                magics.push(magic);
            }
        }
        Ok(magics)
    }

    /// C# `MySqlDB.LoadMonsterDB`（`src/GameSrv/DB/MySqlDB.cs:168`）。
    ///
    /// 掉落列表（`LoadMonitems`）不在此函数内——C# 也是从 `Envir/MonItems`
    /// 目录按怪物名读文件，见 [`crate::monitems`]。
    pub async fn load_monster_db(&self) -> Result<Vec<MonsterInfo>, DataError> {
        let rows = sqlx::query("select * from monsters")
            .fetch_all(&self.pool)
            .await?;
        let rate = f64::from(self.monster_power_rate) / 10.0;
        let mut monsters = Vec::with_capacity(rows.len());
        for row in &rows {
            let race = cast_u8(row, "Race");
            // C#: 城门/城墙 HP 不加倍（MySqlDB.cs:196-203），其余按 MonsterPowerRate 缩放
            let hp = if race == ACTOR_RACE_SABUK_WALL || race == ACTOR_RACE_SABUK_DOOR {
                get_u16(row, "HP")
            } else {
                hutil::round(f64::from(get_i32(row, "HP")) * rate) as u16
            };
            let mut walk_speed = hutil::max_i32(200, get_i32(row, "WALK_SPD")) as u16;
            let mut attack_speed = get_i32(row, "ATTACK_SPD") as u16;
            // C# 的双重下限原样保留（MySqlDB.cs:217-224）
            if walk_speed < 200 {
                walk_speed = 200;
            }
            if attack_speed < 200 {
                attack_speed = 200;
            }
            monsters.push(MonsterInfo {
                name: get_string(row, "Name").trim().to_string(),
                race,
                race_img: cast_u8(row, "RaceImg"),
                appr: get_u16(row, "Appr"),
                level: get_u8(row, "Lvl"),
                life_attrib: cast_u8(row, "Undead"),
                cool_eye: get_u8(row, "CoolEye"),
                exp: get_i32(row, "Exp"),
                hp,
                mp: hutil::round(f64::from(get_i32(row, "MP")) * rate) as u16,
                ac: hutil::round(f64::from(get_i32(row, "AC")) * rate) as u16,
                mac: hutil::round(f64::from(get_i32(row, "MAC")) * rate) as u16,
                dc: hutil::round(f64::from(get_i32(row, "DC")) * rate) as u16,
                max_dc: hutil::round(f64::from(get_i32(row, "DCMAX")) * rate) as u16,
                mc: hutil::round(f64::from(get_i32(row, "MC")) * rate) as u16,
                sc: hutil::round(f64::from(get_i32(row, "SC")) * rate) as u16,
                speed: get_u8(row, "SPEED"),
                hit_point: get_u8(row, "HIT"),
                walk_speed,
                walk_step: hutil::max_i32(1, get_i32(row, "WalkStep")) as u16,
                walk_wait: get_i32(row, "WaLkWait") as u16,
                attack_speed,
            });
        }
        Ok(monsters)
    }

    /// C# `MySqlDB.LoadSellOffItemList`（`src/GameSrv/DB/MySqlDB.cs:251`）。
    ///
    /// BUG-REPLICA（不改）：C# 读 `DealChrName` / `BuyChrName`，而 DDL 列名是
    /// `DealCharName` / `BuyCharName`。`GetOrdinal` 找不到 → `GetString` 返回 ""
    /// → 过滤条件（两名都非空）把所有行静默跳过。即**即使表里有数据，
    /// C# 也永远加载 0 行**。Rust 按同样的错列名读取，行为一致。
    pub async fn load_sell_off_item_list(&self) -> Result<Vec<DealOffInfo>, DataError> {
        let rows = sqlx::query("select * from goldsales")
            .fetch_all(&self.pool)
            .await?;
        let mut list = Vec::new();
        for row in &rows {
            // 照 C# 的列名（DDL 里不存在）→ get_string 得 ""
            let deal_chr_name = get_string(row, "DealChrName");
            let buy_chr_name = get_string(row, "BuyChrName");
            let state = get_u8(row, "State");
            // C# 过滤：两名都非空且 State < 4（MySqlDB.cs:270）
            if deal_chr_name.is_empty() || buy_chr_name.is_empty() || state >= 4 {
                continue;
            }
            // 以下在 C# 里永远走不到（列名错误导致上面全跳过），保留完整移植。
            let use_items_raw = get_string(row, "UseItems");
            // C# GetDateTime：缺列 → DateTime.MinValue；NULL → 抛异常
            let sell_date_time =
                match row.try_get::<Option<chrono::NaiveDateTime>, _>("SellDateTime") {
                    Ok(Some(v)) => v,
                    Ok(None) => {
                        return Err(DataError::NullConvert {
                            column: "SellDateTime",
                        })
                    }
                    Err(sqlx::Error::ColumnNotFound(_)) => chrono::NaiveDateTime::default(),
                    Err(e) => panic!("读取列 SellDateTime 失败: {e}"),
                };
            list.push(DealOffInfo {
                deal_chr_name,
                buy_chr_name,
                sell_date_time,
                sell_gold: get_i16(row, "SellGold"),
                use_items: serde_json::from_str(&use_items_raw).map_err(|e| DataError::Json {
                    column: "UseItems",
                    reason: e.to_string(),
                })?,
                flag: state,
            });
        }
        Ok(list)
    }
}
