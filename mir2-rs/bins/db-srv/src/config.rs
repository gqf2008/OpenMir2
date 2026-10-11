//! DBSrv 的遗留 INI 配置读取（`dbsvr.conf`），键名照现网，**缺关键项即报错**（M1 验收④）。
//!
//! 取值口径逐条对齐 C# `DBSrv/Conf/{SettingsModel,AppConfigManager}.cs` + `Common/ConfigFile.cs`：
//! - 文件按 **GB2312** 读，键值 `Trim()`，行首分号整行注释、`;;` 起行尾注释；同名键**首个生效**；
//!   段名大小写不敏感。本实现按**字节**读文件（保留原始 GBK 字节，供 `(103/ServerName/…)` 原样上线），
//!   同时给出文本视图（ASCII/UTF-8 配置就是正确文本，GBK 配置的文本视图仅用于日志）。
//! - 缺键时 C# 吃 `SettingsModel` 默认值；本实现同默认值。
//! - **不写回**：C# 的 `ReadWriteXxx` 在缺键时把默认值写回配置文件（`Save()`）；写回是对冻结配置
//!   文件的副作用，本实现只读。差异登记 `tests/parity/whitelist.md` A-10。
//! - `GateAddr` 默认 `"*"`，C# `IPAddress.Parse("*")` 抛异常 ⇒ 启动失败；本实现显式报错，语义等价。
//! - `Storeage`（原拼写照抄）是存储插件名；C# 找不到 `DBSrv.Storage.<Storeage>.dll` 即抛异常。
//!   本实现只实现 MySQL 面，非 MySQL 一律明确报错退出。

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// 配置里的一个值：保留**原始字节**（配置是 GB2312 编码，GBK 字节就是要上线的字节）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawStr(Vec<u8>);

impl RawStr {
    /// 原始字节（上线用）。
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    /// 文本视图（ASCII/UTF-8 配置即正确文本；GBK 配置只是给日志看的近似）。
    #[must_use]
    pub fn text(&self) -> String {
        match std::str::from_utf8(&self.0) {
            Ok(s) => s.to_string(),
            Err(_) => String::from_utf8_lossy(&self.0).into_owned(),
        }
    }

    /// 去掉首尾 ASCII 空白后的文本。
    #[must_use]
    pub fn trimmed_text(&self) -> String {
        self.text().trim().to_string()
    }
}

/// DBSrv 配置（M1 取 5100 面 + LoginSrv 会话链所需的键）。
#[derive(Debug, Clone)]
pub struct DbConfig {
    /// `[Setup] ServerName`（给 LoginSrv 的 `(103/<ServerName>/99/…)` 用原始字节）。
    pub server_name: RawStr,
    /// `[Setup] ShowDebug`（决定 `LogService.Debug` 是否输出，与 C# 同一开关）。
    pub show_debug: bool,
    /// `[Setup] ServerAddr` —— 6000（DataService，GameSvr 取档/存档）绑定地址（现网 `0.0.0.0`）。
    pub server_addr: String,
    /// `[Setup] ServerPort` —— 6000。
    pub server_port: u16,
    /// `[Setup] GateAddr` —— 5100 绑定地址（现网 `0.0.0.0`）。
    pub gate_addr: String,
    /// `[Setup] GatePort` —— 5100。
    pub gate_port: u16,
    /// `[Server] MarketAddr` —— 5700（拍卖行）绑定地址（现网 `127.0.0.1`）。
    ///
    /// 5700 `MarketService` 属 S4b 的未做完子片（见交付说明），读写留待该片；此处仅按现网键名解析+校验。
    #[allow(dead_code)]
    pub market_addr: String,
    /// `[Server] MarketPort` —— 5700。
    #[allow(dead_code)]
    pub market_port: u16,
    /// `[Setup] DELMaxLevel` —— 可删除角色的等级上限（C# 字段名 `DeleteMinLevel`）。
    pub delete_min_level: i32,
    /// `[Setup] DynamicIPMode`。
    pub dynamic_ip_mode: bool,
    /// `[Setup] MapFile` —— `MapInfo.txt` 路径（缺省空 ⇒ 不做地图编号换算）。
    pub map_file: RawStr,
    /// `[Setup] EnglishNameOnly`。
    pub english_names: bool,
    /// `[Server] IDSAddr` —— LoginSrv 地址（现网 `127.0.0.1`）。
    pub ids_addr: String,
    /// `[Server] IDSPort` —— LoginSrv 端口（现网 `5600`）。
    pub ids_port: u16,
    /// `[DataBase] Storeage` —— 存储插件名。
    pub storeage: RawStr,
    /// `[DataBase] ConnctionString`（原拼写照抄）—— MySQL 连接串。
    pub conn_string: RawStr,
}

