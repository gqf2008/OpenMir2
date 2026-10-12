//! `System.Random`（固定种子，legacy 减法算法）与 `RandomNumber.cs` 的逐条复刻。
//!
//! 参照源：
//! - `src/OpenMir2/RandomNumber.cs`（包装层，全部委托给 `System.Random`）
//! - .NET `System.Random` 固定种子实现（.NET Framework 与 .NET Core 的
//!   `CompatSeedImpl` 一致：Knuth 减法滞后斐波那契）。
//!
//! 注意：C# 生产路径用的是无参 `new Random()`，在 .NET 6+ 上是 xoshiro256**，
//! 与固定种子的 legacy 算法不是同一序列。无参序列无法做固定种子对拍，
//! 本 crate 只复刻「可仲裁的」固定种子算法（见 tests/parity/README.md）。

/// .NET `System.Random` 的 MBIG 常量。
const MBIG: i32 = i32::MAX; // 2147483647
/// .NET `System.Random` 的 MSEED 常量。
const MSEED: i32 = 161803398;

/// **M3②③ 活栈隔离口径（owner 2026-10-12 21:15，§6.0-3 owner 签字）**：
/// 攻击路径专用种子流的开关环境变量。非空且为「开」值（`1`/`true`/`on`/`yes`，大小写不敏感）
/// ⇒ 开启；空/`0`/`false`/`off`/`no` ⇒ 关闭（缺省）。
///
/// 这是**测量口径**的开关：开启时把攻击路径的取数从全服共享流隔离到
/// [`attack_seed`] 派生出的专用流，使两侧「同种子 ⇒ 逐 k 相同」重新可判；
/// **关闭时（缺省）行为与改动前逐字节相同**（专用流不构造、不被取数）。
pub const ISOLATE_ATTACK_ENV: &str = "MIR2_RNG_ISOLATE_ATTACK";

/// 专用流种子的派生常量（**两侧同口径**：C# 影子 hook 与 Rust 侧必须用同一公式）。
///
/// 口径：`attack_seed = base_seed.wrapping_add(0x9E37)`，`base_seed` = 既有 `MIR2_RNG_SEED`。
/// 用 `i32` 回绕加法（.NET 的 `int + int` 在 unchecked 上下文同样回绕），
/// 保证两侧对同一 `MIR2_RNG_SEED` 得到同一 `attack_seed`。
pub const ATTACK_SEED_DELTA: i32 = 0x9E37;

/// 既有同种子注入口环境变量名（`bins/game-svr/src/seed.rs::RNG_SEED_ENV` 同名，
/// 此处复写以避免 `shared` 反向依赖 `bins`）。
pub const RNG_SEED_ENV: &str = "MIR2_RNG_SEED";

/// `attack_seed = base_seed + 0x9E37`（i32 回绕；两侧同口径）。
#[must_use]
pub fn attack_seed(base_seed: i32) -> i32 {
    base_seed.wrapping_add(ATTACK_SEED_DELTA)
}

/// 从进程环境解析攻击路径专用流种子：
///
/// - [`ISOLATE_ATTACK_ENV`] 未设/空/「关」值 ⇒ `None`（不隔离，生产缺省路径）；
/// - 「开」值 ⇒ 读 [`RNG_SEED_ENV`] 并返回 [`attack_seed`]；
/// - 「开」但 [`RNG_SEED_ENV`] 缺失/为空/非法 int，或 [`ISOLATE_ATTACK_ENV`] 取值认不出
///   ⇒ **panic（fail-closed）**：专用流种子无法与对侧对齐时不许静默退回全局流
///   （那会把「隔离后的逐 k=0」伪装成「没隔离的偶然相等」）。
#[must_use]
pub fn attack_isolation_seed_from_env() -> Option<i32> {
    let raw = std::env::var(ISOLATE_ATTACK_ENV).ok()?;
    let on = match raw.trim().to_ascii_lowercase().as_str() {
        "" | "0" | "false" | "off" | "no" => false,
        "1" | "true" | "on" | "yes" => true,
        other => panic!(
            "{ISOLATE_ATTACK_ENV} 取值非法: {other:?} \
             （只认 1/true/on/yes 与 0/false/off/no/空；别把认不出的值当开或当关）"
        ),
    };
    if !on {
        return None;
    }
    let seed_raw = match std::env::var(RNG_SEED_ENV) {
        Ok(s) if !s.is_empty() => s,
        _ => panic!(
            "{ISOLATE_ATTACK_ENV} 已开启但 {RNG_SEED_ENV} 未设/为空 \
             ⇒ 无法派生专用流种子（fail-closed）"
        ),
    };
    let base: i32 = seed_raw
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("{RNG_SEED_ENV} 不是合法 int: {seed_raw:?}（fail-closed）"));
    Some(attack_seed(base))
}

