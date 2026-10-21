//! T-138 判据：`@requestcastlewarnow<n>`（oracle `GuildOfficial.ReQuestCastleWar`，
//! `src/GameSrv/Npc/GuildOfficial.cs:206-239`）。
//!
//! 全部经**生产入口** [`super::handle_select`]（`CM_MERCHANTDLGSELECT` 命中固定 NPC 的
//! `GuildOfficial.UserSelect` 臂），不直接调 `request_castle_war`（`str_to_int` /
//! `castle_war_index` 另有直调单测钉 .NET `int.TryParse` 语义）。
//!
//! **oracle 怪行为（照搬）**：成功支 `Castle.AddAttackerInfo(MyGuild)` 的接收者静态类型是
//! `IUserCastle` ⇒ 绑定 `UserCastle.AddAttackerInfo(IGuild)` 的 `throw new NotImplementedException()`
//! （`UserCastle.cs:670-673`），被 `UserSelect` 的 catch 吞掉 ⇒ **零帧、零扣件、零战表、零落盘**。
//! `t138_master_with_piece_hits_oracle_n1_exception` 是这条的牙：谁把它「修」成真登记即红。

use std::collections::HashMap;

use game_svr::ability::StdItemCatalog;
use game_svr::bag::BagItem;
use game_svr::downlink::{self, DisplayTable};
use game_svr::npc::{SayMsg, SendMsg};
use mir2_data::guild::GuildFileSections;
use mir2_protocol::messages;
use mir2_world::castle::{CastleSiegeConfig, GuildId, UserCastle};
use tokio::sync::mpsc;

use crate::guild_runtime::GuildMsgCfg;
use crate::{OnlineRegistry, SessionGround, ShopSession, SimToGate};

use super::FixedNpcScript;

/// 本卡只判「标签派发 + 副作用」，脚本说词由 T-137 的判据覆盖 ⇒ 空脚本桩。
struct EmptyScript;

impl FixedNpcScript for EmptyScript {
    fn goto_label(
        &mut self,
        _sg: &mut SessionGround,
        _ss: &mut ShopSession,
        _npc_actor_id: i32,
        _label: &str,
        _bo_ext_jmp: bool,
    ) -> (Vec<SayMsg>, Vec<SendMsg>) {
        (Vec::new(), Vec::new())
    }
}

struct Fixture {
    world: mir2_world::World,
    npc_id: i32,
    online: OnlineRegistry,
    sessions: HashMap<String, SessionGround>,
    shop_sessions: HashMap<String, ShopSession>,
    displays: DisplayTable,
    guilds: mir2_world::GuildManager,
    guild_cfg: mir2_world::GuildConfig,
    guild_msg_cfg: GuildMsgCfg,
    castle_cfg: mir2_data::guild_castle::GuildCastleConfig,
    sections: GuildFileSections,
    catalog: StdItemCatalog,
    gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    rx: mpsc::UnboundedReceiver<SimToGate>,
}

