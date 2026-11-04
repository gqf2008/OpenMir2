//! `mir2-mcp`：OpenMir2 的本地 MCP（Model Context Protocol）服务，rmcp 3.8.0。
//!
//! 用法：
//! ```text
//! mir2-mcp [--root <dir>] [--registry <gm_registry.json>]
//!          [--transport stdio|http] [--bind <ip:port>] [--token-file <path>]
//! ```
//!
//! root 解析优先级：`--root` > env `MIR2_MCP_ROOT` > `E:/MirServer`。
//! `gm_execute` 的 admin 地址：env `MIR2_MCP_ADMIN_ADDR` > `127.0.0.1:<Server.conf [Admin] AdminPort>`。
//!
//! 传输**缺省是本地 stdio**（不监听端口，行为与既有版本一字不变）。`--transport http`
//! 是 opt-in 的第二条传输：Streamable HTTP，**只接受回环 bind** 且**必须配 Bearer token**
//! （env `MIR2_MCP_HTTP_TOKEN` 或 `--token-file`），缺任何一项都在启动前拒绝（rc=2，不监听）。
//! 工具清单见 `docs/mcp-gm-server-20261101.md`。

mod admin;
mod catalog;
mod conf;
mod http;
mod server;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use rmcp::transport::stdio;
use rmcp::ServiceExt;

use crate::conf::AppConfig;
use crate::server::Mir2Mcp;

/// 并发连接上限缺省值（可用 `--max-connections` 覆盖）。
const DEFAULT_MAX_CONNECTIONS: usize = 64;
/// 并发连接上限允许区间。
const MAX_CONNECTIONS_RANGE: std::ops::RangeInclusive<usize> = 1..=4096;
/// 请求头读超时缺省值（毫秒，可用 `--header-timeout-ms` 覆盖）。
const DEFAULT_HEADER_TIMEOUT_MS: u64 = 10_000;
/// 请求头读超时允许区间（毫秒）。
const HEADER_TIMEOUT_MS_RANGE: std::ops::RangeInclusive<u64> = 1..=600_000;

const USAGE: &str = "\
mir2-mcp — OpenMir2 的本地 MCP 服务（stdio 为缺省传输；可选 opt-in Streamable HTTP）

用法:
  mir2-mcp [--root <目录>] [--registry <gm_registry.json>]
           [--transport stdio|http] [--bind <ip:port>] [--token-file <文件>]
           [--max-connections <n>] [--header-timeout-ms <ms>]

选项:
  --root <目录>         MirServer 根目录（含 M2GameSvr/Server.conf）；
                        缺省取 env MIR2_MCP_ROOT，再缺省 E:/MirServer
  --registry <文件>     gm_list 改为实读该 gm_registry.json（缺省用编译期嵌入副本）
  --transport <t>       stdio（缺省，不监听任何端口）或 http（opt-in Streamable HTTP）
  --bind <ip:port>      http 模式监听地址；缺省 127.0.0.1:7950；
                        只接受回环 IP 字面量（127.0.0.0/8 或 ::1），非回环拒绝启动；
                        端口写 0 表示由系统分配，实际端口打印在启动行里
  --token-file <文件>   http 模式 Bearer token 文件（内容去首尾空白）；
                        env MIR2_MCP_HTTP_TOKEN 亦可，--token-file 优先
  --max-connections <n> http 模式并发连接上限（缺省 64，1..=4096）；超出直接关闭新连接
  --header-timeout-ms <ms>
                        http 模式请求头读取超时（缺省 10000，1..=600000）；
                        窗口内未发完请求头的连接被关闭
  -h, --help            打印本帮助
  --version             打印版本

环境变量:
  MIR2_MCP_ROOT         MirServer 根目录（--root 压过它）
  MIR2_MCP_ADMIN_ADDR   game-svr 本机 admin 通道地址（缺省 127.0.0.1:<[Admin] AdminPort>）
  MIR2_MCP_HTTP_TOKEN   http 模式的 Bearer token（--token-file 压过它）

工具: gm_list / gm_help / conf_get / conf_keys / conf_set / gm_execute
传输: stdio（缺省，不监听端口）或 http（只监听回环，必须配 Bearer token）
";

