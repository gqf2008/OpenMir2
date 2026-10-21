//! 攻城状态机（`UserCastle.Run` + `StartWallconquestWar` / `StopWallconquestWar` + 归属判定）
//! —— M3⑧-实现 7（卡 `openmir2-m3-castle-siege-state`）。
//!
//! 盘点依据：`mir2-rs/tests/parity/m3/M3⑧公会攻城现状盘点报告-20261013.md` §1.D / §1.E / §1.F / §1.L。
//! oracle 锚（行号均在本仓库 `src/` 内实测）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`UserCastle::run`] | `src/M2Server/Castle/UserCastle.cs:311-446`（每 tick；跨天重置 `UserCastle.cs:328-339`、每日开战 `UserCastle.cs:340-376`、回收 Ghost `UserCastle.cs:377-389`、围墙 `StoneMode` `UserCastle.cs:391-440`） |
//! | [`UserCastle::start_wallconquest_war`] | `UserCastle.cs:564-572`（刷皇宫 100 范围内玩家 `RefShowName`） |
//! | [`UserCastle::stop_wallconquest_war`] | `UserCastle.cs:577-597`（`UnderWar=false` + 清 `AttackGuildList` + 广播） |
//! | [`UserCastle::main_door_control`] | `UserCastle.cs:890-909`（幂等：只在 `IsOpened` 与入参不符时动） |
//! | [`UserCastle::add_attacker_info`] | `UserCastle.cs:998-1014`（`InAttackerList` 查重 + `AddDateTimeOfDay` + `SaveAttackSabukWall` + `SS_212`） |
//! | [`UserCastle::is_master_guild`] / [`UserCastle::is_member`] | `UserCastle.cs:639-642` / `UserCastle.cs:480-483` |
//! | [`UserCastle::in_palace_guild_count`] | `UserCastle.cs:604-607`（**返回值 = `AttackGuildList.Count`**，语义是「攻城行会数」，不是皇宫内玩家数） |
//! | [`UserCastle::check_in_palace`] | `UserCastle.cs:680-705`（任一城墙格死亡且坐标命中） |
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
//! | [`UserCastle::get_castle_n1`] | `UserCastle.cs:646` | `PlayObject.Message.cs:329`、`ChangeSabukLordCommand.cs:39` |
//! | [`UserCastle::add_attacker_info_n1`] | `UserCastle.cs:672` | `GuildOfficial.cs:219`（NPC 申请攻城） |
//! | [`UserCastle::can_get_castle_n1`] | `UserCastle.cs:677` | `PlayObject.Message.cs:327` |
//! | [`UserCastle::check_in_palace_player_n1`] | `UserCastle.cs:709` | `PlayObject.Operate.cs:371`（开城门前的皇宫判定） |
//! | [`UserCastle::receipt_golds_n1`] | `UserCastle.cs:913` | `CastleOfficial.cs:179`（金库上缴，⑧-9 面） |
//! | [`UserCastle::is_attack_ally_guild_n1`] | `UserCastle.cs:1042` | — |
//! | [`UserCastle::is_attack_guild_n1`] | `UserCastle.cs:1047` | `PlayObject.Message.cs:325` |
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
use mir2_shared::rng::RandomNumber;

/// `UserCastle.Archers = new ArcherUnit[12]`（`UserCastle.cs:48`）与 `CastleConst.MaxCastleArcher`
/// （`CastleConst.cs:10`）。`Guards` 数组长度同为 12（`UserCastle.cs:49`），
/// 与 `MaxCalsteGuard = 4`（雇佣上限）不同，别混用。
pub const MAX_CASTLE_ARCHER: usize = 12;
/// `Guards` 数组长度（`UserCastle.cs:49`）。
pub const MAX_CASTLE_GUARD: usize = 12;
/// `CastleWarRangeX` / `CastleWarRangeY` 的 `InCastleWarArea` 默认（`GameSvrConf.cs:1786-1787`）。
pub const DEFAULT_WAR_RANGE: i32 = 100;
/// `SM_AREASTATE` 的 bit0：`Envir.Flag.FightZone`（`PlayObject.Base.cs:274`）。
pub const AREA_STATE_FIGHT_ZONE: u16 = 1;
/// `SM_AREASTATE` 的 bit1：`Envir.Flag.SafeArea`（`PlayObject.Base.cs:279`）。
pub const AREA_STATE_SAFE_AREA: u16 = 2;
/// `SM_AREASTATE` 的 bit2（值 4）：`InGuildWarArea`（`PlayObject.Base.cs:283`）。
pub const AREA_STATE_GUILD_WAR: u16 = 4;

/// `Server.conf:740-745` 的四个名字色（部署值与 `GameSvrConf.cs:1731-1736` 缺省一致）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastleNameColors {
    /// `PKFlagNameColor`（部署 47）。
    pub pk_flag: u8,
    /// `AllyAndGuildNameColor`（部署 180）。
    pub ally_guild: u8,
    /// `WarGuildNameColor`（部署 69）。
    pub war_guild: u8,
    /// `InFreePKAreaNameColor`（部署 221）。
    pub in_free_pk: u8,
}

impl Default for CastleNameColors {
    fn default() -> Self {
        CastleNameColors {
            pk_flag: 47,
            ally_guild: 180,
            war_guild: 69,
            in_free_pk: 221,
        }
    }
}
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
    /// `GameSvrConf` 初值（`GameSvrConf.cs:1442-1446` 与 `GameSvrConf.cs:1782`）。
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
/// 只保留攻城状态机与门/墙状态机用到的字段；具体门/墙行为见 [`CastleUnit`] 的 `door_*`/`wall_*`
/// 方法（`CastleDoor.cs` / `WallStructure.cs`，⑧-8）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CastleUnit {
    /// 绑定到世界里的实体 `ActorId`（`Arc<ArcherUnit>.BaseObject.ActorId`）。
    ///
    /// `UserCastle.Initialize` 的 `RegenMonsterByName` 成功 ⇒ `Some(actor_id)`；配置存在但
    /// 造怪失败（`nHP<=0` 跳过 / 怪物库无此名）⇒ `None`。门/墙的每 tick `Run` 与死亡路由
    /// （`CastleDoor.Run`/`WallStructure.Run`/`CastleDoor.Die`）靠它把
    /// `World.entities[actor_id]` 与城堡状态机接起来。
    pub entity_id: Option<i32>,
    pub ghost: bool,
    pub death: bool,
    /// 城墙 `StoneMode`：非战时 `true`（不可破坏），战时 `false`。
    pub stone_mode: bool,
    /// `CastleDoor.IsOpened`（`CastleDoor.cs`）。
    pub opened: bool,
    /// `CastleDoor.HoldPlace`（`CastleDoor.cs:12/57/75`）。
    pub hold_place: bool,
    /// `WallStructure.SetMapFlaged`（`WallStructure.cs:7/53-66`）。
    pub set_map_flaged: bool,
    /// `WAbil.HP`（`SabukW.txt` 的 `nHP` 覆盖值；`Initialize` 写入，`Repair*` 回满）。
    pub hp: i32,
    /// `WAbil.MaxHP`（`RegenMonsterByName` 按 race 110/111 建的怪物 MaxHP）。
    pub max_hp: i32,
    /// `BaseObject.Dir`（`CastleDoor`/`WallStructure` 按 HP 比例写入）。
    pub dir: u8,
    pub x: i16,
    pub y: i16,
    /// `BaseObject.StruckTick`（`GetTickCount()` 口径；`RepairDoor`/`RepairWall` 的 60s 门）。
    pub struck_tick: i64,
    /// `AnimalObject.HealthTick`（`CastleDoor.Run`/`WallStructure.Run` 清零）。
    pub health_tick: i64,
    /// `AnimalObject.DeathTick`（死亡且属城堡时由 `Run` 刷 `GetTickCount()`）。
    pub death_tick: i64,
}

impl CastleUnit {
    /// `3 - Round(HP / MaxHP * 3.0)`（`CastleDoor.cs:66/96/108`、`WallStructure.cs:28/70`）。
    ///
    /// 用 [`hutil32_round`] 复刻 `HUtil32.Round`（`MidpointRounding.AwayFromZero`）。
    fn hp_ratio_dir(&self) -> i32 {
        3 - hutil32_round(f64::from(self.hp) / f64::from(self.max_hp) * 3.0)
    }

    /// `CastleDoor.SetMapXyFlag(nFlag)` 展开成逐格 `(x, y, walk)` 帧（顺序 = oracle 调用序）。
    fn door_map_flag_events(&self, n_flag: u8) -> Vec<StructureEvent> {
        door_map_xy_flag_ops(n_flag)
            .into_iter()
            .map(|(dx, dy, walk)| StructureEvent::SetMapXyFlag {
                x: self.x + dx,
                y: self.y + dy,
                walk,
            })
            .collect()
    }

    /// oracle `CastleDoor.Open()`（`CastleDoor.cs:46-58`）。
    ///
    /// `Death` ⇒ 提前返回；否则 `Dir=7` → `RM_DIGUP` → `IsOpened=true` → `StoneMode=true` →
    /// `SetMapXyFlag(0)` → `HoldPlace=false`。
    pub fn door_open(&mut self) -> Vec<StructureEvent> {
        if self.death {
            return Vec::new();
        }
        self.dir = 7;
        self.opened = true;
        self.stone_mode = true;
        self.hold_place = false;
        let mut events = vec![StructureEvent::RefDigUp {
            dir: self.dir,
            x: self.x,
            y: self.y,
        }];
        events.extend(self.door_map_flag_events(0));
        events
    }

    /// oracle `CastleDoor.Close()`（`CastleDoor.cs:60-76`）。
    ///
    /// `Death` ⇒ 提前返回；否则 `Dir=(byte)(3 - Round(HP/MaxHP*3))`、`Dir-3>=0 ⇒ 0` →
    /// `RM_DIGDOWN` → `IsOpened=false` → `StoneMode=false` → `SetMapXyFlag(1)` → `HoldPlace=true`。
    pub fn door_close(&mut self) -> Vec<StructureEvent> {
        if self.death {
            return Vec::new();
        }
        let mut n08 = self.hp_ratio_dir();
        if n08 - 3 >= 0 {
            n08 = 0;
        }
        self.dir = n08 as u8;
        self.opened = false;
        self.stone_mode = false;
        self.hold_place = true;
        let mut events = vec![StructureEvent::RefDigDown {
            dir: self.dir,
            x: self.x,
            y: self.y,
        }];
        events.extend(self.door_map_flag_events(1));
        events
    }

    /// oracle `CastleDoor.Die()`（`CastleDoor.cs:78-82`）：`base.Die()` 后 `SetMapXyFlag(2)`。
    ///
    /// `base.Die()` 自身的通用死亡帧不属本模块（由通用死亡路径发出）。
    pub fn door_die(&mut self) -> Vec<StructureEvent> {
        self.death = true;
        self.door_map_flag_events(2)
    }

    /// oracle `CastleDoor.Die()` 的**子类段**（`CastleDoor.cs:78-82`）：只发 `SetMapXyFlag(2)`。
    ///
    /// SuperMan 时 `base.Die()` 首行早退（`BaseObject.Base.cs:318-322`）⇒ `Death` **不**置位
    /// （`CastleDoor.Die` 的 `base.Die()` 被跳过、子类段照常执行）。`CastleUnit.death` 是
    /// `BaseObject.Death` 的镜像 ⇒ 本支**只出掩码、不改 `death`**。
    pub fn door_die_flag2_events(&self) -> Vec<StructureEvent> {
        self.door_map_flag_events(2)
    }

    /// oracle `CastleDoor.Run()`（`CastleDoor.cs:84-104`）。
    ///
    /// `Death && Castle != null` ⇒ `DeathTick = GetTickCount()`；否则 `HealthTick = 0`。
    /// 未开门时按 HP 比例刷 `Dir` 并发 `RM_TURN`（仅 `n08 < 3`）。
    pub fn door_run(&mut self, now_tick: i64, has_castle: bool) -> Vec<StructureEvent> {
        let mut events = Vec::new();
        if self.death && has_castle {
            self.death_tick = now_tick;
        } else {
            self.health_tick = 0;
        }
        if !self.opened {
            let n08 = self.hp_ratio_dir();
            if i32::from(self.dir) != n08 && n08 < 3 {
                self.dir = n08 as u8;
                events.push(StructureEvent::RefTurn {
                    dir: self.dir,
                    x: self.x,
                    y: self.y,
                });
            }
        }
        events
    }

    /// oracle `CastleDoor.RefStatus()`（`CastleDoor.cs:106-115`）：重置 `Dir` 并发 `RM_ALIVE`。
    pub fn door_ref_status(&mut self) -> Vec<StructureEvent> {
        let mut n08 = self.hp_ratio_dir();
        if n08 - 3 >= 0 {
            n08 = 0;
        }
        self.dir = n08 as u8;
        vec![StructureEvent::RefAlive {
            dir: self.dir,
            x: self.x,
            y: self.y,
        }]
    }

    /// oracle `WallStructure.Initialize()`（`WallStructure.cs:17-21`）：`Dir = 0` 后 `base.Initialize()`。
    pub fn wall_initialize(&mut self) {
        self.dir = 0;
    }

    /// oracle `WallStructure.RefStatus()`（`WallStructure.cs:23-40`）。
    ///
    /// `HP > 0 ⇒ 3 - Round(HP/MaxHP*3)`，否则 `4`；`n08 >= 5 ⇒ 0`；发 `RM_ALIVE`。
    pub fn wall_ref_status(&mut self) -> Vec<StructureEvent> {
        let mut n08 = if self.hp > 0 { self.hp_ratio_dir() } else { 4 };
        if n08 >= 5 {
            n08 = 0;
        }
        self.dir = n08 as u8;
        vec![StructureEvent::RefAlive {
            dir: self.dir,
            x: self.x,
            y: self.y,
        }]
    }

    /// oracle `WallStructure.Run()`（`WallStructure.cs:47-82`）。
    ///
    /// 死亡 ⇒ 刷 `DeathTick`，且若 `SetMapFlaged` 则把本格恢复 `Walk` 并清标记；
    /// 否则清 `HealthTick`，且若未标记则把本格置 `LowWall` 并置标记。随后按 `HP` 比例刷 `Dir`
    /// 并发 `RM_DIGUP`（仅 `n08 < 5`）。
    pub fn wall_run(&mut self, now_tick: i64) -> Vec<StructureEvent> {
        let mut events = Vec::new();
        if self.death {
            self.death_tick = now_tick;
            if self.set_map_flaged {
                events.push(StructureEvent::SetMapXyFlag {
                    x: self.x,
                    y: self.y,
                    walk: true,
                });
                self.set_map_flaged = false;
            }
        } else {
            self.health_tick = 0;
            if !self.set_map_flaged {
                events.push(StructureEvent::SetMapXyFlag {
                    x: self.x,
                    y: self.y,
                    walk: false,
                });
                self.set_map_flaged = true;
            }
        }
        let n08 = if self.hp > 0 { self.hp_ratio_dir() } else { 4 };
        if i32::from(self.dir) != n08 && n08 < 5 {
            self.dir = n08 as u8;
            events.push(StructureEvent::RefDigUp {
                dir: self.dir,
                x: self.x,
                y: self.y,
            });
        }
        events
    }
}

/// 复刻 `HUtil32.Round`（`src/OpenMir2/HUtil32.cs:131-134`）：
/// `(int)Math.Round(r, MidpointRounding.AwayFromZero)`。
///
/// `Math.Round(NaN)` 返回 `NaN`，`(int)NaN` 在 .NET Core 未检查转换下为 `0`；Rust 的 `as` 同为 `0`。
/// `f64::round` 的半数舍入方向与 `AwayFromZero` 一致。
#[must_use]
pub fn hutil32_round(r: f64) -> i32 {
    r.round() as i32
}

/// `CastleDoor.SetMapXyFlag(int nFlag)`（`CastleDoor.cs:23-44`）的**有序**格子掩码。
///
/// 每项 = `(dx, dy, walk)` 相对门坐标；同一格可重复出现（顺序敏感：
/// `Envir.SetMapXyFlag` 只在格子当前 `Valid` 时改属性 ⇒ `LowWall` 无法被后续 `true` 恢复）。
/// `nFlag=0`（开） 15 项 / `nFlag=1`（关） 12 项 / `nFlag=2`（破） 12 项。
#[must_use]
pub fn door_map_xy_flag_ops(n_flag: u8) -> Vec<(i16, i16, bool)> {
    // `CastleDoor.cs:25-27`：三格先无条件置 `true`。
    let mut ops = vec![(0_i16, -2_i16, true), (1, -1, true), (1, -2, true)];
    // `CastleDoor.cs:28`：`boFlag = nFlag != 1`。
    let bo_flag = n_flag != 1;
    // `CastleDoor.cs:29-37`：九格置 `boFlag`。
    ops.push((0, 0, bo_flag));
    ops.push((0, -1, bo_flag));
    ops.push((0, -2, bo_flag));
    ops.push((1, -1, bo_flag));
    ops.push((1, -2, bo_flag));
    ops.push((-1, 0, bo_flag));
    ops.push((-2, 0, bo_flag));
    ops.push((-1, -1, bo_flag));
    ops.push((-1, 1, bo_flag));
    // `CastleDoor.cs:38-43`：`nFlag == 0` 时再把那三格置 `false`。
    if n_flag == 0 {
        ops.push((0, -2, false));
        ops.push((1, -1, false));
        ops.push((1, -2, false));
    }
    ops
}

/// `CastleOfficial.cs:52-67` 的 `$CASTLEDOORSTATE` 文本。
///
/// `Death ⇒ "destroyed"`；`IsOpened ⇒ "opened"`；否则 `"closed"`。
/// **登记**：oracle 对 `Castle.MainDoor.BaseObject` **无 null 检查**（为空会抛 `NullReferenceException`）
/// ⇒ 本函数要求调用方已确认门存在。
#[must_use]
pub fn door_state_text(door: &CastleUnit) -> &'static str {
    if door.death {
        DOOR_STATE_DESTROYED
    } else if door.opened {
        DOOR_STATE_OPENED
    } else {
        DOOR_STATE_CLOSED
    }
}

/// `HUtil32.CompareLStr(src, targ)`（`src/OpenMir2/HUtil32.cs:550-576`）：大小写不敏感的前缀匹配。
///
/// oracle 语义：`compn = targ.Length`（UTF-16 码元数）；`compn <= 0`（空 `targ`）⇒ `false`；
/// `src.Length < compn` ⇒ `false`；否则对 `i in 0..compn` 逐码元
/// `char.ToUpper(src[i]) == char.ToUpper(targ[i])`，全等 ⇒ `true`。本移植按 Rust `char` 逐位比较
/// （BMP 下与 UTF-16 码元一致；本函数调用面 = `HIRE_GUARD_NOW` / `HIRE_ARCHER_NOW` 等全 ASCII），
/// ASCII 大小写折叠与既有口径一致。
///
/// **不做字节切片**（⑧-8 reviewer-5 O-1）：旧实现 `src[..targ.len()]` 在 `targ.len()` 恰好落在
/// 某个多字节 `char` 中间时按 Rust 语义 **panic**，而 oracle 的逐码元循环对任意输入都不会抛
/// （越界前缀只在 UTF-16 码元级比较、不切字节）。逐 `char` 比较对任意 UTF-8 输入都不 panic，
/// 且对 ASCII `targ` 与 oracle 逐码元结果一致（首位不等 ⇒ `false`）。
#[must_use]
pub fn compare_lstr(src: &str, targ: &str) -> bool {
    let mut s = src.chars();
    let mut t = targ.chars();
    loop {
        match (t.next(), s.next()) {
            (Some(tc), Some(sc)) => {
                if !sc.eq_ignore_ascii_case(&tc) {
                    return false;
                }
            }
            // `compn <= 0`（空 targ）⇒ `false`；targ 全部匹配完 ⇒ `true`。
            (None, _) => return !targ.is_empty(),
            // `src.Length < compn`（src 短于 targ）⇒ `false`。
            (Some(_), None) => return false,
        }
    }
}

/// `CastleOfficial.UserSelect` 的标签派发（`CastleOfficial.cs:199-238`）。
///
/// 精确匹配（`string.Compare(..., OrdinalIgnoreCase)`）先于 `CompareLStr` 前缀匹配；
/// `@hireguardnow*`/`@hirearchernow*` 归 [`CastleOfficialAction::HireUnreachable`]（N5，T-40）。
/// 未命中任何分支 ⇒ `None`。
#[must_use]
pub fn castle_official_action(label: &str) -> Option<CastleOfficialAction> {
    if label.eq_ignore_ascii_case(OPEN_MAIN_DOOR) {
        Some(CastleOfficialAction::OpenMainDoor)
    } else if label.eq_ignore_ascii_case(CLOSE_MAIN_DOOR) {
        Some(CastleOfficialAction::CloseMainDoor)
    } else if label.eq_ignore_ascii_case(REPAIR_DOOR_NOW) {
        Some(CastleOfficialAction::RepairDoor)
    } else if label.eq_ignore_ascii_case(REPAIR_WALL_NOW_1) {
        Some(CastleOfficialAction::RepairWall(1))
    } else if label.eq_ignore_ascii_case(REPAIR_WALL_NOW_2) {
        Some(CastleOfficialAction::RepairWall(2))
    } else if label.eq_ignore_ascii_case(REPAIR_WALL_NOW_3) {
        Some(CastleOfficialAction::RepairWall(3))
    } else if compare_lstr(label, HIRE_GUARD_NOW) || compare_lstr(label, HIRE_ARCHER_NOW) {
        Some(CastleOfficialAction::HireUnreachable)
    } else {
        None
    }
}

