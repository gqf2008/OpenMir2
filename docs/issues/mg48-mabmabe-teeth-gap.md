# MG-48（`SKILL_MABE(50)`，魔爆）：**无产品行为牙**——补牙卡

> 状态：**已知缺口（2026-11-21 人工复核结论）**。只立卡，未实验。
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
