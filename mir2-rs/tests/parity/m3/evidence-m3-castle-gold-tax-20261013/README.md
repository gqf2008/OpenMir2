# 证据包：M3⑧-实现 9 税收 / 城堡金库（卡 `openmir2-m3-castle-gold-tax`）

- **branch**：`worktree-m3-castle-gold-tax`
- **base**：`refs/heads/master` = `origin/master db400e869`
- **写集**：`mir2-rs/crates/world/**`（新 `src/castle_tax.rs` + `src/castle.rs` 加 `save_gold_tick` 字段
  + `src/lib.rs` 模块/re-export 两行）+ `mir2-rs/tests/parity/**`（本证据包 + `whitelist.md` 的 **B-16** 行）
- **零起栈 / 零端口**：全离线（构造级 unit 用例，时间门用注入的 `tick_ms`）
- **oracle 锚**：`00-oracle-anchor-lines.txt`（`awk NR` 实测本仓库 `src/`，只读共享主检出）

> 本片 = ⑧-9 的**第①片（离线 `crates/world` 税收面）**。`bins/game-svr` 的收包派发/商人接线
> 与真机两侧同序列 `run_guild_castle_pair.ps1` diff=0 属第②片，等 owner 对 `bins/game-svr`
> 串行序的裁决（comment `018d7be2`）+ 独占窗口（`openmir2-m3-clock-injection` 的 C# 时钟注入）。

## 1. 交付内容

| 文件 | 说明 |
| --- | --- |
| `crates/world/src/castle_tax.rs`（新） | `CastleTaxConfig`（税收配置）+ `UserCastle::inc_rate_gold`（**逐行照搬** `UserCastle.cs:768-812`，含 `Round(AwayFromZero)` / `CastleOneDayGold` 日上限 / `CastleGoldMax` 封顶 / 10min `SaveTick`）+ `CastleManager`（`castle_list` + 升序 `inc_rate_gold` + `get_castle`）+ `MerchantTaxTarget`/`merchant_tax_target`/`merchant_tax_amount`（`Merchant.cs` 四调用点）+ `IncRateGoldOutcome` + 11 条新用例 |
| `crates/world/src/castle.rs` | `UserCastle` 加 `save_gold_tick: i64`（`SaveTick`，`UserCastle.cs:73`）+ ctor 初始化两行（**不动** ⑧-7 的攻城状态机本体） |
| `crates/world/src/lib.rs` | `pub mod castle_tax;` + `pub use castle_tax::{…}`（只加本行） |
| `tests/parity/whitelist.md` | 新增 **B-16** 行（`CastleMgr` 分支税基恒为 `UpgradeWeaponPrice` 的 oracle 怪行为复刻） |

**模块边界**：⑧-9 的税收面独立成 `castle_tax.rs`（而非继续膨胀 `castle.rs`），是为了让**未合**的
⑧-8 片（`worktree-m3-castle-door-repair`，同样改 `castle.rs`）与 ⑧-9 的 `git merge` 冲突面最小
（⑧-8 只动 `CastleUnit` 与 `impl UserCastle` 的门/墙方法，本片只动 `UserCastle` 的字段列表 + 新文件）。

## 2. oracle 锚（逐行，见 `00-oracle-anchor-lines.txt`）

