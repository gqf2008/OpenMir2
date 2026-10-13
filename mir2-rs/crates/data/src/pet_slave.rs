//! 宝宝 / 召唤（slave）配置项（`Server.conf`）—— `GameSvrConf` 里宝宝/召唤相关字段的读侧移植。
//!
//! 移植源（行号均在本仓库 `src/` 内实测）：
//! - **读取点**：`src/Modules/SystemModule/Conf/ServerConf.cs`（`LoadConfig`，逐字段行号见下）；
//! - **默认值**：`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs`（构造函数初值，逐字段行号见下）；
//! - **ini 语义**：`src/OpenMir2/Common/ConfigFile.cs` 的 `ReadWrite*` 读侧
//!   （缺键/空值 ⇒ 默认值且 C# 会**写回**，Rust 只读不写回；整型 `Convert.ChangeType` 见下）。
//! - **数组元素类型**：`src/OpenMir2/Data/RecallMigic.cs`（`struct RecallMigic { nHumLevel; sMonName; nCount; nLevel; }`）。
//!
//! 盘点依据：`openmir2-m3-hero-pet-align` 的
//! [`M3⑨英雄宠物现状盘点报告-20261013.md`](../../../tests/parity/m3/M3⑨英雄宠物现状盘点报告-20261013.md) §1.C / §1.F。
//! 交付卡：`openmir2-m3-pet-slave-config`（M3⑨-实现 1）。
//!
//! ## 照搬的 oracle 缺陷（`BUG-REPLICA`，见各字段文档）
//!
//! 1. `ServerConf.cs:521` 的 `MasterDieMutinySpeed` **读的是 `MasterDieMutinyPower` 键**
//!    （紧邻 `:520` 的同名读取，复制粘贴遗留）⇒ 现网 `MasterDieMutinyPower=0` 时
//!    `MasterDieMutinySpeed` 也被读成 `0`（**不是**初值 `5`）。
//! 2. `ServerConf.cs:533` 的 `MagTammingHPRate` 读的键是 **`MagTammingTargetHPRate`**
//!    （现网 `Server.conf:784` 只有该名）⇒ 键 `MagTammingHPRate` 即使存在也不生效。
//!
//! ## 与 C# 的差异（刻意）
//!
//! - C# `ConfigFile` 对**缺键**会**写回** `Server.conf`；Rust 只读，不写回（内容冻结约束 + 对拍用夹具副本）。
//! - C# `Read<string>`（`WriteString(null)` 直接 return）下，`RecallMigic.sMonName` 缺键的读侧结果 = `null`；
//!   本模块把它表示为**空字符串**（`String::new()`），即 `is_empty()` ⟺ oracle `sMonName == null`。
//! - `ReadWriteByte` 在 C# 走 `Convert.ChangeType`（越界会抛异常）；与 `crates/envir/src/conf.rs` /
//!   本 crate `guild_castle.rs` 的既有约定一致，本模块按 `as u8` 截断（夹具语料不触发越界分支）。

use std::path::Path;

use crate::exps::IniFile;

/// `Grobal2.cs:23` `public const byte SlaveMaxLevel = 50;` —— 宝宝色表的等级上限
/// （`BaseObject.Base.cs:877-881` 的 `if (slaveExpLevel <= Grobal2.SlaveMaxLevel)`）。
pub const SLAVE_MAX_LEVEL: u8 = 50;

/// `src/M2Server/Magic/MagicManager.cs:22` `private const int DwRoyaltySec = 10 * 24 * 60 * 60;`
/// —— 召唤物默认叛变窗口（秒），`MagMakeSlave`/`MagMakeSinSuSlave`/`MagMakeAngelSlave` 传给 `MakeSlave`。
pub const DEFAULT_ROYALTY_SEC: i32 = 10 * 24 * 60 * 60;

/// `RecallMigic`（`src/OpenMir2/Data/RecallMigic.cs`）：召唤物等级映射表的一项。
///
/// `SkeletonArray` / `DragonArray` 均为 `RecallMigic[10]`（`GameSvrConf.cs:1757/1761`），
/// 用 `nHumLevel` 选怪、`sMonName`/`nCount`/`nLevel` 决定召唤结果（`MagicManager.cs:1269-1352`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallMigic {
    /// 触发召唤所需的人物等级（`ServerConf.cs:295/303`，键 `SkeletonHumLevel{i}` / `DragonHumLevel{i}`）。
    pub n_hum_level: i32,
    /// 怪物名（`ServerConf.cs:296/304`，键 `Skeleton{i}` / `Dragon{i}`，`[Names]`）。
    /// C# 初值为 `null`（`new RecallMigic[10]` 的 struct 默认）⇒ 本模块用空串表示。
    pub s_mon_name: String,
    /// 召唤数量（`ServerConf.cs:297/305`，键 `SkeletonCount{i}` / `DragonCount{i}`）。
    pub n_count: i32,
    /// 召唤怪等级（`ServerConf.cs:298/306`，键 `SkeletonLevel{i}` / `DragonLevel{i}`）。
    pub n_level: i32,
}

