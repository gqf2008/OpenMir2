//! 交易（deal / trade）会话状态机 —— M3⑦-实现 3（卡 `openmir2-m3-party-trade-state`）。
//!
//! oracle 是 `PlayObject` 上的**会话内内存状态机**（不落库，盘点 §1.M）：`Dealing` /
//! `DealCreat` / `DealItemList` / `DealGolds` / `DealSuccess` / `DealLastTick`，收发 6 条
//! `CM_DEAL*`、回 `SM_DEAL*`。本仓库把「背包/金币」放在 bins 会话态
//! （[`crate::ground::GroundState`] 的 `item_list`/`gold`），把组队态放在 `crates/world`
//! 的 `Entity` —— 交易态**同时**依赖「对手 `ActorId`」（世界身份）与「双方背包/金币」
//! （bins 会话态），所以本模块放在 bins，与 [`crate::ground`]/[`crate::bag`] 同形：
//! **纯函数 + 事件**，不碰 I/O、不碰 `HashMap`、不碰时间源（`now` 由宿主注入，见 §4.2.2
//! 单线程确定性约束）。
//!
//! # oracle 锚点（行号为在本仓库 `src/` 实测）
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`DealState`] 字段 | `PlayObject.cs:114/118/122/126/130/203`（`DealCreat`/`Dealing`/`DealLastTick`/`DealGolds`/`DealSuccess`/`DealItemList`），构造 `:876-877` |
//! | [`DealState::allow_deal`] / [`DealState::is_can_deal`] | `PlayObject.cs:90`（`AllowDeal`）/`:739`（`IsCanDeal`），构造 `:954/1008` 均 `true` |
//! | [`DealConfig`] | `crates/data/src/party.rs`（⑦-1 配置）+ `Server.conf [Setup]` |
//! | [`deal_try`] | `PlayObject.Operate.cs:911-962`（`ClientDealTry`）+ `PlayObject.Base.cs:2405-2416`（`OpenDealDlg`） |
//! | [`deal_add_item`] | `PlayObject.Operate.cs:964-999`（`ClientAddDealItem`）+ `Base.cs:2383-2403`（`SendAddDealItem`） |
//! | [`deal_del_item`] | `PlayObject.Operate.cs:1001-1042`（`ClientDelDealItem`）+ `Base.cs:2357-2381`（`SendDelDealItem`） |
//! | [`deal_change_gold`] | `PlayObject.Operate.cs:1049-1083`（`ClientChangeDealGold`） |
//! | [`deal_end`] | `PlayObject.Operate.cs:1085-1203`（`ClientDealEnd`：时间门 / 双向锁定 / 容量四检 / 结算） |
//! | [`deal_cancel`] | `PlayObject.Base.cs:730-746`（`DealCancel`，递归）+ `:833-845`（`GetBackDealItems`） |
//! | [`deal_cancel_a`] | `PlayObject.Base.cs:748-752`（`DealCancelA`，`Abil.HP = WAbil.HP` + `DealCancel`） |
//!
//! # 帧面口径（立即 / 排队两桶）
//!
//! 与 ⑦-2（[`crate::party`] 世界侧的同名约定）一致：`SendDefMessage` / `SendSocket` 是
//! **立即**帧（源码调用序），`SendMsg` / `SendUpdateMsg` 入 `MsgQueue`（优先级 = `(byte)wIdent`），
//! 在**同一** `Run()` 内按优先级**升序**出队。⇒ 同一目标的线上序 = 立即帧（源码序）→ 排队帧
//! （优先级升序，同优先级 FIFO）。`SendUpdateMsg` 先移除队列里同 `wIdent` 的旧条目再入队 ⇒
//! [`Emitter::later_update`] 的「同 key 覆盖」语义（`BaseObject.Message.cs:283-306`）。
//!
//! 排队帧优先级字节：`RM_MENU_OK(10309)⇒69` / `RM_WEIGHTCHANGED(10115)⇒131` /
//! `RM_GOLDCHANGED(10136)⇒152` / `RM_SYSMESSAGE(11004)⇒252`。
//!
//! 出队映射：`RM_MENU_OK ⇒ SM_MENU_OK(767)`（`PlayObject.Message.cs:1952-1954`）、
//! `RM_SYSMESSAGE ⇒ SM_SYSMESSAGE(100)`（`:1715-1718`）、`RM_WEIGHTCHANGED ⇒ SM_WEIGHTCHANGED(622)`
//! （`:1760`）、`RM_GOLDCHANGED ⇒ SM_GOLDCHANGED(653)`（`:1834`）。
//!
//! # 连 oracle 缺陷一起搬（§1 硬约束③）
//!
//! - `ClientChangeDealGold` 的「`DealGolds>0 && CanNotGetBackDeal`」守卫回的是
//!   **`SM_DEALDELITEM_FAIL(678)`**（`Operate.cs:1055`），不是 `SM_DEALCHGGOLD_FAIL` —— 照搬。
//! - `ClientChangeDealGold` 的 `DealCreat==null || GetPoseCreate()!=DealCreat` 时不发
//!   「请稍候」类提示，直接落到 `bo09=false` 的 `SM_DEALCHGGOLD_FAIL` —— 照搬。
//!
//! # 未建模（登记，不假造）
//!
//! - `SendAddItem` / `SendAddDealItem` 的 `ClientItem` 尾里的 `GetUpgradeStdItem` desc 升级差量
//!   与 `CustomItemSystem.GetItemName` 自定义名：与 `T-24` 同源（`SM_ADDITEM` 展示层缺口），
//!   本模块对 `Desc[13]==0`（非自定义名）的物品逐字节一致，其余登记。
//! - `DealCancelA` 的 `Abil.HP = WAbil.HP`（`Base.cs:750`）：本仓 `Abil.HP`（基础能力）从不参与
//!   任何可观测路径（`WAbil.HP` 才是真 HP，落在 `crates/world` 的 `Entity::hp`）⇒ 该赋值是
//!   **无可观测差异**；[`deal_cancel_a`] 走与 [`deal_cancel`] 相同的取消路径。

use crate::ability::{Ability, ItemCatalog};
use crate::bag::{item_display_name, BagItem, MAX_BAG_ITEM};
use crate::downlink::gbk;
use crate::itemwire;
use crate::smframe;
use mir2_protocol::{edcode, messages};

/// `Grobal2.MaxBagItem`（`Grobal2.cs:62`）——`ClientDealEnd` 容量四检的背包上限。
pub const DEAL_MAX_BAG_ITEM: usize = MAX_BAG_ITEM;
/// oracle `DealItemList.Count < 12`（`Operate.cs:982`）的硬上限。
pub const DEAL_ITEM_LIST_MAX: usize = 12;

/// `MessageSettings.DisableDealItemsMsg`（`MessageSettings.cs:133`）。
pub const DEAL_DISABLE_MSG: &str = "交易功能暂时关闭!!!";
/// `MessageSettings.PleaseTryDealLaterMsg`（`:131`）。
pub const DEAL_TRY_LATER_MSG: &str = "请稍候再交易!!!";
/// `MessageSettings.CanotTryDealMsg`（`:130`）。
pub const DEAL_CANNOT_TRY_MSG: &str = "当前无法进行此操作!!!";
/// `MessageSettings.DealItemsDenyGetBackMsg`（`:132`）。
pub const DEAL_DENY_GETBACK_MSG: &str = "交易的金币或物品不可以取回，要取回请取消再重新交易!!!";
/// `MessageSettings.DealActionCancelMsg`（`:134`）。
pub const DEAL_CANCEL_MSG: &str = "交易取消!!!";
/// `MessageSettings.PoseDisableDealMsg`（`:135`）。
pub const DEAL_POSE_DISABLE_MSG: &str = "对方禁止进入交易";
/// `MessageSettings.DealSuccessMsg`（`:136`）。
pub const DEAL_SUCCESS_MSG: &str = "交易成功...";
/// `MessageSettings.DealOKTooFast`（`:137`）。
pub const DEAL_OK_TOO_FAST_MSG: &str = "过早按了成交按钮。";
/// `MessageSettings.YourBagSizeTooSmall`（`:138`）。
pub const DEAL_MY_BAG_SMALL_MSG: &str = "你的背包空间不够，无法装下对方交易给你的物品!!!";
/// `MessageSettings.DealHumanBagSizeTooSmall`（`:139`）。
pub const DEAL_HUMAN_BAG_SMALL_MSG: &str = "交易对方的背包空间不够，无法装下对方交易给你的物品!!!";
/// `MessageSettings.YourGoldLargeThenLimit`（`:140`）。
pub const DEAL_MY_GOLD_LIMIT_MSG: &str = "你的所带的金币太多，无法装下对方交易给你的金币!!!";
/// `MessageSettings.DealHumanGoldLargeThenLimit`（`:141`）。
pub const DEAL_HUMAN_GOLD_LIMIT_MSG: &str =
    "交易对方的所带的金币太多，无法装下对方交易给你的金币!!!";
