//! 税收 / 城堡金库（`UserCastle.IncRateGold` + `CastleManager.IncRateGold` + 商人税）
//! —— M3⑧-实现 9（卡 `openmir2-m3-castle-gold-tax`）。
//!
//! 盘点依据：`mir2-rs/tests/parity/m3/M3⑧公会攻城现状盘点报告-20261013.md` §1.G（税收/金库）
//! / §1.K（配置项）/ §1.L（N1 断点）/ §3 卡 9。oracle 锚（行号均在本仓库 `src/` 内实测）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`UserCastle::inc_rate_gold`] | `src/M2Server/Castle/UserCastle.cs:768-812`（`IncRateGold`） |
//! | [`CastleManager::inc_rate_gold`] | `src/M2Server/Castle/CastleManager.cs:209-215`（升序遍历 `CastleList`） |
//! | [`CastleManager::get_castle`] | `CastleManager.cs:184-192`（越界 ⇒ `null`） |
//! | [`merchant_tax_target`] / [`merchant_tax_amount`] | `src/GameSrv/Npc/Merchant.cs:544-553` / `:1250-1259` / `:1385-1393` / `:1637-1645`（四个沙巴克商人税调用点） |
//! | 成员价 80% | `Merchant.cs:765-782`（`GetUserPrice`）——定价本体已在 `crates/formula/src/shop.rs::get_user_price` 落地（⑤ 家族）；本模块只做四调用点取价链/税意图的纯封装（[`merchant_call_price`] / [`merchant_tax_intent`]），**不重写公式** |
//! | 金库提取/上缴 | `UserCastle.cs:814-889`（真身重载 `PlayObject`）+ `:599-602` / `:911-914`（接口重载 **N1**） |
//!
//! ## N1：金库提取/上缴在 oracle 里**不可达**（owner 裁决 comment `a137cc900c`，白名单 **T-38**）
//!
//! `CastleOfficial.UserSelect`（`CastleOfficial.cs:155-183`）的 `Castle` 静态类型是
//! `IUserCastle`（`BaseObject.cs:153`）⇒ `Castle.WithDrawalGolds(PlayObject, …)` /
//! `Castle.ReceiptGolds(PlayObject, …)` 的 C# 重载决议只认**接口成员**
//! `WithDrawalGolds(IPlayerActor,int)`（`UserCastle.cs:599-602`）/ `ReceiptGolds(IPlayerActor,int)`
//! （`:911-914`）——两个都是 `throw new NotImplementedException`，被 `UserSelect` 外层 `catch` 吞掉。
//! 真身重载（`WithDrawalGolds(PlayObject)` `:814-849` / `ReceiptGolds(PlayObject)` `:850-889`）
//! **永远选不中**。⇒ 可观测行为 = **静默无效果**；⑧-7（`openmir2-m3-castle-siege-state`）已按
//! T-38 把两个入口建模为可观测 no-op（[`UserCastle::with_drawal_golds_n1`] /
//! [`UserCastle::receipt_golds_n1`]），本模块**复验**这一 no-op，**不**实现「本该有」的提取/上缴。
//!
//! ## 照搬的 oracle 行为（**不是**差异）
//!
//! 1. **`CastleMgr` 分支恒用 `UpgradeWeaponPrice`**：`Merchant.cs:1250-1259/1385-1393/1637-1645`
//!    的 `else if (GetAllNpcTax)` 分支传的是 `SystemShare.Config.UpgradeWeaponPrice`，**不是**本次
//!    交易的 `nPrice`/`nRepairPrice`。白名单 **B-16** 登记（⑤-4 的 T-47 只登记了「条件照抄」，
//!    本行钉死「税基常量」这条更具体的怪行为）。
//! 2. **`TodayIncome` 日上限的裁剪**（`UserCastle.cs:771-788`）：超额时 `nInGold` 取
//!    「上限 − 当前」，**已到顶**时取 `0`；`TotalGold` 的封顶用**严格小于**判断
//!    （`:789`），相等时走 `= CastleGoldMax` 分支（结果同值）。
//! 3. **`SaveTick` 门是严格大于 10 分钟**（`:798`），且**与 `nGold` 是否为 0 无关**
//!    （`nGold=0` 也刷新 `SaveTick`）。
//!
//! ## 确定性
//!
//! 本模块**零 RNG、零墙钟**：时间来自调用方注入的 `tick_ms`（`HUtil32.GetTickCount()` 口径；
//! 构造级直接给值）。对应 ⑧-1「可注入时钟」在 **Rust 构造级**的用法；真机两侧时间门对拍仍需
//! ⑧-1 的 C# 时钟注入片 + 独占窗口。
//!
//! ## 模块边界
//!
//! 本模块只放 ⑧-9 的**税收/金库**面（`IncRateGold` + `CastleManager` 聚合 + 商人税判定）；
//! 攻城状态机本体在 [`crate::castle`]，门/墙状态属 ⑧-8。放独立文件（而非继续膨胀
//! `castle.rs`）是为了让未合的 ⑧-8 片与 ⑧-9 的 `git merge` 冲突面最小。

use mir2_data::guild_castle::GuildCastleConfig;
use mir2_formula::shop::ShopConfig;
use mir2_shared::hutil;

use crate::castle::UserCastle;

/// `(HUtil32.GetTickCount() - SaveTick) > 10 * 60 * 1000`（`UserCastle.cs:798`）。
pub const CASTLE_GOLD_SAVE_INTERVAL_MS: i64 = 10 * 60 * 1000;

/// `SystemShare.Config` 里税收/金库读到的字段（盘点 §1.K；默认值 = `GameSvrConf.cs` 初值）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastleTaxConfig {
    /// `Config.CastleTaxRate`（`GameSvrConf.cs:960/1788`，默认 `5`）——百分比税比。
    pub castle_tax_rate: i32,
    /// `Config.CastleOneDayGold`（`GameSvrConf.cs:965/1793`，默认 `2_000_000`）——`TodayIncome` 日上限。
    pub castle_one_day_gold: i32,
    /// `Config.CastleGoldMax`（`GameSvrConf.cs:964/1792`，默认 `10_000_000`）——`TotalGold` 上限。
    pub castle_gold_max: i32,
    /// `Config.CastleGetAllNpcTax`（`GameSvrConf.cs:968` 附近，默认 `false`）——无城堡商人也收税。
    pub get_all_npc_tax: bool,
    /// `Config.UpgradeWeaponPrice`（`GameSvrConf.cs:275/1420`，默认 `10_000`）——
    /// `CastleMgr` 分支的**税基常量**（见模块头「照搬 ①」）。
    pub upgrade_weapon_price: i32,
    /// `Config.CastleMemberPriceRate`（`GameSvrConf.cs:968/1796`，默认 `80`）——城堡成员折扣；
    /// 实际定价在 `crates/formula/src/shop.rs::get_user_price`，本模块经
    /// [`merchant_call_price`] 调用（不重写公式）。
    pub castle_member_price_rate: i32,
}