/// **N5（白名单 T-40）**：`CastleOfficial.cs:229/235` 的
/// `sLabel.Substring(prefix.Length, sLabel.Length)` 复刻。
///
/// 第二参（length）用**整串长度** ⇒ `prefix.len() + label.len() > label.len()` 恒真
/// ⇒ 必抛 `ArgumentOutOfRangeException`（被 `UserSelect` 的 `catch` 吞）⇒ 雇佣命令不可达。
///
/// # Errors
/// 恒返 [`HireSubstringError::ArgumentOutOfRange`]（照搬 oracle）。
pub fn hire_substring<'a>(label: &'a str, prefix: &str) -> Result<&'a str, HireSubstringError> {
    let start = prefix.len();
    let length = label.len();
    if start + length > label.len() {
        return Err(HireSubstringError::ArgumentOutOfRange);
    }
    // 照搬后不可达：`start + length > label.len()` 恒真。
    Ok(&label[start..start + length])
}

/// `CastleOfficial.RepairDoor(PlayObject)`（`CastleOfficial.cs:358-381`）。
///
/// `TotalGold < RepairDoorPrice` ⇒ 只回「城内资金不足!!!」；否则调 [`UserCastle::repair_door`]，
/// 成功才扣 `TotalGold`。
#[must_use]
pub fn official_repair_door(
    castle: &mut UserCastle,
    price: i32,
    now_tick: i64,
) -> OfficialRepairResult {
    if castle.total_gold < price {
        return OfficialRepairResult {
            repaired: false,
            gold_spent: 0,
            message: OFFICIAL_REPAIR_NOT_ENOUGH_GOLD_MSG,
            events: Vec::new(),
        };
    }
    let (repaired, events) = castle.repair_door(now_tick);
    if repaired {
        castle.total_gold -= price;
        OfficialRepairResult {
            repaired: true,
            gold_spent: price,
            message: OFFICIAL_REPAIR_OK_MSG,
            events,
        }
    } else {
        OfficialRepairResult {
            repaired: false,
            gold_spent: 0,
            message: OFFICIAL_REPAIR_NOT_NEEDED_MSG,
            events,
        }
    }
}

/// `CastleOfficial.RepairWallNow(nWallIndex, PlayObject)`（`CastleOfficial.cs:383-406`）。
///
/// 与 [`official_repair_door`] 同构，用 `RepairWallPrice` 与 `RepairWall(nWallIndex)`。
#[must_use]
pub fn official_repair_wall(
    castle: &mut UserCastle,
    wall_index: u8,
    price: i32,
    now_tick: i64,
) -> OfficialRepairResult {
    if castle.total_gold < price {
        return OfficialRepairResult {
            repaired: false,
            gold_spent: 0,
            message: OFFICIAL_REPAIR_NOT_ENOUGH_GOLD_MSG,
            events: Vec::new(),
        };
    }
    let (repaired, events) = castle.repair_wall(wall_index, now_tick);
    if repaired {
        castle.total_gold -= price;
        OfficialRepairResult {
            repaired: true,
            gold_spent: price,
            message: OFFICIAL_REPAIR_OK_MSG,
            events,
        }
    } else {
        OfficialRepairResult {
            repaired: false,
            gold_spent: 0,
            message: OFFICIAL_REPAIR_NOT_NEEDED_MSG,
            events,
        }
    }
}

/// 把 [`StructureEvent::SetMapXyFlag`] 按序作用到地图格子（`Envir.SetMapXyFlag` 口径）。
///
/// 非掩码事件被忽略；返回实际改到属性的格子数。
pub fn apply_structure_events(grid: &mut crate::map::MapGrid, events: &[StructureEvent]) -> usize {
    let mut changed = 0;
    for event in events {
        if let StructureEvent::SetMapXyFlag { x, y, walk } = event {
            if grid.set_map_xy_flag(i32::from(*x), i32::from(*y), *walk) {
                changed += 1;
            }
        }
    }
    changed
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
    /// 门/墙的 `SendRefMsg` 帧或 `Envir.SetMapXyFlag` 通行掩码（⑧-8）。顺序 = oracle 调用序。
    Structure(StructureEvent),
    /// `UserCastle.Initialize` 里一次成功的 `RegenMonsterByName` + `WAbil.HP=nHP` + `Castle=this`
    /// + 弓手 `GuardDirection=3`（⑧-6 第②片）。顺序 = oracle 调用序。
    SpawnDefenseUnit(DefenseSpawn),
}

/// `SystemModule.Data.ArcherUnit` 的一条配置（`src/Modules/SystemModule/Data/ArcherUnit.cs:8-16`）。
///
/// `CastleConf.LoadConfig`（`src/M2Server/Castle/CastleConf.cs:39-93`）填好，`UserCastle.Initialize`
/// （`src/M2Server/Castle/UserCastle.cs:108-226`）消费。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefenseUnitSpec {
    /// `sName`（喂 `WorldEngine.RegenMonsterByName`）。
    pub name: String,
    /// `nX`。
    pub x: i16,
    /// `nY`。
    pub y: i16,
    /// `nStatus`——**只有 `MainDoor` 有意义**（= `CastleConf` 的 `[Defense] MainDoorOpen`，
    /// `CastleConf.cs:53`；三面墙的 `nStatus` 从不赋值 ⇒ 恒 `false`）；`Initialize` 里
    /// `if (MainDoor.nStatus) ((CastleDoor)MainDoor.BaseObject).Open();`（`UserCastle.cs:135-138`）。
    pub status: bool,
    /// `nHP`（`WAbil.HP` 的覆盖值）。弓手/守卫 `nHP <= 0` 时 `Initialize` 直接 `continue`
    /// （`UserCastle.cs:177/197`）。
    pub hp: u16,
}

/// `UserCastle.Initialize` 的输入 = `CastleConf.LoadConfig` 结果里 `Initialize` 用到的子集。
///
/// 与 `crates/envir` 的 `SabukWConfig`（⑧-6 第①片读侧）字段一一对应，但**本 crate 不依赖
/// `mir2-envir`**（依赖方向 `bins -> envir/world`）⇒ 由接线层把 `SabukWConfig` 适配成本结构
/// （接线属第②片的 `bins/game-svr` 面，等 owner 对 bins 串行序裁决后另片交付）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastleDefenseSpecs {
    /// `[Setup] OwnGuild`（`CastleConf.cs:18`，缺省 `""`）。
    ///
    /// **读序照抄**：`MasterGuild` 的反查在 `CastleMgr.Initialize`（`GameApp.cs:213`）里，
    /// 早于 `GuildMgr.LoadGuildInfo`（`GameApp.cs:245`）⇒ 启动时恒为 `None`；本结构只搬名字，
    /// 由 [`UserCastle::initialize`] 落到 `own_guild`；[`UserCastle::is_master_guild_name`]
    /// 照 oracle 在 `MasterGuild == None` 时恒 false（登记 `whitelist.md` T-131）。
    pub own_guild: String,
    /// `[Defense] CastleMap`（`CastleConf.cs:39`，缺省 `"3"`）。
    pub map_name: String,
    /// `[Defense] CastleHomeMap`（`CastleConf.cs:40`，缺省 `"3"`）。
    pub home_map: String,
    /// `[Defense] CastleHomeX`（`CastleConf.cs:41`，缺省 644）。
    pub home_x: i16,
    /// `[Defense] CastleHomeY`（`CastleConf.cs:42`，缺省 290）。
    pub home_y: i16,
    /// `[Defense] CastleWarRangeX`（`CastleConf.cs:43`，缺省 100）。
    pub war_range_x: i32,
    /// `[Defense] CastleWarRangeY`（`CastleConf.cs:44`，缺省 100）。
    pub war_range_y: i32,
    /// `[Defense] CastleMapList`（`CastleConf.cs:24-37`，逗号分隔，**保留重复与顺序**）。
    pub envir_list: Vec<String>,
    /// `[Defense] CastlePlaceMap`（皇宫地图，`CastleConf.cs:45`）。
    pub palace_map: String,
    /// `[Defense] CastleSecretMap`（密道地图，`CastleConf.cs:46`）。
    pub secret_map: String,
    /// `MainDoor`（`CastleConf.cs:49-55`）。
    pub main_door: DefenseUnitSpec,
    /// `LeftWall`（`CastleConf.cs:56-61`）。
    pub left_wall: DefenseUnitSpec,
    /// `CenterWall`（`CastleConf.cs:62-67`）。
    pub center_wall: DefenseUnitSpec,
    /// `RightWall`（`CastleConf.cs:68-73`）。
    pub right_wall: DefenseUnitSpec,
    /// `Archers[12]`（`CastleConf.cs:74-83`）。
    pub archers: Vec<DefenseUnitSpec>,
    /// `Guards[12]`（`CastleConf.cs:84-93`）。
    pub guards: Vec<DefenseUnitSpec>,
}

/// 写盘面里一个门 / 墙 / 弓手 / 守卫槽的纯数据（`CastleConf.SaveConfig` 口径）。
///
/// 名称与坐标恒来自配置面（[`CastleDefenseSpecs`]，= `CastleConf.LoadConfig` 的读入值），
/// 因为 oracle 写的是 `ArcherUnit.sName/nX/nY`，与实体是否造出无关；`hp` 才是运行期值。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastleSaveUnit {
    /// `sName`（`CastleConf.cs:198/216/...`，空串 ⇒ 不写该键）。
    pub name: String,
    /// `nX` / `nY`（`0` ⇒ 不写该键）。
    pub x: i16,
    pub y: i16,
    /// `WAbil.HP`：门/墙只在 `BaseObject != null` 时写（`CastleConf.cs:203-265`）⇒ 未挂实体 `None`；
    /// 弓手/守卫无条件写（无实体写 `0`，`CastleConf.cs:283-290/304-313`）⇒ 恒 `Some`。
    pub hp: Option<i32>,
    /// `MainDoorOpen`（`CastleConf.cs:203-207`，`WriteBool`）：**只有主门 `BaseObject != null`**
    /// 时有值；三面墙与该键无关 ⇒ 恒 `None`。
    pub open: Option<bool>,
}

/// [`UserCastle::save_snapshot`] 的成功产物：`CastleConf.SaveConfig`（`CastleConf.cs:95-320`）
/// 的**全字段写面**，纯内存、无 IO。
///
/// 路径拼接 / GBK 编码 / 截断写字节属 P2（`crates/envir`）；本结构只负责把「写什么值」定死。
/// 段/键的写条件（非空、`!= 0`、`BaseObject != null`）也留给写侧按 oracle 逐键判定——快照
/// 如实携带原值（`0`/空串/`None`），不预先裁剪。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastleSaveSnapshot {
    // ---- [Setup]（`CastleConf.cs:109-128`）----
    /// `sName`。
    pub castle_name: String,
    /// `OwnGuild`。
    pub own_guild: Option<String>,
    /// `ChangeDate`（原串，`yyyy-MM-dd HH:mm:ss` 口径）。
    pub change_date: Option<String>,
    /// `WarDate`。
    pub war_date: Option<CastleDate>,
    /// `IncomeToday`。
    pub income_today: CastleDate,
    /// `TotalGold`。
    pub total_gold: i32,
    /// `TodayIncome`。
    pub today_income: i32,
    // ---- [Defense]（`CastleConf.cs:130-317`）----
    /// `CastleMapList`（保留重复与顺序；写侧逐项加 `,`）。
    pub envir_list: Vec<String>,
    /// `CastleMap`。
    pub map_name: Option<String>,
    /// `CastleHomeMap`。
    pub home_map: String,
    /// `CastleHomeX` / `CastleHomeY`。
    pub home_x: i16,
    pub home_y: i16,
    /// `CastleWarRangeX` / `CastleWarRangeY`。
    pub war_range_x: i32,
    pub war_range_y: i32,
    /// `MainDoor*`（`MainDoorOpen` 在 `open`，仅主门有值）。
    pub main_door: CastleSaveUnit,
    /// `LeftWall*` / `CenterWall*` / `RightWall*`。
    pub left_wall: CastleSaveUnit,
    pub center_wall: CastleSaveUnit,
    pub right_wall: CastleSaveUnit,
    /// `Archer_{i+1}_*`，长度 = 配置槽数（生产恒 [`MAX_CASTLE_ARCHER`]）。
    pub archers: Vec<CastleSaveUnit>,
    /// `Guard_{i+1}_*`，长度 = 配置槽数（生产恒 [`MAX_CASTLE_GUARD`]）。
    pub guards: Vec<CastleSaveUnit>,
    // ---- 路径口径（§1.6「读/写不同源」）----
    /// `Castle/<config_dir>/AttackSabukWall.txt`（运行期 `ConfigDir`，申请文件）。
    pub config_dir: String,
    /// `Castle/<ctor_config_dir>/SabukW.txt`（ctor 绑定目录，配置文件）。
    pub ctor_config_dir: String,
}

/// [`UserCastle::save_snapshot`] 的跳过原因（= oracle 写盘路径上的两道门）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastleSaveSkip {
    /// 未 `LoadConfig`（`specs == None`，只跑 ctor 的装载城）：oracle 在
    /// `Archers[i].nX` 抛 `NullReferenceException`（`CastleConf.cs:266-269`）。
    NreArchersNull,
    /// `MapMgr.GetMapOfServerIndex(MapName) != ServerIndex`：`CastleConf.SaveConfig`
    /// 直接 `return`（`CastleConf.cs:105-107`）。
    MapOwnerNotSelf,
}

/// `WorldEngine.RegenMonsterByName` 造出的怪物（本模块只取 `WAbil.MaxHP`）。
///
/// `UserCastle.Initialize` 只覆盖 `WAbil.HP = nHP`，**不改** `MaxHP`（`UserCastle.cs:133-211`）
/// ⇒ 门/墙的 `Dir = 3 - Round(HP/MaxHP*3)`（⑧-8）用的 `MaxHP` 来自怪物模板。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefenseCreature {
    /// 怪物模板的 `WAbil.MaxHP`。
    pub max_hp: i32,
    /// `CreateMonster` 成功返回的 `cert.ActorId`（世界实体 id）。
    pub actor_id: i32,
}

/// `Initialize` 里的防御单元角色（枚举顺序 = `RegenMonsterByName` 调用序）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefenseRole {
    MainDoor,
    LeftWall,
    CenterWall,
    RightWall,
    /// `Archers[i]`（`i` = 下标）。
    Archer(usize),
    /// `Guards[i]`（`i` = 下标）。
    Guard(usize),
}

/// `Initialize` 成功造出并完成字段绑定后的一个防御单元（可观测副作用）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefenseSpawn {
    /// 角色（决定 oracle 的绑定分支）。
    pub role: DefenseRole,
    /// 世界实体 `ActorId`（`RegenMonsterByName` 的返回句柄）。
    pub actor_id: i32,
    /// `MapName`。
    pub map_name: String,
    /// `sName`。
    pub name: String,
    /// `CurrX` / `CurrY`（= 配置坐标）。
    pub x: i16,
    pub y: i16,
    /// `WAbil.HP = nHP`。
    pub hp: u16,
    /// `BaseObject.Castle = this`——**门/墙/弓手为 `true`；守卫为 `false`**
    /// （`UserCastle.cs:134/148/158/168/186` 设，`UserCastle.cs:202-206` 守卫生成后**不设**）。
    pub castle_owner: bool,
    /// 弓手 `((GuardUnit)BaseObject).GuardDirection = 3`（`UserCastle.cs:187`）；其余恒 `None`。
    pub guard_direction: Option<i8>,
}

/// `UserCastle.Initialize`（`UserCastle.cs:108-226`）的宿主面：地图存在性 + 按名造怪 + 告警。
///
/// **调用序 = oracle**：宿主实现**不得**重排/批量——`regen_monster_by_name` 必须在
/// 「主门 → 左墙 → 中墙 → 右墙 → 弓手 `i=0..12` → 守卫 `i=0..12`」的遍历里被同步调用，
/// 因为造怪本身可能消耗全局随机数/实体槽（与本卡的 RNG 账本口径一致）。
pub trait CastleInitializeEnv {
    /// `SystemShare.MapMgr.GetMapOfServerIndex(MapName) == M2Share.ServerIndex`（`UserCastle.cs:113`）。
    fn map_owner_is_self(&self, map_name: &str) -> bool;
    /// `SystemShare.MapMgr.FindMap(name) != null`（`UserCastle.cs:115/121/127`）。
    fn find_map(&self, map_name: &str) -> bool;
    /// `FindMap(name)` 的注册表 id（本仓 `MapRegistry::id_of`；`None` ⇔ oracle `null`）。
    ///
    /// 用于把 `CastleEnvir`/`PalaceEnvir`/`SecretEnvir` 与 `EnvirList` 归一成 id，
    /// 让 [`UserCastle::in_castle_war_area`] 的引用相等语义可离线判定。
    fn map_id(&self, map_name: &str) -> Option<usize>;
    /// `SystemShare.WorldEngine.RegenMonsterByName(map, x, y, name)`：造出怪物则返回其模板
    /// `MaxHP`，怪物库无此名 ⇒ `None`（oracle 返回 `null`）。
    fn regen_monster_by_name(
        &mut self,
        map_name: &str,
        x: i16,
        y: i16,
        name: &str,
    ) -> Option<DefenseCreature>;
    /// `LogService.Warn(text)`。
    fn warn(&mut self, text: String);
}

/// `Initialize` 里「造怪 → 写 `WAbil.HP` → 绑定 → 收集事件」的一步（顺序 = oracle）。
///
/// 造怪失败（`None`）**不**产生事件，由调用方按角色发对应告警。
fn regen_and_bind(
    env: &mut impl CastleInitializeEnv,
    map_name: &str,
    role: DefenseRole,
    spec: &DefenseUnitSpec,
    castle_owner: bool,
    guard_direction: Option<i8>,
    events: &mut Vec<CastleEvent>,
) -> Option<CastleUnit> {
    let creature = env.regen_monster_by_name(map_name, spec.x, spec.y, &spec.name)?;
    let mut unit = CastleUnit {
        entity_id: Some(creature.actor_id),
        x: spec.x,
        y: spec.y,
        hp: i32::from(spec.hp),
        max_hp: creature.max_hp,
        ..CastleUnit::default()
    };
    events.push(CastleEvent::SpawnDefenseUnit(DefenseSpawn {
        role,
        actor_id: creature.actor_id,
        map_name: map_name.to_string(),
        name: spec.name.clone(),
        x: spec.x,
        y: spec.y,
        hp: spec.hp,
        castle_owner,
        guard_direction,
    }));
    // `UserCastle.cs:135-138`：`if (MainDoor.nStatus) ((CastleDoor)BaseObject).Open();`
    // `Open()` 的 `RM_DIGUP` + `SetMapXyFlag(0)` 在绑定之后产生（⑧-8 [`CastleUnit::door_open`]）。
    if spec.status {
        for e in unit.door_open() {
            events.push(CastleEvent::Structure(e));
        }
    }
    Some(unit)
}

/// `UserCastle.RepairDoor`/`RepairWall` 的 60s `StruckTick` 门（`UserCastle.cs:920/956`）。
pub const STRUCTURE_REPAIR_DELAY_MS: i64 = 60 * 1000;

/// `ScriptFlagConst.sOPENMAINDOOR`（`ScriptFlagConst.cs:75`）。
pub const OPEN_MAIN_DOOR: &str = "@openmaindoor";
/// `ScriptFlagConst.sCLOSEMAINDOOR`（`ScriptFlagConst.cs:76`）。
pub const CLOSE_MAIN_DOOR: &str = "@closemaindoor";
/// `ScriptFlagConst.sREPAIRDOORNOW`（`ScriptFlagConst.cs:77`）。
pub const REPAIR_DOOR_NOW: &str = "@repairdoornow";
/// `ScriptFlagConst.sREPAIRWALLNOW1`（`ScriptFlagConst.cs:78`）。
pub const REPAIR_WALL_NOW_1: &str = "@repairwallnow1";
/// `ScriptFlagConst.sREPAIRWALLNOW2`（`ScriptFlagConst.cs:79`）。
pub const REPAIR_WALL_NOW_2: &str = "@repairwallnow2";
/// `ScriptFlagConst.sREPAIRWALLNOW3`（`ScriptFlagConst.cs:80`）。
pub const REPAIR_WALL_NOW_3: &str = "@repairwallnow3";
/// `ScriptFlagConst.sHIREARCHERNOW`（`ScriptFlagConst.cs:81`）。
pub const HIRE_ARCHER_NOW: &str = "@hirearchernow";
/// `ScriptFlagConst.sHIREGUARDNOW`（`ScriptFlagConst.cs:82`）。
pub const HIRE_GUARD_NOW: &str = "@hireguardnow";

