//! 固定管理 NPC 的**世界面** —— `Envir/Npcs.txt` 记录（`LocalDB.LoadNpcs` 的
//! `WorldEngine.AddQuestNpc`）落到世界实体。
//!
//! 交付卡：`openmir2-npcs-fixed-loading`（红名单 R0-1，P0；放置半）。
//! 解析半在 `mir2-rs/crates/envir/src/npcs.rs`；宿主接线在 `mir2-rs/bins/game-svr/src/npcs.rs`。
//!
//! oracle 锚（逐行实读）：
//!
//! | 步骤 | 行号 |
//! | --- | --- |
//! | `new Merchant()/GuildOfficial()/CastleOfficial()`（`LoadNpcs` 循环内） | `LocalDB.cs:690-698` |
//! | ctor 公共链抽数（**4 抽**）`RunTick = now + Random(1500)` / `SearchTime = Random(2000)+2000` / `AttackTick = now - Random(3000)` / `WalkTick = now - Random(3000)` | `BaseObject.cs:439/441` + `AnimalObject.cs:76/77` |
//! | `WorldEngine.AddQuestNpc(npc)` → `QuestNpcList.Add` | `LocalDB.cs:708` / `WorldServer.cs:1924-1927` |
//! | `NpCinitialize`：`for (i = Count-1; i >= 0; i--)`；`FindMap(MapName)` 门；`Initialize()`（`AddToMap`）；`AddtoMapSuccess && !IsHide ⇒ RemoveAt` | `WorldServer.Merchant.cs:37-62` |
//! | 固定 NPC 的 `CellType = CellType.Merchant`（`NormNpc.cs:50` / `Merchant.cs:109`） | `NormNpc.cs:50` |
//!
//! ## 建模差异（登记）
//!
//! - **`ProcessNpcs`/`Run()` 已建模**（**世界 tick 扩容首批 T3**，卡
//!   `openmir2-world-tick-expansion`）：oracle `ProcessNpcs`（`WorldServer.Merchant.cs:118-160`）
//!   从 `NpcPosition` 正序遍历 `QuestNpcList` 调 `NormNpc.Run()`（`GuildOfficial.Run` 抽
//!   `Random(40)`/`RandomByte(8)`/`Random(30)`，`GuildOfficial.cs:39-53`；`Merchant.Run`/
//!   `CastleOfficial` 抽 `Random(50)`/`RandomByte(8)`/`Random(50)`，`Merchant.cs:132-141`）
//!   —— 见 [`World::npc_tick`] / [`npc_run`]（200ms 节奏 + `RunTime` 门；`World::tick` 里本
//!   `npc_tick` 排在 `merchant_tick` **之前**以对齐共享 RNG 消费序；oracle
//!   `MerchantProcessor.ExecuteInternal` 先 `ProcessNpcs()` 后 `ProcessMerchants()`，
//!   `MerchantProcessor.cs:28-32`）。仍**未建模**的面：① `NpcLimit` 墙钟预算断点续跑
//!   （`WorldServer.Merchant.cs:147-151`，登记 `whitelist.md` T-64）；② `Merchant.Run` 的
//!   `BoCanMove` 随机移动支（固定 NPC 经 `Npcs.txt` 装载 `BoCanMove` 恒 `false` ⇒ 不可达，
//!   见 [`NpcState::can_move`]）；③ `RM_TURN`/`RM_HIT` 帧面（属 B-4 AOI 车道，本片只落抽数
//!   与朝向状态）。注：`monster_ai_tick` 仍按 `npc.is_some()` 排除固定 NPC（固定 NPC 只由
//!   [`World::npc_tick`] 驱动）。
//! - **构造抽数（本卡订正：原为 2 抽，与 oracle 相反）**：`new Merchant()/…` 在 `LoadNpcs` 时按
//!   文件序逐条经**整条** ctor 公共链 `BaseObject`（`BaseObject.cs:439/441`）→ `AnimalObject`
//!   （`AnimalObject.cs:76/77`）抽 **4 个**数——`RunTick=now+Random(1500)`、`SearchTime=Random(2000)+2000`、
//!   `AttackTick=now-Random(3000)`、`WalkTick=now-Random(3000)`，次序即此。`NormNpc`/`Merchant`
//!   的 ctor 体本身不再抽（`NormNpc.cs:39`/`Merchant.cs:82` 只有字段赋值）。本片 `insert_npc` 用
//!   **同一条** `world.rng` 按同序抽（唯一抽数点 [`crate::entity::BaseAnimalCtorDraws::draw`]，
//!   怪物/固定 NPC/商人列表三处共用同一张表）。**原实现只抽前 2 抽 ⇒ 每只固定 NPC 少抽 2 次，
//!   共享 RNG 流自首只 NPC 构造起整体错位**（L 级订正，见
//!   `tests/parity/m3/design-buff-and-tick-20261016.md` §B-L5）。绝对值只存随机分量，绝对时钟由
//!   消费方合成（照 [`crate::entity::MonsterCtorDraws`] 的口径）。
//! - **挂图 `obj_list` 追加序**：`NpCinitialize` 倒序 ⇒ 本片宿主按倒序 [`World::attach_npc_to_map`]
//!   （与 [`crate::merchant`] 同款）；实体槽位序 = 文件正序 [`World::insert_npc`]。

use crate::entity::{BaseAnimalCtorDraws, Entity, ACTOR_RACE_NPC};
use crate::map::CellType;
use crate::World;

/// `BaseObject.RunTick = GetTickCount() + Random(1500)`（`BaseObject.cs:439`）。
///
/// 保留作 oracle 模数的具名引用；**权威抽数定义**见 [`crate::entity::BASE_ANIMAL_CTOR_DRAW_MODS`]
/// 与 [`BaseAnimalCtorDraws::draw`]（`insert_npc` 已改走后者，不再直接用本常量）。
pub const NPC_RUN_DELAY_MAX: i32 = 1500;
/// `BaseObject.SearchTime = Random(2000) + 2000`（`BaseObject.cs:441`）。
///
/// 保留作 oracle 模数的具名引用（同上，权威定义在 [`crate::entity::BASE_ANIMAL_CTOR_DRAW_MODS`]）。
pub const NPC_SEARCH_DRAW_MAX: i32 = 2000;