impl Default for CastleTaxConfig {
    /// `GameSvrConf.cs` 初值（`CastleGoldMax` 默认 `10_000_000`；现网 `Server.conf` 覆盖成 `5_000_000`）。
    fn default() -> Self {
        CastleTaxConfig {
            castle_tax_rate: 5,
            castle_one_day_gold: 2_000_000,
            castle_gold_max: 10_000_000,
            get_all_npc_tax: false,
            upgrade_weapon_price: 10_000,
            castle_member_price_rate: 80,
        }
    }
}

impl CastleTaxConfig {
    /// 从 `Server.conf` 的 [`GuildCastleConfig`] 取税收相关键。
    ///
    /// `UpgradeWeaponPrice` 不是 `[Setup]` 键（`crates/data` 不读）⇒ 由调用方按 `GameSvrConf` 传入。
    #[must_use]
    pub fn from_guild_castle_config(cfg: &GuildCastleConfig, upgrade_weapon_price: i32) -> Self {
        CastleTaxConfig {
            castle_tax_rate: cfg.castle_tax_rate,
            castle_one_day_gold: cfg.castle_one_day_gold,
            castle_gold_max: cfg.castle_gold_max,
            get_all_npc_tax: cfg.get_all_npc_tax,
            upgrade_weapon_price,
            castle_member_price_rate: cfg.castle_member_price_rate,
        }
    }
}

/// 一次 `IncRateGold` 的**可观测结果**（oracle 返回 `void`；本结构仅供判据/日志，
/// 不改变任何可观测副作用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IncRateGoldOutcome {
    /// 本次**真正计入 `TotalGold`** 的金额（日上限裁剪后的 `nInGold`）。
    pub n_in_gold: i32,
    /// 调用后的 `TodayIncome`。
    pub today_income: i32,
    /// 调用后的 `TotalGold`。
    pub total_gold: i32,
    /// `(tick_ms - SaveTick) > 10min` ⇒ 本调用刷新了 `SaveTick`。
    pub save_tick_fired: bool,
}

impl UserCastle {
    /// oracle `UserCastle.IncRateGold(int nGold)`（`UserCastle.cs:768-812`）。
    ///
    /// 逐行照搬：`Round(nGold * CastleTaxRate/100.0)`（`MidpointRounding.AwayFromZero`）→
    /// `TodayIncome` 日上限裁剪（超额取余量、到顶取 0）→ `nInGold > 0` 时 `TotalGold` 封顶
    /// （**严格小于** `CastleGoldMax`）→ `SaveTick` 10 分钟门（严格大于，且与 `nGold` 无关）。
    pub fn inc_rate_gold(
        &mut self,
        cfg: &CastleTaxConfig,
        n_gold: i32,
        tick_ms: i64,
    ) -> IncRateGoldOutcome {
        let mut n_in_gold =
            hutil::round(f64::from(n_gold) * (f64::from(cfg.castle_tax_rate) / 100.0));
        if self.today_income + n_in_gold <= cfg.castle_one_day_gold {
            self.today_income += n_in_gold;
        } else if self.today_income >= cfg.castle_one_day_gold {
            n_in_gold = 0;
        } else {
            n_in_gold = cfg.castle_one_day_gold - self.today_income;
            self.today_income = cfg.castle_one_day_gold;
        }
        if n_in_gold > 0 {
            if self.total_gold + n_in_gold < cfg.castle_gold_max {
                self.total_gold += n_in_gold;
            } else {
                self.total_gold = cfg.castle_gold_max;
            }
        }
        let save_tick_fired = (tick_ms - self.save_gold_tick) > CASTLE_GOLD_SAVE_INTERVAL_MS;
        if save_tick_fired {
            self.save_gold_tick = tick_ms;
        }
        IncRateGoldOutcome {
            n_in_gold,
            today_income: self.today_income,
            total_gold: self.total_gold,
            save_tick_fired,
        }
    }
}

/// oracle `CastleManager`（`src/M2Server/Castle/CastleManager.cs`）的**税收面**。
///
/// 完整的城堡管理（`Initialize`/`InCastleWarArea`/`IsCastleMember`/落盘等）不属本片写集；
/// 本片只取 `IncRateGold` 需要的聚合（`CastleList` + 升序 `IncRateGold` + `GetCastle`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CastleManager {
    /// `CastleList`（`CastleManager.cs:14`）。
    pub castle_list: Vec<UserCastle>,
}

impl CastleManager {
    /// 空管理器（oracle ctor `CastleManager.cs:16-19`）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// oracle `CastleManager.IncRateGold(int nGold)`（`CastleManager.cs:209-215`）：
    /// **升序**遍历 `CastleList` 对每座城调用 `UserCastle.IncRateGold`。
    ///
    /// 返回值逐项对应 `castle_list` 的顺序（oracle 返回 `void`）。
    pub fn inc_rate_gold(
        &mut self,
        cfg: &CastleTaxConfig,
        n_gold: i32,
        tick_ms: i64,
    ) -> Vec<IncRateGoldOutcome> {
        self.castle_list
            .iter_mut()
            .map(|c| c.inc_rate_gold(cfg, n_gold, tick_ms))
            .collect()
    }

    /// oracle `CastleManager.GetCastle(int nIndex)`（`CastleManager.cs:184-192`）：
    /// `0 <= nIndex < CastleList.Count` 才返回，否则 `None`。
    #[must_use]
    pub fn get_castle(&self, index: i32) -> Option<&UserCastle> {
        if index < 0 {
            return None;
        }
        self.castle_list.get(index as usize)
    }
}

/// 沙巴克商人税的**目的地**（`Merchant.cs` 四个调用点）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MerchantTaxTarget {
    /// 商人所属城堡（`Castle != null`）——税基 = 调用点价格。
    Castle,
    /// `SystemShare.CastleMgr`（`Castle == null && Config.GetAllNpcTax`）——税基 = `UpgradeWeaponPrice`。
    CastleManager,
}

/// oracle `Merchant.cs:544-553/1250-1259/1385-1393/1637-1645` 的**目的地判定**：
///
/// ```text
/// if (CastleMerchant || Config.GetAllNpcTax) {
///     if (Castle != null)       → Castle
///     else if (GetAllNpcTax)    → CastleManager
///     else                      → 无（CastleMerchant=true 但无城堡、也不全局收税）
/// } else                        → 无
/// ```
#[must_use]
pub fn merchant_tax_target(
    castle_merchant: bool,
    get_all_npc_tax: bool,
    castle_present: bool,
) -> Option<MerchantTaxTarget> {
    if !(castle_merchant || get_all_npc_tax) {
        return None;
    }
    if castle_present {
        Some(MerchantTaxTarget::Castle)
    } else if get_all_npc_tax {
        Some(MerchantTaxTarget::CastleManager)
    } else {
        None
    }
}

/// 目的地分支的**税基**：
///
/// - [`MerchantTaxTarget::Castle`] ⇒ 调用点价格（`nPrice`/`nRepairPrice`；升级武器的调用点
///   `:544` 传的本身就是 `UpgradeWeaponPrice`）；
/// - [`MerchantTaxTarget::CastleManager`] ⇒ **恒为** `Config.UpgradeWeaponPrice`（照搬：见模块头「照搬 ①」，白名单 **B-16**）。
#[must_use]
pub fn merchant_tax_amount(
    target: MerchantTaxTarget,
    price: i32,
    upgrade_weapon_price: i32,
) -> i32 {
    match target {
        MerchantTaxTarget::Castle => price,
        MerchantTaxTarget::CastleManager => upgrade_weapon_price,
    }
}