/// `System.Random(int seed)` 的逐行复刻（legacy 减法算法）。
///
/// 状态布局与 .NET 一致：`seed_array[56]`、`inext`、`inextp`。
#[derive(Clone, Debug)]
pub struct DotNetRandom {
    seed_array: [i32; 56],
    inext: usize,
    inextp: usize,
}

impl DotNetRandom {
    /// 对应 `new Random(int Seed)`。
    pub fn new(seed: i32) -> Self {
        let mut seed_array = [0i32; 56];
        // .NET: int subtraction = (Seed == Int32.MinValue) ? Int32.MaxValue : Math.Abs(Seed);
        let subtraction = if seed == i32::MIN {
            i32::MAX
        } else {
            seed.abs()
        };
        let mut mj = MSEED - subtraction;
        seed_array[55] = mj;
        let mut mk = 1i32;
        for i in 1..55usize {
            let ii = (21 * i) % 55;
            seed_array[ii] = mk;
            mk = mj - mk;
            if mk < 0 {
                mk += MBIG;
            }
            mj = seed_array[ii];
        }
        for _ in 1..5 {
            for i in 1..56usize {
                seed_array[i] -= seed_array[1 + (i + 30) % 55];
                if seed_array[i] < 0 {
                    seed_array[i] += MBIG;
                }
            }
        }
        DotNetRandom {
            seed_array,
            inext: 0,
            inextp: 21,
        }
    }

    /// 对应 `private int InternalSample()`。
    fn internal_sample(&mut self) -> i32 {
        let mut loc_inext = self.inext + 1;
        let mut loc_inextp = self.inextp + 1;
        if loc_inext >= 56 {
            loc_inext = 1;
        }
        if loc_inextp >= 56 {
            loc_inextp = 1;
        }
        let mut ret_val = self.seed_array[loc_inext] - self.seed_array[loc_inextp];
        if ret_val == MBIG {
            ret_val -= 1;
        }
        if ret_val < 0 {
            ret_val += MBIG;
        }
        self.seed_array[loc_inext] = ret_val;
        self.inext = loc_inext;
        self.inextp = loc_inextp;
        ret_val
    }

    /// 对应 `protected virtual double Sample()` = `InternalSample() * (1.0 / MBIG)`。
    fn sample(&mut self) -> f64 {
        self.internal_sample() as f64 * (1.0 / f64::from(MBIG))
    }

    /// 对应 `public virtual int Next()`。
    pub fn next_value(&mut self) -> i32 {
        self.internal_sample()
    }

    /// 对应 `public virtual int Next(int maxValue)`（返回 `[0, maxValue)`）。
    ///
    /// `maxValue < 0` 时 .NET 抛 `ArgumentOutOfRangeException`，此处 panic 对齐。
    pub fn next_max(&mut self, max_value: i32) -> i32 {
        assert!(
            max_value >= 0,
            "maxValue 必须非负（对应 .NET ArgumentOutOfRangeException）"
        );
        (self.sample() * f64::from(max_value)) as i32
    }

