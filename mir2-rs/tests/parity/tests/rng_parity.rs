//! 门禁 ②：固定种子下 RNG 输出序列与 C# 逐项相同。
//!
//! 金标准：`golden/rng_seed*.txt`，由 `csharp/ParityGolden`（链接真实
//! `src/OpenMir2/RandomNumber.cs`，种子经反射注入）产出。
//! 下方 `scripted_sequence` 的调用脚本与 `Program.cs::RngMode` 逐条一致——
//! 改任一侧脚本而忘记另一侧，本测试立刻红（这也是红检的一部分）。

use mir2_shared::rng::RandomNumber;

fn scripted_sequence(seed: i32) -> String {
    let mut rn = RandomNumber::with_seed(seed);
    let mut out = String::new();
    for i in 0..500 {
        out.push_str(&rn.random().to_string());
        out.push('\n');
        out.push_str(&rn.random_below(100).to_string());
        out.push('\n');
        out.push_str(&rn.random_range(10, 1000).to_string());
        out.push('\n');
        out.push_str(&rn.get_random_number(1, 6).to_string());
        out.push('\n');
        out.push_str(&rn.random_byte(200).to_string());
        out.push('\n');
        if i % 10 == 0 {
            out.push_str(&rn.generate_random_number(8));
            out.push('\n');
        }
        if i % 25 == 0 {
            let mut src = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
            let picked = rn.random_select(&mut src, 3);
            let joined = picked
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            out.push_str(&joined);
            out.push('\n');
        }
    }
    out
}

/// 差异定位：返回第一处不一致的行号与两侧内容。
fn first_diff(a: &str, b: &str) -> String {
    for (i, (la, lb)) in a.lines().zip(b.lines()).enumerate() {
        if la != lb {
            return format!("第 {} 行: rust=`{la}` csharp=`{lb}`", i + 1);
        }
    }
    if a.lines().count() != b.lines().count() {
        return format!(
            "行数不同: rust={} csharp={}",
            a.lines().count(),
            b.lines().count()
        );
    }
    "无差异".to_string()
}

#[test]
fn rng_matches_csharp_golden_seed42() {
    let golden = include_str!("../golden/rng_seed42.txt");
    let mine = scripted_sequence(42);
    assert_eq!(
        mine,
        golden,
        "RNG 序列与 C# 金标准不一致: {}",
        first_diff(&mine, golden)
    );
}

#[test]
fn rng_matches_csharp_golden_seed20240229() {
    let golden = include_str!("../golden/rng_seed20240229.txt");
    let mine = scripted_sequence(20240229);
    assert_eq!(
        mine,
        golden,
        "RNG 序列与 C# 金标准不一致: {}",
        first_diff(&mine, golden)
    );
}

/// 红检（自动部分）：换一个种子，输出必须与金标准不同——
/// 若本测试变绿，说明对拍根本不会失败（恒绿门禁 = 没有门禁）。
#[test]
fn redcheck_wrong_seed_must_differ() {
    let golden = include_str!("../golden/rng_seed42.txt");
    assert_ne!(
        scripted_sequence(43),
        golden,
        "红检失效：错误种子产出了与金标准相同的序列"
    );
}

/// 红检（自动部分）：调用脚本顺序错乱（调换两个调用）必须可检出。
#[test]
fn redcheck_shuffled_calls_must_differ() {
    let mut rn = RandomNumber::with_seed(42);
    let a = rn.random_below(100); // 与脚本第一步 random() 不同
    let golden = include_str!("../golden/rng_seed42.txt");
    let first = golden.lines().next().unwrap();
    assert_ne!(
        a.to_string(),
        first,
        "红检失效：不同调用产出了相同首值（极小概率，换例重查）"
    );
}
