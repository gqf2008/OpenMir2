//! LoginGate 的遗留 INI 配置读取（`config.conf`），键名照现网，**缺失即报错**（M1 验收④）。
//!
//! 现网样件（`E:\MirServer\LoginGate\config.conf`）：
//! ```ini
//! [LoginGate]
//! ServerAddr0=127.0.0.1
//! ServerPort0=5500     ; 下游 LoginSrv
//! GateAddr0=127.0.0.1
//! GatePort0=7000       ; 对外监听
//! ```

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::idle_timeout::{DEFAULT_TIMEOUT_MS, MIN_TIMEOUT_MS};

/// 网关配置（只取 M1 必需项 + 会话超时；其余键留给后续按需补）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateConfig {
    /// 下游 LoginSrv 地址。
    pub srv_addr: String,
    /// 下游 LoginSrv 端口（现网 5500）。
    pub srv_port: u16,
    /// 对外监听地址。
    pub gate_addr: String,
    /// 对外监听端口（现网 7000）。
    pub gate_port: u16,
    /// 会话空闲超时（ms）。C# `[Integer] ClientTimeOutTime`：缺省 60s、`<10s` 夹到 10s。
    pub client_timeout_ms: u64,
}

/// 默认配置文件：可执行文件同目录的 `config.conf`（与 C# `AppContext.BaseDirectory` 一致）。
#[must_use]
pub fn default_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("config.conf")
}

/// 读取并校验配置。
///
/// # Errors
/// 文件不存在、缺 `[LoginGate]` 段、缺任一必需键、端口非法 —— 都返回错误（调用方据此非零退出）。
pub fn load(path: &Path) -> Result<GateConfig> {
    // 照 oracle（`ConfigFile.cs:263`）读原始字节 + **GB2312** 解码：现网 `LoginGate\config.conf`
    // 含中文（如 `Title=热血传奇`），严格 UTF-8 会在这种配置上失败。
    let bytes = std::fs::read(path).with_context(|| {
        format!(
            "读取配置文件失败：{}（M1④：缺配置必须明确报错退出）",
            path.display()
        )
    })?;
    let text = crate::gbk::decode(&bytes);
    let section = parse_section(&text, "LoginGate");
    let get = |key: &str| -> Result<String> {
        section
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.clone())
            .ok_or_else(|| anyhow::anyhow!("配置缺少 [LoginGate] {key}（M1④）"))
    };
    let port = |key: &str| -> Result<u16> {
        let raw = get(key)?;
        raw.parse::<u16>()
            .map_err(|_| anyhow::anyhow!("配置 {key} 端口非法：{raw:?}"))
    };
    let cfg = GateConfig {
        srv_addr: get("ServerAddr0")?,
        srv_port: port("ServerPort0")?,
        gate_addr: get("GateAddr0")?,
        gate_port: port("GatePort0")?,
        client_timeout_ms: client_timeout_ms(&text),
    };
    if cfg.srv_port == 0 || cfg.gate_port == 0 {
        bail!("端口为 0 非法：{cfg:?}");
    }
    Ok(cfg)
}

/// C# `LoginGate/Conf/ConfigManager.cs:26-31` 的 `[Integer] ClientTimeOutTime`。
///
/// 语义：键缺失 / 非法 ⇒ 缺省 `60 * 1000`；解析成功但 `< 10 * 1000` ⇒ 夹到 `10 * 1000`。
/// C# 在夹取后会 `WriteInteger` 回写 config.conf；Rust 只在内存里夹取、**不改写用户配置**
/// （回写属配置持久化副作用，不改变客户端可观测行为；报告已登记）。
fn client_timeout_ms(text: &str) -> u64 {
    let section = parse_section(text, "Integer");
    let raw = section
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("ClientTimeOutTime"))
        .map(|(_, v)| v.trim());
    // C# `ReadInteger` 走 int.TryParse：失败即用缺省。
    let default = i64::try_from(DEFAULT_TIMEOUT_MS).unwrap_or(i64::MAX);
    let parsed = raw.and_then(|v| v.parse::<i64>().ok()).unwrap_or(default);
    // C# `if (ClientTimeOutTime < 10 * 1000) ClientTimeOutTime = 10 * 1000;`
    let min = i64::try_from(MIN_TIMEOUT_MS).unwrap_or(i64::MAX);
    let clamped = parsed.max(min);
    u64::try_from(clamped).unwrap_or(DEFAULT_TIMEOUT_MS)
}

