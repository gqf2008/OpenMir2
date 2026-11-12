//! 配置装配：INI 文件 + 环境变量 + 命令行，**fail-closed**。
//!
//! 优先级（高 → 低）：
//!
//! * `bind`：`--bind` > `RES_SVR_BIND` > `[res-svr] bind` > 默认 `127.0.0.1:7300`
//! * `token`：`RES_SVR_TOKEN` > `[res-svr] token`。**没有 `--token` 参数**——token 绝不进
//!   进程命令行（`Get-CimInstance Win32_Process` 对同机其它用户可见）。
//! * 资产根映射：`RES_SVR_MAP_<名称>` > `[map]` > 内置三根（`Data`/`Map`/`Wav`）。
//! * 其余限额：`[res-svr]` / `[ext]` > 内置默认。
//!
//! token 的红线：`ResConfig` 的 `Debug` 手写为 `<redacted>`，任何 `{:?}` 都不会泄漏本体；
//! 错误信息只报来源与规则，不回显 token 值。

use std::collections::BTreeMap;
use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 默认监听地址（卡 A1：默认只绑回环 7300）。
pub const DEFAULT_BIND: &str = "127.0.0.1:7300";
/// 默认单文件上限（1 GiB）：真实资产里最大的 `.wzl` 远小于此，默认即收紧。
pub const DEFAULT_MAX_FILE_SIZE: u64 = 1_073_741_824;
/// 默认并发连接上限。
pub const DEFAULT_MAX_CONNECTIONS: usize = 8;
/// 默认每 token 限流速率（请求/秒）。
pub const DEFAULT_RATE_LIMIT_RPS: f64 = 20.0;
/// 默认每 token 突发额度。
pub const DEFAULT_RATE_LIMIT_BURST: f64 = 40.0;
/// 默认请求头读取超时（毫秒）。
pub const DEFAULT_HEADER_TIMEOUT_MS: u64 = 15_000;
/// 默认扩展白名单（卡 A3：`.wil/.wick` 等一并列入）。
pub const DEFAULT_EXTENSIONS: &[&str] = &[
    ".wzl", ".wzx", ".wav", ".mp3", ".map", ".mex", ".wil", ".wick",
];
/// 默认资产根映射（卡面「建议默认」；可用 `[map]` 或 `RES_SVR_MAP_*` 覆盖）。
pub const DEFAULT_MAPS: &[(&str, &str)] = &[
    ("Data", "D:/MirClient-run/Data"),
    ("Map", "E:/MirServer/M2GameSvr/Map"),
    ("Wav", "D:/MirClient-run/Wav"),
];
/// 建议的 token 最短长度（低于此值只告警、不拒绝；告警行只打长度）。
pub const TOKEN_RECOMMENDED_MIN_LEN: usize = 16;
/// token 长度硬上界（防止把整个文件误配进该键）。
pub const TOKEN_MAX_LEN: usize = 256;
/// 映射环境变量前缀：`RES_SVR_MAP_<名称>=<根路径>`。
pub const MAP_ENV_PREFIX: &str = "RES_SVR_MAP_";

/// 前缀 → 资产根映射（根路径在 [`build`] 里已 canonicalize）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootMap {
    /// 请求路径第一段的前缀名（大小写不敏感匹配，展示时保留配置里的拼写）。
    pub prefix: String,
    /// canonicalize 后的资产根。
    pub root: PathBuf,
}

/// 校验通过的运行配置。
#[derive(Clone)]
pub struct ResConfig {
    /// 监听地址。
    pub bind: SocketAddr,
    /// Bearer token（**不得进入任何日志/调试输出**）。
    pub token: String,
    /// 前缀映射表（至少一条）。
    pub roots: Vec<RootMap>,
    /// 扩展名白名单（小写、带点）。
    pub allowed_exts: Vec<String>,
    /// 单文件大小上限（字节）；超过 ⇒ 413。
    pub max_file_size: u64,
    /// 并发连接上限；超过 ⇒ 新连接直接 429 并关闭。
    pub max_connections: usize,
    /// 每 token 令牌桶速率（请求/秒）。
    pub rate_limit_rps: f64,
    /// 每 token 令牌桶容量。
    pub rate_limit_burst: f64,
    /// 请求头读取超时。
    pub header_timeout: Duration,
}

