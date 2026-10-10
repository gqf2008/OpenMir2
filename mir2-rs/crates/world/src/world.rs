//! 世界 tick 循环 —— 对齐 C# `GameSvr/Word/WorldServer.cs::ProcessHumans` 的 **200ms** 节拍
//!（`WorldServer.cs:487`：`if ((HUtil32.GetTickCount() - ProcessLoadPlayTick) > 200)`）。
//!
//! 本骨架的边界（与设计文档 §4.2 一致）：
//! - **单线程串行**：`World` 只在创建它的线程上推进；`tick()` 每次校验线程 id（判据③）；
//! - **确定性**：时钟是**虚拟时钟**（`clock_ms`，每 tick +200），模拟逻辑不读墙钟；
//!   实体处理顺序 = 槽位顺序（= 插入顺序），命令按 FIFO 处理；随机源是**同种子**的
//!   `RandomNumber`（与 C# 侧按同一调用顺序消费）；
//! - 不实现 M1 的边界服务（登录/网关/DBSrv），只做世界内的 tick/实体/地图/AOI/会话/战斗。

use std::collections::VecDeque;
use std::thread::ThreadId;

use mir2_data::models::{MonsterDropItem, StdItem, STRING_GOLD_NAME};
use mir2_formula::drop::{ItemNumberCounter, VecItemCatalog};
use mir2_shared::rng::RandomNumber;

use crate::aoi::{search_view_range, search_view_range_death};
use crate::combat::{
    attack, award_kill_exp, drop_position, generate_drops, AttackOutcome, CombatConfig,
};
use crate::entity::{Entity, EntityStore};
use crate::map::{CellType, MapGrid};
use crate::session::SessionRegistry;

/// C# `ProcessHumans` 的节拍（毫秒）。
pub const TICK_INTERVAL_MS: i64 = 200;

/// 世界 tick 里排队执行的命令。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// 进入世界（会话建立后）
    Enter { actor_id: i32 },
    /// 离开世界（掉线/小退）
    Leave { actor_id: i32 },
    /// 走一步（服务端做格子校验后更新坐标）
    Walk { actor_id: i32, dx: i32, dy: i32 },
    /// 攻击（`_Attack` 一次结算）——本最小切片未在 golden 里使用（战斗留下轮）
    Attack { actor_id: i32, target_id: i32 },
    /// 脚本化击杀：直接把目标 HP 置 0（最小切片的击杀触发；战斗结算不在本切片）
    Kill { target_id: i32 },
    /// 生成怪物（掉落在地图生成时按 C# 时机预算）
    SpawnMonster {
        id: i32,
        name: String,
        x: i32,
        y: i32,
        map_id: usize,
        level: u8,
        exp: i32,
        hp: u16,
        dc: u16,
        ac: u16,
        undead: u8,
        drop_list: Vec<MonsterDropItem>,
    },
}

/// 一个 tick 的处理记录（判据①：断言"谁、按什么顺序"被处理）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickRecord {
    pub tick_seq: u64,
    pub clock_ms: i64,
    /// 按处理顺序记录的 `(actor_id, 结果)`；`result` = 移动是否成功 / 进出是否成功 / 攻击是否命中
    pub processed: Vec<(i32, bool)>,
    /// 本 tick 执行的 AOI 刷新顺序（按地图、再按槽位顺序）
    pub aoi_order: Vec<i32>,
    /// 本 tick 的攻击结算（对拍用）
    pub attacks: Vec<(i32, i32, AttackOutcome)>,
    /// 本 tick 的死亡结算：`(死者 id, 掉落物品数, 金币, 击杀者, 获得经验)`
    pub deaths: Vec<(i32, usize, i32, Option<i32>, i32)>,
}

