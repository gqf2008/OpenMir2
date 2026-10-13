//! 商店定价 / 修理公式，移植源 `src/GameSrv/Npc/Merchant.cs`：
//! - `GetItemPrice(ushort)` — `Merchant.cs:347`
//! - `GetUserItemPrice(UserItem)` — `Merchant.cs:1130`
//! - `GetUserPrice(IPlayerActor, double)` — `Merchant.cs:765`
//! - `GetSellItemPrice(double)` — `Merchant.cs:1358`（static）
//! - `ClientQueryRepairCost(IPlayerActor, UserItem)` — `Merchant.cs:1555`
//! - `ClientRepairItem(IPlayerActor, UserItem)` — `Merchant.cs:1599`
//!
//! **行为等价优先于修 bug**：以下 oracle 缺陷一并照搬，`BUG-REPLICA` 标注：
//! - `Merchant.cs:1179` 的 `if (nC != 4 || nC != 9)` 逻辑或**恒真**（死代码），Rust 侧不复刻该判断本身，
//!   直接执行其唯一可达分支（等价行为），见 [`get_user_item_price`]。
//! - `StdMode == 43` 时 `GetUserItemPrice` **写回 `userItem.DuraMax = 10000`**（入参可变）。
//! - `StdMode > 4` 分支的 `itemPrice = itemPrice / 5 * n14` 是 **double 除法**（`/5` 不是 C# 整型除法）。
//! - 一般分支对 `stdItem.DuraMax`（模板）与 `userItem.DuraMax`（实例）**两个口径**各用一次。
//! - 修理耗久的 `(userItem.DuraMax - userItem.Dura) / RepairItemDecDura` 是 **C# 整型除法（向零截断）**。

use mir2_shared::hutil;
use std::collections::HashMap;

pub use mir2_data::models::StdItem;

/// `src/OpenMir2/Packets/ClientPackets/ClientUserItem.cs` 的 `UserItem` 子集。
///
/// 与 [`crate::drop::UserItem`] 同形（掉落公式与定价公式在 C# 里操作同一个结构体）。
pub use crate::drop::UserItem;

/// C# `SystemModule.Data.ItemPrice`（`src/Modules/SystemModule/Data/ItemPrice.cs`）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemPrice {
    pub w_index: u16,
    pub n_price: f64,
}

/// 价格公式读到的 `StdItem` 查表面（C# `SystemShare.ItemSystem.GetStdItem(ushort)`）。
pub trait StdItemSource {
    fn get_std_item(&self, index: u16) -> Option<&StdItem>;
}

/// 供向量用例/无引擎调用方使用的内存查表（键 = 物品 `Index`，与 C# `GetStdItem` 入参同域）。
impl StdItemSource for HashMap<u16, StdItem> {
    fn get_std_item(&self, index: u16) -> Option<&StdItem> {
        self.get(&index)
    }
}

/// 复用掉落公式的目录（`index - 1` 入 `StdItemList`，与 C# `GameItemSystem.GetStdItem` 同口径）。
impl StdItemSource for crate::drop::VecItemCatalog<'_> {
    fn get_std_item(&self, index: u16) -> Option<&StdItem> {
        crate::drop::ItemCatalog::get_std_item(self, index)
    }
}

/// `Merchant` 实例上定价/修理读取的两张表（C# `ItemPriceList` / `ItemTypeList`）
/// 加物品模板查表面，捆在一起避免函数参数过长。
pub struct PriceTables<'a> {
    pub source: &'a dyn StdItemSource,
    pub item_price_list: &'a [ItemPrice],
    pub item_type_list: &'a [i32],
}

/// `SystemShare.Config` 里定价/修理读到的字段（默认值见 `GameSvrConf.cs`）。
#[derive(Clone, Copy, Debug)]
pub struct ShopConfig {
    /// `Merchant.PriceRate`（脚本 `%PRICERATE`，默认 100）。
    pub price_rate: i32,
    /// `Merchant.CastleMerchant`。
    pub castle_merchant: bool,
    /// `Castle != null && Castle.IsMasterGuild(playObject.MyGuild)`。
    pub castle_member: bool,
    /// `Config.CastleMemberPriceRate`（默认 80）。
    pub castle_member_price_rate: i32,
    /// `Config.SuperRepairPriceRate`（默认 3）。
    pub super_repair_price_rate: i32,
    /// `Config.RepairItemDecDura`（默认 30，单位：普通修理掉持久数）。
    pub repair_item_dec_dura: i32,
}

