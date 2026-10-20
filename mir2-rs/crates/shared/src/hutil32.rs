//! `HUtil32` 字符串原语的 1:1 移植（参照 `src/OpenMir2/HUtil32.cs`）。
//!
//! 本模块是全 workspace 的唯一实现（B 线脚本引擎、D 线数据层共用）；
//! [`crate::hutil`] 中的同名便捷函数一律委托到这里。
//!
//! 行为等价优先：包括原实现中的缺陷（如 `IsStringNumber` 的 `||` 逻辑、
//! `CaptureString` 的 1-based 遗留索引、无闭合引号时抛异常、`ReplaceChar`
//! 实际不改写返回串）都原样保留。
//! 本文件中的预期值均来自对 .NET 8 实机探测（探针源码与输出见
//! `docs/HUtil32-parity-inventory-20261020.md`）。
//!
//! 成员清册（oracle 76 个公共成员逐项对照）见同一份报告。

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// C# `HUtil32.Backslash = '/'`（注意是**正斜杠**，不是 `\`）。
pub const BACKSLASH: char = '/';

/// C# `HUtil32.Separator = { ' ', ',', '\t' }`。
pub const SEPARATOR: [char; 3] = [' ', ',', '\t'];

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

/// .NET `string.Compare(a, b, StringComparison.OrdinalIgnoreCase)` 的**码位差**语义。
///
/// 实测（.NET 8，见 `guild.rs` 的 C7 矩阵）：返回**首个不同的 UTF-16 码元差**；前缀关系
/// 时返回**码元长度差**（`ab`/`a` ⇒ 1、`a`/`ab` ⇒ -1）。大小写折叠逐码元走
/// [`to_upper_char`]（.NET ordinal-ignore-case 的简单映射口径），不展开、不做文化相关比较。
///
/// `GuildState::del_human_obj_member` 用它判 `== 1`（`GuildInfo.cs:540` 的 `string.Compare(...)
/// == 1`），与 `Ordering::Greater` 不等价（后者会把 `c/a = 2` 也当作命中）。
#[must_use]
pub fn dotnet_compare_ordinal_ignore_case(a: &str, b: &str) -> i32 {
    let au: Vec<u16> = a.encode_utf16().collect();
    let bu: Vec<u16> = b.encode_utf16().collect();
    let n = au.len().min(bu.len());
    for i in 0..n {
        let x = upper_code_unit(au[i]);
        let y = upper_code_unit(bu[i]);
        if x != y {
            return i32::from(x) - i32::from(y);
        }
    }
    i32::try_from(au.len()).unwrap_or(i32::MAX) - i32::try_from(bu.len()).unwrap_or(i32::MAX)
}

/// 单个 UTF-16 码元的大小写折叠；代理项（不可单独成 `char`）原样返回。
fn upper_code_unit(u: u16) -> u16 {
    match char::decode_utf16([u]).next() {
        Some(Ok(c)) => {
            let mut buf = [0u16; 2];
            to_upper_char(c).encode_utf16(&mut buf)[0]
        }
        _ => u,
    }
}

// ---------------------------------------------------------------------------
// 位/字节打包（oracle 的 MakeLong / MakeWord / HiWord / LoWord / HiByte / LoByte）
// ---------------------------------------------------------------------------

/// `HUtil32.MakeLong(short lowPart, short highPart)`：`(ushort)lowPart | highPart << 16`。
#[must_use]
pub fn make_long(low: i16, high: i16) -> i32 {
    i32::from(low as u16) | (i32::from(high) << 16)
}

/// `HUtil32.MakeLong(ushort lowPart, ushort highPart)`。
#[must_use]
pub fn make_long_u16(low: u16, high: u16) -> i32 {
    i32::from(low) | (i32::from(high) << 16)
}

/// `HUtil32.MakeWord(byte low, byte high)`：`(ushort)(high << 8 | low)`。
#[must_use]
pub fn make_word(low: u8, high: u8) -> u16 {
    u16::from(low) | (u16::from(high) << 8)
}

/// `HUtil32.MakeWord(ushort bLow, ushort bHigh)`：`(ushort)(bLow | bHigh << 8)`
/// （`bHigh` 超过 8 位时高位被截断，保持 oracle 行为）。
#[must_use]
pub fn make_word_u16(low: u16, high: u16) -> u16 {
    low | (high << 8)
}

/// `HUtil32.HiWord(ushort dword)`：`(ushort)(dword >> 16)`。
/// `ushort` 先提升为 `int` 再右移 16 位 ⇒ **恒为 0**（oracle 的真实行为）。
#[must_use]
pub fn hi_word_u16(dword: u16) -> u16 {
    ((u32::from(dword)) >> 16) as u16
}

/// `HUtil32.HiWord(int dword)`：`(ushort)(dword >> 16)`（算术右移）。
#[must_use]
pub fn hi_word_i32(dword: i32) -> u16 {
    (dword >> 16) as u16
}

