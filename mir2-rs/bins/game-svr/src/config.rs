//! GameSvr 的遗留 INI 读取（现网样件 `E:\MirServer\M2GameSvr\Server.conf`）。
//!
//! 本切片只读**网关链路**真正需要的那几个键，键名照现网事实（**不发明**）：
//! `[Server] GateAddr / GatePort`（网关连上来的监听端点；现网 `GateAddr= 127.0.0.1` **值带前导空格**，
//! 所以两边都要 trim）、`CheckBlock`（发包阻塞阈值，`ChannelMessageHandler.cs:74` 用它决定何时回 `GM_RECEIVE_OK`）。
//! 世界尺寸（S1 占位）用 `WorldMaps`：形如 `1000x1000` 或 `1000x1000,200x200`。
//!
//! **缺键即报错**（照 M1 验收④的"失败要响"口径），不许静默取默认值。

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// 监听端点 + S1 需要的少量参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateConfig {
    /// `[Server] GateAddr`（现网带前导空格，解析时 trim）。
    pub gate_addr: String,
    /// `[Server] GatePort`（现网 5000）。
    pub gate_port: u16,
    /// `[Server] CheckBlock`（现网 12288）：`ChannelMessageHandler.cs:74` 的阻塞阈值。
    pub check_block: usize,
    /// S1 占位：世界地图尺寸列表（有 Envir 真地图注册表时被其尺寸取代；无 Envir 时才用）。
    pub maps: Vec<(i32, i32)>,
    /// `[Share] EnvirDir`（oracle `SystemShare.Config.EnvirDir`，默认 `Envir`）——`MapInfo.txt` 所在目录。
    pub envir_dir: String,
    /// `[Share] MapDir`（oracle `SystemShare.Config.MapDir`，默认 `Map`）——`*.map` 所在目录。
    pub map_dir: String,
    /// `[Server] IDSAddr`：登录服务器地址（oracle 是 TcpClient 主动连它，`AuthenticationService.cs:22-34`）。
    pub ids_addr: String,
    /// `[Server] IDSPort`（现网 5600）。
    pub ids_port: u16,
    /// `[Server] DBAddr`：DBSvr 地址（`DataQueryServer.cs:28` 用它做 `SetRemoteIPHost`）。
    pub db_addr: String,
    /// `[Server] DBPort`（现网 6000）。
    pub db_port: u16,
}

/// 默认配置文件：可执行文件同目录的 `Server.conf`（与 C# `AppContext.BaseDirectory` 一致）。
#[must_use]
pub fn default_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Server.conf")
}

/// 读取并校验配置。
///
/// # Errors
/// 文件不存在、缺 `[Server]` 段、缺 `GateAddr`/`GatePort`/`CheckBlock`、端口非法、`WorldMaps` 非法
/// —— 都返回错误（调用方据此非零退出）。
pub fn load(path: &Path) -> Result<GateConfig> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("读不到配置文件 {}", path.display()))?;
    let mut section = String::new();
    let mut gate_addr: Option<String> = None;
    let mut gate_port: Option<u16> = None;
    let mut check_block: Option<usize> = None;
    let mut maps: Option<Vec<(i32, i32)>> = None;
    let mut envir_dir: Option<String> = None;
    let mut map_dir: Option<String> = None;
    let mut ids_addr: Option<String> = None;
    let mut ids_port: Option<u16> = None;
    let mut db_addr: Option<String> = None;
    let mut db_port: Option<u16> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = name.trim().to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match section.as_str() {
            // 网关链路真正需要的键（本切片读这些）。
            "Server" => match k {
                "GateAddr" => gate_addr = Some(v.to_string()),
                "GatePort" => {
                    gate_port = Some(
                        v.parse()
                            .with_context(|| format!("GatePort 不是合法端口：{v:?}"))?,
                    );
                }
                "CheckBlock" => {
                    check_block = Some(
                        v.parse()
                            .with_context(|| format!("CheckBlock 不是合法整数：{v:?}"))?,
                    );
                }
                "WorldMaps" => maps = Some(parse_maps(v)?),
                "IDSAddr" => ids_addr = Some(v.to_string()),
                "DBAddr" => db_addr = Some(v.to_string()),
                "DBPort" => {
                    db_port = Some(
                        v.parse()
                            .with_context(|| format!("DBPort 不是合法端口：{v:?}"))?,
                    );
                }
                "IDSPort" => {
                    ids_port = Some(
                        v.parse()
                            .with_context(|| format!("IDSPort 不是合法端口：{v:?}"))?,
                    );
                }
                _ => {}
            },
            // oracle 的取图目录（`ServerConf.cs:75-76` 从 `[Share]` 读，缺省 Envir/Map）。
            "Share" => match k {
                "EnvirDir" => envir_dir = Some(v.to_string()),
                "MapDir" => map_dir = Some(v.to_string()),
                _ => {}
            },
            _ => {}
        }
    }
    let (
        Some(gate_addr),
        Some(gate_port),
        Some(check_block),
        Some(maps),
        Some(ids_addr),
        Some(ids_port),
        Some(db_addr),
        Some(db_port),
    ) = (
        gate_addr,
        gate_port,
        check_block,
        maps,
        ids_addr,
        ids_port,
        db_addr,
        db_port,
    )
    else {
        bail!(
            "[Server] 段缺键：需要 GateAddr/GatePort/CheckBlock/WorldMaps/IDSAddr/IDSPort/DBAddr/DBPort（现网样件见 \
             E:\\MirServer\\M2GameSvr\\Server.conf）"
        );
    };
    if gate_port == 0 || maps.is_empty() {
        bail!("GatePort 不能为 0、WorldMaps 不能为空");
    }
    Ok(GateConfig {
        gate_addr,
        gate_port,
        check_block,
        maps,
        // oracle 缺省值（`GameSvrConf.cs:1295-1296`）：缺 `[Share]` 键不算错。
        envir_dir: envir_dir.unwrap_or_else(|| "Envir".to_string()),
        map_dir: map_dir.unwrap_or_else(|| "Map".to_string()),
        ids_addr,
        ids_port,
        db_addr,
        db_port,
    })
}

