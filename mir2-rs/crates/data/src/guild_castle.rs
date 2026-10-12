//! 行会 / 攻城配置项（`Server.conf`）—— `GameSvrConf` 里行会/攻城相关字段的读侧移植。
//!
//! 移植源（行号均在本仓库 `src/` 内实测）：
//! - **读取点**：`src/Modules/SystemModule/Conf/ServerConf.cs`（`LoadConfig`，逐字段行号见下）；
//! - **默认值**：`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs`（构造函数初值，逐字段行号见下）；
//! - **ini 语义**：`src/OpenMir2/Common/ConfigFile.cs` 的 `ReadWrite*` 读侧
//!   （缺键/空值 ⇒ 默认值；整型 `int.TryParse` 失败 ⇒ 默认值；BOM 探测见 `:263`）。
//!
//! 盘点依据：`openmir2-m3-guild-castle-align` 的
//! [`M3⑧公会攻城现状盘点报告-20261013.md`](../../../tests/parity/m3/M3⑧公会攻城现状盘点报告-20261013.md) §1.K。
//!
//! ## 与 C# 的差异（刻意）
//!
//! - C# `ConfigFile` 对**缺键**会**写回** `Server.conf`；Rust 只读，不写回（内容冻结约束 + 对拍用夹具副本）。
//! - `CastleNameLen`：oracle 里**只有声明**（`GameSvrConf.cs:792`）、`ServerConf.LoadConfig` **无任何读取点**
//!   ⇒ 即使现网 `Server.conf:876 CastleNameLen=16`，oracle 值恒为 `0`。本模块照实给 `0`（见
//!   [`GuildCastleConfig::castle_name_len`]），**不**去读那个键。
//! - 段头文案（`GuildNotice`/`GuildWar`/`GuildAll`/`GuildMember`/`GuildMemberRank`/`GuildChief`）在
//!   `ServerConf.LoadConfig` 里同样**无读取点** ⇒ 恒为 `GameSvrConf` 初值。本模块照实暴露初值。
//! - `ReadWriteByte`/`ReadWriteInt16` 在 C# 走 `Convert.ChangeType`（越界会抛异常）；本模块与
//!   `crates/envir/src/conf.rs` 的既有约定一致，按 `as u8`/`as i16` 截断（夹具语料不触发该分支）。

use std::path::Path;

use crate::exps::IniFile;

