//! **Lane B — 城堡实体挂图宿主接线**（`bins/game-svr`）。
//!
//! oracle 调用点：`GameApp.Initialize` 的 `SystemShare.CastleMgr.Initialize()`
//! （`src/GameSrv/GameApp.cs:213`）——在 `LoadMapQuest`（`:184`）之后、`LoadMerchant`（`:227`）
//! 之前。`CastleManager.Initialize`（`CastleManager.cs:62-88`）在进程启动时建一座
//! `new UserCastle(SystemShare.Config.CastleDir)`（ctor `ConfigDir = "Castle"`），
//! 读 `Castle/Castle/SabukW.txt`，调 `castle.Initialize()`（**实体在这里造**），
//! **之后**才把 `ConfigDir` 改成 `"0"`（`:69`）⇒ 部署实读 `Castle/Castle/SabukW.txt`，
//! 不是夹具目录 `Castle/0`。
//!
//! 本模块做三件事（与简报 §5.1 一致）：
//!
//! 1. [`specs_from_sabuk`]：`crates/envir` 的 `SabukWConfig` → [`CastleDefenseSpecs`]
//!    （`UserCastle.Initialize` 的输入子集；`MainDoor.nStatus = MainDoorOpen`）；
//! 2. [`WorldCastleEnv`]：`CastleInitializeEnv` 的生产实现——地图存在性走 `MapRegistry`，
//!    按名造怪复用 [`crate::guardlist::regen_monster_by_name`]（= oracle `RegenMonsterByName`，
//!    同一条 `CreateMonster` 取数链：构造链 4 抽 → `Dir` → 掉落预算 → `CoolEye` → 地形移位），
//!    随机源由调用方传入**同一条** `world.rng`；
//! 3. [`initialize_castle`]：调 `UserCastle::initialize`、把城堡推入 `World.castles`、按
//!    `CastleEvent::SpawnDefenseUnit` 绑定实体（门/墙/弓手；守卫不绑），并把启动期
//!    `SetMapXyFlag` 事件（`MainDoorOpen=1` 的 `Open()`）按序作用到地图格子。
//!
//! ## 登记的建模偏差（未超写集，不修）
//!
//! - **`GetMapOfServerIndex`**（已接线，2026-10-21）：改读 `MapRegistry::entries()` 的
//!   `MapInfoEntry.server_index`（`MapManager.cs:169-180`：找不到 ⇒ 0），再与
//!   `M2Share.ServerIndex` 比较；单服部署两者同为 0。
//! - **`MonsterThreadMap`**：`UserCastle.Initialize` 的 `RegenMonsterByName` 末尾
//!   `GetMonsterThreadId` 需要 `InitializeMonster` 建好的线程表；本宿主要求 `MonGenHost` 已就绪
//!   （无 `Envir/MonGen.txt` ⇒ 不接城堡块，部署件有该文件）。
//! - **`AntiPoison=200` / `CellType=Door`**：`Entity` 无对应字段（登记，见
//!   `crates/world/src/mongen.rs` 的 class-constant 注释）。
//!
//! ## RNG 账本
//!
//! 成功造怪 = **6 抽**（`BaseObject` ctor 2 + `AnimalObject` ctor 2 + `Dir = RandomByte(8)` +
//! `Random(100) < CoolEyeCode`；无 `MonItems` 时掉落 0 抽）；`nHP<=0` 跳过 / 怪物库无此名
//! = **0 抽**（`race = -1` 在任何构造取数之前返回）。判据见本文件 `tests`。

use std::path::Path;

use mir2_data::exps::IniFile;
use mir2_data::guild_castle::GuildCastleConfig;
use mir2_envir::castle::{
    attack_sabuk_wall_path, castle_list_path, parse_ordered, sabuk_w_path, write_attack_sabuk_wall,
    write_castle_list, write_sabuk_w, AttackSabukWallEntry, DefenseUnit, IniOrdered,
    MainDoorOverlay, SabukWConfig, SabukWDefaults, SabukWOverlay, SabukWUnitOverlay,
};
use mir2_envir::{MapInfoFlag, MapLoader, MapRegistry};
use mir2_shared::rng::RandomNumber;
use mir2_world::castle::{
    area_state, CastleDate, CastleDefenseSpecs, CastleEvent, CastleNameColors, CastleSaveSkip,
    CastleSaveSnapshot, CastleSaveUnit, CastleSiegeConfig, DefenseCreature, DefenseUnitSpec,
    RedDieHome, UserCastle,
};
use mir2_world::mongen::MonGenState;

use crate::downlink;
use crate::mongen::{MonsterTable, WorldMonsterSink};

/// `ArcherUnit` 配置 → `UserCastle.Initialize` 的输入。
#[must_use]
pub fn spec_from_unit(unit: &DefenseUnit, status: bool) -> DefenseUnitSpec {
    DefenseUnitSpec {
        name: unit.name.clone(),
        x: unit.x,
        y: unit.y,
        status,
        hp: unit.hp,
    }
}

/// `CastleConf.LoadConfig` 的读侧 → [`CastleDefenseSpecs`]（逐字段，不改名/不重排）。
///
/// **墙名中/右互换照抄**（`Castle/0/SabukW.txt` 的 `CenterWall` 配「沙巴克右城墙」、
/// `RightWall` 配「沙巴克中城墙」）——本函数只搬运，不做语义纠正（硬约束③）。
#[must_use]
pub fn specs_from_sabuk(cfg: &SabukWConfig) -> CastleDefenseSpecs {
    CastleDefenseSpecs {
        own_guild: cfg.own_guild.clone(),
        map_name: cfg.map_name.clone(),
        home_map: cfg.home_map.clone(),
        home_x: cfg.home_x as i16,
        home_y: cfg.home_y as i16,
        war_range_x: cfg.war_range_x,
        war_range_y: cfg.war_range_y,
        envir_list: cfg.envir_list.clone(),
        palace_map: cfg.palace_map.clone(),
        secret_map: cfg.secret_map.clone(),
        main_door: spec_from_unit(&cfg.main_door, cfg.main_door_open),
        left_wall: spec_from_unit(&cfg.left_wall, false),
        center_wall: spec_from_unit(&cfg.center_wall, false),
        right_wall: spec_from_unit(&cfg.right_wall, false),
        archers: cfg
            .archers
            .iter()
            .map(|u| spec_from_unit(u, false))
            .collect(),
        guards: cfg
            .guards
            .iter()
            .map(|u| spec_from_unit(u, false))
            .collect(),
    }
}

/// 读部署实读路径 `Castle/<CastleDir>/SabukW.txt`（**不是** `Castle/0`）。
///
/// oracle `UserCastle` ctor：`Path.Combine(BasePath, Config.CastleDir, ConfigDir)`，
/// 而 `CastleManager.Initialize` 传进来的 `ConfigDir` 正是 `Config.CastleDir`
/// （`CastleManager.cs:66` 与 `UserCastle.cs:34/50`）⇒ 两级同名目录。
///
/// # Errors
/// 文件读取失败（oracle 此时会 `ConfigFile` 建空文件 + 全默认；本仓按 fail-closed 返回错误，
/// 由调用方记 WRN 并跳过城堡块——部署件存在，不触发）。
pub fn load_sabuk_specs(
    loader: &mir2_envir::MapLoader,
    castle_dir: &str,
) -> Result<CastleDefenseSpecs, String> {
    load_sabuk_config(loader, castle_dir).map(|(_, specs)| specs)
}

/// `CastleInitializeEnv` 的生产实现（同一条 `world.rng`，逐次同步造怪）。
pub struct WorldCastleEnv<'a> {
    pub world: &'a mut mir2_world::World,
    pub registry: &'a MapRegistry,
    pub table: &'a MonsterTable,
    pub state: &'a mut MonGenState,
    pub displays: &'a mut downlink::DisplayTable,
    pub rng: &'a mut RandomNumber,
    /// `M2Share.ServerIndex`（`[Server] ServerIndex`）。
    pub server_index: i32,
    pub warns: Vec<String>,
}

impl mir2_world::castle::CastleInitializeEnv for WorldCastleEnv<'_> {
    fn map_owner_is_self(&self, map_name: &str) -> bool {
        // `MapManager.GetMapOfServerIndex`（`MapManager.cs:169-180`）：
        // 找不到（或空名）⇒ 0；否则该图 `ServerIndex`。再与 `M2Share.ServerIndex` 比。
        let owner = self
            .registry
            .entries()
            .iter()
            .find(|e| e.map_name.eq_ignore_ascii_case(map_name))
            .map_or(0, |e| e.server_index);
        i32::from(owner) == self.server_index
    }

    fn find_map(&self, map_name: &str) -> bool {
        self.registry.id_of(map_name).is_some()
    }

    fn map_id(&self, map_name: &str) -> Option<usize> {
        self.registry.id_of(map_name)
    }

    fn regen_monster_by_name(
        &mut self,
        map_name: &str,
        x: i16,
        y: i16,
        name: &str,
    ) -> Option<DefenseCreature> {
        let mut sink = WorldMonsterSink {
            world: self.world,
            registry: self.registry,
            table: self.table,
            displays: self.displays,
        };
        let actor_id = crate::guardlist::regen_monster_by_name(
            self.rng, &mut sink, self.table, self.state, map_name, x, y, name,
        )?;
        let max_hp = self
            .world
            .entities
            .get(actor_id)
            .map_or(0, |e| i32::from(e.max_hp));
        Some(DefenseCreature { max_hp, actor_id })
    }

    fn warn(&mut self, text: String) {
        self.warns.push(text);
    }
}