/// `HUtil32.HiWord(uint nValue)`：`(ushort)(nValue >> 16)`。
#[must_use]
pub fn hi_word_u32(n_value: u32) -> u16 {
    (n_value >> 16) as u16
}

/// `HUtil32.LoWord(int dword)`：`(ushort)dword`（截低 16 位）。
#[must_use]
pub fn lo_word_i32(dword: i32) -> u16 {
    dword as u16
}

/// `HUtil32.LoWord(uint nValue)`：`(ushort)(nValue & 0xFFFF)`。
#[must_use]
pub fn lo_word_u32(n_value: u32) -> u16 {
    (n_value & 0xFFFF) as u16
}

/// `HUtil32.HiByte(short w)`：`(byte)(w >> 8)`（算术右移后截低 8 位）。
#[must_use]
pub fn hi_byte_i16(w: i16) -> u8 {
    (w >> 8) as u8
}

/// `HUtil32.HiByte(int w)`：`(byte)(w >> 8)`。
#[must_use]
pub fn hi_byte_i32(w: i32) -> u8 {
    (w >> 8) as u8
}

/// `HUtil32.HiByte(ushort nValue)`：`(byte)(nValue >> 8)`。
#[must_use]
pub fn hi_byte_u16(n_value: u16) -> u8 {
    (n_value >> 8) as u8
}

/// `HUtil32.LoByte(short w)`：`(byte)w`。
#[must_use]
pub fn lo_byte_i16(w: i16) -> u8 {
    w as u8
}

/// `HUtil32.LoByte(int w)`：`(byte)w`。
#[must_use]
pub fn lo_byte_i32(w: i32) -> u8 {
    w as u8
}

/// `HUtil32.LoByte(ushort nValue)`：`(byte)(nValue & 0xFF)`。
#[must_use]
pub fn lo_byte_u16(n_value: u16) -> u8 {
    (n_value & 0xFF) as u8
}

// ---------------------------------------------------------------------------
// 数值/范围/布尔
// ---------------------------------------------------------------------------

/// C# `(int)Math.Round(r, MidpointRounding.AwayFromZero)`。
///
/// 关键边界（.NET 8 x64 实机探测）：`Math.Round` 返回 double，随后**非检查转换**；
/// 超出 `int` 范围、`NaN`、`±∞` 一律得 `INT_MIN`（`-2147483648`）。Rust 的 `as i32`
/// 是饱和转换（正溢出得 `INT_MAX`），**不等价**，故显式分支。
#[must_use]
pub fn round(r: f64) -> i32 {
    let x = r.round(); // 一半远离零，与 MidpointRounding.AwayFromZero 一致
    if x.is_nan() || x < -2_147_483_648.0 || x >= 2_147_483_648.0 {
        i32::MIN
    } else {
        x as i32
    }
}

/// `HUtil32.RangeInDefined(int values, int min, int max)`：
/// `Math.Max(min, values) == Math.Min(values, max)`（与 `min <= v <= max` 等价，
/// 但 `min > max` 时 oracle 恒为 false —— 保持原式）。
#[must_use]
pub fn range_in_defined_i32(values: i32, min: i32, max: i32) -> bool {
    min.max(values) == values.min(max)
}

/// `HUtil32.RangeInDefined(long values, int min, int max)`（两侧提升为 long 比较）。
#[must_use]
pub fn range_in_defined_i64(values: i64, min: i64, max: i64) -> bool {
    min.max(values) == values.min(max)
}

/// `HUtil32.RangeInDefined(byte values, int min, int max)`（byte 提升为 int）。
#[must_use]
pub fn range_in_defined_u8(values: u8, min: i32, max: i32) -> bool {
    let v = i32::from(values);
    min.max(v) == v.min(max)
}

/// `HUtil32._MIN(int, int)`。
#[must_use]
pub fn min_i32(n1: i32, n2: i32) -> i32 {
    n1.min(n2)
}

/// `HUtil32._MAX(int, int)`。
#[must_use]
pub fn max_i32(n1: i32, n2: i32) -> i32 {
    n1.max(n2)
}

/// `HUtil32.BoolToStr(bool)`：`"TRUE"` / `"FALSE"`。
#[must_use]
pub fn bool_to_str(b: bool) -> &'static str {
    if b {
        "TRUE"
    } else {
        "FALSE"
    }
}

/// `HUtil32.BoolToCStr(bool)`：`"是"` / `"否"`。
#[must_use]
pub fn bool_to_cstr(b: bool) -> &'static str {
    if b {
        "是"
    } else {
        "否"
    }
}

/// `HUtil32.BoolToIntStr(bool)`：`"1"` / `"0"`。
#[must_use]
pub fn bool_to_int_str(b: bool) -> &'static str {
    if b {
        "1"
    } else {
        "0"
    }
}

// ---------------------------------------------------------------------------
// 整数解析（.NET TryParse / Parse 的空白与符号口径）
// ---------------------------------------------------------------------------

/// .NET `NumberStyles.Integer` 认可的空白：`\t \n \v \f \r` 与空格。
/// **不含** U+00A0 / U+3000（实机探测：`int.TryParse("\u00A012")` 失败）。
fn is_dotnet_parse_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}' | '\u{000A}' | '\u{000B}' | '\u{000C}' | '\u{000D}' | ' '
    )
}

