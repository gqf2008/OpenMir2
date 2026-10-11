//! Rust 版 **DBSrv**（M1）：5100 面（SelGate ↔ DBSrv）+ 6000 面（GameSvr 取档/存档）经 sqlx 接 `mir2_db`。
//!
//! 与 C# `src/DBSrv/**` 一一对应：
//! - `Conf/AppConfigManager` + `SettingsModel` → [`config`]（缺关键项/连不上库都明确报错退出）；
//! - `DBShare` + `AppService.LoadServerInfo/LoadChrNameList` → [`route`]；
//! - `Storeages/DBSvr.Storage.MySQL` 的 `PlayRecordStorage`/`PlayDataStorage` → [`db`]（5100 面）+ [`playdata`]（6000 面）；
//! - `Services/Impl/UserService`（5100 监听、会话表、四个命令）→ [`server`] + [`handler`]；
//! - `Services/Impl/DataService`（6000 监听、`LoadHumanRcd`/`SaveHumanRcd`/`SaveHumanRcdEx`）→ [`data`]；
//! - `Storage/Impl/CacheStorageService`（内存缓存）+ `TimedService` 的缓存回写 → [`cache`] + [`data::cache_flush_task`]；
//! - `Services/Impl/ClientSession`（主动连 LoginSrv 5600 同步会话）→ [`loginsrv`]。
//!
//! **不在本进程范围内**：`MarketService`（5700，拍卖行）。它按 M1 计划 §5 的 S4b 可拆子片，
//! 尚未实现（C# 侧该面本身大半是空实现），已在 `docs/M1-S4b-DBSrv-6000-交付说明.md` 逐条登记。
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
mod log;
mod loginsrv;
mod playdata;
mod route;
mod server;
mod user;

use std::sync::Arc;

use anyhow::{Context, Result};
use mir2_storage::LegacyDsn;

#[tokio::main]
async fn main() -> Result<()> {
    // 配置路径：可选 argv[1] 覆盖（默认取可执行文件同目录，与 C# BaseDirectory 一致）
    let conf = std::env::args()
        .nth(1)
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
    let db = Arc::new(tokio::sync::Mutex::new(db));
    let cache = Arc::new(tokio::sync::Mutex::new(cache::CacheStore::default()));

    let (srv, rx) = server::Server::new(cfg, Arc::clone(&share), Arc::clone(&cache));
    log::info("服务已启动成功...");
    log::info("欢迎使用翎风系列游戏软件...");
    // 6000 `DataService`（GameSvr 取档/存档）+ TimedService 的 5 分钟缓存回写
    tokio::spawn(data::run(Arc::clone(&srv), Arc::clone(&db)));
    tokio::spawn(data::cache_flush_task(Arc::clone(&srv), Arc::clone(&db)));
    srv.run(rx, db).await
}