/// `[Share]` / `[Names]` / `[Setup]` 里的行会 + 攻城配置项，逐字段对应 `GameSvrConf`。
///
/// 字段 → oracle 读取点（`ServerConf.cs`）→ 默认值（`GameSvrConf.cs`）：
///
/// | 字段 | `ServerConf.cs` 读取 | `GameSvrConf.cs` 初值 |
/// | --- | --- | --- |
/// | `guild_dir` | `:69` `[Share] GuildDir` | `:1289` |
/// | `guild_file` | `:70` `[Share] GuildFile` | `:1290` |
/// | `castle_dir` | `:73` `[Share] CastleDir` | `:1293` |
/// | `castle_file` | `:74` `[Share] CastleFile`（默认 = 读到的 `CastleDir` + `List.txt`） | `:1294` |
/// | `woma_horn` | `:102` `[Names] WomaHorn` | `:1318` |
/// | `zuma_piece` | `:103` `[Names] ZumaPiece` | `:1319` |
/// | `build_guild_price` | `:243` `[Setup] BuildGuild` | `:1448` |
/// | `guild_war_price` | `:245` `[Setup] GuildWarFee` | `:1449` |
/// | `guild_war_time` | `:644` `[Setup] GuildWarTime` | `:1447` |
/// | `guild_name_len` | `:499` `[Setup] GuildNameLen` | `:1722` |
/// | `guild_member_max_limit` | `:500` `[Setup] GuildMemberMaxLimit` | `:1721` |
/// | `guild_rank_name_len` | `:498` `[Setup] GuildRankNameLen` | `:1720` |
/// | `castle_name_len` | **无读取点** | `:792` 仅声明 ⇒ `0` |
/// | `start_castle_war_days` | `:638` `[Setup] StartCastleWarDays` | `:1442` |
/// | `start_castlewar_time` | `:639` `[Setup] StartCastlewarTime` | `:1443` |
/// | `show_castle_war_end_msg_time` | `:640` `[Setup] ShowCastleWarEndMsgTime` | `:1444` |
/// | `castle_war_time` | `:642` `[Setup] CastleWarTime` | `:1445` |
/// | `get_castle_time` | `:643` `[Setup] GetCastleTime` | `:1446` |
/// | `hire_guard_price` | `:246` `[Setup] HireGuard` | `:1790` |
/// | `hire_archer_price` | `:247` `[Setup] HireArcher` | `:1791` |
/// | `repair_door_price` | `:248` `[Setup] RepairDoor` | `:1794` |
/// | `repair_wall_price` | `:249` `[Setup] RepairWall` | `:1795` |
/// | `castle_gold_max` | `:251` `[Setup] CastleGoldMax` | `:1792` |
/// | `castle_one_day_gold` | `:252` `[Setup] CastleOneDayGold` | `:1793` |
/// | `castle_tax_rate` | `:259` `[Setup] CastleTaxRate` | `:1788` |
/// | `get_all_npc_tax` | `:260` `[Setup] CastleGetAllNpcTax` | `:1789` |
/// | `castle_member_price_rate` | `:250` `[Setup] CastleMemberPriceRate` | `:1796` |
/// | `castle_name` | `:253` `[Setup] CastleName` | `:1782` |
/// | `castle_home_map` | `:254` `[Setup] CastleHomeMap` | `:1783` |
/// | `castle_home_x` | `:255` `[Setup] CastleHomeX` | `:1784` |
/// | `castle_home_y` | `:256` `[Setup] CastleHomeY` | `:1785` |
/// | `castle_war_range_x` | `:257` `[Setup] CastleWarRangeX` | `:1786` |
/// | `castle_war_range_y` | `:258` `[Setup] CastleWarRangeY` | `:1787` |
/// | `guild_notice` / `guild_war_header` / … | **无读取点** | `:1850`-`:1855` |
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildCastleConfig {
    /// `[Share] GuildDir`（`ServerConf.cs:69`）。`GameSvrConf.cs:1289` 初值 `GuildBase\Guilds\`。
    pub guild_dir: String,
    /// `[Share] GuildFile`（`ServerConf.cs:70`）。`GameSvrConf.cs:1290` 初值 `GuildBase\GuildList.txt`。
    pub guild_file: String,
    /// `[Share] CastleDir`（`ServerConf.cs:73`）。`GameSvrConf.cs:1293` 初值 `Envir\Castle\`。
    pub castle_dir: String,
    /// `[Share] CastleFile`（`ServerConf.cs:74`）。默认值是 `CastleDir + "List.txt"`（用**已读到**的
    /// `CastleDir` 计算，见 [`GuildCastleConfig::from_ini`]）；`GameSvrConf.cs:1294` 初值 `Envir\Castle\List.txt`。
    pub castle_file: String,
    /// `[Names] WomaHorn`（`ServerConf.cs:102`，建会消耗品名）。`GameSvrConf.cs:1318` 初值 `沃玛号角`。
    pub woma_horn: String,
    /// `[Names] ZumaPiece`（`ServerConf.cs:103`，攻城申请消耗品名）。`GameSvrConf.cs:1319` 初值 `祖玛头像`。
    pub zuma_piece: String,
    /// `[Setup] BuildGuild`（`ServerConf.cs:243`，建会金币）。`GameSvrConf.cs:1448` 初值 `1000000`。
    pub build_guild_price: i32,
    /// `[Setup] GuildWarFee`（`ServerConf.cs:245`，宣战金币）。`GameSvrConf.cs:1449` 初值 `30000`。
    pub guild_war_price: i32,
    /// `[Setup] GuildWarTime`（`ServerConf.cs:644`，行会战时长 ms）。`GameSvrConf.cs:1447` 初值 `3*60*60*1000`。
    pub guild_war_time: i32,
    /// `[Setup] GuildNameLen`（`ServerConf.cs:499`，行会名最大长度）。`GameSvrConf.cs:1722` 初值 `16`。
    pub guild_name_len: u8,
    /// `[Setup] GuildMemberMaxLimit`（`ServerConf.cs:500`，行会成员上限）。`GameSvrConf.cs:1721` 初值 `200`。
    pub guild_member_max_limit: i32,
    /// `[Setup] GuildRankNameLen`（`ServerConf.cs:498`，封号最大长度）。`GameSvrConf.cs:1720` 初值 `16`。
    pub guild_rank_name_len: u8,
    /// `CastleNameLen`：oracle **无读取点**（`GameSvrConf.cs:792` 仅声明）⇒ 恒 `0`。
    pub castle_name_len: u8,
    /// `[Setup] StartCastleWarDays`（`ServerConf.cs:638`，申请日 +N 天）。`GameSvrConf.cs:1442` 初值 `4`。
    pub start_castle_war_days: u8,
    /// `[Setup] StartCastlewarTime`（`ServerConf.cs:639`，每日开战小时，墙钟门）。`GameSvrConf.cs:1443` 初值 `20`。
    pub start_castlewar_time: i32,
    /// `[Setup] ShowCastleWarEndMsgTime`（`ServerConf.cs:640`，结束前预告窗口 ms）。`GameSvrConf.cs:1444` 初值 `600000`。
    pub show_castle_war_end_msg_time: i32,
    /// `[Setup] CastleWarTime`（`ServerConf.cs:642`，攻城时长 ms）。`GameSvrConf.cs:1445` 初值 `3*60*60*1000`。
    pub castle_war_time: i32,
    /// `[Setup] GetCastleTime`（`ServerConf.cs:643`，可占领最短时间 ms）。`GameSvrConf.cs:1446` 初值 `600000`。
    pub get_castle_time: i32,
    /// `[Setup] HireGuard`（`ServerConf.cs:246`）。`GameSvrConf.cs:1790` 初值 `300000`。
    pub hire_guard_price: i32,
    /// `[Setup] HireArcher`（`ServerConf.cs:247`）。`GameSvrConf.cs:1791` 初值 `300000`。
    pub hire_archer_price: i32,
    /// `[Setup] RepairDoor`（`ServerConf.cs:248`）。`GameSvrConf.cs:1794` 初值 `2000000`。
    pub repair_door_price: i32,
    /// `[Setup] RepairWall`（`ServerConf.cs:249`）。`GameSvrConf.cs:1795` 初值 `500000`。
    pub repair_wall_price: i32,
    /// `[Setup] CastleGoldMax`（`ServerConf.cs:251`，金库上限）。`GameSvrConf.cs:1792` 初值 `10000000`。
    pub castle_gold_max: i32,
    /// `[Setup] CastleOneDayGold`（`ServerConf.cs:252`，日收入上限）。`GameSvrConf.cs:1793` 初值 `2000000`。
    pub castle_one_day_gold: i32,
    /// `[Setup] CastleTaxRate`（`ServerConf.cs:259`，商人税百分比）。`GameSvrConf.cs:1788` 初值 `5`。
    pub castle_tax_rate: i32,
    /// `[Setup] CastleGetAllNpcTax`（`ServerConf.cs:260`）。`GameSvrConf.cs:1789` 初值 `false`。
    pub get_all_npc_tax: bool,
    /// `[Setup] CastleMemberPriceRate`（`ServerConf.cs:250`，城堡成员价百分比）。`GameSvrConf.cs:1796` 初值 `80`。
    pub castle_member_price_rate: i32,
    /// `[Setup] CastleName`（`ServerConf.cs:253`）。`GameSvrConf.cs:1782` 初值 `沙巴克`。
    pub castle_name: String,
    /// `[Setup] CastleHomeMap`（`ServerConf.cs:254`）。`GameSvrConf.cs:1783` 初值 `3`。
    pub castle_home_map: String,
    /// `[Setup] CastleHomeX`（`ServerConf.cs:255`）。`GameSvrConf.cs:1784` 初值 `644`。
    pub castle_home_x: i16,
    /// `[Setup] CastleHomeY`（`ServerConf.cs:256`）。`GameSvrConf.cs:1785` 初值 `290`。
    pub castle_home_y: i16,
    /// `[Setup] CastleWarRangeX`（`ServerConf.cs:257`）。`GameSvrConf.cs:1786` 初值 `100`。
    pub castle_war_range_x: i16,
    /// `[Setup] CastleWarRangeY`（`ServerConf.cs:258`）。`GameSvrConf.cs:1787` 初值 `100`。
    pub castle_war_range_y: i16,
    /// 行会文件「公告」段头（`GameSvrConf.cs:1850`，`ServerConf` 不读 ⇒ 恒为初值 `公告`）。
    pub guild_notice: String,
    /// 行会文件「敌对行会」段头（`GameSvrConf.cs:1851`，恒为初值 `敌对行会`）。
    pub guild_war_header: String,
    /// 行会文件「联盟行会」段头（`GameSvrConf.cs:1852`，恒为初值 `联盟行会`）。
    pub guild_all: String,
    /// 行会文件「行会成员」段头（`GameSvrConf.cs:1853`，恒为初值 `行会成员`）。
    pub guild_member: String,
    /// 行会封号段头（`GameSvrConf.cs:1854`，恒为初值 `行会成员`）。
    pub guild_member_rank: String,
    /// 行会掌门封号（`GameSvrConf.cs:1855`，恒为初值 `掌门人`）。
    pub guild_chief: String,
}

