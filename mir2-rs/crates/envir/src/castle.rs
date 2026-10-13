//! 攻城文件加载（`Envir/Castle/`）—— 第①片：`Castle/List.txt` + `Castle/<N>/SabukW.txt`
//! + `Castle/<N>/AttackSabukWall.txt` 的解析与**防御单元规格**（门 / 左中右墙 / 12 弓手 / 守卫）。
//!
//! 交付卡：`openmir2-m3-castle-file-load`（M3⑧-实现 6，第①片）。
//! 盘点依据：`mir2-rs/tests/parity/m3/M3⑧公会攻城现状盘点报告-20261013.md` §1.D / §1.F / §2。
//!
//! oracle 锚（行号按本仓库 `src/` 实读）：
//! - 名单读取 `CastleManager.LoadCastleList`：`src/M2Server/Castle/CastleManager.cs:162-185`
//!   （`Path.Combine(BasePath, Config.CastleFile)`，逐行 `Trim()`，空行跳过，`new UserCastle(dir)`）；
//! - 名单落盘 `CastleManager.SaveCastleList`：`CastleManager.cs:187-201`
//!   （**写的是下标 `i.ToString()`，不是目录名**）；
//! - 空列表兜底 `CastleManager.Initialize`：`CastleManager.cs:53-86`
//!   （造一个 `ConfigDir="0"` + `EnvirList=0151..0156` + `Save()` 的城堡）；
//! - `UserCastle` 构造默认：`src/M2Server/Castle/UserCastle.cs:23-59`
//!   （`HomeMap/HomeX/HomeY/sName/PalaceMap/SecretMap/WarRangeX/Y`，`Archers[12]` / `Guards[12]`）；
//! - `CastleConf.LoadConfig`（`[Setup]`/`[Defense]` 全键与默认值）：`src/M2Server/Castle/CastleConf.cs:15-107`；
//! - `LoadAttackSabukWall` / `SaveAttackSabukWall`：`UserCastle.cs:242-286` / `:291-309`；
//! - 守卫数组长度 12：`UserCastle.cs:49`（`CastleConst.MaxCalsteGuard = 4` 只是**雇佣上限**，
//!   见 `CastleConst.cs:11-14`，不是配置数组长度）。
//!
//! ## 建模说明（登记）
//!
//! - **实体挂图不在本片**：`UserCastle.Initialize`（`UserCastle.cs:118-227`）用
//!   `WorldEngine.RegenMonsterByName` 造门/墙/弓手/守卫并把 `WAbil.HP` 写成配置值；这依赖
//!   `CastleDoor` / `WallStructure` / `ArcherGuard` 三个实体类型（Rust `crates/world` 尚无，盘点 §2 #7/#22）
//!   ⇒ 留给 ⑧-7 / ⑧-8 片。本片交付**防御单元规格**（名字 / 坐标 / HP），HP 已按配置就位。
//! - **race 110/111 不缩放**已由 `crates/data/src/loaders.rs:259` 实现（非本片）。
//! - **不写 `SabukW.txt`**：`CastleConf.SaveConfig`（`CastleConf.cs:109+`）的「缺键写回 / `MainDoorOpen`・`HP`
//!   只在 `BaseObject != null` 时写 / 只在 `MapName` 归属本服时写」等条件属攻城状态机（⑧-7 / ⑧-8）
//!   ⇒ 本片只读 `SabukW.txt`。`List.txt` 与 `AttackSabukWall.txt` 的落盘按 oracle 实现。
//! - **日期**：`ReadWriteDate` 的缺键默认是 `DateTime.Now`（`ConfigFile.cs:105-113 / :155-168`），
//!   确定性用例无法复现 ⇒ 以**原始字符串** [`Option<String>`] 暴露，缺键 = `None`；本片不比较「Now」。
//! - **`Convert.ChangeType` 口径**：`ConfigFile.Read<ushort>`（`ConfigFile.cs:113-127`）对非法值抛异常；
//!   本片用 `parse::<u16>().unwrap_or(def)` 的宽松口径（与 `IniFile::read_integer` 已有的宽松口径一致），
//!   在此登记。
//! - **`AttackSabukWall` 的日期归一**：oracle `DateTime.TryParse` 命中后存 `DateTime`，
//!   落盘用 `DateTime.ToString()`（**culture 相关**，非确定）⇒ 本片刻意保留**引号内原始串**
//!   [`AttackSabukWallEntry::attack_date`]，只解析「是否可解析」不重排格式，并在 `README` 登记该偏差。

use std::path::{Path, PathBuf};

use mir2_data::exps::IniFile;
use mir2_shared::hutil32::{arrest_string_ex_ref, get_valid_str3, ArrestWrite};

use crate::error::EnvirError;
use crate::text::decode_text;

/// `LocalDB`/`StringList` 分词用的分隔符（`CastleConf` 走 `GetValidStr3`；`UserCastle.LoadAttackSabukWall` 用 `{' ', '\t'}`）。
pub const CASTLE_TEXT_SPLIT: [char; 2] = [' ', '\t'];

/// `CastleConst.SabukWFileName`（`CastleConst.cs:22`）。
pub const SABUK_W_FILE_NAME: &str = "SabukW.txt";
/// `CastleConst.AttackSabukWallList`（`CastleConst.cs:18`）。
pub const ATTACK_SABUK_WALL_FILE_NAME: &str = "AttackSabukWall.txt";
/// `CastleConst.MaxCastleArcher`（`CastleConst.cs:11`）——也是 `UserCastle.Archers` 数组长度。
pub const MAX_CASTLE_ARCHER: usize = 12;
/// `CastleConst.MaxCalsteGuard`（`CastleConst.cs:14`）——**雇佣上限**（`CastleOfficial.cs:274`），
/// 与 `UserCastle.Guards` 数组长度（12，`UserCastle.cs:49`）不同，别混用。
pub const MAX_CASTLE_GUARD: usize = 4;
/// `UserCastle.Guards = new ArcherUnit[12]`（`UserCastle.cs:49`）——`CastleConf.LoadConfig` 按此长度读 `Guard_{i+1}_*`。
pub const GUARD_ARRAY_LEN: usize = 12;

