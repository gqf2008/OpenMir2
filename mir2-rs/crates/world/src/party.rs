//! 组队（party）会话状态机 —— M3⑦-实现 2（卡 `openmir2-m3-party-group-state`）。
//!
//! oracle 锚点（逐条在本片复核于本仓库 `src/`，行号为实测）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`PartyConfig`] | 组队配置（⑦-1 的 `crates/data/src/party.rs`）+ `GameSvrConf.cs:1544-1545/1844`（颜色/前缀）+ `MessageSettings.cs:215`（`JoinGroup`） |
//! | [`PartyEvent`] + [`World::party_group_mode`] | `PlayObject.Message.cs:914-929`（`CM_GROUPMODE`） |
//! | [`World::party_create_group`] | `PlayObject.Operate.cs:810-845`（`ClientCreateGroup`） |
//! | [`World::party_add_group_member`] | `PlayObject.Operate.cs:847-883`（`ClientAddGroupMember`，`GroupMembers.Count > Max` **严格大于** `:855`） |
//! | [`World::party_del_group_member`] | `PlayObject.Operate.cs:885-909`（`ClientDelGroupMember`） |
//! | [`World::party_del_member`] | `PlayObject.cs:5126-5156`（`DelMember` + `CancelGroup`）+ `PlayObject.Base.cs:1339-1369`（`CancelGroup`/`SendGroupMembers`） |
//! | [`World::party_leave_group`] | `PlayObject.cs:5177-5183`（`LeaveGroup`） |
//! | [`World::party_group_close`] | `PlayObject.Operate.cs:787-808`（`ClientGroupClose`） |
//! | [`World::party_run_tick`] | `PlayObject.Message.cs:704-719`（`Run()` 每 tick 清死亡成员 / 团长死亡散组） |
//! | [`World::party_join_group`] | `PlayObject.Base.cs:2418-2422`（`JoinGroup`） |
//! | [`World::party_send_group_members`] | `PlayObject.Base.cs:1353-1369`（`SendGroupMembers`） |
//! | [`World::party_send_group_text`] | `PlayObject.cs:5185-5197`（`SendGroupText`） |
//!
//! ## 帧面口径（为什么是「立即 / 排队」两桶）
//!
//! oracle `BaseObject.Run()`（`BaseObject.Base.cs:28-31`）每次 `while (GetMessage(...)) Operate(...)`：
//! `SendDefMessage`/`SendSocket` 走网关缓冲**立即**发出，而 `SendMsg(RM_*)` 入 `MsgQueue`
//! （`BaseObject.Message.cs:94`，优先级 = `(byte)wIdent`），在**同一** `Run()` 内按优先级**升序**出队
//! （`GetMessage` 取最小优先级字节）。
//! ⇒ 同一目标的线上序 = **先立即帧（源码调用序）→ 再排队帧（优先级字节升序）**。
//! 本模块用 [`Emitter`] 的两桶精确复刻这一点（`SM_GROUPCANCEL` 既可能是立即帧，
//! 也可能由 `RM_GROUPCANCEL` 排队而来，取决于调用路径）。
//!
//! 三条 `RM_*` 的优先级字节（`(byte)ident`）：
//! - `RM_GROUPCANCEL(10140)` ⇒ `156`（`PlayObject.cs:5182`）
//! - `RM_SYSMESSAGE(11004)` ⇒ `252`（`BaseObject.cs:1682` 的 `SysMsg`）
//! - `RM_GROUPMESSAGE(11005)` ⇒ `253`（`PlayObject.cs:5193`）
//!
//! 出队映射（`PlayObject.Message.cs:1697-1724` / `:1848-1850`）：
//! `RM_GROUPMESSAGE ⇒ SM_SYSMESSAGE(100)`、`RM_SYSMESSAGE ⇒ SM_SYSMESSAGE(100)`、
//! `RM_GROUPCANCEL ⇒ SM_GROUPCANCEL(666)`；三者尾均为 `EDCode.EncodeString`（`SendSocket(ClientMsg, ...)`）。
//!
//! ## 已接线：`Die()` 的死亡退组（F-1 收口）
//! oracle `PlayObject.Die()`（`src/M2Server/Player/PlayObject.cs:1964-1978`）在置 `Death` 后判
//! `if (GroupOwner != 0) groupOwnerPlay.DelMember(this)`（`:1975-1977`）⇒ 死亡者立即退组；
//! 团长死亡 ⇒ 逐成员 `LeaveGroup()` 后整组散。本仓在 `world.rs::settle_deaths`（`hp==0` 的
//! 写入点，即 oracle `Die()` 的等价处）调用 [`World::party_del_member`]，事件落
//! `TickRecord::party_events`，由宿主逐条回帧。
//!
//! ## 已接线：`FunctionNPC.GotoLable`（`openmir2-m3-party-gotolabel-ext`）
//! oracle `Operate.cs:806/843/881/907` 在四条组队成功/`GroupOwner != 0` 分支尾调
//! `SystemShare.FunctionNPC.GotoLable(this, "@GroupClose"/"@GroupCreate"/"@GroupAddMember"/"@GroupDelMember", false)`。
//! 本仓在对应写入点把 [`PartyEvent::GotoLabel`] 按 `RM_MERCHANTSAY`（优先级字节 `1`，最低）入
//! **排队桶**，宿主 drain 时在**该位置**执行 `QFunction` 脚本（脚本说词经 `RM_MERCHANTSAY` 下发）。
//! ⇒ oracle 线上序「立即帧 → 排队帧（优先级升序，脚本说词最先）」无偏移。
//!
//! ## 未建模（登记，随 ⑦ 对齐窗口收口）
//! - `@GroupRecall` / `RECALLGROUPMEMBERS` / `CHECKGROUPCOUNT` / `ISGROUPMASTER` 脚本面（盘点 §1.O）：
//!   非 §6 核心，归可选卡。
//! - `AllowGroupReCall`（`PlayObject.cs:224`）字段已在 [`Entity`] 就位，但本模块无消费点（同上扩展面）。