impl Fixture {
    fn new() -> Self {
        let world = mir2_world::World::new(32, 32);
        let (tx, rx) = mpsc::unbounded_channel::<SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
        gates.insert(0, tx);
        let mut f = Self {
            world,
            npc_id: 0,
            online: Vec::new(),
            sessions: HashMap::new(),
            shop_sessions: HashMap::new(),
            displays: DisplayTable::new(),
            guilds: mir2_world::GuildManager::new(),
            guild_cfg: mir2_world::GuildConfig::default(),
            guild_msg_cfg: GuildMsgCfg {
                show_pre_fix_msg: false,
                prefix: String::new(),
                f_color: 0xDB,
                b_color: 0xFF,
            },
            castle_cfg: mir2_data::guild_castle::GuildCastleConfig::default(),
            sections: GuildFileSections::oracle_defaults(),
            catalog: StdItemCatalog::new(),
            gates,
            rx,
        };
        let zuma = f.castle_cfg.zuma_piece.clone();
        f.catalog.insert(
            42,
            mir2_data::StdItem {
                name: zuma,
                ..mir2_data::StdItem::default()
            },
        );
        f.add_player("hero", 1, 10, 10, 901);
        f.npc_id = f
            .world
            .spawn_npc(mir2_world::npc::NpcSpawn {
                name: "比奇国王".to_string(),
                x: 11,
                y: 10,
                map_id: 0,
                state: mir2_world::npc::NpcState {
                    npc_type: mir2_world::npc::NPC_TYPE_GUILD_OFFICIAL,
                    ..Default::default()
                },
            })
            .expect("固定 NPC 入世界");
        // 两座城：下标 0 / 1（判「下标解析」用）。
        for name in ["沙巴克", "沙巴克二"] {
            f.world
                .castles
                .push(UserCastle::new(name, CastleSiegeConfig::default()));
        }
        // `LableIsCanJmp`（`PlayObject.Base.cs:2814-2828`）第二支：说词里的
        // `<城名/@requestcastlewarnow{i}>` 会进 `CanJmpScriptLableMap`（`GetVariableText`，
        // `GuildOfficial.cs:24-37`）。夹具直接放行本卡用到的标签（含大小写变体）。
        f.shop_sessions
            .entry("hero".to_string())
            .or_default()
            .can_jmp_labels = [
            "@requestcastlewarnow",
            "@requestcastlewarnow0",
            "@requestcastlewarnow1",
            "@requestcastlewarnowabc",
            "@requestcastlewarnow-1",
            "@requestcastlewarnow 1",
            "@requestcastlewarnow+1",
            "@requestcastlewarnow9",
            "@REQUESTCASTLEWARNOW1",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        f
    }

    fn add_player(&mut self, name: &str, actor_id: i32, x: i32, y: i32, socket: i32) {
        let mut e = mir2_world::Entity::new(actor_id, name, x, y);
        e.race = mir2_world::ACTOR_RACE_PLAY;
        self.world.add_entity(e);
        let sg = crate::tests::test_session_ground(actor_id);
        self.sessions.insert(name.to_string(), sg);
        self.displays.set(
            actor_id,
            downlink::ActorDisplay::new(0, 0, 0, 0, 0, 255, name.as_bytes().to_vec()),
        );
        self.online.push(crate::OnlineEntry {
            account: format!("u-{name}"),
            chr_name: name.to_string(),
            session_id: actor_id,
            gate_idx: 0,
            socket,
            actor_id,
        });
    }

    /// `PlayObject.MyGuild` / `GuildRankNo`（`IsGuildMaster` 的两项，`PlayObject.cs:4402-4405`）。
    fn set_guild(&mut self, name: Option<&str>, rank_no: i16) {
        let sg = self.sessions.get_mut("hero").unwrap();
        sg.guild.guild_name = name.map(str::to_string);
        sg.guild.rank_no = rank_no;
        sg.guild.rank_name = if rank_no == 1 {
            "掌门人".to_string()
        } else {
            "行会成员".to_string()
        };
    }

    /// 把第 `idx` 座城挂成 `guild` 的城（`MasterGuild` + `OwnGuild`；`IsMember` 的数据面）。
    fn set_member_castle(&mut self, idx: usize, guild: &str) {
        let c = &mut self.world.castles[idx];
        c.master_guild = Some(GuildId(1));
        c.own_guild = Some(guild.to_string());
    }

    fn give_zuma_piece(&mut self) {
        self.sessions
            .get_mut("hero")
            .unwrap()
            .state
            .item_list
            .push(BagItem {
                make_index: 77,
                index: 42,
                dura: 10,
                dura_max: 10,
                desc: [0; 14],
            });
    }

    fn bag_len(&self) -> usize {
        self.sessions["hero"].state.item_list.len()
    }

    fn call(&mut self, tail: &str) {
        let mut script = EmptyScript;
        super::handle_select(
            self.npc_id,
            Some(tail),
            "hero",
            901,
            &self.online,
            &mut self.sessions,
            &mut self.shop_sessions,
            &self.displays,
            &mut self.world,
            &mut self.guilds,
            None,
            &self.sections,
            &self.guild_cfg,
            &self.guild_msg_cfg,
            &self.castle_cfg,
            0,
            12,
            (255, 56),
            &self.gates,
            &self.catalog,
            1_000,
            &mut script,
        );
    }

    fn drain(&mut self) -> Vec<(i32, Vec<u8>)> {
        let mut out = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            if let SimToGate::SendFrame { socket, body } = ev {
                out.push((socket, body));
            }
        }
        out
    }
}

fn ident(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[4], b[5]])
}

