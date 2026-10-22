//! **行会管理 NPC（`GuildOfficial`，比奇国王）的标签派发**（audit #3）。
//!
//! 逐行对齐 oracle `src/GameSrv/Npc/GuildOfficial.cs`：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`handle_select`] | `ClientMerchantDlgSelect`（`PlayObject.Operate.cs:114-128`）→ `GuildOfficial.UserSelect`（`:55-90`）|
//! | [`request_build_guild`] | `GuildOfficial.ReQuestBuildGuild`（`:112-178`）|
//! | [`war_select`] / [`war_events_to_frames`] | `GuildOfficial.ReQuestGuildWar`（`:180-199`）+ `PlayObject.ReQuestGuildWar`（`PlayObject.Base.cs:3349-3385`）|
//! | `@@donate` / `@exit` / `@back` | `GuildOfficial.cs:78-88`、`DoNate`（`:203-206`）|
//! | [`request_castle_war`] | `GuildOfficial.ReQuestCastleWar`（`GuildOfficial.cs:206-239`）|
//!
//! # 帧序（线上可观测序 = oracle `MsgQueue` 的优先级字节）
//!
//! oracle 的 `SendDefMessage` 是**直发**，`SendMsg`/`SendRefMsg` 是**入队**；`MsgQueue` 按
//! `(byte)wIdent` 升序出队（`BaseObject.Message.cs:334` 的 `TryDequeue`）。本模块用
//! [`PRI_CHANGEGUILDNAME`] 等常量固化这个序，`sort_by_key` 是稳定排序 ⇒ 同优先级保持**入队序**：
//!
//! - 建会成功：`202 SM_DELITEM`（直发）→ `750 SM_CHANGEGUILDNAME`(61) →
//!   `762 SM_BUILDGUILD_OK`(63) → `622 SM_WEIGHTCHANGED`(131) → `653 SM_GOLDCHANGED`(152)
//!   → `42 SM_USERNAME`(165)。622 来自 `DelBagItem(MakeIndex, WomaHorn)` 命中后的
//!   `WeightChanged()`（`BaseObject.cs:2238` → `:1771-1775`），优先级 = `(byte)10115`；
//! - 建会失败：只有 `763 SM_BUILDGUILD_FAIL`(63)；
//! - 宣战：`653`(152) → `42 SM_USERNAME`(165) → `100 SM_SYSMESSAGE`(252) →
//!   `104 SM_GUILDMESSAGE`(255)；
//! - `@@donate`：`764 SM_DONATE_OK`(65)；`@exit`：`644 SM_MERCHANTDLGCLOSE`(143)。
//! - `@requestcastlewarnow<n>`：只有两条红字 `100 SM_SYSMESSAGE`(252)（「你的请求被取消!!!」/
//!   「你没有<ZumaPiece>!!!」）；成功支被 oracle 的 N1 异常吞掉 ⇒ **零帧**（见下「已补 / 未做项」）。
//!
//! # 已补 / 未做项（登记见 `tests/parity/whitelist.md` T-137..T-140）
//!
//! - **固定 NPC 的 `Npc_def/<ChrName>-<MapName>` 脚本（T-137 已装载）**：由
//!   `bins/game-svr/src/npcs.rs::place_fixed_npcs_with_scripts` 在倒序挂图后按 oracle 装载，
//!   塞进与商人共用的 `MerchantScriptBook`（键 = 世界实体 id）。本模块的 `handle_select` 在
//!   `LableIsCanJmp` 之后、行会动作派发之前调用**统一入口** `ScriptEngine.GotoLable`
//!   （`GuildOfficial.cs:70`；实现由宿主以 [`FixedNpcScript`] 注入）⇒ `@main` 菜单说词落
//!   `SM_MERCHANTSAY`、说词里的 `@@buildguildnow`/`@@guildwar` 进 `CanJmpScriptLableMap`。
//!   行会动作本身仍是脚本标签之后的**分支**（`GuildOfficial.cs:71-89`），不是标签入口。
//! - `@@requestcastlewarnow<n>`（T-138 已接）：`GuildOfficial.cs:206-239` 的 `ReQuestCastleWar`
//!   逐行移植（下标解析 / 掌门+非成员门 / `CheckItems(ZumaPiece)` / 两条红字）。**照搬 oracle 的
//!   N1 异常**（白名单 T-38/T-138）：`Castle` 的静态类型是 `IUserCastle`（`:210`）⇒
//!   `Castle.AddAttackerInfo(PlayObject.MyGuild)`（`:219`）绑定接口方法
//!   `IUserCastle.AddAttackerInfo(IGuild)`（`IUserCastle.cs:125`），实现是
//!   `UserCastle.AddAttackerInfo(IGuild)` 的 `throw new NotImplementedException()`
//!   （`UserCastle.cs:670-673`），**不是** `:998` 的 `AddAttackerInfo(GuildInfo)`；异常被
//!   `UserSelect` 的 catch（`:100-103`）吞掉 ⇒ 会长+有令的成功支**零帧、零扣件、零战表、零落盘、
//!   零 SS_212**（`crates/world/src/castle.rs` 的 `add_attacker_info_n1` 早已把 `GuildOfficial.cs:219`
//!   登记为该 N1 调用点）。判据 `t138_*` 钉死这个怪行为：谁把它「修」成真登记，用例即红。
//! - 固定 NPC 按 `Npcs.txt` 的 `type` 分流（`LocalDB.cs:688-698`）：只有 `1` =
//!   `GuildOfficial` 进入本模块标签集；`0` = `Merchant` 的脚本半由 `handle_shop_cmd` 的
//!   固定 NPC 臂负责（T-137）；`2` = `CastleOfficial` 的 `Click`/`UserSelect` 门未移植
//!   （T-140/T-40，见 whitelist T-137 残余①）⇒ 由宿主两条臂的类型门
//!   `fixed_npc_script_allowed` 拦下，本模块对二者零帧零副作用。
//! - `GetShowName()` 的 42 尾用进图时展示表（`guildname` 恒空）；oracle 在 `RefShowName`
//!   时重算（`ShowGuildName`/攻城区分支）。部署件 `ShowGuildName=0` 且 NPC 不在攻城区 ⇒ 等价。

use std::collections::HashMap;

