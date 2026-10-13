//! 组队 / 交易配置项（`Server.conf`）—— `GameSvrConf` 里组队/交易相关字段的读侧移植。
//!
//! 移植源（行号均在本仓库 `src/` 内实测）：
//! - **声明**：`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs`（逐字段行号见下）；
//! - **默认值**：同文件构造函数初值（逐字段行号见下）；
//! - **读取点**：`src/Modules/SystemModule/Conf/ServerConf.cs` 的 `LoadConfig`（逐字段行号见下）；
//! - **ini 语义**：`src/OpenMir2/Common/ConfigFile.cs` 的 `ReadWrite*` 读侧
//!   （缺键/空值 ⇒ 默认值；整型 `int.TryParse` 失败 ⇒ 默认值；`ReadBool` 认 `是`/`YES`/`1`/`TRUE`；
//!   BOM 探测见 `:263`，由 [`IniFile::from_bytes`] 承接）。
//!
//! 盘点依据：`openmir2-m3-party-trade-align` 的
//! [`M3⑦组队交易现状盘点报告-20261013.md`](../../../tests/parity/m3/M3⑦组队交易现状盘点报告-20261013.md) §1.N。
//!
//! ## 字段 → oracle 锚
//!
//! | 字段 | `GameSvrConf.cs` 声明 | `GameSvrConf.cs` 初值 | `ServerConf.cs` 读取 |
//! | --- | --- | --- | --- |
//! | `group_members_max` | `:238` `public byte` | `:1402` `= 10` | `:225` `ReadWriteByte("Setup", "GroupMembersMax", …)` |
//! | `try_deal_time` | `:521` `public int` | `:1515` `= 3000` | `:308` `ReadWriteInteger("Setup", "TryDealTime", …)` |
//! | `deal_ok_time` | `:525` `public int` | `:1516` `= 1000` | `:309` `ReadWriteInteger("Setup", "DealOKTime", …)` |
//! | `can_not_get_back_deal` | `:529` `public bool` | `:1517` `= true` | `:310` `ReadWriteBool("Setup", "CanNotGetBackDeal", …)` |
//! | `disable_deal` | `:533` `public bool` | `:1518` `= false` | `:311` `ReadWriteBool("Setup", "DisableDeal", …)` |
//! | `lock_deal_action` | `:447` `public bool` | `:1490` `= false` | `:571` `ReadWriteBool("Setup", "PasswordLockDealAction", …)` |
//!
//! ## 与 C# 的差异（刻意）
//!
//! - C# `ConfigFile` 对**缺键**会**写回** `Server.conf`；Rust 只读，不写回（内容冻结约束 + 对拍用夹具副本）。
//! - `ReadWriteByte` 在 C# 走 `Convert.ChangeType`（越界会抛异常）；本模块与
//!   [`crate::guild_castle`] 的既有约定一致，按 `as u8` 截断（夹具语料不触发该分支）。
//!
//! 交付卡：`openmir2-m3-party-load-config`（M3⑦-实现 1）。

use std::path::Path;

use crate::exps::IniFile;

/// `[Setup]` 里的组队 / 交易配置项，逐字段对应 `GameSvrConf`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartyConfig {
    /// `[Setup] GroupMembersMax`（`ServerConf.cs:225`，组内成员上限）。
    /// `GameSvrConf.cs:238` 声明 `byte`，初值 `:1402` `= 10`。
    pub group_members_max: u8,
    /// `[Setup] TryDealTime`（`ServerConf.cs:308`，交易发起间隔 ms）。
    /// `GameSvrConf.cs:521` 声明 `int`，初值 `:1515` `= 3000`（注释：3000 为 3 秒）。
    pub try_deal_time: i32,
    /// `[Setup] DealOKTime`（`ServerConf.cs:309`，确认交易时间 ms）。
    /// `GameSvrConf.cs:525` 声明 `int`，初值 `:1516` `= 1000`（注释：1000 为 1 秒）。
    pub deal_ok_time: i32,
    /// `[Setup] CanNotGetBackDeal`（`ServerConf.cs:310`，是否禁止取回交易物品）。
    /// `GameSvrConf.cs:529` 声明 `bool`，初值 `:1517` `= true`。
    pub can_not_get_back_deal: bool,
    /// `[Setup] DisableDeal`（`ServerConf.cs:311`，是否禁止交易）。
    /// `GameSvrConf.cs:533` 声明 `bool`，初值 `:1518` `= false`。
    pub disable_deal: bool,
    /// `[Setup] PasswordLockDealAction`（`ServerConf.cs:571` 读取键名，密码锁交易操作）。
    /// `GameSvrConf.cs:447` 声明 `bool`，初值 `:1490` `= false`。
    pub lock_deal_action: bool,
}

