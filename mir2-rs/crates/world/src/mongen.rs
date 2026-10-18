//! MonGen 刷怪调度 —— 1:1 复刻 C# `GameSvr/Word/WorldServer.MonGen.cs` 的
//! **分组 / 分片 / regen 门 / 复活节拍 / `RegenMonsters` 取数次序**。
//!
//! 交付卡：`openmir2-m3-mongen-load-spawn`（M3④-实现 B）第 ② 片（`crates/world`）。
//! 第 ① 片（`crates/envir` 的 `MonGenInfo` + `Envir/MonGen.txt` 解析）见
//! `mir2-rs/crates/envir/src/mongen.rs`。
//!
//! 逐块 oracle 锚点（行号按 master `1b576172`，**已逐条在 `src/` 实读过**）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`MonGenConfig`] | `GameSvrConf.cs:32/64/68/329/334/1127/1142` + `ServerConf.cs:21/63/64/66/261/636/637` |
//! | [`MonGenEntry`] | `SystemModule/Data/MonGenInfo.cs:9-57`（`ThreadMonLimit=40000`、`TryAdd` 的 `>`） |
//! | [`initialize_monster`] | `WorldServer.MonGen.cs:30-114` |
//! | [`get_monsters_zen_time`] | 同文件 `:168-190` |
//! | [`monster_due_for_revive`] | 同文件 `:283-291`（`Death && CanReAlive && Invisible && MonGen != null`） |
//! | [`gen_mod_count`] | 同文件 `:232` |
//! | [`should_regen`] / [`regen_gate`] | 同文件 `:200-253` |
//! | [`regen_monsters`] | 同文件 `:811-874` |
//!
//! ## RNG 取数次序（Claim B 两侧 site/order 账本的输入；**不许为可测性改序**）
//!
//! 1. [`initialize_monster`]：先按**怪名**（`monsterGenMap.Keys` 的插入序）每名 1 次
//!    `Random(ProcessMonsterMultiThreadLimit)`；再对 `MonsterList.Values` 各 1 次。加载/解析本身不抽。
//! 2. [`regen_gate`]：`MissionGenRate > 0` 时先 1 次 `Random(100)`；命中「集中」支 ⇒ 定中心 2 次
//!    `Random(Range*2+1)`、每只 2 次 `Random(20)`；否则普通支每只 2 次 `Random(Range*2+1)`。
//! 3. 每只 `CreateMonster` 内部再抽（`RandomByte(8)` 的 Dir、`MonGetRandomItems` 预算、`Random(100)` 的
//!    CoolEye、地形不可站时的移位）——本模块把这几抽留在 [`regen_monsters`] 的 `create_monster`
//!    回调里，**回调在两次位置抽之间被同步调用**，因此两侧的 interleaving 结构逐调用相同。
//!    回调的 RNG 次序契约见 [`regen_monsters`] 的文档。
//!
//! ## 与 oracle 的建模差异（登记）
//!
//! - **线程模型**：oracle 每条 `ProcessMonsters` 跑在独立 `Thread` 上（`WorldServer.MonGen.cs:138-166`），
//!   墙钟 + `Thread.Sleep(20)` 的调度不可复现。本 crate 单线程串行（设计文档 §4.2 硬约束①），
//!   把「每线程一步」按 **thread id 升序** 定时驱动（[`MonGenState::step_threads`]）。部署
//!   `ProcessMonsterMultiThreadLimit = 1`（`E:/MirServer/M2GameSvr/Server.conf` 无该键 ⇒
//!   `GameSvrConf.cs:1896` 默认 1）⇒ oracle 侧同样只有一条线程、串行，两者一致。
//! - **`MonGenInfoThreadMap` 的穿插遍历**：oracle `:101-108` 用 `for i in 0..Count` 直接下标
//!   `MonGenInfoThreadMap[i]`，当 `limit > 1` 且某个 id 未被抽中时 .NET 会抛
//!   `KeyNotFoundException`（部署 `limit=1` 不触发）。本实现按 **thread id 升序**遍历实际存在的线程，
//!   并把该 oracle 缺陷登记在此（不"顺手修"，也不引入 panic）。
//! - **`Race` 回填**：oracle `:103-110` 在分片完成后按怪名回填 `MonGenInfo.Race = GetMonRace(name)`
//!   （未命中 = `-1`，`WorldServer.cs:167-174`）。本实现同口径，用 `race_of` 回调喂表。
//! - **`MonGenList.Clear()`**：oracle `:112` 清掉原始配置列表（运行时只留 thread map）。本实现把
//!   配置列表**整体搬进** [`MonGenState::entries`]（所有权转移即 `Clear` 的等价物）。

use std::collections::BTreeMap;

use crate::entity::Entity;
use mir2_shared::hutil::{max_i32, round};
use mir2_shared::rng::RandomNumber;

/// `MonGenConst.ThreadMonLimit`（`SystemModule/Data/MonGenInfo.cs:68`）。
///
/// `MonGenInfo.TryAdd`（`:41-57`）在 `CertList.Count > ThreadMonLimit` 时返回 false
/// —— 注意是 **`>`**：恰好等于上限时**仍允许加入**（最多可到 40001）。
pub const MON_GEN_THREAD_MON_LIMIT: usize = 40000;

/// `GetMonstersZenTime` 的 30 分钟阈值（`WorldServer.MonGen.cs:170`）。
pub const ZEN_TIME_FAST_THRESHOLD_MS: i32 = 30 * 60 * 1000;

/// `ProcessMonsters` 的 `Thread.Sleep(20)`（`WorldServer.MonGen.cs:340`）。
pub const MONSTER_THREAD_SLEEP_MS: i32 = 20;

/// 复活的「隐身后冷却」= `5 * 60 * 1000`（`WorldServer.MonGen.cs:302`）。
pub const MONSTER_GHOST_EXPIRE_MS: i32 = 5 * 60 * 1000;

/// `CreateMonster` 回调（oracle `WorldServer.MonGen.cs:498`）：同步创建一只怪并返回其 actor id。
///
/// 实现方（`bins/game-svr` 接线或测试 sink）应照 oracle 在该回调内消耗的 RNG
/// （`RandomByte(8)` 的 Dir → `MonGetRandomItems` 预算 → `Random(100)` 的 CoolEye → 地形移位）。
pub type MonsterCreator<'a> =
    dyn FnMut(&mut RandomNumber, &str, i16, i16, i16, &str) -> Option<i32> + 'a;

/// `CreateMonster`（`WorldServer.MonGen.cs:504-729`）里派生类构造函数设置的
/// **非抽数类常量**（PR-2）——只写实体字段，**不抽随机数、不驱动任何 AI 行为**。
///
/// 类由 `MonsterInfo.Race` 决定（不是怪名：同一 race 的不同怪名共用同一个 C# 类），
/// 与 oracle 在 `switch` 内 `new Xxx()` 同步执行构造函数的时机对齐。逐 race 出处
/// （`src/M2Server/Monster/Monsters/*.cs` 实读）：
///
/// - 85 `StickMonster`：`StickMonster.cs:19-20` ⇒ `ComeOutValue=4`、`AttackRange=4`；
/// - 107 `CentipedeKingMonster`：`CentipedeKingMonster.cs:14-15` ⇒ 4 / 6；
/// - 132 `SandMobObject`：`SandMobObject.cs:11` ⇒ `ComeOutValue=8`（`AttackRange` 继承 4）；
/// - 134 `BoneKingMonster`：`BoneKingMonster.cs:17` ⇒ `DangerLevel=5`；
/// - 102 `ScultureKingMonster`：`ScultureKingMonster.cs:21` ⇒ `_mNDangerLevel=5`；
/// - 133 `MagicMonObject` / 200 `ElectronicScolpionMon`：ctor `UseMagic=false`
///   （`MagicMonObject.cs:15`、`ElectronicScolpionMon.cs:12`）；
/// - 120 `SoccerBall`：`SoccerBall.cs:15` ⇒ `N550=0`；
/// - 20 `ArcherPolice` / 112 `ArcherGuard`：`ArcherGuard.cs:17` ⇒ `GuardDirection=-1`；
/// - 92 `CowKingMonster`：ctor `CowKingMonster.cs:20-24`（`JumpTime=now`、三个 Crazy 初值）
///   + `Initialize()` `:35-36`（`oldHitTime=NextHitTime`、`oldWalkTime=WalkSpeed`）；
/// - 96 `ZilKinZombi`：ctor `ZilKinZombi.cs:14` ⇒ `ViewRange=6`（`ZilKillCount` 是抽数字段，
///   由 [`AnimalState::seed_from_spawn`] 消费构造抽，不在此表）；
/// - 52/87/93/104：ctor `ViewRange=5` 与 `BaseObject` 缺省同值（ChickenDeer.cs:10、
///   DualAxeMonster.cs:76），无需落字段。
///
/// 表外 race 保持 [`Entity::new`] 的 C# 字段缺省（0/false）。`now` = oracle
/// `HUtil32.GetTickCount()`（本骨架用 `World::clock_ms` 的确定性注入）。
///
/// **不落 `CowKingMonster.ctor` 的 `RushMode = true`**（`CowKingMonster.cs:21`）：
/// `Entity.rush_mode` 已被 `RM_MAGSTRUCK` 消费（`BaseObject.Base.cs:620`），置位会改变行为，
/// 超出 PR-2「零行为改动」；留待 MB-4 连 `Run` 一起接（登记）。
pub fn apply_monster_class_constants(entity: &mut Entity, race: i16, now: i64) {
    match race {
        // `StickMonster` 基类 ctor（`StickMonster.cs:14-23`）：`ViewRange = 7`、
        // `FixedHideMode = true`、`StickMode = true`、`Animal = true`；`:19-20` 的
        // `ComeOutValue = 4` / `AttackRange = 4`（PR-2 已落）。
        85 => {
            entity.come_out_value = 4;
            entity.attack_range = 4;
            entity.view_range = 7;
            entity.fixed_hide_mode = true;
            entity.stick_mode = true;
            entity.ai.animal = true;
        }
        // `DigOutZombi` ctor（`DigOutZombi.cs:12-16`）：`ViewRange = 7`、`FixedHideMode = true`。
        95 => {
            entity.view_range = 7;
            entity.fixed_hide_mode = true;
        }
        // `ScultureMonster` ctor（`ScultureMonster.cs:11-16`）：`ViewRange = 7`、
        // `StoneMode = true`、`CharStatusEx = PoisonState.STONEMODE (0x1)`。
        101 => {
            entity.view_range = 7;
            entity.stone_mode = true;
            entity.char_status_ex = 0x0000_0001;
        }
        // `BigHeartMonster` ctor（`BigHeartMonster.cs:11-15`）：`ViewRange = 16`、`Animal = false`。
        115 => {
            entity.view_range = 16;
            entity.ai.animal = false;
        }
        // `ZilKinZombi` ctor（`ZilKinZombi.cs:14`）：`ViewRange = 6`。
        96 => entity.view_range = 6,
        // `CentipedeKingMonster` 覆盖（`CentipedeKingMonster.cs:14-15`）。
        107 => {
            entity.come_out_value = 4;
            entity.attack_range = 6;
        }
        // `SandMobObject` 只覆盖 ComeOutValue（`SandMobObject.cs:11`；基类 AttackRange=4）。
        132 => {
            entity.come_out_value = 8;
            entity.attack_range = 4;
        }
        // `BoneKingMonster.cs:17` / `ScultureKingMonster.cs:21`。
        134 | 102 => entity.danger_level = 5,
        // `MagicMonObject.cs:15` / `ElectronicScolpionMon.cs:12`：显式落 ctor 值
        // （与字段缺省同值，保留以钉住“类 ctor 有此赋值”这一事实）。
        133 | 200 => entity.use_magic = false,
        // `SoccerBall.cs:15`。
        120 => entity.n550 = 0,
        // `ArcherGuard.cs:17`；`ArcherPolice`（race 20）继承。
        20 | 112 => entity.guard_direction = -1,
        // `CowKingMonster` ctor + `Initialize()`。
        92 => {
            entity.jump_time = now;
            entity.crazy_count = 0;
            entity.crazy_ready_mode = false;
            entity.crazy_king_mode = false;
            entity.old_hit_time = entity.ai.next_hit_time;
            entity.old_walk_time = entity.ai.walk_speed;
        }
        _ => {}
    }
}

