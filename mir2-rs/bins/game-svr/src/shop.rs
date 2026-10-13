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
//! # 本片未做（登记，第②片）
//! - `CM_USERGETDETAILITEM(1015)`（`Merchant.ClientGetDetailGoodsList`，`Merchant.cs:1304`）与
//!   `SM_SENDDETAILGOODSLIST(652)`：物品详情分页编码（`GetUpgradeStdItem`）归第②片。
//! - `CM_USERSTORAGEITEM(1031)` / `CM_USERTAKEBACKSTORAGEITEM(1032)`（仓库会话态 + 39 上限 + 锁）
//!   与 `SM_SENDUSERSTORAGEITEM(700)`/`SM_SAVEITEMLIST(704)` 等归第②片（承接卡 ⑤-5 的存档面）。
//! - `Market_Def` 商人脚本执行 + `GotoLable("~@repair")`（`Merchant.cs:1652/1659`）：本宿主
//!   尚未装载商人脚本（`Merchant.LoadScriptFile`，`Merchant.cs:1089`）⇒ [`RepairResult::goto_label`]
//!   只回标签、由宿主登记「脚本未装载 ⇒ 无后续说词/帧」，见 `main.rs` 的 `handle_shop_cmd`。
//! - 城堡税 `Castle.IncRateGold`/`CastleMgr.IncRateGold`（`Merchant.cs:1249/1384/1638`）：
//!   ⑧ 家族未落地 ⇒ **条件照抄**、效果无目标对象（登记）。
//!
//! # 三条从 ⑤-3 转入的副作用（owner comment `270a3919`）
//! 1. `@Superrepairfail` 的 `SendMsgToUser("对不起!我不能帮你修理这个物品。\\ \\ \\<返回/@main>")`
//!    （`Merchant.cs:1612`）：本模块产出 `SM_SYSMESSAGE` 帧（走 [`RepairResult::frames`] 的
//!    `sent_to_user` 字段）。
//! 2. 城堡税：条件照抄（`CastleMerchant || Config.GetAllNpcTax`），效果登记（无城堡对象）。
//! 3. `GotoLable(REPAIROK/SUPERREPAIROK)`：回 [`RepairResult::goto_label`]，宿主登记未装载脚本。
//!
//! 另 reviewer-5 的 **O-1**：`ScriptLable` 与 `@s_repair` 的比较是 `string.Compare(..., OrdinalIgnoreCase)`，
//! 本模块用 [`eq_label`]（ASCII 大小写不敏感）同口径。

use mir2_data::models::StdItem;
use mir2_formula::shop::{self, PriceTables, ShopConfig, StdItemSource};
use mir2_world::merchant::ACTOR_RACE_MERCHANT;
use mir2_world::merchant::{MerchantFlags, MerchantItem};

use crate::ability::ItemCatalog;
use crate::bag::{BagItem, MAX_BAG_ITEM};
use crate::downlink;
use crate::ground::GroundState;

pub const CM_MERCHANTDLGSELECT: u16 = 1011;
pub const CM_MERCHANTQUERYSELLPRICE: u16 = 1012;
pub const CM_USERSELLITEM: u16 = 1013;
pub const CM_USERBUYITEM: u16 = 1014;
pub const CM_USERGETDETAILITEM: u16 = 1015;
pub const CM_USERREPAIRITEM: u16 = 1023;
pub const CM_MERCHANTQUERYREPAIRCOST: u16 = 1024;

/// `ScriptFlagConst`（`src/Modules/SystemModule/Const/ScriptFlagConst.cs`）本片用到的子集。
pub const LABEL_BUY: &str = "@buy";
pub const LABEL_SELL: &str = "@sell";
pub const LABEL_REPAIR: &str = "@repair";
pub const LABEL_SUPER_REPAIR: &str = "@s_repair";
pub const LABEL_SUPER_REPAIR_FAIL: &str = "@fail_s_repair";
pub const LABEL_REPAIR_OK: &str = "~@repair";
pub const LABEL_SUPER_REPAIR_OK: &str = "~@s_repair";

/// `MessageSettings.CanotGetItems` / 修理失败文案（`Merchant.cs:1612`）；GBK 由 `downlink::gbk` 编码。
pub const SUPER_REPAIR_FAIL_MSG: &str = "对不起!我不能帮你修理这个物品。\\ \\ \\<返回/@main>";

/// 一条出帧（`MakeMessage` 后的 4 个数据字段 + 可选尾）；宿主用 `smframe::command_header` 落 12B 头。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShopFrame {
    pub ident: u16,
    pub recog: i32,
    pub param: u16,
    pub tag: u16,
    pub series: u16,
    pub tail: Vec<u8>,
}