/// `MessageSettings.YouDealOKMsg`（`:142`）。
pub const DEAL_YOU_OK_MSG: &str = "你已经确认交易了。";
/// `MessageSettings.PoseDealOKMsg`（`:143`）。
pub const DEAL_POSE_OK_MSG: &str = "对方已经确认交易了。";
/// `MessageSettings.OpenedDealMsg`（`:196`）——**无前导空格**，调用方拼 `ChrName + 本串`。
pub const DEAL_OPENED_MSG: &str = "开始交易。";

/// `RM_MENU_OK(10309)` 的队列优先级字节：`10309 & 0xFF = 69`。
const PRIO_RM_MENU_OK: u8 = (10_309 & 0xFF) as u8;
/// `RM_WEIGHTCHANGED(10115)` 的队列优先级字节：`131`。
const PRIO_RM_WEIGHTCHANGED: u8 = (10_115 & 0xFF) as u8;
/// `RM_GOLDCHANGED(10136)` 的队列优先级字节：`152`。
const PRIO_RM_GOLDCHANGED: u8 = (10_136 & 0xFF) as u8;
/// `RM_SYSMESSAGE(11004)` 的队列优先级字节：`252`。
const PRIO_RM_SYSMESSAGE: u8 = (11_004 & 0xFF) as u8;

/// `SendUpdateMsg` 的去重 key（同 ident 覆盖）。
const KEY_RM_WEIGHTCHANGED: u16 = messages::RM_WEIGHTCHANGED;
const KEY_RM_GOLDCHANGED: u16 = messages::RM_GOLDCHANGED;

/// 交易相关的运行时配置（host 从 `crates/data` 的 `PartyConfig` + `Server.conf` 取）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DealConfig {
    /// `[Setup] TryDealTime`（`ServerConf.cs:308`，缺省 `3000`）。
    pub try_deal_time: i32,
    /// `[Setup] DealOKTime`（`:309`，缺省 `1000`）。
    pub deal_ok_time: i32,
    /// `[Setup] CanNotGetBackDeal`（`:310`，现网 `=1` ⇒ `true`）。
    pub can_not_get_back_deal: bool,
    /// `[Setup] DisableDeal`（`:311`，现网 `=0` ⇒ `false`）。
    pub disable_deal: bool,
    /// `[Setup] HumanMaxGold`（`GameSvrConf.HumanMaxGold`；`PlayObject.cs:1037` 的 `GoldMax`）。
    pub human_max_gold: i32,
    /// `Grobal2.MaxBagItem`（容量四检用；缺省 46）。
    pub max_bag_item: usize,
    /// `DealItemList.Count < 12` 的上限（缺省 12）。
    pub deal_item_max: usize,
    /// `[Setup] GreenMsgFColor`（`SysMsg(MsgColor.Green)` 的 `fColor`）。
    pub green_f_color: u8,
    /// `[Setup] GreenMsgBColor`。
    pub green_b_color: u8,
    /// `[Setup] RedMsgFColor`。
    pub red_f_color: u8,
    /// `[Setup] RedMsgBColor`。
    pub red_b_color: u8,
}

impl Default for DealConfig {
    fn default() -> Self {
        DealConfig {
            try_deal_time: 3_000,
            deal_ok_time: 1_000,
            can_not_get_back_deal: true,
            disable_deal: false,
            human_max_gold: 5_000_000,
            max_bag_item: DEAL_MAX_BAG_ITEM,
            deal_item_max: DEAL_ITEM_LIST_MAX,
            green_f_color: 0x01,
            green_b_color: 0xFF,
            red_f_color: 0xFF,
            red_b_color: 0x38,
        }
    }
}

/// 一个在线角色的交易会话态（oracle `PlayObject` 上的散字段；不落库）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DealState {
    /// `PlayObject.DealCreat`（`:114`，对手 `ActorId`；`None` = 无对手）。
    pub deal_creature: Option<i32>,
    /// `PlayObject.Dealing`（`:118`）。
    pub dealing: bool,
    /// `PlayObject.DealLastTick`（`:122`；`HUtil32.GetTickCount()` 的毫秒值）。
    pub deal_last_tick: i64,
    /// `PlayObject.DealGolds`（`:126`，构造 `:876` = 0）。
    pub deal_golds: i32,
    /// `PlayObject.DealSuccess`（`:130`，本侧是否已「锁定/确认」）。
    pub deal_success: bool,
    /// `PlayObject.DealItemList`（`:203`，构造 `:877` = `new List`；顺序 = 放入序）。
    pub deal_item_list: Vec<BagItem>,
    /// `PlayObject.AllowDeal`（`:90`，构造 `:1008` = `true`）。
    pub allow_deal: bool,
    /// `PlayObject.IsCanDeal`（`:739`，构造 `:954` = `true`）。
    pub is_can_deal: bool,
}

impl Default for DealState {
    fn default() -> Self {
        DealState {
            deal_creature: None,
            dealing: false,
            deal_last_tick: 0,
            deal_golds: 0,
            deal_success: false,
            deal_item_list: Vec::new(),
            allow_deal: true,
            is_can_deal: true,
        }
    }
}

/// 一次交易操作产生的**帧面事件**（顺序 = 立即桶 ++ 排队桶，见模块头）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DealEvent {
    /// `SendDefMessage(ident, recog, param, tag, series[, name])`（立即）。
    Def {
        /// 收件人 `ActorId`。
        target: i32,
        ident: u16,
        recog: i32,
        param: u16,
        tag: u16,
        series: u16,
        /// `Some(name)` ⇒ 尾 = `EncodeString(name)`（`SM_DEALMENU`）。
        name: Option<String>,
    },
    /// `SendMsg(RM_SYSMESSAGE)`（排队 252）⇒ `SM_SYSMESSAGE`。
    SysMsg {
        target: i32,
        recog: i32,
        f_color: u8,
        b_color: u8,
        text: String,
    },
    /// `SendMsg(RM_MENU_OK)`（排队 69）⇒ `SM_MENU_OK`（尾 = 文本）。
    MenuOk {
        target: i32,
        recog: i32,
        text: String,
    },
    /// `SendAddItem(userItem)`（立即）：`SM_ADDITEM(200)` + `EncodeBuffer(ClientItem)`，`Series=1`。
    AddItem { target: i32, item: BagItem },
    /// `SendSocket(SM_DEALREMOTEADDITEM/REMOTEDELITEM + EncodeBuffer(ClientItem))`（立即）。
    RemoteItem {
        target: i32,
        recog: i32,
        ident: u16,
        item: BagItem,
    },
    /// `WeightChanged()` ⇒ `SendUpdateMsg(RM_WEIGHTCHANGED)`（排队 131，同 ident 覆盖）。
    WeightChanged { target: i32 },
    /// `GoldChanged()` ⇒ `SendUpdateMsg(RM_GOLDCHANGED)`（排队 152，同 ident 覆盖）。
    GoldChanged { target: i32 },
}

/// 「立即帧 + 排队帧」两桶（见模块头）。`finish` = 立即（插入序）++ 排队（优先级升序，稳定）。
/// 排队帧条目（优先级 + 可选去重 key + 事件）。
struct QueuedEvent {
    priority: u8,
    key: Option<(i32, u16)>,
    ev: DealEvent,
}

#[derive(Default)]
struct Emitter {
    immediate: Vec<DealEvent>,
    queued: Vec<QueuedEvent>,
}

impl Emitter {
    fn new() -> Self {
        Emitter::default()
    }

    /// `SendDefMessage` / `SendSocket`：立即帧（保持源码调用序）。
    fn now(&mut self, ev: DealEvent) {
        self.immediate.push(ev);
    }

    /// `SendMsg`：入队（同优先级 FIFO）。
    fn later(&mut self, priority: u8, ev: DealEvent) {
        self.queued.push(QueuedEvent {
            priority,
            key: None,
            ev,
        });
    }

    /// `SendUpdateMsg`：先移除**同一目标**上同 `ident` 的旧条目，再入队
    /// （每个 `PlayObject` 有独立 `MsgQueue`，`BaseObject.Message.cs:283-306`）。
    fn later_update(&mut self, priority: u8, target: i32, key: u16, ev: DealEvent) {
        self.queued
            .retain(|q| !(q.priority == priority && q.key == Some((target, key))));
        self.queued.push(QueuedEvent {
            priority,
            key: Some((target, key)),
            ev,
        });
    }

    fn finish(mut self) -> Vec<DealEvent> {
        // `sort_by_key` 稳定 ⇒ 同优先级保持入队序。
        self.queued.sort_by_key(|q| q.priority);
        let mut out = self.immediate;
        out.extend(self.queued.into_iter().map(|q| q.ev));
        out
    }
}

