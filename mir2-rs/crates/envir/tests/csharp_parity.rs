#![recursion_limit = "256"]
//! C# oracle 对拍：`crates/envir` 的 MapInfo Flag/MapDesc 与配置层输入 vs **C# 探针实测值**。
//!
//! 探针：`tests/parity/csharp/EnvirGolden`（用**真实** `StringList.cs`/`ConfigFile.cs`/`MapInfoFlag.cs`/
//! `ClientConf.cs`/`NakedAbility.cs` 链接件 + **逐字拷贝**的 `Maps.cs` 解析循环与 HUtil32 函数）。
//! 生成命令与原始输出见 `tests/parity/envir-2026-10-11/`（含输入 hash）。
//!
//! 仓内用例（synthetic 夹具 + C# 输出）**常驻门禁**；真机语料用例 `#[ignore]`（读仓外 `E:\MirServer`）。

use std::path::{Path, PathBuf};

use mir2_envir::{
    decode_text, parse_map_info_lines, ClientConf, GameSvrConf, MapInfoEntry, MapInfoFlag,
};
use serde_json::{json, Value};

fn parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/envir-2026-10-11")
}

/// 与 C# 探针 `Program.FlagJson` 一一对应的 JSON 视图（键、值域都相同）。
fn flag_json(f: &MapInfoFlag) -> Value {
    json!({
        "safe_area": f.safe_area,
        "request_level": f.request_level,
        "need_seton_flag": f.need_seton_flag,
        "need_on_off": f.need_on_off,
        "music": f.music,
        "music_id": f.music_id,
        "bo_darkness": f.bo_darkness,
        "day_light": f.day_light,
        "fight_zone": f.fight_zone,
        "fight3_zone": f.fight3_zone,
        "bo_quiz": f.bo_quiz,
        "bo_noreconnect": f.bo_noreconnect,
        // C# 侧该字段默认 null、赋值后是 string；空串按 null 归一（二者对下游 `string.IsNullOrEmpty` 等价）。
        "s_no_reconnect_map": if f.s_no_reconnect_map.is_empty() { Value::Null } else { json!(f.s_no_reconnect_map) },
        "bo_exprate": f.bo_exprate,
        "exp_rate": f.exp_rate,
        "bo_pkwinlevel": f.bo_pkwinlevel,
        "n_pkwinlevel": f.n_pkwinlevel,
        "bo_pkwinexp": f.bo_pkwinexp,
        "n_pkwinexp": f.n_pkwinexp,
        "bo_pklostlevel": f.bo_pklostlevel,
        "n_pklostlevel": f.n_pklostlevel,
        "bo_pklostexp": f.bo_pklostexp,
        "n_pklostexp": f.n_pklostexp,
        "bo_dechp": f.bo_dechp,
        "n_dechppoint": f.n_dechppoint,
        "n_dechptime": f.n_dechptime,
        "bo_inchp": f.bo_inchp,
        "n_inchppoint": f.n_inchppoint,
        "n_inchptime": f.n_inchptime,
        "bo_decgamegold": f.bo_decgamegold,
        "n_decgamegold": f.n_decgamegold,
        "n_decgamegoldtime": f.n_decgamegoldtime,
        "bo_decgamepoint": f.bo_decgamepoint,
        "n_decgamepoint": f.n_decgamepoint,
        "n_decgamepointtime": f.n_decgamepointtime,
        "bo_incgamegold": f.bo_incgamegold,
        "n_incgamegold": f.n_incgamegold,
        "n_incgamegoldtime": f.n_incgamegoldtime,
        "bo_incgamepoint": f.bo_incgamepoint,
        "n_incgamepoint": f.n_incgamepoint,
        "n_incgamepointtime": f.n_incgamepointtime,
        "run_human": f.run_human,
        "run_mon": f.run_mon,
        "bo_needhole": f.bo_needhole,
        "no_re_call": f.no_re_call,
        "no_guild_re_call": f.no_guild_re_call,
        "bo_nodearrecall": f.bo_nodearrecall,
        "master_re_call": f.master_re_call,
        "bo_norandommove": f.bo_norandommove,
        "bo_nodrug": f.bo_nodrug,
        "mine": f.mine,
        "bo_mine2": f.bo_mine2,
        "bo_nopositionmove": f.bo_nopositionmove,
        "no_drop_item": f.no_drop_item,
        "no_throw_item": f.no_throw_item,
        "no_horse": f.no_horse,
        "bo_nochat": f.bo_nochat,
        "bo_killfunc": f.bo_killfunc,
        "n_killfuncno": f.n_killfuncno,
        "bo_nohumnomon": f.bo_nohumnomon,
    })
}

fn section_json(e: &MapInfoEntry) -> Value {
    json!({
        "map_name": e.map_name,
        "map_file_name": e.map_file_name,
        "map_desc": e.desc,
        "server_index": e.server_index,
        "flag": flag_json(&e.flag),
    })
}

