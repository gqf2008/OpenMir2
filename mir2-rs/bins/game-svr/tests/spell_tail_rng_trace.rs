//! **共享尾训练点抽数的 Rust 活栈站点自证**（卡 `openmir2-m3-firewind-spacemove-wire`，复验 must_fix ②）。
//!
//! 独立测试二进制（进程级 `rng_trace` 开关只在一个测试里打开，无并发污染）：
//! 用**未满**的 `train_level`（`[0; 4]`）真正走进 `MagicManager.cs:663` 的
//! `Level < 3 && boTrain && TrainLevel[Level] <= Abil.Level` 门，抽出 `Random(3)+1`。
//!
//! 判据：trace 里**恰好**一条站点，且必须是
//! `crates/world/src/magic_effect.rs:<line>`；**不得**出现 `crates/world/src/magic.rs:`。
//! 后者留在 `magic.rs` 会被 `rng_ledger_diff.py` 的 `RUST_ATTACK_RE`（整文件 `magic\.rs:`）
//! 吞成 `attack` 组（Claim B 豁免），而 C# 侧该抽登记为 `magic_effect` ⇒ `groups
//! --require-closed` 的 `--side csharp` 口径会因两侧 vocabulary 不对称而判红。
//!
//! 证据用法（把真实 trace 留给 `rng_ledger_diff.py groups --require-closed` 复算）：
//! ```text
//! M2_SPELL_TAIL_TRACE_PATH=E:/tmp/spell-tail-rust.tsv \
//!   cargo test -p game-svr --test spell_tail_rng_trace -- --nocapture
//! ```

use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::magic::{self, SkillTrainState};

#[test]
fn spell_tail_train_draw_lands_in_magic_effect_not_attack_file() {
    let from_env = std::env::var("M2_SPELL_TAIL_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-spell-tail-rng-{}.tsv", std::process::id())),
    };
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, 20261017).expect("打开 rng trace");

    // 旧片用 `[255; 4]` 把该抽屏蔽掉（`TrainLevel[Level] <= Abil.Level` 恒假）。这里取未满值，
    // `Abil.Level = 255` ⇒ 门全过，真正走到 `Random(3)+1`。
    let mut state = SkillTrainState {
        magic_id: 1,
        level: 0,
        tran_point: 0,
        train_lv: 3,
        train_level: [0; 4],
        max_train: [100_000; 4],
    };
    let mut rng = RandomNumber::with_seed(20261017);
    let out = magic::do_spell_tail(false, true, true, false, 255, &mut rng, &mut state);
    assert!(out.returned, "boSpellFail=false ⇒ 不早退");
    assert!(out.train.trained, "门全过 ⇒ 必须实际抽 Random(3)+1");
    assert!(
        (1..=3).contains(&state.tran_point),
        "熟练点增量 = Random(3)+1 ∈ [1,3]；实得 {}",
        state.tran_point
    );

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("SPELL_TAIL_TRACE_PATH {path:?}");
    let norm = txt.replace('\\', "/");
    let rows: Vec<&str> = norm
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with("SEED_PROBE") && !l.contains("INJECT_OK"))
        .collect();
    println!("SPELL_TAIL_ROWS {rows:?}");
    assert_eq!(rows.len(), 1, "尾部只抽一次 Random(3)；实得 {rows:?}");
    assert!(
        rows[0].contains("crates/world/src/magic_effect.rs:"),
        "尾部取数站点必须在 magic_effect.rs（consumer_groups.tsv 的 magic_effect 组）: {rows:?}"
    );
    assert!(
        !norm.contains("crates/world/src/magic.rs:"),
        "尾部取数**不得**留在 magic.rs（RUST_ATTACK_RE 会整文件吞成 attack）:\n{txt}"
    );
}