/// 交易操作作用在**一侧**上的可变状态（背包/金币/交易态 + 身份/上限）。
///
/// `name`/`gold_max` 是只读快照；`items`/`gold`/`state` 是宿主会话态的借用。
pub struct DealSide<'a> {
    /// `BaseObject.ActorId`。
    pub id: i32,
    /// `BaseObject.ChrName`。
    pub name: String,
    /// `PlayObject.ItemList`（顺序敏感）。
    pub items: &'a mut Vec<BagItem>,
    /// `BaseObject.Gold`。
    pub gold: &'a mut i32,
    /// `PlayObject.GoldMax`（= `Config.HumanMaxGold`）。
    pub gold_max: i32,
    /// 交易会话态。
    pub state: &'a mut DealState,
}

impl DealSide<'_> {
    /// `GetItemName` 忽略大小写比较（`string.Compare(..., OrdinalIgnoreCase) == 0`）。
    fn name_matches(
        items: &[BagItem],
        catalog: &dyn ItemCatalog,
        idx: i32,
        want: &str,
    ) -> Option<usize> {
        for (i, it) in items.iter().enumerate() {
            if it.make_index == idx {
                let got = item_display_name(catalog, it);
                if got.to_lowercase() == want.to_lowercase() {
                    return Some(i);
                }
            }
        }
        None
    }
}

/// `HUtil32.GetValidStr3(ref sItemName, ' ')`（`HUtil32.cs:384-389`）：
/// `Split(' ', 2, RemoveEmptyEntries)` 取第一段（调用方只在含空格时走它）。
#[must_use]
pub fn first_token(s: &str) -> &str {
    if !s.contains(' ') {
        return s;
    }
    s.split(' ').find(|p| !p.is_empty()).unwrap_or("")
}

/// `OpenDealDlg(baseObject)`（`BaseObject` 侧，`PlayObject.Base.cs:2405-2416`），在 `side` 上。
fn open_deal_dlg(
    side: &mut DealSide<'_>,
    other_id: i32,
    other_name: &str,
    cfg: &DealConfig,
    now: i64,
    em: &mut Emitter,
) {
    side.state.deal_creature = Some(other_id);
    side.state.dealing = true;
    get_back_deal_items(side);
    em.now(DealEvent::Def {
        target: side.id,
        ident: messages::SM_DEALMENU,
        recog: 0,
        param: 0,
        tag: 0,
        series: 0,
        name: Some(other_name.to_string()),
    });
    side.state.deal_last_tick = now;
    let _ = cfg;
}

/// `GetBackDealItems()`（`PlayObject.Base.cs:833-845`）：交易栏物品退回 `ItemList`，
/// `Gold += DealGolds`，清 `DealGolds`/`DealSuccess`。
fn get_back_deal_items(side: &mut DealSide<'_>) {
    for it in side.state.deal_item_list.drain(..) {
        side.items.push(it);
    }
    *side.gold += side.state.deal_golds;
    side.state.deal_golds = 0;
    side.state.deal_success = false;
}

/// `SysMsg(text, Green, Hint)`（`BaseObject.cs:1648-1655`）⇒ 排队 `RM_SYSMESSAGE`。
fn green_msg(em: &mut Emitter, target: i32, cfg: &DealConfig, text: &str) {
    em.later(
        PRIO_RM_SYSMESSAGE,
        DealEvent::SysMsg {
            target,
            recog: target,
            f_color: cfg.green_f_color,
            b_color: cfg.green_b_color,
            text: text.to_string(),
        },
    );
}

/// `SysMsg(text, Red, Hint)`。
fn red_msg(em: &mut Emitter, target: i32, cfg: &DealConfig, text: &str) {
    em.later(
        PRIO_RM_SYSMESSAGE,
        DealEvent::SysMsg {
            target,
            recog: target,
            f_color: cfg.red_f_color,
            b_color: cfg.red_b_color,
            text: text.to_string(),
        },
    );
}

/// 立即的无尾 `SendDefMessage(ident, 0, 0, 0, 0)`。
fn def0(target: i32, ident: u16) -> DealEvent {
    DealEvent::Def {
        target,
        ident,
        recog: 0,
        param: 0,
        tag: 0,
        series: 0,
        name: None,
    }
}

/// `SendDefMessage(ident, recog, param, tag, 0)`。
#[allow(clippy::too_many_arguments)]
fn defn(target: i32, ident: u16, recog: i32, param: u16, tag: u16, series: u16) -> DealEvent {
    DealEvent::Def {
        target,
        ident,
        recog,
        param,
        tag,
        series,
        name: None,
    }
}

/// `ClientDealTry(sHumName)`（`PlayObject.Operate.cs:911-962`）+ 成功时的 `OpenDealDlg`。
///
/// `target` = 宿主解析出的 `GetPoseCreate()` 结果（`Absent` = 无前排/前排为自身；`NotPlayer` = 前排不是玩家）；
/// 前置守卫（`DisableDeal`/`Dealing`/`TryDealTime`/`IsCanDeal`）**先于**目标判定，与 oracle 同序
/// （`Operate.cs:913-926` 四条守卫在 `:928` 的 `GetPoseCreate()` 之前）。
pub fn deal_try(
    me: &mut DealSide<'_>,
    other: &mut DealSide<'_>,
    cfg: &DealConfig,
    now: i64,
    target: DealTryTarget,
) -> Vec<DealEvent> {
    let mut em = Emitter::new();
    if cfg.disable_deal {
        em.later(
            PRIO_RM_MENU_OK,
            DealEvent::MenuOk {
                target: me.id,
                recog: me.id,
                text: DEAL_DISABLE_MSG.to_string(),
            },
        );
        return em.finish();
    }
    if me.state.dealing {
        return em.finish();
    }
    if now - me.state.deal_last_tick < i64::from(cfg.try_deal_time) {
        em.later(
            PRIO_RM_MENU_OK,
            DealEvent::MenuOk {
                target: me.id,
                recog: me.id,
                text: DEAL_TRY_LATER_MSG.to_string(),
            },
        );
        return em.finish();
    }
    if !me.state.is_can_deal {
        em.later(
            PRIO_RM_MENU_OK,
            DealEvent::MenuOk {
                target: me.id,
                recog: me.id,
                text: DEAL_CANNOT_TRY_MSG.to_string(),
            },
        );
        return em.finish();
    }
    match target {
        // oracle `Operate.cs:929` 的 `poseObject.Race`：
        // ① `GetPoseCreate() == null` ⇒ 对 null 解引用 ⇒ NRE，被
        //    `WorldServer.ProcessPlayObjectData` 的 `catch`（`WorldServer.cs:734`）吞掉 ⇒ 一个帧都不发；
        // ② `poseObject == this` ⇒ `:934` 的内层 `if (targetPlayObject != this)` 无 `else` ⇒ 也不发帧。
        // 两态在 oracle 里都「静默」，Rust 不得发 `SM_DEALTRY_FAIL`（那不是 oracle 行为）。
        DealTryTarget::Absent => {}
        DealTryTarget::NotPlayer => em.now(def0(me.id, messages::SM_DEALTRY_FAIL)),
        DealTryTarget::Other { faces_me } => {
            if faces_me && !other.state.dealing {
                if other.state.allow_deal && other.state.is_can_deal {
                    // `targetPlayObject.SysMsg(ChrName + OpenedDealMsg)` → `SysMsg(targetPlayObject.ChrName + …)`
                    let other_text = format!("{}{}", me.name, DEAL_OPENED_MSG);
                    green_msg(&mut em, other.id, cfg, &other_text);
                    let me_text = format!("{}{}", other.name, DEAL_OPENED_MSG);
                    green_msg(&mut em, me.id, cfg, &me_text);
                    // `this.OpenDealDlg(target)` → `target.OpenDealDlg(this)`。
                    open_deal_dlg(me, other.id, &other.name, cfg, now, &mut em);
                    open_deal_dlg(other, me.id, &me.name, cfg, now, &mut em);
                } else {
                    red_msg(&mut em, me.id, cfg, DEAL_POSE_DISABLE_MSG);
                }
            } else {
                em.now(def0(me.id, messages::SM_DEALTRY_FAIL));
            }
        }
    }
    em.finish()
}

/// `ClientDealTry` 的 `GetPoseCreate()` 分辨率（宿主用世界/展示表算出后传入 [`deal_try`]）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DealTryTarget {
    /// 无前排对象（`GetPoseCreate() == null`）或前排为自身（`poseObject == this`）。
    ///
    /// oracle 两态都**不发任何帧**：前者在 `Operate.cs:929` 的 `poseObject.Race` 处 NRE
    /// （被 `WorldServer.cs:734` 的 `catch` 吞），后者走 `:934` 内层 `if` 的无 `else` 静默支。
    Absent,
    /// 前排不是玩家（`Operate.cs:955-958` 的 `else` ⇒ `SM_DEALTRY_FAIL`）。
    NotPlayer,
    /// 前排是另一个玩家；`faces_me` = 对方 `GetPoseCreate() == this`。
    Other { faces_me: bool },
}

