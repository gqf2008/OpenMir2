# A 堆（9 条）三段式证据映射（2026-11-20，树 `165c6edd1`）

> 只读产物；**不改 status / 不碰 `Program.cs` / 不放宽判据**。行号按 `165c6edd1`，master 前进后标 STALE。

## 一、oracle 分派元组（同一 `case` 组）

`src/M2Server/Magic/MagicManager.cs:315-321` 七个 case 落入同一组，`:322-325` 统一
`CheckAmulet(playObject, 1, 1, ref nAmuletIdx)` → 通过后 `switch (userMagic.Magic.MagicId)` 再分派。
**组内实参（对 7 个魔号完全相同）**：`CheckAmulet(1,1)`；失败 ⇒ `boSpellFail` 保持 true（`:322`）。

| 条目 | 魔号 | oracle case 行 | 组内效果分派 | verified 兄弟 |
|---|---|---|---|---|
| MG-20 | 13 FIRECHARM | :315 | `:328` 起（`Random(10) >= AntiMagic` 门 → 撞击/延迟链） | MG-27（firecharm，已 verified） |
| MG-21 | 14 HANGMAJINBUB | :316 | `:352` 起（`MagMakeDefenceArea(...,3,...)`） | MG-28（hangmajinbub） |
| MG-22 | 15 DEJIWONHO | :317 | `:359` 起（同上，DefenceUp 早退语义不同） | MG-29（dejiwonho） |
| MG-23 | 16 HOLYSHIELD | :318 | `:366` 起（`MagMakeHolyCurtain`） | MG-23? 无；见 MG-30 同号 | 
| MG-24 | 17 SKELLETON | :319 | `:372` 起（`MagMakeSlave`） | **无 verified 兄弟** |
| MG-25 | 18 CLOAK | :320 | `:378` 起（`MagMakePrivateTransparent`） | MG-32（cloak） |
| MG-26 | 19 BIGCLOAK | :321 | `:384` 起（`MagMakeGroupTransparent`） | MG-33（bigcloak） |
| MG-30 | 16 HOLYSHIELD | :318 | 同 MG-23（**同魔号、同 case，两条目指同一分派位点**） | — |
| MG-31 | 17 SKELLETON | :319 | 同 MG-24（同上） | — |

**Rust 分派元组**：`amulet_requirement` 在 `mir2-rs/crates/world-bevy/src/magic.rs` 写
`13..=19 => Some((1,1))` —— **7 个魔号同参数，与 oracle 逐字段一致**。
效果分派：14/15 在 `bins/game-svr/src/lib.rs` 里 `SKILL_HANGMAJINBUB | SKILL_DEJIWONHO` 的匹配臂（符号锚）；16/18/19 在 `:15515-15517`（`:15553-15555`
再分 `:15571`（16）/`:15601`（18）/19 支）；17 在 `:14684` 附近并有 `:14825` 的分组说明注释。

## 二、差异枚举（决定能否引用兄弟证据）

| 条目 | 与兄弟的差异 | 能否引用兄弟效果证据 |
|---|---|---|
| MG-20 | 与 MG-27 同魔号 13、同 case ⇒ 输入字段预期全同（**待逐字段复核**） | 可（`scope = shares_effect_evidence_with_MG-27`，交 QA 认可） |
| MG-21 | 同 14 号 | 可（…with MG-28） |
| MG-22 | 同 15 号 | 可（…with MG-29） |
| MG-25 | 同 18 号 | 可（…with MG-32） |
| MG-26 | 同 19 号 | 可（…with MG-33） |
| **MG-23** | 16 号；**无 verified 兄弟**（MG-23/30 是两条目同一魔号） | **不能引用，须本支对拍**（v1 仅 gate_only） |
| **MG-24** | 17 号；**无 verified 兄弟** | **不能引用，须本支对拍** |
| MG-30 | 同 MG-23（同分派位点） | 与 MG-23 应合并判定：**两条目同一效果**；若 QA 同意"一条目一结论"，须各自出结论但可共享证据 |
| MG-31 | 同 MG-24 | 同上 |

## 三、未复核（诚实标注）
- **帧面形状列**（52/657/动作帧/无帧）与 **RNG 抽数列** 未逐条补：13/14/15/16/18/19 的形状可从已 verified
  兄弟的 golden 读出（各自 golden 已有帧/RM 记录）；17（`MagMakeSlave`）与 16 的 RNG 结构需按 helper 体补。
- 上述"可引用"结论**以同魔号同 case 为前提**；`CheckAmulet` 是否消耗随机数**尚未核**——若消耗，引用仍成立
  （两侧同一次消耗），但要在 `scope` 说明里注明。

## 四、建议的首批（差异最大、方法最受考验的 1–2 条）
1. **MG-24（17 SKELLETON）**：无兄弟、需 `MagMakeSlave` 全链，最能检验方法是否站得住；
2. **MG-30（16 HOLYSHIELD 的第二条目）**：检验"同一分派位点两条目"如何各自成结论（共享证据但不并结论）。
