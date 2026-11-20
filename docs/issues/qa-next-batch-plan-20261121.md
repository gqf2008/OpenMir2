# 下一批 magic 叶子排序（2026-11-21，只读体检）

> 现状：magic 域 **40/58 已验**（条目 65 − 重复 7 = 分母 58；verified 40），剩 **18 条**
> （17 条 `implemented_unverified` + 1 条 `missing`）。本文件只做分类与排序，**不改代码、不改装置**。
> 分类判据以 **oracle 实际代码** 为准（行号按 master `c4de3a15`）；凡判 A 类，给出"case 体零抽零帧"或
> "既有 golden+比较器可直接套"的依据。

## 0. 分类口径

- **A（零装置改动）**：case 体无抽无帧（或只有 `boTrain = true`），或**既有装置/比较器模式可直接套**。
- **B（需装置改动）**：需要新夹具旋钮或新观测面（逐条写明缺什么）。
- **C（数据缺口阻塞）**：缺 oracle 侧数据（怪物/物品行），无法出 golden。
- **D（证据其实够，只差升格审计）**：本批 **0 条**——现有 18 条里，只有 20/21/8·37/46 等有**单测**覆盖，但
  parity 认定标准是 golden/比较器对拍，单测不构成升格证据（见 `qa-green-is-not-verified` 口径）。

## 1. 18 条分类表