impl fmt::Debug for ResConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // token 红线：Debug 输出永远只给长度。
        f.debug_struct("ResConfig")
            .field("bind", &self.bind)
            .field(
                "token",
                &format_args!("<redacted>(len={})", self.token.len()),
            )
            .field("roots", &self.roots)
            .field("allowed_exts", &self.allowed_exts)
            .field("max_file_size", &self.max_file_size)
            .field("max_connections", &self.max_connections)
            .field("rate_limit_rps", &self.rate_limit_rps)
            .field("rate_limit_burst", &self.rate_limit_burst)
            .field("header_timeout", &self.header_timeout)
            .finish()
    }
}

/// 命令行（只允许 `--conf` / `--bind` / `--help`）。
#[derive(Debug, Clone, Default)]
pub struct Cli {
    /// 显式配置文件路径。
    pub conf: Option<PathBuf>,
    /// 显式监听地址（覆盖环境变量与配置文件）。
    pub bind: Option<String>,
    /// 打印帮助后退出。
    pub help: bool,
}

impl Cli {
    /// 解析命令行（未知参数 ⇒ 用法错误，fail-closed）。
    ///
    /// # Errors
    /// 缺参数值、未知参数、重复参数都返回人读错误。
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut cli = Self::default();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--help" | "-h" => cli.help = true,
                "--conf" => {
                    let value = iter.next().ok_or("--conf 缺少路径参数")?;
                    cli.conf = Some(PathBuf::from(value));
                }
                "--bind" => {
                    let value = iter.next().ok_or("--bind 缺少地址参数")?;
                    cli.bind = Some(value.clone());
                }
                other => return Err(format!("未知参数 {other:?}（见 --help）")),
            }
        }
        Ok(cli)
    }
}

/// 配置三来源的原始输入（文件已解码为文本；env 已转成可读值）。
#[derive(Debug, Default)]
pub struct Sources {
    /// 配置文件文本（`None` ⇒ 无配置文件，全走 env/默认）。
    pub text: Option<String>,
    /// `RES_SVR_TOKEN`。
    pub token_env: Option<String>,
    /// `RES_SVR_BIND`。
    pub bind_env: Option<String>,
    /// `RES_SVR_MAP_<名称>` 的（名称, 值）列表。
    pub map_env: Vec<(String, String)>,
    /// `--bind`。
    pub cli_bind: Option<String>,
}

impl Sources {
    /// 从进程环境与文件系统收集来源。
    ///
    /// # Errors
    /// 配置文件不可读、环境变量非法 Unicode。
    pub fn collect(cli: &Cli) -> Result<Self, String> {
        let conf_path = resolve_conf_path(cli)?;
        let text = match conf_path {
            Some(path) => {
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("读取配置失败 {}：{error}", path.display()))?;
                Some(mir2_encoding::decode_config_text(&bytes))
            }
            None => None,
        };
        let token_env = env_opt("RES_SVR_TOKEN")?;
        let bind_env = env_opt("RES_SVR_BIND")?;
        let mut map_env = Vec::new();
        for (name, value) in std::env::vars() {
            if name.len() <= MAP_ENV_PREFIX.len() {
                continue;
            }
            let prefix = name.get(..MAP_ENV_PREFIX.len()).unwrap_or("");
            if !prefix.eq_ignore_ascii_case(MAP_ENV_PREFIX) {
                continue;
            }
            if let Some(suffix) = name.get(MAP_ENV_PREFIX.len()..) {
                map_env.push((suffix.to_string(), value));
            }
        }
        Ok(Self {
            text,
            token_env,
            bind_env,
            map_env,
            cli_bind: cli.bind.clone(),
        })
    }
}

fn env_opt(name: &str) -> Result<Option<String>, String> {
    match std::env::var_os(name) {
        None => Ok(None),
        Some(value) => value
            .into_string()
            .map(Some)
            .map_err(|_| format!("环境变量 {name} 不是合法 Unicode")),
    }
}

