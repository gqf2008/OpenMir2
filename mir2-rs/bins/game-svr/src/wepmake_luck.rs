//! **IT-12 祝福油**（StdMode 3 / Shape 4）：`WeaptonMakeLuck()` 的纯决策。
//!
//! oracle：`PlayObject.Base.cs:2259`（`EatUseItems` 的 `case 4`）→ `PlayObject.cs:1500`
//! （原文拼写就是 `Weapton`）。
//!
//! # 语义（逐行照抄 `PlayObject.cs:1500-1555`）
//!
//! 1. 无武器（`UseItems[Weapon] == null || Index <= 0`）⇒ 函数返回 **false** ⇒ **不吃**；
//!    否则**恒返回 true**（中没中、诅咒与否都吃）。
//! 2. 先抽 `Random(WeaponMakeUnLuckRate)`（`:1512`）；`== 1` ⇒ **诅咒支**（`MakeWeaponUnlock`），
//!    **不再抽**：`Desc[3] > 0` ⇒ `Desc[3] -= 1`，否则 `Desc[4] < 10` ⇒ `Desc[4] += 1`。
//! 3. 否则按 `Desc[4]`/`Desc[3]` 的 else-if 链（`&&` 才抽，短路）：
//!    - `Desc[4] > 0` ⇒ `Desc[4] -= 1`（**不抽**）；
//!    - `Desc[3] < point1` ⇒ `Desc[3] += 1`（**不抽**）；
//!    - `Desc[3] < point2` 且 `Random(nRand + point2_rate) == 1` ⇒ `Desc[3] += 1`；
//!    - `Desc[3] < point3` 且 `Random(nRand * point3_rate) == 1` ⇒ `Desc[3] += 1`
//!      （**上一条抽了但没中时，这一条会再抽一次**）。
//! 4. `nRand = |HiByte(StdItem.DC) - LoByte(StdItem.DC)| / 5`（`:1509-1511`，整数除法）。
//!
//! **`Random(0)` 是合法输入**：.NET `Next(0)` 返回 0、**消耗一次抽样**；Rust
//! [`mir2_shared::rng::RandomNumber`] 同口径（`next_max(0)` 走 `sample()` 后乘 0）。
//! `nRand == 0` 时「点3」抽恒为 `Random(0) = 0` ⇒ 永不命中（但**仍抽**）。
//!
//! 状态面（帧上不可见，必须由宿主回写）：武器 `Desc[3]`（幸运）/ `Desc[4]`（诅咒计数，上限 10）。

use mir2_shared::hutil32::{hi_byte_u16, lo_byte_u16};
use mir2_shared::rng::RandomSource;

use crate::config::WeaponMakeLuckCfg;

/// `nRand = |HiByte(dc) - LoByte(dc)| / 5`（`PlayObject.cs:1509-1511`，整数除法）。
#[must_use]
pub fn luck_rand_from_dc(dc: u16) -> i32 {
    let hi = i32::from(hi_byte_u16(dc));
    let lo = i32::from(lo_byte_u16(dc));
    (hi - lo).abs() / 5
}

/// `WeaptonMakeLuck` 的三条出口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuckOutcome {
    /// 首抽 `== 1` ⇒ `MakeWeaponUnlock`：**红字** `TheWeaponIsCursed`。
    Cursed,
    /// 幸运/消诅咒成功 ⇒ **绿字** `WeaptonMakeLuck`。
    Made,
    /// 没中 ⇒ **绿字** `WeaptonNotMakeLuck`。
    NotMade,
}

/// 决策结果：`desc3`/`desc4` 为**回写值**（调用方写进 `UseItems[Weapon].Desc`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LuckResult {
    pub outcome: LuckOutcome,
    pub desc3: u8,
    pub desc4: u8,
}

