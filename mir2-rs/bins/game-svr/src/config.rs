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
    /// `[Setup] RedMsgFColor`（oracle `GameSvrConf.cs:1540` 缺省 `0xFF`）——`SysMsg(..., Red, ...)`
    /// 的前景色，落在 `SM_SYSMESSAGE` 的 `Param` 低字节。
    ///
    /// 用途：`openmir2-m2-session-lifecycle` 的账号级顶号提示（`TCPNetChannel.KickUser`，
    /// `TCPNetChannel.cs:229/262`）。缺键用 oracle 缺省值（`ServerConf.cs:333` 的 `ReadWriteByte` 口径）。
    pub red_msg_f_color: u8,
    /// `[Setup] RedMsgBColor`（oracle `GameSvrConf.cs:1541` 缺省 `0x38`）——同上，高字节。
    pub red_msg_b_color: u8,
    /// `[Setup] HearMsgFColor`（oracle `GameSvrConf.cs:1528` 缺省 `0x00`）——`CM_SAY` 普通说话
    /// 下行 `SM_HEAR` 的前景色（`BaseObject.Base.cs:478` 的 `btHearMsgFColor`）。现网 `=0`。
    pub hear_msg_f_color: u8,
    /// `[Setup] HearMsgBColor`（oracle `GameSvrConf.cs:1529` 缺省 `0xFF`）——同上，背景色。现网 `=255`。
    pub hear_msg_b_color: u8,
    /// `[Server] IDSAddr`：登录服务器地址（oracle 是 TcpClient 主动连它，`AuthenticationService.cs:22-34`）。
    pub ids_addr: String,
    /// `[Server] IDSPort`（现网 5600）。
    pub ids_port: u16,
    /// `[Server] DBAddr`：DBSvr 地址（`DataQueryServer.cs:28` 用它做 `SetRemoteIPHost`）。
    pub db_addr: String,
    /// `[Server] DBPort`（现网 6000）。
    pub db_port: u16,
    /// `[DataBase] ConnctionString`（**原拼写照抄**，`ServerConf.cs:15`）——GameSvr 直连 MySQL 读
    /// `stditems` 用的连接串（oracle `MySqlDB.LoadItemsDB` 也是直连，不经过 DBSvr）。
    ///
    /// `None` = 配置里没这个键 ⇒ 兜底用 oracle 的源码缺省
    /// （`GameSvrConf.cs:1921`：`server=127.0.0.1;uid=root;pwd=;database=mir2;`）；两者都解析失败
    /// ⇒ 物品表为空（`GetStdItem` 恒 null ⇒ 帧 50/41 的装备形态计 0，与 oracle 空表分支等价）+ WRN。
    pub db_conn_string: Option<String>,
    /// `[Setup] ShowRankLevelName`（oracle `GameSvrConf.cs:376/1464` 缺省 `false`）。
    ///
    /// 帧 42 `SM_USERNAME` 的展示名走 `GetShowName()`（`PlayObject.cs:3329`）：
    /// `true` ⇒ `Format(RankLevelName, ChrName)`，`false` ⇒ 原名/转生名模板。现网 `=1`。
    pub show_rank_level_name: bool,
    /// `[Setup] HumanMaxGold`（oracle `GameSvrConf.cs:1451` 缺省 `10000000`）——
    /// `ground::PickUpContext.human_max_gold`（`PlayObject.Base.cs:171` 的 `IncGold` 上限）。
    pub human_max_gold: i32,
    /// `[Setup] FloorItemCanPickUpTime`（oracle `GameSvrConf.cs:1488` 缺省 `2*60*1000`）——
    /// `ground::PickUpContext.floor_item_can_pick_up_time`（`PlayObject.Base.cs:67`）。
    pub floor_item_can_pick_up_time: i32,
    /// `[Setup] ControlDropItem`（oracle `GameSvrConf.cs:1703` 缺省 `false`）——
    /// `ground::DropContext.control_drop_item`（`PlayObject.Operate.cs:284`）。
    pub control_drop_item: bool,
    /// `[Setup] InSafeDisableDrop`（oracle `GameSvrConf.cs:1704` 缺省 `false`）——
    /// `ground::DropContext.in_safe_disable_drop`（`PlayObject.Operate.cs:224/254`）。
    pub in_safe_disable_drop: bool,
    /// `[Setup] CanDropPrice`（oracle `GameSvrConf.cs:1706` 缺省 `500`）——
    /// `ground::DropContext.can_drop_price`（`PlayObject.Operate.cs:284` 的 `stdItem.Price <` 门）。
    pub can_drop_price: i32,
    /// `[Setup] DropItemRage`（oracle `GameSvrConf.cs:1754` 缺省 `3`，`byte`）——
    /// `ground::DropContext.drop_item_rage`（`PlayObject.Operate.cs:291` 的 `_MIN(...,3)` 散布半径）。
    pub drop_item_rage: u8,
    /// `[Setup] SendRefMsgRange`（oracle `GameSvrConf.cs:499`，缺省 `12`，`byte`；
    /// `ServerConf.cs:280` 的 `ReadWriteInteger("Setup", "SendRefMsgRange", …)`）。
    ///
    /// 用途：`BaseObject.SendRefMsg`（`BaseObject.Message.cs:399-402`）扫描周围
    /// `CurrX/Y ± SendRefMsgRange` 的单元格，向其中的玩家广播 `RM_*`。
    /// `openmir2-m2-gamesvr-host` 的装备/卸下 `FeatureChanged()`（`SM_FEATURECHANGED`）用它。
    /// 缺键用 oracle 缺省 `12`（`GameSvrConf.cs:1503`）。
    pub send_ref_msg_range: u8,
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
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("读不到配置文件 {}", path.display()))?;
    // **BOM 口径**（B-1，reviewer-2 `b49897e8`）：现网 `E:\MirServer\M2GameSvr\Server.conf`
    // 首字节是 UTF-8 BOM（`ef bb bf`）。oracle 的 `ConfigFile.Load` 用
    // `new StreamReader(path, GB2312)` 且**默认做 BOM 探测**（`ConfigFile.cs:263`）⇒ 首行的
    // BOM 被吞掉，`[DataBase]` 段被正常识别。Rust 的 `read_to_string` 会把它留成 `U+FEFF`，
    // 而 `str::trim` 不把 `U+FEFF` 当空白 ⇒ `[DataBase]` 头丢失 ⇒ `ConnctionString` 缺键
    // ⇒ 回落到源码缺省的 `database=mir2`（现网不存在该库）⇒ `stditems` 恒空。
    // 照 C# 语义：**只在文件首**剥一个 `U+FEFF`（不是每行、不是循环）。
    let text = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
    let mut section = String::new();
    let mut gate_addr: Option<String> = None;
    let mut gate_port: Option<u16> = None;
    let mut check_block: Option<usize> = None;
    let mut maps: Option<Vec<(i32, i32)>> = None;
    let mut envir_dir: Option<String> = None;
    let mut map_dir: Option<String> = None;
    let mut notice_dir: Option<String> = None;
    let mut red_msg_f_color: Option<u8> = None;
    let mut red_msg_b_color: Option<u8> = None;
    let mut hear_msg_f_color: Option<u8> = None;
    let mut hear_msg_b_color: Option<u8> = None;
    let mut show_rank_level_name: Option<bool> = None;
    let mut human_max_gold: Option<i32> = None;
    let mut floor_item_can_pick_up_time: Option<i32> = None;
    let mut control_drop_item: Option<bool> = None;
    let mut in_safe_disable_drop: Option<bool> = None;
    let mut can_drop_price: Option<i32> = None;
    let mut drop_item_rage: Option<u8> = None;
    let mut send_ref_msg_range: Option<u8> = None;
    let mut ids_addr: Option<String> = None;
    let mut ids_port: Option<u16> = None;
    let mut db_addr: Option<String> = None;
    let mut db_port: Option<u16> = None;
    let mut db_conn_string: Option<String> = None;
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
            // `MySqlDB` 直连用（`ServerConf.cs:15` 从 `[DataBase]` 读 `ConnctionString`；原拼写照抄）。
            // 现网样件 `E:\MirServer\M2GameSvr\Server.conf` 在 `[DataBase]` 段（键 `ConnctionString`）。
            "DATABASE" => {
                if k.as_str() == "CONNCTIONSTRING" {
                    db_conn_string = Some(v.to_string());
                }
            }
            // 消息颜色（`SystemShare.Config.RedMsgFColor/BColor`，`ServerConf.cs:333-334` 的 `ReadWriteByte`）。
            // 缺键不是错误（oracle 读不到就沿用 `GameSvrConf` 的缺省 0xFF/0x38）；写了但非法才报错。
            "SETUP" => match k.as_str() {
                "REDMSGFCOLOR" => {
                    red_msg_f_color = Some(
                        v.parse()
                            .with_context(|| format!("RedMsgFColor 不是合法字节：{v:?}"))?,
                    );
                }
                "REDMSGBCOLOR" => {
                    red_msg_b_color = Some(
                        v.parse()
                            .with_context(|| format!("RedMsgBColor 不是合法字节：{v:?}"))?,
                    );
                }
                "HEARMSGFCOLOR" => {
                    hear_msg_f_color = Some(
                        v.parse()
                            .with_context(|| format!("HearMsgFColor 不是合法字节：{v:?}"))?,
                    );
                }
                "HEARMSGBCOLOR" => {
                    hear_msg_b_color = Some(
                        v.parse()
                            .with_context(|| format!("HearMsgBColor 不是合法字节：{v:?}"))?,
                    );
                }
                // `ServerConf.cs:271` 的 `ReadWriteBool("Setup", ...)`：现网 `0/1`。
                "SHOWRANKLEVELNAME" => {
                    show_rank_level_name = Some(parse_bool(v, "ShowRankLevelName")?);
                }
                // `ServerConf.cs:264` 的 `ReadWriteInteger("Setup", "HumanMaxGold", ...)`。
                "HUMANMAXGOLD" => {
                    human_max_gold = Some(
                        v.parse()
                            .with_context(|| format!("HumanMaxGold 不是合法整数：{v:?}"))?,
                    );
                }
                // `ServerConf.cs:439` 的 `ReadWriteInteger("Setup", "FloorItemCanPickUpTime", ...)`。
                "FLOORITEMCANPICKUPTIME" => {
                    floor_item_can_pick_up_time =
                        Some(v.parse().with_context(|| {
                            format!("FloorItemCanPickUpTime 不是合法整数：{v:?}")
                        })?);
                }
                // `ServerConf.cs:481-482/484` 的三条 `ReadWriteBool/Integer("Setup", ...)`。
                "CONTROLDROPITEM" => {
                    control_drop_item = Some(parse_bool(v, "ControlDropItem")?);
                }
                "INSAFEDISABLEDROP" => {
                    in_safe_disable_drop = Some(parse_bool(v, "InSafeDisableDrop")?);
                }
                "CANDROPPRICE" => {
                    can_drop_price = Some(
                        v.parse()
                            .with_context(|| format!("CanDropPrice 不是合法整数：{v:?}"))?,
                    );
                }
                // `GameSvrConf.DropItemRage` 是 `byte`（`GameSvrConf.cs:878`）；`Server.conf` 现网未写。
                "DROPITEMRAGE" => {
                    drop_item_rage = Some(
                        v.parse()
                            .with_context(|| format!("DropItemRage 不是合法字节：{v:?}"))?,
                    );
                }
                // `GameSvrConf.SendRefMsgRange` 是 `byte`（`GameSvrConf.cs:499`）；`Server.conf` 现网未写。
                "SENDREFMSGRANGE" => {
                    send_ref_msg_range = Some(
                        v.parse()
                            .with_context(|| format!("SendRefMsgRange 不是合法字节：{v:?}"))?,
                    );
                }
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
        // oracle 缺省值（`GameSvrConf.cs:1540-1541`）：`[Setup]` 里没写也照常起。
        red_msg_f_color: red_msg_f_color.unwrap_or(0xFF),
        red_msg_b_color: red_msg_b_color.unwrap_or(0x38),
        // oracle 缺省值（`GameSvrConf.cs:1528-1529`）：`HearMsgFColor=0x00` / `HearMsgBColor=0xFF`。
        hear_msg_f_color: hear_msg_f_color.unwrap_or(0x00),
        hear_msg_b_color: hear_msg_b_color.unwrap_or(0xFF),
        // oracle 缺省值（`GameSvrConf.cs:1464`）：`[Setup]` 里没写不算错。
        show_rank_level_name: show_rank_level_name.unwrap_or(false),
        // oracle 缺省值（`GameSvrConf.cs:1451/1488/1703/1704/1706/1754`）：`[Setup]` 里没写不算错。
        human_max_gold: human_max_gold.unwrap_or(10_000_000),
        floor_item_can_pick_up_time: floor_item_can_pick_up_time.unwrap_or(2 * 60 * 1000),
        control_drop_item: control_drop_item.unwrap_or(false),
        in_safe_disable_drop: in_safe_disable_drop.unwrap_or(false),
        can_drop_price: can_drop_price.unwrap_or(500),
        drop_item_rage: drop_item_rage.unwrap_or(3),
        // oracle 缺省值（`GameSvrConf.cs:1503`）：`[Setup]` 里没写不算错。
        send_ref_msg_range: send_ref_msg_range.unwrap_or(12),
        ids_addr,
        ids_port,
        db_addr,
        db_port,
        db_conn_string,
    })
}

