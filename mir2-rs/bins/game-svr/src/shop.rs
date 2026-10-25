//! NPC/商人商店动作的**纯函数接缝**（卡 `openmir2-m3-shop-dialog-wiring`，M3⑤-实现 4 第①片）。
//!
//! 本片覆盖 oracle 的「对话面板 + 查询售价 + 卖出 + 买入 + 查询修理成本 + 修理」六条链，
//! 逐行对齐：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`CM_MERCHANTDLGSELECT`] → [`user_select`] | `PlayObject.Operate.cs:114`（`ClientMerchantDlgSelect`）+ `Merchant.cs:787`（`UserSelect`）|
//! | [`CM_MERCHANTQUERYSELLPRICE`] → [`query_sell_price`] | `PlayObject.Operate.cs:131` + `Merchant.cs:1345` |
//! | [`CM_USERSELLITEM`] → [`sell_item`] | `PlayObject.Operate.cs:162` + `Merchant.cs:1377`（`AddItemToGoodsList` `:1417`）|
//! | [`CM_USERBUYITEM`] → [`buy_item`] | `PlayObject.Operate.cs:192` + `Merchant.cs:1219` |
//! | [`CM_MERCHANTQUERYREPAIRCOST`] → [`query_repair_cost`] | `PlayObject.Operate.cs:1558` + `Merchant.cs:1555` |
//! | [`CM_USERREPAIRITEM`] → [`repair_item`] | `PlayObject.Operate.cs:1585` + `Merchant.cs:1599` |
//!
//! 取价/修理数值口径复用已合入的 `mir2_formula::shop`（卡 `openmir2-m3-shop-price-formula`，
//! 含 `BUG-REPLICA`）；本模块只做状态迁移 + 帧面 + 分支（**不**重写公式）。
//!
//! # 第②片（本片）
//! - `CM_USERGETDETAILITEM(1015)`（`Merchant.ClientGetDetailGoodsList`，`Merchant.cs:1304`）+ `SM_SENDDETAILGOODSLIST(652)`
//!   ⇒ [`detail_goods_list`]（尾 = `EncodeString(每件 EncodeBuffer(ClientItem) + '/')`，与 `msg.Ident` 的 RM→SM 双重编码口径）。
//! - `CM_USERSTORAGEITEM(1031)` / `CM_USERTAKEBACKSTORAGEITEM(1032)`（`PlayObject.Operate.cs:1607-1713`）
//!   ⇒ [`storage_item`] / [`take_back_storage_item`]；`@storage`/`@getback` 面板 ⇒ [`back_item_list`] 的
//!   `SM_SENDUSERSTORAGEITEM(700)` / `SM_SAVEITEMLIST(704)` 分页（每页 50，`PlayObject.Base.cs:1590`）。
//!
//! # 本片仍登记（不静默跳过，见 `tests/parity/whitelist.md` T-47）
//! - `PayMent == 1 && !Config.TryModeUseStorage` 门（`Operate.cs:1615/1664`）：部署 `TestServer=TRUE`
//!   （`E:\\MirServer\\M2GameSvr\\Server.conf:11`）⇒ oracle `PlayObject.cs:1168-1171` 把 `PayMent` 置 **3**
//!   ⇒ 该门在本部署**恒假**；本宿主未把 `PayMent` 落到会话（登记，行为暂等价）。
//! - `SM_WEIGHTCHANGED`（`BaseObject.cs:1771`）：**买入路径已发**（[`weight_changed_frame`]，
//!   卡 `openmir2-merchant-price-list-and-weight-frame` B 阶段）；`make_drug_item` /
//!   `take_back_storage_item` 仍缺（T-47 ⑤ 残留）。`SM_ADDITEM` 尾（T-24）。
//! - `CustomItemSystem.GetItemName`/`GetUpgradeStdItem` 的物品展示层（T-47 ⑥/T-24）：
//!   **已实现**——名称走 [`crate::bag::item_display_name`]（`Desc[13]==1` ⇒ 自定义名），
//!   升级差量由 [`crate::itemwire::encode_client_item`] 统一叠加。详情列表照 oracle
//!   （`Merchant.cs:1327`）**不**取自定义名（模板名），仓库列表照 oracle（`BaseObject.cs:1613`）取。
//!
//! # 已落地（卡 `openmir2-merchant-script-load`）
//!
//! `Market_Def` 商人脚本装载（`Merchant.LoadMerchantScript`，`Merchant.cs:1096-1102`；`crates/envir::MapLoader::load_merchants_loaded` 在装载期一次解析，`boFlag=true`）与 `GotoLable` 执行（`~@repair`/`~@s_repair`、`~@upgradenow_*`、`~@getbackupgnow_*`）：[`RepairResult::goto_label`] / [`UpgradeOutcome::goto_label`] 只回**标签**（本模块是纯函数接缝，不跑引擎），宿主 `lib.rs` 的 `handle_shop_cmd` 用 `MerchantScriptBook::goto_label` 执行并产出 `SM_MERCHANTSAY(643)` 说词帧 + 脚本帧。
//!
//! # 已接线（卡 `openmir2-castle-merchant-tax-wiring`）
//! - 城堡税 `Castle.IncRateGold`/`CastleMgr.IncRateGold`（`Merchant.cs:546/1257/1392/1644`）：
//!   **已接线**（卡 `openmir2-castle-merchant-tax-wiring`）：四个交易点返回 `tax: MerchantTaxIntent`，
//!   宿主用 [`apply_merchant_tax`] 入账（Castle 目标入该城；CastleManager 目标升序入全部城）。
//!
//! # 三条从 ⑤-3 转入的副作用（owner comment `270a3919`）
//! 1. `@Superrepairfail` 的 `SendMsgToUser("对不起!我不能帮你修理这个物品。\\ \\ \\<返回/@main>")`
//!    （`Merchant.cs:1612`）：本模块产出 `SM_MERCHANTSAY(643)` 帧（走 [`RepairResult::sent_to_user`] 字段；
//!    尾 = `ChrName + '/' + sMsg`，锚 `NormNpc.cs:1613-1616` + `PlayObject.Message.cs:1722-1724`）。
//! 2. 城堡税：条件/目的地/税基照抄；本模块回意图，宿主入账（[`apply_merchant_tax`]）。
//! 3. `GotoLable(REPAIROK/SUPERREPAIROK)`：回 [`RepairResult::goto_label`]，宿主执行（卡 `openmir2-merchant-script-load`）。
//!
//! 另 reviewer-5 的 **O-1**：`ScriptLable` 与 `@s_repair` 的比较是 `string.Compare(..., OrdinalIgnoreCase)`，
//! 本模块用 [`eq_label`]（ASCII 大小写不敏感）同口径。

use mir2_data::models::StdItem;
use mir2_envir::makeitem::MakeItemRegistry;
use mir2_formula::shop::{self, ItemPrice, PriceTables, ShopConfig, StdItemSource};
use mir2_world::castle::UserCastle;
use mir2_world::castle_tax::{
    merchant_tax_intent, CastleTaxConfig, IncRateGoldOutcome, MerchantTaxIntent, MerchantTaxTarget,
};
use mir2_world::merchant::{
    GetBackupgWeaponOutcome, MerchantFlags, MerchantItem, UpgradeWaponOutcome,
};

use crate::ability::{Ability, ItemCatalog};
use crate::bag::{
    add_item_to_bag, add_item_to_bag_with_sink, BagItem, WeightChanged, WeightSink, MAX_BAG_ITEM,
};
use crate::downlink;
use crate::ground::GroundState;
use crate::itemwire::{encode_client_item, ClientItem};
use crate::smframe;

/// `PlayObject.LableIsCanJmp`（`PlayObject.Base.cs:2814-2829`）——**单一实现**：
/// 引擎 `mir2_script::engine::lable_is_can_jmp(can_jmp_labels, play_dice_label, label)`。
/// 本模块只做 re-export，避免 `engine/mod.rs` 与 `shop.rs` 两份口径漂移（T-142 ⑪）。
pub use mir2_script::engine::lable_is_can_jmp;

pub const CM_MERCHANTDLGSELECT: u16 = 1011;
pub const CM_MERCHANTQUERYSELLPRICE: u16 = 1012;
pub const CM_USERSELLITEM: u16 = 1013;
pub const CM_USERBUYITEM: u16 = 1014;
pub const CM_USERGETDETAILITEM: u16 = 1015;
pub const CM_USERREPAIRITEM: u16 = 1023;
pub const CM_MERCHANTQUERYREPAIRCOST: u16 = 1024;
pub const CM_USERSTORAGEITEM: u16 = 1031;
pub const CM_USERTAKEBACKSTORAGEITEM: u16 = 1032;
/// `CM_USERMAKEDRUGITEM`（`src/OpenMir2/Messages.cs:92`）——`PlayObject.Message.cs:972` →
/// `ClientMakeDrugItem(processMsg.nParam1, processMsg.Msg)`（制药片 WB）。
pub const CM_USERMAKEDRUGITEM: u16 = 1034;

/// `ScriptFlagConst`（`src/Modules/SystemModule/Const/ScriptFlagConst.cs`）本片用到的子集。
pub const LABEL_BUY: &str = "@buy";
pub const LABEL_SELL: &str = "@sell";
pub const LABEL_REPAIR: &str = "@repair";
pub const LABEL_SUPER_REPAIR: &str = "@s_repair";
pub const LABEL_SUPER_REPAIR_FAIL: &str = "@fail_s_repair";
pub const LABEL_REPAIR_OK: &str = "~@repair";
pub const LABEL_SUPER_REPAIR_OK: &str = "~@s_repair";
pub const LABEL_STORAGE: &str = "@storage";
pub const LABEL_GETBACK: &str = "@getback";
/// `ScriptFlagConst.sMAKEDURG`（`src/Modules/SystemModule/Const/ScriptFlagConst.cs:48`）：
/// `Merchant.UserSelect`（`Merchant.cs:857-860`）的 `@makedrug` 支 ⇒ `UserSelectMakeDurg`。
pub const LABEL_MAKE_DRUG: &str = "@makedrug";

/// `MessageSettings.CanotGetItems` / 修理失败文案（`Merchant.cs:1612`）；GBK 由 `downlink::gbk` 编码。
pub const SUPER_REPAIR_FAIL_MSG: &str = "对不起!我不能帮你修理这个物品。\\ \\ \\<返回/@main>";

/// `MessageSettings.CanotGetItems`（`MessageSettings.cs:217`；`Operate.cs:1704` 的取回负重不足分支）。
pub const CANOT_GET_ITEMS_MSG: &str = "无法携带更多的东西!!!";

/// 一条出帧（`MakeMessage` 后的 4 个数据字段 + 可选尾）；宿主用 `smframe::command_header` 落 12B 头。
///
/// `tail` = **未编码原文**（GBK 字节，由 `downlink::gbk` 编），线上尾的 `EDCode.EncodeString`
/// 在 `lib.rs::emit_shop_frames` 统一施加（口径锚：`PlayObject.Socket.cs:26-43` 的 `SendDefMessage`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShopFrame {
    pub ident: u16,
    pub recog: i32,
    pub param: u16,
    pub tag: u16,
    pub series: u16,
    pub tail: Vec<u8>,
    /// `true` ⇒ `tail` **已是线上尾**（`SendSocket(ClientMsg, sMsg)` 直连口径：`EncodeBuffer`/
    /// `SM_SAVEITEMLIST` 分页拼接），`emit_shop_frames` **不再**施加 `EncodeString`；
    /// `false` ⇒ `tail` 是未编码原文，`emit` 按 `SendDefMessage` 口径施加 `EDCode.EncodeString`。
    pub tail_encoded: bool,
    /// **卡 `openmir2-script-handler-msgqueue`（rework per review f215e887）**：该帧在 oracle
    /// side 的 `MsgQueue` 优先级 = `(byte)RM_*`（`SendMsg` 的 `(byte)wIdent`，
    /// `BaseObject.Message.cs:181`）。
    ///
    /// **由生产者赋值**（不能再由 SM ident 反推）：同一 SM 可来自不同 RM——`SM_SENDUSERREPAIR`
    /// 普通修理 = `RM_SENDUSERREPAIR`(10141→157)、超级修理 = `RM_SENDUSERSREPAIR`(50142→**222**)，
    /// 二者共用泵分支（`PlayObject.Message.cs:1851-1853`）⇒ 只有发射点知道是哪一个。
    /// `None` = oracle 侧**真直连**（调用点 `SendSocket`/`SendDefMessage`，无 RM 泵）。
    pub queue_priority: Option<u8>,
    /// **原子批次**（`RM_USERGETBACKITEM` → `SendSaveItemList` 一次调用**顺序发多页**，
    /// `PlayObject.Base.cs:1600-1622`）：同批连续的帧合并成**一条**队列消息，出队时按页序
    /// 连发，绝不被四叉堆打散。
    pub atomic_batch: bool,
}

fn frame(ident: u16, recog: i32, param: u16, tag: u16, series: u16, tail: Vec<u8>) -> ShopFrame {
    ShopFrame {
        ident,
        recog,
        param,
        tag,
        series,
        tail,
        tail_encoded: false,
        queue_priority: None,
        atomic_batch: false,
    }
}

/// 尾**已编码**的出帧（`PlayObject.Socket.cs:104-127` 的 `SendSocket(ClientMsg, sMsg)` 直连口径）。
fn frame_encoded(
    ident: u16,
    recog: i32,
    param: u16,
    tag: u16,
    series: u16,
    tail: Vec<u8>,
) -> ShopFrame {
    ShopFrame {
        ident,
        recog,
        param,
        tag,
        series,
        tail,
        tail_encoded: true,
        queue_priority: None,
        atomic_batch: false,
    }
}

impl ShopFrame {
    /// 标记 oracle `MsgQueue` 优先级 = `(byte)RM`（由**发射点**调用；见 [`ShopFrame::queue_priority`]）。
    #[must_use]
    fn queued(mut self, rm: u16) -> ShopFrame {
        self.queue_priority = Some((rm & 0xFF) as u8);
        self
    }

    /// 标记「同一队列消息内顺序发多页」的原子批次（`SendSaveItemList`）。
    #[must_use]
    fn atomic(mut self) -> ShopFrame {
        self.atomic_batch = true;
        self
    }
}

/// `WeightChanged()`（`BaseObject.cs:1771-1774`）：先 `WAbil.Weight = RecalcBagWeight()`，
/// 再 `SendUpdateMsg(RM_WEIGHTCHANGED)`；泵分支（`PlayObject.Message.cs:1760`）产出
/// `SM_WEIGHTCHANGED(622)`：`Recog=Weight`、`Param=WearWeight`、`Tag=HandWeight`、
/// `Series=((W+A+H)^0x3A5F)^0x1F35^0xaa21`（用**当下** `WAbil`）。
///
/// 线上位置由 `MsgQueue` 优先级 `(byte)RM_WEIGHTCHANGED = 131` 决定：`SM_ADDITEM` 走
/// `SendSocket` 直连 ⇒ 先出；622(131) 早于 650（`RM_BUYITEM_SUCCESS` 低字节 149）。
/// 真机证据（`E:/tmp/window/e23-reshot`，卡 `openmir2-merchant-price-list-and-weight-frame`）：
/// C# 是 `200 → 622 → 650`，Rust 此前缺 622。
///
/// 只在**卡内实测的买入路径**发射；`make_drug_item` / `take_back_storage_item` 同样缺 622，
/// 补它们要先补各自的重量重算 / 换队列感知发射器 ⇒ 登记为 T-47 ⑤ 残留，不在本卡夹带。
///
/// 卡 `openmir2-bag-weight-sink`：**「发不发」已收进单点**（[`BuyWeightSink`] 只会在
/// `add_item_to_bag_with_sink` 成功入包后收到一次快照）；本函数只负责把快照编成帧。
#[must_use]
fn weight_changed_frame(snapshot: WeightChanged) -> ShopFrame {
    frame(
        mir2_protocol::messages::SM_WEIGHTCHANGED,
        i32::from(snapshot.weight),
        u16::from(snapshot.wear_weight),
        u16::from(snapshot.hand_weight),
        snapshot.series(),
        Vec::new(),
    )
    .queued(mir2_protocol::messages::RM_WEIGHTCHANGED)
}

/// 买入路径的 [`WeightSink`]：`WeightChanged()`（`BaseObject.cs:1771-1774`）的「重算
/// `WAbil.Weight` + 取快照」在单点的成功回调里做；帧仍由调用方在**既有位置**（200 之后、
/// 650 之前）生成 ⇒ 向量序与线上序都不变。
struct BuyWeightSink<'a> {
    catalog: &'a dyn ItemCatalog,
    wabil: &'a mut Ability,
    snapshot: &'a mut Option<WeightChanged>,
}

impl WeightSink for BuyWeightSink<'_> {
    fn on_item_added(&mut self, items: &[BagItem]) {
        self.wabil.weight = recalc_weight(self.catalog, items);
        *self.snapshot = Some(WeightChanged {
            weight: self.wabil.weight,
            wear_weight: self.wabil.wear_weight,
            hand_weight: self.wabil.hand_weight,
        });
    }
}

/// `Merchant` 实例上本片读到的字段（宿主从世界实体取）。
pub struct MerchantView {
    pub actor_id: i32,
    pub flags: MerchantFlags,
    pub price_rate: i32,
    pub castle_merchant: bool,
    /// `NormNpc.Castle` 在 `World.castles` 的下标（`None` ⇔ oracle `Castle == null`）。
    /// 商税目的地判定用它（`Merchant.cs:546/1257/1392/1644` 的 `Castle != null`）。
    pub castle_index: Option<usize>,
    pub item_type_list: Vec<i32>,
    /// `Merchant.ItemPriceList`（`Merchant.cs:46`）：补货时 `CheckItemPrice` 填的 `(wIndex, nPrice)`。
    /// `PriceTables` 从这里取「已补过价的条目」（`get_item_price` 命中优先）。
    pub item_price_list: Vec<ItemPrice>,
    /// 商人实体 `ChrName`（`BaseObject.ChrName`）。`SendMsgToUser`（`NormNpc.cs:1613-1616`）
    /// 的 `RM_MERCHANTSAY` 尾 = `ChrName + '/' + sMsg` ⇒ 宿主必须把商人名传进来。
    pub chr_name: String,
}

/// 一次商店动作的静态输入（会话/世界/配置）。
pub struct ShopContext<'a> {
    pub catalog: &'a dyn ItemCatalog,
    /// `SystemShare.Config` 的定价/修理字段（`ShopConfig`，缺省值同 `GameSvrConf`）。
    pub cfg: ShopConfig,
    /// `SystemShare.Config.GetAllNpcTax`（`Merchant.cs:1249/1384/1638` 的税条件之一）。
    pub get_all_npc_tax: bool,
    /// `SystemShare.Config.UpgradeWeaponPrice`（CastleManager 分支的税基常量，见 castle_tax 模块头）。
    pub upgrade_weapon_price: i32,
    /// `PlayObject.ScriptLable`（`Merchant.cs:1555/1599` 的 `@s_repair` 分支）。
    pub script_lable: &'a str,
    /// `Config.HumanMaxGold`（`IncGold` 上限，`PlayObject.Base.cs:168`）。
    pub human_max_gold: i32,
    /// 玩家 `BaseObject.ActorId`（`SM_ADDITEM`/`SM_SYSMESSAGE` 的 `Recog`）。
    pub player_actor_id: i32,
    /// `Config.RedMsgFColor`（`SysMsg(..., MsgColor.Red, ...)` 的 `Param` 高字节；`BaseObject.cs:1678`）。
    pub red_f: u8,
    /// `Config.RedMsgBColor`。
    pub red_b: u8,
    /// **制药片（WB）**：`Config.MakeDurgPrice`（`GameSvrConf.cs:1450`，缺省 `100`）——
    /// `UserSelectMakeDurg` 面板价（`Merchant.cs:1010`）与 `ClientMakeDrugItem` 收费/扣金
    /// （`Merchant.cs:1510/1518`）。不放进 `ShopConfig`（那是 `mir2_formula::shop` 的定价/修理公式面，
    /// 制药不在公式层）。
    pub make_durg_price: i32,
}