/// [`initialize_castle`] 的读数（日志/判据）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CastleStartupReport {
    /// 成功挂图的防御单元数（= `CastleEvent::SpawnDefenseUnit` 条数）。
    pub spawned: usize,
    /// `LogService.Warn` 文案（保序）。
    pub warns: Vec<String>,
    /// 全部 [`CastleEvent`]（含 `SpawnDefenseUnit` 与启动期 `Structure`）。
    pub events: Vec<CastleEvent>,
}

/// oracle `CastleMgr.Initialize` 的实体半：建城 → `Initialize` → 入 `World.castles` → 绑实体。
///
/// **调用序**（宿主必须照做）：`LoadMapQuest` 之后、`LoadMerchant` 之前。
///
/// 随机源必须是 `world.rng`（调用方按 `step_mon_gen` 同款 `mem::replace` 移出/还回）。
#[allow(clippy::too_many_arguments)]
pub fn initialize_castle(
    world: &mut mir2_world::World,
    registry: &MapRegistry,
    table: &MonsterTable,
    state: &mut MonGenState,
    displays: &mut downlink::DisplayTable,
    rng: &mut RandomNumber,
    specs: &CastleDefenseSpecs,
    siege: CastleSiegeConfig,
    server_index: i32,
) -> CastleStartupReport {
    let castle_index = world.castles.len();
    let mut castle = UserCastle::new(siege.castle_name.clone(), siege);
    let (events, warns) = {
        let mut env = WorldCastleEnv {
            world,
            registry,
            table,
            state,
            displays,
            rng,
            server_index,
            warns: Vec::new(),
        };
        let events = castle.initialize(specs, &mut env);
        (events, env.warns)
    };
    world.castles.push(castle);
    let mut spawned = 0;
    let map_id = registry.id_of(&specs.map_name);
    for event in &events {
        match event {
            CastleEvent::SpawnDefenseUnit(spawn) => {
                spawned += 1;
                world.bind_castle_defense(castle_index, spawn);
            }
            // 启动期的 `SetMapXyFlag`（`MainDoorOpen=1` 的 `Open()`）——按序落到地图。
            // `Ref*` 此时无在线玩家，`SendRefMsg` 无收件人 ⇒ 不出帧（oracle 同）。
            CastleEvent::Structure(ev) => {
                if let (
                    Some(map_id),
                    mir2_world::castle::StructureEvent::SetMapXyFlag { x, y, walk },
                ) = (map_id, *ev)
                {
                    if let Some(grid) = world.maps.get_mut(map_id) {
                        grid.set_map_xy_flag(i32::from(x), i32::from(y), walk);
                    }
                }
            }
            _ => {}
        }
    }
    CastleStartupReport {
        spawned,
        warns,
        events,
    }
}

/// `GuildCastleConfig` 的 `[Setup]` 子集 → 攻城状态机配置。
#[must_use]
pub fn siege_config_from(cfg: &GuildCastleConfig) -> CastleSiegeConfig {
    CastleSiegeConfig::from_guild_castle_config(cfg)
}

/// `RefUserState` 的 `n8`（`PlayObject.Base.cs:270-286`）——生产帧 766 的输入。
///
/// 只做位累加（`|=`）；**同一 bit 重复置位幂等**（牙：`|=` 改 `^=` ⇒ 重复置位用例红）。
#[must_use]
pub fn area_state_with(flag: &MapInfoFlag, in_guild_war_area: bool) -> u16 {
    area_state(flag.fight_zone, flag.safe_area, in_guild_war_area)
}

/// `GetChrColor` 的生产接线（名字色）：观察者 × 目标的只读快照 → 名字色字节。
///
/// 行会关系只按**会名相等**判同会（`GetGuildRelation` 的 `1`）；盟友/宣战列表需要
/// `GuildId` 映射（本仓生产路径尚未物化）⇒ 传 0/`false`（登记 T-128）。
/// `GuildWarArea` 的按观察者副作用由返回值丢弃（调用方只取色）——查询用户名路径不需要它。
///
/// 颜色取 [`CastleNameColors::default()`]（47/180/69/221）：oracle 从 `Server.conf [Setup]`
/// 读 `PKFlagNameColor` 等四键（`ServerConf.cs:509-512`），本函数**不读配置**；部署件四值与
/// 源码缺省相同 ⇒ 当前等价（配置覆盖未接，登记 T-132）。
///
/// # Errors
/// 见 [`mir2_world::castle::ChrColorError`]（有行会观察者在攻城区 ⇒ oracle 抛 N1，
/// 本函数把失败交给调用方丢弃该帧）。**当前生产调用点**把 `viewer_in_guild_war_area`/
/// `target_in_guild_war_area` 硬传 `false` ⇒ N1 分支不可达（`Err` 丢帧是死代码镜像，登记 T-128）。
pub fn query_user_name_color(
    world: &mir2_world::World,
    viewer_guild: Option<&str>,
    target_guild: Option<&str>,
    target: &mir2_world::Entity,
    target_name_color: u8,
    viewer_in_guild_war_area: bool,
    target_in_guild_war_area: bool,
) -> Result<u8, mir2_world::castle::ChrColorError> {
    let castle = world
        .castle_war_area_at(Some(target.map_id), target.x, target.y)
        .and_then(|i| world.castles.get(i));
    let guild_relation = match (viewer_guild, target_guild) {
        (Some(a), Some(b)) if a == b => 1,
        _ => 0,
    };
    let input = mir2_world::castle::ChrColorInput {
        castle,
        target_name_color,
        target_pvp_level: mir2_world::World::pvp_level(target),
        target_pvp_flag: target.pvp_flag,
        target_fight3_zone: false,
        my_guild_name: viewer_guild,
        target_guild_name: target_guild,
        guild_relation,
        viewer_is_ally_of_target: false,
        viewer_in_guild_war_area,
        target_in_guild_war_area,
    };
    mir2_world::castle::get_chr_color(&input, &CastleNameColors::default()).map(|r| r.color)
}

/// `Server.conf` 的 `RedDieHomeMap/RedDieHomeX/RedDieHomeY`（`WorldServer.cs:343-348`）。
///
/// 读 `ConfigFile` 的 `[Setup]` 段；缺文件/缺键回落 `GameSvrConf.cs:1380-1382` 的
/// `"3"/839/668`（部署件 `E:/MirServer/M2GameSvr/Server.conf:413-415` 为 `3/838/668`）。
#[must_use]
pub fn load_red_die_home(server_conf: &Path) -> RedDieHome {
    let defaults = RedDieHome::default();
    let Ok(bytes) = std::fs::read(server_conf) else {
        return defaults;
    };
    let ini = IniFile::from_bytes(&bytes);
    let map_name = ini
        .read_string("Setup", "RedDieHomeMap", &defaults.map_name)
        .to_string();
    let x = ini.read_integer("Setup", "RedDieHomeX", i32::from(defaults.x));
    let y = ini.read_integer("Setup", "RedDieHomeY", i32::from(defaults.y));
    RedDieHome {
        map_name,
        x: x as i16,
        y: y as i16,
    }
}

// ===========================================================================
// A-22 P3：宿主排水与启动装载（`CastleManager` / `UserCastle` 的落盘半）
// ===========================================================================
//
// oracle 事实（逐行锚，冻结 `src/`）：
// - 启动落盘：`CastleManager.Initialize`（`CastleManager.cs:62-85`）⇒ 建默认城 → `ConfigDir="0"`
//   → `EnvirList.Add("0151".."0156")` → `Save()`（`:77`）；
// - `CastleManager.Save`（`CastleManager.cs:144-152`）写序 = `SaveCastleList`（下标行 + CRLF）→
//   按 `CastleList` 下标升序逐城 `castle.Save()`（`SaveConfigFile` → `SaveAttackSabukWall`，
//   `UserCastle.cs:448-452`）；
// - 惰性城：`LoadCastleList`（`CastleManager.cs:154-176`）只 `new UserCastle(dir)`（只跑 ctor，
//   不跑 `Initialize`）⇒ `MapName=null`、`Archers[i]=null` ⇒ 其 `SaveConfig` 必 NRE
//   （`CastleConf.cs:266-269`）；
// - 申请表读 ctor 路径、写运行期 `ConfigDir`（`UserCastle.cs:251/301`，设计 §1.6）；
// - 周期落盘：`SystemProcessor.ShowOnline`（`SystemProcessor.cs:54-62`）的门是
//   `GetTickCount()-ShowOnlineTick > ConsoleShowUserCountTime`（严格 `>`，默认 600000ms，
//   `GameSvrConf.cs:1439`），与 10s 的 `ProcessGameRun`（`:70-77`）是**两个独立门**。