impl Default for RecallMigic {
    /// C# `new RecallMigic[10]` 的 struct 默认值（全 0 / `sMonName = null` ⇒ 空串）。
    fn default() -> Self {
        RecallMigic {
            n_hum_level: 0,
            s_mon_name: String::new(),
            n_count: 0,
            n_level: 0,
        }
    }
}

/// 宝宝 / 召唤配置项，逐字段对应 `GameSvrConf`。
///
/// 字段 → oracle 读取点（`ServerConf.cs`）→ 默认值（`GameSvrConf.cs`）：
///
/// | 字段 | `ServerConf.cs` 读取 | `GameSvrConf.cs` 初值 |
/// | --- | --- | --- |
/// | `skeleton` | `:104` `[Names] Skeleton` | `:1755` |
/// | `skeleton_count` | `:292` `[Setup] SkeletonCount` | `:1756` |
/// | `skeleton_array` | `:293-298` `SkeletonHumLevel{i}`/`Skeleton{i}`/`SkeletonCount{i}`/`SkeletonLevel{i}` | `:1757` `new RecallMigic[10]` |
/// | `dragon` | `:105` `[Names] Dragon` | `:1758` |
/// | `dragon1` | `:106` `[Names] Dragon1` | `:1759` |
/// | `dragon_count` | `:300` `[Setup] DragonCount` | `:1760` |
/// | `dragon_array` | `:301-306` `DragonHumLevel{i}`/`Dragon{i}`/`DragonCount{i}`/`DragonLevel{i}` | `:1761` `new RecallMigic[10]` |
/// | `angel` | `:107` `[Names] Angel` | `:1762` |
/// | `mon_up_lv_need_kill_base` | `:119` `[Setup] MonUpLvNeedKillBase` | `:1325` |
/// | `mon_up_lv_rate` | `:120` `[Setup] MonUpLvRate` | `:1326` |
/// | `mon_up_lv_need_kill_count` | `:121-123` `MonUpLvNeedKillCount{i}`（15 项） | `:1327` |
/// | `slave_color` | `:125-127` `SlaveColor{i}`（15 项，`ReadWriteByte`） | `:1328` |
/// | `spirit_mutiny` | `:515` `[Setup] SpiritMutiny` | `:1737` |
/// | `spirit_mutiny_time` | `:516` `[Setup] SpiritMutinyTime` | `:1738` |
/// | `master_die_mutiny` | `:518` `[Setup] MasterDieMutiny` | `:1740` |
/// | `master_die_mutiny_rate` | `:519` `[Setup] MasterDieMutinyRate` | `:1741` |
/// | `master_die_mutiny_power` | `:520` `[Setup] MasterDieMutinyPower` | `:1742` |
/// | `master_die_mutiny_speed` | `:521` **`[Setup] MasterDieMutinyPower`**（oracle 缺陷） | `:1743` |
/// | `bb_mon_auto_change_color` | `:522` `[Setup] BBMonAutoChangeColor` | `:1744` |
/// | `bb_mon_auto_change_color_time` | `:523` `[Setup] BBMonAutoChangeColorTime` | `:1745` |
/// | `mag_tamming_level` | `:531` `[Setup] MagTammingLevel` | `:1774` |
/// | `mag_tamming_target_level` | `:532` `[Setup] MagTammingTargetLevel` | `:1775` |
/// | `mag_tamming_hp_rate` | `:533` **`[Setup] MagTammingTargetHPRate`**（oracle 缺陷） | `:1776` |
/// | `mag_tamming_count` | `:534` `[Setup] MagTammingCount`（`ReadWriteByte`） | `:1777` |
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PetSlaveConfig {
    /// `[Names] Skeleton`（`ServerConf.cs:104`）。`GameSvrConf.cs:1755` 初值 `变异骷髅`。
    pub skeleton: String,
    /// `[Setup] SkeletonCount`（`ServerConf.cs:292`）。`GameSvrConf.cs:1756` 初值 `1`。
    pub skeleton_count: i32,
    /// `SkeletonArray`（`GameSvrConf.cs:1757` `new RecallMigic[10]`；逐项读见 `ServerConf.cs:293-298`）。
    pub skeleton_array: [RecallMigic; 10],
    /// `[Names] Dragon`（`ServerConf.cs:105`）。`GameSvrConf.cs:1758` 初值 `神兽`。
    pub dragon: String,
    /// `[Names] Dragon1`（`ServerConf.cs:106`）。`GameSvrConf.cs:1759` 初值 `神兽1`。
    pub dragon1: String,
    /// `[Setup] DragonCount`（`ServerConf.cs:300`）。`GameSvrConf.cs:1760` 初值 `1`。
    pub dragon_count: i32,
    /// `DragonArray`（`GameSvrConf.cs:1761` `new RecallMigic[10]`；逐项读见 `ServerConf.cs:301-306`）。
    pub dragon_array: [RecallMigic; 10],
    /// `[Names] Angel`（`ServerConf.cs:107`）。`GameSvrConf.cs:1762` 初值 `精灵`。
    pub angel: String,
    /// `[Setup] MonUpLvNeedKillBase`（`ServerConf.cs:119`）。`GameSvrConf.cs:1325` 初值 `100`。
    pub mon_up_lv_need_kill_base: i32,
    /// `[Setup] MonUpLvRate`（`ServerConf.cs:120`）。`GameSvrConf.cs:1326` 初值 `16`。
    pub mon_up_lv_rate: i32,
    /// `[Setup] MonUpLvNeedKillCount{i}`（`ServerConf.cs:121-123`，15 项）。
    /// `GameSvrConf.cs:1327` 初值 `{0,0,50,100,200,300,600,1200,2400,4800,9600,19200,38400,76800,153600}`。
    pub mon_up_lv_need_kill_count: [i32; 15],
    /// `[Setup] SlaveColor{i}`（`ServerConf.cs:125-127`，15 项，`ReadWriteByte`）。
    /// `GameSvrConf.cs:1328` 初值 `{0xFF,0xFE,0x93,0x9A,0xE5,0xA8,0xB4,0xFC,249,250,250,250,250,250,250}`。
    pub slave_color: [u8; 15],
    /// `[Setup] SpiritMutiny`（`ServerConf.cs:515`）。`GameSvrConf.cs:1737` 初值 `false`。
    pub spirit_mutiny: bool,
    /// `[Setup] SpiritMutinyTime`（`ServerConf.cs:516`）。`GameSvrConf.cs:1738` 初值 `30*60*1000`。
    pub spirit_mutiny_time: i32,
    /// `[Setup] MasterDieMutiny`（`ServerConf.cs:518`）。`GameSvrConf.cs:1740` 初值 `false`。
    pub master_die_mutiny: bool,
    /// `[Setup] MasterDieMutinyRate`（`ServerConf.cs:519`）。`GameSvrConf.cs:1741` 初值 `5`。
    pub master_die_mutiny_rate: i32,
    /// `[Setup] MasterDieMutinyPower`（`ServerConf.cs:520`）。`GameSvrConf.cs:1742` 初值 `10`。
    pub master_die_mutiny_power: i32,
    /// `ServerConf.cs:521` **错读 `MasterDieMutinyPower` 键**（oracle 缺陷，照搬）。
    /// `GameSvrConf.cs:1743` 初值 `5`，但被键 `MasterDieMutinyPower` 覆盖。
    pub master_die_mutiny_speed: i32,
    /// `[Setup] BBMonAutoChangeColor`（`ServerConf.cs:522`）。`GameSvrConf.cs:1744` 初值 `false`。
    pub bb_mon_auto_change_color: bool,
    /// `[Setup] BBMonAutoChangeColorTime`（`ServerConf.cs:523`）。`GameSvrConf.cs:1745` 初值 `3000`。
    pub bb_mon_auto_change_color_time: i32,
    /// `[Setup] MagTammingLevel`（`ServerConf.cs:531`）。`GameSvrConf.cs:1774` 初值 `50`。
    pub mag_tamming_level: i32,
    /// `[Setup] MagTammingTargetLevel`（`ServerConf.cs:532`）。`GameSvrConf.cs:1775` 初值 `10`。
    pub mag_tamming_target_level: i32,
    /// `ServerConf.cs:533` 读的键是 **`MagTammingTargetHPRate`**（oracle 缺陷，照搬）。
    /// `GameSvrConf.cs:1776` 初值 `100`。
    pub mag_tamming_hp_rate: i32,
    /// `[Setup] MagTammingCount`（`ServerConf.cs:534`，`ReadWriteByte`）。`GameSvrConf.cs:1777` 初值 `5`。
    pub mag_tamming_count: u8,
}

