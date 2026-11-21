# MG-48（`SKILL_MABE(50)`，魔爆）：**无产品行为牙**——补牙卡

> 状态：**已补（2026-11-21，`card/mg48-teeth`）**：产品行为牙已实测；另记录"哪些幅度不可见"及最小可观测幅度。
> 升级口径见 `docs/issues/parity-assertion-selfcheck.md` 判据 1/7。

## 1. 当前无产品行为牙的事实 + 依据

- 条目已 verified（`real_rust_ledger_mabmabe` × s1/s5/s13）。
- 现有唯一"牙"是**装置面存在性牙**：`docs/issues/magic-serverconf-batch-impl.md` §4 变异②
  「台账不写 `ServerConf` ⇒ 驱动 `[mabmabe/s1] Fixture.server_conf/ServerConf 存在性不对称` 判红」。
  ⇒ 它证明"配置被读进去了"，**不证明"伤害/骰门算错会被发现"**。
- oracle 锚：`src/M2Server/Magic/MagicManager.cs:530-536`（`SKILL_MABE` → `MabMabe`）。

## 2. 候选变异（函数/分支级，任选其一）

1. `mir2-rs/crates/world-bevy/src/magic_effect.rs` 的 `mab_mabe_gates`：命中最小等级门
   `Random(...) < MAX(MabMabeHitMinLvLimit, nLevel*8 - nLevel + 15 + nLv)` 的 `<` 改 `<=`；
2. `MabMabeSpellOutcome` 的伤害落地处（`world.rs` 消费点）把 `nPower` +1。

## 3. 基线 / 变异两行期望

| 状态 | 期望 |
|---|---|
| 基线 | `mabmabe_parity_s1/s5/s13` 3 passed；`TargetHp` 100→95/98/100 |
| 变异后 | **至少一支红**（`TargetHp` 或 `31/656` 帧字段或抽数 10/11/5 之一） |

## 4. 预期红在哪

- `real_rust_ledger_mabmabe`：`TargetHp`（100→95/98/100）、帧面（`[17,638,53,31,656]` 等）或抽数不一致。

## 5. 建议

**建议补**（而非"建议不补"）：候选改点便宜、可观测量直接（`TargetHp` 已有三档 golden），
补一颗即可把"配置被读进去"升级为"算错会被发现"。

## 6. 实验记录（2026-11-21）

**先试了两个不可见幅度**（记录它们本身有价值——判据对该字段的分辨率）：

| 变异点 | 幅度 | 结果 | 原因 |
|---|---|---|---|
| `world.rs::cast_mab_mabe_spell` 的 `out.msd_power = n_power` | `+1`、`+3` | **全绿**（driver 3 + comparator 1） | `msd_power` 只是结果报告字段；真正承载 `SM_STRUCK(31)`/`TargetHp` 的是**第一条** `RM_DELAYMAGIC` |
| `magic_effect.rs::mab_mabe_gates` 的 `success_gate_pass = true` → `false` | 恒失败 | **全绿** | 命中门失败只抑制**第二条**延迟；比分面来自第一条（先于等级门发出） |

**可观测变异（最终采用）**：`power / 3` → `power / 3 + 1`（第一条 `RM_DELAYMAGIC` 载荷）。

- **分类（三态）**：**driver 红 + comparator 红 ⇒ 牙完整**（driver 内含 `fireball_case_assert`，双面都能发现）；
- **红原文**（`magic_fireball_parity.rs` 第 1046 行，22 条不等）：
  `[s1] TargetHp 不等：golden=95 rust=94`、`[s1/caster] Sent[1].param 不等：golden=95 rust=94`；
  `[s5] TargetHp 不等：golden=98 rust=97`；Rm 多重集 `[8103,5,95,…]` vs `[8103,6,94,…]`；
  **s13（未中）不变** ⇒ 命中支专属，符合预期；
- **最小可观测幅度 = +1**（power/3 从 5→6；相比之下 `msd_power` 的 +3 仍不可见）；
- **还原四条**：`git hash-object` 回 `2157df49…`（= 变异前）、`MUTATION`=0、driver 3 支与 comparator 复跑 `1 passed`、`git status` 空。