impl Default for ShopConfig {
    fn default() -> Self {
        ShopConfig {
            price_rate: 100,
            castle_merchant: false,
            castle_member: false,
            castle_member_price_rate: 80,
            super_repair_price_rate: 3,
            repair_item_dec_dura: 30,
        }
    }
}

/// C# `Merchant.CheckItemType(int)`（`Merchant.cs:333`）：`ItemTypeList` 是否含该 `StdMode`。
pub fn check_item_type(item_type_list: &[i32], std_mode: i32) -> bool {
    item_type_list.contains(&std_mode)
}

/// C# `Merchant.GetItemPrice(ushort)`（`Merchant.cs:347`）。
///
/// `ItemPriceList` 覆盖优先（命中即 `break`），未命中/命中负价（`result < 0`）时回落到
/// `ItemTypeList` 含该 `StdMode` 的 `StdItem.Price`；否则 `-1`。
pub fn get_item_price(tables: &PriceTables<'_>, index: u16) -> f64 {
    let mut result = -1.0f64;
    for item_price in tables.item_price_list {
        if item_price.w_index == index {
            result = item_price.n_price;
            break;
        }
    }
    if result < 0.0 {
        if let Some(std_item) = tables.source.get_std_item(index) {
            if check_item_type(tables.item_type_list, i32::from(std_item.std_mode)) {
                result = f64::from(std_item.price);
            }
        }
    }
    result
}

/// `userItem.DuraMax - userItem.Dura` 的 C# 语义：两个 `ushort` 相减得到 **int**（可为负）。
fn dura_delta_int(user_item: &UserItem) -> i32 {
    i32::from(user_item.dura_max) - i32::from(user_item.dura)
}

/// C# `Merchant.GetUserItemPrice(UserItem)`（`Merchant.cs:1130`）。
///
/// 注意 `user_item` 为可变引用：`StdMode == 43` 分支会**写回 `dura_max = 10000`**
/// （`Merchant.cs:1158`），这是 oracle 的既有副作用，调用方需按“取价后 DuraMax 可能已变”处理。
pub fn get_user_item_price(tables: &PriceTables<'_>, user_item: &mut UserItem) -> f64 {
    let mut item_price = get_item_price(tables, user_item.index);
    if item_price > 0.0 {
        if let Some(std_item) = tables.source.get_std_item(user_item.index) {
            let std_mode = std_item.std_mode;
            let std_dura_max = std_item.dura_max;
            if std_mode > 4 && std_dura_max > 0 && user_item.dura_max > 0 {
                match std_mode {
                    40 => {
                        if user_item.dura <= user_item.dura_max {
                            let n20 = item_price / 2.0 / f64::from(user_item.dura_max)
                                * f64::from(dura_delta_int(user_item));
                            item_price =
                                f64::from(hutil::max_i32(2, hutil::round(item_price - n20)));
                        } else {
                            item_price += f64::from(hutil::round(
                                item_price / f64::from(user_item.dura_max)
                                    * 2.0
                                    * f64::from(dura_delta_int(user_item)),
                            ));
                        }
                        return item_price;
                    }
                    43 => {
                        if user_item.dura_max < 10000 {
                            // BUG-REPLICA（Merchant.cs:1158）：取价顺带改写实例 DuraMax。
                            user_item.dura_max = 10000;
                        }
                        if user_item.dura <= user_item.dura_max {
                            let n20 = item_price / 2.0 / f64::from(user_item.dura_max)
                                * f64::from(dura_delta_int(user_item));
                            item_price =
                                f64::from(hutil::max_i32(2, hutil::round(item_price - n20)));
                        } else {
                            item_price += f64::from(hutil::round(
                                item_price / f64::from(user_item.dura_max)
                                    * 1.3
                                    * f64::from(dura_delta_int(user_item)),
                            ));
                        }
                        return item_price;
                    }
                    _ => {
                        // BUG-REPLICA（Merchant.cs:1179）：C# 守卫 `nC != 4 || nC != 9` 恒真，
                        // 唯一可达分支即此循环体，故 Rust 侧省略该恒真判断（行为等价）。
                        let mut n14 = 0i32;
                        let mut n_c = 0usize;
                        loop {
                            if std_mode == 5 || std_mode == 6 {
                                if n_c == 6 {
                                    if user_item.desc[n_c] > 10 {
                                        n14 += (i32::from(user_item.desc[n_c]) - 10) * 2;
                                    }
                                } else {
                                    n14 += i32::from(user_item.desc[n_c]);
                                }
                            } else {
                                n14 += i32::from(user_item.desc[n_c]);
                            }
                            n_c += 1;
                            if n_c >= 8 {
                                break;
                            }
                        }
                        if n14 > 0 {
                            // C# `itemPrice / 5 * n14`：double 除法后乘 int。
                            item_price = item_price / 5.0 * f64::from(n14);
                        }
                        // C# 用**模板** `stdItem.DuraMax` 折算到**实例** `userItem.DuraMax`。
                        item_price = f64::from(hutil::round(
                            item_price / f64::from(std_dura_max) * f64::from(user_item.dura_max),
                        ));
                        let n20 = item_price / 2.0 / f64::from(user_item.dura_max)
                            * f64::from(dura_delta_int(user_item));
                        item_price = f64::from(hutil::max_i32(2, hutil::round(item_price - n20)));
                        return item_price;
                    }
                }
            }
        }
    }
    item_price
}

