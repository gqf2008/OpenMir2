//! `mir2_db` 的角色存储（对应 C# `Storeages/DBSvr.Storage.MySQL` 的
//! `PlayRecordStorage` + `PlayDataStorage` 中 5100 面用到的部分）。
//!
//! 两张表、两套编号，**不可混用**（现网实测：`characters_indexes.Id=6229` 对应的
//! `characters.Id=6226`）：
//! - `characters_indexes`（C# `PlayRecordStorage`）：账号 → 角色名/索引，表里还带
//!   `SelectID`（"最近选择"标记）与 `IsDeleted`（0 正常 / 1 删除 / 2 创建异常）；
//! - `characters`（C# `PlayDataStorage`）：角色存档本体；查询角色列表时只读
//!   `ChrName/Job/Hair/Sex/Level`，选角时还要 `MapName`。
//!
//! **启动时一次性载入内存索引**（名字 → Id、账号 → 角色列表），此后查询只走内存 —— 这是
//! C# 的既有行为：SQL 直开的新号/新角色要**重启 DBSrv** 才可见（同 `AccountStore` 的口径）。
//!
//! 名字一律带两种表示：**GBK 字节**（线上形态，用于组包与查表键）与**文本**（日志/比较用）。
//! 查表键统一做 ASCII 折叠（等价 C# `StringComparer.OrdinalIgnoreCase`，见 `gbk::fold_key`）。
//! 取角色名的 GBK 字节走服务端转换 `CAST(CONVERT(ChrName USING gbk) AS BINARY)`，
//! 即让 MySQL 做 utf8mb3→gbk 的编码（等价 .NET `Encoding.GetEncoding("gb2312").GetBytes`），
//! 因此不需要在 Rust 侧内置 GBK 码表。
//!
//! 一处授权偏离（登记 `tests/parity/whitelist.md` A-10）：C# 连不上 MySQL / 表缺列时
//! **只打日志、带空索引继续跑**（之后所有角色操作都失败）；本实现按 M1 验收④
//! **返回错误、启动失败**。

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context, Result};
use sqlx::mysql::{MySqlPool, MySqlPoolOptions, MySqlRow};
use sqlx::{Column, MySql, Row, Transaction};

use crate::gbk::fold_key;
use mir2_storage::LegacyDsn;

/// 角色索引条目（C# `PlayQuick`）。
#[derive(Debug, Clone)]
pub struct Quick {
    /// `characters_indexes.Id`。
    pub index: i64,
    /// 角色名的 GBK 字节（线上形态）。
    pub name_gbk: Vec<u8>,
    /// 角色名的文本视图（日志用）。
    pub name_text: String,
}

// 注：C# `PlayQuick.SelectID` 在 DBSrv 里**只写不读**（`LoadQuickList` 恒写 0，
// `AddRecord` 写 `Header.SelectID`，而 `FindByAccount`/`ChrCountOfAccount` 只用 `Index`/`ChrName`），
// 故 Rust 镜像不保留该字段（保留即为死字段）。

/// `characters_indexes` 一行（C# `PlayerRecordData`）。
#[derive(Debug, Clone)]
pub struct IndexRow {
    /// `characters_indexes.Id`。
    pub id: i64,
    /// `Account` 文本。
    pub account_text: String,
    /// `ChrName` 的 GBK 字节。
    pub name_gbk: Vec<u8>,
    /// `SelectID`（C# 侧 `(byte)dr.GetInt32(...)` 截断到 8 位）。
    pub selected: u8,
    /// `IsDeleted != 0`。
    pub deleted: bool,
}

/// 选角列表里一个角色要用的字段（C# `QueryChr`）。
#[derive(Debug, Clone)]
pub struct QryChar {
    /// `Job`。
    pub job: u8,
    /// `Hair`。
    pub hair: u8,
    /// `Sex`。
    pub sex: u8,
    /// `Level`（C# `dr.GetUInt16("Level")`，列是 int，按 16 位取）。
    pub level: u16,
}

