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

/// `[Integer] NomClientPacketSize` 的缺省值（照 C# `ConfigManager.cs:99` 的 `ReadWriteInt` 带默认口径）。
pub const DEFAULT_NOM_CLIENT_PACKET_SIZE: usize = 400;

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
    /// `[Switch] ChatInterval`（C# `Config.IsChatInterval`；现网 `=1` ⇒ 开）。缺省按现网/默认 `true`。
    pub chat_interval_enabled: bool,
    /// `[Integer] ChatInterval`（C# `Config.ChatInterval`；现网 `=800`）。缺省 800。
    pub chat_interval_ms: i64,
    /// `[Integer] NomClientPacketSize`（C# `ConfigManager.cs:99` 的 `ReadWriteInt(..., 默认)`；
    /// 现网 `=400`）：登录后超过它的帧按 `ClientSession.cs:196-199` **只丢帧、不关会话**。
    /// 缺省 400 —— 照 oracle 的"读配置带默认"口径，**不**硬编码。
    pub nom_client_packet_size: usize,
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
    // 按字节读再 **GB2312 解码**，照 oracle `ConfigFile.Load()`
    //（`src/OpenMir2/Common/ConfigFile.cs:258` 的 `Encoding.GetEncoding("GB2312")`）。
    // 现网 `E:\MirServer\RunGate\config.conf` 经 C# `ConfigFile.Save()`（同样 GB2312 回写）后
    // **不是合法 UTF-8**：`[Strings]` 段混有跨编码字节（例：`Title=游戏网关` 是 UTF-8 字节，
    // 而 `HWIDBlockedMsg=...` 是 GBK 字节），`read_to_string` 会直接失败 ⇒ `-Side rust`
    // 连 7200 都起不来。本文件实际只消费 `[GameGate]/[Switch]/[Integer]`（纯 ASCII，
    // GB2312 与 UTF-8 判定相同），但解码必须与 oracle 同口径，否则连读都读不进来。
    let bytes = std::fs::read(path).with_context(|| {
        format!(
            "读取配置文件失败：{}（M1④：缺配置必须明确报错退出）",
            path.display()
        )
    })?;
    let text = crate::gbk::decode(&bytes);
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
    // `[Switch]` / `[Integer]`：键名照 `ConfigManager.cs:140` 与 `:118` 的读法（同一把键名出现在两个段里）。
    let switch = parse_section(&text, "Switch");
    let integer = parse_section(&text, "Integer");
    let get_in = |sec: &[(String, String)], key: &str| -> Option<String> {
        sec.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.clone())
    };
    let chat_interval_enabled = get_in(&switch, "ChatInterval").is_none_or(|v| v != "0");
    let chat_interval_ms = get_in(&integer, "ChatInterval")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(800);
    // oracle 是 `ReadWriteInt("Integer","NomClientPacketSize", config.NomClientPacketSize)` ⇒ 缺键取默认
    let nom_client_packet_size = get_in(&integer, "NomClientPacketSize")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_NOM_CLIENT_PACKET_SIZE);
    Ok(GateConfig {
        endpoints,
        chat_interval_enabled,
        chat_interval_ms,
        nom_client_packet_size,
    })
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

    /// 现网 `RunGate/config.conf` 经 C# `Save()` 回写后**不是合法 UTF-8**（`[Strings]` 段混编码），
    /// 必须按 GB2312 读进来；用 `read_to_string` 会 `stream did not contain valid UTF-8` 直接起不来。
    /// 这里造一个含非法 UTF-8 字节（0xB0 0xA1 = GBK「啊」）的现网同形文件，断言仍能解析。
    #[test]
    fn loads_non_utf8_deployed_conf() {
        let dir = std::env::temp_dir().join(format!("gg-cfg-nonutf8-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.conf");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(
            b"[GameGate]\nCount=3\nServerAddr1=127.0.0.1\nServerPort1=5000\nGatePort1=7200\n\
              ServerAddr2=127.0.0.1\nServerPort2=5000\nGatePort2=7201\n\
              ServerAddr3=127.0.0.1\nServerPort3=5000\nGatePort3=7202\nGateAddress1=127.0.0.1\n",
        );
        bytes.extend_from_slice(b"[Strings]\nHWIDBlockedMsg=");
        bytes.extend_from_slice(&[0xB0, 0xA1]); // GBK「啊」——不是合法 UTF-8
        bytes.push(b'\n');
        bytes.extend_from_slice(b"[Integer]\nNomClientPacketSize=512\n");
        std::fs::write(&p, &bytes).unwrap();
        let cfg = load(&p).expect("含 GBK 字节的现网同形配置必须能读");
        assert_eq!(cfg.endpoints.len(), 3);
        assert_eq!(cfg.endpoints[2].gate_port, 7202);
        assert_eq!(cfg.nom_client_packet_size, 512);
    }

    /// `NomClientPacketSize` 照 oracle 的"读配置带默认"口径（`ConfigManager.cs:99` 的 ReadWriteInt）：
    /// 给了就用配置值，缺了取默认 400 —— **不许硬编码**（reviewer-3 在 f46f8b45 的 Minor #2）。
    #[test]
    fn nom_client_packet_size_comes_from_config_with_default() {
        let with_key =
            "[GameGate]\nCount=1\nServerAddr1=127.0.0.1\nServerPort1=5000\nGatePort1=7200\n\
                        [Integer]\nNomClientPacketSize=512\n";
        assert_eq!(
            load(&write_conf(with_key)).unwrap().nom_client_packet_size,
            512
        );
        let without =
            "[GameGate]\nCount=1\nServerAddr1=127.0.0.1\nServerPort1=5000\nGatePort1=7201\n";
        assert_eq!(
            load(&write_conf(without)).unwrap().nom_client_packet_size,
            DEFAULT_NOM_CLIENT_PACKET_SIZE,
            "缺键取默认（oracle 的 ReadWriteInt 带默认），不是报错也不是 0"
        );
    }
}
