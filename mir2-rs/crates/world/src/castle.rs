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

/// `UserCastle.Archers = new ArcherUnit[12]`（`UserCastle.cs:48`）与 `CastleConst.MaxCastleArcher`
/// （`CastleConst.cs:10`）。`Guards` 数组长度同为 12（`UserCastle.cs:49`），
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
    /// `[Defense] CastleMap`（`CastleConf.cs:39`，缺省 `"3"`）。
    pub map_name: String,
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
        // `LoadConfig()`：`MapName = ReadWriteString("Defense", "CastleMap", "3")`（`CastleConf.cs:39`）。
        self.map_name = Some(specs.map_name.clone());
        let mut events = Vec::new();
        let map_name = specs.map_name.as_str();

        // `UserCastle.cs:113`：`if (SystemShare.MapMgr.GetMapOfServerIndex(MapName) == M2Share.ServerIndex)`。
        if !env.map_owner_is_self(map_name) {
            return events;
        }

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
            map_name: "3".to_string(),
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
    /// 牙：去掉 `hp==0` 跳过 ⇒ 26/红；把守卫默认 2000 改 0 ⇒ 16/红；调换墙序 ⇒ 名字序红。
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
}
