//! `crates/envir` 与 `crates/script` 共用同一 cp936 入口（E-1 / E-2 / E-4）。
//!
//! 红检：把 `envir::text::decode_text` 改回裸 `encoding_rs::GBK.decode` ⇒
//! `decode_text_matches_dotnet` 的 `0xFF` / `FF FE FF` / UTF-32B* 条必红；
//! 把 `GameSvrConf::from_bytes` 的重编码改回 `encoding_rs::GBK.encode` ⇒
//! `conf_reencode_uses_shared_cp936_encoder` 必红（`U+E7C8` 会变成 `&#59336;`）。

use mir2_envir::{decode_text, GameSvrConf};

#[test]
fn decode_text_matches_dotnet() {
    // cp936 边界：0xFF→U+F8F5、0x80→U+20AC、前导吞任意后继、覆盖表 A8BF。
    assert_eq!(decode_text(&[0xFF]), "\u{F8F5}");
    assert_eq!(decode_text(&[0x80]), "\u{20AC}");
    assert_eq!(decode_text(&[0xA8, 0xBF]), "\u{E7C8}");
    assert_eq!(decode_text(&[0x81, 0x20, 0x41]), "?A");

    // BOM 探测（.NET StreamReader 实测）：UTF-32LE 必须先于 UTF-16LE。
    assert_eq!(
        decode_text(&[0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00]),
        "A"
    );
    // 旧 `envir::text` 会落到 gb2312（第 3 字节 0xFF）并改变结果。
    assert_eq!(decode_text(&[0xFF, 0xFE, 0xFF, 0x41]), "\u{41FF}");
    assert_eq!(
        decode_text(&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41]),
        "A"
    );
    assert_eq!(decode_text(&[0xFE, 0xFF, 0x00, 0x41]), "A");
    assert_eq!(decode_text(&[0xEF, 0xBB, 0xBF, 0x41]), "A");
    // UTF 分支非法序列 → U+FFFD（不是 ?）
    assert_eq!(decode_text(&[0xEF, 0xBB, 0xBF, 0xFF, 0x41]), "\u{FFFD}A");
    assert_eq!(
        decode_text(&[0xFF, 0xFE, 0x00, 0x00, 0x00, 0x00, 0xD8, 0x00]),
        "\u{FFFD}"
    );
}

#[test]
fn conf_reencode_uses_shared_cp936_encoder() {
    // 无 BOM 的 GBK `Server.conf`：值含 A8 BF（cp936 `U+E7C8`）。
    // `IniFile::from_gbk_bytes` 用 WHATWG 解码，A8 BF → U+01F9；
    // 若重编码退回 encoding_rs（U+E7C8 → "&#59336;"），值会变成该 ASCII 串。
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"[Names]\r\nGameGold=");
    bytes.extend_from_slice(&[0xA8, 0xBF]);
    bytes.extend_from_slice(b"\r\nGamePoint=");
    bytes.extend_from_slice(&[0xD2, 0xBB]); // "一"
    bytes.extend_from_slice(b"\r\n");
    let conf = GameSvrConf::from_bytes(&bytes);
    assert_eq!(conf.game_gold_name, "\u{01F9}");
    assert_eq!(conf.game_point_name, "一");

    // 默认值路径（无该键）不受影响。
    let conf2 = GameSvrConf::from_bytes(b"[Names]\r\n");
    assert_eq!(conf2.game_gold_name, "元宝");
}