impl Default for GuildCastleConfig {
    /// `GameSvrConf` 构造函数的初值（行号见各字段文档与模块表）。**不带** `Server.conf` 覆盖。
    fn default() -> Self {
        GuildCastleConfig {
            guild_dir: "GuildBase\\Guilds\\".to_string(),
            guild_file: "GuildBase\\GuildList.txt".to_string(),
            castle_dir: "Envir\\Castle\\".to_string(),
            castle_file: "Envir\\Castle\\List.txt".to_string(),
            woma_horn: "沃玛号角".to_string(),
            zuma_piece: "祖玛头像".to_string(),
            build_guild_price: 1_000_000,
            guild_war_price: 30_000,
            guild_war_time: 3 * 60 * 60 * 1000,
            guild_name_len: 16,
            guild_member_max_limit: 200,
            guild_rank_name_len: 16,
            castle_name_len: 0,
            start_castle_war_days: 4,
            start_castlewar_time: 20,
            show_castle_war_end_msg_time: 10 * 60 * 1000,
            castle_war_time: 3 * 60 * 60 * 1000,
            get_castle_time: 10 * 60 * 1000,
            hire_guard_price: 300_000,
            hire_archer_price: 300_000,
            repair_door_price: 2_000_000,
            repair_wall_price: 500_000,
            castle_gold_max: 10_000_000,
            castle_one_day_gold: 2_000_000,
            castle_tax_rate: 5,
            get_all_npc_tax: false,
            castle_member_price_rate: 80,
            castle_name: "沙巴克".to_string(),
            castle_home_map: "3".to_string(),
            castle_home_x: 644,
            castle_home_y: 290,
            castle_war_range_x: 100,
            castle_war_range_y: 100,
            guild_notice: "公告".to_string(),
            guild_war_header: "敌对行会".to_string(),
            guild_all: "联盟行会".to_string(),
            guild_member: "行会成员".to_string(),
            guild_member_rank: "行会成员".to_string(),
            guild_chief: "掌门人".to_string(),
        }
    }
}