/// `CastleOfficial` 的城门文本（`CastleOfficial.cs:52-67` 的 `$CASTLEDOORSTATE`）。
pub const DOOR_STATE_DESTROYED: &str = "destroyed";
/// `CastleOfficial.cs:59-61`：`IsOpened` ⇒ `"opened"`。
pub const DOOR_STATE_OPENED: &str = "opened";
/// `CastleOfficial.cs:63-65`：否则 `"closed"`。
pub const DOOR_STATE_CLOSED: &str = "closed";

/// `CastleOfficial` 修门/修墙的提示文案（`CastleOfficial.cs:358-406`）。
pub const OFFICIAL_REPAIR_OK_MSG: &str = "修理成功。";
/// `CastleOfficial.cs:374/399`：修理未生效（HP 满 / `UnderWar` / 60s 门未过）。
pub const OFFICIAL_REPAIR_NOT_NEEDED_MSG: &str = "城门不需要修理!!!";
/// `CastleOfficial.cs:379/404`：`TotalGold` 不足。
pub const OFFICIAL_REPAIR_NOT_ENOUGH_GOLD_MSG: &str = "城内资金不足!!!";

/// `CastleDoor`/`WallStructure` 的一次可观测副作用（顺序 = oracle 调用序）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureEvent {
    /// `SendRefMsg(RM_DIGUP, dir, x, y, 0, "")`（`CastleDoor.Open` / `WallStructure.Run`）。
    RefDigUp { dir: u8, x: i16, y: i16 },
    /// `SendRefMsg(RM_DIGDOWN, dir, x, y, 0, "")`（`CastleDoor.Close`）。
    RefDigDown { dir: u8, x: i16, y: i16 },
    /// `SendRefMsg(RM_TURN, dir, x, y, 0, "")`（`CastleDoor.Run`）。
    RefTurn { dir: u8, x: i16, y: i16 },
    /// `SendRefMsg(RM_ALIVE, dir, x, y, 0, "")`（`RefStatus`）。
    RefAlive { dir: u8, x: i16, y: i16 },
    /// `Envir.SetMapXyFlag(x, y, flag)`：`walk=true` ↔ `CellAttribute.Walk`；宿主按序作用到地图格子。
    SetMapXyFlag { x: i16, y: i16, walk: bool },
}

/// `CastleOfficial.UserSelect` 的城门/城墙标签动作（`CastleOfficial.cs:199-238`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastleOfficialAction {
    /// `@openmaindoor` ⇒ `Castle.MainDoorControl(false)`。
    OpenMainDoor,
    /// `@closemaindoor` ⇒ `Castle.MainDoorControl(true)`。
    CloseMainDoor,
    /// `@repairdoornow` ⇒ `RepairDoor(PlayObject)`。
    RepairDoor,
    /// `@repairwallnow1/2/3` ⇒ `RepairWallNow(1/2/3, PlayObject)`。
    RepairWall(u8),
    /// **N5（白名单 T-40）**：`@hireguardnow*`/`@hirearchernow*` 的 `Substring` 长度实参恒越界
    /// ⇒ oracle 抛 `ArgumentOutOfRangeException` 被 `catch` 吞 ⇒ 雇佣命令**不可达**（静默无效果）。
    HireUnreachable,
}

/// `CastleOfficial.RepairDoor`/`RepairWallNow` 的结果（`CastleOfficial.cs:358-406`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfficialRepairResult {
    /// 是否真的修好（`Castle.RepairDoor/Wall` 返回 `true`）。
    pub repaired: bool,
    /// 实际扣掉的 `TotalGold`（成功才扣）。
    pub gold_spent: i32,
    /// `PlayObject.SysMsg` 文案。
    pub message: &'static str,
    /// 修好时 `RefStatus` 产生的帧/掩码（顺序 = oracle 调用序）。
    pub events: Vec<StructureEvent>,
}

/// N5：`Substring(startIndex, length)` 长度实参越界的复刻错误（白名单 T-40）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HireSubstringError {
    /// `13 + sLabel.Length > sLabel.Length` 恒真 ⇒ `ArgumentOutOfRangeException`。
    ArgumentOutOfRange,
}

/// `UserCastle` 的攻城状态机（oracle `src/M2Server/Castle/UserCastle.cs`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserCastle {
    /// `sName`（广播文案里的城堡名）。
    pub name: String,
    /// `MapName`（`CastleConf` 的 `[Setup] MapName`；空 ⇒ `Run` 直接返回）。
    pub map_name: Option<String>,
    /// `HomeMap`（`CastleConf` 的 `[Defense] CastleHomeMap`；死亡回城支用）。
    pub home_map: String,
    /// `CastleEnvir`（`FindMap(MapName)`）的注册表 id（`None` ⇔ oracle `null`）。
    pub castle_map_id: Option<usize>,
    /// `PalaceEnvir`（`FindMap(CastlePlaceMap)`）的注册表 id。
    pub palace_map_id: Option<usize>,
    /// `SecretEnvir`（`FindMap(CastleSecretMap)`）的注册表 id。
    pub secret_map_id: Option<usize>,
    /// `EnvirList` 逐项解析出的 `map_id`（`InCastleWarArea` 的第三支）。
    pub envir_map_ids: Vec<usize>,
    /// `WarRangeX` / `WarRangeY`（`InCastleWarArea` 的严格 `<` 边界）。
    pub war_range_x: i32,
    pub war_range_y: i32,
    /// `M2Share.ServerIndex`。
    pub server_index: i32,
    /// `SystemShare.MapMgr.GetMapOfServerIndex(MapName)`（归属服）。
    pub map_owner_server_index: i32,
    /// `OwnGuild` 经 `GuildMgr.FindGuild` 反查得到的 `MasterGuild`（可能为 `None`）。
    pub master_guild: Option<GuildId>,
    /// `OwnGuild`（落盘字符串口径，`GetCastle` 会写；本卡只读）。
    pub own_guild: Option<String>,
    /// `ChangeDate` 的原串（`[Setup] ChangeDate`；`CastleConf.LoadConfig` 读入时保原样，
    /// [`UserCastle::get_castle`] 写入当前时间串）。
    pub change_date: Option<String>,
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
    /// `TotalGold`（⑧-9 税收面；写入/封顶在 [`crate::castle_tax::UserCastle::inc_rate_gold`]）。
    pub total_gold: i32,
    /// `SaveTick`（`UserCastle.cs:73`）：`IncRateGold` 的 10 分钟落盘节流口（⑧-9）。
    pub save_gold_tick: i64,
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
    /// `CastleConf.LoadConfig` 读入的配置面（`Archer_i_X/Y/Name`、`Guard_i_*`、`MainDoorOpen`
    /// 等写回所需的原值，`CastleConf.cs:15-94`）。
    ///
    /// [`UserCastle::initialize`] 起手落 `Some`（oracle 的 `LoadConfig` 在归属门之前执行）；
    /// 保持 `None` 的城 = 只跑了 ctor 的装载城（`CastleMgr.LoadCastleList` 只 `new`、
    /// 不 `Initialize`）——写盘时对应 oracle 的 `Archers[i] == null` NRE
    /// （[`CastleSaveSkip::NreArchersNull`]）。
    pub specs: Option<CastleDefenseSpecs>,
    /// oracle `ConfigDir`（`UserCastle.cs:34`）：ctor 入参，`CastleManager.Initialize`
    /// 先传 `Config.CastleDir`（= `"Castle"`）后改 `"0"`（`CastleManager.cs:66-75`）。
    /// **申请文件**路径用它（`UserCastle.SaveAttackSabukWall` 读的是运行期 `ConfigDir`）。
    pub config_dir: String,
    /// ctor 绑定目录（`UserCastle.cs:50-55` 的 `filePath`）：**配置文件**路径用它
    /// （`CastleConf.SaveConfig` 写的是 ctor 里定死的 `castleConf.FileName`，不是运行期
    /// `ConfigDir`，§1.3）。
    pub ctor_config_dir: String,
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
            home_map: "3".to_string(),
            castle_map_id: None,
            palace_map_id: None,
            secret_map_id: None,
            envir_map_ids: Vec::new(),
            war_range_x: DEFAULT_WAR_RANGE,
            war_range_y: DEFAULT_WAR_RANGE,
            server_index: 0,
            map_owner_server_index: 0,
            master_guild: None,
            own_guild: None,
            change_date: None,
            under_war: false,
            is_start_war: false,
            show_over_msg: false,
            war_date: None,
            start_castle_war_tick: 0,
            today_income: 0,
            total_gold: 0,
            save_gold_tick: 0,
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
            specs: None,
            config_dir: String::new(),
            ctor_config_dir: String::new(),
            config,
        }
    }

    /// 按 [`DefenseRole`] 取已绑定的门/墙/弓手/守卫（`Initialize` 成功者）。
    #[must_use]
    pub fn defense_unit(&self, role: DefenseRole) -> Option<&CastleUnit> {
        match role {
            DefenseRole::MainDoor => self.main_door.as_ref(),
            DefenseRole::LeftWall => self.left_wall.as_ref(),
            DefenseRole::CenterWall => self.center_wall.as_ref(),
            DefenseRole::RightWall => self.right_wall.as_ref(),
            DefenseRole::Archer(i) => self.archers.get(i).and_then(Option::as_ref),
            DefenseRole::Guard(i) => self.guards.get(i).and_then(Option::as_ref),
        }
    }

    /// [`UserCastle::defense_unit`] 的可写版（门/墙 `Run` 与死亡掩码在这里落状态）。
    pub fn defense_unit_mut(&mut self, role: DefenseRole) -> Option<&mut CastleUnit> {
        match role {
            DefenseRole::MainDoor => self.main_door.as_mut(),
            DefenseRole::LeftWall => self.left_wall.as_mut(),
            DefenseRole::CenterWall => self.center_wall.as_mut(),
            DefenseRole::RightWall => self.right_wall.as_mut(),
            DefenseRole::Archer(i) => self.archers.get_mut(i).and_then(Option::as_mut),
            DefenseRole::Guard(i) => self.guards.get_mut(i).and_then(Option::as_mut),
        }
    }

    /// oracle `UserCastle.Initialize()`（`src/M2Server/Castle/UserCastle.cs:108-226`）+ 其
    /// `LoadConfig()`（`UserCastle.cs:228-232`）的 `MapName` 部分——**⑧-6 第②片「实体挂图」**。
    ///
    /// 顺序逐行照搬：
    /// 1. `UserCastle.cs:113` `GetMapOfServerIndex(MapName) == M2Share.ServerIndex` 门；
    /// 2. `UserCastle.cs:115-125` 皇宫 / 密道 `FindMap` 失败告警（**不**影响后续）；
    /// 3. `UserCastle.cs:127-128` `CastleEnvir == null` ⇒ 告警「城堡所在地图不存在」并整体跳过；
    /// 4. `UserCastle.cs:130-138` 主门 ⇒ `RegenMonsterByName` → `WAbil.HP=nHP` → `Castle=this` →
    ///    `MainDoor.nStatus && Open()`；
    /// 5. `UserCastle.cs:144-173` 左/中/右墙（各写 `HP` + `Castle`）；
    /// 6. `UserCastle.cs:174-193` 弓手 `i=0..12`：`nHP <= 0` 跳过；成功 ⇒ `HP` + `Castle` + `GuardDirection=3`；
    /// 7. `UserCastle.cs:194-211` 守卫 `i=0..12`：`nHP <= 0` 跳过；成功 ⇒ **只**写 `HP`（oracle 不设 `Castle`）。
    ///
    /// 照搬的 oracle 怪行为（不是差异）：
    /// - **墙的失败告警引用上一单元的名字**（copy-paste 怪癖，`UserCastle.cs:152/162/172`）：
    ///   左墙失败报 `MainDoor.sName`、中墙失败报 `LeftWall.sName`、右墙失败报 `CenterWall.sName`；
    /// - 主门造怪失败用英文文案 `[Error] UserCastle.Initialize MainDoor.UnitObj = nil`（`UserCastle.cs:142`）。
    ///
    /// **未接线（如实登记）**：本方法只演世界态 + 产生 [`CastleEvent::SpawnDefenseUnit`]/
    /// 告警；`bins/game-svr` 启动时把 `crates/envir` 的 `SabukWConfig` 适配成
    /// [`CastleDefenseSpecs`] 并调用本方法（以及 `SaveGuildList`/`LoadGuildInfo` 同族接线）属
    /// 第②片的 bins 面，等 owner 对 bins 串行序裁决后另片交付。
    pub fn initialize(
        &mut self,
        specs: &CastleDefenseSpecs,
        env: &mut impl CastleInitializeEnv,
    ) -> Vec<CastleEvent> {
        // `Initialize()` 先 `LoadConfig()`（`UserCastle.cs:110`）——配置面（含 `Archers[12]`）在
        // 归属门之前就已就位；写盘快照要靠它保留未造怪槽的 `x/y/Name` 与 `MainDoorOpen`。
        self.specs = Some(specs.clone());
        // `LoadConfig()`：`MapName = ReadWriteString("Defense", "CastleMap", "3")`（`CastleConf.cs:39`）。
        self.map_name = Some(specs.map_name.clone());
        // `CastleConf.LoadConfig` 的其余键（`CastleConf.cs:18/40-44`）在归属门**之前**装载。
        self.own_guild = Some(specs.own_guild.clone());
        self.home_map = specs.home_map.clone();
        self.home_x = specs.home_x;
        self.home_y = specs.home_y;
        self.war_range_x = specs.war_range_x;
        self.war_range_y = specs.war_range_y;
        self.envir_map_ids = specs
            .envir_list
            .iter()
            .filter_map(|name| env.map_id(name))
            .collect();
        let mut events = Vec::new();
        let map_name = specs.map_name.as_str();

        // `UserCastle.cs:113`：`if (SystemShare.MapMgr.GetMapOfServerIndex(MapName) == M2Share.ServerIndex)`。
        if !env.map_owner_is_self(map_name) {
            return events;
        }

        // `UserCastle.cs:115-127`：`CastleEnvir`/`PalaceEnvir`/`SecretEnvir = FindMap(...)`。
        self.castle_map_id = env.map_id(map_name);
        self.palace_map_id = env.map_id(&specs.palace_map);
        self.secret_map_id = env.map_id(&specs.secret_map);

        // `UserCastle.cs:115-125`：皇宫 / 密道地图告警（找不到只告警，不阻断）。
        if !env.find_map(&specs.palace_map) {
            env.warn(format!("皇宫地图{}没找到!!!", specs.palace_map));
        }
        if !env.find_map(&specs.secret_map) {
            env.warn(format!("密道地图{}没找到!!!", specs.secret_map));
        }

        // `UserCastle.cs:127-128`：`CastleEnvir == null` ⇒ 告警并整体跳过。
        if !env.find_map(map_name) {
            env.warn(format!(
                "[错误信息] 城堡所在地图不存在(检查地图配置文件里是否有地图{map_name}的设置)"
            ));
            return events;
        }

        // `UserCastle.cs:130-138` 主门（成功后 `MainDoor.nStatus` ⇒ `Open()`）。
        self.main_door = regen_and_bind(
            env,
            map_name,
            DefenseRole::MainDoor,
            &specs.main_door,
            true,
            None,
            &mut events,
        );
        if self.main_door.is_none() {
            env.warn("[Error] UserCastle.Initialize MainDoor.UnitObj = nil".to_string());
        }

        // `UserCastle.cs:144-153` 左墙（失败告警引用**主门**名）。
        self.left_wall = regen_and_bind(
            env,
            map_name,
            DefenseRole::LeftWall,
            &specs.left_wall,
            true,
            None,
            &mut events,
        );
        if self.left_wall.is_none() {
            env.warn(format!(
                "[错误信息] 城堡初始化城门失败，检查怪物数据库里有没城门的设置: {}",
                specs.main_door.name
            ));
        }

        // `UserCastle.cs:154-163` 中墙（失败告警引用**左墙**名）。
        self.center_wall = regen_and_bind(
            env,
            map_name,
            DefenseRole::CenterWall,
            &specs.center_wall,
            true,
            None,
            &mut events,
        );
        if self.center_wall.is_none() {
            env.warn(format!(
                "[错误信息] 城堡初始化左城墙失败，检查怪物数据库里有没左城墙的设置: {}",
                specs.left_wall.name
            ));
        }

        // `UserCastle.cs:164-173` 右墙（失败告警引用**中墙**名）。
        self.right_wall = regen_and_bind(
            env,
            map_name,
            DefenseRole::RightWall,
            &specs.right_wall,
            true,
            None,
            &mut events,
        );
        if self.right_wall.is_none() {
            env.warn(format!(
                "[错误信息] 城堡初始化中城墙失败，检查怪物数据库里有没中城墙的设置: {}",
                specs.center_wall.name
            ));
        }

        // `UserCastle.cs:174-193` 弓手：`nHP <= 0` 跳过；成功 ⇒ `HP` + `Castle` + `GuardDirection=3`。
        for (i, spec) in specs.archers.iter().enumerate() {
            if i >= self.archers.len() {
                break;
            }
            if spec.hp == 0 {
                continue;
            }
            self.archers[i] = regen_and_bind(
                env,
                map_name,
                DefenseRole::Archer(i),
                spec,
                true,
                Some(3),
                &mut events,
            );
            if self.archers[i].is_none() {
                env.warn(format!(
                    "[错误信息] 城堡初始化弓箭手失败，检查怪物数据库里有没弓箭手的设置: {}",
                    spec.name
                ));
            }
        }

        // `UserCastle.cs:194-211` 守卫：`nHP <= 0` 跳过；成功 ⇒ 只写 `HP`（不设 `Castle`）。
        for (i, spec) in specs.guards.iter().enumerate() {
            if i >= self.guards.len() {
                break;
            }
            if spec.hp == 0 {
                continue;
            }
            self.guards[i] = regen_and_bind(
                env,
                map_name,
                DefenseRole::Guard(i),
                spec,
                false,
                None,
                &mut events,
            );
            if self.guards[i].is_none() {
                env.warn("[错误信息] 城堡初始化守卫失败(检查怪物数据库里有没守卫怪物)".to_string());
            }
        }

        events
    }

    /// oracle `UserCastle.Run()`（`UserCastle.cs:311-446`）。
    ///
    /// 顺序逐行照搬：`MapName`/归属服门 → 跨天重置 → 每日钟点开战 → 回收 Ghost 守卫/弓手 →
    /// 战时围墙 `StoneMode=false` + 结束预告 + 超时停战 / 非战 `StoneMode=true`。
    pub fn run(&mut self, now: CastleNow) -> Vec<CastleEvent> {
        let mut events = Vec::new();

        // `UserCastle.cs:318-321`：`string.IsNullOrEmpty(MapName)` ⇒ return。
        match &self.map_name {
            None => return events,
            Some(name) if name.is_empty() => return events,
            Some(_) => {}
        }
        // `UserCastle.cs:323-326`：非本服归属 ⇒ return。
        if self.server_index != self.map_owner_server_index {
            return events;
        }

        // `UserCastle.cs:328-339`：跨天重置 `TodayIncome=0; IncomeToday=Now; IsStartWar=false`。
        let today = now.date;
        if today != self.income_today {
            self.today_income = 0;
            self.income_today = today;
            self.is_start_war = false;
        }

        // `UserCastle.cs:340-376`：每日开战。
        if !self.is_start_war && !self.under_war && now.hour == self.config.start_castlewar_time {
            self.is_start_war = true;
            self.attack_guild_list.clear();
            // `UserCastle.cs:347-362`：**倒序**遍历申请表；命中今天者移入攻方列表（`RemoveAt(i)`）。
            let mut i = self.attack_war_list.len() as isize - 1;
            while i >= 0 {
                let idx = i as usize;
                let attacker = &self.attack_war_list[idx];
                if attacker.attack_date == today {
                    let guild = attacker.guild;
                    self.under_war = true;
                    self.show_over_msg = false;
                    self.war_date = Some(today);
                    // `UserCastle.cs:358` `StartCastleWarTick = HUtil32.GetTickCount()`。
                    self.start_castle_war_tick = now.tick_ms;
                    self.attack_guild_list.push(guild);
                    self.attack_war_list.remove(idx);
                }
                i -= 1;
            }
            if self.under_war {
                // `UserCastle.cs:365`：守方（可能为 `None`，oracle `MasterGuild` 为 null 时 `Add(null)` 是 no-op 效果）。
                if let Some(master) = self.master_guild {
                    self.attack_guild_list.push(master);
                }
                // `UserCastle.cs:366` StartWallconquestWar()。
                events.extend(self.start_wallconquest_war());
                // `UserCastle.cs:367` SaveAttackSabukWall()。
                events.push(CastleEvent::SaveAttackSabukWall);
                // `UserCastle.cs:368` SS_212。
                events.push(CastleEvent::ServerGroupMsg {
                    code: SS_212,
                    text: String::new(),
                });
                // `UserCastle.cs:369-371` 开战广播。
                let msg = format!("[{} 攻城战已经开始]", self.name);
                events.push(CastleEvent::Broadcast(msg.clone()));
                events.push(CastleEvent::ServerGroupMsg {
                    code: SS_204,
                    text: msg,
                });
                // `UserCastle.cs:373` MainDoorControl(true)（幂等关闭主门；⑧-8 起带 `CastleDoor.Close` 帧/掩码）。
                events.extend(self.main_door_control(true));
            }
        }

        // `UserCastle.cs:377-389`：回收 Ghost 的守卫 → 弓手（数组下标升序，`Guards` 在前）。
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
            // `UserCastle.cs:393-406`：战时三面墙 `StoneMode=false`。
            set_stone_mode(&mut self.left_wall, false);
            set_stone_mode(&mut self.center_wall, false);
            set_stone_mode(&mut self.right_wall, false);
            // `UserCastle.cs:408-418`：结束预告（只播一次）。
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
            // `UserCastle.cs:419-422`：超时停战（**严格大于**）。
            if (now.tick_ms - self.start_castle_war_tick) > i64::from(self.config.castle_war_time) {
                events.extend(self.stop_wallconquest_war());
            }
        } else {
            // `UserCastle.cs:424-440`：非战时三面墙 `StoneMode=true`。
            set_stone_mode(&mut self.left_wall, true);
            set_stone_mode(&mut self.center_wall, true);
            set_stone_mode(&mut self.right_wall, true);
        }

        events
    }

    /// oracle `StartWallconquestWar()`（`UserCastle.cs:564-572`）：刷皇宫 100 范围内玩家 `RefShowName`。
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

    /// oracle `StopWallconquestWar()`（`UserCastle.cs:577-597`）。
    ///
    /// **照搬 oracle 怪行为**：`UserCastle.cs:582-584` 的 `GetMapOfRangeHumanCount` **丢弃返回值**，
    /// `ListC` 恒空 ⇒ `UserCastle.cs:584-593` 的 `ChangePkStatus(false)` / 非守方 `MapRandomMove` **从不执行**。
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
    /// `boClose=false` 时只对「未开」的门开。实际动作走 [`CastleUnit::door_close`] /
    /// [`CastleUnit::door_open`]（⑧-8），因此带回 `RM_DIGDOWN`/`RM_DIGUP` 与 `SetMapXyFlag` 帧。
    pub fn main_door_control(&mut self, bo_close: bool) -> Vec<CastleEvent> {
        let Some(door) = self.main_door.as_mut() else {
            return Vec::new();
        };
        if door.ghost {
            return Vec::new();
        }
        let events = if bo_close {
            if door.opened {
                door.door_close()
            } else {
                Vec::new()
            }
        } else if door.opened {
            Vec::new()
        } else {
            door.door_open()
        };
        events.into_iter().map(CastleEvent::Structure).collect()
    }

    /// oracle `RepairDoor()`（`UserCastle.cs:920-955`）。
    ///
    /// `MainDoor.BaseObject == null || UnderWar || HP >= MaxHP` ⇒ `(false, [])`；
    /// 否则 `(GetTickCount() - StruckTick) > 60s` 才回满血（死亡时额外 `Death=false`、`IsOpened=false`）
    /// 并发 `RefStatus`。
    pub fn repair_door(&mut self, now_tick: i64) -> (bool, Vec<StructureEvent>) {
        let under_war = self.under_war;
        let Some(door) = self.main_door.as_mut() else {
            return (false, Vec::new());
        };
        if under_war || door.hp >= door.max_hp {
            return (false, Vec::new());
        }
        if now_tick - door.struck_tick <= STRUCTURE_REPAIR_DELAY_MS {
            return (false, Vec::new());
        }
        door.hp = door.max_hp;
        if door.death {
            door.death = false;
            door.opened = false;
        }
        let events = door.door_ref_status();
        (true, events)
    }

    /// oracle `RepairWall(int nWallIndex)`（`UserCastle.cs:956-997`）。
    ///
    /// `nWallIndex` 1/2/3 ⇒ 左/中/右墙；其它值 ⇒ `Wall = null` ⇒ `(false, [])`。
    /// 其余同 [`UserCastle::repair_door`]，但用 [`CastleUnit::wall_ref_status`]。
    pub fn repair_wall(&mut self, wall_index: u8, now_tick: i64) -> (bool, Vec<StructureEvent>) {
        let under_war = self.under_war;
        let slot = match wall_index {
            1 => &mut self.left_wall,
            2 => &mut self.center_wall,
            3 => &mut self.right_wall,
            _ => return (false, Vec::new()),
        };
        let Some(wall) = slot.as_mut() else {
            return (false, Vec::new());
        };
        if under_war || wall.hp >= wall.max_hp {
            return (false, Vec::new());
        }
        if now_tick - wall.struck_tick <= STRUCTURE_REPAIR_DELAY_MS {
            return (false, Vec::new());
        }
        wall.hp = wall.max_hp;
        if wall.death {
            wall.death = false;
        }
        let events = wall.wall_ref_status();
        (true, events)
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

    /// oracle `IsMember(PlayObject)`（`UserCastle.cs:480-483`）/ `IsMember(IPlayerActor)`（`UserCastle.cs:609-612`）
    /// ⇒ `IsMasterGuild(member.MyGuild)`。
    #[must_use]
    pub fn is_member(&self, guild: GuildId) -> bool {
        self.is_master_guild(guild)
    }

    /// **名字口径**的 `IsMasterGuild`（oracle `UserCastle.cs:639-642`：
    /// `MasterGuild != null && MasterGuild == guild`）。
    ///
    /// `MasterGuild` 只由 `UserCastle.LoadConfig`（`UserCastle.cs:231`）与 `GetCastle`
    /// （`UserCastle.cs:543`）赋值；`CastleMgr.Initialize` 早于 `GuildMgr.LoadGuildInfo` ⇒ **行会表装载前
    /// 恒 `None`**，此时本函数**恒 false**（照 oracle，不拿 `OwnGuild` 名字顶替城主身份）。
    /// 城池被占领（`GetCastle`）后 `master_guild` 才有值；本仓生产路径尚未接线 `GetCastle`
    /// ⇒ 生产恒 false（登记 `whitelist.md` T-131）。
    #[must_use]
    pub fn is_master_guild_name(&self, guild_name: Option<&str>) -> bool {
        if self.master_guild.is_none() {
            return false;
        }
        // `MasterGuild == guild` 是引用相等；本签名只有会名 ⇒ 仅在 `master_guild` 已挂时
        // 用 `OwnGuild` 与会名逐字比较（同一个 `GuildMgr` 解析，同名即同对象）。
        match (self.own_guild.as_deref(), guild_name) {
            (Some(own), Some(name)) => !own.is_empty() && own == name,
            _ => false,
        }
    }

    /// **名字口径**的 `IsMember(PlayObject)` = `IsMasterGuild(MyGuild)`（`UserCastle.cs:480-483`）。
    #[must_use]
    pub fn is_member_name(&self, guild_name: Option<&str>) -> bool {
        self.is_master_guild_name(guild_name)
    }

    /// oracle `GetCastle(GuildInfo)`（`UserCastle.cs:539-560`）的**状态字段**半。
    ///
    /// 只落 `MasterGuild` / `OwnGuild` / `ChangeDate` 三个字段；`SaveConfigFile()`、两次
    /// `RefMemberName()`、全服/跨服广播与 `LogService.Info` 都不在本卡（写盘与事件属 P3 宿主）。
    /// 可达性登记不变：生产路径仍无调用者（`is_master_guild_name` 生产恒 false，T-131）。
    ///
    /// `change_date` 口径：oracle 写 `DateTime.Now`（`yyyy-MM-dd HH:mm:ss`）；本模块的注入时钟
    /// [`CastleNow`] 只到「日 + 小时」⇒ 这里生成 `yyyy-MM-dd HH:00:00`，分/秒由 P3 接线时
    /// 按宿主墙钟补全（登记 `whitelist.md` T-152）。
    pub fn get_castle(&mut self, guild: GuildId, guild_name: &str, now: CastleNow) {
        self.master_guild = Some(guild);
        self.own_guild = Some(guild_name.to_string());
        self.change_date = Some(format_change_date(now));
    }

    /// oracle `CastleConf.SaveConfig`（`CastleConf.cs:95-320`）的纯数据面：把当前写盘字段整理成
    /// [`CastleSaveSnapshot`]；**不做 IO**（路径/GBK 编码/写字节是 P2 `crates/envir` 面）。
    ///
    /// 跳过语义与 oracle 同序：
    /// 1. `GetMapOfServerIndex(MapName) != ServerIndex` ⇒ [`CastleSaveSkip::MapOwnerNotSelf`]
    ///    （`CastleConf.cs:105-107`）。`GetMapOfServerIndex(null) == 0`（`MapManager.cs:169-180`）
    ///    ⇒ `map_name == None` 的装载城**不**被这道门挡住；
    /// 2. `Archers[i] == null`（未 `LoadConfig`，`specs == None`）⇒
    ///    [`CastleSaveSkip::NreArchersNull`]（`CastleConf.cs:266-269`）。
    ///
    /// 槽值口径：名称/坐标取配置面（[`CastleDefenseSpecs`]，oracle 写的是 `ArcherUnit` 的
    /// 读入值），HP 取运行期 [`CastleUnit`]——门/墙未挂实体 ⇒ `hp = None`（不写 `<X>HP` 键），
    /// 弓手/守卫未挂实体 ⇒ `hp = Some(0)`（oracle 无条件写 0）；`MainDoorOpen` 只在主门挂实体时
    /// 有值。
    pub fn save_snapshot(&self) -> Result<CastleSaveSnapshot, CastleSaveSkip> {
        if self.map_name.is_some() && self.map_owner_server_index != self.server_index {
            return Err(CastleSaveSkip::MapOwnerNotSelf);
        }
        let Some(specs) = self.specs.as_ref() else {
            return Err(CastleSaveSkip::NreArchersNull);
        };
        let mut main_door = save_unit(&specs.main_door, self.main_door.as_ref(), false);
        main_door.open = self.main_door.as_ref().map(|_| specs.main_door.status);
        let archers = specs
            .archers
            .iter()
            .enumerate()
            .take(MAX_CASTLE_ARCHER)
            .map(|(i, spec)| save_unit(spec, self.archers.get(i).and_then(Option::as_ref), true))
            .collect();
        let guards = specs
            .guards
            .iter()
            .enumerate()
            .take(MAX_CASTLE_GUARD)
            .map(|(i, spec)| save_unit(spec, self.guards.get(i).and_then(Option::as_ref), true))
            .collect();
        Ok(CastleSaveSnapshot {
            castle_name: self.name.clone(),
            own_guild: self.own_guild.clone(),
            change_date: self.change_date.clone(),
            war_date: self.war_date,
            income_today: self.income_today,
            total_gold: self.total_gold,
            today_income: self.today_income,
            envir_list: specs.envir_list.clone(),
            map_name: self.map_name.clone(),
            home_map: self.home_map.clone(),
            home_x: self.home_x,
            home_y: self.home_y,
            war_range_x: self.war_range_x,
            war_range_y: self.war_range_y,
            main_door,
            left_wall: save_unit(&specs.left_wall, self.left_wall.as_ref(), false),
            center_wall: save_unit(&specs.center_wall, self.center_wall.as_ref(), false),
            right_wall: save_unit(&specs.right_wall, self.right_wall.as_ref(), false),
            archers,
            guards,
            config_dir: self.config_dir.clone(),
            ctor_config_dir: self.ctor_config_dir.clone(),
        })
    }

    /// oracle `InCastleWarArea(IEnvirnoment envir, int nX, int nY)`（`UserCastle.cs:454-478`）。
    ///
    /// 逐行：
    /// 1. `envir == null` ⇒ false（本签名用 `Option<usize>` 表达，`None` 直接 false）；
    /// 2. `envir == CastleEnvir` 且 `|HomeX-x| < WarRangeX` 且 `|HomeY-y| < WarRangeY` ⇒ true
    ///    （**严格 `<`**：dx=100 为 false）；
    /// 3. `envir == PalaceEnvir || envir == SecretEnvir` ⇒ true；
    /// 4. `EnvirList` 任一项与 `envir.MapName` 相等（OrdinalIgnoreCase；本仓解析为 map_id 后
    ///    等价）⇒ true；
    /// 5. 否则 false。
    #[must_use]
    pub fn in_castle_war_area(&self, map_id: Option<usize>, x: i32, y: i32) -> bool {
        let Some(map_id) = map_id else {
            return false;
        };
        if self.castle_map_id == Some(map_id)
            && (i32::from(self.home_x) - x).abs() < self.war_range_x
            && (i32::from(self.home_y) - y).abs() < self.war_range_y
        {
            return true;
        }
        if self.palace_map_id == Some(map_id) || self.secret_map_id == Some(map_id) {
            return true;
        }
        self.envir_map_ids.contains(&map_id)
    }

    /// oracle `GetHomeX()`（`UserCastle.cs:649-652`）：`HomeX - 4 + Random(9)`。
    ///
    /// **共享函数**：进图（成员皇宫臂）、死亡（成员+战时臂）与回家技能三处必须走同一实现，
    /// 保证抽数点位与顺序一致。
    #[must_use]
    pub fn get_home_x(&self, rng: &mut RandomNumber) -> i16 {
        self.home_x - 4 + rng.random_below(9) as i16
    }

    /// oracle `GetHomeY()`（`UserCastle.cs:654-657`）：`HomeY - 4 + Random(9)`。
    #[must_use]
    pub fn get_home_y(&self, rng: &mut RandomNumber) -> i16 {
        self.home_y - 4 + rng.random_below(9) as i16
    }

    /// oracle `InPalaceGuildCount()`（`UserCastle.cs:604-607`）= `AttackGuildList.Count`。
    ///
    /// 登记：语义是「攻城行会数」，**不是**皇宫内玩家数（盘点 §1.E）。
    #[must_use]
    pub fn in_palace_guild_count(&self) -> usize {
        self.attack_guild_list.len()
    }

    /// oracle `CheckInPalace(int nX, int nY)`（`UserCastle.cs:680-705`）：
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

/// 写盘槽的取值（[`UserCastle::save_snapshot`] 的口径）：名称/坐标取配置，HP 取运行期。
///
/// `hp_always=true` = archer/guard（oracle 无条件写，未挂实体写 0）；`false` = 门/墙
/// （`BaseObject == null` 时不写 HP 键）。
fn save_unit(
    spec: &DefenseUnitSpec,
    bound: Option<&CastleUnit>,
    hp_always: bool,
) -> CastleSaveUnit {
    CastleSaveUnit {
        name: spec.name.clone(),
        x: spec.x,
        y: spec.y,
        hp: if hp_always {
            Some(bound.map_or(0, |u| u.hp))
        } else {
            bound.map(|u| u.hp)
        },
        open: None,
    }
}

/// `ChangeDate` 的串口径（`ConfigFile.WriteDateTime` = `yyyy-MM-dd HH:mm:ss`）。
///
/// [`CastleNow`] 只到小时 ⇒ 分/秒写 0；P3 接线时由宿主按真墙钟补全（登记 T-152）。
fn format_change_date(now: CastleNow) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:00:00",
        now.date.year, now.date.month, now.date.day, now.hour
    )
}

