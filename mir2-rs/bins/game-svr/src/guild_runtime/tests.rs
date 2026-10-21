//! G3 判据：W1/W2/W3/W4 + 生产锚点 A1-A5（v9 §6）。
//!
//! 变异日志（每条「改坏 ⇒ 指定断言红 ⇒ 逐字节还原」）记录在交付报告；本文件的断言只调
//! **生产接缝**（`guild_runtime::{startup,step,on_disconnect}`、`guild_cmd::handle_guild_cmd`、
//! `crate::{closed_event_is_current,prepare_closed_guild}`）与影子目录读文件字节。

use super::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use mir2_data::guild::{GuildFileSections, GuildStore, GuildStoreError};
use mir2_protocol::messages;
use mir2_world::{Entity, GuildConfig, GuildManager, World};

use crate::guild_cmd::{self, GuildSession, GuildSessions};
use crate::tests::test_session_ground;

// ---------------------------------------------------------------------------
// 影子目录 / GBK
// ---------------------------------------------------------------------------

static SEQ: AtomicUsize = AtomicUsize::new(0);

struct Shadow {
    root: PathBuf,
}

impl Shadow {
    fn new(tag: &str) -> Self {
        let n = SEQ.fetch_add(1, AtomicOrdering::Relaxed);
        let root = std::env::temp_dir().join(format!("mir2-g3-{}-{n}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Guilds")).unwrap();
        Self { root }
    }

    fn store(&self) -> GuildStore {
        GuildStore::new(self.root.join("Guilds"), self.root.join("GuildList.txt"))
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, gbk(text)).unwrap();
    }

    fn read(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.root.join(rel)).unwrap()
    }

    fn read_text(&self, rel: &str) -> String {
        let bytes = self.read(rel);
        let (s, _, _) = encoding_rs::GBK.decode(&bytes);
        s.into_owned()
    }

    fn exists(&self, rel: &str) -> bool {
        self.root.join(rel).exists()
    }

    fn is_empty(&self, rel: &str) -> bool {
        std::fs::metadata(self.root.join(rel))
            .map(|m| m.len() == 0)
            .unwrap_or(false)
    }
}