/// 配置文件路径：`--conf` > `RES_SVR_CONF` > 可执行文件同目录的 `config.conf`（存在才用）。
fn resolve_conf_path(cli: &Cli) -> Result<Option<PathBuf>, String> {
    if let Some(path) = &cli.conf {
        return Ok(Some(path.clone()));
    }
    if let Some(path) = env_opt("RES_SVR_CONF")? {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(Some(PathBuf::from(trimmed)));
        }
    }
    let candidate = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .map(|dir| dir.join("config.conf"));
    match candidate {
        Some(path) if path.is_file() => Ok(Some(path)),
        _ => Ok(None),
    }
}

/// 合并并校验三来源，产出运行配置。
///
/// # Errors
/// token 缺失/非法、`bind` 解析失败、映射根不存在或不是目录、限额非正数、扩展名白名单非法。
pub fn build(sources: &Sources) -> Result<ResConfig, String> {
    let ini = sources.text.as_deref().map(parse_ini).unwrap_or_default();
    let res_section = ini.get("res-svr").cloned().unwrap_or_default();
    let map_section = ini.get("map").cloned().unwrap_or_default();
    let ext_section = ini.get("ext").cloned().unwrap_or_default();

    let token = resolve_token(sources, &res_section)?;
    let bind = resolve_bind(sources, &res_section)?;
    let roots = resolve_roots(&map_section, &sources.map_env)?;
    let allowed_exts = resolve_extensions(&ext_section)?;

    let max_file_size = parse_u64(&res_section, "max_file_size", DEFAULT_MAX_FILE_SIZE)?;
    if max_file_size == 0 {
        return Err("配置项 max_file_size 必须大于 0".to_string());
    }
    let max_connections = parse_usize(&res_section, "max_connections", DEFAULT_MAX_CONNECTIONS)?;
    if max_connections == 0 {
        return Err("配置项 max_connections 必须大于 0".to_string());
    }
    let rate_limit_rps = parse_f64(&res_section, "rate_limit_rps", DEFAULT_RATE_LIMIT_RPS)?;
    if !rate_limit_rps.is_finite() || rate_limit_rps <= 0.0 {
        return Err("配置项 rate_limit_rps 必须是正有限数".to_string());
    }
    let rate_limit_burst = parse_f64(&res_section, "rate_limit_burst", DEFAULT_RATE_LIMIT_BURST)?;
    if !rate_limit_burst.is_finite() || rate_limit_burst < 1.0 {
        return Err("配置项 rate_limit_burst 必须是不小于 1 的有限数".to_string());
    }
    let header_timeout_ms =
        parse_u64(&res_section, "header_timeout_ms", DEFAULT_HEADER_TIMEOUT_MS)?;
    if header_timeout_ms == 0 {
        return Err("配置项 header_timeout_ms 必须大于 0".to_string());
    }

    Ok(ResConfig {
        bind,
        token,
        roots,
        allowed_exts,
        max_file_size,
        max_connections,
        rate_limit_rps,
        rate_limit_burst,
        header_timeout: Duration::from_millis(header_timeout_ms),
    })
}

fn resolve_token(sources: &Sources, section: &BTreeMap<String, String>) -> Result<String, String> {
    let (token, origin) = match &sources.token_env {
        Some(value) => (value.trim().to_string(), "RES_SVR_TOKEN"),
        None => match get_ci(section, "token") {
            Some(value) => (value.trim().to_string(), "配置文件 [res-svr] token"),
            None => return Err(
                "缺少 Bearer token：请设置 RES_SVR_TOKEN 或配置文件 [res-svr] token（fail-closed）"
                    .to_string(),
            ),
        },
    };
    validate_token(&token, origin)?;
    Ok(token)
}

fn validate_token(token: &str, origin: &str) -> Result<(), String> {
    if token.is_empty() {
        return Err(format!(
            "{origin} 为空 —— 必须配置非空 Bearer token（fail-closed）"
        ));
    }
    if token.len() > TOKEN_MAX_LEN {
        return Err(format!(
            "{origin} 长度 {} 超过上限 {TOKEN_MAX_LEN}（疑似误配，fail-closed）",
            token.len()
        ));
    }
    if !token.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
        return Err(format!(
            "{origin} 含空白或非可打印 ASCII 字符（只允许 0x21..=0x7E）"
        ));
    }
    Ok(())
}