/// 去掉 .NET 解析口径的前导空白与**尾部空白/尾部 NUL**。
///
/// 实机探测：`int.TryParse("12\0")` 与 `"12\0\0"` 成功，而 `"1\02"`、`"\0"` 失败
/// ⇒ 尾随 NUL 视作可忽略的尾部杂讯，串中 NUL 不忽略。保持 1:1。
fn trim_parse(s: &str) -> &str {
    let s = s.trim_start_matches(is_dotnet_parse_whitespace);
    s.trim_end_matches(|c: char| is_dotnet_parse_whitespace(c) || c == '\0')
}

/// `HUtil32.StrToInt(string, int)`：`int.TryParse(str, out result) ? result : def`。
///
/// 允许前后空白与前导 `+`/`-`；溢出、内部空白（`"+ 7"`）、`1,000`、全角数字、
/// U+00A0/U+3000 前后缀均失败。`null` 在 Rust 侧以 `""` 表示（TryParse 同样失败）。
pub fn str_to_int(s: &str, def: i32) -> i32 {
    trim_parse(s).parse::<i32>().unwrap_or(def)
}

/// `HUtil32.StrToInt16(string, short)`。
pub fn str_to_int16(s: &str, def: i16) -> i16 {
    trim_parse(s).parse::<i16>().unwrap_or(def)
}

/// `HUtil32.StrToUInt16(string, ushort)`：`ushort.TryParse`。
/// 允许前导 `+` 与前后空白；`-1`、`65536` 失败。
pub fn str_to_uint16(s: &str, def: u16) -> u16 {
    trim_parse(s).parse::<u16>().unwrap_or(def)
}

/// .NET `long.Parse` 口径（同一个 `trim_parse`）。供 [`ip_to_int`] 使用。
fn parse_i64(s: &str) -> Option<i64> {
    trim_parse(s).parse::<i64>().ok()
}

// ---------------------------------------------------------------------------
// 字符串切分（GetValidStr3 / GetValidStrCap / CaptureString）
// ---------------------------------------------------------------------------

fn is_div(c: char, divs: &[char]) -> bool {
    divs.contains(&c)
}

/// `HUtil32.GetValidStr3(source, ref dest, dividerAry)`（四个重载的共同语义）：
/// `source.Split(div, 2, StringSplitOptions.RemoveEmptyEntries)`，dest = 第一个非空段，
/// 返回值为“第二个 token 起点到串尾”的剩余部分。
///
/// 四个 oracle 重载在 Rust 侧收敛为 `&[char]`：
/// - `char` → `&[c]`
/// - `char[]` → 原样
/// - `string[]` → 每个元素只取首字符（oracle 的 `dividerAry[i][0]`）
/// - `string` → 每个字符都是分隔符（oracle 的 `div[i] = dividerAry[i]`）
///
/// .NET 8 实机语义（与 .NET Framework 直觉不同，已探测确认）：
/// 跳过分隔符取第一个 token 为 dest；返回值从第二个 token 起点开始，**连续分隔符整体跳过**。
/// 例：`'  abc  def'` → dest=`abc`，返回 `def`；`'a, ,b'`（逗号分隔）→ dest=`a`，返回 `" ,b"`
/// （空格不是分隔符，故留在剩余串里）。
///
/// `null` 源在 oracle 抛 `NullReferenceException`；Rust `&str` 无 null，调用方以 `""` 表达
/// 空输入（`""` / `"   "` → `("", "")`）。
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

impl fmt::Display for CaptureStringPanic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

/// `HUtil32.GetValidStrCap(str, ref dest, divider)`（三个重载的共同语义）：
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

/// `HUtil32.IsStringNumber(str)`：`!string.IsNullOrEmpty(str) || ValidationNumber().IsMatch(str)`。
/// 注意原实现的 `||` 缺陷：任何非空串都为 true；且正则 `^[+-]?\d*[.]?\d*$` 匹配空串，
/// 因此该函数对任意输入恒为 true。`null` 在 Rust 侧以 `""` 表示，仍为 true。保持 1:1。
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

/// `ArrestStringEx` 对 `ref arrestStr` 的写回语义（三态，对应 C# 的三条路径）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrestWrite {
    /// 写回该值（含“未找到定界符”时经 catch 写回空串）
    Value(String),
    /// 不改写（源为空；或已定位 span 但其中无闭合定界符）
    Unchanged,
}

