//! S1：**单线程 tick 骨架**（`crates/world` 的世界 tick 迁入 `bevy_ecs` 的第一批）。
//!
//! 形态：[`TickRunner`] 包一个 [`Schedule`]，**显式**装上
//! [`SingleThreadedExecutor`]——世界 tick 仍由调用方（`crates/world` 的 `World::tick`）驱动，
//! 只是时钟推进/顺序收集等骨架步骤改由 ECS 系统完成。
//!
//! ## 系统执行顺序：显式契约（本文件是唯一权威）
//!
//! `Query` 的迭代序、bevy 的注册序、系统之间的参数依赖**都不是**本项目的顺序契约
//! （AGENTS.md §4.2.2 第 2 条）。`TickRunner` 的执行顺序**只**由下面这条阶段链决定
//! （S3a 起：与 `crates/world` 的 `tick_head` / 宿主段 / `tick_tail` oracle 段序一一对应，
//! 每段一个阶段，**仍然只有单线程 executor**）：
//!
//! ```text
//! Clock -> Command -> SimHiterSnapshot -> SimExpireHitters -> SimSettleDeaths
//!   -> SimSelfDeliver -> SimExtraAbil -> SimMapEvents -> SimSlaveCleanup -> SimBuff
//!   -> SimMonsterAi -> SimNpc -> SimMerchant -> SimPlayerRunExpiry -> SimDup
//!   -> OutputDrain -> TailSuperManRefill -> TailPlayerDeaths -> TailVerifyMapTime
//!   -> OutputAoi
//! ```
//!
//! 阶段表见 [`TickStage`] 各变体的文档与 [`STAGE_ORDER`]（唯一事实源）；段序的 oracle 依据
//! 见 `docs/world-ecs-migration-20261113.md` §5.1/§5.3。
//!
//! 契约怎么强制（三处缺一不可）：
//!
//! 1. **唯一的注册入口**是 [`TickRunner::add_systems`]，第一个参数**必须**是 [`TickStage`]，
//!    第二个参数是**单个**系统（`impl IntoSystem<(), (), _>`）；本类型**不提供**不带阶段的
//!    注册方法，也**不接受**任何 bevy 的系统配置对象（`.chain()` / `.before()` / `.after()` /
//!    `.ambiguous_with*()` 的返回类型都传不进来，waiver 没有入口）——不是注释约定，是类型上
//!    根本拿不到那一族 API。多个系统就多次调用。
//! 2. 每个系统都被 `.in_set(stage)` 收进对应阶段，阶段之间由 [`TickRunner::new`] 里的
//!    `.chain()` 固定先后；**项目里唯一的顺序维度就是阶段**。
//! 3. 构建期开启 **ambiguity detection = Error**（见 [`tick_schedule_build_settings`]）：
//!    同阶段内两个系统若有冲突访问，schedule 构建直接失败，[`TickRunner::run`] 会在跑
//!    **任何**系统之前 panic（不是 warning，也不会被 `.ambiguous_with*()` 静默豁免——该入口
//!    在类型上不存在）。
//!
//! ## 以后加系统怎么加
//!
//! * 新系统放进**已有阶段**里语义最合适的一个：`runner.add_systems(TickStage::SimBuff, sys)`；
//!   多个系统重复调用即可。
//! * **同阶段内的系统不得依赖彼此的相对顺序**（包括副作用 / I-O）：同阶段且无访问冲突的系统
//!   相对次序由 bevy 拓扑序决定，**不是**项目契约；需要顺序就把它们拆到不同阶段。
//! * 同阶段内两个系统若有数据访问冲突 ⇒ 构建期歧义检测直接让 `run` panic：必须拆阶段，
//!   不要试图用 `.chain()` / `.ambiguous_with*()` 绕过（注册入口根本不接受配置对象）。
//! * 只有确实需要新阶段时才扩 [`TickStage`]（同时改 [`STAGE_ORDER`]、[`TickRunner::new`] 里的
//!   链、本注释的阶段表）——这是**契约变更**，要在评审里单独说明；不要靠注册序或参数依赖碰运气。
//! * **守卫边界（防的是「静默新增逃逸面」）**：源码守卫扫描 `crates/world-bevy/src/**/*.rs`
//!   ——配置对象族 token（剥注释/字符串）、`include!` / `#[path` / `OUT_DIR` 包含指令
//!   （只剥注释、保留字符串）；并对 **crate 级**的 `impl TickRunner`（含子模块）与
//!   `impl <Trait> for TickRunner` 做**方法集合精确白名单**（不分可见性，当前 =
//!   `{add_systems, assert_build_contract, default, new, run}`）；这些方法的**签名不得出现
//!   裸 `Schedule`**（不得暴露 schedule 句柄）。任何新增方法/签名变化都必须显式改白名单。
//!   但**蓄意规避不在文本/结构守卫能力内**：同模块内直接操作私有字段（绕过方法入口）、
//!   宏生成代码、构建脚本注入、跨 crate re-export，均属**评审约定**——不得为 tick 顺序
//!   造 waiver 入口；新增调用面时 reviewer 必须核对。不要声称「已完全覆盖」。
//!
//! ## 为什么保留「时钟 + 线程断言」这两步
//!
//! 既有 `crates/world` 的 `tick()` 是「先校验线程 → 推进虚拟时钟 → 处理命令 → 结算 → AOI」。
//! S1 把前两步（线程校验 + 时钟推进）搬进单线程 Schedule：线程断言仍在（且现在跑在系统里），
//! 语义不变（业务逻辑可见的 `clock_ms` / `tick_seq` 逐 tick 与既有实现相同）。

