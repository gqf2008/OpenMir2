# 商税 apply_merchant_tax 接口加固（C-1）— 2026-10-25

- 卡：`openmir2-tax-apply-interface-hardening`（tier **L**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；分支 `card/tax-apply-interface-hardening`）。
- 基线：本地 `master` `007ad730a`。
- 写集：`mir2-rs/bins/game-svr/src/shop.rs`、`mir2-rs/bins/game-svr/src/lib.rs`、测试、本文件。
- 红线：**未改 `crates/world/src/castle_tax.rs` 纯计算层**；行为逐位不变；未起栈、未占端口、未连库。

## 0. 一句话

把「税去哪」（目的地：城堡金库 / `CastleManager` 全表）从 `merchant_tax_intent` 的税额计算中
分离为**显式类型** `MerchantTaxDestination`（`Castle(i)` 自带下标 / `CastleManager` 无下标），
`apply_merchant_tax` 只接收 `Option<MerchantTaxPosting>`；`merchant_tax_intent` 的返回与税额
**一字未动**，四个调用点经唯一转换点 `posting_for` 落账。四路径税额/入账逐位不变。

## 1. C-1 原文（reviewer-21 `e523aacee`，info）

> `apply_merchant_tax(castles, castle_index, intent, …)` 把「去哪座城」分成两处输入：
> `intent.target` 只说明 Castle/CastleManager，具体下标由调用方另传 `castle_index`。当前四个
> 调用点恒传同一个 `mcastle_index`、且 Castle 目标成立时 `Some(i)` 必然在界内，所以无实际错账；
> 但接口允许「intent=Castle 而 castle_index 指向另一座城」这类误用，未来新增调用点容易踩。
> 建议：把 `castle_index` 并入 `MerchantTaxIntent`（或加 `debug_assert!`）。

本卡采用**类型化分离**（比 `debug_assert!` 更强：误用在编译期不可表达）。

## 2. 接口改动（全部在 bins 层；`castle_tax.rs` 零改动）

```rust
// 新增（shop.rs）
pub enum MerchantTaxDestination {
    Castle(Option<usize>),   // 自带下标；None = 无城可入（无副作用）
    CastleManager,           // 升序入全部城，与任何下标无关
}
pub struct MerchantTaxPosting { pub destination: MerchantTaxDestination, pub amount: i32 }

/// 纯计算层 → 显式入账参数的**唯一转换点**
pub fn posting_for(
    intent: Option<&MerchantTaxIntent>,
    castle_index: Option<usize>,
) -> Option<MerchantTaxPosting>;

// 签名变化：不再同时收 intent 与 castle_index
pub fn apply_merchant_tax(
    castles: &mut [UserCastle],
    posting: Option<MerchantTaxPosting>,   // ← 取代 (castle_index, intent)
    cfg: &CastleTaxConfig,
    tick_ms: i64,
) -> Vec<IncRateGoldOutcome>;
```

- **映射不变**：`MerchantTaxTarget::Castle` ⇒ `Castle(castle_index)`；
  `CastleManager` ⇒ `CastleManager`（丢弃下标，oracle 不看具体城）；`intent=None` ⇒ `None`。
- **金额不变**：仍取 `intent.amount`（纯计算层已按分支算好）。
- **调用点**：`lib.rs::apply_merchant_tax_to_world` 改为
  `posting_for(intent.as_ref(), castle_index)` → `apply_merchant_tax(..., posting, ...)`；
  四个收税路径（升级/买/卖/修理）的入参路径不变。
- **误用不可表达**：`CastleManager` 没有下标字段；`Castle` 的下标与目标在同一值里，
  不存在「intent 说 Castle、另传下标指向别城」的形态。

## 3. 行为逐位不变（证据）

- 既有 6 条合同测试**断言一字未改**，仅把调用从 `(castle_index, intent)` 换成
  `posting_for(...)`（断言仍逐字段比对 `r.tax` 与城堡 `total_gold/today_income`）：
  `sell_tax_intent_and_credit_castle_matches_oracle`、
  `buy_tax_intent_and_credit_castle_matches_oracle`、
  `buy_tax_manager_branch_uses_upgrade_price_not_trade_price`、
  `repair_tax_intent_and_credit_castle_matches_oracle`、
  `tax_precondition_without_castle_and_without_global_tax_is_none`、
  以及 `attack_weapon_wear_tests::upgrade_weapon_tax_credits_castle_or_manager_with_upgrade_price`。
- 抽测：`cargo test -p game-svr --lib tax` ⇒ 7 passed（6 条合同 + 牙 2），
  `--lib posting_for` ⇒ 1 passed。
- 数值抽样：Castle 100 税基 ⇒ 单城 `total_gold=5`（5%）；CastleManager ⇒ 两城各 +5
  （升序）；越界/`None` ⇒ 无副作用（与旧实现同）。

## 4. 牙（≥2，实测「改坏红 + 还原绿」）

| 牙 | 测试 | 改坏方式 | 结果 |
|---|---|---|---|
| 1 目的地映射 | `posting_for_maps_intent_target_to_explicit_destination` | `posting_for` 两个 `match` 分支对调（Castle↔CastleManager） | FAILED / 还原 ok |
| 2 目的地路由 | `apply_merchant_tax_routes_only_by_explicit_destination` | `Castle` 分支改成遍历全部城 | FAILED / 还原 ok |

- 牙 1 钉：Castle 吞下标、CastleManager 丢下标、`None` intent ⇒ `None`。
- 牙 2 钉：`Castle(Some(1))` 只动 1 号城（0 号不得被动）、`CastleManager` 两城各 +5、
  `Castle(Some(9))`/`Castle(None)` 无副作用（不猜 0 号城）。**目的地参数错 ⇒ 错账 ⇒ 红**。

## 5. 门禁

fmt / clippy / `cargo test --workspace` / msgcodegen 全 rc=0；7 项卫生门禁全 rc=0；
`git diff --check` rc=0；`git merge-tree` 无冲突（见 patch body）。
`crates/world/src/castle_tax.rs` **零 diff**（`git diff --stat master..HEAD` 不含该文件）。

## 6. 判不准 / 残余

- 未给 `MerchantTaxPosting` 的字段加私有化（保持 bins 内部直接构造；若未来跨 crate 暴露再收紧）。
- `intent.target` 仍是税额计算的分支依据（纯计算层不可动），但**不再参与入账路由**；
  路由只看显式 `destination`。
- 残留：`%TEMP%` 变异备份已删；未起栈、未占端口、未连库。