/// 一次成交调用点的**税意图**（纯计算：只决定「税进哪座城、按多少钱计税」，不改状态）。
///
/// `Merchant.cs` 的四个调用点（`:544-553` 升级 / `:1250-1259` 买入 / `:1385-1393` 卖出 /
/// `:1637-1645` 修理）形状完全相同：先 [`merchant_tax_target`] 判目的地，再
/// [`merchant_tax_amount`] 取税基。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MerchantTaxIntent {
    /// 税进哪座城（`Castle`）还是 `CastleMgr` 全表。
    pub target: MerchantTaxTarget,
    /// 传给 `IncRateGold` 的税基（**已按分支**：`CastleMgr` 恒 `UpgradeWeaponPrice`，B-16）。
    pub amount: i32,
}

/// `Merchant.cs` 的四个**收税成交调用点**。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MerchantCallPoint {
    /// 买入（`Merchant.cs:1244-1259`）：`n_price` = `GetUserItemPrice(UserItem)` 的 `double` 结果。
    /// 价格 = `GetUserPrice(n_price)`（成员折后）；税基 = 折后成交价。
    Buy { n_price: f64 },
    /// 卖出（`Merchant.cs:1380-1393`）：`n_price` 同上。
    /// 价格 = `GetSellItemPrice(n_price)` = `Round(n_price / 2.0)`，**永不打成员折**
    /// （`GetSellItemPrice` 不经过 `GetUserPrice`）；税基 = 卖价。
    Sell { n_price: f64 },
    /// 普通/特殊修理（`Merchant.cs:1616-1645`）：价格 = `GetUserPrice(n_price)`，特修再乘
    /// `Config.SuperRepairPriceRate`，最后按耐久比例
    /// `Round(price / 3.0 / DuraMax * (DuraMax - Dura))` 收口（**先乘倍率、后按耐久比例**）；
    /// `DuraMax == 0` 时直接用乘完倍率的价（oracle `else nRepairPrice = nPrice`）。税基 = 修理价。
    ///
    /// 调用方须先按 oracle 的门外过滤（`Merchant.cs:1627`：`nPrice > 0 && DuraMax > Dura
    /// && StdMode != 43`）；本函数假设 `DuraMax >= Dura`（否则 `DuraMax - Dura` 在 C# 的
    /// `ushort` 口径会回绕，本层不复刻该不可达支）。
    Repair {
        n_price: f64,
        dura: i32,
        dura_max: i32,
        super_repair: bool,
    },
    /// 武器升级提交（`Merchant.cs:540-553`）：价格 = `Config.UpgradeWeaponPrice`（固定常量，
    /// **不**走 `GetUserPrice`、不吃成员折）；税基 = 同一常量（oracle 传的也是 `UpgradeWeaponPrice`）。
    UpgradeWeapon,
}

/// 一次成交的纯结算：成交价 + 应记的税（`None` = 不收税）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MerchantCallSettlement {
    pub price: i32,
    pub tax: Option<MerchantTaxIntent>,
}

/// 四个成交调用点的**成交价**纯计算（只做取价链，**不改**背包/金币）。
///
/// 定价本体（`GetUserPrice`/`GetSellItemPrice` 的取整与 `max(60, …)` 下限）仍由
/// `crates/formula/src/shop.rs` 承担（⑤ 家族已逐行对拍，本模块**不重写**）；本函数只负责
/// 「哪个调用点走哪条取价链」：
///
/// | 调用点 | oracle | 取价链 |
/// | --- | --- | --- |
/// | [`MerchantCallPoint::Buy`] | `Merchant.cs:1244` | `GetUserPrice(GetUserItemPrice)` |
/// | [`MerchantCallPoint::Sell`] | `Merchant.cs:1380` | `GetSellItemPrice(GetUserItemPrice)` |
/// | [`MerchantCallPoint::Repair`] | `Merchant.cs:1616` | `GetUserPrice` → 特修倍率 → 耐久比例 |
/// | [`MerchantCallPoint::UpgradeWeapon`] | `Merchant.cs:540` | 固定 `UpgradeWeaponPrice` |
///
/// `shop.castle_merchant` / `shop.castle_member` 必须已由调用方按
/// [`MerchantCastleBinding::member_price_applies`] 解析（见 [`MerchantPriceContext`]）。
#[must_use]
pub fn merchant_call_price(
    shop: &ShopConfig,
    upgrade_weapon_price: i32,
    call: MerchantCallPoint,
) -> i32 {
    use mir2_formula::shop::{get_sell_item_price, get_user_price};

    match call {
        MerchantCallPoint::Buy { n_price } => get_user_price(shop, n_price),
        MerchantCallPoint::Sell { n_price } => get_sell_item_price(n_price),
        MerchantCallPoint::Repair {
            n_price,
            dura,
            dura_max,
            super_repair,
        } => {
            let mut price = get_user_price(shop, n_price);
            if super_repair {
                price *= shop.super_repair_price_rate;
            }
            if dura_max > 0 {
                // oracle `HUtil32.Round(nPrice / 3.0 / DuraMax * (DuraMax - Dura))`：
                // 全部 `double` 运算，最后一次 AwayFromZero 取整。
                hutil::round(
                    f64::from(price) / 3.0 / f64::from(dura_max) * f64::from(dura_max - dura),
                )
            } else {
                price
            }
        }
        MerchantCallPoint::UpgradeWeapon => upgrade_weapon_price,
    }
}

/// 四个调用点的**税意图**纯计算（oracle `Merchant.cs` 四处的 `if (CastleMerchant || GetAllNpcTax)` 块）。
#[must_use]
pub fn merchant_tax_intent(
    price: i32,
    castle_merchant: bool,
    get_all_npc_tax: bool,
    castle_present: bool,
    upgrade_weapon_price: i32,
) -> Option<MerchantTaxIntent> {
    let target = merchant_tax_target(castle_merchant, get_all_npc_tax, castle_present)?;
    Some(MerchantTaxIntent {
        target,
        amount: merchant_tax_amount(target, price, upgrade_weapon_price),
    })
}

/// 商人 ⟷ 城堡的**纯数据模型**（oracle `Merchant.CastleMerchant` + `NormNpc.Castle`）。
///
/// - `castle_merchant`：`MerChant.txt` 第 8 列（`LocalDB.cs:461-464`），**加载期固定**；
/// - `castle_index`：`Merchant.Initialize()` 的 `Castle = CastleMgr.InCastleWarArea(this)`
///   （`NormNpc.cs:1674-1678`）在 **spawn 时刻**命中的城堡下标；`Merchant.Run → MapRandomMove`
///   之后**不重算**（`design-castle-tax-and-item-name-20261022.md` §1.2.5）。
///
/// `castle_index = Some(i)` ⇔ oracle `Castle != null`；与 `castle_merchant` **互相独立**
/// （任何商人只要站在攻城区/皇宫/密道/`EnvirList` 图内，`Castle` 都会命中）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MerchantCastleBinding {
    /// `IMerchant.CastleMerchant`（`MerChant.txt` 第 8 列）。
    pub castle_merchant: bool,
    /// `NormNpc.Castle` 在 `CastleMgr.CastleList` 里的下标（`None` ⇔ `Castle == null`）。
    pub castle_index: Option<usize>,
}