// ---- `CastleConf.LoadConfig` 的默认值（`CastleConf.cs:15-107`）+ `UserCastle` 构造默认（`UserCastle.cs:23-59`）。----

/// `[Setup] CastleName` 默认 = `UserCastle.sName` = `SystemShare.Config.CastleName`（`GameSvrConf.cs:338`）。
pub const DEFAULT_CASTLE_NAME: &str = "沙巴克";
/// `[Defense] CastleMap` 默认（`CastleConf.cs:44`）。
pub const DEFAULT_CASTLE_MAP: &str = "3";
/// `[Defense] CastleHomeMap` 默认 = `UserCastle.HomeMap`（`CastleConf.cs:45`）。
pub const DEFAULT_CASTLE_HOME_MAP: &str = "3";
/// `[Defense] CastleHomeX` 默认 = `UserCastle.HomeX`。
pub const DEFAULT_CASTLE_HOME_X: i32 = 644;
/// `[Defense] CastleHomeY` 默认 = `UserCastle.HomeY`。
pub const DEFAULT_CASTLE_HOME_Y: i32 = 290;
/// `[Defense] CastleWarRangeX` 默认 = `UserCastle.WarRangeX`。
pub const DEFAULT_CASTLE_WAR_RANGE_X: i32 = 100;
/// `[Defense] CastleWarRangeY` 默认 = `UserCastle.WarRangeY`。
pub const DEFAULT_CASTLE_WAR_RANGE_Y: i32 = 100;
/// `[Defense] CastlePlaceMap` 默认 = `UserCastle.PalaceMap`（`UserCastle.cs:35`）。
pub const DEFAULT_PALACE_MAP: &str = "0150";
/// `[Defense] CastleSecretMap` 默认 = `UserCastle.SecretMap`（`UserCastle.cs:36`）。
pub const DEFAULT_SECRET_MAP: &str = "D701";
/// `[Defense] CastlePalaceDoorX` 默认（`CastleConf.cs:56`）。
pub const DEFAULT_PALACE_DOOR_X: i32 = 631;
/// `[Defense] CastlePalaceDoorY` 默认（`CastleConf.cs:57`）。
pub const DEFAULT_PALACE_DOOR_Y: i32 = 274;

/// `[Defense] MainDoorX` 默认（`CastleConf.cs:59`）。
pub const DEFAULT_MAIN_DOOR_X: i16 = 672;
/// `[Defense] MainDoorY` 默认（`CastleConf.cs:60`）。
pub const DEFAULT_MAIN_DOOR_Y: i16 = 330;
/// `[Defense] MainDoorName` 默认（`CastleConf.cs:61`）。
pub const DEFAULT_MAIN_DOOR_NAME: &str = "MainDoor";
/// `[Defense] MainDoorOpen` 默认（`CastleConf.cs:62`，`true`）。
pub const DEFAULT_MAIN_DOOR_OPEN: bool = true;
/// `[Defense] MainDoorHP` 默认（`CastleConf.cs:63`）。
pub const DEFAULT_MAIN_DOOR_HP: u16 = 2000;

/// `[Defense] LeftWallX` 默认（`CastleConf.cs:65`）。
pub const DEFAULT_LEFT_WALL_X: i16 = 624;
/// `[Defense] LeftWallY` 默认（`CastleConf.cs:66`）。
pub const DEFAULT_LEFT_WALL_Y: i16 = 278;
/// `[Defense] LeftWallName` 默认（`CastleConf.cs:67`）。
pub const DEFAULT_LEFT_WALL_NAME: &str = "LeftWall";
/// `[Defense] CenterWallX` 默认（`CastleConf.cs:70`）。
pub const DEFAULT_CENTER_WALL_X: i16 = 627;
/// `[Defense] CenterWallY` 默认（`CastleConf.cs:71`）。
pub const DEFAULT_CENTER_WALL_Y: i16 = 278;
/// `[Defense] CenterWallName` 默认（`CastleConf.cs:72`）。
pub const DEFAULT_CENTER_WALL_NAME: &str = "CenterWall";
/// `[Defense] RightWallX` 默认（`CastleConf.cs:75`）。
pub const DEFAULT_RIGHT_WALL_X: i16 = 634;
/// `[Defense] RightWallY` 默认（`CastleConf.cs:76`）。
pub const DEFAULT_RIGHT_WALL_Y: i16 = 271;
/// `[Defense] RightWallName` 默认（`CastleConf.cs:77`）。
pub const DEFAULT_RIGHT_WALL_NAME: &str = "RightWall";
/// 三面墙的 `*WallHP` 默认（`CastleConf.cs:68/73/78`）。
pub const DEFAULT_WALL_HP: u16 = 2000;
/// `Archer_{i}_Name` 默认（`CastleConf.cs:85`）。
pub const DEFAULT_ARCHER_NAME: &str = "弓箭手";
/// `Guard_{i}_Name` 默认（`CastleConf.cs:96`）。
pub const DEFAULT_GUARD_NAME: &str = "守卫";
/// `Archer_{i}_HP` 默认（`CastleConf.cs:86`）。
pub const DEFAULT_ARCHER_HP: u16 = 2000;
/// `Guard_{i}_HP` 默认（`CastleConf.cs:97`）。
pub const DEFAULT_GUARD_HP: u16 = 2000;

