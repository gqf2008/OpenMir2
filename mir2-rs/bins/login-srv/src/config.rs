//! 遗留配置读取 —— 1:1 对齐 C# `src/LoginSrv/Conf/{Config,ConfigManager}.cs`
//! 与 `src/OpenMir2/Common/ConfigFile.cs` 的读取语义。
//!
//! 读取面（键名与文件名照现网，**一字不改**）：
//! - `logsrv.conf`（`AppServer.cs` 传入 `logsrv.conf`）：`[Server]` 段 + `[DataBase] ConnctionString`；
//! - `AddrTable.txt`：`ServerName Title Remote Public Gate[:Port]…`（GB2312）；
//! - `ServerAddr.txt`：允许连 5600 的服务端 IP 白名单；
//! - `UserLimit.txt`：`ServerName Name LimitCountMax`（GB2312）。
//!
//! 已登记的偏离（`tests/parity/whitelist.md`）：
//! - A-11：缺 `AddrTable.txt` / `UserLimit.txt` ⇒ **启动即报错退出**（C# 静默继续，
//!   结果是选服分支 NRE 或永不匹配的静默挂起）；依据＝ M1 验收④「缺配置必须明确报错退出」。
//! - A-12：`ServerAddr.txt` 的行过滤按 `!line.StartsWith(';')`（C# 写的是
//!   `sLineText[i] != ';'`，用**行号**当字符下标，属笔误）。

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use mir2_shared::hutil32;

use crate::gbk;

/// 一条 AddrTable 路由（C# `Conf/LsShare.cs::GateRoute`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateRoute {
    /// `ServerName`（GB2312 域字符串，来自 AddrTable 首列）。
    pub server_name: String,
    /// `Title`。
    pub title: String,
    /// `RemoteAddr`（登录网关的**连接来源**地址）。
    pub remote_addr: String,
    /// `PublicAddr`（对外发布的网关地址）。
    pub public_addr: String,
    /// `Gate[10]`（角色网关列表；Rust 侧与 C# 同形保留 10 槽）。
    pub gates: [TGateNet; 10],
}

/// C# `TGateNet`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TGateNet {
    /// IP 串。
    pub ipaddr: String,
    /// 端口。
    pub port: u16,
    /// 是否启用（`*` 前缀表示停用）。
    pub enable: bool,
}

/// C# `LimitServerUserInfo`（`UserLimit.txt` 一行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitServerUserInfo {
    /// 服务器名（`ServerName`）。
    pub server_name: String,
    /// 限流分组名（`Name`）。
    pub name: String,
    /// 运行期在线数（`LimitCountMin`，由 `RefServerLimit` 刷新）。
    pub limit_count_min: i32,
    /// 上限（`LimitCountMax`，缺省 3000）。
    pub limit_count_max: i32,
}

/// LoginSrv 运行期配置（C# `Config` 中本批用到的字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `[Server] GateAddr`（5500 监听地址）。
    pub gate_addr: String,
    /// `[Server] GatePort`。
    pub gate_port: u16,
    /// `[Server] ServerAddr`（5600 监听地址）。
    pub server_addr: String,
    /// `[Server] ServerPort`。
    pub server_port: u16,
    /// `[Server] TestServer`（C# `ConfigManager` 把它读进 `EnableMakingID`）。
    pub enable_making_id: bool,
    /// `[Server] DynamicIPMode`。
    pub dynamic_ip_mode: bool,
    /// `[DataBase] ConnctionString`（原拼写照抄）。
    pub conn_string: String,
    /// `AddrTable.txt` 解析出的路由（`RouteCount` = `routes.len()`）。
    pub routes: Vec<GateRoute>,
    /// `GenServerNameList` 去重后的服务器名列表。
    pub server_names: Vec<String>,
    /// `ServerAddr.txt` 白名单。
    pub server_addrs: Vec<String>,
    /// `UserLimit.txt`：与 C# 同形保留 100 槽（尾部为 `None`，用于复刻 NRE 语义）。
    pub user_limit: Vec<Option<LimitServerUserInfo>>,
}