/// `WorldMaps=1000x1000[,200x200...]`。
fn parse_maps(v: &str) -> Result<Vec<(i32, i32)>> {
    let mut out = Vec::new();
    for part in v.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((w, h)) = part.split_once(['x', 'X']) else {
            bail!("WorldMaps 里 {part:?} 不是 WxH 形式");
        };
        let w: i32 = w.trim().parse().with_context(|| format!("宽非法：{w:?}"))?;
        let h: i32 = h.trim().parse().with_context(|| format!("高非法：{h:?}"))?;
        if w <= 0 || h <= 0 {
            bail!("地图尺寸必须为正：{part:?}");
        }
        out.push((w, h));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, body: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join("Server.conf");
        std::fs::write(&p, body).unwrap();
        p
    }

    /// 现网样件形态（注意 `GateAddr` 值前的空格——照 `Server.conf` 事实）。
    #[test]
    fn parses_deployed_shape() {
        let dir = std::env::temp_dir().join(format!("gs-conf-{}", std::process::id()));
        let p = write(
            &dir,
            "[DataBase]\nDBPort=6000\n[Server]\nDBPort=6000\nCheckBlock=12288\nGateAddr= 127.0.0.1\n\
             GatePort=5000\nIDSAddr= 127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\nWorldMaps=1000x1000,200x200\n[Share]\nX=1\n",
        );
        let c = load(&p).unwrap();
        assert_eq!(c.gate_addr, "127.0.0.1", "值要 trim（现网带前导空格）");
        assert_eq!(c.gate_port, 5000);
        assert_eq!(c.check_block, 12288);
        assert_eq!(c.maps, vec![(1000, 1000), (200, 200)]);
        assert_eq!(c.ids_addr, "127.0.0.1", "IDSAddr 值也带前导空格，要 trim");
        assert_eq!(c.ids_port, 5600);
        assert_eq!(c.db_addr, "127.0.0.1");
        assert_eq!(c.db_port, 6000);
        assert_eq!(
            c.envir_dir, "Envir",
            "缺 [Share] EnvirDir 时用 oracle 缺省值"
        );
        assert_eq!(c.map_dir, "Map", "缺 [Share] MapDir 时用 oracle 缺省值");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `[Share] EnvirDir/MapDir` 照 oracle 的键读（`ServerConf.cs:75-76`），值要 trim。
    #[test]
    fn parses_share_map_dirs() {
        let dir = std::env::temp_dir().join(format!("gs-share-{}", std::process::id()));
        let p = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nWorldMaps=10x10\n\
             IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n\
             [Share]\nEnvirDir= Envir2\nMapDir=Map2\n",
        );
        let c = load(&p).unwrap();
        assert_eq!(c.envir_dir, "Envir2");
        assert_eq!(c.map_dir, "Map2");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 缺键/非法值必须报错（M1④"失败要响"）。
    #[test]
    fn missing_or_bad_keys_are_fatal() {
        let dir = std::env::temp_dir().join(format!("gs-bad-{}", std::process::id()));
        let p = write(&dir, "[Server]\nGateAddr=127.0.0.1\n");
        assert!(
            load(&p).is_err(),
            "缺 GatePort/CheckBlock/WorldMaps 必须报错"
        );
        let p2 = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=abc\nCheckBlock=1\nWorldMaps=10x10\n",
        );
        assert!(load(&p2).is_err(), "GatePort 非数字必须报错");
        let p3 = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nWorldMaps=10x0\n",
        );
        assert!(load(&p3).is_err(), "地图尺寸非正必须报错");
        std::fs::remove_dir_all(&dir).ok();
    }
}
