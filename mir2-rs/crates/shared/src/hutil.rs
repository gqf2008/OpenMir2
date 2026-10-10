//! `src/OpenMir2/HUtil32.cs` 中数值/解析等价物（只收 D 线用到的函数）。

/// C# `HUtil32.Round` = `Math.Round(r, MidpointRounding.AwayFromZero)` 再转 int。
///
/// Rust 的 `f64::round` 恰好是「一半远离零」，语义一致。
pub fn round(r: f64) -> i32 {
    r.round() as i32
}

/// C# `HUtil32.MakeWord(byte low, byte high)` / `(ushort low, ushort high)`：
/// `low | high << 8`（结果截断为 u16）。
pub fn make_word(low: u16, high: u16) -> u16 {
    low | (high << 8)
}

/// C# `HUtil32.MakeLong(short lowPart, short highPart)`：
/// `(ushort)low | high << 16`。
pub fn make_long_signed(low: i16, high: i16) -> i32 {
    i32::from(low as u16) | (i32::from(high) << 16)
}

/// C# `HUtil32.MakeLong(ushort lowPart, ushort highPart)`。
pub fn make_long_unsigned(low: u16, high: u16) -> i32 {
    i32::from(low) | (i32::from(high) << 16)
}

/// C# `HUtil32.StrToInt(string, int)` = `int.TryParse` 失败给默认值。
///
/// `int.TryParse` 允许首尾空白与可选符号；Rust `from_str` 不允许空白，先 trim。
pub fn str_to_int(s: &str, def: i32) -> i32 {
    // 统一实现：crate::hutil32（1:1 移植，含 .NET TryParse 的空白/符号语义）
    crate::hutil32::str_to_int(s, def)
}

/// C# `HUtil32.GetValidStr3(string, ref string, char[])`：
/// 按分隔符集合切 2 段、去掉空段，`dest` 取第一段，返回剩余段。
///
/// 对应 `source.Split(div, 2, StringSplitOptions.RemoveEmptyEntries)`：
/// 前导连续分隔符整体跳过（空段全部移除），`dest` 为第一个非空段，
/// 返回值为该段之后、第一个分隔符之后的原始剩余串（不 trim）。
pub fn get_valid_str3(source: &str, dividers: &[char]) -> (String, String) {
    // 统一实现：crate::hutil32（按 .NET `Split(div, 2, RemoveEmptyEntries)` 实机语义：
    // 前导分隔符跳过、返回值从第二个 token 起——连续分隔符不会留在返回值里）
    crate::hutil32::get_valid_str3(source, dividers)
}

/// C# `HUtil32.ArrestStringEx(source, "\"", "\"", ref dest)` 在 MonItems 解析里的用法：
/// 串以 `"` 开头时，取出两个引号之间的内容写回 `dest`。
///
/// 返回 `Some((截取内容, 余下部分))`；不匹配时返回 `None`。
pub fn arrest_string_ex_quoted(source: &str) -> Option<(String, String)> {
    if source.len() < 2 {
        return None;
    }
    let stripped = source.strip_prefix('"');
    let Some(span) = stripped else {
        // C# 分支：未以 `"` 开头时按 IndexOf 找——MonItems 只在首字符是 `"` 时调用，
        // 其余情况不进入此函数，这里保守返回 None。
        return None;
    };
    let end = span.find('"')?;
    let arrested = &span[..end];
    let rest = &span[end + 1..];
    Some((arrested.to_string(), rest.to_string()))
}

/// C# `HUtil32._MIN(int, int)`。
pub fn min_i32(n1: i32, n2: i32) -> i32 {
    n1.min(n2)
}

/// C# `HUtil32._MAX(int, int)`。
pub fn max_i32(n1: i32, n2: i32) -> i32 {
    n1.max(n2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_away_from_zero() {
        assert_eq!(round(2.5), 3);
        assert_eq!(round(-2.5), -3);
        assert_eq!(round(2.4), 2);
        assert_eq!(round(-2.4), -2);
    }

    #[test]
    fn make_word_layout() {
        assert_eq!(make_word(0x01, 0x02), 0x0201);
        // C# (ushort)(bLow | bHigh << 8)：high 超 8 位会被截断
        assert_eq!(make_word(0, 0x1FF), 0xFF00);
    }

    #[test]
    fn str_to_int_matches_tryparse() {
        assert_eq!(str_to_int(" 42 ", -1), 42);
        assert_eq!(str_to_int("-7", 0), -7);
        assert_eq!(str_to_int("1a", -1), -1);
        assert_eq!(str_to_int("", -1), -1);
        assert_eq!(str_to_int("99999999999", -1), -1); // 超 i32 → default
    }

    #[test]
    fn get_valid_str3_splits() {
        // "10/300 金币 300" 按 [' ', '/', '\t'] 切
        let (dest, rest) = get_valid_str3("10/300 金币 300", &[' ', '/', '\t']);
        assert_eq!(dest, "10");
        assert_eq!(rest, "300 金币 300");
        // 前导分隔符：C# RemoveEmptyEntries → dest="300"
        let (dest, rest) = get_valid_str3(" 300 金币", &[' ', '\t']);
        assert_eq!(dest, "300");
        assert_eq!(rest, "金币");
    }

    #[test]
    fn arrest_quoted() {
        let (name, rest) = arrest_string_ex_quoted("\"屠龙刀\" 1").unwrap();
        assert_eq!(name, "屠龙刀");
        assert_eq!(rest, " 1");
        assert!(arrest_string_ex_quoted("屠龙刀").is_none());
    }
}