/// 默认配置文件：可执行文件同目录的 `dbsvr.conf`（与 C# `AppContext.BaseDirectory` 一致）。
#[must_use]
pub fn default_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dbsvr.conf")
}

/// 读取并校验配置。
///
/// # Errors
/// 文件不可读、`GateAddr` 缺失/非法、`GatePort` 缺失/非法/为 0、`Storeage` 非 MySQL、
/// 连接串缺失 —— 都返回错误（调用方据此非零退出，M1 验收④）。
pub fn load(path: &Path) -> Result<DbConfig> {
    let raw = std::fs::read(path).with_context(|| {
        format!(
            "读取配置文件失败：{}（M1④：缺配置必须明确报错退出）",
            path.display()
        )
    })?;
    let ini = Ini::parse(&raw);
    let s = |sec: &str, key: &str, def: &str| -> RawStr {
        ini.get(sec, key)
            .map_or_else(|| RawStr(def.as_bytes().to_vec()), |v| RawStr(v.to_vec()))
    };
    let i = |sec: &str, key: &str, def: i32| -> i32 {
        ini.get(sec, key)
            .and_then(|v| std::str::from_utf8(v).ok())
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(def)
    };
    let b = |sec: &str, key: &str, def: bool| -> bool {
        let Some(v) = ini.get(sec, key) else {
            return def;
        };
        match String::from_utf8_lossy(v).trim().to_uppercase().as_str() {
            "" => def,
            "是" | "YES" | "1" | "TRUE" => true,
            _ => false,
        }
    };

    let server_addr = s("Setup", "ServerAddr", "*").trimmed_text();
    if server_addr.is_empty() || server_addr == "*" {
        bail!(
            "配置缺 [Setup] ServerAddr（C# 默认值 \"*\" 会让 IPAddress.Parse 抛异常 ⇒ 启动失败）：{server_addr:?}"
        );
    }
    let server_port = i("Setup", "ServerPort", 6000);
    if server_port <= 0 || server_port > i32::from(u16::MAX) {
        bail!("配置 [Setup] ServerPort 非法：{server_port}");
    }
    let gate_addr = s("Setup", "GateAddr", "*").trimmed_text();
    if gate_addr.is_empty() || gate_addr == "*" {
        bail!(
            "配置缺 [Setup] GateAddr（C# 默认值 \"*\" 会让 IPAddress.Parse 抛异常 ⇒ 启动失败）：{gate_addr:?}"
        );
    }
    let gate_port = i("Setup", "GatePort", 5100);
    if gate_port <= 0 || gate_port > i32::from(u16::MAX) {
        bail!("配置 [Setup] GatePort 非法：{gate_port}");
    }
    let storeage = s("DataBase", "Storeage", "MySQL");
    if storeage.trimmed_text() != "MySQL" {
        bail!(
            "存储策略 {} 未实现（C# 找不到 DBSrv.Storage.{}.dll 即启动失败；本实现只实现 MySQL）",
            storeage.text(),
            storeage.text()
        );
    }
    let conn_string = s(
        "DataBase",
        "ConnctionString",
        "server=127.0.0.1;uid=root;pwd=;database=mir2_db;",
    );
    if conn_string.bytes().is_empty() {
        bail!("配置缺 [DataBase] ConnctionString（M1④）");
    }
    let ids_port = i("Server", "IDSPort", 5600);
    if ids_port <= 0 || ids_port > i32::from(u16::MAX) {
        bail!("配置 [Server] IDSPort 非法：{ids_port}");
    }
    let market_port = i("Server", "MarketPort", 5700);
    if market_port <= 0 || market_port > i32::from(u16::MAX) {
        bail!("配置 [Server] MarketPort 非法：{market_port}");
    }
    Ok(DbConfig {
        server_name: s("Setup", "ServerName", "热血传奇"),
        show_debug: b("Setup", "ShowDebug", false),
        server_addr,
        server_port: u16::try_from(server_port).unwrap_or(6000),
        market_addr: s("Server", "MarketAddr", "127.0.0.1").trimmed_text(),
        market_port: u16::try_from(market_port).unwrap_or(5700),
        gate_addr,
        gate_port: u16::try_from(gate_port).unwrap_or(5100),
        delete_min_level: i("Setup", "DELMaxLevel", 30),
        dynamic_ip_mode: b("Setup", "DynamicIPMode", false),
        map_file: s("Setup", "MapFile", ""),
        english_names: b("Setup", "EnglishNameOnly", false),
        ids_addr: s("Server", "IDSAddr", "127.0.0.1").trimmed_text(),
        ids_port: u16::try_from(ids_port).unwrap_or(5600),
        storeage,
        conn_string,
    })
}

