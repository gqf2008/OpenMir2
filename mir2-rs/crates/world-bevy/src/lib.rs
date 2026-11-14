//! **世界模拟 crate（S5 合并后）**：`bevy_ecs` 承载「实体存储 + tick 骨架」（[`ecs`] / [`tick`]），
//! D3 的「掉落」系统（纯函数式：输入快照 → 输出命令），以及自 `crates/world` 并入的
//! 26 个模拟模块（实体/地图/AOI/战斗/魔法/城堡/怪物 AI/会话…，模块清单在本文件末）。
//!
//! 历史（S1–S4）：ECS 内核先住在本 crate、由 `crates/world` 依赖（`world -> world-bevy`）。
//! S5 把 `crates/world` 的模拟层整体并入本 crate：两侧不再互为依赖，`snapshot_from_world`
//! 适配层随之并入 [`bevy_bridge`]。
//!
//! ## 四条硬约束怎么落实（§4.2.2）
//!
//! 1. **单线程 executor**：显式用 [`bevy_ecs::schedule::SingleThreadedExecutor`]，
//!    绝不使用 `MultiThreadedExecutor`/`bevy_tasks` 并行（守卫：`tests/bevy_drop_parity.rs::no_parallel_guard`）；
//! 2. **遍历顺序显式化**：系统**不迭代任何 `Query`**——处理顺序来自我们自己的
//!    [`OrderIndex`] 资源（实体 id 升序，由调用方给出）。`Query` 迭代序在 bevy 里不是契约
//!    （archetype/table 顺序），本 crate 一处都不用；
//! 3. **确定性守卫升级**：本 crate 自己的 `bevy_drop_parity.rs::no_parallel_guard` 把白名单放宽为
//!    "只允许 `bevy_ecs`/`bevy_app`"（仍旧禁 rayon/crossbeam/bevy_tasks 并行 API），
//!    并保留"tick 必须在同一线程"的运行期断言思路（见测试）；
//! 4. **迁移验收＝原判据不降级**：S1 的实体存储/tick 骨架与原 `EntityStore`/`World::tick` 语义
//!    逐条一致（槽位顺序、空闲槽复用、代际号、时钟/线程断言），三个 golden（basic/session/combat）
//!    逐行不变；D3 的对拍断言「同一输入下，本系统输出的命令与既有实现逐项相同」也保持。
//!
//! ## 为什么第 2 条不是形式主义
//!
//! 掉落预算要按 C# 的**调用顺序**消耗同一个随机源；`Query` 迭代序一变，随机数落到不同
//! 怪物头上 ⇒ 与 C# 的逐行 diff 立刻破。所以顺序只能来自我们自己的索引。

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::SingleThreadedExecutor;

use mir2_data::models::{MonsterDropItem, StdItem, STRING_GOLD_NAME};
use mir2_formula::drop::{mon_get_random_items, ItemNumberCounter, UserItem, VecItemCatalog};
use mir2_shared::rng::RandomNumber;

// ============================ S1 迁移内核（实体存储 + tick 骨架） ============================
//
// 卡片 `openmir2-world-bevy-migration` 的 S1：`crates/world` 的**实体存储**与**tick 骨架**
// 迁到 `bevy_ecs`。内核与 D3 的掉落系统同 crate、同约束：单线程 executor + 自有序索引。
pub mod components;
pub mod ecs;
pub mod tick;

pub use components::{
    AbilitySnapshot, AiState, Appearance, BuffState, CombatAttribution, CoreIdentity, DropState,
    Flags, GroupState, Inventory, LifeCycle, MapBinding, MonsterClassState, PlayerRunState,
    Progression, PvpState, Recovery, Revival, SlaveState, SpawnBinding, Transform, Vitals,
    EXTRA_ABIL_SLOTS, STATUS_TIME_SLOTS, USE_ITEM_SLOTS,
};
pub use ecs::{EcsStore, SlotOrder};
pub use tick::{
    advance_clock_system, tick_schedule_build_settings, TickClock, TickOrder, TickOwnerThread,
    TickRunner, TickStage, STAGE_ORDER,
};