/// 读 GBK/UTF-8 文本 → 逐行（空行丢掉，与 `MapLoader` 的 `split_lines` 同口径）→ 解析 section。
fn parse_file(path: &Path) -> Vec<MapInfoEntry> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("读 {} 失败: {e}", path.display()));
    let text = decode_text(&bytes);
    let lines: Vec<String> = text
        .split(['\n', '\r'])
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    parse_map_info_lines(&lines)
}

fn compare_mapinfo(fixture: &Path, csharp_json: &Path) {
    let rust: Vec<Value> = parse_file(fixture).iter().map(section_json).collect();
    let text = std::fs::read_to_string(csharp_json).expect("读 C# 探针输出");
    let cs: Value = serde_json::from_str(&text).expect("解析 C# 探针 JSON");
    let cs_sections = cs["sections"].as_array().expect("sections 数组").clone();

    assert_eq!(
        rust.len(),
        cs_sections.len(),
        "section 条数 Rust={} C#={}",
        rust.len(),
        cs_sections.len()
    );
    for (i, (r, c)) in rust.iter().zip(cs_sections.iter()).enumerate() {
        // C# 的 `line` 只用于定位；逐字段比其余部分。
        let c_wo_line = {
            let mut v = c.clone();
            v.as_object_mut().unwrap().remove("line");
            v
        };
        assert_eq!(
            *r, c_wo_line,
            "第 {i} 个 section 与 C# 探针不等\nRust={r}\nC#  ={c_wo_line}"
        );
    }
}

fn compare_conf(conf_file: &Path, csharp_json: &Path) {
    let conf = GameSvrConf::load(conf_file)
        .unwrap_or_else(|e| panic!("读 {} 失败: {e}", conf_file.display()));
    let text = std::fs::read_to_string(csharp_json).expect("读 C# 探针输出");
    let cs: Value = serde_json::from_str(&text).expect("解析 C# 探针 JSON");
    assert_eq!(conf.game_gold_name, cs["game_gold_name"].as_str().unwrap());
    assert_eq!(
        conf.game_point_name,
        cs["game_point_name"].as_str().unwrap()
    );
    assert_eq!(conf.client_file1, cs["client_file1"].as_str().unwrap());
    assert_eq!(conf.client_file2, cs["client_file2"].as_str().unwrap());
    assert_eq!(conf.client_file3, cs["client_file3"].as_str().unwrap());
    assert_eq!(
        i64::from(conf.client_file1_crc),
        cs["client_file1_crc"].as_i64().unwrap()
    );
    assert_eq!(
        i64::from(conf.client_file2_crc),
        cs["client_file2_crc"].as_i64().unwrap()
    );
    assert_eq!(
        i64::from(conf.client_file3_crc),
        cs["client_file3_crc"].as_i64().unwrap()
    );
    for (key, a) in [
        ("warr", &conf.bonus_abil_warr),
        ("wizard", &conf.bonus_abil_wizard),
        ("taos", &conf.bonus_abil_taos),
    ] {
        let c = &cs["bonus_abil"][key];
        assert_eq!(
            a.dc,
            c["dc"].as_u64().unwrap() as u16,
            "bonus_abil.{key}.dc"
        );
        assert_eq!(
            a.mc,
            c["mc"].as_u64().unwrap() as u16,
            "bonus_abil.{key}.mc"
        );
        assert_eq!(
            a.sc,
            c["sc"].as_u64().unwrap() as u16,
            "bonus_abil.{key}.sc"
        );
        assert_eq!(
            a.ac,
            c["ac"].as_u64().unwrap() as u16,
            "bonus_abil.{key}.ac"
        );
        assert_eq!(
            a.mac,
            c["mac"].as_u64().unwrap() as u16,
            "bonus_abil.{key}.mac"
        );
        assert_eq!(
            a.hp,
            c["hp"].as_u64().unwrap() as u16,
            "bonus_abil.{key}.hp"
        );
        assert_eq!(
            a.mp,
            c["mp"].as_u64().unwrap() as u16,
            "bonus_abil.{key}.mp"
        );
        assert_eq!(
            a.hit,
            c["hit"].as_u64().unwrap() as u8,
            "bonus_abil.{key}.hit"
        );
        assert_eq!(
            a.speed,
            c["speed"].as_i64().unwrap() as i32,
            "bonus_abil.{key}.speed"
        );
        assert_eq!(
            a.reserved,
            c["reserved"].as_u64().unwrap() as u8,
            "bonus_abil.{key}.reserved"
        );
    }
    for (key, a) in [
        ("warr", &conf.naked_abil_warr),
        ("wizard", &conf.naked_abil_wizard),
        ("taos", &conf.naked_abil_taos),
    ] {
        let c = &cs["naked_abil"][key];
        assert_eq!(
            a.dc,
            c["dc"].as_u64().unwrap() as u16,
            "naked_abil.{key}.dc"
        );
        assert_eq!(
            a.mc,
            c["mc"].as_u64().unwrap() as u16,
            "naked_abil.{key}.mc"
        );
        assert_eq!(
            a.sc,
            c["sc"].as_u64().unwrap() as u16,
            "naked_abil.{key}.sc"
        );
        assert_eq!(
            a.ac,
            c["ac"].as_u64().unwrap() as u16,
            "naked_abil.{key}.ac"
        );
        assert_eq!(
            a.mac,
            c["mac"].as_u64().unwrap() as u16,
            "naked_abil.{key}.mac"
        );
        assert_eq!(
            a.hp,
            c["hp"].as_u64().unwrap() as u16,
            "naked_abil.{key}.hp"
        );
        assert_eq!(
            a.mp,
            c["mp"].as_u64().unwrap() as u16,
            "naked_abil.{key}.mp"
        );
        assert_eq!(
            a.hit,
            c["hit"].as_u64().unwrap() as u8,
            "naked_abil.{key}.hit"
        );
        assert_eq!(
            a.speed,
            c["speed"].as_i64().unwrap() as i32,
            "naked_abil.{key}.speed"
        );
        assert_eq!(
            a.reserved,
            c["reserved"].as_u64().unwrap() as u8,
            "naked_abil.{key}.reserved"
        );
    }
}

