//! 元宝寄售（`goldsales` / `SellOff`）世界态 + 7 个 `CM_SELLOFF*` 派发
//! —— M3⑩-实现 1（卡 `openmir2-m3-selloff-consignment-world`）。
//!
//! oracle 的元宝寄售是一套**进程级全局列表 + 玩家会话散字段**：
//! `M2Share.SellOffItemList`（`M2Share.cs:79`，`IList<DealOffInfo>`，**静态**）与
//! `PlayObject` 上的 `SaleDeal` / `SellOffConfirm` / `SellOffItemList`（临时列表）。
//! 本仓库把「背包/金币/元宝」放在 bins 会话态（[`crate::ground::GroundState`] /
//! [`crate::bag`]），与 ⑦-3 交易（[`crate::trade`]）同形，故本模块也落 bins：
//! **纯函数 + 事件**，不碰 I/O、不碰全局 `static`、不碰时钟（`now` 由宿主注入）。
//! `M2Share.SellOffItemList` 的进程级语义由 [SellOffBook] 承载，宿主在 `sim_loop` 里
//! 以**一个** `SellOffBook` 实例表示（所有会话共享），不引入 `static`（确定性硬约束 §4.2.2）。
//!
//! # oracle 锚点（行号为在本仓库 `src/` 实测）
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`DealOffInfo`] | `OpenMir2/Data/DealOffInfo.cs:9-38`（`sDealChrName`/`sBuyChrName`/`dSellDateTime`/`nSellGold`/`UseItems`/`Flag`） |
//! | [`SellOffBook`] | `M2Server/M2Share.cs:79`（`SellOffItemList`） |
//! | [`SellOffState`] | `M2Server/Player/PlayObject.cs:856/864`（`SaleDeal`/`SellOffConfirm`/`SellOffItemList`），构造 `:937` |
//! | [`update_sell_off_info`] | `PlayObject.Sale.cs:19-56` |
//! | [`client_add_sell_off_item`] | `PlayObject.Sale.cs:60-125` |
//! | [`client_del_sell_off_item`] | `PlayObject.Sale.cs:127-183` |
//! | [`client_cancel_sell_off_ing`] | `PlayObject.Sale.cs:185-252` |
//! | [`client_buy_cancel_sell_off`] | `PlayObject.Sale.cs:254-279` |
//! | [`client_buy_sell_off_item`] | `PlayObject.Sale.cs:281-385`（**成交**） |
//! | [`get_sell_off_gold`] | `PlayObject.Sale.cs:387-429`（出售者取款） |
//! | [`sell_off_cancel`] / [`client_cancel_sell_off`] | `PlayObject.Sale.cs:431-438/516-591` |
//! | [`select_sell_date`] | `PlayObject.Sale.cs:440-473`（**无活调用点**，见「未建模」） |
//! | [`sell_off_in_time`] | `PlayObject.Sale.cs:475-514` |
//! | [`client_sell_off_end`] | `PlayObject.Sale.cs:516-591`（**上架**） |
//! | [`get_back_sell_off_items`] | `PlayObject.Sale.cs:602-617`（退回） |
//! | 派发 | `M2Server/Player/PlayObject.Message.cs:2057-2077`（7 个 `CM_SELLOFF*`） |
//!
//! # 帧面口径（立即 / 排队两桶）
//!
//! 与 [`crate::trade`] 同一约定：`SendAddItem`（`PlayObject.Base.cs:198`）是**立即**帧
//! （`SendSocket(SM_ADDITEM)`，`Series=1`）；其余全部走 `SendMsg`/`SendUpdateMsg`
//! 入 `MsgQueue`（优先级 = `(byte)wIdent`），在**同一** `Run()` 内按优先级**升序**出队，
//! 出队时 `RM_*` 映射成 `SM_*` 再 `SendSocket`。⇒ 线上序 = 立即帧（调用序）→ 排队帧
//! （优先级升序，同优先级 FIFO）。
//!
//! 排队帧优先级字节：`RM_SELLOFFADDITEM_OK(23004)⇒220` / `RM_SellOffADDITEM_FAIL(23006)⇒222` /
//! `RM_SELLOFFDELITEM_OK(23009)⇒225` / `RM_SELLOFFDELITEM_FAIL(23011)⇒227` /
//! `RM_SELLOFFCANCEL(23013)⇒229` / `RM_SELLOFFEND_OK(23017)⇒233` / `RM_SELLOFFEND_FAIL(23019)⇒235` /
//! `RM_SELLOFFBUY_OK(23028)⇒244` / `RM_MENU_OK(10309)⇒69` / `RM_GAMEGOLDCHANGED(8084)⇒148` /
//! `RM_WEIGHTCHANGED(10115)⇒131` / `RM_SYSMESSAGE(11004)⇒252`。
//!
//! # 连 oracle 缺陷一起搬（§1 硬约束③）
//!
//! 1. **`ClientBuySellOffItme` 物品循环空引用/越界**（`PlayObject.Sale.cs:333-361`）：
//!    `for (int j = 0; j <= 9; j++)` 对长度 9 的 `UseItems[]` 越界，且循环体**不判 `UseItems[j] == null`
//!    就直接 `.Index`** ⇒ 会话内建的寄售条目（`ClientSellOffEnd` 只填 `Count` 个槽）在**第一个空槽**
//!    （或 `j == 9`）抛 `NullReferenceException`/`IndexOutOfRangeException`，被外层 `try/catch` 吞掉
//!    ⇒ **`RM_SELLOFFBUY_OK`/`RM_MENU_OK` 恒不可达**（物品 0..首个空槽仍已入包、元宝已扣/已付）。
//!    本模块用「遇 `None` 即中止」1:1 复刻（[`client_buy_sell_off_item`] 的 `aborted`）。
//! 2. **金刚石标记自相矛盾**（`:537-558` vs `:343`）：`ClientSellOffEnd` 把 `Index/Dura/DuraMax` 置
//!    `ushort.MaxValue`（65535），而 `ClientBuySellOffItme`/`ClientCancelSellOffIng` 判的是
//!    `short.MaxValue`（32767）⇒ 会话内建的金刚石条目在买/卖方侧**永不被识别为金刚石**，金钢石丢失。
//!    本模块照搬（写 65535、判 32767）。
//! 3. **金刚石扣款帧错位**（`:221/570`）：改的是 `Gold`（实物金币）却调 `GameGoldChanged()`（元宝帧）。
//!    照搬。
//! 4. `ClientCancelSellOffIng` 的「背包够但不满足负重 / `AddItemToBag` 失败」分支**既不回包也不落地**
//!    （`:203-212` 的 `if(IsEnoughBag()){ if(IsAddWeightAvailable()){ if(AddItemToBag)…} }` 无 else）
//!    ⇒ 物品静默丢失。照搬。
//!
//! # 未建模（登记，不假造）
//!
//! - `CustomItemMgr.GetCustomItemName`（`Desc[13]==1` 的自定义物品名）：与 `T-24` 同源，
//!   [`crate::bag::item_display_name`] 对 `Desc[13]==1` 返回空串 ⇒ 自定义名物品的名字比对走空串
//!   （不在本卡触发面内，登记）。
//! - `GameGoldChanged()` → `SendGoldInfo(false)` 的 `SoftVersionDateEx == 0` 门（`Base.cs:930`）：
//!   本模块由 [`SellOffSide::send_game_gold_info`] 表示（`false` ⇒ 不发 `SM_GAMEGOLDNAME`）。
//! - `WeightChanged()` 重算并回 `RM_WEIGHTCHANGED`：本模块只回帧，`WAbil.Weight` 的重算仍归
//!   `crate::bag`（与 [`crate::trade`] 同口径 ⇒ 帧值可能与 oracle 不同，登记）。
//! - `DropItemDown`（`Base.cs:529`）：oracle 因 `MapItem.ItemId` 恒 0 而**恒落地失败**（`T-19`/`T-25`）
//!   ⇒ 本模块的 [`drop_item_down`] 只「消耗」物品、不产帧（1:1 的可见面）。
//! - [`select_sell_date`]：oracle 的两个调用点（`NormNpc.cs:465/1388`）**都被注释** ⇒ 无活调用点；
//!   本模块实现其纯逻辑但不接线（`dSellDateTime` 的本地时区格式化口径登记）。
//! - `GetSellOffGlod()` 的登录调用点（`PlayObject.cs:1486`）：`SaleDeal` 在 Rust 无 setter
//!   （脚本 `OPENYBDEAL`/`ActionOfOpenSaleDeal` 未移植，见 `crates/script/src/handlers.rs:250`）
//!   ⇒ 恒 `false` ⇒ 该函数恒 no-op，登录路径**不接线**（行为等价：都不动）。见证据 README 登记。

use crate::ability::ItemCatalog;
use crate::bag::{item_display_name, BagItem, MAX_BAG_ITEM};
use crate::downlink::gbk;
use crate::smframe;
use mir2_protocol::{edcode, messages};