use std::thread::ThreadId;

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::{LogLevel, ScheduleBuildSettings, SingleThreadedExecutor};

/// 虚拟时钟（资源）：`clock_ms` 每 tick 前进 `interval_ms`。
///
/// 与 `crates/world` 的 `World::clock_ms` / `tick_seq` 保持**同一份事实**：
/// facade 在 tick 前把值写进来，schedule 跑完再读回去（见 `World::tick`）。
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickClock {
    pub clock_ms: i64,
    pub tick_seq: u64,
    pub interval_ms: i64,
}

impl TickClock {
    pub fn new(interval_ms: i64) -> Self {
        TickClock {
            clock_ms: 0,
            tick_seq: 0,
            interval_ms,
        }
    }
}

/// 判据③的**运行期线程断言**载体：世界 tick 必须始终跑在同一个线程上。
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct TickOwnerThread(pub Option<ThreadId>);

/// 本 tick 的处理顺序（**业务 id**，槽位顺序）——由调用方的系统按 [`crate::ecs::SlotOrder`] 填入。
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct TickOrder(pub Vec<i32>);

/// tick 骨架的唯一顺序维度：系统必须注册进某个阶段。
///
/// 阶段之间的先后是**契约**（见 [`STAGE_ORDER`] 与 [`TickRunner::new`] 的 `.chain()`），
/// 不随注册序/参数依赖变化。
///
/// **S3a 契约变更**：原 4 阶段 `Clock/Command/Simulate/Output` 把 `Simulate` 展开成
/// 「oracle 每段一个阶段」的线性链（编号即 `docs/world-ecs-migration-20261113.md` §5.1
/// 现状段序表的段号）。语义没有变化——拆开只为让每段独占一个阶段，从而在
/// 「歧义检测 = Error」下仍能显式定序；业务逻辑迁入这些阶段是后续切片的事。
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TickStage {
    /// #0 线程断言 + 虚拟时钟推进（`clock_ms` / `tick_seq`）。
    Clock,
    /// #1 命令 FIFO（`World::tick_head` 的 `match cmd`；逐条、一个系统、不得拆散）。
    Command,
    /// #2 PL-1 玩家命中者快照（读清理前的 `LastHiter`）。
    SimHiterSnapshot,
    /// #3 命中者过期（`World::expire_hitters`）。
    SimExpireHitters,
    /// #4 非玩家死亡结算（`World::settle_deaths(false)`）。
    SimSettleDeaths,
    /// #5 宝宝自投递（`World::deliver_self_delay_msgs`）。
    SimSelfDeliver,
    /// #6 `ExtraAbil` 过期（`World::expire_extra_abil`）。
    SimExtraAbil,
    /// #7 地图事件（`World::run_map_events`）。
    SimMapEvents,
    /// #8 宝宝清表（`slave::slave_list_cleanup_all`）。
    SimSlaveCleanup,
    /// #9 buff（`World::buff_tick`；不抽 RNG）。
    SimBuff,
    /// #10 怪物 AI（`monai::monster_ai_tick`；`ai_enabled` 门）。
    SimMonsterAi,
    /// #11 NPC（`World::npc_tick`；必须在商人之前）。
    SimNpc,
    /// #12 商人（`World::merchant_tick`；在 NPC 之后）。
    SimMerchant,
    /// #13 玩家 Run 到期（`World::expire_player_run_state`）。
    SimPlayerRunExpiry,
    /// #14 重叠推挤（`World::check_dup_obj`；在 Run 到期之后）。
    SimDup,
    /// #15 输出转抄（`record.*` → `pending_*`）。
    OutputDrain,
    /// #16 玩家 SuperMan 回满（`Entity::run_super_man_refill`，`tick_tail` 第 0 段）。
    TailSuperManRefill,
    /// #17 玩家死亡结算（`World::settle_deaths(true)`，`tick_tail` 第 1 段）。
    TailPlayerDeaths,
    /// #18 `VerifyMapTime` + 钳制（`World::verify_players_map_time`，`tick_tail` 第 2 段）。
    TailVerifyMapTime,
    /// #19 AOI 快照（`search_view_range_*`，`tick_tail` 第 3 段）。
    OutputAoi,
}

