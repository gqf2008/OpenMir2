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
mod clock;
mod config;
mod dbclient;
mod enter;
mod goldsales;
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
mod storage;
mod strings;
mod users;

use std::sync::Arc;

use anyhow::{Context, Result};
use game_svr::chat;
use game_svr::downlink;
use game_svr::magics;
use game_svr::movewire;
use game_svr::party as partywire;
use game_svr::smframe;
use mir2_protocol::frame::ServerMessage;
use mir2_protocol::messages::grobal2;
use mir2_world::magic;
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

/// 帧 52 尾的布局开关（`openmir2-client-e2e-enter-gaps` 的 ③；真机「跑不动」根因）。
///
/// **默认 `client-58`**：现存冻结客户端（`D:\MirClient-run\MirClinet.exe`，`MirClient@c61d912d`，
/// `HIGHHP=True`，`Source/Common/Grobal2.pas:39`）的 `TAbility` 是 **58B**；`CanRun`
/// （`Source/MirClient/Actor.pas:2162`）要求 `HP >= RUN_MINHEALTH(10)`，而 oracle 的 40B
/// `EncodeMessage(WAbil)` 让客户端从 offset 22 读到的 `HP`(DWORD) = oracle 的 `Exp`（本部署 = 0）
/// ⇒ 客户端 HUD `0/0`、**跑不动**。改用客户端布局后 `HP@22 = 19` ⇒ HUD `19/19`、可跑。
///
/// 真机实测（2026-10-13，本卡；证据 `tests/parity/world/client-e2e-enter-gaps-20261013/61-*`）：
/// `client-58` ⇒ HUD `19/19` + c2s `CM_RUN(3013)` 出现；`oracle-40` ⇒ HUD `0/0` + `CM_RUN = 0`。
///
/// `MIR2_CLIENT_ABILITY_LAYOUT=0|false` ⇒ 回到 oracle 的 40B `EncodeMessage(WAbil)`（逐字节对拍 /
/// 金标准夹具用）。**这是 §6.0-3 意义上的「与 oracle 的故意偏离」**：oracle 的 40B 与冻结客户端的
/// 58B 不可同时满足，而客户端冻结（硬约束 #1）⇒ 默认以客户端为准；白名单登记条目待 owner 签字。
fn mir2_client_ability_layout() -> bool {
    // 唯一真源在库层（`smframe::client_layout_enabled`）——53 的尾布局与 52 的 58B 共用同一开关。
    game_svr::smframe::client_layout_enabled()
}

/// 52 帧尾的**生产选路**（唯一真源 = [`mir2_client_ability_layout`]，与 53 的 `TCharDesc` 尾同开关）：
/// `client_layout=true` ⇒ 冻结客户端 58B `TAbility`（[`record::encode_ability_client`]）；
/// `false` ⇒ oracle 40B `EncodeMessage(WAbil)`（[`record::encode_ability`]，逐字节对拍/金标准夹具）。
///
/// **所有**生产出帧点（入图 `enter.rs` / 升级 [`drain_levelup_downlink`] / 装备卸下
/// [`handle_bag_cmd`]）必须走本函数：冻结客户端 `SM_ABILITY` 支按 `SizeOf(TAbility)` 整块解
/// （`ClMain.pas:10102`），任一处漏用 ⇒ 该帧之后客户端把 `HP` 读成 oracle 的 `Exp`（= 0）
/// ⇒ HUD `0/0`、`CanRun`（`HP >= 10`）拒跑（与 53 缺尾同源的故障类）。
///
/// 红检：任一处换回裸 `record::encode_ability` ⇒ 对应用例的 52 尾断言必红
/// （`tests::ability_tail_switches_between_client_58_and_oracle_40` /
/// `tests::levelup_downlink_sends_levelup_frames_heals_and_syncs_entity` /
/// `tests::bag_takeoff_recalc_uses_prior_subability` 已按 58B 钉尾）。
#[must_use]
fn ability_tail(wabil: &game_svr::ability::Ability, client_layout: bool) -> Vec<u8> {
    if client_layout {
        record::encode_ability_client(wabil)
    } else {
        record::encode_ability(wabil)
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
    // **M3 `openmir2-m3-clock-injection`（Rust 半）**：`MIR2_CLOCK_MS` 可注入时钟。未设/空 ⇒
    // 不启用（`now_ms()` 取墙钟、`world.clock_ms` 从 0 起，行为一字不改）；非法值 ⇒ 显式非零
    // 退出（fail-closed，与 `MIR2_RNG_SEED` 同款纪律：不许「以为注入了时钟、其实没有」）。
    if let Some(origin) = clock::env_clock()? {
        glog!(
            "INF",
            "时钟注入：{}（now_ms=origin+单调经过；world.clock_ms 起点=origin）",
            clock::probe_line(origin)
        );
    }
    // **卡 `openmir2-client-e2e-enter-gaps` 片 ㊷**：帧 52/53 记录布局**起栈自证行**。起栈器
    // （`tools/m2-acceptance/start_rust_stack.ps1 -ClientAbilityLayout`）会打
    // `RUST_CLIENT_ABILITY_LAYOUT_INJECTED=<0|1>`，本行必须与它对上（同 `SEED_PROBE` 口径）：
    // `0` ⇒ `layout=oracle-40`（M2 验收③ 逐字节对拍）；`1` ⇒ `layout=client-58`（冻结客户端可玩）。
    // **只读日志，不改任何出帧语义**；两帧的实际布局由 `smframe::client_layout_enabled()` 统一决定。
    glog!(
        "INF",
        "{}（MIR2_CLIENT_ABILITY_LAYOUT={}）",
        game_svr::smframe::client_layout_probe_line(),
        std::env::var("MIR2_CLIENT_ABILITY_LAYOUT").unwrap_or_else(|_| "<unset>".to_string())
    );
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
    // **M3⑨-实现 2 第②片**：魔法模板表 `magics`（oracle `MySqlDB.LoadMagicDB`，
    // `MySqlDB.cs:105`，`GameApp.Initialize` 直连 MySQL 读）。`DoSpell` 取 `Magic.Effect`/
    // `EffectType`（`RM_SPELL`/`RM_MAGICFIRE` 帧参）与熟练度尾的 `TrainLv`/`TrainLevel`/`MaxTrain`。
    // 读不到 ⇒ 空表（调用方按 `GetMagicInfo == null` 降级：不接效果体，登记）。
    let magic_table = {
        let conn = cfg
            .db_conn_string
            .as_deref()
            .unwrap_or(items::DEFAULT_CONN_STRING);
        match magics::load_magics(conn).await {
            Ok(list) => {
                glog!(
                    "INF",
                    "魔法模板表 magics：{} 条（直连 MySQL，DoSpell 效果体/帧参输入）",
                    list.len()
                );
                magics::MagicTable::new(list)
            }
            Err(e) => {
                glog!(
                    "WRN",
                    "读 magics 失败（{e}）⇒ 空魔表（GetMagicInfo 恒 null ⇒ 效果体不接，登记）"
                );
                magics::MagicTable::default()
            }
        }
    };
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
    let envir_loader = mir2_envir::MapLoader::new(&base, &cfg.map_dir, &cfg.envir_dir);
    let mon_gen_entries = {
        let loader = &envir_loader;
        match registry.as_ref() {
            Some(reg) => match game_svr::mongen::load_mon_gen_entries(loader, reg) {
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
    // **GuardList 放置半（片71）**：`LocalDB.LoadGuardList`（`LocalDB.cs:100-144`）的
    // 文件+解析半在主线程装载；放置半在 sim 线程（要 `world.rng` 同一条流 + MonGen 宿主状态）。
    // oracle 时机 = `GameApp.cs:239-243`（`InitializeWorld`：`InitializationMonsterThread` 之后、
    // `WorldEngine.Initialize` 之前 ⇒ 首个 regen tick 前），且 `GameApp.cs:240` 的
    // `!Config.VentureServer` 门在**调用点**（整文件不读）。
    let guards = if mon_gen_config.venture_server {
        glog!("INF", "VentureServer=TRUE ⇒ 不载守卫（GameApp.cs:240 门）");
        None
    } else {
        match game_svr::guardlist::load_guard_list_file(&envir_loader) {
            Ok(Some(list)) => {
                glog!(
                    "INF",
                    "Envir/GuardList.txt 已载入：{} 条守卫配置",
                    list.len()
                );
                Some(list)
            }
            Ok(None) => {
                glog!(
                    "INF",
                    "无 Envir/GuardList.txt ⇒ 不放守卫（oracle File.Exists 门）"
                );
                None
            }
            Err(e) => {
                glog!(
                    "WRN",
                    "加载 Envir/GuardList.txt 失败（{e}）⇒ 不放守卫（oracle try/catch 登记）"
                );
                None
            }
        }
    };
    // **固定管理 NPC 加载半（R0-1 `openmir2-npcs-fixed-loading`）**：`LocalDB.LoadNpcs`
    // （`LocalDB.cs:655-713`，`GameApp.cs:229` 调）的文件+解析半。解析在主线程装载；建实体
    // （ctor 抽数）+ 挂图在 sim 线程（要 `world.rng` 同一条流），照 oracle
    // 「LoadMerchant → LoadNpcs → … → LoadGuardList → WorldEngine.Initialize」次序：
    // 构造抽在商人之后、守卫之前，挂图在守卫之后（`GameApp.cs:226-244`）。
    let npcs = match game_svr::npcs::load_npcs_file(&envir_loader) {
        Ok(Some(list)) => {
            glog!(
                "INF",
                "Envir/Npcs.txt 已载入：{} 条固定管理 NPC",
                list.len()
            );
            Some(list)
        }
        Ok(None) => {
            glog!(
                "INF",
                "无 Envir/Npcs.txt ⇒ 不放置固定 NPC（oracle File.Exists 门）"
            );
            None
        }
        Err(e) => {
            glog!(
                "WRN",
                "加载 Envir/Npcs.txt 失败（{e}）⇒ 不放置固定 NPC（oracle LoadNpcs 无 try/catch ⇒ 宿主登记）"
            );
            None
        }
    };
    // **R0-2/R0-3/R0-4 内容装载红名单**：`Envir/MiniMap.txt` + `Envir/MakeItem.txt` +
    // `Notice/LineNotice.txt` 的宿主装载。oracle 时机：
    // - `GameApp.cs:118 Map.LoadMinMap()`（`Initialize` 里物品库之后）；
    // - `GameApp.cs:231 LocalDb.LoadMakeItem()`（`InitializeWorld` 里 `LoadNpcs` 之后）；
    // - `GameApp.cs:204 M2Share.LoadLineNotice(SystemShare.GetNoticeFilePath("LineNotice.txt"))`
    //   （`Initialize` 里公告之后）。
    // 解析半在 `crates/envir`（MiniMap/MakeItem）与本 crate 的 `notice.rs`（LineNotice）；
    // 本块只做「读文件 + 解析 + 日志」，结果登记进 [`EnterConfig`]（消费点属 R1 入站红名单，未实现 ⇒ 登记）。
    let minimap = match game_svr::contentload::load_minimap_file(&envir_loader) {
        Ok(Some((reg, dups))) => {
            // 复刻 `Maps.cs:411 LogService.Error($"重复小地图配置信息[...]")`。
            for name in &dups {
                glog!("ERR", "重复小地图配置信息[{name}]");
            }
            glog!("INF", "小地图数据加载成功...[{}]", reg.len());
            Some(reg)
        }
        Ok(None) => {
            glog!(
                "INF",
                "无 Envir/MiniMap.txt ⇒ 不载小地图（oracle File.Exists 门）"
            );
            None
        }
        Err(e) => {
            glog!("WRN", "加载 Envir/MiniMap.txt 失败（{e}）⇒ 不载（登记）");
            None
        }
    };
    let make_items = match game_svr::contentload::load_make_item_file(&envir_loader) {
        Ok(Some(reg)) => {
            glog!("INF", "炼制物品信息加载成功...[{}]", reg.len());
            Some(reg)
        }
        Ok(None) => {
            glog!(
                "INF",
                "无 Envir/MakeItem.txt ⇒ 不载合成配方（oracle File.Exists 门）"
            );
            None
        }
        Err(e) => {
            glog!(
                "WRN",
                "加载 Envir/MakeItem.txt 失败（{e}）⇒ 不载（oracle 异常穿透 ⇒ 登记）"
            );
            None
        }
    };
    let line_notice = notice::load_line_notice(&base, &cfg.notice_dir);
    if line_notice.is_empty() {
        glog!(
            "INF",
            "无 Notice/LineNotice.txt（或为空）⇒ 不载滚动公告（oracle File.Exists 门）"
        );
    } else {
        glog!("INF", "公告提示信息加载成功...[{}]", line_notice.len());
    }
    // **M3⑨-实现 2 第②片**：`Server.conf` 的宝宝配置（`PetSlaveConfig::from_ini`，
    // ① 已交付但未接生产路径）。`DoSpell` 召唤面用它选型。读不到 ⇒ oracle 初值（登记）。
    let pet_cfg = match game_svr::slave::pet_slave_config_from_conf(&conf) {
        Ok(c) => c,
        Err(e) => {
            glog!(
                "WRN",
                "读宝宝配置失败（{e}）⇒ 用 PetSlaveConfig 初值（登记）"
            );
            mir2_data::pet_slave::PetSlaveConfig::default()
        }
    };
    // **M3⑩-实现 2**：`goldsales` **只读**加载接线（oracle `src/GameSvr/GameApp.cs:30` 建空列表，
    // `:181-182` 日志 + `GameShare.DataSource.LoadSellOffItemList()`）。承载 = sim 线程里
    // `M2Share.SellOffItemList` 对应的 [`game_svr::selloff::SellOffBook`]（⑩-1 落）；本块把
    // 加载结果按 `MySqlDB.cs:279` 的 `Add` 顺序尾追成种子。
    // 缺陷照搬：加载器读错列名（`DealChrName`/`BuyChrName` vs DDL `DealCharName`/`BuyCharName`，
    // `sql/mir2_data.sql:26-27`）⇒ **表里有数据也恒 0 行**（`D-5`/`B-5`）；
    // **不实现 save**（`SaveSellOffItemList` 三个调用点在 oracle 全被注释）。
    // 失败按 oracle `!Open()` 分支（`MySqlDB.cs:254-258`）降级：记 ERR、列表保持空（登记，不 panic）。
    let sell_off_seed = {
        glog!("INF", "加载物品寄售系统...");
        let conn = cfg
            .db_conn_string
            .as_deref()
            .unwrap_or(items::DEFAULT_CONN_STRING);
        match goldsales::load_goldsales(conn).await {
            Ok(rows) => {
                glog!("INF", "读取物品寄售列表成功...[{}]", rows.len());
                let mut book = game_svr::selloff::SellOffBook::default();
                goldsales::seed_book(&mut book, &rows);
                book.entries
            }
            Err(e) => {
                glog!(
                    "ERR",
                    "读取物品寄售列表失败（{e}）；列表保持空（oracle !Open() 分支）"
                );
                Vec::new()
            }
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
                    magic_table,
                    pet_cfg,
                    magic_attack_rage: i32::from(cfg.magic_attack_rage),
                    ground: GroundCfg {
                        human_max_gold: cfg.human_max_gold,
                        floor_item_can_pick_up_time: cfg.floor_item_can_pick_up_time,
                        control_drop_item: cfg.control_drop_item,
                        in_safe_disable_drop: cfg.in_safe_disable_drop,
                        can_drop_price: cfg.can_drop_price,
                        drop_item_rage: cfg.drop_item_rage,
                    },
                    shop: ShopCfg {
                        castle_member_price_rate: cfg.castle_member_price_rate,
                        super_repair_price_rate: cfg.super_repair_price_rate,
                        repair_item_dec_dura: cfg.repair_item_dec_dura,
                        get_all_npc_tax: cfg.castle_get_all_npc_tax,
                        human_max_gold: cfg.human_max_gold,
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
                    guards,
                    minimap,
                    make_items,
                    line_notice,
                    npcs,
                    loader: Some(envir_loader),
                    script_goto_count_limit: cfg.script_goto_count_limit,
                    trade: game_svr::trade::DealConfig {
                        try_deal_time: cfg.try_deal_time,
                        deal_ok_time: cfg.deal_ok_time,
                        can_not_get_back_deal: cfg.can_not_get_back_deal,
                        disable_deal: cfg.disable_deal,
                        human_max_gold: cfg.human_max_gold,
                        green_f_color: cfg.green_msg_f_color,
                        green_b_color: cfg.green_msg_b_color,
                        red_f_color: cfg.red_msg_f_color,
                        red_b_color: cfg.red_msg_b_color,
                        ..game_svr::trade::DealConfig::default()
                    },
                    // 元宝寄售（`M3⑩-实现 1`）：`DecUserGameGold` 无配置键（`GameSvrConf.cs:1217` 硬编码
                    // 10000）；`MaxBagItem` = `Grobal2.MaxBagItem`；蓝色提示色从 `[Setup]` 读（现网 251/252）。
                    sell_off: game_svr::selloff::SellOffConfig {
                        dec_user_game_gold: 10_000,
                        max_bag_item: game_svr::bag::MAX_BAG_ITEM,
                        blue_f_color: cfg.blue_msg_f_color,
                        blue_b_color: cfg.blue_msg_b_color,
                    },
                    // `M3⑩-实现 2`：`goldsales` 只读加载的启动种子（oracle `M2Share.SellOffItemList`
                    // 的内容；因读错列名 ⇒ 生产下恒空，见本文件加载块注释）。
                    sell_off_seed,
                },
                notice,
                colors,
                partywire::world_config(
                    cfg.group_members_max,
                    cfg.group_msg_f_color,
                    cfg.group_msg_b_color,
                    cfg.red_msg_f_color,
                    cfg.red_msg_b_color,
                ),
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

/// `openmir2-m3-party-group-state`：4 条组队 `CM_*` 的派发 + 帧路由。
///
/// oracle：`PlayObject.Message.cs:914-935` 的 `CM_GROUPMODE(1019)` / `CM_CREATEGROUP(1020)` /
/// `CM_ADDGROUPMEMBER(1021)` / `CM_DELGROUPMEMBER(1022)` 支；后三条把 `processMsg.Msg.Trim()`
/// 交给 `ClientCreateGroup`/`ClientAddGroupMember`/`ClientDelGroupMember`。
///
/// **收包字段口径**：`CM_GROUPMODE` 判的是 `processMsg.nParam2`（12B 头的 `Param` 字段）；
/// 其余三条用尾 `Msg`（`Recog`/`Tag` 不参与）。
///
/// 世界侧状态机在 `crates/world/src/party.rs`；本函数只负责「找会话 → 调状态机 → 把事件按
/// `[GateIdx, Socket]` 回帧」（事件顺序已按 oracle 的「立即帧 / `MsgQueue` 出队序」排好，别重排）。
///
/// `PartyEvent::GotoLabel`（oracle `FunctionNPC.GotoLable(this, "@GroupClose"/"@GroupCreate"/
/// "@GroupAddMember"/"@GroupDelMember", false)`，`Operate.cs:806/843/881/907`）**不是帧**：
/// 在本位置执行 `QFunction` 脚本（见 [`run_party_goto_label`]），脚本说词经 `RM_MERCHANTSAY`
/// （优先级字节 1）→ `SM_MERCHANTSAY` 下发。
#[allow(clippy::too_many_arguments)]
fn handle_party_cmd(
    ident: u16,
    param: u16,
    tail: Option<&str>,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    world: &mut mir2_world::World,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    party_cfg: &mir2_world::PartyConfig,
    saves: &mut std::collections::HashMap<String, SavedHuman>,
    ground_sessions: &mut std::collections::HashMap<String, SessionGround>,
    npc_host: Option<&game_svr::npc::NpcHost>,
    catalog: &dyn game_svr::ability::ItemCatalog,
) {
    let Some(sender) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "组队 CM：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let actor = sender.actor_id;
    let target = tail.unwrap_or("").trim();
    let events = match ident {
        mir2_protocol::messages::CM_GROUPMODE => world.party_group_mode(actor, param, party_cfg),
        mir2_protocol::messages::CM_CREATEGROUP => {
            world.party_create_group(actor, target, party_cfg)
        }
        mir2_protocol::messages::CM_ADDGROUPMEMBER => {
            world.party_add_group_member(actor, target, party_cfg)
        }
        mir2_protocol::messages::CM_DELGROUPMEMBER => {
            world.party_del_group_member(actor, target, party_cfg)
        }
        _ => return,
    };
    let mut delivered = 0usize;
    for ev in &events {
        if let mir2_world::PartyEvent::GotoLabel { label, .. } = ev {
            // oracle `FunctionNPC.GotoLable`：不是帧。在此位置执行脚本，脚本说词经
            // `RM_MERCHANTSAY`（优先级字节 1）→ `SM_MERCHANTSAY` 在本批帧序里就地下发。
            run_party_goto_label(
                label,
                chr_name,
                socket,
                online,
                world,
                saves,
                ground_sessions,
                npc_host,
                catalog,
                gates,
            );
            continue;
        }
        let Some((to, body)) = partywire::event_body(ev) else {
            continue;
        };
        let Some(t) = online.iter().find(|e| e.actor_id == to) else {
            continue;
        };
        let Some(tx) = route_to_gate(gates, t.gate_idx, t.socket, "组队回帧") else {
            continue;
        };
        let _ = tx.send(SimToGate::SendFrame {
            socket: t.socket,
            body,
        });
        delivered += 1;
    }
    glog!(
        "INF",
        "组队 ChrName={chr_name} ActorId={actor} ident={ident} param={param} target={target:?} → {delivered} 帧（事件 {} 条）",
        events.len()
    );
}

/// `SystemShare.FunctionNPC.GotoLable(this, label, false)`（`PlayObject.Operate.cs:806/843/881/907`）。
///
/// 由 [`handle_party_cmd`] 在 `PartyEvent::GotoLabel` 的**位置**调用（位置 = oracle 排队序：
/// `RM_MERCHANTSAY` 优先级字节 `1`，排在组队其它排队帧前）。
///
/// oracle 守卫 `if (SystemShare.FunctionNPC != null)` ⇒ `QFunction` 未装载（[`NpcHost::has_function_npc`]
/// 为 false）时不执行。`NpcPlayer` 取数/回写与进世界块的 `@Login`（`PlayObject.cs:1440`）同法：
/// 临时移出 `world.rng` 供脚本抽数后归位，脚本改写的 `Gold`/`QuestFlag` 回写存档与地面会话。
#[allow(clippy::too_many_arguments)]
fn run_party_goto_label(
    label: &str,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    world: &mut mir2_world::World,
    saves: &mut std::collections::HashMap<String, SavedHuman>,
    ground_sessions: &mut std::collections::HashMap<String, SessionGround>,
    npc_host: Option<&game_svr::npc::NpcHost>,
    catalog: &dyn game_svr::ability::ItemCatalog,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
) {
    let Some(host) = npc_host else {
        return;
    };
    if !host.has_function_npc() {
        return;
    }
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        return;
    };
    let actor = entry.actor_id;
    let gate_idx = entry.gate_idx;
    let Some(saved) = saves.get(chr_name) else {
        glog!(
            "WRN",
            "组队 GotoLable({label})：ChrName={chr_name} 无存档记录 ⇒ 不执行"
        );
        return;
    };
    // `@Login` 同法取数；但组队发生在**进世界之后**，此时 `PlayObject.Gold/ItemList/WAbil`
    // 的运行时真身在 `ground_sessions`（`saves.record` 只在读档/存档时刷新）⇒ 优先取运行时，
    // 无会话（已断线/未进世界）回退到读档记录。`QuestFlag` 运行时唯一存储就在 `saves.record`。
    let map_name = saved.enter_map.clone();
    let quest_flag = saved.record.data.quest_flag.clone();
    let rec_level = saved
        .record
        .data
        .abil
        .as_ref()
        .map_or(1, |a| i32::from(a.level));
    let rec_gold = saved.record.data.gold;
    let rec_bag = saved.record.data.bag_items.clone();
    let runtime = ground_sessions.get(chr_name);
    let level = runtime.map_or(rec_level, |sg| i32::from(sg.state.wabil.level));
    let gold = runtime.map_or(rec_gold, |sg| sg.state.gold);
    let bag = runtime.map_or(rec_bag, |sg| Some(bag_items_for_save(&sg.state.item_list)));
    let (group_owner, group_member_jobs) = world.party_script_group_snapshot(actor);
    // 脚本取数走**同一条** `world.rng`（临时移出后归位，与 `@Login`/`step_mon_gen` 同法）。
    let mut rng = std::mem::replace(&mut world.rng, mir2_shared::rng::RandomNumber::with_seed(0));
    let mut says_len = 0usize;
    let mut sent = 0usize;
    {
        let mut rngf = |v: i32| rng.random_below(v);
        let mut player = npc_player_from(
            &mut rngf,
            chr_name,
            actor,
            &map_name,
            level,
            gold,
            bag.as_deref(),
            quest_flag.as_deref(),
            catalog,
            group_owner,
            &group_member_jobs,
        );
        if let Some(says) = host.goto_label("QFunction", &mut player, label, false) {
            says_len = says.len();
            if let Some(tx) = route_to_gate(gates, gate_idx, socket, "组队 GotoLable 说词") {
                sent = emit_npc_says(tx, socket, actor, &says);
            }
        }
        // 回写（`@Login` 同法）：脚本改写的 `Gold`/`QuestFlag`。
        if let Some(s) = saves.get_mut(chr_name) {
            s.record.data.gold = player.gold;
            if let Some(qf) = s.record.data.quest_flag.as_mut() {
                let n = qf.len().min(player.quest_flag.as_bytes().len());
                qf[..n].copy_from_slice(&player.quest_flag.as_bytes()[..n]);
            }
        }
        if let Some(sg) = ground_sessions.get_mut(chr_name) {
            sg.state.gold = player.gold;
        }
    }
    world.rng = rng;
    glog!(
        "INF",
        "组队 GotoLable({label}) 已执行 ChrName={chr_name} 说词 {says_len} 条（下发 {sent}）"
    );
}

/// **M3⑦-实现 2 F-1**：oracle `PlayObject.Die()` 的 `DelMember` 死亡退组帧下发。
///
/// `world.tick()` 在 `settle_deaths`（`hp == 0` 的死亡写入点，= oracle `Die()` 的等价处）里按
/// `PlayObject.cs:1975-1977` 调 `world.party_del_member`，事件落在 `TickRecord::party_events`。
/// 与断线路径（`Disappear()`，`PlayObject.Message.cs:2138-2142`）的差别：**死亡者仍在线**
/// ⇒ 它自己的回帧照发（`online` 里找得到它），**不要**复用「已摘 online ⇒ 丢自身帧」口径。
/// 事件顺序已按 oracle 的「立即帧 / `MsgQueue` 出队序」排好，别重排。
fn drain_party_death_events(
    rec: &mir2_world::TickRecord,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
) {
    if rec.party_events.is_empty() {
        return;
    }
    let mut delivered = 0usize;
    for ev in &rec.party_events {
        let (to, body) =
            partywire::event_body(ev).expect("死亡退组路径不含 GotoLabel（无脚本触发）");
        let Some(t) = online.iter().find(|e| e.actor_id == to) else {
            continue;
        };
        let Some(tx) = route_to_gate(gates, t.gate_idx, t.socket, "死亡退组回帧") else {
            continue;
        };
        let _ = tx.send(SimToGate::SendFrame {
            socket: t.socket,
            body,
        });
        delivered += 1;
    }
    glog!(
        "INF",
        "死亡退组（oracle Die→DelMember）：{} 条事件 → {delivered} 帧",
        rec.party_events.len()
    );
}

/// **M3⑨-3 帧面残余（卡 `openmir2-m3-pet-slave-masterdie-wire`）**：
/// `PlayObject.ProcessSlaveMutiny`（`PlayObject.Base.cs:331-359`）末两行
/// （`:351-352` `SlaveList[i].RefNameColor(); SlaveList[i].RefShowName();`）的**下行帧**。
///
/// 状态层（`World::deliver_self_delay_msgs` → `slave::process_slave_mutiny`）已由本卡前片落地，
/// 事件落在 [`mir2_world::TickRecord::slave_self_deliveries`]；本函数只负责把 `mutinied` 出帧（
/// `crates/world/src/world.rs:107-109` 的「宿主据本记录出帧」）。
///
/// # oracle 链路（逐行）
///
/// - `RefNameColor()`（`BaseObject.cs:730-733`）= `SendRefMsg(RM_CHANGENAMECOLOR, 0,0,0,0, "")`；
/// - `RefShowName()`（`BaseObject.cs:1174-1177`）= `SendRefMsg(RM_USERNAME, 0,0,0,0, GetShowName())`；
/// - `SendRefMsg`（`BaseObject.Message.cs:383-475`）对**同图**、`Race == Play`、非 `Ghost`、
///   切比雪夫 ≤ `Config.SendRefMsgRange`（缺省 12）的对象 `SendMsg(this, ident, …)`；
///   非 `Play` 目标仅在 `RM_STRUCK/RM_HEAR/RM_DEATH` 时收 ⇒ 这两条只发玩家；
/// - 观察者侧 `RM_CHANGENAMECOLOR` 支（`PlayObject.Message.cs:1677-1679`）⇒ `SM_CHANGENAMECOLOR(656)`
///   载荷 `(Recog=ActorId, Param=GetChrColor(baseObject), 0, 0)`，无尾；
///   `RM_USERNAME` 支（`:1662-1664`）⇒ `SM_USERNAME(42)` 同载荷 + 尾 `EncodeString(GetShowName())`。
///
/// # 次序（观察者 `MsgQueue` 的**堆出队序**，不是源码入队序）
///
/// oracle `SendMsg`（`BaseObject.Message.cs:98/181`：`SendMsg(IActor, …)` 的 `MsgQueue.Enqueue(msg, (byte)wIdent)`）入队，
/// 而 `MsgQueue` 是 `PriorityQueue<SendMessage, byte>`（`ActorEntity.cs:16`）。入队序 = 每只宝宝
/// `RefNameColor(); RefShowName();`（`:351-352`），宝宝按 `SlaveList` **末尾往前**
/// （[`mir2_world::slave::process_slave_mutiny`] 返回的就是该序）。
///
/// 出队序 = **优先级字节升序**：`(byte)RM_CHANGENAMECOLOR` = `8090 & 0xFF` = **154**
/// < `(byte)RM_USERNAME` = `8101 & 0xFF` = **165** ⇒ **同一观察者上所有宝宝的色帧先于所有名帧**。
/// 但**同优先级不是入队序** —— .NET 8 的 `PriorityQueue` 是 **4 叉最小堆**
/// （`Arity=4`/`Log2Arity=2`），同优先级元素按堆的 sift 结果出队
/// （reviewer-6 F-1：入队 `色(s2),名(s2),色(s1),名(s1)` ⇒ 出队
/// `色(s2),色(s1),名(s1),名(s2)`，名组与入队序**相反**）。
/// 故本函数按 oracle 入队序灌入 [`game_svr::msgqueue::NetPriorityQueue`]（逐位复刻该 4 叉堆）
/// 再出队编码，而不是「全部色循环 + 全部名循环」。
///
/// # 登记（未 1:1，真机项）
///
/// - oracle `SendRefMsg` 的 `VisibleHumanList` 有 **500ms 缓存**（`BaseObject.Message.cs:395`）：缓存命中支
///   （`:453-460`）按 `< 11` 的窄门重扫，重算支（`:399-402`）按 `SendRefMsgRange`=12 的 25×25 块；
///   本实现每 tick 现算现发（同
///   [`drain_combat_downlink`] 口径）—— 真机两侧同序列对拍时须对齐。
/// - 帧先入观察者 `MsgQueue`，下一轮 `Run()` 才回 `SM_*`；本实现当 tick 直发（同既有下行族）。
/// - `GetShowName()` 的 `Master != null ⇒ "(主人名)"` 后缀：本路径 `process_slave_mutiny` 已把
///   `Master` 置 `None` ⇒ 不可达；`NpcNameColor` 分支同理（叛变宝宝恒非 NPC）⇒ 传 0 且不消费。
/// - `ghosted` 支（`MakeGhost` 的 `RM_MASTERDIEGHOST`）在 oracle 里**恒不可达**（本卡 BUG-REPLICA ④）
///   ⇒ 本函数不产帧。
/// - 本模块只复刻**堆算法**；oracle 真实出队序还受「入队前队列里已有其它消息」影响（跨 tick、
///   其它下行），本项目不建模整条观察者队列 ⇒ 混合队列场景登记 `not_verified`（R 级）。
///
/// 每次入队的帧（按 `MsgQueue` 出队序编码）。
enum MutinyFrame {
    /// `RM_CHANGENAMECOLOR`（优先级字节 154）。
    NameColor { sid: i32, color: u8 },
    /// `RM_USERNAME`（优先级字节 165）。
    UserName {
        sid: i32,
        color: u8,
        show_name: Vec<u8>,
    },
}

#[allow(clippy::too_many_arguments)]
fn drain_slave_mutiny_downlink(
    rec: &mir2_world::TickRecord,
    world: &mir2_world::World,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    cfg: &mir2_data::pet_slave::PetSlaveConfig,
    range: i32,
) -> usize {
    if rec.slave_self_deliveries.is_empty() {
        return 0;
    }
    // `NpcNameColor` 未在 Rust 配置层建模；本路径 `Race != NPC` ⇒ 不可达不消费（见函数头登记）。
    const NPC_NAME_COLOR_UNMODELED: u8 = 0;
    // 每只叛变宝宝的 `(观察者, 宝宝 id, 色, 名)`（宝宝按 `mutinied` 序 = `SlaveList` 末尾往前）。
    let mut hits: Vec<(i32, i32, u8, Vec<u8>)> = Vec::new();
    for outcome in &rec.slave_self_deliveries {
        if outcome.ident != mir2_world::slave::RM_MASTERDIEMUTINY {
            continue;
        }
        for sid in &outcome.mutinied {
            let Some(e) = world.entities.get(*sid) else {
                continue;
            };
            // 观察者侧 `GetChrColor(baseObject)`：叛变后 `Master == null` ⇒ 怪的 `NameColor`。
            let color =
                mir2_world::slave::get_chr_color(e, &cfg.slave_color, NPC_NAME_COLOR_UNMODELED);
            // `BaseObject.GetShowName()`（`BaseObject.Base.cs:779`）：怪物走 `FilterShowName`（GBK 字节）。
            let show_name = downlink::gbk(downlink::filter_show_name(&e.name));
            for observer in game_svr::chat::hear_targets(world, *sid, range) {
                hits.push((observer, *sid, color, show_name.clone()));
            }
        }
    }
    if hits.is_empty() {
        return 0;
    }
    let mut observers: Vec<i32> = hits.iter().map(|h| h.0).collect();
    observers.sort_unstable();
    observers.dedup();
    let mut sent = 0usize;
    for observer in observers {
        let Some(entry) = online.iter().find(|e| e.actor_id == observer) else {
            continue;
        };
        let Some(tx) = route_to_gate(gates, entry.gate_idx, entry.socket, "叛变名色帧") else {
            continue;
        };
        // 观察者 `MsgQueue` = `PriorityQueue<SendMessage, byte>`（`ActorEntity.cs:16`）：入队序 = 每只宝宝
        // `RefNameColor(); RefShowName();`（`PlayObject.Base.cs:351-352`），出队 = 4 叉堆序。
        // 按 oracle 入队序灌堆再出队，**不能**用「所有色循环 + 所有名循环」（同优先级非入队序，F-1）。
        let mut queue: game_svr::msgqueue::NetPriorityQueue<MutinyFrame> =
            game_svr::msgqueue::NetPriorityQueue::new();
        for h in hits.iter().filter(|h| h.0 == observer) {
            queue.enqueue(
                MutinyFrame::NameColor {
                    sid: h.1,
                    color: h.2,
                },
                (mir2_protocol::messages::RM_CHANGENAMECOLOR & 0xFF) as u8,
            );
            queue.enqueue(
                MutinyFrame::UserName {
                    sid: h.1,
                    color: h.2,
                    show_name: h.3.clone(),
                },
                (mir2_protocol::messages::RM_USERNAME & 0xFF) as u8,
            );
        }
        while let Some(frame) = queue.dequeue() {
            let body = match frame {
                MutinyFrame::NameColor { sid, color } => {
                    game_svr::smframe::sm_changenamecolor(sid, color)
                }
                MutinyFrame::UserName {
                    sid,
                    color,
                    show_name,
                } => game_svr::smframe::sm_username(sid, color, &show_name),
            };
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body,
            });
            sent += 1;
        }
    }
    sent
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
    // `PlayObject.Dealing` / `PlayObject.DealLastTick`（M3⑦-实现 3/4）的真值来源。
    // 传整表而非两个散值：取值固定在 `handle_ground_cmd` 内，调用点无法各自漏接/写死。
    trade_sessions: &std::collections::HashMap<String, game_svr::trade::DealState>,
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
    // `PlayObject.Dealing`（M3⑦-实现 3）：交易中禁拾取/禁丢弃。
    // `PlayObject.DealLastTick`（M3⑦-实现 4；`handle_deal_cmd` 用 `world.clock_ms` 写入）：
    // 新会话为 0 ⇒ 过 3000ms 门；交易中才拦 drop（`Operate.cs:269`）。
    let dealing = trade_sessions.get(chr_name).is_some_and(|s| s.dealing);
    let deal_last_tick = trade_sessions
        .get(chr_name)
        .map_or(0, |s| s.deal_last_tick as i32);
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
        // `ClientPickUpItemIsOfGroup`（`PlayObject.Base.cs:38-54`）：M3⑦-实现 4 接上真注册表。
        let group_owner = world.entities.get(actor_id).map_or(0, |e| e.group_owner);
        let group_member_actor_ids: Vec<i32> = if group_owner != 0 {
            world
                .entities
                .get(group_owner)
                .map(|e| e.group_members.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let ctx = game_svr::ground::PickUpContext {
            catalog,
            actor_id,
            curr_x: cx,
            curr_y: cy,
            now_tick,
            // `Dealing`（M3⑦-实现 3）：交易中禁拾取（`PlayObject.Base.cs:57`）。
            dealing,
            floor_item_can_pick_up_time: cfg.floor_item_can_pick_up_time,
            human_max_gold: cfg.human_max_gold,
            // `Race == ActorRace.Play`（`SendAddItem` 仅在玩家时发）⇒ 恒真（本宿主只收玩家）。
            race_is_play: true,
            // `ClientPickUpItemIsOfGroup`（`PlayObject.Base.cs:38-54`）：M3⑦-实现 4 接上真注册表。
            group_owner,
            group_member_actor_ids: &group_member_actor_ids,
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
            // `PlayObject.DealLastTick`（M3⑦-实现 4）：交易会话真值（`handle_deal_cmd` 用
            // `world.clock_ms` 写入）⇒ 新会话为 0（过 3000ms 门），交易中才拦 drop。
            deal_last_tick,
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

/// `BaseObject.GetPoseCreate()`（`src/M2Server/Actor/BaseObject.cs:1253-1263`）：
/// `GetFrontPosition()` 命中 ⇒ `Envir.GetMovingObject(nX, nY, true)`，即**前方格**首个
/// `!Ghost && !Death` 的对象。朝向取展示表（`crates/world` 的 `Entity` 无 `dir`），缺省 `0`。
fn pose_create(
    world: &mir2_world::World,
    displays: &downlink::DisplayTable,
    actor_id: i32,
) -> Option<i32> {
    let e = world.entities.get(actor_id)?;
    let dir = displays.get(actor_id).map_or(0, |d| d.dir);
    let (width, height) = world
        .maps
        .get(e.map_id)
        .map_or((0, 0), |m| (m.width, m.height));
    let (fx, fy) = game_svr::attackwire::front_position(dir, e.x, e.y, width, height);
    if (fx, fy) == (e.x, e.y) {
        return None;
    }
    front_target(world, actor_id, e.map_id, fx, fy)
}

/// `ClientDealTry` 的 `GetPoseCreate()` 目标判定（`PlayObject.Operate.cs:932-960`）。
///
/// oracle 在 `:932` 取 `GetPoseCreate()`、`:933` 对结果直接取 `poseObject.Race`：
/// - `null` ⇒ NullReferenceException，被 `WorldServer.ProcessPlayObjectData`（方法体
///   `:625-737`）的 `catch`（`WorldServer.cs:732-735`；该 catch 的日志串写作
///   `WorldServer::ProcessHumans`，与包围方法名不符）吞掉 ⇒ **一个帧都不发**（不是 `SM_DEALTRY_FAIL`）；
/// - 非玩家 ⇒ `:958-960` 的 `else` ⇒ `SM_DEALTRY_FAIL`；
/// - 玩家且 `== this` ⇒ `:936` 内层 `if (targetPlayObject != this)` 无 `else` ⇒ **不发帧**；
/// - 玩家且 `!= this` ⇒ `:938` 取对方 `GetPoseCreate()` 是否回指自己（`faces_me`）。
fn deal_try_target_of(
    world: &mir2_world::World,
    displays: &downlink::DisplayTable,
    actor_id: i32,
    pose_id: Option<i32>,
) -> (Option<i32>, game_svr::trade::DealTryTarget) {
    match pose_id {
        None => (None, game_svr::trade::DealTryTarget::Absent),
        Some(oid) if oid == actor_id => (None, game_svr::trade::DealTryTarget::Absent),
        Some(oid) if !is_play_actor(world, oid) => {
            (None, game_svr::trade::DealTryTarget::NotPlayer)
        }
        Some(oid) => {
            let faces_me = pose_create(world, displays, oid) == Some(actor_id);
            (
                Some(oid),
                game_svr::trade::DealTryTarget::Other { faces_me },
            )
        }
    }
}

/// `poseObject.Race == ActorRace.Play`（`PlayObject.Operate.cs:933`）。
fn is_play_actor(world: &mir2_world::World, id: i32) -> bool {
    world
        .entities
        .get(id)
        .is_some_and(|e| e.race == mir2_world::ACTOR_RACE_PLAY)
}

/// `CM_DEALTRY/ADDITEM/DELITEM/CANCEL/CHGGOLD/END` 的派发（M3⑦-实现 3）。
///
/// 口径：纯状态机在 [`game_svr::trade`]；本函数只负责「找会话 → 解析 `GetPoseCreate()` →
/// 取出双方会话态 → 调用 → 回写 → 逐帧按 `[GateIdx, Socket]` 路由」。
///
/// **入帧映射**（`WorldServer.ProcessUserMessage` 的 `default`/显式支，`WorldServer.cs:820`）：
/// `nParam1 = defMsg.Recog` ⇒ `CM_DEALADDITEM/DELITEM` 的 `nItemIdx`、`CM_DEALCHGGOLD` 的 `nGold`
/// 都取 **`recog`**（32 位）；`Msg` = 尾。`CM_DEALTRY` 的名字参数 oracle **不用**
/// （`ClientDealTry` 内部走 `GetPoseCreate()`），本函数同理只传目标判定。
#[allow(clippy::too_many_arguments)]
fn handle_deal_cmd(
    ident: u16,
    recog: i32,
    tail: Option<&str>,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    world: &mir2_world::World,
    displays: &downlink::DisplayTable,
    ground_sessions: &mut std::collections::HashMap<String, SessionGround>,
    trade_sessions: &mut std::collections::HashMap<String, game_svr::trade::DealState>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    cfg: &game_svr::trade::DealConfig,
    catalog: &dyn game_svr::ability::ItemCatalog,
) {
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "交易命令：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let actor_id = entry.actor_id;
    if ground_sessions.get(chr_name).is_none() {
        glog!(
            "WRN",
            "交易命令：ChrName={chr_name} 无会话态（未进世界/已断线）⇒ 丢弃 ident={ident}"
        );
        return;
    }
    let now = world.clock_ms;

    // 目标解析：`CM_DEALTRY` 用 `GetPoseCreate()`，其余命令用已建立的 `DealCreat`。
    let pose_id = pose_create(world, displays, actor_id);
    let (other_id, try_target) = if ident == mir2_protocol::messages::CM_DEALTRY {
        deal_try_target_of(world, displays, actor_id, pose_id)
    } else {
        (
            trade_sessions.get(chr_name).and_then(|s| s.deal_creature),
            game_svr::trade::DealTryTarget::Other { faces_me: false },
        )
    };
    let other_chr = other_id.and_then(|oid| {
        online
            .iter()
            .find(|e| e.actor_id == oid)
            .map(|e| e.chr_name.clone())
    });
    let other_is_pose = other_id.is_some() && pose_id == other_id;

    // 双方会话态取出（对手缺席 ⇒ 用哑元；`trade` 的每个操作都先查本侧 `DealCreat` 门）。
    let mut me_sg = ground_sessions.remove(chr_name).expect("上面已确认存在");
    let mut me_tr = trade_sessions.remove(chr_name).unwrap_or_default();
    let mut other_sg = other_chr.as_ref().and_then(|c| ground_sessions.remove(c));
    let mut other_tr = other_chr.as_ref().and_then(|c| trade_sessions.remove(c));
    let mut dummy_ground = game_svr::ground::GroundState {
        item_list: Vec::new(),
        gold: 0,
        wabil: game_svr::ability::Ability::default(),
    };
    let mut dummy_tr = game_svr::trade::DealState::default();
    let other_name = other_chr.clone().unwrap_or_default();

    let events = {
        let (other_items, other_gold) = match other_sg.as_mut() {
            Some(sg) => (&mut sg.state.item_list, &mut sg.state.gold),
            None => (&mut dummy_ground.item_list, &mut dummy_ground.gold),
        };
        let other_tr_ref = other_tr.as_mut().unwrap_or(&mut dummy_tr);
        let mut me = game_svr::trade::DealSide {
            id: actor_id,
            name: chr_name.to_string(),
            items: &mut me_sg.state.item_list,
            gold: &mut me_sg.state.gold,
            gold_max: cfg.human_max_gold,
            state: &mut me_tr,
        };
        let mut ot = game_svr::trade::DealSide {
            id: other_id.unwrap_or(0),
            name: other_name.clone(),
            items: other_items,
            gold: other_gold,
            gold_max: cfg.human_max_gold,
            state: other_tr_ref,
        };
        match ident {
            mir2_protocol::messages::CM_DEALTRY => {
                game_svr::trade::deal_try(&mut me, &mut ot, cfg, now, try_target)
            }
            mir2_protocol::messages::CM_DEALADDITEM => game_svr::trade::deal_add_item(
                &mut me,
                &mut ot,
                cfg,
                catalog,
                recog,
                tail.unwrap_or(""),
                now,
            ),
            mir2_protocol::messages::CM_DEALDELITEM => game_svr::trade::deal_del_item(
                &mut me,
                &mut ot,
                cfg,
                catalog,
                recog,
                tail.unwrap_or(""),
                now,
            ),
            mir2_protocol::messages::CM_DEALCANCEL => {
                game_svr::trade::deal_cancel(&mut me, &mut ot, cfg, now)
            }
            mir2_protocol::messages::CM_DEALCHGGOLD => {
                game_svr::trade::deal_change_gold(&mut me, &mut ot, cfg, recog, now, other_is_pose)
            }
            mir2_protocol::messages::CM_DEALEND => {
                game_svr::trade::deal_end(&mut me, &mut ot, cfg, now)
            }
            _ => Vec::new(),
        }
    };

    // 回写（对手缺席时 `other_sg`/`other_tr` 为 `None` ⇒ 不写）。
    ground_sessions.insert(chr_name.to_string(), me_sg);
    trade_sessions.insert(chr_name.to_string(), me_tr);
    if let (Some(c), Some(sg)) = (other_chr.clone(), other_sg) {
        ground_sessions.insert(c, sg);
    }
    if let (Some(c), Some(tr)) = (other_chr.clone(), other_tr) {
        trade_sessions.insert(c, tr);
    }

    // 编码 + 逐帧按目标自己的 `[GateIdx, Socket]` 路由。
    let wire_of = |tid: i32| -> game_svr::trade::DealWire {
        let sg = online
            .iter()
            .find(|e| e.actor_id == tid)
            .and_then(|e| ground_sessions.get(&e.chr_name));
        match sg {
            Some(sg) => game_svr::trade::DealWire {
                actor_id: tid,
                gold: sg.state.gold,
                game_gold: sg.game_gold,
                wabil: sg.state.wabil.clone(),
            },
            None => game_svr::trade::DealWire {
                actor_id: tid,
                gold: 0,
                game_gold: 0,
                wabil: game_svr::ability::Ability::default(),
            },
        }
    };
    for ev in &events {
        let Some((target, body)) = game_svr::trade::deal_event_body(ev, catalog, &wire_of) else {
            continue;
        };
        let Some(t) = online.iter().find(|e| e.actor_id == target) else {
            glog!(
                "DBG",
                "交易回帧：目标 ActorId={target} 不在在线登记表 ⇒ 丢弃"
            );
            continue;
        };
        if let Some(tx) = route_to_gate(gates, t.gate_idx, t.socket, "交易回帧") {
            let _ = tx.send(SimToGate::SendFrame {
                socket: t.socket,
                body,
            });
        }
    }
}

/// 交易中断取消（掉线 `DealCancelA` / `Run()` 每 tick 的 `DealCancel`）：
/// 走 [`handle_deal_cmd`] 的 `CM_DEALCANCEL` 支（`deal_cancel` 递归取消双方）。
#[allow(clippy::too_many_arguments)]
fn deal_cancel_between(
    chr_name: &str,
    online: &OnlineRegistry,
    world: &mir2_world::World,
    displays: &downlink::DisplayTable,
    ground_sessions: &mut std::collections::HashMap<String, SessionGround>,
    trade_sessions: &mut std::collections::HashMap<String, game_svr::trade::DealState>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    cfg: &game_svr::trade::DealConfig,
    catalog: &dyn game_svr::ability::ItemCatalog,
) {
    handle_deal_cmd(
        mir2_protocol::messages::CM_DEALCANCEL,
        0,
        None,
        chr_name,
        0,
        online,
        world,
        displays,
        ground_sessions,
        trade_sessions,
        gates,
        cfg,
        catalog,
    );
}

/// `PlayObject.Run()` 每 tick 的交易自动取消（`PlayObject.Message.cs:30-36`）：
/// `Dealing` 且（`GetPoseCreate() != DealCreat` 或 `DealCreat == this` 或 `DealCreat == null`）
/// ⇒ `DealCancel()`。遍历前先按 `ChrName` 排序（确定性；`HashMap` 迭代序不是契约）。
#[allow(clippy::too_many_arguments)]
fn drain_deal_tick(
    world: &mir2_world::World,
    displays: &downlink::DisplayTable,
    online: &OnlineRegistry,
    ground_sessions: &mut std::collections::HashMap<String, SessionGround>,
    trade_sessions: &mut std::collections::HashMap<String, game_svr::trade::DealState>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    cfg: &game_svr::trade::DealConfig,
    catalog: &dyn game_svr::ability::ItemCatalog,
) {
    let mut names: Vec<String> = trade_sessions
        .iter()
        .filter(|(_, s)| s.dealing)
        .map(|(c, _)| c.clone())
        .collect();
    names.sort();
    for chr in names {
        if !trade_sessions.get(&chr).is_some_and(|s| s.dealing) {
            continue;
        }
        let Some(entry) = online.iter().find(|e| e.chr_name == chr) else {
            // 不在世界（已掉线）⇒ 由掉线路径的 `DealCancelA` 收口。
            continue;
        };
        let me_id = entry.actor_id;
        let pose = pose_create(world, displays, me_id);
        let deal_creature = trade_sessions.get(&chr).and_then(|s| s.deal_creature);
        let doomed =
            deal_creature.is_none() || deal_creature == Some(me_id) || pose != deal_creature;
        if doomed {
            deal_cancel_between(
                &chr,
                online,
                world,
                displays,
                ground_sessions,
                trade_sessions,
                gates,
                cfg,
                catalog,
            );
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
    dealing: bool,
    client_layout: bool,
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
        // `PlayObject.Dealing`（M3⑦-实现 4）：交易中禁穿装/卸装（`Operate.cs:501/525` 的
        // `!Dealing` 门）。
        dealing,
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
        recalc_session_ability(sg, catalog);
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
                &ability_tail(&sg.state.wabil, client_layout),
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

/// 商店会话态读助手：取 `PlayObject.ScriptLable`（未写过 ⇒ 空串）。
fn shop_script_lable(
    sessions: &std::collections::HashMap<String, ShopSession>,
    chr_name: &str,
) -> String {
    sessions
        .get(chr_name)
        .map(|s| s.script_lable.clone())
        .unwrap_or_default()
}

/// 构造 `game_svr::shop::ShopContext`（`price_rate` 随商人；其余随 `[Setup]`）。
fn shop_context<'a>(
    catalog: &'a dyn game_svr::ability::ItemCatalog,
    mprice: i32,
    mcastle: bool,
    cfg: &ShopCfg,
    script_lable: &'a str,
    player_actor_id: i32,
    colors: &MsgColors,
) -> game_svr::shop::ShopContext<'a> {
    game_svr::shop::ShopContext {
        catalog,
        cfg: mir2_formula::shop::ShopConfig {
            price_rate: mprice,
            castle_merchant: mcastle,
            // 城堡/行会未建模 ⇒ 非成员（`Merchant.cs:770` 的 `Castle.IsMasterGuild` 分支恒假；登记）。
            castle_member: false,
            castle_member_price_rate: cfg.castle_member_price_rate,
            super_repair_price_rate: cfg.super_repair_price_rate,
            repair_item_dec_dura: cfg.repair_item_dec_dura,
        },
        get_all_npc_tax: cfg.get_all_npc_tax,
        script_lable,
        human_max_gold: cfg.human_max_gold,
        player_actor_id,
        red_f: colors.red_f,
        red_b: colors.red_b,
    }
}

/// 按 `[GateIdx,Socket]` 下发商店帧（12B 头 + 尾）。
fn emit_shop_frames(
    tx: &mpsc::UnboundedSender<SimToGate>,
    socket: i32,
    frames: &[game_svr::shop::ShopFrame],
) {
    for f in frames {
        let header = game_svr::smframe::command_header(f.recog, f.ident, f.param, f.tag, f.series);
        // 尾编码口径：`SendDefMessage(...)`（`PlayObject.Socket.cs:26-43`）对未编码原文施加
        // `EDCode.EncodeString`；`SendSocket(ClientMsg, sMsg)` 直连（`SM_SAVEITEMLIST` / `SM_ADDITEM` 尾）
        // 已是线上字节 ⇒ 原样下发（`tail_encoded == true`）。空尾 `encode([]) == []` ⇒ header-only 不变。
        let body = if f.tail_encoded {
            game_svr::smframe::gm_data_body(header, &f.tail)
        } else {
            game_svr::smframe::gm_data_body(header, &mir2_protocol::edcode::encode(&f.tail))
        };
        let _ = tx.send(SimToGate::SendFrame { socket, body });
    }
}

/// 商店“找件”的门：`MakeIndex == nMakeIndex && 名 OrdinalIgnoreCase 相等`
/// （`PlayObject.Operate.cs:131/162/1558/1585` 四处同形）。
fn find_bag_item_by_make_index(
    item_list: &[game_svr::bag::BagItem],
    catalog: &dyn game_svr::ability::ItemCatalog,
    make_index: i32,
    wanted_name: &str,
) -> Option<usize> {
    item_list.iter().position(|it| {
        it.make_index == make_index
            && game_svr::shop::item_name(catalog, it.index)
                .is_some_and(|n| game_svr::shop::eq_label(&n, wanted_name))
    })
}

/// `ClientRepairItem`（`PlayObject.Operate.cs:1585-1597`）的「找件」口径 —— **BUG-REPLICA**。
///
/// oracle 循环体 `userItem = ItemList[i];` **无条件**赋值，未命中时循环结束后 `userItem` 仍是
/// **最后一件**背包物品（非 null）⇒ oracle 会继续修**最后一件**；只有 `ItemList` 为空才 `userItem
/// == null` 而 return。同源的 `ClientQueryRepairCost`（`:1558-1583`）用 `userItemA = null` 初值、
/// 仅在命中时赋值 ⇒ 未命中 return；**只有 repair 有这条 bug**（按 §1 硬约束③ 1:1 照搬）。
fn repair_target_index(
    item_list: &[game_svr::bag::BagItem],
    catalog: &dyn game_svr::ability::ItemCatalog,
    make_index: i32,
    wanted_name: &str,
) -> Option<usize> {
    find_bag_item_by_make_index(item_list, catalog, make_index, wanted_name)
        .or_else(|| item_list.len().checked_sub(1))
}

/// `CM_SELLOFFADDITEM`/`CM_SELLOFFDELITEM`/`CM_SELLOFFCANCEL`/`CM_SELLOFFEND`/
/// `CM_CANCELSELLOFFITEMING`/`CM_SELLOFFBUYCANCEL`/`CM_SELLOFFBUY` 的派发（M3⑩-实现 1）。
///
/// oracle 收包 → 回帧映射在 `PlayObject.Message.cs:2057-2077`，判定/状态迁移在
/// `PlayObject.Sale.cs`（纯函数在 [`game_svr::selloff`]）。本函数只做：找会话 →
/// 解析目标（`CM_SELLOFFBUY` 的卖方 = 尾里的名字，`GetPlayObject` 口径）→ 调纯函数 →
/// 按 `[GateIdx, Socket]` 回帧。
///
/// **入帧映射**（`WorldServer.cs:820` 的 `nParam1 = defMsg.Recog`）：`CM_SELLOFFADDITEM/DELITEM`
/// 取 `recog = MakeIndex`、`Msg`/`tail` = 物品名；`CM_SELLOFFEND` 取
/// `(Msg=sBuyChrName, nParam1=nSellGold, nParam2=nGameDiamond, nParam3=nCode)`；
/// `CM_SELLOFFBUYCANCEL/BUY` 取 `Msg = sDealChrName`。
///
/// `M2Share.SellOffItemList`（进程级）由调用方持有的 [`game_svr::selloff::SellOffBook`] 承载；
/// 每个在线角色的 `SaleDeal`/`SellOffConfirm`/`SellOffItemList` 存 `sell_off_sessions`。
#[allow(clippy::too_many_arguments)]
fn handle_selloff_cmd(
    ident: u16,
    recog: i32,
    param: u16,
    tag: u16,
    tail: Option<&str>,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    world: &mir2_world::World,
    ground_sessions: &mut std::collections::HashMap<String, SessionGround>,
    sell_off_sessions: &mut std::collections::HashMap<String, game_svr::selloff::SellOffState>,
    book: &mut game_svr::selloff::SellOffBook,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    cfg: &game_svr::selloff::SellOffConfig,
    catalog: &dyn game_svr::ability::ItemCatalog,
) {
    use mir2_protocol::messages;
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "元宝寄售命令：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let actor_id = entry.actor_id;
    if ground_sessions.get(chr_name).is_none() {
        glog!(
            "WRN",
            "元宝寄售命令：ChrName={chr_name} 无会话态（未进世界/已断线）⇒ 丢弃 ident={ident}"
        );
        return;
    }
    let now = world.clock_ms;
    let tail = tail.unwrap_or("");

    // 卖方（`CM_SELLOFFBUY` 用 `GetPlayObject(sDealChrName)`；`OrdinalIgnoreCase`）。
    let seller_chr: Option<String> = if ident == messages::CM_SELLOFFBUY {
        online
            .iter()
            .find(|e| e.chr_name.eq_ignore_ascii_case(tail))
            .map(|e| e.chr_name.clone())
    } else {
        None
    };

    let mut me_sg = ground_sessions.remove(chr_name).expect("上面已确认存在");
    let mut me_so = sell_off_sessions.remove(chr_name).unwrap_or_default();
    let mut seller_sg = seller_chr.as_ref().and_then(|c| ground_sessions.remove(c));
    let mut seller_so = seller_chr
        .as_ref()
        .and_then(|c| sell_off_sessions.remove(c));

    let events = {
        // 先取出只读快照，避免与 `items`/`gold` 的可变借用冲突。
        let me_weight = me_sg.state.wabil.weight;
        let me_wear_weight = me_sg.state.wabil.wear_weight;
        let me_hand_weight = me_sg.state.wabil.hand_weight;
        let me_max_weight = me_sg.state.wabil.max_weight;
        let me_game_point = me_sg.game_point;
        let me_send_gold = me_sg.soft_version_date != 0;
        let mut me = game_svr::selloff::SellOffSide {
            id: actor_id,
            name: chr_name.to_string(),
            // 在线会话（`PlayObject.OffLineFlag` 未建模 ⇒ 恒 false，登记）。
            offline: false,
            items: &mut me_sg.state.item_list,
            gold: &mut me_sg.state.gold,
            game_gold: &mut me_sg.game_gold,
            game_point: me_game_point,
            weight: me_weight,
            wear_weight: me_wear_weight,
            hand_weight: me_hand_weight,
            max_weight: me_max_weight,
            send_game_gold_info: me_send_gold,
            state: &mut me_so,
        };
        let mut seller = match (seller_sg.as_mut(), seller_so.as_mut()) {
            (Some(sg), Some(so)) => {
                let s_weight = sg.state.wabil.weight;
                let s_wear_weight = sg.state.wabil.wear_weight;
                let s_hand_weight = sg.state.wabil.hand_weight;
                let s_max_weight = sg.state.wabil.max_weight;
                let s_game_point = sg.game_point;
                let s_send_gold = sg.soft_version_date != 0;
                Some(game_svr::selloff::SellOffSide {
                    id: sg.actor_id,
                    name: seller_chr.clone().unwrap_or_default(),
                    offline: false,
                    items: &mut sg.state.item_list,
                    gold: &mut sg.state.gold,
                    game_gold: &mut sg.game_gold,
                    game_point: s_game_point,
                    weight: s_weight,
                    wear_weight: s_wear_weight,
                    hand_weight: s_hand_weight,
                    max_weight: s_max_weight,
                    send_game_gold_info: s_send_gold,
                    state: so,
                })
            }
            _ => None,
        };
        match ident {
            messages::CM_SELLOFFADDITEM => {
                game_svr::selloff::client_add_sell_off_item(&mut me, catalog, recog, tail)
            }
            messages::CM_SELLOFFDELITEM => {
                game_svr::selloff::client_del_sell_off_item(&mut me, catalog, recog, tail)
            }
            messages::CM_SELLOFFCANCEL => game_svr::selloff::client_cancel_sell_off(&mut me),
            messages::CM_SELLOFFEND => game_svr::selloff::client_sell_off_end(
                &mut me,
                book,
                catalog,
                cfg,
                now,
                tail,
                recog,
                i32::from(param),
                i32::from(tag),
            ),
            messages::CM_CANCELSELLOFFITEMING => {
                game_svr::selloff::client_cancel_sell_off_ing(&mut me, book, catalog, cfg)
            }
            messages::CM_SELLOFFBUYCANCEL => {
                game_svr::selloff::client_buy_cancel_sell_off(&mut me, book, tail)
            }
            messages::CM_SELLOFFBUY => game_svr::selloff::client_buy_sell_off_item(
                &mut me,
                seller.as_mut(),
                book,
                catalog,
                cfg,
                tail,
            ),
            _ => Vec::new(),
        }
    };

    // 回写（卖方缺席时 `seller_sg`/`seller_so` 为 `None` ⇒ 不写）。
    ground_sessions.insert(chr_name.to_string(), me_sg);
    sell_off_sessions.insert(chr_name.to_string(), me_so);
    if let (Some(c), Some(sg)) = (seller_chr.clone(), seller_sg) {
        ground_sessions.insert(c, sg);
    }
    if let (Some(c), Some(so)) = (seller_chr, seller_so) {
        sell_off_sessions.insert(c, so);
    }

    for ev in &events {
        let Some((target, body)) = game_svr::selloff::selloff_event_body(ev, catalog) else {
            continue;
        };
        let Some(t) = online.iter().find(|e| e.actor_id == target) else {
            glog!(
                "DBG",
                "元宝寄售回帧：目标 ActorId={target} 不在在线登记表 ⇒ 丢弃"
            );
            continue;
        };
        if let Some(tx) = route_to_gate(gates, t.gate_idx, t.socket, "元宝寄售回帧") {
            let _ = tx.send(SimToGate::SendFrame {
                socket: t.socket,
                body,
            });
        }
    }
}

/// `CM_MERCHANTDLGSELECT`/`CM_MERCHANTQUERYSELLPRICE`/`CM_USERSELLITEM`/`CM_USERBUYITEM`/
/// `CM_USERGETDETAILITEM`/`CM_USERREPAIRITEM`/`CM_MERCHANTQUERYREPAIRCOST` 的派发（⑤-4 第①/②片）。
///
/// **`Dealing` 前置门**（⑦ 落地后的收口）：`CM_USERBUYITEM`/`CM_USERGETDETAILITEM` 在 oracle 里
/// 共用同一个 `ClientUserBuyItem`（`PlayObject.Operate.cs:192-213`），其**第一段**就是
/// `if (Dealing) { return; }`（`:196-199`），先于 `FindMerchant`（`:200`）与任何帧/副作用；
/// 其余五条商店命令（`:114`/`:162`/…）**无**此门 ⇒ 只这两条（`trade_sessions` 传入真 `Dealing`）。
///
/// oracle 收包 → 回帧映射在 `PlayObject.Message.cs:888-942`，判定/状态迁移在
/// `PlayObject.Operate.cs:114/131/162/192/1558/1585` + `Merchant.cs`（纯函数在
/// `game_svr::shop`）。本函数只做：找会话 → `FindMerchant(Recog)` + 同图/视距门 → 调纯函数
/// → 按 `[GateIdx,Socket]` 回帧。
///
/// **字段口径**（`HUtil32.MakeLong((ushort)nParam2, (ushort)nParam3)`）：
/// `Recog` = `nParam1`（商人 `ActorId`）、`MakeIndex = param | (tag << 16)`、`Msg` = tail。
/// **距离 quirk 照搬**：`ClientUserBuyItem` 用 `|dx|>15 || |dy|>15` 拒（=`<=15` 放行），
/// 其余四条用 `IsWithinSight`（`<15`）—— `Operate.cs:200` vs `:1715`。
#[allow(clippy::too_many_arguments)]
fn handle_shop_cmd(
    ident: u16,
    recog: i32,
    param: u16,
    tag: u16,
    tail: Option<&str>,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    shop_sessions: &mut std::collections::HashMap<String, ShopSession>,
    sell_off_sessions: &mut std::collections::HashMap<String, game_svr::selloff::SellOffState>,
    sell_off_book: &mut game_svr::selloff::SellOffBook,
    trade_sessions: &std::collections::HashMap<String, game_svr::trade::DealState>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    world: &mut mir2_world::World,
    catalog: &dyn game_svr::ability::ItemCatalog,
    shop_cfg: &ShopCfg,
    colors: &MsgColors,
) {
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "商店命令：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let gate_idx = entry.gate_idx;
    let Some(sg) = sessions.get_mut(chr_name) else {
        glog!(
            "WRN",
            "商店命令：ChrName={chr_name} 无会话态（未进世界/已断线）⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let actor_id = sg.actor_id;
    // `ClientUserBuyItem`（`PlayObject.Operate.cs:196-199`）：买入/详情两条命令的
    // `if (Dealing) { return; }` **前置门**——oracle 在 `FindMerchant`（`:200`）之前、
    // 任何帧/副作用之前；其余五条商店命令 oracle 无此门（`:114`/`:162` 无）⇒ 只这两条。
    // ⑦-3/⑦-4 落地后 `Dealing` 不再恒 `false`（`trade_sessions[chr].dealing`），本门因此可达。
    if matches!(
        ident,
        game_svr::shop::CM_USERBUYITEM | game_svr::shop::CM_USERGETDETAILITEM
    ) && trade_sessions.get(chr_name).is_some_and(|s| s.dealing)
    {
        glog!(
            "DBG",
            "商店买入/详情：Dealing ⇒ return（oracle Operate.cs:196-199）ChrName={chr_name} ident={ident}"
        );
        return;
    }
    // 玩家当前位置/生死（`FindMerchant` 门与 `IsWithinSight` 用）。
    let Some(pe) = world.entities.get(actor_id) else {
        glog!(
            "WRN",
            "商店命令：actor_id={actor_id} 不在世界 ⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let (px, py, pmap, dead, ghost) = (pe.x, pe.y, pe.map_id, pe.death, pe.ghost);
    let merchant_id = recog;
    let Some(me) = world
        .entities
        .get(merchant_id)
        .filter(|e| e.merchant.is_some())
    else {
        glog!(
            "WRN",
            "商店命令：FindMerchant({merchant_id}) == null ⇒ 丢弃 ident={ident} ChrName={chr_name}"
        );
        return;
    };
    let (mx, my, mmap, mflags, mprice, mcastle, mitem_type, mhide, mchr_name) = {
        let st = me.merchant.as_ref().expect("已过滤 merchant.is_some()");
        (
            me.x,
            me.y,
            me.map_id,
            st.flags,
            st.price_rate,
            st.castle_merchant,
            st.item_type_list.clone(),
            st.is_hide,
            me.name.clone(),
        )
    };
    let same_map = mmap == pmap;
    let within_sight = (mx - px).abs() < 15 && (my - py).abs() < 15;
    let within_15 = (mx - px).abs() <= 15 && (my - py).abs() <= 15;
    let label_full = tail.unwrap_or("").trim();
    // `HUtil32.GetValidStr3(sData, ref sLabel, '\r')`：`dest` = 首段（`Merchant.cs:797`）。
    let label = label_full.split('\r').next().unwrap_or("");
    let make_index = i32::from(param) | (i32::from(tag) << 16);
    let item_name = tail.unwrap_or("");
    let view = game_svr::shop::MerchantView {
        actor_id: merchant_id,
        flags: mflags,
        price_rate: mprice,
        castle_merchant: mcastle,
        item_type_list: mitem_type,
        chr_name: mchr_name.clone(),
    };
    let mut out: Vec<game_svr::shop::ShopFrame> = Vec::new();
    match ident {
        game_svr::shop::CM_MERCHANTDLGSELECT => {
            // `ClientMerchantDlgSelect`（`Operate.cs:114-128`）。
            if dead || ghost {
                glog!(
                    "DBG",
                    "CM_MERCHANTDLGSELECT：Death|Ghost ⇒ return ChrName={chr_name}"
                );
                return;
            }
            if !((same_map && within_sight) || mhide) {
                glog!(
                    "DBG",
                    "CM_MERCHANTDLGSELECT：异图/超视距且非 IsHide ⇒ 不开面板 ChrName={chr_name}"
                );
                return;
            }
            if !label_full.starts_with('@') {
                return;
            }
            // `Merchant.UserSelect`（`Merchant.cs:799`）：`ScriptLable = sData`。
            let session = shop_sessions.entry(chr_name.to_string()).or_default();
            session.script_lable = label_full.to_string();
            let ctx = shop_context(
                catalog,
                mprice,
                mcastle,
                shop_cfg,
                &session.script_lable,
                actor_id,
                colors,
            );
            let goods = world.merchant_goods_groups(merchant_id).unwrap_or(&[]);
            out = game_svr::shop::user_select(label, &view, goods, &sg.storage.items, &ctx);
            // `Merchant.UserSelect`（`Merchant.cs:930-934`）的 `@dealybme` 支（⑩-1 接线）：
            // `if (sLabel == sDealYBme) { if (IsYbDeal) { UserSelectOpenDealOffForm(playObject); } }`
            // ⇒ `SM_SENDDEALOFFFORM(23001)`（开通 + 无进行中）或两条 `SM_MERCHANTSAY(643)` 失败帧。
            // `Rust user_select` 不产出该 label 的帧（需世界侧 `SellOffBook`/`SellOffState`）⇒ 宿主在此派发。
            if mflags.is_yb_deal && label.eq_ignore_ascii_case(game_svr::selloff::LABEL_DEAL_YB_ME)
            {
                let mut me_so = sell_off_sessions.remove(chr_name).unwrap_or_default();
                let events = {
                    let weight = sg.state.wabil.weight;
                    let wear_weight = sg.state.wabil.wear_weight;
                    let hand_weight = sg.state.wabil.hand_weight;
                    let max_weight = sg.state.wabil.max_weight;
                    let game_point = sg.game_point;
                    let send_gold = sg.soft_version_date != 0;
                    let me = game_svr::selloff::SellOffSide {
                        id: actor_id,
                        name: chr_name.to_string(),
                        // 在线会话（`PlayObject.OffLineFlag` 未建模 ⇒ 恒 false，登记）。
                        offline: false,
                        items: &mut sg.state.item_list,
                        gold: &mut sg.state.gold,
                        game_gold: &mut sg.game_gold,
                        game_point,
                        weight,
                        wear_weight,
                        hand_weight,
                        max_weight,
                        send_game_gold_info: send_gold,
                        state: &mut me_so,
                    };
                    game_svr::selloff::open_deal_off_form(
                        &me,
                        sell_off_book,
                        merchant_id,
                        &mchr_name,
                    )
                };
                sell_off_sessions.insert(chr_name.to_string(), me_so);
                for ev in &events {
                    let Some((target, body)) = game_svr::selloff::selloff_event_body(ev, catalog)
                    else {
                        continue;
                    };
                    let Some(t) = online.iter().find(|e| e.actor_id == target) else {
                        glog!(
                            "DBG",
                            "元宝寄售开窗回帧：目标 ActorId={target} 不在在线登记表 ⇒ 丢弃"
                        );
                        continue;
                    };
                    if let Some(tx) = route_to_gate(gates, t.gate_idx, t.socket, "元宝寄售开窗回帧")
                    {
                        let _ = tx.send(SimToGate::SendFrame {
                            socket: t.socket,
                            body,
                        });
                    }
                }
                return;
            }
        }
        game_svr::shop::CM_MERCHANTQUERYSELLPRICE => {
            // `ClientMerchantQuerySellPrice`（`Operate.cs:131-153`）。
            let Some(pos) =
                find_bag_item_by_make_index(&sg.state.item_list, catalog, make_index, item_name)
            else {
                return;
            };
            if !(mflags.is_sell && same_map && within_sight) {
                return;
            }
            let sl = shop_script_lable(shop_sessions, chr_name);
            let ctx = shop_context(catalog, mprice, mcastle, shop_cfg, &sl, actor_id, colors);
            out.push(game_svr::shop::query_sell_price(
                &view,
                &mut sg.state.item_list[pos],
                &ctx,
            ));
        }
        game_svr::shop::CM_USERSELLITEM => {
            // `ClientUserSellItem`（`Operate.cs:162-190`）。
            if !(mflags.is_sell && same_map && within_sight) {
                return;
            }
            let sl = shop_script_lable(shop_sessions, chr_name);
            let ctx = shop_context(catalog, mprice, mcastle, shop_cfg, &sl, actor_id, colors);
            let Some(goods) = world.merchant_goods_groups_mut(merchant_id) else {
                return;
            };
            let r =
                game_svr::shop::sell_item(&view, &mut sg.state, goods, make_index, item_name, &ctx);
            out = r.frames;
        }
        game_svr::shop::CM_USERBUYITEM => {
            // `ClientUserBuyItem`（`Operate.cs:192-220`）+ `Merchant.ClientBuyItem`（`Merchant.cs:1219`）。
            if !(mflags.is_buy && same_map && within_15) {
                return;
            }
            let sl = shop_script_lable(shop_sessions, chr_name);
            let ctx = shop_context(catalog, mprice, mcastle, shop_cfg, &sl, actor_id, colors);
            let Some(goods) = world.merchant_goods_groups_mut(merchant_id) else {
                return;
            };
            let r =
                game_svr::shop::buy_item(&view, &mut sg.state, goods, item_name, make_index, &ctx);
            out = r.frames;
        }
        game_svr::shop::CM_USERGETDETAILITEM => {
            // `ClientUserBuyItem`（`Operate.cs:192-213`）的 `CM_USERGETDETAILITEM` 支：与买入**同一门**
            // （`IsBuy && 同图 && |dx|,|dy| <= 15`）；`nInt` = 收包 `Param`（菜单顶行）。
            if !(mflags.is_buy && same_map && within_15) {
                return;
            }
            let sl = shop_script_lable(shop_sessions, chr_name);
            let ctx = shop_context(catalog, mprice, mcastle, shop_cfg, &sl, actor_id, colors);
            let Some(goods) = world.merchant_goods_groups_mut(merchant_id) else {
                return;
            };
            out.push(game_svr::shop::detail_goods_list(
                &view,
                goods,
                item_name,
                i32::from(param),
                &ctx,
            ));
        }
        game_svr::shop::CM_MERCHANTQUERYREPAIRCOST => {
            // `ClientQueryRepairCost`（`Operate.cs:1558-1583`）。
            let Some(pos) =
                find_bag_item_by_make_index(&sg.state.item_list, catalog, make_index, item_name)
            else {
                return;
            };
            if !(same_map && within_sight) {
                return;
            }
            let sl = shop_script_lable(shop_sessions, chr_name);
            let ctx = shop_context(catalog, mprice, mcastle, shop_cfg, &sl, actor_id, colors);
            out.push(game_svr::shop::query_repair_cost(
                &view,
                &mut sg.state.item_list[pos],
                &ctx,
            ));
        }
        game_svr::shop::CM_USERREPAIRITEM => {
            // `ClientRepairItem`（`Operate.cs:1585-1605`）。
            // 按 §1 硬约束③ **BUG-REPLICA**：oracle 的循环 `userItem = ItemList[i]` **无条件**赋值，
            // 未命中时循环结束后 `userItem` 仍是**最后一件**背包物品（非 null）⇒ oracle 会继续修
            // 最后一件；只有 `ItemList` 为空才 return（`Operate.cs:1593-1597`）。
            // （同源的 `ClientQueryRepairCost`（`:1558-1583`）用 `userItemA = null` 初值 ⇒ 未命中 return；
            //  **只有 repair 有这条 bug**。）
            let Some(pos) =
                repair_target_index(&sg.state.item_list, catalog, make_index, item_name)
            else {
                return;
            };
            if !(same_map && within_sight) {
                return;
            }
            let sl = shop_script_lable(shop_sessions, chr_name);
            let ctx = shop_context(catalog, mprice, mcastle, shop_cfg, &sl, actor_id, colors);
            let r = game_svr::shop::repair_item(&view, &mut sg.state, pos, &ctx);
            if let Some(f) = r.sent_to_user {
                out.push(f);
            }
            out.extend(r.frames);
            if let Some(g) = r.goto_label {
                // `Merchant.cs:1652/1659` 的 `GotoLable`：本宿主尚未装载 `Market_Def`
                // 商人脚本（`Merchant.LoadScriptFile`，`Merchant.cs:1089`）⇒ 只登记、不执行。
                glog!(
                    "WRN",
                    "CM_USERREPAIRITEM：GotoLable({g}) 未执行（Market_Def 商人脚本未装载，登记；ChrName={chr_name}）"
                );
            }
        }
        _ => {}
    }
    if out.is_empty() {
        return;
    }
    let Some(tx) = route_to_gate(gates, gate_idx, socket, "商店回帧") else {
        return;
    };
    emit_shop_frames(tx, socket, &out);
}

/// `CM_USERSTORAGEITEM(1031)` / `CM_USERTAKEBACKSTORAGEITEM(1032)` 的派发（⑤-4 第②片）。
///
/// oracle 链：`PlayObject.Message.cs:963-968` → `Operate.cs:1607`（`ClientStorageItem`）/
/// `:1656`（`ClientTakeBackStorageItem`）。与商店命令不同，这两条**不按商人 flag 提前丢包**：
/// - `ClientStorageItem` 在 `FindMerchant` 为 null 或无 `IsStorage` 时仍回 `SM_STORAGE_FAIL`
///   （门在循环体内，落到 `if (!bo19)`）；
/// - `ClientTakeBackStorageItem` 在 `FindMerchant == null` 或 `!IsCanGetBackItem` 时 **无任何帧**（直接 return）。
#[allow(clippy::too_many_arguments)]
fn handle_storage_cmd(
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
    world: &mut mir2_world::World,
    catalog: &dyn game_svr::ability::ItemCatalog,
    shop_cfg: &ShopCfg,
    colors: &MsgColors,
) {
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "仓库命令：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let gate_idx = entry.gate_idx;
    let Some(sg) = sessions.get_mut(chr_name) else {
        glog!(
            "WRN",
            "仓库命令：ChrName={chr_name} 无会话态（未进世界/已断线）⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let actor_id = sg.actor_id;
    let Some(pe) = world.entities.get(actor_id) else {
        glog!(
            "WRN",
            "仓库命令：actor_id={actor_id} 不在世界 ⇒ 丢弃 ident={ident}"
        );
        return;
    };
    let (px, py, pmap) = (pe.x, pe.y, pe.map_id);
    let merchant_id = recog;
    let merchant = world
        .entities
        .get(merchant_id)
        .filter(|e| e.merchant.is_some());
    let make_index = i32::from(param) | (i32::from(tag) << 16);
    let raw_name = tail.unwrap_or("");
    // 仓库命令不读 `ScriptLable`（仅取回负重不足时用红字系统消息，颜色随 `[Setup]`）。
    let ctx = shop_context(catalog, 0, false, shop_cfg, "", actor_id, colors);
    let mut out: Vec<game_svr::shop::ShopFrame> = Vec::new();
    match ident {
        game_svr::shop::CM_USERSTORAGEITEM => {
            // `merchant.IsStorage && (同图且 IsWithinSight || FunctionNPC)`；FunctionNPC 未建模。
            let allows = merchant.is_some_and(|me| {
                let st = me.merchant.as_ref().expect("已过滤 merchant.is_some()");
                let same_map = me.map_id == pmap;
                let within = (me.x - px).abs() < 15 && (me.y - py).abs() < 15;
                st.flags.is_storage && same_map && within
            });
            let can_store = sg.storage.can_store();
            let r = game_svr::shop::storage_item(
                &mut sg.state,
                &mut sg.storage.items,
                can_store,
                allows,
                make_index,
                raw_name,
                &ctx,
            );
            out = r.frames;
        }
        game_svr::shop::CM_USERTAKEBACKSTORAGEITEM => {
            // `FindMerchant == null` ⇒ return（无帧）。
            let Some(me) = merchant else {
                return;
            };
            // `!IsCanGetBackItem` ⇒ return（无帧）。
            if !sg.storage.is_can_get_back_item {
                return;
            }
            let st = me.merchant.as_ref().expect("merchant.is_some()");
            let same_map = me.map_id == pmap;
            let within = (me.x - px).abs() < 15 && (me.y - py).abs() < 15;
            let allows = st.flags.is_getback && same_map && within;
            let r = game_svr::shop::take_back_storage_item(
                &mut sg.state,
                &mut sg.storage.items,
                allows,
                make_index,
                raw_name,
                &ctx,
            );
            out = r.frames;
        }
        _ => {}
    }
    if out.is_empty() {
        return;
    }
    let Some(tx) = route_to_gate(gates, gate_idx, socket, "仓库回帧") else {
        return;
    };
    emit_shop_frames(tx, socket, &out);
}

/// **M3⑥-实现 3 第②片**：`CharacterData` → [`game_svr::npc::NpcPlayer`] 快照。
///
/// oracle 的脚本执行者就是 `PlayObject` 本身（`playerActor`）——进世界后它持有 `Gold`/
/// `ItemList`/`QuestFlag` 等字段；本函数只把**同源**字段映射过去（不猜、不造）。`bag` 按
/// `ItemList` 的每个非空槽建一条（名称查表口径同 `bins/game-svr/src/items.rs` 的 1-based
/// `GetStdItem`）。
#[allow(clippy::too_many_arguments)]
fn npc_player_from<'r>(
    rng: &'r mut dyn FnMut(i32) -> i32,
    chr_name: &str,
    actor_id: i32,
    map_name: &str,
    level: i32,
    gold: i32,
    bag: Option<&[Option<record::ServerUserItem>]>,
    quest_flag: Option<&[u8]>,
    catalog: &dyn game_svr::ability::ItemCatalog,
    group_owner: i32,
    group_member_jobs: &[i32],
) -> game_svr::npc::NpcPlayer<'r> {
    let mut p = game_svr::npc::NpcPlayer::empty(rng);
    p.chr_name = chr_name.to_string();
    p.actor_id = actor_id;
    p.map_name = map_name.to_string();
    p.level = level;
    p.gold = gold;
    // §1.O `recall-ext` 读侧：`GroupOwner` / `GroupMembers[i].Job`（同源自 `World`）。
    p.group_owner = group_owner;
    p.group_member_jobs = group_member_jobs.to_vec();
    if let Some(cells) = bag {
        for cell in cells.iter().flatten() {
            if let Some(name) = catalog.name(cell.index) {
                p.items.push((name.to_string(), 1));
            }
        }
    }
    if let Some(qf) = quest_flag {
        // oracle 登录：`playObject.QuestFlag = humData.QuestFlag`（`WorldServer.cs:1160`）——
        // 记录里的 128 字节**原位**载入（位语义在 `QuestBitmap` 内，不在载入时拆位）。
        let mut arr = [0u8; mir2_script::engine::quest_state::QUEST_ARRAY_LEN];
        let n = qf.len().min(arr.len());
        arr[..n].copy_from_slice(&qf[..n]);
        p.quest_flag = mir2_script::engine::quest_state::QuestBitmap::from_bytes(arr);
    }
    p
}

/// **M3⑥-实现 3 第②片**：把 NPC 说词按 oracle `MsgQueue` **出队序**落 `SM_MERCHANTSAY(643)`。
///
/// `recog` 两条入队路径取值不同（`ScriptEngine.cs:993/997` 与 `BaseObject.Message.cs:30/180`）：
/// `SendPriorityMsg(High)` 用**玩家** ActorId，`SendMsg(normNpc)` 用**NPC** ActorId（隐藏 NPC 恒 0）。
/// 返回实际下发帧数。
fn emit_npc_says(
    tx: &mpsc::UnboundedSender<SimToGate>,
    socket: i32,
    player_actor_id: i32,
    says: &[game_svr::npc::SayMsg],
) -> usize {
    let mut sent = 0usize;
    for s in game_svr::npc::dequeue_order(says) {
        let recog = if s.priority { player_actor_id } else { 0 };
        let wire = game_svr::npc::merchant_say_gbk(&s.npc_name, &s.raw_msg);
        let body = smframe::sm_merchantsay(recog, 0, 0, &wire);
        let _ = tx.send(SimToGate::SendFrame { socket, body });
        sent += 1;
    }
    sent
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
    // `PlayObject.BodyLuck`（`PlayObject.cs:520`）⇒ `characters.BodyLuck`（`MakeSaveRcd` 读 `BodyLuck`）。
    rec.data.body_luck = sg.equip.body_luck;
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
    // `MakeSaveRcd` 的 `StorageItems`（`WorldServer.cs:1359-1376`）：会话仓库 → 新建 50 槽数组
    // （尾槽填 `DelfautItem`），DBSvr 侧写 `characters_storageitem`（`PlayDataStorage.Save.cs:360`）。
    rec.data.storage_items = Some(storage::storage_items_for_save(&sg.storage.items));
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

/// `RecalcLevelAbilitys()` + `RecalcAbilitys()` 的**单一实现**（登录/升级/装备/卸下四处共用）：
/// 从 `sg.use_items`/`sg.state.item_list`/`sg.equip` 组装 [`game_svr::ability::RecalcInputs`]，
/// 调 [`game_svr::ability::recalc_abilitys_with_prior`]（`prior = sg.sub`，T-18 的显式状态），
/// 写回 `sg.state.wabil` 与 `sg.sub`。
///
/// 调用点：① 装备/卸下成功分支；② 升级（`PlayObject.cs:5245-5246` 的 `HasLevelUp`——oracle 先
/// `RecalcLevelAbilitys()` 再 `RecalcAbilitys()`，本函数内部 `level_ability_base` 已按该序覆盖
/// `MaxHP/MaxMP/MaxWeight/DC/MC/SC/AC/MAC` 等等级派生字段）。
fn recalc_session_ability(sg: &mut SessionGround, catalog: &dyn game_svr::ability::ItemCatalog) {
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

/// **`openmir2-m3-attack-magic-wiring`（第二片）** + **`openmir2-m3-pet-slave-summon`（第②片）**：
/// `CM_SPELL(3017)` 派发（oracle `WorldServer.cs:783-790` → `PlayObject.Message.cs:1339-1400`）。
///
/// 守卫 + 成功后 `+GD` ack 沿用第二片；**第②片**接上 `DoSpell` 的**召唤面**（17/30/41）
/// 效果体：`default` 支的 `CretInNearXy` 目标修正 → `MagicAttackRage` 距离门 →
/// `SendRefMsg(RM_SPELL,…)` → 护身符门/消耗 → `summon_from_spell` → `boSpellFire` 的
/// `RM_MAGICFIRE` → `boTrain` 熟练度尾 `RM_MAGIC_LVEXP`。**第③片**接 **20（诱惑）**：
/// `SKILL_TAMMING` 支（`MagicManager.cs:394-397`）的 `IsProperTarget && !Animal` 门 →
/// `cast_tamming_spell`（`MagTamming` 账本 + `World::tame_adopt` 落尾）。46（分身）登记见
/// `game_svr::slave` 模块头；46 分身已接（[`game_svr::slave::cast_clone_spell`] + 下方 46 支）。
/// 效果体限速 / `SpellTick -= 450` / MP 消耗仍按 `magicwire` 模块头登记未接。
#[allow(clippy::too_many_arguments)]
fn handle_spell_cmd(
    recog: i32,
    tag: u16,
    param: u16,
    series: u16,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    world: &mut mir2_world::World,
    saves: &mut std::collections::HashMap<String, SavedHuman>,
    ground_sessions: &mut std::collections::HashMap<String, SessionGround>,
    catalog: &dyn game_svr::ability::ItemCatalog,
    magic_table: &magics::MagicTable,
    pet_cfg: &mir2_data::pet_slave::PetSlaveConfig,
    magic_attack_rage: i32,
    registry: Option<&mir2_envir::MapRegistry>,
    table: &game_svr::mongen::MonsterTable,
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
    let caster_abil_level = e.level;
    let req = magicwire::parse_spell(recog, tag, param, series);
    // `GetMagicInfo(nKey)` 读角色记录的 `MagicList`（= `SavedHuman.record.data.magic`，进世界时
    // 保位存下的 `CharacterData.magic`）。
    let matched = saves
        .get(chr_name)
        .and_then(|s| s.record.data.magic.as_deref())
        .and_then(|list| magicwire::find_magic(list, req.magic_id))
        .cloned();
    let has_magic = matched.is_some();
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
    // **F-2 收口**（reviewer-5 `1de8fca5`）：该句只在 `default` 支执行，
    // `switch` 的 8 条独立 `case`（12/25/26/27/34/38/43/56）**不执行** ⇒ 只有非这 8 个魔号才改展示方向。
    if matched
        .as_ref()
        .is_some_and(|m| magicwire::takes_default_dir(i32::from(m.mag_idx)))
    {
        let dir = movewire::next_direction(cur_x, cur_y, req.target_x, req.target_y);
        set_display_dir(displays, actor_id, dir);
    }
    // **第②片**：召唤面（17/30/41）的 `default` 支效果体（`PlayObject.Attack.cs:1271-1279` + `MagicManager.cs:145-670`）。
    if matches!(
        req.magic_id,
        mir2_world::slave::SKILL_SKELLETON
            | mir2_world::slave::SKILL_SINSU
            | mir2_world::slave::SKILL_ANGEL
    ) {
        if let (Some(magic), Some(reg)) = (magic_table.get(req.magic_id), registry) {
            // `CretInNearXy(targetObject, nTargetX, nTargetY)`（`PlayObject.Base.cs:984-1011`）：
            // 目标在 `(nTargetX,nTargetY)` 的 1×1 邻域内 ⇒ `spellObject = targetObject` 且
            // snap 坐标到目标实际坐标。
            let (mut tx, mut ty) = (req.target_x, req.target_y);
            let target_id = match world.entities.get(req.target_actor_id) {
                Some(t) if !t.ghost && (t.x - tx).abs() <= 1 && (t.y - ty).abs() <= 1 => {
                    tx = t.x;
                    ty = t.y;
                    Some(req.target_actor_id)
                }
                _ => None,
            };
            // `DoSpell` 距离门（`MagicManager.cs:155`，`SystemShare.Config.MagicAttackRage`）。
            if magic::spell_in_range(
                i16::try_from(cur_x).unwrap_or(0),
                i16::try_from(cur_y).unwrap_or(0),
                i16::try_from(tx).unwrap_or(0),
                i16::try_from(ty).unwrap_or(0),
                magic_attack_rage,
            ) {
                // `SendRefMsg(RM_SPELL, Magic.Effect, nTargetX, nTargetY, Magic.MagicId, "")`（`:159`）。
                let rm = smframe::sm_spell(
                    actor_id,
                    wire_coord(tx),
                    wire_coord(ty),
                    magic.effect,
                    req.magic_id,
                );
                broadcast_ref_msg(world, online, gates, actor_id, &[rm]);
                // 护身符门（`CheckAmulet`）用的本会话 `UseItems[ArmRingl/Bujuk]`。
                let armringl = amulet_slot(
                    ground_sessions.get(chr_name),
                    game_svr::ability::SLOT_ARMRING_L,
                    catalog,
                );
                let bujuk = amulet_slot(
                    ground_sessions.get(chr_name),
                    game_svr::ability::SLOT_BUJUK,
                    catalog,
                );
                let caster_dir = displays.get(actor_id).map_or(0, |d| d.dir);
                // `UserMagic` 的内存态（Level/TranPoint 在尾里被 `TrainSkill`/`CheckMagicLevelUp` 改）。
                let mut train = magic::SkillTrainState {
                    magic_id: req.magic_id,
                    level: matched.as_ref().map_or(0, |m| m.level),
                    tran_point: matched.as_ref().map_or(0, |m| m.tran_point),
                    train_lv: magic.train_lv,
                    train_level: magic.train_level,
                    max_train: magic.max_train,
                };
                let magic_level = train.level;
                let input = game_svr::slave::SlaveCastInput {
                    magic_id: req.magic_id,
                    magic_level,
                    caster_id: actor_id,
                    caster_dir,
                    caster_abil_level,
                    armringl,
                    bujuk,
                    // `FastTrain` 由「快速修炼项链」置位（`PlayObject.cs:2168`），本仓未装载 ⇒ false（登记）。
                    fast_train: false,
                    cfg: pet_cfg,
                    magic,
                };
                let outcome = game_svr::slave::cast_slave_spell(
                    world, &input, &mut train, reg, table, displays,
                );
                // 熟练度回写存档记录（`MakeSaveRcd` 的 `MagicList` 同源）。
                if let Some(rcd) = saves
                    .get_mut(chr_name)
                    .and_then(|s| s.record.data.magic.as_mut())
                    .and_then(|list| {
                        list.iter_mut()
                            .flatten()
                            .find(|m| m.mag_idx == req.magic_id)
                    })
                {
                    rcd.level = train.level;
                    rcd.tran_point = train.tran_point;
                }
                // 护身符消耗（`MagicBase.UseAmulet`，`MagicBase.cs:109-127`）：
                // - `Dura > nCount*100` ⇒ `Dura -= …` + `SendMsg(RM_DURACHANGE, idx, Dura, DuraMax, 0)`
                //   （自身队列 ⇒ `SM_DURACHANGE`）；
                // - else ⇒ `Dura = 0; SendDelItems(item); Index = 0;` ⇒ **立即** `SM_DELITEM(202)`
                //   （`PlayObject.Base.cs:1648-1665`），且帧内 `Dura` 恒 0（oracle 先置 0 再发）。
                if let Some((slot, used)) = outcome.amulet {
                    if used.deleted {
                        // 清槽前取件（`apply_amulet_use` 会把 `UseItems[slot]` 置 None）。
                        let body = ground_sessions
                            .get(chr_name)
                            .and_then(|sg| sg.use_items.get(usize::from(slot)))
                            .and_then(Option::as_ref)
                            .and_then(|it| {
                                game_svr::slave::deleted_amulet_body(actor_id, it, catalog)
                            });
                        apply_amulet_use(ground_sessions.get_mut(chr_name), slot, used);
                        if let Some(body) = body {
                            send_self_frame(online, gates, chr_name, body);
                        }
                    } else {
                        apply_amulet_use(ground_sessions.get_mut(chr_name), slot, used);
                        if let Some(it) = ground_sessions
                            .get(chr_name)
                            .and_then(|sg| sg.use_items.get(usize::from(slot)))
                            .and_then(Option::as_ref)
                        {
                            let body = smframe::sm_durachange(it.dura, slot, it.dura_max);
                            send_self_frame(online, gates, chr_name, body);
                        }
                    }
                }
                // `boSpellFire` ⇒ `SendRefMsg(RM_MAGICFIRE, 0, MakeWord(EffectType,Effect), MakeLong(tx,ty), targetActorId, "")`（`:648-658`）。
                // **F-1**：`RM_MAGICFIRE` 处理器无 `ActorId != ActorId` 守卫（`PlayObject.Message.cs:1788-1793`）
                // ⇒ 广播集合**含施法者自己**（`RM_SPELL` 仍排除自己）。
                if outcome.magic_fire {
                    let body = smframe::sm_magicfire(
                        actor_id,
                        wire_coord(tx),
                        wire_coord(ty),
                        magic.effect_type,
                        magic.effect,
                        target_id.unwrap_or(0),
                    );
                    broadcast_ref_msg_with_self(world, online, gates, actor_id, &[body]);
                }
                // 熟练度尾帧（`:659-669`）：`SendSelfDelayMsg(RM_MAGIC_LVEXP, 0, MagicId, Level, TranPoint, "", 1000)`。
                if outcome.train.levelup_msg {
                    let body = smframe::sm_magic_lvexp(req.magic_id, train.level, train.tran_point);
                    send_self_frame(online, gates, chr_name, body);
                }
                let _ = outcome.accepted; // oracle `ClientSpellXY` 恒 `result = true`（失败只多一帧 RM_MAGICFIREFAIL，未接）。
            }
        } else {
            glog!(
                "DBG",
                "CM_SPELL：召唤魔号 {} 无 MagicInfo/无地图注册表 ⇒ 效果体不接（登记）",
                req.magic_id
            );
        }
    }
    // **⑨-2 第③片（20 诱惑）**：`SKILL_TAMMING` 效果体
    // （`PlayObject.Attack.cs:1268-1278` → `MagicManager.cs:394-397` → `MagTamming` `:704-825`）。
    if req.magic_id == mir2_world::slave::SKILL_TAMMING {
        if let Some(magic) = magic_table.get(req.magic_id) {
            // `CretInNearXy(targetObject, nTargetX, nTargetY)`：与召唤面同口径，
            // 目标在 1×1 邻域内 ⇒ 修正到目标实际坐标并作为 `spellObject`。
            let (mut tx, mut ty) = (req.target_x, req.target_y);
            let target_id = match world.entities.get(req.target_actor_id) {
                Some(t) if !t.ghost && (t.x - tx).abs() <= 1 && (t.y - ty).abs() <= 1 => {
                    tx = t.x;
                    ty = t.y;
                    Some(req.target_actor_id)
                }
                _ => None,
            };
            // `DoSpell` 的距离门（`MagicManager.cs:154`）。
            if magic::spell_in_range(
                i16::try_from(cur_x).unwrap_or(0),
                i16::try_from(cur_y).unwrap_or(0),
                i16::try_from(tx).unwrap_or(0),
                i16::try_from(ty).unwrap_or(0),
                magic_attack_rage,
            ) {
                // `DoSpell` 顶部的 `SendRefMsg(RM_SPELL, Effect, nTargetX, nTargetY, MagicId, "")`（`:159`）。
                let rm = smframe::sm_spell(
                    actor_id,
                    wire_coord(tx),
                    wire_coord(ty),
                    magic.effect,
                    req.magic_id,
                );
                broadcast_ref_msg(world, online, gates, actor_id, &[rm]);
                // `UserMagic` 的内存态（尾里用；与召唤面同一口径）。
                let mut train = magic::SkillTrainState {
                    magic_id: req.magic_id,
                    level: matched.as_ref().map_or(0, |m| m.level),
                    tran_point: matched.as_ref().map_or(0, |m| m.tran_point),
                    train_lv: magic.train_lv,
                    train_level: magic.train_level,
                    max_train: magic.max_train,
                };
                let input = game_svr::slave::TammingCastInput {
                    magic_level: train.level,
                    caster_id: actor_id,
                    caster_abil_level,
                    target_id,
                    // `FastTrain` 未装载 ⇒ false（登记，同召唤面）。
                    fast_train: false,
                    cfg: pet_cfg,
                    magic,
                };
                let outcome = game_svr::slave::cast_tamming_spell(world, &input, &mut train);
                // 熟练度回写存档记录（`MakeSaveRcd` 的 `MagicList` 同源）。
                if let Some(rcd) = saves
                    .get_mut(chr_name)
                    .and_then(|s| s.record.data.magic.as_mut())
                    .and_then(|list| {
                        list.iter_mut()
                            .flatten()
                            .find(|m| m.mag_idx == req.magic_id)
                    })
                {
                    rcd.level = train.level;
                    rcd.tran_point = train.tran_point;
                }
                // `boSpellFire` ⇒ `RM_MAGICFIRE`（`:648-658`）；**F-1**：含施法者自己（同召唤面）。
                if outcome.magic_fire {
                    let body = smframe::sm_magicfire(
                        actor_id,
                        wire_coord(tx),
                        wire_coord(ty),
                        magic.effect_type,
                        magic.effect,
                        target_id.unwrap_or(0),
                    );
                    broadcast_ref_msg_with_self(world, online, gates, actor_id, &[body]);
                }
                // 熟练度尾帧（`:659-669`）。
                if outcome.train.levelup_msg {
                    let body = smframe::sm_magic_lvexp(req.magic_id, train.level, train.tran_point);
                    send_self_frame(online, gates, chr_name, body);
                }
                // 帧面未接（登记）：`RM_MAKEHOLYSEIZEMODE`（`:712/797`）无编码器；
                // 狂暴态/时长（`OpenCrazyMode`）本骨架无字段。状态落尾已由 `cast_tamming_spell` 完成。
                let _ = outcome.accepted;
            }
        } else {
            glog!(
                "DBG",
                "CM_SPELL：诱惑魔号 20 无 MagicInfo ⇒ 效果体不接（登记）"
            );
        }
    }
    // **⑨-2 残余（46 分身）**：`SKILL_46` 效果体
    // （`MagicManager.cs:589-592` → `MagMakeClone` `:1337-1341` → `PlayCloneObject.cs:5-23`）。
    // 46 走 `ClientSpellXY` 的 `default` 支（非 `EXPLICIT_CASE_MAG_IDX`）⇒ 上面的
    // `takes_default_dir` 分支已把主人展示方向转向目标（`PlayObject.Attack.cs:1269`），
    // 克隆体的 `Dir = GetBackDir(主人.Dir)` 取的是**转向后**的 `Dir`。
    if req.magic_id == mir2_world::slave::SKILL_46 {
        if let Some(magic) = magic_table.get(req.magic_id) {
            // `CretInNearXy(targetObject, nTargetX, nTargetY)`（`PlayObject.Base.cs:984-1011`）：
            // 与召唤/诱惑面同口径，目标在 1×1 邻域内 ⇒ 修正到目标实际坐标并作为 `spellObject`。
            let (mut tx, mut ty) = (req.target_x, req.target_y);
            let target_id = match world.entities.get(req.target_actor_id) {
                Some(t) if !t.ghost && (t.x - tx).abs() <= 1 && (t.y - ty).abs() <= 1 => {
                    tx = t.x;
                    ty = t.y;
                    Some(req.target_actor_id)
                }
                _ => None,
            };
            // `DoSpell` 的距离门（`MagicManager.cs:152-155`，`SystemShare.Config.MagicAttackRage`）。
            if magic::spell_in_range(
                i16::try_from(cur_x).unwrap_or(0),
                i16::try_from(cur_y).unwrap_or(0),
                i16::try_from(tx).unwrap_or(0),
                i16::try_from(ty).unwrap_or(0),
                magic_attack_rage,
            ) {
                // `DoSpell` 顶部 `SendRefMsg(RM_SPELL, Effect, nTargetX, nTargetY, MagicId, "")`（`:159`）。
                let rm = smframe::sm_spell(
                    actor_id,
                    wire_coord(tx),
                    wire_coord(ty),
                    magic.effect,
                    req.magic_id,
                );
                broadcast_ref_msg(world, online, gates, actor_id, &[rm]);
                // `UserMagic` 的内存态（尾里被 `TrainSkill`/`CheckMagicLevelUp` 改）。
                let mut train = magic::SkillTrainState {
                    magic_id: req.magic_id,
                    level: matched.as_ref().map_or(0, |m| m.level),
                    tran_point: matched.as_ref().map_or(0, |m| m.tran_point),
                    train_lv: magic.train_lv,
                    train_level: magic.train_level,
                    max_train: magic.max_train,
                };
                let caster_dir = displays.get(actor_id).map_or(0, |d| d.dir);
                let input = game_svr::slave::CloneCastInput {
                    caster_id: actor_id,
                    caster_dir,
                    caster_abil_level,
                    // `FastTrain` 未装载 ⇒ false（登记，同召唤/诱惑面）。
                    fast_train: false,
                };
                let outcome = game_svr::slave::cast_clone_spell(world, &input, &mut train);
                // 熟练度回写存档记录（`MakeSaveRcd` 的 `MagicList` 同源，与召唤/诱惑面同口径）。
                if let Some(rcd) = saves
                    .get_mut(chr_name)
                    .and_then(|s| s.record.data.magic.as_mut())
                    .and_then(|list| {
                        list.iter_mut()
                            .flatten()
                            .find(|m| m.mag_idx == req.magic_id)
                    })
                {
                    rcd.level = train.level;
                    rcd.tran_point = train.tran_point;
                }
                // `PlayCloneObject` 构造器尾 `SendRefMsg(RM_TURN, Dir, CurrX, CurrY, 0, ChrName)`
                // （`PlayCloneObject.cs:17`）：先登记展示表，再广播出现帧（`SM_TURN` 名尾 + 紧随
                // `SM_FEATURECHANGED`，`downlink::turn_pair`）。`Msg="Clone"` 非空 ⇒ 名尾
                // `Encode("Clone/{GetChrColor}")`（= `"Clone/255"`，`PlayObject.Message.cs:1549-1555`）。
                if let Some(clone) = outcome.clone {
                    if let Some(caster_display) = displays.get(actor_id).cloned() {
                        let d = downlink::clone_display(clone.dir, &caster_display);
                        displays.set(clone.clone_id, d.clone());
                        let frames = downlink::turn_pair(
                            clone.clone_id,
                            wire_coord(clone.x),
                            wire_coord(clone.y),
                            &d,
                        );
                        // 广播集合排除克隆体自己（`RM_TURN` 处理器有 `ActorId != ActorId` 守卫）。
                        broadcast_ref_msg(world, online, gates, clone.clone_id, &frames);
                    }
                }
                // `boSpellFire` ⇒ `RM_MAGICFIRE`（`MagicManager.cs:648-658`）；**F-1**：含施法者自己。
                if outcome.magic_fire {
                    let body = smframe::sm_magicfire(
                        actor_id,
                        wire_coord(tx),
                        wire_coord(ty),
                        magic.effect_type,
                        magic.effect,
                        target_id.unwrap_or(0),
                    );
                    broadcast_ref_msg_with_self(world, online, gates, actor_id, &[body]);
                }
                // 熟练度尾帧（`:659-669`）：`SendSelfDelayMsg(RM_MAGIC_LVEXP, …)`。
                if outcome.train.levelup_msg {
                    let body = smframe::sm_magic_lvexp(req.magic_id, train.level, train.tran_point);
                    send_self_frame(online, gates, chr_name, body);
                }
                let _ = outcome.accepted;
            }
        } else {
            glog!(
                "DBG",
                "CM_SPELL：分身魔号 46 无 MagicInfo ⇒ 效果体不接（登记）"
            );
        }
    }
    // `ClientSpellXY` 全分支 `result = true` ⇒ 调用方 `ActionTick = GetTickCount(); SendSocket(GetGoodTick)`。
    send_good_tick(online, gates, chr_name);
}

/// `MagicBase.CheckAmulet`（`MagicBase.cs:47-105`）的**宿主侧查表**：把 `UseItems[slot]`
/// 的 `UserItem{Index,Dura}` + `StdItem{StdMode,Shape}` 装成 `crates/world::magic::AmuletSlot`。
/// `Index == 0` / 槽空 / 模板缺 ⇒ `None`（oracle `Index > 0` + `GetStdItem != null` 的门）。
fn amulet_slot(
    sg: Option<&SessionGround>,
    slot: usize,
    catalog: &dyn game_svr::ability::ItemCatalog,
) -> Option<mir2_world::magic::AmuletSlot> {
    let it = sg?.use_items.get(slot)?.as_ref()?;
    if it.index == 0 {
        return None;
    }
    let std = game_svr::ability::ItemCatalog::get(catalog, it.index)?;
    Some(mir2_world::magic::AmuletSlot {
        index: i32::from(it.index),
        std_mode: std.std_mode,
        shape: std.shape,
        dura: i32::from(it.dura),
    })
}

/// `MagicBase.UseAmulet`（`MagicBase.cs:109-127`）的**宿主副作用**：按 [`mir2_world::magic::AmuletUse`]
/// 扣耐久或清槽（oracle `Dura -= …` / `Dura = 0; Index = 0`）。
/// 帧面（`RM_DURACHANGE` / `RM_DELITEMS`）由调用方发。
fn apply_amulet_use(sg: Option<&mut SessionGround>, slot: u8, used: mir2_world::magic::AmuletUse) {
    let Some(sg) = sg else { return };
    let idx = usize::from(slot);
    if idx >= game_svr::ability::USE_ITEMS_LEN {
        return;
    }
    if used.deleted {
        // oracle `UseItems[idx].Dura = 0; SendDelItems(item); UseItems[idx].Index = 0;`。
        sg.use_items[idx] = None;
    } else if let Some(it) = sg.use_items[idx].as_mut() {
        it.dura = u16::try_from(used.dura_after).unwrap_or(0);
    }
}

/// `SendMsg(this, …)` / `SendSelfDelayMsg(…)` 的**自身收件人**下行（oracle 入自有 `MsgQueue`，
/// 只回发起者那条连接）：`SM_DURACHANGE` / `SM_MAGIC_LVEXP`。
fn send_self_frame(
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    chr_name: &str,
    body: Vec<u8>,
) {
    let Some(me) = online.iter().find(|e| e.chr_name == chr_name) else {
        return;
    };
    let Some(tx) = route_to_gate(gates, me.gate_idx, me.socket, "魔法自身帧") else {
        return;
    };
    let _ = tx.send(SimToGate::SendFrame {
        socket: me.socket,
        body,
    });
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
///
/// **F-1（reviewer-6 `7724b9e441`）**：`RM_SPELL` 保持此口径（`PlayObject.Message.cs:1454-1459`
/// 有 `ActorId != ActorId` 守卫）；`RM_MAGICFIRE` 的处理器**没有**该守卫
/// （`PlayObject.Message.cs:1788-1793`）⇒ 施法者自己也收帧，用 [`broadcast_ref_msg_with_self`]。
fn broadcast_ref_msg(
    world: &mir2_world::World,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    actor_id: i32,
    bodies: &[Vec<u8>],
) {
    broadcast_ref_msg_opt(world, online, gates, actor_id, bodies, false);
}

/// `RM_MAGICFIRE` 口径（`PlayObject.Message.cs:1788-1793` 无 `ActorId != ActorId` 守卫）：
/// 广播集合**包含施法者自己**。
fn broadcast_ref_msg_with_self(
    world: &mir2_world::World,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    actor_id: i32,
    bodies: &[Vec<u8>],
) {
    broadcast_ref_msg_opt(world, online, gates, actor_id, bodies, true);
}

/// [`broadcast_ref_msg`] / [`broadcast_ref_msg_with_self`] 的公共体。
fn broadcast_ref_msg_opt(
    world: &mir2_world::World,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    actor_id: i32,
    bodies: &[Vec<u8>],
    include_self: bool,
) {
    for target_id in chat::hear_targets(world, actor_id, chat::SEND_REF_MSG_RANGE) {
        if !include_self && target_id == actor_id {
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

/// 生产宿主构造**玩家实体**（`PlayObject` 构造器语义的最小面）。
///
/// oracle `PlayObject` 构造器置 `WantRefMsg = true`（`PlayObject.cs:889`；`BaseObject` 缺省 false，
/// `BaseObject.cs:431`）。AOI 可见过滤（`BaseObject.ViewRange.cs:112`）靠它把**非玩家**（怪物）
/// 纳入可见集 —— 只靠 `race == ActorRace.Play` 只能看到其他玩家。
///
/// oracle 同一构造器还置 **`ViewRange = 12`**（`PlayObject.cs:904`；`BaseObject` 缺省 5，
/// `BaseObject.cs:383` = [`mir2_world::Entity::new`] 的 `view_range` 缺省）。AOI 扫描是边长
/// `2*ViewRange+1` 的正方形（`BaseObject.ViewRange.cs:71-74`）⇒ 玩家窗口应为 25×25；
/// 缺这行只剩 11×11，Chebyshev 距离 6..12 的怪/其他 actor 不进 `visible_actors`、
/// `drain_aoi_downlink` 也不发 ⇒ 客户端同屏少怪（卡 `openmir2-client-e2e-enter-gaps` 缺口②）。
fn new_player_entity(chr_name: String, x: i32, y: i32) -> mir2_world::Entity {
    let mut e = mir2_world::Entity::new(0, chr_name, x, y);
    e.want_ref_msg = true;
    e.view_range = 12;
    e
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

/// 进图时把本会话写入 52/752 的**输入值**与 52 的**尾字节**打进日志（只读；卡 `openmir2-client-e2e-enter-gaps`）。
///
/// 用途：真机上“客户端血/负重为 0”时，先看这行就能区分「服务端没发/值错」与「客户端解析」。
/// 不参与任何模拟/出帧语义（与 oracle `RecalcAbilitys` 输出同源，不额外取数）。
fn log_enter_ability(chr_name: &str, out: &game_svr::ability::RecalcOutput) {
    let w = &out.wabil;
    // 与生产路径用**同一**布局（`MIR2_CLIENT_ABILITY_LAYOUT`）：默认冻结客户端 58B，
    // `=0` 时回到 oracle 40B。日志必须反映**实际写入 52/752 的字节**，否则又会造成“血条为 0”的误读。
    let layout = if mir2_client_ability_layout() {
        "client-58"
    } else {
        "oracle-40"
    };
    // 与生产路径用**同一**选路（`ability_tail`，唯一真源 `MIR2_CLIENT_ABILITY_LAYOUT`）。
    let tail_bytes = ability_tail(w, layout == "client-58");
    let tail_len = tail_bytes.len();
    let tail: String = tail_bytes.iter().map(|b| format!("{b:02x}")).collect();
    let s = &out.sub;
    glog!(
        "DBG",
        "52/752 入图输入 ChrName={chr_name} layout={layout} tail_len={tail_len} \
         WAbil[level={} ac={} mac={} dc={} mc={} sc={} hp={} mp={} \
         max_hp={} max_mp={} exp={} max_exp={} weight={} max_weight={} wear_weight={} \
         max_wear_weight={} hand_weight={} max_hand_weight={}] SubAbility[anti_magic={} \
         hit_point={} speed_point={} anti_poison={} poison_recover={} health_recover={} \
         spell_recover={}] ability_tail_hex={tail}",
        w.level,
        w.ac,
        w.mac,
        w.dc,
        w.mc,
        w.sc,
        w.hp,
        w.mp,
        w.max_hp,
        w.max_mp,
        w.exp,
        w.max_exp,
        w.weight,
        w.max_weight,
        w.wear_weight,
        w.max_wear_weight,
        w.hand_weight,
        w.max_hand_weight,
        s.anti_magic,
        s.hit_point,
        s.speed_point,
        s.anti_poison,
        s.poison_recover,
        s.health_recover,
        s.spell_recover
    );
}

/// 一个在世界内的会话的 AOI 观测读数（只读；卡 `openmir2-client-e2e-enter-gaps`）。
///
/// `vis`/`new` 的语义**必须分开**（root 2026-10-14 的 ③ 误判根因）：`update_visible_gay`
/// （`BaseObject.ViewRange.cs:12-38`，Rust `aoi.rs:28-49`）只有**首次看见**才置
/// `VisibleFlag::Show`，对「已在列表、本轮又看见」置 `VisibleFlag::Invisible`（`aoi.rs:39-43`）。
/// ⇒ 只数 `Show` 得到的是「本轮新增」，**不是当前可见集**；`invisible` 语义是「可见」不是「不可见」。
#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionAoiDiag {
    chr_name: String,
    actor_id: i32,
    map_id: usize,
    /// 观察者 `view_range`（`PlayObject.cs:904` = 12）；扫描窗口 = 边长 `2*vr+1` 的正方形。
    view_range: i32,
    /// **当前可见集大小**（**不含自身**）= `visible_actors` 里 `target != self && flag != Hidden`
    /// （即 `Show | Invisible`）的条数。
    vis: usize,
    /// **本轮新增可见**（**不含自身**）= `VisibleFlag::Show` 的条数（= 本 tick 触发出现帧的对数）。
    new: usize,
    /// 本会话**累计**收到的 AOI 出现帧数（`SM_TURN` + `SM_FEATURECHANGED`，实发计数）。
    frames: usize,
    /// 同图（除自己）的实体数。
    same_map: usize,
    /// 同图（除自己）最近的 Chebyshev 距离；无他者 ⇒ `i32::MAX`。
    nearest: i32,
}

/// 周期日志的世界/AOI 观测快照（只读；不改模拟语义）。
///
/// 直接回应 `openmir2-client-e2e-enter-gaps` 的误读：`sim: … 处理 N 项` 的 N 是**世界命令队列**
/// 长度（[[TickRecord::processed]]），不是实体数；实体/怪物/展示表与 AOI 可见数必须另看本快照。
#[derive(Clone, Debug, PartialEq, Eq)]
struct WorldDiag {
    entities: usize,
    displays: usize,
    sessions: Vec<SessionAoiDiag>,
}

impl WorldDiag {
    fn summary(&self) -> String {
        format!(
            "实体内 {} 展示表 {} 在世界会话 {}",
            self.entities,
            self.displays,
            self.sessions.len()
        )
    }

    fn session_summary(&self) -> String {
        let mut out = String::new();
        for s in &self.sessions {
            out.push_str(&format!(
                " [{}({}) map={} vr={} vis={} new={} af={} same_map={} nearest={}]",
                s.chr_name,
                s.actor_id,
                s.map_id,
                s.view_range,
                s.vis,
                s.new,
                s.frames,
                s.same_map,
                if s.nearest == i32::MAX { -1 } else { s.nearest }
            ));
        }
        out
    }
}

/// 按会话表顺序算出世界/AOI 观测快照（确定性：会话表是 `Vec`，实体遍历按槽位）。
///
/// `aoi_frames` = 每会话**累计**实发的 AOI 出现帧数（由 sim 循环从 [`drain_aoi_downlink`]
/// 的返回值累加；按 `chr_name` 键，避免实体槽位 LIFO 复用把旧会话的计数带过来）。
fn world_aoi_diag(
    world: &mir2_world::World,
    displays: &downlink::DisplayTable,
    aoi_frames: &std::collections::HashMap<String, usize>,
) -> WorldDiag {
    let entities: Vec<&mir2_world::Entity> = world.entities.iter().collect();
    let mut sessions = Vec::new();
    for sess in world.sessions.iter() {
        if sess.stage != mir2_world::WorldStage::InWorld {
            continue;
        }
        let Some(actor_id) = sess.actor_id else {
            continue;
        };
        let Some(e) = world.entities.get(actor_id) else {
            continue;
        };
        // `Show` = 本轮新增（出现帧触发批）；`Invisible` = 已在列表且本轮又看见（仍是「可见」）。
        // 两者一起才是**当前可见集**；只数 `Show` 会让停留 >1 tick 的怪在诊断里恒为 0。
        // **排除自身**：`search_view_range` 会把观察者自己所在格子的自己也算进 `visible_actors`
        // （oracle `BaseObject.SearchViewRange` 也不排除），但 `PlayObject.Run()` 的 `baseObject != this`
        // 与 [`drain_aoi_downlink`] 都跳过自身 ⇒ 诊断报的是「同屏**其它**可见对象数」。
        let vis = e
            .visible_actors
            .iter()
            .filter(|v| v.target != actor_id && v.flag != mir2_world::VisibleFlag::Hidden)
            .count();
        let new = e
            .visible_actors
            .iter()
            .filter(|v| v.target != actor_id && v.flag == mir2_world::VisibleFlag::Show)
            .count();
        let frames = aoi_frames.get(&sess.chr_name).copied().unwrap_or(0);
        let mut same_map = 0usize;
        let mut nearest = i32::MAX;
        for o in &entities {
            if o.id == actor_id || o.map_id != e.map_id {
                continue;
            }
            same_map += 1;
            let d = (o.x - e.x).abs().max((o.y - e.y).abs());
            if d < nearest {
                nearest = d;
            }
        }
        sessions.push(SessionAoiDiag {
            chr_name: sess.chr_name.clone(),
            actor_id,
            map_id: e.map_id,
            view_range: e.view_range,
            vis,
            new,
            frames,
            same_map,
            nearest,
        });
    }
    WorldDiag {
        entities: entities.len(),
        displays: displays.len(),
        sessions,
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
///
/// 返回**本调用每个会话实发的帧数**（`(chr_name, frames)`，只含 `>0` 的会话）——供 sim 循环
/// 累加成诊断读数 `af=`（root 2026-10-14 要求「打印可见集大小与下发帧数」）。
/// 这里只对 `Show`（＝首次看见）发帧，是对齐 oracle `Run()` 里 `VisibleFlag.Show` 支。
fn drain_aoi_downlink(
    world: &mir2_world::World,
    displays: &downlink::DisplayTable,
    online: &OnlineRegistry,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
) -> Vec<(String, usize)> {
    let mut sent: Vec<(String, usize)> = Vec::new();
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
        let mut sent_here = 0usize;
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
            let mut frame_display = display.clone();
            if target.race != mir2_world::ACTOR_RACE_PLAY {
                // 怪物的 `BaseObject.Dir` 由 AI（`AnimalObject`/`MonsterObject`）写在 `Entity.ai.dir`；
                // 展示表里的副本是生成时的初值 ⇒ 出现帧用世界**当前**朝向（oracle `SendMsg(…, Dir, …)`）。
                frame_display.dir = target.ai.dir;
            }
            let bodies = downlink::turn_pair(target.id, x, y, &frame_display);
            let Some(tx) = route_to_gate(gates, entry.gate_idx, entry.socket, "同屏下行")
            else {
                continue;
            };
            for body in bodies {
                let _ = tx.send(SimToGate::SendFrame {
                    socket: entry.socket,
                    body,
                });
                sent_here += 1;
            }
        }
        if sent_here > 0 {
            sent.push((sess.chr_name.clone(), sent_here));
        }
    }
    sent
}

/// `MessageSettings.BODYLUCKUNIT`（`MessageSettings.cs:9`）。
const BODYLUCK_UNIT: f64 = 5000.0;

/// `PlayObject.AddBodyLuck(double)`（`PlayObject.cs:4304-4323`）的纯函数形式。
///
/// 逐行：
/// - `if (dLuck > 0 && BodyLuck < 5 * BODYLUCKUNIT) BodyLuck = BodyLuck + dLuck;`
/// - `if (dLuck < 0 && BodyLuck > -(5 * BODYLUCKUNIT)) BodyLuck = BodyLuck + dLuck;`
/// - `BodyLuckLevel = clamp(Convert.ToInt32(BodyLuck / BODYLUCKUNIT), -10, 5)`。
///
/// `Convert.ToInt32(double)` 是 .NET 的**银行家舍入**（`Math.Round`，midpoint-to-even）
/// ⇒ 这里用 `f64::round_ties_even()`（**不是** Rust `round()` 的 half-away-from-zero）。
/// `BodyLuck` 落 `characters.BodyLuck double(10,3)`（`sql/mir2_db.sql:46`）且被 `scoped_diff`
/// 逐字节比对 ⇒ 累积必须与 oracle 同序（`GetExp` 的 `dwExp*0.002` 先于升级块 `+100`）。
fn add_body_luck(body_luck: &mut f64, body_luck_level: &mut i32, d_luck: f64) {
    if d_luck > 0.0 && *body_luck < 5.0 * BODYLUCK_UNIT {
        *body_luck += d_luck;
    }
    if d_luck < 0.0 && *body_luck > -(5.0 * BODYLUCK_UNIT) {
        *body_luck += d_luck;
    }
    let n = (*body_luck / BODYLUCK_UNIT).round_ties_even();
    *body_luck_level = (n as i32).clamp(-10, 5);
}

/// `PlayObject.Initialize()` 的 `AddBodyLuck(0)`（`PlayObject.cs:1060`）：登录/进世界时由
/// 已落库的 `BodyLuck` 重算 `BodyLuckLevel`（`dLuck == 0` 时两个累加支都不进，只重算等级）。
fn body_luck_level_of(body_luck: f64) -> i32 {
    let n = (body_luck / BODYLUCK_UNIT).round_ties_even();
    (n as i32).clamp(-10, 5)
}

/// `openmir2-m3-attack-magic-wiring`（第一片）：本 world tick 的攻击结算 → 客户端下行。
///
/// 逐条口径：
/// - **受击**（`BaseObject.Attack.cs:57` + `BaseObject.Base.cs:699-705` +
///   `BaseObject.Message.cs:383-470` + `PlayObject.Message.cs:1562-1628`）：`_Attack` 对受击者
///   `SendStruckDelayMsg(RM_REFMESSAGE, nPower, HP, MaxHP, ActorId, "", 200)`；受击者 `Run()`
///   处理 `RM_REFMESSAGE` 时 `SendRefMsg(RM_STRUCK, damage, HP, MaxHP, hitter, "")` ⇒
///   **同图、切比雪夫 ≤ `SendRefMsgRange`(12)、`Race==Play` 且非 Ghost 的每个对象**都收到
///   `RM_STRUCK`，各观察者再用**受击者**的 `HP/MaxHP/伤害` 回 `SM_STRUCK` 给自己的客户端。
///   ⇒ **怪物目标同样广播**（怪物自身无连接，但视野内的玩家收到 ⇒ 客户端能看到掉血/伤害）。
///   登记（未 1:1，本次不动）：oracle 该帧经受击者 `MsgQueue` 以 `LateDelivery=200ms` +
///   `MessagePriority.Lowest` 延时投递；本实现**即时**发（见证据 `155-*`）。
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
    // ① 受击帧（oracle `SendRefMsg(RM_STRUCK)` ⇒ **受击者视野内全体 `Play`**，不只是受击者自己）。
    //
    // 口径：`BaseObject.Attack.cs:57`（`SendStruckDelayMsg(RM_REFMESSAGE, …)`）→ 受击者 `Run()` 的
    // `RM_REFMESSAGE` 支（`BaseObject.Base.cs:699-705`）`SendRefMsg(RM_STRUCK, damage, HP, MaxHP,
    // hitter, "")` → `SendRefMsg`（`BaseObject.Message.cs:383+`）对**同图、切比雪夫 ≤ 12、`Race==Play`、
    // 非 Ghost** 的对象 `SendMsg(this /*受击者*/, RM_STRUCK, …)` → 观察者 `RM_STRUCK` 处理器
    // （`PlayObject.Message.cs:1562-1628`）用**受击者**的 `HP/MaxHP/伤害` 回 `SM_STRUCK` 给自己客户端。
    // ⇒ 怪物目标也一样广播（怪物无连接，但视野内玩家收到 ⇒ 客户端能看见掉血/伤害）。
    //
    // 登记（未 1:1，本次不动）：oracle 的该帧经受击者 `MsgQueue` 以 `LateDelivery=200ms` +
    // `MessagePriority.Lowest` 延时投递，本实现即时发。
    for (attacker, target, outcome) in &rec.attacks {
        if outcome.damage == 0 {
            // oracle `RM_STRUCK` 支只在 `processMsg.wParam > 0` 时回帧（`PlayObject.Message.cs:1563`）。
            continue;
        }
        let Some(d) = displays.get(*target) else {
            continue;
        };
        let max_hp = world
            .entities
            .get(*target)
            .map_or(outcome.target_hp_after, |e| e.max_hp);
        // 受击者是**在线玩家**时：世界权威 HP 同步回其会话（供 52/752 与后续帧），并按 oracle
        // `RM_STRUCK` 处理器开头（`PlayObject.Message.cs:1587-1590`）清零体力/魔力点。
        // 怪物无会话 ⇒ 无这些副作用（它们只存在于有 `WAbil`/`MsgQueue` 的受击者上）。
        if let Some(victim_entry) = online.iter().find(|e| e.actor_id == *target) {
            if let Some(sg) = sessions.get_mut(&victim_entry.chr_name) {
                sg.state.wabil.hp = outcome.target_hp_after;
                // 受击清零 ⇒ 受害者永远到不了 `HealthFillTime`，与真机 dump 一致（受害方不回血）。
                sg.stamina.on_struck();
                sg.recovery.per_health = sg.recovery.per_health.wrapping_sub(1);
                sg.recovery.per_spell = sg.recovery.per_spell.wrapping_sub(1);
            }
        }
        let body = game_svr::smframe::sm_struck_frame(
            *target,
            outcome.target_hp_after,
            max_hp,
            outcome.damage,
            d.feature,
            d.status,
            *attacker,
            false,
        );
        for observer_id in chat::hear_targets(world, *target, chat::SEND_REF_MSG_RANGE) {
            let Some(observer) = online.iter().find(|e| e.actor_id == observer_id) else {
                continue;
            };
            let Some(tx) = route_to_gate(gates, observer.gate_idx, observer.socket, "受击帧")
            else {
                continue;
            };
            let _ = tx.send(SimToGate::SendFrame {
                socket: observer.socket,
                body: body.clone(),
            });
        }
    }
    // ② 死亡广播（含死者自己；展示表登记了玩家与已生成的怪物，未登记 actor 跳过并 WRN）。
    for death in &rec.deaths {
        let dead_id = death.0;
        let Some(d) = displays.get(dead_id) else {
            glog!("WRN", "死亡下行：actor={dead_id} 无展示表 ⇒ 跳过（登记）");
            continue;
        };
        // 死者的 `Dir`：玩家用展示表（客户端指令维护），怪物用 `Entity.ai.dir`（AI 维护）。
        let (x, y, dead_dir) = world.entities.get(dead_id).map_or((0, 0, d.dir), |e| {
            (
                e.x,
                e.y,
                if e.race == mir2_world::ACTOR_RACE_PLAY {
                    d.dir
                } else {
                    e.ai.dir
                },
            )
        });
        let body = game_svr::smframe::sm_death(
            mir2_protocol::messages::SM_NOWDEATH,
            dead_id,
            wire_coord(x),
            wire_coord(y),
            dead_dir,
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
    for (winner, amount, leveled, level) in &rec.exp_awards {
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
            // oracle `GetExp`（`PlayObject.Base.cs:151-162`）的两处 `AddBodyLuck`，**次序不可换**：
            // `AddBodyLuck(dwExp * 0.002)` 在 `Abil.Exp += dwExp` 之后、升级块 `AddBodyLuck(100)` 之前。
            add_body_luck(
                &mut sg.equip.body_luck,
                &mut sg.equip.body_luck_level,
                f64::from(*amount) * 0.002,
            );
            if *leveled {
                add_body_luck(
                    &mut sg.equip.body_luck,
                    &mut sg.equip.body_luck_level,
                    100.0,
                );
            }
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

/// **D②（升级链下行 · 第 1 段 = `SM_HEALTHSPELLCHANGED(53)`）**：`TickRecord::exp_awards` 中
/// `leveled == true` 的获得者（= oracle `GetExp` 进了 `Abil.Exp >= Abil.MaxExp` 块）先补
/// `HasLevelUp` 的**能力重算**与 `GetExp` 尾的 `IncHealthSpell(2000,2000)` **回血**，再把升级块的
/// `SM_HEALTHSPELLCHANGED(53)` 发出去。
///
/// **为什么 53 必须在 `drain_combat_downlink` 的 `SM_WINEXP(44)` 之前**
/// （reviewer-6 `c5a90bf4` F-1，owner 裁决 `b2851a44`）：
/// oracle `GetExp`（`PlayObject.Base.cs:149-166`）把 `RM_WINEXP(8075)`、`RM_LEVELUP(8086)`、
/// `RM_HEALTHSPELLCHANGED(10052)` 全部 `MsgQueue.Enqueue(…, (byte)wIdent)`
/// （`BaseObject.Message.cs:26-95`；`RM_HEALTHSPELLCHANGED` 经 `SendUpdateMsg`，`:275-304`，按同 ident
/// 去重）；`BaseObject.Run()` 的 `while (GetMessage(…)) Operate(…)`（`BaseObject.Base.cs:26-30`）
/// 按**优先级字节升序出队**，且 `GetMessage` 每次取当前最小值 ⇒
/// `(byte)RM_HEALTHSPELLCHANGED=68 < (byte)RM_WINEXP=139 < (byte)RM_LEVELUP=150`
/// ⇒ 同一 socket 上的真实线上序 = `53 → 44 → 45 → 52 → 752`（`RM_LEVELUP` 处理器内的 45/52/752
/// 是即时 `SendSocket`，见 [`drain_levelup_downlink`]）。源码里的**入队序**（`44 → 45 → 53`）
/// **不是**线上序。
///
/// 因此本函数必须在 [`drain_combat_downlink`]（发 44）**之前**调用；同 tick 内恢复/体力点段若也产生
/// 53，`SendUpdateMsg` 只保留最后一条（本函数把 HP/MP 补满后，后段不再命中 ⇒ 等价）。
///
/// **登记**：`ShowHp` 的 `SendRefMsg` 广播支恒 false（技能「心灵启示」未移植），与
/// `drain_recovery`/`drain_stamina` 同口径；`WinExp` 的 `LimitExpLevel` 支生产不可达
/// （`GameSvrConf.cs:1838` 默认 `LimitExpLevel = 1000 > 255`）。
#[allow(clippy::too_many_arguments)]
fn drain_levelup_healthspell(
    rec: &mir2_world::TickRecord,
    world: &mut mir2_world::World,
    online: &OnlineRegistry,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    catalog: &dyn game_svr::ability::ItemCatalog,
    range: i32,
) {
    for (winner, _amount, leveled, level) in &rec.exp_awards {
        if !*leveled {
            continue;
        }
        let Some(entry) = online.iter().find(|e| e.actor_id == *winner) else {
            continue;
        };
        let Some(sg) = sessions.get_mut(&entry.chr_name) else {
            continue;
        };
        // `GetExp` 已把 `Abil.Exp/Level/MaxExp` 更新到世界实体（战斗权威值）⇒ 先同步进会话，
        // 否则 `RecalcLevelAbilitys()` 会按旧等级算 MaxHP/MaxMP（`PlayObject.Base.cs:151-159`）。
        if let Some(entity) = world.entities.get(*winner) {
            sg.equip.abil.exp = entity.exp;
            sg.equip.abil.level = *level;
            sg.equip.abil.max_exp = entity.max_exp;
            sg.state.wabil.exp = entity.exp;
            sg.state.wabil.level = *level;
            sg.state.wabil.max_exp = entity.max_exp;
        }
        // ① HasLevelUp：RecalcLevelAbilitys() + RecalcAbilitys()
        recalc_session_ability(sg, catalog);
        // ② GetExp 尾：IncHealthSpell(2000, 2000)（`BaseObject.cs:952-975`）
        let hp = u32::from(sg.state.wabil.hp) + 2000;
        let mp = u32::from(sg.state.wabil.mp) + 2000;
        sg.state.wabil.hp = if hp >= u32::from(sg.state.wabil.max_hp) {
            sg.state.wabil.max_hp
        } else {
            hp as u16
        };
        sg.state.wabil.mp = if mp >= u32::from(sg.state.wabil.max_mp) {
            sg.state.wabil.max_mp
        } else {
            mp as u16
        };
        // ③ 世界实体（战斗权威值）同步
        if let Some(e) = world.entities.get_mut(*winner) {
            apply_ability_to_entity(e, &sg.state.wabil, &sg.sub);
        }
        // ④ `RM_HEALTHSPELLCHANGED` 入队后按优先级 68 出队 ⇒ 线上**先于** 44
        // 布局：默认冻结客户端（`TCharDesc` 尾），`MIR2_CLIENT_ABILITY_LAYOUT=0` ⇒ oracle header-only。
        let hb = game_svr::smframe::sm_healthspellchanged_frame(
            sg.actor_id,
            sg.state.wabil.hp,
            sg.state.wabil.mp,
            sg.state.wabil.max_hp,
        );
        if let Some(tx) = route_to_gate(gates, entry.gate_idx, entry.socket, "升级回帧(53)") {
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body: hb.clone(),
            });
        }
        if sg.show_hp {
            broadcast_healthspellchanged(world, online, gates, sg.actor_id, &hb, range);
        }
    }
}

/// **D②（升级链下行 · 第 2 段 = `SM_LEVELUP(45)` / `SM_ABILITY(52)` / `SM_SUBABILITY(752)`）**：
/// oracle `RM_LEVELUP` 处理器（`PlayObject.Message.cs:1670-1676`）在**出队到 `RM_LEVELUP` 时**
/// 即时 `SendSocket` 三条 ⇒ 线上序固定在 44 **之后**、53 **之后**
/// （入队/出队序见 [`drain_levelup_healthspell`] 的模块头）。必须在 [`drain_combat_downlink`] 之后调用。
///
/// **登记**：`HasLevelUp` 末尾的 `FunctionNPC.GotoLable(this, "@LevelUp", false)` 在 oracle 里
/// 已被注释（`PlayObject.cs:5249-5252`）⇒ 不发；`AddBodyLuck(100)` 已在
/// [`drain_combat_downlink`] 的经验循环里按 oracle 次序（`dwExp*0.002` 之后）落账。
fn drain_levelup_downlink(
    rec: &mir2_world::TickRecord,
    online: &OnlineRegistry,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    client_layout: bool,
) {
    for (winner, _amount, leveled, _level) in &rec.exp_awards {
        if !*leveled {
            continue;
        }
        let Some(entry) = online.iter().find(|e| e.actor_id == *winner) else {
            continue;
        };
        let Some(sg) = sessions.get_mut(&entry.chr_name) else {
            continue;
        };
        let send = |body: Vec<u8>| {
            if let Some(tx) = route_to_gate(gates, entry.gate_idx, entry.socket, "升级回帧") {
                let _ = tx.send(SimToGate::SendFrame {
                    socket: entry.socket,
                    body,
                });
            }
        };
        send(game_svr::smframe::sm_levelup(
            sg.state.wabil.exp,
            sg.state.wabil.level,
        ));
        send(game_svr::smframe::sm_ability(
            sg.state.gold,
            sg.equip.job,
            sg.game_gold,
            &ability_tail(&sg.state.wabil, client_layout),
        ));
        let sb = sg.sub.to_frame_bytes();
        send(game_svr::smframe::sm_subability(
            sb[0], sb[1], sb[2], sb[3], sb[4], sb[5], sb[6],
        ));
    }
}

/// **D①+D② 击杀经验下行链**：把 `GetExp` 产生的四类帧按 oracle `MsgQueue` **出队序**固定在
/// 一处：`SM_HEALTHSPELLCHANGED(53, 优先级 68) → SM_WINEXP(44, 139) → 45/52/752（`RM_LEVELUP`
/// 处理器内即时 SendSocket）`；顺带在同一处发出受击/死亡帧（同一 tick 的 `attacks/deaths`）。
///
/// `sim_loop` 与升级帧序用例都经本函数 ⇒ **顺序本身可被用例钉住**（红检 = 交换前两行 ⇒ 用例必红）。
#[allow(clippy::too_many_arguments)]
fn drain_kill_exp_chain(
    rec: &mir2_world::TickRecord,
    world: &mut mir2_world::World,
    displays: &downlink::DisplayTable,
    online: &OnlineRegistry,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    catalog: &dyn game_svr::ability::ItemCatalog,
    range: i32,
    client_layout: bool,
) {
    // 53（= `GetExp` 尾的 `IncHealthSpell(2000,2000)`）先于 44（`SendMsg(RM_WINEXP)`）。
    drain_levelup_healthspell(rec, world, online, sessions, gates, catalog, range);
    drain_combat_downlink(rec, world, displays, online, sessions, gates);
    drain_levelup_downlink(rec, online, sessions, gates, client_layout);
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
    superseded_healthspell: &std::collections::HashSet<i32>,
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
        // **F-1 的同 tick 去重**：oracle `SendUpdateMsg(RM_HEALTHSPELLCHANGED)`
        // （`BaseObject.Message.cs:275-304`）先移除 `MsgQueue` 里同 ident 的旧消息再入队。恢复段在
        // `PlayObject.Run()` 里**先**入队 53，随后 `BaseObject.Run()` 的消息段处理击杀/升级
        // （触发消息优先级 `MessagePriority.Normal=1` / `RM_HIT=20` 均 < `(byte)RM_HEALTHSPELLCHANGED=68`
        // ⇒ 先出队）⇒ 升级段的 `IncHealthSpell(2000,2000)→HealthSpellChanged()` 会把恢复段的 53 替换掉。
        // 因此若本 tick 该对象进升级块，这里**不回帧**（由 [`drain_levelup_healthspell`] 出唯一一条）。
        // HP/MP/`Inc*` 的状态变更仍照常发生。
        // 登记：由**最低优先级**（255）延时消息驱动的击杀（恢复段 53 先出队）不在此模型内。
        if superseded_healthspell.contains(&sg.actor_id) {
            continue;
        }
        let Some(entry) = online.iter().find(|e| e.chr_name == name) else {
            glog!(
                "WRN",
                "周期恢复：ChrName={name} 无在线网关坐标 ⇒ 不回帧（登记）"
            );
            continue;
        };
        let body = game_svr::smframe::sm_healthspellchanged_frame(
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
/// 位置（reviewer-5 O-1 收口）：oracle 一次 `PlayObject.Run()` 的 `base.Run()`（`PlayObject.Message.cs:765`）
/// 先把**消息段**跑完（`BaseObject.Base.cs:28-34`；攻击/击杀经验链与 AOI 出现帧都在这一段出帧），
/// 再跑本段（`:36-68` 的填充 + `:110-115` 的负值扣血）⇒ 本段必须晚于 `drain_kill_exp_chain` /
/// `drain_aoi_downlink`（次序由 [`drain_run_downlinks`] 固定）。逐会话（**按键排序**）调
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
            let body = game_svr::smframe::sm_healthspellchanged_frame(actor_id, *hp, *mp, max_hp);
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

/// **每 tick `world.tick()` 之后的帧面下行段（oracle 次序）**：一次 `PlayObject.Run()` 里
/// `base.Run()`（`PlayObject.Message.cs:765`）先跑**消息段**（`BaseObject.Base.cs:28-34` 的
/// `while (GetMessage(ref msg)) Operate(msg);`）——本宿主的击杀经验/升级链（`drain_kill_exp_chain`）
/// 与 AOI 出现帧（`drain_aoi_downlink`）都对应这一段——再跑**体力/魔力填充段**
/// （`BaseObject.Base.cs:36-68`）与**体力点扣血段**（`:110-115`，`drain_stamina`）。
/// ⇒ 出帧次序固定在 `drain_kill_exp_chain`（53→44→45→52→752）→ `drain_aoi_downlink` → `drain_stamina`。
/// 与 [`drain_stamina`] 文档「登记（阶段顺序）」一致（reviewer-5 O-1 收口）。
///
/// **改坏必红**：把 [`drain_stamina`] 挪到 [`drain_kill_exp_chain`] 之前 ⇒ 同 tick 既升级又掉点时
/// 帧序变成 `53(体力) → 53(升级) → 44/45/52/752`，`tick_frame_order_puts_stamina_after_kill_exp_chain` 必红。
#[allow(clippy::too_many_arguments)]
fn drain_run_downlinks(
    rec: &mir2_world::TickRecord,
    world: &mut mir2_world::World,
    displays: &downlink::DisplayTable,
    online: &OnlineRegistry,
    sessions: &mut std::collections::HashMap<String, SessionGround>,
    gates: &std::collections::HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    catalog: &dyn game_svr::ability::ItemCatalog,
    range: i32,
    health_fill_time: i32,
    spell_fill_time: i32,
    client_layout: bool,
) -> Vec<(String, usize)> {
    drain_kill_exp_chain(
        rec,
        world,
        displays,
        online,
        sessions,
        gates,
        catalog,
        range,
        client_layout,
    );
    let aoi_frames = drain_aoi_downlink(world, displays, online, gates);
    drain_stamina(
        sessions,
        world,
        online,
        gates,
        range,
        health_fill_time,
        spell_fill_time,
    );
    aoi_frames
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

/// 商店定价/修理/税的配置层输入（`Server.conf [Setup]`；oracle 读取点见 `config.rs`）。
///
/// 价目/修理数值口径在 `mir2_formula::shop`（已合入的 ⑤-3）；本结构只带**配置层**字段，
/// `price_rate`/`castle_merchant`/`item_type_list` 随商人实体（[`mir2_world::merchant::MerchantState`]）。
#[derive(Debug, Clone, Copy)]
struct ShopCfg {
    /// `Config.CastleMemberPriceRate`（`Merchant.cs:772`）。
    castle_member_price_rate: i32,
    /// `Config.SuperRepairPriceRate`（`Merchant.cs:1573/1619`）。
    super_repair_price_rate: i32,
    /// `Config.RepairItemDecDura`（`Merchant.cs:1656`）。
    repair_item_dec_dura: i32,
    /// `Config.GetAllNpcTax`（`Merchant.cs:544/1250/1385/1637`；`[Setup] CastleGetAllNpcTax`）。
    get_all_npc_tax: bool,
    /// `Config.HumanMaxGold`（`IncGold` 上限，`PlayObject.Base.cs:168`）。
    human_max_gold: i32,
}

/// 一个在线角色的**商店会话态**（`CM_MERCHANTDLGSELECT` 写入 `PlayObject.ScriptLable`；
/// ⑤-4 第①片只需要它；仓库态（105-2）第②片再扩）。
#[derive(Debug, Clone, Default)]
struct ShopSession {
    /// `PlayObject.ScriptLable`（`Merchant.cs:799`：`playObject.ScriptLable = sData`）——
    /// `CM_USERREPAIRITEM` 的 `@s_repair`/`@fail_s_repair` 分支靠它。
    script_lable: String,
}

/// 一个在线角色的**地面交互会话态**（`CM_PICKUP`/`CM_DROPITEM` 用）。
///
/// 进世界时从 `CharacterData` 建立、断线/小退时移除（`CharacterData` 在其他路径上用完即弃，
/// 本结构把 `ground::*` 要读写的字段留下来）。
struct SessionGround {
    /// `ground::GroundState`（`PlayObject.ItemList` + `Gold` + `WAbil`）。
    state: game_svr::ground::GroundState,
    /// `PlayObject.StorageItemList` + `IsCanGetBackItem`（仓库会话态，[`storage`]；
    /// 进世界时从 `CharacterData.StorageItems` 读档、存档时由 [`build_save_record`] 写回）。
    storage: storage::StorageState,
    /// `BaseObject.ActorId`（`SM_SYSMESSAGE` 的 `Recog`）。
    actor_id: i32,
    /// `BaseObject.GameGold`（`SM_GOLDCHANGED` 的 LoWord/HiWord）。
    game_gold: i32,
    /// `PlayObject.SoftVersionDateEx`（= `LoadDBInfo.SoftVersionDate`）：
    /// `SendGoldInfo` 的 `== 0 ⇒ 不发` 门（`PlayObject.Base.cs:930`）——元宝寄售的
    /// `SM_GAMEGOLDNAME` 与登录帧 5008 走同一条门。
    soft_version_date: i32,
    /// `BaseObject.GamePoint`（`SM_GAMEGOLDNAME` 的 LoWord/HiWord）。
    game_point: i32,
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
    /// `PlayObject.BodyLuck`（`PlayObject.cs:520`；`characters.BodyLuck double(10,3)`）——
    /// 随 `GetExp` 的 `AddBodyLuck(dwExp*0.002)` / 升级 `AddBodyLuck(100)` 累积，
    /// 存档时由 [`build_save_record`] 写回。
    body_luck: f64,
    /// `PlayObject.BodyLuckLevel`（`PlayObject.cs:521`）——由 `BodyLuck` 派生（`AddBodyLuck` 末段）。
    body_luck_level: i32,
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
    /// **M3⑨-实现 2 第②片**：`magics` 模板表（`M2Share.MagicList` 的查表面）；
    /// `DoSpell` 效果体/帧参用。空表 = `GetMagicInfo == null`（效果体不接，登记）。
    magic_table: magics::MagicTable,
    /// **M3⑨-实现 2 第②片**：`Server.conf` 宝宝配置（召唤选型用）。
    pet_cfg: mir2_data::pet_slave::PetSlaveConfig,
    /// **M3⑨-实现 2 第②片**：`[Setup] MagicAttackRage`（`GameSvrConf.cs:873/1752` 缺省 8）——
    /// `DoSpell` 施法距离门。
    magic_attack_rage: i32,
    /// `CM_PICKUP`/`CM_DROPITEM` 的 `[Setup]` 输入。
    ground: GroundCfg,
    /// `CM_*` 商店动作的 `[Setup]` 输入（⑤-4 第①片）。
    shop: ShopCfg,
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
    /// **GuardList 放置半（片71）**：`Envir/GuardList.txt` 解析出的守卫行（主线程装载，
    /// `game_svr::guardlist::load_guard_list_file`）；`None` = 无文件 / VentureServer=TRUE /
    /// 读失败 ⇒ 不放置。放置在 sim 线程起世界后（`game_svr::guardlist::place_guards`）。
    guards: Option<Vec<mir2_envir::guardlist::GuardInfo>>,
    /// **R0-2（内容装载红名单）**：`Envir/MiniMap.txt` 解析出的地图名→小地图索引表
    /// （主线程装载，`game_svr::contentload::load_minimap_file`）。消费点（`CM_WANTMINIMAP`
    /// → `SM_READMINIMAP_OK/FAIL` + `Envirnoment.MinMap`）属 R1 入站红名单，未实现 ⇒ 暂只登记。
    minimap: Option<mir2_envir::minimap::MiniMapRegistry>,
    /// **R0-3**：`Envir/MakeItem.txt` 解析出的合成配方表（`load_make_item_file`）。消费点
    /// （`CM_USERMAKEDRUGITEM` → `Merchant.ClientMakeDrugCheckNeedItem`）属 R1 ⇒ 暂只登记。
    make_items: Option<mir2_envir::makeitem::MakeItemRegistry>,
    /// **R0-4**：`Notice/LineNotice.txt` 解析出的逐行滚动公告（`notice::load_line_notice`）。
    /// 消费点（`WorldServer.cs:669-696` 的逐玩家定时公告）涉及未实现的逐 tick 公告帧 ⇒ 暂只登记。
    line_notice: Vec<String>,
    /// **固定管理 NPC 加载半（R0-1 `openmir2-npcs-fixed-loading`）**：`Envir/Npcs.txt` 解析出的
    /// 记录（主线程装载，`game_svr::npcs::load_npcs_file`）；`None` = 无文件 / 读失败 ⇒ 不放置。
    /// 建实体（ctor 抽数）+ 挂图在 sim 线程起世界后（`game_svr::npcs::{build_npc_spawns,place_npcs}`）。
    npcs: Option<Vec<mir2_envir::npcs::NpcInfo>>,
    /// **M3⑤-实现 2 第①片**：`Envir/Merchant.txt` 的宿主装载器（O-1 FindMap 门 + O-2 `rename_rand`
    /// 要用**世界同一条** `world.rng`，故装载放在 `sim_loop` 起世界之后，而不是主线程侧）。
    loader: Option<mir2_envir::MapLoader>,
    /// **M3⑥-实现 3 第②片**：`[Setup] ScriptGotoCountLimit`（缺省 30；见 [`config::GateConfig`]）——
    /// `NpcHost` 装载时注入 `ScriptNpc::script_goto_count_limit`（`ScriptEngine.cs:629`）。
    script_goto_count_limit: i32,
    /// **M3⑦-实现 3**：交易状态机的 `[Setup]` 配置（`TryDealTime`/`DealOKTime`/
    /// `CanNotGetBackDeal`/`DisableDeal` + `HumanMaxGold` + 颜色）。
    trade: game_svr::trade::DealConfig,
    /// **M3⑩-实现 1**：元宝寄售状态机的配置（`DecUserGameGold` + `MaxBagItem` + 蓝色提示色）。
    sell_off: game_svr::selloff::SellOffConfig,
    /// **M3⑩-实现 2**：`goldsales` 只读加载的启动种子（`MySqlDB.LoadSellOffItemList`，
    /// `src/GameSrv/GameApp.cs:181-182` 调用）。因读错列名 ⇒ 恒 0 行（`D-5`/`B-5`），
    /// 但接线与顺序（`M2Share.SellOffItemList.Add`）照 oracle；sim 线程用它初始化
    /// [`game_svr::selloff::SellOffBook`]。
    sell_off_seed: Vec<game_svr::selloff::DealOffInfo>,
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
    displays: &mut downlink::DisplayTable,
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
            displays,
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

#[allow(clippy::too_many_arguments)]
fn sim_loop(
    map_sizes: Vec<(i32, i32)>,
    registry: Option<mir2_envir::MapRegistry>,
    mut enter_cfg: EnterConfig,
    notice: Vec<u8>,
    colors: MsgColors,
    party_cfg: mir2_world::PartyConfig,
    ev_rx: std::sync::mpsc::Receiver<GateEvent>,
    mut db: Option<DbWiring>,
) {
    // 尺寸表已由调用方按「注册表优先，无注册表则 WorldMaps 占位（且非空，已 fail-closed）」算好；
    // `map_id` = 注册表下标，与 `resolve_map_id` 同源。
    let mut world = mir2_world::World::with_maps(map_sizes);
    // **M3 `openmir2-m3-clock-injection`（Rust 半）**：注入启用时把 `world.clock_ms` 的起点设成
    // 与 `now_ms()` **同一个** origin（此后仍按 oracle 的 +200ms/tick 推进）⇒ 两侧同起点。
    // 未注入 ⇒ 保持 `World::with_maps` 的 `clock_ms: 0`（行为一字不改）。
    if let Some(origin) = clock::injected_origin() {
        world.clock_ms = origin;
        glog!(
            "INF",
            "{} world_clock_ms={}",
            clock::probe_line(origin),
            world.clock_ms
        );
    }
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
    // **M3⑦-实现 2 F-1**：把与 4 条 `CM_*` 组队派发同一份配置灌进世界，供 `settle_deaths`
    // 复刻 oracle `Die()` 的 `groupOwnerPlay.DelMember(this)`（`PlayObject.cs:1975-1977`）。
    world.party_cfg = party_cfg.clone();
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
    // `BaseObject.Run` 恢复段的到点门（`SystemShare.Config.HealthFillTime/SpellFillTime`，
    // `GameSvrConf.cs:1323-1324` 缺省 300/800；与玩家周期的同名配置同源）。
    world.set_recovery_fill_times(enter_cfg.health_fill_time, enter_cfg.spell_fill_time);
    glog!(
        "INF",
        "怪物恢复节拍已灌入 HealthFillTime={} SpellFillTime={}",
        enter_cfg.health_fill_time,
        enter_cfg.spell_fill_time
    );
    // **M3⑤-实现 2 第②片**：生产宿主启用商人 tick（oracle `MerchantProcessor` 的 200ms 线程
    // 单线程等价物；owner `eb3bf42f` 裁决：sim 时钟 200ms 节奏 + `RunTick/RunTime` 门 +
    // 丢墙钟预算）。`World::tick` 在怪物 AI 之后、AOI 刷新之前驱动 `merchant_tick`。
    // 合成 golden 场景不经本函数 ⇒ 仍缺省关闭，逐字节不变。
    world.enable_merchant_tick();
    glog!(
        "INF",
        "商人 tick 已启用（生产宿主）EnableMerchantTick=true（200ms sim 节奏；whitelist T-42 登记丢 NpcLimit 墙钟预算）"
    );
    // **M3⑨ 主人死/幽灵自投递**（卡 `openmir2-m3-pet-slave-masterdie-wire`）：生产宿主启用
    // 「自身延时消息」驱动并灌入宝宝配置（oracle `PlayObject.Die()` 尾
    // `SendSelfDelayMsg(RM_MASTERDIEMUTINY, …, 1000)`（`PlayObject.cs:1996`）→ 1s 后
    // `Operate`（`PlayObject.Message.cs:2116`）→ `ProcessSlaveMutiny`）。`World::tick` 在死亡
    // 结算之后、宝宝清表之前投递到点消息（与 oracle `PlayObject.Run` 消息段先于 `base.Run` 同形）。
    // 合成 golden 场景不经本函数 ⇒ 仍缺省关闭，逐字节不变。
    world.enable_slave_self_delay(enter_cfg.pet_cfg.clone());
    glog!(
        "INF",
        "主人死自投递已启用（生产宿主）EnableSlaveSelfDelay=true（1000ms sim 延迟；RM_MASTERDIEGHOST 恒不可达 = BUG-REPLICA）"
    );
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
    // **M3⑤-实现 2 第①片**：`Envir/Merchant.txt` 装载 + O-1 FindMap 门 + O-2 `rename_rand`
    // （`LocalDB.cs:412` / `WorldServer.Merchant.cs:8-25`）。装载在起世界之后 ⇒ `rename_rand`
    // 走**同一条** `world.rng`（oracle `SystemShare.RandomNumber`）。`loader=None`（无 Envir）
    // 或 `registry=None` 时跳过（无逻辑名→map_id 映射 ⇒ 全会被 FindMap 门丢掉，故不载）。
    // **M3⑥-实现 3 第②片**：隐藏任务 NPC 脚本宿主（`QManage`/`QFunction`/`RobotManage`）。
    let mut npc_host: Option<game_svr::npc::NpcHost> = None;
    if let (Some(loader), Some(reg)) = (enter_cfg.loader.take(), registry.as_ref()) {
        match game_svr::merchant::load_merchant_spawns(&loader, |n| reg.id_of(n), &mut world.rng) {
            Ok((spawns, report)) => {
                let (kept, failed) = game_svr::merchant::spawn_merchants(&mut world, spawns);
                glog!(
                    "INF",
                    "Envir/Merchant.txt 已载入：{} 条（FindMap 门丢 {} 条，挂图失败 {} 条）⇒ 世界内商人 {}",
                    report.records,
                    report.dropped_unknown_map.len(),
                    failed.len(),
                    kept
                );
                for (name, map) in &report.dropped_unknown_map {
                    glog!(
                        "WRN",
                        "商人 {name} 的 MapName={map} 不在注册表 ⇒ 按 oracle FindMap 门丢弃"
                    );
                }
            }
            Err(e) => glog!("WRN", "加载 Envir/Merchant.txt 失败（{e}）⇒ 无商人（登记）"),
        }
        // **固定管理 NPC 放置半（R0-1 `openmir2-npcs-fixed-loading`）**：`WorldServer.Merchant.cs`
        // 的 `NpCinitialize`（`:37-63`，FindMap 门 + `Initialize()`/AddToMap + 失败移除）。
        // 建实体（`BaseObject` ctor 抽数 `Random(1500)`+`Random(2000)`）在商人之后、守卫之前，
        // 与 oracle `GameApp.cs:226-244`（LoadMerchant → LoadNpcs → … → LoadGuardList）同序；
        // 挂图（RNG-free）紧随其后。走**同一条** `world.rng`。
        if let Some(npcs) = enter_cfg.npcs.take() {
            let (spawns, dropped) = game_svr::npcs::build_npc_spawns(&npcs, |n| reg.id_of(n));
            let (kept, failed) = game_svr::npcs::place_npcs(&mut world, spawns);
            glog!(
                "INF",
                "Envir/Npcs.txt 放置完成：{} 条配置（FindMap 门丢 {}，挂图失败 {}）⇒ 世界内固定 NPC {}",
                npcs.len(),
                dropped.len(),
                failed.len(),
                kept
            );
            for (name, map) in &dropped {
                glog!(
                    "WRN",
                    "固定 NPC {name} 的 MapName={map} 不在注册表 ⇒ 按 oracle FindMap 门丢弃"
                );
            }
        }
        // **M3⑥-实现 3 第②片**：隐藏任务 NPC 脚本宿主（`LocalDB.LoadMapQuest` 的宿主侧消费，
        // `mapquest.rs:403-405` 的 `QMangeNpc()/QFunctionNpc()/RobotNpc()`）。`MapQuest.txt`
        // 缺文件也算成功（空 `QuestList`，`result=1`）——此时仍会建 `QManage`/`QFunction`/
        // `RobotManage` 三个隐藏 NPC（目录存在时）。`rename_rand` 走**同一条** `world.rng`
        // （oracle `ScriptParsers.cs:857` 的 `SystemShare.RandomNumber.GetRandomNumber(1,200)`）。
        let envir_root = loader.base_path.join(&loader.envir_dir);
        match loader.load_map_quest(|name| reg.id_of(name).is_some()) {
            Ok(load) => {
                let mut rename_rand = |min: i32, max: i32| world.rng.get_random_number(min, max);
                let host = game_svr::npc::NpcHost::load(
                    &envir_root,
                    &load.quest_npcs,
                    &load.merchants,
                    enter_cfg.script_goto_count_limit,
                    &mut rename_rand,
                );
                glog!(
                    "INF",
                    "MapQuest 隐藏 NPC 宿主就绪：quest_npcs={} merchants={} 装载失败={} ManageNPC={} FunctionNPC={} goto_limit={}",
                    load.quest_npcs.len(),
                    load.merchants.len(),
                    host.failures.len(),
                    host.has_manage_npc(),
                    host.has_function_npc(),
                    enter_cfg.script_goto_count_limit
                );
                for f in &host.failures {
                    glog!(
                        "WRN",
                        "NPC 脚本装载失败 ChrName={} file={}/{}.txt：{}",
                        f.chr_name,
                        f.file_path,
                        f.script_name,
                        f.reason
                    );
                }
                npc_host = Some(host);
            }
            Err(e) => glog!(
                "WRN",
                "加载 Envir/MapQuest.txt 失败（{e}）⇒ 无 NPC 脚本宿主（登记）"
            ),
        }
    }
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
    // **R0-2/R0-3/R0-4 内容装载红名单**：主线程装载的注册表随 `EnterConfig` 到 sim 线程。
    // 三者的消费点（MiniMap 下发帧 / 制药合成 / 滚动公告）都属 R1 入站红名单、未实现
    // ⇒ 此处只「取用 + 登记」（保留读数，不静默丢字段）。
    if let Some(mm) = enter_cfg.minimap.take() {
        glog!(
            "INF",
            "MiniMap 注册表就绪：{} 条（下发帧 CM_WANTMINIMAP 未实现 ⇒ 登记）",
            mm.len()
        );
    }
    if let Some(mi) = enter_cfg.make_items.take() {
        glog!(
            "INF",
            "MakeItem 注册表就绪：{} 组（CM_USERMAKEDRUGITEM 未实现 ⇒ 登记）",
            mi.len()
        );
    }
    {
        let line_notice = std::mem::take(&mut enter_cfg.line_notice);
        if !line_notice.is_empty() {
            glog!(
                "INF",
                "LineNotice 注册表就绪：{} 行（滚动公告未实现 ⇒ 登记）",
                line_notice.len()
            );
        }
    }
    // **GuardList 放置半（片71）**：`LocalDB.cs:113-135` 的放置循环（逐行
    // `RegenMonsterByName` + `:133` 置 `Dir`）。前置 = `MonGenHost::start` 已按 oracle
    // `InitializeMonster`（`GameApp.cs:150`）填齐 `MonsterThreadMap`；放置在商人/NPC 装载
    // 之后、首个 tick 之前 —— 与 oracle `InitializeWorld` 的
    // 「LoadMerchant → LoadNpcs → … → LoadGuardList → WorldEngine.Initialize」次序一致
    // （`GameApp.cs:226-244`），取数走 `world.rng` 同一条流。
    if let (Some(guards), Some(reg)) = (enter_cfg.guards.take(), registry.as_ref()) {
        match mon_gen_host.as_mut() {
            Some(host) => {
                let report = game_svr::guardlist::place_guards(
                    &guards,
                    &enter_cfg.mon_gen.table,
                    host,
                    reg,
                    &mut world,
                    &mut displays,
                );
                glog!(
                    "INF",
                    "守卫列表放置完成：{} 条配置，世界内守卫 {}，跳过 {}（未知地图/未知模板名/移位失败 ⇒ oracle null 支）",
                    report.lines,
                    report.placed,
                    report.skipped
                );
            }
            None => glog!(
                "WRN",
                "无 MonGen 刷怪宿主（无 Envir/MonGen.txt）⇒ 跳过 {} 条守卫放置（登记：oracle InitializeMonster 恒建线程表，部署件必有 MonGen.txt）",
                guards.len()
            ),
        }
    }
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
    // 商店会话态（`PlayObject.ScriptLable`）：`CM_MERCHANTDLGSELECT` 写入、断线/小退移除。
    let mut shop_sessions: std::collections::HashMap<String, ShopSession> =
        std::collections::HashMap::new();
    // `M3⑦-实现 3`：交易会话态（按 `ChrName` 键；与 `ground_sessions` 同生命周期，
    // 交易栏 `DealItemList`/`DealGolds` 从这里读写；掉线/每 tick 取消见 [`deal_cancel_between`]。
    let mut trade_sessions: std::collections::HashMap<String, game_svr::trade::DealState> =
        std::collections::HashMap::new();
    // `M3⑩-实现 1`：元宝寄售——`M2Share.SellOffItemList`（进程级全局列表，所有会话共享）
    // + 每个在线角色的 `SaleDeal`/`SellOffConfirm`/`SellOffItemList`（后二者又随断线一并移除）。
    let mut sell_off_book = game_svr::selloff::SellOffBook {
        entries: std::mem::take(&mut enter_cfg.sell_off_seed),
    };
    let mut sell_off_sessions: std::collections::HashMap<String, game_svr::selloff::SellOffState> =
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
    // AOI 出现帧累计读数（按 `chr_name`）：root 2026-10-14 要求诊断行同时打印「可见集大小与下发帧数」。
    // 只读观测：不参与任何模拟/出帧语义。
    let mut aoi_frames_total: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
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
                            // `PlayObject.Dealing`/`DealLastTick`（M3⑦-实现 3/4）：整表传入，
                            // 取值在 `handle_ground_cmd` 内完成（被 `drop_blocked_by_fresh_deal_last_tick` 咬住）。
                            &trade_sessions,
                            &world,
                            &mut ground_sessions,
                            &online,
                            &gates,
                            &enter_cfg.ground,
                            &colors,
                            &enter_cfg.catalog,
                            registry.as_ref(),
                        );
                    } else if matches!(
                        cmd.ident,
                        mir2_protocol::messages::CM_DEALTRY
                            | mir2_protocol::messages::CM_DEALADDITEM
                            | mir2_protocol::messages::CM_DEALDELITEM
                            | mir2_protocol::messages::CM_DEALCANCEL
                            | mir2_protocol::messages::CM_DEALCHGGOLD
                            | mir2_protocol::messages::CM_DEALEND
                    ) {
                        // `openmir2-m3-party-trade-state`：`CM_DEAL*` 派发
                        // （oracle `PlayObject.Message.cs:946-960`）。
                        handle_deal_cmd(
                            cmd.ident,
                            cmd.recog,
                            tail.as_deref(),
                            &chr_name,
                            socket,
                            &online,
                            &world,
                            &displays,
                            &mut ground_sessions,
                            &mut trade_sessions,
                            &gates,
                            &enter_cfg.trade,
                            &enter_cfg.catalog,
                        );
                    } else if matches!(
                        cmd.ident,
                        mir2_protocol::messages::CM_SELLOFFADDITEM
                            | mir2_protocol::messages::CM_SELLOFFDELITEM
                            | mir2_protocol::messages::CM_SELLOFFCANCEL
                            | mir2_protocol::messages::CM_SELLOFFEND
                            | mir2_protocol::messages::CM_CANCELSELLOFFITEMING
                            | mir2_protocol::messages::CM_SELLOFFBUYCANCEL
                            | mir2_protocol::messages::CM_SELLOFFBUY
                    ) {
                        // `openmir2-m3-selloff-consignment-world`：7 条元宝寄售 `CM_*` 派发
                        // （oracle `PlayObject.Message.cs:2057-2077`）。
                        handle_selloff_cmd(
                            cmd.ident,
                            cmd.recog,
                            cmd.param,
                            cmd.tag,
                            tail.as_deref(),
                            &chr_name,
                            socket,
                            &online,
                            &world,
                            &mut ground_sessions,
                            &mut sell_off_sessions,
                            &mut sell_off_book,
                            &gates,
                            &enter_cfg.sell_off,
                            &enter_cfg.catalog,
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
                            // `PlayObject.Dealing`（M3⑦-实现 4）：交易中禁穿装/卸装。
                            trade_sessions.get(&chr_name).is_some_and(|s| s.dealing),
                            // 52 的尾布局与入图/升级同源（`ability_tail`）。
                            mir2_client_ability_layout(),
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
                        game_svr::shop::CM_MERCHANTDLGSELECT
                            | game_svr::shop::CM_MERCHANTQUERYSELLPRICE
                            | game_svr::shop::CM_USERSELLITEM
                            | game_svr::shop::CM_USERBUYITEM
                            | game_svr::shop::CM_USERGETDETAILITEM
                            | game_svr::shop::CM_USERREPAIRITEM
                            | game_svr::shop::CM_MERCHANTQUERYREPAIRCOST
                    ) {
                        // M3⑤-实现 4：商店 `CM_*`（对话/售价/卖出/买入/详情/查修理价/修理）。
                        // oracle `PlayObject.Message.cs:888-942`。
                        handle_shop_cmd(
                            cmd.ident,
                            cmd.recog,
                            cmd.param,
                            cmd.tag,
                            tail.as_deref(),
                            &chr_name,
                            socket,
                            &online,
                            &mut ground_sessions,
                            &mut shop_sessions,
                            &mut sell_off_sessions,
                            &mut sell_off_book,
                            &trade_sessions,
                            &gates,
                            &mut world,
                            &enter_cfg.catalog,
                            &enter_cfg.shop,
                            &colors,
                        );
                    } else if matches!(
                        cmd.ident,
                        game_svr::shop::CM_USERSTORAGEITEM
                            | game_svr::shop::CM_USERTAKEBACKSTORAGEITEM
                    ) {
                        // M3⑤-实现 4 第②片：仓库两条（存/取回）。oracle `PlayObject.Message.cs:963-968`
                        // → `PlayObject.Operate.cs:1607/1656`。
                        handle_storage_cmd(
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
                            &mut world,
                            &enter_cfg.catalog,
                            &enter_cfg.shop,
                            &colors,
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
                    } else if matches!(
                        cmd.ident,
                        mir2_protocol::messages::CM_GROUPMODE
                            | mir2_protocol::messages::CM_CREATEGROUP
                            | mir2_protocol::messages::CM_ADDGROUPMEMBER
                            | mir2_protocol::messages::CM_DELGROUPMEMBER
                    ) {
                        // `openmir2-m3-party-group-state`：4 条组队 `CM_*` 派发
                        // （oracle `PlayObject.Message.cs:914-935`）。
                        handle_party_cmd(
                            cmd.ident,
                            cmd.param,
                            tail.as_deref(),
                            &chr_name,
                            socket,
                            &online,
                            &mut world,
                            &gates,
                            &party_cfg,
                            &mut saves,
                            &mut ground_sessions,
                            npc_host.as_ref(),
                            &enter_cfg.catalog,
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
                        // 同片 + ⑨-2 第②片：`CM_SPELL(3017)` 派发（oracle `WorldServer.cs:783-790`）。
                        handle_spell_cmd(
                            cmd.recog,
                            cmd.tag,
                            cmd.param,
                            cmd.series,
                            &chr_name,
                            socket,
                            &online,
                            &mut world,
                            &mut saves,
                            &mut ground_sessions,
                            &enter_cfg.catalog,
                            &enter_cfg.magic_table,
                            &enter_cfg.pet_cfg,
                            enter_cfg.magic_attack_rage,
                            registry.as_ref(),
                            &enter_cfg.mon_gen.table,
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
                        // M3⑦-实现 3：断线中断交易（oracle `DealCancelA`，`WorldServer.cs:712`）——
                        // 须在 `online_take`/存档之前，交易栏物品退回本侧背包后再存档。
                        deal_cancel_between(
                            &chr_name,
                            &online,
                            &world,
                            &displays,
                            &mut ground_sessions,
                            &mut trade_sessions,
                            &gates,
                            &enter_cfg.trade,
                            &enter_cfg.catalog,
                        );
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
                        // 商店会话态（`ScriptLable`）随之移除。
                        shop_sessions.remove(&chr_name);
                        // 元宝寄售会话态（`SellOffConfirm`/`SellOffItemList`）随之移除；
                        // `M2Share.SellOffItemList`（进程级）保留（oracle 不断线清理）。
                        sell_off_sessions.remove(&chr_name);
                        // 同屏展示表随实体一并摘除（ActorId 槽位会被内核回收复用，避免旧展示值残留）。
                        if let Some(aid) = world.session(&chr_name).and_then(|s| s.actor_id) {
                            displays.remove(aid);
                            // `openmir2-m3-party-group-state`：oracle `Disappear()`
                            // （`PlayObject.Message.cs:2138-2142`）在 `MakeGhost` 里调
                            // `groupOwnerPlay.DelMember(this)`（离线即退组 / 团长离线整组散）。
                            // 断线者已从 `online` 摘除 ⇒ 它自己的回帧按 oracle `OffLineFlag`
                            // 丢弃；其余成员正常收帧。
                            let owner = world.entities.get(aid).map(|e| e.group_owner).unwrap_or(0);
                            if owner != 0 {
                                for ev in world.party_del_member(owner, aid, &party_cfg) {
                                    let (to, body) = partywire::event_body(&ev)
                                        .expect("断线退组路径不含 GotoLabel（无脚本触发）");
                                    let Some(t) = online.iter().find(|e| e.actor_id == to) else {
                                        continue;
                                    };
                                    let Some(tx) =
                                        route_to_gate(&gates, t.gate_idx, t.socket, "断线组队清组")
                                    else {
                                        continue;
                                    };
                                    let _ = tx.send(SimToGate::SendFrame {
                                        socket: t.socket,
                                        body,
                                    });
                                }
                            }
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
                                        let ent = new_player_entity(
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
                                                            // 帧 52 尾的冻结客户端 58B `TAbility` 布局；
                                                            // 默认开（未设 = client-58；`MIR2_CLIENT_ABILITY_LAYOUT=0`
                                                            // 才 oracle 逐字节，见该字段文档与
                                                            // `openmir2-client-e2e-enter-gaps` 的 owner 裁决）。
                                                            client_ability_layout:
                                                                mir2_client_ability_layout(),
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
                                                    // 进图可观测性（`openmir2-client-e2e-enter-gaps`）：把本会话**实际写入
                                                    // 52/752 帧的输入值**与**实际尾字节**（两种布局之一，见
                                                    // `MIR2_CLIENT_ABILITY_LAYOUT`）打进日志，便于
                                                    // “客户端血条/负重为 0 是服务端没发/发错，还是客户端解析”一类的真机定位——
                                                    // 只读日志，不改任何模拟/出帧语义。
                                                    log_enter_ability(&head.chr_name, &out);
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
                                                            // `WorldServer.cs:1213-1227`：读档进仓库会话态。
                                                            storage: storage::load_from_record(
                                                                d.storage_items.as_deref(),
                                                            ),
                                                            actor_id: id,
                                                            game_gold: d.game_gold,
                                                            // `PlayObject.SoftVersionDateEx`（= `LoadDBInfo.SoftVersionDate`）：
                                                            // `SendGoldInfo` 的 `== 0 ⇒ 不发` 门（`PlayObject.Base.cs:930`）。
                                                            soft_version_date,
                                                            // `BaseObject.GamePoint`（`SM_GAMEGOLDNAME` 的 LoWord/HiWord）。
                                                            game_point: data.map_or(0, |d| d.game_point),
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
                                                                // `PlayObject.Initialize()` 的 `AddBodyLuck(0)`
                                                                // （`PlayObject.cs:1060`）：从落库的 `BodyLuck` 重算 `BodyLuckLevel`。
                                                                body_luck: d.body_luck,
                                                                body_luck_level: body_luck_level_of(d.body_luck),
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
                                                    // **M3⑥-实现 3 第②片**：进世界完成点 oracle
                                                    // `PlayObject.cs:1440` 的 `if (SystemShare.ManageNPC != null)
                                                    // SystemShare.ManageNPC.GotoLable(this, "@Login", false)`。
                                                    // 引擎产出的说词经 oracle `MsgQueue` 出队序落
                                                    // `SM_MERCHANTSAY(643)`（`PlayObject.Message.cs:1722`）。
                                                    // 脚本取数走**同一条** `world.rng`（临时移出后归位，
                                                    // 与 `step_mon_gen` 同法）。
                                                    if let Some(host) = npc_host.as_ref() {
                                                        if host.has_manage_npc() {
                                                            let mut rng = std::mem::replace(
                                                                &mut world.rng,
                                                                mir2_shared::rng::RandomNumber::with_seed(0),
                                                            );
                                                            {
                                                                let mut rngf =
                                                                    |v: i32| rng.random_below(v);
                                                                let level = data
                                                                    .and_then(|d| d.abil.as_ref())
                                                                    .map_or(1, |a| i32::from(a.level));
                                                                // §1.O `recall-ext` 读侧：同源自 `World` 的组队快照。
                                                                let (group_owner, group_member_jobs) =
                                                                    world
                                                                        .party_script_group_snapshot(
                                                                            id,
                                                                        );
                                                                let mut player = npc_player_from(
                                                                    &mut rngf,
                                                                    &head.chr_name,
                                                                    id,
                                                                    &final_map_name,
                                                                    level,
                                                                    data.map_or(0, |d| d.gold),
                                                                    data.and_then(|d| {
                                                                        d.bag_items.as_deref()
                                                                    }),
                                                                    data.and_then(|d| {
                                                                        d.quest_flag.as_deref()
                                                                    }),
                                                                    &enter_cfg.catalog,
                                                                    group_owner,
                                                                    &group_member_jobs,
                                                                );
                                                                if let Some(says) =
                                                                    host.run_login(&mut player)
                                                                {
                                                                    let sent = emit_npc_says(
                                                                        tx, socket, id, &says,
                                                                    );
                                                                    glog!(
                                                                        "INF",
                                                                        "@Login 已执行 ChrName={} 说词 {} 条（下发 {sent}）",
                                                                        head.chr_name,
                                                                        says.len()
                                                                    );
                                                                }
                                                                // 回写（本片只回写 `Gold` 与 `QuestFlag`；
                                                                // `QuestUnit/QuestUnitOpen/ItemList/LastNpc/Script`
                                                                // 的写回登记为后续分片，见 patch body）。
                                                                if let Some(s) =
                                                                    saves.get_mut(&head.chr_name)
                                                                {
                                                                    s.record.data.gold = player.gold;
                                                                    if let Some(qf) = s
                                                                        .record
                                                                        .data
                                                                        .quest_flag
                                                                        .as_mut()
                                                                    {
                                                                        let n = qf
                                                                            .len()
                                                                            .min(player.quest_flag.as_bytes().len());
                                                                        qf[..n].copy_from_slice(
                                                                            &player.quest_flag.as_bytes()[..n],
                                                                        );
                                                                    }
                                                                }
                                                                if let Some(sg) =
                                                                    ground_sessions
                                                                        .get_mut(&head.chr_name)
                                                                {
                                                                    sg.state.gold = player.gold;
                                                                }
                                                            }
                                                            world.rng = rng;
                                                        }
                                                    }
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
        //
        // M3⑦-实现 3：掉线/顶号中断交易（oracle `DealCancelA`，`WorldServer.cs:712`）——
        // 必须在 `drain_*` 移除会话/存档之前跑，否则交易栏物品会随会话丢弃。
        for chr in kick_pending.clone() {
            deal_cancel_between(
                &chr,
                &online,
                &world,
                &displays,
                &mut ground_sessions,
                &mut trade_sessions,
                &gates,
                &enter_cfg.trade,
                &enter_cfg.catalog,
            );
        }
        for chr in soft_close_pending.clone() {
            deal_cancel_between(
                &chr,
                &online,
                &world,
                &displays,
                &mut ground_sessions,
                &mut trade_sessions,
                &gates,
                &enter_cfg.trade,
                &enter_cfg.catalog,
            );
        }
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
                    &mut displays,
                    i32::try_from(online.len()).unwrap_or(i32::MAX),
                );
            }
        }
        let rec = world.tick();
        // **M3⑦-实现 2 F-1**：死亡退组（oracle `Die()`→`DelMember`）帧——在本 tick 的
        // `settle_deaths` 里已按 oracle 序算好事件，这里只负责回帧（死亡者仍在线，自身帧照发）。
        // 放在 `party_run_tick()` 之前：oracle `Run()` 的组队段（`PlayObject.Message.cs:704-719`）
        // 晚于 `Die()`，而 `Die()` 已把死亡成员摘出（段内无死成员可清）。
        drain_party_death_events(&rec, &online, &gates);
        // **M3⑨-3 帧面残余**：本 tick 的 `World::tick` 已在消息段投递到点自投递消息
        // （`RM_MASTERDIEMUTINY` → `ProcessSlaveMutiny`，`world.rs:565-572`）并落
        // `rec.slave_self_deliveries`；这里补出帧（`RefNameColor`/`RefShowName`）。
        // 位置：oracle 属主人 `Run()` 的消息段（`PlayObject.Message.cs:2116-2121`），先于
        // `base.Run()` 的恢复/体力段 ⇒ 放在 `drain_recovery`/`drain_run_downlinks` 之前。
        let _mutiny_frames = drain_slave_mutiny_downlink(
            &rec,
            &world,
            &online,
            &gates,
            &enter_cfg.pet_cfg,
            enter_cfg.ref_msg_range,
        );
        // `openmir2-m3-party-group-state`：`PlayObject.Run()` 的组队段（`PlayObject.Message.cs:704-719`）
        // 在同一 tick 的恢复段之前（`base.Run()` 在 `:765`，晚于组队段）。
        world.party_run_tick();
        ticks += 1;
        // `PlayObject.Run()` 每 tick 的交易自动取消（`PlayObject.Message.cs:30-36`）——
        // `Dealing` 但 `GetPoseCreate() != DealCreat` / `DealCreat == this` / `DealCreat == null`。
        drain_deal_tick(
            &world,
            &displays,
            &online,
            &mut ground_sessions,
            &mut trade_sessions,
            &gates,
            &enter_cfg.trade,
            &enter_cfg.catalog,
        );
        // T-28：周期恢复 tick（oracle `PlayObject.Run` 的 `:70-141`）——同一 world tick 后处理，
        // 把 `IncHealth/IncSpell/IncHealing` 按 oracle 节奏消费回 `WAbil.HP/MP` 并回帧。
        // F-1：本 tick 进升级块的获得者集合 ⇒ 其恢复段的 53 被升级段的 53 取代（`SendUpdateMsg` 去重）。
        let leveled_this_tick: std::collections::HashSet<i32> = rec
            .exp_awards
            .iter()
            .filter(|(_, _, leveled, _)| *leveled)
            .map(|(actor, _, _, _)| *actor)
            .collect();
        drain_recovery(
            &mut ground_sessions,
            &world,
            &online,
            &gates,
            enter_cfg.ref_msg_range,
            &leveled_this_tick,
        );
        // **reviewer-5 O-1 收口**：oracle 一次 `PlayObject.Run()` = 消息段（击杀经验/升级链 +
        // AOI 出现帧）→ `BaseObject.Run()` 的体力/魔力点段；次序由 [`drain_run_downlinks`] 固定。
        // 返回值 = 本 tick 每会话实发的 AOI 出现帧数（只读诊断，累加进 `af=`）。
        let aoi_frames_this_tick = drain_run_downlinks(
            &rec,
            &mut world,
            &displays,
            &online,
            &mut ground_sessions,
            &gates,
            &enter_cfg.catalog,
            enter_cfg.ref_msg_range,
            enter_cfg.health_fill_time,
            enter_cfg.spell_fill_time,
            mir2_client_ability_layout(),
        );
        for (chr, n) in aoi_frames_this_tick {
            *aoi_frames_total.entry(chr).or_insert(0) += n;
        }
        // 周期存档（oracle `WorldServer.cs:698-703` / `ProcessHumans`）：oracle 在
        // `playObject.Run()` **之后**才查 `SaveRcdTick` ⇒ 本段必须在 [`drain_run_downlinks`] 之后，
        // 存档体才读到本 tick 的升级/恢复/体力点后的 `WAbil`（reviewer-5 O-1 收口）。
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
        if ticks.is_multiple_of(50) {
            let diag = world_aoi_diag(&world, &displays, &aoi_frames_total);
            glog!(
                "DBG",
                "sim: tick #{} clock={}ms 处理 {} 项（本地第 {ticks} 次）{}{}",
                rec.tick_seq,
                rec.clock_ms,
                rec.processed.len(),
                diag.summary(),
                diag.session_summary()
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
///
/// **M3 `openmir2-m3-clock-injection`**：设了 `MIR2_CLOCK_MS` ⇒ 走 `clock::now_ms()` 的注入时钟
/// （origin + 单调经过）；未设 ⇒ 与改动前逐字节相同（墙钟）。
fn now_ms() -> i64 {
    clock::now_ms()
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

    /// **M3⑦-3 · M-1 收口判据**：`ClientDealTry` 的目标判定必须区分「无前排对象」与「前排不是玩家」。
    ///
    /// oracle `PlayObject.Operate.cs:932-933` 先取 `GetPoseCreate()` 再直接读 `poseObject.Race`：
    /// 无前排（`null`）⇒ NullReferenceException，被 `WorldServer.ProcessPlayObjectData` 的
    /// `catch`（`WorldServer.cs:732-735`）吞掉 ⇒ **一个帧都不发**（不是 `SM_DEALTRY_FAIL`）；
    /// 前排非玩家 ⇒ `:958-960` 的 `else` ⇒ `SM_DEALTRY_FAIL`。
    ///
    /// 旧实现把两者都归 `DealTryTarget::NotPlayer` ⇒ 空打也回 FAIL（与 oracle 不等）。
    ///
    /// 红检：把 `deal_try_target_of` 的 `None` 支改回 `NotPlayer` ⇒ ① 断言必红。
    #[test]
    fn deal_try_target_distinguishes_absent_from_non_player() {
        use mir2_world::{Entity, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY};

        // `front = Some((race, peer_dir))`：在前方格 (11,10) 放一个该 race/朝向的实体。
        // hero 在 (10,10) 且 dir=2（Right）⇒ 前方格 = (11,10)。
        let probe = |front: Option<(u8, u8)>| -> (Option<i32>, game_svr::trade::DealTryTarget) {
            let mut world = mir2_world::World::new(32, 32);
            let hero = world.add_entity(Entity::new(1, "hero", 10, 10));
            let mut displays = downlink::DisplayTable::new();
            displays.set(
                hero,
                downlink::ActorDisplay::new(2, 0, 0, 0, 0, 255, b"hero".to_vec()),
            );
            if let Some((race, peer_dir)) = front {
                let fid = world.add_entity({
                    let mut e = Entity::new(2, "front", 11, 10);
                    e.race = race;
                    e
                });
                displays.set(
                    fid,
                    downlink::ActorDisplay::new(peer_dir, 0, 0, 0, 0, 255, b"front".to_vec()),
                );
            }
            let pose = pose_create(&world, &displays, hero);
            deal_try_target_of(&world, &displays, hero, pose)
        };

        // ① 前方格为空 ⇒ `GetPoseCreate() == null` ⇒ oracle NRE 吞错 ⇒ 不发任何帧。
        assert_eq!(
            probe(None),
            (None, game_svr::trade::DealTryTarget::Absent),
            "无前排对象 ⇒ Absent（oracle Operate.cs:932-933 NRE 被吞），不是 NotPlayer"
        );

        // ② 前方格是怪（非玩家）⇒ `:958-960` else ⇒ `SM_DEALTRY_FAIL`。
        assert_eq!(
            probe(Some((ACTOR_RACE_MONSTER, 6))),
            (None, game_svr::trade::DealTryTarget::NotPlayer),
            "前排非玩家 ⇒ NotPlayer（SM_DEALTRY_FAIL）"
        );

        // ③ 前方格是玩家且 `dir=6`（Left）⇒ 它的前方格 = (10,10) = hero ⇒ `faces_me`。
        assert_eq!(
            probe(Some((ACTOR_RACE_PLAY, 6))),
            (
                Some(2),
                game_svr::trade::DealTryTarget::Other { faces_me: true }
            ),
            "前排玩家回指 hero ⇒ faces_me=true"
        );

        // ④ 前方格是玩家但 `dir=2`（Right）⇒ 它的前方格 = (12,10) ≠ hero ⇒ `!faces_me`。
        assert_eq!(
            probe(Some((ACTOR_RACE_PLAY, 2))),
            (
                Some(2),
                game_svr::trade::DealTryTarget::Other { faces_me: false }
            ),
            "前排玩家未回指 ⇒ faces_me=false"
        );
    }

    /// **M3⑦-3 · M-1 收口（进程级）**：走**生产函数** `handle_deal_cmd` 的 `CM_DEALTRY` 支
    /// （oracle `PlayObject.Message.cs:946` ⇒ `ClientDealTry`）：无前排对象时**一个帧都不发**
    /// （oracle `Operate.cs:932-933` 的 NRE 被 `WorldServer.cs:732-735` 吞掉），前排为怪时发
    /// `SM_DEALTRY_FAIL`（非空转对照）。
    ///
    /// 红检：把 `deal_try_target_of` 的 `None` 支改回 `DealTryTarget::NotPlayer` ⇒ ① 的
    /// `frames.is_empty()` 必红（实得 1 帧 `SM_DEALTRY_FAIL`）。
    #[test]
    fn deal_cm_try_no_front_object_emits_no_frame_on_production_path() {
        use std::collections::HashMap;

        let mut world = mir2_world::World::new(32, 32);
        world.clock_ms = 100_000;
        let hero = world.add_entity({
            let mut e = mir2_world::Entity::new(1, "hero", 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e
        });
        let mut displays = downlink::DisplayTable::new();
        displays.set(
            hero,
            downlink::ActorDisplay::new(2, 0, 0, 0, 0, 255, b"hero".to_vec()),
        );

        let mut ground_sessions: HashMap<String, SessionGround> = HashMap::new();
        ground_sessions.insert("hero".into(), test_session_ground(hero));
        let mut trade_sessions: HashMap<String, game_svr::trade::DealState> = HashMap::new();
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
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let cfg = game_svr::trade::DealConfig::default();
        let catalog = game_svr::ability::StdItemCatalog::new();

        let drain_idents = |rx: &mut mpsc::UnboundedReceiver<SimToGate>| -> Vec<u16> {
            let mut out = Vec::new();
            while let Ok(m) = rx.try_recv() {
                if let SimToGate::SendFrame { body, .. } = m {
                    out.push(u16::from_le_bytes([body[4], body[5]]));
                }
            }
            out
        };

        // ① 前方格 (11,10) 空 ⇒ `GetPoseCreate() == null` ⇒ oracle NRE 吞错 ⇒ 不发任何帧。
        handle_deal_cmd(
            mir2_protocol::messages::CM_DEALTRY,
            0,
            None,
            "hero",
            901,
            &online,
            &world,
            &displays,
            &mut ground_sessions,
            &mut trade_sessions,
            &gates,
            &cfg,
            &catalog,
        );
        assert!(
            drain_idents(&mut rx0).is_empty(),
            "无前排对象 ⇒ oracle 静默（不得发 SM_DEALTRY_FAIL）"
        );

        // ② 非空转对照：前方格放一只怪（非玩家）⇒ `:958-960` else ⇒ `SM_DEALTRY_FAIL`。
        world.add_entity({
            let mut e = mir2_world::Entity::new(2, "mon", 11, 10);
            e.race = mir2_world::ACTOR_RACE_MONSTER;
            e
        });
        handle_deal_cmd(
            mir2_protocol::messages::CM_DEALTRY,
            0,
            None,
            "hero",
            901,
            &online,
            &world,
            &displays,
            &mut ground_sessions,
            &mut trade_sessions,
            &gates,
            &cfg,
            &catalog,
        );
        assert_eq!(
            drain_idents(&mut rx0),
            vec![mir2_protocol::messages::SM_DEALTRY_FAIL],
            "前置守卫全过 + 前排非玩家 ⇒ 只发 SM_DEALTRY_FAIL"
        );
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

    /// **`openmir2-m3-party-group-state` 进程级判据**：走**生产路径** `handle_party_cmd`，让一条
    /// 连接（gate 0 / socket 5 = L）与另一条（gate 0 / socket 6 = T）完成
    /// `CM_GROUPMODE(T 开组) → CM_CREATEGROUP(L,T) → CM_DELGROUPMEMBER(L,T)` 的派发，断言：
    /// ① 帧按 `[gate_idx, socket]` 路由到正确连接；
    /// ② `SM_GROUPMEMBERS(667)` 尾 = `EncodeString("L/T/")`；
    /// ③ `SM_GROUPDELMEM_OK(663)` 的 `Recog=0`、尾 = `EncodeString("T")`；
    /// ④ `SM_SYSMESSAGE(100)` 用组队颜色（`Param=MakeWord(0xC4,0xFF)`、`series=1`）。
    ///
    /// 红检：把 `sm_groupmembers` 的尾 `edcode::encode` 删掉、或把 `handle_party_cmd` 的帧序
    /// 打乱 ⇒ 本用例必 FAILED。
    #[test]
    fn party_cm_dispatch_emits_oracle_frames_on_both_connections() {
        use mir2_world::{Entity, PartyConfig};
        use std::collections::HashMap;

        fn drain(rx: &mut mpsc::UnboundedReceiver<SimToGate>) -> Vec<Vec<u8>> {
            let mut out = Vec::new();
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    SimToGate::SendFrame { body, .. } => out.push(body),
                    other => panic!("组队路径不得发 SendFrame 以外的命令，实得 {other:?}"),
                }
            }
            out
        }
        fn ident(body: &[u8]) -> u16 {
            u16::from_le_bytes([body[4], body[5]])
        }
        fn recog(body: &[u8]) -> i32 {
            i32::from_le_bytes([body[0], body[1], body[2], body[3]])
        }

        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        let mut world = mir2_world::World::new(32, 32);
        let l = world.add_entity(Entity::new(1, "L", 10, 10));
        let t = world.add_entity(Entity::new(2, "T", 11, 10));

        let online: OnlineRegistry = vec![
            OnlineEntry {
                account: "a".into(),
                chr_name: "L".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 5,
                actor_id: l,
            },
            OnlineEntry {
                account: "b".into(),
                chr_name: "T".into(),
                session_id: 2,
                gate_idx: 0,
                socket: 6,
                actor_id: t,
            },
        ];

        let cfg = PartyConfig::default();

        // ① T 开 AllowGroup（`CM_GROUPMODE` 的 `param != 0`）
        handle_party_cmd(
            mir2_protocol::messages::CM_GROUPMODE,
            1,
            None,
            "T",
            6,
            &online,
            &mut world,
            &gates,
            &cfg,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashMap::new(),
            None,
            &game_svr::ability::StdItemCatalog::new(),
        );
        let t_mode = drain(&mut rx0);
        assert_eq!(t_mode.len(), 1, "T 只收到一条 SM_GROUPMODECHANGED");
        assert_eq!(
            ident(&t_mode[0]),
            mir2_protocol::messages::SM_GROUPMODECHANGED
        );
        assert_eq!(u16::from_le_bytes([t_mode[0][6], t_mode[0][7]]), 1);

        // ② L 建组（T 已在 AllowGroup）
        handle_party_cmd(
            mir2_protocol::messages::CM_CREATEGROUP,
            0,
            Some("T"),
            "L",
            5,
            &online,
            &mut world,
            &gates,
            &cfg,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashMap::new(),
            None,
            &game_svr::ability::StdItemCatalog::new(),
        );
        let frames = drain(&mut rx0);
        // 事件表的**全局次序**（两 socket 共用一条 channel）：立即帧先、排队帧后；
        // 667 两条（L/T 各一），join 说词四条（join(L) 给 L/T，join(T) 给 L/T）。
        let idents: Vec<u16> = frames.iter().map(|b| ident(b)).collect();
        assert_eq!(
            idents,
            vec![
                mir2_protocol::messages::SM_CREATEGROUP_OK,
                mir2_protocol::messages::SM_GROUPMEMBERS,
                mir2_protocol::messages::SM_GROUPMEMBERS,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
            ]
        );
        // 667 尾 = EncodeString("L/T/")
        let gm = frames
            .iter()
            .find(|b| ident(b) == mir2_protocol::messages::SM_GROUPMEMBERS)
            .unwrap();
        assert_eq!(
            mir2_protocol::edcode::decode(&gm[12..]),
            gbk_encode("L/T/"),
            "SM_GROUPMEMBERS 尾必须是 EncodeString(\"L/T/\")"
        );
        // 100 的颜色：Param = MakeWord(0xC4, 0xFF) = 0xFFC4，series=1
        let sm = frames
            .iter()
            .find(|b| ident(b) == mir2_protocol::messages::SM_SYSMESSAGE)
            .unwrap();
        assert_eq!(u16::from_le_bytes([sm[6], sm[7]]), 0xFFC4, "组队色 Param");
        assert_eq!(u16::from_le_bytes([sm[10], sm[11]]), 1, "series=1");

        // ③ L 把 T 移出组
        handle_party_cmd(
            mir2_protocol::messages::CM_DELGROUPMEMBER,
            0,
            Some("T"),
            "L",
            5,
            &online,
            &mut world,
            &gates,
            &cfg,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashMap::new(),
            None,
            &game_svr::ability::StdItemCatalog::new(),
        );
        let frames = drain(&mut rx0);
        // 全局事件次序：GCANCEL(L,立即), DELMEM_OK(L), GCANCEL(T,排队156),
        // SYSMESSAGE(T 退出→L), SYSMESSAGE(T 退出→T), SYSMESSAGE(解散→L)。
        let idents: Vec<u16> = frames.iter().map(|b| ident(b)).collect();
        assert_eq!(
            idents,
            vec![
                mir2_protocol::messages::SM_GROUPCANCEL,
                mir2_protocol::messages::SM_GROUPDELMEM_OK,
                mir2_protocol::messages::SM_GROUPCANCEL,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
            ]
        );
        let delok = &frames[1];
        assert_eq!(recog(delok), 0, "SM_GROUPDELMEM_OK 的 Recog=0");
        assert_eq!(
            mir2_protocol::edcode::decode(&delok[12..]),
            gbk_encode("T"),
            "SM_GROUPDELMEM_OK 尾 = EncodeString(被删者名)"
        );
    }

    /// **`openmir2-m3-party-gotolabel-ext` 进程级判据**：`FunctionNPC.GotoLable` 宿主接线。
    ///
    /// 走**生产路径** `handle_party_cmd`：`CM_CREATEGROUP` 成功 ⇒ 世界侧在 oracle `Operate.cs:843`
    /// 的位置入 `PartyEvent::GotoLabel("@GroupCreate")`（排队桶，优先级字节 1）；宿主 drain 时装载
    /// `QFunction` 并执行脚本，说词经 `SM_MERCHANTSAY(643)` 就地在 `SM_GROUPMEMBERS(667)` 之后、
    /// `RM_GROUPMESSAGE` 的 `SM_SYSMESSAGE(100)` 之前下发（oracle 线上序：立即帧 → 排队帧按优先级升序）。
    ///
    /// 红检（两条）：① 把世界侧 `party_create_group` 的 `em.later(..GotoLabel..)` 删掉 ⇒ 无 643；
    /// ② 把 `handle_party_cmd` 的 `GotoLabel` 分支删掉（或把脚本说词 append 到帧尾）⇒ 643 缺失/位置必红。
    #[test]
    fn party_gotolabel_runs_qfunction_script_at_queued_position() {
        use mir2_script::{SayingProcedure, SayingRecord, ScriptInfo};
        use mir2_world::{Entity, PartyConfig};
        use std::collections::HashMap;

        fn ident(body: &[u8]) -> u16 {
            u16::from_le_bytes([body[4], body[5]])
        }
        fn drain(rx: &mut mpsc::UnboundedReceiver<SimToGate>) -> Vec<Vec<u8>> {
            let mut out = Vec::new();
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    SimToGate::SendFrame { body, .. } => out.push(body),
                    other => panic!("组队路径不得发 SendFrame 以外的命令，实得 {other:?}"),
                }
            }
            out
        }

        // `QFunction`（`SystemShare.FunctionNPC = new Merchant()`，`GameApp.cs:87`）装载一个
        // `@GroupCreate` 说词；`has_function_npc()` 只看 `loaded`。
        let host = game_svr::npc::NpcHost {
            envir_root: std::path::PathBuf::new(),
            defs: vec![game_svr::npc::NpcDef {
                chr_name: "QFunction".into(),
                file_path: "Market_Def".into(),
                script_name: Some("QFunction".into()),
                is_hide: false,
                is_quest: false,
                loaded: true,
                scripts: vec![ScriptInfo {
                    is_quest: false,
                    quest_info: Default::default(),
                    record_list: vec![SayingRecord {
                        s_label: "@GroupCreate".into(),
                        procedure_list: vec![SayingProcedure {
                            condition_list: Vec::new(),
                            action_list: Vec::new(),
                            else_action_list: Vec::new(),
                            s_say_msg: "grp".into(),
                            s_else_say_msg: String::new(),
                        }],
                        bo_ext_jmp: false,
                    }],
                    quest_count: 0,
                }],
            }],
            failures: Vec::new(),
            goto_count_limit: 40,
        };

        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        let mut world = mir2_world::World::new(32, 32);
        let l = world.add_entity(Entity::new(1, "L", 10, 10));
        let t = world.add_entity(Entity::new(2, "T", 11, 10));
        world.entities.get_mut(t).unwrap().allow_group = true;
        let online: OnlineRegistry = vec![
            OnlineEntry {
                account: "a".into(),
                chr_name: "L".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 5,
                actor_id: l,
            },
            OnlineEntry {
                account: "b".into(),
                chr_name: "T".into(),
                session_id: 2,
                gate_idx: 0,
                socket: 6,
                actor_id: t,
            },
        ];
        let cfg = PartyConfig::default();
        let mut saves: HashMap<String, SavedHuman> = HashMap::new();
        saves.insert(
            "L".into(),
            SavedHuman {
                account: "a".into(),
                record: record::PlayerRecord::default(),
                enter_map: "0".into(),
            },
        );
        let mut ground_sessions: HashMap<String, SessionGround> = HashMap::new();
        // 运行时 Gold（`PlayObject.Gold`）在会话态；脚本不读/不改它 ⇒ 回写后应保持。
        // 令它与 `saves.record.data.gold`（默认 0）不同，钉住「运行时优先于读档记录」的取数口径。
        let mut sg_l = test_session_ground(l);
        sg_l.state.gold = 12345;
        ground_sessions.insert("L".into(), sg_l);
        let catalog = game_svr::ability::StdItemCatalog::new();

        let _ = drain(&mut rx0);
        handle_party_cmd(
            mir2_protocol::messages::CM_GROUPMODE,
            1,
            None,
            "T",
            6,
            &online,
            &mut world,
            &gates,
            &cfg,
            &mut saves,
            &mut ground_sessions,
            Some(&host),
            &catalog,
        );
        let _ = drain(&mut rx0);
        handle_party_cmd(
            mir2_protocol::messages::CM_CREATEGROUP,
            0,
            Some("T"),
            "L",
            5,
            &online,
            &mut world,
            &gates,
            &cfg,
            &mut saves,
            &mut ground_sessions,
            Some(&host),
            &catalog,
        );
        let frames = drain(&mut rx0);
        let idents: Vec<u16> = frames.iter().map(|b| ident(b)).collect();
        assert_eq!(
            idents,
            vec![
                mir2_protocol::messages::SM_CREATEGROUP_OK,
                mir2_protocol::messages::SM_GROUPMEMBERS,
                mir2_protocol::messages::SM_GROUPMEMBERS,
                // GotoLable("@GroupCreate") 的脚本说词：`RM_MERCHANTSAY`（优先级字节 1）
                // ⇒ 在 `RM_GROUPMESSAGE(253)` 的组队说词之前。
                mir2_protocol::messages::SM_MERCHANTSAY,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
            ],
            "实际 {idents:?}"
        );
        let say = frames
            .iter()
            .find(|b| ident(b) == mir2_protocol::messages::SM_MERCHANTSAY)
            .unwrap();
        assert_eq!(
            mir2_protocol::edcode::decode(&say[12..]),
            gbk_encode("QFunction/grp"),
            "GotoLable 说词行 = normNpc.ChrName + '/' + sMsg"
        );
        // 取数口径：运行时 `PlayObject.Gold`（ground_sessions）优先于读档记录（saves）；脚本未改 ⇒ 回写后不变。
        assert_eq!(
            ground_sessions.get("L").unwrap().state.gold,
            12345,
            "运行时 Gold 应保持 12345（若误取 saves 的陈旧 0 则此处必红）"
        );
        assert_eq!(
            saves.get("L").unwrap().record.data.gold,
            12345,
            "saves 回写同步为运行时 Gold"
        );
    }

    /// **F-1（reviewer-5 `fc68afca`）进程级判据**：走**生产路径** —— `world.tick()` 在
    /// `settle_deaths`（`hp == 0` 的死亡写入点 = oracle `Die()`）里按 `PlayObject.cs:1975-1977`
    /// 调 `party_del_member`，再由 `drain_party_death_events` 回帧。断言：
    /// ① 死亡成员 `group_owner` 清 0；② 死亡者**仍在线** ⇒ 它自己的 `SM_GROUPCANCEL(666)` 与
    /// 退出说词 `SM_SYSMESSAGE(100)` 照发（不复用断线路径的「已摘 online ⇒ 丢自身帧」口径）；
    /// ③ 团长侧 `SM_GROUPCANCEL(666)` + 解散说词照发。
    ///
    /// 红检：把 `settle_deaths` 里的 `party_del_member` 调用删掉 ⇒ 本用例必 FAILED（无帧 + owner 不清）。
    #[test]
    fn die_triggers_del_member_frames_on_production_path() {
        use mir2_world::{Entity, PartyConfig};
        use std::collections::HashMap;

        fn drain(rx: &mut mpsc::UnboundedReceiver<SimToGate>) -> Vec<(i32, Vec<u8>)> {
            let mut out = Vec::new();
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    SimToGate::SendFrame { socket, body } => out.push((socket, body)),
                    other => panic!("死亡退组路径不得发 SendFrame 以外的命令，实得 {other:?}"),
                }
            }
            out
        }
        fn ident(body: &[u8]) -> u16 {
            u16::from_le_bytes([body[4], body[5]])
        }

        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        let mut world = mir2_world::World::new(32, 32);
        let l = world.add_entity(Entity::new(1, "L", 10, 10));
        let t = world.add_entity(Entity::new(2, "T", 11, 10));
        let online: OnlineRegistry = vec![
            OnlineEntry {
                account: "a".into(),
                chr_name: "L".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 5,
                actor_id: l,
            },
            OnlineEntry {
                account: "b".into(),
                chr_name: "T".into(),
                session_id: 2,
                gate_idx: 0,
                socket: 6,
                actor_id: t,
            },
        ];
        let cfg = PartyConfig::default();
        // 生产的 world 由 `sim_loop` 灌入同一份配置；此处照做。
        world.party_cfg = cfg.clone();

        // 生产建组（T 开组 → L 建组），清空中间帧。
        handle_party_cmd(
            mir2_protocol::messages::CM_GROUPMODE,
            1,
            None,
            "T",
            6,
            &online,
            &mut world,
            &gates,
            &cfg,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashMap::new(),
            None,
            &game_svr::ability::StdItemCatalog::new(),
        );
        let _ = drain(&mut rx0);
        handle_party_cmd(
            mir2_protocol::messages::CM_CREATEGROUP,
            0,
            Some("T"),
            "L",
            5,
            &online,
            &mut world,
            &gates,
            &cfg,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashMap::new(),
            None,
            &game_svr::ability::StdItemCatalog::new(),
        );
        let _ = drain(&mut rx0);
        assert_eq!(
            world.entities.get(t).unwrap().group_owner,
            l,
            "建组后 T 在 L 组"
        );

        // 死亡写入点（`hp == 0`）→ `tick()` 的 `settle_deaths` 触发 `Die()` 的 `DelMember`。
        world.entities.get_mut(t).unwrap().hp = 0;
        let rec = world.tick();
        assert_eq!(
            world.entities.get(t).unwrap().group_owner,
            0,
            "Die→DelMember 必须在 tick 内清死亡者 GroupOwner"
        );
        drain_party_death_events(&rec, &online, &gates);

        // 2 人组：T 死 ⇒ `CancelGroup()`（`Count <= 1`）⇒ 组散。
        // 全局序：GCANCEL(L,立即) → GCANCEL(T,排队 156) → SYS(T 退出→L/T,253) → SYS(解散→L,253)。
        let frames = drain(&mut rx0);
        let idents: Vec<u16> = frames.iter().map(|(_, b)| ident(b)).collect();
        assert_eq!(
            idents,
            vec![
                mir2_protocol::messages::SM_GROUPCANCEL,
                mir2_protocol::messages::SM_GROUPCANCEL,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
            ],
            "死亡退组帧序不符（实际 {idents:?}）"
        );
        // 死亡者 T（socket 6）仍在线 ⇒ 它自己的 GCANCEL + 退出说词必须照发。
        let t_frames: Vec<u16> = frames
            .iter()
            .filter(|(s, _)| *s == 6)
            .map(|(_, b)| ident(b))
            .collect();
        assert_eq!(
            t_frames,
            vec![
                mir2_protocol::messages::SM_GROUPCANCEL,
                mir2_protocol::messages::SM_SYSMESSAGE,
            ],
            "死亡者自身回帧被吞（应仍在线照发）"
        );
        let l_frames: Vec<u16> = frames
            .iter()
            .filter(|(s, _)| *s == 5)
            .map(|(_, b)| ident(b))
            .collect();
        assert_eq!(
            l_frames,
            vec![
                mir2_protocol::messages::SM_GROUPCANCEL,
                mir2_protocol::messages::SM_SYSMESSAGE,
                mir2_protocol::messages::SM_SYSMESSAGE,
            ],
            "团长收到 SM_GROUPCANCEL + 退出/解散说词"
        );
        // 退出说词尾 = EncodeString(prefix + "T 已经退出了本组.")。
        let t_sys = frames
            .iter()
            .find(|(s, b)| *s == 6 && ident(b) == mir2_protocol::messages::SM_SYSMESSAGE)
            .map(|(_, b)| b)
            .unwrap();
        assert_eq!(
            mir2_protocol::edcode::decode(&t_sys[12..]),
            gbk_encode("〖组队〗T 已经退出了本组."),
            "SM_SYSMESSAGE 尾 = EncodeString(组队前缀 + 退出说词)"
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

        let _ = drain_aoi_downlink(&world, &displays, &online, &gates);

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

    /// **C 第 ③ 片判据（观察者开可见门）**：生产宿主构造玩家实体时置 `WantRefMsg = true`
    /// （oracle `PlayObject.cs:889`）；否则 AOI 可见过滤（`BaseObject.ViewRange.cs:112`）只放行
    /// 玩家目标，怪物永远不可见。
    ///
    /// 红检：删掉 [`new_player_entity`] 里的 `e.want_ref_msg = true;` ⇒ 本用例必红。
    #[test]
    fn new_player_entity_sets_want_ref_msg() {
        let e = new_player_entity("m3p".to_string(), 10, 10);
        assert!(
            e.want_ref_msg,
            "oracle PlayObject 构造器置 WantRefMsg=true（PlayObject.cs:889）"
        );
        assert_eq!(e.race, mir2_world::ACTOR_RACE_PLAY);
        // oracle `PlayObject.cs:904`：玩家 ViewRange=12（缺省 5 是 `BaseObject.cs:383`）。
        // 红检：删掉 `e.view_range = 12;` ⇒ 缺省 5 ⇒ 本断言必红。
        assert_eq!(
            e.view_range, 12,
            "oracle PlayObject 构造器置 ViewRange=12（PlayObject.cs:904）"
        );
    }

    /// `openmir2-client-e2e-enter-gaps`（缺口② 同屏无怪）：oracle 玩家 `ViewRange = 12`
    /// （`PlayObject.cs:904`）⇒ AOI 扫描窗口是边长 `2*12+1` 的正方形（`BaseObject.ViewRange.cs:71-74`，
    /// 25×25）。生产宿主缺这行时只剩缺省 5（`BaseObject.cs:383`）⇒ Chebyshev 距离 6..12 的怪
    /// 不进 `visible_actors`、`drain_aoi_downlink` 不发 ⇒ 客户端在出生点/城墙外看不到怪。
    ///
    /// 红检：删掉 [`new_player_entity`] 里的 `e.view_range = 12;` ⇒ `edge`（Chebyshev 12）不在
    /// 可见集 ⇒ 本用例必红。
    #[test]
    fn player_aoi_window_covers_view_range_12() {
        let mut world = mir2_world::World::new(100, 100);
        world.open_session("p");
        let p = world
            .enter_map("p", new_player_entity("p".to_string(), 10, 10), 0)
            .expect("玩家进图");
        // Chebyshev 12：oracle 的 25×25 窗口内 ⇒ 必须可见；缺省 5 的窗口 ⇒ 不可见。
        let mut edge = mir2_world::Entity::new(0, "edge", 22, 10);
        edge.race = mir2_world::ACTOR_RACE_MONSTER;
        let edge_id = world.add_entity(edge);
        // Chebyshev 13：连 oracle 的 25×25 都出界 ⇒ 必须不可见（钉住窗口上界，不是越大越好）。
        let mut out = mir2_world::Entity::new(0, "out", 23, 10);
        out.race = mir2_world::ACTOR_RACE_MONSTER;
        let out_id = world.add_entity(out);
        world.tick();

        let shown: Vec<i32> = world
            .entities
            .get(p)
            .unwrap()
            .visible_actors
            .iter()
            .filter(|v| v.flag == mir2_world::VisibleFlag::Show)
            .map(|v| v.target)
            .collect();
        assert!(
            shown.contains(&edge_id),
            "距离 12 的怪必须在玩家 25×25 窗口内（oracle ViewRange=12）：{shown:?}"
        );
        assert!(
            !shown.contains(&out_id),
            "距离 13 的怪必须超出窗口（窗口上界 = ViewRange）：{shown:?}"
        );
    }

    /// **C 第 ③ 片判据（怪物出现帧端到端）**：怪物展示层登记后，观察者的 AOI `Show` ⇒ 收到以
    /// 怪物 `ActorId` 为 `Recog` 的 `SM_TURN` + `SM_FEATURECHANGED`；`SM_TURN.Series` 取
    /// **世界当前 `Entity.ai.dir`**（不是登记时的初值）。
    ///
    /// 红检：删掉 `drain_aoi_downlink` 里对怪物的 `frame_display.dir = target.ai.dir`（退回
    /// 展示表初值）⇒ `Series & 0xFF == 5` 断言必红；不登记怪物展示（[`downlink::monster_display`]）
    /// ⇒ 帧数 0 ⇒ 必红。
    #[test]
    fn aoi_show_emits_monster_turn_pair_with_world_dir() {
        use std::collections::HashMap;

        let mut world = mir2_world::World::new(100, 100);
        world.open_session("p");
        let p = world
            .enter_map("p", mir2_world::Entity::new(0, "p", 10, 10), 0)
            .expect("玩家进图");
        // oracle `PlayObject` 构造器置 `WantRefMsg = true`（`PlayObject.cs:889`）——玩家靠它
        // 才能把怪物纳入可见集（AOI 可见过滤，`BaseObject.ViewRange.cs:112`）。
        world.entities.get_mut(p).unwrap().want_ref_msg = true;
        // 怪物以 `add_entity` 入队 `Command::Enter`（tick 时上格）；登记展示时 dir=2，
        // 随后 AI 把世界朝向改成 5 —— 出现帧必须取世界值。
        let mut mon = mir2_world::Entity::new(0, "稻草人2", 12, 10);
        mon.race = mir2_world::ACTOR_RACE_MONSTER;
        let mon_id = world.add_entity(mon);
        world.tick();

        let info = mir2_data::models::MonsterInfo {
            name: "稻草人2".to_string(),
            race: 51,
            race_img: 0x0A,
            appr: 0x3344,
            mp: 0x2211,
            ..Default::default()
        };
        let mut displays = downlink::DisplayTable::new();
        displays.set(mon_id, downlink::monster_display(2, &info));
        world.entities.get_mut(mon_id).unwrap().ai.dir = 5;

        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "up".into(),
                chr_name: "p".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: p,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        let _ = drain_aoi_downlink(&world, &displays, &online, &gates);

        let mut frames = Vec::new();
        while let Ok(SimToGate::SendFrame { body, .. }) = rx0.try_recv() {
            let cm = mir2_protocol::frame::CommandMessage::from_bytes(&body)
                .expect("SM_* body 至少 12B");
            frames.push((cm.recog, cm.ident, cm.series));
        }
        assert_eq!(
            frames.len(),
            2,
            "出现帧 = SM_TURN + SM_FEATURECHANGED（怪物 Recog）"
        );
        assert_eq!(frames[0].0, mon_id, "SM_TURN 的 Recog = 怪物 ActorId");
        assert_eq!(frames[0].1, mir2_protocol::messages::SM_TURN);
        assert_eq!(
            frames[0].2 & 0xFF,
            5,
            "SM_TURN.Series 低字节 = 世界当前 ai.dir（非登记初值 2）"
        );
        assert_eq!(frames[1].0, mon_id, "SM_FEATURECHANGED 的 Recog 同怪物");
        assert_eq!(frames[1].1, mir2_protocol::messages::SM_FEATURECHANGED);
    }

    /// `openmir2-client-e2e-enter-gaps`：周期观测的快照必须反映「世界有实体」「AOI 真可见」——
    /// 这正是那张卡把 `sim: … 处理 0 项` 误读成「世界 tick 无实体」的反例。
    ///
    /// 红检：把 `same_map`/`nearest` 的 `o.id == actor_id` 自身排除删掉、或把 `map_id` 比较写反 ⇒ 必红。
    #[test]
    fn world_aoi_diag_reports_visible_and_nearest() {
        let mut world = mir2_world::World::new(100, 100);
        world.open_session("p");
        let p = world
            .enter_map("p", mir2_world::Entity::new(0, "p", 10, 10), 0)
            .expect("玩家进图");
        // 玩家靠 `WantRefMsg=true` 才能把怪物纳入可见集（`BaseObject.ViewRange.cs:112`）。
        world.entities.get_mut(p).unwrap().want_ref_msg = true;
        let mut near = mir2_world::Entity::new(0, "near", 12, 10);
        near.race = mir2_world::ACTOR_RACE_MONSTER;
        let near_id = world.add_entity(near);
        let mut far = mir2_world::Entity::new(0, "far", 30, 30);
        far.race = mir2_world::ACTOR_RACE_MONSTER;
        let far_id = world.add_entity(far);
        world.tick();

        let info = mir2_data::models::MonsterInfo::default();
        let mut displays = downlink::DisplayTable::new();
        displays.set(near_id, downlink::monster_display(0, &info));
        displays.set(far_id, downlink::monster_display(0, &info));

        let diag = world_aoi_diag(&world, &displays, &std::collections::HashMap::new());
        assert_eq!(diag.entities, 3, "玩家 + 两只怪");
        assert_eq!(diag.displays, 2, "展示表只登记了怪物");
        let s = diag
            .sessions
            .iter()
            .find(|s| s.actor_id == p)
            .expect("玩家会话应在快照里");
        assert!(s.vis >= 1, "近怪（距离 2 ≤ view_range）必须在可见集：{s:?}");
        assert_eq!(s.same_map, 2, "同图除自己外 2 只");
        assert_eq!(s.nearest, 2, "最近 Chebyshev 距离 = 2（近怪）");
    }

    /// 卡 `openmir2-client-e2e-enter-gaps` 缺口②（root 2026-10-14 判读误伤）：AOI 诊断的 `vis`
    /// 必须是**当前可见集**（`Show | Invisible`），**不是**「本轮新增」（`Show`）。
    ///
    /// 缘由：`update_visible_gay`（`BaseObject.ViewRange.cs:12-38`，Rust `aoi.rs:28-49`）只对
    /// **首次看见**置 `VisibleFlag.Show`，对「已在列表、本轮又看见」置 `VisibleFlag.Invisible`。
    /// 旧诊断只数 `Show` ⇒ 怪物在视野里停留 >1 tick 后 `vis` 恒为 0：真机 AFTER 日志
    /// `nearest=2..9`（远小于 `vr=12`）仍报 `vis=0`，而客户端实收 13 对出现帧
    /// （`capture-client-viewrange12-seed42.jsonl`）——会被误读成「AOI 没算/没下发」。
    ///
    /// 判据三读数：tick1（首次出现）`new >= 1` 且实发 2 帧；tick2（仍在视野）`new == 0`
    /// 但 `vis == 1`（只算怪、不含自身）、`af` 累计不回退；tick2 **不得重复发出现帧**。
    ///
    /// 红检：把 `vis` 改回 `.filter(|v| v.flag == Show)` ⇒ tick2 的 `vis == 1` 必红（实测读到 0）。
    #[test]
    fn world_aoi_diag_vis_is_current_view_not_new_appearances() {
        use std::collections::HashMap;

        let mut world = mir2_world::World::new(100, 100);
        world.open_session("p");
        let p = world
            .enter_map("p", new_player_entity("p".to_string(), 10, 10), 0)
            .expect("玩家进图");
        // 怪物以 `add_entity` 入队 `Command::Enter`（tick 时上格），Chebyshev 2 ≤ ViewRange(12)。
        let mut mon = mir2_world::Entity::new(0, "稻草人", 12, 10);
        mon.race = mir2_world::ACTOR_RACE_MONSTER;
        let mon_id = world.add_entity(mon);

        let mut displays = downlink::DisplayTable::new();
        displays.set(
            mon_id,
            downlink::monster_display(0, &mir2_data::models::MonsterInfo::default()),
        );

        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "up".into(),
                chr_name: "p".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: p,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        // 只读诊断的累计表（签名同 sim 循环）。
        let mut aoi_frames_total: HashMap<String, usize> = HashMap::new();

        // tick 1：怪物首次出现 ⇒ `new >= 1`，实发 2 帧（SM_TURN + SM_FEATURECHANGED），
        // 累计 `af >= 2`；`vis` 也必须 >= 1。
        world.tick();
        for (chr, n) in drain_aoi_downlink(&world, &displays, &online, &gates) {
            *aoi_frames_total.entry(chr).or_insert(0) += n;
        }
        let mut sent_tick1 = 0usize;
        while rx0.try_recv().is_ok() {
            sent_tick1 += 1;
        }
        assert_eq!(sent_tick1, 2, "出现帧 = SM_TURN + SM_FEATURECHANGED");
        let d1 = world_aoi_diag(&world, &displays, &aoi_frames_total);
        let s1 = d1
            .sessions
            .iter()
            .find(|s| s.actor_id == p)
            .expect("玩家会话");
        assert!(s1.new >= 1, "首 tick 必须有新增可见（Show）：{s1:?}");
        assert_eq!(
            s1.vis, 1,
            "当前可见集 = 只算**其它**对象（排除自身）：{s1:?}"
        );
        assert!(s1.frames >= 2, "首 tick 累计实发帧 >= 2：{s1:?}");
        assert_eq!(s1.view_range, 12, "玩家 view_range = PlayObject.cs:904");

        // tick 2：怪仍在视野 ⇒ `update_visible_gay` 置 `Invisible` ⇒ `new == 0`，
        // 但当前可见集必须仍 >= 1（旧实现这里恒 0，正是 root 误判的来源）。
        world.tick();
        for (chr, n) in drain_aoi_downlink(&world, &displays, &online, &gates) {
            *aoi_frames_total.entry(chr).or_insert(0) += n;
        }
        let mut sent_tick2 = 0usize;
        while rx0.try_recv().is_ok() {
            sent_tick2 += 1;
        }
        assert_eq!(
            sent_tick2, 0,
            "第二轮不得重复发出现帧（oracle 只在 Show 支发）"
        );
        let d2 = world_aoi_diag(&world, &displays, &aoi_frames_total);
        let s2 = d2
            .sessions
            .iter()
            .find(|s| s.actor_id == p)
            .expect("玩家会话");
        assert_eq!(s2.new, 0, "第二轮不是新增（置 Invisible）：{s2:?}");
        assert_eq!(
            s2.vis, 1,
            "第二轮必须仍算「当前可见」（Show|Invisible，排除自身）：{s2:?}"
        );
        assert_eq!(
            s2.frames, s1.frames,
            "无新增 ⇒ 累计帧数不回退也不增：{s2:?}"
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
            body_luck: 0.0,
            body_luck_level: 0,
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
            storage: storage::StorageState::new(),
            actor_id,
            game_gold: 0,
            soft_version_date: 0,
            game_point: 0,
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

    /// **M3⑥-实现 3 第②片**：`emit_npc_says` 按 oracle `MsgQueue` 出队序（`High` 先）落
    /// `SM_MERCHANTSAY(643)`，且 `recog` 取「`priority` ⇒ 玩家 ActorId；否则 ⇒ NPC ActorId
    /// （隐藏 NPC 恒 0）」（`ScriptEngine.cs:993/997` + `BaseObject.Message.cs:30/180`）。
    ///
    /// 红检：把 `dequeue_order` 改回入队序、或把 `recog` 恒取玩家 ActorId ⇒ 本用例必红。
    #[test]
    fn emit_npc_says_priority_first_and_recog_by_path() {
        let (tx, mut rx) = mpsc::unbounded_channel::<SimToGate>();
        let say = |m: &str, priority| game_svr::npc::SayMsg {
            npc_name: "QManage".to_string(),
            raw_msg: m.to_string(),
            text: format!("QManage/{m}"),
            priority,
        };
        let says = vec![say("a", false), say("b", true)];
        assert_eq!(emit_npc_says(&tx, 7, 4242, &says), 2);
        let mut frames = Vec::new();
        while let Ok(SimToGate::SendFrame { socket, body }) = rx.try_recv() {
            frames.push((socket, body));
        }
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].0, 7, "回送给该会话自己的 socket");
        assert_eq!(
            u16::from_le_bytes([frames[0].1[4], frames[0].1[5]]),
            mir2_protocol::messages::SM_MERCHANTSAY,
            "ident"
        );
        assert_eq!(
            i32::from_le_bytes(frames[0].1[0..4].try_into().unwrap()),
            4242,
            "priority 路径 ⇒ recog = 玩家 ActorId"
        );
        assert_eq!(
            i32::from_le_bytes(frames[1].1[0..4].try_into().unwrap()),
            0,
            "SendMsg(normNpc) 路径 ⇒ recog = NPC ActorId（隐藏 NPC 恒 0）"
        );
        assert_eq!(
            frames[0].1,
            smframe::sm_merchantsay(4242, 0, 0, b"QManage/b"),
            "priority 组先出，尾 = EncodeString(ChrName + '/' + sMsg)"
        );
    }

    /// **M3⑥-实现 3 第②片**：`npc_player_from` 的字段快照（`Gold`/`QuestFlag`/`ItemList`）。
    #[test]
    fn npc_player_snapshot_maps_record_fields() {
        let mut rng = mir2_shared::rng::RandomNumber::with_seed(7);
        let mut rngf = |v: i32| rng.random_below(v);
        let cell = record::ServerUserItem {
            make_index: 11,
            index: 22,
            dura: 0,
            dura_max: 0,
            desc: None,
            color_r: 0,
            color_g: 0,
            color_b: 0,
            prefix: None,
        };
        let bag = vec![Some(cell)];
        let catalog = game_svr::ability::StdItemCatalog::new();
        let p = npc_player_from(
            &mut rngf,
            "hero",
            5,
            "0",
            3,
            100,
            Some(&bag),
            Some(&[7, 8, 9]),
            &catalog,
            77,
            &[0, 1, 2],
        );
        assert_eq!(p.chr_name, "hero");
        assert_eq!(p.actor_id, 5);
        assert_eq!(p.map_name, "0");
        assert_eq!(p.level, 3);
        assert_eq!(p.gold, 100);
        assert_eq!(p.group_owner, 77);
        assert_eq!(p.group_member_jobs, vec![0, 1, 2]);
        assert_eq!(&p.quest_flag.as_bytes()[..3], &[7, 8, 9]);
        assert!(
            p.items.is_empty(),
            "空目录 ⇒ `GetStdItem == null` 分支：不入包（不造名）"
        );
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
            storage: storage::StorageState::new(),
            actor_id: 1,
            game_gold: 77,
            soft_version_date: 0,
            game_point: 0,
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

    /// `MakeSaveRcd` 的 `StorageItems`（`WorldServer.cs:1359-1376`）：会话仓库 → **新建 50 槽**
    /// 存档数组（尾槽填 `DelfautItem`），并经生产编码在 DBSvr 侧解回（⇒ 落 `characters_storageitem`
    /// 的同一字段，`PlayDataStorage.Save.cs:360`）。
    ///
    /// 红检：把 [`storage::storage_items_for_save`] 的尾槽填充删掉（留 `None`）⇒ `stor[2]`/`stor[49]`
    /// 的 `expect("空槽填 DelfautItem")` 必红。
    #[test]
    fn save_record_persists_storage_items_as_fifty_slots() {
        use mir2_storage::MemPack;
        let saved = SavedHuman {
            account: "acct".into(),
            record: stage6_record(),
            enter_map: "0".into(),
        };
        let mut sg = test_session_ground(1);
        sg.storage.items = vec![
            game_svr::bag::BagItem {
                make_index: 7301,
                index: 41,
                dura: 7,
                dura_max: 9,
                desc: [0; 14],
            },
            game_svr::bag::BagItem::new(7302, 42),
        ];
        // 锁只影响取回门（⑤-4 第②片），不改存档内容（`IsCanGetBackItem` 不是记录字段）。
        sg.storage.is_can_get_back_item = false;
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
        let stor = d.storage_items.as_ref().expect("storage_items 非 null");
        assert_eq!(
            stor.len(),
            50,
            "StorageItems 是 50 槽定长（WorldServer.cs:1362）"
        );
        let first = stor[0].as_ref().expect("空槽填 DelfautItem，不许是 None");
        assert_eq!(
            (first.make_index, first.index, first.dura, first.dura_max),
            (7301, 41, 7, 9)
        );
        assert_eq!(stor[1].as_ref().unwrap().make_index, 7302);
        assert_eq!(
            stor[2].as_ref().unwrap().make_index,
            0,
            "尾槽 = 默认物品（Index 0）"
        );
        assert_eq!(stor[49].as_ref().unwrap().make_index, 0);
    }

    /// `WorldServer.cs:1213-1227`：进世界的仓库会话态从记录 `StorageItems` 读出（跳过 `null` 槽与
    /// `Index == 0` 的空槽、保持次序），锁初生为 true（`PlayObject.cs:956`）。
    #[test]
    fn enter_loads_storage_items_from_record() {
        let record = vec![
            Some(record::ServerUserItem {
                make_index: 8101,
                index: 33,
                dura: 5,
                dura_max: 6,
                desc: Some(vec![0u8; 14]),
                color_r: 0,
                color_g: 0,
                color_b: 0,
                prefix: None,
            }),
            None,
            Some(default_server_item()),
            Some(record::ServerUserItem {
                make_index: 8102,
                index: 34,
                dura: 1,
                dura_max: 2,
                desc: Some(vec![0u8; 14]),
                color_r: 0,
                color_g: 0,
                color_b: 0,
                prefix: None,
            }),
        ];
        let st = storage::load_from_record(Some(&record));
        assert_eq!(
            st.items.iter().map(|i| i.make_index).collect::<Vec<_>>(),
            vec![8101, 8102],
            "只收 Index>0 且非 null 的槽，次序不变"
        );
        assert!(st.is_can_get_back_item, "初生锁开");
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
        // oracle 死亡掉落恒不落地（`Envir.AddItemToMap` 首行 `MapItem.ItemId == 0` 门，
        // `Envirnoment.cs:88-92`）⇒ 本接缝不再依赖击杀链，直接显式落一格金币
        // （与 `world::World` 掉落侧同口径；读分片/拾取机制的生产输入来源）。
        let gold_x = 11;
        let gold_y = 10;
        let expected_count = 300;
        assert!(world.maps[0].add_floor_item(
            gold_x,
            gold_y,
            p,
            mir2_world::map::FloorItemMeta {
                kind: mir2_world::FloorItemKind::Gold,
                index: 0,
                count: expected_count,
                item_id: p,
                of_base_object: p,
            },
            0,
        ));
        // 把角色移到金币格（`CM_PICKUP` 的位置门 `CurrX==nParam2 && CurrY==nParam3` 与
        // `map.get_item(CurrX,CurrY)` 都要对上；世界格未参与本接缝判定）。
        {
            let e = world.entities.get_mut(p).expect("player 必在实体表");
            e.x = gold_x;
            e.y = gold_y;
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
                storage: storage::StorageState::new(),
                actor_id: p,
                game_gold: 0,
                soft_version_date: 0,
                game_point: 0,
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
            u16::try_from(gold_x + 1).unwrap(),
            u16::try_from(gold_y).unwrap(),
            None,
            "hero",
            901,
            &std::collections::HashMap::new(),
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
            u16::try_from(gold_x).unwrap(),
            u16::try_from(gold_y).unwrap(),
            None,
            "hero",
            901,
            &std::collections::HashMap::new(),
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
                storage: storage::StorageState::new(),
                actor_id: 42,
                game_gold: 0,
                soft_version_date: 0,
                game_point: 0,
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
            false,
            // 生产默认 = 冻结客户端 58B（`ability_tail`）。
            true,
        );

        // 出帧：SM_ADDITEM(200，换下回背包) → SM_TAKEOFF_OK(619) → SM_TAKEOFF_FAIL(620，oracle
        // 既有形态) → SM_ABILITY(52) → SM_SUBABILITY(752)。
        let mut idents = Vec::new();
        let mut bodies: Vec<Vec<u8>> = Vec::new();
        while let Ok(SimToGate::SendFrame { socket, body }) = rx0.try_recv() {
            assert_eq!(socket, 901, "回帧必须发给该用户的 socket");
            let cm = mir2_protocol::frame::CommandMessage::from_bytes(&body).expect("body >= 12B");
            idents.push(cm.ident);
            bodies.push(body);
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
        // **换装后的 52 尾必须是冻结客户端 58B `TAbility`**（生产默认 `client_layout=true`）：
        // 若 `handle_bag_cmd` 漏用 `ability_tail`（退回 oracle 40B）⇒ 本断言必红。
        let ability = bodies
            .iter()
            .find(|b| {
                CommandMessage::from_bytes(b).expect(">=12B").ident
                    == mir2_protocol::messages::SM_ABILITY
            })
            .expect("必须出现 SM_ABILITY");
        assert_eq!(
            &ability[12..],
            &mir2_protocol::edcode::encode(&record::encode_ability_client(
                &sessions["hero"].state.wabil
            )),
            "换装后 SM_ABILITY 尾 = Encode(58B TAbility)"
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
                storage: storage::StorageState::new(),
                actor_id: 42,
                game_gold: 0,
                soft_version_date: 0,
                game_point: 0,
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
            false,
            false,
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
                storage: storage::StorageState::new(),
                actor_id: hero,
                game_gold: 0,
                soft_version_date: 0,
                game_point: 0,
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
            false,
            false,
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
                storage: storage::StorageState::new(),
                actor_id: 7,
                game_gold: 0,
                soft_version_date: 0,
                game_point: 0,
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
                storage: storage::StorageState::new(),
                actor_id: hid,
                game_gold: 0,
                soft_version_date: 0,
                game_point: 0,
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
        drain_recovery(
            &mut sessions,
            &world,
            &online,
            &gates,
            12,
            &std::collections::HashSet::new(),
        );
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

    /// **reviewer-5 O-1 收口（同 tick 帧序）**：同 tick 内既走**击杀经验/升级链**又走
    /// **体力点扣血段**时，出帧次序 = `drain_kill_exp_chain`（53→44→45→52→752）→ `drain_stamina`（53），
    /// 对齐 oracle 一次 `PlayObject.Run()` 的「消息段 → `BaseObject.Run()` 体力点段」
    /// （`PlayObject.Message.cs:765` + `BaseObject.Base.cs:28-34/36-68/110-115`）。
    ///
    /// 红检：把 [`drain_run_downlinks`] 里的 `drain_stamina` 挪到 `drain_kill_exp_chain` 之前
    /// ⇒ `idents` 变成 `[53(体力), 53(升级), 44, 45, 52, 752]` ⇒ 本用例必红。
    #[test]
    fn tick_frame_order_puts_stamina_after_kill_exp_chain() {
        use mir2_protocol::frame::CommandMessage;
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let killer = {
            let mut e = mir2_world::Entity::new(0, "killer".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
        {
            let e = world.entities.get_mut(killer).unwrap();
            e.exp = 500;
            e.level = 11;
            e.max_exp = 1000;
            e.hp = 10;
            e.max_hp = 20;
        }
        let displays = downlink::DisplayTable::new();
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "uk".into(),
                chr_name: "killer".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 903,
                actor_id: killer,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut sg = test_session_ground(killer);
        sg.equip.abil.level = 10;
        sg.state.wabil.level = 10;
        sg.state.wabil.hp = 10;
        sg.state.wabil.mp = 5;
        // 体力点已被攻击/移动推到阈值以下（`BaseObject.Base.cs:105`），本轮恰好重放一次 `Run()`。
        sg.stamina.health_tick = -400;
        sg.stamina.replay_cursor = now_ms() - 60;
        sg.stamina.auto_recovery_tick = sg.stamina.replay_cursor;
        sessions.insert("killer".into(), sg);

        let rec = mir2_world::TickRecord {
            exp_awards: vec![(killer, 500, true, 11)],
            ..Default::default()
        };
        let _ = drain_run_downlinks(
            &rec,
            &mut world,
            &displays,
            &online,
            &mut sessions,
            &gates,
            &game_svr::ability::StdItemCatalog::new(),
            12,
            300,
            800,
            false,
        );
        let mut frames = Vec::new();
        while let Ok(SimToGate::SendFrame { socket, body }) = rx0.try_recv() {
            assert_eq!(socket, 903);
            frames.push(CommandMessage::from_bytes(&body).expect("12B"));
        }
        let idents: Vec<u16> = frames.iter().map(|cm| cm.ident).collect();
        assert_eq!(
            idents,
            vec![
                mir2_protocol::messages::SM_HEALTHSPELLCHANGED,
                mir2_protocol::messages::SM_WINEXP,
                mir2_protocol::messages::SM_LEVELUP,
                mir2_protocol::messages::SM_ABILITY,
                mir2_protocol::messages::SM_SUBABILITY,
                mir2_protocol::messages::SM_HEALTHSPELLCHANGED,
            ],
            "升级链（53→44→45→52→752）之后才是体力点段（53）"
        );
        // 首条 53 = 升级 `IncHealthSpell(2000,2000)` 后的满血快照；末条 53 = 体力点扣 1 点后的快照。
        let max_hp = sessions["killer"].state.wabil.max_hp;
        assert!(max_hp > 1, "升级链已把 MaxHP 提到等级派生值");
        assert_eq!(frames[0].param, max_hp, "升级段 53 是满血快照");
        let last = frames.last().unwrap();
        assert_eq!(last.recog, killer, "体力点 53 的 recog = ActorId");
        assert_eq!(last.param, max_hp - 1, "体力点段 HP -= 1");
        assert_eq!(sessions["killer"].state.wabil.hp, max_hp - 1);
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
                storage: storage::StorageState::new(),
                actor_id: 7,
                game_gold: 0,
                soft_version_date: 0,
                game_point: 0,
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
        // ⑨-2 第②片新增入参：本用例的魔号（1/99/12）都不是召唤面 ⇒ 空表即等价。
        let mut ground_sessions: HashMap<String, SessionGround> = HashMap::new();
        let catalog = game_svr::ability::StdItemCatalog::new();
        let magic_table = magics::MagicTable::default();
        let pet_cfg = mir2_data::pet_slave::PetSlaveConfig::default();
        let registry: Option<mir2_envir::MapRegistry> = None;
        let table = game_svr::mongen::MonsterTable::new(Vec::new());

        // ① 未知魔法号（守卫 NoMagic）⇒ 不回 ack。
        handle_spell_cmd(
            game_svr::feature::make_long(12, 10),
            99,
            0,
            0,
            "caster",
            911,
            &online,
            &mut world,
            &mut saves,
            &mut ground_sessions,
            &catalog,
            &magic_table,
            &pet_cfg,
            8,
            registry.as_ref(),
            &table,
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
            &mut world,
            &mut saves,
            &mut ground_sessions,
            &catalog,
            &magic_table,
            &pet_cfg,
            8,
            registry.as_ref(),
            &table,
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
            &mut world,
            &mut saves,
            &mut ground_sessions,
            &catalog,
            &magic_table,
            &pet_cfg,
            8,
            registry.as_ref(),
            &table,
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
            &mut world,
            &mut saves,
            &mut ground_sessions,
            &catalog,
            &magic_table,
            &pet_cfg,
            8,
            registry.as_ref(),
            &table,
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

    /// **`openmir2-m3-pet-slave-summon`（第②片）**：`CM_SPELL(17 召唤骷髅)` 效果体接线——
    /// 护身符门过 ⇒ 扣耐久 + 自身 `SM_DURACHANGE`；对观察者广播 `SM_SPELL`/`SM_MAGICFIRE`；
    /// 自身收 `+GD`。空怪表 ⇒ 造怪失败，但仍走完帧面（oracle `ClientSpellXY` 恒 `result=true`）。
    ///
    /// 红检：① `cast_slave_spell` 不消耗护身符（`use_amulet` 不改耐久）⇒ 耐久仍 450 必红；
    /// ② 把 `sm_spell` 的 `Series` 换成 `MagicId`（不取 `Magic.Effect`）⇒ 帧字段断言必红；
    /// ③ 不广播 `RM_SPELL`（去掉 `broadcast_ref_msg`）⇒ 观察者无 `SM_SPELL` 必红。
    #[test]
    fn spell_cmd_summon_wires_amulet_and_frames() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let caster = {
            let mut e = new_player_entity("caster".to_string(), 10, 10);
            e.level = 40;
            world.add_entity(e)
        };
        let observer = world.add_entity(new_player_entity("obs".to_string(), 11, 10));
        world.tick();
        let mut displays = downlink::DisplayTable::new();
        displays.set(
            caster,
            downlink::ActorDisplay::new(4, 0, 111, 0, 0, 255, b"caster".to_vec()),
        );
        displays.set(
            observer,
            downlink::ActorDisplay::new(4, 0, 112, 0, 0, 255, b"obs".to_vec()),
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
        online_set(
            &mut online,
            OnlineEntry {
                account: "uo".into(),
                chr_name: "obs".into(),
                session_id: 2,
                gate_idx: 0,
                socket: 912,
                actor_id: observer,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut saves: HashMap<String, SavedHuman> = HashMap::new();
        let mut record = stage6_record();
        record.data.magic = Some(vec![Some(record::MagicRcd {
            mag_idx: 17,
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
        let mut ground_sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut sg = test_session_ground(caster);
        sg.use_items[game_svr::ability::SLOT_ARMRING_L] = Some(game_svr::bag::BagItem {
            make_index: 1,
            index: 1,
            dura: 450,
            dura_max: 1000,
            desc: [0; 14],
        });
        ground_sessions.insert("caster".into(), sg);
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            1,
            mir2_data::StdItem {
                name: "护身符".into(),
                std_mode: 25,
                shape: 5,
                ..mir2_data::StdItem::default()
            },
        );
        let magic_table = magics::MagicTable::new(vec![mir2_data::MagicInfo {
            magic_id: 17,
            effect: 3,
            effect_type: 4,
            train_lv: 3,
            ..mir2_data::MagicInfo::default()
        }]);
        let pet_cfg = mir2_data::pet_slave::PetSlaveConfig::default();
        let registry = mir2_envir::MapRegistry::from_entries(
            mir2_envir::MapLoader::new(".", "map", "envir"),
            Vec::new(),
        );
        let table = game_svr::mongen::MonsterTable::new(Vec::new());

        handle_spell_cmd(
            game_svr::feature::make_long(10, 11),
            17,
            9999,
            0,
            "caster",
            911,
            &online,
            &mut world,
            &mut saves,
            &mut ground_sessions,
            &catalog,
            &magic_table,
            &pet_cfg,
            8,
            Some(&registry),
            &table,
            &mut displays,
            &gates,
        );

        let mut frames: Vec<(i32, Vec<u8>)> = Vec::new();
        while let Ok(m) = rx0.try_recv() {
            match m {
                SimToGate::SendFrame { socket, body } => frames.push((socket, body)),
                SimToGate::SendRaw { socket, payload } => frames.push((socket, payload)),
                other => panic!("意外的下行：{other:?}"),
            }
        }
        let ident_of = |b: &[u8]| {
            mir2_protocol::frame::CommandMessage::from_bytes(b)
                .unwrap()
                .ident
        };
        let obs: Vec<&Vec<u8>> = frames
            .iter()
            .filter(|(s, _)| *s == 912)
            .map(|(_, b)| b)
            .collect();
        assert!(
            obs.iter()
                .any(|b| ident_of(b) == mir2_protocol::messages::SM_SPELL),
            "观察者应收 SM_SPELL(RM_SPELL)；实得 {obs:?}"
        );
        assert!(
            obs.iter()
                .any(|b| ident_of(b) == mir2_protocol::messages::SM_MAGICFIRE),
            "观察者应收 SM_MAGICFIRE；实得 {obs:?}"
        );
        let me: Vec<&Vec<u8>> = frames
            .iter()
            .filter(|(s, _)| *s == 911)
            .map(|(_, b)| b)
            .collect();
        assert!(
            me.iter()
                .any(|b| ident_of(b) == mir2_protocol::messages::SM_DURACHANGE),
            "施法者自身应收 SM_DURACHANGE；实得 {me:?}"
        );
        // **F-1（reviewer-6 `7724b9e441`）**：`RM_MAGICFIRE` 的 oracle 处理器无
        // `ActorId != ActorId` 守卫（`PlayObject.Message.cs:1788-1793`）⇒ 施法者自己也应收
        // `SM_MAGICFIRE`；而 `RM_SPELL`（有守卫）不给自己发（这里 `me` 不含 `SM_SPELL`）。
        assert!(
            me.iter()
                .any(|b| ident_of(b) == mir2_protocol::messages::SM_MAGICFIRE),
            "F-1：施法者自身应收 SM_MAGICFIRE（RM_MAGICFIRE 无自身守卫）；实得 {me:?}"
        );
        assert!(
            !me.iter()
                .any(|b| ident_of(b) == mir2_protocol::messages::SM_SPELL),
            "RM_SPELL 有 `ActorId != ActorId` 守卫 ⇒ 施法者自身不应收 SM_SPELL；实得 {me:?}"
        );
        assert!(me.iter().any(|b| b.starts_with(b"+GD/")), "成功支仍回 +GD");
        // 护身符扣至 350（450 - 1*100）。
        assert_eq!(
            ground_sessions["caster"].use_items[game_svr::ability::SLOT_ARMRING_L]
                .as_ref()
                .unwrap()
                .dura,
            350,
            "UseAmulet 扣耐久落 UseItems"
        );
    }

    /// **⑨-2 护身符扣光帧面**：`UseAmulet` 的 `else` 支（`MagicBase.cs:121-126`）——耐久恰为
    /// `nCount*100` 时 oracle 走 `Dura = 0; SendDelItems(item); Index = 0;`，宿主须发**立即**
    /// `SM_DELITEM(202)`（`PlayObject.Base.cs:1648-1665`），且帧内 `Dura` = 0、清槽。
    ///
    /// 红检：① 删掉清槽前取件/发帧（回到登记前的 header-only 无帧）⇒ 无 `SM_DELITEM` 必红；
    /// ② 帧内 `Dura` 用扣光前的值（不做 `Dura=0`）⇒ 尾字节不等必红。
    #[test]
    fn spell_cmd_summon_exhausted_amulet_emits_delitem() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let caster = {
            let mut e = new_player_entity("caster".to_string(), 10, 10);
            e.level = 40;
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
            mag_idx: 17,
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
        let mut ground_sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut sg = test_session_ground(caster);
        // 17 的护身符需求 = `CheckAmulet(1,1)`：`round(Dura/100) >= 1` ⇒ Dura=100 过门，
        // 且 `Dura > 100` 为假 ⇒ 走 else（扣光删除）支。
        sg.use_items[game_svr::ability::SLOT_ARMRING_L] = Some(game_svr::bag::BagItem {
            make_index: 1,
            index: 1,
            dura: 100,
            dura_max: 1000,
            desc: [0; 14],
        });
        ground_sessions.insert("caster".into(), sg);
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        let amulet_tpl = mir2_data::StdItem {
            name: "护身符".into(),
            std_mode: 25,
            shape: 5,
            ..mir2_data::StdItem::default()
        };
        catalog.insert(1, amulet_tpl.clone());
        let magic_table = magics::MagicTable::new(vec![mir2_data::MagicInfo {
            magic_id: 17,
            effect: 3,
            effect_type: 4,
            train_lv: 3,
            ..mir2_data::MagicInfo::default()
        }]);
        let pet_cfg = mir2_data::pet_slave::PetSlaveConfig::default();
        let registry = mir2_envir::MapRegistry::from_entries(
            mir2_envir::MapLoader::new(".", "map", "envir"),
            Vec::new(),
        );
        let table = game_svr::mongen::MonsterTable::new(Vec::new());

        handle_spell_cmd(
            game_svr::feature::make_long(10, 11),
            17,
            9999,
            0,
            "caster",
            911,
            &online,
            &mut world,
            &mut saves,
            &mut ground_sessions,
            &catalog,
            &magic_table,
            &pet_cfg,
            8,
            Some(&registry),
            &table,
            &mut displays,
            &gates,
        );

        let mut frames: Vec<(i32, Vec<u8>)> = Vec::new();
        while let Ok(m) = rx0.try_recv() {
            match m {
                SimToGate::SendFrame { socket, body } => frames.push((socket, body)),
                SimToGate::SendRaw { socket, payload } => frames.push((socket, payload)),
                other => panic!("意外的下行：{other:?}"),
            }
        }
        let ident_of = |b: &[u8]| {
            mir2_protocol::frame::CommandMessage::from_bytes(b)
                .unwrap()
                .ident
        };
        let me: Vec<&Vec<u8>> = frames
            .iter()
            .filter(|(s, _)| *s == 911)
            .map(|(_, b)| b)
            .collect();
        let del = me
            .iter()
            .find(|b| ident_of(b) == mir2_protocol::messages::SM_DELITEM)
            .unwrap_or_else(|| panic!("扣光支应发 SM_DELITEM；实得 {me:?}"));
        let cm = mir2_protocol::frame::CommandMessage::from_bytes(del).unwrap();
        assert_eq!(cm.recog, caster, "Recog=ActorId");
        assert_eq!((cm.param, cm.tag, cm.series), (0, 0, 1), "Series=1");
        // 尾 = `EncodeBuffer(clientItem)`：`Dura` 恒 0（oracle 先置 0 再 `SendDelItems`）。
        let expect_ci = game_svr::itemwire::ClientItem {
            tpl: &amulet_tpl,
            name: "护身符".into(),
            make_index: 1,
            dura: 0,
            dura_max: 1000,
            desc: [0; 14],
        };
        let expect_tail =
            mir2_protocol::edcode::encode(&game_svr::itemwire::encode_client_item(&expect_ci));
        assert_eq!(
            mir2_protocol::edcode::decode(&del[12..]),
            mir2_protocol::edcode::decode(&expect_tail),
            "SM_DELITEM 尾 = EncodeBuffer(ClientItem，Dura=0)"
        );
        assert!(
            !me.iter()
                .any(|b| ident_of(b) == mir2_protocol::messages::SM_DURACHANGE),
            "扣光支不发 SM_DURACHANGE（oracle 只发 SendDelItems）；实得 {me:?}"
        );
        assert!(
            ground_sessions["caster"].use_items[game_svr::ability::SLOT_ARMRING_L].is_none(),
            "扣光支清槽（UseItems[idx] = None）"
        );
    }

    /// **⑨-2 残余（46 分身）**：`CM_SPELL(46)` 效果体接线——造玩家型克隆体（名字 `Clone`、
    /// 坐标=主人、`Dir = GetBackDir(主人转向后的 Dir)`、格被占、不入 `SlaveList`）+ 展示表登记
    /// + 主人/观察者收 `SM_TURN`（名尾 `"Clone/255"`）与 `SM_MAGICFIRE`，观察者收 `SM_SPELL`。
    ///
    /// 红检：① `dir` 用主人正向（不用 `GetBackDir`）⇒ 展示/帧方向断言必红；
    /// ② 不发克隆体 `sm_turn` ⇒ `SM_TURN`/名尾断言必红；③ 不造实体 ⇒ 实体/格占用断言必红。
    #[test]
    fn spell_cmd_clone_wires_entity_and_frames() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let caster = {
            let mut e = new_player_entity("caster".to_string(), 10, 10);
            e.level = 40;
            world.add_entity(e)
        };
        let observer = world.add_entity(new_player_entity("obs".to_string(), 11, 10));
        world.tick();
        let mut displays = downlink::DisplayTable::new();
        // 主人 feature：gender=1、hair=5（dress/weapon 故意非 0，副本应丢掉）。
        displays.set(
            caster,
            downlink::ActorDisplay::new(
                4,
                0,
                game_svr::feature::make_human_feature(0, 3 * 2 + 1, 1 + 2 * 2, 5 * 2 + 1),
                0,
                0,
                downlink::PLAYER_NAME_COLOR,
                b"caster".to_vec(),
            ),
        );
        displays.set(
            observer,
            downlink::ActorDisplay::new(4, 0, 112, 0, 0, 255, b"obs".to_vec()),
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
        online_set(
            &mut online,
            OnlineEntry {
                account: "uo".into(),
                chr_name: "obs".into(),
                session_id: 2,
                gate_idx: 0,
                socket: 912,
                actor_id: observer,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut saves: HashMap<String, SavedHuman> = HashMap::new();
        let mut record = stage6_record();
        record.data.magic = Some(vec![Some(record::MagicRcd {
            mag_idx: 46,
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
        let mut ground_sessions: HashMap<String, SessionGround> = HashMap::new();
        let catalog = game_svr::ability::StdItemCatalog::new();
        let magic_table = magics::MagicTable::new(vec![mir2_data::MagicInfo {
            magic_id: 46,
            effect: 1,
            effect_type: 1,
            train_lv: 3,
            ..mir2_data::MagicInfo::default()
        }]);
        let pet_cfg = mir2_data::pet_slave::PetSlaveConfig::default();
        let registry: Option<mir2_envir::MapRegistry> = None;
        let table = game_svr::mongen::MonsterTable::new(Vec::new());

        handle_spell_cmd(
            // 目标在右 ⇒ `default` 支把主人 Dir 转向右(2)；克隆体 `Dir = GetBackDir(2) = 6`。
            game_svr::feature::make_long(12, 10),
            46,
            0,
            0,
            "caster",
            911,
            &online,
            &mut world,
            &mut saves,
            &mut ground_sessions,
            &catalog,
            &magic_table,
            &pet_cfg,
            8,
            registry.as_ref(),
            &table,
            &mut displays,
            &gates,
        );

        let (clone_id, cx, cy, cray, cslaves) = {
            let c = world
                .entities
                .iter()
                .find(|e| e.name == mir2_world::CLONE_NAME)
                .expect("应造出克隆体实体");
            (c.id, c.x, c.y, c.race, c.slave_list.len())
        };
        assert_eq!((cx, cy), (10, 10), "坐标 = 主人坐标");
        assert_eq!(cray, mir2_world::ACTOR_RACE_PLAY, "玩家型克隆体");
        assert_eq!(cslaves, 0, "克隆体不入 SlaveList");
        assert!(
            world.entities.get(caster).unwrap().slave_list.is_empty(),
            "主人的 SlaveList 不被触碰"
        );
        // 展示：`Dir = GetBackDir(主人转向后的 Dir=2) = 6`；feature 取主人 Gender/Hair（1/5）。
        let d = displays.get(clone_id).expect("克隆体展示应登记");
        assert_eq!(d.dir, 6, "Dir = GetBackDir(右=2) = 左=6");
        assert_eq!(d.show_name, downlink::gbk("Clone"));
        assert_eq!(d.name_color, downlink::PLAYER_NAME_COLOR);
        assert_eq!(
            d.feature,
            game_svr::feature::make_human_feature(0, 1, 1, 5 * 2 + 1),
            "Gender/Hair 照抄、Dress/Weapon 归 0"
        );
        // 格占用。
        let (cell, ok) = world.maps[0].get_cell_info(10, 10);
        assert!(ok);
        assert!(
            cell.obj_list
                .iter()
                .any(|o| o.cell_obj_id == clone_id && o.actor_object),
            "克隆体应占主人同格"
        );

        let mut frames: Vec<(i32, Vec<u8>)> = Vec::new();
        while let Ok(m) = rx0.try_recv() {
            match m {
                SimToGate::SendFrame { socket, body } => frames.push((socket, body)),
                SimToGate::SendRaw { socket, payload } => frames.push((socket, payload)),
                other => panic!("意外的下行：{other:?}"),
            }
        }
        let ident_of = |b: &[u8]| {
            mir2_protocol::frame::CommandMessage::from_bytes(b)
                .unwrap()
                .ident
        };
        let obs: Vec<&Vec<u8>> = frames
            .iter()
            .filter(|(s, _)| *s == 912)
            .map(|(_, b)| b)
            .collect();
        let me: Vec<&Vec<u8>> = frames
            .iter()
            .filter(|(s, _)| *s == 911)
            .map(|(_, b)| b)
            .collect();
        assert!(
            obs.iter()
                .any(|b| ident_of(b) == mir2_protocol::messages::SM_SPELL),
            "观察者应收 SM_SPELL(46)；实得 {obs:?}"
        );
        assert!(
            obs.iter()
                .any(|b| ident_of(b) == mir2_protocol::messages::SM_MAGICFIRE),
            "观察者应收 SM_MAGICFIRE"
        );
        assert!(
            me.iter()
                .any(|b| ident_of(b) == mir2_protocol::messages::SM_MAGICFIRE),
            "F-1：主人自己也收 SM_MAGICFIRE；实得 {me:?}"
        );
        assert!(
            !me.iter()
                .any(|b| ident_of(b) == mir2_protocol::messages::SM_SPELL),
            "RM_SPELL 有自身守卫 ⇒ 主人自己不应收 SM_SPELL"
        );
        // `SM_TURN`（Recog = 克隆体 id）+ 名尾 `"Clone/255"`；两个在线玩家各一。
        let turns: Vec<&Vec<u8>> = frames
            .iter()
            .filter(|(_, b)| ident_of(b) == mir2_protocol::messages::SM_TURN)
            .map(|(_, b)| b)
            .collect();
        assert_eq!(turns.len(), 2, "主人与观察者各一（克隆体自己被排除）");
        for t in &turns {
            assert_eq!(
                i32::from_le_bytes([t[0], t[1], t[2], t[3]]),
                clone_id,
                "SM_TURN Recog = 克隆体 ActorId"
            );
            let msg = mir2_protocol::edcode::decode(&t[23..]);
            assert!(
                msg.starts_with(b"Clone/"),
                "名尾应为 Clone/<color>；实得 {msg:?}"
            );
        }
        assert!(me.iter().any(|b| b.starts_with(b"+GD/")), "成功支回 +GD");
        // 熟练度尾：`level=1 < 3` 且 `boTrain=true` ⇒ 若升级门过则 `TrainSkill(Random(3)+1)` 已落。
        assert_eq!(
            saves["caster"].record.data.magic.as_ref().unwrap()[0]
                .as_ref()
                .unwrap()
                .mag_idx,
            46
        );
    }

    /// **第一片（战斗下行）**：手造一个 tick 记录 ⇒ `drain_combat_downlink` 给受击玩家自己发
    /// `SM_STRUCK`（生产默认 = 冻结客户端 `HIGHHP` 布局：头 `LoWord(HP)/HiWord(HP)/伤害` +
    /// 16B `MessageBodyWL` 尾 `{Param1=MaxHP,Param2=status,Tag1=hitter,Tag2=mag}`；
    /// `MIR2_CLIENT_ABILITY_LAYOUT=0` ⇒ oracle 布局 `{Param1=feature}`）、给同屏（含死者自己）发
    /// `SM_NOWDEATH`（头 `X/Y/Dir` + 8B `CharDesc` 尾），并把世界实体 HP 同步回会话 `WAbil.HP`。
    ///
    /// 红检：① 把 `smframe::sm_struck_client` 的头 `hp` 拆位（`lo_word`/`hi_word` 互换）⇒ 客户端布局头断言必红；
    /// ② 把死亡广播改成 `broadcast_ref_msg`（排除自己）⇒ 死者自己收不到 `SM_NOWDEATH` 必红；
    /// ③ 把 `smframe::sm_struck_client` 的尾 `Param1` 改回 `feature` ⇒ `struck[12..]` 尾断言必红
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
        // 生产默认 = 冻结客户端 `HIGHHP` 布局（`sm_struck_frame` ⇒ `sm_struck_client`）：
        // `param/tag` 拆 32 位 HP、`MaxHP` 在尾 `Param1`。`MIR2_CLIENT_ABILITY_LAYOUT=0` ⇒ oracle 布局。
        if game_svr::smframe::client_layout_enabled() {
            assert_eq!(
                (cm.recog, cm.param, cm.tag, cm.series),
                (victim, 0, 0, 7),
                "SM_STRUCK(客户端布局) 头 = (受击者, LoWord(HP), HiWord(HP), 伤害)"
            );
        } else {
            assert_eq!(
                (cm.recog, cm.param, cm.tag, cm.series),
                (victim, 0, 100, 7),
                "SM_STRUCK(oracle) 头 = (受击者, HP, MaxHP, 伤害)"
            );
        }
        // **M-1 收口**（reviewer-5 `ebfd15a8`）：文档声称覆盖 16B `MessageBodyWL` 尾，这里真的
        // 逐字节断言 `struck[12..]`，否则尾 mutation 抓不到。
        // 尾 = Encode(MessageBodyWL{ Param1=?, Param2=CharStatus,
        //                            Tag1=命中者 ActorId, Tag2=(RM_STRUCK_MAG ? 1 : 0) })。
        // `Param1`：客户端布局 = `MaxHP`（HIGHHP 支读这个）；oracle = `GetFeature()`。
        let param1 = if game_svr::smframe::client_layout_enabled() {
            100_i32 // MaxHP
        } else {
            111_i32 // GetFeature()
        };
        let mut wl = [0u8; 16];
        wl[0..4].copy_from_slice(&param1.to_le_bytes()); // Param1
        wl[4..8].copy_from_slice(&0_i32.to_le_bytes()); // Param2 = CharStatus
        wl[8..12].copy_from_slice(&attacker.to_le_bytes()); // Tag1 = 命中者 ActorId
        wl[12..16].copy_from_slice(&0_i32.to_le_bytes()); // Tag2 = (RM_STRUCK_MAG ? 1 : 0)
        assert_eq!(
            &struck[12..],
            &mir2_protocol::edcode::encode(&wl),
            "SM_STRUCK 尾 = Encode(MessageBodyWL{{Param1=MaxHP(client)|feature(oracle),Param2=status,Tag1=hitter,Tag2=mag}})"
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

    /// **片㉜（③ DoD③「屏幕见怪并能攻击造成伤害」）**：受击帧按 oracle `SendRefMsg(RM_STRUCK)`
    /// 广播给**受击者视野内全体 `Play`**（含受害者自己），不再只发受害者一人。
    ///
    /// 锚点：`BaseObject.Attack.cs:57` → `BaseObject.Base.cs:699-705` →
    /// `BaseObject.Message.cs:383+`（`Race==Play` 且切比雪夫 ≤ `SendRefMsgRange`=12）→
    /// `PlayObject.Message.cs:1562-1628`。
    ///
    /// 红检：把 `drain_combat_downlink` ① 的广播循环改回「只发受击者自己」（旧实现）⇒
    /// 本用例必红（观察者 902 收不到 `SM_STRUCK`）。
    #[test]
    fn combat_downlink_broadcasts_struck_to_all_visible_players() {
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
        let observer = {
            let mut e = mir2_world::Entity::new(0, "obs".to_string(), 21, 10);
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
        online_set(
            &mut online,
            OnlineEntry {
                account: "uo".into(),
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
        sessions.insert("victim".into(), test_session_ground(victim));

        let rec = mir2_world::TickRecord {
            attacks: vec![(
                attacker,
                victim,
                mir2_world::AttackOutcome {
                    power: 10,
                    hit: true,
                    damage: 7,
                    killed: false,
                    target_hp_after: 12,
                },
            )],
            ..Default::default()
        };
        drain_combat_downlink(&rec, &world, &displays, &online, &mut sessions, &gates);
        let mut per_socket: HashMap<i32, Vec<u8>> = HashMap::new();
        while let Ok(m) = rx0.try_recv() {
            if let SimToGate::SendFrame { socket, body } = m {
                per_socket.insert(socket, body);
            }
        }
        assert_eq!(
            per_socket.len(),
            2,
            "受击帧必须广播给同图视野内全体在线玩家（受害者 + 观察者）：{per_socket:?}"
        );
        for socket in [901_i32, 902_i32] {
            let body = per_socket.get(&socket).expect("每个玩家都应收受击帧");
            let cm = CommandMessage::from_bytes(body).expect("12B");
            assert_eq!(
                u16::from_le_bytes([body[4], body[5]]),
                mir2_protocol::messages::SM_STRUCK
            );
            // 生产默认 = 客户端 `HIGHHP` 布局（`param=LoWord(HP)=12, tag=HiWord(HP)=0`）；
            // `=0` ⇒ oracle 布局（`param=HP=12, tag=MaxHP=100`）。HP=12<65536 ⇒ HiWord=0。
            let want = if game_svr::smframe::client_layout_enabled() {
                (victim, 12, 0, 7)
            } else {
                (victim, 12, 100, 7)
            };
            assert_eq!(
                (cm.recog, cm.param, cm.tag, cm.series),
                want,
                "socket={socket} 的受击帧头 = (受击者, HP, MaxHP, 伤害)"
            );
        }
        assert_eq!(
            sessions["victim"].state.wabil.hp, 12,
            "受击后世界 HP 同步回会话 WAbil"
        );
    }

    /// **片㉜**：**怪物目标**也要广播受击帧 —— 怪物自身无连接，但视野内玩家收到 ⇒ 真客户端
    /// 能看到怪掉血/伤害（DoD③「能攻击造成伤害」）。
    ///
    /// 红检：把广播循环改回「只发受击者自己」（旧实现）⇒ 怪物不在 `online` ⇒ **零帧**，本用例必红。
    #[test]
    fn combat_downlink_broadcasts_struck_for_monster_target() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let monster = {
            let mut e = mir2_world::Entity::new(0, "mon".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_MONSTER;
            e.hp = 200;
            e.max_hp = 200;
            world.add_entity(e)
        };
        let attacker = {
            let mut e = mir2_world::Entity::new(0, "atk".to_string(), 11, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        let observer = {
            let mut e = mir2_world::Entity::new(0, "obs".to_string(), 13, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
        let mut displays = downlink::DisplayTable::new();
        displays.set(
            monster,
            downlink::ActorDisplay::new(0, 0, 222, 0, 0, 255, Vec::new()),
        );
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "uo".into(),
                chr_name: "obs".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 902,
                actor_id: observer,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();

        let rec = mir2_world::TickRecord {
            attacks: vec![(
                attacker,
                monster,
                mir2_world::AttackOutcome {
                    power: 30,
                    hit: true,
                    damage: 20,
                    killed: false,
                    target_hp_after: 180,
                },
            )],
            ..Default::default()
        };
        drain_combat_downlink(&rec, &world, &displays, &online, &mut sessions, &gates);
        let mut bodies = Vec::new();
        while let Ok(m) = rx0.try_recv() {
            if let SimToGate::SendFrame { socket, body } = m {
                assert_eq!(socket, 902, "只有观察者在线 ⇒ 只给他发");
                bodies.push(body);
            }
        }
        assert_eq!(
            bodies.len(),
            1,
            "怪物受击 ⇒ 视野内玩家收 1 帧 SM_STRUCK（旧实现 = 0 帧）"
        );
        let cm = CommandMessage::from_bytes(&bodies[0]).expect("12B");
        assert_eq!(
            u16::from_le_bytes([bodies[0][4], bodies[0][5]]),
            mir2_protocol::messages::SM_STRUCK
        );
        // 生产默认 = 客户端 `HIGHHP` 布局（`param=LoWord(HP)=180, tag=HiWord(HP)=0`）。
        let want = if game_svr::smframe::client_layout_enabled() {
            (monster, 180, 0, 20)
        } else {
            (monster, 180, 200, 20)
        };
        assert_eq!(
            (cm.recog, cm.param, cm.tag, cm.series),
            want,
            "SM_STRUCK 头 = (怪物, 扣血后 HP, MaxHP, 伤害)"
        );
    }

    /// **M3⑨-3 帧面残余**：`rec.slave_self_deliveries` 里一条 `RM_MASTERDIEMUTINY` ⇒
    /// `drain_slave_mutiny_downlink` 给**叛变宝宝视野内的在线玩家**发
    /// `SM_CHANGENAMECOLOR(656)`（无尾，`Param = GetChrColor`）与 `SM_USERNAME(42)`
    /// （尾 = `EncodeString(FilterShowName(ChrName))`）。
    ///
    /// 锚：`PlayObject.Base.cs:351-352`（`RefNameColor(); RefShowName();`）→
    /// `BaseObject.cs:730-733/1174-1177` → `PlayObject.Message.cs:1662-1664/1677-1679`。
    ///
    /// 红检：删掉 `drain_slave_mutiny_downlink` 的名帧（或把色帧/名帧互换）⇒ 本用例必红（帧序/条数）。
    #[test]
    fn slave_mutiny_downlink_emits_namecolor_then_username() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let master = {
            let mut e = mir2_world::Entity::new(0, "master".to_string(), 9, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.death = true;
            world.add_entity(e)
        };
        // 叛变后的宝宝：`Master == null`（`process_slave_mutiny` 已置）⇒ `GetChrColor` = `NameColor`(255)。
        let slave = {
            let mut e = mir2_world::Entity::new(0, "骷髅2".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_MONSTER;
            world.add_entity(e)
        };
        let near = {
            let mut e = mir2_world::Entity::new(0, "near".to_string(), 12, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        let far = {
            let mut e = mir2_world::Entity::new(0, "far".to_string(), 40, 40);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
        let mut online: OnlineRegistry = Vec::new();
        for (i, (name, socket, actor)) in [("near", 901_i32, near), ("far", 902, far)]
            .into_iter()
            .enumerate()
        {
            online_set(
                &mut online,
                OnlineEntry {
                    account: format!("u{i}"),
                    chr_name: name.into(),
                    session_id: i as i32 + 1,
                    gate_idx: 0,
                    socket,
                    actor_id: actor,
                },
            );
        }
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        let rec = mir2_world::TickRecord {
            slave_self_deliveries: vec![mir2_world::slave::SelfDelayOutcome {
                actor_id: master,
                ident: mir2_world::slave::RM_MASTERDIEMUTINY,
                mutinied: vec![slave],
                ghosted: Vec::new(),
            }],
            ..Default::default()
        };
        let cfg = mir2_data::pet_slave::PetSlaveConfig::default();
        let sent = drain_slave_mutiny_downlink(&rec, &world, &online, &gates, &cfg, 12);
        assert_eq!(sent, 2, "只给视野内玩家（near）发 2 帧");
        let mut bodies: Vec<(i32, Vec<u8>)> = Vec::new();
        while let Ok(m) = rx0.try_recv() {
            if let SimToGate::SendFrame { socket, body } = m {
                bodies.push((socket, body));
            }
        }
        assert!(
            bodies.iter().all(|(s, _)| *s == 901),
            "远距（40,40）观察者不得收帧：{bodies:?}"
        );
        assert_eq!(bodies.len(), 2, "色帧 + 名帧");
        let cm0 = CommandMessage::from_bytes(&bodies[0].1).expect("12B");
        assert_eq!(cm0.ident, mir2_protocol::messages::SM_CHANGENAMECOLOR);
        assert_eq!(
            (cm0.recog, cm0.param, cm0.tag, cm0.series),
            (slave, 255, 0, 0)
        );
        assert_eq!(bodies[0].1.len(), 12, "SM_CHANGENAMECOLOR 无尾");
        let cm1 = CommandMessage::from_bytes(&bodies[1].1).expect("12B");
        assert_eq!(cm1.ident, mir2_protocol::messages::SM_USERNAME);
        assert_eq!(
            (cm1.recog, cm1.param, cm1.tag, cm1.series),
            (slave, 255, 0, 0)
        );
        assert_eq!(
            mir2_protocol::edcode::decode(&bodies[1].1[12..]),
            downlink::gbk("骷髅"),
            "尾 = EncodeString(FilterShowName(ChrName))（“骷髅2” ⇒ “骷髅”）"
        );
    }

    /// **M3⑨-3 帧面残余（出队序，reviewer-6 F-1）**：多只宝宝叛变时，按观察者 `MsgQueue` 的
    /// **堆出队序**（`BaseObject.Message.cs:181` 的 `(byte)wIdent` 入队 + `ActorEntity.cs:16` 的
    /// `PriorityQueue`）—— 优先级字节 154(色) < 165(名) ⇒ 所有色帧先于所有名帧；但**同优先级不是
    /// 入队序**（.NET 8 为 4 叉堆）⇒ mutinied `[s2,s1]` 的两条名帧出队为 `名(s1),名(s2)`（**反序**）。
    ///
    /// 堆算法逐位对照见 [`game_svr::msgqueue`] 的 `matches_dotnet_priority_queue_dequeue_order`
    /// （n=1..12 与 net8.0.26 实跑 `dotnet` 对齐）。
    ///
    /// 红检：① `msgqueue` 换成「按优先级稳定排序」（入队序）⇒ 名帧序反过来，本断言必红；
    ///       ② 删名帧 / 把色帧名帧互换 ⇒ 帧数/种类必红。
    #[test]
    fn slave_mutiny_downlink_follows_msgqueue_heap_order() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let master = {
            let mut e = mir2_world::Entity::new(0, "master".to_string(), 9, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.death = true;
            world.add_entity(e)
        };
        let mut slaves = Vec::new();
        for (name, x) in [("s1", 10_i32), ("s2", 11)] {
            let mut e = mir2_world::Entity::new(0, name.to_string(), x, 10);
            e.race = mir2_world::ACTOR_RACE_MONSTER;
            slaves.push(world.add_entity(e));
        }
        let obs = {
            let mut e = mir2_world::Entity::new(0, "obs".to_string(), 12, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "uo".into(),
                chr_name: "obs".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 905,
                actor_id: obs,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);

        // `mutinied` 序 = `SlaveList` 末尾往前 ⇒ [s2, s1]。
        let rec = mir2_world::TickRecord {
            slave_self_deliveries: vec![mir2_world::slave::SelfDelayOutcome {
                actor_id: master,
                ident: mir2_world::slave::RM_MASTERDIEMUTINY,
                mutinied: vec![slaves[1], slaves[0]],
                ghosted: Vec::new(),
            }],
            ..Default::default()
        };
        let cfg = mir2_data::pet_slave::PetSlaveConfig::default();
        let sent = drain_slave_mutiny_downlink(&rec, &world, &online, &gates, &cfg, 12);
        assert_eq!(sent, 4, "2 只宝宝 × (色 + 名)");
        let mut idents: Vec<(u16, i32)> = Vec::new();
        while let Ok(m) = rx0.try_recv() {
            if let SimToGate::SendFrame { body, .. } = m {
                let cm = CommandMessage::from_bytes(&body).expect("12B");
                idents.push((cm.ident, cm.recog));
            }
        }
        assert_eq!(
            idents,
            vec![
                (mir2_protocol::messages::SM_CHANGENAMECOLOR, slaves[1]),
                (mir2_protocol::messages::SM_CHANGENAMECOLOR, slaves[0]),
                (mir2_protocol::messages::SM_USERNAME, slaves[0]),
                (mir2_protocol::messages::SM_USERNAME, slaves[1]),
            ],
            "入队序 色(s2),名(s2),色(s1),名(s1) ⇒ oracle 4 叉堆出队 色(s2),色(s1),名(s1),名(s2)"
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

    /// **52 帧尾的布局选路（`ability_tail`）**：生产默认 `client_layout=true` ⇒ 冻结客户端 58B
    /// `TAbility`（`HP@22` / `MaxHP@30`）；`false` ⇒ oracle 40B `EncodeMessage(WAbil)`。
    ///
    /// 红检（改坏必红）：`ability_tail` 两支互换（或恒走 `encode_ability`）⇒
    /// `oracle.len() == 40` / `client.len() == 58` / `client[22..26] == hp` 必红。
    #[test]
    fn ability_tail_switches_between_client_58_and_oracle_40() {
        let mut w = test_ability();
        w.hp = 19;
        w.mp = 15;
        w.max_hp = 19;
        w.max_mp = 15;
        let oracle = ability_tail(&w, false);
        let client = ability_tail(&w, true);
        assert_eq!(oracle.len(), 40, "oracle 支 = 40B EncodeMessage(WAbil)");
        assert_eq!(client.len(), 58, "client 支 = 58B TAbility");
        assert_eq!(
            i32::from_le_bytes(client[22..26].try_into().unwrap()),
            i32::from(w.hp),
            "58B 布局的 HP 在 offset 22（`Grobal2.pas:2167-2184`）"
        );
        assert_eq!(
            i32::from_le_bytes(client[30..34].try_into().unwrap()),
            i32::from(w.max_hp),
            "58B 布局的 MaxHP 在 offset 30"
        );
        assert_ne!(oracle, client, "两种布局不得同字节");
    }

    /// **D②（升级链下行）**：一条 `leveled=true` 的 `exp_awards` ⇒ 帧序按 oracle `MsgQueue`
    /// **出队序**（优先级字节升序）
    /// `SM_HEALTHSPELLCHANGED(53) → SM_WINEXP(44) → SM_LEVELUP(45) → SM_ABILITY(52) → SM_SUBABILITY(752)`
    /// （reviewer-6 `c5a90bf4` F-1：53 的优先级字节 68 < 44 的 139 < 45 的 150），
    /// 不是源码入队序 44→45→53。`SM_LEVELUP` 头 = `(total_exp, new_level, 0, 0)`；
    /// `RecalcLevelAbilitys()+RecalcAbilitys()` 把 `MaxHP/MaxMP` 从 0 提到等级派生值，然后
    /// `IncHealthSpell(2000,2000)` 按**新** `MaxHP/MaxMP` 截断 ⇒ 满血满蓝；世界实体同步到同一 `WAbil`。
    ///
    /// 红检：① 交换 `sm_levelup` 的 `recog`/`param` ⇒ `SM_LEVELUP` 头断言必红；
    /// ② 删 `drain_levelup_healthspell` 的 `IncHealthSpell(2000,2000)` ⇒ `hp == max_hp` 必红；
    /// ③ 不调 `recalc_session_ability` ⇒ `max_hp == 0` 必红；
    /// ④（F-1 红检）交换 `drain_kill_exp_chain` 的前两行（把 53 落到 44 之后）
    ///   ⇒ `idents` 序断言必红。
    #[test]
    fn levelup_downlink_sends_levelup_frames_heals_and_syncs_entity() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let killer = {
            let mut e = mir2_world::Entity::new(0, "killer".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
        {
            let e = world.entities.get_mut(killer).unwrap();
            e.exp = 500;
            e.level = 11;
            e.max_exp = 1000;
            e.hp = 10;
            e.max_hp = 20;
        }
        let displays = downlink::DisplayTable::new();
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "uk".into(),
                chr_name: "killer".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 903,
                actor_id: killer,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut sg = test_session_ground(killer);
        sg.equip.abil.level = 10; // drain_combat_downlink 会把它同步成 11
        sg.state.wabil.level = 10;
        sg.state.wabil.hp = 10; // 升级前残血
        sg.state.wabil.mp = 5;
        sessions.insert("killer".into(), sg);

        let rec = mir2_world::TickRecord {
            exp_awards: vec![(killer, 500, true, 11)],
            ..Default::default()
        };
        // 与 `sim_loop` 同一条链：oracle 出队序 53(68) → 44(139) → 45(150)/52/752。
        drain_kill_exp_chain(
            &rec,
            &mut world,
            &displays,
            &online,
            &mut sessions,
            &gates,
            &game_svr::ability::StdItemCatalog::new(),
            12,
            true, // 生产默认 = 冻结客户端 58B（`ability_tail`）
        );
        let mut bodies: Vec<Vec<u8>> = Vec::new();
        let mut frames = Vec::new();
        while let Ok(SimToGate::SendFrame { socket, body }) = rx0.try_recv() {
            assert_eq!(socket, 903);
            frames.push(CommandMessage::from_bytes(&body).expect("12B"));
            bodies.push(body);
        }
        let idents: Vec<u16> = frames.iter().map(|cm| cm.ident).collect();
        assert_eq!(
            idents,
            vec![
                mir2_protocol::messages::SM_HEALTHSPELLCHANGED,
                mir2_protocol::messages::SM_WINEXP,
                mir2_protocol::messages::SM_LEVELUP,
                mir2_protocol::messages::SM_ABILITY,
                mir2_protocol::messages::SM_SUBABILITY,
            ],
            "升级帧序 = HEALTHSPELLCHANGED(53) → WINEXP(44) → LEVELUP(45) → ABILITY(52) → SUBABILITY(752)"
        );
        let lv = &frames[2];
        assert_eq!((lv.recog, lv.param, lv.tag, lv.series), (500, 11, 0, 0));
        let sg = &sessions["killer"];
        assert!(
            sg.state.wabil.max_hp > 0,
            "RecalcLevelAbilitys+RecalcAbilitys 必须给 MaxHP 等级派生值（非 0）"
        );
        assert!(
            sg.state.wabil.hp == sg.state.wabil.max_hp,
            "IncHealthSpell(2000,2000) 应按升级后的 MaxHP 截断 ⇒ 满血: {:?}",
            sg.state.wabil
        );
        assert_eq!(sg.state.wabil.mp, sg.state.wabil.max_mp, "MP 同样满额");
        assert_eq!(sg.equip.abil.level, 11, "会话基能力等级 = 升级后");
        // F-2：`GetExp` 的两处 `AddBodyLuck` 按 oracle 次序落账：500*0.002 + 100 = 101.0；
        // `BodyLuckLevel = Convert.ToInt32(101/5000) = 0`。
        assert_eq!(
            sg.equip.body_luck, 101.0,
            "GetExp: dwExp*0.002（1.0）与升级 +100 均须累加"
        );
        assert_eq!(sg.equip.body_luck_level, 0);
        // 52 的尾 = **冻结客户端 58B `TAbility`**（升级重算后的值；生产默认 `client_layout=true`）。
        // bodies = [53,44,45,52,752] ⇒ 52 在 index 3。oracle 40B 支由
        // `record::encode_ability_matches_golden_frame_12_tail` 与 `ability_tail` 单测钉住。
        assert_eq!(
            &bodies[3][12..],
            &mir2_protocol::edcode::encode(&record::encode_ability_client(&sg.state.wabil)),
            "SM_ABILITY 尾 = Encode(58B TAbility)"
        );
        // 世界实体（战斗权威值）同步。
        let e = world.entities.get(killer).unwrap();
        assert_eq!(e.level, 11);
        assert!(e.max_hp > 0 && e.hp == e.max_hp, "实体 HP/MaxHP 同步");
        assert_eq!(e.max_hp, sg.state.wabil.max_hp);
    }

    /// **F-1（同 tick 去重）**：恢复段已产生 53 的对象，若本 tick 进升级块，
    /// `SendUpdateMsg(RM_HEALTHSPELLCHANGED)`（`BaseObject.Message.cs:275-304`）会把恢复段入队的 53
    /// 移除 ⇒ `drain_recovery` 不得回帧（升级段由 `drain_levelup_healthspell` 出唯一一条）。
    ///
    /// 红检：删掉 `drain_recovery` 的 `superseded_healthspell.contains(...)` 分支 ⇒ 本用例必红（① 收到帧）。
    #[test]
    fn recovery_healthspell_is_superseded_by_levelup_in_same_tick() {
        use std::collections::{HashMap, HashSet};
        let mut world = mir2_world::World::new(32, 32);
        let hid = {
            let mut e = mir2_world::Entity::new(0, "hero".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
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

        // 「恢复段必命中」的会话：池里 20 点 IncHealth、HP 未满、tick 初值 0（真实时钟远超阈值）。
        let mk = || {
            let mut sg = test_session_ground(hid);
            sg.state.wabil.hp = 10;
            sg.state.wabil.max_hp = 100;
            sg.recovery = game_svr::recover::Recovery::new(0);
            sg.recovery.inc_health = 20;
            sg
        };

        // ① 在「本 tick 进升级块」集合里 ⇒ 不回帧（被升级段的 53 取代），但状态照常消费。
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert("hero".into(), mk());
        let superseded: HashSet<i32> = [hid].into_iter().collect();
        drain_recovery(&mut sessions, &world, &online, &gates, 12, &superseded);
        assert!(
            rx0.try_recv().is_err(),
            "升级块取代恢复段 53 ⇒ 本段不得回帧"
        );
        assert_eq!(
            sessions["hero"].recovery.inc_health, 15,
            "HP/MP 与 Inc* 的状态变更仍照常发生（20 - PerHealth 5）"
        );

        // ② 不在集合里 ⇒ 照常回帧（自见一条）。
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert("hero".into(), mk());
        drain_recovery(&mut sessions, &world, &online, &gates, 12, &HashSet::new());
        let (cm, frame_body) = match rx0.try_recv() {
            Ok(SimToGate::SendFrame { body, socket }) => {
                assert_eq!(socket, 901);
                (CommandMessage::from_bytes(&body).expect("12B"), body)
            }
            _ => panic!("未取代时必须有 53 帧"),
        };
        assert_eq!(cm.ident, mir2_protocol::messages::SM_HEALTHSPELLCHANGED);
        // **①/② 真机根因（卡 `openmir2-client-e2e-enter-gaps`）**：冻结客户端 `HIGHHP=True`
        // （`Grobal2.pas:39`）的 53 支从**尾**读 `TCharDesc`（`ClMain.pas:10220`）⇒ 生产默认
        // （`client_layout_enabled()`）必须带 12B 尾，否则客户端 HUD 归零、`CanRun` 拒绝跑。
        // 红检：把 `sm_healthspellchanged_frame` 的默认支改回 header-only ⇒ 下面断言必红。
        if game_svr::smframe::client_layout_enabled() {
            let desc = mir2_protocol::edcode::decode(&frame_body[12..]);
            assert_eq!(
                desc.len(),
                12,
                "53 尾 = TCharDesc（12B，HIGHHP 客户端要求）"
            );
            let le = |o: usize| i32::from_le_bytes(desc[o..o + 4].try_into().unwrap());
            assert_eq!(
                le(0),
                i32::from(sessions["hero"].state.wabil.hp),
                "Feature = HP"
            );
            assert_eq!(
                le(4),
                i32::from(sessions["hero"].state.wabil.mp),
                "Status = MP"
            );
            assert_eq!(
                le(8),
                i32::from(sessions["hero"].state.wabil.max_hp),
                "StatusEx = MaxHP"
            );
        }
    }

    /// **F-2（`AddBodyLuck`）**：`PlayObject.AddBodyLuck`（`PlayObject.cs:4304-4323`）的纯函数口径——
    /// 两个 `±5*BODYLUCKUNIT` 累加门 + `Convert.ToInt32`（**银行家舍入**）派生 `BodyLuckLevel`
    /// （clamp `[-10,5]`）。`BodyLuck` 是 `characters.BodyLuck double(10,3)`，被 `scoped_diff` 逐字节比。
    ///
    /// 红检：把 `round_ties_even()` 换成 `round()` ⇒ `2500/5000=0.5` 的用例必红（期望 0，实得 1）。
    #[test]
    fn add_body_luck_matches_oracle_vectors() {
        // (初值, dLuck, 期望 BodyLuck, 期望 BodyLuckLevel)
        let cases: &[(f64, f64, f64, i32)] = &[
            (0.0, 0.0, 0.0, 0),
            (2500.0, 0.0, 2500.0, 0), // Convert.ToInt32(0.5) = 0（midpoint-to-even）
            (7500.0, 0.0, 7500.0, 2), // 1.5 → 2
            (-2500.0, 0.0, -2500.0, 0), // -0.5 → 0
            (24950.0, 100.0, 25050.0, 5), // 5.01 → clamp 5
            (25000.0, 100.0, 25000.0, 5), // 正门：>= 5*unit ⇒ 不加
            (25050.0, -100.0, 24950.0, 5), // 负向可加（> -5*unit）
            (-24950.0, -100.0, -25050.0, -5), // -5.01 → -5
            (-25000.0, -100.0, -25000.0, -5), // 负门：<= -5*unit ⇒ 不加
            (0.0, 24999.0, 24999.0, 5), // 4.9998 → 5
        ];
        for (init, d, exp_luck, exp_level) in cases {
            let mut luck = *init;
            let mut level = 0;
            add_body_luck(&mut luck, &mut level, *d);
            assert_eq!(luck, *exp_luck, "BodyLuck: init={init} d={d}");
            assert_eq!(level, *exp_level, "BodyLuckLevel: init={init} d={d}");
        }
    }

    /// **F-2（落库）**：击杀后会话的 `BodyLuck` 必须写进 `characters.BodyLuck`（`MakeSaveRcd` 口径）。
    /// `drain_combat_downlink` 按 oracle 次序（`dwExp*0.002` 然后升级 `+100`）累加 ⇒
    /// `build_save_record` 原样带出（`WorldServer.cs:1290 save playerInfo.BodyLuck = playObject.BodyLuck`）。
    ///
    /// 红检：把 `rec.data.body_luck = sg.equip.body_luck` 删掉 ⇒ 存档断言必红（保留加载值）。
    #[test]
    fn body_luck_accumulates_on_getexp_and_persists() {
        use std::collections::HashMap;
        let mut world = mir2_world::World::new(64, 64);
        let killer = {
            let mut e = mir2_world::Entity::new(0, "killer".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            world.add_entity(e)
        };
        world.tick();
        {
            let e = world.entities.get_mut(killer).unwrap();
            e.exp = 100;
            e.level = 11;
            e.max_exp = 1000;
        }
        let displays = downlink::DisplayTable::new();
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "uk".into(),
                chr_name: "killer".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 905,
                actor_id: killer,
            },
        );
        let (tx0, _rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut sg = test_session_ground(killer);
        sg.equip.abil.level = 10;
        sg.state.wabil.level = 10;
        sessions.insert("killer".into(), sg);

        // 非升级（`leveled=false`）⇒ 只加 `dwExp*0.002 = 5000`；`BodyLuckLevel = 1`。
        let rec = mir2_world::TickRecord {
            exp_awards: vec![(killer, 2_500_000, false, 11)],
            ..Default::default()
        };
        drain_combat_downlink(&rec, &world, &displays, &online, &mut sessions, &gates);
        let sg = sessions.get("killer").unwrap();
        assert_eq!(sg.equip.body_luck, 5000.0, "dwExp*0.002 累加");
        assert_eq!(sg.equip.body_luck_level, 1, "Convert.ToInt32(5000/5000)=1");

        let saved = SavedHuman {
            account: "acct".into(),
            record: stage6_record(),
            enter_map: "0".into(),
        };
        let out = build_save_record(&saved, "killer", &world, sg, None);
        assert_eq!(
            out.data.body_luck, 5000.0,
            "characters.BodyLuck 必须带出会话的实时 BodyLuck"
        );
    }

    /// **⑤-4 返工 F-2**：`ClientRepairItem`（`PlayObject.Operate.cs:1585-1597`）的 **BUG-REPLICA**
    /// —— 未命中 ⇒ 修最后一件；空背包 ⇒ 无目标；命中 ⇒ 命中项（即使不是最后一件）。
    /// 改坏（把 `or_else(...last)` 删掉、改成未命中即 return）⇒ 第一/三两个断言红。
    #[test]
    fn repair_target_index_replicates_oracle_last_item_bug() {
        let mut cat = game_svr::ability::StdItemCatalog::new();
        cat.insert(
            1,
            mir2_data::models::StdItem {
                name: "金创药(小量)".to_string(),
                ..Default::default()
            },
        );
        let item = |make_index: i32, index: u16| game_svr::bag::BagItem {
            make_index,
            index,
            dura: 0,
            dura_max: 0,
            desc: [0; 14],
        };
        let list = vec![item(9, 1), item(10, 1), item(11, 1)];
        // 未命中 ⇒ 最后一件（下标 2），不是 None。
        assert_eq!(
            repair_target_index(&list, &cat, 999, "金创药(小量)"),
            Some(2),
            "oracle bug：未命中 ⇒ 修最后一件"
        );
        // 命中第 0 件（名字 `OrdinalIgnoreCase`）。
        assert_eq!(repair_target_index(&list, &cat, 9, "金创药(小量)"), Some(0));
        // 空背包 ⇒ None（oracle `userItem` 仍为 null）。
        assert_eq!(repair_target_index(&[], &cat, 9, "金创药(小量)"), None);
    }

    /// **⑤-4 返工（自报 Important）**：shop 帧的线上尾 = `EDCode.EncodeString(GBK 原文)`
    /// （oracle `SendDefMessage` 非空尾走 `SendSocket(ClientMsg, EDCode.EncodeString(sMsg))`，
    /// `PlayObject.Socket.cs:26-43`）；空尾保持空（header-only 帧不变）。
    /// 改坏（退回 `&f.tail` 裸拼）⇒ 中间两个断言红。
    #[test]
    fn emit_shop_frames_edcodes_tail_like_send_def_message() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tail = game_svr::downlink::gbk("商人乙/对不起!我不能帮你修理这个物品。");
        let frames = vec![
            game_svr::shop::ShopFrame {
                ident: mir2_protocol::messages::SM_MERCHANTSAY,
                recog: 5_555,
                param: 0,
                tag: 0,
                series: 1,
                tail: tail.clone(),
                tail_encoded: false,
            },
            game_svr::shop::ShopFrame {
                ident: mir2_protocol::messages::SM_USERREPAIRITEM_FAIL,
                recog: 0,
                param: 0,
                tag: 0,
                series: 0,
                tail: Vec::new(),
                tail_encoded: false,
            },
        ];
        emit_shop_frames(&tx, 7, &frames);
        let (sock1, body1) = match rx.try_recv().expect("第一帧") {
            SimToGate::SendFrame { socket, body } => (socket, body),
            _ => panic!("非 SendFrame"),
        };
        let (sock2, body2) = match rx.try_recv().expect("第二帧") {
            SimToGate::SendFrame { socket, body } => (socket, body),
            _ => panic!("非 SendFrame"),
        };
        assert_eq!((sock1, sock2), (7, 7));
        assert_eq!(
            &body1[..12],
            &game_svr::smframe::command_header(
                5_555,
                mir2_protocol::messages::SM_MERCHANTSAY,
                0,
                0,
                1
            )
        );
        assert_eq!(&body1[12..], &mir2_protocol::edcode::encode(&tail));
        assert_ne!(&body1[12..], &tail[..], "线上尾不是裸 GBK（EDCode 会变换）");
        // 空尾：仍只发 12B 头（`EncodeString("")` 不追加字节）。
        assert_eq!(
            &body2[..12],
            &game_svr::smframe::command_header(
                0,
                mir2_protocol::messages::SM_USERREPAIRITEM_FAIL,
                0,
                0,
                0
            )
        );
        assert_eq!(body2.len(), 12, "空尾 ⇒ header-only 不变");
    }

    /// **M3⑦-实现 4**：`CM_PICKUP` 的归属组队分支接上真注册表
    /// （oracle `PlayObject.Base.cs:38-54` `ClientPickUpItemIsOfGroup`）：非组员不可拾、
    /// 同组成员可拾。
    ///
    /// 改坏必红：把 `handle_ground_cmd` 里的 `group_owner`/`group_member_actor_ids` 换回
    /// `0`/`&[]` ⇒ 情形 B 收不到 `SM_GOLDCHANGED`、会话金币不变，两条断言必红。
    #[test]
    fn pickup_group_attribution_uses_registry() {
        use std::collections::HashMap;

        let mut world = mir2_world::World::new(32, 32);
        world.item_catalog = vec![mir2_data::StdItem {
            name: "金币".into(),
            ..Default::default()
        }];
        let hero = world.add_entity({
            let mut e = mir2_world::Entity::new(1, "hero", 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e
        });
        let mate = world.add_entity(mir2_world::Entity::new(2, "mate", 11, 10));
        let (gx, gy) = (11, 10);
        let count = 300;
        // 地面金币归属 = mate（`MapItem.OfBaseObject`）。
        assert!(world.maps[0].add_floor_item(
            gx,
            gy,
            mate,
            mir2_world::map::FloorItemMeta {
                kind: mir2_world::map::FloorItemKind::Gold,
                index: 0,
                count,
                item_id: mate,
                of_base_object: mate,
            },
            0,
        ));
        {
            let e = world.entities.get_mut(hero).expect("hero 必在实体表");
            e.x = gx;
            e.y = gy;
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
                storage: storage::StorageState::new(),
                actor_id: hero,
                game_gold: 0,
                soft_version_date: 0,
                game_point: 0,
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
                actor_id: hero,
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

        // 情形 A：hero 与 mate **未组队** ⇒ 归属既非自己也非组员 ⇒ 拒绝（无帧、金币不变）。
        handle_ground_cmd(
            game_svr::ground::CM_PICKUP,
            u16::try_from(gx).unwrap(),
            u16::try_from(gy).unwrap(),
            None,
            "hero",
            901,
            &std::collections::HashMap::new(),
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
            "未组队 ⇒ 不得拾取他人归属物（oracle Base.cs:57-63）"
        );
        assert_eq!(sessions.get("hero").unwrap().state.gold, 0);

        // 情形 B：hero 与 mate 同组（hero 团长）⇒ `ClientPickUpItemIsOfGroup(mate)` 成立 ⇒ 可拾。
        world.entities.get_mut(mate).unwrap().allow_group = true;
        let _ = world.party_create_group(hero, "mate", &mir2_world::PartyConfig::default());
        assert_eq!(world.entities.get(hero).unwrap().group_owner, hero);
        assert_eq!(world.entities.get(mate).unwrap().group_owner, hero);
        handle_ground_cmd(
            game_svr::ground::CM_PICKUP,
            u16::try_from(gx).unwrap(),
            u16::try_from(gy).unwrap(),
            None,
            "hero",
            901,
            &std::collections::HashMap::new(),
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
                assert_eq!(socket, 901);
                body
            }
            other => panic!("组员归属物应可拾并回 SM_GOLDCHANGED，实收 {other:?}"),
        };
        let cm = mir2_protocol::frame::CommandMessage::from_bytes(&body).expect("body >= 12B");
        assert_eq!(cm.ident, mir2_protocol::messages::SM_GOLDCHANGED);
        assert_eq!(cm.recog, count);
        assert_eq!(sessions.get("hero").unwrap().state.gold, count);
    }

    /// **M3⑦-实现 4**：`CM_TAKEOFFITEM` 的 `Dealing` 守卫接上真交易态
    /// （oracle `PlayObject.Operate.cs:501` `if (!Dealing && btWhere < 13)`）。
    ///
    /// 改坏必红：把 `handle_bag_cmd` 的 `dealing` 换回 `false` ⇒ 卸装成功、物品从 `UseItems`
    /// 移除，两条断言（`recog == -1` / `use_items[0].is_some()`）必红。
    #[test]
    fn bag_takeoff_blocked_while_dealing() {
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
                storage: storage::StorageState::new(),
                actor_id: 42,
                game_gold: 0,
                soft_version_date: 0,
                game_point: 0,
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

        // `recog=60001`（MakeIndex）、`param=0`（槽位）、`dealing=true`。
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
            true,
            false,
        );
        let mut fail_seen = false;
        while let Ok(m) = rx0.try_recv() {
            if let SimToGate::SendFrame { body, .. } = m {
                let cm = mir2_protocol::frame::CommandMessage::from_bytes(&body).expect(">=12B");
                if cm.ident == mir2_protocol::messages::SM_TAKEOFF_FAIL {
                    assert_eq!(
                        cm.recog, -1,
                        "Dealing ⇒ n10 = -1（oracle Operate.cs:501/535）"
                    );
                    fail_seen = true;
                }
            }
        }
        assert!(fail_seen, "交易中卸装必须回 SM_TAKEOFF_FAIL(-1)");
        assert!(
            sessions.get("hero").unwrap().use_items[0].is_some(),
            "交易中不得真正卸下装备"
        );
    }

    /// **M3⑦-实现 4（M-1 收口）**：`deal_last_tick` 的**生产取数**被用例咬住——
    /// 交易会话 `DealLastTick` 未过 3000ms 门 ⇒ `CM_DROPITEM` 不得进 `ClientDropItem` 体。
    ///
    /// oracle：`PlayObject.Operate.cs:269`（`(HUtil32.GetTickCount() - DealLastTick) > 3000`
    /// 才进 `ItemList` 丢弃查表）；`DealLastTick` 字段见 `PlayObject.cs:122`。本仓
    /// `drop_item_down` 恒 false（`MapItem.ItemId == 0`，T-19），
    /// 故「是否进体」在成功/失败帧上不可分；可分的观测点是丢弃体里 `StdMode == 40` 的
    /// `Dura = (ushort)(Dura - 2000)` 原地扣减（`BaseObject.cs:541-548`，u16 回绕口径）。
    ///
    /// 改坏必红：把 `handle_ground_cmd` 里 `deal_last_tick` 的取数改回恒 `0` ⇒ 第一次调用即
    /// 过门、`Dura` 从 5000 变 3000 ⇒ `5000` 断言必红。
    #[test]
    fn drop_blocked_by_fresh_deal_last_tick() {
        use std::collections::HashMap;

        let mut world = mir2_world::World::new(32, 32);
        world.clock_ms = 10_000;
        let hero = world.add_entity({
            let mut e = mir2_world::Entity::new(1, "hero", 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e
        });

        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert(
            "hero".into(),
            SessionGround {
                state: game_svr::ground::GroundState {
                    item_list: vec![game_svr::bag::BagItem {
                        make_index: 60001,
                        index: 1,
                        // `StdMode 40` 分支：`Dura 5000 - 2000 = 3000`。
                        dura: 5000,
                        dura_max: 5000,
                        desc: [0u8; 14],
                    }],
                    gold: 0,
                    wabil: test_ability(),
                },
                storage: storage::StorageState::new(),
                actor_id: hero,
                game_gold: 0,
                soft_version_date: 0,
                game_point: 0,
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
                actor_id: hero,
            },
        );
        let (tx0, _rx0) = mpsc::unbounded_channel::<SimToGate>();
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
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            1,
            mir2_data::StdItem {
                name: "木剑".into(),
                std_mode: 40,
                ..Default::default()
            },
        );

        // 交易会话：`DealLastTick = now_tick`（`handle_deal_cmd` 写的是 `world.clock_ms`）。
        let mut trade_sessions: HashMap<String, game_svr::trade::DealState> = HashMap::new();
        trade_sessions.insert(
            "hero".into(),
            game_svr::trade::DealState {
                deal_last_tick: world.clock_ms,
                ..Default::default()
            },
        );

        // ① 新鲜 `DealLastTick` ⇒ `now - DealLastTick > 3000` 不成立 ⇒ 不进丢弃体 ⇒ Dura 不变。
        handle_ground_cmd(
            game_svr::ground::CM_DROPITEM,
            60001,
            0,
            Some("木剑"),
            "hero",
            901,
            &trade_sessions,
            &world,
            &mut sessions,
            &online,
            &gates,
            &cfg,
            &colors,
            &catalog,
            None,
        );
        assert_eq!(
            sessions.get("hero").unwrap().state.item_list[0].dura,
            5000,
            "3000ms 门内（交易中）⇒ ClientDropItem 体不得执行（oracle Operate.cs:269）"
        );

        // ② 过门（`DealLastTick` 陈旧）⇒ 进体 ⇒ StdMode40 原地扣 2000（u16 口径 5000→3000）。
        trade_sessions.get_mut("hero").unwrap().deal_last_tick = 1_000;
        handle_ground_cmd(
            game_svr::ground::CM_DROPITEM,
            60001,
            0,
            Some("木剑"),
            "hero",
            901,
            &trade_sessions,
            &world,
            &mut sessions,
            &online,
            &gates,
            &cfg,
            &colors,
            &catalog,
            None,
        );
        assert_eq!(
            sessions.get("hero").unwrap().state.item_list[0].dura,
            3000,
            "过 3000ms 门 ⇒ 进体 ⇒ Dura 5000-2000（BaseObject.cs:541-548）"
        );
    }

    /// **`openmir2-m3-selloff-consignment-world` 进程级判据**：走**生产路径** `handle_selloff_cmd`，
    /// 让卖方 L（gate 0 / socket 5）与买方 T（gate 1 / socket 6）完成
    /// `CM_SELLOFFADDITEM → CM_SELLOFFEND → CM_SELLOFFBUY`，断言：
    /// ① 帧按 `[gate_idx, socket]` 路由到正确连接；
    /// ② 上架扣 `DecUserGameGold=10000`、成交时买方 `-= nSellGold+10000`、卖方 `+= nSellGold`；
    /// ③ 上架回帧出队序 = `SM_MENU_OK(767) → SM_SELLOFFEND_OK(23016)`（优先级 69 < 233）；
    /// ④ 买方 = 立即 `SM_ADDITEM(200)` + 排队 `SM_WEIGHTCHANGED(622) → SM_GAMEGOLDNAME(5008)`；
    /// ⑤ `M2Share.SellOffItemList`（进程级 `SellOffBook`）跨会话共享（成交后 `flag=2`）。
    ///
    /// 红检：把 `handle_selloff_cmd` 的 `CM_SELLOFFEND` 支去掉、或把 `client_buy_sell_off_item`
    /// 的首次空槽中止删掉（让 `SM_SELLOFFBUY_OK` 出现）⇒ 本用例必 FAILED。
    #[test]
    fn selloff_cm_dispatch_emits_oracle_frames_on_both_connections() {
        use mir2_protocol::messages;
        use std::collections::HashMap;

        fn drain(rx: &mut mpsc::UnboundedReceiver<SimToGate>) -> Vec<Vec<u8>> {
            let mut out = Vec::new();
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    SimToGate::SendFrame { body, .. } => out.push(body),
                    other => panic!("元宝寄售路径不得发 SendFrame 以外的命令，实得 {other:?}"),
                }
            }
            out
        }
        fn ident(body: &[u8]) -> u16 {
            u16::from_le_bytes([body[4], body[5]])
        }
        fn idents(frames: &[Vec<u8>]) -> Vec<u16> {
            frames.iter().map(|b| ident(b)).collect()
        }

        // 卖方 gate 0 / socket 5，买方 gate 1 / socket 6（按 `[gate_idx, socket]` 分流可区分）。
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);

        let online: OnlineRegistry = vec![
            OnlineEntry {
                account: "a".into(),
                chr_name: "L".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 5,
                actor_id: 1,
            },
            OnlineEntry {
                account: "b".into(),
                chr_name: "T".into(),
                session_id: 2,
                gate_idx: 1,
                socket: 6,
                actor_id: 2,
            },
        ];

        let weapon = game_svr::bag::BagItem {
            make_index: 100,
            index: 1,
            dura: 7,
            dura_max: 20,
            desc: [0; 14],
        };
        let mut seller = test_session_ground(1);
        seller.state.item_list.push(weapon);
        seller.state.wabil.weight = 0;
        seller.state.wabil.max_weight = 1000;
        seller.game_gold = 100_000;
        seller.soft_version_date = 1; // `SendGoldInfo` 门开（真客户端）
        let mut buyer = test_session_ground(2);
        buyer.state.wabil.weight = 0;
        buyer.state.wabil.max_weight = 1000;
        buyer.game_gold = 100_000;
        buyer.soft_version_date = 1;
        let mut ground_sessions: HashMap<String, SessionGround> = HashMap::new();
        ground_sessions.insert("L".into(), seller);
        ground_sessions.insert("T".into(), buyer);
        let mut sell_off_sessions: HashMap<String, game_svr::selloff::SellOffState> =
            HashMap::new();
        let mut book = game_svr::selloff::SellOffBook::default();
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            1,
            mir2_data::StdItem {
                name: "木剑".into(),
                std_mode: 5,
                shape: 1,
                weight: 3,
                dura_max: 20,
                price: 10,
                ..mir2_data::StdItem::default()
            },
        );
        let cfg = game_svr::selloff::SellOffConfig::default();
        let world = mir2_world::World::new(32, 32);

        // ① 卖方把一件背包物品加进寄售窗（`CM_SELLOFFADDITEM`，`recog=MakeIndex`、尾=名字）
        handle_selloff_cmd(
            messages::CM_SELLOFFADDITEM,
            100,
            0,
            0,
            Some("木剑"),
            "L",
            5,
            &online,
            &world,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
            &gates,
            &cfg,
            &catalog,
        );
        assert_eq!(
            idents(&drain(&mut rx0)),
            vec![messages::SM_SELLOFFADDITEM_OK]
        );
        assert!(drain(&mut rx1).is_empty(), "买方不应收到上架回帧");
        assert_eq!(
            sell_off_sessions["L"].sell_off_item_list.len(),
            1,
            "物品应移进寄售临时列表"
        );
        assert!(
            ground_sessions["L"].state.item_list.is_empty(),
            "物品应移出背包"
        );

        // ② 上架（`CM_SELLOFFEND`，`recog=nSellGold`、尾=买方名）
        handle_selloff_cmd(
            messages::CM_SELLOFFEND,
            1000,
            0,
            0,
            Some("T"),
            "L",
            5,
            &online,
            &world,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
            &gates,
            &cfg,
            &catalog,
        );
        assert_eq!(
            idents(&drain(&mut rx0)),
            vec![messages::SM_MENU_OK, messages::SM_SELLOFFEND_OK],
            "上架出队序：MENU_OK(69) 先于 SELLOFFEND_OK(233)"
        );
        assert!(drain(&mut rx1).is_empty());
        assert_eq!(book.entries.len(), 1, "进程级 SellOffItemList 追加一条");
        assert_eq!(book.entries[0].deal_chr_name, "L");
        assert_eq!(book.entries[0].buy_chr_name, "T");
        assert_eq!(book.entries[0].sell_gold, 1000);
        assert_eq!(book.entries[0].flag, 0);
        assert_eq!(
            ground_sessions["L"].game_gold, 90_000,
            "上架扣 DecUserGameGold=10000"
        );

        // ③ 买方成交（`CM_SELLOFFBUY`，尾=卖方名）
        handle_selloff_cmd(
            messages::CM_SELLOFFBUY,
            0,
            0,
            0,
            Some("L"),
            "T",
            6,
            &online,
            &world,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
            &gates,
            &cfg,
            &catalog,
        );
        // 买方：立即 SM_ADDITEM(200) + 排队 131(WEIGHT) → 148(GAMEGOLD)。
        let buyer_frames = idents(&drain(&mut rx1));
        assert_eq!(
            buyer_frames,
            vec![
                messages::SM_ADDITEM,
                messages::SM_WEIGHTCHANGED,
                messages::SM_GAMEGOLDNAME,
            ]
        );
        // 卖方：只收一条元宝变化帧（`GameGoldChanged`）。
        assert_eq!(idents(&drain(&mut rx0)), vec![messages::SM_GAMEGOLDNAME]);
        assert_eq!(
            ground_sessions["T"].game_gold, 89_000,
            "买方扣 nSellGold(1000)+DecUserGameGold(10000)"
        );
        assert_eq!(
            ground_sessions["L"].game_gold, 91_000,
            "卖方 += nSellGold(1000)"
        );
        assert_eq!(
            ground_sessions["T"].state.item_list.len(),
            1,
            "物品进买方背包"
        );
        assert_eq!(book.entries[0].flag, 2, "成交后 flag=2（已售）");
        // 缺陷照搬①：`for j<=9` 首个空槽中止 ⇒ `SM_SELLOFFBUY_OK` 不可达。
        assert!(
            !buyer_frames.contains(&messages::SM_SELLOFFBUY_OK),
            "oracle 越界缺陷 ⇒ BUY_OK 恒不可达"
        );
    }

    /// **元宝寄售取消路径进程级判据**：走生产路径，分别驱动
    /// ① `CM_SELLOFFADDITEM → CM_SELLOFFEND → CM_SELLOFFBUYCANCEL`（买方取消 ⇒ `flag=3`
    ///    以及文案 `"取消交易成功!"`）；
    /// ② `CM_SELLOFFADDITEM → CM_SELLOFFEND → CM_CANCELSELLOFFITEMING`（卖方取消 ⇒ 事务条目移除、
    ///    物品退回背包；出队序 = 立即 `SM_ADDITEM`×2 → `SM_MENU_OK(69)`×2 → `SM_WEIGHTCHANGED(131)`）；
    ///    注意 oracle **不 break** ⇒ 同一卖方名下所有 `flag∈{0,3}` 的条目**一次全清**（连①的 flag=3 也退）。
    /// ③ `CM_SELLOFFDELITEM` / `CM_SELLOFFCANCEL` 的派发不 panic 且路由正确。
    ///
    /// 红检：把 `handle_selloff_cmd` 的 `CM_SELLOFFBUYCANCEL` 支去掉（`flag` 不变 3）⇒ ①②
    /// 的 flag 断言必 FAILED。
    #[test]
    fn selloff_cancel_paths_dispatch_and_mutate_global_book() {
        use mir2_protocol::messages;
        use std::collections::HashMap;

        fn drain(rx: &mut mpsc::UnboundedReceiver<SimToGate>) -> Vec<u16> {
            let mut out = Vec::new();
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    SimToGate::SendFrame { body, .. } => {
                        out.push(u16::from_le_bytes([body[4], body[5]]));
                    }
                    other => panic!("元宝寄售路径不得发 SendFrame 以外的命令，实得 {other:?}"),
                }
            }
            out
        }

        let weapon = |mi: i32| game_svr::bag::BagItem {
            make_index: mi,
            index: 1,
            dura: 7,
            dura_max: 20,
            desc: [0; 14],
        };
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            1,
            mir2_data::StdItem {
                name: "木剑".into(),
                std_mode: 5,
                shape: 1,
                weight: 3,
                dura_max: 20,
                price: 10,
                ..mir2_data::StdItem::default()
            },
        );
        let cfg = game_svr::selloff::SellOffConfig::default();
        let world = mir2_world::World::new(32, 32);
        let online: OnlineRegistry = vec![
            OnlineEntry {
                account: "a".into(),
                chr_name: "L".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 5,
                actor_id: 1,
            },
            OnlineEntry {
                account: "b".into(),
                chr_name: "T".into(),
                session_id: 2,
                gate_idx: 1,
                socket: 6,
                actor_id: 2,
            },
        ];
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        gates.insert(1, tx1);

        let mut ground_sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut l = test_session_ground(1);
        l.state.wabil.weight = 0;
        l.state.wabil.max_weight = 1000;
        l.game_gold = 100_000;
        l.soft_version_date = 1;
        ground_sessions.insert("L".into(), l);
        let mut t = test_session_ground(2);
        t.game_gold = 100_000;
        t.soft_version_date = 1;
        ground_sessions.insert("T".into(), t);
        let mut sell_off_sessions: HashMap<String, game_svr::selloff::SellOffState> =
            HashMap::new();
        let mut book = game_svr::selloff::SellOffBook::default();

        // ① 买方取消：L 上架 → T 调 `CM_SELLOFFBUYCANCEL`（尾=卖方名）
        let call = |ident,
                    recog,
                    param,
                    tag,
                    tail: &str,
                    chr: &str,
                    socket,
                    ground: &mut HashMap<String, SessionGround>,
                    sessions: &mut HashMap<String, game_svr::selloff::SellOffState>,
                    book: &mut game_svr::selloff::SellOffBook| {
            handle_selloff_cmd(
                ident,
                recog,
                param,
                tag,
                Some(tail),
                chr,
                socket,
                &online,
                &world,
                ground,
                sessions,
                book,
                &gates,
                &cfg,
                &catalog,
            );
        };
        // 先给 L 背包放一件物品再上架。
        ground_sessions
            .get_mut("L")
            .unwrap()
            .state
            .item_list
            .push(weapon(100));
        call(
            messages::CM_SELLOFFADDITEM,
            100,
            0,
            0,
            "木剑",
            "L",
            5,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
        );
        assert_eq!(drain(&mut rx0), vec![messages::SM_SELLOFFADDITEM_OK]);
        call(
            messages::CM_SELLOFFEND,
            1000,
            0,
            0,
            "T",
            "L",
            5,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
        );
        let _ = drain(&mut rx0);
        assert_eq!(book.entries.len(), 1);
        call(
            messages::CM_SELLOFFBUYCANCEL,
            0,
            0,
            0,
            "L",
            "T",
            6,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
        );
        assert_eq!(
            drain(&mut rx1),
            vec![messages::SM_MENU_OK],
            "买方收取消回帧"
        );
        assert_eq!(book.entries[0].flag, 3, "买方取消 ⇒ Flag=3");

        // ② 卖方取消正在寄售中：重新上架一条 → L 调 `CM_CANCELSELLOFFITEMING`
        ground_sessions
            .get_mut("L")
            .unwrap()
            .state
            .item_list
            .push(weapon(200));
        call(
            messages::CM_SELLOFFADDITEM,
            200,
            0,
            0,
            "木剑",
            "L",
            5,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
        );
        let _ = drain(&mut rx0);
        call(
            messages::CM_SELLOFFEND,
            1000,
            0,
            0,
            "T",
            "L",
            5,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
        );
        let _ = drain(&mut rx0);
        call(
            messages::CM_CANCELSELLOFFITEMING,
            0,
            0,
            0,
            "",
            "L",
            5,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
        );
        assert_eq!(
            drain(&mut rx0),
            vec![
                messages::SM_ADDITEM,
                messages::SM_ADDITEM,
                messages::SM_MENU_OK,
                messages::SM_MENU_OK,
                messages::SM_WEIGHTCHANGED,
            ],
            "取消寄售中：oracle 不 break ⇒ ①的 flag=3 条目也一并退回（2 次 AddItem/MENU_OK），\
             且 MENU_OK(69) 先于 WEIGHTCHANGED(131)"
        );
        assert!(
            book.entries.is_empty(),
            "同一卖方名下 flag∈{{0,3}} 的条目全部移除"
        );
        assert_eq!(
            ground_sessions["L"].state.item_list.len(),
            2,
            "两条目的物品都退回背包"
        );

        // ③ `CM_SELLOFFDELITEM` / `CM_SELLOFFCANCEL` 的派发（各回一帧）
        call(
            messages::CM_SELLOFFDELITEM,
            200,
            0,
            0,
            "木剑",
            "L",
            5,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
        );
        assert_eq!(
            drain(&mut rx0),
            vec![messages::SM_SELLOFFDELITEM_FAIL],
            "列表里无此件 ⇒ FAIL"
        );
        ground_sessions
            .get_mut("L")
            .unwrap()
            .state
            .item_list
            .push(weapon(300));
        call(
            messages::CM_SELLOFFADDITEM,
            300,
            0,
            0,
            "木剑",
            "L",
            5,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
        );
        let _ = drain(&mut rx0);
        call(
            messages::CM_SELLOFFCANCEL,
            0,
            0,
            0,
            "",
            "L",
            5,
            &mut ground_sessions,
            &mut sell_off_sessions,
            &mut book,
        );
        assert_eq!(drain(&mut rx0), vec![messages::SM_SellOffCANCEL]);
        assert!(sell_off_sessions["L"].sell_off_item_list.is_empty());
    }

    /// ⑤-4 第②片：`tail_encoded == true` 的帧（`SM_SAVEITEMLIST` 分页 / `SM_ADDITEM` 尾）走 oracle
    /// `SendSocket(ClientMsg, sMsg)` **直连**（`PlayObject.Socket.cs:104-127`），尾**不再**施加
    /// `EncodeString`。改坏（`emit_shop_frames` 忽略 `tail_encoded`、一律 `edcode::encode`）⇒ 本用例红。
    #[test]
    fn emit_shop_frames_keeps_pre_encoded_tail() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let raw = vec![0x3C_u8, 0x40, 0x7B, b'/'];
        let frames = vec![game_svr::shop::ShopFrame {
            ident: mir2_protocol::messages::SM_SAVEITEMLIST,
            recog: 4_242,
            param: 0,
            tag: 0,
            series: 0,
            tail: raw.clone(),
            tail_encoded: true,
        }];
        emit_shop_frames(&tx, 7, &frames);
        let (sock, body) = match rx.try_recv().expect("帧") {
            SimToGate::SendFrame { socket, body } => (socket, body),
            _ => panic!("非 SendFrame"),
        };
        assert_eq!(sock, 7);
        assert_eq!(
            &body[..12],
            &game_svr::smframe::command_header(
                4_242,
                mir2_protocol::messages::SM_SAVEITEMLIST,
                0,
                0,
                0
            )
        );
        assert_eq!(
            &body[12..],
            &raw[..],
            "直连尾原样下发（不重新 EncodeString）"
        );
    }

    /// **M3⑤-4 收口（T-47 ⑦）：`ClientUserBuyItem` 的 `Dealing` 前置门**
    /// （`PlayObject.Operate.cs:196-199` `if (Dealing) { return; }`，先于 `:200 FindMerchant`、
    /// 先于任何帧/副作用）。`CM_USERBUYITEM`/`CM_USERGETDETAILITEM` 在 oracle 里共用**同一方法体**
    /// （`Message.cs:897-902`）⇒ 两条都要过此门；其余五条商店命令 oracle 无此门。
    ///
    /// ⑦-3/⑦-4 落地后 `Dealing` 不再恒 `false` ⇒ 交易中买货/拉详情是**可达行为差异**。
    /// 判据：`trade_sessions[chr].dealing=true` ⇒ 两条命令零帧 + 金币/背包/库存全不变；
    /// 正对硪 `dealing=false` ⇒ 买入成功（回帧 + 扣金 100 + 加背包 + 库存减）。
    /// **改坏必红**：删掉 `handle_shop_cmd` 里的 `Dealing` 门 ⇒ 情形 A 出现买入帧且金币 500→400。
    #[test]
    fn shop_buy_and_detail_blocked_while_dealing() {
        use std::collections::HashMap;

        // —— 世界：玩家 (10,10) + 买入商人 (11,10)，在售 1 件 `Index=1` 的货。
        let mut world = mir2_world::World::new(32, 32);
        let hero = world.add_entity({
            let mut e = mir2_world::Entity::new(1, "hero", 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e
        });
        let merchant_id = world
            .insert_merchant(mir2_world::merchant::MerchantSpawn {
                name: "商人".to_string(),
                x: 11,
                y: 10,
                map_id: 0,
                state: mir2_world::merchant::MerchantState::new(
                    "卫家店".to_string(),
                    "0".to_string(),
                    0,
                    0,
                    100,
                    false,
                    false,
                    0,
                    mir2_world::merchant::MerchantFlags {
                        is_buy: true,
                        ..Default::default()
                    },
                    vec![0, 5],
                    Vec::new(),
                    0,
                ),
            })
            .expect("商人入世界");
        world.merchant_add_item_to_goods_list(
            merchant_id,
            mir2_world::merchant::MerchantItem {
                make_index: 7,
                index: 1,
                dura: 10,
                dura_max: 10,
                desc: [0; 14],
            },
        );

        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            1,
            mir2_data::StdItem {
                name: "金创药(小量)".to_string(),
                price: 100,
                weight: 3,
                std_mode: 0,
                dura_max: 10,
                ..mir2_data::StdItem::default()
            },
        );

        // —— 玩家会话：金币 500、负重 0/100（够买）。
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        let mut sg = test_session_ground(hero);
        sg.state.gold = 500;
        sg.state.wabil.weight = 0;
        sg.state.wabil.max_weight = 100;
        sessions.insert("hero".to_string(), sg);
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".to_string(),
                chr_name: "hero".to_string(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: hero,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut shop_sessions: HashMap<String, ShopSession> = HashMap::new();
        let mut sell_off_sessions: HashMap<String, game_svr::selloff::SellOffState> =
            HashMap::new();
        let mut sell_off_book = game_svr::selloff::SellOffBook::default();
        let shop_cfg = ShopCfg {
            castle_member_price_rate: 100,
            super_repair_price_rate: 100,
            repair_item_dec_dura: 0,
            get_all_npc_tax: false,
            human_max_gold: 10_000_000,
        };
        let colors = MsgColors {
            red_f: 255,
            red_b: 56,
            hear_f: 0,
            hear_b: 255,
        };

        // —— 情形 A：`Dealing=true` ⇒ 两条命令都零帧、零副作用。
        let mut trade_dealing: HashMap<String, game_svr::trade::DealState> = HashMap::new();
        trade_dealing.insert(
            "hero".to_string(),
            game_svr::trade::DealState {
                dealing: true,
                ..Default::default()
            },
        );
        handle_shop_cmd(
            game_svr::shop::CM_USERBUYITEM,
            merchant_id,
            0,
            0,
            Some("金创药(小量)"),
            "hero",
            901,
            &online,
            &mut sessions,
            &mut shop_sessions,
            &mut sell_off_sessions,
            &mut sell_off_book,
            &trade_dealing,
            &gates,
            &mut world,
            &catalog,
            &shop_cfg,
            &colors,
        );
        assert!(
            matches!(rx0.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "Dealing ⇒ 买入不得发任何帧（oracle Operate.cs:196-199）"
        );
        assert_eq!(
            sessions.get("hero").unwrap().state.gold,
            500,
            "Dealing ⇒ 不得扣金"
        );
        assert!(
            sessions.get("hero").unwrap().state.item_list.is_empty(),
            "Dealing ⇒ 不得加背包"
        );
        assert_eq!(
            world.merchant_goods_groups(merchant_id).unwrap()[0].len(),
            1,
            "Dealing ⇒ 库存不动"
        );
        handle_shop_cmd(
            game_svr::shop::CM_USERGETDETAILITEM,
            merchant_id,
            1,
            0,
            Some("金创药(小量)"),
            "hero",
            901,
            &online,
            &mut sessions,
            &mut shop_sessions,
            &mut sell_off_sessions,
            &mut sell_off_book,
            &trade_dealing,
            &gates,
            &mut world,
            &catalog,
            &shop_cfg,
            &colors,
        );
        assert!(
            matches!(rx0.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "Dealing ⇒ 详情列表不得发任何帧"
        );

        // —— 情形 B（正对照）：`Dealing=false` ⇒ 买入成功。
        let trade_open: HashMap<String, game_svr::trade::DealState> = HashMap::new();
        handle_shop_cmd(
            game_svr::shop::CM_USERBUYITEM,
            merchant_id,
            0,
            0,
            Some("金创药(小量)"),
            "hero",
            901,
            &online,
            &mut sessions,
            &mut shop_sessions,
            &mut sell_off_sessions,
            &mut sell_off_book,
            &trade_open,
            &gates,
            &mut world,
            &catalog,
            &shop_cfg,
            &colors,
        );
        let mut emitted = 0usize;
        while rx0.try_recv().is_ok() {
            emitted += 1;
        }
        assert!(emitted > 0, "非 Dealing ⇒ 应回买入帧");
        assert_eq!(
            sessions.get("hero").unwrap().state.gold,
            400,
            "非 Dealing ⇒ 扣金（100）"
        );
        assert_eq!(
            sessions.get("hero").unwrap().state.item_list.len(),
            1,
            "非 Dealing ⇒ 加背包"
        );
        assert!(
            world.merchant_goods_groups(merchant_id).unwrap().is_empty(),
            "非 Dealing ⇒ 库存减（空组移除）"
        );
    }

    /// **⑩-1 接线判据**：`Merchant.UserSelect` 的 `@dealybme` 支（`Merchant.cs:930-934`）已接线：
    /// `IsBDeal` 商人 + label `@dealybme` ⇒ `UserSelectOpenDealOffForm(playObject)`：
    /// 未开通 ⇒ `SM_MERCHANTSAY(643)`（`Recog`=商人 `ActorId`、`Param=Tag=0`、`Series=1`、
    /// 尾 = `EncodeString(GBK(商人名+'/'+text))`）；开通且无进行中 ⇒ `SM_SENDDEALOFFFORM(23001)`
    /// （`Recog`=商人 `ActorId`、`Param=0`）；`IsBDeal=false` ⇒ 零帧（oracle `if (IsYbDeal)` 门）。
    #[test]
    fn shop_dealybme_opens_consignment_form() {
        use std::collections::HashMap;

        let mut world = mir2_world::World::new(32, 32);
        let hero = world.add_entity({
            let mut e = mir2_world::Entity::new(1, "hero", 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e
        });
        let merchant_id = world
            .insert_merchant(mir2_world::merchant::MerchantSpawn {
                name: "商人".to_string(),
                x: 11,
                y: 10,
                map_id: 0,
                state: mir2_world::merchant::MerchantState::new(
                    "卫家店".to_string(),
                    "0".to_string(),
                    0,
                    0,
                    100,
                    false,
                    false,
                    0,
                    mir2_world::merchant::MerchantFlags {
                        is_yb_deal: true,
                        ..Default::default()
                    },
                    Vec::new(),
                    Vec::new(),
                    0,
                ),
            })
            .expect("商人入世界");

        let catalog = game_svr::ability::StdItemCatalog::new();
        let mut sessions: HashMap<String, SessionGround> = HashMap::new();
        sessions.insert("hero".to_string(), test_session_ground(hero));
        let mut online: OnlineRegistry = Vec::new();
        online_set(
            &mut online,
            OnlineEntry {
                account: "ua".to_string(),
                chr_name: "hero".to_string(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id: hero,
            },
        );
        let (tx0, mut rx0) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx0);
        let mut shop_sessions: HashMap<String, ShopSession> = HashMap::new();
        let mut sell_off_sessions: HashMap<String, game_svr::selloff::SellOffState> =
            HashMap::new();
        let mut sell_off_book = game_svr::selloff::SellOffBook::default();
        let shop_cfg = ShopCfg {
            castle_member_price_rate: 100,
            super_repair_price_rate: 100,
            repair_item_dec_dura: 0,
            get_all_npc_tax: false,
            human_max_gold: 10_000_000,
        };
        let colors = MsgColors {
            red_f: 255,
            red_b: 56,
            hear_f: 0,
            hear_b: 255,
        };
        let trade: HashMap<String, game_svr::trade::DealState> = HashMap::new();

        // ---- ① 未开通 `SaleDeal` ⇒ `SM_MERCHANTSAY` 失败支①。
        handle_shop_cmd(
            game_svr::shop::CM_MERCHANTDLGSELECT,
            merchant_id,
            0,
            0,
            Some(game_svr::selloff::LABEL_DEAL_YB_ME),
            "hero",
            901,
            &online,
            &mut sessions,
            &mut shop_sessions,
            &mut sell_off_sessions,
            &mut sell_off_book,
            &trade,
            &gates,
            &mut world,
            &catalog,
            &shop_cfg,
            &colors,
        );
        let body = match rx0.try_recv() {
            Ok(SimToGate::SendFrame { socket, body }) => {
                assert_eq!(socket, 901);
                body
            }
            _ => panic!("@dealybme 未开通 ⇒ 应回 1 帧商人说话"),
        };
        assert!(
            matches!(rx0.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "@dealybme ⇒ 恰 1 帧"
        );
        assert_eq!(
            u16::from_le_bytes([body[4], body[5]]),
            mir2_protocol::messages::SM_MERCHANTSAY
        );
        assert_eq!(
            i32::from_le_bytes([body[0], body[1], body[2], body[3]]),
            merchant_id,
            "Recog = 商人 ActorId"
        );
        assert_eq!(u16::from_le_bytes([body[6], body[7]]), 0, "Param=0");
        assert_eq!(u16::from_le_bytes([body[8], body[9]]), 0, "Tag=0");
        assert_eq!(u16::from_le_bytes([body[10], body[11]]), 1, "Series=1");
        let expect = mir2_protocol::edcode::encode(&game_svr::downlink::gbk(
            "商人/您未开通元宝服务,请先开通元宝服务!!\\ \\<返回/@main>",
        ));
        assert_eq!(
            &body[12..],
            &expect[..],
            "尾 = EncodeString(商人名+'/'+text)"
        );

        // ---- ② 开通且无进行中 ⇒ `SM_SENDDEALOFFFORM`。
        sell_off_sessions.get_mut("hero").unwrap().sale_deal = true;
        handle_shop_cmd(
            game_svr::shop::CM_MERCHANTDLGSELECT,
            merchant_id,
            0,
            0,
            Some(game_svr::selloff::LABEL_DEAL_YB_ME),
            "hero",
            901,
            &online,
            &mut sessions,
            &mut shop_sessions,
            &mut sell_off_sessions,
            &mut sell_off_book,
            &trade,
            &gates,
            &mut world,
            &catalog,
            &shop_cfg,
            &colors,
        );
        let body = match rx0.try_recv() {
            Ok(SimToGate::SendFrame { socket, body }) => {
                assert_eq!(socket, 901);
                body
            }
            _ => panic!("@dealybme 开通 ⇒ 应回 1 帧开窗"),
        };
        assert_eq!(
            u16::from_le_bytes([body[4], body[5]]),
            mir2_protocol::messages::SM_SENDDEALOFFFORM
        );
        assert_eq!(
            i32::from_le_bytes([body[0], body[1], body[2], body[3]]),
            merchant_id,
            "Recog = 商人 ActorId（32 位）"
        );
        assert_eq!(u16::from_le_bytes([body[6], body[7]]), 0, "Param=0");

        // ---- ③ `IsBDeal=false` ⇒ 零帧（oracle `if (IsBDeal)` 门）。
        world
            .entities
            .get_mut(merchant_id)
            .unwrap()
            .merchant
            .as_mut()
            .unwrap()
            .flags
            .is_yb_deal = false;
        handle_shop_cmd(
            game_svr::shop::CM_MERCHANTDLGSELECT,
            merchant_id,
            0,
            0,
            Some(game_svr::selloff::LABEL_DEAL_YB_ME),
            "hero",
            901,
            &online,
            &mut sessions,
            &mut shop_sessions,
            &mut sell_off_sessions,
            &mut sell_off_book,
            &trade,
            &gates,
            &mut world,
            &catalog,
            &shop_cfg,
            &colors,
        );
        assert!(
            matches!(rx0.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
            "IsBDeal=false ⇒ 不得开窗（oracle Merchant.cs:932）"
        );
    }
}
