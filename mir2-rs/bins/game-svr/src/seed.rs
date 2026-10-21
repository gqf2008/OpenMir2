//! `MIR2_RNG_SEED` 同种子注入口（M3 **T-30**）。
//!
//! C# 侧用 `mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs`（.NET Startup Hook，只放**影子副本**）
//! 把 oracle 的随机源换成 `new Random(seed)`；而 Rust game-svr 的生产路径此前只吃
//! `crates/world::World::with_maps` 里写死的 `RandomNumber::with_seed(42)`，**不读任何种子环境变数**
//! ⇒ 真机两侧「固定种子 100 次攻击伤害序列逐项相同」跑不出结论（whitelist T-30）。
//!
//! 本模块把 C# 钩子的读取口径 1:1 搬过来（变数名/格式/失败处置），只在**设了变数**时覆盖随机源：
//! - 变数名 [`RNG_SEED_ENV`]，值按 `int.TryParse`（`NumberStyles.Integer`：前后空白 + 前导正负号 +
//!   十进制）解析成 `i32`；
//! - `null`/空串 ⇒ **不启用**（对应钩子开头的 `string.IsNullOrEmpty` 提前 return）⇒ 生产路径一字不改；
//! - 解析失败 ⇒ [`SeedError`]（钩子此时 `Fail` ⇒ `Environment.Exit(97)`）。**不许静默降级**：
//!   「以为注入了种子、其实没有」会污染整批对拍结论（C# 侧模块头已写明同一条纪律）。
//!
//! 种子只喂给 `mir2_shared::rng::RandomNumber::with_seed`（`.NET System.Random(int)` 的 legacy
//! 减法算法逐行复刻，见 `crates/shared/src/rng.rs`）——**不在 game-svr 里另造随机源**。

#[cfg(test)]
use mir2_shared::rng::RandomNumber;
/// C# 钩子读取的环境变数名（`StartupHook.cs:41`）。
pub const RNG_SEED_ENV: &str = "MIR2_RNG_SEED";

/// `SEED_PROBE` 自证序列长度（照 C# `StartupHook`：`new Random(seed).Next()` 取 8 项）。
pub const PROBE_LEN: usize = 8;

/// `MIR2_RNG_SEED` 解析失败（对应 C# 钩子的 `Fail(...)` → `Environment.Exit(97)`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedError {
    message: String,
}

impl SeedError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SeedError {}

/// 解析 `MIR2_RNG_SEED` 的文本（照 C# `string.IsNullOrEmpty` + `int.TryParse`）。
///
/// - `None`（变数未设）或 `Some("")`（变数被设为空串）⇒ `Ok(None)`：**不启用**；
/// - 十进制 `i32`（允许前后空白与 `+`/`-`）⇒ `Ok(Some(seed))`；
/// - 其余（非数字/十六进制/小数/**溢出**）⇒ `Err`（对应 C# `TryParse` 失败后的 `Fail`）。
///
/// 注意空白串（`"   "`）：C# 的 `IsNullOrEmpty` 不认它，会走到 `int.TryParse` 并失败 ⇒ `Err`，
/// 本函数同形（**不是**「空串」分支）。
pub fn parse_seed(raw: Option<&str>) -> Result<Option<i32>, SeedError> {
    let Some(text) = raw else {
        return Ok(None);
    };
    if text.is_empty() {
        // C# `String.IsNullOrEmpty`：空串与 null 同义。
        return Ok(None);
    }
    text.trim()
        .parse::<i32>()
        .map(Some)
        .map_err(|_| SeedError::new(format!("MIR2_RNG_SEED 不是合法 int: {text}")))
}

/// 从进程环境读 `MIR2_RNG_SEED`（生产路径入口）。
pub fn env_seed() -> Result<Option<i32>, SeedError> {
    match std::env::var_os(RNG_SEED_ENV) {
        None => Ok(None),
        Some(os) => match os.to_str() {
            Some(text) => parse_seed(Some(text)),
            // C# 侧变数是 .NET string（Windows 上 UTF-16）⇒ 非 UTF-8 是异常形态；**fail-closed**，
            // 不当成「没设」静默放行（同「不许静默降级」）。
            None => Err(SeedError::new(format!(
                "MIR2_RNG_SEED 含非 UTF-8 字节: {os:?}"
            ))),
        },
    }
}

