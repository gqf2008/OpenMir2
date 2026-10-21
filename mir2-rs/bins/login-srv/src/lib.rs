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
//! 一律打印明确错误并以非零码退出（连不上库的偏离已登记 A-6）。

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

/// 监听地址：C# 默认 `"*"`（`IPAddress.Parse("*")` 会抛）；这里按 C#「监听 Any」的意图取 0.0.0.0。
fn bind_addr(addr: &str) -> String {
    if addr.trim().is_empty() || addr == "*" {
        "0.0.0.0".to_string()
    } else {
        addr.to_string()
    }
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
    // → accountStorage.Initialization(SQL)
    let gate_listener = TcpListener::bind((bind_addr(&cfg.gate_addr), cfg.gate_port))
        .await
        .with_context(|| {
            format!(
                "绑定账号登陆服务 {}:{} 失败（端口被占？M1④：失败必须明确报错退出）",
                cfg.gate_addr, cfg.gate_port
            )
        })?;
    log_line(
        "INF",
        &format!("账号登陆服务[{}:{}]已启动.", cfg.gate_addr, cfg.gate_port),
    );
    let session_listener = TcpListener::bind((bind_addr(&cfg.server_addr), cfg.server_port))
        .await
        .with_context(|| {
            format!(
                "绑定账号数据服务 {}:{} 失败（端口被占？M1④）",
                cfg.server_addr, cfg.server_port
            )
        })?;
    log_line(
        "INF",
        &format!(
            "账号数据服务[{}:{}]已启动.",
            cfg.server_addr, cfg.server_port
        ),
    );
    log_line("INF", "正在连接SQL服务器...");
    let dsn = LegacyDsn::parse(&cfg.conn_string)
        .with_context(|| "[错误] SQL 连接失败!请检查SQL设置...（连接串缺键）".to_string())?;
    let store = AccountStore::connect(&dsn)
        .await
        .context("[错误] SQL 连接失败!请检查SQL设置...（M1④/A-6：连不上库必须报错退出）")?;
    log_line("INF", "连接SQL服务器成功...");
    log_line("INF", &format!("账号数据读取成功...[{}]", store.count()));
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
