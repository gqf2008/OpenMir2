//! S1 tick 骨架门禁：单线程推进 + 换线程 panic + **系统顺序显式契约**。
//!
//! 放在 `tests/`（不在 `src/`）：`no_parallel.rs`/`bevy_drop_parity.rs` 的源码扫描会禁
//! `std::thread::spawn`，而「把骨架挪到别的线程上必须 panic」这条反证需要开线程。

use std::path::{Path, PathBuf};

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

// ==================== 顺序契约源码守卫（F4：整文件扫描） ====================

fn strip_comments_and_strings(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            // 字符串字面量：整体丢掉（含转义）
            out.push(' ');
            let mut escaped = false;
            for sc in chars.by_ref() {
                if escaped {
                    escaped = false;
                } else if sc == '\\' {
                    escaped = true;
                } else if sc == '"' {
                    break;
                } else if sc == '\n' {
                    out.push('\n');
                }
            }
        } else if c == '/' && chars.peek() == Some(&'/') {
            for sc in chars.by_ref() {
                if sc == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut prev = '\0';
            for sc in chars.by_ref() {
                if sc == '\n' {
                    out.push('\n');
                }
                if prev == '*' && sc == '/' {
                    break;
                }
                prev = sc;
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 契约禁止出现的标识符：配置对象族（可挂 `.ambiguous_with*()` / `.chain()`）
/// 与歧义豁免入口。扫描域是**整个 `crates/world-bevy/src/**/*.rs` 的代码**
/// （剥注释/字符串，但**保留 `use` 行**）——别名可以挪到 `lib.rs` 或其它同 crate 文件，
/// 只扫 tick.rs 会被绕过（reviewer F6）。
const BANNED_CONTRACT_TOKENS: [&str; 5] = [
    "IntoScheduleConfigs",
    "ScheduleConfigs",
    "IntoSystemConfigs",
    "ambiguous_with",
    "ignore_ambiguity",
];

/// 收集 `src/` 下所有 `.rs`（递归）：扫描域是整个 crate 的源码，不只是 tick.rs。
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// 读入 `src/**/*.rs`（按路径排序，保证判定确定性）。
fn src_documents() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "应能收集到 src/ 下的源文件");
    files
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p).unwrap();
            (p.display().to_string(), text)
        })
        .collect()
}

fn tick_source(files: &[(String, String)]) -> &str {
    files
        .iter()
        .find(|(p, _)| p.ends_with("tick.rs"))
        .map(|(_, s)| s.as_str())
        .expect("src/tick.rs 必须存在")
}

/// 取 `pub fn add_systems` 的签名区间；用**词边界**匹配，避免 F5 的
/// `add_systems_unchecked` 前缀误匹配。
fn narrow_add_systems_signature(code: &str) -> Option<&str> {
    let name = "pub fn add_systems";
    let mut from = 0usize;
    while let Some(rel) = code[from..].find(name) {
        let start = from + rel;
        let after = code.as_bytes().get(start + name.len()).copied();
        // 名字后面不能紧跟标识符字符（否则是 add_systems_unchecked 之类的另一入口）。
        let boundary = !matches!(after, Some(b) if b.is_ascii_alphanumeric() || b == b'_');
        if boundary {
            let rest = &code[start..];
            let end = rest.find('{').unwrap_or(rest.len());
            return Some(&rest[..end]);
        }
        from = start + name.len();
    }
    None
}

/// 返回契约违规项（空 = 绿色）。正例与三条反例共用同一判定。
///
/// * `files`：**整个 crate 的 src 文档**，逐个剥注释/字符串后禁 banned token
///   （别名/再导出放在 lib.rs 也逃不掉）；
/// * `entry_src`：仅 tick.rs，额外断言 `add_systems` 是单系统签名（入口契约）。
fn contract_violations(files: &[(String, String)], entry_src: Option<&str>) -> Vec<String> {
    let mut violations = Vec::new();
    for (name, src) in files {
        let code = strip_comments_and_strings(src);
        for token in BANNED_CONTRACT_TOKENS {
            if code.contains(token) {
                violations.push(format!("{name}: 出现被禁 token `{token}`"));
            }
        }
    }
    if let Some(src) = entry_src {
        let code = strip_comments_and_strings(src);
        match narrow_add_systems_signature(&code) {
            Some(sig) if sig.contains("impl IntoSystem<(), (), M>") => {}
            Some(sig) => violations.push(format!(
                "tick.rs add_systems 必须是单系统签名（impl IntoSystem<(), (), M>），实际: {sig}"
            )),
            None => violations.push("tick.rs 缺少 `pub fn add_systems` 入口".to_string()),
        }
    }
    violations
}

