//! `CM_PICKUP`(1001) / `CM_DROPITEM`(1000) 的**接线层**（owner §23.1/§25.1/§26）：
//! 把 `ground::*` 纯函数产出的 `Frame` / `QueuedNotice` 翻成经网关下发的 `SM_*` body
//! （`GM_DATA.body = 12B 命令头 ++ 尾`），并把 world 的只读地面视图装配成 `ground::GroundMap`。
//!
//! 口径逐条照 oracle（作者 comment `d552d2a2`；行号）：
//!
//! ```text
//! CM_DROPITEM（PlayObject.Message.cs:801-810）
//!   if (ClientDropItem(Msg, nParam1))  SendDefMessage(SM_DROPITEM_SUCCESS, nParam1,0,0,0, Msg)
//!   else                               SendDefMessage(SM_DROPITEM_FAIL,    nParam1,0,0,0, Msg)
//!   ⇒ Recog = make_index{nParam1}；尾 = EncodeString(请求的 Msg)（`ground::DropOutcome` 只给 12B 头，
//!     尾由本层补）。
//! CM_PICKUP（:811-816）
//!   先判 CurrX==nParam2 && CurrY==nParam3 才 ClientPickUpItem()（**位置门在派发层**，见 lib.rs）。
//! 延时通知 → SM（全部收件人=自己）
//!   RM_ITEMHIDE(8085)      → SM_ITEMHIDE(611)      recog=nParam1 param=nParam2 tag=nParam3 series=0
//!   RM_WEIGHTCHANGED(10115)→ SM_WEIGHTCHANGED(622) 用**当下 WAbil**（Weight/WearWeight/HandWeight + 校验和）
//!   RM_GOLDCHANGED(10136)  → SM_GOLDCHANGED(653)   Gold + LoWord/HiWord(GameGold)
//!   RM_SYSMESSAGE(11004)   → SM_SYSMESSAGE(100)    recog=ActorId, param=MakeWord(nParam1,nParam2), series=1, 尾=EncodeString(msg)
//!   RM_MENU_OK(10309)      → SM_MENU_OK(767)        recog=nParam1, 尾=processMsg.Msg（见 `script_frame`）
//! ```
//!
//! # 已知缺口（登记，不静默）
//!
//! 1. `RM_SYSMESSAGE` 的文本：取件一条（oracle `SysMsg(MessageSettings.CanotPickUpItem, Red, Hint)`）
//!    在 `ground::pick_up` 的纯函数里拿不到配置 ⇒ 通知标记为 `NoticeText::SessionPickup`，
//!    由本层用 [`WireSession::sys_msg`] 注入；缺该输入时**跳过**（不编造空文本）。
//!    吃饭/修复油/彩票的通知自带 `NoticeText::Inline`（入队时由 `useitem` / `handle_eat_cmd`
//!    放好文本，见卡 `openmir2-runtime-sysmsg-text-frames`），**不走**会话回退。
//! 2. 装配地面物品时 `BagItem.dura/dura_max/desc` 不在 world 只读视图里
//!    （`FloorItemMeta` 只记 `kind/index/count/item_id/of_base_object`）⇒ 取 `BagItem` 默认值。
//!    要逐字节复原需扩 world 记账（下一片，需 owner 放行写分片）。
//! 3. `CM_DROPITEM` 的成功写回（`Envir.AddItemToMap`）在本接缝下恒失败（`MapItem.ItemId == 0`，
//!    见 `ground.rs` 模块头 `T-19`）⇒ 不加地面；一旦 oracle 补上 `ItemId`，写回属下一片。

use crate::ability::{Ability, ItemCatalog};
use crate::bag::{BagItem, Frame};
use crate::ground::{
    GroundItem, GroundMap, RM_GOLDCHANGED, RM_ITEMHIDE, RM_MENU_OK, RM_SYSMESSAGE,
    RM_WEIGHTCHANGED, SM_ADDITEM, SM_DROPITEM_FAIL, SM_DROPITEM_SUCCESS,
};
use crate::npc::SendMsg;
use crate::smframe;
use crate::useitem::{
    my_status, NoticeText, QueuedNotice, RM_ABILITY, RM_CHARSTATUSCHANGED, RM_HEALTHSPELLCHANGED,
    RM_MYSTATUS, RM_SUBABILITY,
};
use mir2_world_bevy::{FloorItemKind, FloorItemView};

/// 接线层在一次 pickup/drop 处理里需要读到的**会话态**（oracle `PlayObject` 上的当下值）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireSession {
    /// `BaseObject.ActorId`（`SM_SYSMESSAGE` 的 `Recog`）。
    pub actor_id: i32,
    /// `BaseObject.Gold`（`SM_GOLDCHANGED` 用**当下值**）。
    pub gold: i32,
    /// `BaseObject.GameGold`（`SM_GOLDCHANGED` 的 LoWord/HiWord）。
    pub game_gold: i32,
    /// `BaseObject.WAbil`（`SM_WEIGHTCHANGED` 用**当下值**）。
    pub wabil: Ability,
    /// `BaseObject.Abil.Exp`（**基础**，`SM_WINEXP` 的 `Recog` 取它——oracle `GetExp` 只改 `Abil`，
    /// `WAbil.Exp` 在非升级后**陈旧**，不可用它当 `Recog`）。
    pub abil_exp: i32,
    /// `MessageSettings.CanotPickUpItem` 的 GBK 原文。**只服务 `NoticeText::SessionPickup`**
    /// （取件一条）；其余通知自带文本，缺文本时不产帧——不做全域回退。
    pub sys_msg: Option<Vec<u8>>,
    /// `PlayObject.Job`（`SM_ABILITY` 的 `MakeWord((byte)Job, 99)`，`PlayObject.Message.cs:1733`）。
    pub job: u8,
    /// `Ability` 的客户端尾（`SM_ABILITY` 的 EDCode 尾；同 `crate::send_levelup_frames` 的
    /// `ability_tail(&sg.state.wabil, client_layout)`）。
    pub ability_tail: Vec<u8>,
    /// `SubAbility::to_frame_bytes()`（`SM_SUBABILITY` 四参数来源，`PlayObject.Message.cs:1935`）。
    pub sub_bytes: [u16; 7],
    /// `PlayObject.HungerStatus`（`PlayObject.cs:225`）——`RM_MYSTATUS` 的 `GetMyStatus()`
    /// （`PlayObject.Base.cs:1183`）读它（**制药/食药片 WB**：`CM_EAT` StdMode 1 的跨档出帧）。
    pub hunger_status: i32,
    /// `PlayObject.HitSpeed`（`PlayObject.cs:2899`，由 `RecalcAbilitys` 写）。
    /// `SM_CHARSTATUSCHANGED(657)` 的 `Series` 取**收件人自己**的该值
    /// （`PlayObject.Message.cs:1846` 的 `this.HitSpeed`）；本结构是"该会话自己"的，
    /// 故 `RM_CHARSTATUSCHANGED` 的自通知用它。
    pub hit_speed: u16,
}

/// 把一条立即帧（[`Frame`]）翻成 `SM_*` body；未知 ident ⇒ `None`（调用方记 WRN）。
///
/// `SM_ADDITEM` 需要**实际入包的物品**（G-1：`SendAddItem` 的 `ClientItem` 尾）；
/// `add_item`/`catalog` 为 `None` 时该帧不产出（不静默退化成 header-only）。
#[must_use]
pub fn immediate_frame(
    frame: &Frame,
    drop_msg: &[u8],
    add_item: Option<&BagItem>,
    catalog: &dyn ItemCatalog,
) -> Option<Vec<u8>> {
    match frame.ident {
        SM_ADDITEM => crate::trade::send_add_item_body(frame.recog, add_item?, catalog),
        SM_DROPITEM_SUCCESS => Some(smframe::sm_dropitem_reply(true, frame.recog, drop_msg)),
        SM_DROPITEM_FAIL => Some(smframe::sm_dropitem_reply(false, frame.recog, drop_msg)),
        _ => None,
    }
}

