//! `mir2-mcp`：OpenMir2 的本地 MCP（Model Context Protocol）服务，rmcp 3.8.0 + stdio。
//!
//! 用法：
//! ```text
//! mir2-mcp [--root <dir>] [--registry <gm_registry.json>]
//! ```
//!
//! root 解析优先级：`--root` > env `MIR2_MCP_ROOT` > `E:/MirServer`。
//! 传输永远是**本地 stdio**，不监听端口；工具清单见 `docs/mcp-gm-server-20261101.md`。

mod catalog;
mod conf;
mod server;

use std::path::PathBuf;
use std::process::ExitCode;

use rmcp::transport::stdio;
use rmcp::ServiceExt;

use crate::conf::AppConfig;
use crate::server::Mir2Mcp;

const USAGE: &str = "\
mir2-mcp — OpenMir2 的本地 MCP（stdio）服务

用法:
  mir2-mcp [--root <目录>] [--registry <gm_registry.json>]

选项:
  --root <目录>         MirServer 根目录（含 M2GameSvr/Server.conf）；
                        缺省取 env MIR2_MCP_ROOT，再缺省 E:/MirServer
  --registry <文件>     gm_list 改为实读该 gm_registry.json（缺省用编译期嵌入副本）
  -h, --help            打印本帮助
  --version             打印版本

工具: gm_list / gm_help / conf_get / conf_keys / conf_set / gm_execute
传输: stdio（不监听任何端口）
";

enum Cli {
    Help,
    Version,
    Run(AppConfig),
}

fn main() -> ExitCode {
    let cli = match parse_args(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("mir2-mcp: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match cli {
        Cli::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Cli::Version => {
            println!("mir2-mcp {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Cli::Run(config) => run(config),
    }
}

#[tokio::main]
async fn run(config: AppConfig) -> ExitCode {
    let service = match Mir2Mcp::new(config).serve(stdio()).await {
        Ok(service) => service,
        Err(error) => {
            eprintln!("mir2-mcp: 启动失败：{error:?}");
            return ExitCode::FAILURE;
        }
    };
    match service.waiting().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mir2-mcp: 服务异常退出：{error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Cli, String> {
    let mut root: Option<PathBuf> = None;
    let mut registry: Option<PathBuf> = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Cli::Help),
            "--version" => return Ok(Cli::Version),
            "--root" => {
                root = Some(next_value(&mut args, "--root")?.into());
            }
            "--registry" => {
                registry = Some(next_value(&mut args, "--registry")?.into());
            }
            other if other.starts_with("--root=") => {
                root = Some(value_after_eq(other, "--root")?.into());
            }
            other if other.starts_with("--registry=") => {
                registry = Some(value_after_eq(other, "--registry")?.into());
            }
            other => return Err(format!("未知参数 `{other}`")),
        }
    }
    let mut config = AppConfig::from_env();
    if let Some(root) = root {
        config.root = root;
    }
    config.registry_path = registry;
    Ok(Cli::Run(config))
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    let value = args.next().ok_or_else(|| format!("{flag} 缺少参数值"))?;
    if value.is_empty() {
        return Err(format!("{flag} 的参数值为空"));
    }
    Ok(value)
}

fn value_after_eq(arg: &str, flag: &str) -> Result<String, String> {
    let value = &arg[flag.len() + 1..];
    if value.is_empty() {
        return Err(format!("{flag} 的参数值为空"));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, String> {
        parse_args(args.iter().map(ToString::to_string))
    }

    #[test]
    fn help_and_version() {
        assert!(matches!(parse(&["--help"]), Ok(Cli::Help)));
        assert!(matches!(parse(&["-h"]), Ok(Cli::Help)));
        assert!(matches!(parse(&["--version"]), Ok(Cli::Version)));
    }

    #[test]
    fn root_flag_wins_over_env_default() {
        let Ok(Cli::Run(config)) = parse(&["--root", "C:/tmp/fake-root"]) else {
            panic!("应解析为 Run");
        };
        assert_eq!(config.root, PathBuf::from("C:/tmp/fake-root"));

        let Ok(Cli::Run(config)) = parse(&["--root=C:/tmp/other"]) else {
            panic!("应解析为 Run");
        };
        assert_eq!(config.root, PathBuf::from("C:/tmp/other"));
    }

    #[test]
    fn registry_override_and_errors() {
        let Ok(Cli::Run(config)) = parse(&["--registry", "reg.json"]) else {
            panic!("应解析为 Run");
        };
        assert_eq!(config.registry_path, Some(PathBuf::from("reg.json")));

        assert!(parse(&["--root"]).is_err());
        assert!(parse(&["--root="]).is_err());
        assert!(parse(&["--bogus"]).is_err());
    }
}