impl GuildCastleConfig {
    /// 按 `ServerConf.cs` 的读取点取值（缺键/空值 ⇒ `GameSvrConf` 初值，不写回）。
    #[must_use]
    pub fn from_ini(ini: &IniFile) -> Self {
        let d = GuildCastleConfig::default();
        // `ServerConf.cs:73-74`：`CastleDir` 先读，`CastleFile` 的默认值用**已读到**的 `CastleDir` 计算。
        let castle_dir = ini
            .read_string("Share", "CastleDir", &d.castle_dir)
            .to_string();
        let castle_file_default = format!("{castle_dir}List.txt");
        GuildCastleConfig {
            guild_dir: ini
                .read_string("Share", "GuildDir", &d.guild_dir)
                .to_string(),
            guild_file: ini
                .read_string("Share", "GuildFile", &d.guild_file)
                .to_string(),
            castle_dir,
            castle_file: ini
                .read_string("Share", "CastleFile", &castle_file_default)
                .to_string(),
            woma_horn: ini
                .read_string("Names", "WomaHorn", &d.woma_horn)
                .to_string(),
            zuma_piece: ini
                .read_string("Names", "ZumaPiece", &d.zuma_piece)
                .to_string(),
            build_guild_price: ini.read_integer("Setup", "BuildGuild", d.build_guild_price),
            guild_war_price: ini.read_integer("Setup", "GuildWarFee", d.guild_war_price),
            guild_war_time: ini.read_integer("Setup", "GuildWarTime", d.guild_war_time),
            guild_name_len: read_u8(ini, "Setup", "GuildNameLen", d.guild_name_len),
            guild_member_max_limit: ini.read_integer(
                "Setup",
                "GuildMemberMaxLimit",
                d.guild_member_max_limit,
            ),
            guild_rank_name_len: read_u8(ini, "Setup", "GuildRankNameLen", d.guild_rank_name_len),
            // oracle 无读取点（见模块头）⇒ 恒为初值。
            castle_name_len: d.castle_name_len,
            start_castle_war_days: read_u8(
                ini,
                "Setup",
                "StartCastleWarDays",
                d.start_castle_war_days,
            ),
            start_castlewar_time: ini.read_integer(
                "Setup",
                "StartCastlewarTime",
                d.start_castlewar_time,
            ),
            show_castle_war_end_msg_time: ini.read_integer(
                "Setup",
                "ShowCastleWarEndMsgTime",
                d.show_castle_war_end_msg_time,
            ),
            castle_war_time: ini.read_integer("Setup", "CastleWarTime", d.castle_war_time),
            get_castle_time: ini.read_integer("Setup", "GetCastleTime", d.get_castle_time),
            hire_guard_price: ini.read_integer("Setup", "HireGuard", d.hire_guard_price),
            hire_archer_price: ini.read_integer("Setup", "HireArcher", d.hire_archer_price),
            repair_door_price: ini.read_integer("Setup", "RepairDoor", d.repair_door_price),
            repair_wall_price: ini.read_integer("Setup", "RepairWall", d.repair_wall_price),
            castle_gold_max: ini.read_integer("Setup", "CastleGoldMax", d.castle_gold_max),
            castle_one_day_gold: ini.read_integer(
                "Setup",
                "CastleOneDayGold",
                d.castle_one_day_gold,
            ),
            castle_tax_rate: ini.read_integer("Setup", "CastleTaxRate", d.castle_tax_rate),
            get_all_npc_tax: ini.read_bool("Setup", "CastleGetAllNpcTax", d.get_all_npc_tax),
            castle_member_price_rate: ini.read_integer(
                "Setup",
                "CastleMemberPriceRate",
                d.castle_member_price_rate,
            ),
            castle_name: ini
                .read_string("Setup", "CastleName", &d.castle_name)
                .to_string(),
            castle_home_map: ini
                .read_string("Setup", "CastleHomeMap", &d.castle_home_map)
                .to_string(),
            castle_home_x: read_i16(ini, "Setup", "CastleHomeX", d.castle_home_x),
            castle_home_y: read_i16(ini, "Setup", "CastleHomeY", d.castle_home_y),
            castle_war_range_x: read_i16(ini, "Setup", "CastleWarRangeX", d.castle_war_range_x),
            castle_war_range_y: read_i16(ini, "Setup", "CastleWarRangeY", d.castle_war_range_y),
            // 段头：oracle 无读取点 ⇒ 恒为初值（见模块头）。
            guild_notice: d.guild_notice,
            guild_war_header: d.guild_war_header,
            guild_all: d.guild_all,
            guild_member: d.guild_member,
            guild_member_rank: d.guild_member_rank,
            guild_chief: d.guild_chief,
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

/// `ConfigFile.ReadWriteInt16` 读侧（`ServerConf.cs` 的 `ReadWriteInt16`）。
fn read_i16(ini: &IniFile, section: &str, key: &str, def: i16) -> i16 {
    ini.read_integer(section, key, i32::from(def)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 缺键 ⇒ 全部回落 `GameSvrConf` 初值（不静默乱给）。
    #[test]
    fn missing_keys_fall_back_to_game_svr_conf_defaults() {
        let ini = IniFile::from_bytes(b"[Setup]\nUnrelated=1\n");
        let c = GuildCastleConfig::from_ini(&ini);
        assert_eq!(c, GuildCastleConfig::default());
    }

    /// `CastleNameLen` 在 oracle 里无读取点：现网文件有 `CastleNameLen=16` 也不生效。
    #[test]
    fn castle_name_len_is_never_read() {
        let c = GuildCastleConfig::from_bytes(b"[Setup]\nCastleNameLen=16\n");
        assert_eq!(c.castle_name_len, 0, "oracle ServerConf 不读该键");
    }

    /// `CastleFile` 的默认值用**已读到**的 `CastleDir` 计算（`ServerConf.cs:73-74`）。
    #[test]
    fn castle_file_default_uses_read_castle_dir() {
        let c = GuildCastleConfig::from_bytes(b"[Share]\nCastleDir=MyCastle\n");
        assert_eq!(c.castle_dir, "MyCastle");
        assert_eq!(c.castle_file, "MyCastleList.txt");
    }

    /// 现网 UTF-8+BOM 的 `Server.conf` 按 UTF-8 解码（`ConfigFile.cs:263`），
    /// 且 `[Setup]` 值覆盖初值。
    #[test]
    fn utf8_bom_server_conf_parses_deployed_values() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM
        bytes.extend_from_slice(
            concat!(
                "[Share]\nGuildDir=GuildBase/Guilds\nGuildFile=GuildBase/Guildlist.txt\n",
                "CastleDir=Castle\nCastleFile=Castle/List.txt\n",
                "[Names]\nWomaHorn=沃玛号角\nZumaPiece=祖玛头像\n",
                "[Setup]\nBuildGuild=1000000\nGuildWarFee=30000\nGuildWarTime=7200000\n",
                "GuildNameLen=15\nGuildMemberMaxLimit=500\nGuildRankNameLen=16\n",
                "CastleGoldMax=5000000\nCastleName=沙巴克\nCastleGetAllNpcTax=0\n",
            )
            .as_bytes(),
        );
        let c = GuildCastleConfig::from_bytes(&bytes);
        assert_eq!(c.guild_dir, "GuildBase/Guilds");
        assert_eq!(c.castle_file, "Castle/List.txt");
        assert_eq!(c.woma_horn, "沃玛号角");
        assert_eq!(c.zuma_piece, "祖玛头像");
        assert_eq!(c.guild_war_time, 7_200_000, "现网覆盖初值 10800000");
        assert_eq!(c.guild_name_len, 15);
        assert_eq!(c.guild_member_max_limit, 500);
        assert_eq!(c.castle_gold_max, 5_000_000);
        assert_eq!(c.castle_name, "沙巴克");
        assert!(!c.get_all_npc_tax);
    }

    /// `[Setup] CastleGetAllNpcTax` 的 bool 读侧（`是`/`YES`/`1`/`TRUE`）。
    #[test]
    fn get_all_npc_tax_bool_forms() {
        for truthy in ["1", "YES", "true"] {
            let src = format!("[Setup]\nCastleGetAllNpcTax={truthy}\n");
            let c = GuildCastleConfig::from_bytes(src.as_bytes());
            assert!(c.get_all_npc_tax, "{truthy} 应为真");
        }
        // 「是」的 GBK 字节（0xCA 0xC7）——无 BOM 的 ini 按 GB2312/GBK 解（见 `IniFile::from_bytes`）。
        let c = GuildCastleConfig::from_bytes(b"[Setup]\nCastleGetAllNpcTax=\xCA\xC7\n");
        assert!(c.get_all_npc_tax, "是 应为真");
        let c = GuildCastleConfig::from_bytes(b"[Setup]\nCastleGetAllNpcTax=0\n");
        assert!(!c.get_all_npc_tax);
    }
}