/// 阶段顺序（唯一事实源；[`TickRunner::new`] 的 `.chain()` 必须与它一致）。
///
/// 顺序 = oracle 段序（`tick_head` → 宿主段（不在 schedule 内）→ `tick_tail`）。
/// `Tail*` 三段排在 `OutputDrain` 之后，因为它们在宿主段 P21..P32 之后执行。
pub const STAGE_ORDER: [TickStage; 20] = [
    TickStage::Clock,
    TickStage::Command,
    TickStage::SimHiterSnapshot,
    TickStage::SimExpireHitters,
    TickStage::SimSettleDeaths,
    TickStage::SimSelfDeliver,
    TickStage::SimExtraAbil,
    TickStage::SimMapEvents,
    TickStage::SimSlaveCleanup,
    TickStage::SimBuff,
    TickStage::SimMonsterAi,
    TickStage::SimNpc,
    TickStage::SimMerchant,
    TickStage::SimPlayerRunExpiry,
    TickStage::SimDup,
    TickStage::OutputDrain,
    TickStage::TailSuperManRefill,
    TickStage::TailPlayerDeaths,
    TickStage::TailVerifyMapTime,
    TickStage::OutputAoi,
];

/// tick schedule 的构建设置：**歧义即构建失败**（`LogLevel::Error`）。
///
/// 公开给契约测试：故意构造一个绕过阶段/未显式排序的冲突 schedule 时，
/// 必须能被**同一份**设置报出来（证明检测器真的接上了，而不是注释约定）。
pub fn tick_schedule_build_settings() -> ScheduleBuildSettings {
    ScheduleBuildSettings {
        ambiguity_detection: LogLevel::Error,
        ..Default::default()
    }
}

/// 时钟推进系统：线程断言（同线程）+ `clock_ms += interval` + `tick_seq += 1`。
///
/// 断言放在系统里（而非 facade），是为了让「tick 骨架」本身带确定性守卫：
/// 把 world 挪到别的线程上跑，`Schedule::run` 会在此 panic。
pub fn advance_clock_system(
    mut clock: ResMut<TickClock>,
    mut owner: ResMut<TickOwnerThread>,
    mut trace: Option<ResMut<StageTrace>>,
) {
    if let Some(t) = trace.as_deref_mut() {
        t.0.push(TickStage::Clock);
    }
    let current = std::thread::current().id();
    match owner.0 {
        None => owner.0 = Some(current),
        Some(t) => assert_eq!(
            t, current,
            "世界 tick 必须始终在同一线程上串行执行（设计文档 §4.2：禁止并行）"
        ),
    }
    clock.clock_ms += clock.interval_ms;
    clock.tick_seq += 1;
}

// ======================= S4d-3：段白名单与按段裁剪适配器 =======================
//
// 设计件：`docs/world-ecs-s4d3-entitymut-20261113.md` §2.2/§2.3/§2.4/§3.2/§3.3。
//
// 契约（编译期为主、运行期为副）：
//   * 每个段一个零大小类型并实现 [`StageSpec`]；组件白名单用 [`Reads`] / [`Writes`] 标记 trait 表达，
//     [`StageEntities`] 的方法带 `where S: Reads<C>` / `S: Writes<C>` 约束 ⇒ **越白名单是编译错误**；
//   * 结构操作（`insert` / `remove`）另需 [`Structural`] 标记 ⇒ 非结构段调用也是编译错误；
//   * **运行期镜像**（P2 前置，review P2-1）：每次取组件都核对 [`ComponentRegistry`] 里该组件的
//     短名是否在本段 `READ`/`WRITE` 清单内，越界即 panic（fail-closed）——给「标记 trait 与
//     名单表漂移」「泛型/dyn 转发」兜底；组件未注册同样 panic；
//   * 拒绝语义**逐项照抄旧 facade 的 Option**：id 不在槽位表 / 实体已移除 / 组件缺失 ⇒ `None`
//     并静默跳过；组件缺失另计 [`StageSkip::component_missing`]（诊断计数，不改变行为）。

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use bevy_ecs::query::{QueryData, QueryState};

use crate::ecs::SlotTable;

/// **段规格**：每个段一个零大小类型，声明自己的 [`TickStage`] 与组件白名单（运行期镜像）。
///
/// 结构变更的编译期门是 [`Structural`] 标记 trait（只有 Command / NPC / 商人 /
/// 宝宝清表会实现），不用常量：常量无消费者时是死配置（review P3-4）。
pub trait StageSpec: 'static {
    /// 本段在 [`STAGE_ORDER`] 里的阶段。
    const STAGE: TickStage;
    /// 本段允许**读**的组件短名（与 [`Reads`] 实现必须一致；由 `stage_spec!` 生成）。
    const READ: &'static [&'static str] = &[];
    /// 本段允许**写**的组件短名（与 [`Writes`] 实现必须一致；由 `stage_spec!` 生成）。
    const WRITE: &'static [&'static str] = &[];
}