impl Drop for Shadow {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn gbk(s: &str) -> Vec<u8> {
    let (b, _, _) = encoding_rs::GBK.encode(s);
    b.to_vec()
}

fn sections() -> GuildFileSections {
    GuildFileSections::from_strings("公告X", "敌对X", "联盟X", "成员段X")
}

fn msg_cfg(show_prefix: bool) -> GuildMsgCfg {
    GuildMsgCfg {
        show_pre_fix_msg: show_prefix,
        prefix: "〖行会X〗".to_string(),
        f_color: 0x0A,
        b_color: 0x0B,
    }
}

fn cfg() -> GuildConfig {
    GuildConfig {
        guild_chief: "掌门X".to_string(),
        guild_member_rank: "职位X".to_string(),
        ..GuildConfig::default()
    }
}

fn parse_104(body: &[u8]) -> (i32, u16, u16, u16, String) {
    let recog = i32::from_le_bytes([body[0], body[1], body[2], body[3]]);
    let ident = u16::from_le_bytes([body[4], body[5]]);
    let param = u16::from_le_bytes([body[6], body[7]]);
    let series = u16::from_le_bytes([body[10], body[11]]);
    let raw = mir2_protocol::edcode::decode(&body[12..]);
    let text = encoding_rs::GBK.decode(&raw).0.into_owned();
    (recog, ident, param, series, text)
}

// ---------------------------------------------------------------------------
// W1 启动加载
// ---------------------------------------------------------------------------

fn w1_shadow() -> Shadow {
    let sh = Shadow::new("w1");
    sh.write("GuildList.txt", "B\r\nA\r\nC\r\n");
    sh.write(
        "Guilds/B.txt",
        "公告X\r\n+N-B\r\n \r\n敌对X\r\n+B\r\n \r\n联盟X\r\n \r\n成员段X\r\n#1 掌门X\r\n+chiefB\r\n",
    );
    sh.write(
        "Guilds/A.txt",
        "公告X\r\n+N-A\r\n \r\n敌对X\r\n+C\r\n \r\n联盟X\r\n \r\n成员段X\r\n#1 掌门X\r\n+chiefA\r\n",
    );
    sh
}

#[test]
fn g3_w1_startup_loads_in_list_order_and_rewrites_list() {
    let sh = w1_shadow();
    let store = sh.store();
    let m = startup(Some(&store), &sections(), 16, 12_345, 0);

    // 1. 存活表 = 名单序 [B, A]（不是字母序）。
    assert_eq!(m.names(), vec!["B".to_string(), "A".to_string()]);
    assert_eq!(m.guilds[0].name, "B");
    assert_eq!(m.guilds[1].name, "A");

    // 2. GuildList.txt 回写为 B\r\nA\r\n（含末 CRLF）。
    assert_eq!(sh.read("GuildList.txt"), gbk("B\r\nA\r\n"));

    // 3. 三个 .ini 都在且 0 字节（C 的 .ini 在 .txt 缺失前已建）。
    for n in ["A", "B", "C"] {
        assert!(sh.exists(&format!("Guilds/{n}.ini")), "{n}.ini 必须存在");
        assert!(
            sh.is_empty(&format!("Guilds/{n}.ini")),
            "{n}.ini 必须 0 字节"
        );
    }

    // 4. 行会战：A 引用 C（已逆序移除）⇒ 空；B 的 +B 保留，war_tick=now、war_time=0。
    let a = m.find_guild("A").unwrap();
    assert!(a.wars.is_empty(), "引用已移除会 ⇒ 战条不入表");
    let b = m.find_guild("B").unwrap();
    assert_eq!(b.wars.len(), 1);
    assert_eq!(b.wars[0].guild_name, "B");
    assert_eq!(b.wars[0].war_tick, 12_345);
    assert_eq!(b.wars[0].war_time, 0, "oracle 残留 s20 ⇒ WarTime 恒 0");

    // 6a. 段头解析（4 键）。
    assert_eq!(a.notices, vec!["N-A".to_string()]);
    assert_eq!(a.ranks[0].members, vec!["chiefA".to_string()]);
    assert!(!b.wars.is_empty());

    // 7. 加载后置脏（5 setter）、DwSaveTick 保持 0。
    for g in &m.guilds {
        assert!(g.bo_changed, "GuildConf.LoadConfig 的 setter 置脏");
        assert_eq!(g.dw_save_tick, 0);
    }
}

#[test]
fn g3_w1_missing_list_file_is_empty_not_fatal() {
    let sh = Shadow::new("w1-missing");
    let store = sh.store();
    let m = startup(Some(&store), &sections(), 16, 0, 0);
    assert!(m.guilds.is_empty(), "MissingListFile ⇒ 空表、不 panic");
}

#[test]
fn g3_w1_six_string_keys_and_msg_cfg_single_source() {
    // `[Guild]` 六键 + `[String] GuildMsgPreFix`（`StringConf.cs:30-38`）。
    let sc = crate::strings::parse(&gbk(
        "[Guild]\nGuildNotice=公告X\nGuildWar=敌对X\nGuildAll=联盟X\n\
         GuildMember=成员段X\nGuildMemberRank=职位X\nGuildChief=掌门X\n\
         [String]\nGuildMsgPreFix=〖行会X〗\n",
    ));
    let mut c =
        GuildConfig::from_guild_castle(&mir2_data::guild_castle::GuildCastleConfig::default());
    // I-6：生产源是 String.conf，不是 GuildCastleConfig。
    c.guild_chief = sc.guild_chief().to_string();
    c.guild_member_rank = sc.guild_member_rank().to_string();
    c.guild_msg_f_color = 0x0A;
    c.guild_msg_b_color = 0x0B;
    assert_eq!(c.guild_chief, "掌门X");
    assert_eq!(c.guild_member_rank, "职位X");
    assert_eq!(
        GuildFileSections::from_strings(
            sc.guild_notice(),
            sc.guild_war(),
            sc.guild_all(),
            sc.guild_member()
        ),
        sections()
    );
    // M-8：颜色从 GuildConfig 转发（唯一来源）。
    let msg = GuildMsgCfg::from_config(&c, true, sc.guild_msg_prefix());
    assert!(msg.show_pre_fix_msg);
    assert_eq!(msg.prefix, "〖行会X〗");
    assert_eq!((msg.f_color, msg.b_color), (0x0A, 0x0B));
}

// ---------------------------------------------------------------------------
// W2 10s 门 / 30s 门 / 战争到期
// ---------------------------------------------------------------------------

fn w2_shadow() -> Shadow {
    let sh = Shadow::new("w2");
    sh.write("GuildList.txt", "G\r\n");
    sh.write(
        "Guilds/G.txt",
        "公告X\r\n+N1\r\n \r\n敌对X\r\n \r\n联盟X\r\n \r\n成员段X\r\n#1 掌门X\r\n+gm\r\n#99 职位X\r\n+a\r\n",
    );
    sh
}

fn no_recipients(_: &str) -> Option<(i32, bool)> {
    None
}

#[test]
fn g3_w2_case_a_strict_10s_gate_and_30s_check_save() {
    let sh = w2_shadow();
    let store = sh.store();
    let mut m = startup(Some(&store), &sections(), 16, 0, 0);
    m.find_guild_mut("G").unwrap().notices = vec!["N2".to_string()];
    let mut run_tick = 0i32;

    let o = step(
        &mut m,
        Some(&store),
        &sections(),
        &msg_cfg(false),
        0,
        10_000,
        &mut run_tick,
        &no_recipients,
    );
    assert!(!o.ran, "10000-0 == 10000 ⇒ 严格 > 不过门");

    let o = step(
        &mut m,
        Some(&store),
        &sections(),
        &msg_cfg(false),
        0,
        10_200,
        &mut run_tick,
        &no_recipients,
    );
    assert!(o.ran);
    assert!(o.events.is_empty(), "10200 无 Save（30s 门未到）");

    let o = step(
        &mut m,
        Some(&store),
        &sections(),
        &msg_cfg(false),
        0,
        20_400,
        &mut run_tick,
        &no_recipients,
    );
    assert!(o.ran);
    assert!(o.events.is_empty());

    let o = step(
        &mut m,
        Some(&store),
        &sections(),
        &msg_cfg(false),
        0,
        30_600,
        &mut run_tick,
        &no_recipients,
    );
    assert!(o.ran);
    assert_eq!(
        o.events,
        vec![GuildRunEvent::Save {
            guild: "G".into(),
            reason: GuildSaveReason::CheckSaveGuildFile
        }]
    );
    let file = sh.read_text("Guilds/G.txt");
    assert!(file.contains("+N2"), "内存 N2 必须被 30s check_save 写出");
    assert!(!file.contains("+N1"));
    // 写侧 4 段头。
    assert!(file.starts_with("公告X\r\n"));
    for line in ["敌对X", "联盟X", "成员段X"] {
        assert!(file.lines().any(|l| l == line), "缺段头 {line}");
    }
    let g = m.find_guild("G").unwrap();
    assert!(!g.bo_changed);
    assert_eq!(g.dw_save_tick, 0, "CheckSave 不推 DwSaveTick");
}

#[test]
fn g3_w2_case_b_injected_clock_origin() {
    let sh = w2_shadow();
    let store = sh.store();
    let mut m = startup(Some(&store), &sections(), 16, 0, 0);
    assert!(m.find_guild("G").unwrap().bo_changed);
    let mut run_tick = 3_600_000i32;

    let o = step(
        &mut m,
        Some(&store),
        &sections(),
        &msg_cfg(false),
        0,
        3_610_200,
        &mut run_tick,
        &no_recipients,
    );
    assert_eq!(
        o.events,
        vec![GuildRunEvent::Save {
            guild: "G".into(),
            reason: GuildSaveReason::CheckSaveGuildFile
        }]
    );
    assert!(!m.find_guild("G").unwrap().bo_changed);

    let o = step(
        &mut m,
        Some(&store),
        &sections(),
        &msg_cfg(false),
        0,
        3_620_400,
        &mut run_tick,
        &no_recipients,
    );
    assert!(o.ran, "10s 门照过");
    assert!(o.events.is_empty(), "bo_changed=false ⇒ 第二跳无 Save");
}

#[test]
fn g3_w2_case_c_war_expiry_event_order_and_104() {
    let sh = Shadow::new("w2-c");
    sh.write("GuildList.txt", "G\r\n");
    sh.write(
        "Guilds/G.txt",
        "公告X\r\n \r\n敌对X\r\n \r\n联盟X\r\n \r\n成员段X\r\n#1 掌门X\r\n+gm\r\n#99 职位X\r\n+b\r\n+c\r\n",
    );
    let store = sh.store();
    let c = cfg();
    let mut m = GuildManager::new();
    assert!(m.add_guild("G", "gm", &c));
    m.find_guild_mut("G").unwrap().add_member("b", &c, 0);
    m.find_guild_mut("G").unwrap().add_member("c", &c, 0);
    {
        let g = m.find_guild_mut("G").unwrap();
        g.wars.push(mir2_world::guild::WarGuild {
            guild_name: "E1".into(),
            war_tick: 100,
            war_time: 50,
        });
        g.wars.push(mir2_world::guild::WarGuild {
            guild_name: "E2".into(),
            war_tick: 1000,
            war_time: 50,
        });
    }
    let rcpt = |name: &str| match name {
        "b" => Some((2, true)),
        "c" => Some((3, true)),
        _ => None,
    };
    // now=150：门放行，但 150-100=50 不 > 50 ⇒ 无 WarEnded、无 Save。
    let mut run_tick = 150 - 10_001;
    let o = step(
        &mut m,
        Some(&store),
        &sections(),
        &msg_cfg(true),
        0,
        150,
        &mut run_tick,
        &rcpt,
    );
    assert!(o.ran);
    assert!(o.events.is_empty());

    // now=151：WarEnded(E1) 先、Save{UpdateGuildFile} 后。
    run_tick = 151 - 10_001;
    let o = step(
        &mut m,
        Some(&store),
        &sections(),
        &msg_cfg(true),
        0,
        151,
        &mut run_tick,
        &rcpt,
    );
    assert_eq!(
        o.events,
        vec![
            GuildRunEvent::WarEnded {
                guild: "G".into(),
                enemy: "E1".into(),
                message: "***E1行会战争结束".into()
            },
            GuildRunEvent::Save {
                guild: "G".into(),
                reason: GuildSaveReason::UpdateGuildFile
            }
        ]
    );
    // 104：文本带前缀；收件人 rank 序 b,c；Param 与 Series。
    assert_eq!(o.notices.len(), 2);
    for (i, (aid, body)) in o.notices.iter().enumerate() {
        let (recog, ident, param, series, text) = parse_104(body);
        assert_eq!(recog, *aid);
        assert_eq!(aid, &i32::try_from(i + 2).unwrap());
        assert_eq!(ident, messages::SM_GUILDMESSAGE);
        assert_eq!(param, u16::from_le_bytes([0x0A, 0x0B]));
        assert_eq!(series, 1);
        assert_eq!(text, "〖行会X〗***E1行会战争结束");
    }
    let file = sh.read_text("Guilds/G.txt");
    assert!(!file.contains("E1"), "到期战条被移除");
    assert!(
        file.contains("+E2 899"),
        "E2 剩余 = 50-(151-1000)=899，实得:\n{file}"
    );
    assert!(
        m.find_guild("G").unwrap().bo_changed,
        "UpdateGuildFile 置脏"
    );
    assert_eq!(m.find_guild("G").unwrap().dw_save_tick, 151);
}

// ---------------------------------------------------------------------------
// W3 变更点立即落盘
// ---------------------------------------------------------------------------

const GM: &str = "gm";
const TARGET: &str = "新人";
const GREEN: (u8, u8) = (0xDB, 0xFF);
const RED: (u8, u8) = (0xFF, 0x38);

fn write_guild_file(sh: &Shadow, name: &str, notices: &[&str], ranks: &[(i16, &str, &[&str])]) {
    let mut text = String::from("公告X\r\n");
    for n in notices {
        text.push_str(&format!("+{n}\r\n"));
    }
    text.push_str(" \r\n敌对X\r\n \r\n联盟X\r\n \r\n成员段X\r\n");
    for (no, rn, members) in ranks {
        text.push_str(&format!("#{no} {rn}\r\n"));
        for m in *members {
            text.push_str(&format!("+{m}\r\n"));
        }
    }
    sh.write(&format!("Guilds/{name}.txt"), &text);
}

struct CmdRig {
    online: crate::OnlineRegistry,
    world: World,
    displays: game_svr::downlink::DisplayTable,
    gates: HashMap<u8, tokio::sync::mpsc::UnboundedSender<crate::SimToGate>>,
    sessions: HashMap<String, crate::SessionGround>,
    guilds: GuildManager,
    cfg: GuildConfig,
    store: GuildStore,
    sections: GuildFileSections,
    msg_cfg: GuildMsgCfg,
}

fn add_player(world: &mut World, name: &str, x: i32, y: i32, dir: u8) -> i32 {
    let mut e = Entity::new(0, name.to_string(), x, y);
    e.race = mir2_world::ACTOR_RACE_PLAY;
    e.ai.dir = dir;
    world.add_entity(e)
}

impl CmdRig {
    /// `players` 第一个是掌门（面右），其余在 (6,5) 面左（互相面对）。
    fn new(sh: &Shadow, players: &[&str]) -> Self {
        let store = sh.store();
        let sections = sections();
        let cfg = cfg();
        let guilds = startup(Some(&store), &sections, cfg.guild_rank_name_len, 0, 0);
        let mut world = World::with_maps(vec![(20, 20)]);
        let mut displays = game_svr::downlink::DisplayTable::new();
        let mut online: crate::OnlineRegistry = Vec::new();
        let mut sessions = HashMap::new();
        for (i, name) in players.iter().enumerate() {
            let (x, y, dir) = if i == 0 { (5, 5, 2) } else { (6, 5, 6) };
            let id = add_player(&mut world, name, x, y, dir);
            displays.set(
                id,
                game_svr::downlink::ActorDisplay::new(
                    0,
                    0,
                    0,
                    0,
                    0,
                    255,
                    crate::downlink::gbk(name),
                ),
            );
            crate::online_set(
                &mut online,
                crate::OnlineEntry {
                    account: (*name).to_string(),
                    chr_name: (*name).to_string(),
                    session_id: i as i32 + 1,
                    gate_idx: 0,
                    socket: 100 + i as i32,
                    actor_id: id,
                },
            );
            sessions.insert((*name).to_string(), test_session_ground(id));
        }
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut gates = HashMap::new();
        gates.insert(0u8, tx);
        Self {
            online,
            world,
            displays,
            gates,
            sessions,
            guilds,
            cfg,
            store,
            sections,
            msg_cfg: msg_cfg(false),
        }
    }