use crate::entity::{Entity, ACTOR_RACE_PLAY};
use crate::world::World;

/// `GameSvrConf.cs:1402` 的 `GroupMembersMax` 初值（部署 `Server.conf:292` 同为 `10`）。
pub const PARTY_GROUP_MEMBERS_MAX_DEFAULT: u8 = 10;
/// `src/OpenMir2/Grobal2.cs:35` 的 `GroupMax`（组队经验 bonus 表的下标上界；`GainExp` 用它做下标门）。
pub const PARTY_GROUP_MAX: usize = 11;
/// `PlayObject.GainExp` 的 `bonus[12]`（`PlayObject.Base.cs:766`）。
pub const PARTY_EXP_BONUS: [f64; 12] = [1.0, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0, 2.1, 2.2];
/// `GameSvrConf.cs:1844` 的 `GroupMsgPreFix` 初值（`[String]` 缺键 ⇒ 默认）。
pub const PARTY_GROUP_MSG_PREFIX_DEFAULT: &str = "〖组队〗";
/// `GameSvrConf.cs:1544` 的 `GroupMsgFColor` 初值（部署 `Server.conf:608` 同为 `196`）。
pub const PARTY_GROUP_MSG_F_COLOR_DEFAULT: u8 = 0xC4;
/// `GameSvrConf.cs:1545` 的 `GroupMsgBColor` 初值（部署 `Server.conf:609` 同为 `255`）。
pub const PARTY_GROUP_MSG_B_COLOR_DEFAULT: u8 = 0xFF;
/// `MessageSettings.cs:215` 的 `JoinGroup`（`StringConf.cs:223` 的 `[String] JoinGroupMsg` 覆盖点）。
pub const PARTY_JOIN_GROUP_MSG_DEFAULT: &str = "{0} 已加入小组.";
/// `PlayObject.cs:5179` 的 `LeaveGroup` 文案（**硬编码**，非配置）。
pub const PARTY_EXIT_GROUP_MSG: &str = "{0} 已经退出了本组.";
/// `PlayObject.Base.cs:1341` 的 `CancelGroup` 文案（**硬编码**，非配置）。
pub const PARTY_CANCEL_GROUP_MSG: &str = "你的小组被解散了.";
/// `PlayObject.Operate.cs:804` 的 `ClientGroupClose` 团长分支文案（**硬编码**）。
pub const PARTY_LEADER_CLOSE_MSG: &str = "如果你想退出，使用编组功能（删除按钮）";

/// `SystemShare.FunctionNPC.GotoLable(this, "@GroupClose", false)`（`Operate.cs:806`）。
pub const PARTY_LABEL_GROUP_CLOSE: &str = "@GroupClose";
/// `SystemShare.FunctionNPC.GotoLable(this, "@GroupCreate", false)`（`Operate.cs:843`）。
pub const PARTY_LABEL_GROUP_CREATE: &str = "@GroupCreate";
/// `SystemShare.FunctionNPC.GotoLable(this, "@GroupAddMember", false)`（`Operate.cs:881`）。
pub const PARTY_LABEL_GROUP_ADD_MEMBER: &str = "@GroupAddMember";
/// `SystemShare.FunctionNPC.GotoLable(this, "@GroupDelMember", false)`（`Operate.cs:907`）。
pub const PARTY_LABEL_GROUP_DEL_MEMBER: &str = "@GroupDelMember";

/// `RM_GROUPCANCEL` 的队列优先级字节：`10140 & 0xFF = 156`。
const PRIO_RM_GROUPCANCEL: u8 = (10_140 & 0xFF) as u8;
/// `RM_SYSMESSAGE` 的队列优先级字节：`11004 & 0xFF = 252`。
const PRIO_RM_SYSMESSAGE: u8 = (11_004 & 0xFF) as u8;
/// `RM_GROUPMESSAGE` 的队列优先级字节：`11005 & 0xFF = 253`。
const PRIO_RM_GROUPMESSAGE: u8 = (11_005 & 0xFF) as u8;
/// `RM_MERCHANTSAY` 的队列优先级字节：`11009 & 0xFF = 1`（`NormNpc.cs:1615` 的 `SayMsg`）。
///
/// `FunctionNPC.GotoLable` 的脚本说词经此队列下发；**1 是最低优先级字节** ⇒ 同一 `Run()` 的
/// `while (GetMessage)` 出队里，脚本说词排在本模块其它排队帧（`RM_GROUPCANCEL` 156 /
/// `RM_SYSMESSAGE` 252 / `RM_GROUPMESSAGE` 253）**之前**。见 [`PartyEvent::GotoLabel`]。
const PRIO_RM_MERCHANTSAY: u8 = (11_009 & 0xFF) as u8;

/// 组队相关的运行时配置（host 从 `crates/data` 的 `PartyConfig` + `Server.conf` 的颜色/前缀/文案取）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartyConfig {
    /// `[Setup] GroupMembersMax`（`ServerConf.cs:225`）。
    pub group_members_max: u8,
    /// `[String] GroupMsgPreFix`（`StringConf.cs:39`）。
    pub group_msg_prefix: String,
    /// `[Setup] GroupMsgFColor`（`ServerConf.cs:337`）。
    pub group_msg_f_color: u8,
    /// `[Setup] GroupMsgBColor`（`ServerConf.cs:338`）。
    pub group_msg_b_color: u8,
    /// `[String] JoinGroupMsg`（`StringConf.cs:223`；`{0}` = 加入者 `ChrName`）。
    pub join_group_msg: String,
    /// `[Setup] RedMsgFColor`（`ServerConf.cs:333`）—— `ClientGroupClose` 团长提示的 `SysMsg(Red)` 用。
    pub red_msg_f_color: u8,
    /// `[Setup] RedMsgBColor`（`ServerConf.cs:334`）—— 同上。
    pub red_msg_b_color: u8,
}