/// `ProcessMonsters` 的 `canCreate` 判定：`!(map == null || (map.Flag.boNOHUMNOMON && map.HumCount <= 0))`。
pub type MapCanCreate<'a> = dyn FnMut(&str) -> bool + 'a;

/// `HUtil32.GetTickCount()` 的注入点（单线程下由调用方给出确定时刻）。
pub type TickClockFn<'a> = dyn FnMut() -> i32 + 'a;

/// 本模块用到的配置项（逐键 oracle 缺省值见本模块头）。
///
/// 缺省值 = `GameSvrConf` 的构造缺省（部署件未覆盖的键）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonGenConfig {
    /// `Config.ProcessMonsterMultiThreadLimit`（`GameSvrConf.cs:1127`，缺省 `:1896` = 1，`byte`）。
    pub process_monster_multi_thread_limit: i32,
    /// `Config.MonGenRate`（`GameSvrConf.cs:334`，缺省 `:1434` = 10；键名 `Setup/GenMonRate`）。
    pub mon_gen_rate: i32,
    /// `Config.RegenMonstersTime`（`GameSvrConf.cs:329`，缺省 `:1433` = 200，毫秒）。
    pub regen_monsters_time: i32,
    /// `Config.ProcessMonsterInterval`（`GameSvrConf.cs:1142`，缺省 `:1895` = 2）。
    pub process_monster_interval: i32,
    /// `Config.ZenFastStep`（`GameSvrConf.cs:68`，缺省 `:1264` = 300）。
    pub zen_fast_step: i32,
    /// `Config.UserFull`（`GameSvrConf.cs:64`，缺省 `:1263` = 5000；部署 3000）。
    pub user_full: i32,
    /// `Config.VentureServer`（`GameSvrConf.cs:32`，缺省 `:1251` = false；部署 FALSE）。
    pub venture_server: bool,
    /// `M2Share.MonLimit`（`M2Share.cs:176`，缺省 30；部署 10）—— 单线程处理一轮的时间闸门。
    pub mon_limit: i32,
    /// `M2Share.ZenLimit`（`M2Share.cs:177`，缺省 5；部署 5）—— `RegenMonsters` 的时间闸门。
    pub zen_limit: i32,
    /// `Config.MakeGhostTime`（`GameSvrConf.cs:431`，构造缺省 `:1486` = `3 * 60 * 1000`；
    /// `ServerConf.cs:567` 读 `[Setup] MakeGhostTime`；部署件 = 90000）。
    ///
    /// `BaseObject.Run` 死亡支的两条门都用它（`BaseObject.Base.cs:88/99`）：可复活支取
    /// `min(max(10s, zen-20s), Config.MakeGhostTime)`，普适支直接用它。
    pub make_ghost_time: i32,
}

impl Default for MonGenConfig {
    fn default() -> Self {
        Self {
            process_monster_multi_thread_limit: 1,
            mon_gen_rate: 10,
            regen_monsters_time: 200,
            process_monster_interval: 2,
            zen_fast_step: 300,
            user_full: 5000,
            venture_server: false,
            mon_limit: 30,
            zen_limit: 5,
            make_ghost_time: 3 * 60 * 1000,
        }
    }
}

/// 一条刷怪配置的**运行时状态**（`SystemModule/Data/MonGenInfo.cs:9-57`）。
///
/// 配置列（`map_name/x/y/mon_name/range/count/zen_time/mission_gen_rate`）由
/// `crates/envir` 的 `MonGenInfo` 解析；本结构补上 oracle 的运行时列
/// （`active_count/race/start_tick/cert_list/thread_id`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonGenEntry {
    pub map_name: String,
    pub x: i16,
    pub y: i16,
    pub mon_name: String,
    /// 刷新范围（`(byte)`）。
    pub range: u8,
    /// 配置刷新数量（`(ushort)`）。
    pub count: u16,
    /// 已刷出数量（`MonGenInfo.ActiveCount`；**每成功创建一个就 +1，即使 `TryAdd` 因上限失败**）。
    pub active_count: u16,
    /// 刷新间隔毫秒（`ZenTime`）。
    pub zen_time: i32,
    /// 集中坐标刷新机率 0~100（`(byte)`）。
    pub mission_gen_rate: u8,
    /// 怪物 Race（`InitializeMonster` 回填；未命中 = -1）。
    pub race: i16,
    /// 上次成功刷怪时刻（`GetTickCount`；0 = 从未刷过）。
    pub start_tick: i32,
    /// 该条已创建的怪物 actor id（`MonGenInfo.CertList`）。
    pub cert_list: Vec<i32>,
    /// 分片后的归属线程 id（`InitializeMonster` 写入）。
    pub thread_id: usize,
}

impl MonGenEntry {
    /// 新建一条（运行时列取零值；与 oracle `new MonGenInfo()` 的字段缺省一致）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        map_name: impl Into<String>,
        x: i16,
        y: i16,
        mon_name: impl Into<String>,
        range: u8,
        count: u16,
        zen_time: i32,
        mission_gen_rate: u8,
    ) -> Self {
        Self {
            map_name: map_name.into(),
            x,
            y,
            mon_name: mon_name.into(),
            range,
            count,
            active_count: 0,
            zen_time,
            mission_gen_rate,
            race: 0,
            start_tick: 0,
            cert_list: Vec::new(),
            thread_id: 0,
        }
    }

    /// 尾部哨兵（`LocalDB.cs:572-587`）：`LoadMonGen` 在文件存在时无条件向
    /// `WorldEngine.MonGenInfoThreadMap[0]` 追加的缺省 `MonGenInfo`——空怪名、`Count=0`、
    /// `CertList` 空表、`Envir=null`。它不在 `MonGenList` 里，因此不计入 `MonGenCount`；
    /// 但会占用线程列表的一个轮询槽位（`ProcessMonsters` 对空怪名直接跳过）。
    #[must_use]
    pub fn sentinel() -> Self {
        Self::new("", 0, 0, "", 0, 0, 0, 0)
    }

    /// `MonGenInfo.TryAdd` 的闸门语义（`MonGenInfo.cs:41-57`）：未满则登记并返回 `true`。
    ///
    /// oracle 用 `>`（不是 `>=`）。注意 oracle **不看返回值**就 `ActiveCount++`，
    /// 因此本方法只做登记闸门，`active_count` 的自增由调用方（[`regen_monsters`]）负责。
    pub fn try_add(&mut self, actor_id: i32) -> bool {
        if self.cert_list.len() > MON_GEN_THREAD_MON_LIMIT {
            return false;
        }
        self.cert_list.push(actor_id);
        true
    }
}

/// 一个「怪物处理线程」的游标状态（`GameSvr/Word/MonsterThread.cs` + `ProcessMonsters` 的本地状态）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MonGenThread {
    /// `MonsterThread.Id`
    pub id: usize,
    /// `MonsterThread.RegenMonstersTick`
    pub regen_monsters_tick: i32,
    /// `MonsterThread.CurrMonGenIdx`（regen 轮询下标）
    pub curr_mon_gen_idx: usize,
    /// `MonsterThread.MonGenListPosition`
    pub mon_gen_list_position: usize,
    /// `MonsterThread.MonGenCertListPosition`
    pub mon_gen_cert_list_position: usize,
    /// `MonsterThread.MonsterProcessCount`
    pub monster_process_count: i32,
    /// `MonsterThread.MonsterProcessPostion`
    pub monster_process_postion: i32,
    /// `MonsterThread.MonsterCount`
    pub monster_count: i32,
    /// `MonGenInfoThreadMap[id]`：该线程负责的条目下标（按 `MonGenList` 顺序）。
    pub entries: Vec<usize>,
}

/// 整套刷怪调度状态（`MonGenList` + `MonGenInfoThreadMap` + `MonsterThreadMap` 的合体）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MonGenState {
    /// 运行时条目表（`MonGenInfoThreadMap` 里存的是这里的下标）。
    pub entries: Vec<MonGenEntry>,
    /// thread id → 线程状态（`MonGenInfoThreadMap`，按 id 升序遍历）。
    pub threads: BTreeMap<usize, MonGenThread>,
    /// 怪名 → thread id（`MonsterThreadMap`；oracle 用**大小写敏感**的默认比较器）。
    pub monster_thread_map: BTreeMap<String, i32>,
}

/// [`initialize_monster`] 的结果读数（判据/日志用）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InitializeMonsterRecord {
    /// `LocalDB.cs:572-587` 的末尾哨兵是否已预置在 `MonGenInfoThreadMap[0]`。
    pub sentinel_present: bool,
    /// `monsterGenMap.Count`（不同怪名数，大小写不敏感）。
    pub distinct_mon_names: usize,
    /// `MonsterList.Count`（第二次补抽的怪名数）。
    pub monster_list_count: usize,
    /// 抽取到的 thread id 序列（先按怪名、再按 `MonsterList.Values`）——判据用。
    pub drawn_thread_ids: Vec<usize>,
    /// 写入 `MonsterThreadMap` 的 `(怪名, thread id)`（按写入顺序）。
    pub thread_map_inserts: Vec<(String, i32)>,
}

