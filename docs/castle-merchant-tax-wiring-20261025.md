# 商税四调用点接线（`castle_tax::merchant_tax_intent` 消费，tier R）

- 卡：`openmir2-castle-merchant-tax-wiring`（tier **R**，owner `openmir2-dev-2`，branch `card/castle-merchant-tax-wiring`）。
- 基线：本检出 `master` `a6fc02d0a`（含 P-1..P-7 检查器合并）。
- 写集：`mir2-rs/bins/game-svr/src/shop.rs`、`mir2-rs/bins/game-svr/src/lib.rs`、合同测试、本文档。
- **未改**：`crates/world/src/castle_tax.rs` 纯计算层、任何 C# `src/**`、场景 JSON、协议。
- 结论：`merchant_tax_intent` 原先只有纯计算、四个交易点 `let _tax = …` 丢弃；现改为
  **交易点返回税意图、宿主入账**（`Castle.IncRateGold` / `CastleMgr.IncRateGold`），
  条件/目的地/税基逐行照 oracle。

## 1. oracle 证据链（谁付、付给谁、失败时行为）

四处 `if (CastleMerchant || SystemShare.Config.GetAllNpcTax)`：

| # | 调用点 | oracle 行 | 谁付 | 付给谁 / 税基 | 失败/无条件时 |
|---|---|---|---|---|---|
| 1 | 买卖前先扣款的 **武器升级** | `src/GameSrv/Npc/Merchant.cs:546-554` | 玩家（`DecGold(UpgradeWeaponPrice)` 已成功） | `Castle != null` ⇒ `Castle.IncRateGold(UpgradeWeaponPrice)`；否则 `GetAllNpcTax` ⇒ `CastleMgr.IncRateGold(UpgradeWeaponPrice)` | 两条件皆假 / `CastleMerchant` 但无城堡且不全局收税 ⇒ **无税**；`DecGold` 失败 ⇒ 整个升级分支不进 |
| 2 | 买入 | `Merchant.cs:1257-1266` | 玩家（`Gold -= nPrice` 且 `AddItemToBag` 成功） | Castle ⇒ `IncRateGold(nPrice)`；否则 `GetAllNpcTax` ⇒ `CastleMgr.IncRateGold(`**`UpgradeWeaponPrice`**`)` | 钱不够/背包满/负重不足 ⇒ 无税（税在成功分支内） |
| 3 | 卖出 | `Merchant.cs:1392-1400` | 玩家收钱（`IncGold(nPrice)` 成功） | Castle ⇒ `IncRateGold(nPrice)`；否则 `GetAllNpcTax` ⇒ `CastleMgr.IncRateGold(UpgradeWeaponPrice)` | `nPrice<=0` / StdMode 25/30 耐久不足 / IncGold 超上限 ⇒ 无税 |
| 4 | 修理 | `Merchant.cs:1644-1652` | 玩家（`DecGold(nRepairPrice)` 成功） | Castle ⇒ `IncRateGold(nRepairPrice)`；否则 `GetAllNpcTax` ⇒ `CastleMgr.IncRateGold(UpgradeWeaponPrice)` | 不支持修理 / 价格<=0 / 金币不足 ⇒ 无税 |

- 入账本体：`UserCastle.IncRateGold`（`src/M2Server/Castle/UserCastle.cs:768-812`）——
  `Round(nGold * CastleTaxRate/100.0)`（AwayFromZero）→ `TodayIncome` 日上限裁剪 →
  `TotalGold` 封顶（严格小于）→ `SaveTick` 10 分钟门（严格大于，`nGold=0` 也刷新）。
  已由 `crates/world/src/castle_tax.rs:132` 逐行实现，本卡**不改**。
- `CastleMgr.IncRateGold`（`CastleManager.cs:209-215`）：升序遍历 `CastleList` 对每座城调用。
- **`CastleMgr` 分支税基恒 `UpgradeWeaponPrice`**（B-16）：买/卖/修的 `else if (GetAllNpcTax)`
  传的不是本次 `nPrice`/`nRepairPrice`，而是 `Config.UpgradeWeaponPrice`。本卡用合同测试钉住
  （`buy_tax_manager_branch_uses_upgrade_price_not_trade_price`）。