impl Default for PartyConfig {
    fn default() -> Self {
        PartyConfig {
            group_members_max: PARTY_GROUP_MEMBERS_MAX_DEFAULT,
            group_msg_prefix: PARTY_GROUP_MSG_PREFIX_DEFAULT.to_string(),
            group_msg_f_color: PARTY_GROUP_MSG_F_COLOR_DEFAULT,
            group_msg_b_color: PARTY_GROUP_MSG_B_COLOR_DEFAULT,
            join_group_msg: PARTY_JOIN_GROUP_MSG_DEFAULT.to_string(),
            red_msg_f_color: 0xFF,
            red_msg_b_color: 0x38,
        }
    }
}

impl PartyConfig {
    /// `Format(MessageSettings.JoinGroup, ChrName)`（`PlayObject.Base.cs:2421`）。
    #[must_use]
    pub fn join_msg(&self, name: &str) -> String {
        self.join_group_msg.replace("{0}", name)
    }

    /// `Format(sExitGroupMsg, ChrName)`（`PlayObject.cs:5181`）。
    #[must_use]
    pub fn exit_msg(name: &str) -> String {
        PARTY_EXIT_GROUP_MSG.replace("{0}", name)
    }
}

/// 组队操作产生的**帧面事件**（顺序 = 见模块头「立即 / 排队」口径）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartyEvent {
    /// `SendDefMessage(SM_GROUPMODECHANGED(659), 0, param, 0, 0)`（`PlayObject.Message.cs:926/929`）。
    GroupModeChanged { target: i32, param: u16 },
    /// `SendDefMessage(SM_CREATEGROUP_OK(660), 0, 0, 0, 0)`（`Operate.cs:840`）。
    CreateGroupOk { target: i32 },
    /// `SendDefMessage(SM_CREATEGROUP_FAIL(661), recog, 0, 0, 0)`（`Operate.cs:816/820/825/830`）。
    CreateGroupFail { target: i32, recog: i32 },
    /// `SendDefMessage(SM_GROUPADDMEM_OK(662), 0, 0, 0, 0)`（`Operate.cs:878`）。
    AddMemberOk { target: i32 },
    /// `SendDefMessage(SM_GROUPADDMEM_FAIL(664), recog, 0, 0, 0)`（`Operate.cs:852/857/861/866/871`）。
    AddMemberFail { target: i32, recog: i32 },
    /// `SendDefMessage(SM_GROUPDELMEM_OK(663), 0, 0, 0, 0, name)`（`Operate.cs:904`）。
    DelMemberOk { target: i32, name: String },
    /// `SendDefMessage(SM_GROUPDELMEM_FAIL(665), recog, 0, 0, 0)`（`Operate.cs:890/894/900`）。
    DelMemberFail { target: i32, recog: i32 },
    /// `SendDefMessage(SM_GROUPMEMBERS(667), 0, 0, 0, 0, members_text)`（`Base.cs:1361-1366`）。
    GroupMembers { target: i32, members_text: String },
    /// `SendDefMessage(SM_GROUPCANCEL(666), 0, 0, 0, 0)`（立即：`PlayObject.cs:5150`；
    /// 排队：`RM_GROUPCANCEL` ⇒ `PlayObject.Message.cs:1848-1850`）。
    GroupCancel { target: i32 },
    /// `SendDefMessage(SM_SYSMESSAGE(100), recog, MakeWord(f,b), 0, 1, text)`：
    /// 组队说词（`RM_GROUPMESSAGE`）与 `SysMsg`（`RM_SYSMESSAGE`）。
    SysMessage {
        target: i32,
        recog: i32,
        f_color: u8,
        b_color: u8,
        text: String,
    },
    /// `SystemShare.FunctionNPC.GotoLable(this, label, false)`（`Operate.cs:806/843/881/907`）。
    ///
    /// **不是帧**：宿主收到本事件时按 oracle 执行 `QFunction` 脚本，脚本说词另经
    /// `RM_MERCHANTSAY`（优先级字节 `1`）下发 ⇒ 本事件排在 [`Emitter::finish`] 的**排队桶最前**
    /// （`em.later(PRIO_RM_MERCHANTSAY, ..)`），宿主 drain 时在此位置执行脚本即可复刻 oracle
    /// 「先立即帧 → 再排队帧按优先级升序（脚本说词最先）」的线上序。
    GotoLabel {
        /// 触发者 `ActorId`（oracle 的 `this`；CM 发送者）。
        target: i32,
        /// `@GroupClose`/`@GroupCreate`/`@GroupAddMember`/`@GroupDelMember`。
        label: &'static str,
    },
}

impl PartyEvent {
    /// 事件收件人 `ActorId`（oracle `MsgQueue` 的归属者 = 该 `PlayObject`）。
    ///
    /// 排队桶按它**分组建堆**：oracle 每人一条 `PriorityQueue`，跨收件人混堆会改变本会话的 sift。
    #[must_use]
    pub fn target(&self) -> i32 {
        match self {
            PartyEvent::GroupModeChanged { target, .. }
            | PartyEvent::CreateGroupOk { target }
            | PartyEvent::CreateGroupFail { target, .. }
            | PartyEvent::AddMemberOk { target }
            | PartyEvent::AddMemberFail { target, .. }
            | PartyEvent::DelMemberOk { target, .. }
            | PartyEvent::DelMemberFail { target, .. }
            | PartyEvent::GroupMembers { target, .. }
            | PartyEvent::GroupCancel { target }
            | PartyEvent::SysMessage { target, .. }
            | PartyEvent::GotoLabel { target, .. } => *target,
        }
    }
}