    fn set_master(&mut self, name: &str, guild: &str) {
        let s = &mut self.sessions.get_mut(name).unwrap().guild;
        s.guild_name = Some(guild.to_string());
        s.rank_no = 1;
        s.rank_name = self.cfg.guild_chief.clone();
        s.allow_guild = true;
    }

    fn allow(&mut self, name: &str) {
        self.sessions.get_mut(name).unwrap().guild.allow_guild = true;
    }

    fn clear_dirty(&mut self) {
        let mut run_tick = 0i32;
        let _ = step(
            &mut self.guilds,
            Some(&self.store),
            &self.sections,
            &self.msg_cfg,
            0,
            31_000,
            &mut run_tick,
            &no_recipients,
        );
    }

    fn cmd(
        &mut self,
        ident: u16,
        tail: &str,
        chr: &str,
        now: i32,
    ) -> Vec<guild_cmd::GuildServerEvent> {
        guild_cmd::handle_guild_cmd(
            ident,
            Some(tail),
            chr,
            100,
            &self.online,
            &self.world,
            &self.displays,
            &mut self.guilds,
            &self.cfg,
            &mut self.sessions,
            &self.gates,
            12,
            GREEN,
            RED,
            Some(&self.store),
            &self.sections,
            &self.msg_cfg,
            0,
            now,
        )
    }
}

#[test]
fn g3_w3_1038_add_member_saves_immediately() {
    let sh = Shadow::new("w3-1038");
    sh.write("GuildList.txt", "G\r\n");
    write_guild_file(
        &sh,
        "G",
        &["N1"],
        &[(1, "掌门X", &[GM]), (99, "职位X", &["old"])],
    );
    let mut rig = CmdRig::new(&sh, &[GM, TARGET]);
    rig.set_master(GM, "G");
    rig.allow(TARGET);
    rig.clear_dirty();
    rig.cmd(messages::CM_GUILDADDMEMBER, TARGET, GM, 40_000);
    let file = sh.read_text("Guilds/G.txt");
    assert!(
        file.contains(&format!("+{TARGET}")),
        "1038 必须当场落盘: {file}"
    );
    let g = rig.guilds.find_guild("G").unwrap();
    assert!(g.bo_changed);
    assert_eq!(g.dw_save_tick, 40_000);
}

#[test]
fn g3_w3_1038b_creates_rank99_with_member_rank_header() {
    let sh = Shadow::new("w3-1038b");
    sh.write("GuildList.txt", "G\r\n");
    write_guild_file(&sh, "G", &["N1"], &[(1, "掌门X", &[GM])]);
    let mut rig = CmdRig::new(&sh, &[GM, TARGET]);
    rig.set_master(GM, "G");
    rig.allow(TARGET);
    rig.clear_dirty();
    rig.cmd(messages::CM_GUILDADDMEMBER, TARGET, GM, 40_000);
    let file = sh.read_text("Guilds/G.txt");
    assert!(
        file.lines().any(|l| l == "#99 职位X"),
        "GuildMemberRank 行为牙（缺省会写 行会成员）: {file}"
    );
    assert!(file.contains(&format!("+{TARGET}")));
}

#[test]
fn g3_w3_1039_delete_other_saves_immediately() {
    let sh = Shadow::new("w3-1039");
    sh.write("GuildList.txt", "G\r\n");
    write_guild_file(
        &sh,
        "G",
        &["N1"],
        &[(1, "掌门X", &[GM]), (99, "职位X", &[TARGET])],
    );
    let mut rig = CmdRig::new(&sh, &[GM]);
    rig.set_master(GM, "G");
    rig.clear_dirty();
    rig.cmd(messages::CM_GUILDDELMEMBER, TARGET, GM, 40_000);
    let file = sh.read_text("Guilds/G.txt");
    assert!(!file.contains(&format!("+{TARGET}")), "1039 必须当场落盘");
    let g = rig.guilds.find_guild("G").unwrap();
    assert!(g.bo_changed);
    assert_eq!(g.dw_save_tick, 40_000);
}

#[test]
fn g3_w3_1039_self_dissolve_writes_empty_body_and_removes_list_entry() {
    let sh = Shadow::new("w3-self");
    sh.write("GuildList.txt", "G\r\n");
    write_guild_file(&sh, "G", &["N1"], &[(1, "掌门X", &[GM])]);
    let mut rig = CmdRig::new(&sh, &[GM]);
    rig.set_master(GM, "G");
    rig.clear_dirty();
    rig.cmd(messages::CM_GUILDDELMEMBER, GM, GM, 40_000);
    assert_eq!(
        sh.read_text("Guilds/G.txt"),
        "公告X\r\n \r\n敌对X\r\n \r\n联盟X\r\n \r\n成员段X\r\n",
        "自删解散 ⇒ 空会 7 行体（文件仍存在）"
    );
    assert!(!sh.read_text("GuildList.txt").contains('G'));
    assert!(!sh.exists("Guilds/G.bak"), "不写 .bak");
    assert!(!sh.exists("Guilds/G"), "不建 <名>/ 子目录");
    let g = rig.guilds.find_guild("G");
    assert!(g.is_none());
    // bo_changed/dw_save_tick 不变（CancelGuld/DelGuild 都不置脏）。
}

#[test]
fn g3_w3_1040_notice_direct_save_keeps_dirty_flags() {
    let sh = Shadow::new("w3-1040");
    sh.write("GuildList.txt", "G\r\n");
    write_guild_file(&sh, "G", &["N1"], &[(1, "掌门X", &[GM])]);
    let mut rig = CmdRig::new(&sh, &[GM]);
    rig.set_master(GM, "G");
    rig.clear_dirty();
    rig.cmd(messages::CM_GUILDUPDATENOTICE, "N3", GM, 40_000);
    let file = sh.read_text("Guilds/G.txt");
    assert!(file.contains("+N3"));
    let g = rig.guilds.find_guild("G").unwrap();
    assert!(!g.bo_changed, "1040 直写不经 UpdateGuildFile");
    assert_eq!(g.dw_save_tick, 0);
}

#[test]
fn g3_w3_1041_update_rank_saves_and_clears_ranks() {
    let sh = Shadow::new("w3-1041");
    sh.write("GuildList.txt", "G\r\n");
    write_guild_file(&sh, "G", &["N1"], &[(1, "掌门X", &[GM])]);
    let mut rig = CmdRig::new(&sh, &[GM]);
    rig.set_master(GM, "G");
    rig.clear_dirty();
    rig.cmd(
        messages::CM_GUILDUPDATERANKINFO,
        "#1 职位X\rgm\r",
        GM,
        40_000,
    );
    let file = sh.read_text("Guilds/G.txt");
    assert!(
        !file.lines().any(|l| l.starts_with('#')),
        "N3 清空 ⇒ 文件无 # 行: {file}"
    );
    let g = rig.guilds.find_guild("G").unwrap();
    assert!(g.bo_changed);
    assert_eq!(g.dw_save_tick, 40_000);
}

#[test]
fn g3_w3_1044_1045_are_write_free() {
    let sh = Shadow::new("w3-44");
    sh.write("GuildList.txt", "G\r\n");
    write_guild_file(&sh, "G", &["N1"], &[(1, "掌门X", &[GM])]);
    let mut rig = CmdRig::new(&sh, &[GM]);
    rig.set_master(GM, "G");
    rig.clear_dirty();
    let before = sh.read("Guilds/G.txt");
    rig.cmd(messages::CM_GUILDALLY, "", GM, 40_000);
    rig.cmd(messages::CM_GUILDBREAKALLY, "别会", GM, 40_000);
    assert_eq!(
        sh.read("Guilds/G.txt"),
        before,
        "1044/1045 生产恒失败、无新写"
    );
    let g = rig.guilds.find_guild("G").unwrap();
    assert!(!g.bo_changed);
    assert_eq!(g.dw_save_tick, 0);
}

#[test]
fn g3_w3_0_add_guild_two_variants() {
    // 变体 1：预存旧体 .txt ⇒ load_guild_conf 建 .ini → add_guild → save_one → save_list。
    let sh = Shadow::new("w3-0a");
    sh.write("GuildList.txt", "G\r\n");
    sh.write("Guilds/新会.txt", "公告X\r\n+OLD\r\n");
    let store = sh.store();
    let c = cfg();
    let _ = store.load_guild_conf("新会").expect("建 .ini");
    let mut m = GuildManager::new();
    assert!(m.add_guild("新会", "chiefX", &c));
    let g = m.find_guild("新会").unwrap();
    save_one(Some(&store), &sections(), g, 1_000, 0).expect("旧文件存在 ⇒ 可写");
    save_list(Some(&store), &m.names(), 0).unwrap();
    let file = sh.read_text("Guilds/新会.txt");
    assert!(file.contains("#1 掌门X"), "GuildChief 行为牙: {file}");
    assert!(file.contains("+chiefX"));
    assert!(!file.contains("OLD"));
    assert!(sh.is_empty("Guilds/新会.ini"));
    assert!(sh.read_text("GuildList.txt").contains("新会"));
    let g = m.find_guild("新会").unwrap();
    assert!(!g.bo_changed, "SetGuildInfo 不置脏");
    assert_eq!(g.dw_save_tick, 0);

    // 变体 2：<新会>.txt 缺失 ⇒ save_one 返回 MissingSaveTarget（不建文件），名单仍增。
    let sh = Shadow::new("w3-0b");
    sh.write("GuildList.txt", "G\r\n");
    let store = sh.store();
    let _ = store.load_guild_conf("新会").expect("建 .ini");
    let mut m = GuildManager::new();
    assert!(m.add_guild("新会", "chiefX", &c));
    let err = save_one(
        Some(&store),
        &sections(),
        m.find_guild("新会").unwrap(),
        1_000,
        0,
    )
    .expect_err("缺文件必须 Err");
    assert!(matches!(err, GuildStoreError::MissingSaveTarget(_)));
    assert!(!sh.exists("Guilds/新会.txt"), "不代 oracle 创建文件");
    save_list(Some(&store), &m.names(), 0).unwrap();
    assert!(sh.read_text("GuildList.txt").contains("新会"));
}

// ---------------------------------------------------------------------------
// W4 断线生命周期
// ---------------------------------------------------------------------------

const W4_NAMES: [&str; 5] = ["gm", "a", "b", "bb", "c"];

fn w4_world() -> (World, crate::OnlineRegistry, HashMap<String, i32>) {
    let mut world = World::with_maps(vec![(40, 40)]);
    let mut online: crate::OnlineRegistry = Vec::new();
    let mut ids = HashMap::new();
    for (i, name) in W4_NAMES.iter().enumerate() {
        let id = world.add_entity(Entity::new(0, (*name).to_string(), 5, 5));
        crate::online_set(
            &mut online,
            crate::OnlineEntry {
                account: (*name).to_string(),
                chr_name: (*name).to_string(),
                session_id: i as i32 + 1,
                gate_idx: 0,
                socket: 500 + i as i32,
                actor_id: id,
            },
        );
        ids.insert((*name).to_string(), id);
    }
    (world, online, ids)
}

fn base_manager() -> GuildManager {
    let c = cfg();
    let mut m = GuildManager::new();
    assert!(m.add_guild("G", "gm", &c));
    for n in ["a", "b", "bb", "c"] {
        m.find_guild_mut("G").unwrap().add_member(n, &c, 0);
    }
    m.find_guild_mut("G").unwrap().notices = vec!["N2".to_string()];
    m
}

fn w4_shadow() -> Shadow {
    let sh = Shadow::new("w4");
    sh.write("GuildList.txt", "G\r\n");
    sh.write(
        "Guilds/G.txt",
        "公告X\r\n+N1\r\n \r\n敌对X\r\n \r\n联盟X\r\n \r\n成员段X\r\n#1 掌门X\r\n+gm\r\n#99 职位X\r\n+a\r\n+b\r\n+bb\r\n+c\r\n",
    );
    sh
}

fn w4_sessions() -> HashMap<String, GuildSession> {
    let mut s: HashMap<String, GuildSession> = W4_NAMES
        .iter()
        .map(|n| ((*n).to_string(), GuildSession::default()))
        .collect();
    s.get_mut("a").unwrap().guild_name = Some("G".to_string());
    s
}

fn ban_from(sessions: &HashMap<String, GuildSession>) -> HashMap<String, bool> {
    sessions
        .iter()
        .map(|(k, s)| (k.clone(), s.ban_guild_chat))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn disconnect<S: GuildSessions>(
    guilds: &mut GuildManager,
    store: Option<&GuildStore>,
    sessions: &mut S,
    ctx: &DisconnectCtx<'_>,
    online: &crate::OnlineRegistry,
    world: &World,
    cfg: &GuildMsgCfg,
    ban: &HashMap<String, bool>,
) -> DisconnectOutcome {
    let rcpt = crate::guild_recipients(online, world, ban);
    on_disconnect(guilds, store, &sections(), cfg, sessions, ctx, &rcpt)
}

fn ctx_a<'a>(chr: &'a str, now: i32, server: i32) -> DisconnectCtx<'a> {
    DisconnectCtx {
        chr_name: chr,
        bo_reconnection: false,
        bo_soft_close: true,
        now_tick: now,
        server_index: server,
    }
}