impl Default for Config {
    /// C# `Config()` 构造函数的默认值（`Conf/Config.cs`）。
    fn default() -> Self {
        Self {
            gate_addr: "*".into(),
            gate_port: 5500,
            server_addr: "*".into(),
            server_port: 5600,
            enable_making_id: true,
            dynamic_ip_mode: false,
            conn_string: "server=127.0.0.1;uid=root;pwd=;database=mir2_account;".into(),
            routes: Vec::new(),
            server_names: Vec::new(),
            server_addrs: Vec::new(),
            user_limit: (0..100).map(|_| None).collect(),
        }
    }
}

/// 默认配置文件：可执行文件同目录的 `logsrv.conf`（C# `AppContext.BaseDirectory`）。
#[must_use]
pub fn default_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("logsrv.conf")
}

/// 读 `logsrv.conf` + 三个伴随文件；缺关键项即报错（M1④）。
///
/// # Errors
/// 配置文件/伴随文件不存在、缺 `ConnctionString`、端口非法 —— 返回错误，调用方非零退出。
pub fn load(path: &Path) -> Result<Config> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let text = std::fs::read_to_string(path).with_context(|| {
        format!(
            "读取配置失败：{}（M1④：缺配置必须明确报错退出）",
            path.display()
        )
    })?;
    let ini = Ini::parse(&text);
    let mut cfg = Config::default();
    // 与 C# `ConfigManager.LoadConfig` 同序、同名（大小写不敏感）。
    cfg.gate_addr = ini.string("Server", "GateAddr", &cfg.gate_addr);
    cfg.gate_port = ini.integer("Server", "GatePort", cfg.gate_port)?;
    cfg.server_addr = ini.string("Server", "ServerAddr", &cfg.server_addr);
    cfg.server_port = ini.integer("Server", "ServerPort", cfg.server_port)?;
    cfg.enable_making_id = ini.boolean("Server", "TestServer", cfg.enable_making_id);
    cfg.dynamic_ip_mode = ini.boolean("Server", "DynamicIPMode", cfg.dynamic_ip_mode);
    cfg.conn_string = ini.string("DataBase", "ConnctionString", &cfg.conn_string);
    if cfg.gate_port == 0 || cfg.server_port == 0 {
        bail!(
            "端口为 0 非法：GatePort={} ServerPort={}",
            cfg.gate_port,
            cfg.server_port
        );
    }
    load_addr_table(dir, &mut cfg)?;
    load_server_addr(dir, &mut cfg);
    load_user_limit(dir, &mut cfg)?;
    Ok(cfg)
}

/// `ConfigManager.LoadAddrTable` + `GenServerNameList`。
fn load_addr_table(dir: &Path, cfg: &mut Config) -> Result<()> {
    let file = dir.join("AddrTable.txt");
    let bytes = std::fs::read(&file).with_context(|| {
        format!(
            "读取 {} 失败（M1④/A-11：AddrTable 缺失则选服永不匹配）",
            file.display()
        )
    })?;
    let text = gbk::decode(&bytes);
    let mut n_route_idx = 0usize;
    for line in text.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let (server_name, rest) = hutil32::get_valid_str3(line, &[' ']);
        let (title, rest) = hutil32::get_valid_str3(&rest, &[' ']);
        let (remote, rest) = hutil32::get_valid_str3(&rest, &[' ']);
        let (public, mut rest) = hutil32::get_valid_str3(&rest, &[' ']);
        rest = rest.trim().to_string();
        if title.is_empty() || remote.is_empty() || public.is_empty() || n_route_idx >= 60 {
            continue;
        }
        let mut route = GateRoute {
            server_name,
            title,
            remote_addr: remote,
            public_addr: public,
            gates: std::array::from_fn(|_| TGateNet::default()),
        };
        let mut sel_gate_idx = 0usize;
        while !rest.is_empty() {
            if sel_gate_idx > 9 {
                break;
            }
            let (mut gate, after) = hutil32::get_valid_str3(&rest, &[' ']);
            rest = after.trim().to_string();
            if gate.is_empty() {
                continue;
            }
            let mut enable = true;
            if gate.starts_with('*') {
                gate = gate[1..].to_string();
                enable = false;
            }
            // C# `sGatePort = HUtil32.GetValidStr3(sGate, ref sGate, ':')`
            // ⇒ dest 是 ':' 前段（IP），返回值是端口串
            let (ip, port_str) = hutil32::get_valid_str3(&gate, &[':']);
            route.gates[sel_gate_idx] = TGateNet {
                ipaddr: ip,
                port: u16::try_from(hutil32::str_to_int(&port_str, 0)).unwrap_or(0),
                enable,
            };
            sel_gate_idx += 1;
        }
        cfg.routes.push(route);
        n_route_idx += 1;
    }
    // GenServerNameList：按出现顺序去重（C# 是 O(n²) 线性查重，等价于保序去重）。
    cfg.server_names.clear();
    for r in &cfg.routes {
        if !cfg.server_names.iter().any(|n| n == &r.server_name) {
            cfg.server_names.push(r.server_name.clone());
        }
    }
    Ok(())
}

