//! DBSrv **6000 `DataService` 面**的 MySQL 存储层：`IPlayDataStorage` 的
//! `PlayDataStorage.Query` / `Save` / `Add` 三段（C# `Storeages/DBSvr.Storage.MySQL/PlayDataStorage.{Query,Save,Add}.cs`）。
//!
//! 逐条 1:1 照搬 C#（含既有 bug，行为等价优先）：
//! - 取档 `GetRecord`：`characters` 主表 + `characters_ablity`/`characters_bonusability`/
//!   `characters_magic`/`characters_item`/`characters_bagitem`/`characters_storageitem`/
//!   `characters_status` 七张附属表；主表缺行 ⇒ C# NRE（`GetRecord` catch 后返回 false）；
//! - 存档 `SaveRecord`：主表 `UPDATE` 里 `Job = Job`（job **不更新**）、`characters_ablity` 的
//!   `Ac = @Ac` 绑的是 `Abil.Level`、`@Hp` 只写 `Abil.HP`（AC 落在 HP 列高 16 位却从不写回）——
//!   全是 oracle 既有行为，逐字保留；
//! - `SaveItem`/`SaveBagItem`/`SaveStorageItem` 用 `ComparerUserItem` 做「差量」，
//!   `UpdateItemAttr`/`ClearItemAttr`/`SaveMagics` 走 C# 的**字符串拼 SQL**（整数内联），
//!   这里同样拼文本后逐条执行（同一事务内等价于 C# 的多语句批处理）；
//! - `QueryItemAttr` 的 `userItems[nPosition].Desc[nPosition] = VALUE{nPosition}` 既有 bug 照搬。
//! - `Add` 的 `CreateCharacters` 与 5100 面的建角不同：它把完整 `CharacterData` 字段写进主表；
//!   `CreateAblity` 是 **INSERT**（`Add.cs:157-193`）——旧 Rust 实现误用 UPDATE，新角色无行
//!   ⇒ 整表永不落库（G-20，本批按 oracle 改回 INSERT）。
//!
//! 一处必要差异：C# `dr.GetByte/GetInt16/GetUInt16` 在列值越界时抛异常（被上层 catch）；
//! 这里同样用 `try_from` 报错并让调用方按「取档失败」处理，不静默截断。

use anyhow::{Context, Result};
use futures_util::TryStreamExt;
use mir2_storage::playerdata::{
    Ability, CharacterData, CharacterDataInfo, MagicRcd, NakedAbility, ServerUserItem,
};
use sqlx::mysql::MySqlRow;
use sqlx::{Connection, MySqlConnection, Row};
use std::fmt::Write as _;

use crate::db::{int_col, resolve_col, Db};
use crate::gbk::fold_key;
use crate::log;

/// 存档物品槽位数（C# `CharacterData` 构造函数）。
const HUM_ITEM_SLOTS: usize = 13;
const BAG_ITEM_SLOTS: usize = 46;
const STORAGE_ITEM_SLOTS: usize = 50;
/// 技能槽位数（C# `Magic = new MagicRcd[20]`）。
const MAGIC_SLOTS: usize = 20;
/// 状态槽位数（C# `StatusTimeArr = new ushort[15]`）。
const STATUS_SLOTS: usize = 15;

// ==== 列读取助手（口径与 C# `dr.GetXxx` 一致） ====

fn opt_text(row: &MySqlRow, name: &str) -> Result<Option<String>> {
    let name = resolve_col(row, name)?;
    row.try_get::<Option<String>, _>(name)
        .with_context(|| format!("列 {name} 不是文本类型（表结构与 C# 预期不符）"))
}

fn byte_col(row: &MySqlRow, name: &str) -> Result<u8> {
    u8::try_from(int_col(row, name)?)
        .with_context(|| format!("列 {name} 超出 byte 范围（C# `GetByte` 同场景抛异常）"))
}

fn word_col(row: &MySqlRow, name: &str) -> Result<u16> {
    u16::try_from(int_col(row, name)?)
        .with_context(|| format!("列 {name} 超出 ushort 范围（C# `GetUInt16` 同场景抛异常）"))
}

fn short_col(row: &MySqlRow, name: &str) -> Result<i16> {
    i16::try_from(int_col(row, name)?)
        .with_context(|| format!("列 {name} 超出 short 范围（C# `GetInt16` 同场景抛异常）"))
}

fn bool_col(row: &MySqlRow, name: &str) -> Result<bool> {
    Ok(int_col(row, name)? != 0)
}

fn double_col(row: &MySqlRow, name: &str) -> Result<f64> {
    let name = resolve_col(row, name)?;
    row.try_get::<f64, _>(name)
        .with_context(|| format!("列 {name} 不是 double（表结构与 C# 预期不符）"))
}

// ==== 取档 ====

/// C# `PlayDataStorage.GetChrRecord`：`characters` 主表。
async fn get_chr_record(
    conn: &mut MySqlConnection,
    player_id: i64,
) -> Result<Option<CharacterData>> {
    let row = sqlx::query("SELECT * FROM characters WHERE ID=?")
        .bind(player_id)
        .fetch_optional(&mut *conn)
        .await
        .context("查询 characters 主表失败")?;
    let Some(row) = row else {
        return Ok(None);
    };
    // 顺序与 C# `GetChrRecord` 的读取顺序一致（顺序不影响结果，但便于对读）。
    // `MapName` 为 NULL 时 C# 保留构造函数默认（null），不写空串。
    let d = CharacterData {
        account: opt_text(&row, "LoginID")?,
        chr_name: opt_text(&row, "ChrName")?,
        cur_map: opt_text(&row, "MapName")?,
        cur_x: short_col(&row, "CX")?,
        cur_y: short_col(&row, "CY")?,
        dir: byte_col(&row, "Dir")?,
        hair: byte_col(&row, "Hair")?,
        sex: byte_col(&row, "Sex")?,
        job: byte_col(&row, "Job")?,
        gold: i32::try_from(int_col(&row, "Gold")?).context("Gold 越界")?,
        home_map: opt_text(&row, "HomeMap")?,
        home_x: short_col(&row, "HomeX")?,
        home_y: short_col(&row, "HomeY")?,
        dear_name: opt_text(&row, "DearName")?,
        master_name: opt_text(&row, "MasterName")?,
        is_master: bool_col(&row, "IsMaster")?,
        credit_point: byte_col(&row, "CreditPoint")?,
        storage_pwd: opt_text(&row, "StoragePwd")?,
        re_level: byte_col(&row, "ReLevel")?,
        lock_logon: bool_col(&row, "LockLogon")?,
        bonus_point: i32::try_from(int_col(&row, "BonusPoint")?).context("BonusPoint 越界")?,
        // C# 既有行为：`GameGold` 与 `Gold` 读的是**同一列**。
        game_gold: i32::try_from(int_col(&row, "Gold")?).context("Gold 越界")?,
        game_point: i32::try_from(int_col(&row, "GamePoint")?).context("GamePoint 越界")?,
        pay_ment_point: i32::try_from(int_col(&row, "PayMentPoint")?)
            .context("PayMentPoint 越界")?,
        hunger_status: i32::try_from(int_col(&row, "HungerStatus")?)
            .context("HungerStatus 越界")?,
        allow_group: u8::try_from(int_col(&row, "AllowGroup")?).context("AllowGroup 越界")?,
        attatck_mode: byte_col(&row, "AttatckMode")?,
        inc_health: byte_col(&row, "IncHealth")?,
        inc_spell: byte_col(&row, "IncSpell")?,
        inc_healing: byte_col(&row, "IncHealing")?,
        fight_zone_die_count: byte_col(&row, "FightZoneDieCount")?,
        allow_guild_re_call: bool_col(&row, "AllowGuildReCall")?,
        allow_group_re_call: bool_col(&row, "AllowGroupReCall")?,
        group_rcall_time: short_col(&row, "GroupRcallTime")?,
        body_luck: double_col(&row, "BodyLuck")?,
        ..CharacterData::default()
    };
    Ok(Some(d))
}

/// C# `PlayDataStorage.GetAbilGetRecord`：**总是**返回一个 `Ability`（无行即全零默认值）。
async fn get_abil_record(conn: &mut MySqlConnection, player_id: i64) -> Result<Ability> {
    let mut a = Ability::default();
    let row = sqlx::query("select * from characters_ablity where playerId=?")
        .bind(player_id)
        .fetch_optional(&mut *conn)
        .await
        .context("查询 characters_ablity 失败")?;
    let Some(row) = row else {
        return Ok(a);
    };
    a.level = byte_col(&row, "Level")?;
    // C# `HUtil32.LoWord/HiWord`：HP 列是 int32，低 16 位是 HP、高 16 位是 AC。
    let hp = i32::try_from(int_col(&row, "HP")?).context("HP 越界")?;
    a.hp = lo_word(hp);
    a.ac = hi_word(hp);
    let mp = i32::try_from(int_col(&row, "MP")?).context("MP 越界")?;
    a.mp = lo_word(mp);
    a.mac = hi_word(mp);
    a.dc = word_col(&row, "DC")?;
    a.mc = word_col(&row, "MC")?;
    a.sc = word_col(&row, "SC")?;
    a.exp = i32::try_from(int_col(&row, "EXP")?).context("EXP 越界")?;
    a.max_exp = i32::try_from(int_col(&row, "MaxExp")?).context("MaxExp 越界")?;
    a.weight = word_col(&row, "Weight")?;
    a.max_weight = word_col(&row, "MaxWeight")?;
    a.wear_weight = byte_col(&row, "WearWeight")?;
    a.max_wear_weight = byte_col(&row, "MaxWearWeight")?;
    a.hand_weight = byte_col(&row, "HandWeight")?;
    a.max_hand_weight = byte_col(&row, "MaxHandWeight")?;
    Ok(a)
}

/// C# `PlayDataStorage.GetBonusAbilRecord`：**只在有行时**写 `BonusAbil`（无行保留默认对象）。
async fn get_bonus_abil_record(
    conn: &mut MySqlConnection,
    player_id: i64,
    d: &mut CharacterData,
) -> Result<()> {
    let row = sqlx::query("SELECT * FROM characters_bonusability WHERE PlayerId=?")
        .bind(player_id)
        .fetch_optional(&mut *conn)
        .await
        .context("查询 characters_bonusability 失败")?;
    let Some(row) = row else {
        return Ok(());
    };
    d.bonus_abil = Some(NakedAbility {
        ac: word_col(&row, "AC")?,
        mac: word_col(&row, "MAC")?,
        dc: word_col(&row, "DC")?,
        mc: word_col(&row, "MC")?,
        sc: word_col(&row, "SC")?,
        hp: word_col(&row, "HP")?,
        mp: word_col(&row, "MP")?,
        hit: byte_col(&row, "HIT")?,
        speed: i32::try_from(int_col(&row, "SPEED")?).context("SPEED 越界")?,
        reserved: byte_col(&row, "RESERVED")?,
    });
    Ok(())
}