/// `DealOffInfo.UseItems` 的长度（`ClientSellOffEnd` 的 `new UserItem[9]`，`:526`）。
pub const DEAL_OFF_ITEMS: usize = 9;
/// `SellOffItemList.Count < 9`（`PlayObject.Sale.cs:93` 的临时列表上限）。
pub const SELL_OFF_TEMP_MAX: usize = 9;
/// oracle `short.MaxValue`（金刚石判定的 `Index == short.MaxValue`，`:343` 等）。
const SHORT_MAX: i32 = i16::MAX as i32;
/// oracle `ushort.MaxValue`（写入金刚石标记，`:556-558`）。
const USHORT_MAX: u16 = u16::MAX;

/// `RM_SELLOFFADDITEM_OK(23004)` 的队列优先级字节。
const PRIO_RM_SELLOFFADDITEM_OK: u8 = (messages::RM_SELLOFFADDITEM_OK & 0xFF) as u8;
/// `RM_SellOffADDITEM_FAIL(23006)`。
const PRIO_RM_SELLOFFADDITEM_FAIL: u8 = (messages::RM_SellOffADDITEM_FAIL & 0xFF) as u8;
/// `RM_SELLOFFDELITEM_OK(23009)`。
const PRIO_RM_SELLOFFDELITEM_OK: u8 = (messages::RM_SELLOFFDELITEM_OK & 0xFF) as u8;
/// `RM_SELLOFFDELITEM_FAIL(23011)`。
const PRIO_RM_SELLOFFDELITEM_FAIL: u8 = (messages::RM_SELLOFFDELITEM_FAIL & 0xFF) as u8;
/// `RM_SELLOFFCANCEL(23013)`。
const PRIO_RM_SELLOFFCANCEL: u8 = (messages::RM_SELLOFFCANCEL & 0xFF) as u8;
/// `RM_SELLOFFEND_OK(23017)`。
const PRIO_RM_SELLOFFEND_OK: u8 = (messages::RM_SELLOFFEND_OK & 0xFF) as u8;
/// `RM_SELLOFFEND_FAIL(23019)`。
const PRIO_RM_SELLOFFEND_FAIL: u8 = (messages::RM_SELLOFFEND_FAIL & 0xFF) as u8;
/// `RM_SELLOFFBUY_OK(23028)`。
const PRIO_RM_SELLOFFBUY_OK: u8 = (messages::RM_SELLOFFBUY_OK & 0xFF) as u8;
/// `RM_MENU_OK(10309)`。
const PRIO_RM_MENU_OK: u8 = (10_309 & 0xFF) as u8;
/// `RM_WEIGHTCHANGED(10115)`。
const PRIO_RM_WEIGHTCHANGED: u8 = (10_115 & 0xFF) as u8;
/// `RM_GAMEGOLDCHANGED(8084)`。
const PRIO_RM_GAMEGOLDCHANGED: u8 = (8_084 & 0xFF) as u8;
/// `RM_SYSMESSAGE(11004)`。
const PRIO_RM_SYSMESSAGE: u8 = (11_004 & 0xFF) as u8;

/// `SendUpdateMsg` 的去重 key（同 ident 覆盖，`BaseObject.Message.cs:283-306`）。
const KEY_RM_GAMEGOLDCHANGED: u16 = messages::RM_GAMEGOLDCHANGED;

/// `[Setup] BlueMsgFColor`（`GameSvrConf.cs:1538` 缺省 `0xFF`）。
pub const DEFAULT_BLUE_F_COLOR: u8 = 0xFF;
/// `[Setup] BlueMsgBColor`（`GameSvrConf.cs:1539` 缺省 `0xFC`）。
pub const DEFAULT_BLUE_B_COLOR: u8 = 0xFC;

/// 元宝寄售相关的运行时配置（host 从 `Server.conf`/`GameSvrConf` 取）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SellOffConfig {
    /// `SystemShare.Config.DecUserGameGold`（`GameSvrConf.cs:1217/1919`，**硬编码缺省 10000**，
    /// oracle 无配置键读取 ⇒ 本结构也按常量给默认）。
    pub dec_user_game_gold: i32,
    /// `Grobal2.MaxBagItem`（`Grobal2.cs:62`，缺省 46）——`IsEnoughBag()`/`AddItemToBag` 用。
    pub max_bag_item: usize,
    /// `[Setup] BlueMsgFColor`。
    pub blue_f_color: u8,
    /// `[Setup] BlueMsgBColor`。
    pub blue_b_color: u8,
}

impl Default for SellOffConfig {
    fn default() -> Self {
        SellOffConfig {
            dec_user_game_gold: 10_000,
            max_bag_item: MAX_BAG_ITEM,
            blue_f_color: DEFAULT_BLUE_F_COLOR,
            blue_b_color: DEFAULT_BLUE_B_COLOR,
        }
    }
}

/// 元宝寄售数据结构（`OpenMir2/Data/DealOffInfo.cs`）。
///
/// `use_items` 用 `Vec<Option<BagItem>>` 表示 C# 的 `UserItem[]`（元素可为 `null`）——
/// 空槽的 `None` 是复刻 `ClientBuySellOffItme` 空引用中止的**关键**（见模块头缺陷①）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DealOffInfo {
    /// 寄售人（`sDealChrName`）。
    pub deal_chr_name: String,
    /// 购买人（`sBuyChrName`）。
    pub buy_chr_name: String,
    /// 寄售时间（`dSellDateTime`；oracle `DateTime.Now`，本模块注入毫秒）。
    pub sell_datetime_ms: i64,
    /// 交易的元宝数（`nSellGold`）。
    pub sell_gold: i32,
    /// 物品（`UseItems`；会话内建者长度 = [`DEAL_OFF_ITEMS`]）。
    pub use_items: Vec<Option<BagItem>>,
    /// `Flag`（0 寄售中 / 1 已售未取款 / 2 交易结束 / 3 购买人取消 / 4 已取消/已购锁定）。
    pub flag: u8,
}

impl DealOffInfo {
    /// `new DealOffInfo() { UseItems = new UserItem[9] }`（`PlayObject.Sale.cs:526`）。
    #[must_use]
    pub fn new_with_slots() -> Self {
        DealOffInfo {
            deal_chr_name: String::new(),
            buy_chr_name: String::new(),
            sell_datetime_ms: 0,
            sell_gold: 0,
            use_items: vec![None; DEAL_OFF_ITEMS],
            flag: 0,
        }
    }
}

/// `M2Share.SellOffItemList`（`M2Share.cs:79`）——**进程级**全局寄售列表。
///
/// 遍历/删除顺序与 oracle 一致：`Add` 尾追；各处扫描均 `for i = Count-1 downto 0`。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SellOffBook {
    /// 条目（顺序 = 入列序）。
    pub entries: Vec<DealOffInfo>,
}

/// 一个在线角色的元宝寄售会话态（oracle `PlayObject` 上的散字段；不落库）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SellOffState {
    /// `PlayObject.SaleDeal`（`:856`，是否开通元宝服务；构造 `:937` = `false`）。
    pub sale_deal: bool,
    /// `PlayObject.SellOffConfirm`（`:862`；构造 `false`）。
    pub sell_off_confirm: bool,
    /// `PlayObject.SellOffItemList`（`:864`；构造 `new List`；顺序 = 放入序）。
    pub sell_off_item_list: Vec<BagItem>,
}

/// 寄售操作作用在**一侧**上的可变状态（背包/金币/元宝 + 身份/上限/展示）。
///
/// `id`/`name`/`game_point`/… 是只读快照；`items`/`gold`/`game_gold`/`state` 是宿主会话态的借用。
pub struct SellOffSide<'a> {
    /// `BaseObject.ActorId`。
    pub id: i32,
    /// `BaseObject.ChrName`。
    pub name: String,
    /// `PlayObject.OffLineFlag`（挂机/离线；购方付款时据此决定 `Flag=1`）。
    pub offline: bool,
    /// `PlayObject.ItemList`（顺序敏感）。
    pub items: &'a mut Vec<BagItem>,
    /// `BaseObject.Gold`（**金刚石**口径，oracle 误用此字段）。
    pub gold: &'a mut i32,
    /// `BaseObject.GameGold`（元宝）。
    pub game_gold: &'a mut i32,
    /// `BaseObject.GamePoint`（`RM_GAMEGOLDCHANGED` 帧的 LoWord/HiWord）。
    pub game_point: i32,
    /// `BaseObject.WAbil.Weight`（`RM_WEIGHTCHANGED` 帧）。
    pub weight: u16,
    /// `BaseObject.WAbil.WearWeight`。
    pub wear_weight: u8,
    /// `BaseObject.WAbil.HandWeight`。
    pub hand_weight: u8,
    /// `BaseObject.WAbil.MaxWeight`（`IsAddWeightAvailable` 的门）。
    pub max_weight: u16,
    /// `PlayObject.SoftVersionDateEx != 0`（`SendGoldInfo` 的门）。
    pub send_game_gold_info: bool,
    /// 寄售会话态。
    pub state: &'a mut SellOffState,
}