pub struct World {
    /// 地图表（下标 = `map_id`）；`World::new` 建一张 0 号图。
    pub maps: Vec<MapGrid>,
    pub entities: EntityStore,
    pub sessions: SessionRegistry,
    /// 虚拟时钟（毫秒），每 tick 前进 `TICK_INTERVAL_MS`
    pub clock_ms: i64,
    pub tick_seq: u64,
    /// 随机源（同种子 = 与 C# 可对拍）
    pub rng: RandomNumber,
    /// 物品目录（掉落生成按名查 `StdItem`）
    pub item_catalog: Vec<StdItem>,
    /// 怪物极品掉落几率（`Config.MonRandomAddValue`）
    pub mon_random_add_value: i32,
    /// 经验表（`Exps.conf` 的 `NeedExps`）
    pub need_exps: Vec<i32>,
    pub combat_config: CombatConfig,
    /// C# `Config.ClearDropOnFloorItemTime`
    pub clear_drop_on_floor_item_time: i64,
    /// 帧内待处理命令（FIFO）
    pending: VecDeque<Command>,
    /// 判据③：tick 必须始终跑在同一个线程上
    owner_thread: Option<ThreadId>,
    /// 掉落物品的 `MakeIndex` 计数器（`M2Share.GetItemNumber`）
    item_number: ItemNumberCounter,
    /// 最近一次 tick 的记录
    pub last_tick: TickRecord,
    /// AOI 分组的复用缓冲（避免每 tick 分配；P99 回归即由那次分配量出）
    by_map_scratch: Vec<Vec<i32>>,
}

impl World {
    /// 单地图世界（与 M2 上半程的 `World::new` 语义一致）。
    pub fn new(width: i32, height: i32) -> Self {
        Self::with_maps(vec![(width, height)])
    }

    /// 多地图世界（切图用）；`map_id` = 下标。
    pub fn with_maps(sizes: Vec<(i32, i32)>) -> Self {
        let maps = sizes.into_iter().map(|(w, h)| MapGrid::new(w, h)).collect();
        World {
            maps,
            entities: EntityStore::new(),
            sessions: SessionRegistry::new(),
            clock_ms: 0,
            tick_seq: 0,
            rng: RandomNumber::with_seed(42),
            item_catalog: Vec::new(),
            mon_random_add_value: 40,
            need_exps: vec![0; 1000],
            combat_config: CombatConfig::default(),
            clear_drop_on_floor_item_time: 5 * 60 * 1000,
            pending: VecDeque::new(),
            owner_thread: None,
            item_number: ItemNumberCounter::default(),
            last_tick: TickRecord::default(),
            by_map_scratch: Vec::new(),
        }
    }

    pub fn map(&self, map_id: usize) -> &MapGrid {
        &self.maps[map_id]
    }

    /// 入队一条命令（会话线程 → 世界线程的交接点；本骨架里同线程、仍走队列以保证顺序）。
    pub fn enqueue(&mut self, cmd: Command) {
        self.pending.push_back(cmd);
    }

    /// 会话进入世界并落图。
    pub fn add_entity(&mut self, mut entity: Entity) -> i32 {
        if entity.id <= 0 {
            entity.id = self.entities.alloc_id();
        }
        let id = self.entities.insert(entity);
        self.enqueue(Command::Enter { actor_id: id });
        id
    }

    /// 走一步（入队，tick 时按序生效）。
    pub fn walk(&mut self, actor_id: i32, dx: i32, dy: i32) {
        self.enqueue(Command::Walk { actor_id, dx, dy });
    }

    /// 攻击（入队）。
    pub fn attack_cmd(&mut self, actor_id: i32, target_id: i32) {
        self.enqueue(Command::Attack {
            actor_id,
            target_id,
        });
    }

    /// 脚本化击杀（入队）。
    pub fn kill(&mut self, target_id: i32) {
        self.enqueue(Command::Kill { target_id });
    }

    /// 离开世界（入队）。
    pub fn leave(&mut self, actor_id: i32) {
        self.enqueue(Command::Leave { actor_id });
    }