/// 段表：`段名 → [(键, 值)]`（字节级，镜像 C# `ConfigFile.Load`）。
type Sections = Vec<(Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>)>;

/// 极简 INI 解析（字节级，镜像 C# `ConfigFile.Load`）。
struct Ini(Sections);

impl Ini {
    fn parse(raw: &[u8]) -> Self {
        let mut sections: Sections = Vec::new();
        let mut cur: Option<usize> = None;
        let mut in_block_comment = false;
        for line in raw.split(|b| *b == b'\n') {
            let line = trim_bytes(line);
            if line.is_empty() || line.starts_with(b";") {
                continue;
            }
            if line.starts_with(b"/*") {
                in_block_comment = true;
                continue;
            }
            if in_block_comment {
                if line.starts_with(b"*/") {
                    in_block_comment = false;
                }
                continue;
            }
            if line[0] == b'[' {
                if let Some(inner) = sec_name(line) {
                    if inner.starts_with(b";;") {
                        cur = None; // `[;;…]` 段注释：后续行全部忽略
                        continue;
                    }
                    sections.push((inner, Vec::new()));
                    cur = Some(sections.len() - 1);
                    continue;
                }
            }
            let Some(eq) = line.iter().position(|b| *b == b'=') else {
                continue;
            };
            if eq == 0 {
                continue;
            }
            let key = trim_bytes(&line[..eq]).to_vec();
            let mut val = trim_bytes(&line[eq + 1..]).to_vec();
            if let Some(pos) = find_sub(&val, b";;") {
                val.truncate(pos);
                val = trim_bytes(&val).to_vec();
            }
            if key.is_empty() {
                continue;
            }
            // 无段名的行 C# 落到 "" 段；现网配置都有段，这里直接忽略（已在上方 cur=None 语义内）
            let Some(idx) = cur else { continue };
            if !sections[idx]
                .1
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case(&key))
            {
                sections[idx].1.push((key, val));
            }
        }
        Self(sections)
    }

    fn get(&self, section: &str, key: &str) -> Option<&[u8]> {
        let sec = self
            .0
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(section.as_bytes()))?;
        sec.1
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key.as_bytes()))
            .map(|(_, v)| v.as_slice())
    }
}

/// `[...]` 段名（`GetSecString`）：返回 `[` 与 `]` 之间的内容。
fn sec_name(line: &[u8]) -> Option<Vec<u8>> {
    if line.first() != Some(&b'[') {
        return None;
    }
    let end = line.iter().position(|b| *b == b']')?;
    Some(trim_bytes(&line[1..end]).to_vec())
}