/// C# `PlayDataStorage.GetMagicRecord`：**先把 20 个槽全填成空 `MagicRcd`**，再按行覆盖。
async fn get_magic_record(
    conn: &mut MySqlConnection,
    player_id: i64,
    d: &mut CharacterData,
) -> Result<()> {
    d.magic = Some(vec![Some(MagicRcd::default()); MAGIC_SLOTS]);
    let rows = sqlx::query("SELECT * FROM characters_magic WHERE PlayerId=?")
        .bind(player_id)
        .fetch_all(&mut *conn)
        .await
        .context("查询 characters_magic 失败")?;
    for (position, row) in rows.iter().enumerate() {
        if position >= MAGIC_SLOTS {
            // C# `Magic[position]` 越界 → 抛异常（被 catch、保留已填部分）⇒ 这里停下。
            log::error("[Exception] GetMagicRecord（技能行数超过 20）");
            break;
        }
        let Some(slot) = d.magic.as_mut().and_then(|m| m.get_mut(position)) else {
            break;
        };
        let rec = MagicRcd {
            mag_idx: word_col(row, "MagicId")?,
            level: byte_col(row, "Level")?,
            // C# `dr.GetChar("UseKey")`：单 UTF-16 码元。这里取首码元（列是 char(2)）。
            magic_key: char_col(row, "UseKey")?,
            tran_point: i32::try_from(int_col(row, "CurrTrain")?).context("CurrTrain 越界")?,
        };
        *slot = Some(rec);
    }
    Ok(())
}

/// `char(2)` 列取首个 UTF-16 码元（C# `dr.GetChar`）。
fn char_col(row: &MySqlRow, name: &str) -> Result<u16> {
    let name = resolve_col(row, name)?;
    let s: String = row
        .try_get::<String, _>(name)
        .with_context(|| format!("列 {name} 不是文本（C# `GetChar` 预期 char）"))?;
    if s.is_empty() {
        return Ok(0);
    }
    Ok(s.chars()
        .next()
        .map_or(0, |c| u16::try_from(u32::from(c)).unwrap_or(0)))
}

/// C# `PlayDataStorage.GetItemRecord` 一类：把某张物品表读进定长槽位，再套 `QueryItemAttr`。
async fn get_item_slots(
    conn: &mut MySqlConnection,
    player_id: i64,
    table: &str,
    slots: usize,
) -> Vec<Option<ServerUserItem>> {
    // C# `GetItemRecord/GetBagItemRecord/GetStorageRecord` 的 `try { while (dr.Read()) …;
    // QueryItemAttr(…); } catch { LogService.Error(…); }`：**逐行**读，任何一行失败
    // （`GetInt32` 类型错、`array[pos]` 越界、网络/游标错误）都跳进 catch —— 已读入的槽位
    // **保留**，且**不再**执行 `QueryItemAttr`。旧实现用 `fetch_all(...).await?` 一次取完、
    // 失败即整表放弃（G7）；这里改成流式读 + 吞错，和 oracle 的「部分填充 + 继续」同口径。
    let sql = format!("SELECT * FROM {table} WHERE PlayerId=?");
    let mut items: Vec<Option<ServerUserItem>> = vec![None; slots];
    let mut read_failed = false;
    {
        let mut rows = sqlx::query(&sql).bind(player_id).fetch(&mut *conn);
        loop {
            let row = match rows.try_next().await {
                Ok(Some(row)) => row,
                Ok(None) => break,
                Err(e) => {
                    log::error(&format!("[Exception] PlayDataStorage.Get({table}): {e:#}"));
                    read_failed = true;
                    break;
                }
            };
            let parsed = (|| -> Result<usize> {
                let pos = i32::try_from(int_col(&row, "Position")?).context("Position 越界")?;
                let idx =
                    usize::try_from(pos).with_context(|| format!("{table} Position={pos} 为负"))?;
                if idx >= slots {
                    anyhow::bail!("{table} Position={pos} 超出 {slots} 槽（C# 同场景抛异常）");
                }
                let mut item = ServerUserItem::csharp_new();
                item.make_index =
                    i32::try_from(int_col(&row, "MakeIndex")?).context("MakeIndex 越界")?;
                item.index = word_col(&row, "StdIndex")?;
                item.dura = word_col(&row, "Dura")?;
                item.dura_max = word_col(&row, "DuraMax")?;
                items[idx] = Some(item);
                Ok(idx)
            })();
            if let Err(e) = parsed {
                log::error(&format!("[Exception] PlayDataStorage.Get({table}): {e:#}"));
                read_failed = true;
                break;
            }
        }
    }
    if read_failed {
        // C# catch：已填部分保留，`QueryItemAttr` 不执行。
        return items;
    }
    if let Err(e) = query_item_attr(conn, player_id, &mut items).await {
        log::error(&format!(
            "[Exception] PlayDataStorage.QueryItemAttr({table}): {e:#}"
        ));
    }
    items
}

/// C# `PlayDataStorage.QueryItemAttr`（**照搬既有 bug**）：
/// SQL 把 `MakeIndex` 列表拼成一个字符串参数（`in ('123,456')`），且回写用**行序**同时索引
/// 物品槽与 `Desc` 下标（`userItems[nPosition].Desc[nPosition]`）。
async fn query_item_attr(
    conn: &mut MySqlConnection,
    player_id: i64,
    items: &mut [Option<ServerUserItem>],
) -> Result<()> {
    let make_indexes: Vec<String> = items
        .iter()
        .flatten()
        .filter(|x| x.make_index > 0)
        .map(|x| x.make_index.to_string())
        .collect();
    if make_indexes.is_empty() {
        return Ok(());
    }
    let joined = make_indexes.join(",");
    let rows =
        sqlx::query("SELECT * FROM characters_item_attr WHERE PlayerId=? and MakeIndex in (?)")
            .bind(player_id)
            .bind(&joined)
            .fetch_all(&mut *conn)
            .await
            .context("查询 characters_item_attr 失败")?;
    for (position, row) in rows.iter().enumerate() {
        if position >= items.len() {
            log::error("[Exception] PlayDataStorage.QueryItemAttr（属性行数超过物品槽）");
            break;
        }
        let value = u8::try_from(int_col(row, &format!("VALUE{position}"))?).unwrap_or(0);
        if let Some(Some(item)) = items.get_mut(position) {
            if let Some(desc) = item.desc.as_mut() {
                if let Some(cell) = desc.get_mut(position) {
                    *cell = value;
                }
            }
        }
    }
    Ok(())
}

/// C# `PlayDataStorage.GetPlayerStatus`。
async fn get_player_status(
    conn: &mut MySqlConnection,
    player_id: i64,
    d: &mut CharacterData,
) -> Result<()> {
    let row = sqlx::query("SELECT * FROM characters_status WHERE PlayerId=?")
        .bind(player_id)
        .fetch_optional(&mut *conn)
        .await
        .context("查询 characters_status 失败")?;
    let Some(row) = row else {
        return Ok(());
    };
    for i in 0..STATUS_SLOTS {
        if let Some(slot) = d.status_time_arr.as_mut().and_then(|v| v.get_mut(i)) {
            *slot = word_col(&row, &format!("Status{i}"))?;
        }
    }
    Ok(())
}

/// C# `PlayDataStorage.GetRecord`：主表 + 七张附属表。
async fn get_record(
    conn: &mut MySqlConnection,
    player_id: i64,
) -> Result<Option<CharacterDataInfo>> {
    if player_id == 0 {
        return Ok(None);
    }
    let Some(mut data) = get_chr_record(conn, player_id).await? else {
        // C# 此处 `humanRcd.Data` 为 null，紧接着 `.ChrName` NRE ⇒ 上层 catch 返回 false。
        return Ok(None);
    };
    let mut info = CharacterDataInfo::csharp_new();
    info.header.set_name(data.chr_name.clone());
    data.abil = Some(get_abil_record(conn, player_id).await?);
    get_bonus_abil_record(conn, player_id, &mut data).await?;
    get_magic_record(conn, player_id, &mut data).await?;
    data.hum_items = Some(get_item_slots(conn, player_id, "characters_item", HUM_ITEM_SLOTS).await);
    data.bag_items =
        Some(get_item_slots(conn, player_id, "characters_bagitem", BAG_ITEM_SLOTS).await);
    data.storage_items = Some(
        get_item_slots(
            conn,
            player_id,
            "characters_storageitem",
            STORAGE_ITEM_SLOTS,
        )
        .await,
    );
    get_player_status(conn, player_id, &mut data).await?;
    info.data = Some(data);
    Ok(Some(info))
}

// ==== HUtil32 位运算 ====

fn lo_word(v: i32) -> u16 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lo = v as u16;
    lo
}

fn hi_word(v: i32) -> u16 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let hi = (v >> 16) as u16;
    hi
}

// ==== 存档（Save） ====

const SAVE_CHARACTERS_SQL: &str = "UPDATE characters SET ServerIndex = ?, LoginID = ?, MapName = ?, CX = ?, CY = ?, Level = ?, Dir = ?, Hair = ?, Sex = ?, Job = Job, Gold = ?, \
     GamePoint = ?, HomeMap = ?, HomeX = ?, HomeY = ?, PkPoint = ?, ReLevel = ?, AttatckMode = ?, FightZoneDieCount = ?, BodyLuck = ?, IncHealth = ?, IncSpell = ?, \
     IncHealing = ?, CreditPoint = ?, BonusPoint = ?, HungerStatus = ?, PayMentPoint = ?, LockLogon = ?, MarryCount = ?, AllowGroupReCall = ?, \
     GroupRcallTime = ?, AllowGuildReCall = ?, IsMaster = ?, MasterName = ?, DearName = ?, StoragePwd = ?, Deleted = ?, LASTUPDATE = now() WHERE ID = ?";

/// C# `PlayDataStorage.SaveRecord(context, playerId, hd)`。
async fn save_characters(
    conn: &mut MySqlConnection,
    player_id: i64,
    d: &CharacterData,
    abil: &Ability,
) -> Result<()> {
    sqlx::query(SAVE_CHARACTERS_SQL)
        .bind(i32::from(d.server_index))
        .bind(d.account.as_deref())
        .bind(d.cur_map.as_deref())
        .bind(d.cur_x)
        .bind(d.cur_y)
        .bind(i16::from(abil.level))
        .bind(i16::from(d.dir))
        .bind(i16::from(d.hair))
        .bind(i16::from(d.sex))
        // 注意：SQL 是 `Job = Job`（不更新），没有 `?` 占位符。C# 里 `@Job` 是**具名**参数，
        // MySqlConnector 忽略未用到的具名参数；sqlx 是**位置**绑定，这里必须**不 bind**，
        // 否则后面所有参数整体错位（实测会把 WHERE ID 错绑成 0 ⇒ 静默 0 行更新）。
        .bind(d.gold)
        .bind(d.game_point)
        .bind(d.home_map.as_deref())
        .bind(d.home_x)
        .bind(d.home_y)
        .bind(d.pk_point)
        .bind(i16::from(d.re_level))
        .bind(i16::from(d.attatck_mode))
        .bind(i16::from(d.fight_zone_die_count))
        .bind(d.body_luck)
        .bind(i16::from(d.inc_health))
        .bind(i16::from(d.inc_spell))
        .bind(i16::from(d.inc_healing))
        .bind(i16::from(d.credit_point))
        .bind(d.bonus_point)
        .bind(d.hunger_status)
        .bind(d.pay_ment_point)
        .bind(d.lock_logon)
        .bind(i16::from(d.marry_count))
        // C# 既有 bug：`@AllowGroupReCall` 绑的是 `hd.AllowGroup`。
        .bind(i16::from(d.allow_group))
        .bind(d.group_rcall_time)
        .bind(d.allow_guild_re_call)
        .bind(d.is_master)
        .bind(d.master_name.as_deref())
        .bind(d.dear_name.as_deref())
        .bind(d.storage_pwd.as_deref())
        .bind(0_i16) // @Deleted 恒 0
        .bind(player_id)
        .execute(&mut *conn)
        .await
        .context("更新 characters 失败")?;
    Ok(())
}