/// 本段**允许读**的组件（标记 trait；由 `stage_spec!` 或手写 impl 生成）。
pub trait Reads<C: Component> {}

/// 本段**允许写**的组件（标记 trait；由 `stage_spec!` 或手写 impl 生成）。
pub trait Writes<C: Component<Mutability = bevy_ecs::component::Mutable>> {}

/// 本段允许结构变更（增删实体）。
pub trait Structural {}

/// `type_name` 的短名（去模块路径与泛型参数）：`mir2_world_bevy::components::Vitals` ⇒ `Vitals`。
pub fn short_type_name<C: 'static>() -> &'static str {
    let full = core::any::type_name::<C>();
    let base = full.split('<').next().unwrap_or(full);
    base.rsplit("::").next().unwrap_or(base)
}

/// **组件注册表（资源）**：`TypeId` ⇒ 组件短名（P2 前置，设计 §2.4）。
///
/// 用途有二：① 运行期镜像白名单的「名字来源」；② 「新组件忘了登记」的 fail-closed——
/// 段里取一个未注册的组件直接 panic，而不是被静默当成 `None`。
///
/// `EcsStore::new` 会插入空表；`EntityStore::new`（`crates/world`）把 25 个组件逐个登记。
#[derive(Resource, Default)]
pub struct ComponentRegistry {
    names: HashMap<TypeId, &'static str>,
}

impl ComponentRegistry {
    /// 登记一个组件类型（幂等）。
    pub fn register<C: Component>(&mut self) -> &mut Self {
        self.names.insert(TypeId::of::<C>(), short_type_name::<C>());
        self
    }

    /// 组件短名；未登记 ⇒ `None`。
    pub fn name_of<C: Component>(&self) -> Option<&'static str> {
        self.names.get(&TypeId::of::<C>()).copied()
    }

    /// 组件必须已登记，否则 panic（fail-closed）。
    pub fn assert_registered<C: Component>(&self) {
        assert!(
            self.name_of::<C>().is_some(),
            "组件 {} 未在 ComponentRegistry 注册：段适配器拒绝在未登记组件上工作（P2 前置）",
            short_type_name::<C>()
        );
    }

    /// 已登记的组件数（诊断/测试用）。
    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// **诊断计数（资源）**：本 tick 里「实体存在但组件缺失」的取组件次数（P2 前置，设计 §2.4）。
///
/// 21 个 bundle 组件对每个实体齐全，因此正常路径恒 0；非 0 即接线 bug 或半实体。
#[derive(Resource, Default)]
pub struct StageSkip {
    pub component_missing: AtomicU64,
}

impl StageSkip {
    pub fn get(&self) -> u64 {
        self.component_missing.load(Ordering::Relaxed)
    }
}

/// **按段缓存的 `QueryState`（资源）**：`view_mut` 的组合视图用（P2 前置，设计 §3.3）。
///
/// 段适配器在段开始时把本资源**移出** bevy `World` 自持，`Drop` 时放回——`QueryState::get_mut`
/// 需要同时可变借用 state 与 World，二者必须不同所有者。结构变更段要在收尾时刷新
/// archetype（P5/P6 接线，见 docs 登记表）。
#[derive(Resource, Default)]
pub struct StageQueries {
    states: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl StageQueries {
    /// 取（首次自动创建）某 `QueryData` 的缓存状态。
    fn state<Q: QueryData + 'static>(&mut self, world: &mut World) -> &mut QueryState<Q> {
        self.states
            .entry(TypeId::of::<Q>())
            .or_insert_with(|| Box::new(world.query::<Q>()))
            .downcast_mut::<QueryState<Q>>()
            .expect("StageQueries 的 TypeId 键与 QueryData 类型必须一一对应")
    }
}

/// **测试/诊断用**：本 tick 已执行的段序列（资源缺席 ⇒ 零开销、零行为）。
///
/// 用途只有一颗牙：`tick_head`/`tick_tail` 驱动真实 runner 后，断言序列等于 `STAGE_ORDER`
/// 的对应切片 —— **删掉任一段的系统注册，序列就缺一项**（设计件 §2.7.1 的统一牙）。
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct StageTrace(pub Vec<TickStage>);

impl StageTrace {
    /// 若资源存在则记录一次（不存在时是 no-op）。
    pub fn record(world: &mut World, stage: TickStage) {
        if let Some(mut trace) = world.get_resource_mut::<StageTrace>() {
            trace.0.push(stage);
        }
    }
}

