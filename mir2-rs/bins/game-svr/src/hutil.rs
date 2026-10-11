//! `HUtil32` 里本切片用到的三个纯工具（照 `src/OpenMir2/HUtil32.cs` 的行号）。

/// `HUtil32.TagCount(source, tag)`（`:650-662`）：数 `tag` 出现次数。
#[must_use]
pub fn tag_count(source: &str, tag: char) -> usize {
    source.chars().filter(|c| *c == tag).count()
}

/// `HUtil32.StrToInt(str, def)`（`:359-362`）＝ `int.TryParse(str, out r) ? r : def`。
/// .NET 的 `TryParse` 允许前后空白与符号；溢出/有杂字符 ⇒ 取 `def`。
#[must_use]
pub fn str_to_int(s: &str, def: i32) -> i32 {
    s.trim().parse::<i32>().unwrap_or(def)
}

/// `HUtil32.ArrestStringEx(source, searchAfter, arrestBefore, ref arrestStr)`（`:488-546`）：
/// 取 `searchAfter` 与 `arrestBefore` 之间的内容，返回**剩余串**，`arrested` 为取到的内容。
///
/// 照抄的三条怪癖（差分时别按直觉改）：
/// ① 找 `searchAfter`：串首就是它 ⇒ 从下标 1 起算；否则要它**首次出现在 n > 0 处**（n == 0 视为没找到，`:509-514`）；
/// ② 找到后取 `arrestBefore` 首次出现处 `i`：`arrested = span[..i]`、`rest = span[i + len(arrestBefore)..]`（`:519-523`）；
///    没出现 ⇒ `rest = searchAfter + span`，`arrested` 保持空（`:527`）；
/// ③ `spanLen >= 2` 这个门槛（`:500`）：长度 < 2 时**不做**查找（`findData` 保持 false）。
///
/// 已知缺陷（照抄，不修）：C# 在"没找到 searchAfter"的分支里用 `sourceSpan[i - 1]`，`i == 0` 时越界抛异常，
/// 被 catch 吞掉后**两个输出都置空**（`:532-545`）⇒ 这里等价实现为"返回两个空串"。
#[must_use]
pub fn arrest_string_ex(source: &str, search_after: &str, arrest_before: &str) -> (String, String) {
    if source.is_empty() {
        return (String::new(), String::new()); // `:490-493`
    }
    let mut body = source;
    let mut found = false;
    if source.chars().count() >= 2 {
        if let Some(s) = source.strip_prefix(search_after) {
            body = s;
            found = true;
        } else if let Some(n) = source.find(search_after) {
            if n > 0 {
                body = &source[n + search_after.len()..];
                found = true;
            }
        }
    }
    if !found {
        return (String::new(), String::new()); // 复刻上面的越界缺陷（结果为空）
    }
    if let Some(i) = body.find(arrest_before) {
        let arrested = body[..i].to_string();
        let rest = body[i + arrest_before.len()..].to_string();
        (rest, arrested)
    } else {
        (format!("{search_after}{body}"), String::new())
    }
}

/// 复刻 `SpanExtension.ToString(ReadOnlySpan<byte>, Encoding.ASCII)`（`src/OpenMir2/Extensions/SpanExtension.cs`）：
/// `Encoding.ASCII.GetString(bytes)` —— `< 0x80` 的字节原样成字符，其余一律成 `'?'`。
#[must_use]
pub fn ascii_from_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| if *b < 0x80 { char::from(*b) } else { '?' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_count_and_str_to_int() {
        assert_eq!(tag_count("#0abc!", '!'), 1);
        assert_eq!(tag_count("", '!'), 0);
        assert_eq!(str_to_int(" 12 ", 0), 12, "TryParse 允许空白");
        assert_eq!(str_to_int("2x", 7), 7, "有杂字符 ⇒ 取默认");
        assert_eq!(str_to_int("", -1), -1);
    }

    /// 本切片真正用到的那一形：`("#" + "<payload>" + "!")` ⇒ arrested 是 payload；
    /// rest = 闭合标记**之后**的内容（`:523` 的 `sourceSpan[(arrestStr.Length + 1)..]`）⇒ 这里为空。
    #[test]
    fn arrest_login_packet_shape() {
        let (rest, arrested) = arrest_string_ex("#0PAYLOAD!", "#", "!");
        assert_eq!(arrested, "0PAYLOAD");
        assert_eq!(rest, "", "rest 是闭合标记之后的内容（不是标记本身）");
    }

    #[test]
    fn arrest_edge_shapes() {
        // 串首就是 searchAfter：从下标 1 起算（`:502-506`）
        assert_eq!(
            arrest_string_ex("#a#b!", "#", "!"),
            (String::new(), "a#b".to_string())
        );
        // searchAfter 在 n>0 处：从那里之后起算（`:509-514`）
        assert_eq!(
            arrest_string_ex("xxXa!", "X", "!"),
            (String::new(), "a".to_string())
        );
        // 闭合标记之后还有内容 ⇒ rest 就是那段
        assert_eq!(
            arrest_string_ex("#abc!tail", "#", "!"),
            ("tail".to_string(), "abc".to_string())
        );
        // 没有 arrestBefore ⇒ rest = searchAfter + body（`:527`）
        assert_eq!(
            arrest_string_ex("#abc", "#", "!"),
            ("#abc".to_string(), String::new())
        );
        // 长度 < 2 ⇒ 不查找（`:500`）
        assert_eq!(
            arrest_string_ex("!", "#", "!"),
            (String::new(), String::new())
        );
    }

    #[test]
    fn ascii_rendering_maps_high_bytes_to_question_mark() {
        assert_eq!(ascii_from_bytes(&[b'#', b'0', 0x41]), "#0A");
        assert_eq!(
            ascii_from_bytes(&[0x80, 0xFF]),
            "??",
            "ASCII.GetString 的高位映射"
        );
    }
}