/// 「立即帧 + 排队帧」两桶（见模块头）。`finish` = 立即（插入序）++ 排队（**每收件人各自的四叉堆出队序**）。
#[derive(Default)]
struct Emitter {
    immediate: Vec<PartyEvent>,
    queued: Vec<(u8, PartyEvent)>,
}

impl Emitter {
    fn new() -> Self {
        Emitter::default()
    }

    fn now(&mut self, ev: PartyEvent) {
        self.immediate.push(ev);
    }

    fn later(&mut self, priority: u8, ev: PartyEvent) {
        self.queued.push((priority, ev));
    }

    /// 排队桶按 **oracle `MsgQueue`（每人一条队列）的四叉最小堆出队序**落成线性序。
    ///
    /// - 先**按收件人分组**（分组序 = 首次入队序）：`MsgQueue` 是 `PlayObject` 的实例字段
    ///   （`ActorEntity.cs:16`），跨收件人混堆会改变本会话的 sift；
    /// - 每组按入队序灌 [`crate::msgqueue::NetPriorityQueue`]（与宿主**同一份**实现）再出队。
    ///   **同优先级不是入队序**：3 条出队 `[0,2,1]`、4 条 `[0,3,2,1]`（dotnet 实跑）。
    ///
    /// 同 ident 多帧**可达**：团长死亡散组（`party_del_member_em` 的 `owner == target` 支）从尾到头
    /// 逐成员 `LeaveGroup()`，每次 `SendGroupText` 给**仍在组内**的每个成员各一条 `RM_GROUPMESSAGE`
    /// （253）⇒ 同一成员一次收多条。卡 `openmir2-frame-drain-order-audit`（此前是 `sort_by_key`
    /// 稳定排序 = 退化为入队序）。
    fn finish(self) -> Vec<PartyEvent> {
        let mut out = self.immediate;
        let mut buckets: Vec<(i32, Vec<(u8, PartyEvent)>)> = Vec::new();
        for q in self.queued {
            let target = q.1.target();
            match buckets.iter_mut().find(|(t, _)| *t == target) {
                Some((_, v)) => v.push(q),
                None => buckets.push((target, vec![q])),
            }
        }
        for (_, queue) in buckets {
            let mut heap: crate::msgqueue::NetPriorityQueue<(u8, PartyEvent)> =
                crate::msgqueue::NetPriorityQueue::new();
            for q in queue {
                let priority = q.0;
                heap.enqueue(q, priority);
            }
            while let Some((_, ev)) = heap.dequeue() {
                out.push(ev);
            }
        }
        out
    }
}

impl World {
    /// C# `SystemShare.WorldEngine.GetPlayObject(name)`（按 `ChrName` 线性扫 `PlayObjectList`）。
    ///
    /// oracle 只扫 `PlayObjectList`（**仅玩家**）；本仓实体表含怪物/NPC 且都带 `name`
    /// （reviewer-5 `fc68afca` 的 M-1：怪物/NPC 名与目标名重名时会误命中 ⇒ 走到 `-3/-4`，
    /// 而 oracle 判 `playObject==null` 回 `-2`）⇒ 这里按 `Race == Play` 过滤。
    ///
    /// 本仓玩家名唯一；返回首个匹配的 `ActorId`。遍历顺序 = 实体槽位序（确定性）。
    #[must_use]
    pub fn party_play_object_by_name(&self, name: &str) -> Option<i32> {
        self.entities
            .iter()
            // 卡 `openmir2-makeghost-lifecycle` M-2（矩阵 B13）：ghost 的世界对象仍可能存在（M-3 后），
            // 但组队/邀请按名找人**不该命中 ghost**（oracle 的 ghost 名字不再是对外可用的在线对象）。
            // **依据等级**：没有直接的 oracle 行号（矩阵 §8 已如实登记）⇒ 按卡面「举证不了就按需补过滤」处理。
            .find(|e| e.race == ACTOR_RACE_PLAY && e.name == name && !e.ghost)
            .map(|e| e.id)
    }

    fn party_group_owner_of(&self, id: i32) -> i32 {
        self.entities.get(id).map(|e| e.group_owner).unwrap_or(0)
    }

    fn party_members_of(&self, id: i32) -> Vec<i32> {
        self.entities
            .get(id)
            .map(|e| e.group_members.clone())
            .unwrap_or_default()
    }

    fn party_name_of(&self, id: i32) -> String {
        self.entities
            .get(id)
            .map(|e| e.name.clone())
            .unwrap_or_default()
    }

    fn party_is_dead_or_ghost(&self, id: i32) -> bool {
        match self.entities.get(id) {
            None => true,
            Some(e) => e.death || e.ghost,
        }
    }

    /// `PlayObject.cs:5185-5197` 的 `SendGroupText`：把 `prefix + sMsg` 逐成员入 `RM_GROUPMESSAGE`
    /// 队列（优先级 `253`）。`sender` 的 `GroupOwner` 为 `0` ⇒ 什么都不发。
    ///
    /// **宿主接缝**（`openmir2-channel-chat`）：`CM_SAY` 的 `!!` 支（`PlayObject.Chat.cs:167-169`）
    /// 在 `bins/game-svr` 直接调用本函数，把返回的事件按 [`crate::party::PartyEvent::SysMessage`]
    /// 编码成 `SM_SYSMESSAGE(100)` 下发；`recog` = 说话者 `ActorId`（oracle `SendMsg(this, …)`）。
    pub fn party_send_group_text(
        &self,
        sender: i32,
        cfg: &PartyConfig,
        s_msg: &str,
    ) -> Vec<(u8, PartyEvent)> {
        let mut out = Vec::new();
        let owner = self.party_group_owner_of(sender);
        if owner == 0 {
            return out;
        }
        let text = format!("{}{}", cfg.group_msg_prefix, s_msg);
        for m in self.party_members_of(owner) {
            out.push((
                PRIO_RM_GROUPMESSAGE,
                PartyEvent::SysMessage {
                    target: m,
                    recog: sender,
                    f_color: cfg.group_msg_f_color,
                    b_color: cfg.group_msg_b_color,
                    text: text.clone(),
                },
            ));
        }
        out
    }