/// 一个防御单元（门 / 墙 / 弓手 / 守卫）的配置规格。
///
/// oracle 侧类型是 `ArcherUnit`（`MainDoor`/`LeftWall`/…/`Archers[i]`/`Guards[i]`），
/// 本片只取 `Initialize` 真正用到的四个字段：`sName` / `nX` / `nY` / `nHP`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefenseUnit {
    /// `ArcherUnit.sName`（喂 `RegenMonsterByName`）。
    pub name: String,
    /// `ArcherUnit.nX`（`ReadWriteInt16`，`CastleConf.cs:59/65/70/75/82/93`）。
    pub x: i16,
    /// `ArcherUnit.nY`。
    pub y: i16,
    /// `ArcherUnit.nHP`（`ReadWriteUInt16`，`CastleConf.cs:63/68/73/78/86/97`）。
    /// `Initialize` 把它写进 `BaseObject.WAbil.HP`（`UserCastle.cs:135/148/161/174/198/215`）。
    pub hp: u16,
}

/// `Castle/<N>/SabukW.txt` 的 `[Setup]` + `[Defense]` 全量读侧。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SabukWConfig {
    // ---- [Setup]（`CastleConf.cs:17-23`）----
    /// `[Setup] CastleName`（缺键 = [`DEFAULT_CASTLE_NAME`]）。
    pub castle_name: String,
    /// `[Setup] OwnGuild`（缺键 = `""`）。
    pub own_guild: String,
    /// `[Setup] ChangeDate`（缺键 = oracle `DateTime.Now` ⇒ 本片 `None`）。
    pub change_date: Option<String>,
    /// `[Setup] WarDate`（缺键 = `DateTime.Now` ⇒ `None`）。
    pub war_date: Option<String>,
    /// `[Setup] IncomeToday`（缺键 = `DateTime.Now` ⇒ `None`）。
    pub income_today: Option<String>,
    /// `[Setup] TotalGold`（缺键 = 0）。
    pub total_gold: i32,
    /// `[Setup] TodayIncome`（缺键 = 0）。
    pub today_income: i32,

    // ---- [Defense]（`CastleConf.cs:24-107`）----
    /// `[Defense] CastleMapList`（逗号分隔；`GetValidStr3` 循环，遇空段 `break`）。
    pub envir_list: Vec<String>,
    /// `[Defense] CastleMap`。
    pub map_name: String,
    /// `[Defense] CastleHomeMap`。
    pub home_map: String,
    /// `[Defense] CastleHomeX`。
    pub home_x: i32,
    /// `[Defense] CastleHomeY`。
    pub home_y: i32,
    /// `[Defense] CastleWarRangeX`。
    pub war_range_x: i32,
    /// `[Defense] CastleWarRangeY`。
    pub war_range_y: i32,
    /// `[Defense] CastlePlaceMap`。
    pub palace_map: String,
    /// `[Defense] CastleSecretMap`。
    pub secret_map: String,
    /// `[Defense] CastlePalaceDoorX`。
    pub palace_door_x: i32,
    /// `[Defense] CastlePalaceDoorY`。
    pub palace_door_y: i32,

    /// `[Defense] MainDoor*` 规格。
    pub main_door: DefenseUnit,
    /// `[Defense] MainDoorOpen`（`ReadWriteBool`，`CastleConf.cs:62`）。
    pub main_door_open: bool,
    /// `[Defense] LeftWall*` 规格。
    pub left_wall: DefenseUnit,
    /// `[Defense] CenterWall*` 规格。
    pub center_wall: DefenseUnit,
    /// `[Defense] RightWall*` 规格。
    pub right_wall: DefenseUnit,
    /// `Archer_{i+1}_*`，长度恒 [`MAX_CASTLE_ARCHER`]。
    pub archers: Vec<DefenseUnit>,
    /// `Guard_{i+1}_*`，长度恒 [`GUARD_ARRAY_LEN`]（oracle 数组长度，不是雇佣上限 4）。
    pub guards: Vec<DefenseUnit>,
}

impl Default for SabukWConfig {
    fn default() -> Self {
        Self {
            castle_name: DEFAULT_CASTLE_NAME.to_string(),
            own_guild: String::new(),
            change_date: None,
            war_date: None,
            income_today: None,
            total_gold: 0,
            today_income: 0,
            envir_list: Vec::new(),
            map_name: DEFAULT_CASTLE_MAP.to_string(),
            home_map: DEFAULT_CASTLE_HOME_MAP.to_string(),
            home_x: DEFAULT_CASTLE_HOME_X,
            home_y: DEFAULT_CASTLE_HOME_Y,
            war_range_x: DEFAULT_CASTLE_WAR_RANGE_X,
            war_range_y: DEFAULT_CASTLE_WAR_RANGE_Y,
            palace_map: DEFAULT_PALACE_MAP.to_string(),
            secret_map: DEFAULT_SECRET_MAP.to_string(),
            palace_door_x: DEFAULT_PALACE_DOOR_X,
            palace_door_y: DEFAULT_PALACE_DOOR_Y,
            main_door: DefenseUnit {
                name: DEFAULT_MAIN_DOOR_NAME.to_string(),
                x: DEFAULT_MAIN_DOOR_X,
                y: DEFAULT_MAIN_DOOR_Y,
                hp: DEFAULT_MAIN_DOOR_HP,
            },
            main_door_open: DEFAULT_MAIN_DOOR_OPEN,
            left_wall: DefenseUnit {
                name: DEFAULT_LEFT_WALL_NAME.to_string(),
                x: DEFAULT_LEFT_WALL_X,
                y: DEFAULT_LEFT_WALL_Y,
                hp: DEFAULT_WALL_HP,
            },
            center_wall: DefenseUnit {
                name: DEFAULT_CENTER_WALL_NAME.to_string(),
                x: DEFAULT_CENTER_WALL_X,
                y: DEFAULT_CENTER_WALL_Y,
                hp: DEFAULT_WALL_HP,
            },
            right_wall: DefenseUnit {
                name: DEFAULT_RIGHT_WALL_NAME.to_string(),
                x: DEFAULT_RIGHT_WALL_X,
                y: DEFAULT_RIGHT_WALL_Y,
                hp: DEFAULT_WALL_HP,
            },
            archers: (0..MAX_CASTLE_ARCHER)
                .map(|_| DefenseUnit {
                    name: DEFAULT_ARCHER_NAME.to_string(),
                    x: 0,
                    y: 0,
                    hp: DEFAULT_ARCHER_HP,
                })
                .collect(),
            guards: (0..GUARD_ARRAY_LEN)
                .map(|_| DefenseUnit {
                    name: DEFAULT_GUARD_NAME.to_string(),
                    x: 0,
                    y: 0,
                    hp: DEFAULT_GUARD_HP,
                })
                .collect(),
        }
    }
}