/// 一条寄售操作产出的**帧面事件**（顺序 = 立即桶 ++ 排队桶，见模块头）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SellOffEvent {
    /// `SendAddItem(userItem)`（立即）：`SM_ADDITEM(200)` + `EncodeBuffer(ClientItem)`，`Series=1`。
    AddItem { target: i32, item: BagItem },
    /// 排队帧（`priority` = `(byte)RM_ident`）。
    Queued {
        priority: u8,
        #[allow(dead_code)]
        key: Option<(i32, u16)>,
        body: SellOffFrame,
    },
}

/// 排队帧的最终 `SM_*` 形态（oracle 在 `MsgQueue` 出队时由 `RM_*` 映射）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SellOffFrame {
    /// `SendDefMessage(ident, recog, param, tag, series[, name])`（立即下发）。
    Def {
        target: i32,
        ident: u16,
        recog: i32,
        param: u16,
        tag: u16,
        series: u16,
        name: Option<String>,
    },
    /// `SendDefMessage(SM_MENU_OK, nParam1, 0, 0, 0, Msg)`（尾 = `EncodeString(Msg)`）。
    MenuOk {
        target: i32,
        recog: i32,
        text: String,
    },
    /// `SendDefMessage(SM_SYSMESSAGE, ActorId, MakeWord(f,b), 0, 1, Msg)`。
    SysMsg {
        target: i32,
        recog: i32,
        f_color: u8,
        b_color: u8,
        text: String,
    },
    /// `SendGoldInfo(false)` 的 `SM_GAMEGOLDNAME`。
    GameGoldChanged {
        target: i32,
        game_gold: i32,
        game_point: i32,
    },
    /// `WeightChanged()` 的 `SM_WEIGHTCHANGED`。
    WeightChanged {
        target: i32,
        weight: u16,
        wear_weight: u8,
        hand_weight: u8,
    },
}

/// 一条排队帧：`(优先级字节, 去重 key, 出队后的 SM 形态)`。
type QueuedFrame = (u8, Option<(i32, u16)>, SellOffFrame);

/// 「立即帧 + 排队帧」两桶。`finish` = 立即（插入序）++ 排队（优先级升序，稳定）。
#[derive(Default)]
struct Emitter {
    immediate: Vec<SellOffEvent>,
    queued: Vec<QueuedFrame>,
}

impl Emitter {
    fn new() -> Self {
        Emitter::default()
    }

    /// `SendSocket`：立即帧。
    fn now(&mut self, ev: SellOffEvent) {
        self.immediate.push(ev);
    }

    /// `SendMsg`：入队（同优先级 FIFO）。
    fn later(&mut self, priority: u8, body: SellOffFrame) {
        self.queued.push((priority, None, body));
    }

    /// `SendUpdateMsg`：先移除**同一目标**上同 `ident` 的旧条目，再入队。
    fn update(&mut self, priority: u8, target: i32, key: u16, body: SellOffFrame) {
        self.queued.retain(|(_, k, _)| *k != Some((target, key)));
        self.queued.push((priority, Some((target, key)), body));
    }

    fn finish(mut self) -> Vec<SellOffEvent> {
        // `sort_by_key` 稳定 ⇒ 同优先级保持入队序。
        self.queued.sort_by_key(|(p, _, _)| *p);
        let mut out = self.immediate;
        out.extend(
            self.queued
                .into_iter()
                .map(|(priority, key, body)| SellOffEvent::Queued {
                    priority,
                    key,
                    body,
                }),
        );
        out
    }
}

/// `IsEnoughBag()`（`PlayObject.Base.cs:183`）：`ItemList.Count < Grobal2.MaxBagItem`。
#[must_use]
fn is_enough_bag(items: &[BagItem], max_bag_item: usize) -> bool {
    items.len() < max_bag_item
}

/// `AddItemToBag(userItem)`（`BaseObject.cs:1241-1251`）：满则 `false`；否则入包 + `WeightChanged()`。
fn add_item_to_bag(
    side: &mut SellOffSide<'_>,
    item: BagItem,
    cfg: &SellOffConfig,
    em: &mut Emitter,
) -> bool {
    if side.items.len() >= cfg.max_bag_item {
        return false;
    }
    side.items.push(item);
    weight_changed(side, em);
    true
}

/// `DropItemDown(userItem, 3, false, ActorId, ActorId)`（`BaseObject.cs:529`）：
/// oracle 因 `MapItem.ItemId` 恒 0 而**恒落地失败**（`T-19`/`T-25`）⇒ 只「消耗」、不产帧。
fn drop_item_down(_item: BagItem) {}

/// `WeightChanged()`（`BaseObject.cs:578`）⇒ `SendUpdateMsg(RM_WEIGHTCHANGED)`（排队 131，同 ident 覆盖）。
fn weight_changed(side: &mut SellOffSide<'_>, em: &mut Emitter) {
    em.update(
        PRIO_RM_WEIGHTCHANGED,
        side.id,
        messages::RM_WEIGHTCHANGED,
        SellOffFrame::WeightChanged {
            target: side.id,
            weight: side.weight,
            wear_weight: side.wear_weight,
            hand_weight: side.hand_weight,
        },
    );
}

/// `GameGoldChanged()`（`BaseObject.cs:593`）⇒ `SendUpdateMsg(RM_GAMEGOLDCHANGED)`（排队 148，同 ident 覆盖）。
fn game_gold_changed(side: &mut SellOffSide<'_>, em: &mut Emitter) {
    if !side.send_game_gold_info {
        return;
    }
    em.update(
        PRIO_RM_GAMEGOLDCHANGED,
        side.id,
        KEY_RM_GAMEGOLDCHANGED,
        SellOffFrame::GameGoldChanged {
            target: side.id,
            game_gold: *side.game_gold,
            game_point: side.game_point,
        },
    );
}

/// `SendMsg(RM_MENU_OK, 0, ActorId, 0, 0, text)`（排队 69 ⇒ `SM_MENU_OK`，`Recog = ActorId`）。
fn menu_ok(em: &mut Emitter, target: i32, text: &str) {
    em.later(
        PRIO_RM_MENU_OK,
        SellOffFrame::MenuOk {
            target,
            recog: target,
            text: text.to_string(),
        },
    );
}

/// `SysMsg(text, MsgColor.Blue, MsgType.Hint)`（`BaseObject.cs:1588+`）⇒ 排队 `RM_SYSMESSAGE(252)`。
fn blue_msg(em: &mut Emitter, target: i32, cfg: &SellOffConfig, text: &str) {
    em.later(
        PRIO_RM_SYSMESSAGE,
        SellOffFrame::SysMsg {
            target,
            recog: target,
            f_color: cfg.blue_f_color,
            b_color: cfg.blue_b_color,
            text: text.to_string(),
        },
    );
}

/// `SendMsg(RM_*, 0, 0, 0, 0)`（排队；出队映射到 `sm_ident`）。
fn rm_empty(em: &mut Emitter, target: i32, priority: u8, sm_ident: u16) {
    em.later(
        priority,
        SellOffFrame::Def {
            target,
            ident: sm_ident,
            recog: 0,
            param: 0,
            tag: 0,
            series: 0,
            name: None,
        },
    );
}

/// `HUtil32.GetValidStr3(ref sItemName, ' ')`：`Split(' ', 2, RemoveEmptyEntries)` 取第一段。
#[must_use]
pub fn first_token(s: &str) -> &str {
    if !s.contains(' ') {
        return s;
    }
    s.split(' ').find(|p| !p.is_empty()).unwrap_or("")
}

/// `UpdateSellOffInfo(code)`（`PlayObject.Sale.cs:19-56`）。
///
/// 返回被移除条目的索引（升序）——宿主/调用方据此固定住自己持有的条目下标（oracle 用对象引用，
/// 不会因 `RemoveAt` 失效；Rust 以索引复刻等价语义）。`code`：0 = 出售者，1 = 购买者。
/// 注意 oracle 此处用 `==`（**大小写敏感**），其余函数用 `OrdinalIgnoreCase`。
#[must_use]
pub fn update_sell_off_info(
    side: &mut SellOffSide<'_>,
    book: &mut SellOffBook,
    code: i32,
) -> Vec<usize> {
    let mut removed = Vec::new();
    if !side.state.sale_deal {
        return removed;
    }
    let mut i = book.entries.len();
    while i > 0 {
        i -= 1;
        if book.entries.is_empty() {
            break;
        }
        let e = &book.entries[i];
        if e.flag == 2 {
            let hit = match code {
                0 => e.deal_chr_name == side.name,
                1 => e.buy_chr_name == side.name,
                _ => false,
            };
            if hit {
                book.entries.remove(i);
                removed.push(i);
            }
        }
    }
    removed.sort_unstable();
    removed
}

