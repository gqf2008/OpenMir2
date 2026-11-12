//! **P0 / S3+ 组件骨架**：把 `crates/world` 的 151 字段大组件 `Entity` 拆成细粒度 ECS 组件的
//! **落点**（设计件：`docs/world-ecs-migration-20261113.md`）。
//!
//! ## 这个文件在 P0 里是什么
//!
//! * **是**：组件布局的权威定义（25 个组件、覆盖 151 个字段）+ 迁移用适配器的形状；
//! * **不是**：生产路径。本 crate 不接入任何 tick 系统，`crates/world` 的 `Entity` / `EntityStore`
//!   一行未改；P0 的门禁只要求「能编译 + 字段 round-trip 不改值」。
//!
//! ## 为什么有几个组件带泛型载荷
//!
//! 依赖方向是 `mir2-world` → `mir2-world-bevy`（S1 起，防环）。本 crate **不能**依赖
//! `mir2-world`，因此拿不到 `AnimalState` / `MerchantState` / `NpcState` / `UseItemSlot` /
//! `VisibleBaseObject` / `DropItem` 等 world 侧类型。P0 的处理：把这些**整块透传的状态**做成
//! 泛型载荷组件（`AiState<A>` 等），载荷类型由迁移侧/P1 落点给出。P1 若把状态的数据定义搬进
//! 本 crate（或独立 `mir2-world-types` 小 crate），组件即可退化为具体类型。
//!
//! 这层泛型是**迁移缝**，不是最终形态：它保证 P0 不复制 100+ 个字段的镜像结构（复制会立刻
//! 产生两份真值），也不破坏依赖方向。
//!
//! ## 适配器为什么是 `#[cfg(test)]`
//!
//! `From<&Entity>` / `apply_to(&mut Entity)` 需要 `mir2-world` 的 `Entity`。两条现有约束把
//! 它挡在正常依赖之外：
//!
//! 1. `tests/bevy_drop_parity.rs::no_parallel_guard` 对 `[dependencies]` 做**精确白名单**
//!    （只允许 `bevy_ecs`/`bevy_app`/`mir2-shared`/`mir2-data`/`mir2-formula`）；
//! 2. 依赖方向必须保持 `world -> world-bevy`（S1 的防环契约）。
//!
//! ⇒ 适配器只能作为 **dev-dependency + `cfg(test)`** 存在（`Cargo.toml` 的
//! `[dev-dependencies]` 不在白名单扫描域内）。它们的作用是给 P0 的 round-trip 与 P1 的切片
//! 验证当**迁移工具**；生产代码永远不引用它们。P1 的迁移工具应放在 `crates/world` 侧
//! （它本来就依赖本 crate），而不是反向依赖。
//!
//! ## 顺序契约（不变）
//!
//! 组件化**不改变**遍历顺序来源：系统仍然只按显式槽位索引（[`crate::SlotOrder`] /
//! [`crate::TickOrder`]）处理，`Query` 迭代序不是契约。新增/删除组件会改变 archetype 布局，
//! 因此更不能用 `Query` 迭代求顺序（见设计件「风险」节）。

use bevy_ecs::prelude::*;

/// 装备/背包槽位数（C# `CharacterObject.UseItems` = 13）。
pub const USE_ITEM_SLOTS: usize = 13;

/// 状态计时槽位数（C# `BaseObject.StatusTimeArr` = 15）。
pub const STATUS_TIME_SLOTS: usize = 15;

/// `ExtraAbil` 槽位数（C# `PlayObject.ExtraAbil[7]`）。
pub const EXTRA_ABIL_SLOTS: usize = 7;

// ============================ 1. 身份与空间 ============================

/// 业务身份：`ActorId` + `ChrName`。
///
/// **迁移注意**：`id` 同时是 `EcsStore` 的索引键，不能只活在组件里；P1 起由 store 索引与
/// 组件各持一份、以 store 索引为准（生成后不可变）。
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct CoreIdentity {
    pub id: i32,
    pub name: String,
}

/// 位置与视野（oracle `CurrX` / `CurrY` / `MapId` / `ViewRange`）。
///
/// 热路径：AOI 扫描、格子索引、移动/推挤/冲锋都读写它。**最后一个迁移**（见设计件切片 P9）。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Transform {
    pub x: i32,
    pub y: i32,
    pub map_id: usize,
    pub view_range: i32,
}

/// 外观轴：`RaceImg` / `Appr` / `Light` / `NameColor` + 行为轴 `Race`。
///
/// `race` 与 `race_img` 是**两个不同的轴**（商人：`race = NPC(10)`、`race_img = Merchant(50)`），
/// 不得合并。`race` 参与攻击/死亡/掉落判定，使用面很广（Leader 统计约 96 处），因此本组
/// 不按「外观」的低风险假设排早（见设计件迁移顺序）。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Appearance {
    pub race: u8,
    pub race_img: u8,
    pub appr: u16,
    pub light: u8,
    pub name_color: u8,
}

