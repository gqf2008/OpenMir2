//! 攻城状态机（`UserCastle.Run` + `StartWallconquestWar` / `StopWallconquestWar` + 归属判定）
//! —— M3⑧-实现 7（卡 `openmir2-m3-castle-siege-state`）。
//!
//! 盘点依据：`mir2-rs/tests/parity/m3/M3⑧公会攻城现状盘点报告-20261013.md` §1.D / §1.E / §1.F / §1.L。
//! oracle 锚（行号均在本仓库 `src/` 内实测）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`UserCastle::run`] | `src/M2Server/Castle/UserCastle.cs:311-448`（每 tick；跨天重置 `:328-339`、每日开战 `:340-376`、回收 Ghost `:377-389`、围墙 `StoneMode` `:391-440`） |
//! | [`UserCastle::start_wallconquest_war`] | `UserCastle.cs:564-575`（刷皇宫 100 范围内玩家 `RefShowName`） |
//! | [`UserCastle::stop_wallconquest_war`] | `UserCastle.cs:577-600`（`UnderWar=false` + 清 `AttackGuildList` + 广播） |
//! | [`UserCastle::main_door_control`] | `UserCastle.cs:890-909`（幂等：只在 `IsOpened` 与入参不符时动） |
//! | [`UserCastle::add_attacker_info`] | `UserCastle.cs:998-1014`（`InAttackerList` 查重 + `AddDateTimeOfDay` + `SaveAttackSabukWall` + `SS_212`） |
//! | [`UserCastle::is_master_guild`] / [`UserCastle::is_member`] | `UserCastle.cs:639-642` / `:480-483` |
//! | [`UserCastle::in_palace_guild_count`] | `UserCastle.cs:604-607`（**返回值 = `AttackGuildList.Count`**，语义是「攻城行会数」，不是皇宫内玩家数） |
//! | [`UserCastle::check_in_palace`] | `UserCastle.cs:680-706`（任一城墙格死亡且坐标命中） |
//!
//! ## N1：10 个 `IUserCastle` 重载的可观测 no-op（owner 裁决 comment `a137cc900c`，白名单 **T-38**）
//!
//! oracle 里这些重载体是 `throw new NotImplementedException`，被调用方外层 `catch` 吞掉
//! ⇒ **该链在 oracle 里「静默无效果」**；且因为调用点变量静态类型是接口 `IUserCastle`，
//! C# 重载决议只看接口成员 ⇒ 真实实现所在的 `GuildInfo`/`PlayObject` 重载**永远选不中**
//! （盘点 §1.L）。本模块照此建模：入口走 **no-op 分支**（世界/角色可观测状态不变），
//! **不** `panic!`/返 `Err`，**也不**实现「本该有」的提取/上缴/归属逻辑。
//!
//! | no-op 入口 | oracle 抛异常行 | 被吞调用点（盘点 §1.L） |
//! | --- | --- | --- |
//! | [`UserCastle::with_drawal_golds_n1`] | `UserCastle.cs:601` | `CastleOfficial.cs:157`（金库提取，⑧-9 面） |
//! | [`UserCastle::get_castle_n1`] | `UserCastle.cs:646` | `PlayObject.Message.cs:330`、`ChangeSabukLordCommand.cs:39` |
//! | [`UserCastle::add_attacker_info_n1`] | `UserCastle.cs:672` | `GuildOfficial.cs:219`（NPC 申请攻城） |
//! | [`UserCastle::can_get_castle_n1`] | `UserCastle.cs:677` | `PlayObject.Message.cs:328` |
//! | [`UserCastle::check_in_palace_player_n1`] | `UserCastle.cs:709` | `PlayObject.Operate.cs:371`（开城门前的皇宫判定） |
//! | [`UserCastle::receipt_golds_n1`] | `UserCastle.cs:913` | `CastleOfficial.cs:179`（金库上缴，⑧-9 面） |
//! | [`UserCastle::is_attack_ally_guild_n1`] | `UserCastle.cs:1042` | — |
//! | [`UserCastle::is_attack_guild_n1`] | `UserCastle.cs:1047` | `PlayObject.Message.cs:326` |
//! | [`UserCastle::is_defense_ally_guild_n1`] | `UserCastle.cs:1052` | — |
//! | [`UserCastle::is_defense_guild_n1`] | `UserCastle.cs:1057` | — |
//!
//! [`UserCastle::try_auto_occupy_n1`] 把 `PlayObject.Message.cs:324-334` 的**自动占领整链**建模为 no-op
//! （该链在第一个 `castle.IsAttackGuild(MyGuild)` 就抛异常被吞 ⇒ 后续 `GetCastle`/`SS_211`/
//! `StopWallconquestWar` 都不可达）。
//!
//! ## 照搬的 oracle 怪行为（不是差异）
//!
//! 1. **`AddDateTimeOfDay` 忽略天数**：`M2Share.cs:458-493` 的函数体整段被注释，最后只有
//!    `DateTime result = DateTime.Now;` ⇒ `AddAttackerInfo(GuildInfo).AttackDate = 本刻`（**不是** `+StartCastleWarDays`）。
//!    本模块 [`UserCastle::add_attacker_info`] 照搬：`attack_date = now.date`。
//! 2. **`StopWallconquestWar` 的玩家处理不可达**：`UserCastle.cs:582-584` 的
//!    `GetMapOfRangeHumanCount(CastleEnvir, HomeX, HomeY, 100);` **丢弃返回值**，而 `ListC` 始终为空
//!    ⇒ `for (i < ListC.Count)` 循环体（`ChangePkStatus(false)` / 非守方 `MapRandomMove`）**从不执行**。
//!    本模块照搬：停止时只在状态机里清 `AttackGuildList` 并发广播，**不**产生玩家位移/换 PK 状态事件。
//! 3. **`InPalaceGuildCount` 命名与语义不符**：返回的是 `AttackGuildList.Count`（攻城行会数），
//!    不是「皇宫内玩家数」（盘点 §1.E，登记）。
//!
//! ## 确定性
//!
//! 本模块**零 RNG、零墙钟**：时间全部来自调用方注入的 [`CastleNow`]（`GetTickCount` 口径 = `tick_ms`，
//! 日历/Hour 口径 = `date`/`hour`）。对应 ⑧-1「可注入时钟」在 **Rust 构造级**的用法；
//! 真机两侧时间门对拍仍需 ⑧-1 的 C# 时钟注入片 + 独占窗口。