fn resolve_bind(
    sources: &Sources,
    section: &BTreeMap<String, String>,
) -> Result<SocketAddr, String> {
    let raw = sources
        .cli_bind
        .as_ref()
        .or(sources.bind_env.as_ref())
        .or_else(|| get_ci(section, "bind"))
        .cloned()
        .unwrap_or_else(|| DEFAULT_BIND.to_string());
    raw.trim()
        .parse::<SocketAddr>()
        .map_err(|_| format!("监听地址非法：{raw:?}（期望 ip:port，如 {DEFAULT_BIND}）"))
}

fn resolve_roots(
    section: &BTreeMap<String, String>,
    env_overrides: &[(String, String)],
) -> Result<Vec<RootMap>, String> {
    let mut roots: Vec<RootMap> = if section.is_empty() {
        DEFAULT_MAPS
            .iter()
            .map(|(prefix, root)| RootMap {
                prefix: (*prefix).to_string(),
                root: PathBuf::from(root),
            })
            .collect()
    } else {
        section
            .iter()
            .map(|(prefix, root)| RootMap {
                prefix: prefix.clone(),
                root: PathBuf::from(root),
            })
            .collect()
    };
    for (name, value) in env_overrides {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(format!(
                "环境变量 {MAP_ENV_PREFIX}{name} 为空（fail-closed）"
            ));
        }
        if name.trim().is_empty() {
            return Err(format!(
                "环境变量 {MAP_ENV_PREFIX} 后缺少映射名（fail-closed）"
            ));
        }
        let root = PathBuf::from(trimmed);
        match roots
            .iter_mut()
            .find(|existing| existing.prefix.eq_ignore_ascii_case(name))
        {
            Some(existing) => existing.root = root,
            None => roots.push(RootMap {
                prefix: name.clone(),
                root,
            }),
        }
    }
    let mut validated = Vec::with_capacity(roots.len());
    for (index, map) in roots.iter().enumerate() {
        if map.prefix.is_empty() {
            return Err("资产根映射的映射名为空（fail-closed）".to_string());
        }
        if roots[..index]
            .iter()
            .any(|other| other.prefix.eq_ignore_ascii_case(&map.prefix))
        {
            return Err(format!(
                "资产根映射名重复（大小写不敏感）：{:?}（fail-closed）",
                map.prefix
            ));
        }
        if !map.root.is_dir() {
            return Err(format!(
                "资产根不存在或不是目录：{:?} → {}（fail-closed）",
                map.prefix,
                map.root.display()
            ));
        }
        let canonical = std::fs::canonicalize(&map.root).map_err(|error| {
            format!(
                "资产根 canonicalize 失败：{:?} → {}：{error}（fail-closed）",
                map.prefix,
                map.root.display()
            )
        })?;
        validated.push(RootMap {
            prefix: map.prefix.clone(),
            root: canonical,
        });
    }
    if validated.is_empty() {
        return Err("资产根映射为空（fail-closed）".to_string());
    }
    Ok(validated)
}

fn resolve_extensions(section: &BTreeMap<String, String>) -> Result<Vec<String>, String> {
    let raw = get_ci(section, "whitelist")
        .cloned()
        .unwrap_or_else(|| DEFAULT_EXTENSIONS.join(","));
    let mut exts = Vec::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        let Some(rest) = lower.strip_prefix('.') else {
            return Err(format!("扩展名必须以 '.' 开头：{trimmed:?}（fail-closed）"));
        };
        if rest.is_empty() || !rest.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(format!(
                "扩展名只允许 ASCII 字母数字：{trimmed:?}（fail-closed）"
            ));
        }
        if !exts.contains(&lower) {
            exts.push(lower);
        }
    }
    if exts.is_empty() {
        return Err("扩展名白名单为空（fail-closed，拒绝起一个什么都发不出的服务）".to_string());
    }
    Ok(exts)
}

/// 大小写不敏感的键查询（INI 键保留原拼写，查询按名匹配）。
fn get_ci<'a>(section: &'a BTreeMap<String, String>, key: &str) -> Option<&'a String> {
    section
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        .map(|(_, value)| value)
}

fn parse_u64(section: &BTreeMap<String, String>, key: &str, default: u64) -> Result<u64, String> {
    match get_ci(section, key) {
        None => Ok(default),
        Some(raw) => raw
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("配置项 {key} 不是合法无符号整数：{raw:?}")),
    }
}