/// `SessionServer.LoadServerAddr`（见 A-12）。
fn load_server_addr(dir: &Path, cfg: &mut Config) {
    let file = dir.join("ServerAddr.txt");
    let Ok(bytes) = std::fs::read(&file) else {
        log_err(&format!(
            "[Critical Failure] file not found. {}",
            file.display()
        ));
        return;
    };
    let text = gbk::decode(&bytes);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line.matches('.').count() == 3 {
            cfg.server_addrs.push(line.to_string());
            if cfg.server_addrs.len() >= 100 {
                break;
            }
        }
    }
}

/// `SessionServer.LoadUserLimit`（100 槽静态数组，尾部保持 null ⇒ 复刻 NRE 语义）。
fn load_user_limit(dir: &Path, cfg: &mut Config) -> Result<()> {
    let file = dir.join("UserLimit.txt");
    let bytes = std::fs::read(&file).with_context(|| {
        format!(
            "读取 {} 失败（M1④/A-11：缺 UserLimit 后 C# 会在选服分支 NRE 静默挂起）",
            file.display()
        )
    })?;
    let text = gbk::decode(&bytes);
    let mut n = 0usize;
    for line in text.lines() {
        let (server_name, rest) = hutil32::get_valid_str3(line, &[' ', '\t']);
        let (name, rest) = hutil32::get_valid_str3(&rest, &[' ', '\t']);
        let (max_str, _) = hutil32::get_valid_str3(&rest, &[' ', '\t']);
        if !server_name.is_empty() && n < cfg.user_limit.len() {
            cfg.user_limit[n] = Some(LimitServerUserInfo {
                server_name,
                name,
                limit_count_min: 0,
                limit_count_max: hutil32::str_to_int(&max_str, 3000),
            });
            n += 1;
        }
    }
    Ok(())
}

fn log_err(msg: &str) {
    crate::log_line("ERR", msg);
}

/// `ConfigFile` 的读取面（只读，不回写；键与段名大小写不敏感，重复键取先出现者）。
#[derive(Debug, Default)]
struct Ini {
    sections: Vec<(String, Vec<(String, String)>)>,
}