/// `BaseObject.RunTime = 250`（`BaseObject.cs:440`）——`ProcessNpcs` 的逐 NPC `Run` 到点门。
pub const NPC_RUN_TIME: i64 = 250;
/// `MerchantProcessor` 的定时周期 200ms（`MerchantProcessor.cs:7`）。
pub const NPC_PASS_INTERVAL_MS: i64 = 200;
/// `ProcessNpcs` 的 Ghost 过期门 `(now - GhostTick) > 60 * 1000`（`WorldServer.Merchant.cs:143`）。
pub const NPC_GHOST_EXPIRE_MS: i64 = 60 * 1000;

/// `GuildOfficial.Run` 的转身门 `Random(40)`（`GuildOfficial.cs:41`）。
pub const GUILD_TURN_RANDOM_MOD: i32 = 40;
/// `GuildOfficial.Run` 的命中门 `Random(30)`（`GuildOfficial.cs:45`）。
pub const GUILD_HIT_RANDOM_MOD: i32 = 30;
/// `Merchant.Run` 的转身/命中门 `Random(50)`（`Merchant.cs:132/138`；`CastleOfficial : Merchant`）。
pub const MERCHANT_BODY_TURN_RANDOM_MOD: i32 = 50;
/// `TurnTo(RandomByte(8))` 的取数上界（`RandomNumber.cs:117`；`RandomByte(8)` = 一抽）。
pub const NPC_TURN_DIR_MAX: u8 = 8;

/// `LocalDB.LoadNpcs` 的 `switch` 分支：`0`=`Merchant`（`LocalDB.cs:690-692`）。
pub const NPC_TYPE_MERCHANT: i32 = 0;
/// `case 1` ⇒ `GuildOfficial`（`LocalDB.cs:693-695`）。
pub const NPC_TYPE_GUILD_OFFICIAL: i32 = 1;
/// `case 2` ⇒ `CastleOfficial`（`LocalDB.cs:696-698`）。
pub const NPC_TYPE_CASTLE_OFFICIAL: i32 = 2;

/// 固定管理 NPC 的状态（`INormNpc`/`BaseObject` 在加载层可对拍的字段子集 + ctor 抽数）。
///
/// `npc_type` 是 `LoadNpcs` 的 `switch` 分支（0 商人 / 1 行会管理 / 2 沙城管理）；
/// 后四个字段是构造链（`BaseObject`+`AnimalObject`）抽出的**随机分量**（不是绝对值），
/// 定义见 [`crate::entity::BASE_ANIMAL_CTOR_DRAW_MODS`]。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NpcState {
    /// `switch` 分支（`LocalDB.cs:688-698`）：`0`=`Merchant` / `1`=`GuildOfficial` / `2`=`CastleOfficial`。
    pub npc_type: i32,
    /// `NormNpc.NpcFlag`（`LocalDB.cs:706`）。
    pub npc_flag: i16,
    /// `BaseObject.Appr`（`LocalDB.cs:707`）。
    pub appr: u16,
    /// `BaseObject` ctor 的 `Random(1500)`（`BaseObject.cs:439`）；绝对值 = 构造时刻 + 本分量。
    pub run_tick_delta: i32,
    /// `BaseObject` ctor 的 `Random(2000) + 2000`（`BaseObject.cs:441`）。
    pub base_search_time: i32,
    /// `AnimalObject` ctor 的 `Random(3000)`（`AnimalObject.cs:76`）——原 Rust 侧漏抽的第 3 个数。
    pub attack_tick_delta: i32,
    /// `AnimalObject` ctor 的 `Random(3000)`（`AnimalObject.cs:77`）——原 Rust 侧漏抽的第 4 个数。
    pub walk_tick_delta: i32,
    /// `BaseObject.RunTick`（`BaseObject.cs:439`）的**绝对值**：`insert_npc` 时 =
    /// 构造时刻 + [`NpcState::run_tick_delta`]（照 [`crate::merchant::MerchantState::run_tick`] 口径）。
    /// 缺省 `0` = 未落定（宿主 `insert_npc` 按 oracle 次序合成）。
    pub run_tick: i64,
    /// `BaseObject.GhostTick`（`BaseObject.cs:207`；`ProcessNpcs` 的 60s 过期门）。
    pub ghost_tick: i64,
    /// `IMerchant.BoCanMove`（`Merchant.cs:55`）——固定 NPC 经 `Npcs.txt` 装载**恒 `false`**
    /// （`Npcs.txt` 无此列；oracle 只在 `LoadMerchantFile` 的商人对里置真，`LocalDB.cs:1060`）。
    /// 保留字段以逐行同形承载 `Merchant.Run` 的移动支（`Merchant.cs:159`）。
    pub can_move: bool,
    /// `IMerchant.MoveTime`（秒，`Merchant.cs:56`）。
    pub move_time: i32,
    /// `IMerchant.MoveTick`（`Merchant.cs:57`；ctor = `GetTickCount()`）。
    pub move_tick: i64,
}

/// 一条待挂图的固定 NPC（宿主从 [`crate::npc`] 的 `NpcInfo` + `map_id` 装配后的入参）。
#[derive(Clone, Debug)]
pub struct NpcSpawn {
    /// `ChrName`。
    pub name: String,
    /// `CurrX`。
    pub x: i32,
    /// `CurrY`。
    pub y: i32,
    /// `FindMap(MapName)` 命中的地图下标（宿主已过门）。
    pub map_id: usize,
    pub state: NpcState,
}