/// `CastleManager.Initialize` / `UserCastle` 构造函数取 `SystemShare.Config` 的六个值
/// （`GameSvrConf.cs:1782-1787`，部署 `Server.conf [Setup]` 可覆盖）。
///
/// - 默认城：`new UserCastle(Config.CastleDir)` 的 ctor 初值 + `CastleConf.LoadConfig` 的缺键默认；
/// - 惰性城：`LoadCastleList` 的 `new UserCastle(行)` 同样取这六个值（不读文件）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastleCtorDefaults {
    /// `[Setup] CastleName`（`UserCastle.cs:31`）。
    pub name: String,
    /// `[Setup] CastleHomeMap`（`UserCastle.cs:29`）。
    pub home_map: String,
    /// `[Setup] CastleHomeX`（`UserCastle.cs:30`）。
    pub home_x: i16,
    /// `[Setup] CastleHomeY`。
    pub home_y: i16,
    /// `[Setup] CastleWarRangeX`（`UserCastle.cs:44`）。
    pub war_range_x: i32,
    /// `[Setup] CastleWarRangeY`。
    pub war_range_y: i32,
}

impl CastleCtorDefaults {
    /// 从 `GuildCastleConfig`（`Server.conf [Setup]` 读侧）取六个默认值。
    #[must_use]
    pub fn from_config(cfg: &GuildCastleConfig) -> Self {
        Self {
            name: cfg.castle_name.clone(),
            home_map: cfg.castle_home_map.clone(),
            home_x: cfg.castle_home_x,
            home_y: cfg.castle_home_y,
            war_range_x: i32::from(cfg.castle_war_range_x),
            war_range_y: i32::from(cfg.castle_war_range_y),
        }
    }

    /// `CastleConf.LoadConfig` 缺键回填用的六个值（`GameSvrConf.cs:1782-1787`）。
    #[must_use]
    pub fn sabuk_defaults(&self) -> SabukWDefaults {
        SabukWDefaults {
            castle_name: self.name.clone(),
            home_map: self.home_map.clone(),
            home_x: i32::from(self.home_x),
            home_y: i32::from(self.home_y),
            war_range_x: self.war_range_x,
            war_range_y: self.war_range_y,
        }
    }
}

/// `CastleManager.Save` 的一次实际落盘（判据 C-A22-8 的写序）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastleWriteStep {
    /// `SaveCastleList`（`CastleManager.cs:178-192`）。
    CastleList,
    /// 第 `index` 座城的 `SaveConfig`（ctor 路径 `SabukW.txt`）。
    SabukW(usize),
    /// 第 `index` 座城的 `SaveAttackSabukWall`（运行期 `ConfigDir` 路径）。
    AttackSabukWall(usize),
}

/// 单文件落盘的结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastleFileOutcome {
    /// 已写。
    Written,
    /// 未 `LoadConfig`（装载城）：oracle `Archers[i].nX` NRE（`CastleConf.cs:266-269`），
    /// 该城**两文件都不写**。
    SkippedNre,
    /// `GetMapOfServerIndex(MapName) != ServerIndex`：`SaveConfig` 早退（`CastleConf.cs:104-107`），
    /// 但同一次 `Save()` 的 `SaveAttackSabukWall` 仍会执行。
    SkippedOwner,
    /// 未到该文件（前一步已失败/跳过）。
    NotAttempted,
    /// IO/序列化失败（oracle 会抛异常；本宿主记错并继续，见白名单残余）。
    Failed(String),
}

/// 一座城的一次 `castle.Save()`（`UserCastle.cs:448-452`）读数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastleSaveOutcome {
    /// `CastleList` 下标。
    pub index: usize,
    /// `SaveConfigFile` 的结果。
    pub sabuk: CastleFileOutcome,
    /// `SaveAttackSabukWall` 的结果。
    pub attack: CastleFileOutcome,
}

/// 一次 `CastleManager.Save`（`CastleManager.cs:144-152`）的读数。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CastleSaveReport {
    /// `SaveCastleList` 是否写成。
    pub list_written: bool,
    /// `SaveCastleList` 的错误（`None` = 成功）。
    pub list_error: Option<String>,
    /// 逐城结果（下标升序）。
    pub castles: Vec<CastleSaveOutcome>,
    /// 启动期 `LoadAttackSabukWall` 读到的**行数**（ctor 路径；oracle 此时 `GuildMgr` 为空 ⇒
    /// 全部 `continue`，`AttackWarList` 保持空，见 [`CastleStore::startup_attack_entries`]）。
    pub startup_attack_lines: usize,
    /// 启动期申请表读取失败（`None` = 成功/文件不存在）。
    pub startup_attack_error: Option<String>,
    /// `LoadCastleList` 装载的惰性城数（`None` = 列表文件缺失/读取失败）。
    pub loaded: Option<usize>,
    /// `LoadCastleList` 的错误。
    pub load_error: Option<String>,
}

/// 城堡文件宿主：路径口径 + 有序 ini base 缓存（每城一份）+ `CastleManager.Save` 等价物。
///
/// 只做 IO 与编排；「写什么值」由 `crates/world` 的 `UserCastle::save_snapshot` 定死（P1），
/// 「怎么写字节」由 `crates/envir` 的 `write_sabuk_w`/`write_attack_sabuk_wall`/
/// `write_castle_list` 定死（P2）。
pub struct CastleStore {
    /// `MapLoader`（`base_path` + 读侧加载器；`CastleManager` 的所有路径都从 `BasePath` 拼）。
    pub loader: MapLoader,
    /// `[Share] CastleDir`（部署 `Castle`）。
    pub castle_dir: String,
    /// `[Share] CastleFile`（部署 `Castle/List.txt`）。
    pub castle_file: String,
    /// `M2Share.ServerIndex`（`CastleConf.SaveConfig` 的归属门右值）。
    pub server_index: i32,
    /// ctor 配置面默认值。
    pub ctor: CastleCtorDefaults,
    /// `CastleConf.LoadConfig` 缺键回填默认值。
    pub defaults: SabukWDefaults,
    /// 每座城 ctor 时读入的有序 ini 缓存（`ConfigFile.Load` 口径），下标对齐 `World.castles`。
    pub bases: Vec<IniOrdered>,
}

impl CastleStore {
    /// 由同一份 `Server.conf` 的 `GuildCastleConfig` 建宿主。
    #[must_use]
    pub fn from_loader(loader: &MapLoader, cfg: &GuildCastleConfig, server_index: i32) -> Self {
        let ctor = CastleCtorDefaults::from_config(cfg);
        Self {
            loader: loader.clone(),
            castle_dir: cfg.castle_dir.clone(),
            castle_file: cfg.castle_file.clone(),
            server_index,
            defaults: ctor.sabuk_defaults(),
            ctor,
            bases: Vec::new(),
        }
    }

    /// 追加一份 base（下标对齐 `World.castles` 的推入顺序）。
    pub fn push_base(&mut self, base: IniOrdered) {
        self.bases.push(base);
    }