    /// 对应 `public virtual int Next(int minValue, int maxValue)`（返回 `[minValue, maxValue)`）。
    ///
    /// `minValue > maxValue` 时 .NET 抛异常，此处 panic 对齐。
    ///
    /// 大范围分支（`range > Int32.MaxValue`）走 `GetSampleForLargeRange()`：
    /// **消耗两次采样**，第二次采样决定符号（.NET 参考实现）。
    /// 乘积先转 64 位再与 `minValue` 相加、最后截断回 i32——该形态由
    /// `golden/rng_large_range.txt`（.NET 实机 + 内部采样反射对拍）钉死：
    /// 先加后转（double 表达式一次截断）与先转后加（32 位）两种写法都对不上真值。
    pub fn next_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        assert!(
            min_value <= max_value,
            "minValue 不得大于 maxValue（对应 .NET 异常）"
        );
        let range = i64::from(max_value) - i64::from(min_value);
        if range <= i64::from(i32::MAX) {
            (self.sample() * range as f64) as i32 + min_value
        } else {
            let scaled = (self.get_sample_for_large_range() * range as f64) as i64;
            scaled.wrapping_add(i64::from(min_value)) as i32
        }
    }

    /// 对应 .NET `private double GetSampleForLargeRange()`：
    /// `result = InternalSample()`；`negative = InternalSample() % 2 == 0`；
    /// `d = (negative ? -result : result) + (Int32.MaxValue - 1)`；
    /// `d /= 2 * (uint)Int32.MaxValue - 1`。
    fn get_sample_for_large_range(&mut self) -> f64 {
        let result = self.internal_sample();
        let negative = self.internal_sample() % 2 == 0;
        let signed = if negative { -result } else { result };
        let mut d = f64::from(signed);
        d += f64::from(i32::MAX - 1);
        d /= 2.0 * f64::from(i32::MAX) - 1.0;
        d
    }

    /// 对应 `public virtual double NextDouble()`。
    pub fn next_double(&mut self) -> f64 {
        self.sample()
    }
}

impl Default for DotNetRandom {
    /// 不提供时间种子：种子必须显式给出，保证可复现。
    fn default() -> Self {
        Self::new(0)
    }
}

/// `src/OpenMir2/RandomNumber.cs` 的复刻（去掉单例与锁，种子显式注入）。
///
/// C# 侧是无状态单例 + 全局共享一个 `System.Random`；Rust 侧由调用方持有实例，
/// 方法与 C# 一一对应（ Rust 不支持重载，用后缀区分）：
///
/// | C# | Rust |
/// | --- | --- |
/// | `Random()` | [`RandomNumber::random`] |
/// | `Random(int value)` | [`RandomNumber::random_below`] |
/// | `Random(int min, int max)` | [`RandomNumber::random_range`] |
/// | `GetRandomNumber(min, max)` | [`RandomNumber::get_random_number`] |
/// | `RandomByte(byte value)` | [`RandomNumber::random_byte`] |
/// | `GenerateRandomNumber(len)` | [`RandomNumber::generate_random_number`] |
/// | `RandomSelect(list, count)` | [`RandomNumber::random_select`] |
#[derive(Clone, Debug)]
pub struct RandomNumber {
    rng: DotNetRandom,
}

/// C# `RandomNumber.Constant` 原样。
const CONSTANT: [char; 62] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i',
    'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B',
    'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U',
    'V', 'W', 'X', 'Y', 'Z',
];

impl RandomNumber {
    /// 用固定种子构造（C# 侧为私有构造 + 单例，种子由 `new Random()` 时间源给出）。
    ///
    /// **M3②③ 决策片**：构造时按环境变量 [`crate::rng_trace::TRACE_ENV`]（`MIR2_RNG_LOG`）
    /// 尝试开启逐次 RNG trace（只生效一次；未设 ⇒ 不开启，取数序列一字不改）。
    pub fn with_seed(seed: i32) -> Self {
        crate::rng_trace::enable_from_env();
        RandomNumber {
            rng: DotNetRandom::new(seed),
        }
    }

