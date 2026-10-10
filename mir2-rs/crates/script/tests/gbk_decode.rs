//! GB2312(cp936) 解码的穷举门禁：对探测表里全部 32256 个双字节对 + 边界单字节，
//! 断言 `decode_bytes` 的结果与 .NET cp936 完全一致（探测表由 C# 探针生成，见 tools/gbk-probe）。
//!
//! 红检方式：改动 `gbk_overrides.rs` 任一映射或 `textfile.rs` 的字节状态机 → 该测试必须红。

use std::path::PathBuf;

fn probe_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/cp936-pairs.txt")
}

#[test]
fn all_cp936_pairs_match_dotnet() {
    let text = std::fs::read_to_string(probe_path()).expect("探测表缺失");
    let mut checked = 0usize;
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (key, val) = line.split_once('\t').expect("探测表格式");
        let key = u16::from_str_radix(key, 16).expect("字节对");
        // FE FF 是 UTF-16BE BOM：decode_bytes 与 C# GetEncoding 一样先按 BOM 判定
        // （见 bom 测试），不属于 gb2312 分支口径，此处跳过。
        if key == 0xFEFF {
            continue;
        }
        let bytes = [(key >> 8) as u8, (key & 0xFF) as u8];
        let expected: String = val
            .split(',')
            .map(|cp| char::from_u32(u32::from_str_radix(cp, 16).unwrap()).unwrap())
            .collect();
        let got = mir2_script::decode_bytes(&bytes);
        assert_eq!(
            got,
            expected,
            "字节对 {key:04X}（第 {} 行）解码不符",
            lineno + 1
        );
        checked += 1;
    }
    // 探测表行数锚定（cp936 双字节对总数：0x81..=0xFE × 0x00..=0xFF）
    assert_eq!(checked, 126 * 256 - 1, "探测表规模变化（已排除 FE FF BOM）");
}

#[test]
fn single_byte_edges_match_dotnet() {
    // .NET cp936 实测：0x80 → U+20AC（欧元，单字节）；0xFF → U+F8F5（PUA，单字节）；
    // 孤立引导字节 → '?'
    assert_eq!(mir2_script::decode_bytes(&[0x80]), "\u{20AC}");
    assert_eq!(mir2_script::decode_bytes(&[0xFF]), "\u{F8F5}");
    assert_eq!(mir2_script::decode_bytes(&[0x81]), "?");
    assert_eq!(mir2_script::decode_bytes(&[0xA8]), "?");
    // 0x80/0xFF 不吞后续字节
    assert_eq!(mir2_script::decode_bytes(&[0x80, 0x41]), "\u{20AC}A");
    assert_eq!(mir2_script::decode_bytes(&[0xFF, 0x41]), "\u{F8F5}A");
    assert_eq!(mir2_script::decode_bytes(&[0x41, 0x80]), "A\u{20AC}");
}