/// `HUtil32.ArrestStringEx(source, searchAfter, arrestBefore, ref arrestStr)` 的精确移植。
/// 返回 (对 ref 参数的写回动作, 方法返回值)。单字符定界符（oracle 全部调用点均为单字符；
/// 多字符实参在 oracle 里先 `StartsWith` 再固定 `[1..]`，语义怪异且无调用方）。
///
/// C# 控制流（`HUtil32.cs:488-548`）：
/// - 源为空 → 直接 return ""，**不改写** arrestStr；
/// - `spanLen >= 2` 且能以 after 定位（首字符命中或 IndexOf 位置 n>0）→ findData=true；
/// - findData 为真：在 span 中找 before，命中 → 写回 before 之前内容、返回其后内容；
///   未命中 → **不改写**，返回 `after + span`；
/// - findData 为假（含 `spanLen < 2` 与 after 不存在）：进入 `for` 循环后于 `i == 0`
///   访问 `sourceSpan[-1]` 抛 `IndexOutOfRangeException`，被 catch 吞掉 →
///   **写回空串**、返回 ""。
pub fn arrest_string_ex_ref(source: &str, after: char, before: char) -> (ArrestWrite, String) {
    if source.is_empty() {
        return (ArrestWrite::Unchanged, String::new());
    }
    let cs = chars(source);
    // C# 用 OrdinalIgnoreCase 的 IndexOf；对定界符（[ ] " ( ) 等）等价于精确匹配。
    let span_start = if cs.len() >= 2 {
        if cs[0] == after {
            Some(1)
        } else {
            cs.iter()
                .position(|&c| c == after)
                .filter(|&n| n > 0)
                .map(|n| n + 1)
        }
    } else {
        None
    };
    match span_start {
        Some(s) => {
            let span = &cs[s..];
            match span.iter().position(|&c| c == before) {
                Some(p) => (
                    ArrestWrite::Value(span[..p].iter().collect()),
                    span[p + 1..].iter().collect(),
                ),
                None => (
                    ArrestWrite::Unchanged,
                    format!("{}{}", after, span.iter().collect::<String>()),
                ),
            }
        }
        // findData == false 分支：C# 抛异常被 catch → 写回空串、返回 ""
        None => (ArrestWrite::Value(String::new()), String::new()),
    }
}

// ---------------------------------------------------------------------------
// 前缀/后缀比较与字符谓词
// ---------------------------------------------------------------------------

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

/// `HUtil32.CompareBackLStr(src, targ, compn)`：从**尾部**取 compn 个字符做
/// 大小写不敏感比较（`compn <= 0` 或任一串过短 → false）。
pub fn compare_back_lstr(src: &str, targ: &str, compn: usize) -> bool {
    let s: Vec<char> = chars(src);
    let t: Vec<char> = chars(targ);
    if compn == 0 || s.len() < compn || t.len() < compn {
        return false;
    }
    let s_end = s.len();
    let t_end = t.len();
    for i in 0..compn {
        if to_upper_char(s[s_end - (i + 1)]) != to_upper_char(t[t_end - (i + 1)]) {
            return false;
        }
    }
    true
}

/// `HUtil32.IsVarNumber(str)`：`HUMAN`（5）/`GUILD`（5）/`GLOBAL`（6）前缀，大小写不敏感。
#[must_use]
pub fn is_var_number(s: &str) -> bool {
    compare_lstr_prefix(s, "HUMAN", 5)
        || compare_lstr_prefix(s, "GUILD", 5)
        || compare_lstr_prefix(s, "GLOBAL", 6)
}

fn is_english(ch: char) -> bool {
    ch.is_ascii_alphabetic()
}

/// `HUtil32.IsEngNumeric(char)`：ASCII 字母或数字。
#[must_use]
pub fn is_eng_numeric(ch: char) -> bool {
    is_english(ch) || ch.is_ascii_digit()
}

/// `HUtil32.IsEnglishStr(s)`：**只要存在一个** ASCII 字母就返回 true
/// （oracle 在首个 true 处 break；不是“全是字母”）。空串 → false。
#[must_use]
pub fn is_english_str(s: &str) -> bool {
    s.chars().any(is_english)
}

/// `HUtil32.ReplaceChar(src, srcchr, repchr)`。
///
/// **oracle 缺陷**：它把替换结果写进一个局部 `StringBuilder` 却返回原 `src`，
/// 因此对任何输入都**原样返回**。保持 1:1（`null` 源在 oracle 抛 NRE，Rust 无 null）。
#[must_use]
pub fn replace_char(src: &str, _srcchr: char, _repchr: char) -> String {
    src.to_string()
}

/// `HUtil32.TagCount(source, tag)`：数 `tag` 出现次数（按 UTF-16 字符计）。
#[must_use]
pub fn tag_count(source: &str, tag: char) -> usize {
    source.chars().filter(|c| *c == tag).count()
}

// ---------------------------------------------------------------------------
// 日期 / 容量 / IP
// ---------------------------------------------------------------------------

/// `HUtil32.GetDayCount(DateTime maxDate, DateTime minDate)` 的纯日期版。
///
/// 入参为 `(year, month, day)`。oracle 先用 `maxDate < minDate` 判 0；随后把
/// `minDate` 的**年份清零**再相减（真实缺陷），故月/日按 30 天折算、年按 360 天：
/// `(maxYear - minYear) * 360 + maxMonth * 30 + maxDay - minMonth * 30 - minDay`。
/// 例：(2026-10-20, 2025-01-01) → 649。
#[must_use]
pub fn get_day_count(max: (i32, u32, u32), min: (i32, u32, u32)) -> i32 {
    if max < min {
        return 0;
    }
    let (ym, mm, dm) = max;
    let (yn, mn, dn) = min;
    (ym - yn) * 12 * 30 + (mm as i32) * 30 + dm as i32 - ((mn as i32) * 30 + dn as i32)
}

