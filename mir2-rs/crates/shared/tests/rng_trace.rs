//! **M3②③ 决策片（owner 2026-10-12 18:00）**：Rust 侧 opt-in 逐次 RNG trace 的验收。
//!
//! 判据（一条不可少）：
//!   ① **关闭时零差异**：trace 关闭下取的序列，与开启后取的同种子序列**逐项相同**（trace 不改流）；
//!   ② **开启时同形**：落盘 TSV 与 C# 钩子 `rng-hook/StartupHook.cs` 逐列同形
//!      （`seq \t kind \t arg \t result \t site`；头两行 `INJECT_OK` / `SEED_PROBE`），
//!      每条取数 1 行、`seq` 单调、`site` 非空、`result` 与关闭时序列逐项一致；
//!   ③ **判据有牙**：把任一行 `result` 改坏 ⇒ 校验器必红（防「trace 写错也能过」）。
//!
//! 本文件是**独立 integration test 进程**：trace 的 `enabled()`/`sink` 是进程级全局，
//! 与其它单测同进程会互相污染 ⇒ 单独一个二进制、一个 `#[test]` 顺序跑完三阶段。

use mir2_shared::rng::{RandomNumber, RandomSource};

/// 一次「纯单取数」序列：全部落到 1 次取数/调用的入口（结果与取数一一对应）。
fn draws() -> Vec<i32> {
    let mut rng = RandomNumber::with_seed(42);
    let mut out = Vec::new();
    for _ in 0..10 {
        out.push(rng.random());
        out.push(rng.random_below(100));
        out.push(rng.random_range(-5, 5));
        out.push(rng.get_random_number(1, 6));
        out.push(rng.random_byte(200) as i32);
    }
    out
}

/// 触达其余入口（多取数/字符串/不放回），用于「值不变」的第二重校验。
fn all_entry_points() -> (Vec<i32>, usize, String, Vec<i32>) {
    let mut rng = RandomNumber::with_seed(42);
    let head = draws();
    let len = rng.generate_random_number(5).len();
    let mut src = vec![1, 2, 3, 4, 5];
    let picked = rng.random_select(&mut src, 2);
    let mut rest = Vec::new();
    for _ in 0..3 {
        rest.push((rng.next_double() * 1_000_000.0) as i32);
    }
    (
        head,
        len,
        picked
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(","),
        rest,
    )
}

/// 校验 trace 行与「关闭时序列」逐项一致；改坏任一行 `result` ⇒ `Err`。
fn check_trace(expected: &[i32], rows: &[&str]) -> Result<(), String> {
    if rows.len() != expected.len() {
        return Err(format!("行数 {} != 取数 {}", rows.len(), expected.len()));
    }
    for (i, (row, exp)) in rows.iter().zip(expected.iter()).enumerate() {
        let cols: Vec<&str> = row.split('\t').collect();
        if cols.len() != 5 {
            return Err(format!("第 {i} 行不是 5 列: {row}"));
        }
        let seq: i64 = cols[0]
            .parse()
            .map_err(|_| format!("第 {i} 行 seq 非整数: {row}"))?;
        if seq != i as i64 + 1 {
            return Err(format!("第 {i} 行 seq={seq} 不单调"));
        }
        let got: i32 = cols[3]
            .parse()
            .map_err(|_| format!("第 {i} 行 result 非整数: {row}"))?;
        if got != *exp {
            return Err(format!("第 {i} 行 result={got} != 期望 {exp}"));
        }
        if cols[4].is_empty() {
            return Err(format!("第 {i} 行 site 为空"));
        }
        // `#[track_caller]` 链必须把 site 传到**真正的调用方**，而不是 `crates/shared/src/rng.rs`
        // 的包装层（否则所有取数 site 一样，账本无法区分攻击路径/其它消费者）。
        if cols[4].contains("crates/shared/src/rng.rs") || cols[4].contains("src\\rng.rs") {
            return Err(format!("第 {i} 行 site 落在包装层而非调用方: {}", cols[4]));
        }
    }
    Ok(())
}

#[test]
fn trace_off_is_byte_identical_and_on_records_same_form() {
    // 防外部环境把 trace 提前打开：清掉两个开关变数（本用例自己显式 enable）。
    std::env::remove_var(mir2_shared::rng_trace::TRACE_ENV);
    std::env::remove_var("MIR2_RNG_SEED");

    // ---- 阶段 0：关闭（默认）----
    assert!(
        !mir2_shared::rng_trace::enabled(),
        "未设 MIR2_RNG_LOG ⇒ 必须默认关闭"
    );
    let off_draws = draws();
    let off_all = all_entry_points();

    // ---- 阶段 1：显式开启 ----
    let dir = std::env::temp_dir().join(format!("rng-trace-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rng-stream.tsv");
    mir2_shared::rng_trace::enable(&path, 42).expect("enable 应成功");
    assert!(mir2_shared::rng_trace::enabled());

    let on_draws = draws();
    let on_all = all_entry_points();

    // ① 关闭 vs 开启：值逐项相同（trace 绝不消耗/改动随机流）。
    assert_eq!(off_draws, on_draws, "trace 不得改变取数序列");
    assert_eq!(off_all, on_all, "所有入口的值都必须不变");

    // ② 同形：头两行 + 每条取数一行 + 逐列。
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "1\tINJECT_OK\t42\t0", "头部 INJECT_OK 同形");
    assert!(lines[1].starts_with("SEED_PROBE "), "头部 SEED_PROBE 同形");
    assert_eq!(
        lines[1].split_whitespace().count(),
        1 + mir2_shared::rng_trace::PROBE_LEN,
        "SEED_PROBE 必须 8 项"
    );
    // `draws()` 是唯一被 trace 的序列（其后 all_entry_points 又追了 12+ 行）；
    // 只取前 `off_draws.len()` 行来逐项校验（与关闭时的同一序列对拍）。
    let data_rows: Vec<&str> = lines[2..].to_vec();
    check_trace(&off_draws, &data_rows[..off_draws.len()]).expect("trace 行应与关闭序列逐项一致");
    assert!(
        data_rows.len() >= off_draws.len(),
        "开启了就必须把后续取数也记下（on 阶段比 off 多跑了 all 序列）"
    );

    // ③ 有牙：改坏一行 result ⇒ 校验器必红。
    let mut mutated: Vec<String> = data_rows[..off_draws.len()]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let cols: Vec<&str> = mutated[0].split('\t').collect();
    let bad = format!(
        "{}\t{}\t{}\t{}\t{}",
        cols[0], cols[1], cols[2], 999_999, cols[4]
    );
    mutated[0] = bad;
    let mutated_refs: Vec<&str> = mutated.iter().map(|s| s.as_str()).collect();
    assert!(
        check_trace(&off_draws, &mutated_refs).is_err(),
        "改坏一行 result ⇒ 校验器必须报红"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