// ---------------------------------------------------------------------------
// 进图 / 死亡落点改写（`WorldServer.cs:298-350`）
// ---------------------------------------------------------------------------

/// 一次落点改写的产物（地图逻辑名 + 短坐标）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relocation {
    /// 新 `MapName`（oracle `playObject.MapName`）。
    pub map_name: String,
    /// 新 `CurrX`。
    pub x: i16,
    /// 新 `CurrY`。
    pub y: i16,
}

/// `Server.conf` 的 `RedDieHomeMap/RedDieHomeX/RedDieHomeY`（`WorldServer.cs:343-348`）。
///
/// 缺键缺省 = `GameSvrConf.cs:1380-1382` 的 `"3"/839/668`；部署件实测 `3/838/668`
/// （`E:/MirServer/M2GameSvr/Server.conf:413-415`）⇒ 生产由宿主读配置传入。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedDieHome {
    pub map_name: String,
    pub x: i16,
    pub y: i16,
}

impl Default for RedDieHome {
    fn default() -> Self {
        RedDieHome {
            map_name: "3".to_string(),
            x: 839,
            y: 668,
        }
    }
}

/// 进图落点改写的输入（`WorldServer.cs:299-318`）。
#[derive(Clone, Copy, Debug)]
pub struct EnterRelocateInput<'a> {
    /// `CastleMgr.InCastleWarArea(envir, CurrX, CurrY)` 的首个命中城（`None` = 不在攻城区）。
    pub castle: Option<&'a UserCastle>,
    /// 解析后的 `map_id`（对应 `envir`）。
    pub map_id: usize,
    /// 当前 `CurrX/CurrY`。
    pub x: i32,
    pub y: i32,
    /// `MyGuild?.GuildName`（`None` ⇔ `MyGuild == null`）。
    pub my_guild_name: Option<&'a str>,
    /// 角色记录 `HomeMap/HomeX/HomeY`（**不是**城堡的 `HomeMap`）。
    pub home_map: &'a str,
    pub home_x: i16,
    pub home_y: i16,
}

/// oracle `WorldServer.cs:298-318` 的进图改写两臂。
///
/// - 外层：`envir != null && castle != null && (castle.PalaceEnvir == envir || castle.UnderWar)`；
/// - 非成员（`CastleMgr.IsCastleMember == null`）⇒ 本人 `HomeMap` + `HomeX-2+Random(5)`、
///   `HomeY-2+Random(5)`（**X 先 Y 后**）；
/// - 成员且 `PalaceEnvir == envir` ⇒ `castle.MapName` + `GetHomeX()`/`GetHomeY()`
///   （各 `Random(9)`，X 先 Y 后）；
/// - 成员但不在皇宫 ⇒ **不改写、0 抽**。
///
/// `None` = 不改写（调用方继续用原坐标）；`Some` 时已按顺序抽完数。
pub fn relocate_on_enter(
    input: &EnterRelocateInput<'_>,
    rng: &mut RandomNumber,
) -> Option<Relocation> {
    let castle = input.castle?;
    if !(castle.palace_map_id == Some(input.map_id) || castle.under_war) {
        return None;
    }
    if !castle.is_master_guild_name(input.my_guild_name) {
        // `WorldServer.cs:305-307`：先 X 后 Y，各一次 `Random(5)`。
        let x = input.home_x - 2 + rng.random_below(5) as i16;
        let y = input.home_y - 2 + rng.random_below(5) as i16;
        return Some(Relocation {
            map_name: input.home_map.to_string(),
            x,
            y,
        });
    }
    if castle.palace_map_id == Some(input.map_id) {
        // `WorldServer.cs:313-315`：`GetMapName()` + `GetHomeX()/GetHomeY()`（各 `Random(9)`）。
        let x = castle.get_home_x(rng);
        let y = castle.get_home_y(rng);
        return Some(Relocation {
            map_name: castle.map_name.clone().unwrap_or_default(),
            x,
            y,
        });
    }
    None
}

/// 死亡（`HP <= 0`）落点改写的输入（`WorldServer.cs:324-350`）。
#[derive(Clone, Copy, Debug)]
pub struct DeathRelocateInput<'a> {
    /// `CastleMgr.IsCastleMember(playObject)` 的首个命中城（与进图改写**独立**重算）。
    pub member_castle: Option<&'a UserCastle>,
    /// `playObject.Abil.HP`（调用方已按 `Fight3Zone` 恢复支处理过）。
    pub hp: i32,
    /// `PvpLevel()` = `PkPoint / 100`。
    pub pvp_level: u8,
    /// 角色记录 `HomeMap/HomeX/HomeY`。
    pub home_map: &'a str,
    pub home_x: i16,
    pub home_y: i16,
    /// `Config.RedDieHome*`。
    pub red_die: &'a RedDieHome,
}