/// 把一条延时通知翻成 `SM_*` body；未知 ident ⇒ `None`。
///
/// `RM_SYSMESSAGE` 缺 [`WireSession::sys_msg`] 时也返回 `None`（缺口 1）。
#[must_use]
pub fn notice_frame(n: &QueuedNotice, session: &WireSession) -> Option<Vec<u8>> {
    match n.ident {
        RM_ITEMHIDE => Some(smframe::sm_itemhide(n.n_param1, n.n_param2, n.n_param3)),
        RM_WEIGHTCHANGED => Some(smframe::sm_weightchanged(
            session.wabil.weight,
            session.wabil.wear_weight,
            session.wabil.hand_weight,
        )),
        // `CM_EAT` 的 `StdMode 0` 回帧（`BaseObject.IncHealthSpell` 尾部 `HealthSpellChanged()`，
        // `BaseObject.cs:704` ⇒ `SendMsg(RM_HEALTHSPELLCHANGED)`，`PlayObject.Message.cs:1736-1739`）。
        // 布局：生产默认冻结客户端（`TCharDesc` 尾），`MIR2_CLIENT_ABILITY_LAYOUT=0` ⇒ oracle header-only。
        RM_HEALTHSPELLCHANGED => Some(smframe::sm_healthspellchanged_frame(
            session.actor_id,
            session.wabil.hp,
            session.wabil.mp,
            session.wabil.max_hp,
        )),
        RM_GOLDCHANGED => Some(smframe::sm_goldchanged(session.gold, session.game_gold)),
        RM_SYSMESSAGE => {
            // 文本来源按**通知种类**收窄（卡 `openmir2-runtime-sysmsg-text-frames` §4）：
            // ① `Inline`：调用点已知 `sMsg`（禁药/修复油/彩票）——`String.conf` 的 cp936
            //    在此**唯一一次**编回字节，随后 `sm_sysmessage` 做 EDCode；
            // ② `SessionPickup`：**只有取件**这条允许回退到会话静态文本
            //    `WireSession::sys_msg`（`MessageSettings.CanotPickUpItem` 在
            //    `ground::pick_up` 的纯函数里拿不到配置）；
            // ③ `None`：不产帧。**不做全域回退**——否则会话里留着的取件文案会串到
            //    吃饭/彩票帧上（负例见 `sysmessage_inline_text_never_uses_session_text`）。
            use std::borrow::Cow;
            let text: Option<Cow<'_, [u8]>> = match &n.text {
                NoticeText::Inline(t) => Some(Cow::Owned(crate::enter::gbk(t))),
                NoticeText::SessionPickup => session.sys_msg.as_deref().map(Cow::Borrowed),
                NoticeText::None => None,
            };
            text.map(|msg| {
                smframe::sm_sysmessage(
                    session.actor_id,
                    // `MakeWord((ushort)nParam1, (ushort)nParam2)`：低 16 位（G-9）。
                    u16::try_from(n.n_param1 & 0xFFFF).unwrap_or(0),
                    u16::try_from(n.n_param2 & 0xFFFF).unwrap_or(0),
                    &msg,
                )
            })
        }
        RM_MENU_OK => {
            // 卡 `openmir2-menu-ok-text`：oracle `PlayObject.Message.cs:1952-1954` 的
            // `SendDefMessage(SM_MENU_OK, nParam1, 0,0,0, Msg)` ⇒ 尾 = `EDCode.EncodeString(Msg)`。
            // 四个生产者的文案不同（`CanotUseItemMsg` / `CanotDropInSafeZoneMsg` /
            // `CanotDropGoldMsg` / `CanotDropItemMsg`），由入队方或发送点以 `Inline` 给出。
            let tail = match &n.text {
                NoticeText::Inline(t) => crate::enter::gbk(t),
                NoticeText::None | NoticeText::SessionPickup => Vec::new(),
            };
            Some(smframe::sm_menu_ok(n.n_param1, &tail))
        }
        // `RM_ABILITY`(10051) ⇒ `SM_ABILITY(52)`（`PlayObject.Message.cs:1732-1733`）：
        // `MakeMessage(SM_ABILITY, Gold, MakeWord((byte)Job, 99), LoWord(GameGold), HiWord(GameGold))`。
        // **低字节 67**：泵序上晚于 `RM_SUBABILITY`(10302→62)。
        RM_ABILITY => Some(smframe::sm_ability(
            session.gold,
            session.job,
            session.game_gold,
            &session.ability_tail,
        )),
        // `RM_SUBABILITY`(10302) ⇒ `SM_SUBABILITY(752)`（`PlayObject.Message.cs:1934-1935`）：
        // `SendDefMessage(SM_SUBABILITY, MakeLong(MakeWord(AntiMagic,0),0),
        //  MakeWord(HitPoint,SpeedPoint), MakeWord(AntiPoison,PoisonRecover),
        //  MakeWord(HealthRecover,SpellRecover))`。**低字节 62**：线上先于 SM_ABILITY。
        RM_SUBABILITY => {
            let b = session.sub_bytes;
            Some(smframe::sm_subability(
                b[0],
                u8::try_from(b[1]).unwrap_or(0),
                u8::try_from(b[2]).unwrap_or(0),
                u8::try_from(b[3]).unwrap_or(0),
                u8::try_from(b[4]).unwrap_or(0),
                u8::try_from(b[5]).unwrap_or(0),
                u8::try_from(b[6]).unwrap_or(0),
            ))
        }
        // `RM_WINEXP`（8075）⇒ `SM_WINEXP(44)`：`PlayObject.Message.cs:1666-1669` 的
        // `MakeMessage(SM_WINEXP, Abil.Exp, LoWord(nParam1), HiWord(nParam1), 0)`。`Recog` 取**基础** `Abil.Exp`
        //（`session.abil_exp`），**不是** `WAbil.Exp`（非升级 `GetExp` 后 WAbil 陈旧）。IT-08 经此出帧。
        RM_SCRIPT_WINEXP => Some(smframe::sm_winexp(session.abil_exp, n.n_param1)),
        // `RM_MYSTATUS`（`PlayObject.Base.cs:291` 的 `RefMyStatus()`）⇒
        // `SendDefMessage(SM_MYSTATUS, 0, (short)GetMyStatus(), 0, 0)`（`PlayObject.Message.cs:1950`）。
        // `GetMyStatus()` = `min(HungerStatus/1000, 4)` 的**当下**值（`PlayObject.Base.cs:1183`）。
        RM_MYSTATUS => Some(crate::smframe_ext::sm_mystatus(my_status(
            session.hunger_status,
        ))),
        // `RM_CHARSTATUSCHANGED`（10139）⇒ `SM_CHARSTATUSCHANGED(657)`。
        //
        // 产生点两处（同 ident）：`RecalcAbilitys()` 尾部的 `SendRefMsg(RM_CHARSTATUSCHANGED, 0,
        // CharStatus, 0, 0, "")`（`BaseObject.cs:1516`）与去重变体 `SendUpdateMsg(...)`
        // （`PlayObject.cs:2876`）。出队口径 `PlayObject.Message.cs:1845-1847`：
        // `SendDefMessage(SM_CHARSTATUSCHANGED, processMsg.ActorId, LoWord(nParam1), HiWord(nParam1),
        // this.HitSpeed)`。
        //
        // 本接缝只承担**发送者自己的那一份**（`this` = 接收者 = 发送者 ⇒ `Series` 取本会话的
        // `hit_speed`）；观察者那份由广播路径（`push_charstatuschanged` /
        // `broadcast_charstatuschanged_except_self`）负责，二者**互斥不重叠**——广播函数
        // 显式 `skip` 自己，见 `charstatuschanged_self_and_broadcast_do_not_duplicate`。
        RM_CHARSTATUSCHANGED => Some(crate::smframe_ext::sm_charstatuschanged(
            session.actor_id,
            n.n_param1,
            session.hit_speed,
        )),
        _ => None,
    }
}

/// `RM_GAMEGOLDCHANGED(8084)`：`GAMEGOLD`/`GAMEPOINT` 动作改元宝后
/// `GameGoldChanged()`（`BaseObject.cs:593-599`）排队的那条消息。
pub const RM_SCRIPT_GAMEGOLDCHANGED: i32 = mir2_protocol::messages::RM_GAMEGOLDCHANGED as i32;
/// `Messages.RM_PLAYDICE`（`Messages.cs` 10500；泵 `PlayObject.Message.cs:2034-2041`）。
pub const RM_SCRIPT_PLAYDICE: i32 = mir2_protocol::messages::RM_PLAYDICE as i32;
/// `Messages.RM_WINEXP`（`Messages.cs` 8075；泵 `PlayObject.Message.cs:1666-1669`）。
pub const RM_SCRIPT_WINEXP: i32 = mir2_protocol::messages::RM_WINEXP as i32;
/// `Messages.RM_DURACHANGE`（`Messages.cs` 10125；泵 `PlayObject.Message.cs:1803-1805`）。
pub const RM_SCRIPT_DURACHANGE: i32 = mir2_protocol::messages::RM_DURACHANGE as i32;
/// `Messages.RM_MERCHANTDLGCLOSE`（`Messages.cs` 10127；泵 `PlayObject.Message.cs:1806-1808`）。
pub const RM_SCRIPT_MERCHANTDLGCLOSE: i32 = mir2_protocol::messages::RM_MERCHANTDLGCLOSE as i32;