/// 复刻 `WorldEngine.InitializeMonster`（`WorldServer.MonGen.cs:30-114`）。
///
/// 输入：
/// - `entries`：`Envir/MonGen.txt` 解析出的条目（本函数**接管所有权语义**，对应 oracle `:112` 的
///   `MonGenList.Clear()`）；
/// - `monster_list_names`：`MonsterList.Values` 的顺序（`MonsterInfo.Name`），用于第二次补抽
///   （`WorldServer.MonGen.cs:91-99`）；
/// - `race_of`：`GetMonRace`（`WorldServer.cs:167-174`），未命中返回 `-1`；
/// - `sentinel_present`：`LocalDB.LoadMonGen`（`:572-587`）是否已在 `MonGenInfoThreadMap[0]`
///   预置末尾哨兵（文件存在 ⇒ `true`，即 `crates/envir` `load_mon_gen` 返回 `Ok(Some(_))`）。
///   哨兵会在分片前占住 `thread 0` 的首个轮询槽位（与 oracle 逐 tick 节奏一致）。
///
/// RNG：先按**怪名**（`monsterGenMap` 的插入序）每名 1 次 `Random(limit)`，再对
/// `monster_list_names` 各 1 次 `Random(limit)`。这与 `:66` / `:94` 两处逐行对应。
///
/// 返回 [`MonGenState`]；`record` 给出读数。
pub fn initialize_monster(
    entries: Vec<MonGenEntry>,
    monster_list_names: &[String],
    sentinel_present: bool,
    config: &MonGenConfig,
    rng: &mut RandomNumber,
    race_of: &dyn Fn(&str) -> i16,
) -> (MonGenState, InitializeMonsterRecord) {
    let limit = config.process_monster_multi_thread_limit.max(0);
    let mut state = MonGenState {
        entries,
        threads: BTreeMap::new(),
        monster_thread_map: BTreeMap::new(),
    };
    let mut record = InitializeMonsterRecord::default();

    // `:32-48`：按怪名分组（`StringComparer.OrdinalIgnoreCase`），保留**首次出现的拼写**与插入序。
    // 存放 (首次出现名, 条目下标列表)。
    let mut monster_gen_map: Vec<(String, Vec<usize>)> = Vec::new();
    for (idx, e) in state.entries.iter().enumerate() {
        match monster_gen_map
            .iter_mut()
            .find(|(name, _)| ordinal_ignore_case_eq(name, &e.mon_name))
        {
            Some((_, list)) => list.push(idx),
            None => monster_gen_map.push((e.mon_name.clone(), vec![idx])),
        }
    }
    record.distinct_mon_names = monster_gen_map.len();

    // `LocalDB.cs:572-587`：`LoadMonGen` 在分片**之前**已把末尾哨兵写进 `MonGenInfoThreadMap[0]`。
    // 先建 `monsterGenMap`（只看解析出的 `MonGenList`）再把哨兵插进去，保证它不参与分组、
    // 也不被 `:83-86` 的 `MonGenList[i]` 错位逻辑看到。
    if sentinel_present {
        let idx = state.entries.len();
        state.entries.push(MonGenEntry::sentinel());
        let thread = state.threads.entry(0).or_default();
        thread.id = 0;
        thread.entries.push(idx);
    }
    record.sentinel_present = sentinel_present;

    if monster_gen_map.is_empty() {
        // `:50-61`：`monsterGenMap.Count <= 0` ⇒ 为 0..limit 各建一个空线程表。
        for i in 0..limit as usize {
            state.threads.entry(i).or_default().id = i;
        }
    } else {
        // `:63-88`：按怪名分片。
        for (i, (_name, idxs)) in monster_gen_map.iter().enumerate() {
            let thread_id = rng.random_below(limit).max(0) as usize;
            record.drawn_thread_ids.push(thread_id);
            let thread = state.threads.entry(thread_id).or_default();
            thread.id = thread_id;
            for &idx in idxs {
                thread.entries.push(idx);
                state.entries[idx].thread_id = thread_id;
            }
            // `:83-86`：`MonsterThreadMap.Add(MonGenList[i].MonName, threadId)`（**用 MonGenList[i] 的下标，
            // 不是 monsterNames[i]**——原文如此，照抄）；已存在则不覆盖。
            if i < state.entries.len() {
                let key = state.entries[i].mon_name.clone();
                if !state.monster_thread_map.contains_key(&key) {
                    state
                        .monster_thread_map
                        .insert(key.clone(), thread_id as i32);
                    record.thread_map_inserts.push((key, thread_id as i32));
                }
            }
        }
    }

    // `:91-99`：对 `MonsterList.Values` 各再抽一次（`MonsterThreadMap` 已有则不覆盖）。
    record.monster_list_count = monster_list_names.len();
    for name in monster_list_names {
        let thread_id = rng.random_below(limit).max(0) as usize;
        record.drawn_thread_ids.push(thread_id);
        if !state.monster_thread_map.contains_key(name) {
            state
                .monster_thread_map
                .insert(name.clone(), thread_id as i32);
            record
                .thread_map_inserts
                .push((name.clone(), thread_id as i32));
        }
    }

    // `:101-110`：按怪名回填 Race（oracle 直接下标 `MonGenInfoThreadMap[i]`；见模块头「建模差异」，
    // 本实现按 thread id 升序遍历实际存在的线程）。
    for thread in state.threads.values_mut() {
        for &idx in &thread.entries {
            let name = state.entries[idx].mon_name.clone();
            if name.is_empty() {
                continue;
            }
            let race = race_of(&name);
            state.entries[idx].race = race;
        }
    }

    (state, record)
}

/// `StringComparer.OrdinalIgnoreCase` 的等价物。
///
/// .NET 的 ordinal-ignore-case 逐 UTF-16 码元做 invariant 大小写折叠；Mir2 的怪名是
/// ASCII + GBK 汉字（无大小写折叠），[`str::eq_ignore_ascii_case`] 与之在实语料上等价。
fn ordinal_ignore_case_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// `WorldEngine.GetMonstersZenTime`（`WorldServer.MonGen.cs:168-190`）。
///
/// `dwTime < 30min` 且 `d10 = (PlayObjectCount - UserFull) / max(1, ZenFastStep) > 0` 时，
/// 按 `d10`（夹 6）加速：`dwTime - Round(dwTime / 10.0 * d10)`；否则原值。
///
/// 整数除法截断方向、`Round` 的「一半远离零」都由 [`mir2_shared::hutil::round`] 钉死。
#[must_use]
pub fn get_monsters_zen_time(dw_time: i32, play_object_count: i32, config: &MonGenConfig) -> i32 {
    if dw_time < ZEN_TIME_FAST_THRESHOLD_MS {
        let mut d10 = (play_object_count - config.user_full) / max_i32(1, config.zen_fast_step);
        if d10 > 0 {
            if d10 > 6 {
                d10 = 6;
            }
            return dw_time - round(f64::from(dw_time) / 10.0 * f64::from(d10));
        }
        dw_time
    } else {
        dw_time
    }
}

/// `ProcessMonsters` 的 `genModCount`（`WorldServer.MonGen.cs:232`）：
/// `max(1, Round(max(1, Count) / (MonGenRate / 10.0)))`。
///
/// `MonGenRate / 10.0` 是 double（部署 `GenMonRate=10` ⇒ 除数 1.0）。
#[must_use]
pub fn gen_mod_count(count: u16, mon_gen_rate: i32) -> i32 {
    max_i32(
        1,
        round(f64::from(max_i32(1, i32::from(count))) / (f64::from(mon_gen_rate) / 10.0)),
    )
}

/// `ProcessMonsters` 的 regen 门（`WorldServer.MonGen.cs:207` + `:235-253`）：
/// 到点且该条到期、且 `genModCount > ActiveCount`、且地图可刷 ⇒ 返回 `Some(nCount)`。
///
/// `can_create` 对应 `!(map == null || (map.Flag.boNOHUMNOMON && map.HumCount <= 0))`。
#[must_use]
pub fn should_regen(
    entry: &MonGenEntry,
    now: i32,
    play_object_count: i32,
    config: &MonGenConfig,
    can_create: bool,
) -> bool {
    if entry.mon_name.is_empty() || config.venture_server {
        return false;
    }
    let zen = get_monsters_zen_time(entry.zen_time, play_object_count, config);
    if entry.start_tick != 0 && (now.wrapping_sub(entry.start_tick)) <= zen {
        return false;
    }
    let gen_mod_count = gen_mod_count(entry.count, config.mon_gen_rate);
    if gen_mod_count <= i32::from(entry.active_count) || !can_create {
        return false;
    }
    true
}

/// regen 门的读数（判据/日志用）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegenDecision {
    pub thread_id: usize,
    pub entry_index: usize,
    /// 该线程是否到了 `RegenMonstersTime` 闸门。
    pub gate_due: bool,
    /// `genModCount`（`ProcessMonsters` 计算值）。
    pub gen_mod_count: i32,
    /// 决策前的 `ActiveCount`。
    pub active_count: i32,
    /// 本轮到期的刷新数量 `genModCount - ActiveCount`（未到期 = 0）。
    pub n_count: i32,
    /// 是否调用了 `RegenMonsters`。
    pub regened: bool,
    /// `RegenMonsters` 的返回值（`boRegened`）。
    pub regen_result: bool,
    /// 是否因此回写了 `StartTick`。
    pub start_tick_updated: bool,
}

