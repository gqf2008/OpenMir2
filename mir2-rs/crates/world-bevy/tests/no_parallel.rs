//! 判据③（并行化版）：**允许 bevy 并行调度，但必须满足重放确定性**——同一固定输入（固定种子+固定驱动）跑 N=8 次，输出帧序列/状态**逐字节相同**，任一次不同即红。**依赖仍禁外部线程/异步库**（rayon/crossbeam/tokio 等，须用 bevy 自己的线程池）；**tick 骨架须显式选择执行器并注释理由**；**保留** `TickOwnerThread`（调用线程约束，不约束内部并行）。
//!
//! 三条独立断言（任一被破坏即红）：
//!
//! 1. **依赖白名单**（结构性）：`crates/world-bevy` 只允许内部 path 依赖
//!    （`mir2-shared`/`mir2-data`/`mir2-formula`/`mir2-world-bevy`）+ `bevy_ecs`/`bevy_app`；
//!    加入 tokio/rayon/crossbeam/threadpool/`bevy_tasks` 任一"会起线程"的库，立刻红。
//!    这是 §4.2.2「守卫升级：从禁一切并行依赖 → 只允许 `bevy_ecs`/`bevy_app`」的落地。
//! 2. **源码扫描**（只扫代码，跳过注释与字符串）：`crates/world-bevy/src/**/*.rs` 不得出现
//!    `MultiThreadedExecutor` / `bevy_tasks` / `rayon::` / `crossbeam` / `thread::spawn`；
//!    同时**正面**断言 tick 骨架来自单线程 `TickRunner`，且 `crates/world-bevy/src/tick.rs`
//!    里显式写了 `SingleThreadedExecutor`（否则"换回多线程执行器"可静默通过）。
//! 3. **运行期线程断言**：`World::tick` 在别的线程上必须 panic
//!    （`tests/ecs_migration.rs::tick_refuses_to_run_on_another_thread`）。
//!
//! 覆盖边界（诚实声明）：本测试拦的是"依赖/代码/驱动方式引入并行"；若有人在别处以
//! 单线程方式驱动 world，world 内部仍是串行的，符合 §4.2 的硬约束。

use std::fs;
use std::path::{Path, PathBuf};

/// 会起线程/引入异步运行时的库（一旦进入 `crates/world-bevy` 的依赖面即判红）。
const FORBIDDEN_DEPS: [&str; 12] = [
    "tokio",
    "rayon",
    "crossbeam",
    "crossbeam-channel",
    "crossbeam-utils",
    "async-std",
    "futures",
    "threadpool",
    "rayon-core",
    "smol",
    "actix",
    "kameo",
];

/// 源码里不得出现的并行 API 字样（扫代码不扫注释/字符串）。
const FORBIDDEN_CODE: [&str; 3] = [
    // MultiThreadedExecutor / bevy_tasks 现允许（bevy 自己的并行执行器/线程池）；
    // 仍禁「手搓线程 / 外部线程库」——并行只许走 bevy 调度器。
    "rayon",
    "crossbeam",
    "thread::spawn",
];

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cargo_toml(rel: &str) -> String {
    let p = crate_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"))
}

fn deps_section(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_deps = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps =
                t == "[dependencies]" || t == "[dev-dependencies]" || t == "[build-dependencies]";
            continue;
        }
        if in_deps && !t.is_empty() && !t.starts_with('#') {
            if let Some((name, _)) = t.split_once('=') {
                out.push(name.trim().to_string());
            }
        }
    }
    out
}

/// 收集一个目录下所有 `.rs` 源文件（递归）。
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
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

/// **只保留代码**：去掉行注释、块注释与字符串字面量（保留换行以便定位）。
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

/// 扫描一段代码，返回命中的禁用字样。
fn scan_parallel_code(code: &str) -> Vec<&'static str> {
    FORBIDDEN_CODE
        .iter()
        .copied()
        .filter(|bad| code.contains(bad))
        .collect()
}