impl SabukWConfig {
    /// `CastleConf.LoadConfig` 的读侧 1:1 移植（`CastleConf.cs:15-107`）。
    #[must_use]
    #[allow(clippy::field_reassign_with_default)]
    pub fn from_ini(ini: &IniFile) -> Self {
        let mut cfg = Self::default();
        // [Setup]
        cfg.castle_name = ini
            .read_string("Setup", "CastleName", DEFAULT_CASTLE_NAME)
            .to_string();
        cfg.own_guild = ini.read_string("Setup", "OwnGuild", "").to_string();
        cfg.change_date = optional_string(ini.read_string("Setup", "ChangeDate", ""));
        cfg.war_date = optional_string(ini.read_string("Setup", "WarDate", ""));
        cfg.income_today = optional_string(ini.read_string("Setup", "IncomeToday", ""));
        cfg.total_gold = ini.read_integer("Setup", "TotalGold", 0);
        cfg.today_income = ini.read_integer("Setup", "TodayIncome", 0);

        // [Defense]
        cfg.envir_list = parse_castle_map_list(ini.read_string("Defense", "CastleMapList", ""));
        cfg.map_name = ini
            .read_string("Defense", "CastleMap", DEFAULT_CASTLE_MAP)
            .to_string();
        cfg.home_map = ini
            .read_string("Defense", "CastleHomeMap", DEFAULT_CASTLE_HOME_MAP)
            .to_string();
        cfg.home_x = ini.read_integer("Defense", "CastleHomeX", DEFAULT_CASTLE_HOME_X);
        cfg.home_y = ini.read_integer("Defense", "CastleHomeY", DEFAULT_CASTLE_HOME_Y);
        cfg.war_range_x =
            ini.read_integer("Defense", "CastleWarRangeX", DEFAULT_CASTLE_WAR_RANGE_X);
        cfg.war_range_y =
            ini.read_integer("Defense", "CastleWarRangeY", DEFAULT_CASTLE_WAR_RANGE_Y);
        cfg.palace_map = ini
            .read_string("Defense", "CastlePlaceMap", DEFAULT_PALACE_MAP)
            .to_string();
        cfg.secret_map = ini
            .read_string("Defense", "CastleSecretMap", DEFAULT_SECRET_MAP)
            .to_string();
        cfg.palace_door_x = ini.read_integer("Defense", "CastlePalaceDoorX", DEFAULT_PALACE_DOOR_X);
        cfg.palace_door_y = ini.read_integer("Defense", "CastlePalaceDoorY", DEFAULT_PALACE_DOOR_Y);

        cfg.main_door = DefenseUnit {
            name: ini
                .read_string("Defense", "MainDoorName", DEFAULT_MAIN_DOOR_NAME)
                .to_string(),
            x: read_int16(ini, "Defense", "MainDoorX", DEFAULT_MAIN_DOOR_X),
            y: read_int16(ini, "Defense", "MainDoorY", DEFAULT_MAIN_DOOR_Y),
            hp: read_uint16(ini, "Defense", "MainDoorHP", DEFAULT_MAIN_DOOR_HP),
        };
        cfg.main_door_open = ini.read_bool("Defense", "MainDoorOpen", DEFAULT_MAIN_DOOR_OPEN);
        cfg.left_wall = DefenseUnit {
            name: ini
                .read_string("Defense", "LeftWallName", DEFAULT_LEFT_WALL_NAME)
                .to_string(),
            x: read_int16(ini, "Defense", "LeftWallX", DEFAULT_LEFT_WALL_X),
            y: read_int16(ini, "Defense", "LeftWallY", DEFAULT_LEFT_WALL_Y),
            hp: read_uint16(ini, "Defense", "LeftWallHP", DEFAULT_WALL_HP),
        };
        cfg.center_wall = DefenseUnit {
            name: ini
                .read_string("Defense", "CenterWallName", DEFAULT_CENTER_WALL_NAME)
                .to_string(),
            x: read_int16(ini, "Defense", "CenterWallX", DEFAULT_CENTER_WALL_X),
            y: read_int16(ini, "Defense", "CenterWallY", DEFAULT_CENTER_WALL_Y),
            hp: read_uint16(ini, "Defense", "CenterWallHP", DEFAULT_WALL_HP),
        };
        cfg.right_wall = DefenseUnit {
            name: ini
                .read_string("Defense", "RightWallName", DEFAULT_RIGHT_WALL_NAME)
                .to_string(),
            x: read_int16(ini, "Defense", "RightWallX", DEFAULT_RIGHT_WALL_X),
            y: read_int16(ini, "Defense", "RightWallY", DEFAULT_RIGHT_WALL_Y),
            hp: read_uint16(ini, "Defense", "RightWallHP", DEFAULT_WALL_HP),
        };

        // `for (int i = 0; i < userCastle.Archers.Length; i++)`（`CastleConf.cs:80-90`）。
        cfg.archers = (0..MAX_CASTLE_ARCHER)
            .map(|i| {
                let n = i + 1;
                DefenseUnit {
                    name: ini
                        .read_string("Defense", &format!("Archer_{n}_Name"), DEFAULT_ARCHER_NAME)
                        .to_string(),
                    x: read_int16(ini, "Defense", &format!("Archer_{n}_X"), 0),
                    y: read_int16(ini, "Defense", &format!("Archer_{n}_Y"), 0),
                    hp: read_uint16(ini, "Defense", &format!("Archer_{n}_HP"), DEFAULT_ARCHER_HP),
                }
            })
            .collect();
        // `for (int i = 0; i < userCastle.Guards.Length; i++)`（`CastleConf.cs:91-101`）——长度 12，不是 4。
        cfg.guards = (0..GUARD_ARRAY_LEN)
            .map(|i| {
                let n = i + 1;
                DefenseUnit {
                    name: ini
                        .read_string("Defense", &format!("Guard_{n}_Name"), DEFAULT_GUARD_NAME)
                        .to_string(),
                    x: read_int16(ini, "Defense", &format!("Guard_{n}_X"), 0),
                    y: read_int16(ini, "Defense", &format!("Guard_{n}_Y"), 0),
                    hp: read_uint16(ini, "Defense", &format!("Guard_{n}_HP"), DEFAULT_GUARD_HP),
                }
            })
            .collect();
        cfg
    }