use mir2_data::guild_castle::GuildCastleConfig;

/// `UserCastle.Archers = new ArcherUnit[12]`（`UserCastle.cs:48`）与 `CastleConst.MaxCastleArcher`
/// （`CastleConst.cs:11`）。`Guards` 数组长度同为 12（`UserCastle.cs:49`），
/// 与 `MaxCalsteGuard = 4`（雇佣上限）不同，别混用。
pub const MAX_CASTLE_ARCHER: usize = 12;
/// `Guards` 数组长度（`UserCastle.cs:49`）。
pub const MAX_CASTLE_GUARD: usize = 12;
/// `CastleWarRangeX` / `CastleWarRangeY` 的 `InCastleWarArea` 默认（`GameSvrConf.cs:1786-1787`）。
pub const DEFAULT_WAR_RANGE: i32 = 100;
/// `Messages.cs:453` 的 `SS_204`（跨服系统消息）。
pub const SS_204: u16 = 204;
/// `Messages.cs:461` 的 `SS_212`（跨服 `RELOADCASTLEINFO`）。
pub const SS_212: u16 = 212;

/// 真身 `PlayObject.ChrName` 之类的**行会身份**用不透明 id 表示（本模块不依赖行会系统）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GuildId(pub u64);

/// 只到「日」粒度的日历（`DateTime.Now.Year/Month/Day` 口径，`UserCastle.cs:328-330`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct CastleDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl CastleDate {
    /// 构造。
    #[must_use]
    pub const fn new(year: i32, month: u32, day: u32) -> Self {
        CastleDate { year, month, day }
    }
}

/// 注入时钟：`HUtil32.GetTickCount()`（毫秒）与 `DateTime.Now`（日历 + 小时）的合一快照。
///
/// 真机 `GetTickCount` = 进程启动后的毫秒；构造级用例直接给 `tick_ms`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastleNow {
    pub date: CastleDate,
    pub hour: i32,
    pub tick_ms: i64,
}

impl CastleNow {
    /// 构造。
    #[must_use]
    pub const fn new(date: CastleDate, hour: i32, tick_ms: i64) -> Self {
        CastleNow {
            date,
            hour,
            tick_ms,
        }
    }
}

/// 攻城状态机用到的配置（来自 [`GuildCastleConfig`]，见 `crates/data/src/guild_castle.rs` §1.K）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastleSiegeConfig {
    /// `[Setup] StartCastlewarTime`（`ServerConf.cs:639`，每日开战小时）。
    pub start_castlewar_time: i32,
    /// `[Setup] CastleWarTime`（`ServerConf.cs:642`，攻城时长 ms）。
    pub castle_war_time: i32,
    /// `[Setup] ShowCastleWarEndMsgTime`（`ServerConf.cs:640`，结束前预告窗口 ms）。
    pub show_castle_war_end_msg_time: i32,
    /// `[Setup] GetCastleTime`（`ServerConf.cs:643`，可占领最短时长 ms）。
    pub get_castle_time: i32,
    /// `[Setup] StartCastleWarDays`（`ServerConf.cs:638`，申请日 +N 天；`AddDateTimeOfDay` 实际忽略）。
    pub start_castle_war_days: u8,
    /// `[Setup] CastleName`（`ServerConf.cs:253`，广播文案里的城堡名）。
    pub castle_name: String,
}

impl Default for CastleSiegeConfig {
    /// `GameSvrConf` 初值（`GameSvrConf.cs:1442-1446` 与 `:1782`）。
    fn default() -> Self {
        CastleSiegeConfig {
            start_castlewar_time: 20,
            castle_war_time: 3 * 60 * 60 * 1000,
            show_castle_war_end_msg_time: 10 * 60 * 1000,
            get_castle_time: 10 * 60 * 1000,
            start_castle_war_days: 4,
            castle_name: "沙巴克".to_string(),
        }
    }
}

impl CastleSiegeConfig {
    /// 从 `Server.conf` 的 [`GuildCastleConfig`] 取本卡用到的 6 项（其余键属 ⑧-9）。
    #[must_use]
    pub fn from_guild_castle_config(cfg: &GuildCastleConfig) -> Self {
        CastleSiegeConfig {
            start_castlewar_time: cfg.start_castlewar_time,
            castle_war_time: cfg.castle_war_time,
            show_castle_war_end_msg_time: cfg.show_castle_war_end_msg_time,
            get_castle_time: cfg.get_castle_time,
            start_castle_war_days: cfg.start_castle_war_days,
            castle_name: cfg.castle_name.clone(),
        }
    }
}

/// 攻城申请人（oracle `AttackerInfo`：`sGuildName` + `Guild` + `AttackDate`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttackerInfo {
    pub guild_name: String,
    pub guild: GuildId,
    pub attack_date: CastleDate,
}

