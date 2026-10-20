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
//! 建索引时取角色名的 GBK 字节走服务端转换 `CAST(CONVERT(ChrName USING gbk) AS BINARY)`，
//! 即让 MySQL 做 utf8mb3→gbk 的编码（等价 .NET `Encoding.GetEncoding("gb2312").GetBytes`）。
//! **例外**：6000 面 1100 取档帧的 `ChrName` 由 oracle `HUtil32.GetBytes` 产生，必须与
//! 库可用性无关 —— 见 [`crate::gbk::encode`]（本地 `encoding_rs::GBK`，A-14 口径）。
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
    /// Errors: 表/列缺失（`SELECT` 报错）、两张表里有重名列（C# `Dictionary.Add` 会抛）。
    pub async fn load_quick_lists(&mut self) -> Result<()> {
        self.load_indexes().await?;
        self.load_chars().await?;
        Ok(())
    }

    /// `characters_indexes`（`IsDeleted < 2`，与 C# 的 SQL 一字不差）。
    ///
    /// 重名判定按折叠键（等价 C# `StringComparer.OrdinalIgnoreCase`）：C# 的
    /// `_quickList.Add` / `_indexQuickList.Add` 都是 `Dictionary.Add`，重名/重 Id 直接抛
    /// `ArgumentException` ⇒ 宿主启动失败（LB-09）。这里同样 `bail!`，**不得**静默覆盖。
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
        let mut names: Vec<(Vec<u8>, String, i64)> = Vec::new();
        let mut deleted: Vec<i64> = Vec::new();
        for row in &rows {
            let id = int_col(row, "Id")?;
            let account = text_col(row, "Account")?;
            let name_text = text_col(row, "ChrName")?;
            let name_gbk = gbk_col(row, "name_gbk")?;
            if int_col(row, "IsDeleted")? != 0 {
                deleted.push(id);
                continue;
            }
            names.push((fold_key(&name_gbk), name_text, id));
            account_list.push((fold_key(account.as_bytes()), name_gbk, id));
        }
        for (key, name_text, id) in names {
            // LB-09：oracle `PlayRecordStorage.LoadQuickList` 的 `_quickList.Add` 重名即抛，
            // 未捕获 ⇒ 启动失败；Rust 曾经 `insert` 静默覆盖（带病运行），两侧口径不一致。
            if self.quick.insert(key, id).is_some() {
                bail!(
                    "characters_indexes 表里角色名重复（C# 侧 Dictionary.Add 会抛异常 ⇒ 启动失败）：{name_text}"
                );
            }
            // 同一循环里 oracle 还有 `_indexQuickList.Add(Id, Name)`（Dictionary.Add ⇒ 重 Id 也抛）；
            // `Id` 是主键，实际不可达，仍按 oracle 口径守住。
            if !self.index_ids.insert(id) {
                bail!(
                    "characters_indexes 表里 Id 重复（C# 侧 _indexQuickList 的 Dictionary.Add 会抛异常 ⇒ 启动失败）：{id}"
                );
            }
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
            // oracle 的 `_IndexQuickIdMap.Add(Id, Id)` 同样是 `Dictionary.Add`；`Id` 是主键，
            // 重 Id 不可达，仍按 oracle 口径守住。
            if !self.char_ids.insert(id) {
                bail!(
                    "characters 表里 Id 重复（C# 侧 _IndexQuickIdMap 的 Dictionary.Add 会抛异常 ⇒ 启动失败）：{id}"
                );
            }
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

    /// C# `PlayDataStorage.Add`：建角写 `characters` + 六张附属表（同一事务）。
    ///
    /// 附属表的失败在 C# 里各自 `catch` 吞掉（只打日志、不回滚），这里同样只记日志。
    ///
    /// 早退守卫复刻 `Add → CheckChrExists(_NameQuickMap)`：同名已在 `characters` 即返回
    /// `None`，整条事务不开始（含附属表），与 oracle `Add` 的重复判定**等价**。
    ///
    /// **不建 `characters_bonusability` 行**：oracle `PlayDataStorage.Add`/`AddRecord` 同样
    /// **没有** `CreateBonusAbility`（全仓该表只有 `SELECT` 与 `UPDATE`）⇒ 本仓照抄。
    /// 审计件的 G-03 原判「Rust 缺 INSERT」**不成立**——两侧同为「奖励属性永不落库」，
    /// 属 oracle 既有行为（硬约束③：先照搬，不在此批修）。
    ///
    /// **F4 登记（本片不展开）**：`Ok(None)` 同时承载两种 **oracle 语义不同**的情形——
    /// ① `name_quick` 命中（C# `CheckChrExists` => `Add` 返回 false，**不建角**）；
    /// ② `create_character` 返回 `id <= 0`（C# `CreateCharacters`/`AddRecord` 失败 =>
    ///    上一条 INSERT 已执行、事务被回滚，返回 false）。
    /// 两者在 oracle 的对外行为不同：①「名字已存在」⇒ `NewChrData` 返 false（`UserService.cs:261-265`，
    /// 调用方随后走 `Delete` 的异常路径、**不回包**）；②「建角失败」⇒ `Add` 返 false 被忽略、
    /// `NewChrData` 恒 true ⇒ **回成功**。本仓 ① 已由 `IndexExists` 分支 `record_soft_delete + bail!`
    /// 对齐（不回包）；② 在 Rust 由 `Err` 传播（与 oracle 吞异常同口径），`Ok(None)` 的 `id<=0`
    /// 支近乎不可达、若触发则与 oracle 不一致（**未覆盖**）。后续若要把两者映射成不同应答/日志级别，
    /// 必须先在此处拆分返回类型（如 `enum DataAddOutcome { Added(i64), NameExists, CreateFailed }`），
    /// 而不是继续用 `Option`。拆分点：本函数的 `Ok(None)` 两处 + `handler.rs` 的 `Ok(None)` 匹配臂。
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
        // C# `PlayDataStorage.Add` → `CheckChrExists(_NameQuickMap)`：同名已在 `characters`
        // （启动时载入 + 本进程内新建）即返回 false，后续 INSERT 一条都不执行。
        if self.name_quick.contains_key(&fold_key(name_gbk)) {
            return Ok(None);
        }
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
        // C# `CreateUseItem`/`CreateBagItem` 的 INSERT 循环在 `AddRecord` 的 try **之外**
        // ⇒ 任一 INSERT 抛异常就冒到 `AddRecord` 的 catch ⇒ `RollBack()`（连 characters 行
        // 一起回滚）；`CreateStorageItem` 的循环在自己的 try 内 ⇒ 吞错并继续提交（LB-07）。
        for (table, count, swallow) in [
            ("characters_item", 13i32, false),
            ("characters_bagitem", 46, false),
            ("characters_storageitem", 50, true),
        ] {
            let sql = format!(
                "INSERT INTO {table} (PlayerId, Position, MakeIndex, StdIndex, Dura, DuraMax) \
                 VALUES (?, ?, 0, 0, 0, 0)"
            );
            let mut items: Vec<Option<mir2_storage::playerdata::ServerUserItem>> = Vec::new();
            for pos in 0..count {
                match sqlx::query(&sql)
                    .bind(player_id)
                    .bind(pos)
                    .execute(&mut *tx)
                    .await
                {
                    Ok(_) => {
                        items.push(Some(mir2_storage::playerdata::ServerUserItem::csharp_new()));
                    }
                    Err(e) => {
                        if swallow {
                            crate::log::warn(&format!(
                                "建角附属表 {table} 写入失败（C# CreateStorageItem 吞错并继续）: {e}"
                            ));
                            break;
                        }
                        // oracle 整次建角回滚（含主表 characters 行）；调用方按 `Add` 返回
                        // false 处理（5100 侧忽略返回值回成功、6000 侧回 1100 recog=0）。
                        tx.rollback().await.ok();
                        return Err(anyhow::anyhow!(
                            "建角附属表 {table} 写入失败（C# 在 try 外 ⇒ 整次建角回滚）: {e}"
                        ));
                    }
                }
            }
            // C# `CreateUseItem`/`CreateBagItem`/`CreateStorageItem` 在插完空行后调用
            // `CreateItemAttr`；`addItem` 全是 `new UserItem()`（`MakeIndex=0`）⇒ 门把它们
            // 全部挡掉（G-01 的 oracle 既有行为，照搬）。
            if let Err(e) = crate::playdata::create_item_attr(&mut tx, player_id, &items).await {
                crate::log::warn(&format!(
                    "建角附属表 {table} CreateItemAttr 写入失败（C# 同样吞掉）: {e}"
                ));
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

#[cfg(test)]
mod create_rows_tests {
    use super::{int_col, Db};
    use mir2_storage::playerdata::{Ability, CharacterDataInfo};
    use mir2_storage::LegacyDsn;
    use sqlx::{Connection, MySqlConnection, Row};

    const DEFAULT_DSN: &str = "server=127.0.0.1;uid=root;pwd=;database=mir2_test_pcrow_rows;";
    /// 建角涉及的 8 张表（DDL 从冻结的 `sql/mir2_db.sql` 原样抽取）。
    const TABLES: [&str; 8] = [
        "characters_indexes",
        "characters",
        "characters_ablity",
        "characters_bonusability",
        "characters_item",
        "characters_bagitem",
        "characters_storageitem",
        "characters_status",
    ];
    const ABLITY_COLS: [&str; 18] = [
        "Level",
        "Ac",
        "Mac",
        "Dc",
        "Mc",
        "Sc",
        "Hp",
        "Mp",
        "MaxHP",
        "MAxMP",
        "Exp",
        "MaxExp",
        "Weight",
        "MaxWeight",
        "WearWeight",
        "MaxWearWeight",
        "HandWeight",
        "MaxHandWeight",
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

    async fn setup(dsn: &LegacyDsn) {
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
    }

    async fn row_ints(conn: &mut MySqlConnection, sql: &str, id: i64, cols: &[&str]) -> Vec<i64> {
        let row = sqlx::query(sql)
            .bind(id)
            .fetch_one(&mut *conn)
            .await
            .expect("期望的行不存在");
        cols.iter()
            .map(|c| int_col(&row, c).expect("整数列"))
            .collect()
    }

    async fn count(conn: &mut MySqlConnection, sql: &str, id: i64) -> i64 {
        let row = sqlx::query(sql)
            .bind(id)
            .fetch_one(&mut *conn)
            .await
            .expect("计数查询失败");
        int_col(&row, "c").expect("计数列")
    }

    async fn count_str(conn: &mut MySqlConnection, sql: &str, name: &str) -> i64 {
        let row = sqlx::query(sql)
            .bind(name)
            .fetch_one(&mut *conn)
            .await
            .expect("计数查询失败");
        int_col(&row, "c").expect("计数列")
    }

    /// 构造级验收（G-20；G-03 已证伪——oracle 同样不建 `characters_bonusability` 行）：
    /// a) 正常建角后 `characters`/`characters_ablity` 各 1 行（ablity 18 列全 0），
    ///    **`characters_bonusability` 0 行**（与 oracle `Add` 一样不建）；
    /// b) 索引缺失触发的 `play_data_add` 兜底路径必须建出 `characters_ablity` 行，值 = `hd.Abil`
    ///    （`Ac = Level` 的既有 bug 照搬）；
    /// c) 二次建角（同名）不产生任何重复行（oracle `Add → CheckChrExists` 语义）。
    ///
    /// 跑法（需本机 MySQL；默认 `root` 空口令、库名 `mir2_test_pcrow_rows`，测试自建自删）：
    /// `cargo test -p db-srv --bins create_rows_tests -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "需要本机 MySQL；MIR2_TEST_DB_DSN 可覆盖，库名必须以 mir2_test_ 开头"]
    #[allow(clippy::too_many_lines)]
    async fn normal_and_fallback_creation_write_oracle_default_rows() {
        let text = std::env::var("MIR2_TEST_DB_DSN").unwrap_or_else(|_| DEFAULT_DSN.to_string());
        let dsn = LegacyDsn::parse(&text).expect("测试 DSN 非法");
        assert!(
            dsn.database.starts_with("mir2_test_"),
            "拒绝写非测试库：{}",
            dsn.database
        );
        setup(&dsn).await;

        let mut db = Db::connect(&dsn).await.expect("连测试库失败");
        db.load_quick_lists().await.expect("载入索引失败");

        // ---- a) 正常建角（handler.rs → data_add） ----
        let id = db
            .data_add(b"pcrow_a", "pcrow_acct", 1, 2, 3)
            .await
            .expect("data_add 失败")
            .expect("首次建角应成功");
        let mut conn = MySqlConnection::connect(&dsn.to_sqlx_url())
            .await
            .expect("连测试库失败");
        assert_eq!(
            count(
                &mut conn,
                "SELECT COUNT(*) AS c FROM characters WHERE Id=?",
                id
            )
            .await,
            1,
            "characters 应恰 1 行"
        );
        let abl = row_ints(
            &mut conn,
            "SELECT * FROM characters_ablity WHERE PlayerId=?",
            id,
            &ABLITY_COLS,
        )
        .await;
        assert_eq!(abl.len(), 18);
        assert!(abl.iter().all(|v| *v == 0), "ablity 列值必须全 0: {abl:?}");
        // G-03 证伪：oracle `PlayDataStorage.Add` 没有 `CreateBonusAbility`，全仓该表只有
        // SELECT/UPDATE ⇒ 正常建角**不得**出现该行（照抄 oracle 既有行为）。
        assert_eq!(
            count(
                &mut conn,
                "SELECT COUNT(*) AS c FROM characters_bonusability WHERE PlayerId=?",
                id
            )
            .await,
            0,
            "oracle 不建 bonusability 行，Rust 必须照抄"
        );
        assert_eq!(
            count(
                &mut conn,
                "SELECT COUNT(*) AS c FROM characters_status WHERE PlayerId=?",
                id
            )
            .await,
            1
        );

        // ---- b) 兜底建角（data.rs::save_human_rcd → play_data_add） ----
        let mut info = CharacterDataInfo::csharp_new();
        info.header.name = Some("pcrow_b".to_string());
        info.header.s_account = Some("pcrow_acct".to_string());
        let d = info.data.as_mut().expect("csharp_new 的 Data 非空");
        d.chr_name = Some("pcrow_b".to_string());
        d.account = Some("pcrow_acct".to_string());
        d.abil = Some(Ability {
            level: 7,
            ac: 99, // 必须被忽略：oracle CreateAblity 的 @Ac 绑的是 Level
            mac: 1,
            dc: 2,
            mc: 3,
            sc: 4,
            hp: 5,
            mp: 6,
            max_hp: 8,
            max_mp: 9,
            exp: 10,
            max_exp: 11,
            weight: 12,
            max_weight: 13,
            wear_weight: 14,
            max_wear_weight: 15,
            hand_weight: 16,
            max_hand_weight: 17,
            ..Ability::default()
        });
        assert!(db.play_data_add(&info).await.expect("play_data_add 失败"));
        let row = sqlx::query("SELECT Id FROM characters WHERE ChrName=?")
            .bind("pcrow_b")
            .fetch_one(&mut conn)
            .await
            .expect("兜底建角未写 characters");
        let id_b: i64 = row.try_get("Id").expect("Id 列");
        // **G6**：oracle `Add` 成功后立即 `_NameQuickMap.Add(sChrName, playerId)`
        // （`Add.cs:42-47`）⇒ 6000 面新建的角色必须立刻对 5100 面可见。Rust 的 5100 面
        // `data_index` 读 GBK 字节键 ⇒ `play_data_add` 也必须更新 `name_quick`。
        let gbk_b = db.gbk_bytes("pcrow_b").await.expect("GBK 转换失败");
        assert_eq!(
            db.data_index(&gbk_b),
            Some(id_b),
            "G6：play_data_add 必须更新 name_quick（否则 5100 面看不到该角色）"
        );
        assert_eq!(
            count(
                &mut conn,
                "SELECT COUNT(*) AS c FROM characters_ablity WHERE PlayerId=?",
                id_b
            )
            .await,
            1,
            "兜底路径必须建出 characters_ablity 行（G-20）"
        );
        let abl_b = row_ints(
            &mut conn,
            "SELECT * FROM characters_ablity WHERE PlayerId=?",
            id_b,
            &ABLITY_COLS,
        )
        .await;
        assert_eq!(
            abl_b,
            vec![7, 7, 1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17],
            "兜底建角 ablity 值必须 = hd.Abil（Ac = Level）"
        );

        // ---- c) 二次建角：同名不重复插入（oracle Add 的重复判定） ----
        assert!(!db
            .play_data_add(&info)
            .await
            .expect("二次 play_data_add 失败"));
        assert_eq!(
            count_str(
                &mut conn,
                "SELECT COUNT(*) AS c FROM characters WHERE ChrName=?",
                "pcrow_b"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &mut conn,
                "SELECT COUNT(*) AS c FROM characters_ablity WHERE PlayerId=?",
                id_b
            )
            .await,
            1
        );
        assert!(
            db.data_add(b"pcrow_a", "pcrow_acct", 0, 0, 0)
                .await
                .expect("二次 data_add 失败")
                .is_none(),
            "同名二次 data_add 必须返回 None"
        );
        assert_eq!(
            count_str(
                &mut conn,
                "SELECT COUNT(*) AS c FROM characters WHERE ChrName=?",
                "pcrow_a"
            )
            .await,
            1
        );
        assert_eq!(
            count(
                &mut conn,
                "SELECT COUNT(*) AS c FROM characters_bonusability WHERE PlayerId=?",
                id
            )
            .await,
            0,
            "二次建角后仍不得出现 bonusability 行（oracle 从不建）"
        );

        drop(db);
        sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", dsn.database))
            .execute(&mut conn)
            .await
            .expect("清理测试库失败");
    }
}

/// LB-09：`characters_indexes` 重名必须**启动失败**（oracle `PlayRecordStorage.LoadQuickList`
/// 用 `Dictionary.Add`，重名抛 `ArgumentException` 且无 catch ⇒ 宿主退出），不得像旧的
/// `HashMap::insert` 那样静默覆盖后带病运行。
///
/// 跑法（需本机 MySQL；用例自建自删 `mir2_test_*` 库）：
/// `cargo test -p db-srv --bins index_dup_tests -- --ignored --nocapture`
#[cfg(test)]
mod index_dup_tests {
    use crate::db::Db;
    use crate::testsupport as ts;

    const DB: &str = "mir2_test_lb09_dup_index";

    #[tokio::test]
    #[ignore = "需要本机 MySQL；自建 mir2_test_* 库"]
    async fn duplicate_index_name_fails_startup() {
        let dsn = ts::setup_db(DB).await;
        let mut c = ts::conn(&dsn).await;

        // 两条同 `ChrName`、`IsDeleted=0` 的索引行。`characters_indexes` 只有 `Id` 主键、
        // 没有 `ChrName` 唯一索引 ⇒ 历史脏数据/双写可以造出这种行（LB-09 的可达场景）。
        for account in ["lb09_acct_a", "lb09_acct_b"] {
            sqlx::query(
                "INSERT INTO characters_indexes (Account, ChrName, SelectID, IsDeleted, \
                 CreateDate, ModifyDate) VALUES (?, 'lb09_dup', 0, 0, now(), now())",
            )
            .bind(account)
            .execute(&mut c)
            .await
            .expect("插入重复索引行失败（表上无 ChrName 唯一索引，应当允许）");
        }

        // ---- 有重复 ⇒ `load_quick_lists`（main.rs 的启动路径）返回 Err，且信息含冲突键 ----
        let mut db = Db::connect(&dsn).await.expect("连测试库失败");
        let err = db
            .load_quick_lists()
            .await
            .expect_err("重名索引必须启动失败（oracle Dictionary.Add 会抛）");
        let msg = format!("{err:#}");
        assert!(msg.contains("lb09_dup"), "错误信息必须含冲突键：{msg}");
        assert!(
            msg.contains("重复"),
            "措辞必须与 characters 重名路径同口径：{msg}"
        );

        // ---- 去掉重复行 ⇒ 启动正常 ----
        sqlx::query(
            "DELETE FROM characters_indexes WHERE ChrName='lb09_dup' ORDER BY Id DESC LIMIT 1",
        )
        .execute(&mut c)
        .await
        .expect("删除重复行失败");
        db.load_quick_lists().await.expect("无重复时必须启动成功");
        assert!(db.record_index(b"lb09_dup").is_some(), "正常路径的索引缺失");
        assert_eq!(
            db.find_by_account(b"lb09_acct_a").len(),
            1,
            "正常路径应按账号建出 1 条索引"
        );

        drop(db);
        drop(c);
        ts::drop_db(&dsn).await;
    }
}
