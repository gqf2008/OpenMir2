//! S1：**单线程 tick 骨架**（`crates/world` 的世界 tick 迁入 `bevy_ecs` 的第一批）。
//!
//! 形态：[`TickRunner`] 包一个 [`Schedule`]，**显式**装上
//! [`SingleThreadedExecutor`]——世界 tick 仍由调用方（`crates/world` 的 `World::tick`）驱动，
//! 只是时钟推进/顺序收集等骨架步骤改由 ECS 系统完成。
//!
//! ## 为什么保留「时钟 + 线程断言」这两步
//!
//! 既有 `crates/world` 的 `tick()` 是「先校验线程 → 推进虚拟时钟 → 处理命令 → 结算 → AOI」。
//! S1 把前两步（线程校验 + 时钟推进）搬进单线程 Schedule：线程断言仍在（且现在跑在系统里），
//! 语义不变（业务逻辑可见的 `clock_ms` / `tick_seq` 逐 tick 与既有实现相同）。

use std::thread::ThreadId;

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::SingleThreadedExecutor;

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

/// 单线程 tick 骨架：`Schedule` + [`SingleThreadedExecutor`]。
///
/// 只暴露「加系统 / 跑一次」，**不提供**多线程执行器入口（守卫会正面断言本文件出现
/// `SingleThreadedExecutor`，且源码扫描禁 `MultiThreadedExecutor`）。
pub struct TickRunner {
    schedule: Schedule,
}

impl TickRunner {
    pub fn new() -> Self {
        let mut schedule = Schedule::default();
        schedule.set_executor(SingleThreadedExecutor::default());
        TickRunner { schedule }
    }

    /// 往骨架里挂系统（顺序由系统自身的参数依赖决定；没有依赖时按注册序）。
    pub fn add_systems<M>(
        &mut self,
        systems: impl IntoScheduleConfigs<bevy_ecs::system::ScheduleSystem, M>,
    ) -> &mut Self {
        self.schedule.add_systems(systems);
        self
    }

    /// 在调用线程上跑一遍（`SingleThreadedExecutor` 保证不开线程）。
    pub fn run(&mut self, world: &mut World) {
        self.schedule.run(world);
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