/// 正例：整个 `world-bevy/src/**/*.rs` 必须零违规（剥注释后扫描；入口签名只查 tick.rs）。
#[test]
fn contract_rejects_schedule_config_escape_hatch() {
    let files = src_documents();
    let tick = tick_source(&files);
    let violations = contract_violations(&files, Some(tick));
    assert!(
        violations.is_empty(),
        "world-bevy 源码违反顺序契约: {violations:?}"
    );
}

/// 反例①（F4 绕过①）：在原入口之后新增第二个接受配置对象的方法 ⇒ 守卫必须红。
#[test]
fn redcheck_second_config_object_method_is_rejected() {
    let mut files = src_documents();
    {
        let tick = files
            .iter_mut()
            .find(|(p, _)| p.ends_with("tick.rs"))
            .expect("src/tick.rs 必须存在");
        tick.1.push_str("pub fn add_systems_unchecked<M>(&mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) -> &mut Self { self }");
    }
    let tick = tick_source(&files).to_string();
    let violations = contract_violations(&files, Some(&tick));
    assert!(
        violations.iter().any(|v| v.contains("IntoScheduleConfigs")),
        "新增的配置对象方法未被守卫发现: {violations:?}"
    );
}

/// 反例②（F6 绕过）：别名放在 **lib.rs**，tick.rs 里用 `crate::Cfg`。
/// tick.rs 内不出现任何 banned token，但整 crate 扫描必须在 lib.rs 命中原始类型名。
#[test]
fn redcheck_alias_in_lib_rs_used_from_tick_is_rejected() {
    let tick = "pub fn add_systems<M>(&mut self, stage: TickStage, systems: impl crate::Cfg<bevy_ecs::system::ScheduleSystem, M>) -> &mut Self { self }";
    let files = vec![
        (
            "lib.rs".to_string(),
            "pub use bevy_ecs::schedule::IntoScheduleConfigs as Cfg;".to_string(),
        ),
        ("tick.rs".to_string(), tick.to_string()),
    ];
    let violations = contract_violations(&files, Some(tick));
    assert!(
        violations
            .iter()
            .any(|v| v.contains("lib.rs") && v.contains("IntoScheduleConfigs")),
        "lib.rs 里的别名未被整 crate 扫描发现: {violations:?}"
    );
}

/// 反例②b（F6 变体）：lib.rs 直接 `pub use ...IntoScheduleConfigs...` ⇒ 必须红。
#[test]
fn redcheck_direct_reexport_of_config_type_is_rejected() {
    let tick = "pub fn add_systems<M>(&mut self, stage: TickStage, system: impl IntoSystem<(), (), M>) -> &mut Self { self }";
    let files = vec![
        (
            "lib.rs".to_string(),
            "pub use bevy_ecs::schedule::IntoScheduleConfigs;".to_string(),
        ),
        ("tick.rs".to_string(), tick.to_string()),
    ];
    let violations = contract_violations(&files, Some(tick));
    assert!(
        violations.iter().any(|v| v.contains("IntoScheduleConfigs")),
        "lib.rs 的直接再导出未被发现: {violations:?}"
    );
}

/// 反例③（误报检查）：正常代码必须绿；注释/字符串里的禁用 token 不得误报。
#[test]
fn redcheck_clean_source_passes_and_comments_do_not_false_positive() {
    let tick = "// IntoScheduleConfigs ambiguous_with ignore_ambiguity in comment
pub fn add_systems<M>(&mut self, stage: TickStage, system: impl IntoSystem<(), (), M>) -> &mut Self { self }";
    let files = vec![
        (
            "lib.rs".to_string(),
            "pub use crate::tick::TickStage;".to_string(),
        ),
        ("tick.rs".to_string(), tick.to_string()),
    ];
    let violations = contract_violations(&files, Some(tick));
    assert!(violations.is_empty(), "正常代码/注释被误判: {violations:?}");
}

/// F5 回归：`add_systems_unchecked` 不得被当成 `add_systems` 入口的前缀匹配。
#[test]
fn redcheck_prefix_named_method_is_not_the_entry() {
    let only_unchecked = "pub fn add_systems_unchecked<M>(&mut self) -> &mut Self { self }";
    assert!(
        narrow_add_systems_signature(&strip_comments_and_strings(only_unchecked)).is_none(),
        "add_systems_unchecked 不得被当成 add_systems 入口"
    );
    let unchecked_first = "pub fn add_systems_unchecked<M>(&mut self) -> &mut Self { self }
pub fn add_systems<M>(&mut self, stage: TickStage, system: impl IntoSystem<(), (), M>) -> &mut Self { self }";
    let code = strip_comments_and_strings(unchecked_first);
    let sig = narrow_add_systems_signature(&code).expect("应找到真正的 add_systems");
    assert!(
        sig.contains("impl IntoSystem<(), (), M>"),
        "误取了前缀同名方法: {sig}"
    );
}