    /// C# `int Random()` → `random.Next()`（非负随机数）。
    #[track_caller]
    pub fn random(&mut self) -> i32 {
        let v = self.rng.next_value();
        if crate::rng_trace::enabled() {
            let loc = std::panic::Location::caller();
            crate::rng_trace::record("Next()", 0, i64::from(v), loc.file(), loc.line());
        }
        v
    }

    /// C# `int Random(int value)` → `random.Next(value)`，返回 `[0, value)`。
    ///
    /// C# 在 `value == int.MaxValue` 时抛 `Exception("错误的数值")`，
    /// `value < 0` 时由 `Next` 抛 `ArgumentOutOfRangeException`；均对齐为 panic。
    #[track_caller]
    pub fn random_below(&mut self, value: i32) -> i32 {
        assert!(value < i32::MAX, "错误的数值");
        let v = self.rng.next_max(value);
        if crate::rng_trace::enabled() {
            let loc = std::panic::Location::caller();
            crate::rng_trace::record(
                "Next(max)",
                i64::from(value),
                i64::from(v),
                loc.file(),
                loc.line(),
            );
        }
        v
    }

    /// C# `int Random(int minValue, int maxValue)` → `random.Next(min, max)`，
    /// 返回 `[min, max)`（**上界不含**，与 `get_random_number` 不同）。
    #[track_caller]
    pub fn random_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        let v = self.rng.next_range(min_value, max_value);
        if crate::rng_trace::enabled() {
            let loc = std::panic::Location::caller();
            // C# `RecordingRandom` 的 `Next(min,max)` 参数编码：`((long)min << 32) | (uint)max`。
            let arg = (i64::from(min_value) << 32) | i64::from(max_value as u32);
            crate::rng_trace::record("Next(min,max)", arg, i64::from(v), loc.file(), loc.line());
        }
        v
    }

    /// C# `GetRandomNumber(minValue, maxValue)` → `random.Next(min, max + 1)`，
    /// 返回 `[min, max]`（**上下界都含**）。
    ///
    /// C# 在 unchecked 上下文 `max + 1` 可溢出回绕，用 `wrapping_add` 对齐。
    #[track_caller]
    pub fn get_random_number(&mut self, min_value: i32, max_value: i32) -> i32 {
        // 复用 `random_range` ⇒ 取数形态/记录口径与 C# `Next(min, max+1)` 完全一致。
        self.random_range(min_value, max_value.wrapping_add(1))
    }

    /// C# `byte RandomByte(byte value)`，`value == 255` 时 C# 抛异常。
    #[track_caller]
    pub fn random_byte(&mut self, value: u8) -> u8 {
        assert!(value < u8::MAX, "错误的数值");
        // C# `RandomByte` → `random.Next(value)` ⇒ 记录形态同 `Next(max)`。
        self.random_below(i32::from(value)) as u8
    }

    /// C# `string GenerateRandomNumber(int Length)`：从 62 字符表逐位取随机字符。
    #[track_caller]
    pub fn generate_random_number(&mut self, length: usize) -> String {
        let mut out = String::with_capacity(length);
        for _ in 0..length {
            // C# `Random(62)` 逐位 ⇒ 记录形态同 `Next(max)`。
            out.push(CONSTANT[self.random_below(62) as usize]);
        }
        out
    }

    /// C# `IList<int> RandomSelect(IList<int>, int)`：不放回抽取 `select_count` 个。
    ///
    /// 注意 C# 每轮用 `GetRandomNumber(1, sourceList.Count)`（含上界）再减一取下标，
    /// 且会**移除**原列表元素——`source` 会被原地修改，与 C# 一致。
    #[track_caller]
    pub fn random_select(&mut self, source: &mut Vec<i32>, select_count: usize) -> Vec<i32> {
        assert!(
            select_count <= source.len(),
            "selectCount必需大于sourceList.Count（原文如此，C# 的异常消息写反了）"
        );
        let mut result = Vec::with_capacity(select_count);
        for _ in 0..select_count {
            let next_index = self.get_random_number(1, source.len() as i32);
            result.push(source.remove((next_index - 1) as usize));
        }
        result
    }
}