use game_svr::ability::ItemCatalog;
use game_svr::bag::BagItem;
use game_svr::downlink::{self, DisplayTable};
use game_svr::shop::eq_label;
use game_svr::smframe;
use mir2_data::guild::{GuildFileSections, GuildStore};
use mir2_data::guild_castle::GuildCastleConfig;
use mir2_protocol::messages;
use mir2_world::{GuildConfig, GuildManager, GuildWarEvent, GuildWarRequest};
use tokio::sync::mpsc::UnboundedSender;

use crate::guild_cmd::GuildSession;
use crate::guild_frames;
use crate::guild_runtime::{self, GuildMsgCfg};
use crate::{glog, OnlineRegistry, SessionGround, ShopSession, SimToGate};

/// `ScriptFlagConst.sBUILDGUILDNOW`（`src/Modules/SystemModule/Const/ScriptFlagConst.cs:68`）。
pub(crate) const LABEL_BUILD_GUILD_NOW: &str = "@@buildguildnow";
/// `ScriptFlagConst.sSCL_GUILDWAR`（`:69`）。
pub(crate) const LABEL_GUILD_WAR: &str = "@@guildwar";
/// `ScriptFlagConst.sDONATE`（`:70`）。
pub(crate) const LABEL_DONATE: &str = "@@donate";
/// `ScriptFlagConst.sEXIT`（`:60`）。
pub(crate) const LABEL_EXIT: &str = "@exit";
/// `ScriptFlagConst.sBACK`（`:61`）。
pub(crate) const LABEL_BACK: &str = "@back";
/// `ScriptFlagConst.sMAIN`（`:62`）。
pub(crate) const LABEL_MAIN: &str = "@main";
/// `ScriptFlagConst.sREQUESTCASTLEWAR`（`ScriptFlagConst.cs:71`）。
pub(crate) const LABEL_REQUEST_CASTLE_WAR: &str = "@requestcastlewarnow";

/// `MsgQueue` 出队优先级 = oracle `(byte)wIdent`（`BaseObject.Message.cs:334`）。
const fn pri(rm_ident: u16) -> u8 {
    rm_ident as u8
}
const PRI_CHANGEGUILDNAME: u8 = pri(messages::RM_CHANGEGUILDNAME);
const PRI_BUILDGUILD_OK: u8 = pri(messages::RM_BUILDGUILD_OK);
const PRI_BUILDGUILD_FAIL: u8 = pri(messages::RM_BUILDGUILD_FAIL);
const PRI_GOLDCHANGED: u8 = pri(messages::RM_GOLDCHANGED);
const PRI_USERNAME: u8 = pri(messages::RM_USERNAME);
const PRI_SYSMESSAGE: u8 = pri(messages::RM_SYSMESSAGE);
const PRI_GUILDMESSAGE: u8 = pri(messages::RM_GUILDMESSAGE);
const PRI_DONATE_OK: u8 = pri(messages::RM_DONATE_OK);
const PRI_MERCHANTDLGCLOSE: u8 = pri(messages::RM_MERCHANTDLGCLOSE);
const PRI_WEIGHTCHANGED: u8 = pri(messages::RM_WEIGHTCHANGED);

/// `HUtil32.GetValidStr3(sData, ref sLabel, '\r')` 的**首段**（`GuildOfficial.cs:67`）。
///
/// `sData` 已由 `ClientMerchantDlgSelect` 的 `sMsg.Trim()` 去掉首尾空白。
#[must_use]
pub(crate) fn split_label(s_data: &str) -> (&str, &str) {
    s_data.split_once('\r').unwrap_or((s_data, ""))
}

/// `BaseObject.CheckItems(sItemName)`（`BaseObject.cs:2189-2205`）：`ItemList` **正序**首个
/// `GetStdItemName(Index)` 大小写不敏感命中的物品。
#[must_use]
pub(crate) fn check_items(
    items: &[BagItem],
    catalog: &dyn ItemCatalog,
    item_name: &str,
) -> Option<BagItem> {
    items
        .iter()
        .find(|it| {
            catalog
                .name(it.index)
                .is_some_and(|n| eq_label(n, item_name))
        })
        .cloned()
}

/// `BaseObject.DelBagItem(int nItemIndex, string sItemName)`（`BaseObject.cs:2223-2244`）：
/// 首个 `MakeIndex` 与名字**同时**命中的物品移除；命中才 `WeightChanged()`
/// （`BaseObject.cs:2238` ⇒ `:1771-1775`：`WAbil.Weight = RecalcBagWeight()` + 入队
/// `RM_WEIGHTCHANGED`）。调用方在返回 `true` 时产 `SM_WEIGHTCHANGED(622)`。
pub(crate) fn del_bag_item(
    state: &mut game_svr::ground::GroundState,
    make_index: i32,
    item_name: &str,
    catalog: &dyn ItemCatalog,
) -> bool {
    let Some(pos) = state.item_list.iter().position(|it| {
        it.make_index == make_index
            && catalog
                .name(it.index)
                .is_some_and(|n| eq_label(n, item_name))
    }) else {
        return false;
    };
    state.item_list.remove(pos);
    // `WeightChanged()`：`WAbil.Weight = RecalcBagWeight()`（`BaseObject.cs:1356`）。
    state.wabil.weight = game_svr::ground::recalc_bag_weight(catalog, &state.item_list);
    true
}

/// `ReQuestBuildGuild` 的输入（`GuildOfficial.cs:112-178`）。
pub(crate) struct BuildGuildRequest<'a> {
    /// `sGuildName`（`GetValidStr3` 的余串；本函数内 `Trim`）。
    pub guild_name: &'a str,
    /// `PlayObject.ChrName`。
    pub chief: &'a str,
    /// `PlayObject.MyGuild != null`。
    pub has_guild: bool,
    /// `PlayObject.Gold`。
    pub gold: i32,
    /// `SystemShare.Config.BuildGuildPrice`。
    pub price: i32,
    /// `SystemShare.Config.WomaHorn`。
    pub woma_horn: &'a str,
    /// `PlayObject.ItemList`。
    pub items: &'a [BagItem],
    pub catalog: &'a dyn ItemCatalog,
    pub cfg: &'a GuildConfig,
}

