//! Rust 版 **DBSrv**（M1）：5100 面（SelGate ↔ DBSrv）+ 6000 面（GameSvr 取档/存档）经 sqlx 接 `mir2_db`。
//!
//! 与 C# `src/DBSrv/**` 一一对应：
//! - `Conf/AppConfigManager` + `SettingsModel` → [`config`]（缺关键项/连不上库都明确报错退出）；
//! - `DBShare` + `AppService.LoadServerInfo/LoadChrNameList` → [`route`]；
//! - `Storeages/DBSvr.Storage.MySQL` 的 `PlayRecordStorage`/`PlayDataStorage` → [`db`]（5100 面）+ [`playdata`]（6000 面）；
//! - `Services/Impl/UserService`（5100 监听、会话表、四个命令）→ [`server`] + [`handler`]；
//! - `Services/Impl/DataService`（6000 监听、`LoadHumanRcd`/`SaveHumanRcd`/`SaveHumanRcdEx`）→ [`data`]；
//! - `Services/Impl/MarketService`（5700 监听、拍卖行四个 ident）→ [`market`] + [`marketdata`]；
//! - `Storage/Impl/CacheStorageService`（内存缓存）+ `TimedService` 的缓存回写 → [`cache`] + [`data::cache_flush_task`]；
//! - `Services/Impl/ClientSession`（主动连 LoginSrv 5600 同步会话）→ [`loginsrv`]。
//!
//! C# `MarketService`（5700 拍卖行）**大半是空桩且不可达**（`MarketItem` 未注册 MemoryPack ⇒
//! 任何携带它的编解码都抛），逐条登记见 [`market`]/[`marketdata`] 模块头与
//! `docs/M1-S4b-DBSrv-6000-交付说明.md`。
//!
//! **失败要响**（M1 验收④）：缺配置 / 存储插件非 MySQL / 连不上 MySQL / 表缺列 / 端口占用
//! —— 全部明确报错并以非零码退出。

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

mod cache;
mod config;
mod data;
mod db;
mod gbk;
mod handler;
#[cfg(test)]
mod lb08_tests;
mod log;
mod loginsrv;
mod market;
mod marketdata;
mod playdata;
mod route;
mod server;
#[cfg(test)]
mod testsupport;
mod user;

use std::sync::Arc;

use anyhow::{Context, Result};
use mir2_storage::LegacyDsn;

async fn run_inner(args: &[String]) -> Result<()> {
    // 配置路径：可选 argv[1] 覆盖（默认取可执行文件同目录，与 C# BaseDirectory 一致）
    let conf = args
        .first()
        .map_or_else(config::default_path, std::path::PathBuf::from);
    let cfg = config::load(&conf)?;
    log::set_debug(cfg.show_debug);
    // 配置表文件与 C# 一样取 `AppContext.BaseDirectory`（= 可执行文件同目录）
    let base = conf.parent().map_or_else(
        || std::path::PathBuf::from("."),
        std::path::Path::to_path_buf,
    );
    log::info("正在读取基础配置信息...");
    let map_file = std::path::PathBuf::from(cfg.map_file.trimmed_text());
    let share = Arc::new(route::Share::load(&base, &map_file)?);
    log::info(&format!(
        "加载IP授权文件列表成功...[{}]",
        share.server_ips.len()
    ));
    log::info(&format!(
        "读取网关配置信息成功.[{}]",
        share
            .routes
            .iter()
            .map(route::GateRoute::gate_count)
            .sum::<usize>()
    ));

    // 数据层（M1④：连不上/表缺列即报错退出）
    let dsn = LegacyDsn::parse(&cfg.conn_string.text()).context("解析 ConnctionString 失败")?;
    let mut db = db::Db::connect(&dsn).await?;
    db.load_quick_lists().await?;
    log::info(&format!("[{}]数据存储初始化成功.", cfg.storeage.text()));
    // 方案 A（`design-dbsrv-connection-20261021.md` §4）：不再有 `Arc<Mutex<Db>>`。
    // `Db` 内部是共享池 + 索引 RwLock + 建角闸门，本身 `Send + Sync`。
    let db = Arc::new(db);
    let cache = Arc::new(tokio::sync::Mutex::new(cache::CacheStore::default()));

    let (srv, rx) = server::Server::new(cfg, Arc::clone(&share), Arc::clone(&cache));
    log::info("服务已启动成功...");
    log::info("欢迎使用翎风系列游戏软件...");
    // 6000 `DataService`（GameSvr 取档/存档）+ TimedService 的 5 分钟缓存回写
    tokio::spawn(data::run(Arc::clone(&srv), Arc::clone(&db)));
    tokio::spawn(data::cache_flush_task(Arc::clone(&srv), Arc::clone(&db)));
    // 5700 `MarketService`（拍卖行；C# 侧大半空桩，逐条见 `market.rs` 模块头）
    tokio::spawn(market::run(Arc::clone(&srv), Arc::clone(&db)));
    srv.run(rx, db).await
}

/// 旧二进制入口（`db-srv.exe`）：argv 语义与改造前一致（`env::args().skip(1)`）。
///
/// 单测/`mir2-server` 都走 [`run_with_args`]，本函数只负责取进程 argv。
#[must_use]
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run_with_args(&args)
}

/// 角色分派入口：`args` = **角色名之后**的剩余 argv（与旧二进制的 `env::args().skip(1)` 等长同序，
/// 含首个可选配置文件路径）。启动参数解析、日志、端口、配置读取全部沿用 [`run_inner`]，
/// 退出码口径与 `fn main() -> Result<()>` 相同（成功 0，失败 1）。
///
/// # Panics
///
/// 仅在 tokio 运行期构建失败时 panic（与 `#[tokio::main]` 展开的行为一致）。
#[must_use]
pub fn run_with_args(args: &[String]) -> i32 {
    use std::io::Write as _;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed building the Runtime");
    let result = rt.block_on(run_inner(args));
    let code = match result {
        Ok(()) => 0,
        Err(err) => {
            // 与 `fn main() -> Result<()>` 的 `Termination` 实现同形：`Error: {err:?}` 进 stderr。
            let _ = writeln!(std::io::stderr(), "Error: {err:?}");
            1
        }
    };
    // `process::exit` 跳过运行期退出清理，显式刷新，保持旧二进制"退出前输出可见"。
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    code
}