/// `Merchant` 侧目录适配器：把宿主的 `ability::ItemCatalog`（键 = 1-based `Index`）接到
/// `mir2_formula::shop::StdItemSource`（同口径，见 `formula::shop` 的 `VecItemCatalog` 注释）。
struct CatalogSource<'a>(&'a dyn ItemCatalog);

impl StdItemSource for CatalogSource<'_> {
    fn get_std_item(&self, index: u16) -> Option<&StdItem> {
        self.0.get(index)
    }
}

/// `string.Compare(a, b, OrdinalIgnoreCase) == 0` 的标签口径（ASCII 大小写不敏感；标签恒 ASCII）。
#[must_use]
pub fn eq_label(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// oracle `UserItem`（`MerchantItem`）→ 宿主 `BagItem`（同字段）。
#[must_use]
pub fn to_bag(it: &MerchantItem) -> BagItem {
    BagItem {
        make_index: it.make_index,
        index: it.index,
        dura: it.dura,
        dura_max: it.dura_max,
        desc: it.desc,
    }
}

/// 宿主 `BagItem` → oracle `UserItem`（`MerchantItem`）。
#[must_use]
pub fn to_merchant(it: &BagItem) -> MerchantItem {
    MerchantItem {
        make_index: it.make_index,
        index: it.index,
        dura: it.dura,
        dura_max: it.dura_max,
        desc: it.desc,
    }
}

/// `CustomItemSystem.GetItemName(userItem)`（`CustomItemSystem.cs:119-125`）：模板缺失 ⇒ `None`
/// （调用方先过 `GetStdItem != null` 门）；否则 `Desc[13] == 1` 走自定义名（未命中 = 空串）、
/// 其余取模板名——见 [`crate::bag::item_display_name`]。
#[must_use]
pub fn item_name(catalog: &dyn ItemCatalog, item: &BagItem) -> Option<String> {
    catalog.get(item.index)?;
    Some(crate::bag::item_display_name(catalog, item))
}

/// 建 `PriceTables`：`item_price_list` = 商人自己的 `ItemPriceList`（补货时由 `CheckItemPrice`
/// 填入；`Merchant.cs:46/184-204`）。空表是**有语义**的初值——首次补货才补 ×1.1。
///
/// `source`/`item_price_list` 由调用方持有（`PriceTables` 只活到本函数返回）。
fn price_tables<'a>(
    source: &'a CatalogSource<'a>,
    item_type_list: &'a [i32],
    item_price_list: &'a [ItemPrice],
) -> PriceTables<'a> {
    PriceTables {
        source,
        item_price_list,
        item_type_list,
    }
}

/// 把世界侧 `(u16, i32)` 价格表转成公式层的 [`ItemPrice`]（`n_price` 已是取整后的整数）。
#[must_use]
pub fn price_list_from(rows: &[(u16, i32)]) -> Vec<ItemPrice> {
    rows.iter()
        .map(|(w_index, n_price)| ItemPrice {
            w_index: *w_index,
            n_price: f64::from(*n_price),
        })
        .collect()
}

/// `PlayObject.IncGold`（`PlayObject.Base.cs:168`）：超 `HumanMaxGold` ⇒ 失败（不动余额）。
#[must_use]
pub fn inc_gold(gold: i32, t_gold: i32, human_max_gold: i32) -> bool {
    gold + t_gold <= human_max_gold
}

/// `PlayObject.DecGold`（`PlayObject.Base.cs:754`）：不足 ⇒ 失败。
#[must_use]
pub fn dec_gold(gold: i32, n_gold: i32) -> bool {
    gold >= n_gold
}

/// `BaseObject.IsAddWeightAvailable`（`:193`）。
#[must_use]
pub fn is_add_weight_available(wabil: &crate::ability::Ability, n_weight: i32) -> bool {
    i32::from(wabil.weight) + n_weight <= i32::from(wabil.max_weight)
}

/// `Merchant.UserSelect`（`Merchant.cs:787-960`）的 `@buy/@sell/@repair/@s_repair` 四支。
///
/// `label` = `ClientMerchantDlgSelect` 收到并经 `GetValidStr3(sData, …, '\r')` 取出的**首段**
/// （`Merchant.cs:797`），再 `Trim()`。面板帧与 oracle `UserSelect*` 一一对应：
/// `@buy` ⇒ `RM_SENDGOODSLIST`，`@sell` ⇒ `RM_SENDUSERSELL`，
/// `@repair`/`@s_repair` ⇒ `RM_SENDUSERREPAIR`/`RM_SENDUSERSREPAIR`。
#[must_use]
pub fn user_select(
    label: &str,
    m: &MerchantView,
    goods: &[Vec<MerchantItem>],
    storage_items: &[BagItem],
    ctx: &ShopContext<'_>,
) -> Vec<ShopFrame> {
    let mut out = Vec::new();
    if eq_label(label, LABEL_BUY) && m.flags.is_buy {
        // `UserSelectBuyItem`（`Merchant.cs:962`）：`名/子菜单/价/库存/`。价 = `GetUserPrice(GetItemPrice(Index))`。
        let source = CatalogSource(ctx.catalog);
        let t = price_tables(&source, &m.item_type_list, &m.item_price_list);
        let mut s_send = String::new();
        let mut n10 = 0i32;
        for group in goods {
            let Some(it) = group.first() else { continue };
            let Some(tpl) = ctx.catalog.get(it.index) else {
                continue;
            };
            let name = item_name(ctx.catalog, &to_bag(it)).unwrap_or_default();
            let n_price = shop::get_user_price(&ctx.cfg, shop::get_item_price(&t, it.index));
            let n_stock = i32::try_from(group.len()).unwrap_or(i32::MAX);
            let n_submenu = if tpl.std_mode <= 4 || tpl.std_mode == 42 || tpl.std_mode == 31 {
                0
            } else {
                1
            };
            s_send.push_str(&format!("{name}/{n_submenu}/{n_price}/{n_stock}/"));
            n10 += 1;
        }
        // `RM_SENDGOODSLIST` → `SM_SENDGOODSLIST(Recog=ActorId, Param=n10, tail=Msg)`。
        out.push(
            frame(
                mir2_protocol::messages::SM_SENDGOODSLIST,
                m.actor_id,
                n10 as u16,
                0,
                0,
                downlink::gbk(&s_send),
            )
            .queued(mir2_protocol::messages::RM_SENDGOODSLIST),
        );
    } else if eq_label(label, LABEL_SELL) && m.flags.is_sell {
        out.push(
            frame(
                mir2_protocol::messages::SM_SENDUSERSELL,
                m.actor_id,
                0,
                0,
                0,
                Vec::new(),
            )
            .queued(mir2_protocol::messages::RM_SENDUSERSELL),
        );
    } else if eq_label(label, LABEL_SUPER_REPAIR) && m.flags.is_sup_repair {
        // `UserSelectSuperRepairItem` → `RM_SENDUSERSREPAIR` → `SM_SENDUSERREPAIR`。
        out.push(
            frame(
                mir2_protocol::messages::SM_SENDUSERREPAIR,
                m.actor_id,
                0,
                0,
                0,
                Vec::new(),
            )
            .queued(mir2_protocol::messages::RM_SENDUSERSREPAIR),
        );
    } else if eq_label(label, LABEL_REPAIR) && m.flags.is_repair {
        // `UserSelectRepairItem` → `RM_SENDUSERREPAIR` → `SM_SENDUSERREPAIR`。
        out.push(
            frame(
                mir2_protocol::messages::SM_SENDUSERREPAIR,
                m.actor_id,
                0,
                0,
                0,
                Vec::new(),
            )
            .queued(mir2_protocol::messages::RM_SENDUSERREPAIR),
        );
    } else if eq_label(label, LABEL_STORAGE) && m.flags.is_storage {
        // `UserSelectStorage`（`Merchant.cs:1024`）→ `RM_USERSTORAGEITEM` →
        // `SM_SENDUSERSTORAGEITEM(Recog=ActorId, Param=0)`（`PlayObject.Message.cs:1866`）。
        out.push(
            frame(
                mir2_protocol::messages::SM_SENDUSERSTORAGEITEM,
                m.actor_id,
                0,
                0,
                0,
                Vec::new(),
            )
            .queued(mir2_protocol::messages::RM_USERSTORAGEITEM),
        );
    } else if eq_label(label, LABEL_GETBACK) && m.flags.is_getback {
        // `UserSelectGetBack`（`Merchant.cs:1029`）→ `RM_USERGETBACKITEM` → `SendSaveItemList(ActorId)`。
        out.extend(back_item_list(m.actor_id, storage_items, ctx));
    } else if eq_label(label, LABEL_MAKE_DRUG) && m.flags.is_make_drug {
        // `UserSelectMakeDurg`（`Merchant.cs:1002-1017`）：
        // `for (i<GoodsList.Count) { stdItem = GetStdItem(GoodsList[i][0].Index);
        //                             sSendMsg += stdItem.Name + '/' + 0 + '/' + MakeDurgPrice + '/' + 1 + '/'; }`
        // 非空 ⇒ `SendMsg(RM_USERMAKEDRUGITEMLIST, 0, ActorId, 0, 0, sSendMsg)`
        // ⇒ `SM_SENDUSERMAKEDRUGITEMLIST(Recog=nParam1=ActorId, Param=nParam2=0, 尾=EncodeString(sSendMsg))`
        //（`PlayObject.Message.cs:1876`）。
        let mut s_send = String::new();
        for group in goods {
            let Some(it) = group.first() else { continue };
            let Some(tpl) = ctx.catalog.get(it.index) else {
                continue;
            };
            // `sSendMsg += stdItem.Name + '/' + 0 + '/' + MakeDurgPrice + '/' + 1 + '/'`。
            s_send.push_str(&tpl.name);
            s_send.push_str(&format!("/0/{}/1/", ctx.make_durg_price));
        }
        if !s_send.is_empty() {
            out.push(
                frame(
                    mir2_protocol::messages::SM_SENDUSERMAKEDRUGITEMLIST,
                    m.actor_id,
                    0,
                    0,
                    0,
                    downlink::gbk(&s_send),
                )
                .queued(mir2_protocol::messages::RM_USERMAKEDRUGITEMLIST),
            );
        }
    }
    out
}

/// `NormNpc.UserSelect` 的**标签账本**半边（`NormNpc.cs:1632-1658`）：
/// `ScriptGotoCount = 0` 由调用方每次新建 player 表达；本函数只维护
/// `ScriptCurrLable / ScriptGoBackLable`（比较一律 `OrdinalIgnoreCase`）。
///
/// 逐行：
/// ```text
/// if (sData[0] == '@') {
///   GetValidStr3(sData, ref sLabel, '\r');
///   if (ScriptCurrLable != sLabel) {
///     if (sLabel != "@back") { ScriptGoBackLable = ScriptCurrLable; ScriptCurrLable = sLabel; }
///     else if (ScriptCurrLable != "") ScriptCurrLable = "";
///     else ScriptGoBackLable = "";
///   }
/// }
/// ```
pub fn base_user_select(curr_lable: &mut String, go_back_lable: &mut String, label: &str) {
    if label.is_empty() || !label.starts_with('@') {
        return;
    }
    if !eq_label(curr_lable, label) {
        if !eq_label(label, "@back") {
            *go_back_lable = curr_lable.clone();
            *curr_lable = label.to_string();
        } else if !curr_lable.is_empty() {
            curr_lable.clear();
        } else {
            go_back_lable.clear();
        }
    }
}

/// `Merchant.ClientMakeDrugItem`（`Merchant.cs:1500-1551`）的结果。
pub struct MakeDrugResult {
    /// 出帧（oracle 发送序：`SM_ADDITEM`（`SendAddItem` 直发，成功时）→ `SM_DELITEMS`
    /// （`RM_SENDDELITEMLIST` 经 `MsgQueue`，优先级 `(byte)10148=164`）→ `SM_MAKEDRUG_SUCCESS/FAIL`
    /// （`RM_MAKEDRUG_SUCCESS/FAIL`，优先级 `166/167`））。见 [`make_drug_item`] 的头注。
    pub frames: Vec<ShopFrame>,
    /// `n14 == 0`（`SM_MAKEDRUG_SUCCESS`）。
    pub ok: bool,
}

/// `Merchant.ClientMakeDrugCheckNeedItem`（`Merchant.cs:1432-1500`）：按 `MakeItem.txt`
/// 配方核对背包材料，命中则**消耗**（从后往前删）并把删除项列表返回。
///
/// 逐行：先统计（`for j in 0..ItemList.Count`，精确名 `==`，非 IgnoreCase）；
/// 全齐则再反向 `for j in (Count-1)..=0` 删除，每删一件记一条 `DeleteItem{ItemName, MakeIndex}`。
///
/// 返回 `(result, deleted)`：`result == false` ⇒ 材料不足（oracle `:1462` 的 `break`）。
/// `deleted` 非空 ⇒ oracle 会 `SendMsg(this, RM_SENDDELITEMLIST, 0, objectId, 0, 0)` ⇒ `SM_DELITEMS`。
#[must_use]
pub fn check_need_item(
    make_items: &MakeItemRegistry,
    catalog: &dyn ItemCatalog,
    item_list: &mut Vec<BagItem>,
    s_item_name: &str,
) -> (bool, Vec<(String, i32)>) {
    let Some(list10) = make_items.get(s_item_name) else {
        // `list10 == null` ⇒ `return false`（`Merchant.cs:1436`）。
        return (false, Vec::new());
    };
    let mut result = true;
    for mi in list10 {
        let mut n1c = mi.item_count;
        for it in item_list.iter() {
            // `SystemShare.ItemSystem.GetStdItemName(ItemList[j].Index) == s20`（精确串比较）。
            if catalog.name(it.index) == Some(mi.item_name.as_str()) {
                n1c -= 1;
            }
        }
        if n1c > 0 {
            result = false;
            break;
        }
    }
    let mut deleted: Vec<(String, i32)> = Vec::new();
    if result {
        for mi in list10 {
            let mut n1c = mi.item_count;
            let mut j = item_list.len();
            while j > 0 && n1c > 0 {
                j -= 1;
                if catalog.name(item_list[j].index) == Some(mi.item_name.as_str()) {
                    deleted.push((mi.item_name.clone(), item_list[j].make_index));
                    item_list.remove(j);
                    n1c -= 1;
                }
            }
        }
    }
    (result, deleted)
}

/// `Merchant.ClientMakeDrugItem`（`Merchant.cs:1500-1551`）：由 `CM_USERMAKEDRUGITEM` 触发
/// （`PlayObject.Operate.cs:1216` 已过 `FindMerchant`/`IsMakeDrug`/`Envir`/`IsWithinSight` 门，
/// 由宿主 `handle_shop_cmd` 判）。
///
/// 逐行（`n14` = 失败码）：
/// - 逐个在售组取首件 `makeItem`，`GetStdItem(makeItem.Index)` 非空且 `Name`（IgnoreCase）匹配
///   请求名 ⇒ 命中；否则 `n14` 保持 `1`（未命中/成功前）。
/// - `Gold >= MakeDurgPrice` 才进：否则 `n14 = 3`；
/// - 材料齐（[`check_need_item`]）：`CopyToUserItemFromName` 造新件（`Index` = 名首命中、`MakeIndex` =
///   计数器、`Dura=DuraMax=tpl.DuraMax`），`AddItemToBag`（46 上限）成功 ⇒ `Gold -= price`、
///   `SendAddItem`（`SM_ADDITEM`）、`n14 = 0`、`break`；不成功 ⇒ `n14 = 2`（材料**已消耗**）；
/// - 材料不足 ⇒ `n14 = 4`。
/// - 收尾：`n14 == 0` ⇒ `SM_MAKEDRUG_SUCCESS(Recog=Gold)`；否则 `SM_MAKEDRUG_FAIL(Recog=n14)`。
///
/// **发送序（R 级，见 rationale）**：`SendAddItem` 走 `SendSocket(ClientMsg, EncodeBuffer(...))`（**直发**，
/// `PlayObject.Base.cs:198-227`）⇒ `SM_ADDITEM` 先出；`RM_SENDDELITEMLIST`（优先级 164）与
/// `RM_MAKEDRUG_SUCCESS/FAIL`（166/167）经 `MsgQueue` 按 `(byte)wIdent` 升序出队 ⇒ `SM_DELITEMS` 先于
/// 结果帧。`MakeIndex` 用传入的 `item_number` 单调计数器（`M2Share.GetItemNumber()` 的 `+ GetTickCount()`
/// 项登记，同 `add_newbie_items`）。
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn make_drug_item(
    state: &mut GroundState,
    goods: &[Vec<MerchantItem>],
    s_item_name: &str,
    catalog: &dyn ItemCatalog,
    make_items: &MakeItemRegistry,
    item_number: &mut i32,
    ctx: &ShopContext<'_>,
) -> MakeDrugResult {
    let mut frames: Vec<ShopFrame> = Vec::new();
    let mut n14 = 1i32;
    for group in goods {
        let Some(first) = group.first() else { continue };
        let Some(tpl) = catalog.get(first.index) else {
            continue;
        };
        // `string.Compare(stdItem.Name, sItemName, OrdinalIgnoreCase) == 0`。
        if !tpl.name.eq_ignore_ascii_case(s_item_name) {
            continue;
        }
        if state.gold >= ctx.make_durg_price {
            let (ok, deleted) =
                check_need_item(make_items, catalog, &mut state.item_list, s_item_name);
            if ok {
                // `CopyToUserItemFromName(sItemName, ref userItem)`（`GameItemSystem.cs:113-140`）：
                // 名首命中 ⇒ `Index`/`MakeIndex`/`Dura=DuraMax`；未命中 ⇒ `new UserItem()` 形态（全 0）。
                let mut user_item = BagItem::new(0, 0);
                if let Some(ni) = catalog.find_by_name(s_item_name) {
                    if let Some(nt) = catalog.get(ni) {
                        *item_number += 1;
                        user_item = BagItem {
                            make_index: *item_number,
                            index: ni,
                            dura: nt.dura_max,
                            dura_max: nt.dura_max,
                            desc: [0; 14],
                        };
                    }
                }
                // `SendAddItem`（`PlayObject.Base.cs:198-227`）⇒ `SM_ADDITEM(Recog=ActorId, Series=1)`
                // + 尾 `EncodeBuffer(clientItem)`（G-1：复用 `trade::add_item_tail`）。
                // 尾按**入包前**的 `user_item` 编码（纯计算，无副作用）。
                let add_tail =
                    crate::trade::add_item_tail(&user_item, ctx.catalog).unwrap_or_default();
                // `AddItemToBag`（`BaseObject.cs:1241`）：`ItemList.Count >= MaxBagItem(46)` ⇒ false。
                if add_item_to_bag(&mut state.item_list, user_item, MAX_BAG_ITEM) {
                    // `WeightChanged()` 未发（T-47 ⑤ 残留）：本路径没有重量重算、宿主发射器
                    // 虽同队列但缺 622 的实测证据 ⇒ 留到后继卡（`buy_item` 已发）。
                    state.gold -= ctx.make_durg_price;
                    // 尾已是 `EDCode.EncodeBuffer(ClientItem)` ⇒ 不能再 `EncodeString`。
                    frames.push(frame_encoded(
                        mir2_protocol::messages::SM_ADDITEM,
                        ctx.player_actor_id,
                        0,
                        0,
                        1,
                        add_tail,
                    ));
                    if !deleted.is_empty() {
                        frames.push(del_items_frame(&deleted));
                    }
                    n14 = 0;
                    break;
                }
                // `DisPose(userItem); n14 = 2`——材料已在 `check_need_item` 消耗，删除列表照发。
                if !deleted.is_empty() {
                    frames.push(del_items_frame(&deleted));
                }
                n14 = 2;
            } else {
                n14 = 4;
            }
        } else {
            n14 = 3;
        }
    }
    if n14 == 0 {
        // `SendMsg(RM_MAKEDRUG_SUCCESS, 0, Gold, 0, 0)` ⇒ `SendDefMessage(SM_MAKEDRUG_SUCCESS, Gold, 0,0,0)`。
        frames.push(
            frame(
                mir2_protocol::messages::SM_MAKEDRUG_SUCCESS,
                state.gold,
                0,
                0,
                0,
                Vec::new(),
            )
            .queued(mir2_protocol::messages::RM_MAKEDRUG_SUCCESS),
        );
    } else {
        // `SendMsg(RM_MAKEDRUG_FAIL, 0, n14, 0, 0)` ⇒ `SendDefMessage(SM_MAKEDRUG_FAIL, n14, 0,0,0)`。
        frames.push(
            frame(
                mir2_protocol::messages::SM_MAKEDRUG_FAIL,
                n14,
                0,
                0,
                0,
                Vec::new(),
            )
            .queued(mir2_protocol::messages::RM_MAKEDRUG_FAIL),
        );
    }
    MakeDrugResult {
        frames,
        ok: n14 == 0,
    }
}