    /// `PlayObject.Base.cs:1353-1369` 的 `SendGroupMembers`：拼 `a/b/c/` 后逐成员 `SendDefMessage`（立即）。
    fn party_send_group_members(&self, owner: i32) -> Vec<PartyEvent> {
        let members = self.party_members_of(owner);
        let mut text = String::new();
        for &m in &members {
            text.push_str(&self.party_name_of(m));
            text.push('/');
        }
        members
            .into_iter()
            .map(|m| PartyEvent::GroupMembers {
                target: m,
                members_text: text.clone(),
            })
            .collect()
    }

    /// `PlayObject.Base.cs:2418-2422` 的 `JoinGroup(playObject)` —— 在**加入者**上调用：
    /// `GroupOwner = 团长`，再 `SendGroupText(Format(JoinGroup, 本名))`。
    fn party_join_group_em(
        &mut self,
        member: i32,
        leader: i32,
        cfg: &PartyConfig,
        em: &mut Emitter,
    ) {
        if let Some(e) = self.entities.get_mut(member) {
            e.group_owner = leader;
        }
        let name = self.party_name_of(member);
        for (prio, ev) in self.party_send_group_text(member, cfg, &cfg.join_msg(&name)) {
            em.later(prio, ev);
        }
    }

    /// `PlayObject.cs:5177-5183` 的 `LeaveGroup` —— 在**退出者**上调用：
    /// 先 `SendGroupText(exit)`（此时 `GroupOwner` 仍指向团长），再清 `GroupOwner`，再 `RM_GROUPCANCEL`。
    fn party_leave_group_em(&mut self, member: i32, cfg: &PartyConfig, em: &mut Emitter) {
        let name = self.party_name_of(member);
        for (prio, ev) in self.party_send_group_text(member, cfg, &PartyConfig::exit_msg(&name)) {
            em.later(prio, ev);
        }
        if let Some(e) = self.entities.get_mut(member) {
            e.group_owner = 0;
        }
        em.later(
            PRIO_RM_GROUPCANCEL,
            PartyEvent::GroupCancel { target: member },
        );
    }

    /// `PlayObject.Base.cs:1339-1352` 的 `CancelGroup()` —— 在**团长**（`owner`）上调用。
    ///
    /// `GroupMembers.Count <= 1` ⇒ `SendGroupText("你的小组被解散了.")` + `Clear()` + `GroupOwner=0`，
    /// 返回 `false`（调用方据此发立即 `SM_GROUPCANCEL`）；否则返回 `true`（调用方发 `SendGroupMembers`）。
    fn party_cancel_group_em(&mut self, owner: i32, cfg: &PartyConfig, em: &mut Emitter) -> bool {
        let count = self.party_members_of(owner).len();
        if count <= 1 {
            for (prio, ev) in self.party_send_group_text(owner, cfg, PARTY_CANCEL_GROUP_MSG) {
                em.later(prio, ev);
            }
            if let Some(e) = self.entities.get_mut(owner) {
                e.group_members.clear();
                e.group_owner = 0;
            }
            em.now(PartyEvent::GroupCancel { target: owner });
            false
        } else {
            for ev in self.party_send_group_members(owner) {
                em.now(ev);
            }
            true
        }
    }

    /// `PlayObject.cs:5126-5156` 的 `DelMember(baseObject)` —— 在**团长**（`owner`）上调用。
    ///
    /// `owner == target` ⇒ 团长自己：从尾到头逐成员 `LeaveGroup()` + `RemoveAt`；
    /// 否则 ⇒ 找到该成员 `LeaveGroup()` + `RemoveAt`。随后 `CancelGroup()==false` ⇒ 立即
    /// `SM_GROUPCANCEL`，否则 `SendGroupMembers()`。
    fn party_del_member_em(
        &mut self,
        owner: i32,
        target: i32,
        cfg: &PartyConfig,
        em: &mut Emitter,
    ) {
        let members = self.party_members_of(owner);
        if owner != target {
            if let Some(pos) = members.iter().position(|&m| m == target) {
                self.party_leave_group_em(target, cfg, em);
                if let Some(e) = self.entities.get_mut(owner) {
                    if pos < e.group_members.len() {
                        e.group_members.remove(pos);
                    }
                }
            }
        } else {
            for &m in members.iter().rev() {
                self.party_leave_group_em(m, cfg, em);
                if let Some(e) = self.entities.get_mut(owner) {
                    if let Some(p) = e.group_members.iter().rposition(|&x| x == m) {
                        e.group_members.remove(p);
                    }
                }
            }
        }
        self.party_cancel_group_em(owner, cfg, em);
    }

    /// `PlayObject.cs:5126-5156` 的 `DelMember`（公开包装）。
    pub fn party_del_member(
        &mut self,
        owner: i32,
        target: i32,
        cfg: &PartyConfig,
    ) -> Vec<PartyEvent> {
        let mut em = Emitter::new();
        self.party_del_member_em(owner, target, cfg, &mut em);
        em.finish()
    }