/// C# `PlayDataStorage.SaveAblity`（`Ac = @Ac` 绑的是 `Level`，既有 bug）。
async fn save_ablity(conn: &mut MySqlConnection, player_id: i64, abil: &Ability) -> Result<()> {
    sqlx::query(
        "UPDATE characters_ablity SET Level = ?,Ac = ?, Mac = ?, Dc = ?, Mc = ?, Sc = ?, Hp = ?, Mp = ?, MaxHP = ?, MAxMP = ?, Exp = ?, MaxExp = ?, Weight = ?, MaxWeight = ?, WearWeight = ?,MaxWearWeight = ?, HandWeight = ?, MaxHandWeight = ?,ModifyTime=now() WHERE PlayerId = ?",
    )
    .bind(i16::from(abil.level))
    // 既有 bug：`@Ac` 绑的是 `Abil.Level`。
    .bind(i16::from(abil.level))
    .bind(i32::from(abil.mac))
    .bind(i32::from(abil.dc))
    .bind(i32::from(abil.mc))
    .bind(i32::from(abil.sc))
    .bind(i32::from(abil.hp))
    .bind(i32::from(abil.mp))
    .bind(i32::from(abil.max_hp))
    .bind(i32::from(abil.max_mp))
    .bind(abil.exp)
    .bind(abil.max_exp)
    .bind(i32::from(abil.weight))
    .bind(i32::from(abil.max_weight))
    .bind(i16::from(abil.wear_weight))
    .bind(i16::from(abil.max_wear_weight))
    .bind(i16::from(abil.hand_weight))
    .bind(i16::from(abil.max_hand_weight))
    .bind(player_id)
    .execute(&mut *conn)
    .await
    .context("更新 characters_ablity 失败")?;
    Ok(())
}