/// .NET `Math.Round(x, 2)`（默认 `MidpointRounding.ToEven`）。
fn round_half_even_2(x: f64) -> f64 {
    (x * 100.0).round_ties_even() / 100.0
}

/// `HUtil32.FormatBytesValue(double length)`：按 1024 进制选择单位，
/// 数值走 `Math.Round(x, 2)`（**一半取偶**，与 [`round`] 的 AwayFromZero 不同）后
/// 以 .NET 默认 double 格式拼接。
///
/// 实机探测：`1029.12 → "1 KB"`、`1294467.072 → "1.23 MB"`、`1e30 → "867361737988.4 EB"`。
/// 边界说明：`±∞` 在 .NET 显示为 `∞`，Rust 显示 `inf`；`|v| >= 1e15` 时 .NET 用科学计数法。
/// 部署输入（文件大小）远小于这些边界。
#[must_use]
pub fn format_bytes_value(length: f64) -> String {
    const UNITS: [(f64, &str); 6] = [
        (1_152_921_504_606_846_976.0, "EB"), // 1024^6
        (1_125_899_906_842_624.0, "PB"),     // 1024^5
        (1_099_511_627_776.0, "TB"),         // 1024^4
        (1_073_741_824.0, "GB"),             // 1024^3
        (1_048_576.0, "MB"),                 // 1024^2
        (1024.0, "KB"),
    ];
    for (threshold, unit) in UNITS {
        if length >= threshold {
            return format!("{} {}", round_half_even_2(length / threshold), unit);
        }
    }
    format!("{length} Bytes")
}

/// `IpToInt` 的失败分类（oracle 的三种异常：`IndexOutOfRangeException` /
/// `FormatException` / `OverflowException`；后两者在此合并为 [`IpToIntError::Format`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpToIntError {
    /// 段数 < 4（oracle `items[1..3]` 越界）。
    MissingSegment,
    /// 段不是合法 `long`（oracle `long.Parse` 抛 Format/Overflow）。
    Format,
}

impl fmt::Display for IpToIntError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IpToIntError::MissingSegment => f.write_str("IndexOutOfRangeException (IpToInt)"),
            IpToIntError::Format => f.write_str("FormatException (IpToInt)"),
        }
    }
}

impl std::error::Error for IpToIntError {}

/// `HUtil32.IpToInt(ip)`：空/null → -1；否则按 `.` 切分，取前四段各自 `long.Parse`
/// 后 `a << 24 | b << 16 | c << 8 | d`。
///
/// **oracle 缺陷**：没有对每段做 0–255 校验或掩码（`"1.2.3.999"` → 16909287，
/// `"1.2.3.-4"` → -4），且第 5 段起被忽略。保持 1:1。
pub fn ip_to_int(ip: &str) -> Result<i64, IpToIntError> {
    if ip.is_empty() {
        return Ok(-1);
    }
    let items: Vec<&str> = ip.split('.').collect();
    if items.len() < 4 {
        return Err(IpToIntError::MissingSegment);
    }
    let a = parse_i64(items[0]).ok_or(IpToIntError::Format)?;
    let b = parse_i64(items[1]).ok_or(IpToIntError::Format)?;
    let c = parse_i64(items[2]).ok_or(IpToIntError::Format)?;
    let d = parse_i64(items[3]).ok_or(IpToIntError::Format)?;
    Ok((a << 24) | (b << 16) | (c << 8) | d)
}

// ---------------------------------------------------------------------------
// 时间戳
// ---------------------------------------------------------------------------

/// `HUtil32.GetTimestamp()`：`DateTimeOffset.Now.ToUnixTimeMilliseconds()`。
pub fn get_timestamp() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
    }
}

/// `HUtil32.GetTimestamp(TimeSpan)`：`DateTimeOffset.Now.Add(timeSpan).ToUnixTimeMilliseconds()`。
pub fn get_timestamp_offset(offset: Duration) -> i64 {
    get_timestamp().saturating_add(i64::try_from(offset.as_millis()).unwrap_or(i64::MAX))
}

// ---------------------------------------------------------------------------
// 字节 ↔ 字符（不涉及 GB2312 编解码，故可在零依赖的 shared 内实现）
// ---------------------------------------------------------------------------

/// `HUtil32.StrPas(byte[])`：每个字节按 **Latin-1/UTF-16 码元**转成一个字符
/// （`(char)buff[i]`，NUL 保留）。
#[must_use]
pub fn str_pas(buff: &[u8]) -> String {
    buff.iter().map(|&b| char::from(b)).collect()
}