    /// `ConfigFile.Load`（`ConfigFile.cs:256-261`）：读 `Castle/<config_dir>/SabukW.txt` 的有序缓存；
    /// 文件不存在 ⇒ **建空文件**（oracle ctor 的 `File.Create` 副作用）并返回空缓存。
    ///
    /// # Errors
    /// 建目录/建文件/读文件失败。
    pub fn load_base(&self, config_dir: &str) -> Result<IniOrdered, String> {
        let path = sabuk_w_path(&self.loader.base_path, &self.castle_dir, config_dir);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(parse_ordered(&bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("建立目录 {} 失败：{e}", parent.display()))?;
                    }
                }
                // `File.Create(FileName).Close()`：0 字节文件（部署 `1/SabukW.txt` 的来历）。
                std::fs::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&path)
                    .map_err(|e| format!("建立 {} 失败：{e}", path.display()))?;
                Ok(IniOrdered::new())
            }
            Err(e) => Err(format!("读取 {} 失败：{e}", path.display())),
        }
    }

    /// `CastleManager.SaveCastleList`（`CastleManager.cs:178-192`）：先建 `CastleDir`，
    /// 再写 `0..count` 下标行（CRLF）。文件不存在 ⇒ `StringList.SaveToFile` 抛
    /// `FileNotFoundException`（`StringList.cs:260-271`）。
    ///
    /// # Errors
    /// 建目录/写文件失败。
    pub fn save_castle_list(&self, count: usize) -> Result<(), String> {
        let path = castle_list_path(&self.loader.base_path, &self.castle_file);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("建立目录 {} 失败：{e}", parent.display()))?;
            }
        }
        write_castle_list(&path, count).map_err(|e| format!("写 {} 失败：{e}", path.display()))
    }

    /// `CastleManager.LoadCastleList`（`CastleManager.cs:154-176`）的读：`File.Exists` 为假 ⇒ `None`。
    ///
    /// # Errors
    /// 文件存在但读取失败。
    pub fn load_castle_list(&self) -> Result<Option<Vec<String>>, String> {
        self.loader
            .load_castle_list(&self.castle_file)
            .map_err(|e| format!("读取城堡列表失败：{e}"))
    }

    /// `UserCastle.LoadAttackSabukWall`（`UserCastle.cs:242-288`）的语法层：读
    /// `Castle/<config_dir>/AttackSabukWall.txt`；文件不存在 ⇒ 空表（oracle 直接 `return`）。
    ///
    /// 行会过滤（`GuildMgr.FindGuild`）不在本函数（envir 读侧同口径），见
    /// [`Self::startup_attack_entries`] 的相位说明。
    ///
    /// # Errors
    /// 文件存在但读取失败。
    pub fn load_attack_entries(
        &self,
        config_dir: &str,
    ) -> Result<Vec<AttackSabukWallEntry>, String> {
        self.loader
            .load_attack_sabuk_wall(&self.castle_dir, config_dir)
            .map(Option::unwrap_or_default)
            .map_err(|e| format!("读取申请表失败：{e}"))
    }

    /// 启动期 `LoadAttackSabukWall` 读的是 **ctor 路径**（`UserCastle.cs:50-55/251`）——
    /// 此时 `CastleManager.Initialize` 尚未把 `ConfigDir` 改成 `"0"`（`CastleManager.cs:69`）。
    ///
    /// **相位（T-131 同源）**：oracle 在 `GameApp.cs:213` 调 `CastleMgr.Initialize`，
    /// `GuildMgr.LoadGuildInfo` 在 `:245` ⇒ 行会表为空 ⇒ `FindGuild` 恒 `null` ⇒ 读到的**每一行**
    /// 都被 `continue` 丢弃，`AttackWarList` 保持空（申请文件存在与否都读不回来，设计 §1.6）。
    /// 因此本函数只复刻「读哪个文件、读了几行」的 IO/路径语义；世界侧 `attack_war_list` 不填充。
    ///
    /// # Errors
    /// 文件存在但读取失败。
    pub fn startup_attack_entries(
        &self,
        ctor_config_dir: &str,
    ) -> Result<Vec<AttackSabukWallEntry>, String> {
        self.load_attack_entries(ctor_config_dir)
    }

    /// `UserCastle.SaveAttackSabukWall`（`UserCastle.cs:291-309`）：写
    /// `Castle/<castle.config_dir>/AttackSabukWall.txt`（运行期 `ConfigDir`，设计 §1.6 的读/写不同源）。
    ///
    /// # Errors
    /// 文件不存在（`StringList.SaveToFile` 不建文件）或写入失败。
    pub fn save_attack(&self, castle: &UserCastle) -> Result<(), String> {
        let path =
            attack_sabuk_wall_path(&self.loader.base_path, &self.castle_dir, &castle.config_dir);
        write_attack_sabuk_wall(&path, &attack_entries(castle))
            .map_err(|e| format!("写 {} 失败：{e}", path.display()))
    }
}

/// `DateTime.ToString()` 的日期串（已登记偏差：oracle 是 culture 相关全时间戳，Rust 保留
/// `yyyy-MM-dd HH:mm:ss` 形状；逐字节由窗口 W-A22-1/W-A22-3 裁决）。
fn format_attack_date(date: CastleDate) -> String {
    format!(
        "{:04}-{:02}-{:02} 00:00:00",
        date.year, date.month, date.day
    )
}

/// 世界申请表的落盘形态（`SaveAttackSabukWall` 的 `sGuildName + "       \"" + AttackDate + '"'`，
/// `UserCastle.cs:303-308`）。
#[must_use]
pub fn attack_entries(castle: &UserCastle) -> Vec<AttackSabukWallEntry> {
    castle
        .attack_war_list
        .iter()
        .map(|a| AttackSabukWallEntry {
            guild_name: a.guild_name.clone(),
            attack_date: format_attack_date(a.attack_date),
            attack_date_parsed: true,
        })
        .collect()
}

/// `CastleSaveSnapshot` → `SabukWOverlay`（`CastleConf.SaveConfig` 的逐键写条件由 P2 的
/// `apply_overlay` 按 oracle 再判一次；这里只如实搬运原值）。
#[must_use]
pub fn overlay_from_snapshot(snap: &CastleSaveSnapshot) -> SabukWOverlay {
    SabukWOverlay {
        castle_name: snap.castle_name.clone(),
        own_guild: snap.own_guild.clone().unwrap_or_default(),
        change_date: snap.change_date.clone().unwrap_or_default(),
        war_date: snap.war_date.clone().unwrap_or_default(),
        income_today: snap.income_today.clone().unwrap_or_default(),
        total_gold: snap.total_gold,
        today_income: snap.today_income,
        envir_list: snap.envir_list.clone(),
        // `if (MainDoor.BaseObject != null) { WriteBool(MainDoorOpen, nStatus); WriteInteger(MainDoorHP, HP) }`
        // （`CastleConf.cs:203-207`）——快照里 `open` 只在实体存在时有值。
        main_door: MainDoorOverlay {
            present: snap.main_door.open.is_some(),
            hp: snap.main_door.hp.unwrap_or(0),
        },
        walls: [snap.left_wall.hp, snap.center_wall.hp, snap.right_wall.hp],
        archers: std::array::from_fn(|i| snapshot_unit(snap.archers.get(i))),
        guards: std::array::from_fn(|i| snapshot_unit(snap.guards.get(i))),
    }
}

fn snapshot_unit(unit: Option<&CastleSaveUnit>) -> SabukWUnitOverlay {
    unit.map_or_else(SabukWUnitOverlay::default, |u| SabukWUnitOverlay {
        name: u.name.clone(),
        x: i32::from(u.x),
        y: i32::from(u.y),
        // 弓手/守卫 `HP` 无条件写（无实体写 0，`CastleConf.cs:283-290/309-316`）。
        hp: u.hp.unwrap_or(0),
    })
}

/// 一座城的 `castle.Save()`（`UserCastle.cs:448-452`）：先 `SaveConfigFile`，再
/// `SaveAttackSabukWall`；NRE 城两文件都不写。
fn save_one(
    store: &CastleStore,
    index: usize,
    castle: &UserCastle,
    on_step: &mut dyn FnMut(CastleWriteStep),
) -> CastleSaveOutcome {
    let mut outcome = CastleSaveOutcome {
        index,
        sabuk: CastleFileOutcome::NotAttempted,
        attack: CastleFileOutcome::NotAttempted,
    };
    match castle.save_snapshot() {
        Ok(snap) => {
            let path = sabuk_w_path(
                &store.loader.base_path,
                &store.castle_dir,
                &snap.ctor_config_dir,
            );
            let base = store.bases.get(index).cloned().unwrap_or_default();
            match write_sabuk_w(&path, &base, &overlay_from_snapshot(&snap), &store.defaults) {
                Ok(()) => {
                    on_step(CastleWriteStep::SabukW(index));
                    outcome.sabuk = CastleFileOutcome::Written;
                }
                Err(e) => {
                    // oracle 的异常会中断整条 `CastleMgr.Save`；本宿主记错继续（白名单残余）。
                    outcome.sabuk =
                        CastleFileOutcome::Failed(format!("写 {} 失败：{e}", path.display()));
                    return outcome;
                }
            }
        }
        Err(CastleSaveSkip::MapOwnerNotSelf) => {
            // `SaveConfig` 早退；同一次 `Save()` 的申请表仍写（`UserCastle.cs:448-452`）。
            outcome.sabuk = CastleFileOutcome::SkippedOwner;
        }
        Err(CastleSaveSkip::NreArchersNull) => {
            // oracle `Archers[i].nX` NRE ⇒ `Save()` 在 `SaveConfigFile` 内抛出 ⇒ 申请表不写。
            outcome.sabuk = CastleFileOutcome::SkippedNre;
            return outcome;
        }
    }
    match store.save_attack(castle) {
        Ok(()) => {
            on_step(CastleWriteStep::AttackSabukWall(index));
            outcome.attack = CastleFileOutcome::Written;
        }
        Err(e) => outcome.attack = CastleFileOutcome::Failed(e),
    }
    outcome
}

/// `CastleManager.Save`（`CastleManager.cs:144-152`）等价物：`SaveCastleList` → 升序逐城
/// `[SaveConfig → SaveAttackSabukWall]`。写序用 `on_step` 暴露给判据/日志。
pub fn save_all_with(
    store: &CastleStore,
    castles: &[UserCastle],
    on_step: &mut dyn FnMut(CastleWriteStep),
) -> CastleSaveReport {
    let mut report = CastleSaveReport::default();
    match store.save_castle_list(castles.len()) {
        Ok(()) => {
            report.list_written = true;
            on_step(CastleWriteStep::CastleList);
        }
        Err(e) => report.list_error = Some(e),
    }
    for (index, castle) in castles.iter().enumerate() {
        let outcome = save_one(store, index, castle, on_step);
        report.castles.push(outcome);
    }
    report
}