/// `ReQuestBuildGuild` 的产出（`result` = oracle 返回值，同时是 `RM_BUILDGUILD_FAIL` 的 `nParam1`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BuildGuildOutcome {
    /// `result`（`0` = 成功；`-1` 已有行会 / `-2` 缺费用 / `-3` 缺物品 / `-4` 名字非法或重名）。
    pub result: i32,
    /// `GuildMgr.AddGuild` 返回 true。
    pub added: bool,
    /// `GuildMgr.MemberOfGuild(ChrName)`。
    pub my_guild: Option<String>,
    /// `MyGuild.GetRankName(PlayObject)`。
    pub rank: Option<(i16, String)>,
    /// `CheckItems(WomaHorn)` 命中的物品（成功时随后 `SendDelItems` + `DelBagItem`）。
    pub removed: Option<BagItem>,
    /// `DecGold(BuildGuildPrice)` 的扣减额（成功 = price）。
    pub gold_spent: i32,
}

/// `GuildOfficial.ReQuestBuildGuild`（`GuildOfficial.cs:112-178`）的**判定 + 会表副作用**。
///
/// **照搬的 oracle 次序/怪分支**：
/// 1. 空名字先置 `-4`，但**不短路**：`MyGuild == null` 且金/物品不足时会分别被 `-2`/`-3`
///    覆盖，`MyGuild != null` 时被 `-1` 覆盖（`:115-136`）；
/// 2. `AddGuild` 返回 false（重名/`CheckGuildName` 失败）⇒ `-4`；
/// 3. 成功时才 `MemberOfGuild`/`GetRankName`（`:150-156`）。
pub(crate) fn request_build_guild(
    guilds: &mut GuildManager,
    req: &BuildGuildRequest<'_>,
) -> BuildGuildOutcome {
    let guild_name = req.guild_name.trim();
    let mut result = 0;
    let mut item = None;
    if guild_name.is_empty() {
        result = -4;
    }
    if req.has_guild {
        result = -1;
    } else if req.gold >= req.price {
        item = check_items(req.items, req.catalog, req.woma_horn);
        if item.is_none() {
            result = -3;
        }
    } else {
        result = -2;
    }
    let mut outcome = BuildGuildOutcome {
        result,
        ..BuildGuildOutcome::default()
    };
    if result == 0 {
        if guilds.add_guild(guild_name, req.chief, req.cfg) {
            outcome.added = true;
            outcome.my_guild = guilds.member_of_guild(req.chief).map(str::to_string);
            outcome.rank = outcome
                .my_guild
                .as_deref()
                .and_then(|g| guilds.find_guild(g))
                .and_then(|g| g.get_rank_name(req.chief));
            outcome.removed = item;
            outcome.gold_spent = req.price;
        } else {
            // `AddGuild` false ⇒ `result = -4`（`:160`）。
            outcome.result = -4;
        }
    }
    outcome
}

/// `SendDelItems(UserItem)`（`PlayObject.Base.cs:1648-1665`）的 `SM_DELITEM(202)`；模板缺失
/// 时 oracle 整帧不发（`GetStdItem == null` 提前 return）。
#[must_use]
pub(crate) fn delitem_frame(
    actor_id: i32,
    item: &BagItem,
    catalog: &dyn ItemCatalog,
) -> Option<Vec<u8>> {
    let tail = game_svr::trade::del_item_tail(item, catalog)?;
    Some(smframe::sm_delitem(actor_id, &tail))
}

/// `ReQuestBuildGuild` 的**排队帧**（不含 42；42 由调用方按观察者广播）。
///
/// 返回 `(优先级, body)`，已按 oracle `MsgQueue` 出队序排好。
#[must_use]
pub(crate) fn build_queued_frames(
    outcome: &BuildGuildOutcome,
    new_gold: i32,
    game_gold: i32,
) -> Vec<(u8, Vec<u8>)> {
    let mut out: Vec<(u8, Vec<u8>)> = Vec::new();
    if let Some(guild) = &outcome.my_guild {
        let rank = outcome
            .rank
            .as_ref()
            .map(|(_, name)| name.as_str())
            .unwrap_or("");
        out.push((
            PRI_CHANGEGUILDNAME,
            guild_frames::sm_guild_changename_750(Some(guild), rank),
        ));
    }
    if outcome.result >= 0 {
        out.push((PRI_BUILDGUILD_OK, guild_frames::sm_buildguild_ok_762()));
    } else {
        out.push((
            PRI_BUILDGUILD_FAIL,
            guild_frames::sm_buildguild_fail_763(outcome.result),
        ));
    }
    if outcome.added {
        out.push((
            PRI_GOLDCHANGED,
            smframe::sm_goldchanged(new_gold, game_gold),
        ));
    }
    out.sort_by_key(|(p, _)| *p);
    out
}

/// `BaseObject.RefShowName()`（`BaseObject.cs:1174-1177`）的 42 帧（`Recog` = 被展示者）。
#[must_use]
pub(crate) fn show_name_frame(actor_id: i32, display: &downlink::ActorDisplay) -> Vec<u8> {
    smframe::sm_username(actor_id, display.name_color, &display.show_name)
}

/// 一条按 `(优先级, 入队序)` 排序的出帧。
struct QueuedFrame {
    priority: u8,
    seq: usize,
    actor_id: i32,
    body: Vec<u8>,
}

fn push_frame(queue: &mut Vec<QueuedFrame>, priority: u8, actor_id: i32, body: Vec<u8>) {
    let seq = queue.len();
    queue.push(QueuedFrame {
        priority,
        seq,
        actor_id,
        body,
    });
}

/// 把排好序的帧逐条路由到 `[GateIdx, Socket]`。
fn flush(
    queue: &mut [QueuedFrame],
    online: &OnlineRegistry,
    gates: &HashMap<u8, UnboundedSender<SimToGate>>,
) -> usize {
    queue.sort_by_key(|f| (f.priority, f.seq));
    let mut sent = 0usize;
    for f in queue.iter() {
        let Some(t) = online.iter().find(|e| e.actor_id == f.actor_id) else {
            glog!(
                "WRN",
                "行会 NPC 回帧：ActorId={} 不在在线登记表 ⇒ 丢弃 pri={}",
                f.actor_id,
                f.priority
            );
            continue;
        };
        if let Some(tx) = crate::route_to_gate(gates, t.gate_idx, t.socket, "行会 NPC 回帧") {
            let _ = tx.send(SimToGate::SendFrame {
                socket: t.socket,
                body: f.body.clone(),
            });
            sent += 1;
        }
    }
    sent
}

