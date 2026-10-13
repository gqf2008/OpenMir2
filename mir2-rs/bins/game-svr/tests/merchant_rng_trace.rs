//! **M3⑤-实现 2 第②片**：商人 tick 的 Rust 活栈 RNG 站点自证（Claim B `merchant` 组）。
//!
//! 本文件是**独立测试二进制**（`cargo test --test merchant_rng_trace`）——`rng_trace` 的
//! 开关是进程级全局（`OnceLock`），只有独占进程才不会被别的用例的取数污染。
//!
//! 判据：开着 `MIR2_RNG_LOG` 同口径的 trace，跑一次 `World::spawn_merchant` + `merchant_tick`
//! 与 `merchant_rename_rand`，落盘里必须出现 **`crates/world/src/merchant.rs:<line>`** 站点
//! （`consumer_groups.tsv` 的 `merchant`/`rust` 行）——认不出即 Claim B 判红。
//!
//! 证据用法（把真实 trace 留给 `rng_ledger_diff.py groups` 复算）：
//! ```text
//! M2_MERCHANT_TRACE_PATH=E:/tmp/merchant-rust.tsv \
//!   cargo test -p game-svr --test merchant_rng_trace -- --nocapture
//! ```

use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::merchant::{MerchantFlags, MerchantGoods, MerchantSpawn, MerchantState};
use mir2_world::World;

fn merchant_state(goods: Vec<MerchantGoods>) -> MerchantState {
    MerchantState::new(
        "m/script".to_string(),
        "0".to_string(),
        0,
        9,
        100,
        false,
        false,
        0,
        MerchantFlags::default(),
        vec![],
        goods,
        0,
    )
}

#[test]
fn merchant_draws_record_world_merchant_module_sites() {
    let from_env = std::env::var("M2_MERCHANT_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-merchant-rng-{}.tsv", std::process::id())),
    };
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, 2024).expect("打开 rng trace");

    // 构造抽（RunTick = now + Random(1500)）走 `spawn_merchant`（站点在 world/merchant.rs）。
    let mut w = World::new(64, 64);
    w.rng = RandomNumber::with_seed(2024);
    w.clock_ms = 0;
    let spawn = MerchantSpawn {
        name: "m".to_string(),
        x: 3,
        y: 4,
        map_id: 0,
        state: merchant_state(vec![MerchantGoods {
            item_name: "金创药(小量)".to_string(),
            count: 5,
            refill_time: 5,
            refill_tick: 0,
        }]),
    };
    let id = w.spawn_merchant(spawn).expect("挂图成功");
    if let Some(st) = w.entities.get_mut(id).and_then(|e| e.merchant.as_mut()) {
        st.run_tick = -1000;
    }
    w.clock_ms = 100_000;
    let _ = w.merchant_tick();

    // O-2 `rename_rand` 也必须记到 world/merchant.rs（宿主的闭包只转发）。
    let mut rng = RandomNumber::with_seed(2024);
    let _ = mir2_world::merchant::merchant_rename_rand(&mut rng, 1, 200);

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("MERCHANT_TRACE_PATH {path:?}");
    let mut merchant_sites = 0usize;
    for line in txt.lines() {
        if line.contains("merchant.rs") {
            merchant_sites += 1;
            println!("MERCHANT_RNG_SITE {line}");
        }
    }
    assert!(
        merchant_sites > 0,
        "trace 应含 world/merchant.rs 站点（否则 Claim B merchant 组认不出）:\n{txt}"
    );
    assert!(
        txt.contains("crates/world/src/merchant.rs:")
            || txt.contains("crates\\world\\src\\merchant.rs:"),
        "站点必须落在 crates/world/src/merchant.rs（不是 bins/**）:\n{txt}"
    );
}
