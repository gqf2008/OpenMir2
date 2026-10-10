//! 账号存储 —— 1:1 移植 C# `LoginSrv/Storage/AccountStorage.cs`（574 行）。
//!
//! **语义要点（逐条来自 C# 真身，含刻意保留的怪行为）**：
//!
//! 1. **启动时一次性载入内存表**：`Initialization → LoadQuickList` 执行
//!    `SELECT Id,Account,State FROM account order by Id Desc`，只收 `State=0` 且账号非空的行，
//!    建 `账号 → Id` 映射（**大小写不敏感**）。此后只查内存 —— **SQL 旁路新开的号要重启才可见**
//!    （D2 实测：5 个 SQL 直开号全部登录失败；服务端自建号 3/3 成功，因 `Add` 会同步写内存表）。
//! 2. `GetAccount` 用 **INNER JOIN**：`account JOIN account_protection on Id=AccountId`
//!    ⇒ 缺 protection 行的账号**取不到资料**（表现为"资料读取失败"，非"账号不存在"）。
//! 3. `Index(account)` 返回的是 **Id 而不是位置**；`Add` 的重复判定是 `Index(s) > 0`
//!    ⇒ **Id 恰为 0 的账号会被当成不存在**（怪行为，保留）。
//! 4. `Update(nIndex, …)` 的越界判定是 `_accountMap.Count <= nIndex`
//!    ⇒ **Id ≥ 账号数**时更新被静默跳过（怪行为，保留；见 `update()` 的注释）。
//! 5. 建号在**同一事务**里写 `account` + `account_protection`（后者 13 列全填，缺失位填空串）。
//! 6. 删号是**软删**：`UPDATE account SET State=1, ModifyTime=now WHERE Account='…'`，并从内存表移除。
//!
//! **一处有意的行为偏离（M1 验收④授权）**：C# `Initialization` 连接失败只打日志、
//! 带空内存表继续跑（结果所有登录都变"账号不存在"）；本实现按验收④
//! **连不上库即返回错误**，由调用方明确报错退出。已登记 `tests/parity/whitelist.md` A-6。

use std::collections::HashMap;

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::Row;

use crate::legacy_dsn::LegacyDsn;
use crate::StorageError;

/// 内存索引条目（C# `AccountQuick`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountQuick {
    /// 账号名。
    pub account: String,
    /// **账号 Id**（不是位置）。
    pub index: i32,
}

/// 账号记录（C# `AccountRecord` + `UserEntry` + `UserEntryAdd` 中 `GetAccount` 实际读到的字段）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountRecord {
    /// `account.Id`。
    pub account_id: i32,
    /// `PassFailCount`。
    pub error_count: i32,
    /// `PassFailTime`。
    pub action_tick: i32,
    /// `Seconds`。
    pub play_time: i64,
    /// `PayMode`。
    pub pay_model: u8,
    /// 账号名。
    pub account: String,
    /// 密码（**明文**，行为等价优先，勿顺手哈希）。
    pub password: String,
    /// `account_protection.UserName`。
    pub user_name: String,
    /// `account_protection.Quiz2`。
    pub quiz2: String,
}

/// 账号存储（对应 C# `AccountStorage`）。
pub struct AccountStore {
    pool: MySqlPool,
    /// `账号(ASCII 小写规范化) → 条目`；大小写不敏感对应 C# `StringComparer.OrdinalIgnoreCase`。
    map: HashMap<String, AccountQuick>,
}

/// 从行里取整数：**按 sqlx 的类型阶梯逐个尝试**（MySQL `tinyint/smallint/int/bigint`
/// 分别映射 i8/i16/i32/i64），全部失败即报错 —— 不静默取默认值。
///
/// 教训：本文件第一版用 `try_get::<u8>("State").unwrap_or(1)`，对 `State tinyint`
/// （sqlx 映射 **i8**）静默失败 ⇒ 内存账号表读成 0 条（C# 侧正常）。实库测试当场抓到。
fn int_col(row: &sqlx::mysql::MySqlRow, name: &str) -> Result<i64, StorageError> {
    macro_rules! try_ty {
        ($($t:ty),*) => {$(
            if let Ok(v) = row.try_get::<$t, _>(name) {
                return Ok(i64::from(v));
            }
        )*};
    }
    try_ty!(i8, i16, i32, i64);
    if let Ok(v) = row.try_get::<u64, _>(name) {
        return i64::try_from(v).map_err(|_| StorageError::Corrupt(format!("列 {name} 超出 i64")));
    }
    Err(StorageError::Corrupt(format!(
        "列 {name} 不存在或类型不兼容"
    )))
}

