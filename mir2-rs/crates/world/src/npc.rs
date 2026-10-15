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
//! | `BaseObject` ctor 抽数 `RunTick = now + Random(1500)` / `SearchTime = Random(2000) + 2000` | `BaseObject.cs:439/441` |
//! | `WorldEngine.AddQuestNpc(npc)` → `QuestNpcList.Add` | `LocalDB.cs:708` / `WorldServer.cs:1924-1927` |
//! | `NpCinitialize`：`for (i = Count-1; i >= 0; i--)`；`FindMap(MapName)` 门；`Initialize()`（`AddToMap`）；`AddtoMapSuccess && !IsHide ⇒ RemoveAt` | `WorldServer.Merchant.cs:37-62` |
//! | 固定 NPC 的 `CellType = CellType.Merchant`（`NormNpc.cs:50` / `Merchant.cs:109`） | `NormNpc.cs:50` |
//!
//! ## 建模差异（登记，**不在本片修**）
//!
//! - **NPC 行为/RNG 由 `ProcessNpcs` 驱动，本仓未建模**：oracle `ProcessNpcs`
//!   （`WorldServer.Merchant.cs:118-160`）从 `NpcPosition` 正序遍历 `QuestNpcList` 调
//!   `NormNpc.Run()`（`GuildOfficial.Run` 抽 `Random(40)`/`RandomByte(8)`/`Random(30)`，
//!   `GuildOfficial.cs:29-40`）。本片只落「实体存在 + 挂图」，**不**跑 NPC 的 `Run()`——
//!   由 [`crate::monai::monster_ai_tick`] 与 [`crate::World::merchant_tick`] 双双跳过
//!   （`npc.is_some()` 排除），避免与 oracle 不同的取数路径污染 RNG 流。
//! - **构造抽数（本片已照搬）**：`new Merchant()/…` 在 `LoadNpcs` 时按文件序逐条经
//!   `BaseObject` ctor 抽 `Random(1500)`（`BaseObject.cs:439`）+ `Random(2000)`（`:441`），
//!   每次 2 抽；本片 `insert_npc` 用**同一条** `world.rng` 按同序抽（业主抽数 = 构造期，
//!   与 oracle 构造点一致）。绝对值只存随机分量（`run_tick_delta` / `base_search_time`），
//!   绝对时钟由消费方合成（照 [`crate::entity::MonsterCtorDraws`] 的口径）。
//! - **挂图 `obj_list` 追加序**：`NpCinitialize` 倒序 ⇒ 本片宿主按倒序 [`World::attach_npc_to_map`]
//!   （与 [`crate::merchant`] 同款）；实体槽位序 = 文件正序 [`World::insert_npc`]。

use crate::entity::{Entity, ACTOR_RACE_NPC};
use crate::map::CellType;
use crate::World;

/// `BaseObject.RunTick = GetTickCount() + Random(1500)`（`BaseObject.cs:439`）。
pub const NPC_RUN_DELAY_MAX: i32 = 1500;
/// `BaseObject.SearchTime = Random(2000) + 2000`（`BaseObject.cs:441`）。
pub const NPC_SEARCH_DRAW_MAX: i32 = 2000;

/// 固定管理 NPC 的状态（`INormNpc`/`BaseObject` 在加载层可对拍的字段子集 + ctor 抽数）。
///
/// `npc_type` 是 `LoadNpcs` 的 `switch` 分支（0 商人 / 1 行会管理 / 2 沙城管理）；
/// 后两个字段是 `BaseObject` ctor 抽出的**随机分量**（不是绝对值）。
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
    /// **建实体（构造期）**：入 `EntityStore`（**不挂图**），并**照 oracle ctor 抽 2 次数**。
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
        // `BaseObject.cs:439` `RunTick = GetTickCount() + Random(1500)`；
        // `:441` `SearchTime = Random(2000) + 2000`。**次序固定：先 RunTick 后 SearchTime**
        // （与 oracle ctor 逐行一致）。仅存随机分量，绝对时钟由消费方合成。
        state.run_tick_delta = self.rng.random_below(NPC_RUN_DELAY_MAX);
        state.base_search_time = self.rng.random_below(NPC_SEARCH_DRAW_MAX) + NPC_SEARCH_DRAW_MAX;

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
}