/// `ReadWriteBool` 口径（`ConfigFile.cs` 的布尔读法）：`0/1` + `true/false`（大小写不敏感），其余报错。
fn parse_bool(v: &str, key: &str) -> Result<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        other => bail!("{key} 不是布尔：{other:?}"),
    }
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
             GatePort=5000\nIDSAddr= 127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\nWorldMaps=1000x1000,200x200\n[Share]\nX=1\n[Setup]\nShowRankLevelName=1\nShowGuildName=0\n",
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
        assert!(
            c.show_rank_level_name,
            "[Setup] ShowRankLevelName=1（现网值）"
        );
        assert_eq!(
            c.hear_msg_f_color, 0x00,
            "缺 [Setup] HearMsgFColor ⇒ oracle 缺省 0x00（GameSvrConf.cs:1528）"
        );
        assert_eq!(
            c.hear_msg_b_color, 0xFF,
            "缺 [Setup] HearMsgBColor ⇒ oracle 缺省 0xFF（GameSvrConf.cs:1529）"
        );
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

    /// `openmir2-m2-gamesvr-host`（sim 循环对接片）：`CM_PICKUP`/`CM_DROPITEM` 的 `ground::*` 输入
    /// 键（`GameSvrConf.HumanMaxGold` 等 6 个）。两条口径：
    /// ① 缺键 ⇒ 用 oracle 源码缺省（`GameSvrConf.cs:1451/1488/1703/1704/1706/1754`），不算错；
    /// ② 写了但非法（如 `DropItemRage=300` 超出 `byte`）⇒ 报错。
    #[test]
    fn setup_drop_pickup_keys_and_defaults() {
        let dir = std::env::temp_dir().join(format!("gs-conf-drop-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=12288\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        let p = write(
            &dir,
            &format!(
                "{base}[Setup]\nHumanMaxGold=10000000\nFloorItemCanPickUpTime=120000\nControlDropItem=1\nInSafeDisableDrop=0\nCanDropPrice=500\nDropItemRage=3\nSendRefMsgRange=15\n"
            ),
        );
        let c = load(&p).unwrap();
        assert_eq!(c.human_max_gold, 10_000_000);
        assert_eq!(c.floor_item_can_pick_up_time, 120_000);
        assert!(c.control_drop_item);
        assert!(!c.in_safe_disable_drop);
        assert_eq!(c.can_drop_price, 500);
        assert_eq!(c.drop_item_rage, 3);
        assert_eq!(c.send_ref_msg_range, 15, "[Setup] SendRefMsgRange 实值");

        // 缺键 ⇒ oracle 缺省（`SendRefMsgRange` 缺省 12，`GameSvrConf.cs:1503`）。
        let p2 = write(&dir, base);
        let c2 = load(&p2).unwrap();
        assert_eq!(c2.human_max_gold, 10_000_000);
        assert_eq!(c2.floor_item_can_pick_up_time, 120_000);
        assert!(!c2.control_drop_item);
        assert!(!c2.in_safe_disable_drop);
        assert_eq!(c2.can_drop_price, 500);
        assert_eq!(c2.drop_item_rage, 3);
        assert_eq!(c2.send_ref_msg_range, 12, "缺键 ⇒ SendRefMsgRange 缺省 12");

        // 写了但非法 ⇒ 必须报错（不静默截断）。
        let p3 = write(&dir, &format!("{base}[Setup]\nDropItemRage=300\n"));
        assert!(load(&p3).is_err(), "DropItemRage=300 超出 byte ⇒ 必须报错");
        let p5 = write(&dir, &format!("{base}[Setup]\nSendRefMsgRange=300\n"));
        assert!(
            load(&p5).is_err(),
            "SendRefMsgRange=300 超出 byte ⇒ 必须报错"
        );
        let p4 = write(&dir, &format!("{base}[Setup]\nControlDropItem=maybe\n"));
        assert!(load(&p4).is_err(), "ControlDropItem 非布尔 ⇒ 必须报错");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `[Setup] HearMsgFColor/HearMsgBColor`（现网 `Server.conf` 实值 `0` / `255`）：解析到
    /// `GateConfig` 供 `CM_SAY` 回帧 `SM_HEAR` 的 `Param` 低/高字节。
    #[test]
    fn hears_msg_colors_from_setup_section() {
        let dir = std::env::temp_dir().join(format!("gs-conf-hear-{}", std::process::id()));
        let p = write(
            &dir,
            "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=12288\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n[Setup]\nHearMsgFColor=0\nHearMsgBColor=255\n",
        );
        let c = load(&p).unwrap();
        assert_eq!(c.hear_msg_f_color, 0);
        assert_eq!(c.hear_msg_b_color, 255);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **B-1**（reviewer-2 `b49897e8`）：现网 `Server.conf` 首字节是 UTF-8 BOM（`ef bb bf`）。
    ///
    /// oracle `ConfigFile.Load` 用 `new StreamReader(path, GB2312)` 且**默认做 BOM 探测**
    /// （`ConfigFile.cs:263`）⇒ 首行 BOM 被吞掉，`[DataBase]` 段照常识别。Rust 若不剥 BOM，
    /// `[DataBase]` 头丢失 ⇒ `ConnctionString` 缺键 ⇒ 回落 `database=mir2`（现网无此库）
    /// ⇒ `stditems` 恒空 ⇒ **带装备角色的 50/41 与 oracle 偏离**（裸装夹具/真机号看不出来）。
    /// 本用例把「带 BOM 的现网形态」钉住；把 `load` 里的 `strip_prefix('\u{feff}')` 改回去 ⇒ 本用例必红。
    #[test]
    fn deployed_conf_with_utf8_bom_reads_database_section() {
        let dir = std::env::temp_dir().join(format!("gs-bom-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Server.conf");
        // 首 3 字节照现网事实（`head -c 32 E:/MirServer/M2GameSvr/Server.conf | xxd` = `ef bb bf`）。
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(
            b"[DataBase]\nDBType=MySQL\nConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_data;\n[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\nIDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n",
        );
        std::fs::write(&p, bytes).unwrap();
        let c = load(&p).expect("带 BOM 的现网形态必须能解析（BOM 只在文件首剥一个）");
        assert_eq!(
            c.db_conn_string.as_deref(),
            Some("server=127.0.0.1;uid=root;pwd=;database=mir2_data;"),
            "BOM 被吞后 [DataBase] ConnctionString 必须读到（现网 database=mir2_data）"
        );
        assert_eq!(c.gate_port, 5000, "BOM 后的 [Server] 段也要正常解析");
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
        assert!(
            !c.show_rank_level_name,
            "缺 [Setup] ShowRankLevelName ⇒ oracle 缺省 false"
        );
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

    /// **`openmir2-m2-session-lifecycle`**：`[Setup] RedMsgFColor/RedMsgBColor` 照
    /// `ServerConf.cs:333-334` 的 `ReadWriteByte` 读；**缺键用 oracle 缺省**（0xFF/0x38）不自造。
    #[test]
    fn parses_setup_red_msg_colors_with_oracle_defaults() {
        let dir = std::env::temp_dir().join(format!("gs-setup-colors-{}", std::process::id()));
        let base = "[Server]\nGateAddr=127.0.0.1\nGatePort=5000\nCheckBlock=1\n\
                    IDSAddr=127.0.0.1\nIDSPort=5600\nDBAddr=127.0.0.1\nDBPort=6000\n";
        // 不写 [Setup] ⇒ 缺省（现网 Server.conf: RedMsgFColor=255 / RedMsgBColor=56）。
        let p = write(&dir, base);
        let c = load(&p).unwrap();
        assert_eq!(c.red_msg_f_color, 0xFF, "GameSvrConf.cs:1540 缺省");
        assert_eq!(c.red_msg_b_color, 0x38, "GameSvrConf.cs:1541 缺省");
        // 写了就照写读，值带前导空格也要 trim。
        let p2 = write(
            &dir,
            &format!("{base}[Setup]\nRedMsgFColor= 255\nRedMsgBColor=56\n"),
        );
        let c2 = load(&p2).unwrap();
        assert_eq!(c2.red_msg_f_color, 255);
        assert_eq!(c2.red_msg_b_color, 56);
        // 写了但越界（300 > u8::MAX）⇒ 错误（不是静默取默认）。
        let p3 = write(&dir, &format!("{base}[Setup]\nRedMsgBColor=300\n"));
        assert!(load(&p3).is_err(), "RedMsgBColor=300 超出 u8 ⇒ 必须报错");
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