/// 固定 NPC 的脚本入口（宿主注入）：`M2Share.ScriptEngine.GotoLable(PlayObject, ActorId, sLabel, boExtJmp)`
/// 的等价物（`ScriptEngine.cs:38-129`）。
///
/// 返回 `(说词, 脚本帧, 改动的 UseItems 槽)`；`SessionGround`/`ShopSession` 的 seed/写回由实现方
/// 完成（oracle 里它们本就是同一个 `PlayObject` 的字段）。第三项供调用方把 `TAKEW` 改过的佩戴位
/// 同步到世界实体（`Entity.use_items` 是每 tick 回写会话的权威源）。`None` 语义
/// （`ActorMgr.Get(actorId) == null` ⇒ 无说词）在实现方折叠成空 `Vec`。
pub(crate) trait FixedNpcScript {
    fn goto_label(
        &mut self,
        sg: &mut SessionGround,
        ss: &mut ShopSession,
        npc_actor_id: i32,
        label: &str,
        bo_ext_jmp: bool,
    ) -> (
        Vec<game_svr::npc::SayMsg>,
        Vec<game_svr::npc::SendMsg>,
        Vec<usize>,
    );
}

/// `RM_MERCHANTSAY` 在 `MsgQueue` 里的优先级 = `(byte)11009 = 1`（`BaseObject.Message.cs:180`）；
/// `SendPriorityMsg(High)` = 0（`:30`）。
const PRI_MERCHANTSAY: u8 = pri(messages::RM_MERCHANTSAY);

/// 把 `GotoLable` 的说词与脚本帧放进本 handler 的出帧队列。
///
/// oracle 里两者都是**入队**消息（`SendPriorityMsg`/`SendMsg` ⇒ 脚本 `SendMsg`），handler 结束后
/// 由 `MsgQueue` 按 `(byte)wIdent` 出队；[`flush`] 的 `(priority, seq)` 排序即同口径
/// （说词 0/1 先于 `RM_GAMEGOLDCHANGED` 148 等脚本帧）。
fn queue_script_output(
    queue: &mut Vec<QueuedFrame>,
    says: &[game_svr::npc::SayMsg],
    frames: &[game_svr::npc::SendMsg],
    sg: &SessionGround,
    player_actor_id: i32,
    npc_actor_id: i32,
) {
    for s in game_svr::npc::dequeue_order(says) {
        // `ScriptEngine.cs:993/997`：priority 支用玩家（既有金标准），非 priority 支用 NPC ActorId。
        let recog = if s.priority {
            player_actor_id
        } else {
            s.npc_actor_id
        };
        let wire = game_svr::npc::merchant_say_gbk(&s.npc_name, &s.raw_msg);
        let priority = if s.priority { 0 } else { PRI_MERCHANTSAY };
        push_frame(
            queue,
            priority,
            player_actor_id,
            smframe::sm_merchantsay(recog, 0, 0, &wire),
        );
    }
    for f in frames {
        // 未知 ident 与 `emit_script_frames` 同口径丢弃（T-142 ⑧ 残余）。
        let Some(body) = game_svr::pickwire::script_frame(
            f,
            npc_actor_id,
            sg.state.gold,
            sg.game_gold,
            sg.game_point,
            sg.soft_version_date,
        ) else {
            continue;
        };
        push_frame(queue, f.ident as u8, player_actor_id, body);
    }
}

