//! GB2312/GBK（cp936）字节 ↔ 文本原语。
//!
//! DBSrv 的角色名/账号在线上是 **GBK 字节流**（客户端是 Delphi + GB2312），而 C# 侧的处理全是
//! **先 `HUtil32.GetString(...)` 把字节按 cp936 解成 .NET 字符串、再按字符切分/取长度/Trim**
//! （`string.Split` / `string.Length` / `string.Trim` / `HUtil32.ArrestStringEx`）。
//!
//! 本模块用**同一模型**：先 [`decode`] 成 `String`，在字符上做运算，再 [`encode`] 编回字节。
//! 合法 GBK 输入逐字节不变；畸形输入与 oracle 一样落成 `?`（`0x81` 前导吞掉**任意**后继字节，
//! 见 E-3）。旧实现直接用「裸字节扫描 + 只认 `0x40..=0xFE` 后继」，在 `81 20/2F/7F/00/FF/0A`
//! 这类畸形序列上字符数、`Split` 边界与 `arrest` 的取值都与 .NET 不同（审计
//! `audit-encoding-v2-20261021.md` §1.3 / E-3）。
//!
//! 编解码实现是 workspace 唯一的 `mir2_encoding::cp936`（`mir2_script::cp936` 转发）；
//! 本模块不复制任何表。

use mir2_script::cp936;

/// 前导字节：`0x81..=0xFE`。
#[must_use]
pub fn is_lead(b: u8) -> bool {
    (0x81..=0xFE).contains(&b)
}

/// 每个字符的字节跨度 `(start, end)`（等价 .NET cp936 解码的字符边界）。
///
/// **`0x81..=0xFE` 后只要有下一字节就成对消耗**（后继任意，含 `0x00/0x0A/0x20/0x2F/0x7F/0xFF`）；
/// 只有流末尾孤立的引导字节才是 1 个 `?`。这与 .NET `Encoding.GetEncoding(936)` 的实测一致
/// （审计 v2 §1.3/§2.4）；`0x80`、`0xFF` 各自单字节。
#[must_use]
pub fn char_spans(b: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        if is_lead(b[i]) && i + 1 < b.len() {
            out.push((i, i + 2));
            i += 2;
        } else {
            out.push((i, i + 1));
            i += 1;
        }
    }
    out
}

/// 字符数（等价 C# `string.Length`）。
#[must_use]
pub fn char_count(b: &[u8]) -> usize {
    char_spans(b).len()
}

/// 等价 C# `string.Split(dividers, 2, StringSplitOptions.RemoveEmptyEntries)`。
///
/// 返回 `(head, tail)`，对应 C# 的 `(ref dest, 返回值)` —— `head` 是第一个**非空**片段，
/// `tail` 是其后内容（count=2 时不再切）。语义**实测**自 .NET（PowerShell 直调，
/// `" 2 5".Split([char[]]@(' ',9), 2, RemoveEmptyEntries)` 等 20 组）：
/// - 空片段被丢弃且**不占用 count**（`" 2 5"` → `2` / `5`；`"a  b"` → `a` / `b`）；
/// - 命中第 1 个非空片段后的「剩余」**跳过连续分隔符**（`"a,,b"` → `a` / `b`，
///   而 `"a, ,b"` → `a` / ` ,b`：逗号后的空格不是分隔符，故属于剩余内容）；
/// - 剩余为空则没有 tail（`"a/"` → `a` / 空；`"/"`、`""` → 空 / 空）。
///
/// 切分在**解码后的字符**上做（C# 先 `GetString` 再 `Split`）：`81 2F` 是一个字符（`?`），
/// 其中的 `/` 不是分隔符；片段按 `HUtil32.GetBytes` 编回字节（畸形序列落 `?`）。
#[must_use]
pub fn split2_nonempty(b: &[u8], dividers: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let chars: Vec<char> = decode(b).chars().collect();
    let is_divider = |c: char| match u8::try_from(c) {
        Ok(v) => dividers.contains(&v),
        Err(_) => false,
    };
    let div: Vec<usize> = chars
        .iter()
        .enumerate()
        .filter(|(_, c)| is_divider(**c))
        .map(|(i, _)| i)
        .collect();
    let mut start = 0usize;
    let mut di = 0usize;
    while di < div.len() {
        let d = div[di];
        if start == d {
            start = d + 1; // 空片段：丢弃且不占 count
            di += 1;
            continue;
        }
        let mut j = d + 1;
        while di + 1 < div.len() && div[di + 1] == j {
            j += 1; // 连续分隔符整体跳过（.NET 实测语义）
            di += 1;
        }
        return if j >= chars.len() {
            (encode_chars(&chars[start..d]), Vec::new())
        } else {
            (encode_chars(&chars[start..d]), encode_chars(&chars[j..]))
        };
    }
    if start >= chars.len() {
        (Vec::new(), Vec::new())
    } else {
        (encode_chars(&chars[start..]), Vec::new())
    }
}

