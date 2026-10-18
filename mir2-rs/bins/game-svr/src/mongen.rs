//! MonGen **宿主接线层**（卡 `openmir2-m3-mongen-load-spawn` 第 ③ 片，`bins/game-svr`）。
//!
//! 第 ① 片（`crates/envir`）给 `Envir/MonGen.txt` 的加载/解析；第 ② 片（`crates/world`）
//! 给分组/分片/regen 门/复活判定/`RegenMonsters` 的 RNG 次序。本片把两者接到宿主：
//!
//! 1. **配置**：oracle `Server.conf` 的 8 个键 → [`mir2_world::mongen::MonGenConfig`]
//!    （`ServerConf.cs:21/53/54/63/64/66/636/637`，缺省 = `GameSvrConf.cs` 构造值）；
//! 2. **加载**：`MapLoader::load_mon_gen`（`LocalDB.cs:501`）→ 运行时条目 + 末尾哨兵标志
//!    （`LocalDB.cs:570-590`，`Some(_) ⇒ sentinel_present = true`）；
//! 3. **分组/分片**：`initialize_monster`（`WorldServer.MonGen.cs:30-114`）；
//! 4. **每 tick 驱动**：`step_threads`（`WorldServer.MonGen.cs:192-253` 的单线程串行版）；
//! 5. **`CreateMonster` 回调**（`WorldServer.MonGen.cs:498-810`）的 **RNG 取数次序**：**构造链取数**（`draw_monster_construct`：`BaseObject`/`AnimalObject`/`MonsterObject`/… 各层 ctor + switch 内联 51/52/53/95/96/97）→ `Dir = RandomByte(8)` → `MonGetRandomItems`（掉落预算）→ `Random(100) < CoolEyeCode` → 地形移位。
//!
//! **实体类的行为面**（`ChickenDeer`/`AtMonster`/守卫…的 AI）属承接卡 `openmir2-m3-monster-ai-tick`；本层负责「取数次序 + 构造抽落实体 + 落到世界」。
//!
//! ## 与 oracle 的建模差异（登记）
//!
//! - **线程节拍**：oracle 每线程 `Thread.Sleep(20)`（`WorldServer.MonGen.cs:340`）⇒ regen 门
//!   约每 `RegenMonstersTime + 20ms` 触发一次；本宿主在**世界 tick（200ms）**里调一次
//!   [`MonGenHost::step`]（单线程确定性，设计文档 §4.2 硬约束①）。总刷怪数由 `genModCount`
//!   封顶（每分钟的 `Count`），故长跑收敛；**短窗内的 interleaving 与 oracle 不同**——这条已由
//!   第 ② 片登记，Claim B 的 site/order 账本据此对齐。
//! - **`dir`/`cool_eye`**：本层按 oracle 取数并随 [`MonsterSpawnRequest`] 交给 sink；
//!   `WorldMonsterSink` 落库时写进 `Entity.ai`（`dir`/`cool_eye`，卡 C 第 ② 片宿主接线）。
//!   **怪物帧面**（`SM_TURN`/`SM_FEATURECHANGED` 的 monster `CharDesc`）仍属后续分片（登记）。
//! - **构造链抽值**：本层按 oracle 逐 race 抽完（含公共链 `BaseObject`/`AnimalObject` ctor），并作为
//!   [`MonsterCtorDraws`] 随生成结果落到 `Entity.ctor_draws`（供承接卡 C 的 AI 消费，C 不再重抽）。
//! - **`Envir.FindMap` 已并入 `MapRegistry::id_of`**（部署态 `ServerIndex` 全 0，与
//!   `crates/envir` 的合并口径一致）。
//!
//! 判据（构造级）：见本文件 `tests`——每组都有一条「与 oracle 次序手工复算逐抽对齐」的用例，
//! 以及「改坏必红」的变异说明。

use std::path::Path;

use mir2_data::exps::IniFile;
use mir2_data::models::MonsterInfo;
use mir2_envir::registry::MapRegistry;
use mir2_envir::MapLoader;
use mir2_shared::hutil::make_word;
use mir2_shared::rng::RandomNumber;
use mir2_world::combat::DropItem;
use mir2_world::mongen::{
    initialize_monster, step_threads, InitializeMonsterRecord, MapCanCreate, MonGenConfig,
    MonGenEntry, MonGenState, RegenDecision, TickClockFn,
};
use mir2_world::MonsterCtorDraws;

use crate::downlink;

/// `Server.conf` 缺省：`[Server] VentureServer`（`ServerConf.cs:21`，`"FALSE"` ⇒ false）。
pub const VENTURE_SERVER_DEFAULT: &str = "FALSE";

/// **M3④-实现 C 第 ② 片**：生产宿主的怪物 AI 闸门（`bins/game-svr` 的 sim 循环起世界时调用一次）。
///
/// 单独成函数（而不是在 `sim_loop` 里直接 `world.enable_monster_ai()`）是为了给「宿主真的开了
/// AI」一条**可判据 + 可改坏**的接缝：合成 golden 场景不经这里 ⇒ 仍缺省关闭、逐字节不变。
///
/// `World::tick` 在死亡结算之后、AOI 刷新之前按实体槽位顺序驱动 `monster_ai_tick`
/// （oracle `ProcessMonsters` 线程的单线程等价物，见设计文档 §4.2 硬约束①）。
pub fn enable_production_monster_ai(world: &mut mir2_world::World) {
    world.enable_monster_ai();
}

/// 剥 UTF-8 BOM（`GameSvrConf::load` 走 `decode_text` 的同口径；现网 `Server.conf` 首行带 BOM）。
fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => rest,
        other => other,
    }
}

/// `ServerConf.cs` 的 8 个键 + `[Server] VentureServer` → [`MonGenConfig`]。
///
/// 逐键锚点：`ServerConf.cs:53`（`[Server] MonLimit`）、`:54`（`[Server] ZenLimit`）、
/// `:63`（`[Server] UserFull`）、`:64`（`[Server] ZenFastStep`）、`:66`（`[Server] RegenMonstersTime`）、
/// `:636`（`[Setup] ProcessMonsterInterval`）、`:637`（`[Setup] ProcessMonsterMultiThreadLimit`，
/// `ReadWriteByte`）、`[Setup] GenMonRate`（`GameSvrConf.cs:334` 的字段；`ServerConf` 读它）。
///
/// 缺省值 = [`MonGenConfig::default`]（= `GameSvrConf` 构造值，见 `crates/world/src/mongen.rs` 模块头）。
///
/// # Errors
/// 读文件失败返回人话错误（不静默用缺省）。
pub fn mon_gen_config_from_conf(path: &Path) -> Result<MonGenConfig, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读 {} 失败：{e}", path.display()))?;
    let ini = IniFile::from_gbk_bytes(strip_utf8_bom(&bytes));
    let mut c = MonGenConfig::default();
    c.mon_limit = ini.read_integer("Server", "MonLimit", c.mon_limit);
    c.zen_limit = ini.read_integer("Server", "ZenLimit", c.zen_limit);
    c.user_full = ini.read_integer("Server", "UserFull", c.user_full);
    c.zen_fast_step = ini.read_integer("Server", "ZenFastStep", c.zen_fast_step);
    c.regen_monsters_time = ini.read_integer("Server", "RegenMonstersTime", c.regen_monsters_time);
    c.mon_gen_rate = ini.read_integer("Setup", "GenMonRate", c.mon_gen_rate);
    c.process_monster_interval = ini.read_integer(
        "Setup",
        "ProcessMonsterInterval",
        c.process_monster_interval,
    );
    c.process_monster_multi_thread_limit = ini.read_integer(
        "Setup",
        "ProcessMonsterMultiThreadLimit",
        c.process_monster_multi_thread_limit,
    );
    // `ServerConf.cs:567`：`ReadWriteInteger("Setup", "MakeGhostTime", ...)`；部署件 = 90000。
    c.make_ghost_time = ini.read_integer("Setup", "MakeGhostTime", c.make_ghost_time);
    // `ServerConf.cs:21`：`string.Compare(读取值, "TRUE", OrdinalIgnoreCase) == 0`。
    c.venture_server = ini
        .read_string("Server", "VentureServer", VENTURE_SERVER_DEFAULT)
        .eq_ignore_ascii_case("TRUE");
    Ok(c)
}

/// `crates/envir` 的 `MonGenInfo` → `crates/world` 的运行时 [`MonGenEntry`]（逐字段）。
///
/// 运行时列（`active_count`/`race`/`start_tick`/`cert_list`/`thread_id`）按 oracle
/// `new MonGenInfo()` 的字段缺省给零值；`initialize_monster` 再分片/回填。
#[must_use]
pub fn entries_from_envir(list: &[mir2_envir::MonGenInfo]) -> Vec<MonGenEntry> {
    list.iter()
        .map(|m| MonGenEntry {
            map_name: m.map_name.clone(),
            x: m.x,
            y: m.y,
            mon_name: m.mon_name.clone(),
            range: m.range,
            count: m.count,
            active_count: m.active_count,
            zen_time: m.zen_time,
            mission_gen_rate: m.mission_gen_rate,
            race: m.race,
            start_tick: m.start_tick,
            cert_list: Vec::new(),
            thread_id: 0,
        })
        .collect()
}

/// `MapLoader::load_mon_gen`（`LocalDB.cs:501`）→ 运行时条目。
///
/// `Ok(None)` 对应 oracle 的 `File.Exists == false`（`result=0`、**无哨兵**）；
/// `Ok(Some(_))` 对应 `result=1`（`LocalDB.cs:570-590` 追加末尾哨兵）——调用方把
/// `sentinel_present` 传 [`MonGenHost::start`]。
///
/// # Errors
/// 主文件存在但读取失败、或某行怪物名列触发 `CaptureString` 越界时返回人话错误。
pub fn load_mon_gen_entries(
    loader: &MapLoader,
    registry: &MapRegistry,
) -> Result<Option<Vec<MonGenEntry>>, String> {
    let list = loader
        .load_mon_gen(registry)
        .map_err(|e| format!("加载 Envir/MonGen.txt 失败：{e}"))?;
    Ok(list.map(|l| entries_from_envir(&l)))
}

/// `WorldEngine.MonsterList` 的宿主侧视图（`WorldServer.cs:77` 的
/// `Dictionary<string, MonsterInfo>(StringComparer.OrdinalIgnoreCase)`）。
///
/// 行序 = `select * from monsters` 的行序 = `MonsterList.Values` 的顺序（`initialize_monster`
/// 第二次补抽用 [`MonsterTable::name_list`]）。
#[derive(Clone, Debug, Default)]
pub struct MonsterTable {
    rows: Vec<MonsterInfo>,
}

impl MonsterTable {
    #[must_use]
    pub fn new(rows: Vec<MonsterInfo>) -> Self {
        Self { rows }
    }

    /// `MonsterList.TryGetValue`（OrdinalIgnoreCase，取**首个**命中）。
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&MonsterInfo> {
        self.rows.iter().find(|m| m.name.eq_ignore_ascii_case(name))
    }