| 本模块 | oracle | 事实 |
| --- | --- | --- |
| `UserCastle::inc_rate_gold` | `UserCastle.cs:768-812` | `nInGold = Round(nGold * CastleTaxRate/100.0)`；`TodayIncome+nInGold <= CastleOneDayGold` ⇒ 累加，否则「已到顶取 0 / 未到顶取余量并置顶」；`nInGold>0` 时 `TotalGold+nInGold < CastleGoldMax`（**严格小于**）⇒ 累加，否则 `= CastleGoldMax`；`(GetTickCount()-SaveTick) > 10min` ⇒ `SaveTick = now`（与金额无关） |
| `CastleTaxConfig` 字段 | `GameSvrConf.cs:960/964/965/968/1420/1788-1796` | `CastleTaxRate=5` / `CastleGoldMax=10000000` / `CastleOneDayGold=2000000` / `CastleMemberPriceRate=80` / `UpgradeWeaponPrice=10000` / `GetAllNpcTax=false`；现网 `Server.conf` 覆盖 `CastleGoldMax=5000000` |
| `CastleManager::inc_rate_gold` | `CastleManager.cs:209-215` | 升序 `for i in CastleList` 调 `IncRateGold` |
| `CastleManager::get_castle` | `CastleManager.cs:184-192` | `0 <= nIndex < Count` 才返回，否则 `null` |
| `merchant_tax_target` | `Merchant.cs:544-553/1250-1259/1385-1393/1637-1645` | `if (CastleMerchant \|\| GetAllNpcTax) { if (Castle != null) → Castle; else if (GetAllNpcTax) → CastleMgr; }` |
| `merchant_tax_amount` | 同上 | `Castle` 分支 = 调用点价格；`CastleMgr` 分支 = `Config.UpgradeWeaponPrice`（**B-16**） |
| `hutil::round` | `HUtil32.cs:131-134` | `(int)Math.Round(r, MidpointRounding.AwayFromZero)`（`crates/shared/src/hutil.rs:6` 1:1） |
| 金库提取/上缴 | `UserCastle.cs:599-602` / `:911-914`（N1，**可达**）vs `:814-889`（真身，**不可达**） | `CastleOfficial.cs:155-183` 的 `Castle` 静态类型 = `IUserCastle`（`BaseObject.cs:153`）⇒ 重载决议只认接口成员（N1 抛异常）⇒ 真身重载永远选不中 |
| 成员价 80% | `Merchant.cs:765-782` | `max(60, Round(PriceRate*CastleMemberPriceRate/100))`；已在 `crates/formula/src/shop.rs::get_user_price` 落地（⑤ 家族），本片只登记 |

## 3. 构造级判据（11/11，`01-rust-vectors.txt`，命令可复现）

```
(cd mir2-rs && CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target \
  cargo test -p mir2-world --lib castle_tax --offline)
```

| # | 用例 | 钉住的 oracle 行为 |
| --- | --- | --- |
| 1 | `inc_rate_gold_rounds_away_from_zero_like_hutil` | `10*5%=0.5 ⇒ 1`、`110*5%=5.5 ⇒ 6`、`90*5%=4.5 ⇒ 5`（**不是**截断） |
| 2 | `inc_rate_gold_caps_today_income_and_rolls_remainder` | 日上限超额 ⇒ `nInGold` 取余量、`TodayIncome` 置顶；已到顶 ⇒ `nInGold=0`、金额不动 |
| 3 | `inc_rate_gold_caps_total_gold_at_castle_gold_max` | `TotalGold` 封顶（含「恰好加满」「已超上限」两种边界） |
| 4 | `inc_rate_gold_zero_price_keeps_totals_but_still_advances_save_tick` | `nGold=0` 金额不变，但 `SaveTick` 门**照样**刷新 |
| 5 | `inc_rate_gold_save_tick_is_strictly_greater_than_ten_minutes` | `> 10min`（等于不刷）；刷新后从新 `SaveTick` 重新计时 |
| 6 | `castle_manager_inc_rate_gold_visits_every_castle_in_order` | 升序遍历 + 每座城各自记账 + `GetCastle` 越界 |
| 7 | `merchant_tax_target_matches_oracle_branches` | 两个开关 × 有无城堡的 7 种组合 |
| 8 | `merchant_tax_amount_uses_upgrade_weapon_price_for_manager_branch` | `Castle` 用价格；`CastleMgr` 恒用 `UpgradeWeaponPrice`（B-16） |
| 9 | `cross_day_run_resets_today_income_but_keeps_total_gold` | 与 ⑧-7 `Run` 的跨天重置接缝：`TodayIncome` 归零、`TotalGold` 保留 |
| 10 | `vault_withdrawal_and_receipt_stay_n1_noop_while_tax_changes_state` | 提取/上缴 = N1 可观测 no-op（T-38）；对照 `IncRateGold` 真实改状态 |
| 11 | `castle_tax_config_maps_guild_castle_config_keys` | `[Setup]` 键走 `GuildCastleConfig`，`UpgradeWeaponPrice` 由调用方传入 |

## 4. 改坏必红（`02` / `03`）