impl World {
    /// **建实体（构造期）**：入 `EntityStore`（**不挂图**），并**照 oracle ctor 公共链抽 4 次数**
    /// （`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`；唯一抽数点
    /// [`BaseAnimalCtorDraws::draw`]，一处定义、三处消费）。
    ///
    /// 顺序契约：宿主必须**正序**逐条调用（实体槽位序 = `QuestNpcList` 序 = Npcs.txt 文件序），
    /// 再**倒序** [`World::attach_npc_to_map`] 落格（`NpCinitialize` 倒序）。
    ///
    /// 返回 `Some(id)` = 已入世界；`None` = `map_id` 越界（oracle 不可达，宿主 `FindMap` 门已保证）。
    pub fn insert_npc(&mut self, spawn: NpcSpawn) -> Option<i32> {
        if spawn.map_id >= self.maps.len() {
            return None;
        }
        let mut state = spawn.state;
        // oracle 构造链 `BaseObject`（`:439/441`）→ `AnimalObject`（`:76/77`）共 **4 抽**、次序固定；
        // 唯一抽数点 = `BaseAnimalCtorDraws::draw`。原 Rust 侧只抽 `BaseObject` 的 2 抽 ⇒ 每只固定
        // NPC 少抽 `AnimalObject` 的 2 抽，共享 RNG 流自首只 NPC 起整体错位（本卡订正）。
        let ctor = BaseAnimalCtorDraws::draw(&mut self.rng);
        state.run_tick_delta = ctor.run_tick_delta;
        state.base_search_time = ctor.base_search_time;
        state.attack_tick_delta = ctor.attack_tick_delta;
        state.walk_tick_delta = ctor.walk_tick_delta;
        // 绝对 `RunTick`（oracle `ctor` 的 `GetTickCount()` 就是当时的 sim 时钟 == 构造时刻）。
        state.run_tick = self.clock_ms + i64::from(state.run_tick_delta);
        // `Merchant.cs:108` `MoveTick = HUtil32.GetTickCount()`（`Merchant`/`NormNpc` ctor 尾）。
        state.move_tick = self.clock_ms;

        let mut e = Entity::new(0, spawn.name, spawn.x, spawn.y);
        // 唯一 id（照 [`World::insert_merchant`] 的口径：`Entity::new(0,…)` 的占位必须换掉）。
        e.id = self.entities.alloc_id();
        // `NormNpc.cs:42` `Race = ActorRace.NPC`（=10）；`Merchant` 继承它（只设 `RaceImg`）。
        e.race = ACTOR_RACE_NPC;
        e.map_id = spawn.map_id;
        e.npc = Some(state);
        Some(self.entities.insert(e))
    }

    /// **挂图**：执行 oracle `BaseObject.Initialize()` 的挂图段
    /// `AddtoMapSuccess = !(CanWalk(CurrX,CurrY,true) && AddToMap())`；为真 ⇒ 从世界移除
    /// （`NpCinitialize` 的 `if (normNpc.AddtoMapSuccess && !normNpc.IsHide) RemoveAt`；`IsHide`
    /// 在 `LoadNpcs` 路径恒 `false`（`NormNpc.cs:45` ctor 初值 + `LoadNpcs` 不设））。
    ///
    /// 宿主按 `NpCinitialize` 的**倒序**逐条调用（map cell `obj_list` 追加序 = 倒序）；
    /// 该序与 [`World::insert_npc`] 决定的实体槽位序**互相独立**。
    ///
    /// 返回 `true` = NPC 留在世界；`false` = 挂图失败被移除（oracle `RemoveAt`）。
    pub fn attach_npc_to_map(&mut self, id: i32) -> bool {
        let Some(entity) = self.entities.get(id) else {
            return false;
        };
        let (map_id, x, y) = (entity.map_id, entity.x, entity.y);
        if map_id >= self.maps.len() {
            return false;
        }
        let now = self.clock_ms;
        // `Envir.CanWalk(CurrX, CurrY, true)`：只看格子是否可走（忽略活体阻挡）。
        // `MapGrid::cell_valid` 对越界返回 `true`（复刻 C# `CellValid` 方向）⇒ 先 `cell_match` 判界。
        let (can_walk, added) = {
            let map = &mut self.maps[map_id];
            let can_walk = map.cell_match(x, y) && map.cell_valid(x, y);
            // `NormNpc.CellType = CellType.Merchant`（`NormNpc.cs:50`）。
            let added = map.add_map_object(x, y, CellType::Merchant, id, now);
            (can_walk, added)
        };
        let add_to_map_success = !(can_walk && added);
        if add_to_map_success {
            // oracle `LogService.Warn("... Npc Initalize fail...") + RemoveAt(i)`。
            self.entities.remove(id);
            return false;
        }
        if let Some(ent) = self.entities.get_mut(id) {
            ent.add_to_mapped = !add_to_map_success;
        }
        true
    }

    /// 一步到位：建实体（[`World::insert_npc`]）+ 挂图（[`World::attach_npc_to_map`]）。
    ///
    /// 生产装载按 oracle 两段次序**分两步**调用（正序 insert、倒序 attach，见宿主
    /// `place_npcs`）；本函数保留给单测 / 无预抽的直接挂图场景。
    ///
    /// 返回 `Some(id)` = NPC 留在世界；`None` = `map_id` 越界或挂图失败被移除。
    pub fn spawn_npc(&mut self, spawn: NpcSpawn) -> Option<i32> {
        let id = self.insert_npc(spawn)?;
        if self.attach_npc_to_map(id) {
            Some(id)
        } else {
            None
        }
    }

    /// 按**槽位顺序**列出所有固定 NPC 实体 id（`QuestNpcList` 的确定性等价物）。
    ///
    /// 槽位序 = [`World::insert_npc`] 的插入序 = Npcs.txt 文件序。
    #[must_use]
    pub fn npc_ids_sorted(&self) -> Vec<i32> {
        self.entities
            .iter()
            .filter(|e| e.npc.is_some())
            .map(|e| e.id)
            .collect()
    }