/// 取可空整数（NULL → `None`）：同样走类型阶梯，不静默默认。
fn opt_int_col(row: &sqlx::mysql::MySqlRow, name: &str) -> Result<Option<i64>, StorageError> {
    macro_rules! try_ty {
        ($($t:ty),*) => {$(
            if let Ok(v) = row.try_get::<Option<$t>, _>(name) {
                return Ok(v.map(i64::from));
            }
        )*};
    }
    try_ty!(i8, i16, i32, i64);
    Err(StorageError::Corrupt(format!(
        "列 {name} 不存在或类型不兼容"
    )))
}

/// 取字符串（C# `dr.GetString`；NULL 视为空串）。
fn str_col(row: &sqlx::mysql::MySqlRow, name: &str) -> Result<String, StorageError> {
    row.try_get::<Option<String>, _>(name)
        .map(Option::unwrap_or_default)
        .map_err(|_| StorageError::Corrupt(format!("列 {name} 不存在或类型不兼容")))
}

/// 内存表键：C# 用 OrdinalIgnoreCase（ASCII 域内等价于 ASCII 小写；非 ASCII 差异见 whitelist B-103 同族）。
fn key(account: &str) -> String {
    account.to_ascii_lowercase()
}

impl AccountStore {
    /// 连接数据库并载入内存账号表（对应 C# `Initialization`）。
    ///
    /// # Errors
    /// 连不上库（含 DSN 缺键）时返回错误 —— 这是对 C#「只记日志继续跑」的**授权偏离**（M1 验收④）。
    pub async fn connect(dsn: &LegacyDsn) -> Result<Self, StorageError> {
        let pool = MySqlPoolOptions::new()
            .max_connections(8)
            .connect(&dsn.to_sqlx_url())
            .await
            .map_err(|e| StorageError::Backend(format!("连接 MySQL 失败：{e}")))?;
        let mut store = Self {
            pool,
            map: HashMap::new(),
        };
        store.load_quick_list().await?;
        Ok(store)
    }