/// `Cli::RunHttp` 的运行配置：`app` 与 stdio 共用，另加回环监听地址、Bearer token
/// 与连接层资源护栏（并发上限、请求头读超时）。
pub struct HttpConfig {
    pub app: AppConfig,
    pub bind: SocketAddr,
    pub token: String,
    /// 并发连接上限（超出的新连接立即关闭）。
    pub max_connections: usize,
    /// 请求头读取超时（窗口内未发完请求头的连接被关闭）。
    pub header_timeout: Duration,
}

/// 手写 Debug：token **永不以明文出现**（不打印、不进日志）。
impl std::fmt::Debug for HttpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpConfig")
            .field("app", &self.app)
            .field("bind", &self.bind)
            .field("token", &"<redacted>")
            .field("max_connections", &self.max_connections)
            .field("header_timeout", &self.header_timeout)
            .finish()
    }
}

#[derive(Debug)]
enum Cli {
    Help,
    Version,
    /// 缺省传输：stdio（既有行为）。
    Run(AppConfig),
    /// opt-in 传输：Streamable HTTP。
    RunHttp(HttpConfig),
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
        Cli::Run(config) => run_stdio(config),
        Cli::RunHttp(config) => run_http(config),
    }
}

#[tokio::main]
async fn run_stdio(config: AppConfig) -> ExitCode {
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

#[tokio::main]
async fn run_http(config: HttpConfig) -> ExitCode {
    match http::serve(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mir2-mcp: HTTP 传输启动失败：{error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Cli, String> {
    let env_token = std::env::var("MIR2_MCP_HTTP_TOKEN").ok();
    parse_args_with_env(args, env_token)
}

/// 与 [`parse_args`] 相同，但把 env token 显式注入（单测不依赖进程环境）。
fn parse_args_with_env(
    args: impl Iterator<Item = String>,
    env_token: Option<String>,
) -> Result<Cli, String> {
    let mut root: Option<PathBuf> = None;
    let mut registry: Option<PathBuf> = None;
    let mut transport: Option<String> = None;
    let mut bind: Option<String> = None;
    let mut token_file: Option<PathBuf> = None;
    let mut max_connections: Option<String> = None;
    let mut header_timeout_ms: Option<String> = None;
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
            "--transport" => {
                transport = Some(next_value(&mut args, "--transport")?);
            }
            "--bind" => {
                bind = Some(next_value(&mut args, "--bind")?);
            }
            "--token-file" => {
                token_file = Some(next_value(&mut args, "--token-file")?.into());
            }
            "--max-connections" => {
                max_connections = Some(next_value(&mut args, "--max-connections")?);
            }
            "--header-timeout-ms" => {
                header_timeout_ms = Some(next_value(&mut args, "--header-timeout-ms")?);
            }
            other if other.starts_with("--root=") => {
                root = Some(value_after_eq(other, "--root")?.into());
            }
            other if other.starts_with("--registry=") => {
                registry = Some(value_after_eq(other, "--registry")?.into());
            }
            other if other.starts_with("--transport=") => {
                transport = Some(value_after_eq(other, "--transport")?);
            }
            other if other.starts_with("--bind=") => {
                bind = Some(value_after_eq(other, "--bind")?);
            }
            other if other.starts_with("--token-file=") => {
                token_file = Some(value_after_eq(other, "--token-file")?.into());
            }
            other if other.starts_with("--max-connections=") => {
                max_connections = Some(value_after_eq(other, "--max-connections")?);
            }
            other if other.starts_with("--header-timeout-ms=") => {
                header_timeout_ms = Some(value_after_eq(other, "--header-timeout-ms")?);
            }
            other => return Err(format!("未知参数 `{other}`")),
        }
    }
    let mut config = AppConfig::from_env();
    if let Some(root) = root {
        config.root = root;
    }
    config.registry_path = registry;

    // 缺省 stdio：不读 token、不解析 bind，既有行为一字不变。
    let transport = transport.unwrap_or_else(|| "stdio".to_string());
    match transport.as_str() {
        "stdio" => {
            if bind.is_some()
                || token_file.is_some()
                || max_connections.is_some()
                || header_timeout_ms.is_some()
            {
                return Err(
                    "--bind / --token-file / --max-connections / --header-timeout-ms 只在 --transport http 下有效（缺省传输是 stdio）"
                        .to_string(),
                );
            }
            Ok(Cli::Run(config))
        }
        "http" => {
            let bind = match bind {
                Some(value) => parse_bind(&value)?,
                None => parse_bind("127.0.0.1:7950")?,
            };
            let token = resolve_token(token_file.as_deref(), env_token)?;
            let max_connections = match max_connections {
                Some(value) => parse_max_connections(&value)?,
                None => DEFAULT_MAX_CONNECTIONS,
            };
            let header_timeout = match header_timeout_ms {
                Some(value) => Duration::from_millis(parse_header_timeout_ms(&value)?),
                None => Duration::from_millis(DEFAULT_HEADER_TIMEOUT_MS),
            };
            Ok(Cli::RunHttp(HttpConfig {
                app: config,
                bind,
                token,
                max_connections,
                header_timeout,
            }))
        }
        other => Err(format!("--transport 只接受 `stdio` 或 `http`：{other:?}")),
    }
}

/// `--bind`：只接受**回环 IP 字面量**的 `ip:port`（对齐 game-svr `[Admin] AdminBind` 的口径）。
///
/// 主机名（含 `localhost`）不接受——避免依赖 DNS/解析歧义；非回环（`0.0.0.0`、局域网/公网 IP）
/// 在**启动前**直接拒绝：HTTP 传输能执行 GM 命令，绝不能暴露到本机之外。
/// 端口 `0` 合法：由系统分配实际端口，启动行打印解析后的地址（e2e 据此取端口，避免先绑后放的 TOCTOU）。
fn parse_bind(value: &str) -> Result<SocketAddr, String> {
    let addr: SocketAddr = value.trim().parse().map_err(|_| {
        format!("--bind 不是合法的 `ip:port`（只接受 IP 字面量，不接受主机名）：{value:?}")
    })?;
    if !addr.ip().is_loopback() {
        return Err(format!(
            "--bind {value} 不是回环地址；HTTP 传输只接受 127.0.0.0/8 或 ::1（拒绝在非本机地址上暴露 GM 执行面）"
        ));
    }
    Ok(addr)
}

/// `--max-connections`：并发连接上限，`1..=4096`。
fn parse_max_connections(value: &str) -> Result<usize, String> {
    let parsed: usize = value.trim().parse().map_err(|_| {
        format!("--max-connections 不是合法的十进制整数：{value:?}（允许 1..=4096）")
    })?;
    if !MAX_CONNECTIONS_RANGE.contains(&parsed) {
        return Err(format!(
            "--max-connections {parsed} 超出允许区间 {}..={}",
            MAX_CONNECTIONS_RANGE.start(),
            MAX_CONNECTIONS_RANGE.end()
        ));
    }
    Ok(parsed)
}

/// `--header-timeout-ms`：请求头读超时（毫秒），`1..=600000`。
fn parse_header_timeout_ms(value: &str) -> Result<u64, String> {
    let parsed: u64 = value.trim().parse().map_err(|_| {
        format!("--header-timeout-ms 不是合法的十进制整数：{value:?}（允许 1..=600000 毫秒）")
    })?;
    if !HEADER_TIMEOUT_MS_RANGE.contains(&parsed) {
        return Err(format!(
            "--header-timeout-ms {parsed} 超出允许区间 {}..={}",
            HEADER_TIMEOUT_MS_RANGE.start(),
            HEADER_TIMEOUT_MS_RANGE.end()
        ));
    }
    Ok(parsed)
}

/// Bearer token：`--token-file`（内容去首尾空白）> env `MIR2_MCP_HTTP_TOKEN`；两者都缺/为空 ⇒ 拒绝。
fn resolve_token(token_file: Option<&Path>, env_token: Option<String>) -> Result<String, String> {
    let token = match token_file {
        Some(path) => std::fs::read_to_string(path)
            .map_err(|e| format!("读 --token-file {} 失败：{e}", path.display()))?,
        None => env_token.unwrap_or_default(),
    };
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(
            "--transport http 缺少 Bearer token：请设置 env MIR2_MCP_HTTP_TOKEN 或 --token-file <path>（内容去首尾空白）"
                .to_string(),
        );
    }
    Ok(token)
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

    fn parse_with_env(args: &[&str], env_token: Option<&str>) -> Result<Cli, String> {
        parse_args_with_env(
            args.iter().map(ToString::to_string),
            env_token.map(ToString::to_string),
        )
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

    /// A1：不传 `--transport` ⇒ stdio（既有行为），且 env token 完全不参与。
    #[test]
    fn transport_defaults_to_stdio_and_ignores_http_env() {
        let cli = parse_with_env(&["--root", "C:/tmp/r"], Some("ignored-token"))
            .expect("stdio 应解析成功");
        let Cli::Run(config) = cli else {
            panic!("缺省必须是 stdio");
        };
        assert_eq!(config.root, PathBuf::from("C:/tmp/r"));
    }

    /// stdio 模式下给出 http-only 参数 ⇒ 拒绝（不静默忽略用户意图）。
    #[test]
    fn stdio_rejects_http_only_flags() {
        assert!(parse_with_env(&["--bind", "127.0.0.1:7950"], None).is_err());
        assert!(parse_with_env(&["--token-file", "t.txt"], None).is_err());
    }

    /// A2：http 模式缺 token ⇒ 拒绝，错误里点名 token。
    #[test]
    fn http_requires_token() {
        let err = parse_with_env(&["--transport", "http"], None).expect_err("缺 token 必须拒绝");
        assert!(err.contains("token"), "错误必须点名 token：{err}");
        let err =
            parse_with_env(&["--transport", "http"], Some("   ")).expect_err("空 token 必须拒绝");
        assert!(err.contains("token"), "{err}");
    }

    /// http 模式缺省 bind = 127.0.0.1:7950；env token 生效。
    #[test]
    fn http_default_bind_and_env_token() {
        let Ok(Cli::RunHttp(config)) = parse_with_env(&["--transport", "http"], Some("  s3cret "))
        else {
            panic!("应解析为 RunHttp");
        };
        assert_eq!(config.bind, "127.0.0.1:7950".parse().expect("addr"));
        assert_eq!(config.token, "s3cret", "token 必须去首尾空白");
    }

    /// A3：非回环 bind ⇒ 拒绝，错误里点名回环。
    #[test]
    fn http_rejects_non_loopback_bind() {
        for bad in ["0.0.0.0:7950", "192.168.1.10:7950", "[::]:7950"] {
            let err = parse_with_env(&["--transport", "http", "--bind", bad], Some("t"))
                .expect_err("非回环必须拒绝");
            assert!(err.contains("回环"), "--bind={bad} 错误必须点名回环：{err}");
        }
    }

    /// 主机名不是 IP 字面量 ⇒ 拒绝（对齐 AdminBind 口径）。
    #[test]
    fn http_rejects_hostname_bind() {
        let err = parse_with_env(
            &["--transport", "http", "--bind", "localhost:7950"],
            Some("t"),
        )
        .expect_err("主机名必须拒绝");
        assert!(err.contains("IP 字面量"), "{err}");
    }

    /// 回环字面量（含 127.0.0.0/8 与 ::1）放行。
    #[test]
    fn http_accepts_loopback_literals() {
        for good in ["127.0.0.1:7950", "127.0.0.2:1", "[::1]:7950"] {
            let ok = parse_with_env(&["--transport", "http", "--bind", good], Some("t"));
            assert!(
                matches!(ok, Ok(Cli::RunHttp(_))),
                "--bind={good} 应放行：{ok:?}"
            );
        }
    }

    /// `--token-file` 优先于 env，且内容去首尾空白。
    #[test]
    fn token_file_wins_over_env() {
        let path =
            std::env::temp_dir().join(format!("mir2-mcp-token-{}-prec.txt", std::process::id()));
        std::fs::write(&path, "  from-file\r\n").expect("写 token 夹具");
        let parsed = parse_with_env(
            &[
                "--transport",
                "http",
                "--token-file",
                path.to_str().expect("utf8 path"),
            ],
            Some("from-env"),
        );
        let _ = std::fs::remove_file(&path);
        let Ok(Cli::RunHttp(config)) = parsed else {
            panic!("应解析为 RunHttp：{parsed:?}");
        };
        assert_eq!(config.token, "from-file");
    }

    /// 未知传输名 ⇒ 拒绝。
    #[test]
    fn unknown_transport_rejected() {
        assert!(parse_with_env(&["--transport", "sse"], Some("t")).is_err());
    }

    /// 连接层护栏的缺省值：并发上限 64、请求头读超时 10000ms。
    #[test]
    fn http_resource_limit_defaults() {
        let Ok(Cli::RunHttp(config)) = parse_with_env(&["--transport", "http"], Some("t")) else {
            panic!("应解析为 RunHttp");
        };
        assert_eq!(config.max_connections, 64);
        assert_eq!(config.header_timeout, Duration::from_millis(10_000));
    }

    /// 连接层护栏可由 CLI 覆盖（e2e 用 `--max-connections 2` / 小超时驱动）。
    #[test]
    fn http_resource_limits_are_overridable() {
        let Ok(Cli::RunHttp(config)) = parse_with_env(
            &[
                "--transport",
                "http",
                "--max-connections",
                "2",
                "--header-timeout-ms=300",
            ],
            Some("t"),
        ) else {
            panic!("应解析为 RunHttp");
        };
        assert_eq!(config.max_connections, 2);
        assert_eq!(config.header_timeout, Duration::from_millis(300));
    }

    /// Minor-3：两个区间的**上界正例**——恰在上界接受、上界+1 拒绝。
    ///
    /// 没有正例时把上界各减 1（4096→4095、600000→599999），全套用例仍绿。
    #[test]
    fn resource_limit_upper_bounds_are_exact() {
        let Ok(Cli::RunHttp(config)) = parse_with_env(
            &[
                "--transport",
                "http",
                "--max-connections",
                "4096",
                "--header-timeout-ms",
                "600000",
            ],
            Some("t"),
        ) else {
            panic!("上界值 4096 / 600000 必须被接受");
        };
        assert_eq!(config.max_connections, 4096, "上界 4096 必须原样生效");
        assert_eq!(
            config.header_timeout,
            Duration::from_millis(600_000),
            "上界 600000ms 必须原样生效"
        );

        // 上界 +1 必须拒绝（与上界一起构成闭区间的两颗牙）。
        let err = parse_with_env(
            &["--transport", "http", "--max-connections", "4097"],
            Some("t"),
        )
        .expect_err("4097 必须拒绝");
        assert!(err.contains("--max-connections"), "{err}");
        let err = parse_with_env(
            &["--transport", "http", "--header-timeout-ms", "600001"],
            Some("t"),
        )
        .expect_err("600001 必须拒绝");
        assert!(err.contains("--header-timeout-ms"), "{err}");
    }

    /// 越界/非数字的护栏值 ⇒ 启动前拒绝，错误点名参数。
    #[test]
    fn http_rejects_invalid_resource_limits() {
        for bad in ["0", "4097", "-1", "many", ""] {
            let err = parse_with_env(
                &["--transport", "http", "--max-connections", bad],
                Some("t"),
            )
            .expect_err("非法并发上限必须拒绝");
            assert!(err.contains("--max-connections"), "{err}");
        }
        for bad in ["0", "600001", "-5", "slow"] {
            let err = parse_with_env(
                &["--transport", "http", "--header-timeout-ms", bad],
                Some("t"),
            )
            .expect_err("非法请求头超时必须拒绝");
            assert!(err.contains("--header-timeout-ms"), "{err}");
        }
    }

    /// 连接层护栏只属于 http 传输：stdio 下给出 ⇒ 拒绝（与 --bind/--token-file 同口径）。
    #[test]
    fn stdio_rejects_http_only_resource_flags() {
        assert!(parse_with_env(&["--max-connections", "2"], None).is_err());
        assert!(parse_with_env(&["--header-timeout-ms", "300"], None).is_err());
    }

    /// `--bind` 的端口可以为 0（系统分配），启动行再打印实际端口。
    #[test]
    fn http_accepts_zero_port_bind() {
        let Ok(Cli::RunHttp(config)) =
            parse_with_env(&["--transport", "http", "--bind", "127.0.0.1:0"], Some("t"))
        else {
            panic!("127.0.0.1:0 应放行");
        };
        assert_eq!(config.bind.port(), 0);
    }
}