// ============================ 组件 / 资源 ============================

/// 怪物的业务 id（组件）——系统输出命令时带上它，避免依赖 ECS 的 `Entity` 句柄顺序。
#[derive(Component, Clone, Copy, Debug)]
pub struct MonsterId(pub i32);

/// 怪物实体上的掉落表（组件）。
#[derive(Component, Clone, Debug)]
pub struct DropTable(pub Vec<MonsterDropItem>);

/// **地图表资源（S3b）**：`World::maps` 在 tick 边界一次性移入本资源，供 `OutputAoi`
/// 系统扫描；跑完再移回 facade。同一时刻只有一个所有者（设计件 §4.2 的合法同步形态）。
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct MapsRes<M>(pub Vec<M>);

/// 怪物在地图上的位置（组件）。
#[derive(Component, Clone, Copy, Debug)]
pub struct Atlas {
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
}

/// 生成时预算出的掉落（组件；系统产出、由适配层写回既有世界）。
#[derive(Component, Clone, Debug, Default)]
pub struct DropBudget {
    pub gold: i32,
    pub items: Vec<UserItem>,
}

/// 随机源（资源）。同种子 ⇒ 与既有实现、与 C# 同流。
#[derive(Resource)]
pub struct RngRes(pub RandomNumber);

/// 物品目录（资源）：掉落生成按名查 `StdItem`。
#[derive(Resource)]
pub struct CatalogRes(pub Vec<StdItem>);

/// `Config.MonRandomAddValue`（资源）。
#[derive(Resource, Clone, Copy)]
pub struct MonRandomAddValue(pub i32);

/// 掉落物品的 `MakeIndex` 计数器（资源，对应 `M2Share.GetItemNumber`）。
#[derive(Resource, Default)]
pub struct ItemNumberRes(pub ItemNumberCounter);

/// **显式处理顺序**（资源）：实体 id 升序。系统只按这个顺序处理，不碰 `Query` 迭代序。
#[derive(Resource, Default)]
pub struct OrderIndex(pub Vec<bevy_ecs::prelude::Entity>);

/// 输出命令（资源）：本系统唯一的产出面。
#[derive(Resource, Default)]
pub struct OutBudgets(pub Vec<(i32, DropBudget)>);

// ============================ 纯函数式系统 ============================

/// 掉落预算系统：按 [`OrderIndex`] 顺序，为每个带 [`DropTable`] 的实体算出 [`DropBudget`]。
///
/// 随机数消耗顺序与 C# `WorldServer.MonGen.cs::MonGetRandomItems` 逐条一致
/// （公式本体来自 `crates/formula`，已与 C# 对拍）。
// bevy 系统的参数列表 = 它声明的数据访问（resource/query/commands），
// 参数多是 ECS 的常态而不是坏味道；clippy 的默认阈值不适用于 System。
#[allow(clippy::too_many_arguments)]
pub fn drop_budget_system(
    mut out: ResMut<OutBudgets>,
    order: Res<OrderIndex>,
    catalog: Res<CatalogRes>,
    add_value: Res<MonRandomAddValue>,
    mut rng: ResMut<RngRes>,
    mut item_number: ResMut<ItemNumberRes>,
    mut commands: Commands,
    tables: Query<(&MonsterId, &DropTable, Option<&DropBudget>)>,
) {
    // 注意：这里**只**用 `order.0` 决定顺序；`tables` 查询只用于按 Entity 取组件，
    // 不迭代它（`Query` 迭代序不是契约）。
    for entity in order.0.iter() {
        let Ok((monster_id, table, existing)) = tables.get(*entity) else {
            continue;
        };
        if existing.is_some() {
            continue; // 已有预算则不重复（幂等）
        }
        let catalog_ref = VecItemCatalog { items: &catalog.0 };
        let result = mon_get_random_items(
            &table.0,
            &catalog_ref,
            &mut rng.0,
            add_value.0,
            STRING_GOLD_NAME,
            &mut item_number.0,
        );
        let budget = DropBudget {
            gold: result.gold,
            items: result.items,
        };
        out.0.push((monster_id.0, budget.clone()));
        commands.entity(*entity).insert(budget);
    }
}