/// `ClientAddSellOffItem(nItemIdx, sItemName)`（`PlayObject.Sale.cs:60-125`）。
pub fn client_add_sell_off_item(
    me: &mut SellOffSide<'_>,
    catalog: &dyn ItemCatalog,
    n_item_idx: i32,
    s_item_name: &str,
) -> Vec<SellOffEvent> {
    let mut em = Emitter::new();
    if n_item_idx <= 0 || s_item_name.is_empty() {
        rm_empty(
            &mut em,
            me.id,
            PRIO_RM_SELLOFFADDITEM_FAIL,
            messages::SM_SellOffADDITEM_FAIL,
        );
        return em.finish();
    }
    let want = if s_item_name.contains(' ') {
        first_token(s_item_name)
    } else {
        s_item_name
    };
    let mut s_user_item_name = String::new();
    let mut ok = false;
    if !me.state.sell_off_confirm {
        let mut i = me.items.len();
        while i > 0 {
            i -= 1;
            if me.items.is_empty() {
                break;
            }
            if me.items[i].make_index != n_item_idx {
                continue;
            }
            // `Desc[13] == 1` ⇒ 自定义物品名（`GetCustomItemName`，未建模 ⇒ 空串，见模块头）。
            if me.items[i].desc[13] == 1 {
                s_user_item_name = String::new();
            }
            if s_user_item_name.is_empty() {
                s_user_item_name = item_display_name(catalog, &me.items[i]);
            }
            if s_user_item_name.eq_ignore_ascii_case(want)
                && me.state.sell_off_item_list.len() < SELL_OFF_TEMP_MAX
            {
                let item = me.items.remove(i);
                me.state.sell_off_item_list.push(item);
                rm_empty(
                    &mut em,
                    me.id,
                    PRIO_RM_SELLOFFADDITEM_OK,
                    messages::SM_SELLOFFADDITEM_OK,
                );
                ok = true;
                break;
            }
        }
    }
    if !ok {
        rm_empty(
            &mut em,
            me.id,
            PRIO_RM_SELLOFFADDITEM_FAIL,
            messages::SM_SellOffADDITEM_FAIL,
        );
    }
    em.finish()
}

/// `ClientDelSellOffItem(nItemIdx, sItemName)`（`PlayObject.Sale.cs:127-183`）。
pub fn client_del_sell_off_item(
    me: &mut SellOffSide<'_>,
    catalog: &dyn ItemCatalog,
    n_item_idx: i32,
    s_item_name: &str,
) -> Vec<SellOffEvent> {
    let mut em = Emitter::new();
    if n_item_idx <= 0 || s_item_name.is_empty() {
        rm_empty(
            &mut em,
            me.id,
            PRIO_RM_SELLOFFDELITEM_FAIL,
            messages::SM_SELLOFFDELITEM_FAIL,
        );
        return em.finish();
    }
    let want = if s_item_name.contains(' ') {
        first_token(s_item_name)
    } else {
        s_item_name
    };
    let mut ok = false;
    if !me.state.sell_off_confirm {
        let mut i = me.state.sell_off_item_list.len();
        while i > 0 {
            i -= 1;
            if me.state.sell_off_item_list.is_empty() {
                break;
            }
            if me.state.sell_off_item_list[i].make_index != n_item_idx {
                continue;
            }
            let mut s_user_item_name = String::new();
            if me.state.sell_off_item_list[i].desc[13] == 1 {
                s_user_item_name = String::new();
            }
            if s_user_item_name.is_empty() {
                s_user_item_name = item_display_name(catalog, &me.state.sell_off_item_list[i]);
            }
            if s_user_item_name.eq_ignore_ascii_case(want) {
                let item = me.state.sell_off_item_list.remove(i);
                me.items.push(item);
                rm_empty(
                    &mut em,
                    me.id,
                    PRIO_RM_SELLOFFDELITEM_OK,
                    messages::SM_SELLOFFDELITEM_OK,
                );
                ok = true;
                break;
            }
        }
    }
    if !ok {
        rm_empty(
            &mut em,
            me.id,
            PRIO_RM_SELLOFFDELITEM_FAIL,
            messages::SM_SELLOFFDELITEM_FAIL,
        );
    }
    em.finish()
}

/// `GetBackSellOffItems()`（`PlayObject.Sale.cs:602-617`）：临时列表**倒序**退回 `ItemList`，
/// 清 `SellOffConfirm`。
pub fn get_back_sell_off_items(me: &mut SellOffSide<'_>) {
    while let Some(item) = me.state.sell_off_item_list.pop() {
        me.items.push(item);
    }
    me.state.sell_off_confirm = false;
}

/// `SellOffCancel()`（`PlayObject.Sale.cs:593-597`）：`RM_SELLOFFCANCEL` + 退回物品。
pub fn sell_off_cancel(me: &mut SellOffSide<'_>) -> Vec<SellOffEvent> {
    let mut em = Emitter::new();
    rm_empty(
        &mut em,
        me.id,
        PRIO_RM_SELLOFFCANCEL,
        messages::SM_SellOffCANCEL,
    );
    get_back_sell_off_items(me);
    em.finish()
}

/// `ClientCancelSellOff()`（`PlayObject.Sale.cs:431-438`）→ [`sell_off_cancel`]。
pub fn client_cancel_sell_off(me: &mut SellOffSide<'_>) -> Vec<SellOffEvent> {
    sell_off_cancel(me)
}

/// `ClientCancelSellOffIng()`（`PlayObject.Sale.cs:185-252`）：出售人取消**正在寄售中**的交易。
///
/// oracle 前置：`SellOffItemList == null || Count == 0 || !IsEnoughBag()` ⇒ **直接 return（无帧）**。
pub fn client_cancel_sell_off_ing(
    me: &mut SellOffSide<'_>,
    book: &mut SellOffBook,
    catalog: &dyn ItemCatalog,
    cfg: &SellOffConfig,
) -> Vec<SellOffEvent> {
    let mut em = Emitter::new();
    if book.entries.is_empty() || !is_enough_bag(me.items, cfg.max_bag_item) {
        return em.finish();
    }
    let mut i = book.entries.len();
    while i > 0 {
        i -= 1;
        if book.entries.is_empty() {
            break;
        }
        let e = &book.entries[i];
        if !(e.deal_chr_name.eq_ignore_ascii_case(&me.name) && (e.flag == 0 || e.flag == 3)) {
            continue;
        }
        let entry = book.entries[i].clone();
        book.entries[i].flag = 4;
        for j in 0..DEAL_OFF_ITEMS {
            let Some(item) = entry.use_items.get(j).and_then(|x| x.as_ref()) else {
                continue;
            };
            match catalog.get(item.index) {
                Some(tpl) => {
                    if is_enough_bag(me.items, cfg.max_bag_item) {
                        if weight_available(me, i32::from(tpl.weight)) {
                            let item = item.clone();
                            if add_item_to_bag(me, item.clone(), cfg, &mut em) {
                                em.now(SellOffEvent::AddItem {
                                    target: me.id,
                                    item,
                                });
                            }
                        }
                    } else {
                        drop_item_down(item.clone());
                    }
                }
                // 金刚石：oracle 判 `short.MaxValue`（32767），但写入的是 `ushort.MaxValue`（65535）
                // ⇒ 会话内建条目在此永不为真（缺陷②）。
                None => {
                    if item.make_index > 0
                        && i32::from(item.index) == SHORT_MAX
                        && i32::from(item.dura) == SHORT_MAX
                        && i32::from(item.dura_max) == SHORT_MAX
                    {
                        *me.gold += item.make_index;
                        game_gold_changed(me, &mut em);
                    }
                }
            }
        }
        book.entries.remove(i);
        drop_entry(entry);
        menu_ok(&mut em, me.id, "取消寄售成功!");
    }
    em.finish()
}

/// `ClientBuyCancelSellOff(dealChrName)`（`PlayObject.Sale.cs:254-279`）。
pub fn client_buy_cancel_sell_off(
    me: &mut SellOffSide<'_>,
    book: &mut SellOffBook,
    deal_chr_name: &str,
) -> Vec<SellOffEvent> {
    let mut em = Emitter::new();
    let mut i = book.entries.len();
    while i > 0 {
        i -= 1;
        if book.entries.is_empty() {
            break;
        }
        let e = &book.entries[i];
        if e.deal_chr_name.eq_ignore_ascii_case(deal_chr_name)
            && e.buy_chr_name.eq_ignore_ascii_case(&me.name)
            && e.flag == 0
        {
            book.entries[i].flag = 3;
            menu_ok(&mut em, me.id, "取消交易成功!");
            break;
        }
    }
    em.finish()
}