/// 门 / 墙 / 弓手 / 守卫的最小可观测状态（oracle `ArcherUnit.BaseObject` 的子集）。
///
/// 只保留 [`UserCastle::run`] 与 [`UserCastle::main_door_control`] 用到的字段；
/// 城门/城墙的 `Open/Close/Die/RefStatus` 语义与 `SetMapXyFlag` 通行掩码属 ⑧-8。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CastleUnit {
    pub ghost: bool,
    pub death: bool,
    /// 城墙 `StoneMode`：非战时 `true`（不可破坏），战时 `false`。
    pub stone_mode: bool,
    /// `CastleDoor.IsOpened`（`CastleDoor.cs`）。
    pub opened: bool,
    pub x: i16,
    pub y: i16,
    /// `BaseObject.StruckTick`（`GetTickCount()` 口径；`RepairDoor`/`RepairWall` 的 60s 门，属 ⑧-8）。
    pub struck_tick: i64,
}

/// 状态机对宿主发出的**可观测副作用**（顺序 = oracle 调用序）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastleEvent {
    /// `SendBroadCastMsgExt(text, MsgType.System)`（全服系统广播）。
    Broadcast(String),
    /// `SendServerGroupMsg(code, ServerIndex, text)`（跨服系统消息）。
    ServerGroupMsg { code: u16, text: String },
    /// `SaveAttackSabukWall()`（落 `AttackSabukWall.txt`，由宿主执行）。
    SaveAttackSabukWall,
    /// `StartWallconquestWar`：对皇宫 (x,y) 半径 range 内的玩家 `RefShowName`。
    RefreshShowName { x: i16, y: i16, range: i32 },
}

/// `UserCastle` 的攻城状态机（oracle `src/M2Server/Castle/UserCastle.cs`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserCastle {
    /// `sName`（广播文案里的城堡名）。
    pub name: String,
    /// `MapName`（`CastleConf` 的 `[Setup] MapName`；空 ⇒ `Run` 直接返回）。
    pub map_name: Option<String>,
    /// `M2Share.ServerIndex`。
    pub server_index: i32,
    /// `SystemShare.MapMgr.GetMapOfServerIndex(MapName)`（归属服）。
    pub map_owner_server_index: i32,
    /// `OwnGuild` 经 `GuildMgr.FindGuild` 反查得到的 `MasterGuild`（可能为 `None`）。
    pub master_guild: Option<GuildId>,
    /// `OwnGuild`（落盘字符串口径，`GetCastle` 会写；本卡只读）。
    pub own_guild: Option<String>,
    /// `UnderWar`。
    pub under_war: bool,
    /// `IsStartWar`（当日已检查过一次开战钟点）。
    pub is_start_war: bool,
    /// `ShowOverMsg`（本场已播过「离结束还有 N 分钟」）。
    pub show_over_msg: bool,
    /// `WarDate`。
    pub war_date: Option<CastleDate>,
    /// `StartCastleWarTick`（`GetTickCount()` 口径）。
    pub start_castle_war_tick: i64,
    /// `TodayIncome`。
    pub today_income: i32,
    /// `TotalGold`（⑧-9 税收面；本卡只随状态机保留）。
    pub total_gold: i32,
    /// `IncomeToday`（跨天判定用）。
    pub income_today: CastleDate,
    /// `AttackWarList`（申请登记表）。
    pub attack_war_list: Vec<AttackerInfo>,
    /// `AttackGuildList`（本场攻方；含 `MasterGuild`）。
    pub attack_guild_list: Vec<GuildId>,
    /// `MainDoor`。
    pub main_door: Option<CastleUnit>,
    /// `LeftWall` / `CenterWall` / `RightWall`。
    pub left_wall: Option<CastleUnit>,
    pub center_wall: Option<CastleUnit>,
    pub right_wall: Option<CastleUnit>,
    /// `Archers[12]`。
    pub archers: [Option<CastleUnit>; MAX_CASTLE_ARCHER],
    /// `Guards[12]`。
    pub guards: [Option<CastleUnit>; MAX_CASTLE_GUARD],
    /// `HomeX` / `HomeY`（`StartWallconquestWar` 的范围中心）。
    pub home_x: i16,
    pub home_y: i16,
    /// 配置。
    pub config: CastleSiegeConfig,
}

impl UserCastle {
    /// 按 oracle `UserCastle` 构造函数（`UserCastle.cs:23-59`）+ `CastleConf` 默认建一座空城。
    #[must_use]
    pub fn new(name: impl Into<String>, config: CastleSiegeConfig) -> Self {
        UserCastle {
            name: name.into(),
            map_name: None,
            server_index: 0,
            map_owner_server_index: 0,
            master_guild: None,
            own_guild: None,
            under_war: false,
            is_start_war: false,
            show_over_msg: false,
            war_date: None,
            start_castle_war_tick: 0,
            today_income: 0,
            total_gold: 0,
            income_today: CastleDate::default(),
            attack_war_list: Vec::new(),
            attack_guild_list: Vec::new(),
            main_door: None,
            left_wall: None,
            center_wall: None,
            right_wall: None,
            archers: [None; MAX_CASTLE_ARCHER],
            guards: [None; MAX_CASTLE_GUARD],
            home_x: 644,
            home_y: 290,
            config,
        }
    }