/// `SendDelItemList`（`PlayObject.Base.cs:1637-1646`）的出帧：
/// `s10 = "ItemName/MakeIndex/" × n`；`MakeMessage(SM_DELITEMS, 0, 0, 0, n)` + `SendSocket(ClientMsg, EncodeString(s10))`
/// （**尾已编码** ⇒ `tail_encoded = true`，见 [`ShopFrame`] 口径）。
///
/// `ItemName` 是 `std::string`（GBK 线上）⇒ 拼接后按 GBK 字节编码。
pub fn del_items_frame(deleted: &[(String, i32)]) -> ShopFrame {
    let mut s10 = String::new();
    for (name, make_index) in deleted {
        s10.push_str(name);
        s10.push('/');
        s10.push_str(&make_index.to_string());
        s10.push('/');
    }
    frame_encoded(
        mir2_protocol::messages::SM_DELITEMS,
        0,
        0,
        0,
        deleted.len() as u16,
        mir2_protocol::edcode::encode(&downlink::gbk(&s10)),
    )
    .queued(mir2_protocol::messages::RM_SENDDELITEMLIST)
}

///
/// 收包侧 `CM_USERGETDETAILITEM` 与 `CM_USERBUYITEM` 共用 `ClientUserBuyItem`（`Operate.cs:192-213`）
/// ⇒ 同一套 `IsBuy && 同图 && |dx|,|dy| <= 15` 门（由调用方判）；`n_int` = 收包 `Param`（菜单顶行）。
///
/// 帧尾口径：`sSendMsg` = 每件 `EDCode.EncodeBuffer(clientItem) + '/'`，再经 RM→SM 的 `SendDefMessage`
/// 施加 `EDCode.EncodeString`（`PlayObject.Message.cs:1830-1831` + `PlayObject.Socket.cs:26-43`）
/// ⇒ 本帧 `tail_encoded = false`（原文 = 已编码件的 ASCII 拼接，`edcode::encode` 无损往返）。
#[must_use]
pub fn detail_goods_list(
    m: &MerchantView,
    goods: &mut [Vec<MerchantItem>],
    s_item_name: &str,
    n_int_in: i32,
    ctx: &ShopContext<'_>,
) -> ShopFrame {
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list, &m.item_price_list);
    let mut n_int = n_int_in;
    let mut s_send_msg: Vec<u8> = Vec::new();
    let mut n_item_count: u16 = 0;
    // `for (i = 0; i < GoodsList.Count; i++)`：第一件模板名 `== sItemName`（**ordinal**，非 IgnoreCase）就进。
    for group in goods.iter_mut() {
        if group.is_empty() {
            continue;
        }
        let first_idx = group[0].index;
        let Some(tpl) = ctx.catalog.get(first_idx) else {
            continue;
        };
        if tpl.name != s_item_name {
            continue;
        }
        let group_len = i32::try_from(group.len()).unwrap_or(i32::MAX);
        if group_len - 1 < n_int {
            n_int = i32::max(0, group_len - 10);
        }
        // `for (j = list20.Count - 1; j >= 0; j--)` 取到 10 件为止。
        let mut j = group_len - 1;
        while j >= 0 {
            let item = &mut group[j as usize];
            // `GetUpgradeStdItem`（T-47 ⑥）+ `GetUserPrice(playObject, GetUserItemPrice(userItem))`。
            let n_price = shop::get_user_price(&ctx.cfg, shop::get_user_item_price(&t, item));
            let ci = ClientItem {
                tpl,
                name: tpl.name.clone(),
                make_index: item.make_index,
                dura: item.dura,
                dura_max: n_price as u16,
                desc: item.desc,
            };
            s_send_msg.extend_from_slice(&mir2_protocol::edcode::encode(&encode_client_item(&ci)));
            s_send_msg.push(b'/');
            n_item_count = n_item_count.saturating_add(1);
            if n_item_count >= 10 {
                break;
            }
            j -= 1;
        }
        break;
    }
    // `SendMsg(RM_SENDDETAILGOODSLIST, 0, ActorId, nItemCount, nInt, sSendMsg)` →
    // `SM_SENDDETAILGOODSLIST(Recog=ActorId, Param=nItemCount, Tag=nInt)`。
    frame(
        mir2_protocol::messages::SM_SENDDETAILGOODSLIST,
        m.actor_id,
        n_item_count,
        n_int as u16,
        0,
        s_send_msg,
    )
    .queued(mir2_protocol::messages::RM_SENDDETAILGOODSLIST)
}

/// `PlayObject.SendSaveItemList`（`PlayObject.Base.cs:1590-1620`）：仓库列表分页（每页 50，末尾页 `Series = page-1`）。
///
/// 帧尾是 `SendSocket(ClientMsg, sSendMsg)` **直连**（无 `EncodeString`）⇒ `tail_encoded = true`；
/// 整页模板全缺 ⇒ `sSendMsg` 为空、`SendSocket` 直接 return（该页不发）。
#[must_use]
pub fn back_item_list(actor_id: i32, items: &[BagItem], ctx: &ShopContext<'_>) -> Vec<ShopFrame> {
    let mut out = Vec::new();
    let max_count = i32::try_from(items.len()).unwrap_or(i32::MAX);
    let page = (max_count as f32 / 50.0).ceil() as i32;
    for p in 0..page {
        let start = p * 50;
        let end = i32::min(start + 50, max_count);
        let mut tail: Vec<u8> = Vec::new();
        for i in start..end {
            let it = &items[i as usize];
            if let Some(tpl) = ctx.catalog.get(it.index) {
                let ci = ClientItem {
                    tpl,
                    // `clientItem.Item.Name = CustomItemSystem.GetItemName(userItem)`
                    // （`PlayObject.Base.cs:1611-1613`；仓库列表**用**自定义名）。
                    name: crate::bag::item_display_name(ctx.catalog, it),
                    make_index: it.make_index,
                    dura: it.dura,
                    dura_max: it.dura_max,
                    desc: it.desc,
                };
                tail.extend_from_slice(&mir2_protocol::edcode::encode(&encode_client_item(&ci)));
                tail.push(b'/');
            }
        }
        if tail.is_empty() {
            continue;
        }
        let series = if page - 1 < 0 { 0 } else { page - 1 };
        out.push(
            frame_encoded(
                mir2_protocol::messages::SM_SAVEITEMLIST,
                actor_id,
                0,
                p as u16,
                series as u16,
                tail,
            )
            .queued(mir2_protocol::messages::RM_USERGETBACKITEM)
            .atomic(),
        );
    }
    out
}

/// `ClientStorageItem`（`PlayObject.Operate.cs:1607-1655`）的结果。
pub struct StorageResult {
    pub frames: Vec<ShopFrame>,
    /// `bo19`（命中名/号且商人允许存 ⇒ true）。
    pub ok: bool,
}

/// `PlayObject.ClientStorageItem`：把背包件移入仓库（39 上限）。
///
/// `storage_items` = `PlayObject.StorageItemList`（调用方持有）；`can_store` = oracle
/// `StorageItemList.Count < 39`（`Operate.cs:1629`，由调用方用 [`crate::storage`] 的口径算出）。
/// `merchant_allows` = `merchant != null && merchant.IsStorage && (同图且 IsWithinSight || FunctionNPC)`
/// （由调用方预算；`FunctionNPC` 未建模 ⇒ 只可能是同图+视距）。
/// `raw_name` 是收包 `Msg` 原文（函数内先做 `GetValidStr3(sMsg, ref sMsg, ' ')` 取首段）。
#[must_use]
pub fn storage_item(
    state: &mut GroundState,
    storage_items: &mut Vec<BagItem>,
    can_store: bool,
    merchant_allows: bool,
    make_index: i32,
    raw_name: &str,
    ctx: &ShopContext<'_>,
) -> StorageResult {
    let mut frames = Vec::new();
    // `if (sMsg.Contains(' ')) HUtil32.GetValidStr3(sMsg, ref sMsg, ' ');`（取首个空格前的段）。
    let s_msg = raw_name.split(' ').next().unwrap_or(raw_name);
    // `PayMent == 1 && !Config.TryModeUseStorage` 门：本部署 `TestServer=TRUE` ⇒ PayMent=3 ⇒ 恒假（登记）。
    let mut bo19 = false;
    for i in 0..state.item_list.len() {
        let it = &state.item_list[i];
        let name_matches = item_name(ctx.catalog, it).is_some_and(|n| eq_label(&n, s_msg));
        if it.make_index == make_index && name_matches {
            if merchant_allows {
                if can_store {
                    let moved = state.item_list.remove(i);
                    storage_items.push(moved);
                    state.wabil.weight = recalc_weight(ctx.catalog, &state.item_list);
                    frames.push(frame(
                        mir2_protocol::messages::SM_STORAGE_OK,
                        0,
                        0,
                        0,
                        0,
                        Vec::new(),
                    ));
                } else {
                    frames.push(frame(
                        mir2_protocol::messages::SM_STORAGE_FULL,
                        0,
                        0,
                        0,
                        0,
                        Vec::new(),
                    ));
                }
                bo19 = true;
            }
            break;
        }
    }
    if !bo19 {
        frames.push(frame(
            mir2_protocol::messages::SM_STORAGE_FAIL,
            0,
            0,
            0,
            0,
            Vec::new(),
        ));
    }
    StorageResult { frames, ok: bo19 }
}

/// `ClientTakeBackStorageItem`（`PlayObject.Operate.cs:1656-1713`）的结果。
pub struct TakeBackResult {
    pub frames: Vec<ShopFrame>,
    /// `bo19`（命中名/号、负重够且商人允许取回 ⇒ true）。
    pub ok: bool,
}

/// `PlayObject.ClientTakeBackStorageItem`：把仓库件取回背包。
///
/// 调用方须先完成 oracle 的前置：`FindMerchant(actorId) != null` 且 `IsCanGetBackItem`（否则**无任何帧**）。
/// `merchant_allows` = `merchant.IsGetback && (同图+视距 || FunctionNPC)`；`raw_name` 是收包 `Msg`
/// 原文（本路径**不**做空格截断，`Operate.cs:1656-1659`）。
#[must_use]
pub fn take_back_storage_item(
    state: &mut GroundState,
    storage_items: &mut Vec<BagItem>,
    merchant_allows: bool,
    make_index: i32,
    raw_name: &str,
    ctx: &ShopContext<'_>,
) -> TakeBackResult {
    let mut frames = Vec::new();
    let mut bo19 = false;
    for i in 0..storage_items.len() {
        let it = &storage_items[i];
        let name_matches = item_name(ctx.catalog, it).is_some_and(|n| eq_label(&n, raw_name));
        if it.make_index == make_index && name_matches {
            let std_weight = ctx.catalog.get(it.index).map_or(0, |t| i32::from(t.weight));
            if is_add_weight_available(&state.wabil, std_weight) {
                if merchant_allows {
                    if state.item_list.len() < MAX_BAG_ITEM {
                        let moved = storage_items.remove(i);
                        // `AddItemToBag`（`BaseObject.cs:1241`）：先入包（内含 WeightChanged）。
                        // `SendAddItem`（`BaseObject.cs:225`）：`SM_ADDITEM(ActorId,0,0,1)`
                        // + 尾 `EncodeBuffer(clientItem)`（G-1）。
                        let add_tail =
                            crate::trade::add_item_tail(&moved, ctx.catalog).unwrap_or_default();
                        frames.push(frame_encoded(
                            mir2_protocol::messages::SM_ADDITEM,
                            ctx.player_actor_id,
                            0,
                            0,
                            1,
                            add_tail,
                        ));
                        // 上面已过容量门 ⇒ 单点必返回 true（返回值与 oracle 一样被忽略）。
                        let _ = add_item_to_bag(&mut state.item_list, moved, MAX_BAG_ITEM);
                        state.wabil.weight = recalc_weight(ctx.catalog, &state.item_list);
                        // `SM_TAKEBACKSTORAGEITEM_OK(Recog=nItemIdx)`。
                        frames.push(frame(
                            mir2_protocol::messages::SM_TAKEBACKSTORAGEITEM_OK,
                            make_index,
                            0,
                            0,
                            0,
                            Vec::new(),
                        ));
                    } else {
                        frames.push(frame(
                            mir2_protocol::messages::SM_TAKEBACKSTORAGEITEM_FULLBAG,
                            0,
                            0,
                            0,
                            0,
                            Vec::new(),
                        ));
                    }
                    bo19 = true;
                }
            } else {
                // `SysMsg(MessageSettings.CanotGetItems, Red, Hint)` → `SM_SYSMESSAGE(ActorId,
                // MakeWord(RedMsgFColor, RedMsgBColor), 0, 1, 原文)`（`BaseObject.cs:1678`）。
                frames.push(
                    frame(
                        mir2_protocol::messages::SM_SYSMESSAGE,
                        ctx.player_actor_id,
                        smframe::make_word(ctx.red_f, ctx.red_b),
                        0,
                        1,
                        downlink::gbk(CANOT_GET_ITEMS_MSG),
                    )
                    .queued(mir2_protocol::messages::RM_SYSMESSAGE),
                );
            }
            break;
        }
    }
    if !bo19 {
        frames.push(frame(
            mir2_protocol::messages::SM_TAKEBACKSTORAGEITEM_FAIL,
            0,
            0,
            0,
            0,
            Vec::new(),
        ));
    }
    TakeBackResult { frames, ok: bo19 }
}

/// `Merchant.ClientQuerySellPrice`（`Merchant.cs:1345`）：`nC = GetSellItemPrice(GetUserItemPrice(item))`，
/// 回 `RM_SENDBUYPRICE(0, nC>=0 ? nC : 0)`。
///
/// ⚠ `GetUserItemPrice` 对 `StdMode == 43` **写回 `DuraMax = 10000`**（oracle 副作用）⇒ 这里把
/// 写回同步到 `item`（与 `PlayObject.Operate.cs:131` 传**真身**给 `GetUserItemPrice` 同口径）。
#[must_use]
pub fn query_sell_price(m: &MerchantView, item: &mut BagItem, ctx: &ShopContext<'_>) -> ShopFrame {
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list, &m.item_price_list);
    let mut it = to_merchant(item);
    let price = shop::get_user_item_price(&t, &mut it);
    item.dura_max = it.dura_max; // BUG-REPLICA：StdMode 43 的写回
    let n_c = shop::get_sell_item_price(price);
    let recog = if n_c >= 0 { n_c } else { 0 };
    frame(
        mir2_protocol::messages::SM_SENDBUYPRICE,
        recog,
        0,
        0,
        0,
        Vec::new(),
    )
    .queued(mir2_protocol::messages::RM_SENDBUYPRICE)
}

/// `CM_USERSELLITEM` 的结果：成功时 `item_list` 已移除、`GoodsList` 已回收。
pub struct SellResult {
    pub frames: Vec<ShopFrame>,
    /// `ClientSellItem` 的返回值（`Operate.cs:172` 用它决定是否 `RemoveAt`）。
    pub ok: bool,
    /// 成功后待入账的商税意图（`Merchant.cs:1392-1400`）；失败/无条件 ⇒ `None`。
    pub tax: Option<MerchantTaxIntent>,
}

/// `Merchant.ClientSellItem`（`Merchant.cs:1377`）+ `PlayObject.ClientUserSellItem`（`Operate.cs:162`）。
///
/// `make_index`/`item_name` 来自收包（`nParam1`/`Msg`）；调用方已完成 `merchant.IsSell && 同图 &&
/// IsWithinSight` 门。成功分支：`IncGold`（超 `HumanMaxGold` ⇒ `RM_USERSELLITEM_FAIL`）→ 税 →
/// `RM_USERSELLITEM_OK(0, Gold)` → `AddItemToGoodsList`；随后宿主移除背包件 + `WeightChanged`。
pub fn sell_item(
    m: &MerchantView,
    state: &mut GroundState,
    goods: &mut Vec<Vec<MerchantItem>>,
    make_index: i32,
    wanted_name: &str,
    ctx: &ShopContext<'_>,
) -> SellResult {
    let mut frames = Vec::new();
    // `Operate.cs:164-171`：按 `MakeIndex` + 名（OrdinalIgnoreCase）找件。
    let Some(pos) = state.item_list.iter().position(|it| {
        it.make_index == make_index
            && item_name(ctx.catalog, it).is_some_and(|n| eq_label(&n, wanted_name))
    }) else {
        return SellResult {
            frames,
            ok: false,
            tax: None,
        };
    };
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list, &m.item_price_list);
    let mut it = to_merchant(&state.item_list[pos]);
    let n_price = shop::get_sell_item_price(shop::get_user_item_price(&t, &mut it));
    state.item_list[pos].dura_max = it.dura_max; // BUG-REPLICA 写回
    let std_mode = ctx.catalog.get(it.index).map_or(0, |s| s.std_mode);
    // `ClientSellItem_sub_4A1C84`（`Merchant.cs:1364`）：StdMode 25/30 需 `Dura >= 4000`。
    let sub_ok = !(std_mode == 25 || std_mode == 30) || it.dura >= 4000;
    if n_price > 0 && sub_ok {
        if inc_gold(state.gold, n_price, ctx.human_max_gold) {
            state.gold += n_price;
            // `Merchant.cs:1392-1400`：`Castle.IncRateGold(nPrice)` / `CastleMgr.IncRateGold(UpgradeWeaponPrice)`。
            let tax = merchant_tax_intent(
                n_price,
                m.castle_merchant,
                ctx.get_all_npc_tax,
                m.castle_index.is_some(),
                ctx.upgrade_weapon_price,
            );
            frames.push(
                frame(
                    mir2_protocol::messages::SM_USERSELLITEM_OK,
                    state.gold,
                    0,
                    0,
                    0,
                    Vec::new(),
                )
                .queued(mir2_protocol::messages::RM_USERSELLITEM_OK),
            );
            // `AddItemToGoodsList`（`Merchant.cs:1417`）：`Dura <= 0` 不收，否则按 `Index` 组 `Insert(0)`。
            if it.dura > 0 {
                match goods
                    .iter_mut()
                    .find(|g| g.first().is_some_and(|f| f.index == it.index))
                {
                    Some(group) => group.insert(0, it),
                    None => goods.push(vec![it]),
                }
            }
            // 宿主移除背包件（`Operate.cs:176-179` 的 `ItemList.RemoveAt` + `WeightChanged`）。
            state.item_list.remove(pos);
            state.wabil.weight = recalc_weight(ctx.catalog, &state.item_list);
            SellResult {
                frames,
                ok: true,
                tax,
            }
        } else {
            frames.push(
                frame(
                    mir2_protocol::messages::SM_USERSELLITEM_FAIL,
                    0,
                    0,
                    0,
                    0,
                    Vec::new(),
                )
                .queued(mir2_protocol::messages::RM_USERSELLITEM_FAIL),
            );
            SellResult {
                frames,
                ok: false,
                tax: None,
            }
        }
    } else {
        frames.push(
            frame(
                mir2_protocol::messages::SM_USERSELLITEM_FAIL,
                0,
                0,
                0,
                0,
                Vec::new(),
            )
            .queued(mir2_protocol::messages::RM_USERSELLITEM_FAIL),
        );
        SellResult {
            frames,
            ok: false,
            tax: None,
        }
    }
}