/// `ClientBuySellOffItme(dealChrName)`（`PlayObject.Sale.cs:281-385`）——**成交**。
///
/// `seller` = `GetPlayObject(卖方名)` 的在线侧（`None` = 不在线）；`seller` 为 `Some` 且
/// `offline` 为 `true` 时同「不在线」处理（`Flag=1`）。oracle 物品循环 `for (int j = 0; j <= 9; j++)`
/// 在首个空槽（或下标 9 越界）处抛异常被 `catch` 吞掉（模块头缺陷①）⇒ 本函数在该点**中止**并返回
/// 已产出的帧 ⇒ `RM_SELLOFFBUY_OK`/`RM_MENU_OK` 不可达。
pub fn client_buy_sell_off_item(
    me: &mut SellOffSide<'_>,
    seller: Option<&mut SellOffSide<'_>>,
    book: &mut SellOffBook,
    catalog: &dyn ItemCatalog,
    cfg: &SellOffConfig,
    deal_chr_name: &str,
) -> Vec<SellOffEvent> {
    let mut em = Emitter::new();
    let Some(mut i) = book.entries.iter().rposition(|e| {
        e.deal_chr_name.eq_ignore_ascii_case(deal_chr_name)
            && e.buy_chr_name.eq_ignore_ascii_case(&me.name)
            && e.flag == 0
    }) else {
        return em.finish();
    };
    book.entries[i].flag = 4;
    if *me.game_gold >= book.entries[i].sell_gold + cfg.dec_user_game_gold {
        *me.game_gold -= book.entries[i].sell_gold + cfg.dec_user_game_gold;
        if *me.game_gold < 0 {
            *me.game_gold = 0;
        }
        game_gold_changed(me, &mut em);
        let mut seller_credited = false;
        match seller {
            None => {}
            Some(s) if s.offline => {}
            Some(s) => {
                let removed = update_sell_off_info(me, book, 1);
                let shift = removed.iter().filter(|r| **r < i).count();
                i -= shift;
                book.entries[i].flag = 2;
                *s.game_gold += book.entries[i].sell_gold;
                game_gold_changed(s, &mut em);
                seller_credited = true;
            }
        }
        if !seller_credited {
            book.entries[i].flag = 1;
        }
        let use_items = book.entries[i].use_items.clone();
        // `for (int j = 0; j <= 9; j++)`：首个 `None`（空槽）或下标 9 越界 ⇒ 中止。
        for j in 0..=DEAL_OFF_ITEMS {
            let Some(item) = use_items.get(j).and_then(|x| x.as_ref()) else {
                return em.finish();
            };
            match catalog.get(item.index) {
                Some(tpl) => {
                    if is_enough_bag(me.items, cfg.max_bag_item) {
                        if weight_available(me, i32::from(tpl.weight)) {
                            let item = item.clone();
                            if add_item_to_bag(me, item.clone(), cfg, &mut em) {
                                em.now(SellOffEvent::AddItem {
                                    target: me.id,
                                    item,
                                });
                            }
                        }
                    } else {
                        drop_item_down(item.clone());
                    }
                }
                None => {
                    if item.make_index > 0
                        && i32::from(item.index) == SHORT_MAX
                        && i32::from(item.dura) == SHORT_MAX
                        && i32::from(item.dura_max) == SHORT_MAX
                    {
                        *me.gold += item.make_index;
                        blue_msg(
                            &mut em,
                            me.id,
                            cfg,
                            &format!("{} 颗金刚石增加", item.make_index),
                        );
                    }
                }
            }
        }
        rm_empty(
            &mut em,
            me.id,
            PRIO_RM_SELLOFFBUY_OK,
            messages::SM_SELLOFFBUY_OK,
        );
        menu_ok(&mut em, me.id, "[成功] 系统已经成功接受您的申请");
    } else {
        book.entries[i].flag = 0;
        menu_ok(&mut em, me.id, "[错误] 您的申请提交不成功");
    }
    em.finish()
}

/// `IsAddWeightAvailable(nWeight)`（`PlayObject.Base.cs:191`）：`WAbil.Weight + nWeight <= MaxWeight`。
#[must_use]
fn weight_available(me: &SellOffSide<'_>, n_weight: i32) -> bool {
    i32::from(me.weight) + n_weight <= i32::from(me.max_weight)
}

/// `Dispose(DealOffInfo)`（`IDisposable` 语义）：本仓库无需释放，保留调用点语义。
fn drop_entry(_entry: DealOffInfo) {}

/// `ClientSellOffEnd(sBuyChrName, nSellGold, nGameDiamond, nCode)`（`PlayObject.Sale.cs:516-591`）——**上架**。
#[allow(clippy::too_many_arguments)]
pub fn client_sell_off_end(
    me: &mut SellOffSide<'_>,
    book: &mut SellOffBook,
    catalog: &dyn ItemCatalog,
    cfg: &SellOffConfig,
    now_ms: i64,
    s_buy_chr_name: &str,
    n_sell_gold: i32,
    n_game_diamond: i32,
    n_code: i32,
) -> Vec<SellOffEvent> {
    let mut em = Emitter::new();
    me.state.sell_off_confirm = true;
    let mut ok = false;
    if me.state.sell_off_confirm
        && (!me.state.sell_off_item_list.is_empty() || n_game_diamond > 0)
        && me.state.sell_off_item_list.len() < 10
        && s_buy_chr_name.chars().count() < 20
        && n_sell_gold > 0
        && n_sell_gold < 100_000_000
        && !s_buy_chr_name.eq_ignore_ascii_case(&me.name)
    {
        let mut deal = DealOffInfo::new_with_slots();
        if !me.state.sell_off_item_list.is_empty() {
            for i in 0..me.state.sell_off_item_list.len() {
                let user_item = &me.state.sell_off_item_list[i];
                let std_item = catalog.get(user_item.index);
                if std_item.is_some() && user_item.make_index > 0 {
                    deal.use_items[i] = Some(user_item.clone());
                }
            }
        }
        // 金刚石特判（缺陷②）：`GetStdItem == null` 的槽 —— 但会话内建的槽已由上面填过，
        // 且金刚石要求 `nCode == short.MaxValue`；此循环在 `UseItems[j] != null` 时才检查。
        let mut diamond_fail = false;
        for j in 0..SELL_OFF_TEMP_MAX {
            let Some(item) = deal.use_items[j].as_ref() else {
                continue;
            };
            let std_item = catalog.get(item.index);
            if std_item.is_none()
                && n_game_diamond > 0
                && n_game_diamond < 10_000
                && n_code == SHORT_MAX
            {
                if n_game_diamond > *me.gold {
                    rm_empty(
                        &mut em,
                        me.id,
                        PRIO_RM_SELLOFFEND_FAIL,
                        messages::SM_SELLOFFEND_FAIL,
                    );
                    menu_ok(&mut em, me.id, "[错误] 你没有那么多金币");
                    get_back_sell_off_items(me);
                    diamond_fail = true;
                    break;
                }
                *me.gold -= n_game_diamond;
                game_gold_changed(me, &mut em);
                let slot = deal.use_items[j].as_mut().expect("上面已确认非空");
                slot.make_index = n_game_diamond;
                slot.index = USHORT_MAX;
                slot.dura = USHORT_MAX;
                slot.dura_max = USHORT_MAX;
                break;
            }
        }
        if diamond_fail {
            return em.finish();
        }
        deal.deal_chr_name = me.name.clone();
        deal.buy_chr_name = s_buy_chr_name.trim().to_string();
        deal.sell_gold = n_sell_gold;
        deal.sell_datetime_ms = now_ms;
        deal.flag = 0;
        book.entries.push(deal);
        rm_empty(
            &mut em,
            me.id,
            PRIO_RM_SELLOFFEND_OK,
            messages::SM_SELLOFFEND_OK,
        );
        *me.game_gold -= cfg.dec_user_game_gold;
        if *me.game_gold < 0 {
            *me.game_gold = 0;
        }
        menu_ok(&mut em, me.id, "[成功] 系统已经成功接受您的申请");
        ok = true;
        me.state.sell_off_confirm = false;
        me.state.sell_off_item_list.clear();
    }
    if !ok {
        rm_empty(
            &mut em,
            me.id,
            PRIO_RM_SELLOFFEND_FAIL,
            messages::SM_SELLOFFEND_FAIL,
        );
        menu_ok(&mut em, me.id, "[错误:] 寄售物品失败");
        get_back_sell_off_items(me);
    }
    em.finish()
}

/// `GetSellOffGlod()`（`PlayObject.Sale.cs:387-429`）：上线时把 `Flag == 1`（已售未取款）结清。
///
/// 注意 oracle 先 `UpdateSellOffInfo(0)`（可能移除 `flag==2` 条目）再改当前条目 ⇒ 本函数用
/// 移除下标平移当前下标（等价于 oracle 的对象引用）。
pub fn get_sell_off_gold(me: &mut SellOffSide<'_>, book: &mut SellOffBook) -> Vec<SellOffEvent> {
    let mut em = Emitter::new();
    let Some(mut i) = book
        .entries
        .iter()
        .rposition(|e| e.deal_chr_name.eq_ignore_ascii_case(&me.name) && e.flag == 1)
    else {
        return em.finish();
    };
    let removed = update_sell_off_info(me, book, 0);
    let shift = removed.iter().filter(|r| **r < i).count();
    i -= shift;
    book.entries[i].flag = 2;
    *me.game_gold += book.entries[i].sell_gold;
    game_gold_changed(me, &mut em);
    em.finish()
}

/// `RM_SENDDEALOFFFORM(23000)` 的队列优先级字节（`(byte)23000 = 152`）。
const PRIO_RM_SENDDEALOFFFORM: u8 = (messages::RM_SENDDEALOFFFORM & 0xFF) as u8;