/// 地图绑定/观察者状态：入图标记、可见集、可见事件、活跃位、tick 标记、VerifyTick。
///
/// `V` = 可见对象条目（world 侧 `VisibleBaseObject`），`E` = 可见事件条目
/// （world 侧 `VisibleEvent`）。它们与 AOI 同切片迁移。
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct MapBinding<V = (), E = ()> {
    pub add_to_mapped: bool,
    pub visible_actors: Vec<V>,
    pub visible_events: Vec<E>,
    pub is_visible_active: bool,
    pub processed_tick: u64,
    pub verify_tick: i64,
}

// 手写 `Default`（derive 会给 `V`/`E` 加 `Default` bound，而 `VisibleBaseObject`/
// `VisibleEvent` 不实现 `Default`）。6 字段缺省与 `Entity::new` 的 ctor 缺省逐项一致
// （`false` / 空 / 空 / `false` / `0` / `0`），供 `EntityStore::insert` 统一挂载。
impl<V, E> Default for MapBinding<V, E> {
    fn default() -> Self {
        MapBinding {
            add_to_mapped: false,
            visible_actors: Vec::new(),
            visible_events: Vec::new(),
            is_visible_active: false,
            processed_tick: 0,
            verify_tick: 0,
        }
    }
}

// ============================ 2. 属性与成长 ============================

/// 战斗数值（`WAbil` 的可变子集 + `HitSpeed`）。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vitals {
    pub hp: u16,
    pub max_hp: u16,
    pub mp: u16,
    pub max_mp: u16,
    pub hit_speed: u16,
    pub dc: i32,
    pub ac: i32,
    pub mac: u16,
    pub hit_point: u8,
    pub speed_point: u8,
    pub anti_magic: u16,
    pub anti_poison: u8,
    pub undead_power: i32,
}

/// `Abil` 快照（oracle `BaseObject.Abil` 的本骨架子集：`DC` / `MaxHP`）。
///
/// `B` = world 侧 `BaseAbil`。`None` = 尚未取快照（生成路径直接写 `Vitals`，首次需要时懒取）。
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbilitySnapshot<B = ()> {
    pub abil: Option<B>,
}

// 手写 `Default`（derive 会给 `B` 加 `Default` bound，而 world 侧 `BaseAbil` 不实现）。
// 缺省 `None` = 尚未取快照，与 `Entity::new` 的 ctor 缺省一致。
impl<B> Default for AbilitySnapshot<B> {
    fn default() -> Self {
        AbilitySnapshot { abil: None }
    }
}

/// 成长/等级：职业、不死系、等级、经验与杀怪经验基数。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progression {
    pub job: u8,
    pub life_attrib: u8,
    pub level: u8,
    pub mon_exp: i32,
    pub exp: i32,
    pub max_exp: i32,
}

// ============================ 3. 状态位与生命周期 ============================

/// 布尔/模式状态位（可见性、管理员、石化、狂暴外的通用标志）。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub invisible: bool,
    pub hide_mode: bool,
    pub fixed_hide_mode: bool,
    pub ob_mode: bool,
    pub nasty_mode: bool,
    pub attack_mode: u8,
    pub in_safe_zone: bool,
    pub admin_mode: bool,
    pub stone_mode: bool,
    pub super_man: bool,
    pub rush_mode: bool,
    pub want_ref_msg: bool,
    pub stick_mode: bool,
}

/// PK/善恶状态（`PvpLevel` 派生自 `pk_point`，见 oracle 事实）。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PvpState {
    pub dear_human: Option<i32>,
    pub pk_point: i32,
    pub pvp_flag: bool,
    pub pvp_name_color_tick: i64,
    pub die_in_fight3_zone: bool,
}

/// 生死与幽灵生命周期（`Death` / `Ghost` / 两个时刻 / 可复活 / 已结算 / 直接 Die 标记）。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LifeCycle {
    pub death: bool,
    pub ghost: bool,
    pub death_tick: i64,
    pub ghost_tick: i64,
    pub can_re_alive: bool,
    pub re_alive_tick: i64,
    pub death_settled: bool,
    pub die_called: bool,
}

/// buff / 毒 / 心灵启示 / 魔法盾 / `ExtraAbil` 计时。
///
/// `status_time` 只有写入点、没有周期衰减（oracle 的递减循环被注释）⇒ 本组不做「每 tick 递减」
/// 的想当然实现。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuffState {
    pub poisoning_tick: i64,
    pub green_poisoning_point: u8,
    pub char_status: i32,
    pub char_status_ex: i32,
    pub status_time: [u16; STATUS_TIME_SLOTS],
    pub transparent: bool,
    pub show_hp: bool,
    pub show_hp_tick: i64,
    pub show_hp_interval: i32,
    pub magic_shield: bool,
    pub un_magic_shield: bool,
    pub extra_abil: [u16; EXTRA_ABIL_SLOTS],
    pub extra_abil_times: [i64; EXTRA_ABIL_SLOTS],
    pub extra_abil_flag: [u8; EXTRA_ABIL_SLOTS],
}