/// `CM_USERBUYITEM` 的结果。
pub struct BuyResult {
    pub frames: Vec<ShopFrame>,
    /// `n1C == 0`（买入成功）。
    pub ok: bool,
    /// 成功后待入账的商税意图（`Merchant.cs:1257-1266`）；失败/无条件 ⇒ `None`。
    pub tax: Option<MerchantTaxIntent>,
}

/// `Merchant.ClientBuyItem`（`Merchant.cs:1219`）。
///
/// `s_item_name` = `processMsg.Msg`（oracle 用 `Equals`，**大小写敏感**；见 `Merchant.cs:1239`），
/// `n_int` = `processMsg.nParam2`（`Operate.cs:192` 的 `nInt`）。失败码：`2` = 负重不足/背包满、
/// `3` = 钱不够或价格 <= 0。
pub fn buy_item(
    m: &MerchantView,
    state: &mut GroundState,
    goods: &mut Vec<Vec<MerchantItem>>,
    s_item_name: &str,
    n_int: i32,
    ctx: &ShopContext<'_>,
) -> BuyResult {
    let mut frames = Vec::new();
    let mut tax = None;
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list, &m.item_price_list);
    let mut n1c = 1i32;
    let mut bo29 = false;
    let mut i = 0usize;
    while i < goods.len() && !bo29 {
        let Some(first) = goods[i].first().cloned() else {
            i += 1;
            continue;
        };
        let Some(tpl) = ctx.catalog.get(first.index) else {
            i += 1;
            continue;
        };
        let std_weight = i32::from(tpl.weight);
        let std_mode = tpl.std_mode;
        if is_add_weight_available(&state.wabil, std_weight) {
            let s_user_item_name = item_name(ctx.catalog, &to_bag(&first)).unwrap_or_default();
            if s_user_item_name == s_item_name {
                let group_len = goods[i].len();
                let mut j = 0usize;
                while j < group_len && !bo29 {
                    let candidate = goods[i][j].clone();
                    if std_mode <= 4
                        || std_mode == 42
                        || std_mode == 31
                        || candidate.make_index == n_int
                    {
                        let mut price_item = candidate.clone();
                        let n_price = shop::get_user_price(
                            &ctx.cfg,
                            shop::get_user_item_price(&t, &mut price_item),
                        );
                        if state.gold >= n_price && n_price > 0 {
                            // `SendAddItem` 的尾按**入包前**的件编码（纯计算，无副作用）。
                            let bought = to_bag(&price_item);
                            let add_tail = crate::trade::add_item_tail(&bought, ctx.catalog)
                                .unwrap_or_default();
                            // 卡 `openmir2-bag-weight-sink`：`WeightChanged()` 由单点在成功入包后
                            // 回调一次（满包不回调）；这里只接快照，帧仍在既有位置生成。
                            let mut weight_after: Option<WeightChanged> = None;
                            let mut sink = BuyWeightSink {
                                catalog: ctx.catalog,
                                wabil: &mut state.wabil,
                                snapshot: &mut weight_after,
                            };
                            if add_item_to_bag_with_sink(
                                &mut state.item_list,
                                bought,
                                MAX_BAG_ITEM,
                                &mut sink,
                            ) {
                                // `AddItemToBag` 成功：金币/税/出包 + 从库存移除。
                                state.gold -= n_price;
                                // `Merchant.cs:1257-1266`：Castle 分支用 `nPrice`；CastleManager 分支用
                                // `Config.UpgradeWeaponPrice`（castle_tax 模块头「照搬 ①」）。
                                tax = merchant_tax_intent(
                                    n_price,
                                    m.castle_merchant,
                                    ctx.get_all_npc_tax,
                                    m.castle_index.is_some(),
                                    ctx.upgrade_weapon_price,
                                );
                                frames.push(frame_encoded(
                                    mir2_protocol::messages::SM_ADDITEM,
                                    ctx.player_actor_id,
                                    0,
                                    0,
                                    1,
                                    add_tail,
                                ));
                                // `WeightChanged()`（`BaseObject.cs:1771-1774`）在 `AddItemToBag` 内、
                                // `SendAddItem` 之前触发；线上 622 排在直连的 200 之后、650(149) 之前。
                                //
                                // 走单点 ⇒ 成功入包**必**回调一次（满包不回调）⇒ 快照必在：
                                // 用 `expect` 把「sink 没触发 ⇒ 静默少一帧」变成大声失败
                                // （行为与旧的无条件 push 一致，review post-merge `bca8074bf` O2）。
                                let snapshot =
                                    weight_after.expect("单点成功入包 ⇒ WeightSink 必已回调一次");
                                frames.push(weight_changed_frame(snapshot));
                                let g = &mut goods[i];
                                g.remove(j);
                                if g.is_empty() {
                                    goods.remove(i);
                                }
                                n1c = 0;
                            } else {
                                n1c = 2;
                            }
                        } else {
                            n1c = 3;
                        }
                        bo29 = true;
                    }
                    j += 1;
                }
            }
        } else {
            n1c = 2;
        }
        if !bo29 {
            i += 1;
        }
    }
    if n1c == 0 {
        // `RM_BUYITEM_SUCCESS(0, Gold, nInt)` → `SM_BUYITEM_SUCCESS(Recog=Gold, LoWord(nInt), HiWord(nInt))`。
        frames.push(
            frame(
                mir2_protocol::messages::SM_BUYITEM_SUCCESS,
                state.gold,
                (n_int & 0xFFFF) as u16,
                ((n_int >> 16) & 0xFFFF) as u16,
                0,
                Vec::new(),
            )
            .queued(mir2_protocol::messages::RM_BUYITEM_SUCCESS),
        );
        BuyResult {
            frames,
            ok: true,
            tax,
        }
    } else {
        // `RM_BUYITEM_FAIL(0, n1C)` → `SM_BUYITEM_FAIL(Recog=n1C)`。
        frames.push(
            frame(
                mir2_protocol::messages::SM_BUYITEM_FAIL,
                n1c,
                0,
                0,
                0,
                Vec::new(),
            )
            .queued(mir2_protocol::messages::RM_BUYITEM_FAIL),
        );
        BuyResult {
            frames,
            ok: false,
            tax: None,
        }
    }
}

/// `Merchant.ClientQueryRepairCost`（`Merchant.cs:1555`）的出帧值（`Recog = nRepairPrice`）。
#[must_use]
pub fn query_repair_cost(m: &MerchantView, item: &mut BagItem, ctx: &ShopContext<'_>) -> ShopFrame {
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list, &m.item_price_list);
    let mut it = to_merchant(item);
    let is_super = eq_label(ctx.script_lable, LABEL_SUPER_REPAIR);
    let n_repair_price = shop::query_repair_cost(
        &t,
        &ctx.cfg,
        &mut it,
        is_super,
        m.flags.is_repair,
        m.flags.is_sup_repair,
    );
    item.dura_max = it.dura_max; // BUG-REPLICA：StdMode 43 的写回
    frame(
        mir2_protocol::messages::SM_SENDREPAIRCOST,
        n_repair_price,
        0,
        0,
        0,
        Vec::new(),
    )
    .queued(mir2_protocol::messages::RM_SENDREPAIRCOST)
}

/// `CM_USERREPAIRITEM` 的结果。
pub struct RepairResult {
    pub frames: Vec<ShopFrame>,
    /// 成功后待入账的商税意图（`Merchant.cs:1644-1652`）；失败/不支持 ⇒ `None`。
    pub tax: Option<MerchantTaxIntent>,
    /// 修理成功时 oracle `GotoLable(REPAIROK/SUPERREPAIROK)` 的标签（`Merchant.cs:1652/1659`）。
    /// 本模块只回**标签**；宿主 `handle_shop_cmd` 在 `Merchant.ScriptList` 上执行它并出说词帧
    /// （卡 `openmir2-merchant-script-load`）。
    pub goto_label: Option<&'static str>,
    /// `SendMsgToUser`（`@Superrepairfail` 支）的 `SM_MERCHANTSAY` 帧若产出，则在此。
    pub sent_to_user: Option<ShopFrame>,
}

/// `Merchant.ClientRepairItem`（`Merchant.cs:1599`）。
///
/// 三分支：不支持 ⇒ **无帧 return**；`@Superrepairfail` ⇒ `SendMsgToUser` + `RM_USERREPAIRITEM_FAIL`；
/// 其余按 `mir2_formula::shop::repair_item`（`Dura`/`DuraMax`/金币已写回 `item`）。
pub fn repair_item(
    m: &MerchantView,
    state: &mut GroundState,
    item_pos: usize,
    ctx: &ShopContext<'_>,
) -> RepairResult {
    let gold = state.gold;
    let mut it = to_merchant(&state.item_list[item_pos]);
    let is_super = eq_label(ctx.script_lable, LABEL_SUPER_REPAIR);
    let label = if is_super {
        shop::RepairLabel::SuperRepair
    } else if eq_label(ctx.script_lable, LABEL_SUPER_REPAIR_FAIL) {
        shop::RepairLabel::SuperRepairFail
    } else {
        shop::RepairLabel::Other
    };
    // 不支持分支：oracle 直接 `return`（无任何帧）。
    if (is_super && !m.flags.is_sup_repair) || (!is_super && !m.flags.is_repair) {
        return RepairResult {
            frames: Vec::new(),
            goto_label: None,
            sent_to_user: None,
            tax: None,
        };
    }
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list, &m.item_price_list);
    let out = shop::repair_item(
        &t,
        &ctx.cfg,
        &mut it,
        gold,
        label,
        m.flags.is_repair,
        m.flags.is_sup_repair,
    );
    let item = &mut state.item_list[item_pos];
    item.dura = out.dura;
    item.dura_max = out.dura_max;
    let mut frames = Vec::new();
    let mut sent_to_user = None;
    let mut goto_label = None;
    let mut tax = None;
    match out.outcome {
        shop::RepairOutcome::NoFrame => {}
        shop::RepairOutcome::Fail => {
            if label == shop::RepairLabel::SuperRepairFail {
                // `Merchant.ClientRepairItem`（`Merchant.cs:1612`）的 `SendMsgToUser`
                // （`NormNpc.cs:1613-1616`）= `SendMsg(this, RM_MERCHANTSAY, 0,0,0,0,
                // ChrName + '/' + sMsg)`；`RM_MERCHANTSAY` 映射（`PlayObject.Message.cs:1722-1724`）
                // = `MakeMessage(SM_MERCHANTSAY(643), ActorId, MakeWord(0,0), 0, 1)`
                // ⇒ ident=643、Recog=**商人** ActorId、Param=Tag=0、Series=1、尾 = `ChrName + '/' + sMsg`。
                let tail = format!("{}/{}", m.chr_name, SUPER_REPAIR_FAIL_MSG);
                sent_to_user = Some(
                    frame(
                        mir2_protocol::messages::SM_MERCHANTSAY,
                        m.actor_id,
                        0,
                        0,
                        1,
                        downlink::gbk(&tail),
                    )
                    .queued(mir2_protocol::messages::RM_MERCHANTSAY),
                );
            }
            frames.push(
                frame(
                    mir2_protocol::messages::SM_USERREPAIRITEM_FAIL,
                    0,
                    0,
                    0,
                    0,
                    Vec::new(),
                )
                .queued(mir2_protocol::messages::RM_USERREPAIRITEM_FAIL),
            );
        }
        shop::RepairOutcome::Ok => {
            state.gold = out.gold;
            // `Merchant.cs:1644-1652`：税基 = 本次实际扣掉的 `nRepairPrice` = `gold - out.gold`。
            let n_repair_price = gold - out.gold;
            tax = merchant_tax_intent(
                n_repair_price,
                m.castle_merchant,
                ctx.get_all_npc_tax,
                m.castle_index.is_some(),
                ctx.upgrade_weapon_price,
            );
            // `RM_USERREPAIRITEM_OK(0, Gold, Dura, DuraMax)` → `SM_USERREPAIRITEM_OK(Recog=Gold, Param=Dura, Tag=DuraMax)`。
            frames.push(
                frame(
                    mir2_protocol::messages::SM_USERREPAIRITEM_OK,
                    state.gold,
                    out.dura,
                    out.dura_max,
                    0,
                    Vec::new(),
                )
                .queued(mir2_protocol::messages::RM_USERREPAIRITEM_OK),
            );
            goto_label = Some(if is_super {
                LABEL_SUPER_REPAIR_OK
            } else {
                LABEL_REPAIR_OK
            });
        }
    }
    RepairResult {
        frames,
        goto_label,
        sent_to_user,
        tax,
    }
}

/// `BaseObject.RecalcBagWeight`（`BaseObject.cs:1356`）的商店路径等价物。
fn recalc_weight(catalog: &dyn ItemCatalog, item_list: &[BagItem]) -> u16 {
    let mut w: u16 = 0;
    for it in item_list {
        if let Some(t) = catalog.get(it.index) {
            w = w.wrapping_add(u16::from(t.weight));
        }
    }
    w
}

// ───────────────────── 武器升级 NPC 接缝（`@upgradenow` / `@getbackupgnow`） ─────────────────────
//
// oracle：`Merchant.cs:883`（`@upgradenow` → `UpgradeWapon`）/ `:890`（`@getbackupgnow` →
// `GetBackupgWeapon`）。判定/状态迁移/RNG 全在 `mir2_world::merchant`（**宿主不抽数**）；
// 本接缝只做「世界结果 → 帧 + 跳转标签」，并把宿主必须执行的动作钉在 [`UpgradeOutcome`]。

/// `ScriptFlagConst.sUPGRADENOW`（`ScriptFlagConst.cs:52`）。
pub const LABEL_UPGRADENOW: &str = "@upgradenow";
/// `ScriptFlagConst.sUPGRADEING`。
pub const LABEL_UPGRADENOW_ING: &str = "~@upgradenow_ing";
/// `ScriptFlagConst.sUPGRADEOK`。
pub const LABEL_UPGRADENOW_OK: &str = "~@upgradenow_ok";
/// `ScriptFlagConst.sUPGRADEFAIL`。
pub const LABEL_UPGRADENOW_FAIL: &str = "~@upgradenow_fail";
/// `ScriptFlagConst.sGETBACKUPGNOW`（`ScriptFlagConst.cs:56`）。
pub const LABEL_GETBACKUPGNOW: &str = "@getbackupgnow";
/// `ScriptFlagConst.sGETBACKUPGOK`。
pub const LABEL_GETBACKUPGNOW_OK: &str = "~@getbackupgnow_ok";
/// `ScriptFlagConst.sGETBACKUPGFAIL`。
pub const LABEL_GETBACKUPGNOW_FAIL: &str = "~@getbackupgnow_fail";
/// `ScriptFlagConst.sGETBACKUPGFULL`（部署脚本**没有**该标签 ⇒ `GotoLable` 空跳、无 say）。
pub const LABEL_GETBACKUPGNOW_BAGFULL: &str = "~@getbackupgnow_bagfull";
/// `ScriptFlagConst.sGETBACKUPGING`。
pub const LABEL_GETBACKUPGNOW_ING: &str = "~@getbackupgnow_ing";

/// 宿主需要执行的武器升级动作（`user_select` 判定，世界层执行）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpgradeRequest {
    /// `Merchant.UpgradeWapon`（`Merchant.cs:527`）。
    UpgradeWapon,
    /// `Merchant.GetBackupgWeapon`（`Merchant.cs:592`）。
    GetBackupgWeapon,
}

/// [`user_select`] 两条新分支的接缝结果（A5 定死的返回值）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeOutcome {
    /// 出帧（发送序；宿主按 vector 顺序直发，见 `emit_shop_frames`）。
    ///
    /// 提交成功 = `SM_DELITEM(202)` → `SM_ABILITY(52)` → `SM_FEATURECHANGED(41)` →
    /// `SM_GOLDCHANGED(653)` → `SM_DELITEMS(709)`。say 帧**不在**这里：它们由宿主在
    /// `GotoLable(goto_label)` 执行后按 `MsgQueue` 优先级排水（卡 `openmir2-merchant-script-load`）。
    ///
    /// **657**：本接缝只回上面 5 帧；oracle `RecalcAbilitys` 尾段的 `SM_CHARSTATUSCHANGED(657)×2`
    /// （`PlayObject.cs:2874-2876`）由宿主在 `653` 与 `709` 之间按发送序插入，近邻支另行广播
    /// （见 `lib.rs::charstatuschanged_shop_frame` / `broadcast_charstatuschanged_except_self`）。
    pub frames: Vec<ShopFrame>,
    /// `GotoLable(...)` 的标签名（`Merchant.cs:538/582/586/600/752/755/758`）。
    /// 本模块只回**标签**；宿主 `handle_shop_cmd` 在 `Merchant.ScriptList` 上执行并出说词帧。
    pub goto_label: Option<&'static str>,
    /// 本结果对应的宿主动作（`None` = 尚未执行；A5 的接缝标记）。
    pub upgrade_request: Option<UpgradeRequest>,
    /// `FeatureChanged()` 的 `(feature, feature_ex)`：`41` 已按**自己**收进 `frames`，
    /// 宿主还须对**同图 12 格内其它在线玩家**广播同字节（`SendRefMsg` 口径，含自己那条已在 frames）。
    pub feature_changed: Option<(i32, u16)>,
}

/// `Merchant.UserSelect` 的两条新分支（`Merchant.cs:883/890`）+ 脚本头标志门（`:889/896`）。
#[must_use]
pub fn upgrade_request_for(label: &str, m: &MerchantView) -> Option<UpgradeRequest> {
    if eq_label(label, LABEL_UPGRADENOW) && m.flags.is_upgradenow {
        return Some(UpgradeRequest::UpgradeWapon);
    }
    if eq_label(label, LABEL_GETBACKUPGNOW) && m.flags.is_get_backupgnow {
        return Some(UpgradeRequest::GetBackupgWeapon);
    }
    None
}