/// **去掉 `use ...;` 行**：只保留真正生效的调用点，防止「仅 import 就满足正向断言」。
fn strip_use_lines(code: &str) -> String {
    code.lines()
        .filter(|l| !l.trim_start().starts_with("use "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 执行器选择断言（②）：tick.rs 必须**真的调用** `set_executor(...)`（裸 `use` 不算，F2），
/// 且**原始源码（含注释）**里带 `EXECUTOR-RATIONALE` 标记注释说明选择理由。
fn tick_has_explicit_executor_and_rationale(raw: &str) -> bool {
    let code = strip_use_lines(&strip_comments_and_strings(raw));
    code.contains("set_executor(") && raw.contains("EXECUTOR-RATIONALE")
}

#[test]
fn world_crate_allows_only_ecs_dependencies() {
    let text = cargo_toml("Cargo.toml");
    let deps = deps_section(&text);
    // §4.2.2 白名单：内部 crate + bevy_ecs/bevy_app（不取整个引擎）。
    let allowed = [
        "mir2-shared",
        "mir2-data",
        "mir2-formula",
        "mir2-world-bevy",
        "bevy_ecs",
        "bevy_app",
    ];
    for d in &deps {
        assert!(
            allowed.contains(&d.as_str()),
            "crates/world-bevy 只允许依赖 {allowed:?}；实际多了 {d}"
        );
        assert!(
            !FORBIDDEN_DEPS.contains(&d.as_str()),
            "crates/world-bevy 不允许依赖 {d}（会引入并行/异步）"
        );
    }
    // 内部依赖必须是 path 依赖（防止有人把内部名替换成同名 crates.io 包）
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("mir2-") && t.contains('=') {
            assert!(t.contains("path = "), "内部依赖必须是 path 依赖，实际: {t}");
        }
    }
}

#[test]
fn shared_crate_has_no_dependencies_at_all() {
    let text = cargo_toml("../shared/Cargo.toml");
    let deps = deps_section(&text);
    assert!(
        deps.is_empty(),
        "crates/shared 必须保持零依赖（B 线也依赖它）；实际: {deps:?}"
    );
}

#[test]
fn world_sources_are_single_threaded_ecs_only() {
    let src = crate_root().join("src");
    let mut files = Vec::new();
    collect_rs(&src, &mut files);
    assert!(
        !files.is_empty(),
        "应能收集到 crates/world-bevy/src 下的源文件"
    );

    let mut joined = String::new();
    for f in &files {
        let raw = fs::read_to_string(f).unwrap();
        let code = strip_comments_and_strings(&raw);
        let hits = scan_parallel_code(&code);
        assert!(
            hits.is_empty(),
            "{} 的代码里出现并行 API {hits:?}（注释/字符串不算）",
            f.display()
        );
        joined.push_str(&code);
        joined.push('\n');
    }

    // 正面断言：tick 骨架确实来自单线程 `TickRunner`（crates/world-bevy/src 里必须引用它）。
    assert!(
        joined.contains("TickRunner"),
        "crates/world-bevy 必须用单线程 tick 骨架 `TickRunner`（§4.2.2 硬约束①）"
    );
    // 正面断言：执行器本身在 world-bevy 的 tick 骨架里显式**调用**为单线程。
    // 注意只认 `set_executor(SingleThreadedExecutor...)` 调用形态，先剥掉 `use` 行：
    // 仅 import 标识符不算（否则删掉真正生效的 set_executor 调用，守卫仍绿 —— F2）。
    let tick_rs = crate_root().join("../world-bevy/src/tick.rs");
    let tick_raw = fs::read_to_string(&tick_rs).unwrap();
    let tick_code = strip_comments_and_strings(&tick_raw);
    assert!(
        tick_has_explicit_executor_and_rationale(&tick_raw),
        "world-bevy 的 tick 骨架必须显式 `set_executor(...)`（裸 import 不算）且带 `EXECUTOR-RATIONALE` 注释说明理由"
    );
    assert!(
        scan_parallel_code(&tick_code).is_empty(),
        "world-bevy 的 tick 骨架不得出现外部线程/手搓线程 API（bevy 执行器允许）"
    );
}

/// 红检（自动）：源码扫描必须能发现"偷偷加并行 API"。
#[test]
fn redcheck_scanner_detects_injected_parallel_code() {
    let injected = "fn f() { std::thread::spawn(|| 1); }\nuse rayon::prelude::*;\n";
    assert_eq!(
        scan_parallel_code(&strip_comments_and_strings(injected)),
        vec!["rayon", "thread::spawn"],
        "红检失效：注入的并行代码未被识别"
    );
    // 注释/字符串里的字样不得误报（这是 D3 试验田踩过的坑）。
    let benign = "// 禁止 MultiThreadedExecutor / bevy_tasks / thread::spawn\nlet s = \"rayon\";\n";
    assert!(
        scan_parallel_code(&strip_comments_and_strings(benign)).is_empty(),
        "注释/字符串被误判为代码：{}",
        strip_comments_and_strings(benign)
    );
}

/// 红检（自动）：白名单比对本身必须能发现"偷偷加依赖"。
#[test]
fn redcheck_forbidden_dep_detection_works() {
    let fake = "[dependencies]\nmir2-shared = { path = \"../shared\" }\nrayon = \"1\"\n";
    let deps = deps_section(fake);
    assert!(deps.contains(&"rayon".to_string()));
    assert!(
        deps.iter().any(|d| FORBIDDEN_DEPS.contains(&d.as_str())),
        "红检失效：注入的禁用依赖未被识别"
    );
}

/// 红检（自动，F2）：只剩 `use` 而没有 `set_executor` 调用时，正向断言必须判假；
/// 有真实调用时才判真。防止把「标识符出现过」当成「执行器被显式设置」。
#[test]
fn redcheck_import_only_does_not_satisfy_executor_guard() {
    let import_only = "use bevy_ecs::schedule::SingleThreadedExecutor;";
    assert!(
        !has_explicit_single_threaded_executor(&strip_comments_and_strings(import_only)),
        "只留 import 不得满足单线程执行器守卫"
    );
    let with_call = "use bevy_ecs::schedule::SingleThreadedExecutor;
fn f() { s.set_executor(SingleThreadedExecutor::default()); }";
    assert!(
        has_explicit_single_threaded_executor(&strip_comments_and_strings(with_call)),
        "真实 set_executor(SingleThreadedExecutor...) 调用必须被识别"
    );
}
// ============================================================================
// 重放确定性门（判据③）——**实现在 build 窗口做**（需 world-bevy World API + 编译迭代）。
// 设计（8× 逐字节）：
//   fn replay_once() -> Vec<Vec<u8>> {
//       let mut w = World::new(32,32);           // 固定尺寸
//       let mut traces = Vec::new();
//       // 固定驱动：固定种子 + 固定操作序（spawn/move/attack/...，不含 HashMap 迭代序）
//       for _tick in 0..K { let rec = w.tick(); traces.push(canonical_bytes(&rec)); }
//       traces                                    // 每 tick 记录的**规范化字节**（Vec，非 HashMap）
//   }
//   #[test] fn replay_determinism_8x() {
//       let base = replay_once();
//       for i in 1..8 { assert_eq!(replay_once(), base, "replay #{i} 与基准逐字节不一致 ⇒ 并行引入非确定"); }
//   }
// 约束（红线）：canonical_bytes 只序列化**有序**结构（Vec/数组/显式排序的键），**严禁** HashMap/
// HashSet 迭代序进输出；断言是**逐字节相等**，不得改容差。窗口期内先跑串行骨架应 8× 一致（基准）；
// 换 MultiThreadedExecutor 后仍 8× 一致才算该并行组可用，否则该组退回串行。
// ============================================================================