/// oracle `WorldServer.cs:324-350` 的死亡三臂：
///
/// 1. `PvpLevel < 2` 且成员城 `UnderWar` ⇒ `castle.HomeMap` + `GetHomeX()/GetHomeY()`
///    （各 `Random(9)`，X 先 Y 后）；
/// 2. `PvpLevel < 2` 其余 ⇒ 本人 `HomeMap` + `HomeX-2+Random(5)` ×2（X 先 Y 后）；
/// 3. `PvpLevel >= 2` ⇒ `RedDieHomeMap` + `RedDieHomeX+Random(13)` / `+Random(13)`。
///
/// `hp > 0` ⇒ `None`（0 抽）。
pub fn relocate_on_death(
    input: &DeathRelocateInput<'_>,
    rng: &mut RandomNumber,
) -> Option<Relocation> {
    if input.hp > 0 {
        return None;
    }
    if input.pvp_level < 2 {
        if let Some(castle) = input.member_castle.filter(|c| c.under_war) {
            let x = castle.get_home_x(rng);
            let y = castle.get_home_y(rng);
            return Some(Relocation {
                map_name: castle.home_map.clone(),
                x,
                y,
            });
        }
        let x = input.home_x - 2 + rng.random_below(5) as i16;
        let y = input.home_y - 2 + rng.random_below(5) as i16;
        return Some(Relocation {
            map_name: input.home_map.to_string(),
            x,
            y,
        });
    }
    // `WorldServer.cs:345-347`：`Random(13) + RedDieHomeX`（**先 X 后 Y**）。
    let x = (i32::from(input.red_die.x) + rng.random_below(13)) as i16;
    let y = (i32::from(input.red_die.y) + rng.random_below(13)) as i16;
    Some(Relocation {
        map_name: input.red_die.map_name.clone(),
        x,
        y,
    })
}

/// `RefUserState` 的位累加器：只在 `cond` 为真时置位。
///
/// 用 `|=`（不是 `^=`/`=`）：同一 bit **重复置位幂等**，不同 bit 互不覆盖
/// （`PlayObject.Base.cs:270-286` 的逐位 `if` 语义）。
pub fn set_area_flag(n8: &mut u16, bit: u16, cond: bool) {
    if cond {
        *n8 |= bit;
    }
}

/// oracle `RefUserState()` 的 `n8`（`PlayObject.Base.cs:270-286`）：
/// `FightZone|1`、`SafeArea|2`、`InGuildWarArea|4`。
#[must_use]
pub fn area_state(fight_zone: bool, safe_area: bool, in_guild_war_area: bool) -> u16 {
    let mut n8 = 0u16;
    set_area_flag(&mut n8, AREA_STATE_FIGHT_ZONE, fight_zone);
    set_area_flag(&mut n8, AREA_STATE_SAFE_AREA, safe_area);
    set_area_flag(&mut n8, AREA_STATE_GUILD_WAR, in_guild_war_area);
    n8
}

// ---------------------------------------------------------------------------
// 名字色（`PlayObject.GetChrColor`，`PlayObject.cs:3585-3657`）
// ---------------------------------------------------------------------------

/// `GetChrColor` 的输入（观察者 + 目标 + 城堡态的只读快照）。
#[derive(Clone, Copy, Debug)]
pub struct ChrColorInput<'a> {
    /// `CastleMgr.InCastleWarArea(targetObject)` 的首个命中城。
    pub castle: Option<&'a UserCastle>,
    /// `target.NameColor`（玩家 ctor = 255）。
    pub target_name_color: u8,
    /// `target.PvpLevel()`。
    pub target_pvp_level: u8,
    /// `target.PvpFlag`。
    pub target_pvp_flag: bool,
    /// `target.Envir.Flag.Fight3Zone`。
    pub target_fight3_zone: bool,
    /// 观察者 `MyGuild?.GuildName`（`None` ⇔ `MyGuild == null`）。
    pub my_guild_name: Option<&'a str>,
    /// 目标 `MyGuild?.GuildName`。
    pub target_guild_name: Option<&'a str>,
    /// `GetGuildRelation(this, target)`：0 无 / 1 同会 / 2 宣战 / 3 盟友。
    pub guild_relation: u8,
    /// `MyGuild.IsAllyGuild(targetObject.MyGuild)`（城主分支用）。
    pub viewer_is_ally_of_target: bool,
    /// 观察者 `InGuildWarArea`。
    pub viewer_in_guild_war_area: bool,
    /// 目标 `InGuildWarArea`。
    pub target_in_guild_war_area: bool,
}

/// `GetChrColor` 的 `NotImplementedException`（`UserCastle.cs:1045-1048`）。
///
/// oracle 里 `IsAttackGuild(IGuild)` 选中接口重载 ⇒ 抛异常 ⇒ 被 `PlayObject.Run` 的消息段
/// `catch` 吞掉 ⇒ **本 tick 剩余消息全部不再出队**（不只是名字帧）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChrColorError {
    /// `IUserCastle.IsAttackGuild(IGuild)`（`UserCastle.cs:1045-1048`）。
    AttackGuildNotImplemented,
}

/// `GetChrColor` 的结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChrColorResult {
    /// 名字色字节。
    pub color: u8,
    /// oracle 的 `GuildWarArea = true` 副作用（影响同 tick 攻击判定）。
    pub guild_war_area: bool,
}

/// `PlayObject.GetChrColor(IActor)`（`PlayObject.cs:3585-3657`）的 1:1。
///
/// 顺序：`PvpLevel < 2` 的 PK/行会/Fight3Zone 三段 ⇒ 城堡段。城堡段的怪行为
/// **照抄不修**：有行会观察者（非城主或目标非同会/盟友）走 `IsAttackGuild(IGuild)`
/// ⇒ [`ChrColorError::AttackGuildNotImplemented`]；无行会观察者返回 221。
///
/// # Errors
/// 见 [`ChrColorError`]。
pub fn get_chr_color(
    input: &ChrColorInput<'_>,
    colors: &CastleNameColors,
) -> Result<ChrColorResult, ChrColorError> {
    let mut result = input.target_name_color;
    let mut guild_war_area = false;
    if input.target_pvp_level < 2 {
        if input.target_pvp_flag {
            result = colors.pk_flag;
        }
        match input.guild_relation {
            1 | 3 => result = colors.ally_guild,
            2 => result = colors.war_guild,
            _ => {}
        }
        if input.target_fight3_zone {
            result = if input.my_guild_name == input.target_guild_name {
                colors.ally_guild
            } else {
                colors.war_guild
            };
        }
    }
    if let Some(castle) = input.castle {
        if castle.under_war && input.viewer_in_guild_war_area && input.target_in_guild_war_area {
            result = colors.in_free_pk;
            guild_war_area = true;
            let Some(my_guild) = input.my_guild_name else {
                return Ok(ChrColorResult {
                    color: result,
                    guild_war_area,
                });
            };
            if castle.is_master_guild_name(Some(my_guild)) {
                if input.my_guild_name == input.target_guild_name || input.viewer_is_ally_of_target
                {
                    result = colors.ally_guild;
                } else {
                    // `castle.IsAttackGuild(targetObject.MyGuild)` ⇒ N1。
                    return Err(ChrColorError::AttackGuildNotImplemented);
                }
            } else {
                // `castle.IsAttackGuild(MyGuild)` ⇒ N1（任何非城主有会观察者）。
                return Err(ChrColorError::AttackGuildNotImplemented);
            }
        }
    }
    Ok(ChrColorResult {
        color: result,
        guild_war_area,
    })
}

// ---------------------------------------------------------------------------
// 每 tick 的 766 `InGuildWarArea` 置位/清除（`PlayObject.Message.cs:286-350`）
// ---------------------------------------------------------------------------

/// 一个在线玩家的每 tick 世界快照（宿主从 `Entity` + `SessionGround` 装配）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastleWarPlayer {
    /// `BaseObject.ActorId`。
    pub actor_id: i32,
    /// `BaseObject.Envir` 的 `map_id`。
    pub map_id: usize,
    /// `CurrX/CurrY`。
    pub x: i32,
    pub y: i32,
    /// `MyGuild?.GuildName`（皇宫自动占领链的 N1 门）。
    pub guild_name: Option<String>,
}

/// `PlayObject` 的城堡会话位（`InGuildWarArea` / `NameColorChanged` / `DiscountForNightTick`）。
///
/// ctor：`InGuildWarArea=false`（`PlayObject.cs:905`）、`NameColorChanged=false`、
/// `DiscountForNightTick=GetTickCount()`（`PlayObject.cs:914`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastleWarState {
    /// `PlayObject.InGuildWarArea`。
    pub in_guild_war_area: bool,
    /// `PlayObject.NameColorChanged`。
    pub name_color_changed: bool,
    /// `PlayObject.DiscountForNightTick`（1s 门基准）。
    pub discount_for_night_tick: i64,
}

impl CastleWarState {
    /// ctor 口径：门基准 = 当前 tick。
    #[must_use]
    pub const fn new(now_ms: i64) -> Self {
        CastleWarState {
            in_guild_war_area: false,
            name_color_changed: false,
            discount_for_night_tick: now_ms,
        }
    }

    /// `PlayObject.ChangePkStatus(boWarFlag)`（`PlayObject.cs:4343-4350`）：
    /// 只在值不同时写位并置 `NameColorChanged`。
    pub fn change_pk_status(&mut self, bo_war_flag: bool) {
        if self.in_guild_war_area != bo_war_flag {
            self.in_guild_war_area = bo_war_flag;
            self.name_color_changed = true;
        }
    }
}

/// 每 tick 推给宿主的 766/名牌重发事件。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastleWarEvent {
    /// `RefUserState()` ⇒ `SendDefMessage(SM_AREASTATE, n8, 0, 0, 0)`（发给本人）。
    AreaState {
        actor_id: i32,
        in_guild_war_area: bool,
    },
    /// `RefShowName()` ⇒ `SendRefMsg(RM_USERNAME, ..., GetShowName())`（发给同屏观察者）。
    ShowName { actor_id: i32 },
}