/// 随机源抽象：让数值公式既能吃"同种子实现"（[`RandomNumber`]），
/// 也能吃"记录-回放实现"（`crate::replay::ReplayRandom`）。
///
/// 方法集与 C# `RandomNumber` 的取数入口一一对应，见 [`RandomNumber`] 的对照表。
pub trait RandomSource {
    /// C# `int Random()`。
    #[track_caller]
    fn random(&mut self) -> i32;
    /// C# `int Random(int value)`：`[0, value)`。
    #[track_caller]
    fn random_below(&mut self, value: i32) -> i32;
    /// C# `int Random(int min, int max)`：`[min, max)`。
    #[track_caller]
    fn random_range(&mut self, min_value: i32, max_value: i32) -> i32;
    /// C# `double NextDouble()`（记录-回放需要）。
    #[track_caller]
    fn next_double(&mut self) -> f64;
    /// C# `GetRandomNumber(min, max)`：`[min, max]`（**含上界**）。
    #[track_caller]
    fn get_random_number(&mut self, min_value: i32, max_value: i32) -> i32 {
        self.random_range(min_value, max_value.wrapping_add(1))
    }
    /// C# `byte RandomByte(byte value)`。
    #[track_caller]
    fn random_byte(&mut self, value: u8) -> u8 {
        assert!(value < u8::MAX, "错误的数值");
        self.random_below(i32::from(value)) as u8
    }
    /// C# `string GenerateRandomNumber(int Length)`。
    #[track_caller]
    fn generate_random_number(&mut self, length: usize) -> String {
        let mut out = String::with_capacity(length);
        for _ in 0..length {
            out.push(CONSTANT[self.random_below(62) as usize]);
        }
        out
    }
    /// C# `IList<int> RandomSelect(IList<int>, int)`：不放回抽取，`source` 原地修改。
    #[track_caller]
    fn random_select(&mut self, source: &mut Vec<i32>, select_count: usize) -> Vec<i32> {
        assert!(
            select_count <= source.len(),
            "selectCount必需大于sourceList.Count"
        );
        let mut result = Vec::with_capacity(select_count);
        for _ in 0..select_count {
            let next_index = self.get_random_number(1, source.len() as i32);
            result.push(source.remove((next_index - 1) as usize));
        }
        result
    }
}

impl RandomSource for DotNetRandom {
    #[track_caller]
    fn random(&mut self) -> i32 {
        self.next_value()
    }

    #[track_caller]
    fn random_below(&mut self, value: i32) -> i32 {
        self.next_max(value)
    }

    #[track_caller]
    fn random_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        self.next_range(min_value, max_value)
    }

    #[track_caller]
    fn next_double(&mut self) -> f64 {
        DotNetRandom::next_double(self)
    }
}

impl RandomSource for RandomNumber {
    #[track_caller]
    fn random(&mut self) -> i32 {
        RandomNumber::random(self)
    }

    #[track_caller]
    fn random_below(&mut self, value: i32) -> i32 {
        RandomNumber::random_below(self, value)
    }

