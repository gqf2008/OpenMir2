//! `HUtil32` 字符串原语的 1:1 移植（参照 `src/OpenMir2/HUtil32.cs`）。
//!
//! 行为等价优先：包括原实现中的缺陷（如 `IsStringNumber` 的 `||` 逻辑、
//! `CaptureString` 的 1-based 遗留索引、无闭合引号时抛异常）都原样保留。
//! 本文件中的预期值均来自对 .NET 8 实机探测（见 `tests/fixtures` 注释与各用例）。

/// C# 字符索引以 UTF-16 码元计；脚本语料均为 BMP 字符，用 `Vec<char>` 等效。
/// （语料中非 BMP 字符由 script-tool 的 corpus 检查单独报警。）
fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn slice_to_string(cs: &[char], start: usize, end: usize) -> String {
    cs[start..end].iter().collect()
}

/// C# `char.ToUpper`：简单大小写映射（不展开）。Rust `to_uppercase` 可能一对多，
/// 一对多时保持原字符，与 .NET 行为一致。
pub fn to_upper_char(c: char) -> char {
    let mut it = c.to_uppercase();
    match (it.next(), it.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

/// C# `string.ToUpper()`（逐字符简单映射）。
pub fn to_upper(s: &str) -> String {
    s.chars().map(to_upper_char).collect()
}

/// `HUtil32.StrToInt`：`int.TryParse(str, out result) ? result : def`。
/// .NET `int.TryParse` 默认允许前后空白与前导符号，溢出/含杂质失败。
pub fn str_to_int(s: &str, def: i32) -> i32 {
    s.trim().parse::<i32>().unwrap_or(def)
}

/// `HUtil32.StrToInt16`。
pub fn str_to_int16(s: &str, def: i16) -> i16 {
    s.trim().parse::<i16>().unwrap_or(def)
}

/// `HUtil32.IsStringNumber`：`!string.IsNullOrEmpty(str) || ValidationNumber().IsMatch(str)`。
/// 注意原实现的 `||` 缺陷：任何非空串都为 true；且正则 `^[+-]?\d*[.]?\d*$` 匹配空串，
/// 因此该函数对任意输入恒为 true。保持 1:1。
pub fn is_string_number(s: &str) -> bool {
    !s.is_empty() || validation_number_matches(s)
}

fn validation_number_matches(s: &str) -> bool {
    // ^[+-]?\d*[.]?\d*$
    let cs = chars(s);
    let mut i = 0;
    if i < cs.len() && (cs[i] == '+' || cs[i] == '-') {
        i += 1;
    }
    while i < cs.len() && cs[i].is_ascii_digit() {
        i += 1;
    }
    if i < cs.len() && cs[i] == '.' {
        i += 1;
    }
    while i < cs.len() && cs[i].is_ascii_digit() {
        i += 1;
    }
    i == cs.len()
}

fn is_div(c: char, divs: &[char]) -> bool {
    divs.contains(&c)
}

/// `HUtil32.GetValidStr3(source, ref dest, dividerAry)`：
/// `source.Split(div, 2, RemoveEmptyEntries)`，dest = 第一个非空段，返回第二个非空段。
///
/// .NET 8 实机语义（与 .NET Framework 直觉不同，已探测确认）：
/// 跳过分隔符取第一个 token 为 dest；返回值为“第二个 token 起点到串尾”的剩余部分。
/// 例：`'  abc  def'` → dest=`abc`，返回 `def`；`'a b c'` → dest=`a`，返回 `b c`。
pub fn get_valid_str3(source: &str, divs: &[char]) -> (String, String) {
    let cs = chars(source);
    let mut i = 0;
    while i < cs.len() && is_div(cs[i], divs) {
        i += 1;
    }
    let start = i;
    while i < cs.len() && !is_div(cs[i], divs) {
        i += 1;
    }
    let dest = slice_to_string(&cs, start, i);
    let mut k = i;
    while k < cs.len() && is_div(cs[k], divs) {
        k += 1;
    }
    let rest = slice_to_string(&cs, k, cs.len());
    (dest, rest)
}

/// `HUtil32.CaptureString` 抛出的异常（对应 .NET `IndexOutOfRangeException`）。
/// C# 侧无 try/catch 兜底，异常直接穿透解析器；Rust 侧以 `Err` 显式上抛。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureStringPanic;

impl std::fmt::Display for CaptureStringPanic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IndexOutOfRangeException (CaptureString)")
    }
}

impl std::error::Error for CaptureStringPanic {}