/// C# `Merchant.GetUserPrice(IPlayerActor, double)`（`Merchant.cs:765`）。
///
/// `n_price` 是 `GetUserItemPrice` 的 double 结果；返回 `HUtil32.Round`（AwayFromZero）。
/// 城堡成员价：`n14 = max(60, Round(PriceRate * CastleMemberPriceRate / 100.0))`。
pub fn get_user_price(cfg: &ShopConfig, n_price: f64) -> i32 {
    if cfg.castle_merchant && cfg.castle_member {
        let n14 = hutil::max_i32(
            60,
            hutil::round(
                f64::from(cfg.price_rate) * (f64::from(cfg.castle_member_price_rate) / 100.0),
            ),
        );
        hutil::round(n_price / 100.0 * f64::from(n14))
    } else {
        hutil::round(n_price / 100.0 * f64::from(cfg.price_rate))
    }
}

/// C# `Merchant.GetSellItemPrice(double)`（`Merchant.cs:1358`）：`Round(nPrice / 2.0)`。
pub fn get_sell_item_price(n_price: f64) -> i32 {
    hutil::round(n_price / 2.0)
}

/// C# `Merchant.ClientQueryRepairCost` 的**出帧值**（`Merchant.cs:1555`）。
///
/// 返回 `RM_SENDREPAIRCOST` 的 `nParam1`（`-1` = 不可修）。`script_lable_is_super_repair`
/// 对应 `playObject.ScriptLable == ScriptFlagConst.SuperRepair`（`@s_repair`）。
pub fn query_repair_cost(
    tables: &PriceTables<'_>,
    cfg: &ShopConfig,
    user_item: &mut UserItem,
    script_lable_is_super_repair: bool,
    is_repair: bool,
    is_sup_repair: bool,
) -> i32 {
    let n_price = get_user_price(cfg, get_user_item_price(tables, user_item));
    if n_price > 0 && user_item.dura_max > user_item.dura {
        let mut n_repair_price = if user_item.dura_max > 0 {
            hutil::round(
                (f64::from(n_price) / 3.0) / f64::from(user_item.dura_max)
                    * f64::from(dura_delta_int(user_item)),
            )
        } else {
            n_price
        };
        if script_lable_is_super_repair {
            if is_sup_repair {
                n_repair_price *= cfg.super_repair_price_rate;
            } else {
                n_repair_price = -1;
            }
        } else if !is_repair {
            n_repair_price = -1;
        }
        n_repair_price
    } else {
        -1
    }
}

