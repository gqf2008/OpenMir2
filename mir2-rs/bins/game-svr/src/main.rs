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
mod mapwire;
mod notice;
mod record;
mod strings;
mod users;

use std::sync::Arc;

use anyhow::{Context, Result};
use game_svr::chat;
use game_svr::downlink;
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
    /// 已发出、还没取走的**存档**应答 `query_id`（`DBR_SAVEHUMANRCD(1102)` 或失败时的
    /// `DBR_LOADHUMANRCD(1100)`）——只为把应答从 `DbReplies` 的表里取走并记日志，避免泄漏
    /// （oracle 的 `PlayerDataService.Enqueue` 也会把它们取走）。
    save_outstanding: Vec<i32>,
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
        inc_health: sg.inc_health,
        inc_spell: sg.inc_spell,
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
    sg.inc_health = state.inc_health;
    sg.inc_spell = state.inc_spell;
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
    rec.data.inc_health = u8::try_from(sg.inc_health & 0x00FF).unwrap_or(0);
    rec.data.inc_spell = u8::try_from(sg.inc_spell & 0x00FF).unwrap_or(0);
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

/// 发 `DB_SAVEHUMANRCD(101)`（`WorldServer.SaveHumanRcd` `:1077` → `PlayerDataService.SavePlayerRcd`）。
///
/// **时机照 oracle**：在 `MakeGhost()`（退出世界）时调用（`RunNotice` 的 `BoSoftClose`/`BoKickFlag`
/// 支与 `CloseUser` 断线支都会走到），所以调用点必须在 `world.disconnect` **之前**。
/// 周期存档（`SaveHumanRcdTime` 默认 10 分钟，`WorldServer.cs:698-702`）本轮**未做**（登记）。
fn save_human_rcd(
    db: &mut Option<DbWiring>,
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
            w.save_outstanding.push(qid);
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
            save_human_rcd(db, saved, &chr, world, sg, registry);
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
            save_human_rcd(db, saved, &chr, world, sg, registry);
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
    /// `CharacterObject.IncHealth`（`CM_EAT` 的 `StdMode 0 default` 分支叠加；oracle `ushort` 语义）。
    inc_health: u16,
    /// `CharacterObject.IncSpell`。
    inc_spell: u16,
    /// `CharacterObject.UserUnLockDurg`（`CM_EAT` 的 `StdMode 0 Shape 2` 置 true）。
    user_unlock_durg: bool,
    /// `PlayObject.HungerStatus`（`CM_EAT` 的 `StdMode 1`：`+= DuraMax/10`，上限 5000）。
    hunger_status: i32,
    /// `PlayObject.QueryBagItemsTick`（`PlayObject.Message.cs:786` 的 30s 限频）：`None` = 未查过（照 oracle 首次即过）。
    query_bag_items_tick: Option<i64>,
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
    /// 空 `HomeMap` 新角色的进图回落（`homemap::HomeFallback`，`WorldServer.cs:250-260`）。
    home: homemap::HomeFallback,
    /// `IsNewHuman` 的新手物品输入。
    newbie: NewbieItems,
    /// `Exps.conf` 的 `NeedExps` 表（`BaseObject.GetLevelExp` 的数据源，`crates/data::exps`）。
    /// 存档 `MaxExp`（`PlayObject.cs:1310`）与击杀经验都用它；读不到时为全 0（登记）。
    need_exps: Vec<i32>,
}

fn sim_loop(
    map_sizes: Vec<(i32, i32)>,
    registry: Option<mir2_envir::MapRegistry>,
    enter_cfg: EnterConfig,
    notice: Vec<u8>,
    colors: MsgColors,
    ev_rx: std::sync::mpsc::Receiver<GateEvent>,
    mut db: Option<DbWiring>,
) {
    // 尺寸表已由调用方按「注册表优先，无注册表则 WorldMaps 占位（且非空，已 fail-closed）」算好；
    // `map_id` = 注册表下标，与 `resolve_map_id` 同源。
    let mut world = mir2_world::World::with_maps(map_sizes);
    // `Exps.conf` 的 NeedExps（`BaseObject.GetLevelExp` 的数据源；存档 `MaxExp` 也用它）。
    world.need_exps = enter_cfg.need_exps.clone();
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
                                                            inc_health: u16::from(d.inc_health),
                                                            inc_spell: u16::from(d.inc_spell),
                                                            // `UserUnLockDurg` 不在角色记录里（会话初生 false；
                                                            // 与 `CM_EAT` 的 Shape 2 同一状态变）：登记为未建模。
                                                            user_unlock_durg: false,
                                                            hunger_status: d.hunger_status,
                                                            query_bag_items_tick: None,
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
            // 存档应答（`DBR_SAVEHUMANRCD(1102)` / 失败的 `DBR_LOADHUMANRCD(1100)`）：取走并记日志
            // （`FrontEngine.InSaveRcdList` 的「保存中」生命周期未建模，登记）。
            let mut still_saves = Vec::new();
            for qid in std::mem::take(&mut w.save_outstanding) {
                match r.take(qid) {
                    Some(reply) => {
                        let (ident, recog) =
                            dbclient::parse_reply_message(&reply.message).unwrap_or((0, 0));
                        glog!("INF", "存档应答 queryId={qid} Ident={ident} Recog={recog}");
                    }
                    None => still_saves.push(qid),
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
            &mut saves,
            &mut ground_sessions,
            registry.as_ref(),
        );
        let rec = world.tick();
        ticks += 1;
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
        SimToGate::SendFrame { .. } => {}
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
        other => {
            apply_sim_cmd(other, users);
            Ok(())
        }
    }
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
            inc_health: 3,
            inc_spell: 4,
            user_unlock_durg: false,
            hunger_status: 55,
            query_bag_items_tick: None,
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
                inc_health: 0,
                inc_spell: 0,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
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
                inc_health: 0,
                inc_spell: 0,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
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
                inc_health: 0,
                inc_spell: 0,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
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
                inc_health: 0,
                inc_spell: 0,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
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
                inc_health: 0,
                inc_spell: 0,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
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
                inc_health: 0,
                inc_spell: 0,
                user_unlock_durg: false,
                hunger_status: 0,
                query_bag_items_tick: None,
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
}