    /// `LoadQuickList`：`SELECT Id,Account,State FROM account order by Id Desc`。
    ///
    /// # Errors
    /// 查询失败时返回错误。
    pub async fn load_quick_list(&mut self) -> Result<usize, StorageError> {
        let rows = sqlx::query("SELECT Id,Account,State FROM account order by Id Desc")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::Backend(format!("读取账号列表失败：{e}")))?;
        self.map.clear();
        for r in rows {
            let id = i32::try_from(int_col(&r, "Id")?)
                .map_err(|_| StorageError::Corrupt("account.Id 超出 i32".into()))?;
            let account = str_col(&r, "Account")?;
            let state = int_col(&r, "State")?;
            if state == 0 && !account.is_empty() {
                // C# `Dictionary.Add`：重复键会抛；此处同名行只可能来自库内重名，保留先出现（Id 更小）的那条
                self.map
                    .entry(key(&account))
                    .or_insert(AccountQuick { account, index: id });
            }
        }
        Ok(self.map.len())
    }

    /// 内存表中账号数（C# `_accountMap.Count`）。
    #[must_use]
    pub fn count(&self) -> usize {
        self.map.len()
    }

    /// `Index(account)`：命中返回 **Id**，未命中返回 `None`（C# 返回 -1）。
    #[must_use]
    pub fn index(&self, account: &str) -> Option<i32> {
        self.map.get(&key(account)).map(|q| q.index)
    }

    /// `FindByName`：把命中条目追加进列表，返回列表长度。
    pub fn find_by_name(&self, account: &str, list: &mut Vec<AccountQuick>) -> usize {
        if let Some(q) = self.map.get(&key(account)) {
            list.push(q.clone());
        }
        list.len()
    }

    /// `GetAccount(nIndex)`：`SELECT a.*,b.* FROM account a join account_protection b on a.Id=b.AccountId WHERE ID=?`
    ///
    /// INNER JOIN ⇒ 缺 protection 行返回 `Ok(None)`。
    ///
    /// # Errors
    /// 查询失败时返回错误（C# 侧该分支打印"获取账号信息失败"并返回 -1）。
    pub async fn get(&self, id: i32) -> Result<Option<AccountRecord>, StorageError> {
        if id < 0 {
            return Ok(None);
        }
        let row = sqlx::query(
            "SELECT a.*,b.* FROM account a join account_protection b on a.Id=b.AccountId WHERE ID=?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Backend(format!("获取账号信息失败：{e}")))?;
        let Some(r) = row else { return Ok(None) };
        Ok(Some(AccountRecord {
            account_id: i32::try_from(int_col(&r, "Id")?)
                .map_err(|_| StorageError::Corrupt("account.Id 超出 i32".into()))?,
            // PassFailCount/PassFailTime 在 DDL 里可空；NULL 按 C# `GetInt32` 的 0 处理
            error_count: i32::try_from(opt_int_col(&r, "PassFailCount")?.unwrap_or(0))
                .unwrap_or_default(),
            action_tick: i32::try_from(opt_int_col(&r, "PassFailTime")?.unwrap_or(0))
                .unwrap_or_default(),
            play_time: int_col(&r, "Seconds")?,
            pay_model: u8::try_from(int_col(&r, "PayMode")?).unwrap_or_default(),
            account: str_col(&r, "Account")?,
            password: str_col(&r, "PassWord")?,
            user_name: str_col(&r, "UserName")?,
            quiz2: str_col(&r, "Quiz2")?,
        }))
    }

    /// `Add`：`Index(s) > 0` 视为已存在（**Id=0 的账号会被当成不存在**，保留 C# 怪行为）。
    ///
    /// 建号在同一事务里写 `account` + `account_protection`，成功后同步内存表。
    ///
    /// # Errors
    /// 事务失败时返回错误（C# 侧回滚并返回 0）。
    pub async fn add(&mut self, rec: &AccountRecord) -> Result<bool, StorageError> {
        if self.index(&rec.account).is_some_and(|id| id > 0) {
            return Ok(false);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StorageError::Backend(format!("建号事务开启失败：{e}")))?;
        let res = sqlx::query(
            "INSERT INTO account (Account, PassWord, PassFailCount, PassFailTime, ValidFrom, ValidUntil, Seconds, StopUntil, PayMode, State, CreateTime, ModifyTime, LastLoginTime) \
             VALUES (?, ?, 0, 0, NOW(), NOW(), 0, NOW(), 0, 0, UNIX_TIMESTAMP(), UNIX_TIMESTAMP(), 0)",
        )
        .bind(&rec.account)
        .bind(&rec.password)
        .execute(&mut *tx)
        .await;
        let new_id = match res {
            // MySQL AUTO_INCREMENT 是 u64；现网 Id 远小于 i32 上限，溢出即报错而非截断
            Ok(r) => i32::try_from(r.last_insert_id())
                .map_err(|_| StorageError::Corrupt("自增 Id 超出 i32".into()))?,
            Err(e) => {
                let _ = tx.rollback().await;
                return Err(StorageError::Backend(format!("创建账号失败：{e}")));
            }
        };
        // protection 行：C# 把 Phone/MobilePhone/Birthday 都写成 BirthDay，ADDRESS1/2 写空串
        let res2 = sqlx::query(
            "INSERT INTO account_protection (AccountId, UserName, IdCard, Birthday, Phone, MobilePhone, ADDRESS1, ADDRESS2, EMail, Quiz1, Answer1, Quiz2, Answer2) \
             VALUES (?, ?, '', '', '', '', '', '', '', '', '', ?, '')",
        )
        .bind(new_id)
        .bind(&rec.user_name)
        .bind(&rec.quiz2)
        .execute(&mut *tx)
        .await;
        if let Err(e) = res2 {
            let _ = tx.rollback().await;
            return Err(StorageError::Backend(format!("创建账号资料失败：{e}")));
        }
        tx.commit()
            .await
            .map_err(|e| StorageError::Backend(format!("建号事务提交失败：{e}")))?;
        self.map.entry(key(&rec.account)).or_insert(AccountQuick {
            account: rec.account.clone(),
            index: new_id,
        });
        Ok(true)
    }

    /// `ChanggePassword(accountId, newPassword)`（原拼写照抄）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub async fn change_password(
        &self,
        account_id: i32,
        new_password: &str,
    ) -> Result<bool, StorageError> {
        let r = sqlx::query(
            "UPDATE account SET PassWord = ?, PassFailCount = 0, PassFailTime = 0, ModifyTime = UNIX_TIMESTAMP() WHERE Id = ?",
        )
        .bind(new_password)
        .bind(account_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Backend(format!("改密失败：{e}")))?;
        Ok(r.rows_affected() > 0)
    }

    /// `UpdateRecord`：`ModifyTime/PassFailCount/PassFailTime`。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub async fn update_record(&self, rec: &AccountRecord) -> Result<bool, StorageError> {
        let r = sqlx::query(
            "UPDATE account SET ModifyTime=UNIX_TIMESTAMP(), PassFailCount=?, PassFailTime=? WHERE Id=?",
        )
        .bind(rec.error_count)
        .bind(rec.action_tick)
        .bind(rec.account_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Backend(format!("更新账号记录失败：{e}")))?;
        Ok(r.rows_affected() > 0)
    }

    /// `UpdateLoginRecord`：追加写 `LastLoginTime`。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub async fn update_login_record(&self, rec: &AccountRecord) -> Result<bool, StorageError> {
        let r = sqlx::query(
            "UPDATE account SET ModifyTime=UNIX_TIMESTAMP(), PassFailCount=?, PassFailTime=?, LastLoginTime=UNIX_TIMESTAMP() WHERE Id=?",
        )
        .bind(rec.error_count)
        .bind(rec.action_tick)
        .bind(rec.account_id)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Backend(format!("更新登录记录失败：{e}")))?;
        Ok(r.rows_affected() > 0)
    }

    /// `Update(nIndex, …)`：**保留 C# 怪行为** —— 越界判定是 `_accountMap.Count <= nIndex`，
    /// 其中 `nIndex` 实际是账号 **Id**。故 Id ≥ 账号数时静默跳过更新（返回 `Ok(false)`）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub async fn update(&self, index: i32, rec: &AccountRecord) -> Result<bool, StorageError> {
        if index < 0 {
            return Ok(false);
        }
        if self.map.len() <= usize::try_from(index).unwrap_or(usize::MAX) {
            return Ok(false); // C# 此处 return false（可能是把 Id 当位置用的笔误，行为等价保留）
        }
        self.update_record(rec).await
    }

    /// `UpdateAccount`：事务内同时更新 `account` 与 `account_protection`
    /// （注意 C# 把 `PassFailCount/PassFailTime/LastLoginTime` 重置为 0）。
    ///
    /// # Errors
    /// 事务失败时返回错误。
    pub async fn update_account(&self, id: i32, rec: &AccountRecord) -> Result<bool, StorageError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StorageError::Backend(format!("更新账号事务开启失败：{e}")))?;
        let r1 = sqlx::query(
            "UPDATE account SET PassWord = ?, PassFailCount = 0, PassFailTime = 0, ModifyTime = UNIX_TIMESTAMP(), LastLoginTime = 0 WHERE Id = ?",
        )
        .bind(&rec.password)
        .bind(id)
        .execute(&mut *tx)
        .await;
        if let Err(e) = r1 {
            let _ = tx.rollback().await;
            return Err(StorageError::Backend(format!("更新账号失败：{e}")));
        }
        let r2 = sqlx::query(
            "UPDATE account_protection SET UserName = ?, Quiz2 = ? WHERE AccountId = ?",
        )
        .bind(&rec.user_name)
        .bind(&rec.quiz2)
        .bind(id)
        .execute(&mut *tx)
        .await;
        if let Err(e) = r2 {
            let _ = tx.rollback().await;
            return Err(StorageError::Backend(format!("更新账号资料失败：{e}")));
        }
        tx.commit()
            .await
            .map_err(|e| StorageError::Backend(format!("更新账号事务提交失败：{e}")))?;
        Ok(true)
    }

    /// `Delete`：软删（`State=1`）并从内存表移除。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub async fn delete(&mut self, index: i32, account: &str) -> Result<bool, StorageError> {
        if index < 0 || self.map.len() <= usize::try_from(index).unwrap_or(usize::MAX) {
            return Ok(false);
        }
        let r =
            sqlx::query("UPDATE account SET State=1, ModifyTime=UNIX_TIMESTAMP() WHERE Account=?")
                .bind(account)
                .execute(&self.pool)
                .await
                .map_err(|e| StorageError::Backend(format!("删除账号失败：{e}")))?;
        if r.rows_affected() > 0 {
            self.map.remove(&key(account));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// `GetAccountPlayTime`：`SELECT Seconds FROM ACCOUNT WHERE Account=?`（查不到返回 0）。
    ///
    /// # Errors
    /// 查询失败时返回错误。
    pub async fn play_time(&self, account: &str) -> Result<i64, StorageError> {
        let row = sqlx::query("SELECT Seconds FROM ACCOUNT WHERE Account=?")
            .bind(account)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::Backend(format!("获取账号[{account}]游戏时间失败：{e}")))?;
        match row {
            None => Ok(0),
            Some(r) => Ok(opt_int_col(&r, "Seconds")?.unwrap_or(0)),
        }
    }

    /// `UpdateAccountPlayTime`。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub async fn update_play_time(&self, account: &str, seconds: i64) -> Result<(), StorageError> {
        sqlx::query("UPDATE ACCOUNT SET Seconds=? WHERE Account=?")
            .bind(seconds)
            .bind(account)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Backend(format!("更新账号[{account}]游戏时间失败：{e}")))?;
        Ok(())
    }
}