#[test]
fn g3_w4_1_single_source_path_a_and_check_save_before_clear() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert_eq!(
        out.cleared.as_deref(),
        Some("b"),
        "Compare(b,a)==1 清首个槽"
    );
    assert_eq!(
        out.notices.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![ids["gm"], ids["b"], ids["bb"], ids["c"]],
        "104 收件人 = rank/member 序、排除 a"
    );
    assert!(out.saved.as_ref().is_some_and(|r| r.is_ok()));
    let file = sh.read_text("Guilds/G.txt");
    assert!(file.contains("+N2"), "check_save 确实落盘: {file}");
    assert!(
        !file.contains("+\r\n"),
        "清槽在落盘之后 ⇒ 文件无空槽: {file}"
    );
    for n in ["gm", "a", "b", "bb", "c"] {
        assert!(file.contains(&format!("+{n}")), "断线后文件仍是旧体");
    }
    let g = m.find_guild("G").unwrap();
    assert!(!g.bo_changed);
    assert_eq!(g.dw_save_tick, 0, "CheckSave 不推 DwSaveTick");
    assert_eq!(g.count(), 5);
    assert!(!g.is_member("b"));
    assert!(!g.is_member(""));
    assert!(g.is_member("bb") && g.is_member("c"));
}

#[test]
fn g3_w4_2a_104_frame_fields_with_prefix() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert_eq!(out.notices.len(), 4, "不收 a");
    for (aid, body) in &out.notices {
        let (recog, ident, param, series, text) = parse_104(body);
        assert_eq!(recog, *aid, "Recog=收件人自己");
        assert_eq!(ident, messages::SM_GUILDMESSAGE);
        assert_eq!(param, u16::from_le_bytes([0x0A, 0x0B]));
        assert_eq!(series, 1);
        assert_eq!(text, "〖行会X〗a 已经退出游戏.");
    }
}