    /// oracle `UserCastle.Run()`（`UserCastle.cs:311-448`）。
    ///
    /// 顺序逐行照搬：`MapName`/归属服门 → 跨天重置 → 每日钟点开战 → 回收 Ghost 守卫/弓手 →
    /// 战时围墙 `StoneMode=false` + 结束预告 + 超时停战 / 非战 `StoneMode=true`。
    pub fn run(&mut self, now: CastleNow) -> Vec<CastleEvent> {
        let mut events = Vec::new();

        // `:318-321`：`string.IsNullOrEmpty(MapName)` ⇒ return。
        match &self.map_name {
            None => return events,
            Some(name) if name.is_empty() => return events,
            Some(_) => {}
        }
        // `:323-326`：非本服归属 ⇒ return。
        if self.server_index != self.map_owner_server_index {
            return events;
        }

        // `:328-339`：跨天重置 `TodayIncome=0; IncomeToday=Now; IsStartWar=false`。
        let today = now.date;
        if today != self.income_today {
            self.today_income = 0;
            self.income_today = today;
            self.is_start_war = false;
        }

        // `:340-376`：每日开战。
        if !self.is_start_war && !self.under_war && now.hour == self.config.start_castlewar_time {
            self.is_start_war = true;
            self.attack_guild_list.clear();
            // `:347-362`：**倒序**遍历申请表；命中今天者移入攻方列表（`RemoveAt(i)`）。
            let mut i = self.attack_war_list.len() as isize - 1;
            while i >= 0 {
                let idx = i as usize;
                let attacker = &self.attack_war_list[idx];
                if attacker.attack_date == today {
                    let guild = attacker.guild;
                    self.under_war = true;
                    self.show_over_msg = false;
                    self.war_date = Some(today);
                    // `:358` `StartCastleWarTick = HUtil32.GetTickCount()`。
                    self.start_castle_war_tick = now.tick_ms;
                    self.attack_guild_list.push(guild);
                    self.attack_war_list.remove(idx);
                }
                i -= 1;
            }
            if self.under_war {
                // `:365`：守方（可能为 `None`，oracle `MasterGuild` 为 null 时 `Add(null)` 是 no-op 效果）。
                if let Some(master) = self.master_guild {
                    self.attack_guild_list.push(master);
                }
                // `:366` StartWallconquestWar()。
                events.extend(self.start_wallconquest_war());
                // `:367` SaveAttackSabukWall()。
                events.push(CastleEvent::SaveAttackSabukWall);
                // `:368` SS_212。
                events.push(CastleEvent::ServerGroupMsg {
                    code: SS_212,
                    text: String::new(),
                });
                // `:369-371` 开战广播。
                let msg = format!("[{} 攻城战已经开始]", self.name);
                events.push(CastleEvent::Broadcast(msg.clone()));
                events.push(CastleEvent::ServerGroupMsg {
                    code: SS_204,
                    text: msg,
                });
                // `:373` MainDoorControl(true)（幂等关闭主门）。
                self.main_door_control(true);
            }
        }

        // `:377-389`：回收 Ghost 的守卫 → 弓手（数组下标升序，`Guards` 在前）。
        for slot in self.guards.iter_mut() {
            if slot.is_some_and(|u| u.ghost) {
                *slot = None;
            }
        }
        for slot in self.archers.iter_mut() {
            if slot.is_some_and(|u| u.ghost) {
                *slot = None;
            }
        }

        if self.under_war {
            // `:393-406`：战时三面墙 `StoneMode=false`。
            set_stone_mode(&mut self.left_wall, false);
            set_stone_mode(&mut self.center_wall, false);
            set_stone_mode(&mut self.right_wall, false);
            // `:408-418`：结束预告（只播一次）。
            if !self.show_over_msg
                && (now.tick_ms - self.start_castle_war_tick)
                    > i64::from(
                        self.config.castle_war_time - self.config.show_castle_war_end_msg_time,
                    )
            {
                self.show_over_msg = true;
                let minutes = self.config.show_castle_war_end_msg_time / (60 * 1000);
                let msg = format!("[{} 攻城战离结束还有{}分钟]", self.name, minutes);
                events.push(CastleEvent::Broadcast(msg.clone()));
                events.push(CastleEvent::ServerGroupMsg {
                    code: SS_204,
                    text: msg,
                });
            }
            // `:419-422`：超时停战（**严格大于**）。
            if (now.tick_ms - self.start_castle_war_tick) > i64::from(self.config.castle_war_time) {
                events.extend(self.stop_wallconquest_war());
            }
        } else {
            // `:424-440`：非战时三面墙 `StoneMode=true`。
            set_stone_mode(&mut self.left_wall, true);
            set_stone_mode(&mut self.center_wall, true);
            set_stone_mode(&mut self.right_wall, true);
        }

        events
    }

    /// oracle `StartWallconquestWar()`（`UserCastle.cs:564-575`）：刷皇宫 100 范围内玩家 `RefShowName`。
    ///
    /// 玩家选择（`GetMapRageHuman(PalaceEnvir, HomeX, HomeY, 100)`）属宿主能力 ⇒ 发
    /// [`CastleEvent::RefreshShowName`] 由宿主解析目标。
    pub fn start_wallconquest_war(&self) -> Vec<CastleEvent> {
        vec![CastleEvent::RefreshShowName {
            x: self.home_x,
            y: self.home_y,
            range: DEFAULT_WAR_RANGE,
        }]
    }