/// `CM_MERCHANTDLGSELECT` 命中**固定 NPC**（`Envir/Npcs.txt`，`npc.is_some()`）时的派发。
///
/// 入口等价 `ClientMerchantDlgSelect`（`PlayObject.Operate.cs:114-128`）的
/// `FindMerchant(nParam1) ?? FindNpc(nParam1)` 第二臂 + `GuildOfficial.UserSelect`。
///
/// 返回 `true` = 命令已被本派发消费（无论是否有帧）；调用方不得再落商店派发。
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_select(
    npc_actor_id: i32,
    tail: Option<&str>,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    sessions: &mut HashMap<String, SessionGround>,
    shop_sessions: &mut HashMap<String, ShopSession>,
    displays: &DisplayTable,
    world: &mut mir2_world::World,
    guilds: &mut GuildManager,
    guild_store: Option<&GuildStore>,
    guild_sections: &GuildFileSections,
    guild_cfg: &GuildConfig,
    guild_msg_cfg: &GuildMsgCfg,
    castle_cfg: &GuildCastleConfig,
    server_index: i32,
    ref_range: i32,
    red_color: (u8, u8),
    gates: &HashMap<u8, UnboundedSender<SimToGate>>,
    catalog: &dyn ItemCatalog,
    now_tick: i32,
    script: &mut dyn FixedNpcScript,
) -> bool {
    let Some(entry) = online.iter().find(|e| e.chr_name == chr_name) else {
        glog!(
            "WRN",
            "NPC 对话：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃"
        );
        return true;
    };
    let gate_idx = entry.gate_idx;
    // `BanGuildChat` 快照（104 收件人门）——先于 `sg` 的可变借用。
    let ban: HashMap<String, bool> = sessions
        .iter()
        .map(|(k, sg)| (k.clone(), sg.guild.ban_guild_chat))
        .collect();
    let Some(sg) = sessions.get_mut(chr_name) else {
        glog!(
            "WRN",
            "NPC 对话：ChrName={chr_name} 无会话态（未进世界/已断线）⇒ 丢弃"
        );
        return true;
    };
    let actor_id = sg.actor_id;
    let Some(pe) = world.entities.get(actor_id) else {
        glog!("WRN", "NPC 对话：actor_id={actor_id} 不在世界 ⇒ 丢弃");
        return true;
    };
    // `if (Death || Ghost) return;`（`Operate.cs:116-119`）。
    if pe.death || pe.ghost {
        glog!("DBG", "NPC 对话：Death/Ghost ⇒ return ChrName={chr_name}");
        return true;
    }
    let (px, py, pmap) = (pe.x, pe.y, pe.map_id);
    let Some(npc) = world.entities.get(npc_actor_id) else {
        return true;
    };
    let Some(st) = npc.npc.as_ref() else {
        return true;
    };
    let (nx, ny, nmap, npc_type) = (npc.x, npc.y, npc.map_id, st.npc_type);
    // `npc.Envir == Envir && IsWithinSight(npc) || npc.IsHide`（`Operate.cs:122-125`）。
    let within = nmap == pmap && (nx - px).abs() < 15 && (ny - py).abs() < 15;
    if !within {
        glog!(
            "DBG",
            "NPC 对话：异图/超视距 ChrName={chr_name} npc={npc_actor_id}"
        );
        return true;
    }
    // `npc.UserSelect(this, sMsg.Trim())`（`Operate.cs:123`）是**按实际类**的虚调用：
    // `Npcs.txt` type 0/1/2 ⇒ Merchant/GuildOfficial/CastleOfficial（`LocalDB.cs:688-698`）。
    // 只有 `GuildOfficial` 的 `UserSelect` 在本模块；type 0 `Merchant` 的脚本半由
    // `handle_shop_cmd` 的固定 NPC 臂负责（T-137），type 2 `CastleOfficial` 由两条臂的
    // `fixed_npc_script_allowed` 类型门拦下（T-140 / T-137 残余①）⇒ 二者都不进本函数。
    if npc_type != mir2_world::npc::NPC_TYPE_GUILD_OFFICIAL {
        glog!(
            "DBG",
            "NPC 对话：npc={npc_actor_id} type={npc_type} 非 GuildOfficial ⇒ 不派发 ChrName={chr_name}"
        );
        return true;
    }
    // `npc.UserSelect(this, sMsg.Trim())`（`Operate.cs:123`）。
    let s_data = tail.unwrap_or("").trim();
    if !s_data.starts_with('@') {
        return true;
    }
    let (label, msg) = split_label(s_data);
    // `base.UserSelect`（`NormNpc.cs:1633-1657`）：`ScriptGotoCount = 0` + 回跳标签账本。
    {
        let ss = shop_sessions.entry(chr_name.to_string()).or_default();
        if !eq_label(&ss.script_curr_lable, label) {
            if !eq_label(label, LABEL_BACK) {
                ss.script_go_back_lable = ss.script_curr_lable.clone();
                ss.script_curr_lable = label.to_string();
            } else if !ss.script_curr_lable.is_empty() {
                ss.script_curr_lable.clear();
            } else {
                ss.script_go_back_lable.clear();
            }
        }
    }
    // `LableIsCanJmp(sLabel)`（`PlayObject.Base.cs:2814-2826`）⇒ 不可跳则 `GotoLable` 后 return。
    let can_jmp = {
        // `LableIsCanJmp`（`PlayObject.Base.cs:2814-2828`）第三支读/清 `PlayDiceLabel`
        // （`PlayObject.cs:588`）；会话态在 `ShopSession::play_dice_label`。
        let ss = shop_sessions.entry(chr_name.to_string()).or_default();
        game_svr::shop::lable_is_can_jmp(&ss.can_jmp_labels, &mut ss.play_dice_label, label)
    };
    let mut queue: Vec<QueuedFrame> = Vec::new();
    let mut direct: Vec<Vec<u8>> = Vec::new();
    // `M2Share.ScriptEngine.GotoLable(PlayObject, ActorId, sLabel, !boCanJmp)`（`GuildOfficial.cs:70`）
    // —— **统一入口**：先跑 `Npc_def` 脚本（T-137），再按 `!boCanJmp` return（说词照出：
    // `GotoLable` 的 `boExtJmp && !sayingRecord.boExtJmp ⇒ return` 由引擎判定）。
    let (says, script_frames, use_slots) = {
        let ss = shop_sessions.entry(chr_name.to_string()).or_default();
        script.goto_label(sg, ss, npc_actor_id, label, !can_jmp)
    };
    // `TAKEW` 摘下的佩戴位：会话已写回，实体同步（否则下一 tick 的实体→会话回写会撤掉）。
    for slot in use_slots {
        crate::sync_use_item_slot_to_entity(world, sg, slot);
    }
    // `GotoLableSendMerChantSayMsg` → `playerActor.GetScriptLabel(sMsg)`（`ScriptEngine.cs:993`）：
    // 每句说词 Clear 后重建 `CanJmpScriptLableMap`（与商人 1011 路径同口径）。
    {
        let ss = shop_sessions.entry(chr_name.to_string()).or_default();
        for say in &says {
            ss.can_jmp_labels = game_svr::shop::get_script_label(&say.raw_msg);
        }
    }
    queue_script_output(
        &mut queue,
        &says,
        &script_frames,
        sg,
        actor_id,
        npc_actor_id,
    );
    if !can_jmp {
        glog!(
            "DBG",
            "NPC 对话：LableIsCanJmp({label}) == false ⇒ GotoLable 后 return ChrName={chr_name}"
        );
        let sent = flush(&mut queue, online, gates);
        glog!(
            "INF",
            "行会 NPC 标签 ChrName={chr_name} npc={npc_actor_id} label={label}（不可跳）→ 说词/脚本帧 {sent} 帧"
        );
        return true;
    }
    if eq_label(label, LABEL_BUILD_GUILD_NOW) {
        let outcome = request_build_guild(
            guilds,
            &BuildGuildRequest {
                guild_name: msg,
                chief: chr_name,
                has_guild: sg.guild.guild_name.is_some(),
                gold: sg.state.gold,
                price: castle_cfg.build_guild_price,
                woma_horn: &castle_cfg.woma_horn,
                items: &sg.state.item_list,
                catalog,
                cfg: guild_cfg,
            },
        );
        if outcome.added {
            // `GuildManager.AddGuild`：`new GuildInfo(sName)`（ctor 的 `GuildConf.Load()` 对不存在
            // 的 `<名>.ini` 建空文件）→ `SetGuildInfo`（写 `<名>.txt`）→ `SaveGuildList()`。
            if let Some(name) = outcome.my_guild.as_deref() {
                if let Some(store) = guild_store {
                    if let Err(e) = store.load_guild_conf(name) {
                        glog!("WRN", "建行会配置失败（.ini ctor 副作用）: {e}");
                    }
                }
            }
            // `SetGuildInfo` → `SaveGuildInfoFile()`（`GuildInfo.cs:463-483`）。
            if let Some(name) = outcome.my_guild.as_deref() {
                if let Some(g) = guilds.find_guild(name) {
                    if let Err(e) = guild_runtime::save_one(
                        guild_store,
                        guild_sections,
                        g,
                        now_tick,
                        server_index,
                    ) {
                        glog!("ERR", "保存行会信息失败!!! {e}");
                    }
                }
            }
            // `SaveGuildList()`（`GuildManager.cs:27`）——写失败按 oracle catch 吞。
            if let Err(e) = guild_runtime::save_list(guild_store, &guilds.names(), server_index) {
                glog!("ERR", "行会信息保存失败!!! {e}");
            }
            // `SendServerGroupMsg(SS_205, ServerIndex, sGuildName + '/' + ChrName)`（`:144`）。
            glog!("INF", "SS_205（单服无接收端）: {}/{}", msg.trim(), chr_name);
            // `SendDelItems(UserItem)` **直发**（`:145`）。
            if let Some(item) = &outcome.removed {
                if let Some(body) = delitem_frame(actor_id, item, catalog) {
                    direct.push(body);
                }
                // `DelBagItem(UserItem.MakeIndex, WomaHorn)`（`:146`）⇒ 命中则
                // `WeightChanged()`（`:2238`）重算 `WAbil.Weight` 并入队 `RM_WEIGHTCHANGED`。
                // 本仓在命中后立即产 `SM_WEIGHTCHANGED(622)`，优先级 131。
                if del_bag_item(
                    &mut sg.state,
                    item.make_index,
                    &castle_cfg.woma_horn,
                    catalog,
                ) {
                    let w = &sg.state.wabil;
                    push_frame(
                        &mut queue,
                        PRI_WEIGHTCHANGED,
                        actor_id,
                        smframe::sm_weightchanged(w.weight, w.wear_weight, w.hand_weight),
                    );
                }
            }
            // `DecGold(BuildGuildPrice); GoldChanged();`（`:147-148`）。
            sg.state.gold -= outcome.gold_spent;
            // `MyGuild = MemberOfGuild(ChrName)`；有会才 `GetRankName` + `RefShowName`。
            if let Some(name) = &outcome.my_guild {
                let (rank_no, rank_name) = outcome.rank.clone().unwrap_or((0, String::new()));
                sg.guild = GuildSession {
                    guild_name: Some(name.clone()),
                    rank_no,
                    rank_name: rank_name.clone(),
                    allow_guild: sg.guild.allow_guild,
                    ban_guild_chat: sg.guild.ban_guild_chat,
                    guild_done: sg.guild.guild_done,
                };
            }
        }
        for (priority, body) in build_queued_frames(&outcome, sg.state.gold, sg.game_gold) {
            push_frame(&mut queue, priority, actor_id, body);
        }
        // `RefShowName()`（`:155`）——只在 `MyGuild != null` 分支内。
        if outcome.my_guild.is_some() {
            if let Some(display) = displays.get(actor_id) {
                let body = show_name_frame(actor_id, display);
                for recipient in game_svr::chat::hear_targets(world, actor_id, ref_range) {
                    push_frame(&mut queue, PRI_USERNAME, recipient, body.clone());
                }
            } else {
                glog!(
                    "WRN",
                    "RefShowName：ActorId={actor_id} 无展示表条目 ⇒ 42 不产帧"
                );
            }
        }
    } else if eq_label(label, LABEL_GUILD_WAR) {
        war_select(
            actor_id,
            msg,
            chr_name,
            sg,
            online,
            world,
            displays,
            guilds,
            guild_store,
            guild_sections,
            guild_cfg,
            guild_msg_cfg,
            castle_cfg,
            server_index,
            ref_range,
            red_color,
            &ban,
            &mut queue,
            now_tick,
        );
    } else if eq_label(label, LABEL_DONATE) {
        // `DoNate`（`GuildOfficial.cs:203-206`）：`SendMsg(this, RM_DONATE_OK, 0, 0, 0, 0)`。
        push_frame(
            &mut queue,
            PRI_DONATE_OK,
            actor_id,
            guild_frames::sm_donate_ok_764(0),
        );
    } else if mir2_world::castle::compare_lstr(label, LABEL_REQUEST_CASTLE_WAR) {
        // `HUtil32.CompareLStr(sLabel, sREQUESTCASTLEWAR)`（`GuildOfficial.cs:82`）是**前缀**
        // 匹配（大小写不敏感）⇒ 后缀 = 城堡下标（`sLabel[prefix.Length..]`，`:84`）。
        request_castle_war(
            actor_id,
            label.get(LABEL_REQUEST_CASTLE_WAR.len()..).unwrap_or(""),
            sg,
            &world.castles,
            castle_cfg,
            red_color,
            catalog,
            &mut queue,
        );
    } else if eq_label(label, LABEL_EXIT) {
        // `SendMsg(this, RM_MERCHANTDLGCLOSE, 0, ActorId, 0, 0)`（`:84`）。
        push_frame(
            &mut queue,
            PRI_MERCHANTDLGCLOSE,
            actor_id,
            smframe::sm_merchantdlgclose(npc_actor_id),
        );
    } else if eq_label(label, LABEL_BACK) {
        // `GuildOfficial.cs:86-89`：`ScriptGoBackLable` 空则置 `@main`，再
        // `GotoLable(PlayObject, ActorId, ScriptGoBackLable)`（`boExtJmp = false`）。
        let back = match shop_sessions.get_mut(chr_name) {
            Some(ss) => {
                if ss.script_go_back_lable.is_empty() {
                    ss.script_go_back_lable = LABEL_MAIN.to_string();
                }
                ss.script_go_back_lable.clone()
            }
            None => LABEL_MAIN.to_string(),
        };
        let (says, script_frames, use_slots) = {
            let ss = shop_sessions.entry(chr_name.to_string()).or_default();
            script.goto_label(sg, ss, npc_actor_id, &back, false)
        };
        for slot in use_slots {
            crate::sync_use_item_slot_to_entity(world, sg, slot);
        }
        {
            let ss = shop_sessions.entry(chr_name.to_string()).or_default();
            for say in &says {
                ss.can_jmp_labels = game_svr::shop::get_script_label(&say.raw_msg);
            }
        }
        queue_script_output(
            &mut queue,
            &says,
            &script_frames,
            sg,
            actor_id,
            npc_actor_id,
        );
    }

    for body in &direct {
        if let Some(tx) = crate::route_to_gate(gates, gate_idx, socket, "行会 NPC 直发帧") {
            let _ = tx.send(SimToGate::SendFrame {
                socket,
                body: body.clone(),
            });
        }
    }
    let sent = flush(&mut queue, online, gates);
    glog!(
        "INF",
        "行会 NPC 标签 ChrName={chr_name} npc={npc_actor_id} type={npc_type} label={label} → 直发 {} + 排队 {sent} 帧",
        direct.len()
    );
    true
}