    #[track_caller]
    fn random_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        RandomNumber::random_range(self, min_value, max_value)
    }

    #[track_caller]
    fn next_double(&mut self) -> f64 {
        let v = self.rng.next_double();
        if crate::rng_trace::enabled() {
            let loc = std::panic::Location::caller();
            // C# `RecordingRandom` 的 `NextDouble()` 结果编码：IEEE-754 位型。
            crate::rng_trace::record(
                "NextDouble()",
                0,
                v.to_bits() as i64,
                loc.file(),
                loc.line(),
            );
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 大区间分支（`range > Int32.MaxValue`）的 .NET 实测值：`GetSampleForLargeRange()`
    /// 消耗**两次**采样并带随机正负号（B 线探针，2026-10-10）。
    #[test]
    fn large_range_matches_dotnet() {
        assert_eq!(
            DotNetRandom::new(42).next_range(i32::MIN, i32::MAX),
            1_434_747_709
        );
        assert_eq!(
            DotNetRandom::new(7).next_range(-2_000_000_000, 2_000_000_000),
            766_440_938
        );
    }

    /// 小/大区间不得互相串味：同一 seed 下先取小区间值再取大区间值，两者都要对。
    #[test]
    fn small_and_large_range_sequences() {
        // 生产路径是 RandomNumber.GetRandomNumber(min, max) = Next(min, max + 1)
        let mut r = RandomNumber::with_seed(42);
        let seq: Vec<i32> = (0..10).map(|_| r.get_random_number(1, 200)).collect();
        assert_eq!(seq, vec![134, 29, 26, 105, 34, 53, 145, 103, 35, 153]);
    }

    /// .NET `new Random(42).Next()` 的实测值（C# golden 生成器同序列首项）。
    /// 完整序列对拍见 tests/parity（金标准由真实 `RandomNumber.cs` 产出）。
    #[test]
    fn dotnet_random_seed42_smoke() {
        let mut r = DotNetRandom::new(42);
        assert_eq!(r.next_value(), 1_434_747_710);
    }

    /// M3②③ 隔离口径的种子派生必须逐位可复现，且**两侧同公式**（C# 影子 hook 复用同一常量）。
    #[test]
    fn attack_seed_derivation_is_stable() {
        assert_eq!(attack_seed(42), 42 + 0x9E37);
        // i32 回绕（.NET `int + int` unchecked 同口径）。
        assert_eq!(attack_seed(i32::MAX), i32::MAX.wrapping_add(0x9E37));
        assert_eq!(attack_seed(i32::MIN), i32::MIN.wrapping_add(0x9E37));
        // 专用流与全局流在同一基础种子上必须**可分辨**（否则隔离无意义）。
        assert_ne!(attack_seed(42), 42);
    }

    /// 专用流确实是一条独立推进的 `System.Random` 序列（首项与全局流不同、且可复现）。
    #[test]
    fn attack_stream_is_a_distinct_reproducible_sequence() {
        let mut a1 = DotNetRandom::new(attack_seed(42));
        let mut a2 = DotNetRandom::new(attack_seed(42));
        let mut global = DotNetRandom::new(42);
        let sa: Vec<i32> = (0..4).map(|_| a1.next_value()).collect();
        let sb: Vec<i32> = (0..4).map(|_| a2.next_value()).collect();
        let sg: Vec<i32> = (0..4).map(|_| global.next_value()).collect();
        assert_eq!(sa, sb, "同 attack_seed ⇒ 逐项可复现");
        assert_ne!(sa, sg, "专用流种子必须与全局种子不同源");
    }

    #[test]
    fn next_max_zero_returns_zero() {
        let mut r = DotNetRandom::new(1);
        assert_eq!(r.next_max(0), 0);
    }

    #[test]
    fn get_random_number_is_inclusive() {
        // 大量采样应能同时取到上下界。
        let mut r = RandomNumber::with_seed(7);
        let mut seen_min = false;
        let mut seen_max = false;
        for _ in 0..1000 {
            let v = r.get_random_number(1, 2);
            assert!((1..=2).contains(&v));
            seen_min |= v == 1;
            seen_max |= v == 2;
        }
        assert!(seen_min && seen_max);
    }

    #[test]
    fn random_select_removes_from_source() {
        let mut r = RandomNumber::with_seed(3);
        let mut src = vec![1, 2, 3, 4, 5];
        let picked = r.random_select(&mut src, 2);
        assert_eq!(picked.len(), 2);
        assert_eq!(src.len(), 3);
        for v in &picked {
            assert!(!src.contains(v));
        }
    }
}