impl Default for PetSlaveConfig {
    /// `GameSvrConf` 构造函数的初值（行号见各字段文档与模块表）。**不带** `Server.conf` 覆盖。
    fn default() -> Self {
        PetSlaveConfig {
            skeleton: "变异骷髅".to_string(),
            skeleton_count: 1,
            skeleton_array: std::array::from_fn(|_| RecallMigic::default()),
            dragon: "神兽".to_string(),
            dragon1: "神兽1".to_string(),
            dragon_count: 1,
            dragon_array: std::array::from_fn(|_| RecallMigic::default()),
            angel: "精灵".to_string(),
            mon_up_lv_need_kill_base: 100,
            mon_up_lv_rate: 16,
            mon_up_lv_need_kill_count: [
                0, 0, 50, 100, 200, 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 76800, 153600,
            ],
            slave_color: [
                0xFF, 0xFE, 0x93, 0x9A, 0xE5, 0xA8, 0xB4, 0xFC, 249, 250, 250, 250, 250, 250, 250,
            ],
            spirit_mutiny: false,
            spirit_mutiny_time: 30 * 60 * 1000,
            master_die_mutiny: false,
            master_die_mutiny_rate: 5,
            master_die_mutiny_power: 10,
            master_die_mutiny_speed: 5,
            bb_mon_auto_change_color: false,
            bb_mon_auto_change_color_time: 3000,
            mag_tamming_level: 50,
            mag_tamming_target_level: 10,
            mag_tamming_hp_rate: 100,
            mag_tamming_count: 5,
        }
    }
}

