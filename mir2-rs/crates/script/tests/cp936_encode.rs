//! E-1 / E-2 机判：.NET cp936 编码入口 + 解码统一入口。
//!
//! 探测表由 .NET 探针生成（`Encoding.GetEncoding(936)` 逐 BMP 码点 `GetBytes`），
//! 与 `audit-encoding-v2-20261021.md` §1.1/§2.3 的输出同源；测试直接调生产函数
//! `mir2_script::cp936::{encode_gb2312_dotnet, decode, decode_string_list}`，
//! 不复制实现。红检：把生产入口改回 `encoding_rs::GBK.encode`、把 BOM 顺序改回
//! `FF FE` 先于 `FF FE 00 00`、或改掉 `gbk_overrides.rs` 任一映射 => 对应断言必红。

use std::collections::HashMap;
use std::path::PathBuf;

use mir2_script::cp936;
use mir2_script::gbk_overrides::GBK_OVERRIDES;

fn data(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} 读取失败: {e}", path.display()))
}

/// `"A8BF"` -> `[0xA8, 0xBF]`。
fn hex_bytes(hex: &str) -> Vec<u8> {
    assert!(hex.len().is_multiple_of(2), "探测表 HEX 长度非法: {hex}");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("HEX 字节"))
        .collect()
}

fn parse_line(line: &str) -> (&str, &str) {
    line.split_once('\t').expect("探测表格式: KEY<TAB>VALUE")
}

fn char_of(cp: u16) -> char {
    char::from_u32(u32::from(cp)).expect("探测表不含代理码点")
}

/// E-1 判据 2（全表）：63,488 个非代理 BMP 码点逐点与 .NET 表相等。
/// 这条同时覆盖 `U+00A6->7C`、`U+F8F5->FF`、PUA、兼容汉字等 best-fit 项。
#[test]
fn full_bmp_table_matches_dotnet() {
    let text = data("cp936-encode-bmp.txt");
    let mut checked = 0usize;
    for (lineno, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let (key, hex) = parse_line(line);
        let cp = u16::from_str_radix(key, 16).expect("码点");
        let expected = hex_bytes(hex);
        let got = cp936::encode_gb2312_dotnet(&char_of(cp).to_string());
        assert_eq!(
            got,
            expected,
            "U+{cp:04X}（第 {} 行）编码不符: {got:02X?} != {expected:02X?}",
            lineno + 1
        );
        checked += 1;
    }
    assert_eq!(checked, 63_488, "BMP 编码表规模变化");
}

/// E-1 判据 1：99 个 WHATWG-only 码点（WHATWG 有真实字节、.NET 回退 `?`）必须编成
/// **单个 `0x3F`**。只查 `had_errors` 发现不了这类差异，因此必须查 .NET 表。
/// E-2 解码侧证据：这些码点对应的 WHATWG 字节对，在 .NET 下解成**另一个字符**，
/// 且该字节对已被 7032 条覆盖表（B-105）收录。
#[test]
fn whatwg_only_encode_to_question_and_decode_is_covered() {
    let text = data("cp936-encode-diff-whatwg.txt");
    let mut checked = 0usize;
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let (cp_s, hex) = parse_line(line);
        let cp = u16::from_str_radix(&cp_s[2..], 16).expect("U+XXXX");
        let ch = char_of(cp);
        assert_eq!(
            cp936::encode_gb2312_dotnet(&ch.to_string()),
            vec![0x3F],
            "WHATWG-only U+{cp:04X} 应回退单个 '?'"
        );

        let key = u16::from_str_radix(hex, 16).expect("WHATWG 字节对");
        let pair = [(key >> 8) as u8, (key & 0xFF) as u8];
        let dotnet = cp936::decode(&pair);
        let mut chars = dotnet.chars();
        let first = chars.next().expect("解码非空");
        assert_eq!(chars.next(), None, "U+{cp:04X} 的字节对解码多字符");
        assert_ne!(
            first, ch,
            "U+{cp:04X}: cp936 解码不应等于 WHATWG 码点（覆盖表失效）"
        );
        assert!(
            GBK_OVERRIDES.iter().any(|(k, c)| *k == key && *c == first),
            "U+{cp:04X} 的字节对 {key:04X} 未被 7032 条覆盖表收录"
        );
        checked += 1;
    }
    assert_eq!(checked, 99, "WHATWG-only 向量规模变化");
}

