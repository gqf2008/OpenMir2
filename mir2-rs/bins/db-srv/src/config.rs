//! DBSrv 的遗留 INI 配置读取（`dbsvr.conf`），键名照现网，**缺关键项即报错**（M1 验收④）。
//!
//! 取值口径逐条对齐 C# `DBSrv/Conf/{SettingsModel,AppConfigManager}.cs` + `Common/ConfigFile.cs`：
//! - 文件按 `ConfigFile.Load` 的 `new StreamReader(File.Open(...), Encoding.GetEncoding("GB2312"))`
//!   口径读（`Common/ConfigFile.cs:263`）：**带 BOM 探测**（UTF-8 / UTF-16LE / UTF-16BE /
//!   UTF-32LE `FF FE 00 00` / UTF-32BE），无 BOM 按 cp936；解码后是 .NET `string`，
//!   键值 `Trim()`，行首分号整行注释、`;;` 起行尾注释；同名键**首个生效**；段名大小写不敏感。
//!   本实现保存**解码后的文本**（UTF-8 视图）；上线字节（`(103/ServerName/…)`）由
//!   [`RawStr::wire_bytes`] 在出口按 cp936 重编码，等价 C# 的 `HUtil32.GetBytes(string)`。
//! - 缺键时 C# 吃 `SettingsModel` 默认值；本实现同默认值。
//! - **不写回**：C# 的 `ReadWriteXxx` 在缺键时把默认值写回配置文件（`Save()`）；写回是对冻结配置
//!   文件的副作用，本实现只读。差异登记 `tests/parity/whitelist.md` A-10。
//! - `GateAddr` 默认 `"*"`，C# `IPAddress.Parse("*")` 抛异常 ⇒ 启动失败；本实现显式报错，语义等价。
//! - `Storeage`（原拼写照抄）是存储插件名；C# 找不到 `DBSrv.Storage.<Storeage>.dll` 即抛异常。
//!   本实现只实现 MySQL 面，非 MySQL 一律明确报错退出。

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// 配置里的一个值：保存**解码后的文本**（UTF-8 字节）。
///
/// oracle 侧是 .NET `string`（`ReadWriteString` 的返回值）；上线字节由调用方在出口按
/// `HUtil32.GetBytes`（cp936）重编码 ⇒ [`RawStr::wire_bytes`]。配置文件本身可以是 GBK、
/// UTF-8+BOM 或 UTF-16（`ConfigFile.Load` 的 BOM 探测），解码后语义相同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawStr(Vec<u8>);

