//! res-svr 入口（薄壳）：解析命令行 → 装配/校验配置 → 起 HTTP 服务。
//!
//! 退出码：`0` 正常（含 `--help`）；`2` 配置/监听失败（fail-closed，打印人读错误，**不含 token**）。

use std::process::ExitCode;
use std::sync::Arc;

use res_svr::config::{self, Cli, Sources};
use res_svr::http::{self, AppState};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("res-svr: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = Cli::parse(&args)?;
    if cli.help {
        print_help();
        return Ok(());
    }
    let sources = Sources::collect(&cli)?;
    let cfg = config::build(&sources)?;
    if cfg.token.len() < config::TOKEN_RECOMMENDED_MIN_LEN {
        eprintln!(
            "res-svr: 警告：token 长度 {} 低于建议下限 {}（token 本体不打印）",
            cfg.token.len(),
            config::TOKEN_RECOMMENDED_MIN_LEN
        );
    }
    let state = Arc::new(AppState::new(cfg));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("创建 tokio 运行时失败：{error}"))?;
    runtime.block_on(http::serve(state))
}

fn print_help() {
    println!(
        "res-svr — 资产补齐下载服务（默认只绑回环，Bearer token 鉴权）\n\
         \n\
         用法：res-svr [--conf <路径>] [--bind <ip:port>]\n\
         \n\
         路由：\n\
         \x20 GET|HEAD /res/index.json        全量清单 {{\"files\":[{{path,size,sha256}}]}}\n\
         \x20 GET|HEAD /res/file/<path>       流式下载（支持单段 Range: bytes=a-b / bytes=a-）\n\
         \n\
         配置（优先级从高到低）：\n\
         \x20 --bind / RES_SVR_BIND / 配置文件 [res-svr] bind / 默认 {default_bind}\n\
         \x20 token：RES_SVR_TOKEN / 配置文件 [res-svr] token（**没有 --token：token 不进 argv**）\n\
         \x20 映射：RES_SVR_MAP_<名称> / 配置文件 [map] / 默认 Data、Map、Wav 三根\n\
         \x20 配置文件：--conf / RES_SVR_CONF / 可执行文件同目录 config.conf（存在才用）\n\
         \n\
         安全：所有请求必须带 Authorization: Bearer <token>（缺失/不符 ⇒ 403）；\n\
         \x20 路径归一化后必须仍在映射根内，越界/穿越/绝对路径/UNC/盘符 ⇒ 403；\n\
         \x20 审计日志只写 stderr，且任何字段都不包含 token。",
        default_bind = config::DEFAULT_BIND
    );
}