/// `SellOffInTime(nCode)`（`PlayObject.Sale.cs:475-514`）：查询玩家是否有进行中的寄售。
#[must_use]
pub fn sell_off_in_time(me: &SellOffSide<'_>, book: &SellOffBook, n_code: i32) -> bool {
    let mut result = false;
    if book.entries.is_empty() {
        return result;
    }
    for e in &book.entries {
        let hit = match n_code {
            0 => e.deal_chr_name.eq_ignore_ascii_case(&me.name) && (e.flag == 0 || e.flag == 3),
            1 => e.buy_chr_name.eq_ignore_ascii_case(&me.name) && e.flag == 0,
            _ => false,
        };
        if hit {
            result = true;
        }
    }
    result
}

/// `UserSelectOpenDealOffForm(user)`（`Merchant.cs:1038-1056`）的**成功支**帧：
/// `user.SendMsg(RM_SENDDEALOFFFORM, 0, ActorId, 0, 0)`（`ActorId` = 商人）⇒ `Run()` 出队时映射
/// `SM_SENDDEALOFFFORM(23001)`（`PlayObject.Message.cs:2046-2048`）。
///
/// 返回 `None` 表示 oracle 走 `RM_MERCHANTSAY` 失败支（未开通元宝服务 / 已有进行中的寄售）
/// （`Merchant.cs:1041-1056`）—— 那两路是 **商人说话帧**（`SM_MERCHANTSAY(643)`，
/// `ChrName + '/' + text`），由 merchant/NPC 层产出（[`crate::npc`]/[`crate::shop`]）。
///
/// **未接线**：生产触发点 = merchant 脚本 label `@dealybme`（`Merchant.cs:930-934`），在
/// `crates/script`（不在本卡写集）⇒ 本函数只落纯逻辑 + 帧，供脚本层接线时直接调用。
#[must_use]
pub fn open_deal_off_form(
    me: &SellOffSide<'_>,
    book: &SellOffBook,
    npc_id: i32,
) -> Option<SellOffEvent> {
    if !me.state.sale_deal || sell_off_in_time(me, book, 0) {
        return None;
    }
    let mut em = Emitter::new();
    em.later(
        PRIO_RM_SENDDEALOFFFORM,
        SellOffFrame::Def {
            target: me.id,
            ident: messages::SM_SENDDEALOFFFORM,
            recog: 0,
            param: npc_id as u16,
            tag: 0,
            series: 0,
            name: None,
        },
    );
    em.finish().into_iter().next()
}

/// `SelectSellDate()`（`PlayObject.Sale.cs:440-473`）的纯逻辑。
///
/// **无活调用点**（`NormNpc.cs:465/1388` 的 `ReplaceVariableText` 调用均被注释）⇒ 本仓库不接线。
/// `dSellDateTime` 用注入的 `sell_datetime_ms`，按 oracle `ToString("yyyy年mm月dd日 hh时ss分")`
/// 的本地时区口径**未**实现（登记）。
#[must_use]
pub fn select_sell_date(me: &SellOffSide<'_>, book: &SellOffBook, game_gold_name: &str) -> String {
    if !me.state.sale_deal {
        return format!("您未开通{game_gold_name}寄售服务,请先开通!!!\\ \\<返回/@main>");
    }
    for e in &book.entries {
        if e.deal_chr_name.eq_ignore_ascii_case(&me.name) && e.flag == 2 {
            return format!(
                "最后一笔出售记录:\\   {},\\  您与{}交易成功,获得了{}个{}。\\ \\<返回/@main>",
                format_sell_date(e.sell_datetime_ms),
                e.buy_chr_name,
                e.sell_gold,
                game_gold_name
            );
        }
        if e.buy_chr_name.eq_ignore_ascii_case(&me.name) && (e.flag == 1 || e.flag == 2) {
            return format!(
                "最后一笔购买记录:\\   {},\\  您与{}交易成功,支付了{}个{}。\\ \\<返回/@main>",
                format_sell_date(e.sell_datetime_ms),
                e.deal_chr_name,
                e.sell_gold,
                game_gold_name
            );
        }
    }
    "您未进行任何寄售交易!!!\\ \\<返回/@main>".to_string()
}

/// `dSellDateTime.ToString("yyyy年mm月dd日 hh时ss分")` 的占位（本地时区口径未实现，登记）。
fn format_sell_date(_ms: i64) -> String {
    String::new()
}