## 2. 实现（bins 接线）

### 2.1 纯函数层 → 交易点返回税意图

- `MerchantView` 增 `castle_index: Option<usize>`（`None` ⇔ oracle `Castle == null`）。
- `ShopContext` 增 `upgrade_weapon_price: i32`（CastleManager 分支税基常量）。
- `SellResult` / `BuyResult` / `RepairResult` 各增 `tax: Option<MerchantTaxIntent>`：
  - 卖：`merchant_tax_intent(n_price, castle_merchant, get_all_npc_tax, castle_index.is_some(), upgrade_weapon_price)`；
  - 买：同上，`price = n_price`；
  - 修：`price = gold - out.gold`（本次**实际扣掉**的 `nRepairPrice`，与 oracle `DecGold` 同值）；
  - 失败/无条件分支 `tax: None`。
- `pub fn apply_merchant_tax(castles, castle_index, intent, cfg, tick_ms) -> Vec<IncRateGoldOutcome>`：
  - `Castle` ⇒ 只入 `castle_index` 那座城；越界 ⇒ 空表（不猜、不 panic）；
  - `CastleManager` ⇒ **升序**遍历 `&mut castles` 全表；
  - `intent == None` ⇒ 空表。

### 2.2 宿主（`lib.rs`）

- `ShopCfg` 增 `castle_tax_rate` / `castle_one_day_gold` / `castle_gold_max`（production 从
  `GuildCastleConfig` 取；默认 5 / 2_000_000 / 10_000_000）。
- `shop_castle_tax_config(&ShopCfg) -> CastleTaxConfig`（`UpgradeWeaponPrice` 取 `shop_cfg.upgrade.price`）。
- `apply_merchant_tax_to_world(world, castle_index, intent, shop_cfg)`：`tick_ms = world.clock_ms`
  （虚拟时钟；与 `castle.rs` 的 10min 门同基）。
- **GetCastle 前置**：`mcastle_index = world.merchant_castle_index(id).filter(|i| world.castles.get(*i).is_some())`；
  越界视同无城堡（oracle `Castle` 是对象引用、不会悬空，此支只在内部不一致时出现，不猜目的地）。
- **MasterGuild 不是商税前置**：oracle 税分支只判 `CastleMerchant/GetAllNpcTax/Castle!=null`；
  `MasterGuild` 只用于成员价（`Merchant.cs:765-782`）。合同测试钉住 `master_guild=None` 仍入账。
- 四个调用点：
  - 升级：`Accepted` 分支最前（`DecGold` 之后、摘武器/`RecalcAbilitys` 之前）调 `apply_upgrade_weapon_tax`；
  - 买/卖/修：交易纯函数返回后立即 `apply_merchant_tax_to_world(world, mcastle_index, r.tax, shop_cfg)`。

## 3. 合同测试（改前 `let _tax` 丢弃 vs 改后逐位）

`cargo test -p game-svr --lib tax` ⇒ **6 passed**：

| 测试 | 路径 | oracle 算术 | 断言 |
|---|---|---|---|
| `sell_tax_intent_and_credit_castle_matches_oracle` | 卖 | `GetSellItemPrice(100)=50` ⇒ `Round(50*5%)=Round(2.5)=3` | intent `{Castle, 50}`；入账 `n_in_gold=3`，`TotalGold=3`，`TodayIncome=3`；`master_guild=None` 仍入账 |
| `buy_tax_intent_and_credit_castle_matches_oracle` | 买 | 价 100 ⇒ `Round(5.0)=5` | intent `{Castle, 100}`；入账 5 |
| `buy_tax_manager_branch_uses_upgrade_price_not_trade_price` | 买 + `GetAllNpcTax` | 税基 **10000**（不是 100）⇒ `Round(500)=500` | intent `{CastleManager, 10000}`；两座城各 +500（升序） |
| `repair_tax_intent_and_credit_castle_matches_oracle` | 修 | 修价 `Round((100/3)/10*6)=20` ⇒ `Round(1.0)=1` | intent `{Castle, 20}`；入账 1 |
| `tax_precondition_without_castle_and_without_global_tax_is_none` | 前置 | `CastleMerchant=1` 但无城堡且不全局 ⇒ 无税；越界下标 ⇒ 无副作用 | `tax=None`；`apply(Some(9))` 空表 |
| `upgrade_weapon_tax_credits_castle_or_manager_with_upgrade_price` | 升级 | Castle ⇒ 500；Manager ⇒ 两城各 500；两条件皆假/无城堡 ⇒ 不动 | 见测试 |