fn frame(ident: u16, recog: i32, param: u16, tag: u16, series: u16, tail: Vec<u8>) -> ShopFrame {
    ShopFrame {
        ident,
        recog,
        param,
        tag,
        series,
        tail,
    }
}

/// `Merchant` 实例上本片读到的字段（宿主从世界实体取）。
pub struct MerchantView {
    pub actor_id: i32,
    pub flags: MerchantFlags,
    pub price_rate: i32,
    pub castle_merchant: bool,
    pub item_type_list: Vec<i32>,
}

/// 一次商店动作的静态输入（会话/世界/配置）。
pub struct ShopContext<'a> {
    pub catalog: &'a dyn ItemCatalog,
    /// `SystemShare.Config` 的定价/修理字段（`ShopConfig`，缺省值同 `GameSvrConf`）。
    pub cfg: ShopConfig,
    /// `SystemShare.Config.GetAllNpcTax`（`Merchant.cs:1249/1384/1638` 的税条件之一）。
    pub get_all_npc_tax: bool,
    /// `PlayObject.ScriptLable`（`Merchant.cs:1555/1599` 的 `@s_repair` 分支）。
    pub script_lable: &'a str,
    /// `Config.HumanMaxGold`（`IncGold` 上限，`PlayObject.Base.cs:168`）。
    pub human_max_gold: i32,
    /// 玩家 `BaseObject.ActorId`（`SM_ADDITEM`/`SM_SYSMESSAGE` 的 `Recog`）。
    pub player_actor_id: i32,
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

/// `CustomItemSystem.GetItemName(userItem)`（`Merchant.cs:967/1237` 等）的**默认分支** = 模板名。
///
/// oracle 的 `Desc[13]` 自定义名（`DelCustomItemName`，`Operate.cs:174`）未建模 ⇒ 登记。
#[must_use]
pub fn item_name(catalog: &dyn ItemCatalog, index: u16) -> Option<String> {
    catalog.get(index).map(|t| t.name.clone())
}

/// 建 `PriceTables`（`ItemPriceList` 空：oracle `LoadGoodPriceRecord` 是空壳，`LocalDB.cs:1111`）。
///
/// `source` 由调用方持有（`PriceTables.source` 的 `&dyn StdItemSource` 只活到本函数返回）。
fn price_tables<'a>(source: &'a CatalogSource<'a>, item_type_list: &'a [i32]) -> PriceTables<'a> {
    PriceTables {
        source,
        item_price_list: &[],
        item_type_list,
    }
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
    ctx: &ShopContext<'_>,
) -> Vec<ShopFrame> {
    let mut out = Vec::new();
    if eq_label(label, LABEL_BUY) && m.flags.is_buy {
        // `UserSelectBuyItem`（`Merchant.cs:962`）：`名/子菜单/价/库存/`。价 = `GetUserPrice(GetItemPrice(Index))`。
        let source = CatalogSource(ctx.catalog);
        let t = price_tables(&source, &m.item_type_list);
        let mut s_send = String::new();
        let mut n10 = 0i32;
        for group in goods {
            let Some(it) = group.first() else { continue };
            let Some(tpl) = ctx.catalog.get(it.index) else {
                continue;
            };
            let name = item_name(ctx.catalog, it.index).unwrap_or_default();
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
        out.push(frame(
            mir2_protocol::messages::SM_SENDGOODSLIST,
            m.actor_id,
            n10 as u16,
            0,
            0,
            downlink::gbk(&s_send),
        ));
    } else if eq_label(label, LABEL_SELL) && m.flags.is_sell {
        out.push(frame(
            mir2_protocol::messages::SM_SENDUSERSELL,
            m.actor_id,
            0,
            0,
            0,
            Vec::new(),
        ));
    } else if eq_label(label, LABEL_SUPER_REPAIR) && m.flags.is_sup_repair {
        // `UserSelectSuperRepairItem` → `RM_SENDUSERSREPAIR` → `SM_SENDUSERREPAIR`。
        out.push(frame(
            mir2_protocol::messages::SM_SENDUSERREPAIR,
            m.actor_id,
            0,
            0,
            0,
            Vec::new(),
        ));
    } else if eq_label(label, LABEL_REPAIR) && m.flags.is_repair {
        // `UserSelectRepairItem` → `RM_SENDUSERREPAIR` → `SM_SENDUSERREPAIR`。
        out.push(frame(
            mir2_protocol::messages::SM_SENDUSERREPAIR,
            m.actor_id,
            0,
            0,
            0,
            Vec::new(),
        ));
    }
    out
}

/// `Merchant.ClientQuerySellPrice`（`Merchant.cs:1345`）：`nC = GetSellItemPrice(GetUserItemPrice(item))`，
/// 回 `RM_SENDBUYPRICE(0, nC>=0 ? nC : 0)`。
///
/// ⚠ `GetUserItemPrice` 对 `StdMode == 43` **写回 `DuraMax = 10000`**（oracle 副作用）⇒ 这里把
/// 写回同步到 `item`（与 `PlayObject.Operate.cs:131` 传**真身**给 `GetUserItemPrice` 同口径）。
#[must_use]
pub fn query_sell_price(m: &MerchantView, item: &mut BagItem, ctx: &ShopContext<'_>) -> ShopFrame {
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list);
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
}

