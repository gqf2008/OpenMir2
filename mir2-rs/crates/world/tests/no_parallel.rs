//! 判据③：**禁止引入并行**。两条独立断言（任一被破坏即红）。
//!
//! 1. **依赖白名单**（结构性）：`crates/world` 与 `crates/shared` 的依赖必须为空
//!    （`mir2-world` 只许依赖 `mir2-shared`）。加入 tokio/rayon/crossbeam/threadpool 等
//!    任一"会起线程"的库，本测试立刻红。
//! 2. **运行期线程断言**：`World::tick` 每次校验线程 id（见 `crates/world/src/world.rs`
//!    的 `tick_refuses_to_run_on_another_thread`）——把 world 挪到别的线程上跑即 panic。
//!
//! 覆盖边界（诚实声明）：本测试拦的是"**依赖/驱动方式**引入并行"；
//! 若有人在别处 `std::thread::spawn` 后通过本 crate 的公开 API 以单线程方式驱动 world，
//! 依赖白名单看不出问题——但那种用法下 world 内部仍是串行的，符合 §4.2 的硬约束。

use std::fs;
use std::path::Path;

/// 会起线程/引入异步运行时的库（一旦进入 world 的依赖面即判红）。
const FORBIDDEN: [&str; 12] = [
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

fn cargo_toml(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
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

#[test]
fn world_crate_has_no_threading_dependencies() {
    let text = cargo_toml("Cargo.toml");
    let deps = deps_section(&text);
    assert_eq!(
        deps,
        vec!["mir2-shared".to_string()],
        "crates/world 只允许依赖 mir2-shared（零并行/零异步）；实际: {deps:?}"
    );
    for d in &deps {
        for bad in FORBIDDEN {
            assert_ne!(d, bad, "crates/world 不允许依赖 {bad}（会引入并行/异步）");
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

/// 红检（自动）：白名单比对本身必须能发现"偷偷加依赖"。
#[test]
fn redcheck_forbidden_dep_detection_works() {
    let fake = "[dependencies]\nmir2-shared = { path = \"../shared\" }\nrayon = \"1\"\n";
    let deps = deps_section(fake);
    assert!(deps.contains(&"rayon".to_string()));
    assert!(
        deps.iter().any(|d| FORBIDDEN.contains(&d.as_str())),
        "红检失效：注入的禁用依赖未被识别"
    );
}
