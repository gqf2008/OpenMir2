//! GB2312/GBK 字节边界原语。
//!
//! DBSrv 的角色名在线上是 **GBK 字节流**（客户端是 Delphi + GB2312），而 C# 侧的处理全是
//! **先 `HUtil32.GetString(...)` 把字节按 GB2312 解成 .NET 字符串、再按字符切分/取长度**
//! （`string.Split` / `string.Length`）。本模块把这些操作在**字节级**等价复现：
//! 只要知道「哪几个字节构成一个字符」，`Split`/`Length` 就能在字节上做，与 C# 逐字符操作等价。
//!
//! 边界规则（GBK，cp936）：前导字节 `0x81..=0xFE`，后继字节 `0x40..=0xFE` 且非 `0x7F`；
//! 其余单字节自成字符。ASCII 分隔符（斜杠、反斜杠、空格等）永远落在单字节字符上，因此用本模块
//! 切分不会把某个汉字的第二个字节误当分隔符（C# 先解码再切分，天然不会）。
//!
//! 适用范围：输入是**合法 GBK**（现网角色名/账号/地图名即此）。非法序列按单字节处理
//! （.NET 会各自产出替代字符，字符数一致）。

/// 前导字节：`0x81..=0xFE`。
#[must_use]
pub fn is_lead(b: u8) -> bool {
    (0x81..=0xFE).contains(&b)
}

/// 后继字节：`0x40..=0xFE` 且不是 `0x7F`。
#[must_use]
pub fn is_trail(b: u8) -> bool {
    (0x40..=0xFE).contains(&b) && b != 0x7F
}

