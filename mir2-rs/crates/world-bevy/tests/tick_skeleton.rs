//! S1 tick 骨架门禁：单线程推进 + 换线程 panic + **系统顺序显式契约**。
//!
//! 放在 `tests/`（不在 `src/`）：`no_parallel.rs`/`bevy_drop_parity.rs` 的源码扫描会禁
//! `std::thread::spawn`，而「把骨架挪到别的线程上必须 panic」这条反证需要开线程。

use bevy_ecs::prelude::{ResMut, Resource, World};
use bevy_ecs::schedule::{Schedule, SingleThreadedExecutor};
use mir2_world_bevy::{
    advance_clock_system, tick_schedule_build_settings, TickClock, TickOwnerThread, TickRunner,
    TickStage,
};

fn pinned_runner() -> TickRunner {
    let mut runner = TickRunner::new();
    runner.add_systems(TickStage::Clock, advance_clock_system);
    runner
}

#[test]
fn clock_advances_and_thread_is_pinned() {
    let mut world = World::new();
    world.insert_resource(TickClock::new(200));
    world.insert_resource(TickOwnerThread::default());
    let mut runner = pinned_runner();

    runner.run(&mut world);
    runner.run(&mut world);
    let clock = *world.resource::<TickClock>();
    assert_eq!(clock.clock_ms, 400);
    assert_eq!(clock.tick_seq, 2);

    // 换线程 ⇒ 断言必然触发（catch_unwind 需要 UnwindSafe，用 AssertUnwindSafe 包住）
    let mut moved_world = world;
    let hit = std::thread::spawn(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pinned_runner().run(&mut moved_world);
        }))
        .is_err()
    })
    .join()
    .unwrap();
    assert!(hit, "tick 骨架在别的线程上必须 panic");
}

// ======================= A2：唯一注册入口必须携带阶段 =======================

#[derive(Resource, Default)]
struct Trace(Vec<&'static str>);

fn probe_clock(mut t: ResMut<Trace>) {
    t.0.push("clock");
}

fn probe_output(mut t: ResMut<Trace>) {
    t.0.push("output");
}

/// 阶段顺序覆盖注册顺序：故意**反着**注册（先 Output 后 Clock）。
///
/// 若阶段契约失效（退回按注册序跑），结果是 `["output", "clock"]`；
/// 若阶段存在但链没接上，两个系统都写 `Trace` 会构成歧义，`TickRunner::run` 直接 panic。
/// 两条路都会让本用例红——这就是 A2 的牙。
#[test]
fn stage_order_overrides_registration_order() {
    let mut world = World::new();
    world.insert_resource(Trace::default());

    let mut runner = TickRunner::new();
    runner.add_systems(TickStage::Output, probe_output);
    runner.add_systems(TickStage::Clock, probe_clock);

    runner.run(&mut world);
    assert_eq!(
        world.resource::<Trace>().0,
        vec!["clock", "output"],
        "执行顺序必须由 TickStage 阶段链决定，而不是注册序"
    );
}

/// 绕过阶段契约必须失败：直接构造裸 `Schedule`（`TickRunner` 不提供此入口），
/// 用**同一份**契约构建设置加入两个冲突且未显式排序的系统 ⇒ 构建期歧义检测报错。
///
/// 这就是「没有不带 ordering 的注册入口」的运行期牙：任何绕过阶段/chain 的冲突顺序
/// 都不可能静默通过。
#[test]
fn bypassing_stage_registration_is_rejected() {
    fn write_a(mut t: ResMut<Trace>) {
        t.0.push("a");
    }
    fn write_b(mut t: ResMut<Trace>) {
        t.0.push("b");
    }

    let mut schedule = Schedule::default();
    schedule.set_executor(SingleThreadedExecutor::default());
    schedule.set_build_settings(tick_schedule_build_settings());
    schedule.add_systems((write_a, write_b));

    let mut world = World::new();
    world.insert_resource(Trace::default());
    let err = match schedule.initialize(&mut world) {
        Err(e) => e,
        Ok(_) => panic!("绕过阶段/显式排序的冲突系统必须被歧义检测拦下，实际构建成功"),
    };
    let err_str = format!("{err:?}");
    assert!(
        err_str.contains("Ambiguity"),
        "构建失败必须来自歧义检测（Ambiguity），实际: {err_str}"
    );
}

/// A3：`TickRunner` 自己断言零歧义——同阶段内两个冲突系统未 chain ⇒ run 立即 panic。
#[test]
fn tick_runner_asserts_zero_ambiguity() {
    fn write_a(mut t: ResMut<Trace>) {
        t.0.push("a");
    }
    fn write_b(mut t: ResMut<Trace>) {
        t.0.push("b");
    }

    let mut world = World::new();
    world.insert_resource(Trace::default());
    let mut runner = TickRunner::new();
    // 同阶段 + 冲突访问 + 未显式排序：契约要求 run 在跑任何系统前 panic。
    // （注册入口一次只收一个系统；两系统要分开调用。）
    runner.add_systems(TickStage::Simulate, write_a);
    runner.add_systems(TickStage::Simulate, write_b);

    let hit = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner.run(&mut world)));
    let payload = hit.expect_err("TickRunner 必须对歧义 schedule panic");
    let msg = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_default();
    assert!(
        msg.contains("TickRunner") && msg.contains("契约"),
        "panic 文案必须指出系统顺序契约被破坏，实际: {msg}"
    );
}

/// A2（F1 收口）：注册入口签名必须是**单个** `IntoSystem`，不得退回可挂
/// `.ambiguous_with*()` / `.chain()` 的配置对象族（`IntoScheduleConfigs` /
/// `ScheduleConfigs` / `IntoSystemConfigs`）。
///
/// 这是 F1 的**持久牙**：把 `add_systems` 的参数类型放宽回配置对象 ⇒ 本用例红。
/// 类型层面的证据（传 `.ambiguous_with_all()` 直接编译失败）见 patch body 的临时
/// `cargo check` 记录——编译失败无法写成运行时用例，故这里用签名扫描兜住回退。
#[test]
fn contract_rejects_schedule_config_escape_hatch() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tick.rs"),
    )
    .unwrap();
    let start = src
        .find("pub fn add_systems")
        .expect("tick.rs 必须定义 add_systems");
    let tail = &src[start..];
    let sig_end = tail.find('{').expect("add_systems 必须有函数体");
    let sig = &tail[..sig_end];
    assert!(
        sig.contains("impl IntoSystem<(), (), M>"),
        "add_systems 必须只接受单个系统（impl IntoSystem<(), (), M>），实际签名: {sig}"
    );
    for banned in [
        "IntoScheduleConfigs",
        "ScheduleConfigs",
        "IntoSystemConfigs",
    ] {
        assert!(
            !sig.contains(banned),
            "add_systems 不得接受 {banned} 配置对象（否则 .ambiguous_with*() 可绕过零歧义检测）: {sig}"
        );
    }
}
