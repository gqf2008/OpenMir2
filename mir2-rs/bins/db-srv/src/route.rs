//! `DBShare` 的等价物：启动时读 `AddrTable.txt` / `SelectID.txt` / `ServerInfo.txt`
//! （都在**可执行文件同目录**，与 C# `AppDomain.CurrentDomain.BaseDirectory` 一致），
//! 外加拿不到就为空的 `DenyChrName.txt` 与 `MapFile`（MapInfo.txt）。
//!
//! 逐条对齐 `src/DBSrv/AppService.cs:LoadServerInfo/LoadChrNameList` 与 `src/DBSrv/DBShare.cs`。
//! 这些文件在现网部署里都可能**不存在**（缺文件＝空表，不报错），沿用同一口径。

use std::path::Path;

use anyhow::{bail, Result};

use crate::gbk::{split2_nonempty, str_to_int, trim_name};

/// 一条「角色网关 → 游戏网关」路由（C# `GateRouteInfo`，数组容量 20）。
#[derive(Debug, Clone, Default)]
pub struct GateRoute {
    /// `ServerInfo.txt` 每行的第一个字段：角色网关 IP（原始字节，与线上比较口径一致）。
    pub sel_gate_ip: Vec<u8>,
    /// 游戏网关 IP（C# 数组容量 20；原始字节，回包时直接拼进 `EncodeString`）。
    pub game_gate_ip: Vec<Vec<u8>>,
    /// 游戏网关端口（与 `game_gate_ip` 同下标）。
    pub game_gate_port: Vec<i32>,
}

impl GateRoute {
    /// 路由条数（C# `GateCount`）。
    #[must_use]
    pub fn gate_count(&self) -> usize {
        self.game_gate_ip.len()
    }
}

/// 全局只读配置表（C# `DBShare` 的静态字段）。
#[derive(Debug, Default)]
pub struct Share {
    /// `AddrTable.txt`：允许接入的服务器 IP 列表。
    pub server_ips: Vec<String>,
    /// `SelectID.txt`：`IP → 网关 ID`（建角时写入 `characters_indexes.SelectID`）。
    pub gate_ids: Vec<(String, i16)>,
    /// `ServerInfo.txt`：路由表（最多 20 条）。
    pub routes: Vec<GateRoute>,
    /// `MapFile`（MapInfo.txt）：地图名 → 服务器编号（`OrdinalIgnoreCase`）。
    pub map_list: Vec<(String, i32)>,
    /// `DenyChrName.txt`：禁用角色名（按行，解码为 String）。
    pub deny_names: Vec<String>,
}

impl Share {
    /// 读全部配置文件（缺文件＝空表；`ServerInfo.txt` 超过 20 条与 C# 一样算致命）。
    ///
    /// # Errors
    /// `ServerInfo.txt` 路由行数超过 20（C# `RouteInfo[20]` 越界抛异常 ⇒ 启动失败）。
    pub fn load(base: &Path, map_file: &Path) -> Result<Self> {
        let server_ips = read_lines(&base.join("AddrTable.txt"))
            .into_iter()
            .map(|l| String::from_utf8_lossy(&l).trim().to_string())
            .collect();
        let gate_ids = load_gate_ids(&base.join("SelectID.txt"));
        let routes = load_routes(&base.join("ServerInfo.txt"))?;
        let map_list = load_map_list(map_file);
        let deny_names = read_lines(&base.join("DenyChrName.txt"))
            .into_iter()
            .map(|l| crate::gbk::decode(&trim_name(&l)))
            .filter(|l| !l.is_empty())
            .collect();
        Ok(Self {
            server_ips,
            gate_ids,
            routes,
            map_list,
            deny_names,
        })
    }

    /// C# `CheckServerIP`：不在表里即拒绝（表为空则全部拒绝）。
    #[must_use]
    pub fn check_server_ip(&self, ip: &str) -> bool {
        self.server_ips.iter().any(|s| s == ip)
    }

    /// C# `GetGateID`：没配就是 0。
    #[must_use]
    pub fn get_gate_id(&self, ip: &str) -> i16 {
        self.gate_ids
            .iter()
            .find(|(k, _)| k == ip)
            .map_or(0, |(_, v)| *v)
    }