/// **脚本帧**（[`SendMsg`]，`NpcPlayer.frames`）⇒ wire body。
///
/// 映射脚本 `RM_*`（与 oracle 消息泵逐字段对齐）：
///
/// - `RM_GOLDCHANGED`（10136）⇒ `SM_GOLDCHANGED(653)`（`PlayObject.Message.cs:1833-1835`）；
/// - `RM_GAMEGOLDCHANGED`（8084）⇒ `SendGoldInfo(false)`（`PlayObject.Message.cs:1836-1837`）
///   ⇒ `SendDefMessage(SM_GAMEGOLDNAME, GameGold, LoWord(GamePoint), HiWord(GamePoint), 0, "")`
///   （`PlayObject.Base.cs:927-939`）；`SoftVersionDateEx == 0` 时整帧不发（同 `:930`）。
/// - `RM_PLAYDICE`（10500）⇒ `PlayObject.Message.cs:2034-2041`：
///   `MakeMessage(SM_PLAYDICE, processMsg.ActorId, processMsg.wParam, 0, 0)`
///   ++ `EncodePacket(MessageBodyWL{Param1,Param2,Tag1,0})` ++ `EncodeString(Msg)`；
///   `ActorId` = 入队方 NPC 的 `ActorId`（`BaseObject.Message.cs:26/90` 的 `this.ActorId`），
///   调用方须传入本次 `GotoLable` 的 NPC 实体 id。
/// - `RM_SYSMESSAGE`（11004）⇒ `PlayObject.Message.cs:1715-1718`：
///   `MakeMessage(SM_SYSMESSAGE, ActorId, MakeWord(FColor,BColor), 0, 1)` + `EncodeString(Msg)`；
///   `ActorId` = **收帧玩家自己**（`BaseObject.SysMsg` 的 `this.ActorId`），`F/B` 由
///   `SysMsg` 按 `MsgColor` 从 `SystemShare.Config` 选好（`NpcPlayer::sys_msg`）。
/// - `RM_MENU_OK`（10309）⇒ `SM_MENU_OK(767)`：`PlayObject.Message.cs:1952-1953`
///   （`SendDefMessage(SM_MENU_OK, processMsg.nParam1, 0, 0, 0, Msg)`）；`Recog` = `nParam1`
///   （脚本 `MESSAGEBOX` 的 `playerActor.ActorId`，`ExecutionProcessingSys.cs:3418`），
///   尾 = `EDCode.EncodeString(cp936(Msg))`（T-194）。
/// - `RM_WINEXP`（8075）⇒ `SM_WINEXP(44)`：`PlayObject.Message.cs:1666-1669` 的 `MakeMessage(SM_WINEXP, Abil.Exp, LoWord(processMsg.nParam1), HiWord(processMsg.nParam1), 0)` + `SendSocket(ClientMsg)`（**无尾**）。`Recog` = 加完之后的 `Abil.Exp` **总量**（排水时的会话当下值，由 `exp` 形参给出，**不是**本次增量）、`Param/Tag` = 增量 `nParam1` 的 Lo/Hi、`Series` = 0。脚本发点 = `ExecutionProcessingSys.cs:2861`（`ActionOfChangeExp` 的 `'+'` 支：`SendMsg(playerActor, RM_WINEXP, 0, dwInt, 0, 0)` ⇒ `wParam` = 0、`nParam1` = `dwInt`）。
/// - `RM_DURACHANGE`（10125）⇒ `SM_DURACHANGE(642)`：`PlayObject.Message.cs:1803-1805` 的 `SendDefMessage(SM_DURACHANGE, processMsg.nParam1, processMsg.wParam, LoWord(processMsg.nParam2), HiWord(processMsg.nParam2))` ⇒ `Recog` = `nParam1` = **`Dura`**、`Param` = `wParam` = 槽位、`Tag/Series` = `nParam2` = **`DuraMax`** 的 Lo/Hi（`ushort` ⇒ Hi = 0）。脚本发点 = `ExecutionProcessingSys.cs:3945`（`ActionOfRepairAllItem`：`SendMsg(normNpc, RM_DURACHANGE, (short)i, Dura, DuraMax, 0)`）。
/// - `RM_MERCHANTDLGCLOSE`（10127）⇒ `SM_MERCHANTDLGCLOSE(644)`：`PlayObject.Message.cs:1806-1808` 的 `SendDefMessage(SM_MERCHANTDLGCLOSE, processMsg.nParam1, 0, 0, 0)`（**header-only**）。脚本发点 = `ExecutionProcessingSys.cs:2183`（`ActionOfClose`：`SendMsg(normNpc, RM_MERCHANTDLGCLOSE, 0, normNpc.ActorId, 0, 0)`）⇒ `Recog` = `nParam1` = **NPC** 的 `ActorId`（**不是**玩家、也不是排水时的 `npc_actor_id`：oracle 读的是消息自带的 `processMsg.nParam1`，此处取 `s.n_param1` 才与泵同源）。
///
/// **`RM_ADJUST_BONUS`（8078）本片仍不映射（登记，非新增缺口）**：泵
/// `PlayObject.Message.cs:2002-2003` 调 `SendAdjustBonus()`
/// （`PlayObject.Base.cs:1193-1210`：`MakeMessage(SM_ADJUST_BONUS, BonusPoint, 0, 0, 0)` +
/// `EncodeMessage(Config.BonusAbilofXxx)/EncodeMessage(BonusAbil)/EncodeMessage(Config.NakedAbilofXxx)`
/// 尾），而帧体**不带** `BonusPoint`/职业/三段能力模板（`SendMsg(RM_ADJUST_BONUS, 0,0,0,0)`
/// 四参全 0）⇒ 要发对必须由宿主注入 4 项额外状态。部署 `Envir` 的脚本动作里**零处**触发它
/// （`BONUSPOINT`/`RESTBONUSPOINT`/`RENEWLEVEL` 各 0 处，见 T-207 的 grep 计数）⇒
/// 按「不可达只登记、不硬造分支」处理，等真有内容触发（或需要 811 的脚本路径）时单独一片接。
///
/// 其余 ident 仍按既有口径丢弃（T-142 残余）。
/// `None` = 不发帧（未知 ident，或 `SoftVersionDateEx == 0` 门挡住）。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn script_frame(
    s: &SendMsg,
    npc_actor_id: i32,
    // 玩家自己的 `ActorId`（`SM_SYSMESSAGE` 的 `Recog`）；脚本帧排水时由会话给出。
    player_actor_id: i32,
    // `BaseObject.Gold`（`SM_GOLDCHANGED` 的 `Recog`）——排水时的当下值。
    gold: i32,
    game_gold: i32,
    game_point: i32,
    soft_version_date: i32,
    // `PlayObject.Abil.Exp`（`SM_WINEXP` 的 `Recog`）——排水时的**当下总量**（不是增量）。
    exp: i32,
) -> Option<Vec<u8>> {
    match s.ident {
        // `BaseObject.GoldChanged`（`BaseObject.cs:585-591`）⇒
        // `SM_GOLDCHANGED(653)`：`SendDefMessage(SM_GOLDCHANGED, Gold, LoWord(GameGold),
        // HiWord(GameGold), 0)`（`PlayObject.Message.cs:1833-1835`）。
        RM_GOLDCHANGED => Some(smframe::sm_goldchanged(gold, game_gold)),
        RM_SCRIPT_GAMEGOLDCHANGED => {
            if soft_version_date == 0 {
                return None;
            }
            Some(smframe::sm_gamegoldname(game_gold, game_point, &[]))
        }
        // `BaseObject.SysMsg` 的非 `Notice` 支（`BaseObject.cs:1666-1686`）：
        // `SendMsg(RM_SYSMESSAGE, 0, FColor, BColor, 0, sMsg)` ⇒ `SM_SYSMESSAGE(100)`、
        // `Recog` = 玩家 `ActorId`、`Param = MakeWord(F,B)`、`Series = 1`。
        // 尾 = `EncodeString(sMsg)`（cp936 字节；`NpcPlayer` 存的是已解码的 UTF-16 字符串）。
        i if i == mir2_protocol::messages::RM_SYSMESSAGE as i32 => {
            let msg = s.msg.as_deref().unwrap_or("");
            let bytes = mir2_script::cp936::encode_gb2312_dotnet(msg);
            // oracle 的色词是 `byte`（`GameSvrConf.*MsgFColor`）⇒ 截断到低 8 位再打包。
            Some(crate::smframe::sm_sysmessage(
                player_actor_id,
                u16::from(s.n_param1 as u8),
                u16::from(s.n_param2 as u8),
                &bytes,
            ))
        }
        // `RM_HEAR`（11001）⇒ `SM_HEAR(40)`（`PlayObject.Message.cs:1705-1707`）：
        //   `MakeMessage(SM_HEAR, processMsg.ActorId, MakeWord(nParam1, nParam2), 0, 1)`
        //   + 尾 `EncodeString(processMsg.Msg)`。
        //
        // 脚本发点 = `ActionOfLineMsg` case 4（`normNpc.SendSayMsg` ⇒ `ProcessSayMsg` ⇒
        // `SendRefMsg(RM_HEAR, 0, btHearMsgFColor, btHearMsgBColor, 0, sChrName + ':' + sMsg)`，
        // `BaseObject.Base.cs:475-478`）——`SendRefMsg` 以 **NPC** 为源 ⇒ `ActorId = normNpc.ActorId`
        // （本函数的 `npc_actor_id`），`Param = MakeWord(F, B)` 取自帧自带的 `n_param1/n_param2`
        // （`world_broadcast`/`hear` 在脚本期就写死 `btHearMsgFColor/BColor`）。
        i if i == mir2_protocol::messages::RM_HEAR as i32 => {
            let msg = s.msg.as_deref().unwrap_or("");
            let bytes = mir2_script::cp936::encode_gb2312_dotnet(msg);
            Some(smframe::sm_hear(
                npc_actor_id,
                u8::try_from(s.n_param1).unwrap_or(0),
                u8::try_from(s.n_param2).unwrap_or(0),
                &bytes,
            ))
        }
        RM_SCRIPT_PLAYDICE => {
            let msg = s.msg.as_deref().unwrap_or("");
            // 线上尾 = `EDCode.EncodeString(processMsg.Msg)`；`Msg` 在线上是 cp936 字节
            // （`HUtil32.GetBytes` 口径：不可映射回退单个 `3F`，绝不输出 `&#N;`）。
            let bytes = mir2_script::cp936::encode_gb2312_dotnet(msg);
            Some(crate::smframe_ext::sm_playdice(
                npc_actor_id,
                s.w_param,
                s.n_param1,
                s.n_param2,
                s.n_param3,
                &bytes,
            ))
        }
        // `RM_MENU_OK`（10309）⇒ `SM_MENU_OK(767)`：脚本 `MESSAGEBOX`
        // （`ActionOfMessageBox`，`ExecutionProcessingSys.cs:3416-3419`）入队的是
        // `SendMsg(normNpc, RM_MENU_OK, 0, playerActor.ActorId, 0, 0, text)`；泵
        // `PlayObject.Message.cs:1952-1953` 的 `SendDefMessage(SM_MENU_OK, processMsg.nParam1,
        // 0, 0, 0, Msg)` ⇒ `Recog` = **`nParam1`（脚本入队时写死的玩家 ActorId）**、
        // `Param/Tag/Series` = 0、尾 = `EDCode.EncodeString(cp936(Msg))`。
        //
        // **Recog 取 `nParam1` 而不是 `player_actor_id`**：oracle 读的是消息自带的
        // `processMsg.nParam1`，不是排水时刻的会话 id（`MESSAGEBOX` 由 `ActionOfMessageBox`
        // 写 `playerActor.ActorId`，两者在生产路径上同值；显式取 `nParam1` 才能同时覆盖
        // 其它写入点的帧）。`SendDefMessage` 的 `string.IsNullOrEmpty(Msg)` 空尾支由
        // `sm_menu_ok(_, &[])` 保持（`edcode::encode(&[])` 为空 ⇒ 12B header-only）。
        i if i == mir2_protocol::messages::RM_MENU_OK as i32 => {
            let msg = s.msg.as_deref().unwrap_or("");
            let bytes = mir2_script::cp936::encode_gb2312_dotnet(msg);
            Some(smframe::sm_menu_ok(s.n_param1, &bytes))
        }
        // `RM_WINEXP`（8075）⇒ `SM_WINEXP(44)`（`PlayObject.Message.cs:1666-1669`）：
        //   `MakeMessage(SM_WINEXP, Abil.Exp, LoWord(processMsg.nParam1), HiWord(...), 0)`
        //   + `SendSocket(ClientMsg)` ⇒ **无尾**。
        // 脚本发点 `ExecutionProcessingSys.cs:2861`（`ActionOfChangeExp` 的 `'+'` 支）：
        //   `SendMsg(playerActor, RM_WINEXP, 0, dwInt, 0, 0)`（`nParam1` = 本次增量）。
        // `Recog` = **加完之后的总量** `Abil.Exp` ⇒ 用排水时的会话当下值 `exp`，不是增量。
        RM_SCRIPT_WINEXP => Some(smframe::sm_winexp(exp, s.n_param1)),
        // `RM_DURACHANGE`（10125）⇒ `SM_DURACHANGE(642)`（`PlayObject.Message.cs:1803-1805`）：
        //   `SendDefMessage(SM_DURACHANGE, nParam1 /*Dura*/, wParam /*槽位*/,
        //   LoWord(nParam2) /*DuraMax*/, HiWord(nParam2))`，无尾。
        // 脚本发点 `ExecutionProcessingSys.cs:3945`（`ActionOfRepairAllItem`）：
        //   `SendMsg(normNpc, RM_DURACHANGE, (short)i, Dura, DuraMax, 0)`。
        // `Dura`/`DuraMax` 在 oracle 是 `ushort`，`MakeMessage` 再把 Param/Tag/Series 截成 16 位
        // ⇒ 取低 16 位（用掩码 + `as`，不用 `try_from(...).unwrap_or(0)`：掩码后必成功，那个
        // `unwrap_or` 是死码）。
        //
        // **口径差（登记，实语料不可达）**：槽位在 oracle 走 `(ushort)param`（16 位），本支经
        // `smframe::sm_durachange(slot: u8)` 只保留低 8 位。发点 `ActionOfRepairAllItem` 的槽位 =
        // `ItemLocation`（0..12）⇒ 差不出；要 1:1 需把该签名改成 `u16`，那会牵动 `smframe.rs`
        // 与 lib.rs 的 9 处既有调用点（他线写集）⇒ 本片不扩面，记为口径差。
        RM_SCRIPT_DURACHANGE => Some(smframe::sm_durachange(
            (s.n_param1 & 0xFFFF) as u16,
            (s.w_param & 0xFF) as u8,
            (s.n_param2 & 0xFFFF) as u16,
        )),
        // `RM_MERCHANTDLGCLOSE`（10127）⇒ `SM_MERCHANTDLGCLOSE(644)`
        // （`PlayObject.Message.cs:1806-1808`）：`SendDefMessage(SM_MERCHANTDLGCLOSE,
        // processMsg.nParam1, 0, 0, 0)` ⇒ **header-only**，无尾。
        // 脚本发点 `ExecutionProcessingSys.cs:2183`（`ActionOfClose`）：
        //   `SendMsg(normNpc, RM_MERCHANTDLGCLOSE, 0, normNpc.ActorId, 0, 0)`
        // ⇒ `Recog` = `nParam1` = **NPC** 的 `ActorId`（oracle 读消息自带值，不是排水时的
        // `npc_actor_id`，也不是玩家 `ActorId`）。
        RM_SCRIPT_MERCHANTDLGCLOSE => Some(smframe::sm_merchantdlgclose(s.n_param1)),
        _ => None,
    }
}

