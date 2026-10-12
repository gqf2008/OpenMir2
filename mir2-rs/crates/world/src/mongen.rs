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
/// - `race_of`：`GetMonRace`（`WorldServer.cs:167-174`），未命中返回 `-1`。
///
/// RNG：先按**怪名**（`monsterGenMap` 的插入序）每名 1 次 `Random(limit)`，再对
/// `monster_list_names` 各 1 次 `Random(limit)`。这与 `:66` / `:94` 两处逐行对应。
///
/// 返回 [`MonGenState`]；`record` 给出读数。
pub fn initialize_monster(
    entries: Vec<MonGenEntry>,
    monster_list_names: &[String],
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
    if gen_mod_count <= i32::from(entry.active_count) || !can_create(&entry.map_name) {
        return decision;
    }
    let n_count = gen_mod_count - i32::from(entry.active_count);
    decision.n_count = n_count;
    decision.regened = true;
    let result = regen_monsters(rng, entry, n_count, config, clock, create_monster);
    decision.regen_result = result;
    if result {
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
    if entry.race <= 0 {
        return result;
    }
    let dw_start_tick = clock();
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let (state, rec) = initialize_monster(entries, &list, &cfg(), &mut rng, &no_race);
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
        let (state, _) = initialize_monster(entries, &list, &cfg(), &mut rng, &race_of);
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
        let (state, _) = initialize_monster(entries, &[], &cfg(), &mut rng, &no_race);
        // thread map 里第一条是 A（i=0），第二条 key 用 entries[1].MonName = A ⇒ 已存在不覆盖
        // ⇒ B 不会进 MonsterThreadMap（这就是 oracle 的错位缺陷，1:1 保留）。
        assert!(state.monster_thread_map.contains_key("A"));
        assert!(
            !state.monster_thread_map.contains_key("B"),
            "oracle 的 MonGenList[i] 错位：B 不写入 MonsterThreadMap"
        );
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
        let (_state, rec) = initialize_monster(entries, &list, &c, &mut rng, &no_race);
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
}