/// `HUtil32.StrToInt(sIndex, -1)`（`HUtil32.cs:359-362`：`int.TryParse`，失败取 `def`；
/// 默认 `NumberStyles.Integer` = 允许首尾空白与前导 `+`/`-`，不接受小数点/千分位）。
#[must_use]
pub(crate) fn str_to_int(s: &str, def: i32) -> i32 {
    s.trim().parse::<i32>().unwrap_or(def)
}

/// `ReQuestCastleWar` 的城堡下标（`GuildOfficial.cs:208-212`）：
/// `nIndex = StrToInt(sIndex, -1); if (nIndex < 0) nIndex = 0;`，再按
/// `CastleManager.GetCastle(nIndex)`（`CastleManager.cs:194-201`：`0 <= nIndex < Count`）取下标；
/// 越界 ⇒ `None`（oracle `GetCastle` 返 null）。
#[must_use]
pub(crate) fn castle_war_index(s_index: &str, castle_count: usize) -> Option<usize> {
    let mut n_index = str_to_int(s_index, -1);
    if n_index < 0 {
        n_index = 0;
    }
    usize::try_from(n_index).ok().filter(|i| *i < castle_count)
}

/// `PlayObject.SysMsg(sMsg, MsgColor.Red, MsgType.Hint)`（`BaseObject.cs:1588-1686`）：
/// `MsgType != Notice` + `MsgColor.Red` ⇒ `SendMsg(RM_SYSMESSAGE, 0, RedMsgFColor, RedMsgBColor, 0, sMsg)`
/// （**入队**，`MsgQueue` 优先级 = `(byte)100 = 252`；非 Notice 支**不**加 `LineNoticePreFix`）。
fn push_sys_msg(queue: &mut Vec<QueuedFrame>, actor_id: i32, red_color: (u8, u8), text: &str) {
    push_frame(
        queue,
        PRI_SYSMESSAGE,
        actor_id,
        smframe::sm_sysmessage(
            actor_id,
            u16::from(red_color.0),
            u16::from(red_color.1),
            &downlink::gbk(text),
        ),
    );
}