/// C# `PlayDataStorage.CreateAblity`（`Add.cs:157-193`）：**INSERT** 一行 `characters_ablity`。
///
/// 与 [`save_ablity`] 同列、同值（含 `Ac = Level` 的既有 bug），只是语句形态为 INSERT ——
/// `Add` 兜底建角路径旧实现误用 UPDATE，新角色没有行 ⇒ 影响 0 行，整表字段永不落库（G-20）。
async fn create_ablity(conn: &mut MySqlConnection, player_id: i64, abil: &Ability) -> Result<()> {
    sqlx::query(
        "INSERT INTO characters_ablity (PlayerId, Level, Ac, Mac, Dc, Mc, Sc, Hp, Mp, MaxHP, MAxMP, Exp, MaxExp, Weight, MaxWeight, WearWeight,MaxWearWeight, HandWeight, MaxHandWeight) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(player_id)
    .bind(i16::from(abil.level))
    // 既有 bug：`@Ac` 绑的是 `Abil.Level`（与 `save_ablity` 同口径）。
    .bind(i16::from(abil.level))
    .bind(i32::from(abil.mac))
    .bind(i32::from(abil.dc))
    .bind(i32::from(abil.mc))
    .bind(i32::from(abil.sc))
    .bind(i32::from(abil.hp))
    .bind(i32::from(abil.mp))
    .bind(i32::from(abil.max_hp))
    .bind(i32::from(abil.max_mp))
    .bind(abil.exp)
    .bind(abil.max_exp)
    .bind(i32::from(abil.weight))
    .bind(i32::from(abil.max_weight))
    .bind(i16::from(abil.wear_weight))
    .bind(i16::from(abil.max_wear_weight))
    .bind(i16::from(abil.hand_weight))
    .bind(i16::from(abil.max_hand_weight))
    .execute(&mut *conn)
    .await
    .context("插入 characters_ablity 失败")?;
    Ok(())
}

/// `ComparerUserItem` 的差量结果：`(要改的槽, 要删的槽)`。
type ItemDiff = (Vec<Option<ServerUserItem>>, Vec<Option<ServerUserItem>>);

/// C# `ComparerUserItem`：算出要删（`del`）与要改（`chg`）的槽位。
/// `old` 为空的槽直接跳过；`new` 在 `old` 非空时为 `None` ⇒ C# NRE（这里返回 Err，由调用方吞掉）。
fn comparer_user_item(
    new_items: &[Option<ServerUserItem>],
    old_items: &[Option<ServerUserItem>],
) -> Result<ItemDiff> {
    let mut chg = vec![None; new_items.len()];
    let mut del = vec![None; new_items.len()];
    for i in 0..new_items.len() {
        let Some(old) = old_items.get(i).and_then(Option::as_ref) else {
            continue;
        };
        let Some(new) = new_items[i].as_ref() else {
            anyhow::bail!("newItems[{i}] 为 null 而 oldItems[{i}] 非空（C# 同场景 NRE）");
        };
        if new.make_index == 0 && old.make_index > 0 {
            del[i] = Some(old.clone());
            continue;
        }
        if new.make_index > 0 || new.make_index > old.make_index {
            chg[i] = Some(new.clone());
            continue;
        }
        if old.index == 0 && old.make_index == 0 && new.make_index > 0 && new.index > 0 {
            del[i] = Some(new.clone());
        }
    }
    Ok((chg, del))
}

/// C# `PlayDataStorage.ClearItemAttr`（拼文本、整数内联，逐条执行）。
async fn clear_item_attr(
    conn: &mut MySqlConnection,
    player_id: i64,
    make_index: &[i32],
) -> Result<()> {
    if make_index.is_empty() {
        return Ok(());
    }
    for mk in make_index {
        let sql = format!(
            "UPDATE characters_item_attr SET MakeIndex=0,VALUE0=0,VALUE1=0,VALUE2=0,VALUE3=0,VALUE4=0,VALUE5=0,VALUE6=0,VALUE7=0,VALUE8=0,VALUE9=0,VALUE10=0,VALUE11=0,VALUE12=0,VALUE13=0 WHERE PlayerId={player_id} AND MakeIndex ={mk};"
        );
        sqlx::query(&sql)
            .execute(&mut *conn)
            .await
            .context("更新 characters_item_attr（清零）失败")?;
    }
    Ok(())
}

/// C# `PlayDataStorage.UpdateItemAttr`（拼文本、整数内联，逐条执行）。
async fn update_item_attr(
    conn: &mut MySqlConnection,
    player_id: i64,
    items: &[Option<ServerUserItem>],
) -> Result<()> {
    // C# `UpdateItemAttr` 先把**整批** SQL 拼进 `strSqlList`、最后一次性执行：任一非空物品的
    // `Desc` 为 null（NRE）或长度 < 14（越界）都会在拼串阶段被内层 `catch` 吞掉 ⇒
    // **整批一行不改**。旧实现逐条 `unwrap_or_else(|| vec![0; 14])` 补 0 执行 ⇒ 把
    // `VALUE0..VALUE13` 全写 0（数据丢失，LB-06）。
    let pending: Vec<&ServerUserItem> = items.iter().flatten().collect();
    if pending
        .iter()
        .any(|i| i.desc.as_ref().is_none_or(|d| d.len() < 14))
    {
        log::error(
            "[Exception] PlayDataStorage.UpdateItemAttr: 物品 Desc 缺失或长度不足（C# NRE 被 catch ⇒ 整批跳过）",
        );
        return Ok(());
    }
    for item in pending {
        let desc = item.desc.as_ref().expect("上面已确保 Desc 长度 >= 14");
        let sql = format!(
            "UPDATE characters_item_attr SET VALUE0={},VALUE1={},VALUE2={},VALUE3={},VALUE4={},VALUE5={},VALUE6={},VALUE7={},VALUE8={},VALUE9={},VALUE10={},VALUE11={},VALUE12={},VALUE13={} WHERE PlayerId={player_id} AND MakeIndex={};",
            desc[0], desc[1], desc[2], desc[3], desc[4], desc[5], desc[6], desc[7], desc[8],
            desc[9], desc[10], desc[11], desc[12], desc[13], item.make_index
        );
        sqlx::query(&sql)
            .execute(&mut *conn)
            .await
            .context("更新 characters_item_attr 失败")?;
    }
    Ok(())
}

/// 某张物品表的定长差量存档（C# `SaveItem`/`SaveBagItem`/`SaveStorageItem` 三份的共性）。
async fn save_item_table(
    conn: &mut MySqlConnection,
    player_id: i64,
    table: &str,
    new_items: &[Option<ServerUserItem>],
    slots: usize,
) -> Result<()> {
    // C# `SaveItem`/`SaveBagItem`/`SaveStorageItem` 先 `Get*ItemRecord`（内部 `try/catch`）：
    // 读失败只留下部分/空 `oldItems`，随后**照常**进 `ComparerUserItem` 与写循环。
    let old_items = get_item_slots(conn, player_id, table, slots).await;
    let (chg, del) = comparer_user_item(new_items, &old_items)?;
    // 删除循环
    for (i, d) in del.iter().enumerate() {
        let Some(d) = d else { continue };
        let sql = format!(
            "UPDATE {table} SET Position = ?, MakeIndex = 0, StdIndex = 0, Dura = 0, DuraMax = 0 WHERE PlayerId = ? AND Position = ? AND MakeIndex = ? AND StdIndex = ?;"
        );
        sqlx::query(&sql)
            .bind(i32::try_from(i).unwrap_or(i32::MAX))
            .bind(player_id)
            .bind(i32::try_from(i).unwrap_or(i32::MAX))
            .bind(d.make_index)
            .bind(i32::from(d.index))
            .execute(&mut *conn)
            .await
            .with_context(|| format!("清空 {table} 槽位 {i} 失败"))?;
    }
    let del_mi: Vec<i32> = del
        .iter()
        .flatten()
        .filter(|x| x.make_index > 0)
        .map(|x| x.make_index)
        .collect();
    if let Err(e) = clear_item_attr(conn, player_id, &del_mi).await {
        log::error(&format!("[Exception] {table} ClearItemAttr: {e:#}"));
    }
    // 更新循环
    for (i, c) in chg.iter().enumerate() {
        let Some(c) = c else { continue };
        let sql = format!(
            "UPDATE {table} SET Position = ?, MakeIndex =?, StdIndex = ?, Dura = ?, DuraMax = ? WHERE PlayerId = ? AND Position = ?;"
        );
        sqlx::query(&sql)
            .bind(i32::try_from(i).unwrap_or(i32::MAX))
            .bind(c.make_index)
            .bind(i32::from(c.index))
            .bind(i32::from(c.dura))
            .bind(i32::from(c.dura_max))
            .bind(player_id)
            .bind(i32::try_from(i).unwrap_or(i32::MAX))
            .execute(&mut *conn)
            .await
            .with_context(|| format!("更新 {table} 槽位 {i} 失败"))?;
    }
    if let Err(e) = update_item_attr(conn, player_id, &chg).await {
        log::error(&format!("[Exception] {table} UpdateItemAttr: {e:#}"));
    }
    Ok(())
}

/// C# `PlayDataStorage.SaveMagics`：先 `DELETE`（**在 try 之外**，失败会冒泡触发回滚），
/// 再拼文本逐条 `INSERT`（`MagIdx > 0` 才写）。
async fn save_magics(
    conn: &mut MySqlConnection,
    player_id: i64,
    magic: &[Option<MagicRcd>],
) -> Result<()> {
    sqlx::query("DELETE FROM characters_magic WHERE PlayerId=?")
        .bind(player_id)
        .execute(&mut *conn)
        .await
        .context("DELETE characters_magic 失败")?;
    for m in magic.iter().flatten() {
        if m.mag_idx == 0 {
            continue;
        }
        let key = char::from_u32(u32::from(m.magic_key)).unwrap_or('\0');
        let sql = format!(
            "INSERT INTO characters_magic(PlayerId,MagicId,Level,Usekey,CurrTrain) VALUES ({player_id},{},{},'{key}',{});",
            m.mag_idx, m.level, m.tran_point
        );
        // C# 这些 INSERT 在 try 内、失败只打日志，不冒泡。
        if let Err(e) = sqlx::query(&sql).execute(&mut *conn).await {
            log::error(&format!("[Exception] PlayDataStorage.SaveMagics: {e:#}"));
        }
    }
    Ok(())
}

/// C# `PlayDataStorage.SaveBonusability`。
async fn save_bonusability(
    conn: &mut MySqlConnection,
    player_id: i64,
    b: &NakedAbility,
) -> Result<()> {
    sqlx::query(
        "UPDATE characters_bonusability SET AC=?, MAC=?, DC=?, MC=?, SC=?, HP=?, MP=?, HIT=?, SPEED=?, RESERVED=? WHERE PlayerId=?",
    )
    .bind(i32::from(b.ac))
    .bind(i32::from(b.mac))
    .bind(i32::from(b.dc))
    .bind(i32::from(b.mc))
    .bind(i32::from(b.sc))
    .bind(i32::from(b.hp))
    .bind(i32::from(b.mp))
    .bind(i32::from(b.hit))
    .bind(b.speed)
    .bind(i32::from(b.reserved))
    .bind(player_id)
    .execute(&mut *conn)
    .await
    .context("更新 characters_bonusability 失败")?;
    Ok(())
}

/// C# `PlayDataStorage.SaveStatus`。
async fn save_status(conn: &mut MySqlConnection, player_id: i64, status: &[u16]) -> Result<()> {
    let mut sql = String::from("UPDATE characters_status SET ");
    for i in 0..STATUS_SLOTS {
        if i > 0 {
            sql.push(',');
        }
        let _ = write!(sql, "Status{i}=?");
    }
    sql.push_str(" WHERE PlayerId=?;");
    let mut q = sqlx::query(&sql);
    for i in 0..STATUS_SLOTS {
        q = q.bind(i32::from(status.get(i).copied().unwrap_or(0)));
    }
    q.bind(player_id)
        .execute(&mut *conn)
        .await
        .context("更新 characters_status 失败")?;
    Ok(())
}

/// C# `PlayDataStorage.SaveRecord(context, playerId, humanRcd)`：
/// 一个事务里跑 8 个 `Save*`（各自吞错），只有 `SaveMagics` 的 `DELETE` 与 `Commit` 失败会回滚。
#[allow(clippy::too_many_lines)] // 逐段照抄 oracle 的 8 个 Save*，拆函数会打散行号对照
async fn save_record(
    conn: &mut MySqlConnection,
    player_id: i64,
    info: &CharacterDataInfo,
) -> Result<bool> {
    let Some(d) = info.data.as_ref() else {
        return Ok(false);
    };
    // C# `SaveRecord(context, playerId, hd)` 的**参数绑定**在它自己的内层 try 之外：
    // `hd.Abil.Level` 为 null 时 NRE 冒到外层 `SaveRecord` 的 catch ⇒ `RollBack()` +
    // `result=false` ⇒ **8 张表一行不改**（不是只跳过 characters_ablity）。
    // `hd.HumItems` 同理（`SaveItem` 的 `userItems.Length` 在 SaveItem 的 try 之外）。
    // 判据：`partial_save_tests`（LB-06 重写）。
    if d.abil.is_none() {
        log::error(
            "[Exception] PlayDataStorage.UpdateRecord: Abil 缺失（C# 同场景 NRE ⇒ 整次存档回滚）",
        );
        return Ok(false);
    }
    if d.hum_items.is_none() {
        log::error(
            "[Exception] PlayDataStorage.SaveItem: HumItems 缺失（C# 同场景 NRE ⇒ 整次存档回滚）",
        );
        return Ok(false);
    }
    let Some(abil) = d.abil.as_ref() else {
        return Ok(false);
    };
    let mut tx = conn.begin().await.context("开启存档事务失败")?;
    if let Err(e) = save_characters(&mut tx, player_id, d, abil).await {
        log::error(&format!("[Exception] PlayDataStorage.UpdateRecord: {e:#}"));
    }
    if let Err(e) = save_ablity(&mut tx, player_id, abil).await {
        log::error(&format!("[Exception] PlayDataStorage.SaveAblity: {e:#}"));
    }
    if let Err(e) = save_item_table(
        &mut tx,
        player_id,
        "characters_item",
        d.hum_items.as_deref().unwrap_or(&[]),
        HUM_ITEM_SLOTS,
    )
    .await
    {
        log::error(&format!("[Exception] PlayDataStorage.SaveItem: {e:#}"));
    }
    if let Err(e) = save_item_table(
        &mut tx,
        player_id,
        "characters_bagitem",
        d.bag_items.as_deref().unwrap_or(&[]),
        BAG_ITEM_SLOTS,
    )
    .await
    {
        log::error(&format!("[Exception] PlayDataStorage.SaveBagItem: {e:#}"));
    }
    if let Err(e) = save_item_table(
        &mut tx,
        player_id,
        "characters_storageitem",
        d.storage_items.as_deref().unwrap_or(&[]),
        STORAGE_ITEM_SLOTS,
    )
    .await
    {
        log::error(&format!(
            "[Exception] PlayDataStorage.SaveStorageItem: {e:#}"
        ));
    }
    // `SaveMagics` 的 DELETE 在 C# try 之外 ⇒ 失败要回滚整个存档。
    if let Err(e) = save_magics(&mut tx, player_id, d.magic.as_deref().unwrap_or(&[])).await {
        log::error(&format!("[Exception] PlayDataStorage.SaveMagics: {e:#}"));
        tx.rollback().await.ok();
        return Ok(false);
    }
    if let Some(bonus) = d.bonus_abil.as_ref() {
        if let Err(e) = save_bonusability(&mut tx, player_id, bonus).await {
            log::error(&format!(
                "[Exception] PlayDataStorage.SaveBonusability: {e:#}"
            ));
        }
    } else {
        // C# `SaveBonusability` 的内层 try：`bonusAbil.AC` NRE 被 catch ⇒ 该表一行不改。
        log::error(
            "[Exception] PlayDataStorage.SaveBonusability: BonusAbil 缺失（C# NRE 被 catch ⇒ 该表不改）",
        );
    }
    match d.status_time_arr.as_deref() {
        Some(status) if status.len() >= STATUS_SLOTS => {
            if let Err(e) = save_status(&mut tx, player_id, status).await {
                log::error(&format!("[Exception] PlayDataStorage.SaveStatus: {e:#}"));
            }
        }
        _ => {
            // C# `SaveStatus` 的内层 try：`statusTimeArr[0..14]` NRE/越界被 catch ⇒ 该表一行不改。
            log::error(
                "[Exception] PlayDataStorage.SaveStatus: StatusTimeArr 缺失或长度不足（C# NRE 被 catch ⇒ 该表不改）",
            );
        }
    }
    if let Err(e) = tx.commit().await {
        log::error(&format!(
            "[Exception] PlayDataStorage.SaveRecord 提交失败: {e:#}"
        ));
        return Ok(false);
    }
    // C# `SaveRecord` 在 `context.Commit()` 后 `LogService.Debug($"保存角色[{name}]数据成功")`。
    log::debug(&format!(
        "保存角色[{}]数据成功",
        info.header.name.as_deref().unwrap_or_default()
    ));
    Ok(true)
}

// ==== 新建（Add） ====

/// C# `PlayDataStorage.CreateCharacters`（写完整 `CharacterData` 字段，`AllowGroupReCall` 收的是 `AllowGroup`）。
async fn create_characters(conn: &mut MySqlConnection, info: &CharacterDataInfo) -> Result<i64> {
    let Some(d) = info.data.as_ref() else {
        return Ok(0);
    };
    let abil = d.abil.clone().unwrap_or_default();
    let res = sqlx::query(
        "insert into characters (ServerIndex, LoginID, ChrName, MapName, CX, CY, Level, Dir, Hair, Sex, Job, Gold, GamePoint, HomeMap, \
         HomeX, HomeY, PkPoint, ReLevel, AttatckMode, FightZoneDieCount, BodyLuck, IncHealth,IncSpell, IncHealing, CreditPoint, BonusPoint, \
         HungerStatus, PayMentPoint, LockLogon, MarryCount, AllowGroupReCall, GroupRcallTime, AllowGuildReCall, IsMaster, MasterName, DearName \
         ,StoragePwd, Deleted, CREATEDATE, LASTUPDATE) VALUES \
         (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, now(),now());",
    )
    .bind(i32::from(d.server_index))
    .bind(d.account.as_deref())
    .bind(d.chr_name.as_deref())
    .bind(d.cur_map.as_deref())
    .bind(d.cur_x)
    .bind(d.cur_y)
    .bind(i16::from(abil.level))
    .bind(i16::from(d.dir))
    .bind(i16::from(d.hair))
    .bind(i16::from(d.sex))
    .bind(i16::from(d.job))
    .bind(d.gold)
    .bind(d.game_point)
    .bind(d.home_map.as_deref())
    .bind(d.home_x)
    .bind(d.home_y)
    .bind(d.pk_point)
    .bind(i16::from(d.re_level))
    .bind(i16::from(d.attatck_mode))
    .bind(i16::from(d.fight_zone_die_count))
    .bind(d.body_luck)
    .bind(i16::from(d.inc_health))
    .bind(i16::from(d.inc_spell))
    .bind(i16::from(d.inc_healing))
    .bind(i16::from(d.credit_point))
    .bind(d.bonus_point)
    .bind(d.hunger_status)
    .bind(d.pay_ment_point)
    .bind(d.lock_logon)
    .bind(i16::from(d.marry_count))
    .bind(i16::from(d.allow_group))
    .bind(d.group_rcall_time)
    .bind(d.allow_guild_re_call)
    .bind(d.is_master)
    .bind(d.master_name.as_deref())
    .bind(d.dear_name.as_deref())
    .bind(d.storage_pwd.as_deref())
    .bind(0_i16)
    .execute(&mut *conn)
    .await
    .context("插入 characters 失败")?;
    #[allow(clippy::cast_possible_wrap)]
    Ok(res.last_insert_id() as i64)
}

/// C# `PlayDataStorage.CreateAblity`（同样 `Ac = Level` 的既有 bug）+ `CreateStatus`。
async fn create_ablity_status(
    conn: &mut MySqlConnection,
    player_id: i64,
    d: &CharacterData,
) -> Result<()> {
    if let Err(e) = create_ablity(conn, player_id, &d.abil.clone().unwrap_or_default()).await {
        log::error(&format!("[Exception] PlayDataStorage.CreateAblity: {e:#}"));
    }
    if let Err(e) = sqlx::query(
        "INSERT INTO characters_status (PlayerId, Status0, Status1, Status2, Status3, Status4, Status5, Status6, Status7, Status8, Status9, Status10, Status11, Status12, Status13, Status14, Status15) VALUES (?, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);",
    )
    .bind(player_id)
    .execute(&mut *conn)
    .await
    {
        log::error(&format!("[Exception] PlayDataStorage.CreateStatus: {e:#}"));
    }
    Ok(())
}

/// C# `PlayDataStorage.CreateItemAttr`（`Add.cs::CreateItemAttr`）：建角物品表的属性 INSERT。
///
/// **oracle 的门**：`null` 与 `MakeIndex <= 0` 的物品一律跳过；其余逐条 INSERT
/// `VALUE0..VALUE13 = Desc[0..13]`。三条建角物品路径（`characters_item` /
/// `characters_bagitem` / `characters_storageitem`）传入的都是 `new UserItem()`
/// （`MakeIndex=0`）⇒ **一行也不插**。这是 oracle 侧既有缺陷（G-01），本批按等价原则
/// **照搬门与调用点**，不做 upsert 修正（修正会偏离 oracle，另立批次）。
pub(crate) async fn create_item_attr(
    conn: &mut MySqlConnection,
    player_id: i64,
    items: &[Option<ServerUserItem>],
) -> Result<()> {
    let mut statements = Vec::new();
    for item in items.iter().flatten() {
        // C# `if (userItems[i] == null || userItems[i].MakeIndex <= 0) continue;`
        if item.make_index <= 0 {
            continue;
        }
        let desc = item.desc.clone().unwrap_or_else(|| vec![0; 14]);
        let v: Vec<u8> = (0..14).map(|i| desc.get(i).copied().unwrap_or(0)).collect();
        statements.push(format!(
            "INSERT INTO characters_item_attr (PlayerId,MakeIndex,VALUE0,VALUE1,VALUE2,VALUE3,VALUE4,VALUE5,VALUE6,VALUE7,VALUE8,VALUE9,VALUE10,VALUE11,VALUE12,VALUE13) VALUES ({player_id},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{});",
            item.make_index, v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12], v[13]
        ));
    }
    if statements.is_empty() {
        return Ok(());
    }
    // C# 用 string.Join 把多条语句拼成一个多语句命令执行；这里在同一连接/事务上
    // 逐条按序执行，语句集合与顺序一致（失败时已执行的语句保留，与多语句批处理同口径）。
    for sql in statements {
        sqlx::query(&sql)
            .execute(&mut *conn)
            .await
            .context("插入 characters_item_attr 失败")?;
    }
    Ok(())
}