// ============================ 输入快照 / 输出命令 ============================

/// 输入快照（纯数据，不含任何 ECS 类型）：一次 tick 里所有需要预算掉落的怪物。
#[derive(Clone, Debug, Default)]
pub struct DropSnapshot {
    pub seed: i32,
    pub mon_random_add_value: i32,
    pub catalog: Vec<StdItem>,
    /// **顺序即处理顺序**（调用方给，通常 = 实体 id 升序）
    pub monsters: Vec<MonsterSnap>,
}

#[derive(Clone, Debug)]
pub struct MonsterSnap {
    pub id: i32,
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    pub drop_table: Vec<MonsterDropItem>,
}

/// 输出命令：预算结果（对应既有实现的 `Entity.gold` / `Entity.drop_items`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropPlanOut {
    pub id: i32,
    pub gold: i32,
    pub items: Vec<UserItem>,
}

/// 输出命令：把掉落物落到地上（对应既有实现的 `place_floor_item`）。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FloorPlaceOut {
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    pub obj_id: i32,
}

/// **纯函数式入口（预算）**：输入快照 → 输出命令。
///
/// 内部构造一个 `bevy_ecs::World`，用**单线程** `Schedule` 跑一次 [`drop_budget_system`]。
/// 除随机源外无任何隐藏状态：同快照 + 同种子 ⇒ 同输出。
///
/// **处理顺序**：按**实体 id 升序**建立 [`OrderIndex`]（与快照里数组的先后无关）。
/// 这一点是硬要求：掉落消耗同一条随机流，顺序一变结果就变（与 C# 的逐行 diff 会立刻破）。
/// 因此"ECS 的 spawn 顺序 / `Query` 迭代序"对本函数的输出**没有任何影响**。
pub fn plan_drops(snapshot: &DropSnapshot) -> Vec<DropPlanOut> {
    let mut world = bevy_ecs::prelude::World::new();
    world.insert_resource(RngRes(RandomNumber::with_seed(snapshot.seed)));
    world.insert_resource(CatalogRes(snapshot.catalog.clone()));
    world.insert_resource(MonRandomAddValue(snapshot.mon_random_add_value));
    world.insert_resource(ItemNumberRes::default());
    world.insert_resource(OutBudgets::default());

    // spawn 顺序 = 快照里的顺序（无所谓）；处理顺序 = 实体 id 升序（我们的索引说了算）
    let mut entity_of: Vec<(i32, bevy_ecs::prelude::Entity)> =
        Vec::with_capacity(snapshot.monsters.len());
    for m in &snapshot.monsters {
        let e = world
            .spawn((
                MonsterId(m.id),
                DropTable(m.drop_table.clone()),
                Atlas {
                    map_id: m.map_id,
                    x: m.x,
                    y: m.y,
                },
            ))
            .id();
        entity_of.push((m.id, e));
    }
    let mut sorted = entity_of.clone();
    sorted.sort_by_key(|(id, _)| *id);
    world.insert_resource(OrderIndex(sorted.into_iter().map(|(_, e)| e).collect()));

    let mut schedule = Schedule::default();
    schedule.set_executor(SingleThreadedExecutor::default());
    schedule.add_systems(drop_budget_system);
    schedule.run(&mut world);

    let out = world.resource::<OutBudgets>();
    out.0
        .iter()
        .map(|(id, budget)| DropPlanOut {
            id: *id,
            gold: budget.gold,
            items: budget.items.clone(),
        })
        .collect()
}