/// 武器升级的世界结果 → 帧 + 标签。
///
/// 帧字段逐条锚（简报 §2.6）：
/// - `202`：`SendDelItems` 直发（`Base.cs:1648-1665`）⇒ `Recog=ActorId`、尾 =
///   `EDCode.EncodeBuffer(ClientItem)`（**无** `StdModeMap` 的 Shape 改写）。
/// - `52`：`MakeMessage(SM_ABILITY, Gold, MakeWord(Job,99), LoWord(GameGold), HiWord(GameGold))`
///   尾 = `Encode(WAbil)`（`ability_tail` 的 40B/58B 口径由宿主传）。
/// - `41`：`FeatureChanged()`（`BaseObject.cs:1509`）⇒ `Recog=ActorId`、`Param/Tag=Lo/Hi(feature)`、
///   `Series=feature_ex`。
/// - `653`：`Recog=Gold`、`Param/Tag=Lo/Hi(GameGold)`、`Series=0`。
/// - `709`：`Series=条数`，尾 = `EncodeString("名/makeindex/…")`（[`del_items_frame`]）。
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn upgrade_wapon_outcome(
    outcome: &UpgradeWaponOutcome,
    weapon: Option<&MerchantItem>,
    player_actor_id: i32,
    job: u8,
    gold: i32,
    game_gold: i32,
    wabil_tail: &[u8],
    feature: (i32, u16),
    catalog: &dyn ItemCatalog,
) -> UpgradeOutcome {
    let mut frames: Vec<ShopFrame> = Vec::new();
    let (goto_label, feature_changed) = match outcome {
        UpgradeWaponOutcome::Ok(r) => {
            let tail = weapon
                .and_then(|w| crate::trade::del_item_tail(&to_bag(w), catalog))
                .unwrap_or_default();
            frames.push(frame_encoded(
                mir2_protocol::messages::SM_DELITEM,
                player_actor_id,
                0,
                0,
                1,
                tail,
            ));
            frames.push(
                frame_encoded(
                    mir2_protocol::messages::SM_ABILITY,
                    gold,
                    smframe::make_word(job, 99),
                    smframe::lo_word(game_gold),
                    smframe::hi_word(game_gold),
                    mir2_protocol::edcode::encode(wabil_tail),
                )
                .queued(mir2_protocol::messages::RM_ABILITY),
            );
            frames.push(
                frame_encoded(
                    mir2_protocol::messages::SM_FEATURECHANGED,
                    player_actor_id,
                    smframe::lo_word(feature.0),
                    smframe::hi_word(feature.0),
                    feature.1,
                    Vec::new(),
                )
                .queued(mir2_protocol::messages::RM_FEATURECHANGED),
            );
            frames.push(
                frame(
                    mir2_protocol::messages::SM_GOLDCHANGED,
                    gold,
                    smframe::lo_word(game_gold),
                    smframe::hi_word(game_gold),
                    0,
                    Vec::new(),
                )
                .queued(mir2_protocol::messages::RM_GOLDCHANGED),
            );
            frames.push(del_items_frame(&r.deleted));
            (Some(LABEL_UPGRADENOW_OK), Some(feature))
        }
        UpgradeWaponOutcome::AlreadyUpgrading => (Some(LABEL_UPGRADENOW_ING), None),
        UpgradeWaponOutcome::Fail => (Some(LABEL_UPGRADENOW_FAIL), None),
    };
    UpgradeOutcome {
        frames,
        goto_label,
        upgrade_request: Some(UpgradeRequest::UpgradeWapon),
        feature_changed,
    }
}

/// 取回的世界结果 → 帧 + 标签。
///
/// 成功只有一帧 `SM_ADDITEM(200)`（`SendAddItem` 直发，`Base.cs:198-227`）：`Recog=ActorId`、
/// 尾 = `EDCode.EncodeBuffer(ClientItem)`（含 `desc[10]`）。**不改 `ItemList`**
/// （oracle `AddItemToBag` 被注释的 BUG-REPLICA，`Merchant.cs:744`）。
#[must_use]
pub fn get_backupgnow_outcome(
    outcome: &GetBackupgWeaponOutcome,
    player_actor_id: i32,
    catalog: &dyn ItemCatalog,
) -> UpgradeOutcome {
    let mut frames: Vec<ShopFrame> = Vec::new();
    let goto_label = match outcome {
        GetBackupgWeaponOutcome::Ok(info) => {
            let tail =
                crate::trade::add_item_tail(&to_bag(&info.user_item), catalog).unwrap_or_default();
            frames.push(frame_encoded(
                mir2_protocol::messages::SM_ADDITEM,
                player_actor_id,
                0,
                0,
                1,
                tail,
            ));
            LABEL_GETBACKUPGNOW_OK
        }
        GetBackupgWeaponOutcome::BagFull => LABEL_GETBACKUPGNOW_BAGFULL,
        GetBackupgWeaponOutcome::NoRecord => LABEL_GETBACKUPGNOW_FAIL,
        GetBackupgWeaponOutcome::NotDue => LABEL_GETBACKUPGNOW_ING,
    };
    UpgradeOutcome {
        frames,
        goto_label: Some(goto_label),
        upgrade_request: Some(UpgradeRequest::GetBackupgWeapon),
        feature_changed: None,
    }
}

/// `PlayObject.GetScriptLabel`（`PlayObject.Base.cs:2775-2807`）的标签抽取：
/// 按 `\` 分行（`GetValidStr3` 的 `divider = '\\'`），每行每个 `<...>` 内按首个 `/` 切两段
/// （`RemoveEmptyEntries`），**取第二段**为标签（`<返回/@main>` ⇒ `@main`）。
///
/// 返回**去重保序**的标签表（`CanJmpScriptLableMap.Clear()` 后的新表）。
/// 生产调用点：宿主 `emit_merchant_goto_result` 对每条说词调用（`Market_Def` 脚本已装载，
/// 卡 `openmir2-merchant-script-load`）。
#[must_use]
pub fn get_script_label(say_text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in say_text.split('\\') {
        let mut rest = line;
        while let Some(start) = rest.find('<') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('>') else {
                break;
            };
            let inner = &after[..end];
            // `Split('/', 2, RemoveEmptyEntries)`：先切两段，再去掉空段。
            let segs: Vec<&str> = inner.splitn(2, '/').filter(|s| !s.is_empty()).collect();
            let label = if segs.len() > 1 { segs[1] } else { "" };
            if !label.is_empty() && !out.iter().any(|l| eq_label(l, label)) {
                out.push(label.to_string());
            }
            rest = &after[end + 1..];
        }
    }
    out
}

/// 商税入账目的地（**显式**；与 `merchant_tax_intent` 的税额计算分离）。
///
/// `Castle(i)` **自带下标** ⇒ 「intent=Castle 但下标指向另一座城」这种误用不再可表达；
/// `CastleManager` 无下标（oracle `CastleMgr` 恒升序入全部城，与任何具体城无关）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MerchantTaxDestination {
    /// 只入下标 `i` 的城；`None` = 无城可入（无副作用，绝不猜 0 号城）。
    Castle(Option<usize>),
    /// `CastleManager.cs:209-215`：**升序**入全部城，与任何下标无关。
    CastleManager,
}

/// 一次商税入账（目的地 + 金额），由 [`posting_for`] 从纯计算层**唯一**转换得到。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MerchantTaxPosting {
    pub destination: MerchantTaxDestination,
    pub amount: i32,
}

/// 纯计算层 `MerchantTaxIntent` ⇒ 显式入账参数（**唯一转换点**，卡 C-1）。
///
/// `Castle` 目标吞掉调用方的 `castle_index`（`None` 合法）；`CastleManager` 目标**丢弃**下标
/// （oracle 不看具体城）。`intent == None`（无税/前置不满足）⇒ `None`，绝不猜测目的地。
#[must_use]
pub fn posting_for(
    intent: Option<&MerchantTaxIntent>,
    castle_index: Option<usize>,
) -> Option<MerchantTaxPosting> {
    let intent = intent?;
    let destination = match intent.target {
        MerchantTaxTarget::Castle => MerchantTaxDestination::Castle(castle_index),
        MerchantTaxTarget::CastleManager => MerchantTaxDestination::CastleManager,
    };
    Some(MerchantTaxPosting {
        destination,
        amount: intent.amount,
    })
}