/// [`save_all_with`] 的无观察者版本（生产路径）。
#[must_use]
pub fn save_all(store: &CastleStore, castles: &[UserCastle]) -> CastleSaveReport {
    save_all_with(store, castles, &mut |_| {})
}

/// `SystemProcessor.ShowOnline` 的 10 分钟门（`SystemProcessor.cs:56`）：
/// `GetTickCount() - ShowOnlineTick > ConsoleShowUserCountTime`（**严格 `>`**，命中后刷新基准）。
///
/// 与 10s 的 `ProcessGameRun` 门（`:70-77`）是两个独立门——把落盘塞进 10s 门会多写（C-A22-8 的牙）。
pub fn castle_save_due(clock_ms: i64, save_tick: &mut i64, interval_ms: i64) -> bool {
    if clock_ms - *save_tick > interval_ms {
        *save_tick = clock_ms;
        true
    } else {
        false
    }
}

/// `Server.conf [Setup] ConsoleShowUserCountTime`（`ServerConf.cs:586`）读侧；
/// 缺文件/缺键/非法值 ⇒ `GameSvrConf.cs:1439` 的 `10*60*1000`。
#[must_use]
pub fn load_console_show_user_count_time(server_conf: &Path) -> i64 {
    const DEFAULT: i32 = 10 * 60 * 1000;
    let Ok(bytes) = std::fs::read(server_conf) else {
        return i64::from(DEFAULT);
    };
    let ini = IniFile::from_bytes(&bytes);
    i64::from(ini.read_integer("Setup", "ConsoleShowUserCountTime", DEFAULT))
}

/// `DateTime.Parse` 的日期部分（`yyyy-MM-dd` / `yyyy/MM/dd`）；非法 ⇒ `None`
/// （oracle `ReadDateTime` 对非法串抛异常，见设计 §1.3 登记）。
#[must_use]
pub fn parse_castle_date(text: &str) -> Option<CastleDate> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let date = text.split_once([' ', 'T']).map_or(text, |(d, _)| d);
    let norm = date.replace('/', "-");
    let mut it = norm.split('-');
    let (Some(y), Some(m), Some(d), None) = (it.next(), it.next(), it.next(), it.next()) else {
        return None;
    };
    let (Ok(year), Ok(month), Ok(day)) = (y.parse::<i32>(), m.parse::<u32>(), d.parse::<u32>())
    else {
        return None;
    };
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(CastleDate::new(year, month, day))
}

/// `map_owner_server_index` 的生产取值：`MapMgr.GetMapOfServerIndex(MapName)`
/// （`MapManager.cs:169-180`；找不到/空名 ⇒ 0）。
fn owner_server_index(registry: Option<&MapRegistry>, map_name: &str) -> i32 {
    registry.map_or(0, |registry| {
        registry
            .entries()
            .iter()
            .find(|e| e.map_name.eq_ignore_ascii_case(map_name))
            .map_or(0, |e| i32::from(e.server_index))
    })
}

/// `CastleConf.LoadConfig`（`CastleConf.cs:15-94`）的宿主侧：把读侧 `SabukWConfig` 的
/// **落盘状态**灌进 P1 新增字段（`UserCastle.cs:17-23` 的 `TotalGold/TodayIncome/WarDate/IncomeToday`
/// 此前被丢弃，设计 §3.4 的实质缺口）。
///
/// 调用序 = oracle：`LoadConfig` →（归属门）→ `ConfigDir="0"` → `EnvirList.Add("0151".."0156")`。
/// `now_date` / `wall_clock` 是宿主注入的墙钟（`chrono::Local::now()`）：日期键缺键时 oracle 取
/// `DateTime.Now`（`CastleConf.cs:19-21`）。
pub fn hydrate_castle(
    castle: &mut UserCastle,
    cfg: &SabukWConfig,
    registry: Option<&MapRegistry>,
    castle_dir: &str,
    now_date: CastleDate,
    wall_clock: &str,
    server_index: i32,
) {
    // [Setup]（`CastleConf.cs:17-23`）
    castle.name = cfg.castle_name.clone();
    castle.own_guild = Some(cfg.own_guild.clone());
    castle.change_date = Some(
        cfg.change_date
            .clone()
            .unwrap_or_else(|| wall_clock.to_string()),
    );
    castle.war_date_text = Some(
        cfg.war_date
            .clone()
            .unwrap_or_else(|| wall_clock.to_string()),
    );
    castle.income_today_text = Some(
        cfg.income_today
            .clone()
            .unwrap_or_else(|| wall_clock.to_string()),
    );
    castle.war_date = Some(
        cfg.war_date
            .as_deref()
            .and_then(parse_castle_date)
            .unwrap_or(now_date),
    );
    castle.income_today = cfg
        .income_today
        .as_deref()
        .and_then(parse_castle_date)
        .unwrap_or(now_date);
    castle.total_gold = cfg.total_gold;
    castle.today_income = cfg.today_income;
    // `CastleManager.Initialize`（`CastleManager.cs:69`）：`ConfigDir="0"`；ctor 绑定目录仍是
    // `CastleDir`（`UserCastle.cs:50-55`）⇒ 配置文件写 `Castle/Castle/`、申请表写 `Castle/0/`。
    castle.config_dir = "0".to_string();
    castle.ctor_config_dir = castle_dir.to_string();
    castle.server_index = server_index;
    castle.map_owner_server_index = owner_server_index(registry, &cfg.map_name);
    // `CastleManager.cs:70-75`：六张攻城图**只加字符串**（不是 `FindMap`），不查重、不去尾。
    if let Some(specs) = castle.specs.as_mut() {
        for name in ["0151", "0152", "0153", "0154", "0155", "0156"] {
            specs.envir_list.push(name.to_string());
            if let Some(id) = registry.and_then(|r| r.id_of(name)) {
                castle.envir_map_ids.push(id);
            }
        }
    }
}

/// `CastleManager.LoadCastleList`（`CastleManager.cs:154-176`）：读 `List.txt`，逐行
/// `new UserCastle(行)`（**只跑 ctor**：`map_name=None`、`specs=None`、申请/配置目录 = 行）。
///
/// # Errors
/// 列表文件缺失（oracle 记 `城堡列表文件未找到!!!` 并返回）或 ctor 的 `CastleConf.Load` 失败。
pub fn load_castle_list_into(
    store: &mut CastleStore,
    castles: &mut Vec<UserCastle>,
    siege: &CastleSiegeConfig,
) -> Result<usize, String> {
    let Some(lines) = store.load_castle_list()? else {
        return Err("城堡列表文件未找到!!!".to_string());
    };
    for line in &lines {
        let mut castle = UserCastle::new(store.ctor.name.clone(), siege.clone());
        castle.home_map = store.ctor.home_map.clone();
        castle.home_x = store.ctor.home_x;
        castle.home_y = store.ctor.home_y;
        castle.war_range_x = store.ctor.war_range_x;
        castle.war_range_y = store.ctor.war_range_y;
        castle.config_dir = line.clone();
        castle.ctor_config_dir = line.clone();
        castle.server_index = store.server_index;
        // ctor 的 `CastleConf.Load`：文件缺失 ⇒ 建空文件（`ConfigFile.cs:256-261`）。
        let base = store.load_base(line)?;
        castles.push(castle);
        store.push_base(base);
    }
    Ok(lines.len())
}

/// 启动持久化总序（`GameApp.cs:213` `CastleMgr.Initialize` → `:246` `LoadCastleList`）：
/// ① 读 ctor 路径的申请表（读数见 [`CastleStore::startup_attack_entries`]）；
/// ② `CastleMgr.Save()`（`CastleManager.cs:77`）；
/// ③ `LoadCastleList` 装载惰性城。
///
/// `ctor_config_dir` 是默认城的 ctor 目录（部署 = `Castle`）。
pub fn startup_finish(
    store: &mut CastleStore,
    castles: &mut Vec<UserCastle>,
    siege: &CastleSiegeConfig,
    ctor_config_dir: &str,
) -> CastleSaveReport {
    let startup_attack = store.startup_attack_entries(ctor_config_dir);
    let mut report = save_all(store, castles);
    match startup_attack {
        Ok(entries) => report.startup_attack_lines = entries.len(),
        Err(e) => report.startup_attack_error = Some(e),
    }
    match load_castle_list_into(store, castles, siege) {
        Ok(loaded) => report.loaded = Some(loaded),
        Err(e) => report.load_error = Some(e),
    }
    report
}