/// 玩家 `Run` 的运行期状态：幸运、1s 递减倍率、随机号、烈火/双龙到期。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerRunState {
    pub luck: i32,
    pub power_rate: i32,
    pub power_rate_time: i32,
    pub kill_mon_exp_rate: i32,
    pub kill_mon_exp_rate_time: i32,
    pub exp_rate_tick: i64,
    pub random_no: i32,
    pub bo_power_item: bool,
    pub power_item: u8,
    pub fire_hit_skill: bool,
    pub latest_fire_hit_tick: i64,
    pub twin_hit_skill: bool,
    pub latest_twin_hit_tick: i64,
}

/// 重叠推挤状态（`PlayObject.Run` 的 `CheckDupObj` 段）。
///
/// **S3a 首个落地组件**：3 个字段、无存档面、专项用例
/// `crates/world/tests/check_dup_obj_vectors.rs`。字段的 oracle 语义：
///
/// - `check_dup_obj_tick`：上一次重叠检查的时刻；ctor `CheckDupObjTick = GetTickCount()`
///   （`PlayObject.cs:913`）⇒ `EntityStore::insert` 默认 `0`，落图点（`World::add_entity` /
///   `SessionRegistry::enter_map_at`）按 ctor 时刻/落图时钟重盖。
/// - `bo_duplication` / `dup_start_tick`：本格对象数 `>= 2` 的**首次**观测标记与时刻；
///   对象数掉回 `< 2` 只清 `bo_duplication`，**不**复位 `dup_start_tick`（照 oracle）。
///
/// 真值归属：S3a 起**只在组件里**（`Entity` 已删除这三个字段）；读写经
/// `EntityStore::dup_state` / `dup_state_mut`，业务模块不直接持有裸 `Query`。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DupState {
    pub bo_duplication: bool,
    pub dup_start_tick: i64,
    pub check_dup_obj_tick: i64,
}

impl DupState {
    /// ctor 缺省（`Entity::new` 迁出前的同值：`false/0/0`）。
    pub const fn new() -> Self {
        DupState {
            bo_duplication: false,
            dup_start_tick: 0,
            check_dup_obj_tick: 0,
        }
    }
}

/// 装备槽（oracle `CharacterObject.UseItems`，13 格）。
///
/// `S` = world 侧 `UseItemSlot`。存档面：`DuraMax` / `Desc` 随槽进存档，迁移时不得丢。
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Inventory<S = ()> {
    pub use_items: [S; USE_ITEM_SLOTS],
}

/// 掉落预算与「禁止掉落」门（`Gold` / `ItemList` / `AngryRing` / `NoDropUseItem` / `NoItem`）。
///
/// `D` = world 侧 `DropItem`（= `mir2_formula::drop::UserItem`）。
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct DropState<D = ()> {
    pub gold: i32,
    pub items: Vec<D>,
    pub angry_ring: bool,
    pub no_drop_use_item: bool,
    pub no_item: bool,
}

/// 命中归属：`LastHiter` / `ExpHitter` 与两个过期时刻。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CombatAttribution {
    pub last_hiter: Option<i32>,
    pub exp_hitter: Option<i32>,
    pub last_hiter_tick: i64,
    pub exp_hitter_tick: i64,
}

/// HP/MP 自然恢复计时（`HealthTick` / `SpellTick` / `AutoRecoveryTick`）。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Recovery {
    pub health_tick: i32,
    pub spell_tick: i32,
    pub auto_recovery_tick: i64,
}

/// 复活戒指 / 火球戒指的粘滞锁存（oracle 无置 false 的写入点，照抄）。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Revival {
    pub revival: bool,
    pub revival_tick: i64,
    pub un_revival: bool,
    pub flame_ring: bool,
}

// ============================ 4. 宝宝/归属 ============================

/// 宝宝/下属关系与主仆生命周期。
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct SlaveState {
    pub master: Option<i32>,
    pub slave_relax: bool,
    pub slave_list: Vec<i32>,
    pub special_children: Vec<i32>,
    pub is_slave: bool,
    pub is_clone: bool,
    pub slave_exp_level: u8,
    pub slave_make_level: u8,
    pub master_royalty_tick: i64,
    pub master_tick: i64,
    pub class_slave_list: Vec<i32>,
}

/// 生成/归属绑定：城堡防御单元、MonGen 下标、ZenTime 缓存、ctor 抽数。
///
/// `C` = world 侧 `CastleBinding`，`D` = world 侧 `MonsterCtorDraws`。三者都在生成时写一次。
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnBinding<C = (), D = ()> {
    pub castle_binding: Option<C>,
    pub mon_gen: Option<usize>,
    pub mon_gen_zen_time: i32,
    pub ctor_draws: D,
}

// ============================ 5. 类专用状态与 AI ============================

/// 怪物类专用状态（MB 系列的类私有字段：CowKing / ElfWarrior / ScultureKing / SoccerBall …）。
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MonsterClassState {
    pub come_out_value: i32,
    pub attack_range: i32,
    pub use_magic: bool,
    pub danger_level: i32,
    pub n550: i32,
    pub class_attack_tick: i64,
    pub bo_is_first: bool,
    pub dig_down_tick: i64,
    pub appear_start_tick: i64,
    pub guard_direction: i8,
    pub siege_lock_count: i32,
    pub jump_time: i64,
    pub crazy_ready_mode: bool,
    pub crazy_king_mode: bool,
    pub crazy_count: i32,
    pub crazy_ready: i64,
    pub crazy_time: i64,
    pub old_hit_time: i64,
    pub old_walk_time: i64,
}