impl Default for PartyConfig {
    /// `GameSvrConf` 构造函数的初值（行号见各字段文档与模块表）。**不带** `Server.conf` 覆盖。
    fn default() -> Self {
        PartyConfig {
            group_members_max: 10,
            try_deal_time: 3000,
            deal_ok_time: 1000,
            can_not_get_back_deal: true,
            disable_deal: false,
            lock_deal_action: false,
        }
    }
}

impl PartyConfig {
    /// 按 `ServerConf.cs` 的读取点取值（缺键/空值 ⇒ `GameSvrConf` 初值，不写回）。
    #[must_use]
    pub fn from_ini(ini: &IniFile) -> Self {
        let d = PartyConfig::default();
        PartyConfig {
            // `ServerConf.cs:225` `ReadWriteByte`（byte 读侧按 `as u8` 截断）。
            group_members_max: read_u8(ini, "Setup", "GroupMembersMax", d.group_members_max),
            try_deal_time: ini.read_integer("Setup", "TryDealTime", d.try_deal_time),
            deal_ok_time: ini.read_integer("Setup", "DealOKTime", d.deal_ok_time),
            can_not_get_back_deal: ini.read_bool(
                "Setup",
                "CanNotGetBackDeal",
                d.can_not_get_back_deal,
            ),
            disable_deal: ini.read_bool("Setup", "DisableDeal", d.disable_deal),
            // 注意：oracle 的键名是 `PasswordLockDealAction`，不是字段名 `LockDealAction`（`ServerConf.cs:571`）。
            lock_deal_action: ini.read_bool("Setup", "PasswordLockDealAction", d.lock_deal_action),
        }
    }

    /// 从 `Server.conf` 字节加载（按 BOM 探测编码，见 [`IniFile::from_bytes`]）。
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self::from_ini(&IniFile::from_bytes(bytes))
    }

    /// 从文件路径加载（只读；不写回缺键）。
    ///
    /// # Errors
    /// 读文件失败 ⇒ 原样返回 [`std::io::Error`]。
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Ok(Self::from_bytes(&bytes))
    }
}

/// `ConfigFile.ReadWriteByte` 读侧（`ServerConf.cs` 的 `ReadWriteByte`）。
fn read_u8(ini: &IniFile, section: &str, key: &str, def: u8) -> u8 {
    ini.read_integer(section, key, i32::from(def)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_game_svr_conf() {
        let c = PartyConfig::default();
        assert_eq!(c.group_members_max, 10);
        assert_eq!(c.try_deal_time, 3000);
        assert_eq!(c.deal_ok_time, 1000);
        assert!(c.can_not_get_back_deal);
        assert!(!c.disable_deal);
        assert!(!c.lock_deal_action);
    }

    #[test]
    fn from_ini_reads_oracle_keys() {
        let bytes = b"[Setup]\nGroupMembersMax=20\nTryDealTime=4000\nDealOKTime=1500\nCanNotGetBackDeal=0\nDisableDeal=1\nPasswordLockDealAction=1\n";
        let c = PartyConfig::from_bytes(bytes);
        assert_eq!(c.group_members_max, 20);
        assert_eq!(c.try_deal_time, 4000);
        assert_eq!(c.deal_ok_time, 1500);
        assert!(!c.can_not_get_back_deal);
        assert!(c.disable_deal);
        assert!(c.lock_deal_action);
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let c = PartyConfig::from_bytes(b"[Setup]\nTryDealTime=\n");
        assert_eq!(c, PartyConfig::default());
    }

    #[test]
    fn lock_deal_action_uses_password_lock_key_only() {
        // 误写成字段名 `LockDealAction` 不该被读到（oracle 读的是 `PasswordLockDealAction`）。
        let c = PartyConfig::from_bytes(b"[Setup]\nLockDealAction=1\n");
        assert!(!c.lock_deal_action);
    }
}