/// `SendAddDealItem(userItem)`（`PlayObject.Base.cs:2383-2403`）：先给自己 `SM_DEALADDITEM_OK`，
/// 模板存在时给对手 `SM_DEALREMOTEADDITEM`（`ActorId` = 自己），双方刷 `DealLastTick`。
fn send_add_deal_item(
    me: &mut DealSide<'_>,
    other: &mut DealSide<'_>,
    catalog: &dyn ItemCatalog,
    item: &BagItem,
    now: i64,
    em: &mut Emitter,
) {
    em.now(def0(me.id, messages::SM_DEALADDITEM_OK));
    if me.state.deal_creature.is_some() && catalog.get(item.index).is_some() {
        em.now(DealEvent::RemoteItem {
            target: other.id,
            recog: me.id,
            ident: messages::SM_DEALREMOTEADDITEM,
            item: item.clone(),
        });
        other.state.deal_last_tick = now;
        me.state.deal_last_tick = now;
    }
}

/// `ClientAddDealItem(nItemIdx, sItemName)`（`PlayObject.Operate.cs:964-999`）。
pub fn deal_add_item(
    me: &mut DealSide<'_>,
    other: &mut DealSide<'_>,
    cfg: &DealConfig,
    catalog: &dyn ItemCatalog,
    idx: i32,
    raw_name: &str,
    now: i64,
) -> Vec<DealEvent> {
    let mut em = Emitter::new();
    if me.state.deal_creature.is_none() || !me.state.dealing {
        return em.finish();
    }
    let want = first_token(raw_name).to_string();
    let mut success = false;
    if !other.state.deal_success {
        if let Some(i) = DealSide::name_matches(me.items, catalog, idx, &want) {
            if me.state.deal_item_list.len() < cfg.deal_item_max {
                let item = me.items.remove(i);
                me.state.deal_item_list.push(item.clone());
                send_add_deal_item(me, other, catalog, &item, now, &mut em);
                success = true;
            }
        }
    }
    if !success {
        em.now(def0(me.id, messages::SM_DEALADDITEM_FAIL));
    }
    em.finish()
}

/// `SendDelDealItem(userItem)`（`PlayObject.Base.cs:2357-2381`）。
fn send_del_deal_item(
    me: &mut DealSide<'_>,
    other: &mut DealSide<'_>,
    catalog: &dyn ItemCatalog,
    item: &BagItem,
    now: i64,
    em: &mut Emitter,
) {
    if me.state.deal_creature.is_some() {
        if catalog.get(item.index).is_some() {
            em.now(DealEvent::RemoteItem {
                target: other.id,
                recog: me.id,
                ident: messages::SM_DEALREMOTEDELITEM,
                item: item.clone(),
            });
            other.state.deal_last_tick = now;
            me.state.deal_last_tick = now;
        }
        em.now(def0(me.id, messages::SM_DEALDELITEM_OK));
    } else {
        em.now(def0(me.id, messages::SM_DEALDELITEM_FAIL));
    }
}

/// `ClientDelDealItem(nItemIdx, sItemName)`（`PlayObject.Operate.cs:1001-1042`）。
///
/// `CanNotGetBackDeal` 在**最前**（先于 `Dealing` 门）：照 oracle 直接回
/// `RM_MENU_OK` + `SM_DEALDELITEM_FAIL`。
pub fn deal_del_item(
    me: &mut DealSide<'_>,
    other: &mut DealSide<'_>,
    cfg: &DealConfig,
    catalog: &dyn ItemCatalog,
    idx: i32,
    raw_name: &str,
    now: i64,
) -> Vec<DealEvent> {
    let mut em = Emitter::new();
    if cfg.can_not_get_back_deal {
        em.later(
            PRIO_RM_MENU_OK,
            DealEvent::MenuOk {
                target: me.id,
                recog: me.id,
                text: DEAL_DENY_GETBACK_MSG.to_string(),
            },
        );
        em.now(def0(me.id, messages::SM_DEALDELITEM_FAIL));
        return em.finish();
    }
    if me.state.deal_creature.is_none() || !me.state.dealing {
        return em.finish();
    }
    let want = first_token(raw_name).to_string();
    let mut ok = false;
    if !other.state.deal_success {
        let pos = DealSide::name_matches(&me.state.deal_item_list, catalog, idx, &want);
        if let Some(i) = pos {
            let item = me.state.deal_item_list[i].clone();
            me.items.push(item.clone());
            send_del_deal_item(me, other, catalog, &item, now, &mut em);
            me.state.deal_item_list.remove(i);
            ok = true;
        }
    }
    if !ok {
        em.now(def0(me.id, messages::SM_DEALDELITEM_FAIL));
    }
    em.finish()
}

/// `ClientChangeDealGold(nGold)`（`PlayObject.Operate.cs:1049-1083`）。
///
/// `other_is_pose` = `GetPoseCreate() == DealCreat`。`CanNotGetBackDeal` 且已放金币时回
/// **`SM_DEALDELITEM_FAIL`**（oracle 原样，见模块头）。
pub fn deal_change_gold(
    me: &mut DealSide<'_>,
    other: &mut DealSide<'_>,
    cfg: &DealConfig,
    n_gold: i32,
    now: i64,
    other_is_pose: bool,
) -> Vec<DealEvent> {
    let mut em = Emitter::new();
    if me.state.deal_golds > 0 && cfg.can_not_get_back_deal {
        em.later(
            PRIO_RM_MENU_OK,
            DealEvent::MenuOk {
                target: me.id,
                recog: me.id,
                text: DEAL_DENY_GETBACK_MSG.to_string(),
            },
        );
        em.now(def0(me.id, messages::SM_DEALDELITEM_FAIL));
        return em.finish();
    }
    if n_gold < 0 {
        em.now(defn(
            me.id,
            messages::SM_DEALCHGGOLD_FAIL,
            me.state.deal_golds,
            smframe::lo_word(*me.gold),
            smframe::hi_word(*me.gold),
            0,
        ));
        return em.finish();
    }
    let mut ok = false;
    if me.state.deal_creature.is_some()
        && other_is_pose
        && !other.state.deal_success
        && *me.gold + me.state.deal_golds >= n_gold
    {
        *me.gold = *me.gold + me.state.deal_golds - n_gold;
        me.state.deal_golds = n_gold;
        em.now(defn(
            me.id,
            messages::SM_DEALCHGGOLD_OK,
            me.state.deal_golds,
            smframe::lo_word(*me.gold),
            smframe::hi_word(*me.gold),
            0,
        ));
        em.now(defn(
            other.id,
            messages::SM_DEALREMOTECHGGOLD,
            me.state.deal_golds,
            0,
            0,
            0,
        ));
        other.state.deal_last_tick = now;
        ok = true;
        me.state.deal_last_tick = now;
    }
    if !ok {
        em.now(defn(
            me.id,
            messages::SM_DEALCHGGOLD_FAIL,
            me.state.deal_golds,
            smframe::lo_word(*me.gold),
            smframe::hi_word(*me.gold),
            0,
        ));
    }
    em.finish()
}

/// `DealCancel()`（`PlayObject.Base.cs:730-746`）的递归体。
fn deal_cancel_em(
    me: &mut DealSide<'_>,
    other: &mut DealSide<'_>,
    cfg: &DealConfig,
    now: i64,
    em: &mut Emitter,
) {
    if !me.state.dealing {
        return;
    }
    me.state.dealing = false;
    em.now(def0(me.id, messages::SM_DEALCANCEL));
    if me.state.deal_creature.is_some() {
        // `DealCreat.DealCancel()`：对手的 `Dealing` 已被置 false ⇒ 递归立刻返回。
        deal_cancel_em(other, me, cfg, now, em);
    }
    me.state.deal_creature = None;
    get_back_deal_items(me);
    green_msg(em, me.id, cfg, DEAL_CANCEL_MSG);
    me.state.deal_last_tick = now;
}

/// `ClientCancelDeal()`（`Operate.cs:1044-1047`）⇒ `DealCancel()`。
pub fn deal_cancel(
    me: &mut DealSide<'_>,
    other: &mut DealSide<'_>,
    cfg: &DealConfig,
    now: i64,
) -> Vec<DealEvent> {
    let mut em = Emitter::new();
    deal_cancel_em(me, other, cfg, now, &mut em);
    em.finish()
}

/// `DealCancelA()`（`PlayObject.Base.cs:748-752`）：`Abil.HP = WAbil.HP` + `DealCancel()`。
///
/// `Abil.HP` 在本仓不参与可观测路径（见模块头）⇒ 与 [`deal_cancel`] 同路径。
pub fn deal_cancel_a(
    me: &mut DealSide<'_>,
    other: &mut DealSide<'_>,
    cfg: &DealConfig,
    now: i64,
) -> Vec<DealEvent> {
    deal_cancel(me, other, cfg, now)
}