impl PetSlaveConfig {
    /// 按 `ServerConf.cs` 的读取点取值（缺键/空值 ⇒ `GameSvrConf` 初值，不写回）。
    #[must_use]
    pub fn from_ini(ini: &IniFile) -> Self {
        let d = PetSlaveConfig::default();

        // `ServerConf.cs:293-298`：`SkeletonArray[i]` 四项逐项读；`sMonName` 缺键 ⇒ C# `null` ⇒ 本模块空串。
        let mut skeleton_array = d.skeleton_array.clone();
        for (i, m) in skeleton_array.iter_mut().enumerate() {
            m.n_hum_level =
                ini.read_integer("Setup", &format!("SkeletonHumLevel{i}"), m.n_hum_level);
            m.s_mon_name = ini
                .read_string("Names", &format!("Skeleton{i}"), "")
                .to_string();
            m.n_count = ini.read_integer("Setup", &format!("SkeletonCount{i}"), m.n_count);
            m.n_level = ini.read_integer("Setup", &format!("SkeletonLevel{i}"), m.n_level);
        }

        // `ServerConf.cs:301-306`：`DragonArray[i]` 同型。
        let mut dragon_array = d.dragon_array.clone();
        for (i, m) in dragon_array.iter_mut().enumerate() {
            m.n_hum_level = ini.read_integer("Setup", &format!("DragonHumLevel{i}"), m.n_hum_level);
            m.s_mon_name = ini
                .read_string("Names", &format!("Dragon{i}"), "")
                .to_string();
            m.n_count = ini.read_integer("Setup", &format!("DragonCount{i}"), m.n_count);
            m.n_level = ini.read_integer("Setup", &format!("DragonLevel{i}"), m.n_level);
        }

        // `ServerConf.cs:121-123`：`MonUpLvNeedKillCount` 15 项。
        let mut mon_up_lv_need_kill_count = d.mon_up_lv_need_kill_count;
        for (i, v) in mon_up_lv_need_kill_count.iter_mut().enumerate() {
            *v = ini.read_integer("Setup", &format!("MonUpLvNeedKillCount{i}"), *v);
        }

        // `ServerConf.cs:125-127`：`SlaveColor` 15 项（`ReadWriteByte`）。
        let mut slave_color = d.slave_color;
        for (i, v) in slave_color.iter_mut().enumerate() {
            *v = read_u8(ini, "Setup", &format!("SlaveColor{i}"), *v);
        }

        PetSlaveConfig {
            skeleton: ini
                .read_string("Names", "Skeleton", &d.skeleton)
                .to_string(),
            skeleton_count: ini.read_integer("Setup", "SkeletonCount", d.skeleton_count),
            skeleton_array,
            dragon: ini.read_string("Names", "Dragon", &d.dragon).to_string(),
            dragon1: ini.read_string("Names", "Dragon1", &d.dragon1).to_string(),
            dragon_count: ini.read_integer("Setup", "DragonCount", d.dragon_count),
            dragon_array,
            angel: ini.read_string("Names", "Angel", &d.angel).to_string(),
            mon_up_lv_need_kill_base: ini.read_integer(
                "Setup",
                "MonUpLvNeedKillBase",
                d.mon_up_lv_need_kill_base,
            ),
            mon_up_lv_rate: ini.read_integer("Setup", "MonUpLvRate", d.mon_up_lv_rate),
            mon_up_lv_need_kill_count,
            slave_color,
            spirit_mutiny: ini.read_bool("Setup", "SpiritMutiny", d.spirit_mutiny),
            spirit_mutiny_time: ini.read_integer("Setup", "SpiritMutinyTime", d.spirit_mutiny_time),
            master_die_mutiny: ini.read_bool("Setup", "MasterDieMutiny", d.master_die_mutiny),
            master_die_mutiny_rate: ini.read_integer(
                "Setup",
                "MasterDieMutinyRate",
                d.master_die_mutiny_rate,
            ),
            master_die_mutiny_power: ini.read_integer(
                "Setup",
                "MasterDieMutinyPower",
                d.master_die_mutiny_power,
            ),
            // oracle 缺陷：键写作 `MasterDieMutinyPower`（`ServerConf.cs:521`），照搬。
            master_die_mutiny_speed: ini.read_integer(
                "Setup",
                "MasterDieMutinyPower",
                d.master_die_mutiny_speed,
            ),
            bb_mon_auto_change_color: ini.read_bool(
                "Setup",
                "BBMonAutoChangeColor",
                d.bb_mon_auto_change_color,
            ),
            bb_mon_auto_change_color_time: ini.read_integer(
                "Setup",
                "BBMonAutoChangeColorTime",
                d.bb_mon_auto_change_color_time,
            ),
            mag_tamming_level: ini.read_integer("Setup", "MagTammingLevel", d.mag_tamming_level),
            mag_tamming_target_level: ini.read_integer(
                "Setup",
                "MagTammingTargetLevel",
                d.mag_tamming_target_level,
            ),
            // oracle 缺陷：键写作 `MagTammingTargetHPRate`（`ServerConf.cs:533`），照搬。
            mag_tamming_hp_rate: ini.read_integer(
                "Setup",
                "MagTammingTargetHPRate",
                d.mag_tamming_hp_rate,
            ),
            mag_tamming_count: read_u8(ini, "Setup", "MagTammingCount", d.mag_tamming_count),
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

    /// 缺键 ⇒ 全部回落 `GameSvrConf` 初值（不静默乱给）。
    #[test]
    fn missing_keys_fall_back_to_game_svr_conf_defaults() {
        let ini = IniFile::from_bytes(b"[Setup]\nUnrelated=1\n");
        let c = PetSlaveConfig::from_ini(&ini);
        assert_eq!(c, PetSlaveConfig::default());
    }

    /// oracle 缺陷照搬：`MasterDieMutinySpeed` 读 `MasterDieMutinyPower` 键（`ServerConf.cs:521`）。
    #[test]
    fn master_die_mutiny_speed_reads_power_key() {
        let c = PetSlaveConfig::from_bytes(
            b"[Setup]\nMasterDieMutinyPower=0\nMasterDieMutinySpeed=77\n",
        );
        assert_eq!(c.master_die_mutiny_power, 0, "读 MasterDieMutinyPower 键");
        assert_eq!(
            c.master_die_mutiny_speed, 0,
            "oracle 错读 MasterDieMutinyPower 键 ⇒ 键 MasterDieMutinySpeed 不生效"
        );
    }

    /// oracle 缺陷照搬：`MagTammingHPRate` 读 `MagTammingTargetHPRate` 键（`ServerConf.cs:533`）。
    #[test]
    fn mag_tamming_hp_rate_reads_target_hp_rate_key() {
        let c =
            PetSlaveConfig::from_bytes(b"[Setup]\nMagTammingTargetHPRate=7\nMagTammingHPRate=88\n");
        assert_eq!(c.mag_tamming_hp_rate, 7, "读 MagTammingTargetHPRate 键");
    }

    /// `SkeletonArray`/`DragonArray` 的 `sMonName` 缺键 ⇒ C# `null` ⇒ 本模块空串；
    /// 数值四项逐索引读取（`ServerConf.cs:293-306`）。
    #[test]
    fn arrays_read_per_index() {
        let mut src = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM（现网 Server.conf 形态）
        src.extend_from_slice(
            concat!(
                "[Names]\nDragon0=神兽\nDragon1=神兽\n",
                "[Setup]\n",
                "DragonHumLevel0=41\nDragonCount0=1\nDragonLevel0=4\n",
                "DragonHumLevel1=42\nDragonCount1=1\nDragonLevel1=5\n",
            )
            .as_bytes(),
        );
        let c = PetSlaveConfig::from_bytes(&src);
        assert_eq!(
            c.dragon_array[0],
            RecallMigic {
                n_hum_level: 41,
                s_mon_name: "神兽".to_string(),
                n_count: 1,
                n_level: 4,
            }
        );
        assert_eq!(c.dragon_array[1].n_hum_level, 42);
        assert_eq!(c.dragon_array[1].n_level, 5);
        // 未给键的索引仍是 struct 默认：全 0 / 空串。
        assert_eq!(c.dragon_array[2], RecallMigic::default());
        // 现网无 Skeleton{i} 键 ⇒ 全为空串（oracle null）。
        assert!(c.skeleton_array.iter().all(|m| m.s_mon_name.is_empty()));
    }

    /// `ReadWriteByte` 读侧：`MagTammingCount`（`byte`）与 `SlaveColor{i}`。
    #[test]
    fn byte_reads() {
        let c = PetSlaveConfig::from_bytes(b"[Setup]\nMagTammingCount=300\nSlaveColor0=254\n");
        assert_eq!(
            c.mag_tamming_count, 300u32 as u8,
            "as u8 截断（C# 会抛越界）"
        );
        assert_eq!(c.slave_color[0], 254);
    }

    /// 现网 `Server.conf`（UTF-8+BOM，`ConfigFile.cs:263` 的 BOM 探测）与初值不同处应被覆盖；
    /// 尤其 `MasterDieMutinySpeed` 随 `MasterDieMutinyPower=0` 被读成 `0`（缺陷照搬）。
    #[test]
    fn utf8_bom_deployed_values_are_parsed() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM
        bytes.extend_from_slice(
            concat!(
                "[Names]\nAngel=精灵\nDragon0=神兽\nDragon1=神兽\n",
                "[Setup]\n",
                "MonUpLvNeedKillBase=100\nMonUpLvRate=16\n",
                "MagTammingLevel=40\nMagTammingTargetLevel=1\nMagTammingTargetHPRate=100\n",
                "MagTammingCount=5\n",
                "SpiritMutiny=1\nSpiritMutinyTime=600000\n",
                "MasterDieMutiny=0\nMasterDieMutinyRate=10\nMasterDieMutinyPower=0\n",
                "BBMonAutoChangeColor=0\nBBMonAutoChangeColorTime=3000\n",
            )
            .as_bytes(),
        );
        let c = PetSlaveConfig::from_bytes(&bytes);
        assert_eq!(c.mag_tamming_level, 40, "现网覆盖初值 50");
        assert_eq!(c.mag_tamming_target_level, 1, "现网覆盖初值 10");
        assert_eq!(c.mag_tamming_hp_rate, 100);
        assert_eq!(c.mag_tamming_count, 5);
        assert!(c.spirit_mutiny, "SpiritMutiny=1");
        assert_eq!(c.spirit_mutiny_time, 600000, "现网覆盖初值 1800000");
        assert!(!c.master_die_mutiny, "MasterDieMutiny=0");
        assert_eq!(c.master_die_mutiny_rate, 10, "现网覆盖初值 5");
        assert_eq!(c.master_die_mutiny_power, 0, "现网覆盖初值 10");
        assert_eq!(
            c.master_die_mutiny_speed, 0,
            "oracle 错读 MasterDieMutinyPower ⇒ 覆盖初值 5"
        );
        assert!(!c.bb_mon_auto_change_color);
        assert_eq!(c.bb_mon_auto_change_color_time, 3000);
        // [Names] 段：`Dragon1=神兽` 覆盖初值 `神兽1`；`dragon`/`skeleton` 无键 ⇒ 初值。
        assert_eq!(c.dragon, "神兽");
        assert_eq!(c.dragon1, "神兽", "现网 Dragon1 覆盖初值 神兽1");
        assert_eq!(c.angel, "精灵");
        assert_eq!(c.skeleton, "变异骷髅");
    }
}