/// 按段裁剪的实体适配器：以 [`SlotTable`] 资源为「id -> 实体 + 槽位序」的唯一真值。
///
/// 生命周期与借用规则（设计件 §2.2 A1/A2）：`get_mut` 借用 `&mut self`，因此同一时刻只能
/// 存在一个可写视图；跨实体协作必须「快照 -> 改副本 -> 回写」，与既有 `spawn_cloned` /
/// `write_back` 纪律一致。
pub struct StageEntities<'w, S: StageSpec> {
    world: &'w mut World,
    /// 段开始时从 bevy `World` 移出、`Drop` 时放回（`view_mut` 的 `QueryState` 缓存）。
    queries: Option<StageQueries>,
    _spec: core::marker::PhantomData<fn() -> S>,
}

impl<'w, S: StageSpec> StageEntities<'w, S> {
    pub fn new(world: &'w mut World) -> Self {
        let queries = Some(
            world
                .remove_resource::<StageQueries>()
                .expect("StageQueries 必须已由 EcsStore::new 插入（段适配器前置）"),
        );
        StageEntities {
            world,
            queries,
            _spec: core::marker::PhantomData,
        }
    }

    /// 槽位顺序的 `(业务 id, bevy 实体)` 表（每段入口取一次；结构变更后要重新取）。
    pub fn order(&self) -> Vec<(i32, Entity)> {
        self.world.resource::<SlotTable>().order()
    }

    /// id -> bevy 实体（三态之一的 **id 不在槽位表 / 槽已空** 都落到 `None`）。
    pub fn resolve(&self, id: i32) -> Option<Entity> {
        self.world.resource::<SlotTable>().entity_of(id)
    }

    /// **运行期镜像白名单**：组件必须已注册，且短名在本段对应清单里；否则 panic（fail-closed）。
    fn assert_stage_component<C: Component>(&self, write: bool) {
        let registry = self
            .world
            .get_resource::<ComponentRegistry>()
            .expect("ComponentRegistry 必须已由 EcsStore::new 插入（段适配器前置）");
        registry.assert_registered::<C>();
        let name = short_type_name::<C>();
        let allowed = if write { S::WRITE } else { S::READ };
        assert!(
            allowed.contains(&name),
            "段 {:?} 越白名单{}组件 {}：运行期镜像与编译期标记 trait 必须一致（P2 前置）",
            S::STAGE,
            if write { "写" } else { "读" },
            name
        );
    }

    /// 记一次「实体存在但组件缺失」（诊断计数；不改变 `None` 的跳过语义）。
    fn note_component_missing(&self) {
        if let Some(skip) = self.world.get_resource::<StageSkip>() {
            skip.component_missing.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 只读取组件；`None` 覆盖「id 不存在 / 已移除 / 该实体没有该组件」三种情形。
    pub fn get<C>(&self, id: i32) -> Option<&C>
    where
        C: Component,
        S: Reads<C>,
    {
        let entity = self.resolve(id)?;
        self.assert_stage_component::<C>(false);
        match self.world.get::<C>(entity) {
            Some(c) => Some(c),
            None => {
                self.note_component_missing();
                None
            }
        }
    }

    /// 可变读取组件；借用期不超过本次调用（同实体/跨实体都不可能并存两个 `&mut`）。
    pub fn get_mut<C>(&mut self, id: i32) -> Option<&mut C>
    where
        C: Component<Mutability = bevy_ecs::component::Mutable>,
        S: Writes<C>,
    {
        let entity = self.resolve(id)?;
        self.assert_stage_component::<C>(true);
        if self.world.get::<C>(entity).is_none() {
            self.note_component_missing();
            return None;
        }
        self.world.get_mut::<C>(entity).map(|c| c.into_inner())
    }

    /// **组合视图**（P2 前置，设计 §3.3）：一次借用拿到同实体上 `Q` 声明的多组件
    /// （`QueryData` 元组，可含 `&mut`）。`Q` 由段自己声明，等价于「白名单里的多字段视图」。
    ///
    /// 顺序/别名约束同 A1：视图借用期不超过本次调用；同一时刻只有一个实体视图。
    pub fn view_mut<Q>(&mut self, id: i32) -> Option<Q::Item<'_, '_>>
    where
        Q: QueryData + 'static,
    {
        let entity = self.resolve(id)?;
        let queries = self.queries.as_mut()?;
        let state = queries.state::<Q>(self.world);
        state.get_mut(self.world, entity).ok()
    }

    /// 按槽位顺序对每个实体执行 `f`（跳过取不到组件的实体，与旧 `for_each_mut` 同语义）。
    pub fn for_each_mut<C>(&mut self, mut f: impl FnMut(i32, &mut C))
    where
        C: Component<Mutability = bevy_ecs::component::Mutable>,
        S: Writes<C>,
    {
        for (id, _entity) in self.order() {
            if let Some(c) = self.get_mut::<C>(id) {
                f(id, c);
            }
        }
    }

    /// 移除整个实体（只有 [`Structural`] 段可调用）。返回 `false` = id 不存在 / 已移除。
    ///
    /// **接线责任**：`EcsStore` 的缓存 `QueryState` 在 facade 里，本适配器够不到；
    /// 结构变更段收尾时由 facade 调 `EntityStore` 的刷新入口（P5 批次落地，届时补牙）。
    pub fn remove(&mut self, id: i32) -> bool
    where
        S: Structural,
    {
        let Some((slot_idx, entity)) = self.world.resource_mut::<SlotTable>().unbind(id) else {
            return false;
        };
        self.world.despawn(entity);
        self.world
            .resource_mut::<SlotTable>()
            .release_slot(slot_idx);
        true
    }
}

impl<S: StageSpec> Drop for StageEntities<'_, S> {
    fn drop(&mut self) {
        if let Some(queries) = self.queries.take() {
            self.world.insert_resource(queries);
        }
    }
}

/// **段规格声明宏**：一次生成 `StageSpec`（含 READ/WRITE 名单）+ `Reads`/`Writes` 标记 impl。
///
/// ```text
/// stage_spec!(SimExpireHitters, TickStage::SimExpireHitters,
///     read: [CoreIdentity, LifeCycle], write: [CombatAttribution]);
/// ```
///
/// 名单用 `stringify!` 生成短名（调用处写 `Vitals` 则名单就是 `"Vitals"`），与
/// [`ComponentRegistry`] 的 `short_type_name` 同口径。泛型组件请在调用处写别名。
#[macro_export]
macro_rules! stage_spec {
    ($spec:ident, $stage:expr, read: [$($r:ty),* $(,)?], write: [$($w:ty),* $(,)?]) => {
        pub struct $spec;
        impl $crate::tick::StageSpec for $spec {
            const STAGE: $crate::tick::TickStage = $stage;
            const READ: &'static [&'static str] = &[$(stringify!($r)),*];
            const WRITE: &'static [&'static str] = &[$(stringify!($w)),*];
        }
        $(impl $crate::tick::Reads<$r> for $spec {})*
        $(impl $crate::tick::Writes<$w> for $spec {})*
    };
}

