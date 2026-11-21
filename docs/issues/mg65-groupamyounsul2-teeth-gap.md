# MG-65（`Shape=2@MagGroupAmyounsul`，群体红毒臂）：**无牙**——补牙卡

> 状态：**已知缺口（2026-11-21 人工复核结论）**。只立卡，未实验。
> 升级口径见 `docs/issues/parity-assertion-selfcheck.md` 判据 1/7。

## 1. 当前无牙的事实 + 依据

- 条目已 verified（`real_rust_ledger_groupamyounsul2` × s1/s5/s13），但**没有任何产品侧变异记录**。
- 易被误当"有牙"的记录：`docs/issues/openmir2-groupamyounsul-nondegenerate-golden.md` §2 的**四刀变异**
  跑的是 `tests::groupamyounsul_parity_s1`（**48 主叶**），**不覆盖本 Shape=2 叶**；
  且那四刀**全绿**是"循环零次"的**诊断**，本身不是牙。
- oracle 锚：`src/M2Server/Magic/MagicManager.cs:923`（`MagGroupAmyounsul` 的 Shape=2 臂）。

## 2. 候选变异（函数/分支级，任选其一）

1. `mir2-rs/crates/world-bevy/src/magic.rs` 的 `power_poison`：Shape=2（红毒）的 `n_int`（30）改成 31，
   或把 Shape=2 误用 Shape=1 的 `n_int=40`；
2. `mir2-rs/crates/world-bevy/src/magic_effect.rs` 的 `poison_dice`：`anti_poison` 比较 `<` 改 `<=`；
3. `mir2-rs/crates/world-bevy/src/world.rs` 的 `cast_group_poison_spell`：`shape` 取值分支写错
   （只在 Shape=2 时生效，避免波及 48 主叶）。

## 3. 基线 / 变异两行期望

| 状态 | 期望 |
|---|---|
| 基线 | `groupamyounsul2_parity_s1/s5/s13` 3 passed |
| 变异后 | **至少 s1 红**（帧/目标毒状态/抽数之一）；48 主叶（`groupamyounsul_parity_*`）应**不红**，否则说明改动落在共享路径 ⇒ 加 shape 条件重来 |

## 4. 预期红在哪

- `real_rust_ledger_groupamyounsul2`：`RM_POISON`（type/nPower/caster/point）或目标 HP/毒位
  /`Rng` 抽数不一致；
- 若驱动侧有断言（`tests::groupamyounsul2_parity_s*`），同样贴其 `left/right`。