fn parse_usize(
    section: &BTreeMap<String, String>,
    key: &str,
    default: usize,
) -> Result<usize, String> {
    let value = parse_u64(section, key, u64::try_from(default).unwrap_or(u64::MAX))?;
    usize::try_from(value).map_err(|_| format!("配置项 {key} 超出平台 usize 范围：{value}"))
}

fn parse_f64(section: &BTreeMap<String, String>, key: &str, default: f64) -> Result<f64, String> {
    match get_ci(section, key) {
        None => Ok(default),
        Some(raw) => raw
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("配置项 {key} 不是合法数值：{raw:?}")),
    }
}

/// 极简 INI 解析：`[section]`、`key=value`、`;`/`#` 行注释与值后 `;` 行内注释。
///
/// section/key 统一小写；值保留原文（去除首尾空白与行内注释）。无法识别的行静默跳过
/// ——**未知键不报错**（允许运维在配置文件里写注释性键），但任一**已识别键**取值非法
/// 都会 fail-closed。
fn parse_ini(text: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let Some(name) = rest.strip_suffix(']') else {
                continue;
            };
            let name = name.trim().to_ascii_lowercase();
            sections.entry(name.clone()).or_default();
            current = Some(name);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(section) = current.as_ref() else {
            continue;
        };
        // [res-svr]/[ext] 的键查询走 `get_ci`（大小写不敏感）；[map] 的键**保留原拼写**
        // ——它就是对外前缀名（`Data/...` 而不是 `data/...`）。
        let key = key.trim().to_string();
        if key.is_empty() {
            continue;
        }
        let value = value.split(';').next().unwrap_or("").trim().to_string();
        if let Some(entry) = sections.get_mut(section) {
            entry.insert(key, value);
        }
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_sources(dir: &crate::testutil::TempDir, extra: &str) -> Sources {
        let data = dir.dir("Data");
        let map = dir.dir("Map");
        let wav = dir.dir("Wav");
        let text = format!(
            "[res-svr]\r\ntoken=PAGE-TEST-TOKEN-0123456789\r\n{extra}\r\n[map]\r\nData={}\r\nMap={}\r\nWav={}\r\n",
            data.display(),
            map.display(),
            wav.display()
        );
        Sources {
            text: Some(text),
            ..Sources::default()
        }
    }

    #[test]
    fn token_from_env_overrides_file_and_argv_is_rejected() {
        let dir = crate::testutil::TempDir::new("cfg-token");
        let mut sources = fixture_sources(&dir, "");
        sources.token_env = Some("ENV-TOKEN-0123456789".to_string());
        let cfg = build(&sources).expect("配置应通过");
        assert_eq!(cfg.token, "ENV-TOKEN-0123456789");

        let cli = Cli::parse(&["--token".to_string(), "x".to_string()]);
        assert!(cli.is_err(), "绝不接受 --token（token 不进 argv）");
    }

    #[test]
    fn missing_token_fails_closed() {
        let dir = crate::testutil::TempDir::new("cfg-notoken");
        let mut sources = fixture_sources(&dir, "");
        sources.text = Some(
            sources
                .text
                .expect("有文本")
                .replace("token=PAGE-TEST-TOKEN-0123456789\r\n", ""),
        );
        let error = build(&sources).expect_err("缺 token 必须拒绝");
        assert!(error.contains("缺少 Bearer token"), "{error}");
    }

    #[test]
    fn token_with_space_or_control_fails_closed() {
        for bad in ["has space", "tab\there", "nul\u{0}"] {
            let dir = crate::testutil::TempDir::new("cfg-badtoken");
            let mut sources = fixture_sources(&dir, "");
            sources.token_env = Some(bad.to_string());
            assert!(build(&sources).is_err(), "token {bad:?} 必须拒绝");
        }
    }

    #[test]
    fn debug_output_never_contains_token() {
        let dir = crate::testutil::TempDir::new("cfg-debug");
        let mut sources = fixture_sources(&dir, "");
        sources.token_env = Some("SUPER-SECRET-TOKEN-999".to_string());
        let cfg = build(&sources).expect("配置应通过");
        let debug = format!("{cfg:?}");
        assert!(!debug.contains("SUPER-SECRET-TOKEN-999"), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");
    }

    #[test]
    fn defaults_apply_when_sections_missing() {
        let dir = crate::testutil::TempDir::new("cfg-defaults");
        let mut sources = Sources {
            token_env: Some("DEFAULT-TOKEN-0123456789".to_string()),
            ..Sources::default()
        };
        sources.map_env = vec![("Data".to_string(), dir.dir("Data").display().to_string())];
        // 其余两根走内置默认（D:/E: 上真实存在与否不确定）⇒ 这里显式给三根。
        sources
            .map_env
            .push(("Map".to_string(), dir.dir("Map").display().to_string()));
        sources
            .map_env
            .push(("Wav".to_string(), dir.dir("Wav").display().to_string()));
        let cfg = build(&sources).expect("配置应通过");
        assert_eq!(cfg.bind, DEFAULT_BIND.parse().expect("默认绑定地址合法"));
        assert_eq!(cfg.max_file_size, DEFAULT_MAX_FILE_SIZE);
        assert_eq!(cfg.max_connections, DEFAULT_MAX_CONNECTIONS);
        assert_eq!(cfg.allowed_exts, DEFAULT_EXTENSIONS);
        assert_eq!(cfg.roots.len(), 3);
    }

    #[test]
    fn env_map_override_replaces_same_prefix() {
        let dir = crate::testutil::TempDir::new("cfg-mapenv");
        let mut sources = fixture_sources(&dir, "");
        let replacement = dir.dir("Data2");
        sources
            .map_env
            .push(("DATA".to_string(), replacement.display().to_string()));
        let cfg = build(&sources).expect("配置应通过");
        let data = cfg
            .roots
            .iter()
            .find(|map| map.prefix.eq_ignore_ascii_case("Data"))
            .expect("Data 映射存在");
        assert_eq!(
            data.root,
            std::fs::canonicalize(&replacement).expect("canonicalize"),
            "环境变量按名称覆盖同名前缀"
        );
    }

    #[test]
    fn missing_root_or_empty_whitelist_fails_closed() {
        let dir = crate::testutil::TempDir::new("cfg-badroot");
        let mut sources = fixture_sources(&dir, "missing=bad");
        sources.text = Some(format!(
            "{}\n[ext]\nwhitelist=,\n",
            sources.text.expect("有文本")
        ));
        let error = build(&sources).expect_err("空白名单必须拒绝");
        assert!(error.contains("白名单为空"), "{error}");

        sources.text = Some(format!(
            "[res-svr]\ntoken=T-0123456789ABCDEF\n[map]\nData={}\n",
            dir.path().join("does-not-exist").display()
        ));
        let error = build(&sources).expect_err("缺根目录必须拒绝");
        assert!(error.contains("资产根不存在"), "{error}");
    }

    #[test]
    fn limits_are_parsed_and_validated() {
        let dir = crate::testutil::TempDir::new("cfg-limits");
        let sources = fixture_sources(
            &dir,
            "max_file_size=1024\nmax_connections=2\nrate_limit_rps=3.5\nrate_limit_burst=7\nheader_timeout_ms=500",
        );
        let cfg = build(&sources).expect("配置应通过");
        assert_eq!(cfg.max_file_size, 1024);
        assert_eq!(cfg.max_connections, 2);
        assert!((cfg.rate_limit_rps - 3.5).abs() < f64::EPSILON);
        assert!((cfg.rate_limit_burst - 7.0).abs() < f64::EPSILON);
        assert_eq!(cfg.header_timeout, Duration::from_millis(500));

        let sources = fixture_sources(&dir, "rate_limit_rps=0");
        assert!(build(&sources).is_err(), "0 速率必须拒绝");
        let sources = fixture_sources(&dir, "max_connections=0");
        assert!(build(&sources).is_err(), "0 并发必须拒绝");
    }

    #[test]
    fn cli_rejects_unknown_and_missing_values() {
        assert!(Cli::parse(&["--nope".to_string()]).is_err());
        assert!(Cli::parse(&["--conf".to_string()]).is_err());
        let cli = Cli::parse(&[
            "--conf".to_string(),
            "a.conf".to_string(),
            "--bind".to_string(),
            "127.0.0.1:9".to_string(),
        ])
        .expect("合法命令行");
        assert_eq!(cli.conf, Some(PathBuf::from("a.conf")));
        assert_eq!(cli.bind.as_deref(), Some("127.0.0.1:9"));
    }
}