#[test]
fn g3_w4_2b_104_without_prefix() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(false),
        &ban,
    );
    for (_, body) in &out.notices {
        let (_, _, _, _, text) = parse_104(body);
        assert_eq!(text, "a 已经退出游戏.", "ShowPreFixMsg=0 ⇒ 无前缀");
    }
}

#[test]
fn g3_w4_3_cleared_slot_is_written_by_next_direct_save() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let _ = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(false),
        &ban,
    );
    // `handle_guild_cmd(1040, "N3")` by gm：会话在 `HashMap<String,GuildSession>` 模型里。
    sessions.get_mut("gm").unwrap().guild_name = Some("G".into());
    sessions.get_mut("gm").unwrap().rank_no = 1;
    sessions.get_mut("gm").unwrap().rank_name = "掌门X".into();
    let gates: HashMap<u8, tokio::sync::mpsc::UnboundedSender<crate::SimToGate>> = HashMap::new();
    let displays = game_svr::downlink::DisplayTable::new();
    guild_cmd::handle_guild_cmd(
        messages::CM_GUILDUPDATENOTICE,
        Some("N3"),
        "gm",
        500,
        &online,
        &world,
        &displays,
        &mut m,
        &cfg(),
        &mut sessions,
        &gates,
        12,
        GREEN,
        RED,
        Some(&store),
        &sections(),
        &msg_cfg(false),
        0,
        41_000,
    );
    let file = sh.read_text("Guilds/G.txt");
    assert!(file.contains("+\r\n"), "清槽由下一次直写带出: {file}");
    assert!(file.contains("+N3"));
    let g = m.find_guild("G").unwrap();
    assert!(!g.bo_changed);
    assert_eq!(g.dw_save_tick, 0);
}

#[test]
fn g3_w4_4a_softclose_shape_no_104_but_saves_and_clears() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let ctx = DisconnectCtx {
        chr_name: "a",
        bo_reconnection: true,
        bo_soft_close: true,
        now_tick: 40_000,
        server_index: 0,
    };
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx,
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(out.notices.is_empty());
    assert!(out.ss208.is_none());
    assert!(out.saved.as_ref().is_some_and(|r| r.is_ok()));
    assert_eq!(out.cleared.as_deref(), Some("b"));
}

#[test]
fn g3_w4_4b_path_b_does_not_override_manager() {
    let sh = w4_shadow();
    let store = sh.store();
    let c = cfg();
    let mut m = GuildManager::new();
    assert!(m.add_guild("G", "b", &c));
    m.find_guild_mut("G").unwrap().add_member("c", &c, 0);
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let ctx = DisconnectCtx {
        chr_name: "a",
        bo_reconnection: true,
        bo_soft_close: true,
        now_tick: 40_000,
        server_index: 0,
    };
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx,
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(out.notices.is_empty());
    assert!(out.ss208.is_none());
    assert!(out.saved.as_ref().is_some_and(|r| r.is_ok()));
    assert_eq!(
        out.cleared.as_deref(),
        Some("b"),
        "清槽发生在 manager 的 G 上"
    );
    assert_eq!(
        m.find_guild("G").unwrap().ranks[0].members,
        vec![String::new()]
    );
}

#[test]
fn g3_w4_5_kick_shape_no_104_but_saves_and_clears() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let ctx = DisconnectCtx {
        chr_name: "a",
        bo_reconnection: false,
        bo_soft_close: false,
        now_tick: 40_000,
        server_index: 0,
    };
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx,
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(out.notices.is_empty());
    assert!(out.ss208.is_none());
    assert!(out.saved.is_some());
    assert_eq!(out.cleared.as_deref(), Some("b"));
}

#[test]
fn g3_w4_6a_session_none_is_overridden_by_manager() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    sessions.get_mut("a").unwrap().guild_name = None; // 会话条目存在但没有会
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert_eq!(out.notices.len(), 4, "路径 A 覆盖后照发 104");
    assert!(out.ss208.is_some());
    assert!(out.saved.is_some());
    assert_eq!(out.cleared.as_deref(), Some("b"));
    assert_eq!(
        sessions["a"].guild_name.as_deref(),
        Some("G"),
        "覆盖写回会话字段"
    );
}

#[test]
fn g3_w4_6b_manager_without_member_overrides_to_null() {
    let sh = w4_shadow();
    let store = sh.store();
    let c = cfg();
    let mut m = GuildManager::new();
    assert!(m.add_guild("G", "b", &c));
    m.find_guild_mut("G").unwrap().add_member("c", &c, 0);
    m.find_guild_mut("G").unwrap().notices = vec!["N2".to_string()];
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(out.notices.is_empty());
    assert!(out.ss208.is_none());
    assert!(out.saved.is_none(), "null ⇒ 不 check_save");
    assert!(out.cleared.is_none());
    assert!(sh.read_text("Guilds/G.txt").contains("+N1"), "文件仍 N1");
    assert!(m.find_guild("G").unwrap().bo_changed);
    assert_eq!(
        m.find_guild("G").unwrap().ranks[1].members,
        vec!["c".to_string()]
    );
}