/// 把一条 [`SellOffEvent`] 编成 `(收件人 ActorId, GM_DATA body)`（纯函数，无 I/O）。
#[must_use]
pub fn selloff_event_body(ev: &SellOffEvent, catalog: &dyn ItemCatalog) -> Option<(i32, Vec<u8>)> {
    match ev {
        SellOffEvent::AddItem { target, item } => Some((
            *target,
            crate::trade::send_add_item_body(*target, item, catalog)?,
        )),
        SellOffEvent::Queued { body, .. } => match body {
            SellOffFrame::Def {
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
            SellOffFrame::MenuOk {
                target,
                recog,
                text,
            } => Some((*target, smframe::sm_menu_ok(*recog, &gbk(text)))),
            SellOffFrame::SysMsg {
                target,
                recog,
                f_color,
                b_color,
                text,
            } => Some((
                *target,
                smframe::sm_sysmessage(*recog, *f_color, *b_color, &gbk(text)),
            )),
            SellOffFrame::GameGoldChanged {
                target,
                game_gold,
                game_point,
            } => Some((
                *target,
                smframe::sm_gamegoldname(*game_gold, *game_point, &[]),
            )),
            SellOffFrame::WeightChanged {
                target,
                weight,
                wear_weight,
                hand_weight,
            } => Some((
                *target,
                smframe::sm_weightchanged(*weight, *wear_weight, *hand_weight),
            )),
        },
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
        offline: bool,
        items: Vec<BagItem>,
        gold: i32,
        game_gold: i32,
        game_point: i32,
        weight: u16,
        max_weight: u16,
        send_game_gold_info: bool,
        state: SellOffState,
    }

    impl P {
        fn new(id: i32, name: &str) -> Self {
            P {
                id,
                name: name.to_string(),
                offline: false,
                items: Vec::new(),
                gold: 0,
                game_gold: 0,
                game_point: 0,
                weight: 0,
                max_weight: 50,
                send_game_gold_info: true,
                state: SellOffState::default(),
            }
        }
    }

    fn side(p: &mut P) -> SellOffSide<'_> {
        SellOffSide {
            id: p.id,
            name: p.name.clone(),
            offline: p.offline,
            items: &mut p.items,
            gold: &mut p.gold,
            game_gold: &mut p.game_gold,
            game_point: p.game_point,
            weight: p.weight,
            wear_weight: 0,
            hand_weight: 0,
            max_weight: p.max_weight,
            send_game_gold_info: p.send_game_gold_info,
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

    fn entry(
        deal: &str,
        buy: &str,
        sell_gold: i32,
        flag: u8,
        use_items: Vec<Option<BagItem>>,
    ) -> DealOffInfo {
        DealOffInfo {
            deal_chr_name: deal.to_string(),
            buy_chr_name: buy.to_string(),
            sell_datetime_ms: 0,
            sell_gold,
            use_items,
            flag,
        }
    }

    fn slots(first: Option<BagItem>) -> Vec<Option<BagItem>> {
        let mut v = vec![None; DEAL_OFF_ITEMS];
        v[0] = first;
        v
    }

    /// 一条排队帧的出队后 `SM_*` ident（`MenuOk`/`SysMsg`/金/负重各归其位）。
    fn frame_ident(body: &SellOffFrame) -> u16 {
        match body {
            SellOffFrame::Def { ident, .. } => *ident,
            SellOffFrame::MenuOk { .. } => messages::SM_MENU_OK,
            SellOffFrame::SysMsg { .. } => messages::SM_SYSMESSAGE,
            SellOffFrame::GameGoldChanged { .. } => messages::SM_GAMEGOLDNAME,
            SellOffFrame::WeightChanged { .. } => messages::SM_WEIGHTCHANGED,
        }
    }

    /// 取排队帧里每条的出队 `SM_*` ident。
    fn queued_idents(evs: &[SellOffEvent]) -> Vec<u16> {
        evs.iter()
            .filter_map(|e| match e {
                SellOffEvent::Queued { body, .. } => Some(frame_ident(body)),
                _ => None,
            })
            .collect()
    }

    fn has_menu_ok(evs: &[SellOffEvent], text: &str) -> bool {
        evs.iter().any(|e| {
            matches!(
                e,
                SellOffEvent::Queued {
                    body: SellOffFrame::MenuOk { text: t, .. },
                    ..
                } if t == text
            )
        })
    }

    // ---- ClientAddSellOffItem / ClientDelSellOffItem ----

    #[test]
    fn add_item_ok_moves_to_temp_and_replies() {
        let cat = catalog();
        let mut m = P::new(1, "seller");
        m.items.push(bag(1, 100));
        let evs = client_add_sell_off_item(&mut side(&mut m), &cat, 100, "木剑");
        assert!(m.items.is_empty(), "物品应移入临时列表");
        assert_eq!(m.state.sell_off_item_list.len(), 1);
        assert_eq!(queued_idents(&evs), vec![messages::SM_SELLOFFADDITEM_OK]);
    }

    #[test]
    fn add_item_wrong_name_fails() {
        let cat = catalog();
        let mut m = P::new(1, "seller");
        m.items.push(bag(1, 100));
        let evs = client_add_sell_off_item(&mut side(&mut m), &cat, 100, "铁剑");
        assert_eq!(m.items.len(), 1);
        assert!(m.state.sell_off_item_list.is_empty());
        assert_eq!(queued_idents(&evs), vec![messages::SM_SellOffADDITEM_FAIL]);
    }

    #[test]
    fn add_item_blocked_while_confirming() {
        let cat = catalog();
        let mut m = P::new(1, "seller");
        m.items.push(bag(1, 100));
        m.state.sell_off_confirm = true;
        let evs = client_add_sell_off_item(&mut side(&mut m), &cat, 100, "木剑");
        assert_eq!(queued_idents(&evs), vec![messages::SM_SellOffADDITEM_FAIL]);
    }

    #[test]
    fn del_item_ok_moves_back_to_bag() {
        let cat = catalog();
        let mut m = P::new(1, "seller");
        m.state.sell_off_item_list.push(bag(1, 100));
        let evs = client_del_sell_off_item(&mut side(&mut m), &cat, 100, "木剑");
        assert_eq!(m.items.len(), 1);
        assert!(m.state.sell_off_item_list.is_empty());
        assert_eq!(queued_idents(&evs), vec![messages::SM_SELLOFFDELITEM_OK]);
    }

    // ---- ClientSellOffEnd（上架） ----

    #[test]
    fn sell_off_end_success_lists_and_charges_fee() {
        let cat = catalog();
        let cfg = SellOffConfig::default();
        let mut book = SellOffBook::default();
        let mut m = P::new(1, "seller");
        m.game_gold = 100_000;
        m.state.sell_off_item_list.push(bag(1, 100));
        let evs = client_sell_off_end(
            &mut side(&mut m),
            &mut book,
            &cat,
            &cfg,
            1234,
            "buyer",
            500,
            0,
            0,
        );
        assert_eq!(book.entries.len(), 1);
        let e = &book.entries[0];
        assert_eq!(e.flag, 0);
        assert_eq!(e.deal_chr_name, "seller");
        assert_eq!(e.buy_chr_name, "buyer");
        assert_eq!(e.sell_gold, 500);
        assert_eq!(e.sell_datetime_ms, 1234);
        assert!(e.use_items[0].is_some());
        assert!(e.use_items[1..].iter().all(Option::is_none));
        assert_eq!(m.game_gold, 90_000, "手续费 DecUserGameGold=10000");
        assert!(m.state.sell_off_item_list.is_empty());
        assert!(!m.state.sell_off_confirm);
        // 排队帧序：MENU_OK(69) → SELLOFFEND_OK(233)。
        assert_eq!(
            queued_idents(&evs),
            vec![messages::SM_MENU_OK, messages::SM_SELLOFFEND_OK]
        );
    }

    #[test]
    fn sell_off_end_rejects_self_and_returns_items() {
        let cat = catalog();
        let cfg = SellOffConfig::default();
        let mut book = SellOffBook::default();
        let mut m = P::new(1, "seller");
        m.game_gold = 100_000;
        m.state.sell_off_item_list.push(bag(1, 100));
        let evs = client_sell_off_end(
            &mut side(&mut m),
            &mut book,
            &cat,
            &cfg,
            0,
            "Seller",
            500,
            0,
            0,
        );
        assert!(book.entries.is_empty());
        assert_eq!(m.items.len(), 1, "失败应退回物品");
        assert_eq!(m.game_gold, 100_000, "失败不扣手续费");
        assert_eq!(
            queued_idents(&evs),
            vec![messages::SM_MENU_OK, messages::SM_SELLOFFEND_FAIL]
        );
        assert!(has_menu_ok(&evs, "[错误:] 寄售物品失败"));
    }

    #[test]
    fn sell_off_end_rejects_sell_gold_out_of_range() {
        let cat = catalog();
        let cfg = SellOffConfig::default();
        let mut book = SellOffBook::default();
        let mut m = P::new(1, "seller");
        m.state.sell_off_item_list.push(bag(1, 100));
        let evs = client_sell_off_end(
            &mut side(&mut m),
            &mut book,
            &cat,
            &cfg,
            0,
            "buyer",
            100_000_000,
            0,
            0,
        );
        assert!(book.entries.is_empty());
        assert_eq!(m.items.len(), 1);
        assert!(has_menu_ok(&evs, "[错误:] 寄售物品失败"));
    }

    // ---- ClientBuySellOffItme（成交） ----

    #[test]
    fn buy_single_item_aborts_at_first_null_slot() {
        let cat = catalog();
        let cfg = SellOffConfig::default();
        let mut book = SellOffBook::default();
        book.entries
            .push(entry("seller", "buyer", 500, 0, slots(Some(bag(1, 100)))));
        let mut buyer = P::new(2, "buyer");
        buyer.game_gold = 100_000;
        let mut seller = P::new(1, "seller");
        let evs = client_buy_sell_off_item(
            &mut side(&mut buyer),
            Some(&mut side(&mut seller)),
            &mut book,
            &cat,
            &cfg,
            "seller",
        );
        assert_eq!(buyer.game_gold, 89_500, "买方扣 500+10000");
        assert_eq!(seller.game_gold, 500, "卖方 +500");
        assert_eq!(book.entries[0].flag, 2);
        assert_eq!(buyer.items.len(), 1, "物品 0 在抛点前已入包");
        // 缺陷①：首个空槽中止 ⇒ RM_SELLOFFBUY_OK / 成功 MENU_OK 不可达。
        assert!(
            !queued_idents(&evs).contains(&messages::SM_SELLOFFBUY_OK),
            "oracle j<=9 空引用 ⇒ BUY_OK 不可达，实得 {:?}",
            queued_idents(&evs)
        );
        assert!(!has_menu_ok(&evs, "[成功] 系统已经成功接受您的申请"));
        assert!(matches!(evs.first(), Some(SellOffEvent::AddItem { .. })));
    }

    #[test]
    fn buy_nine_items_still_aborts_at_out_of_range() {
        let cat = catalog();
        let cfg = SellOffConfig::default();
        let mut book = SellOffBook::default();
        let use_items = vec![Some(bag(1, 100)); DEAL_OFF_ITEMS];
        book.entries
            .push(entry("seller", "buyer", 500, 0, use_items));
        let mut buyer = P::new(2, "buyer");
        buyer.game_gold = 100_000;
        let mut seller = P::new(1, "seller");
        let evs = client_buy_sell_off_item(
            &mut side(&mut buyer),
            Some(&mut side(&mut seller)),
            &mut book,
            &cat,
            &cfg,
            "seller",
        );
        assert!(!queued_idents(&evs).contains(&messages::SM_SELLOFFBUY_OK));
        assert_eq!(buyer.items.len(), DEAL_OFF_ITEMS);
    }

    #[test]
    fn buy_insufficient_game_gold_only_menu_ok() {
        let cat = catalog();
        let cfg = SellOffConfig::default();
        let mut book = SellOffBook::default();
        book.entries
            .push(entry("seller", "buyer", 500, 0, slots(Some(bag(1, 100)))));
        let mut buyer = P::new(2, "buyer");
        buyer.game_gold = 100;
        let mut seller = P::new(1, "seller");
        let evs = client_buy_sell_off_item(
            &mut side(&mut buyer),
            Some(&mut side(&mut seller)),
            &mut book,
            &cat,
            &cfg,
            "seller",
        );
        assert_eq!(buyer.game_gold, 100, "钱不够不扣款");
        assert_eq!(book.entries[0].flag, 0, "flag 复位 0");
        assert_eq!(seller.game_gold, 0);
        assert_eq!(queued_idents(&evs), vec![messages::SM_MENU_OK]);
        assert!(has_menu_ok(&evs, "[错误] 您的申请提交不成功"));
    }

    #[test]
    fn buy_with_offline_seller_sets_flag1() {
        let cat = catalog();
        let cfg = SellOffConfig::default();
        let mut book = SellOffBook::default();
        book.entries
            .push(entry("seller", "buyer", 500, 0, slots(Some(bag(1, 100)))));
        let mut buyer = P::new(2, "buyer");
        buyer.game_gold = 100_000;
        let mut seller = P::new(1, "seller");
        seller.offline = true;
        let evs = client_buy_sell_off_item(
            &mut side(&mut buyer),
            Some(&mut side(&mut seller)),
            &mut book,
            &cat,
            &cfg,
            "seller",
        );
        assert_eq!(buyer.game_gold, 89_500);
        assert_eq!(seller.game_gold, 0, "挂机卖方不即时结款");
        assert_eq!(book.entries[0].flag, 1);
        assert!(!queued_idents(&evs).contains(&messages::SM_SELLOFFBUY_OK));
    }

    #[test]
    fn buy_with_absent_seller_sets_flag1() {
        let cat = catalog();
        let cfg = SellOffConfig::default();
        let mut book = SellOffBook::default();
        book.entries
            .push(entry("seller", "buyer", 500, 0, slots(Some(bag(1, 100)))));
        let mut buyer = P::new(2, "buyer");
        buyer.game_gold = 100_000;
        let _ =
            client_buy_sell_off_item(&mut side(&mut buyer), None, &mut book, &cat, &cfg, "seller");
        assert_eq!(book.entries[0].flag, 1);
        assert_eq!(buyer.game_gold, 89_500);
    }

    #[test]
    fn buy_cancel_sets_flag3() {
        let mut book = SellOffBook::default();
        book.entries
            .push(entry("seller", "buyer", 500, 0, slots(None)));
        let mut buyer = P::new(2, "buyer");
        let evs = client_buy_cancel_sell_off(&mut side(&mut buyer), &mut book, "Seller");
        assert_eq!(book.entries[0].flag, 3);
        assert!(has_menu_ok(&evs, "取消交易成功!"));
    }

    // ---- 卖方取消正在寄售中 / 退回 ----

    #[test]
    fn cancel_selling_returns_items_and_removes_entry() {
        let cat = catalog();
        let cfg = SellOffConfig::default();
        let mut book = SellOffBook::default();
        book.entries
            .push(entry("seller", "buyer", 500, 0, slots(Some(bag(1, 100)))));
        let mut m = P::new(1, "seller");
        let evs = client_cancel_sell_off_ing(&mut side(&mut m), &mut book, &cat, &cfg);
        assert!(book.entries.is_empty());
        assert_eq!(m.items.len(), 1);
        assert!(matches!(evs.first(), Some(SellOffEvent::AddItem { .. })));
        assert!(has_menu_ok(&evs, "取消寄售成功!"));
    }

    #[test]
    fn cancel_selling_no_entry_emits_nothing() {
        let cat = catalog();
        let cfg = SellOffConfig::default();
        let mut book = SellOffBook::default();
        book.entries
            .push(entry("other", "buyer", 500, 0, slots(None)));
        let mut m = P::new(1, "seller");
        let evs = client_cancel_sell_off_ing(&mut side(&mut m), &mut book, &cat, &cfg);
        assert!(evs.is_empty());
        assert_eq!(book.entries.len(), 1);
    }

    #[test]
    fn cancel_selling_requires_empty_bag_slot() {
        let cat = catalog();
        let cfg = SellOffConfig {
            max_bag_item: 1,
            ..SellOffConfig::default()
        };
        let mut book = SellOffBook::default();
        book.entries
            .push(entry("seller", "buyer", 500, 0, slots(None)));
        let mut m = P::new(1, "seller");
        m.items.push(bag(2, 1)); // 背包已满（1/1）
        let evs = client_cancel_sell_off_ing(&mut side(&mut m), &mut book, &cat, &cfg);
        assert!(evs.is_empty(), "背包满 ⇒ oracle 直接 return（无帧）");
        assert_eq!(book.entries.len(), 1);
    }

    #[test]
    fn sell_off_cancel_returns_temp_in_reverse() {
        let mut m = P::new(1, "seller");
        m.state.sell_off_item_list.push(bag(1, 1));
        m.state.sell_off_item_list.push(bag(2, 2));
        m.state.sell_off_confirm = true;
        let evs = sell_off_cancel(&mut side(&mut m));
        assert_eq!(queued_idents(&evs), vec![messages::SM_SellOffCANCEL]);
        assert_eq!(
            m.items.iter().map(|b| b.make_index).collect::<Vec<_>>(),
            vec![2, 1],
            "GetBackSellOffItems 倒序退回"
        );
        assert!(!m.state.sell_off_confirm);
    }

    // ---- GetSellOffGlod / SellOffInTime / UpdateSellOffInfo ----

    #[test]
    fn get_sell_off_gold_credits_and_removes_prior_flag2() {
        let mut book = SellOffBook::default();
        book.entries.push(entry("seller", "x", 100, 2, slots(None)));
        book.entries.push(entry("seller", "y", 700, 1, slots(None)));
        let mut m = P::new(1, "seller");
        m.state.sale_deal = true;
        m.game_gold = 10;
        let evs = get_sell_off_gold(&mut side(&mut m), &mut book);
        assert_eq!(book.entries.len(), 1);
        assert_eq!(book.entries[0].flag, 2);
        assert_eq!(m.game_gold, 710);
        assert!(!evs.is_empty(), "应发 GameGoldChanged");
    }

    #[test]
    fn get_sell_off_gold_credits_without_sale_deal_but_update_is_noop() {
        // oracle `GetSellOffGlod` 本体**不**查 `SaleDeal`（只有内层 `UpdateSellOffInfo` 查）
        // ⇒ 未开通元宝服务也会结款；`UpdateSellOffInfo(0)` 为 no-op（不清旧 `flag==2` 条目）。
        let mut book = SellOffBook::default();
        book.entries.push(entry("seller", "x", 100, 2, slots(None)));
        book.entries.push(entry("seller", "y", 700, 1, slots(None)));
        let mut m = P::new(1, "seller");
        m.game_gold = 10;
        let evs = get_sell_off_gold(&mut side(&mut m), &mut book);
        assert!(!evs.is_empty(), "应发 GameGoldChanged");
        assert_eq!(m.game_gold, 710);
        assert_eq!(
            book.entries.len(),
            2,
            "SaleDeal=false ⇒ UpdateSellOffInfo no-op"
        );
        assert_eq!(book.entries[1].flag, 2);
    }

    #[test]
    fn update_sell_off_info_is_case_sensitive() {
        let mut book = SellOffBook::default();
        book.entries.push(entry("Seller", "b", 1, 2, slots(None)));
        let mut m = P::new(1, "seller");
        m.state.sale_deal = true;
        let removed = update_sell_off_info(&mut side(&mut m), &mut book, 0);
        assert!(removed.is_empty(), "oracle 此处用 ==（大小写敏感）");
        assert_eq!(book.entries.len(), 1);
    }

    #[test]
    fn sell_off_in_time_variants() {
        let mut book = SellOffBook::default();
        book.entries
            .push(entry("seller", "buyer", 1, 0, slots(None)));
        let mut seller = P::new(1, "seller");
        assert!(sell_off_in_time(&side(&mut seller), &book, 0));
        assert!(!sell_off_in_time(&side(&mut seller), &book, 1));
        let mut buyer = P::new(2, "buyer");
        assert!(sell_off_in_time(&side(&mut buyer), &book, 1));
    }

    /// **`RM_SENDDEALOFFFORM` 的成功支**（`Merchant.cs:1038-1045`）：未开通 ⇒ `None`；
    /// 开通且无进行中的寄售 ⇒ `SM_SENDDEALOFFFORM(23001)` 排队帧（`param` = 商人 `ActorId`）；
    /// 已有进行中 ⇒ `None`（或oracle 回商人说话帧）。
    #[test]
    fn open_deal_off_form_gated_by_sale_deal_and_active_sell() {
        let mut book = SellOffBook::default();
        let mut m = P::new(1, "seller");
        assert!(
            open_deal_off_form(&side(&mut m), &book, 42).is_none(),
            "未开通元宝服务 ⇒ 不开窗"
        );
        m.state.sale_deal = true;
        let ev = open_deal_off_form(&side(&mut m), &book, 42).expect("应发 SM_SENDDEALOFFFORM");
        match ev {
            SellOffEvent::Queued {
                priority,
                body:
                    SellOffFrame::Def {
                        ident,
                        param,
                        recog,
                        ..
                    },
                ..
            } => {
                assert_eq!(
                    priority, PRIO_RM_SENDDEALOFFFORM,
                    "优先级字节 23000&0xFF=152"
                );
                assert_eq!(ident, messages::SM_SENDDEALOFFFORM);
                assert_eq!(param, 42, "param = 商人 ActorId");
                assert_eq!(recog, 0);
            }
            other => panic!("应为排队 Def 帧，实得 {other:?}"),
        }
        book.entries.push(entry("seller", "x", 1, 0, slots(None)));
        assert!(
            open_deal_off_form(&side(&mut m), &book, 42).is_none(),
            "已有进行中的寄售 ⇒ 不开窗（回商人说话帧）"
        );
    }

    // ---- 帧编码 ----

    #[test]
    fn event_body_encodes_menu_ok_and_gamemoney_frames() {
        let cat = catalog();
        let evs = vec![
            SellOffEvent::Queued {
                priority: PRIO_RM_MENU_OK,
                key: None,
                body: SellOffFrame::MenuOk {
                    target: 7,
                    recog: 7,
                    text: "取消交易成功!".to_string(),
                },
            },
            SellOffEvent::Queued {
                priority: PRIO_RM_GAMEGOLDCHANGED,
                key: None,
                body: SellOffFrame::GameGoldChanged {
                    target: 7,
                    game_gold: 500,
                    game_point: 0,
                },
            },
        ];
        for ev in &evs {
            let (target, body) = selloff_event_body(ev, &cat).expect("应可编码");
            assert_eq!(target, 7);
            assert!(!body.is_empty());
        }
    }
}
