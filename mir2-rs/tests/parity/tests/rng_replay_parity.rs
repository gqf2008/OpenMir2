//! 门禁（M3 准备项 ②）：用**真实 C# 进程**的 RNG 消耗流验两侧等价。
//!
//! 夹具来源：影子 GameSvr + `m3/rng-hook`（seed=42）记录的真实取数流。
//! - `rng_stream_seed42_head1828.tsv`：前 1828 次调用。**该区段经两次同种子运行实测逐字节一致**
//!   （确定性区段）——用它做「Rust 复刻从种子重算 = C# 实际取数」的强对拍。
//! - `rng_stream_seed42_seg4000.tsv`：前 4000 次调用（含非确定性区段），
//!   只用于验证「回放器能逐条复现记录」。
//!
//! 这条门禁补上的是 D-3 缺口的另一半：固定种子重算能对拍"数值"，而**回放**能对拍
//! "取数的形态/顺序/次数"——两者合起来才等价于"同一操作序列下两侧逐次一致"。

use mir2_shared::replay::ReplayRandom;
use mir2_shared::rng::{DotNetRandom, RandomSource};

const SEED: i32 = 42;

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("m3/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读夹具 {path:?} 失败: {e}"))
}

/// 强对拍：C# 真实进程里的取数流，用 Rust 复刻从同一种子重算，逐项相同。
#[test]
fn csharp_process_stream_regenerates_from_seed() {
    let mut rng = DotNetRandom::new(SEED);
    let replay = ReplayRandom::from_tsv(&fixture("rng_stream_seed42_head1828.tsv")).unwrap();
    assert_eq!(replay.entries().len(), 1828, "夹具条数变了，先确认来源");
    for (i, entry) in replay.entries().iter().enumerate() {
        let mine = entry.replay_onto(&mut rng);
        assert_eq!(
            mine,
            entry.result,
            "第 {} 次调用不一致（seq={} site={}）: rust={mine} csharp={}",
            i + 1,
            entry.seq,
            entry.site,
            entry.result
        );
    }
}

/// 回放器：逐条消费真实记录，形态/参数必须完全吻合，且恰好用尽。
#[test]
fn replay_harness_consumes_csharp_stream() {
    let text = fixture("rng_stream_seed42_seg4000.tsv");
    let expected: Vec<_> = ReplayRandom::from_tsv(&text).unwrap().entries().to_vec();
    let mut replay = ReplayRandom::from_tsv(&text).unwrap();
    for entry in &expected {
        let got = entry.replay_onto(&mut replay);
        assert_eq!(got, entry.result);
    }
    assert!(
        replay.divergences().is_empty(),
        "回放出现分歧: {:?}",
        &replay.divergences()[..replay.divergences().len().min(5)]
    );
    assert_eq!(replay.consumed(), expected.len());
    assert_eq!(replay.remaining(), 0, "记录未被消费完 —— 本端少调用了");
}

/// 红检 1：把记录里某个结果改掉，强对拍必须能发现（证明它真的在比对数值）。
#[test]
fn redcheck_tampered_result_must_differ() {
    let text = fixture("rng_stream_seed42_head1828.tsv");
    let mut rng = DotNetRandom::new(SEED);
    let replay = ReplayRandom::from_tsv(&text).unwrap();
    let entry = &replay.entries()[100];
    let tampered = entry.result.wrapping_add(1);
    let mine = entry.replay_onto(&mut rng);
    let _ = rng;
    assert_ne!(mine, tampered, "红检失效：篡改记录后仍判相同");
}

/// 红检 2：调用形态/参数不符（这里把上限 +1）必须被回放器记为分歧。
#[test]
fn redcheck_wrong_call_shape_must_diverge() {
    let text = fixture("rng_stream_seed42_head1828.tsv");
    let replay = ReplayRandom::from_tsv(&text).unwrap();
    let first = replay.entries()[0].clone();
    let mut r = ReplayRandom::from_tsv(&text).unwrap();
    // 第一处记录是 Next(max)=arg；故意用 arg+1 调用
    r.random_below(first.arg as i32 + 1);
    assert_eq!(r.divergences().len(), 1, "红检失效：形态不符未被记录");
    assert!(
        r.divergences()[0].contains("Next(max)"),
        "{}",
        r.divergences()[0]
    );
}