#[test]
fn g3_w4_6c_member_of_guild_first_hit_and_order() {
    let sh = w4_shadow();
    let store = sh.store();
    let c = cfg();
    let mut m = GuildManager::new();
    // 顺序钉死 [H, G]：H 先命中。
    assert!(m.add_guild("H", "a", &c));
    m.find_guild_mut("H").unwrap().add_member("hb", &c, 0);
    assert!(m.add_guild("G", "a", &c));
    m.find_guild_mut("G").unwrap().add_member("b", &c, 0);
    assert_eq!(m.member_of_guild("a"), Some("H"));
    let mut world = World::with_maps(vec![(40, 40)]);
    let mut online: crate::OnlineRegistry = Vec::new();
    let mut ids = HashMap::new();
    for (i, name) in ["gm", "a", "hb", "b"].iter().enumerate() {
        let id = world.add_entity(Entity::new(0, (*name).to_string(), 5, 5));
        crate::online_set(
            &mut online,
            crate::OnlineEntry {
                account: (*name).to_string(),
                chr_name: (*name).to_string(),
                session_id: i as i32 + 1,
                gate_idx: 0,
                socket: 600 + i as i32,
                actor_id: id,
            },
        );
        ids.insert((*name).to_string(), id);
    }
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert_eq!(
        out.notices.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![ids["hb"]],
        "104 只发 H 的 hb"
    );
    assert!(out
        .ss208
        .as_ref()
        .is_some_and(|s| s.payload.starts_with("H//")));
    assert!(out.cleared.is_none(), "H 里 Compare(hb,a) != 1");
    assert_eq!(m.find_guild("G").unwrap().ranks[0].members, vec!["a"]);
    assert_eq!(m.find_guild("G").unwrap().ranks[1].members, vec!["b"]);
    assert_eq!(sessions["a"].guild_name.as_deref(), Some("H"));
}

#[test]
fn g3_w4_6d_no_session_entry_is_total_noop() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    sessions.remove("a");
    let ban = HashMap::new();
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert_eq!(out, DisconnectOutcome::default());
    assert_eq!(m.find_guild("G").unwrap().count(), 5);
}

#[test]
fn g3_w4_7_compare_must_equal_one() {
    let c = cfg();
    let (world, online, _ids) = w4_world();
    // [a,c] 断 a ⇒ c/a == 2 ⇒ 不清。
    let mut m = GuildManager::new();
    assert!(m.add_guild("G", "a", &c));
    m.find_guild_mut("G").unwrap().add_member("c", &c, 0);
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        None,
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(out.cleared.is_none());

    // [a] 断 a ⇒ 0 ⇒ 不清。
    let mut m = GuildManager::new();
    assert!(m.add_guild("G", "a", &c));
    let mut sessions = w4_sessions();
    let out = disconnect(
        &mut m,
        None,
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(out.cleared.is_none());

    // [b,c] 断 c ⇒ Compare(c,c)==0；b/c == -1 ⇒ 不清。
    let mut m = GuildManager::new();
    assert!(m.add_guild("G", "b", &c));
    m.find_guild_mut("G").unwrap().add_member("c", &c, 0);
    let mut sessions = w4_sessions();
    sessions.get_mut("c").unwrap().guild_name = Some("G".into());
    let out = disconnect(
        &mut m,
        None,
        &mut sessions,
        &ctx_a("c", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(out.cleared.is_none());
}

#[test]
fn g3_w4_8a_path_a_called_twice_is_idempotent() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let _ = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    let file_after_first = sh.read("Guilds/G.txt");
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_001, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(out.notices.is_empty(), "第二次不得再发 104");
    assert!(out.ss208.is_none());
    assert!(out.cleared.is_none(), "第二次不得再清槽（否则 bb 被清）");
    assert!(m.find_guild("G").unwrap().is_member("bb"));
    assert_eq!(sh.read("Guilds/G.txt"), file_after_first);
}

#[test]
fn g3_w4_8b_softclose_then_closed_and_stale_event() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let ctx = DisconnectCtx {
        chr_name: "a",
        bo_reconnection: true,
        bo_soft_close: true,
        now_tick: 40_000,
        server_index: 0,
    };
    let _ = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx,
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    // 再来 `Closed`（路径 A）⇒ `guild_done` ⇒ 完全 no-op。
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_001, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert_eq!(out, DisconnectOutcome::default());
    // `drain_soft_close` 后 `online` 已无该角色 ⇒ 陈旧 Closed 门为 false。
    let mut online_without = online.clone();
    crate::online_take(&mut online_without, "a");
    assert!(!crate::closed_event_is_current(
        &online_without,
        "a",
        0,
        504
    ));
}

#[test]
fn g3_w4_9_ban_guild_chat_false_blocks_104_but_not_ss208() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, ids) = w4_world();
    let mut sessions = w4_sessions();
    sessions.get_mut("b").unwrap().ban_guild_chat = false;
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(!out.notices.iter().any(|(id, _)| *id == ids["b"]));
    assert!(out.ss208.is_some(), "SS_208 与实际发出帧数无关");
}

#[test]
fn g3_w4_10a_ss208_three_fields() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    m.find_guild_mut("G").unwrap().bo_changed = false;
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 7),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    let ss = out.ss208.expect("SS_208");
    assert_eq!(ss.ident, messages::SS_208);
    assert_eq!(ss.ident, 208);
    assert_eq!(ss.server_index, 7);
    assert_eq!(ss.payload, "G//a has exited the game.");
    assert!(out.saved.is_none(), "bo_changed=false ⇒ 不 save");
}

#[test]
fn g3_w4_10b_path_gate_controls_104_and_ss208() {
    for (reconn, soft) in [(true, true), (false, false)] {
        let sh = w4_shadow();
        let store = sh.store();
        let mut m = base_manager();
        let (world, online, _ids) = w4_world();
        let mut sessions = w4_sessions();
        let ban = ban_from(&sessions);
        let ctx = DisconnectCtx {
            chr_name: "a",
            bo_reconnection: reconn,
            bo_soft_close: soft,
            now_tick: 40_000,
            server_index: 0,
        };
        let out = disconnect(
            &mut m,
            Some(&store),
            &mut sessions,
            &ctx,
            &online,
            &world,
            &msg_cfg(true),
            &ban,
        );
        assert!(out.notices.is_empty(), "flags={reconn}/{soft}");
        assert!(out.ss208.is_none(), "flags={reconn}/{soft}");
    }
}

#[test]
fn g3_w4_10c_path_a_manager_missing_guild_has_no_ss208() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = GuildManager::new();
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(out.notices.is_empty());
    assert!(out.ss208.is_none());
}