/// `GuildOfficial.ReQuestCastleWar`（`GuildOfficial.cs:206-239`）。
///
/// 逐行次序：`StrToInt(sIndex,-1)` → `<0` 归 0（`:208-212`）→ `CastleMgr.GetCastle(nIndex)`（`:213`）
/// → 门 `IsGuildMaster() && !Castle.IsMember(PlayObject)`（`:214`）→ `CheckItems(ZumaPiece)`
/// （`:216`，无则「你没有<ZumaPiece>!!!」`SysMsg`）→ `Castle.AddAttackerInfo(MyGuild)`（`:219`）。
///
/// **N1（照搬，不修）**：`:219` 的接收者静态类型是 `IUserCastle`，接口只声明
/// `AddAttackerInfo(IGuild)`（`IUserCastle.cs:125`），实现是 `UserCastle.cs:670-673` 的
/// `throw new NotImplementedException()`。所以成功支必然抛异常、被 `UserSelect` 的 catch
/// （`:100-103`）吞掉 ⇒ 客户端零帧、物品不扣、`AttackWarList` 不登记、不落盘、不发 SS_212；
/// `else` 支的「你现在无法请求攻城!!!」（`:227`）与 `~@request_ok`（`:224`）都**不可达**。
/// `Castle == null` 且 `IsGuildMaster()` 为真时 `Castle.IsMember` 抛 NRE（同样被吞）。
#[allow(clippy::too_many_arguments)]
fn request_castle_war(
    actor_id: i32,
    s_index: &str,
    sg: &mut SessionGround,
    castles: &[mir2_world::castle::UserCastle],
    castle_cfg: &GuildCastleConfig,
    red_color: (u8, u8),
    catalog: &dyn ItemCatalog,
    queue: &mut Vec<QueuedFrame>,
) {
    let castle = castle_war_index(s_index, castles.len()).map(|i| &castles[i]);
    let my_guild = sg.guild.guild_name.clone();
    // `PlayObject.IsGuildMaster()`（`PlayObject.cs:4402-4405`）= `MyGuild != null && GuildRankNo == 1`。
    let is_guild_master = my_guild.is_some() && sg.guild.rank_no == 1;
    if !is_guild_master {
        // `&&` 短路：非掌门不碰 `Castle.IsMember`（`Castle == null` 也不会抛 NRE）。
        push_sys_msg(queue, actor_id, red_color, "你的请求被取消!!!");
        return;
    }
    let Some(castle) = castle else {
        // `GetCastle` 越界返 null ⇒ `Castle.IsMember(...)` 抛 `NullReferenceException`，
        // 被 `UserSelect` 的 catch 吞 ⇒ 零帧零副作用。
        glog!(
            "ERR",
            "NullReferenceException: ReQuestCastleWar 城堡下标越界（sIndex={s_index:?} 城数={}）",
            castles.len()
        );
        return;
    };
    // `!Castle.IsMember(PlayObject)`（`UserCastle.cs:609-612` ⇒ `IsMasterGuild(MyGuild)`）。
    if castle.is_member_name(my_guild.as_deref()) {
        push_sys_msg(queue, actor_id, red_color, "你的请求被取消!!!");
        return;
    }
    // `PlayObject.CheckItems(SystemShare.Config.ZumaPiece)`（`BaseObject.cs:2189-2205`）。
    if check_items(&sg.state.item_list, catalog, &castle_cfg.zuma_piece).is_none() {
        push_sys_msg(
            queue,
            actor_id,
            red_color,
            &format!("你没有{}!!!", castle_cfg.zuma_piece),
        );
        return;
    }
    // `Castle.AddAttackerInfo(PlayObject.MyGuild)` ⇒ `NotImplementedException`（见函数头 N1）。
    glog!(
        "ERR",
        "NotImplementedException: IUserCastle.AddAttackerInfo(IGuild)（UserCastle.cs:670-673）；ReQuestCastleWar 成功支被 UserSelect 的 catch 吞掉（照搬 oracle，白名单 T-38/T-138）"
    );
}