/// `HUtil32.CaptureString` 1:1 移植（含 1-based 遗留索引与越界抛错）。
///
/// 实机探测语义：
/// - 从 `c = 1` 起跳过 `' '`；若越界或遇 `source[c] == '"'` 前的判断越界 → panic；
/// - 引号分支：找下一个 `"` 闭合，找不到（扫描越界）→ panic；
/// - 非引号分支：找下一个 `' '` 为止，找不到 → panic；
/// - dest = `source[st-1 .. et]`（含起始引号），rest = `source[et+1 ..]`（len >= et+2 时）。
fn capture_string(source: &[char]) -> Result<(String, String), CaptureStringPanic> {
    let len = source.len();
    if len == 0 {
        return Ok((String::new(), String::new()));
    }
    let mut c = 1usize;
    // while (source[c] == ' ') —— c 越界时 C# 抛 IndexOutOfRange
    while {
        if c >= len {
            return Err(CaptureStringPanic);
        }
        source[c] == ' '
    } {
        c += 1;
    }
    // if (source[c] == '"' && c < len) —— C# 先取 source[c] 再比 c<len，c==len 时抛
    if c >= len {
        return Err(CaptureStringPanic);
    }
    let (st, et);
    if source[c] == '"' {
        st = c + 1;
        // for (i = c+1; i <= len; i++) —— i==len 时 source[i] 抛
        let mut i = c + 1;
        loop {
            if i >= len {
                return Err(CaptureStringPanic);
            }
            if source[i] == '"' {
                et = i - 1;
                break;
            }
            i += 1;
        }
    } else {
        st = c;
        let mut i = c;
        loop {
            if i >= len {
                return Err(CaptureStringPanic);
            }
            if source[i] == ' ' {
                et = i - 1;
                break;
            }
            i += 1;
        }
    }
    // rdstr = source.Substring(st - 1, et - st + 1) → [st-1, et)
    let dest = slice_to_string(source, st - 1, et);
    // if (len >= et + 2) result = source.Substring(et + 1, len - (et + 1))
    let rest = if len >= et + 2 {
        slice_to_string(source, et + 1, len)
    } else {
        String::new()
    };
    Ok((dest, rest))
}

/// `HUtil32.GetValidStrCap(str, ref dest, divider)`：
/// 空串 → ("", "")；首字符为 `"` → `CaptureString`；否则 `GetValidStr3`。
pub fn get_valid_str_cap(s: &str, divs: &[char]) -> Result<(String, String), CaptureStringPanic> {
    if s.is_empty() {
        return Ok((String::new(), String::new()));
    }
    let cs = chars(s);
    if cs[0] == '"' {
        capture_string(&cs)
    } else {
        Ok(get_valid_str3(s, divs))
    }
}

/// `HUtil32.ArrestStringEx(source, searchAfter, arrestBefore, ref arrestStr)`：
/// 返回 (arrestStr, rest)。单字符定界符（所有调用点均为单字符）。
///
/// 实机探测语义：
/// - 空源或 len<2 → ("", "")（arrestStr 不变，调用方传入什么就是什么；此处返回 ""）；
/// - 以 after 开头或 after 出现在 n>0 → 取其后的 span；在 span 中找 before：
///   找到 → arrestStr = before 之前内容，rest = before 之后内容；
///   找不到 → arrestStr 不变（""），rest = after + span；
/// - after 不在源中（或仅在 0 号位以外的判断失败）→ C# 循环 `sourceSpan[i-1]` 于 i=0 抛错，
///   被 catch 吞掉 → ("", "")。
pub fn arrest_string_ex(source: &str, after: char, before: char) -> (String, String) {
    if source.is_empty() {
        return (String::new(), String::new());
    }
    let cs = chars(source);
    if cs.len() < 2 {
        return (String::new(), String::new());
    }
    // C# 用 OrdinalIgnoreCase 的 IndexOf；对定界符（[ ] " 等）等价于精确匹配。
    let span_start = if cs[0] == after {
        Some(1)
    } else {
        cs.iter()
            .position(|&c| c == after)
            .filter(|&n| n > 0)
            .map(|n| n + 1)
    };
    match span_start {
        Some(s) => {
            let span = &cs[s..];
            match span.iter().position(|&c| c == before) {
                Some(p) => (span[..p].iter().collect(), span[p + 1..].iter().collect()),
                None => (
                    String::new(),
                    format!("{}{}", after, span.iter().collect::<String>()),
                ),
            }
        }
        None => (String::new(), String::new()),
    }
}

/// `HUtil32.CompareLStr(src, targ)`：以 targ 长度为前缀做大小写不敏感比较。
pub fn compare_lstr(src: &str, targ: &str) -> bool {
    let s: Vec<char> = chars(src);
    let t: Vec<char> = chars(targ);
    compare_lstr_n(&s, &t, t.len())
}

/// `HUtil32.CompareLStr(src, targ, compn)`。
pub fn compare_lstr_prefix(src: &str, targ: &str, compn: usize) -> bool {
    let s: Vec<char> = chars(src);
    let t: Vec<char> = chars(targ);
    compare_lstr_n(&s, &t, compn)
}