    /// `PlayObject.Operate.cs:787-808` 的 `ClientGroupClose()`（公开包装）。
    pub fn party_group_close(&mut self, actor: i32, cfg: &PartyConfig) -> Vec<PartyEvent> {
        let mut em = Emitter::new();
        self.party_group_close_em(actor, cfg, &mut em);
        em.finish()
    }

    fn party_group_close_em(&mut self, actor: i32, cfg: &PartyConfig, em: &mut Emitter) {
        let owner = self.party_group_owner_of(actor);
        if owner == 0 {
            if let Some(e) = self.entities.get_mut(actor) {
                e.allow_group = false;
            }
            return;
        }
        if owner != actor {
            // 非团长：团长 `DelMember(this)`，随后本侧 `AllowGroup = false`。
            self.party_del_member_em(owner, actor, cfg, em);
            if let Some(e) = self.entities.get_mut(actor) {
                e.allow_group = false;
            }
        } else {
            // 团长自己：`SysMsg("如果你想退出，使用编组功能（删除按钮）", Red, Hint)` ⇒
            // `SendMsg(RM_SYSMESSAGE, 0, RedF, RedB, 0, sMsg)`（`BaseObject.cs:1682`，排队优先级 `252`），
            // `Recog` = 自身 `ActorId`（`SendMsg` 的 `ActorId=this.ActorId`）。
            em.later(
                PRIO_RM_SYSMESSAGE,
                PartyEvent::SysMessage {
                    target: actor,
                    recog: actor,
                    f_color: cfg.red_msg_f_color,
                    b_color: cfg.red_msg_b_color,
                    text: PARTY_LEADER_CLOSE_MSG.to_string(),
                },
            );
        }
        // oracle `Operate.cs:806`：`GroupOwner != 0`（上面已早退滤除 0）⇒ 两条分支后都触发
        // `FunctionNPC.GotoLable(this, "@GroupClose", false)`（脚本说词经 `RM_MERCHANTSAY` 排队，
        // 优先级字节 1 ⇒ 排在本方法的其它排队帧之前）。
        em.later(
            PRIO_RM_MERCHANTSAY,
            PartyEvent::GotoLabel {
                target: actor,
                label: PARTY_LABEL_GROUP_CLOSE,
            },
        );
    }

    /// `PlayObject.Message.cs:914-929` 的 `CM_GROUPMODE` 支。
    ///
    /// `param2 == 0` ⇒ `ClientGroupClose()`；否则 `AllowGroup = true`。随后按 `AllowGroup` 发
    /// `SM_GROUPMODECHANGED`（`param` = 1/0）。
    pub fn party_group_mode(
        &mut self,
        actor: i32,
        param2: u16,
        cfg: &PartyConfig,
    ) -> Vec<PartyEvent> {
        let mut em = Emitter::new();
        if param2 == 0 {
            self.party_group_close_em(actor, cfg, &mut em);
        } else if let Some(e) = self.entities.get_mut(actor) {
            e.allow_group = true;
        }
        let allow = self
            .entities
            .get(actor)
            .map(|e| e.allow_group)
            .unwrap_or(false);
        em.now(PartyEvent::GroupModeChanged {
            target: actor,
            param: u16::from(allow),
        });
        em.finish()
    }

    /// `PlayObject.Operate.cs:810-845` 的 `ClientCreateGroup(sHumName)`。
    pub fn party_create_group(
        &mut self,
        actor: i32,
        target_name: &str,
        cfg: &PartyConfig,
    ) -> Vec<PartyEvent> {
        let mut em = Emitter::new();
        if self.party_group_owner_of(actor) != 0 {
            em.now(PartyEvent::CreateGroupFail {
                target: actor,
                recog: -1,
            });
            return em.finish();
        }
        let Some(target) = self.party_play_object_by_name(target_name) else {
            em.now(PartyEvent::CreateGroupFail {
                target: actor,
                recog: -2,
            });
            return em.finish();
        };
        if target == actor || self.party_is_dead_or_ghost(target) {
            em.now(PartyEvent::CreateGroupFail {
                target: actor,
                recog: -2,
            });
            return em.finish();
        }
        if self.party_group_owner_of(target) != 0 {
            em.now(PartyEvent::CreateGroupFail {
                target: actor,
                recog: -3,
            });
            return em.finish();
        }
        if !self
            .entities
            .get(target)
            .map(|e| e.allow_group)
            .unwrap_or(false)
        {
            em.now(PartyEvent::CreateGroupFail {
                target: actor,
                recog: -4,
            });
            return em.finish();
        }
        if let Some(e) = self.entities.get_mut(actor) {
            e.group_members.clear();
            e.group_members.push(actor);
            e.group_members.push(target);
        }
        self.party_join_group_em(actor, actor, cfg, &mut em);
        self.party_join_group_em(target, actor, cfg, &mut em);
        if let Some(e) = self.entities.get_mut(actor) {
            e.allow_group = true;
        }
        em.now(PartyEvent::CreateGroupOk { target: actor });
        for ev in self.party_send_group_members(actor) {
            em.now(ev);
        }
        // oracle `Operate.cs:843`：成功路径尾 `FunctionNPC.GotoLable(this, "@GroupCreate", false)`。
        em.later(
            PRIO_RM_MERCHANTSAY,
            PartyEvent::GotoLabel {
                target: actor,
                label: PARTY_LABEL_GROUP_CREATE,
            },
        );
        em.finish()
    }