/// `HUtil32.SpanToStr(ReadOnlySpan<byte>)`：`Encoding.ASCII.GetString`，
/// `< 0x80` 原样，其余一律 `'?'`。
#[must_use]
pub fn span_to_str(buff: &[u8]) -> String {
    buff.iter()
        .map(|&b| if b < 0x80 { char::from(b) } else { '?' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 以下来自 .NET 8 实机探测（探针源码/输出见 docs/HUtil32-parity-inventory-20261020.md）。

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
        // 探测补充：空格不是逗号分隔符 ⇒ 留在剩余串里（" ,b"）。
        assert_eq!(get_valid_str3("a, ,b", &[',']), ("a".into(), " ,b".into()));
        // 全角空格不是分隔符。
        assert_eq!(
            get_valid_str3("\u{3000}a b", &[' ']),
            ("\u{3000}a".into(), "b".into())
        );
        // oracle string[] 重载只取每个元素的首字符。
        assert_eq!(get_valid_str3("abXabY", &['a']), ("bX".into(), "bY".into()));
    }

    /// C7 矩阵：.NET 8 的 `string.Compare(..., OrdinalIgnoreCase)` 返回**码位差**。
    /// 牙：改成 `Ordering::Greater`/`== 0`、去掉长度分支 ⇒ 对应行必红。
    #[test]
    fn dotnet_compare_ordinal_ignore_case_c7_matrix() {
        let cmp = dotnet_compare_ordinal_ignore_case;
        assert_eq!(cmp("b", "a"), 1);
        assert_eq!(cmp("c", "a"), 2);
        assert_eq!(cmp("z", "a"), 25);
        assert_eq!(cmp("ab", "a"), 1, "前缀：码元长度差");
        assert_eq!(cmp("a", "ab"), -1);
        assert_eq!(cmp("B", "a"), 1, "大小写折叠");
        assert_eq!(cmp("丁", "一"), 1);
        assert_eq!(cmp("中", "一"), 45);
        assert_eq!(cmp("会乙", "会甲"), -9945);
        assert_eq!(cmp("b", "b"), 0);
        // `DelHumanObj` 的空槽口径：`Compare("", name) = -len ≠ 1`。
        assert_ne!(cmp("", "a"), 1);
        assert_eq!(cmp("", ""), 0);
    }

    #[test]
    fn str_to_int_dotnet_semantics() {
        assert_eq!(str_to_int(" 12 ", 0), 12);
        assert_eq!(str_to_int("+12", 0), 12);
        assert_eq!(str_to_int("12a", -1), -1);
        assert_eq!(str_to_int("99999999999", -1), -1);
        assert_eq!(str_to_int("", -1), -1);
        // 探测：只认 .NET Integer 空白集，不认 NBSP/全角空格。
        assert_eq!(str_to_int("\u{00A0}12\u{00A0}", -1), -1);
        assert_eq!(str_to_int("\u{3000}12", -1), -1);
        assert_eq!(str_to_int("+ 7", -1), -1);
        assert_eq!(str_to_int("1,000", -1), -1);
        assert_eq!(str_to_int("１２", -1), -1);
        // 探测：尾随 NUL 可忽略；串中 NUL 不可。
        assert_eq!(str_to_int("12\0", -1), 12);
        assert_eq!(str_to_int("12\0\0", -1), 12);
        assert_eq!(str_to_int("1\u{0}2", -1), -1);
        assert_eq!(str_to_int("\0", -1), -1);
    }

    #[test]
    fn str_to_int16_and_uint16_match_oracle() {
        assert_eq!(str_to_int16("-32768", 0), i16::MIN);
        assert_eq!(str_to_int16("32767", 0), i16::MAX);
        assert_eq!(str_to_int16("32768", 999), 999);
        assert_eq!(str_to_int16("-32769", 999), 999);
        assert_eq!(str_to_uint16("+12", 0), 12);
        assert_eq!(str_to_uint16(" 12 ", 0), 12);
        assert_eq!(str_to_uint16("-1", 999), 999);
        assert_eq!(str_to_uint16("65535", 0), u16::MAX);
        assert_eq!(str_to_uint16("65536", 999), 999);
        assert_eq!(str_to_uint16("", 999), 999);
    }

    /// 牙：把 `round` 的越界分支删掉（退回 `as i32` 饱和）⇒ 本用例必红。
    #[test]
    fn round_matches_dotnet_int_cast() {
        assert_eq!(round(2.5), 3);
        assert_eq!(round(-2.5), -3);
        assert_eq!(round(2.4), 2);
        assert_eq!(round(-2.4), -2);
        assert_eq!(round(2_147_483_647.0), i32::MAX);
        assert_eq!(round(2_147_483_648.0), i32::MIN);
        assert_eq!(round(3e9), i32::MIN);
        assert_eq!(round(-3e9), i32::MIN);
        assert_eq!(round(f64::NAN), i32::MIN);
        assert_eq!(round(f64::INFINITY), i32::MIN);
        assert_eq!(round(f64::NEG_INFINITY), i32::MIN);
    }

    #[test]
    fn bit_packers_match_oracle() {
        assert_eq!(make_long(-1, 0), 0x0000_FFFF);
        assert_eq!(make_long(-1, -1), -1);
        assert_eq!(make_long_u16(65535, 1), 0x0001_FFFF);
        assert_eq!(make_word(1, 2), 0x0201);
        assert_eq!(make_word_u16(0x1FF, 2), 0x03FF);
        assert_eq!(hi_word_i32(-1), 0xFFFF);
        assert_eq!(hi_word_u32(0xFFFF_FFFF), 0xFFFF);
        assert_eq!(hi_word_u16(0xFFFF), 0, "ushort 提升后右移 16 位恒为 0");
        assert_eq!(lo_word_i32(-1), 0xFFFF);
        assert_eq!(lo_word_u32(0x1_0001), 1);
        assert_eq!(hi_byte_i16(-1), 0xFF);
        assert_eq!(hi_byte_i32(-1), 0xFF);
        assert_eq!(hi_byte_u16(0xFFFF), 0xFF);
        assert_eq!(lo_byte_i16(-1), 0xFF);
        assert_eq!(lo_byte_i32(-1), 0xFF);
        assert_eq!(lo_byte_u16(0x1FF), 0xFF);
    }

    #[test]
    fn range_in_defined_matrix() {
        assert!(range_in_defined_i32(5, 1, 10));
        assert!(!range_in_defined_i32(0, 1, 10));
        assert!(!range_in_defined_i32(11, 1, 10));
        assert!(range_in_defined_i32(1, 1, 10));
        assert!(range_in_defined_i32(10, 1, 10));
        assert!(range_in_defined_i32(-5, -10, -1));
        // min > max：oracle 恒 false。
        assert!(!range_in_defined_i32(4, 5, 3));
        assert!(!range_in_defined_i32(5, 5, 3));
        assert!(range_in_defined_i64(5, 1, 10));
        assert!(!range_in_defined_i64(5, 5, 3));
        assert!(range_in_defined_u8(255, 200, 300));
        assert!(!range_in_defined_u8(255, 0, 254));
    }

    #[test]
    fn predicates_match_oracle() {
        assert!(is_var_number("HUMAN"));
        assert!(is_var_number("humanoid"));
        assert!(is_var_number("GUILDX"));
        assert!(is_var_number("globalx"));
        assert!(!is_var_number("HUM"));
        assert!(!is_var_number(""));
        assert!(is_english_str("中a"));
        assert!(!is_english_str("中1"));
        assert!(!is_english_str(""));
        assert!(is_eng_numeric('A'));
        assert!(is_eng_numeric('9'));
        assert!(!is_eng_numeric('_'));
        assert!(!is_eng_numeric('中'));
        assert!(compare_lstr("#IFxxx", "#IF"));
        assert!(!compare_lstr("#I", "#IF"));
        assert!(compare_lstr_prefix("[NECKLACE]x", "[NECKLACE]", 4));
        assert!(compare_back_lstr("abcd", "ZZcd", 2));
        assert!(!compare_back_lstr("abcd", "ab", 4));
        assert!(!compare_back_lstr("abcd", "abcd", 0));
        assert_eq!(bool_to_str(true), "TRUE");
        assert_eq!(bool_to_str(false), "FALSE");
        assert_eq!(bool_to_cstr(true), "是");
        assert_eq!(bool_to_cstr(false), "否");
        assert_eq!(bool_to_int_str(true), "1");
        assert_eq!(bool_to_int_str(false), "0");
        assert_eq!(min_i32(3, -2), -2);
        assert_eq!(max_i32(3, -2), 3);
    }

    /// 牙：把 `replace_char` “修好”成真的替换 ⇒ 本用例必红（锁住 oracle 缺陷）。
    #[test]
    fn replace_char_preserves_oracle_bug() {
        assert_eq!(replace_char("abc", 'a', 'x'), "abc");
        assert_eq!(replace_char("", 'a', 'x'), "");
        assert_eq!(tag_count("#0abc!", '!'), 1);
        assert_eq!(tag_count("", '!'), 0);
        assert_eq!(tag_count("a!a!a", 'a'), 3);
    }

    #[test]
    fn get_day_count_zeroes_min_year() {
        assert_eq!(get_day_count((2026, 10, 20), (2026, 10, 19)), 1);
        assert_eq!(get_day_count((2026, 10, 20), (2025, 1, 1)), 649);
        assert_eq!(get_day_count((2026, 1, 2), (2025, 12, 31)), 1);
        assert_eq!(get_day_count((2026, 10, 20), (2026, 10, 21)), 0);
        assert_eq!(get_day_count((2024, 2, 29), (2024, 2, 28)), 1);
    }

    #[test]
    fn format_bytes_value_matrix() {
        assert_eq!(format_bytes_value(0.0), "0 Bytes");
        assert_eq!(format_bytes_value(1023.0), "1023 Bytes");
        assert_eq!(format_bytes_value(1024.0), "1 KB");
        assert_eq!(format_bytes_value(1025.0), "1 KB");
        assert_eq!(format_bytes_value(1536.0), "1.5 KB");
        // 112.5 → 一半取偶 ⇒ 1.12（AwayFromZero 会得 1.13）。
        assert_eq!(format_bytes_value(1152.0), "1.12 KB");
        assert_eq!(format_bytes_value(1029.12), "1 KB");
        assert_eq!(format_bytes_value(1_048_576.0), "1 MB");
        assert_eq!(format_bytes_value(1_294_467.072), "1.23 MB");
        assert_eq!(format_bytes_value(1_073_741_824.0), "1 GB");
        assert_eq!(format_bytes_value(1_099_511_627_776.0), "1 TB");
        assert_eq!(format_bytes_value(1_125_899_906_842_624.0), "1 PB");
        assert_eq!(format_bytes_value(1_152_921_504_606_846_976.0), "1 EB");
        assert_eq!(format_bytes_value(-1024.0), "-1024 Bytes");
    }

    #[test]
    fn ip_to_int_matrix() {
        assert_eq!(ip_to_int("127.0.0.1"), Ok(2_130_706_433));
        assert_eq!(ip_to_int(" 1.2.3.4"), Ok(16_909_060));
        assert_eq!(ip_to_int("255.255.255.255"), Ok(4_294_967_295));
        assert_eq!(ip_to_int("0.0.0.0"), Ok(0));
        assert_eq!(ip_to_int("1.2.3.-4"), Ok(-4));
        // oracle 不做 0–255 校验：999 直接加在最低字节。
        assert_eq!(ip_to_int("1.2.3.999"), Ok(16_909_287));
        // 第 5 段起被忽略。
        assert_eq!(ip_to_int("1.2.3.4.5"), Ok(16_909_060));
        assert_eq!(ip_to_int(""), Ok(-1));
        assert_eq!(ip_to_int("1.2.3"), Err(IpToIntError::MissingSegment));
        assert_eq!(ip_to_int("1.2.3.x"), Err(IpToIntError::Format));
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
        // 期望值与 .NET 8 实机探测一致：探针里 `string ar = ""` 是调用前的初值，
        // 故“未改写”在探测输出里表现为 ar 保持 ""（三态下是 Unchanged）。
        let w = |src: &str, a: char, b: char| {
            let (write, rest) = arrest_string_ex_ref(src, a, b);
            let val = match write {
                ArrestWrite::Value(v) => v,
                ArrestWrite::Unchanged => String::new(), // 探针初值为 ""
            };
            (val, rest)
        };
        assert_eq!(
            w(r"#CALL [\游戏登陆\QF.txt] @main", '[', ']'),
            (r"\游戏登陆\QF.txt".into(), " @main".into())
        );
        assert_eq!(w("[main]", '[', ']'), ("main".into(), String::new()));
        assert_eq!(w("[main] TRUE", '[', ']'), ("main".into(), " TRUE".into()));
        assert_eq!(w("abc[de]fg", '[', ']'), ("de".into(), "fg".into()));
        // 无定界符 → C# catch 路径写回空串
        assert_eq!(
            arrest_string_ex_ref("nobrackets", '[', ']'),
            (ArrestWrite::Value(String::new()), String::new())
        );
        // 有开无闭 → 不改写，返回 after+span
        assert_eq!(
            arrest_string_ex_ref("[onlyopen", '[', ']'),
            (ArrestWrite::Unchanged, "[onlyopen".into())
        );
        assert_eq!(
            arrest_string_ex_ref("\"祈福项链", '"', '"'),
            (ArrestWrite::Unchanged, "\"祈福项链".into())
        );
        assert_eq!(w("\"abc\"", '"', '"'), ("abc".into(), String::new()));
        // 空源 → 直接返回，不改写
        assert_eq!(
            arrest_string_ex_ref("", '[', ']'),
            (ArrestWrite::Unchanged, String::new())
        );
        // len < 2 → findData 假 → catch 路径写回空串
        assert_eq!(
            arrest_string_ex_ref("x", '[', ']'),
            (ArrestWrite::Value(String::new()), String::new())
        );
        assert_eq!(
            arrest_string_ex_ref("M2 999", '[', ']'),
            (ArrestWrite::Value(String::new()), String::new())
        );
    }

    #[test]
    fn is_string_number_always_true() {
        // 原实现缺陷：|| 逻辑 + 空串匹配正则 → 恒 true
        assert!(is_string_number(""));
        assert!(is_string_number("abc"));
        assert!(is_string_number("-12.5"));
    }

    #[test]
    fn str_pas_and_span_to_str_match_oracle() {
        // 实机探测：StrPas 保留 NUL；SpanToStr(80 FF) = "??"。
        assert_eq!(str_pas(&[0x41, 0x00, 0x42]), "A\0B");
        assert_eq!(span_to_str(&[0x80, 0xFF]), "??");
        assert_eq!(span_to_str(b"#0A"), "#0A");
    }

    #[test]
    fn constants_match_oracle() {
        assert_eq!(BACKSLASH, '/');
        assert_eq!(SEPARATOR, [' ', ',', '\t']);
    }
}
