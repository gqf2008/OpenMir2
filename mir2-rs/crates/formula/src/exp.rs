//! 经验公式，移植源 `src/M2Server/Actor/BaseObject.cs`。

use mir2_shared::hutil;

/// C# `BaseObject.GetLevelExp`（`BaseObject.cs:1409`）：
///
/// ```csharp
/// if (nLevel <= Grobal2.MaxLevel) result = Config.NeedExps[nLevel];
/// else result = Config.NeedExps[Config.NeedExps.Length]; // BUG-REPLICA：恒越界
/// ```
///
/// BUG-REPLICA：`nLevel > 255` 时 C# 访问 `NeedExps[1000]`（长度 1000），
/// 抛 `IndexOutOfRangeException`。Rust 侧同样越界 panic，行为一致。
/// （实际调用方传入的是 byte 等级，永不触发该分支。）
///
/// `need_exps` 由 `mir2_data::exps::ExpsConfig` 从 `Exps.conf` 加载；
/// 英雄与人物共用同一张表（本代码库没有独立的英雄经验表）。
pub fn get_level_exp(need_exps: &[i32], max_level: i32, n_level: i32) -> i32 {
    if n_level <= max_level {
        need_exps[n_level as usize]
    } else {
        need_exps[need_exps.len()]
    }
}

/// C# `BaseObject.CalcGetExp`（`BaseObject.cs:712`）：击杀经验按等级差衰减。
///
/// - `abil_level`：攻击方（自己）等级；`n_level`：怪物等级；`n_exp`：怪物基础经验。
/// - `high_level_kill_mon_fix_exp`：`Config.HighLevelKillMonFixExp`。
/// - 结果 ≤ 0 时钳到 1。
pub fn calc_get_exp(
    abil_level: i32,
    n_level: i32,
    n_exp: i32,
    high_level_kill_mon_fix_exp: bool,
) -> i32 {
    let result = if high_level_kill_mon_fix_exp || abil_level < n_level + 10 {
        n_exp
    } else {
        n_exp - hutil::round(n_exp as f64 / 15.0 * (abil_level as f64 - (n_level as f64 + 10.0)))
    };
    if result <= 0 {
        1
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_data::models::MAX_LEVEL;

    #[test]
    fn level_exp_indexes_table() {
        let mut table = [0i32; 1000];
        table[1] = 10;
        table[255] = 9_999_999;
        assert_eq!(get_level_exp(&table, MAX_LEVEL, 1), 10);
        assert_eq!(get_level_exp(&table, MAX_LEVEL, 255), 9_999_999);
    }

    #[test]
    #[should_panic]
    fn level_exp_above_max_panics_like_csharp() {
        let table = [0i32; 1000];
        let _ = get_level_exp(&table, MAX_LEVEL, 256);
    }

    #[test]
    fn calc_get_exp_no_decay_when_fix_on() {
        assert_eq!(calc_get_exp(80, 10, 500, true), 500);
    }

    #[test]
    fn calc_get_exp_decay() {
        // abil 50, mon 20：50 >= 30 → 500 - Round(500/15*(50-30)) = 500 - Round(666.67) = -167 → 钳 1
        assert_eq!(calc_get_exp(50, 20, 500, false), 1);
        // abil 30, mon 20：30 >= 30 → 500 - Round(500/15*0) = 500
        assert_eq!(calc_get_exp(30, 20, 500, false), 500);
        // abil 31, mon 20：500 - Round(33.333) = 467
        assert_eq!(calc_get_exp(31, 20, 500, false), 467);
    }
}
