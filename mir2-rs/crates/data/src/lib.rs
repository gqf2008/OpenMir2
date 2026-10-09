//! D 线数据层：`mir2_data` 四表加载 + Envir 数据文件解析。
//!
//! 移植源：
//! - `src/GameSrv/DB/MySqlDB.cs`（`stditems` / `monsters` / `magics` / `goldsales`）
//! - `src/GameSrv/DB/LocalDB.cs`（`LoadMonitems`，GBK 文本）
//! - `src/Modules/SystemModule/Conf/ExpsConf.cs` + `src/OpenMir2/Common/ConfigFile.cs`
//!
//! 硬约束：只读。不改表结构、不写数据（C# `ConfigFile` 有「缺键写回」行为，
//! Rust 侧刻意不实现写回，见各模块注释）。

pub mod exps;
pub mod loaders;
pub mod models;
pub mod monitems;

pub use loaders::DataLoaders;
pub use models::*;