    /// C# `GetMapIndex`：没配就是 0（`OrdinalIgnoreCase`）。
    #[must_use]
    pub fn get_map_index(&self, map: &str) -> i32 {
        self.map_list
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(map))
            .map_or(0, |(_, v)| *v)
    }

    /// C# `CheckDenyChrName`：表为空即放行；命中名单即拒绝（`OrdinalIgnoreCase`）。
    ///
    /// E-7：名字域用解码后的 String + [`crate::gbk::fold_text`]（D-2/B-103），
    /// 不再拿线上 GBK 字节做 ASCII 折叠。
    #[must_use]
    pub fn check_deny_chr_name(&self, name: &str) -> bool {
        let folded = crate::gbk::fold_text(name);
        !self
            .deny_names
            .iter()
            .any(|d| crate::gbk::fold_text(d) == folded)
    }

    /// C# `GateRouteIp` + `GetGameGateRoute`：按角色网关 IP 找路由，
    /// 用 `pick`（取值范围 `0..gate_count`，调用方给随机数）选一条游戏网关。
    ///
    /// 返回 `(游戏网关 IP 的原始字节, 端口)`；没匹配到路由时两者是 `(空, 0)`（C# 同样给空串）。
    #[must_use]
    pub fn gate_route(&self, sel_gate_ip: &[u8], pick: usize) -> (Vec<u8>, i32) {
        for r in &self.routes {
            if r.sel_gate_ip == sel_gate_ip {
                if r.gate_count() == 0 {
                    return (Vec::new(), 0);
                }
                let idx = if r.gate_count() == 1 {
                    0
                } else {
                    pick % r.gate_count()
                };
                return (r.game_gate_ip[idx].clone(), r.game_gate_port[idx]);
            }
        }
        (Vec::new(), 0)
    }
}

/// 按行读文本文件（缺文件 → 空；行尾 `\r` 去掉）。
fn read_lines(path: &Path) -> Vec<Vec<u8>> {
    let Ok(raw) = std::fs::read(path) else {
        return Vec::new();
    };
    raw.split(|b| *b == b'\n')
        .map(|l| {
            let l = l.strip_suffix(b"\r").unwrap_or(l);
            l.to_vec()
        })
        .collect()
}

/// `SelectID.txt`：`ID IP` 两列（`GetValidStr3` 按空格/制表切，ID < 0 的行跳过）。
fn load_gate_ids(path: &Path) -> Vec<(String, i16)> {
    let mut out = Vec::new();
    for line in read_lines(path) {
        let line = trim_name(&line);
        if line.is_empty() || line[0] == b';' {
            continue;
        }
        let (id_b, rest) = split2_nonempty(&line, b" \t");
        if rest.is_empty() {
            continue;
        }
        let (addr_b, _) = split2_nonempty(&rest, b" \t");
        let n = str_to_int(&id_b, -1);
        if n < 0 {
            continue;
        }
        // C# `(short)nID`：按 16 位截断（既有行为）
        #[allow(clippy::cast_possible_truncation)]
        let short = n as i16;
        out.push((String::from_utf8_lossy(&addr_b).trim().to_string(), short));
    }
    out
}

/// `ServerInfo.txt`：每行 `角色网关IP 游戏网关IP 端口[,端口…]`。
fn load_routes(path: &Path) -> Result<Vec<GateRoute>> {
    let mut out: Vec<GateRoute> = Vec::new();
    for line in read_lines(path) {
        let line = trim_name(&line);
        if line.is_empty() || line[0] == b';' {
            continue;
        }
        let (sel_ip, mut rest) = split2_nonempty(&line, b" \t");
        if sel_ip.is_empty() || rest.is_empty() {
            continue;
        }
        if out.len() >= 20 {
            bail!("ServerInfo.txt 路由条目超过 20 条（C# RouteInfo[20] 越界 ⇒ 启动失败）");
        }
        let mut route = GateRoute {
            sel_gate_ip: trim_name(&sel_ip),
            ..GateRoute::default()
        };
        while !rest.is_empty() {
            let (ip_token, tail) = split2_nonempty(&rest, b" \t");
            // C# 是 `sGameGate.Split(",")`（对**剩余**串切逗号，与命名不符，如实复刻）
            for piece in tail.split(|b| *b == b',') {
                route.game_gate_ip.push(trim_name(&ip_token));
                route.game_gate_port.push(str_to_int(piece, 0));
            }
            rest = Vec::new();
        }
        out.push(route);
    }
    Ok(out)
}