| # | 条目 | 魔号 / 名称 | 类 | oracle 依据（file:line）与抽数/帧面 | B 类缺什么 |
| --- | --- | --- | --- | --- | --- |
| 1 | MG-13 | 6 `SKILL_AMYOUNSUL`（单目标施毒） | **B** | `src/M2Server/Magic/MagicManager.cs:225-259`：`CheckAmulet(1,2)` → `Random(AntiPoison+7)`（`:235`）→ 按 `stdItem.Shape` 1/2 发 `RM_POISON` 自延迟（`:239/:243`，功率除 `Config.AmyOunsulPoint`）；`boTrain` 在 `:249-251` | **护身符 Shape 旋钮**（现夹具只有 `ArmRingAmulet` 布尔=固定 Shape 5，过不了 `CheckAmulet(1,2)`）+ **目标 `AntiPoison` 旋钮** + `AmyOunsulPoint` 配置（`MagicGolden` 只 `ExpConf.LoadConfig()`，未载 `ServerConf`，默认 0 会除零） |
| 2 | MG-14 | `Shape=1@DoSpell` | **B** | 同上 case 的 `case 1:` 臂（`:239-241`，绿毒 `DECHEALTH`） | 同 MG-13（同一 case 的两个 shape 臂） |
| 3 | MG-15 | `Shape=2@DoSpell` | **B** | 同上 case 的 `case 2:` 臂（`:243-245`，红毒 `DAMAGEARMOR`） | 同 MG-13 |
| 4 | MG-16 | 8 `SKILL_FIREWIND`（抗拒火环） | **B** | `src/M2Server/Magic/MagicManager.cs:259-264`：`MagPushArround(level)`；helper `:24-52` 抽 `Random(20)`（`:37`）+ `Random(2)`（`:41`）；帧面 = 逐格 `SM_BACKSTEP` + 共享尾 638 | **视野回填**（`MagPushArround` 遍历 `VisibleActors`，探针手工建对象 ⇒ 表空）+ **可走格**（`CharPushed` 要 `CanWalk`）——即计划 §二 的 MG-57 装置前提 |
| 5 | MG-18 | 10 `SKILL_SHOOTLIGHTEN`（疾光电影） | **A** | `src/M2Server/Magic/MagicManager.cs:277-289`：与 9 同形 `MagPassThroughMagic(..., 8, **true**)`（穿透 8 格）；**装置可直接套**：9 已有 `fire.json` + `real_ledger_verify` + `cast_pass_through_spell`，单测 `spell_p3b5_9_10_pass_through_draws_compounding_and_frames` 已同时覆盖 9/10 | — |
| 6 | MG-34 | 20 `SKILL_TAMMING`（诱惑之光） | **B** | `src/M2Server/Magic/MagicManager.cs:394-402`：`!target.Animal` + `MagTamming`（`:704+`，抽 `Random(4-level)` 等）；效果 = 怪变 slave | **奴仆/收服观测面**（现有 `Skeleton` block 只对魔号 17 落盘，`FireballResult.Skeleton` 由 `spec.MagicId != 17 ? null` 门控）+ 非动物怪夹具 |
| 7 | MG-35 | 21 `SKILL_SPACEMOVE`（瞬息移动） | **B** | `src/M2Server/Magic/MagicManager.cs:403-409`：case **自**发 `RM_MAGICFIRE` 并置 `boSpellFire=false`；`MagSaceMove`（`:876-891`）抽 `Random(11)`（`:879`） → `MapRandomMove(HomeMap,1)` 移动施法者 + `RM_SPACEMOVE_FIRE2` | **施法后坐标观测**（golden 只记夹具输入 `caster_xy`，不记落点）+ MapMgr/HomeMap（`EnableMaps`/`OfflineMapSystem` 已在，需按此支启用） |
| 8 | MG-36 | 22 `SKILL_EARTHFIRE`（火墙） | **B** | `src/M2Server/Magic/MagicManager.cs:412-417`：`MagMakeFireCross(attackPower, time, x, y)`（helper `:1064-1098`，抽伤+`GetRPow`）；效果 = 地面火墙事件，跨 tick 伤害 | **定时地面事件观测**（火墙 MapEvent 的逐 tick 伤害/命中面，现有装置只跑施法+短窗口排水） |
| 9 | MG-42 | 41 `SKILL_ANGEL`（精灵召唤） | **C** | `src/M2Server/Magic/MagicManager.cs:462-472`：`CheckAmulet(2,1)` + `MagMakeAngelSlave` | **数据缺口**：angel 怪物行缺失（卡 `openmir2-angel-summon-monster-missing`）；640 尾已由单测 `spell_cmd_angel_training_tail_is_self_delayed` 覆盖，但召唤效果本身无 golden |
| 10 | MG-44 | 32 `SKILL_KILLUNDEAD`（圣言术） | **B** | `src/M2Server/Magic/MagicManager.cs:480-487`：`IsProperTarget` + `MagTurnUndead`（`:827-856`，抽 `Random(2)`（`:842`）/`Random(100)`（`:847`）；门读 `Config.MagTurnUndeadLevel`（`:844`）） | **`ServerConf` 配置装载**（harness 只载 `ExpConf` ⇒ `MagTurnUndeadLevel` 默认 0 ⇒ 即死支恒不达）+ 目标 `LifeAttrib=LA_UNDEAD`（`MonsterSpec.LifeAttrib` 已有）+ 目标 HP=0 观测（golden 已有 `HpBefore/HpAfter`） |
| 11 | MG-47 | 35 `SKILL_WINDTEBO`（灭天火/狂风） | **B+** | `src/M2Server/Magic/MagicManager.cs:524-529`：`MagWindTebo`（`:858-874`，`Random(20)` + 邻格门 + 等级门 + `CharPushed`）；**Rust 侧 0 引用 ⇒ 未实现**（`missing`，T-285 登记） | **先补实现**（推挤+骰），再复用 8/37 的视野/可走地图装置 |
| 12 | MG-48 | 50 `SKILL_MABE`（地狱火/魔爆？） | **B** | `src/M2Server/Magic/MagicManager.cs:530-536`：`MabMabe`（`:1230-1267`，抽 `Random(10)`（`:1237`）/`Random(2)`（`:1240`）/`Random(MabMabeHitRandRate)`（`:1243`）/`Random(MabMabeHitSucessRate)`（`:1245`）；发 `RM_DELAYMAGIC`+`RM_POISON(STONE)`） | **`ServerConf` 装载**（`MabMabeHit*` 默认 0 ⇒ 成功支恒不达）+ 目标须为玩家（`Race==Play` 才发 STONE）+ 状态观测（现装置无目标 `StatusTimeArr` 落盘） |
| 13 | MG-50 | 48 `SKILL_GROUPAMYOUNSUL`（群体施毒） | **B** | `src/M2Server/Magic/MagicManager.cs:543-548` → `MagGroupAmyounsul`（`:893+`）；现行 golden **退化**（`rng_total_draws=0`、无 `RM_POISON`），根因=夹具护身符 Shape=5 过不了 `CheckAmulet(1,2)`（卡 `openmir2-groupamyounsul-nondegenerate-golden`） | 同 MG-13（Shape 旋钮 + AntiPoison + `AmyOunsulPoint`）+ 目标落在 `mag_level` 范围内 |
| 14 | MG-52 | 43 `SKILL_43`（破空剑） | **A** | **case 体是空体**：`src/M2Server/Magic/MagicManager.cs:555-556` `case MagicConst.SKILL_43: break;`（零抽、零帧、零状态）；`mir2-rs/crates/world-bevy/src/magic.rs`（`no_effect_case` 注释） 把它归 `no_effect_case`。注意其可达路径是 ClientSpellXY **显式 case** 的开关（`PlayObject.Attack.cs:1253-1266`，`SendSocket("+CID"/"+UCID")`） | —（走 warr 式入口装置；观测面是 `+CID/+UCID` 短包，须确认 harness 记录短包；见 §3 风险） |
| 15 | MG-55 | 46 `SKILL_46`（分身术） | **B** | `src/M2Server/Magic/MagicManager.cs:589-594`：`MagMakeClone`（`:1337+`，恒 true）；帧面 = 克隆体 `SM_TURN(10)`/`SM_FEATURECHANGED(41)` + 638/640 | **克隆体观测**（harness 需能 `new PlayCloneObject` 并记录 10/41；现有单测 `spell_tail_clone46_...` 只覆盖尾 640） |
| 16 | MG-57 | 37 `SKILL_ENERGYREPULSOR`（气功波） | **B** | `src/M2Server/Magic/MagicManager.cs:601-606`：与 8 **逐字同形**（`MagPushArround`，helper `:24-52`） | 同 MG-16（视野回填 + 可走格；8/37 共用同一装置批） |
| 17 | MG-64 | `Shape=1@MagGroupAmyounsul` | **B** | `src/M2Server/Magic/MagicManager.cs:919-921`：`MagGroupAmyounsul` 内 `case 1:`（绿毒） | 同 MG-50 |
| 18 | MG-65 | `Shape=2@MagGroupAmyounsul` | **B** | `src/M2Server/Magic/MagicManager.cs:923-925`：`case 2:`（红毒） | 同 MG-50 |