impl Ini {
    /// 镜像 `ConfigFile.Load`：`;` 行注释、`/* */` 块注释、`;;` 行内注释、
    /// `[sec]` 段（**重复段出现即终止整个读取**）、`k=v` 取第一个 '=' 且 pos>0、首键优先。
    fn parse(text: &str) -> Self {
        let mut ini = Ini::default();
        let mut large_comment = false;
        let mut cur: Option<usize> = None;
        let mut cur_sec_comment = false;
        for raw in text.lines() {
            let line = raw.trim();
            if line.starts_with(';') {
                continue;
            }
            if line.is_empty() {
                continue;
            }
            if line.starts_with("/*") {
                large_comment = true;
                continue;
            }
            if large_comment {
                if line.starts_with("*/") {
                    large_comment = false;
                }
                continue;
            }
            if line.starts_with('[') {
                let Some(sec) = line.split_once(']').map(|(a, _)| a[1..].trim().to_string()) else {
                    continue;
                };
                if sec.is_empty() {
                    continue;
                }
                if sec.starts_with(";;") {
                    cur_sec_comment = true;
                    continue;
                }
                cur_sec_comment = false;
                if ini
                    .sections
                    .iter()
                    .any(|(s, _)| s.eq_ignore_ascii_case(&sec))
                {
                    break; // C# 走到 Label_02AE：重复段直接结束读取
                }
                ini.sections.push((sec, Vec::new()));
                cur = Some(ini.sections.len() - 1);
                continue;
            }
            if cur_sec_comment {
                continue;
            }
            let line = match line.find(";;") {
                Some(i) => line[..i].trim(),
                None => line,
            };
            let Some(eq) = line.find('=') else {
                continue;
            };
            if eq == 0 {
                continue;
            }
            let key = line[..eq].trim().to_string();
            let val = line[eq + 1..].trim().to_string();
            if key.is_empty() {
                continue;
            }
            let idx = if let Some(i) = cur {
                i
            } else {
                ini.sections.push((String::new(), Vec::new()));
                cur = Some(ini.sections.len() - 1);
                ini.sections.len() - 1
            };
            let sec = &mut ini.sections[idx].1;
            if !sec.iter().any(|(k, _)| k.eq_ignore_ascii_case(&key)) {
                sec.push((key, val));
            }
        }
        ini
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        let (_, items) = self
            .sections
            .iter()
            .find(|(s, _)| s.eq_ignore_ascii_case(section))?;
        items
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// `LoadConfigString`：键缺失或值为空 ⇒ 默认值。
    fn string(&self, section: &str, key: &str, default: &str) -> String {
        match self.get(section, key) {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => default.to_string(),
        }
    }

    /// `LoadConfigInteger`：`ReadWriteInteger(def=-1)` 后 `< 0` ⇒ 默认值（即负值视为缺省）。
    fn integer(&self, section: &str, key: &str, default: u16) -> Result<u16> {
        let loaded = match self.get(section, key) {
            Some(v) if !v.is_empty() => hutil32::str_to_int(v, -1),
            _ => -1,
        };
        if loaded < 0 {
            return Ok(default);
        }
        u16::try_from(loaded)
            .map_err(|_| anyhow::anyhow!("配置 [{section}] {key}={loaded} 超出端口范围"))
    }

    /// `LoadConfigBoolean`：整数 == 1 ⇒ true（负数/缺省 ⇒ 默认值）。
    fn boolean(&self, section: &str, key: &str, default: bool) -> bool {
        let loaded = match self.get(section, key) {
            Some(v) if !v.is_empty() => hutil32::str_to_int(v, -1),
            _ => -1,
        };
        if loaded < 0 {
            default
        } else {
            loaded == 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fixture(dir: &Path, conf: &str, addr: &str, saddr: &str, limit: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("logsrv.conf"), conf).unwrap();
        std::fs::write(dir.join("AddrTable.txt"), gbk::encode(addr)).unwrap();
        std::fs::write(dir.join("ServerAddr.txt"), saddr).unwrap();
        std::fs::write(dir.join("UserLimit.txt"), gbk::encode(limit)).unwrap();
        dir.join("logsrv.conf")
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lsrv-cfg-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    /// 现网两份样件（`E:\MirServer\LoginSrv\*` 与仓库构建输出）都要能吃。
    #[test]
    fn parses_deployed_shaped_config() {
        let d = tmp("deployed");
        let conf = write_fixture(
            &d,
            "[Server]\nEnableMakingID=1\nTestServer=1\nGateAddr=0.0.0.0\nGatePort=5500\n\
             ServerAddr=127.0.0.1\nServerPort=5600\nDynamicIPMode=0\n\
             ; comment\n[DataBase]\nDbType=MySQL\n\
             ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_account;\n",
            "热血传奇 Title1 127.0.0.1 127.0.0.1 127.0.0.1:7100\n\
             热血传奇 Title1 127.0.0.1 127.0.0.1 127.0.0.1:7100\n",
            "10.10.0.163\n127.0.0.1\n",
            "热血传奇 热血传奇 10000\n",
        );
        let cfg = load(&conf).unwrap();
        assert_eq!(cfg.gate_addr, "0.0.0.0");
        assert_eq!(cfg.gate_port, 5500);
        assert_eq!(cfg.server_port, 5600);
        assert!(cfg.enable_making_id);
        assert!(!cfg.dynamic_ip_mode);
        assert_eq!(cfg.routes.len(), 2);
        assert_eq!(cfg.server_names, vec!["热血传奇".to_string()]);
        assert_eq!(cfg.routes[0].gates[0].ipaddr, "127.0.0.1");
        assert_eq!(cfg.routes[0].gates[0].port, 7100);
        assert!(cfg.routes[0].gates[0].enable);
        assert_eq!(cfg.server_addrs, vec!["10.10.0.163", "127.0.0.1"]);
        assert_eq!(cfg.user_limit.len(), 100);
        let ul = cfg.user_limit[0].as_ref().unwrap();
        assert_eq!(ul.server_name, "热血传奇");
        assert_eq!(ul.limit_count_max, 10000);
        assert!(cfg.user_limit[1].is_none());
        std::fs::remove_dir_all(&d).ok();
    }

    /// 停用的角色网关（`*` 前缀）与多网关行。
    #[test]
    fn parses_disabled_gate_and_multi_gate() {
        let d = tmp("gates");
        let conf = write_fixture(
            &d,
            "[Server]\nGatePort=5500\nServerPort=5600\n[DataBase]\nConnctionString=x\n",
            "热血传奇 T1 127.0.0.1 127.0.0.1 *10.0.0.9:7100 127.0.0.1:7200\n",
            "127.0.0.1\n",
            "热血传奇 g1 3000\n",
        );
        let cfg = load(&conf).unwrap();
        let g = &cfg.routes[0].gates;
        assert_eq!(g[0].ipaddr, "10.0.0.9");
        assert!(!g[0].enable);
        assert_eq!(g[1].ipaddr, "127.0.0.1");
        assert_eq!(g[1].port, 7200);
        assert!(g[1].enable);
        assert!(g[2].ipaddr.is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    /// 缺 AddrTable / UserLimit / 文件不存在 ⇒ 报错（A-11，M1④）。
    #[test]
    fn missing_companion_files_are_fatal() {
        let d = tmp("missing");
        let conf = write_fixture(
            &d,
            "[Server]\nGatePort=5500\nServerPort=5600\n[DataBase]\nConnctionString=x\n",
            "热血传奇 T1 127.0.0.1 127.0.0.1 127.0.0.1:7100\n",
            "127.0.0.1\n",
            "热血传奇 g1 3000\n",
        );
        assert!(load(&conf).is_ok());
        std::fs::remove_file(d.join("AddrTable.txt")).unwrap();
        assert!(load(&conf).is_err(), "缺 AddrTable 必须报错");
        std::fs::write(
            d.join("AddrTable.txt"),
            gbk::encode("热血传奇 T1 127.0.0.1 127.0.0.1 127.0.0.1:7100\n"),
        )
        .unwrap();
        std::fs::remove_file(d.join("UserLimit.txt")).unwrap();
        assert!(load(&conf).is_err(), "缺 UserLimit 必须报错");
        assert!(load(&d.join("nosuch.conf")).is_err());
        std::fs::remove_dir_all(&d).ok();
    }

    /// 缺ServerAddr.txt 只告警不致命（C# 侧同样只有白名单为空的效果）。
    #[test]
    fn missing_server_addr_is_not_fatal() {
        let d = tmp("no-saddr");
        let conf = write_fixture(
            &d,
            "[Server]\nGatePort=5500\nServerPort=5600\n",
            "热血传奇 T1 127.0.0.1 127.0.0.1 127.0.0.1:7100\n",
            "127.0.0.1\n",
            "热血传奇 g1 3000\n",
        );
        std::fs::remove_file(d.join("ServerAddr.txt")).unwrap();
        let cfg = load(&conf).unwrap();
        assert!(cfg.server_addrs.is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    /// INI 语义锚点：`;;` 行内注释、首键优先、重复段终止读取、负数端口回落默认。
    #[test]
    fn ini_semantics() {
        let ini = Ini::parse(
            "[Server]\nGatePort=7000\nGatePort=8000\nServerPort=-5\nGateAddr=1.2.3.4 ;; 注释\n\
             [DataBase]\nConnctionString=a=b\n[Server]\nGateAddr=9.9.9.9\n",
        );
        assert_eq!(ini.string("server", "GateAddr", "d"), "1.2.3.4");
        assert_eq!(ini.string("server", "gateport", "d"), "7000");
        assert_eq!(ini.string("database", "ConnctionString", "d"), "a=b");
        assert_eq!(ini.integer("Server", "ServerPort", 5600).unwrap(), 5600);
        assert_eq!(ini.integer("Server", "GatePort", 5500).unwrap(), 7000);
        let ini2 = Ini::parse("[Server]\nGatePort=0\n");
        assert_eq!(ini2.integer("Server", "GatePort", 5500).unwrap(), 0);
        assert!(ini2.boolean("Server", "TestServer", true));
    }
}
