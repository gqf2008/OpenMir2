//! 脚本变量层：`SystemShare.GetValNameNo` 与 `ConditionProcessingSys.CheckVarNameNo` 的 1:1 移植。
//!
//! `GetValNameNo` 把 `P3 / G12 / M5 / S01 ...` 这类变量名解析成全局槽号；
//! `CheckVarNameNo` 再按槽号区间取玩家/全局存储的值并与参数比较。

use mir2_shared::hutil32::str_to_int;

/// `SystemShare.GetValNameNo(sText)`（前缀 P/G/M/I/D/N/S/A/T/E/W，含 C# 的长度分支与偏移）。
pub fn get_val_name_no(text: &str) -> i32 {
    let cs: Vec<char> = text.chars().collect();
    if cs.len() < 2 {
        return -1;
    }
    let val_type = mir2_shared::hutil32::to_upper_char(cs[0]);
    // C# 用 `sText.Length`（UTF-16 码元数）；语料变量名均为 ASCII，chars 等价
    let len = cs.len();
    let digits = |from: usize, count: usize| -> i32 {
        let s: String = cs[from..(from + count).min(len)].iter().collect();
        str_to_int(&s, -1)
    };
    let single = |idx: usize| -> i32 { str_to_int(&cs[idx].to_string(), -1) };
    // 命中返回槽号；未命中返回 -1（保持 C# 的 result 初值语义）
    match val_type {
        'P' => {
            if len == 3 {
                let n = digits(1, 2);
                if (0..100).contains(&n) {
                    return n;
                }
            } else {
                let n = single(1);
                if (0..10).contains(&n) {
                    return n;
                }
            }
        }
        'G' => {
            // C# 是两个**独立** if（不是 else-if）：len==4 命中 +700 后，
            // 仍会落入下面的 else（len != 3）分支按首位数覆盖成 +100。
            let mut result = -1;
            if len == 4 {
                let n = digits(1, 3);
                if n > 99 && n < 500 {
                    result = n + 700;
                }
            }
            if len == 3 {
                let n = digits(1, 2);
                if (0..100).contains(&n) {
                    result = n + 100;
                }
            } else {
                let n = single(1);
                if (0..10).contains(&n) {
                    result = n + 100;
                }
            }
            return result;
        }
        'M' => {
            let n = if len == 3 { digits(1, 2) } else { single(1) };
            let ok = if len == 3 {
                (0..100).contains(&n)
            } else {
                (0..10).contains(&n)
            };
            if ok {
                return n + 300;
            }
        }
        'I' => {
            let n = if len == 3 { digits(1, 2) } else { single(1) };
            let ok = if len == 3 {
                (0..100).contains(&n)
            } else {
                (0..10).contains(&n)
            };
            if ok {
                return n + 400;
            }
        }
        'D' => {
            let n = if len == 3 { digits(1, 2) } else { single(1) };
            let ok = if len == 3 {
                (0..100).contains(&n)
            } else {
                (0..10).contains(&n)
            };
            if ok {
                return n + 200;
            }
        }
        'N' => {
            let n = if len == 3 { digits(1, 2) } else { single(1) };
            let ok = if len == 3 {
                (0..100).contains(&n)
            } else {
                (0..10).contains(&n)
            };
            if ok {
                return n + 500;
            }
        }
        'S' => {
            let n = if len == 3 { digits(1, 2) } else { single(1) };
            let ok = if len == 3 {
                (0..100).contains(&n)
            } else {
                (0..10).contains(&n)
            };
            if ok {
                return n + 600;
            }
        }
        'A' => {
            if len == 4 {
                let n = digits(1, 3);
                if n > 99 && n < 500 {
                    return n + 1100;
                }
            } else if len == 3 {
                let n = digits(1, 2);
                if (0..100).contains(&n) {
                    return n + 700;
                }
            } else {
                let n = single(1);
                if (0..10).contains(&n) {
                    return n + 700;
                }
            }
        }
        'T' => {
            let n = if len == 3 { digits(1, 2) } else { single(1) };
            let ok = if len == 3 {
                (0..100).contains(&n)
            } else {
                (0..10).contains(&n)
            };
            if ok {
                return n + 700;
            }
        }
        'E' => {
            let n = if len == 3 { digits(1, 2) } else { single(1) };
            let ok = if len == 3 {
                (0..100).contains(&n)
            } else {
                (0..10).contains(&n)
            };
            if ok {
                return n + 1600;
            }
        }
        'W' => {
            let n = if len == 3 { digits(1, 2) } else { single(1) };
            let ok = if len == 3 {
                (0..100).contains(&n)
            } else {
                (0..10).contains(&n)
            };
            if ok {
                return n + 1700;
            }
        }
        _ => {}
    }
    -1
}