/// 动物/怪物 AI 状态（oracle `AnimalObject` / `MonsterObject` 计时器与目标格子集）。
///
/// `A` = world 侧 `AnimalState`。该字段是全部字段里使用最广的（Leader 统计约 173 处），
/// **最后迁移**；P0 不拆它的内部字段（拆内部字段会引入 40+ 个新引用面）。
#[derive(Component, Clone, Debug, PartialEq)]
pub struct AiState<A = ()> {
    pub ai: A,
}

/// 商人状态（交易商人的身份门：`Some` ⇒ 商人）。
///
/// `M` = world 侧 `MerchantState`。
#[derive(Component, Clone, Debug, PartialEq)]
pub struct MerchantState<M = ()> {
    pub merchant: Option<M>,
}

/// 固定 NPC 状态（`Envir/Npcs.txt` 的管理 NPC）。
///
/// `N` = world 侧 `NpcState`。
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NpcState<N = ()> {
    pub npc: Option<N>,
}

/// 组队实体侧状态（**组队服务本身仍是 actor**，见 AGENTS §4.2.2）。
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupState {
    pub allow_group: bool,
    pub allow_group_re_call: bool,
    pub group_owner: i32,
    pub group_members: Vec<i32>,
}

// ============================ 迁移适配器（仅测试/迁移工具） ============================
//
// 只做「Entity ↔ 组件」的形状验证，**不接生产**。见文件头「适配器为什么是 cfg(test)」。
// P1 的落地方式是「每片搬一组字段出 Entity」，届时用这里的适配器在片边界做值等价断言，
// 断言通过后适配器随旧字段一起删除。
#[cfg(test)]
mod adapters {
    use super::*;
    use mir2_world::combat::DropItem;
    use mir2_world::entity::{AnimalState, CastleBinding, Entity, MonsterCtorDraws, UseItemSlot};
    use mir2_world::merchant::MerchantState as WorldMerchantState;
    use mir2_world::npc::NpcState as WorldNpcState;

    /// 迁移工具用：把一组字段从 `Entity` 读成组件。
    pub trait FromEntity {
        fn from_entity(e: &Entity) -> Self;
    }

    /// 迁移工具用：把组件写回 `Entity`（只动本组件拥有的字段）。
    pub trait ApplyToEntity {
        fn apply_to(&self, e: &mut Entity);
    }

    /// 全 `Copy` 字段组：直接搬值（避免在 `Copy` 上写 `.clone()`）。
    macro_rules! copy_adapter {
        ($ty:ty, $($field:ident),+ $(,)?) => {
            impl FromEntity for $ty {
                fn from_entity(e: &Entity) -> Self {
                    Self { $($field: e.$field),+ }
                }
            }
            impl ApplyToEntity for $ty {
                fn apply_to(&self, e: &mut Entity) {
                    $(e.$field = self.$field;)+
                }
            }
        };
    }

    /// 含 `String`/`Vec` 的字段组：按值 clone。
    macro_rules! clone_adapter {
        ($ty:ty, $($field:ident),+ $(,)?) => {
            impl FromEntity for $ty {
                fn from_entity(e: &Entity) -> Self {
                    Self { $($field: e.$field.clone()),+ }
                }
            }
            impl ApplyToEntity for $ty {
                fn apply_to(&self, e: &mut Entity) {
                    $(e.$field = self.$field.clone();)+
                }
            }
        };
    }

    clone_adapter!(CoreIdentity, id, name);
    copy_adapter!(Appearance, race, race_img, appr, light, name_color);
    copy_adapter!(
        Flags,
        invisible,
        hide_mode,
        fixed_hide_mode,
        ob_mode,
        nasty_mode,
        attack_mode,
        in_safe_zone,
        admin_mode,
        stone_mode,
        super_man,
        rush_mode,
        want_ref_msg,
        stick_mode
    );
    copy_adapter!(
        PvpState,
        dear_human,
        pk_point,
        pvp_flag,
        pvp_name_color_tick,
        die_in_fight3_zone
    );
    copy_adapter!(
        LifeCycle,
        death,
        ghost,
        death_tick,
        ghost_tick,
        can_re_alive,
        re_alive_tick,
        death_settled,
        die_called
    );
    copy_adapter!(
        BuffState,
        poisoning_tick,
        green_poisoning_point,
        char_status,
        char_status_ex,
        status_time,
        transparent,
        show_hp,
        show_hp_tick,
        show_hp_interval,
        magic_shield,
        un_magic_shield,
        extra_abil,
        extra_abil_times,
        extra_abil_flag
    );
    copy_adapter!(
        PlayerRunState,
        luck,
        power_rate,
        power_rate_time,
        kill_mon_exp_rate,
        kill_mon_exp_rate_time,
        exp_rate_tick,
        random_no,
        bo_power_item,
        power_item,
        fire_hit_skill,
        latest_fire_hit_tick,
        twin_hit_skill,
        latest_twin_hit_tick
    );
    copy_adapter!(
        CombatAttribution,
        last_hiter,
        exp_hitter,
        last_hiter_tick,
        exp_hitter_tick
    );
    copy_adapter!(Recovery, health_tick, spell_tick, auto_recovery_tick);
    copy_adapter!(Revival, revival, revival_tick, un_revival, flame_ring);
    clone_adapter!(
        SlaveState,
        master,
        slave_relax,
        slave_list,
        special_children,
        is_slave,
        is_clone,
        slave_exp_level,
        slave_make_level,
        master_royalty_tick,
        master_tick,
        class_slave_list
    );
    copy_adapter!(
        MonsterClassState,
        come_out_value,
        attack_range,
        use_magic,
        danger_level,
        n550,
        class_attack_tick,
        bo_is_first,
        dig_down_tick,
        appear_start_tick,
        guard_direction,
        siege_lock_count,
        jump_time,
        crazy_ready_mode,
        crazy_king_mode,
        crazy_count,
        crazy_ready,
        crazy_time,
        old_hit_time,
        old_walk_time
    );
    clone_adapter!(
        GroupState,
        allow_group,
        allow_group_re_call,
        group_owner,
        group_members
    );