**合计**：A **2**（10、43）｜B **15**（含 1 条 B+ 未实现 35）｜C **1**（41）｜D **0** —— 2+15+1=18。

## 2. 建议：下一批做哪几条

**第一批 = 6 条 Amyounsul 线（MG-13/14/15/50/64/65）**，理由：
1. **一个装置旋钮闭掉 6 条**：只需给 MagicGolden 加「护身符 `Shape` 选择」+「目标 `AntiPoison`」两个旋钮、并在装置侧补 `AmyOunsulPoint`（三个都是 harness-only，**不动产品代码**）；
2. 它同时解除一张**已存在的阻塞卡** `openmir2-groupamyounsul-nondegenerate-golden`（MG-50 的退化 golden），把该卡的 DoD 直接变成新 golden 的判据；
3. 单目标（6）与群体（48）走**同一形态**，golden+比较器模式与 49–54 批一致，可一次成型。

**并行可做（A 类快赢）**：MG-18（10）直接套 `fire` 的装置（`fire.json` 同构、只换魔号/穿透格数/`res=true`）；MG-52（43）case 体为空体，但**先确认** `+CID/+UCID` 短包在 harness（`RecordingNetChannel`/gate 缓冲）里可观测——若可观测则同批做，否则列入 B（短包观测面）。

**次批**：8/37/21（视野回填 + 可走格，计划 §二 的独立批）+ 32/50（先补 `ServerConf` 装载）。这两组的共同点是"装置面扩展"，与 Amyounsul 批互不依赖，可并行派。

**暂不做**：41（等 angel 怪物数据）、35（先补产品实现）。

## 3. 风险与前提（给派工用）

- **A 类 MG-52 的观测面**：`+CID/+UCID` 是 `SendSocket` 短包，不是 SM 帧；若 harness 不记短包，则该条实为 B（需在装置里加短包记录）。派工前先核 `RecordingNetChannel` 的缓冲是否收短包。
- **B 类共同前提**：`MagicGolden` 目前只 `ExpConf.LoadConfig()`（`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs`（`MagicGolden` 只调 `ExpConf.LoadConfig()`））；`AmyOunsulPoint` / `MagTurnUndeadLevel` / `MabMabeHit*` 都在 `ServerConf` 里 ⇒ 相关批次要么在装置里补载 `ServerConf`，要么显式设置这几项（**不得依赖默认 0**，否则分支恒不达、golden 再次退化）。
- **35 的 B+**：先补 `MagWindTebo` 实现（推挤 + 等级门 + `Random(20)`），产品面改动与 8/37 同文件同区，须与视野装置批排同序，避免写集冲突。