    /// 推进一个 tick（固定 200ms；不做 sleep —— 调度归调用方）。
    pub fn tick(&mut self) -> TickRecord {
        let current = std::thread::current().id();
        match self.owner_thread {
            None => self.owner_thread = Some(current),
            Some(t) => assert_eq!(
                t, current,
                "世界 tick 必须始终在同一线程上串行执行（设计文档 §4.2：禁止并行）"
            ),
        }

        self.clock_ms += TICK_INTERVAL_MS;
        self.tick_seq += 1;
        let mut record = TickRecord {
            tick_seq: self.tick_seq,
            clock_ms: self.clock_ms,
            processed: Vec::new(),
            aoi_order: Vec::new(),
            attacks: Vec::new(),
            deaths: Vec::new(),
        };

        // 命令 FIFO
        while let Some(cmd) = self.pending.pop_front() {
            match cmd {
                Command::Enter { actor_id } => {
                    let ok = self.enter_world(actor_id);
                    record.processed.push((actor_id, ok));
                }
                Command::Leave { actor_id } => {
                    let ok = self.leave_world(actor_id);
                    record.processed.push((actor_id, ok));
                }
                Command::Walk { actor_id, dx, dy } => {
                    let ok = self.try_step(actor_id, dx, dy);
                    record.processed.push((actor_id, ok));
                }
                Command::Kill { target_id } => {
                    let ok = match self.entities.get_mut(target_id) {
                        Some(e) => {
                            e.hp = 0;
                            true
                        }
                        None => false,
                    };
                    record.processed.push((target_id, ok));
                }
                Command::Attack {
                    actor_id,
                    target_id,
                } => {
                    let outcome = self.attack_once(actor_id, target_id);
                    match outcome {
                        Some(o) => {
                            record.processed.push((actor_id, o.hit));
                            record.attacks.push((actor_id, target_id, o));
                        }
                        None => record.processed.push((actor_id, false)),
                    }
                }
                Command::SpawnMonster {
                    id,
                    name,
                    x,
                    y,
                    map_id,
                    level,
                    exp,
                    hp,
                    dc,
                    ac,
                    undead,
                    drop_list,
                } => {
                    let ok = self.spawn_monster_now(
                        id, &name, x, y, map_id, level, exp, hp, dc, ac, undead, &drop_list,
                    );
                    record.processed.push((id, ok));
                }
            }
        }

        // 死亡结算（C# 在实体下一次 `Run()` 里发现 `HP == 0` 才 `Die()`；
        // 这里同一 tick 内先跑命令、再统一结算死亡，语义等价且顺序确定）
        self.settle_deaths(&mut record);

        // AOI 刷新：按地图、地图内按槽位顺序
        let ids: Vec<(i32, usize)> = self.entities.iter().map(|e| (e.id, e.map_id)).collect();
        // 复用缓冲（每 tick 不重新分配 —— 之前每次 vec![Vec::new(); maps] 让 P99 涨了约 15~35%）
        self.by_map_scratch.clear();
        self.by_map_scratch.resize_with(self.maps.len(), Vec::new);
        for v in self.by_map_scratch.iter_mut() {
            v.clear();
        }
        for (id, map_id) in ids {
            if let Some(v) = self.by_map_scratch.get_mut(map_id) {
                v.push(id);
            }
        }
        let by_map = std::mem::take(&mut self.by_map_scratch);
        for (map_id, ids) in by_map.iter().enumerate() {
            for id in ids {
                let death = match self.entities.get(*id) {
                    Some(e) => e.death,
                    None => continue,
                };
                if death {
                    let (map, entities) = (&mut self.maps, &mut self.entities);
                    search_view_range_death(
                        &mut map[map_id],
                        entities,
                        *id,
                        self.clock_ms,
                        self.clear_drop_on_floor_item_time,
                    );
                } else {
                    let (map, entities, clock) =
                        (&mut self.maps, &mut self.entities, self.clock_ms);
                    search_view_range(&mut map[map_id], entities, *id, clock);
                }
                record.aoi_order.push(*id);
            }
        }
        self.by_map_scratch = by_map;

        self.last_tick = record.clone();
        record
    }