fn trim_bytes(b: &[u8]) -> &[u8] {
    let mut s = b;
    while let Some((f, rest)) = s.split_first() {
        if f.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    while let Some((l, rest)) = s.split_last() {
        if l.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    s
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, body: &[u8]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join("dbsvr.conf");
        std::fs::write(&p, body).unwrap();
        p
    }

    /// 现网 `src/DBSrv/bin/Release/dbsvr.conf` 的真实内容必须解析出 5100/5600/MySQL。
    #[test]
    fn parses_deployed_dbsrv_conf() {
        let dir = std::env::temp_dir().join(format!("dbsrv-cfg-{}", std::process::id()));
        let p = write(
            &dir,
            b"[Setup]\nShowDebug=0\nServerAddr=0.0.0.0\nServerPort=6000\nGateAddr=0.0.0.0\nGatePort=5100\nServerName=LegendOfMir2\nDenyChrName=1\nDELMaxLevel=30\nDynamicIPMode=0\n\n[Server]\nIDSAddr=127.0.0.1\nIDSPort=5600\nMarketAddr=127.0.0.1\nMarketPort=5700\n\n[DataBase]\nStoreage=MySQL\nConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_db;\n\n[DBClear]\nInterval=3000\n",
        );
        let cfg = load(&p).unwrap();
        assert_eq!(cfg.gate_addr, "0.0.0.0");
        assert_eq!(cfg.gate_port, 5100);
        assert_eq!(cfg.server_name.text(), "LegendOfMir2");
        assert_eq!(cfg.server_addr, "0.0.0.0");
        assert_eq!(cfg.server_port, 6000);
        assert_eq!(cfg.market_addr, "127.0.0.1");
        assert_eq!(cfg.market_port, 5700);
        assert_eq!(cfg.delete_min_level, 30);
        assert_eq!(cfg.ids_addr, "127.0.0.1");
        assert_eq!(cfg.ids_port, 5600);
        assert_eq!(cfg.storeage.text(), "MySQL");
        assert_eq!(
            cfg.conn_string.text(),
            "server=127.0.0.1;uid=root;pwd=;database=mir2_db;"
        );
        assert!(!cfg.dynamic_ip_mode);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 值里的单个 `;` 不是注释（C# 只认 `;;`），`;;` 之后的才是注释。
    #[test]
    fn semicolon_rules_match_csharp() {
        let dir = std::env::temp_dir().join(format!("dbsrv-cfg-semi-{}", std::process::id()));
        let p = write(
            &dir,
            b"[DataBase]\nConnctionString=server=h;uid=u;database=d;  ;; trailing\n[Setup]\nServerAddr=0.0.0.0\nServerPort=6000\nGateAddr=0.0.0.0\nGatePort=5100\nServerName=LegendOfMir2\n",
        );
        let cfg = load(&p).unwrap();
        assert_eq!(cfg.conn_string.text(), "server=h;uid=u;database=d;");
        assert_eq!(cfg.server_name.bytes(), b"LegendOfMir2");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 缺关键项/非法项必须报错（M1④）。
    #[test]
    fn fatal_on_missing_or_bad() {
        let dir = std::env::temp_dir().join(format!("dbsrv-cfg-bad-{}", std::process::id()));
        for (why, body) in [
            ("缺 ServerAddr", &b"[Setup]\nServerPort=6000\nGateAddr=0.0.0.0\nGatePort=5100\n"[..]),
            ("ServerAddr=*", &b"[Setup]\nServerAddr=*\nServerPort=6000\nGateAddr=0.0.0.0\nGatePort=5100\n"[..]),
            (
                "ServerPort=0",
                &b"[Setup]\nServerAddr=0.0.0.0\nServerPort=0\nGateAddr=0.0.0.0\nGatePort=5100\n"[..],
            ),
            ("缺 GateAddr", &b"[Setup]\nServerAddr=0.0.0.0\nServerPort=6000\nGatePort=5100\n"[..]),
            ("GateAddr=*", &b"[Setup]\nServerAddr=0.0.0.0\nServerPort=6000\nGateAddr=*\nGatePort=5100\n"[..]),
            (
                "GatePort=0",
                &b"[Setup]\nServerAddr=0.0.0.0\nServerPort=6000\nGateAddr=0.0.0.0\nGatePort=0\n"[..],
            ),
            (
                "非 MySQL 存储",
                &b"[Setup]\nServerAddr=0.0.0.0\nServerPort=6000\nGateAddr=0.0.0.0\nGatePort=5100\n[DataBase]\nStoreage=Sqlite\n"[..],
            ),
        ] {
            let p = write(&dir, body);
            assert!(load(&p).is_err(), "应在「{why}」上报错：{body:?}");
        }
        assert!(load(&dir.join("nonexistent.conf")).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 非数字端口 C# 侧是"解析失败吃默认值"（`int.TryParse` 失败 ⇒ `ReadInteger` 返回 defValue），
    /// 不是启动失败 —— 同口径，别把它当"缺配置"。
    #[test]
    fn bad_port_falls_back_to_default() {
        let dir = std::env::temp_dir().join(format!("dbsrv-cfg-port-{}", std::process::id()));
        let p = write(
            &dir,
            b"[Setup]\nServerAddr=0.0.0.0\nServerPort=6000\nGateAddr=0.0.0.0\nGatePort=abc\n[DataBase]\nStoreage=MySQL\n",
        );
        assert_eq!(load(&p).unwrap().gate_port, 5100);
        std::fs::remove_dir_all(&dir).ok();
    }
}