    /// oracle `StopWallconquestWar()`（`UserCastle.cs:577-600`）。
    ///
    /// **照搬 oracle 怪行为**：`:582-584` 的 `GetMapOfRangeHumanCount` **丢弃返回值**，
    /// `ListC` 恒空 ⇒ `:584-593` 的 `ChangePkStatus(false)` / 非守方 `MapRandomMove` **从不执行**。
    /// 因此本函数只：`UnderWar=false` + 清 `AttackGuildList` + 广播。
    pub fn stop_wallconquest_war(&mut self) -> Vec<CastleEvent> {
        self.under_war = false;
        self.attack_guild_list.clear();
        let msg = format!("[{} 攻城战已经结束]", self.name);
        vec![
            CastleEvent::Broadcast(msg.clone()),
            CastleEvent::ServerGroupMsg {
                code: SS_204,
                text: msg,
            },
        ]
    }

    /// oracle `MainDoorControl(bool boClose)`（`UserCastle.cs:890-909`）：幂等开关主门。
    ///
    /// `BaseObject == null || Ghost` ⇒ 不动；`boClose=true` 时只对「已开」的门关，
    /// `boClose=false` 时只对「未开」的门开。
    pub fn main_door_control(&mut self, bo_close: bool) {
        let Some(door) = self.main_door.as_mut() else {
            return;
        };
        if door.ghost {
            return;
        }
        if bo_close {
            if door.opened {
                door.opened = false;
            }
        } else if !door.opened {
            door.opened = true;
        }
    }

    /// oracle `AddAttackerInfo(GuildInfo)`（`UserCastle.cs:998-1014`）。
    ///
    /// 返回 `true` = 登记成功。**照搬** `AddDateTimeOfDay` 忽略天数的怪行为（`attack_date = now.date`）。
    pub fn add_attacker_info(
        &mut self,
        guild: GuildId,
        guild_name: &str,
        now: CastleNow,
    ) -> (bool, Vec<CastleEvent>) {
        if self.in_attacker_list(guild) {
            return (false, Vec::new());
        }
        self.attack_war_list.push(AttackerInfo {
            guild_name: guild_name.to_string(),
            guild,
            attack_date: now.date,
        });
        let events = vec![
            CastleEvent::SaveAttackSabukWall,
            CastleEvent::ServerGroupMsg {
                code: SS_212,
                text: String::new(),
            },
        ];
        (true, events)
    }

    /// oracle `InAttackerList(GuildInfo)`（`UserCastle.cs:1016-1030`）。
    #[must_use]
    pub fn in_attacker_list(&self, guild: GuildId) -> bool {
        self.attack_war_list.iter().any(|a| a.guild == guild)
    }

    /// oracle `IsMasterGuild(IGuild)`（`UserCastle.cs:639-642`）。
    #[must_use]
    pub fn is_master_guild(&self, guild: GuildId) -> bool {
        self.master_guild == Some(guild)
    }

    /// oracle `IsMember(PlayObject)`（`UserCastle.cs:480-483`）/ `IsMember(IPlayerActor)`（`:609-612`）
    /// ⇒ `IsMasterGuild(member.MyGuild)`。
    #[must_use]
    pub fn is_member(&self, guild: GuildId) -> bool {
        self.is_master_guild(guild)
    }

    /// oracle `InPalaceGuildCount()`（`UserCastle.cs:604-607`）= `AttackGuildList.Count`。
    ///
    /// 登记：语义是「攻城行会数」，**不是**皇宫内玩家数（盘点 §1.E）。
    #[must_use]
    pub fn in_palace_guild_count(&self) -> usize {
        self.attack_guild_list.len()
    }

    /// oracle `CheckInPalace(int nX, int nY)`（`UserCastle.cs:680-706`）：
    /// 任一面墙（左/中/右）的 `BaseObject` 非空、`Death` 且坐标命中 ⇒ `true`。
    #[must_use]
    pub fn check_in_palace(&self, x: i16, y: i16) -> bool {
        [&self.left_wall, &self.center_wall, &self.right_wall]
            .into_iter()
            .flatten()
            .any(|w| w.death && w.x == x && w.y == y)
    }

    // ------------------------------------------------------------------
    // N1：10 个 `IUserCastle` 重载的**可观测 no-op**（白名单 T-38）
    // ------------------------------------------------------------------

    /// N1：`UserCastle.cs:601`（`WithDrawalGolds(IPlayerActor,int)`）抛异常被吞 ⇒ no-op。
    ///
    /// 返回值仅作占位（oracle 抛异常、无返回值）；**不**改 `total_gold`、**不**发事件。
    pub fn with_drawal_golds_n1(&mut self, _guild: GuildId, _gold: i32) -> i32 {
        -1
    }

    /// N1：`UserCastle.cs:646`（`GetCastle(IGuild)`）抛异常被吞 ⇒ no-op（**不**改归属）。
    pub fn get_castle_n1(&mut self, _guild: GuildId) {}

    /// N1：`UserCastle.cs:672`（`AddAttackerInfo(IGuild)`）抛异常被吞 ⇒ no-op（**不**登记）。
    pub fn add_attacker_info_n1(&mut self, _guild: GuildId) -> bool {
        false
    }

    /// N1：`UserCastle.cs:677`（`CanGetCastle(IGuild)`）抛异常被吞 ⇒ no-op ⇒ `false`。
    #[must_use]
    pub fn can_get_castle_n1(&self, _guild: GuildId) -> bool {
        false
    }

    /// N1：`UserCastle.cs:709`（`CheckInPalace(int,int,IPlayerActor)`）抛异常被吞 ⇒ no-op ⇒ `false`。
    #[must_use]
    pub fn check_in_palace_player_n1(&self, _x: i16, _y: i16, _guild: GuildId) -> bool {
        false
    }