/// `CheckVarNameNo` 的取值结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarLookup {
    /// 数值型：`(n140, n180)`
    Number(i32, i32),
    /// 字符串型命中（槽号 2000..2499 / 1400..1499）：`(0, 0)`
    StringHit,
}

/// `CheckVarNameNo`：按槽号区间解析数值/字符串变量。
///
/// `global_val` / `globa_dy_mval` / `global_a_val` 为 `SystemShare.Config` 侧存储，
/// `player_*` 为玩家侧存储。返回 `None` 表示未命中（C# `success = false`）。
#[allow(clippy::too_many_arguments)]
pub fn check_var_name_no(
    param1: &str,
    param2: &str,
    global_val: &dyn Fn(usize) -> i32,
    globa_dy_mval: &dyn Fn(usize) -> i32,
    global_a_val: &dyn Fn(usize) -> String,
    player_mn_val: &dyn Fn(usize) -> i32,
    player_mdy_val: &dyn Fn(usize) -> i32,
    player_mnmval: &dyn Fn(usize) -> i32,
    player_mn_integer: &dyn Fn(usize) -> i32,
    player_ms_string: &dyn Fn(usize) -> String,
) -> Option<VarLookup> {
    let n100 = get_val_name_no(param1);
    if n100 <= 0 {
        return None;
    }
    let target = str_to_int(param2, 0);
    if (0..=99).contains(&n100) {
        return Some(VarLookup::Number(global_val(n100 as usize), target));
    }
    if (1000..=1099).contains(&n100) {
        return Some(VarLookup::Number(
            globa_dy_mval((n100 - 1000) as usize),
            target,
        ));
    }
    if (1100..=1109).contains(&n100) {
        return Some(VarLookup::Number(
            player_mn_val((n100 - 1100) as usize),
            target,
        ));
    }
    if (1110..=1119).contains(&n100) {
        return Some(VarLookup::Number(
            player_mdy_val((n100 - 1110) as usize),
            target,
        ));
    }
    if (1200..=1299).contains(&n100) {
        return Some(VarLookup::Number(
            player_mnmval((n100 - 1200) as usize),
            target,
        ));
    }
    if (1300..=1399).contains(&n100) {
        return Some(VarLookup::Number(
            player_mn_integer((n100 - 1300) as usize),
            target,
        ));
    }
    if (2000..=2499).contains(&n100) {
        if global_a_val((n100 - 2000) as usize).eq_ignore_ascii_case(param2) {
            return Some(VarLookup::StringHit);
        }
        return None;
    }
    if (1400..=1499).contains(&n100) {
        if player_ms_string((n100 - 1400) as usize).eq_ignore_ascii_case(param2) {
            return Some(VarLookup::StringHit);
        }
        return None;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_val_name_no_prefixes() {
        assert_eq!(get_val_name_no("P3"), 3);
        assert_eq!(get_val_name_no("p12"), 12);
        assert_eq!(get_val_name_no("G5"), 105);
        // C# 语义：len==4 的 +700 被随后的 else 分支覆盖为 +100
        assert_eq!(get_val_name_no("G123"), 101);
        assert_eq!(get_val_name_no("M7"), 307);
        assert_eq!(get_val_name_no("I7"), 407);
        assert_eq!(get_val_name_no("D7"), 207);
        assert_eq!(get_val_name_no("N7"), 507);
        assert_eq!(get_val_name_no("S7"), 607);
        assert_eq!(get_val_name_no("T7"), 707);
        assert_eq!(get_val_name_no("E7"), 1607);
        assert_eq!(get_val_name_no("W7"), 1707);
        assert_eq!(get_val_name_no("A7"), 707);
        assert_eq!(get_val_name_no("A123"), 1223);
        // C# 的 P 分支：len != 3 一律走「取首位数」分支 ⇒ P100 解析为首位 1
        assert_eq!(get_val_name_no("P100"), 1);
        assert_eq!(get_val_name_no("X"), -1);
        assert_eq!(get_val_name_no("X"), -1);
        assert_eq!(get_val_name_no(""), -1);
    }
}