/// **常驻 world 形态**（真实集成用的形态）：world 建一次，之后每批只重置状态并重跑系统。
///
/// 与 [`plan_drops`] 的区别只在"是否重建 world"；数值输出必须与 [`plan_drops`] 逐项相同
/// （由 `bevy_drop_parity::persistent_world_matches_fresh_world` 断言）。
pub fn plan_drops_in_world(
    world: &mut bevy_ecs::prelude::World,
    snapshot: &DropSnapshot,
) -> Vec<DropPlanOut> {
    // 重置随机源/计数器/输出（每次预算都从同一种子重来 ⇒ 与 plan_drops 同流）
    world.insert_resource(RngRes(RandomNumber::with_seed(snapshot.seed)));
    world.insert_resource(ItemNumberRes::default());
    world.insert_resource(OutBudgets::default());
    // 清掉上一轮的预算组件（幂等语义：系统只在没有 DropBudget 时计算）
    let order: Vec<bevy_ecs::prelude::Entity> = world.resource::<OrderIndex>().0.clone();
    for e in order {
        world.entity_mut(e).remove::<DropBudget>();
    }
    let mut schedule = Schedule::default();
    schedule.set_executor(SingleThreadedExecutor::default());
    schedule.add_systems(drop_budget_system);
    schedule.run(world);
    let out = world.resource::<OutBudgets>();
    out.0
        .iter()
        .map(|(id, b)| DropPlanOut {
            id: *id,
            gold: b.gold,
            items: b.items.clone(),
        })
        .collect()
}

/// 建一个常驻 world（把快照里的怪物一次装进去）；之后用 [`plan_drops_in_world`] 反复跑。
pub fn build_world(snapshot: &DropSnapshot) -> bevy_ecs::prelude::World {
    let mut world = bevy_ecs::prelude::World::new();
    world.insert_resource(CatalogRes(snapshot.catalog.clone()));
    world.insert_resource(MonRandomAddValue(snapshot.mon_random_add_value));
    world.insert_resource(RngRes(RandomNumber::with_seed(snapshot.seed)));
    world.insert_resource(ItemNumberRes::default());
    world.insert_resource(OutBudgets::default());
    let mut entity_of: Vec<(i32, bevy_ecs::prelude::Entity)> =
        Vec::with_capacity(snapshot.monsters.len());
    for m in &snapshot.monsters {
        let e = world
            .spawn((
                MonsterId(m.id),
                DropTable(m.drop_table.clone()),
                Atlas {
                    map_id: m.map_id,
                    x: m.x,
                    y: m.y,
                },
            ))
            .id();
        entity_of.push((m.id, e));
    }
    entity_of.sort_by_key(|(id, _)| *id);
    world.insert_resource(OrderIndex(entity_of.into_iter().map(|(_, e)| e).collect()));
    world
}

/// 落地快照：把预算好的掉落物分散到空格子。
#[derive(Clone, Debug, Default)]
pub struct PlaceSnapshot {
    /// 初始被占用的格子（来自既有世界的地图；`(map_id, x, y)`）
    pub occupied: Vec<(usize, i32, i32)>,
    /// 顺序即处理顺序
    pub drops: Vec<DropAt>,
}

#[derive(Clone, Debug)]
pub struct DropAt {
    pub id: i32,
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    /// 依次尝试落地的对象 id（物品用 `index`、金币用怪物 id —— 与既有实现一致）
    pub obj_ids: Vec<i32>,
}