/// 单线程一步的 regen 门 + 轮询下标推进（`WorldServer.MonGen.cs:200-253`）。
///
/// 与 oracle 对齐的点：
/// - 闸门 `(now - RegenMonstersTick) > RegenMonstersTime`（`:207`，**严格大于**）；
/// - 到点即回写 `RegenMonstersTick = now`（`:209`）；
/// - 取当前 `CurrMonGenIdx`（越界取 0），再推进下标（`:211-230`）；
/// - 只有 `!venture_server` 且怪名非空才继续（`:233`）；
/// - `boRegened` 为真才 `monGen.StartTick = now`（`:250-252`）。
///
/// `can_create`/`create_monster` 见 [`should_regen`] 与 [`regen_monsters`]。
#[allow(clippy::too_many_arguments)]
pub fn regen_gate(
    state: &mut MonGenState,
    thread_id: usize,
    now: i32,
    play_object_count: i32,
    config: &MonGenConfig,
    rng: &mut RandomNumber,
    clock: &mut TickClockFn,
    can_create: &mut MapCanCreate,
    create_monster: &mut MonsterCreator,
) -> RegenDecision {
    let mut decision = RegenDecision {
        thread_id,
        ..RegenDecision::default()
    };
    let Some(thread) = state.threads.get_mut(&thread_id) else {
        return decision;
    };
    if now.wrapping_sub(thread.regen_monsters_tick) <= config.regen_monsters_time {
        return decision;
    }
    decision.gate_due = true;
    thread.regen_monsters_tick = now;

    let list_len = thread.entries.len();
    if list_len == 0 {
        thread.curr_mon_gen_idx = 0;
        return decision;
    }
    let entry_index = if thread.curr_mon_gen_idx < list_len {
        thread.entries[thread.curr_mon_gen_idx]
    } else {
        thread.entries[0]
    };
    if thread.curr_mon_gen_idx < list_len - 1 {
        thread.curr_mon_gen_idx += 1;
    } else {
        thread.curr_mon_gen_idx = 0;
    }
    decision.entry_index = entry_index;

    let entry = &mut state.entries[entry_index];
    if entry.mon_name.is_empty() || config.venture_server {
        return decision;
    }
    let zen = get_monsters_zen_time(entry.zen_time, play_object_count, config);
    if entry.start_tick != 0 && now.wrapping_sub(entry.start_tick) <= zen {
        return decision;
    }
    let gen_mod_count = gen_mod_count(entry.count, config.mon_gen_rate);
    decision.gen_mod_count = gen_mod_count;
    decision.active_count = i32::from(entry.active_count);
    // **R5-1**：oracle 进入 zen 到期块就 `bool boRegened = true;`（`WorldServer.MonGen.cs:231`），
    // 只有 `genModCount > nGenCount && canCreate` 成立才把它改成 `RegenMonsters` 的返回值
    // （`:244-246`），随后 `if (boRegened) monGen.StartTick = GetTickCount();`（`:248-250`）
    // ⇒ **饱和 / `!canCreate` 也要回写 `StartTick`**（否则 `!canCreate` 恢复可刷后 Rust 比 oracle 早一个 zen 周期）。
    let mut bo_regened = true;
    if gen_mod_count > i32::from(entry.active_count) && can_create(&entry.map_name) {
        let n_count = gen_mod_count - i32::from(entry.active_count);
        decision.n_count = n_count;
        // `decision.regened` 仍只表示「调用了 `RegenMonsters`」。
        decision.regened = true;
        bo_regened = regen_monsters(rng, entry, n_count, config, clock, create_monster);
        decision.regen_result = bo_regened;
    }
    if bo_regened {
        entry.start_tick = now;
        decision.start_tick_updated = true;
    }
    decision
}

/// 按 thread id 升序驱动每条线程一步（单线程串行版本的 `ProcessMonsters` 定时驱动）。
#[allow(clippy::too_many_arguments)]
pub fn step_threads(
    state: &mut MonGenState,
    now: i32,
    play_object_count: i32,
    config: &MonGenConfig,
    rng: &mut RandomNumber,
    clock: &mut TickClockFn,
    can_create: &mut MapCanCreate,
    create_monster: &mut MonsterCreator,
) -> Vec<RegenDecision> {
    let ids: Vec<usize> = state.threads.keys().copied().collect();
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        out.push(regen_gate(
            state,
            id,
            now,
            play_object_count,
            config,
            rng,
            clock,
            can_create,
            create_monster,
        ));
    }
    out
}

/// `WorldEngine.RegenMonsters`（`WorldServer.MonGen.cs:811-874`）的 1:1 移植（RNG 次序部分）。
///
/// ## RNG 次序契约（判据钉死）
///
/// `entry.race <= 0` ⇒ 不抽任何数、返回 `true`。
///
/// 「集中」支（`MissionGenRate > 0` 且 `Random(100) < MissionGenRate`）：
/// 1. 先抽 1 次 `Random(100)`；
/// 2. 定中心：`Random(Range*2+1)`、`Random(Range*2+1)`；
/// 3. 每只：`Random(20)`、`Random(20)`，随后同步调用 `create_monster`（其内部再抽 Dir/掉落/CoolEye）。
///
/// 普通支：
/// 1. 每只：`Random(Range*2+1)`、`Random(Range*2+1)`，随后同步调用 `create_monster`。
///
/// `create_monster` **必须在两次位置抽之后、下一只的位置抽之前**被同步调用（本函数保证）；
/// 回调内部应照 oracle `CreateMonster`（`WorldServer.MonGen.cs:498-810`）的次序取数
/// （`RandomByte(8)` 的 `Dir` → `MonGetRandomItems` 的预算抽 → `Random(100)` 的 `CoolEye`
/// → 地形不可站时的移位抽）。
///
/// `clock` 对应循环里的两次 `HUtil32.GetTickCount()`（`:815` 与 `:836`/`:853-855`）：
/// 超 `ZenLimit` 则中断（集中支 `result=false`；普通支直接 `return false`）。
///
/// 普通支 `create_monster` 返回 `None` ⇒ **直接 `return false`**（`:857`）。
#[allow(clippy::too_many_arguments)]
pub fn regen_monsters(
    rng: &mut RandomNumber,
    entry: &mut MonGenEntry,
    n_count: i32,
    config: &MonGenConfig,
    clock: &mut TickClockFn,
    create_monster: &mut MonsterCreator,
) -> bool {
    let mut result = true;
    // **R5-2**：oracle 在 `if (monGen.Race > 0)` **之前**就取 `dwStartTick`（`WorldServer.MonGen.cs:816`）；
    // 真实时钟无功能影响，但注入式 `TickClockFn` 下会被放大成假绿/假红。
    let dw_start_tick = clock();
    if entry.race <= 0 {
        return result;
    }
    let range = i32::from(entry.range);
    let span = range * 2 + 1;
    let base_x = i32::from(entry.x) - range;
    let base_y = i32::from(entry.y) - range;
    let map_name = entry.map_name.clone();
    let mon_name = entry.mon_name.clone();
    let race = entry.race;
    let mission = entry.mission_gen_rate;

    if mission > 0 && rng.random_below(100) < i32::from(mission) {
        let n_x = base_x + rng.random_below(span);
        let n_y = base_y + rng.random_below(span);
        for _ in 0..n_count {
            let x = n_x - 10 + rng.random_below(20);
            let y = n_y - 10 + rng.random_below(20);
            if let Some(cert) = create_monster(rng, &map_name, x as i16, y as i16, race, &mon_name)
            {
                entry.active_count = entry.active_count.wrapping_add(1);
                entry.try_add(cert);
            }
            if clock().wrapping_sub(dw_start_tick) > config.zen_limit {
                result = false;
                break;
            }
        }
    } else {
        for _ in 0..n_count {
            let x = base_x + rng.random_below(span);
            let y = base_y + rng.random_below(span);
            match create_monster(rng, &map_name, x as i16, y as i16, race, &mon_name) {
                Some(cert) => {
                    entry.active_count = entry.active_count.wrapping_add(1);
                    entry.try_add(cert);
                }
                None => return false,
            }
            if clock().wrapping_sub(dw_start_tick) > config.zen_limit {
                result = false;
                break;
            }
        }
    }
    result
}

/// 复活 tick 的判定（`WorldServer.MonGen.cs:283-291`）：
/// `Death && CanReAlive && Invisible && MonGen != null`，且
/// `(now - ReAliveTick) > GetMonstersZenTime(MonGen.ZenTime)`。
///
/// 真实 `ReAliveEx` 属 `crates/world` 的怪物 AI/实体面（卡 `openmir2-m3-monster-ai-tick`），
/// 本函数只钉住**触发条件**（含 `GetMonstersZenTime` 这一层 RNG-free 的时间换算）。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn monster_due_for_revive(
    death: bool,
    can_re_alive: bool,
    invisible: bool,
    has_mon_gen: bool,
    re_alive_tick: i32,
    mon_gen_zen_time: i32,
    now: i32,
    play_object_count: i32,
    config: &MonGenConfig,
) -> bool {
    if !(death && can_re_alive && invisible && has_mon_gen) {
        return false;
    }
    now.wrapping_sub(re_alive_tick)
        > get_monsters_zen_time(mon_gen_zen_time, play_object_count, config)
}

/// 幽灵怪 5 分钟清理判定（`WorldServer.MonGen.cs:302`）：`(now - GhostTick) > 5min`。
#[must_use]
pub fn monster_ghost_expired(ghost_tick: i32, now: i32) -> bool {
    now.wrapping_sub(ghost_tick) > MONSTER_GHOST_EXPIRE_MS
}

/// `BaseObject.Run` 死亡支的门限值（`BaseObject.Base.cs:85-101`）：
///
/// ```csharp
/// if (CanReAlive && MonGen != null)
/// {
///     int makeGhostTime = MAX(10 * 1000, GetMonstersZenTime(MonGen.ZenTime) - 20 * 1000);
///     if (makeGhostTime > Config.MakeGhostTime) makeGhostTime = Config.MakeGhostTime;
///     if (GetTickCount() - DeathTick > makeGhostTime) MakeGhost();
/// }
/// else
/// {
///     if ((GetTickCount() - DeathTick) > Config.MakeGhostTime) MakeGhost();
/// }
/// ```
///
/// `make_ghost_time` 参数 = `Config.MakeGhostTime`（部署 90000）。
/// 可复活支的 **MAX(10s) 下限**与 **`Config.MakeGhostTime` 截断**是两步独立运算，顺序照抄：
/// 先 `max(10s, zen - 20s)`，再 `min(结果, Config.MakeGhostTime)`。
///
/// `play_object_count` / `config.user_full` / `config.zen_fast_step` 只喂
/// [`get_monsters_zen_time`] 的加速档（`WorldServer.MonGen.cs:168-190`）。
#[must_use]
pub fn monster_death_gate(
    can_re_alive: bool,
    has_mon_gen: bool,
    mon_gen_zen_time: i32,
    make_ghost_time: i32,
    play_object_count: i32,
    config: &MonGenConfig,
) -> i32 {
    if can_re_alive && has_mon_gen {
        let zen = get_monsters_zen_time(mon_gen_zen_time, play_object_count, config);
        let mut gate = max_i32(10 * 1000, zen.wrapping_sub(20 * 1000));
        if gate > make_ghost_time {
            gate = make_ghost_time;
        }
        gate
    } else {
        make_ghost_time
    }
}

/// 死亡支到点判定：`(now - DeathTick) > gate`（**严格大于**，`BaseObject.Base.cs:90/99`）。
#[must_use]
pub fn monster_due_for_ghost(death_tick: i32, gate: i32, now: i32) -> bool {
    now.wrapping_sub(death_tick) > gate
}

