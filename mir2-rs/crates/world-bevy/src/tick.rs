//! S1：**单线程 tick 骨架**（`crates/world` 的世界 tick 迁入 `bevy_ecs` 的第一批）。
//!
//! 形态：[`TickRunner`] 包一个 [`Schedule`]，**显式**装上
//! [`SingleThreadedExecutor`]——世界 tick 仍由调用方（`crates/world` 的 `World::tick`）驱动，
//! 只是时钟推进/顺序收集等骨架步骤改由 ECS 系统完成。
//!
//! ## 系统执行顺序：显式契约（本文件是唯一权威）
//!
//! `Query` 的迭代序、bevy 的注册序、系统之间的参数依赖**都不是**本项目的顺序契约
//! （AGENTS.md §4.2.2 第 2 条）。`TickRunner` 的执行顺序**只**由下面这条阶段链决定：
//!
//! ```text
//! Clock -> Command -> Simulate -> Output
//! ```
//!
//! * [`TickStage::Clock`]：线程断言 + 虚拟时钟推进（`clock_ms` / `tick_seq`）；
//! * [`TickStage::Command`]：处理玩家输入/命令（第 2 批）；
//! * [`TickStage::Simulate`]：结算/战斗/AI 等世界模拟（第 2 批）；
//! * [`TickStage::Output`]：AOI 快照与输出命令（第 2 批）。
//!
//! 契约怎么强制（三处缺一不可）：
//!
//! 1. **唯一的注册入口**是 [`TickRunner::add_systems`]，第一个参数**必须**是 [`TickStage`]；
//!    本类型**不提供**不带阶段的注册方法（不是注释约定，是 API 上不存在这个入口）。
//! 2. 每个系统都被 `.in_set(stage)` 收进对应阶段，阶段之间由 [`TickRunner::new`] 里的
//!    `.chain()` 固定先后。
//! 3. 构建期开启 **ambiguity detection = Error**（见 [`tick_schedule_build_settings`]）：
//!    同阶段内两个系统若有冲突访问却未被 `.chain()` 显式排序，schedule 构建直接失败，
//!    [`TickRunner::run`] 会在跑**任何**系统之前 panic（不是 warning，不会被静默放过）。
//!
//! ## 以后加系统怎么加
//!
//! * 新系统放进**已有阶段**里语义最合适的一个：`runner.add_systems(TickStage::Simulate, sys)`。
//! * 同一阶段内多个系统若有数据访问冲突，必须显式排序：`(a, b).chain()` 或 `a.before(b)`。
//! * 只有确实需要新阶段时才扩 [`TickStage`]（同时改 [`STAGE_ORDER`]、[`TickRunner::new`] 里的
//!   链、本注释的阶段表）——这是**契约变更**，要在评审里单独说明；不要靠注册序或参数依赖碰运气。
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
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TickStage {
    /// 线程断言 + 虚拟时钟推进。
    Clock,
    /// 玩家输入/命令处理。
    Command,
    /// 结算/战斗/AI 等世界模拟。
    Simulate,
    /// AOI 快照 + 输出命令。
    Output,
}

/// 阶段顺序（唯一事实源；[`TickRunner::new`] 的 `.chain()` 必须与它一致）。
pub const STAGE_ORDER: [TickStage; 4] = [
    TickStage::Clock,
    TickStage::Command,
    TickStage::Simulate,
    TickStage::Output,
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
        schedule.configure_sets(
            (
                STAGE_ORDER[0],
                STAGE_ORDER[1],
                STAGE_ORDER[2],
                STAGE_ORDER[3],
            )
                .chain(),
        );
        TickRunner { schedule }
    }

    /// **唯一的系统注册入口**：必须显式给出 [`TickStage`]。
    ///
    /// 阶段内的多个系统若访问冲突，调用方必须再显式排序（如 `(a, b).chain()`），
    /// 否则 [`TickRunner::run`] 会因构建期歧义检测 panic——顺序**不会**静默地按注册序发生。
    pub fn add_systems<M>(
        &mut self,
        stage: TickStage,
        systems: impl IntoScheduleConfigs<bevy_ecs::system::ScheduleSystem, M>,
    ) -> &mut Self {
        self.schedule.add_systems(systems.in_set(stage));
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
