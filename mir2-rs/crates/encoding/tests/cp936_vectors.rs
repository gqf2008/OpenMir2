//! 新编码 crate 的自证向量（判据 1–6）。
//!
//! 红检（改坏生产实现 ⇒ 本文件对应断言必红）：
//! - `encode_unit` 回退从 `0x3F` 改成别的/写 `&#N;` ⇒ `encode_*` 必红；
//! - 去掉 `GBK_OVERRIDES` 查询或改 `A8BF` 映射 ⇒ `decode_*` / `override_*` 必红；
//! - 把 `decode_string_list` 的 `FF FE` 分支改成先判 UTF-32LE ⇒ `string_list_bom_model` 必红；
//! - 把 `decode_config_text` 改回 StringList 口径 ⇒ `config_bom_model` 必红；
//! - 破坏 cp936 状态机（前导不吞任意后继）⇒ `decode_state_machine` 必红。

use mir2_encoding::{
    decode, decode_config_text, decode_stream_reader, decode_string_list, encode_gb2312_dotnet,
    GBK_OVERRIDES,
};

/// 判据 1：.NET `InternalEncoderBestFitFallback` 的编码向量（含 best-fit / PUA / 非 BMP）。
#[test]
fn encode_matches_dotnet_vectors() {
    let cases: &[(char, &[u8])] = &[
        ('A', &[0x41]),
        ('一', &[0xD2, 0xBB]),
        ('\u{E7C8}', &[0xA8, 0xBF]), // cp936-only（WHATWG 解成 U+01F9）
        ('\u{20AC}', &[0x80]),
        ('\u{F8F5}', &[0xFF]),
        ('\u{00A6}', &[0x7C]), // best-fit：断线竖 -> '|'
        ('\u{00AA}', &[0x61]), // best-fit：ª -> 'a'
        ('\u{01F9}', &[0x3F]), // 99 个 WHATWG-only 之一
        ('\u{FFFD}', &[0x3F]),
        ('\u{0080}', &[0x3F]),
        ('\u{FFFF}', &[0x3F]),
        ('\u{1F600}', &[0x3F, 0x3F]), // 非 BMP：每个 UTF-16 码元一个 '?'
        ('\u{10400}', &[0x3F, 0x3F]),
    ];
    for (ch, want) in cases {
        assert_eq!(
            encode_gb2312_dotnet(&ch.to_string()),
            *want,
            "U+{:04X} 编码不符",
            *ch as u32
        );
    }
}

/// 判据 2：不可映射/非 BMP 绝不产生 `&#N;` 数字字符引用。
#[test]
fn encode_never_emits_numeric_character_references() {
    let sample = "\u{FFFD}\u{1F600}\u{0080}\u{FFFF}A\u{E7C8}\u{01F9}";
    let out = encode_gb2312_dotnet(sample);
    assert!(
        !out.windows(2).any(|w| w == b"&#"),
        "编码器输出了数字字符引用: {out:02X?}"
    );
    assert_eq!(
        out,
        vec![0x3F, 0x3F, 0x3F, 0x3F, 0x3F, 0x41, 0xA8, 0xBF, 0x3F]
    );
}

/// 判据 3：解码状态机（前导吞任意后继；0x80/0xFF 特例；末位孤立前导 → `?`）。
#[test]
fn decode_state_machine() {
    assert_eq!(decode(&[0x80]), "\u{20AC}");
    assert_eq!(decode(&[0xFF]), "\u{F8F5}");
    assert_eq!(decode(&[0xA8, 0xBF]), "\u{E7C8}");
    assert_eq!(decode(&[0x81, 0x40]), "\u{4E02}");
    assert_eq!(decode(&[0x81, 0x20, 0x41]), "?A"); // 0x20 被前导吞掉
    assert_eq!(decode(&[0x81, 0x0A, 0x41]), "?A"); // LF 被吞
    assert_eq!(decode(&[0x81, 0x2F]), "?");
    assert_eq!(decode(&[0x81, 0xFF]), "?");
    assert_eq!(decode(&[0x41, 0x81]), "A?"); // 末位孤立前导
    assert_eq!(decode(&[0x81, 0x00]), "?");
}