/// **纯函数式入口（落地）**：输入快照 → 输出命令。
///
/// 扫描顺序完全照 `BaseObject.GetDropPosition` 的子集：`for i in 0..range { for ii in -i..=i { for iii in -i..=i } }`，
/// 取第一个空格子；每落一件就把它记为已占（同一批内后续件不会再选同一格）。
pub fn plan_floor_placement(snapshot: &PlaceSnapshot) -> Vec<FloorPlaceOut> {
    let mut occupied: std::collections::HashSet<(usize, i32, i32)> =
        snapshot.occupied.iter().copied().collect();
    let mut out = Vec::new();
    for d in &snapshot.drops {
        for obj_id in &d.obj_ids {
            let mut placed_at = None;
            'scan: for i in 0..3 {
                for ii in -i..=i {
                    for iii in -i..=i {
                        let px = d.x + iii + 1;
                        let py = d.y + ii + 1;
                        if !occupied.contains(&(d.map_id, px, py)) {
                            placed_at = Some((px, py));
                            break 'scan;
                        }
                    }
                }
            }
            if let Some((px, py)) = placed_at {
                occupied.insert((d.map_id, px, py));
                out.push(FloorPlaceOut {
                    map_id: d.map_id,
                    x: px,
                    y: py,
                    obj_id: *obj_id,
                });
            }
        }
    }
    out
}

// 注：D3 的 `snapshot_from_world` 适配层（读世界实体 → 掉落快照）住在 [`bevy_bridge`]。
// S5 起模拟层与 ECS 内核同 crate，适配层不再跨 crate，函数签名与行为不变。

// ============================ 自 crates/world 并入的模拟层（S5） ============================
// M2 世界模拟骨架（单线程 tick + 实体存储 + 地图/AOI + 会话进出）。
//
// 行为基准：`src/GameSvr/Word/WorldServer.cs`（`ProcessHumans` 的 200ms 节拍）与
// `src/M2Server/Maps/Envirnoment.cs`、`src/M2Server/Actor/BaseObject.ViewRange.cs`。
//
// 范围（明确不做）：M1 的边界服务（LoginGate/SelGate/LoginSrv/DBSrv 一律不在此 crate）；
// 协议编解码见 `crates/protocol`（A 线）。
//
// 三条纪律：
// 1. **单线程串行**：`World::tick` 每次校验线程 id（见 `world.rs` 的测试）；
// 2. **确定性**：虚拟时钟 + 固定遍历顺序（S1 起遍历顺序来自本 crate 的显式槽位索引
//    `EcsStore`），模拟逻辑不读墙钟、不用 HashMap 迭代序；
// 3. **零并行依赖**：本 crate 只依赖 `bevy_ecs`（单线程 executor）与数据/公式 crate，
//    不引入任何线程/异步运行时（`tests/no_parallel.rs` 钉住）。

pub mod action;
pub mod aoi;
pub mod bevy_bridge;
pub mod buff;
pub mod castle;
pub mod castle_tax;
pub mod combat;
pub mod ctor_fold;
mod death_drop;
pub mod entity;
pub mod guild;
pub mod magic;
pub mod magic_effect;
pub mod map;
pub mod merchant;
pub mod monai;
pub mod mongen;
pub mod msgqueue;
pub mod npc;
pub mod party;
pub mod run_gap;
pub mod session;
pub mod slave;
pub mod space_move;
pub mod special_monster;
pub mod world;

