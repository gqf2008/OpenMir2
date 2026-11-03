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
//! * 新系统放进**已有阶段**里语义最合适的一个：`runner.add_systems(TickStage::Simulate, sys)`；
//!   多个系统重复调用即可。
//! * **同阶段内的系统不得依赖彼此的相对顺序**（包括副作用 / I-O）：同阶段且无访问冲突的系统
//!   相对次序由 bevy 拓扑序决定，**不是**项目契约；需要顺序就把它们拆到不同阶段。
//! * 同阶段内两个系统若有数据访问冲突 ⇒ 构建期歧义检测直接让 `run` panic：必须拆阶段，
//!   不要试图用 `.chain()` / `.ambiguous_with*()` 绕过（注册入口根本不接受配置对象）。
//! * 只有确实需要新阶段时才扩 [`TickStage`]（同时改 [`STAGE_ORDER`]、[`TickRunner::new`] 里的
//!   链、本注释的阶段表）——这是**契约变更**，要在评审里单独说明；不要靠注册序或参数依赖碰运气。
//! * **守卫边界（防的是「静默新增逃逸面」）**：源码守卫扫描 `crates/world-bevy/src/**/*.rs`
//!   ——配置对象族 token（剥注释/字符串）与 `include!` / `#[path` / `OUT_DIR` 包含指令
//!   （只剥注释、保留字符串）；并对 `impl TickRunner` 的**公开方法集合做精确白名单**
//!   （当前 = `{new, add_systems, run}`）——**任何新增公开方法都必须显式改白名单**，否则守卫红。
//!   但**蓄意规避不在文本守卫能力内**：跨 crate 再导出（配置对象族由别的 crate
//!   `pub use ... as X` 提供）、宏生成代码、构建脚本自定义注入，均属**评审约定**——
//!   不得为 tick 顺序造 waiver 入口；新增调用面时 reviewer 必须核对。不要声称「已完全覆盖」。
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