/// 一次 `ProcessMonsters` 在册怪物段（`WorldServer.MonGen.cs:278-333`）的 `ReAliveEx` 帧面读数。
///
/// oracle `ReAliveEx`（`BaseObject.cs:2740-2750`）尾部：
/// `SendRefMsg(RM_TURN, Dir, CurrX, CurrY, GetFeatureToLong(), "")`，随后
/// `MonsterSayMessage(null, MonStatus.MonGen)`。`GetFeatureToLong()` 的怪物 `CharDesc`
/// 属展示层（本仓怪物帧面仍未建模，登记）⇒ 本记录保留 `RM_TURN` 的
/// `(Dir, CurrX, CurrY)` 三元组。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReviveFrame {
    /// `RM_TURN` 的 ident（`BaseObject.Dir`）。
    pub dir: u8,
    /// `RM_TURN` 的 recog（落位后的 `CurrX`）。
    pub x: i32,
    /// `RM_TURN` 的 param（落位后的 `CurrY`）。
    pub y: i32,
}

/// 一次 `ReAliveEx` 的读数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviveOutcome {
    pub actor_id: i32,
    /// 本次 `ReAliveEx` 发出的 `RM_TURN` 帧（保序）：
    /// 1. `AddToMap()` 内部的 `SendRefMsg(RM_TURN, Dir, CurrX, CurrY, 0, "")`
    ///    （`BaseObject.cs:1370-1378`，首格 `CanWalk(...,true)` + `AddMapObject` 成功时）；
    /// 2. 函数末尾的 `SendRefMsg(RM_TURN, Dir, CurrX, CurrY, GetFeatureToLong(), "")`
    ///    （`BaseObject.cs:2747`）。
    ///
    /// 注意 `AddMapObject` 在**回退循环**与**恢复旧格**两支里是直接调用、不经 `AddToMap`
    /// ⇒ 那两支不产生额外 `RM_TURN`（照 oracle）。
    pub frames: Vec<ReviveFrame>,
}

/// 一轮在册怪物段的读数。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CertListRecord {
    /// 真正复活（`ReAliveEx` 返回 true）的怪，按 `CertList` 遍历序。
    pub revived: Vec<ReviveOutcome>,
    /// 5 分钟门移除并释放的 Ghost 怪，按遍历序。
    pub removed: Vec<i32>,
}