/// 取 `[section]` 下的键值对（大小写不敏感；`;`/`#` 起头为注释；去掉行尾注释）。
fn parse_section(text: &str, want: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            if let Some(name) = rest.strip_suffix(']') {
                in_section = name.trim().eq_ignore_ascii_case(want);
            }
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.split(';').next().unwrap_or("").trim();
        out.push((k.trim().to_string(), v.to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 现网样件必须能解析出 7000/5500 四元组。
    #[test]
    fn parses_deployed_login_gate_conf() {
        let dir = std::env::temp_dir().join(format!("lg-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.conf");
        std::fs::write(
            &p,
            "[LoginGate]\nKeepConnectTimeOut=60000\nServerAddr0=127.0.0.1\nServerPort0=5500\nGateAddr0=127.0.0.1\nGatePort0=7000\nShowDebug=0\n",
        )
        .unwrap();
        let cfg = load(&p).unwrap();
        assert_eq!(cfg.srv_addr, "127.0.0.1");
        assert_eq!(cfg.srv_port, 5500);
        assert_eq!(cfg.gate_addr, "127.0.0.1");
        assert_eq!(cfg.gate_port, 7000);
        // `[LoginGate] KeepConnectTimeOut` 不是 oracle 读取的键；超时取 `[Integer]` 缺省。
        assert_eq!(cfg.client_timeout_ms, 60_000);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 判据：`[Integer] ClientTimeOutTime` 必须被读出（C# `ConfigManager.cs:26`）。
    /// 牙：把读取改回常量 60000 ⇒ 本用例红。
    #[test]
    fn parses_integer_client_timeout() {
        let dir = std::env::temp_dir().join(format!("lg-timeout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.conf");
        std::fs::write(
            &p,
            "[Integer]\nClientTimeOutTime=45000\n\n[LoginGate]\nServerAddr0=127.0.0.1\n\
             ServerPort0=5500\nGateAddr0=127.0.0.1\nGatePort0=7000\n",
        )
        .unwrap();
        assert_eq!(load(&p).unwrap().client_timeout_ms, 45_000);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 判据：缺键/非法值 ⇒ 60s；`<10s`（含负数）⇒ 夹到 10s（C# `ConfigManager.cs:27-31`）。
    /// 牙：去掉夹取、或把非法值当 0 ⇒ 本用例红。
    #[test]
    fn client_timeout_defaults_and_clamps_like_oracle() {
        let dir = std::env::temp_dir().join(format!("lg-timeout-clamp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.conf");
        let base = "[LoginGate]\nServerAddr0=127.0.0.1\nServerPort0=5500\n\
                    GateAddr0=127.0.0.1\nGatePort0=7000\n";
        for (integer, want) in [
            (String::new(), 60_000_u64),                             // 缺键 ⇒ 缺省
            ("[Integer]\nClientTimeOutTime=abc\n".into(), 60_000),   // 非法 ⇒ 缺省
            ("[Integer]\nClientTimeOutTime=5000\n".into(), 10_000),  // 低于 10s ⇒ 夹取
            ("[Integer]\nClientTimeOutTime=-1\n".into(), 10_000),    // 负数 ⇒ 夹取
            ("[Integer]\nClientTimeOutTime=10000\n".into(), 10_000), // 恰好 10s
        ] {
            std::fs::write(&p, format!("{integer}{base}")).unwrap();
            assert_eq!(
                load(&p).unwrap().client_timeout_ms,
                want,
                "[Integer] 段={integer:?} 时必须得到 {want}ms"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 判据：同名键放在 `[LoginGate]` 段**不得**被当成超时（oracle 读 `[Integer]`）。
    #[test]
    fn client_timeout_is_read_from_integer_section_only() {
        let dir = std::env::temp_dir().join(format!("lg-timeout-sect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.conf");
        std::fs::write(
            &p,
            "[LoginGate]\nServerAddr0=127.0.0.1\nServerPort0=5500\nGateAddr0=127.0.0.1\n\
             GatePort0=7000\nClientTimeOutTime=12000\n",
        )
        .unwrap();
        assert_eq!(load(&p).unwrap().client_timeout_ms, 60_000);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 缺任一必需键必须报错（M1④ 的"缺配置要响"）。
    #[test]
    fn missing_keys_are_fatal() {
        let dir = std::env::temp_dir().join(format!("lg-cfg-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.conf");
        for body in [
            "[LoginGate]\nServerAddr0=127.0.0.1\nServerPort0=5500\nGateAddr0=127.0.0.1\n", // 缺 GatePort0
            "[LoginGate]\nServerAddr0=127.0.0.1\nServerPort0=abc\nGateAddr0=127.0.0.1\nGatePort0=7000\n", // 端口非法
            "[Other]\nServerAddr0=127.0.0.1\n", // 缺段
        ] {
            std::fs::write(&p, body).unwrap();
            assert!(load(&p).is_err(), "应在该配置上报错：{body:?}");
        }
        std::fs::write(&p, "").unwrap();
        assert!(load(&p).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 含中文的 GBK 配置必须按 GB2312 解码（oracle `ConfigFile.cs:263`）；严格 UTF-8 会读失败。
    #[test]
    fn parses_gbk_chinese_conf() {
        let dir = std::env::temp_dir().join(format!("lg-gbk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.conf");
        let body = crate::gbk::encode(
            "[LoginGate]\nTitle=热血传奇\n; 中文注释\nServerAddr0=127.0.0.1\nServerPort0=5500\n\
             GateAddr0=127.0.0.1\nGatePort0=7000\n",
        );
        assert!(
            std::str::from_utf8(&body).is_err(),
            "GBK 中文配置不应是合法 UTF-8"
        );
        std::fs::write(&p, &body).unwrap();
        let cfg = load(&p).unwrap();
        assert_eq!(cfg.srv_port, 5500);
        assert_eq!(cfg.gate_port, 7000);
        std::fs::remove_dir_all(&dir).ok();
    }
}