    /// `UserCastle.Initialize` 会按顺序 `RegenMonsterByName` 的 1 门 + 3 墙 + 12 弓手 + 12 守卫
    /// （`UserCastle.cs:130-220`；弓手/守卫 `nHP <= 0` 跳过在调用方）。
    #[must_use]
    pub fn defense_units(&self) -> Vec<(&'static str, &DefenseUnit)> {
        let mut out: Vec<(&'static str, &DefenseUnit)> = vec![
            ("MainDoor", &self.main_door),
            ("LeftWall", &self.left_wall),
            ("CenterWall", &self.center_wall),
            ("RightWall", &self.right_wall),
        ];
        out.extend(self.archers.iter().map(|u| ("Archer", u)));
        out.extend(self.guards.iter().map(|u| ("Guard", u)));
        out
    }
}

/// `ReadWriteInt16` 读侧（`ConfigFile.cs:65-73`）：缺键/空值给默认；本片用 `i32` 读后截断。
fn read_int16(ini: &IniFile, section: &str, key: &str, def: i16) -> i16 {
    ini.read_integer(section, key, i32::from(def)) as i16
}

/// `ReadWriteUInt16` 读侧（`ConfigFile.cs:75-83`）：缺键/空值给默认；非法值按默认（登记）。
fn read_uint16(ini: &IniFile, section: &str, key: &str, def: u16) -> u16 {
    let v = ini.read_integer(section, key, i32::from(def));
    u16::try_from(v).unwrap_or(def)
}

/// `ReadWriteString` 返回空串 ⇒ oracle 视为「缺键」（`ConfigFile.cs:105-113` + `Read` 的空值分支）。
fn optional_string(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// `CastleConf.cs:26-41`：`CastleMapList` 按 `','` 用 `GetValidStr3` 循环切段，**空段即 `break`**。
#[must_use]
pub fn parse_castle_map_list(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s.to_string();
    while !rest.is_empty() {
        let (item, next) = get_valid_str3(&rest, &[',']);
        if item.is_empty() {
            break;
        }
        out.push(item);
        rest = next;
    }
    out
}

/// `CastleManager.LoadCastleList` 的逐行解析（`CastleManager.cs:162-185`）：
/// 逐行 `Trim()`，空行跳过；每行是一个城堡目录名（`new UserCastle(dir)` 的 `ConfigDir`）。
#[must_use]
pub fn parse_castle_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// `CastleManager.SaveCastleList`（`CastleManager.cs:187-201`）：**写的是下标 `i.ToString()`**，
/// 不是 `CastleList[i].ConfigDir`。返回 `StringList.SaveToFile` 的行（每行一条 + 结尾换行）。
#[must_use]
pub fn serialize_castle_list(count: usize) -> String {
    let mut out = String::new();
    for i in 0..count {
        out.push_str(&i.to_string());
        out.push('\n');
    }
    out
}

/// `Castle/<N>/AttackSabukWall.txt` 的一行（`UserCastle.LoadAttackSabukWall`，`UserCastle.cs:242-286`）。
///
/// 行格式为 `行会名<空白>"日期"`（`SaveAttackSabukWall` 用 7 个空格，`UserCastle.cs:305`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttackSabukWallEntry {
    /// `AttackerInfo.sGuildName`（`GetValidStr3` 的首个非空白 token）。
    pub guild_name: String,
    /// `AttackerInfo.AttackDate` 的**原始串**（`ArrestStringEx` 取引号内）。见模块头「日期归一」登记。
    pub attack_date: String,
    /// oracle `DateTime.TryParse(s20, out time)` 是否成功；失败时 oracle 用 `DateTime.Now`（本片不建模 Now）。
    pub attack_date_parsed: bool,
}

/// `LoadAttackSabukWall` 的逐行解析（`UserCastle.cs:242-286`）。
///
/// 只做「行 → `(guild_name, attack_date_raw)`」的**语法**解析；oracle 的
/// `SystemShare.GuildMgr.FindGuild(guildName) == null ⇒ continue` 依赖行会存活集，
/// 由调用方（`GuildStore`）过滤，本函数不做（寄存器见 §1.B）。
#[must_use]
pub fn parse_attack_sabuk_wall_line(line: &str) -> Option<AttackSabukWallEntry> {
    let (guild_name, rest) = get_valid_str3(line, &CASTLE_TEXT_SPLIT);
    if guild_name.is_empty() {
        return None;
    }
    // `ArrestStringEx(s20, "\"", "\"", ref s20)`：取引号内文本。
    let raw = match arrest_string_ex_ref(&rest, '"', '"') {
        (ArrestWrite::Value(v), _) => v,
        (ArrestWrite::Unchanged, _) => rest.clone(),
    };
    Some(AttackSabukWallEntry {
        guild_name,
        attack_date_parsed: is_parseable_datetime(&raw),
        attack_date: raw,
    })
}

/// `SaveAttackSabukWall`（`UserCastle.cs:291-309`）：`sGuildName + "       \"" + AttackDate + '"'`。
/// 返回逐行文本（每行一条 + 结尾换行）。
#[must_use]
pub fn serialize_attack_sabuk_wall(entries: &[AttackSabukWallEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!("{}       \"{}\"\n", e.guild_name, e.attack_date));
    }
    out
}