    /// N1：`UserCastle.cs:913`（`ReceiptGolds(IPlayerActor,int)`）抛异常被吞 ⇒ no-op。
    pub fn receipt_golds_n1(&mut self, _guild: GuildId, _gold: i32) -> i32 {
        -1
    }

    /// N1：`UserCastle.cs:1042`（`IsAttackAllyGuild(IGuild)`）抛异常被吞 ⇒ no-op ⇒ `false`。
    #[must_use]
    pub fn is_attack_ally_guild_n1(&self, _guild: GuildId) -> bool {
        false
    }

    /// N1：`UserCastle.cs:1047`（`IsAttackGuild(IGuild)`）抛异常被吞 ⇒ no-op ⇒ `false`。
    #[must_use]
    pub fn is_attack_guild_n1(&self, _guild: GuildId) -> bool {
        false
    }

    /// N1：`UserCastle.cs:1052`（`IsDefenseAllyGuild(IGuild)`）抛异常被吞 ⇒ no-op ⇒ `false`。
    #[must_use]
    pub fn is_defense_ally_guild_n1(&self, _guild: GuildId) -> bool {
        false
    }

    /// N1：`UserCastle.cs:1057`（`IsDefenseGuild(IGuild)`）抛异常被吞 ⇒ no-op ⇒ `false`。
    #[must_use]
    pub fn is_defense_guild_n1(&self, _guild: GuildId) -> bool {
        false
    }

    /// N1：`PlayObject.Message.cs:324-334` 的**自动占领整链**（`IsAttackGuild` → `CanGetCastle`
    /// → `GetCastle` → `SS_211` → `InPalaceGuildCount()<=1` ⇒ `StopWallconquestWar`）。
    ///
    /// 该链在第一个接口调用 `castle.IsAttackGuild(MyGuild)` 就抛异常被吞 ⇒ **整链不可达**
    /// ⇒ no-op（返回 `false`，不改状态、不发事件、不停战）。
    pub fn try_auto_occupy_n1(&mut self, _guild: GuildId) -> bool {
        false
    }
}