/// `ProcessMonsters` 的**在册怪物段**（`WorldServer.MonGen.cs:278-333`）：
///
/// - `!Ghost`：`Death && CanReAlive && Invisible && MonGen != null` 且
///   `(now - ReAliveTick) > GetMonstersZenTime(MonGen.ZenTime)` ⇒ `ReAliveEx(MonGen)`
///   （`:283-292`）；
/// - `Ghost`：`(now - GhostTick) > 5 * 60 * 1000` ⇒ `CertList.RemoveAt` 并释放（`:320-333`）。
///
/// **顺序**：thread id 升序 → `thread.entries` 顺序 → `cert_list` 顺序（oracle 的
/// `MonGenInfoThreadMap` 遍历 + 每条的 `CertList` 遍历）。本仓把 oracle 的
/// `MonsterThread` 墙钟节拍折叠为「宿主每世界 tick 调一次」（既有登记：`ProcessMonsters`
/// 的单线程串行等价物），因此这里没有 `MonLimit` 断点续跑与 `RunTime` 门。
///
/// **不判 `RunTick`**：oracle 的复活检查在 `if ((currentTick - monster.RunTick) > monster.RunTime)`
/// 之内（`RunTime = 250`，`BaseObject.cs:440`）。本仓 `monster_ai_tick` 也不做 RunTime 门
/// （既有建模差异，按世界 tick 200ms 驱动）⇒ 本 pass 与 AI 同相位、每 tick 至多一次判定。
pub fn process_cert_lists(
    state: &mut MonGenState,
    world: &mut crate::world::World,
    now: i64,
    play_object_count: i32,
    config: &MonGenConfig,
) -> CertListRecord {
    let mut record = CertListRecord::default();
    let now_i32 = i32::try_from(now).unwrap_or(i32::MAX);
    // 先固定遍历序：thread id 升序（BTreeMap）→ 条目顺序。
    let entry_lists: Vec<Vec<usize>> = state.threads.values().map(|t| t.entries.clone()).collect();
    for entries in entry_lists {
        for entry_idx in entries {
            if entry_idx >= state.entries.len() {
                continue;
            }
            let ids = state.entries[entry_idx].cert_list.clone();
            for id in ids {
                let Some(e) = world.entities.get(id) else {
                    continue;
                };
                if !e.ghost {
                    // oracle 的 `MonGen` 是实体自身引用（不一定等于当前遍历到的条目）。
                    let mon_gen_idx = e.mon_gen.unwrap_or(entry_idx);
                    let Some(owner) = state.entries.get(mon_gen_idx) else {
                        continue;
                    };
                    let due = monster_due_for_revive(
                        e.death,
                        e.can_re_alive,
                        e.invisible,
                        e.mon_gen.is_some(),
                        i32::try_from(e.re_alive_tick).unwrap_or(i32::MAX),
                        owner.zen_time,
                        now_i32,
                        play_object_count,
                        config,
                    );
                    if due {
                        let (x, y, range) = (owner.x, owner.y, owner.range);
                        if let Some(outcome) = world.revive_monster(id, x, y, range, now) {
                            record.revived.push(outcome);
                        }
                    }
                } else {
                    let expired = monster_ghost_expired(
                        i32::try_from(e.ghost_tick).unwrap_or(i32::MAX),
                        now_i32,
                    );
                    if expired {
                        world.release_entity(id);
                        state.entries[entry_idx].cert_list.retain(|v| *v != id);
                        record.removed.push(id);
                    }
                }
            }
        }
    }
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Entity;
    use crate::world::World;

    /// PR-2：装载钩子按 **race（= C# `new Xxx()` 的类）** 落非抽数类常量；
    /// 表外 race 保持 `Entity::new` 的字段缺省。逐值出处见
    /// [`apply_monster_class_constants`]。
    ///
    /// 牙②：把 race 107 的 `attack_range` 从 6 改成 4（照抄 `StickMonster` 基类）⇒ 本用例红。
    #[test]
    fn class_constants_match_oracle_ctors_per_race() {
        let mk = |race: i16, now: i64| {
            let mut e = Entity::new(0, "x", 0, 0);
            apply_monster_class_constants(&mut e, race, now);
            e
        };

        let stick = mk(85, 0);
        assert_eq!((stick.come_out_value, stick.attack_range), (4, 4));
        // **MB-1**：`StickMonster.cs:14-23` 的 ctor 常量。
        assert_eq!(stick.view_range, 7, "StickMonster ViewRange=7");
        assert!(stick.fixed_hide_mode, "StickMonster FixedHideMode=true");
        assert!(stick.stick_mode, "StickMonster StickMode=true");
        assert!(stick.ai.animal, "StickMonster Animal=true");
        let dig = mk(95, 0);
        assert_eq!(dig.view_range, 7, "DigOutZombi ViewRange=7");
        assert!(dig.fixed_hide_mode, "DigOutZombi FixedHideMode=true");
        assert!(!dig.stone_mode, "DigOutZombi 非石像");
        let sculture = mk(101, 0);
        assert_eq!(sculture.view_range, 7, "ScultureMonster ViewRange=7");
        assert!(sculture.stone_mode, "ScultureMonster StoneMode=true");
        assert_eq!(
            sculture.char_status_ex, 0x1,
            "ScultureMonster CharStatusEx=STONEMODE"
        );
        let heart = mk(115, 0);
        assert_eq!(heart.view_range, 16, "BigHeartMonster ViewRange=16");
        assert!(!heart.ai.animal, "BigHeartMonster Animal=false");
        let centipede = mk(107, 0);
        assert_eq!((centipede.come_out_value, centipede.attack_range), (4, 6));
        let sand = mk(132, 0);
        assert_eq!((sand.come_out_value, sand.attack_range), (8, 4));
        assert_eq!(mk(134, 0).danger_level, 5, "BoneKing ctor");
        assert_eq!(mk(102, 0).danger_level, 5, "ScultureKing ctor");
        assert!(!mk(133, 0).use_magic, "MagicMonObject ctor");
        assert!(!mk(200, 0).use_magic, "ElectronicScolpionMon ctor");
        assert_eq!(mk(120, 0).n550, 0, "SoccerBall ctor");
        assert_eq!(mk(20, 0).guard_direction, -1, "ArcherPolice ⇒ ArcherGuard");
        assert_eq!(mk(112, 0).guard_direction, -1, "ArcherGuard ctor");

        let cow = mk(92, 55_000);
        assert_eq!(cow.jump_time, 55_000, "ctor JumpTime = GetTickCount()");
        assert!(!cow.crazy_ready_mode, "ctor CrazyReadyMode=false");
        assert!(!cow.crazy_king_mode, "ctor CrazyKingMode=false");
        assert_eq!(cow.crazy_count, 0, "ctor CrazyCount=0");
        assert_eq!((cow.crazy_ready, cow.crazy_time), (0, 0));
        assert_eq!(
            cow.old_hit_time, 2000,
            "Initialize: oldHitTime = NextHitTime"
        );
        assert_eq!(
            cow.old_walk_time, 1400,
            "Initialize: oldWalkTime = WalkSpeed"
        );
        assert_eq!(cow.siege_lock_count, 0, "oracle 无持久字段（现算）");

        let plain = mk(80, 0);
        assert_eq!(
            (
                plain.come_out_value,
                plain.attack_range,
                plain.danger_level,
                plain.n550,
                plain.guard_direction,
                plain.jump_time
            ),
            (0, 0, 0, 0, 0, 0),
            "表外 race 不许被类常量污染"
        );
    }

    fn cfg() -> MonGenConfig {
        MonGenConfig::default()
    }

    fn no_race(_: &str) -> i16 {
        -1
    }

    fn entry(name: &str, count: u16, range: u8, mission: u8, x: i16, y: i16) -> MonGenEntry {
        MonGenEntry::new("0", x, y, name, range, count, 60_000, mission)
    }

    #[test]
    fn initialize_monster_draw_count_matches_oracle() {
        // 3 条不同怪名 + 2 个 MonsterList 项 ⇒ 5 次 Random(limit)（:66 一次/名 + :94 一次/项）。
        let entries = vec![
            entry("A", 1, 0, 0, 0, 0),
            entry("B", 1, 0, 0, 0, 0),
            entry("a", 1, 0, 0, 0, 0), // 与 A 大小写不敏感同组
            entry("C", 1, 0, 0, 0, 0),
        ];
        let list = vec!["A".to_string(), "C".to_string()];
        let mut rng = RandomNumber::with_seed(42);
        let (state, rec) = initialize_monster(entries, &list, false, &cfg(), &mut rng, &no_race);
        // distinct = A / B / C = 3；list = 2 ⇒ 5 次抽数
        assert_eq!(rec.distinct_mon_names, 3);
        assert_eq!(rec.drawn_thread_ids.len(), 5);
        // limit=1 ⇒ 全部为 0
        assert!(rec.drawn_thread_ids.iter().all(|&t| t == 0));
        // A 与 a 同组（OrdinalIgnoreCase）：thread 0 下应有 2 条（A、a）而非 1 条。
        let t0 = state.threads.get(&0).expect("thread 0");
        assert_eq!(t0.entries.len(), 4, "全部 4 条都落在 thread 0（limit=1）");
    }

    #[test]
    fn initialize_monster_race_backfill_and_unknown_is_minus_one() {
        let entries = vec![entry("A", 1, 0, 0, 0, 0), entry("X", 1, 0, 0, 0, 0)];
        let list: Vec<String> = vec![];
        let race_of = |n: &str| if n == "A" { 80 } else { -1 };
        let mut rng = RandomNumber::with_seed(7);
        let (state, _) = initialize_monster(entries, &list, false, &cfg(), &mut rng, &race_of);
        assert_eq!(state.entries[0].race, 80);
        assert_eq!(state.entries[1].race, -1);
    }

    #[test]
    fn initialize_monster_thread_map_uses_gen_list_index_not_name_index() {
        // oracle :84 用 MonGenList[i].MonName（i 是 monsterNames 的下标），不是 monsterNames[i]。
        // 构造：entries = [A, B]，distinct names = ["A","B"]，i=0 时写 MonGenList[0]=A，i=1 写 MonGenList[1]=B。
        // 再构造一个「错位」判定：entries = [B, A] ⇒ i=0 写 B（而 monsterNames[0]=B 恰好相同）。
        // 真正的错位用例：entries = [A, A, B] ⇒ distinct = [A, B]；i=1 时 MonGenList[1].MonName = A
        // ⇒ 第二次写入的是 A（不是 B）。
        let entries = vec![
            entry("A", 1, 0, 0, 0, 0),
            entry("A", 1, 0, 0, 0, 0),
            entry("B", 1, 0, 0, 0, 0),
        ];
        let mut rng = RandomNumber::with_seed(1);
        let (state, _) = initialize_monster(entries, &[], false, &cfg(), &mut rng, &no_race);
        // thread map 里第一条是 A（i=0），第二条 key 用 entries[1].MonName = A ⇒ 已存在不覆盖
        // ⇒ B 不会进 MonsterThreadMap（这就是 oracle 的错位缺陷，1:1 保留）。
        assert!(state.monster_thread_map.contains_key("A"));
        assert!(
            !state.monster_thread_map.contains_key("B"),
            "oracle 的 MonGenList[i] 错位：B 不写入 MonsterThreadMap"
        );
    }

    #[test]
    fn initialize_monster_sentinel_is_first_round_robin_slot() {
        // `LocalDB.cs:572-587`：文件存在时末尾哨兵先占 `MonGenInfoThreadMap[0]` 的首槽。
        let entries = vec![entry("A", 5, 0, 0, 10, 10), entry("B", 5, 0, 0, 20, 20)];
        let mut rng = RandomNumber::with_seed(42);
        let race_of = |_: &str| 80;
        let (mut state, rec) = initialize_monster(entries, &[], true, &cfg(), &mut rng, &race_of);
        assert!(rec.sentinel_present);
        // 哨兵不计入分组（distinct 只看解析出的怪名）。
        assert_eq!(rec.distinct_mon_names, 2);
        // 解析条目 0/1 + 哨兵 = 3 条；哨兵在 entries 末尾（构造顺序）。
        assert_eq!(state.entries.len(), 3);
        let t0 = &state.threads[&0];
        assert_eq!(t0.entries.len(), 3);
        let sentinel_idx = t0.entries[0];
        assert_eq!(state.entries[sentinel_idx].mon_name, "");
        assert_eq!(state.entries[sentinel_idx].count, 0);
        assert_eq!(state.entries[sentinel_idx].zen_time, 0);
        assert!(state.entries[sentinel_idx].cert_list.is_empty());

        // 首个 regen 门选中哨兵：空怪名 ⇒ 不刷怪、不回写 StartTick（与 oracle 同支）。
        let mut clk = || 100;
        let mut can = |_: &str| true;
        let mut created = 0;
        let d = regen_gate(
            &mut state,
            0,
            1000,
            5000,
            &cfg(),
            &mut rng,
            &mut clk,
            &mut can,
            &mut |_rng, _, _, _, _, _| {
                created += 1;
                Some(1)
            },
        );
        assert!(d.gate_due);
        assert_eq!(d.entry_index, sentinel_idx);
        assert!(!d.regened, "哨兵空怪名 ⇒ 不调 RegenMonsters");
        assert!(!d.start_tick_updated);
        assert_eq!(created, 0);

        // 无哨兵时不会多出条目。
        let entries2 = vec![entry("A", 5, 0, 0, 10, 10)];
        let (state2, rec2) = initialize_monster(entries2, &[], false, &cfg(), &mut rng, &race_of);
        assert!(!rec2.sentinel_present);
        assert_eq!(state2.entries.len(), 1);
    }

    #[test]
    fn get_monsters_zen_time_matches_oracle() {
        let c = cfg(); // user_full=5000, zen_fast_step=300
                       // 用户数低于 UserFull ⇒ d10 <= 0 ⇒ 原值
        assert_eq!(get_monsters_zen_time(60_000, 5000, &c), 60_000);
        // d10 = (5300-5000)/300 = 1 ⇒ 60000 - round(6000*1) = 54000
        assert_eq!(get_monsters_zen_time(60_000, 5300, &c), 54_000);
        // d10 = (8000-5000)/300 = 10 ⇒ 夹到 6 ⇒ 60000 - round(36000) = 24000
        assert_eq!(get_monsters_zen_time(60_000, 8000, &c), 24_000);
        // >= 30min ⇒ 原值（不进加速）
        assert_eq!(
            get_monsters_zen_time(30 * 60 * 1000, 99_999, &c),
            30 * 60 * 1000
        );
        // d10 夹 6 的边界：d10 = 6 ⇒ 60000 - 36000
        assert_eq!(get_monsters_zen_time(60_000, 6800, &c), 24_000);
        // 负差值（用户数远低于上限）⇒ 不加速，且不 panic
        assert_eq!(get_monsters_zen_time(60_000, 0, &c), 60_000);
    }

    #[test]
    fn gen_mod_count_matches_oracle() {
        assert_eq!(gen_mod_count(1, 10), 1);
        assert_eq!(gen_mod_count(50, 10), 50);
        assert_eq!(gen_mod_count(0, 10), 1);
        // 3 / (20/10.0) = 1.5 ⇒ Round away from zero = 2
        assert_eq!(gen_mod_count(3, 20), 2);
        // 1 / (20/10.0) = 0.5 ⇒ 1
        assert_eq!(gen_mod_count(1, 20), 1);
        // 5 / (40/10.0) = 1.25 ⇒ 1
        assert_eq!(gen_mod_count(5, 40), 1);
    }

    #[test]
    fn should_regen_gate_and_zen_time() {
        let c = cfg();
        let mut e = entry("A", 10, 2, 0, 166, 266);
        e.race = 80;
        // 从未刷过（start_tick=0）⇒ 到期
        assert!(should_regen(&e, 1000, 5000, &c, true));
        // 刚刷过且未超 ZenTime ⇒ 不到期
        e.start_tick = 1000;
        assert!(!should_regen(&e, 20_000, 5000, &c, true));
        // 超 ZenTime（60s）⇒ 到期
        assert!(should_regen(&e, 1000 + 60_001, 5000, &c, true));
        // 地图不可刷 ⇒ 不到期
        e.start_tick = 0;
        assert!(!should_regen(&e, 1000, 5000, &c, false));
        // ActiveCount 已达 genModCount ⇒ 不到期
        e.active_count = 10;
        assert!(!should_regen(&e, 1000, 5000, &c, true));
        // VentureServer ⇒ 不到期
        let mut cv = c;
        cv.venture_server = true;
        e.active_count = 0;
        assert!(!should_regen(&e, 1000, 5000, &cv, true));
    }

    /// 记录型 sink：每次 create 记录 `(map,x,y,race,name)` 并返回自增 id。
    struct Sink {
        calls: Vec<(String, i16, i16, i16, String)>,
        next: i32,
    }
    impl Sink {
        fn new() -> Self {
            Self {
                calls: Vec::new(),
                next: 1000,
            }
        }
        fn create(
            &mut self,
            _rng: &mut RandomNumber,
            map: &str,
            x: i16,
            y: i16,
            race: i16,
            name: &str,
        ) -> Option<i32> {
            self.calls
                .push((map.to_string(), x, y, race, name.to_string()));
            self.next += 1;
            Some(self.next)
        }
    }

    #[test]
    fn regen_monsters_normal_branch_draws_two_per_monster_and_interleaves_create() {
        let c = cfg();
        // 普通支：MissionGenRate=0
        let mut e = entry("A", 4, 2, 0, 166, 266);
        e.race = 80;
        let mut rng = RandomNumber::with_seed(42);
        let mut clock = || 100;
        let mut sink = Sink::new();
        let ok = regen_monsters(
            &mut rng,
            &mut e,
            4,
            &c,
            &mut clock,
            &mut |rng, map, x, y, race, name| sink.create(rng, map, x, y, race, name),
        );
        assert!(ok);
        assert_eq!(sink.calls.len(), 4);
        assert_eq!(e.active_count, 4);
        assert_eq!(e.cert_list.len(), 4);
        // 位置应落在 [X-Range, X+Range] × [Y-Range, Y+Range]
        for (_, x, y, race, name) in &sink.calls {
            assert!(*x >= 164 && *x <= 168, "x={x} in [164,168]");
            assert!(*y >= 264 && *y <= 268, "y={y} in [264,268]");
            assert_eq!(*race, 80);
            assert_eq!(name, "A");
        }
    }

    #[test]
    fn regen_monsters_mission_branch_draws_center_and_20_window() {
        let c = cfg();
        // MissionGenRate=100 ⇒ Random(100) < 100 恒真（0..99）⇒ 集中支
        let mut e = entry("A", 3, 3, 100, 166, 266);
        e.race = 80;
        let mut rng = RandomNumber::with_seed(42);
        let mut clock = || 100;
        let mut sink = Sink::new();
        let ok = regen_monsters(
            &mut rng,
            &mut e,
            3,
            &c,
            &mut clock,
            &mut |rng, map, x, y, race, name| sink.create(rng, map, x, y, race, name),
        );
        assert!(ok);
        assert_eq!(sink.calls.len(), 3);
        // 集中支：中心在 [X-Range, X+Range]；每只在中心 ±10 内
        for (_, x, y, _, _) in &sink.calls {
            assert!(*x >= 163 - 10 && *x <= 169 + 10);
            assert!(*y >= 263 - 10 && *y <= 269 + 10);
        }
    }

    #[test]
    fn regen_monsters_race_le_zero_draws_nothing() {
        let c = cfg();
        let mut e = entry("A", 3, 3, 0, 166, 266);
        e.race = -1;
        let mut rng = RandomNumber::with_seed(42);
        let mut clock = || 100;
        let mut sink = Sink::new();
        let ok = regen_monsters(
            &mut rng,
            &mut e,
            3,
            &c,
            &mut clock,
            &mut |rng, map, x, y, race, name| sink.create(rng, map, x, y, race, name),
        );
        assert!(ok);
        assert!(sink.calls.is_empty());
        assert_eq!(e.active_count, 0);
    }

    /// **R5-2**：`dw_start_tick = clock()` 必须在 `entry.race <= 0` 判断**之前**
    /// （oracle `WorldServer.MonGen.cs:816`）；注入式时钟下可判红。
    #[test]
    fn regen_monsters_reads_clock_before_race_guard() {
        let c = cfg();
        let mut e = entry("A", 1, 0, 0, 0, 0);
        e.race = 0; // race <= 0 ⇒ 早退，但先取一次时钟
        let mut rng = RandomNumber::with_seed(1);
        let mut calls = 0;
        let mut clock = || {
            calls += 1;
            100
        };
        let mut created = 0;
        let ok = regen_monsters(
            &mut rng,
            &mut e,
            1,
            &c,
            &mut clock,
            &mut |_rng, _map, _x, _y, _race, _name| {
                created += 1;
                Some(1)
            },
        );
        assert!(ok);
        assert_eq!(created, 0);
        assert_eq!(calls, 1, "R5-2：race<=0 早退前也必须先取一次 dwStartTick");
    }

    #[test]
    fn regen_monsters_normal_branch_null_returns_false() {
        let c = cfg();
        let mut e = entry("A", 2, 3, 0, 166, 266);
        e.race = 80;
        let mut rng = RandomNumber::with_seed(42);
        let mut clock = || 100;
        let mut calls = 0;
        let ok = regen_monsters(
            &mut rng,
            &mut e,
            2,
            &c,
            &mut clock,
            &mut |_rng, _, _, _, _, _| {
                calls += 1;
                None
            },
        );
        assert!(!ok, "普通支 create 返回 null ⇒ 直接 false（:857）");
        assert_eq!(calls, 1, "第一次就返回");
        assert_eq!(e.active_count, 0);
    }

    #[test]
    fn regen_monsters_zen_limit_breaks_mission_branch() {
        let c = cfg();
        let mut e = entry("A", 3, 3, 100, 166, 266);
        e.race = 80;
        let mut rng = RandomNumber::with_seed(42);
        // 第一次 clock() 为 dwStartTick=0；随后每次 clock 递增 10 ⇒ 第 1 只后 10 > 5 ⇒ 中断
        let mut t = 0;
        let mut clock = || {
            t += 10;
            t
        };
        let mut sink = Sink::new();
        let ok = regen_monsters(
            &mut rng,
            &mut e,
            3,
            &c,
            &mut clock,
            &mut |rng, map, x, y, race, name| sink.create(rng, map, x, y, race, name),
        );
        assert!(!ok, "超 ZenLimit ⇒ result=false（集中支）");
        assert_eq!(sink.calls.len(), 1);
    }

    #[test]
    fn regen_monsters_zen_limit_breaks_normal_branch() {
        let c = cfg();
        let mut e = entry("A", 3, 3, 0, 166, 266);
        e.race = 80;
        let mut rng = RandomNumber::with_seed(42);
        let mut t = 0;
        let mut clock = || {
            t += 10;
            t
        };
        let mut sink = Sink::new();
        let ok = regen_monsters(
            &mut rng,
            &mut e,
            3,
            &c,
            &mut clock,
            &mut |rng, map, x, y, race, name| sink.create(rng, map, x, y, race, name),
        );
        assert!(!ok);
        assert_eq!(sink.calls.len(), 1);
    }

    #[test]
    fn regen_gate_round_robin_and_start_tick() {
        let c = cfg();
        let mut state = MonGenState {
            entries: vec![entry("A", 1, 0, 0, 10, 10), entry("B", 1, 0, 0, 20, 20)],
            threads: BTreeMap::new(),
            monster_thread_map: BTreeMap::new(),
        };
        state.entries[0].race = 80;
        state.entries[1].race = 80;
        let mut rng = RandomNumber::with_seed(42);
        // 手工建一条 thread 0 承担两条（limit=1 的 initialize 结果）。
        let t = MonGenThread {
            entries: vec![0, 1],
            ..MonGenThread::default()
        };
        state.threads.insert(0, t);

        let mut clock = || 100;
        let mut can = |_: &str| true;
        let mut sink = Sink::new();
        // now > 0 - 0 ⇒ gate_due（初始 RegenMonstersTick=0；now=1000）
        let d1 = regen_gate(
            &mut state,
            0,
            1000,
            5000,
            &c,
            &mut rng,
            &mut clock,
            &mut can,
            &mut |rng, map, x, y, race, name| sink.create(rng, map, x, y, race, name),
        );
        assert!(d1.gate_due);
        assert_eq!(d1.entry_index, 0, "先处理下标 0");
        assert!(d1.regened);
        assert!(d1.start_tick_updated);
        assert_eq!(state.entries[0].start_tick, 1000);
        // 第二个 gate：同一 now 不满足 >200 ⇒ 不处理
        let d2 = regen_gate(
            &mut state,
            0,
            1100,
            5000,
            &c,
            &mut rng,
            &mut clock,
            &mut can,
            &mut |rng, map, x, y, race, name| sink.create(rng, map, x, y, race, name),
        );
        assert!(!d2.gate_due);
        // now 前进到 1300（>200）⇒ 轮询到下标 1
        let d3 = regen_gate(
            &mut state,
            0,
            1300,
            5000,
            &c,
            &mut rng,
            &mut clock,
            &mut can,
            &mut |rng, map, x, y, race, name| sink.create(rng, map, x, y, race, name),
        );
        assert!(d3.gate_due);
        assert_eq!(d3.entry_index, 1);
        assert_eq!(state.entries[1].start_tick, 1300);
    }

    #[test]
    fn monster_due_for_revive_and_ghost_expiry() {
        let c = cfg();
        // 正常：Death/CanReAlive/Invisible/有 MonGen，且超过 zen time
        assert!(monster_due_for_revive(
            true, true, true, true, 0, 60_000, 60_001, 5000, &c
        ));
        // 未超 zen time ⇒ false
        assert!(!monster_due_for_revive(
            true, true, true, true, 0, 60_000, 60_000, 5000, &c
        ));
        // 缺任一前置 ⇒ false
        assert!(!monster_due_for_revive(
            false, true, true, true, 0, 60_000, 99_999, 5000, &c
        ));
        assert!(!monster_due_for_revive(
            true, false, true, true, 0, 60_000, 99_999, 5000, &c
        ));
        assert!(!monster_due_for_revive(
            true, true, false, true, 0, 60_000, 99_999, 5000, &c
        ));
        assert!(!monster_due_for_revive(
            true, true, true, false, 0, 60_000, 99_999, 5000, &c
        ));
        // ghost 5 分钟
        assert!(!monster_ghost_expired(0, 5 * 60 * 1000));
        assert!(monster_ghost_expired(0, 5 * 60 * 1000 + 1));
    }

    /// 与独立 Python 复刻交叉验证 `initialize_monster` 的抽数序列
    /// （`tests/parity/world/mongen-20261013/02-crosscheck-python-mongen-rng.py`，seed=42、limit=2）。
    #[test]
    fn initialize_monster_draw_order_matches_independent_python() {
        let mut c = cfg();
        c.process_monster_multi_thread_limit = 2;
        let entries = vec![
            entry("A", 1, 0, 0, 0, 0),
            entry("B", 1, 0, 0, 0, 0),
            entry("a", 1, 0, 0, 0, 0),
            entry("C", 1, 0, 0, 0, 0),
        ];
        let list = vec!["A".to_string(), "C".to_string()];
        let mut rng = RandomNumber::with_seed(42);
        let (_state, rec) = initialize_monster(entries, &list, false, &c, &mut rng, &no_race);
        assert_eq!(rec.distinct_mon_names, 3);
        assert_eq!(rec.drawn_thread_ids, vec![1, 0, 0, 1, 0]);
    }

    /// 与独立 Python 复刻交叉验证普通支的位置序列：每只 `CreateMonster` 内部消耗
    /// `RandomByte(8)` × 2（Dir + 一次掉落抽），以钉住「位置抽 ↔ create 抽」的 interleaving。
    #[test]
    fn regen_positions_match_independent_python_normal() {
        let c = cfg();
        let mut e = entry("A", 4, 2, 0, 166, 266);
        e.race = 80;
        let mut rng = RandomNumber::with_seed(42);
        let mut clock = || 100;
        let mut xy: Vec<(i16, i16)> = Vec::new();
        let ok = regen_monsters(
            &mut rng,
            &mut e,
            4,
            &c,
            &mut clock,
            &mut |rng, _, x, y, _, _| {
                rng.random_byte(8);
                rng.random_byte(8);
                xy.push((x, y));
                Some(1)
            },
        );
        assert!(ok);
        assert_eq!(xy, vec![(167, 264), (164, 265), (164, 267), (166, 265)]);
        // 消费完这 4 只后的下一次取数也钉死，防止"位置抽少一次"这类偏移悄悄过关。
        assert_eq!(rng.random_below(1_000_000), 517_451);
    }

    /// 与独立 Python 复刻交叉验证「集中」支的位置序列。
    #[test]
    fn regen_positions_match_independent_python_mission() {
        let c = cfg();
        let mut e = entry("A", 3, 3, 100, 166, 266);
        e.race = 80;
        let mut rng = RandomNumber::with_seed(42);
        let mut clock = || 100;
        let mut xy: Vec<(i16, i16)> = Vec::new();
        let ok = regen_monsters(
            &mut rng,
            &mut e,
            3,
            &c,
            &mut clock,
            &mut |rng, _, x, y, _, _| {
                rng.random_byte(8);
                rng.random_byte(8);
                xy.push((x, y));
                Some(1)
            },
        );
        assert!(ok);
        assert_eq!(xy, vec![(163, 256), (163, 256), (158, 263)]);
    }

    /// **R5-1 用例①**：饱和（`active_count >= genModCount`）且 zen 到期 ⇒ `StartTick` 被回写、
    /// 不调 `RegenMonsters`、**不抽随机数**（oracle `WorldServer.MonGen.cs:231/244-250`）。
    #[test]
    fn regen_gate_writes_start_tick_when_saturated() {
        let mut state = MonGenState::default();
        let mut e = entry("A", 1, 0, 0, 10, 10);
        e.active_count = 1; // genModCount = max(1, Round(1 / (10/10.0))) = 1 ⇒ 饱和
        e.start_tick = 1; // 非 0 ⇒ 走 zen 判定
        state.entries.push(e);
        let mut thread = MonGenThread {
            id: 0,
            ..MonGenThread::default()
        };
        thread.entries.push(0);
        state.threads.insert(0, thread);

        let mut rng = RandomNumber::with_seed(1);
        let mut fresh = RandomNumber::with_seed(1);
        let mut clock = || 0;
        let mut can = |_: &str| true;
        let mut created = 0;
        let d = regen_gate(
            &mut state,
            0,
            1_000_000,
            0,
            &cfg(),
            &mut rng,
            &mut clock,
            &mut can,
            &mut |_rng, _, _, _, _, _| {
                created += 1;
                Some(1)
            },
        );
        assert!(d.gate_due);
        assert!(!d.regened, "饱和不调 RegenMonsters");
        assert!(
            d.start_tick_updated,
            "R5-1：饱和（boRegened 初值 true）也要回写 StartTick"
        );
        assert_eq!(state.entries[0].start_tick, 1_000_000);
        assert_eq!(created, 0);
        assert_eq!(rng.random(), fresh.random(), "饱和支不抽任何随机数");
    }

    /// **R5-1 用例②**：`can_create = false`（`boNOHUMNOMON && HumCount<=0`）且 zen 到期
    /// ⇒ `StartTick` 同样被回写、不抽随机数（否则 !canCreate 恢复后可刷时 Rust 比 oracle 早一个 zen 周期）。
    #[test]
    fn regen_gate_writes_start_tick_when_map_cannot_create() {
        let mut state = MonGenState::default();
        let mut e = entry("A", 1, 0, 0, 10, 10);
        e.active_count = 0; // genModCount = 1 > 0，但 canCreate=false 阻止进入 RegenMonsters
        e.start_tick = 1;
        state.entries.push(e);
        let mut thread = MonGenThread {
            id: 0,
            ..MonGenThread::default()
        };
        thread.entries.push(0);
        state.threads.insert(0, thread);

        let mut rng = RandomNumber::with_seed(1);
        let mut fresh = RandomNumber::with_seed(1);
        let mut clock = || 0;
        let mut can = |_: &str| false;
        let mut created = 0;
        let d = regen_gate(
            &mut state,
            0,
            1_000_000,
            0,
            &cfg(),
            &mut rng,
            &mut clock,
            &mut can,
            &mut |_rng, _, _, _, _, _| {
                created += 1;
                Some(1)
            },
        );
        assert!(d.gate_due);
        assert!(!d.regened);
        assert!(
            d.start_tick_updated,
            "R5-1：!can_create（boRegened 初值 true）也要回写 StartTick"
        );
        assert_eq!(state.entries[0].start_tick, 1_000_000);
        assert_eq!(created, 0);
        assert_eq!(rng.random(), fresh.random(), "!can_create 支不抽任何随机数");
    }

    // ---- 复活 / Ghost 5min 清理（卡 openmir2-run-gap-corpse-ghost-revive） ----------------

    /// 单条 entry + 单线程的 `MonGenState`（`MonGenInfoThreadMap[0] = [entry]`）。
    fn one_entry_state(zen_time: i32, x: i16, y: i16, range: u8, cert: Vec<i32>) -> MonGenState {
        let mut state = MonGenState::default();
        let mut entry = MonGenEntry::new("0", x, y, "鸡", range, 1, zen_time, 0);
        entry.cert_list = cert;
        state.entries.push(entry);
        let thread = MonGenThread {
            id: 0,
            entries: vec![0],
            ..MonGenThread::default()
        };
        state.threads.insert(0, thread);
        state
    }

    fn revive_corpse(world: &mut World, zen_time: i32, re_alive_tick: i64) -> i32 {
        let mut e = Entity::new(0, "鸡", 50, 50);
        e.race = 80;
        e.max_hp = 60;
        e.hp = 0;
        e.max_mp = 10;
        e.mp = 0;
        e.death = true;
        e.invisible = true;
        e.can_re_alive = true;
        e.mon_gen = Some(0);
        e.mon_gen_zen_time = zen_time;
        e.re_alive_tick = re_alive_tick;
        e.death_tick = re_alive_tick;
        e.gold = 7;
        e.stone_mode = true;
        e.fixed_hide_mode = true;
        world.add_entity(e)
    }

    /// 判据 d：到点后可复活 ⇒ **真正复活**（状态 + 位置 + 帧面）；恰等 ZenTime 不触发。
    ///
    /// **牙④**：把 `process_cert_lists` 的 `ReAliveEx` 接线删掉（只判不复活）⇒ 状态断言红。
    #[test]
    fn revivable_corpse_is_revived_with_state_position_and_frames() {
        let mut world = World::new(64, 64);
        let id = revive_corpse(&mut world, 60_000, 1_000);
        let mut state = one_entry_state(60_000, 50, 50, 0, vec![id]);
        let config = cfg();

        // 恰等 `ZenTime`：`(now - ReAliveTick) > zen` 严格大于 ⇒ 不复活。
        let rec = process_cert_lists(&mut state, &mut world, 61_000, 0, &config);
        assert!(rec.revived.is_empty(), "恰等 ZenTime 不得复活");
        let e = world.entities.get(id).unwrap();
        assert!(e.death && e.invisible, "未到点仍是尸体");

        // +1ms：复活。
        let rec = process_cert_lists(&mut state, &mut world, 61_001, 0, &config);
        assert_eq!(rec.revived.len(), 1, "到点必须真正复活");
        let e = world.entities.get(id).unwrap();
        assert!(!e.death, "Death=false");
        assert!(!e.invisible, "Invisible=false");
        assert!(!e.ghost, "不得变 Ghost");
        assert_eq!(e.hp, e.max_hp, "HP=MaxHP");
        assert_eq!(e.mp, e.max_mp, "MP=MaxMP");
        assert_eq!(e.gold, 0, "Gold=0");
        assert!(
            !e.stone_mode && !e.fixed_hide_mode,
            "StoneMode/FixedHideMode 复位"
        );
        assert_eq!(e.re_alive_tick, 61_001, "ReAliveTick 刷新");
        assert_eq!((e.x, e.y), (50, 50), "range=0 ⇒ 落位 = monGen 坐标");
        assert!(
            world.maps[0]
                .get_cell_info(50, 50)
                .0
                .obj_list
                .iter()
                .any(|o| { o.actor_object && o.cell_obj_id == id }),
            "复活必须重新 AddMapObject 落格"
        );
        // 帧面：`AddToMap` 的 RM_TURN + 尾部 RM_TURN（`BaseObject.cs:1370-1378/2747`）。
        let frames = &rec.revived[0].frames;
        assert_eq!(frames.len(), 2, "复活帧序 = AddToMap 帧 + 尾帧");
        assert_eq!(
            frames.last().copied(),
            Some(ReviveFrame {
                dir: 0,
                x: 50,
                y: 50
            }),
            "尾部 RM_TURN(Dir, CurrX, CurrY)"
        );
        assert_eq!(
            frames[0], frames[1],
            "首格 CanWalk 成功 ⇒ AddToMap 与尾帧同参"
        );
    }

    /// **复验 F1 回归**：复活必须清「死亡结算」闩锁，否则第二次死亡永不结算（复活即免死）。
    ///
    /// oracle `BaseObject.Die`（`BaseObject.Base.cs:318-345`）**每次**死亡都无条件置 `Death`/`DeathTick`，
    /// 复活后 `CanReAlive` 仍为 true ⇒ 「死→复活→再死」是合法循环。
    ///
    /// **牙⑥**：去掉 `revive_monster` 复位块里的 `e.death_settled = false` ⇒ 第一条断言红；
    /// 保留的话第二次死亡不会置 `death`（`settle_deaths` 按 `!death_settled` 过滤）。
    #[test]
    fn revived_monster_can_die_and_settle_again() {
        let mut world = World::new(64, 64);
        let id = revive_corpse(&mut world, 60_000, 1_000);
        // 首次死亡已结算 ⇒ 闩锁为 true（与生产路径一致）。
        world.entities.get_mut(id).unwrap().death_settled = true;
        let mut state = one_entry_state(60_000, 50, 50, 0, vec![id]);
        let config = cfg();

        let rec = process_cert_lists(&mut state, &mut world, 61_001, 0, &config);
        assert_eq!(rec.revived.len(), 1, "到点复活");
        assert!(
            !world.entities.get(id).unwrap().death_settled,
            "复活必须清死亡结算闩锁（复验 F1）"
        );

        // 第二次死亡：HP 归零后 tick 必须重新结算。
        let before = world.clock_ms;
        world.entities.get_mut(id).unwrap().hp = 0;
        world.tick();
        let e = world.entities.get(id).unwrap();
        assert!(e.death, "第二次死亡必须重新进入 Death");
        assert!(e.death_settled, "第二次死亡必须结算");
        assert!(e.death_tick >= before, "DeathTick 重新落账");
    }

    /// 判据 e：Ghost 后 `5min-1` 不移除、`5min`（等于）不移除、`+1` 移除并释放。
    ///
    /// **牙⑤**：删掉 `monster_ghost_expired` 的门（无条件移除）⇒ 第一条断言红。
    #[test]
    fn ghost_is_removed_only_after_five_minutes() {
        let mut world = World::new(64, 64);
        let mut e = Entity::new(0, "幽", 40, 40);
        e.race = 80;
        e.ghost = true;
        e.ghost_tick = 10_000;
        e.mon_gen = Some(0);
        let id = world.add_entity(e);
        let mut state = one_entry_state(60_000, 40, 40, 0, vec![id]);
        let config = cfg();

        let rec = process_cert_lists(&mut state, &mut world, 10_000 + 299_999, 0, &config);
        assert!(rec.removed.is_empty(), "5min-1 不得移除");
        assert!(world.entities.get(id).is_some());
        assert_eq!(state.entries[0].cert_list, vec![id]);

        let rec = process_cert_lists(&mut state, &mut world, 10_000 + 300_000, 0, &config);
        assert!(rec.removed.is_empty(), "恰等 5min 不得移除（严格 >）");

        let rec = process_cert_lists(&mut state, &mut world, 10_000 + 300_001, 0, &config);
        assert_eq!(rec.removed, vec![id], "5min+1 必须移除");
        assert!(world.entities.get(id).is_none(), "实体必须释放");
        assert!(state.entries[0].cert_list.is_empty(), "CertList.RemoveAt");
    }
}