/// 常驻门禁：仓内 synthetic MapInfo 夹具 vs C# 探针（覆盖 `Maps.cs:82-320` 全部命令 + 两个 oracle 既有形态 + 跨节状态）。
#[test]
fn synthetic_mapinfo_matches_csharp_probe() {
    let dir = parity_dir();
    compare_mapinfo(
        &dir.join("MapInfo-synth.txt"),
        &dir.join("mapinfo-synth-cs.json"),
    );
}

/// 常驻门禁：仓内 synthetic Server.conf 夹具 vs C# 探针。
#[test]
fn synthetic_server_conf_matches_csharp_probe() {
    let dir = parity_dir();
    compare_conf(
        &dir.join("Server.conf-synth"),
        &dir.join("serverconf-synth-cs.json"),
    );
}

/// 常驻门禁：`ClientConf` 序列化原文（11029 的输入）——17 字节，与 C# 探针 `clientconf` 一致。
#[test]
fn client_conf_bytes_match_csharp_probe() {
    let cs: Value = serde_json::from_str(
        &std::fs::read_to_string(parity_dir().join("clientconf-cs.json")).unwrap(),
    )
    .unwrap();
    let expect: Vec<u8> = cs["client_conf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect();
    assert_eq!(ClientConf::game_svr_default().to_bytes(), expect);
}

/// 真机语料：读仓外 `E:\MirServer\M2GameSvr\Envir\MapInfo.txt`（322 section），与已存档的 C# 探针输出逐项相等。
///
/// ```text
/// cargo test -p mir2-envir --test csharp_parity --offline -- --ignored --nocapture
/// ```
#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_REAL_MAPINFO 可覆盖路径"]
fn real_mapinfo_matches_csharp_probe() {
    let path = std::env::var("MIR2_REAL_MAPINFO").map_or_else(
        |_| PathBuf::from(r"E:\MirServer\M2GameSvr\Envir\MapInfo.txt"),
        PathBuf::from,
    );
    let rust = parse_file(&path);
    assert_eq!(rust.len(), 322, "现网 MapInfo.txt 应有 322 条 section");
    // 帧 54/46/766 的锚点：`[0\t比奇省] DAY`
    let map0 = rust.iter().find(|e| e.map_name == "0").expect("地图 0");
    assert_eq!(map0.desc, "比奇省");
    assert!(map0.flag.day_light);
    assert!(!map0.flag.music);
    assert_eq!(map0.flag.music_id, -1, "非 MUSIC 图 nMusicid = -1");
    assert!(!map0.flag.fight_zone);
    assert!(!map0.flag.safe_area);
    compare_mapinfo(&path, &parity_dir().join("mapinfo-real-cs.json"));
}

/// 真机语料：`Server.conf`（UTF-8+BOM）读取点 vs C# 探针。
#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_REAL_SERVERCONF 可覆盖路径"]
fn real_server_conf_matches_csharp_probe() {
    let path = std::env::var("MIR2_REAL_SERVERCONF").map_or_else(
        |_| PathBuf::from(r"E:\MirServer\M2GameSvr\Server.conf"),
        PathBuf::from,
    );
    let conf = GameSvrConf::load(&path).expect("读 Server.conf");
    assert_eq!(conf.game_gold_name, "元宝");
    assert_eq!(conf.game_point_name, "成长");
    compare_conf(&path, &parity_dir().join("serverconf-real-cs.json"));
}
