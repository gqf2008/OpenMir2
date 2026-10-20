//! **行会会话层 G2：9 个 `CM_GUILD*` 的派发与回包**（oracle `PlayObject.Operate.cs:1229-1560`）。
//!
//! 交付卡：`openmir2-m3-guild-session-g2`（简报
//! `mir2-rs/tests/parity/m3/design-guild-g2-dispatch-20261020.md`，**以「评审员更正」节为准**）。
//!
//! # 范围
//!
//! | # | CM | ident | oracle | 本模块 |
//! |---|---|---|---|---|
//! | 1 | `CM_OPENGUILDDLG` | 1035 | `ClientOpenGuildDlg` `:1229-1277` | [`Ctx::open_dlg`] |
//! | 2 | `CM_GUILDHOME` | 1036 | `ClientGuildHome` `:1278-1281`（= 1035） | 同上（同一函数） |
//! | 3 | `CM_GUILDMEMBERLIST` | 1037 | `ClientGuildMemberList` `:1283-1305` | [`Ctx::member_list`] |
//! | 4 | `CM_GUILDADDMEMBER` | 1038 | `ClientGuildAddMember` `:1307-1367` | [`Ctx::add_member`] |
//! | 5 | `CM_GUILDDELMEMBER` | 1039 | `ClientGuildDelMember` `:1369-1424` | [`Ctx::del_member`] |
//! | 6 | `CM_GUILDUPDATENOTICE` | 1040 | `ClientGuildUpdateNotice` `:1426-1442` | [`Ctx::update_notice`] |
//! | 7 | `CM_GUILDUPDATERANKINFO` | 1041 | `ClientGuildUpdateRankInfo` `:1444-1463` | [`Ctx::update_rank`] |
//! | 8 | `CM_GUILDALLY` | 1044 | `ClientGuildAlly` `:1465-1523` | [`Ctx::ally`] |
//! | 9 | `CM_GUILDBREAKALLY` | 1045 | `ClientGuildBreakAlly` `:1525-1560` | [`Ctx::break_ally`] |
//!
//! **绝不能**用 `1035..=1045` 区间匹配：`1042`（非法数据日志）与 `1043`（`CM_ADJUST_BONUS`）
//! 不属行会臂，它们由 `inbound::INBOUND_IDENTS` 兜底（简报 §0.3）。[`GUILD_IDENTS`] 是**显式
//! 9 条**，主派发臂用它判等。
//!
//! # 会话态
//!
//! [`GuildSession`] 内嵌在宿主 `SessionGround`（`main.rs`）里，逐字段复刻 `PlayObject` 的
//! `MyGuild` / `GuildRankNo` / `GuildRankName` / `AllowGuild` / `BanGuildChat`：
//!
//! - 默认 `guild_name=None, rank_no=0, rank_name="", allow_guild=false, ban_guild_chat=true`
//!   （`PlayObject.cs:903/1007` 的 ctor 初值）；
//! - `allow_guild` 唯一生产写点是 GM `@Letguild`（`LetGuildCommand.cs:12`），本仓未接 ⇒ **恒 false**，
//!   加成员生产恒 `758/5`；判据用测试夹具直接置位覆盖成功路径，**不改默认值**；
//! - `ban_guild_chat` 唯一写点是 GM `@BanGuildChat` 与跨服 `SwitchData`（本仓未移植）⇒ **恒 true**
//!   （名字是反向的：`true` = **允许**收行会消息，见 `PlayObject.cs:1007`）。
//!
//! 管理器 `mir2_world::GuildManager` 才是真源；会话字段是 `PlayObject` 侧缓存。G2 的写点只有
//! 登录物化（[`materialize`]）、1038 成功（目标）、1039 删人/自删（目标/自己）。
//!
//! # 回帧的「直发 vs 排队」口径
//!
//! oracle 里 `SendDefMessage`（759/757/761/769/753…）是**当场写 socket**，而 `RefRankInfo` 的 750、
//! `RefShowName` 的 42、`SysMsg` 的 100 走 `SendMsg` **入队**（优先级 61 / 165 / 252，下一轮消息泵才出）。
//! 因此同一 socket 上 **759/757 先于** 750/42/100。本模块按这个**线上序**发帧：先直发回包，
//! 再按 750 → 42 → 100 的顺序发排队帧（见 [`Deferred`]）。
//!
//! `SS_206`/`SS_207` 单服无接收端（`WorldServer.cs:885-895`）⇒ 只落 [`GuildServerEvent`] 供日志/判据，
//! **不产客户端帧**。
//!
//! # 照搬的 oracle 形态（不是本项目 bug）
//!
//! 1. **1041 成功 = 0 帧**（N3：`UpdateRank` 成功后 `RankList = rankListTmp` 空表 ⇒
//!    `ClientGuildMemberList` 拼出空串 ⇒ `SendSocket(defMsg, "")` 整帧不发）；
//! 2. **1045 失败发 `769/0`，不是 771**（`771` 全仓无发点）；`768/770` 生产不可达；
//! 3. **1044 恒 `769/-4`**：`GuildInfo.EnableAuthAlly` ctor false、唯一取反点被注释；
//! 4. **1039 自删**：`CancelGuld` 成功才 `DelGuild` + `SS_206` + 清自己会话 + `100` 红字；
//!    多人会 `CancelGuld` 返回 false ⇒ 最终 `760/3`（不是 4）。

use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

use game_svr::chat;
use game_svr::downlink::{self, DisplayTable};
use mir2_protocol::messages;
use mir2_shared::hutil32::to_upper;
use mir2_world::{GuildConfig, GuildManager};

use crate::guild_frames;
use crate::smframe;
use crate::{OnlineEntry, OnlineRegistry, SimToGate};

/// 行会臂的**显式 9 条** ident（**不得**用 `1035..=1045` 区间，见模块头）。
pub(crate) const GUILD_IDENTS: [u16; 9] = [
    messages::CM_OPENGUILDDLG,
    messages::CM_GUILDHOME,
    messages::CM_GUILDMEMBERLIST,
    messages::CM_GUILDADDMEMBER,
    messages::CM_GUILDDELMEMBER,
    messages::CM_GUILDUPDATENOTICE,
    messages::CM_GUILDUPDATERANKINFO,
    messages::CM_GUILDALLY,
    messages::CM_GUILDBREAKALLY,
];

/// `PlayObject` 侧的行会会话字段（简报 §2.1）——内嵌在宿主 `SessionGround`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuildSession {
    /// `PlayObject.MyGuild?.GuildName`（管理器是真源，这里是缓存；`None` = `MyGuild == null`）。
    pub guild_name: Option<String>,
    /// `PlayObject.GuildRankNo`（`0` = 无）。
    pub rank_no: i16,
    /// `PlayObject.GuildRankName`。
    pub rank_name: String,
    /// `PlayObject.AllowGuild`（ctor `false`；`@Letguild` 未接 ⇒ 生产恒 false）。
    pub allow_guild: bool,
    /// `PlayObject.BanGuildChat`（ctor `true`；**`true` = 允许**收行会消息）。
    pub ban_guild_chat: bool,
}

impl Default for GuildSession {
    /// `PlayObject.cs:903/1007` 的 ctor 初值。**`ban_guild_chat` 默认必须为 `true`**
    /// （字段名反向），`allow_guild` 必须为 `false`（牙：改任一个 ⇒ 会话默认值用例红）。
    fn default() -> Self {
        Self {
            guild_name: None,
            rank_no: 0,
            rank_name: String::new(),
            allow_guild: false,
            ban_guild_chat: true,
        }
    }
}