/// C# `CreateUseItem`/`CreateBagItem`/`CreateStorageItem` 在「新角色」下的等价物：
/// 新角色这些表还没有行 ⇒ `oldItems` 全空 ⇒ 每个槽都插一行全 0；随后照 oracle 调
/// `CreateItemAttr(..., addItem)`——`addItem` 全是 `new UserItem()`（`MakeIndex=0`）
/// ⇒ 同一个门把全部槽位挡掉，**attr 表零行**（G-01，照搬）。
async fn create_empty_items(
    conn: &mut MySqlConnection,
    player_id: i64,
    table: &str,
    count: i32,
    swallow: bool,
) -> Result<()> {
    let sql = format!(
        "INSERT INTO {table} (PlayerId, Position, MakeIndex, StdIndex, Dura, DuraMax) VALUES (?, ?, 0, 0, 0, 0);"
    );
    let mut items: Vec<Option<ServerUserItem>> = Vec::new();
    for pos in 0..count {
        match sqlx::query(&sql)
            .bind(player_id)
            .bind(pos)
            .execute(&mut *conn)
            .await
        {
            Ok(_) => items.push(Some(ServerUserItem::csharp_new())),
            Err(e) => {
                if !swallow {
                    // C# `CreateUseItem`/`CreateBagItem` 的 INSERT 循环在 `AddRecord` 的
                    // try **之外** ⇒ 异常冒到 `AddRecord` 的 catch ⇒ 整次建角回滚
                    // （含主表 characters 行）。调用方负责回滚事务。
                    return Err(anyhow::anyhow!(
                        "建角附属表 {table} 写入失败（C# 在 try 外 ⇒ 整次建角回滚）: {e}"
                    ));
                }
                // C# `CreateStorageItem` 的循环在自己的 try 内 ⇒ 吞错并继续。
                log::error(&format!(
                    "[Exception] PlayDataStorage.CreateItem({table}): {e:#}"
                ));
                break;
            }
        }
    }
    if let Err(e) = create_item_attr(conn, player_id, &items).await {
        // C# `CreateItemAttr` 自带 try/catch（调用点外层还有一层）：只打日志、不冒泡。
        log::error(&format!(
            "[Exception] PlayDataStorage.CreateItemAttr({table}): {e:#}"
        ));
    }
    Ok(())
}

// ==== 对外（DataService 调用） ====

impl Db {
    /// C# `PlayDataStorage.Index(chrName)`：`characters` 名字（Unicode，大小写不敏感）→ Id。
    #[must_use]
    pub fn play_data_index_text(&self, name: &str) -> Option<i64> {
        self.name_text.get(&name.to_lowercase()).copied()
    }

    /// C# `HUtil32.GetBytes(str)`（GB2312 编码）：`EDCode.EncodeString` 用它把 Unicode 名
    /// 变成 GBK 字节。这里让 MySQL 做同一转换（与 `INDEX_COLS` 的 `CONVERT(… USING gbk)` 同口径）。
    pub async fn gbk_bytes(&self, s: &str) -> Result<Vec<u8>> {
        let row = sqlx::query("SELECT CAST(CONVERT(? USING gbk) AS BINARY) AS b")
            .bind(s)
            .fetch_one(&self.pool)
            .await
            .context("GBK 转换失败")?;
        row.try_get::<Vec<u8>, _>("b").context("GBK 字节读取失败")
    }

    /// C# `PlayDataStorage.Get(chrName, ref humanRcd)`：
    /// 命中索引 → 读整份存档；索引未命中或读取失败都返回 `None`（调用方分别对应 `-3`/`-2`）。
    pub async fn play_data_get(&self, name: &str) -> Result<Option<CharacterDataInfo>> {
        if name.is_empty() {
            return Ok(None);
        }
        let Some(id) = self.play_data_index_text(name) else {
            return Ok(None);
        };
        let mut conn = self.pool.acquire().await.context("取连接失败")?;
        match get_record(&mut conn, id).await {
            Ok(v) => Ok(v),
            Err(e) => {
                log::error(&format!("获取角色[{id}]数据失败. {e:#}"));
                Ok(None)
            }
        }
    }

    /// C# `PlayDataStorage.Update(chrName, humanRcd)`：命中索引才存档。
    pub async fn play_data_update(&self, name: &str, info: &CharacterDataInfo) -> Result<bool> {
        let Some(id) = self.play_data_index_text(name) else {
            return Ok(false);
        };
        let mut conn = self.pool.acquire().await.context("取连接失败")?;
        save_record(&mut conn, id, info).await
    }

    /// C# `PlayDataStorage.Add(humanRcd)`：名字已存在即 false；否则建角色并更新内存索引。
    pub async fn play_data_add(&mut self, info: &CharacterDataInfo) -> Result<bool> {
        let Some(name) = info.header.name.clone() else {
            return Ok(false);
        };
        if self.name_text.contains_key(&name.to_lowercase()) {
            return Ok(false);
        }
        // C# `PlayDataStorage` 只有一份字符串键的 `_NameQuickMap`（`PlayDataStorage.Query.cs:16-23`），
        // `Add` 成功即 `_NameQuickMap.Add(sChrName, playerId)`（`Add.cs:42-47`）⇒ 6000 面新建的
        // 角色**立刻**对 5100 面的 `Index/Query/Select` 可见（G6）。Rust 的 5100 面读的是 GBK
        // 字节键的 [`Db::name_quick`]，故这里按同一口径转 GBK（`add` 后不再查库，只写内存索引）。
        let name_gbk = self
            .gbk_bytes(&name)
            .await
            .context("角色名 GBK 转换失败（play_data_add 更新 name_quick 用）")?;
        let Some(d) = info.data.as_ref() else {
            return Ok(false);
        };
        // C# `AddRecord` → `CreateCharacters` 的 `hd.Abil.Level`（try 内）对 null NRE ⇒
        // catch ⇒ 整次建角回滚、`Add` 返回 false。照搬：不建行、不写内存索引。
        if d.abil.is_none() {
            log::error(
                "[Exception] PlayDataStorage.CreateCharacters: Abil 缺失（C# NRE ⇒ 整次建角回滚）",
            );
            return Ok(false);
        }
        let mut tx = self.pool.begin().await.context("开启建角事务失败")?;
        let player_id = create_characters(&mut tx, info).await?;
        if player_id <= 0 {
            tx.rollback().await.ok();
            return Ok(false);
        }
        create_ablity_status(&mut tx, player_id, d).await?;
        // `characters_item` / `characters_bagitem` 的 Oracle INSERT 循环在 try 外 ⇒
        // 失败即整次建角回滚；`characters_storageitem` 在 try 内 ⇒ 吞错并提交（LB-07）。
        if let Err(e) = create_empty_items(&mut tx, player_id, "characters_item", 13, false).await {
            tx.rollback().await.ok();
            return Err(e);
        }
        if let Err(e) =
            create_empty_items(&mut tx, player_id, "characters_bagitem", 46, false).await
        {
            tx.rollback().await.ok();
            return Err(e);
        }
        if let Err(e) =
            create_empty_items(&mut tx, player_id, "characters_storageitem", 50, true).await
        {
            tx.rollback().await.ok();
            return Err(e);
        }
        tx.commit().await.context("提交建角事务失败")?;
        self.name_text.insert(name.to_lowercase(), player_id);
        self.name_quick.insert(fold_key(&name_gbk), player_id);
        self.char_ids.insert(player_id);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hi_lo_word_match_csharp() {
        // C# `HUtil32.LoWord/HiWord`
        let v: i32 = (7 << 16) | 0x012C;
        assert_eq!(lo_word(v), 300);
        assert_eq!(hi_word(v), 7);
        assert_eq!(hi_word(-1), 0xFFFF);
        assert_eq!(lo_word(-1), 0xFFFF);
    }

    /// `ComparerUserItem`：清空（新 0 / 旧 >0）进 `del`；换物进 `chg`。
    #[test]
    fn comparer_matches_csharp_branches() {
        let mut old = vec![None; 3];
        let mut new = vec![None; 3];
        // 槽 0：旧有物品、新的 MakeIndex=0 ⇒ del = 旧
        old[0] = Some(ServerUserItem {
            make_index: 5,
            index: 9,
            ..ServerUserItem::csharp_new()
        });
        new[0] = Some(ServerUserItem::csharp_new());
        // 槽 1：新有物品 ⇒ chg
        old[1] = Some(ServerUserItem::csharp_new());
        new[1] = Some(ServerUserItem {
            make_index: 8,
            index: 2,
            ..ServerUserItem::csharp_new()
        });
        let (chg, del) = comparer_user_item(&new, &old).unwrap();
        assert_eq!(del[0].as_ref().unwrap().make_index, 5);
        assert!(del[1].is_none());
        assert_eq!(chg[1].as_ref().unwrap().make_index, 8);
        assert!(chg[0].is_none());
    }

    /// `new` 在 `old` 非空处为 null ⇒ C# NRE；这里显式报错。
    #[test]
    fn comparer_errors_on_new_null() {
        let old = vec![Some(ServerUserItem::csharp_new())];
        let new = vec![None];
        assert!(comparer_user_item(&new, &old).is_err());
    }
}

#[cfg(test)]
mod item_attr_tests {
    //! 构造级验收（G-01；卡 `openmir2-persist-item-attr-and-gold`）：
    //! a) `create_item_attr` 对 `MakeIndex > 0` 的物品按 oracle 门 INSERT，14 列 = `Desc[0..13]`；
    //! b) `None` / `MakeIndex = 0`（建角全空槽）零插入；
    //! c) `query_item_attr` 的行序双索引与 oracle 同（第 n 行 → 槽 n 的 `Desc[n]`）；
    //! d) 存档往返：`save_item_table` 的 `update_item_attr` 更新 14 列，回读按 oracle 映射。
    //!
    //! 跑法（需本机 MySQL；默认 `root` 空口令、库名 `mir2_test_pattr`，测试自建自删）：
    //! `cargo test -p db-srv --bins item_attr_tests -- --ignored --nocapture`