impl RawStr {
    /// 文本（UTF-8；配置解码后的原值）。
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }

    /// 上线字节：等价 C# `HUtil32.GetBytes(string)`（.NET cp936，逐码元查表，不可映射 `?`）。
    #[must_use]
    pub fn wire_bytes(&self) -> Vec<u8> {
        mir2_script::cp936::encode_gb2312_dotnet(&self.text())
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

/// 单元测试用的最小配置（会话链只需要 `ids_addr`/`ids_port`/`server_name`）。
#[cfg(test)]
pub(crate) fn for_test(ids_addr: String, ids_port: u16) -> DbConfig {
    DbConfig {
        server_name: RawStr(b"LegendOfMir2".to_vec()),
        show_debug: false,
        server_addr: "127.0.0.1".to_string(),
        server_port: 0,
        gate_addr: "127.0.0.1".to_string(),
        gate_port: 0,
        market_addr: "127.0.0.1".to_string(),
        market_port: 0,
        delete_min_level: 0,
        dynamic_ip_mode: false,
        map_file: RawStr(Vec::new()),
        english_names: false,
        ids_addr,
        ids_port,
        storeage: RawStr(b"MySQL".to_vec()),
        conn_string: RawStr(Vec::new()),
    }
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
    // `ConfigFile.Load` 的 `StreamReader(..., GB2312)` 口径（BOM 探测；无 BOM 按 cp936）。
    // 统一入口 `mir2_encoding::decode_config_text`（`mir2-script` 转发），与 game-svr/envir 同链。
    let text = mir2_script::cp936::decode_config_text(&raw);
    let ini = Ini::parse(&text);
    let s = |sec: &str, key: &str, def: &str| -> RawStr {
        ini.get(sec, key).map_or_else(
            || RawStr(def.as_bytes().to_vec()),
            |v| RawStr(v.as_bytes().to_vec()),
        )
    };
    let i = |sec: &str, key: &str, def: i32| -> i32 {
        ini.get(sec, key)
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(def)
    };
    let b = |sec: &str, key: &str, def: bool| -> bool {
        let Some(v) = ini.get(sec, key) else {
            return def;
        };
        match v.trim().to_uppercase().as_str() {
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
    if conn_string.text().is_empty() {
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

/// 段表：`段名 → [(键, 值)]`（文本级，镜像 C# `ConfigFile.Load` 的 .NET `string` 视图）。
type Sections = Vec<(String, Vec<(String, String)>)>;

/// 极简 INI 解析（文本级，镜像 C# `ConfigFile.Load`；`ReadLine` 的 `\r\n`/`\n` 由 `str::lines` 覆盖）。
struct Ini(Sections);

impl Ini {
    fn parse(text: &str) -> Self {
        let mut sections: Sections = Vec::new();
        let mut cur: Option<usize> = None;
        let mut in_block_comment = false;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if line.starts_with("/*") {
                in_block_comment = true;
                continue;
            }
            if in_block_comment {
                if line.starts_with("*/") {
                    in_block_comment = false;
                }
                continue;
            }
            if line.starts_with('[') {
                if let Some(inner) = sec_name(line) {
                    if inner.starts_with(";;") {
                        cur = None; // `[;;…]` 段注释：后续行全部忽略
                        continue;
                    }
                    sections.push((inner.to_string(), Vec::new()));
                    cur = Some(sections.len() - 1);
                    continue;
                }
            }
            let Some(eq) = line.find('=') else {
                continue;
            };
            if eq == 0 {
                continue;
            }
            let key = line[..eq].trim();
            let mut val = line[eq + 1..].trim();
            if let Some(pos) = val.find(";;") {
                val = val[..pos].trim();
            }
            if key.is_empty() {
                continue;
            }
            // 无段名的行 C# 落到 "" 段；现网配置都有段，这里直接忽略（已在上方 cur=None 语义内）
            let Some(idx) = cur else { continue };
            if !sections[idx]
                .1
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case(key))
            {
                sections[idx].1.push((key.to_string(), val.to_string()));
            }
        }
        Self(sections)
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        let sec = self
            .0
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(section))?;
        sec.1
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

/// `[...]` 段名（`GetSecString`）：返回 `[` 与 `]` 之间的内容。
fn sec_name(line: &str) -> Option<&str> {
    let inner = line.strip_prefix('[')?;
    let end = inner.find(']')?;
    Some(inner[..end].trim())
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
        assert_eq!(cfg.server_name.wire_bytes(), b"LegendOfMir2".to_vec());
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

    /// **E-5 BOM 矩阵（db-srv 链）**：`dbsvr.conf` 由 `AppConfigManager : ConfigFile` 读
    /// （`ConfigFile.cs:263` 的 `StreamReader(..., GB2312)`，GB2312 preamble 长度 0 ⇒ BOM 探测）。
    /// 4 份夹具（GBK 无 BOM / UTF-8 BOM / UTF-16LE BOM / GBK 含 `0xFF`）都必须：
    /// 首行 `[DataBase]` 被识别、`ConnctionString` 与 `[Setup] ServerName` 读到；
    /// `wire_bytes()` 与 C# `HUtil32.GetBytes(ServerName)` 同字节（cp936）。
    ///
    /// 牙：把 `load` 改回 `Ini::parse(&raw)`（字节级、不剥 BOM）⇒ UTF-8 BOM 夹具的
    /// `[DataBase]` 头丢（`ConnctionString` 落到默认值）⇒ 本用例必红。
    #[test]
    fn config_bom_matrix_keeps_first_section_and_values() {
        use mir2_script::cp936::{decode_config_text, encode_gb2312_dotnet};
        let dir = std::env::temp_dir().join(format!("dbsrv-bom-matrix-{}", std::process::id()));
        let base = "[DataBase]\nStoreage=MySQL\n\
             ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_bom;\n\
             [Setup]\nServerAddr=0.0.0.0\nServerPort=6000\nGateAddr=0.0.0.0\nGatePort=5100\n\
             ServerName=传奇\n[Server]\nIDSAddr=127.0.0.1\nIDSPort=5600\n";
        let text_ff = base.replace("ServerName=传奇", "ServerName=传奇\u{F8F5}");
        let gbk = encode_gb2312_dotnet(base);
        let gbk_ff = encode_gb2312_dotnet(&text_ff);
        assert!(gbk_ff.contains(&0xFF), "夹具必须真的含 0xFF 字节");
        let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
        utf8_bom.extend_from_slice(base.as_bytes());
        let mut utf16le = vec![0xFF, 0xFE];
        for u in base.encode_utf16() {
            utf16le.extend_from_slice(&u.to_le_bytes());
        }
        for (name, bytes, want_name) in [
            ("gbk-no-bom", gbk, "传奇"),
            ("utf8-bom", utf8_bom, "传奇"),
            ("utf16le-bom", utf16le, "传奇"),
            ("gbk-0xff", gbk_ff, "传奇\u{F8F5}"),
        ] {
            // 夹具自检：oracle 口径解码后首行就是 `[DataBase]`（BOM 已被吞）。
            assert!(
                decode_config_text(&bytes).starts_with("[DataBase]"),
                "{name}: 解码后首行必须是 [DataBase]"
            );
            std::fs::create_dir_all(&dir).unwrap();
            let p = dir.join("dbsvr.conf");
            std::fs::write(&p, &bytes).unwrap();
            let cfg = load(&p).unwrap_or_else(|e| panic!("{name}: 必须能读：{e}"));
            assert_eq!(cfg.storeage.text(), "MySQL", "{name}: [DataBase] Storeage");
            assert_eq!(
                cfg.conn_string.text(),
                "server=127.0.0.1;uid=root;pwd=;database=mir2_bom;",
                "{name}: [DataBase] ConnctionString（首行段名不能被 BOM 吃掉）"
            );
            assert_eq!(
                cfg.server_name.text(),
                want_name,
                "{name}: [Setup] ServerName"
            );
            assert_eq!(
                cfg.server_name.wire_bytes(),
                encode_gb2312_dotnet(want_name),
                "{name}: ServerName 上线字节 = HUtil32.GetBytes（cp936）"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
