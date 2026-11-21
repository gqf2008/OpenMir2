# MG-57（`SKILL_ENERGYREPULSOR(37)`，气功波）：**无牙**——补牙卡

> 状态：**已知缺口（2026-11-21 人工复核结论）**。只立卡，未实验。
> 升级口径见 `docs/issues/parity-assertion-selfcheck.md` 判据 1/7。

## 1. 当前无牙的事实 + 依据

- 条目已 verified（`real_rust_ledger_energyrepulsor` × s1/s5/s13），但**没有产品侧变异执行记录**。
- 依据：`docs/issues/magic-push-batch-impl.md` §5「待办」仍列 **"退化守卫（两侧 ≥1 效果帧）与双向牙"**
  ⇒ 只有设计/计划（`magic-push-batch-wiring-plan.md` §4），**未落地**。
  `assert_effect_frames_non_degenerate` 是**结果守卫**，不是"改坏产品会红"的牙。
- oracle 锚：`src/M2Server/Magic/MagicManager.cs:601-606`（与 8 的函数体逐字相同：`MagPushArround` + `boTrain`）。

## 2. 候选变异（函数/分支级，任选其一）

1. `mir2-rs/crates/world-bevy/src/magic.rs` 的 `mag_push_arround`：推动步数 `nPushLevel` 用
   `level` 改 `level + 1`（或方向/坐标推进 1 格偏差）；
2. 帧编码处 `SM_BACKSTEP(9)` 的 `series/param` 取错格（只影响 37 的推送分支）。

## 3. 基线 / 变异两行期望

| 状态 | 期望 |
|---|---|
| 基线 | `energyrepulsor_parity_s1/s5/s13` 3 passed |
| 变异后 | **至少一支红**（推送帧 `SM_BACKSTEP(9)` 的字段/序，或 `Rng` 抽数） |

## 4. 预期红在哪

- `real_rust_ledger_energyrepulsor`：各收件人 `Sent` 严格序 / `SM_BACKSTEP` 字段 / `rng_total_draws`（golden 3/3/3）；
- 驱动 `tests::energyrepulsor_parity_s*` 若内置断言同样会红，贴原文。
