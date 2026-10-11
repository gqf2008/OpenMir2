//! GameGate 的遗留 INI 配置读取（`config.conf`），键名与现网 `E:\MirServer\RunGate\config.conf` 一致，
//! **缺失即报错**（照 M1 验收④的"失败要响"口径）。
//!
//! **编码按 oracle 走 GB2312**：C# `src/OpenMir2/Common/ConfigFile.cs:263` 是
//! `new StreamReader(File.Open(...), Encoding.GetEncoding("GB2312"))`（写也用 GB2312）。
//! 现网 `E:\MirServer\RunGate\config.conf` 含中文（`[Strings]`/`[MagicInterval]`），
//! **不是合法 UTF-8**，旧实现用 `std::fs::read_to_string`（严格 UTF-8）会直接失败
//! （M3-Rust 首验 7200 起不来）；`from_utf8_lossy` 同样不是 oracle 语义。
//! 这里复用本 crate 的 `gbk::decode`（`encoding_rs::GBK` ≈ cp936，差异见 whitelist A-14）。
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
    // 照 oracle 先读**原始字节**再按 **GB2312** 解码（`ConfigFile.cs:263` 的 StreamReader 口径）。
    // 严格 UTF-8 会在现网配置上失败；lossy UTF-8 会把 GBK 中文吃成替换符，都不是 oracle 语义。
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

    /// **GBK 夹具回归**：现网 `E:\MirServer\RunGate\config.conf` 的逐字节副本
    /// （`tests/fixtures/rungate-config.conf`，含 `[Strings]`/`[MagicInterval]` 中文 ⇒ **非 UTF-8**）。
    /// oracle（`ConfigFile.cs:263` 的 GB2312 StreamReader）能读；旧实现 `read_to_string` 在此红。
    #[test]
    fn parses_deployed_gbk_conf_fixture() {
        const DEPLOYED: &[u8] = include_bytes!("../tests/fixtures/rungate-config.conf");
        let deployed = DEPLOYED.to_vec();
        assert!(
            std::str::from_utf8(&deployed).is_err(),
            "夹具必须含非 UTF-8 字节（否则红检无意义）"
        );
        let dir = std::env::temp_dir().join(format!("gg-gbk-fixture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.conf");
        std::fs::write(&p, &deployed).unwrap();
        let cfg = load(&p).unwrap();
        // 逐项对齐 C# `ConfigManager.LoadConfig` 在现网 config.conf 上的取值。
        let ep = |gate_port: u16| Endpoint {
            srv_addr: "127.0.0.1".into(),
            srv_port: 5000,
            gate_addr: "127.0.0.1".into(),
            gate_port,
        };
        assert_eq!(cfg.endpoints, vec![ep(7200), ep(7201), ep(7202)]);
        assert!(cfg.chat_interval_enabled, "[Switch] ChatInterval=1");
        assert_eq!(cfg.chat_interval_ms, 800, "[Integer] ChatInterval=800");
        assert_eq!(
            cfg.nom_client_packet_size, 400,
            "[Integer] NomClientPacketSize=400"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 中文注释/中文值必须走 GB2312 解码（oracle `ConfigFile.cs:263`）：用真 GBK 字节构造。
    #[test]
    fn decodes_gbk_comments_and_values() {
        let body = crate::gbk::encode(
            "[GameGate]\n; GBK 注释：这一行不是合法 UTF-8\nCount=1\n\
             ServerAddr1=127.0.0.1\nServerPort1=5000\nGatePort1=7200\nGateAddress1=0.0.0.0\n\
             [Integer]\n; 封包上限\nNomClientPacketSize=512\nChatInterval=800\n",
        );
        assert!(
            std::str::from_utf8(&body).is_err(),
            "GBK 中文注释字节不应是合法 UTF-8"
        );
        let dir = std::env::temp_dir().join(format!("gg-gbk-inline-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.conf");
        std::fs::write(&p, &body).unwrap();
        let cfg = load(&p).unwrap();
        assert_eq!(cfg.endpoints.len(), 1);
        assert_eq!(cfg.endpoints[0].gate_port, 7200);
        assert_eq!(cfg.nom_client_packet_size, 512);
        assert_eq!(cfg.chat_interval_ms, 800);
        std::fs::remove_dir_all(&dir).ok();
    }
}
