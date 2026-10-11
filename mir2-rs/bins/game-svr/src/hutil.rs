//! `HUtil32` 里本切片用到的三个纯工具（照 `src/OpenMir2/HUtil32.cs` 的行号）。

/// `HUtil32.TagCount(source, tag)`（`:650-662`）：数 `tag` 出现次数。
#[must_use]
pub fn tag_count(source: &str, tag: char) -> usize {
    source.chars().filter(|c| *c == tag).count()
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
    fn tag_count_counts() {
        assert_eq!(tag_count("#0abc!", '!'), 1);
        assert_eq!(tag_count("", '!'), 0);
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
