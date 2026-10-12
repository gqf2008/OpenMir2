//! 怪物模板表（`monsters`）的**加载段**——oracle `GameApp.Initialize()` 直连 MySQL 读 `monsters`
//! （`MySqlDB.LoadMonsterDB`，`src/GameSrv/DB/MySqlDB.cs:168`），并在入库后按 `MonsterInfo.Name`
//! 读 `Envir/MonItems/<name>.txt`（GBK，`LocalDB.LoadMonitems`，`LocalDB.cs:596`）挂 `ItemList`
//! （`MySqlDB.cs:226` 的 `monster.ItemList = null;` + `LoadMonitems(...)`）。
//!
//! 两半的分工与 oracle 一致：
//! - [`mir2_data::loaders::DataLoaders::load_monster_db`] = oracle 的 `select * from monsters`
//!   循环体（只读 DB）；
//! - [`mir2_data::monitems::attach_monitems`] = 同一循环体里的 `ItemList` 挂接段（只读 Envir）。
//!
//! 本模块把两半合成 oracle 的 `LoadMonsterDB` 整体，作为**生产装载入口**（像 [`crate::items`]
//! 之于 `stditems`）：供后续 `openmir2-m3-mongen-load-spawn`（刷怪）与
//! `openmir2-m3-drop-exp-wiring`（死亡掉落）消费。**本卡不接线到 `main.rs`**（接线归后续卡，
//! §0b：同一文件一次只动一路）。
//!
//! 失败口径（与 oracle 对齐，见 `attach_monitems` 文档）：
//! - DB 连不上 / `select` 失败 ⇒ `Err`（oracle `LoadMonsterDB` 里 `Open()` 失败返回 0，启动期
//!   记 WRN 后怪物表为空——本模块只把错误交给调用方，由调用方决定降级，**不伪造**空表）；
//! - 某个 `MonItems/<name>.txt` 存在但读不了 ⇒ `Err` fail-closed（oracle 不吞该异常）。

use std::path::Path;

use mir2_data::loaders::DataLoaders;
use mir2_data::models::MonsterInfo;
use mir2_data::monitems::{attach_monitems, AttachReport};
use mir2_storage::LegacyDsn;

/// `monsters` 装载 + MonItems 挂接的产物。
#[derive(Clone, Debug, Default)]
pub struct MonsterLoad {
    /// `load_monster_db` 的顺序表（与 `select * from monsters` 的行序一致）。
    pub monsters: Vec<MonsterInfo>,
    /// 挂接统计（`total` = `monsters.len()`）。
    pub report: AttachReport,
}

/// oracle `LoadMonsterDB` 的另一半：把已从 DB 载入的怪物表按名挂 `MonItems`。
///
/// `envir_dir` 是 **`Envir` 目录本身**（oracle `Path.Combine(BasePath, EnvirDir, "MonItems", ...)`），
/// 例如 `E:/MirServer/M2GameSvr/Envir`。
///
/// # Errors
/// 某个 `MonItems/<name>.txt` 存在但读不了时返回人话错误（fail-closed，不静默当「无掉落」）。
pub fn with_monitems(
    mut monsters: Vec<MonsterInfo>,
    envir_dir: &Path,
) -> Result<MonsterLoad, String> {
    let report = attach_monitems(&mut monsters, envir_dir)
        .map_err(|e| format!("挂接 MonItems 失败（envir={}）：{e}", envir_dir.display()))?;
    Ok(MonsterLoad { monsters, report })
}

/// 直连 MySQL 读 `monsters` 并挂 `Envir/MonItems`（oracle `LoadMonsterDB` 整体）。
///
/// `monster_power_rate` = `SystemShare.Config.MonsterPowerRate`（现网 `Server.conf` = 10）；
/// 与 stdout 日志无关，只影响怪物 HP/AC/DC 等的缩放。连接串走 [`LegacyDsn`]（与 db-srv/login-srv
/// 同一份解析，勿再手写一份）。
///
/// # Errors
/// 连接串解析失败、连库失败、`select * from monsters` 失败、或 MonItems 坏表时返回人话错误。
pub async fn load_monsters(
    conn: &str,
    envir_dir: &Path,
    monster_power_rate: i32,
) -> Result<MonsterLoad, String> {
    let dsn = LegacyDsn::parse(conn).map_err(|e| format!("ConnctionString 解析失败：{e}"))?;
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&dsn.to_sqlx_url())
        .await
        .map_err(|e| format!("连接 MySQL 失败（读 monsters）：{e}"))?;
    let monsters = DataLoaders::new(pool, monster_power_rate)
        .load_monster_db()
        .await
        .map_err(|e| format!("加载 monsters 失败：{e}"))?;
    with_monitems(monsters, envir_dir)
}