/// 宿主会话表里「取行会字段」的最小接缝。
///
/// 生产实现是 `SessionGround`（`main.rs`）；测试实现是 `HashMap<String, GuildSession>`
/// （避免为用例复制整张 `SessionGround` 的 40+ 字段）。
pub(crate) trait GuildSessions {
    fn guild(&self, chr_name: &str) -> Option<&GuildSession>;
    fn guild_mut(&mut self, chr_name: &str) -> Option<&mut GuildSession>;
}

impl GuildSessions for HashMap<String, crate::SessionGround> {
    fn guild(&self, chr_name: &str) -> Option<&GuildSession> {
        self.get(chr_name).map(|s| &s.guild)
    }
    fn guild_mut(&mut self, chr_name: &str) -> Option<&mut GuildSession> {
        self.get_mut(chr_name).map(|s| &mut s.guild)
    }
}

impl GuildSessions for HashMap<String, GuildSession> {
    fn guild(&self, chr_name: &str) -> Option<&GuildSession> {
        self.get(chr_name)
    }
    fn guild_mut(&mut self, chr_name: &str) -> Option<&mut GuildSession> {
        self.get_mut(chr_name)
    }
}

/// 登录物化（`PlayObject.cs:1391-1400`）：
/// `MyGuild = MemberOfGuild(ChrName)`；有会才 `GetRankName`（`OrdinalIgnoreCase`），
/// 找不到封号则 `0`/空（oracle 的 `short rankNo = 0` 初值）。
#[must_use]
pub(crate) fn materialize(guilds: &GuildManager, chr_name: &str) -> GuildSession {
    let Some(guild_name) = guilds.member_of_guild(chr_name) else {
        return GuildSession::default();
    };
    let (rank_no, rank_name) = guilds
        .find_guild(guild_name)
        .and_then(|g| g.get_rank_name(chr_name))
        .unwrap_or((0, String::new()));
    GuildSession {
        guild_name: Some(guild_name.to_string()),
        rank_no,
        rank_name,
        ..GuildSession::default()
    }
}

/// 行会臂的**跨服事件**（`WorldServer.SendServerGroupMsg`）。单服无接收端 ⇒ 只落日志/判据。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GuildServerEvent {
    /// `SendServerGroupMsg(SS_206/SS_207, ServerIndex, guild)`——`ident` 是 `SS_*` 消息号。
    ServerGroupMsg { ident: u16, guild: String },
}

impl GuildServerEvent {
    /// 一行可读日志（与 oracle 的 `SS_*` 同形）。
    #[must_use]
    pub(crate) fn describe(&self) -> String {
        match self {
            GuildServerEvent::ServerGroupMsg { ident, guild } => format!("SS_{ident} {guild}"),
        }
    }
}

/// `WorldServer.GetPlayObject`（`WorldServer.cs:937-956`）：
/// 线性扫在线表，**首个** `OrdinalIgnoreCase` 命中即 `break`；命中项 `Ghost` 或
/// `IsPasswordLocked && ObMode && AdminMode` ⇒ 返回 `None`（**不再继续找**）。
///
/// 本仓 `IsPasswordLocked`/`AdminMode` 未建模（`pwd_sessions` 只有密码流程）⇒ 只判 `Ghost`。
pub(crate) fn find_play_object<'a>(
    online: &'a OnlineRegistry,
    world: &mir2_world::World,
    name: &str,
) -> Option<&'a OnlineEntry> {
    let upper = to_upper(name);
    let e = online.iter().find(|e| to_upper(&e.chr_name) == upper)?;
    let ghost = world.entities.get(e.actor_id).is_none_or(|x| x.ghost);
    if ghost {
        return None;
    }
    Some(e)
}

/// 一条 `CM_GUILD*` 的派发入口（主派发臂只做 ident 判等后调用本函数）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_guild_cmd<S: GuildSessions>(
    ident: u16,
    tail: Option<&str>,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    world: &mir2_world::World,
    displays: &DisplayTable,
    guilds: &mut GuildManager,
    cfg: &GuildConfig,
    sessions: &mut S,
    gates: &HashMap<u8, UnboundedSender<SimToGate>>,
    ref_range: i32,
    green: (u8, u8),
    red: (u8, u8),
    now_ms: i64,
) -> Vec<GuildServerEvent> {
    let Some(me) = online.iter().find(|e| e.chr_name == chr_name) else {
        crate::glog!(
            "WRN",
            "CM_GUILD ident={ident}：ChrName={chr_name} socket={socket} 不在在线登记表 ⇒ 丢弃（与 handle_party_cmd 同构）"
        );
        return Vec::new();
    };
    if sessions.guild(chr_name).is_none() {
        crate::glog!(
            "WRN",
            "CM_GUILD ident={ident}：ChrName={chr_name} 无 SessionGround ⇒ 丢弃（登记）"
        );
        return Vec::new();
    }
    let mut ctx = Ctx {
        tail: tail.unwrap_or(""),
        chr_name,
        me,
        online,
        world,
        displays,
        guilds,
        cfg,
        sessions,
        gates,
        ref_range,
        green,
        red,
        now_tick: now_ms as i32,
        events: Vec::new(),
    };
    // 两个臂**保持独立**（同体是 oracle 同构：`ClientGuildHome` 直接调 `ClientOpenGuildDlg`）：
    // recon `dispatch` 的生产派发点按 `messages::CM_X =>` 臂头取证，合并成联合臂会让
    // `CM_GUILDHOME` 落成 tracked（假缺口）。
    #[allow(clippy::match_same_arms)]
    match ident {
        messages::CM_OPENGUILDDLG => ctx.open_dlg(),
        messages::CM_GUILDHOME => ctx.open_dlg(),
        messages::CM_GUILDMEMBERLIST => ctx.member_list(),
        messages::CM_GUILDADDMEMBER => ctx.add_member(),
        messages::CM_GUILDDELMEMBER => ctx.del_member(),
        messages::CM_GUILDUPDATENOTICE => ctx.update_notice(),
        messages::CM_GUILDUPDATERANKINFO => ctx.update_rank(),
        messages::CM_GUILDALLY => ctx.ally(),
        messages::CM_GUILDBREAKALLY => ctx.break_ally(),
        _ => crate::glog!(
            "ERR",
            "CM_GUILD 臂收到非行会 ident={ident}（派发臂表与 GUILD_IDENTS 不一致）"
        ),
    }
    ctx.events
}

/// oracle 里走 `SendMsg` **排队**的帧（750/42/100），在直发回包**之后**按队列优先级发出。
enum Deferred {
    /// 已编码帧（750 纯头 / 带尾）。
    Frame { actor_id: i32, body: Vec<u8> },
    /// `RefShowName()` 的 42 广播。
    RefShowName { target_id: i32 },
    /// `SysMsg(..., Green/Red, Hint)` 的 100。
    SysMsg {
        actor_id: i32,
        color: (u8, u8),
        text: String,
    },
}

/// 一次派发的上下文（含宿主句柄 + 事件收集）。
struct Ctx<'a, S: GuildSessions> {
    tail: &'a str,
    chr_name: &'a str,
    me: &'a OnlineEntry,
    online: &'a OnlineRegistry,
    world: &'a mir2_world::World,
    displays: &'a DisplayTable,
    guilds: &'a mut GuildManager,
    cfg: &'a GuildConfig,
    sessions: &'a mut S,
    gates: &'a HashMap<u8, UnboundedSender<SimToGate>>,
    ref_range: i32,
    green: (u8, u8),
    red: (u8, u8),
    now_tick: i32,
    events: Vec<GuildServerEvent>,
}

