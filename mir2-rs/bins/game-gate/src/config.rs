//! GameGate 的遗留 INI 配置读取（`config.conf`），键名与现网 `E:\MirServer\RunGate\config.conf` 一致，
//! **缺失即报错**（照 M1 验收④的"失败要响"口径）。
//!
//! 现网样件（前 13 行）：
//! ```ini
//! [GameGate]
//! Count=3
//! ServerAddr1=127.0.0.1
//! ServerPort1=5000
//! GatePort1=7200
//! ... ServerAddr2/ServerPort2/GatePort2=7201、ServerAddr3/ServerPort3/GatePort3=7202 同形
//! GateAddress1=127.0.0.1
//! ```
//! 注意两点（照现网事实，不照直觉）：
//! 1. 监听地址键是 **`GateAddressN`**（不是 `GateAddrN`），且现网**只写了 `GateAddress1`**
//!    ⇒ 其余下标回退到 `GateAddress1`，再回退到 `0.0.0.0`；
//! 2. `Count` 决定有几组 (ServerAddrN/ServerPortN/GatePortN)，与 `ClientThread.SessionArray` 无关。

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// 一组"网关→GameSvr 连接 + 对客户端监听"的端点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// 下游 GameSvr 地址（`ServerAddrN`）。
    pub srv_addr: String,
    /// 下游 GameSvr 端口（`ServerPortN`，现网 5000）。
    pub srv_port: u16,
    /// 对外监听地址（`GateAddressN`，缺省回退 `GateAddress1`，再回退 `0.0.0.0`）。
    pub gate_addr: String,
    /// 对外监听端口（`GatePortN`，现网 7200/7201/7202）。
    pub gate_port: u16,
}

/// 网关配置：`Count` 组端点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateConfig {
    pub endpoints: Vec<Endpoint>,
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
/// 文件不存在、缺 `[GameGate]` 段、`Count` 非法/为 0、缺任一 `ServerAddrN`/`ServerPortN`/`GatePortN`、
/// 端口非法 —— 都返回错误（调用方据此非零退出）。
pub fn load(path: &Path) -> Result<GateConfig> {
    let text = std::fs::read_to_string(path).with_context(|| {
        format!(
            "读取配置文件失败：{}（M1④：缺配置必须明确报错退出）",
            path.display()
        )
    })?;
    let section = parse_section(&text, "GameGate");
    let get = |key: &str| -> Option<String> {
        section
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.clone())
    };
    let count: usize = get("Count").map_or(1, |v| v.parse::<usize>().unwrap_or(0));
    if count == 0 {
        bail!("配置 Count 非法（必须 ≥ 1）");
    }
    let gate_addr_default = get("GateAddress1").unwrap_or_else(|| "0.0.0.0".to_string());
    let mut endpoints = Vec::with_capacity(count);
    for n in 1..=count {
        let srv_addr = get(&format!("ServerAddr{n}"))
            .with_context(|| format!("配置缺少 [GameGate] ServerAddr{n}（M1④）"))?;
        let port = |key: String| -> Result<u16> {
            let raw = get(&key).with_context(|| format!("配置缺少 [GameGate] {key}（M1④）"))?;
            let p = raw
                .parse::<u16>()
                .map_err(|_| anyhow::anyhow!("配置 {key} 端口非法：{raw:?}"))?;
            if p == 0 {
                bail!("配置 {key} 端口为 0 非法");
            }
            Ok(p)
        };
        endpoints.push(Endpoint {
            srv_addr,
            srv_port: port(format!("ServerPort{n}"))?,
            gate_addr: get(&format!("GateAddress{n}")).unwrap_or_else(|| gate_addr_default.clone()),
            gate_port: port(format!("GatePort{n}"))?,
        });
    }
    Ok(GateConfig { endpoints })
}

/// 取 `[section]` 下的键值对（大小写不敏感；`;`/`#` 起头为注释；去掉行尾注释）。
///
/// 与 `bins/login-gate/src/config.rs` 的同名函数同形（同一份 INI 方言）。
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

    fn write_conf(body: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gg-cfg-{}-{}", std::process::id(), body.len()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.conf");
        std::fs::write(&p, body).unwrap();
        p
    }

    /// 现网样件（前 13 行）必须解析出 3 组端点，且 2/3 的监听地址回退到 GateAddress1。
    #[test]
    fn parses_deployed_gamegate_conf() {
        let p = write_conf(
            "[GameGate]\nCount=3\nServerAddr1=127.0.0.1\nServerPort1=5000\nGatePort1=7200\n\
             ServerAddr2=127.0.0.1\nServerPort2=5000\nGatePort2=7201\n\
             ServerAddr3=127.0.0.1\nServerPort3=5000\nGatePort3=7202\nGateAddress1=127.0.0.1\n",
        );
        let cfg = load(&p).unwrap();
        assert_eq!(cfg.endpoints.len(), 3);
        assert_eq!(cfg.endpoints[0].gate_port, 7200);
        assert_eq!(cfg.endpoints[1].gate_port, 7201);
        assert_eq!(cfg.endpoints[2].gate_port, 7202);
        assert_eq!(cfg.endpoints[0].srv_port, 5000);
        for e in &cfg.endpoints {
            assert_eq!(e.gate_addr, "127.0.0.1", "2/3 应回退到 GateAddress1");
        }
    }

    /// 缺键/Count 非法必须报错（"失败要响"）。
    #[test]
    fn missing_or_bad_keys_are_fatal() {
        for body in [
            "[GameGate]\nCount=1\nServerAddr1=127.0.0.1\nServerPort1=5000\n", // 缺 GatePort1
            "[GameGate]\nCount=1\nServerAddr1=127.0.0.1\nServerPort1=abc\nGatePort1=7200\n", // 端口非法
            "[GameGate]\nCount=0\n", // Count 非法
            "[Other]\nCount=1\n",    // 缺段
        ] {
            let p = write_conf(body);
            assert!(load(&p).is_err(), "应在该配置上报错：{body:?}");
        }
    }
}