| 变异 | 改法 | 预期 | 实测 |
| --- | --- | --- | --- |
| M1 | `hutil::round(...)` ⇒ `(... ) as i32`（截断） | `0.5` 不再进位 ⇒ 向量 1 必红 | **rc=101，2 failed**（`rounds_away_from_zero` / `caps_total_gold`）→ `02-mutation-1-round-truncated-RED.txt` |
| M2 | 删掉 `TotalGold` 的 `CastleGoldMax` 封顶分支 | 上限向量必红 | **rc=101，1 failed**（`caps_total_gold_at_castle_gold_max`）→ `03-mutation-2-gold-max-uncapped-RED.txt` |

两次变异后 `git checkout`/还原，源文件 sha256 不变：
`mir2-rs/crates/world/src/castle_tax.rs = 3f8280027f3bf8abcf3155b7b0caf9be82819c1ca873b773d1460796199744e6`。

## 5. 照搬的 oracle 怪行为 / 已裁决断点（**不是**差异）

1. **B-16：`CastleMgr` 分支税基恒为 `UpgradeWeaponPrice`**（`Merchant.cs:1250-1259/1385-1393/1637-1645`）——
   不是本次交易的 `nPrice`/`nRepairPrice`。`merchant_tax_amount` 照搬；向量 8 锁死。
2. **N1（T-38）：金库提取/上缴不可达**——见 §2 与模块头。⑧-7 已把两个入口建模为可观测 no-op；
   本片**复验**（向量 10）并**不**实现「本该有」的提取/上缴。
3. `AddDateTimeOfDay` 忽略天数、`StopWallconquestWar` 玩家处理不可达、`InPalaceGuildCount` 语义错位
   属 ⑧-7 已登记项（本片不重复）。

## 6. 四门禁（base = `db400e869`）

| 门 | 命令 | 读数 | 原始件 |
| --- | --- | --- | --- |
| ① fmt | `cargo fmt --all -- --check` | rc=0 | `04-gate-fmt.txt` |
| ② clippy | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 | `05-gate-clippy.txt` |
| ③ test | `cargo test --workspace --offline --no-fail-fast` | **98 suites / 1054 passed / 0 failed / 21 ignored** | `06-gate-test.txt` |
| ④ msgcodegen | `cargo run -p msgcodegen --offline -- --check` | **604** 常量 OK | `07-gate-msgcodegen.txt` |

（全部 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`，隔离主检出。）

## 7. 边界（`08-boundary.txt`）

相对 `db400e869` 的改动只落在 `crates/world/src/{castle,castle_tax,lib}.rs` + `tests/parity/m3/**`
+ `tests/parity/whitelist.md`；冻结面 `src/ sql/ MirClient crates/protocol crates/storage bins/` **空**
（oracle 只读，未编译主检出，未起栈/占端口）。

## 8. 未做 / 下一步（第②片）

1. **`bins/game-svr` 接线**（等 owner 对 bins 串行序裁决）：`UserCastle::run` 里 `IncRateGold` 的
   调用点实际来自商人（⑤-4 的 `Merchant.cs` 四调用点），需要 ⑤-4 的商店派发落地后接
   `merchant_tax_target`/`merchant_tax_amount` + `CastleManager`；`CastleMgr` 的装载（`CastleManager.LoadCastleList`）
   也归该片。
2. **真机两侧时间门对拍**：`run_guild_castle_pair.ps1` 的 `IncRateGold`/`SaveTick` 站点，依赖
   `openmir2-m3-clock-injection`（⑧-1）的 C# `GetTickCount` 注入 + 独占窗口。
3. ⑧-9 的 Rust 侧符号（`CastleTaxConfig`/`CastleManager`/`merchant_tax_*`）**不**被现有
   `bins` 复用（避免与未合的 ⑤-4/⑧ 片争写集）。

## 9. 残留物清单

- 编译缓存 `E:/tmp/openmir2-m3cs-target`（保留供复现）。
- 变异临时件 `/tmp/m3cs-castle-tax-orig.rs`（还原用；还原后源 sha 一致）。
- 分支 `worktree-m3-castle-gold-tax`（本地 + origin + walgit）保留，等非作者 review。
