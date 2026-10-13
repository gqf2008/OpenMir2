//! M3⑧-3 判据③：把 oracle 真身探针产出的行会文件向量**逐字段 / 逐字节**核对进 Rust。
//!
//! 向量来源：`mir2-rs/tests/parity/m3/guild-store-probe`（C#，直调 `M2Server.Guild` 的
//! `GuildManager` / `GuildInfo` / `GuildConf`，在临时目录里实跑）
//!   → `guild-store-probe/vectors/guild_store_vectors.tsv`（group/key/value）
//!   → `guild-store-probe/vectors/guild_file_bytes.tsv`（scenario/sha256/hex）
//!
//! 被测实现：`mir2_data::guild`（`crates/data/src/guild.rs`）。
//!
//! 覆盖（盘点 §1.A + 探针实测的 oracle quirk）：
//! ① `GuildList.txt` 读取/回写与 `ServerIndex != 0` 的 no-op；
//! ② `<名>.txt` 四段解析（`+` / `#` / `;` / 空行 / `,`+空格分词）与**逐字节**序列化；
//! ③ 保存目标不存在 ⇒ 报错且**不新建文件**（oracle `StringList.SaveToFile` 行为）；
//! ④ `<名>.ini` 只建空文件、五键读侧、**从不落盘**；
//! ⑤ 行会战 `WarTime` 加载恒 0、联盟段丢弃、职名 `>30` 才截断、缺 `<名>.txt` ⇒ 移除并回写名单。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use mir2_data::guild::{
    get_valid_str3, parse_guild_file, serialize_guild_file, GuildConfValues, GuildFileBody,
    GuildFileSections, GuildRankRecord, GuildRecord, GuildStore, GuildStoreError, SaveStatus,
    WarGuildRecord, GUILD_SPLIT,
};
use mir2_data::guild_castle::GuildCastleConfig;

fn probe_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/m3/guild-store-probe")
}

fn read_vectors() -> BTreeMap<(String, String), String> {
    let text = fs::read_to_string(probe_dir().join("vectors/guild_store_vectors.tsv")).unwrap();
    let mut out = BTreeMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut it = line.splitn(3, '\t');
        let group = it.next().unwrap().to_string();
        let key = it.next().unwrap().to_string();
        let value = it.next().unwrap_or("").to_string();
        assert!(
            out.insert((group.clone(), key.clone()), value).is_none(),
            "向量重复: {group}/{key}"
        );
    }
    out
}