/// `@@guildwar` 的宿主侧：NPC 层 + `PlayObject` 层 + 帧面（顺序见模块头）。
#[allow(clippy::too_many_arguments)]
fn war_select(
    actor_id: i32,
    target: &str,
    chr_name: &str,
    sg: &mut SessionGround,
    online: &OnlineRegistry,
    world: &mut mir2_world::World,
    displays: &DisplayTable,
    guilds: &mut GuildManager,
    guild_store: Option<&GuildStore>,
    guild_sections: &GuildFileSections,
    guild_cfg: &GuildConfig,
    guild_msg_cfg: &GuildMsgCfg,
    castle_cfg: &GuildCastleConfig,
    server_index: i32,
    ref_range: i32,
    red_color: (u8, u8),
    ban: &HashMap<String, bool>,
    queue: &mut Vec<QueuedFrame>,
    now_tick: i32,
) {
    let my_guild = sg.guild.guild_name.clone();
    let rank_no = sg.guild.rank_no;
    let war_price = castle_cfg.guild_war_price;
    let gold_before = sg.state.gold;
    let target_exists = guilds.find_guild(target).is_some();
    // NPC 层：目标存在 + `Gold >= GuildWarPrice` ⇒ `DecGold(GuildWarPrice); GoldChanged();`。
    let price_gate = target_exists && gold_before >= war_price;
    // `PlayObject.ReQuestGuildWar` 的进入条件（`Base.cs:3351-3367`）：掌门 / 0 区 / 目标存在。
    let play_ok = price_gate && my_guild.is_some() && rank_no == 1 && server_index == 0;
    // `if (warGuild.WarTick > 0)`（`:3369`）：只有本会**已在战**时 `AddWarGuild` 才返回非 0。
    let was_at_war = my_guild
        .as_deref()
        .and_then(|g| guilds.find_guild(g))
        .is_some_and(|g| g.is_war_guild(target));
    let req = GuildWarRequest {
        requester_guild: my_guild.as_deref(),
        requester_rank_no: rank_no,
        server_index,
        target_guild: target,
        gold: gold_before,
        war_price,
    };
    let world_ref = &*world;
    let outcome = guilds.request_guild_war(&req, guild_cfg, now_tick, |name| {
        online.iter().any(|e| {
            e.chr_name == name && world_ref.entities.get(e.actor_id).is_some_and(|x| !x.ghost)
        })
    });
    // `DecGold` 先于掌门判定（`GuildOfficial.cs:186`）⇒ 非掌门/非 0 区也白扣。
    if price_gate {
        sg.state.gold -= outcome.gold_spent;
    }
    // `AddWarGuild` 尾的 `UpdateGuildFile()`：本会（`MyGuild.AddWarGuild`）始终执行；
    // 目标会只在 `first.result.WarTick > 0`（= 本会已在战且 now > 0）时进入第二层。
    if play_ok {
        if let Some(g) = my_guild.as_deref() {
            if let Some(state) = guilds.find_guild(g) {
                if let Err(e) = guild_runtime::save_one(
                    guild_store,
                    guild_sections,
                    state,
                    now_tick,
                    server_index,
                ) {
                    glog!("ERR", "保存行会信息失败!!! {e}");
                }
            }
        }
        if was_at_war && now_tick > 0 {
            if let Some(state) = guilds.find_guild(target) {
                if let Err(e) = guild_runtime::save_one(
                    guild_store,
                    guild_sections,
                    state,
                    now_tick,
                    server_index,
                ) {
                    glog!("ERR", "保存行会信息失败!!! {e}");
                }
            }
        }
    }
    if price_gate {
        push_frame(
            queue,
            PRI_GOLDCHANGED,
            actor_id,
            smframe::sm_goldchanged(sg.state.gold, sg.game_gold),
        );
    }
    war_events_to_frames(
        actor_id,
        chr_name,
        &outcome.events,
        online,
        world,
        displays,
        guilds,
        guild_msg_cfg,
        ref_range,
        red_color,
        ban,
        queue,
    );
}

/// `GuildWarOutcome.events` → 帧（`104`/`42`/`100`；`SS_207` 只落日志）。
#[allow(clippy::too_many_arguments)]
fn war_events_to_frames(
    actor_id: i32,
    chr_name: &str,
    events: &[GuildWarEvent],
    online: &OnlineRegistry,
    world: &mir2_world::World,
    displays: &DisplayTable,
    guilds: &GuildManager,
    guild_msg_cfg: &GuildMsgCfg,
    ref_range: i32,
    red_color: (u8, u8),
    ban: &HashMap<String, bool>,
    queue: &mut Vec<QueuedFrame>,
) {
    for ev in events {
        match ev {
            GuildWarEvent::GuildMsg { guild, text } => {
                let rcpt = crate::guild_recipients(online, world, ban);
                for (recipient, body) in guild_runtime::guild_broadcast_104(
                    guilds,
                    guild,
                    text,
                    guild_msg_cfg,
                    &rcpt,
                    None,
                ) {
                    push_frame(queue, PRI_GUILDMESSAGE, recipient, body);
                }
            }
            GuildWarEvent::RefShowName { guild, member } => {
                // `MemberList[j].PlayObject != null` 由 `ref_member_name(online)` 已保证；这里
                // 再按名解析展示表（`chat::hear_targets` 的收件人集合含自己）。
                let Some(e) = online.iter().find(|e| eq_label(&e.chr_name, member)) else {
                    continue;
                };
                let Some(display) = displays.get(e.actor_id) else {
                    glog!(
                        "WRN",
                        "行会战 RefShowName：{guild}/{member} 无展示表条目 ⇒ 42 不产帧"
                    );
                    continue;
                };
                let body = show_name_frame(e.actor_id, display);
                for recipient in game_svr::chat::hear_targets(world, e.actor_id, ref_range) {
                    push_frame(queue, PRI_USERNAME, recipient, body.clone());
                }
            }
            GuildWarEvent::SysMsg { text } => {
                push_frame(
                    queue,
                    PRI_SYSMESSAGE,
                    actor_id,
                    smframe::sm_sysmessage(
                        actor_id,
                        u16::from(red_color.0),
                        u16::from(red_color.1),
                        &downlink::gbk(text),
                    ),
                );
            }
            GuildWarEvent::ServerGroupMsg { guild } => {
                // `SendServerGroupMsg(SS_207, ServerIndex, sGuildName)`：单服无接收端。
                glog!("INF", "SS_207（单服无接收端）: {guild} ChrName={chr_name}");
            }
        }
    }
}

#[cfg(test)]
mod tests;