/// `MapInfo.txt`：只认 `[地图名 服务器编号 …]` 形状的行。
fn load_map_list(path: &Path) -> Vec<(String, i32)> {
    let mut out = Vec::new();
    let text = std::fs::read(path).unwrap_or_default();
    for raw in text.split(|b| *b == b'\n') {
        let line = trim_name(raw);
        if line.first() != Some(&b'[') {
            continue;
        }
        let (inner, _) = crate::gbk::arrest(&line, b'[', b']');
        if inner.is_empty() {
            continue;
        }
        // C#：`sMapInfo = GetValidStr3(sMapName, ref sMapName, ws)`（dest=名字，返回=余量）；
        // 再 `sServerIndex = GetValidStr3(sMapInfo, ref sMapInfo, ws)` ⇒ **编号取第二次切分的返回值**
        // （即"名字 填充 编号"形状的第 3 段），如实照搬。
        let (name, rest) = split2_nonempty(&inner, b" \t");
        if name.is_empty() {
            continue;
        }
        let (_filler, server_index_b) = split2_nonempty(&rest, b" \t");
        // MapInfo 经 C# `StringList.LoadFromFile`（cp936）读入；名字是 String 键，
        // 必须解码后与 DB/线上名字比较（`from_utf8_lossy` 会把 GBK 中文变成替换字符）。
        out.push((
            crate::gbk::decode(&name).trim().to_string(),
            str_to_int(&server_index_b, 0),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dbsrv-route-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 现网 DBSrv 根目录的三份文件（AddrTable=127.0.0.1；ServerInfo=127.0.0.1 127.0.0.1 7200）。
    #[test]
    fn loads_deployed_tables() {
        let d = base_dir("ok");
        std::fs::write(d.join("AddrTable.txt"), "127.0.0.1\n").unwrap();
        std::fs::write(d.join("ServerInfo.txt"), "127.0.0.1 127.0.0.1 7200\n").unwrap();
        std::fs::write(d.join("SelectID.txt"), "").unwrap();
        let s = Share::load(&d, &d.join("MapInfo-nonexistent.txt")).unwrap();
        assert!(s.check_server_ip("127.0.0.1"));
        assert!(!s.check_server_ip("10.0.0.1"));
        assert_eq!(s.get_gate_id("127.0.0.1"), 0);
        assert_eq!(s.routes.len(), 1);
        assert_eq!(s.routes[0].sel_gate_ip, b"127.0.0.1".to_vec());
        assert_eq!(s.routes[0].gate_count(), 1);
        assert_eq!(s.gate_route(b"127.0.0.1", 0), (b"127.0.0.1".to_vec(), 7200));
        assert_eq!(s.gate_route(b"9.9.9.9", 0), (Vec::new(), 0));
        assert_eq!(s.get_map_index("0"), 0);
        assert!(s.check_deny_chr_name("anything"));
        std::fs::remove_dir_all(&d).ok();
    }

    /// 逗号端口列表：C# 是对**剩余串**切逗号（命名与语义不符），这里同口径。
    #[test]
    fn route_comma_ports_follow_csharp() {
        let d = base_dir("comma");
        std::fs::write(
            d.join("ServerInfo.txt"),
            "1.1.1.1 2.2.2.2,3,4\nA 9.9.9.9 7200,7300\n",
        )
        .unwrap();
        let s = Share::load(&d, &d.join("none.txt")).unwrap();
        assert_eq!(s.routes[0].sel_gate_ip, b"1.1.1.1".to_vec());
        // 第一次 GetValidStr3 把 "2.2.2.2,3,4" 整体当 dest（该段里没有空白），
        // 剩余串为空 ⇒ `Split(",")` = [""] ⇒ 端口 StrToInt("") = 0
        assert_eq!(s.routes[0].game_gate_ip, vec![b"2.2.2.2,3,4".to_vec()]);
        assert_eq!(s.routes[0].game_gate_port, vec![0]);
        assert_eq!(s.routes[1].sel_gate_ip, b"A".to_vec());
        // "A 9.9.9.9 7200,7300"：dest=9.9.9.9，剩余="7200,7300" 按逗号切出两个端口 ⇒ 2 条同 IP 路由
        assert_eq!(
            s.routes[1].game_gate_ip,
            vec![b"9.9.9.9".to_vec(), b"9.9.9.9".to_vec()]
        );
        assert_eq!(s.routes[1].game_gate_port, vec![7200, 7300]);
        std::fs::remove_dir_all(&d).ok();
    }

    /// 缺文件 = 空表（不报错）；MapInfo 只认 `[名字 编号]` 行。
    #[test]
    fn missing_files_and_map_list() {
        let d = base_dir("map");
        // MapInfo.txt 是 GBK（oracle `StringList.LoadFromFile` 按 cp936 解码），
        // 夹具也必须是 GBK 字节（`比奇省` = B1C8 C6E6 CAA1）。
        std::fs::write(
            d.join("MapInfo.txt"),
            b"[0 0]\n[\xB1\xC8\xC6\xE6\xCA\xA1 x 7]\n[ 2 5]\nnot a map\n",
        )
        .unwrap();
        let s = Share::load(&d, &d.join("MapInfo.txt")).unwrap();
        assert_eq!(s.get_map_index("0"), 0);
        // C# 取的是"第二次切分的返回值"：`[名字 填充 编号]` 形状才能拿到编号
        assert_eq!(s.get_map_index("比奇省"), 7);
        assert_eq!(s.get_map_index("2"), 0);
        assert_eq!(s.get_map_index("nope"), 0);
        assert!(s.server_ips.is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    /// SelectID.txt 的 `ID IP` 两列。
    #[test]
    fn select_id_table() {
        let d = base_dir("selid");
        std::fs::write(
            d.join("SelectID.txt"),
            "; comment\n7 127.0.0.1\n-3 1.2.3.4\n8 10.0.0.9\n",
        )
        .unwrap();
        let s = Share::load(&d, &d.join("none.txt")).unwrap();
        assert_eq!(s.get_gate_id("127.0.0.1"), 7);
        assert_eq!(s.get_gate_id("10.0.0.9"), 8);
        assert_eq!(s.get_gate_id("1.2.3.4"), 0);
        std::fs::remove_dir_all(&d).ok();
    }
}