/// E-2：496 个 cp936-only 码点（.NET 有真实字节、WHATWG 不可编码）按 oracle 字节编码；
/// 其中解码可逆的项（`encode(decode(pair)) == pair`）必须逐字节往返。
#[test]
fn cp936_only_encode_and_decode_roundtrip() {
    let roundtrip = roundtrip_map();
    let text = data("cp936-encode-diff-cp936.txt");
    let mut checked = 0usize;
    let mut reversible = 0usize;
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let (cp_s, hex) = parse_line(line);
        let cp = u16::from_str_radix(&cp_s[2..], 16).expect("U+XXXX");
        let ch = char_of(cp);
        let expected = hex_bytes(hex);
        let got = cp936::encode_gb2312_dotnet(&ch.to_string());
        assert_eq!(
            got, expected,
            "cp936-only U+{cp:04X} 编码不符（不得回退 '?'）"
        );
        assert_ne!(got, vec![0x3F], "cp936-only U+{cp:04X} 被错误回退成 '?'");

        if expected.len() == 2 {
            let key = (u16::from(expected[0]) << 8) | u16::from(expected[1]);
            // cp936-only 里既有 `U+00A2 -> A1E9` 这类 best-fit 别名（字节对解码回
            // 规范字符 `U+FFE0`），也有真正可逆项；两者都必须 encode(decode(pair)) == pair。
            let canonical = cp936::decode(&expected);
            assert_eq!(
                cp936::encode_gb2312_dotnet(&canonical),
                expected,
                "字节对 {key:04X} 的规范解码不可逆"
            );
            if roundtrip.get(&key) == Some(&ch) {
                assert_eq!(canonical, ch.to_string());
                reversible += 1;
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 496, "cp936-only 向量规模变化");
    assert!(reversible > 0, "没有任何 cp936-only 项可解码往返");
}

/// E-1：非 BMP 按 UTF-16 码元回退（`U+1F600` -> `3F 3F`），BMP 不可映射 -> 单个 `3F`；
/// 生产编码器绝不输出 `&#N;` 数字字符引用。
#[test]
fn non_bmp_and_unmapped_fallback_never_numeric_reference() {
    assert_eq!(cp936::encode_gb2312_dotnet("\u{1F600}"), vec![0x3F, 0x3F]);
    assert_eq!(cp936::encode_gb2312_dotnet("\u{10400}"), vec![0x3F, 0x3F]);
    assert_eq!(cp936::encode_gb2312_dotnet("\u{10FFFF}"), vec![0x3F, 0x3F]);
    assert_eq!(
        cp936::encode_gb2312_dotnet("A\u{1F600}B"),
        vec![0x41, 0x3F, 0x3F, 0x42]
    );
    assert_eq!(cp936::encode_gb2312_dotnet("\u{FFFD}"), vec![0x3F]);
    assert_eq!(cp936::encode_gb2312_dotnet("\u{0080}"), vec![0x3F]);
    assert_eq!(cp936::encode_gb2312_dotnet("\u{FFFF}"), vec![0x3F]);
    assert_eq!(cp936::encode_gb2312_dotnet("一"), vec![0xD2, 0xBB]);
    assert_eq!(cp936::encode_gb2312_dotnet("\u{20AC}"), vec![0x80]);
    assert_eq!(cp936::encode_gb2312_dotnet("\u{F8F5}"), vec![0xFF]);
    assert_eq!(cp936::encode_gb2312_dotnet("\u{E7C8}"), vec![0xA8, 0xBF]);

    let sample = "\u{FFFD}\u{1F600}\u{0080}\u{FFFF}A\u{E7C8}";
    let out = cp936::encode_gb2312_dotnet(sample);
    assert!(
        !out.windows(2).any(|w| w == b"&#"),
        "编码器输出了数字字符引用: {out:02X?}"
    );

    for cp in [0xFFFDu32, 0x1F600, 0x3013, 0xFFFF, 0xE7C8] {
        let ch = char::from_u32(cp).unwrap();
        assert_eq!(
            mir2_script::textfile::encode_gb2312(&ch.to_string()),
            cp936::encode_gb2312_dotnet(&ch.to_string()),
            "textfile::encode_gb2312 未委托统一入口 (U+{cp:04X})"
        );
    }
}

/// E-2 / E-4：BOM 顺序按 .NET `StreamReader` 实测 —— `FF FE 00 00` 是 UTF-32LE，
/// **必须先于** `FF FE`（UTF-16LE）判定。
#[test]
fn bom_order_utf32le_before_utf16le() {
    let vectors: &[(&[u8], &str)] = &[
        (&[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00], "A"),
        (&[0xFF, 0xFE, 0x41, 0x00], "A"),
        (&[0xFF, 0xFE, 0xFF, 0x41], "\u{41FF}"),
        (&[0xFE, 0xFF, 0x00, 0x41], "A"),
        (&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41], "A"),
        (&[0xEF, 0xBB, 0xBF, 0x41], "A"),
        (
            &[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x11, 0x00, 0x00],
            "\u{FFFD}",
        ),
        (
            &[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0xD8, 0x00],
            "\u{FFFD}",
        ),
        (
            &[0xFF, 0xFE, 0x00, 0x00, 0x00, 0x00, 0xD8, 0x00],
            "\u{FFFD}",
        ),
        (
            &[0xFF, 0xFE, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00],
            "\u{FFFF}",
        ),
        (
            &[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00, 0x42],
            "A\u{FFFD}",
        ),
        (&[0xFF, 0xFE, 0x00, 0xD8, 0x41, 0x00], "\u{FFFD}A"),
        (&[0xFF, 0xFE, 0x00], "\u{FFFD}"),
        (&[0xA8, 0xBF], "\u{E7C8}"),
        (&[0xFF], "\u{F8F5}"),
        (&[0x80], "\u{20AC}"),
        (&[0x81, 0x20, 0x41], "?A"),
    ];
    for (bytes, want) in vectors {
        assert_eq!(
            cp936::decode_string_list(bytes),
            *want,
            "BOM/解码向量 {bytes:02X?} 不符"
        );
        assert_eq!(mir2_script::textfile::decode_bytes(bytes), *want);
    }
}