fn recog(b: &[u8]) -> i32 {
    i32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// `EDCode.EncodeString(GBK)` 的逆：6-bit 解码 → GBK 文本。
fn tail_text(b: &[u8]) -> String {
    let raw = mir2_protocol::edcode::decode(&b[12..]);
    let (text, _, _) = encoding_rs::GBK.decode(&raw);
    text.into_owned()
}

/// 英雄自己收到的 `SM_SYSMESSAGE` 说词（按出帧序，含 `Recog`）。
fn sys_msgs(frames: &[(i32, Vec<u8>)]) -> Vec<(i32, String)> {
    frames
        .iter()
        .filter(|(s, b)| *s == 901 && ident(b) == messages::SM_SYSMESSAGE)
        .map(|(_, b)| (recog(b), tail_text(b)))
        .collect()
}

/// 判据 1：**非会长**（无行会 / 有会但 rank != 1）⇒ 单条「你的请求被取消!!!」，零写入。
///
/// 牙：删掉 `request_castle_war` 的 `!is_guild_master` 早退（把非掌门当掌门）⇒ 两支都变成
/// 「你没有祖玛头像!!!」或成功支零帧 ⇒ 说词断言红。
#[test]
fn t138_non_master_is_cancelled() {
    // ① 无行会。
    let mut f = Fixture::new();
    f.set_guild(None, 0);
    f.call("@requestcastlewarnow0");
    let msgs = sys_msgs(&f.drain());
    assert_eq!(msgs, vec![(1, "你的请求被取消!!!".to_string())]);
    assert_eq!(f.bag_len(), 0, "非会长不扣件");
    assert!(f.world.castles.iter().all(|c| c.attack_war_list.is_empty()));

    // ② 有行会但 rank != 1（`IsGuildMaster` 的第二项）。
    let mut f = Fixture::new();
    f.set_guild(Some("A"), 99);
    f.call("@requestcastlewarnow0");
    let msgs = sys_msgs(&f.drain());
    assert_eq!(msgs, vec![(1, "你的请求被取消!!!".to_string())]);
    assert_eq!(f.bag_len(), 0);
    assert!(f.world.castles.iter().all(|c| c.attack_war_list.is_empty()));
}

/// 判据 2：**已是成员**（`Castle.IsMember(PlayObject)` 真）⇒ 同样「你的请求被取消!!!」+ 零写入。
///
/// 牙：去掉 `castle.is_member_name(...)` 门 ⇒ 变成「你没有祖玛头像!!!」⇒ 红。
#[test]
fn t138_member_is_cancelled() {
    let mut f = Fixture::new();
    f.set_guild(Some("A"), 1);
    f.set_member_castle(0, "A");
    f.give_zuma_piece();
    f.call("@requestcastlewarnow0");
    let msgs = sys_msgs(&f.drain());
    assert_eq!(msgs, vec![(1, "你的请求被取消!!!".to_string())]);
    assert_eq!(f.bag_len(), 1, "成员申请不扣件");
    assert!(f.world.castles[0].attack_war_list.is_empty());
}

/// 判据 3：**掌门 + 非成员 + 无申请令** ⇒ 「你没有祖玛头像!!!」（`CheckItems(ZumaPiece)` 空），
/// 零写入。
///
/// 牙：把 `CheckItems` 判空写反 / 改成恒 Some ⇒ 说词变成功支（零帧）⇒ 红。
#[test]
fn t138_without_zuma_piece_reports_missing() {
    let mut f = Fixture::new();
    f.set_guild(Some("A"), 1);
    let zuma = f.castle_cfg.zuma_piece.clone();
    f.call("@requestcastlewarnow1");
    let msgs = sys_msgs(&f.drain());
    assert_eq!(msgs, vec![(1, format!("你没有{zuma}!!!"))]);
    assert_eq!(f.bag_len(), 0);
    assert!(f.world.castles.iter().all(|c| c.attack_war_list.is_empty()));
}

/// 判据 4（**N1 牙**）：掌门 + 非成员 + **有申请令** ⇒ oracle 在
/// `Castle.AddAttackerInfo(MyGuild)` 抛 `NotImplementedException`（`UserCastle.cs:670-673`，
/// 接口静态类型绑定），被 `UserSelect` 的 catch 吞 ⇒ **零帧**、物品**不**扣、`AttackWarList`
/// **不**登记、无 `SaveAttackSabukWall`、无 SS_212、无 `~@request_ok`。
///
/// 牙：把这个分支改成调 `castle.add_attacker_info(...)`（「修 bug」）⇒ 物品被删 + 战表非空 ⇒ 红；
/// 改成给 `else` 支的「你现在无法请求攻城!!!」⇒ 说词断言红。
#[test]
fn t138_master_with_piece_hits_oracle_n1_exception() {
    let mut f = Fixture::new();
    f.set_guild(Some("A"), 1);
    f.give_zuma_piece();
    f.call("@requestcastlewarnow1");
    let frames = f.drain();
    assert!(
        frames.is_empty(),
        "oracle N1：成功支被 catch 吞掉，客户端零帧（实收 {frames:?}）"
    );
    assert_eq!(f.bag_len(), 1, "照搬 oracle：申请令不扣");
    assert_eq!(
        f.sessions["hero"].state.item_list[0].make_index, 77,
        "仍是同一件"
    );
    assert!(
        f.world.castles.iter().all(|c| c.attack_war_list.is_empty()),
        "照搬 oracle：AttackWarList 不登记"
    );
}

/// 判据 5：**下标解析**（`StrToInt(sIndex,-1)` + `<0` 归 0 + `GetCastle` 越界）。
///
/// 城 0 = 玩家行会 A 的城（成员 ⇒ 「取消」），城 1 = 无主（掌门 ⇒ 「没有祖玛头像!!!」）⇒
/// 说词本身就是下标解析的可观测读数。
///
/// 牙：`castle_war_index` 恒返 0 ⇒ `…1`/`… 1`/`…+1` 三条红；去掉 `<0` 归 0 ⇒ `…-1` 红
/// （越界 ⇒ NRE ⇒ 零帧）；`str_to_int` 去掉 `trim()` ⇒ `… 1` 红（Rust `parse` 不认首尾空白）。
#[test]
fn t138_index_parsing_through_production_path() {
    fn one(tail: &str) -> Vec<String> {
        let mut f = Fixture::new();
        f.set_guild(Some("A"), 1);
        f.set_member_castle(0, "A");
        f.call(tail);
        sys_msgs(&f.drain()).into_iter().map(|(_, t)| t).collect()
    }
    let cancel = vec!["你的请求被取消!!!".to_string()];
    let missing = vec!["你没有祖玛头像!!!".to_string()];

    assert_eq!(
        one("@requestcastlewarnow"),
        cancel,
        "空后缀 = StrToInt 失败 ⇒ -1 ⇒ 0"
    );
    assert_eq!(one("@requestcastlewarnow0"), cancel, "显式 0");
    assert_eq!(one("@requestcastlewarnow1"), missing, "显式 1 命中第二座城");
    assert_eq!(one("@requestcastlewarnowabc"), cancel, "非数字 ⇒ -1 ⇒ 0");
    assert_eq!(one("@requestcastlewarnow-1"), cancel, "负数 ⇒ 归 0");
    assert_eq!(
        one("@requestcastlewarnow 1"),
        missing,
        "首尾空白（int.TryParse AllowLeadingWhite）"
    );
    assert_eq!(
        one("@requestcastlewarnow+1"),
        missing,
        "前导 +（AllowLeadingSign）"
    );
    assert_eq!(
        one("@REQUESTCASTLEWARNOW1"),
        missing,
        "CompareLStr 前缀匹配大小写不敏感"
    );
    // 越界 + 掌门 ⇒ `Castle == null` ⇒ `IsMember` NRE 被吞 ⇒ 零帧。
    assert!(one("@requestcastlewarnow9").is_empty(), "越界 ⇒ NRE ⇒ 零帧");
}

/// 判据 5b：越界 + **非掌门** ⇒ 走短路（`IsGuildMaster() && …` 第一项为假）⇒ 仍是「取消」
/// 而不是 NRE 零帧。
#[test]
fn t138_out_of_range_non_master_still_cancelled() {
    let mut f = Fixture::new();
    f.set_guild(None, 0);
    f.call("@requestcastlewarnow9");
    let msgs = sys_msgs(&f.drain());
    assert_eq!(msgs, vec![(1, "你的请求被取消!!!".to_string())]);
}

/// 判据 5c：`StrToInt` / `castle_war_index` 的直调单测（.NET `int.TryParse` 口径）。
#[test]
fn t138_str_to_int_and_index_semantics() {
    assert_eq!(super::str_to_int("3", -1), 3);
    assert_eq!(
        super::str_to_int(" 3 ", -1),
        3,
        "AllowLeadingWhite/TrailingWhite"
    );
    assert_eq!(super::str_to_int("+3", -1), 3, "AllowLeadingSign");
    assert_eq!(super::str_to_int("-3", -1), -3);
    assert_eq!(super::str_to_int("", -1), -1);
    assert_eq!(super::str_to_int("abc", -1), -1);
    assert_eq!(super::str_to_int("3.0", -1), -1, "不接受小数点");
    assert_eq!(
        super::str_to_int("2147483648", -1),
        -1,
        "溢出 ⇒ TryParse false"
    );

    assert_eq!(super::castle_war_index("", 2), Some(0));
    assert_eq!(super::castle_war_index("abc", 2), Some(0));
    assert_eq!(super::castle_war_index("-5", 2), Some(0));
    assert_eq!(super::castle_war_index("1", 2), Some(1));
    assert_eq!(
        super::castle_war_index("2", 2),
        None,
        "越界 ⇒ GetCastle null"
    );
    assert_eq!(super::castle_war_index("0", 0), None, "无城 ⇒ null");
}