/// 每个字符的字节跨度 `(start, end)`（等价 .NET 的字符边界）。
#[must_use]
pub fn char_spans(b: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        if is_lead(b[i]) && i + 1 < b.len() && is_trail(b[i + 1]) {
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
#[must_use]
pub fn split2_nonempty(b: &[u8], dividers: &[u8]) -> (Vec<u8>, Vec<u8>) {
    // 单字节分隔符在字符串里的位置（都落在字符边界上）
    let div: Vec<usize> = char_spans(b)
        .into_iter()
        .filter(|(s, e)| e - s == 1 && dividers.contains(&b[*s]))
        .map(|(s, _)| s)
        .collect();
    let mut start = 0usize;
    let mut di = 0usize;
    while di < div.len() {
        let d = div[di];
        let cand = &b[start..d];
        if cand.is_empty() {
            start = d + 1; // 空片段：丢弃且不占 count
            di += 1;
            continue;
        }
        let mut j = d + 1;
        while di + 1 < div.len() && div[di + 1] == j {
            j += 1; // 连续分隔符整体跳过（.NET 实测语义）
            di += 1;
        }
        return if j >= b.len() {
            (cand.to_vec(), Vec::new())
        } else {
            (cand.to_vec(), b[j..].to_vec())
        };
    }
    let tail = &b[start..];
    if tail.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (tail.to_vec(), Vec::new())
    }
}

/// 等价 C# `HUtil32.ArrestStringEx(source, after, before, ref out)` 的**取值**部分：
/// 返回 `(out, rest)` —— `out` 是 `after` 与 `before` 之间的内容（不含定界符），
/// `rest` 是 `before` 之后的剩余文本（对应 C# 的返回值）。
#[must_use]
pub fn arrest(source: &[u8], after: u8, before: u8) -> (Vec<u8>, Vec<u8>) {
    if source.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let body = if source[0] == after {
        Some(&source[1..])
    } else {
        source
            .iter()
            .position(|b| *b == after)
            .filter(|p| *p > 0)
            .map(|p| &source[p + 1..])
    };
    let Some(body) = body else {
        // findData=false：C# 循环体（`sourceSpan[i-1]` 写法）永不命中 ⇒ 返回空串且不动 ref 参数
        return (Vec::new(), Vec::new());
    };
    if let Some(n) = body.iter().position(|b| *b == before) {
        (body[..n].to_vec(), body[n + 1..].to_vec())
    } else {
        // C# `result = searchAfter + sourceSpan`：把定界符接回剩余串
        let mut rest = vec![after];
        rest.extend_from_slice(body);
        (Vec::new(), rest)
    }
}

/// 等价 C# `HUtil32.StrToInt(s, def)`（`int.TryParse`）：允许首尾空白与正负号。
#[must_use]
pub fn str_to_int(b: &[u8], def: i32) -> i32 {
    let Ok(s) = std::str::from_utf8(b) else {
        return def;
    };
    s.trim().parse::<i32>().unwrap_or(def)
}

/// 等价 C# `string.Trim()` 对现网名字的行为：去掉首尾 ASCII 空白与全角空格（GBK `A1A1`）。
#[must_use]
pub fn trim_name(b: &[u8]) -> Vec<u8> {
    let mut s = b;
    loop {
        if let Some((f, rest)) = s.split_first() {
            if f.is_ascii_whitespace() {
                s = rest;
                continue;
            }
        }
        if s.starts_with(&[0xA1, 0xA1]) {
            s = &s[2..];
            continue;
        }
        break;
    }
    loop {
        if let Some((l, rest)) = s.split_last() {
            if l.is_ascii_whitespace() {
                s = rest;
                continue;
            }
        }
        if s.ends_with(&[0xA1, 0xA1]) {
            s = &s[..s.len() - 2];
            continue;
        }
        break;
    }
    s.to_vec()
}

/// 大小写不敏感的 key（ASCII 折叠）。
///
/// C# 侧 `_quickList`/`_NameQuickMap`/`PlayQuickList` 都是 `StringComparer.OrdinalIgnoreCase`；
/// 现网名字是 GBK 中文 + ASCII，两套规则在该域内判定相同（同 whitelist D-2/B-103 口径）。
#[must_use]
pub fn fold_key(b: &[u8]) -> Vec<u8> {
    b.iter().map(u8::to_ascii_lowercase).collect()
}

/// 编码（对应 `HUtil32.GetBytes(str)`）：**逐字节等于 .NET cp936**。
///
/// 走 `mir2_script::cp936::encode_gb2312_dotnet`（全 workspace 唯一编码实现）：
/// 不可映射码点（含 99 个 WHATWG-only、代理码元）回退单个 `0x3F`，绝不输出 `&#N;`。
/// 本 crate 的解码仍用 `encoding_rs::GBK` 近似（未切换的编码点见 whitelist **A-14** 分项台账；本 crate 的编码点已切换）。
///
/// **不查库**：oracle `DataService.cs:192` 的 `EDCode.EncodeString(chrName)` 走的是
/// `HUtil32.GetBytes`，DB 停机时该帧的角色名依然必须是 GB2312 字节。
#[must_use]
pub fn encode(s: &str) -> Vec<u8> {
    mir2_script::cp936::encode_gb2312_dotnet(s)
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
    }

    /// StrToInt = int.TryParse（失败给默认值）。
    #[test]
    fn str_to_int_tryparse() {
        assert_eq!(str_to_int(b"123", -2), 123);
        assert_eq!(str_to_int(b" 123 ", -2), 123);
        assert_eq!(str_to_int(b"-7", -2), -7);
        assert_eq!(str_to_int(b"", -2), -2);
        assert_eq!(str_to_int(b"abc", -2), -2);
        assert_eq!(str_to_int(b"999999999999", -2), -2);
    }

    /// `HUtil32.GetBytes` 的 GB2312 字节：不依赖 MySQL 也能拿到（DBSrv 1100 降级帧用）。
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

    /// 判据 E-1（db-srv 6000 面）：合法名字的 GBK 字节经 `EDCode` 后必须与 oracle
    /// `EDCode.EncodeString(name)` 逐字节相同（`DataService.cs:192` 的 1100 取档帧字段）。
    /// 牙：若编码回退 UTF-8（旧实现）或回退 `encoding_rs`，EDCode 串都会变。
    #[test]
    fn name_and_6000_frame_bytes_unchanged() {
        let gbk = encode("测试");
        assert_eq!(
            gbk,
            vec![0xB2, 0xE2, 0xCA, 0xD4],
            "oracle `HUtil32.GetBytes(\"测试\")`"
        );
        assert_eq!(
            mir2_protocol::edcode::encode(&gbk),
            b"BNb[X>".to_vec(),
            "1100 取档帧 ChrName = EDCode(GBK(name))"
        );
        // 反向牙：UTF-8 字节的 EDCode 与 oracle 不同。
        assert_ne!(
            mir2_protocol::edcode::encode("测试".as_bytes()),
            mir2_protocol::edcode::encode(&gbk)
        );
    }
}
