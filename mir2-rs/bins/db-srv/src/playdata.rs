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
//! - `Add` 的 `CreateCharacters` 与 5100 面的建角不同：它把完整 `CharacterData` 字段写进主表。
//!
//! 一处必要差异：C# `dr.GetByte/GetInt16/GetUInt16` 在列值越界时抛异常（被上层 catch）；
//! 这里同样用 `try_from` 报错并让调用方按「取档失败」处理，不静默截断。

use anyhow::{Context, Result};
use mir2_storage::playerdata::{
    Ability, CharacterData, CharacterDataInfo, MagicRcd, NakedAbility, ServerUserItem,
};
use sqlx::mysql::MySqlRow;
use sqlx::{Connection, MySqlConnection, Row};
use std::fmt::Write as _;

use crate::db::{int_col, resolve_col, Db};
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
) -> Result<Vec<Option<ServerUserItem>>> {
    let sql = format!("SELECT * FROM {table} WHERE PlayerId=?");
    let rows = sqlx::query(&sql)
        .bind(player_id)
        .fetch_all(&mut *conn)
        .await
        .with_context(|| format!("查询 {table} 失败"))?;
    let mut items: Vec<Option<ServerUserItem>> = vec![None; slots];
    for row in &rows {
        let pos = i32::try_from(int_col(row, "Position")?).context("Position 越界")?;
        let Ok(idx) = usize::try_from(pos) else {
            log::error(&format!("[Exception] {table} Position={pos} 为负"));
            break;
        };
        let Some(slot) = items.get_mut(idx) else {
            // C# `array[pos]` 越界 → 抛异常（被 catch，已填部分保留，且不再走 QueryItemAttr）。
            log::error(&format!(
                "[Exception] {table} Position={pos} 超出 {slots} 槽（C# 同场景抛异常）"
            ));
            return Ok(items);
        };
        let mut item = ServerUserItem::csharp_new();
        item.make_index = i32::try_from(int_col(row, "MakeIndex")?).context("MakeIndex 越界")?;
        item.index = word_col(row, "StdIndex")?;
        item.dura = word_col(row, "Dura")?;
        item.dura_max = word_col(row, "DuraMax")?;
        *slot = Some(item);
    }
    query_item_attr(conn, player_id, &mut items).await?;
    Ok(items)
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
    data.hum_items =
        Some(get_item_slots(conn, player_id, "characters_item", HUM_ITEM_SLOTS).await?);
    data.bag_items =
        Some(get_item_slots(conn, player_id, "characters_bagitem", BAG_ITEM_SLOTS).await?);
    data.storage_items = Some(
        get_item_slots(
            conn,
            player_id,
            "characters_storageitem",
            STORAGE_ITEM_SLOTS,
        )
        .await?,
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
) -> Result<()> {
    let abil = d.abil.clone().unwrap_or_default();
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
    for item in items.iter().flatten() {
        let desc = item.desc.clone().unwrap_or_else(|| vec![0; 14]);
        let v: Vec<u8> = (0..14).map(|i| desc.get(i).copied().unwrap_or(0)).collect();
        let sql = format!(
            "UPDATE characters_item_attr SET VALUE0={},VALUE1={},VALUE2={},VALUE3={},VALUE4={},VALUE5={},VALUE6={},VALUE7={},VALUE8={},VALUE9={},VALUE10={},VALUE11={},VALUE12={},VALUE13={} WHERE PlayerId={player_id} AND MakeIndex={};",
            v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11], v[12], v[13],
            item.make_index
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
    let old_items = get_item_slots(conn, player_id, table, slots).await?;
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
async fn save_record(
    conn: &mut MySqlConnection,
    player_id: i64,
    info: &CharacterDataInfo,
) -> Result<bool> {
    let Some(d) = info.data.as_ref() else {
        return Ok(false);
    };
    let mut tx = conn.begin().await.context("开启存档事务失败")?;
    if let Err(e) = save_characters(&mut tx, player_id, d).await {
        log::error(&format!("[Exception] PlayDataStorage.UpdateRecord: {e:#}"));
    }
    if let Err(e) = save_ablity(&mut tx, player_id, &d.abil.clone().unwrap_or_default()).await {
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
    if let Err(e) = save_bonusability(
        &mut tx,
        player_id,
        &d.bonus_abil.clone().unwrap_or_default(),
    )
    .await
    {
        log::error(&format!(
            "[Exception] PlayDataStorage.SaveBonusability: {e:#}"
        ));
    }
    if let Err(e) = save_status(
        &mut tx,
        player_id,
        d.status_time_arr.as_deref().unwrap_or(&[]),
    )
    .await
    {
        log::error(&format!("[Exception] PlayDataStorage.SaveStatus: {e:#}"));
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
    if let Err(e) = save_ablity(conn, player_id, &d.abil.clone().unwrap_or_default()).await {
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

/// C# `CreateUseItem`/`CreateBagItem`/`CreateStorageItem` 在「新角色」下的等价物：
/// 新角色这些表还没有行 ⇒ `oldItems` 全空 ⇒ 每个槽都插一行全 0；`CreateItemAttr` 因 `MakeIndex=0` 无行。
async fn create_empty_items(
    conn: &mut MySqlConnection,
    player_id: i64,
    table: &str,
    count: i32,
) -> Result<()> {
    let sql = format!(
        "INSERT INTO {table} (PlayerId, Position, MakeIndex, StdIndex, Dura, DuraMax) VALUES (?, ?, 0, 0, 0, 0);"
    );
    for pos in 0..count {
        if let Err(e) = sqlx::query(&sql)
            .bind(player_id)
            .bind(pos)
            .execute(&mut *conn)
            .await
        {
            log::error(&format!(
                "[Exception] PlayDataStorage.CreateItem({table}): {e:#}"
            ));
            break;
        }
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
        let Some(d) = info.data.as_ref() else {
            return Ok(false);
        };
        let mut tx = self.pool.begin().await.context("开启建角事务失败")?;
        let player_id = create_characters(&mut tx, info).await?;
        if player_id <= 0 {
            tx.rollback().await.ok();
            return Ok(false);
        }
        create_ablity_status(&mut tx, player_id, d).await?;
        create_empty_items(&mut tx, player_id, "characters_item", 13).await?;
        create_empty_items(&mut tx, player_id, "characters_bagitem", 46).await?;
        create_empty_items(&mut tx, player_id, "characters_storageitem", 50).await?;
        tx.commit().await.context("提交建角事务失败")?;
        self.name_text.insert(name.to_lowercase(), player_id);
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