/// `CM_USERSELLITEM` 的结果：成功时 `item_list` 已移除、`GoodsList` 已回收。
pub struct SellResult {
    pub frames: Vec<ShopFrame>,
    /// `ClientSellItem` 的返回值（`Operate.cs:172` 用它决定是否 `RemoveAt`）。
    pub ok: bool,
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
            && item_name(ctx.catalog, it.index).is_some_and(|n| eq_label(&n, wanted_name))
    }) else {
        return SellResult { frames, ok: false };
    };
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list);
    let mut it = to_merchant(&state.item_list[pos]);
    let n_price = shop::get_sell_item_price(shop::get_user_item_price(&t, &mut it));
    state.item_list[pos].dura_max = it.dura_max; // BUG-REPLICA 写回
    let std_mode = ctx.catalog.get(it.index).map_or(0, |s| s.std_mode);
    // `ClientSellItem_sub_4A1C84`（`Merchant.cs:1364`）：StdMode 25/30 需 `Dura >= 4000`。
    let sub_ok = !(std_mode == 25 || std_mode == 30) || it.dura >= 4000;
    if n_price > 0 && sub_ok {
        if inc_gold(state.gold, n_price, ctx.human_max_gold) {
            state.gold += n_price;
            // 税条件照抄（`Merchant.cs:1384-1391`）；城堡对象未建模 ⇒ 无目标（登记）。
            let _tax = m.castle_merchant || ctx.get_all_npc_tax;
            frames.push(frame(
                mir2_protocol::messages::SM_USERSELLITEM_OK,
                state.gold,
                0,
                0,
                0,
                Vec::new(),
            ));
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
            SellResult { frames, ok: true }
        } else {
            frames.push(frame(
                mir2_protocol::messages::SM_USERSELLITEM_FAIL,
                0,
                0,
                0,
                0,
                Vec::new(),
            ));
            SellResult { frames, ok: false }
        }
    } else {
        frames.push(frame(
            mir2_protocol::messages::SM_USERSELLITEM_FAIL,
            0,
            0,
            0,
            0,
            Vec::new(),
        ));
        SellResult { frames, ok: false }
    }
}

/// `CM_USERBUYITEM` 的结果。
pub struct BuyResult {
    pub frames: Vec<ShopFrame>,
    /// `n1C == 0`（买入成功）。
    pub ok: bool,
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
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list);
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
            let s_user_item_name = item_name(ctx.catalog, first.index).unwrap_or_default();
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
                            if state.item_list.len() < MAX_BAG_ITEM {
                                // `AddItemToBag` 成功：金币/税/出包 + 从库存移除。
                                state.gold -= n_price;
                                let _tax = m.castle_merchant || ctx.get_all_npc_tax;
                                frames.push(frame(
                                    mir2_protocol::messages::SM_ADDITEM,
                                    ctx.player_actor_id,
                                    0,
                                    0,
                                    1,
                                    Vec::new(),
                                ));
                                state.item_list.push(to_bag(&price_item));
                                state.wabil.weight = recalc_weight(ctx.catalog, &state.item_list);
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
        frames.push(frame(
            mir2_protocol::messages::SM_BUYITEM_SUCCESS,
            state.gold,
            (n_int & 0xFFFF) as u16,
            ((n_int >> 16) & 0xFFFF) as u16,
            0,
            Vec::new(),
        ));
        BuyResult { frames, ok: true }
    } else {
        // `RM_BUYITEM_FAIL(0, n1C)` → `SM_BUYITEM_FAIL(Recog=n1C)`。
        frames.push(frame(
            mir2_protocol::messages::SM_BUYITEM_FAIL,
            n1c,
            0,
            0,
            0,
            Vec::new(),
        ));
        BuyResult { frames, ok: false }
    }
}

