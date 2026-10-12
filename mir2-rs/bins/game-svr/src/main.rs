//! Rust 版 **GameSvr 世界宿主**（M2 主体，切片 S1：进程骨架 + 网关链路）。
//!
//! 这一片只做两件事，并把其余登记清楚（不静默跳过）：
//! 1. **网关↔GameSvr 链路**：同 20B `ServerMessage` 头 + `Grobal2.GM_*`
//!    （口径与 `bins/game-gate` 的协议梳理 §3 一致）——`GM_OPEN` 登记用户并回 `GM_SERVERUSERINDEX`、
//!    `GM_DATA` 解析 12B 命令头（+ 可选的 EDCode 尾）并按 `SessionIndex-1`/socket 路由、
//!    `GM_CLOSE` 按 socket 清槽、`GM_CHECKCLIENT`/`GM_RECEIVE_OK` 维护保活/阻塞计数。
//! 2. **单线程 tick 宿主**：`std::thread` 拥有 `crats/world::World`，按 `TICK_INTERVAL_MS` 调 `World::tick`；
//!    I/O（tokio）侧**只发事件**、绝不碰 `World`（设计文档 §4.2 的四条确定性硬约束）。
//!
//! **本切片不做**（登记在卡内与报告，S2/S3/S4 承接）：
//! - 认证链（`DoClientCertification`）已落 `cert.rs`；**取角色/进世界**（`LoadDBInfo` → DBSrv → PlayObject）——S2b-3；
//! - 会话**进世界**（`World::open_session`/`enter_map`）与命令到世界的映射——S2；
//! - 世界 → 客户端的 **SM_* 出包**（`SendMsgQueue`/编码）——S3；
//! - 真地图/Envir 加载：已接入 `crates/envir` 的 `MapRegistry`（集成点 ④）——有 Envir 时按
//!   `CurMap`→`map_id` 建世界并拒绝未知地图，无 Envir 时退回 `WorldMaps` 占位（见 `mapwire.rs`）；
//! - 保活定时器（消费者是 **M2Server 自己** 的 `TCPNetChannel.cs:435-439`，本片只维护标志）。
//!
//! **失败要响**：缺配置键、绑不上端口 —— 明确报错并非零退出。

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

mod auth;
mod cert;
mod config;
mod dbclient;
mod enter;
mod homemap;
mod hutil;
mod ids;
mod items;
mod load;
mod magicwire;
mod mapwire;
mod notice;
mod record;
mod seed;
mod state_log;
mod strings;
mod users;

use std::sync::Arc;

use anyhow::{Context, Result};
use game_svr::chat;
use game_svr::downlink;
use game_svr::movewire;
use game_svr::smframe;
use mir2_protocol::frame::ServerMessage;
use mir2_protocol::messages::grobal2;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

use users::UserList;

/// C# `CommandMessage.Size`（`src/OpenMir2/Packets/ClientPackets/ClientMesaagePacket.cs:24`）。
const COMMAND_MESSAGE_SIZE: usize = 12;

/// Serilog 同形日志前缀（便于与两侧日志对时间线）。
#[macro_export]
macro_rules! glog {
    ($lvl:expr, $($arg:tt)*) => {{
        let now = ::std::time::SystemTime::now()
            .duration_since(::std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let ms = now.subsec_millis();
        let secs = now.as_secs();
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        println!("{h:02}:{m:02}:{s:02}.{ms:03} [{}] {}", $lvl, format!($($arg)*));
    }};
}

/// sim 侧 → I/O 侧的命令（世界/读档流程要动网关连接持有的槽表时用）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum SimToGate {
    /// `NetChannel.CloseUser(gateIdx, socket)`（`TCPNetChannel.cs:221-224`）的对应物：清掉该 socket 的槽。
    KickUser { socket: i32 },
    /// 进世界完成（`World::open_session` + `enter_map` 成功）⇒ 该槽的 `in_world` 置真
    /// （oracle 里是 `gateUser.PlayObject`/`WorldEngine` 挂上，`:201` 的分支门就是它）。
    MarkInWorld { socket: i32 },
    /// S2b-4 续：世界 → 客户端的一条 `SM_*` 帧（`body` = 12B 命令头 + 尾）。
    /// gate 侧按 oracle `ClientSession.cs:897-920` 变成 `'#' + Encode(头12B) + 尾 + '!'` 发客户端。
    SendFrame { socket: i32, body: Vec<u8> },
    /// `openmir2-m3-move-wiring`：`SendSocket(string)` 的**短包**支（oracle `PlayObject.Socket.cs:48-71`
    /// 把 `PackLength` 置为 `-len`、正文原样）。网关端 `client_frame(pack_length < 0)` 变成
    /// `'#' + 正文 + '!'` —— 真机观测到的 `#+GD/<tick>!`（`M2Share.GetGoodTick`）就是这条。
    SendRaw { socket: i32, payload: Vec<u8> },
}

/// I/O 侧 → sim 侧的事件（sim 线程是唯一碰 `World` 的地方）。
#[derive(Debug)]
enum GateEvent {
    /// 新用户登记（`GM_OPEN`）。
    Opened { index: i32, ip: String },
    /// 客户端命令帧（`GM_DATA` 里是 12B 命令头 + 可选 EDCode 尾）。
    ///
    /// `chr_name` = `ChannelMessageHandler.cs:170-198` 在该网关连接自己的 `UserList` 上解出的
    /// 说话者（先按 `SessionIndex-1` 且 socket 相符，否则线性扫 socket）——`CM_SAY` 要按它找回
    /// `online` 登记表里的说话者；只按 `socket` 查会在**不同网关线 socket 号重复**时认错人。
    /// `socket` = 该槽的客户端句柄（回帧路由仍要用它）。
    Command {
        index: i32,
        chr_name: String,
        socket: i32,
        cmd: CommandMessage,
        tail: Option<String>,
    },
    /// 没通过认证的会话收到的帧：oracle 在这一支走 `DoClientCertification` 的空转（`Account` 为空才真认证）。
    CertificationPending { index: i32, ip: String },
    /// `GM_CLOSE`。
    ///
    /// `chr_name` = 关断时该槽已认证的角色名（未认证则为空）；`socket` = 该槽的客户端句柄。
    /// 两者用于 `openmir2-m2-session-lifecycle` 的「断线清世界」：
    /// oracle `ChannelMessageHandler.CloseUser`（`:363-...`）先 `FrontEngine.DeleteHuman(i, socket)`，
    /// 已进世界的 PlayObject 再走 `BoSoftClose` ⇒ `RunNotice` 的 `MakeGhost()`。
    Closed {
        index: i32,
        chr_name: String,
        socket: i32,
    },
    /// 认证通过 ⇒ 取角色（oracle `:323-338` 的 `LoadDBInfo` + `AddToLoadRcdList`）。
    /// 事件直接带 `LoadDbInfo`，交给 sim 线程的 `FrontEngine` 队列。
    LoadDbRequest(load::LoadDbInfo),
    /// 网关连接建好：登记它的 `sim → gate` 命令通道（`service_id` = 该连接的线号 = oracle 的 `GateIdx`）。
    RegisterGate {
        service_id: u8,
        tx: mpsc::UnboundedSender<SimToGate>,
    },
    /// 网关连接断开：注销它的线号通道（oracle `Disconnected` 释放槽位）。
    UnregisterGate { service_id: u8 },
    /// **账号级顶号**（`openmir2-m2-session-lifecycle` ①）：IDS 链路收到 `SS_CLOSESESSION`
    /// （`AuthenticationService.GetCancelAdmission` → `DelSession`）后，若被删的是某在线账号的会话，
    /// 则照 `TCPNetChannel.KickUser` 给**旧**连接发 `SM_SYSMESSAGE` 并软关（`BoEmergencyClose`/`BoSoftClose`）。
    ///
    /// 与 [`Self::Closed`] 的区别：这是**服务器主动**踢（不是对端断线）。
    KickAccount { account: String, session_id: i32 },
}

/// C# `CommandMessage`（12B 解码态布局，`:11-24`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CommandMessage {
    recog: i32,
    ident: u16,
    param: u16,
    tag: u16,
    series: u16,
}

impl CommandMessage {
    /// 从解码态字节解析（不足 12B 返回 `None`）。
    #[must_use]
    fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < COMMAND_MESSAGE_SIZE {
            return None;
        }
        Some(Self {
            recog: i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ident: u16::from_le_bytes([b[4], b[5]]),
            param: u16::from_le_bytes([b[6], b[7]]),
            tag: u16::from_le_bytes([b[8], b[9]]),
            series: u16::from_le_bytes([b[10], b[11]]),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let conf = std::env::args()
        .nth(1)
        .map_or_else(config::default_path, std::path::PathBuf::from);
    let cfg = config::load(&conf)?;
    // **M3 T-30**：`MIR2_RNG_SEED` 同种子注入（读取口径照 C# 侧 `m3/rng-hook/StartupHook.cs`）。
    // 未设/空 ⇒ `None`（生产路径保持既有随机源，行为一字不改）；非法值 ⇒ 显式非零退出
    // （fail-closed，不许「以为注入了种子、其实没有」）。
    let rng_seed = seed::env_seed()?;
    glog!(
        "INF",
        "游戏服务启动：监听 {}:{}（CheckBlock={}，{ } 张地图占位，config={}）",
        cfg.gate_addr,
        cfg.gate_port,
        cfg.check_block,
        cfg.maps.len(),
        conf.display()
    );

    // DB 链路（GameSvr → DBSvr）：oracle 的 `DataQueryServer` 也是 TcpClient 主动连（`:25-33`）。
    // 本切片只**发**取角色请求；收/解析属 S2b-2b。
    let db_state = dbclient::DbLinkState::new();
    let db_replies = std::sync::Arc::new(std::sync::Mutex::new(dbclient::DbReplies::new()));
    let (db_req_tx, db_req_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    {
        let db_state = db_state.clone();
        let db_addr = cfg.db_addr.clone();
        let db_port = cfg.db_port;
        let replies = std::sync::Arc::clone(&db_replies);
        tokio::spawn(async move {
            dbclient::run_db_link(db_addr, db_port, db_state, db_req_rx, replies).await;
        });
    }
    // 查询号：`SystemShare.Config.DBQueryID++`（`PlayerDataService.GetQueryId` `:183-191`，从 1 起、溢出回 1）
    let db = DbWiring {
        state: db_state,
        req_tx: db_req_tx,
        next_query_id: std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0)),
        replies: db_replies,
        outstanding: Vec::new(),
        save_outstanding: Vec::new(),
    };

    // sim 侧：一个专用线程独占 World（§4.2 第 1 条：不许并行 executor 驱动世界）
    let (ev_tx, ev_rx) = std::sync::mpsc::channel::<GateEvent>();
    // 准入会话表（C# `AuthenticationService._sessionList`）：跨网关连接共享；
    // 填充它的是登录链的通知（S2b 的传输），本切片只实现表语义与认证匹配。
    let auth: Arc<std::sync::Mutex<auth::AdmissionStore>> =
        Arc::new(std::sync::Mutex::new(auth::AdmissionStore::new()));
    // 真地图注册表（`openmir2-m2-map-loading` 集成点 ④）：BasePath = `Server.conf` 所在目录
    // （生产下即 `config::default_path()` 的可执行体目录，等价 oracle `M2Share.BasePath`），
    // `EnvirDir`/`MapDir` 照 `[Share]`。拿不到 Envir 时保留 `WorldMaps` 占位并记 WRN
    // （占位键仅开发/测试用）。
    let base = mapwire::base_path(&conf);
    let registry = match mapwire::load_registry(&base, &cfg.envir_dir, &cfg.map_dir) {
        mapwire::Maps::Envir(reg) => {
            glog!(
                "INF",
                "Envir 真地图注册表：{} 张（base={} EnvirDir={} MapDir={}，跳过 {} 张）",
                reg.len(),
                base.display(),
                cfg.envir_dir,
                cfg.map_dir,
                reg.skipped().len()
            );
            Some(reg)
        }
        mapwire::Maps::Placeholder => {
            glog!(
                "WRN",
                "未找到 Envir/MapInfo.txt（base={} EnvirDir={} MapDir={}）⇒ 地图尺寸用 WorldMaps 占位、CurMap 不校验（开发/测试形态，登记：真机验收后删占位键）",
                base.display(),
                cfg.envir_dir,
                cfg.map_dir
            );
            None
        }
        // 「Envir 在但一张图都没加载成功」与「没有 Envir」不是一回事：前者必须拒绝启动，
        // 否则会静默退回 0 号占位图（卡面 ④ 明禁）。
        mapwire::Maps::Broken(why) => {
            anyhow::bail!("Envir 地图配置损坏：{why}（拒绝以占位图启动）")
        }
    };
    // `WorldMaps` 可选化的 **fail-closed 半边**（owner `272582e1`）：有真注册表就用它的尺寸；
    // 无 Envir 但有 `WorldMaps` ⇒ 占位尺寸（上面已记 WRN）；**两者都没有 ⇒ 拒绝启动**
    // （不许静默起空世界 / 回落 0 号占位图）。"缺某个键"与"键写了但坏"都在这里或 config 里响。
    let map_sizes = match &registry {
        Some(reg) => mapwire::sizes(reg),
        None => mapwire::placeholder_sizes(&cfg.maps).map_err(anyhow::Error::msg)?,
    };
    // 配置层输入（帧 11029/1106/5008/811；`openmir2-m2-envir-inputs`）：与地图注册表同源，
    // 都读 `Server.conf`。读不到 ⇒ 记 WRN 并跳过依赖它的帧（不猜；见 `enter::apply`）。
    let game_conf = match mir2_envir::GameSvrConf::load(&conf) {
        Ok(c) => {
            glog!(
                "INF",
                "配置层已载入：[Names] GameGold={} GamePoint={}（帧 5008）/ [Setup] BonusAbilofWarr/NakedAbilofWarr 等（帧 811）",
                c.game_gold_name,
                c.game_point_name
            );
            Some(c)
        }
        Err(e) => {
            glog!(
                "WRN",
                "读取配置层失败（{}）：{e} ⇒ 帧 11029/1106/5008/811 跳过（登记）",
                conf.display()
            );
            None
        }
    };
    // `String.conf`（帧 42 `SM_USERNAME` 的展示名模板；`openmir2-m2-enter-world-frames`）：
    // oracle `SystemShare.StringConf`（`SystemShare.cs:187`，与 `Server.conf` 同目录名 `string.conf`）。
    // 缺文件按 oracle `ConfigFile.Load` 的「建空文件 + 沿源码默认值」处理（不当错误）。
    let string_conf = match strings::load(&base.join("string.conf")) {
        Ok(c) => c,
        Err(e) => {
            glog!(
                "WRN",
                "读取 String.conf 失败（{e}）⇒ 帧 42 用 oracle 源码默认模板（登记）"
            );
            strings::StringConf::defaults()
        }
    };
    // 拷出（`bool`）供 `move` 闭包使用；`cfg` 在 spawn 之后还要用（IDS/DB 端点）。
    let show_rank_level_name = cfg.show_rank_level_name;
    // 公告文本（658 `SM_SENDNOTICE` 的配置层输入）：`<base>/<NoticeDir>/Notice.txt`。
    // 与 oracle `NoticeMgr.GetNoticeMsg("Notice")` 同源；**R1-1**：读不到/为空时 oracle 仍发
    // 一条 header-only 658（`SendDefMessage` 空尾走 `SendSocket(ClientMsg)`），故这里只记 WRN。
    let notice = notice::load_notice(&base, &cfg.notice_dir);
    if notice.is_empty() {
        glog!(
            "WRN",
            "公告为空/缺失（base={} NoticeDir={}）⇒ 仍发 header-only 658（照 oracle）",
            base.display(),
            cfg.notice_dir
        );
    } else {
        glog!(
            "INF",
            "公告已载入：{} 字节（base={} NoticeDir={}）",
            notice.len(),
            base.display(),
            cfg.notice_dir
        );
    }
    // 出生点回落（`WorldServer.cs:250-260` 的 `GetHomeInfo`）：空 `HomeMap` 的新角色用它进图（见
    // `homemap` 模块头的缺陷现场）。`<base>/<EnvirDir>/StartPoint.txt` + `[Setup] StartPointSize/HomeMap/HomeX/HomeY`。
    let start_points = homemap::load_start_points(&base, &cfg.envir_dir);
    let home_fallback = homemap::HomeFallback {
        start_point_size: cfg.start_point_size,
        home_map: cfg.home_map.clone(),
        home_x: cfg.home_x,
        home_y: cfg.home_y,
        start_points,
    };
    glog!(
        "INF",
        "出生点回落已载入：StartPoint.txt {} 条，StartPointSize={}，Config.HomeMap={} ({},{})（空 HomeMap 的新角色用它；RNG 抽数照 oracle）",
        home_fallback.start_points.len(),
        home_fallback.start_point_size,
        home_fallback.home_map,
        home_fallback.home_x,
        home_fallback.home_y
    );
    // 消息颜色（`[Setup] RedMsgFColor/BColor`）：帧 `SM_SYSMESSAGE` 的 `Param`，
    // 账号级顶号提示（`openmir2-m2-session-lifecycle`）用它。
    let colors = MsgColors {
        red_f: cfg.red_msg_f_color,
        red_b: cfg.red_msg_b_color,
        hear_f: cfg.hear_msg_f_color,
        hear_b: cfg.hear_msg_b_color,
    };
    // 物品模板表（帧 50/41 的 `UseItems[Dress/Weapon].StdItem.Shape` 输入）：oracle 在 `GameApp.Initialize`
    // 直连 MySQL 读 `stditems`（`MySqlDB.LoadItemsDB`，不经 DBSvr）；加载失败 ⇒ 空表
    // （`GetStdItem` 恒 null ⇒ 形态计 0，与 oracle 空物品系统分支同形）。
    let item_table = {
        let conn = cfg
            .db_conn_string
            .as_deref()
            .unwrap_or(items::DEFAULT_CONN_STRING);
        match items::load_items(conn).await {
            Ok(list) => {
                glog!(
                    "INF",
                    "物品模板表 stditems：{} 条（直连 MySQL，帧 50/41 的 Shape 输入）",
                    list.len()
                );
                list
            }
            Err(e) => {
                glog!(
                    "WRN",
                    "读 stditems 失败（{e}）⇒ 装备形态按 `GetStdItem == null` 计 0（oracle 空物品系统分支）"
                );
                Vec::new()
            }
        }
    };
    // 查表口径只有一份：装成 `game_svr::ability::StdItemCatalog`（1-based + 空名按缺），
    // 供帧 50/41 的 `StdItem.Shape` 与 52/752 的 `RecalcAbilitys` 共用。
    let item_catalog = items::catalog_from(&item_table);
    // 经验表（`Exps.conf` 的 `NeedExps`，`BaseObject.GetLevelExp` 的数据源）：存档 `MaxExp`
    // （`PlayObject.cs:1310`）与击杀经验都用它。读不到 ⇒ 全 0（oracle `Config.NeedExps` 未载入同形；登记）。
    let need_exps = match mir2_data::exps::ExpsConfig::from_file(&base.join("Exps.conf")) {
        Ok(c) => {
            glog!(
                "INF",
                "经验表 Exps.conf 已载入：NeedExps[1..] 非零 {} 项",
                c.need_exps.iter().filter(|v| **v != 0).count()
            );
            c.need_exps.to_vec()
        }
        Err(e) => {
            glog!(
                "WRN",
                "读取 Exps.conf 失败（{e}）⇒ NeedExps 全 0（存档 MaxExp / 击杀经验登记）"
            );
            vec![0; mir2_data::models::MAX_CHANGE_LEVEL]
        }
    };
    // **M3④-实现 B 第 ③ 片**：怪物模板表（`monsters` + MonItems）+ `Envir/MonGen.txt` 运行时条目
    // + `Server.conf` 刷怪参数。三者任一拿不到时刷怪不启（登记），不影响既有链路。
    let monster_table = {
        let conn = cfg
            .db_conn_string
            .as_deref()
            .unwrap_or(items::DEFAULT_CONN_STRING);
        let envir_dir = base.join(&cfg.envir_dir);
        match game_svr::monitems::load_monsters(conn, &envir_dir, cfg.monster_power_rate).await {
            Ok(load) => {
                glog!(
                    "INF",
                    "怪物模板表 monsters：{} 条（MonItems 挂接 {}/{}，{} 条掉落），刷怪输入就绪",
                    load.monsters.len(),
                    load.report.attached,
                    load.report.total,
                    load.report.entries
                );
                game_svr::mongen::MonsterTable::new(load.monsters)
            }
            Err(e) => {
                glog!("WRN", "读 monsters 失败（{e}）⇒ 刷怪表为空（登记：不刷怪）");
                game_svr::mongen::MonsterTable::new(Vec::new())
            }
        }
    };
    let mon_gen_entries = {
        let loader = mir2_envir::MapLoader::new(&base, &cfg.map_dir, &cfg.envir_dir);
        match registry.as_ref() {
            Some(reg) => match game_svr::mongen::load_mon_gen_entries(&loader, reg) {
                Ok(Some(entries)) => {
                    glog!(
                        "INF",
                        "Envir/MonGen.txt 已载入：{} 条刷怪配置（有文件 ⇒ 末尾哨兵占 thread 0 首槽）",
                        entries.len()
                    );
                    Some(entries)
                }
                Ok(None) => {
                    glog!("INF", "无 Envir/MonGen.txt ⇒ 不刷怪（oracle result=0）");
                    None
                }
                Err(e) => {
                    glog!("WRN", "加载 Envir/MonGen.txt 失败（{e}）⇒ 不刷怪（登记）");
                    None
                }
            },
            None => {
                glog!(
                    "WRN",
                    "无 Envir 真地图注册表 ⇒ 跳过 Envir/MonGen.txt（不刷怪）"
                );
                None
            }
        }
    };
    let mon_gen_config = match game_svr::mongen::mon_gen_config_from_conf(&conf) {
        Ok(c) => c,
        Err(e) => {
            glog!(
                "WRN",
                "解析刷怪配置失败（{e}）⇒ 用 GameSvrConf 构造缺省（登记）"
            );
            mir2_world::mongen::MonGenConfig::default()
        }
    };
    let _sim = std::thread::Builder::new()
        .name("m2-sim".to_string())
        // 公告文本（oracle `NoticeMgr.GetNoticeMsg("Notice")`，帧 658 `SM_SENDNOTICE`）：
        // 配置层输入，启动时读一次、进程内复用；**R1-1**：读不到时为空字节，但生产路径仍发
        // header-only 658（照 oracle 空尾分支；`notice.rs` 模块头有源码定位）。
        .spawn(move || {
            sim_loop(
                map_sizes,
                registry,
                EnterConfig {
                    game_conf,
                    string_conf,
                    show_rank_level_name,
                    catalog: item_catalog,
                    ground: GroundCfg {
                        human_max_gold: cfg.human_max_gold,
                        floor_item_can_pick_up_time: cfg.floor_item_can_pick_up_time,
                        control_drop_item: cfg.control_drop_item,
                        in_safe_disable_drop: cfg.in_safe_disable_drop,
                        can_drop_price: cfg.can_drop_price,
                        drop_item_rage: cfg.drop_item_rage,
                    },
                    ref_msg_range: i32::from(cfg.send_ref_msg_range),
                    save_human_rcd_time: i64::from(cfg.save_human_rcd_time),
                    health_fill_time: cfg.health_fill_time,
                    spell_fill_time: cfg.spell_fill_time,
                    home: home_fallback,
                    newbie: NewbieItems {
                        candle: cfg.candle.clone(),
                        basic_drug: cfg.basic_drug.clone(),
                        wooden_sword: cfg.wooden_sword.clone(),
                        cloths_man: cfg.cloths_man.clone(),
                        cloths_woman: cfg.cloths_woman.clone(),
                        item_number: cfg.item_number,
                    },
                    need_exps,
                    rng_seed,
                    mon_gen: MonGenStartup {
                        table: monster_table,
                        entries: mon_gen_entries,
                        config: mon_gen_config,
                    },
                },
                notice,
                colors,
                ev_rx,
                Some(db),
            )
        })
        .context("起 sim 线程失败")?;

    // 登录服务器（IDS）链路：oracle 是 TcpClient 主动连它（`AuthenticationService.cs:22-34`），
    // 会话通知（SS_OPENSESSION/SS_CLOSESESSION）填/删准入表 ⇒ 认证链靠它
    {
        let store = Arc::clone(&auth);
        let ids_addr = cfg.ids_addr.clone();
        let ids_port = cfg.ids_port;
        let kick_tx = ev_tx.clone();
        tokio::spawn(async move { ids::run_ids_link(ids_addr, ids_port, store, kick_tx).await });
    }

    let listener = TcpListener::bind((cfg.gate_addr.as_str(), cfg.gate_port))
        .await
        .with_context(|| {
            format!(
                "绑定 {}:{} 失败（端口被占？）",
                cfg.gate_addr, cfg.gate_port
            )
        })?;
    glog!(
        "INF",
        "游戏服务[{}:{}]已启动...",
        cfg.gate_addr,
        cfg.gate_port
    );
    // 网关（GameGate）以客户端身份连上来；同一进程可接待多条网关连接，但**各自独立的会话表**。
    // sim 线程随进程存活（`join` 不必要：世界只由它驱动，进程退出即结束）。
    // 线号按 oracle `TCPNetChannel.Connecting` 的 `AllocGateSlot()` 分配：**每个连接占一个槽**，
    // 断线释放后复用最小空闲槽（`TCPNetChannel.cs:74-92`）。**不能恒为 0**：三条网关连接都登记成
    // 0 号线 ⇒ sim 的 `gates` 表只剩最后一条，进世界帧按 `gate_idx` 路由会打到别的连接上，
    // `UserList` 找不到 socket ⇒ 静默丢弃顶包（真机症状＝客户端黑屏；`TCPNetChannel.cs:26-31`
    // 的注释描述了同一坑："世界下发的数据就打到已断开的旧槽位上——表现为客户端进世界黑屏"）。
    let slots: Arc<std::sync::Mutex<GateSlots>> =
        Arc::new(std::sync::Mutex::new(GateSlots::default()));
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        glog!("INF", "游戏网关连接：{peer}");
        let service_id = {
            let mut g = slots.lock().expect("gate slots poisoned");
            match g.alloc() {
                Some(i) => i,
                None => {
                    glog!("ERR", "网关槽位已满（20），拒绝连接 {peer}");
                    continue;
                }
            }
        };
        glog!("DBG", "sim: 网关连接 {peer} 分配线号 {service_id}");
        let ev_tx = ev_tx.clone();
        let check_block = cfg.check_block;
        let auth = Arc::clone(&auth);
        let slots_done = Arc::clone(&slots);
        tokio::spawn(async move {
            if let Err(e) = gate_conn(sock, ev_tx.clone(), check_block, auth, service_id).await {
                glog!("WRN", "网关连接结束：{e:#}");
            }
            // 注销线号通道并释放槽位（重连时复用回同一个最小空闲号，与 oracle 同）。
            let _ = ev_tx.send(GateEvent::UnregisterGate { service_id });
            slots_done
                .lock()
                .expect("gate slots poisoned")
                .free(service_id);
            glog!("DBG", "sim: 网关线号 {service_id} 已释放");
        });
    }
}

/// sim 线程：建世界 → 按 `TICK_INTERVAL_MS` tick → 消费 I/O 侧事件。
///
/// 目前把事件**分类记录**（世界侧的会话/命令映射、取角色进世界是 S2b-3），但 tick 循环与"唯一所有者"这条纪律走真路径。
/// sim 侧用到的 DB 接线（没配 DB 时传 `None` ⇒ 读档恒走"取数失败"支）。
#[derive(Debug)]
struct DbWiring {
    state: dbclient::DbLinkState,
    req_tx: mpsc::UnboundedSender<Vec<u8>>,
    next_query_id: std::sync::Arc<std::sync::atomic::AtomicI32>,
    /// 收侧汇总（分帧 + 签名 + 查询表），由 DB 链路任务与 sim 线程共享。
    replies: std::sync::Arc<std::sync::Mutex<dbclient::DbReplies>>,
    /// 已发出、还没取走的 `(query_id, socket, gate_idx, soft_version_date, session_id, account)`——socket 用于回复到达后把该槽标进世界，
    /// `gate_idx` 用于把标记/踢人**路由回该用户的网关线**（oracle 每个 GameGate 连接一套 `SessionUser` 表，
    /// `ChannelMessageHandler.cs:318-322` 的 `gateUser` 属该连接；各网关的 socket 号可重复 ⇒ 不能「随便挑一个」）。
    /// `soft_version_date` = `LoadDBInfo.SoftVersionDate`（oracle `PlayObject.SoftVersionDateEx` 的来源，
    /// `WorldServer.cs:469`）——它是**取档请求**的字段、不在角色记录里，须随查询带过来做帧 11029/5008 的门。
    /// `session_id`/`account` 同样是取档请求的字段（不在 `PlayerDataHead` 里），用于进世界时登记
    /// 在线表（账号级顶号 `KickUser` 按 `Account`/`SessionID` 匹配）。
    outstanding: Vec<(i32, i32, i32, i32, i32, String)>,
    /// 已发出、还没取走的**存档**应答 `(query_id, ChrName)`（`DBR_SAVEHUMANRCD(1102)` 或失败时的
    /// `DBR_LOADHUMANRCD(1100)`）——取走时同步 `FrontEngine.end_save`（oracle `RemoveSaveList`
    /// 的时机就是「应答回来」，`WorldServer.cs:1091` ↔ 进世界守卫 `:203`）。
    save_outstanding: Vec<(i32, String)>,
}

impl DbWiring {
    /// `GetQueryId()`（`PlayerDataService.cs:183-191`）：先自增，再从 1 起、溢出回 1。
    fn next_query_id(&self) -> i32 {
        use std::sync::atomic::Ordering;
        let q = self.next_query_id.fetch_add(1, Ordering::SeqCst) + 1;
        if q > i32::MAX - 1 {
            self.next_query_id.store(1, Ordering::SeqCst);
            return 1;
        }
        q
    }
}

/// `NetChannel` 按 gateIdx 选连接（`TCPNetChannel.cs:221-224`）：取该线号的 `sim → gate` 命令通道。
///
/// 读档流程里的两条 sim→gate 命令（`KickUser`/`MarkInWorld`）**都**走这里 —— 每个 GameGate 连接的
/// `UserList` 是独立的，socket 号在网关之间可重复，因此只能按 `gate_idx` 命中，找不到线号就记 WRN。
///
/// **R4-1**：`gate_idx` 来自客户端认证串第 7 段，除 `-1` 外还可能是 `-2`/`300` 一类越界或非法值。
/// oracle `TCPNetChannel.cs:213-218` 的 `GetGate(gateIdx)` 对越界/非法线号返回 `null` ⇒ **直接丢弃**；
/// 本函数与之同构：`u8::try_from` 失败即记 WRN + 返回 `None`，**绝不** `unwrap_or(0)` 把非法线号
/// 静默折算成 0 号线（与 N1「不许静默挑线」同族）。
fn route_to_gate<'a>(
    gates: &'a std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    gate_idx: i32,
    socket: i32,
    what: &str,
) -> Option<&'a mpsc::UnboundedSender<SimToGate>> {
    let Some(idx) = u8::try_from(gate_idx).ok() else {
        glog!(
            "WRN",
            "网关线号 {gate_idx} 越界/非法（不在 0..=255），无法{what} socket={socket}（照 oracle GetGate 丢弃）"
        );
        return None;
    };
    match gates.get(&idx) {
        Some(tx) => Some(tx),
        None => {
            glog!(
                "WRN",
                "找不到网关线号 {gate_idx}，无法{what} socket={socket}"
            );
            None
        }
    }
}

/// `CM_SAY`(3030) 的**普通说话**落地：按在网关连接自己的 `UserList` 上解出的 `chr_name`
/// 找回 `online` 登记表里的说话者，组 `SM_HEAR` 帧，
/// 经 `chat::hear_targets`（`SendRefMsg` 的收件人集合）逐收件人按其 `[GateIdx, Socket]` 路由。
///
/// 口径见 `chat` 模块头；非普通行（`/`、`!`、`@`）本切片不接并记日志（不静默丢）。
fn handle_say(
    online: &OnlineRegistry,
    world: &mir2_world::World,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    chr_name: &str,
    socket: i32,
    tail: Option<&str>,
    colors: &MsgColors,
) {
    let Some(speaker) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "CM_SAY：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 不猜说话者，丢弃 tail={tail:?}"
        );
        return;
    };
    let Some(line) = tail else {
        return;
    };
    if !chat::is_plain_say(line) {
        glog!(
            "DBG",
            "CM_SAY：非普通说话（频道/命令），本切片未接 ⇒ 丢弃 ChrName={} tail={line:?}",
            speaker.chr_name
        );
        return;
    }
    // `BaseObject.Base.cs:478` 的 `ChrName + ':' + sMsg`（GBK 字节；协议层不转 UTF-8）。
    let text = chat::hear_text(&gbk_encode(&speaker.chr_name), &gbk_encode(line));
    let body = smframe::sm_hear(speaker.actor_id, colors.hear_f, colors.hear_b, &text);
    let mut delivered = 0usize;
    for target_id in chat::hear_targets(world, speaker.actor_id, chat::SEND_REF_MSG_RANGE) {
        let Some(t) = online.iter().find(|e| e.actor_id == target_id) else {
            continue;
        };
        let Some(tx) = route_to_gate(gates, t.gate_idx, t.socket, "说话") else {
            continue;
        };
        let _ = tx.send(SimToGate::SendFrame {
            socket: t.socket,
            body: body.clone(),
        });
        delivered += 1;
    }
    glog!(
        "INF",
        "说话 ChrName:{} ActorId:{} → {} 个收件人 tail={:?}",
        speaker.chr_name,
        speaker.actor_id,
        delivered,
        line
    );
}

/// `CM_PICKUP`(1001) / `CM_DROPITEM`(1000) 的派发（owner §26）。
///
/// 口径：纯判定在 `ground::pick_up`/`drop_item`（m3bag 交付，逐字段对 oracle 向量），
/// `SM_*` 编码/下行序在 `pickwire::{outcome_frames, build_ground_map}`；本函数只负责
/// 「找会话 → 用 world 只读视图装 `GroundMap` → 调用 → 按 `[GateIdx,Socket]` 回帧」。
///
/// **位置门在派发层**（`PlayObject.Message.cs:811-816`）：`CurrX==nParam2 && CurrY==nParam3`
/// 才进 `ClientPickUpItem`（`CM_PICKUP` 的 `nParam2/nParam3` = 12B 头的 `param/tag`）。
/// `CM_DROPITEM` 的尾 = 请求 `Msg`，`nParam1`（`param`）= 要丢的 `MakeIndex`。
///
/// **已知缺口（登记，不静默）**：拾取删格对 `crates/world` 的**写回**尚未开（owner §25.1 (A)
/// 只批只读分片）⇒ 地面物品在 world 里不消失（客户端已由 `SM_ITEMHIDE` 隐藏）；成功 drop 的
/// `Envir.AddItemToMap` 在本仓 oracle 下恒失败（`MapItem.ItemId == 0`，`ground.rs` 模块头 T-19）。
#[allow(clippy::too_many_arguments)]
fn handle_ground_cmd(
    ident: u16,
    param: u16,
    tag: u16,
    tail: Option<&str>,
    chr_name: &str,
    socket: i32,
    world: &mir2_world::World,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    cfg: &GroundCfg,
    colors: &MsgColors,
    catalog: &dyn game_svr::ability::ItemCatalog,
    registry: Option<&mir2_envir::MapRegistry>,
) {
    // 找会话（`online` 给出该角色当前的网关线号；`sessions` 给出地面交互态）。
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "CM_PICKUP/DROP：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃"
        );
        return;
    };
    let gate_idx = entry.gate_idx;
    let Some(sg) = sessions.get_mut(chr_name) else {
        glog!(
            "WRN",
            "CM_PICKUP/DROP：ChrName={chr_name} 无地面会话态（未进世界/已断线）⇒ 丢弃"
        );
        return;
    };
    let actor_id = sg.actor_id;
    let Some(actor) = world.entities.get(actor_id) else {
        glog!("WRN", "CM_PICKUP/DROP：actor_id={actor_id} 不在世界 ⇒ 丢弃");
        return;
    };
    let (cx, cy, map_id) = (actor.x, actor.y, actor.map_id);
    let Some(grid) = world.maps.get(map_id) else {
        glog!("WRN", "CM_PICKUP/DROP：map_id={map_id} 越界 ⇒ 丢弃");
        return;
    };
    // 地面视图 = world 的**只读分片**（`floor_items_view_sorted`，owner §25.1 (A)）。
    let view = world.floor_items_view_sorted();
    let mut map =
        game_svr::pickwire::build_ground_map(&view, map_id, grid.width, grid.height, catalog);
    let now_tick = world.clock_ms as i32;
    let flag = registry.and_then(|r| r.flag_of(map_id));
    // 先跑纯函数（会**原地改** `sg.state`），再按**改后**的会话态组 `WireSession`
    // ——oracle 的 `GoldChanged()`/`WeightChanged()` 都在 `Gold +=`/`AddItemToBag` **之后**发
    // （`PlayObject.Base.cs:79-84`），故 `SM_GOLDCHANGED`/`SM_WEIGHTCHANGED` 用新值。
    let (immediate, notices, drop_msg) = if ident == game_svr::ground::CM_PICKUP {
        // oracle 位置门：不满足 ⇒ 不进 `ClientPickUpItem`。
        if !game_svr::pickwire::pickup_position_ok(cx, cy, i32::from(param), i32::from(tag)) {
            glog!(
                "DBG",
                "CM_PICKUP：位置门不成立 ChrName={chr_name} Curr=({cx},{cy}) 请求=({param},{tag}) ⇒ 忽略"
            );
            return;
        }
        let ctx = game_svr::ground::PickUpContext {
            catalog,
            actor_id,
            curr_x: cx,
            curr_y: cy,
            now_tick,
            // `Dealing` 本接缝未建模（无交易系统）⇒ false（登记）。
            dealing: false,
            floor_item_can_pick_up_time: cfg.floor_item_can_pick_up_time,
            human_max_gold: cfg.human_max_gold,
            // `Race == ActorRace.Play`（`SendAddItem` 仅在玩家时发）⇒ 恒真（本宿主只收玩家）。
            race_is_play: true,
            // 组队系统未移植（`GroupOwner=0`）⇒ 群组分支恒 false（登记）。
            group_owner: 0,
            group_member_actor_ids: &[],
            red_msg_f_color: u16::from(colors.red_f),
            red_msg_b_color: u16::from(colors.red_b),
        };
        let out = game_svr::ground::pick_up(&ctx, &mut map, &mut sg.state);
        (out.immediate_frames, out.notices, Vec::new())
    } else {
        // `CM_DROPITEM`：`Msg` = `processMsg.Msg`（EDCode 解码后的 GBK 文本）；
        // `SendDefMessage(..., Msg)` 会 `EncodeString(Msg)` ⇒ 这里传**未编码的 GBK 字节**。
        let item_name = tail.unwrap_or("");
        let ctx = game_svr::ground::DropContext {
            catalog,
            actor_id,
            curr_x: cx,
            curr_y: cy,
            now_tick,
            // `DealLastTick` 未建模（无交易系统）⇒ 0（新会话即过 3000ms 门）。
            deal_last_tick: 0,
            make_index: i32::from(param),
            item_name,
            in_safe_disable_drop: cfg.in_safe_disable_drop,
            in_safe_zone: flag.as_ref().is_some_and(|f| f.safe_area),
            // `IsCanDrop` 在无 PK/摆摊等状态下恒真；未建模的状态（摆摊/交易）登记。
            is_can_drop: true,
            no_throw_item: flag.as_ref().is_some_and(|f| f.no_throw_item),
            control_drop_item: cfg.control_drop_item,
            can_drop_price: cfg.can_drop_price,
            drop_item_rage: cfg.drop_item_rage,
        };
        let out = game_svr::ground::drop_item(&ctx, &mut map, &mut sg.state);
        (out.immediate_frames, out.notices, enter::gbk(item_name))
    };
    let wire = game_svr::pickwire::WireSession {
        actor_id,
        gold: sg.state.gold,
        game_gold: sg.game_gold,
        wabil: sg.state.wabil.clone(),
        sys_msg: sg.sys_msg.clone(),
    };
    let bodies = game_svr::pickwire::outcome_frames(&immediate, &notices, &wire, &drop_msg);
    if let Some(tx) = route_to_gate(gates, gate_idx, socket, "拾取/丢弃回帧") {
        for body in bodies {
            let _ = tx.send(SimToGate::SendFrame { socket, body });
        }
    }
}

/// `CM_TAKEONITEM`(1003) / `CM_TAKEOFFITEM`(1004) 的派发（owner `f4a1b02f` 的 **N-1 调用点接线**）。
///
/// 口径：纯判定/状态迁移在 `bag::take_on`/`bag::take_off`（m3bag 交付，逐字段对 oracle
/// `bag_vectors.json`）；本函数只负责「找会话 → 装 `EquipState`/`TakeOnContext` → 调用 →
/// 重算 → 按 `[GateIdx,Socket]` 回帧」。
///
/// **T-18 调用点接线（本片重点）**：oracle 的成功分支会再调 `RecalcAbilitys()`
///（`PlayObject.Operate.cs:458`/`:547`），而 `RecalcAbilitys` **不重置**
/// `PoisonRecover/HealthRecover/SpellRecover`（`BaseObject.Abil.cs:30-31`）、道士的 `HitPoint`
/// 也不重置 ⇒ 重算必须传 **调用前的** `SubAbility`（`sg.sub`），不能用 `first_call()`，
/// 否则线上行为等价于「首次调用」。
///
/// **收包字段口径**（`PlayObject.Message.cs:820-824`）：`ClientTakeOnItems((byte)processMsg.nParam2,
/// processMsg.nParam1, processMsg.Msg)`（take-off 同形），而 12B 头里 `Recog`(b0-3) = `nParam1`、
/// `Param`(b6-7) = `nParam2`、`Tag`(b8-9) = `nParam3`（`WorldServer.cs:820` 的
/// `SendMsg(..., Recog, Param, Tag, ...)` + `BaseObject.Message.cs:98` 的形参序）⇒
/// **`make_index` = `Recog`、`slot` = `Param` 的低 8 位**；`Tag` 未用（真客户端恒 0）。
/// 佐证：冻结客户端 `ClMain.pas:7049` 的 `MakeDefaultMsg(CM_TAKEONITEM, itmindex, where, 0, 0)`。
///
/// **oracle 出帧顺序**：换下的旧物品 `SendAddItem`（`SM_ADDITEM`，`Operate.cs:455`/`:539`）→
/// `SM_TAKEON_OK/FAIL`（`SM_TAKEOFF_OK/FAIL`）→ 成功重算后的 `SM_ABILITY/SM_SUBABILITY`
/// （oracle 走普通优先级队列，下一轮 flush；本宿主在同一批直接下发，顺序一致）。
/// 成功换装还会 `FeatureChanged()` ⇒ 向同图周围玩家（含自己）广播 `SM_FEATURECHANGED(41)`。
#[allow(clippy::too_many_arguments)]
fn handle_bag_cmd(
    ident: u16,
    recog: i32,
    param: u16,
    tag: u16,
    tail: Option<&str>,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    catalog: &dyn game_svr::ability::ItemCatalog,
    world: &mir2_world::World,
    displays: &mut downlink::DisplayTable,
    ref_msg_range: i32,
) {
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "CM_TAKEON/TAKEOFF：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃"
        );
        return;
    };
    let gate_idx = entry.gate_idx;
    let Some(sg) = sessions.get_mut(chr_name) else {
        glog!(
            "WRN",
            "CM_TAKEON/TAKEOFF：ChrName={chr_name} 无会话态（未进世界/已断线）⇒ 丢弃"
        );
        return;
    };
    // C-1（reviewer-3 `5df38bad`）：`slot` 从 **`Param`**（b6-7）取，按 oracle 的 `(byte)` 低 8 位截断；
    // `make_index` 从 **`Recog`**（b0-3）取；`Tag`（b8-9）未用。
    let slot = (param & 0x00FF) as u8;
    let make_index = recog;
    let _ = tag; // oracle `nParam3` 未参与 `ClientTakeOnItems/ClientTakeOffItems`
    let item_name = tail.unwrap_or("");

    let mut state = game_svr::bag::EquipState {
        use_items: sg.use_items.clone(),
        item_list: sg.state.item_list.clone(),
    };
    // 本骨架未建模的 `TakeOnContext` 输入（无行会/城堡/交易/坐骑/自定义药品）⇒ 常量；
    // `wabil` = **调用前**的 `WAbil`（`CheckTakeOnItems` 的腕力/负重门）。
    let ctx = game_svr::bag::TakeOnContext {
        catalog,
        job: sg.equip.job,
        gender: sg.equip.gender,
        hair: sg.equip.hair,
        abil_level: sg.equip.abil.level,
        re_level: sg.equip.re_level,
        credit_point: sg.equip.credit_point,
        member_type: 0,
        member_level: 0,
        has_guild: false,
        guild_rank_no: 0,
        is_castle_member: false,
        user_unlock_durg: false,
        dealing: false,
        disable_take_off_keys: &[],
        on_horse: false,
        horse_type: 0,
        dress_eff_type: 0,
        wabil: &sg.state.wabil,
    };
    let out = if ident == game_svr::bag::CM_TAKEONITEM {
        game_svr::bag::take_on(&mut state, &ctx, slot, make_index, item_name)
    } else {
        game_svr::bag::take_off(&mut state, &ctx, slot, make_index, item_name)
    };
    // 状态迁移写回会话（`bag::*` 只在成功分支改集合；失败分支原样返回）。
    sg.use_items = state.use_items;
    sg.state.item_list = state.item_list;

    if out.recalc {
        let bag: Vec<u16> = sg.state.item_list.iter().map(|i| i.index).collect();
        let mut use_inst: Vec<Option<game_svr::ability::ItemInstance>> =
            vec![None; game_svr::ability::USE_ITEMS_LEN];
        for (i, cell) in sg.use_items.iter().enumerate() {
            if let Some(it) = cell {
                use_inst[i] = Some(game_svr::ability::ItemInstance {
                    index: it.index,
                    dura: it.dura,
                });
            }
        }
        let inputs = game_svr::ability::RecalcInputs {
            job: sg.equip.job,
            abil: &sg.equip.abil,
            bonus_abil: &sg.equip.bonus_abil,
            bonus_tick_hit: sg.equip.bonus_tick_hit,
            bonus_tick_speed: sg.equip.bonus_tick_speed,
            cur_hp: sg.state.wabil.hp,
            cur_mp: sg.state.wabil.mp,
            use_items: &use_inst,
            bag: &bag,
        };
        // T-18：传**调用前的** `SubAbility`（`sg.sub`），**不是** `SubAbility::first_call()`。
        let recalc = game_svr::ability::recalc_abilitys_with_prior(catalog, &inputs, &sg.sub);
        sg.state.wabil = recalc.wabil;
        sg.sub = recalc.sub;
    }

    let Some(tx) = route_to_gate(gates, gate_idx, socket, "装备/卸下回帧") else {
        return;
    };
    // oracle：换下的旧物品 `SendAddItem` 在 OK/FAIL 帧之前（`Operate.cs:455`/`:539`）。
    if out.swapped_out.is_some() {
        let _ = tx.send(SimToGate::SendFrame {
            socket,
            body: game_svr::smframe::sm_additem(sg.actor_id),
        });
    }
    for f in &out.frames {
        let body = game_svr::smframe::gm_data_body(
            game_svr::smframe::command_header(f.recog, f.ident, f.param, f.tag, f.series),
            &[],
        );
        let _ = tx.send(SimToGate::SendFrame { socket, body });
    }
    // 成功重算后的 52 `SM_ABILITY` / 752 `SM_SUBABILITY`
    // （oracle `SendMsg(RM_ABILITY/RM_SUBABILITY)`，`Operate.cs:459-460`/`:548-549`）。
    if out.recalc {
        let _ = tx.send(SimToGate::SendFrame {
            socket,
            body: game_svr::smframe::sm_ability(
                sg.state.gold,
                sg.equip.job,
                sg.game_gold,
                &record::encode_ability(&sg.state.wabil),
            ),
        });
        let sb = sg.sub.to_frame_bytes();
        let _ = tx.send(SimToGate::SendFrame {
            socket,
            body: game_svr::smframe::sm_subability(sb[0], sb[1], sb[2], sb[3], sb[4], sb[5], sb[6]),
        });
    }
    // oracle `FeatureChanged()`（`PlayObject.Operate.cs:459`/`:548` ⇒ `BaseObject.cs:1509`
    // → `SendRefMsg(RM_FEATURECHANGED, GetFeatureEx(), GetFeatureToLong(), 0,0)`）：
    // 成功换装后向**周围 `SendRefMsgRange` 格内同图**的玩家（**含自己**）广播 `SM_FEATURECHANGED`。
    // `RM_FEATURECHANGED=10116` 的队列优先级 `(byte)10116=68` 排在 `RM_ABILITY(19)`/`RM_SUBABILITY(62)`
    // 之后 ⇒ 线上顺序 = `SM_ADDITEM → OK/FAIL → 52 → 752 → 41`（与 oracle 消息泵一致）。
    if out.recalc {
        let use_inst: Vec<Option<game_svr::ability::ItemInstance>> = sg
            .use_items
            .iter()
            .map(|c| {
                c.as_ref().map(|it| game_svr::ability::ItemInstance {
                    index: it.index,
                    dura: it.dura,
                })
            })
            .collect();
        let feature =
            game_svr::ability::equipped_feature(catalog, &use_inst, sg.equip.gender, sg.equip.hair);
        let light_pairs: Vec<(u16, u16)> = sg
            .use_items
            .iter()
            .take(downlink::USE_ITEM_SLOTS)
            .map(|c| c.as_ref().map_or((0, 0), |it| (it.index, it.dura)))
            .collect();
        let light = downlink::my_light(sg.equip.abil.level, &light_pairs, catalog);
        if let Some(mut d) = displays.get(sg.actor_id).cloned() {
            d.feature = feature;
            d.light = light;
            displays.set(sg.actor_id, d);
        }
        broadcast_feature_changed(
            world,
            online,
            gates,
            sg.actor_id,
            feature,
            // `GetFeatureEx()` = `MakeWord(OnHorse?HorseType:0, DressEffType)`；本宿主两者未建模 ⇒ 0。
            game_svr::feature::feature_ex(false, 0, 0),
            ref_msg_range,
        );
    }
}

/// oracle `PlayObject.cs:1205-1245` 的 `IsNewHuman` 四件新手物品（**固定顺序**：烛烛→金创药(小量)→木剑→布衣(男/女)）。
///
/// 每件照 `GameItemSystem.CopyToUserItemFromName`（`GameItemSystem.cs:113-140`）：按名查模板（`OrdinalIgnoreCase`），
/// `Index = 表内位置+1`、`Dura = DuraMax = tpl.DuraMax`；模板缺失 ⇒ 不入包（oracle `else Dispose(userItem)`）。
/// `MakeIndex` = `M2Share.GetItemNumber()`（`M2Share.cs:547`）——本宿主用单调计数器（`GetTickCount` 项登记）。
/// 返回模板缺失的告警（调用方记 WRN）。
fn add_newbie_items(
    item_list: &mut Vec<game_svr::bag::BagItem>,
    catalog: &game_svr::ability::StdItemCatalog,
    newbie: &NewbieItems,
    gender: u8,
    item_number: &mut i32,
) -> Vec<String> {
    let cloth = if gender == 0 {
        &newbie.cloths_man
    } else {
        &newbie.cloths_woman
    };
    let names = [
        &newbie.candle,
        &newbie.basic_drug,
        &newbie.wooden_sword,
        cloth,
    ];
    let mut warns = Vec::new();
    for name in names {
        match catalog.find_by_name(name) {
            Some(index) => {
                if let Some(tpl) = game_svr::ability::ItemCatalog::get(catalog, index) {
                    *item_number += 1;
                    item_list.push(game_svr::bag::BagItem {
                        make_index: *item_number,
                        index,
                        dura: tpl.dura_max,
                        dura_max: tpl.dura_max,
                        desc: [0; 14],
                    });
                }
            }
            None => warns.push(format!("新手物品模板缺失（Name={name}）")),
        }
    }
    warns
}

/// `CM_QUERYBAGITEMS(81)` 的派发（owner 追加的卡项 ②）。
///
/// oracle 链：`PlayObject.Message.cs:784-795` 的 30s 限频 → `ClientQueryBagItems()`
/// （`PlayObject.Operate.cs:37-64`）：逐件 `GetStdItem != null` → `ClientItem` → `EncodeBuffer + '/'`；
/// 非空则 `MakeMessage(SM_BAGITEMS, ActorId, 0, 0, ItemList.Count)` + 尾。
/// 本函数只负责「找会话 → 30s 门 → 装 `ClientItem` 视图 → `itemwire` 编码 → 按 `[GateIdx,Socket]` 回帧」。
///
/// **登记**：限频命中时的 `SysMsg(MessageSettings.QUERYBAGITEMS, Red, Hint)` 文本未建模（`String.conf` 无该键）。
#[allow(clippy::too_many_arguments)]
fn handle_query_bag_cmd(
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    catalog: &game_svr::ability::StdItemCatalog,
    world: &mir2_world::World,
) {
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "CM_QUERYBAGITEMS：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃"
        );
        return;
    };
    let gate_idx = entry.gate_idx;
    let Some(sg) = sessions.get_mut(chr_name) else {
        glog!(
            "WRN",
            "CM_QUERYBAGITEMS：ChrName={chr_name} 无会话态（未进世界/已断线）⇒ 丢弃"
        );
        return;
    };
    // `PlayObject.Message.cs:786`：`(GetTickCount() - QueryBagItemsTick) > 30*1000` 才回；否则 `SysMsg`。
    let now = world.clock_ms;
    if let Some(t) = sg.query_bag_items_tick {
        if now - t <= 30_000 {
            glog!(
                "WRN",
                "CM_QUERYBAGITEMS：30s 限频内重复查询 ChrName={chr_name} ⇒ 照 oracle 回 SysMsg（文案未建模，登记）"
            );
            return;
        }
    }
    sg.query_bag_items_tick = Some(now);
    let actor_id = sg.actor_id;
    glog!(
        "INF",
        "CM_QUERYBAGITEMS：ChrName={chr_name} ItemList={} 件",
        sg.state.item_list.len()
    );
    // `ClientQueryBagItems`（`Operate.cs:39-57`）：逐件 `GetStdItem(Index) != null` 才编。
    let mut views: Vec<game_svr::itemwire::ClientItem<'_>> = Vec::new();
    for it in &sg.state.item_list {
        let Some(tpl) = game_svr::ability::ItemCatalog::get(catalog, it.index) else {
            continue;
        };
        // `CustomItemSystem.GetItemName`（默认 = 模板名；`Desc[13]==1` 的自定义名未建模，登记）。
        let mut name = tpl.name.clone();
        // `item.StdMode == 50` 的名字后缀（`Operate.cs:52-55`）。
        if tpl.std_mode == 50 {
            name = format!("{name} #{}", it.dura);
        }
        views.push(game_svr::itemwire::ClientItem {
            tpl,
            name,
            make_index: it.make_index,
            dura: it.dura,
            dura_max: it.dura_max,
            desc: it.desc,
        });
    }
    let tail = game_svr::itemwire::bagitems_tail(&views);
    if tail.is_empty() {
        return;
    }
    let body = game_svr::itemwire::sm_bagitems(actor_id, views.len(), &tail);
    glog!(
        "INF",
        "SM_BAGITEMS 下发：ChrName={chr_name} socket={socket} gate={gate_idx} {} 件（{} 字节）",
        views.len(),
        body.len()
    );
    if let Some(tx) = route_to_gate(gates, gate_idx, socket, "查询背包回帧") {
        let _ = tx.send(SimToGate::SendFrame { socket, body });
    } else {
        glog!("WRN", "SM_BAGITEMS：找不到网关线号 {gate_idx}，丢弃");
    }
}

/// `CM_EAT`(1006) 的派发（issue `8d07694d` 的 ②）。
///
/// oracle 链：`PlayObject.Message.cs:826` 的 `case CM_EAT: ClientUseItems(processMsg.nParam1, Msg)`
/// ⇒ 纯函数在 `useitem::use_item`（m3bag 交付，逐字段对 oracle `eat_vectors.json`）；本函数只负责
/// 「找会话 → 装 `UseItemContext`/`UseItemActorState` → 调用 → 按 `[GateIdx,Socket]` 回帧」。
///
/// **收包字段口径**（`WorldServer.cs:820` + `BaseObject.Message.cs:98`）：12B 头的 `Recog`(b0-3) = `nParam1`
/// ⇒ **`make_index` = `recog`**（`Param`/`Tag` 未参与 `ClientUseItems`；`Msg` = 物品名，oracle 未用）。
///
/// **两路下行**（口径见 `useitem` 模块头）：
/// - **立即帧** `SendDefMessage(SM_EAT_OK/FAIL)` ⇒ header-only，直接下发（恒 1 条）；
/// - **延时通知** `SendMsg(RM_*)` 入 `MsgQueue`，由消息泵翻成 `SM_*`（`PlayObject.Message.cs:1736-1960`）；
///   本宿主在同一批按 `QueuedNotice` 出队序下发（`useitem::use_item` 已按 `(byte)wIdent` 排序）。
#[allow(clippy::too_many_arguments)]
fn handle_eat_cmd(
    recog: i32,
    _param: u16,
    _tag: u16,
    tail: Option<&str>,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    catalog: &dyn game_svr::ability::ItemCatalog,
    world: &mir2_world::World,
    colors: &MsgColors,
    registry: Option<&mir2_envir::MapRegistry>,
) {
    // `ClientUseItems(int nItemIdx, string sItemName)` 的 `sItemName` 在 oracle 里未被使用。
    let _ = tail;
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "CM_EAT：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃"
        );
        return;
    };
    let gate_idx = entry.gate_idx;
    let Some(sg) = sessions.get_mut(chr_name) else {
        glog!(
            "WRN",
            "CM_EAT：ChrName={chr_name} 无会话态（未进世界/已断线）⇒ 丢弃"
        );
        return;
    };
    let actor_id = sg.actor_id;
    // `EatItems` 的首个守卫（`PlayObject.Base.cs:2048`）：`Envir.Flag.boNODRUG`。
    let no_drug = world
        .entities
        .get(actor_id)
        .and_then(|a| registry.and_then(|r| r.flag_of(a.map_id)))
        .is_some_and(|f| f.bo_nodrug);
    let mut state = game_svr::useitem::UseItemActorState {
        wabil: sg.state.wabil.clone(),
        inc_health: sg.recovery.inc_health,
        inc_spell: sg.recovery.inc_spell,
        user_unlock_durg: sg.user_unlock_durg,
        hunger_status: sg.hunger_status,
    };
    // `ItemList` 从会话态拿出（`use_item` 原地改）后马上放回。
    let mut item_list = std::mem::take(&mut sg.state.item_list);
    // `RefMyStatus()` ⇒ `RecalcAbilitys()` 的观测输入（StdMode 1 跨档才用到；与 `handle_bag_cmd` 同源）。
    let bag: Vec<u16> = item_list.iter().map(|i| i.index).collect();
    let mut use_inst: Vec<Option<game_svr::ability::ItemInstance>> =
        vec![None; game_svr::ability::USE_ITEMS_LEN];
    for (i, cell) in sg.use_items.iter().enumerate() {
        if let Some(it) = cell {
            use_inst[i] = Some(game_svr::ability::ItemInstance {
                index: it.index,
                dura: it.dura,
            });
        }
    }
    let recalc = game_svr::ability::RecalcInputs {
        job: sg.equip.job,
        abil: &sg.equip.abil,
        bonus_abil: &sg.equip.bonus_abil,
        bonus_tick_hit: sg.equip.bonus_tick_hit,
        bonus_tick_speed: sg.equip.bonus_tick_speed,
        cur_hp: sg.state.wabil.hp,
        cur_mp: sg.state.wabil.mp,
        use_items: &use_inst,
        bag: &bag,
    };
    let ctx = game_svr::useitem::UseItemContext {
        catalog,
        // `BoCanUseItem` 在 oracle 初生即 true（`PlayObject.cs:961`）；运行期置位未建模 ⇒ 恒 true（登记）。
        can_use_item: true,
        // `Death` 未建模（在线即活）⇒ false（登记）。
        dead: false,
        actor_id,
        // `ClientUseItems(processMsg.nParam1, ...)`：`make_index` 取自 12B 头 `Recog`。
        make_index: recog,
        no_drug,
        // `SystemShare.FunctionNPC` 未建模（StdMode 31 分支）⇒ false（登记）；
        // `StdMode 31` 的 `AniCount == 0`（解包）也未接（T-17）。
        function_npc_present: false,
        red_msg_fcolor: u16::from(colors.red_f),
        red_msg_bcolor: u16::from(colors.red_b),
        recalc: Some(recalc),
    };
    let out = game_svr::useitem::use_item(&ctx, &mut state, &mut item_list);
    glog!(
        "INF",
        "CM_EAT 结果：ChrName={chr_name} make_index={recog} eaten={} immediate={} notices={}",
        out.eaten,
        out.immediate_frames.len(),
        out.notices.len()
    );
    // 状态迁移写回会话（`use_item` 只在成功分支改集合；失败分支原样返回）。
    sg.state.item_list = item_list;
    sg.state.wabil = state.wabil;
    sg.recovery.inc_health = state.inc_health;
    sg.recovery.inc_spell = state.inc_spell;
    sg.user_unlock_durg = state.user_unlock_durg;
    sg.hunger_status = state.hunger_status;

    let Some(tx) = route_to_gate(gates, gate_idx, socket, "用药回帧") else {
        return;
    };
    // 立即帧（`SendDefMessage`）：`SM_EAT_OK(635)`/`SM_EAT_FAIL(636)`，header-only。
    for f in &out.immediate_frames {
        let body = game_svr::smframe::gm_data_body(
            game_svr::smframe::command_header(f.recog, f.ident, f.param, f.tag, f.series),
            &[],
        );
        let _ = tx.send(SimToGate::SendFrame { socket, body });
    }
    // 延时通知（`SendMsg(RM_*)` ⇒ `SM_*`）。
    let wire = game_svr::pickwire::WireSession {
        actor_id,
        gold: sg.state.gold,
        game_gold: sg.game_gold,
        wabil: sg.state.wabil.clone(),
        sys_msg: None,
    };
    let mut sent = out.immediate_frames.len();
    for n in &out.notices {
        match game_svr::pickwire::notice_frame(n, &wire) {
            Some(body) => {
                let _ = tx.send(SimToGate::SendFrame { socket, body });
                sent += 1;
            }
            // `RM_CHARSTATUSCHANGED(10139)`/`RM_MYSTATUS(8102)`（StdMode 1 食物）需要
            // `HitSpeed`/`GetMyStatus()` 的运行期值，本宿主未建模 ⇒ 不编造（登记，不静默）。
            None => glog!(
                "WRN",
                "CM_EAT：通知 ident={} 未接 SM_* 映射（登记：StdMode 1 的 RM_CHARSTATUSCHANGED/RM_MYSTATUS）",
                n.ident
            ),
        }
    }
    glog!(
        "INF",
        "CM_EAT 已下发 {} 帧（socket={socket} gate={gate_idx}）",
        sent
    );
}

/// `FeatureChanged()`（oracle `BaseObject.cs:1509` → `SendRefMsg(RM_FEATURECHANGED, …)`）的下行：
/// 向**同图、周围 `range` 格（含自身格）内**的所有在线玩家各发一条 `SM_FEATURECHANGED`
/// （`PlayObject.Message.cs:1762-1764`：`recog = processMsg.ActorId`、`param/tag = LoWord/HiWord(feature)`、
/// `series = GetFeatureEx()`）。
///
/// **登记（未建模，不静默）**：oracle `SendRefMsg` 的 `VisibleHumanList` 有 **500ms 重建缓存**
/// （`BaseObject.Message.cs:395-397/455`）与 `ObMode/FixedHideMode` 的“只发自己”分支；
/// 非玩家目标的 `WantRefMsg` 名单也不在
/// 本实现内（本宿主只承载玩家）。这两项对所有 `RM_*` 广播同源，随具体 `SM_*` 再接。
fn broadcast_feature_changed(
    world: &mir2_world::World,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    actor_id: i32,
    feature: i32,
    feature_ex: u16,
    range: i32,
) {
    let Some(src) = world.entities.get(actor_id) else {
        glog!(
            "WRN",
            "FeatureChanged：actor_id={actor_id} 不在世界 ⇒ 不下发"
        );
        return;
    };
    let (sx, sy, smap) = (src.x, src.y, src.map_id);
    let body = game_svr::smframe::sm_featurechanged(actor_id, feature, feature_ex);
    for e in online {
        let Some(t) = world.entities.get(e.actor_id) else {
            continue;
        };
        if t.map_id != smap || (t.x - sx).abs() > range || (t.y - sy).abs() > range {
            continue;
        }
        if let Some(tx) = route_to_gate(gates, e.gate_idx, e.socket, "装备 FeatureChanged 广播")
        {
            let _ = tx.send(SimToGate::SendFrame {
                socket: e.socket,
                body: body.clone(),
            });
        }
    }
}

/// `record::ServerUserItem` → `bag::BagItem`（`CharacterData.bag_items` ⇒ `GroundState.item_list`）。
///
/// oracle `PlayObject.ItemList` 是**非空物品的列表**（`AddItemToBag` 追加 / 移除删除），记录里的
/// `Option<Vec<Option<ServerUserItem>>>` 是定长槽位 ⇒ 展平后与 `ItemList` 同形。`desc` 在 oracle
/// `UserItem` 是定长 14B（`bag::BagItem.desc: [u8;14]`），记录的 `Option<Vec<u8>>` 截断/补零
/// （只用于显示，不参与 `pick_up`/`drop_item` 判定）。
fn bag_items_from(items: Option<&[Option<record::ServerUserItem>]>) -> Vec<game_svr::bag::BagItem> {
    items
        .unwrap_or(&[])
        .iter()
        .flatten()
        // oracle `WorldServer.cs:1187-1189`：`if (bagItems[i].Index > 0) ItemList.Add(...)`
        // —— 记录里的定长槽位（新角色 = 46 个 `StdIndex=0` 空槽）**不进 `ItemList`**。
        .filter(|it| it.index > 0)
        .map(|it| {
            let mut desc = [0u8; 14];
            if let Some(d) = it.desc.as_deref() {
                let n = d.len().min(desc.len());
                desc[..n].copy_from_slice(&d[..n]);
            }
            game_svr::bag::BagItem {
                make_index: it.make_index,
                index: it.index,
                dura: it.dura,
                dura_max: it.dura_max,
                desc,
            }
        })
        .collect()
}

/// `record::ServerUserItem` → `bag::BagItem`（单个物品；`desc` 截断/补零到定长 14B）。
fn to_bag_item(it: &record::ServerUserItem) -> game_svr::bag::BagItem {
    let mut desc = [0u8; 14];
    if let Some(d) = it.desc.as_deref() {
        let n = d.len().min(desc.len());
        desc[..n].copy_from_slice(&d[..n]);
    }
    game_svr::bag::BagItem {
        make_index: it.make_index,
        index: it.index,
        dura: it.dura,
        dura_max: it.dura_max,
        desc,
    }
}

/// `record::CharacterData.hum_items` → `bag::EquipState.use_items`（**保位**：槽位下标 = `UseItems` 下标）。
///
/// 与 `bag_items_from`（背包 `ItemList`，展平）不同：`UseItems` 是定长 13 槽的数组，
/// `None` 必须留在原位（oracle `UseItems[btWhere]` 按槽位下标读写）。
fn use_items_from(
    items: Option<&[Option<record::ServerUserItem>]>,
) -> Vec<Option<game_svr::bag::BagItem>> {
    let mut out: Vec<Option<game_svr::bag::BagItem>> = vec![None; game_svr::ability::USE_ITEMS_LEN];
    for (i, cell) in items.unwrap_or(&[]).iter().enumerate() {
        if i >= out.len() {
            break;
        }
        out[i] = cell.as_ref().map(to_bag_item);
    }
    // 照 oracle `WorldServer.GetHumData`（`:1163-1176`）：记录的 `HumItems[Helmet]/[Necklace]`
    // 与 `UseItems` 的两槽是**交换**映射（`UseItems[Helmet] = humItems[Necklace]`、反之亦然）。
    // 存档侧 `MakeSaveRcd`（`:1302-1303`）再交换一次 ⇒ 记录↔UseItems 往返不变。
    out.swap(
        game_svr::ability::SLOT_NECKLACE,
        game_svr::ability::SLOT_HELMET,
    );
    out
}

/// 登入时留下的「原始记录 + 进图逻辑名」——存档（`WorldServer.SaveHumanRcd` → `MakeSaveRcd`）
/// 在它之上叠加会话实时增量（坐标/Gold/Abil/物品族表）。
struct SavedHuman {
    /// `playObject.UserAccount`（`MakeSaveRcd` 的 `SaveCharacterData.Account`）。
    account: String,
    /// 取档时的整条记录——未在 `SessionGround` 里建模的字段（magic/storage/quest/bonus…）原样保留。
    record: record::PlayerRecord,
    /// `playObject.MapName`（进图时 `resolve_enter_map` 定的逻辑名）。
    enter_map: String,
}

/// `HUtil32.DelfautItem.ToServerItem()`（`HUtil32.cs:14`）：空槽用的**非 null** 默认物品
/// （`UserItem.Desc = new byte[14]`，MakeIndex/Index/Dura/DuraMax = 0）。
fn default_server_item() -> record::ServerUserItem {
    record::ServerUserItem {
        make_index: 0,
        index: 0,
        dura: 0,
        dura_max: 0,
        desc: Some(vec![0u8; 14]),
        color_r: 0,
        color_g: 0,
        color_b: 0,
        prefix: None,
    }
}

/// `UserItem.ToServerItem()`（`ClientUserItem.cs:75-88`）：把本宿主的 `BagItem` 子集回写为记录字段。
/// `Desc` 存 14 字节（`BagItem.desc` 就是 14B），颜色/Prefix 取默认（oracle 新建物品同形）。
fn server_item_from_bag(it: &game_svr::bag::BagItem) -> record::ServerUserItem {
    record::ServerUserItem {
        make_index: it.make_index,
        index: it.index,
        dura: it.dura,
        dura_max: it.dura_max,
        desc: Some(it.desc.to_vec()),
        color_r: 0,
        color_g: 0,
        color_b: 0,
        prefix: None,
    }
}

/// `MakeSaveRcd` 的 `HumItems`（13 槽，`WorldServer.cs:1294-1311`）：每槽 = `UseItems[槽].ToServerItem()`
/// 或默认物品；**照 oracle 的 Slot3/Slot4（Necklace/Helmet）交换**（`:1302-1303`，与 `use_items_from`
/// 的 load 交换互逆）。
fn human_items_for_save(
    use_items: &[Option<game_svr::bag::BagItem>],
) -> Vec<Option<record::ServerUserItem>> {
    let mut out = vec![Some(default_server_item()); game_svr::ability::USE_ITEMS_LEN];
    for (i, slot) in out.iter_mut().enumerate() {
        let src = if i == game_svr::ability::SLOT_HELMET {
            game_svr::ability::SLOT_NECKLACE
        } else if i == game_svr::ability::SLOT_NECKLACE {
            game_svr::ability::SLOT_HELMET
        } else {
            i
        };
        *slot = Some(
            use_items
                .get(src)
                .and_then(Option::as_ref)
                .map_or_else(default_server_item, server_item_from_bag),
        );
    }
    out
}

/// `MakeSaveRcd` 的 `BagItems`（46 槽，`WorldServer.cs:1312-1329`）：`ItemList[i]` → 槽 i，
/// 其余槽 = 默认物品（**不是** `None`；DBSvr `ComparerUserItem` 要求槽位非 null）。
fn bag_items_for_save(item_list: &[game_svr::bag::BagItem]) -> Vec<Option<record::ServerUserItem>> {
    let mut out = vec![Some(default_server_item()); game_svr::bag::MAX_BAG_ITEM];
    for (i, it) in item_list
        .iter()
        .take(game_svr::bag::MAX_BAG_ITEM)
        .enumerate()
    {
        out[i] = Some(server_item_from_bag(it));
    }
    out
}

/// `WorldServer.cs:262-275` 的新角色 `Abil` 初始化（**逐字段照抄**）：
/// `Level=1`、`AC=MAC=0`、`DC=MC=SC=(ushort)MakeLong(1,2)=1`、`HP=MP=MaxHP=MaxMP=15`、
/// `Exp=0`、`MaxExp=100`、`Weight=0`、`MaxWeight=30`。
///
/// 随后的 `RecalcLevelAbilitys()/RecalcAbilitys()`（`PlayObject.cs:1308-1310`）会按 `Job` 覆盖
/// 等级派生字段（`MaxHP/MaxMP/MaxWeight/DC/MC/SC…`）——本宿主在 `enter::recalc_for_session` 里做。
fn new_human_ability() -> record::Ability {
    record::Ability {
        level: 1,
        reserved1: 0,
        ac: 0,
        mac: 0,
        // `(ushort)MakeLong(1, 2)`：`MakeLong=(hi<<16)|lo` ⇒ 低 16 位 = 1（oracle 原样）。
        dc: 1,
        mc: 1,
        sc: 1,
        hp: 15,
        mp: 15,
        max_hp: 15,
        max_mp: 15,
        reserved2: 0,
        exp_count: 0,
        exp_max_count: 0,
        exp: 0,
        max_exp: 100,
        weight: 0,
        max_weight: 30,
        wear_weight: 0,
        max_wear_weight: 0,
        hand_weight: 0,
        max_hand_weight: 0,
    }
}

/// `WorldServer.MakeSaveRcd`（`:1231-1360`）的 `CharacterData`：原始记录 + 会话实时增量。
///
/// 实时值取自 `SessionGround`（Gold/Abil/物品族表）与 `World` 的实体坐标；其余字段（magic/
/// storage/quest/BonusAbil/HomeMap 老角色…）保留记录原值。
/// `Abil` 照 `:1239-1275` 从**新建** `Ability` 开始只拷这批字段（`AC/MAC/DC/MC/SC` 留 0——oracle 同）；
/// `Exp/MaxExp` 取 `Abil`（`OnLogin` 里 `MaxExp = GetLevelExp(Level)`，`PlayObject.cs:1310`）。
fn build_save_record(
    saved: &SavedHuman,
    chr_name: &str,
    world: &mir2_world::World,
    sg: &SessionGround,
    registry: Option<&mir2_envir::MapRegistry>,
) -> record::PlayerRecord {
    let mut rec = saved.record.clone();
    // oracle `MakeSaveRcd`（`:1243`）写的是 `playObject.MapName`（**当前**逻辑图名，不是进图时的）。
    // 换图/传送接线前 `enter_map` == 当前图；这里优先用会话的 `map_id` 反查注册表
    //（`MapRegistry::name_of`），拿不到注册表/会话时回落到 `enter_map`。
    let cur_map = world
        .session(chr_name)
        .and_then(|s| registry.and_then(|r| r.name_of(s.map_id)))
        .unwrap_or(saved.enter_map.as_str());
    rec.data.cur_map = Some(cur_map.to_string());
    if let Some(e) = world
        .session(chr_name)
        .and_then(|s| s.actor_id)
        .and_then(|id| world.entities.get(id))
    {
        rec.data.cur_x = i16::try_from(e.x).unwrap_or(rec.data.cur_x);
        rec.data.cur_y = i16::try_from(e.y).unwrap_or(rec.data.cur_y);
    }
    rec.data.gold = sg.state.gold;
    rec.data.game_gold = sg.game_gold;
    rec.data.inc_health = u8::try_from(sg.recovery.inc_health & 0x00FF).unwrap_or(0);
    rec.data.inc_spell = u8::try_from(sg.recovery.inc_spell & 0x00FF).unwrap_or(0);
    rec.data.hunger_status = sg.hunger_status;
    let base = &sg.equip.abil;
    let w = &sg.state.wabil;
    rec.data.abil = Some(record::Ability {
        level: base.level,
        reserved1: 0,
        ac: 0,
        mac: 0,
        dc: 0,
        mc: 0,
        sc: 0,
        hp: w.hp,
        mp: w.mp,
        max_hp: w.max_hp,
        max_mp: w.max_mp,
        reserved2: 0,
        exp_count: 0,
        exp_max_count: 0,
        exp: base.exp,
        // `PlayObject.cs:1310`：登录后 `Abil.MaxExp = GetLevelExp(Abil.Level)`。
        max_exp: mir2_formula::exp::get_level_exp(
            &world.need_exps,
            mir2_data::models::MAX_LEVEL,
            i32::from(base.level),
        ),
        weight: w.weight,
        max_weight: w.max_weight,
        wear_weight: w.wear_weight,
        max_wear_weight: w.max_wear_weight,
        hand_weight: w.hand_weight,
        max_hand_weight: w.max_hand_weight,
    });
    rec.data.hum_items = Some(human_items_for_save(&sg.use_items));
    rec.data.bag_items = Some(bag_items_for_save(&sg.state.item_list));
    rec
}

/// 存档应答落地（oracle `PlayerDataService.cs:118-136` 的 `ProcessSaveQueue`）。
///
/// **仅** `DBR_SAVEHUMANRCD(1102) && recog==1` 才把角色移出保存名单（`RemoveSaveList` →
/// `FrnEngn.cs:156` 的 `InSaveRcdList`）；失败应答 `DBR_LOADHUMANRCD(1100) recog=0`
/// （`src/DBSrv/Services/Impl/DataService.cs:257-263`）只把该请求出队、**不**复位 in-save
/// ⇒ oracle 会把角色永久留在保存名单（之后每次进世界都判「正在保存」而被踢）。
/// 按红线「行为等价优先于修 bug：先 1:1 连 bug 一起搬」照抄该分支
/// （卡 `openmir2-m3-gamesvr-save` F-1 reviewer-5 `962fdf81`，owner 裁决 `99c89a88` 选项 a）。
fn apply_save_reply(engine: &mut load::FrontEngine, chr_name: &str, ident: u16, recog: i32) {
    if ident == mir2_protocol::messages::DBR_SAVEHUMANRCD && recog == 1 {
        engine.end_save(chr_name);
    }
}

/// 发 `DB_SAVEHUMANRCD(101)`（`WorldServer.SaveHumanRcd` `:1077` → `PlayerDataService.SavePlayerRcd`）。
///
/// **时机照 oracle**：在 `MakeGhost()`（退出世界）时调用（`RunNotice` 的 `BoSoftClose`/`BoKickFlag`
/// 支与 `CloseUser` 断线支都会走到），所以调用点必须在 `world.disconnect` **之前**；
/// **周期存档**（`SaveHumanRcdTime` 默认 10 分钟，`WorldServer.cs:698-703`）复用本函数
/// （见 [`drain_periodic_save`]，卡 `openmir2-m3-periodic-save`）。
fn save_human_rcd(
    db: &mut Option<DbWiring>,
    engine: &mut load::FrontEngine,
    saved: &SavedHuman,
    chr_name: &str,
    world: &mir2_world::World,
    sg: &SessionGround,
    registry: Option<&mir2_envir::MapRegistry>,
) -> Option<i32> {
    let w = db.as_mut()?;
    if !w.state.is_connected() {
        glog!(
            "WRN",
            "存档：DB 链路未连接 ⇒ 跳过 ChrName:{chr_name}（照 oracle IsConnected 支；未落库）"
        );
        return None;
    }
    let rec = build_save_record(saved, chr_name, world, sg, registry);
    let packet = record::encode_save_character_data(&saved.account, chr_name, &rec);
    let qid = w.next_query_id();
    let frame = dbclient::save_human_rcd_request(qid, &packet);
    match w.req_tx.send(frame) {
        Ok(()) => {
            // oracle `SaveHumanRcd` 先 `AddToSaveRcdList`（`WorldServer.cs:1091`）；进世界守卫读
            // `InSaveRcdList`（`:203`），命中则走「关新」支。应答回来时 `end_save`（见收侧轮询）。
            engine.begin_save(chr_name);
            w.save_outstanding.push((qid, chr_name.to_string()));
            glog!(
                "INF",
                "存档请求已发（DB_SAVEHUMANRCD）ChrName:{chr_name} queryId={qid} 记录 {} 字节",
                packet.len()
            );
            Some(qid)
        }
        Err(_) => {
            glog!(
                "WRN",
                "存档请求发送失败（DB 链路断开？）ChrName:{chr_name}（未落库）"
            );
            None
        }
    }
}

/// `CM_TURN`(3010) / `CM_WALK`(3011) / `CM_RUN`(3013) / `CM_HORSERUN`(3035) 的派发
/// （`openmir2-m3-move-wiring`）。
///
/// 口径（照 oracle，逐行见 `game_svr::movewire` 模块头）：
/// - 四者共用 `WorldServer.cs:831-838` 的入帧映射 ⇒ `nX/nY = LoWord/HiWord(Recog)`、`nDir = (byte)Tag`；
/// - `CM_TURN`：`ClientChangeDir`（`PlayObject.Operate.cs:310-348`）**只在 `nX == CurrX && nY == CurrY`**
///   时改朝向 + `Walk(RM_TURN)`，由调用方 `SendSocket(GetGoodTick)` 回 `+GD`；下行 `SM_TURN`
///   **名尾为空**（`SendRefMsg(..., "")` ⇒ `processMsg.Msg` 恒空）⇒ 用 `turn_pair_nameless`；
/// - `CM_WALK/RUN/HORSERUN`：`M2Share.GetNextDirection` 求方向后各走 1/2/3 格；移动帧广播不受
///   result 门影响，**但 `+GD` 只在 `movewire::move_result_ok`（oracle `SpaceMoved || CurrX==nX &&
///   CurrY==nY`，本仓库无 `SpaceMoved` ⇒ 退化为 `at_target`）为真时回**；
/// - **`Death` 守卫**：四支开头 oracle 都有 `if (Death || StatusTimeArr[PoisonState.STONE] != 0)
///   return false`（`PlayObject.Operate.cs:312` / `PlayObject.Attack.cs:1373/1297/980`）；
///   本片已接 `Entity.death`，**`STONE` 未移植（登记）**；
/// - `RM_TURN`/`RM_WALK`/`RM_RUN`/`RM_HORSERUN` 都经 `SendRefMsg`（±`SendRefMsgRange` 格内
///   `Race==Play && !Ghost`）广播，而收件人处理器排除自己（`processMsg.ActorId != ActorId`）
///   ⇒ **转折/移动帧只发给附近观察者**，自己那条连接只收 `+GD` ack。
///
/// 未接（登记，见 `movewire` 模块头）：限速/超速、失败支 `RM_MOVEFAIL`。
#[allow(clippy::too_many_arguments)]
fn handle_move_cmd(
    ident: u16,
    recog: i32,
    tag: u16,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    world: &mut mir2_world::World,
    displays: &mut downlink::DisplayTable,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
) {
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "移动命令：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let actor_id = entry.actor_id;
    let Some(e) = world.entities.get(actor_id) else {
        glog!(
            "WRN",
            "移动命令：actor_id={actor_id} 不在世界 ⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let (cur_x, cur_y, is_dead) = (e.x, e.y, e.death);
    if is_dead {
        // oracle `ClientChangeDir`/`ClientWalkXY`/`ClientRunXY`/`ClientHorseRunXY` 开头的
        // `if (Death || StatusTimeArr[PoisonState.STONE] != 0) return false`。`Entity.death` 已接；
        // `STONE`（麻痹状态位）未移植 ⇒ 登记（movewire 模块头）。拒绝支不回 `+GD`，
        // `delayTime == 0` 时 oracle 改发 `RM_MOVEFAIL`（未接，只记日志）。
        glog!(
            "DBG",
            "移动命令：ChrName={chr_name} 已死亡 ⇒ 拒绝（oracle Death 守卫；STONE 未接）ident={ident}"
        );
        return;
    }
    // `WorldServer.cs:831-838`：`nX = (short)LoWord(Recog)`、`nY = (short)HiWord(Recog)`、`wParam = Tag`。
    let nx = i32::from(game_svr::feature::lo_word(recog) as i16);
    let ny = i32::from(game_svr::feature::hi_word(recog) as i16);

    match ident {
        mir2_protocol::messages::CM_TURN => {
            let n_dir = u8::try_from(tag & 0x00FF).unwrap_or(0);
            if !movewire::turn_is_at_target(nx, ny, cur_x, cur_y) {
                // oracle `ClientChangeDir` 返回 false；`delayTime == 0` 时调用方发 `RM_MOVEFAIL`
                // （未接 ⇒ 只记日志，不静默）。
                glog!(
                    "DBG",
                    "CM_TURN：目标 ({nx},{ny}) != 当前位置 ({cur_x},{cur_y}) ⇒ 不转身（oracle 回 RM_MOVEFAIL，登记未接）ChrName={chr_name}"
                );
                return;
            }
            set_display_dir(displays, actor_id, n_dir);
            match displays.get(actor_id) {
                Some(d) => {
                    // `CM_TURN` 的 `Msg` 恒空 ⇒ 无名尾（见 `downlink::turn_pair_nameless`）。
                    let bodies = downlink::turn_pair_nameless(
                        actor_id,
                        wire_coord(cur_x),
                        wire_coord(cur_y),
                        d,
                    );
                    broadcast_ref_msg(world, online, gates, actor_id, &bodies);
                }
                None => glog!(
                    "WRN",
                    "CM_TURN：actor_id={actor_id} 无展示表 ⇒ 不发 SM_TURN（登记）"
                ),
            }
            send_good_tick(online, gates, chr_name);
        }
        mir2_protocol::messages::CM_WALK
        | mir2_protocol::messages::CM_RUN
        | mir2_protocol::messages::CM_HORSERUN => {
            let dir = movewire::next_direction(cur_x, cur_y, nx, ny);
            let (ddx, ddy) = movewire::direction_delta(dir);
            let steps = movewire::steps_for(ident);
            if !world.try_step_now(actor_id, ddx, ddy, steps) {
                glog!(
                    "DBG",
                    "移动：actor_id={actor_id} 方向={dir} {steps} 格被挡/越界 ⇒ 不动（oracle 回 RM_MOVEFAIL，登记未接）ChrName={chr_name}"
                );
                return;
            }
            set_display_dir(displays, actor_id, dir);
            // 体力/魔力点（`PlayObject.Attack.cs:1424` 走路 `HealthTick -= 10`；
            // `:1352-1356`/`:1041-1045` 跑步/骑跑 `HealthTick -= 60; SpellTick -= 10;
            // SpellTick = max(0, SpellTick); PerHealth -= 1; PerSpell -= 1`）。
            if let Some(sg) = sessions.get_mut(chr_name) {
                if ident == mir2_protocol::messages::CM_WALK {
                    // `ClientWalkXY` 只 `HealthTick -= 10`（无 Per* 扣减）。
                    sg.stamina.on_walk_success();
                } else {
                    sg.stamina.on_run_success();
                    // `ClientRunXY`/`ClientHorseRunXY` 另扣 `PerHealth/PerSpell` 各 1
                    // （`PerHealth`/`PerSpell` 是 `byte` ⇒ oracle 是无符号回绕减）。
                    sg.recovery.per_health = sg.recovery.per_health.wrapping_sub(1);
                    sg.recovery.per_spell = sg.recovery.per_spell.wrapping_sub(1);
                }
            }
            let (px, py) = world
                .entities
                .get(actor_id)
                .map_or((cur_x, cur_y), |e| (e.x, e.y));
            let sm_id = movewire::sm_ident_for(ident);
            match displays.get(actor_id) {
                Some(d) => {
                    match downlink::move_frame(sm_id, actor_id, wire_coord(px), wire_coord(py), d) {
                        Some(body) => broadcast_ref_msg(world, online, gates, actor_id, &[body]),
                        None => glog!("WRN", "移动：ident={sm_id} 无编码器 ⇒ 不发"),
                    }
                }
                None => glog!(
                    "WRN",
                    "移动：actor_id={actor_id} 无展示表 ⇒ 不发 {sm_id}（登记）"
                ),
            }
            // F-2：oracle `ClientWalkXY`/`RunTo`/`HorseRunTo` 的 result 门 = `SpaceMoved ||
            // (CurrX == nX && CurrY == nY)`；移动帧广播已在 `Walk()` 内发出（与本门无关），
            // **只有 result 为真才回 `+GD`**。本仓库无 `SpaceMoved` ⇒ 门退化 `at_target`（登记）。
            if movewire::move_result_ok(px, py, nx, ny) {
                send_good_tick(online, gates, chr_name);
            } else {
                glog!(
                    "DBG",
                    "移动：actor_id={actor_id} 方向={dir} {steps} 格后 ({px},{py}) != 目标 ({nx},{ny}) ⇒ 不回 +GD（oracle 回 RM_MOVEFAIL，登记未接）ChrName={chr_name}"
                );
            }
        }
        _ => {}
    }
}

/// M3②③ k=2 收口（真机攻方属性装载）：把 `RecalcAbilitys()` 产出的 `WAbil`/`SubAbility`
/// 写进 **世界实体**。
///
/// oracle 登录序 = `AbilCopyToWAbil()` → `RecalcLevelAbilitys()` → `RecalcAbilitys()`
/// （`WorldServer.cs:351` / `PlayObject.cs:1308`），此后 `_Attack` 读的就是这些字段：
/// - `Entity.dc`/`ac` ← `WAbil.DC`/`WAbil.AC`（`BaseObject.Attack.cs:25/27` 的
///   `GetBaseAttackPoewr`；`BaseObject.Base.cs:789` 的 `GetHitStruckDamage`）；
/// - `Entity.hit_point`/`speed_point` ← `SubAbility.HitPoint`/`SpeedPoint`
///   （`BaseObject.Attack.cs:41` 的命中门 `HitPoint < RandomByte(target.SpeedPoint)`）；
/// - `Entity.hp`/`max_hp` ← `WAbil.HP`/`MaxHP`；`Entity.level` ← `WAbil.Level`。
///
/// 此前这三组只喂了 52/752/50/41 帧，实体仍是 `Entity::new` 的空属性
/// （`dc/ac/hit_point/speed_point=0`、`hp/max_hp=100`）⇒ 攻方攻击力恒 0，且
/// **守方** `hit_point==0` 让命中门短路 ⇒ 真机 120 击全记躲闪（`attack_diff.py` 的 k=2 分岔根因）。
fn apply_ability_to_entity(
    e: &mut mir2_world::Entity,
    wabil: &game_svr::ability::Ability,
    sub: &game_svr::ability::SubAbility,
) {
    e.dc = i32::from(wabil.dc);
    e.ac = i32::from(wabil.ac);
    e.hit_point = sub.hit_point;
    e.speed_point = sub.speed_point;
    e.hp = wabil.hp;
    e.max_hp = wabil.max_hp;
    e.level = wabil.level;
}

/// `Envir.GetMovingObject(frontX, frontY, true)`（`Envirnoment.cs:1203-1234`）的**接线面最小等价物**：
/// 前方格里**首个** `!Ghost && !Death` 且不是自己的对象。
///
/// 与 oracle 的差异（登记）：oracle 走格子 `ObjList`（`GetCellInfo(...).IsAvailable` 门 + 该格
/// 插入次序），本接缝用**实体槽位序** + `map_id` 对齐代替（不建地图格子模型）。
///
/// 该接缝是 `openmir2-m3-attacker-attr-loading` 空打片（owner 2026-10-12 19:20）的**判据**：
/// 真机 84/120「前方格 (18,20) 无目标」是受害者在 k=36 HP 归零、`death=true` 后的**正常
/// oracle 行为**（不是取目标算错）；本函数把这一语义钉成可判红/红检的单测。
fn front_target(
    world: &mir2_world::World,
    actor_id: i32,
    map_id: usize,
    fx: i32,
    fy: i32,
) -> Option<i32> {
    world
        .entities
        .iter()
        .find(|t| {
            t.id != actor_id && t.map_id == map_id && t.x == fx && t.y == fy && !t.ghost && !t.death
        })
        .map(|t| t.id)
}

/// `openmir2-m3-attack-magic-wiring`（第一片）——`CM_HIT`/`CM_HEAVYHIT`/`CM_BIGHIT`/
/// `CM_POWERHIT`/`CM_LONGHIT`/`CM_WIDEHIT`/`CM_FIREHIT`/`CM_CRSHIT`/`CM_TWINHIT` 9 条攻击命令的派发。
///
/// 口径（照 oracle，逐行见 [`game_svr::attackwire`] 模块头）：
/// - 入帧映射与移动同段：`nX = LoWord(Recog)`、`nY = HiWord(Recog)`、`nDir = (byte)Tag`
///   （`WorldServer.cs:835-844`）；
/// - `Death` 守卫（`PlayObject.Attack.cs:835`；`STONE` 未移植登记）⇒ 拒绝支不回 `+GD`；
/// - **成功门** = `nX == CurrX && nY == CurrY`（`PlayObject.Attack.cs:884`）：不满足则不攻击、
///   不回 ack（oracle 在 `delayTime == 0` 时改发 `RM_MOVEFAIL`，编码器未接 ⇒ 只记日志）；
/// - 成功 ⇒ `Dir = nDir` + 取目标（`GetPoseCreate` = 前方格首个 `!Ghost && !Death` 对象）⇒
///   `world.attack_cmd`（下一 world tick 结算）+ 给附近观察者发动作帧（`SM_HIT` 族，排除自己）+ 回 `+GD`；
/// - **无目标支**（前方格无 `!Ghost && !Death` 目标）⇒ 仍走 oracle 的「抽攻击力」一击语义
///   （`world.attack_swing`），而不是什么都不抽。oracle 把 `GetBaseAttackPoewr()` 放在
///   **取目标之后、`_Attack` 之前**求值（`PlayObject.Attack.cs:73`），所以 `GetPoseCreate()==null`
///   时攻击力照抽；`_Attack(null)` / `AttackSuccess(…, null)` 才早退（无其它取数）。
///
/// **登记**（详见 `attackwire` 模块头）：限速/`LateDelivery`/`CheckActionStatus`、`STONE`、
/// `GetHitMode` 的技能态（无技能 ⇒ 模式 3/4/5/7/8 回落 `SM_HIT`）、目标筛选的安全区/攻击模式、
/// `GetMovingObject` 的格子 `ObjList` 次序（本片取实体槽位序）。
///
/// **返回值**：`true` = 本击**进入了攻击路径**（有目标 ⇒ `attack_cmd`，无目标 ⇒ `attack_swing`，
/// 两者都在 oracle 首抽之前）⇒ `sim_loop` 据此落一行服务端权威状态 dump
/// （`state_log::StateLog`，owner 2026-10-12 23:50 第 ③ 条 / §5.254，夹具 `run_attack_pair.ps1:1164`
/// 明写「归 m2-svr」）；被任一守卫拒绝的支返回 `false`（无攻击单元 ⇒ 无首抽 ⇒ 无 dump 行）。
#[allow(clippy::too_many_arguments)]
fn handle_hit_cmd(
    ident: u16,
    recog: i32,
    tag: u16,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    world: &mut mir2_world::World,
    displays: &mut downlink::DisplayTable,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
) -> bool {
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "攻击命令：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃 ident={ident}"
        );
        return false;
    };
    let actor_id = entry.actor_id;
    let Some(e) = world.entities.get(actor_id) else {
        glog!(
            "WRN",
            "攻击命令：actor_id={actor_id} 不在世界 ⇒ 丢弃 ident={ident}"
        );
        return false;
    };
    let (cur_x, cur_y, is_dead, map_id) = (e.x, e.y, e.death, e.map_id);
    if is_dead {
        // oracle `ClientHitXY` 开头 `if (Death || StatusTimeArr[PoisonState.STONE] != 0) return false;`
        // （`PlayObject.Attack.cs:835`）。`STONE` 未移植（登记）；拒绝支不回 `+GD`。
        glog!(
            "DBG",
            "攻击：ChrName={chr_name} 已死亡 ⇒ 拒绝（oracle Death 守卫；STONE 未接）ident={ident}"
        );
        return false;
    }
    // `WorldServer.cs:835-844`：`nX = LoWord(Recog)`、`nY = HiWord(Recog)`、`nDir = (byte)Tag`。
    let nx = i32::from(game_svr::feature::lo_word(recog) as i16);
    let ny = i32::from(game_svr::feature::hi_word(recog) as i16);
    if !game_svr::attackwire::at_self(nx, ny, cur_x, cur_y) {
        glog!(
            "DBG",
            "攻击：目标 ({nx},{ny}) != 当前位置 ({cur_x},{cur_y}) ⇒ 不攻击（oracle 回 RM_MOVEFAIL，登记未接）ChrName={chr_name} ident={ident}"
        );
        return false;
    }
    let Some(mode) = game_svr::attackwire::hit_mode(ident) else {
        return false;
    };
    let dir = u8::try_from(tag & 0x00FF).unwrap_or(0);
    // ⚠ 取目标用的是**状态化朝向**（对象当前 `Dir`），**不是**本帧消息 dir：oracle
    // `PlayObject.Attack.cs:26` 的 `attackTarget = targetObject ?? GetPoseCreate()` 发生在
    // `BaseObject.Attack.cs:12` 的 `Dir = nDir` **之前**；`nDir` 只影响此后的动作帧
    // （`PlayObject.Attack.cs:76` 的 `SendAttackMsg(..., Dir, ...)`）与后续朝向。
    // `Dir` 由 bin 侧 `DisplayTable` 承载（`crates/world` 的 `Entity` 无 `dir` 字段；
    // 进世界时用 `record.dir` 初始化，`set_display_dir` 维护）。展示表缺失时退回本帧 dir
    // （等价旧行为；生产路径进世界必登记展示表）。
    let pose_dir = displays.get(actor_id).map_or(dir, |d| d.dir);
    // 取目标：`GetPoseCreate()` = 前方格首个 `!Ghost && !Death` 的对象。
    let (width, height) = world
        .maps
        .get(map_id)
        .map_or((0, 0), |m| (m.width, m.height));
    let (fx, fy) = game_svr::attackwire::front_position(pose_dir, cur_x, cur_y, width, height);
    // `BaseObject.AttackDir` 开头 `Dir = nDir`（`BaseObject.Attack.cs:12`）——取目标**之后**才更新。
    set_display_dir(displays, actor_id, dir);
    let target_id = if (fx, fy) == (cur_x, cur_y) {
        None
    } else {
        front_target(world, actor_id, map_id, fx, fy)
    };
    if let Some(tid) = target_id {
        world.attack_cmd(actor_id, tid);
    } else {
        // oracle `PlayObject.AttackDir(targetObject = null)`（`PlayObject.Attack.cs:25`）：
        //   `attackTarget = targetObject ?? GetPoseCreate()`（`:26`，前方格无目标 ⇒ null）；
        //   随后 `GetAttackPowerHit(wHitMode, GetBaseAttackPoewr(), attackTarget, ref canHit)`（`:73`）
        //   ⇒ **攻击力照抽**（玩家 `Luck <= 0` ⇒ `Random(LoByte(DC)+1)` + `Random(10)` 共 2 抽，
        //   `PlayObject.cs:2929/2932`）；
        //   之后 `_Attack(null)`（`BaseObject.Attack.cs:32`）与 `AttackSuccess(…, null)`
        //   （`PlayObject.Attack.cs:215-218` 开头 `return`）都不再取数。
        // 接线到 m3cs 的 `World::attack_swing`（`crates/world/src/world.rs:199`，owner 20:10 裁决）：
        // 缺这一段 ⇒ 真机「有攻击命令、无目标」的 84 击取数整段缺失（C# 120 段 vs Rust 36 段）。
        let _ = world.attack_swing(actor_id);
        glog!(
            "DBG",
            "攻击：actor_id={actor_id} 前方格 ({fx},{fy}) 无目标 ⇒ 仍按 oracle 抽攻击力（attack_swing）+ 只播动作帧（`_Attack(null)` 返回 false）"
        );
    }
    // 成功支的体力扣减（`PlayObject.Attack.cs:955-959`）：`HealthTick -= 30; SpellTick -= 100;
    // SpellTick = max(0, SpellTick); PerHealth -= 2; PerSpell -= 2`（`PerHealth`/`PerSpell` 是
    // `byte` ⇒ oracle 是无符号回绕减）。
    if let Some(sg) = sessions.get_mut(chr_name) {
        sg.recovery.per_health = sg.recovery.per_health.wrapping_sub(2);
        sg.recovery.per_spell = sg.recovery.per_spell.wrapping_sub(2);
        sg.stamina.on_attack_success();
    }
    // 动作帧：`SendAttackMsg(GetHitMode, Dir, CurrX, CurrY)` → `SendRefMsg` ⇒ 附近观察者（排除自己）。
    if let Some(d) = displays.get(actor_id) {
        let sm = game_svr::attackwire::action_sm_ident(
            mode,
            game_svr::attackwire::HitSkillState::default(),
        );
        let body = game_svr::smframe::sm_action_hit(
            sm,
            actor_id,
            wire_coord(cur_x),
            wire_coord(cur_y),
            dir,
        );
        broadcast_ref_msg(world, online, gates, actor_id, &[body]);
        let _ = d;
    } else {
        glog!(
            "WRN",
            "攻击：actor_id={actor_id} 无展示表 ⇒ 不发动作帧（登记）"
        );
    }
    // `ClientHitXY` 成功 ⇒ 调用方 `SendSocket(GetGoodTick)` 回 `+GD` 短包。
    send_good_tick(online, gates, chr_name);
    // owner 2026-10-12 23:50 第 ③ 条 / §5.254：本击**进入了攻击路径**（抽攻击力）⇒ 调用方
    // （`sim_loop`）在返回后落一行服务端权威状态 dump（`state_log::StateLog`）；被守卫拒的支
    // 返回 `false` ⇒ 不落行（与 oracle「没有攻击单元就没有首抽」一致）。
    true
}

/// **`openmir2-m3-attack-magic-wiring`（第二片）**：`CM_SPELL(3017)` 派发
/// （oracle `WorldServer.cs:783-790` → `PlayObject.Message.cs:1339-1400`）。
///
/// 本片实现到 `ClientSpellXY` 的**守卫**与成功后 `+GD` ack；技能效果体（限速 /`switch(MagIdx)`/
/// `DoSpell`/`RM_SPELL`）未接，登记见 `magicwire` 模块头。拒绝支 oracle 在 `delayTime == 0`
/// 时 `SendRefMsg(RM_MOVEFAIL, …)`（编码器未接）⇒ 只记日志、不静默。
#[allow(clippy::too_many_arguments)]
fn handle_spell_cmd(
    recog: i32,
    tag: u16,
    param: u16,
    series: u16,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    world: &mir2_world::World,
    saves: &std::collections::HashMap<String, SavedHuman>,
    displays: &mut downlink::DisplayTable,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
) {
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "魔法命令：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃 CM_SPELL"
        );
        return;
    };
    let actor_id = entry.actor_id;
    let Some(e) = world.entities.get(actor_id) else {
        glog!(
            "WRN",
            "魔法命令：actor_id={actor_id} 不在世界 ⇒ 丢弃 CM_SPELL"
        );
        return;
    };
    let (cur_x, cur_y) = (e.x, e.y);
    let is_dead = e.death;
    let req = magicwire::parse_spell(recog, tag, param, series);
    // `GetMagicInfo(nKey)` 读角色记录的 `MagicList`（= `SavedHuman.record.data.magic`，进世界时
    // 保位存下的 `CharacterData.magic`）。
    let matched_mag_idx = saves
        .get(chr_name)
        .and_then(|s| s.record.data.magic.as_deref())
        .and_then(|list| magicwire::find_magic(list, req.magic_id))
        .map(|m| i32::from(m.mag_idx));
    let has_magic = matched_mag_idx.is_some();
    // `IsCanSpell`（`PlayObject.cs:960` 初值 true；`LockSpellAction` 状态面未移植 ⇒ 恒 true，登记）。
    if let Some(reject) = magicwire::spell_reject(true, is_dead, has_magic) {
        glog!(
            "DBG",
            "CM_SPELL：ChrName={chr_name} magic_id={} 被守卫拒绝 {reject:?}（oracle 回 RM_MOVEFAIL，编码器未接）",
            req.magic_id
        );
        return;
    }
    // `switch (UserMagic.MagIdx)` 的 `default` 支首句
    // `Dir = M2Share.GetNextDirection(CurrX, CurrY, nTargetX, nTargetY)`
    // （`PlayObject.Attack.cs:1269-1270`；原注误写 :1256，已按 reviewer-5 `1de8fca5` 的 F-1 订正）；
    // 效果体其余部分未接（登记）。**F-2 收口**（reviewer-5 `1de8fca5`）：该句只在 `default` 支执行，
    // `switch` 的 8 条独立 `case`（12/25/26/27/34/38/43/56）**不执行** ⇒ 只有非这 8 个魔号才改展示方向。
    if matched_mag_idx.is_some_and(magicwire::takes_default_dir) {
        let dir = movewire::next_direction(cur_x, cur_y, req.target_x, req.target_y);
        set_display_dir(displays, actor_id, dir);
    }
    // `ClientSpellXY` 全分支 `result = true` ⇒ 调用方 `ActionTick = GetTickCount(); SendSocket(GetGoodTick)`。
    send_good_tick(online, gates, chr_name);
}

/// **`openmir2-m3-attack-magic-wiring`（第二片）**：`CM_MAGICKEYCHANGE(1008)` 派发
/// （oracle `WorldServer.cs:857-859` → `PlayObject.Message.cs:871-873` → `PlayObject.Operate.cs:774-786`）。
///
/// 只改 `MagicList` 里命中项 `Key`，**无回帧**（oracle 不 `SendSocket`）。改的是角色记录
/// （`SavedHuman.record.data.magic`）⇒ 存档（`build_save_record` 克隆整条记录）会把新 `Key` 一起落库
/// （oracle `MakeSaveRcd` 的 `MagicList` 同源）。
fn handle_magic_key_cmd(
    recog: i32,
    param: u16,
    chr_name: &str,
    socket: i32,
    saves: &mut std::collections::HashMap<String, SavedHuman>,
) {
    let (skill_idx, key) = magicwire::parse_magic_key(recog, param);
    let Some(saved) = saves.get_mut(chr_name) else {
        glog!(
            "WRN",
            "CM_MAGICKEYCHANGE：ChrName={chr_name} socket={socket} 无角色记录 ⇒ 丢弃"
        );
        return;
    };
    let Some(list) = saved.record.data.magic.as_mut() else {
        glog!(
            "DBG",
            "CM_MAGICKEYCHANGE：ChrName={chr_name} magic_id={skill_idx} 记录无 MagicList ⇒ 不改键"
        );
        return;
    };
    if !magicwire::change_magic_key(list, skill_idx, key) {
        glog!(
            "DBG",
            "CM_MAGICKEYCHANGE：ChrName={chr_name} magic_id={skill_idx} 不在 MagicList ⇒ 不改键（oracle 同：无命中即无动作）"
        );
    }
}

/// oracle `SendRefMsg` 的收件人集合 = ±`SendRefMsgRange` 格内 `Race==Play && !Ghost`
/// （`BaseObject.Message.cs:383-440`，本仓库已有同构实现 `chat::hear_targets`）。
///
/// 各收件人的 `GetMessage` 对 `RM_TURN`/`RM_WALK`/`RM_RUN`/`RM_HORSERUN` 都带
/// `if (processMsg.ActorId != ActorId)` ⇒ **广播集合去掉自己**（自己那条连接只收 `+GD` ack）。
fn broadcast_ref_msg(
    world: &mir2_world::World,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    actor_id: i32,
    bodies: &[Vec<u8>],
) {
    for target_id in chat::hear_targets(world, actor_id, chat::SEND_REF_MSG_RANGE) {
        if target_id == actor_id {
            continue;
        }
        let Some(t) = online.iter().find(|e| e.actor_id == target_id) else {
            continue;
        };
        let Some(tx) = route_to_gate(gates, t.gate_idx, t.socket, "移动/转身下行") else {
            continue;
        };
        for body in bodies {
            let _ = tx.send(SimToGate::SendFrame {
                socket: t.socket,
                body: body.clone(),
            });
        }
    }
}

/// 帮 `ActorDisplay.dir` 随转向/走步更新（展示字段在 `bins/game-svr` 的展示表里，不进 `crates/world`）。
fn set_display_dir(displays: &mut downlink::DisplayTable, actor_id: i32, dir: u8) {
    if let Some(mut d) = displays.get(actor_id).cloned() {
        d.dir = dir;
        displays.set(actor_id, d);
    }
}

/// 坐标进帧的 u16 写法与 `drain_aoi_downlink` 一致（保留 `i16` 位模式）。
fn wire_coord(v: i32) -> u16 {
    u16::from_ne_bytes(i16::try_from(v).unwrap_or(0).to_ne_bytes())
}

/// `SendSocket(M2Share.GetGoodTick)`：给**动作发起者自己**回一条 `+GD/<tick>` 短包。
fn send_good_tick(
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    chr_name: &str,
) {
    let Some(me) = online.iter().find(|e| e.chr_name == chr_name) else {
        return;
    };
    let Some(tx) = route_to_gate(gates, me.gate_idx, me.socket, "回 +GD") else {
        return;
    };
    let _ = tx.send(SimToGate::SendRaw {
        socket: me.socket,
        payload: movewire::good_tick_payload(now_ms()),
    });
}

/// **S3 后半段（第一格）**：把世界 tick 的 AOI 结果（观察者实体 `VisibleActors` 里
/// `VisibleFlag::Show` 的对象）翻成 `SM_TURN` + `SM_FEATURECHANGED` 回帧，
/// 按该观察者的 `[GateIdx, Socket]` 路由回**它自己那条**网关线。
///
/// 顺序 = 会话表顺序（`SessionRegistry` 是 `Vec`），内层 = `VisibleActors` 列表顺序
///（oracle `PlayObject.Run` 的 `n18` 升序）⇒ 确定性。
///
/// 两处「不猜」：① 自身不入帧（oracle `if (baseObject != this)`，`PlayObject.cs:3238`）；
/// ② 展示表缺项（怪物/未登记）⇒ 跳过并记 WRN，不编造 `Dir/Light/Feature`。
fn drain_aoi_downlink(
    world: &mir2_world::World,
    displays: &downlink::DisplayTable,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
) {
    for sess in world.sessions.iter() {
        if sess.stage != mir2_world::WorldStage::InWorld {
            continue;
        }
        let Some(observer_id) = sess.actor_id else {
            continue;
        };
        let Some(observer) = world.entities.get(observer_id) else {
            continue;
        };
        let Some(entry) = online.iter().find(|e| e.chr_name == sess.chr_name) else {
            glog!(
                "WRN",
                "同屏下行：ChrName={} 已进世界但无在线网关坐标 ⇒ 不猜线、跳过",
                sess.chr_name
            );
            continue;
        };
        for visible in &observer.visible_actors {
            if visible.flag != mir2_world::VisibleFlag::Show || visible.target == observer_id {
                continue;
            }
            let Some(target) = world.entities.get(visible.target) else {
                continue;
            };
            let Some(display) = displays.get(visible.target) else {
                glog!(
                    "WRN",
                    "同屏下行：actor={} 无展示表（怪物/未登记）⇒ 跳过（登记）",
                    visible.target
                );
                continue;
            };
            let x = u16::from_ne_bytes(i16::try_from(target.x).unwrap_or(0).to_ne_bytes());
            let y = u16::from_ne_bytes(i16::try_from(target.y).unwrap_or(0).to_ne_bytes());
            let bodies = downlink::turn_pair(target.id, x, y, display);
            let Some(tx) = route_to_gate(gates, entry.gate_idx, entry.socket, "同屏下行")
            else {
                continue;
            };
            for body in bodies {
                let _ = tx.send(SimToGate::SendFrame {
                    socket: entry.socket,
                    body,
                });
            }
        }
    }
}

/// `openmir2-m3-attack-magic-wiring`（第一片）：本 world tick 的攻击结算 → 客户端下行。
///
/// 逐条口径：
/// - **受击**（`BaseObject.Attack.cs:61-64`）：`_Attack` 对 `Race > Play` 的目标
///   `SendMsg(target, RM_STRUCK, nPower, HP, MaxHP, ActorId)`，由目标自己的
///   `RM_STRUCK` 处理器（`PlayObject.Message.cs:1562-1628`）回 `SM_STRUCK`。只有玩家目标有连接
///   ⇒ 仅对在线玩家发帧；怪物（无连接）不发（登记）。
/// - **死亡**（`BaseObject.Base.cs:351`）：`Die()` 的 `SendRefMsg(RM_DEATH, Dir, CurrX, CurrY, 1, "")`
///   ⇒ 观察者（**含自己**，该 `case` 不像 `RM_HIT` 那样过滤 `ActorId`）收 `SM_NOWDEATH`。
///   怪物 death 帧的 `CharDesc`（`GetFeature`/`CharStatus`）未建模 ⇒ 跳过并登记。
/// - **经验**（`BaseObject.cs:2429 KillFunc` → `PlayObject.cs:1632 KillTargetTrigger` →
///   `PlayObject.Base.cs:149 GetExp`）：按 `TickRecord::exp_awards`（= `KillFunc` 的
///   `ExpHitter.Master`/`ExpHitter` 两支）给**获得者**发 `SM_WINEXP(44)`（`Recog=总量`、
///   `Param/Tag=本次增量的 Lo/Hi`），并把世界实体的经验/等级同步回会话（存档 `build_save_record`
///   读 `sg.equip.abil`）。
fn drain_combat_downlink(
    rec: &mir2_world::TickRecord,
    world: &mir2_world::World,
    displays: &downlink::DisplayTable,
    online: &OnlineRegistry,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
) {
    // ① 受击帧（发给受击者自己）。
    for (attacker, target, outcome) in &rec.attacks {
        if outcome.damage == 0 {
            continue;
        }
        let Some(entry) = online.iter().find(|e| e.actor_id == *target) else {
            continue;
        };
        let Some(d) = displays.get(*target) else {
            continue;
        };
        let max_hp = world
            .entities
            .get(*target)
            .map_or(outcome.target_hp_after, |e| e.max_hp);
        // 世界实体 HP 是本次结算后的权威值；同步回该会话的 `WAbil.HP`（供 52/752 与后续帧）。
        if let Some(sg) = sessions.get_mut(&entry.chr_name) {
            sg.state.wabil.hp = outcome.target_hp_after;
            // `RM_STRUCK` 处理器开头（`PlayObject.Message.cs:1587-1590`）：
            // `HealthTick = 0; SpellTick = 0; PerHealth -= 1; PerSpell -= 1`。
            // 受击清零 ⇒ 受害者永远到不了 `HealthFillTime`，与真机 dump 一致（受害方不回血）。
            sg.stamina.on_struck();
            sg.recovery.per_health = sg.recovery.per_health.wrapping_sub(1);
            sg.recovery.per_spell = sg.recovery.per_spell.wrapping_sub(1);
        }
        let body = game_svr::smframe::sm_struck(
            *target,
            outcome.target_hp_after,
            max_hp,
            outcome.damage,
            d.feature,
            d.status,
            *attacker,
            false,
        );
        if let Some(tx) = route_to_gate(gates, entry.gate_idx, entry.socket, "受击帧") {
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body,
            });
        }
    }
    // ② 死亡广播（含死者自己；展示表只登记玩家 ⇒ 怪物登记跳过）。
    for death in &rec.deaths {
        let dead_id = death.0;
        let Some(d) = displays.get(dead_id) else {
            glog!(
                "WRN",
                "死亡下行：actor={dead_id} 无展示表（怪物 feature/status 未建模）⇒ 跳过（登记）"
            );
            continue;
        };
        let (x, y) = world.entities.get(dead_id).map_or((0, 0), |e| (e.x, e.y));
        let body = game_svr::smframe::sm_death(
            mir2_protocol::messages::SM_NOWDEATH,
            dead_id,
            wire_coord(x),
            wire_coord(y),
            d.dir,
            &downlink::char_desc(d.feature, d.status),
        );
        for target_id in chat::hear_targets(world, dead_id, chat::SEND_REF_MSG_RANGE) {
            let Some(t) = online.iter().find(|e| e.actor_id == target_id) else {
                continue;
            };
            let Some(tx) = route_to_gate(gates, t.gate_idx, t.socket, "死亡下行") else {
                continue;
            };
            let _ = tx.send(SimToGate::SendFrame {
                socket: t.socket,
                body: body.clone(),
            });
        }
    }
    // ③ 经验帧（`GetExp` → `SendMsg(RM_WINEXP,0,dwExp,0,0)` → `SM_WINEXP`）。
    // 归属已由 world 按 `KillFunc` 算好（`ExpHitter.Master` 优先）；这里只做「找到在线会话 ⇒
    // 同步世界权威值 + 回帧」。未在线（怪物/NPC 获得者）跳过。
    for (winner, amount, _leveled, level) in &rec.exp_awards {
        let Some(entry) = online.iter().find(|e| e.actor_id == *winner) else {
            continue;
        };
        let Some(entity) = world.entities.get(*winner) else {
            continue;
        };
        let (total_exp, max_exp) = (entity.exp, entity.max_exp);
        // 世界实体是经验/等级的权威值：同步回会话，否则存档（`build_save_record` 读
        // `sg.equip.abil`）会把本 tick 获得的经验丢掉（`GetExp` 的 `Abil.Exp += dwExp`）。
        if let Some(sg) = sessions.get_mut(&entry.chr_name) {
            sg.equip.abil.exp = total_exp;
            sg.equip.abil.level = *level;
            sg.equip.abil.max_exp = max_exp;
            sg.state.wabil.exp = total_exp;
            sg.state.wabil.level = *level;
            sg.state.wabil.max_exp = max_exp;
        }
        let body = game_svr::smframe::sm_winexp(total_exp, *amount);
        if let Some(tx) = route_to_gate(gates, entry.gate_idx, entry.socket, "经验帧") {
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body,
            });
        }
    }
}

/// 每 tick 的**周期恢复**（oracle `PlayObject.Run` 的 `:70-141` 段，见 [`game_svr::recover`] 模块头）。
///
/// 逐会话（**按键排序**，不让 `HashMap` 迭代序参与逻辑）执行
/// [`game_svr::recover::recovery_tick`]；命中（`IncHealthSpell` 被调用）⇒ 照
/// `BaseObject.HealthSpellChanged()`（`BaseObject.cs:700-710`）：
///
/// - `Race==Play`（本宿主的会话全是玩家）⇒ `SendUpdateMsg(RM_HEALTHSPELLCHANGED,…)` = **给自己**；
/// - `ShowHp` ⇒ 再 `SendRefMsg(RM_HEALTHSPELLCHANGED,…)` = 同一图内 `SendRefMsgRange` 格的观察者
///   （含自己；`SendRefMsg` 的单元格扫描含源格）。`ShowHp` 由技能「心灵启示」置位（未移植）
///   ⇒ 生产路径不可达，登记。
///
/// **登记**：oracle `SendRefMsg` 有 500ms 的 `VisibleHumanList` 重建缓存（`BaseObject.Message.cs:395`），
/// 与 `broadcast_feature_changed` 同源地按“实时扫描”近似。
fn drain_recovery(
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    world: &mir2_world::World,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    range: i32,
) {
    let now = now_ms();
    let mut names: Vec<String> = sessions.keys().cloned().collect();
    names.sort();
    for name in names {
        let Some(sg) = sessions.get_mut(&name) else {
            continue;
        };
        let death = world.entities.get(sg.actor_id).is_some_and(|e| e.death);
        if !game_svr::recover::recovery_tick(&mut sg.recovery, &mut sg.state.wabil, death, now) {
            continue;
        }
        let Some(entry) = online.iter().find(|e| e.chr_name == name) else {
            glog!(
                "WRN",
                "周期恢复：ChrName={name} 无在线网关坐标 ⇒ 不回帧（登记）"
            );
            continue;
        };
        let body = game_svr::smframe::sm_healthspellchanged(
            sg.actor_id,
            sg.state.wabil.hp,
            sg.state.wabil.mp,
            sg.state.wabil.max_hp,
        );
        if let Some(tx) = route_to_gate(gates, entry.gate_idx, entry.socket, "周期恢复回帧") {
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body: body.clone(),
            });
        }
        // `ShowHp` 的 AOI 支（oracle `SendRefMsg`）：同图范围内观察者各收一份。
        if sg.show_hp {
            broadcast_healthspellchanged(world, online, gates, sg.actor_id, &body, range);
        }
    }
}

/// 每 tick 的**体力/魔力点段**（oracle `BaseObject.Run()` 的 `:26-109`，见 [`game_svr::stamina`]）。
///
/// 与 [`drain_recovery`] 同一位置（`PlayObject.Run()` 的恢复段之后）：逐会话（**按键排序**）调
/// [`game_svr::stamina::stamina_tick`]，命中（`HealthSpellChanged()` 被调用）⇒ 照
/// `BaseObject.HealthSpellChanged()` 给自己发 `SM_HEALTHSPELLCHANGED`（`ShowHp` 再广播）。
/// **一次 `Run()` 可命中两次**（HP 支 `:55` + MP 支 `:68`）⇒ 逐次各发一条帧，帧内容取自当次
/// `(HP, MP)` 快照（HP 支那条是「MP 未加」的中间态，照 `PlayObject.Message.cs:1736-1739`）。
///
/// **与 [`drain_recovery`] 的差别**：本段改动的是 `BaseObject.Run()` 里的 `WAbil.HP/MP`，
/// 攻击结算与 `state_log` 的 `SERVER_STATE` dump 读的是世界实体 [`mir2_world::Entity::hp`]
/// ⇒ 命中后**回写实体 `hp`**（`mp` 不在世界实体里，只在 `WAbil`，见 `Entity` 字段表）。
///
/// **登记（阶段顺序）**：oracle 一次 `ProcessPlayObjectData` 的顺序是「`PlayObject.Run()` 的恢复段
/// → `BaseObject.Run()` 的消息段（攻击/移动在此扣点）→ `BaseObject.Run()` 的体力点段」。
/// 本宿主把玩家指令走了 gate→sim 的异步通道（不在 tick 内收消息）⇒ 本段相对同 tick 内已处理的
/// 指令晚一拍/早一拍（最多一个 world tick = 200ms）。掉血阈值跨过点因此可能与 oracle 差 1 击，
/// 但**终值一致**（本卡判据 = `scoped_diff` 的末态），登记不静默。
fn drain_stamina(
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    world: &mut mir2_world::World,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    range: i32,
    health_fill_time: i32,
    spell_fill_time: i32,
) {
    let now = now_ms();
    let mut names: Vec<String> = sessions.keys().cloned().collect();
    names.sort();
    for name in names {
        let Some(sg) = sessions.get_mut(&name) else {
            continue;
        };
        let death = world.entities.get(sg.actor_id).is_some_and(|e| e.death);
        // oracle `HealthSpellChanged()` 的调用次数 = 帧数（HP 支/MP 支各一条，内容为当时 WAbil）。
        let mut changes: Vec<(u16, u16)> = Vec::new();
        let changed = game_svr::stamina::stamina_tick(
            &mut sg.stamina,
            &mut sg.state.wabil,
            death,
            now,
            health_fill_time,
            spell_fill_time,
            &mut changes,
        );
        if changed == 0 {
            continue;
        }
        let actor_id = sg.actor_id;
        let show_hp = sg.show_hp;
        let max_hp = sg.state.wabil.max_hp;
        // 回写世界实体：攻击结算（`crates/world`）与逐击 `SERVER_STATE` dump 读 `Entity.hp`。
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.hp = sg.state.wabil.hp;
        }
        let Some(entry) = online.iter().find(|e| e.chr_name == name) else {
            glog!(
                "WRN",
                "体力/魔力点：ChrName={name} 无在线网关坐标 ⇒ 不回帧（登记）"
            );
            continue;
        };
        // 每次 `HealthSpellChanged()` 各发一条帧；帧内容取自**当次快照**（不是终态）——
        // oracle `PlayObject.Message.cs:1736-1739` 用当时 `WAbil` 拼包，HP 支那条是中间态。
        for (hp, mp) in &changes {
            let body = game_svr::smframe::sm_healthspellchanged(actor_id, *hp, *mp, max_hp);
            if let Some(tx) = route_to_gate(gates, entry.gate_idx, entry.socket, "体力/魔力点回帧")
            {
                let _ = tx.send(SimToGate::SendFrame {
                    socket: entry.socket,
                    body: body.clone(),
                });
            }
            if show_hp {
                broadcast_healthspellchanged(&*world, online, gates, actor_id, &body, range);
            }
        }
    }
}

/// oracle `WorldServer.ProcessPlayObjectData`（`WorldServer.cs:698`）的周期存档判定：
///
/// ```text
/// !M2Share.FrontEngine.IsFull() && (HUtil32.GetTickCount() - playObject.SaveRcdTick)
///     > SystemShare.Config.SaveHumanRcdTime
///   ⇒ playObject.SaveRcdTick = HUtil32.GetTickCount(); playObject.DealCancelA(); SaveHumanRcd(playObject);
/// ```
///
/// 两个口径都照抄：**严格 `>`**（相等不触发）与**先重置再发请求**。
/// `IsFull()` 是 `m_SaveRcdList.Count >= int.MaxValue`（`FrnEngn.cs:120-133`）⇒ 实际恒 `false`
/// （由调用方 [`load::FrontEngine::is_full`] 映射）。
fn periodic_save_due(save_rcd_tick: &mut i64, save_time: i64, now: i64) -> bool {
    if now - *save_rcd_tick > save_time {
        *save_rcd_tick = now;
        true
    } else {
        false
    }
}

/// 每 tick 的**周期存档**（oracle `WorldServer.cs:698-703`，卡 `openmir2-m3-periodic-save`）。
///
/// 对**每个在线角色**复用既有三处触发的同一口径（[`save_human_rcd`] ⇒ `MakeSaveRcd` 逐字段 +
/// in-save 配对 + `save_outstanding` 轮询）。逐会话**按键排序**（不让 `HashMap` 迭代序参与逻辑）。
///
/// # 计时口径（oracle 逐行，已核）
///
/// - **起算**：`PlayObject.SaveRcdTick` 在 `new PlayObject()`（`PlayObject.cs:886`）取 `GetTickCount()`；
///   oracle 每次登入都 `new PlayObject()`（`WorldServer.cs:234`，不复用 free list）⇒ 等价于本宿主
///   「进世界建 `SessionGround` 时取 [`now_ms`]」。断线/换图 **不复用** 对象 ⇒ 下次进世界重新起算。
/// - **重置**：每次触发时**先**把 `SaveRcdTick = GetTickCount()`，**再** `SaveHumanRcd`（`:700-702`）。
/// - **单位**：`SaveHumanRcdTime` 是毫秒（`GameSvrConf.cs:1484` 的 `10*60*1000`），缺省 10 分钟。
/// - **门**：`!IsFull()`（恒真，见 [`periodic_save_due`]）+ oracle 只在 `LoginNoticeOk && BoReadyRun` 的
///   最内层 `else` 里查（`:685-698`）。本宿主 `SessionGround` 在入图帧全部下发后才建 ⇒ 建即已就绪；
///   构造 → 就绪之间的亚秒窗口与「10 分钟」量级无关，登记不影响判据。
///
/// # 登记（未建模，不静默）
///
/// - `playObject.DealCancelA()`（`PlayObject.Base.cs:748-752`）= `Abil.HP = WAbil.HP; DealCancel();`：
///   交易层未移植，且 `MakeSaveRcd` 读的是 `WAbil.HP`（`WorldServer.cs:1247`）⇒ 对存档体无影响。
/// - oracle 的 `SaveHumanRcd` 里 `IsRobot` 早返回：本宿主不建机器人会话（`RobotProcessor` 未移植）。
///
/// 返回**本轮发起的请求数**（DB 未连接时计时仍已重置——照 oracle 先重置再调用；计数为 0）。
#[allow(clippy::too_many_arguments)]
fn drain_periodic_save(
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    saves: &std::collections::HashMap<String, SavedHuman>,
    world: &mir2_world::World,
    db: &mut Option<DbWiring>,
    engine: &mut load::FrontEngine,
    registry: Option<&mir2_envir::MapRegistry>,
    save_time: i64,
    now: i64,
) -> usize {
    // oracle `:698` 的 `!FrontEngine.IsFull()`：`m_SaveRcdList.Count >= int.MaxValue`
    // （`FrnEngn.cs:120-133`）⇒ 恒 false。查一次与逐角色查等价（本宿主每 tick 调用一次）。
    if engine.is_full() {
        return 0;
    }
    let mut names: Vec<String> = sessions.keys().cloned().collect();
    names.sort();
    let mut fired = 0usize;
    for name in names {
        let Some(sg) = sessions.get_mut(&name) else {
            continue;
        };
        if !periodic_save_due(&mut sg.save_rcd_tick, save_time, now) {
            continue;
        }
        // 计时已重置（oracle `:700` 先重置再存档）；没留下登入记录底稿的会话无法合成存档体。
        let Some(saved) = saves.get(&name) else {
            glog!(
                "WRN",
                "周期存档：ChrName={name} 无登入记录底稿 ⇒ 跳过（登记；不计入发起数）"
            );
            continue;
        };
        if save_human_rcd(db, engine, saved, &name, world, sg, registry).is_some() {
            fired += 1;
            glog!(
                "INF",
                "周期存档触发（SaveHumanRcdTime={save_time}ms）ChrName={name}"
            );
        }
    }
    fired
}

/// `HealthSpellChanged()` 的 `ShowHp` 支（`SendRefMsg(RM_HEALTHSPELLCHANGED,…)`）在同一图上按
/// `SendRefMsgRange` 广播；`body` 是源 actor 的那条 `SM_HEALTHSPELLCHANGED`（观察者收到同一字节）。
fn broadcast_healthspellchanged(
    world: &mir2_world::World,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    actor_id: i32,
    body: &[u8],
    range: i32,
) {
    let Some(src) = world.entities.get(actor_id) else {
        return;
    };
    let (sx, sy, smap) = (src.x, src.y, src.map_id);
    for e in online {
        let Some(t) = world.entities.get(e.actor_id) else {
            continue;
        };
        if t.map_id != smap || (t.x - sx).abs() > range || (t.y - sy).abs() > range {
            continue;
        }
        if let Some(tx) = route_to_gate(gates, e.gate_idx, e.socket, "周期恢复广播") {
            let _ = tx.send(SimToGate::SendFrame {
                socket: e.socket,
                body: body.to_vec(),
            });
        }
    }
}

/// 在线用户的世界侧网关坐标（`PlayObject.GateIdx`/`SocketId` 的对应物）。
///
/// 进世界成功时登记，断线/被踢时移除；重复进世界的"踢旧"与账号级顶号靠它找回旧连接。
/// 用 `Vec` 而不是 `HashMap`：顺序确定（本项目不让 `HashMap` 迭代序参与逻辑）。
///
/// `account`/`session_id` 用于账号级顶号（`TCPNetChannel.KickUser` 按 `Account` 或 `SessionID` 匹配）；
/// `actor_id` 是进世界时拿到的实体 id（= oracle `PlayObject.ActorId`），`SM_SYSMESSAGE` 的 `Recog` 要用它。
type OnlineRegistry = Vec<OnlineEntry>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct OnlineEntry {
    account: String,
    chr_name: String,
    session_id: i32,
    gate_idx: i32,
    socket: i32,
    actor_id: i32,
}

fn online_set(online: &mut OnlineRegistry, e: OnlineEntry) {
    if let Some(x) = online.iter_mut().find(|x| x.chr_name == e.chr_name) {
        *x = e;
        return;
    }
    online.push(e);
}

fn online_take(online: &mut OnlineRegistry, chr: &str) -> Option<OnlineEntry> {
    let pos = online.iter().position(|e| e.chr_name == chr)?;
    Some(online.remove(pos))
}

/// 账号级顶号的目标查找（oracle `TCPNetChannel.KickUser`，`TCPNetChannel.cs:246-274`）：
/// 对**每个网关** `_gameGates[i]` 的 `UserList` 线性扫，命中**第一个**匹配项即 `break`
/// ⇒ 语义是「**每个网关最多踢一个**」（不是「全踢同名/同会话」，也不是「全局首个」）。
/// 本函数按 `gate_idx` 分桶，每桶取登记序里的首个匹配（顺序确定，不依赖 `HashMap` 迭代序）。
///
/// 匹配条件（`:252`）：`Account` **OrdinalIgnoreCase** 相等 **或** `SessionID` 相等。
///
/// **O-3 裁决（owner `cb0c70fd`，2026-10-12）**：本项目硬约束是「行为等价优先于修 bug」，oracle 的
/// 「每网关各扫各 break」在「同一账号经两个网关各有一条会话」时与「全局首个」**必然可见差异**
/// （`tools/botload` 可构造）⇒ **按 oracle 1:1 修**，不再按「实语料不可观测」登记偏差；
/// 可判红用例见 `account_kick_targets_every_gateway_once`。
fn online_find_for_kick(
    online: &OnlineRegistry,
    account: &str,
    session_id: i32,
) -> Vec<OnlineEntry> {
    let matched =
        |e: &OnlineEntry| e.account.eq_ignore_ascii_case(account) || e.session_id == session_id;
    let mut targets: Vec<OnlineEntry> = Vec::new();
    let mut seen_gate: Vec<i32> = Vec::new();
    for e in online.iter().filter(|e| matched(e)) {
        if seen_gate.contains(&e.gate_idx) {
            continue; // 该网关已命中第一个 ⇒ oracle 的 `break`
        }
        seen_gate.push(e.gate_idx);
        targets.push(e.clone());
    }
    targets
}

/// 同一角色**重复进世界**的即时动作（oracle `WorldServer.cs:503/535/536-537/568-570`）。
///
/// 准入判据 `!IsFull && !ProcessHumansIsLogined(chr)`（`:503`）为假时：
/// ① `:535` `KickOnlineUser(chr)`（`:1052-1064`）给**旧** PlayObject 置 `BoKickFlag`
///    （oracle 下一 tick 在 `RunNotice` 里收 `SM_OUTOFCONNECTION` 后 `MakeGhost()`；本骨架在同一
///    tick 的后段 [`drain_kicks`] 落地 —— 帧序不变，见 docs `M2会话生命周期-对齐Rust-20261011.md` §4）；
/// ② `:536-537` 记下**新**连接的 `(GateIdx, SocketId)`；
/// ③ `:568-570` `NetChannel.CloseUser(新)` 把新连接关掉。
///
/// `kick_old` = 旧会话**是否在 `PlayObjectList` 里**（实测口径：命中 `InSaveRcdList` 但旧已不在
/// `PlayObjectList` 时，`KickOnlineUser` 扫不到旧 ⇒ **只关新、不置踢旧标志**，旧也就收不到 528）。
///
/// **O-4 登记（等价，不静默）**：准入的 `!IsFull()` 一分句在本骨架**省略**——oracle `FrnEngn.IsFull()`
/// 判据是 `m_SaveRcdList.Count >= int.MaxValue`（`FrnEngn.cs:120-135`），恒为 false ⇒ 省略等价。
///
/// 本函数只做"置踢旧标志 + 关新连接"这两件即时动作；旧连接的落地（发 528 + 摘世界）交给同一 tick 后段的
/// [`drain_kicks`]。
fn handle_duplicate_enter(
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    kick_pending: &mut Vec<String>,
    chr_name: &str,
    new_gate_idx: i32,
    new_socket: i32,
    kick_old: bool,
) {
    glog!(
        "WRN",
        "重复进世界 ChrName:{chr_name}：命中 ProcessHumansIsLogined（在世界={kick_old}） ⇒ 关新（oracle WorldServer.cs:503/535/536-537/568-570）"
    );
    // ③ 关新连接（`:568-570`）
    if let Some(tx) = route_to_gate(gates, new_gate_idx, new_socket, "关掉重复进世界的新连接")
    {
        let _ = tx.send(SimToGate::KickUser { socket: new_socket });
    }
    // ① 置旧会话的 BoKickFlag（同一 tick 的后段由 `drain_kicks` 落地）。
    // 仅当旧仍在 `PlayObjectList`（这里用 `online` 登记近似）时；旧若只剩"保存中"，
    // `KickOnlineUser` 本就扫不到，不得凭空给它发 528（口径以夹具实测为准）。
    if kick_old {
        if !kick_pending.iter().any(|n| n == chr_name) {
            kick_pending.push(chr_name.to_string());
        }
    } else {
        glog!(
            "INF",
            "重复进世界 ChrName:{chr_name}：旧不在 PlayObjectList ⇒ 不置踢旧标志（oracle KickOnlineUser 扫不到）"
        );
    }
}

/// 处理待踢名单（oracle `PlayObject.RunNotice` 的 `BoKickFlag` 支，`PlayObject.cs:1082-1090`）：
/// 给**旧**连接发 `SM_OUTOFCONNECTION`（`SendDefMessage(SM_OUTOFCONNECTION, 0, 0, 0, 0)`），
/// 再 `MakeGhost()` = 离开世界（摘格子 + 移除实体）并清掉旧槽。
#[allow(clippy::too_many_arguments)]
fn drain_kicks(
    world: &mut mir2_world::World,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    online: &mut OnlineRegistry,
    kick_pending: &mut Vec<String>,
    db: &mut Option<DbWiring>,
    engine: &mut load::FrontEngine,
    saves: &mut std::collections::HashMap<String, SavedHuman>,
    ground_sessions: &mut std::collections::HashMap<String, SessionGround>,
    registry: Option<&mir2_envir::MapRegistry>,
) {
    for chr in std::mem::take(kick_pending) {
        let Some(e) = online_take(online, &chr) else {
            glog!(
                "WRN",
                "重复进世界踢旧：ChrName:{chr} 的在线网关坐标未知，无法踢旧（oracle 只置标志；本骨架须有坐标才能清槽）"
            );
            continue;
        };
        if let Some(tx) = route_to_gate(gates, e.gate_idx, e.socket, "踢掉旧会话") {
            let _ = tx.send(SimToGate::SendFrame {
                socket: e.socket,
                body: smframe::sm_outofconnection(),
            });
            let _ = tx.send(SimToGate::KickUser { socket: e.socket });
        }
        // 存档：oracle `MakeGhost()` 里的 `SaveHumanRcd`（`WorldServer.cs:1077/713`）——
        // 必须在退出世界前拿实时态（坐标/物品/Gold）。
        if let (Some(saved), Some(sg)) = (saves.get(&chr), ground_sessions.get(&chr)) {
            save_human_rcd(db, engine, saved, &chr, world, sg, registry);
        }
        match world.disconnect(&chr) {
            Ok(()) => glog!(
                "INF",
                "重复进世界踢旧：旧会话已离开世界 ChrName:{chr} socket={}（MakeGhost）",
                e.socket
            ),
            Err(e) => glog!(
                "WRN",
                "重复进世界踢旧：旧会话离开世界失败 ChrName:{chr}：{e:?}"
            ),
        }
        saves.remove(&chr);
        ground_sessions.remove(&chr);
    }
}

/// `TCPNetChannel.cs:229` 的 `sKickUserMsg`——账号级顶号提示（**逐字照抄**；线上按 GB2312 字节）。
///
/// 该字面量是 oracle 里唯一会被 `KickUser` 发出的提示：`DelSession`（`AuthenticationService.cs:299-327`）
/// 在 `RemoveAt` 后把 `sessInfo` 置 `null`，于是 `sessInfo?.PayMode ?? 0` 恒为 0 ⇒ 永远走
/// `payMode == 0` 分支（`TCPNetChannel.cs:260-263`）。本仓 1:1 搬这个口径，不做"两分支都实现"。
const KICK_USER_MSG: &[u8] = &[
    0xb5, 0xb1, 0xc7, 0xb0, 0xb5, 0xc7, 0xc2, 0xbc, 0xd5, 0xca, 0xba, 0xc5, 0xd5, 0xfd, 0xd4, 0xda,
    0xc6, 0xe4, 0xcb, 0xfc, 0xce, 0xbb, 0xd6, 0xc3, 0xb5, 0xc7, 0xc2, 0xbc, 0xa3, 0xac, 0xb1, 0xbe,
    0xbb, 0xfa, 0xd2, 0xd1, 0xb1, 0xbb, 0xc7, 0xbf, 0xd0, 0xd0, 0xc0, 0xeb, 0xcf, 0xdf, 0x21, 0x21,
    0x21,
];

/// 消息前景/背景色（`[Setup] RedMsgFColor/RedMsgBColor`）——打包成结构体，
/// 避免 `sim_loop`/`handle_kick_account` 的参数个数超过 clippy 阈值（且两色总是一起用）。
#[derive(Debug, Clone, Copy)]
struct MsgColors {
    red_f: u8,
    red_b: u8,
    hear_f: u8,
    hear_b: u8,
}

/// 账号级顶号的**即时部分**（oracle `TCPNetChannel.KickUser`，`TCPNetChannel.cs:226-284`）：
/// 按 `Account`/`SessionID` 找到旧 `SessionUser` ⇒ `FrontEngine.DeleteHuman` ⇒
/// `SysMsg(sKickUserMsg, Red, Hint)`（= `SM_SYSMESSAGE`，**不是** `SM_OUTOFCONNECTION(528)`）⇒
/// `BoEmergencyClose`/`BoSoftClose` 置真。
///
/// 提示帧**立即**发（oracle 里 `KickUser` 当场 `SysMsg`）；离开世界/清槽由同一 tick **后段**的
/// [`drain_soft_close`] 落地（oracle 是旧 PlayObject **下一 tick** 的 `MakeGhost()`，本骨架早约 1 tick；
/// 帧序不变，见 docs §4 的 O-1 登记）。
fn handle_kick_account(
    engine: &mut load::FrontEngine,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    online: &OnlineRegistry,
    soft_close_pending: &mut Vec<String>,
    colors: MsgColors,
    account: &str,
    session_id: i32,
) {
    let targets = online_find_for_kick(online, account, session_id);
    if targets.is_empty() {
        glog!(
            "DBG",
            "账号级顶号：Account:{account} SessionId:{session_id} 无在线会话（无需踢）"
        );
        return;
    }
    // 每个网关各一个（oracle `KickUser` 的每网关 `break`）⇒ 逐条处置。
    for e in targets {
        engine.delete_human(e.socket);
        if let Some(tx) = route_to_gate(gates, e.gate_idx, e.socket, "账号级顶号提示") {
            let _ = tx.send(SimToGate::SendFrame {
                socket: e.socket,
                body: smframe::sm_sysmessage(e.actor_id, colors.red_f, colors.red_b, KICK_USER_MSG),
            });
        }
        if !soft_close_pending.iter().any(|n| n == &e.chr_name) {
            soft_close_pending.push(e.chr_name.clone());
        }
        glog!(
            "INF",
            "账号级顶号：向旧会话发 SM_SYSMESSAGE 并软关 ChrName:{} Account:{} SessionId:{} socket={}",
            e.chr_name,
            e.account,
            e.session_id,
            e.socket
        );
    }
}

/// 账号级顶号的**同一 tick 后段落地**部分（oracle `BoEmergencyClose`/`BoSoftClose` 支，
/// `PlayObject.cs:1082-1090`）：oracle 是旧 PlayObject **下一 tick** `MakeGhost()` 离开世界 +
/// `CloseUser(旧)`；本骨架在同一 tick 的后段落地（早约 1 tick，帧序不变，见 docs §4 的 O-1 登记）。
///
/// 与 [`drain_kicks`] 的区别：这里**不再发帧**（`SM_SYSMESSAGE` 已在 [`GateEvent::KickAccount`]
/// 处理时立即发出，照 oracle `KickUser` 里先 `SysMsg` 再置标志的次序）；而 `drain_kicks` 对应
/// `BoKickFlag` 支、要先发 `SM_OUTOFCONNECTION`(528)。
#[allow(clippy::too_many_arguments)]
fn drain_soft_close(
    world: &mut mir2_world::World,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    online: &mut OnlineRegistry,
    soft_close_pending: &mut Vec<String>,
    db: &mut Option<DbWiring>,
    engine: &mut load::FrontEngine,
    saves: &mut std::collections::HashMap<String, SavedHuman>,
    ground_sessions: &mut std::collections::HashMap<String, SessionGround>,
    registry: Option<&mir2_envir::MapRegistry>,
) {
    for chr in std::mem::take(soft_close_pending) {
        let Some(e) = online_take(online, &chr) else {
            glog!(
                "DBG",
                "账号级顶号落地：ChrName:{chr} 已不在在线登记（可能已自发断线）"
            );
            continue;
        };
        if let Some(tx) = route_to_gate(gates, e.gate_idx, e.socket, "软关账号级顶号的旧会话")
        {
            let _ = tx.send(SimToGate::KickUser { socket: e.socket });
        }
        // 存档（同 [`drain_kicks`]：oracle `BoSoftClose` 支也会 `MakeGhost` ⇒ `SaveHumanRcd`）。
        if let (Some(saved), Some(sg)) = (saves.get(&chr), ground_sessions.get(&chr)) {
            save_human_rcd(db, engine, saved, &chr, world, sg, registry);
        }
        match world.disconnect(&chr) {
            Ok(()) => glog!(
                "INF",
                "账号级顶号：旧会话已离开世界 ChrName:{chr} socket={}（MakeGhost）",
                e.socket
            ),
            Err(e) => glog!("WRN", "账号级顶号：旧会话离开世界失败 ChrName:{chr}：{e:?}"),
        }
        saves.remove(&chr);
        ground_sessions.remove(&chr);
    }
}

/// `CM_PICKUP`/`CM_DROPITEM` 的配置层输入（`Server.conf [Setup]`，逐条 oracle 缺省见 `config.rs`）。
#[derive(Debug, Clone, Copy)]
struct GroundCfg {
    /// `GameSvrConf.HumanMaxGold`（`PlayObject.Base.cs:171` 的 `IncGold` 上限）。
    human_max_gold: i32,
    /// `GameSvrConf.FloorItemCanPickUpTime`（`PlayObject.Base.cs:67`）。
    floor_item_can_pick_up_time: i32,
    /// `GameSvrConf.ControlDropItem`（`PlayObject.Operate.cs:284`）。
    control_drop_item: bool,
    /// `GameSvrConf.InSafeDisableDrop`（`PlayObject.Operate.cs:224/254`）。
    in_safe_disable_drop: bool,
    /// `GameSvrConf.CanDropPrice`（`PlayObject.Operate.cs:284`）。
    can_drop_price: i32,
    /// `GameSvrConf.DropItemRage`（`PlayObject.Operate.cs:291` 的 `_MIN(...,3)`）。
    drop_item_rage: u8,
}

/// 一个在线角色的**地面交互会话态**（`CM_PICKUP`/`CM_DROPITEM` 用）。
///
/// 进世界时从 `CharacterData` 建立、断线/小退时移除（`CharacterData` 在其他路径上用完即弃，
/// 本结构把 `ground::*` 要读写的字段留下来）。
struct SessionGround {
    /// `ground::GroundState`（`PlayObject.ItemList` + `Gold` + `WAbil`）。
    state: game_svr::ground::GroundState,
    /// `BaseObject.ActorId`（`SM_SYSMESSAGE` 的 `Recog`）。
    actor_id: i32,
    /// `BaseObject.GameGold`（`SM_GOLDCHANGED` 的 LoWord/HiWord）。
    game_gold: i32,
    /// `MessageSettings.CanotPickUpItem` 的 GBK 原文（缺则 `RM_SYSMESSAGE` 不产出）。
    sys_msg: Option<Vec<u8>>,
    /// `PlayObject.UseItems`（13 槽；`CM_TAKEONITEM`/`CM_TAKEOFFITEM` 迁移用）。
    use_items: Vec<Option<game_svr::bag::BagItem>>,
    /// **调用前的** `SubAbility`（752 的 7 项）——`RecalcAbilitys` 的 `prior`（`T-18` 调用点接线）。
    sub: game_svr::ability::SubAbility,
    /// 装备/卸下重算的静态输入（进世界时从 `CharacterData` + `Server.conf` 建立）。
    equip: EquipInputs,
    /// `CharacterObject.IncHealth/IncSpell/IncHealing` + `Per*` + `IncHealthSpellTick`
    /// （周期恢复 tick，[`game_svr::recover::Recovery`]；`CM_EAT` 的 `StdMode 0 default` 分支
    /// 叠加 `IncHealth/IncSpell`）。
    recovery: game_svr::recover::Recovery,
    /// `BaseObject.HealthTick/SpellTick/AutoRecoveryTick`（体力/魔力点，[`game_svr::stamina`]）：
    /// 攻击/移动扣点、按 `Run()` 节奏回收、到 `HealthFillTime` 回血、低于 `-HealthFillTime` 掉 1 HP。
    /// 本片判据：真机隔离两相 `scoped_diff` 唯一残留 `characters_ablity.Hp` 攻方（C# 20668/Rust 20669）。
    stamina: game_svr::stamina::Stamina,
    /// `BaseObject.ShowHp`（`MakeOpenHealth()` 置位，`BaseObject.cs:944`）；发点是技能「心灵启示」
    /// （`MagicManager.cs:438`）⇒ 技能层未移植，生产路径恒 `false`（登记）。
    show_hp: bool,
    /// `CharacterObject.UserUnLockDurg`（`CM_EAT` 的 `StdMode 0 Shape 2` 置 true）。
    user_unlock_durg: bool,
    /// `PlayObject.HungerStatus`（`CM_EAT` 的 `StdMode 1`：`+= DuraMax/10`，上限 5000）。
    hunger_status: i32,
    /// `PlayObject.QueryBagItemsTick`（`PlayObject.Message.cs:786` 的 30s 限频）：`None` = 未查过（照 oracle 首次即过）。
    query_bag_items_tick: Option<i64>,
    /// `PlayObject.SaveRcdTick`（`PlayObject.cs:601/886`）——周期存档计时基准（毫秒，
    /// 与 [`now_ms`] 同源）。进世界时起算（= oracle `new PlayObject()`），每次周期触发时
    /// **先重置**再发 `DB_SAVEHUMANRCD`（`WorldServer.cs:698-703`）；见 [`drain_periodic_save`]。
    save_rcd_tick: i64,
}

/// 一次 `RecalcAbilitys()` 的静态输入（不含随动作变化的 `UseItems`/`ItemList`/`WAbil`）。
///
/// 与 `enter::AbilityInputs` 同源，但**拥有**借用的记录字段（`CharacterData` 在进世界后即丢），
/// 供装备/卸下成功分支的重算（`ability::recalc_abilitys_with_prior`）复用。
#[derive(Debug, Clone)]
struct EquipInputs {
    /// `PlayerJob`（0 战 / 1 法 / 2 道）。
    job: u8,
    /// `PlayerGender`（记录 `sex`：0 男 / 1 女）。
    gender: u8,
    /// `Hair`（`GetFeatureToLong` 用）。
    hair: u8,
    /// `ReLevel`。
    re_level: u8,
    /// `CreditPoint`。
    credit_point: u16,
    /// `BaseObject.Abil`（角色记录基础能力）。
    abil: game_svr::ability::Ability,
    /// `PlayObject.BonusAbil`。
    bonus_abil: game_svr::ability::NakedAbility,
    /// `Config.BonusAbilofXxx.Hit`。
    bonus_tick_hit: u8,
    /// `Config.BonusAbilofXxx.Speed`。
    bonus_tick_speed: i32,
}

/// `IsNewHuman` 四件新手物品的名字 + `GetItemNumber` 初值（`Server.conf [Setup]`；`PlayObject.cs:1205-1245`）。
#[derive(Debug, Clone)]
struct NewbieItems {
    candle: String,
    basic_drug: String,
    wooden_sword: String,
    cloths_man: String,
    cloths_woman: String,
    item_number: i32,
}

/// `sim_loop` 需要的入图帧配置层（`Server.conf` 的 `GameSvrConf` + `String.conf` + `[Setup] ShowRankLevelName`）。
struct EnterConfig {
    game_conf: Option<mir2_envir::GameSvrConf>,
    string_conf: crate::strings::StringConf,
    show_rank_level_name: bool,
    /// `stditems` 物品目录（`game_svr::ability::StdItemCatalog`，**1-based** 查表，见 `crate::items`）；
    /// 空目录 = 未加载/无配置。帧 50/41 的 `StdItem.Shape` 与 52/752 的 `RecalcAbilitys` 共用。
    catalog: game_svr::ability::StdItemCatalog,
    /// `CM_PICKUP`/`CM_DROPITEM` 的 `[Setup]` 输入。
    ground: GroundCfg,
    /// `[Setup] SendRefMsgRange`（oracle `GameSvrConf.cs:499`，缺省 12）：
    /// 装备/卸下成功后的 `FeatureChanged()` 广播半径（`BaseObject.SendRefMsg` 的单元格扫描）。
    ref_msg_range: i32,
    /// `[Setup] SaveHumanRcdTime`（oracle `GameSvrConf.cs:423/1484`，缺省 `600000`ms）：
    /// 周期存档间隔（`WorldServer.cs:698-703`）；见 [`drain_periodic_save`]。
    save_human_rcd_time: i64,
    /// `[Setup] HealthFillTime`（oracle `GameSvrConf.cs:1323`，缺省 `300`）：
    /// 体力点回血/掉血阈值（`BaseObject.Base.cs:44/74/105`）；见 [`game_svr::stamina`]。
    health_fill_time: i32,
    /// `[Setup] SpellFillTime`（oracle `GameSvrConf.cs:1324`，缺省 `800`）：
    /// 魔力点回蓝阈值（`BaseObject.Base.cs:52/78`）。
    spell_fill_time: i32,
    /// 空 `HomeMap` 新角色的进图回落（`homemap::HomeFallback`，`WorldServer.cs:250-260`）。
    home: homemap::HomeFallback,
    /// `IsNewHuman` 的新手物品输入。
    newbie: NewbieItems,
    /// `Exps.conf` 的 `NeedExps` 表（`BaseObject.GetLevelExp` 的数据源，`crates/data::exps`）。
    /// 存档 `MaxExp`（`PlayObject.cs:1310`）与击杀经验都用它；读不到时为全 0（登记）。
    need_exps: Vec<i32>,
    /// **M3 T-30**：`MIR2_RNG_SEED` 解析出的固定种子（`seed::env_seed`）。`None` ⇒ 不注入，
    /// 保持 `World::with_maps` 的既有随机源（生产路径一字不改）。
    rng_seed: Option<i32>,
    /// **M3④-实现 B 第 ③ 片**：MonGen 刷怪启动输入（怪物模板表 + `Envir/MonGen.txt` 运行时条目
    /// + `Server.conf` 刷怪参数）；`entries=None` ⇒ 无 `MonGen.txt`，不建宿主。
    mon_gen: MonGenStartup,
}

/// MonGen 刷怪启动输入（[`game_svr::mongen`]）。
struct MonGenStartup {
    table: game_svr::mongen::MonsterTable,
    entries: Option<Vec<mir2_world::mongen::MonGenEntry>>,
    config: mir2_world::mongen::MonGenConfig,
}

/// `SEED_PROBE` 的 8 项参照序列（`new Random(seed).Next()` 前 `seed::PROBE_LEN` 项）。
///
/// **用独立实例且不经 `RandomNumber`**（[`mir2_shared::rng::DotNetRandom::new`] +
/// [`mir2_shared::rng::DotNetRandom::next_value`]）——与 C# 钩子（`rng-hook/StartupHook.cs` 的独立
/// `reference` 实例）以及 `crates/shared/src/rng_trace.rs::enable` 写进流水头部的 `SEED_PROBE`
/// 同口径：
/// - **不消耗**游戏随机流（独立实例）；
/// - **不进** `MIR2_RNG_LOG` 逐次流水。`RandomNumber::random()` 在 trace 开启时会记一条
///   `bins/game-svr/src/main.rs` 记录，而那 8 条是仪器自证、不是游戏消费者，C# 侧也不存在
///   ⇒ 会把两侧 `site/order` 账本的第一处分歧钉到 index 0（真机实测：Rust 独有站点
///   `main.rs:2856` × 8，见 `mir2-rs/tests/parity/m3/evidence-rng-ledger-diff-20261012/32-diff-full.txt`）。
///
/// 序列与 `RandomNumber::with_seed(seed)` 逐项相同（后者内层就是 `DotNetRandom::new(seed)`），
/// 单测 `seed_probe_text_matches_csharp_seed42_and_random_number` 把这层等价关系锁死。
fn seed_probe_text(seed: i32) -> String {
    let mut rng = mir2_shared::rng::DotNetRandom::new(seed);
    (0..seed::PROBE_LEN)
        .map(|_| rng.next_value().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// **M3④-实现 B 第 ③ 片**：单调一次 MonGen 调度（`WorldServer.MonGen.cs:192-253` 的单线程版）。
///
/// `world.rng` 会被临时移出并交给回调：oracle 的 `CreateMonster`（`:498-810`）与 `RegenMonsters`
/// （`:811-874`）取的是**同一条** `M2Share.RandomNumber`；本函数把同一条流贯穿「位置抽 → 每只
/// CreateMonster 取数」的 interleaving（`regen_monsters` 的回调契约），避免落库用的 `world.rng`
/// 与调度用的流分叉。
fn step_mon_gen(
    host: &mut game_svr::mongen::MonGenHost,
    table: &game_svr::mongen::MonsterTable,
    registry: &mir2_envir::MapRegistry,
    world: &mut mir2_world::World,
    play_object_count: i32,
) {
    let now = i32::try_from(world.clock_ms).unwrap_or(i32::MAX);
    // oracle `map.HumCount`：当前图上的玩家数。
    let mut hum_by_map: std::collections::HashMap<usize, i32> = std::collections::HashMap::new();
    for e in world.entities.iter() {
        if e.race == mir2_world::ACTOR_RACE_PLAY {
            *hum_by_map.entry(e.map_id).or_insert(0) += 1;
        }
    }
    // `canCreate = !(map == null || (map.Flag.boNOHUMNOMON && map.HumCount <= 0))`（`:235-243`）。
    let mut can_map: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    for entry in &host.state.entries {
        if can_map.contains_key(&entry.map_name) {
            continue;
        }
        let ok = registry.id_of(&entry.map_name).is_some_and(|id| {
            registry.flag_of(id).is_some_and(|f| {
                !(f.bo_nohumnomon && hum_by_map.get(&id).copied().unwrap_or(0) <= 0)
            })
        });
        can_map.insert(entry.map_name.clone(), ok);
    }
    let mut can = |name: &str| can_map.get(name).copied().unwrap_or(false);
    // `HUtil32.GetTickCount()` 的确定性注入：同一 tick 内取同一值（`regen_monsters` 的
    // `ZenLimit` 闸门因此不会因墙钟抖动而提前中断；oracle 的墙钟分支见 `game_svr::mongen` 模块头）。
    let tick_now = now;
    let mut clock = || tick_now;
    // 把 `world.rng` 移出：`WorldMonsterSink` 需要 `&mut world` 落库，而取数必须走同一条流。
    let mut rng = std::mem::replace(&mut world.rng, mir2_shared::rng::RandomNumber::with_seed(0));
    {
        let mut sink = game_svr::mongen::WorldMonsterSink {
            world,
            registry,
            table,
        };
        let _ = host.step(
            now,
            play_object_count,
            &mut rng,
            &mut clock,
            &mut can,
            &mut sink,
            table,
        );
    }
    world.rng = rng;
}

fn sim_loop(
    map_sizes: Vec<(i32, i32)>,
    registry: Option<mir2_envir::MapRegistry>,
    mut enter_cfg: EnterConfig,
    notice: Vec<u8>,
    colors: MsgColors,
    ev_rx: std::sync::mpsc::Receiver<GateEvent>,
    mut db: Option<DbWiring>,
) {
    // 尺寸表已由调用方按「注册表优先，无注册表则 WorldMaps 占位（且非空，已 fail-closed）」算好；
    // `map_id` = 注册表下标，与 `resolve_map_id` 同源。
    let mut world = mir2_world::World::with_maps(map_sizes);
    // **M3 T-30**：种子注入只在设了 `MIR2_RNG_SEED` 时覆盖随机源（未设 ⇒ 保持 `World::with_maps`
    // 的既有随机源，`crates/world` 行为一字不改）。发一条与 C# 钩子同形的 `SEED_PROBE` 自证行：
    // 参考序列走**独立实例**（[`seed_probe_text`]）——与 C# 钩子 / `rng_trace` 头部同口径，
    // 不消耗游戏流，也**不进** `MIR2_RNG_LOG` 逐次流水（经 `RandomNumber` 取那 8 项会把两侧
    // `site/order` 账本的第一分歧钉到 index 0）。
    if let Some(seed) = enter_cfg.rng_seed {
        world.rng = mir2_shared::rng::RandomNumber::with_seed(seed);
        let probe_text = seed_probe_text(seed);
        glog!(
            "INF",
            "RNG 同种子注入：seed={seed} 参照序列=SEED_PROBE {probe_text}"
        );
    }
    // `Exps.conf` 的 NeedExps（`BaseObject.GetLevelExp` 的数据源；存档 `MaxExp` 也用它）。
    world.need_exps = enter_cfg.need_exps.clone();
    // `Envir.Flag.boEXPRATE`/`ExpRate`（`PlayObject.Base.cs:143-146`）——地图级杀怪经验倍率。
    // `map_id` = 注册表下标（与 `resolve_map_id` 同源）⇒ 逐图灌进 world；无注册表 ⇒ 全 `None`（行为不变）。
    if let Some(reg) = registry.as_ref() {
        for map_id in 0..world.maps.len() {
            if let Some(f) = reg.flag_of(map_id) {
                world.set_map_exp_rate(map_id, f.bo_exprate.then_some(f.exp_rate));
            }
        }
    }
    // **M3④-实现 C 第 ② 片**：生产宿主启用怪物 AI（oracle `ProcessMonsters` 线程的单线程等价物）。
    // 生成路径已按 oracle 把 `cert.Dir`/`CoolEye`/构造链抽数写进 `Entity.ai`
    // （`World::spawn_monster_precomputed` → `AnimalState::seed_from_spawn`），这里只开闸；
    // `World::tick` 在死亡结算之后、AOI 刷新之前按实体槽位顺序驱动 `monster_ai_tick`。
    // 合成 golden 场景不经本函数 ⇒ 仍缺省关闭，逐字节不变。
    game_svr::mongen::enable_production_monster_ai(&mut world);
    glog!("INF", "Monster AI 已启用（生产宿主）EnableMonsterAI=true");
    // **M3④-实现 B 第 ③ 片**：MonGen 刷怪调度宿主（`WorldServer.MonGen.cs:30/192/811`）。
    // `entries=None` ⇒ 无 `Envir/MonGen.txt`，不建宿主；`initialize_monster` 在此消耗一次
    // `world.rng`（oracle `InitializeMonster` 同一时机；RNG 次序 = 怪名分组序 + `MonsterList.Values`）。
    let mut mon_gen_host = match enter_cfg.mon_gen.entries.take() {
        Some(entries) => {
            let host = game_svr::mongen::MonGenHost::start(
                entries,
                &enter_cfg.mon_gen.table,
                true, // 有 MonGen.txt ⇒ `LocalDB.cs:570-590` 末尾哨兵已入 thread 0
                enter_cfg.mon_gen.config,
                &mut world.rng,
            );
            glog!(
                "INF",
                "MonGen 刷怪宿主就绪：{} 条配置，{} 个怪名，threads={}，哨兵={}（mon_limit={} zen_limit={} user_full={} regen_time={}ms gen_rate={}）",
                host.state.entries.len(),
                host.init.distinct_mon_names,
                host.state.threads.len(),
                host.init.sentinel_present,
                host.config.mon_limit,
                host.config.zen_limit,
                host.config.user_full,
                host.config.regen_monsters_time,
                host.config.mon_gen_rate
            );
            Some(host)
        }
        None => None,
    };
    // 取角色的加载队列（`FrnEngn` 的双缓冲 + `CharacterDataProcessor` 的读档循环，见 `load.rs`）
    let mut engine = load::FrontEngine::new();
    // `openmir2-m2-session-lifecycle`：在线用户的世界侧网关坐标 + 待踢名单（重复进世界用）
    let mut online: OnlineRegistry = Vec::new();
    let mut kick_pending: Vec<String> = Vec::new();
    // 账号级顶号（`SS_CLOSESESSION` ⇒ `KickUser`）：提示帧已即时发出，离开世界/清槽隔一 tick 落地。
    let mut soft_close_pending: Vec<String> = Vec::new();
    // `GateIdx -> 该网关连接的 sim→gate 通道`（`NetChannel` 按 gateIdx 选连接，`TCPNetChannel.cs:221-223`）
    let mut gates: std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>> =
        std::collections::HashMap::new();
    // S3 后半段（第一格）：同屏 AOI 下行要用的展示表（`ActorId -> Dir/Light/Feature/…`）。
    // 展示字段来自角色记录 + 装备（属 bins/game-svr 的读档面），**不进 `crates/world`**。
    let mut displays = downlink::DisplayTable::new();
    // owner 2026-10-12 23:50 第 ③ 条 / §5.254（夹具 `run_attack_pair.ps1:1164`「归 m2-svr」）：
    // **服务端权威逐击状态 dump**（环境变量 `MIR2_RNG_STATE_LOG`；未设 ⇒ 关闭、零行为差）。
    // 每个**玩家攻击单元**落一行，供 `server_state_gate.py` 判「目标格上是否还站着 `!Ghost && !Death`
    // 的对象」——客户端视图看不到服务端 `Ghost`，必须用它。
    let mut state_log = state_log::StateLog::from_env();
    if state_log.enabled() {
        // 只在**设了** `MIR2_RNG_STATE_LOG` 时打印（未设 ⇒ 一行不多，行为与改动前逐字节相同）。
        glog!(
            "INF",
            "服务端逐击状态 dump 已开启（{}）：每个玩家攻击单元落一行 SERVER_STATE",
            state_log::STATE_LOG_ENV
        );
    }
    // `M2Share.GetItemNumber()` 的计数器（`M2Share.cs:547`；初值 `[Setup] ItemNumber`）——
    // 新角色新手物品的 `MakeIndex` 用它。
    let mut item_number = enter_cfg.newbie.item_number;
    // `CM_PICKUP`/`CM_DROPITEM` 的**地面交互会话态**（按 `ChrName` 键；进世界建立、断线/小退移除）。
    // 进世界前解析出的 `CharacterData` 在其他路径上用完即弃，这里把 `ground::*` 要的字段留下来。
    let mut ground_sessions: std::collections::HashMap<String, SessionGround> =
        std::collections::HashMap::new();
    // 存档用：登入时留下的原始记录 + 进图逻辑名（断线/`MakeGhost` 时合成存档请求）。
    let mut saves: std::collections::HashMap<String, SavedHuman> = std::collections::HashMap::new();
    glog!(
        "INF",
        "sim 线程就绪：tick={}ms，地图 {} 张（{}）",
        mir2_world::TICK_INTERVAL_MS,
        world.maps.len(),
        if registry.is_some() {
            "Envir 真地图"
        } else {
            "WorldMaps 占位"
        }
    );
    let tick = std::time::Duration::from_millis(
        u64::try_from(mir2_world::TICK_INTERVAL_MS).unwrap_or(200),
    );
    let mut ticks: u64 = 0;
    loop {
        let next = std::time::Instant::now() + tick;
        while let Ok(ev) = ev_rx.try_recv() {
            match ev {
                GateEvent::Opened { index, ip } => {
                    glog!(
                        "DBG",
                        "sim: 新用户登记 index={index} ip={ip}（open_session 属 S2b-3）"
                    );
                }
                GateEvent::Command {
                    index,
                    chr_name,
                    socket,
                    cmd,
                    tail,
                } => {
                    glog!(
                        "DBG",
                        "sim: index={index} 命令 Recog={} Ident={} Param={} Tag={} Series={} tail={:?}",
                        cmd.recog,
                        cmd.ident,
                        cmd.param,
                        cmd.tag,
                        cmd.series,
                        tail.as_deref().unwrap_or("")
                    );
                    // `CM_SAY`：普通说话回 `SM_HEAR`（其余 ident 仍在切片清单里，只记日志）。
                    if cmd.ident == chat::CM_SAY {
                        handle_say(
                            &online,
                            &world,
                            &gates,
                            &chr_name,
                            socket,
                            tail.as_deref(),
                            &colors,
                        );
                    } else if cmd.ident == game_svr::ground::CM_PICKUP
                        || cmd.ident == game_svr::ground::CM_DROPITEM
                    {
                        handle_ground_cmd(
                            cmd.ident,
                            cmd.param,
                            cmd.tag,
                            tail.as_deref(),
                            &chr_name,
                            socket,
                            &world,
                            &mut ground_sessions,
                            &online,
                            &gates,
                            &enter_cfg.ground,
                            &colors,
                            &enter_cfg.catalog,
                            registry.as_ref(),
                        );
                    } else if cmd.ident == game_svr::bag::CM_TAKEONITEM
                        || cmd.ident == game_svr::bag::CM_TAKEOFFITEM
                    {
                        handle_bag_cmd(
                            cmd.ident,
                            cmd.recog,
                            cmd.param,
                            cmd.tag,
                            tail.as_deref(),
                            &chr_name,
                            socket,
                            &online,
                            &mut ground_sessions,
                            &gates,
                            &enter_cfg.catalog,
                            &world,
                            &mut displays,
                            enter_cfg.ref_msg_range,
                        );
                    } else if cmd.ident == game_svr::useitem::CM_EAT {
                        // `CM_EAT(1006)`：oracle `PlayObject.Message.cs:826` 的 `ClientUseItems(nParam1, Msg)`。
                        handle_eat_cmd(
                            cmd.recog,
                            cmd.param,
                            cmd.tag,
                            tail.as_deref(),
                            &chr_name,
                            socket,
                            &online,
                            &mut ground_sessions,
                            &gates,
                            &enter_cfg.catalog,
                            &world,
                            &colors,
                            registry.as_ref(),
                        );
                    } else if cmd.ident == mir2_protocol::messages::CM_QUERYBAGITEMS {
                        // `CM_QUERYBAGITEMS(81)`：`PlayObject.Message.cs:784` → `ClientQueryBagItems()`。
                        handle_query_bag_cmd(
                            &chr_name,
                            socket,
                            &online,
                            &mut ground_sessions,
                            &gates,
                            &enter_cfg.catalog,
                            &world,
                        );
                    } else if matches!(
                        cmd.ident,
                        mir2_protocol::messages::CM_TURN
                            | mir2_protocol::messages::CM_WALK
                            | mir2_protocol::messages::CM_RUN
                            | mir2_protocol::messages::CM_HORSERUN
                    ) {
                        // `openmir2-m3-move-wiring`：`CM_TURN/WALK/RUN/HORSERUN` 派发
                        // （oracle `PlayObject.Message.cs:1008-1210`）。
                        handle_move_cmd(
                            cmd.ident,
                            cmd.recog,
                            cmd.tag,
                            &chr_name,
                            socket,
                            &online,
                            &mut ground_sessions,
                            &mut world,
                            &mut displays,
                            &gates,
                        );
                    } else if game_svr::attackwire::is_hit_ident(cmd.ident) {
                        // `openmir2-m3-attack-magic-wiring`（第一片）：9 条 `CM_*HIT` 派发
                        // （oracle `PlayObject.Message.cs:1221-1230` → `ClientHitXY`）。
                        // 返回 `true` = 本击进入了攻击路径（抽攻击力）⇒ 落一行服务端权威状态 dump
                        // （owner 2026-10-12 23:50 第 ③ 条；与 `StartupHook.cs::DumpActors` 同形）。
                        let attacked = handle_hit_cmd(
                            cmd.ident,
                            cmd.recog,
                            cmd.tag,
                            &chr_name,
                            socket,
                            &online,
                            &mut world,
                            &mut displays,
                            &mut ground_sessions,
                            &gates,
                        );
                        if attacked {
                            state_log.record(&world, &displays, registry.as_ref());
                        }
                    } else if magicwire::is_magic_key_ident(cmd.ident) {
                        // `openmir2-m3-attack-magic-wiring`（第二片）：`CM_MAGICKEYCHANGE(1008)`
                        // （oracle `WorldServer.cs:857-859` → `PlayObject.Operate.cs:774`）。
                        handle_magic_key_cmd(cmd.recog, cmd.param, &chr_name, socket, &mut saves);
                    } else if magicwire::is_spell_ident(cmd.ident) {
                        // 同片：`CM_SPELL(3017)` 派发（oracle `WorldServer.cs:783-790`）。
                        handle_spell_cmd(
                            cmd.recog,
                            cmd.tag,
                            cmd.param,
                            cmd.series,
                            &chr_name,
                            socket,
                            &online,
                            &world,
                            &saves,
                            &mut displays,
                            &gates,
                        );
                    }
                }
                GateEvent::CertificationPending { index, ip } => {
                    glog!(
                        "DBG",
                        "sim: index={index} ip={ip} 收到无 `!` 的帧 ⇒ 认证链返回 Ignored（oracle 的 `if` 不成立）"
                    );
                }
                GateEvent::Closed {
                    index,
                    chr_name,
                    socket,
                } => {
                    // oracle `ChannelMessageHandler.CloseUser`（`:363-...`）：先
                    // `FrontEngine.DeleteHuman(i, socket)` 摘掉未完成的读档项；已进世界的 PlayObject
                    // 走 `BoSoftClose` ⇒ `RunNotice`（`PlayObject.cs:1082-1090`）的 `MakeGhost()`
                    // 离开世界（摘格子 + 移除实体）。
                    engine.delete_human(socket);
                    if chr_name.is_empty() {
                        glog!(
                            "DBG",
                            "sim: index={index} 关闭（未认证/无角色，无需清世界）"
                        );
                    } else {
                        kick_pending.retain(|n| n != &chr_name);
                        online_take(&mut online, &chr_name);
                        // 存档：oracle `CloseUser` 后的 `MakeGhost()` 里调 `SaveHumanRcd`
                        //（`WorldServer.cs:1077`）——必须在摘世界/移除会话态前拿实时态。
                        if let (Some(saved), Some(sg)) =
                            (saves.get(&chr_name), ground_sessions.get(&chr_name))
                        {
                            save_human_rcd(
                                &mut db,
                                &mut engine,
                                saved,
                                &chr_name,
                                &world,
                                sg,
                                registry.as_ref(),
                            );
                        }
                        saves.remove(&chr_name);
                        // 地面交互会话态随断线一并移除（否则重进后还带着旧 `ItemList`/`Gold`）。
                        ground_sessions.remove(&chr_name);
                        // 同屏展示表随实体一并摘除（ActorId 槽位会被内核回收复用，避免旧展示值残留）。
                        if let Some(aid) = world.session(&chr_name).and_then(|s| s.actor_id) {
                            displays.remove(aid);
                        }
                        match world.disconnect(&chr_name) {
                            Ok(()) => glog!(
                                "INF",
                                "断线清世界 ChrName:{chr_name} socket={socket} index={index}（FrontEngine.DeleteHuman + WorldEngine 摘除）"
                            ),
                            Err(mir2_world::SessionError::NotInWorld) => glog!(
                                "DBG",
                                "sim: index={index} ChrName={chr_name} 关闭但未在世界（无需摘除）"
                            ),
                            Err(e) => {
                                glog!("WRN", "断线清世界失败 ChrName:{chr_name}：{e:?}");
                            }
                        }
                    }
                }
                GateEvent::LoadDbRequest(info) => {
                    glog!(
                        "INF",
                        "sim: 认证通过 ⇒ 取角色进世界 Account:{} ChrName:{} SessionId:{} GateIdx:{} \
                         （读档控制流在 load.rs；DB 请求属 S2b-2b）",
                        info.account,
                        info.chr_name,
                        info.session_id,
                        info.gate_idx
                    );
                    engine.add_to_load_rcd_list(info);
                }
                GateEvent::RegisterGate { service_id, tx } => {
                    gates.insert(service_id, tx);
                    glog!("DBG", "sim: 登记网关线号 {service_id} 的命令通道");
                }
                GateEvent::UnregisterGate { service_id } => {
                    if gates.remove(&service_id).is_some() {
                        glog!("DBG", "sim: 注销网关线号 {service_id} 的命令通道");
                    }
                }
                GateEvent::KickAccount {
                    account,
                    session_id,
                } => {
                    handle_kick_account(
                        &mut engine,
                        &gates,
                        &online,
                        &mut soft_close_pending,
                        colors,
                        &account,
                        session_id,
                    );
                }
            }
        }
        // 一轮读档：`query` = oracle 的 `QueryCharacterData`（`CharacterDataProcessor.cs:165`）——
        // 已连上 DBSvr ⇒ 发 `DB_LOADHUMANRCD` 请求并返回 query_id；未连上 ⇒ None（照 `IsConnected`，
        // 走 `SendOutConnectMsg`（**oracle 里是"待实现"、不发帧**）+ `CloseUser` 的失败支）。
        // **收/解析**（`LoadPlayerDataPacket` → 取角色 → 进世界）属 S2b-2b 的第二半（已登记）。
        let mut query_fail = false;
        let actions = match &mut db {
            Some(w) => engine.drain_read_storage(|info| {
                if !w.state.is_connected() {
                    return None; // `IsConnected`（`DataQueryServer.cs:60`）
                }
                let qid = w.next_query_id();
                // N1：把该用户的线号一并记下，回复到达后按它路由（见 `route_to_gate`）
                let frame = dbclient::load_human_rcd_request(
                    qid,
                    &info.account,
                    &info.chr_name,
                    &info.ipaddr,
                    info.session_id,
                );
                match w.req_tx.send(frame) {
                    Ok(()) => {
                        w.outstanding.push((
                            qid,
                            info.socket_id,
                            info.gate_idx,
                            info.soft_version_date,
                            info.session_id,
                            info.account.clone(),
                        ));
                        Some(qid)
                    }
                    Err(_) => {
                        query_fail = true;
                        None
                    }
                }
            }),
            None => engine.drain_read_storage(|_| None),
        };
        let _ = query_fail;
        for action in actions {
            match action {
                load::LoadAction::RequeueSaving { account, chr_name } => {
                    glog!(
                        "INF",
                        "角色数据保存中,稍后重新读取. Account:{account} ChrName:{chr_name}"
                    );
                }
                load::LoadAction::StartQuery { .. } => {}
                load::LoadAction::KickNoData {
                    gate_idx,
                    socket_id,
                    account,
                    chr_name,
                } => {
                    glog!(
                        "WRN",
                        "读取用户数据失败,踢出用户. Account:{account} ChrName:{chr_name}"
                    );
                    if let Some(tx) = route_to_gate(&gates, gate_idx, socket_id, "踢") {
                        let _ = tx.send(SimToGate::KickUser { socket: socket_id });
                    }
                }
            }
        }
        // 收侧轮询：已发出且应答已到的查询（`GetDataSrvMessage` `:32-48` 命中即移除）
        if let Some(w) = &mut db {
            let replies = std::sync::Arc::clone(&w.replies);
            let mut r = replies.lock().expect("DB 应答表锁被毒化");
            let mut still = Vec::new();
            for (qid, socket, gate_idx, soft_version_date, session_id, account) in
                std::mem::take(&mut w.outstanding)
            {
                match r.take(qid) {
                    Some(reply) => {
                        let (ident, recog) =
                            dbclient::parse_reply_message(&reply.message).unwrap_or((0, 0));
                        glog!(
                            "INF",
                            "收到角色数据 queryId={qid} Ident={ident} Recog={recog}"
                        );
                        // `PlayerDataService.cs:150`：`nIdent == DBR_LOADHUMANRCD && nRecog == 1 && data.Length > 0`
                        if ident == mir2_protocol::messages::DBR_LOADHUMANRCD && recog == 1 {
                            let payload = mir2_protocol::edcode::decode(&reply.packet);
                            // 头部窗口维持既有控制流（`parse_player_data_head`）；**全字段**
                            // （`parse_player_data`，`openmir2-m2-record-fields`）额外供给入图帧的
                            // 世界/配置层输入（`job`/`game_gold`/`bonus_abil` 等）。两者都解不出才算失败。
                            let head_only = record::parse_player_data_head(&payload);
                            let mut full = record::parse_player_data(&payload);
                            match head_only {
                                Some(head) => {
                                    if full.is_none() {
                                        glog!(
                                            "WRN",
                                            "角色记录全字段解析失败（{} 字节），仅头部窗口可解 ⇒ 入图帧只发有输入的（合成/诊断载荷；真线上 DB 回包恒为全字段）",
                                            payload.len()
                                        );
                                    }
                                    // oracle `WorldServer.cs:248-262`：`MapName = 记录 CurMap`；`HomeMap` 空 ⇒
                                    // `GetHomeInfo` 覆盖 `MapName` 与坐标（`-FixedNewChr` 新角色两条都空）。
                                    // 缺陷现场与确定性口径见 `homemap` 模块头。
                                    // 只有**全字段可解**时才能判定 `HomeMap` 是否为空；仅头部窗口（合成/诊断载荷，
                                    // 真线上 DB 回包恒为全字段）保持记录 `CurMap` + 坐标，不假设为空。
                                    let rec_home = full
                                        .as_ref()
                                        .and_then(|r| r.data.home_map.as_deref())
                                        .unwrap_or("");
                                    let home_empty = full.as_ref().is_some_and(|r| {
                                        r.data.home_map.as_deref().unwrap_or("").is_empty()
                                    });
                                    let (enter_map_name, home_x, home_y, enter_x, enter_y) =
                                        if home_empty {
                                            // oracle `WorldServer.cs:254-259` 的**同序 3 抽**
                                            //（1×`Random(StartPointSize)` + `GetRandHomeX/Y` 各 1×`Random(3)`）：
                                            // 见 `homemap` 模块头的 RNG 抽数账本。同时取出生点**原坐标**
                                            //（`MakeSaveRcd` 要写 `HomeX/HomeY`，`:1257-1258`）。
                                            enter_cfg.home.new_human_spawn_with_home(&mut world.rng)
                                        } else {
                                            let hx = full.as_ref().map_or(0, |r| r.data.home_x);
                                            let hy = full.as_ref().map_or(0, |r| r.data.home_y);
                                            (head.cur_map.clone(), hx, hy, head.cur_x, head.cur_y)
                                        };
                                    // `is_new_human` 是本轮新角色分支的**唯一判据**（oracle `WorldServer.cs:258`
                                    // 的 `if (playObject.Abil.Level == 0)`）；不能拿重算后的 Abil 当门。
                                    let abil_level = full
                                        .as_ref()
                                        .and_then(|r| r.data.abil.as_ref())
                                        .map_or(0, |a| a.level);
                                    let is_new_human = home_empty && abil_level == 0;
                                    // 集成点 ④：`CurMap`（逻辑名）→ `map_id`。有真地图注册表时必须命中；
                                    // 找不到且有非空记录 `HomeMap` 时按 oracle `FindMap==null ⇒ MapName=HomeMap`
                                    // 回落（`WorldServer.cs:319-341`）；仍找不到即拒绝进图并记 WRN
                                    //（`MapRegistry::require_id`），不许静默退回 0 号占位图。无注册表（S1
                                    // `WorldMaps` 占位，启动时已记 WRN）时保留 0 号图。
                                    match mapwire::resolve_enter_map(
                                        registry.as_ref(),
                                        &enter_map_name,
                                        rec_home,
                                    ) {
                                        Err(e) => glog!(
                                            "WRN",
                                            "拒绝进图 ChrName:{} CurMap:{}：{e}（真地图注册表里没有这张图）",
                                            head.chr_name,
                                            enter_map_name
                                        ),
                                        Ok((final_map_name, map_id)) => {
                                        // 准入判据（oracle `WorldServer.cs:503` 的
                                        // `!IsFull && !ProcessHumansIsLogined(chr)`）：
                                        // `ProcessHumansIsLogined`（`:200-217`）= `FrontEngine.InSaveRcdList(chr)`
                                        // **或** `PlayObjectList` 有同名。命中 ⇒ 关新（旧仍在世界时再踢旧）；
                                        // 未命中 ⇒ 放行。夹具实测的两支（A 放行 / B 关新）就是它的竞态。
                                        let in_save = engine.in_save_rcd_list(&head.chr_name);
                                        let in_world = matches!(
                                            world.session(&head.chr_name).map(|s| s.stage),
                                            Some(mir2_world::WorldStage::InWorld)
                                        );
                                        if in_save || in_world {
                                            if in_save {
                                                glog!(
                                                    "INF",
                                                    "角色数据保存中,稍后重新读取. Account:{} ChrName:{}",
                                                    head.account,
                                                    head.chr_name
                                                );
                                            }
                                            handle_duplicate_enter(
                                                &gates,
                                                &mut kick_pending,
                                                &head.chr_name,
                                                gate_idx,
                                                socket,
                                                in_world,
                                            );
                                        } else {
                                        // 进世界：`World::open_session` + `enter_map`
                                        let _ = world.open_session(&head.chr_name);
                                        let ent = mir2_world::Entity::new(
                                            0,
                                            head.chr_name.clone(),
                                            i32::from(enter_x),
                                            i32::from(enter_y),
                                        );
                                        match world.enter_map(&head.chr_name, ent, map_id) {
                                            Ok(id) => {
                                                // 在线登记（`PlayObject.GateIdx`/`SocketId`）：断线、重复进世界
                                                // （踢旧）与账号级顶号都要靠它找回该角色当前的网关连接。
                                                online_set(
                                                    &mut online,
                                                    OnlineEntry {
                                                        account: account.clone(),
                                                        chr_name: head.chr_name.clone(),
                                                        session_id,
                                                        gate_idx,
                                                        socket,
                                                        actor_id: id,
                                                    },
                                                );
                                                glog!(
                                                    "INF",
                                                    concat!(
                                                        "进世界 ChrName:{} CurMap:{} ({},{}) ",
                                                        "Dir:{} entity={}"
                                                    ),
                                                    head.chr_name,
                                                    final_map_name,
                                                    enter_x,
                                                    enter_y,
                                                    head.dir,
                                                    id
                                                );
                                                // S2b-4 续：进世界后按 oracle `PlayObject.Message.cs:1680-1692`
                                                // 的 `RM_LOGON` 支出入图帧，经**该用户的网关线**下发。
                                                // `enter::apply` 装配世界/配置/记录层有输入的帧（51/766/54/11029/1106/
                                                // 5008/811/42/46/50/41/213）；仍缺输入的（52/752 = `RecalcAbilitys`
                                                // 未移植、10 = 世界 tick）保持 `None` 并登记。
                                                let mut inputs = smframe::EnterWorldInputs {
                                                    actor_id: id,
                                                    cur_x: u16::from_ne_bytes(enter_x.to_ne_bytes()),
                                                    cur_y: u16::from_ne_bytes(enter_y.to_ne_bytes()),
                                                    dir: head.dir,
                                                    // 658 `SM_SENDNOTICE`：oracle `PlayObject.SendNotice`
                                                    // 在 `SendLogon` **之前**下发（金标准 #1 在 #2 前），
                                                    // `EnterWorldInputs::frames()` 的序已把它放第一位。
                                                    notice: notice.clone(),
                                                    map_file_name: Some(
                                                        final_map_name.as_bytes().to_vec(),
                                                    ),
                                                    ..smframe::EnterWorldInputs::default()
                                                };
                                                // oracle `WorldServer.cs:262-275` 的新角色 `Abil` 初始化 +
                                                // `HomeMap/HomeX/HomeY/MapName/CurrX/CurrY` 落记录：
                                                // 随后的 `RecalcLevelAbilitys()/RecalcAbilitys()`（本宿主在
                                                // `enter::recalc_for_session` 里）按 `Job` 覆盖等级派生字段。
                                                if is_new_human {
                                                    if let Some(r) = full.as_mut() {
                                                        r.data.home_map =
                                                            Some(enter_map_name.clone());
                                                        r.data.home_x = home_x;
                                                        r.data.home_y = home_y;
                                                        r.data.cur_map =
                                                            Some(final_map_name.clone());
                                                        r.data.cur_x = enter_x;
                                                        r.data.cur_y = enter_y;
                                                        r.data.abil = Some(new_human_ability());
                                                    }
                                                }
                                                let data = full.as_ref().map(|r| &r.data);
                                                // 帧 42 的展示名输入（`GetShowName()`）：成员/师徒/婚姻/转生/职业
                                                // 全取角色记录；行会在本骨架恒空（`MyGuild == null`，见 `enter::show_name`）。
                                                let show_name = enter::ShowNameInputs {
                                                    chr_name: &head.chr_name,
                                                    re_level: data.map_or(0, |d| d.re_level),
                                                    job: data.map_or(0, |d| d.job),
                                                    sex: data.map_or(0, |d| d.sex),
                                                    dear_name: data
                                                        .and_then(|d| d.dear_name.as_deref())
                                                        .unwrap_or(""),
                                                    master_name: data
                                                        .and_then(|d| d.master_name.as_deref())
                                                        .unwrap_or(""),
                                                    is_master: data.is_some_and(|d| d.is_master),
                                                    show_rank_level_name: enter_cfg
                                                        .show_rank_level_name,
                                                };
                                                // 帧 50/41 的 `GetFeatureToLong()` 两段输入（Dress/Weapon 的
                                                // `StdItem.Shape`）：从启动时读入的 `stditems` 表 1-based 查
                                                // （`GameItemSystem.GetStdItem`）；查不到（越界/空表）计 0
                                                // （oracle `GetStdItem == null` 分支，见 `enter::feature_shapes`）。
                                                let (dress_shape, weapon_shape, shape_warns) =
                                                    enter::feature_shapes(
                                                        data.and_then(|d| d.hum_items.as_deref()),
                                                        &enter_cfg.catalog,
                                                    );
                                                for w in shape_warns {
                                                    glog!("WRN", "入图帧输入：{w}");
                                                }
                                                // 帧 50/41/213 的输入（`PlayObject.SendLogon()`，
                                                // `PlayObject.cs:1128-1147`）：同组连发三条，缺则三条一起跳过。
                                                let logon = data.map(|d| enter::LogonInputs {
                                                    feature: enter::FeatureInputs {
                                                        dress_shape,
                                                        weapon_shape,
                                                        gender: d.sex,
                                                        hair: d.hair,
                                                        // `CharacterObject.DressEffType`/`HorseType` 在 oracle
                                                        // 全仓无赋值点（`feature.rs` 模块文档已记）⇒ 0。
                                                        on_horse: false,
                                                        horse_type: 0,
                                                        dress_eff_type: 0,
                                                    },
                                                    // `GetCharStatus()` = `ActorBuffSystem.GetBuffStatus()
                                                    // | (CharStatusEx & 0xFFFF)`；会话初生无 buff、`CharStatusEx=0`
                                                    // （`BaseObject.cs:393`）⇒ 0。buff 系统未移植 ⇒ 记 0 并登记
                                                    // （金标准 #3 的 `CharStatus == 0` 印证）。
                                                    char_status: 0,
                                                    // `WorldServer.cs:1146`：`AllowGroup = 记录 allow_group > 0`。
                                                    allow_group: d.allow_group > 0,
                                                    // `WorldServer.cs:1147`：`AttatckMode = (AttackMode)记录 attatck_mode`。
                                                    attack_mode: d.attatck_mode,
                                                });
                                                // 帧 52/752 的输入（`AbilityInputs`）：`wx` 与下面登记的地面会话态
                                                // 共用同一份（`AbilityInputs` 是 `Copy`），保证只有一份 `RecalcAbilitys`
                                                // 实现（`enter::recalc_for_session`）。
                                                let ability_inputs: Option<enter::AbilityInputs> =
                                                    data.and_then(|d| {
                                                        let conf = enter_cfg.game_conf.as_ref()?;
                                                        let abil = d.abil.as_ref()?;
                                                        let bonus_abil = d.bonus_abil.as_ref()?;
                                                        let (hit, speed) =
                                                            enter::bonus_tick(conf, d.job);
                                                        Some(enter::AbilityInputs {
                                                            catalog: &enter_cfg.catalog,
                                                            abil,
                                                            bonus_abil,
                                                            bonus_tick_hit: hit,
                                                            bonus_tick_speed: speed,
                                                            use_items: d.hum_items.as_deref(),
                                                            bag_items: d.bag_items.as_deref(),
                                                            gold: d.gold,
                                                        })
                                                    });
                                                let wx = enter::WorldFrameInputs {
                                                    flag: registry
                                                        .as_ref()
                                                        .and_then(|r| r.flag_of(map_id)),
                                                    map_desc: registry
                                                        .as_ref()
                                                        .and_then(|r| r.map_desc_of(map_id)),
                                                    conf: enter_cfg.game_conf.as_ref(),
                                                    bonus_abil: data
                                                        .and_then(|d| d.bonus_abil.as_ref()),
                                                    soft_version_date,
                                                    game_gold: data.map_or(0, |d| d.game_gold),
                                                    game_point: data.map_or(0, |d| d.game_point),
                                                    job: data.map_or(0, |d| d.job),
                                                    bonus_point: data.map_or(0, |d| d.bonus_point),
                                                    // `Bright = M2Share.GameTime`：oracle 由 `SystemProcessor`
                                                    // 每 50ms 按真实小时刷新，登录时读到的就是该桶值。
                                                    game_time: Some(enter::game_time_now()),
                                                    strings: Some(&enter_cfg.string_conf),
                                                    show_name: Some(show_name),
                                                    logon,
                                                    // 帧 52/752：角色记录 `Abil`/`BonusAbil` + `stditems`
                                                    // 目录 + 各职业 `Config.BonusAbilofXxx` 除数（见上面 hoist）。
                                                    ability: ability_inputs,
                                                };
                                                for w in enter::apply(&mut inputs, &wx) {
                                                    glog!("WRN", "入图帧输入：{w}");
                                                }
                                                // 地面交互会话态（`CM_PICKUP`/`CM_DROPITEM`，owner §26）：进世界时
                                                // 从同一份 `CharacterData` + `recalc_for_session`（**与帧 52/752
                                                // 同一实现**）建立；`sys_msg` = `String.conf` 的 `CanotPickUpItem`
                                                // GBK 原文（oracle `MessageSettings.CanotPickUpItem`）。
                                                if let (Some(d), Some(a)) = (data, ability_inputs.as_ref()) {
                                                    let out = enter::recalc_for_session(a, d.job);
                                                    // M3②③ k=2 收口：同一份 `RecalcAbilitys` 输出**同时**写进世界实体
                                                    // （此前只喂 52/752 帧，实体属性仍是 `Entity::new` 的空值）。
                                                    if let Some(e) = world.entities.get_mut(id) {
                                                        apply_ability_to_entity(e, &out.wabil, &out.sub);
                                                    } else {
                                                        glog!(
                                                            "WRN",
                                                            "进世界后找不到 actor_id={id} 的实体 ⇒ 攻击/命中属性未装载（登记）"
                                                        );
                                                    }
                                                    ground_sessions.insert(
                                                        head.chr_name.clone(),
                                                        SessionGround {
                                                            state: game_svr::ground::GroundState {
                                                                item_list: bag_items_from(
                                                                    d.bag_items.as_deref(),
                                                                ),
                                                                gold: d.gold,
                                                                wabil: out.wabil,
                                                            },
                                                            actor_id: id,
                                                            game_gold: d.game_gold,
                                                            sys_msg: Some(enter::gbk(
                                                                enter_cfg.string_conf.canot_pick_up_item(),
                                                            )),
                                                            // `PlayObject.UseItems`（13 槽，保位）。
                                                            use_items: use_items_from(
                                                                d.hum_items.as_deref(),
                                                            ),
                                                            // `RecalcAbilitys` 的 **prior**：登录时首次重算的
                                                            // `SubAbility`（后续装备/卸下重算在此之上累加，T-18）。
                                                            sub: out.sub,
                                                            equip: EquipInputs {
                                                                job: d.job,
                                                                gender: d.sex,
                                                                hair: d.hair,
                                                                re_level: d.re_level,
                                                                credit_point: u16::from(d.credit_point),
                                                                abil: a.abil.clone(),
                                                                bonus_abil: a.bonus_abil.clone(),
                                                                bonus_tick_hit: a.bonus_tick_hit,
                                                                bonus_tick_speed: a.bonus_tick_speed,
                                                            },
                                                            recovery: game_svr::recover::Recovery {
                                                                inc_health: u16::from(d.inc_health),
                                                                inc_spell: u16::from(d.inc_spell),
                                                                // `CharacterObject()` 初值：Per*=5、
                                                                // tick = GetTickCount()（`CharacterObject.cs:83-86`）。
                                                                ..game_svr::recover::Recovery::new(now_ms())
                                                            },
                                                            // 体力/魔力点（`BaseObject.cs:445-448`：HealthTick/SpellTick=0、
                                                            // AutoRecoveryTick=GetTickCount()）。
                                                            stamina: game_svr::stamina::Stamina::new(now_ms()),
                                                            // `ShowHp` 由技能「心灵启示」置位（未移植）⇒ 初生 false。
                                                            show_hp: false,
                                                            // `UserUnLockDurg` 不在角色记录里（会话初生 false；
                                                            // 与 `CM_EAT` 的 Shape 2 同一状态变）：登记为未建模。
                                                            user_unlock_durg: false,
                                                            hunger_status: d.hunger_status,
                                                            query_bag_items_tick: None,
                                                            // `PlayObject.SaveRcdTick`（`PlayObject.cs:886`）：
                                                            // 进世界时起算。oracle 在 `new PlayObject()` 里取
                                                            // `GetTickCount()`（每次登入都新建对象，`WorldServer.cs:234`），
                                                            // 与本宿主「`SessionGround` 随进世界建」同生命周期。
                                                            save_rcd_tick: now_ms(),
                                                        },
                                                    );
                                                    // 存档用：留下原始记录 + 进图逻辑名（断线/`MakeGhost` 时用它合成）。
                                                    // 新角色的 `Abil/HomeMap/坐标` 已在上面按 oracle 落进 `full`。
                                                    if let Some(r) = full.as_ref() {
                                                        saves.insert(
                                                            head.chr_name.clone(),
                                                            SavedHuman {
                                                                account: account.clone(),
                                                                record: r.clone(),
                                                                enter_map: final_map_name.clone(),
                                                            },
                                                        );
                                                    }
                                                    // oracle `PlayObject.cs:1205-1245` 的 `IsNewHuman` 四件新手物品
                                                    // （`IsNewHuman` 由 `WorldServer.cs:258-277` 在 `HomeMap` 空且
                                                    // `Abil.Level==0` 时置位）。
                                                    glog!(
                                                        "INF",
                                                        "IsNewHuman 判定 ChrName={} home_empty={home_empty} 记录 Abil.Level={abil_level} is_new_human={is_new_human} bag_items={}",
                                                        head.chr_name,
                                                        d.bag_items.as_ref().map_or(0, Vec::len)
                                                    );
                                                    if is_new_human {
                                                        if let Some(sg) =
                                                            ground_sessions.get_mut(&head.chr_name)
                                                        {
                                                            for w in add_newbie_items(
                                                                &mut sg.state.item_list,
                                                                &enter_cfg.catalog,
                                                                &enter_cfg.newbie,
                                                                d.sex,
                                                                &mut item_number,
                                                            ) {
                                                                glog!(
                                                                    "WRN",
                                                                    "{w}（照 oracle CopyToUserItemFromName 失败分支，不入包）"
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                                // S3 后半段：把 oracle 在 `SendLogon` 同轮算出的
                                                // **展示层**字段登记进同屏下行表（`GetFeatureToLong` /
                                                // `GetCharStatus` / `GetMyLight` / `GetShowName`）。
                                                // `Dir` 取记录值（`ClientChangeDir` 会改它——属移动接线，登记为下一格）。
                                                let light_pairs: Vec<(u16, u16)> = data
                                                    .and_then(|d| d.hum_items.as_deref())
                                                    .unwrap_or(&[])
                                                    .iter()
                                                    .take(downlink::USE_ITEM_SLOTS)
                                                    .map(|cell| {
                                                        cell.as_ref().map_or((0, 0), |it| (it.index, it.dura))
                                                    })
                                                    .collect();
                                                let light = downlink::my_light(
                                                    data.and_then(|d| d.abil.as_ref())
                                                        .map_or(0, |a| a.level),
                                                    &light_pairs,
                                                    &enter_cfg.catalog,
                                                );
                                                if let Some(wl) = inputs.logon_body_wl {
                                                    let show_bytes = wx.show_name.as_ref().map_or_else(
                                                        Vec::new,
                                                        |sn| {
                                                            enter::gbk(&enter::show_name(
                                                                &enter_cfg.string_conf,
                                                                sn,
                                                            ))
                                                        },
                                                    );
                                                    displays.set(
                                                        id,
                                                        downlink::ActorDisplay::new(
                                                            inputs.dir,
                                                            light,
                                                            wl.0,
                                                            inputs.feature.map_or(0, |(_, fe)| fe),
                                                            wl.1,
                                                            enter::PLAY_NAME_COLOR,
                                                            show_bytes,
                                                        ),
                                                    );
                                                    glog!(
                                                        "DBG",
                                                        "同屏展示表登记：actor={id} dir={} light={light} feature={}",
                                                        inputs.dir,
                                                        wl.0
                                                    );
                                                }
                                                // N1（reviewer-3）：按该用户的 gate_idx 路由回**它自己那条**网关线
                                                // （与上面 KickNoData 的 `route_to_gate` 完全同构）；找不到线号记 WRN，
                                                // 绝不 `gates.values().next()` 静默挑一个。
                                                if let Some(tx) = route_to_gate(
                                                    &gates,
                                                    gate_idx,
                                                    socket,
                                                    "标记进世界/下发入图帧",
                                                ) {
                                                    let bodies = inputs.frames();
                                                    glog!(
                                                        "INF",
                                                        "进世界帧下发：socket={socket} 共 {} 条（输入齐备的）",
                                                        bodies.len()
                                                    );
                                                    for body in bodies {
                                                        let _ = tx.send(SimToGate::SendFrame {
                                                            socket,
                                                            body,
                                                        });
                                                    }
                                                    let _ = tx.send(SimToGate::MarkInWorld { socket });
                                                }
                                            }
                                            // oracle `WorldServer.cs:503` 的准入判据：
                                            // `AlreadyInWorld` 在加了上面的预检后基本不可达（`in_world` 已挡在前面）；
                                            // 保留为安全网：`enter_map` 报此错 ⇒ 旧必在世界 ⇒ `kick_old = true`。
                                            Err(mir2_world::SessionError::AlreadyInWorld) => {
                                                handle_duplicate_enter(
                                                    &gates,
                                                    &mut kick_pending,
                                                    &head.chr_name,
                                                    gate_idx,
                                                    socket,
                                                    true,
                                                );
                                            }
                                            Err(e) => glog!(
                                                "WRN",
                                                "进世界失败 ChrName:{}：{e:?}",
                                                head.chr_name
                                            ),
                                        }
                                        }
                                        }
                                    }
                                }
                                None => {
                                    glog!(
                                        "WRN",
                                        "角色记录头部窗口解析失败（{} 字节；`LoadPlayerDataPacket`）",
                                        payload.len()
                                    )
                                }
                            }
                        }
                    }
                    None => still.push((
                        qid,
                        socket,
                        gate_idx,
                        soft_version_date,
                        session_id,
                        account,
                    )),
                }
            }
            // 存档应答（`DBR_SAVEHUMANRCD(1102)` / 失败的 `DBR_LOADHUMANRCD(1100)`）：取走并记日志。
            // 只有成功应答才复位 in-save（`apply_save_reply`，照 oracle `ProcessSaveQueue`）。
            let mut still_saves = Vec::new();
            for (qid, chr) in std::mem::take(&mut w.save_outstanding) {
                match r.take(qid) {
                    Some(reply) => {
                        let (ident, recog) =
                            dbclient::parse_reply_message(&reply.message).unwrap_or((0, 0));
                        glog!(
                            "INF",
                            "存档应答 queryId={qid} ChrName={chr} Ident={ident} Recog={recog}"
                        );
                        // 仅 `1102 && recog==1` 才 `end_save`；`1100 recog=0`（存库失败）保持
                        // in-save（oracle 会把角色永久留在保存名单——连 bug 一起搬）。
                        apply_save_reply(&mut engine, &chr, ident, recog);
                    }
                    None => still_saves.push((qid, chr)),
                }
            }
            w.save_outstanding = still_saves;
            w.outstanding = still;
        }
        // 踢旧落地（oracle `RunNotice` 的 `BoKickFlag` 支）：oracle 是"置标志 → 下一 tick 的
        // RunNotice 落地"（≥1 tick）；本骨架在事件 drain 之后的**同一 tick 后段**落地，帧序不变。
        drain_kicks(
            &mut world,
            &gates,
            &mut online,
            &mut kick_pending,
            &mut db,
            &mut engine,
            &mut saves,
            &mut ground_sessions,
            registry.as_ref(),
        );
        // 账号级顶号落地（`BoSoftClose` 支）：提示帧已即时发出，这里只离开世界 + 清槽。
        // 同一 tick 后段落地（oracle ≥1 tick）——见 docs §4 的 O-1 登记。
        drain_soft_close(
            &mut world,
            &gates,
            &mut online,
            &mut soft_close_pending,
            &mut db,
            &mut engine,
            &mut saves,
            &mut ground_sessions,
            registry.as_ref(),
        );
        // **M3④-实现 B 第 ③ 片**：MonGen regen 门 + 刷怪（单线程串行版的 `ProcessMonsters`，
        // `WorldServer.MonGen.cs:192-253`）。`dir`/`cool_eye`/构造抽已随生成结果写进
        // `Entity.ai`（卡 C 第 ② 片）；**怪物帧面**（`SM_TURN`/`SM_FEATURECHANGED`）仍属后续分片。
        if let Some(host) = mon_gen_host.as_mut() {
            if let Some(reg) = registry.as_ref() {
                step_mon_gen(
                    host,
                    &enter_cfg.mon_gen.table,
                    reg,
                    &mut world,
                    i32::try_from(online.len()).unwrap_or(i32::MAX),
                );
            }
        }
        let rec = world.tick();
        ticks += 1;
        // T-28：周期恢复 tick（oracle `PlayObject.Run` 的 `:70-141`）——同一 world tick 后处理，
        // 把 `IncHealth/IncSpell/IncHealing` 按 oracle 节奏消费回 `WAbil.HP/MP` 并回帧。
        drain_recovery(
            &mut ground_sessions,
            &world,
            &online,
            &gates,
            enter_cfg.ref_msg_range,
        );
        // 体力/魔力点段（oracle `BaseObject.Run()` `:26-109`）：同一 `Run()` 里紧接
        // `PlayObject` 的恢复段之后；命中会回写 `Entity.hp` ⇒ 必须在周期存档之前。
        drain_stamina(
            &mut ground_sessions,
            &mut world,
            &online,
            &gates,
            enter_cfg.ref_msg_range,
            enter_cfg.health_fill_time,
            enter_cfg.spell_fill_time,
        );
        // 周期存档（oracle `WorldServer.cs:698-703`）：oracle 在同一 `Run()` 之后查，
        // 这里紧跟 [`drain_recovery`]（保证存档体读到的是本 tick 恢复后的 `WAbil`）。
        let _ = drain_periodic_save(
            &mut ground_sessions,
            &saves,
            &world,
            &mut db,
            &mut engine,
            registry.as_ref(),
            enter_cfg.save_human_rcd_time,
            now_ms(),
        );
        // `openmir2-m3-attack-magic-wiring`（第一片）：本 tick 的攻击结算 → `SM_STRUCK`（受击者自见）
        // + `SM_NOWDEATH`（死亡广播）。
        drain_combat_downlink(
            &rec,
            &world,
            &displays,
            &online,
            &mut ground_sessions,
            &gates,
        );
        // S3 后半段（第一格）：世界 tick 的 AOI 结果 → `SM_TURN` + `SM_FEATURECHANGED` 回帧。
        drain_aoi_downlink(&world, &displays, &online, &gates);
        if ticks.is_multiple_of(50) {
            glog!(
                "DBG",
                "sim: tick #{} clock={}ms 处理 {} 项（本地第 {ticks} 次）",
                rec.tick_seq,
                rec.clock_ms,
                rec.processed.len()
            );
        }
        if let Some(d) = next.checked_duration_since(std::time::Instant::now()) {
            std::thread::sleep(d);
        }
    }
}

/// `_gameGates = new ChannelMessageHandler[20]`（`TCPNetChannel.cs`）。
const GATE_SLOT_CAPACITY: usize = 20;

/// 网关连接的**线号分配器**（oracle `TCPNetChannel.AllocGateSlot`：复用最小空闲槽、
/// 上限 20；断线释放）。
///
/// 线号就是世界侧寻址用的 `GateIdx`（`playObject.GateIdx` = 网关在客户端认证串里声明的 ServiceId），
/// 因此同一时刻每个连接必须有**唯一**线号，否则进世界帧会路由错连接。
#[derive(Default)]
struct GateSlots {
    used: Vec<bool>,
}

impl GateSlots {
    /// 分配最小空闲槽（0..）。满 [`GATE_SLOT_CAPACITY`] ⇒ `None`（oracle `gateIdx < 0` 的拒绝支）。
    fn alloc(&mut self) -> Option<u8> {
        if let Some(i) = self.used.iter().position(|&u| !u) {
            self.used[i] = true;
            return u8::try_from(i).ok();
        }
        if self.used.len() >= GATE_SLOT_CAPACITY {
            return None;
        }
        self.used.push(true);
        u8::try_from(self.used.len() - 1).ok()
    }

    /// 断线释放（下标越界无操作）。
    fn free(&mut self, idx: u8) {
        if let Some(slot) = self.used.get_mut(usize::from(idx)) {
            *slot = false;
        }
    }
}

/// 一条网关连接：20B 头 + body 的收发与 `GM_*` 分派（照 `ChannelMessageHandler.cs:146-230`）。
async fn gate_conn(
    sock: TcpStream,
    ev_tx: std::sync::mpsc::Sender<GateEvent>,
    check_block: usize,
    auth: Arc<std::sync::Mutex<auth::AdmissionStore>>,
    service_id: u8,
) -> Result<()> {
    let (mut rd, wr) = sock.into_split();
    let tx = Arc::new(Mutex::new(wr));
    // sim → gate：世界/读档流程要清某个用户的槽时走这条（`NetChannel.CloseUser(gateIdx, socket)`）
    let (sim_tx, mut sim_rx) = mpsc::unbounded_channel::<SimToGate>();
    let _ = ev_tx.send(GateEvent::RegisterGate {
        service_id,
        tx: sim_tx,
    });
    let mut users = UserList::new();
    let mut state = KeepAlive {
        send_keep_alive: false,
        ..KeepAlive::default()
    };
    loop {
        let mut head = [0u8; ServerMessage::SIZE];
        // sim 侧的命令（清槽等）**不能等下一次入站帧**才处理：oracle 的 `CloseUser` 是调用线程直接清
        // ⇒ 这里用 `select!` 让"下一帧的头"与"sim 命令"任意一个就绪都能推进。
        let head_res = tokio::select! {
            r = rd.read_exact(&mut head) => r,
            Some(cmd) = sim_rx.recv() => {
                if let Err(e) = apply_sim_cmd_async(cmd, &mut users, &tx, &mut state, check_block).await {
                    glog!("WRN", "sim→gate 命令发送失败：{e:#}");
                }
                continue;
            }
        };
        if head_res.is_err() {
            glog!("ERR", "网关连接断开。");
            return Ok(());
        }
        let Some(msg) = ServerMessage::from_bytes(&head) else {
            continue;
        };
        if msg.packet_code != grobal2::PacketCode {
            continue; // `:122-125` 非本协议包直接丢
        }
        // F5（reviewer-2）：`:127-134` 只在 `PackLength > 0` 时当有载荷，否则按 `(null, 0)` 处理
        let body_len = if msg.pack_length > 0 {
            usize::try_from(msg.pack_length).unwrap_or(0)
        } else {
            0
        };
        let mut body = vec![0u8; body_len];
        if body_len > 0 && rd.read_exact(&mut body).await.is_err() {
            glog!("ERR", "读取网关载荷失败。");
            return Ok(());
        }
        dispatch(
            &msg,
            &body,
            &mut users,
            &mut state,
            &tx,
            &ev_tx,
            check_block,
            &auth,
        )
        .await?;
        // 处理与本帧同批到达的 sim 命令（清槽/入图发包等）
        while let Ok(cmd) = sim_rx.try_recv() {
            if let Err(e) = apply_sim_cmd_async(cmd, &mut users, &tx, &mut state, check_block).await
            {
                glog!("WRN", "sim→gate 命令发送失败：{e:#}");
            }
        }
    }
}

/// 施放一条 sim → gate 的**槽操作**（`NetChannel.CloseUser(gateIdx, socket)` 的落地）。
///
/// `SendFrame` 是异步发包，由 [`apply_sim_cmd_async`] 处理；本函数只做纯槽表动作
/// （单测直接调它，不必起 tokio 运行时）。
fn apply_sim_cmd(cmd: SimToGate, users: &mut UserList) {
    match cmd {
        SimToGate::MarkInWorld { socket } => {
            if users.mark_in_world(socket) {
                glog!("INF", "进世界标记：socket={socket}");
            }
        }
        SimToGate::KickUser { socket } => {
            if let Some(index) = users.close_by_socket(socket) {
                glog!(
                    "WRN",
                    "按世界侧要求清槽：socket={socket} index={index} 在线={} 槽={}",
                    users.online(),
                    users.slots()
                );
            }
        }
        // 异步发包，不在纯槽操作里处理。
        SimToGate::SendFrame { .. } | SimToGate::SendRaw { .. } => {}
    }
}

/// sim → gate 命令的异步落地：`SendFrame` 按 oracle `PlayObject.Socket.cs:101-116` 拼
/// `GM_DATA`（20B 头 + 12B 命令头 + 尾）并经 [`send_gate_msg`]（= `ProcessBufferSend` 规则）上路；
/// 其余命令走纯槽操作。
async fn apply_sim_cmd_async(
    cmd: SimToGate,
    users: &mut UserList,
    tx: &Arc<Mutex<OwnedWriteHalf>>,
    state: &mut KeepAlive,
    check_block: usize,
) -> Result<()> {
    match cmd {
        SimToGate::SendFrame { socket, body } => {
            // oracle `messageHead.Socket = SocketId` / `SessionId = SocketIdx`（`PlayObject.Socket.cs:104-115`）；
            // 按 socket 找回该槽（`SessionIndex` 在出包方向上 oracle 不设，保持 0）。
            let Some(u) = users.find(0, socket) else {
                glog!(
                    "WRN",
                    "下发 SM_* 帧时 socket={socket} 的槽不存在，丢弃 {} 字节",
                    body.len()
                );
                return Ok(());
            };
            let head = ServerMessage {
                packet_code: grobal2::PacketCode,
                socket: u.socket,
                session_id: u.session_id,
                ident: u16::from(grobal2::GM_DATA),
                session_index: 0,
                pack_length: i32::try_from(body.len()).unwrap_or(0),
            };
            let mut wire = head.to_bytes().to_vec();
            wire.extend_from_slice(&body);
            send_gate_msg(tx, state, check_block, &wire).await
        }
        SimToGate::SendRaw { socket, payload } => {
            apply_sim_send_raw(socket, &payload, users, tx, state, check_block).await
        }
        other => {
            apply_sim_cmd(other, users);
            Ok(())
        }
    }
}

/// `SendRaw`（`SendSocket(string)` 的短包支）：`PackLength = -len`，网关端产出 `'#' + 正文 + '!'`。
async fn apply_sim_send_raw(
    socket: i32,
    payload: &[u8],
    users: &mut UserList,
    tx: &Arc<Mutex<OwnedWriteHalf>>,
    state: &mut KeepAlive,
    check_block: usize,
) -> Result<()> {
    // `PlayObject.Socket.cs:63-64`：`Socket = SocketId`、`SessionId = SocketIdx`。
    let Some(u) = users.find(0, socket) else {
        glog!(
            "WRN",
            "下发短包时 socket={socket} 的槽不存在，丢弃 {} 字节",
            payload.len()
        );
        return Ok(());
    };
    let head = ServerMessage {
        packet_code: grobal2::PacketCode,
        socket: u.socket,
        session_id: u.session_id,
        ident: u16::from(grobal2::GM_DATA),
        session_index: 0,
        pack_length: -i32::try_from(payload.len()).unwrap_or(i32::MAX),
    };
    let mut wire = head.to_bytes().to_vec();
    wire.extend_from_slice(payload);
    send_gate_msg(tx, state, check_block, &wire).await
}

/// 保活/阻塞计数（C# `GateInfo.SendKeepAlive/SendChecked/SendBlockCount/SendCheckTick`）。
#[derive(Debug, Default)]
struct KeepAlive {
    send_keep_alive: bool,
    send_checked: i32,
    send_block_count: usize,
    /// `SendCheckTick`：`send_decision` 读它判超时（S2b-4 后 `SM_*` 入图帧已走生产路径 ⇒
    /// 整套阻塞/确认逻辑不再是纸面代码）。
    send_check_tick: i64,
}

/// `M2Share.SocCheckTimeOut`（`ChannelMessageHandler.cs:60` 比较用的那个静态）。
///
/// **是字面量 50**：`src/M2Server/M2Share.cs:180` 的静态默认值就是 50，而且它**没有**被配置赋值
/// （`ServerConf.cs:60` 只写 `SystemShare.SocCheckTimeOut`，那是另一个静态；现网 `Server.conf:48`
/// `SocCheckTimeOut=50` 恰好也是 50，容易误以为是"从配置读 2000"——原注释抄了 `:60` 行尾那句过时的
/// `// 2 * 1000`，reviewer-2 的 F2 已指出）。
const SOC_CHECK_TIMEOUT_MS: i64 = 50;

/// 当前毫秒（C# 用 `HUtil32.GetTickCount`；这里只有"差值/超时"参与判定）。
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// 发送口的判定结果（`ProcessBufferSend` `:58-90` 的纯函数版，便于单测）。
#[derive(Debug, PartialEq, Eq)]
enum SendDecision {
    /// `SendChecked > 0`（等网关回 `GM_RECEIVE_OK`）：**包括刚超时清零的那一次也压住不发**（`:58-66` 的无条件 `return`）
    Hold,
    /// 累积到阈值 ⇒ 先发一条 `GM_RECEIVE_OK` 再发正文（`:74-83`）
    CheckFirst,
    /// 直接发（`:84-90`）
    Send,
    /// 单包就超阈值 ⇒ 丢弃（`:76-79`）
    Drop,
}

/// 判定的纯函数（副作用只有超时清零，照 `:58-66`）。
fn send_decision(state: &mut KeepAlive, check_block: usize, len: usize, now: i64) -> SendDecision {
    if state.send_checked > 0 {
        if now - state.send_check_tick > SOC_CHECK_TIMEOUT_MS {
            state.send_checked = 0;
            state.send_block_count = 0;
        }
        // 注意：oracle 这里是**无条件 return**（`:65`）——刚清零的那一包也要丢
        return SendDecision::Hold;
    }
    let threshold = check_block.saturating_mul(10);
    if state.send_block_count + len >= threshold {
        if state.send_block_count == 0 && threshold <= len {
            return SendDecision::Drop; // `:76-79`
        }
        return SendDecision::CheckFirst;
    }
    SendDecision::Send
}

/// 网关发送口的 oracle 规则（`ChannelMessageHandler.cs:50-95` 的 `ProcessBufferSend`）：
/// ① `SendChecked > 0`（等网关回 `GM_RECEIVE_OK`）时**不发**；超过 `SocCheckTimeOut`(50ms) 清零后**仍 Hold**
///    （oracle `:58-66` 无条件 `return`，刚清零的那一包也丢）；
/// ② 累积 `SendBlockCount + len >= CheckBlock*10` ⇒ 先发一条 `GM_RECEIVE_OK` 并置 `SendChecked=1`（`:74-83`）；
///    若 `SendBlockCount == 0 && CheckBlock*10 <= len` ⇒ **直接丢**（`:76-79`）；
/// ③ 发送后 `SendBlockCount += len`（`:89`）。
async fn send_gate_msg(
    tx: &Arc<Mutex<OwnedWriteHalf>>,
    state: &mut KeepAlive,
    check_block: usize,
    wire: &[u8],
) -> Result<()> {
    match send_decision(state, check_block, wire.len(), now_ms()) {
        SendDecision::Hold | SendDecision::Drop => return Ok(()),
        SendDecision::CheckFirst => {}
        SendDecision::Send => {}
    }
    if state.send_checked == 0
        && state.send_block_count + wire.len() >= check_block.saturating_mul(10)
    {
        let check = ServerMessage {
            packet_code: grobal2::PacketCode,
            socket: 0,
            session_id: 0,
            ident: u16::from(grobal2::GM_RECEIVE_OK),
            session_index: 0,
            pack_length: 0,
        };
        send_raw(tx, check.to_bytes().as_slice()).await?;
        state.send_checked = 1;
        state.send_check_tick = now_ms();
    }
    send_raw(tx, wire).await?;
    state.send_block_count += wire.len();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn dispatch(
    msg: &ServerMessage,
    body: &[u8],
    users: &mut UserList,
    state: &mut KeepAlive,
    tx: &Arc<Mutex<OwnedWriteHalf>>,
    ev_tx: &std::sync::mpsc::Sender<GateEvent>,
    check_block: usize,
    auth: &Arc<std::sync::Mutex<auth::AdmissionStore>>,
) -> Result<()> {
    match u8::try_from(msg.ident).unwrap_or(0) {
        grobal2::GM_OPEN => {
            // `:154-159`：体 = IP 字符串；登记后回 `GM_SERVERUSERINDEX`（索引 = 下标+1）
            let ip = gbk_decode(body);
            let index = users.open(msg.socket, msg.session_id, ip.clone());
            let reply = ServerMessage {
                packet_code: grobal2::PacketCode,
                socket: msg.socket,
                session_id: msg.session_id,
                ident: u16::from(grobal2::GM_SERVERUSERINDEX),
                session_index: index,
                pack_length: 0,
            };
            // F1（reviewer-2）：oracle 的 `SendNewUserMsg` 是**直发** `socket.Send(data)`（`:461`），
            // 完全不经过 `:50-95` 的 `ProcessBufferSend` ⇒ 这里也必须直发（不计 SendBlockCount、不受 SendChecked 拦截）
            send_raw(tx, reply.to_bytes().as_slice()).await?;
            // 该分支（GM_OPEN）无其它出站载荷；`SM_*` 出包走 `apply_sim_cmd_async` → `send_gate_msg`。
            let _ = (state, check_block);
            glog!(
                "INF",
                "新用户链接... socket={} SessionId={} index={} ip={ip} 在线={} 槽={}",
                msg.socket,
                msg.session_id,
                index,
                users.online(),
                users.slots()
            );
            let _ = ev_tx.send(GateEvent::Opened { index, ip });
        }
        grobal2::GM_CLOSE => {
            // `:160-162`：**按 socket** 关。角色名要先读（`close_by_socket` 会把槽置空）。
            let chr_name = users.chr_name_by_socket(msg.socket);
            if let Some(index) = users.close_by_socket(msg.socket) {
                glog!(
                    "INF",
                    "用户关闭：socket={} index={index} 在线={} 槽={}",
                    msg.socket,
                    users.online(),
                    users.slots()
                );
                let _ = ev_tx.send(GateEvent::Closed {
                    index,
                    chr_name,
                    socket: msg.socket,
                });
            }
        }
        grobal2::GM_CHECKCLIENT => {
            // `:163-165`：只置标志。**消费者在本进程**：`src/M2Server/Net/TCPNetChannel.cs:435-439`
            // 见 `SendKeepAlive` 就发 `SendCheck(GM_CHECKSERVER)`；网关侧只是反向（`GameGate/Services/ClientThread.cs:444`
            // 发 `GM_CHECKCLIENT`、`:268-271` 收 `GM_CHECKSERVER`）。本切片不做这个定时器（S2b-3 登记）。
            state.send_keep_alive = true;
            glog!("DBG", "GM_CHECKCLIENT：置保活标志（消费者是 M2Server/Net/TCPNetChannel.cs:435-439，属 S2b-3）");
        }
        grobal2::GM_RECEIVE_OK => {
            // `:166-169`：清零阻塞计数
            state.send_checked = 0;
            state.send_block_count = 0;
        }
        grobal2::GM_DATA => {
            let Some(u) = users.find(msg.session_index, msg.socket) else {
                glog!(
                    "WRN",
                    "SessionIndex={} socket={} 的用户不存在，丢弃 {} 字节",
                    msg.session_index,
                    msg.socket,
                    body.len()
                );
                return Ok(());
            };
            let index = u.index;
            let ip = u.ip.clone();
            // 说话者身份（oracle 是在该网关自己的 `UserList` 上解出的）：`CM_SAY` 用它找 `online`。
            let chr_name = u.chr_name.clone();
            // `:201-225`：还没进世界的会话走 `DoClientCertification`（认证语义属 S2）；
            // 已进世界的才把 12B 命令头（+ EDCode 尾）当命令处理。
            if !u.in_world {
                // `:201-225` 的分支门是「**挂上 PlayObject/WorldEngine**」= 已进世界，**不是** `Certification`（F3）。
                // 已认证但尚未进世界的窗口里，oracle 会进 `DoClientCertification`，而那里 `:307` 见 `Account` 非空
                // **直接空转** ⇒ 这一帧被丢弃（不是当命令处理）。
                if u.certification {
                    glog!(
                        "DBG",
                        "index={index} 已认证未进世界 ⇒ 照 oracle 空转丢帧（{} 字节）",
                        body.len()
                    );
                    return Ok(());
                }
                // 未认证 ⇒ 认证链（`cert.rs`）
                let outcome = {
                    let store = auth.lock().expect("准入表锁被毒化");
                    cert::do_client_certification(body, &store)
                };
                match outcome {
                    cert::CertOutcome::Ok {
                        cert,
                        session_id,
                        pay_ment,
                    } => {
                        users.certify(msg.socket, &cert.account, &cert.chr_name, session_id);
                        glog!(
                            "INF",
                            "会话验证通过 Account:{} ChrName:{} SessionId:{} PayMent:{} index={index}",
                            cert.account,
                            cert.chr_name,
                            session_id,
                            pay_ment
                        );
                        // `:323-337` 的 `LoadDBInfo`（字段逐个对应；`SocketId` = 客户端句柄、`GSocketIdx` = 网关 SessionId）
                        let _ = ev_tx.send(GateEvent::LoadDbRequest(load::LoadDbInfo {
                            account: cert.account.clone(),
                            chr_name: cert.chr_name.clone(),
                            ipaddr: ip.clone(),
                            session_id,
                            soft_version_date: cert.client_version,
                            pay_ment,
                            pay_mode: 0,
                            socket_id: msg.socket,
                            gate_socket_idx: msg.session_id,
                            gate_idx: cert.gate_id,
                            new_user_tick: now_ms(),
                            play_time: 0,
                        }));
                    }
                    cert::CertOutcome::Failed {
                        account,
                        game_session_id: game_sid,
                    } => {
                        // `:348-353` / `:340-346`：标 *disable* + 关用户（槽位置空；客户端由网关侧后续处理）
                        glog!(
                            "WRN",
                            "会话验证失败.Account:{account} SessionId:{game_sid} SessionId(gate)={} Address:{ip}",
                            msg.session_id
                        );
                        users.close_by_socket(msg.socket);
                        let _ = ev_tx.send(GateEvent::Closed {
                            index,
                            chr_name: String::new(),
                            socket: msg.socket,
                        });
                    }
                    cert::CertOutcome::Ignored => {
                        let _ = ev_tx.send(GateEvent::CertificationPending { index, ip });
                    }
                }
                return Ok(());
            }
            let Some(cmd) = CommandMessage::from_bytes(body) else {
                glog!("WRN", "index={index} 的帧不足 12B（{}），丢弃", body.len());
                return Ok(());
            };
            // `:211-218`：`nMsgLen == 12` ⇒ 尾为 None；否则 `EDCode.DeCodeString(msgBuff[12..^1])`
            let tail = if body.len() == COMMAND_MESSAGE_SIZE {
                None
            } else {
                Some(mir2_protocol::edcode::decode(
                    &body[COMMAND_MESSAGE_SIZE..body.len() - 1],
                ))
            };
            let tail = tail.map(|bytes| gbk_decode(&bytes));
            let _ = ev_tx.send(GateEvent::Command {
                index,
                chr_name,
                socket: msg.socket,
                cmd,
                tail,
            });
        }
        other => {
            glog!("DBG", "未处理的 GM_{other}（socket={}）", msg.socket);
        }
    }
    Ok(())
}

/// GBK 解码（现网账号/角色名是 GBK 字节；展示层才解码，协议层按原始字节处理）。
fn gbk_decode(bytes: &[u8]) -> String {
    let (s, _, _) = encoding_rs::GBK.decode(bytes);
    s.into_owned()
}

/// GBK 编码（说话回帧 `SM_HEAR` 的尾：`ChrName + ':' + sMsg` 要与 oracle 的 GB2312 字节一致）。
/// 合法 GBK 经 `gbk_decode` ⇒ 本函数是逐字节往返（未配对代理等在 GBK 域不存在）。
fn gbk_encode(s: &str) -> Vec<u8> {
    encoding_rs::GBK.encode(s).0.into_owned()
}

async fn send_raw(tx: &Arc<Mutex<OwnedWriteHalf>>, wire: &[u8]) -> Result<()> {
    let mut w = tx.lock().await;
    w.write_all(wire).await.context("写网关失败")?;
    w.flush().await.ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **M2 RNG trace 纯度**：`seed_probe_text` 必须
    /// ① 与 C# 钩子实测的 seed=42 参照序列逐项相同（自我标尺）；
    /// ② 与生产注入的 `RandomNumber::with_seed(seed)` 前 `PROBE_LEN` 项逐项相同（换独立实例不改自证值）。
    ///
    /// 红检：把 `seed_probe_text` 改回经 `RandomNumber`（被 trace 的入口）取数 ⇒ 进程级
    /// `tools/m2-acceptance/check_rng_trace_purity.ps1` 的「流水不得含 `main.rs` 记录」门必红；
    /// 本用例锁的是「换实例后值不变」，是那道门的前置。
    #[test]
    fn seed_probe_text_matches_csharp_seed42_and_random_number() {
        let expected =
            "1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307";
        assert_eq!(
            seed_probe_text(42),
            expected,
            "与 C# 钩子 seed=42 参照序列不一致"
        );
        let mut world_rng = mir2_shared::rng::RandomNumber::with_seed(42);
        let via_random_number = (0..seed::PROBE_LEN)
            .map(|_| world_rng.random().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            seed_probe_text(42),
            via_random_number,
            "独立实例与 RandomNumber 序列不一致（说明不再是同一算法）"
        );
    }

    /// M3②③ k=2 收口红检：`apply_ability_to_entity` 必须把 `WAbil`/`SubAbility` **真写进世界实体**。
    ///
    /// 锚 oracle：L1 战士登录时 `RecalcLevelAbilitys()` 把 `Abil.DC` 设为
    /// `MakeWord(MAX(1/5-1,1), MAX(1,1/5)) = 0x0101`（`PlayObject.cs:5305`），此后
    /// `GetBaseAttackPoewr` 取 `LoByte(DC)=1` 作为攻击力（`BaseObject.Attack.cs:25/27`）；
    /// `SubAbility.HitPoint/SpeedPoint = 5/15`（`BaseObject.Attack.cs:41` 的命中门）。
    /// **改坏（逐字段）**：删掉 `apply_ability_to_entity` 里任一赋值行（如 `e.dc = ...`）
    /// ⇒ 本用例必红。修复前的真机形态 = 全部默认 0（本用例开头的断言锁死它）。
    #[test]
    fn apply_ability_to_entity_loads_wabil_and_sub() {
        let mut e = mir2_world::Entity::new(0, "k2", 17, 20);
        // 进世界前的空实体默认值（＝修复前真机形态：dc/ac/hit_point/speed_point=0）。
        assert_eq!((e.dc, e.ac, e.hit_point, e.speed_point), (0, 0, 0, 0));
        let wabil = game_svr::ability::Ability {
            level: 1,
            reserved1: 0,
            ac: 0,
            mac: 0,
            dc: 0x0101, // L1 战士 `RecalcLevelAbilitys` 的等级派生 DC
            mc: 0,
            sc: 0,
            hp: 18,
            mp: 15,
            max_hp: 19,
            max_mp: 15,
            reserved2: 0,
            exp_count: 0,
            exp_max_count: 0,
            exp: 0,
            max_exp: 10,
            weight: 0,
            max_weight: 50,
            wear_weight: 0,
            max_wear_weight: 15,
            hand_weight: 0,
            max_hand_weight: 12,
        };
        let sub = game_svr::ability::SubAbility {
            hit_point: 5,
            speed_point: 15,
            ..game_svr::ability::SubAbility::default()
        };
        apply_ability_to_entity(&mut e, &wabil, &sub);
        assert_eq!(
            e.dc, 0x0101,
            "攻方 DC ← WAbil.DC（GetBaseAttackPoewr 取 LoByte=1，不是 0）"
        );
        assert_eq!(e.ac, 0, "目标 AC ← WAbil.AC");
        assert_eq!(
            (e.hit_point, e.speed_point),
            (5, 15),
            "命中门 HitPoint/SpeedPoint ← SubAbility"
        );
        assert_eq!(
            (e.hp, e.max_hp, e.level),
            (18, 19, 1),
            "HP/MaxHP/Level ← WAbil"
        );
    }

    /// **空打片（owner 2026-10-12 19:20）判据**：`dir=2` 的前方格 = 受害者所在格；取目标只按
    /// `!ghost && !death` 过滤（oracle `Envir.GetMovingObject(nX,nY,true)`，`Envirnoment.cs:1203-1234`）。
    ///
    /// 真机 120 击里 84 击「前方格 (18,20) 无目标」是受害者在 36 击后 HP 归零、`death=true`
    /// 被排除的**正常 oracle 行为**（`world.rs` 的 `e.death = true` = oracle `Die()`），
    /// **不是**前方格/取目标算错。原始件见
    /// `tools/m2-acceptance/evidence/m2svr-attack-empty-target-20261012/`。
    ///
    /// 红检：删掉 `front_target` 的 `!t.death` ⇒ 死亡后 `None` 断言必红；删 `!t.ghost` ⇒ 幽灵断言必红。
    #[test]
    fn front_target_dir2_finds_victim_and_excludes_ghost_or_dead() {
        let mut world = mir2_world::World::new(30, 33);
        let atk = world.add_entity({
            let mut e = mir2_world::Entity::new(1, "atk", 17, 20);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e
        });
        let vic = world.add_entity({
            let mut e = mir2_world::Entity::new(2, "vic", 18, 20);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.hp = 19;
            e.max_hp = 19;
            e
        });
        // oracle `GetFrontPosition` + `Direction.Right = 2` ⇒ (x+1, y) = (18,20)
        assert_eq!(
            game_svr::attackwire::front_position(2, 17, 20, 30, 33),
            (18, 20),
            "dir=2 的前方格 = 受害者所在格"
        );
        assert_eq!(
            front_target(&world, atk, 0, 18, 20),
            Some(vic),
            "受害者在位 ⇒ 取到"
        );
        // 自己（`t.id != actor_id`）不算目标
        assert_eq!(front_target(&world, atk, 0, 17, 20), None, "自己不算目标");
        // 死亡后（`world.rs:516` 的 `e.death = true`；oracle `Die()`）⇒ `!Death` 排除 ⇒ 空打
        world.entities.get_mut(vic).unwrap().death = true;
        assert_eq!(front_target(&world, atk, 0, 18, 20), None, "!Death 过滤");
        world.entities.get_mut(vic).unwrap().death = false;
        // 幽灵（`MakeGhost`）⇒ `!Ghost` 排除
        world.entities.get_mut(vic).unwrap().ghost = true;
        assert_eq!(front_target(&world, atk, 0, 18, 20), None, "!Ghost 过滤");
    }

    /// F2（reviewer-2）的判据：`ProcessBufferSend` `:58-90` 的四条分支，**含"刚超时清零的那一包也要压住"**。
    #[test]
    fn send_decision_matches_process_buffer_send() {
        let mut st = KeepAlive::default();
        // ① 正常：未到阈值 ⇒ Send
        assert_eq!(send_decision(&mut st, 12288, 20, 0), SendDecision::Send);
        // ② 累积到阈值 ⇒ 先发 check
        st.send_block_count = 122_880 - 10; // CheckBlock*10 = 122880
        assert_eq!(
            send_decision(&mut st, 12288, 20, 0),
            SendDecision::CheckFirst
        );
        // ③ 单包就超阈值（且累计数为 0）⇒ 丢
        let mut st2 = KeepAlive::default();
        assert_eq!(send_decision(&mut st2, 1, 20, 0), SendDecision::Drop);
        // ④ SendChecked>0 且未超时 ⇒ Hold（不发、不清）
        let mut st3 = KeepAlive {
            send_keep_alive: false,
            send_checked: 1,
            send_check_tick: 1000,
            send_block_count: 7,
        };
        assert_eq!(send_decision(&mut st3, 12288, 20, 1040), SendDecision::Hold);
        assert_eq!(st3.send_checked, 1, "未超时不清零");
        // ⑤ SendChecked>0 且**已超时**：清零后仍 Hold（oracle `:58-66` 的无条件 return）
        let mut st4 = KeepAlive {
            send_keep_alive: false,
            send_checked: 1,
            send_check_tick: 1000,
            send_block_count: 7,
        };
        assert_eq!(send_decision(&mut st4, 12288, 20, 1060), SendDecision::Hold);
        assert_eq!(st4.send_checked, 0, "超时清零");
        assert_eq!(st4.send_block_count, 0);
        // 清零后的下一包才正常发
        assert_eq!(send_decision(&mut st4, 12288, 20, 1061), SendDecision::Send);
    }

    /// N1（reviewer-3）的红检：`MarkInWorld` 必须按该用户的 `gate_idx` 路由，不能「随便挑一个网关」。
    ///
    /// 造两条假网关线并**故意让 socket 号重复**：标记应只投到 1 号线，0 号线的同号槽**不得**被标。
    /// 两个 `same_channel` 断言锁死「按线号取通道」——把实现改回 `gates.values().next()`（对同一张表
    /// 两次调用必返回同一条）时，这两个断言至少有一个必红。
    #[test]
    fn mark_in_world_routes_by_gate_idx_not_arbitrary_gate() {
        use std::collections::HashMap;
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let tx0_keep = tx0.clone();
        let tx1_keep = tx1.clone();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);

        // 路由选择：按 gate_idx 取到的必须是**对应那条**
        assert!(
            route_to_gate(&gates, 1, 5, "标记进世界")
                .expect("1 号线存在")
                .same_channel(&tx1_keep),
            "gate_idx=1 必须路由到 1 号线"
        );
        assert!(
            route_to_gate(&gates, 0, 5, "标记进世界")
                .expect("0 号线存在")
                .same_channel(&tx0_keep),
            "gate_idx=0 必须路由到 0 号线"
        );

        // 两条线各自的槽表里都有 **同一个** socket=5（各网关各自分配，号可重复）
        let mut users0 = UserList::new();
        users0.open(5, 111, "a".into());
        let mut users1 = UserList::new();
        users1.open(5, 222, "b".into());

        // 该用户的 gate_idx=1 ⇒ 标记只投 1 号线
        route_to_gate(&gates, 1, 5, "标记进世界")
            .unwrap()
            .send(SimToGate::MarkInWorld { socket: 5 })
            .unwrap();
        assert!(
            matches!(rx0.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "0 号线不得收到该标记"
        );
        let cmd = rx1.try_recv().expect("1 号线应收到该标记");
        apply_sim_cmd(cmd, &mut users1);
        assert!(
            users1.find(1, 5).unwrap().in_world,
            "1 号线的槽应被标 in_world"
        );
        assert!(
            !users0.find(1, 5).unwrap().in_world,
            "0 号线的同号 socket **不得**被标（N1 的错标场景）"
        );
    }

    /// N1 的负向：线号不在表里 ⇒ 不投递（返回 `None`），而不是静默挑一个。
    #[test]
    fn mark_in_world_missing_gate_idx_does_not_pick_arbitrary() {
        use std::collections::HashMap;
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx2, mut rx2) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(2, tx2);
        assert!(
            route_to_gate(&gates, 1, 5, "标记进世界").is_none(),
            "1 号线不在表里 ⇒ 不许挑 0/2 号"
        );
        assert!(matches!(
            rx0.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        assert!(matches!(
            rx2.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }

    /// **R4-1**（reviewer-4）：越界/非法线号（`-2`、`256`、`300`）必须与 oracle `TCPNetChannel.cs:213-218`
    /// `GetGate` 的 `null` 同构 —— **直接丢弃**，不许用 `unwrap_or(0)` 静默折算到 0 号线。
    ///
    /// 判据两条：① `route_to_gate` 返回 `None`（不投递）；② 0/1 号线的同号槽**不得**被标 `in_world`。
    /// 红检：把实现改回 `unwrap_or(0)` ⇒ `-2`/`256`/`300` 都会命中 0 号线，本用例第一条断言即红。
    #[test]
    fn route_to_gate_rejects_out_of_range_gate_idx() {
        use std::collections::HashMap;
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);

        // 0 号线里有一个 socket=5 的槽：越界线号**不得**把它标上（R4-1 的错标场景）。
        let mut users0 = UserList::new();
        users0.open(5, 111, "a".into());

        for bad in [-2i32, 256, 300] {
            assert!(
                route_to_gate(&gates, bad, 5, "标记进世界").is_none(),
                "gate_idx={bad} 越界/非法 ⇒ 必须丢弃（oracle GetGate 返回 null），不许折算成 0 号线"
            );
        }
        assert!(matches!(
            rx0.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        assert!(matches!(
            rx1.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        assert!(
            !users0.find(1, 5).unwrap().in_world,
            "越界线号不得把 0 号线同号 socket 标成 in_world"
        );
    }

    /// **S3 后半段（第一格）**：世界 tick 的 AOI `Show` ⇒ 每个观察者收到“对方”的
    /// `SM_TURN` + `SM_FEATURECHANGED`，且**按各自的 `[GateIdx, Socket]` 路由**到对应网关线。
    ///
    /// 造两个玩家（gate 0/socket 901 与 gate 1/socket 902，各自有展示表）在世界里相邻，
    /// tick 一次让 AOI 互相标 `Show`，再 drain ⇒ 两条线各收到一对帧、且 `Recog` 是**对方**的
    /// `ActorId`（自身不入帧）。
    ///
    /// 红检：把 `drain_aoi_downlink` 的 `visible.target == observer_id` 过滤去掉 ⇒ 自身帧
    /// 挤到最前，第一条 `Recog` 不再是对方 ⇒ 必红；把 `route_to_gate(gates, entry.gate_idx, …)`
    /// 改成 `gates.values().next()` ⇒ 两条线的帧落到同一条 ⇒ 另一条断言必红。
    #[test]
    fn aoi_show_emits_turn_pair_routed_by_gate() {
        use std::collections::HashMap;

        let mut world = mir2_world::World::new(100, 100);
        world.open_session("a");
        world.open_session("b");
        let a = world
            .enter_map("a", mir2_world::Entity::new(0, "a", 10, 10), 0)
            .expect("a 进图");
        let b = world
            .enter_map("b", mir2_world::Entity::new(0, "b", 12, 10), 0)
            .expect("b 进图");
        // 一个 tick 让 AOI 把彼此标 `VisibleFlag::Show`（闸道世界 tick 每轮都刷新）。
        world.tick();

        let mut displays = downlink::DisplayTable::new();
        displays.set(
            a,
            downlink::ActorDisplay::new(3, 0, 111, 0, 0, 255, b"a".to_vec()),
        );
        displays.set(
            b,
            downlink::ActorDisplay::new(4, 1, 222, 0, 0, 255, b"b".to_vec()),
        );

        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "a".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: a,
            },
        );
        online_set(
            &mut online,
            OnlineEntry {
                account: "ub".into(),
                chr_name: "b".into(),
                session_id: 2,
                gate_idx: 1,
                socket: 902,
                actor_id: b,
            },
        );

        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);

        drain_aoi_downlink(&world, &displays, &online, &gates);

        let collect = |rx: &mut mpsc::UnboundedReceiver<SimToGate>| -> Vec<(i32, u16)> {
            let mut out = Vec::new();
            while let Ok(SimToGate::SendFrame { body, .. }) = rx.try_recv() {
                let cm = mir2_protocol::frame::CommandMessage::from_bytes(&body)
                    .expect("SM_* body 至少 12B");
                out.push((cm.recog, cm.ident));
            }
            out
        };
        assert_eq!(
            collect(&mut rx0),
            vec![
                (b, mir2_protocol::messages::SM_TURN),
                (b, mir2_protocol::messages::SM_FEATURECHANGED),
            ],
            "gate0（a 的线）应只收到 b 的一对帧（自身不入帧）"
        );
        assert_eq!(
            collect(&mut rx1),
            vec![
                (a, mir2_protocol::messages::SM_TURN),
                (a, mir2_protocol::messages::SM_FEATURECHANGED),
            ],
            "gate1（b 的线）应只收到 a 的一对帧（按 GateIdx 路由）"
        );
    }

    /// 测试用的 `WAbil`（金币分支不使用 `wabil`；字段值不影响本例断言）。
    fn test_ability() -> game_svr::ability::Ability {
        game_svr::ability::Ability {
            level: 1,
            reserved1: 0,
            ac: 0,
            mac: 0,
            dc: 0,
            mc: 0,
            sc: 0,
            hp: 0,
            mp: 0,
            max_hp: 0,
            max_mp: 0,
            reserved2: 0,
            exp_count: 0,
            exp_max_count: 0,
            exp: 0,
            max_exp: 0,
            weight: 100,
            max_weight: 0,
            wear_weight: 5,
            max_wear_weight: 0,
            hand_weight: 0,
            max_hand_weight: 0,
        }
    }

    /// 测试用的 `EquipInputs`（`CM_TAKEONITEM`/`CM_TAKEOFFITEM` 的静态输入；不在本用例断言）。
    fn test_equip_inputs() -> EquipInputs {
        EquipInputs {
            job: 0,
            gender: 0,
            hair: 0,
            re_level: 0,
            credit_point: 0,
            abil: test_ability(),
            bonus_abil: game_svr::ability::NakedAbility {
                dc: 0,
                mc: 0,
                sc: 0,
                ac: 0,
                mac: 0,
                hp: 0,
                mp: 0,
                hit: 0,
                speed: 0,
                reserved: 0,
            },
            bonus_tick_hit: 20,
            bonus_tick_speed: 35,
        }
    }

    /// 测试用的最小 `SessionGround`（攻击/受击用例只需 `wabil` 与 `per_*`）。
    fn test_session_ground(actor_id: i32) -> SessionGround {
        SessionGround {
            state: game_svr::ground::GroundState {
                item_list: Vec::new(),
                gold: 0,
                wabil: test_ability(),
            },
            actor_id,
            game_gold: 0,
            sys_msg: None,
            use_items: vec![None; game_svr::ability::USE_ITEMS_LEN],
            sub: game_svr::ability::SubAbility::first_call(),
            equip: test_equip_inputs(),
            recovery: game_svr::recover::Recovery::new(0),
            stamina: game_svr::stamina::Stamina::new(0),
            show_hp: false,
            user_unlock_durg: false,
            hunger_status: 0,
            query_bag_items_tick: None,
            save_rcd_tick: 0,
        }
    }

    /// oracle 全字段角色记录（`stage6` 向量，`tests/fixtures/player_data_full_oracle.txt`）——存档用例的底稿。
    fn stage6_record() -> record::PlayerRecord {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/player_data_full_oracle.txt"
        );
        let text = std::fs::read_to_string(path).expect("player_data_full_oracle.txt");
        let hexs = text
            .split_whitespace()
            .find_map(|t| t.strip_prefix("stage6="))
            .expect("fixture 里必须有 stage6 向量");
        let bytes: Vec<u8> = (0..hexs.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hexs[i..i + 2], 16).unwrap())
            .collect();
        record::parse_player_data(&bytes).expect("stage6 必须可解为全字段记录")
    }

    /// 存档记录 = 原始记录 + 会话实时增量（`MakeSaveRcd` 口径）。用 **DBSvr 侧类型**解回断言，
    /// 覆盖实时坐标/Gold/物品与 Helmet/Necklace 交换 + 46 槽补位。
    #[test]
    fn save_record_reflects_live_state_and_item_layout() {
        use mir2_storage::MemPack;
        let saved = SavedHuman {
            account: "acct".into(),
            record: stage6_record(),
            enter_map: "0".into(),
        };
        let weapon = game_svr::bag::BagItem {
            make_index: 9001,
            index: 207,
            dura: 4000,
            dura_max: 4000,
            desc: [0; 14],
        };
        let necklace = game_svr::bag::BagItem::new(9002, 88);
        let helmet = game_svr::bag::BagItem::new(9003, 99);
        let mut use_items = vec![None; game_svr::ability::USE_ITEMS_LEN];
        use_items[game_svr::ability::SLOT_WEAPON] = Some(weapon.clone());
        use_items[game_svr::ability::SLOT_NECKLACE] = Some(necklace);
        use_items[game_svr::ability::SLOT_HELMET] = Some(helmet);
        let sg = SessionGround {
            state: game_svr::ground::GroundState {
                item_list: vec![weapon],
                gold: 649,
                wabil: test_ability(),
            },
            actor_id: 1,
            game_gold: 77,
            sys_msg: None,
            use_items,
            sub: game_svr::ability::SubAbility::first_call(),
            equip: test_equip_inputs(),
            recovery: game_svr::recover::Recovery {
                inc_health: 3,
                inc_spell: 4,
                ..game_svr::recover::Recovery::new(0)
            },
            stamina: game_svr::stamina::Stamina::new(0),
            show_hp: false,
            user_unlock_durg: false,
            hunger_status: 55,
            query_bag_items_tick: None,
            save_rcd_tick: 0,
        };
        let mut world = mir2_world::World::with_maps(vec![(100, 100)]);
        world.open_session("hero");
        world
            .enter_map("hero", mir2_world::Entity::new(0, "hero", 11, 22), 0)
            .expect("进世界");
        let rec = build_save_record(&saved, "hero", &world, &sg, None);
        let parsed = mir2_storage::SaveCharacterData::from_bytes(
            &record::encode_save_character_data(&saved.account, "hero", &rec),
        )
        .expect("DBSvr 侧必须能解存档体");
        let d = parsed.character_data.unwrap().data.unwrap();
        assert_eq!(d.cur_map.as_deref(), Some("0"));
        assert_eq!((d.cur_x, d.cur_y), (11, 22), "实时坐标来自世界实体");
        assert_eq!(d.gold, 649);
        assert_eq!(d.game_gold, 77);
        assert_eq!(d.inc_health, 3);
        assert_eq!(d.inc_spell, 4);
        assert_eq!(d.hunger_status, 55);
        assert!(
            d.abil.as_ref().unwrap().hp == 0,
            "保存的 Abil 取自会话 WAbil（test_ability hp=0）"
        );
        let bag = d.bag_items.as_ref().expect("bag_items 非 null");
        assert_eq!(bag.len(), 46, "BagItems 是 46 槽定长");
        assert_eq!(bag[0].as_ref().unwrap().index, 207);
        assert_eq!(
            bag[1].as_ref().unwrap().index,
            0,
            "空槽 = 默认物品（Index 0）"
        );
        let hum = d.hum_items.as_ref().expect("hum_items 非 null");
        assert_eq!(hum.len(), 13, "HumItems 是 13 槽定长");
        assert_eq!(
            hum[game_svr::ability::SLOT_WEAPON].as_ref().unwrap().index,
            207
        );
        assert_eq!(
            hum[game_svr::ability::SLOT_HELMET].as_ref().unwrap().index,
            88,
            "HumItems[Helmet] = UseItems[Necklace]（oracle 交换）"
        );
        assert_eq!(
            hum[game_svr::ability::SLOT_NECKLACE]
                .as_ref()
                .unwrap()
                .index,
            99,
            "HumItems[Necklace] = UseItems[Helmet]（oracle 交换）"
        );
    }

    /// owner-precheck `a8836e1b`：存档 ↔ 进世界守卫的 in-save 账本配对。生产路径发
    /// `DB_SAVEHUMANRCD` 时 `begin_save`、应答回来（`DBR_SAVEHUMANRCD(1102)`）时 `end_save`；
    /// 进世界守卫读 `in_save_rcd_list`（`ProcessHumansIsLogined` 的 in-save 半边）。
    #[test]
    fn save_ledger_round_trips_between_begin_and_reply() {
        let mut engine = load::FrontEngine::new();
        assert!(!engine.in_save_rcd_list("Hero"));
        engine.begin_save("Hero"); // 发 DB_SAVEHUMANRCD 时
        assert!(
            engine.in_save_rcd_list("hero"),
            "存档窗口内守卫必须命中（⇒ 关新，与 oracle 同）"
        );
        engine.end_save("hero"); // DBR_SAVEHUMANRCD 应答回来时
        assert!(!engine.in_save_rcd_list("HERO"), "应答后复位");
    }

    /// **F-1（reviewer-5 `962fdf81` / owner 裁决 `99c89a88` 选项 a）**：存档应答的失败路径照
    /// oracle `PlayerDataService.cs:118-136` 的 `ProcessSaveQueue`——**仅**
    /// `DBR_SAVEHUMANRCD(1102) && recog==1` 才把角色移出 `InSaveRcdList`；
    /// `DBR_LOADHUMANRCD(1100) recog=0`（存库失败）只出队、**不**复位
    /// （oracle 的「卡在保存中」连 bug 一起搬）。
    ///
    /// 红检：把 `apply_save_reply` 改成无条件 `engine.end_save(chr_name)` ⇒ 第一条断言必红。
    #[test]
    fn save_ledger_only_released_on_success_reply() {
        use mir2_protocol::messages::{DBR_LOADHUMANRCD, DBR_SAVEHUMANRCD};
        let mut engine = load::FrontEngine::new();
        engine.begin_save("Hero");
        assert!(engine.in_save_rcd_list("Hero"));

        // 失败应答：1100 recog=0 ⇒ 不移出保存名单（与 oracle 同）
        apply_save_reply(&mut engine, "Hero", DBR_LOADHUMANRCD, 0);
        assert!(
            engine.in_save_rcd_list("hero"),
            "DBR_LOADHUMANRCD(1100) 不得把角色移出 InSaveRcdList（oracle 连 bug 一起搬）"
        );

        // 成功应答：1102 recog=1 ⇒ 移出
        apply_save_reply(&mut engine, "Hero", DBR_SAVEHUMANRCD, 1);
        assert!(
            !engine.in_save_rcd_list("HERO"),
            "DBR_SAVEHUMANRCD(1102) 应答后必须复位"
        );
    }

    /// 周期存档的计时口径（oracle `WorldServer.cs:698`）：**严格 `>`**（相等不触发）+ 触发时
    /// **先**把 tick 重置到 `now`（随后才发请求）；单位 = `SaveHumanRcdTime` 毫秒。
    ///
    /// 红检：把 `>` 改成 `>=` ⇒ 第一条断言（差值 == 阈值不得触发）必红；
    /// 删掉 `*save_rcd_tick = now;`（不重置）⇒ 第二条「重置后须再等一个周期」必红。
    #[test]
    fn periodic_save_due_is_strict_gt_and_resets_on_fire() {
        let save_time = 5_000i64;
        let mut tick = 1_000i64;
        // 差值恰等于阈值 ⇒ 不触发（严格 `>`）。
        assert!(!periodic_save_due(&mut tick, save_time, 6_000));
        assert_eq!(tick, 1_000, "未触发不得动 tick");
        // 超过 1ms ⇒ 触发，且 tick 重置到 now。
        assert!(periodic_save_due(&mut tick, save_time, 6_001));
        assert_eq!(tick, 6_001, "触发时先重置");
        // 刚重置过 ⇒ 同一周期不再触发。
        assert!(!periodic_save_due(&mut tick, save_time, 11_001));
        // 再过一个完整周期 +1 ⇒ 第二次触发。
        assert!(periodic_save_due(&mut tick, save_time, 11_002));
    }

    /// **卡 `openmir2-m3-periodic-save`（DoD ③）**：把 `SaveHumanRcdTime` 调到秒级 ⇒ 一个会话上
    /// **观测 ≥2 次**周期 `DB_SAVEHUMANRCD` 请求；请求体用 **DBSvr 侧同一类型**
    /// `mir2_storage::SaveCharacterData` 解回，逐字段核对（不按公式重推）。
    ///
    /// 红检（改坏 ⇒ 必红，每次跑完立即还原）：
    /// - 删掉 `drain_periodic_save` 里对 `periodic_save_due` 的调用（或恒 `false`）⇒ 请求数 = 0 ⇒ 必红；
    /// - 把记录某列写坏（例如 `build_save_record` 的 `gold` 置 0）⇒ 记录字段断言必红；
    /// - 不把重置写回 `SessionGround`（`periodic_save_due` 不写 `*save_rcd_tick`）⇒ 第 3 段
    ///   「未到期不得再发」必红。
    #[test]
    fn periodic_save_fires_repeatedly_and_record_is_well_formed() {
        use mir2_protocol::messages::DB_SAVEHUMANRCD;
        use mir2_storage::MemPack;
        use std::sync::atomic::AtomicI32;

        let mut world = mir2_world::World::with_maps(vec![(100, 100)]);
        world.open_session("hero");
        let actor_id = world
            .enter_map("hero", mir2_world::Entity::new(0, "hero", 11, 22), 0)
            .expect("进世界");
        let mut sg = test_session_ground(actor_id);
        sg.state.gold = 649;
        // 计时基准 = 1_000；周期 = 5_000ms（“调到秒级”）。
        sg.save_rcd_tick = 1_000;

        let mut sessions = std::collections::HashMap::new();
        sessions.insert("hero".to_string(), sg);
        let mut saves = std::collections::HashMap::new();
        saves.insert(
            "hero".to_string(),
            SavedHuman {
                account: "acct".into(),
                record: stage6_record(),
                enter_map: "0".into(),
            },
        );

        // 已连接的 DB 链路：请求落到 `req_tx`，测试从 `rx` 取走。
        let state = dbclient::DbLinkState::new();
        state.mark_connected();
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let mut db = Some(DbWiring {
            state,
            req_tx: tx,
            next_query_id: Arc::new(AtomicI32::new(0)),
            replies: Arc::new(std::sync::Mutex::new(dbclient::DbReplies::new())),
            outstanding: Vec::new(),
            save_outstanding: Vec::new(),
        });
        let mut engine = load::FrontEngine::new();

        let mut fired = 0usize;
        for now in [6_000i64, 6_001, 11_001, 11_002] {
            fired += drain_periodic_save(
                &mut sessions,
                &saves,
                &world,
                &mut db,
                &mut engine,
                None,
                5_000,
                now,
            );
        }
        assert_eq!(fired, 2, "秒级周期参数下必须观测到 ≥2 次周期请求");

        let mut frames = Vec::new();
        while let Ok(f) = rx.try_recv() {
            frames.push(f);
        }
        assert_eq!(frames.len(), 2, "请求数必须 = 触发数");
        for (i, frame) in frames.iter().enumerate() {
            let mut p = dbclient::ReplyParser::default();
            p.push(frame);
            let r = p.next().expect("存档请求应可被同信封解析器解出");
            assert_eq!(
                dbclient::parse_reply_message(&r.message),
                Some((DB_SAVEHUMANRCD, 0)),
                "第 {} 条请求的 Message.Ident 必须是 DB_SAVEHUMANRCD(101)",
                i + 1
            );
            let decoded = mir2_protocol::edcode::decode(&r.packet);
            let parsed = mir2_storage::SaveCharacterData::from_bytes(&decoded)
                .expect("DBSvr 侧必须能解周期存档体");
            assert_eq!(parsed.account.as_deref(), Some("acct"));
            assert_eq!(parsed.chr_name.as_deref(), Some("hero"));
            let d = parsed.character_data.unwrap().data.unwrap();
            assert_eq!(d.cur_map.as_deref(), Some("0"));
            assert_eq!((d.cur_x, d.cur_y), (11, 22), "实时坐标来自世界实体");
            assert_eq!(d.gold, 649, "落库记录必须带实时 Gold（改坏某列 ⇒ 必红）");
        }
        // 计时器已推进到第二次触发时刻（重置写回了会话态）。
        assert_eq!(sessions.get("hero").unwrap().save_rcd_tick, 11_002);
    }

    /// **`openmir2-m3-move-wiring` ①–③（移动接线）**：`CM_TURN` **只在**目标=当前位置时改朝向、
    /// 给附近观察者发 `SM_TURN`+`SM_FEATURECHANGED`、给自己回 `+GD` 短包；目标≠当前位置时既不改
    /// 朝向也不回 ack。`CM_WALK` 走一格、给观察者发 `SM_WALK`（ident 11、移动后坐标、
    /// `MakeWord(Dir, Light)`）、给自己回 `+GD`。
    ///
    /// 返工（reviewer-5 F-1/F-2/F-3）：
    /// - **F-1**：`CM_TURN` 下行 `SM_TURN` 尾**只有** `Encode(8B CharDesc)`（无名尾）；
    /// - **F-2**：移动帧广播与 result 门分离——非相邻目标仍广播 `SM_WALK`，但**不回** `+GD`；
    /// - **F-3**：`Entity.death` ⇒ 四支全拒（不改朝向/不移动/不发帧/不回 ack）。
    ///
    /// 红检：① `movewire::turn_is_at_target` 改成恒 `true` ⇒ 第 ① 段（目标≠自身不得回 ack）必红；
    /// ② `movewire::sm_ident_for` 的 `CM_WALK` 支改成 `SM_RUN`（或 `next_direction` 改坏）
    /// ⇒ 帧头 ident / 坐标断言必红；③ `handle_move_cmd` 的 CM_TURN 支改回 `turn_pair` ⇒
    /// F-1 尾断言必红；④ 去掉 `movewire::move_result_ok` 门（恒回 ack）⇒ ④ 段必红；
    /// ⑤ 去掉 `is_dead` 守卫 ⇒ ⑤ 段必红。
    #[test]
    fn move_cmd_dispatches_turn_walk_ack_and_aoi() {
        use mir2_protocol::messages::{CM_TURN, CM_WALK, SM_TURN, SM_WALK};
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let mover = {
            let mut e = mir2_world::Entity::new(0, "mover".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        let observer = {
            let mut e = mir2_world::Entity::new(0, "obs".to_string(), 12, 12);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick(); // Enter 落格（`try_step` 要从源格里摘除）
        let mut displays = downlink::DisplayTable::new();
        displays.set(
            mover,
            downlink::ActorDisplay::new(0, 0, 111, 0, 0, 255, b"mover".to_vec()),
        );
        displays.set(
            observer,
            downlink::ActorDisplay::new(0, 0, 222, 0, 0, 255, b"obs".to_vec()),
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "mover".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: mover,
            },
        );
        online_set(
            &mut online,
            OnlineEntry {
                account: "ub".into(),
                chr_name: "obs".into(),
                session_id: 2,
                gate_idx: 0,
                socket: 902,
                actor_id: observer,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        // ① 目标 != 当前位置：不转身、不回 ack、不发帧。
        handle_move_cmd(
            CM_TURN,
            game_svr::feature::make_long(15, 10),
            2,
            "mover",
            901,
            &online,
            &mut std::collections::HashMap::new(),
            &mut world,
            &mut displays,
            &gates,
        );
        assert!(rx0.try_recv().is_err(), "目标≠当前位置 ⇒ 不得回 ack / 发帧");
        assert_eq!(
            displays.get(mover).unwrap().dir,
            0,
            "目标≠当前位置 ⇒ 朝向不变"
        );

        // ② 目标 = 自身：改朝向 + SM_TURN 给 obs + +GD 给自己。
        handle_move_cmd(
            CM_TURN,
            game_svr::feature::make_long(10, 10),
            2,
            "mover",
            901,
            &online,
            &mut std::collections::HashMap::new(),
            &mut world,
            &mut displays,
            &gates,
        );
        assert_eq!(
            displays.get(mover).unwrap().dir,
            2,
            "目标=自身 ⇒ Dir = (byte)Tag"
        );
        let (mut turn, mut ack) = (Vec::new(), None);
        while let Ok(m) = rx0.try_recv() {
            match m {
                SimToGate::SendFrame { socket, body } => {
                    assert_eq!(socket, 902, "转身帧只给观察者");
                    turn.push(body);
                }
                SimToGate::SendRaw { socket, payload } => {
                    assert_eq!(socket, 901, "+GD 只给发起者自己");
                    ack = Some(payload);
                }
                other => panic!("意外 sim→gate 命令：{other:?}"),
            }
        }
        assert_eq!(
            turn.len(),
            2,
            "`RM_TURN` 支 = SM_TURN + SM_FEATURECHANGED 两帧"
        );
        assert_eq!(
            &turn[0][..12],
            &smframe::command_header(mover, SM_TURN, 10, 10, game_svr::feature::make_word(2, 0))
        );
        // F-1：`CM_TURN` 的 `processMsg.Msg` 恒空 ⇒ `SM_TURN` 尾**只有** `Encode(8B CharDesc)`，
        // 无名尾。若改回 `downlink::turn_pair`（出现帧路径）会多出 `Encode("ShowName/Color")`
        // ⇒ 下面两条断言必红。
        assert_eq!(
            &turn[0][12..],
            &mir2_protocol::edcode::encode(&downlink::char_desc(111, 0)),
            "CM_TURN 的 SM_TURN 尾必须只有 Encode(8B CharDesc)，不得含 ShowName/Color 名尾"
        );
        assert_eq!(turn[0].len(), 12 + 11, "SM_TURN 尾 = Encode(8B) = 11 字符");
        assert_eq!(
            u16::from_le_bytes([turn[1][4], turn[1][5]]),
            mir2_protocol::messages::SM_FEATURECHANGED
        );
        let ack = ack.expect("发起者应收 +GD");
        assert!(
            ack.starts_with(b"+GD/"),
            "ack 形状必须是 +GD/<tick>：{ack:?}"
        );

        // ③ CM_WALK 往右走一格：坐标 10,10 → 11,10；SM_WALK 给 obs；+GD 给自己。
        handle_move_cmd(
            CM_WALK,
            game_svr::feature::make_long(11, 10),
            0,
            "mover",
            901,
            &online,
            &mut std::collections::HashMap::new(),
            &mut world,
            &mut displays,
            &gates,
        );
        {
            let e = world.entities.get(mover).unwrap();
            assert_eq!((e.x, e.y), (11, 10), "走一步后坐标");
        }
        assert_eq!(
            displays.get(mover).unwrap().dir,
            2,
            "方向由 GetNextDirection 求出 = Right"
        );
        let (mut walk, mut ack2) = (None, None);
        while let Ok(m) = rx0.try_recv() {
            match m {
                SimToGate::SendFrame { socket, body } => {
                    assert_eq!(socket, 902);
                    walk = Some(body);
                }
                SimToGate::SendRaw { socket, payload } => {
                    assert_eq!(socket, 901);
                    ack2 = Some(payload);
                }
                other => panic!("意外 sim→gate 命令：{other:?}"),
            }
        }
        let walk = walk.expect("观察者应收 SM_WALK");
        assert_eq!(
            &walk[..12],
            &smframe::command_header(mover, SM_WALK, 11, 10, game_svr::feature::make_word(2, 0))
        );
        assert!(ack2.expect("+GD").starts_with(b"+GD/"));

        // ④ F-2：`CM_WALK` 目标非相邻（(14,10)，走 1 格只能到 (12,10)）⇒ 仍广播 `SM_WALK`
        //    （oracle `WalkTo` 内 `Walk(RM_WALK)` 与 result 门无关），但 result 门
        //    `SpaceMoved || at_target` 为假 ⇒ **不回** `+GD`。
        handle_move_cmd(
            CM_WALK,
            game_svr::feature::make_long(14, 10),
            0,
            "mover",
            901,
            &online,
            &mut std::collections::HashMap::new(),
            &mut world,
            &mut displays,
            &gates,
        );
        {
            let e = world.entities.get(mover).unwrap();
            assert_eq!((e.x, e.y), (12, 10), "目标非相邻时仍按方向走 1 格");
        }
        let (mut walk2, mut ack3) = (None, None);
        while let Ok(m) = rx0.try_recv() {
            match m {
                SimToGate::SendFrame { socket, body } => {
                    assert_eq!(socket, 902);
                    walk2 = Some(body);
                }
                SimToGate::SendRaw { payload, .. } => ack3 = Some(payload),
                other => panic!("意外 sim→gate 命令：{other:?}"),
            }
        }
        let walk2 = walk2.expect("移动已发生 ⇒ 观察者仍收 SM_WALK");
        assert_eq!(
            &walk2[..12],
            &smframe::command_header(mover, SM_WALK, 12, 10, game_svr::feature::make_word(2, 0))
        );
        assert!(
            ack3.is_none(),
            "F-2：移动后 (12,10) != 目标 (14,10) ⇒ 不得回 +GD（oracle result 门）：{ack3:?}"
        );

        // ⑤ F-3：`Death` 守卫——四支都要在开头拒绝（不改朝向、不移动、不发帧、不回 ack）。
        world.entities.get_mut(mover).unwrap().death = true;
        for (ident, target) in [(CM_TURN, (12, 10)), (CM_WALK, (13, 10))] {
            handle_move_cmd(
                ident,
                game_svr::feature::make_long(target.0, target.1),
                3,
                "mover",
                901,
                &online,
                &mut std::collections::HashMap::new(),
                &mut world,
                &mut displays,
                &gates,
            );
            assert!(
                rx0.try_recv().is_err(),
                "F-3：death=true ⇒ ident={ident} 不得发帧/回 ack"
            );
        }
        {
            let e = world.entities.get(mover).unwrap();
            assert_eq!((e.x, e.y), (12, 10), "F-3：死亡后不得移动");
        }
        assert_eq!(
            displays.get(mover).unwrap().dir,
            2,
            "F-3：死亡后不得改朝向（tag=3 未被采纳）"
        );
    }

    /// **owner §26（sim 循环对接）**：`CM_PICKUP` 派发到 `ground::pick_up` + `pickwire`，
    /// 经该角色的 `[GateIdx,Socket]` 回 `SM_GOLDCHANGED`，并把金币写进会话态 `GroundState`。
    ///
    /// 金币地面物由「怪物击杀掉落」产出（与 world 的只读分片同一口径），`of_base_object` = 击杀者
    /// ⇒ `is_self` 成立、可拾。`SM_GOLDCHANGED` 必须用**改后**的 `Gold`（oracle `GoldChanged()`
    /// 在 `Gold +=` 之后）。
    ///
    /// 红检：① 位置门 `pickup_position_ok` 反掉 ⇒ 第一条断言收不到帧；② 去掉
    /// `RM_GOLDCHANGED → sm_goldchanged` 映射（或 `route_to_gate` 改回 `values().next()`）
    /// ⇒ 同样收不到该帧；③ `SM_GOLDCHANGED` 用改前 `gold`（=0）⇒ `recog` 断言必红。
    #[test]
    fn pickup_gold_dispatches_and_updates_session() {
        use mir2_world::Command;
        use std::collections::HashMap;

        let mut world = mir2_world::World::new(32, 32);
        world.item_catalog = vec![mir2_data::StdItem {
            name: "金币".into(),
            ..Default::default()
        }];
        world.mon_random_add_value = 0;
        let p = world.add_entity({
            let mut e = mir2_world::Entity::new(1, "hero", 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.hp = 200;
            e.max_hp = 200;
            e.dc = 0x0A_14; // LoByte=20, HiByte=10 ⇒ 攻击力足以一击击杀
            e.hit_point = 10;
            e.speed_point = 0;
            e.max_exp = 1000;
            e
        });
        world.spawn_monster(Command::SpawnMonster {
            id: 2,
            name: "僵尸".into(),
            x: 11,
            y: 10,
            map_id: 0,
            level: 1,
            exp: 0,
            hp: 3,
            dc: 1,
            ac: 0,
            undead: 0,
            hit_point: 5,
            speed_point: 15,
            drop_list: vec![mir2_data::MonsterDropItem {
                sel_point: 10,
                max_point: 10,
                item_name: "金币".into(),
                count: 300,
            }],
        });
        world.tick(); // 生成
        world.attack_cmd(p, 2);
        let _ = world.tick(); // 死亡结算 + 掉落落地
        let gold = world
            .floor_items_view_sorted()
            .into_iter()
            .find(|v| v.kind == mir2_world::FloorItemKind::Gold)
            .expect("必须有金币落地（掉落预算）");
        let expected_count = gold.count;
        assert!(expected_count > 0, "金币数应为正：{gold:?}");
        // 把角色移到金币格（`CM_PICKUP` 的位置门 `CurrX==nParam2 && CurrY==nParam3` 与
        // `map.get_item(CurrX,CurrY)` 都要对上；世界格未参与本接缝判定）。
        {
            let e = world.entities.get_mut(p).expect("player 必在实体表");
            e.x = gold.x;
            e.y = gold.y;
        }

        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert(
            "hero".into(),
            SessionGround {
                state: game_svr::ground::GroundState {
                    item_list: Vec::new(),
                    gold: 0,
                    wabil: test_ability(),
                },
                actor_id: p,
                game_gold: 0,
                sys_msg: None,
                use_items: vec![None; game_svr::ability::USE_ITEMS_LEN],
                sub: game_svr::ability::SubAbility::first_call(),
                equip: test_equip_inputs(),
                recovery: game_svr::recover::Recovery::new(0),
                stamina: game_svr::stamina::Stamina::new(0),
                show_hp: false,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
                save_rcd_tick: 0,
            },
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "hero".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: p,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let cfg = GroundCfg {
            human_max_gold: 10_000_000,
            floor_item_can_pick_up_time: 120_000,
            control_drop_item: false,
            in_safe_disable_drop: false,
            can_drop_price: 500,
            drop_item_rage: 3,
        };
        let colors = MsgColors {
            red_f: 255,
            red_b: 56,
            hear_f: 0,
            hear_b: 255,
        };
        let catalog = game_svr::ability::StdItemCatalog::new();

        // 位置门不成立（X 差 1）⇒ 不投递。
        handle_ground_cmd(
            game_svr::ground::CM_PICKUP,
            u16::try_from(gold.x + 1).unwrap(),
            u16::try_from(gold.y).unwrap(),
            None,
            "hero",
            901,
            &world,
            &mut sessions,
            &online,
            &gates,
            &cfg,
            &colors,
            &catalog,
            None,
        );
        assert!(
            matches!(rx0.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "位置门不成立 ⇒ 不得投递（oracle PlayObject.Message.cs:811-816）"
        );
        assert_eq!(
            sessions.get("hero").unwrap().state.gold,
            0,
            "位置门不成立 ⇒ 金币不得变"
        );

        // 位置门成立 ⇒ 拾取成功，回 SM_GOLDCHANGED（用改后 Gold），且会话金币增加。
        handle_ground_cmd(
            game_svr::ground::CM_PICKUP,
            u16::try_from(gold.x).unwrap(),
            u16::try_from(gold.y).unwrap(),
            None,
            "hero",
            901,
            &world,
            &mut sessions,
            &online,
            &gates,
            &cfg,
            &colors,
            &catalog,
            None,
        );
        let body = match rx0.try_recv() {
            Ok(SimToGate::SendFrame { body, socket }) => {
                assert_eq!(socket, 901, "回帧必须发给该用户的 socket");
                body
            }
            other => panic!("应收到 SM_GOLDCHANGED，实收 {other:?}"),
        };
        let cm = mir2_protocol::frame::CommandMessage::from_bytes(&body).expect("body >= 12B");
        assert_eq!(cm.ident, mir2_protocol::messages::SM_GOLDCHANGED);
        assert_eq!(
            cm.recog, expected_count,
            "SM_GOLDCHANGED 的 Recog = 拾取后的 Gold（不是改前值）"
        );
        assert_eq!(
            sessions.get("hero").unwrap().state.gold,
            expected_count,
            "金币必须写进会话态 GroundState"
        );
        assert!(
            matches!(rx0.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "一次拾取只应回一帧"
        );
    }

    /// **N-1（T-18 调用点接线）**：`CM_TAKEOFFITEM` 成功分支的重算必须传**调用前的** `SubAbility`
    /// （`sg.sub`），**不是** `SubAbility::first_call()`——oracle `RecalcAbilitys` 不重置
    /// `PoisonRecover/HealthRecover/SpellRecover`（`BaseObject.Abil.cs:30-31`）⇒ 重复调用累加。
    ///
    /// 造：会话里 `sub.poison_recover = 7`（登录时首次重算的遗留值），卸下一件无 recover 属性的物品
    /// ⇒ `take_off` 成功（`SM_TAKEOFF_OK(619)` + oracle 既有形态的 `SM_TAKEOFF_FAIL(620)`）、
    /// `recalc=true`；重算后的 `sub.poison_recover` 必须**还是 7**（prior 累加 + add=0）。
    ///
    /// 红检：把 `recalc_abilitys_with_prior(catalog, &inputs, &sg.sub)` 的 `&sg.sub` 换成
    /// `&game_svr::ability::SubAbility::first_call()` ⇒ `sub.poison_recover` 断言必红。
    #[test]
    fn bag_takeoff_recalc_uses_prior_subability() {
        use std::collections::HashMap;

        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            1,
            mir2_data::StdItem {
                name: "木剑".into(),
                std_mode: 5,
                ..Default::default()
            },
        );

        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut use_items: Vec<Option<game_svr::bag::BagItem>> =
            vec![None; game_svr::ability::USE_ITEMS_LEN];
        use_items[0] = Some(game_svr::bag::BagItem {
            make_index: 60001,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: [0u8; 14],
        });
        sessions.insert(
            "hero".into(),
            SessionGround {
                state: game_svr::ground::GroundState {
                    item_list: Vec::new(),
                    gold: 123,
                    wabil: test_ability(),
                },
                actor_id: 42,
                game_gold: 0,
                sys_msg: None,
                use_items,
                // 登录时首次 `RecalcAbilitys` 留下的 recover（非 0 ⇒ 可实测「传 prior」）。
                sub: game_svr::ability::SubAbility {
                    anti_magic: 1,
                    hit_point: 8,
                    speed_point: 9,
                    anti_poison: 0,
                    poison_recover: 7,
                    health_recover: 0,
                    spell_recover: 0,
                },
                equip: test_equip_inputs(),
                recovery: game_svr::recover::Recovery::new(0),
                stamina: game_svr::stamina::Stamina::new(0),
                show_hp: false,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
                save_rcd_tick: 0,
            },
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "hero".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: 42,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        // actor 42 不在世界里 ⇒ `FeatureChanged()` 广播无收件人（本用例只断言 52/752 以前的出帧序）。
        let world = mir2_world::World::new(8, 8);
        let mut displays = downlink::DisplayTable::new();

        // `recog=60001`（物品 `MakeIndex`）、`param=0`（槽位）、`tag` 未用、`name` 尾（C-1）。
        handle_bag_cmd(
            game_svr::bag::CM_TAKEOFFITEM,
            60001,
            0,
            0,
            Some("木剑"),
            "hero",
            901,
            &online,
            &mut sessions,
            &gates,
            &catalog,
            &world,
            &mut displays,
            12,
        );

        // 出帧：SM_ADDITEM(200，换下回背包) → SM_TAKEOFF_OK(619) → SM_TAKEOFF_FAIL(620，oracle
        // 既有形态) → SM_ABILITY(52) → SM_SUBABILITY(752)。
        let mut idents = Vec::new();
        while let Ok(SimToGate::SendFrame { socket, body }) = rx0.try_recv() {
            assert_eq!(socket, 901, "回帧必须发给该用户的 socket");
            let cm = mir2_protocol::frame::CommandMessage::from_bytes(&body).expect("body >= 12B");
            idents.push(cm.ident);
        }
        assert_eq!(
            idents,
            vec![
                200,
                619,
                620,
                mir2_protocol::messages::SM_ABILITY,
                mir2_protocol::messages::SM_SUBABILITY
            ],
            "出帧序：SM_ADDITEM → SM_TAKEOFF_OK/FAIL → SM_ABILITY → SM_SUBABILITY"
        );

        let sg = sessions.get("hero").unwrap();
        assert!(sg.use_items[0].is_none(), "卸下成功 ⇒ 槽位必须清空");
        assert_eq!(sg.state.item_list.len(), 1, "物品回背包");
        assert_eq!(
            sg.sub.poison_recover, 7,
            "重算必须传 prior（T-18）：prior.poison_recover=7 + add=0 ⇒ 7；\
             若误用 first_call() ⇒ 0 ⇒ 必红"
        );
    }

    /// **C-1（reviewer-3 `5df38bad`，owner 裁决 `54182d61`）**：`CM_TAKEONITEM`/`CM_TAKEOFFITEM` 的
    /// 字段口径必须与 oracle 一致——`Recog`(b0-3) = 物品 `MakeIndex`、`Param`(b6-7) = 槽位（`(byte)` 低 8 位）、
    /// `Tag`(b8-9) 未用。
    ///
    /// 造一件在 `UseItems[0]` 的物品（`MakeIndex=60001`），把请求的 `Tag` 设成**诱饵槽位** `7`：
    /// 只有 oracle 口径（slot=param=0、make_index=recog=60001）才命中并卸下成功；若按旧/错位口径
    /// （slot=tag=7 或 make_index=param=0）⇒ 槽位/物品对不上 ⇒ 走 FAIL ⇒ 本用例必红。
    #[test]
    fn bag_cmd_field_order_is_recog_makeindex_param_slot() {
        use std::collections::HashMap;

        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            1,
            mir2_data::StdItem {
                name: "木剑".into(),
                std_mode: 5,
                ..Default::default()
            },
        );

        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut use_items: Vec<Option<game_svr::bag::BagItem>> =
            vec![None; game_svr::ability::USE_ITEMS_LEN];
        use_items[0] = Some(game_svr::bag::BagItem {
            make_index: 60001,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: [0u8; 14],
        });
        sessions.insert(
            "hero".into(),
            SessionGround {
                state: game_svr::ground::GroundState {
                    item_list: Vec::new(),
                    gold: 0,
                    wabil: test_ability(),
                },
                actor_id: 42,
                game_gold: 0,
                sys_msg: None,
                use_items,
                sub: game_svr::ability::SubAbility::first_call(),
                equip: test_equip_inputs(),
                recovery: game_svr::recover::Recovery::new(0),
                stamina: game_svr::stamina::Stamina::new(0),
                show_hp: false,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
                save_rcd_tick: 0,
            },
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "hero".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: 42,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let world = mir2_world::World::new(8, 8);
        let mut displays = downlink::DisplayTable::new();

        // recog=60001（MakeIndex）、param=0（真槽位）、tag=7（诱饵，oracle 不读）。
        handle_bag_cmd(
            game_svr::bag::CM_TAKEOFFITEM,
            60001,
            0,
            7,
            Some("木剑"),
            "hero",
            901,
            &online,
            &mut sessions,
            &gates,
            &catalog,
            &world,
            &mut displays,
            12,
        );

        let sg = sessions.get("hero").unwrap();
        assert!(
            sg.use_items[0].is_none(),
            "oracle 口径：slot=param=0、make_index=recog=60001 ⇒ 必须命中并卸下"
        );
        assert_eq!(sg.state.item_list.len(), 1, "卸下的物品进背包");
        let mut idents = Vec::new();
        while let Ok(SimToGate::SendFrame { body, .. }) = rx0.try_recv() {
            let cm = mir2_protocol::frame::CommandMessage::from_bytes(&body).expect(">=12B");
            idents.push(cm.ident);
        }
        assert!(
            idents.contains(&619),
            "必须出现 SM_TAKEOFF_OK(619)（字段错位只会走 FAIL）：{idents:?}"
        );
    }

    /// **O-1（reviewer-6 对 N-1 片的 finding）**：装备/卸下成功后的 `FeatureChanged()` 必须向
    /// 同图 `SendRefMsgRange`(缺省 12) 格内的玩家（**含自己**）广播 `SM_FEATURECHANGED(41)`。
    ///
    /// 造两个玩家 `hero`(10,10)/`watcher`(12,10) 同图，`hero` 卸下一件武器 ⇒ 成功分支
    /// （`out.recalc`）后：`hero` 线上出帧序 `200 → 619 → 620 → 52 → 752 → 41`，
    /// `watcher` 只收 `41`（`recog=hero`，`param/tag` = 卸下后的 `GetFeatureToLong()`）。
    ///
    /// 红检：注释掉 `broadcast_feature_changed(...)` ⇒ `watcher` 线收不到 `41` ⇒ 必红；
    /// 把自己从收件人里过滤掉（`e.actor_id != actor_id`）⇒ `hero` 的末条不再为 `41` ⇒ 必红。
    #[test]
    fn bag_equip_broadcasts_feature_changed_to_nearby() {
        use mir2_world::Entity;
        use std::collections::HashMap;

        let mut world = mir2_world::World::new(64, 64);
        world.open_session("hero");
        world.open_session("watcher");
        let hero = world
            .enter_map("hero", Entity::new(0, "hero", 10, 10), 0)
            .expect("hero 进图");
        let watcher = world
            .enter_map("watcher", Entity::new(0, "watcher", 12, 10), 0)
            .expect("watcher 进图");
        world.tick();

        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            1,
            mir2_data::StdItem {
                name: "木剑".into(),
                std_mode: 5,
                ..Default::default()
            },
        );

        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut use_items: Vec<Option<game_svr::bag::BagItem>> =
            vec![None; game_svr::ability::USE_ITEMS_LEN];
        use_items[0] = Some(game_svr::bag::BagItem {
            make_index: 60001,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: [0u8; 14],
        });
        sessions.insert(
            "hero".into(),
            SessionGround {
                state: game_svr::ground::GroundState {
                    item_list: Vec::new(),
                    gold: 123,
                    wabil: test_ability(),
                },
                actor_id: hero,
                game_gold: 0,
                sys_msg: None,
                use_items,
                sub: game_svr::ability::SubAbility::first_call(),
                equip: test_equip_inputs(),
                recovery: game_svr::recover::Recovery::new(0),
                stamina: game_svr::stamina::Stamina::new(0),
                show_hp: false,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
                save_rcd_tick: 0,
            },
        );

        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "hero".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: hero,
            },
        );
        online_set(
            &mut online,
            OnlineEntry {
                account: "uw".into(),
                chr_name: "watcher".into(),
                session_id: 2,
                gate_idx: 1,
                socket: 902,
                actor_id: watcher,
            },
        );

        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);

        let mut displays = downlink::DisplayTable::new();
        displays.set(
            hero,
            downlink::ActorDisplay::new(0, 0, 111, 0, 0, 255, b"hero".to_vec()),
        );

        // `recog=60001`（物品 `MakeIndex`）、`param=0`（槽位）、`tag` 未用（C-1）。
        handle_bag_cmd(
            game_svr::bag::CM_TAKEOFFITEM,
            60001,
            0,
            0,
            Some("木剑"),
            "hero",
            901,
            &online,
            &mut sessions,
            &gates,
            &catalog,
            &world,
            &mut displays,
            12,
        );

        let collect = |rx: &mut mpsc::UnboundedReceiver<SimToGate>| -> Vec<(i32, u16, u16, u16)> {
            let mut out = Vec::new();
            while let Ok(SimToGate::SendFrame { body, .. }) = rx.try_recv() {
                let cm =
                    mir2_protocol::frame::CommandMessage::from_bytes(&body).expect("body >= 12B");
                out.push((cm.recog, cm.ident, cm.param, cm.tag));
            }
            out
        };
        assert_eq!(
            collect(&mut rx0).iter().map(|f| f.1).collect::<Vec<_>>(),
            vec![
                200,
                619,
                620,
                mir2_protocol::messages::SM_ABILITY,
                mir2_protocol::messages::SM_SUBABILITY,
                mir2_protocol::messages::SM_FEATURECHANGED,
            ],
            "hero 线上出帧序：200 → 619 → 620 → 52 → 752 → 41"
        );
        let watcher_frames = collect(&mut rx1);
        assert_eq!(
            watcher_frames.len(),
            1,
            "watcher 只应收一条 SM_FEATURECHANGED"
        );
        assert_eq!(
            watcher_frames[0].1,
            mir2_protocol::messages::SM_FEATURECHANGED
        );
        assert_eq!(watcher_frames[0].0, hero, "41 的 Recog = 换装者 ActorId");
        // `SM_FEATURECHANGED` 的 Param/Tag = LoWord/HiWord(GetFeatureToLong())（oracle Message.cs:1762-1764）。
        let feature = i32::from(watcher_frames[0].2) | (i32::from(watcher_frames[0].3) << 16);
        assert_eq!(
            displays.get(hero).unwrap().feature,
            feature,
            "展示表必须用换装后的 GetFeatureToLong 更新"
        );
        assert_ne!(feature, 111, "卸下后 feature 必须变化（不是换装前的值）");
    }

    /// `bag_items_from`（`CharacterData.bag_items` ⇒ `GroundState.item_list`）：展平非空槽、
    /// `desc` 截断/补零到定长 14B（oracle `UserItem.Desc`）。
    #[test]
    fn bag_items_from_flattens_and_pads_desc() {
        let item = |make_index: i32, index: u16, desc: Option<Vec<u8>>| record::ServerUserItem {
            make_index,
            index,
            dura: 11,
            dura_max: 22,
            desc,
            color_r: 0,
            color_g: 0,
            color_b: 0,
            prefix: None,
        };
        let items = vec![
            None,
            Some(item(70001, 3, Some(vec![1, 2, 3]))),
            None,
            Some(item(70002, 4, None)),
            // oracle `WorldServer.cs:1188` 的 `Index > 0` 门：空槽（Index=0）不进 ItemList。
            Some(item(70003, 0, None)),
        ];
        let bags = bag_items_from(Some(&items));
        assert_eq!(
            bags.len(),
            2,
            "空槽与 Index=0 槽必须被展平（oracle ItemList 是非空列表）"
        );
        assert_eq!(bags[0].make_index, 70001);
        assert_eq!(bags[0].index, 3);
        assert_eq!(&bags[0].desc[..3], &[1, 2, 3]);
        assert!(bags[0].desc[3..].iter().all(|b| *b == 0), "desc 补齐到 14B");
        assert_eq!(bags[1].desc, [0u8; 14], "无 desc ⇒ 全 0");
    }

    /// **本片真机收口**：网关线号必须**按连接唯一分配**（oracle `AllocGateSlot`）。
    ///
    /// 真机症状：三条 GameGate 连接都拿到线号 0（旧实现 `let active_service_id = 0`）⇒ `gates`
    /// 表只剩最后一条，进世界帧路由错连接、`UserList` 找不到 socket ⇒ 丢弃顶包、客户端黑屏。
    /// 红检：把分配改回恒 0（`alloc()` 只返回 `Some(0)` / 直接 `let service_id = 0;`）⇒ 本用例
    /// 第二条断言（两个连接线号不同）必红。
    #[test]
    fn gate_slots_are_unique_per_connection_and_reusable() {
        let mut s = GateSlots::default();
        assert_eq!(s.alloc(), Some(0));
        assert_eq!(s.alloc(), Some(1));
        assert_eq!(s.alloc(), Some(2));
        assert_eq!(s.alloc(), Some(3));
        // 释放中间一个 ⇒ 复用最小空闲
        s.free(1);
        assert_eq!(s.alloc(), Some(1));
        // 释放不存在的槽不出错
        s.free(200);
        // 满 20 槽 ⇒ 拒绝
        let mut full = GateSlots::default();
        for i in 0..GATE_SLOT_CAPACITY {
            assert_eq!(full.alloc(), Some(u8::try_from(i).unwrap()));
        }
        assert_eq!(full.alloc(), None, "满 20 槽后必须拒绝（oracle gateIdx<0）");
    }

    /// **`openmir2-m2-session-lifecycle` ③**：同一角色重复进世界必须 **踢旧 + 关新**，
    /// 照 oracle `WorldServer.cs:503/535/536-537/568-570`。
    ///
    /// 判据（全自动、无网络）：
    /// - 新连接（gate=1/socket=902）收到 `KickUser`（`:568-570` `NetChannel.CloseUser(新)`）；
    /// - 旧连接（gate=0/socket=901）下一 tick 先收 `SendFrame`（`SM_OUTOFCONNECTION`=528、header-only），
    ///   再收 `KickUser`（`RunNotice` 的 `BoKickFlag` 支 + `MakeGhost`）；
    /// - 旧会话离开世界、实体被摘。
    ///
    /// 红检：把 `enter_map` 的 `AlreadyInWorld` 分支改回只记 WRN（现状）⇒ 本用例第一条断言即红。
    #[test]
    fn duplicate_enter_kicks_old_and_closes_new() {
        use std::collections::HashMap;
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);

        // 旧会话已在世界里
        let mut world = mir2_world::World::with_maps(vec![(64, 64)]);
        world.open_session("lb1a");
        let ent = mir2_world::Entity::new(0, "lb1a".to_string(), 10, 20);
        let actor = world.enter_map("lb1a", ent, 0).expect("旧会话应先在世界里");
        let mut online: OnlineRegistry = Vec::new();
        online_set(&mut online, entry("acct1", "lb1a", 77, 0, 901, actor));

        // 新连接（另一条网关线 1 / socket 902）重复进世界
        let mut kick_pending: Vec<String> = Vec::new();
        handle_duplicate_enter(&gates, &mut kick_pending, "lb1a", 1, 902, true);
        match rx1.try_recv().expect("新连接必须收到 KickUser") {
            SimToGate::KickUser { socket } => assert_eq!(socket, 902),
            other => panic!("新连接第一件事必须是 KickUser，实得 {other:?}"),
        }
        // 旧连接此时还没被踢（oracle 是置标志、下一 tick 落地）
        assert!(matches!(
            rx0.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));

        // 下一 tick：踢旧落地
        drain_kicks(
            &mut world,
            &gates,
            &mut online,
            &mut kick_pending,
            &mut None,
            &mut load::FrontEngine::new(),
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashMap::new(),
            None,
        );
        match rx0.try_recv().expect("旧连接必须收到 SM_OUTOFCONNECTION") {
            SimToGate::SendFrame { socket, body } => {
                assert_eq!(socket, 901);
                assert_eq!(body.len(), 12, "528 是 header-only");
                assert_eq!(
                    u16::from_le_bytes([body[4], body[5]]),
                    528,
                    "SM_OUTOFCONNECTION"
                );
            }
            other => panic!("旧连接第一帧必须是 SendFrame(528)，实得 {other:?}"),
        }
        match rx0.try_recv().expect("旧连接随后必须被清槽") {
            SimToGate::KickUser { socket } => assert_eq!(socket, 901),
            other => panic!("旧连接第二件事必须是 KickUser，实得 {other:?}"),
        }
        // 旧会话确实离开世界、实体被摘（MakeGhost）
        assert_eq!(
            world.session("lb1a").unwrap().stage,
            mir2_world::WorldStage::Left
        );
        assert!(
            world.entities.get(actor).is_none(),
            "MakeGhost 必须摘掉旧实体"
        );
        assert!(online.is_empty(), "在线登记必须清空");
    }

    /// 踢旧善后必须**幂等**；没有在线坐标时只记 WRN（不 panic、不改世界、名单被消费）。
    #[test]
    fn drain_kicks_is_idempotent_and_safe_without_coords() {
        use std::collections::HashMap;
        let gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        let mut world = mir2_world::World::with_maps(vec![(64, 64)]);
        let mut online: OnlineRegistry = Vec::new();
        let mut kick_pending = vec!["ghost".to_string()];
        drain_kicks(
            &mut world,
            &gates,
            &mut online,
            &mut kick_pending,
            &mut None,
            &mut load::FrontEngine::new(),
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashMap::new(),
            None,
        );
        assert!(kick_pending.is_empty(), "待踢名单必须被消费掉（幂等）");
        assert!(world.session("ghost").is_none());
    }

    fn entry(account: &str, chr: &str, sid: i32, gate: i32, sock: i32, actor: i32) -> OnlineEntry {
        OnlineEntry {
            account: account.to_string(),
            chr_name: chr.to_string(),
            session_id: sid,
            gate_idx: gate,
            socket: sock,
            actor_id: actor,
        }
    }

    /// 在线登记在**进世界成功**时落坐标，断线/踢旧时被 `online_take` 拿走（名字精确匹配）。
    #[test]
    fn online_registry_set_and_take() {
        let mut online: OnlineRegistry = Vec::new();
        online_set(&mut online, entry("a", "lb1a", 1, 0, 901, 5));
        online_set(&mut online, entry("a", "lb1a", 1, 2, 903, 5)); // 同名刷新（重进世界）只有一条
        assert_eq!(online.len(), 1);
        assert_eq!(online_take(&mut online, "lb1a").unwrap().socket, 903);
        assert!(online_take(&mut online, "lb1a").is_none());
        assert!(online.is_empty());
    }

    /// **`CM_SAY` 普通说话**：与 oracle `SendRefMsg`（`BaseObject.Message.cs:395-440`）同集合 ——
    /// ±`SendRefMsgRange`(12) 格内、同图、`Race==Play && !Ghost` 的 actor，**含说话者自己**。
    /// 帧 = 12B 头（`Recog=说话者 ActorId`、`Ident=SM_HEAR(40)`、`Param=MakeWord(F,B)`、`Tag=0`、
    /// `Series=1`）+ 尾 `EncodeString("aaa:hi")`。
    ///
    /// 红检：把 `chat::hear_targets` 的"含自己"去掉（或把 `sm_hear` 的 `Series` 改 0 / `Ident` 改错）
    /// ⇒ 本用例必红。
    #[test]
    fn say_broadcasts_hear_to_self_and_near_players_only() {
        use std::collections::HashMap;
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let (tx2, mut rx2) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);
        gates.insert(2, tx2);

        let mut world = mir2_world::World::with_maps(vec![(64, 64), (64, 64)]);
        let mut mk = |name: &str, x: i32, y: i32, map_id: usize| {
            let mut e = mir2_world::Entity::new(0, name.to_string(), x, y);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.map_id = map_id;
            world.add_entity(e)
        };
        let a = mk("aaa", 10, 10, 0); // 说话者
        let b = mk("bbb", 12, 12, 0); // dx=dy=12 ⇒ 边界内
        let c = mk("ccc", 11, 11, 1); // 另一张图 ⇒ 不收

        let mut online: OnlineRegistry = Vec::new();
        online_set(&mut online, entry("a", "aaa", 1, 0, 901, a));
        online_set(&mut online, entry("b", "bbb", 2, 1, 902, b));
        online_set(&mut online, entry("c", "ccc", 3, 2, 903, c));

        handle_say(
            &online,
            &world,
            &gates,
            "aaa",
            901,
            Some("hi"),
            &MsgColors {
                red_f: 0xFF,
                red_b: 0x38,
                hear_f: 0x00,
                hear_b: 0xFF,
            },
        );

        let check =
            |rx: &mut mpsc::UnboundedReceiver<SimToGate>, want_sock: i32| match rx.try_recv() {
                Ok(SimToGate::SendFrame { socket, body }) => {
                    assert_eq!(socket, want_sock);
                    assert_eq!(body.len(), 12 + 8, "12B 头 + Encode(\"aaa:hi\")(8)");
                    assert_eq!(
                        i32::from_le_bytes([body[0], body[1], body[2], body[3]]),
                        a,
                        "Recog = 说话者 ActorId（BaseObject.Message.cs:154）"
                    );
                    assert_eq!(
                        u16::from_le_bytes([body[4], body[5]]),
                        mir2_protocol::messages::SM_HEAR,
                        "Ident = SM_HEAR(40)"
                    );
                    assert_eq!(
                        u16::from_le_bytes([body[6], body[7]]),
                        0xFF00,
                        "Param = MakeWord(FColor=0x00, BColor=0xFF)"
                    );
                    assert_eq!(u16::from_le_bytes([body[8], body[9]]), 0, "Tag=0");
                    assert_eq!(u16::from_le_bytes([body[10], body[11]]), 1, "Series=1");
                    assert_eq!(
                        mir2_protocol::edcode::decode(&body[12..]),
                        b"aaa:hi".to_vec(),
                        "尾 = EncodeString(ChrName + ':' + sMsg)"
                    );
                }
                _ => panic!("socket={want_sock} 未收到 SM_HEAR"),
            };
        check(&mut rx0, 901); // 自己（客户端靠这条回声渲染自己发的话）
        check(&mut rx1, 902); // 边界内近邻
        assert!(rx2.try_recv().is_err(), "另一张图的角色不该收到普通说话");
    }

    /// **`openmir2-m2-session-lifecycle` ①**：账号级顶号（`SS_CLOSESESSION` ⇒ `KickUser`）：
    ///
    /// - 旧连接**立即**收到 `SM_SYSMESSAGE`（ident 100，**不是** 528），`Recog` = 旧 ActorId，
    ///   `Param` = `make_word(RedMsgFColor, RedMsgBColor)`（oracle `BaseObject.cs:1678` 的 Red 支），
    ///   尾 = 账号级提示文案（GBK），`Series` = 1；
    /// - 同一 tick 后段（`drain_soft_close`）才 `KickUser` 清槽 + 离开世界（oracle 是下一 tick；
    ///   `BoSoftClose` 的 `MakeGhost`，见 docs §4 的 O-1 登记）；
    /// - 匹配允许"`Account` 相等（大小写不敏感）**或** `SessionID` 相等"（`TCPNetChannel.cs:252`）。
    ///
    /// 红检：把 `sm_sysmessage` 改回发 528（或把 `ident` 改错、把 `param` 的高低字节反）⇒ 本用例必红。
    #[test]
    fn account_kick_sends_sysmessage_then_soft_closes() {
        use std::collections::HashMap;
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        let mut world = mir2_world::World::with_maps(vec![(64, 64)]);
        world.open_session("lb1a");
        let ent = mir2_world::Entity::new(0, "lb1a".to_string(), 10, 20);
        let actor = world.enter_map("lb1a", ent, 0).expect("旧会话应在世界里");
        let mut online: OnlineRegistry = Vec::new();
        online_set(&mut online, entry("Acct1", "lb1a", 77, 0, 901, actor));

        let mut engine = load::FrontEngine::new();
        let mut soft_close_pending: Vec<String> = Vec::new();
        // 用**大小写不同**的 account 命中（OrdinalIgnoreCase）；session_id 故意传不存在的值。
        handle_kick_account(
            &mut engine,
            &gates,
            &online,
            &mut soft_close_pending,
            MsgColors {
                red_f: 0xFF,
                red_b: 0x38,
                hear_f: 0x00,
                hear_b: 0xFF,
            },
            "acct1",
            424242,
        );
        match rx0.try_recv().expect("旧连接必须立即收到 SM_SYSMESSAGE") {
            SimToGate::SendFrame { socket, body } => {
                assert_eq!(socket, 901);
                assert!(body.len() > 12, "SM_SYSMESSAGE 有尾（不是 header-only）");
                assert_eq!(
                    u16::from_le_bytes([body[4], body[5]]),
                    mir2_protocol::messages::SM_SYSMESSAGE,
                    "ident 必须是 100（不是 528）"
                );
                assert_eq!(
                    i32::from_le_bytes([body[0], body[1], body[2], body[3]]),
                    actor,
                    "Recog = 旧 PlayObject 的 ActorId"
                );
                assert_eq!(
                    u16::from_le_bytes([body[6], body[7]]),
                    smframe::make_word(0xFF, 0x38),
                    "Param = make_word(RedMsgFColor, RedMsgBColor)"
                );
                assert_eq!(u16::from_le_bytes([body[10], body[11]]), 1, "Series 恒 1");
                let text = mir2_protocol::edcode::decode(&body[12..]);
                assert_eq!(text, KICK_USER_MSG, "尾必须是账号级顶号文案（GBK 原字节）");
            }
            other => panic!("旧连接第一帧必须是 SendFrame(SM_SYSMESSAGE)，实得 {other:?}"),
        }
        // 旧会话此时还在世界（oracle 是置标志、下一 tick 才 MakeGhost）
        assert_eq!(
            world.session("lb1a").unwrap().stage,
            mir2_world::WorldStage::InWorld
        );
        // 下一 tick 落地
        drain_soft_close(
            &mut world,
            &gates,
            &mut online,
            &mut soft_close_pending,
            &mut None,
            &mut load::FrontEngine::new(),
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashMap::new(),
            None,
        );
        assert_eq!(soft_close_pending.len(), 0, "待软关名单被消费");
        match rx0.try_recv().expect("随后必须清槽（CloseUser）") {
            SimToGate::KickUser { socket } => assert_eq!(socket, 901),
            other => panic!("第二件事必须是 KickUser，实得 {other:?}"),
        }
        assert_eq!(
            world.session("lb1a").unwrap().stage,
            mir2_world::WorldStage::Left,
            "MakeGhost：离开世界"
        );
        assert!(online.is_empty(), "在线登记被摘");
    }

    /// **`openmir2-m2-session-lifecycle` O-3（owner `cb0c70fd`）**：账号级顶号按 oracle
    /// `TCPNetChannel.KickUser`「**每个网关各扫、各 `break`**」（`TCPNetChannel.cs:246-274`）——
    /// 同一账号经两个网关各有一条会话时，**两个都被顶（每网关各一个）**，不是只踢全局首个。
    ///
    /// 构造：同账号 `acct1` 在网关 0（`lb1a`/socket 901）与网关 1（`lb1b`/socket 902）各一条会话。
    /// 断言：两条都收到 `SM_SYSMESSAGE`、两条都在软关名单；`drain_soft_close` 后两条都离开世界、登记清空。
    ///
    /// 红检：把 `online_find_for_kick` 改回「全局首个」（如 `targets.truncate(1)`）⇒ 网关 1 的会话
    /// 收不到 `SM_SYSMESSAGE` ⇒ 本用例必 FAILED。
    #[test]
    fn account_kick_targets_every_gateway_once() {
        use std::collections::HashMap;
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);

        let mut world = mir2_world::World::with_maps(vec![(64, 64)]);
        let mut online: OnlineRegistry = Vec::new();
        for (chr, gate, sock) in [("lb1a", 0_i32, 901_i32), ("lb1b", 1, 902)] {
            world.open_session(chr);
            let ent = mir2_world::Entity::new(0, chr.to_string(), 10, 20);
            let actor = world.enter_map(chr, ent, 0).expect("会话进世界");
            online_set(&mut online, entry("acct1", chr, 7, gate, sock, actor));
        }
        assert_eq!(online.len(), 2, "两台网关各一条会话");

        let mut engine = load::FrontEngine::new();
        let mut soft_close_pending: Vec<String> = Vec::new();
        // 同账号命中两条（`Account` OrdinalIgnoreCase）；`session_id` 故意传不存在的值。
        handle_kick_account(
            &mut engine,
            &gates,
            &online,
            &mut soft_close_pending,
            MsgColors {
                red_f: 0xFF,
                red_b: 0x38,
                hear_f: 0x00,
                hear_b: 0xFF,
            },
            "acct1",
            0,
        );

        for (rx, sock, who) in [(&mut rx0, 901, "lb1a"), (&mut rx1, 902, "lb1b")] {
            match rx
                .try_recv()
                .unwrap_or_else(|_| panic!("{who} 必须收到 SM_SYSMESSAGE（每网关各一个）"))
            {
                SimToGate::SendFrame { socket, body } => {
                    assert_eq!(socket, sock, "{who} 的提示必须发给自己的 socket");
                    assert_eq!(
                        u16::from_le_bytes([body[4], body[5]]),
                        mir2_protocol::messages::SM_SYSMESSAGE
                    );
                }
                other => panic!("{who} 第一帧必须是 SM_SYSMESSAGE，实得 {other:?}"),
            }
        }
        assert_eq!(
            soft_close_pending.len(),
            2,
            "两条会话都要软关（每网关各一个）"
        );
        drain_soft_close(
            &mut world,
            &gates,
            &mut online,
            &mut soft_close_pending,
            &mut None,
            &mut load::FrontEngine::new(),
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashMap::new(),
            None,
        );
        assert_eq!(
            world.session("lb1a").unwrap().stage,
            mir2_world::WorldStage::Left
        );
        assert_eq!(
            world.session("lb1b").unwrap().stage,
            mir2_world::WorldStage::Left
        );
        assert!(online.is_empty(), "两条会话都被摘");
    }

    /// `KICK_USER_MSG` 必须就是 oracle 文案的 GB2312 字节（防手抄错）。
    #[test]
    fn kick_user_msg_is_oracle_text_in_gb2312() {
        let (bytes, _, _) =
            encoding_rs::GBK.encode("当前登录帐号正在其它位置登录，本机已被强行离线!!!");
        assert_eq!(&bytes[..], KICK_USER_MSG);
    }

    /// **`openmir2-m2-session-lifecycle` ②**：`ProcessHumansIsLogined` 的两条判据
    /// （`WorldServer.cs:200-217`）——
    ///
    /// - 旧**在世界**：重复进世界 ⇒ 关新 +（下一 tick）踢旧（发 528）；
    /// - 旧**已离场（不在世界、也不在保存中）**：放行（`enter_map` 成功）。
    ///
    /// 这就是夹具实测的轮 A（放行）与轮 B（关新）；本用例证明两支都**可达**（不是钉死某一支）。
    #[test]
    fn duplicate_login_race_has_both_reachable_branches() {
        use std::collections::HashMap;
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);

        let mut world = mir2_world::World::with_maps(vec![(64, 64)]);
        world.open_session("lb1a");
        let ent = mir2_world::Entity::new(0, "lb1a".to_string(), 10, 20);
        let actor = world.enter_map("lb1a", ent, 0).unwrap();
        let mut online: OnlineRegistry = Vec::new();
        online_set(&mut online, entry("a", "lb1a", 1, 0, 901, actor));
        let mut engine = load::FrontEngine::new();

        // 支路 1（轮 B）：旧在世界 ⇒ 关新（不再走 enter_map）+ 踢旧落地。
        let mut kick_pending = Vec::new();
        handle_duplicate_enter(&gates, &mut kick_pending, "lb1a", 1, 902, true);
        match rx1.try_recv().expect("轮 B：新连接被关") {
            SimToGate::KickUser { socket } => assert_eq!(socket, 902),
            other => panic!("轮 B 新连接必须被 KickUser，实得 {other:?}"),
        }
        drain_kicks(
            &mut world,
            &gates,
            &mut online,
            &mut kick_pending,
            &mut None,
            &mut load::FrontEngine::new(),
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashMap::new(),
            None,
        );
        let _ = rx0.try_recv(); // 528
        let _ = rx0.try_recv(); // KickUser
        assert_eq!(
            world.session("lb1a").unwrap().stage,
            mir2_world::WorldStage::Left
        );
        assert!(
            !engine.in_save_rcd_list("lb1a") && online.is_empty(),
            "轮 B 后旧已离场"
        );

        // 支路 2（轮 A）：旧已离场 ⇒ 放行。
        let ent2 = mir2_world::Entity::new(0, "lb1a".to_string(), 11, 21);
        let id2 = world
            .enter_map("lb1a", ent2, 0)
            .expect("旧已离场 ⇒ 新必须放行");
        online_set(&mut online, entry("a", "lb1a", 2, 1, 902, id2));
        assert_eq!(
            world.session("lb1a").unwrap().stage,
            mir2_world::WorldStage::InWorld
        );

        // 支路 3：旧不在世界但**在保存中**（`InSaveRcdList`）⇒ 仍然关新（且旧不在世界 ⇒ 不踢旧）。
        engine.begin_save("lb1a");
        let mut kick_pending2 = Vec::new();
        handle_duplicate_enter(&gates, &mut kick_pending2, "lb1a", 0, 903, false);
        match rx0.try_recv().expect("保存中：新连接被关") {
            SimToGate::KickUser { socket } => assert_eq!(socket, 903),
            other => panic!("保存中必须关新，实得 {other:?}"),
        }
        assert!(
            kick_pending2.is_empty(),
            "旧不在 PlayObjectList ⇒ 不得置踢旧标志（夹具轮 B 旧未收到 528 的口径）"
        );
    }

    /// **issue `8d07694d` ②（CM_EAT 派发）**：`handle_eat_cmd` 派发到 `useitem::use_item`，
    /// 经该角色的 `[GateIdx,Socket]` 回 `SM_EAT_OK(635)` + `RM_HEALTHSPELLCHANGED→SM_HEALTHSPELLCHANGED(53)`
    /// + `RM_WEIGHTCHANGED→SM_WEIGHTCHANGED(622)`，并把吃掉后的背包/血量写回会话态。
    ///
    /// **牙齿**：`make_index` 必须从 12B 头 `Recog` 取（`ClientUseItems(processMsg.nParam1, Msg)`）。
    /// 本用例故意把 `param`/`tag` 传成**不是** `make_index` 的值（4242）——
    /// 若把派发改成从 `param` 读（或摘掉 `useitem::CM_EAT` 分支），`hit` 找不到 ⇒ 落 `SM_EAT_FAIL`
    /// ⇒ 本用例必红。
    #[test]
    fn eat_drug_dispatches_and_updates_session() {
        use mir2_protocol::frame::CommandMessage;
        use std::collections::HashMap;

        // 金创药(小量)：`StdMode 0` / `Shape 1`（固定量 `IncHealthSpell(AC, MAC)`）。
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            100,
            mir2_data::StdItem {
                name: "金创药(小量)".into(),
                std_mode: 0,
                shape: 1,
                ac: 10,
                mac: 0,
                ..Default::default()
            },
        );
        let world = mir2_world::World::new(32, 32);
        let make_index = 29_033_300;
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert(
            "hero".into(),
            SessionGround {
                state: game_svr::ground::GroundState {
                    item_list: vec![game_svr::bag::BagItem {
                        make_index,
                        index: 100,
                        dura: 0,
                        dura_max: 0,
                        desc: [0; 14],
                    }],
                    gold: 0,
                    wabil: {
                        let mut a = test_ability();
                        a.hp = 5;
                        a.max_hp = 20;
                        a.mp = 3;
                        a.max_mp = 10;
                        a
                    },
                },
                actor_id: 7,
                game_gold: 0,
                sys_msg: None,
                use_items: vec![None; game_svr::ability::USE_ITEMS_LEN],
                sub: game_svr::ability::SubAbility::first_call(),
                equip: test_equip_inputs(),
                recovery: game_svr::recover::Recovery::new(0),
                stamina: game_svr::stamina::Stamina::new(0),
                show_hp: false,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
                save_rcd_tick: 0,
            },
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "hero".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: 7,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let colors = MsgColors {
            red_f: 255,
            red_b: 56,
            hear_f: 0,
            hear_b: 255,
        };

        // `recog` = `make_index`；`param`/`tag` 故意错位（4242）—— 读数位置错就必红。
        handle_eat_cmd(
            make_index,
            4242,
            4242,
            None,
            "hero",
            901,
            &online,
            &mut sessions,
            &gates,
            &catalog,
            &world,
            &colors,
            None,
        );

        let mut frames: Vec<(i32, u16)> = Vec::new();
        while let Ok(SimToGate::SendFrame { body, .. }) = rx0.try_recv() {
            let cm = CommandMessage::from_bytes(&body).expect("SM_* body 至少 12B");
            frames.push((cm.recog, cm.ident));
        }
        assert_eq!(
            frames,
            vec![
                (0, game_svr::useitem::SM_EAT_OK),
                (7, mir2_protocol::messages::SM_HEALTHSPELLCHANGED),
                (0, mir2_protocol::messages::SM_WEIGHTCHANGED),
            ],
            "成功吃药：SM_EAT_OK + SM_HEALTHSPELLCHANGED + SM_WEIGHTCHANGED"
        );
        let sg = &sessions["hero"];
        assert!(
            sg.state.item_list.is_empty(),
            "吃掉的物品必须从 ItemList 移除"
        );
        assert_eq!(sg.state.wabil.hp, 15, "HP 5 + AC 10 未到上限 20 ⇒ 15");
        assert_eq!(sg.state.wabil.max_hp, 20);
    }

    /// **T-28（卡 `openmir2-m3-recovery-tick`）进程级判据**：吃下 `StdMode 0` 的池药后，
    /// 周期恢复 tick 把它消费回 HP 并回 `SM_HEALTHSPELLCHANGED`——HP 已满 ⇒ 首轮即把池清零
    /// （T-14 `characters.IncHealth` 归零的主路径）。
    ///
    /// 同时钉住 [`drain_recovery`] 的 `ShowHp` 支：`show_hp=true` 时同屏观察者也收到同一字节的
    /// `SM_HEALTHSPELLCHANGED`（oracle `SendRefMsg`；生产上 `ShowHp` 由未移植的技能置位）。
    ///
    /// 红检：① 把 `recovery_tick` 里 `IncHealth < PerHealth` 分支的 `rec.inc_health = 0`
    /// 删掉 ⇒ `recover::tests::pooled_drug_is_consumed_over_ticks_when_not_at_full_hp` 必红；
    /// ② 把 `drain_recovery` 回帧的 `hp`/`mp` 互换 ⇒ 本用例的头部断言必红。
    #[test]
    fn recovery_tick_consumes_pool_and_broadcasts() {
        use mir2_protocol::frame::CommandMessage;
        use std::collections::HashMap;

        // 金创药(小量) 的**池药**形态：`StdMode 0` / `Shape 0` ⇒ `IncHealth += AC`（无即时帧）。
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            100,
            mir2_data::StdItem {
                name: "金创药(小量)".into(),
                std_mode: 0,
                shape: 0,
                ac: 20,
                mac: 0,
                ..Default::default()
            },
        );

        let mut world = mir2_world::World::with_maps(vec![(32, 32)]);
        world.open_session("hero");
        let hid = world
            .enter_map("hero", mir2_world::Entity::new(0, "hero", 10, 10), 0)
            .expect("hero 进世界");
        world.open_session("obs");
        let oid = world
            .enter_map("obs", mir2_world::Entity::new(0, "obs", 11, 11), 0)
            .expect("obs 进世界");

        let make_index = 29_033_300;
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert(
            "hero".into(),
            SessionGround {
                state: game_svr::ground::GroundState {
                    item_list: vec![game_svr::bag::BagItem {
                        make_index,
                        index: 100,
                        dura: 0,
                        dura_max: 0,
                        desc: [0; 14],
                    }],
                    gold: 0,
                    wabil: {
                        let mut a = test_ability();
                        a.level = 1;
                        a.hp = 19;
                        a.max_hp = 19;
                        a.mp = 15;
                        a.max_mp = 15;
                        a
                    },
                },
                actor_id: hid,
                game_gold: 0,
                sys_msg: None,
                use_items: vec![None; game_svr::ability::USE_ITEMS_LEN],
                sub: game_svr::ability::SubAbility::first_call(),
                equip: test_equip_inputs(),
                recovery: game_svr::recover::Recovery::new(0),
                stamina: game_svr::stamina::Stamina::new(0),
                show_hp: true,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
                save_rcd_tick: 0,
            },
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "hero".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: hid,
            },
        );
        online_set(
            &mut online,
            OnlineEntry {
                account: "ub".into(),
                chr_name: "obs".into(),
                session_id: 2,
                gate_idx: 0,
                socket: 902,
                actor_id: oid,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let colors = MsgColors {
            red_f: 255,
            red_b: 56,
            hear_f: 0,
            hear_b: 255,
        };

        // 1) 吃药：池药形态只叠加 `IncHealth = 0 + 20`，尾只有 `SM_EAT_OK` + `RM_WEIGHTCHANGED`。
        handle_eat_cmd(
            make_index,
            0,
            0,
            None,
            "hero",
            901,
            &online,
            &mut sessions,
            &gates,
            &catalog,
            &world,
            &colors,
            None,
        );
        let mut eat_frames = Vec::new();
        while let Ok(SimToGate::SendFrame { body, .. }) = rx0.try_recv() {
            let cm = CommandMessage::from_bytes(&body).expect("SM_* body 至少 12B");
            eat_frames.push((cm.recog, cm.ident));
        }
        assert_eq!(
            eat_frames,
            vec![
                (0, game_svr::useitem::SM_EAT_OK),
                (0, mir2_protocol::messages::SM_WEIGHTCHANGED),
            ],
            "池药（Shape 0）不吃即时回血帧，只累加 IncHealth"
        );
        assert_eq!(sessions["hero"].recovery.inc_health, 20);

        // 2) 周期恢复 tick：tick 初值 0、now = 真实时钟 ⇒ 远超 1 级阈值 590ms ⇒ 首轮命中。
        drain_recovery(&mut sessions, &world, &online, &gates, 12);
        let mut frames = Vec::new();
        while let Ok(SimToGate::SendFrame { body, socket }) = rx0.try_recv() {
            let cm = CommandMessage::from_bytes(&body).expect("SM_* body 至少 12B");
            frames.push((socket, cm.recog, cm.ident, cm.param, cm.tag, cm.series));
        }
        // 自见（`SendUpdateMsg`）+ ShowHp 的 AOI（`SendRefMsg` 含源格 ⇒ 自己也再收一份 + 观察者一份）。
        assert_eq!(
            frames,
            vec![
                (
                    901,
                    hid,
                    mir2_protocol::messages::SM_HEALTHSPELLCHANGED,
                    19,
                    15,
                    19
                ),
                (
                    901,
                    hid,
                    mir2_protocol::messages::SM_HEALTHSPELLCHANGED,
                    19,
                    15,
                    19
                ),
                (
                    902,
                    hid,
                    mir2_protocol::messages::SM_HEALTHSPELLCHANGED,
                    19,
                    15,
                    19
                ),
            ],
            "HP 夹回 MaxHP=19、MP=15、MaxHP=19；观察者收到同一字节"
        );
        let sg = &sessions["hero"];
        assert_eq!(
            sg.recovery.inc_health, 0,
            "HP 打满 ⇒ IncHealth 清零（T-14 归零路径）"
        );
        assert_eq!(sg.state.wabil.hp, 19, "HP 夹到 MaxHP");
    }

    /// **本片（`openmir2-m3-scoped-diff-ability`）构造级判据**：`drain_stamina` 把
    /// `BaseObject.Run()` 的负值扣血支（`BaseObject.Base.cs:105-109`）作用到 `WAbil.HP`，
    /// **回写世界实体**（逐击 `SERVER_STATE` dump 与攻击结算读 `Entity.hp`），并照
    /// `HealthSpellChanged()` 给自己回一条 `SM_HEALTHSPELLCHANGED`（帧内容 = 当次快照）。
    ///
    /// 红检：① 删掉 `world.entities.get_mut(...).hp = …` 回写 ⇒ 实体 HP 断言必红；
    /// ② 回帧的 `hp`/`mp` 互换 ⇒ 头部断言必红；③ 把两次 `HealthSpellChanged()` 合并成一条
    /// （只发终态）⇒ `stamina::tests::both_branches_in_one_run_produce_two_distinct_change_snapshots` 必红。
    #[test]
    fn drain_stamina_drops_one_hp_writes_back_and_emits_frame() {
        use mir2_protocol::frame::CommandMessage;
        use std::collections::HashMap;
        let mut world = mir2_world::World::with_maps(vec![(32, 32)]);
        world.open_session("hero");
        let hid = world
            .enter_map("hero", mir2_world::Entity::new(0, "hero", 10, 10), 0)
            .expect("hero 进世界");
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut sg = test_session_ground(hid);
        sg.state.wabil.hp = 20_669;
        sg.state.wabil.max_hp = 20_669;
        sg.state.wabil.mp = 900;
        sg.state.wabil.max_mp = 900;
        // 体力点已被打事件推到阈值以下；`replay_cursor` 落后 ~60ms ⇒ 本轮至少重放一次 `Run()`。
        sg.stamina.health_tick = -400;
        sg.stamina.replay_cursor = now_ms() - 60;
        sg.stamina.auto_recovery_tick = sg.stamina.replay_cursor;
        sessions.insert("hero".into(), sg);
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "hero".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: hid,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        drain_stamina(&mut sessions, &mut world, &online, &gates, 12, 300, 800);
        let mut frames = Vec::new();
        while let Ok(SimToGate::SendFrame { socket, body }) = rx0.try_recv() {
            let cm = CommandMessage::from_bytes(&body).expect("SM_* body 至少 12B");
            frames.push((socket, cm.recog, cm.ident, cm.param, cm.tag, cm.series));
        }
        assert_eq!(
            frames,
            vec![(
                901,
                hid,
                mir2_protocol::messages::SM_HEALTHSPELLCHANGED,
                20_668,
                900,
                20_669
            )],
            "负值扣血支：一条自见帧（recog=ActorId、param=HP、tag=MP、series=MaxHP）"
        );
        assert_eq!(
            world.entities.get(hid).unwrap().hp,
            20_668,
            "命中后必须回写世界实体 `hp`（逐击 dump / 攻击结算读它）"
        );
        assert_eq!(sessions["hero"].state.wabil.hp, 20_668);
        assert!(
            sessions["hero"].stamina.health_tick > -300,
            "扣血后 HealthTick += HealthFillTime"
        );

        // 无变更 ⇒ 无帧（oracle 只在 `HealthSpellChanged()` 被调用时发包）。
        sessions.get_mut("hero").unwrap().stamina.replay_cursor = now_ms();
        drain_stamina(&mut sessions, &mut world, &online, &gates, 12, 300, 800);
        assert!(rx0.try_recv().is_err(), "无变更 ⇒ 不得回帧");
    }

    /// **本片构造级判据（移动消耗点）**：`handle_move_cmd` 成功支按 oracle 扣体力/魔力点——
    /// `CM_WALK` 只 `HealthTick -= 10`（`PlayObject.Attack.cs:1424`）；
    /// `CM_RUN`/`CM_HORSERUN` = `HealthTick -= 60; SpellTick -= 10; SpellTick = max(0, ·);
    /// PerHealth -= 1; PerSpell -= 1`（`:1352-1356` / `:1041-1045`）。
    ///
    /// 红检：把 `on_run_success` 的 `SpellTick` 扣减删掉，或跑步不扣 `PerHealth/PerSpell`
    /// ⇒ 本用例必红。
    #[test]
    fn move_run_consumes_stamina_and_per_pool() {
        use mir2_protocol::messages::{CM_HORSERUN, CM_RUN, CM_WALK};
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let mover = {
            let mut e = mir2_world::Entity::new(0, "mover".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
        let mut displays = downlink::DisplayTable::new();
        displays.set(
            mover,
            downlink::ActorDisplay::new(0, 0, 111, 0, 0, 255, b"mover".to_vec()),
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "mover".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: mover,
            },
        );
        let (tx0, _rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert("mover".into(), test_session_ground(mover));

        // 走路（1 格）：只 `HealthTick -= 10`，不动 `Per*`。
        handle_move_cmd(
            CM_WALK,
            game_svr::feature::make_long(11, 10),
            0,
            "mover",
            901,
            &online,
            &mut sessions,
            &mut world,
            &mut displays,
            &gates,
        );
        assert_eq!(sessions["mover"].stamina.health_tick, -10);
        assert_eq!(
            sessions["mover"].recovery.per_health, 5,
            "走路不扣 PerHealth（oracle 只在跑步/骑跑扣）"
        );
        assert_eq!(sessions["mover"].recovery.per_spell, 5);

        // 跑步（2 格）：`HealthTick -= 60`；`SpellTick = max(0, -10) = 0`；`Per*` 各 -1。
        handle_move_cmd(
            CM_RUN,
            game_svr::feature::make_long(13, 10),
            0,
            "mover",
            901,
            &online,
            &mut sessions,
            &mut world,
            &mut displays,
            &gates,
        );
        assert_eq!(sessions["mover"].stamina.health_tick, -70);
        assert_eq!(
            sessions["mover"].stamina.spell_tick, 0,
            "SpellTick 有 0 下限（`HUtil32._MAX(0, …)`）"
        );
        assert_eq!(sessions["mover"].recovery.per_health, 4);
        assert_eq!(sessions["mover"].recovery.per_spell, 4);

        // 骑跑（3 格）：再 `HealthTick -= 60` ⇒ -130。
        handle_move_cmd(
            CM_HORSERUN,
            game_svr::feature::make_long(16, 10),
            0,
            "mover",
            901,
            &online,
            &mut sessions,
            &mut world,
            &mut displays,
            &gates,
        );
        assert_eq!(sessions["mover"].stamina.health_tick, -130);
        assert_eq!(sessions["mover"].recovery.per_health, 3);
        assert_eq!(sessions["mover"].recovery.per_spell, 3);
    }

    /// 测试用：带四种新手物品模板的目录（`find_by_name` 按名字查）。
    fn newbie_catalog() -> game_svr::ability::StdItemCatalog {
        let mut c = game_svr::ability::StdItemCatalog::new();
        c.insert(
            1,
            mir2_data::StdItem {
                name: "蜡烛".into(),
                dura_max: 20,
                ..Default::default()
            },
        );
        c.insert(
            2,
            mir2_data::StdItem {
                name: "金创药(小量)".into(),
                std_mode: 0,
                shape: 1,
                ac: 10,
                dura_max: 30,
                ..Default::default()
            },
        );
        c.insert(
            3,
            mir2_data::StdItem {
                name: "木剑".into(),
                std_mode: 5,
                dura_max: 20,
                ..Default::default()
            },
        );
        c.insert(
            4,
            mir2_data::StdItem {
                name: "布衣(男)".into(),
                ..Default::default()
            },
        );
        c.insert(
            5,
            mir2_data::StdItem {
                name: "布衣(女)".into(),
                ..Default::default()
            },
        );
        c
    }

    fn newbie_names() -> NewbieItems {
        NewbieItems {
            candle: "蜡烛".into(),
            basic_drug: "金创药(小量)".into(),
            wooden_sword: "木剑".into(),
            cloths_man: "布衣(男)".into(),
            cloths_woman: "布衣(女)".into(),
            item_number: 100,
        }
    }

    /// **owner 追加的卡项 ④**：`IsNewHuman` 四件新手物品按 oracle 固定顺序入包
    /// （`PlayObject.cs:1205-1245`：烛烛→金创药(小量)→木剑→布衣）；`Dura=DuraMax=tpl.DuraMax`。
    /// 红检：去掉其中一件（或改顺序）⇒ 本用例必红。
    #[test]
    fn newbie_items_follow_oracle_order_and_slots() {
        let catalog = newbie_catalog();
        let names = newbie_names();
        let mut list = Vec::new();
        let mut n = names.item_number;
        let warns = add_newbie_items(&mut list, &catalog, &names, 0, &mut n);
        assert!(warns.is_empty(), "模板都在：{warns:?}");
        assert_eq!(
            list.iter().map(|i| i.index).collect::<Vec<_>>(),
            vec![1, 2, 3, 4],
            "顺序 = 烛烛/金创药/木剑/布衣(男)"
        );
        assert_eq!(
            list.iter().map(|i| i.make_index).collect::<Vec<_>>(),
            vec![101, 102, 103, 104],
            "MakeIndex 单调递增"
        );
        assert_eq!((list[1].dura, list[1].dura_max), (30, 30), "金创药 30/30");

        // 女号取 `ClothsWoman`（`Gender != Man`）。
        let mut list_f = Vec::new();
        let mut nf = 0;
        add_newbie_items(&mut list_f, &catalog, &names, 1, &mut nf);
        assert_eq!(list_f[3].index, 5, "女号第 4 件 = 布衣(女)（index 5）");
    }

    /// **owner 追加的卡项 ②**：`CM_QUERYBAGITEMS(81)` 回 `SM_BAGITEMS(201)`，`recog=ActorId`、
    /// `series=ItemList.Count`，尾 = 每件 `EncodeBuffer(ClientItem)+'/'`（非空才发）。
    /// 红检：把 `sm_bagitems` 的 ident/series 改错、或摘掉 `handle_query_bag_cmd` 的发送 ⇒ 本用例必红。
    #[test]
    fn query_bagitems_replies_sm_bagitems_with_items() {
        use mir2_protocol::frame::CommandMessage;
        use std::collections::HashMap;

        let catalog = newbie_catalog();
        let names = newbie_names();
        let mut list = Vec::new();
        let mut n = names.item_number;
        add_newbie_items(&mut list, &catalog, &names, 0, &mut n);

        let world = mir2_world::World::new(32, 32);
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert(
            "hero".into(),
            SessionGround {
                state: game_svr::ground::GroundState {
                    item_list: list,
                    gold: 0,
                    wabil: test_ability(),
                },
                actor_id: 7,
                game_gold: 0,
                sys_msg: None,
                use_items: vec![None; game_svr::ability::USE_ITEMS_LEN],
                sub: game_svr::ability::SubAbility::first_call(),
                equip: test_equip_inputs(),
                recovery: game_svr::recover::Recovery::new(0),
                stamina: game_svr::stamina::Stamina::new(0),
                show_hp: false,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
                save_rcd_tick: 0,
            },
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "hero".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: 7,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        handle_query_bag_cmd(
            "hero",
            901,
            &online,
            &mut sessions,
            &gates,
            &catalog,
            &world,
        );
        let SimToGate::SendFrame { body, .. } = rx0.try_recv().expect("必须回 SM_BAGITEMS")
        else {
            panic!("应为 SendFrame")
        };
        let cm = CommandMessage::from_bytes(&body).expect("至少 12B");
        assert_eq!(cm.ident, mir2_protocol::messages::SM_BAGITEMS, "ident=201");
        assert_eq!(cm.recog, 7, "recog=ActorId");
        assert_eq!(cm.series, 4, "series=ItemList.Count=4");
        assert!(body.len() > 12, "必须带尾（4 件 ClientItem）");
        assert_eq!(body.last(), Some(&b'/'), "尾以 '/' 收（每件后缀）");

        // 空背包 ⇒ 不发（oracle `if (!string.IsNullOrEmpty(sSendMsg))`）。
        sessions.get_mut("hero").unwrap().state.item_list.clear();
        sessions.get_mut("hero").unwrap().query_bag_items_tick = None;
        handle_query_bag_cmd(
            "hero",
            901,
            &online,
            &mut sessions,
            &gates,
            &catalog,
            &world,
        );
        assert!(
            matches!(rx0.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "空背包不得发 SM_BAGITEMS"
        );
    }

    /// **`openmir2-m3-attack-magic-wiring`（第一片）**：`CM_HIT`/`CM_HEAVYHIT` 派发——
    /// `nX==CurrX && nY==CurrY` 成功门、`Death` 守卫、前方格取目标并 `attack_cmd`、
    /// 给附近观察者发 `SM_HIT` 族动作帧（无尾，排除自己）、给自己回 `+GD`。
    ///
    /// 红检：① `attackwire::at_self` 改成恒 `true` ⇒ 段 ① 的「不回 ack」必红；
    /// ② `attackwire::hit_mode` 把 `CM_HEAVYHIT` 映射成 0 ⇒ 段 ③ 的 `SM_HEAVYHIT` 断言必红；
    /// ③ `attackwire::front_position` 的 `DIR_UP` 支改成 `(x, y - 2)`（或
    /// `smframe::sm_action_hit` 头字段互换）⇒ 段 ② 的「目标被击中」/帧头断言必红；
    /// ④ 取目标改回用**本帧消息 dir**（`front_position(dir, …)` 而非 `pose_dir`）⇒ 段 ② 必红
    /// （本帧 dir=4=下 与状态化朝向 0=上 相反，怪在正上方 ⇒ 不再命中；oracle
    /// `PlayObject.Attack.cs:26` 在 `BaseObject.Attack.cs:12` 的 `Dir = nDir` 之前取目标）。
    #[test]
    fn hit_cmd_dispatches_ack_action_frame_and_world_attack() {
        use mir2_protocol::messages::{CM_HEAVYHIT, CM_HIT, SM_HEAVYHIT, SM_HIT};
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let attacker = {
            let mut e = mir2_world::Entity::new(0, "atk".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.dc = 0x0064; // DC 100
            world.add_entity(e)
        };
        let observer = {
            let mut e = mir2_world::Entity::new(0, "obs".to_string(), 12, 12);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        let monster = {
            let mut e = mir2_world::Entity::new(0, "mon".to_string(), 10, 9); // 正上方（Dir=0）
            e.race = mir2_world::ACTOR_RACE_MONSTER;
            e.hp = 200;
            e.max_hp = 200;
            e.hit_point = 0; // `BaseObject.Attack.cs:39` 的命中判定：HitPoint==0 ⇒ 必中
            e.speed_point = 15;
            world.add_entity(e)
        };
        world.tick(); // Enter 落格
        let mut displays = downlink::DisplayTable::new();
        displays.set(
            attacker,
            // 状态化 `Dir` = 0（上）= 怪物所在方向；本帧消息 dir 段 ② 用 4（下）以区分两者。
            downlink::ActorDisplay::new(0, 0, 111, 0, 0, 255, b"atk".to_vec()),
        );
        displays.set(
            observer,
            downlink::ActorDisplay::new(4, 0, 222, 0, 0, 255, b"obs".to_vec()),
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "atk".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: attacker,
            },
        );
        online_set(
            &mut online,
            OnlineEntry {
                account: "ub".into(),
                chr_name: "obs".into(),
                session_id: 2,
                gate_idx: 0,
                socket: 902,
                actor_id: observer,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert("atk".into(), test_session_ground(attacker));

        // ① 目标格 != 自身（(11,10)）⇒ 不攻击、不回 ack、不发帧。
        let rejected = handle_hit_cmd(
            CM_HIT,
            game_svr::feature::make_long(11, 10),
            0,
            "atk",
            901,
            &online,
            &mut world,
            &mut displays,
            &mut sessions,
            &gates,
        );
        assert!(rx0.try_recv().is_err(), "目标≠自身 ⇒ 不得回 ack / 发帧");
        assert!(
            !rejected,
            "被守卫拒的一击必须返回 false（无攻击单元 ⇒ 调用方不落服务端状态 dump 行）"
        );

        // ② 目标格 = 自身 ⇒ SM_HIT 动作帧给观察者 + +GD 给自己 + 攻击入队（tick 后结算）。
        let per_before = sessions["atk"].recovery.per_health;
        let swung = handle_hit_cmd(
            CM_HIT,
            game_svr::feature::make_long(10, 10),
            4, // 本帧消息 dir = 下（与状态化朝向 0=上 相反）⇒ 取目标必须用状态化朝向才击中正上方怪
            "atk",
            901,
            &online,
            &mut world,
            &mut displays,
            &mut sessions,
            &gates,
        );
        let (mut frames, mut ack) = (Vec::new(), None);
        while let Ok(m) = rx0.try_recv() {
            match m {
                SimToGate::SendFrame { socket, body } => {
                    assert_eq!(socket, 902, "动作帧只给观察者");
                    frames.push(body);
                }
                SimToGate::SendRaw { socket, payload } => {
                    assert_eq!(socket, 901, "+GD 只给发起者自己");
                    ack = Some(payload);
                }
                other => panic!("意外 sim→gate 命令：{other:?}"),
            }
        }
        assert_eq!(frames.len(), 1, "CM_HIT ⇒ 只一帧 SM_HIT（无尾）");
        assert!(
            swung,
            "进入攻击路径的一击必须返回 true（owner 2026-10-12 23:50 第 ③ 条：调用方据此落
             SERVER_STATE dump 行；恒 false ⇒ 夹具的服务端状态门永远拿不到 Rust 行）"
        );
        assert_eq!(
            &frames[0][..12],
            // 动作帧 `Dir` 用**新** Dir（= 本帧消息 dir=4；`PlayObject.Attack.cs:76`）。
            &smframe::command_header(attacker, SM_HIT, 10, 10, 4)
        );
        assert_eq!(frames[0].len(), 12, "SM_HIT 无尾");
        assert!(ack.expect("+GD").starts_with(b"+GD/"));
        assert_eq!(
            sessions["atk"].recovery.per_health,
            per_before.wrapping_sub(2),
            "成功支 PerHealth -= 2（oracle 无符号回绕）"
        );
        // 本片（`openmir2-m3-scoped-diff-ability`）：成功支的体力/魔力点（`PlayObject.Attack.cs:955-957`）。
        assert_eq!(
            sessions["atk"].stamina.health_tick, -30,
            "成功支 HealthTick -= 30（初始 0 ⇒ -30）"
        );
        assert_eq!(
            sessions["atk"].stamina.spell_tick, 0,
            "成功支 SpellTick -= 100 ⇒ max(0, ...)（初始 0 ⇒ 0）"
        );
        let rec = world.tick();
        assert!(
            rec.attacks
                .iter()
                .any(|(a, t, _)| *a == attacker && *t == monster),
            "攻击必须落到正上方怪：{rec:?}"
        );
        assert!(
            world.entities.get(monster).unwrap().hp < 200,
            "命中应扣血（DC=100 vs AC=0）"
        );

        // ③ CM_HEAVYHIT ⇒ 观察者收 SM_HEAVYHIT(15)。
        handle_hit_cmd(
            CM_HEAVYHIT,
            game_svr::feature::make_long(10, 10),
            0,
            "atk",
            901,
            &online,
            &mut world,
            &mut displays,
            &mut sessions,
            &gates,
        );
        let mut heavy = None;
        while let Ok(m) = rx0.try_recv() {
            if let SimToGate::SendFrame { body, .. } = m {
                heavy = Some(body);
            }
        }
        let heavy = heavy.expect("CM_HEAVYHIT ⇒ 观察者收帧");
        assert_eq!(
            u16::from_le_bytes([heavy[4], heavy[5]]),
            SM_HEAVYHIT,
            "CM_HEAVYHIT ⇒ `GetHitMode(1)` = RM_HEAVYHIT ⇒ SM_HEAVYHIT"
        );

        // ④ Death 守卫 ⇒ 不发帧/不回 ack。
        world.entities.get_mut(attacker).unwrap().death = true;
        handle_hit_cmd(
            CM_HIT,
            game_svr::feature::make_long(10, 10),
            0,
            "atk",
            901,
            &online,
            &mut world,
            &mut displays,
            &mut sessions,
            &gates,
        );
        assert!(rx0.try_recv().is_err(), "Death ⇒ 不得发帧/回 ack");
    }

    /// 数出两次 RNG 快照之间消耗的**内部采样次数**：`RandomNumber` 的完整 state 由 `Debug` 暴露，
    /// 同一 seed 的参照流走 k 步后 state 必然相同 ⇒ 直接线性求解 k。
    /// 无目标一击（小范围 `random_below`）每次恰 1 个内部采样 ⇒ 该值即「抽数」（= 账本的一段）。
    fn draws_between(
        before: &mir2_shared::rng::RandomNumber,
        after: &mir2_shared::rng::RandomNumber,
    ) -> usize {
        let after_state = format!("{after:?}");
        let mut probe = before.clone();
        for k in 0..100_000 {
            if format!("{probe:?}") == after_state {
                return k;
            }
            let _ = probe.random();
        }
        panic!("参照流 100000 步内未匹配到目标状态（seed 不同或状态被外部改写）");
    }

    /// **owner 2026-10-12 20:10（M3②③ 关键路径）**：`handle_hit_cmd` 的**无目标支**必须仍走
    /// oracle 的「一击抽攻击力」语义（`World::attack_swing`），否则真机 84/120 空打的取数整段缺失
    /// （C# 120 段 vs Rust 36 段）。
    ///
    /// oracle 锚（逐行）：
    /// - `PlayObject.AttackDir(targetObject = null)`（`PlayObject.Attack.cs:25`）先取目标
    ///   `attackTarget = targetObject ?? GetPoseCreate()`（`:26`）⇒ 前方格无目标 = null；
    /// - 随后 `GetBaseAttackPoewr()` 作为 `GetAttackPowerHit(...)` 的实参被求值（`:73`）⇒
    ///   **攻击力照抽**：玩家 `Luck <= 0` ⇒ `Random(LoByte(DC)+1)` + `Random(10)` 共 2 抽
    ///   （`PlayObject.cs:2929/2932`）；
    /// - `_Attack(null)`（`BaseObject.Attack.cs:32`）与 `AttackSuccess(…, null)`
    ///   （`PlayObject.Attack.cs:215-218` 开头 `return`）都早退 ⇒ 无其它取数。
    ///
    /// 红检：把无目标支的 `world.attack_swing(actor_id)` 删掉（改回「只记日志」）⇒ 世界 RNG 停在
    /// 原位、下面的逐采样状态比对必红。
    #[test]
    fn hit_cmd_without_target_still_draws_attack_power() {
        use mir2_protocol::messages::CM_HIT;
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let attacker = {
            let mut e = mir2_world::Entity::new(0, "atk".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.dc = 0x1E14; // LoByte=0x14=20、HiByte=0x1E=30 ⇒ (base=20, power=10)
            e.luck = 0;
            e.power_rate = 100;
            world.add_entity(e)
        };
        world.tick();
        // 前方格（dir=2 ⇒ (11,10)）**故意留空** ⇒ `front_target` = None。
        let mut displays = downlink::DisplayTable::new();
        displays.set(
            attacker,
            downlink::ActorDisplay::new(2, 0, 111, 0, 0, 255, b"atk".to_vec()),
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".into(),
                chr_name: "atk".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: attacker,
            },
        );
        let (tx0, _rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert("atk".into(), test_session_ground(attacker));

        const SEED: i32 = 20261012;
        world.rng = mir2_shared::rng::RandomNumber::with_seed(SEED);
        // oracle 期望值：玩家 `GetBaseAttackPoewr` 版（`Random(LoByte(DC)+1)` + `Luck<=0` 的
        // `Random(10)`）。用**独立**参照流手推，不由生产 `attack_swing` 反推。
        let mut expected = mir2_shared::rng::RandomNumber::with_seed(SEED);
        let expected_power = mir2_world::combat::get_base_attack_power_player(
            &mut expected,
            0x1E14,
            0,
            100,
            false,
            0,
        );
        assert!(
            (20..=30).contains(&expected_power),
            "DC=0x1E14 ⇒ 攻击力 ∈ [20,30]，实际 {expected_power}"
        );

        let before_one = world.rng.clone();
        handle_hit_cmd(
            CM_HIT,
            game_svr::feature::make_long(10, 10),
            2, // 本帧消息 dir=2（右）⇒ 前方格 (11,10) 无目标
            "atk",
            901,
            &online,
            &mut world,
            &mut displays,
            &mut sessions,
            &gates,
        );
        // 改后读数：无目标一击 = 玩家 `GetBaseAttackPoewr` **恰 2 抽**（= 账本的一段）；
        // 改回「只记日志」⇒ 此值为 0（红检实测见证据 `05-redcheck-remove-swing.txt`）。
        let one = draws_between(&before_one, &world.rng);
        println!("ATTACK_SWING_DRAWS_PER_EMPTY_HIT={one}");
        assert_eq!(
            one, 2,
            "无目标一击必须抽 2 次（oracle `PlayObject.Attack.cs:73` 的玩家 GetBaseAttackPoewr）"
        );

        // 同一读数的 84 击外推（真机形态：受害者 36 击后 `death=true` ⇒ 余 84 击空打）：
        // 改后 84 × 2 = 168 抽 / 84 段；改前 0 抽 / 0 段 ⇒ 攻击路径段数 36 → 36 + 84 = 120。
        let before_seq = world.rng.clone();
        for _ in 0..84 {
            handle_hit_cmd(
                CM_HIT,
                game_svr::feature::make_long(10, 10),
                2,
                "atk",
                901,
                &online,
                &mut world,
                &mut displays,
                &mut sessions,
                &gates,
            );
        }
        let seq = draws_between(&before_seq, &world.rng);
        println!("ATTACK_SWING_DRAWS_84_EMPTY_HITS={seq}");
        assert_eq!(
            seq,
            84 * 2,
            "84 击空打必须每击抽 2 次（= 84 段，补回 C# 120 vs Rust 36 的缺口）"
        );
    }

    /// **`openmir2-m3-attack-magic-wiring`（第二片）**：`CM_SPELL`/`CM_MAGICKEYCHANGE` 派发——
    /// 守卫（未知魔法号 / `Death` ⇒ 不回 ack）、成功后 `+GD` + `Dir = GetNextDirection`
    /// （**仅 `default` 支的魔号**，见 F-2），以及 `ClientChangeMagicKey` 的魔法键切换（无回帧）。
    ///
    /// 红检：① `magicwire::change_magic_key` 改比 `magic_key` 而不是 `mag_idx` ⇒ 段 ④ 必红；
    /// ② `magicwire::spell_reject` 不看 `has_magic`（未知魔法号也过）⇒ 段 ① 必红；
    /// ③（旧 F-2 收口）`magicwire::EXPLICIT_CASE_MAG_IDX` 里任一魔号改错 ⇒ 段 ⑤ 与
    ///   `magicwire::tests::default_dir_only_for_non_explicit_case_magics` 必红。
    #[test]
    fn spell_cmd_dispatches_guard_ack_and_magic_key_change() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let caster = {
            let mut e = mir2_world::Entity::new(0, "caster".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
        let mut displays = downlink::DisplayTable::new();
        displays.set(
            caster,
            downlink::ActorDisplay::new(4, 0, 111, 0, 0, 255, b"caster".to_vec()),
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "uc".into(),
                chr_name: "caster".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 911,
                actor_id: caster,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut saves: HashMap<String, SavedHuman> = HashMap::new();
        let mut record = stage6_record();
        record.data.magic = Some(vec![Some(record::MagicRcd {
            mag_idx: 1,
            level: 1,
            magic_key: u16::from(b'm'),
            tran_point: 0,
        })]);
        saves.insert(
            "caster".into(),
            SavedHuman {
                account: "uc".into(),
                record,
                enter_map: "0".into(),
            },
        );
        const FIREBALL: u16 = 1;

        // ① 未知魔法号（守卫 NoMagic）⇒ 不回 ack。
        handle_spell_cmd(
            game_svr::feature::make_long(12, 10),
            99,
            0,
            0,
            "caster",
            911,
            &online,
            &world,
            &saves,
            &mut displays,
            &gates,
        );
        assert!(rx0.try_recv().is_err(), "未知魔法号 ⇒ 不回 ack / 无帧");

        // ② 有该魔法 + 目标在右 ⇒ 回 `+GD`，且展示方向朝向目标（`GetNextDirection`）。
        handle_spell_cmd(
            game_svr::feature::make_long(12, 10),
            FIREBALL,
            0,
            0,
            "caster",
            911,
            &online,
            &world,
            &saves,
            &mut displays,
            &gates,
        );
        match rx0.try_recv() {
            Ok(SimToGate::SendRaw { socket, payload }) => {
                assert_eq!(socket, 911, "+GD 只给发起者自己");
                assert!(payload.starts_with(b"+GD/"), "成功支回 +GD");
            }
            other => panic!("CM_SPELL 成功支应回 +GD：{other:?}"),
        }
        assert!(
            rx0.try_recv().is_err(),
            "本片不发任何魔法帧（效果体未接，登记）"
        );
        assert_eq!(
            displays.get(caster).unwrap().dir,
            game_svr::movewire::DIR_RIGHT,
            "default 支 `Dir = GetNextDirection(10,10 → 12,10)` = 右"
        );

        // ③ `Death` 守卫 ⇒ 不回 ack。
        world.entities.get_mut(caster).unwrap().death = true;
        handle_spell_cmd(
            game_svr::feature::make_long(12, 10),
            FIREBALL,
            0,
            0,
            "caster",
            911,
            &online,
            &world,
            &saves,
            &mut displays,
            &gates,
        );
        assert!(rx0.try_recv().is_err(), "Death ⇒ 不回 ack");
        world.entities.get_mut(caster).unwrap().death = false;

        // ④ `CM_MAGICKEYCHANGE`：命中 `mag_idx == Recog` ⇒ 改 `Key`；无回帧。
        handle_magic_key_cmd(1, u16::from(b'f'), "caster", 911, &mut saves);
        assert_eq!(
            saves["caster"].record.data.magic.as_ref().unwrap()[0]
                .as_ref()
                .unwrap()
                .magic_key,
            u16::from(b'f'),
            "ClientChangeMagicKey 置 Key"
        );
        assert!(
            rx0.try_recv().is_err(),
            "改魔法键不发帧（oracle 无 SendSocket）"
        );
        // 未命中 ⇒ 不改、也不红。
        handle_magic_key_cmd(42, u16::from(b'z'), "caster", 911, &mut saves);
        assert_eq!(
            saves["caster"].record.data.magic.as_ref().unwrap()[0]
                .as_ref()
                .unwrap()
                .magic_key,
            u16::from(b'f'),
            "未命中不改已有键"
        );

        // ⑤ F-2 收口（reviewer-5 `1de8fca5`）：`switch(MagIdx)` 有 8 条独立 `case` 的魔号不套用
        //    `default` 的 `Dir`。补一个 SKILL_ERGUM(12)，把展示方向置为左，再打右侧目标。
        saves.get_mut("caster").unwrap().record.data.magic = Some(vec![
            Some(record::MagicRcd {
                mag_idx: 1,
                level: 1,
                magic_key: u16::from(b'f'),
                tran_point: 0,
            }),
            Some(record::MagicRcd {
                mag_idx: 12,
                level: 1,
                magic_key: u16::from(b'e'),
                tran_point: 0,
            }),
        ]);
        displays.set(
            caster,
            downlink::ActorDisplay::new(
                game_svr::movewire::DIR_LEFT,
                0,
                111,
                0,
                0,
                255,
                b"caster".to_vec(),
            ),
        );
        handle_spell_cmd(
            game_svr::feature::make_long(12, 10),
            12,
            0,
            0,
            "caster",
            911,
            &online,
            &world,
            &saves,
            &mut displays,
            &gates,
        );
        match rx0.try_recv() {
            Ok(SimToGate::SendRaw { socket, payload }) => {
                assert_eq!(socket, 911);
                assert!(
                    payload.starts_with(b"+GD/"),
                    "有独立 case 的魔号仍过守卫并回 +GD"
                );
            }
            other => panic!("CM_SPELL(12) 成功支应回 +GD：{other:?}"),
        }
        assert_eq!(
            displays.get(caster).unwrap().dir,
            game_svr::movewire::DIR_LEFT,
            "F-2：mag_idx=12 有独立 case ⇒ 不套用 default 的 Dir（方向保持左）"
        );
    }

    /// **第一片（战斗下行）**：手造一个 tick 记录 ⇒ `drain_combat_downlink` 给受击玩家自己发
    /// `SM_STRUCK`（头 `HP/MaxHP/伤害` + 16B `MessageBodyWL` 尾）、给同屏（含死者自己）发
    /// `SM_NOWDEATH`（头 `X/Y/Dir` + 8B `CharDesc` 尾），并把世界实体 HP 同步回会话 `WAbil.HP`。
    ///
    /// 红检：① 把 `smframe::sm_struck` 的头 `hp`/`max_hp` 互换 ⇒ 头断言必红；
    /// ② 把死亡广播改成 `broadcast_ref_msg`（排除自己）⇒ 死者自己收不到 `SM_NOWDEATH` 必红；
    /// ③ 把 `smframe::sm_struck` 的尾 `feature`/`status` 互换 ⇒ `struck[12..]` 尾断言必红
    ///   （**M-1 收口**：本用例现在真的逐字节断言 16B `MessageBodyWL` 尾，不再只声称覆盖）。
    #[test]
    fn combat_downlink_sends_struck_and_nowdeath() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let victim = {
            let mut e = mir2_world::Entity::new(0, "victim".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        let attacker = {
            let mut e = mir2_world::Entity::new(0, "atk".to_string(), 11, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
        let mut displays = downlink::DisplayTable::new();
        displays.set(
            victim,
            downlink::ActorDisplay::new(4, 0, 111, 0, 0, 255, b"victim".to_vec()),
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "uv".into(),
                chr_name: "victim".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: victim,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert("victim".into(), test_session_ground(victim));

        let rec = mir2_world::TickRecord {
            attacks: vec![(
                attacker,
                victim,
                mir2_world::AttackOutcome {
                    power: 10,
                    hit: true,
                    damage: 7,
                    killed: true,
                    target_hp_after: 0,
                },
            )],
            deaths: vec![(victim, 0, 0, None, 0)],
            ..Default::default()
        };
        drain_combat_downlink(&rec, &world, &displays, &online, &mut sessions, &gates);
        let mut bodies = Vec::new();
        while let Ok(m) = rx0.try_recv() {
            if let SimToGate::SendFrame { socket, body } = m {
                assert_eq!(socket, 901);
                bodies.push(body);
            }
        }
        assert_eq!(bodies.len(), 2, "先受击后死亡两帧");
        let struck = &bodies[0];
        assert_eq!(
            u16::from_le_bytes([struck[4], struck[5]]),
            mir2_protocol::messages::SM_STRUCK,
            "第一帧 = SM_STRUCK"
        );
        let cm = CommandMessage::from_bytes(struck).expect("12B");
        assert_eq!(
            (cm.recog, cm.param, cm.tag, cm.series),
            (victim, 0, 100, 7),
            "SM_STRUCK 头 = (受击者, HP, MaxHP, 伤害)"
        );
        // **M-1 收口**（reviewer-5 `ebfd15a8`）：文档声称覆盖 16B `MessageBodyWL` 尾，这里真的
        // 逐字节断言 `struck[12..]`，否则尾 mutation 抓不到。
        // 尾 = Encode(MessageBodyWL{ Param1=GetFeature(), Param2=CharStatus,
        //                            Tag1=命中者 ActorId, Tag2=(RM_STRUCK_MAG ? 1 : 0) })。
        let mut wl = [0u8; 16];
        wl[0..4].copy_from_slice(&111_i32.to_le_bytes()); // Param1 = GetFeature()
        wl[4..8].copy_from_slice(&0_i32.to_le_bytes()); // Param2 = CharStatus
        wl[8..12].copy_from_slice(&attacker.to_le_bytes()); // Tag1 = 命中者 ActorId
        wl[12..16].copy_from_slice(&0_i32.to_le_bytes()); // Tag2 = (RM_STRUCK_MAG ? 1 : 0)
        assert_eq!(
            &struck[12..],
            &mir2_protocol::edcode::encode(&wl),
            "SM_STRUCK 尾 = Encode(MessageBodyWL{{Param1=feature,Param2=status,Tag1=hitter,Tag2=mag}})"
        );
        assert_eq!(
            sessions["victim"].state.wabil.hp, 0,
            "受击后世界 HP 同步回会话 WAbil"
        );
        let death = &bodies[1];
        assert_eq!(
            u16::from_le_bytes([death[4], death[5]]),
            mir2_protocol::messages::SM_NOWDEATH,
            "第二帧 = SM_NOWDEATH（nParam3==1）"
        );
        let cm = CommandMessage::from_bytes(death).expect("12B");
        assert_eq!((cm.recog, cm.param, cm.tag, cm.series), (victim, 10, 10, 4));
        assert_eq!(
            &death[12..],
            &mir2_protocol::edcode::encode(&downlink::char_desc(111, 0)),
            "死亡尾 = Encode(8B CharDesc)"
        );
    }

    /// **D①（击杀经验下行）**：`TickRecord::exp_awards` 一条 ⇒ `drain_combat_downlink` 给获得者发
    /// `SM_WINEXP(44)`（`Recog=总量`、`Param=LoWord(增量)`、`Tag=HiWord(增量)`、无尾），并把世界实体的
    /// 经验/等级同步回会话（存档 `build_save_record` 读 `sg.equip.abil`）。
    /// 红检：把 `sm_winexp` 的 `param`/`tag` 互换 ⇒ 头断言必红；不同步会话 ⇒ 会话 exp 断言必红。
    #[test]
    fn exp_downlink_sends_sm_winexp_and_syncs_session() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let killer = {
            let mut e = mir2_world::Entity::new(0, "killer".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
        world.entities.get_mut(killer).unwrap().exp = 1234;
        world.entities.get_mut(killer).unwrap().level = 9;
        let mut displays = downlink::DisplayTable::new();
        displays.set(
            killer,
            downlink::ActorDisplay::new(4, 0, 111, 0, 0, 255, b"killer".to_vec()),
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "uk".into(),
                chr_name: "killer".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 902,
                actor_id: killer,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert("killer".into(), test_session_ground(killer));

        // 增量 0x0001_0203（Param=0x0203、Tag=0x0001），总量 1234。
        let rec = mir2_world::TickRecord {
            exp_awards: vec![(killer, 0x0001_0203, true, 9)],
            ..Default::default()
        };
        drain_combat_downlink(&rec, &world, &displays, &online, &mut sessions, &gates);
        let mut bodies = Vec::new();
        while let Ok(m) = rx0.try_recv() {
            if let SimToGate::SendFrame { socket, body } = m {
                assert_eq!(socket, 902);
                bodies.push(body);
            }
        }
        assert_eq!(bodies.len(), 1, "只有经验一帧");
        let cm = CommandMessage::from_bytes(&bodies[0]).expect("12B");
        assert_eq!(cm.ident, mir2_protocol::messages::SM_WINEXP);
        assert_eq!(cm.recog, 1234, "Recog = 加完后的 Abil.Exp（总量）");
        assert_eq!(cm.param, 0x0203, "Param = LoWord(增量)");
        assert_eq!(cm.tag, 0x0001, "Tag = HiWord(增量)");
        assert_eq!(cm.series, 0);
        assert_eq!(bodies[0].len(), 12, "SM_WINEXP 无尾");
        let sg = &sessions["killer"];
        assert_eq!(sg.equip.abil.exp, 1234, "会话基能力 exp 同步（存档口径）");
        assert_eq!(sg.equip.abil.level, 9);
        assert_eq!(sg.state.wabil.exp, 1234);
        assert_eq!(sg.state.wabil.level, 9);
    }
}
