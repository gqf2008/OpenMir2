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

## 5. 实测补牙（2026-11-21，`card/mg57-tooth`）——**已从"无牙"转为"有牙"**

**变异（leaf-specific，打 37 自己的判定维度）**：在共享推挤分支的宿主调用点
（`mir2-rs/bins/game-svr/src/lib.rs` 的 8/21/37 共用块）把 `nPushLevel` **仅对
`SKILL_ENERGYREPULSOR(37)` 加 1**（8 保持原值）：
`cast_firewind_spell(actor_id, if magic_id == 37 { level + 1 } else { level })`。
→ 打的是 37 的 `MagPushArround(nPushLevel)` 维度，不是共享 `mag_push_arround` 的实现细节。

**基线**：`energyrepulsor` golden 的推送步数是 **s1 1 步**（目标 101→102）、s5/s13 相应步数；
37 三驱动 3 passed、比较器 0 红。

**变异后红线（分开记）**
- **驱动（三支各自红，报帧序列原文）**：
  ```
  [energyrepulsor/s1] 逐收件人 (ident,param,tag,series) 序列必须与金标准一致
    left: caster [(638,101,100,257),(9,102,100,6),(9,103,100,6),(640,1,2,0)] …
  [energyrepulsor/s5] … left: caster [(638,…),(9,102,…),(9,103,…),(640,1,1,0)] …
  [energyrepulsor/s13] … left: caster [...,(9,104,…),(9,103,…),...] …
  ```
  （golden s1 只有 `(9,102,100,6)` 一条；变异多出 `(9,103,…)`）
- **比较器（`real_rust_ledger_energyrepulsor`）**：**18 条红**，逐条为多出的
  `RM_PUSH(8092,wParam=6,p1=103)` 与多出的 `SM_BACKSTEP(9,param=103)`（caster/target/obs_in × s1/s5/s13）。
- ⇒ 该维度**驱动 + 比较器双守**。

**最小可观测幅度 = `nPushLevel +1`（= 1 step）**：`push_steps = 1 + max(0, L-1) + Random(2)`，
L 加 1 即基数加 1；更小只能加 0（= 基线）。

**叶归属证明**：同一变异下 `firewind_parity_s1` **仍 ok**（8 的调用点保持原 level）⇒ 牙打的是
37 的分条，不是 firewind/共享机制。

**还原**：`cp` 快照还原后 `git hash-object lib.rs == git rev-parse HEAD:path`；`MUTATION=0`；
37 三驱动 + 比较器复绿。