/// 角色存储（两张表共用一个池 + 三份内存索引）。
pub struct Db {
    /// 连接池（`playdata` 模块的 6000 面直接复用）。
    pub(crate) pool: MySqlPool,
    /// `characters_indexes`：名字（折叠）→ Id，只含未删除行。
    quick: HashMap<Vec<u8>, i64>,
    /// `characters_indexes`：账号（折叠）→ 角色列表（按读库顺序）。
    by_account: HashMap<Vec<u8>, Vec<Quick>>,
    /// `characters_indexes`：未删除行的 Id 集合（C# `_indexQuickList`，`Update` 的存在性守卫）。
    index_ids: HashSet<i64>,
    /// `characters_indexes`：已删除的 Id（C# `_deletedList`，仅记录，建角不复用）。
    deleted_ids: Vec<i64>,
    /// `characters`：角色 Id 集合（C# `_IndexQuickIdMap`，键值同为 Id）。
    pub(crate) char_ids: HashSet<i64>,
    /// `characters`：名字（折叠）→ `characters.Id`。
    pub(crate) name_quick: HashMap<Vec<u8>, i64>,
    /// `characters`：名字（Unicode 小写折叠）→ `characters.Id`。
    ///
    /// 6000 `DataService` 收的是 MemoryPack 字符串（UTF-16），C# `PlayDataStorage._NameQuickMap`
    /// 就是 `StringComparer.OrdinalIgnoreCase` 的字符串键；这里镜像一份 Unicode 键表供 6000 面使用，
    /// 5100 面的 GBK 字节键表（[`Db::data_index`]）保持不变。
    pub(crate) name_text: HashMap<String, i64>,
}

/// 列名**大小写不敏感**地解析（C# MySqlConnector 的 `dr.GetXxx("HP")` 对 `Hp` 列也能取到）。
/// sqlx 按名字取列是大小写敏感的，这里统一先解析真实列名。
pub(crate) fn resolve_col<'a>(row: &'a MySqlRow, name: &str) -> Result<&'a str> {
    row.columns()
        .iter()
        .find(|c| c.name().eq_ignore_ascii_case(name))
        .map(Column::name)
        .ok_or_else(|| anyhow::anyhow!("列 {name} 不存在（表结构与 C# 预期不符）"))
}

/// 整数列取值：**按 sqlx 的类型阶梯逐个尝试**（`tinyint→i8`、`smallint→i16`、
/// `int→i32`、`bigint→i64` 各自映射不同 Rust 类型），全失败即报错 —— 不静默取默认值。
pub(crate) fn int_col(row: &MySqlRow, name: &str) -> Result<i64> {
    let name = resolve_col(row, name)?;
    macro_rules! try_ty {
        ($($t:ty),*) => {$(
            if let Ok(v) = row.try_get::<$t, _>(name) {
                return Ok(i64::from(v));
            }
        )*};
    }
    try_ty!(i8, u8, i16, u16, i32, u32, i64);
    bail!("列 {name} 不是可识别的整数类型（表结构与 C# 预期不符）")
}

pub(crate) fn text_col(row: &MySqlRow, name: &str) -> Result<String> {
    let name = resolve_col(row, name)?;
    row.try_get::<String, _>(name)
        .with_context(|| format!("列 {name} 不是文本类型（表结构与 C# 预期不符）"))
}

fn gbk_col(row: &MySqlRow, name: &str) -> Result<Vec<u8>> {
    let name = resolve_col(row, name)?;
    row.try_get::<Vec<u8>, _>(name)
        .with_context(|| format!("列 {name} 取不到字节（表结构与 C# 预期不符）"))
}

/// `characters_indexes` 查询列：`ChrName` 同时取文本与 GBK 字节。
const INDEX_COLS: &str = "Id, Account, ChrName, \
     CAST(CONVERT(ChrName USING gbk) AS BINARY) AS name_gbk, SelectID, IsDeleted";

