//! D 线数据层：`mir2_data` 四表加载 + Envir 数据文件解析。
//!
//! 移植源：
//! - `src/GameSrv/DB/MySqlDB.cs`（`stditems` / `monsters` / `magics` / `goldsales`）
//! - `src/GameSrv/DB/LocalDB.cs`（`LoadMonitems`，GBK 文本）
//! - `src/Modules/SystemModule/Conf/ExpsConf.cs` + `src/OpenMir2/Common/ConfigFile.cs`
//!
//! 硬约束：只读。不改表结构、不写数据（C# `ConfigFile` 有「缺键写回」行为，
//! Rust 侧刻意不实现写回，见各模块注释）。
//!
//! 例外（唯一写盘面）：[`guild`] 是行会文件层（`GuildBase/GuildList.txt` +
//! `GuildBase/Guilds/<名>.txt` + `<名>.ini`）的加载/落盘移植，按 oracle 逐字节口径写这两个
//! **游戏数据文件**（不是 DB 表、不是表结构/脚本语法/数据格式变更）；其写盘规则照搬 oracle
//! （目标文件不存在则报错不新建）。

pub mod exps;
pub mod guild;
pub mod guild_castle;
pub mod loaders;
pub mod models;
pub mod monitems;
pub mod party;
pub mod pet_slave;

pub use loaders::DataLoaders;
pub use models::*;
