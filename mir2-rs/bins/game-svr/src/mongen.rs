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
//! 5. **`CreateMonster` 回调**（`WorldServer.MonGen.cs:498-810`）的 **RNG 取数次序**：
//!    per-race 抽 → `Dir = RandomByte(8)` → `MonGetRandomItems`（掉落预算）→ `Random(100) < CoolEyeCode`
//!    → 地形移位。**实体类的行为面**（`ChickenDeer`/`AtMonster`/守卫…的 AI）属承接卡
//!    `openmir2-m3-monster-ai-tick`；本层只负责「取数次序 + 落到世界」。
//!
//! ## 与 oracle 的建模差异（登记）
//!
//! - **线程节拍**：oracle 每线程 `Thread.Sleep(20)`（`WorldServer.MonGen.cs:340`）⇒ regen 门
//!   约每 `RegenMonstersTime + 20ms` 触发一次；本宿主在**世界 tick（200ms）**里调一次
//!   [`MonGenHost::step`]（单线程确定性，设计文档 §4.2 硬约束①）。总刷怪数由 `genModCount`
//!   封顶（每分钟的 `Count`），故长跑收敛；**短窗内的 interleaving 与 oracle 不同**——这条已由
//!   第 ② 片登记，Claim B 的 site/order 账本据此对齐。
//! - **`dir`/`cool_eye`**：本层按 oracle 取数并随 [`MonsterSpawnRequest`] 交给 sink；world 的
//!   `Entity` 尚未建模这两个字段（AOI 面属承接卡），故 `WorldMonsterSink` 落库时**不入实体内存**。
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

/// `Server.conf` 缺省：`[Server] VentureServer`（`ServerConf.cs:21`，`"FALSE"` ⇒ false）。
pub const VENTURE_SERVER_DEFAULT: &str = "FALSE";

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

/// switch 内**带额外抽数**的 race（其余 race 无此段）。
fn draw_race_specific(rng: &mut RandomNumber, race: i16) {
    match race {
        // `:511` `AnimalChicken`：`Random(3500) + 3000` 的 `MeatQuality`。
        51 => {
            let _ = rng.random_below(3500);
        }
        // `:520-539` `AnimalDeer`：`Random(30) == 0` 时 `Random(20000)`，否则 `Random(8000)`。
        52 => {
            if rng.random_below(30) == 0 {
                let _ = rng.random_below(20000);
            } else {
                let _ = rng.random_below(8000);
            }
        }
        // `:542` `AnimalWolf`：`Random(8000)`。
        53 => {
            let _ = rng.random_below(8000);
        }
        // `:594-598` `MonsterDigoutzombi`：`Random(2) == 0 ⇒ BoFearFire`。
        95 => {
            let _ = rng.random_below(2);
        }
        // `:611-615` `MonsterZilkinzombi`：`Random(4) == 0 ⇒ BoFearFire`。
        96 => {
            let _ = rng.random_below(4);
        }
        // `:618-622` `case 97`（`CowMonster`）：`Random(2) == 0 ⇒ BoFearFire`。
        97 => {
            let _ = rng.random_below(2);
        }
        _ => {}
    }
}

/// [`MonsterCreator`] 回调的生产实现：`WorldServer.MonGen.cs:498-810` 的**取数次序**。
///
/// 次序（判据逐抽钉死）：
/// 1. `Envir.FindMap(sMapName) == null` ⇒ `None`（**不抽**）；
/// 2. race 不在 switch 内 ⇒ `None`（**不抽**）；
/// 3. per-race 抽（[`draw_race_specific`]，仅 51/52/53/95/96/97）；
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
    draw_race_specific(rng, race);

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

/// 生产 sink：把 [`MonsterSpawnRequest`] 落到 `crates/world` 的实体仓库。
///
/// 能力字段照 oracle `ApplyMonsterAbility`（`WorldServer.MonGen.cs:880-910`）：
/// `Level = monster.Level`、`HP/MaxHP = monster.HP`、`AC = AC`、`DC = MakeWord(DC, MaxDC)`、
/// `LifeAttrib = monster.btLifeAttrib`、`HitPoint = monster.HitPoint`、`SpeedPoint = monster.Speed`、
/// `FightExp = monster.Exp`。**`dir`/`cool_eye` 不入实体内存**（建模差异，见模块头）。
pub struct WorldMonsterSink<'a> {
    pub world: &'a mut mir2_world::World,
    pub registry: &'a MapRegistry,
    pub table: &'a MonsterTable,
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
        self.world.spawn_monster_precomputed(
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

    /// **判据 1（per-race 抽数次序）**：race 51 的 `create_monster_oracle` 之后，
    /// 随机流的下一抽必须与「手工按 oracle 次序复算」完全一致。
    #[test]
    fn create_monster_chicken_consumes_random_3500_then_dir_drops_cool_eye() {
        let table = MonsterTable::new(vec![
            info("鸡", 51, 0, None),
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

        // 手工复算（oracle 次序）：Random(3500) → RandomByte(8) → Random(100)。
        let mut b = RandomNumber::with_seed(4242);
        let _ = b.random_below(3500);
        let exp_dir = b.random_byte(8);
        let exp_cool = b.random_below(100);
        // 随机流位置对齐：下一抽两侧相同。
        assert_eq!(a.random(), b.random(), "抽数个数/次序与 oracle 不一致");
        assert_eq!(sink.spawns[0].dir, exp_dir);
        assert_eq!(sink.spawns[0].cool_eye, exp_cool < 0);
    }

    /// 判据 2：race 52 的 `Random(30) == 0` 分支决定后续是 `Random(20000)` 还是 `Random(8000)`。
    #[test]
    fn create_monster_deer_branch_matches_oracle() {
        let table = MonsterTable::new(vec![info("鹿", 52, 0, None)]);
        // 直接构造两个独立流逐步验证「命中 0 分支」的抽数：
        let mut a = RandomNumber::with_seed(7);
        let mut b = RandomNumber::with_seed(7);
        let mut sink = RecordingSink::with_size(100, 100);
        sink.set_cell(0, 0, true, true);
        let _ = create_monster_oracle(&mut a, &mut sink, &table, "0", 0, 0, 52, "鹿");
        // 手工：Random(30) 若为 0 ⇒ Random(20000)，否则 Random(8000)。
        let first = b.random_below(30);
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
        // 手工序列 = Dir + CoolEye。
        let _ = b.random_byte(8);
        let _ = b.random_below(100);
        assert_eq!(a.random(), b.random());
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
}