/// 把 [`MerchantTaxPosting`] 落到城堡金库（oracle `Castle.IncRateGold` / `CastleMgr.IncRateGold`）。
///
/// - `Castle(i)`：只入第 `i` 座城（`CastleManager.GetCastle` 口径；下标越界/`None` ⇒ 无副作用）；
/// - `CastleManager`：**升序**遍历全部城（`CastleManager.cs:209-215`）；
/// - `tick_ms` = `HUtil32.GetTickCount()` 口径；生产传 `world.clock_ms`（虚拟时钟）。
///
/// `posting == None`（无税/前置不满足）⇒ 返回空表，绝不猜测目的地。
#[must_use]
pub fn apply_merchant_tax(
    castles: &mut [UserCastle],
    posting: Option<MerchantTaxPosting>,
    cfg: &CastleTaxConfig,
    tick_ms: i64,
) -> Vec<IncRateGoldOutcome> {
    let Some(posting) = posting else {
        return Vec::new();
    };
    match posting.destination {
        MerchantTaxDestination::Castle(index) => index
            .and_then(|i| castles.get_mut(i))
            .map(|c| vec![c.inc_rate_gold(cfg, posting.amount, tick_ms)])
            .unwrap_or_default(),
        MerchantTaxDestination::CastleManager => castles
            .iter_mut()
            .map(|c| c.inc_rate_gold(cfg, posting.amount, tick_ms))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    /// **C9**：`@upgradenow`/`@getbackupgnow` 的脚本头标志门（`Merchant.cs:883-896`）+
    /// 标签比较 `OrdinalIgnoreCase`。漏标志检查 ⇒ 红。
    #[test]
    fn upgrade_request_respects_script_header_flags() {
        let view = |flags: MerchantFlags| MerchantView {
            actor_id: 5,
            flags,
            price_rate: 100,
            castle_merchant: false,
            castle_index: None,
            item_type_list: Vec::new(),
            item_price_list: Vec::new(),
            chr_name: "铁匠".to_string(),
        };
        let off = view(MerchantFlags::default());
        assert_eq!(
            upgrade_request_for("@upgradenow", &off),
            None,
            "IsUpgradenow=false ⇒ no-op"
        );
        assert_eq!(
            upgrade_request_for("@getbackupgnow", &off),
            None,
            "IsGetBackupgnow=false ⇒ no-op"
        );
        let on = view(MerchantFlags {
            is_upgradenow: true,
            is_get_backupgnow: true,
            ..MerchantFlags::default()
        });
        assert_eq!(
            upgrade_request_for("@UPGRADENOW", &on),
            Some(UpgradeRequest::UpgradeWapon),
            "标签比较 OrdinalIgnoreCase"
        );
        assert_eq!(
            upgrade_request_for("@getbackupgnow", &on),
            Some(UpgradeRequest::GetBackupgWeapon)
        );
        assert_eq!(upgrade_request_for("@buy", &on), None);
    }

    /// **C8b 登记**：say 帧（`SM_MERCHANTSAY(643)`）随 `Market_Def` 装载卡延期——
    /// 失败/未到期支**只回标签**、`frames` 为空（不是把 say 伪造成空尾帧）。
    #[test]
    fn upgrade_outcome_failure_labels_have_no_frames() {
        let catalog = crate::ability::StdItemCatalog::new();
        let ing = upgrade_wapon_outcome(
            &UpgradeWaponOutcome::AlreadyUpgrading,
            None,
            1,
            0,
            0,
            0,
            &[],
            (0, 0),
            &catalog,
        );
        assert!(ing.frames.is_empty(), "Market_Def 未装载 ⇒ 无 say 帧");
        assert_eq!(ing.goto_label, Some(LABEL_UPGRADENOW_ING));
        assert_eq!(ing.upgrade_request, Some(UpgradeRequest::UpgradeWapon));
        let fail = upgrade_wapon_outcome(
            &UpgradeWaponOutcome::Fail,
            None,
            1,
            0,
            0,
            0,
            &[],
            (0, 0),
            &catalog,
        );
        assert!(fail.frames.is_empty());
        assert_eq!(fail.goto_label, Some(LABEL_UPGRADENOW_FAIL));
        for (outcome, label) in [
            (
                GetBackupgWeaponOutcome::BagFull,
                LABEL_GETBACKUPGNOW_BAGFULL,
            ),
            (GetBackupgWeaponOutcome::NoRecord, LABEL_GETBACKUPGNOW_FAIL),
            (GetBackupgWeaponOutcome::NotDue, LABEL_GETBACKUPGNOW_ING),
        ] {
            let oc = get_backupgnow_outcome(&outcome, 1, &catalog);
            assert!(oc.frames.is_empty(), "{label} 无帧（say 未装载）");
            assert_eq!(oc.goto_label, Some(label));
            assert_eq!(oc.feature_changed, None);
        }
    }

    /// `GetScriptLabel`（`PlayObject.Base.cs:2775-2807`）：`<显示/@label>` 取 `/` 之后；
    /// `\` 分行；无 `/` 的条目不入表；去重保序。
    #[test]
    fn get_script_label_extracts_label_after_slash() {
        let got = get_script_label(r"请选择\<确认/@upgradenow> \<取消/@exit> \<无斜杠>");
        assert_eq!(
            got,
            vec!["@upgradenow".to_string(), "@exit".to_string()],
            "只收 `/` 之后的段，且去重保序"
        );
        let mut dice = None;
        assert!(lable_is_can_jmp(&[], &mut dice, "@main"), "@main 恒可跳");
        assert!(
            !lable_is_can_jmp(&[], &mut dice, "@upgradenow"),
            "未展示 ⇒ 不可跳"
        );
        assert!(
            lable_is_can_jmp(&got, &mut dice, "@upgradenow"),
            "展示过 ⇒ 可跳（OrdinalIgnoreCase 由 eq_label 保证）"
        );
    }

    /// `LableIsCanJmp` 第三支（`PlayObject.Base.cs:2824-2828`）：`PlayDiceLabel` 命中 ⇒ 放行并
    /// **一次性清空**（`PlayDiceLabel = string.Empty`）；`None`（C# 未初始化 = null）永不匹配；
    /// `@main` 支先于第三支且**不消费**它。
    ///
    /// 牙：删掉第三支 ⇒ 本用例红（`@DICE` 不再可跳、`@dice` 第二次仍可跳）。
    #[test]
    fn play_dice_label_branch_is_one_shot_and_none_never_matches() {
        let mut none: Option<String> = None;
        assert!(!lable_is_can_jmp(&[], &mut none, ""));
        assert_eq!(none, None, "`None` 不参与比较、不被改写");
        let mut dice = Some("@dice".to_string());
        assert!(
            lable_is_can_jmp(&[], &mut dice, "@DICE"),
            "OrdinalIgnoreCase 命中"
        );
        assert_eq!(dice, Some(String::new()), "命中即清 `string.Empty`");
        assert!(!lable_is_can_jmp(&[], &mut dice, "@dice"), "只能跳一次");
        // `@main` 分支（`Base.cs:2816`）先于第三支，且不消费 `PlayDiceLabel`。
        let mut dice = Some("@main".to_string());
        assert!(lable_is_can_jmp(&[], &mut dice, "@main"));
        assert_eq!(dice, Some("@main".to_string()), "`@main` 分支不消费");
    }

    use super::*;
    use crate::ability::StdItemCatalog;
    use mir2_data::models::StdItem;

    fn catalog() -> StdItemCatalog {
        let mut c = StdItemCatalog::new();
        c.insert(
            1,
            StdItem {
                name: "金创药(小量)".to_string(),
                price: 100,
                weight: 3,
                std_mode: 0,
                dura_max: 10,
                ..StdItem::default()
            },
        );
        c.insert(
            2,
            StdItem {
                name: "木剑".to_string(),
                price: 30,
                weight: 5,
                std_mode: 5,
                dura_max: 20,
                ..StdItem::default()
            },
        );
        c
    }

    fn ability(weight: u16, max_weight: u16) -> crate::ability::Ability {
        crate::ability::Ability {
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
            weight,
            max_weight,
            wear_weight: 0,
            max_wear_weight: 0,
            hand_weight: 0,
            max_hand_weight: 0,
        }
    }

    fn merchant(flags: MerchantFlags) -> MerchantView {
        MerchantView {
            actor_id: 4242,
            flags,
            price_rate: 100,
            castle_merchant: false,
            castle_index: None,
            item_type_list: vec![0, 5],
            item_price_list: Vec::new(),
            chr_name: "商人".to_string(),
        }
    }

    fn mflags(buy: bool, sell: bool) -> MerchantFlags {
        MerchantFlags {
            is_buy: buy,
            is_sell: sell,
            ..MerchantFlags::default()
        }
    }

    fn ctx<'a>(catalog: &'a dyn ItemCatalog) -> ShopContext<'a> {
        ShopContext {
            catalog,
            cfg: ShopConfig::default(),
            get_all_npc_tax: false,
            upgrade_weapon_price: 10_000,
            script_lable: "",
            human_max_gold: 1_000_000,
            player_actor_id: 4242,
            red_f: 0xFF,
            red_b: 0x38,
            make_durg_price: 100,
        }
    }

    #[test]
    fn user_select_buy_emits_goods_list_with_price_and_stock() {
        let cat = catalog();
        let c = ctx(&cat);
        let m = merchant(mflags(true, false));
        let goods = vec![vec![MerchantItem {
            make_index: 7,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: [0; 14],
        }]];
        let out = user_select(LABEL_BUY, &m, &goods, &[], &c);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].ident, 645);
        assert_eq!(out[0].recog, 4242);
        assert_eq!(out[0].param, 1);
        assert_eq!(out[0].tail, downlink::gbk("金创药(小量)/0/100/1/"));
    }

    #[test]
    fn user_select_sell_and_repair_panels() {
        let cat = catalog();
        let c = ctx(&cat);
        let m = merchant(mflags(false, true));
        assert_eq!(user_select(LABEL_SELL, &m, &[], &[], &c)[0].ident, 646);
        let mut m2 = merchant(MerchantFlags::default());
        m2.flags.is_repair = true;
        assert_eq!(user_select(LABEL_REPAIR, &m2, &[], &[], &c)[0].ident, 668);
        m2.flags.is_sup_repair = true;
        assert_eq!(
            user_select(LABEL_SUPER_REPAIR, &m2, &[], &[], &c)[0].ident,
            668
        );
    }

    #[test]
    fn sell_moves_item_to_goods_and_adds_gold() {
        let cat = catalog();
        let c = ctx(&cat);
        let m = merchant(mflags(false, true));
        let mut state = GroundState {
            item_list: vec![BagItem {
                make_index: 9,
                index: 1,
                dura: 10,
                dura_max: 10,
                desc: [0; 14],
            }],
            gold: 1000,
            wabil: ability(3, 100),
        };
        let mut goods = Vec::new();
        let r = sell_item(&m, &mut state, &mut goods, 9, "金创药(小量)", &c);
        assert!(r.ok);
        assert_eq!(r.frames[0].ident, 648);
        assert_eq!(r.frames[0].recog, 1050, "GetSellItemPrice(100)=50");
        assert!(state.item_list.is_empty());
        assert_eq!(state.gold, 1050);
        assert_eq!(goods.len(), 1);
        assert_eq!(goods[0][0].index, 1);
        assert_eq!(state.wabil.weight, 0, "WeightChanged");
    }

    #[test]
    fn sell_fails_when_gold_over_max() {
        let cat = catalog();
        let mut c = ctx(&cat);
        c.human_max_gold = 1040;
        let m = merchant(mflags(false, true));
        let mut state = GroundState {
            item_list: vec![BagItem {
                make_index: 9,
                index: 1,
                dura: 10,
                dura_max: 10,
                desc: [0; 14],
            }],
            gold: 1000,
            wabil: ability(3, 100),
        };
        let mut goods = Vec::new();
        let r = sell_item(&m, &mut state, &mut goods, 9, "金创药(小量)", &c);
        assert!(!r.ok, "1050 > 1040 ⇒ IncGold 失败");
        assert_eq!(r.frames[0].ident, 649);
        assert!(goods.is_empty());
        assert_eq!(state.gold, 1000);
    }

    #[test]
    fn buy_failure_codes_two_and_three() {
        let cat = catalog();
        let c = ctx(&cat);
        let m = merchant(mflags(true, false));
        // 负重不足 ⇒ n1C = 2。
        let mut state = GroundState {
            item_list: Vec::new(),
            gold: 10_000,
            wabil: ability(98, 100),
        };
        let mut goods = vec![vec![MerchantItem {
            make_index: 7,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: [0; 14],
        }]];
        let r = buy_item(&m, &mut state, &mut goods, "金创药(小量)", 0, &c);
        assert!(!r.ok);
        assert_eq!(r.frames[0].ident, 651);
        assert_eq!(r.frames[0].recog, 2);
        assert!(
            r.frames.iter().all(|f| f.ident != 650),
            "失败分支不得发 SM_BUYITEM_SUCCESS(650)"
        );
        // 钱不够 ⇒ n1C = 3。
        state.wabil = ability(0, 100);
        state.gold = 0;
        let r = buy_item(&m, &mut state, &mut goods, "金创药(小量)", 0, &c);
        assert_eq!(r.frames[0].recog, 3);
        assert!(
            r.frames.iter().all(|f| f.ident != 650),
            "金币不足分支不得发 SM_BUYITEM_SUCCESS(650)"
        );
        // 成功 ⇒ n1C = 0，金币扣、库存减、背包加。
        state.gold = 500;
        let r = buy_item(&m, &mut state, &mut goods, "金创药(小量)", 0, &c);
        assert!(r.ok);
        let ok650 = r
            .frames
            .iter()
            .find(|f| f.ident == 650)
            .expect("成功购买必须发 SM_BUYITEM_SUCCESS(650)（oracle Merchant.cs:1303）");
        assert_eq!(
            ok650.recog, state.gold,
            "650.recog = Gold（PlayObject.Message.cs:1825）"
        );
        let add = r
            .frames
            .iter()
            .find(|f| f.ident == 200)
            .expect("成功购买必须发 SM_ADDITEM");
        assert!(
            add.tail_encoded,
            "ClientItem 尾已编码 ⇒ 不得再 EncodeString"
        );
        let ci = crate::itemwire::ClientItem {
            tpl: cat.get(1).expect("模板 1"),
            name: "金创药(小量)".to_string(),
            make_index: 7,
            dura: 10,
            dura_max: 10,
            desc: [0; 14],
        };
        assert_eq!(
            add.tail,
            mir2_protocol::edcode::encode(&crate::itemwire::encode_client_item(&ci)),
            "G-1：尾 = EncodeBuffer(ClientItem 124B)"
        );
        assert_eq!(add.tail.len(), 166, "encode_len(124)=166");
        // 622 夹在 200 与 650 之间（直连 200 先出；622 优先级 131 < 650 的 149）。
        let idents: Vec<u16> = r.frames.iter().map(|f| f.ident).collect();
        assert_eq!(
            idents,
            vec![
                mir2_protocol::messages::SM_ADDITEM,
                mir2_protocol::messages::SM_WEIGHTCHANGED,
                mir2_protocol::messages::SM_BUYITEM_SUCCESS,
            ],
            "帧序：SM_ADDITEM（直连）→ SM_WEIGHTCHANGED(131) → SM_BUYITEM_SUCCESS(149)"
        );
        let changed = &r.frames[1];
        assert_eq!(
            changed.recog,
            i32::from(state.wabil.weight),
            "622.Recog = 重算后 WAbil.Weight"
        );
        assert_eq!(
            (changed.param, changed.tag),
            (
                u16::from(state.wabil.wear_weight),
                u16::from(state.wabil.hand_weight)
            ),
            "622.Param/Tag = WearWeight/HandWeight"
        );
        let sum = i32::from(state.wabil.weight)
            + i32::from(state.wabil.wear_weight)
            + i32::from(state.wabil.hand_weight);
        assert_eq!(
            changed.series,
            (((sum ^ 0x3A5F) ^ 0x1F35) ^ 0xaa21) as u16,
            "622.Series = ((W+A+H)^0x3A5F)^0x1F35^0xaa21"
        );
        assert_eq!(
            changed.queue_priority,
            Some((mir2_protocol::messages::RM_WEIGHTCHANGED & 0xFF) as u8),
            "622 走 RM_WEIGHTCHANGED 队列（131）"
        );
        assert_eq!(r.frames.last().unwrap().ident, 650);
        assert_eq!(state.gold, 400);
        assert_eq!(state.item_list.len(), 1);
        assert!(goods.is_empty(), "空组移除");

        // **负例（满包）**：`AddItemToBag` 的 `return false` 在 `ItemList.Add` 之前
        // （`BaseObject.cs:1241-1242`）⇒ 既不入包、也不 `WeightChanged` ⇒ **不得发 622**。
        let mut full = GroundState {
            item_list: (0..game_svr::bag::MAX_BAG_ITEM)
                .map(|i| bag(2000 + i as i32, 1))
                .collect(),
            gold: 10_000,
            wabil: ability(0, 1000),
        };
        let mut goods_full = vec![vec![MerchantItem {
            make_index: 9,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: [0; 14],
        }]];
        let rf = buy_item(&m, &mut full, &mut goods_full, "金创药(小量)", 0, &c);
        assert!(!rf.ok);
        assert!(
            rf.frames
                .iter()
                .all(|f| f.ident != mir2_protocol::messages::SM_WEIGHTCHANGED),
            "满包失败不得发 622"
        );
        assert_eq!(rf.frames[0].recog, 2, "背包满 ⇒ n1C=2");
        assert_eq!(
            full.item_list.len(),
            game_svr::bag::MAX_BAG_ITEM,
            "背包未被改动"
        );
        assert_eq!(full.gold, 10_000, "金币未扣");
    }

    /// 卡 `openmir2-merchant-price-list-and-weight-frame` C 阶段：宿主 `MerchantView.item_price_list`
    /// 必须**真的**传给 `PriceTables`（`get_item_price` 命中 `ItemPriceList` 优先于 `StdItem.Price`）。
    ///
    /// 牙：把 `price_tables(&source, &m.item_type_list, &m.item_price_list)` 改回第三参数 `&[]`
    /// ⇒ 买价回落 100 ⇒ 金币 400（而非 412）、650.Recog 也差 12 ⇒ 本用例红。
    #[test]
    fn merchant_item_price_list_drives_buy_price_and_gold() {
        let cat = catalog();
        let c = ctx(&cat);
        let mut m = merchant(mflags(true, false));
        // 补货路径会写的就是这条：`CheckItemPrice` ⇒ `Round(StdItem.Price 100 * 1.1) = 110`。
        // 价格表内容由世界侧补齐（另有世界侧牙），本用例只验「宿主把它传进公式」。
        m.item_price_list = vec![mir2_formula::shop::ItemPrice {
            w_index: 1,
            n_price: 110.0,
        }];
        let mut state = GroundState {
            item_list: Vec::new(),
            gold: 500,
            wabil: ability(0, 100),
        };
        let mut goods = vec![vec![MerchantItem {
            make_index: 7,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: [0; 14],
        }]];
        let r = buy_item(&m, &mut state, &mut goods, "金创药(小量)", 0, &c);
        assert!(r.ok);
        assert_eq!(
            state.gold, 390,
            "500 - ItemPriceList 的 110（不是 StdItem.Price 的 100）"
        );
        let ok650 = r
            .frames
            .iter()
            .find(|f| f.ident == mir2_protocol::messages::SM_BUYITEM_SUCCESS)
            .expect("成功必须发 650");
        assert_eq!(ok650.recog, 390, "650.Recog = 扣款后 Gold（随价格表走）");
    }

    #[test]
    fn query_sell_price_and_repair_cost_frames() {
        let cat = catalog();
        let mut c = ctx(&cat);
        let m = merchant(mflags(false, true));
        let mut it = BagItem {
            make_index: 9,
            index: 1,
            dura: 4,
            dura_max: 10,
            desc: [0; 14],
        };
        assert_eq!(query_sell_price(&m, &mut it, &c).recog, 50);
        c.script_lable = LABEL_REPAIR;
        let mr = merchant(MerchantFlags {
            is_repair: true,
            ..MerchantFlags::default()
        });
        let mut it2 = BagItem {
            make_index: 9,
            index: 1,
            dura: 4,
            dura_max: 10,
            desc: [0; 14],
        };
        // `GetUserPrice(GetUserItemPrice)` = 100（StdMode 0）；`Round((100/3)/10*6)` = 20。
        assert_eq!(query_repair_cost(&mr, &mut it2, &c).recog, 20);
        // 不支持修理 ⇒ -1（`is_repair == false`）。
        let m2 = merchant(mflags(false, true));
        assert_eq!(query_repair_cost(&m2, &mut it2, &c).recog, -1);
    }

    #[test]
    fn repair_ok_and_superrepairfail_side_effects() {
        let cat = catalog();
        let mut c = ctx(&cat);
        c.script_lable = LABEL_REPAIR;
        let m = merchant(MerchantFlags {
            is_repair: true,
            ..MerchantFlags::default()
        });
        let mut state = GroundState {
            item_list: vec![BagItem {
                make_index: 9,
                index: 1,
                dura: 4,
                dura_max: 10,
                desc: [0; 14],
            }],
            gold: 10_000,
            wabil: ability(0, 100),
        };
        let r = repair_item(&m, &mut state, 0, &c);
        // `DuraMax -= (10-4)/30 = 0` ⇒ `Dura = DuraMax = 10`；`Round((100/3)/10*6)=20`。
        assert_eq!(r.frames[0].ident, 669);
        assert_eq!(r.frames[0].recog, 9_980);
        assert_eq!(state.item_list[0].dura, 10);
        assert_eq!(r.goto_label, Some(LABEL_REPAIR_OK));
        assert!(r.sent_to_user.is_none());

        // `@fail_s_repair` ⇒ `SendMsgToUser`（`SM_MERCHANTSAY(643)`、Recog=**商人** ActorId）+ FAIL
        // （无 `GotoLable`）。商人实体 id 与玩家 id 取不同值，钉死「Recog 是商人不是玩家」。
        c.script_lable = LABEL_SUPER_REPAIR_FAIL;
        let m = MerchantView {
            actor_id: 5_555,
            flags: MerchantFlags {
                is_repair: true,
                ..MerchantFlags::default()
            },
            price_rate: 100,
            castle_merchant: false,
            castle_index: None,
            item_type_list: vec![0, 5],
            item_price_list: Vec::new(),
            chr_name: "商人乙".to_string(),
        };
        let mut state2 = GroundState {
            item_list: vec![BagItem {
                make_index: 9,
                index: 1,
                dura: 4,
                dura_max: 10,
                desc: [0; 14],
            }],
            gold: 10_000,
            wabil: ability(0, 100),
        };
        let r = repair_item(&m, &mut state2, 0, &c);
        assert_eq!(r.frames[0].ident, 670);
        let su = r
            .sent_to_user
            .expect("@Superrepairfail 必须发 SendMsgToUser");
        assert_eq!(su.ident, mir2_protocol::messages::SM_MERCHANTSAY);
        assert_eq!(su.recog, 5_555, "Recog = 商人 ActorId（不是玩家）");
        assert_eq!((su.param, su.tag, su.series), (0, 0, 1));
        assert_eq!(
            su.tail,
            downlink::gbk(&format!("商人乙/{SUPER_REPAIR_FAIL_MSG}")),
            "尾 = ChrName + '/' + sMsg（`NormNpc.cs:1613-1616`）"
        );
        assert!(r.goto_label.is_none());
        assert_eq!(state2.gold, 10_000, "失败不动金币");

        // 不支持修理 ⇒ 无帧（`repair_item` 的直接 return）。
        let m_none = merchant(MerchantFlags::default());
        let mut state3 = GroundState {
            item_list: vec![BagItem {
                make_index: 1,
                index: 1,
                dura: 1,
                dura_max: 10,
                desc: [0; 14],
            }],
            gold: 0,
            wabil: ability(0, 100),
        };
        let r = repair_item(&m_none, &mut state3, 0, &c);
        assert!(r.frames.is_empty(), "不支持 ⇒ 无帧 return");
    }

    fn bag(make_index: i32, index: u16) -> BagItem {
        BagItem {
            make_index,
            index,
            dura: 5,
            dura_max: 10,
            desc: [0; 14],
        }
    }

    /// `ClientGetDetailGoodsList`（`Merchant.cs:1304`）：从组尾往前取 **最多 10 件**；
    /// 尾 = 每件 `EncodeBuffer(ClientItem) + '/'`（未编码原文，`tail_encoded=false`）；
    /// `nInt` 超过 `Count-1` 时夹到 `max(0, Count-10)`（`Merchant.cs:1320-1323`）。
    /// 改坏（`n_item_count >= 10` → `>= 11` 或去掉取尾循环）⇒ 本用例红。
    #[test]
    fn detail_goods_list_encodes_last_ten_items_and_page_offset() {
        let cat = catalog();
        let c = ctx(&cat);
        let m = merchant(mflags(true, false));
        let goods: Vec<MerchantItem> = (0..12)
            .map(|k| MerchantItem {
                make_index: 900 + k,
                index: 1,
                dura: 5,
                dura_max: 10,
                desc: [0; 14],
            })
            .collect();
        let mut goods = vec![goods];
        let f = detail_goods_list(&m, &mut goods, "金创药(小量)", 0, &c);
        assert_eq!(f.ident, mir2_protocol::messages::SM_SENDDETAILGOODSLIST);
        assert_eq!(f.recog, 4242, "Recog = 商人 ActorId");
        assert_eq!(f.param, 10, "最多 10 件");
        assert_eq!(f.tag, 0, "nInt=0 未超界 ⇒ 不夹");
        assert!(
            !f.tail_encoded,
            "详情尾经 SendDefMessage 再施加 EncodeString"
        );
        // 尾字节范围 [0x3C,0x7B] ⇒ 内部不含 '/'（0x2F）⇒ 可按 '/' 安全切分。
        let chunks: Vec<&[u8]> = f.tail.split(|b| *b == b'/').collect();
        assert_eq!(chunks.len(), 11, "10 件 + 尾部空段");
        for (k, ch) in chunks[..10].iter().enumerate() {
            let buf = mir2_protocol::edcode::decode(ch);
            assert_eq!(buf.len(), 124, "ClientItem 定长 124B");
            let want = (911 - k as i32).to_le_bytes();
            assert_eq!(
                &buf[100..104],
                &want,
                "第 {k} 件应取组尾往前（make_index 911..902）"
            );
        }
        // `nInt` 超界 ⇒ 夹到 `max(0, Count-10) = 2`（只改回帧 Tag，不改取的件）。
        let f2 = detail_goods_list(&m, &mut goods, "金创药(小量)", 100, &c);
        assert_eq!((f2.param, f2.tag), (10, 2), "nInt 夹到 Count-10");
        // 模板名不符（ordinal 区分大小写）⇒ 不发件（header-only）。
        let f3 = detail_goods_list(&m, &mut goods, "金创药", 0, &c);
        assert_eq!((f3.param, f3.tail.len()), (0, 0));
    }

    /// `SendSaveItemList`（`PlayObject.Base.cs:1590`）：每页 50、`Tag=p`、`Series=page-1`；
    /// 尾是 `SendSocket` 直连（`tail_encoded=true`）；空仓库 ⇒ 0 帧（`page=0`）。
    #[test]
    fn back_item_list_pages_fifty_and_tags_series() {
        let cat = catalog();
        let c = ctx(&cat);
        let items: Vec<BagItem> = (0..51).map(|k| bag(100 + k, 1)).collect();
        let out = back_item_list(4242, &items, &c);
        assert_eq!(out.len(), 2, "51 件 ⇒ ceil(51/50)=2 页");
        assert_eq!(
            (
                out[0].ident,
                out[0].recog,
                out[0].param,
                out[0].tag,
                out[0].series
            ),
            (704, 4242, 0, 0, 1)
        );
        assert_eq!((out[1].tag, out[1].series), (1, 1), "Tag=p，Series=page-1");
        assert!(
            out.iter().all(|f| f.tail_encoded),
            "分页尾走 SendSocket 直连"
        );
        assert_eq!(out[0].tail.split(|b| *b == b'/').count() - 1, 50);
        assert_eq!(out[1].tail.split(|b| *b == b'/').count() - 1, 1);
        assert!(back_item_list(4242, &[], &c).is_empty(), "空仓库 ⇒ 0 帧");
    }

    /// `ClientStorageItem`（`Operate.cs:1607`）：存入（39 上限）+ 背包移除 + 负重重算；
    /// 仓满 ⇒ `SM_STORAGE_FULL`；商人无 `IsStorage` ⇒ `SM_STORAGE_FAIL`（件不动）。
    #[test]
    fn storage_item_moves_to_storage_and_reports_full_or_fail() {
        let cat = catalog();
        let c = ctx(&cat);
        let mut state = GroundState {
            item_list: vec![bag(9, 1)],
            gold: 0,
            wabil: ability(3, 100),
        };
        let mut storage: Vec<BagItem> = Vec::new();
        let r = storage_item(&mut state, &mut storage, true, true, 9, "金创药(小量)", &c);
        assert!(r.ok);
        assert_eq!(r.frames.len(), 1);
        assert_eq!(r.frames[0].ident, mir2_protocol::messages::SM_STORAGE_OK);
        assert!(state.item_list.is_empty(), "背包件已移出");
        assert_eq!(storage.len(), 1);
        assert_eq!(state.wabil.weight, 0, "WeightChanged 重算");
        // 仓满。
        let mut state2 = GroundState {
            item_list: vec![bag(9, 1)],
            gold: 0,
            wabil: ability(3, 100),
        };
        let mut full: Vec<BagItem> = (0..39).map(|k| bag(k, 1)).collect();
        let r2 = storage_item(&mut state2, &mut full, false, true, 9, "金创药(小量)", &c);
        assert_eq!(r2.frames[0].ident, mir2_protocol::messages::SM_STORAGE_FULL);
        assert!(r2.ok, "oracle 满支也置 bo19=true（`Operate.cs:1644-1645`）");
        assert_eq!(state2.item_list.len(), 1, "满 ⇒ 件不动");
        assert_eq!(full.len(), 39);
        // 商人无 IsStorage ⇒ FAIL（件不动）。
        let mut state3 = GroundState {
            item_list: vec![bag(9, 1)],
            gold: 0,
            wabil: ability(3, 100),
        };
        let mut storage3: Vec<BagItem> = Vec::new();
        let r3 = storage_item(
            &mut state3,
            &mut storage3,
            true,
            false,
            9,
            "金创药(小量)",
            &c,
        );
        assert_eq!(r3.frames[0].ident, mir2_protocol::messages::SM_STORAGE_FAIL);
        assert_eq!(state3.item_list.len(), 1);
        // 名不符 ⇒ FAIL。
        let r4 = storage_item(&mut state3, &mut storage3, true, true, 9, "木剑", &c);
        assert_eq!(r4.frames[0].ident, mir2_protocol::messages::SM_STORAGE_FAIL);
    }

    /// `ClientTakeBackStorageItem`（`Operate.cs:1656`）：取回 = `SM_ADDITEM` + `SM_TAKEBACKSTORAGEITEM_OK(nItemIdx)`；
    /// 负重不足 ⇒ `SysMsg(CanotGetItems, Red)` + `SM_TAKEBACKSTORAGEITEM_FAIL`；背包满 ⇒ `_FULLBAG`。
    /// 改坏（交换 ADDITEM/OK 顺序或删负重门）⇒ 本用例红。
    #[test]
    fn take_back_storage_item_adds_to_bag_and_weight_denies() {
        let cat = catalog();
        let c = ctx(&cat);
        let mut state = GroundState {
            item_list: Vec::new(),
            gold: 0,
            wabil: ability(0, 100),
        };
        let mut storage = vec![bag(7, 1)];
        let r = take_back_storage_item(&mut state, &mut storage, true, 7, "金创药(小量)", &c);
        assert!(r.ok);
        assert_eq!(r.frames.len(), 2);
        assert_eq!(r.frames[0].ident, mir2_protocol::messages::SM_ADDITEM);
        assert_eq!(
            r.frames[0].recog, 4242,
            "SendAddItem 的 Recog = 玩家 ActorId"
        );
        assert_eq!(
            r.frames[1].ident,
            mir2_protocol::messages::SM_TAKEBACKSTORAGEITEM_OK
        );
        assert_eq!(r.frames[1].recog, 7, "OK 帧 Recog = nItemIdx");
        assert_eq!(state.item_list.len(), 1);
        assert!(storage.is_empty());
        assert_eq!(state.wabil.weight, 3, "取回后重算负重");

        // 负重不足 ⇒ SysMsg + FAIL（件不动）。
        let mut state2 = GroundState {
            item_list: Vec::new(),
            gold: 0,
            wabil: ability(2, 2),
        };
        let mut storage2 = vec![bag(7, 1)];
        let r2 = take_back_storage_item(&mut state2, &mut storage2, true, 7, "金创药(小量)", &c);
        assert!(!r2.ok);
        assert_eq!(r2.frames.len(), 2);
        assert_eq!(r2.frames[0].ident, mir2_protocol::messages::SM_SYSMESSAGE);
        assert_eq!(r2.frames[0].recog, 4242);
        assert_eq!(r2.frames[0].param, smframe::make_word(0xFF, 0x38));
        assert_eq!(r2.frames[0].tail, downlink::gbk(CANOT_GET_ITEMS_MSG));
        assert_eq!(
            r2.frames[1].ident,
            mir2_protocol::messages::SM_TAKEBACKSTORAGEITEM_FAIL
        );
        assert_eq!(state2.item_list.len(), 0);
        assert_eq!(storage2.len(), 1);

        // 背包满 ⇒ `_FULLBAG`。
        let mut state3 = GroundState {
            item_list: (0..MAX_BAG_ITEM as i32).map(|k| bag(k, 1)).collect(),
            gold: 0,
            wabil: ability(0, 100),
        };
        let mut storage3 = vec![bag(7, 1)];
        let r3 = take_back_storage_item(&mut state3, &mut storage3, true, 7, "金创药(小量)", &c);
        assert_eq!(
            r3.frames[0].ident,
            mir2_protocol::messages::SM_TAKEBACKSTORAGEITEM_FULLBAG
        );
        assert_eq!(storage3.len(), 1, "满 ⇒ 件仍在仓库");

        // 商人无 IsGetback ⇒ 仅 FAIL（无 SysMsg/ADDITEM）。
        let mut state4 = GroundState {
            item_list: Vec::new(),
            gold: 0,
            wabil: ability(0, 100),
        };
        let mut storage4 = vec![bag(7, 1)];
        let r4 = take_back_storage_item(&mut state4, &mut storage4, false, 7, "金创药(小量)", &c);
        assert_eq!(r4.frames.len(), 1);
        assert_eq!(
            r4.frames[0].ident,
            mir2_protocol::messages::SM_TAKEBACKSTORAGEITEM_FAIL
        );
    }

    /// `UserSelect` 的 `@storage`/`@getback` 两支（`Merchant.cs:1024/1029`）。
    #[test]
    fn user_select_storage_and_getback_panels() {
        let cat = catalog();
        let c = ctx(&cat);
        let mut m = merchant(MerchantFlags::default());
        m.flags.is_storage = true;
        let out = user_select(LABEL_STORAGE, &m, &[], &[], &c);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].ident,
            mir2_protocol::messages::SM_SENDUSERSTORAGEITEM
        );
        assert_eq!(out[0].recog, 4242);
        m.flags.is_storage = false;
        m.flags.is_getback = true;
        let items = vec![bag(5, 1)];
        let out = user_select(LABEL_GETBACK, &m, &[], &items, &c);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].ident, mir2_protocol::messages::SM_SAVEITEMLIST);
        assert_eq!(out[0].recog, 4242);
        // flag 不满足 ⇒ 无帧。
        let m_none = merchant(MerchantFlags::default());
        assert!(user_select(LABEL_STORAGE, &m_none, &[], &[], &c).is_empty());
        assert!(user_select(LABEL_GETBACK, &m_none, &[], &items, &c).is_empty());
    }

    // ---- 制药片 WB：`@makedrug` 面板 + `CM_USERMAKEDRUGITEM` 全链（R 级，见 rationale） ----

    /// 把一条 [`ShopFrame`] 编成线上 body = `emit_shop_frames` 的字节等价
    /// （`command_header` + `tail_encoded ? tail : EncodeString(tail)`）。
    fn body_of(f: &ShopFrame) -> Vec<u8> {
        let header = crate::smframe::command_header(f.recog, f.ident, f.param, f.tag, f.series);
        let tail = if f.tail_encoded {
            f.tail.clone()
        } else {
            mir2_protocol::edcode::encode(&f.tail)
        };
        crate::smframe::gm_data_body(header, &tail)
    }

    fn mflags_makedrug() -> MerchantFlags {
        MerchantFlags {
            is_make_drug: true,
            ..MerchantFlags::default()
        }
    }

    fn drug_catalog() -> StdItemCatalog {
        let mut c = catalog();
        c.insert(
            3,
            StdItem {
                name: "金创药".to_string(),
                weight: 3,
                std_mode: 0,
                dura_max: 12,
                ..StdItem::default()
            },
        );
        c.insert(
            4,
            StdItem {
                name: "草药".to_string(),
                weight: 1,
                std_mode: 0,
                dura_max: 5,
                ..StdItem::default()
            },
        );
        c.insert(
            5,
            StdItem {
                name: "布衣".to_string(),
                weight: 1,
                std_mode: 0,
                dura_max: 5,
                ..StdItem::default()
            },
        );
        c
    }

    fn make_reg(text: &str) -> MakeItemRegistry {
        mir2_envir::makeitem::parse_make_item_lines(text).unwrap()
    }

    fn mbag(index: u16, make_index: i32) -> BagItem {
        BagItem {
            make_index,
            index,
            dura: 0,
            dura_max: 0,
            desc: [0; 14],
        }
    }

    fn ground(gold: i32, items: Vec<BagItem>) -> GroundState {
        GroundState {
            item_list: items,
            gold,
            wabil: ability(100, 200),
        }
    }

    fn goods_one(index: u16) -> Vec<Vec<MerchantItem>> {
        vec![vec![MerchantItem {
            make_index: 9,
            index,
            dura: 12,
            dura_max: 12,
            desc: [0; 14],
        }]]
    }

    /// `@makedrug`（`Merchant.cs:1002-1017`）：`SM_SENDUSERMAKEDRUGITEMLIST(712)`。
    /// `Recog = nParam1 = ActorId`（商人）、`Param = nParam2 = 0`；尾 = `名/0/价/1/` × N（GBK + EncodeString）。
    ///
    /// 变异必红：① recog/param 互换 ② 漏 `MakeDurgPrice` 段（写成占位 0）③ 把 `IsMakeDrug` 门去掉。
    #[test]
    fn user_select_makedrug_emits_item_list_frame() {
        let cat = drug_catalog();
        let c = ctx(&cat);
        let m = merchant(mflags_makedrug());
        let goods = vec![
            vec![MerchantItem {
                make_index: 7,
                index: 3,
                dura: 12,
                dura_max: 12,
                desc: [0; 14],
            }],
            vec![MerchantItem {
                make_index: 8,
                index: 2,
                dura: 20,
                dura_max: 20,
                desc: [0; 14],
            }],
        ];
        let out = user_select(LABEL_MAKE_DRUG, &m, &goods, &[], &c);
        assert_eq!(out.len(), 1, "只产一条 SM_SENDUSERMAKEDRUGITEMLIST");
        assert_eq!(
            out[0].ident,
            mir2_protocol::messages::SM_SENDUSERMAKEDRUGITEMLIST
        );
        assert_eq!(out[0].recog, 4242, "Recog = nParam1 = ActorId（商人）");
        assert_eq!(out[0].param, 0, "Param = nParam2 = 0");
        assert_eq!((out[0].tag, out[0].series), (0, 0));
        let mut expect = String::new();
        for tpl in ["金创药", "木剑"] {
            expect.push_str(tpl);
            expect.push_str("/0/100/1/");
        }
        assert_eq!(out[0].tail, downlink::gbk(&expect));
        // 逐字节钉到已复核编码器 `smframe_ext::sm_sendusermakedrugitemlist`。
        assert_eq!(
            body_of(&out[0]),
            crate::smframe_ext::sm_sendusermakedrugitemlist(4242, 0, &downlink::gbk(&expect)),
            "线上 body 必须与 W1 编码器逐字节一致"
        );
        // `IsMakeDrug` 门：非制药商人 ⇒ 无帧。
        assert!(user_select(
            LABEL_MAKE_DRUG,
            &merchant(MerchantFlags::default()),
            &goods,
            &[],
            &c
        )
        .is_empty());
        // 空在售库存 ⇒ `sSendMsg` 空 ⇒ 不产帧（`!string.IsNullOrEmpty` 门）。
        assert!(user_select(LABEL_MAKE_DRUG, &m, &[], &[], &c).is_empty());
    }

    /// `ClientMakeDrugCheckNeedItem`（`Merchant.cs:1432-1500`）：先统计（精确名）→ 全齐则**反向**删并记删除表。
    ///
    /// 变异必红：① 把删除扫描改成正向（`j` 升序）⇒ 删除顺序变 ⇒ `deleted` 断言必红；
    /// ② 把名比较改成 IgnoreCase 之外的模糊匹配 ⇒ 缺料用例仍判齐 ⇒ 必红。
    #[test]
    fn check_need_item_counts_and_consumes_from_back() {
        let cat = drug_catalog();
        let reg = make_reg("[金创药]\n草药 2\n布衣 1\n");
        let mut list = vec![
            mbag(4, 71), // 草药
            mbag(4, 72), // 草药
            mbag(5, 73), // 布衣
            mbag(2, 74), // 木剑（无关）
        ];
        let (ok, deleted) = check_need_item(&reg, &cat, &mut list, "金创药");
        assert!(ok);
        assert_eq!(
            deleted,
            vec![
                ("草药".to_string(), 72),
                ("草药".to_string(), 71),
                ("布衣".to_string(), 73),
            ],
            "每条配方按材料名从头删除（后进先出）"
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, 2, "只有无关的木剑留下");

        // 缺一件草药 ⇒ false 且**不删**（oracle 统计失败即 break，不进删除段）。
        let mut short = vec![mbag(4, 71), mbag(5, 73)];
        let (ok2, del2) = check_need_item(&reg, &cat, &mut short, "金创药");
        assert!(!ok2);
        assert!(del2.is_empty());
        assert_eq!(short.len(), 2, "缺料不得消耗");

        // 无该配方组 ⇒ false（`GetMakeItemInfo` null）。
        let (ok3, _) = check_need_item(&reg, &cat, &mut vec![], "不存在");
        assert!(!ok3);
    }

    /// `CM_USERMAKEDRUGITEM` 成功支：`SM_ADDITEM`（直发）→ `SM_DELITEMS`（优先级 164）→
    /// `SM_MAKEDRUG_SUCCESS(Recog=扣后 Gold)`（优先级 166）。
    ///
    /// 变异必红：① 漏 `SM_DELITEMS` ② 成功帧 `Recog` 用 `n14` 而非金币 ③ 不扣金/不入包
    /// ④ 帧序换位（`SM_MAKEDRUG_SUCCESS` 提前）。
    #[test]
    fn make_drug_success_emits_additem_delitems_success_in_order() {
        let cat = drug_catalog();
        let c = ctx(&cat);
        let reg = make_reg("[金创药]\n草药 2\n布衣 1\n");
        let mut st = ground(1000, vec![mbag(4, 71), mbag(4, 72), mbag(5, 73)]);
        let goods = goods_one(3);
        let mut item_number = 500;
        let r = make_drug_item(&mut st, &goods, "金创药", &cat, &reg, &mut item_number, &c);

        assert!(r.ok);
        let idents: Vec<u16> = r.frames.iter().map(|f| f.ident).collect();
        assert_eq!(
            idents,
            vec![
                mir2_protocol::messages::SM_ADDITEM,
                mir2_protocol::messages::SM_DELITEMS,
                mir2_protocol::messages::SM_MAKEDRUG_SUCCESS,
            ],
            "发送序：SM_ADDITEM → SM_DELITEMS → SM_MAKEDRUG_SUCCESS"
        );
        assert_eq!(st.gold, 900, "Gold -= MakeDurgPrice(100)");
        assert_eq!(st.item_list.len(), 1, "三件材料被消耗、成品入包");
        assert_eq!(st.item_list[0].index, 3);
        assert_eq!(st.item_list[0].make_index, 501, "GetItemNumber() 计数器 +1");
        assert_eq!(st.item_list[0].dura, 12);
        assert_eq!(st.item_list[0].dura_max, 12);

        // SM_ADDITEM：Recog=ActorId、Series=1。
        assert_eq!(r.frames[0].ident, mir2_protocol::messages::SM_ADDITEM);
        assert_eq!(r.frames[0].recog, 4242);
        assert_eq!(r.frames[0].series, 1);
        // SM_DELITEMS：Series=删除数 3、尾已编码（`EncodeString("草药/72/草药/71/布衣/73/")`）。
        assert_eq!(r.frames[1].ident, mir2_protocol::messages::SM_DELITEMS);
        assert_eq!(
            (r.frames[1].recog, r.frames[1].param, r.frames[1].tag),
            (0, 0, 0)
        );
        assert_eq!(r.frames[1].series, 3);
        assert!(r.frames[1].tail_encoded);
        assert_eq!(
            r.frames[1].tail,
            mir2_protocol::edcode::encode(&downlink::gbk("草药/72/草药/71/布衣/73/"))
        );
        // SM_MAKEDRUG_SUCCESS：Recog=扣后金币；逐字节钉到编码器。
        assert_eq!(
            r.frames[2].ident,
            mir2_protocol::messages::SM_MAKEDRUG_SUCCESS
        );
        assert_eq!(r.frames[2].recog, 900);
        assert_eq!(
            body_of(&r.frames[2]),
            crate::smframe_ext::sm_makedrug_success(900)
        );
    }

    /// `CM_USERMAKEDRUGITEM` 的四个失败码（`Merchant.cs:1502-1550` 的 `n14`）：
    /// `1`=名不在在售库存、`2`=背包满（`AddItemToBag` 失败）、`3`=金币不足、`4`=材料不足。
    ///
    /// 变异必红：把任一 `n14` 写成别的值 ⇒ `Recog` 断言必红。
    #[test]
    fn make_drug_failure_codes() {
        let cat = drug_catalog();
        let c = ctx(&cat);
        let reg = make_reg("[金创药]\n草药 2\n布衣 1\n");
        let goods = goods_one(3);

        // n14=1：请求名不在在售组（组首件名 != 请求名）。
        let mut st = ground(1000, vec![]);
        let r = make_drug_item(&mut st, &goods, "别的药", &cat, &reg, &mut 0, &c);
        assert_eq!(r.frames.len(), 1);
        assert_eq!(r.frames[0].ident, mir2_protocol::messages::SM_MAKEDRUG_FAIL);
        assert_eq!(r.frames[0].recog, 1);

        // n14=3：金币不足（< 100）⇒ 不消耗、不发 DELITEMS。
        let mut st = ground(50, vec![mbag(4, 71), mbag(4, 72), mbag(5, 73)]);
        let r = make_drug_item(&mut st, &goods, "金创药", &cat, &reg, &mut 0, &c);
        assert_eq!(r.frames.len(), 1);
        assert_eq!(r.frames[0].recog, 3);
        assert_eq!(st.item_list.len(), 3, "金币不足不得消耗材料");

        // n14=4：材料不足（只有 1 件草药 < 2）。
        let mut st = ground(1000, vec![mbag(4, 71)]);
        let r = make_drug_item(&mut st, &goods, "金创药", &cat, &reg, &mut 0, &c);
        assert_eq!(r.frames.len(), 1);
        assert_eq!(r.frames[0].recog, 4);
        assert_eq!(st.item_list.len(), 1, "材料不足不得消耗");

        // n14=2：空配方（不需材料）+ 背包满 46 ⇒ `AddItemToBag` 失败。
        let empty_reg = make_reg("[金创药]\n");
        let full: Vec<BagItem> = (0..46).map(|i| mbag(2, 100 + i)).collect();
        let mut st = ground(1000, full);
        let r = make_drug_item(&mut st, &goods, "金创药", &cat, &empty_reg, &mut 0, &c);
        assert_eq!(r.frames.len(), 1, "无材料消耗 ⇒ 只发结果帧");
        assert_eq!(r.frames[0].recog, 2);
        assert_eq!(st.item_list.len(), 46);
        assert_eq!(st.gold, 1000, "失败不得扣金");
    }

    /// **C5 `merchant_base_userselect_back_state`**：`NormNpc.UserSelect`（`:1632-1658`）的
    /// `ScriptCurrLable/ScriptGoBackLable` 账本。
    ///
    /// 牙：把两个字段合并 / 漏 `@back` 的双清空分支 / 把 `@main` 特判掉 ⇒ 任一断言红。
    #[test]
    fn base_user_select_tracks_curr_and_back_labels() {
        let (mut curr, mut back) = (String::new(), String::new());
        // 首次 `@a`：curr=@a，back=""（原 curr 为空）。
        base_user_select(&mut curr, &mut back, "@a");
        assert_eq!((curr.as_str(), back.as_str()), ("@a", ""));
        // `@b`：back=@a，curr=@b。
        base_user_select(&mut curr, &mut back, "@b");
        assert_eq!((curr.as_str(), back.as_str()), ("@b", "@a"));
        // 重复点 `@b`（大小写不敏感）：账本不动。
        base_user_select(&mut curr, &mut back, "@B");
        assert_eq!((curr.as_str(), back.as_str()), ("@b", "@a"));
        // `@back`：curr 非空 ⇒ 只清 curr（back 保留，供上层回跳）。
        base_user_select(&mut curr, &mut back, "@back");
        assert_eq!((curr.as_str(), back.as_str()), ("", "@a"));
        // 再 `@back`：curr 空 ⇒ 清 back。
        base_user_select(&mut curr, &mut back, "@back");
        assert_eq!((curr.as_str(), back.as_str()), ("", ""));
        // 非 `@` 开头 / 空：完全不碰账本（`Merchant.UserSelect` 的前置门）。
        base_user_select(&mut curr, &mut back, "hello");
        assert_eq!((curr.as_str(), back.as_str()), ("", ""));
    }

    // -----------------------------------------------------------------
    // A-09：商人展示/成交路径上的 `GetUpgradeStdItem` 差量 + `GetItemName` 自定义名
    // -----------------------------------------------------------------

    fn w(lo: u8, hi: u8) -> u16 {
        u16::from(lo) | (u16::from(hi) << 8)
    }

    /// 带升级差量的武器模板（`StdMode 5`，DC/MC/SC/AC/MAC 都非 0 便于观察差量）。
    fn upgrade_catalog() -> StdItemCatalog {
        let mut c = StdItemCatalog::new();
        c.insert(
            2,
            StdItem {
                name: "木剑".to_string(),
                price: 30,
                weight: 5,
                std_mode: 5,
                dura_max: 20,
                ac: w(1, 2),
                mac: w(3, 4),
                dc: w(5, 6),
                mc: w(7, 8),
                sc: w(9, 10),
                ..StdItem::default()
            },
        );
        c.set_item_names(vec![mir2_data::lists::ItemName {
            make_index: 7,
            item_index: 2,
            item_name: "★自定义剑★".to_string(),
        }]);
        c
    }

    fn upgraded_bag_item() -> BagItem {
        BagItem {
            make_index: 7,
            index: 2,
            dura: 20,
            dura_max: 20,
            // d0..d2 抬 DC/MC/SC 高字节；d13=1 同时置 Tox 与自定义名标记。
            desc: [1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
        }
    }

    /// 详情列表（`Merchant.cs:1304-1345`）：oracle **只**做 `GetUpgradeStdItem`，**不**取自定义名
    /// （模板名）。牙：把 `encode_client_item` 的升级去掉 ⇒ DC/MC/SC/Tox 断言红；把名字改自定义名 ⇒ 名字断言红。
    #[test]
    fn detail_goods_list_upgrades_fields_but_keeps_template_name() {
        let cat = upgrade_catalog();
        let c = ctx(&cat);
        let m = merchant(mflags(true, false));
        let mut goods = vec![vec![MerchantItem {
            make_index: 7,
            index: 2,
            dura: 20,
            dura_max: 20,
            desc: upgraded_bag_item().desc,
        }]];
        let f = detail_goods_list(&m, &mut goods, "木剑", 0, &c);
        assert_eq!(f.ident, mir2_protocol::messages::SM_SENDDETAILGOODSLIST);
        let raw = mir2_protocol::edcode::decode(&f.tail[..f.tail.len() - 1]);
        let name = downlink::gbk("木剑");
        assert_eq!(raw[0] as usize, name.len(), "模板名长度（不是自定义名）");
        assert_eq!(&raw[1..1 + name.len()], &name[..], "详情列表用模板名");
        assert_eq!(
            u16::from_le_bytes([raw[30], raw[31]]),
            w(5, 7),
            "DC += Desc[0]"
        );
        assert_eq!(
            u16::from_le_bytes([raw[32], raw[33]]),
            w(7, 10),
            "MC += Desc[1]"
        );
        assert_eq!(
            u16::from_le_bytes([raw[34], raw[35]]),
            w(9, 13),
            "SC += Desc[2]"
        );
        assert_eq!(raw[75], 1, "Tox += Desc[13]");
    }

    /// 仓库列表（`PlayObject.Base.cs:1590-1620`）：`GetUpgradeStdItem` + `GetItemName`（**取**自定义名）。
    #[test]
    fn back_item_list_uses_custom_name_and_upgraded_template() {
        let cat = upgrade_catalog();
        let c = ctx(&cat);
        let out = back_item_list(4242, &[upgraded_bag_item()], &c);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].ident, mir2_protocol::messages::SM_SAVEITEMLIST);
        let raw = mir2_protocol::edcode::decode(&out[0].tail[..out[0].tail.len() - 1]);
        let name = downlink::gbk("★自定义剑★");
        assert_eq!(raw[0] as usize, name.len(), "自定义名长度");
        assert_eq!(&raw[1..1 + name.len()], &name[..], "仓库列表用自定义名");
        assert_eq!(
            u16::from_le_bytes([raw[30], raw[31]]),
            w(5, 7),
            "DC += Desc[0]"
        );
        assert_eq!(raw[75], 1, "Tox += Desc[13]");
    }

    /// 商人卖出（`Merchant.ClientSellItem`，`Merchant.cs:1377`）：按 `CustomItemSystem.GetItemName`
    /// 做 `OrdinalIgnoreCase` 比对 ⇒ `Desc[13]==1` 的物品必须能用**自定义名**卖出。
    ///
    /// 牙：把 `item_name` 改回“Desc[13]==1 ⇒ 空串” ⇒ 本用例红（找不到件、`ok=false`）。
    #[test]
    fn sell_item_matches_custom_item_name_from_registry() {
        let cat = upgrade_catalog();
        let c = ctx(&cat);
        let m = merchant(mflags(false, true));
        let mut state = GroundState {
            item_list: vec![upgraded_bag_item()],
            gold: 0,
            wabil: ability(0, 100),
        };
        let mut goods: Vec<Vec<MerchantItem>> = Vec::new();
        let r = sell_item(&m, &mut state, &mut goods, 7, "★自定义剑★", &c);
        assert!(r.ok, "自定义名命中（oracle GetItemName 语义）");
        assert!(state.item_list.is_empty(), "卖出成功 ⇒ 背包移除");
        assert!(
            r.frames
                .iter()
                .any(|f| f.ident == mir2_protocol::messages::SM_USERSELLITEM_OK),
            "成功帧"
        );
        // 反证：模板名**不能**卖出该件（oracle 的 GetItemName 已换成自定义名）。
        let mut state2 = GroundState {
            item_list: vec![upgraded_bag_item()],
            gold: 0,
            wabil: ability(0, 100),
        };
        let r2 = sell_item(&m, &mut state2, &mut goods, 7, "木剑", &c);
        assert!(!r2.ok, "Desc[13]==1 时名字是自定义名，模板名不匹配");
    }

    // ---- 商税四调用点合同测试（卡 `openmir2-castle-merchant-tax-wiring`）----------
    //
    // oracle 锚：`Merchant.cs:1392-1400`（卖）/`:1257-1266`（买）/`:1644-1652`（修）；
    // Castle 目标税基 = 本次交易价；CastleManager 目标税基**恒** `UpgradeWeaponPrice`（B-16）。
    // `UserCastle.IncRateGold`：`Round(amount * 5 / 100)`（AwayFromZero）；日上限/金库上限默认不触发。

    fn tax_cfg() -> CastleTaxConfig {
        CastleTaxConfig {
            castle_tax_rate: 5,
            castle_one_day_gold: 2_000_000,
            castle_gold_max: 10_000_000,
            get_all_npc_tax: false,
            upgrade_weapon_price: 10_000,
            castle_member_price_rate: 80,
        }
    }

    fn castle(name: &str) -> UserCastle {
        UserCastle::new(name, mir2_world::castle::CastleSiegeConfig::default())
    }

    fn tax_merchant(
        flags: MerchantFlags,
        castle_merchant: bool,
        castle_index: Option<usize>,
    ) -> MerchantView {
        MerchantView {
            actor_id: 4242,
            flags,
            price_rate: 100,
            castle_merchant,
            castle_index,
            item_type_list: vec![0, 5],
            item_price_list: Vec::new(),
            chr_name: "城堡商人".to_string(),
        }
    }

    #[test]
    fn sell_tax_intent_and_credit_castle_matches_oracle() {
        let cat = catalog();
        let c = ctx(&cat);
        let m = tax_merchant(mflags(false, true), true, Some(0));
        let mut state = GroundState {
            item_list: vec![BagItem {
                make_index: 9,
                index: 1,
                dura: 10,
                dura_max: 10,
                desc: [0; 14],
            }],
            gold: 1000,
            wabil: ability(3, 100),
        };
        let mut goods = Vec::new();
        let r = sell_item(&m, &mut state, &mut goods, 9, "金创药(小量)", &c);
        assert!(r.ok);
        // `GetSellItemPrice(100)=50` ⇒ 税 `Round(50*5/100)=Round(2.5)=3`（AwayFromZero）。
        assert_eq!(
            r.tax,
            Some(MerchantTaxIntent {
                target: MerchantTaxTarget::Castle,
                amount: 50,
            })
        );
        let mut castles = vec![castle("沙巴克")];
        let applied = apply_merchant_tax(
            &mut castles,
            posting_for(r.tax.as_ref(), Some(0)),
            &tax_cfg(),
            0,
        );
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].n_in_gold, 3);
        assert_eq!(castles[0].total_gold, 3);
        assert_eq!(castles[0].today_income, 3);
        // oracle 税分支不查 `MasterGuild`（成员价才查）⇒ master_guild=None 仍入账。
        assert!(castles[0].master_guild.is_none());
    }

    #[test]
    fn buy_tax_intent_and_credit_castle_matches_oracle() {
        let cat = catalog();
        let c = ctx(&cat);
        let m = tax_merchant(mflags(true, false), true, Some(0));
        let mut state = GroundState {
            item_list: Vec::new(),
            gold: 500,
            wabil: ability(0, 100),
        };
        let mut goods = vec![vec![MerchantItem {
            make_index: 7,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: [0; 14],
        }]];
        let r = buy_item(&m, &mut state, &mut goods, "金创药(小量)", 0, &c);
        assert!(r.ok);
        // 买入价 100 ⇒ Castle 税基 = 100 ⇒ `Round(5.0)=5`。
        assert_eq!(
            r.tax,
            Some(MerchantTaxIntent {
                target: MerchantTaxTarget::Castle,
                amount: 100,
            })
        );
        let mut castles = vec![castle("沙巴克")];
        let applied = apply_merchant_tax(
            &mut castles,
            posting_for(r.tax.as_ref(), Some(0)),
            &tax_cfg(),
            0,
        );
        assert_eq!(applied[0].n_in_gold, 5);
        assert_eq!(castles[0].total_gold, 5);
    }

    #[test]
    fn buy_tax_manager_branch_uses_upgrade_price_not_trade_price() {
        let cat = catalog();
        let mut c = ctx(&cat);
        c.get_all_npc_tax = true;
        c.upgrade_weapon_price = 10_000;
        // `Castle == null` + `GetAllNpcTax` ⇒ CastleManager 分支：税基恒 UpgradeWeaponPrice。
        let m = tax_merchant(mflags(true, false), true, None);
        let mut state = GroundState {
            item_list: Vec::new(),
            gold: 500,
            wabil: ability(0, 100),
        };
        let mut goods = vec![vec![MerchantItem {
            make_index: 7,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: [0; 14],
        }]];
        let r = buy_item(&m, &mut state, &mut goods, "金创药(小量)", 0, &c);
        assert!(r.ok);
        assert_eq!(
            r.tax,
            Some(MerchantTaxIntent {
                target: MerchantTaxTarget::CastleManager,
                amount: 10_000,
            }),
            "税基 = Round(10000*5/100)=500，**不是**交易价 100 的 5"
        );
        let mut castles = vec![castle("沙巴克"), castle("沙巴克二")];
        let applied = apply_merchant_tax(
            &mut castles,
            posting_for(r.tax.as_ref(), None),
            &tax_cfg(),
            0,
        );
        assert_eq!(applied.len(), 2, "CastleManager 升序遍历全部城");
        assert_eq!(castles[0].total_gold, 500);
        assert_eq!(castles[1].total_gold, 500);
    }

    #[test]
    fn repair_tax_intent_and_credit_castle_matches_oracle() {
        let cat = catalog();
        let mut c = ctx(&cat);
        c.script_lable = LABEL_REPAIR;
        let m = tax_merchant(
            MerchantFlags {
                is_repair: true,
                ..MerchantFlags::default()
            },
            true,
            Some(0),
        );
        let mut state = GroundState {
            item_list: vec![BagItem {
                make_index: 9,
                index: 1,
                dura: 4,
                dura_max: 10,
                desc: [0; 14],
            }],
            gold: 10_000,
            wabil: ability(0, 100),
        };
        let r = repair_item(&m, &mut state, 0, &c);
        // 修理价 `Round((100/3)/10*6)=20` ⇒ 税 `Round(20*5/100)=1`。
        assert_eq!(
            r.tax,
            Some(MerchantTaxIntent {
                target: MerchantTaxTarget::Castle,
                amount: 20,
            })
        );
        let mut castles = vec![castle("沙巴克")];
        let applied = apply_merchant_tax(
            &mut castles,
            posting_for(r.tax.as_ref(), Some(0)),
            &tax_cfg(),
            0,
        );
        assert_eq!(applied[0].n_in_gold, 1);
        assert_eq!(castles[0].total_gold, 1);
    }

    #[test]
    fn tax_precondition_without_castle_and_without_global_tax_is_none() {
        let cat = catalog();
        let c = ctx(&cat);
        // `CastleMerchant=true` 但 `Castle == null`、`GetAllNpcTax=false` ⇒ oracle 无税分支。
        let m = tax_merchant(mflags(false, true), true, None);
        let mut state = GroundState {
            item_list: vec![BagItem {
                make_index: 9,
                index: 1,
                dura: 10,
                dura_max: 10,
                desc: [0; 14],
            }],
            gold: 1000,
            wabil: ability(3, 100),
        };
        let mut goods = Vec::new();
        let r = sell_item(&m, &mut state, &mut goods, 9, "金创药(小量)", &c);
        assert!(r.ok);
        assert_eq!(r.tax, None, "缺 Castle 且不全局收税 ⇒ 不猜目的地");
        let mut castles = vec![castle("沙巴克")];
        assert!(apply_merchant_tax(
            &mut castles,
            posting_for(r.tax.as_ref(), None),
            &tax_cfg(),
            0
        )
        .is_empty());
        assert_eq!(castles[0].total_gold, 0);
        // `CastleManager.GetCastle` 前置：越界下标 ⇒ 无副作用（不 panic、不猜 0 号城）。
        let stray = Some(MerchantTaxIntent {
            target: MerchantTaxTarget::Castle,
            amount: 50,
        });
        assert!(apply_merchant_tax(
            &mut castles,
            posting_for(stray.as_ref(), Some(9)),
            &tax_cfg(),
            0
        )
        .is_empty());
        assert_eq!(castles[0].total_gold, 0);
    }
    /// **C-1 牙 1（目的地映射）**：`posting_for` 是纯计算层 → 显式目的地的唯一转换点；
    /// Castle 目标吞下标、CastleManager 丢弃下标、`None` intent ⇒ `None`。
    ///
    /// 红检（改坏必红）：把两个 `match` 分支对调（Castle↔CastleManager）或让 `None` 也返回
    /// posting ⇒ 断言必红。
    #[test]
    fn posting_for_maps_intent_target_to_explicit_destination() {
        let castle = MerchantTaxIntent {
            target: MerchantTaxTarget::Castle,
            amount: 7,
        };
        assert_eq!(
            posting_for(Some(&castle), Some(3)),
            Some(MerchantTaxPosting {
                destination: MerchantTaxDestination::Castle(Some(3)),
                amount: 7,
            })
        );
        assert_eq!(
            posting_for(Some(&castle), None),
            Some(MerchantTaxPosting {
                destination: MerchantTaxDestination::Castle(None),
                amount: 7,
            })
        );
        let mgr = MerchantTaxIntent {
            target: MerchantTaxTarget::CastleManager,
            amount: 9,
        };
        assert_eq!(
            posting_for(Some(&mgr), Some(3)),
            Some(MerchantTaxPosting {
                destination: MerchantTaxDestination::CastleManager,
                amount: 9,
            }),
            "CastleManager 丢弃下标（oracle 不看具体城）"
        );
        assert_eq!(posting_for(None, Some(3)), None);
    }

    /// **C-1 牙 2（目的地是唯一路由源）**：`Castle(Some(i))` 只入第 i 座城、`Castle(None)` /
    /// 越界无副作用、`CastleManager` 升序入全部城；目的地参数错即错账。
    ///
    /// 红检（改坏必红）：把 `Castle` 分支改成遍历全部城（或把 `CastleManager` 分支改成只入
    /// 0 号城）⇒ 断言必红。
    #[test]
    fn apply_merchant_tax_routes_only_by_explicit_destination() {
        let cfg = tax_cfg();
        let mut castles = vec![castle("沙巴克"), castle("沙巴克二")];
        let posting = |destination| {
            Some(MerchantTaxPosting {
                destination,
                amount: 100,
            })
        };
        let one = apply_merchant_tax(
            &mut castles,
            posting(MerchantTaxDestination::Castle(Some(1))),
            &cfg,
            0,
        );
        assert_eq!(one.len(), 1, "Castle(i) 只入一座城");
        assert_eq!(castles[0].total_gold, 0, "0 号城不得被动");
        assert_eq!(castles[1].total_gold, 5);
        let all = apply_merchant_tax(
            &mut castles,
            posting(MerchantTaxDestination::CastleManager),
            &cfg,
            0,
        );
        assert_eq!(all.len(), 2, "CastleManager 升序入全部城");
        assert_eq!(castles[0].total_gold, 5);
        assert_eq!(castles[1].total_gold, 10);
        // 越界 / None：不猜 0 号城。
        let mut c2 = vec![castle("沙巴克")];
        assert!(apply_merchant_tax(
            &mut c2,
            posting(MerchantTaxDestination::Castle(Some(9))),
            &cfg,
            0,
        )
        .is_empty());
        assert!(apply_merchant_tax(
            &mut c2,
            posting(MerchantTaxDestination::Castle(None)),
            &cfg,
            0,
        )
        .is_empty());
        assert_eq!(c2[0].total_gold, 0);
    }

    /// **卡 `openmir2-script-handler-msgqueue`（rework per review f215e887）**：`MsgQueue` 优先级
    /// 必须由**生产者**标注——同一 SM 可来自不同 RM：普通修理 `RM_SENDUSERREPAIR`=10141→**157**
    /// 与超级修理 `RM_SENDUSERSREPAIR`=50142→**222** 共用泵分支
    /// （`PlayObject.Message.cs:1851-1853`；入队点 `Merchant.cs:1001` / `:958`）⇒ 不能用 SM 反推。
    ///
    /// 另钉三处被 review 指正的「泵内才 SendSocket」帧：`SM_DELITEMS` ← `RM_SENDDELITEMLIST`
    /// （10148→**164**，泵 `PlayObject.Message.cs:1870-1873`）、`SM_SAVEITEMLIST` ←
    /// `RM_USERGETBACKITEM`（10147→**163**，泵 `:1867-1868`）且**多页是原子批次**。
    ///
    /// 牙（实测）：超级修理改回 157 ⇒ 本用例红；DELITEMS 改 None ⇒ 红；去掉 `.atomic()` ⇒ 红。
    #[test]
    fn producer_frames_carry_oracle_queue_priorities() {
        let cat = catalog();
        let c = ctx(&cat);
        let no_goods: Vec<Vec<MerchantItem>> = Vec::new();
        // `@makedrug` 只在商品表非空时产帧（`UserSelectMakeDurg` 的 `!string.IsNullOrEmpty` 门）。
        let one_good: Vec<Vec<MerchantItem>> = vec![vec![MerchantItem {
            make_index: 0,
            index: 1,
            dura: 5,
            dura_max: 10,
            desc: [0; 14],
        }]];
        let prio = |label: &str, flags: MerchantFlags, goods: &[Vec<MerchantItem>]| {
            let out = user_select(label, &merchant(flags), goods, &[], &c);
            out.first().and_then(|f| f.queue_priority)
        };
        assert_eq!(
            prio(
                LABEL_BUY,
                MerchantFlags {
                    is_buy: true,
                    ..Default::default()
                },
                &no_goods
            ),
            Some(144)
        );
        assert_eq!(
            prio(
                LABEL_SELL,
                MerchantFlags {
                    is_sell: true,
                    ..Default::default()
                },
                &no_goods
            ),
            Some(145)
        );
        assert_eq!(
            prio(
                LABEL_REPAIR,
                MerchantFlags {
                    is_repair: true,
                    ..Default::default()
                },
                &no_goods
            ),
            Some(157)
        );
        assert_eq!(
            prio(
                LABEL_SUPER_REPAIR,
                MerchantFlags {
                    is_sup_repair: true,
                    ..Default::default()
                },
                &no_goods
            ),
            Some(222),
            "超级修理 = RM_SENDUSERSREPAIR(50142→222)，不是普通修理的 157"
        );
        assert_eq!(
            prio(
                LABEL_STORAGE,
                MerchantFlags {
                    is_storage: true,
                    ..Default::default()
                },
                &no_goods
            ),
            Some(162)
        );
        assert_eq!(
            prio(
                LABEL_MAKE_DRUG,
                MerchantFlags {
                    is_make_drug: true,
                    ..Default::default()
                },
                &one_good
            ),
            Some(165)
        );
        let del = del_items_frame(&[("金创药(小量)".to_string(), 7)]);
        assert_eq!(
            del.queue_priority,
            Some(164),
            "SM_DELITEMS ← RM_SENDDELITEMLIST(10148→164)，泵内才 SendSocket"
        );
        let items: Vec<BagItem> = (0..120)
            .map(|i| BagItem {
                make_index: i,
                index: 1,
                dura: 5,
                dura_max: 10,
                desc: [0; 14],
            })
            .collect();
        let pages = back_item_list(4242, &items, &c);
        assert!(pages.len() >= 3, "120 件 ⇒ ≥3 页，实际 {}", pages.len());
        for f in &pages {
            assert_eq!(
                f.queue_priority,
                Some(163),
                "SM_SAVEITEMLIST ← RM_USERGETBACKITEM(10147→163)"
            );
            assert!(f.atomic_batch, "分页必须是同一条队列消息的原子批次");
        }
    }
}
