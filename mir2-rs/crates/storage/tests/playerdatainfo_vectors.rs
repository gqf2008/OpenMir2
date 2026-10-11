//! S4a 门禁：`CharacterDataInfo` 等大结构的 **C# oracle 字节向量**逐字节对拍。
//!
//! 向量由 `tests/parity/oracle`（引用真身 `OpenMir2.dll` 的 `SerializerUtil` →
//! `MemoryPackSerializerOptions.Utf16`）生成，落在 `tests/parity/vectors/oracle_vectors.jsonl`
//! 的 `kind:"playerdatainfo"` 行。这里把每条向量在 Rust 侧**重建**（同一组字段值），要求
//! `to_bytes` 逐字节相同，且 `from_bytes` 能读回同一个值——只写"能解析"不算数。
//!
//! 破坏者自检：`mutation_changes_bytes` 改一个字段必须让字节变化（防自洽往返盲区）。

#![allow(clippy::doc_markdown)]
// 与 C# 一致：先 `new CharacterData()`（构造函数默认）再逐字段赋值。
#![allow(clippy::field_reassign_with_default)]

use std::path::{Path, PathBuf};

use mir2_storage::playerdata::{
    Ability, CharacterData, CharacterDataInfo, LoadCharacterData, LoadPlayerDataPacket, MagicRcd,
    MemPack, NakedAbility, RecordHeader, SaveCharacterData, ServerRequestData,
    ServerRequestMessage, ServerUserItem,
};

fn vectors_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/vectors/oracle_vectors.jsonl")
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

/// 取 `kind:"playerdatainfo"` 且 `name` 匹配的向量 hex。
fn vector(name: &str) -> Vec<u8> {
    let text = std::fs::read_to_string(vectors_path()).expect("read oracle_vectors.jsonl");
    for line in text.lines() {
        if !line.contains("\"kind\":\"playerdatainfo\"") {
            continue;
        }
        let name_key = format!("\"name\":\"{name}\"");
        if !line.contains(&name_key) {
            continue;
        }
        let start = line.find("\"hex\":\"").expect("hex key") + 7;
        let end = line[start..].find('"').expect("hex end") + start;
        return unhex(&line[start..end]);
    }
    panic!("vector {name} not found in {}", vectors_path().display());
}

fn ability() -> Ability {
    Ability {
        level: 3,
        reserved1: 0,
        ac: 1,
        mac: 2,
        dc: 3,
        mc: 4,
        sc: 5,
        hp: 6,
        mp: 7,
        max_hp: 8,
        max_mp: 9,
        reserved2: -2,
        exp_count: 1,
        exp_max_count: 2,
        exp: 100,
        max_exp: 200,
        weight: 10,
        max_weight: 11,
        wear_weight: 12,
        max_wear_weight: 13,
        hand_weight: 14,
        max_hand_weight: 15,
    }
}

fn naked() -> NakedAbility {
    NakedAbility {
        dc: 1,
        mc: 2,
        sc: 3,
        ac: 4,
        mac: 5,
        hp: 6,
        mp: 7,
        hit: 8,
        speed: 9,
        reserved: 10,
    }
}

fn magic() -> MagicRcd {
    MagicRcd {
        mag_idx: 0x1234,
        level: 5,
        magic_key: u16::from(b'x'),
        tran_point: 999,
    }
}

fn item() -> ServerUserItem {
    let mut it = ServerUserItem::csharp_new();
    it.make_index = 0x0102_0304;
    it.index = 0x0506;
    it.dura = 0x0708;
    it.dura_max = 0x090A;
    it.color_r = 1;
    it.color_g = 2;
    it.color_b = 3;
    it.desc.as_mut().unwrap()[0] = 0xAA;
    it.desc.as_mut().unwrap()[13] = 0xBB;
    it.prefix.as_mut().unwrap()[0] = u16::from(b'p');
    it.prefix.as_mut().unwrap()[12] = u16::from(b'q');
    it
}

fn header() -> RecordHeader {
    RecordHeader {
        s_account: Some("acct".into()),
        name: Some("aaa".into()),
        select_id: 7,
        d_create_date: 1.5,
        deleted: false,
        update_date: 2.5,
        create_date: 3.5,
    }
}