/// C# `Merchant.ClientRepairItem` 读到的 `playObject.ScriptLable` 三分支
/// （`ScriptFlagConst.SuperRepair` = `@s_repair`、`Superrepairfail` = `@fail_s_repair`，其余 = `Other`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairLabel {
    Other,
    SuperRepair,
    SuperRepairFail,
}

/// `ClientRepairItem` 的出帧结果（`Merchant.cs:1599`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairOutcome {
    /// 脚本不支持 / 无 `StdItem` ⇒ **直接 return，无任何帧**。
    NoFrame,
    /// `RM_USERREPAIRITEM_FAIL`。
    Fail,
    /// `RM_USERREPAIRITEM_OK`。
    Ok,
}

/// `ClientRepairItem` 的副作用结果：金币 + 修理后的持久。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepairItemResult {
    pub outcome: RepairOutcome,
    pub gold: i32,
    pub dura: u16,
    pub dura_max: u16,
}

/// C# `Merchant.ClientRepairItem(IPlayerActor, UserItem)`（`Merchant.cs:1599`）。
///
/// 纯函数化：`gold` 为入参余额，`user_item` 为可变引用（持久变化写回）。
/// 特殊修理 `Dura = DuraMax`；普通修理先 `DuraMax -= (DuraMax - Dura) / RepairItemDecDura`
/// （**C# 整型除法**）再 `Dura = DuraMax`。
pub fn repair_item(
    tables: &PriceTables<'_>,
    cfg: &ShopConfig,
    user_item: &mut UserItem,
    gold: i32,
    label: RepairLabel,
    is_repair: bool,
    is_sup_repair: bool,
) -> RepairItemResult {
    let sup_repair = label == RepairLabel::SuperRepair;
    let mut result = RepairItemResult {
        outcome: RepairOutcome::NoFrame,
        gold,
        dura: user_item.dura,
        dura_max: user_item.dura_max,
    };
    if sup_repair && !is_sup_repair {
        return result; // 不支持特殊修理
    }
    if !sup_repair && !is_repair {
        return result; // 不支持修理和特殊修理
    }
    if label == RepairLabel::SuperRepairFail {
        result.outcome = RepairOutcome::Fail;
        return result;
    }
    let mut n_price = get_user_price(cfg, get_user_item_price(tables, user_item));
    if sup_repair {
        n_price *= cfg.super_repair_price_rate;
    }
    let Some(std_item) = tables.source.get_std_item(user_item.index) else {
        return result; // C# `if (stdItem != null)` 无 else ⇒ 无帧
    };
    if n_price > 0 && user_item.dura_max > user_item.dura && std_item.std_mode != 43 {
        let n_repair_price = if user_item.dura_max > 0 {
            hutil::round(
                f64::from(n_price) / 3.0 / f64::from(user_item.dura_max)
                    * f64::from(dura_delta_int(user_item)),
            )
        } else {
            n_price
        };
        if gold >= n_repair_price {
            // DecGold
            result.gold = gold - n_repair_price;
            if sup_repair {
                user_item.dura = user_item.dura_max;
            } else {
                // C# `(ushort)((DuraMax - Dura) / RepairItemDecDura)`（向零截断）。
                let delta = dura_delta_int(user_item) / cfg.repair_item_dec_dura;
                let new_dura_max = i32::from(user_item.dura_max) - i32::from(delta as u16);
                user_item.dura_max = new_dura_max as u16;
                user_item.dura = user_item.dura_max;
            }
            result.outcome = RepairOutcome::Ok;
            result.dura = user_item.dura;
            result.dura_max = user_item.dura_max;
        } else {
            result.outcome = RepairOutcome::Fail;
            result.dura = user_item.dura;
            result.dura_max = user_item.dura_max;
        }
    } else {
        result.outcome = RepairOutcome::Fail;
        result.dura = user_item.dura;
        result.dura_max = user_item.dura_max;
    }
    result
}
