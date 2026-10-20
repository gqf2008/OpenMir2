//! `crates/data` 生产编码调用点已切到 `mir2_encoding`（E-4），且字节口径与 .NET cp936 一致。
//!
//! 红检（改坏生产路径 ⇒ 指定断言必红）：
//! - `guild::encode_lines_gbk` 改回 `encoding_rs::GBK.encode` ⇒
//!   `guild_serialize_never_emits_numeric_reference` 必红（会出现 `&#128512;`）；
//! - `guild::parse_guild_list` / `parse_guild_file` 改回 WHATWG ⇒
//!   `guild_list_roundtrip_uses_dotnet_cp936` 必红（`A8 BF` 变 `U+01F9`）；
//! - `IniFile::from_bytes` 改回手写 BOM 分支 ⇒ `ini_from_bytes_*` 的 UTF-32 条必红；
//! - `IniFile::from_gbk_bytes` 改回 WHATWG ⇒ `ini_from_gbk_bytes_uses_cp936` 必红；
//! - `lists::read_gbk_if_file` / `monitems::parse_monitems_gbk` 改回 WHATWG ⇒
//!   对应 `*_uses_cp936` 必红。

use std::path::PathBuf;

use mir2_data::exps::{ExpsConfig, IniFile};
use mir2_data::guild::{parse_guild_list, serialize_guild_list};
use mir2_data::lists::load_item_name_list_file;
use mir2_data::monitems::parse_monitems_gbk;

/// 让 ASCII/etc 文本按 UTF-32 编码（带 BOM）用于 BOM 模型判据。
fn utf32(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = if little_endian {
        vec![0xFF, 0xFE, 0x00, 0x00]
    } else {
        vec![0x00, 0x00, 0xFE, 0xFF]
    };
    for unit in text.encode_utf16() {
        let cp = u32::from(unit);
        let raw = if little_endian {
            cp.to_le_bytes()
        } else {
            cp.to_be_bytes()
        };
        out.extend_from_slice(&raw);
    }
    out
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mir2-enc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 判据 7：行会名单写侧 = .NET cp936（`StringList.SaveToFile`），读侧同口径可往返。
#[test]
fn guild_list_roundtrip_uses_dotnet_cp936() {
    let names = vec!["祈福项链".to_string(), "\u{E7C8}".to_string()];
    let bytes = serialize_guild_list(&names);
    assert_eq!(
        bytes,
        vec![0xC6, 0xED, 0xB8, 0xA3, 0xCF, 0xEE, 0xC1, 0xB4, 0x0D, 0x0A, 0xA8, 0xBF, 0x0D, 0x0A],
        "行会名单字节不是 .NET cp936"
    );
    assert_eq!(parse_guild_list(&bytes), names);
    // WHATWG 会把 A8 BF 解成 U+01F9 —— 生产读侧必须是 U+E7C8。
    assert_eq!(
        parse_guild_list(&[0xA8, 0xBF]),
        vec!["\u{E7C8}".to_string()]
    );
}

/// 判据 8：写回不可映射字符 → 单个 `?`，绝不写 `&#N;`（会把名单文件写坏）。
#[test]
fn guild_serialize_never_emits_numeric_reference() {
    let bytes = serialize_guild_list(&["A\u{1F600}B".to_string()]);
    assert_eq!(bytes, b"A??B\r\n".to_vec());
    assert!(!bytes.windows(2).any(|w| w == b"&#"));
}

/// 判据 9：`IniFile` 的 GBK 入口是 .NET cp936（cp936-only 字节对不再变 U+01F9）。
#[test]
fn ini_from_gbk_bytes_uses_cp936() {
    let ini = IniFile::from_gbk_bytes(b"[Exp]\nName=\xA8\xBF\n");
    assert_eq!(ini.read_string("Exp", "Name", ""), "\u{E7C8}");
}

/// 判据 10：`IniFile::from_bytes` = `ConfigFile.Load` 的裸 StreamReader 模型。
/// `FF FE 00 00` 是 UTF-32LE（旧手写分支会当成 UTF-16LE）。
#[test]
fn ini_from_bytes_uses_configfile_bom_model() {
    let le = utf32("[Exp]\nA=9\n", true);
    assert_eq!(IniFile::from_bytes(&le).read_integer("Exp", "A", 0), 9);

    // `00 00 FE FF` 是 UTF-32BE（旧手写分支没有，落到 GBK 会解析失败）。
    let be = utf32("[Exp]\nA=7\n", false);
    assert_eq!(IniFile::from_bytes(&be).read_integer("Exp", "A", 0), 7);

    // 无 BOM 的 cp936：A8 BF 的键值可读。
    let ini = IniFile::from_bytes(b"[Exp]\nName=\xA8\xBF\n");
    assert_eq!(ini.read_string("Exp", "Name", ""), "\u{E7C8}");
}

/// 判据 11：`ExpsConfig::from_file` 走 `ConfigFile.Load`（BOM 探测），不是裸 GBK。
#[test]
fn exps_from_file_detects_bom() {
    let dir = temp_dir("exps");
    let path = dir.join("Exps.conf");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"[Exp]\nLevel1=123\n");
    std::fs::write(&path, &bytes).unwrap();
    let cfg = ExpsConfig::from_file(&path).unwrap();
    assert_eq!(cfg.need_exps[1], 123);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 判据 12：`Envir/*.txt` 名单读侧 = .NET cp936。
#[test]
fn item_name_list_file_uses_cp936() {
    let dir = temp_dir("lists");
    let path = dir.join("ItemNameList.txt");
    std::fs::write(&path, b"1073741869\t372\t1000\n26282853\t443\t\xA8\xBF\n").unwrap();
    let list = load_item_name_list_file(&path).unwrap().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[1].item_name, "\u{E7C8}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 判据 13：MonItems 掉落表读侧 = .NET cp936。
#[test]
fn monitems_uses_cp936() {
    let list = parse_monitems_gbk(b"1\t1\t\xA8\xBF\t1\n");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].item_name, "\u{E7C8}");
}