    use super::{
        byte_col, create_empty_items, create_item_attr, get_item_slots, query_item_attr,
        save_item_table,
    };
    use mir2_storage::playerdata::ServerUserItem;
    use mir2_storage::LegacyDsn;
    use sqlx::mysql::MySqlConnection;
    use sqlx::{Connection, Row};

    const DEFAULT_DSN: &str = "server=127.0.0.1;uid=root;pwd=;database=mir2_test_pattr;";

    fn ddl_for(dump: &str, table: &str) -> String {
        let marker = format!("CREATE TABLE `{table}` ");
        let start = dump
            .find(&marker)
            .unwrap_or_else(|| panic!("{table} 建表语句缺失"));
        let rest = &dump[start..];
        let engine = rest.find(") ENGINE").expect("建表语句缺 ENGINE 尾");
        let semi = rest[engine..].find(';').expect("建表语句缺分号");
        rest[..=engine + semi].to_string()
    }

    async fn setup(dsn: &LegacyDsn) -> MySqlConnection {
        let admin = LegacyDsn {
            database: "mysql".to_string(),
            ..dsn.clone()
        };
        let mut conn = MySqlConnection::connect(&admin.to_sqlx_url())
            .await
            .expect("连测试 MySQL 失败（本测试需本机 MySQL）");
        sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", dsn.database))
            .execute(&mut conn)
            .await
            .expect("删测试库失败");
        sqlx::query(&format!(
            "CREATE DATABASE `{}` CHARACTER SET utf8 COLLATE utf8_general_ci",
            dsn.database
        ))
        .execute(&mut conn)
        .await
        .expect("建测试库失败");
        let dump = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../sql/mir2_db.sql"
        ))
        .expect("读 sql/mir2_db.sql 失败");
        let mut conn = MySqlConnection::connect(&dsn.to_sqlx_url())
            .await
            .expect("连测试库失败");
        for table in ["characters_item", "characters_item_attr"] {
            let ddl = ddl_for(&dump, table);
            sqlx::query(&ddl)
                .execute(&mut conn)
                .await
                .unwrap_or_else(|e| panic!("建表 {table} 失败: {e}"));
        }
        conn
    }

    fn item(make_index: i32, desc: [u8; 14]) -> ServerUserItem {
        ServerUserItem {
            make_index,
            index: 7,
            dura: 8,
            dura_max: 9,
            desc: Some(desc.to_vec()),
            ..ServerUserItem::csharp_new()
        }
    }

    fn seq(start: u8) -> [u8; 14] {
        let mut d = [0u8; 14];
        for (i, v) in d.iter_mut().enumerate() {
            *v = start + u8::try_from(i).expect("i < 14");
        }
        d
    }

    async fn attr_rows(conn: &mut MySqlConnection, player_id: i64) -> Vec<Vec<i64>> {
        let rows = sqlx::query("SELECT * FROM characters_item_attr WHERE PlayerId=? ORDER BY Id")
            .bind(player_id)
            .fetch_all(&mut *conn)
            .await
            .expect("读 characters_item_attr 失败");
        rows.iter()
            .map(|r| {
                (0..14)
                    .map(|i| i64::from(byte_col(r, &format!("VALUE{i}")).expect("VALUE 列")))
                    .collect()
            })
            .collect()
    }

    async fn attr_count(conn: &mut MySqlConnection, player_id: i64) -> i64 {
        let row = sqlx::query("SELECT COUNT(*) AS c FROM characters_item_attr WHERE PlayerId=?")
            .bind(player_id)
            .fetch_one(&mut *conn)
            .await
            .expect("计数失败");
        row.try_get::<i64, _>("c").expect("c 列")
    }

    fn desc_values(desc: &[u8; 14]) -> Vec<i64> {
        desc.iter().map(|v| i64::from(*v)).collect()
    }

    #[tokio::test]
    #[ignore = "需要本机 MySQL；MIR2_TEST_DB_DSN 可覆盖，库名必须以 mir2_test_ 开头"]
    #[allow(clippy::too_many_lines)]
    async fn create_gate_query_mapping_and_save_roundtrip_match_oracle() {
        let text = std::env::var("MIR2_TEST_DB_DSN").unwrap_or_else(|_| DEFAULT_DSN.to_string());
        let dsn = LegacyDsn::parse(&text).expect("测试 DSN 非法");
        assert!(
            dsn.database.starts_with("mir2_test_"),
            "拒绝写非测试库：{}",
            dsn.database
        );
        let mut conn = setup(&dsn).await;

        // ---- a) 有 MakeIndex 的物品：INSERT 行存在，14 列逐列等于 Desc ----
        let pid_a = 9001;
        let d_a = seq(10);
        let items_a = vec![None, Some(item(0, seq(90))), Some(item(101, d_a))];
        create_item_attr(&mut conn, pid_a, &items_a)
            .await
            .expect("create_item_attr 失败");
        assert_eq!(
            attr_rows(&mut conn, pid_a).await,
            vec![desc_values(&d_a)],
            "14 列必须等于 Desc[0..13]"
        );

        // ---- b) NULL / MakeIndex=0：与 oracle 的门一致，零插入 ----
        let pid_b = 9002;
        let empty = vec![None, Some(ServerUserItem::csharp_new())];
        create_item_attr(&mut conn, pid_b, &empty)
            .await
            .expect("create_item_attr 失败");
        assert_eq!(
            attr_count(&mut conn, pid_b).await,
            0,
            "空槽/无属性物品不得进表"
        );
        // 建角路径（create_empty_items）同样不得产生 attr 行
        let pid_creation = 9003;
        create_empty_items(&mut conn, pid_creation, "characters_item", 13, false)
            .await
            .expect("create_empty_items 失败");
        assert_eq!(
            attr_count(&mut conn, pid_creation).await,
            0,
            "建角全空槽不得产生 attr 行"
        );

        // ---- c) 行序双索引：第 n 行只写 槽 n 的 Desc[n]（oracle 既有 bug） ----
        let pid_c = 9004;
        let rows_c = vec![
            Some(item(500, seq(10))),
            Some(item(500, seq(30))),
            Some(item(500, seq(50))),
        ];
        create_item_attr(&mut conn, pid_c, &rows_c)
            .await
            .expect("create_item_attr 失败");
        assert_eq!(attr_count(&mut conn, pid_c).await, 3);
        // 回写目标槽位必须从全 0 的 Desc 开始（否则分不清「原始值」与「回写值」）。
        let mut slots: Vec<Option<ServerUserItem>> =
            (0..3).map(|_| Some(item(500, [0u8; 14]))).collect();
        query_item_attr(&mut conn, pid_c, &mut slots)
            .await
            .expect("query_item_attr 失败");
        let desc = |i: usize| slots[i].as_ref().unwrap().desc.clone().unwrap();
        assert_eq!(desc(0)[0], 10, "第 0 行 → 槽 0 的 Desc[0]");
        assert_eq!(desc(1)[1], 31, "第 1 行 → 槽 1 的 Desc[1]");
        assert_eq!(desc(2)[2], 52, "第 2 行 → 槽 2 的 Desc[2]");
        // 证明下标取的是行序而不是「整行 14 列写回同一槽」
        assert_eq!(desc(0)[1], 0, "槽 0 的 Desc[1] 必须保持 0（oracle 不写）");
        assert_eq!(desc(1)[0], 0, "槽 1 的 Desc[0] 必须保持 0（oracle 不写）");
        assert_eq!(desc(2)[13], 0, "槽 2 的 Desc[13] 必须保持 0（oracle 不写）");

        // ---- d) 存档往返：UPDATE 写全部 14 列；回读按 oracle 映射 ----
        let pid_d = 9005;
        let d_saved = seq(40);
        create_empty_items(&mut conn, pid_d, "characters_item", 13, false)
            .await
            .expect("create_empty_items 失败");
        // 先按 oracle 的 INSERT 门造出一行（模拟「已有 attr 行」）
        create_item_attr(&mut conn, pid_d, &[Some(item(101, seq(20)))])
            .await
            .expect("create_item_attr 失败");
        // 13 槽全部 Some（C# HumItems 从库里读出来就是 13 个 Some）；槽 0 换成带属性的物品
        let mut new_items: Vec<Option<ServerUserItem>> = (0..13)
            .map(|_| Some(ServerUserItem::csharp_new()))
            .collect();
        new_items[0] = Some(item(101, d_saved));
        save_item_table(&mut conn, pid_d, "characters_item", &new_items, 13)
            .await
            .expect("save_item_table 失败");
        assert_eq!(
            attr_rows(&mut conn, pid_d).await,
            vec![desc_values(&d_saved)],
            "存档必须 UPDATE 全部 14 列"
        );
        let loaded = get_item_slots(&mut conn, pid_d, "characters_item", 13).await;
        assert_eq!(loaded[0].as_ref().unwrap().make_index, 101);
        let ld = loaded[0].as_ref().unwrap().desc.clone().unwrap();
        assert_eq!(ld[0], 40, "oracle 只把 VALUE0 写回 Desc[0]");
        assert!(
            ld[1..].iter().all(|v| *v == 0),
            "oracle 的行序双索引不写 Desc[1..]: {ld:?}"
        );

        sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", dsn.database))
            .execute(&mut conn)
            .await
            .expect("清理测试库失败");
    }
}

#[cfg(test)]
mod single_sided_column_tests {
    //! **卡 `openmir2-persist-single-sided-columns`（R）**：逐字段收口 oracle 的「单边」列
    //! （读↔写不对称）与 Rust-only 缺口（G-09/G-10/G-19 的库侧往返）。
    //!
    //! - `AllowGroup`：oracle UPDATE 清单没有该列（只读不写）⇒ 存档后列值必须**保持原值**；
    //! - `AllowGroupReCall`：oracle 把 `hd.AllowGroup` 绑到它（列映射错位，照抄）；
    //! - `PkPoint`/`MarryCount`：oracle 写、**载入不读** ⇒ 落库值正确但 `get_record` 读出 0；
    //! - `ServerIndex`/`Dir`/`IncHealing`：本批补的 Rust-only 落库字段（仍照 oracle 载入口径，
    //!   `ServerIndex` 读回 0）；
    //! - `characters_ablity`：`Ac = Level`、`Hp/Mp` 只写低字（不写 AC/MAC 高字）——照抄。
    //!
    //! 跑法（需本机 MySQL；库名 `mir2_test_single_sided`，自建自删）：
    //! `cargo test -p db-srv --bins single_sided_column_tests -- --ignored --nocapture`