    /// **`ProcessNpcs`**（`WorldServer.Merchant.cs:118-160`）的单线程等价物。
    ///
    /// oracle 是 **200ms 定时线程**（`MerchantProcessor.cs:7`）+ 持久 `NpcPosition` 索引 +
    /// 逐 NPC `(now - RunTick) > RunTime` 门（`RunTime` = 250，`WorldServer.Merchant.cs:131`）
    /// 加墙钟预算 `(now - dwRunTick) > M2Share.NpcLimit` 断点续跑
    /// （`WorldServer.Merchant.cs:147-151`）与 Ghost 超 60s 移除（`:140-145`）。
    ///
    /// **本片口径**（照 [`World::merchant_tick`] 的 T-42 裁决）：
    /// 1. 节奏 200ms 用 **sim 时钟整数毫秒**（不是墙钟；同一窗口的第二次调用 no-op）；
    /// 2. `RunTick/RunTime` 门**逐字照搬**（`NpcPosition` 正序 = `QuestNpcList` 序 = Npcs.txt 文件序）；
    /// 3. **丢掉墙钟预算分支**（`NpcLimit=5`，见 `whitelist.md` T-42）⇒ 一趟扫到底、扫完
    ///    [`World::npc_position`] 归 0；
    /// 4. Ghost 用 sim 时钟的同单位整数（不是墙钟）。
    ///
    /// 跨 pass 交错（`NpcLimit` 预算决定某个该跑的 NPC 落在哪一 pass）**不在判据内**；
    /// 同一 pass 内次序 = 槽位序。RNG 取数**逐次**发生在 [`npc_run`]（`#[track_caller]` 站点归
    /// `npc_ai` 组；oracle `GuildOfficial.Run`/`Merchant.Run`）。
    pub fn npc_tick(&mut self) -> NpcTickOutcome {
        let now = self.clock_ms;
        let mut out = NpcTickOutcome::default();
        // 节奏门：同一 200ms 窗口内只跑一趟（`MerchantProcessor` 的 200ms 定时器，sim 时钟）。
        if now.saturating_sub(self.npc_pass_tick) < NPC_PASS_INTERVAL_MS {
            return out;
        }
        self.npc_pass_tick = now;
        let ids = self.npc_ids_sorted();
        let mut i = self.npc_position;
        while i < ids.len() {
            let id = ids[i];
            let (is_ghost, run_tick, ghost_tick) = match self.entities.get(id) {
                Some(e) => {
                    let (rt, gt) = e
                        .npc
                        .as_ref()
                        .map_or((0, 0), |n| (n.run_tick, n.ghost_tick));
                    (e.ghost, rt, gt)
                }
                None => {
                    i += 1;
                    continue;
                }
            };
            if !is_ghost {
                if (now - run_tick) > NPC_RUN_TIME {
                    // oracle 先写 `RunTick = now` 再 `Run()`（`WorldServer.Merchant.cs:133-134`）。
                    if let Some(n) = self.entities.get_mut(id).and_then(|e| e.npc.as_mut()) {
                        n.run_tick = now;
                    }
                    let r = npc_run(self, id);
                    out.ran.push(id);
                    if let Some(dir) = r.turned {
                        out.turns.push((id, dir));
                    }
                    if r.hit {
                        out.hits.push(id);
                    }
                }
            } else if (now - ghost_tick) > NPC_GHOST_EXPIRE_MS {
                // oracle `QuestNpcList.RemoveAt(i); break;`（移除后本 pass 不再跑后面的）。
                self.entities.remove(id);
                out.removed.push(id);
                break;
            }
            i += 1;
        }
        // oracle `if (!boProcessLimit) NpcPosition = 0;` —— 本片无预算分支 ⇒ 每 pass 归 0。
        self.npc_position = 0;
        out
    }
}

/// 一个固定 NPC 的一次 `Run()`（`WorldServer.Merchant.cs:134` 的 `normNpc.Run()`）。
///
/// 分派依据 = [`NpcState::npc_type`]（`LocalDB.cs:688-698` 的 `switch` 同口径）：
/// * `1`（`GuildOfficial`）⇒ [`guild_official_run_body`]：`Random(40)` + [`RandomByte(8)` |
///   `Random(30)`]，**恒 2 抽**；
/// * `0`/`2`（`Merchant`/`CastleOfficial : Merchant`）⇒ [`merchant_body_run`]：`Random(50)` +
///   [`RandomByte(8)` | `Random(50)`]，**恒 2 抽**；`BoCanMove` 到点再 `MapRandomMove` **2 抽**
///   （固定 NPC 经 `Npcs.txt` 装载恒 `BoCanMove == false` ⇒ 常态 2 抽）。
///
/// 尾部 `base.Run()`（`BaseObject.Base.cs:25-316`）对无消息 NPC：`GetMessage` 空、恢复段无 RNG、
/// `SuperMan` 否、`!Death` ⇒ 恢复/`Die`/靶标清理；**不抽数**（见 B-L4）。本片**只落抽数与朝向状态**
/// （`RM_TURN`/`RM_HIT` 出帧属 B-4，帧面接线随 AOI 车道）。
pub fn npc_run(world: &mut World, actor_id: i32) -> NpcRunOutcome {
    let npc_type = world
        .entities
        .get(actor_id)
        .and_then(|e| e.npc.as_ref())
        .map(|n| n.npc_type);
    let mut outcome = NpcRunOutcome::default();
    match npc_type {
        // 不可达（`NpcType::from_raw` 已过滤非 {0,1,2}；`entity.npc` 为 None 时上面 `None`）。
        None => return outcome,
        Some(NPC_TYPE_GUILD_OFFICIAL) => guild_official_run_body(world, actor_id, &mut outcome),
        // `CastleOfficial : Merchant`（`CastleOfficial.cs:13`）⇒ 同一份 `Merchant.Run` 体。
        Some(NPC_TYPE_MERCHANT | NPC_TYPE_CASTLE_OFFICIAL) => {
            merchant_body_run(world, actor_id, &mut outcome);
        }
        // oracle `switch` 只产 0/1/2（其余 `npc = null` 整行跳过）⇒ 其余值不可达。
        Some(_) => {}
    }
    outcome
}

/// `GuildOfficial.Run`（`GuildOfficial.cs:39-53`）：`Random(40)` 命中 ⇒ `TurnTo(RandomByte(8))`；
/// 否则 `Random(30)` 命中 ⇒ `RM_HIT`。**两条路径都恒 2 抽**（`RandomByte(8)` 也是一抽）。
fn guild_official_run_body(world: &mut World, actor_id: i32, outcome: &mut NpcRunOutcome) {
    if world.rng.random_below(GUILD_TURN_RANDOM_MOD) == 0 {
        let dir = world.rng.random_byte(NPC_TURN_DIR_MAX);
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.dir = dir;
        }
        outcome.turned = Some(dir);
    } else if world.rng.random_below(GUILD_HIT_RANDOM_MOD) == 0 {
        outcome.hit = true;
    }
}

