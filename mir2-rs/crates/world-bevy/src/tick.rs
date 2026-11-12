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
pub fn advance_clock_system(mut clock: ResMut<TickClock>, mut owner: ResMut<TickOwnerThread>) {
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