> 数字口径：`MerchantTaxIntent.amount` 是**税基**（10 万/50/20/10000），
> 入账金额是 `UserCastle.IncRateGold` 的 `n_in_gold`（5%/AwayFromZero）。

## 4. 牙（≥2，各还原）

| 牙 | 变异 | 结果 |
|---|---|---|
| T1 税额算错 | `apply_merchant_tax` 的 Castle 分支 `intent.amount` → `intent.amount * 2` | **RED**：卖/买/修/升级 4 条合同测试失败（`2 passed; 4 failed`），还原后 6/6 绿 |
| T2 前置缺失 | 卖分支 `m.castle_index.is_some()` 强改为 `true` | **RED**：`tax_precondition_without_castle_and_without_global_tax_is_none` 失败（`0 passed; 1 failed`），还原后绿 |
| T3（附） | 若把 `MasterGuild` 加成税前置 | 现有测试 ⑤ 会红（`master_guild=None` 仍须入账），作为反向约束保留 |

两次变异均用 `cp` 备份 + 还原，最终 `grep` 确认无残留。

## 5. 门禁

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | **rc=0，3180 passed / 0 failed**（基线 3174 + 本卡 6） |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条） |
| `conflict_marker` / `stale_pointer` / `dynamic_count_stamp` / `t_reservation` / `whitelist_anchor` / `m3_registry` / `board_coverage` | rc=0 |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree master HEAD` | 提交后复跑，见 patch body |
| `src/**` / `crates/world/src/castle_tax.rs` | 零变更 |

## 6. R 级窗口项（`window_required: true`，合并后保持 in-progress）

依赖：城堡已初始化、测试库有城堡商人；`CastleGetAllNpcTax` 分支需要**窗口专用**配置副本
（内容冻结，不改 `E:/MirServer`）。现有 18 场景不含“城堡商人税”，窗口需要手工点城堡商人的
买/卖/修/升级，或另立窗口场景。

1. **四路径真机双栈**：同一测试库/城堡状态，对城堡商人各做一次买、卖、修、升级；
   比较玩家金币帧 + `Castle/0/SabukW.txt` 的 `TotalGold` / `TodayIncome` / `SaveTick`，
   要求两侧 diff=0（`SaveTick` 需 ⑧-1 的 C# 时钟注入片，同 tick 窗口）。
2. **CastleManager 分支**（`CastleGetAllNpcTax=1` 窗口副本）：税基恒 `UpgradeWeaponPrice`（B-16），
   两座以上城堡时升序各入账；两侧 `TotalGold` diff=0。
3. **前置**：`CastleMerchant=1` 但商人不在攻城区（`castle_index=None`）⇒ 零税；
   无行会/非城主（`MasterGuild=None`）**仍收税**（税不查 MasterGuild）。
4. **日上限/金库上限/SaveTick 10min 门**：同 tick 注入下 `TodayIncome` 裁剪与 `SaveTick` 刷新逐位一致。

## 7. 残留与限制

- 现有 18 场景无“城堡商人”路径 ⇒ 窗口需手工操作或新增场景（本卡不改场景 JSON）。
- `world.clock_ms` 是虚拟时钟；与 oracle `HUtil32.GetTickCount()` 的映射沿用 `castle.rs` 既有口径，
  真机 `SaveTick` 对拍依赖 ⑧-1 时钟注入。
- `apply_merchant_tax` 返回的 `IncRateGoldOutcome` 生产路径不消费（oracle 返回 `void`）；仅测试/诊断用。
- 临时物：`E:/tmp/shop-tax.bak`（牙备份，已还原；可删）、`E:/tmp/dev2-tax-gates.log`、`/tmp/tooth1*.log`、`/tmp/tooth2.log`。
- 未自批：审查归独立 principal；本卡不自行 merge / 不自签 review。
