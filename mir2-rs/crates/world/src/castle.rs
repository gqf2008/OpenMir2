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
/// 只保留攻城状态机与门/墙状态机用到的字段；具体门/墙行为见 [`CastleUnit`] 的 `door_*`/`wall_*`
/// 方法（`CastleDoor.cs` / `WallStructure.cs`，⑧-8）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CastleUnit {
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

/// `HUtil32.CompareLStr(src, targ)`（`src/OpenMir2/HUtil32.cs`）：大小写不敏感的前缀匹配。
///
/// `targ` 为空 ⇒ `false`；`src` 短于 `targ` ⇒ `false`；否则逐字符 `ToUpper` 比较前 `targ.len()` 个字符。
#[must_use]
pub fn compare_lstr(src: &str, targ: &str) -> bool {
    if targ.is_empty() || src.len() < targ.len() {
        return false;
    }
    src[..targ.len()].eq_ignore_ascii_case(targ)
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
                // `:373` MainDoorControl(true)（幂等关闭主门；⑧-8 起带 `CastleDoor.Close` 帧/掩码）。
                events.extend(self.main_door_control(true));
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
        // 事件顺序 = oracle :366-373（⑧-8 起 `MainDoorControl(true)` 带 `CastleDoor.Close` 帧/掩码）。
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
                // `SetMapXyFlag(1)`：`nFlag != 1` 为 false ⇒ 三格（`:25-27`）先 true、
                // 九格（`:29-37`）全 false（9 个不重复格）。
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
        // `Die()` 本身仍 `SetMapXyFlag(2)`（`:78-82`）。
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
        // `:94-102`：未开门且 `n08 < 3` 才刷 `RM_TURN`。
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

        // 关门：9 个不重复格全 `LowWall`（`:29-37`）。
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
        // `UnderWar` ⇒ 拒（`:920-923`）。
        c.under_war = true;
        assert_eq!(c.repair_door(999_999), (false, Vec::new()));
        c.under_war = false;
        // 60s 门：`>` 严格大于（`:951`）。
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
        // 已满 ⇒ 拒（`:923`），不产生帧。
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
        // `nWallIndex` 1/2/3 ⇒ 左/中/右；其它 ⇒ `Wall = null`（`:958-992`）。
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
}