impl Db {
    /// 连接 `mir2_db` 并建立连接池（连不上即报错，M1 验收④）。
    ///
    /// Errors: 连接串非法、服务不可达、认证失败时返回错误。
    pub async fn connect(dsn: &LegacyDsn) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(8)
            .connect(&dsn.to_sqlx_url())
            .await
            .with_context(|| {
                format!(
                    "连接 MySQL 失败：{}:{}/{}（M1④：连不上库必须明确报错退出）",
                    dsn.host, dsn.port, dsn.database
                )
            })?;
        Ok(Self {
            pool,
            quick: HashMap::new(),
            by_account: HashMap::new(),
            index_ids: HashSet::new(),
            deleted_ids: Vec::new(),
            char_ids: HashSet::new(),
            name_quick: HashMap::new(),
            name_text: HashMap::new(),
        })
    }

    /// 载入两张表的快速索引（C# 两个 `LoadQuickList`）。
    ///
    /// Errors: 表/列缺失（`SELECT` 报错）、`characters` 里有重名列（C# `Dictionary.Add` 会抛）。
    pub async fn load_quick_lists(&mut self) -> Result<()> {
        self.load_indexes().await?;
        self.load_chars().await?;
        Ok(())
    }

    /// `characters_indexes`（`IsDeleted < 2`，与 C# 的 SQL 一字不差）。
    async fn load_indexes(&mut self) -> Result<()> {
        self.quick.clear();
        self.by_account.clear();
        self.index_ids.clear();
        self.deleted_ids.clear();
        let sql = format!("select {INDEX_COLS} from characters_indexes where IsDeleted < 2");
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .context("载入 characters_indexes 失败（表/列与 C# 预期不符 ⇒ 启动失败）")?;
        let mut account_list: Vec<(Vec<u8>, Vec<u8>, i64)> = Vec::new();
        let mut names: Vec<(Vec<u8>, i64)> = Vec::new();
        let mut deleted: Vec<i64> = Vec::new();
        for row in &rows {
            let id = int_col(row, "Id")?;
            let account = text_col(row, "Account")?;
            let name_gbk = gbk_col(row, "name_gbk")?;
            if int_col(row, "IsDeleted")? != 0 {
                deleted.push(id);
                continue;
            }
            names.push((fold_key(&name_gbk), id));
            account_list.push((fold_key(account.as_bytes()), name_gbk, id));
        }
        for (key, id) in names {
            self.quick.insert(key, id);
            self.index_ids.insert(id);
        }
        self.deleted_ids = deleted;
        for (account, name_gbk, id) in account_list {
            self.by_account.entry(account).or_default().push(Quick {
                index: id,
                name_text: String::from_utf8_lossy(&name_gbk).into_owned(),
                name_gbk,
            });
        }
        Ok(())
    }

    /// `characters`（`Deleted=0`，与 C# 的 SQL 一字不差）。
    async fn load_chars(&mut self) -> Result<()> {
        self.char_ids.clear();
        self.name_quick.clear();
        self.name_text.clear();
        let rows = sqlx::query(
            "SELECT Id, LoginID, ChrName, CAST(CONVERT(ChrName USING gbk) AS BINARY) AS name_gbk \
             FROM characters WHERE Deleted=0",
        )
        .fetch_all(&self.pool)
        .await
        .context("载入 characters 失败（表/列与 C# 预期不符 ⇒ 启动失败）")?;
        for row in &rows {
            let id = int_col(row, "Id")?;
            let name_text = text_col(row, "ChrName")?;
            if name_text.is_empty() {
                continue;
            }
            let name_gbk = gbk_col(row, "name_gbk")?;
            if self.name_quick.insert(fold_key(&name_gbk), id).is_some() {
                bail!(
                    "characters 表里角色名重复（C# 侧 Dictionary.Add 会抛异常 ⇒ 启动失败）：{name_text}"
                );
            }
            self.char_ids.insert(id);
            self.name_text.insert(name_text.to_lowercase(), id);
        }
        Ok(())
    }

    /// C# `PlayRecordStorage.Index`：名字 → `characters_indexes.Id`。
    #[must_use]
    pub fn record_index(&self, name_gbk: &[u8]) -> Option<i64> {
        self.quick.get(&fold_key(name_gbk)).copied()
    }

    /// C# `PlayRecordStorage.FindByAccount`：账号 → 角色列表（账号键大小写不敏感）。
    #[must_use]
    pub fn find_by_account(&self, account: &[u8]) -> Vec<Quick> {
        self.by_account
            .get(&fold_key(account))
            .cloned()
            .unwrap_or_default()
    }

    /// C# `PlayRecordStorage.GetBy`：`Id > 0` 才查库，取不到返回 `None`。
    ///
    /// Errors: SQL 失败时返回错误。
    pub async fn record_get_by(&self, id: i64) -> Result<Option<IndexRow>> {
        if id <= 0 {
            return Ok(None);
        }
        let sql = format!("select {INDEX_COLS} from characters_indexes where Id=?");
        let Some(row) = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .context("查询 characters_indexes 失败")?
        else {
            return Ok(None);
        };
        Ok(Some(IndexRow {
            id: int_col(&row, "Id")?,
            account_text: text_col(&row, "Account")?,
            name_gbk: gbk_col(&row, "name_gbk")?,
            selected: u8::try_from(int_col(&row, "SelectID")? & 0xFF).unwrap_or(0),
            deleted: int_col(&row, "IsDeleted")? != 0,
        }))
    }

    /// C# `PlayRecordStorage.Add`：插入一行索引（`IsDeleted=0`），返回新 Id。
    ///
    /// 名字已在索引里（含"已软删但未重启"的）即返回 `Ok(None)`（C# 返回 false）。
    ///
    /// Errors: SQL 失败时返回错误。
    pub async fn record_add(
        &mut self,
        name_gbk: &[u8],
        account_text: &str,
        sel_gate_id: i16,
    ) -> Result<Option<i64>> {
        if self.quick.contains_key(&fold_key(name_gbk)) {
            return Ok(None);
        }
        // C# `Add` 先从 `_deletedList` 头部取一个待复用 Id（`_recordCount++` 为备选），
        // 随后被 INSERT 的 `LastInsertedId` 覆盖 ⇒ 取值只影响这两个计数器，如实照搬。
        let reuse = if self.deleted_ids.is_empty() {
            None
        } else {
            Some(self.deleted_ids.remove(0))
        };
        let res = sqlx::query(
            "INSERT INTO characters_indexes (Account, ChrName, SelectID, IsDeleted, CreateDate, ModifyDate) \
             VALUES (?, CONVERT(? USING gbk), ?, 0, now(), now())",
        )
        .bind(account_text)
        .bind(name_gbk)
        // C# 绑的是 `PlayerRecordData.Selected`（此处恒 0），`Header.SelectID`（网关 ID）只进内存索引
        .bind(0i16)
        .execute(&self.pool)
        .await
        .context("插入 characters_indexes 失败")?;
        #[allow(clippy::cast_possible_wrap)]
        let id = res.last_insert_id() as i64;
        self.quick.insert(fold_key(name_gbk), id);
        self.index_ids.insert(id);
        self.by_account
            .entry(fold_key(account_text.as_bytes()))
            .or_default()
            .push(Quick {
                index: id,
                name_gbk: name_gbk.to_vec(),
                name_text: String::from_utf8_lossy(name_gbk).into_owned(),
            });
        crate::log::info(&format!(
            "新建角色索引[{}] account={account_text} id={id} selGate={sel_gate_id} reuse={reuse:?}",
            String::from_utf8_lossy(name_gbk),
        ));
        Ok(Some(id))
    }

    /// C# `PlayRecordStorage.Update`（**带** `_indexQuickList` 存在性守卫）：整行更新。
    ///
    /// Errors: SQL 失败时返回错误。
    pub async fn record_update(&self, row: &IndexRow) -> Result<bool> {
        if !self.index_ids.contains(&row.id) {
            return Ok(false);
        }
        self.write_index_row(row).await?;
        Ok(true)
    }

    /// C# `PlayRecordStorage.Delete(sChrName)`：软删索引（`IsDeleted=2`）并摘掉 `_indexQuickList`。
    ///
    /// **如实保留 C# 的怪行为**：`_quickList`（名字 → Id）**不删**，所以重启前同名角色
    /// 不能重建（`Add` 命中已存在的名字即返回 false）。名字不在表里时 C# 也返回 true。
    ///
    /// Errors: SQL 失败时返回错误。
    pub async fn record_soft_delete(&mut self, name_gbk: &[u8]) -> Result<bool> {
        let Some(id) = self.quick.get(&fold_key(name_gbk)).copied() else {
            return Ok(true);
        };
        sqlx::query("UPDATE characters_indexes SET IsDeleted = 2, ModifyDate = now() WHERE Id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("软删 characters_indexes 失败")?;
        self.index_ids.remove(&id);
        Ok(true)
    }

    /// C# `PlayRecordStorage.UpdateBy`（**不带**守卫）：选角时写 `SelectID`。
    ///
    /// Errors: SQL 失败时返回错误。
    pub async fn record_update_by(&self, row: &IndexRow) -> Result<()> {
        self.write_index_row(row).await
    }

    /// `UPDATE characters_indexes`（C# `UpdateChrIndexes`，列与顺序一致）。
    async fn write_index_row(&self, row: &IndexRow) -> Result<()> {
        sqlx::query(
            "UPDATE characters_indexes SET Account = ?, ChrName = CONVERT(? USING gbk), \
             SelectID = ?, IsDeleted = ?, ModifyDate = now() WHERE Id = ?",
        )
        .bind(&row.account_text)
        .bind(&row.name_gbk)
        .bind(i16::from(row.selected))
        .bind(i16::from(row.deleted))
        .bind(row.id)
        .execute(&self.pool)
        .await
        .context("更新 characters_indexes 失败")?;
        Ok(())
    }

    /// C# `PlayRecordStorage.ChrCountOfAccount`：账号下**未删除**角色数（逐条 `GetBy` 复核）。
    ///
    /// Errors: SQL 失败时返回错误。
    pub async fn chr_count_of_account(&self, account: &[u8]) -> Result<i32> {
        let mut n = 0;
        for q in self.find_by_account(account) {
            if let Some(row) = self.record_get_by(q.index).await? {
                if !row.deleted {
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// C# `PlayDataStorage.Index`：`ChrName → characters.Id`。
    #[must_use]
    pub fn data_index(&self, name_gbk: &[u8]) -> Option<i64> {
        self.name_quick.get(&fold_key(name_gbk)).copied()
    }

    /// C# `PlayDataStorage.GetQryChar`：取选角列表要用的字段。
    ///
    /// 取不到行返回 `Ok(None)` —— C# 此时把 `ref` 对象留成 `null`，调用方随即 NRE
    /// （被上层 catch、**不回包**）；Rust 侧按错误处置，行为等价。
    ///
    /// Errors: SQL 失败时返回错误。
    pub async fn get_query_char(&self, id: i64) -> Result<Option<QryChar>> {
        if id < 0 {
            return Ok(None);
        }
        let Some(row) =
            sqlx::query("SELECT ChrName, Job, Hair, Sex, Level FROM characters WHERE ID=?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .context("查询 characters（选角字段）失败")?
        else {
            return Ok(None);
        };
        Ok(Some(QryChar {
            job: u8::try_from(int_col(&row, "Job")? & 0xFF).unwrap_or(0),
            hair: u8::try_from(int_col(&row, "Hair")? & 0xFF).unwrap_or(0),
            sex: u8::try_from(int_col(&row, "Sex")? & 0xFF).unwrap_or(0),
            level: u16::try_from(int_col(&row, "Level")? & 0xFFFF).unwrap_or(0),
        }))
    }

    /// C# `PlayDataStorage.Query`：取 `ChrName` + `MapName`（选角决定进哪张图）。
    ///
    /// Errors: SQL 失败时返回错误。
    pub async fn query_name_and_map(&self, id: i64) -> Result<Option<(String, String)>> {
        let Some(row) = sqlx::query("SELECT ChrName, MapName FROM characters WHERE ID=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .context("查询 characters（地图）失败")?
        else {
            return Ok(None);
        };
        let name = row.try_get::<Option<String>, _>("ChrName").ok().flatten();
        let map = row.try_get::<Option<String>, _>("MapName").ok().flatten();
        Ok(Some((name.unwrap_or_default(), map.unwrap_or_default())))
    }

    /// C# `PlayDataStorage.Add`：建角写 `characters` + 五张附属表（同一事务）。
    ///
    /// 附属表的失败在 C# 里各自 `catch` 吞掉（只打日志、不回滚），这里同样只记日志。
    ///
    /// Errors: 主表插入失败、事务失败时返回错误。
    pub async fn data_add(
        &mut self,
        name_gbk: &[u8],
        account_text: &str,
        sex: u8,
        job: u8,
        hair: u8,
    ) -> Result<Option<i64>> {
        let mut tx = self.pool.begin().await.context("开启事务失败")?;
        let player_id = create_character(&mut tx, name_gbk, account_text, sex, job, hair).await?;
        if player_id <= 0 {
            tx.rollback().await.ok();
            return Ok(None);
        }
        // 附属表：C# 各自 catch 吞异常（不随主表一起回滚）
        if let Err(e) = sqlx::query(
            "INSERT INTO characters_ablity (PlayerId, Level, Ac, Mac, Dc, Mc, Sc, Hp, Mp, MaxHP, \
             MAxMP, Exp, MaxExp, Weight, MaxWeight, WearWeight, MaxWearWeight, HandWeight, MaxHandWeight) \
             VALUES (?, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)",
        )
        .bind(player_id)
        .execute(&mut *tx)
        .await
        {
            crate::log::warn(&format!(
                "建角附属表 characters_ablity 写入失败（C# 同样吞掉）: {e}"
            ));
        }
        if let Err(e) = sqlx::query(
            "INSERT INTO characters_status (PlayerId, Status0, Status1, Status2, Status3, Status4, \
             Status5, Status6, Status7, Status8, Status9, Status10, Status11, Status12, Status13, \
             Status14, Status15) VALUES (?, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)",
        )
        .bind(player_id)
        .execute(&mut *tx)
        .await
        {
            crate::log::warn(&format!(
                "建角附属表 characters_status 写入失败（C# 同样吞掉）: {e}"
            ));
        }
        for (table, count) in [
            ("characters_item", 13i32),
            ("characters_bagitem", 46),
            ("characters_storageitem", 50),
        ] {
            let sql = format!(
                "INSERT INTO {table} (PlayerId, Position, MakeIndex, StdIndex, Dura, DuraMax) \
                 VALUES (?, ?, 0, 0, 0, 0)"
            );
            for pos in 0..count {
                if let Err(e) = sqlx::query(&sql)
                    .bind(player_id)
                    .bind(pos)
                    .execute(&mut *tx)
                    .await
                {
                    crate::log::warn(&format!("建角附属表 {table} 写入失败（C# 同样吞掉）: {e}"));
                    break;
                }
            }
        }
        tx.commit().await.context("提交建角事务失败")?;
        self.name_quick.insert(fold_key(name_gbk), player_id);
        self.name_text
            .insert(String::from_utf8_lossy(name_gbk).to_lowercase(), player_id);
        self.char_ids.insert(player_id);
        Ok(Some(player_id))
    }
}

/// C# `PlayDataStorage.CreateCharacters`：列与值一字不差（新角色字段全是默认值 0 / 空串）。
async fn create_character(
    tx: &mut Transaction<'_, MySql>,
    name_gbk: &[u8],
    account_text: &str,
    sex: u8,
    job: u8,
    hair: u8,
) -> Result<i64> {
    let res = sqlx::query(
        // 值口径逐列对 C# `PlayDataStorage.CreateCharacters`：`MapName`/`HomeMap`/`MasterName`/`DearName`/
        // `StoragePwd` 在 C# 绑的是 `CharacterData` 里**未赋值**的字符串 ⇒ 落库是 **NULL**（不是空串）；
        // `Ac` 那格 C# 绑的是 `hd.Abil.Level`（既有 bug），新角色两个值都是 0，故这里写 0。
        "insert into characters (ServerIndex, LoginID, ChrName, MapName, CX, CY, Level, Dir, Hair, Sex, Job, \
         Gold, GamePoint, HomeMap, HomeX, HomeY, PkPoint, ReLevel, AttatckMode, FightZoneDieCount, BodyLuck, \
         IncHealth, IncSpell, IncHealing, CreditPoint, BonusPoint, HungerStatus, PayMentPoint, LockLogon, \
         MarryCount, AllowGroupReCall, GroupRcallTime, AllowGuildReCall, IsMaster, MasterName, DearName, \
         StoragePwd, Deleted, CREATEDATE, LASTUPDATE) VALUES \
         (0, ?, CONVERT(? USING gbk), NULL, 0, 0, 0, 0, ?, ?, ?, 0, 0, NULL, 0, 0, 0, 0, 0, 0, 0, \
          0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, NULL, NULL, NULL, 0, now(), now())",
    )
    .bind(account_text)
    .bind(name_gbk)
    .bind(i16::from(hair))
    .bind(i16::from(sex))
    .bind(i16::from(job))
    .execute(&mut **tx)
    .await
    .context("插入 characters 失败")?;
    #[allow(clippy::cast_possible_wrap)]
    Ok(res.last_insert_id() as i64)
}