fn set_stone_mode(slot: &mut Option<CastleUnit>, value: bool) {
    if let Some(unit) = slot.as_mut() {
        unit.stone_mode = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G_ATTACK: GuildId = GuildId(1);
    const G_DEFEND: GuildId = GuildId(2);

    fn config() -> CastleSiegeConfig {
        CastleSiegeConfig {
            start_castlewar_time: 20,
            castle_war_time: 7_200_000,
            show_castle_war_end_msg_time: 600_000,
            get_castle_time: 600_000,
            start_castle_war_days: 2,
            castle_name: "沙巴克".to_string(),
        }
    }

    /// 一座「本服持有 + 今日已有申请人 + 四面墙/门/弓手就位」的城。
    fn ready_castle() -> UserCastle {
        let mut c = UserCastle::new("沙巴克", config());
        c.map_name = Some("3".to_string());
        c.server_index = 0;
        c.map_owner_server_index = 0;
        c.master_guild = Some(G_DEFEND);
        c.own_guild = Some("守方".to_string());
        c.income_today = CastleDate::new(2026, 10, 13);
        c.main_door = Some(CastleUnit {
            opened: true,
            x: 672,
            y: 330,
            ..CastleUnit::default()
        });
        c.left_wall = Some(CastleUnit {
            x: 624,
            y: 278,
            ..CastleUnit::default()
        });
        c.center_wall = Some(CastleUnit {
            x: 627,
            y: 278,
            ..CastleUnit::default()
        });
        c.right_wall = Some(CastleUnit {
            x: 634,
            y: 271,
            ..CastleUnit::default()
        });
        c.archers[0] = Some(CastleUnit::default());
        c.guards[0] = Some(CastleUnit::default());
        c
    }

    fn now(hour: i32, tick_ms: i64) -> CastleNow {
        CastleNow::new(CastleDate::new(2026, 10, 13), hour, tick_ms)
    }

    #[test]
    fn daily_war_starts_at_configured_hour_with_today_attackers() {
        let mut c = ready_castle();
        // AttackDate = 今天（现网 StartCastleWarDays=2，但 AddDateTimeOfDay 忽略天数）。
        let (ok, ev) = c.add_attacker_info(G_ATTACK, "攻方", now(10, 0));
        assert!(ok);
        assert_eq!(
            ev,
            vec![
                CastleEvent::SaveAttackSabukWall,
                CastleEvent::ServerGroupMsg {
                    code: SS_212,
                    text: String::new()
                }
            ]
        );
        assert_eq!(c.attack_war_list.len(), 1);

        // 未到 20 点：不开战。
        let ev = c.run(now(19, 1_000));
        assert!(ev.is_empty());
        assert!(!c.under_war);
        assert!(!c.is_start_war);

        // 20 点：开战。
        let ev = c.run(now(20, 2_000));
        assert!(c.under_war);
        assert!(c.is_start_war);
        assert_eq!(c.attack_guild_list, vec![G_ATTACK, G_DEFEND]);
        assert!(c.attack_war_list.is_empty());
        assert_eq!(c.war_date, Some(CastleDate::new(2026, 10, 13)));
        assert_eq!(c.start_castle_war_tick, 2_000);
        assert!(!c.show_over_msg);
        // 事件顺序 = oracle :366-371。
        assert_eq!(
            ev,
            vec![
                CastleEvent::RefreshShowName {
                    x: 644,
                    y: 290,
                    range: 100
                },
                CastleEvent::SaveAttackSabukWall,
                CastleEvent::ServerGroupMsg {
                    code: SS_212,
                    text: String::new()
                },
                CastleEvent::Broadcast("[沙巴克 攻城战已经开始]".to_string()),
                CastleEvent::ServerGroupMsg {
                    code: SS_204,
                    text: "[沙巴克 攻城战已经开始]".to_string()
                },
            ]
        );
        // MainDoorControl(true) 关主门 + 战时墙 StoneMode=false。
        assert!(!c.main_door.unwrap().opened);
        assert!(!c.left_wall.unwrap().stone_mode);
        assert!(!c.center_wall.unwrap().stone_mode);
        assert!(!c.right_wall.unwrap().stone_mode);
        assert!(c.archers[0].is_some());
        assert!(c.guards[0].is_some());
    }

    #[test]
    fn war_not_started_when_attacker_date_is_another_day() {
        let mut c = ready_castle();
        c.attack_war_list.push(AttackerInfo {
            guild_name: "攻方".to_string(),
            guild: G_ATTACK,
            attack_date: CastleDate::new(2026, 10, 12),
        });
        let ev = c.run(now(20, 5_000));
        assert!(ev.is_empty());
        assert!(!c.under_war);
        // IsStartWar 仍置 true（oracle :345，进了钟点分支就置位）。
        assert!(c.is_start_war);
        assert_eq!(c.attack_war_list.len(), 1);
        assert!(c.attack_guild_list.is_empty());
        // 未开战 ⇒ 墙保持 StoneMode=true。
        assert!(c.left_wall.unwrap().stone_mode);
    }

    #[test]
    fn cross_day_resets_income_and_start_flag() {
        let mut c = ready_castle();
        c.today_income = 123;
        c.is_start_war = true;
        let ev = c.run(CastleNow::new(CastleDate::new(2026, 10, 14), 1, 9_000));
        assert!(ev.is_empty());
        assert_eq!(c.today_income, 0);
        assert_eq!(c.income_today, CastleDate::new(2026, 10, 14));
        assert!(!c.is_start_war);
    }

    #[test]
    fn empty_or_foreign_map_returns_early() {
        let mut c = ready_castle();
        c.map_name = None;
        assert!(c.run(now(20, 1_000)).is_empty());
        assert!(!c.is_start_war);

        let mut c = ready_castle();
        c.map_name = Some(String::new());
        assert!(c.run(now(20, 1_000)).is_empty());

        let mut c = ready_castle();
        c.map_owner_server_index = 7;
        assert!(c.run(now(20, 1_000)).is_empty());
        assert!(!c.is_start_war);
    }

    #[test]
    fn war_end_warning_then_stop() {
        let mut c = ready_castle();
        c.add_attacker_info(G_ATTACK, "攻方", now(10, 0));
        c.run(now(20, 1_000));
        assert!(c.under_war);

        // 阈值 = CastleWarTime - ShowCastleWarEndMsgTime = 7_200_000 - 600_000 = 6_600_000（严格大于）。
        let before = 1_000 + 6_600_000;
        let ev = c.run(now(21, before));
        assert!(ev.is_empty());
        assert!(!c.show_over_msg);

        let ev = c.run(now(21, before + 1));
        assert!(c.show_over_msg);
        assert_eq!(
            ev,
            vec![
                CastleEvent::Broadcast("[沙巴克 攻城战离结束还有10分钟]".to_string()),
                CastleEvent::ServerGroupMsg {
                    code: SS_204,
                    text: "[沙巴克 攻城战离结束还有10分钟]".to_string()
                },
            ]
        );

        // 再 tick 不重复播预告。
        let ev = c.run(now(21, before + 2));
        assert!(ev.is_empty());

        // 超过 CastleWarTime = 7_200_000 ⇒ 停战。
        let stop_tick = 1_000 + 7_200_000;
        let ev = c.run(now(22, stop_tick));
        assert!(ev.is_empty());
        let ev = c.run(now(22, stop_tick + 1));
        assert!(!c.under_war);
        assert!(c.attack_guild_list.is_empty());
        assert_eq!(
            ev,
            vec![
                CastleEvent::Broadcast("[沙巴克 攻城战已经结束]".to_string()),
                CastleEvent::ServerGroupMsg {
                    code: SS_204,
                    text: "[沙巴克 攻城战已经结束]".to_string()
                },
            ]
        );
        // 照搬 oracle 结构：停战发生在 `if (UnderWar)` 块内 ⇒ **本 tick** 墙仍 `StoneMode=false`
        // （`:391-406` 先置 false，`:421` 才停战，本 tick 不再走 `else`）。
        assert!(!c.left_wall.unwrap().stone_mode);
        // 下一 tick 起 `UnderWar==false` ⇒ `else` 分支恢复 `StoneMode=true`。
        let ev = c.run(now(22, stop_tick + 2));
        assert!(ev.is_empty());
        assert!(c.left_wall.unwrap().stone_mode);
    }

    /// 照搬 oracle 怪行为：`StopWallconquestWar` 里 `GetMapOfRangeHumanCount` 丢弃返回值、`ListC` 恒空
    /// ⇒ **不**产生任何玩家位移/换 PK 状态事件。
    #[test]
    fn stop_war_does_not_touch_players_like_oracle_bug() {
        let mut c = ready_castle();
        c.under_war = true;
        c.attack_guild_list = vec![G_ATTACK, G_DEFEND];
        let ev = c.stop_wallconquest_war();
        assert_eq!(ev.len(), 2);
        assert!(matches!(ev[0], CastleEvent::Broadcast(_)));
        assert!(matches!(
            ev[1],
            CastleEvent::ServerGroupMsg { code: SS_204, .. }
        ));
        // 没有 ChangePkStatus / MapRandomMove 之类的玩家事件变体。
        assert!(!ev.iter().any(|e| !matches!(
            e,
            CastleEvent::Broadcast(_) | CastleEvent::ServerGroupMsg { .. }
        )));
    }

    #[test]
    fn ghost_units_are_reclaimed_guards_before_archers() {
        let mut c = ready_castle();
        c.guards[0] = Some(CastleUnit {
            ghost: true,
            ..CastleUnit::default()
        });
        c.guards[1] = Some(CastleUnit::default());
        c.archers[0] = Some(CastleUnit {
            ghost: true,
            ..CastleUnit::default()
        });
        c.archers[1] = Some(CastleUnit::default());
        c.run(now(1, 1_000));
        assert!(c.guards[0].is_none());
        assert!(c.guards[1].is_some());
        assert!(c.archers[0].is_none());
        assert!(c.archers[1].is_some());
    }

    #[test]
    fn main_door_control_is_idempotent_and_skips_ghost() {
        let mut c = ready_castle();
        c.main_door = Some(CastleUnit {
            opened: true,
            ..CastleUnit::default()
        });
        c.main_door_control(true);
        assert!(!c.main_door.unwrap().opened);
        c.main_door_control(true); // 再关不动
        assert!(!c.main_door.unwrap().opened);
        c.main_door_control(false);
        assert!(c.main_door.unwrap().opened);
        c.main_door_control(false); // 再开不动
        assert!(c.main_door.unwrap().opened);

        // Ghost 门：不动。
        c.main_door = Some(CastleUnit {
            opened: false,
            ghost: true,
            ..CastleUnit::default()
        });
        c.main_door_control(false);
        assert!(!c.main_door.unwrap().opened);

        // 无门：不 panic。
        c.main_door = None;
        c.main_door_control(true);
    }

    #[test]
    fn add_attacker_info_dedups_and_ignores_days_like_oracle() {
        let mut c = ready_castle();
        let (ok, _) = c.add_attacker_info(G_ATTACK, "攻方", now(10, 0));
        assert!(ok);
        // 照搬 `AddDateTimeOfDay` 忽略天数：AttackDate = 本刻，而不是 +2 天。
        assert_eq!(
            c.attack_war_list[0].attack_date,
            CastleDate::new(2026, 10, 13)
        );
        let (ok, ev) = c.add_attacker_info(G_ATTACK, "攻方", now(11, 0));
        assert!(!ok);
        assert!(ev.is_empty());
        assert_eq!(c.attack_war_list.len(), 1);
    }

    #[test]
    fn predicates_match_oracle() {
        let mut c = ready_castle();
        assert!(c.is_master_guild(G_DEFEND));
        assert!(c.is_member(G_DEFEND));
        assert!(!c.is_member(G_ATTACK));
        assert_eq!(c.in_palace_guild_count(), 0);
        c.attack_guild_list = vec![G_ATTACK, G_DEFEND];
        assert_eq!(c.in_palace_guild_count(), 2);

        // CheckInPalace(int,int)：只有「死亡且坐标命中」的墙算。
        assert!(!c.check_in_palace(624, 278));
        c.left_wall = Some(CastleUnit {
            death: true,
            x: 624,
            y: 278,
            ..CastleUnit::default()
        });
        assert!(c.check_in_palace(624, 278));
        assert!(!c.check_in_palace(625, 278));
    }

    /// N1（T-38）：10 个接口重载 + 自动占领整链 = 可观测 no-op（调用前后状态逐字段相同、零事件）。
    #[test]
    fn n1_entry_points_are_observable_noops() {
        let mut c = ready_castle();
        c.assert_n1_noop(|c| {
            assert_eq!(c.with_drawal_golds_n1(G_ATTACK, 100), -1);
        });
        c.assert_n1_noop(|c| c.get_castle_n1(G_ATTACK));
        c.assert_n1_noop(|c| {
            assert!(!c.add_attacker_info_n1(G_ATTACK));
        });
        c.assert_n1_noop(|c| {
            assert!(!c.can_get_castle_n1(G_ATTACK));
        });
        c.assert_n1_noop(|c| {
            assert!(!c.check_in_palace_player_n1(624, 278, G_ATTACK));
        });
        c.assert_n1_noop(|c| {
            assert_eq!(c.receipt_golds_n1(G_ATTACK, 100), -1);
        });
        c.assert_n1_noop(|c| {
            assert!(!c.is_attack_ally_guild_n1(G_ATTACK));
        });
        c.assert_n1_noop(|c| {
            assert!(!c.is_attack_guild_n1(G_ATTACK));
        });
        c.assert_n1_noop(|c| {
            assert!(!c.is_defense_ally_guild_n1(G_ATTACK));
        });
        c.assert_n1_noop(|c| {
            assert!(!c.is_defense_guild_n1(G_ATTACK));
        });
        // 自动占领整链：即使把攻方塞进攻击位，也不停战、不改归属。
        c.under_war = true;
        c.attack_guild_list = vec![G_ATTACK, G_DEFEND];
        c.assert_n1_noop(|c| {
            assert!(!c.try_auto_occupy_n1(G_ATTACK));
        });
        assert!(c.under_war);
        assert_eq!(c.attack_guild_list, vec![G_ATTACK, G_DEFEND]);
        assert_eq!(c.master_guild, Some(G_DEFEND));
    }

    impl UserCastle {
        fn assert_n1_noop(&mut self, call: impl FnOnce(&mut UserCastle)) {
            let before = self.clone();
            call(self);
            assert_eq!(*self, before, "N1 入口必须可观测 no-op（T-38）");
        }
    }
}