/// 等价 C# `HUtil32.ArrestStringEx(source, after, before, ref out)` 的**取值**部分：
/// 返回 `(out, rest)` —— `out` 是 `after` 与 `before` 之间的内容（不含定界符），
/// `rest` 是 `before` 之后的剩余文本（对应 C# 的返回值）。
///
/// 在**解码后的字符**上找定界符（`HUtil32.cs:488` 的 `IndexOf`），所以「前导 + 定界符字节」
/// 会被当成一个字符吞掉，不再被裸字节扫描误当定界符（E-3）。
#[must_use]
pub fn arrest(source: &[u8], after: u8, before: u8) -> (Vec<u8>, Vec<u8>) {
    let chars: Vec<char> = decode(source).chars().collect();
    // C# `spanLen < 2` 时 `findData` 保持 false，回退循环在 `sourceSpan[-1]` 抛
    // `IndexOutOfRangeException` ⇒ catch 把 `arrestStr`/`result` 都置空。
    if chars.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let after_c = char::from(after);
    let before_c = char::from(before);
    let body_start = if chars[0] == after_c {
        1
    } else {
        // `IndexOf(after, OrdinalIgnoreCase)`：调用点的定界符都是非字母 ASCII，
        // OrdinalIgnoreCase 与精确相等在该域内同值。
        match chars.iter().position(|c| *c == after_c) {
            Some(p) if p > 0 => p + 1,
            _ => return (Vec::new(), Vec::new()),
        }
    };
    let body = &chars[body_start..];
    if let Some(n) = body.iter().position(|c| *c == before_c) {
        (encode_chars(&body[..n]), encode_chars(&body[n + 1..]))
    } else {
        // C# `result = searchAfter + sourceSpan`：把定界符接回剩余串
        let mut rest = vec![after];
        rest.extend_from_slice(&encode_chars(body));
        (Vec::new(), rest)
    }
}

/// 等价 C# `HUtil32.StrToInt(s, def)`（`int.TryParse`）：允许首尾空白与正负号。
#[must_use]
pub fn str_to_int(b: &[u8], def: i32) -> i32 {
    decode(b).trim().parse::<i32>().unwrap_or(def)
}

/// 等价 C# `string.Trim()`：去掉首尾 Unicode 空白（含全角空格 `A1A1` → `U+3000`）。
#[must_use]
pub fn trim_name(b: &[u8]) -> Vec<u8> {
    let text = decode(b);
    encode(text.trim())
}

/// 账号域的字节级大小写折叠（ASCII）。
///
/// C# `_quickList`/`_NameQuickMap`/`PlayQuickList` 都是 `StringComparer.OrdinalIgnoreCase`；
/// 账号是 ASCII（A-14 域），ASCII 折叠与 ordinal-ignore-case 同值。**角色名**键改用
/// [`fold_text`]（String + D-2/B-103 折叠），不再走本函数。
#[must_use]
pub fn fold_key(b: &[u8]) -> Vec<u8> {
    b.iter().map(u8::to_ascii_lowercase).collect()
}

/// 角色名内存键的折叠（`StringComparer.OrdinalIgnoreCase` 的已登记近似，D-2/B-103）。
///
/// 用 Unicode 简单大写折叠；生产语料是 GBK 中文 + ASCII，两种规则在该域内判定相同。
/// E-7 要求内存键是**解码后的 String**，不是 MySQL `CONVERT(… USING gbk)` 生成的字节。
#[must_use]
pub fn fold_text(s: &str) -> String {
    s.chars().flat_map(char::to_uppercase).collect()
}