    /// `GetMonRace`（`WorldServer.cs:167-174`）：未命中 = `-1`。
    #[must_use]
    pub fn race_of(&self, name: &str) -> i16 {
        self.get(name).map_or(-1, |m| i16::from(m.race))
    }

    /// `MonsterList.Values` 的怪名顺序（DB 行序）。
    #[must_use]
    pub fn name_list(&self) -> Vec<String> {
        self.rows.iter().map(|m| m.name.clone()).collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// [`MonsterSink`] 的落地请求（oracle `CreateMonster` 里「取数完成、登记之前」的全部结果）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonsterSpawnRequest {
    pub map_name: String,
    pub mon_name: String,
    /// 地形移位后的最终坐标。
    pub x: i32,
    pub y: i32,
    /// `MonsterInfo.Race`。
    pub race: i16,
    /// `cert.Dir = RandomByte(8)`。
    pub dir: u8,
    /// `Random(100) < cert.CoolEyeCode`。
    pub cool_eye: bool,
    /// `MonGetRandomItems` 预算的金币（`mon.Gold`）。
    pub gold: i32,
    /// `MonGetRandomItems` 预算的物品（`mon.ItemList`）。
    pub drop_items: Vec<DropItem>,
    /// 构造函数链抽出的值（oracle `BaseObject`/`AnimalObject`/`MonsterObject`/… ctor；
    /// 见 [`draw_monster_construct`]）——随实体落库（供承接卡 C 的 AI 消费，C 不再重抽）。
    pub ctor: MonsterCtorDraws,
}

/// `CreateMonster` 里「实体落地 + 地图查询」的那一半（承接卡 `openmir2-m3-monster-ai-tick` 填实体类）。
///
/// RNG 次序由本层的 [`create_monster_oracle`] 统一驱动；sink 只在**规定的位置**被调用：
/// 掉落预算（Dir 抽之后、CoolEye 抽之前）与最终登记（全部取数完成之后）。
pub trait MonsterSink {
    /// oracle `Envir.Width/Height`（`Envir.FindMap` 命中才返回 `Some`）。
    fn map_size(&self, map_name: &str) -> Option<(i32, i32)>;

    /// oracle `Envir.CanWalk(x, y, true)`：`cellSuccess && Valid`（**忽略**占位）。
    fn cell_walkable_ignore_actors(&self, map_name: &str, x: i32, y: i32) -> bool;

    /// oracle `Envir.CanWalk(x, y, false)`：`cellSuccess && Valid && 无阻挡占位`
    /// （非 `Ghost`/非 `Death`/非 `FixedHideMode`/非 `ObMode`）。
    fn can_walk(&self, map_name: &str, x: i32, y: i32) -> bool;

    /// `MonGetRandomItems`（`WorldServer.MonGen.cs:441-488`）：在给定随机流上预算掉落。
    fn budget_drops(&mut self, rng: &mut RandomNumber, mon_name: &str) -> (i32, Vec<DropItem>);

    /// 登记怪物并返回 actor id（`null` ⇒ 生成失败，`RegenMonsters` 普通支据此 `return false`）。
    fn spawn_monster(&mut self, req: &MonsterSpawnRequest) -> Option<i32>;
}

/// oracle `CreateMonster` 的 switch：这些 `MonsterInfo.Race` 才能构造出非 null 的 `cert`
/// （`WorldServer.MonGen.cs:504-729`）。不在集合内的 race ⇒ **不抽任何数**、直接返回 `None`。
#[must_use]
pub fn race_is_spawnable(race: i16) -> bool {
    matches!(
        race,
        11 | 12
            | 20
            | 51
            | 52
            | 53
            | 55
            | 80..=92
            | 93
            | 94
            | 95
            | 96
            | 97
            | 100..=107
            | 110
            | 111
            | 112
            | 113
            | 114
            | 115..=120
            | 130..=134
            | 200
            | 201
            | 203
            | 206
            | 208
            | 209
            | 210
            | 214
            | 215
    )
}

/// `BaseObject` + `AnimalObject` 两层 ctor 的公共取数（怪物/守卫/练功师都走这条链）。
///
/// - `BaseObject.cs:439` `RunTick = GetTickCount() + Random(1500)`；
/// - `BaseObject.cs:441` `SearchTime = Random(2000) + 2000`；
/// - `AnimalObject.cs:76` `AttackTick = GetTickCount() - Random(3000)`；
/// - `AnimalObject.cs:77` `WalkTick = GetTickCount() - Random(3000)`。
fn draw_base_and_animal(rng: &mut RandomNumber, d: &mut MonsterCtorDraws) {
    d.run_tick_delta = rng.random_below(1500);
    d.base_search_time = rng.random_below(2000) + 2000;
    d.search_time = d.base_search_time;
    d.attack_tick_delta = rng.random_below(3000);
    d.walk_tick_delta = rng.random_below(3000);
}

/// `MonsterObject` ctor（`MonsterObject.cs:65`）：`SearchTime = 3000 + Random(2000)`。
fn draw_monster_object(rng: &mut RandomNumber, d: &mut MonsterCtorDraws) {
    d.search_time = 3000 + rng.random_below(2000);
}

/// `AtMonster` ctor（`ATMonster.cs:10`）：`SearchTime = Random(1500) + 1500`。
fn draw_at_monster(rng: &mut RandomNumber, d: &mut MonsterCtorDraws) {
    d.search_time = rng.random_below(1500) + 1500;
}

/// 派生类 ctor 里的 `SearchTime = Random(1500) + base`（`StickMonster.cs:17` 等 8 处同型）。
fn draw_search_over(rng: &mut RandomNumber, d: &mut MonsterCtorDraws, base: i32) {
    d.search_time = rng.random_below(1500) + base;
}

/// `CreateMonster` switch 内 **race → 构造链** 的全部取数（`WorldServer.MonGen.cs:504-729`）。
///
/// **为什么在这里**：oracle 里 `cert = new Xxx()` 的构造函数在 switch 内同步执行，早于
/// `cert.Dir`（`:740`）。构造链 `BaseObject` → `AnimalObject` → `MonsterObject`/`AtMonster`/… →
/// 派生类**每层都会抽随机数**，因此这些取数属**生成路径**的 RNG 流，不能留给每 tick 的 AI
/// （承接卡 C）。本函数按 oracle 次序抽完，并返回抽出的值（[`MonsterCtorDraws`]）供落实体。
///
/// 次序与行号（逐 race 实读 `src/`）：
/// - `:527` race 52 的 `if (Random(30) == 0)` 在**所有构造函数之前**求值；
/// - 公共链 `BaseObject.cs:439/441` + `AnimalObject.cs:76/77`；
/// - `MonsterObject.cs:65` / `ATMonster.cs:10` / 各派生类 ctor；
/// - switch 内联 `:522`(51) / `:532`(52) / `:541`(52) / `:550`(53) / `:604`(95) / `:612`(96) / `:620`(97)。
///
/// 不在 switch 内的 race ⇒ 不抽任何数（与 [`race_is_spawnable`] 一致）。
#[must_use]
pub fn draw_monster_construct(rng: &mut RandomNumber, race: i16) -> MonsterCtorDraws {
    let mut d = MonsterCtorDraws::default();
    // `:527`：race 52 的 `if (Random(30) == 0)` 早于任何 `new`。
    let deer_zero = if race == 52 {
        Some(rng.random_below(30) == 0)
    } else {
        None
    };
    if !race_is_spawnable(race) {
        return d;
    }
    // `AnimalObject` 链公共部分（`MonsterObject` 与 `GuardUnit` 都经 `AnimalObject`）。
    draw_base_and_animal(rng, &mut d);
    match race {
        // `GuardUnit`/`NormNpc`（守卫/练功师/门/墙）：无额外抽。
        11 | 12 | 20 | 55 | 110 | 111 | 112 => {}
        // `MonsterObject` 本体 + 无 ctor 抽的派生（`DualAxeMonster`/`ThornDarkMonster`/
        // `ArcherMonster`/`ElfMonster`）。
        80 | 87 | 93 | 104 | 113 => draw_monster_object(rng, &mut d),
        // `AnimalObject` 直派生、无额外抽（`BigHeartMonster`/`SoccerBall`）：`SearchTime` 停在 Base 值。
        115 | 120 => {}
        // `AnimalObject` 直派生 + `SearchTime = Random(1500) + 2500`（`StickMonster`/`BeeQueen`/
        // `SpiderHouseMonster`；`CentipedeKingMonster`/`SandMobObject` 继承 `StickMonster`）。
        85 | 103 | 107 | 116 | 132 => draw_search_over(rng, &mut d, 2500),
        // `MonsterObject` + `SearchTime = Random(1500) + 1500`（`RonObject`/`MagicMonObject`/
        // `BoneKingMonster`/`ElectronicScolpionMon`/`CloneMonster`/`TeleMonster`/`Khazard`/
        // `GreenMonster`/`RedMonster`/`FrostTiger`/`FireMonster`/`LightingZombi`/`ScultureKingMonster`）。
        94 | 102 | 131 | 133 | 134 | 200 | 201 | 203 | 206 | 208 | 209 | 210 | 214 => {
            draw_monster_object(rng, &mut d);
            draw_search_over(rng, &mut d, 1500);
        }
        // `ScultureMonster`：MonsterObject + `Random(1500) + 1500`，且初始化式 `BoFearFire = true`（无抽）。
        101 => {
            draw_monster_object(rng, &mut d);
            draw_search_over(rng, &mut d, 1500);
            d.bo_fear_fire = true;
        }
        // `ExplosionSpider`：MonsterObject + `Random(1500) + 2500`。
        117 => {
            draw_monster_object(rng, &mut d);
            draw_search_over(rng, &mut d, 2500);
        }
        // `DigOutZombi`：MonsterObject + 2500，后接 switch 内联 `Random(2)` ⇒ `BoFearFire`（`:604`）。
        95 => {
            draw_monster_object(rng, &mut d);
            draw_search_over(rng, &mut d, 2500);
            d.bo_fear_fire = rng.random_below(2) == 0;
        }
        // `AtMonster` 本体 + 无 ctor 抽的派生（`SlowAtMonster`/`Scorpion`/`WhiteSkeleton`/
        // `DoubleCriticalMonster`）。
        81 | 83 | 84 | 86 | 88 | 89 | 100 | 130 => {
            draw_monster_object(rng, &mut d);
            draw_at_monster(rng, &mut d);
        }
        // `SpitSpider` 系（`ElfWarriorMonster`/`HighRiskSpider`/`BigPoisionSpider`）：
        // AtMonster + `Random(1500) + 1500`（`SpitSpider.cs:14`）。
        82 | 114 | 118 | 119 => {
            draw_monster_object(rng, &mut d);
            draw_at_monster(rng, &mut d);
            draw_search_over(rng, &mut d, 1500);
        }
        // `GasAttackMonster` 系（`GasMothMonster`/`GasDungMonster`）：AtMonster + 1500。
        90 | 105 | 106 => {
            draw_monster_object(rng, &mut d);
            draw_at_monster(rng, &mut d);
            draw_search_over(rng, &mut d, 1500);
        }
        // `MagCowMonster`：AtMonster + 1500。
        91 => {
            draw_monster_object(rng, &mut d);
            draw_at_monster(rng, &mut d);
            draw_search_over(rng, &mut d, 1500);
        }
        // `CowKingMonster`：AtMonster + `Random(1500) + 500`（`CowKingMonster.cs:19`）。
        92 => {
            draw_monster_object(rng, &mut d);
            draw_at_monster(rng, &mut d);
            draw_search_over(rng, &mut d, 500);
        }
        // `CowMonster`：AtMonster + 1500，后接 switch 内联 `Random(2)` ⇒ `BoFearFire`（`:620`）。
        97 => {
            draw_monster_object(rng, &mut d);
            draw_at_monster(rng, &mut d);
            draw_search_over(rng, &mut d, 1500);
            d.bo_fear_fire = rng.random_below(2) == 0;
        }
        // `ZilKinZombi`：AtMonster + `Random(1500) + 2500`；ctor 内 `if (Random(3)==0)
        // ZilKillCount = Random(3)+1`（`:18-20`）；再 switch 内联 `Random(4)` ⇒ `BoFearFire`（`:612`）。
        96 => {
            draw_monster_object(rng, &mut d);
            draw_at_monster(rng, &mut d);
            draw_search_over(rng, &mut d, 2500);
            if rng.random_below(3) == 0 {
                d.zil_kill_count = rng.random_below(3) + 1;
            }
            d.bo_fear_fire = rng.random_below(4) == 0;
        }
        // `AnimalChicken`：MonsterObject ctor → 初始化式 `MeatQuality = Random(3500) + 3000`（`:522`）。
        51 => {
            draw_monster_object(rng, &mut d);
            d.meat_quality = (rng.random_below(3500) + 3000) as u16;
        }
        // `AnimalDeer`：先 `Random(30)`（已抽），再 `new ChickenDeer`/`new MonsterObject`（同 5 抽），
        // 再 `MeatQuality = Random(20000)+10000` 或 `Random(8000)+8000`（`:532`/`:541`）。
        52 => {
            draw_monster_object(rng, &mut d);
            let zero = deer_zero.unwrap_or(false);
            d.meat_quality = if zero {
                (rng.random_below(20000) + 10000) as u16
            } else {
                (rng.random_below(8000) + 8000) as u16
            };
        }
        // `AnimalWolf`：`new AtMonster` + `MeatQuality = Random(8000) + 8000`（`:550`）。
        53 => {
            draw_monster_object(rng, &mut d);
            draw_at_monster(rng, &mut d);
            d.meat_quality = (rng.random_below(8000) + 8000) as u16;
        }
        // `FireballMonster : MagicMonster : MonsterObject`：两层 ctor 各 1 抽
        //（`MagicMonster.cs:13` 3000+Random(2000)，`FireballMonster.cs:11` Random(1500)+1500）。
        215 => {
            draw_monster_object(rng, &mut d);
            d.search_time = 3000 + rng.random_below(2000);
            draw_search_over(rng, &mut d, 1500);
        }
        // 其余可生成 race 已在上面覆盖；不可生成 race 在函数开头已返回。
        _ => {}
    }
    d
}

/// [`MonsterCreator`] 回调的生产实现：`WorldServer.MonGen.cs:498-810` 的**取数次序**。
///
/// 次序（判据逐抽钉死）：
/// 1. `Envir.FindMap(sMapName) == null` ⇒ `None`（**不抽**）；
/// 2. race 不在 switch 内 ⇒ `None`（**不抽**）；
/// 3. **构造链取数**（[`draw_monster_construct`]，含 switch 内联 51/52/53/95/96/97）；
/// 4. `Dir = RandomByte(8)`（`:740`）；
/// 5. `MonGetRandomItems`（`:742` → `:441-488`）；
/// 6. `Random(100) < CoolEyeCode`（`:743-746`）；
/// 7. 地形移位（`:748-800`）：**仅当初始格 `CanWalk(x,y,true)` 为假**时进入；循环内
///    `CanWalk(x,y,false)` 为假才移位（`Random(Width/2)` / `Random(Height/2)`），最多 31 次。
///
/// 返回 `None` 等价 oracle 的 `cert == null` / `!outofrange`（`RegenMonsters` 普通支据此停）。
#[allow(clippy::too_many_arguments)]
pub fn create_monster_oracle<S: MonsterSink>(
    rng: &mut RandomNumber,
    sink: &mut S,
    table: &MonsterTable,
    map_name: &str,
    x: i16,
    y: i16,
    race: i16,
    mon_name: &str,
) -> Option<i32> {
    // `:500-503`：`FindMap` 为空 ⇒ null（在 any RNG 之前）。
    let (width, height) = sink.map_size(map_name)?;
    // switch 未命中 ⇒ null（在 any RNG 之前）。
    if !race_is_spawnable(race) {
        return None;
    }
    let ctor = draw_monster_construct(rng, race);

    let dir = rng.random_byte(8);
    let (gold, drop_items) = sink.budget_drops(rng, mon_name);
    let cool_eye_code = table.get(mon_name).map_or(0, |m| i32::from(m.cool_eye));
    let cool_eye = rng.random_below(100) < cool_eye_code;

    let mut cx = i32::from(x);
    let mut cy = i32::from(y);
    // `:735-800`：`Initialize()` 里 `if (CanWalk(x,y,true) && AddToMap()) AddtoMapSuccess = false;`
    // ⇒ `AddtoMapSuccess` 为真（进入移位循环）当且仅当**忽略占位**也不可站。
    if !sink.cell_walkable_ignore_actors(map_name, cx, cy) {
        let n20: i32 = if width < 50 { 2 } else { 3 };
        let n24: i32 = if height < 250 {
            if height < 30 {
                2
            } else {
                20
            }
        } else {
            50
        };
        let mut out_of_range = false;
        let mut attempts = 0;
        loop {
            if !sink.can_walk(map_name, cx, cy) {
                if (width - n24 - 1) > cx {
                    cx += n20;
                } else {
                    cx = rng.random_below(width / 2) + n24;
                    if height - n24 - 1 > cy {
                        cy += n20;
                    } else {
                        cy = rng.random_below(height / 2) + n24;
                    }
                }
            } else {
                // `:787`：`outofrange = AddMapObject(...)`（可走 ⇒ 登记成功）。
                out_of_range = true;
                break;
            }
            attempts += 1;
            if attempts >= 31 {
                break;
            }
        }
        if !out_of_range {
            return None;
        }
    }

    sink.spawn_monster(&MonsterSpawnRequest {
        map_name: map_name.to_string(),
        mon_name: mon_name.to_string(),
        x: cx,
        y: cy,
        race,
        dir,
        cool_eye,
        gold,
        drop_items,
        ctor,
    })
}

/// 刷怪调度宿主（`MonGenList` + `MonGenInfoThreadMap` + 每条线程的游标）。
#[derive(Clone, Debug)]
pub struct MonGenHost {
    pub state: MonGenState,
    pub config: MonGenConfig,
    pub init: InitializeMonsterRecord,
}

impl MonGenHost {
    /// oracle `WorldEngine.InitializeMonster`（`WorldServer.MonGen.cs:30-114`）的宿主包装。
    ///
    /// `sentinel_present` = `LocalDB.LoadMonGen` 返回 `Ok(Some(_))`（`LocalDB.cs:570-590`）。
    #[must_use]
    pub fn start(
        entries: Vec<MonGenEntry>,
        table: &MonsterTable,
        sentinel_present: bool,
        config: MonGenConfig,
        rng: &mut RandomNumber,
    ) -> Self {
        let names = table.name_list();
        let race_of = |name: &str| table.race_of(name);
        let (state, init) =
            initialize_monster(entries, &names, sentinel_present, &config, rng, &race_of);
        Self {
            state,
            config,
            init,
        }
    }

