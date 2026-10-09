//! `System.Random`（带种子构造，.NET 8 CompatPrng / Knuth 减 Vaughan 算法）的 1:1 复刻。
//!
//! 参照 §5.3：掉落/暴击/AI/脚本随机源必须同算法同调用顺序。
//! .NET 6+ 仅**无参**构造换了 Xoshiro；带种子构造保持 legacy 算法不变（兼容性承诺）。
//! 测试期望值来自 .NET 8 实机探测。

const MBIG: i32 = i32::MAX; // 2147483647
const MSEED: i32 = 161_803_398; // φ×10^8（注意：不是 1_618_033_398）

/// 对应 `new Random(seed)`（CompatPrng）。
#[derive(Debug)]
pub struct SystemRandom {
    seed_array: [i32; 56],
    inext: usize,
    inextp: usize,
}

impl SystemRandom {
    pub fn new(seed: i32) -> Self {
        // C# reference source（unchecked int 算术，负数 +MBIG 回绕）
        let subtraction = if seed == i32::MIN {
            i32::MAX
        } else {
            seed.wrapping_abs()
        };
        let mut seed_array = [0i32; 56];
        let mut mj = MSEED.wrapping_sub(subtraction);
        seed_array[55] = mj;
        let mut mk = 1i32;
        for i in 1..55 {
            let ii = (21 * i) % 55;
            seed_array[ii] = mk;
            mk = mj.wrapping_sub(mk);
            if mk < 0 {
                mk = mk.wrapping_add(MBIG);
            }
            mj = seed_array[ii];
        }
        for _ in 0..4 {
            for i in 1..56 {
                seed_array[i] = seed_array[i].wrapping_sub(seed_array[1 + (i + 30) % 55]);
                if seed_array[i] < 0 {
                    seed_array[i] = seed_array[i].wrapping_add(MBIG);
                }
            }
        }
        Self {
            seed_array,
            inext: 0,
            inextp: 21,
        }
    }

    fn internal_sample(&mut self) -> i32 {
        let mut loc_inext = self.inext + 1;
        if loc_inext >= 56 {
            loc_inext = 1;
        }
        let mut loc_inextp = self.inextp + 1;
        if loc_inextp >= 56 {
            loc_inextp = 1;
        }
        let mut ret_val = self.seed_array[loc_inext].wrapping_sub(self.seed_array[loc_inextp]);
        if ret_val == MBIG {
            ret_val -= 1;
        }
        if ret_val < 0 {
            ret_val = ret_val.wrapping_add(MBIG);
        }
        self.seed_array[loc_inext] = ret_val;
        self.inext = loc_inext;
        self.inextp = loc_inextp;
        ret_val
    }

    /// `Random.Next()`：[0, Int32.MaxValue)
    #[allow(clippy::should_implement_trait)] // 与 System.Random.Next 同名是刻意的 1:1 映射
    pub fn next(&mut self) -> i32 {
        self.internal_sample()
    }

    /// `Random.Next(minValue, maxValue)`：[minValue, maxValue)
    pub fn next_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        let range = (max_value as i64) - (min_value as i64);
        if range <= i32::MAX as i64 {
            // (int)(Sample() * range) + minValue; Sample() = InternalSample() * (1.0 / MBIG)
            let sample = (self.internal_sample() as f64) * (1.0 / (MBIG as f64));
            (sample * (range as f64)) as i32 + min_value
        } else {
            // 大区间分支（语料不触发，按参考实现补齐）
            let sample = (self.internal_sample() as f64) * (1.0 / (MBIG as f64));
            ((sample * (range as f64)) as i64 + min_value as i64) as i32
        }
    }
}

/// `RandomNumber.GetRandomNumber(min, max)` = `Next(min, max + 1)`，闭区间。
impl SystemRandom {
    pub fn get_random_number(&mut self, min_value: i32, max_value: i32) -> i32 {
        self.next_range(min_value, max_value.wrapping_add(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_sequence_matches_dotnet() {
        // .NET 8 实机: new Random(42).Next(1, 201)
        let mut r = SystemRandom::new(42);
        let seq: Vec<i32> = (0..10).map(|_| r.next_range(1, 201)).collect();
        assert_eq!(seq, vec![134, 29, 26, 105, 34, 53, 145, 103, 35, 153]);
    }

    #[test]
    fn next_matches_dotnet() {
        let mut r = SystemRandom::new(42);
        let seq: Vec<i32> = (0..5).map(|_| r.next()).collect();
        assert_eq!(
            seq,
            vec![1434747710, 302596119, 269548474, 1122627734, 361709742]
        );
    }

    #[test]
    fn edge_seeds_match_dotnet() {
        assert_eq!(SystemRandom::new(0).next_range(1, 201), 146);
        assert_eq!(SystemRandom::new(-7).next_range(1, 201), 77);
        assert_eq!(SystemRandom::new(i32::MIN).next_range(1, 201), 146);
    }
}