/// `ClientDealEnd()`（`PlayObject.Operate.cs:1085-1203`）。
pub fn deal_end(
    me: &mut DealSide<'_>,
    other: &mut DealSide<'_>,
    cfg: &DealConfig,
    now: i64,
) -> Vec<DealEvent> {
    let mut em = Emitter::new();
    me.state.deal_success = true;
    if me.state.deal_creature.is_none() {
        return em.finish();
    }
    if now - me.state.deal_last_tick < i64::from(cfg.deal_ok_time)
        || now - other.state.deal_last_tick < i64::from(cfg.deal_ok_time)
    {
        red_msg(&mut em, me.id, cfg, DEAL_OK_TOO_FAST_MSG);
        deal_cancel_em(me, other, cfg, now, &mut em);
        return em.finish();
    }
    if other.state.deal_success {
        // 容量四检：**全部评估**（不短路），各自回 SysMsg(Red)，任一不满足 ⇒ `bo11=false`。
        let mut ok = true;
        if cfg.max_bag_item.saturating_sub(me.items.len()) < other.state.deal_item_list.len() {
            red_msg(&mut em, me.id, cfg, DEAL_MY_BAG_SMALL_MSG);
            ok = false;
        }
        if me.gold_max - *me.gold < other.state.deal_golds {
            red_msg(&mut em, me.id, cfg, DEAL_MY_GOLD_LIMIT_MSG);
            ok = false;
        }
        if cfg.max_bag_item.saturating_sub(other.items.len()) < me.state.deal_item_list.len() {
            red_msg(&mut em, me.id, cfg, DEAL_HUMAN_BAG_SMALL_MSG);
            ok = false;
        }
        if other.gold_max - *other.gold < me.state.deal_golds {
            red_msg(&mut em, me.id, cfg, DEAL_HUMAN_GOLD_LIMIT_MSG);
            ok = false;
        }
        if ok {
            // 我的交易栏物品 → 对方背包（顺序 = 放入序）。
            for item in std::mem::take(&mut me.state.deal_item_list) {
                other.items.push(item.clone());
                em.later_update(
                    PRIO_RM_WEIGHTCHANGED,
                    other.id,
                    KEY_RM_WEIGHTCHANGED,
                    DealEvent::WeightChanged { target: other.id },
                );
                em.now(DealEvent::AddItem {
                    target: other.id,
                    item,
                });
            }
            if me.state.deal_golds > 0 {
                *other.gold += me.state.deal_golds;
                em.later_update(
                    PRIO_RM_GOLDCHANGED,
                    other.id,
                    KEY_RM_GOLDCHANGED,
                    DealEvent::GoldChanged { target: other.id },
                );
            }
            // 对方交易栏物品 → 我的背包。
            for item in std::mem::take(&mut other.state.deal_item_list) {
                me.items.push(item.clone());
                em.later_update(
                    PRIO_RM_WEIGHTCHANGED,
                    me.id,
                    KEY_RM_WEIGHTCHANGED,
                    DealEvent::WeightChanged { target: me.id },
                );
                em.now(DealEvent::AddItem {
                    target: me.id,
                    item,
                });
            }
            if other.state.deal_golds > 0 {
                *me.gold += other.state.deal_golds;
                em.later_update(
                    PRIO_RM_GOLDCHANGED,
                    me.id,
                    KEY_RM_GOLDCHANGED,
                    DealEvent::GoldChanged { target: me.id },
                );
            }
            // 先清对手，再清自己。
            em.now(def0(other.id, messages::SM_DEALSUCCESS));
            green_msg(&mut em, other.id, cfg, DEAL_SUCCESS_MSG);
            reset_after_success(other);
            em.now(def0(me.id, messages::SM_DEALSUCCESS));
            green_msg(&mut em, me.id, cfg, DEAL_SUCCESS_MSG);
            reset_after_success(me);
        } else {
            deal_cancel_em(me, other, cfg, now, &mut em);
        }
    } else {
        green_msg(&mut em, me.id, cfg, DEAL_YOU_OK_MSG);
        green_msg(&mut em, other.id, cfg, DEAL_POSE_OK_MSG);
    }
    em.finish()
}

/// `DealCreat.DealCreat = null; Dealing = false; DealItemList.Clear(); DealGolds = 0; DealSuccess = false;`
fn reset_after_success(side: &mut DealSide<'_>) {
    side.state.deal_creature = None;
    side.state.dealing = false;
    side.state.deal_item_list.clear();
    side.state.deal_golds = 0;
    side.state.deal_success = false;
}

/// 一次交易下行要读到的**目标会话态**（`SM_GOLDCHANGED`/`SM_WEIGHTCHANGED` 用当下值）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DealWire {
    /// `BaseObject.ActorId`。
    pub actor_id: i32,
    /// `BaseObject.Gold`。
    pub gold: i32,
    /// `BaseObject.GameGold`。
    pub game_gold: i32,
    /// `BaseObject.WAbil`。
    pub wabil: Ability,
}

/// `SendAddItem(userItem)`（`PlayObject.Base.cs:198-227`）的 `SM_ADDITEM` body：
/// `MakeMessage(SM_ADDITEM, ActorId, 0, 0, 1)` + `EncodeBuffer(clientItem)`。
///
/// `clientItem.Item.Shape` 按 `M2Share.StdModeMap` + `Desc[8]` 置 0/130；
/// `StdMode == 50` 名称加 `" #" + Dura`。`GetUpgradeStdItem` 的 desc 升级差量未建模（同 `T-24`）。
pub(crate) fn send_add_item_body(
    recipient: i32,
    item: &BagItem,
    catalog: &dyn ItemCatalog,
) -> Option<Vec<u8>> {
    let tpl = catalog.get(item.index)?;
    let mut owned = tpl.clone();
    let mut name = item_display_name(catalog, item);
    if owned.std_mode == 50 {
        name = format!("{name} #{}", item.dura);
    }
    if crate::bag::std_mode_in_map(owned.std_mode) {
        owned.shape = if item.desc[8] == 0 { 0 } else { 130 };
    }
    let tpl: &mir2_data::StdItem = &owned;
    // `owned` 在函数内；`ClientItem` 借用它并在本函数内编码 ⇒ 生命周期安全。
    let ci = itemwire::ClientItem {
        tpl,
        name,
        make_index: item.make_index,
        dura: item.dura,
        dura_max: item.dura_max,
        desc: item.desc,
    };
    let tail = edcode::encode(&itemwire::encode_client_item(&ci));
    Some(smframe::gm_data_body(
        smframe::command_header(recipient, messages::SM_ADDITEM, 0, 0, 1),
        &tail,
    ))
}

/// `SendSocket(MakeMessage(SM_DEALREMOTEADDITEM/…, ActorId, 0, 0, 1), EncodeBuffer(clientItem))`。
///
/// `clientItem.Item.Name = CustomItemSystem.GetItemName(userItem)`（**无** StdMode==50 后缀，
/// 与 `SendAddItem` 不同）；`Shape` 取模板原值（oracle 不改）。
fn remote_item_body(
    recog: i32,
    ident: u16,
    item: &BagItem,
    catalog: &dyn ItemCatalog,
) -> Option<Vec<u8>> {
    let tpl = catalog.get(item.index)?;
    let ci = itemwire::ClientItem {
        tpl,
        name: item_display_name(catalog, item),
        make_index: item.make_index,
        dura: item.dura,
        dura_max: item.dura_max,
        desc: item.desc,
    };
    let tail = edcode::encode(&itemwire::encode_client_item(&ci));
    Some(smframe::gm_data_body(
        smframe::command_header(recog, ident, 0, 0, 1),
        &tail,
    ))
}