    /// 单线程串行驱动一轮（宿主每 tick 调一次）：`step_threads` 包装。
    ///
    /// `can_create(map_name)` 对应 `map == null || (map.Flag.boNOHUMNOMON && map.HumCount <= 0)`
    /// 的取反（`WorldServer.MonGen.cs:235-243`）。
    #[allow(clippy::too_many_arguments)]
    pub fn step<S: MonsterSink>(
        &mut self,
        now: i32,
        play_object_count: i32,
        rng: &mut RandomNumber,
        clock: &mut TickClockFn,
        can_create: &mut MapCanCreate,
        sink: &mut S,
        table: &MonsterTable,
    ) -> Vec<RegenDecision> {
        let mut create = |rng: &mut RandomNumber,
                          map_name: &str,
                          x: i16,
                          y: i16,
                          race: i16,
                          mon_name: &str|
         -> Option<i32> {
            create_monster_oracle(rng, sink, table, map_name, x, y, race, mon_name)
        };
        step_threads(
            &mut self.state,
            now,
            play_object_count,
            &self.config,
            rng,
            clock,
            can_create,
            &mut create,
        )
    }
}

/// 把本轮 `RegenMonsters` 新建的怪登记为「可复活」（oracle `WorldServer.MonGen.cs:832-835`
/// 与 `:854-857` 的 `cert.CanReAlive = true; cert.ReAliveTick = GetTickCount(); cert.MonGen = monGen;`）。
///
/// **只登记本轮 regen 追加到 `CertList` 的 id**（`before` = 调用前的各条 `cert_list.len()`）：
/// oracle 的 `RegenMonsterByName`（GuardList 放置，`:382-432`）**不**置这三个字段 ⇒ 守卫不可复活。
/// 若按「所有 cert_list 成员」登记会把守卫误标成可复活（行为分叉）。
///
/// 本函数不消费随机数。
pub fn bind_new_certificates(
    state: &MonGenState,
    world: &mut mir2_world::World,
    before: &[usize],
    now: i64,
) -> Vec<i32> {
    let now_i32 = i32::try_from(now).unwrap_or(i32::MAX);
    let mut bound = Vec::new();
    for (idx, entry) in state.entries.iter().enumerate() {
        let start = before.get(idx).copied().unwrap_or(0);
        for &id in entry.cert_list.iter().skip(start) {
            let Some(e) = world.entities.get_mut(id) else {
                continue;
            };
            // 已有归属（复活后的怪仍在同一 cert_list）⇒ 不覆盖。
            if e.mon_gen.is_some() {
                continue;
            }
            e.can_re_alive = true;
            e.re_alive_tick = i64::from(now_i32);
            e.mon_gen = Some(idx);
            e.mon_gen_zen_time = entry.zen_time;
            bound.push(id);
        }
    }
    bound
}

/// 生产 sink：把 [`MonsterSpawnRequest`] 落到 `crates/world` 的实体仓库。
///
/// 能力字段照 oracle `ApplyMonsterAbility`（`WorldServer.MonGen.cs:880-910`）：
/// `Level = monster.Level`、`HP/MaxHP = monster.HP`、`AC = AC`、`DC = MakeWord(DC, MaxDC)`、
/// `LifeAttrib = monster.btLifeAttrib`、`HitPoint = monster.HitPoint`、`SpeedPoint = monster.Speed`、
/// `FightExp = monster.Exp`。`dir`/`cool_eye` 随生成结果写进 `Entity.ai`（见模块头）。
pub struct WorldMonsterSink<'a> {
    pub world: &'a mut mir2_world::World,
    pub registry: &'a MapRegistry,
    pub table: &'a MonsterTable,
    /// **M3④-实现 C 第 ③ 片**：怪物的展示层字段（`Feature/Light/ShowName/…`）由
    /// `bins/game-svr` 的展示表承载（与玩家同表）——生成时按 `MonsterInfo` 算一次注册，
    /// 同屏下行（`drain_aoi_downlink`）/死亡广播（`drain_combat_downlink`）按 `ActorId` 查回。
    /// 展示字段属读档/编码面，**不进** `crates/world`。
    pub displays: &'a mut downlink::DisplayTable,
}