    /// `PlayObject.Operate.cs:847-883` 的 `ClientAddGroupMember(sHumName)`。
    ///
    /// `GroupMembers.Count > GroupMembersMax`（**严格大于**，`Operate.cs:855`）⇒ `-5`。
    pub fn party_add_group_member(
        &mut self,
        actor: i32,
        target_name: &str,
        cfg: &PartyConfig,
    ) -> Vec<PartyEvent> {
        let mut em = Emitter::new();
        if self.party_group_owner_of(actor) != actor {
            em.now(PartyEvent::AddMemberFail {
                target: actor,
                recog: -1,
            });
            return em.finish();
        }
        if self.party_members_of(actor).len() > usize::from(cfg.group_members_max) {
            em.now(PartyEvent::AddMemberFail {
                target: actor,
                recog: -5,
            });
            return em.finish();
        }
        let Some(target) = self.party_play_object_by_name(target_name) else {
            em.now(PartyEvent::AddMemberFail {
                target: actor,
                recog: -2,
            });
            return em.finish();
        };
        if target == actor || self.party_is_dead_or_ghost(target) {
            em.now(PartyEvent::AddMemberFail {
                target: actor,
                recog: -2,
            });
            return em.finish();
        }
        if self.party_group_owner_of(target) != 0 {
            em.now(PartyEvent::AddMemberFail {
                target: actor,
                recog: -3,
            });
            return em.finish();
        }
        if !self
            .entities
            .get(target)
            .map(|e| e.allow_group)
            .unwrap_or(false)
        {
            em.now(PartyEvent::AddMemberFail {
                target: actor,
                recog: -4,
            });
            return em.finish();
        }
        if let Some(e) = self.entities.get_mut(actor) {
            e.group_members.push(target);
        }
        self.party_join_group_em(target, actor, cfg, &mut em);
        em.now(PartyEvent::AddMemberOk { target: actor });
        for ev in self.party_send_group_members(actor) {
            em.now(ev);
        }
        // oracle `Operate.cs:881`：成功路径尾 `FunctionNPC.GotoLable(this, "@GroupAddMember", false)`。
        em.later(
            PRIO_RM_MERCHANTSAY,
            PartyEvent::GotoLabel {
                target: actor,
                label: PARTY_LABEL_GROUP_ADD_MEMBER,
            },
        );
        em.finish()
    }

    /// `PlayObject.cs:5158-5175` 的 `IsGroupMember(target)`。
    #[must_use]
    pub fn party_is_group_member(&self, actor: i32, target: i32) -> bool {
        let owner = self.party_group_owner_of(actor);
        if owner == 0 {
            return false;
        }
        self.party_members_of(owner).contains(&target)
    }

    /// §1.O 扩展面（`recall-ext`）脚本读侧的宿主快照：`(GroupOwner, GroupMembers 各成员的 Job)`。
    ///
    /// oracle 侧 `ConditionProcessingSys` 读的是 `playerActor.GroupOwner` /
    /// `playerActor.GroupMembers[i].Job`（`ConditionProcessingSys.cs:2670/262/4466`）；本仓
    /// 把这两个字段从实体表取出来交给 `bins/game-svr` 的 `ScriptPlayer` 适配。
    ///
    /// `GroupMembers` **只存在于团长实体上**（`PlayObject.Operate.cs:834-835/875`，与
    /// [`Self::party_members_of`] 同口径）⇒ 非团长的成员列表为空；成员 Job 按**加入序**。
    #[must_use]
    pub fn party_script_group_snapshot(&self, actor: i32) -> (i32, Vec<i32>) {
        let owner = self.party_group_owner_of(actor);
        let jobs = self
            .party_members_of(actor)
            .into_iter()
            .filter_map(|m| self.entities.get(m).map(|e| i32::from(e.job)))
            .collect();
        (owner, jobs)
    }

    /// `PlayObject.Operate.cs:885-909` 的 `ClientDelGroupMember(sHumName)`。
    pub fn party_del_group_member(
        &mut self,
        actor: i32,
        target_name: &str,
        cfg: &PartyConfig,
    ) -> Vec<PartyEvent> {
        let mut em = Emitter::new();
        if self.party_group_owner_of(actor) != actor {
            em.now(PartyEvent::DelMemberFail {
                target: actor,
                recog: -1,
            });
            return em.finish();
        }
        let Some(target) = self.party_play_object_by_name(target_name) else {
            em.now(PartyEvent::DelMemberFail {
                target: actor,
                recog: -2,
            });
            return em.finish();
        };
        if !self.party_is_group_member(actor, target) {
            em.now(PartyEvent::DelMemberFail {
                target: actor,
                recog: -3,
            });
            return em.finish();
        }
        self.party_del_member_em(actor, target, cfg, &mut em);
        em.now(PartyEvent::DelMemberOk {
            target: actor,
            name: target_name.to_string(),
        });
        // oracle `Operate.cs:907`：成功路径尾 `FunctionNPC.GotoLable(this, "@GroupDelMember", false)`。
        em.later(
            PRIO_RM_MERCHANTSAY,
            PartyEvent::GotoLabel {
                target: actor,
                label: PARTY_LABEL_GROUP_DEL_MEMBER,
            },
        );
        em.finish()
    }

