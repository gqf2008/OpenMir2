//! GameSvr 的遗留 INI 读取（现网样件 `E:\MirServer\M2GameSvr\Server.conf`）。
//!
//! 本切片只读**网关链路**真正需要的那几个键，键名照现网事实（**不发明**）：
//! `[Server] GateAddr / GatePort`（网关连上来的监听端点；现网 `GateAddr= 127.0.0.1` **值带前导空格**，
//! 所以两边都要 trim）、`CheckBlock`（发包阻塞阈值，`ChannelMessageHandler.cs:74` 用它决定何时回 `GM_RECEIVE_OK`）。
//! 世界尺寸（S1 占位）用 `WorldMaps`：形如 `1000x1000` 或 `1000x1000,200x200`。
//!
//! **`WorldMaps` 是可选键**（owner 裁决，见 `openmir2-m2-gamesvr-host` 卡 comment `272582e1`）：
//! 真 `E:\MirServer\M2GameSvr\Server.conf` 的 `[Server]` 段**没有这个键**（它是 S1 引入的占位），
//! 真地图表在 `[Share] EnvirDir`（现网 `Envir`）下的 `MapInfo.txt`。`WorldMaps` 只在**没有 Envir**
//! （开发/测试）时兜底；**两者都没有必须拒绝启动**（fail-closed），由 `mapwire::placeholder_sizes` 判。
//! 其余键**缺键即报错**（照 M1 验收④的"失败要响"口径），不许静默取默认值。

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
    ///
    /// **可选键**：配置里没有 `WorldMaps` ⇒ 空表。空表**不是**错误，但"空表 + 没有 Envir"
    /// 必须 fail-closed（见 `mapwire::placeholder_sizes`）——不要在这里提前拒绝，否则现网
    /// 无 `WorldMaps` 的真配置起不来。
    pub maps: Vec<(i32, i32)>,
    /// `[Share] EnvirDir`（oracle `SystemShare.Config.EnvirDir`，默认 `Envir`）——`MapInfo.txt` 所在目录。
    pub envir_dir: String,
    /// `[Share] MapDir`（oracle `SystemShare.Config.MapDir`，默认 `Map`）——`*.map` 所在目录。
    pub map_dir: String,
    /// `[Share] NoticeDir`（oracle `SystemShare.Config.NoticeDir`，默认 `Notice`）——
    /// `Notice.txt` 所在目录（`openmir2-m2-enter-world-frames` 的 658 `SM_SENDNOTICE` 用它）。
    pub notice_dir: String,
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
/// 文件不存在、缺 `[Server]` 段、缺 `GateAddr`/`GatePort`/`CheckBlock`/`IDSAddr`/`IDSPort`/`DBAddr`/`DBPort`、
/// 端口非法、`WorldMaps` **写了但非法** —— 都返回错误（调用方据此非零退出）。
/// `WorldMaps` **没写**不算错（可选键，见模块头）；`WorldMaps=` 空值 ⇒ 空表。
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
    let mut notice_dir: Option<String> = None;
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
            // oracle `ConfigFile` 的 section 字典是 `StringComparer.OrdinalIgnoreCase`
            // （`src/OpenMir2/Common/ConfigFile.cs:13`，R3-2）⇒ 段名按 ASCII 大小写不敏感归一。
            section = name.trim().to_ascii_uppercase();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        // 键字典同样 `OrdinalIgnoreCase`（`ConfigFile.cs:301`，R3-2）：键名也归一到 ASCII 大写。
        let (k, v) = (k.trim().to_ascii_uppercase(), v.trim());
        match section.as_str() {
            // 网关链路真正需要的键（本切片读这些）。
            "SERVER" => match k.as_str() {
                "GATEADDR" => gate_addr = Some(v.to_string()),
                "GATEPORT" => {
                    gate_port = Some(
                        v.parse()
                            .with_context(|| format!("GatePort 不是合法端口：{v:?}"))?,
                    );
                }
                "CHECKBLOCK" => {
                    check_block = Some(
                        v.parse()
                            .with_context(|| format!("CheckBlock 不是合法整数：{v:?}"))?,
                    );
                }
                "WORLDMAPS" => maps = Some(parse_maps(v)?),
                "IDSADDR" => ids_addr = Some(v.to_string()),
                "DBADDR" => db_addr = Some(v.to_string()),
                "DBPORT" => {
                    db_port = Some(
                        v.parse()
                            .with_context(|| format!("DBPort 不是合法端口：{v:?}"))?,
                    );
                }
                "IDSPORT" => {
                    ids_port = Some(
                        v.parse()
                            .with_context(|| format!("IDSPort 不是合法端口：{v:?}"))?,
                    );
                }
                _ => {}
            },
            // oracle 的取图目录（`ServerConf.cs:75-76` 从 `[Share]` 读，缺省 Envir/Map）。
            "SHARE" => match k.as_str() {
                "ENVIRDIR" => envir_dir = Some(v.to_string()),
                "MAPDIR" => map_dir = Some(v.to_string()),
                "NOTICEDIR" => notice_dir = Some(v.to_string()),
                _ => {}
            },
            _ => {}
        }
    }
    let (
        Some(gate_addr),
        Some(gate_port),
        Some(check_block),
        Some(ids_addr),
        Some(ids_port),
        Some(db_addr),
        Some(db_port),
    ) = (
        gate_addr,
        gate_port,
        check_block,
        ids_addr,
        ids_port,
        db_addr,
        db_port,
    )
    else {
        bail!(
            "[Server] 段缺键：需要 GateAddr/GatePort/CheckBlock/IDSAddr/IDSPort/DBAddr/DBPort（现网样件见 \
             E:\\MirServer\\M2GameSvr\\Server.conf；`WorldMaps` 可选）"
        );
    };
    // `WorldMaps` 可选：没写 ⇒ 空表（合法，是否 fail-closed 由 `mapwire::placeholder_sizes` 决定）。
    let maps = maps.unwrap_or_default();
    if gate_port == 0 {
        bail!("GatePort 不能为 0");
    }
    Ok(GateConfig {
        gate_addr,
        gate_port,
        check_block,
        maps,
        // oracle 缺省值（`GameSvrConf.cs:1295-1296`）：缺 `[Share]` 键不算错。
        envir_dir: envir_dir.unwrap_or_else(|| "Envir".to_string()),
        map_dir: map_dir.unwrap_or_else(|| "Map".to_string()),
        notice_dir: notice_dir.unwrap_or_else(|| "Notice".to_string()),
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
        assert_eq!(
            c.notice_dir, "Notice",
            "缺 [Share] NoticeDir 时用 oracle 缺省值（GameSvrConf.cs:1297）"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **红检 ①**（owner 裁决）：真 `Server.conf` **没有** `WorldMaps` ⇒ 必须能解析（`maps` 为空表）。
    ///
    /// 现网样件的 `[Server]` 段唯一缺的键就是 `WorldMaps`（`272582e1` 逐行核过）；把它当必需键
    /// 就等于"现网配置起不来"，所以它是可选键。"起得来"的另一半（有真 Envir ⇒ 用注册表）由
    /// `mapwire::envir_registry_covers_missing_worldmaps` 覆盖。
    #[test]
    fn deployed_conf_without_worldmaps_parses() {
        let dir = std::env::temp_dir().join(format!("gs-nowm-{}", std::process::id()));
        let p = write(
            &dir,
            "[Server]\nCheckBlock=12288\nGateAddr= 127.0.0.1\nGatePort=5000\n\
             IDSAddr= 127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n\
             [Share]\nEnvirDir=Envir\nMapDir=Map\nNoticeDir=Notice\n",
        );
        let c = load(&p).expect("缺 WorldMaps 不许报错（可选键）");
        assert!(c.maps.is_empty(), "没写 WorldMaps ⇒ 空表");
        assert_eq!(c.gate_port, 5000);
        assert_eq!(c.envir_dir, "Envir");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **红检 ③**：`WorldMaps` **写了但非法**（`10x0`）仍必须报错——可选≠不校验。
    #[test]
    fn bad_worldmaps_value_is_still_fatal() {
        let dir = std::env::temp_dir().join(format!("gs-badwm-{}", std::process::id()));
        let p = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nWorldMaps=10x0\n\
             IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n",
        );
        assert!(load(&p).is_err(), "WorldMaps=10x0 必须报错");
        let p2 = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nWorldMaps=nope\n\
             IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n",
        );
        assert!(load(&p2).is_err(), "WorldMaps 非 WxH 必须报错");
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

    /// `[Share] NoticeDir` 照 oracle 的键读（`ServerConf.cs:77`），值要 trim。
    #[test]
    fn parses_share_notice_dir() {
        let dir = std::env::temp_dir().join(format!("gs-notice-cfg-{}", std::process::id()));
        let p = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nWorldMaps=10x10\n\
             IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n\
             [Share]\nNoticeDir= Notice2\n",
        );
        let c = load(&p).unwrap();
        assert_eq!(c.notice_dir, "Notice2", "NoticeDir 值要 trim");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **R3-2**：oracle `ConfigFile` 的 section/key 字典都是 `OrdinalIgnoreCase`（`ConfigFile.cs:13/301`）
    /// ⇒ `[server]`/`gateaddr`/`[share]`/`envirdir` 都必须认，不能把 oracle 认的键当没写。
    #[test]
    fn section_and_key_matching_is_ascii_case_insensitive() {
        let dir = std::env::temp_dir().join(format!("gs-case-{}", std::process::id()));
        let p = write(
            &dir,
            "[server]\ngateaddr=127.0.0.1\ngateport=5000\ncheckblock=1\nworldmaps=10x10\n\
             idsaddr=127.0.0.1\nidsport=5600\ndbaddr=127.0.0.1\ndbport=6000\n\
             [share]\nenvirDir= Envir3\nMAPDIR=Map3\n",
        );
        let c = load(&p).unwrap();
        assert_eq!(c.gate_port, 5000, "[server]/gateport 小写必须认");
        assert_eq!(c.envir_dir, "Envir3", "[share]/envirDir 混合大小写必须认");
        assert_eq!(c.map_dir, "Map3", "[share]/MAPDIR 大写必须认");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 缺键/非法值必须报错（M1④"失败要响"）。
    #[test]
    fn missing_or_bad_keys_are_fatal() {
        let dir = std::env::temp_dir().join(format!("gs-bad-{}", std::process::id()));
        let p = write(&dir, "[Server]\nGateAddr=127.0.0.1\n");
        assert!(
            load(&p).is_err(),
            "缺 GatePort/CheckBlock 必须报错（`WorldMaps` 已降为可选，不再计入缺键）"
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