/// 把一条 [`DealEvent`] 编成 `(收件人 ActorId, GM_DATA body)`（纯函数，无 I/O）。
///
/// `wire_of` 按 `ActorId` 取该目标的**当下** `Gold`/`GameGold`/`WAbil`（`SM_GOLDCHANGED`/
/// `SM_WEIGHTCHANGED` 在出队时读当下值，`PlayObject.Message.cs:1760/1834`）。
#[must_use]
pub fn deal_event_body(
    ev: &DealEvent,
    catalog: &dyn ItemCatalog,
    wire_of: &dyn Fn(i32) -> DealWire,
) -> Option<(i32, Vec<u8>)> {
    match ev {
        DealEvent::Def {
            target,
            ident,
            recog,
            param,
            tag,
            series,
            name,
        } => {
            let tail = name
                .as_ref()
                .map(|n| edcode::encode(&gbk(n)))
                .unwrap_or_default();
            Some((
                *target,
                smframe::gm_data_body(
                    smframe::command_header(*recog, *ident, *param, *tag, *series),
                    &tail,
                ),
            ))
        }
        DealEvent::SysMsg {
            target,
            recog,
            f_color,
            b_color,
            text,
        } => Some((
            *target,
            smframe::sm_sysmessage(*recog, *f_color, *b_color, &gbk(text)),
        )),
        DealEvent::MenuOk {
            target,
            recog,
            text,
        } => Some((*target, smframe::sm_menu_ok(*recog, &gbk(text)))),
        DealEvent::AddItem { target, item } => {
            Some((*target, send_add_item_body(*target, item, catalog)?))
        }
        DealEvent::RemoteItem {
            target,
            recog,
            ident,
            item,
        } => Some((*target, remote_item_body(*recog, *ident, item, catalog)?)),
        DealEvent::WeightChanged { target } => {
            let w = wire_of(*target);
            Some((
                *target,
                smframe::sm_weightchanged(w.wabil.weight, w.wabil.wear_weight, w.wabil.hand_weight),
            ))
        }
        DealEvent::GoldChanged { target } => {
            let w = wire_of(*target);
            Some((*target, smframe::sm_goldchanged(w.gold, w.game_gold)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::StdItemCatalog;
    use mir2_data::StdItem;

    struct P {
        id: i32,
        name: String,
        items: Vec<BagItem>,
        gold: i32,
        gold_max: i32,
        state: DealState,
    }

    impl P {
        fn new(id: i32, name: &str, gold: i32) -> Self {
            P {
                id,
                name: name.to_string(),
                items: Vec::new(),
                gold,
                gold_max: 5_000_000,
                state: DealState::default(),
            }
        }
    }

    fn side(p: &mut P) -> DealSide<'_> {
        DealSide {
            id: p.id,
            name: p.name.clone(),
            items: &mut p.items,
            gold: &mut p.gold,
            gold_max: p.gold_max,
            state: &mut p.state,
        }
    }

    fn catalog() -> StdItemCatalog {
        let mut c = StdItemCatalog::new();
        c.insert(
            1,
            StdItem {
                name: "木剑".into(),
                std_mode: 5,
                shape: 1,
                weight: 3,
                dura_max: 20,
                dc: 3,
                price: 10,
                ..StdItem::default()
            },
        );
        c.insert(
            2,
            StdItem {
                name: "金创药".into(),
                std_mode: 0,
                shape: 0,
                weight: 1,
                dura_max: 0,
                price: 1,
                ..StdItem::default()
            },
        );
        c
    }

    fn bag(index: u16, make_index: i32) -> BagItem {
        BagItem {
            make_index,
            index,
            dura: 7,
            dura_max: 20,
            desc: [0; 14],
        }
    }

    fn names(evs: &[DealEvent]) -> Vec<(i32, u16)> {
        evs.iter()
            .map(|e| match e {
                DealEvent::Def { target, ident, .. } => (*target, *ident),
                DealEvent::SysMsg { target, .. } => (*target, u16::MAX),
                DealEvent::MenuOk { target, .. } => (*target, messages::SM_MENU_OK),
                DealEvent::AddItem { target, .. } => (*target, messages::SM_ADDITEM),
                DealEvent::RemoteItem { target, ident, .. } => (*target, *ident),
                DealEvent::WeightChanged { target } => (*target, messages::SM_WEIGHTCHANGED),
                DealEvent::GoldChanged { target } => (*target, messages::SM_GOLDCHANGED),
            })
            .collect()
    }

    fn open_pair(cfg: &DealConfig) -> (P, P) {
        let mut a = P::new(1, "甲", 1_000);
        let mut b = P::new(2, "乙", 500);
        a.items.push(bag(1, 11));
        b.items.push(bag(2, 22));
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let _ = deal_try(
            &mut sa,
            &mut sb,
            cfg,
            10_000,
            DealTryTarget::Other { faces_me: true },
        );
        drop(sa);
        drop(sb);
        (a, b)
    }

    /// `CM_DEALTRY` 成功：双向 `OpenDealDlg`（各一条 `SM_DEALMENU`，尾 = 对手名），
    /// 两条绿色 `SysMsg`（排队 252）。立即桶 = 两条 `SM_DEALMENU`；排队桶 = 两条 `SysMsg`。
    #[test]
    fn deal_try_opens_both_sides() {
        let cfg = DealConfig::default();
        let (mut a, mut b) = (P::new(1, "甲", 1_000), P::new(2, "乙", 500));
        a.state.deal_last_tick = 0;
        b.state.deal_last_tick = 0;
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_try(
            &mut sa,
            &mut sb,
            &cfg,
            10_000,
            DealTryTarget::Other { faces_me: true },
        );
        assert_eq!(
            names(&evs),
            vec![
                (1, messages::SM_DEALMENU),
                (2, messages::SM_DEALMENU),
                (2, u16::MAX),
                (1, u16::MAX),
            ],
            "立即两条 DEALMENU，随后排队两条 SysMsg"
        );
        assert_eq!(a.state.deal_creature, Some(2));
        assert_eq!(b.state.deal_creature, Some(1));
        assert!(a.state.dealing && b.state.dealing);
        assert_eq!(a.state.deal_last_tick, 10_000);
        // SM_DEALMENU 尾 = EncodeString(对手名)。
        match &evs[0] {
            DealEvent::Def { name, ident, .. } => {
                assert_eq!(*ident, messages::SM_DEALMENU);
                assert_eq!(name.as_deref(), Some("乙"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    /// `TryDealTime` 门（`Operate.cs:922`）：`now - DealLastTick < 3000` ⇒ `RM_MENU_OK`，不开交易。
    #[test]
    fn deal_try_respects_try_deal_time_gate() {
        let cfg = DealConfig::default();
        let mut a = P::new(1, "甲", 1_000);
        let mut b = P::new(2, "乙", 500);
        a.state.deal_last_tick = 9_000;
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_try(
            &mut sa,
            &mut sb,
            &cfg,
            11_000,
            DealTryTarget::Other { faces_me: true },
        );
        assert_eq!(names(&evs), vec![(1, messages::SM_MENU_OK)]);
        assert!(!a.state.dealing && !b.state.dealing, "未开交易");
    }

    /// `DisableDeal` ⇒ `RM_MENU_OK` + 不开（`Operate.cs:913-917`）。
    #[test]
    fn deal_try_disabled() {
        let cfg = DealConfig {
            disable_deal: true,
            ..DealConfig::default()
        };
        let mut a = P::new(1, "甲", 1_000);
        let mut b = P::new(2, "乙", 500);
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_try(
            &mut sa,
            &mut sb,
            &cfg,
            10_000,
            DealTryTarget::Other { faces_me: true },
        );
        assert_eq!(names(&evs), vec![(1, messages::SM_MENU_OK)]);
        match &evs[0] {
            DealEvent::MenuOk { text, .. } => assert_eq!(text, DEAL_DISABLE_MSG),
            other => panic!("unexpected {other:?}"),
        }
    }

    /// `ClientAddDealItem`：按 `MakeIndex` + 名称忽略大小写命中 ⇒ `DealItemList` 收件、
    /// `ItemList` 移除；给自己 `SM_DEALADDITEM_OK`，给对手 `SM_DEALREMOTEADDITEM`（`Series=1`）。
    #[test]
    fn add_deal_item_moves_and_notifies() {
        let cfg = DealConfig::default();
        let cat = catalog();
        let (mut a, mut b) = open_pair(&cfg);
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_add_item(&mut sa, &mut sb, &cfg, &cat, 11, "木剑", 11_000);
        assert_eq!(
            names(&evs),
            vec![
                (1, messages::SM_DEALADDITEM_OK),
                (2, messages::SM_DEALREMOTEADDITEM)
            ]
        );
        assert!(a.items.is_empty(), "物品离开背包");
        assert_eq!(a.state.deal_item_list.len(), 1);
        assert_eq!(a.state.deal_item_list[0].make_index, 11);
        assert_eq!(b.state.deal_last_tick, 11_000, "对手 DealLastTick 刷新");
        assert_eq!(a.state.deal_last_tick, 11_000);
    }

    /// 名称不匹配 / 对手已 `DealSuccess` ⇒ `SM_DEALADDITEM_FAIL`，物品不动。
    #[test]
    fn add_deal_item_fail_branches() {
        let cfg = DealConfig::default();
        let cat = catalog();
        let (mut a, mut b) = open_pair(&cfg);
        {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            let evs = deal_add_item(&mut sa, &mut sb, &cfg, &cat, 11, "屠龙", 11_000);
            assert_eq!(evs, vec![def0(1, messages::SM_DEALADDITEM_FAIL)]);
        }
        b.state.deal_success = true;
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_add_item(&mut sa, &mut sb, &cfg, &cat, 11, "木剑", 11_000);
        assert_eq!(evs, vec![def0(1, messages::SM_DEALADDITEM_FAIL)]);
        assert_eq!(a.items.len(), 1, "物品仍在背包");
    }

    /// `CanNotGetBackDeal`（现网 `=1`）⇒ 取回被拒：`RM_MENU_OK` + `SM_DEALDELITEM_FAIL`，
    /// 交易栏物品**不**退回。
    #[test]
    fn del_deal_item_denied_by_can_not_get_back() {
        let cfg = DealConfig::default();
        assert!(cfg.can_not_get_back_deal);
        let cat = catalog();
        let (mut a, mut b) = open_pair(&cfg);
        {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            let _ = deal_add_item(&mut sa, &mut sb, &cfg, &cat, 11, "木剑", 11_000);
        }
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_del_item(&mut sa, &mut sb, &cfg, &cat, 11, "木剑", 11_500);
        assert_eq!(
            names(&evs),
            vec![
                (1, messages::SM_DEALDELITEM_FAIL),
                (1, messages::SM_MENU_OK)
            ]
        );
        assert_eq!(a.state.deal_item_list.len(), 1, "不许取回");
        assert!(a.items.is_empty());
    }

    /// `CanNotGetBackDeal=false` 时可取回：`ItemList` 收回，发 `SM_DEALDELITEM_OK` +
    /// `SM_DEALREMOTEDELITEM`。
    #[test]
    fn del_deal_item_returns_when_allowed() {
        let cfg = DealConfig {
            can_not_get_back_deal: false,
            ..DealConfig::default()
        };
        let cat = catalog();
        let (mut a, mut b) = open_pair(&cfg);
        {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            let _ = deal_add_item(&mut sa, &mut sb, &cfg, &cat, 11, "木剑", 11_000);
        }
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_del_item(&mut sa, &mut sb, &cfg, &cat, 11, "木剑", 11_500);
        assert_eq!(
            names(&evs),
            vec![
                (2, messages::SM_DEALREMOTEDELITEM),
                (1, messages::SM_DEALDELITEM_OK)
            ]
        );
        assert_eq!(a.items.len(), 1, "物品回背包");
        assert!(a.state.deal_item_list.is_empty());
    }

    /// `ClientChangeDealGold`：成功时 `Gold = Gold + DealGolds - nGold`，`DealGolds = nGold`，
    /// 回 `SM_DEALCHGGOLD_OK`（`Recog=DealGolds, Param=Lo(Gold), Tag=Hi(Gold)`）+ 对手
    /// `SM_DEALREMOTECHGGOLD`（`Recog=DealGolds`）。
    #[test]
    fn change_deal_gold_ok_and_fail() {
        let cfg = DealConfig::default();
        let (mut a, mut b) = open_pair(&cfg);
        {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            let evs = deal_change_gold(&mut sa, &mut sb, &cfg, 200, 11_000, true);
            assert_eq!(
                names(&evs),
                vec![
                    (1, messages::SM_DEALCHGGOLD_OK),
                    (2, messages::SM_DEALREMOTECHGGOLD)
                ]
            );
            match &evs[0] {
                DealEvent::Def {
                    recog, param, tag, ..
                } => assert_eq!(
                    (*recog, *param, *tag),
                    (200, smframe::lo_word(800), smframe::hi_word(800))
                ),
                other => panic!("unexpected {other:?}"),
            }
            assert_eq!((a.gold, a.state.deal_golds), (800, 200));
        }
        // `DealGolds>0 && CanNotGetBackDeal` ⇒ oracle 原样回 `SM_DEALDELITEM_FAIL`（bug 照搬）。
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_change_gold(&mut sa, &mut sb, &cfg, 0, 11_500, true);
        assert_eq!(
            names(&evs),
            vec![
                (1, messages::SM_DEALDELITEM_FAIL),
                (1, messages::SM_MENU_OK)
            ],
            "oracle 缺陷：守卫回 DELITEM_FAIL"
        );
    }

    /// 非负金币但 `nGold > Gold+DealGolds` ⇒ `SM_DEALCHGGOLD_FAIL`，金币不动。
    #[test]
    fn change_deal_gold_fail_when_insufficient() {
        let cfg = DealConfig::default();
        let (mut a, mut b) = open_pair(&cfg);
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_change_gold(&mut sa, &mut sb, &cfg, 1_000_000, 11_000, true);
        assert_eq!(names(&evs), vec![(1, messages::SM_DEALCHGGOLD_FAIL)]);
        assert_eq!((a.gold, a.state.deal_golds), (1_000, 0));
    }

    /// `DealOKTime` 时间门（任一侧 `now-DealLastTick < 1000`）⇒ 红 `SysMsg` + `DealCancel`。
    #[test]
    fn deal_end_ok_time_gate_cancels() {
        let cfg = DealConfig::default();
        let (mut a, mut b) = open_pair(&cfg);
        b.state.deal_last_tick = 10_500;
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_end(&mut sa, &mut sb, &cfg, 11_000);
        assert_eq!(
            names(&evs),
            vec![
                (1, messages::SM_DEALCANCEL),
                (2, messages::SM_DEALCANCEL),
                (1, u16::MAX),
                (2, u16::MAX),
                (1, u16::MAX),
            ],
            "立即两条 DEALCANCEL；随后排队红提示 + 两条绿色取消提示"
        );
        assert!(!a.state.dealing && !b.state.dealing);
    }

    /// 双向锁定：一侧先确认 ⇒ 只回 `YouDealOKMsg`/`PoseDealOKMsg`，不结算。
    #[test]
    fn deal_end_first_side_waits() {
        let cfg = DealConfig::default();
        let (mut a, mut b) = open_pair(&cfg);
        a.state.deal_last_tick = 5_000;
        b.state.deal_last_tick = 5_000;
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_end(&mut sa, &mut sb, &cfg, 10_000);
        assert_eq!(names(&evs), vec![(1, u16::MAX), (2, u16::MAX)]);
        assert!(a.state.deal_success, "本侧已锁定");
        assert!(!b.state.deal_success);
        assert!(a.state.dealing && b.state.dealing, "交易未结束");
    }

    /// 结算：物品双向搬运 + `SM_ADDITEM` 到各自 + `SM_DEALSUCCESS`；背包不残留。
    #[test]
    fn deal_end_settles_items() {
        let cfg = DealConfig::default();
        let cat = catalog();
        let (mut a, mut b) = open_pair(&cfg);
        {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            let _ = deal_add_item(&mut sa, &mut sb, &cfg, &cat, 11, "木剑", 11_000);
        }
        {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            let _ = deal_add_item(&mut sb, &mut sa, &cfg, &cat, 22, "金创药", 11_000);
        }
        a.state.deal_last_tick = 9_000;
        b.state.deal_success = true;
        b.state.deal_last_tick = 9_000;
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_end(&mut sa, &mut sb, &cfg, 11_000);
        // 立即：给乙 SM_ADDITEM(木剑)；给甲 SM_ADDITEM(金创药)；乙 SM_DEALSUCCESS；甲 SM_DEALSUCCESS。
        let immediates: Vec<(i32, u16)> = names(&evs)
            .into_iter()
            .filter(|(_, ident)| {
                *ident == messages::SM_ADDITEM || *ident == messages::SM_DEALSUCCESS
            })
            .collect();
        assert_eq!(
            immediates,
            vec![
                (2, messages::SM_ADDITEM),
                (1, messages::SM_ADDITEM),
                (2, messages::SM_DEALSUCCESS),
                (1, messages::SM_DEALSUCCESS),
            ]
        );
        assert_eq!(a.items.len(), 1);
        assert_eq!(b.items.len(), 1);
        assert_eq!(a.items[0].make_index, 22, "甲收到乙的金创药");
        assert_eq!(b.items[0].make_index, 11, "乙收到甲的木剑");
        assert!(!a.state.dealing && !b.state.dealing);
        assert!(a.state.deal_item_list.is_empty() && b.state.deal_item_list.is_empty());
    }

    /// 容量四检：甲背包装满 46 件、乙给 1 件 ⇒ 不结算，`DealCancel`（红提示 + 取消链）。
    #[test]
    fn deal_end_capacity_check_cancels() {
        let cfg = DealConfig::default();
        let cat = catalog();
        let (mut a, mut b) = open_pair(&cfg);
        {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            let _ = deal_add_item(&mut sa, &mut sb, &cfg, &cat, 11, "木剑", 11_000);
            let _ = deal_add_item(&mut sb, &mut sa, &cfg, &cat, 22, "金创药", 11_000);
        }
        // 甲背包补满到 46（含交易栏的 1 件已移出 ⇒ 当前 0 件）。
        while a.items.len() < 46 {
            a.items.push(bag(2, 100 + a.items.len() as i32));
        }
        a.state.deal_last_tick = 9_000;
        b.state.deal_last_tick = 9_000;
        b.state.deal_success = true;
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_end(&mut sa, &mut sb, &cfg, 11_000);
        // 红提示（`YourBagSizeTooSmall`）+ 取消链。
        assert!(
            evs.iter().any(|e| matches!(
                e,
                DealEvent::SysMsg { text, .. } if text == DEAL_MY_BAG_SMALL_MSG
            )),
            "容量不足必提示"
        );
        assert!(evs.iter().any(
            |e| matches!(e, DealEvent::Def { ident, .. } if *ident == messages::SM_DEALCANCEL)
        ));
        assert!(!a.state.dealing && !b.state.dealing);
        assert!(
            a.state.deal_item_list.is_empty() && b.state.deal_item_list.is_empty(),
            "取消后交易栏清空、物品退回背包"
        );
        assert!(
            a.items.iter().any(|it| it.make_index == 11),
            "甲的木剑退回背包"
        );
    }

    /// 金币结算：甲给 300，乙给 100 ⇒ 双方 `Gold` 净变；`SM_GOLDCHANGED` 排到队列。
    #[test]
    fn deal_end_settles_gold() {
        let cfg = DealConfig::default();
        let (mut a, mut b) = open_pair(&cfg);
        a.state.deal_golds = 300;
        a.gold = 700;
        b.state.deal_golds = 100;
        b.gold = 400;
        a.state.deal_last_tick = 9_000;
        b.state.deal_last_tick = 9_000;
        b.state.deal_success = true;
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_end(&mut sa, &mut sb, &cfg, 11_000);
        assert_eq!(a.gold, 800, "700 + 100");
        assert_eq!(b.gold, 700, "400 + 300");
        assert_eq!(
            names(&evs)
                .into_iter()
                .filter(|(_, i)| *i == messages::SM_GOLDCHANGED)
                .collect::<Vec<_>>(),
            vec![(2, messages::SM_GOLDCHANGED), (1, messages::SM_GOLDCHANGED)]
        );
    }

    /// `DealCancel` 递归：两侧都被取消（各一条 `SM_DEALCANCEL`），交易栏物品归还。
    #[test]
    fn deal_cancel_is_recursive() {
        let cfg = DealConfig::default();
        let cat = catalog();
        let (mut a, mut b) = open_pair(&cfg);
        {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            let _ = deal_add_item(&mut sa, &mut sb, &cfg, &cat, 11, "木剑", 11_000);
        }
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_cancel(&mut sa, &mut sb, &cfg, 12_000);
        assert_eq!(
            names(&evs),
            vec![
                (1, messages::SM_DEALCANCEL),
                (2, messages::SM_DEALCANCEL),
                (2, u16::MAX),
                (1, u16::MAX),
            ]
        );
        assert_eq!(a.items.len(), 1, "甲的木剑退回");
        assert_eq!(a.state.deal_last_tick, 12_000);
        assert!(a.state.deal_creature.is_none() && b.state.deal_creature.is_none());
    }

    /// `DealTryTarget::NotPlayer`：前置守卫全过但前排不是玩家 ⇒ `SM_DEALTRY_FAIL`，不开交易。
    #[test]
    fn deal_try_not_player_fails() {
        let cfg = DealConfig::default();
        let mut a = P::new(1, "甲", 1_000);
        let mut b = P::new(2, "乙", 500);
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_try(&mut sa, &mut sb, &cfg, 10_000, DealTryTarget::NotPlayer);
        assert_eq!(names(&evs), vec![(1, messages::SM_DEALTRY_FAIL)]);
        assert!(!a.state.dealing && !b.state.dealing);
    }

    /// `DealTryTarget::Absent`：无前排对象（oracle `GetPoseCreate() == null` ⇒ `Operate.cs:929`
    /// 对 null 解引用 ⇒ NRE 被 `WorldServer.cs:734` 吞）或前排为自身（`:934` 内层 `if` 无 `else`）
    /// ⇒ oracle **一个帧都不发**；Rust 不得发 `SM_DEALTRY_FAIL`。
    #[test]
    fn deal_try_absent_pose_emits_nothing() {
        let cfg = DealConfig::default();
        let mut a = P::new(1, "甲", 1_000);
        let mut b = P::new(2, "乙", 500);
        let evs = {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            deal_try(&mut sa, &mut sb, &cfg, 10_000, DealTryTarget::Absent)
        };
        assert!(
            evs.is_empty(),
            "无前排/前排为自身 ⇒ oracle 静默，实得 {evs:?}"
        );
        assert!(!a.state.dealing && !b.state.dealing);
        // 未开交易对话 ⇒ `DealCreat`/`DealLastTick` 不被改写。
        assert!(a.state.deal_creature.is_none());
        assert_eq!(a.state.deal_last_tick, 0);
    }

    /// 前置守卫**先于**目标判定（oracle `Operate.cs:913-926` 四条守卫在 `:928` 的
    /// `GetPoseCreate()` 之前）：`Absent` 也要先吃 `DisableDeal`/`Dealing`/`TryDealTime`/`IsCanDeal`。
    #[test]
    fn deal_try_absent_pose_still_obeys_guards_first() {
        // ① `DisableDeal` ⇒ `RM_MENU_OK`（DisableDealItemsMsg），早于 pose 取数。
        let cfg_disabled = DealConfig {
            disable_deal: true,
            ..DealConfig::default()
        };
        let mut a = P::new(1, "甲", 1_000);
        let mut b = P::new(2, "乙", 500);
        let evs = {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            deal_try(
                &mut sa,
                &mut sb,
                &cfg_disabled,
                10_000,
                DealTryTarget::Absent,
            )
        };
        assert_eq!(names(&evs), vec![(1, messages::SM_MENU_OK)]);

        // ② `Dealing` ⇒ 静默返回。
        let cfg = DealConfig::default();
        a.state.dealing = true;
        let evs = {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            deal_try(&mut sa, &mut sb, &cfg, 10_000, DealTryTarget::Absent)
        };
        assert!(evs.is_empty(), "Dealing ⇒ 直接返回");

        // ③ `TryDealTime` 未过 ⇒ `RM_MENU_OK`（PleaseTryDealLaterMsg）。
        a.state.dealing = false;
        a.state.deal_last_tick = 9_000;
        let evs = {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            deal_try(&mut sa, &mut sb, &cfg, 10_000, DealTryTarget::Absent)
        };
        assert_eq!(names(&evs), vec![(1, messages::SM_MENU_OK)]);

        // ④ `!IsCanDeal` ⇒ `RM_MENU_OK`（CanotTryDealMsg）。
        a.state.deal_last_tick = 0;
        a.state.is_can_deal = false;
        let evs = {
            let mut sa = side(&mut a);
            let mut sb = side(&mut b);
            deal_try(&mut sa, &mut sb, &cfg, 10_000, DealTryTarget::Absent)
        };
        assert_eq!(names(&evs), vec![(1, messages::SM_MENU_OK)]);
    }

    /// `first_token`（`HUtil32.GetValidStr3(..., ' ')`，信件后缀场景）：含空格取首段。
    #[test]
    fn first_token_matches_get_valid_str3() {
        assert_eq!(first_token("木剑"), "木剑");
        assert_eq!(first_token("木剑 3/10"), "木剑");
        assert_eq!(first_token("  木剑"), "木剑");
        assert_eq!(first_token(""), "");
    }

    /// 帧编码：`SM_DEALMENU` 尾 = `EncodeString`、`SM_DEALREMOTEADDITEM` 头 + `ClientItem` 尾、
    /// `SM_ADDITEM` `Series=1`。
    #[test]
    fn event_bodies_match_oracle_shapes() {
        let cfg = DealConfig::default();
        let cat = catalog();
        let (mut a, mut b) = open_pair(&cfg);
        let mut sa = side(&mut a);
        let mut sb = side(&mut b);
        let evs = deal_add_item(&mut sa, &mut sb, &cfg, &cat, 11, "木剑", 11_000);
        let wire = |id: i32| DealWire {
            actor_id: id,
            gold: 0,
            game_gold: 0,
            wabil: Ability::default(),
        };
        let (_, ok_body) = deal_event_body(&evs[0], &cat, &wire).expect("OK 编码");
        let recog = i32::from_le_bytes([ok_body[0], ok_body[1], ok_body[2], ok_body[3]]);
        let ident = u16::from_le_bytes([ok_body[4], ok_body[5]]);
        assert_eq!((recog, ident), (0, messages::SM_DEALADDITEM_OK));
        assert_eq!(ok_body.len(), 12, "header-only");
        let (_, remote) = deal_event_body(&evs[1], &cat, &wire).expect("remote 编码");
        let series = u16::from_le_bytes([remote[10], remote[11]]);
        assert_eq!(
            (
                i32::from_le_bytes([remote[0], remote[1], remote[2], remote[3]]),
                u16::from_le_bytes([remote[4], remote[5]]),
                series
            ),
            (1, messages::SM_DEALREMOTEADDITEM, 1)
        );
        assert!(remote.len() > 12, "带 ClientItem 尾");
    }
}