/// `PlayObject.WeaptonMakeLuck()` 的纯决策半（RNG + `Desc` 迁移），**不含帧与 Recalc**。
///
/// 调用方必须先保证「有武器且 `Index > 0`」（oracle 的前置；否则根本不进本函数）。
#[must_use]
pub fn weapton_make_luck(
    rng: &mut impl RandomSource,
    cfg: &WeaponMakeLuckCfg,
    n_rand: i32,
    desc3: u8,
    desc4: u8,
) -> LuckResult {
    if rng.random_below(cfg.un_luck_rate) == 1 {
        // 诅咒支（`:1512-1515` → `MakeWeaponUnlock`，`PlayObject.cs:1601-1631`）。
        let (d3, d4) = if desc3 > 0 {
            (desc3.wrapping_sub(1), desc4)
        } else if desc4 < 10 {
            (desc3, desc4.wrapping_add(1))
        } else {
            (desc3, desc4)
        };
        return LuckResult {
            outcome: LuckOutcome::Cursed,
            desc3: d3,
            desc4: d4,
        };
    }
    let mut d3 = desc3;
    let mut d4 = desc4;
    let made = if d4 > 0 {
        // `:1519-1524`：先消诅咒（不抽）。
        d4 = d4.wrapping_sub(1);
        true
    } else if i32::from(d3) < cfg.point1 {
        // `:1525-1530`：点1 直接加（不抽）。
        d3 = d3.wrapping_add(1);
        true
    } else if i32::from(d3) < cfg.point2 && rng.random_below(n_rand + cfg.point2_rate) == 1 {
        // `:1531-1536`：点2 抽。
        d3 = d3.wrapping_add(1);
        true
    } else if i32::from(d3) < cfg.point3 && rng.random_below(n_rand * cfg.point3_rate) == 1 {
        // `:1537-1542`：点3 抽（`nRand * point3_rate`；可能为 0 ⇒ `Random(0) = 0` 恒不中）。
        d3 = d3.wrapping_add(1);
        true
    } else {
        false
    };
    LuckResult {
        outcome: if made {
            LuckOutcome::Made
        } else {
            LuckOutcome::NotMade
        },
        desc3: d3,
        desc4: d4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_shared::rng::RandomNumber;

    /// 脚本化随机源：按序返回给定值，并计数取数次数。
    struct Scripted {
        values: Vec<i32>,
        i: usize,
        draws: usize,
    }
    impl Scripted {
        fn new(values: &[i32]) -> Self {
            Self {
                values: values.to_vec(),
                i: 0,
                draws: 0,
            }
        }
    }
    impl RandomSource for Scripted {
        fn random(&mut self) -> i32 {
            self.random_below(1)
        }
        fn random_below(&mut self, _value: i32) -> i32 {
            self.draws += 1;
            let v = self.values[self.i];
            self.i += 1;
            v
        }
        fn random_range(&mut self, _min: i32, _max: i32) -> i32 {
            self.random_below(1)
        }
        fn next_double(&mut self) -> f64 {
            0.0
        }
    }

    fn cfg() -> WeaponMakeLuckCfg {
        WeaponMakeLuckCfg::default()
    }

    #[test]
    fn n_rand_is_abs_hi_lo_over_five() {
        assert_eq!(luck_rand_from_dc(0x0A05), 1); // |10-5|/5
        assert_eq!(luck_rand_from_dc(0x1400), 4); // |20-0|/5
        assert_eq!(luck_rand_from_dc(0), 0);
        assert_eq!(luck_rand_from_dc(0x0B09), 0); // |11-9|/5 = 0（整数除法）
    }

    #[test]
    fn curse_branch_subtracts_luck_first() {
        let mut r = Scripted::new(&[1]);
        let res = weapton_make_luck(&mut r, &cfg(), 2, 3, 0);
        assert_eq!(
            res,
            LuckResult {
                outcome: LuckOutcome::Cursed,
                desc3: 2,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 1, "诅咒支只抽一次");
    }

    #[test]
    fn curse_branch_at_luck_zero_raises_curse_count_capped_at_ten() {
        let mut r = Scripted::new(&[1]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 0, 4),
            LuckResult {
                outcome: LuckOutcome::Cursed,
                desc3: 0,
                desc4: 5
            }
        );
        // 上界：`Desc[4] < 10` 才加；已 10 ⇒ 不变、**仍是诅咒支**（不是 `Desc[4]--` 支）。
        let mut r = Scripted::new(&[1]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 0, 10),
            LuckResult {
                outcome: LuckOutcome::Cursed,
                desc3: 0,
                desc4: 10
            }
        );
        assert_eq!(r.draws, 1);
    }

    #[test]
    fn removes_curse_without_drawing() {
        let mut r = Scripted::new(&[0]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 0, 3),
            LuckResult {
                outcome: LuckOutcome::Made,
                desc3: 0,
                desc4: 2
            }
        );
        assert_eq!(r.draws, 1, "只有首抽");
    }

    #[test]
    fn point1_adds_luck_without_drawing() {
        let mut r = Scripted::new(&[0]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 0, 0),
            LuckResult {
                outcome: LuckOutcome::Made,
                desc3: 1,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 1);
    }

    #[test]
    fn point2_hit_draws_twice() {
        // desc3=1（>=point1, <point2）⇒ 点2 抽；命中。
        let mut r = Scripted::new(&[0, 1]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 1, 0),
            LuckResult {
                outcome: LuckOutcome::Made,
                desc3: 2,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 2);
    }

    #[test]
    fn point2_miss_then_point3_draws_three_times() {
        // 点2 抽不中 ⇒ 落到点3 再抽一次，命中。
        let mut r = Scripted::new(&[0, 0, 1]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 1, 0),
            LuckResult {
                outcome: LuckOutcome::Made,
                desc3: 2,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 3, "两次条件抽");
    }

    #[test]
    fn both_tiers_miss_is_not_made() {
        let mut r = Scripted::new(&[0, 0, 0]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 1, 0),
            LuckResult {
                outcome: LuckOutcome::NotMade,
                desc3: 1,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 3);
    }

    #[test]
    fn desc3_at_point3_skips_both_tiers_single_draw() {
        let mut r = Scripted::new(&[0]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 2, 7, 0),
            LuckResult {
                outcome: LuckOutcome::NotMade,
                desc3: 7,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 1, "点2/点3 的条件都不成立 ⇒ 不抽");
    }

    #[test]
    fn n_rand_zero_point3_is_dead_but_still_draws() {
        // desc3=1（>=point1、<point2）⇒ 点2 抽不中后**仍会**落到点3 再抽一次；
        // nRand=0 ⇒ 点3 的 `Random(0)` 恒 0（!=1）⇒ 不中。本例共 3 抽（含首抽）。
        let mut r = Scripted::new(&[0, 0, 0]);
        assert_eq!(
            weapton_make_luck(&mut r, &cfg(), 0, 1, 0),
            LuckResult {
                outcome: LuckOutcome::NotMade,
                desc3: 1,
                desc4: 0
            }
        );
        assert_eq!(r.draws, 3);
    }

    /// **`Random(0)` 跨语言口径**（opc-19 点名）：.NET 实测（PowerShell `System.Random`）为
    /// seed=1 ⇒ `Next(0)=0, Next(100)=11, 46, Next(30000)=23148`；seed=20261120 ⇒
    /// `0, 63, 32, 11637`。Rust 必须**返回值与后续序列都相同**（含「消耗一次抽样」）。
    /// 若这里红 ⇒ 是**真分歧**，不许在 Rust 侧加 `if bound == 0` 抹平。
    #[test]
    fn random_zero_matches_dotnet_value_and_stream() {
        let mut r = RandomNumber::with_seed(1);
        assert_eq!(r.random_below(0), 0);
        assert_eq!(r.random_below(100), 11);
        assert_eq!(r.random_below(100), 46);
        assert_eq!(r.random_below(30000), 23148);
        let mut r = RandomNumber::with_seed(20261120);
        assert_eq!(r.random_below(0), 0);
        assert_eq!(r.random_below(100), 63);
        assert_eq!(r.random_below(100), 32);
        assert_eq!(r.random_below(30000), 11637);
    }
}
