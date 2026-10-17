//! 魔号效果体的 **RNG 取数站点**（`MagPushArround` / `MagSaceMove` 的正面取数）。
//!
//! **为什么单独成文件（2026-10-17，魔号 8/21 接线卡）**：RNG 结构一致性门
//! （`tests/parity/m3/rng-trace-probe/consumer_groups.tsv` 的 `groups --require-closed`）
//! 按**站点字符串**归组，而 `rng_ledger_diff.py` 的 `RUST_ATTACK_RE` 把整个
//! `crates/world/src/magic\.rs:` 前缀视作**攻击路径**（历史语义漂移）。效果体的取数若留在
//! `magic.rs`，会被误归 `attack` 组 ⇒ 与 C# 侧 `MagicManager.MagPushArround` /
//! `MagSaceMove` 的独立站点不对称。把取数搬进本文件后：
//!
//! | 组（consumer_groups.tsv） | C# oracle | Rust |
//! | --- | --- | --- |
//! | `magic_effect` | `MagicManager.MagPushArround@MagicManager.cs:37/41`、`MagSaceMove@:879` | 本文件 |
//! | `merchant` | `BaseObject.MapRandomMove@BaseObject.cs:1235/1236`（经 `MagSaceMove` 调用） | `crate::merchant::map_random_move_xy` |
//! | `space_move` | `BaseObject.SpaceMoveGetRandXY@BaseObject.cs:1040` | `crate::space_move::space_move_get_rand_xy` |
//!
//! 取数式与次序（逐行锚 `src/M2Server/Magic/MagicManager.cs`）：
//! - `MagPushArround`（`:24-52`）：`:37` `Random(20) < 6 + nPushLevel*3 + levelgap`，
//!   仅**通过且** `IsProperTarget` 后 `:41` `Random(2)`（`push = 1 + MAX(0, level-1) + Random(2)`）。
//! - `MagSaceMove`（`:876-891`）：`:879` `Random(11) < nLevel*2 + 4`。
//!
//! 本模块只做**取数**；门/落点/位移在调用方（`crate::world` 与 `crate::magic` 的探针版）逐行实现。

use mir2_shared::rng::RandomSource;

/// `MagPushArround` 的第一抽：`Random(20) < 6 + nPushLevel * 3 + levelgap`（`MagicManager.cs:37`）。
#[must_use]
pub fn push_chance(rng: &mut impl RandomSource, push_level: i32, level_gap: i32) -> bool {
    rng.random_below(20) < 6 + push_level * 3 + level_gap
}

/// `MagPushArround` 的第二抽：`(byte)(1 + HUtil32._MAX(0, nPushLevel - 1) + Random(2))`（`:41`）。
#[must_use]
pub fn push_steps(rng: &mut impl RandomSource, push_level: i32) -> u8 {
    (1 + 0.max(push_level - 1) + rng.random_below(2)) as u8
}

/// `MagSaceMove` 的施放门：`Random(11) < nLevel * 2 + 4`（`:879`）。
#[must_use]
pub fn sace_move_gate(rng: &mut impl RandomSource, level: i32) -> bool {
    rng.random_below(11) < level * 2 + 4
}

/// `DoSpell` 共享尾的训练点抽样：`Random(3) + 1`（`MagicManager.cs:663`）。
///
/// 站点必须落在本文件：留在 `magic.rs` 会被 `RUST_ATTACK_RE`（整文件 `magic\.rs:`）误归
/// `attack` 组，而 C# oracle 的该抽在 `MagicManager.DoSpell`（非攻击路径方法）。
#[must_use]
pub fn spell_train_points(rng: &mut impl RandomSource) -> i32 {
    rng.random_below(3) + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_shared::rng::RandomNumber;

    /// 取数式逐值钉死（seed 固定；`Next(10)` 在多个索引同为 1 ⇒ 用**连续三抽**，不用单抽）。
    #[test]
    fn draw_formulas_and_order_source_cited() {
        let mut a = RandomNumber::with_seed(20261017);
        let mut b = RandomNumber::with_seed(20261017);
        // 同一条流：chance 先抽 20，再 steps 抽 2。期望表 = 独立复算（同 seed 的另一实例）。
        let chance = push_chance(&mut a, 3, 2);
        let steps = push_steps(&mut a, 3);
        let want_chance = b.random_below(20) < 6 + 3 * 3 + 2;
        let want_steps = (1 + 2 + b.random_below(2)) as u8;
        assert_eq!(chance, want_chance);
        assert_eq!(steps, want_steps);
        // 随后连续三抽也应逐项一致（证明前面恰好消费 2 个样本、次序未错位）。
        for _ in 0..3 {
            assert_eq!(a.random_below(10), b.random_below(10));
        }

        let mut c = RandomNumber::with_seed(20261017);
        let mut d = RandomNumber::with_seed(20261017);
        let got = sace_move_gate(&mut c, 2);
        let want = d.random_below(11) < 2 * 2 + 4;
        assert_eq!(got, want);
        for _ in 0..3 {
            assert_eq!(c.random_below(10), d.random_below(10));
        }
    }

    /// 红检（牙）：把公式改坏 ⇒ 随机取样的用例必红。
    #[test]
    fn teeth_wrong_threshold_must_fail() {
        // `6 + 0*3 + 0` = 6：seed 下 `Random(20)` 的样本会落在两侧。
        let mut bad = RandomNumber::with_seed(7);
        let mut good = RandomNumber::with_seed(7);
        let mut diff = false;
        for _ in 0..64 {
            let g = push_chance(&mut good, 0, 0);
            // 故意漏掉 `+6`（错式）。
            let b = bad.random_below(20) < 0;
            if g != b {
                diff = true;
                break;
            }
        }
        assert!(diff, "错式必须与正确式出现分歧（牙不成立）");
    }
}