/// 单线程 tick 骨架：`Schedule` + [`SingleThreadedExecutor`] + 显式阶段顺序契约。
///
/// 只暴露「带阶段地加系统 / 跑一次」，**不提供**无阶段注册入口；也**不提供**多线程执行器入口
/// （守卫会正面断言本文件出现 `SingleThreadedExecutor`，且源码扫描禁 `MultiThreadedExecutor`）。
pub struct TickRunner {
    schedule: Schedule,
}

impl TickRunner {
    pub fn new() -> Self {
        let mut schedule = Schedule::default();
        schedule.set_executor(SingleThreadedExecutor::default());
        schedule.set_build_settings(tick_schedule_build_settings());
        // 顺序契约：阶段之间固定链式先后（与 STAGE_ORDER 逐项一致）。
        // bevy 的元组 `.chain()` 支持最多 20 项；本契约恰好 20 段（见 STAGE_ORDER）。
        schedule.configure_sets(
            (
                STAGE_ORDER[0],
                STAGE_ORDER[1],
                STAGE_ORDER[2],
                STAGE_ORDER[3],
                STAGE_ORDER[4],
                STAGE_ORDER[5],
                STAGE_ORDER[6],
                STAGE_ORDER[7],
                STAGE_ORDER[8],
                STAGE_ORDER[9],
                STAGE_ORDER[10],
                STAGE_ORDER[11],
                STAGE_ORDER[12],
                STAGE_ORDER[13],
                STAGE_ORDER[14],
                STAGE_ORDER[15],
                STAGE_ORDER[16],
                STAGE_ORDER[17],
                STAGE_ORDER[18],
                STAGE_ORDER[19],
            )
                .chain(),
        );
        TickRunner { schedule }
    }

    /// **唯一的系统注册入口**：必须显式给出 [`TickStage`]，且**一次只接受一个系统**。
    ///
    /// 参数类型刻意收窄成 `impl IntoSystem<(), (), _>`：调用方拿不到 bevy 的
    /// `IntoSystemConfigs`/`ScheduleConfigs` 那一族，因而无法挂 `.chain()` / `.before()` /
    /// `.after()` / `.ambiguous_with_all()` / `.ambiguous_with(...)`——顺序与「零歧义」都不可能
    /// 被调用方静默豁免。多个系统请多次调用本方法。
    ///
    /// 同一阶段内出现两个访问冲突的系统时，[`TickRunner::run`] 会因构建期歧义检测 panic；
    /// 请把它们拆到不同阶段（同阶段无冲突系统的相对顺序也不是契约，见文件头注释）。
    pub fn add_systems<M>(
        &mut self,
        stage: TickStage,
        system: impl IntoSystem<(), (), M>,
    ) -> &mut Self {
        self.schedule.add_systems(system.in_set(stage));
        self
    }

    /// 在调用线程上跑一遍（`SingleThreadedExecutor` 保证不开线程）。
    ///
    /// 跑之前先构建 schedule 并断言**零歧义**；歧义 = 构建错误 = 立即 panic，
    /// 不会带着不确定的顺序执行任何系统。
    pub fn run(&mut self, world: &mut World) {
        self.assert_build_contract(world);
        self.schedule.run(world);
    }