/// 一次 pickup/drop 的完整下行序列 = **立即帧（按 `ground::*` 内序）** 再 **延时通知（按出队序）**。
#[must_use]
pub fn outcome_frames(
    immediate: &[Frame],
    notices: &[QueuedNotice],
    session: &WireSession,
    drop_msg: &[u8],
    add_item: Option<&BagItem>,
    catalog: &dyn ItemCatalog,
) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(immediate.len() + notices.len());
    for f in immediate {
        if let Some(b) = immediate_frame(f, drop_msg, add_item, catalog) {
            out.push(b);
        }
    }
    for n in notices {
        if let Some(b) = notice_frame(n, session) {
            out.push(b);
        }
    }
    out
}

/// 用 world 的只读视图（owner §25.1 (A)）装配 `ground::GroundMap`。
///
/// `width`/`height` = 该图的真尺寸（`World::maps[map_id]`）；视图坐标是绝对坐标。
/// 不在视图里的格子留空。字段缺口见模块头「已知缺口 2」。
#[must_use]
pub fn build_ground_map(
    view: &[FloorItemView],
    map_id: usize,
    width: i32,
    height: i32,
    catalog: &dyn ItemCatalog,
) -> GroundMap {
    let mut map = GroundMap::new(width, height);
    for v in view.iter().filter(|v| v.map_id == map_id) {
        let item = match v.kind {
            FloorItemKind::Gold => {
                GroundItem::gold(v.item_id, v.count, v.of_base_object, v.can_pick_up_tick)
            }
            FloorItemKind::Item => {
                let (name, looks, ani_count) = catalog.get(v.index).map_or_else(
                    || (String::new(), 0u16, 0u8),
                    |t| (t.name.clone(), t.looks, t.ani_count),
                );
                GroundItem {
                    item_id: v.item_id,
                    name,
                    looks,
                    ani_count,
                    reserved: 0,
                    count: v.count,
                    drop_base_object: 0,
                    of_base_object: v.of_base_object,
                    can_pick_up_tick: v.can_pick_up_tick,
                    // 缺口 2：dura/dura_max/desc 不在只读视图里 ⇒ 默认值。
                    user_item: Some(BagItem {
                        make_index: v.item_id,
                        index: v.index,
                        dura: 0,
                        dura_max: 0,
                        desc: [0; 14],
                    }),
                }
            }
        };
        let _ = map.put_item(v.x, v.y, item);
    }
    map
}

