//! Rust 版 **LoginSrv**（M1/A2）：顶住 **5500**（LoginGate → 本进程）与 **5600**
//! （DBSrv/GameSvr → 本进程），行为对齐 C# `src/LoginSrv/**`（只读参照）。
//!
//! 链路（与 `bins/login-gate`、`bins/sel-gate` 同形的内部帧）：
//! 6B `ServerDataPacket` 头 + MemoryPack `ServerDataMessage`（`crates/protocol::internal`，
//! 已由 C# oracle 向量字节级钉住）。`Data` 的载荷是客户端**原始帧** `#1…!`（网关不重编码）。
//!
//! 启动顺序与日志逐条对齐 C# `AppServer`/`AppService`（M1① 的"两侧日志时间线"）：
//! `[客服列表]` → `账号登陆服务[…]已启动.` → `账号数据服务[…]已启动.` → `正在连接SQL服务器...`
//! → `连接SQL服务器成功...` → `账号数据读取成功...[N]` → `服务已启动成功...`。
//!
//! **失败要响（M1④）**：缺配置键、缺 AddrTable/UserLimit、绑不上端口、连不上 MySQL
//! 一律打印明确错误并以非零码退出（缺配置/缺伴随文件/端口非法）；SQL 连接失败按
//! oracle fail-soft 语义只记日志继续跑（卡 `openmir2-login-srv-dsn-source`，A-6 偏离撤销）。

// 文档与日志逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

mod config;
mod gbk;
mod login_server;
mod session_server;
mod state;
mod wire;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mir2_storage::{AccountStore, LegacyDsn};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// C# Serilog 输出模板 `{Timestamp:HH:mm:ss.fff} [{Level:u3}] {Message:lj}` 的同形前缀。
#[allow(clippy::cast_possible_truncation)] // 时间取模，回绕即语义
pub fn log_line(level: &str, msg: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let ms = now.subsec_millis();
    let secs = now.as_secs();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    println!("{h:02}:{m:02}:{s:02}.{ms:03} [{level}] {msg}");
}

/// C# `AppServer.PrintUsage` 的启动横幅（原样照抄，保持日志形状一致）。
fn print_banner() {
    println!(
        r"    ___                           __  __   _          ____          
   / _ \   _ __     ___   _ __   |  \/  | (_)  _ __  |___ \         
  | | | | | '_ \   / _ \ | '_ \  | |\/| | | | | '__|   __) |        
  | |_| | | |_) | |  __/ | | | | | |  | | | | | |     / __/         
   \___/  | .__/   \___| |_| |_| |_|  |_| |_| |_|    |_____|        
          |_|                                                      
   _                       _           ____                        
  | |       ___     __ _  (_)  _ __   / ___|   __  __   __        
  | |      / _ \   / _` | | | | '_ \  \___ \  | '__| \ \ / /       
  | |___  | (_) | | (_| | | | | | | |  ___) | | |     \ V /        
  |_____|  \___/   \__, | |_| |_| |_| |____/  |_|      \_/         
                   |___/                                           
"
    );
}

/// 监听/广播分离（卡 `openmir2-login-srv-bind-addr`，reviewer-22 R①）：LoginSrv 的
/// 两个监听**不读 conf 地址字段**——生产可观测 oracle（同 conf `GateAddr=ServerAddr=
/// 10.10.0.102`，C# 照常监听）：5500 面 = `0.0.0.0`（Any）、5600 面 = `127.0.0.1`
/// （loopback，不把账号会话面暴露到网段；`ServerAddr.txt` whitelist 仍按对端 IP 生效）。
/// conf 的 `GateAddr`/`ServerAddr`/`MonAddr`/`RunAddr` 只作日志/广播数据，值不变。
fn listen_gate_ip() -> &'static str {
    "0.0.0.0"
}

fn listen_session_ip() -> &'static str {
    "127.0.0.1"
}

/// 起 5500/5600 两个监听（`AppService.StartAsync`：login.StartServer → masSoc.StartServer）。
async fn bind_listeners(cfg: &config::Config) -> std::io::Result<(TcpListener, TcpListener)> {
    Ok((
        TcpListener::bind((listen_gate_ip(), cfg.gate_port)).await?,
        TcpListener::bind((listen_session_ip(), cfg.server_port)).await?,
    ))
}