/// `Merchant.Run`（`Merchant.cs:112-171`）对**固定 NPC** 的等价体（`CastleOfficial` 继承）。
///
/// 抽数：转身/命中段 `Random(50)` **恒 1 抽**；命中 `0` ⇒ `RandomByte(8)` **再 1 抽**；否则
/// `Random(50)` **再 1 抽** ⇒ 此段**恒 2 抽**。`BoCanMove && (now - MoveTick) > MoveTime*1000`
/// 为真 ⇒ `MapRandomMove` **再 2 抽**（`BaseObject.cs:1235-1236`）。
///
/// **本片省略（固定 NPC 不可达，登记）**：
/// * `RefillGoods`/`ClearExpreUpgradeListData` 两个门（`Merchant.cs:117-131`）——固定 NPC 经
///   `Npcs.txt` 装载**不含** `[goods]` 脚本（`LoadMerchantScript` 未跑）⇒ `GoodsList` 空、
///   两者皆无可观测效果；引 RNG 的是转身/命中段，与本段无关。
/// * 沙巴克显隐段（`Merchant.cs:143-158`）——固定 NPC `CastleMerchant` 恒 `false`（ctor 缺省，
///   `Npcs.txt` 无此列）且 `FixedHideMode` 恒 `false` ⇒ oracle 走 else 支且 `FixedHideMode == false`
///   ⇒ **零动作**。
fn merchant_body_run(world: &mut World, actor_id: i32, outcome: &mut NpcRunOutcome) {
    let now = world.clock_ms;
    // `Merchant.cs:132-141`：`Random(50)==0` ⇒ `TurnTo(RandomByte(8))`；否则 `Random(50)==0` ⇒ `RM_HIT`。
    if world.rng.random_below(MERCHANT_BODY_TURN_RANDOM_MOD) == 0 {
        let dir = world.rng.random_byte(NPC_TURN_DIR_MAX);
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.dir = dir;
        }
        outcome.turned = Some(dir);
    } else if world.rng.random_below(MERCHANT_BODY_TURN_RANDOM_MOD) == 0 {
        outcome.hit = true;
    }
    // `Merchant.cs:159-164`：`BoCanMove && (now - MoveTick) > MoveTime*1000` ⇒ `MapRandomMove`。
    let Some((can_move, move_time, move_tick)) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.npc.as_ref())
        .map(|n| (n.can_move, n.move_time, n.move_tick))
    else {
        return;
    };
    if can_move && (now - move_tick) > i64::from(move_time) * 1000 {
        if let Some(n) = world
            .entities
            .get_mut(actor_id)
            .and_then(|e| e.npc.as_mut())
        {
            n.move_tick = now;
        }
        crate::merchant::map_random_move(world, actor_id);
        outcome.space_moved = true;
    }
}

/// 一个固定 NPC 的一次 `Run()` 结果（供 `npc_tick` 与单测；帧面接线属 B-4）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcRunOutcome {
    /// `TurnTo(RandomByte(8))` 抽到的朝向（`RM_TURN`）。
    pub turned: Option<u8>,
    /// 发 `RM_HIT`（`Random(30)`/`Random(50)` 命中支）。
    pub hit: bool,
    /// `BoCanMove` 到点 ⇒ `MapRandomMove`（固定 NPC 恒 `false` ⇒ 常态 `false`）。
    pub space_moved: bool,
}