/// `SabukW.txt` 的读侧 + `CastleDefenseSpecs` 适配（同一次 `new UserCastle` + `LoadConfig`）。
///
/// # Errors
/// `Castle/<CastleDir>/SabukW.txt` 读取失败（oracle 此时建空文件 + 全默认；本仓 fail-closed，
/// 部署件存在，不触发）。
pub fn load_sabuk_config(
    loader: &MapLoader,
    castle_dir: &str,
) -> Result<(SabukWConfig, CastleDefenseSpecs), String> {
    let cfg = loader
        .load_sabuk_w(castle_dir, castle_dir)
        .map_err(|e| format!("读取 {castle_dir}/{castle_dir}/SabukW.txt 失败：{e}"))?;
    let specs = specs_from_sabuk(&cfg);
    Ok((cfg, specs))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod persist_tests {
    //! A-22 P3 判据（设计 `docs/design-castle-persistence-20261021.md` §4 P3）：
    //! C-A22-8（10 分钟门 + 写序）、C-A22-9（申请表读/写路径不对称）、C-A22-10（启动恢复
    //! 资金/日期）、C-A22-11（启动形状：`Save` 截断 List.txt → 装载惰性城）、C-A22-12
    //! （装载城 NRE ⇒ `Castle/0/SabukW.txt` 字节不变）。
    //!
    //! 所有文件写入都落在 `E:/tmp/a22-p3-<pid>-<name>/`（任务约束：不写 `E:/MirServer`）；
    //! `E:/tmp` 不存在时回落到系统临时目录。每条用例的「牙」（改坏生产路径必红）写在函数文档里。

    use std::path::PathBuf;

    use super::*;
    use mir2_envir::castle::parse_sabuk_w_bytes;
    use mir2_world::castle::{CastleNow, GuildId};

    // ------------------------------------------------------------------
    // 夹具
    // ------------------------------------------------------------------

    /// 临时根（`E:/tmp/a22-p3-<pid>-<name>`；每次起跑先清空）。
    fn fixture_root(name: &str) -> PathBuf {
        let base = if Path::new("E:/tmp").is_dir() {
            PathBuf::from("E:/tmp")
        } else {
            std::env::temp_dir()
        };
        let dir = base.join(format!("a22-p3-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建立夹具目录");
        dir
    }

    fn write_file(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("建立父目录");
        }
        std::fs::write(path, bytes).expect("写夹具文件");
    }

    fn hex_bytes(hex: &str) -> Vec<u8> {
        let digits: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        assert_eq!(digits.len() % 2, 0, "hex 夹具长度必须为偶");
        digits
            .chunks(2)
            .map(|c| {
                let hi = (c[0] as char).to_digit(16).expect("hex 高位") as u8;
                let lo = (c[1] as char).to_digit(16).expect("hex 低位") as u8;
                (hi << 4) | lo
            })
            .collect()
    }

    /// 部署件 `Castle/Castle/SabukW.txt`（8870 B，sha `b84c2679…`；`CastleMapList` = 1332 项）。
    fn deployed_castle_sabuk() -> Vec<u8> {
        hex_bytes(include_str!(
            "../../../crates/envir/tests/fixtures/SabukW-Castle.hex"
        ))
    }

    /// 部署件 `Castle/0/SabukW.txt`（1748 B，sha `6021523b…`；装载城从不被运行期写过）。
    fn deployed_zero_sabuk() -> Vec<u8> {
        hex_bytes(include_str!(
            "../../../tests/parity/m3/evidence-m3-castle-file-load-20261013/fixtures/SabukW-0.hex"
        ))
    }

    fn test_cfg() -> GuildCastleConfig {
        GuildCastleConfig {
            castle_dir: "Castle".to_string(),
            castle_file: "Castle/List.txt".to_string(),
            ..GuildCastleConfig::default()
        }
    }

    fn store_for(root: &Path) -> CastleStore {
        let loader = MapLoader::new(root, "Map", "Envir");
        CastleStore::from_loader(&loader, &test_cfg(), 0)
    }

    /// 默认城：`initialize` 已跑（`specs=Some`），但无实体（门/墙 HP 不写、弓手/守卫写 0）。
    fn ready_castle(cfg: &SabukWConfig) -> UserCastle {
        let mut castle = UserCastle::new(cfg.castle_name.clone(), CastleSiegeConfig::default());
        castle.specs = Some(specs_from_sabuk(cfg));
        castle
    }

    fn hydrate_ready(castle: &mut UserCastle, cfg: &SabukWConfig) {
        hydrate_castle(
            castle,
            cfg,
            None,
            "Castle",
            CastleDate::new(2026, 10, 21),
            "2026-10-21 12:00:00",
            0,
        );
    }

    fn read_text(root: &Path, rel: &str) -> String {
        mir2_envir::text::decode_text(&std::fs::read(root.join(rel)).expect("读夹具文件"))
    }

    // ------------------------------------------------------------------
    // C-A22-8：10 分钟门（严格 `>`）+ 写序 List → 逐城 SabukW → 申请表
    // ------------------------------------------------------------------

    /// 门：10000/600000 不存、10001/600001 存；与 10s 门独立。
    ///
    /// 牙：`>` 改 `>=` ⇒ 600000 用例红；把间隔传成 10_000 ⇒ 10001/600000 用例红
    /// （= 「把 Save 塞进 10s 门」，假 tick 序列下会多写）。
    #[test]
    fn save_10min_gate_strictly_greater() {
        let interval = 600_000_i64;
        let mut tick = 0_i64;
        assert!(!castle_save_due(10_000, &mut tick, interval));
        assert!(!castle_save_due(600_000, &mut tick, interval));
        assert_eq!(tick, 0, "未命中门不得推进基准");
        assert!(castle_save_due(600_001, &mut tick, interval));
        assert_eq!(tick, 600_001);
        assert!(!castle_save_due(1_200_001, &mut tick, interval));
        assert!(castle_save_due(1_200_002, &mut tick, interval));
    }

    /// 接线口径：10s 的 `CastleMgr.Run` 门原样保留；10 分钟落盘走独立函数 + 配置值。
    ///
    /// 牙：把 `castle_save_due` 调用挪进 `> 10_000` 块（或把 10s 门常量改掉）⇒ 本用例红。
    #[test]
    fn lib_wiring_keeps_two_independent_gates() {
        let src = include_str!("lib.rs");
        assert!(
            src.contains("world.clock_ms - castle_run_tick > 10_000"),
            "10s CastleMgr.Run 门不得改动"
        );
        assert!(
            src.contains("game_svr::castle::castle_save_due("),
            "10 分钟落盘必须走独立门函数"
        );
        let call_at = src
            .find("game_svr::castle::castle_save_due(")
            .expect("落盘门调用点");
        assert!(
            src[call_at..call_at + 240].contains("enter_cfg.console_show_user_count_time"),
            "门间隔必须来自 Server.conf [Setup] ConsoleShowUserCountTime（塞进 10s 门 ⇒ 多写）"
        );
        assert!(
            src.contains("store.save_attack(&world.castles[idx])"),
            "SaveAttackSabukWall 事件必须接宿主落盘"
        );
        assert!(
            src.contains("let mut castle_save_tick = world.clock_ms;"),
            "10 分钟门基准必须独立于 castle_run_tick"
        );
    }

    /// 写序：`CastleManager.Save` = `SaveCastleList` → 城 0 `SaveConfig` → 城 0 申请表 →
    /// 城 1 `…`（`CastleManager.cs:144-152`、`UserCastle.cs:448-452`）。
    ///
    /// 牙：城序倒置 ⇒ `steps` 红；先写申请表再写配置 ⇒ 红；漏 `SaveCastleList` ⇒ 红。
    #[test]
    fn save_order_is_list_then_each_castle() {
        let root = fixture_root("save-order");
        write_file(&root, "Castle/List.txt", b"0\r\n1\r\n");
        write_file(&root, "Castle/Castle/SabukW.txt", &deployed_castle_sabuk());
        write_file(&root, "Castle/0/AttackSabukWall.txt", b"");
        write_file(&root, "Castle/1/AttackSabukWall.txt", b"");
        let cfg = parse_sabuk_w_bytes(&deployed_castle_sabuk());
        let mut store = store_for(&root);
        store.push_base(store.load_base("Castle").expect("部署 base"));
        let mut first = ready_castle(&cfg);
        hydrate_ready(&mut first, &cfg);
        let mut second = ready_castle(&cfg);
        second.config_dir = "1".to_string();
        second.ctor_config_dir = "1".to_string();
        store.push_base(store.load_base("1").expect("城 1 base"));
        let castles = vec![first, second];

        let mut steps = Vec::new();
        let report = save_all_with(&store, &castles, &mut |step| steps.push(step));
        assert!(report.list_written);
        assert_eq!(
            steps,
            vec![
                CastleWriteStep::CastleList,
                CastleWriteStep::SabukW(0),
                CastleWriteStep::AttackSabukWall(0),
                CastleWriteStep::SabukW(1),
                CastleWriteStep::AttackSabukWall(1),
            ]
        );
        assert!(report.castles.iter().all(
            |c| c.sabuk == CastleFileOutcome::Written && c.attack == CastleFileOutcome::Written
        ));
        assert_eq!(read_text(&root, "Castle/List.txt"), "0\r\n1\r\n");
        assert!(
            std::fs::metadata(root.join("Castle/1/SabukW.txt"))
                .unwrap()
                .len()
                > 0
        );
    }

    // ------------------------------------------------------------------
    // C-A22-9：申请表读 ctor 路径、写运行期 ConfigDir 路径
    // ------------------------------------------------------------------

    /// 夹具 `Castle/0/AttackSabukWall.txt` 1 行、`Castle/Castle/AttackSabukWall.txt` 不存在
    /// ⇒ 启动读 ctor 路径得 0 行；事件排水写 `0/` 路径（7 空格 + 引号 + CRLF）。
    ///
    /// 牙：把 `startup_attack_entries` 的实参改成运行期 `0` ⇒ 第一个断言得 1 ⇒ 红
    /// （读路径改成 `config_dir`）。
    #[test]
    fn attack_list_path_asymmetry() {
        let root = fixture_root("attack-path");
        let one_line = b"guildA       \"2026-10-21\"\r\n";
        write_file(&root, "Castle/0/AttackSabukWall.txt", one_line);
        let store = store_for(&root);
        // 读 ctor 路径：文件不存在 ⇒ 0 行（`UserCastle.cs:251-256`）。
        assert_eq!(
            store
                .startup_attack_entries("Castle")
                .expect("读 ctor 路径")
                .len(),
            0
        );
        // 同一时刻运行期目录里确实有 1 行 ⇒ 证明两条路径不同源。
        assert_eq!(store.load_attack_entries("0").expect("读 0 路径").len(), 1);
        // 反过来：写一份 ctor 路径的文件 ⇒ 启动读能看见（不是永远读 0/）。
        write_file(&root, "Castle/Castle/AttackSabukWall.txt", one_line);
        assert_eq!(
            store
                .startup_attack_entries("Castle")
                .expect("读 ctor 路径")
                .len(),
            1
        );

        // 事件排水：`CastleEvent::SaveAttackSabukWall` ⇒ 写 `Castle/<config_dir>/…`。
        let cfg = parse_sabuk_w_bytes(&deployed_zero_sabuk());
        let mut castle = ready_castle(&cfg);
        hydrate_ready(&mut castle, &cfg);
        let (added, events) = castle.add_attacker_info(
            GuildId(7),
            "攻方",
            CastleNow::new(CastleDate::new(2026, 10, 21), 12, 1_000),
        );
        assert!(added);
        assert!(events
            .iter()
            .any(|e| matches!(e, CastleEvent::SaveAttackSabukWall)));
        store.save_attack(&castle).expect("写申请表");
        assert_eq!(
            read_text(&root, "Castle/0/AttackSabukWall.txt"),
            "攻方       \"2026-10-21 00:00:00\"\r\n"
        );
    }

    // ------------------------------------------------------------------
    // C-A22-10：启动恢复资金/日期（设计 §3.4 的实质缺口）
    // ------------------------------------------------------------------

    /// 夹具 `TotalGold=123456`、`TodayIncome=77`、`WarDate=…` ⇒ 启动后世界字段一致，
    /// 回写后仍是 123456/77。
    ///
    /// 牙：不恢复（保持 ctor 的 0）⇒ 字段断言红；把 `total_gold/today_income` 互换 ⇒ 红；
    /// 把 `config_dir` 与 `ctor_config_dir` 合并 ⇒ 路径断言红。
    #[test]
    fn startup_restores_gold_and_dates() {
        let root = fixture_root("restore-gold");
        let base = b"[Setup]\r\nCastleName=Sabuk\r\nChangeDate=2026-10-09 07:10:32\r\nWarDate=2026-10-09 07:10:32\r\nIncomeToday=2026-10-21 01:04:01\r\nTotalGold=123456\r\nTodayIncome=77\r\n[Defense]\r\nCastleMap=3\r\nMainDoorX=672\r\nMainDoorY=330\r\nMainDoorName=MainDoor\r\nMainDoorHP=2000\r\n".to_vec();
        write_file(&root, "Castle/List.txt", b"0\r\n");
        write_file(&root, "Castle/Castle/SabukW.txt", &base);
        write_file(&root, "Castle/0/AttackSabukWall.txt", b"");
        let cfg = parse_sabuk_w_bytes(&base);
        let mut store = store_for(&root);
        store.push_base(store.load_base("Castle").expect("base"));
        let mut castle = ready_castle(&cfg);
        hydrate_ready(&mut castle, &cfg);

        assert_eq!(castle.name, "Sabuk", "CastleName 取文件而不是 Config");
        assert_eq!(castle.total_gold, 123456);
        assert_eq!(castle.today_income, 77);
        assert_eq!(castle.war_date_text.as_deref(), Some("2026-10-09 07:10:32"));
        assert_eq!(
            castle.income_today_text.as_deref(),
            Some("2026-10-21 01:04:01")
        );
        assert_eq!(castle.war_date, Some(CastleDate::new(2026, 10, 9)));
        assert_eq!(castle.income_today, CastleDate::new(2026, 10, 21));
        // 读/写不同源：配置文件写 ctor 目录、申请表写运行期目录。
        assert_eq!(castle.ctor_config_dir, "Castle");
        assert_eq!(castle.config_dir, "0");

        let report = save_all(&store, &castles_of(&castle));
        assert_eq!(report.castles[0].sabuk, CastleFileOutcome::Written);
        let written =
            parse_sabuk_w_bytes(&std::fs::read(root.join("Castle/Castle/SabukW.txt")).unwrap());
        assert_eq!(written.total_gold, 123456);
        assert_eq!(written.today_income, 77);
        assert_eq!(written.castle_name, "Sabuk");
        // 快照同值（回写不是只靠 base 里的旧值）。
        let snap = castle.save_snapshot().expect("默认城快照");
        assert_eq!((snap.total_gold, snap.today_income), (123456, 77));
    }

    fn castles_of(castle: &UserCastle) -> Vec<UserCastle> {
        vec![castle.clone()]
    }

    /// `ConsoleShowUserCountTime`：有键取值；缺键/非法/缺文件 ⇒ 600000（`GameSvrConf.cs:1439`）。
    ///
    /// 牙：默认值改成 0/其它 ⇒ 三条回落断言红。
    #[test]
    fn console_show_user_count_time_defaults_to_ten_minutes() {
        let root = fixture_root("conf");
        let conf = root.join("Server.conf");
        std::fs::write(&conf, b"[Setup]\r\nConsoleShowUserCountTime=1234\r\n").unwrap();
        assert_eq!(load_console_show_user_count_time(&conf), 1234);
        std::fs::write(&conf, b"[Setup]\r\n").unwrap();
        assert_eq!(load_console_show_user_count_time(&conf), 600_000);
        std::fs::write(&conf, b"[Setup]\r\nConsoleShowUserCountTime=abc\r\n").unwrap();
        assert_eq!(load_console_show_user_count_time(&conf), 600_000);
        assert_eq!(
            load_console_show_user_count_time(&root.join("missing.conf")),
            600_000
        );
    }

    // ------------------------------------------------------------------
    // C-A22-11 / C-A22-12：启动形状 + 装载城 NRE 不改字节
    // ------------------------------------------------------------------

    /// 部署形状：`List.txt` 2 行 ⇒ `Initialize` 的 Save 截成 1 行 ⇒ `LoadCastleList` 装载
    /// 1 个惰性城 ⇒ `world.castles.len()==2`；默认城 `CastleMapList` = 原值 + 6。
    ///
    /// 牙：把 `LoadCastleList` 提到 `Save` 之前 ⇒ 3 城红；对 `CastleMapList` 去重 ⇒ 项数红；
    /// 不追加 `0151..0156` ⇒ 项数红。
    #[test]
    fn startup_castle_shape_and_map_list_growth() {
        let root = fixture_root("startup-shape");
        write_file(&root, "Castle/List.txt", b"0\r\n1\r\n");
        write_file(&root, "Castle/Castle/SabukW.txt", &deployed_castle_sabuk());
        write_file(&root, "Castle/0/AttackSabukWall.txt", b"");
        write_file(&root, "Castle/0/SabukW.txt", &deployed_zero_sabuk());
        let cfg = parse_sabuk_w_bytes(&deployed_castle_sabuk());
        let envir_before = cfg.envir_list.len();
        assert_eq!(envir_before, 1332, "部署件 CastleMapList 项数（= 6 × 222）");
        let mut store = store_for(&root);
        store.push_base(store.load_base("Castle").expect("base"));
        let mut castle = ready_castle(&cfg);
        hydrate_ready(&mut castle, &cfg);
        let mut castles = vec![castle];

        let report = startup_finish(
            &mut store,
            &mut castles,
            &CastleSiegeConfig::default(),
            "Castle",
        );
        assert!(report.list_written, "Initialize 的 Save 必须写 List.txt");
        assert_eq!(report.startup_attack_lines, 0, "caster 路径申请表不存在");
        assert_eq!(report.loaded, Some(1));
        assert_eq!(castles.len(), 2, "默认 + 装载 \"0\"");
        assert_eq!(
            read_text(&root, "Castle/List.txt"),
            "0\r\n",
            "Initialize 的 Save 把 2 行截成 1 行"
        );
        assert_eq!(
            castles[0]
                .specs
                .as_ref()
                .expect("默认城 specs")
                .envir_list
                .len(),
            envir_before + 6
        );
        let written =
            parse_sabuk_w_bytes(&std::fs::read(root.join("Castle/Castle/SabukW.txt")).unwrap());
        assert_eq!(written.envir_list.len(), envir_before + 6);
        assert_eq!(
            &written.envir_list[written.envir_list.len() - 6..],
            ["0151", "0152", "0153", "0154", "0155", "0156"]
        );
        // 装载城：只跑 ctor（`map_name=None`、`specs=None`、目录 = 行）。
        let lazy = &castles[1];
        assert_eq!(lazy.map_name, None);
        assert!(lazy.specs.is_none());
        assert_eq!(lazy.config_dir, "0");
        assert_eq!(lazy.ctor_config_dir, "0");
        assert_eq!(lazy.name, "沙巴克", "ctor 取 Config.CastleName");
        assert_eq!(lazy.home_map, "3");

        // C-A22-12：周期 Save 里装载城走 NreArchersNull ⇒ `Castle/0/SabukW.txt` 字节不变。
        let before = std::fs::read(root.join("Castle/0/SabukW.txt")).unwrap();
        assert_eq!(
            sha256_hex(&before),
            "6021523b8bc4ada412588109c4417a9876dc30f62a6b086b18ec9cc230eb78ed"
        );
        let tick = save_all(&store, &castles);
        assert_eq!(tick.castles[0].sabuk, CastleFileOutcome::Written);
        assert_eq!(tick.castles[1].sabuk, CastleFileOutcome::SkippedNre);
        assert_eq!(tick.castles[1].attack, CastleFileOutcome::NotAttempted);
        let after = std::fs::read(root.join("Castle/0/SabukW.txt")).unwrap();
        assert_eq!(after, before, "装载城两文件都不写");
        assert_eq!(
            sha256_hex(&after),
            "6021523b8bc4ada412588109c4417a9876dc30f62a6b086b18ec9cc230eb78ed"
        );
        // 周期 Save 的 List.txt 行数 = `CastleList.Count`（此刻 2）。
        assert_eq!(read_text(&root, "Castle/List.txt"), "0\r\n1\r\n");
    }

    /// C-A22-12 的牙：给装载城补 `specs`（= 让它写成功）⇒ `Castle/0/SabukW.txt` 字节变 ⇒ 红。
    #[test]
    fn lazy_castle_with_specs_would_write_and_change_bytes() {
        let root = fixture_root("lazy-tooth");
        write_file(&root, "Castle/List.txt", b"0\r\n");
        write_file(&root, "Castle/0/AttackSabukWall.txt", b"");
        write_file(&root, "Castle/0/SabukW.txt", &deployed_zero_sabuk());
        let cfg = parse_sabuk_w_bytes(&deployed_zero_sabuk());
        let mut store = store_for(&root);
        store.push_base(store.load_base("0").expect("base"));
        let mut lazy = UserCastle::new("沙巴克", CastleSiegeConfig::default());
        lazy.config_dir = "0".to_string();
        lazy.ctor_config_dir = "0".to_string();
        // 装载城正常是 `specs=None` ⇒ NRE。
        let before = std::fs::read(root.join("Castle/0/SabukW.txt")).unwrap();
        let skipped = save_all(&store, &[lazy.clone()]);
        assert_eq!(skipped.castles[0].sabuk, CastleFileOutcome::SkippedNre);
        assert_eq!(
            std::fs::read(root.join("Castle/0/SabukW.txt")).unwrap(),
            before
        );
        // 牙：补上 specs 后写成功 ⇒ 字节变。
        lazy.specs = Some(specs_from_sabuk(&cfg));
        let written = save_all(&store, &[lazy]);
        assert_eq!(written.castles[0].sabuk, CastleFileOutcome::Written);
        assert_ne!(
            std::fs::read(root.join("Castle/0/SabukW.txt")).unwrap(),
            before
        );
    }

    /// `ServerIndex != 0` 的部署：装载城 `map_owner_server_index=0 != server_index` ⇒
    /// `SaveConfig` 早退（`SkippedOwner`），但申请表仍写（`UserCastle.cs:448-452`）。
    ///
    /// 牙：把两道门并成一道（owner 早退也跳过申请表）⇒ `attack` 断言红。
    #[test]
    fn owner_gate_skips_config_but_still_writes_attack_list() {
        let root = fixture_root("owner-gate");
        write_file(&root, "Castle/List.txt", b"0\r\n");
        write_file(&root, "Castle/0/AttackSabukWall.txt", b"");
        write_file(&root, "Castle/0/SabukW.txt", &deployed_zero_sabuk());
        let loader = MapLoader::new(&root, "Map", "Envir");
        let mut store = CastleStore::from_loader(&loader, &test_cfg(), 7);
        store.push_base(store.load_base("0").expect("base"));
        let mut lazy = UserCastle::new("沙巴克", CastleSiegeConfig::default());
        lazy.config_dir = "0".to_string();
        lazy.ctor_config_dir = "0".to_string();
        lazy.server_index = 7;
        let before = std::fs::read(root.join("Castle/0/SabukW.txt")).unwrap();
        let report = save_all(&store, &[lazy]);
        assert_eq!(report.castles[0].sabuk, CastleFileOutcome::SkippedOwner);
        assert_eq!(report.castles[0].attack, CastleFileOutcome::Written);
        assert_eq!(
            std::fs::read(root.join("Castle/0/SabukW.txt")).unwrap(),
            before
        );
    }

    // ------------------------------------------------------------------
    // SHA-256（测试内自证；不为一个判据引依赖）
    // ------------------------------------------------------------------

    fn sha256_hex(bytes: &[u8]) -> String {
        const K: [u32; 64] = [
            0x428a_2f98,
            0x7137_4491,
            0xb5c0_fbcf,
            0xe9b5_dba5,
            0x3956_c25b,
            0x59f1_11f1,
            0x923f_82a4,
            0xab1c_5ed5,
            0xd807_aa98,
            0x1283_5b01,
            0x2431_85be,
            0x550c_7dc3,
            0x72be_5d74,
            0x80de_b1fe,
            0x9bdc_06a7,
            0xc19b_f174,
            0xe49b_69c1,
            0xefbe_4786,
            0x0fc1_9dc6,
            0x240c_a1cc,
            0x2de9_2c6f,
            0x4a74_84aa,
            0x5cb0_a9dc,
            0x76f9_88da,
            0x983e_5152,
            0xa831_c66d,
            0xb003_27c8,
            0xbf59_7fc7,
            0xc6e0_0bf3,
            0xd5a7_9147,
            0x06ca_6351,
            0x1429_2967,
            0x27b7_0a85,
            0x2e1b_2138,
            0x4d2c_6dfc,
            0x5338_0d13,
            0x650a_7354,
            0x766a_0abb,
            0x81c2_c92e,
            0x9272_2c85,
            0xa2bf_e8a1,
            0xa81a_664b,
            0xc24b_8b70,
            0xc76c_51a3,
            0xd192_e819,
            0xd699_0624,
            0xf40e_3585,
            0x106a_a070,
            0x19a4_c116,
            0x1e37_6c08,
            0x2748_774c,
            0x34b0_bcb5,
            0x391c_0cb3,
            0x4ed8_aa4a,
            0x5b9c_ca4f,
            0x682e_6ff3,
            0x748f_82ee,
            0x78a5_636f,
            0x84c8_7814,
            0x8cc7_0208,
            0x90be_fffa,
            0xa450_6ceb,
            0xbef9_a3f7,
            0xc671_78f2,
        ];
        let mut h: [u32; 8] = [
            0x6a09_e667,
            0xbb67_ae85,
            0x3c6e_f372,
            0xa54f_f53a,
            0x510e_527f,
            0x9b05_688c,
            0x1f83_d9ab,
            0x5be0_cd19,
        ];
        let bit_len = (bytes.len() as u64) * 8;
        let mut msg = bytes.to_vec();
        msg.push(0x80);
        while msg.len() % 64 != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bit_len.to_be_bytes());
        for chunk in msg.chunks(64) {
            let mut w = [0u32; 64];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([
                    chunk[4 * i],
                    chunk[4 * i + 1],
                    chunk[4 * i + 2],
                    chunk[4 * i + 3],
                ]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }
            let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
                (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let t1 = hh
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let t2 = s0.wrapping_add(maj);
                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(t1);
                d = c;
                c = b;
                b = a;
                a = t1.wrapping_add(t2);
            }
            for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
                *slot = slot.wrapping_add(value);
            }
        }
        h.iter().map(|x| format!("{x:08x}")).collect()
    }

    /// SHA 实现自证（夹具判据依赖它）：`abc` 的标准向量。
    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let zero = deployed_zero_sabuk();
        assert_eq!(
            sha256_hex(&zero),
            "6021523b8bc4ada412588109c4417a9876dc30f62a6b086b18ec9cc230eb78ed"
        );
    }
}