fn full_character_data() -> CharacterData {
    let mut cd = CharacterData::default();
    cd.server_index = 2;
    cd.chr_name = Some("aaa".into());
    cd.cur_map = Some("0".into());
    cd.cur_x = 300;
    cd.cur_y = 301;
    cd.dir = 2;
    cd.hair = 1;
    cd.sex = 0;
    cd.job = 1;
    cd.gold = 12345;
    cd.abil = Some(ability());
    cd.status_time_arr.as_mut().unwrap()[0] = 11;
    cd.status_time_arr.as_mut().unwrap()[14] = 22;
    cd.home_map = Some("1".into());
    cd.home_x = 5;
    cd.home_y = 6;
    cd.bonus_abil = Some(naked());
    cd.bonus_point = 7;
    cd.credit_point = 8;
    cd.re_level = 9;
    cd.master_name = Some("m".into());
    cd.is_master = true;
    cd.dear_name = Some("d".into());
    cd.storage_pwd = Some("p".into());
    cd.game_gold = 10;
    cd.game_point = 11;
    cd.pay_ment_point = 12;
    cd.pk_point = 13;
    cd.allow_group = 1;
    cd.bt_f9 = 2;
    cd.attatck_mode = 3;
    cd.inc_health = 4;
    cd.inc_spell = 5;
    cd.inc_healing = 6;
    cd.fight_zone_die_count = 7;
    cd.bt_ee = 8;
    cd.bt_ef = 9;
    cd.account = Some("acct".into());
    cd.lock_logon = true;
    cd.contribution = 14;
    cd.hunger_status = 15;
    cd.allow_guild_re_call = true;
    cd.group_rcall_time = 16;
    cd.body_luck = 2.5;
    cd.allow_group_re_call = true;
    cd.quest_unit_open.as_mut().unwrap()[0] = 1;
    cd.quest_unit.as_mut().unwrap()[0] = 2;
    cd.quest_flag.as_mut().unwrap()[0] = 3;
    cd.marry_count = 4;
    cd.hum_items.as_mut().unwrap()[0] = Some(item());
    cd.hum_items.as_mut().unwrap()[1] = Some(ServerUserItem::csharp_new());
    cd.bag_items.as_mut().unwrap()[3] = Some(item());
    cd.storage_items.as_mut().unwrap()[5] = Some(item());
    cd.magic.as_mut().unwrap()[0] = Some(magic());
    cd
}

fn full_cdi() -> CharacterDataInfo {
    CharacterDataInfo {
        header: header(),
        data: Some(full_character_data()),
    }
}

/// 逐条向量：`to_bytes` 字节级相同 + `from_bytes` 读回同一个值。
#[test]
fn playerdatainfo_vectors_byte_exact() {
    // 每条 make + 判据（比对字节 + 往返）。
    fn check<T: MemPack + PartialEq + std::fmt::Debug>(name: &str, v: &T) {
        let want = vector(name);
        let got = v.to_bytes();
        assert_eq!(hex(&got), hex(&want), "vector {name} bytes differ");
        let back = T::from_bytes(&want).unwrap_or_else(|e| panic!("{name} decode: {e}"));
        assert_eq!(&back, v, "vector {name} round-trip value differ");
    }

    check("ability", &ability());
    check("nakedability", &naked());
    check("magicrcd", &magic());
    check("serveruseritem", &item());
    check("recordheader", &header());
    check("characterdata_full", &full_character_data());
    check("characterdatainfo_empty", &CharacterDataInfo::csharp_new());
    check("characterdatainfo_full", &full_cdi());
    check(
        "savecharacterdata",
        &SaveCharacterData {
            account: Some("acct".into()),
            chr_name: Some("aaa".into()),
            character_data: Some(full_cdi()),
        },
    );
    check(
        "loadcharacterdata",
        &LoadCharacterData {
            account: Some("acct".into()),
            chr_name: Some("aaa".into()),
            user_addr: Some("127.0.0.1".into()),
            session_id: 42,
        },
    );
    check(
        "loadplayerdatapacket",
        &LoadPlayerDataPacket {
            chr_name: Some("aaa".into()),
            hum_data_info: Some(full_cdi()),
        },
    );
    check(
        "serverrequestdata",
        &ServerRequestData {
            query_id: 5,
            message: Some(vec![1, 2, 3]),
            packet: Some(vec![4, 5]),
            sign: Some(vec![6]),
        },
    );
    check(
        "serverrequestdata_null",
        &ServerRequestData {
            query_id: -1,
            ..ServerRequestData::default()
        },
    );
    check(
        "serverrequestmessage",
        &ServerRequestMessage::new(0x1234, 5, 6, 7, 8),
    );
}

/// 规模锚点：向量集缩水（文件被截断/生成器漏产）也要能发现 —— 与 `crates/protocol` 的
/// `oracle_vectors_byte_exact` 同口径（这里是 `playerdatainfo` 的业主）。
#[test]
fn playerdatainfo_vector_scale_anchor() {
    let text = std::fs::read_to_string(vectors_path()).expect("read oracle_vectors.jsonl");
    let n = text
        .lines()
        .filter(|l| l.contains("\"kind\":\"playerdatainfo\""))
        .count();
    assert_eq!(n, 14, "playerdatainfo 向量应为 14 条（实测 n={n}）");
}

/// 破坏者自检：改一个字段（`CharacterData.gold`）必须让密集结构的字节变化。
#[test]
fn mutation_changes_bytes() {
    let a = full_character_data();
    let mut b = a.clone();
    b.gold += 1;
    assert_ne!(a.to_bytes(), b.to_bytes());
    // 未改的字段改回后必须逐字节相同
    b.gold = a.gold;
    assert_eq!(a.to_bytes(), b.to_bytes());
}