/// 轻量「`DateTime.TryParse` 是否成功」判定：只认 `yyyy-MM-dd` / `yyyy/MM/dd`（可带时间）；
/// 覆盖部署件与 oracle 落盘的常见形态；不追求 .NET culture 全集（见模块头登记）。
#[must_use]
pub fn is_parseable_datetime(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let (date, _time) = match s.split_once([' ', 'T']) {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    let norm = date.replace('/', "-");
    let mut it = norm.split('-');
    let (Some(y), Some(m), Some(d), None) = (it.next(), it.next(), it.next(), it.next()) else {
        return false;
    };
    let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i32>(), m.parse::<u32>(), d.parse::<u32>()) else {
        return false;
    };
    (1..=9999).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d)
}

// ---- 路径 + 文件加载（`MapLoader` 集成）。----

/// `Path.Combine(BasePath, Config.CastleDir, ConfigDir)`（`UserCastle.cs:52`）。
#[must_use]
pub fn castle_config_dir(base_path: &Path, castle_dir: &str, config_dir: &str) -> PathBuf {
    crate::paths::path_combine(&[base_path, Path::new(castle_dir), Path::new(config_dir)])
}

/// `Path.Combine(BasePath, Config.CastleFile)`（`CastleManager.cs:164`）。
#[must_use]
pub fn castle_list_path(base_path: &Path, castle_file: &str) -> PathBuf {
    crate::paths::path_combine(&[base_path, Path::new(castle_file)])
}

/// `Path.Combine(BasePath, Config.CastleDir, ConfigDir, "SabukW.txt")`（`UserCastle.cs:55`）。
#[must_use]
pub fn sabuk_w_path(base_path: &Path, castle_dir: &str, config_dir: &str) -> PathBuf {
    castle_config_dir(base_path, castle_dir, config_dir).join(SABUK_W_FILE_NAME)
}

/// `Path.Combine(BasePath, Config.CastleDir, ConfigDir, "AttackSabukWall.txt")`（`UserCastle.cs:251/301`）。
#[must_use]
pub fn attack_sabuk_wall_path(base_path: &Path, castle_dir: &str, config_dir: &str) -> PathBuf {
    castle_config_dir(base_path, castle_dir, config_dir).join(ATTACK_SABUK_WALL_FILE_NAME)
}

/// 从字节解析 `SabukW.txt`（`ConfigFile` 走 GB2312 + BOM 探测 ⇒ 复用 [`IniFile::from_bytes`]）。
#[must_use]
pub fn parse_sabuk_w_bytes(bytes: &[u8]) -> SabukWConfig {
    SabukWConfig::from_ini(&IniFile::from_bytes(bytes))
}