/// `ProcessNpcs` 一趟的结果（`World::tick` 写进 [`crate::world::TickRecord`]）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcTickOutcome {
    /// 本 pass 执行了 `Run()` 的 NPC id（按槽位序）。
    pub ran: Vec<i32>,
    /// 本 pass `TurnTo` 的结果：`(npc_id, dir)`（`RM_TURN`；帧面归 B-4）。
    pub turns: Vec<(i32, u8)>,
    /// 本 pass 发 `RM_HIT` 的 NPC id。
    pub hits: Vec<i32>,
    /// 本 pass 被 Ghost 60s 门移除的 NPC id（oracle `RemoveAt`）。
    pub removed: Vec<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn(name: &str, map_id: usize, x: i32, y: i32) -> NpcSpawn {
        NpcSpawn {
            name: name.to_string(),
            x,
            y,
            map_id,
            state: NpcState {
                npc_type: 0,
                npc_flag: 0,
                appr: 0,
                run_tick_delta: 0,
                base_search_time: 0,
                ..NpcState::default()
            },
        }
    }

    /// `spawn_npc` 建实体并挂图：`race = ActorRace.NPC`、`map_id/(x,y)` 落定、`npc` 置位。
    #[test]
    fn spawn_npc_places_entity_with_npc_race() {
        let mut world = World::with_maps(vec![(8, 8)]);
        let id = world.spawn_npc(spawn("老人", 0, 3, 3)).expect("挂图成功");
        assert_eq!(world.npc_ids_sorted(), vec![id]);
        let e = world.entities.get(id).unwrap();
        assert_eq!((e.race, e.map_id, e.x, e.y), (ACTOR_RACE_NPC, 0, 3, 3));
        assert!(e.npc.is_some());
    }

    /// **牙齿**：`monster_ai_tick` 必须跳过固定 NPC（`monai.rs` 的 `e.npc.is_none()` 过滤）。
    ///
    /// 把 `clock_ms` 拉高到 `think` 门（3000ms）之后：若 NPC 被当怪驱动 ⇒ `think()`
    /// 会把 `ai.think_tick` 刷成 `now` ⇒ 本断言必红（去 `npc.is_none()` 过滤即复现）。
    #[test]
    fn npc_is_excluded_from_monster_ai_tick() {
        let mut world = World::with_maps(vec![(8, 8)]);
        let id = world.spawn_npc(spawn("老人", 0, 3, 3)).expect("挂图成功");
        world.clock_ms = 10_000;
        crate::monai::monster_ai_tick(&mut world);
        assert_eq!(
            world.entities.get(id).unwrap().ai.think_tick,
            0,
            "NPC 不得被 monster_ai_tick 驱动（去 npc.is_none() 过滤 ⇒ think_tick 被刷成 now ⇒ 红）"
        );
    }

    // ------------------------------------------------------------------
    // Part B（NPC Run）判据 —— B-L1/L2/L3/L4/L5 + RNG 抽数序列（本批最重要）
    // ------------------------------------------------------------------

    use mir2_shared::rng::RandomNumber;

    fn spawn_typed(name: &str, npc_type: i32, map_id: usize, x: i32, y: i32) -> NpcSpawn {
        NpcSpawn {
            name: name.to_string(),
            x,
            y,
            map_id,
            state: NpcState {
                npc_type,
                ..NpcState::default()
            },
        }
    }

    /// **RNG 抽数序列（本批最重要的判据）**：脚本化一个 `GuildOfficial`（`npc_type=1`），
    /// 对一批种子逐一生断言 `npc_run` 的取数 **kind + 次序 + 次数** 与 oracle
    /// `GuildOfficial.cs:39-53` 推出的期望序列逐项一致（用一个同种子独立流逐抽复算）。
    ///
    /// oracle：`Random(40)==0 ⇒ TurnTo(RandomByte(8))`；否则 `Random(30)==0 ⇒ RM_HIT`。
    /// **两条路径都恒 2 抽**（`RandomByte(8)` 也是一抽）。
    ///
    /// **为何扫种子**：`RandomNumber`（.NET legacy 复刻）对任意 `n` 的 `Next(n)` 消耗**同一份**
    /// 内部状态 ⇒ 单看「抽完两流仍同源」只能咬住**次数**，咬不住**参数值**（`Next(40)` 与
    /// `Next(30)` 的状态推进相同）。故本测试用**字面量 oracle 参数**（40/8/30）复算期望的
    /// `(turned,hit)`，并在多种子上比 —— 参数写错（如把 40 与 30 对调、把 `RandomByte(8)` 写成
    /// `Random(8)` 之外的分支次序）会让某些种子的零判定落进可区分的窗口 ⇒ 必红。
    ///
    /// **模数 ±1 的区分窗口极窄**（`random_below(40)` 与 `random_below(41)` 的 `==0` 概率仅差
    /// ~1/1640/seed）⇒ 本测试扫 **20000** 种子，否则 ±1 变异可能全绿（小样本咬不住模数；
    /// `random_below` 的状态推进与模数无关，只有返回**值**的布尔可观察量能咬）。
    #[test]
    fn npc_run_guild_official_draws_two_kinds_in_order() {
        // 世界/实体建一次；每轮只**重播种**共享流（`npc_run` 起于位置 0）⇒ 可与独立流逐抽对齐。
        // 须扫足够多种子：`random_below(40)` 与 `random_below(41)` 的 `==0` 窗口仅差 ~1/1640 概率
        // ⇒ 小种子数咬不住模数，20000 才让 ±1 变异落在可区分窗口（见证据）。
        let mut world = World::with_maps(vec![(20, 20)]);
        let id = world
            .spawn_npc(spawn_typed("比奇国王", NPC_TYPE_GUILD_OFFICIAL, 0, 3, 3))
            .expect("挂图成功");
        for seed in 0..20_000i32 {
            world.rng = RandomNumber::with_seed(seed);
            let mut exp = RandomNumber::with_seed(seed);
            let out = npc_run(&mut world, id);
            // 期望（**字面量 oracle 参数**：40 → [8 | 30]）。
            let expect = if exp.random_below(40) == 0 {
                (Some(exp.random_byte(8)), false)
            } else {
                (None, exp.random_below(30) == 0)
            };
            assert_eq!((out.turned, out.hit), expect, "seed={seed} 取数 kind/次序");
            assert_eq!(world.rng.random(), exp.random(), "seed={seed} 两流同源");
            if let Some(d) = out.turned {
                assert_eq!(world.entities.get(id).unwrap().ai.dir, d, "TurnTo 置 Dir");
            }
        }
    }

    /// 脚本化一个 `Merchant` 类（`npc_type=0`，`CastleOfficial` 同体）NPC：对一批种子断言
    /// `Merchant.Run` 的转身/命中段**恒 2 抽**（`Random(50)` + [`RandomByte(8)`|`Random(50)`]），
    /// 且固定 NPC `BoCanMove=false` ⇒ **不触** `MapRandomMove` 的 2 抽（`Merchant.cs:159` 门不满足）。
    /// 期望用**字面量 oracle 参数**（50/8/50）复算 ⇒ 参数写错必在某些种子红。
    ///
    /// 同 guild 测试：**扫 20000 种子**才能咬住 `Random(50)` 模数 ±1（区分窗口 ~1/2550/seed）。
    #[test]
    fn npc_run_merchant_body_draws_two_and_no_map_move() {
        let mut world = World::with_maps(vec![(20, 20)]);
        let id = world
            .spawn_npc(spawn_typed("老人", NPC_TYPE_MERCHANT, 0, 4, 4))
            .expect("挂图成功");
        // 大种子扫描（同 guild 测试口径）：咬住 `Random(50)` 模数 ±1。
        for seed in 0..20_000i32 {
            world.rng = RandomNumber::with_seed(seed);
            let mut exp = RandomNumber::with_seed(seed);
            world.clock_ms = 500_000; // 远超 MoveTime*1000（但 can_move=false ⇒ 门不成立）
            let out = npc_run(&mut world, id);
            // 期望（字面量 oracle 参数：50 → [8 | 50]）。
            let expect = if exp.random_below(50) == 0 {
                (Some(exp.random_byte(8)), false)
            } else {
                (None, exp.random_below(50) == 0)
            };
            assert_eq!((out.turned, out.hit), expect, "seed={seed} 转身/命中段取数");
            assert!(
                !out.space_moved,
                "seed={seed} BoCanMove=false ⇒ 无 MapRandomMove"
            );
            assert_eq!(
                world.rng.random(),
                exp.random(),
                "seed={seed} 两流同源（恰 2 抽）"
            );
        }
    }

    /// `CastleOfficial`（`npc_type=2`）与 `Merchant` **同一份 Run 体**（`CastleOfficial.cs:13`
    /// 继承）⇒ 取数形态与 `npc_type=0` 相同。
    #[test]
    fn npc_run_castle_official_shares_merchant_body() {
        let mut world = World::with_maps(vec![(20, 20)]);
        world.rng = RandomNumber::with_seed(777);
        let id = world
            .spawn_npc(spawn_typed(
                "沙巴克城堡管理",
                NPC_TYPE_CASTLE_OFFICIAL,
                0,
                5,
                5,
            ))
            .expect("挂图成功");
        let mut exp = RandomNumber::with_seed(777);
        // 复算构造期抽数：整条 ctor 公共链 4 抽（唯一抽数点），再复算 `Run` 段。
        let _ = BaseAnimalCtorDraws::draw(&mut exp);
        world.clock_ms = 10_000;
        let out = npc_run(&mut world, id);
        // 复算 `Merchant.Run` 转身/命中段（分支逐抽对齐；**字面量 50/8/50**）。
        let expect = if exp.random_below(50) == 0 {
            (Some(exp.random_byte(8)), false)
        } else {
            (None, exp.random_below(50) == 0)
        };
        assert_eq!((out.turned, out.hit), expect);
        assert_eq!(
            world.rng.random(),
            exp.random(),
            "CastleOfficial 复用 Merchant.Run 体"
        );
    }

    /// **B-L5（构造抽不回归，⚠ 订正：原为「恰 2 次」）**：`insert_npc` 按 oracle **整条** ctor 公共链
    /// 恰抽 **4 次**（`BaseObject` `Random(1500)`→`Random(2000)+2000`，再 `AnimalObject`
    /// `Random(3000)`→`Random(3000)`；`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`），加入
    /// `npc_tick` 后构造段账本不变。**订正缘由**：原断言「恰 2 抽」与 oracle **相反**（少抽
    /// `AnimalObject` 的 2 抽）⇒ 共享 RNG 流自首只 NPC 构造起整体错位，且该错值被本断言**钉死**。
    ///
    /// **字面量钉死模数**：`1500`/`2000`/`3000` 改 ±1 ⇒ 对应分量值断言必红（`random_below` 的状态
    /// 推进与模数无关 ⇒ 只有「值」断言能咬住模数，两流对齐咬不住）。
    ///
    /// **为何扫种子（T3-fix2）**：`random_below(1500)` 的**返回值**对 `1500→1499` 这类 -1 变异
    /// 只在「原始采样落在被截掉的末档」时才变（单种子的区分概率远低于 1）⇒ 单种子（旧 `seed=1234`）
    /// 对 -1 方向**可能全绿**（复验者实测：`1500→1499` 该具名断言仍绿，仅 `+1` 被抓）。扫 **20000**
    /// 种子后 ±1 两方向都落进可区分窗口。
    #[test]
    fn insert_npc_still_draws_exactly_four() {
        // 每轮只**重播种**共享流（`insert_npc` 构造抽起于位置 0）⇒ 可与独立流逐抽对齐；
        // 实体只增不删（本测试不挂图，只读该轮 id 的 npc 分量）。
        let mut world = World::with_maps(vec![(20, 20)]);
        for seed in 0..20_000i32 {
            world.rng = RandomNumber::with_seed(seed);
            let id = world
                .insert_npc(spawn_typed("老人", NPC_TYPE_MERCHANT, 0, 3, 3))
                .expect("map_id 合法");
            let mut exp = RandomNumber::with_seed(seed);
            // 字面量 4 抽：`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`。
            let d = exp.random_below(1500);
            let sq = exp.random_below(2000) + 2000;
            let at = exp.random_below(3000);
            let wk = exp.random_below(3000);
            let n = world.entities.get(id).unwrap().npc.as_ref().unwrap();
            assert_eq!(
                n.run_tick_delta, d,
                "seed={seed} RunTick 分量 = Random(1500)"
            );
            assert_eq!(
                n.base_search_time, sq,
                "seed={seed} SearchTime = Random(2000)+2000"
            );
            assert_eq!(
                n.attack_tick_delta, at,
                "seed={seed} AttackTick 分量 = Random(3000)"
            );
            assert_eq!(
                n.walk_tick_delta, wk,
                "seed={seed} WalkTick 分量 = Random(3000)"
            );
            assert_eq!(world.rng.random(), exp.random(), "seed={seed} 构造恰 4 抽");
        }
    }

    /// **B-L2（节奏门）＋ `RunTime`/pass 字面量钉死**：`npc_tick`（200ms 节奏）× `RunTime`（250ms）
    /// 门的**边界两侧**；drive 开关牙见 `world_tick_drives_npc_only_when_enabled`。
    ///
    /// 字面量硬编码 250/200 ⇒ `NPC_RUN_TIME`/`NPC_PASS_INTERVAL_MS` 改 ±1 ⇒ 必红（见证据）。
    #[test]
    fn npc_tick_cadence_and_run_time_gate() {
        let mut world = World::with_maps(vec![(20, 20)]);
        world.rng = RandomNumber::with_seed(9);
        let a = world
            .spawn_npc(spawn_typed("老人", NPC_TYPE_MERCHANT, 0, 2, 2))
            .expect("挂图成功");
        let set_rt = |w: &mut World, v: i64| {
            if let Some(n) = w.entities.get_mut(a).and_then(|e| e.npc.as_mut()) {
                n.run_tick = v;
            }
        };
        let reset_pass = |w: &mut World| w.npc_pass_tick = i64::MIN;

        // --- `RunTime` = 250（字面量）：run_tick=0 时 `(now-0) > 250` 的边界两侧。
        set_rt(&mut world, 0);
        world.clock_ms = 250;
        assert!(world.npc_tick().ran.is_empty(), "250-0 不 > 250 ⇒ 不 Run");
        reset_pass(&mut world);
        set_rt(&mut world, 0);
        world.clock_ms = 251;
        assert_eq!(world.npc_tick().ran, vec![a], "251-0 > 250 ⇒ Run");

        // --- pass = 200（字面量）：`(now-pass_tick) < 200` 的边界两侧。
        reset_pass(&mut world);
        set_rt(&mut world, 0);
        world.clock_ms = 10_000;
        assert_eq!(world.npc_tick().ran, vec![a], "窗口起点 Run");
        set_rt(&mut world, 0);
        world.clock_ms = 10_199;
        assert!(world.npc_tick().ran.is_empty(), "199ms < 200 ⇒ no-op");
        set_rt(&mut world, 0);
        world.clock_ms = 10_200;
        assert_eq!(world.npc_tick().ran, vec![a], "恰 200ms ⇒ 放开");
    }

    /// **驱动开关牙（改坏必红）**：`World::tick` 在 `npc_enabled=false`（缺省）时**不**驱动
    /// `npc_tick` ⇒ `rec.npcs_run` 恒空；`enable_npc_tick()` 后同一场景 ⇒ 必跑。
    ///
    /// 去掉 `World::tick` 里的 `if self.npc_enabled { … }` 块、或把 `npc_enabled` 缺省置真，
    /// 都会让下面两条断言之一变红。
    #[test]
    fn world_tick_drives_npc_only_when_enabled() {
        let mut world = World::with_maps(vec![(20, 20)]);
        world.rng = RandomNumber::with_seed(5);
        let a = world
            .spawn_npc(spawn_typed("老人", NPC_TYPE_MERCHANT, 0, 3, 3))
            .expect("挂图成功");
        if let Some(n) = world.entities.get_mut(a).and_then(|e| e.npc.as_mut()) {
            n.run_tick = 0;
        }
        world.clock_ms = 10_000;
        let rec = world.tick();
        assert!(
            rec.npcs_run.is_empty(),
            "缺省不对（NPC 线程未启用）⇒ tick 不驱动"
        );

        world.enable_npc_tick();
        if let Some(n) = world.entities.get_mut(a).and_then(|e| e.npc.as_mut()) {
            n.run_tick = 0;
        }
        let rec = world.tick();
        assert_eq!(rec.npcs_run, vec![a], "启用后 tick 必驱动 ProcessNpcs");
    }

    /// **B-L3（次序）**：多个 NPC（含不同 `npc_type`）按**槽位序**触发 ⇒ `ran` 次序 =
    /// 槽位序（= `QuestNpcList` 序 = Npcs.txt 文件序）。
    #[test]
    fn npc_tick_runs_in_slot_order() {
        let mut world = World::with_maps(vec![(20, 20)]);
        world.rng = RandomNumber::with_seed(31);
        world.enable_npc_tick();
        let a = world
            .spawn_npc(spawn_typed("甲", NPC_TYPE_GUILD_OFFICIAL, 0, 1, 1))
            .expect("挂图成功");
        let b = world
            .spawn_npc(spawn_typed("乙", NPC_TYPE_MERCHANT, 0, 2, 2))
            .expect("挂图成功");
        let c = world
            .spawn_npc(spawn_typed("丙", NPC_TYPE_CASTLE_OFFICIAL, 0, 3, 3))
            .expect("挂图成功");
        for id in [a, b, c] {
            if let Some(n) = world.entities.get_mut(id).and_then(|e| e.npc.as_mut()) {
                n.run_tick = 0;
            }
        }
        world.clock_ms = 10_000;
        assert_eq!(world.npc_tick().ran, vec![a, b, c], "槽位序");
    }

    /// Ghost 60s 过期门（`WorldServer.Merchant.cs:140-145`）用 **sim 时钟**；移除后
    /// `npc_ids_sorted` 不再含它（照 `merchant_tick` 同款）。
    #[test]
    fn npc_ghost_expires_after_60s_sim_clock() {
        let mut world = World::with_maps(vec![(20, 20)]);
        world.rng = RandomNumber::with_seed(11);
        world.enable_npc_tick();
        let a = world
            .spawn_npc(spawn_typed("老人", NPC_TYPE_MERCHANT, 0, 4, 4))
            .expect("挂图成功");
        if let Some(e) = world.entities.get_mut(a) {
            e.ghost = true;
        }
        if let Some(n) = world.entities.get_mut(a).and_then(|e| e.npc.as_mut()) {
            n.ghost_tick = 0;
        }
        world.clock_ms = 60_000; // 字面量：`(now-GhostTick) > 60*1000`
        assert!(
            world.npc_tick().removed.is_empty(),
            "60000-0 不 > 60000 ⇒ 不移除"
        );
        world.npc_pass_tick = i64::MIN; // 重开 pass 窗口（否则 1ms 内被 200ms 节奏门拦下）
        world.clock_ms = 60_001;
        assert_eq!(
            world.npc_tick().removed,
            vec![a],
            "60001-0 > 60000 ⇒ 移除（RemoveAt）"
        );
        assert!(world.npc_ids_sorted().is_empty());
    }

    /// **B-L4（`base.Run` 无抽数）**：无消息 NPC 的 `npc_run` 尾部 `base.Run` 段**新增 0 抽**——
    /// 取数完全由 Run 体（2 抽）决定（上面对齐断言已覆盖；此处再以负向断言钉死）。
    #[test]
    fn npc_run_tail_draws_nothing() {
        let mut world = World::with_maps(vec![(20, 20)]);
        world.rng = RandomNumber::with_seed(64);
        let id = world
            .spawn_npc(spawn_typed("老人", NPC_TYPE_GUILD_OFFICIAL, 0, 3, 3))
            .expect("挂图成功");
        let mut exp = RandomNumber::with_seed(64);
        // 构造期整条 ctor 公共链 4 抽（唯一抽数点）；`Run` 段随后复算。
        let _ = BaseAnimalCtorDraws::draw(&mut exp);
        world.clock_ms = 10_000;
        let _ = npc_run(&mut world, id);
        // oracle 恰 2 抽（字面量 40/8/30）：分支逐抽对齐后两流同源即证明尾部无额外抽数。
        if exp.random_below(40) == 0 {
            let _ = exp.random_byte(8);
        } else {
            let _ = exp.random_below(30);
        }
        assert_eq!(world.rng.random(), exp.random(), "base.Run 段零抽数");
    }
}