impl WorldMonsterSink<'_> {
    fn map_id(&self, map_name: &str) -> Option<usize> {
        self.registry.id_of(map_name)
    }
}

/// 与 oracle `CanWalk(x,y,false)` 同构：`cellSuccess && Valid && 无阻挡活体`。
fn cell_walkable(
    world: &mir2_world::World,
    map: &mir2_world::map::MapGrid,
    x: i32,
    y: i32,
    ignore_actors: bool,
) -> bool {
    let (cell, success) = map.get_cell_info(x, y);
    if !success {
        return false;
    }
    if ignore_actors {
        return true;
    }
    for obj in &cell.obj_list {
        if obj.actor_object {
            if let Some(e) = world.entities.get(obj.cell_obj_id) {
                if !e.ghost && !e.death && !e.fixed_hide_mode && !e.ob_mode {
                    return false;
                }
            }
        }
    }
    true
}

impl MonsterSink for WorldMonsterSink<'_> {
    fn map_size(&self, map_name: &str) -> Option<(i32, i32)> {
        let id = self.map_id(map_name)?;
        let map = self.world.maps.get(id)?;
        Some((map.width, map.height))
    }

    fn cell_walkable_ignore_actors(&self, map_name: &str, x: i32, y: i32) -> bool {
        let Some(id) = self.map_id(map_name) else {
            return false;
        };
        let Some(map) = self.world.maps.get(id) else {
            return false;
        };
        cell_walkable(self.world, map, x, y, true)
    }

    fn can_walk(&self, map_name: &str, x: i32, y: i32) -> bool {
        let Some(id) = self.map_id(map_name) else {
            return false;
        };
        let Some(map) = self.world.maps.get(id) else {
            return false;
        };
        cell_walkable(self.world, map, x, y, false)
    }

    fn budget_drops(&mut self, rng: &mut RandomNumber, mon_name: &str) -> (i32, Vec<DropItem>) {
        // oracle `MonGetRandomItems`：`monster.ItemList != null && Count > 0` 才抽。
        let list: &[mir2_data::models::MonsterDropItem] = self
            .table
            .get(mon_name)
            .and_then(|m| m.item_list.as_deref())
            .unwrap_or(&[]);
        if list.is_empty() {
            return (0, Vec::new());
        }
        self.world.budget_monster_drops(rng, list)
    }

    fn spawn_monster(&mut self, req: &MonsterSpawnRequest) -> Option<i32> {
        let map_id = self.map_id(&req.map_name)?;
        let info = self.table.get(&req.mon_name);
        let (level, exp, hp, dc, ac, undead, hit_point, speed_point) =
            info.map_or((0, 0, 0, 0, 0, 0, 0, 0), |m| {
                (
                    m.level,
                    m.exp,
                    m.hp,
                    make_word(m.dc, m.max_dc),
                    m.ac,
                    m.life_attrib,
                    m.hit_point,
                    m.speed,
                )
            });
        let id = self.world.spawn_monster_precomputed(
            &req.mon_name,
            req.x,
            req.y,
            map_id,
            u8::try_from(req.race).unwrap_or(80),
            level,
            exp,
            hp,
            dc,
            ac,
            undead,
            hit_point,
            speed_point,
            req.gold,
            req.drop_items.clone(),
            req.ctor,
            req.dir,
            req.cool_eye,
        )?;
        // **C 第 ③ 片**：生成成功即登记怪物展示层。`MonsterList` 未命中时 oracle 的
        // `ApplyMonsterAbility` 不做任何事（`WorldServer.MonGen.cs:880-882`），`cert` 仍被构造；
        // 本仓缺表 ⇒ 无 feature 输入，保持不登记（与玩家的“无展示表 ⇒ 跳过”同口径）。
        if let Some(mon) = info {
            // `ApplyMonsterAbility`（`WorldServer.MonGen.cs:896-897`）：`Abil.MP = 0`、
            // `Abil.MaxMP = monster.MP`。HP 已在生成路径写入，这里补齐 MP 上限
            // （缺表时 oracle 不动能力⇒保持 0）。
            if let Some(e) = self.world.entities.get_mut(id) {
                e.mp = 0;
                e.max_mp = mon.mp;
            }
            self.displays
                .set(id, downlink::monster_display(req.dir, mon));
        }
        Some(id)
    }
}

/// **卡 `openmir2-run-gap-monster-specials`（F1）**：`SpecialSpawner` 的**生产实现**。
///
/// oracle `BeeQueen.Operate` / `SpiderHouseMonster.Operate` 的 `RM_ZEN_BEE` 支调用
/// `SystemShare.WorldEngine.RegenMonsterByName(Envir.MapName, x, y, Config.Bee/Spider)`
/// （`BeeQueen.cs:35`、`SpiderHouseMonster.cs:35`）。本实现把
/// [`crate::guardlist::regen_monster_by_name`]（同一 `CreateMonster` 取数链）接到宿主：
/// 地图 `id` → 逻辑名（`MapRegistry::name_of`）、宿主展示表登记、`MonGenState` 与
/// MonGen 同一份（oracle 的合成 `MonGenInfo` 也追加到线程表）。
///
/// RNG：调用方（`World::flush_special_spawns` / `deliver_self_delay_msgs_with`）传入
/// **同一条** `world.rng`，本实现不额外抽数。
pub struct WorldSpecialSpawner<'a> {
    pub registry: &'a MapRegistry,
    pub table: &'a MonsterTable,
    pub state: &'a mut MonGenState,
    pub displays: &'a mut downlink::DisplayTable,
}

impl mir2_world::SpecialSpawner for WorldSpecialSpawner<'_> {
    fn regen_monster_by_name(
        &mut self,
        world: &mut mir2_world::World,
        rng: &mut RandomNumber,
        map_id: usize,
        name: &str,
        x: i16,
        y: i16,
    ) -> Option<i32> {
        // oracle `Envir.MapName`：本仓 `map_id` 的注册名；未注册 ⇒ `FindMap == null` ⇒ null。
        let map_name = self.registry.name_of(map_id)?.to_string();
        let mut sink = WorldMonsterSink {
            world,
            registry: self.registry,
            table: self.table,
            displays: self.displays,
        };
        crate::guardlist::regen_monster_by_name(
            rng, &mut sink, self.table, self.state, &map_name, x, y, name,
        )
    }
}