impl crate::registry::MapLoader {
    /// `CastleManager.LoadCastleList`（`CastleManager.cs:162-185`）：`File.Exists` 为假 ⇒ `Ok(None)`。
    ///
    /// # Errors
    /// 文件存在但读取失败。
    pub fn load_castle_list(&self, castle_file: &str) -> Result<Option<Vec<String>>, EnvirError> {
        let path = castle_list_path(&self.base_path, castle_file);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(EnvirError::Io { path, source }),
        };
        Ok(Some(parse_castle_list(&decode_text(&bytes))))
    }

    /// `UserCastle.Initialize` 的 `LoadConfig`（`UserCastle.cs:229-233`）：读 `Castle/<N>/SabukW.txt`。
    ///
    /// `IniFile` 是「缺键/空值/非法值都给默认」的宽松读取（`ConfigFile.ReadWrite*` 的读侧口径）
    /// ⇒ 本加载器唯一的失败面是文件 IO，不存在「解析失败」分支。
    ///
    /// # Errors
    /// 文件读取失败。
    pub fn load_sabuk_w(
        &self,
        castle_dir: &str,
        config_dir: &str,
    ) -> Result<SabukWConfig, EnvirError> {
        let path = sabuk_w_path(&self.base_path, castle_dir, config_dir);
        let bytes = std::fs::read(&path).map_err(|source| EnvirError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(parse_sabuk_w_bytes(&bytes))
    }

    /// `UserCastle.LoadAttackSabukWall`（`UserCastle.cs:242-286`）：文件不存在 ⇒ `Ok(None)`
    /// （oracle 直接 `return`，`AttackWarList` 保持空）。
    ///
    /// # Errors
    /// 文件存在但读取失败。
    pub fn load_attack_sabuk_wall(
        &self,
        castle_dir: &str,
        config_dir: &str,
    ) -> Result<Option<Vec<AttackSabukWallEntry>>, EnvirError> {
        let path = attack_sabuk_wall_path(&self.base_path, castle_dir, config_dir);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(EnvirError::Io { path, source }),
        };
        Ok(Some(
            decode_text(&bytes)
                .lines()
                .filter_map(parse_attack_sabuk_wall_line)
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **真数据夹具**：部署件 `E:/MirServer/M2GameSvr/Castle/0/SabukW.txt` 的 GBK 原始字节
    /// （sha256 `6021523b8bc4ada412588109c4417a9876dc30f62a6b086b18ec9cc230eb78ed`，1748 字节，CRLF）。
    /// 以 hex 形式随证据包入库（避免 GBK + CRLF 被 git 文本归一改字节），用例在此还原后解析。
    fn real_deployed_sabuk_w() -> Vec<u8> {
        let hex = std::fs::read_to_string(evidence_dir().join("fixtures/SabukW-0.hex"))
            .expect("证据夹具 fixtures/SabukW-0.hex");
        let digits: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        assert_eq!(digits.len() % 2, 0, "hex 夹具长度必须为偶");
        digits
            .chunks(2)
            .map(|c| {
                let hi = (c[0] as char).to_digit(16).expect("hex 高位") as u8;
                let lo = (c[1] as char).to_digit(16).expect("hex 低位") as u8;
                (hi << 4) | lo
            })
            .collect()
    }

    fn evidence_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/parity/m3/evidence-m3-castle-file-load-20261013")
    }

    /// 合成小样（覆盖 GBK 写盘 + `MapLoader` 落盘路径；字段值与部署件无关）。
    const SAMPLE_SABUK_W: &str = "[Setup]
CastleName=沙巴克
[Defense]
MainDoorHP=9999
";

    /// 部署件 `0/SabukW.txt` 的逐字段读数（`CastleConf.LoadConfig` 口径）。
    #[test]
    fn parses_deployed_sabuk_w() {
        let cfg = parse_sabuk_w_bytes(&real_deployed_sabuk_w());
        assert_eq!(cfg.castle_name, "沙巴克");
        assert_eq!(cfg.own_guild, "");
        assert_eq!(cfg.change_date.as_deref(), Some("1998-01-01"));
        assert_eq!(cfg.war_date.as_deref(), Some("1998-01-01"));
        assert_eq!(cfg.income_today.as_deref(), Some("2021-08-23 15:31:29"));
        assert_eq!(cfg.total_gold, 0);
        assert_eq!(cfg.today_income, 0);
        // `NextWar=0` 在 [Setup] 里但 `CastleConf` 从不读它 ⇒ 不进任何字段（无字段即证明）。
        assert_eq!(
            cfg.envir_list,
            ["0151", "0152", "0153", "0154", "0155", "0156"]
        );
        assert_eq!(cfg.map_name, "3");
        assert_eq!(cfg.home_map, "3");
        assert_eq!((cfg.home_x, cfg.home_y), (644, 290));
        assert_eq!((cfg.war_range_x, cfg.war_range_y), (100, 100));
        assert_eq!(cfg.palace_map, "0150");
        assert_eq!(cfg.secret_map, "D701");
        assert_eq!((cfg.palace_door_x, cfg.palace_door_y), (631, 274));
        assert_eq!(
            cfg.main_door,
            DefenseUnit {
                name: "沙巴克城门".to_string(),
                x: 672,
                y: 330,
                hp: 9999
            }
        );
        // `MainDoorOpen=0` ⇒ `ReadBool` 假（不是缺键默认 true）。
        assert!(!cfg.main_door_open);
        assert_eq!(cfg.left_wall.hp, 4999);
        assert_eq!(cfg.center_wall.hp, 4999);
        assert_eq!(cfg.right_wall.hp, 4999);
        // 弓手 12 个、守卫 12 个（`UserCastle.cs:48-49`）；部署件只填 Archer_1..12、Guard_1..4。
        assert_eq!(cfg.archers.len(), MAX_CASTLE_ARCHER);
        assert_eq!(cfg.guards.len(), GUARD_ARRAY_LEN);
        assert_eq!(cfg.archers[0].x, 662);
        assert_eq!(cfg.archers[0].name, "弓箭手");
        assert_eq!(cfg.archers[11].name, DEFAULT_ARCHER_NAME);
        assert_eq!(cfg.archers[11].hp, DEFAULT_ARCHER_HP);
        // 部署件 `Archer_1..12` 只写了 1 个；其余弓手取默认 HP 2000。
        assert_eq!(cfg.archers[0].hp, 2000);
    }

    /// **牙齿（改坏必红）**：守卫数组长度是 12（`UserCastle.cs:49`），不是
    /// `CastleConst.MaxCalsteGuard` 的 4；若有人「顺手」把它改成 4，本用例必红。
    #[test]
    fn guard_array_len_is_twelve_not_hire_cap() {
        assert_eq!(GUARD_ARRAY_LEN, 12);
        assert_eq!(MAX_CASTLE_GUARD, 4);
        let cfg = parse_sabuk_w_bytes(&real_deployed_sabuk_w());
        assert_eq!(cfg.guards.len(), 12);
        // 第 5 个守卫（部署件未配置）取默认。
        assert_eq!(cfg.guards[4].name, DEFAULT_GUARD_NAME);
        assert_eq!(cfg.guards[4].hp, DEFAULT_GUARD_HP);
    }

    /// 缺键默认值（`CastleConf.cs` 各 `ReadWrite*` 的第三参）。
    #[test]
    fn missing_keys_use_oracle_defaults() {
        let cfg = parse_sabuk_w_bytes(b"[Setup]\n[Defense]\n");
        let d = SabukWConfig::default();
        assert_eq!(cfg.castle_name, DEFAULT_CASTLE_NAME);
        assert_eq!(cfg.own_guild, "");
        assert_eq!(cfg.change_date, None);
        assert_eq!(cfg.map_name, "3");
        assert_eq!((cfg.home_x, cfg.home_y), (644, 290));
        assert_eq!(cfg.palace_map, "0150");
        assert_eq!(cfg.secret_map, "D701");
        assert_eq!((cfg.palace_door_x, cfg.palace_door_y), (631, 274));
        assert!(cfg.main_door_open); // 缺键默认 true（`CastleConf.cs:62`）
        assert_eq!(cfg.main_door, d.main_door);
        assert_eq!(cfg.left_wall, d.left_wall);
        assert_eq!(cfg.center_wall, d.center_wall);
        assert_eq!(cfg.right_wall, d.right_wall);
        assert_eq!(cfg.archers, d.archers);
        assert_eq!(cfg.guards, d.guards);
    }

    /// `defense_units` 顺序 = `UserCastle.Initialize` 的 `RegenMonsterByName` 顺序。
    #[test]
    fn defense_units_order_and_count() {
        let cfg = parse_sabuk_w_bytes(&real_deployed_sabuk_w());
        let units = cfg.defense_units();
        assert_eq!(units.len(), 1 + 3 + MAX_CASTLE_ARCHER + GUARD_ARRAY_LEN);
        assert_eq!(units[0].0, "MainDoor");
        assert_eq!(units[1].0, "LeftWall");
        assert_eq!(units[2].0, "CenterWall");
        assert_eq!(units[3].0, "RightWall");
        assert_eq!(units[4].0, "Archer");
        assert_eq!(units[4 + MAX_CASTLE_ARCHER].0, "Guard");
    }

    /// `[Setup] CastleName=` 显式空值 ⇒ `ReadWriteString` 的空值分支回默认（`Read` 空串 → 默认）。
    #[test]
    fn empty_string_value_falls_back_to_default() {
        let cfg = parse_sabuk_w_bytes(b"[Setup]\nCastleName=\n");
        assert_eq!(cfg.castle_name, DEFAULT_CASTLE_NAME);
    }

    /// `CastleMapList` 的 `GetValidStr3` 循环语义（`CastleConf.cs:26-41`）。
    #[test]
    fn castle_map_list_splitting() {
        assert_eq!(parse_castle_map_list(""), Vec::<String>::new());
        assert_eq!(parse_castle_map_list("0151"), vec!["0151"]);
        // 尾随逗号：最后一段为空 ⇒ 循环退出（与 oracle 同）。
        assert_eq!(parse_castle_map_list("0151,0152,"), vec!["0151", "0152"]);
        // 中间空段：`GetValidStr3` 会连跳多个分隔符 ⇒ "0151,,0152" 得两个段（与 oracle 同）。
        assert_eq!(parse_castle_map_list("0151,,0152"), vec!["0151", "0152"]);
        // 纯分隔符 ⇒ 首段为空 ⇒ break。
        assert_eq!(parse_castle_map_list(","), Vec::<String>::new());
    }

    /// `Castle/List.txt` 的逐行读取（`CastleManager.cs:162-185`）。
    #[test]
    fn castle_list_parse_and_serialize() {
        assert_eq!(parse_castle_list("0\n1\n"), vec!["0", "1"]);
        assert_eq!(parse_castle_list("  0  \r\n\r\n 1 \r\n"), vec!["0", "1"]);
        assert_eq!(parse_castle_list(""), Vec::<String>::new());
        // `SaveCastleList` 写的是下标，不是目录名（`CastleManager.cs:196`）。
        assert_eq!(serialize_castle_list(2), "0\n1\n");
        assert_eq!(serialize_castle_list(0), "");
    }

    /// `AttackSabukWall.txt` 的读写往返（`UserCastle.cs:242-309`）：7 个空格 + 引号日期。
    #[test]
    fn attack_sabuk_wall_roundtrip() {
        let line = "同盟会       \"2026-10-15\"";
        let e = parse_attack_sabuk_wall_line(line).expect("应解析");
        assert_eq!(e.guild_name, "同盟会");
        assert_eq!(e.attack_date, "2026-10-15");
        assert!(e.attack_date_parsed);
        assert_eq!(
            serialize_attack_sabuk_wall(&[e]),
            "同盟会       \"2026-10-15\"\n"
        );
        // 制表符分隔（`CASTLE_TEXT_SPLIT` 含 `\t`）。
        let e2 = parse_attack_sabuk_wall_line("乙会\t\"2026/10/16\"").unwrap();
        assert_eq!(e2.guild_name, "乙会");
        assert_eq!(e2.attack_date, "2026/10/16");
        assert!(e2.attack_date_parsed);
        // 空行 / 无行会名 ⇒ None。
        assert!(parse_attack_sabuk_wall_line("").is_none());
        assert!(parse_attack_sabuk_wall_line("   ").is_none());
    }

    /// `DateTime.TryParse` 轻量判定：非法日期 ⇒ `attack_date_parsed=false`（oracle 会退回 `Now`）。
    #[test]
    fn attack_date_parse_flag() {
        let e = parse_attack_sabuk_wall_line("甲会 \"不是日期\"").unwrap();
        assert!(!e.attack_date_parsed);
        // 保留原始串（本片刻意不重排格式，见模块头登记）。
        assert_eq!(e.attack_date, "不是日期");
        assert!(!is_parseable_datetime("2026-13-40"));
        assert!(is_parseable_datetime("2026-10-15 03:04:05"));
    }

    /// GBK 解码路径：非 BOM 字节按 GBK 解（`ConfigFile.Load` 的 `Encoding.GetEncoding("GB2312")`）。
    #[test]
    fn gbk_decoding_of_castle_name() {
        let (gbk, _, _) = encoding_rs::GBK.encode("[Setup]\nCastleName=沙巴克\n");
        let cfg = parse_sabuk_w_bytes(&gbk);
        assert_eq!(cfg.castle_name, "沙巴克");
    }

    /// `MapLoader` 的文件加载路径（临时目录里现写 GBK 文件）。
    #[test]
    fn loader_reads_files_from_disk() {
        let root = std::env::temp_dir().join(format!("m3-castle-{}", std::process::id()));
        let dir = castle_config_dir(&root, "Castle", "0");
        std::fs::create_dir_all(&dir).unwrap();
        let loader = crate::registry::MapLoader::new(&root, "Map", "Envir");
        // SabukW（GBK 写盘）
        let (gbk, _, _) = encoding_rs::GBK.encode(SAMPLE_SABUK_W);
        std::fs::write(sabuk_w_path(&root, "Castle", "0"), &gbk[..]).unwrap();
        let cfg = loader.load_sabuk_w("Castle", "0").unwrap();
        assert_eq!(cfg.main_door.hp, 9999);
        // List.txt
        std::fs::write(castle_list_path(&root, "Castle/List.txt"), b"0\n").unwrap();
        assert_eq!(
            loader.load_castle_list("Castle/List.txt").unwrap(),
            Some(vec!["0".to_string()])
        );
        // AttackSabukWall：缺文件 ⇒ None
        assert_eq!(loader.load_attack_sabuk_wall("Castle", "0").unwrap(), None);
        let (gbk_line, _, _) = encoding_rs::GBK.encode("同盟会       \"2026-10-15\"\n");
        std::fs::write(attack_sabuk_wall_path(&root, "Castle", "0"), &gbk_line[..]).unwrap();
        let list = loader
            .load_attack_sabuk_wall("Castle", "0")
            .unwrap()
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].guild_name, "同盟会");
        let _ = std::fs::remove_dir_all(&root);
    }
}