/// `TimedService.ExecuteAsync`（10ms 轮询 + 20s/10s 两个节拍）。
async fn timer_loop(share: std::sync::Arc<state::Share>) {
    let mut last_status = Instant::now();
    let mut last_check = Instant::now();
    let mut ticker = tokio::time::interval(Duration::from_millis(10));
    let test_server = share.cfg.enable_making_id;
    loop {
        ticker.tick().await;
        session_server::session_clear_kick(&share).await;
        session_server::session_clear_no_payment(&share, test_server).await;
        if last_status.elapsed() >= Duration::from_secs(20) {
            last_status = Instant::now();
            session_server::system_status(&share).await;
        }
        if last_check.elapsed() >= Duration::from_secs(10) {
            last_check = Instant::now();
            session_server::check_server_status(&share).await;
        }
    }
}

/// A2（卡 `openmir2-login-srv-dsn-source`，重切第三层修订 + 第四次重切决定性实验）：
/// 账号库 DSN = **dbsvr.conf 全套凭据**、仅覆写 `database` → 账号库。
///
/// 决定性实验（2026-10-29，本机 mysqld 127.0.0.1:3306）：
/// - A：`mysql -u root`（无口令）TCP ⇒ **成功**（dbsvr.conf pwd=〔空〕同判）；
/// - C：`mysql -u root -p123456` ⇒ **1045 Access denied (using password: YES)**
///   —— config.conf 的 `pwd=123456` 是陈旧值，「凭据沿用 config.conf」会把错口令
///   带进运行时（v2 的缺陷）。
///
/// `config.conf` 的 `[DataBase] ConnctionString` 整条为陈旧配置（host 指向不可达的
/// 10.10.0.199、pwd=123456 陈旧），**不参与**账号库连接；`database` 固定账号库
/// `mir2_account`（account 表仅在 mir2_account——information_schema 与 winplay0
/// 实证，mir2_db 无此表）。dbsvr.conf 缺失/不可解析 ⇒ 回退 config.conf 原值
/// （连接失败走 fail-soft，不退）。
fn authoritative_account_dsn(conf_path: &std::path::Path, own: LegacyDsn) -> LegacyDsn {
    let Some(root) = conf_path.parent().and_then(|p| p.parent()) else {
        return own;
    };
    let dbsvr_path = ["/DBServer/dbsvr.conf", "/DBSrv/dbsvr.conf"]
        .iter()
        .map(|rel| root.join(rel.trim_start_matches('/')))
        .find(|p| p.is_file());
    let Some(dbsvr_path) = dbsvr_path else {
        return own;
    };
    let text = std::fs::read_to_string(&dbsvr_path).unwrap_or_default();
    let Some(conn) = text
        .lines()
        .find(|l| {
            l.trim_start()
                .to_ascii_lowercase()
                .starts_with("connctionstring")
        })
        .and_then(|l| l.split_once('='))
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
    else {
        return own;
    };
    let Ok(dbsvr_dsn) = LegacyDsn::parse(conn) else {
        return own;
    };
    // 重切第三层修订：凭据沿用 config.conf（root/空口令＝生产 MySQL 实证可用）；
    // dbsvr.conf 存储的口令对本机 root 已失效（重切窗 1045 实证）。
    // database 固定账号库 mir2_account（account 表仅在 mir2_account 实证）。
    // dbsvr.conf 全套凭据（root/空口令＝实验 A 实证）+ 仅覆写账号库名。
    LegacyDsn {
        host: dbsvr_dsn.host,
        port: dbsvr_dsn.port,
        user: dbsvr_dsn.user,
        password: dbsvr_dsn.password,
        database: "mir2_account".into(),
    }
}