/// `[Server] VentureServer` 的缺省常量（供调用方/测试对照）。
#[must_use]
pub fn venture_server_default_is_false() -> bool {
    !VENTURE_SERVER_DEFAULT.eq_ignore_ascii_case("TRUE")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_data::models::MonsterDropItem;

    /// 用例级唯一临时目录的序号（`pid + 原子计数`，照 `LESSON_并行测试临时路径须pid加原子计数构造`）。
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    /// 记录型 sink：把回调结果留档，地图查询走构造表（不依赖 `crates/world`）。
    #[derive(Default)]
    struct RecordingSink {
        size: Option<(i32, i32)>,
        /// `(x, y) -> (walkable_ignore_actors, walkable)`
        cells: std::collections::HashMap<(i32, i32), (bool, bool)>,
        /// 每次 `budget_drops` 记 `(mon_name, 返回值固定为 0/空)` 的调用次数。
        budget_calls: Vec<String>,
        spawns: Vec<MonsterSpawnRequest>,
        next_id: i32,
    }

    impl RecordingSink {
        fn with_size(w: i32, h: i32) -> Self {
            Self {
                size: Some((w, h)),
                next_id: 100,
                ..Self::default()
            }
        }
        fn set_cell(&mut self, x: i32, y: i32, walk_ignore: bool, walk: bool) {
            self.cells.insert((x, y), (walk_ignore, walk));
        }
    }

    impl MonsterSink for RecordingSink {
        fn map_size(&self, _map_name: &str) -> Option<(i32, i32)> {
            self.size
        }
        fn cell_walkable_ignore_actors(&self, _map_name: &str, x: i32, y: i32) -> bool {
            self.cells.get(&(x, y)).is_some_and(|c| c.0)
        }
        fn can_walk(&self, _map_name: &str, x: i32, y: i32) -> bool {
            self.cells.get(&(x, y)).is_some_and(|c| c.1)
        }
        fn budget_drops(
            &mut self,
            _rng: &mut RandomNumber,
            mon_name: &str,
        ) -> (i32, Vec<DropItem>) {
            self.budget_calls.push(mon_name.to_string());
            (7, Vec::new())
        }
        fn spawn_monster(&mut self, req: &MonsterSpawnRequest) -> Option<i32> {
            self.spawns.push(req.clone());
            let id = self.next_id;
            self.next_id += 1;
            Some(id)
        }
    }

    fn info(
        name: &str,
        race: u8,
        cool_eye: u8,
        items: Option<Vec<MonsterDropItem>>,
    ) -> MonsterInfo {
        MonsterInfo {
            name: name.to_string(),
            race,
            cool_eye,
            level: 3,
            hp: 25,
            dc: 1,
            max_dc: 2,
            ac: 4,
            hit_point: 5,
            speed: 6,
            exp: 11,
            life_attrib: 1,
            item_list: items,
            ..MonsterInfo::default()
        }
    }

    /// **判据 1（构造链 + 内联抽的完整次序）**：race 51 的 `create_monster_oracle` 之后，
    /// 随机流的下一抽必须与「按 oracle 真次序（含构造链）手工复算」完全一致，且构造抽值逐抽相等。
    ///
    /// oracle 次序（`WorldServer.MonGen.cs:511-513` + 构造链）：
    /// `new MonsterObject()` = `BaseObject.cs:439/441` → `AnimalObject.cs:76/77` → `MonsterObject.cs:65`
    /// ⇒ 5 抽；再初始化式 `MeatQuality = Random(3500)+3000`。
    #[test]
    fn create_monster_chicken_consumes_random_3500_then_dir_drops_cool_eye() {
        let table = MonsterTable::new(vec![
            info("鸡", 51, 50, None),
            info("鹿", 52, 0, None),
            info("稻草人", 80, 0, None),
        ]);
        let mut sink = RecordingSink::with_size(100, 100);
        sink.set_cell(10, 10, true, true);

        let mut a = RandomNumber::with_seed(4242);
        let req_id = create_monster_oracle(&mut a, &mut sink, &table, "0", 10, 10, 51, "鸡");
        assert_eq!(req_id, Some(100));
        assert_eq!(
            sink.budget_calls,
            vec!["鸡".to_string()],
            "掉落预算必须恰好一次"
        );
        assert_eq!(sink.spawns.len(), 1);
        assert_eq!(sink.spawns[0].gold, 7);
        assert_eq!(sink.spawns[0].x, 10);
        assert_eq!(sink.spawns[0].y, 10);

        // 手工复算（oracle 真次序，含构造链）：
        //   RunTick Random(1500) → BaseObject.SearchTime Random(2000)+2000 →
        //   AttackTick Random(3000) → WalkTick Random(3000) → MonsterObject.SearchTime Random(2000)+3000 →
        //   MeatQuality Random(3500)+3000 → Dir RandomByte(8) → CoolEye Random(100)。
        let mut b = RandomNumber::with_seed(4242);
        let exp = MonsterCtorDraws {
            run_tick_delta: b.random_below(1500),
            base_search_time: b.random_below(2000) + 2000,
            attack_tick_delta: b.random_below(3000),
            walk_tick_delta: b.random_below(3000),
            search_time: b.random_below(2000) + 3000,
            meat_quality: (b.random_below(3500) + 3000) as u16,
            bo_fear_fire: false,
            zil_kill_count: 0,
        };
        let exp_dir = b.random_byte(8);
        let exp_cool = b.random_below(100);
        // 随机流位置对齐：下一抽两侧相同。
        assert_eq!(a.random(), b.random(), "抽数个数/次序与 oracle 不一致");
        assert_eq!(sink.spawns[0].ctor, exp, "构造链抽值必须逐抽相同");
        assert_eq!(sink.spawns[0].dir, exp_dir);
        assert_eq!(sink.spawns[0].cool_eye, exp_cool < 50);
    }

    /// 判据 2：race 52 的 `Random(30) == 0` 分支决定后续是 `Random(20000)` 还是 `Random(8000)`，
    /// 且 `Random(30)`（`:527`）在构造函数**之前**抽——构造链总抽数两支都是 7。
    #[test]
    fn create_monster_deer_branch_matches_oracle() {
        let table = MonsterTable::new(vec![info("鹿", 52, 0, None)]);
        // 直接构造两个独立流逐步验证「命中 0 分支」的抽数：
        let mut a = RandomNumber::with_seed(7);
        let mut b = RandomNumber::with_seed(7);
        let mut sink = RecordingSink::with_size(100, 100);
        sink.set_cell(0, 0, true, true);
        let _ = create_monster_oracle(&mut a, &mut sink, &table, "0", 0, 0, 52, "鹿");
        // 手工复算（oracle 真次序）：Random(30) → 构造链 5 抽 → Random(20000)/Random(8000)。
        let first = b.random_below(30);
        let _ = b.random_below(1500);
        let _ = b.random_below(2000);
        let _ = b.random_below(3000);
        let _ = b.random_below(3000);
        let _ = b.random_below(2000);
        if first == 0 {
            let _ = b.random_below(20000);
        } else {
            let _ = b.random_below(8000);
        }
        let _ = b.random_byte(8);
        let _ = b.random_below(100);
        assert_eq!(a.random(), b.random());
    }

    /// 判据 3：race 不在 switch 内 ⇒ 不抽任何数、返回 None（oracle `cert == null`）。
    #[test]
    fn create_monster_unknown_race_consumes_nothing() {
        let table = MonsterTable::new(vec![]);
        let mut sink = RecordingSink::with_size(100, 100);
        let mut a = RandomNumber::with_seed(1);
        let mut b = RandomNumber::with_seed(1);
        assert_eq!(
            create_monster_oracle(&mut a, &mut sink, &table, "0", 0, 0, 50, "x"),
            None
        );
        assert_eq!(a.random(), b.random(), "未知 race 不许抽数");
        assert!(sink.spawns.is_empty());
    }

    /// 判据 4：`FindMap` 为空（sink 无该图尺寸）⇒ 不抽任何数。
    #[test]
    fn create_monster_missing_map_consumes_nothing() {
        let table = MonsterTable::new(vec![info("稻草人", 80, 0, None)]);
        let mut sink = RecordingSink::default(); // size = None
        let mut a = RandomNumber::with_seed(1);
        let mut b = RandomNumber::with_seed(1);
        assert_eq!(
            create_monster_oracle(&mut a, &mut sink, &table, "没有这张图", 0, 0, 80, "稻草人"),
            None
        );
        assert_eq!(a.random(), b.random());
    }

    /// 判据 5：`CoolEyeCode` 的阈值使用 `Random(100) < code`（code=100 ⇒ 恒真）。
    #[test]
    fn create_monster_cool_eye_threshold() {
        let table = MonsterTable::new(vec![info("稻草人", 80, 100, None)]);
        let mut sink = RecordingSink::with_size(100, 100);
        sink.set_cell(0, 0, true, true);
        let mut rng = RandomNumber::with_seed(9);
        let _ = create_monster_oracle(&mut rng, &mut sink, &table, "0", 0, 0, 80, "稻草人");
        assert!(sink.spawns[0].cool_eye, "code=100 ⇒ Random(100)<100 恒真");
    }

    /// 判据 6：初始格「忽略占位也不可站」⇒ 进入移位循环；可站 ⇒ 不移位。
    #[test]
    fn create_monster_terrain_shift_only_when_cell_unwalkable() {
        let table = MonsterTable::new(vec![info("稻草人", 80, 0, None)]);
        // (5,5) 不可站（ignore-actors 也 false），(6,5) 可走 ⇒ 移位 +n20（宽<50 ⇒ 2）到 (7,5)。
        let mut sink = RecordingSink::with_size(30, 30);
        sink.set_cell(5, 5, false, false);
        sink.set_cell(7, 5, true, true);
        let mut a = RandomNumber::with_seed(3);
        let mut b = RandomNumber::with_seed(3);
        let _ = create_monster_oracle(&mut a, &mut sink, &table, "0", 5, 5, 80, "稻草人");
        assert_eq!(sink.spawns[0].x, 7, "n20 = 2（Width < 50）");
        assert_eq!(sink.spawns[0].y, 5);
        // 移位分支**不抽数**（`(width-n24-1) > cx` 成立时走 `cx += n20`）；
        // 手工序列 = 构造链 5 抽（`new MonsterObject()`）+ Dir + CoolEye。
        let _ = b.random_below(1500);
        let _ = b.random_below(2000);
        let _ = b.random_below(3000);
        let _ = b.random_below(3000);
        let _ = b.random_below(2000);
        let _ = b.random_byte(8);
        let _ = b.random_below(100);
        assert_eq!(a.random(), b.random());
    }

    // ================= 地形移位环「活栈」判据（卡 `openmir2-mongen-terrain-loop-live`）=================
    //
    // ## 逐条对照：oracle `src/GameSrv/Word/WorldServer.MonGen.cs` ↔ `create_monster_oracle`（本文件）
    //
    // | oracle（行号，实读） | Rust（本文件，行号，实读） | 判定 |
    // | --- | --- | --- |
    // | `:751` `if (cert.AddtoMapSuccess)`——`Initialize()`（`BaseObject.Base.cs:17-21`）置真当且仅当
    //   `Envir.CanWalk(x,y,true)` 为假；`AddToMap()`（`BaseObject.cs:1370`）⇔ `AddMapObject`
    //   （`Envirnoment.cs:252-294`，界内且 `Valid`）与 `CanWalk(...,true)`（`Envirnoment.cs:442-451`）
    //   同条件 ⇒ `AddtoMapSuccess == !CanWalk(x,y,true)` | `:499` `if !sink.cell_walkable_ignore_actors(map_name, cx, cy)`；
    //   sink 侧 `cell_walkable(...,true)`（`:645-663`）= `get_cell_info(x,y).1` = 界内且 `Valid` | 同构 |
    // | `:754` `n20 = Width < 50 ? 2 : 3` | `:500` `if width < 50 { 2 } else { 3 }` | 同构 |
    // | `:755-763` `n24 = Height < 250 ? (Height < 30 ? 2 : 20) : 50` | `:501-509` 同式 | 同构 |
    // | `:764` `int n1C = 0;` | `:511` `let mut attempts = 0;` | 同构 |
    // | `:767` 环首 `if (!cert.Envir.CanWalk(x,y,false))` | `:513` `if !sink.can_walk(map_name, cx, cy)` | 同构 |
    // | `:769-771` X 非边界 `if ((Width - n24 - 1) > x) x += n20;` | `:514-515` 同式 | 同构 |
    // | `:775` X 边界支 `x = Random(Width / 2) + n24;`（**在环内**） | `:517` `cx = rng.random_below(width / 2) + n24;`（在环内） | 同构 |
    // | `:776-778` Y 非边界 `if (Height - n24 - 1 > y) y += n20;` | `:518-519` 同式 | 同构 |
    // | `:782` Y 边界支 `y = Random(Height / 2) + n24;`（**只在上一条为假时**） | `:521` 同式（同一嵌套） | 同构 |
    // | `:788-789` 可走 ⇒ `outofrange = AddMapObject(...); break;` | `:526` `out_of_range = true; break;`（`can_walk` 真 ⇒ 界内且 `Valid` ⇒ `AddMapObject` 必真） | 同构 |
    // | `:791-795` `n1C++; if (n1C >= 31) break;` | `:529-531` `attempts += 1; if attempts >= 31 { break; }` | 同构 |
    // | `:797-801` 31 次仍不可走 ⇒ `if (!outofrange) return null;` | `:534-535` `if !out_of_range { return None; }` | 同构 |
    //
    // **差异点：无**——边界支取数式在环内、X 抽先于 Y 抽、Y 抽只在 `Height - n24 - 1 <= y` 时发生、
    // 31 计数在移位之后。逐行同构（已含「门/环体/计数/返回」全路径）。下面的活栈用例把
    // 「环被进入 / 首轮即走边界支 / `world.rng` 抽数逐项 == oracle 期望表」钉死。

    /// **判据 15a（活栈 · 双轴边界支）**：初始格是墙（忽略占位也不可站）⇒ 进入环；**首轮** X 在边界
    /// （`x >= Width - n24 - 1`）⇒ 抽 `Random(Width/2)`；Y 也在边界 ⇒ 再抽 `Random(Height/2)`。
    ///
    /// oracle 期望表（独立 Python 复刻 .NET `System.Random` legacy 减法算法算得；seed=12345，
    /// 地图 100×100 ⇒ `n20=3`、`n24=20`；起点 (85,85) ⇒ `Width-n24-1=79 > 85` 假、`79 > 85` 假）：
    /// 构造链 5 抽 `[100,140,2324,1533,1594]` → `Dir=RandomByte(8)=6` → `CoolEye=Random(100)=16`
    /// → 边界支 `Random(50)=36`（X）→ `Random(50)=13`（Y）⇒ 落格 `(36+20, 13+20) = (56, 33)`。
    #[test]
    fn terrain_boundary_both_axes_draws_oracle_expected() {
        let table = MonsterTable::new(vec![info("稻草人", 80, 0, None)]);
        let mut sink = RecordingSink::with_size(100, 100);
        sink.set_cell(85, 85, false, false); // 墙格出生点（忽略占位也不可站）
        sink.set_cell(56, 33, true, true); // 边界支算出的落点（可站 ⇒ 环第 2 轮即登记）
        let mut a = RandomNumber::with_seed(12345);
        let id = create_monster_oracle(&mut a, &mut sink, &table, "0", 85, 85, 80, "稻草人");
        assert_eq!(id, Some(100), "环进入并在第 2 轮登记成功");
        assert_eq!(sink.spawns.len(), 1);
        // ① 环被进入（坐标从出生点移开）；② 首轮即走边界支 ⇒ 落格 = 边界支取数式结果。
        assert_eq!(
            (sink.spawns[0].x, sink.spawns[0].y),
            (56, 33),
            "x = Random(Width/2)+n24、y = Random(Height/2)+n24"
        );
        // ③ world.rng 抽数逐项等于 oracle 期望表（在独立流上重放同一序列）。
        let mut b = RandomNumber::with_seed(12345);
        for m in [1500, 2000, 3000, 3000, 2000] {
            let _ = b.random_below(m);
        }
        let _ = b.random_byte(8);
        let _ = b.random_below(100);
        assert_eq!(b.random_below(100 / 2), 36, "边界支 X 抽 = Random(Width/2)");
        assert_eq!(
            b.random_below(100 / 2),
            13,
            "边界支 Y 抽 = Random(Height/2)"
        );
        assert_eq!(a.random(), b.random(), "取数序列与 oracle 期望表逐项一致");
    }

    /// **判据 15b（活栈 · 仅 X 边界支）**：首轮 X 在边界但 Y 不在（`Height-n24-1 > y` 真）⇒
    /// **只抽 X**（`Random(Width/2)`），Y 走 `y += n20` **不抽数**——钉死「Y 抽挂在 `:776` 的假支」。
    ///
    /// 期望表（seed=12345；起点 (85,10)）:构造链 5 抽 + `Dir` + `CoolEye` → `Random(50)=36` ⇒
    /// 落格 `(36+20, 10+3) = (56, 13)`（`n20=3`、`n24=20`），**无第二抽**。
    #[test]
    fn terrain_boundary_x_only_draws_one() {
        let table = MonsterTable::new(vec![info("稻草人", 80, 0, None)]);
        let mut sink = RecordingSink::with_size(100, 100);
        sink.set_cell(85, 10, false, false);
        sink.set_cell(56, 13, true, true);
        let mut a = RandomNumber::with_seed(12345);
        let _ = create_monster_oracle(&mut a, &mut sink, &table, "0", 85, 10, 80, "稻草人");
        assert_eq!(
            (sink.spawns[0].x, sink.spawns[0].y),
            (56, 13),
            "x 抽 Random(50)、y 走 +n20（不抽数）"
        );
        let mut b = RandomNumber::with_seed(12345);
        for m in [1500, 2000, 3000, 3000, 2000] {
            let _ = b.random_below(m);
        }
        let _ = b.random_byte(8);
        let _ = b.random_below(100);
        assert_eq!(b.random_below(100 / 2), 36, "只有 X 抽");
        assert_eq!(a.random(), b.random(), "Y 不得抽数（否则流位置差 1）");
    }

    /// **判据 15c（活栈 · 31 次计数）**：15×40 小图令 X 恒在边界（`Width-n24-1 = -6 < x`）、
    /// Y 恒在边界（Y 重置落 [20,40) ⇒ `Height-n24-1 = 19 ≤ y` 恒假）且全格不可走 ⇒ 每轮**恰 2 抽**，
    /// 走满 31 轮仍不可走 ⇒ `return None`，共 `31×2 = 62` 抽（钉死 `:792` 的 `>= 31`）。
    #[test]
    fn terrain_all_wall_runs_exactly_31_rounds_then_none() {
        let table = MonsterTable::new(vec![info("稻草人", 80, 0, None)]);
        let mut sink = RecordingSink::with_size(15, 40); // 全格默认不可走
        let mut a = RandomNumber::with_seed(12345);
        let id = create_monster_oracle(&mut a, &mut sink, &table, "0", 5, 30, 80, "稻草人");
        assert_eq!(id, None, "31 轮仍不可走 ⇒ oracle `!outofrange ⇒ null`");
        assert!(sink.spawns.is_empty());
        let mut b = RandomNumber::with_seed(12345);
        for m in [1500, 2000, 3000, 3000, 2000] {
            let _ = b.random_below(m);
        }
        let _ = b.random_byte(8);
        let _ = b.random_below(100);
        // 31 轮 × (X 抽 `Random(Width/2)=random_below(7)`，Y 抽 `Random(Height/2)=random_below(20)`)
        for _ in 0..31 {
            let _ = b.random_below(15 / 2);
            let _ = b.random_below(40 / 2);
        }
        assert_eq!(a.random(), b.random(), "必须恰 62 抽（31 轮 × 2）");
    }

    /// **判据 15d（阴性对照）**：初始格可站 ⇒ 门为假 ⇒ **环不进入**（坐标不动、抽数为 0）。
    #[test]
    fn terrain_loop_not_entered_when_start_walkable() {
        let table = MonsterTable::new(vec![info("稻草人", 80, 0, None)]);
        let mut sink = RecordingSink::with_size(100, 100);
        sink.set_cell(85, 85, true, true); // 出生格可站 ⇒ 不进环
        let mut a = RandomNumber::with_seed(12345);
        let _ = create_monster_oracle(&mut a, &mut sink, &table, "0", 85, 85, 80, "稻草人");
        assert_eq!(
            (sink.spawns[0].x, sink.spawns[0].y),
            (85, 85),
            "可站 ⇒ 原地落格（不移位）"
        );
        let mut b = RandomNumber::with_seed(12345);
        for m in [1500, 2000, 3000, 3000, 2000] {
            let _ = b.random_below(m);
        }
        let _ = b.random_byte(8);
        let _ = b.random_below(100);
        assert_eq!(a.random(), b.random(), "环不进入 ⇒ 地形支不抽任何数");
    }

    /// **判据 15e（活栈 · 宽 ≠ 高，钉死 X 用 `Width/2`、Y 用 `Height/2` 不可对调）**：15c 的全墙场景下
    /// `random_below(max)` 的**消耗采样数**与 `max` 无关（`max` 只缩放返回值，`internal_sample` 每次恰 1），
    /// 故对调 `Width/2`/`Height/2` 不改流位置——必须在**宽 ≠ 高**时用**位置断言**才钉得住。
    ///
    /// 期望表（seed=12345，race 80，地图 100×60 ⇒ `n20=3`、`n24=20`；起点 (85,45)）：
    /// 构造链 5 抽 `[100,140,2324,1533,1594]` + `Dir=6` + `CoolEye=16` → `Random(Width/2=50)=36`（X）
    /// → `Random(Height/2=30)=7`（Y）⇒ 落格 `(36+20, 7+20) = (56, 27)`；X/Y 对调则落 `(42, 33)`。
    #[test]
    fn terrain_boundary_uses_width_for_x_height_for_y() {
        let table = MonsterTable::new(vec![info("稻草人", 80, 0, None)]);
        let mut sink = RecordingSink::with_size(100, 60);
        sink.set_cell(85, 45, false, false);
        sink.set_cell(56, 27, true, true);
        let mut a = RandomNumber::with_seed(12345);
        let _ = create_monster_oracle(&mut a, &mut sink, &table, "0", 85, 45, 80, "稻草人");
        assert_eq!(
            (sink.spawns[0].x, sink.spawns[0].y),
            (56, 27),
            "X 抽 Random(Width/2)、Y 抽 Random(Height/2)（对调则 (42,33)）"
        );
        let mut b = RandomNumber::with_seed(12345);
        for m in [1500, 2000, 3000, 3000, 2000] {
            let _ = b.random_below(m);
        }
        let _ = b.random_byte(8);
        let _ = b.random_below(100);
        assert_eq!(b.random_below(100 / 2), 36, "X 抽 = Random(Width/2)");
        assert_eq!(b.random_below(60 / 2), 7, "Y 抽 = Random(Height/2)");
        assert_eq!(a.random(), b.random(), "取数序列逐项一致");
    }

    /// **判据 13（逐 race 构造抽数）**：每条 race 的构造链总抽数（含公共链
    /// `BaseObject`+`AnimalObject`）与 oracle 逐行数出的一致；不在 switch 内的 race 不抽任何数。
    /// （race 96 因 ctor 内条件抽数（`Random(3)==0`）不在此表，由判据 14 的两支覆盖。）
    #[test]
    fn monster_constructor_draw_count_matches_oracle_per_race() {
        let table: &[(i16, usize)] = &[
            // `GuardUnit`/`NormNpc` 链：`BaseObject`(2)+`AnimalObject`(2)。
            (11, 4),
            (12, 4),
            (20, 4),
            (55, 4),
            (110, 4),
            (111, 4),
            (112, 4),
            // switch 内联 + 构造链（`:522`/`:532`/`:541`/`:550`）。
            (51, 6),
            (52, 7),
            (53, 7),
            // `MonsterObject`/`AtMonster`/各派生（逐 ctor 实读 `src/M2Server/Monster/**`）。
            (80, 5),
            (81, 6),
            (82, 7),
            (83, 6),
            (84, 6),
            (85, 5),
            (86, 6),
            (87, 5),
            (88, 6),
            (89, 6),
            (90, 7),
            (91, 7),
            (92, 7),
            (93, 5),
            (94, 6),
            (95, 7),
            (97, 8),
            (100, 6),
            (101, 6),
            (102, 6),
            (103, 5),
            (104, 5),
            (105, 7),
            (106, 7),
            (107, 5),
            (113, 5),
            (114, 7),
            (115, 4),
            (116, 5),
            (117, 6),
            (118, 7),
            (119, 7),
            (120, 4),
            (130, 6),
            (131, 6),
            (132, 5),
            (133, 6),
            (134, 6),
            (200, 6),
            (201, 6),
            (203, 6),
            (206, 6),
            (208, 6),
            (209, 6),
            (210, 6),
            (214, 6),
            (215, 7),
        ];
        for &(race, n) in table {
            let mut a = RandomNumber::with_seed(13579);
            let _ = draw_monster_construct(&mut a, race);
            let mut b = RandomNumber::with_seed(13579);
            for _ in 0..n {
                let _ = b.random_below(1_000_000);
            }
            assert_eq!(a.random(), b.random(), "race {race} 构造抽数应为 {n}");
        }
        // 不在 switch 内的 race ⇒ 0 抽（`cert == null`，`WorldServer.MonGen.cs:731-734`）。
        for race in [
            0i16, 1, 2, 3, 10, 13, 50, 54, 108, 109, 121, 129, 135, 153, 199, 202, 204, 205, 207,
            211, 212, 213, 216, 255,
        ] {
            let mut a = RandomNumber::with_seed(24680);
            let _ = draw_monster_construct(&mut a, race);
            let mut b = RandomNumber::with_seed(24680);
            assert_eq!(a.random(), b.random(), "race {race} 不应抽任何数");
        }
    }

    /// **卡 `openmir2-run-gap-monster-specials`（RNG 判据）**：三种专用怪的 ctor
    /// `Random(1500)` 与 oracle **同点同次**抽。
    ///
    /// - `BeeQueen : AnimalObject`（`BeeQueen.cs:9`）：Base 2 抽 + Animal 2 抽 → 派生
    ///   `SearchTime = Random(1500) + 2500`（`BeeQueen.cs:17`）。
    /// - `SpiderHouseMonster : AnimalObject`（`SpiderHouseMonster.cs:9`）：同序、同式（`:17`）。
    /// - `CloneMonster : MonsterObject`（`Clone.cs:7`）：Base 2 + Animal 2 + MonsterObject 1
    ///   → 派生 `SearchTime = Random(1500) + 1500`（`Clone.cs:11`）。
    #[test]
    fn special_monster_ctor_draws_match_oracle_order() {
        // 103 / 116：第 5 抽（索引 4）是派生 ctor 的 `Random(1500)`。
        for race in [103i16, 116] {
            let mut a = RandomNumber::with_seed(777);
            let d = draw_monster_construct(&mut a, race);
            let mut b = RandomNumber::with_seed(777);
            let _run_tick = b.random_below(1500); // BaseObject.cs:439
            let _base_search = b.random_below(2000) + 2000; // BaseObject.cs:441
            let _attack = b.random_below(3000); // AnimalObject.cs:76
            let _walk = b.random_below(3000); // AnimalObject.cs:77
            let expected = b.random_below(1500) + 2500; // BeeQueen.cs:17 / SpiderHouse:17
            assert_eq!(
                d.search_time, expected,
                "race {race} 的派生 SearchTime 位置/基值"
            );
            assert_eq!(a.random(), b.random(), "race {race} 之后不得再抽/少抽");
        }
        // 201：Base 2 + Animal 2 + MonsterObject 1 → 派生 `Random(1500) + 1500`。
        let mut a = RandomNumber::with_seed(777);
        let d = draw_monster_construct(&mut a, 201);
        let mut b = RandomNumber::with_seed(777);
        let _run_tick = b.random_below(1500);
        let _base_search = b.random_below(2000) + 2000;
        let _attack = b.random_below(3000);
        let _walk = b.random_below(3000);
        let _monster_search = b.random_below(2000) + 3000; // MonsterObject.cs:65
        let expected = b.random_below(1500) + 1500; // Clone.cs:11
        assert_eq!(d.search_time, expected, "Clone 的派生 SearchTime 位置/基值");
        assert_eq!(a.random(), b.random(), "Clone 之后不得再抽/少抽");
    }

    /// **判据 14（构造抽值逐抽钉死）**：`MonsterCtorDraws` 的每个字段都对应 oracle 具体 ctor 的那一抽。
    /// 覆盖守卫链（`SearchTime` 停在 `BaseObject` 值）与 race 96 的条件支（`Random(3)==0`）。
    #[test]
    fn monster_constructor_values_match_oracle_order() {
        // race 11 守卫：BaseObject→AnimalObject→NormNpc→GuardUnit，无派生 ctor 抽。
        {
            let table = MonsterTable::new(vec![info("守卫", 11, 0, None)]);
            let mut sink = RecordingSink::with_size(100, 100);
            sink.set_cell(0, 0, true, true);
            let mut a = RandomNumber::with_seed(2025);
            let _ = create_monster_oracle(&mut a, &mut sink, &table, "0", 0, 0, 11, "守卫");
            let mut b = RandomNumber::with_seed(2025);
            let run = b.random_below(1500);
            let base = b.random_below(2000) + 2000;
            let atk = b.random_below(3000);
            let walk = b.random_below(3000);
            let _ = b.random_byte(8);
            let _ = b.random_below(100);
            let c = sink.spawns[0].ctor;
            assert_eq!(c.run_tick_delta, run);
            assert_eq!(c.base_search_time, base);
            assert_eq!(c.attack_tick_delta, atk);
            assert_eq!(c.walk_tick_delta, walk);
            assert_eq!(c.search_time, base, "守卫的 SearchTime 停在 BaseObject 值");
            assert_eq!(c.meat_quality, 0);
            assert!(!c.bo_fear_fire);
            assert_eq!(c.zil_kill_count, 0);
            assert_eq!(a.random(), b.random());
        }
        // race 96 ZilKinZombi：构造链 7 抽 + ctor 条件支（`Random(3)==0 ⇒ Random(3)+1`）+ 内联 `Random(4)`。
        for seed in [1i32, 2, 3, 4, 5, 6] {
            let table = MonsterTable::new(vec![info("僵尸", 96, 0, None)]);
            let mut sink = RecordingSink::with_size(100, 100);
            sink.set_cell(0, 0, true, true);
            let mut a = RandomNumber::with_seed(seed);
            let _ = create_monster_oracle(&mut a, &mut sink, &table, "0", 0, 0, 96, "僵尸");
            let mut b = RandomNumber::with_seed(seed);
            let _ = b.random_below(1500);
            let _ = b.random_below(2000);
            let _ = b.random_below(3000);
            let _ = b.random_below(3000);
            let _ = b.random_below(2000);
            let _ = b.random_below(1500);
            let exp_search = b.random_below(1500) + 2500;
            let exp_zil = if b.random_below(3) == 0 {
                b.random_below(3) + 1
            } else {
                0
            };
            let exp_fear = b.random_below(4) == 0;
            let _ = b.random_byte(8);
            let _ = b.random_below(100);
            let c = sink.spawns[0].ctor;
            assert_eq!(c.search_time, exp_search, "seed {seed} SearchTime");
            assert_eq!(c.zil_kill_count, exp_zil, "seed {seed} ZilKillCount");
            assert_eq!(c.bo_fear_fire, exp_fear, "seed {seed} BoFearFire");
            assert_eq!(a.random(), b.random(), "seed {seed} 流位置");
        }
    }

    /// 判据 7：`entries_from_envir` 逐字段搬运 + 运行时列清零。
    #[test]
    fn entries_from_envir_field_mapping() {
        let src = mir2_envir::MonGenInfo {
            map_name: "3".to_string(),
            x: 294,
            y: 341,
            mon_name: "神鹰".to_string(),
            range: 10,
            count: 1,
            active_count: 0,
            zen_time: 360_000,
            mission_gen_rate: 0,
            race: 0,
            start_tick: 0,
        };
        let out = entries_from_envir(std::slice::from_ref(&src));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].map_name, "3");
        assert_eq!(out[0].x, 294);
        assert_eq!(out[0].y, 341);
        assert_eq!(out[0].mon_name, "神鹰");
        assert_eq!(out[0].range, 10);
        assert_eq!(out[0].count, 1);
        assert_eq!(out[0].zen_time, 360_000);
        assert!(out[0].cert_list.is_empty());
        assert_eq!(out[0].thread_id, 0);
    }

    /// 判据 8：配置解析逐键（含 BOM）——写一个临时 conf。
    #[test]
    fn mon_gen_config_reads_oracle_keys_with_bom() {
        let mut body = Vec::new();
        body.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        body.extend_from_slice(
            b"[Server]\nMonLimit=10\nZenLimit=5\nUserFull=3000\nZenFastStep=300\nVentureServer=FALSE\n\
              [Setup]\nGenMonRate=10\nProcessMonsterInterval=2\n",
        );
        let path = std::env::temp_dir().join(format!(
            "mir2-mongen-conf-{}-{}.conf",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::write(&path, &body).expect("写临时 conf");
        let c = mon_gen_config_from_conf(&path).expect("解析");
        std::fs::remove_file(&path).ok();
        assert_eq!(c.mon_limit, 10);
        assert_eq!(c.zen_limit, 5);
        assert_eq!(c.user_full, 3000);
        assert_eq!(c.zen_fast_step, 300);
        assert_eq!(c.mon_gen_rate, 10);
        assert_eq!(c.process_monster_interval, 2);
        // 未覆盖键回落到 `GameSvrConf` 构造缺省。
        assert_eq!(c.process_monster_multi_thread_limit, 1);
        assert_eq!(c.regen_monsters_time, 200);
        assert!(!c.venture_server);
    }

    /// 判据 9：`MonsterTable` 的大小写不敏感查表与 `race_of` 未命中 -1。
    #[test]
    fn monster_table_lookup_is_ordinal_ignore_case() {
        let t = MonsterTable::new(vec![info("鸡", 51, 0, None), info("Deer", 52, 0, None)]);
        assert_eq!(t.race_of("鸡"), 51);
        assert_eq!(t.race_of("deer"), 52, "OrdinalIgnoreCase");
        assert_eq!(t.race_of("not-there"), -1);
        assert_eq!(t.name_list(), vec!["鸡".to_string(), "Deer".to_string()]);
    }

    /// 判据 10（端到端构造层）：`MonGenHost::start` + `step` 在构造输入上刷出一只怪，
    /// 且 `MonGenEntry.active_count` / `cert_list` 按 oracle 同步。
    #[test]
    fn host_start_and_step_regen_one_monster() {
        let table = MonsterTable::new(vec![info("鸡", 51, 0, None)]);
        let entries = vec![MonGenEntry::new("0", 50, 50, "鸡", 0, 1, 60_000, 0)];
        let mut rng = RandomNumber::with_seed(11);
        let mut host = MonGenHost::start(entries, &table, false, MonGenConfig::default(), &mut rng);
        assert_eq!(host.init.distinct_mon_names, 1);
        assert!(!host.init.sentinel_present);

        let mut sink = RecordingSink::with_size(100, 100);
        sink.set_cell(50, 50, true, true);
        let mut clock = || 0;
        let mut can = |_: &str| true;
        // 第一 tick：StartTick == 0 ⇒ 立即刷（`get_monsters_zen_time` 不挡）。
        let d = host.step(1000, 0, &mut rng, &mut clock, &mut can, &mut sink, &table);
        assert_eq!(d.len(), 1);
        assert!(d[0].regened);
        assert!(d[0].regen_result);
        assert!(d[0].start_tick_updated);
        assert_eq!(host.state.entries[0].active_count, 1);
        assert_eq!(host.state.entries[0].cert_list.len(), 1);

        // 第二 tick：`ActiveCount == genModCount` ⇒ 不再刷（不调回调）。
        let spent = sink.spawns.len();
        let _ = host.step(2000, 0, &mut rng, &mut clock, &mut can, &mut sink, &table);
        assert_eq!(sink.spawns.len(), spent, "active_count 已满 ⇒ 不刷");
    }

    /// 判据 11：末尾哨兵（`LocalDB.cs:570-590`）占 thread 0 首槽，首个 regen 门选它但不刷。
    #[test]
    fn host_sentinel_first_slot_is_skipped_by_regen() {
        let table = MonsterTable::new(vec![info("鸡", 51, 0, None)]);
        let entries = vec![MonGenEntry::new("0", 50, 50, "鸡", 0, 1, 60_000, 0)];
        let mut rng = RandomNumber::with_seed(11);
        let mut host = MonGenHost::start(entries, &table, true, MonGenConfig::default(), &mut rng);
        assert!(host.init.sentinel_present);
        assert_eq!(host.state.entries.len(), 2, "解析 1 条 + 哨兵 1 条");
        let mut sink = RecordingSink::with_size(100, 100);
        sink.set_cell(50, 50, true, true);
        let mut clock = || 0;
        let mut can = |_: &str| true;
        let d = host.step(1000, 0, &mut rng, &mut clock, &mut can, &mut sink, &table);
        assert!(d[0].gate_due);
        assert_eq!(d[0].entry_index, 1, "哨兵在 entries 末尾且占 thread 0 首槽");
        assert!(!d[0].regened, "哨兵空怪名 ⇒ 不刷");
        assert!(sink.spawns.is_empty());
    }

    /// 判据 12：`venture_server = true` ⇒ 不刷（oracle `!VentureServer` 门）。
    #[test]
    fn host_venture_server_disables_regen() {
        let table = MonsterTable::new(vec![info("鸡", 51, 0, None)]);
        let entries = vec![MonGenEntry::new("0", 50, 50, "鸡", 0, 1, 60_000, 0)];
        let cfg = MonGenConfig {
            venture_server: true,
            ..MonGenConfig::default()
        };
        let mut rng = RandomNumber::with_seed(11);
        let mut host = MonGenHost::start(entries, &table, false, cfg, &mut rng);
        let mut sink = RecordingSink::with_size(100, 100);
        sink.set_cell(50, 50, true, true);
        let mut clock = || 0;
        let mut can = |_: &str| true;
        let d = host.step(1000, 0, &mut rng, &mut clock, &mut can, &mut sink, &table);
        assert!(d[0].gate_due);
        assert!(!d[0].regened);
        assert!(sink.spawns.is_empty());
    }

    /// **C 第 ② 片判据 1（宿主开闸）**：生产宿主必须开怪物 AI；合成 golden 不经此函数 ⇒ 缺省仍关。
    ///
    /// 红检：把 [`enable_production_monster_ai`] 改成空函数 ⇒ 本用例必红（`monster_ai_enabled()` 为
    /// false）。
    #[test]
    fn production_host_enables_monster_ai() {
        let mut world = mir2_world::World::new(4, 4);
        assert!(
            !world.monster_ai_enabled(),
            "缺省必须关（既有 golden/合成场景逐字节不变）"
        );
        enable_production_monster_ai(&mut world);
        assert!(
            world.monster_ai_enabled(),
            "生产宿主必须开闸（World::tick 才会驱动 monster_ai_tick）"
        );
    }

    /// **C 第 ② 片判据 2（生成期取数落进 AI 初始态）**：`WorldMonsterSink::spawn_monster` 把
    /// `dir`/`cool_eye`/构造抽（含 `AttackTick`/`WalkTick`/`SearchTime` 的随机分量）写进 `Entity.ai`，
    /// 且**不重抽**。
    ///
    /// 红检：把 sink 的 `req.dir, req.cool_eye` 改成 `0, false` ⇒ `ai.dir`/`ai.cool_eye` 断言必红。
    #[test]
    fn world_monster_sink_seeds_ai_from_spawn_request() {
        let base = std::env::temp_dir().join(format!(
            "gs-mongen-sink-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        // 5×5 全零地形：`parse_map_bytes` 只要求 width/height > 1 且字节足够。
        let mut bytes = vec![0u8; mir2_envir::HEADER_BYTES];
        bytes[0..2].copy_from_slice(&5_i16.to_le_bytes());
        bytes[2..4].copy_from_slice(&5_i16.to_le_bytes());
        bytes.resize(mir2_envir::HEADER_BYTES + 5 * 5 * mir2_envir::CELL_BYTES, 0);
        std::fs::create_dir_all(base.join("Map")).unwrap();
        std::fs::write(base.join("Map").join("0.map"), &bytes).unwrap();
        let loader = MapLoader::new(base.clone(), "Map", "Envir");
        let registry = MapRegistry::from_entries(
            loader,
            vec![mir2_envir::MapInfoEntry {
                map_name: "0".to_string(),
                map_file_name: "0".to_string(),
                desc: String::new(),
                server_index: 0,
                flag: mir2_envir::MapInfoFlag::default(),
            }],
        );
        assert_eq!(registry.id_of("0"), Some(0), "注册表必须命中 map 0");

        let mut world = mir2_world::World::with_maps(vec![(5, 5)]);
        world.clock_ms = 12_345;
        let table = MonsterTable::default();
        let ctor = MonsterCtorDraws {
            attack_tick_delta: 33,
            walk_tick_delta: 44,
            search_time: 3333,
            bo_fear_fire: true,
            ..MonsterCtorDraws::default()
        };
        let req = MonsterSpawnRequest {
            map_name: "0".to_string(),
            mon_name: "鸡".to_string(),
            x: 2,
            y: 2,
            race: 51,
            dir: 5,
            cool_eye: true,
            gold: 7,
            drop_items: Vec::new(),
            ctor,
        };
        let mut displays = downlink::DisplayTable::new();
        let id = WorldMonsterSink {
            world: &mut world,
            registry: &registry,
            table: &table,
            displays: &mut displays,
        }
        .spawn_monster(&req)
        .expect("生成成功");
        let e = world.entities.get(id).expect("怪物在仓库");
        assert_eq!(e.ai.dir, 5, "cert.Dir 必须落进 Entity.ai.dir");
        assert!(e.ai.cool_eye, "CoolEye 必须落进 Entity.ai.cool_eye");
        assert!(e.ai.bo_fear_fire, "构造抽 BoFearFire 必须落进 Entity.ai");
        assert_eq!(e.ai.attack_tick, 12_345 - 33);
        assert_eq!(e.ai.walk_tick, 12_345 - 44);
        assert_eq!(e.ai.search_time, 3333);
        assert_eq!(e.ctor_draws, ctor, "构造抽结果必须原样落库（C 只消费不抽）");
        assert_eq!(e.gold, 7, "生成期已预算的金币原样落库");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **C 第 ③ 片判据**：`WorldMonsterSink::spawn_monster` 成功后把怪物展示层登记进
    /// `DisplayTable`（`feature`/`show_name`/`name_color`/`light` 逐字段按 oracle）。
    ///
    /// 红检：删掉 sink 里的 `self.displays.set(...)` ⇒ 本用例必红（`displays.get(id)` 为 None）；
    /// 把 `monster_display` 的 `LoByte(mp)` 换成整 `mp` ⇒ `feature` 断言必红。
    #[test]
    fn world_monster_sink_registers_monster_display() {
        let base = std::env::temp_dir().join(format!(
            "gs-mongen-display-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut bytes = vec![0u8; mir2_envir::HEADER_BYTES];
        bytes[0..2].copy_from_slice(&5_i16.to_le_bytes());
        bytes[2..4].copy_from_slice(&5_i16.to_le_bytes());
        bytes.resize(mir2_envir::HEADER_BYTES + 5 * 5 * mir2_envir::CELL_BYTES, 0);
        std::fs::create_dir_all(base.join("Map")).unwrap();
        std::fs::write(base.join("Map").join("0.map"), &bytes).unwrap();
        let loader = MapLoader::new(base.clone(), "Map", "Envir");
        let registry = MapRegistry::from_entries(
            loader,
            vec![mir2_envir::MapInfoEntry {
                map_name: "0".to_string(),
                map_file_name: "0".to_string(),
                desc: String::new(),
                server_index: 0,
                flag: mir2_envir::MapInfoFlag::default(),
            }],
        );
        let mut world = mir2_world::World::with_maps(vec![(5, 5)]);
        let table = MonsterTable::new(vec![MonsterInfo {
            name: "稻草人2".to_string(),
            race: 51,
            race_img: 0x0A,
            appr: 0x3344,
            mp: 0x2211,
            ..MonsterInfo::default()
        }]);
        let req = MonsterSpawnRequest {
            map_name: "0".to_string(),
            mon_name: "稻草人2".to_string(),
            x: 2,
            y: 2,
            race: 51,
            dir: 5,
            cool_eye: false,
            gold: 0,
            drop_items: Vec::new(),
            ctor: MonsterCtorDraws::default(),
        };
        let mut displays = downlink::DisplayTable::new();
        let id = WorldMonsterSink {
            world: &mut world,
            registry: &registry,
            table: &table,
            displays: &mut displays,
        }
        .spawn_monster(&req)
        .expect("生成成功");
        let d = displays
            .get(id)
            .expect("怪物必须登记展示层（否则 AOI 下行跳过）");
        assert_eq!(d.dir, 5, "登记时的 Dir 取 req.dir（= cert.Dir）");
        assert_eq!(d.light, 0);
        assert_eq!(
            d.feature,
            crate::feature::make_monster_feature(0x0A, 0x11, 0x3344),
            "MakeLong(MakeWord(raceImg, LoByte(mp)), appr)"
        );
        assert_eq!(d.feature_ex, 0);
        assert_eq!(d.status, 0);
        assert_eq!(d.name_color, downlink::MONSTER_NAME_COLOR);
        assert_eq!(d.show_name, downlink::gbk("稻草人"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 尸体/复活片：只把**本轮追加**的 cert 登记为可复活（oracle `RegenMonsters` 的三字段），
    /// 已在 cert_list 里的旧成员（如 GuardList 放置，oracle `RegenMonsterByName` 不置位）保持不动。
    #[test]
    fn bind_new_certificates_only_marks_appended_ids() {
        use mir2_world::{Entity, MonGenEntry, MonGenState, MonGenThread};
        let mut world = mir2_world::World::new(64, 64);
        let old = world.add_entity(Entity::new(0, "旧", 5, 5));
        let new_id = world.add_entity(Entity::new(0, "新", 6, 6));

        let mut state = MonGenState::default();
        let mut entry = MonGenEntry::new("0", 5, 5, "鸡", 0, 1, 60_000, 0);
        entry.cert_list = vec![old, new_id];
        state.entries.push(entry);
        state.threads.insert(
            0,
            MonGenThread {
                id: 0,
                entries: vec![0],
                ..MonGenThread::default()
            },
        );

        // before = [1] ⇒ 只登记下标 1 之后追加的 `new_id`。
        let bound = bind_new_certificates(&state, &mut world, &[1], 1_234);
        assert_eq!(bound, vec![new_id]);
        let o = world.entities.get(old).unwrap();
        assert!(!o.can_re_alive && o.mon_gen.is_none(), "旧成员不得被登记");
        let n = world.entities.get(new_id).unwrap();
        assert!(n.can_re_alive);
        assert_eq!(n.mon_gen, Some(0));
        assert_eq!(n.mon_gen_zen_time, 60_000);
        assert_eq!(n.re_alive_tick, 1_234);
    }
}