/// `Merchant.ClientQueryRepairCost`（`Merchant.cs:1555`）的出帧值（`Recog = nRepairPrice`）。
#[must_use]
pub fn query_repair_cost(m: &MerchantView, item: &mut BagItem, ctx: &ShopContext<'_>) -> ShopFrame {
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list);
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
}

/// `CM_USERREPAIRITEM` 的结果。
pub struct RepairResult {
    pub frames: Vec<ShopFrame>,
    /// 修理成功时 oracle `GotoLable(REPAIROK/SUPERREPAIROK)` 的标签（本宿主未装载 `Market_Def`
    /// ⇒ 只回标签，由宿主登记）。
    pub goto_label: Option<&'static str>,
    /// `SendMsgToUser`（`@Superrepairfail` 支）的 `SM_SYSMESSAGE` 帧若产出，则在此。
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
        };
    }
    let source = CatalogSource(ctx.catalog);
    let t = price_tables(&source, &m.item_type_list);
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
    match out.outcome {
        shop::RepairOutcome::NoFrame => {}
        shop::RepairOutcome::Fail => {
            if label == shop::RepairLabel::SuperRepairFail {
                sent_to_user = Some(frame(
                    mir2_protocol::messages::SM_SYSMESSAGE,
                    ctx.player_actor_id,
                    0,
                    0,
                    0,
                    downlink::gbk(SUPER_REPAIR_FAIL_MSG),
                ));
            }
            frames.push(frame(
                mir2_protocol::messages::SM_USERREPAIRITEM_FAIL,
                0,
                0,
                0,
                0,
                Vec::new(),
            ));
        }
        shop::RepairOutcome::Ok => {
            state.gold = out.gold;
            // 税条件照抄（`Merchant.cs:1638-1646`）；城堡对象未建模 ⇒ 无目标（登记）。
            let _tax = m.castle_merchant || ctx.get_all_npc_tax;
            // `RM_USERREPAIRITEM_OK(0, Gold, Dura, DuraMax)` → `SM_USERREPAIRITEM_OK(Recog=Gold, Param=Dura, Tag=DuraMax)`。
            frames.push(frame(
                mir2_protocol::messages::SM_USERREPAIRITEM_OK,
                state.gold,
                out.dura,
                out.dura_max,
                0,
                Vec::new(),
            ));
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

/// `WorldEngine.FindMerchant(actorId)`（`WorldServer.cs:1425`）的宿主等价物：实体 id = `ActorId`。
#[must_use]
pub fn is_merchant(world: &mir2_world::World, actor_id: i32) -> bool {
    world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.race == ACTOR_RACE_MERCHANT && e.merchant.is_some())
}

#[cfg(test)]
mod tests {
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
            item_type_list: vec![0, 5],
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
            script_lable: "",
            human_max_gold: 1_000_000,
            player_actor_id: 4242,
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
        let out = user_select(LABEL_BUY, &m, &goods, &c);
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
        assert_eq!(user_select(LABEL_SELL, &m, &[], &c)[0].ident, 646);
        let mut m2 = merchant(MerchantFlags::default());
        m2.flags.is_repair = true;
        assert_eq!(user_select(LABEL_REPAIR, &m2, &[], &c)[0].ident, 668);
        m2.flags.is_sup_repair = true;
        assert_eq!(user_select(LABEL_SUPER_REPAIR, &m2, &[], &c)[0].ident, 668);
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
        // 钱不够 ⇒ n1C = 3。
        state.wabil = ability(0, 100);
        state.gold = 0;
        let r = buy_item(&m, &mut state, &mut goods, "金创药(小量)", 0, &c);
        assert_eq!(r.frames[0].recog, 3);
        // 成功 ⇒ n1C = 0，金币扣、库存减、背包加。
        state.gold = 500;
        let r = buy_item(&m, &mut state, &mut goods, "金创药(小量)", 0, &c);
        assert!(r.ok);
        assert!(r.frames.iter().any(|f| f.ident == 200));
        assert_eq!(r.frames.last().unwrap().ident, 650);
        assert_eq!(state.gold, 400);
        assert_eq!(state.item_list.len(), 1);
        assert!(goods.is_empty(), "空组移除");
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

        // `@fail_s_repair` ⇒ SendMsgToUser + FAIL（无 GotoLable）。
        c.script_lable = LABEL_SUPER_REPAIR_FAIL;
        let m = merchant(MerchantFlags {
            is_repair: true,
            ..MerchantFlags::default()
        });
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
        assert_eq!(su.ident, mir2_protocol::messages::SM_SYSMESSAGE);
        assert_eq!(su.recog, 4242);
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
}