/// B-105：7032 条覆盖表仍生效（解码侧 oracle），且编码侧可逆。
#[test]
fn override_table_still_effective() {
    assert_eq!(GBK_OVERRIDES.len(), 7_032, "覆盖表规模变化（B-105）");
    assert_eq!(cp936::decode(&[0xA8, 0xBF]), "\u{E7C8}");
    assert_eq!(cp936::encode_gb2312_dotnet("\u{E7C8}"), vec![0xA8, 0xBF]);
    assert_eq!(cp936::decode(&[0x81, 0x00]), "?");
    assert_eq!(cp936::decode(&[0x81, 0x40]), "\u{4E02}");
    assert_eq!(cp936::decode(&[0x81, 0x2F]), "?");
    assert_eq!(cp936::decode(&[0x81, 0xFF]), "?");
    assert_eq!(cp936::decode(&[0x81, 0x0A, 0x41]), "?A");
    assert_eq!(cp936::decode(&[0x41, 0x81]), "A?");
}

/// E-1：合法 GBK 往返逐字节不变（ASCII + 所有 `encode(decode(pair)) == pair` 的双字节对）。
#[test]
fn valid_gbk_roundtrip_byte_identical() {
    let mut checked = 0usize;
    let text = data("cp936-roundtrip.txt");
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let (key, cp_s) = parse_line(line);
        let key = u16::from_str_radix(key, 16).expect("字节对");
        let cp = u32::from_str_radix(&cp_s[2..], 16).expect("U+XXXX");
        let ch = char::from_u32(cp).expect("合法字符");
        // 单字节项（ASCII/0x80/0xFF）的 key 是 0x00XX；双字节对 key >= 0x8100。
        let bytes: Vec<u8> = if key <= 0xFF {
            vec![key as u8]
        } else {
            vec![(key >> 8) as u8, (key & 0xFF) as u8]
        };
        assert_eq!(
            cp936::decode(&bytes),
            ch.to_string(),
            "字节对 {key:04X} 解码不符"
        );
        assert_eq!(
            cp936::encode_gb2312_dotnet(&ch.to_string()),
            bytes,
            "字符 U+{cp:04X} 往返字节不符"
        );
        checked += 1;
    }
    assert_eq!(checked, 24_070, "往返向量规模变化");

    let phrase = "祈福项链";
    let bytes = cp936::encode_gb2312_dotnet(phrase);
    assert_eq!(bytes, vec![0xC6, 0xED, 0xB8, 0xA3, 0xCF, 0xEE, 0xC1, 0xB4]);
    assert_eq!(cp936::decode(&bytes), phrase);
}

fn roundtrip_map() -> HashMap<u16, char> {
    let mut map = HashMap::new();
    for line in data("cp936-roundtrip.txt").lines() {
        if line.is_empty() {
            continue;
        }
        let (key, cp_s) = parse_line(line);
        let key = u16::from_str_radix(key, 16).expect("字节对");
        let cp = u32::from_str_radix(&cp_s[2..], 16).expect("U+XXXX");
        map.insert(key, char::from_u32(cp).expect("合法字符"));
    }
    map
}