async fn run_inner(args: &[String]) -> Result<()> {
    let conf = args
        .first()
        .map_or_else(config::default_path, PathBuf::from);
    print_banner();
    log_line("INF", "正在启动服务器...");
    let cfg = config::load(&conf)?;
    log_line(
        "INF",
        &format!(
            "[客服列表] RouteCount={} Count={} [{}]",
            cfg.routes.len(),
            cfg.server_names.len(),
            cfg.server_names.join("|")
        ),
    );
    // C# `AppService.StartAsync` 的顺序：login.StartServer(5500) → masSoc.StartServer(5600)
    // → accountStorage.Initialization(SQL)。监听恒 0.0.0.0；GateAddr/ServerAddr 仅广播。
    let (gate_listener, session_listener) = bind_listeners(&cfg).await.with_context(|| {
        format!(
            "绑定账号服务 0.0.0.0:{}/127.0.0.1:{} 失败（端口被占？M1④：失败必须明确报错退出）；conf 广播地址 GateAddr={}/ServerAddr={} 仅下发，不参与绑定",
            cfg.gate_port, cfg.server_port, cfg.gate_addr, cfg.server_addr
        )
    })?;
    log_line(
        "INF",
        &format!(
            "账号登陆服务[GateAddr={}（仅广播）; 监听 0.0.0.0:{}]已启动.",
            cfg.gate_addr, cfg.gate_port
        ),
    );
    log_line(
        "INF",
        &format!(
            "账号数据服务[ServerAddr={}（仅广播）; 监听 127.0.0.1:{}]已启动.",
            cfg.server_addr, cfg.server_port
        ),
    );
    log_line("INF", "正在连接SQL服务器...");
    let own_dsn = LegacyDsn::parse(&cfg.conn_string)
        .with_context(|| "[错误] SQL 连接失败!请检查SQL设置...（连接串缺键）".to_string())?;
    // A2（卡 `openmir2-login-srv-dsn-source`）：DSN 权威源 = dbsvr.conf（host/port/凭据）
    // + 账号库 mir2_account；连接失败按 oracle fail-soft 只记日志继续（A-6 偏离撤销）。
    let dsn = authoritative_account_dsn(&conf, own_dsn);
    log_line(
        "INF",
        &format!(
            "账号库连接参数（权威源）：server={}:{} database={}",
            dsn.host, dsn.port, dsn.database
        ),
    );
    let (store, init_err) = AccountStore::init_or_empty(&dsn).await;
    if let Some(e) = &init_err {
        // 卡续作 ①：oracle 同款文本 + 底层 cause **同一条日志**（sqlx/DB 层文本随
        // StorageError::Backend 的消息一起输出，不吞任何一层）。
        log_line(
            "ERR",
            &format!("[错误] SQL 连接失败!请检查SQL设置...：{e:#}"),
        );
    } else {
        log_line("INF", "连接SQL服务器成功...");
        log_line("INF", &format!("账号数据读取成功...[{}]", store.count()));
    }
    log_line("INF", "服务已启动成功...");
    log_line("INF", "欢迎使用翎风系列游戏软件...");
    log_line("INF", "网站:http://www.gameofmir.com");
    log_line("INF", "论坛:http://bbs.gameofmir.com");

    let share = state::new_share(cfg, store);
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(login_server::run_gate_listener(
        std::sync::Arc::clone(&share),
        tx,
        gate_listener,
    ));
    tokio::spawn(login_server::run_dispatch(
        std::sync::Arc::clone(&share),
        rx,
    ));
    tokio::spawn(session_server::run_session_listener(
        std::sync::Arc::clone(&share),
        session_listener,
    ));
    tokio::spawn(timer_loop(std::sync::Arc::clone(&share)));

    tokio::signal::ctrl_c().await.ok();
    log_line("INF", "账号登陆服务已停止.");
    log_line("INF", "账号数据服务已关闭.");
    Ok(())
}

/// 旧二进制入口（`login-srv.exe`）：argv 语义与改造前一致（`env::args().skip(1)`）。
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

#[cfg(test)]
mod bind_addr_teeth {
    use super::*;

    /// A1/A3（卡 `openmir2-login-srv-bind-addr`，reviewer-22 R②）：conf
    /// `GateAddr=ServerAddr=10.10.0.102`（切流 S4 RED：字面绑定 os error 10049）时——
    /// 5500 面监听必须起在 `0.0.0.0`（oracle Any）、5600 面监听必须落在 `127.0.0.1`
    /// （oracle 生产实况 loopback）；conf 地址字段只进日志/下发，值不变。
    ///
    /// 反向牙（A2/A3 变异）：把 `bind_listeners` 改回按 conf 字面绑定
    /// （`cfg.gate_addr`/`cfg.server_addr`）⇒ 本机（无 10.10.0.102）bind 直接
    /// `Err(10049)` ⇒ expect 红；有该地址的机器上 local_addr 断言红。两条路都咬得住。
    #[test]
    fn a1_listen_binds_oracle_addrs_even_when_conf_addr_unbindable() {
        let cfg = config::Config {
            gate_addr: "10.10.0.102".into(),
            server_addr: "10.10.0.102".into(),
            gate_port: 0,
            server_port: 0,
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (g, s) = rt
            .block_on(bind_listeners(&cfg))
            .expect("conf 含不可绑地址时监听仍必须成功（监听/广播分离）");
        assert!(
            g.local_addr().unwrap().ip().is_unspecified(),
            "5500 面监听必须落在 0.0.0.0（oracle Any），实测 {:?}",
            g.local_addr()
        );
        assert_eq!(
            s.local_addr().unwrap().ip().to_string(),
            "127.0.0.1",
            "5600 面监听必须落在 127.0.0.1（oracle 生产实况 loopback）"
        );
    }
}