/// `new Random(seed).Next()` 的参照序列（与 C# 钩子写进 `MIR2_RNG_LOG` 的 `SEED_PROBE` 同口径）。
///
/// 每次调用都新建 [`RandomNumber`] ⇒ **不消耗游戏随机流**（照 C# 钩子用独立 `reference` 实例的做法）。
#[cfg(test)]
pub fn probe_values(seed: i32) -> [i32; PROBE_LEN] {
    let mut rng = RandomNumber::with_seed(seed);
    std::array::from_fn(|_| rng.random())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C# 钩子实测的 `SEED_PROBE`（`MIR2_RNG_SEED=42`，见
    /// `mir2-rs/tests/parity/m3/RNG同种子与记录回放方案.md` §二「实测证据」）。
    /// 这是「C# 进程内的随机源」的验收标尺：Rust 同种子序列必须逐项相同。
    const CSHARP_SEED42_PROBE: [i32; PROBE_LEN] = [
        1_434_747_710,
        302_596_119,
        269_548_474,
        1_122_627_734,
        361_709_742,
        563_913_476,
        1_555_655_117,
        1_101_493_307,
    ];

    #[test]
    fn parse_seed_unset_or_empty_is_disabled() {
        assert_eq!(parse_seed(None), Ok(None), "未设变数 ⇒ 不启用");
        assert_eq!(
            parse_seed(Some("")),
            Ok(None),
            "空串 ⇒ 不启用（IsNullOrEmpty）"
        );
    }

    #[test]
    fn parse_seed_accepts_decimal_with_sign_and_whitespace() {
        assert_eq!(parse_seed(Some("42")), Ok(Some(42)));
        assert_eq!(
            parse_seed(Some(" 42 ")),
            Ok(Some(42)),
            "int.TryParse 允许前后空白"
        );
        assert_eq!(parse_seed(Some("+7")), Ok(Some(7)));
        assert_eq!(parse_seed(Some("-7")), Ok(Some(-7)));
        assert_eq!(parse_seed(Some("0")), Ok(Some(0)));
        assert_eq!(parse_seed(Some("2147483647")), Ok(Some(i32::MAX)));
        assert_eq!(parse_seed(Some("-2147483648")), Ok(Some(i32::MIN)));
    }

    #[test]
    fn parse_seed_rejects_non_integer() {
        // 四条都必须 Err：非数字 / 十六进制 / 小数 / 溢出。
        for text in ["abc", "0x10", "4.2", "2147483648"] {
            assert!(
                parse_seed(Some(text)).is_err(),
                "{text:?} 不是合法 int，必须 Err（对应 C# TryParse 失败 ⇒ Fail）"
            );
        }
        // 空白串不是「空串」：C# IsNullOrEmpty 不认它，TryParse 也失败 ⇒ Err。
        assert!(
            parse_seed(Some("   ")).is_err(),
            "空白串必须 Err（不是空串分支）"
        );
    }

    /// 判据：Rust 的 `with_seed(42)` 参照序列与 C# 钩子实测逐项相同。
    #[test]
    fn probe_matches_csharp_seed42() {
        assert_eq!(probe_values(42), CSHARP_SEED42_PROBE);
    }

    #[test]
    fn probe_is_deterministic_and_seed_sensitive() {
        assert_eq!(probe_values(42), probe_values(42), "同种子必须可复现");
        assert_ne!(
            probe_values(42),
            probe_values(7),
            "不同种子必须给出不同序列（防「忽略种子」的假绿）"
        );
    }

    /// 构造性用例（生产路径写法，`lib.rs::sim_loop`）：世界随机源被注入的种子取代后，
    /// 从**该实例**取出的序列必须与 C# 同种子逐项相同。
    #[test]
    fn injected_world_rng_matches_csharp_probe() {
        let mut world = mir2_world::World::with_maps(vec![(8, 8)]);
        world.rng = RandomNumber::with_seed(42);
        let got: Vec<i32> = (0..PROBE_LEN).map(|_| world.rng.random()).collect();
        assert_eq!(got, CSHARP_SEED42_PROBE);
    }

    /// 构造性用例：**不设**环境变数（`None`）时不得动世界的既有随机源 ——
    /// 即「与现状逐字节相同」（`crates/world::World::with_maps` 的 seed 42）。
    #[test]
    fn unset_seed_keeps_existing_world_rng() {
        let mut world = mir2_world::World::with_maps(vec![(8, 8)]);
        assert_eq!(
            world.rng.random(),
            RandomNumber::with_seed(42).random(),
            "未注入 ⇒ 保持既有随机源（不引入默认种子覆盖）"
        );
    }
}