    impl FromEntity for Inventory<UseItemSlot> {
        fn from_entity(e: &Entity) -> Self {
            Inventory {
                use_items: e.use_items,
            }
        }
    }
    impl ApplyToEntity for Inventory<UseItemSlot> {
        fn apply_to(&self, e: &mut Entity) {
            e.use_items = self.use_items;
        }
    }

    impl FromEntity for DropState<DropItem> {
        fn from_entity(e: &Entity) -> Self {
            DropState {
                gold: e.gold,
                items: e.drop_items.clone(),
                angry_ring: e.angry_ring,
                no_drop_use_item: e.no_drop_use_item,
                no_item: e.no_item,
            }
        }
    }
    impl ApplyToEntity for DropState<DropItem> {
        fn apply_to(&self, e: &mut Entity) {
            e.gold = self.gold;
            e.drop_items = self.items.clone();
            e.angry_ring = self.angry_ring;
            e.no_drop_use_item = self.no_drop_use_item;
            e.no_item = self.no_item;
        }
    }

    impl FromEntity for SpawnBinding<CastleBinding, MonsterCtorDraws> {
        fn from_entity(e: &Entity) -> Self {
            SpawnBinding {
                castle_binding: e.castle_binding,
                mon_gen: e.mon_gen,
                mon_gen_zen_time: e.mon_gen_zen_time,
                ctor_draws: e.ctor_draws,
            }
        }
    }
    impl ApplyToEntity for SpawnBinding<CastleBinding, MonsterCtorDraws> {
        fn apply_to(&self, e: &mut Entity) {
            e.castle_binding = self.castle_binding;
            e.mon_gen = self.mon_gen;
            e.mon_gen_zen_time = self.mon_gen_zen_time;
            e.ctor_draws = self.ctor_draws;
        }
    }

    impl FromEntity for AiState<AnimalState> {
        fn from_entity(e: &Entity) -> Self {
            AiState { ai: e.ai.clone() }
        }
    }
    impl ApplyToEntity for AiState<AnimalState> {
        fn apply_to(&self, e: &mut Entity) {
            e.ai = self.ai.clone();
        }
    }

    impl FromEntity for MerchantState<WorldMerchantState> {
        fn from_entity(e: &Entity) -> Self {
            MerchantState {
                merchant: e.merchant.clone(),
            }
        }
    }
    impl ApplyToEntity for MerchantState<WorldMerchantState> {
        fn apply_to(&self, e: &mut Entity) {
            e.merchant = self.merchant.clone();
        }
    }

    impl FromEntity for NpcState<WorldNpcState> {
        fn from_entity(e: &Entity) -> Self {
            NpcState { npc: e.npc }
        }
    }
    impl ApplyToEntity for NpcState<WorldNpcState> {
        fn apply_to(&self, e: &mut Entity) {
            e.npc = self.npc;
        }
    }

    /// 全部组件的**具体实例化**（P0 round-trip 的载体；P1 每片从这里减一项）。
    #[derive(Clone, Debug)]
    pub struct EntityComponents {
        pub identity: CoreIdentity,
        pub appearance: Appearance,
        pub flags: Flags,
        pub pvp: PvpState,
        pub life: LifeCycle,
        pub buff: BuffState,
        pub run_state: PlayerRunState,
        pub inventory: Inventory<UseItemSlot>,
        pub drop: DropState<DropItem>,
        pub attribution: CombatAttribution,
        pub recovery: Recovery,
        pub revival: Revival,
        pub slave: SlaveState,
        pub spawn: SpawnBinding<CastleBinding, MonsterCtorDraws>,
        pub class_state: MonsterClassState,
        pub ai: AiState<AnimalState>,
        pub merchant: MerchantState<WorldMerchantState>,
        pub npc: NpcState<WorldNpcState>,
        pub group: GroupState,
    }