    /// 构建期契约断言：只用契约设置构一次 schedule；任何构建错误（含歧义）都直接 panic。
    fn assert_build_contract(&mut self, world: &mut World) {
        if let Err(e) = self.schedule.initialize(world) {
            panic!(
                "TickRunner 的系统顺序契约被破坏（schedule 构建失败；每个系统必须带 TickStage，阶段内冲突必须显式 chain）: {}",
                e.to_string(self.schedule.graph(), world)
            );
        }
    }
}

impl Default for TickRunner {
    fn default() -> Self {
        Self::new()
    }
}

// 线程断言的反证（“把骨架挪到别的线程上必须 panic”）放在 `tests/tick_skeleton.rs`：
// `src/` 下的源码要能被 `no_parallel.rs` 全量扫描（禁 `thread::spawn`），
// 而“开线程验证断言”这条反证需要在测试里用 `std::thread::spawn`。

#[cfg(test)]
mod stage_adapter_tests {
    use super::*;
    use crate::ecs::EcsStore;

    #[derive(Component, Debug, PartialEq, Eq)]
    struct A(i32);

    #[derive(Component, Debug, PartialEq, Eq)]
    struct B(i32);

    // 探针段：允许读写 A、B（只有测试用）。
    stage_spec!(Probe, TickStage::SimBuff, read: [A, B], write: [A, B]);

    fn store_with(ids: &[i32]) -> EcsStore<A> {
        let mut s = EcsStore::new();
        s.register_component::<A>().register_component::<B>();
        for id in ids {
            s.insert(*id, A(*id));
        }
        s
    }

    /// 负例 1：id 不在槽位表 ⇒ `None`（旧 `EntityStore::get/get_mut` 的 `index.get` 语义）。
    ///
    /// 牙：把 `resolve` 改成「找不到就取第一个实体」⇒ 该断言红。
    #[test]
    fn adapter_unknown_id_is_none() {
        let mut store = store_with(&[1, 2]);
        {
            let mut ad = StageEntities::<Probe>::new(store.world_mut());
            assert_eq!(ad.resolve(999), None);
            assert_eq!(ad.get::<A>(999), None);
            assert_eq!(ad.get_mut::<A>(999), None);
            // 不得改变槽位顺序
            assert_eq!(
                ad.order(),
                vec![(1, ad.resolve(1).unwrap()), (2, ad.resolve(2).unwrap())]
            );
        }
        assert_eq!(store.order_index(), vec![1, 2]);
    }

    /// 负例 2：实体已移除 ⇒ `None`（槽位 `entity = None` 的旧语义）。
    ///
    /// 牙：把 `resolve` 改成只查 `index` 不查槽位的 `entity` ⇒ 移除后仍返回旧句柄 ⇒ 红。
    #[test]
    fn adapter_removed_id_is_none() {
        let mut store = store_with(&[7]);
        assert!(store.remove(7).is_some());
        let mut ad = StageEntities::<Probe>::new(store.world_mut());
        assert_eq!(ad.resolve(7), None);
        assert_eq!(ad.get_mut::<A>(7), None);
        assert_eq!(ad.get::<A>(7), None);
    }

    /// 负例 3：槽位复用不得串号（新实体复用旧槽位后，**旧 id 仍是 `None`**）。
    ///
    /// 牙：把 `resolve` 改成「按槽位下标返回 `slots[slot].entity` 而不校验 id」⇒ 旧 id 会拿到
    /// 新实体的组件 ⇒ 红。
    #[test]
    fn adapter_slot_reuse_does_not_alias_ids() {
        let mut store = store_with(&[7]);
        assert!(store.remove(7).is_some());
        store.insert(8, A(80)); // LIFO 复用 7 的槽位
        assert_eq!(store.order_index(), vec![8], "复用槽位后槽位序只有一个实体");
        let mut ad = StageEntities::<Probe>::new(store.world_mut());
        assert_eq!(ad.resolve(7), None, "旧 id 不得解析到复用槽位的新实体");
        assert_eq!(ad.get_mut::<A>(7), None);
        assert_eq!(ad.get_mut::<A>(8).map(|a| a.0), Some(80));
    }

    /// 负例 4：组件缺失 ⇒ `None`；`for_each_mut` 跳过该实体（旧 `filter_map` / `mut_view` 语义）。
    ///
    /// 牙：把 `get` 改成「缺组件就返回默认值」⇒ 第二次断言红；把 `for_each_mut` 的跳过改成
    /// 直接 `unwrap` ⇒ 该用例 panic（红）。
    #[test]
    fn adapter_missing_component_is_none_and_skipped() {
        let mut store = store_with(&[1, 2]);
        assert!(store.insert_component(1, B(11)));
        let mut ad = StageEntities::<Probe>::new(store.world_mut());
        assert_eq!(ad.get::<B>(1), Some(&B(11)));
        assert_eq!(ad.get::<B>(2), None, "2 号没有 B 组件");
        assert_eq!(ad.get_mut::<B>(2), None);
        let mut seen = Vec::new();
        ad.for_each_mut::<B>(|id, b| {
            seen.push(id);
            b.0 += 1;
        });
        assert_eq!(seen, vec![1], "只有挂了 B 的实体被访问");
        assert_eq!(ad.get::<B>(1), Some(&B(12)));
    }