impl<S: GuildSessions> Ctx<'_, S> {
    // ---- 会话读 ----

    fn guild_name(&self) -> Option<String> {
        self.sessions
            .guild(self.chr_name)
            .and_then(|s| s.guild_name.clone())
    }

    fn rank_no(&self) -> i16 {
        self.sessions.guild(self.chr_name).map_or(0, |s| s.rank_no)
    }

    /// `PlayObject.IsGuildMaster()`（`PlayObject.cs:4402-4405`）：`MyGuild != null && GuildRankNo == 1`。
    fn is_guild_master(&self) -> bool {
        self.guild_name().is_some() && self.rank_no() == 1
    }

    fn allow_guild(&self, chr_name: &str) -> bool {
        self.sessions.guild(chr_name).is_some_and(|s| s.allow_guild)
    }

    fn target_guild_name(&self, chr_name: &str) -> Option<String> {
        self.sessions
            .guild(chr_name)
            .and_then(|s| s.guild_name.clone())
    }

    fn clear_guild(&mut self, chr_name: &str) {
        if let Some(s) = self.sessions.guild_mut(chr_name) {
            s.guild_name = None;
            s.rank_no = 0;
            s.rank_name.clear();
        }
    }

    // ---- 发帧 ----

    /// 按 `ActorId` 找在线会话的 `[GateIdx, Socket]` 并投递一帧。
    fn send_to(&self, actor_id: i32, body: Vec<u8>, what: &str) {
        let Some(t) = self.online.iter().find(|e| e.actor_id == actor_id) else {
            crate::glog!("WRN", "{what}：ActorId={actor_id} 不在在线登记表 ⇒ 丢弃");
            return;
        };
        let Some(tx) = crate::route_to_gate(self.gates, t.gate_idx, t.socket, what) else {
            return;
        };
        let _ = tx.send(SimToGate::SendFrame {
            socket: t.socket,
            body,
        });
    }

    fn send_self(&self, body: Vec<u8>) {
        self.send_to(self.me.actor_id, body, "行会回帧");
    }

    /// `BaseObject.SysMsg(text, msgColor, msgType)`（`BaseObject.cs:1588-1680`）的队列支：
    /// `SendMsg(RM_SYSMESSAGE, 0, FColor, BColor, 0, text)` ⇒ `SM_SYSMESSAGE(100)`，
    /// `Recog` = 收件人自己的 `ActorId`、`Param = MakeWord(F,B)`、`Series = 1`。
    fn sys_msg(&self, actor_id: i32, color: (u8, u8), text: &str) {
        let body = smframe::sm_sysmessage(
            actor_id,
            u16::from(color.0),
            u16::from(color.1),
            &downlink::gbk(text),
        );
        self.send_to(actor_id, body, "行会 SysMsg");
    }

    /// `BaseObject.RefShowName()`（`BaseObject.Base.cs:1174-1177`）⇒ `SendRefMsg(RM_USERNAME)`：
    /// 收件人集合 = [`chat::hear_targets`]（含自己，已处理 `ObMode`/`FixedHideMode`）；
    /// `Recog` = **被展示者**的 `ActorId`，尾 = `GetShowName()` 的 GBK 字节（展示表缓存，与登录同源）。
    fn ref_show_name(&self, target_id: i32) {
        let Some(display) = self.displays.get(target_id) else {
            crate::glog!(
                "WRN",
                "RefShowName：ActorId={target_id} 无展示表条目 ⇒ 42 不产帧（登记）"
            );
            return;
        };
        let body = smframe::sm_username(target_id, display.name_color, &display.show_name);
        for recipient in chat::hear_targets(self.world, target_id, self.ref_range) {
            self.send_to(recipient, body.clone(), "行会 RefShowName");
        }
    }

    /// 直发回包**之后**再发排队帧（750 → 42 → 100）。
    fn flush(&self, queue: Vec<Deferred>) {
        for item in queue {
            match item {
                Deferred::Frame { actor_id, body } => self.send_to(actor_id, body, "行会排队帧"),
                Deferred::RefShowName { target_id } => self.ref_show_name(target_id),
                Deferred::SysMsg {
                    actor_id,
                    color,
                    text,
                } => self.sys_msg(actor_id, color, &text),
            }
        }
    }

    // ---- 1/2. 1035 / 1036：ClientOpenGuildDlg（同构） ----

    fn open_dlg(&mut self) {
        let Some(name) = self.guild_name() else {
            // `Operate.cs:1274`：无会 ⇒ `SM_OPENGUILDDLG_FAIL(754)` 纯头。
            self.send_self(guild_frames::sm_guild_open_dlg_fail_754());
            return;
        };
        let rank_no = self.rank_no();
        let Some(panel) = self
            .guilds
            .find_guild(&name)
            .map(|g| mir2_world::guild::open_dlg_text(g, rank_no))
        else {
            crate::glog!(
                "WRN",
                "ClientOpenGuildDlg：会话 MyGuild={name} 在管理器里不存在 ⇒ 按无会发 754（登记）"
            );
            self.send_self(guild_frames::sm_guild_open_dlg_fail_754());
            return;
        };
        // `MakeMessage(753,0,0,0,1)` + `SendSocket(defMsg, EncodeString(panel))`：直发。
        if let Some(body) = guild_frames::sm_guild_open_dlg_753(&panel) {
            self.send_self(body);
        }
    }

    // ---- 3. 1037 ----

    fn member_list(&mut self) {
        let Some(name) = self.guild_name() else {
            // `Operate.cs:1285-1288`：无会直接 return（零帧）。
            return;
        };
        let text = self
            .guilds
            .find_guild(&name)
            .map_or_else(String::new, mir2_world::GuildState::member_list_text);
        // 文本为空 ⇒ `SendSocket(defMsg, "")` 整帧不发（`PlayObject.Socket.cs:108-111`）。
        if let Some(body) = guild_frames::sm_guild_member_list_756(&text) {
            self.send_self(body);
        }
    }

    // ---- 4. 1038 ----

    fn add_member(&mut self) {
        let s_hum_name = self.tail;
        let mut n_c: u8 = 1;
        let mut queue: Vec<Deferred> = Vec::new();
        if self.is_guild_master() {
            // `GetPlayObject(name)`（**OrdinalIgnoreCase**；首个命中即 break，Ghost ⇒ null）。
            if let Some(target) = find_play_object(self.online, self.world, s_hum_name) {
                let target_actor = target.actor_id;
                let target_chr = target.chr_name.clone();
                // 目标必须**面朝掌门**（`playObject.GetPoseCreate() == this`）。
                if crate::pose_create(self.world, self.displays, target_actor)
                    == Some(self.me.actor_id)
                {
                    if self.allow_guild(&target_chr) {
                        let my_guild = self.guild_name().unwrap_or_default();
                        // `MyGuild.IsMember(sHumName)`：**大小写敏感** `==`。
                        let is_member = self
                            .guilds
                            .find_guild(&my_guild)
                            .is_some_and(|g| g.is_member(s_hum_name));
                        if !is_member {
                            let target_has_guild = self.target_guild_name(&target_chr).is_some();
                            let rank_count = self
                                .guilds
                                .find_guild(&my_guild)
                                .map_or(0, |g| g.ranks.len());
                            if target_has_guild
                                || rank_count >= mir2_world::guild::GUILD_RANK_LIST_LIMIT
                            {
                                n_c = 4;
                            } else {
                                n_c = self.finish_add_member(&my_guild, &target_chr, &mut queue);
                            }
                        } else {
                            n_c = 3;
                        }
                    } else {
                        // `AllowGuild == false`：生产恒走这支（简报 §0.6）。
                        n_c = 5;
                    }
                } else {
                    n_c = 2;
                }
            } else {
                n_c = 2;
            }
        }
        // 直发 757 在排队帧之前（模块头「直发 vs 排队」）。
        if n_c == 0 {
            self.send_self(guild_frames::sm_guild_add_member_ok_757());
        } else {
            self.send_self(guild_frames::sm_guild_add_member_fail_758(n_c));
        }
        self.flush(queue);
    }

    /// 1038 成功链（`Operate.cs:1323-1331`）：`AddMember` → `SS_207` → 目标会话 → 目标 `750` →
    /// 目标 `42` → 目标 `100`（Green/Hint）。返回 `0`。
    fn finish_add_member(
        &mut self,
        my_guild: &str,
        target_chr: &str,
        queue: &mut Vec<Deferred>,
    ) -> u8 {
        // `AddMember(playObject)`：找/建 `RankNo==99`，存 `playObject.ChrName`（规范名）+ `UpdateGuildFile`。
        if let Some(g) = self.guilds.find_guild_mut(my_guild) {
            g.add_member(target_chr, self.cfg, self.now_tick);
        }
        self.events.push(GuildServerEvent::ServerGroupMsg {
            ident: messages::SS_207,
            guild: my_guild.to_string(),
        });
        // 目标会话：`MyGuild = MyGuild`；`GuildRankName = GetRankName(...)`（刚加入必命中）。
        let (rank_no, rank_name) = self
            .guilds
            .find_guild(my_guild)
            .and_then(|g| g.get_rank_name(target_chr))
            .unwrap_or((
                mir2_world::guild::GUILD_RANK_NO_MEMBER,
                self.cfg.guild_member_rank.clone(),
            ));
        if let Some(s) = self.sessions.guild_mut(target_chr) {
            s.guild_name = Some(my_guild.to_string());
            s.rank_no = rank_no;
            s.rank_name = rank_name.clone();
        }
        let Some(target) = self
            .online
            .iter()
            .find(|e| e.chr_name == target_chr)
            .map(|e| e.actor_id)
        else {
            crate::glog!(
                "WRN",
                "ClientGuildAddMember：目标 {target_chr} 已离线 ⇒ 750/42/100 不产（登记）"
            );
            return 0;
        };
        // 目标 `750`（`GetRankName` 里 `SendMsg(RM_CHANGEGUILDNAME)`，优先级 61）。
        queue.push(Deferred::Frame {
            actor_id: target,
            body: guild_frames::sm_guild_changename_750(Some(my_guild), &rank_name),
        });
        // 目标 `42`（`RefShowName()`，优先级 165）。
        queue.push(Deferred::RefShowName { target_id: target });
        // 目标 `100`（`SysMsg("你已加入行会: … 当前封号为: …", Green, Hint)`，优先级 252）。
        queue.push(Deferred::SysMsg {
            actor_id: target,
            color: self.green,
            text: format!("你已加入行会: {my_guild} 当前封号为: {rank_name}"),
        });
        0
    }

    // ---- 5. 1039 ----

    fn del_member(&mut self) {
        let s_hum_name = self.tail;
        let mut n_c: u8 = 1;
        let mut queue: Vec<Deferred> = Vec::new();
        if self.is_guild_master() {
            let my_guild = self.guild_name().unwrap_or_default();
            let is_member = self
                .guilds
                .find_guild(&my_guild)
                .is_some_and(|g| g.is_member(s_hum_name));
            if is_member {
                // `ChrName != sHumName`：**大小写敏感**。
                if self.chr_name != s_hum_name {
                    let removed = self
                        .guilds
                        .find_guild_mut(&my_guild)
                        .is_some_and(|g| g.del_member(s_hum_name, self.now_tick));
                    if removed {
                        // 目标可能已离线：`GetPlayObject == null` ⇒ 不写会话、不产 750/42。
                        if let Some(target) = find_play_object(self.online, self.world, s_hum_name)
                        {
                            let target_actor = target.actor_id;
                            let target_chr = target.chr_name.clone();
                            // `MyGuild = null; RefRankInfo(0, "")` ⇒ 会话清空 + **纯头 750**。
                            self.clear_guild(&target_chr);
                            queue.push(Deferred::Frame {
                                actor_id: target_actor,
                                body: guild_frames::sm_guild_changename_750(None, ""),
                            });
                            queue.push(Deferred::RefShowName {
                                target_id: target_actor,
                            });
                        }
                        self.events.push(GuildServerEvent::ServerGroupMsg {
                            ident: messages::SS_207,
                            guild: my_guild,
                        });
                        n_c = 0;
                    } else {
                        n_c = 4;
                    }
                } else {
                    // 自删：`nC` 先置 3；`CancelGuld` 成功才归 0。
                    n_c = 3;
                    let s14 = my_guild.clone();
                    let cancelled = self
                        .guilds
                        .find_guild_mut(&s14)
                        .is_some_and(|g| g.cancel_guld(s_hum_name));
                    if cancelled {
                        self.guilds.del_guild(&s14);
                        self.events.push(GuildServerEvent::ServerGroupMsg {
                            ident: messages::SS_206,
                            guild: s14.clone(),
                        });
                        // `MyGuild = null; RefRankInfo(0,"")` ⇒ 自己会话清空 + 纯头 750。
                        self.clear_guild(self.chr_name);
                        queue.push(Deferred::Frame {
                            actor_id: self.me.actor_id,
                            body: guild_frames::sm_guild_changename_750(None, ""),
                        });
                        queue.push(Deferred::RefShowName {
                            target_id: self.me.actor_id,
                        });
                        // `SysMsg("行会{名}已被取消!!!", Red, Hint)`。
                        queue.push(Deferred::SysMsg {
                            actor_id: self.me.actor_id,
                            color: self.red,
                            text: format!("行会{s14}已被取消!!!"),
                        });
                        n_c = 0;
                    }
                }
            } else {
                n_c = 2;
            }
        }
        // 直发 759/760 先于排队帧（oracle 的 SendDefMessage 当场写 socket）。
        if n_c == 0 {
            self.send_self(guild_frames::sm_guild_del_member_ok_759());
        } else {
            self.send_self(guild_frames::sm_guild_del_member_fail_760(n_c));
        }
        self.flush(queue);
    }

    // ---- 6. 1040 ----

    fn update_notice(&mut self) {
        let Some(name) = self.guild_name() else {
            return;
        };
        if self.rank_no() != 1 {
            return;
        }
        let notices = mir2_world::guild::notices_from_text(self.tail);
        if let Some(g) = self.guilds.find_guild_mut(&name) {
            g.notices = notices;
            // `SaveGuildInfoFile()`：**不经 BoChanged**（G3 接 `GuildStore`；G2 不落盘，登记）。
        }
        self.events.push(GuildServerEvent::ServerGroupMsg {
            ident: messages::SS_207,
            guild: name,
        });
        // `ClientOpenGuildDlg()`：直发 753 面板。
        self.open_dlg();
    }

    // ---- 7. 1041 ----

    fn update_rank(&mut self) {
        let Some(name) = self.guild_name() else {
            return;
        };
        if self.rank_no() != 1 {
            return;
        }
        let online = self.online;
        let world = self.world;
        let cfg = self.cfg;
        let now_tick = self.now_tick;
        let tail = self.tail;
        let n_c = self.guilds.find_guild_mut(&name).map_or(-1, |g| {
            g.update_rank(tail, cfg, now_tick, |n| {
                find_play_object(online, world, n).is_some()
            })
        });
        if n_c == 0 {
            self.events.push(GuildServerEvent::ServerGroupMsg {
                ident: messages::SS_207,
                guild: name.clone(),
            });
            // `ClientGuildMemberList()`：N3 已把 ranks 清空 ⇒ 文本空 ⇒ **0 帧**（§0.1）。
            self.member_list();
        } else if n_c <= -2 {
            self.send_self(guild_frames::sm_guild_rank_update_fail_761(n_c));
        }
        // `-1`：0 帧；**会话 rank_no/rank_name 不动**（oracle 不清）。
    }

    // ---- 8. 1044 ----

    fn ally(&mut self) {
        let mut n8: i32 = -1;
        // `GetPoseCreate()`：掌门面前的 actor；非玩家 ⇒ 保持 -1。
        if let Some(pose) = crate::pose_create(self.world, self.displays, self.me.actor_id) {
            if crate::is_play_actor(self.world, pose) {
                if let Some(t) = self.online.iter().find(|e| e.actor_id == pose) {
                    let target_chr = t.chr_name.clone();
                    // `posePlayer.MyGuild != null && posePlayer.GetPoseCreate() == this`。
                    if self.target_guild_name(&target_chr).is_some()
                        && crate::pose_create(self.world, self.displays, pose)
                            == Some(self.me.actor_id)
                    {
                        // `if (posePlayer.MyGuild.EnableAuthAlly)`：**生产恒 false**
                        // （`GuildInfo.cs:71/118`；唯一取反点 `AuthallyCommand.cs:13` 被注释）
                        // ⇒ 恒 -4；掌门/战态门（-3/-2）与成功路径（768 + 双向 SS_207）
                        // **不可达、不建模**（简报 §1.2 #8）。
                        n8 = -4;
                    }
                }
            }
        }
        if n8 == 0 {
            self.send_self(guild_frames::sm_guild_make_ally_ok_768());
        } else {
            self.send_self(guild_frames::sm_guild_make_ally_fail_769(n8));
        }
    }

    // ---- 9. 1045 ----

    fn break_ally(&mut self) {
        if !self.is_guild_master() {
            // `Operate.cs:1527-1529`：非掌门静默（零帧）。
            return;
        }
        // `FindGuild(sGuildName)` 大小写敏感；`IsAllyGuild` 恒 false（`GuildAllList` 恒空，
        // `GuildInfo.cs:1016` 的 `Add` 被注释）⇒ 成功支（770）不可达。
        // 失败：`SendDefMessage(SM_GUILDMAKEALLY_FAIL, 0, 0, 0, 0)` —— **769/Recog=0，不是 771**。
        self.send_self(guild_frames::sm_guild_make_ally_fail_769(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    use game_svr::downlink::ActorDisplay;
    use mir2_protocol::edcode;
    use mir2_world::{Entity, World, ACTOR_RACE_PLAY};

    const GM: &str = "掌门";
    const MEMBER: &str = "成员";
    const BYSTANDER: &str = "路人";
    const GUILD: &str = "测试会";
    const GM_SOCKET: i32 = 101;
    const MEMBER_SOCKET: i32 = 102;
    const BYSTANDER_SOCKET: i32 = 103;
    /// `[Setup] GreenMsgFColor/BColor` 缺省（部署件未覆盖）。
    const GREEN: (u8, u8) = (0xDB, 0xFF);
    /// `[Setup] RedMsgFColor/BColor` 缺省。
    const RED: (u8, u8) = (0xFF, 0x38);

    fn recog(b: &[u8]) -> i32 {
        i32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
    fn ident(b: &[u8]) -> u16 {
        u16::from_le_bytes([b[4], b[5]])
    }
    fn param(b: &[u8]) -> u16 {
        u16::from_le_bytes([b[6], b[7]])
    }
    fn tag(b: &[u8]) -> u16 {
        u16::from_le_bytes([b[8], b[9]])
    }
    fn series(b: &[u8]) -> u16 {
        u16::from_le_bytes([b[10], b[11]])
    }
    fn gbk_tail(b: &[u8]) -> String {
        let raw = edcode::decode(&b[12..]);
        encoding_rs::GBK.decode(&raw).0.into_owned()
    }

    fn add_player(world: &mut World, name: &str, x: i32, y: i32, dir: u8) -> i32 {
        let mut e = Entity::new(0, name.to_string(), x, y);
        e.race = ACTOR_RACE_PLAY;
        e.ai.dir = dir;
        world.add_entity(e)
    }

    struct Rig {
        online: OnlineRegistry,
        world: World,
        displays: DisplayTable,
        gates: HashMap<u8, UnboundedSender<SimToGate>>,
        rx: mpsc::UnboundedReceiver<SimToGate>,
        sessions: HashMap<String, GuildSession>,
        guilds: GuildManager,
        cfg: GuildConfig,
        ids: HashMap<String, i32>,
        now: i64,
    }

    impl Rig {
        fn new() -> Self {
            let mut world = World::with_maps(vec![(20, 20)]);
            // 掌门 (5,5) 面右 ⇒ 面前 (6,5)；成员 (6,5) 面左 ⇒ 面前 (5,5)（互看）。
            let gm = add_player(&mut world, GM, 5, 5, 2);
            let member = add_player(&mut world, MEMBER, 6, 5, 6);
            let bystander = add_player(&mut world, BYSTANDER, 5, 6, 0);
            let mut displays = DisplayTable::new();
            let mut online: OnlineRegistry = Vec::new();
            for (i, (name, id, socket)) in [
                (GM, gm, GM_SOCKET),
                (MEMBER, member, MEMBER_SOCKET),
                (BYSTANDER, bystander, BYSTANDER_SOCKET),
            ]
            .into_iter()
            .enumerate()
            {
                displays.set(
                    id,
                    ActorDisplay::new(0, 0, 0, 0, 0, 255, downlink::gbk(name)),
                );
                online.push(OnlineEntry {
                    account: name.to_string(),
                    chr_name: name.to_string(),
                    session_id: i as i32 + 1,
                    gate_idx: 0,
                    socket,
                    actor_id: id,
                });
            }
            let (tx, rx) = mpsc::unbounded_channel();
            let mut gates = HashMap::new();
            gates.insert(0_u8, tx);
            let mut sessions = HashMap::new();
            for name in [GM, MEMBER, BYSTANDER] {
                sessions.insert(name.to_string(), GuildSession::default());
            }
            Self {
                online,
                world,
                displays,
                gates,
                rx,
                sessions,
                guilds: GuildManager::new(),
                cfg: GuildConfig::default(),
                ids: [
                    (GM.to_string(), gm),
                    (MEMBER.to_string(), member),
                    (BYSTANDER.to_string(), bystander),
                ]
                .into_iter()
                .collect(),
                now: 12_345,
            }
        }

        fn actor(&self, name: &str) -> i32 {
            self.ids[name]
        }

        /// 建会并把会话置为掌门（`rank_no = 1`）。
        fn give_guild(&mut self) {
            assert!(self.guilds.add_guild(GUILD, GM, &self.cfg));
            let s = self.sessions.get_mut(GM).unwrap();
            s.guild_name = Some(GUILD.to_string());
            s.rank_no = 1;
            s.rank_name = self.cfg.guild_chief.clone();
        }

        fn add_member_to_guild(&mut self, name: &str) {
            let cfg = self.cfg.clone();
            self.guilds
                .find_guild_mut(GUILD)
                .expect("会必须存在")
                .add_member(name, &cfg, 0);
            let s = self.sessions.get_mut(name).unwrap();
            s.guild_name = Some(GUILD.to_string());
            s.rank_no = mir2_world::guild::GUILD_RANK_NO_MEMBER;
            s.rank_name = self.cfg.guild_member_rank.clone();
        }

        fn cmd(&mut self, ident: u16, tail: &str) -> Vec<GuildServerEvent> {
            let master = GM.to_string();
            handle_guild_cmd(
                ident,
                Some(tail),
                &master,
                GM_SOCKET,
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
                self.now,
            )
        }

        fn drain(&mut self) -> Vec<(i32, Vec<u8>)> {
            let mut out = Vec::new();
            while let Ok(ev) = self.rx.try_recv() {
                match ev {
                    SimToGate::SendFrame { socket, body } => out.push((socket, body)),
                    other => panic!("行会派发不应产出 {other:?}"),
                }
            }
            out
        }

        /// 取某 socket 上的帧（保持发送序）。
        fn at(frames: &[(i32, Vec<u8>)], socket: i32) -> Vec<Vec<u8>> {
            frames
                .iter()
                .filter(|(s, _)| *s == socket)
                .map(|(_, b)| b.clone())
                .collect()
        }
    }

    fn idents(frames: &[Vec<u8>]) -> Vec<u16> {
        frames.iter().map(|b| ident(b)).collect()
    }

    // ---- T2: 1035/1036 面板 ----

    /// 1035 有会：753 的完整字节 = `MakeMessage(753,0,0,0,1)` + `EncodeString(panel)`；
    /// 1036 = 1035（`ClientGuildHome` 直接调 `ClientOpenGuildDlg`）。
    /// 牙：`\r \r` 改成 `\r\r`（world 侧用例）、Series 改 0、无会回 753 ⇒ 红。
    #[test]
    fn g2_open_dlg_panel_exact_and_home_isomorphic() {
        let mut rig = Rig::new();
        rig.give_guild();
        let ev = rig.cmd(messages::CM_OPENGUILDDLG, "");
        assert!(ev.is_empty());
        let frames = rig.drain();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0, GM_SOCKET);
        let body = &frames[0].1;
        let panel = format!("{GUILD}\r \r1\r<Notice>\r<KillGuilds>\r<AllyGuilds>\r");
        let mut expected = Vec::new();
        expected.extend_from_slice(&crate::smframe::command_header(
            0,
            messages::SM_OPENGUILDDLG,
            0,
            0,
            1,
        ));
        expected.extend_from_slice(&edcode::encode(&downlink::gbk(&panel)));
        assert_eq!(body, &expected, "753 面板逐字节");
        assert_eq!(recog(body), 0);
        assert_eq!(param(body), 0);
        assert_eq!(tag(body), 0);
        assert_eq!(series(body), 1);
        assert_eq!(gbk_tail(body), panel);

        // 1036 与 1035 完全同构（同一 handler）。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.cmd(messages::CM_GUILDHOME, "");
        let home = rig.drain();
        assert_eq!(home.len(), 1);
        assert_eq!(home[0].1, expected, "1036 ≡ 1035");

        // 无会 ⇒ 754 纯头（不是 753）。
        let mut rig = Rig::new();
        rig.cmd(messages::CM_OPENGUILDDLG, "");
        let frames = rig.drain();
        assert_eq!(frames.len(), 1);
        assert_eq!(ident(&frames[0].1), messages::SM_OPENGUILDDLG_FAIL);
        assert_eq!(frames[0].1.len(), 12, "754 是纯头");
        assert_eq!(frames[0].1, crate::smframe::command_header(0, 754, 0, 0, 0));
    }

    // ---- T3: 1037 ----

    /// 1037：无会 ⇒ 0 帧；有会但 ranks 空 ⇒ 0 帧；有成员 ⇒ 756（Series=1 + 文本）。
    /// 牙：无会回 756；空文本仍发 756 ⇒ 红。
    #[test]
    fn g2_member_list_empty_gate() {
        let mut rig = Rig::new();
        rig.cmd(messages::CM_GUILDMEMBERLIST, "");
        assert!(rig.drain().is_empty(), "无会必须零帧");

        // 有会但 ranks 空（N3 后的形态）：文本空 ⇒ `SendSocket(defMsg, "")` 整帧不发。
        rig.guilds.guilds.push(mir2_world::GuildState::new("空会"));
        rig.sessions.get_mut(GM).unwrap().guild_name = Some("空会".to_string());
        rig.cmd(messages::CM_GUILDMEMBERLIST, "");
        assert!(rig.drain().is_empty(), "ranks 空必须零帧");

        // 正常会：`#1/*掌门人/掌门/`。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.cmd(messages::CM_GUILDMEMBERLIST, "");
        let frames = rig.drain();
        assert_eq!(frames.len(), 1);
        let body = &frames[0].1;
        assert_eq!(ident(body), messages::SM_SENDGUILDMEMBERLIST);
        assert_eq!(series(body), 1);
        assert_eq!(gbk_tail(body), "#1/*掌门人/掌门/");
    }

    // ---- T4: 1038 ----

    /// 1038 的失败码 1/2/2/3/4/4/5（顺序即 oracle 守卫序）+ 成功链（夹具置 `allow_guild=true`）。
    /// 牙：`AllowGuild` 检查挪到 `add_member` 后；rank 门改成成员数；目标 750 加尾/漏 42 ⇒ 红。
    #[test]
    fn g2_add_member_codes_and_order() {
        // 1 非掌门。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(GM).unwrap().rank_no = 99;
        rig.cmd(messages::CM_GUILDADDMEMBER, MEMBER);
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDADDMEMBER_FAIL, 1)
        );

        // 2 目标不在线（GetPlayObject == null）。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(MEMBER).unwrap().allow_guild = true;
        rig.cmd(messages::CM_GUILDADDMEMBER, "不存在");
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDADDMEMBER_FAIL, 2)
        );

        // 2 没面对（成员面右，不朝掌门）。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(MEMBER).unwrap().allow_guild = true;
        rig.world
            .entities
            .get_mut(rig.actor(MEMBER))
            .unwrap()
            .ai
            .dir = 2;
        rig.cmd(messages::CM_GUILDADDMEMBER, MEMBER);
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDADDMEMBER_FAIL, 2)
        );

        // 5 AllowGuild=false（ctor 默认，生产恒走这支）。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.cmd(messages::CM_GUILDADDMEMBER, MEMBER);
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDADDMEMBER_FAIL, 5)
        );
        assert!(rig.sessions[MEMBER].guild_name.is_none(), "失败零状态变化");

        // 3 已在本会（大小写敏感 `==`；用规范名）。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(MEMBER).unwrap().allow_guild = true;
        rig.add_member_to_guild(MEMBER);
        rig.cmd(messages::CM_GUILDADDMEMBER, MEMBER);
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDADDMEMBER_FAIL, 3)
        );

        // 4 已在别的会。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(MEMBER).unwrap().allow_guild = true;
        rig.sessions.get_mut(MEMBER).unwrap().guild_name = Some("别会".to_string());
        rig.cmd(messages::CM_GUILDADDMEMBER, MEMBER);
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDADDMEMBER_FAIL, 4)
        );

        // 4 rank 数 >= 400（`GUILD_RANK_LIST_LIMIT`）。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(MEMBER).unwrap().allow_guild = true;
        {
            let g = rig.guilds.find_guild_mut(GUILD).unwrap();
            for i in 0..mir2_world::guild::GUILD_RANK_LIST_LIMIT {
                g.ranks.push(mir2_world::GuildRank {
                    rank_no: (i % 99 + 1) as i16,
                    rank_name: format!("r{i}"),
                    members: Vec::new(),
                });
            }
        }
        rig.cmd(messages::CM_GUILDADDMEMBER, MEMBER);
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDADDMEMBER_FAIL, 4)
        );

        // 成功：757 直发在先；目标 750(会名/行会成员) → 42 → 100(Green/Hint)；SS_207。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(MEMBER).unwrap().allow_guild = true;
        let ev = rig.cmd(messages::CM_GUILDADDMEMBER, MEMBER);
        assert_eq!(
            ev,
            vec![GuildServerEvent::ServerGroupMsg {
                ident: messages::SS_207,
                guild: GUILD.to_string()
            }]
        );
        let frames = rig.drain();
        let gm = Rig::at(&frames, GM_SOCKET);
        assert_eq!(
            gm[0],
            crate::smframe::command_header(0, 757, 0, 0, 0),
            "757 纯头"
        );
        assert_eq!(idents(&gm), vec![757, 42], "掌门 socket：757 直发先于 42");
        let mb = Rig::at(&frames, MEMBER_SOCKET);
        assert_eq!(idents(&mb), vec![750, 42, 100], "目标：750 → 42 → 100");
        assert_eq!(recog(&mb[0]), 0);
        assert_eq!(recog(&mb[1]), rig.actor(MEMBER), "42 的 Recog = 被展示者");
        assert_eq!(gbk_tail(&mb[0]), "测试会/行会成员");
        assert_eq!(recog(&mb[2]), rig.actor(MEMBER));
        assert_eq!(param(&mb[2]), u16::from_le_bytes([GREEN.0, GREEN.1]));
        assert_eq!(series(&mb[2]), 1);
        assert_eq!(
            gbk_tail(&mb[2]),
            "你已加入行会: 测试会 当前封号为: 行会成员"
        );
        // 状态：rank 99 + 会话 + 成员表。
        let s = &rig.sessions[MEMBER];
        assert_eq!(s.guild_name.as_deref(), Some(GUILD));
        assert_eq!(s.rank_no, 99);
        assert_eq!(s.rank_name, "行会成员");
        assert!(rig.guilds.find_guild(GUILD).unwrap().is_member(MEMBER));
    }

    // ---- T5: 1039 ----

    /// 1039 删他人：759 先出 → 目标 750 纯头 → 42 → SS_207；会话清空、成员移除。
    /// 牙：目标 750 带尾；漏 42；DelMember 后不清会话 ⇒ 红。
    #[test]
    fn g2_del_member_other() {
        let mut rig = Rig::new();
        rig.give_guild();
        rig.add_member_to_guild(MEMBER);
        let ev = rig.cmd(messages::CM_GUILDDELMEMBER, MEMBER);
        assert_eq!(
            ev,
            vec![GuildServerEvent::ServerGroupMsg {
                ident: messages::SS_207,
                guild: GUILD.to_string()
            }]
        );
        let frames = rig.drain();
        let gm = Rig::at(&frames, GM_SOCKET);
        assert_eq!(idents(&gm), vec![759, 42], "掌门 socket：759 直发先于 42");
        assert_eq!(gm[0], crate::smframe::command_header(0, 759, 0, 0, 0));
        let mb = Rig::at(&frames, MEMBER_SOCKET);
        assert_eq!(idents(&mb), vec![750, 42]);
        assert_eq!(mb[0].len(), 12, "RefRankInfo 的无会 750 是纯头");
        assert_eq!(recog(&mb[1]), rig.actor(MEMBER));
        assert!(!rig.guilds.find_guild(GUILD).unwrap().is_member(MEMBER));
        assert!(rig.sessions[MEMBER].guild_name.is_none());
        assert_eq!(rig.sessions[MEMBER].rank_no, 0);
    }

    /// 1039 自删（单人会）：759 先出 → 750 纯头 → 42 → 100(Red)；会表少一会、SS_206。
    /// 牙：删掉 DelGuild；自删发 760/4；100 写成 Green ⇒ 红。
    #[test]
    fn g2_self_disband_single_member_guild() {
        let mut rig = Rig::new();
        rig.give_guild();
        let ev = rig.cmd(messages::CM_GUILDDELMEMBER, GM);
        assert_eq!(
            ev,
            vec![GuildServerEvent::ServerGroupMsg {
                ident: messages::SS_206,
                guild: GUILD.to_string()
            }]
        );
        let frames = rig.drain();
        let gm = Rig::at(&frames, GM_SOCKET);
        assert_eq!(idents(&gm), vec![759, 750, 42, 100], "759 先于排队帧");
        assert_eq!(gm[0], crate::smframe::command_header(0, 759, 0, 0, 0));
        assert_eq!(gm[1].len(), 12, "自删的 750 是纯头");
        assert_eq!(recog(&gm[3]), rig.actor(GM));
        assert_eq!(
            param(&gm[3]),
            u16::from_le_bytes([RED.0, RED.1]),
            "自删是 Red"
        );
        assert_eq!(gbk_tail(&gm[3]), format!("行会{GUILD}已被取消!!!"));
        assert!(rig.guilds.find_guild(GUILD).is_none(), "DelGuild 必须执行");
        assert!(rig.sessions[GM].guild_name.is_none());

        // 自删多人会：CancelGuld 返回 false ⇒ 760/3（不是 4，也不是 0）。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.add_member_to_guild(MEMBER);
        rig.cmd(messages::CM_GUILDDELMEMBER, GM);
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDDELMEMBER_FAIL, 3)
        );
        assert!(rig.guilds.find_guild(GUILD).is_some(), "不能解散多人会");
    }

    /// 1039 失败码 1/2：非掌门、非成员（含大小写/尾随字符不同的名字）。
    #[test]
    fn g2_del_member_codes() {
        // 1 非掌门。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(GM).unwrap().rank_no = 99;
        rig.cmd(messages::CM_GUILDDELMEMBER, MEMBER);
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDDELMEMBER_FAIL, 1)
        );

        // 2 不是本会成员。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.cmd(messages::CM_GUILDDELMEMBER, MEMBER);
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDDELMEMBER_FAIL, 2)
        );

        // 成员名带尾随空格：`IsMember` 与 `DelMember` 都是大小写敏感的 `==` ⇒ 走 2。
        // （`nC=4` 只在两者观察不同时出现，当前同源 ⇒ 不可构造，登记。）
        let mut rig = Rig::new();
        rig.give_guild();
        rig.add_member_to_guild(MEMBER);
        rig.cmd(messages::CM_GUILDDELMEMBER, "成员 ");
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDDELMEMBER_FAIL, 2)
        );
    }

    // ---- T6: 1040 ----

    /// 1040：公告分段 + 重开 753；非掌门/无会静默 0 帧。
    /// 牙：守卫失败回 754；朴素 split 保留空段 ⇒ 红。
    #[test]
    fn g2_notice_segments_and_reopen() {
        let mut rig = Rig::new();
        rig.give_guild();
        let ev = rig.cmd(messages::CM_GUILDUPDATENOTICE, "公告一\r\r公告二\r");
        assert_eq!(ev.len(), 1);
        let frames = rig.drain();
        assert_eq!(frames.len(), 1, "成功只重开面板（SS_207 不产帧）");
        assert_eq!(ident(&frames[0].1), messages::SM_OPENGUILDDLG);
        assert_eq!(
            rig.guilds.find_guild(GUILD).unwrap().notices,
            vec!["公告一".to_string(), "公告二".to_string()]
        );
        assert!(gbk_tail(&frames[0].1).contains("<Notice>\r公告一\r公告二\r<KillGuilds>"));

        // 空公告：NoticeList 清空，753 仍重开。
        rig.cmd(messages::CM_GUILDUPDATENOTICE, "");
        assert!(rig.guilds.find_guild(GUILD).unwrap().notices.is_empty());
        let frames = rig.drain();
        assert_eq!(ident(&frames[0].1), messages::SM_OPENGUILDDLG);

        // 非掌门：0 帧、公告不变。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(GM).unwrap().rank_no = 99;
        rig.cmd(messages::CM_GUILDUPDATENOTICE, "x");
        assert!(rig.drain().is_empty());
        assert!(rig.guilds.find_guild(GUILD).unwrap().notices.is_empty());

        // 无会：0 帧。
        let mut rig = Rig::new();
        rig.cmd(messages::CM_GUILDUPDATENOTICE, "x");
        assert!(rig.drain().is_empty());
    }

    // ---- T7: 1041 ----

    /// 1041 成功（N3）：ranks 清空、**0 帧**、SS_207，会话 rank 不动。
    /// 牙：期待 756；「修好」N3 改成 `ranks=parsed` ⇒ 红。
    #[test]
    fn g2_update_rank_n3_zero_frames() {
        let mut rig = Rig::new();
        rig.give_guild();
        let ev = rig.cmd(messages::CM_GUILDUPDATERANKINFO, "#1 新封号\r掌门\r");
        assert_eq!(
            ev,
            vec![GuildServerEvent::ServerGroupMsg {
                ident: messages::SS_207,
                guild: GUILD.to_string()
            }]
        );
        assert!(rig.drain().is_empty(), "nC==0 ⇒ 0 帧（不是 756）");
        assert!(
            rig.guilds.find_guild(GUILD).unwrap().ranks.is_empty(),
            "N3 清空"
        );
        // 会话 rank 不动（oracle 不清）。
        assert_eq!(rig.sessions[GM].rank_no, 1);
        assert_eq!(rig.sessions[GM].rank_name, "掌门人");
    }

    /// 1041 `-1`（无变化）与 `<= -2`（761 带负 Recog）与守卫。
    #[test]
    fn g2_update_rank_negative_codes() {
        let mut rig = Rig::new();
        rig.give_guild();
        // 与当前表完全一致 ⇒ -1，0 帧。
        rig.cmd(messages::CM_GUILDUPDATERANKINFO, "#1 掌门人\r掌门\r");
        assert!(rig.drain().is_empty(), "-1 必须 0 帧");
        assert_eq!(rig.guilds.find_guild(GUILD).unwrap().ranks.len(), 1);

        // 首个 rank 的 RankNo != 1 ⇒ -2 ⇒ 761/-2。
        rig.cmd(messages::CM_GUILDUPDATERANKINFO, "#99 成员\r掌门\r");
        let f = rig.drain();
        assert_eq!(ident(&f[0].1), messages::SM_GUILDRANKUPDATE_FAIL);
        assert_eq!(recog(&f[0].1), -2);
        assert_eq!(f[0].1.len(), 12);
        assert_eq!(
            rig.guilds.find_guild(GUILD).unwrap().ranks.len(),
            1,
            "-2 不改状态"
        );

        // 非掌门 ⇒ 0 帧。
        rig.sessions.get_mut(GM).unwrap().rank_no = 99;
        rig.cmd(messages::CM_GUILDUPDATERANKINFO, "#1 新封号\r掌门\r");
        assert!(rig.drain().is_empty());
    }

    // ---- T8: 1044/1045 ----

    /// 1044：双方掌门面对面 ⇒ 769/-4（EnableAuthAlly 恒 false）；无会/没回看 ⇒ 769/-1；
    /// 768 任何输入都不出现。
    #[test]
    fn g2_ally_unreachable_paths() {
        // 目标无会 ⇒ -1。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.cmd(messages::CM_GUILDALLY, "");
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDMAKEALLY_FAIL, -1)
        );

        // 目标有会但没回看（成员面右）⇒ -1。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(MEMBER).unwrap().guild_name = Some("别会".to_string());
        rig.world
            .entities
            .get_mut(rig.actor(MEMBER))
            .unwrap()
            .ai
            .dir = 2;
        rig.cmd(messages::CM_GUILDALLY, "");
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDMAKEALLY_FAIL, -1)
        );

        // 双方有会 + 互看 ⇒ EnableAuthAlly=false ⇒ -4。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(MEMBER).unwrap().guild_name = Some("别会".to_string());
        rig.cmd(messages::CM_GUILDALLY, "");
        let f = rig.drain();
        assert_eq!(
            (ident(&f[0].1), recog(&f[0].1)),
            (messages::SM_GUILDMAKEALLY_FAIL, -4)
        );
        assert!(!idents(&[f[0].1.clone()]).contains(&messages::SM_GUILDMAKEALLY_OK));
    }

    /// 1045：掌门恒 `769/0`（不是 771）；非掌门 0 帧；770/771 永不出现。
    #[test]
    fn g2_break_ally_769_zero() {
        let mut rig = Rig::new();
        rig.give_guild();
        rig.cmd(messages::CM_GUILDBREAKALLY, "别会");
        let f = rig.drain();
        assert_eq!(f.len(), 1);
        assert_eq!(ident(&f[0].1), messages::SM_GUILDMAKEALLY_FAIL);
        assert_eq!(recog(&f[0].1), 0, "BreakAlly 失败 = 769/0，不是 771");
        assert!(!idents(&[f[0].1.clone()]).contains(&messages::SM_GUILDBREAKALLY_OK));

        // 非掌门：0 帧。
        let mut rig = Rig::new();
        rig.give_guild();
        rig.sessions.get_mut(GM).unwrap().rank_no = 99;
        rig.cmd(messages::CM_GUILDBREAKALLY, "别会");
        assert!(rig.drain().is_empty());
    }

    // ---- T10: 会话归属/默认值/物化 ----

    /// 会话默认值（`PlayObject.cs:903/1007`）：`allow=false`、`ban=true`；
    /// 物化 rank 1 / 99 两例；非成员 ⇒ 默认。
    /// 牙：默认 allow=true；从 Entity 读 guild ⇒ 红（本仓 Entity 无行会字段）。
    #[test]
    fn g2_session_defaults_and_materialize() {
        let d = GuildSession::default();
        assert!(!d.allow_guild, "AllowGuild ctor=false");
        assert!(d.ban_guild_chat, "BanGuildChat ctor=true（true=允许）");
        assert!(d.guild_name.is_none());
        assert_eq!(d.rank_no, 0);
        assert!(d.rank_name.is_empty());

        let cfg = GuildConfig::default();
        let mut guilds = GuildManager::new();
        assert!(guilds.add_guild(GUILD, GM, &cfg));
        guilds
            .find_guild_mut(GUILD)
            .unwrap()
            .add_member(MEMBER, &cfg, 0);

        let gm = materialize(&guilds, GM);
        assert_eq!(gm.guild_name.as_deref(), Some(GUILD));
        assert_eq!(gm.rank_no, 1);
        assert_eq!(gm.rank_name, cfg.guild_chief);
        let member = materialize(&guilds, MEMBER);
        assert_eq!(member.rank_no, 99);
        assert_eq!(member.rank_name, cfg.guild_member_rank);
        assert_eq!(materialize(&guilds, BYSTANDER), GuildSession::default());
    }

    // ---- 派发面 ----

    /// 显式 9 条：含 1035..1041/1044/1045，**不含** 1042（非法数据）/1043（adjust bonus）。
    /// 牙：改成 `1035..=1045` 区间 ⇒ 本用例红。
    #[test]
    fn g2_dispatch_surface_is_explicit_nine() {
        assert_eq!(GUILD_IDENTS.len(), 9);
        let set: std::collections::BTreeSet<u16> = GUILD_IDENTS.iter().copied().collect();
        assert_eq!(set.len(), 9, "不得重复");
        for id in [1035u16, 1036, 1037, 1038, 1039, 1040, 1041, 1044, 1045] {
            assert!(set.contains(&id), "缺 CM_{id}");
        }
        assert!(!set.contains(&1042), "1042 不进行会臂");
        assert!(!set.contains(&1043), "1043 不进行会臂");
    }
}