    use super::{create_ablity_status, create_characters, get_record, save_record};
    use mir2_storage::playerdata::{Ability, CharacterDataInfo};
    use mir2_storage::LegacyDsn;
    use sqlx::mysql::MySqlConnection;
    use sqlx::Connection;

    const TABLES: [&str; 9] = [
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

    fn ddl_for(dump: &str, table: &str) -> String {
        let marker = format!("CREATE TABLE `{table}` ");
        let start = dump
            .find(&marker)
            .unwrap_or_else(|| panic!("{table} 建表语句缺失"));
        let rest = &dump[start..];
        let engine = rest.find(") ENGINE").expect("建表语句缺 ENGINE 尾");
        let semi = rest[engine..].find(';').expect("建表语句缺分号");
        rest[..=engine + semi].to_string()
    }

    async fn setup(database: &str) -> (LegacyDsn, MySqlConnection) {
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
        let mut conn = MySqlConnection::connect(&admin.to_sqlx_url())
            .await
            .expect("连测试 MySQL 失败（本测试需本机 MySQL）");
        sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", dsn.database))
            .execute(&mut conn)
            .await
            .expect("删测试库失败");
        sqlx::query(&format!(
            "CREATE DATABASE `{}` CHARACTER SET utf8 COLLATE utf8_general_ci",
            dsn.database
        ))
        .execute(&mut conn)
        .await
        .expect("建测试库失败");
        let dump = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../sql/mir2_db.sql"
        ))
        .expect("读 sql/mir2_db.sql 失败");
        let mut conn = MySqlConnection::connect(&dsn.to_sqlx_url())
            .await
            .expect("连测试库失败");
        for table in TABLES {
            let ddl = ddl_for(&dump, table);
            sqlx::query(&ddl)
                .execute(&mut conn)
                .await
                .unwrap_or_else(|e| panic!("建表 {table} 失败: {e}"));
        }
        (dsn, conn)
    }

    fn info() -> CharacterDataInfo {
        let mut v = CharacterDataInfo::csharp_new();
        v.header.set_name(Some("sidhero".to_string()));
        let d = v.data.as_mut().expect("csharp_new 的 Data 非空");
        d.chr_name = Some("sidhero".to_string());
        d.account = Some("sid_acct".to_string());
        d.abil = Some(Ability {
            level: 1,
            hp: 10,
            ..Ability::default()
        });
        v
    }

    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_single_sided（自建自删）"]
    #[allow(clippy::too_many_lines)]
    async fn single_sided_columns_and_rust_only_gaps_roundtrip() {
        let (dsn, mut conn) = setup("mir2_test_single_sided").await;
        let mut info = info();
        let id = create_characters(&mut conn, &info)
            .await
            .expect("create_characters 失败");
        assert!(id > 0);
        // 正常建角路径（`db.rs::data_add`）在 `create_character` 之后写 ablity/status；
        // 这里直接调 `create_characters`，补同一条 INSERT（否则 `save_ablity` 是 0 行 UPDATE）。
        create_ablity_status(&mut conn, id, info.data.as_ref().expect("Data 非空"))
            .await
            .expect("create_ablity_status 失败");

        // 建角 INSERT 不含 `AllowGroup`（oracle `CreateCharacters` 同）⇒ 手工置 1，用于验证
        // 「存档 UPDATE 永不更新该列」。
        sqlx::query("UPDATE characters SET AllowGroup=1 WHERE Id=?")
            .bind(id)
            .execute(&mut conn)
            .await
            .expect("置 AllowGroup=1 失败");

        // ---- 存档：与建角值全部不同 ----
        {
            let d = info.data.as_mut().unwrap();
            d.allow_group = 0; // 若 UPDATE 含该列，落库会变 0
            d.allow_group_re_call = true; // oracle 错绑 ⇒ 列值应当 = allow_group = 0
            d.pk_point = 77;
            d.marry_count = 3;
            d.dir = 5;
            d.server_index = 4;
            d.inc_healing = 44;
            d.abil = Some(Ability {
                level: 7,
                ac: 99, // 必须被忽略：`Ac` 列绑的是 Level
                mac: 3,
                dc: 4,
                mc: 5,
                sc: 6,
                hp: 0x1234, // 只写低 16 位（AC 高字不写）
                mp: 0x5678,
                max_hp: 9,
                max_mp: 10,
                exp: 11,
                max_exp: 12,
                weight: 13,
                max_weight: 14,
                wear_weight: 15,
                max_wear_weight: 16,
                hand_weight: 17,
                max_hand_weight: 18,
                ..Ability::default()
            });
        }
        save_record(&mut conn, id, &info)
            .await
            .expect("save_record 失败");

        // ---- characters 列 ----
        let row = sqlx::query(
            "SELECT ServerIndex, Dir, IncHealing, PkPoint, MarryCount, AllowGroup, AllowGroupReCall \
             FROM characters WHERE Id=?",
        )
        .bind(id)
        .fetch_one(&mut conn)
        .await
        .expect("读 characters 失败");
        let col = |n: &str| -> i64 { super::int_col(&row, n).expect(n) };
        assert_eq!(col("ServerIndex"), 4, "G-10：ServerIndex 必须落库");
        assert_eq!(col("Dir"), 5, "G-09：Dir 必须落当前朝向");
        assert_eq!(col("IncHealing"), 44, "G-19：IncHealing 必须落库");
        assert_eq!(col("PkPoint"), 77, "PkPoint 写侧保留（载入不读）");
        assert_eq!(col("MarryCount"), 3, "MarryCount 写侧保留（载入不读）");
        assert_eq!(
            col("AllowGroup"),
            1,
            "oracle UPDATE 清单没有 AllowGroup ⇒ 必须保持原值（单边：只读不写）"
        );
        assert_eq!(
            col("AllowGroupReCall"),
            0,
            "oracle 把 hd.AllowGroup 绑到 @AllowGroupReCall ⇒ 列 = 0（不是 true）"
        );

        // ---- characters_ablity：Ac = Level、Hp/Mp 只写低字 ----
        let arow = sqlx::query(
            "SELECT Level, Ac, Mac, Dc, Mc, Sc, Hp, Mp, MaxHP, MAxMP, Exp, MaxExp \
             FROM characters_ablity WHERE PlayerId=?",
        )
        .bind(id)
        .fetch_one(&mut conn)
        .await
        .expect("读 characters_ablity 失败");
        let acol = |n: &str| -> i64 { super::int_col(&arow, n).expect(n) };
        assert_eq!(acol("Level"), 7);
        assert_eq!(
            acol("Ac"),
            7,
            "oracle 既有 bug：Ac 绑 Abil.Level（不是 AC=99）"
        );
        assert_eq!(acol("Hp"), 0x1234, "Hp 只写 Abil.HP（低 16 位）");
        assert_eq!(acol("Mp"), 0x5678, "Mp 只写 Abil.MP（低 16 位）");
        assert_eq!(acol("Mac"), 3);
        assert_eq!(acol("Dc"), 4);
        assert_eq!(acol("MaxHP"), 9);
        assert_eq!(acol("MAxMP"), 10);

        // ---- 载入：oracle 不读 PkPoint/MarryCount/ServerIndex；其余逐字段回来 ----
        let loaded = get_record(&mut conn, id)
            .await
            .expect("get_record 失败")
            .expect("记录应存在")
            .data
            .expect("Data 非空");
        assert_eq!(loaded.pk_point, 0, "PkPoint 只写不读（oracle 单边）");
        assert_eq!(loaded.marry_count, 0, "MarryCount 只写不读（oracle 单边）");
        assert_eq!(loaded.server_index, 0, "ServerIndex 载入不读（oracle 同）");
        assert_eq!(loaded.dir, 5, "Dir 载入读取");
        assert_eq!(loaded.inc_healing, 44, "IncHealing 载入读取");
        assert_eq!(loaded.allow_group, 1, "AllowGroup 载入读取");
        assert!(!loaded.allow_group_re_call, "列值为 0 ⇒ false（错绑照抄）");
        let abil = loaded.abil.expect("Abil 非空");
        assert_eq!(abil.level, 7);
        assert_eq!(abil.hp, 0x1234, "HP 低字往返");
        assert_eq!(abil.ac, 0, "AC 高字从未写入 ⇒ 重登 0（oracle 同）");

        sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", dsn.database))
            .execute(&mut conn)
            .await
            .expect("清理测试库失败");
    }

    /// **G7**：物品表**读失败不整表放弃**——oracle `GetItemRecord` 一类 `try/catch` 吞错后
    /// `oldItems` 为空/部分，仍继续 `ComparerUserItem` 与写循环；旧 Rust 实现 `fetch_all(...)?`
    /// 直接把整张表交给 `save_record` 记日志跳过。
    ///
    /// 失败注入：删掉 `characters_bagitem` 表 ⇒ `get_item_slots` 必须返回 46 个空槽（**不 panic、
    /// 不 Err**），`save_item_table` 必须继续走到差量逻辑并正常返回。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_item_readfail（自建自删）"]
    async fn item_read_failure_keeps_table_in_play() {
        use super::{get_item_slots, save_item_table};
        use mir2_storage::playerdata::ServerUserItem;
        let (dsn, mut conn) = setup("mir2_test_item_readfail").await;
        sqlx::query("DROP TABLE characters_bagitem")
            .execute(&mut conn)
            .await
            .expect("删 characters_bagitem 失败（失败注入）");
        let slots = get_item_slots(&mut conn, 1, "characters_bagitem", 46).await;
        assert_eq!(slots.len(), 46, "槽位数不因读失败而变");
        assert!(
            slots.iter().all(Option::is_none),
            "读失败 ⇒ 已读行保留（此例 0 行）⇒ 全空，且不返回 Err"
        );
        let new_items: Vec<Option<ServerUserItem>> = (0..46)
            .map(|_| Some(ServerUserItem::csharp_new()))
            .collect();
        save_item_table(&mut conn, 1, "characters_bagitem", &new_items, 46)
            .await
            .expect("读失败后 save_item_table 不得整表放弃");
        sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", dsn.database))
            .execute(&mut conn)
            .await
            .expect("清理测试库失败");
    }
}

#[cfg(test)]
mod partial_save_tests {
    //! 存档体缺字段不得写 0 覆盖（LB-06 重写，按评审更正节的 oracle 口径）：
    //!
    //! | 缺失字段 | oracle 实际行为（本测试钉住） |
    //! |---|---|
    //! | `Abil` | `SaveRecord(context,…,hd)` 的 `hd.Abil.Level` 在**内层 try 之外** NRE ⇒ 外层 catch ⇒ `RollBack()`，**8 张表一行不改**、`Update` 返回 false |
    //! | `BonusAbil` | `SaveBonusability` 自己的 try 内 NRE 被 catch ⇒ 只跳过 `characters_bonusability` |
    //! | `StatusTimeArr` | `SaveStatus` 自己的 try 内 NRE/越界被 catch ⇒ 只跳过 `characters_status` |
    //! | 物品 `Desc` | `UpdateItemAttr` 先拼**整批** SQL 再执行 ⇒ 拼串时 NRE 被 catch ⇒ 该批 `characters_item_attr` 一行不改 |
    //!
    //! 旧实现四处 `unwrap_or_default()` / 补 0 ⇒ 把等级/HP/AC/状态位/物品属性全部清零。
    //!
    //! 跑法（需本机 MySQL；各用例独立 `mir2_test_*` 库，自建自删）：
    //! `cargo test -p db-srv --bins partial_save_tests -- --ignored --nocapture`