#[test]
fn g3_w4_10d_only_leaver_still_emits_ss208() {
    let sh = w4_shadow();
    let store = sh.store();
    let c = cfg();
    let mut m = GuildManager::new();
    assert!(m.add_guild("G", "a", &c));
    let (world, online, _ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(out.notices.is_empty(), "只有离场者 ⇒ 104 零帧");
    assert!(out.ss208.is_some(), "门是 MyGuild != null，不是帧发得出去");
}

#[test]
fn g3_w4_11_online_still_contains_leaver_and_is_excluded() {
    let sh = w4_shadow();
    let store = sh.store();
    let mut m = base_manager();
    let (world, online, ids) = w4_world();
    let mut sessions = w4_sessions();
    let ban = ban_from(&sessions);
    // Closed 的真实时序：离场者仍在 online 且实体 ghost=false。
    assert!(online.iter().any(|e| e.chr_name == "a"));
    assert!(!world.entities.get(ids["a"]).unwrap().ghost);
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("a", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert!(!out.notices.iter().any(|(id, _)| *id == ids["a"]));
    assert_eq!(out.notices.len(), 4);
}

#[test]
fn g3_w4_12_gm_del_guild_then_member_disconnect_is_noop() {
    let sh = w4_shadow();
    let store = sh.store();
    let c = cfg();
    let mut m = GuildManager::new();
    assert!(m.add_guild("G", "a", &c));
    m.find_guild_mut("G").unwrap().ranks[0]
        .members
        .push("hb".to_string());
    assert_eq!(m.find_guild("G").unwrap().ranks.len(), 1);
    assert!(
        m.del_guild("G").is_some(),
        "单 rank 多成员可删（GM del_guild）"
    );
    assert!(m.find_guild("G").is_none());
    let mut world = World::with_maps(vec![(40, 40)]);
    let id = world.add_entity(Entity::new(0, "hb".to_string(), 5, 5));
    let mut online: crate::OnlineRegistry = Vec::new();
    crate::online_set(
        &mut online,
        crate::OnlineEntry {
            account: "hb".into(),
            chr_name: "hb".into(),
            session_id: 1,
            gate_idx: 0,
            socket: 700,
            actor_id: id,
        },
    );
    let mut sessions: HashMap<String, GuildSession> = HashMap::new();
    let hb = GuildSession {
        guild_name: Some("G".into()),
        ..GuildSession::default()
    };
    sessions.insert("hb".into(), hb);
    let ban = ban_from(&sessions);
    // 路径 A：member_of_guild == None ⇒ 全 no-op。
    let out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions,
        &ctx_a("hb", 40_000, 0),
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    assert_eq!(out, DisconnectOutcome::default());
    // 路径 B：会话缓存指向已删会 ⇒ find_guild == None ⇒ 同样 no-op。
    let mut sessions_b: HashMap<String, GuildSession> = HashMap::new();
    sessions_b.insert("hb".into(), sessions["hb"].clone());
    let mut out = disconnect(
        &mut m,
        Some(&store),
        &mut sessions_b,
        &DisconnectCtx {
            chr_name: "hb",
            bo_reconnection: false,
            bo_soft_close: false,
            now_tick: 40_000,
            server_index: 0,
        },
        &online,
        &world,
        &msg_cfg(true),
        &ban,
    );
    out.notices.clear();
    assert!(out.ss208.is_none() && out.saved.is_none() && out.cleared.is_none());
}

#[test]
fn g3_w4_13_closed_event_is_current_four_cases() {
    let mut world = World::with_maps(vec![(20, 20)]);
    let id = world.add_entity(Entity::new(0, "a".to_string(), 5, 5));
    let mut online: crate::OnlineRegistry = Vec::new();
    crate::online_set(
        &mut online,
        crate::OnlineEntry {
            account: "a".into(),
            chr_name: "a".into(),
            session_id: 1,
            gate_idx: 0,
            socket: 501,
            actor_id: id,
        },
    );
    assert!(crate::closed_event_is_current(&online, "a", 0, 501));
    assert!(!crate::closed_event_is_current(&online, "a", 0, 500));
    assert!(!crate::closed_event_is_current(&online, "a", 1, 501));
    assert!(!crate::closed_event_is_current(&online, "nope", 0, 500));
}

#[test]
fn g3_w4_14_prepare_closed_guild_stale_does_not_dirty_new_session() {
    let mut world = World::with_maps(vec![(20, 20)]);
    let id = world.add_entity(Entity::new(0, "a".to_string(), 5, 5));
    let mut online: crate::OnlineRegistry = Vec::new();
    crate::online_set(
        &mut online,
        crate::OnlineEntry {
            account: "a".into(),
            chr_name: "a".into(),
            session_id: 2,
            gate_idx: 0,
            socket: 501,
            actor_id: id,
        },
    );
    let mut sessions: HashMap<String, crate::SessionGround> = HashMap::new();
    let mut sg = test_session_ground(id);
    sg.bo_soft_close = false;
    sg.bo_reconnection = true;
    sessions.insert("a".into(), sg);

    // ① 陈旧 (0,500)：返回 false、不置位。
    assert!(!crate::prepare_closed_guild(
        &mut sessions,
        &online,
        "a",
        0,
        500
    ));
    assert!(!sessions["a"].bo_soft_close, "置位必须在早退之后");
    assert!(sessions["a"].bo_reconnection);

    // ② 当前 (0,501)：返回 true、置 soft、不动 reconnection。
    assert!(crate::prepare_closed_guild(
        &mut sessions,
        &online,
        "a",
        0,
        501
    ));
    assert!(sessions["a"].bo_soft_close);
    assert!(sessions["a"].bo_reconnection);

    // ③ 行为记录：路径 B（= CM_SOFTCLOSE 后再 Closed）。
    let mut m = GuildManager::new();
    let bo_reconnection = sessions["a"].bo_reconnection;
    let bo_soft_close = sessions["a"].bo_soft_close;
    let out = on_disconnect(
        &mut m,
        None,
        &sections(),
        &msg_cfg(true),
        &mut sessions,
        &DisconnectCtx {
            chr_name: "a",
            bo_reconnection,
            bo_soft_close,
            now_tick: 1,
            server_index: 0,
        },
        &crate::guild_recipients(&online, &world, &HashMap::new()),
    );
    assert_eq!(out, DisconnectOutcome::default());
}

// ---------------------------------------------------------------------------
// 生产锚点（A1-A5）——只扫代码、剥注释（`//` 前缀改变 trim 结果 ⇒ 注释行不命中）
// ---------------------------------------------------------------------------

const MAIN_SRC: &str = include_str!("../lib.rs");
const RUNTIME_SRC: &str = include_str!("../guild_runtime.rs");

fn lines(src: &str) -> Vec<&str> {
    src.lines().collect()
}

fn exact(src: &[&str], needle: &str) -> Vec<usize> {
    src.iter()
        .enumerate()
        .filter(|(_, l)| l.trim() == needle)
        .map(|(i, _)| i)
        .collect()
}

fn after(src: &[&str], from: usize, needle: &str) -> Option<usize> {
    (from..src.len()).find(|&i| src[i].trim() == needle)
}

fn block_end(src: &[&str], start: usize) -> usize {
    (start + 1..src.len())
        .find(|&i| src[i] == "}")
        .unwrap_or(src.len() - 1)
}

fn count_contains(src: &[&str], needle: &str) -> usize {
    src.iter().filter(|l| l.contains(needle)).count()
}

fn after_contains(src: &[&str], from: usize, needle: &str) -> Option<usize> {
    (from..src.len()).find(|&i| src[i].contains(needle))
}

#[test]
fn g3_anchor_a1_four_disconnect_calls_and_order() {
    let l = lines(MAIN_SRC);
    for needle in [
        "let guild_closed = guild_runtime::on_disconnect(",
        "let guild_kick = guild_runtime::on_disconnect(",
        "let guild_soft = guild_runtime::on_disconnect(",
        "let guild_softcmd = guild_runtime::on_disconnect(",
    ] {
        assert_eq!(exact(&l, needle).len(), 1, "锚点行必须唯一: {needle}");
    }
    let closed = exact(&l, "let guild_closed = guild_runtime::on_disconnect(")[0];
    let arm = (0..closed)
        .rev()
        .find(|&i| l[i].trim() == "GateEvent::Closed {")
        .expect("Closed 臂");
    let deal = after(&l, closed, "deal_cancel_between(").expect("arm 内 deal_cancel");
    assert!(closed < deal, "104 必须先于 deal_cancel_between（N-10）");
    assert!(after(&l, closed, "online_take(&mut online, &chr_name);").is_some());
    assert!(after(&l, closed, "ground_sessions.remove(&chr_name);").is_some());
    let _ = arm;

    let kick_fn = l
        .iter()
        .position(|x| x.trim() == "fn drain_kicks(")
        .unwrap();
    let soft_fn = l
        .iter()
        .position(|x| x.trim() == "fn drain_soft_close(")
        .unwrap();
    let kick = exact(&l, "let guild_kick = guild_runtime::on_disconnect(")[0];
    let soft = exact(&l, "let guild_soft = guild_runtime::on_disconnect(")[0];
    assert!(kick_fn < kick && kick < soft_fn);
    assert!(soft_fn < soft);
    assert!(after_contains(&l, kick_fn, "online_take(online, &chr)").unwrap() < kick);
    assert!(after_contains(&l, kick, "world.disconnect(&chr)").unwrap() > kick);
    assert!(after(&l, kick, "ground_sessions.remove(&chr);").unwrap() > kick);
    assert!(after_contains(&l, soft_fn, "online_take(online, &chr)").unwrap() < soft);
    assert!(after_contains(&l, soft, "world.disconnect(&chr)").unwrap() > soft);
    assert!(after(&l, soft, "ground_sessions.remove(&chr);").unwrap() > soft);

    let sc_fn = l
        .iter()
        .position(|x| x.trim() == "fn handle_soft_close_cmd(")
        .unwrap();
    let sc_end = block_end(&l, sc_fn);
    let sc = exact(&l, "let guild_softcmd = guild_runtime::on_disconnect(")[0];
    assert!(sc_fn < sc && sc < sc_end);
    let sc_call = after_contains(&l, sc_fn, "world.soft_close(chr_name)").unwrap();
    assert!(sc_call < sc, "guild_softcmd 在 world.soft_close 之后");
}

#[test]
fn g3_anchor_a2_exclude_line() {
    let l = lines(RUNTIME_SRC);
    let ex = exact(&l, "if exclude == Some(m.as_str()) {");
    assert_eq!(ex.len(), 1);
    let cont = after(&l, ex[0], "continue;").expect("排除后 continue");
    assert!(cont - ex[0] <= 2);
}

#[test]
fn g3_anchor_a3_recipients_closure() {
    let l = lines(MAIN_SRC);
    let defs = exact(&l, "fn guild_recipients<'a>(");
    assert_eq!(defs.len(), 1, "定义行唯一");
    let start = defs[0];
    let end = block_end(&l, start);
    let body = l[start..=end].join("\n");
    assert!(body.contains("ban: &'a std::collections::HashMap<String, bool>"));
    assert!(body.contains("guild_cmd::find_play_object(online, world, name)?"));
    assert!(
        !body.contains("ground_sessions"),
        "闭包不得捕获 ground_sessions"
    );
    assert_eq!(
        exact(
            &l,
            "let guild_rcpt = guild_recipients(&online, &world, &guild_ban);"
        )
        .len(),
        4,
        "C-2 四处快照"
    );
}

#[test]
fn g3_anchor_a4_production_wiring() {
    let l = lines(MAIN_SRC);
    assert_eq!(count_contains(&l, "guild_runtime::startup("), 1);
    let startup = (0..l.len())
        .find(|&i| l[i].contains("guild_runtime::startup("))
        .unwrap();
    let sblk = l[startup..startup + 8].join("\n");
    assert!(sblk.contains("guild_store"));
    assert!(sblk.contains("guild_sections"));

    assert_eq!(
        exact(&l, "let mut guild_run_tick = world.clock_ms as i32;").len(),
        1
    );

    assert_eq!(count_contains(&l, "guild_runtime::step("), 1);
    let step = (0..l.len())
        .find(|&i| l[i].contains("guild_runtime::step("))
        .unwrap();
    let stepblk = l[step..step + 16].join("\n");
    assert!(stepblk.contains("&mut guild_run_tick,"));

    assert_eq!(count_contains(&l, "guild_cmd::handle_guild_cmd("), 1);
    let cmd = (0..l.len())
        .find(|&i| l[i].contains("guild_cmd::handle_guild_cmd("))
        .unwrap();
    let cend = (cmd..l.len()).find(|&i| l[i].trim() == ");").unwrap();
    let cblk = l[cmd..=cend].join("\n");
    assert!(cblk.contains("world.clock_ms as i32,"));
    assert!(!cblk.contains("now_ms(),"));
}

#[test]
fn g3_anchor_a5_prepare_closed_guild_and_flags() {
    let l = lines(MAIN_SRC);
    let fnl = exact(&l, "fn prepare_closed_guild(");
    assert_eq!(fnl.len(), 1);
    let end = block_end(&l, fnl[0]);
    let gate = exact(
        &l,
        "if !closed_event_is_current(online, chr_name, gate_idx, socket) {",
    );
    assert_eq!(gate.len(), 1);
    assert!((fnl[0]..end).contains(&gate[0]));
    assert!(
        l[gate[0] + 1..=gate[0] + 3]
            .iter()
            .any(|x| x.trim() == "return false;"),
        "早退必须在门内 3 行"
    );
    let set_in_prepare = (fnl[0]..end)
        .find(|&i| l[i].trim() == "sg.bo_soft_close = true;")
        .expect("prepare 内的置位");
    assert!(gate[0] < set_in_prepare, "置位必须在早退之后");
    let ret_true = after(&l, set_in_prepare, "return true;").unwrap();
    assert!(ret_true > set_in_prepare);

    // ② drain_soft_close：两处置位各 1，且在 guild_soft 之前。
    let soft_fn = exact(&l, "fn drain_soft_close(")[0];
    let soft_end = block_end(&l, soft_fn);
    let in_soft = |needle: &str| {
        (soft_fn..soft_end)
            .filter(|&i| l[i].trim() == needle)
            .collect::<Vec<_>>()
    };
    let b1 = in_soft("sg.bo_soft_close = true;");
    let b2 = in_soft("sg.bo_emergency_close = true;");
    assert_eq!(b1.len(), 1);
    assert_eq!(b2.len(), 1);
    let gs = exact(&l, "let guild_soft = guild_runtime::on_disconnect(")[0];
    assert!(b1[0] < gs && b2[0] < gs);

    // ③ 臂内调用行与位置。
    // rustfmt（max_width=100）会把该调用折成多行；按折叠空白后的 token 串逐字匹配，
    // token 序列与 v9 §A5③ 的锚点相同（改参数/改函数名 ⇒ 必红）。
    let calls: Vec<usize> = l
        .iter()
        .enumerate()
        .filter(|(_, x)| x.trim_start().starts_with("if !prepare_closed_guild("))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(calls.len(), 1, "Closed 臂内调用唯一");
    let c = calls[0];
    let bend = (c..=c + 12)
        .find(|&i| l[i].trim() == ") {")
        .expect("调用闭合行");
    let normalized: String = l[c..=bend]
        .join(" ")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace(",)", ")");
    let expected: String =
        "if !prepare_closed_guild(&mut ground_sessions, &online, &chr_name, index, socket) {"
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
    assert_eq!(normalized, expected);
    assert!(
        l[bend + 1..=bend + 3]
            .iter()
            .any(|x| x.trim() == "continue;"),
        "整臂早退必须紧随"
    );
    let del = (0..c)
        .rev()
        .find(|&i| l[i].trim() == "engine.delete_human(socket);")
        .unwrap();
    let kickret = (0..c)
        .rev()
        .find(|&i| l[i].trim() == "kick_pending.retain(|n| n != &chr_name);")
        .unwrap();
    let deal = after(&l, c, "deal_cancel_between(").unwrap();
    assert!(del < c && kickret < c && c < deal);

    // ④ 实参局部名 + 同臂作用域（`drain_soft_close` 也快照同名局部量，故只要求在 Closed 臂内）。
    let letflags = exact(&l, "let (bo_reconnection, bo_soft_close) = ground_sessions");
    assert!(!letflags.is_empty(), "Closed 臂必须先快照 flags");
    let closed = exact(&l, "let guild_closed = guild_runtime::on_disconnect(")[0];
    let arm_start = (0..closed)
        .rev()
        .find(|&i| l[i].trim() == "GateEvent::Closed {")
        .unwrap();
    assert!(
        letflags.iter().any(|&i| arm_start < i && i < closed),
        "同臂作用域内、guild_closed 之前"
    );
    let arm = &l[arm_start..deal];
    assert!(
        arm.iter()
            .any(|x| x.trim() == "bo_reconnection: bo_reconnection,"),
        "bo_reconnection 实参必须是局部名"
    );
    assert!(
        arm.iter()
            .any(|x| x.trim() == "bo_soft_close: bo_soft_close,"),
        "bo_soft_close 实参必须是局部名"
    );
    assert!(
        !arm.iter().any(|x| {
            let t = x.trim();
            t == "bo_reconnection: false," || t == "bo_soft_close: false,"
        }),
        "不得写死 false"
    );
}
