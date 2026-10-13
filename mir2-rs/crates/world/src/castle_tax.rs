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
//! | 成员价 80% | `Merchant.cs:765-782`（`GetUserPrice`）——已在 `crates/formula/src/shop.rs::get_user_price` 落地（⑤ 家族），本模块只登记不进写集 |
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
    /// 本模块只保存供登记，实际定价在 `crates/formula/src/shop.rs::get_user_price`。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::castle::{CastleDate, CastleNow, CastleSiegeConfig};

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
        use crate::castle::GuildId;
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
}