fn compare_lstr_n(src: &[char], targ: &[char], compn: usize) -> bool {
    if compn == 0 || src.len() < compn || targ.len() < compn {
        return false;
    }
    for i in 0..compn {
        if to_upper_char(src[i]) != to_upper_char(targ[i]) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // 以下来自 .NET 8 实机探测（script-parity 探针输出）的期望值。

    #[test]
    fn get_valid_str3_dotnet_semantics() {
        assert_eq!(get_valid_str3("a,,b", &[',']), ("a".into(), "b".into()));
        assert_eq!(
            get_valid_str3(" abc def", &[' ']),
            ("abc".into(), "def".into())
        );
        assert_eq!(
            get_valid_str3("  abc  def", &[' ']),
            ("abc".into(), "def".into())
        );
        assert_eq!(
            get_valid_str3("abc ", &[' ']),
            ("abc".into(), String::new())
        );
        assert_eq!(get_valid_str3("", &[' ']), (String::new(), String::new()));
        assert_eq!(get_valid_str3("a b c", &[' ']), ("a".into(), "b c".into()));
    }

    #[test]
    fn str_to_int_dotnet_semantics() {
        assert_eq!(str_to_int(" 12 ", 0), 12);
        assert_eq!(str_to_int("+12", 0), 12);
        assert_eq!(str_to_int("12a", -1), -1);
        assert_eq!(str_to_int("99999999999", -1), -1);
        assert_eq!(str_to_int("", -1), -1);
    }

    #[test]
    fn is_string_number_always_true() {
        // 原实现缺陷：|| 逻辑 + 空串匹配正则 → 恒 true
        assert!(is_string_number(""));
        assert!(is_string_number("abc"));
        assert!(is_string_number("-12.5"));
    }

    #[test]
    fn capture_string_probe_cases() {
        // Cap("祈福项链" 1) => dest=["祈福项链] rest=[ 1]
        let (d, r) = get_valid_str_cap("\"祈福项链\" 1", &[' ', '\t']).unwrap();
        assert_eq!(d, "\"祈福项链");
        assert_eq!(r, " 1");
        // Cap("祈福 项链" 1) => dest=["祈] rest=[ 项链" 1]
        let (d, r) = get_valid_str_cap("\"祈福 项链\" 1", &[' ', '\t']).unwrap();
        assert_eq!(d, "\"祈");
        assert_eq!(r, " 项链\" 1");
        // Cap(""abc" 1) => dest=["ab] rest=[" 1]
        let (d, r) = get_valid_str_cap("\"\"abc\" 1", &[' ', '\t']).unwrap();
        assert_eq!(d, "\"ab");
        assert_eq!(r, "\" 1");
        // 无闭合/无空格 → 抛 IndexOutOfRange
        assert!(get_valid_str_cap("\"abc", &[' ', '\t']).is_err());
        assert!(get_valid_str_cap("\"\" 1", &[' ', '\t']).is_err());
        assert!(get_valid_str_cap("\"", &[' ', '\t']).is_err());
        // 非引号走 GetValidStr3
        assert_eq!(
            get_valid_str_cap("checkitem 祈福项链 1", &[' ', '\t']).unwrap(),
            ("checkitem".into(), "祈福项链 1".into())
        );
        assert_eq!(
            get_valid_str_cap("  lead", &[' ', '\t']).unwrap(),
            ("lead".into(), String::new())
        );
        assert_eq!(
            get_valid_str_cap("", &[' ', '\t']).unwrap(),
            (String::new(), String::new())
        );
    }

    #[test]
    fn arrest_string_ex_probe_cases() {
        assert_eq!(
            arrest_string_ex("#CALL [\\游戏登陆\\QF.txt] @main", '[', ']'),
            ("\\游戏登陆\\QF.txt".into(), " @main".into())
        );
        assert_eq!(
            arrest_string_ex("[main]", '[', ']'),
            ("main".into(), String::new())
        );
        assert_eq!(
            arrest_string_ex("[main] TRUE", '[', ']'),
            ("main".into(), " TRUE".into())
        );
        assert_eq!(
            arrest_string_ex("abc[de]fg", '[', ']'),
            ("de".into(), "fg".into())
        );
        assert_eq!(
            arrest_string_ex("nobrackets", '[', ']'),
            (String::new(), String::new())
        );
        assert_eq!(
            arrest_string_ex("[onlyopen", '[', ']'),
            (String::new(), "[onlyopen".into())
        );
        assert_eq!(
            arrest_string_ex("\"祈福项链", '"', '"'),
            (String::new(), "\"祈福项链".into())
        );
        assert_eq!(
            arrest_string_ex("\"abc\"", '"', '"'),
            ("abc".into(), String::new())
        );
        assert_eq!(
            arrest_string_ex("", '[', ']'),
            (String::new(), String::new())
        );
        assert_eq!(
            arrest_string_ex("x", '[', ']'),
            (String::new(), String::new())
        );
        assert_eq!(
            arrest_string_ex("M2 999", '[', ']'),
            (String::new(), String::new())
        );
    }

    #[test]
    fn compare_lstr_probe_cases() {
        assert!(compare_lstr("#IFxxx", "#IF"));
        assert!(!compare_lstr("#I", "#IF"));
        assert!(compare_lstr("#if", "#IF"));
        assert!(compare_lstr_prefix("[NECKLACE]x", "[NECKLACE]", 4));
    }
}