/// cp936 解码（`HUtil32.GetString`）：前导吞任意后继，非法/末位给 `?`，`0x80→U+20AC`，
/// `0xFF→U+F8F5`。实现见 `mir2_encoding::cp936`（B-105）。
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    cp936::decode(bytes)
}

/// 编码（对应 `HUtil32.GetBytes(str)`）：逐 UTF-16 码元查 .NET cp936 表，
/// 不可映射 → 单个 `0x3F`，绝不输出 `&#N;`。
///
/// **不查库**：oracle `DataService.cs:192` 的 `EDCode.EncodeString(chrName)` 走的是
/// `HUtil32.GetBytes`，DB 停机时该帧的角色名依然必须是 GB2312 字节。
#[must_use]
pub fn encode(s: &str) -> Vec<u8> {
    cp936::encode_gb2312_dotnet(s)
}

/// 把一段 `char` 按 cp936 编回字节。
fn encode_chars(chars: &[char]) -> Vec<u8> {
    let text: String = chars.iter().collect();
    encode(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 汉字按 2 字节切，ASCII 按 1 字节切：`a` + 血 + `b` → 3 个字符。
    #[test]
    fn counts_gbk_and_ascii() {
        let mut v = vec![b'a'];
        v.extend_from_slice(&[0xD1, 0xAA]); // 血
        v.push(b'b');
        assert_eq!(char_spans(&v), vec![(0, 1), (1, 3), (3, 4)]);
        assert_eq!(char_count(&v), 3);
        assert_eq!(char_count(b""), 0);
        assert_eq!(char_count(b"abc"), 3);
    }

    /// E-3 判据 1：`0x81` 前导**吞任意后继**（与 .NET 一致），不是只吞合法后继。
    /// 牙：把 `char_spans` 改回「后继必须落在 `0x40..=0xFE` 且非 `0x7F`」⇒ 前 9 条必红。
    #[test]
    fn char_count_matches_dotnet_on_lead_swallowing_any_successor() {
        let cases: &[(&[u8], usize)] = &[
            (b"\x81\x20\x41", 2),
            (b"\x81\x2F", 1),
            (b"\x81\x7F", 1),
            (b"\x81\x00\x41", 2),
            (b"\x81\xFF", 1),
            (b"\x81\x40", 1),
            (b"\x81\x0A\x41", 2),
            (b"\x41\x81", 2),
            (b"\x81\x20\x20\x41", 3),
            (b"\xFF", 1),
            (b"\x80", 1),
            (b"\x80\x63", 2),
            (b"\xA8\xBF", 1),
            (b"\x81", 1),
            (b"", 0),
        ];
        for (input, want) in cases {
            assert_eq!(char_count(input), *want, "输入 {input:02X?}");
        }
    }

    /// 分隔符不会被汉字的后继字节抢走（后继 0x5C 也是合法 GBK 后继字节）。
    #[test]
    fn divider_never_lands_inside_a_char() {
        let v = vec![0xB0u8, 0x5C, b'/', b'a']; // 0xB0 0x5C 是一个汉字，后续字节恰为反斜杠
        let (head, tail) = split2_nonempty(&v, b"/");
        assert_eq!(head, vec![0xB0, 0x5C]);
        assert_eq!(tail, b"a".to_vec());
        let (h, t) = split2_nonempty(&v, b"\\");
        assert_eq!(h, v);
        assert!(t.is_empty());
    }

    /// E-3 判据 2：切分在**解码后的字符**上做，片段按 cp936 编回字节。
    /// 牙：把片段输出改回原始字节切片 ⇒ `a \x81/ b` 会得到 `[81 2F 20 62]` 而不是 `[3F 20 62]`。
    #[test]
    fn split2_decodes_before_splitting() {
        // `81 2F` 是一个字符（`?`），因此 `/` 不是分隔符：`a?b` 整体不切。
        assert_eq!(
            split2_nonempty(b"a\x81/b", b"/"),
            (b"a?b".to_vec(), Vec::new())
        );
        // 空格在 `81 2F` 之后：`a` / `? b`（C# `[a | ? b]`）。
        assert_eq!(
            split2_nonempty(b"a \x81/ b", b" "),
            (b"a".to_vec(), b"? b".to_vec())
        );
        // 前导吞掉分隔符：`a` + `81 20` + `b` 在空格上不切（空格是字符的一部分）。
        assert_eq!(
            split2_nonempty(b"a\x81 b", b" "),
            (b"a?b".to_vec(), Vec::new())
        );
        // 合法 GBK 逐字节不变（现网账号/名字域）。
        let mut v = vec![b'a'];
        v.extend_from_slice(&[0xD1, 0xAA]); // 血
        v.push(b'/');
        v.push(b'b');
        assert_eq!(
            split2_nonempty(&v, b"/"),
            (vec![b'a', 0xD1, 0xAA], b"b".to_vec())
        );
    }

    /// RemoveEmptyEntries 语义（**逐条实测自 .NET**：PowerShell 直调
    /// `s.Split([char[]]@(','), 2, [System.StringSplitOptions]::RemoveEmptyEntries)`）。
    #[test]
    fn split2_matches_dotnet_remove_empty_entries() {
        let cases: &[(&str, &str, &str)] = &[
            // (输入, head, tail)
            ("mir2test/1234", "mir2test", "1234"),
            ("/1234", "1234", ""),
            ("mir2test/", "mir2test", ""),
            ("mir2test", "mir2test", ""),
            ("/", "", ""),
            ("", "", ""),
            ("a/b/c", "a", "b/c"),
            ("a//b", "a", "b"),
            (" 2 5", "2", "5"),
            ("a  b", "a", "b"),
            ("a b c", "a", "b c"),
            (" a", "a", ""),
            ("a ", "a", ""),
            (" ", "", ""),
        ];
        for (input, head, tail) in cases {
            let (h, t) = split2_nonempty(input.as_bytes(), b" /");
            assert_eq!(
                (
                    String::from_utf8_lossy(&h).into_owned(),
                    String::from_utf8_lossy(&t).into_owned()
                ),
                ((*head).to_string(), (*tail).to_string()),
                "输入 {input:?}"
            );
        }
    }

    /// ArrestStringEx：取 `%` 与 `$` 之间的内容 + 之后的剩余。
    #[test]
    fn arrest_extracts_between_delimiters() {
        let (data, rest) = arrest(b"%K7/1.2.3.4/1.2.3.4$", b'%', b'$');
        assert_eq!(data, b"K7/1.2.3.4/1.2.3.4".to_vec());
        assert!(rest.is_empty());
        let (_, rest) = arrest(b"%x$tail", b'%', b'$');
        assert_eq!(rest, b"tail".to_vec());
        assert_eq!(arrest(b"nope", b'%', b'$'), (Vec::new(), Vec::new()));
        // 缺 `$`：C# 返回 `after + 剩余`（内容取不到）
        let (data, rest) = arrest(b"%partial", b'%', b'$');
        assert!(data.is_empty());
        assert_eq!(rest, b"%partial".to_vec());
        // C# `spanLen < 2` 走向越界 catch ⇒ 空串（旧实现返回 `("%", "")`，E-3 订正）。
        assert_eq!(arrest(b"%", b'%', b'$'), (Vec::new(), Vec::new()));
        assert_eq!(arrest(b"", b'%', b'$'), (Vec::new(), Vec::new()));
    }

    /// E-3 判据 3：`arrest` 按 cp936 字符边界找定界符，前导吞掉的定界符字节不再命中。
    /// 牙：把实现改回裸字节 `position(|b| *b == before)` ⇒ 两条都取到 `a?` 之后的错位剩余。
    #[test]
    fn arrest_is_cp936_char_aware() {
        // `#a\x81!t!` 解成 `#a?t!` ⇒ out=`a?t`，rest=""
        assert_eq!(
            arrest(b"#a\x81!t!", b'#', b'!'),
            (b"a?t".to_vec(), Vec::new())
        );
        // `(a\x81)b)` 解成 `(a?b)` ⇒ out=`a?b`，rest=""
        assert_eq!(
            arrest(b"(a\x81)b)", b'(', b')'),
            (b"a?b".to_vec(), Vec::new())
        );
        // 合法 GBK 定界符两侧逐字节不变。
        assert_eq!(
            arrest(b"#\xD1\xAA!", b'#', b'!'),
            (vec![0xD1, 0xAA], Vec::new())
        );
    }

    /// StrToInt = int.TryParse（失败给默认值）。
    #[test]
    fn str_to_int_tryparse() {
        assert_eq!(str_to_int(b"123", -2), 123);
        assert_eq!(str_to_int(b" 123 ", -2), 123);
        assert_eq!(str_to_int(b"-7", -2), -7);
        assert_eq!(str_to_int(b"+7", -2), 7);
        assert_eq!(str_to_int(b"", -2), -2);
        assert_eq!(str_to_int(b"abc", -2), -2);
        assert_eq!(str_to_int(b"999999999999", -2), -2);
    }

    /// Trim = C# `string.Trim()`：ASCII 空白与全角空格（`A1A1` → `U+3000`）。
    /// 牙：改回裸字节 `is_ascii_whitespace` 扫描 ⇒ `81 20` 这类字符的后继字节会被误裁。
    #[test]
    fn trim_matches_dotnet_string_trim() {
        assert_eq!(trim_name(b"  blood  "), b"blood".to_vec());
        assert_eq!(trim_name(b"\xa1\xa1ab\xa1\xa1"), b"ab".to_vec());
        assert_eq!(trim_name(b"\t a b \n"), b"a b".to_vec());
        // `81 20` 是一个字符（`?`）：尾部的 0x20 属于该字符，不是空白。
        assert_eq!(trim_name(b"\x81\x20"), b"?".to_vec());
        assert_eq!(trim_name(b" \x81\x20 "), b"?".to_vec());
    }

    /// 合法名字的 GB2312 字节：不依赖 MySQL 也能拿到（DBSrv 1100 降级帧用）。
    #[test]
    fn encodes_gb2312_without_db() {
        assert_eq!(encode("测试"), vec![0xB2, 0xE2, 0xCA, 0xD4]);
        assert_eq!(encode("loadhero"), b"loadhero".to_vec());
    }

    /// 判据 E-1（WHATWG-only）：99 个 `.NET cp936` 不可映射、但 WHATWG GBK 有真实字节的
    /// 码点必须回退单个 `0x3F`。牙：改回 `encoding_rs::GBK.encode` 时 `U+01F9` 编成
    /// `A8 BF`、`U+2E81` 编成 `FE 50`，本断言必红。
    #[test]
    fn whatwg_only_codepoints_fall_back_to_question_mark() {
        assert_eq!(encode("\u{01F9}"), vec![0x3F]);
        assert_eq!(encode("\u{2E81}"), vec![0x3F]);
        // 前后的合法字符不受影响（逐码元编码，不是整串替换）。
        assert_eq!(encode("a\u{01F9}\u{8840}"), vec![b'a', 0x3F, 0xD1, 0xAA]);
    }

    /// 判据 E-1（非 BMP）：按 UTF-16 码元拆成两个代理码元，各回退一个 `0x3F`。
    /// 牙：`encoding_rs::GBK.encode` 对 `U+1F600` 输出 `&#128512;`（9 字节），本断言必红。
    #[test]
    fn non_bmp_char_falls_back_to_two_question_marks() {
        assert_eq!(encode("\u{1F600}"), vec![0x3F, 0x3F]);
        assert_eq!(encode("a\u{1F600}b"), vec![b'a', 0x3F, 0x3F, b'b']);
    }

    /// 判据 E-1（合法 GBK 逐字节不变）：现网名字域的常用汉字与 ASCII 必须与
    /// C# `HUtil32.GetBytes` 同字节；best-fit 项（`U+00A6 -> 0x7C`）同 .NET 表。
    #[test]
    fn legal_gbk_bytes_unchanged() {
        assert_eq!(encode("测试"), vec![0xB2, 0xE2, 0xCA, 0xD4]);
        assert_eq!(
            encode("热血传奇"),
            vec![0xC8, 0xC8, 0xD1, 0xAA, 0xB4, 0xAB, 0xC6, 0xE6]
        );
        assert_eq!(encode("loadhero/1234"), b"loadhero/1234".to_vec());
        assert_eq!(encode("\u{00A6}"), vec![0x7C]);
    }

    /// 判据 E-1（非法输入）：孤立代理在 .NET 下回退 `0x3F`；Rust `&str` 不能承载孤立
    /// 代理，等价输入是 UTF-16 lossy 后的 `U+FFFD`。同时断言绝不输出 `&#N;`。
    #[test]
    fn invalid_or_unmappable_input_falls_back_to_single_question_mark() {
        let lone_surrogate = String::from_utf16_lossy(&[0xD800]);
        assert_eq!(encode(&lone_surrogate), vec![0x3F]);
        assert_eq!(encode("\u{FFFD}"), vec![0x3F]);
        let emoji = encode("\u{1F600}");
        assert_eq!(emoji, vec![0x3F, 0x3F], "每个代理码元各回退一个 `?`");
        assert!(
            !emoji.windows(2).any(|w| w[0] == b'&' && w[1] == b'#'),
            "绝不能输出数字字符引用"
        );
    }

    /// 判据 E-3（解码回环）：合法 GBK（含 0x80/0xFF 的 oracle 映射）解码后再编码
    /// 逐字节不变 —— 这是 DB 列值改绑 String 后 5100/6000 帧字节不变的前提。
    /// 牙：把 `decode` 换成 WHATWG `encoding_rs::GBK`（`0xFF→U+FFFD`）⇒ `0xFF` 条必红。
    #[test]
    fn decode_encode_roundtrip_is_byte_stable() {
        for input in [
            b"loadhero".as_slice(),
            &[0xB2, 0xE2, 0xCA, 0xD4], // 测试
            &[0xC8, 0xC8, 0xD1, 0xAA], // 热血
            &[0xA8, 0xBF],             // cp936 U+E7C8（B-105）
            &[0x80],                   // U+20AC
            &[0xFF],                   // U+F8F5
            &[0xA1, 0xA1],             // 全角空格 U+3000
        ] {
            assert_eq!(encode(&decode(input)), input.to_vec(), "输入 {input:02X?}");
        }
        // 畸形序列按 oracle 落 `?`（不是原字节）。
        assert_eq!(decode(b"\x81\x20"), "?");
        assert_eq!(encode(&decode(b"\x81\x20")), b"?".to_vec());
    }

    /// 判据 E-7（折叠一致性）：名字键用 String + D-2/B-103 折叠；ASCII 大小写不敏感，
    /// 汉字原样；折叠只用于内存键，不改变线上字节。
    #[test]
    fn fold_text_is_ordinal_ignore_case_approx() {
        assert_eq!(fold_text("Hero"), fold_text("hERO"));
        assert_eq!(fold_text("测试"), fold_text("测试"));
        assert_ne!(fold_text("测试"), fold_text("测试2"));
        let name = decode(b"\xB2\xE2\xCA\xD4");
        assert_eq!(name, "测试");
        assert_eq!(fold_text(&name), "测试");
    }

    /// 判据 E-7（名字解码后绑定）：5100 名字帧的字节先解码成 String 再入索引键；
    /// 合法字符下 `encode(decode(bytes)) == bytes`，6000/5100 帧字节不变。
    #[test]
    fn name_binds_decoded_string_and_frame_bytes_unchanged() {
        let wire = [0xB2u8, 0xE2, 0xCA, 0xD4]; // 测试
        let text = decode(&wire);
        assert_eq!(text, "测试");
        assert_eq!(encode(&text), wire.to_vec());
        assert_eq!(
            mir2_protocol::edcode::encode(&encode(&text)),
            b"BNb[X>".to_vec(),
            "6000/5100 帧 `ChrName = EDCode(GBK(name))`"
        );
        // ASCII 名字（现网绝大多数）逐字节不变。
        assert_eq!(encode(&decode(b"loadhero")), b"loadhero".to_vec());
    }
}
