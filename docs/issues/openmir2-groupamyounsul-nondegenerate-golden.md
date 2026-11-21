# MG-50 `groupamyounsul(48)` 需要一份**非退化** golden（2026-11-20）

> 触发：`card/magic-mg23-49-50`（`9ec253049`）接线时发现，48 的现行 golden **完全没覆盖效果体**。
> 本卡只登记"为什么需要新 case、需要什么形态、怎么验收"；**不在本卡造 oracle 数据**。
> 现行处置：48 保持 `implemented_unverified`（接线已合，`notes` 记"golden 退化、效果体未覆盖"）。

## 1. 现象：golden 是退化场景

`golden/magic/groupamyounsul.json`（`s1/s5/s13` 三支）的真实形态：

- `Fixture.rng_total_draws = 0`（施法与交付**一抽都没有**）；
- 两侧都没有 `RM_POISON`；`TargetHp` 前后都是 `100`；
- 帧面只有 `17`（`SM_SPELL`）+ `638`（`SM_MAGICFIRE`），即**只剩施法本身**。

⇒ `cast_group_poison_spell` 的逐目标循环**一次都没执行**。**根因已钉（2026-11-20 QA 复核）**：
现行探针的护身符写死 `StdMode=25, **Shape=5**`，而 48 的逐目标门是 `src/M2Server/Magic/MagicManager.cs:908` 的
`CheckAmulet(playObject, 1, **nType=2**)`——`src/M2Server/Magic/MagicBase.cs:65-70` 的 nType=2 支要求 **`Shape <= 2`** ⇒
**每个目标都在护身符门处被跳过，根本走不到 `Random(AntiPoison+7)`**（这正是 `Rng=0` 的直接原因）。
（"目标是否在 `mag_objects_in_range` 内"仍未被单独证明；造新夹具时**两件事都要满足**：目标在范围内 + 护身符 `Shape≤2`。）
48 的"群体施毒"声明行为在本 golden 下**完全不可观测**。

## 2. 四刀变异原文（最强证据：全绿 ⇒ 循环零次）

对 `base 3362758fa` 逐刀施加、每刀单跑 `tests::groupamyounsul_parity_s1`，**四刀全部 `1 passed`**：

| # | 变异 | 改动点 |
| --- | --- | --- |
| ① | 毒伤表达式 | `power_poison`：`i32::from(get_rpow(rng, sc)) * 2` → `* 3` |
| ② | 目标范围 | `cast_group_poison_spell` 内 `let range = i32::from(level).max(1)` → `+ 5` |
| ③ | 合法目标门 | 移除 `if !crate::combat::is_proper_target_grouped(...) { continue; }` |
| ④ | 毒素骰 | `if !crate::magic_effect::poison_dice(&mut self.rng, anti_poison)` → `if false && ...` |

**判读**：④ 强制骰子通过都无变化 ⇒ 不是骰子/门/表达式的问题，是**循环体没进入**。
（各刀还原均核 `sha256sum` 与快照一致、`MUTATION` 标记为 0。）

## 3. 新 case 需要的夹具形态

- **一个敌对目标真的落在 `mag_objects_in_range(caster.map, tx, ty, level.max(1))` 内**
  （现行夹具里 caster/target/obs_in 都不进该集合 —— 这是根因）；
- 施法者有可用护身符，且 **`CheckAmulet(playObject, 1, nType=2)` 通过**（`src/M2Server/Magic/MagicManager.cs:908`；
  `src/M2Server/Magic/MagicBase.cs:65-70`：nType=2 要求 **`Shape <= 2`**）⇒ `use_amulet` 才会返回 `Some(Shape)`。
  **现行探针 `StdMode=25, Shape=5` 过不了这道门**；`nType=1`（`Shape == 5`）是 `:323` 那条、属魔号 14/15（`MG-28/29`），与 48 无关。
- 触发的 `Shape` 至少覆盖 **1（绿毒，`n_int=40`）**；若 oracle 场景允许，**2（红毒，30）** 另起一支；
- 期望面必须包含：`RM_POISON`（`type/nPower/caster/point` 四字段）、`Rng` 非空、
  目标 `HP` 或毒状态的可观测变化。

## 4. 验收条件（本卡的 DoD）

1. 新 case 落盘后，**§2 的四刀变异至少一刀变红**——这是"效果体真的跑到了"的**必要条件**，
   也是本卡的**唯一硬判据**（不许用"帧数多了"代替）；
2. 新 case 的 `rng_total_draws > 0`，且台账与 golden 都出现 `RM_POISON`；
3. 48 的驱动/比较器（`groupamyounsul_parity_s*` / `real_rust_ledger_groupamyounsul`）把新 case
   一并纳入（`b1_golden_drain_assert` 的 `DrainPasses` 按新 golden 更新）；
4. 只有 1–3 全过，才允许把 MG-50 从 `implemented_unverified` 升格 —— 且 manifest `notes`
   里"golden 退化"那条同步改写为"已由非退化 case 覆盖（case 名）"。

## 5. 与其它卡的关系

- `card/magic-mg23-49-50`（`9ec253049`）：接线已完成并合入；本卡**不改**它，只补 oracle 数据。
- 不新造 oracle 数据的边界属于那张卡；**本卡就是解除它的那张**——需要跑 C# 侧重新采集。