    /// `PlayObject.Message.cs:704-719` 的每 tick 清理（oracle `Run()` 的组队段）。
    ///
    /// ① `GroupOwner != 0` 且团长缺失/`Death`/`Ghost` ⇒ 本侧 `GroupOwner = 0`；
    /// ② `GroupOwner == ActorId`（团长本人）⇒ 从尾到头移除 `Death`/`Ghost` 的成员。
    ///
    /// 「团长缺失」是 oracle `ActorMgr.Get` 返回 `null` 的防御等价（oracle 会 NRE，但正常流程下
    /// 团长会先被 ① 处理；本仓没有「半销毁实体」，缺失只在离世界后出现）。
    pub fn party_run_tick(&mut self) {
        let ids = self.entities.ids_sorted();
        for id in ids {
            let owner = match self.entities.get(id) {
                Some(e) => e.group_owner,
                None => continue,
            };
            if owner != 0 && self.party_is_dead_or_ghost(owner) {
                if let Some(e) = self.entities.get_mut(id) {
                    e.group_owner = 0;
                }
            }
            let is_leader = match self.entities.get(id) {
                Some(e) => e.group_owner == id,
                None => false,
            };
            if is_leader {
                let members = self.party_members_of(id);
                for m in members.into_iter().rev() {
                    if self.party_is_dead_or_ghost(m) {
                        if let Some(e) = self.entities.get_mut(id) {
                            if let Some(p) = e.group_members.iter().rposition(|&x| x == m) {
                                e.group_members.remove(p);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 空实现占位：提醒 [`Entity`] 的组队字段是 `crates/world` 的权威状态（host 不另存一份）。
#[allow(dead_code)]
fn _party_entity_fields_are_authoritative(_e: &Entity) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys_msg(target: i32, text: &str) -> PartyEvent {
        PartyEvent::SysMessage {
            target,
            recog: target,
            f_color: 0,
            b_color: 0,
            text: text.to_string(),
        }
    }

    fn texts(evs: &[PartyEvent]) -> Vec<String> {
        evs.iter()
            .map(|e| match e {
                PartyEvent::SysMessage { text, .. } => text.clone(),
                other => panic!("只应产 SysMessage，实得 {other:?}"),
            })
            .collect()
    }

    /// **卡 `openmir2-frame-drain-order-audit` 判据**：排队桶出队 = oracle `MsgQueue`（每人一条）
    /// 的 .NET 8 四叉堆序 ⇒ 同优先级 3 条 `[0,2,1]`、4 条 `[0,3,2,1]`。
    ///
    /// 可达性：团长死亡散组（`party_del_member_em` 的 `owner == target` 支）从尾到头逐成员
    /// `LeaveGroup()`，每次 `SendGroupText` 给**仍在组内**的每个成员各一条 `RM_GROUPMESSAGE`
    /// （253）⇒ 同一成员一次收多条。
    ///
    /// 牙（改坏必红）：`Emitter::finish` 改回 `self.queued.sort_by_key(|(p, _)| *p)` ⇒
    /// 得 `[s0,s1,s2]`/`[s0,s1,s2,s3]`，断言红。
    #[test]
    fn emitter_finish_same_priority_follows_four_ary_heap() {
        for (n, want) in [
            (3usize, vec!["s0", "s2", "s1"]),
            (4, vec!["s0", "s3", "s2", "s1"]),
        ] {
            let mut em = Emitter::new();
            for i in 0..n {
                em.later(PRIO_RM_GROUPMESSAGE, sys_msg(901, &format!("s{i}")));
            }
            assert_eq!(texts(&em.finish()), want, "n={n}");
        }
    }

    /// **跨收件人不得混堆**：oracle 每人一条 `MsgQueue`。`[A0,B1,A2,B3]` 交替入队 ⇒ 混堆会让 B
    /// 出 `[B3,B1]`，分组（本实现）出 `[B1,B3]`。
    #[test]
    fn emitter_finish_keeps_recipients_in_separate_heaps() {
        let mut em = Emitter::new();
        for (target, text) in [(901, "a0"), (902, "b1"), (901, "a2"), (902, "b3")] {
            em.later(PRIO_RM_GROUPMESSAGE, sys_msg(target, text));
        }
        let evs = em.finish();
        let pick = |want: i32| -> Vec<String> {
            texts(
                &evs.iter()
                    .filter(|e| e.target() == want)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(pick(901), vec!["a0", "a2"]);
        assert_eq!(pick(902), vec!["b1", "b3"], "混堆会得 [b3,b1]");
    }

    /// 分组不改变**立即桶**次序（立即帧按源码调用序，见模块头）。
    #[test]
    fn emitter_finish_keeps_immediate_order() {
        let mut em = Emitter::new();
        em.now(PartyEvent::CreateGroupOk { target: 901 });
        em.now(PartyEvent::AddMemberOk { target: 902 });
        em.later(
            PRIO_RM_MERCHANTSAY,
            PartyEvent::GotoLabel {
                target: 901,
                label: "@GroupCreate",
            },
        );
        let evs = em.finish();
        assert!(matches!(evs[0], PartyEvent::CreateGroupOk { target: 901 }));
        assert!(matches!(evs[1], PartyEvent::AddMemberOk { target: 902 }));
        assert!(matches!(evs[2], PartyEvent::GotoLabel { target: 901, .. }));
    }

    /// **M-2（矩阵 §6#6）`party_play_object_by_name` 跳过 ghost**：ghost 的世界对象（M-3 后仍在）
    /// 不该被组队/邀请的按名查找命中。
    ///
    /// 牙：删 `.find` 谓词里的 `&& !e.ghost` ⇒ ghost 被命中（`Some(gid)`）⇒ 红。
    /// 规则 19 退化输入：同名语料里活玩家照常命中（证明门只挡 ghost）。
    #[test]
    fn m2_party_play_object_by_name_skips_ghost() {
        let mut w = World::new(16, 16);
        let mut g = Entity::new(1, "ghost", 1, 1);
        g.race = ACTOR_RACE_PLAY;
        g.ghost = true;
        let gid = w.add_entity(g);
        let mut l = Entity::new(2, "ally", 2, 2);
        l.race = ACTOR_RACE_PLAY;
        let lid = w.add_entity(l);
        assert_eq!(w.party_play_object_by_name("ghost"), None, "不得命中 ghost");
        assert_eq!(
            w.party_play_object_by_name("ally"),
            Some(lid),
            "活玩家照常命中（门只挡 ghost）"
        );
        // 负例的负例：清掉 ghost 位 ⇒ 立刻能命中（证明上面 None 确由 ghost 门给出）。
        w.entities.get_mut(gid).unwrap().ghost = false;
        assert_eq!(w.party_play_object_by_name("ghost"), Some(gid));
    }
}