impl MerchantCastleBinding {
    #[must_use]
    pub fn new(castle_merchant: bool, castle_index: Option<usize>) -> Self {
        MerchantCastleBinding {
            castle_merchant,
            castle_index,
        }
    }

    /// oracle `Castle != null`（税分支与成员价门都用它）。
    #[must_use]
    pub fn castle_present(&self) -> bool {
        self.castle_index.is_some()
    }

    /// 四个调用点的目的地判定（委托 [`merchant_tax_target`]）。
    #[must_use]
    pub fn tax_target(&self, get_all_npc_tax: bool) -> Option<MerchantTaxTarget> {
        merchant_tax_target(self.castle_merchant, get_all_npc_tax, self.castle_present())
    }

    /// `CastleMerchant && Castle != null && Castle.IsMasterGuild(MyGuild)`（`Merchant.cs:769`）。
    ///
    /// 只有**城主行会**成员在**自己那座城**的商人处才吃成员价；同城其它行会、盟友、攻方、
    /// 无行会都不打折。`castles` 越界（畸形态）⇒ `false`（oracle 取到的 `Castle` 必然有效）。
    #[must_use]
    pub fn member_price_applies(&self, castles: &[UserCastle], guild_name: Option<&str>) -> bool {
        if !self.castle_merchant {
            return false;
        }
        self.castle_index
            .and_then(|index| castles.get(index))
            .is_some_and(|castle| castle.is_master_guild_name(guild_name))
    }
}

/// 一次商人成交的**纯计算上下文**：商人绑定 + 玩家行会名 + 定价/税收配置。
///
/// 宿主只需把 [`MerchantPriceContext::settlement`] 的结果落到状态（金币/税/帧），
/// 不重复实现取价链与税分支。
#[derive(Clone, Copy, Debug)]
pub struct MerchantPriceContext<'a> {
    /// `Merchant` 侧的定价配置（`PriceRate` / `CastleMemberPriceRate` / 特修倍率）。
    pub shop: &'a ShopConfig,
    /// `SystemShare.Config` 的税收面。
    pub tax: &'a CastleTaxConfig,
    /// 商人 ⟷ 城堡绑定。
    pub binding: MerchantCastleBinding,
    /// `CastleMgr.CastleList`（成员判定按下标取城）。
    pub castles: &'a [UserCastle],
    /// 玩家行会名（`GuildMgr.MemberOfGuild(ChrName)`；无行会 = `None`）。
    pub guild_name: Option<&'a str>,
}

impl<'a> MerchantPriceContext<'a> {
    #[must_use]
    pub fn new(
        shop: &'a ShopConfig,
        tax: &'a CastleTaxConfig,
        binding: MerchantCastleBinding,
        castles: &'a [UserCastle],
        guild_name: Option<&'a str>,
    ) -> Self {
        MerchantPriceContext {
            shop,
            tax,
            binding,
            castles,
            guild_name,
        }
    }

    /// 把绑定解析成 `GetUserPrice` 读的两扇门：`castle_merchant`（外层）与
    /// `castle_member`（`Castle != null && IsMasterGuild`）。
    #[must_use]
    pub fn effective_shop_config(&self) -> ShopConfig {
        let mut cfg = *self.shop;
        cfg.castle_merchant = self.binding.castle_merchant;
        cfg.castle_member = self
            .binding
            .member_price_applies(self.castles, self.guild_name);
        cfg
    }

    /// 调用点成交价（纯函数）。
    #[must_use]
    pub fn price(&self, call: MerchantCallPoint) -> i32 {
        merchant_call_price(
            &self.effective_shop_config(),
            self.tax.upgrade_weapon_price,
            call,
        )
    }