/// 判据 3b：7032 条覆盖表规模与关键项（B-105）。
#[test]
fn override_table_size_and_spot_checks() {
    assert_eq!(GBK_OVERRIDES.len(), 7_032, "覆盖表规模变化（B-105）");
    assert!(
        GBK_OVERRIDES
            .iter()
            .any(|(k, c)| *k == 0xA8BF && *c == '\u{E7C8}'),
        "A8BF → U+E7C8 映射缺失"
    );
}

/// 判据 4：`StringList.LoadFromFile` BOM 模型（`FF FE 00 00` = UTF-16LE）。
#[test]
fn string_list_bom_model() {
    let cases: &[(&[u8], &str)] = &[
        (
            &[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00],
            "\u{0000}A\u{0000}",
        ),
        (&[0xFF, 0xFE, 0xFF, 0xFE], "\u{FEFF}"),
        (&[0xFF, 0xFE, 0xFF, 0x41], "\u{41FF}"),
        (&[0xFF, 0xFE, 0x41, 0x00], "A"),
        (&[0xFE, 0xFF, 0x00, 0x41], "A"),
        (&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41], "A"),
        (&[0xEF, 0xBB, 0xBF, 0x41], "A"),
        (&[0xFF, 0xFE, 0x00], "\u{FFFD}"),
        (&[0xA8, 0xBF], "\u{E7C8}"),
        (&[0xFF], "\u{F8F5}"),
    ];
    for (bytes, want) in cases {
        assert_eq!(
            decode_string_list(bytes),
            *want,
            "StringList 向量 {bytes:02X?} 不符"
        );
    }
}

/// 判据 4b：`ConfigFile.Load` BOM 模型（裸 StreamReader ⇒ `FF FE 00 00` = UTF-32LE）。
#[test]
fn config_bom_model() {
    let cases: &[(&[u8], &str)] = &[
        (&[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00], "A"),
        (
            &[0xFF, 0xFE, 0x00, 0x00, 0x00, 0x00, 0xD8, 0x00],
            "\u{FFFD}",
        ),
        (
            &[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00, 0x42],
            "A\u{FFFD}",
        ),
        (&[0xFF, 0xFE, 0xFF, 0x41], "\u{41FF}"),
        (&[0xFE, 0xFF, 0x00, 0x41], "A"),
        (&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41], "A"),
        (&[0xEF, 0xBB, 0xBF, 0x41], "A"),
        (&[0xFF, 0xFE, 0x00], "\u{FFFD}"),
        (&[0xA8, 0xBF], "\u{E7C8}"),
    ];
    for (bytes, want) in cases {
        assert_eq!(
            decode_config_text(bytes),
            *want,
            "ConfigFile 向量 {bytes:02X?} 不符"
        );
    }
}

/// 判据 5：`decode_config_text` 是 `decode_stream_reader` 的规范名（逐字节同结果）。
#[test]
fn config_text_is_stream_reader_alias() {
    let vectors: &[&[u8]] = &[
        &[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00],
        &[0xFF, 0xFE, 0xFF, 0x41],
        &[0xFE, 0xFF, 0x00, 0x41],
        &[0xEF, 0xBB, 0xBF, 0x41],
        &[0xA8, 0xBF],
        b"[Names]\r\nGameGold=\xA8\xBF\r\n",
    ];
    for bytes in vectors {
        assert_eq!(
            decode_config_text(bytes),
            decode_stream_reader(bytes),
            "decode_config_text 与 decode_stream_reader 分叉: {bytes:02X?}"
        );
    }
}

/// 判据 6：合法 GBK 字符往返逐字节不变（ASCII + cp936-only 代表项）。
#[test]
fn valid_roundtrip_byte_identical() {
    for s in ["祈福项链", "一", "\u{E7C8}", "abcXYZ", "\u{20AC}"] {
        let bytes = encode_gb2312_dotnet(s);
        assert_eq!(decode(&bytes), s, "往返不符: {s}");
    }
}
