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

/// 网关配置（只取 M1 必需项；其余键留给后续按需补）。
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
    let text = std::fs::read_to_string(path).with_context(|| {
        format!(
            "读取配置文件失败：{}（M1④：缺配置必须明确报错退出）",
            path.display()
        )
    })?;
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
    };
    if cfg.srv_port == 0 || cfg.gate_port == 0 {
        bail!("端口为 0 非法：{cfg:?}");
    }
    Ok(cfg)
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
}