    /// 调用点成交价 + 税意图（纯函数；**不改**城堡金库，落账由调用方做）。
    #[must_use]
    pub fn settlement(&self, call: MerchantCallPoint) -> MerchantCallSettlement {
        let price = self.price(call);
        let tax = merchant_tax_intent(
            price,
            self.binding.castle_merchant,
            self.tax.get_all_npc_tax,
            self.binding.castle_present(),
            self.tax.upgrade_weapon_price,
        );
        MerchantCallSettlement { price, tax }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::castle::{CastleDate, CastleNow, CastleSiegeConfig, GuildId};

    fn tax() -> CastleTaxConfig {
        CastleTaxConfig {
            castle_tax_rate: 5,
            castle_one_day_gold: 2_000_000,
            castle_gold_max: 5_000_000,
            get_all_npc_tax: false,
            upgrade_weapon_price: 10_000,
            castle_member_price_rate: 80,
        }
    }

    fn empty_castle(name: &str) -> UserCastle {
        let mut c = UserCastle::new(name, CastleSiegeConfig::default());
        c.map_name = Some("3".to_string());
        c.server_index = 0;
        c.map_owner_server_index = 0;
        c
    }

    /// `HUtil32.Round`（AwayFromZero）而不是截断——`10 * 5% = 0.5 ⇒ 1`、`110 * 5% = 5.5 ⇒ 6`。
    #[test]
    fn inc_rate_gold_rounds_away_from_zero_like_hutil() {
        let cfg = tax();

        let mut c = empty_castle("沙巴克");
        let out = c.inc_rate_gold(&cfg, 10, 0);
        assert_eq!(out.n_in_gold, 1, "0.5 必须进位（截断会得 0）");
        assert_eq!(c.today_income, 1);
        assert_eq!(c.total_gold, 1);

        let mut c = empty_castle("沙巴克");
        let out = c.inc_rate_gold(&cfg, 110, 0);
        assert_eq!(out.n_in_gold, 6, "5.5 必须进位");
        assert_eq!(c.total_gold, 6);

        let mut c = empty_castle("沙巴克");
        let out = c.inc_rate_gold(&cfg, 90, 0);
        assert_eq!(out.n_in_gold, 5, "4.5 远离零 ⇒ 5");
        assert_eq!(c.total_gold, 5);
    }

    /// 日上限裁剪（`UserCastle.cs:771-788`）：超额取余量、到顶取 0。
    #[test]
    fn inc_rate_gold_caps_today_income_and_rolls_remainder() {
        let cfg = tax();

        let mut c = empty_castle("沙巴克");
        c.today_income = cfg.castle_one_day_gold - 3;
        let out = c.inc_rate_gold(&cfg, 110, 0); // 5.5 ⇒ 6，超出余量 3
        assert_eq!(c.today_income, cfg.castle_one_day_gold);
        assert_eq!(out.n_in_gold, 3, "只记余量 3");
        assert_eq!(c.total_gold, 3);

        // 已到顶 ⇒ nInGold = 0、TodayIncome 不变、TotalGold 不变。
        let before_gold = c.total_gold;
        let out = c.inc_rate_gold(&cfg, 110, 0);
        assert_eq!(out.n_in_gold, 0);
        assert_eq!(c.today_income, cfg.castle_one_day_gold);
        assert_eq!(c.total_gold, before_gold);
    }

    /// `TotalGold` 封顶用**严格小于**（`UserCastle.cs:789`；相等时也走 `= CastleGoldMax`）。
    #[test]
    fn inc_rate_gold_caps_total_gold_at_castle_gold_max() {
        let cfg = tax();

        let mut c = empty_castle("沙巴克");
        c.total_gold = cfg.castle_gold_max - 2;
        let out = c.inc_rate_gold(&cfg, 110, 0); // 6 ⇒ 2+6=8 > 上限
        assert_eq!(out.n_in_gold, 6, "nInGold 是裁剪前的进账额");
        assert_eq!(c.total_gold, cfg.castle_gold_max);

        // 恰好加满（strict `<` 不成立）⇒ 仍封顶到同值。
        let mut c = empty_castle("沙巴克");
        c.total_gold = cfg.castle_gold_max - 6;
        c.inc_rate_gold(&cfg, 110, 0);
        assert_eq!(c.total_gold, cfg.castle_gold_max);

        // 已经超过上限（畸形态）⇒ 加任何正数都回落到上限。
        let mut c = empty_castle("沙巴克");
        c.total_gold = cfg.castle_gold_max + 100;
        c.inc_rate_gold(&cfg, 110, 0);
        assert_eq!(c.total_gold, cfg.castle_gold_max);
    }

    /// `nGold = 0`：`nInGold = 0`（金额不变），但 `SaveTick` 门**照样**评估/刷新（`UserCastle.cs:798` 在门内不判金额）。
    #[test]
    fn inc_rate_gold_zero_price_keeps_totals_but_still_advances_save_tick() {
        let cfg = tax();
        let mut c = empty_castle("沙巴克");
        c.total_gold = 123;
        c.today_income = 45;
        let out = c.inc_rate_gold(&cfg, 0, CASTLE_GOLD_SAVE_INTERVAL_MS + 1);
        assert_eq!(out.n_in_gold, 0);
        assert_eq!(c.total_gold, 123);
        assert_eq!(c.today_income, 45);
        assert!(out.save_tick_fired, "金额为 0 也刷新 SaveTick");
        assert_eq!(c.save_gold_tick, CASTLE_GOLD_SAVE_INTERVAL_MS + 1);
    }

    /// `SaveTick` 门：严格大于 10 分钟；刷新后从新 `SaveTick` 起重新计时。
    #[test]
    fn inc_rate_gold_save_tick_is_strictly_greater_than_ten_minutes() {
        let cfg = tax();
        let mut c = empty_castle("沙巴克");

        // t=0：`0 - 0 = 0` 不 > 10min ⇒ 不刷。
        assert!(!c.inc_rate_gold(&cfg, 100, 0).save_tick_fired);
        assert_eq!(c.save_gold_tick, 0);

        // t=10min 整：等于 ⇒ 不刷。
        assert!(
            !c.inc_rate_gold(&cfg, 100, CASTLE_GOLD_SAVE_INTERVAL_MS)
                .save_tick_fired
        );
        assert_eq!(c.save_gold_tick, 0);

        // t=10min+1 ⇒ 刷。
        assert!(
            c.inc_rate_gold(&cfg, 100, CASTLE_GOLD_SAVE_INTERVAL_MS + 1)
                .save_tick_fired
        );
        assert_eq!(c.save_gold_tick, CASTLE_GOLD_SAVE_INTERVAL_MS + 1);

        // 紧接着再调（差 1ms）⇒ 不刷。
        assert!(
            !c.inc_rate_gold(&cfg, 100, CASTLE_GOLD_SAVE_INTERVAL_MS + 2)
                .save_tick_fired
        );
        assert_eq!(c.save_gold_tick, CASTLE_GOLD_SAVE_INTERVAL_MS + 1);

        // 再过 10min+1 ⇒ 又刷。
        assert!(
            c.inc_rate_gold(&cfg, 100, 2 * CASTLE_GOLD_SAVE_INTERVAL_MS + 2)
                .save_tick_fired
        );
        assert_eq!(c.save_gold_tick, 2 * CASTLE_GOLD_SAVE_INTERVAL_MS + 2);
    }

    /// `CastleManager.IncRateGold`：升序遍历全部城堡（每座城各自的 `TodayIncome`/`TotalGold`）。
    #[test]
    fn castle_manager_inc_rate_gold_visits_every_castle_in_order() {
        let cfg = tax();
        let mut mgr = CastleManager::new();
        let mut a = empty_castle("城A");
        a.total_gold = 100;
        let mut b = empty_castle("城B");
        b.total_gold = 200;
        b.today_income = cfg.castle_one_day_gold; // 城B 已到日顶
        mgr.castle_list.push(a);
        mgr.castle_list.push(b);

        let outs = mgr.inc_rate_gold(&cfg, 1000, 0); // 5% = 50
        assert_eq!(outs.len(), 2);
        assert_eq!(outs[0].n_in_gold, 50);
        assert_eq!(mgr.castle_list[0].total_gold, 150);
        assert_eq!(mgr.castle_list[0].today_income, 50);
        // 城B 已到日顶 ⇒ nInGold = 0，TotalGold 不动。
        assert_eq!(outs[1].n_in_gold, 0);
        assert_eq!(mgr.castle_list[1].total_gold, 200);
        assert_eq!(mgr.castle_list[1].today_income, cfg.castle_one_day_gold);

        // GetCastle 越界口径。
        assert!(mgr.get_castle(0).is_some());
        assert_eq!(mgr.get_castle(1).expect("城B").name, "城B");
        assert!(mgr.get_castle(2).is_none());
        assert!(mgr.get_castle(-1).is_none());
    }

    /// 商人税目的地表（`Merchant.cs` 四调用点的条件分支）。
    #[test]
    fn merchant_tax_target_matches_oracle_branches() {
        // 两个开关都关 ⇒ 不收。
        assert_eq!(merchant_tax_target(false, false, false), None);
        assert_eq!(merchant_tax_target(false, false, true), None);
        // 有城堡 ⇒ 无论哪个开关，都收进自己的城。
        assert_eq!(
            merchant_tax_target(true, false, true),
            Some(MerchantTaxTarget::Castle)
        );
        assert_eq!(
            merchant_tax_target(false, true, true),
            Some(MerchantTaxTarget::Castle)
        );
        // 无城堡：只有 GetAllNpcTax 才走管理器。
        assert_eq!(merchant_tax_target(true, false, false), None);
        assert_eq!(
            merchant_tax_target(false, true, false),
            Some(MerchantTaxTarget::CastleManager)
        );
        assert_eq!(
            merchant_tax_target(true, true, false),
            Some(MerchantTaxTarget::CastleManager)
        );
    }

    /// 税基：`Castle` 用调用点价格；`CastleMgr` 恒用 `UpgradeWeaponPrice`（照搬，B-16）。
    #[test]
    fn merchant_tax_amount_uses_upgrade_weapon_price_for_manager_branch() {
        assert_eq!(
            merchant_tax_amount(MerchantTaxTarget::Castle, 777, 10_000),
            777
        );
        assert_eq!(
            merchant_tax_amount(MerchantTaxTarget::CastleManager, 777, 10_000),
            10_000,
            "管理器分支忽略本次价格，恒用 UpgradeWeaponPrice"
        );
    }

    /// 税收与攻城状态机的接缝：跨天 `Run` 清 `TodayIncome`，但 `TotalGold` **保留**。
    #[test]
    fn cross_day_run_resets_today_income_but_keeps_total_gold() {
        let cfg = tax();
        let mut c = empty_castle("沙巴克");
        c.income_today = CastleDate::new(2026, 10, 13);
        c.inc_rate_gold(&cfg, 100_000, 0); // 5% = 5000
        assert_eq!(c.today_income, 5_000);
        assert_eq!(c.total_gold, 5_000);

        // 次日 run（非开战钟点）⇒ TodayIncome 归零、TotalGold 不动。
        c.run(CastleNow::new(CastleDate::new(2026, 10, 14), 1, 1_000));
        assert_eq!(c.today_income, 0);
        assert_eq!(c.total_gold, 5_000);

        // 同日再收 ⇒ 从新的一天重新计。
        c.inc_rate_gold(&cfg, 100_000, 2_000);
        assert_eq!(c.today_income, 5_000);
        assert_eq!(c.total_gold, 10_000);
    }

    /// N1（T-38）：金库提取/上缴入口 = 可观测 no-op；与真实生效的 `IncRateGold` 对照，
    /// 证明 ⑧-9 只对「存得进」这条链建模状态变化。
    #[test]
    fn vault_withdrawal_and_receipt_stay_n1_noop_while_tax_changes_state() {
        let cfg = tax();
        let mut c = empty_castle("沙巴克");
        c.total_gold = 1_000;
        c.inc_rate_gold(&cfg, 20_000, 0); // 5% = 1000 ⇒ total 2000
        assert_eq!(c.total_gold, 2_000);

        let before = c.clone();
        // 两个接口重载（`UserCastle.cs:601/:913`）在 oracle 抛异常被吞 ⇒ no-op。
        assert_eq!(c.with_drawal_golds_n1(GuildId(7), 500), -1);
        assert_eq!(c.receipt_golds_n1(GuildId(7), 500), -1);
        assert_eq!(c, before, "N1 入口必须可观测 no-op（T-38）");
        assert_eq!(c.total_gold, 2_000);
    }

    /// 配置映射：`[Setup]` 键走 `GuildCastleConfig`，`UpgradeWeaponPrice` 由调用方按 `GameSvrConf` 传入。
    #[test]
    fn castle_tax_config_maps_guild_castle_config_keys() {
        let cfg = GuildCastleConfig {
            castle_tax_rate: 7,
            castle_one_day_gold: 3_000_000,
            castle_gold_max: 6_000_000,
            get_all_npc_tax: true,
            castle_member_price_rate: 60,
            ..GuildCastleConfig::default()
        };

        let t = CastleTaxConfig::from_guild_castle_config(&cfg, 20_000);
        assert_eq!(t.castle_tax_rate, 7);
        assert_eq!(t.castle_one_day_gold, 3_000_000);
        assert_eq!(t.castle_gold_max, 6_000_000);
        assert!(t.get_all_npc_tax);
        assert_eq!(t.upgrade_weapon_price, 20_000);
        assert_eq!(t.castle_member_price_rate, 60);
    }

    fn shop_cfg(price_rate: i32, castle_merchant: bool, castle_member: bool) -> ShopConfig {
        ShopConfig {
            price_rate,
            castle_merchant,
            castle_member,
            castle_member_price_rate: 80,
            super_repair_price_rate: 3,
            ..ShopConfig::default()
        }
    }

    /// 城主行会 `guild_name` 的城（攻城区 = map 3、(644,290)±100）。
    fn castle_with_master(guild_name: &str) -> UserCastle {
        let mut c = empty_castle("沙巴克");
        c.castle_map_id = Some(3);
        c.home_x = 644;
        c.home_y = 290;
        c.war_range_x = 100;
        c.war_range_y = 100;
        c.master_guild = Some(GuildId(1));
        c.own_guild = Some(guild_name.to_string());
        c
    }

    fn intent(target: MerchantTaxTarget, amount: i32) -> Option<MerchantTaxIntent> {
        Some(MerchantTaxIntent { target, amount })
    }

    /// G-T2 `castle_member_price_vectors`：`GetUserPrice` 的成员价取值链——
    /// `n14 = max(60, Round(PriceRate × CastleMemberPriceRate / 100))`，最后
    /// `Round(nPrice / 100.0 × n14)`（AwayFromZero）。
    ///
    /// 牙：`max(60, …)` 被删 ⇒ PriceRate=55 行得 44（红）；`castle_member` 来源改成
    /// “有行会” ⇒ 非城主行会行得 80（红）。
    #[test]
    fn merchant_buy_price_uses_member_rate_with_60_floor() {
        let member = shop_cfg(100, true, true);
        assert_eq!(
            merchant_call_price(&member, 10_000, MerchantCallPoint::Buy { n_price: 100.0 }),
            80
        );
        assert_eq!(
            merchant_call_price(&member, 10_000, MerchantCallPoint::Buy { n_price: 101.0 }),
            81,
            "80.8 必须 AwayFromZero 进位"
        );
        assert_eq!(
            merchant_call_price(&member, 10_000, MerchantCallPoint::Buy { n_price: 1.0 }),
            1,
            "0.8 必须 AwayFromZero 进位"
        );
        // PriceRate=55 ⇒ Round(44) 被 max(60) 抬到 60（成员价反而高于 55%）。
        assert_eq!(
            merchant_call_price(
                &shop_cfg(55, true, true),
                10_000,
                MerchantCallPoint::Buy { n_price: 100.0 }
            ),
            60
        );
        assert_eq!(
            merchant_call_price(
                &shop_cfg(80, true, true),
                10_000,
                MerchantCallPoint::Buy { n_price: 100.0 }
            ),
            64
        );
        // 外层 `if (CastleMerchant)` 不成立 ⇒ 不折。
        assert_eq!(
            merchant_call_price(
                &shop_cfg(100, false, true),
                10_000,
                MerchantCallPoint::Buy { n_price: 100.0 }
            ),
            100
        );
        // `Castle != null && IsMasterGuild` 门不成立 ⇒ 不折。
        assert_eq!(
            merchant_call_price(
                &shop_cfg(100, true, false),
                10_000,
                MerchantCallPoint::Buy { n_price: 100.0 }
            ),
            100
        );

        // CastleMemberPriceRate 覆盖：50 ⇒ max(60,50)=60；90 ⇒ 90。
        let mut rate50 = member;
        rate50.castle_member_price_rate = 50;
        assert_eq!(
            merchant_call_price(&rate50, 10_000, MerchantCallPoint::Buy { n_price: 100.0 }),
            60,
            "max(60, 50) 下限"
        );
        let mut rate90 = member;
        rate90.castle_member_price_rate = 90;
        assert_eq!(
            merchant_call_price(&rate90, 10_000, MerchantCallPoint::Buy { n_price: 100.0 }),
            90
        );
    }

    /// 卖出永不打成员折：`GetSellItemPrice(nPrice) = Round(nPrice / 2.0)`，不经过 `GetUserPrice`。
    ///
    /// 牙：把 Sell 改成先走 `get_user_price` ⇒ 成员态 100 得 40（红）。
    #[test]
    fn merchant_sell_price_never_applies_member_discount() {
        let member = shop_cfg(100, true, true);
        assert_eq!(
            merchant_call_price(&member, 10_000, MerchantCallPoint::Sell { n_price: 100.0 }),
            50
        );
        assert_eq!(
            merchant_call_price(&member, 10_000, MerchantCallPoint::Sell { n_price: 101.0 }),
            51,
            "50.5 AwayFromZero"
        );
        assert_eq!(
            merchant_call_price(
                &shop_cfg(100, false, false),
                10_000,
                MerchantCallPoint::Sell { n_price: 100.0 }
            ),
            50
        );
        // PriceRate 完全不参与卖出取价。
        assert_eq!(
            merchant_call_price(
                &shop_cfg(55, true, true),
                10_000,
                MerchantCallPoint::Sell { n_price: 100.0 }
            ),
            50
        );
        // 对照：同一成员态走买入是 80，卖出必须仍是 50。
        assert_eq!(
            merchant_call_price(&member, 10_000, MerchantCallPoint::Buy { n_price: 100.0 }),
            80
        );
    }

    /// 修理：`GetUserPrice` → 特修 ×`SuperRepairPriceRate` → `Round(price/3.0/DuraMax*(DuraMax-Dura))`。
    ///
    /// 牙：把特修倍率移到耐久比例**之后** ⇒ dura=5 行得 17×3=51（红，正确 50）。
    #[test]
    fn merchant_repair_applies_super_rate_before_durability_ratio() {
        let member = shop_cfg(100, true, true);
        let nonmember = shop_cfg(100, true, false);
        let repair = |super_repair| MerchantCallPoint::Repair {
            n_price: 100.0,
            dura: 5,
            dura_max: 10,
            super_repair,
        };
        assert_eq!(merchant_call_price(&nonmember, 10_000, repair(false)), 17);
        assert_eq!(
            merchant_call_price(&nonmember, 10_000, repair(true)),
            50,
            "300 ⇒ 50；先比例后倍率会得 51"
        );
        assert_eq!(merchant_call_price(&member, 10_000, repair(false)), 13);
        assert_eq!(
            merchant_call_price(&member, 10_000, repair(true)),
            40,
            "240 ⇒ 40；先比例后倍率会得 39"
        );

        // G-T3 修理向量：DuraMax=10, Dura=4 ⇒ 6/10 比例。
        let repair_4 = |super_repair| MerchantCallPoint::Repair {
            n_price: 100.0,
            dura: 4,
            dura_max: 10,
            super_repair,
        };
        assert_eq!(merchant_call_price(&nonmember, 10_000, repair_4(false)), 20);
        assert_eq!(merchant_call_price(&nonmember, 10_000, repair_4(true)), 60);

        // DuraMax == 0 ⇒ 直接用乘完倍率的价（oracle `else nRepairPrice = nPrice`）。
        let repair_zero = |super_repair| MerchantCallPoint::Repair {
            n_price: 100.0,
            dura: 0,
            dura_max: 0,
            super_repair,
        };
        assert_eq!(
            merchant_call_price(&nonmember, 10_000, repair_zero(false)),
            100
        );
        assert_eq!(
            merchant_call_price(&nonmember, 10_000, repair_zero(true)),
            300
        );
    }

    /// 武器升级提交：固定 `Config.UpgradeWeaponPrice`，不走 `GetUserPrice`、不吃成员折。
    ///
    /// 牙：改成 `get_user_price(shop, upgrade_weapon_price)` ⇒ PriceRate=55 行得 55（红）。
    #[test]
    fn merchant_upgrade_weapon_price_is_config_constant() {
        for (rate, castle_merchant, castle_member) in
            [(100, true, true), (55, true, true), (100, false, false)]
        {
            assert_eq!(
                merchant_call_price(
                    &shop_cfg(rate, castle_merchant, castle_member),
                    10_000,
                    MerchantCallPoint::UpgradeWeapon
                ),
                10_000,
                "rate={rate} 也必须原样"
            );
        }
        // 费用随配置走（不是写死 10000）。
        assert_eq!(
            merchant_call_price(
                &shop_cfg(55, true, true),
                20_000,
                MerchantCallPoint::UpgradeWeapon
            ),
            20_000
        );
    }

    /// G-T3/G-T5：四个调用点的税意图——目的地 + 税基（含 B-16 常量税基与 `Castle != null` 优先）。
    ///
    /// 牙：`merchant_tax_intent` 忽略 `castle_present` 优先支 ⇒ `(true,true,true)` 回
    /// `CastleManager/10000`（红）；`CastleManager` 分支改用本次价格 ⇒ 777 行得 777（红）。
    #[test]
    fn merchant_tax_intent_covers_four_call_points_and_b16() {
        // Castle 分支：税基 = 调用点成交价。
        assert_eq!(
            merchant_tax_intent(100, true, false, true, 10_000),
            intent(MerchantTaxTarget::Castle, 100)
        );
        assert_eq!(
            merchant_tax_intent(50, true, false, true, 10_000),
            intent(MerchantTaxTarget::Castle, 50)
        );
        // CastleManager 分支：税基恒 UpgradeWeaponPrice（B-16）。
        assert_eq!(
            merchant_tax_intent(777, true, true, false, 10_000),
            intent(MerchantTaxTarget::CastleManager, 10_000)
        );
        assert_eq!(
            merchant_tax_intent(50, false, true, false, 10_000),
            intent(MerchantTaxTarget::CastleManager, 10_000),
            "卖出 50 也按 10000 计税"
        );
        // `Castle != null` 优先于 `GetAllNpcTax`。
        assert_eq!(
            merchant_tax_intent(777, true, true, true, 10_000),
            intent(MerchantTaxTarget::Castle, 777)
        );
        // 不收税的组合。
        assert_eq!(merchant_tax_intent(777, false, false, false, 10_000), None);
        assert_eq!(
            merchant_tax_intent(777, true, false, false, 10_000),
            None,
            "CastleMerchant=true 但 Castle=null 且 GetAllNpcTax=false"
        );
    }

    /// 商人 ⟷ 城堡纯数据模型：只有**绑定城**的**城主行会**成员才吃成员价。
    ///
    /// 牙：`member_price_applies` 去掉 `is_master_guild_name`（有行会即 true）⇒
    /// 非城主行会行红；去掉 `castle_merchant` 外层门 ⇒ CastleMerchant=false 行红。
    #[test]
    fn merchant_member_price_requires_master_guild_of_bound_castle() {
        let castles = vec![castle_with_master("沙巴克")];
        let binding = MerchantCastleBinding::new(true, Some(0));
        assert!(binding.castle_present());
        assert!(binding.member_price_applies(&castles, Some("沙巴克")));
        assert!(
            !binding.member_price_applies(&castles, Some("其他行会")),
            "非城主行会不打折"
        );
        assert!(
            !binding.member_price_applies(&castles, None),
            "无行会不打折"
        );
        assert!(
            !MerchantCastleBinding::new(false, Some(0))
                .member_price_applies(&castles, Some("沙巴克")),
            "CastleMerchant=false 外层门"
        );
        assert!(
            !MerchantCastleBinding::new(true, None).member_price_applies(&castles, Some("沙巴克")),
            "Castle=null"
        );
        assert!(
            !MerchantCastleBinding::new(true, Some(1))
                .member_price_applies(&castles, Some("沙巴克")),
            "下标越界"
        );

        // 下标指向另一座城 ⇒ 用那座城的城主行会判定。
        let mut second = empty_castle("城B");
        second.master_guild = Some(GuildId(2));
        second.own_guild = Some("城B行会".to_string());
        let castles = vec![castle_with_master("沙巴克"), second];
        assert!(MerchantCastleBinding::new(true, Some(1))
            .member_price_applies(&castles, Some("城B行会")));
        assert!(!MerchantCastleBinding::new(true, Some(1))
            .member_price_applies(&castles, Some("沙巴克")));
    }

    /// G-T3 `castle_tax_shop_vectors`：结算 → 税意图 → `IncRateGold`，逐场景断言金库增量。
    ///
    /// 牙：把税意图丢弃（不落 `inc_rate_gold`）⇒ 全部增量为 0（红）；税基改成调用点价格
    /// 但目的地用 `CastleManager` ⇒ B-16 行增量为 39 不是 500（红）。
    #[test]
    fn merchant_call_tax_lands_in_bound_castle_vault() {
        let cfg = tax();
        let castles = vec![castle_with_master("沙巴克")];
        let member = shop_cfg(100, true, true);
        let nonmember = shop_cfg(100, true, false);
        let binding = MerchantCastleBinding::new(true, Some(0));
        let settle = |shop: &ShopConfig, guild: Option<&str>, call: MerchantCallPoint| {
            MerchantPriceContext::new(shop, &cfg, binding, &castles, guild).settlement(call)
        };

        let mut vault = castles[0].clone();
        let mut apply = |settlement: MerchantCallSettlement| -> i32 {
            let before = vault.total_gold;
            let tax = settlement.tax.expect("有城堡必收税");
            vault.inc_rate_gold(&cfg, tax.amount, 0);
            vault.total_gold - before
        };

        // 买入（非成员，100）⇒ +5；买入（成员，80）⇒ +4（4.0）。
        let s = settle(
            &nonmember,
            Some("攻方行会"),
            MerchantCallPoint::Buy { n_price: 100.0 },
        );
        assert_eq!(s.price, 100);
        assert_eq!(s.tax, intent(MerchantTaxTarget::Castle, 100));
        assert_eq!(apply(s), 5);

        let s = settle(
            &member,
            Some("沙巴克"),
            MerchantCallPoint::Buy { n_price: 100.0 },
        );
        assert_eq!(s.price, 80);
        assert_eq!(s.tax, intent(MerchantTaxTarget::Castle, 80));
        assert_eq!(apply(s), 4);

        // 卖出 50 ⇒ +3（2.5 AwayFromZero）。
        let s = settle(
            &nonmember,
            Some("攻方行会"),
            MerchantCallPoint::Sell { n_price: 100.0 },
        );
        assert_eq!(s.price, 50);
        assert_eq!(apply(s), 3);

        // 普通修理 20 ⇒ +1；特修 60 ⇒ +3。
        let s = settle(
            &nonmember,
            Some("攻方行会"),
            MerchantCallPoint::Repair {
                n_price: 100.0,
                dura: 4,
                dura_max: 10,
                super_repair: false,
            },
        );
        assert_eq!(s.price, 20);
        assert_eq!(apply(s), 1);
        let s = settle(
            &nonmember,
            Some("攻方行会"),
            MerchantCallPoint::Repair {
                n_price: 100.0,
                dura: 4,
                dura_max: 10,
                super_repair: true,
            },
        );
        assert_eq!(s.price, 60);
        assert_eq!(apply(s), 3);

        // 武器升级 10000 ⇒ +500。
        let s = settle(
            &nonmember,
            Some("攻方行会"),
            MerchantCallPoint::UpgradeWeapon,
        );
        assert_eq!(s.price, 10_000);
        assert_eq!(s.tax, intent(MerchantTaxTarget::Castle, 10_000));
        assert_eq!(apply(s), 500);

        // B-16：Castle=null + GetAllNpcTax ⇒ 各城按 UpgradeWeaponPrice 计税（777 → +500，不是 +39）。
        let cfg_all = CastleTaxConfig {
            get_all_npc_tax: true,
            ..cfg.clone()
        };
        let s = MerchantPriceContext::new(
            &nonmember,
            &cfg_all,
            MerchantCastleBinding::new(true, None),
            &[],
            None,
        )
        .settlement(MerchantCallPoint::Buy { n_price: 777.0 });
        assert_eq!(s.price, 777);
        assert_eq!(
            s.tax,
            intent(MerchantTaxTarget::CastleManager, 10_000),
            "管理器分支税基恒 UpgradeWeaponPrice（B-16）"
        );
        let mut mgr = CastleManager::new();
        let mut a = empty_castle("城A");
        let mut b = empty_castle("城B");
        a.total_gold = 0;
        b.total_gold = 0;
        mgr.castle_list.push(a);
        mgr.castle_list.push(b);
        let outs = mgr.inc_rate_gold(&cfg_all, s.tax.expect("管理器收税").amount, 0);
        assert_eq!(
            outs.iter().map(|o| o.n_in_gold).collect::<Vec<_>>(),
            vec![500, 500]
        );
    }

    /// 上下文把绑定解析成 `GetUserPrice` 的两扇门（`castle_merchant` / `castle_member`）。
    ///
    /// 牙：`effective_shop_config` 不写 `castle_member`（保留基础配置的 false）⇒ 城主行会
    /// 买入价得 100 而不是 80（红）。
    #[test]
    fn merchant_price_context_resolves_binding_into_shop_config() {
        let cfg = tax();
        let castles = vec![castle_with_master("沙巴克")];
        let base = shop_cfg(100, false, false); // 基础配置两扇门都关
        let binding = MerchantCastleBinding::new(true, Some(0));

        let ctx = MerchantPriceContext::new(&base, &cfg, binding, &castles, Some("沙巴克"));
        let effective = ctx.effective_shop_config();
        assert!(effective.castle_merchant);
        assert!(effective.castle_member);
        assert_eq!(ctx.price(MerchantCallPoint::Buy { n_price: 100.0 }), 80);

        let outsider = MerchantPriceContext::new(&base, &cfg, binding, &castles, Some("攻方行会"));
        assert!(outsider.effective_shop_config().castle_merchant);
        assert!(!outsider.effective_shop_config().castle_member);
        assert_eq!(
            outsider.price(MerchantCallPoint::Buy { n_price: 100.0 }),
            100
        );

        // CastleMerchant=false ⇒ 即使玩家是城主行会，外层门也关。
        let not_castle_merchant = MerchantPriceContext::new(
            &base,
            &cfg,
            MerchantCastleBinding::new(false, Some(0)),
            &castles,
            Some("沙巴克"),
        );
        assert!(!not_castle_merchant.effective_shop_config().castle_merchant);
        assert!(!not_castle_merchant.effective_shop_config().castle_member);
        assert_eq!(
            not_castle_merchant.price(MerchantCallPoint::Buy { n_price: 100.0 }),
            100
        );
    }
}