/// `CM_PICKUP` 的**位置门**（`PlayObject.Message.cs:811-816`）：`CurrX==nParam2 && CurrY==nParam3`。
///
/// 不满足时 oracle 直接不进 `ClientPickUpItem`。
#[must_use]
pub fn pickup_position_ok(curr_x: i32, curr_y: i32, param2: i32, param3: i32) -> bool {
    curr_x == param2 && curr_y == param3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::StdItemCatalog;
    use mir2_data::StdItem;
    use mir2_protocol::edcode;
    use serde_json::Value;

    fn fixture() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/ground_vectors.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("oracle 向量夹具缺失/不可读：{path}（{e}）—— 不许静默跳过"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} 不是合法 JSON：{e}"))
    }

    fn catalog_from(doc: &Value) -> StdItemCatalog {
        let mut c = StdItemCatalog::new();
        for (k, v) in doc["templates"].as_object().unwrap() {
            c.insert(
                k.parse::<u16>().unwrap(),
                StdItem {
                    name: v["name"].as_str().unwrap().to_string(),
                    std_mode: v["std_mode"].as_u64().unwrap() as u8,
                    shape: v["shape"].as_u64().unwrap() as u8,
                    weight: v["weight"].as_u64().unwrap() as u8,
                    price: v["price"].as_i64().unwrap() as i32,
                    looks: v["looks"].as_u64().unwrap() as u16,
                    ani_count: v["ani_count"].as_u64().unwrap() as u8,
                    dura_max: v["dura_max"].as_u64().unwrap() as u16,
                    ..StdItem::default()
                },
            );
        }
        c
    }

    fn wire_ability() -> Ability {
        Ability {
            level: 1,
            reserved1: 0,
            ac: 0,
            mac: 0,
            dc: 0,
            mc: 0,
            sc: 0,
            hp: 0,
            mp: 0,
            max_hp: 0,
            max_mp: 0,
            reserved2: 0,
            exp_count: 0,
            exp_max_count: 0,
            exp: 0,
            max_exp: 0,
            weight: 100,
            max_weight: 0,
            wear_weight: 5,
            max_wear_weight: 0,
            hand_weight: 2,
            max_hand_weight: 0,
        }
    }

    fn session(actor_id: i32, gold: i32, game_gold: i32, sys_msg: Option<&[u8]>) -> WireSession {
        WireSession {
            actor_id,
            gold,
            game_gold,
            wabil: wire_ability(),
            abil_exp: 0,
            sys_msg: sys_msg.map(<[u8]>::to_vec),
            job: 0,
            ability_tail: Vec::new(),
            sub_bytes: [0; 7],
            hunger_status: 0,
            hit_speed: 0,
        }
    }

    /// **`RM_ABILITY`→`SM_ABILITY(52)` / `RM_SUBABILITY`→`SM_SUBABILITY(752)` 映射**
    /// （oracle `PlayObject.Message.cs:1732-1735` / `:1934-1935`；卡 `openmir2-items-parity` §9.16 的 52 缺口）。
    /// 逐字段机判（头 + 尾字节），并钉住"尾 = `Encode(WAbil 原文)`、不再二次编码"。
    #[test]
    fn notice_ability_and_subability_map_field_by_field() {
        let plain = |ident: i32| QueuedNotice {
            ident,
            w_param: 0,
            n_param1: 0,
            n_param2: 0,
            n_param3: 0,
            text: NoticeText::None,
        };
        let mut s = session(7, 12_345, 0x0002_0001, None);
        s.job = 3;
        s.ability_tail = vec![0x41, 0x42];
        let body = notice_frame(&plain(RM_ABILITY), &s).expect("RM_ABILITY 必须映射");
        let m = mir2_protocol::frame::CommandMessage::from_bytes(&body).expect("SM_ABILITY 可解析");
        assert_eq!(m.ident, mir2_protocol::messages::SM_ABILITY);
        assert_eq!(m.recog, 12_345, "Recog = Gold");
        assert_eq!(m.param, 0x6303, "Param = MakeWord(Job=3, 99)");
        assert_eq!(m.tag, 0x0001, "Tag = LoWord(GameGold)");
        assert_eq!(m.series, 0x0002, "Series = HiWord(GameGold)");
        assert_eq!(
            &body[mir2_protocol::frame::CommandMessage::SIZE..],
            &edcode::encode(&[0x41, 0x42])[..],
            "尾 = EDCode(WAbil 原文)，不二次编码"
        );

        s.sub_bytes = [0x1234, 1, 2, 3, 4, 5, 6];
        let body = notice_frame(&plain(RM_SUBABILITY), &s).expect("RM_SUBABILITY 必须映射");
        let m =
            mir2_protocol::frame::CommandMessage::from_bytes(&body).expect("SM_SUBABILITY 可解析");
        assert_eq!(m.ident, mir2_protocol::messages::SM_SUBABILITY);
        assert_eq!(m.recog, 0x1234, "Recog = AntiMagic 完整 16 位");
        assert_eq!(m.param, 0x0201, "MakeWord(HitPoint, SpeedPoint)");
        assert_eq!(m.tag, 0x0403, "MakeWord(AntiPoison, PoisonRecover)");
        assert_eq!(m.series, 0x0605, "MakeWord(HealthRecover, SpellRecover)");
        assert_eq!(
            body.len(),
            mir2_protocol::frame::CommandMessage::SIZE,
            "无尾"
        );
    }
    /// **IT-08 同源错位牙**：`notice_frame` 的 `RM_WINEXP` 必须用**基础** `Abil.Exp`（`abil_exp`）当
    /// `SM_WINEXP.Recog`，**不是** `WAbil.Exp`（非升级 `GetExp` 后 WAbil 陈旧）。WAbil.exp=0、base=100 ⇒ Recog 必须 100。
    /// 反向牙：把该分支改成读 `session.wabil.exp` ⇒ Recog=0 ⇒ 本用例红。
    #[test]
    fn notice_winexp_uses_base_exp_not_wabil() {
        let mut s = session(7, 0, 0, None);
        s.wabil.exp = 0; // 陈旧 WAbil.Exp
        s.abil_exp = 100; // 基础 Abil.Exp
        let n = QueuedNotice {
            ident: RM_SCRIPT_WINEXP,
            w_param: 0,
            n_param1: 50,
            n_param2: 0,
            n_param3: 0,
            text: NoticeText::None,
        };
        let body = notice_frame(&n, &s).expect("SM_WINEXP");
        let recog = i32::from_le_bytes([body[0], body[1], body[2], body[3]]);
        assert_eq!(
            recog, 100,
            "SM_WINEXP.Recog = 基础 Abil.Exp（非陈旧 WAbil.Exp）"
        );
        let param = u16::from_le_bytes([body[6], body[7]]);
        assert_eq!(param, 50, "SM_WINEXP.Param = LoWord(nParam1=增量)");
    }

    fn header(recog: i32, ident: u16, param: u16, tag: u16, series: u16) -> Vec<u8> {
        let mut v = vec![0u8; 12];
        v[0..4].copy_from_slice(&recog.to_le_bytes());
        v[4..6].copy_from_slice(&ident.to_le_bytes());
        v[6..8].copy_from_slice(&param.to_le_bytes());
        v[8..10].copy_from_slice(&tag.to_le_bytes());
        v[10..12].copy_from_slice(&series.to_le_bytes());
        v
    }

    /// 立即帧：`SM_DROPITEM_SUCCESS/FAIL`（尾 = `EncodeString(Msg)`）；`SM_ADDITEM` 的尾 =
    /// `EncodeBuffer(ClientItem 124B)`（G-1：`PlayObject.Base.cs:225-227`）。
    ///
    /// 红检：把 `sm_additem` 的 `series` 从 1 改成 0，或去掉尾 ⇒ `add` 断言必红。
    #[test]
    fn immediate_frames_map_to_oracle_bytes() {
        let doc = fixture();
        let catalog = catalog_from(&doc);
        let item = BagItem {
            make_index: 7,
            index: 3001,
            dura: 10,
            dura_max: 20,
            desc: [0; 14],
        };
        // 期望尾独立走 `itemwire::encode_client_item`（124B 编码器自己的单测在 itemwire）。
        let ci = crate::itemwire::ClientItem {
            tpl: catalog.get(3001).expect("模板 3001"),
            name: "ring".to_string(),
            make_index: 7,
            dura: 10,
            dura_max: 20,
            desc: [0; 14],
        };
        let tail = edcode::encode(&crate::itemwire::encode_client_item(&ci));
        assert_eq!(tail.len(), 166, "encode_len(124) = 166");
        let mut expect_add = header(0x1234, 200, 0, 0, 1);
        expect_add.extend_from_slice(&tail);
        let add = immediate_frame(
            &Frame {
                ident: SM_ADDITEM,
                recog: 0x1234,
                param: 0,
                tag: 0,
                series: 1,
            },
            b"",
            Some(&item),
            &catalog,
        )
        .expect("SM_ADDITEM 必须可映射（带物品）");
        assert_eq!(add, expect_add);
        assert!(
            immediate_frame(
                &Frame {
                    ident: SM_ADDITEM,
                    recog: 0x1234,
                    param: 0,
                    tag: 0,
                    series: 1,
                },
                b"",
                None,
                &catalog,
            )
            .is_none(),
            "缺物品输入 ⇒ 不产出（不许静默 header-only）"
        );

        let msg = b"ring";
        let mut expect_fail = header(70001, 601, 0, 0, 0);
        expect_fail.extend_from_slice(&edcode::encode(msg));
        let fail = immediate_frame(
            &Frame {
                ident: SM_DROPITEM_FAIL,
                recog: 70001,
                param: 0,
                tag: 0,
                series: 0,
            },
            msg,
            None,
            &catalog,
        )
        .expect("SM_DROPITEM_FAIL 必须可映射");
        assert_eq!(fail, expect_fail);

        let mut expect_ok = header(70001, 600, 0, 0, 0);
        expect_ok.extend_from_slice(&edcode::encode(msg));
        let ok = immediate_frame(
            &Frame {
                ident: SM_DROPITEM_SUCCESS,
                recog: 70001,
                param: 0,
                tag: 0,
                series: 0,
            },
            msg,
            None,
            &catalog,
        )
        .unwrap();
        assert_eq!(ok, expect_ok);
    }

    /// 五条 `RM_*` 的映射（`recog/param/tag/series` 落位逐字节）。
    ///
    /// 红检：把 `RM_GOLDCHANGED` 的 `series` 从 0 改成 1 ⇒ `gold` 断言必红。
    #[test]
    fn notices_map_to_oracle_frames() {
        let s = session(0x7C30_3998, 1000, 0x0001_0002, Some(b"cannot"));

        let itemhide = notice_frame(
            &QueuedNotice {
                ident: RM_ITEMHIDE,
                w_param: 0,
                n_param1: 70001,
                n_param2: 32,
                n_param3: 33,
                text: NoticeText::None,
            },
            &s,
        )
        .unwrap();
        assert_eq!(itemhide, header(70001, 611, 32, 33, 0));

        let weight = notice_frame(
            &QueuedNotice {
                ident: RM_WEIGHTCHANGED,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::None,
            },
            &s,
        )
        .unwrap();
        let chk = ((107i32 ^ 0x3A5F) ^ 0x1F35) ^ 0xaa21;
        assert_eq!(weight, header(100, 622, 5, 2, chk as u16));

        let gold = notice_frame(
            &QueuedNotice {
                ident: RM_GOLDCHANGED,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::None,
            },
            &s,
        )
        .unwrap();
        assert_eq!(gold, header(1000, 653, 0x0002, 0x0001, 0));

        let mut expect_sys = header(0x7C30_3998, 100, 0x00FF, 0, 1);
        expect_sys.extend_from_slice(&edcode::encode(b"cannot"));
        let sys = notice_frame(
            &QueuedNotice {
                ident: RM_SYSMESSAGE,
                w_param: 0,
                n_param1: 0xFF,
                n_param2: 0x00,
                n_param3: 0,
                // 取件通知：**唯一**允许用会话静态文本的种类（`MessageSettings.CanotPickUpItem`）。
                text: NoticeText::SessionPickup,
            },
            &s,
        )
        .unwrap();
        assert_eq!(sys, expect_sys);

        let menu = notice_frame(
            &QueuedNotice {
                ident: RM_MENU_OK,
                w_param: 0,
                n_param1: 4660,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::None,
            },
            &s,
        )
        .unwrap();
        assert_eq!(menu, header(4660, 767, 0, 0, 0));
    }

    /// **制药/食药片 WB**：`RM_MYSTATUS`（`PlayObject.Base.cs:291` 的 `RefMyStatus`）⇒
    /// `SM_MYSTATUS(708)`，`Param = (u16)(short)GetMyStatus()`、`Recog=0`，其中
    /// `GetMyStatus() = min(HungerStatus/1000, 4)`（当下值）。
    ///
    /// 变异必红：把 `Recog` 从 0 改成 `actor_id`、或不做 `/1000` 截断（直接用 HungerStatus）⇒ 必红。
    #[test]
    fn mystatus_maps_to_sm_mystatus_with_derived_status() {
        let mut s = session(0x7C30_3998, 0, 0, None);
        s.hunger_status = 4500; // ⇒ GetMyStatus() = 4
        let body = notice_frame(
            &QueuedNotice {
                ident: RM_MYSTATUS,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::None,
            },
            &s,
        )
        .expect("RM_MYSTATUS 必须可映射");
        assert_eq!(body, header(0, 708, 4, 0, 0));
        assert_eq!(body, crate::smframe_ext::sm_mystatus(4));
        // 跨档下界（999 ⇒ 0）。
        s.hunger_status = 999;
        let low = notice_frame(
            &QueuedNotice {
                ident: RM_MYSTATUS,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::None,
            },
            &s,
        )
        .unwrap();
        assert_eq!(low, header(0, 708, 0, 0, 0));
    }

    /// **卡 `openmir2-charstatuschanged-gap`**：`RM_CHARSTATUSCHANGED(10139)` 的出队口径
    /// （`PlayObject.Message.cs:1845-1847`）：`Recog = processMsg.ActorId`、
    /// `Param/Tag = LoWord/HiWord(nParam1)`、`Series = 收件人自己的 HitSpeed`。
    /// 本条是**发送者自己那份**自通知 ⇒ `Series` 取本会话的 `hit_speed`。
    #[test]
    fn charstatuschanged_maps_to_sm_657_with_recipient_hit_speed() {
        let mut s = session(0x7C30_3998, 0, 0, None);
        s.hit_speed = 0x1234;
        let body = notice_frame(
            &QueuedNotice {
                ident: RM_CHARSTATUSCHANGED,
                w_param: 0,
                n_param1: 0x0002_0001,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::None,
            },
            &s,
        )
        .expect("10139 必须映射（此前被丢弃）");
        assert_eq!(body, header(0x7C30_3998, 657, 0x0001, 0x0002, 0x1234));
        assert_eq!(
            body,
            crate::smframe_ext::sm_charstatuschanged(0x7C30_3998, 0x0002_0001, 0x1234)
        );
    }

    /// **泵序（三条同批）**：`RM_WEIGHTCHANGED(10115→131) < RM_CHARSTATUSCHANGED(10139→155)
    /// < RM_SYSMESSAGE(11004→252)`。
    ///
    /// ⚠️ **口径**：真实泵是**堆出队**（`NetPriorityQueue`，章程 §29），**不是排序**。
    /// 本条用例用 `sort_by_key(|n| n.ident as u8)` 近似，**仅因三条的低字节互不相同
    /// （131/155/252）**——等键（同 `(byte)ident`）时必须用真实的 `NetPriorityQueue`，
    /// 不得照抄本写法当通用做法。
    #[test]
    fn charstatuschanged_orders_between_weightchanged_and_sysmessage() {
        let s = session(7, 0, 0, Some(&b"txt"[..]));
        let mut notices = [
            QueuedNotice {
                ident: RM_SYSMESSAGE,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::Inline("x".to_string()),
            },
            QueuedNotice {
                ident: RM_CHARSTATUSCHANGED,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::None,
            },
            QueuedNotice {
                ident: RM_WEIGHTCHANGED,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::None,
            },
        ];
        notices.sort_by_key(|n| n.ident as u8);
        let idents: Vec<u16> = notices
            .iter()
            .map(|n| {
                let body = notice_frame(n, &s).expect("三条都必须有 SM_* 映射");
                u16::from_le_bytes([body[4], body[5]])
            })
            .collect();
        assert_eq!(
            idents,
            vec![622, 657, 100],
            "泵序 = (byte)RM ident 升序：131 < 155 < 252"
        );
    }

    /// **卡 `openmir2-menu-ok-text`**：`RM_MENU_OK` 的帧体 = 12B 头 ++ `EDCode.GBK(文案)`。
    /// 三个生产者文案不同，这里用 `use_item` 那条（`CanotUseItemMsg`）钉编码路径；
    /// `None` ⇒ 空尾（保持 header-only 的旧行为）。
    #[test]
    fn menu_ok_frame_carries_text_tail() {
        let s = session(4660, 0, 0, None);
        let body = notice_frame(
            &QueuedNotice {
                ident: RM_MENU_OK,
                w_param: 0,
                n_param1: 4660,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::Inline(crate::strings::DEFAULT_CANOT_USE_ITEM_MSG.to_string()),
            },
            &s,
        )
        .expect("RM_MENU_OK 必须产帧");
        let mut want = header(4660, mir2_protocol::messages::SM_MENU_OK, 0, 0, 0);
        want.extend_from_slice(&edcode::encode(&crate::enter::gbk(
            crate::strings::DEFAULT_CANOT_USE_ITEM_MSG,
        )));
        assert_eq!(body, want, "整帧 = 头 ++ EDCode(GBK(文案))");

        // **独立 oracle 抓帧**（不现算）：`oracle_tail` = 由 oracle
        // `EDCode.EncodeString(GBK("当前无法进行此操作!!!"))` 得到、并经 QA 独立复算验证的
        // **冻结字节**（等价值，不是夹具拷贝；`SM_MENU_OK = 767` 见
        // `crates/protocol/src/messages.rs:354`）。
        // 现算的 `want` 与被测 `body` 共用 `edcode::encode`/`enter::gbk` ⇒ 原语错了会一起错；
        // 这条硬编码抓帧才能咬住"编码错"。
        let oracle_tail: &[u8] = b"AAgW@VnXC<MEPXxS@WZEY[WR]]Ik";
        let mut oracle_frame = header(4660, mir2_protocol::messages::SM_MENU_OK, 0, 0, 0);
        oracle_frame.extend_from_slice(oracle_tail);
        assert_eq!(
            body, oracle_frame,
            "整帧 == oracle 抓帧（硬编码 hex，不现算）"
        );

        // 无文本 ⇒ 空尾（与旧行为一致）。
        let bare = notice_frame(
            &QueuedNotice {
                ident: RM_MENU_OK,
                w_param: 0,
                n_param1: 1,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::None,
            },
            &s,
        )
        .expect("无文本也产帧（空尾）");
        let mut want_bare = header(1, mir2_protocol::messages::SM_MENU_OK, 0, 0, 0);
        want_bare.extend_from_slice(&edcode::encode(&[]));
        assert_eq!(bare, want_bare, "无文本 ⇒ header-only");
    }

    /// **负例（卡 §4「回退按通知种类收窄」）**：通知**无文本**（`NoticeText::None`）时，
    /// 即使会话里恰好留着取件静态文本（`sys_msg = Some(...)`），也**不产帧**——否则取件文案
    /// 会串到吃饭/彩票帧上。同一条通知标成 `SessionPickup` 时才走会话回退（证明通道仍在）。
    #[test]
    fn sysmessage_without_text_is_skipped_even_with_session_text() {
        let s = session(1, 0, 0, Some(&b"cannot-pick-up"[..]));
        assert!(
            notice_frame(
                &QueuedNotice {
                    ident: RM_SYSMESSAGE,
                    w_param: 0,
                    n_param1: 0,
                    n_param2: 0,
                    n_param3: 0,
                    text: NoticeText::None,
                },
                &s
            )
            .is_none(),
            "无文本通知不得借用会话静态文本"
        );
        let pickup = notice_frame(
            &QueuedNotice {
                ident: RM_SYSMESSAGE,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
                text: NoticeText::SessionPickup,
            },
            &s,
        )
        .expect("取件通知回退会话文本");
        let mut expect = header(1, 100, 0, 0, 1);
        expect.extend_from_slice(&edcode::encode(b"cannot-pick-up"));
        assert_eq!(pickup, expect);
    }

    /// **文本不串**（卡 §4 负例之二）：`Inline` 通知 + 会话里留着取件静态文本 ⇒ 帧尾必须是
    /// `Inline` 的 GBK 字节、**不是**会话文本；同时钉住「整帧 = 头 ++ EDCode(gbk(文本))」
    /// 这条唯一编码路径（卡 §3-3）。
    #[test]
    fn sysmessage_inline_text_never_uses_session_text() {
        let s = session(9, 0, 0, Some(&b"CANOT-PICKUP"[..]));
        let text = "武器修复成功...";
        let body = notice_frame(
            &QueuedNotice {
                ident: RM_SYSMESSAGE,
                w_param: 0,
                n_param1: 0xDB,
                n_param2: 0xFF,
                n_param3: 0,
                text: NoticeText::Inline(text.to_string()),
            },
            &s,
        )
        .expect("Inline 文本必须产帧");
        let mut expect = header(9, 100, 0xFFDB, 0, 1);
        expect.extend_from_slice(&edcode::encode(&crate::enter::gbk(text)));
        assert_eq!(body, expect, "整帧 = 头 ++ EDCode(gbk(Inline))");
        let mut session_frame = header(9, 100, 0xFFDB, 0, 1);
        session_frame.extend_from_slice(&edcode::encode(b"CANOT-PICKUP"));
        assert_ne!(body, session_frame, "不得回落到会话文本");
    }

    /// **整帧字节级对拍**（卡 `openmir2-runtime-sysmsg-text-frames` §3-3「编码路径只有一条」）：
    /// 取 oracle 探针实测的两条 `notices[].text`（`EDCode.EncodeString(Buff)` 的原始字节，
    /// 见 `eat_vectors.json` 的 `eat_fail_nodrug_no_consume` 与 `eat_ok_shape9_repair_normal`），
    /// 断言 `notice_frame` 的输出与「12B 头 ++ 文本尾」**全等**。
    /// 外部锚（不是本仓编码器自证）：任何"多做一次转换"的写法都会在这里红。
    #[test]
    fn sysmessage_full_frame_matches_oracle_bytes() {
        fn hex(s: &str) -> Vec<u8> {
            (0..s.len() / 2)
                .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex"))
                .collect()
        }
        let s = session(4660, 0, 0, None);

        // 禁药红字：`RedMsgFColor=255 / RedMsgBColor=56`（`eat_fail_nodrug_no_consume`）。
        let drug_hex = "4057554558554c404243745956583d5356417b5157545e5942567a503d5657425d5d496b";
        let got = notice_frame(
            &QueuedNotice {
                ident: RM_SYSMESSAGE,
                w_param: 0,
                n_param1: 255,
                n_param2: 56,
                n_param3: 0,
                text: NoticeText::Inline(
                    crate::strings::DEFAULT_CANOT_USE_DRUG_ON_THIS_MAP.to_string(),
                ),
            },
            &s,
        )
        .expect("禁药红字");
        let mut want = header(4660, 100, 0x38FF, 0, 1);
        want.extend_from_slice(&hex(drug_hex));
        assert_eq!(got, want, "禁药：整帧 == oracle 头 ++ EDCode 文本");

        // 修复油绿字：`GreenMsgFColor=219 / GreenMsgBColor=255`（`eat_ok_shape9_repair_normal`）。
        let repair_hex = "564c664e53586e5740405b42554146415e5e3e5c";
        let got = notice_frame(
            &QueuedNotice {
                ident: RM_SYSMESSAGE,
                w_param: 0,
                n_param1: 219,
                n_param2: 255,
                n_param3: 0,
                text: NoticeText::Inline(crate::strings::DEFAULT_WEAPON_REPAIR_SUCCESS.to_string()),
            },
            &s,
        )
        .expect("修复油绿字");
        let mut want = header(4660, 100, 0xFFDB, 0, 1);
        want.extend_from_slice(&hex(repair_hex));
        assert_eq!(got, want, "修复油：整帧 == oracle 头 ++ EDCode 文本");
    }

    /// 装配：只装目标图；金币 `item_id`/`count`/归属、物品模板名逐项对上 world 视图。
    ///
    /// 红检：把 `item_id: v.item_id` 改成 `v.item_id + 1` ⇒ 本用例必红。
    #[test]
    fn build_ground_map_matches_view() {
        let doc = fixture();
        let catalog = catalog_from(&doc);
        let view = vec![
            FloorItemView {
                map_id: 0,
                x: 32,
                y: 33,
                obj_id: 1234,
                kind: FloorItemKind::Item,
                index: 3001,
                count: 1,
                item_id: 70001,
                of_base_object: 0,
                can_pick_up_tick: 1_200_000,
            },
            FloorItemView {
                map_id: 0,
                x: 40,
                y: 41,
                obj_id: 7,
                kind: FloorItemKind::Gold,
                index: 0,
                count: 555,
                item_id: 7,
                of_base_object: 42,
                can_pick_up_tick: 1_200_000,
            },
            FloorItemView {
                map_id: 1,
                x: 1,
                y: 1,
                obj_id: 1,
                kind: FloorItemKind::Gold,
                index: 0,
                count: 1,
                item_id: 9,
                of_base_object: 0,
                can_pick_up_tick: 0,
            },
        ];
        let map = build_ground_map(&view, 0, 64, 64, &catalog);
        assert_eq!(map.item_count(), 2, "只装配 map_id=0 的两件");
        let it = map.get_item(32, 33).unwrap();
        assert_eq!(it.item_id, 70001);
        assert_eq!(it.count, 1);
        assert_eq!(it.name, "ring");
        assert_eq!(it.user_item.as_ref().unwrap().index, 3001);
        let g = map.get_item(40, 41).unwrap();
        assert_eq!(g.name, crate::ground::GOLD_NAME);
        assert_eq!(g.count, 555);
        assert_eq!(g.of_base_object, 42);
    }

    /// **`RM_PLAYDICE` 映射**（`PlayObject.Message.cs:2034-2041`）：
    /// `ActorId` = 入队 NPC、`wParam` 原样、WL 三参 = 脚本 nParam1..3、尾 = `EncodeString(Msg)`。
    /// 牙①：删 `script_frame` 的 `RM_SCRIPT_PLAYDICE` 分支 ⇒ `None`，本用例红。
    /// 牙②：尾编码改回 `encoding_rs::GBK.encode` ⇒ `U+E7C8` 变成 `&#59336;`（或回退 `?`），
    /// 下面的 `A8 BF` 期望字节必红（cp936-only 496 表）。
    #[test]
    fn script_frame_maps_playdice_to_sm_playdice() {
        let s = SendMsg {
            ident: RM_SCRIPT_PLAYDICE,
            w_param: 1,
            n_param1: 0x0102_0304,
            n_param2: 0x0506_0708,
            n_param3: 0x090a_0b0c,
            msg: Some("@cube82\u{E7C8}".to_string()),
        };
        let got = script_frame(&s, 4242, 9001, 0, 0, 0, 0, 0).expect("RM_PLAYDICE 必须映射");
        let want = crate::smframe_ext::sm_playdice(
            4242,
            1,
            0x0102_0304,
            0x0506_0708,
            0x090a_0b0c,
            b"@cube82\xA8\xBF",
        );
        assert_eq!(got, want, "SM_PLAYDICE 字段/次序必须与泵逐字节一致");
    }

    /// `CM_PICKUP` 的位置门（派发层口径）。
    #[test]
    fn pickup_position_gate() {
        assert!(pickup_position_ok(10, 20, 10, 20));
        assert!(!pickup_position_ok(10, 20, 11, 20));
    }

    /// **`RM_WINEXP`(8075) → `SM_WINEXP`(44)**：`PlayObject.Message.cs:1666-1669`
    /// `MakeMessage(SM_WINEXP, Abil.Exp, LoWord(nParam1), HiWord(nParam1), 0)` + `SendSocket`
    /// （**无尾**）。脚本发点 = `ExecutionProcessingSys.cs:2861`（`ActionOfChangeExp` 的 `'+'` 支：
    /// `SendMsg(playerActor, RM_WINEXP, 0, dwInt, 0, 0)`）。
    ///
    /// 断言钉死四件事：`Recog` = **总量**（不是增量）、`Param/Tag` = 增量的 Lo/Hi、`Series` = 0、
    /// 尾为空（12B）。
    ///
    /// 牙①：删 `RM_SCRIPT_WINEXP` 分支 ⇒ 返回 `None`，本用例红。
    /// 牙②：把 `sm_winexp(exp, s.n_param1)` 写成 `(s.n_param1, exp)`（`Recog`/`Param` 对调）
    /// ⇒ `Recog=1234567/Param=0x86A0` 的帧断言红。
    #[test]
    fn script_frame_maps_winexp_to_sm_winexp() {
        let s = SendMsg {
            ident: RM_SCRIPT_WINEXP,
            w_param: 0,
            n_param1: 100_000, // `dwInt`（本次增量）
            n_param2: 0,
            n_param3: 0,
            msg: None,
        };
        let got = script_frame(&s, 4242, 9001, 0, 0, 0, 0, 1_234_567).expect("RM_WINEXP 必须映射");
        // 100000 = 0x0001_86A0 ⇒ Param = LoWord = 0x86A0、Tag = HiWord = 0x0001。
        assert_eq!(got, header(1_234_567, 44, 0x86A0, 0x0001, 0));
        assert_eq!(got.len(), 12, "`SendSocket(ClientMsg)` ⇒ 无尾");
        assert_eq!(got, crate::smframe::sm_winexp(1_234_567, 100_000));
        // 反例锚：`Recog` 若是增量 / `Param` 若是总量 ⇒ 上面断言红。
        assert_ne!(got, header(100_000, 44, 0x86A0, 0x0001, 0));
    }

    /// **`RM_DURACHANGE`(10125) → `SM_DURACHANGE`(642)**：`PlayObject.Message.cs:1803-1805`
    /// `SendDefMessage(SM_DURACHANGE, nParam1 /*Dura*/, wParam /*槽位*/,
    /// LoWord(nParam2) /*DuraMax*/, HiWord(nParam2))`。脚本发点 =
    /// `ExecutionProcessingSys.cs:3945`（`ActionOfRepairAllItem`：
    /// `SendMsg(normNpc, RM_DURACHANGE, (short)i, Dura, DuraMax, 0)`）。
    ///
    /// 牙①：删 `RM_SCRIPT_DURACHANGE` 分支 ⇒ 用例红。
    /// 牙②：`Recog`/`Param` 对调（`sm_durachange(slot, dura, dura_max)`）⇒ 帧断言红。
    #[test]
    fn script_frame_maps_durachange_to_sm_durachange() {
        let s = SendMsg {
            ident: RM_SCRIPT_DURACHANGE,
            w_param: 5,     // 槽位（`ItemLocation`）
            n_param1: 3200, // Dura
            n_param2: 4000, // DuraMax
            n_param3: 0,
            msg: None,
        };
        let got = script_frame(&s, 4242, 9001, 0, 0, 0, 0, 0).expect("RM_DURACHANGE 必须映射");
        assert_eq!(got, header(3200, 642, 5, 4000, 0));
        assert_eq!(got.len(), 12, "`SendDefMessage` 无尾");
        assert_eq!(got, crate::smframe::sm_durachange(3200, 5, 4000));
        assert_ne!(
            got,
            header(5, 642, 3200, 4000, 0),
            "Recog 是 Dura，不是槽位"
        );
    }

    /// **`RM_MERCHANTDLGCLOSE`(10127) → `SM_MERCHANTDLGCLOSE`(644)**：
    /// `PlayObject.Message.cs:1806-1808` 的 `SendDefMessage(SM_MERCHANTDLGCLOSE,
    /// processMsg.nParam1, 0, 0, 0)`（**header-only**）。脚本发点 =
    /// `ExecutionProcessingSys.cs:2183`（`ActionOfClose`：
    /// `SendMsg(normNpc, RM_MERCHANTDLGCLOSE, 0, normNpc.ActorId, 0, 0)`）⇒ `Recog` =
    /// **NPC** 的 `ActorId`。
    ///
    /// 牙①：删 `RM_SCRIPT_MERCHANTDLGCLOSE` 分支 ⇒ 用例红。
    /// 牙②：`Recog` 改用排水时的 `npc_actor_id`/`player_actor_id`（oracle 读消息自带
    /// `nParam1`）⇒ `assert_ne!` 与帧断言红。
    #[test]
    fn script_frame_maps_merchantdlgclose_to_sm_merchantdlgclose() {
        let s = SendMsg {
            ident: RM_SCRIPT_MERCHANTDLGCLOSE,
            w_param: 0,
            n_param1: 4711, // `normNpc.ActorId`
            n_param2: 0,
            n_param3: 0,
            msg: None,
        };
        let got =
            script_frame(&s, 4242, 9001, 0, 0, 0, 0, 0).expect("RM_MERCHANTDLGCLOSE 必须映射");
        assert_eq!(
            got,
            header(4711, 644, 0, 0, 0),
            "Recog = nParam1 = NPC ActorId"
        );
        assert_eq!(got.len(), 12, "`SendDefMessage` 无尾 ⇒ header-only");
        assert_eq!(got, crate::smframe::sm_merchantdlgclose(4711));
        // 排水层的两个 ActorId 都**不是** Recog（只有消息自带的 `nParam1` 才是）。
        assert_ne!(got, header(4242, 644, 0, 0, 0));
        assert_ne!(got, header(9001, 644, 0, 0, 0));
    }

    /// **`RM_ADJUST_BONUS`(8078) 仍按 T-207 登记丢弃**（不硬造分支）：
    /// 泵 `PlayObject.Message.cs:2002-2003` → `SendAdjustBonus()`（`PlayObject.Base.cs:1193-1210`）
    /// 要 `BonusPoint` + 职业 + 三段能力模板，而脚本入队帧 `SendMsg(RM_ADJUST_BONUS,0,0,0,0)`
    /// 四参全 0 ⇒ 帧体**不带**这些输入；部署 `Envir` 的脚本动作 `BONUSPOINT`/`RESTBONUSPOINT`/
    /// `RENEWLEVEL` 各 **0 处** ⇒ 脚本侧不可达。
    ///
    /// 红检：谁给它加支（header-only 811 之类）却没带全 4 项输入 ⇒ 本用例红，逼其登记。
    #[test]
    fn script_frame_adjust_bonus_stays_registered_drop() {
        let s = SendMsg {
            ident: mir2_protocol::messages::RM_ADJUST_BONUS as i32,
            w_param: 0,
            n_param1: 0,
            n_param2: 0,
            n_param3: 0,
            msg: None,
        };
        assert!(
            script_frame(&s, 4242, 9001, 0, 0, 0, 0, 1234).is_none(),
            "RM_ADJUST_BONUS 无脚本可达路径（T-207）⇒ 不得凭空发 811"
        );
    }
}
