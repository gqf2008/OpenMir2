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
//! - 四处 `StringList.LoadFromFile` 读侧（`lists::read_gbk_if_file`、
//!   `monitems::parse_monitems_gbk`、`guild::parse_guild_list`、`guild::parse_guild_file`）
//!   改回裸 `decode`（丢 BOM 探测）⇒ 各自 `*_detects_*bom*` / `guild_list_ff_fe...` 必红。

use std::path::PathBuf;

use mir2_data::exps::{ExpsConfig, IniFile};
use mir2_data::guild::{parse_guild_file, parse_guild_list, serialize_guild_list};
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

/// UTF-16LE（带 `FF FE` BOM）。
fn utf16le_bom(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// `FF FE 00 00` 前缀 + UTF-16LE 载荷。`StringList` 模型把它当 UTF-16LE（**不是** UTF-32LE），
/// 所以 BOM 只吃掉 2 字节，载荷开头多出一个 `U+0000`。
fn utf16le_bom4(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE, 0x00, 0x00];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// 判据 14：`Envir/ItemNameList.txt` 读侧走 `StringList.LoadFromFile` 的 UTF-8 BOM 模型。
/// 裸 `decode` 会把 `EF BB BF` 当 cp936 前导对，首行整型解析失败 ⇒ 列表为空。
#[test]
fn item_name_list_file_detects_utf8_bom() {
    let dir = temp_dir("lists-utf8bom");
    let path = dir.join("ItemNameList.txt");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice("1073741869\t372\t祈福项链\n".as_bytes());
    std::fs::write(&path, &bytes).unwrap();

    let list = load_item_name_list_file(&path).unwrap().unwrap();
    assert_eq!(list.len(), 1, "UTF-8 BOM 未剥掉，首行整型解析失败");
    assert_eq!(
        list[0],
        mir2_data::lists::ItemName {
            make_index: 1073741869,
            item_index: 372,
            item_name: "祈福项链".to_string(),
        }
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 判据 15：MonItems 读侧走 `StringList.LoadFromFile` 的 UTF-16LE BOM 模型。
/// 裸 `decode` 下 `FF FE` 会被当 cp936 字节，切不出三个整型 token ⇒ 0 条。
#[test]
fn monitems_detects_utf16le_bom() {
    let bytes = utf16le_bom("1\t1\t祈福项链\t1\r\n");
    let list = parse_monitems_gbk(&bytes);
    assert_eq!(list.len(), 1, "UTF-16LE BOM 未识别，掉落项丢失");
    assert_eq!(list[0].item_name, "祈福项链");
    assert_eq!(list[0].count, 1);
}

/// 判据 16：`FF FE 00 00` 在 `StringList` 路径是 **UTF-16LE**（BOM 只吃 2 字节，
/// 载荷首字符 `U+0000`），不是 `ConfigFile.Load` 的 UTF-32LE。
/// 裸 `decode` 会把整段当 cp936，行切分也失效 ⇒ 名单条数不为 2。
#[test]
fn guild_list_ff_fe_00_00_is_utf16le() {
    let bytes = utf16le_bom4("A\r\nB\r\n");
    assert_eq!(
        parse_guild_list(&bytes),
        vec!["\u{0}A".to_string(), "B".to_string()],
        "FF FE 00 00 必须按 UTF-16LE 解码（首字符多一个 U+0000）"
    );
}

/// 判据 17：`Guilds/<名>.txt` 四段读侧走 `StringList.LoadFromFile` 的 UTF-8 BOM 模型。
/// 裸 `decode` 下段头匹配失败 ⇒ 公告/成员全空。
#[test]
fn guild_file_detects_utf8_bom() {
    use mir2_data::guild::{GuildFileSections, ORACLE_GUILD_MEMBER, ORACLE_GUILD_NOTICE};
    let sections = GuildFileSections::oracle_defaults();
    let src = format!(
        "{}\n+公告甲\n{}\n#1 掌门\n+甲会长\n",
        ORACLE_GUILD_NOTICE, ORACLE_GUILD_MEMBER
    );
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(src.as_bytes());

    let body = parse_guild_file(&bytes, &sections, 16, 5, |_| false);
    assert_eq!(
        body.notices,
        vec!["公告甲".to_string()],
        "UTF-8 BOM 破坏了段头匹配"
    );
    assert_eq!(body.ranks.len(), 1);
    assert_eq!(body.ranks[0].rank_name, "掌门");
    assert_eq!(body.ranks[0].members, vec!["甲会长".to_string()]);
}

/// 判据 18：无 BOM（现网 209 个 MonItems + 七名单 + 行会文件的实际形态）⇒ 仍走 cp936，
/// 与改造前逐字节同解。四类入口一起回归。
#[test]
fn no_bom_paths_stay_cp936() {
    // lists：A8 BF 是 cp936-only 对（WHATWG 会解成 U+01F9）。
    let dir = temp_dir("lists-nobom");
    let path = dir.join("ItemNameList.txt");
    std::fs::write(&path, b"1073741869\t372\t\xA8\xBF\n").unwrap();
    let items = load_item_name_list_file(&path).unwrap().unwrap();
    assert_eq!(items[0].item_name, "\u{E7C8}");
    std::fs::remove_dir_all(&dir).unwrap();

    // monitems。
    let drops = parse_monitems_gbk(b"1\t1\t\xA8\xBF\t1\n");
    assert_eq!(drops[0].item_name, "\u{E7C8}");

    // guild list。
    assert_eq!(
        parse_guild_list(b"\xA8\xBF\r\n"),
        vec!["\u{E7C8}".to_string()]
    );

    // guild file：GBK 段头 + 成员。
    use mir2_data::guild::{GuildFileSections, ORACLE_GUILD_MEMBER, ORACLE_GUILD_NOTICE};
    let sections = GuildFileSections::oracle_defaults();
    let src = format!(
        "{}\n+公告甲\n{}\n#1 掌门\n+甲会长\n",
        ORACLE_GUILD_NOTICE, ORACLE_GUILD_MEMBER
    );
    let gbk = mir2_encoding::encode_gb2312_dotnet(&src);
    let body = parse_guild_file(&gbk, &sections, 16, 0, |_| false);
    assert_eq!(body.notices, vec!["公告甲".to_string()]);
    assert_eq!(body.ranks[0].members, vec!["甲会长".to_string()]);
}
