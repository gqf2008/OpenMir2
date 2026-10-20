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
//! - **`GetMapOfServerIndex`**：`crates/envir::MapRegistry` 未暴露 `MapInfoEntry.server_index`
//!   的读接口 ⇒ `map_owner_is_self` 按部署口径（`ServerIndex=0`、MapInfo 无 server index ⇒ 0）
//!   恒 `true`。多服分图时需另开片（登记）。
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

use mir2_data::guild_castle::GuildCastleConfig;
use mir2_envir::castle::{DefenseUnit, SabukWConfig};
use mir2_envir::MapRegistry;
use mir2_shared::rng::RandomNumber;
use mir2_world::castle::{
    CastleDefenseSpecs, CastleEvent, CastleSiegeConfig, DefenseCreature, DefenseUnitSpec,
    UserCastle,
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
        map_name: cfg.map_name.clone(),
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
    let cfg = loader
        .load_sabuk_w(castle_dir, castle_dir)
        .map_err(|e| format!("读取 {castle_dir}/{castle_dir}/SabukW.txt 失败：{e}"))?;
    Ok(specs_from_sabuk(&cfg))
}

/// `CastleInitializeEnv` 的生产实现（同一条 `world.rng`，逐次同步造怪）。
pub struct WorldCastleEnv<'a> {
    pub world: &'a mut mir2_world::World,
    pub registry: &'a MapRegistry,
    pub table: &'a MonsterTable,
    pub state: &'a mut MonGenState,
    pub displays: &'a mut downlink::DisplayTable,
    pub rng: &'a mut RandomNumber,
    pub warns: Vec<String>,
}

impl mir2_world::castle::CastleInitializeEnv for WorldCastleEnv<'_> {
    fn map_owner_is_self(&self, _map_name: &str) -> bool {
        // 登记：`MapRegistry` 未暴露 `MapInfoEntry.server_index`；部署件 `ServerIndex=0` 且
        // MapInfo 无 server index ⇒ `GetMapOfServerIndex == 0` 恒真。
        true
    }

    fn find_map(&self, map_name: &str) -> bool {
        self.registry.id_of(map_name).is_some()
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

#[cfg(test)]
mod tests;