/// oracle `PlayObject.Message.cs:286-350` 的每 tick 城堡段（1:1）。
///
/// - **每 tick**：`castle != null && castle.UnderWar` ⇒ `ChangePkStatus(true)`；
/// - **1s 门内**（`now - DiscountForNightTick > 1000`，**先**把基准写成 `now`）：
///   - 战时且皇宫且 `MyGuild != null` **且 `!castle.IsMember(this)`** ⇒ 自动占领链第一跳
///     `IsAttackGuild(IGuild)` 抛 N1 ⇒ 本 tick 后续（含 `NameColorChanged` 块）全部跳过，
///     `NameColorChanged` 保持真；城主行会成员站皇宫 ⇒ 不抛，照常清标记并重发两条；
///   - 否则若不在战时 ⇒ `ChangePkStatus(false)`；
///   - `NameColorChanged` ⇒ 清标记 + `RefUserState` + `RefShowName`（两条事件）。
///
/// 事件顺序与 oracle 的发送序一致。`states` 缺项按 ctor 口径 [`CastleWarState::new`] 建立。
pub fn castle_war_tick(
    castles: &[UserCastle],
    players: &[CastleWarPlayer],
    states: &mut std::collections::HashMap<i32, CastleWarState>,
    now_ms: i64,
) -> Vec<CastleWarEvent> {
    let mut events = Vec::new();
    for p in players {
        let st = states
            .entry(p.actor_id)
            .or_insert_with(|| CastleWarState::new(now_ms));
        let castle = castles
            .iter()
            .find(|c| c.in_castle_war_area(Some(p.map_id), p.x, p.y));
        let in_war = castle.is_some_and(|c| c.under_war);
        // `PlayObject.Message.cs:286-290`：每 tick 置位。
        if in_war {
            st.change_pk_status(true);
        }
        // `PlayObject.Message.cs:291-350`：1s 门（严格 `>`），先写基准。
        if now_ms - st.discount_for_night_tick > 1000 {
            st.discount_for_night_tick = now_ms;
            // `PlayObject.Message.cs:319-338`：皇宫 `MyGuild != null` **且
            // `!castle.IsMember(this)`**（`UserCastle.cs:480-483`）⇒ 自动占领链在
            // `castle.IsAttackGuild(IGuild)` 抛 N1，本 tick 剩余块不再执行。
            // 城主行会成员（`IsMember == true`）不进该链 ⇒ 继续 `NameColorChanged` 块。
            let n1_throw = castle.is_some_and(|c| {
                c.under_war
                    && c.palace_map_id == Some(p.map_id)
                    && p.guild_name.is_some()
                    && !c.is_member_name(p.guild_name.as_deref())
            });
            if !(castle.is_some_and(|c| c.under_war)) {
                st.change_pk_status(false);
            }
            if !n1_throw && st.name_color_changed {
                st.name_color_changed = false;
                events.push(CastleWarEvent::AreaState {
                    actor_id: p.actor_id,
                    in_guild_war_area: st.in_guild_war_area,
                });
                events.push(CastleWarEvent::ShowName {
                    actor_id: p.actor_id,
                });
            }
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{CellAttribute, MapGrid};

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
            hp: 10_000,
            max_hp: 10_000,
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
        // 事件顺序 = oracle UserCastle.cs:366-373（⑧-8 起 `MainDoorControl(true)` 带 `CastleDoor.Close` 帧/掩码）。
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
                // `CastleDoor.Close()`：`Dir = 3 - Round(10000/10000*3) = 0`。
                CastleEvent::Structure(StructureEvent::RefDigDown {
                    dir: 0,
                    x: 672,
                    y: 330
                }),
                // `SetMapXyFlag(1)`：`nFlag != 1` 为 false ⇒ 三格（`CastleDoor.cs:25-27`）先 true、
                // 九格（`CastleDoor.cs:29-37`）全 false（9 个不重复格）。
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 672,
                    y: 328,
                    walk: true
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 673,
                    y: 329,
                    walk: true
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 673,
                    y: 328,
                    walk: true
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 672,
                    y: 330,
                    walk: false
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 672,
                    y: 329,
                    walk: false
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 672,
                    y: 328,
                    walk: false
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 673,
                    y: 329,
                    walk: false
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 673,
                    y: 328,
                    walk: false
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 671,
                    y: 330,
                    walk: false
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 670,
                    y: 330,
                    walk: false
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 671,
                    y: 329,
                    walk: false
                }),
                CastleEvent::Structure(StructureEvent::SetMapXyFlag {
                    x: 671,
                    y: 331,
                    walk: false
                }),
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
        // IsStartWar 仍置 true（oracle UserCastle.cs:345，进了钟点分支就置位）。
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
        // （`UserCastle.cs:391-406` 先置 false，`UserCastle.cs:421` 才停战，本 tick 不再走 `else`）。
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
            x: 672,
            y: 330,
            hp: 10_000,
            max_hp: 10_000,
            ..CastleUnit::default()
        });
        // ⑧-8：`Close()` 带 1 个 `RM_DIGDOWN` + 12 格掩码（`nFlag=1`）。
        let ev = c.main_door_control(true);
        assert!(!c.main_door.unwrap().opened);
        assert_eq!(ev.len(), 13);
        assert_eq!(
            ev[0],
            CastleEvent::Structure(StructureEvent::RefDigDown {
                dir: 0,
                x: 672,
                y: 330
            })
        );
        c.main_door_control(true); // 再关不动
        assert!(!c.main_door.unwrap().opened);
        assert!(c.main_door_control(true).is_empty());
        // ⑧-8：`Open()` 带 1 个 `RM_DIGUP` + 15 格掩码（`nFlag=0`，末尾三格回 false）。
        let ev = c.main_door_control(false);
        assert!(c.main_door.unwrap().opened);
        assert_eq!(ev.len(), 1 + 15);
        assert_eq!(
            ev[0],
            CastleEvent::Structure(StructureEvent::RefDigUp {
                dir: 7,
                x: 672,
                y: 330
            })
        );
        c.main_door_control(false); // 再开不动
        assert!(c.main_door.unwrap().opened);
        assert!(c.main_door_control(false).is_empty());

        // Ghost 门：不动。
        c.main_door = Some(CastleUnit {
            opened: false,
            ghost: true,
            ..CastleUnit::default()
        });
        assert!(c.main_door_control(false).is_empty());
        assert!(!c.main_door.unwrap().opened);

        // 无门：不 panic。
        c.main_door = None;
        assert!(c.main_door_control(true).is_empty());
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

    // ------------------------------------------------------------------
    // ⑧-8：门/墙状态机、通行掩码、修理 60s 门、CastleOfficial 标签（卡 openmir2-m3-castle-door-repair）
    // ------------------------------------------------------------------

    fn door(hp: i32, max_hp: i32) -> CastleUnit {
        CastleUnit {
            x: 672,
            y: 330,
            hp,
            max_hp,
            ..CastleUnit::default()
        }
    }

    fn wall(hp: i32, max_hp: i32) -> CastleUnit {
        CastleUnit {
            x: 624,
            y: 278,
            hp,
            max_hp,
            ..CastleUnit::default()
        }
    }

    #[test]
    fn door_open_close_sets_state_and_frames_like_oracle() {
        let mut d = door(10_000, 10_000);
        // `CastleDoor.cs:46-58`。
        let ev = d.door_open();
        assert_eq!(d.dir, 7);
        assert!(d.opened);
        assert!(d.stone_mode);
        assert!(!d.hold_place);
        assert_eq!(
            ev[0],
            StructureEvent::RefDigUp {
                dir: 7,
                x: 672,
                y: 330
            }
        );
        assert_eq!(ev.len(), 1 + 15, "`SetMapXyFlag(0)` = 3 true + 9 + 3 false");

        // `CastleDoor.cs:60-76`：`Dir = 3 - Round(10000/10000*3) = 0`。
        let ev = d.door_close();
        assert_eq!(d.dir, 0);
        assert!(!d.opened);
        assert!(!d.stone_mode);
        assert!(d.hold_place);
        assert_eq!(
            ev[0],
            StructureEvent::RefDigDown {
                dir: 0,
                x: 672,
                y: 330
            }
        );
        assert_eq!(ev.len(), 1 + 12, "`SetMapXyFlag(1)` = 3 true + 9 false");
    }

    #[test]
    fn door_close_dir_tracks_hp_ratio_and_clamps_like_oracle() {
        // `3 - Round(hp/max*3)`，`Dir-3>=0 ⇒ 0`（`CastleDoor.cs:66-70`）。
        for (hp, want) in [
            (10_000, 0),
            (7_500, 1),
            (5_000, 1),
            (2_500, 2),
            (100, 0),
            (0, 0),
        ] {
            let mut d = door(hp, 10_000);
            d.door_close();
            assert_eq!(d.dir, want, "hp={hp}");
        }
    }

    #[test]
    fn door_open_close_and_die_return_early_when_dead() {
        let mut d = door(5_000, 10_000);
        d.death = true;
        let before = d;
        assert!(d.door_open().is_empty());
        assert_eq!(d, before);
        assert!(d.door_close().is_empty());
        assert_eq!(d, before);
        // `Die()` 本身仍 `SetMapXyFlag(2)`（`CastleDoor.cs:78-82`）。
        assert_eq!(d.door_die().len(), 12);
        assert!(d.death);
    }

    #[test]
    fn door_die_sets_death_then_mask2() {
        let mut d = door(0, 10_000);
        let ev = d.door_die();
        assert!(d.death);
        assert_eq!(ev, d.door_map_flag_events(2));
        assert_eq!(ev.len(), 12);
        assert!(ev
            .iter()
            .all(|e| matches!(e, StructureEvent::SetMapXyFlag { walk: true, .. })));
    }

    #[test]
    fn door_run_renews_death_tick_only_when_owned_by_castle() {
        // `CastleDoor.cs:86-93`：`Death && Castle != null` ⇒ `DeathTick`；否则 `HealthTick=0`。
        let mut d = door(0, 10_000);
        d.death = true;
        d.death_tick = 5;
        d.health_tick = 9;
        assert!(d.door_run(1_000, true).is_empty());
        assert_eq!(d.death_tick, 1_000);
        assert_eq!(d.health_tick, 9);

        assert!(d.door_run(2_000, false).is_empty());
        assert_eq!(d.death_tick, 1_000, "无城堡不刷 DeathTick");
        assert_eq!(d.health_tick, 0, "走 else 分支清 HealthTick");
    }

    #[test]
    fn door_run_turns_dir_only_when_closed_and_n08_below_3() {
        // `CastleDoor.cs:94-102`：未开门且 `n08 < 3` 才刷 `RM_TURN`。
        let mut d = door(2_500, 10_000);
        let ev = d.door_run(0, true);
        assert_eq!(d.dir, 2);
        assert_eq!(
            ev,
            vec![StructureEvent::RefTurn {
                dir: 2,
                x: 672,
                y: 330
            }]
        );

        // 已开门 ⇒ 不刷 Dir。
        let mut opened = door(2_500, 10_000);
        opened.opened = true;
        assert!(opened.door_run(0, true).is_empty());
        assert_eq!(opened.dir, 0);

        // `n08 >= 3`（HP=0 ⇒ 3）⇒ 不刷。
        let mut zero = door(0, 10_000);
        zero.dir = 5;
        assert!(zero.door_run(0, true).is_empty());
        assert_eq!(zero.dir, 5);
    }

    #[test]
    fn door_and_wall_ref_status_reset_dir_and_frame() {
        // `CastleDoor.cs:106-115`。
        let mut d = door(0, 10_000);
        d.dir = 9;
        assert_eq!(
            d.door_ref_status(),
            vec![StructureEvent::RefAlive {
                dir: 0,
                x: 672,
                y: 330
            }]
        );
        assert_eq!(d.dir, 0, "n08=3 ⇒ n08-3>=0 ⇒ 0");
        let mut d2 = door(2_500, 10_000);
        d2.door_ref_status();
        assert_eq!(d2.dir, 2);

        // `WallStructure.cs:23-40`：`HP>0 ⇒ 3-Round(...)`，否则 4。
        let mut w = wall(10_000, 10_000);
        w.dir = 3;
        w.wall_initialize();
        assert_eq!(w.dir, 0);
        assert_eq!(
            w.wall_ref_status(),
            vec![StructureEvent::RefAlive {
                dir: 0,
                x: 624,
                y: 278
            }]
        );
        let mut dead = wall(0, 10_000);
        assert_eq!(
            dead.wall_ref_status(),
            vec![StructureEvent::RefAlive {
                dir: 4,
                x: 624,
                y: 278
            }]
        );
        assert_eq!(dead.dir, 4);
    }

    #[test]
    fn wall_run_blocks_own_cell_and_restores_it_on_death() {
        // `WallStructure.cs:47-82`。
        let mut w = wall(10_000, 10_000);
        let ev = w.wall_run(500);
        assert_eq!(w.health_tick, 0);
        assert!(w.set_map_flaged);
        assert_eq!(
            ev,
            vec![StructureEvent::SetMapXyFlag {
                x: 624,
                y: 278,
                walk: false
            }]
        );
        // 第二 tick：已标记 ⇒ 不再重复置位。
        assert!(w.wall_run(600).is_empty());

        w.death = true;
        let ev = w.wall_run(700);
        assert_eq!(w.death_tick, 700);
        assert!(!w.set_map_flaged);
        assert_eq!(
            ev,
            vec![StructureEvent::SetMapXyFlag {
                x: 624,
                y: 278,
                walk: true
            }]
        );

        // `n08 < 5` 时发 `RM_DIGUP`（`n08=2`）。
        let mut hurt = wall(2_500, 10_000);
        let ev = hurt.wall_run(0);
        assert_eq!(
            ev,
            vec![
                StructureEvent::SetMapXyFlag {
                    x: 624,
                    y: 278,
                    walk: false
                },
                StructureEvent::RefDigUp {
                    dir: 2,
                    x: 624,
                    y: 278
                },
            ]
        );
    }

    #[test]
    fn door_mask_blocks_cells_and_low_wall_cannot_be_restored_like_oracle() {
        // `Envir.SetMapXyFlag` 只在格子当前 `Valid` 时才改属性（`Envirnoment.cs:1362-1369`）。
        let mut grid = MapGrid::new(700, 400);
        let mut d = door(10_000, 10_000);

        // 开门：只把那 3 个「门板」格置 `LowWall`，其余保持 `Walk`。
        apply_structure_events(&mut grid, &d.door_open());
        assert_eq!(grid.cell_attribute(672, 328), Some(CellAttribute::LowWall));
        assert_eq!(grid.cell_attribute(673, 329), Some(CellAttribute::LowWall));
        assert_eq!(grid.cell_attribute(673, 328), Some(CellAttribute::LowWall));
        assert_eq!(grid.cell_attribute(672, 330), Some(CellAttribute::Walk));
        assert_eq!(grid.cell_attribute(671, 331), Some(CellAttribute::Walk));

        // 关门：9 个不重复格全 `LowWall`（`CastleDoor.cs:29-37`）。
        apply_structure_events(&mut grid, &d.door_close());
        assert_eq!(grid.cell_attribute(672, 330), Some(CellAttribute::LowWall));
        assert_eq!(grid.cell_attribute(671, 329), Some(CellAttribute::LowWall));
        assert_eq!(grid.cell_attribute(670, 330), Some(CellAttribute::LowWall));

        // **怪行为照搬**：再开门时 `GetCellInfo` 对 `LowWall` 返 `success=false`
        // ⇒ `boFlag=true` 无法恢复 `Walk` ⇒ 门开了但格子仍阻断。
        apply_structure_events(&mut grid, &d.door_open());
        assert_eq!(grid.cell_attribute(672, 330), Some(CellAttribute::LowWall));
        assert_eq!(grid.cell_attribute(671, 329), Some(CellAttribute::LowWall));
        assert_eq!(grid.cell_attribute(670, 330), Some(CellAttribute::LowWall));

        // 越界格：`SetMapXyFlag` 静默不改（`MapGrid::set_map_xy_flag` 返 false）。
        assert!(!grid.set_map_xy_flag(-1, 0, false));
        assert_eq!(grid.cell_attribute(-1, 0), None);
    }

    #[test]
    fn door_map_xy_flag_ops_match_oracle_call_sequence() {
        // `CastleDoor.cs:25-43` 的调用序（含重复格）。
        assert_eq!(door_map_xy_flag_ops(2).len(), 12);
        assert_eq!(door_map_xy_flag_ops(1).len(), 12);
        assert_eq!(door_map_xy_flag_ops(0).len(), 15);
        // `nFlag != 1` ⇒ 0/2 的九格为 true，1 的九格为 false。
        assert!(door_map_xy_flag_ops(2)[3..].iter().all(|op| op.2));
        assert!(door_map_xy_flag_ops(1)[3..].iter().all(|op| !op.2));
        // `nFlag=0` 末尾三格回 false。
        let open = door_map_xy_flag_ops(0);
        assert_eq!(
            &open[12..],
            &[(0, -2, false), (1, -1, false), (1, -2, false)]
        );
        assert_eq!(&open[..3], &[(0, -2, true), (1, -1, true), (1, -2, true)]);
    }

    #[test]
    fn repair_door_gates_on_underwar_full_hp_and_60s_tick() {
        let mut c = ready_castle();
        {
            let d = c.main_door.as_mut().unwrap();
            d.hp = 5_000;
            d.struck_tick = 1_000;
        }
        // `UnderWar` ⇒ 拒（`UserCastle.cs:920-927`：签名 920、`UnderWar` 守卫 924、返回 926）。
        c.under_war = true;
        assert_eq!(c.repair_door(999_999), (false, Vec::new()));
        c.under_war = false;
        // 60s 门：`>` 严格大于（`UserCastle.cs:930` 活门 / `UserCastle.cs:939` 死门）。
        assert_eq!(c.repair_door(1_000 + 60_000), (false, Vec::new()));
        let (ok, ev) = c.repair_door(1_000 + 60_001);
        assert!(ok);
        assert_eq!(c.main_door.unwrap().hp, c.main_door.unwrap().max_hp);
        assert_eq!(
            ev,
            vec![StructureEvent::RefAlive {
                dir: 0,
                x: 672,
                y: 330
            }]
        );
        // 已满 ⇒ 拒（`UserCastle.cs:924` 的 `HP >= MaxHP` 守卫），不产生帧。
        assert_eq!(c.repair_door(999_999), (false, Vec::new()));
    }

    #[test]
    fn repair_door_dead_branch_clears_death_and_opened() {
        let mut c = ready_castle();
        {
            let d = c.main_door.as_mut().unwrap();
            d.death = true;
            d.hp = 0;
            d.opened = true;
            d.struck_tick = 0;
        }
        let (ok, _) = c.repair_door(60_001);
        assert!(ok);
        let d = c.main_door.unwrap();
        assert!(!d.death);
        assert!(!d.opened);
        assert_eq!(d.hp, d.max_hp);
        assert_eq!(d.dir, 0);
    }

    #[test]
    fn repair_door_rejects_missing_door() {
        let mut c = ready_castle();
        c.main_door = None;
        assert_eq!(c.repair_door(999_999), (false, Vec::new()));
    }

    #[test]
    fn repair_wall_maps_indices_one_to_three_and_rejects_others() {
        let mut c = ready_castle();
        for slot in [&mut c.left_wall, &mut c.center_wall, &mut c.right_wall] {
            let w = slot.as_mut().unwrap();
            w.hp = 1_000;
            w.max_hp = 2_000;
            w.struck_tick = 0;
        }
        // `nWallIndex` 1/2/3 ⇒ 左/中/右；其它 ⇒ `Wall = null`（`UserCastle.cs:958-992`）。
        assert_eq!(c.repair_wall(0, 60_001), (false, Vec::new()));
        assert_eq!(c.repair_wall(4, 60_001), (false, Vec::new()));
        assert!(c.repair_wall(1, 60_001).0);
        assert!(c.repair_wall(2, 60_001).0);
        assert!(c.repair_wall(3, 60_001).0);
        assert_eq!(c.left_wall.unwrap().hp, 2_000);
        assert_eq!(c.center_wall.unwrap().hp, 2_000);
        assert_eq!(c.right_wall.unwrap().hp, 2_000);
        // 已满 ⇒ 后续再修拒。
        assert_eq!(c.repair_wall(1, 60_001), (false, Vec::new()));
    }

    #[test]
    fn official_repair_door_checks_gold_then_repairs() {
        let mut c = ready_castle();
        c.total_gold = 1_000;
        {
            let d = c.main_door.as_mut().unwrap();
            d.hp = 5_000;
            d.struck_tick = 0;
        }
        // `TotalGold < RepairDoorPrice` ⇒ 只回文案（`CastleOfficial.cs:365-380`）。
        let r = official_repair_door(&mut c, 2_000_000, 60_001);
        assert!(!r.repaired);
        assert_eq!(r.gold_spent, 0);
        assert_eq!(r.message, OFFICIAL_REPAIR_NOT_ENOUGH_GOLD_MSG);
        assert_eq!(c.total_gold, 1_000);
        assert!(r.events.is_empty());

        c.total_gold = 2_000_000;
        let r = official_repair_door(&mut c, 2_000_000, 60_001);
        assert!(r.repaired);
        assert_eq!(r.gold_spent, 2_000_000);
        assert_eq!(r.message, OFFICIAL_REPAIR_OK_MSG);
        assert_eq!(c.total_gold, 0);
        assert_eq!(r.events.len(), 1);

        // 已经满了：`RepairDoor()` 返 false ⇒ 不扣钱、回「不需要修理」。
        // 注意：oracle 先判 `TotalGold >= RepairDoorPrice`（`CastleOfficial.cs:365`）⇒ 先把钱补回。
        c.total_gold = 2_000_000;
        let r = official_repair_door(&mut c, 2_000_000, 60_001);
        assert!(!r.repaired);
        assert_eq!(r.gold_spent, 0);
        assert_eq!(r.message, OFFICIAL_REPAIR_NOT_NEEDED_MSG);
        assert_eq!(c.total_gold, 2_000_000, "未修好不扣钱");
    }

    #[test]
    fn official_repair_wall_checks_gold_then_repairs() {
        let mut c = ready_castle();
        c.total_gold = 500_000;
        {
            let w = c.left_wall.as_mut().unwrap();
            w.hp = 1_000;
            w.max_hp = 5_000;
            w.struck_tick = 0;
        }
        let r = official_repair_wall(&mut c, 1, 500_000, 60_001);
        assert!(r.repaired);
        assert_eq!(r.gold_spent, 500_000);
        assert_eq!(c.total_gold, 0);
        assert_eq!(c.left_wall.unwrap().hp, 5_000);
        assert_eq!(r.message, OFFICIAL_REPAIR_OK_MSG);
        // 无效索引也走「资金不足」前的金币门（先判钱，后判墙）。
        let r = official_repair_wall(&mut c, 0, 1, 60_001);
        assert_eq!(r.message, OFFICIAL_REPAIR_NOT_ENOUGH_GOLD_MSG);
        c.total_gold = 500_000;
        let r = official_repair_wall(&mut c, 0, 500_000, 60_001);
        assert!(!r.repaired);
        assert_eq!(r.message, OFFICIAL_REPAIR_NOT_NEEDED_MSG);
        assert_eq!(c.total_gold, 500_000);
    }

    #[test]
    fn castle_official_labels_dispatch_and_n5_hire_is_unreachable() {
        assert_eq!(
            castle_official_action("@openmaindoor"),
            Some(CastleOfficialAction::OpenMainDoor)
        );
        assert_eq!(
            castle_official_action("@OpenMainDoor"),
            Some(CastleOfficialAction::OpenMainDoor),
            "OrdinalIgnoreCase"
        );
        assert_eq!(
            castle_official_action("@closemaindoor"),
            Some(CastleOfficialAction::CloseMainDoor)
        );
        assert_eq!(
            castle_official_action("@repairdoornow"),
            Some(CastleOfficialAction::RepairDoor)
        );
        assert_eq!(
            castle_official_action("@repairwallnow1"),
            Some(CastleOfficialAction::RepairWall(1))
        );
        assert_eq!(
            castle_official_action("@repairwallnow2"),
            Some(CastleOfficialAction::RepairWall(2))
        );
        assert_eq!(
            castle_official_action("@repairwallnow3"),
            Some(CastleOfficialAction::RepairWall(3))
        );
        // N5（T-40）：`CompareLStr` 前缀匹配 ⇒ 带序号也命中，但下游 `Substring` 必抛。
        assert_eq!(
            castle_official_action("@hireguardnow3"),
            Some(CastleOfficialAction::HireUnreachable)
        );
        assert_eq!(
            castle_official_action("@HIREARCHERNOW7"),
            Some(CastleOfficialAction::HireUnreachable)
        );
        assert_eq!(castle_official_action("@exit"), None);
        assert_eq!(castle_official_action("@"), None);
        assert_eq!(castle_official_action("@repairwallnow4"), None);
    }

    #[test]
    fn n5_hire_substring_is_always_out_of_range_like_oracle() {
        // `CastleOfficial.cs:229/235`：length 实参用整串长度 ⇒ `13 + len > len` 恒真。
        assert_eq!(
            hire_substring("@hireguardnow0", HIRE_GUARD_NOW),
            Err(HireSubstringError::ArgumentOutOfRange)
        );
        assert_eq!(
            hire_substring("@hirearchernow12", HIRE_ARCHER_NOW),
            Err(HireSubstringError::ArgumentOutOfRange)
        );
        // 不带序号、甚至空前缀后缀：仍越界。
        assert_eq!(
            hire_substring("@hireguardnow", HIRE_GUARD_NOW),
            Err(HireSubstringError::ArgumentOutOfRange)
        );
        assert_eq!(
            hire_substring("", HIRE_GUARD_NOW),
            Err(HireSubstringError::ArgumentOutOfRange)
        );
        // 前缀匹配口径（`CompareLStr`：空 targ ⇒ false；src 短于 targ ⇒ false）。
        assert!(!compare_lstr("abc", ""));
        assert!(!compare_lstr("ab", "abc"));
        assert!(compare_lstr("ABCD", "abc"));
        assert!(compare_lstr("abc", "abc"));
    }

    /// ⑧-8 reviewer-5 O-1：`compare_lstr` 必须与 oracle `HUtil32.CompareLStr`
    /// （`src/OpenMir2/HUtil32.cs:550-576`）同语义，**且对任意 UTF-8 输入都不 panic**。
    ///
    /// 旧实现 `src[..targ.len()].eq_ignore_ascii_case(targ)` 会对落在多字节 `char` 中间的
    /// 字节边界切片 ⇒ Rust panic；oracle 逐 UTF-16 码元比较不会抛 ⇒ 这是移植偏差，不是
    /// oracle 行为。本用例用一段**首个字符边界恰在 `targ.len()` 中间**的输入锁死这一点。
    #[test]
    fn compare_lstr_matches_oracle_prefix_semantics_without_panic() {
        // oracle 前置门与全等的正样本。
        assert!(!compare_lstr("abc", "")); // compn <= 0
        assert!(!compare_lstr("", "@hireguardnow")); // src 短于 targ
        assert!(!compare_lstr("@hireguardno", "@hireguardnow")); // src 短 1
        assert!(compare_lstr("@hireguardnow", "@hireguardnow"));
        assert!(compare_lstr("@HireGuardNow", "@hireguardnow")); // ASCII 大小写折叠
        assert!(compare_lstr("@hireguardnow0", "@hireguardnow")); // 前缀匹配，尾部多余无碍
        assert!(!compare_lstr("@hirearchernow", "@hireguardnow")); // 位同位不等

        // O-1 牙：`targ.len()` 落在 'é'（2 字节）中间 —— 旧实现 `src[..13]` 必 panic。
        assert!(!compare_lstr("@hireguardnoé", "@hireguardnow"));
        assert!(!compare_lstr("@hireguardnoézz", "@hireguardnow"));
        // 其它多字节位置/长度：不得 panic，且与 oracle 逐位结果一致（首位或中位不等 ⇒ false）。
        assert!(!compare_lstr("é@hireguardnow", "@hireguardnow"));
        assert!(!compare_lstr("@hireguardnowé", "@hirearchernow"));
        assert!(!compare_lstr("城堡@hireguardnow", "@hireguardnow"));
    }

    #[test]
    fn door_state_text_matches_oracle_order() {
        // `CastleOfficial.cs:52-67`：先 `Death`，再 `IsOpened`。
        let mut d = door(10_000, 10_000);
        assert_eq!(door_state_text(&d), DOOR_STATE_CLOSED);
        d.opened = true;
        assert_eq!(door_state_text(&d), DOOR_STATE_OPENED);
        d.death = true;
        assert_eq!(door_state_text(&d), DOOR_STATE_DESTROYED);
    }

    impl UserCastle {
        fn assert_n1_noop(&mut self, call: impl FnOnce(&mut UserCastle)) {
            let before = self.clone();
            call(self);
            assert_eq!(*self, before, "N1 入口必须可观测 no-op（T-38）");
        }
    }

    // ---- ⑧-6 第②片：`UserCastle.Initialize`（实体挂图）。----

    fn unit(name: &str, x: i16, y: i16, hp: u16) -> DefenseUnitSpec {
        DefenseUnitSpec {
            name: name.to_string(),
            x,
            y,
            status: false,
            hp,
        }
    }

    /// 一份与 `CastleConf` 默认值等价的规格（`CastleConf.cs:39-93`）：弓手/守卫 `nHP=2000`>0。
    fn defense_specs() -> CastleDefenseSpecs {
        CastleDefenseSpecs {
            own_guild: String::new(),
            map_name: "3".to_string(),
            home_map: "3".to_string(),
            home_x: 644,
            home_y: 290,
            war_range_x: 100,
            war_range_y: 100,
            envir_list: vec!["0151".to_string(), "d701".to_string()],
            palace_map: "0150".to_string(),
            secret_map: "D701".to_string(),
            main_door: DefenseUnitSpec {
                name: "MainDoor".to_string(),
                x: 672,
                y: 330,
                status: true,
                hp: 2000,
            },
            left_wall: unit("LeftWall", 624, 278, 2000),
            center_wall: unit("CenterWall", 627, 278, 2000),
            right_wall: unit("RightWall", 634, 271, 2000),
            archers: (0..MAX_CASTLE_ARCHER)
                .map(|i| unit("弓箭手", i as i16, 100, 2000))
                .collect(),
            guards: (0..MAX_CASTLE_GUARD)
                .map(|i| unit("守卫", i as i16, 200, 2000))
                .collect(),
        }
    }

    /// 记录式宿主：地图/怪物库可控，按调用序记录 `RegenMonsterByName` 与 `Warn`。
    struct FakeEnv {
        owner: bool,
        maps: Vec<String>,
        /// 怪物数据库里的「名字 ⇒ 模板 MaxHP」。
        creatures: Vec<(&'static str, i32)>,
        warns: Vec<String>,
        regen_calls: Vec<(String, i16, i16, String)>,
    }

    impl FakeEnv {
        fn full() -> Self {
            FakeEnv {
                owner: true,
                maps: vec!["3".to_string(), "0150".to_string(), "D701".to_string()],
                creatures: vec![
                    ("MainDoor", 5000),
                    ("LeftWall", 3000),
                    ("CenterWall", 3000),
                    ("RightWall", 3000),
                    ("弓箭手", 900),
                    ("守卫", 700),
                ],
                warns: Vec::new(),
                regen_calls: Vec::new(),
            }
        }
    }

    impl CastleInitializeEnv for FakeEnv {
        fn map_owner_is_self(&self, _map_name: &str) -> bool {
            self.owner
        }
        fn find_map(&self, map_name: &str) -> bool {
            self.maps.iter().any(|m| m == map_name)
        }
        fn map_id(&self, map_name: &str) -> Option<usize> {
            self.maps.iter().position(|m| m == map_name)
        }
        fn regen_monster_by_name(
            &mut self,
            map_name: &str,
            x: i16,
            y: i16,
            name: &str,
        ) -> Option<DefenseCreature> {
            self.regen_calls
                .push((map_name.to_string(), x, y, name.to_string()));
            self.creatures
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, hp)| DefenseCreature {
                    max_hp: *hp,
                    // 记录式 env：actor id 取「本次调用序号」，供 C3 绑定断言。
                    actor_id: i32::try_from(self.regen_calls.len()).unwrap_or(i32::MAX),
                })
        }
        fn warn(&mut self, text: String) {
            self.warns.push(text);
        }
    }

    fn spawns(events: &[CastleEvent]) -> Vec<&DefenseSpawn> {
        events
            .iter()
            .filter_map(|e| match e {
                CastleEvent::SpawnDefenseUnit(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    fn structures(events: &[CastleEvent]) -> Vec<StructureEvent> {
        events
            .iter()
            .filter_map(|e| match e {
                CastleEvent::Structure(s) => Some(*s),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn initialize_spawns_1_door_3_walls_12_archers_12_guards_in_oracle_order() {
        let specs = defense_specs();
        let mut env = FakeEnv::full();
        let mut c = UserCastle::new("沙巴克", config());
        let events = c.initialize(&specs, &mut env);

        // 调用序 = `UserCastle.cs:128-220`：主门 → 左/中/右墙 → 弓手 0..12 → 守卫 0..12。
        let names: Vec<&str> = env.regen_calls.iter().map(|r| r.3.as_str()).collect();
        let mut want: Vec<&str> = vec!["MainDoor", "LeftWall", "CenterWall", "RightWall"];
        want.extend(std::iter::repeat_n("弓箭手", MAX_CASTLE_ARCHER));
        want.extend(std::iter::repeat_n("守卫", MAX_CASTLE_GUARD));
        assert_eq!(names, want);
        assert_eq!(env.regen_calls.len(), 28);

        let sp = spawns(&events);
        assert_eq!(sp.len(), 28);
        assert_eq!(sp[0].role, DefenseRole::MainDoor);
        assert_eq!(sp[1].role, DefenseRole::LeftWall);
        assert_eq!(sp[2].role, DefenseRole::CenterWall);
        assert_eq!(sp[3].role, DefenseRole::RightWall);
        assert_eq!(sp[4].role, DefenseRole::Archer(0));
        assert_eq!(sp[16].role, DefenseRole::Guard(0));
        assert_eq!(sp[27].role, DefenseRole::Guard(11));

        // 门/墙/弓手绑定 `Castle`；守卫不绑（`UserCastle.cs:134/148/158/168/186` vs `UserCastle.cs:202-206`）。
        assert!(
            sp[0].castle_owner && sp[1].castle_owner && sp[2].castle_owner && sp[3].castle_owner
        );
        assert!(sp[4..16].iter().all(|s| s.castle_owner));
        assert!(sp[16..28].iter().all(|s| !s.castle_owner));
        // 弓手 `GuardDirection=3`（`UserCastle.cs:187`），其余无。
        assert!(sp[4..16].iter().all(|s| s.guard_direction == Some(3)));
        assert!(sp[0..4].iter().all(|s| s.guard_direction.is_none()));
        assert!(sp[16..28].iter().all(|s| s.guard_direction.is_none()));

        // 世界态：门 `nStatus=true` ⇒ `Open()`（Dir=7/IsOpened/StoneMode/HoldPlace=false）+ 帧/掩码。
        let door = c.main_door.expect("main door");
        assert_eq!(
            (door.x, door.y, door.hp, door.max_hp),
            (672, 330, 2000, 5000)
        );
        assert_eq!(door.dir, 7);
        assert!(door.opened && door.stone_mode && !door.hold_place);
        assert!(structures(&events).contains(&StructureEvent::RefDigUp {
            dir: 7,
            x: 672,
            y: 330
        }));
        let lw = c.left_wall.expect("left wall");
        assert_eq!((lw.hp, lw.max_hp), (2000, 3000));
        assert_eq!(
            (c.archers[0].unwrap().hp, c.archers[0].unwrap().max_hp),
            (2000, 900)
        );
        assert_eq!(
            (c.guards[11].unwrap().hp, c.guards[11].unwrap().max_hp),
            (2000, 700)
        );
        assert!(env.warns.is_empty(), "{:?}", env.warns);
    }

    #[test]
    fn initialize_door_status_false_does_not_open() {
        let mut specs = defense_specs();
        specs.main_door.status = false;
        let mut env = FakeEnv::full();
        let mut c = UserCastle::new("沙巴克", config());
        let events = c.initialize(&specs, &mut env);
        let door = c.main_door.expect("main door");
        assert_eq!(door.dir, 0);
        assert!(!door.opened && !door.stone_mode && !door.hold_place);
        assert!(structures(&events).is_empty());
        assert_eq!(spawns(&events).len(), 28);
    }

    #[test]
    fn initialize_skips_zero_hp_archers_and_guards() {
        let mut specs = defense_specs();
        specs.archers[3].hp = 0;
        specs.guards[0].hp = 0;
        specs.guards[11].hp = 0;
        let mut env = FakeEnv::full();
        let mut c = UserCastle::new("沙巴克", config());
        let events = c.initialize(&specs, &mut env);

        assert_eq!(env.regen_calls.len(), 28 - 3);
        assert!(c.archers[3].is_none());
        assert!(c.guards[0].is_none() && c.guards[11].is_none());
        assert!(c.archers[2].is_some() && c.archers[4].is_some());
        let sp = spawns(&events);
        assert!(sp
            .iter()
            .all(|s| s.role != DefenseRole::Archer(3) && s.role != DefenseRole::Guard(0)));
        assert!(env.warns.is_empty(), "{:?}", env.warns);
    }

    #[test]
    fn initialize_non_owner_map_is_noop() {
        let specs = defense_specs();
        let mut env = FakeEnv::full();
        env.owner = false;
        let mut c = UserCastle::new("沙巴克", config());
        let events = c.initialize(&specs, &mut env);
        assert!(events.is_empty());
        assert!(env.regen_calls.is_empty());
        assert!(env.warns.is_empty());
        assert!(c.main_door.is_none() && c.archers[0].is_none());
        // `LoadConfig` 部分仍然生效（`MapName` 已写入）。
        assert_eq!(c.map_name.as_deref(), Some("3"));
    }

    #[test]
    fn initialize_missing_castle_map_warns_and_skips_all_spawns() {
        let specs = defense_specs();
        let mut env = FakeEnv::full();
        env.maps.retain(|m| m.as_str() != "3");
        let mut c = UserCastle::new("沙巴克", config());
        let events = c.initialize(&specs, &mut env);
        assert!(events.is_empty());
        assert!(env.regen_calls.is_empty());
        assert_eq!(
            env.warns,
            vec!["[错误信息] 城堡所在地图不存在(检查地图配置文件里是否有地图3的设置)".to_string()]
        );
    }

    #[test]
    fn initialize_warns_palace_and_secret_map_but_still_spawns() {
        let specs = defense_specs();
        let mut env = FakeEnv::full();
        env.maps = vec!["3".to_string()];
        let mut c = UserCastle::new("沙巴克", config());
        let events = c.initialize(&specs, &mut env);
        assert_eq!(
            env.warns,
            vec![
                "皇宫地图0150没找到!!!".to_string(),
                "密道地图D701没找到!!!".to_string()
            ]
        );
        assert_eq!(spawns(&events).len(), 28);
    }

    #[test]
    fn initialize_wall_failure_warnings_use_previous_unit_name_quirk() {
        let specs = defense_specs();
        let mut env = FakeEnv::full();
        // 只有主门在怪物库里：三面墙 + 弓手 + 守卫全造怪失败。
        env.creatures.retain(|(n, _)| *n == "MainDoor");
        let mut c = UserCastle::new("沙巴克", config());
        let _ = c.initialize(&specs, &mut env);
        // oracle copy-paste 怪癖（`UserCastle.cs:152/162/172`）：
        // 左墙失败报 MainDoor.sName；中墙失败报 LeftWall.sName；右墙失败报 CenterWall.sName。
        assert_eq!(
            env.warns[0],
            "[错误信息] 城堡初始化城门失败，检查怪物数据库里有没城门的设置: MainDoor"
        );
        assert_eq!(
            env.warns[1],
            "[错误信息] 城堡初始化左城墙失败，检查怪物数据库里有没左城墙的设置: LeftWall"
        );
        assert_eq!(
            env.warns[2],
            "[错误信息] 城堡初始化中城墙失败，检查怪物数据库里有没中城墙的设置: CenterWall"
        );
        // 随后 12 条弓手 + 12 条守卫告警。
        assert_eq!(env.warns.len(), 3 + MAX_CASTLE_ARCHER + MAX_CASTLE_GUARD);
        assert_eq!(
            env.warns[3],
            "[错误信息] 城堡初始化弓箭手失败，检查怪物数据库里有没弓箭手的设置: 弓箭手"
        );
        assert_eq!(
            env.warns[15],
            "[错误信息] 城堡初始化守卫失败(检查怪物数据库里有没守卫怪物)"
        );
        assert!(c.main_door.is_some());
        assert!(c.left_wall.is_none() && c.center_wall.is_none() && c.right_wall.is_none());
    }

    #[test]
    fn initialize_missing_main_door_uses_english_warning() {
        let specs = defense_specs();
        let mut env = FakeEnv::full();
        env.creatures.retain(|(n, _)| *n != "MainDoor");
        let mut c = UserCastle::new("沙巴克", config());
        let _ = c.initialize(&specs, &mut env);
        assert_eq!(
            env.warns[0],
            "[Error] UserCastle.Initialize MainDoor.UnitObj = nil"
        );
        assert!(c.main_door.is_none());
    }

    /// **C2 `castle_initialize_call_order_and_skip_counts`**：`Castle/0` 口径
    /// （守卫 1..4 HP=0 跳过、5..12 缺模板）⇒ 24 次 Regen、16 个 `SpawnDefenseUnit`、8 条告警。
    ///
    /// 牙：去掉**守卫** `hp==0` 跳过 ⇒ 28 次 Regen（本用例红）；调换墙序 ⇒ 名字序红。
    /// （弓手在本用例全部 `HP>0`，其跳过分支由 C1 的解析默认值用例覆盖；「守卫默认 2000」属 C1，
    /// 不在本手造 specs 用例内。）
    #[test]
    fn castle_initialize_call_order_and_skip_counts() {
        let mut specs = defense_specs();
        for g in &mut specs.guards[..4] {
            g.hp = 0;
        }
        let mut env = FakeEnv::full();
        env.creatures.retain(|(n, _)| *n != "守卫");
        let mut c = UserCastle::new("沙巴克", config());
        let events = c.initialize(&specs, &mut env);

        let names: Vec<&str> = env.regen_calls.iter().map(|r| r.3.as_str()).collect();
        let mut want: Vec<&str> = vec!["MainDoor", "LeftWall", "CenterWall", "RightWall"];
        want.extend(std::iter::repeat_n("弓箭手", 12));
        want.extend(std::iter::repeat_n("守卫", 8));
        assert_eq!(names, want, "调用序 = oracle（墙序 + 守卫 5..12）");
        assert_eq!(env.regen_calls.len(), 24);
        let sp = spawns(&events);
        assert_eq!(sp.len(), 16);
        assert_eq!(env.warns.len(), 8);
        assert!(env
            .warns
            .iter()
            .all(|w| w == "[错误信息] 城堡初始化守卫失败(检查怪物数据库里有没守卫怪物)"));
        assert!(c.main_door.is_some() && c.left_wall.is_some());
        assert!(c.archers.iter().all(Option::is_some));
        assert!(c.guards.iter().all(Option::is_none));
    }

    /// **C3（绑定半）**：造怪句柄写回 `CastleUnit.entity_id` 与 `DefenseSpawn.actor_id`，
    /// 且门/墙/弓手 `castle_owner=true`、守卫 `false`。
    ///
    /// 牙：`regen_and_bind` 不写 `entity_id` ⇒ 本用例红；把守卫 `castle_owner` 置 true ⇒ 红。
    #[test]
    fn castle_spawn_carries_actor_id_binding() {
        let specs = defense_specs();
        let mut env = FakeEnv::full();
        let mut c = UserCastle::new("沙巴克", config());
        let events = c.initialize(&specs, &mut env);
        let sp = spawns(&events);
        assert_eq!(sp.len(), 28);
        for s in &sp {
            assert!(s.actor_id > 0);
            let unit = c.defense_unit(s.role).expect("unit bound");
            assert_eq!(unit.entity_id, Some(s.actor_id));
        }
        assert_eq!(sp[0].role, DefenseRole::MainDoor);
        assert_eq!(c.main_door.unwrap().entity_id, Some(sp[0].actor_id));
        assert_eq!(c.archers[0].unwrap().entity_id, Some(sp[4].actor_id));
        assert!(sp[..16].iter().all(|s| s.castle_owner));
        assert!(sp[16..].iter().all(|s| !s.castle_owner));
    }

    // =====================================================================
    // 车道 castlewar：攻城区语义判据（C1/C2/C3/C4/C5/C6/C13）
    // =====================================================================

    /// C1 `castle_war_area_predicate`：部署 `Home=644/290`、`Range=100/100` 的谓词向量。
    ///
    /// 牙：`<` 改 `<=` ⇒ `(3,544,290)/(3,644,390)` 翻真；删 EnvirList 扫描 ⇒ 0151 红；
    /// 删 Palace/Secret 分支 ⇒ 0150/D701 红；删 `map_id=None` 门 ⇒ panic。
    #[test]
    fn c1_castle_war_area_predicate_vectors() {
        let mut c = UserCastle::new("沙巴克", config());
        c.home_x = 644;
        c.home_y = 290;
        c.war_range_x = 100;
        c.war_range_y = 100;
        c.castle_map_id = Some(0); // "3"
        c.palace_map_id = Some(1); // "0150"
        c.secret_map_id = Some(2); // "D701"
        c.envir_map_ids = vec![3, 4]; // "0151", "0152"

        assert!(c.in_castle_war_area(Some(0), 643, 289));
        assert!(
            !c.in_castle_war_area(Some(0), 544, 290),
            "dx=100 ⇒ 严格 < 为假"
        );
        assert!(!c.in_castle_war_area(Some(0), 543, 290), "dx=101");
        assert!(c.in_castle_war_area(Some(0), 743, 290), "dx=99");
        assert!(!c.in_castle_war_area(Some(0), 744, 290), "dx=100");
        assert!(c.in_castle_war_area(Some(0), 644, 389), "dy=99");
        assert!(!c.in_castle_war_area(Some(0), 644, 390), "dy=100");
        assert!(c.in_castle_war_area(Some(1), 0, 0), "皇宫无条件 true");
        assert!(c.in_castle_war_area(Some(2), 9999, 9999), "密道无条件 true");
        assert!(c.in_castle_war_area(Some(3), 0, 0), "EnvirList 命中");
        assert!(!c.in_castle_war_area(Some(9), 644, 290), "未知图 false");
        assert!(!c.in_castle_war_area(None, 644, 290), "envir=None ⇒ false");
    }

    /// C1 补充：EnvirList 的 `OrdinalIgnoreCase` 等价 —— 启动时归一成 map_id 后 `d701`/`D701`
    /// 命中同一个 id（`MapRegistry::id_of` 的 `normalize_name`）。
    #[test]
    fn c1_envir_list_resolves_to_map_ids_case_insensitively() {
        struct Env {
            ids: Vec<(&'static str, usize)>,
        }
        impl CastleInitializeEnv for Env {
            fn map_owner_is_self(&self, _map_name: &str) -> bool {
                true
            }
            fn find_map(&self, map_name: &str) -> bool {
                self.ids.iter().any(|(n, _)| *n == map_name)
            }
            fn map_id(&self, map_name: &str) -> Option<usize> {
                self.ids
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(map_name))
                    .map(|(_, id)| *id)
            }
            fn regen_monster_by_name(
                &mut self,
                _map_name: &str,
                _x: i16,
                _y: i16,
                _name: &str,
            ) -> Option<DefenseCreature> {
                None
            }
            fn warn(&mut self, _text: String) {}
        }
        let mut env = Env {
            ids: vec![("3", 0), ("0150", 1), ("D701", 2), ("d701", 2), ("0151", 3)],
        };
        let mut c = UserCastle::new("沙巴克", config());
        let specs = CastleDefenseSpecs {
            own_guild: String::new(),
            map_name: "3".to_string(),
            home_map: "3".to_string(),
            home_x: 644,
            home_y: 290,
            war_range_x: 100,
            war_range_y: 100,
            envir_list: vec!["d701".to_string(), "0151".to_string()],
            palace_map: "0150".to_string(),
            secret_map: "D701".to_string(),
            main_door: DefenseUnitSpec {
                name: "x".to_string(),
                x: 0,
                y: 0,
                status: false,
                hp: 0,
            },
            left_wall: DefenseUnitSpec {
                name: "x".to_string(),
                x: 0,
                y: 0,
                status: false,
                hp: 0,
            },
            center_wall: DefenseUnitSpec {
                name: "x".to_string(),
                x: 0,
                y: 0,
                status: false,
                hp: 0,
            },
            right_wall: DefenseUnitSpec {
                name: "x".to_string(),
                x: 0,
                y: 0,
                status: false,
                hp: 0,
            },
            archers: Vec::new(),
            guards: Vec::new(),
        };
        c.initialize(&specs, &mut env);
        assert_eq!(c.envir_map_ids, vec![2, 3], "小写 d701 归一为同一个 map_id");
        assert!(c.in_castle_war_area(Some(2), 0, 0));
        assert!(c.in_castle_war_area(Some(3), 0, 0));
    }

    fn enter_castle() -> UserCastle {
        let mut c = UserCastle::new("沙巴克", config());
        c.map_name = Some("3".to_string());
        c.home_map = "3".to_string();
        c.home_x = 644;
        c.home_y = 290;
        c.palace_map_id = Some(1);
        c.castle_map_id = Some(0);
        // `MasterGuild` 必须已挂（oracle `IsMasterGuild` 的 `MasterGuild != null` 门）；
        // 只给 `OwnGuild` 名字不够（`is_master_guild_name` 照 oracle 在 `None` 时恒 false）。
        c.master_guild = Some(G_DEFEND);
        c.own_guild = Some("守方".to_string());
        c.under_war = false;
        c
    }

    /// C2/C3/C4 `castle_relocation_vectors`：四条落点向量，按独立复算的 `System.Random(seed)`
    /// 手算值钉死（seed=20261021；`Random(5)` 前两抽 = [4,2]、`Random(9)` = [8,5]、
    /// `Random(13)` = [12,7]）。
    ///
    /// 牙：`Random(5)` 改 `Random(9)` ⇒ 坐标/下一抽红；交换 X/Y（r0≠r1）⇒ 坐标红；
    /// 成员判定写成「有会即成员」⇒ 非成员臂红；死亡三臂顺序写反 ⇒ 红。
    #[test]
    fn c2_c3_c4_relocation_vectors_and_rng_ledger() {
        const SEED: i32 = 20261021;
        let castle = enter_castle();
        let red = RedDieHome {
            map_name: "3".to_string(),
            x: 838,
            y: 668,
        };

        // (a) 非成员进皇宫：2×Random(5)，X 先 Y 后 ⇒ (644-2+4, 290-2+2) = (646, 290)。
        let mut rng = RandomNumber::with_seed(SEED);
        let reloc = relocate_on_enter(
            &EnterRelocateInput {
                castle: Some(&castle),
                map_id: 1,
                x: 644,
                y: 290,
                my_guild_name: None,
                home_map: "0",
                home_x: 644,
                home_y: 290,
            },
            &mut rng,
        )
        .expect("非成员进皇宫必须改写");
        assert_eq!((reloc.map_name.as_str(), reloc.x, reloc.y), ("0", 646, 290));
        assert_eq!(rng.random_below(5), 0, "R5 第 3 抽 = 0（恰 2 抽）");

        // (b) 成员进皇宫：castle.MapName + 2×Random(9) ⇒ (644-4+8, 290-4+5) = (648, 291)。
        let mut rng = RandomNumber::with_seed(SEED);
        let reloc = relocate_on_enter(
            &EnterRelocateInput {
                castle: Some(&castle),
                map_id: 1,
                x: 644,
                y: 290,
                my_guild_name: Some("守方"),
                home_map: "0",
                home_x: 644,
                home_y: 290,
            },
            &mut rng,
        )
        .expect("成员进皇宫必须改写");
        assert_eq!((reloc.map_name.as_str(), reloc.x, reloc.y), ("3", 648, 291));
        assert_eq!(rng.random_below(9), 1, "R9 第 3 抽 = 1（恰 2 抽）");

        // (c) 死亡：成员 + UnderWar ⇒ castle.HomeMap + 2×Random(9)。
        let mut war = enter_castle();
        war.under_war = true;
        let mut rng = RandomNumber::with_seed(SEED);
        let reloc = relocate_on_death(
            &DeathRelocateInput {
                member_castle: Some(&war),
                hp: 0,
                pvp_level: 0,
                home_map: "0",
                home_x: 100,
                home_y: 200,
                red_die: &red,
            },
            &mut rng,
        )
        .expect("HP<=0 必须改写");
        assert_eq!((reloc.map_name.as_str(), reloc.x, reloc.y), ("3", 648, 291));

        // (d) 死亡：非成员 ⇒ 本人 HomeMap + 2×Random(5) ⇒ (100-2+4, 200-2+2) = (102, 200)。
        let mut rng = RandomNumber::with_seed(SEED);
        let reloc = relocate_on_death(
            &DeathRelocateInput {
                member_castle: None,
                hp: 0,
                pvp_level: 1,
                home_map: "0",
                home_x: 100,
                home_y: 200,
                red_die: &red,
            },
            &mut rng,
        )
        .expect("HP<=0 必须改写");
        assert_eq!((reloc.map_name.as_str(), reloc.x, reloc.y), ("0", 102, 200));

        // (e) 死亡：红名 ⇒ RedDieHome + 2×Random(13) ⇒ (838+12, 668+7) = (850, 675)。
        let mut rng = RandomNumber::with_seed(SEED);
        let reloc = relocate_on_death(
            &DeathRelocateInput {
                member_castle: Some(&war),
                hp: 0,
                pvp_level: 2,
                home_map: "0",
                home_x: 100,
                home_y: 200,
                red_die: &red,
            },
            &mut rng,
        )
        .expect("HP<=0 必须改写");
        assert_eq!((reloc.map_name.as_str(), reloc.x, reloc.y), ("3", 850, 675));

        // (f) HP>0 ⇒ 0 抽、不改写。
        let mut rng = RandomNumber::with_seed(SEED);
        assert!(relocate_on_death(
            &DeathRelocateInput {
                member_castle: Some(&war),
                hp: 1,
                pvp_level: 2,
                home_map: "0",
                home_x: 100,
                home_y: 200,
                red_die: &red,
            },
            &mut rng,
        )
        .is_none());
        assert_eq!(rng.random_below(13), 12, "HP>0 ⇒ 一次都没抽");
    }

    /// C2 补充：成员但不在皇宫且非战 ⇒ 0 抽、不改写；`UnderWar=false` 的非皇宫图也不改写。
    #[test]
    fn c2_member_not_palace_no_relocation_zero_draws() {
        let castle = enter_castle();
        let mut rng = RandomNumber::with_seed(20261021);
        assert!(relocate_on_enter(
            &EnterRelocateInput {
                castle: Some(&castle),
                map_id: 0,
                x: 644,
                y: 290,
                my_guild_name: Some("守方"),
                home_map: "0",
                home_x: 644,
                home_y: 290,
            },
            &mut rng,
        )
        .is_none());
        assert_eq!(rng.random_below(5), 4, "0 抽（独立复算第 1 抽 = 4）");
    }

    /// C5 `area_state_bit4`：四向量 + 重复置位幂等。
    ///
    /// 牙：位常量 4 改 8 ⇒ 红；`|=` 改 `^=` ⇒ 重复置位 4→0 红；`|=` 改 `=` ⇒ `1|2|4` 变 4 红。
    #[test]
    fn c5_area_state_bit4_and_idempotent_set() {
        assert_eq!(area_state(true, false, false), 1);
        assert_eq!(area_state(false, true, false), 2);
        assert_eq!(area_state(false, false, true), 4);
        assert_eq!(area_state(true, true, true), 7);
        let mut n8 = area_state(true, true, true);
        set_area_flag(&mut n8, AREA_STATE_GUILD_WAR, true);
        assert_eq!(n8, 7, "同一 bit 重复置位幂等");
        let mut n8 = 4;
        set_area_flag(&mut n8, AREA_STATE_GUILD_WAR, false);
        assert_eq!(
            n8, 4,
            "cond=false 不清位（ChangePkStatus 只由调用方显式清）"
        );
    }

    /// C6 `castle_name_color_vectors`：普通/PvpFlag/同会/敌对/攻城区 221/N1/红名 255。
    ///
    /// 牙：221 改 255 ⇒ 红；红名改 249 ⇒ 红；「补全」城主/攻方色 ⇒ 红（必须 Err）。
    #[test]
    fn c6_name_color_vectors() {
        let colors = CastleNameColors::default();
        let mut war = enter_castle();
        war.under_war = true;
        let base = ChrColorInput {
            castle: None,
            target_name_color: 255,
            target_pvp_level: 0,
            target_pvp_flag: false,
            target_fight3_zone: false,
            my_guild_name: None,
            target_guild_name: None,
            guild_relation: 0,
            viewer_is_ally_of_target: false,
            viewer_in_guild_war_area: false,
            target_in_guild_war_area: false,
        };
        // 普通。
        assert_eq!(get_chr_color(&base, &colors).unwrap().color, 255);
        // PvpFlag ⇒ 47。
        let mut i = base;
        i.target_pvp_flag = true;
        assert_eq!(get_chr_color(&i, &colors).unwrap().color, 47);
        // 同会/盟友 ⇒ 180；宣战 ⇒ 69。
        let mut i = base;
        i.guild_relation = 1;
        assert_eq!(get_chr_color(&i, &colors).unwrap().color, 180);
        let mut i = base;
        i.guild_relation = 3;
        assert_eq!(get_chr_color(&i, &colors).unwrap().color, 180);
        let mut i = base;
        i.guild_relation = 2;
        assert_eq!(get_chr_color(&i, &colors).unwrap().color, 69);
        // Fight3Zone：同会 180 / 异会 69。
        let mut i = base;
        i.target_fight3_zone = true;
        i.my_guild_name = Some("守方");
        i.target_guild_name = Some("守方");
        assert_eq!(get_chr_color(&i, &colors).unwrap().color, 180);
        let mut i = base;
        i.target_fight3_zone = true;
        i.my_guild_name = Some("攻方");
        i.target_guild_name = Some("守方");
        assert_eq!(get_chr_color(&i, &colors).unwrap().color, 69);
        // 攻城区 + 战时 + 双方 InGuildWarArea：无行会观者 ⇒ 221 且置 GuildWarArea。
        let mut i = base;
        i.castle = Some(&war);
        i.viewer_in_guild_war_area = true;
        i.target_in_guild_war_area = true;
        let r = get_chr_color(&i, &colors).unwrap();
        assert_eq!(r.color, 221);
        assert!(r.guild_war_area, "副作用 GuildWarArea=true");
        // 有行会观者（非城主）⇒ N1。
        let mut i = base;
        i.castle = Some(&war);
        i.my_guild_name = Some("攻方");
        i.target_guild_name = Some("守方");
        i.viewer_in_guild_war_area = true;
        i.target_in_guild_war_area = true;
        assert_eq!(
            get_chr_color(&i, &colors),
            Err(ChrColorError::AttackGuildNotImplemented)
        );
        // 红名（PvpLevel>=2）且不在攻城区 ⇒ 255（怪行为：跳过整个 PK 块）。
        let mut i = base;
        i.target_pvp_level = 2;
        i.target_pvp_flag = true;
        assert_eq!(get_chr_color(&i, &colors).unwrap().color, 255);
    }

    /// Minor 1：`IsMasterGuild` 的 `MasterGuild != null` 门（oracle `UserCastle.cs:639-642`）。
    /// 只有 `OwnGuild` 名字、`MasterGuild == None` ⇒ 恒 false（行会表装载前的相位）。
    ///
    /// 牙：去掉 `if self.master_guild.is_none() { return false; }` ⇒ 第一条断言红。
    #[test]
    fn master_guild_name_requires_master_guild_loaded() {
        let mut c = UserCastle::new("沙巴克", config());
        c.own_guild = Some("守方".to_string());
        assert!(
            !c.is_master_guild_name(Some("守方")),
            "MasterGuild == None ⇒ 名字相等也不得判城主"
        );
        c.master_guild = Some(G_DEFEND);
        assert!(c.is_master_guild_name(Some("守方")));
        assert!(!c.is_master_guild_name(Some("攻方")));
        assert!(!c.is_master_guild_name(None));
    }

    /// N1 判据的 `!castle.IsMember(this)` 门（oracle `PlayObject.Message.cs:319-338`）：
    /// 城主行会成员（`own_guild` 与会名相同、`MasterGuild` 已挂）站皇宫且 1s 门过
    /// ⇒ 自动占领链不执行 ⇒ `NameColorChanged` 块照常出 2 条事件。
    ///
    /// 牙：去掉 `!c.is_member_name(...)` ⇒ N1 ⇒ 0 条事件 ⇒ 红。
    #[test]
    fn c13_palace_master_member_skips_n1_and_emits_two_events() {
        let mut c = ready_castle();
        c.under_war = true;
        c.palace_map_id = Some(0);
        let castles = vec![c];
        let players = [CastleWarPlayer {
            actor_id: 7,
            map_id: 0,
            x: 644,
            y: 290,
            guild_name: Some("守方".to_string()),
        }];
        let mut states = std::collections::HashMap::new();
        assert!(
            castle_war_tick(&castles, &players, &mut states, 0).is_empty(),
            "t=0：1s 门未开（只置 `InGuildWarArea`，不发事件）"
        );
        let ev = castle_war_tick(&castles, &players, &mut states, 1001);
        assert_eq!(
            ev,
            vec![
                CastleWarEvent::AreaState {
                    actor_id: 7,
                    in_guild_war_area: true
                },
                CastleWarEvent::ShowName { actor_id: 7 },
            ],
            "城主成员站皇宫 ⇒ 不抛 N1 ⇒ RefUserState()+RefShowName() 两条"
        );
    }

    /// C13 `castle_in_guild_war_area_setclear`：每 tick 置位、1s 门清除、变化才发 766。
    ///
    /// 牙：清除移出 1s 门 ⇒ 第一 tick 就清红；置位移进 1s 门 ⇒ 迟到红；无变化也发 ⇒ 重复帧红。
    #[test]
    fn c13_war_area_set_clear_and_one_second_gate() {
        let mut war = enter_castle();
        war.under_war = true;
        let castles = vec![war];
        let players = vec![CastleWarPlayer {
            actor_id: 7,
            map_id: 0,
            x: 644,
            y: 290,
            guild_name: None,
        }];
        let mut states = std::collections::HashMap::new();
        // t=0：置位（每 tick），但 1s 门未到 ⇒ 不发帧。
        let ev = castle_war_tick(&castles, &players, &mut states, 0);
        assert!(ev.is_empty(), "1s 门内不得发 766");
        assert!(states[&7].in_guild_war_area);
        assert!(states[&7].name_color_changed);
        // t=1000：严格 `>`，仍不发。
        assert!(castle_war_tick(&castles, &players, &mut states, 1000).is_empty());
        // t=1001：发 AreaState + ShowName。
        assert_eq!(
            castle_war_tick(&castles, &players, &mut states, 1001),
            vec![
                CastleWarEvent::AreaState {
                    actor_id: 7,
                    in_guild_war_area: true
                },
                CastleWarEvent::ShowName { actor_id: 7 }
            ]
        );
        // t=1002：无变化 ⇒ 不发。
        assert!(castle_war_tick(&castles, &players, &mut states, 1002).is_empty());
        // 离开攻城区：每 tick 不置位；清除在 1s 门内（t=2002 恰好差 1001）。
        let out = vec![CastleWarPlayer {
            actor_id: 7,
            map_id: 9,
            x: 0,
            y: 0,
            guild_name: None,
        }];
        assert!(castle_war_tick(&castles, &out, &mut states, 1500).is_empty());
        assert!(states[&7].in_guild_war_area, "清除被 1s 门节流");
        assert_eq!(
            castle_war_tick(&castles, &out, &mut states, 2002),
            vec![
                CastleWarEvent::AreaState {
                    actor_id: 7,
                    in_guild_war_area: false
                },
                CastleWarEvent::ShowName { actor_id: 7 }
            ]
        );
        // 皇宫 + 有行会 ⇒ 自动占领链 N1：不发、NameColorChanged 保持。
        let in_palace = vec![CastleWarPlayer {
            actor_id: 7,
            map_id: 1,
            x: 10,
            y: 10,
            guild_name: Some("攻方".to_string()),
        }];
        assert!(castle_war_tick(&castles, &in_palace, &mut states, 4000).is_empty());
        assert!(states[&7].name_color_changed, "N1 跳过 NameColorChanged 块");
    }

    // ---- A-22 P1：写盘快照（`castle-persist-world`，设计 §4 P1）----

    /// C-A22-1 的构造城：部署 `0/SabukW.txt` 语义（`defense_specs`：主门/三墙 + 12 弓手 + 12 守卫）
    /// + 非零资金/收入 + 主门 HP 1800 + 左墙已挂、右墙未挂 + 两个目录口径。
    fn save_castle() -> UserCastle {
        let mut c = UserCastle::new("沙巴克", config());
        let mut env = FakeEnv::full();
        c.initialize(&defense_specs(), &mut env);
        c.server_index = 0;
        c.map_owner_server_index = 0;
        c.total_gold = 123_456;
        c.today_income = 77;
        c.income_today = CastleDate::new(2026, 10, 21);
        c.war_date = Some(CastleDate::new(2026, 10, 21));
        c.change_date = Some("2026-10-20 19:00:00".to_string());
        c.config_dir = "0".to_string();
        c.ctor_config_dir = "Castle".to_string();
        c.main_door.as_mut().unwrap().hp = 1800;
        // 右墙未挂实体（造怪失败/回收后的形态）——oracle 此时**不写** `RightWallHP` 键。
        c.right_wall = None;
        c
    }

    /// C-A22-1（L）：快照逐字段 = `CastleConf.SaveConfig` 的写面。
    ///
    /// 牙：`total_gold/today_income` 互换 ⇒ 本用例红；`RightWall=None` 仍写 HP ⇒ 红；
    /// 未 `initialize` 的城不返回 `NreArchersNull` ⇒ [`castle_save_skip_semantics`] 红。
    #[test]
    fn castle_save_snapshot_fields() {
        let snap = save_castle().save_snapshot().unwrap();

        // [Setup]
        assert_eq!(snap.castle_name, "沙巴克");
        assert_eq!(snap.own_guild.as_deref(), Some(""));
        assert_eq!(snap.change_date.as_deref(), Some("2026-10-20 19:00:00"));
        assert_eq!(snap.war_date, Some(CastleDate::new(2026, 10, 21)));
        assert_eq!(snap.income_today, CastleDate::new(2026, 10, 21));
        assert_eq!(snap.total_gold, 123_456);
        assert_eq!(snap.today_income, 77);

        // [Defense]
        assert_eq!(
            snap.envir_list,
            vec!["0151".to_string(), "d701".to_string()]
        );
        assert_eq!(snap.map_name.as_deref(), Some("3"));
        assert_eq!(snap.home_map, "3");
        assert_eq!((snap.home_x, snap.home_y), (644, 290));
        assert_eq!((snap.war_range_x, snap.war_range_y), (100, 100));

        // 主门：配置面 name/x/y + 运行期 HP 1800（不是配置 HP 2000）+ MainDoorOpen。
        assert_eq!(
            snap.main_door,
            CastleSaveUnit {
                name: "MainDoor".to_string(),
                x: 672,
                y: 330,
                hp: Some(1800),
                open: Some(true),
            }
        );
        // 左墙已挂 ⇒ HP 有值；open 与墙无关。
        assert_eq!(
            snap.left_wall,
            CastleSaveUnit {
                name: "LeftWall".to_string(),
                x: 624,
                y: 278,
                hp: Some(2000),
                open: None,
            }
        );
        assert_eq!(snap.center_wall.hp, Some(2000));
        // 右墙未挂实体 ⇒ **不写 HP 键**（hp=None），名称/坐标仍来自配置面。
        assert_eq!(
            snap.right_wall,
            CastleSaveUnit {
                name: "RightWall".to_string(),
                x: 634,
                y: 271,
                hp: None,
                open: None,
            }
        );

        assert_eq!(snap.archers.len(), MAX_CASTLE_ARCHER);
        assert_eq!(snap.guards.len(), MAX_CASTLE_GUARD);
        for (i, a) in snap.archers.iter().enumerate() {
            assert_eq!(a.name, "弓箭手");
            assert_eq!((a.x, a.y), (i as i16, 100));
            assert_eq!(a.hp, Some(2000));
            assert_eq!(a.open, None);
        }
        for (i, g) in snap.guards.iter().enumerate() {
            assert_eq!(g.name, "守卫");
            assert_eq!((g.x, g.y), (i as i16, 200));
            assert_eq!(g.hp, Some(2000));
            assert_eq!(g.open, None);
        }

        // 路径口径（§1.6）：两个目录都进快照，且不相等。
        assert_eq!(snap.config_dir, "0");
        assert_eq!(snap.ctor_config_dir, "Castle");
    }

    /// C-A22-1 的跳过牙：未 `initialize`（只跑 ctor 的装载城）⇒ `NreArchersNull`。
    #[test]
    fn castle_save_skip_semantics() {
        // 未 `initialize`、`map_name=None`：oracle 的归属门放行（GetMapOfServerIndex(null)==0），
        // 随后在 `Archers[i].nX` NRE。
        assert_eq!(
            UserCastle::new("沙巴克", config()).save_snapshot(),
            Err(CastleSaveSkip::NreArchersNull)
        );
        // 即使手动塞了防御单元，只要没 `LoadConfig`（specs=None）就仍是 NRE。
        let mut no_specs = UserCastle::new("沙巴克", config());
        no_specs.main_door = Some(CastleUnit::default());
        assert_eq!(
            no_specs.save_snapshot(),
            Err(CastleSaveSkip::NreArchersNull)
        );

        // 归属门先于 NRE（`CastleConf.cs:105-107` 在 `:266` 之前）：map_name 有值且非本服。
        let mut foreign = UserCastle::new("沙巴克", config());
        foreign.map_name = Some("3".to_string());
        foreign.map_owner_server_index = 1;
        assert_eq!(
            foreign.save_snapshot(),
            Err(CastleSaveSkip::MapOwnerNotSelf)
        );
        // 本服（0==0）⇒ 继续走到 NRE。
        let mut owned = UserCastle::new("沙巴克", config());
        owned.map_name = Some("3".to_string());
        assert_eq!(owned.save_snapshot(), Err(CastleSaveSkip::NreArchersNull));
    }

    /// C-A22-2（L）：配置文件走 ctor 绑定目录、申请文件走运行期 `ConfigDir`。
    ///
    /// 牙：把两个目录字段合并成一个 ⇒ 两条路径相同 ⇒ 本用例红。
    #[test]
    fn castle_save_path_dirs() {
        let snap = save_castle().save_snapshot().unwrap();
        assert_eq!(snap.config_dir, "0");
        assert_eq!(snap.ctor_config_dir, "Castle");
        assert_ne!(snap.config_dir, snap.ctor_config_dir);

        // P2（envir）的拼接口径：`CastleConf.SaveConfig` 写 ctor 绑定的文件名；
        // `UserCastle.SaveAttackSabukWall` 写运行期 `ConfigDir`。
        let config_path = format!("Castle/{}/SabukW.txt", snap.ctor_config_dir);
        let attack_path = format!("Castle/{}/AttackSabukWall.txt", snap.config_dir);
        assert_eq!(config_path, "Castle/Castle/SabukW.txt");
        assert_eq!(attack_path, "Castle/0/AttackSabukWall.txt");
    }

    /// 弓手/守卫未挂实体时：名称/坐标仍写配置面，HP 无条件写 0（`CastleConf.cs:283-290`）。
    #[test]
    fn castle_save_unbound_archer_and_guard_write_zero_hp() {
        let mut c = save_castle();
        c.archers[3] = None;
        c.guards[5] = None;
        let snap = c.save_snapshot().unwrap();
        assert_eq!(
            snap.archers[3],
            CastleSaveUnit {
                name: "弓箭手".to_string(),
                x: 3,
                y: 100,
                hp: Some(0),
                open: None,
            }
        );
        assert_eq!(
            snap.guards[5],
            CastleSaveUnit {
                name: "守卫".to_string(),
                x: 5,
                y: 200,
                hp: Some(0),
                open: None,
            }
        );
    }

    /// `GetCastle` 的字段半（`UserCastle.cs:539-560`）：归属 + `ChangeDate`；本卡只做字段。
    #[test]
    fn get_castle_sets_change_date_and_ownership_fields() {
        let mut c = ready_castle();
        assert!(!c.is_master_guild(G_ATTACK));
        c.get_castle(G_ATTACK, "攻方", now(10, 0));
        assert_eq!(c.master_guild, Some(G_ATTACK));
        assert_eq!(c.own_guild.as_deref(), Some("攻方"));
        assert_eq!(c.change_date.as_deref(), Some("2026-10-13 10:00:00"));
        assert!(c.is_master_guild(G_ATTACK));
        assert!(c.is_member(G_ATTACK));
    }
}