    /// 进图：实体落进 `map_id` 的格子（对应 `Envirnoment.AddMapObject`）。
    pub fn enter_world(&mut self, actor_id: i32) -> bool {
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (x, y, map_id) = (e.x, e.y, e.map_id);
        let Some(map) = self.maps.get_mut(map_id) else {
            return false;
        };
        let now = self.clock_ms;
        if !map.add_map_object(x, y, CellType::Play, actor_id, now) {
            return false;
        }
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.add_to_mapped = true;
        }
        true
    }

    /// 离开世界：从格子摘除 + 从仓库移除。
    pub fn leave_world(&mut self, actor_id: i32) -> bool {
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (x, y, map_id) = (e.x, e.y, e.map_id);
        let Some(map) = self.maps.get_mut(map_id) else {
            return false;
        };
        let (cell, success) = map.get_cell_info_mut(x, y);
        if success {
            let before = cell.count();
            cell.obj_list
                .retain(|o| !(o.actor_object && o.cell_obj_id == actor_id));
            if cell.count() == 0 {
                cell.clear();
            }
            if cell.count() == before {
                return false;
            }
        }
        self.entities.remove(actor_id);
        true
    }

    /// 生成怪物：入队后由 tick 处理（掉落按 C# 时机在**生成时**预算）。
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_monster(&mut self, cmd: Command) -> bool {
        let Command::SpawnMonster { .. } = &cmd else {
            return false;
        };
        self.enqueue(cmd);
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_monster_now(
        &mut self,
        id: i32,
        name: &str,
        x: i32,
        y: i32,
        map_id: usize,
        level: u8,
        exp: i32,
        hp: u16,
        dc: u16,
        ac: u16,
        undead: u8,
        drop_list: &[MonsterDropItem],
    ) -> bool {
        if map_id >= self.maps.len() {
            return false;
        }
        // 掉落预算（`WorldServer.MonGen.cs:440` 的时机）：RNG 在这里消耗
        let catalog_items = std::mem::take(&mut self.item_catalog);
        let (gold, items) = {
            let catalog = VecItemCatalog {
                items: &catalog_items,
            };
            generate_drops(
                &mut self.rng,
                drop_list,
                &catalog,
                self.mon_random_add_value,
                STRING_GOLD_NAME,
                &mut self.item_number,
            )
        };
        self.item_catalog = catalog_items;

        let mut e = crate::combat::monster_from_row(crate::combat::MonsterRow {
            id,
            name,
            x,
            y,
            level,
            exp,
            hp,
            dc,
            ac,
            undead,
        });
        e.id = id;
        e.map_id = map_id;
        e.gold = gold;
        e.drop_items = items;
        let id = self.entities.insert(e);
        self.enter_world(id)
    }

    /// 死亡结算：把预算好的掉落散到地上 + 给击杀者发经验。
    fn settle_deaths(&mut self, record: &mut TickRecord) {
        let dead: Vec<i32> = self
            .entities
            .iter()
            .filter(|e| e.hp == 0 && !e.death_settled)
            .map(|e| e.id)
            .collect();
        for id in dead {
            let Some(e) = self.entities.get(id) else {
                continue;
            };
            let (x, y, map_id, gold, items, mon_exp, level, killer) = (
                e.x,
                e.y,
                e.map_id,
                e.gold,
                e.drop_items.clone(),
                e.mon_exp,
                e.level,
                e.last_hiter,
            );
            // 死亡标记（C# `Die()` 的 `Death = true`）
            if let Some(e) = self.entities.get_mut(id) {
                e.death = true;
                e.death_settled = true;
            }
            // 掉落落地：逐件找空格子（`GetDropPosition` 的扫描顺序）
            let mut placed = 0usize;
            for item in &items {
                let _shape = self
                    .item_catalog
                    .get(usize::from(item.index.saturating_sub(1)))
                    .map(|s| s.looks)
                    .unwrap_or(0);
                if self.place_floor_item(map_id, x, y, item.index as i32) {
                    placed += 1;
                }
            }
            // 金币落地（`DropGoldDown`：`GetDropPosition(CurrX, CurrY, 3, ...)`）
            if gold > 0 {
                let _shape = crate::combat::get_gold_shape(gold);
                if self.place_floor_item(map_id, x, y, id) {
                    placed += 1;
                }
            }
            // 经验：击杀者是玩家才发（C# `KillTargetTrigger` 由玩家侧调用）
            let mut gained = 0;
            if let Some(killer_id) = killer {
                let is_player = self
                    .entities
                    .get(killer_id)
                    .is_some_and(|k| k.race == crate::entity::ACTOR_RACE_PLAY && !k.death);
                if is_player {
                    let need_exps = std::mem::take(&mut self.need_exps);
                    let cfg = self.combat_config;
                    if let Some(k) = self.entities.get_mut(killer_id) {
                        let (g, _, _) =
                            award_kill_exp(k, i32::from(level), mon_exp, &need_exps, &cfg);
                        gained = g;
                    }
                    self.need_exps = need_exps;
                }
            }
            record.deaths.push((id, placed, gold, killer, gained));
        }
    }

    /// 把一个地面物品放到 (ox, oy) 附近第一个空格子。
    fn place_floor_item(&mut self, map_id: usize, ox: i32, oy: i32, obj_id: i32) -> bool {
        let Some(map) = self.maps.get_mut(map_id) else {
            return false;
        };
        let pos = drop_position(
            |x, y| {
                let (cell, ok) = map.get_cell_info(x, y);
                ok && cell.count() == 0
            },
            ox,
            oy,
            3,
        );
        let Some((px, py)) = pos else {
            return false;
        };
        map.add_map_object(px, py, CellType::Item, obj_id, self.clock_ms)
    }

    /// 一次攻击结算（`_Attack`）。
    fn attack_once(&mut self, actor_id: i32, target_id: i32) -> Option<AttackOutcome> {
        if self.entities.get(actor_id).is_none() || self.entities.get(target_id).is_none() {
            return None;
        }
        // 取出双方属性做结算（避免同时可变借用），再写回
        let mut attacker = self.entities.get(actor_id)?.clone();
        let mut target = self.entities.get(target_id)?.clone();
        let cfg = self.combat_config;
        let outcome = attack(&mut self.rng, &mut attacker, &mut target, &cfg);
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.last_hiter = attacker.last_hiter;
        }
        if let Some(e) = self.entities.get_mut(target_id) {
            e.hp = target.hp;
            e.last_hiter = target.last_hiter;
        }
        Some(outcome)
    }

    /// 单步移动：格子校验 → 更新坐标。
    fn try_step(&mut self, actor_id: i32, dx: i32, dy: i32) -> bool {
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (fx, fy, map_id) = (e.x, e.y, e.map_id);
        let (tx, ty) = (fx + dx, fy + dy);
        let Some(map) = self.maps.get_mut(map_id) else {
            return false;
        };
        if !map.cell_match(tx, ty) {
            return false;
        }
        let blockers: Vec<i32> = {
            let (cell, _) = map.get_cell_info(tx, ty);
            cell.obj_list
                .iter()
                .filter(|o| o.actor_object)
                .map(|o| o.cell_obj_id)
                .collect()
        };
        let blocking: Vec<i32> = blockers
            .into_iter()
            .filter(|id| {
                self.entities
                    .get(*id)
                    .is_some_and(|o| !o.ghost && !o.death && !o.fixed_hide_mode && !o.ob_mode)
            })
            .collect();
        let now = self.clock_ms;
        let moved = map.move_object(
            crate::map::MoveRequest {
                from_x: fx,
                from_y: fy,
                actor_id,
                to_x: tx,
                to_y: ty,
                ignore_block: false,
                add_time: now,
            },
            |cid| blocking.contains(&cid),
        );
        if moved {
            if let Some(e) = self.entities.get_mut(actor_id) {
                e.x = tx;
                e.y = ty;
            }
        }
        moved
    }

    /// 对拍用地面物品清单：`(map_id, x, y, obj_id)`，按 `(map, x, y, obj_id)` 升序。
    ///
    /// 对应 C# 侧驱动从各 `EnvirnomentStub.CellArray` 扫出来的 `CellType.Item` 项。
    pub fn floor_items_sorted(&self) -> Vec<(usize, i32, i32, i32)> {
        let mut out = Vec::new();
        for (map_id, map) in self.maps.iter().enumerate() {
            for x in 0..map.width {
                for y in 0..map.height {
                    let (cell, ok) = map.get_cell_info(x, y);
                    if !ok {
                        continue;
                    }
                    for o in &cell.obj_list {
                        if o.cell_type == CellType::Item {
                            out.push((map_id, x, y, o.cell_obj_id));
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// 对拍用快照：全部实体的 `(id, map, x, y, hp, level, exp, 视野列表)`，**按 id 升序**。
    pub fn snapshot_sorted(&self) -> Vec<EntitySnapshot> {
        let mut ids = self.entities.ids_sorted();
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(|id| {
                self.entities.get(id).map(|e| {
                    (
                        e.id,
                        e.map_id,
                        e.x,
                        e.y,
                        e.hp,
                        e.level,
                        e.exp,
                        crate::aoi::visible_snapshot(&self.entities, id),
                    )
                })
            })
            .collect()
    }
}

/// 对拍/输出用的实体快照：`(id, map_id, x, y, hp, level, exp, 视野列表[(目标 id, 标志)])`。
pub type EntitySnapshot = (i32, usize, i32, i32, u16, u8, i32, Vec<(i32, u8)>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, ACTOR_RACE_PLAY};

    fn world_with(entities: &[(i32, i32, i32)]) -> World {
        let mut w = World::new(64, 64);
        for (id, x, y) in entities {
            let mut e = Entity::new(*id, format!("e{id}"), *x, *y);
            e.id = *id;
            w.add_entity(e);
        }
        w
    }

    #[test]
    fn tick_advances_clock_by_200ms() {
        let mut w = World::new(16, 16);
        assert_eq!(w.clock_ms, 0);
        let r1 = w.tick();
        assert_eq!(r1.clock_ms, 200);
        assert_eq!(r1.tick_seq, 1);
        let r2 = w.tick();
        assert_eq!(r2.clock_ms, 400);
    }

    #[test]
    fn tick_order_is_deterministic_and_documented() {
        let mut w = world_with(&[(1, 10, 10), (2, 12, 10), (3, 20, 20)]);
        w.walk(2, 1, 0);
        w.walk(1, 0, 1);
        let r = w.tick();
        assert_eq!(
            r.processed,
            vec![(1, true), (2, true), (3, true), (2, true), (1, true)],
            "命令必须按 FIFO 处理"
        );
        assert_eq!(r.aoi_order, vec![1, 2, 3]);

        let mut w2 = world_with(&[(1, 10, 10), (2, 12, 10), (3, 20, 20)]);
        w2.walk(2, 1, 0);
        w2.walk(1, 0, 1);
        let r2 = w2.tick();
        assert_eq!(r.processed, r2.processed);
        assert_eq!(r.aoi_order, r2.aoi_order);
    }

    #[test]
    fn aoi_boundary_at_view_range() {
        let mut w = world_with(&[(1, 30, 30), (2, 35, 30), (3, 36, 30)]);
        w.tick();
        let vis = crate::aoi::visible_snapshot(&w.entities, 1);
        let targets: Vec<i32> = vis.iter().map(|(t, _)| *t).collect();
        assert!(targets.contains(&2), "dx=ViewRange 必须可见（闭区间）");
        assert!(!targets.contains(&3), "dx=ViewRange+1 不可见");
    }

    #[test]
    fn aoi_scan_order_is_x_outer_y_inner() {
        let mut w = world_with(&[(1, 30, 30), (10, 33, 32), (11, 34, 31)]);
        w.tick();
        let vis = crate::aoi::visible_snapshot(&w.entities, 1);
        let order: Vec<i32> = vis.iter().map(|(t, _)| *t).collect();
        assert_eq!(
            order,
            vec![1, 10, 11],
            "扫描顺序：自己先入列（无自排除），随后 X 升序外层、Y 升序内层"
        );
    }

    #[test]
    fn tick_refuses_to_run_on_another_thread() {
        let mut w = World::new(8, 8);
        w.tick();
        let handle = std::thread::spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.tick()));
            r.is_err()
        });
        assert!(
            handle.join().unwrap(),
            "tick 在别的线程上必须 panic（世界只能单线程串行）"
        );
    }

    #[test]
    fn session_enter_leave_world() {
        let mut w = World::new(16, 16);
        let id = w.add_entity(Entity::new(0, "p", 5, 5));
        w.tick();
        assert!(w.entities.get(id).unwrap().add_to_mapped);
        assert_eq!(w.map(0).cell(5, 5).count(), 1);
        w.leave(id);
        w.tick();
        assert!(w.entities.get(id).is_none(), "离开世界后实体应从仓库移除");
        assert_eq!(w.map(0).cell(5, 5).count(), 0);
    }

    fn player(id: i32, name: &str, x: i32, y: i32, level: u8) -> Entity {
        let mut e = Entity::new(id, name, x, y);
        e.race = ACTOR_RACE_PLAY;
        e.level = level;
        e.hp = 200;
        e.max_hp = 200;
        e.dc = 0x0A_14; // LoByte=20, HiByte=10 → 攻击力 20+(0..0)（差为负 → 归零）
        e.ac = 0x00_00;
        e.hit_point = 10;
        e.speed_point = 0;
        e.max_exp = 1000;
        e
    }

    /// 战斗子集：固定种子下攻击结算可复现（判据①「可证伪」）。
    #[test]
    fn attack_outcome_is_deterministic() {
        let build = || {
            let mut w = World::new(32, 32);
            let p = w.add_entity(player(1, "hero", 10, 10, 10));
            w.spawn_monster(Command::SpawnMonster {
                id: 2,
                name: "鸡".into(),
                x: 11,
                y: 10,
                map_id: 0,
                level: 1,
                exp: 10,
                hp: 5,
                dc: 1,
                ac: 0,
                undead: 0,
                drop_list: vec![],
            });
            w.tick();
            w.attack_cmd(p, 2);
            let rec = w.tick();
            rec.attacks[0].2
        };
        let a = build();
        let b = build();
        assert_eq!(a, b, "同种子同输入的攻击结算必须一致");
        assert!(
            a.power >= 20,
            "攻击力下限 = LoByte(DC) = 20，实际 {}",
            a.power
        );
        assert!(a.damage > 0, "护甲 0 时伤害应大于 0");
    }

    /// 死亡 → 掉落落地 + 击杀者得经验；再 tick 不重复结算。
    #[test]
    fn monster_death_drops_items_and_awards_exp() {
        let mut w = World::new(32, 32);
        // 经验表：1 级需 1000，2 级需 2000（自造，只为验证升级路径）
        w.need_exps[1] = 1000;
        w.need_exps[2] = 2000;
        w.mon_random_add_value = 1; // 必出极品（消耗 RNG 的分支也要覆盖）
        let p = w.add_entity(player(1, "hero", 10, 10, 10));
        w.spawn_monster(Command::SpawnMonster {
            id: 2,
            name: "鸡".into(),
            x: 11,
            y: 10,
            map_id: 0,
            level: 1,
            exp: 500,
            hp: 3,
            dc: 1,
            ac: 0,
            undead: 0,
            drop_list: vec![mir2_data::models::MonsterDropItem {
                sel_point: 9,
                max_point: 10,
                item_name: "金币".into(),
                count: 100,
            }],
        });
        w.tick(); // 生成（掉落预算在这里消耗 RNG）
        let exp_before = w.entities.get(p).unwrap().exp;
        w.attack_cmd(p, 2);
        let rec = w.tick(); // 攻击 → HP 归零；同 tick 内死亡结算
        assert_eq!(rec.deaths.len(), 1, "应有 1 次死亡结算: {rec:?}");
        let (dead_id, placed, gold, killer, gained) = rec.deaths[0];
        assert_eq!(dead_id, 2);
        assert!(gold > 0, "金币掉落应大于 0");
        assert!(placed > 0, "掉落物应落到地上");
        assert_eq!(killer, Some(p));
        assert!(gained > 0, "击杀者应获得经验");
        assert_eq!(w.entities.get(p).unwrap().exp, exp_before + gained);
        assert!(w.entities.get(2).unwrap().death);
        // 再 tick 不重复结算
        let rec2 = w.tick();
        assert!(rec2.deaths.is_empty(), "死亡只能结算一次: {rec2:?}");
    }

    /// 攻击目标判定子集：不打自己/自己宝宝/已死者。
    #[test]
    fn attack_target_rules_subset() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(1, "hero", 10, 10, 10));
        let mut pet = player(3, "pet", 11, 10, 1);
        pet.race = 80;
        pet.master = Some(p);
        w.add_entity(pet);
        w.tick();
        w.attack_cmd(p, p); // 自己
        w.attack_cmd(p, 3); // 自己宝宝
        let rec = w.tick();
        let hits: Vec<bool> = rec.processed.iter().map(|(_, ok)| *ok).collect();
        assert!(
            rec.attacks.iter().all(|(_, _, o)| o.damage == 0),
            "打自己/自己宝宝不应造成伤害: {:?}",
            rec.attacks
        );
        assert!(hits.contains(&false));
    }

    /// 会话状态机：进图 → 切图 → 小退 → 再进（新的 actor_id），并且视野列表整体失效。
    #[test]
    fn session_state_machine_enter_switch_softclose_reenter() {
        let mut w = World::with_maps(vec![(32, 32), (32, 32)]);
        w.open_session("hero");
        let mut e = Entity::new(0, "hero", 5, 5);
        e.race = ACTOR_RACE_PLAY;
        let first = w.enter_map("hero", e, 0).unwrap();
        assert_eq!(w.session("hero").unwrap().actor_id, Some(first));
        assert_eq!(w.map(0).cell(5, 5).count(), 1);

        // 切图：旧图摘除、新图登记、视野清空
        w.switch_map("hero", 1, 7, 9).unwrap();
        assert_eq!(w.map(0).cell(5, 5).count(), 0, "旧图必须摘除");
        assert_eq!(w.map(1).cell(7, 9).count(), 1, "新图必须登记");
        assert_eq!(w.entities.get(first).unwrap().map_id, 1);
        assert!(w.entities.get(first).unwrap().visible_actors.is_empty());

        // 小退：离开世界但会话保留（可再进）
        w.soft_close("hero").unwrap();
        assert!(w.entities.get(first).is_none());
        assert_eq!(w.session("hero").unwrap().soft_close_count, 1);
        assert_eq!(w.map(1).cell(7, 9).count(), 0);

        // 再进：新的 actor_id（C# 侧重新建 PlayObject）
        let mut e2 = Entity::new(0, "hero", 7, 9);
        e2.race = ACTOR_RACE_PLAY;
        let second = w.enter_map("hero", e2, 1).unwrap();
        assert_ne!(first, second, "小退再进必须得到新的 ActorId");
        assert_eq!(w.map(1).cell(7, 9).count(), 1);
    }
}