    impl EntityComponents {
        pub fn split(e: &Entity) -> Self {
            EntityComponents {
                identity: FromEntity::from_entity(e),
                appearance: FromEntity::from_entity(e),
                flags: FromEntity::from_entity(e),
                pvp: FromEntity::from_entity(e),
                life: FromEntity::from_entity(e),
                buff: FromEntity::from_entity(e),
                run_state: FromEntity::from_entity(e),
                inventory: FromEntity::from_entity(e),
                drop: FromEntity::from_entity(e),
                attribution: FromEntity::from_entity(e),
                recovery: FromEntity::from_entity(e),
                revival: FromEntity::from_entity(e),
                slave: FromEntity::from_entity(e),
                spawn: FromEntity::from_entity(e),
                class_state: FromEntity::from_entity(e),
                ai: FromEntity::from_entity(e),
                merchant: FromEntity::from_entity(e),
                npc: FromEntity::from_entity(e),
                group: FromEntity::from_entity(e),
            }
        }

        pub fn apply_to(&self, e: &mut Entity) {
            self.identity.apply_to(e);
            self.appearance.apply_to(e);
            self.flags.apply_to(e);
            self.pvp.apply_to(e);
            self.life.apply_to(e);
            self.buff.apply_to(e);
            self.run_state.apply_to(e);
            self.inventory.apply_to(e);
            self.drop.apply_to(e);
            self.attribution.apply_to(e);
            self.recovery.apply_to(e);
            self.revival.apply_to(e);
            self.slave.apply_to(e);
            self.spawn.apply_to(e);
            self.class_state.apply_to(e);
            self.ai.apply_to(e);
            self.merchant.apply_to(e);
            self.npc.apply_to(e);
            self.group.apply_to(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::adapters::EntityComponents;
    use super::*;
    use mir2_world::castle::DefenseRole;
    use mir2_world::combat::DropItem;
    use mir2_world::entity::{
        BaseAbil, CastleBinding, Entity, EntitySpawn, MonsterCtorDraws, UseItemSlot,
    };
    use mir2_world::entity::{VisibleBaseObject, VisibleEvent};
    use mir2_world::merchant::{MerchantFlags, MerchantState as WorldMerchantState};
    use mir2_world::npc::NpcState as WorldNpcState;

    /// 给 **`Entity` 当前全部字段逐个** 赋非缺省值：任何一组适配器漏字段/写错字段，
    /// 全量 round-trip 的 `Debug` 比对都会红。
    ///
    /// S3a/S3b/S4a 起已迁出的字段不再参与本函数：`DupState` 3 + `Transform` 4 +
    /// `MapBinding` 6 + S4a 20（Vitals 13 / AbilitySnapshot 1 / Progression 6）⇒
    /// `Entity` 剩 **118 字段**（P0 的 151 减 33）。迁出字段的 value 语义由各自的
    /// 组件契约用例（如 `s4a_component_ctor_defaults_match_entity_new`）钉住。
    fn distinctive_entity() -> EntitySpawn {
        let mut e = Entity::new(91, "ecs-p0", 13, 27);

        // CoreIdentity
        // Appearance
        e.race = 50;
        e.race_img = 51;
        e.appr = 704;
        e.light = 2;
        e.name_color = 7;
        // Vitals
        e.hp = 41;
        e.max_hp = 42;
        e.mp = 43;
        e.max_mp = 44;
        e.hit_speed = 45;
        e.dc = 46;
        e.ac = 47;
        e.mac = 48;
        e.hit_point = 49;
        e.speed_point = 50;
        e.anti_magic = 51;
        e.anti_poison = 52;
        e.undead_power = 53;
        // AbilitySnapshot
        e.abil = Some(BaseAbil { dc: 54, max_hp: 55 });
        // Progression
        e.job = 2;
        e.life_attrib = 1;
        e.level = 33;
        e.mon_exp = 56;
        e.exp = 57;
        e.max_exp = 58;
        // Flags
        e.invisible = true;
        e.hide_mode = true;
        e.fixed_hide_mode = true;
        e.ob_mode = true;
        e.nasty_mode = true;
        e.attack_mode = 6;
        e.in_safe_zone = true;
        e.admin_mode = true;
        e.stone_mode = true;
        e.super_man = true;
        e.rush_mode = true;
        e.want_ref_msg = true;
        e.stick_mode = true;
        // PvpState
        e.dear_human = Some(59);
        e.pk_point = 200;
        e.pvp_flag = true;
        e.pvp_name_color_tick = 60;
        e.die_in_fight3_zone = true;
        // LifeCycle
        e.death = true;
        e.ghost = true;
        e.death_tick = 61;
        e.ghost_tick = 62;
        e.can_re_alive = true;
        e.re_alive_tick = 63;
        e.death_settled = true;
        e.die_called = true;
        // BuffState
        e.poisoning_tick = 70;
        e.green_poisoning_point = 3;
        e.char_status = 71;
        e.char_status_ex = 72;
        for (i, v) in e.status_time.iter_mut().enumerate() {
            *v = i as u16 + 1;
        }
        e.transparent = true;
        e.show_hp = true;
        e.show_hp_tick = 73;
        e.show_hp_interval = 74;
        e.magic_shield = true;
        e.un_magic_shield = true;
        e.extra_abil = [1, 2, 3, 4, 5, 6, 7];
        e.extra_abil_times = [75, 76, 77, 78, 79, 80, 81];
        e.extra_abil_flag = [1, 2, 3, 4, 5, 6, 7];
        // PlayerRunState
        e.luck = 8;
        e.power_rate = 102;
        e.power_rate_time = 9;
        e.kill_mon_exp_rate = 103;
        e.kill_mon_exp_rate_time = 10;
        e.exp_rate_tick = 82;
        e.random_no = 83;
        e.bo_power_item = true;
        e.power_item = 4;
        e.fire_hit_skill = true;
        e.latest_fire_hit_tick = 84;
        e.twin_hit_skill = true;
        e.latest_twin_hit_tick = 85;
        // DupState：S3a 已从 `Entity` 迁出（真值在组件里，见 `EntityStore::insert`），
        // 不再参与 Entity ↔ 组件的 round-trip。
        // Inventory（首尾两槽都写，证明整数组被搬运）
        let mut slots = [UseItemSlot::default(); USE_ITEM_SLOTS];
        slots[0] = UseItemSlot {
            make_index: 88,
            index: 89,
            dura: 90,
            dura_max: 91,
            desc: [1; 14],
        };
        slots[USE_ITEM_SLOTS - 1] = UseItemSlot {
            make_index: 92,
            index: 93,
            dura: 94,
            dura_max: 95,
            desc: [9; 14],
        };
        e.use_items = slots;
        // DropState
        e.gold = 96;
        e.drop_items = vec![DropItem {
            make_index: 97,
            index: 98,
            dura: 99,
            dura_max: 100,
            desc: [2; 14],
        }];
        e.angry_ring = true;
        e.no_drop_use_item = true;
        e.no_item = true;
        // CombatAttribution
        e.last_hiter = Some(101);
        e.exp_hitter = Some(102);
        e.last_hiter_tick = 103;
        e.exp_hitter_tick = 104;
        // Recovery
        e.health_tick = 105;
        e.spell_tick = 106;
        e.auto_recovery_tick = 107;
        // Revival
        e.revival = true;
        e.revival_tick = 108;
        e.un_revival = true;
        e.flame_ring = true;
        // SlaveState
        e.master = Some(109);
        e.slave_relax = true;
        e.slave_list = vec![110, 111];
        e.special_children = vec![112];
        e.is_slave = true;
        e.is_clone = true;
        e.slave_exp_level = 5;
        e.slave_make_level = 6;
        e.master_royalty_tick = 113;
        e.master_tick = 114;
        e.class_slave_list = vec![115];
        // SpawnBinding
        e.castle_binding = Some(CastleBinding {
            castle_index: 2,
            role: DefenseRole::Archer(3),
        });
        e.mon_gen = Some(4);
        e.mon_gen_zen_time = 116;
        e.ctor_draws = MonsterCtorDraws {
            run_tick_delta: 117,
            base_search_time: 118,
            attack_tick_delta: 119,
            walk_tick_delta: 120,
            search_time: 121,
            meat_quality: 122,
            bo_fear_fire: true,
            zil_kill_count: 123,
        };
        // MonsterClassState
        e.come_out_value = 124;
        e.attack_range = 125;
        e.use_magic = true;
        e.danger_level = 126;
        e.n550 = 127;
        e.class_attack_tick = 128;
        e.bo_is_first = true;
        e.dig_down_tick = 129;
        e.appear_start_tick = 130;
        e.guard_direction = -1;
        e.siege_lock_count = 131;
        e.jump_time = 132;
        e.crazy_ready_mode = true;
        e.crazy_king_mode = true;
        e.crazy_count = 133;
        e.crazy_ready = 134;
        e.crazy_time = 135;
        e.old_hit_time = 136;
        e.old_walk_time = 137;
        // AiState（整块透传；抽 3 个字段做非缺省）
        e.ai.dir = 7;
        e.ai.search_tick = 138;
        e.ai.target_creature = Some(139);
        // MerchantState（整块透传）
        e.merchant = Some(WorldMerchantState::new(
            "merchant-p0".to_string(),
            "map-p0".to_string(),
            3,
            4,
            105,
            true,
            true,
            140,
            MerchantFlags::default(),
            vec![1, 2],
            vec![],
            141,
        ));
        // NpcState
        e.npc = Some(WorldNpcState {
            npc_type: 2,
            npc_flag: 3,
            appr: 4,
            run_tick_delta: 142,
            base_search_time: 143,
            attack_tick_delta: 144,
            walk_tick_delta: 145,
            run_tick: 146,
            ghost_tick: 147,
            can_move: true,
            move_time: 148,
            move_tick: 149,
        });
        // GroupState
        e.allow_group = true;
        e.allow_group_re_call = true;
        e.group_owner = 150;
        e.group_members = vec![151, 152];

        e
    }

    /// S4a 起 Vitals/AbilitySnapshot/Progression 已迁出 `Entity`：round-trip 只比较
    /// `Entity` 主组件（剩余 118 字段）；迁出字段由各自组件契约用例钉住。
    fn debug_snapshot(e: &Entity) -> String {
        format!("{e:?}")
    }

    /// 全量 round-trip：Entity → 23 组件（`DupState`/`Transform`/`MapBinding` 已迁出）→ Entity，
    /// **剩余字段零变化**。
    ///
    /// 这条用例的判据是全字段 `Debug` 相等——任何一组适配器漏掉字段，落回的新实体就保留
    /// `Entity::new` 的缺省值，与源实体不同 ⇒ 红。若 `Entity` 以后新增字段而没进任何组件，
    /// 本用例**不会**自动发现（新字段在两侧都是缺省值）；因此同目录维护
    /// `distinctive_entity` 时必须同步加赋值（设计件「字段覆盖」节登记了这个边界）。
    #[test]
    fn split_and_apply_round_trip_all_remaining_fields() {
        let src = distinctive_entity();
        let split = EntityComponents::split(&src.entity);

        let mut restored = Entity::new(0, "", 0, 0);
        split.apply_to(&mut restored);

        assert_eq!(
            debug_snapshot(&src.entity),
            debug_snapshot(&restored.entity)
        );
    }

    /// 负对照：只改一个字段，`Debug` 快照必须不同（证明上面的比较不是恒等/空过）。
    #[test]
    fn debug_snapshot_detects_single_field_change() {
        let src = distinctive_entity();
        let mut altered = src.clone();
        altered.appr = altered.appr.wrapping_add(1);
        assert_ne!(debug_snapshot(&src.entity), debug_snapshot(&altered.entity));
    }

    /// S3a：`DupState` 已脱离 `Entity`，改由组件独立 API（`new`/`Default`）构造。
    ///
    /// 判据：组件缺省必须与迁出前 `Entity::new` 的 ctor 缺省逐字段一致（`false/0/0`），
    /// 否则 `EntityStore::insert` 给每个实体插的默认状态会让首次重叠检查的门基漂移。
    /// 牙：把 `DupState::new()` 的 `check_dup_obj_tick` 改成非 0 ⇒ 第二条断言红。
    #[test]
    fn dup_state_component_default_matches_entity_ctor_defaults() {
        let d = DupState::new();
        assert_eq!(d, DupState::default(), "`new` 与派生 `Default` 必须同值");
        assert!(!d.bo_duplication, "ctor 缺省 BoDuplication=false");
        assert_eq!(d.dup_start_tick, 0, "ctor 缺省 DupStartTick=0");
        assert_eq!(d.check_dup_obj_tick, 0, "ctor 缺省 CheckDupObjTick=0");
        // 组件可独立读写（不再需要 Entity 适配器）
        let mut state = DupState {
            bo_duplication: true,
            dup_start_tick: 1_234,
            check_dup_obj_tick: 5_678,
        };
        state.check_dup_obj_tick += 1;
        assert_eq!(state.check_dup_obj_tick, 5_679);
        assert_ne!(
            format!("{state:?}"),
            format!("{d:?}"),
            "非缺省状态必须可区分"
        );
    }

    /// S4a：`Vitals`/`AbilitySnapshot`/`Progression` 已脱离 `Entity`。
    ///
    /// 判据：生成期载体（`Entity::new`）的 ctor 缺省必须与迁移前逐字段一致
    /// （`hp/max_hp=100`、`level=1`、`abil=None`），且**不得**用 `Component::default()`
    /// 直接造实体（派生 `Default` 是 0/0/0）。
    ///
    /// 牙：把 `EntitySpawn::CTOR_LEVEL` 改成 0 ⇒ 升级/属性用例红（本用例 + `levelabil`）；
    /// 把 `CTOR_HP` 改成 0 ⇒ 进图即被死亡结算当死者（本用例红）。
    #[test]
    fn s4a_component_ctor_defaults_match_entity_new() {
        let e = Entity::new(1, "x", 0, 0);
        assert_eq!((e.hp, e.max_hp), (100, 100), "ctor 缺省 HP/MaxHP=100/100");
        assert_eq!(e.level, 1, "ctor 缺省 Abil.Level=1");
        assert_eq!(e.abil, None, "生成路径不取 Abil 快照");
        assert_eq!(e.job, 0);
        assert_eq!(e.life_attrib, 0);
        assert_eq!(e.mon_exp, 0);
        assert_eq!(e.exp, 0);
        assert_eq!(e.max_exp, 0);
        // 派生 Default 与 ctor 缺省**不同**：这是「不得用 Default 造实体」的负对照。
        assert_eq!(Vitals::default().hp, 0);
        assert_eq!(Vitals::default().max_hp, 0);
        assert_eq!(Progression::default().level, 0);
        assert_eq!(AbilitySnapshot::<BaseAbil>::default().abil, None);
    }

    #[test]
    fn transform_and_map_binding_defaults_match_ctor_contract() {
        let t = Transform::default();
        assert_eq!((t.x, t.y, t.map_id, t.view_range), (0, 0, 0, 0));
        assert_ne!(t.view_range, 5, "ctor ViewRange=5 不是组件 Default");

        let d = MapBinding::<VisibleBaseObject, VisibleEvent>::default();
        assert!(!d.add_to_mapped);
        assert!(d.visible_actors.is_empty());
        assert!(d.visible_events.is_empty());
        assert!(!d.is_visible_active);
        assert_eq!(d.processed_tick, 0);
        assert_eq!(d.verify_tick, 0);
    }
}
