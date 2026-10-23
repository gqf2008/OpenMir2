# `weapon_strong_of` 补 oracle `Dura == 0` 整槽跳过（`openmir2-weaponstrong-dura-zero-skip`）

- 卡：`openmir2-weaponstrong-dura-zero-skip`（tier=R，`window_required=true`）；
  分支 `card/weaponstrong-dura-zero`；owner `openmir2-worker-63`（worktree `wt-worker-63`）。
- 基线：`walgit/master = c0f8d65a8`；实现提交 `e2d7ec568`（文档提交为其后一条）。
- 性质：关掉 `AddAbil.WeaponStrong` 的**潜在分叉**；当前部署不可达，行为对现网零变化。

## 1. oracle 的 `Dura == 0` 判定在哪一层（以代码为准）

卡面把它写在 `BaseObject.RecalcAbilitys`，**实际不是**：

| 层 | 位置 | 事实 |
|---|---|---|
| `BaseObject.RecalcAbilitys()` | `src/M2Server/Actor/BaseObject.Abil.cs:15-89` | 只做 `AddAbil = new()`、`WAbil = Abil.Clone()`、AntiPoison/AntiMagic 重置与尾部合成；**没有装备循环，也没有 `Dura` 判定**。 |
| `PlayObject.RecalcAbilitys()`（override） | `src/M2Server/Player/PlayObject.cs:2008` | 先 `base.RecalcAbilitys()`，随后才是装备循环。 |
| 装备循环 | `src/M2Server/Player/PlayObject.cs:2100-2102` | `for i in 0..UseItems.Length`；门 = `UseItems[i] != null && Index > 0`。 |
| **`Dura == 0` 判定** | `src/M2Server/Player/PlayObject.cs:2105-2120` | 在 `ApplyItemParameters` **之前**：只把模板 `Weight` 计入 `WAbil.HandWeight/WearWeight`，然后 `continue`（`:2119`）。 |
| 属性累加 | `src/M2Server/Player/PlayObject.cs:2122-2123` | `ApplyItemParameters` / `ApplyItemParametersEx`；`WeaponStrong` 的赋值在 `:4424-4426`、`:4563-4566`。 |

⇒ 跳过的是**整个槽**（重量之外的一切属性），不是只跳过 `WeaponStrong` 一项。

## 2. Rust 对应实现

| 函数 | 位置 | 事实 |
|---|---|---|
| `recalc_abilitys_with_prior` | `mir2-rs/bins/game-svr/src/ability.rs:645-649` | 既有实现已同层复刻：`if inst.dura == 0 { add_weight(..); continue; }`。 |
| `weapon_strong_of` | `mir2-rs/bins/game-svr/src/ability.rs:301` | 修复前只判 `index == 0`（`:311`），**漏了 `dura == 0`** ⇒ 与 oracle 潜在分叉。 |
| 修复 | `mir2-rs/bins/game-svr/src/ability.rs:312-315` | 在模板查表与 `apply_item_parameters` 之前加 `if it.dura == 0 { continue; }`，与主循环同层同口径。 |

`use_items` 的类型是 `[Option<crate::bag::BagItem>]`，`BagItem.dura` 来自 DB/装备动作；`Index > 0 & Dura == 0` 在 Rust 里**可构造**（测试直接构造），不是类型上不可达。

## 3. 用例（2 条，`bins/game-svr/src/ability.rs`）

| 用例 | 断言 |
|---|---|
| `weapon_strong_of_counts_slot_when_dura_positive`（`:1926`） | `Dura=1`、`StdMode=5`、`SpecialPwr=4` ⇒ `weapon_strong_of == 4`（防回归）。 |
| `weapon_strong_of_skips_slot_when_dura_zero`（`:1954`） | `Dura=0`、`SpecialPwr=4` ⇒ `== 0`；再以 `Desc[7]=7` 的同物品验 `Desc[7]` 覆盖支也**不得**越过该跳过 ⇒ `== 0`。 |

既有 `weapon_strong_of_consumes_upgrade_desc7_for_weapons` 的夹具原本是 `dura: 0`（修复后会被整槽跳过、不再验证它声称的口径），本轮把夹具改为 `dura: 1` 并注明原因；五档断言不变。

## 4. 部署可达性复核

查询（reviewer-6 在 `d3868d66` 的实跑；本车道按红线**不连库**，沿用该结果）：

```sql
SELECT COUNT(*) FROM mir2_db.characters_item
WHERE Position BETWEEN 0 AND 12 AND (Dura = 0 OR DuraMax = 0);
-- ⇒ 0 / 95409
```

即：穿戴表装备槽（`Position 0..12`）共 95409 行，`Dura=0` 或 `DuraMax=0` 的行为 **0**。
另外 38 条 `SpecialPwr ∈ 1..=10` 命中模板的 `DuraMax` 全 > 0；正常磨损到 0 时
`DoDamageWeapon` 会把 `Index` 清 0（`src/M2Server/Player/PlayObject.Attack.cs:1444-1457`）。

⇒ **当前不可达、已按 oracle 对齐**（T-266 行已补记），不是「已知偏差」，不需要新 `T-*` 号。

> 复核备注：仓内 `sql/mir2_db.sql` 是 2023 年的**脱敏**导出（`characters_item` 77935 行里
> 仅 4 行 `StdIndex>0`，其余被清零），**不能**用来重算 95409 分母；分母只在部署库有意义。

## 5. 牙（先提交再变异，`git checkout --` 还原）

| 步骤 | 结果 |
|---|---|
| 变异：删除 `weapon_strong_of` 里刚加的 `if it.dura == 0 { continue; }` | — |
| `cargo test -p game-svr --lib weapon_strong_of_skips_slot_when_dura_zero` | **FAILED**，`rc=101`，`left: 4 / right: 0`（断言 `Dura==0 ⇒ 整槽跳过，模板 SpecialPwr 不贡献`） |
| `git checkout -- mir2-rs/bins/game-svr/src/ability.rs` | `git status --short` 空 |
| `sha256sum` 工作树 vs `git show HEAD:...` | 两侧均 `63be6595…a276d1`（对齐 head blob） |

## 6. 门禁（head 实跑）

| 门禁 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | **141 suites / 3153 passed / 0 failed / 62 ignored**，rc=0 |
| `cargo run -p msgcodegen -- --check` | **604 条**一致，rc=0 |
| `whitelist_anchor_gate.py check` | `ALL PASS`（589 条锚） |
| `t_reservation_gate.py check` | GREEN（rows=255 / whitelist=255 / max=T-267） |
| 冲突标记 / `git diff --check` | 0 / rc=0 |
| `git merge-tree --write-tree refs/remotes/walgit/master HEAD` | 见卡上 patch 条目 |

## 7. R 级窗口项（`window_required=true`，随 T-266 一并保持）

本卡不改变当前可观测行为（部署不可达），但结论依赖「装备槽 `Dura=0` 行 = 0」这一数据事实，
仍按 R 记账：真机窗口跑 `tools/probe/scenarios/equip_struck_save.json` 时顺带复核
`ⓓ 存档 characters_item.Dura 与帧一致`；若将来出现 `Index>0 & Dura==0` 的档案（异常写入/老档/
放宽破零清 Index），两侧 `AddAbil.WeaponStrong` 现已同口径（本卡关闭该分叉）。