fn read_bytes_vectors() -> BTreeMap<String, String> {
    let text = fs::read_to_string(probe_dir().join("vectors/guild_file_bytes.tsv")).unwrap();
    let mut out = BTreeMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut it = line.splitn(3, '\t');
        let scenario = it.next().unwrap().to_string();
        let _sha = it.next().unwrap();
        let hex = it.next().unwrap().to_string();
        out.insert(scenario, hex);
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn gbk(s: &str) -> Vec<u8> {
    let (bytes, _, _) = encoding_rs::GBK.encode(s);
    bytes.into_owned()
}

fn tmp_root(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("m3-guild-store-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("GuildBase/Guilds")).unwrap();
    base
}

fn store_for(root: &Path) -> GuildStore {
    GuildStore::new(
        root.join("GuildBase/Guilds"),
        root.join("GuildBase/GuildList.txt"),
    )
}

/// 盘点 §1.K / 探针实测的段头与配置默认值。
#[test]
fn vector_config_sections_and_limits() {
    let v = read_vectors();
    let sections = GuildFileSections::oracle_defaults();
    assert_eq!(
        sections.notice,
        v[&("config".into(), "guild_notice".into())]
    );
    assert_eq!(
        sections.war,
        v[&("config".into(), "guild_war_header".into())]
    );
    assert_eq!(sections.all, v[&("config".into(), "guild_all".into())]);
    assert_eq!(
        sections.member,
        v[&("config".into(), "guild_member".into())]
    );
    assert_eq!(
        mir2_data::guild::ORACLE_GUILD_CHIEF,
        v[&("config".into(), "guild_chief".into())]
    );

    // 段头同源于 ⑧-2 的 GuildCastleConfig（`from_config` 必须给同样的四串）。
    let cfg = GuildCastleConfig::default();
    assert_eq!(GuildFileSections::from_config(&cfg), sections);
    assert_eq!(
        cfg.guild_rank_name_len.to_string(),
        v[&("config".into(), "guild_rank_name_len".into())]
    );
    assert_eq!(
        cfg.guild_member_max_limit.to_string(),
        v[&("config".into(), "guild_member_max_limit".into())]
    );
}

/// `HUtil32.GetValidStr3` 分词口径（oracle 真身 10 例）。
#[test]
fn vector_get_valid_str3_tokenizer() {
    let v = read_vectors();
    for i in 0..10 {
        let key = |suffix: &str| format!("case_{i}_{suffix}");
        let unmask = |s: &str| s.replace("<SP>", " ");
        let mask = |s: &str| s.replace(' ', "<SP>");
        let input = unmask(&v[&("H_get_valid_str3".into(), key("in"))]);
        let expect_dest = v[&("H_get_valid_str3".into(), key("dest"))].clone();
        let expect_rest = v[&("H_get_valid_str3".into(), key("rest"))].clone();
        let mut dest = String::new();
        let rest = get_valid_str3(&input, &mut dest, &GUILD_SPLIT);
        assert_eq!(mask(&dest), expect_dest, "case {i} dest（输入 {input:?}）");
        assert_eq!(mask(&rest), expect_rest, "case {i} rest（输入 {input:?}）");
    }
}

/// ④ `.ini`：不存在时建**空**文件、五键初值 0、保存 `<名>.txt` 不改 `.ini`。
#[test]
fn vector_ini_is_only_ever_created_empty() {
    let v = read_vectors();
    let sections = GuildFileSections::oracle_defaults();
    let root = tmp_root("ini");
    let store = store_for(&root);

    let conf = store.load_guild_conf("甲会").unwrap();
    assert_eq!(conf, GuildConfValues::default());
    assert_eq!(
        v[&(
            "A_save_missing_target".into(),
            "ini_exists_after_ctor".into()
        )],
        "True"
    );
    assert_eq!(v[&("A_save_missing_target".into(), "ini_size".into())], "0");
    assert_eq!(
        fs::metadata(store.guild_conf_path("甲会")).unwrap().len(),
        0
    );

    // 预建空 `.txt` → 落盘（与探针 B 同体）。
    fs::write(store.guild_txt_path("甲会"), b"").unwrap();
    let record = GuildRecord {
        name: "甲会".into(),
        conf,
        body: sample_body(),
    };
    assert_eq!(
        store.save_guild(&record, &sections, 0, 0).unwrap(),
        SaveStatus::Written
    );
    assert_eq!(
        fs::metadata(store.guild_conf_path("甲会")).unwrap().len(),
        0
    );
}

/// A（不落盘规则）：保存目标不存在 ⇒ `MissingSaveTarget` 且**不新建文件**。
#[test]
fn vector_save_refuses_to_create_target() {
    let v = read_vectors();
    let sections = GuildFileSections::oracle_defaults();
    let root = tmp_root("save");
    let store = store_for(&root);
    let conf = store.load_guild_conf("甲会").unwrap();
    let record = GuildRecord {
        name: "甲会".into(),
        conf,
        body: GuildFileBody::default(),
    };
    let err = store.save_guild(&record, &sections, 0, 0).unwrap_err();
    assert!(
        matches!(err, GuildStoreError::MissingSaveTarget(_)),
        "{err:?}"
    );
    assert!(!store.guild_txt_path("甲会").exists());
    assert_eq!(
        v[&(
            "A_save_missing_target".into(),
            "txt_exists_after_save".into()
        )],
        "False"
    );

    // `GuildManager.SaveGuildList` 同规则。
    let names = vec!["甲会".to_string()];
    let err = store.save_guild_list(&names, 0).unwrap_err();
    assert!(
        matches!(err, GuildStoreError::MissingSaveTarget(_)),
        "{err:?}"
    );
    assert!(!store.guild_file().exists());
}

fn sample_body() -> GuildFileBody {
    GuildFileBody {
        notices: vec!["公告一".into(), "公告二".into()],
        // oracle 侧 WarTick=now-100000 / WarTime=0 ⇒ n14 <= 0 被跳过（不写这一行）。
        wars: vec![WarGuildRecord {
            guild_name: "乙会".into(),
            war_tick: 0,
            war_time: 0,
        }],
        allies: Vec::new(),
        ranks: vec![GuildRankRecord {
            rank_no: 99,
            rank_name: "成员".into(),
            members: vec!["乙".into(), "丙".into()],
        }],
    }
}

/// B（逐字节）：`SaveGuildFile` 的 GBK + CRLF + `" "` 分隔行口径。
#[test]
fn vector_save_bytes_exact() {
    let v = read_vectors();
    let w = read_bytes_vectors();
    let sections = GuildFileSections::oracle_defaults();
    let body = sample_body();
    let bytes = serialize_guild_file(&body, &sections, 0);
    assert_eq!(hex(&bytes), w["B_save_precreated_txt"]);
    assert_eq!(
        bytes.len().to_string(),
        v[&("B_save_precreated_txt".into(), "len".into())]
    );

    // 已到期的行会战必须整行不出现。
    assert!(!String::from_utf8_lossy(&bytes).contains("乙会"));
}

/// C（解析）：四段 + 三个 quirk + `.ini` 五键读侧；缺 `<名>.txt` ⇒ `MissingGuildFile` 但 `.ini` 已建。
#[test]
fn vector_load_quattro_sections_and_quirks() {
    let root = tmp_root("load");
    let store = store_for(&root);
    let sections = GuildFileSections::oracle_defaults();

    fs::write(
        store.guild_conf_path("加载会"),
        gbk("[Guild]\nGuildName=加载会\nBuildPoint=42\nAurae=7\nStability=3\nFlourishing=9\nChiefItemCount=5\n"),
    )
    .unwrap();
    fs::write(
        store.guild_txt_path("加载会"),
        gbk(&format!(
            "; 分号注释行\n{}\n+第一行公告\n+第二行公告\n \n{}\n+乙会 123\n{}\n+丙会\n{}\n#1 掌门人\n+甲会长\n#99 成员\n+乙,丙\n",
            sections.notice, sections.war, sections.all, sections.member
        )),
    )
    .unwrap();

    let rec = store
        .load_guild("加载会", &sections, 16, 777, |n| n == "乙会")
        .unwrap();
    assert_eq!(
        rec.conf,
        GuildConfValues {
            build_point: 42,
            aurae: 7,
            stability: 3,
            flourishing: 9,
            chief_item_count: 5,
        }
    );
    assert_eq!(rec.body.notices, vec!["第一行公告", "第二行公告"]);
    assert_eq!(rec.body.wars.len(), 1);
    assert_eq!(rec.body.wars[0].guild_name, "乙会");
    assert_eq!(rec.body.wars[0].war_time, 0, "oracle 读残留 s20 ⇒ 恒 0");
    assert_eq!(rec.body.wars[0].war_tick, 777);
    assert!(rec.body.allies.is_empty(), "联盟段被注释丢弃");
    assert_eq!(rec.body.ranks.len(), 2);
    assert_eq!(rec.body.ranks[0].rank_no, 1);
    assert_eq!(rec.body.ranks[0].rank_name, "掌门人");
    assert_eq!(rec.body.ranks[0].members, vec!["甲会长"]);
    assert_eq!(rec.body.ranks[1].rank_no, 99);
    assert_eq!(
        rec.body.ranks[1].members,
        vec!["乙", "丙"],
        "`,` 与空格都是分词符"
    );

    let err = store
        .load_guild("无文件会", &sections, 16, 0, |_| false)
        .unwrap_err();
    assert!(
        matches!(err, GuildStoreError::MissingGuildFile(_)),
        "{err:?}"
    );
    assert!(store.guild_conf_path("无文件会").exists());
    assert_eq!(
        fs::metadata(store.guild_conf_path("无文件会"))
            .unwrap()
            .len(),
        0
    );
}

/// 解析出的四段再序列化应稳定（`allies` 恒空 ⇒ 联盟段内容被丢弃后不再回来）。
#[test]
fn vector_parse_save_round_trip_drops_allies() {
    let sections = GuildFileSections::oracle_defaults();
    let src = format!(
        "{}\n+公告甲\n{}\n+乙会 1\n{}\n+丙会\n{}\n#3 封号\n+某甲\n",
        sections.notice, sections.war, sections.all, sections.member
    );
    let body = parse_guild_file(src.as_bytes(), &sections, 16, 5, |n| n == "乙会");
    let once = serialize_guild_file(&body, &sections, 5);
    let body2 = parse_guild_file(&once, &sections, 16, 5, |n| n == "乙会");
    let twice = serialize_guild_file(&body2, &sections, 5);
    assert_eq!(once, twice);
    assert!(!String::from_utf8_lossy(&once).contains("丙会"));
}

/// G（职名截断）：`Length > 30` 才截到 `GuildRankNameLen`。
#[test]
fn vector_rank_name_truncates_only_over_30() {
    let root = tmp_root("trunc");
    let store = store_for(&root);
    let sections = GuildFileSections::oracle_defaults();
    let long = "甲".repeat(31);
    let short = "乙".repeat(30);
    fs::write(
        store.guild_txt_path("截断会"),
        gbk(&format!(
            "{}\n#7 {long}\n+某甲\n#8 {short}\n+某乙\n",
            sections.member
        )),
    )
    .unwrap();
    let rec = store
        .load_guild("截断会", &sections, 16, 0, |_| false)
        .unwrap();
    assert_eq!(rec.body.ranks.len(), 2);
    assert_eq!(rec.body.ranks[0].rank_name.chars().count(), 16);
    assert_eq!(rec.body.ranks[1].rank_name.chars().count(), 30);
}

/// D（`LoadGuildInfo`）：逐行读名、加载失败者逆序移除并**回写名单**（逐字节）；`.ini` 副作用保留。
#[test]
fn vector_load_guild_info_rewrites_list() {
    let v = read_vectors();
    let w = read_bytes_vectors();
    let root = tmp_root("loadinfo");
    let store = store_for(&root);
    let sections = GuildFileSections::oracle_defaults();
    fs::write(store.guild_file(), gbk("甲会\n\n   \n加载会\n幽灵会\n")).unwrap();
    fs::write(
        store.guild_txt_path("甲会"),
        gbk(&format!("{}\n", sections.notice)),
    )
    .unwrap();
    fs::write(
        store.guild_txt_path("加载会"),
        gbk(&format!(
            "{}\n+公告\n{}\n#1 掌门人\n+会长\n",
            sections.notice, sections.member
        )),
    )
    .unwrap();

    let load = store.load_guild_info(&sections, 16, 777, 0).unwrap();
    assert_eq!(load.names, vec!["甲会", "加载会"]);
    assert_eq!(load.removed, vec!["幽灵会"]);
    assert_eq!(load.guilds.len(), 2);
    assert_eq!(load.guilds[0].name, "甲会");
    assert_eq!(load.guilds[1].name, "加载会");
    assert_eq!(v[&("D_load_guild_info".into(), "find_甲会".into())], "True");
    assert_eq!(
        v[&("D_load_guild_info".into(), "find_幽灵会".into())],
        "False"
    );
    assert_eq!(
        hex(&fs::read(store.guild_file()).unwrap()),
        w["D_load_guild_info_list_rewritten"]
    );
    // oracle ctor 为名单里每个会名都建 `.ini`（含随后被移除的 `幽灵会`）。
    assert_eq!(
        v[&("D_load_guild_info".into(), "ini_幽灵会_exists".into())],
        "True"
    );
    assert!(store.guild_conf_path("幽灵会").exists());
    assert_eq!(
        fs::metadata(store.guild_conf_path("幽灵会")).unwrap().len(),
        0
    );
}

/// E（非 0 区）：`SaveGuildList` 直接 return ⇒ 名单文件逐字节不动。
#[test]
fn vector_save_guild_list_noop_on_nonzero_server_index() {
    let w = read_bytes_vectors();
    let root = tmp_root("serveridx");
    let store = store_for(&root);
    let sections = GuildFileSections::oracle_defaults();
    fs::write(store.guild_file(), gbk("甲会\n幽灵会\n")).unwrap();
    fs::write(
        store.guild_txt_path("甲会"),
        gbk(&format!("{}\n", sections.notice)),
    )
    .unwrap();

    let load = store.load_guild_info(&sections, 16, 0, 1).unwrap();
    assert_eq!(load.names, vec!["甲会"]);
    assert_eq!(load.removed, vec!["幽灵会"]);
    assert_eq!(
        hex(&fs::read(store.guild_file()).unwrap()),
        w["E_server_index_nonzero_list_unchanged"]
    );

    let names = vec!["甲会".to_string()];
    assert_eq!(
        store.save_guild_list(&names, 3).unwrap(),
        SaveStatus::SkippedNonZeroServerIndex
    );
}

/// F（fail-closed）：`GuildFile` 不存在 ⇒ 报错、不建文件、无异味副作用。
#[test]
fn vector_missing_list_file_fails_closed() {
    let v = read_vectors();
    let root = tmp_root("nofile");
    let store = store_for(&root);
    let sections = GuildFileSections::oracle_defaults();
    let err = store.load_guild_list().unwrap_err();
    assert!(
        matches!(err, GuildStoreError::MissingListFile(_)),
        "{err:?}"
    );
    let err = store.load_guild_info(&sections, 16, 0, 0).unwrap_err();
    assert!(
        matches!(err, GuildStoreError::MissingListFile(_)),
        "{err:?}"
    );
    assert!(!store.guild_file().exists());
    assert_eq!(
        v[&(
            "F_missing_list_file".into(),
            "list_file_still_absent".into()
        )],
        "True"
    );
}
