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
use mir2_world_bevy::{advance_clock_system, TickClock, TickOwnerThread, TickRunner};

use crate::aoi::{search_view_range, search_view_range_death};
use crate::combat::{
    attack_dir, award_kill_exp, drop_position, generate_drops, swing_power, AttackOutcome,
    AttackPowerMode, CombatConfig,
};
use crate::entity::{Entity, EntityStore};
use crate::map::{CellType, FloorItemKind, FloorItemMeta, MapGrid};
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
    /// 攻击（`_Attack` 一次结算 + 命中者登记）——`combat.json` 场景用它把 `_Attack` 的 RNG 面纳入 golden
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
        /// `HitPoint` / `SpeedPoint`（命中判定用；缺省照 `BaseObject` 构造 = 5 / 15）
        hit_point: u8,
        speed_point: u8,
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
    /// 本 tick 的死亡结算：`(死者 id, 掉落物品数, 金币, 经验归属者, 获得经验)`
    /// —— 归属者取 C# `ExpHitter`（不是 `LastHiter`），见 `Entity::exp_hitter`
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
    /// **M3②③ 活栈隔离口径（owner 2026-10-12 21:15）**：攻击路径**专用**随机流。
    ///
    /// 与 `rng` 同源但独立推进：种子 = [`mir2_shared::rng::attack_seed`]`(MIR2_RNG_SEED)`。
    /// 仅在 [`World::attack_isolation_enabled`] 时被攻击路径取数；关闭时（缺省）**一次都不会被取数**，
    /// 行为与改动前逐字节相同。
    pub rng_attack: RandomNumber,
    /// 是否把攻击路径取数路由到 [`World::rng_attack`]。缺省 `false`（生产/既有 golden 路径不变）。
    isolate_attack: bool,
    /// 是否在 [`World::tick`] 里驱动怪物 AI（[`crate::monai::monster_ai_tick`]）。
    ///
    /// **缺省 `false`**：M2 的 golden 场景（`tests/parity/world/scenarios/*.json`）是「直接驱动
    /// `_Attack`」的合成探针（`csharp/WorldGolden`），不含 oracle 的 `ProcessMonsters` 线程，
    /// 因此**不能**默认跑 AI（否则每次 `CreateMonster` 后的第一只怪就会多抽 `Wondering`，
    /// 把 golden 的 RNG 序列整体移位）。接线层（`bins/game-svr` 的 sim 循环）在起世界时显式
    /// [`World::enable_monster_ai`]；`openmir2-m3-monster-ai-tick` 的宿主接线为后续分片。
    ai_enabled: bool,
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
    /// S1 tick 骨架：时钟推进 + 线程断言（单线程 `Schedule`）。
    clock_runner: TickRunner,
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
        // S1：实体存储是 bevy_ecs 内核；时钟/线程断言/顺序收集都走它的单线程骨架。
        let mut entities = EntityStore::new();
        entities
            .world_mut()
            .insert_resource(TickClock::new(TICK_INTERVAL_MS));
        entities
            .world_mut()
            .insert_resource(TickOwnerThread::default());
        let mut clock_runner = TickRunner::new();
        clock_runner.add_systems(advance_clock_system);
        let mut world = World {
            maps,
            entities,
            sessions: SessionRegistry::new(),
            clock_ms: 0,
            tick_seq: 0,
            rng: RandomNumber::with_seed(42),
            rng_attack: RandomNumber::with_seed(42),
            isolate_attack: false,
            ai_enabled: false,
            item_catalog: Vec::new(),
            mon_random_add_value: 40,
            need_exps: vec![0; 1000],
            combat_config: CombatConfig::default(),
            clear_drop_on_floor_item_time: 5 * 60 * 1000,
            pending: VecDeque::new(),
            owner_thread: None,
            clock_runner,
            item_number: ItemNumberCounter::default(),
            last_tick: TickRecord::default(),
            by_map_scratch: Vec::new(),
        };
        // **M3②③ 隔离口径**：`MIR2_RNG_ISOLATE_ATTACK` 开启时按 `MIR2_RNG_SEED` 派生专用流
        // （缺省未设 ⇒ `None`，`isolate_attack` 保持 `false`，一字不改）。读 env 的 fail-closed
        // 语义见 [`mir2_shared::rng::attack_isolation_seed_from_env`]。
        if let Some(seed) = mir2_shared::rng::attack_isolation_seed_from_env() {
            world.enable_attack_isolation(seed);
        }
        world
    }

    /// 显式开启攻击路径专用流（种子由调用方按两侧同口径给出）。
    ///
    /// 幂等：重复调用只重播专用流。测试与真机接线（经 [`World::with_maps`] 的环境入口）共用本方法。
    pub fn enable_attack_isolation(&mut self, attack_seed: i32) {
        self.rng_attack = RandomNumber::with_seed(attack_seed);
        self.isolate_attack = true;
    }

    /// 攻击路径专用流是否开启（隔离口径的真机自证）。
    #[must_use]
    pub fn attack_isolation_enabled(&self) -> bool {
        self.isolate_attack
    }

    /// 开启怪物 AI 驱动（缺省关闭；见 `World` 的 `ai_enabled` 字段说明）。
    ///
    /// 幂等：重复调用无副作用。生产路径由 `bins/game-svr` 的 sim 循环在起世界时调用；
    /// 合成 golden 场景保持关闭 ⇒ 既有 `tests/parity/world` 逐字节不变。
    pub fn enable_monster_ai(&mut self) {
        self.ai_enabled = true;
    }

    /// 怪物 AI 驱动是否开启（自证/测试用）。
    #[must_use]
    pub fn monster_ai_enabled(&self) -> bool {
        self.ai_enabled
    }

    /// 攻击路径取数的统一入口：按隔离开关选流。
    ///
    /// **只把攻击路径的取数**（[`crate::combat::attack_dir`] / [`crate::combat::swing_power`] 及其
    /// 全部下取：`atk_power_a/b`、`hit_gate`、`hit_struck_damage`、`struck_durability`、
    /// `attack_success`）路由到专用流；其余消费者仍走全局 `rng`。
    fn attack_rng(&mut self) -> &mut RandomNumber {
        if self.isolate_attack {
            &mut self.rng_attack
        } else {
            &mut self.rng
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

    /// **无目标挥砍**（`PlayObject.AttackDir(targetObject = null)`，`PlayObject.Attack.cs:25`）。
    ///
    /// oracle 在**取目标之前**就调 `GetAttackPowerHit(wHitMode, GetBaseAttackPoewr(), …)` ⇒
    /// 即使前方格无目标（`GetPoseCreate()` 返回 null），**攻击力取数照抽**（玩家 `Luck <= 0` 时
    /// `Random(power+1)` + `Random(10)` 共 2 抽）；之后 `_Attack(null)` 立刻 `return false`、
    /// `AttackSuccess(…, null)` 也在开头 `return` ⇒ **不再有别的取数**。
    ///
    /// 返回本次挥砍抽出的攻击力（`GetBaseAttackPoewr` 的结果）；actor 不存在 ⇒ `None`。
    ///
    /// **接线（`bins/game-svr`，属 m2-svr 写集）**：`handle_hit_cmd` 在“前方格无目标”支目前
    /// 只记日志（`main.rs` 的「只播动作帧（oracle `_Attack(null)` 返回 false）」支）⇒ 必须改调本方法；
    /// 否则 C# 侧那 115/120「有攻击命令、无目标」的 `atk_power_a/b` 两抽在 Rust 侧**整段缺失**
    /// （本片 .rs 侧归因，见 `M3RNG攻击路径三差归因报告-20261012.md`）。本 crate 不改 `bins/**`。
    #[must_use]
    pub fn attack_swing(&mut self, actor_id: i32) -> Option<i32> {
        let (dc, mode) = {
            let attacker = self.entities.get(actor_id)?;
            (attacker.dc, AttackPowerMode::for_attacker(attacker))
        };
        let power = swing_power(self.attack_rng(), dc, mode);
        Some(power)
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

        // S1 tick 骨架：时钟推进（+ 线程断言）在单线程 Schedule 里跑。
        // facade 仍是事实源（`clock_ms`/`tick_seq` 是对外 pub 字段）：先把值写进资源，
        // 跑完读回；数值语义与原 `clock_ms += 200; tick_seq += 1` 逐 tick 相同。
        {
            let mut clock = self.entities.world_mut().resource_mut::<TickClock>();
            clock.clock_ms = self.clock_ms;
            clock.tick_seq = self.tick_seq;
        }
        self.clock_runner.run(self.entities.world_mut());
        {
            let clock = self.entities.world().resource::<TickClock>();
            self.clock_ms = clock.clock_ms;
            self.tick_seq = clock.tick_seq;
        }
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
                    hit_point,
                    speed_point,
                    drop_list,
                } => {
                    let ok = self.spawn_monster_now(
                        id,
                        &name,
                        x,
                        y,
                        map_id,
                        level,
                        exp,
                        hp,
                        dc,
                        ac,
                        undead,
                        hit_point,
                        speed_point,
                        &drop_list,
                    );
                    record.processed.push((id, ok));
                }
            }
        }

        // 命中者过期清理（照 `BaseObject.Base.cs:119-132` 的结构；墙钟那两项不在本骨架）
        self.expire_hitters();
        // 死亡结算（C# 在实体下一次 `Run()` 里发现 `HP == 0` 才 `Die()`；
        // 这里同一 tick 内先跑命令、再统一结算死亡，语义等价且顺序确定）
        self.settle_deaths(&mut record);

        // M3④-实现 C：怪物 AI 一步（oracle `ProcessMonsters` → 每只 `monster.Run()`）。
        // **缺省关闭**：合成 golden 场景不含 oracle 的怪物线程，开 AI 会把 RNG 序列整体移位；
        // 由 `bins/game-svr` 的 sim 循环显式 `enable_monster_ai()`（见字段说明）。
        // 单线程串行：按实体槽位顺序（`EntityStore` 显式索引），在死亡结算之后、AOI 之前。
        if self.ai_enabled {
            crate::monai::monster_ai_tick(self);
        }

        // AOI 刷新：按地图、地图内按槽位顺序（顺序来自 ECS 内核的**显式槽位索引**，
        // `EcsStore::iter` 不迭代 `Query` 求顺序）；AOI 的扫描/过滤语义仍归 S2。
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
        hit_point: u8,
        speed_point: u8,
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
            hit_point,
            speed_point,
        });
        e.id = id;
        e.map_id = map_id;
        e.gold = gold;
        e.drop_items = items;
        let id = self.entities.insert(e);
        self.enter_world(id)
    }

    /// **生成期的掉落预算**（`MonGetRandomItems` 的时机）——用**调用方给定的**随机流。
    ///
    /// oracle `WorldServer.MonGen.cs:441-488` 的 `MonGetRandomItems` 在 `CreateMonster`
    /// 内部、即 **Dir 抽之后、CoolEye 抽之前** 发生在同一 `M2Share.RandomNumber` 上。
    /// 接线层的 `MonsterCreator` 回调（[`crate::mongen::regen_monsters`]）需要在「两次位置抽
    /// 之间」把这半段插入正确位置；本方法让回调不必碰 `World` 私有字段（`item_catalog` /
    /// `item_number` / `mon_random_add_value`）就能在**同一条**流上预算。
    pub fn budget_monster_drops(
        &mut self,
        rng: &mut RandomNumber,
        drop_list: &[MonsterDropItem],
    ) -> (i32, Vec<crate::combat::DropItem>) {
        let catalog_items = std::mem::take(&mut self.item_catalog);
        let (gold, items) = {
            let catalog = VecItemCatalog {
                items: &catalog_items,
            };
            generate_drops(
                rng,
                drop_list,
                &catalog,
                self.mon_random_add_value,
                STRING_GOLD_NAME,
                &mut self.item_number,
            )
        };
        self.item_catalog = catalog_items;
        (gold, items)
    }

    /// 生成**已预算好掉落/朝向**的怪物：**不再取任何随机数**（掉落预算由调用方在
    /// [`World::budget_monster_drops`] 里按 oracle 次序完成；构造函数链的取数由调用方的
    /// `CreateMonster` 回调完成）。
    ///
    /// 与 [`World::spawn_monster_now`] 的唯一差别是「不自己抽掉落」——用于
    /// `RegenMonsters` 的 `CreateMonster` 回调（拖到 tick 里再抽会与 oracle 的 interleaving 不同）。
    /// `ctor` = 构造函数链已抽出的值（[`crate::entity::MonsterCtorDraws`]），随生成结果落实体；
    /// `dir`/`cool_eye` 属 AOI 实体面（承接卡 `openmir2-m3-monster-ai-tick`），本骨架的
    /// `Entity` 尚未建模，故**此处登记为不入参与返回值的差值**（接线层已按 oracle 取数）。
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_monster_precomputed(
        &mut self,
        name: &str,
        x: i32,
        y: i32,
        map_id: usize,
        race: u8,
        level: u8,
        exp: i32,
        hp: u16,
        dc: u16,
        ac: u16,
        undead: u8,
        hit_point: u8,
        speed_point: u8,
        gold: i32,
        drop_items: Vec<crate::combat::DropItem>,
        ctor: crate::entity::MonsterCtorDraws,
    ) -> Option<i32> {
        if map_id >= self.maps.len() {
            return None;
        }
        let id = self.entities.alloc_id();
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
            hit_point,
            speed_point,
        });
        e.race = race;
        e.id = id;
        e.map_id = map_id;
        e.gold = gold;
        e.drop_items = drop_items;
        e.ctor_draws = ctor;
        let id = self.entities.insert(e);
        if !self.enter_world(id) {
            self.entities.remove(id);
            return None;
        }
        Some(id)
    }

    /// 死亡结算：把预算好的掉落散到地上 + 给击杀者发经验。
    ///
    /// 命中者过期（`LastHiter`/`ExpHitter` 在命中者死亡或隐身后失效）**不在这里**判，
    /// 由 `expire_hitters()` 在每 tick 的命令之后、死亡结算之前统一清理——照 oracle 的结构
    /// （`BaseObject.Base.cs:119-132` 是在**命中者自己的** `Run()` 里清的），这样"归属者已死/已隐身"
    /// 与 oracle 同样表现为"根本没人认领"，而不是"认领了但不发"。
    fn expire_hitters(&mut self) {
        let gone: Vec<i32> = self
            .entities
            .iter()
            .filter(|e| e.death || e.ghost)
            .map(|e| e.id)
            .collect();
        if gone.is_empty() {
            return;
        }
        for e in self.entities.iter_mut() {
            if e.last_hiter.is_some_and(|h| gone.contains(&h)) {
                e.last_hiter = None;
            }
            if e.exp_hitter.is_some_and(|h| gone.contains(&h)) {
                e.exp_hitter = None;
            }
        }
    }

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
            let (x, y, map_id, gold, items, mon_exp, exp_hitter) = (
                e.x,
                e.y,
                e.map_id,
                e.gold,
                e.drop_items.clone(),
                e.mon_exp,
                e.exp_hitter,
            );
            // 死亡标记（C# `Die()` 的 `Death = true`）
            if let Some(e) = self.entities.get_mut(id) {
                e.death = true;
                e.death_settled = true;
            }
            // 掉落落地：逐件找空格子（`GetDropPosition` 的扫描顺序）。
            // 归属者（`mapItem.OfBaseObject`）= oracle `RM_DIEDROPITEM` 的 `wParam`：
            // `ExpHitter.Master?.ActorId ?? ExpHitter.ActorId`（`BaseObject.cs:2437/2442`）——
            // 由 world 在落地时记账，不留给接线层去派生（owner §25.1）。
            let drop_owner = exp_hitter
                .and_then(|h| self.entities.get(h).map(|k| k.master.unwrap_or(h)))
                .unwrap_or(0);
            let mut placed = 0usize;
            for item in &items {
                let meta = FloorItemMeta {
                    kind: FloorItemKind::Item,
                    index: item.index,
                    count: 1,
                    item_id: item.make_index,
                    of_base_object: drop_owner,
                };
                if self.place_floor_item(map_id, x, y, item.index as i32, meta) {
                    placed += 1;
                }
            }
            // 金币落地（`DropGoldDown`：`GetDropPosition(CurrX, CurrY, 3, ...)`）。`item_id` 取落主
            // `ActorId`（各金币堆落不同格 ⇒ 同格删除键仍唯一；金币无 `UserItem.MakeIndex`）。
            if gold > 0 {
                let meta = FloorItemMeta {
                    kind: FloorItemKind::Gold,
                    index: 0,
                    count: gold,
                    item_id: id,
                    of_base_object: drop_owner,
                };
                if self.place_floor_item(map_id, x, y, id, meta) {
                    placed += 1;
                }
            }
            // 经验：`KillFunc`（`BaseObject.cs:2429`）认的是 `ExpHitter`，不是 `LastHiter`
            // —— `ExpHitter` 是**第一个**命中者（`SetLastHiter` 里 `if (ExpHitter == null)`）。
            // "命中者死亡/隐身后不再认领"由**每 tick 的 `expire_hitters()`** 按 oracle 的结构清理
            // （`BaseObject.Base.cs:119-132` 在命中者自己的 `Run()` 里清），故这里只判"归属者是玩家本人"
            // —— 与 `KillFunc` 的形状一致。
            // 子集口径：`ExpHitter.Master != null`（宝宝击杀）不在本骨架；30s/6s 的**墙钟**过期也不做。
            let mut gained = 0;
            if let Some(hitter_id) = exp_hitter {
                let is_player = self
                    .entities
                    .get(hitter_id)
                    .is_some_and(|k| k.race == crate::entity::ACTOR_RACE_PLAY);
                if is_player {
                    let need_exps = std::mem::take(&mut self.need_exps);
                    let cfg = self.combat_config;
                    if let Some(k) = self.entities.get_mut(hitter_id) {
                        let (g, _, _) = award_kill_exp(k, mon_exp, &need_exps, &cfg);
                        gained = g;
                    }
                    self.need_exps = need_exps;
                }
            }
            record.deaths.push((id, placed, gold, exp_hitter, gained));
        }
    }

    /// 把一个地面物品放到 (ox, oy) 附近第一个空格子。
    fn place_floor_item(
        &mut self,
        map_id: usize,
        ox: i32,
        oy: i32,
        obj_id: i32,
        meta: FloorItemMeta,
    ) -> bool {
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
        map.add_floor_item(px, py, obj_id, meta, self.clock_ms)
    }

    /// 一次攻击结算（`_Attack`）。
    pub(crate) fn attack_once(&mut self, actor_id: i32, target_id: i32) -> Option<AttackOutcome> {
        if self.entities.get(actor_id).is_none() || self.entities.get(target_id).is_none() {
            return None;
        }
        // 取出双方属性做结算（避免同时可变借用），再写回
        let mut attacker = self.entities.get(actor_id)?.clone();
        let mut target = self.entities.get(target_id)?.clone();
        // `StruckDamage` 的倍率取目标**当下存着**的 `LastHiter`（= 上一次命中者：本次命中者要等
        // 目标下一轮 `Run()` 处理 `RM_STRUCK` 才登记）。默认倍率 10 时数值不可分辨，非默认配置下
        // 才有差异 ⇒ 必须照 oracle 取，别改成本次攻击者。
        let last_hitter = target
            .last_hiter
            .and_then(|id| self.entities.get(id))
            .map(|h| crate::combat::LastHitter {
                race: h.race,
                job: h.job,
                has_master: h.master.is_some(),
            });
        let cfg = self.combat_config;
        // 攻方攻击力口径：oracle 的 `GetBaseAttackPoewr` 走**虚函数** `GetAttackPower`
        //（`BaseObject.Attack.cs:25` → `BaseObject.Base.cs:886` 基版；`PlayObject.cs:2909` 玩家 override）。
        // 玩家（`Race == ActorRace.Play`）走 override（`Luck`/`PowerRate`/`BoPowerItem`）；
        // 怪物等无 override 的对象走基版。两版**取数次数不同**（`Luck <= 0` 时 override 多一次
        // `Random(10)`）⇒ 必须按对象分派，不能一律走 `Base`（否则跨侧序列从第一次命中起错位）。
        //
        // 玩家攻方还要走完整的 `PlayObject.AttackDir`：`_Attack` 之后 `AttackSuccess` 再抽一次
        // `Random(5)`（`PlayObject.Attack.cs:225`）—— 由 [`crate::combat::attack_dir`] 统一补上；
        // 怪物攻方无这条（`AttackSuccess` 是 `PlayObject` 私有方法）。
        let outcome = attack_dir(
            self.attack_rng(),
            &mut attacker,
            &mut target,
            last_hitter,
            &cfg,
        );
        if let Some(e) = self.entities.get_mut(target_id) {
            e.hp = target.hp;
            // 命中者登记的**折叠**：C# 在 `_Attack` 里对 `Race > ActorRace.Play` 的目标
            // `SendMsg(RM_STRUCK)`（`BaseObject.Attack.cs:61-64`），由目标在自己的 `Run()` 里
            // `SetLastHiter(攻击方)`（`AnimalObject.cs:118-124`）。这里在结算之后立刻登记：
            // 本次 `_Attack` 期间的 `StruckDamage` 仍看到旧的 `LastHiter`，与 C# 同序。
            if target.race > crate::entity::ACTOR_RACE_PLAY {
                e.set_last_hiter(actor_id);
            }
        }
        Some(outcome)
    }

    /// **同步移动**（`openmir2-m3-move-wiring` 的接线入口）：`steps` = 走 / 跑 / 骑跑的格数（1/2/3）。
    ///
    /// oracle 的 `ClientWalkXY`/`ClientRunXY`/`ClientHorseRunXY` 在**消息处理线程上立即**改坐标
    /// （`WalkTo`/`RunTo`/`HorseRunTo` → `Envir.MoveToMovingObject`），不等 tick；本方法对应
    /// `Command::Walk` 的**同一套** `try_step` 判据（阻挡/格子校验逐字节同构），只是不入队。
    ///
    /// `steps > 1` 时照 oracle 的写法**先逐格判可走、再一次性移动**（`RunTo` 的
    /// `CanWalkEx(cur+1) && CanWalkEx(cur+2) && MoveToMovingObject(cur, cur+2)`；
    /// `HorseRunTo` 为 3 格）：中途任一格不可走 ⇒ 一格都不动（不是走一段再失败）。
    #[must_use]
    pub fn try_step_now(&mut self, actor_id: i32, dx: i32, dy: i32, steps: i32) -> bool {
        if steps <= 0 || (dx == 0 && dy == 0) {
            return false;
        }
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (fx, fy, map_id) = (e.x, e.y, e.map_id);
        for k in 1..=steps {
            if !self.cell_free(map_id, fx + dx * k, fy + dy * k, actor_id) {
                return false;
            }
        }
        self.try_step(actor_id, dx * steps, dy * steps)
    }

    /// 目标格是否可站（与 `try_step` 的阻挡口径同源：`cell_match` + `valid()/is_available()` +
    /// 阻挡 actor 的 `!Ghost && !Death && !FixedHideMode && !ObMode`；`self_id` 自己不算阻挡）。
    fn cell_free(&self, map_id: usize, tx: i32, ty: i32, self_id: i32) -> bool {
        let Some(map) = self.maps.get(map_id) else {
            return false;
        };
        if !map.cell_match(tx, ty) {
            return false;
        }
        let (cell, ok) = map.get_cell_info(tx, ty);
        if !ok {
            return false;
        }
        if !(cell.valid() && cell.is_available()) {
            return true;
        }
        !cell.obj_list.iter().any(|o| {
            o.actor_object
                && o.cell_obj_id != self_id
                && self
                    .entities
                    .get(o.cell_obj_id)
                    .is_some_and(|e| !e.ghost && !e.death && !e.fixed_hide_mode && !e.ob_mode)
        })
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

    /// 地面物品的**最小身份视图**（world 侧只读分片，owner §25.1 裁决 (A)）：
    /// 每个 `CellType::Item` 格子对象一条，按 `(map, x, y, obj_id, item_id)` 升序。
    ///
    /// 字段全部来自 world 在**掉落落地时**的记账（`FloorItemMeta`），不派生、不改模拟语义；
    /// 接线层拿它 + item_catalog 装配 `ground::GroundItem`，不需要反推金币 `count` 或归属。
    /// `can_pick_up_tick` = 落地时间（`CellObject.add_time` 的虚拟时钟转 `i32`）。
    pub fn floor_items_view_sorted(&self) -> Vec<FloorItemView> {
        let mut out = Vec::new();
        for (map_id, map) in self.maps.iter().enumerate() {
            for x in 0..map.width {
                for y in 0..map.height {
                    let (cell, ok) = map.get_cell_info(x, y);
                    if !ok {
                        continue;
                    }
                    for o in &cell.obj_list {
                        if o.cell_type != CellType::Item {
                            continue;
                        }
                        let Some(meta) = o.item else {
                            continue;
                        };
                        out.push(FloorItemView {
                            map_id,
                            x,
                            y,
                            obj_id: o.cell_obj_id,
                            kind: meta.kind,
                            index: meta.index,
                            count: meta.count,
                            item_id: meta.item_id,
                            of_base_object: meta.of_base_object,
                            can_pick_up_tick: o.add_time as i32,
                        });
                    }
                }
            }
        }
        out.sort_unstable_by_key(|v| (v.map_id, v.x, v.y, v.obj_id, v.item_id));
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

/// 地面物品的最小身份视图（[`World::floor_items_view_sorted`] 的元素）。
///
/// `obj_id` = `CellObject.cell_obj_id`（**与 `floor_items_sorted` 同口径**，不变）；
/// `item_id` 是拾取/删除格子的**唯一键**（物品 = `UserItem.MakeIndex`，金币 = 落主 `ActorId`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FloorItemView {
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    pub obj_id: i32,
    pub kind: FloorItemKind,
    /// `StdItem` 序号（1-based）；金币 = 0。
    pub index: u16,
    /// 金币数 / 物品数（物品恒 1）。
    pub count: i32,
    /// 唯一 id。
    pub item_id: i32,
    /// 掉落归属者（0 = 任何人可拾）。
    pub of_base_object: i32,
    /// 可拾起时刻（落地时的虚拟时钟）。
    pub can_pick_up_tick: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, MonsterCtorDraws, ACTOR_RACE_PLAY};

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

    /// `openmir2-m3-move-wiring`：同步移动（`ClientWalkXY`/`RunTo`/`HorseRunTo` 口径）。
    /// - 1/2/3 格的 `steps` 语义；
    /// - `steps > 1` 时**任一中间格不可走 ⇒ 一格都不动**（oracle `RunTo` 先 `CanWalkEx(cur+1) &&
    ///   CanWalkEx(cur+2)` 再 `MoveToMovingObject`，不是走一段再失败）；
    /// - `(0,0)` / `steps <= 0` 拒绝。
    #[test]
    fn try_step_now_is_synchronous_and_atomic() {
        let mut w = world_with(&[(1, 5, 5)]);
        w.tick(); // Enter 落格
        assert!(w.try_step_now(1, 1, 0, 1));
        assert_eq!(
            (w.entities.get(1).unwrap().x, w.entities.get(1).unwrap().y),
            (6, 5),
            "走 1 格"
        );
        assert!(w.try_step_now(1, 1, 0, 2));
        assert_eq!(
            (w.entities.get(1).unwrap().x, w.entities.get(1).unwrap().y),
            (8, 5),
            "跑 2 格"
        );
        assert!(w.try_step_now(1, 0, 1, 3));
        assert_eq!(
            (w.entities.get(1).unwrap().x, w.entities.get(1).unwrap().y),
            (8, 8),
            "骑跑 3 格"
        );
        assert!(!w.try_step_now(1, 0, 0, 1), "(0,0) 拒绝");
        assert!(!w.try_step_now(1, 1, 0, 0), "steps<=0 拒绝");

        // 中间格被另一个 actor 占住 ⇒ 2 格跑一格都不动（整段原子）。
        let mut w2 = world_with(&[(1, 5, 5), (2, 6, 5)]);
        w2.tick();
        assert!(!w2.try_step_now(1, 1, 0, 2), "第 1 格被占 ⇒ 整段不动");
        assert_eq!(
            (w2.entities.get(1).unwrap().x, w2.entities.get(1).unwrap().y),
            (5, 5)
        );
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
                hit_point: 5,
                speed_point: 15,
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
            hit_point: 5,
            speed_point: 15,
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

    /// 只读分片（owner §25.1 裁决 (A)）：掉落落地后 `floor_items_view_sorted` 的
    /// `(kind, index, count, item_id, of_base_object)` 必须与 world 状态/oracle 语义一致。
    /// **红检**：改坏任一映射（kind/index/count/归属/item_id）⇒ 本用例必红。
    #[test]
    fn monster_death_floor_item_view_matches_world_state() {
        let mut w = World::new(32, 32);
        // 1-based 查表：下标 0 → `index = 1`（与 `combat.json` 的 `金创药(小量)` 同口径）
        w.item_catalog = vec![StdItem {
            name: "金创药(小量)".into(),
            dura_max: 1,
            ..Default::default()
        }];
        w.mon_random_add_value = 1; // 必出极品（消耗 RNG 的分支也覆盖）
        let p = w.add_entity(player(1, "hero", 10, 10, 10));
        w.spawn_monster(Command::SpawnMonster {
            id: 2,
            name: "僵尸".into(),
            x: 11,
            y: 10,
            map_id: 0,
            level: 1,
            exp: 500,
            hp: 3,
            dc: 1,
            ac: 0,
            undead: 0,
            hit_point: 5,
            speed_point: 15,
            // 金币 + 普通物品双掉落（与 `world/scenarios/combat.json` 的 `僵尸` 同形）
            drop_list: vec![
                MonsterDropItem {
                    sel_point: 9,
                    max_point: 10,
                    item_name: "金币".into(),
                    count: 300,
                },
                MonsterDropItem {
                    sel_point: 9,
                    max_point: 10,
                    item_name: "金创药(小量)".into(),
                    count: 1,
                },
            ],
        });
        w.tick(); // 生成（掉落预算在这里消耗 RNG）
        w.attack_cmd(p, 2);
        let rec = w.tick(); // 攻击 → 死亡结算 + 掉落落地
        assert_eq!(rec.deaths.len(), 1, "应有 1 次死亡结算: {rec:?}");
        let rec_gold = rec.deaths[0].2;
        assert!(rec_gold > 0, "本用例需金币掉落: {rec:?}");

        let view = w.floor_items_view_sorted();
        // 与既有对拍口径一致：`floor_items_sorted` 的 `(map,x,y,obj_id)` 必须与视图逐项相同
        let legacy: Vec<(usize, i32, i32, i32)> = view
            .iter()
            .map(|v| (v.map_id, v.x, v.y, v.obj_id))
            .collect();
        assert_eq!(
            legacy,
            w.floor_items_sorted(),
            "视图的 obj_id 口径必须与 floor_items_sorted 相同"
        );

        // 金币：index=0、count=预算金币、item_id=落主 ActorId、归属=击杀者（无 Master）
        let gold = view
            .iter()
            .find(|v| v.kind == FloorItemKind::Gold)
            .expect("应有金币地面物");
        assert_eq!(
            (gold.index, gold.count, gold.of_base_object),
            (0, rec_gold, p)
        );
        assert_eq!(gold.item_id, 2, "金币 item_id = 落主 ActorId");

        // 普通物品：index=StdItem 序号、count=1、item_id=UserItem.MakeIndex(>0)、归属=击杀者
        let item = view
            .iter()
            .find(|v| v.kind == FloorItemKind::Item)
            .expect("应有普通物品地面物");
        assert_eq!((item.index, item.count, item.of_base_object), (1, 1, p));
        assert!(item.item_id > 0, "物品 item_id = MakeIndex 应为正");
        assert_ne!(item.item_id, gold.item_id, "物品/金币的 item_id 不应相等");
        // 可拾起时刻 = 落地时的虚拟时钟（非 None/占位 0 的构造口径）
        assert!(view.iter().all(|v| v.can_pick_up_tick >= 0));
    }

    /// 攻击目标判定（`is_proper_target`）：F-K 修复后按 oracle `IsProperTarget` 忠实建模。
    ///
    /// 关键变化（原 `is_attack_target` 子集 ⇒ oracle）：**`HAM_ALL`（新角色默认 `AttatckMode=0`）
    /// 下允许 PvP**（`PlayObject.Attack.cs:632-640` 对 `Race=Play` 目标 `result=true`，再经
    /// `IsProtectTarget` 安全区门）；自己宝宝在 `HAM_ALL` 下也被允许（`PlayObject.cs:1944-1948`），
    /// 非 `HAM_ALL` 才拒。
    #[test]
    fn attack_target_rules_follow_oracle_proper_target() {
        use crate::combat::{is_proper_target, ATTACK_MODE_HAM_ALL, ATTACK_MODE_HAM_PEACE};

        let mut attacker = player(1, "hero", 10, 10, 10);
        let other = player(2, "rival", 11, 10, 10);
        // 新角色 `PlayObject.cs:908`：HAM_ALL。
        assert_eq!(attacker.attack_mode, ATTACK_MODE_HAM_ALL);
        // PvP：HAM_ALL 且非安全区 ⇒ 允许（旧子集恒 false，F-K 的真实缺口）。
        assert!(
            is_proper_target(&attacker, &other),
            "HAM_ALL 非安全区应允许 PvP"
        );
        // 自己：恒拒。
        assert!(!is_proper_target(&attacker, &attacker));
        // 安全区：PvP 拒；怪仍可打（安全区只挡 Play 目标）。
        let mut safe = attacker.clone();
        safe.in_safe_zone = true;
        assert!(!is_proper_target(&safe, &other), "任一方在安全区应拒绝 PvP");

        // 自己宝宝（非玩、Master==自己）：HAM_ALL 允许，HAM_PEACE 拒。
        let mut pet = player(3, "pet", 11, 10, 1);
        pet.race = 80;
        pet.master = Some(attacker.id);
        assert!(
            is_proper_target(&attacker, &pet),
            "HAM_ALL 下自己宝宝允许（oracle）"
        );
        attacker.attack_mode = ATTACK_MODE_HAM_PEACE;
        assert!(
            !is_proper_target(&attacker, &pet),
            "非 HAM_ALL 下自己宝宝拒绝"
        );
        attacker.attack_mode = ATTACK_MODE_HAM_ALL;

        // 已死/幽灵/隐身：不可被选为目标（折入的目标可见性门）。
        let mut dead = other.clone();
        dead.death = true;
        assert!(!is_proper_target(&attacker, &dead));
        let mut ghost = other.clone();
        ghost.ghost = true;
        assert!(!is_proper_target(&attacker, &ghost));

        // 世界路径仍能把拒绝转成「无伤害」（`World::attack_cmd` 走同一函数）。
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(1, "hero", 10, 10, 10));
        w.tick();
        w.attack_cmd(p, p); // 自己
        let rec = w.tick();
        assert!(
            rec.attacks.iter().all(|(_, _, o)| o.damage == 0),
            "打自己不应造成伤害: {:?}",
            rec.attacks
        );
        assert!(rec.processed.iter().any(|(_, ok)| !*ok));
    }

    /// F-K 真机根因的正向回归：**两个玩家**在 `World::attack_cmd` 生产路径上能真的造成伤害
    /// （旧 `is_attack_target` 对人打人恒 false ⇒ 恒无 `SM_STRUCK`）。
    #[test]
    fn pvp_attack_lands_damage_in_world_path() {
        let mut w = World::new(32, 32);
        let a = w.add_entity(player(1, "hero", 10, 10, 10));
        let b = w.add_entity(player(2, "rival", 11, 10, 10));
        // `player()` 的 HitPoint=10 / SpeedPoint=0 ⇒ 命中判定必中（`10 < RandomByte(0)=0` 为假）。
        w.tick();
        w.attack_cmd(a, b);
        let rec = w.tick();
        let out = rec
            .attacks
            .iter()
            .find(|(atk, tgt, _)| *atk == a && *tgt == b)
            .map(|(_, _, o)| *o)
            .expect("PvP 攻击必须产生结算记录");
        assert!(out.damage > 0, "HAM_ALL 下 PvP 应造成伤害，实际 {out:?}");
        assert!(w.entities.get(b).unwrap().hp < 200, "目标 HP 应下降");
    }

    /// 掉线：离开世界、会话留在 `Left`（与"小退"的区别：不计数 softclose、语义上不可再进同一会话实例）。
    #[test]
    fn session_disconnect_leaves_world_without_softclose_count() {
        let mut w = World::with_maps(vec![(16, 16)]);
        w.open_session("hero");
        let mut e = Entity::new(0, "hero", 3, 3);
        e.race = ACTOR_RACE_PLAY;
        let id = w.enter_map("hero", e, 0).unwrap();
        assert_eq!(w.map(0).cell(3, 3).count(), 1);

        w.disconnect("hero").unwrap();
        assert!(w.entities.get(id).is_none(), "掉线后实体应从世界移除");
        assert_eq!(w.map(0).cell(3, 3).count(), 0, "掉线必须清掉格子登记");
        let sess = w.session("hero").unwrap();
        assert_eq!(sess.stage, crate::session::WorldStage::Left);
        assert_eq!(sess.actor_id, None);
        assert_eq!(sess.soft_close_count, 0, "掉线不计入小退次数");

        // 掉线后再掉线是非法转移（显式报错，不静默）
        assert_eq!(
            w.disconnect("hero").unwrap_err(),
            crate::session::SessionError::NotInWorld
        );
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

    /// `spawn_monster_precomputed`（M3④-实现 B 第 ③ 片的新 API）：掉落/朝向/构造抽已在回调里
    /// 按 oracle 次序预算完，本方法**不再取任何随机数**；构造抽结果随实体落库。
    #[test]
    fn spawn_monster_precomputed_consumes_no_rng() {
        let mut w = World::new(16, 16);
        let mut fresh = w.rng.clone();
        let ctor = MonsterCtorDraws {
            run_tick_delta: 11,
            base_search_time: 2222,
            attack_tick_delta: 33,
            walk_tick_delta: 44,
            search_time: 3333,
            meat_quality: 555,
            bo_fear_fire: true,
            zil_kill_count: 6,
        };
        let id = w
            .spawn_monster_precomputed(
                "鸡",
                3,
                3,
                0,
                80,
                2,
                30,
                40,
                0x0A_14,
                5,
                0,
                6,
                7,
                99,
                vec![],
                ctor,
            )
            .expect("生成成功");
        let e = w.entities.get(id).expect("怪物在仓库");
        assert_eq!(e.name, "鸡");
        assert_eq!(e.race, 80);
        assert_eq!(e.hp, 40);
        assert_eq!(e.max_hp, 40);
        assert_eq!(e.dc, 0x0A_14);
        assert_eq!(e.ac, 5);
        assert_eq!(e.life_attrib, 0);
        assert_eq!(e.hit_point, 6);
        assert_eq!(e.speed_point, 7);
        assert_eq!(e.mon_exp, 30);
        assert_eq!(e.gold, 99, "预置金币必须原样落库");
        assert!(e.drop_items.is_empty());
        assert_eq!(e.ctor_draws, ctor, "构造抽结果必须原样落实体");
        assert!(e.add_to_mapped, "生成后必须落图");
        // `world.rng` 必须一步未动。
        assert_eq!(w.rng.random(), fresh.random());
        // 越界地图 ⇒ None（不落库）。
        assert!(w
            .spawn_monster_precomputed(
                "鸡",
                3,
                3,
                9,
                80,
                0,
                0,
                1,
                0,
                0,
                0,
                0,
                0,
                0,
                vec![],
                MonsterCtorDraws::default(),
            )
            .is_none());
    }

    /// `budget_monster_drops`（新 API）的掉落预算与 `spawn_monster_now`（既有路径）逐字节相同。
    #[test]
    fn budget_monster_drops_matches_spawn_monster_now_drop_budget() {
        let drop_list = vec![MonsterDropItem {
            max_point: 100,
            sel_point: 100, // 恒命中
            item_name: STRING_GOLD_NAME.to_string(),
            count: 1000,
        }];
        let mut a = World::new(16, 16);
        let mut rng = RandomNumber::with_seed(5);
        let (gold, items) = a.budget_monster_drops(&mut rng, &drop_list);

        let mut b = World::new(16, 16);
        b.rng = RandomNumber::with_seed(5);
        b.spawn_monster(Command::SpawnMonster {
            id: 2,
            name: "鸡".into(),
            x: 1,
            y: 1,
            map_id: 0,
            level: 1,
            exp: 0,
            hp: 5,
            dc: 0,
            ac: 0,
            undead: 0,
            hit_point: 5,
            speed_point: 15,
            drop_list: drop_list.clone(),
        });
        b.tick();
        let e = b.entities.get(2).expect("spawned");
        assert_eq!(e.gold, gold, "预算金币必须与既有生成路径一致");
        assert_eq!(e.drop_items, items, "预算物品必须一致");
    }
}