pub use action::{ButchEvent, PushEvent, RushEvent, RushFrame, SkeletonEvent};
pub use aoi::{
    search_view_range, search_view_range_death, search_view_range_events,
    search_view_range_with_updates, update_monster_visible, update_visible_event,
    update_visible_gay, visible_snapshot, SearchViewRangeOutcome,
};
pub use bevy_bridge::{collect_tick_order_system, snapshot_from_world};
pub use buff::{
    damage_health, get_char_status, ActorBuffData, BuffSysMsg, BuffSystem, BuffTickOutcome,
    BuffType, BUFF_DESTROY_TICK_MS, BUFF_EPOCH_MS, BUFF_PASSES_PER_TICK, BUFF_PASS_INTERVAL_MS,
    BUFF_STATE_TICK_GATE_MS, BUFF_STATE_TICK_MAX, MAX_STATUS_ATTRIBUTE,
};
pub use castle::{
    apply_structure_events, castle_official_action, compare_lstr, door_map_xy_flag_ops,
    door_state_text, hire_substring, hutil32_round, official_repair_door, official_repair_wall,
    AttackerInfo, CastleDate, CastleDefenseSpecs, CastleEvent, CastleInitializeEnv, CastleNow,
    CastleOfficialAction, CastleSiegeConfig, CastleUnit, DefenseCreature, DefenseRole,
    DefenseSpawn, DefenseUnitSpec, GuildId, HireSubstringError, OfficialRepairResult,
    StructureEvent, UserCastle, CLOSE_MAIN_DOOR, DOOR_STATE_CLOSED, DOOR_STATE_DESTROYED,
    DOOR_STATE_OPENED, HIRE_ARCHER_NOW, HIRE_GUARD_NOW, MAX_CASTLE_ARCHER, MAX_CASTLE_GUARD,
    OFFICIAL_REPAIR_NOT_ENOUGH_GOLD_MSG, OFFICIAL_REPAIR_NOT_NEEDED_MSG, OFFICIAL_REPAIR_OK_MSG,
    OPEN_MAIN_DOOR, REPAIR_DOOR_NOW, REPAIR_WALL_NOW_1, REPAIR_WALL_NOW_2, REPAIR_WALL_NOW_3,
    SS_204, SS_212, STRUCTURE_REPAIR_DELAY_MS,
};
pub use castle_tax::{
    merchant_tax_amount, merchant_tax_target, CastleManager, CastleTaxConfig, IncRateGoldOutcome,
    MerchantTaxTarget, CASTLE_GOLD_SAVE_INTERVAL_MS,
};
pub use combat::{
    attack, attack_dir, attack_dir_grouped, attack_success_weapon_damage, attack_with,
    attack_with_grouped, get_attack_power_hit, is_attack_target, is_attack_target_base,
    is_attack_target_player, is_attack_target_player_grouped, is_proper_target,
    is_proper_target_grouped, is_proper_target_grouped_direct, is_protect_target, recalc_hit_speed,
    win_exp, AttackHitState, AttackOutcome, AttackPowerMode, CombatConfig, HitSpeedInput,
    HitSpeedOutcome, ACTOR_RACE_ANIMAL, ACTOR_RACE_NPC, ACTOR_RACE_PEACE_NPC, ATTACK_MODE_HAM_ALL,
    ATTACK_MODE_HAM_DEAR, ATTACK_MODE_HAM_GROUP, ATTACK_MODE_HAM_GUILD, ATTACK_MODE_HAM_MASTER,
    ATTACK_MODE_HAM_PEACE, ATTACK_MODE_HAM_PKATTACK,
};
pub use ctor_fold::{
    CtorDrawBatch, CtorFoldEntry, CtorFoldLedger, CtorFoldPlan, CtorFoldSite, LoginPlayerCtorDraws,
    CTOR_FOLD_BARRIER_A_DRAWS, CTOR_FOLD_BARRIER_A_MOD, CTOR_FOLD_DRAWS_PER_SITE,
    CTOR_FOLD_DRAW_KIND, PLAYER_CTOR_RANDOM_NO_MOD,
};
pub use entity::{
    BaseAnimalCtorDraws, Entity, EntityMut, EntityRef, EntitySpawn, EntityStore, MonsterCtorDraws,
    VisibleEvent, VisibleFlag, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY, BASE_ANIMAL_CTOR_DRAW_MODS,
};
// S3a：重叠推挤状态的真值已迁到 `world-bevy` 的组件；本 crate 透传，供宿主/测试按类型名使用。
pub use components::DupState;
pub use guild::{
    check_guild_name, AddWarGuildOutcome, GuildConfig, GuildManager, GuildRank, GuildRunEvent,
    GuildSaveReason, GuildState, GuildWarEnded, GuildWarEvent, GuildWarOutcome, GuildWarRequest,
    WarGuild,
};
pub use magic::{
    can_read_book, get_power, get_power13, get_rpow, get_spell_point, magic_attack_interval, mpow,
};
pub use map::{
    CellAttribute, CellObject, CellType, FloorItemKind, FloorItemMeta, MapCellInfo, MapGrid,
};
pub use merchant::{
    MerchantFlags, MerchantGoods, MerchantSpawn, MerchantState, ACTOR_RACE_MERCHANT,
    MERCHANT_RUN_TIME,
};
pub use monai::{
    animal_struck, attack_target, big_heart_attack_target, can_move, del_target_creature,
    dir_delta, get_attack_dir, goto_target_xy, is_active_seek_race, is_animal_race, is_guard_race,
    make_long, monster_ai_tick, monster_class_run, monster_run, search_target, set_target_creature,
    target_in_spit_range, think, turn_to, walk_to, wondering, ClassRunBase, MonsterRefMsg,
};
pub use mongen::{
    gen_mod_count, get_monsters_zen_time, initialize_monster, monster_death_gate,
    monster_due_for_ghost, monster_due_for_revive, monster_ghost_expired, process_cert_lists,
    regen_gate, regen_monsters, should_regen, step_threads, CertListRecord, MapCanCreate,
    MonGenConfig, MonGenEntry, MonGenState, MonGenThread, MonsterCreator, ReviveFrame,
    ReviveOutcome, TickClockFn, MON_GEN_THREAD_MON_LIMIT,
};
pub use npc::{NpcSpawn, NpcState, NPC_RUN_DELAY_MAX, NPC_SEARCH_DRAW_MAX};
pub use party::{
    PartyConfig, PartyEvent, PARTY_CANCEL_GROUP_MSG, PARTY_EXIT_GROUP_MSG, PARTY_EXP_BONUS,
    PARTY_GROUP_MAX, PARTY_GROUP_MEMBERS_MAX_DEFAULT, PARTY_GROUP_MSG_B_COLOR_DEFAULT,
    PARTY_GROUP_MSG_F_COLOR_DEFAULT, PARTY_GROUP_MSG_PREFIX_DEFAULT, PARTY_JOIN_GROUP_MSG_DEFAULT,
    PARTY_LEADER_CLOSE_MSG,
};
pub use session::{send_ref_msg_targets_at, Session, SessionError, WorldStage, SEND_REF_MSG_RANGE};
pub use slave::{
    check_server_make_slave, is_slave, mag_make_angel_slave_plan, mag_make_sinsu_slave_plan,
    mag_make_slave_plan, make_slave_gate, recall_slave_target, space_move_get_rand_xy, spawn_clone,
    summon_slave, CloneSpawn, MakeSlavePlan, SlaveSpawner, SummonOutcome, CLONE_NAME,
    PLAYER_VIEW_RANGE, SKILL_46, SKILL_ANGEL, SKILL_SINSU, SKILL_SKELLETON, SKILL_TAMMING,
};
pub use special_monster::{
    adopt_special_child, apply_ctor_fields, clone_operate, operate_special, special_monster_run,
    special_spawn_request, SpecialKind, SpecialRefMsg, SpecialSpawnOutcome, SpecialSpawnRequest,
    SpecialSpawner, CHILD_LIMIT, RACE_BEE_QUEEN, RACE_CLONE_MONSTER, RACE_SPIDER_HOUSE,
    RM_MAGSTRUCK, RM_MAGSTRUCK_MINE, RM_SPELL, RM_STRUCK, RM_ZEN_BEE, ZEN_BEE_DELAY_MS,
};
pub use world::{
    AutoGetExpState, BaseObjectMoveOutcome, Command, DigOutEvent, EntitySnapshot,
    ExtraAbilEventKind, ExtraAbilExpiry, FireBurnEvent, FireWallOutcome, FloorItemView,
    HideObserverKind, HomeMoveTarget, LampDuraChange, PassThroughHit, PassThroughOutcome,
    RouteCheck, RunStateExpiry, SpaceMoveCastOutcome, StepBlock, StepOutcome, TickRecord,
    UEnhancerOutcome, WeaponUpgradeEvent, WeaponUpgradeOutcome, World, TICK_INTERVAL_MS,
};
