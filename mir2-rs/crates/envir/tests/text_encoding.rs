//! `crates/envir` 与 `crates/script` 共用同一 cp936 入口（E-1 / E-2 / E-4），但
//! **BOM 模型按 oracle 调用点分两套**：
//! - [`decode_text`] = `StringList.LoadFromFile`（`GetEncoding` + `StreamReader`）：
//!   `FF FE 00 00` / `FF FE FF FE` 都是 **UTF-16LE**；
//! - [`decode_config_text`] = `ConfigFile.Load`（裸 `StreamReader(..., gb2312)`）：
//!   `FF FE 00 00` 是 **UTF-32LE**。
//!
//! 红检：
//! - 把 `envir::text::decode_text` 改回裸 `encoding_rs::GBK.decode` ⇒
//!   `decode_text_matches_dotnet` 的 `0xFF` / `FF FE FF` / UTF-32BE 条必红；
//! - 把 `decode_string_list` 的 `FF FE` 分支改回“`FF FE 00 00` 先判 UTF-32LE” ⇒
//!   StringList 的 `FF FE 00 00` 条必红（会从 `U+0000 U+0041 U+0000` 变成 `U+0041`）；
//! - 把 `GameSvrConf::from_bytes` 改回“cp936 解码 → 重编码 → `from_gbk_bytes`” ⇒
//!   `conf_from_bytes_uses_dotnet_cp936_once` 必红（`U+E7C8` 变 `U+01F9`）。

use mir2_envir::{decode_config_text, decode_text, GameSvrConf};

#[test]
fn decode_text_matches_dotnet() {
    // cp936 边界：0xFF→U+F8F5、0x80→U+20AC、前导吞任意后继、覆盖表 A8BF。
    assert_eq!(decode_text(&[0xFF]), "\u{F8F5}");
    assert_eq!(decode_text(&[0x80]), "\u{20AC}");
    assert_eq!(decode_text(&[0xA8, 0xBF]), "\u{E7C8}");
    assert_eq!(decode_text(&[0x81, 0x20, 0x41]), "?A");

    // StringList.LoadFromFile（GetEncoding 先给 Encoding.Unicode）：
    // `FF FE 00 00` 保持 UTF-16LE，首字符 U+0000 —— **不是** UTF-32LE。
    assert_eq!(
        decode_text(&[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00]),
        "\u{0000}A\u{0000}"
    );
    // `FF FE FF` 虽让 GetEncoding 保持 gb2312，StreamReader 的 BOM 探测仍是 UTF-16LE。
    assert_eq!(decode_text(&[0xFF, 0xFE, 0xFF, 0x41]), "\u{41FF}");
    assert_eq!(decode_text(&[0xFF, 0xFE, 0xFF, 0xFE]), "\u{FEFF}");
    assert_eq!(
        decode_text(&[0xFF, 0xFE, 0x00, 0x00, 0x00, 0x00, 0xD8, 0x00]),
        "\u{0000}\u{0000}\u{00D8}"
    );
    assert_eq!(
        decode_text(&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41]),
        "A"
    );
    assert_eq!(decode_text(&[0xFE, 0xFF, 0x00, 0x41]), "A");
    assert_eq!(decode_text(&[0xEF, 0xBB, 0xBF, 0x41]), "A");
    assert_eq!(decode_text(&[0xFF, 0xFE, 0x00]), "\u{FFFD}");
    // UTF 分支非法序列 → U+FFFD（不是 ?）
    assert_eq!(decode_text(&[0xEF, 0xBB, 0xBF, 0xFF, 0x41]), "\u{FFFD}A");
}

#[test]
fn decode_config_text_uses_stream_reader_model() {
    // ConfigFile.Load 是裸 `StreamReader(stream, gb2312)`：BOM 探测会把
    // `FF FE 00 00` 升级为 UTF-32LE（与 StringList 的调用点不同）。
    assert_eq!(
        decode_config_text(&[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00]),
        "A"
    );
    assert_eq!(
        decode_config_text(&[0xFF, 0xFE, 0x00, 0x00, 0x00, 0x00, 0xD8, 0x00]),
        "\u{FFFD}"
    );
    assert_eq!(
        decode_config_text(&[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00, 0x42]),
        "A\u{FFFD}"
    );
    // 其余 BOM/无 BOM 分支与 StringList 相同。
    assert_eq!(decode_config_text(&[0xFF, 0xFE, 0xFF, 0x41]), "\u{41FF}");
    assert_eq!(decode_config_text(&[0xFF, 0xFE, 0xFF, 0xFE]), "\u{FEFF}");
    assert_eq!(decode_config_text(&[0xFE, 0xFF, 0x00, 0x41]), "A");
    assert_eq!(
        decode_config_text(&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41]),
        "A"
    );
    assert_eq!(decode_config_text(&[0xEF, 0xBB, 0xBF, 0x41]), "A");
    assert_eq!(decode_config_text(&[0xFF, 0xFE, 0x00]), "\u{FFFD}");
    assert_eq!(decode_config_text(&[0xA8, 0xBF]), "\u{E7C8}");
}

#[test]
fn conf_from_bytes_uses_dotnet_cp936_once() {
    // 无 BOM 的 GBK `Server.conf`：值含 A8 BF（cp936 `U+E7C8`）。
    // 若把文本重编码回 GBK 再让 `IniFile::from_gbk_bytes` 用 WHATWG 解码，
    // A8 BF 会被二次解释成 U+01F9 —— 这就是要防的回归。
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"[Names]\r\nGameGold=");
    bytes.extend_from_slice(&[0xA8, 0xBF]);
    bytes.extend_from_slice(b"\r\nGamePoint=");
    bytes.extend_from_slice(&[0xD2, 0xBB]); // "一"
    bytes.extend_from_slice(b"\r\n");
    let conf = GameSvrConf::from_bytes(&bytes);
    assert_eq!(conf.game_gold_name, "\u{E7C8}");
    assert_eq!(conf.game_point_name, "一");

    // 默认值路径（无该键）不受影响。
    let conf2 = GameSvrConf::from_bytes(b"[Names]\r\n");
    assert_eq!(conf2.game_gold_name, "元宝");
}

/// 判据 14：`crates/envir` 的两个入口就是 `mir2_encoding` 的转发（E-4 统一入口），
/// 改回 `mir2_script::cp936` 或自写分支 ⇒ 本测试仍可能过，但下面的逐向量相等把
/// 「两个真源」钉死；配合 `decode_text_matches_dotnet` 的边界向量即改坏必红。
#[test]
fn envir_text_delegates_to_encoding_crate() {
    let vectors: &[&[u8]] = &[
        &[0xFF],
        &[0x80],
        &[0xA8, 0xBF],
        &[0x81, 0x20, 0x41],
        &[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00],
        &[0xFF, 0xFE, 0xFF, 0x41],
        &[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41],
    ];
    for v in vectors {
        assert_eq!(decode_text(v), mir2_encoding::decode_string_list(v));
        assert_eq!(decode_config_text(v), mir2_encoding::decode_config_text(v));
    }
}
