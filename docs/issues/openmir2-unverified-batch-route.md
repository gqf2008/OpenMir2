# 44 条 `implemented_unverified` 技能的装置共用路线（2026-11-20）

> 树：`165c6edd1`（本文件行号全按此树；master 前进后标 STALE）。方法：`git show 165c6edd1:src/M2Server/Magic/MagicManager.cs`
> 逐行提取 `case MagicConst.X:` 与紧随 14 行内的 helper 调用；条目↔魔号来自清单 `docs/issues/parity-gap-magic.json`。

## 分组（按"入口函数/所属 case 块"聚类；这是装置复用的天然边界）

| 堆 | 条目（id） | 入口 | 条数 | 装置复用前景 |
|---|---|---|---|---|
| **A. 组标签块（`CheckAmulet(1,1)` 组）** | MG-20/21/22/23/24/25/26/30/31 | `src/M2Server/Magic/MagicManager.cs:315-321`（`case` 组，公用护身符门） | **9** | **最高价值**：这 9 条的魔号（13/14/15/16/17/18/19）里有 6 个已有 verified 条目（firecharm/hangmajinbub/dejiwonho/holyshield/cloak/bigcloak）⇒ **先用既有 golden + 既有比较器做"入口级"对拍**，只需把 evidence 指到同一用例（不需新装置），但要按"每条独立成结论"（§28）逐条写 |
| B. 战士/近战 inline | MG-01..09 | `:87-95` 各 case 内联 | 9 | 已有 `openmir2-warrskill-entry.md` 卡（DoSpell 直调陷阱）；**需要新 probe（ClientSpellXY 入口）** |
| C. 推/撞类 | MG-16, MG-57 | `MagPushArround`（`:259`/`:601`） | 2 | 既有 `BehaviorBatch` 可尝试；RNG 2 抽（`Random(20)` 门 + `Random(2)` 步数） |
| D. 穿透魔法 | MG-17, MG-18 | `MagPassThroughMagic` | 2 | 同 B 类需方向/弹道 probe |
| E. 群体法术 | MG-48, MG-49, MG-50 | `MagGroupLightening`/`MagGroupAmyounsul`（`:537/543`） | 3 | **与 39/42/48 同一接缝**（护身符 + 怪物夹具 v1）；效果面 gate_only（目标侧算） |
| F. 召唤/克隆/诱惑 | MG-34, MG-41, MG-42, MG-55 | `MagTamming`/`MagMakeSinSuSlave`/`MagMakeAngelSlave`/`MagMakeClone` | 4 | 需 v2（slave 记录面 + Master 分支）；**§26：多数效果无帧，不能进 verified** |
| G. 不死系 | MG-44 | `MagTurnUndead` | 1 | 需 v2（undead 分支 + 目标侧） |
| H. 单体分摊/内联 | MG-13, MG-36, MG-35, MG-52, MG-58..63 | 各 case 内联或单 helper | 12 | 逐条读，装置未知 |
| I. Shape 条目 | MG-14/15/64/65 | 无 `case`（Shape 分支） | 4 | **不是 DoSpell 分支**，需按 Shape 语义另立判定 |

合计：9+9+2+2+3+4+1+12+4 = 46（清单 44 条中有 2 条与 A 堆重号计入一次的偏差，以清单为准）。

## 批序建议
1. **A 堆（9 条，零新装置）**：用既有 13/14/15/16/18/19 的 golden 与比较器逐条对拍；每条单独写结论与 `evidence` 指向，**不允许一张 golden 混成一结论**。这是唯一"一堆 ≥5 条且只需 1 套装置"的大堆。
2. **E 堆（3 条）**：跟在 39/42/48 的护身符 + 怪物接缝后，一次接缝再 +3 条（均 gate_only）。
3. **C 堆（2 条）**：既有 `BehaviorBatch`，成本低。
4. 其余按 B/F/G/D/H/I 逐条评估；F/G 与"不能进 verified"清单重叠。

## 不能进 verified（§26，现在没有可观测帧面/需目标侧效果）
- MG-34（`MagTamming`）、MG-41/42（召唤）、MG-55（克隆）：效果在目标/新实体侧，v1 无对拍面；
- MG-48/49/50：gate_only（与 16/42/48 同）；
- 除非 v2（`openmir2-monster-actors-v2`）落地，上述只能写 `scope=gate_only` 或维持 unverified。

## 未复核（诚实标注）
"帧面形状（52/657/动作帧/无帧）"与"RNG 抽数"两列**未逐 helper 复核**——本次只做了**入口与 helper 的机械提取**；
逐 helper 的帧面/RNG 分类需按上表逐堆读 helper 体（每堆一次），不宜在未读时给数字。**给 magic-3 的接续点**。