    use super::save_record;
    use crate::testsupport as ts;
    use mir2_storage::playerdata::{Ability, CharacterDataInfo, NakedAbility, ServerUserItem};
    use mir2_storage::LegacyDsn;
    use sqlx::Row;

    fn info(name: &str) -> CharacterDataInfo {
        let mut v = CharacterDataInfo::csharp_new();
        v.header.set_name(Some(name.to_string()));
        let d = v.data.as_mut().expect("CharacterData 默认非空");
        d.chr_name = Some(name.to_string());
        d.gold = 999;
        d.abil = Some(Ability {
            level: 5,
            hp: 55,
            ..Ability::default()
        });
        d.bonus_abil = Some(NakedAbility {
            ac: 9,
            ..NakedAbility::default()
        });
        d.status_time_arr = Some(vec![444; 15]);
        v
    }

    async fn one(dsn: &LegacyDsn, sql: &str, id: i64) -> i64 {
        let mut c = ts::conn(dsn).await;
        let row = sqlx::query(sql)
            .bind(id)
            .fetch_one(&mut c)
            .await
            .unwrap_or_else(|e| panic!("查询失败 ({sql}): {e}"));
        // 列类型跨 int/bigint/tinyint(1)（后者 sqlx 视作 BOOLEAN），逐档取。
        row.try_get::<i64, _>("v")
            .or_else(|_| row.try_get::<i32, _>("v").map(i64::from))
            .or_else(|_| row.try_get::<i16, _>("v").map(i64::from))
            .or_else(|_| row.try_get::<u16, _>("v").map(i64::from))
            .or_else(|_| row.try_get::<i8, _>("v").map(i64::from))
            .or_else(|_| row.try_get::<u8, _>("v").map(i64::from))
            .or_else(|_| row.try_get::<bool, _>("v").map(i64::from))
            .unwrap_or_else(|e| panic!("v 列解码失败 ({sql}): {e}"))
    }

    /// 种一份带区分度的存档（characters.Gold=100 / ablity.Level=7 / status.Status0=111 /
    /// bonusability.AC=1），返回 `characters.Id`。
    async fn seed(dsn: &LegacyDsn, name: &str) -> i64 {
        let id = ts::insert_character(dsn, "acct", name).await;
        let mut c = ts::conn(dsn).await;
        sqlx::query("UPDATE characters SET Gold=100, Level=1 WHERE Id=?")
            .bind(id)
            .execute(&mut c)
            .await
            .expect("seed characters 失败");
        sqlx::query(
            "INSERT INTO characters_ablity (PlayerId, Level, Ac, Mac, Dc, Mc, Sc, Hp, Mp, MaxHP, MAxMP, \
             Exp, MaxExp, Weight, MaxWeight, WearWeight, MaxWearWeight, HandWeight, MaxHandWeight) \
             VALUES (?, 7, 41, 42, 1, 2, 3, 7, 8, 101, 102, 100, 1000, 10, 100, 11, 110, 12, 120)",
        )
        .bind(id)
        .execute(&mut c)
        .await
        .expect("seed characters_ablity 失败");
        sqlx::query(
            "INSERT INTO characters_status (PlayerId, Status0, Status1, Status2, Status3, Status4, \
             Status5, Status6, Status7, Status8, Status9, Status10, Status11, Status12, Status13, \
             Status14, Status15) VALUES (?, 111, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 222, 0)",
        )
        .bind(id)
        .execute(&mut c)
        .await
        .expect("seed characters_status 失败");
        sqlx::query(
            "INSERT INTO characters_bonusability (PLAYERID, AC, MAC, DC, MC, SC, HP, MP, HIT, SPEED, RESERVED) \
             VALUES (?, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10)",
        )
        .bind(id)
        .execute(&mut c)
        .await
        .expect("seed characters_bonusability 失败");
        id
    }

    /// LB-06(a)：`Abil` 缺失 ⇒ oracle 整次存档回滚（不是只跳过 ablitiy 表）。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_psave_abil（自建自删）"]
    async fn save_missing_abil_rolls_back_everything() {
        let dsn = ts::setup_db("mir2_test_psave_abil").await;
        let name = "psabilhero";
        let id = seed(&dsn, name).await;
        let mut rcd = info(name);
        rcd.data.as_mut().expect("data").abil = None;
        let mut c = ts::conn(&dsn).await;
        let ok = save_record(&mut c, id, &rcd)
            .await
            .expect("save_record 不得报错（oracle 外层 catch 吞掉 NRE）");
        assert!(!ok, "Abil 缺失 ⇒ oracle 外层 catch ⇒ Update 返回 false");
        assert_eq!(
            one(&dsn, "SELECT Gold AS v FROM characters WHERE Id=?", id).await,
            100,
            "characters 也不得更新（整次回滚）"
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT Level AS v FROM characters_ablity WHERE PlayerId=?",
                id
            )
            .await,
            7,
            "characters_ablity 保持库内旧值"
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT Status0 AS v FROM characters_status WHERE PlayerId=?",
                id
            )
            .await,
            111,
            "characters_status 保持库内旧值"
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT AC AS v FROM characters_bonusability WHERE PlayerId=?",
                id
            )
            .await,
            1,
            "characters_bonusability 保持库内旧值"
        );
        ts::drop_db(&dsn).await;
    }

    /// LB-06(b)：`BonusAbil` 缺失 ⇒ 只跳过 `characters_bonusability`，其余照常落库。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_psave_bonus（自建自删）"]
    async fn save_missing_bonus_abil_keeps_bonusability_table() {
        let dsn = ts::setup_db("mir2_test_psave_bonus").await;
        let name = "psbonushero";
        let id = seed(&dsn, name).await;
        let mut rcd = info(name);
        rcd.data.as_mut().expect("data").bonus_abil = None;
        let mut c = ts::conn(&dsn).await;
        let ok = save_record(&mut c, id, &rcd).await.expect("保存失败");
        assert!(ok, "BonusAbil 缺失被 catch ⇒ 其余表照常提交");
        assert_eq!(
            one(&dsn, "SELECT Gold AS v FROM characters WHERE Id=?", id).await,
            999
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT Level AS v FROM characters_ablity WHERE PlayerId=?",
                id
            )
            .await,
            5
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT AC AS v FROM characters_bonusability WHERE PlayerId=?",
                id
            )
            .await,
            1,
            "characters_bonusability 一行不改（不得写 0）"
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT Status0 AS v FROM characters_status WHERE PlayerId=?",
                id
            )
            .await,
            444,
            "其余表照常更新"
        );
        ts::drop_db(&dsn).await;
    }

    /// LB-06(c)：`StatusTimeArr` 缺失 ⇒ 只跳过 `characters_status`。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_psave_status（自建自删）"]
    async fn save_missing_status_keeps_status_table() {
        let dsn = ts::setup_db("mir2_test_psave_status").await;
        let name = "psstatushero";
        let id = seed(&dsn, name).await;
        let mut rcd = info(name);
        rcd.data.as_mut().expect("data").status_time_arr = None;
        let mut c = ts::conn(&dsn).await;
        let ok = save_record(&mut c, id, &rcd).await.expect("保存失败");
        assert!(ok);
        assert_eq!(
            one(&dsn, "SELECT Gold AS v FROM characters WHERE Id=?", id).await,
            999
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT Status0 AS v FROM characters_status WHERE PlayerId=?",
                id
            )
            .await,
            111,
            "characters_status 一行不改（不得把 15 个状态位写 0）"
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT Status14 AS v FROM characters_status WHERE PlayerId=?",
                id
            )
            .await,
            222
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT AC AS v FROM characters_bonusability WHERE PlayerId=?",
                id
            )
            .await,
            9,
            "其余表照常更新"
        );
        ts::drop_db(&dsn).await;
    }

    /// LB-06(d)：物品 `Desc` 缺失 ⇒ `characters_item_attr` 整批一行不改（不是补 0）。
    #[tokio::test]
    #[ignore = "需要本机 MySQL；库名 mir2_test_psave_desc（自建自删）"]
    async fn save_missing_item_desc_keeps_item_attr_table() {
        let dsn = ts::setup_db("mir2_test_psave_desc").await;
        let name = "psdeschero";
        let id = seed(&dsn, name).await;
        {
            let mut c = ts::conn(&dsn).await;
            // 旧物品：槽 0 穿戴 MakeIndex=111；槽 1 空。
            sqlx::query(
                "INSERT INTO characters_item (PlayerId, Position, MakeIndex, StdIndex, Dura, DuraMax) \
                 VALUES (?, 0, 111, 45, 0, 0), (?, 1, 0, 0, 0, 0)",
            )
            .bind(id)
            .bind(id)
            .execute(&mut c)
            .await
            .expect("seed characters_item 失败");
            // attr 行：目标 MakeIndex=123 的 VALUE0..13 种成非零。
            sqlx::query(
                "INSERT INTO characters_item_attr (PlayerId, MakeIndex, VALUE0, VALUE1, VALUE2, VALUE3, \
                 VALUE4, VALUE5, VALUE6, VALUE7, VALUE8, VALUE9, VALUE10, VALUE11, VALUE12, VALUE13) \
                 VALUES (?, 123, 77, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13)",
            )
            .bind(id)
            .execute(&mut c)
            .await
            .expect("seed characters_item_attr 失败");
        }
        let mut rcd = info(name);
        {
            let d = rcd.data.as_mut().expect("data");
            let mut hum = vec![None; 13];
            hum[0] = Some(ServerUserItem::csharp_new()); // 旧 MakeIndex>0、新 0 ⇒ del
            let mut it = ServerUserItem::csharp_new();
            it.make_index = 123;
            it.index = 45;
            it.desc = None; // 缺失 Desc
            hum[1] = Some(it); // 空槽 → 有物 ⇒ chg
            d.hum_items = Some(hum);
        }
        let mut c = ts::conn(&dsn).await;
        let ok = save_record(&mut c, id, &rcd).await.expect("保存失败");
        assert!(ok, "UpdateItemAttr 的 NRE 被 catch，不影响提交");
        assert_eq!(
            one(
                &dsn,
                "SELECT VALUE0 AS v FROM characters_item_attr WHERE PlayerId=? AND MakeIndex=123",
                id
            )
            .await,
            77,
            "characters_item_attr 整批一行不改（旧实现会写 0）"
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT MakeIndex AS v FROM characters_item WHERE PlayerId=? AND Position=1",
                id
            )
            .await,
            123,
            "槽位表的 UPDATE 循环照常执行"
        );
        assert_eq!(
            one(
                &dsn,
                "SELECT MakeIndex AS v FROM characters_item WHERE PlayerId=? AND Position=0",
                id
            )
            .await,
            0,
            "del 分支照常清空槽 0"
        );
        ts::drop_db(&dsn).await;
    }
}