    // ---------------- P2 前置：运行期镜像 / 未注册 / StageSkip / view_mut ----------------

    /// T-neg-1：**标记 trait 与运行期名单漂移**（`Writes<B>` 有、`WRITE` 名单漏 B）⇒ 取组件
    /// 必须 panic（fail-closed）。
    ///
    /// 牙：删掉 `get_mut` 里的 `assert_stage_component::<C>(true)` ⇒ 不再 panic ⇒ 红。
    struct Drift;
    impl StageSpec for Drift {
        const STAGE: TickStage = TickStage::SimBuff;
        const READ: &'static [&'static str] = &["A"];
        const WRITE: &'static [&'static str] = &["A"];
    }
    impl Writes<A> for Drift {}
    impl Writes<B> for Drift {}
    impl Reads<B> for Drift {}

    #[test]
    #[should_panic(expected = "越白名单写组件 B")]
    fn t_neg_1_mirror_catches_whitelist_drift() {
        let mut store = store_with(&[1]);
        assert!(store.insert_component(1, B(1)));
        let mut ad = StageEntities::<Drift>::new(store.world_mut());
        let _ = ad.get_mut::<B>(1);
    }

    /// T-neg-2：组件**未在 ComponentRegistry 注册** ⇒ 取组件必须 panic（fail-closed）。
    ///
    /// 牙：删掉 `ComponentRegistry::assert_registered` ⇒ 未注册不再 panic ⇒ 红。
    #[derive(Component, Debug, PartialEq, Eq)]
    struct Unregistered(i32);
    struct Unreg;
    impl StageSpec for Unreg {
        const STAGE: TickStage = TickStage::SimBuff;
        const READ: &'static [&'static str] = &["Unregistered"];
    }
    impl Reads<Unregistered> for Unreg {}

    #[test]
    #[should_panic(expected = "未在 ComponentRegistry 注册")]
    fn t_neg_2_unregistered_component_panics() {
        let mut store = store_with(&[1]);
        assert!(store.insert_component(1, Unregistered(1)));
        let ad = StageEntities::<Unreg>::new(store.world_mut());
        let _ = ad.get::<Unregistered>(1);
    }

    /// `StageSkip::ComponentMissing`：实体存在但组件缺失 ⇒ 计数 +1；行为仍是 `None`/跳过。
    ///
    /// 牙：删掉 `note_component_missing()` 的两处调用 ⇒ 计数停在 0 ⇒ 红。
    #[test]
    fn stage_skip_counts_missing_component() {
        let mut store = store_with(&[1, 2]);
        assert!(store.insert_component(1, B(11)));
        {
            let mut ad = StageEntities::<Probe>::new(store.world_mut());
            assert_eq!(ad.get::<B>(2), None);
            let mut seen = Vec::new();
            ad.for_each_mut::<B>(|id, _| seen.push(id));
            assert_eq!(seen, vec![1]);
        }
        assert_eq!(
            store.world().resource::<StageSkip>().get(),
            2,
            "get(2) 与 for_each_mut 里的 get_mut(2) 各记一次缺失"
        );
    }

    /// `view_mut`：一次借用拿到同实体上的组合视图（`&mut A` + `&B`），且 `QueryState` 缓存可复用。
    ///
    /// 牙：把 `view_mut` 的 `state.get_mut(...)` 改成 `None`（或让 `StageQueries` 不落位）⇒ 红。
    #[derive(bevy_ecs::query::QueryData)]
    #[query_data(mutable)]
    struct AbView {
        a: &'static mut A,
        b: &'static B,
    }

    #[test]
    fn view_mut_reads_and_writes_combined_components() {
        let mut store = store_with(&[1]);
        assert!(store.insert_component(1, B(5)));
        {
            let mut ad = StageEntities::<Probe>::new(store.world_mut());
            {
                let mut v = ad.view_mut::<AbView>(1).expect("组合视图");
                v.a.0 += v.b.0;
            }
            // 第二次仍可用（QueryState 已缓存、资源在 Drop 前自持）
            let v2 = ad.view_mut::<AbView>(1).expect("组合视图（缓存）");
            assert_eq!(v2.a.0, 6);
            assert_eq!(v2.b.0, 5);
            assert!(ad.view_mut::<AbView>(999).is_none(), "未知 id ⇒ None");
        }
        assert_eq!(store.get(1).map(|a| a.0), Some(6));
    }
}
