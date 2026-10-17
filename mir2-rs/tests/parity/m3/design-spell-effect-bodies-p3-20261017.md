# `DoSpell` P3 效果体（DeferredCase 28 + P2 3 + ExplicitCase 1 = 32 行）—— 逐条对读 / 可达性 / 分批 / 判据模板

- 卡片：`openmir2-spell-shared-tail-wiring`（本件 = P3 设计件，tier=L；实现片按批再判 **R**）
- 基线：`0d4b61840`（本件首版产出时基线 `dce6642f4`；P2 已由 `8ca4be1d9` 落地并合入 `0d4b61840`）
- 上游设计件：`mir2-rs/tests/parity/m3/design-spell-shared-tail-20261017.md`（下称「上游件」）
- 范围：**只**处理 P3 的 32 行（= 上游 enum `DeferredCase` 28 + P2 已接效果体 3（14/15/31）+ `ExplicitCase` 1（43））——逐条 oracle 对读、部署魔表可达性、3–5 批实现计划、
  每批判据模板。**本件不实现任何效果体**，不改生产代码。
- 引用纪律：oracle（`src/**`、`sql/**`，冻结）一律 `文件:行号`，本件行号全部 `awk NR` 逐条实测；
  Rust（`mir2-rs/**` 会随并行车道位移）**只写符号 / 块名 / 函数名，不写 `:行号`**。
- 只读声明：本件首版在自建 worktree `/e/tmp/impl-p3plan`（`git worktree add … dce6642f4 --detach`）产出，本修订版在 `/e/tmp/impl-p3plan3`（`0d4b61840 --detach`）产出；
  共享主检出未改一行。未改 `src/**`、`sql/**`、`MirClient/`、`crates/{protocol,storage}/**`、`E:/MirServer/**`；
  未起服务/客户端/假人、未占端口、未删文件。数据库只执行 **SELECT**（`mir2_data.magics` / `stditems`），
  未写库（详见 §2.1、§7）。
- 数据源时点：2026-10-17，本机 MariaDB 10.11.19（`127.0.0.1:3306`，`mir2_data`）。
  `sql/mir2_data.sql` 与在线表在 32 行上逐字段一致（§7-⑤）——两源互证，无漂移。

---

## 0. 结论摘要（先看这里）

1. **P3 范围 = 32 行**：上游 enum `DeferredCase` **28**（含 35）+ P2 已接效果体 **3**（14/15/31）+ `ExplicitCase` **1**（43）。**32 不是上游 `DeferredCase` 集合**（该集合是 28，`{1,2,5,6,9,10,11,13,16,18,19,22,23,24,28,29,32,33,35,36,39,40,42,44,45,47,48,50}`）；本件按 32 行铺表。拆开是：
   - **P2 尾只差接线 3 个**：14、15、31（效果体已在，只缺 `bo_train`/共享尾/护身符门）；
   - **P3 效果体可开工 26 个**：1、2、5、6、9、10、11、13、16、18、19、22、23、24、28、29、32、33、36、40、42、44、45、47、48、50；
   - **不可达 / 不实现 3 个**：35（魔表无行）、39（无技能书）、43（`PlayObject.Attack.cs:1253` 显式 case，玩家路径不调 `DoSpell`）。
2. **可达性（服务端语义）**：32 − 35 − 39 − 43 = **29**。其中 40/42 是战士技能，`default` 支服务端可达，
   但「客户端是否真的发 `CM_SPELL`」无源码可证（MirClient 不在本仓）⇒ 列为**待客户端确认**，不阻塞服务端实现。
3. **分批 5 批（每批 ≤6）**，覆盖 26 个 P3 效果体；P2 的 3 个单列前置批，35/39/43 不入批：
   - B1 单体直击/骰门（6）：1、5、11、13、44、45
   - B2 治疗/爆炸 AoE（6）：2、29、23、24、33、47
   - B3 状态/辅助/不死（6）：16、18、19、32、28、40
   - B4 群体列表/毒（4）：6、48、50、42
   - B5 穿透/火墙/友方 buff（4）：9、10、22、36
4. **判级：5 批全 R**（全部触碰 RNG 抽取次序、帧/时序，部分触碰熟练度落库）。每批实现卡审查必须
   `window_required: true` 并给出真机窗口证据；P3-0（把 P1 内联尾提成 `finish_spell_tail`）与 P3-4（延迟 RM 处理器/状态落地）是 **L**，但 P3-0 触碰 RNG 位置、P3-4 触碰延迟帧序，审查须显式说明 `window_required` 的豁免依据。
5. **两个横切前置**：
   - **目标 AntiMagic / AntiPoison 注入面**：`Entity` 没有 `anti_magic`/`anti_poison`；**SC/MC 不是缺口**——
     `handle_spell_cmd` 已从 `SessionGround.state.wabil` 取到宿主本地 `(mc, sc)`。现有先例是
     `World::cast_defence_area_spell` 由宿主把 `sc` 传进来 ⇒ P3 只为**目标** `AntiMagic`/`AntiPoison`
     沿用「宿主传值」，不扩 `Entity` 字段（扩字段会碰 `crates/world` 接口冻结线）。
   - **延迟消息队列**：`SendSelfDelayMsg` 目前只有 `slave.rs::SelfDelayMsg` 的窄实现；
     `RM_DELAYMAGIC`（8021）、`RM_MAGHEALING`（8034）、`RM_TRANSPARENT`（8045）、
     `RM_DOOPENHEALTH`（8040）、`RM_POISON`（8037）、`RM_DELAYPUSHED`（8043）均未入队。
     严格「帧序」判据必须等它；只判「帧集合」可用既有立即发口径（登记为全局偏差）。
6. **必须先等模型的批**：
   - **目标 AntiMagic/AntiPoison 模型**：B1（全部，AntiMagic）、B4（6/48 另需 AntiPoison；50/42 需 AntiMagic）、B5（9/10，AntiMagic）——没有目标 `AntiMagic` 就无法复刻骰门，
     只补帧会把「缺 N 抽」变成「错位 N 抽」；
   - **延迟队列**：B1、B4、B5 的帧序判据；B2/B3 可先按立即发口径开工，但验收前补队列；
   - **目标枚举（`GetMapBaseObjects` 序）**：B2（23/24/29）、B3（16/18/19）、B4（6/48/42）——
     `x` 外层升序 → `y` 内层升序 → 格内 `ObjList` 插入序，顺序错则 RNG/帧序必错。

---

## 1. P3 范围 32 行逐条表（DeferredCase 28 + P2 3 + ExplicitCase 43）

### 1.1 记号（本表 RNG 列的唯一口径）

| 记号 | oracle 语句 | 抽数 |
|---|---|---|
| `R(n)` | `M2Share.RandomNumber.Random(n)` | 1 次（**`n==0` 也推进一次**：`RandomNumber.cs:93-100` → `System.Random.Next(0)` 仍走 `Sample()`；本仓 `random_below(0)` 同口径） |
| `MPow` | `MPow(userMagic)`（`MagicManager.cs:102-105`） | 1 次 `R(MaxPower-Power)` |
| `GP` | `GetPower(userMagic, n)`（`:107-111`） | 1 次 `R(DefMaxPower-DefPower)` |
| `GP13` | `GetPower13(userMagic, n)`（`:113-119`） | 1 次 `R(DefMaxPower-DefPower)` |
| `RP` | `GetRPow(word)`（`:121-133`） | **0 或 1 次**：仅 `HiByte > LoByte` 时 `R(Hi-Lo+1)` |
| `AP` | `playObject.GetAttackPower(base, range)`（`PlayObject.cs:2909-2952`；`return` 在 2951、方法体止于 2952） | **`Luck==0` 恰 2 次**（`R(range+1)` + `R(10)`）；`Luck>0` 为 1–2 次（`:2918/:2924`）。本表按 `Luck==0` 记 2 |
| `MSD` | `targetObject.GetMagStruckDamage(playObject, nDamage)`（`BaseObject.Base.cs:814`） | 1 次 `R(abs(Hi(MAC)-Lo(MAC))+1)`（命中臂必抽一次；用目标的 `WAbil.MAC`） |
| `×N` | 每目标/每格各执行一次 | 抽数随 N 线性 |

> 关键坑：魔表多数行 `Power==MaxPower`、`DefPower==DefMaxPower` ⇒ `R(0)`。**它们不是「无抽」**，
> 而是「抽了但恒 0」。P3 的连续抽数牙必须按「次数」对齐，不能按「值变化」对齐。
> 取数顺序按 C# 实参从左到右：`GetPower(userMagic, MPow(...))` = 先 `MPow` 后 `GP`；
> `GetPower13(GetRPow(...)+n)` = 先 `RP` 后 `GP13`。

### 1.2 32 行表

「case」列：`N-M` = 顶层 `case` 标签在 `N`、`case` 块止于 `M`；`N/M` = 外组标签在 `N`、内层 `switch` 分支体为 `M`。
「自有帧」只列**可直接归到该 case/helper** 的 `Send*`；`(helper)` 表示帧发生在被调函数内。

| MagID | 常量 | case | 效果体（`Mag*`/helper） | RNG（按执行序） | 自有帧（收件人/延迟） | `boTrain` | `boSpellFail`/`boSpellFire` |
|---|---|---|---|---|---|---|---|
| 1 | `SKILL_FIREBALL` | 174-203（与 5 同块） | `MagCanHitTarget`→`IsProperTarget`→威力式 | `R(10)`；门过再 `MPow+GP+AP`（1 或 5） | `RM_DELAYMAGIC`→自己/600（:183） | :186 门过且 `Race>=Animal` | —/— |
| 5 | `SKILL_FIREBALL2` | 174-203 | 同 1 | 同 1 | 同 1 | 同 1（:186） | —/— |
| 2 | `SKILL_HEALLING` | 204-224 | `IsProperFriend`→威力式→目标自愈 | `MPow+GP+AP`（4）；`IsProperFriend` 失败 ⇒ 0（`:211`） | `RM_MAGHEALING`→目标/800（:216）；`RM_ABILSEEHEALGAUGE`→目标（:221） | :217 `HP < MaxHP` | —/— |
| 6 | `SKILL_AMYOUNSUL` | 225-258 | `CheckAmulet(1,2)`/`UseAmulet`/`GetPower13`/`GetRPow` | `R(AntiPoison+7)`（1）；过则 `GP13+RP`（1–2）⇒ 命中 2–3；门失败 1；无护身符/非 proper target 0 | `RM_POISON`→目标/1000（:241/:245）；`RM_DURACHANGE`→自己（helper `MagicBase.cs:119`） | :250 骰门过且 `Race==Play 或 >=Animal` | **true**→:254 清（需 `IsProperTarget && CheckAmulet && stdItem!=null`）/— |
| 9 | `SKILL_FIRE` | 265-276 | `GetNextPosition`→`MagPassThroughMagic` | `MPow+GP+AP`（4）；helper `R(10)×≤12` | `RM_MAGSTRUCK`→被击中者/600（helper `BaseObject.cs:912`） | :273 helper 命中数 >0 | —/— |
| 10 | `SKILL_SHOOTLIGHTEN` | 277-288 | 同 9（射程 8、不死 ×1.5） | 同 9 | 同 9 | :285 helper 命中数 >0 | —/— |
| 11 | `SKILL_LIGHTENING` | 289-314 | `IsProperTarget`→骰门→威力式 | `R(10)`；过则 `MPow+GP+AP`（1 或 5） | `RM_DELAYMAGIC`→自己/600（:299） | :302 骰门过且 `Race>=Animal` | —/— |
| 13 | `SKILL_FIRECHARM` | 315 / 328-351 | `CheckAmulet(1,1)`/`UseAmulet`→`MagCanHitTarget`→骰门→威力式 | `R(10)`；过则 `MPow+GP+AP`（1 或 5） | `RM_DELAYMAGIC`→自己/1200（:338）；`RM_DURACHANGE`→自己 | :341 骰门过且贴身且 `Race>=Animal` | **true**→:391 清（需 `CheckAmulet`）/— |
| 14 | `SKILL_HANGMAJINBUB` | 316 / 352-358 | `MagMakeDefenceArea(...,3,power,1)`→`MagDefenceUp` | `GP13+AP`（3） | `RM_ABILITY`→每个友方（helper `BaseObject.cs:2185`）；`RM_DURACHANGE`→自己 | :356 helper 结果 >0（中心格 bug ⇒ 49×N） | **true**→:391 清/— |
| 15 | `SKILL_DEJIWONHO` | 317 / 359-365 | `MagMakeDefenceArea(...,3,power,0)`→`DefenceUp` | `GP13+AP`（3） | `RM_ABILITY`→每个友方（helper `BaseObject.cs:2161`）；`RM_DURACHANGE`→自己 | :363 helper 结果 >0（中心格 bug ⇒ 49×N） | **true**→:391 清/— |
| 16 | `SKILL_HOLYSHIELD` | 318 / 366-371 | `MagMakeHolyCurtain(power,...)` | `GP13+RP`（1–2）；helper `R(4)×每个 3×3 内 Animal 对象` | `RM_MAKEHOLYSEIZEMODE`→目标（helper :1168）；`RM_DURACHANGE`→自己 | :369 helper 结果 >0（**任一不合格对象会把 result 重置 0**，:1174） | **true**→:391 清/— |
| 18 | `SKILL_CLOAK` | 320 / 378-383 | `MagMakePrivateTransparent` | `GP13+RP`（1–2）；helper `R(2)×每个贴身合格怪（range 9 的 19×19 内、Race>=Animal、TargetCret==自己、|dx|,|dy|<=1）` | 外层护身符门：`RM_DURACHANGE`→自己（`MagicBase.cs:119`）；`RM_CHARSTATUSCHANGED`(657)→按 `CharStatus` 广播（`MagicManager.cs:695`→`BaseObject.cs:1516`） | :381 helper true（已透明则 false） | **true**→:391 清/— |
| 19 | `SKILL_BIGCLOAK` | 321 / 384-389 | `MagMakeGroupTransparent` | `GP13+RP`（1–2） | `RM_TRANSPARENT`→友方/800（helper :1221）；`RM_DURACHANGE`→自己 | :387 helper true | **true**→:391 清/— |
| 22 | `SKILL_EARTHFIRE` | 412-417 | `MagMakeFireCross`（5 格 FireBurnEvent） | `MPow+GP+AP`（4）+`GP`（1）+`RP`（0/1）= 5–6 | 无立即帧（事件落地） | :415 helper true（非安全区） | —/— |
| 23 | `SKILL_FIREBOOM` | 418-423 | `MagBigExplosion(...,FireBoomRage)` | `MPow+GP+AP`（4） | `RM_MAGSTRUCK`→每个 proper target（helper :1111） | :421 任一目标 | —/— |
| 24 | `SKILL_LIGHTFLOWER` | 424-430 | `MagElecBlizzard` | `MPow+GP+AP`（4） | `RM_MAGSTRUCK`→每个目标（helper :1138；不死全额、其余 /10） | :428 任一目标 | —/— |
| 28 | `SKILL_SHOWHP` | 431-442 | `targetObject != null && !ShowHp` 门（`:432`）→`R(6)` 门→`GetPower13(GetRPow*2+30)*1000` | `R(6)`；过则 `RP+GP13`（2–3） | `RM_DOOPENHEALTH`→目标/1500（:438） | :439 骰门过 | —/— |
| 29 | `SKILL_BIGHEALLING` | 443-449 | `MagBigHealing`（3×3、range 1） | `MPow+GP+AP`（4） | `RM_MAGHEALING`→每个友方/800（:66）；`RM_ABILSEEHEALGAUGE`→每个友方（:71） | :447 任一友方回血 | —/— |
| 31 | `SKILL_SHIELD` | 474-479 | `MagBubbleDefenceUp` | `RP+GP`（1–2） | 无直接帧（buff 下行 657） | :477 helper true（见 §1.4-10 的 `StatusTimeArr` 读） | —/— |
| 32 | `SKILL_KILLUNDEAD` | 480-488 | `MagTurnUndead`→`AnimalObject.Struck` | 非不死/SuperMan：0；否则 `Struck` 的 `R(6)`（条件）+`R(300)`（Animal）+`R(2)`+`R(100)`（等级门）= 0–4 | 无直接帧（改目标态） | :485 helper true（目标 HP 置 0） | —/— |
| 33 | `SKILL_SNOWWIND` | 489-494 | `MagBigExplosion(...,SnowWindRange)` | `MPow+GP+AP`（4） | `RM_MAGSTRUCK`→每个 proper target（helper :1111） | :492 任一目标 | —/— |
| 35 | `SKILL_WINDTEBO` | 524-529 | `MagWindTebo`→`CharPushed` | helper `R(20)`（0–1） | `RM_PUSH`→被推者/逐格（helper `BaseObject.cs:871`） | :527 helper true | —/— |
| 36 | `SKILL_UENHANCER` | 610-630 | `CheckAmulet(1,1)`→`AttPowerUp` | `R(Level)`（Level=0 也抽取一次）+`GP13+AP`（4） | `RM_DURACHANGE`→自己（`MagicBase.cs:119`）；`RM_ABILITY`→被 buff 的目标（helper `PlayObject.cs:5329`→`SM_ABILITY(52)`，`Messages.cs:210`） | :626 门内无条件 true | **true**→:627 清（需 `IsProperFriend && CheckAmulet`）/— |
| 39 | `SKILL_GROUPDEDING` | 549-554 | `MagGroupDeDing`（中心 `targetX/targetY`、半径 `max(1,Level)`） | 每目标 `AP`（2）+`R(SpeedPoint)`（1）= 3N；`nPower>0` 时 `GetHitStruckDamage` 再 1–2 抽（`BaseObject.Base.cs:792/795`）⇒ 3N–5N（39 不可达、不入批，账本仅备查） | `RM_DELAYMAGIC`→自己/200（:968，每命中）；`RM_10205`→广播（:975，每目标） | :552 任一 `Race>=Animal` | —/— |
| 40 | `SKILL_UNAMYOUNSUL` | 495-523 | `IsProperFriend`→`R(7)` 门→清三态计时器 | `R(7)`（1） | 无 | :509/:514/:519 任一状态计时器 !=0 | —/— |
| 42 | `SKILL_GROUPLIGHTENING` | 537-542 | `MagGroupLightening`（先自发 `RM_MAGICFIRE`，再逐目标） | 每目标 `R(10)`；过则 `MPow+GP+AP`（1 或 5）= 1N–5N | `RM_MAGICFIRE`→广播（helper :987；`targetObject.ActorId`，**null ⇒ NPE**）；`RM_DELAYMAGIC`→自己/600（:1004）；`RM_10205`→广播（:1012） | :540 任一命中 | —/**false**（:984 经 `ref`） |
| 43 | `SKILL_43` | 555-556 | 纯 `break`（无效果体） | 无 | 无 | 恒 false | —/— |
| 44 | `SKILL_44` | 557-562 | `MagHbFireBall`（命中/贴身/骰门→威力→推） | `R(10)`；过则 `MPow+GP+AP`（4）+`R(20)`+`R(Level)`（5–7） | `RM_DELAYMAGIC`→自己/600（:1039）；`RM_DELAYPUSHED`→自己/600（:1053） | :560 helper true | —/— |
| 45 | `SKILL_45` | 563-588 | 与 11 同构（`IsProperTarget`→骰门→威力式） | `R(10)`；过则 `MPow+GP+AP`（1 或 5） | `RM_DELAYMAGIC`→自己/600（:573） | :576 骰门过且 `Race>=Animal` | —/— |
| 47 | `SKILL_47` | 595-600 | `MagBigExplosion(...,FireBoomRage)` | `MPow+GP+AP`（4） | `RM_MAGSTRUCK`→每个 proper target（helper :1111） | :598 任一目标 | —/— |
| 48 | `SKILL_GROUPAMYOUNSUL` | 543-548 | `MagGroupAmyounsul`（中心 `targetX/targetY`、半径 `max(1,Level)`） | 每目标 0–3：`CheckAmulet` 失败（`:908`）或非 proper target ⇒ 0；过门 `R(AntiPoison+7)`（1）；命中再 `GP13+RP`（2–3）⇒ 0N–3N；**每目标各消耗一次护身符** | `RM_POISON`→目标/1000（helper :921/:925）；`RM_DURACHANGE`→自己（每目标） | :546 任一命中 | —/— |
| 50 | `SKILL_MABE` | 530-536 | `MabMabe` | `MPow+GP+AP`（4）；`MagCanHitTarget`/`IsProperTarget` 失败 ⇒ 4；`R(10)` 门失败 ⇒ 5；过门后 `R(2)`+`R(rate1)`+`R(rate2)` 各按门 ⇒ 6–8；命中臂 `MSD`（`GetMagStruckDamage`）⇒ 9；玩家且非麻痹再 `R(Level)` ⇒ 10；整体 4–10，成功 9–10（玩家且非麻痹 10，否则 9） | `RM_DELAYMAGIC`→自己/600（:1239）；`RM_DELAYMAGIC`→自己/600（:1254）；`RM_POISON(STONE)`→目标/650（:1257） | :534 helper true | —/— |

> 表内 `×N` 的 N 取决于地图对象数与骰门；判据必须用「构造已知 N」的夹具（见 §4）。
> `RM_MAGICFIRE`（10120）与训练尾不在「自有帧」列：由共享尾统一发；42 例外（helper 自帧且尾置 `boSpellFire=false`）。

### 1.3 helper 家族（分批依据）

| 家族 | 成员 | 共用 oracle helper |
|---|---|---|
| F-直击骰门 | 1、5、11、13、45、44 | `R(10)>=AntiMagic`（44 在 helper 内同式）+ `MPow/GP/AP` + `RM_DELAYMAGIC` |
| F-治疗/爆炸 | 2、29、23、24、33、47 | `MagBigHealing` / `MagBigExplosion` / `MagElecBlizzard` |
| F-状态/辅助 | 16、18、19、28、32、40 | 透明/圣言/困魔/心灵/清毒计时器 |
| F-群体列表/毒 | 6、48、50、42 | `GetMapBaseObjects` 序 + `Random(AntiPoison+7)` + 麻痹 |
| F-穿透/火墙/友方 | 9、10、22、36 | `MagPassThroughMagic` / `MagMakeFireCross` / `AttPowerUp` |

### 1.4 与上游件的交叉核对（**不一致以 oracle 为准**）

| # | 上游写法 | oracle 实测 | 处置 |
|---|---|---|---|
| 1 | 32 行只给 `boTrain` 聚合列表（G2-c），无逐抽 | 每行 RNG 次数/顺序见 §1.2；`R(0)` 也推进 | 本件补全；实现按本表 |
| 2 | 14/15「自有帧 = 无」 | `MagMakeDefenceArea` 对每个友方调 `DefenceUp`/`MagDefenceUp`，二者各发 `RM_ABILITY`（`BaseObject.cs:2161/2185`） | 以 oracle 为准：14/15 有 helper 帧 |
| 3 | 32「无（helper 只改状态）」 | 无直接 `Send*` 正确；但 `AnimalObject.Struck` 抽 `R(6)`/`R(300)`（`AnimalObject.cs:342/352`），`MagTurnUndead` 另抽 `R(2)`/`R(100)`（`MagicManager.cs:842/847`） | 以 oracle 为准：32 有 0–4 抽 |
| 4 | 「自有帧」列未列护身符消耗帧 | 6/13/14/15/16/18/19/36 的 `UseAmulet`：`Dura > nCount*100` 发 `RM_DURACHANGE`→自己（`MagicBase.cs:116-119`）；否则清槽发 `SendDelItems`（`:121-126`） | 以 oracle 为准：**不是**凡过 `CheckAmulet` 都有 `RM_DURACHANGE`——`CheckAmulet` 在 `Round(Dura/100)>=1`（AwayFromZero ⇒ Dura≥50）即过，Dura∈[50,100] 走删除支不发该帧；帧集合判据与耐久夹具需覆盖两支 |
| 5 | 16 helper 只写 `RM_MAKEHOLYSEIZEMODE` | 该 helper 的 `result` 会被**任一不合格对象重置 0**（`:1174`）⇒ 有合格目标也可能 `boTrain=false` | 以 oracle 为准；实现须逐对象保留该重置 |
| 6 | 1/5「命中门内 `Race>=Animal`」 | 实际是 `AntiMagic <= R(10) && 贴身` 之内再判 `Race>=Animal`（`:180-186`） | 以 oracle 为准；`R(10)` 失败时 `target=null`（:191） |
| 7 | 9/10「helper 内部逐格帧」 | 是 `SendStruckDelayMsg(RM_MAGSTRUCK,…,600)`（`BaseObject.cs:912`），每格命中一帧 | 补明帧号/延迟/收件人 |
| 8 | 13 `boTrain :341` | 只在 `R(10)` 过且 `|dx|,|dy|<=1` 且 `Race>=Animal` 时置真；`R(10)` 失败**不**把 target 置 null | 以 oracle 为准 |
| 9 | 28 `GetPower13(GetRPow(...)*2+30)` | 取数序是 `RP` → `GP13`（实参先求值） | 补明顺序 |
| 10 | 31 `MagBubbleDefenceUp`「恒 true」 | `CharacterObject.cs:98` 会因 `StatusTimeArr[BubbleDefenceUP] != 0` 返回 false；状态写入已注释（`:104-105`）⇒ 当前实际恒 true，但**不能**直接写死 `true` | 实现保留该读；`bo_train=true` 只在当前数据下成立 |
| 11 | 42 `boSpellFire=false` + NPE | 正确；NPE 发生在 `:987` 的 `targetObject.ActorId`，被 `PlayObject.Base.cs:1525-1529` 吞掉 ⇒ 639、无尾 | 原样保留；实现为显式「失败臂」 |
| 12 | 48「MagID 48 两行，首行胜」 | **不成立**：`ReadBook` 用 `FindMagic(stdItem.Name)` 按名字精确匹配（`WorldServer.cs:1559-1570`），书 `stditems.Id=503` 命中「气功波」行（`Idx=48`、`MagicId=48`）；`IsTrainingSkill` 只按 `MagIdx==48` 拦（`PlayObject.Attack.cs:340-351`），未学过 48 时可学，`UserMagic.Magic` 保留该行（Spell=8/DefSpell=0/Delay=30），施法仍进 `case SKILL_GROUPAMYOUNSUL(48)` | **行由玩家读到的书决定，不能按魔号首行解析**；实现 `resolve_magic` 须保留书名→行的选择（见 §2.2、§6.1） |
| 13 | 上游 §5.3「P3 其余 29 条」 | 32 − 3（P2）= 29；其中 35/43 不可达、39 无书 ⇒ **可开工 26** | 本件按 26 分批 |
| 14 | 上游 P2 说 14/15 须补护身符门 | 正确；另注意 `MagMakeDefenceArea` 的**中心格 bug**（`BaseObject.cs:2105` 读 `(nX,nY)` 而非 `(cX,cY)`）⇒ range=3 时每友方被加 49 次 buff/帧 | 实现照搬；Rust 现有 `World::mag_make_defence_area` 已复刻，但遍历序见 §6-4 |
| 15 | 上游未提 40/42 的战士职业可达性 | 40（Job=0、NeedL1=48、书 561）、42（Job=0、NeedL1=38、书 501）可经 `ReadBook` 学；`default` 支可达 | 入批，标注「客户端触发待证」 |
| 16 | 上游未提 39 | `MagName=乌龟跑步`、Job=99、NeedL1=50；`stditems` **无同名 StdMode=4 书** ⇒ 玩家学不到 | 判不可达，不入批 |
| 17 | 上游未提 43 的书 | 书 502 存在、可学；但 `PlayObject.Attack.cs:1253` 显式 `case` ⇒ 玩家施法不调 `DoSpell`；仅机器人 `AutoSpell`（`src/Modules/RobotSystem/Services/RobotPlayer.cs:1870`）可达 | 判玩家路径不可达，不入批 |

---

## 2. 可达性（部署魔表只读 SELECT）

### 2.1 判据与数据源

玩家路径要走到 `MagicManager.DoSpell` 的某个 `case`，必须依次满足：

1. **学得到**：`ReadBook`（`PlayObject.Base.cs:2171-2203`）要求
   - `!IsTrainingSkill(magic.MagicId)`（`MagicList` 中尚无该魔号）；
   - `magic.Job == 99 || magic.Job == (byte)Job`（`:2179`）；
   - `Abil.Level >= magic.TrainLevel[0]`（`:2181`）。`TrainLevel[0]` 由 `NeedL1` 装载（`MySqlDB.cs:131`）。
   - 入口：吃 `StdMode==4` 的书（`PlayObject.Operate.cs:632-633`）→ `ReadBook` 用
     `FindMagic(stdItem.Name)`（`PlayObject.Base.cs:2174`）按**名字精确匹配**（`WorldServer.cs:1559-1570`）。
2. **施得出**：`CM_SPELL` → `ClientSpellXY`（`PlayObject.Attack.cs:1054`）先 `GetMagicInfo(nKey)`（`:1065`），
   `MagicList` 无该魔号即返回 false（无任何帧/抽数）。
3. **进得了 `DoSpell`**：`UserMagic.MagIdx` 不是 `switch` 的 8 条显式 `case`
   （`PlayObject.Attack.cs:1120/1136/1152/1168/1187/1218/1234/1253` = 12/25/56/26/27/34/38/43）；
   否则走 `default`（`:1269-1283`）→ `DoSpell`。战士门 `IsWarrSkill` 只拦 3/4/7/12/25/26/27/34/38
   （`MagicManager.cs:82-100`），**不拦 40/42/43**。
4. **过版本门**：`MagicId > 80` 一律 `return false`（`MagicManager.cs:167`）；本表 32 个魔号全部 ≤ 80。

数据源：`mir2_data.magics`（只读）+ `mir2_data.stditems`（只读）。连接串见
`E:/MirServer/M2GameSvr/Server.conf:2-3`（`uid=root;pwd=;database=mir2_data`）；本件只执行 SELECT。
在线表：**108 行 / 95 个唯一 MagID**；32 行逐字段与 `sql/mir2_data.sql:70-177` 一致（§7-⑤）。
`MagicId` 在 C# 侧按 `UInt16` 读取（`MySqlDB.cs:122`），`TrainLv` 恒 3（`:139`），
`TrainLevel = [NeedL1, NeedL2, NeedL3, NeedL3]`（`:131-134`）。

### 2.2 逐行可达性表

Job：`0=战 / 1=法 / 2=道 / 99=任意`。等级 = `NeedL1`（可学等级）。「书(Id)」= `stditems` 中
`StdMode=4` 且名字与 `MagName` 精确相同的行；无书即不可学。

| MagID | 名称 | Job | NeedL1/2/3 | Spell | DefSpell | Delay | 书(Id) | 玩家路径结论 |
|---|---|---|---|---|---|---|---|---|
| 1 | 火球术 | 1 | 7/11/16 | 4 | 1 | 60 | 382 | ✅ 法师 7 级 |
| 2 | 治愈术 | 2 | 7/11/16 | 7 | 0 | 40 | 369 | ✅ 道士 7 级 |
| 5 | 大火球 | 1 | 19/23/25 | 3 | 5 | 60 | 388 | ✅ 法师 19 级 |
| 6 | 施毒术 | 2 | 14/17/20 | 4 | 0 | 40 | 371 | ✅ 道士 14 级 |
| 9 | 地狱火 | 1 | 16/21/26 | 10 | 10 | 60 | 385 | ✅ 法师 16 级 |
| 10 | 疾光电影 | 1 | 26/29/32 | 25 | 20 | 100 | 391 | ✅ 法师 26 级 |
| 11 | 雷电术 | 1 | 17/20/23 | 12 | 6 | 100 | 386 | ✅ 法师 17 级 |
| 13 | 灵魂火符 | 2 | 18/21/24 | 5 | 2 | 60 | 372 | ✅ 道士 18 级 |
| 14 | 幽灵盾 | 2 | 22/24/26 | 15 | 0 | 40 | 376 | ✅ 道士 22 级（P2，效果体已有） |
| 15 | 神圣战甲术 | 2 | 25/27/29 | 15 | 0 | 40 | 377 | ✅ 道士 25 级（P2，效果体已有） |
| 16 | 困魔咒 | 2 | 28/30/32 | 10 | 5 | 50 | 379 | ✅ 道士 28 级 |
| 18 | 隐身术 | 2 | 20/23/26 | 5 | 0 | 50 | 374 | ✅ 道士 20 级 |
| 19 | 集体隐身术 | 2 | 21/25/29 | 10 | 0 | 50 | 375 | ✅ 道士 21 级 |
| 22 | 火墙 | 1 | 24/29/33 | 20 | 25 | 120 | 390 | ✅ 法师 24 级 |
| 23 | 爆裂火焰 | 1 | 22/27/31 | 15 | 10 | 60 | 389 | ✅ 法师 22 级 |
| 24 | 地狱雷光 | 1 | 30/32/34 | 35 | 20 | 60 | 392 | ✅ 法师 30 级 |
| 28 | 心灵启示 | 2 | 26/30/35 | 16 | 0 | 40 | 378 | ✅ 道士 26 级 |
| 29 | 群体治疗术 | 2 | 33/35/38 | 12 | 25 | 40 | 380 | ✅ 道士 33 级 |
| 31 | 魔法盾 | 1 | 31/34/38 | 20 | 30 | 0 | 393 | ✅ 法师 31 级（P2，效果体已有） |
| 32 | 圣言术 | 1 | 32/35/39 | 50 | 40 | 120 | 394 | ✅ 法师 32 级 |
| 33 | 冰咆哮 | 1 | 35/37/40 | 12 | 30 | 60 | 395 | ✅ 法师 35 级 |
| 35 | —（无行；「乌龟跑步」属 39） | — | — | — | — | — | **无** | ❌ **魔表无 MagID 行 / 字段不可从 `magics` 取得**（`FindMagic` 返回 null ⇒ 学不到） |
| 36 | 火焰冰 | 1 | 48/50/52 | 14 | 10 | 60 | 557 | ✅ 法师 48 级 |
| 39 | 乌龟跑步 | 99 | 50/52/55 | 4 | 1 | 20 | **无** | ❌ **无同名技能书**（Job=99 本身不拦；`stditems` 无 StdMode=4 的「乌龟跑步」） |
| 40 | 十方斩 | 0 | 48/50/52 | 20 | 3 | 20 | 561 | ⚠️ 战士 48 级、服务端可达；**客户端触发待证** |
| 42 | 龙影剑法 | 0 | 38/41/44 | 0 | 3 | 0 | 501 | ⚠️ 战士 38 级、服务端可达；**客户端触发待证** |
| 43 | 开天斩 | 0 | 37/40/43 | 20 | 20 | 0 | 502 | ❌ 玩家施法走 `PlayObject.Attack.cs:1253` 显式 `case` ⇒ 不调 `DoSpell`（仅机器人 `AutoSpell` 可达） |
| 44 | 寒冰掌 | 1 | 36/39/41 | 14 | 8 | 60 | 508 | ✅ 法师 36 级 |
| 45 | 灭天火 | 1 | 38/41/44 | 15 | 15 | 60 | 509 | ✅ 法师 38 级 |
| 47 | 地狱烈焰 | 1 | 46/48/50 | 32 | 16 | 120 | 563 | ✅ 法师 46 级 |
| 48 | 群体施毒术 | 2 | 48/50/52 | 12 | 30 | 40 | 559 | ✅ 道士 48 级；书 559 按名字命中本行（`Idx=38`）⇒ 可学 |
| 48 | 气功波 | 2 | 36/39/41 | 8 | 0 | 30 | 503 | ✅ 道士 36 级；书 503 按名字命中本行（`Idx=48`），未学过 48 时可学 ⇒ **可达**（不能按 MagID 首行判遮蔽） |
| 50 | 无极真气 | 2 | 38/41/44 | 8 | 4 | 30 | 504 | ✅ 道士 38 级 |

> 注：35 与 39 在 oracle 中是两个不同的常量（35 `SKILL_WINDTEBO`、39 `SKILL_GROUPDEDING`），
> 但魔表里 **35 无行**、**39 无书**（唯一的「乌龟跑步」行属于 `MagID=39`；`MagID=35` 没有任何行），是数据/命名的历史错位。两者都不可达。
>
> 注（48）：`magics` 有两行 `MagID=48`（`Idx=38` 群体施毒术、`Idx=48` 气功波），但 `ReadBook` 按**书名**取行：书 559→`Idx=38` 行、书 503→`Idx=48` 行；两本都可学，因此 48 可达，行数据由读到的书决定。

### 2.3 可达性结论

- **服务端可达 29**：32 − 35 − 39 − 43。
- 其中 **26 个是 P3 效果体实现目标**（29 − P2 的 14/15/31）。
- `reachable_count = 29`；`p3_scope_count = 32`（`deferred_case_count = 28` + P2 3 + `ExplicitCase` 1）；`P3 actionable = 26`。
- 40/42 的「客户端触发」未证：MirClient（Delphi）不在本仓，`ClientSpellXY` 只能证明服务端在收到
  `CM_SPELL` 后会走 `default`→`DoSpell`。真机验收前必须用抓包/客户端行为确认；若客户端只走 hit-mode
  （`TrainCurrentSkill` 的 8/10），则应把它们从 P3 真机判据中移出（保留实现，不占窗口）。

---

## 3. 分批计划（5 批，每批 ≤6）

### 3.0 横切前置（所有批共用）

| 前置 | 内容 | 判级 | 为什么必须先做 |
|---|---|---|---|
| **P3-0** | 把 P1 现有内联共享尾块提成 `finish_spell_tail(ctx)`（宿主私有），输入 `bo_spell_fail/bo_spell_fire/bo_train/tx/ty/target_actor_id/magic/level/tran_point`；P1 的 A/B 两类改为调用它，**行为逐字节不变** | **L**（纯重构；但触碰 RNG 位置 ⇒ 审查按 R 记录窗口豁免需显式说明） | 5 批若各写一遍尾，必然在「取数位置/帧序/回写」上漂移 |
| **P3-1** | **目标 `AntiMagic`/`AntiPoison`** 注入面：`Entity` 无这两个字段，沿用 `World::cast_defence_area_spell(..., sc, ...)` 的「宿主传值」先例；不扩 `Entity` 字段。**SC/MC 不是缺口**：`handle_spell_cmd` 已从 `SessionGround.state.wabil` 取到宿主本地 `(mc, sc)` | R（接口） | B1/B4/B5 的目标 `AntiMagic`（B4 的 6/48 另需 `AntiPoison`）；B2/B3 的 2/29/16/18/19 读 SC、22/36 读 MC/SC，但宿主已有，见 §3.6 |
| **P3-2** | 延迟消息队列：把 `slave.rs::SelfDelayMsg` 泛化为可承载 `RM_DELAYMAGIC`/`RM_MAGHEALING`/`RM_TRANSPARENT`/`RM_DOOPENHEALTH`/`RM_POISON`/`RM_DELAYPUSHED`；出队按 `DeliveryTime` 稳定序 | R | 帧序判据（§4）必须可观察到延迟帧 |
| **P3-3** | `World` 内的有序目标枚举 `mag_objects_in_range(map, x, y, range)`：`x` 外层升序 → `y` 内层升序 → 格内 `ObjList` 插入序；过滤 `CellObjId>0 && ActorObject && !Death && !Ghost` | R | B2/B3/B4 的 RNG 顺序与帧顺序都依赖它 |
| **P3-4** | **真正缺口 = 延迟 RM 处理器**（`RM_DELAYMAGIC`/`RM_MAGHEALING`/`RM_POISON`/`RM_TRANSPARENT`/`RM_DOOPENHEALTH`/`RM_DELAYPUSHED`/`RM_MAKEHOLYSEIZEMODE`）与 `World` 状态落地，**不是新 SM 编码器**。可复用现状：`sm_spell`/`sm_magicfire`/`sm_magic_lvexp`/`sm_healthspellchanged`（53）/`sm_struck`（31，`RM_MAGSTRUCK` 直接复用）/`sm_ability`/`sm_durachange`（**正确拼写，不是 `sm_durachage`**）/`sm_magicfire_fail`（639）/`sm_openhealth`（1100，`RM_DOOPENHEALTH`→`RM_OPENHEALTH`）/`sm_charstatuschanged`（657，毒/透明状态）/`sm_716`（716，`RM_10205`）/`RM_PUSH`→`SM_BACKSTEP`（9） | L（处理器/状态落地，逐字段对照 oracle） | 现有 `smframe`/`smframe_ext` 已覆盖上述 SM 帧；缺口在延迟队列出队后的 RM 处理，不在编码器 |
| **P3-5** | `spell_arm`/`spell_tail_bo_train` 的 P3 扩展：把「case 内 `bo_train`/`bo_spell_fail`/`bo_spell_fire` 由效果体算出」作为唯一来源，禁止 `_ =>` 兜底 | L | 防止 5 批各自猜 `bo_train` |

> P3-0…P3-5 都是**独立小卡**；每批实现卡依赖它们（P3-0/P3-4/P3-5 可并行，P3-1/P3-2/P3-3 按批所需）。

### 3.1 B1 —— 单体直击 / 骰门家族（6 个）

| 项 | 内容 |
|---|---|
| 魔号 | **1、5、11、13、44、45** |
| oracle | `:174-203`（1/5）、`:289-314`（11）、`:315/328-351`（13）、`:557-562`（44）、`:563-588`（45） |
| 写集预计 | `handle_spell_cmd` 新增 5 条臂；`World::cast_single_target_bolt`（1/5/11/13/45 共用：`MagCanHitTarget`→`IsProperTarget`→`R(10)`→威力→`RM_DELAYMAGIC`）；`World::cast_hbfireball_spell`（44，含 `R(20)`/`R(Level)`/推）；`magic_effect` 新增 `bolt_gate`/`hbfireball_push` 纯函数；`RM_DELAYMAGIC`/`RM_DELAYPUSHED` 的处理器（帧面分别复用 `sm_struck`/既有 push→`SM_BACKSTEP`）；护身符门复用 `magic::check_amulet`/`use_amulet`（B1 的 13） |
| RNG 风险 | 每目标 1 或 5 抽（44 为 5–7）；`R(0)` 照样推进；`AP` 在 `Luck==0` 恰 2 抽。**13 的护身符门失败 ⇒ 零抽 + 639 + 无 `RM_MAGICFIRE`** |
| 帧风险 | `RM_DELAYMAGIC` 是 self-delay（600/1200），44 另有 `RM_DELAYPUSHED`（600）；共享尾 `RM_MAGICFIRE` 用 **case 后的** `tx/ty/target_id`（13 的 `MagCanHitTarget` 失败会把 target 置 null） |
| 判级 | **R** |
| 依赖未决模型 | **目标 AntiMagic 模型（硬前置）**；延迟队列（帧序）；`IsProperTarget`/`MagCanHitTarget` 的地图格语义（已有 `combat::is_proper_target` + `queryuser::cetin_near_xy`） |
| 备注 | 1/5 的 `R(10)` 失败分支把 `targetObject=null`（`:191`）；13 的 `R(10)` 失败**不**置 null——两处不同，不可合并 |

### 3.2 B2 —— 治疗 / 爆炸 AoE（6 个）

| 项 | 内容 |
|---|---|
| 魔号 | **2、29、23、24、33、47** |
| oracle | `:204-224`（2）、`:443-449`（29）、`:418-423`（23）、`:424-430`（24）、`:489-494`（33）、`:595-600`（47） |
| 写集预计 | `World::cast_healing_spell`（2）、`World::cast_big_healing_spell`（29）；`World::cast_big_explosion_spell`（23/33/47 共用，range 由调用方给）；`World::cast_elec_blizzard_spell`（24，中心=施法者坐标）；`mag_objects_in_range`；`RM_MAGHEALING`/`RM_ABILSEEHEALGAUGE`/`RM_MAGSTRUCK` 的处理器（帧面复用 `sm_instancehealguage`/`sm_struck`） |
| RNG 风险 | 2 命中友方后 4 抽（`MPow+GP+AP`），但 `IsProperFriend` 失败 ⇒ **0 抽**（`:211`）；29/23/24/33/47 固定 4 抽（`MagBigHealing`/`MagBigExplosion` 无抽）。治疗在 **HP 满时仍抽 4 次**（`:213` 在 HP 判断之前）⇒ 不能把「是否回血」当抽数门 |
| 帧风险 | `RM_MAGHEALING`→每个友方/800，但仅 `HP < MaxHP` 才发（2 在 `:214-216`、29 helper 在 `:64-66`；抽数在 HP 判断之前已发生）；`RM_MAGSTRUCK`→每个 proper target（`SendMsg` 即时，不延迟）；23/33/47 的 `MagBigExplosion` 用 `SetTargetCreat`（无帧）；24 对不死全额、其余 /10 |
| 判级 | **R** |
| 依赖未决模型 | 延迟队列（治疗帧）；有序目标枚举（23/24/29/33/47）；**不依赖 AntiMagic**；`ElecBlizzardRange`/`FireBoomRage`/`SnowWindRange` 配置项需装载（`SystemShare.Config`） |
| 备注 | 29 的 helper 是 3×3（range 1）；2 只对单目标（target 或自己）；23/33/47 的 `boTrain` 是「任一 proper target」 |

### 3.3 B3 —— 状态 / 辅助 / 不死（6 个）

| 项 | 内容 |
|---|---|
| 魔号 | **16、18、19、32、28、40** |
| oracle | `:318/366-371`（16）、`:320/378-383`（18）、`:321/384-389`（19）、`:480-488`（32）、`:431-442`（28）、`:495-523`（40） |
| 写集预计 | `World::cast_holy_curtain_spell`（16，含 MagicEvent/HolyCurtainEvent）；`World::cast_transparent_spell`（18 私有/19 群体）；`World::cast_turn_undead_spell`（32，含 `animal_struck` 抽数）；`World::cast_show_hp_spell`（28）；`World::cast_unamyounsul_spell`（40）；`RM_MAKEHOLYSEIZEMODE`/`RM_TRANSPARENT`/`RM_DOOPENHEALTH` 的处理器（`RM_DOOPENHEALTH`→既有 `sm_openhealth`；透明/HolySeize 走状态+657）；buff/state 字段（`hide_mode`/`transparent` 状态、`show_hp*`） |
| RNG 风险 | 16：`GP13+RP` + `R(4)×3×3 内 Animal`；18：`GP13+RP` + `R(2)×贴身合格怪（19×19）`；19：`GP13+RP`；32：`R(6)`（条件）`+R(300)`+`R(2)`+`R(100)`；28：`R(6)`+`RP+GP13`；40：`R(7)` |
| 帧风险 | 16 的 `result` 重置语义（§1.4-5）；19 的 `RM_TRANSPARENT`→友方/800；28 的 `RM_DOOPENHEALTH`→目标/1500；18：`RM_DURACHANGE`→自己 + `RM_CHARSTATUSCHANGED`(657)；40 无帧；32 无帧但改目标态 |
| 判级 | **R** |
| 依赖未决模型 | **buff/状态模型**（透明、HolySeize、ShowHp、三态中毒计时器）；延迟队列；有序目标枚举（16/18/19）；32 需要 `LA_UNDEAD`/`SuperMan`/`Animal` 与 `Abil.Level`；**不依赖 AntiMagic** |
| 备注 | 18 的 `MagMakePrivateTransparent`（`:686`）只在 `Race>=Animal && TargetCret==自己` 且 `|dx|<=1 && |dy|<=1` 时抽 `R(2)`（C# `||` 短路：任一坐标差 >1 直接 `TargetCret=null`，**不抽**）；以 §1.2 行 18 为准。19 只对 `IsProperFriend` 且未透明者发帧 |

### 3.4 B4 —— 群体列表 / 毒（4 个）

| 项 | 内容 |
|---|---|
| 魔号 | **6、48、50、42** |
| oracle | `:225-258`（6）、`:543-548`（48）、`:530-536`（50）、`:537-542`（42） |
| 写集预计 | `World::cast_poison_spell`（6，单目标）；`World::cast_group_poison_spell`（48，每目标消耗护身符）；`World::cast_mabe_spell`（50，多段骰）；`World::cast_group_lightening_spell`（42，先自帧再逐目标）；`RM_POISON`/`RM_DELAYMAGIC` 的处理器；`RM_10205`→既有 `sm_716`；`mag_objects_in_range` |
| RNG 风险 | 逐目标抽数（6 为单目标、不走枚举；48/50/42 按 `GetMapBaseObjects` 序）；48 每目标 **0–3**（无护身符/非 proper target ⇒ 0；过门 1；命中 2–3）；42 每目标 1 或 5；50 **4–10**（`MagCanHitTarget`/`IsProperTarget` 失败 4；`R(10)` 门失败 5；命中臂含 `GetMagStruckDamage` 的 `MSD` 抽，成功 9–10：玩家且非麻痹 10，否则 9）；**42 的 null 目标 NPE**（`:987`）必须在循环前显式判 |
| 帧风险 | 42 的 `RM_MAGICFIRE` 由 helper 自发（`boSpellFire=false`，尾不得再发）；48 的 `RM_POISON`→目标/1000；50 的两次 `RM_DELAYMAGIC`（600）+ `RM_POISON(STONE)`/650；39 的 `RM_10205` 广播（每对象无条件，`:975`）；42 的 `RM_10205` **有条件**——仅当 `baseObject.CurrX/Y != nTargetX/Y` 时发（`:1010-1012`），与 39 不同 |
| 判级 | **R** |
| 依赖未决模型 | **目标 AntiMagic/AntiPoison 模型（硬前置）**；目标枚举序；延迟队列；**42 的异常语义**（NPE → 吞异常 → 639，无尾）；48 的护身符逐目标消耗；50 的 `MabMabeHitRandRate`/`MabMabeHitSucessRate`/`MabMabeHitMinLvLimit`/`MabMabeHitMabeTimeRate` 配置项 |
| 备注 | 6 与 48 的绿/红毒由 `stdItem.Shape` 选 `GetPower13(40)` 或 `(30)`；48 的行由读到的书决定（书 559→`Idx=38` 群体施毒术 Spell=12/DefSpell=30/Delay=40；书 503→`Idx=48` 气功波 Spell=8/DefSpell=0/Delay=30），决定 `train_level`/威力，**不能按 MagID 首行解析** |

### 3.5 B5 —— 穿透 / 火墙 / 友方 buff（4 个）

| 项 | 内容 |
|---|---|
| 魔号 | **9、10、22、36** |
| oracle | `:265-276`（9）、`:277-288`（10）、`:412-417`（22）、`:610-630`（36） |
| 写集预计 | `World::cast_pass_through_spell`（9/10，12 格逐格）；`World::cast_fire_wall_spell`（22，FireBurnEvent×5）；`World::cast_uenhancer_spell`（36，友方+护身符+`AttPowerUp`）；`sm_struck`（复用，`RM_MAGSTRUCK`→`SM_STRUCK`=31）/`sm_durachange`（`RM_DURACHANGE`）/`sm_ability`（`RM_ABILITY`→`SM_ABILITY`=52）；`RM_DELAYMAGIC` 不涉及（B5 无，9/10 用 helper 帧） |
| RNG 风险 | 9/10：固定 4 + 逐格 `R(10)`（≤12）；22：5–6（`GP`/`RP` 顺序在 `power_fire_wall` 已钉）；36：`R(Level)`（Level=0 也抽）+3 |
| 帧风险 | 9/10：`RM_MAGSTRUCK`→命中者/600；22：无立即帧（事件）；36：`RM_DURACHANGE`→自己（`MagicBase.cs:119`）+`AttPowerUp` 的 `RM_ABILITY`→被 buff 目标（`PlayObject.cs:5329`）；9/10 会把 `nTargetX/nTargetY` 改为**穿透终点**（`:269`/`:281`）⇒ 尾 `RM_MAGICFIRE` 坐标是终点不是点击点 |
| 判级 | **R** |
| 依赖未决模型 | **目标 AntiMagic 模型（硬前置，9/10 逐格骰）**；地图格 `IsValidCell`/`GetNextPosition`；FireBurnEvent 事件表；护身符（36）；友方判定；延迟队列（36 的 `RM_DURACHANGE` 即时，但 `AttPowerUp` 的 buff 时序需登记） |
| 备注 | 22 的 `DisableInSafeZoneFireCross` 与安全区判定是 `boTrain` 的前置；9/10 的 `undeadAttack` 只在 helper 内乘 1.5，尾帧参数不受影响 |

### 3.6 批次依赖矩阵

| 批 | 目标 AntiMagic/AntiPoison | 延迟队列 | 有序目标枚举 | buff/状态 | 地图事件 | 护身符 | SC/MC（宿主已有） | 判级 |
|---|---|---|---|---|---|---|---|---|
| B1 | **必须**（AntiMagic） | 必须（帧序） | 否 | 否 | 否 | 13 | 13/44/45 读 SC | R |
| B2 | 否 | 必须（帧序） | 必须（23/24/29/33/47） | 否 | 否 | 否 | 2/29 读 SC | R |
| B3 | 否 | 必须（帧序） | 必须（16/18/19） | **必须** | 16 | 16/18/19 | 16/18/19 读 SC | R |
| B4 | **必须**（AntiMagic；6/48 另需 AntiPoison） | 必须（帧序） | **必须**（48/50/42；6 单目标不走枚举） | 毒/麻痹 | 否 | 6/48 | 6/48/50 读 SC；42 读 MC | R |
| B5 | **必须**（9/10 AntiMagic） | 必须（帧序） | 否 | 36 | 22 | 36 | 9/10/22/36 读 MC/SC | R |

> SC/MC 不构成前置：`handle_spell_cmd` 已由 `SessionGround.state.wabil` 提供宿主本地 `(mc, sc)`；矩阵末列只记录各批**使用**它，不是缺口。真正的注入缺口是目标 `AntiMagic`（B1/B4/B5）与 `AntiPoison`（B4 的 6/48）。

> 「必须等」的准确口径：
> - **目标 AntiMagic/AntiPoison 模型**没落地前，B1/B4/B5 **不得开工**（骰门决定抽不抽；先补帧只会把「缺 N 抽」变成「错位 N 抽」）。
> - **延迟队列**没落地前，B1–B5 可以做「帧集合」实现，但 **R 级验收的帧序牙必须等队列**；若先合，
>   `review` 写 `window_required: true` 并列出缺的队列项。
> - B2/B3 不需要 AntiMagic，可最早开工（但仍受 `finish_spell_tail`/帧编码器约束）。

---

## 4. 判据模板（每批统一）

### 4.1 统一模板 T-P3

所有用例走**生产入口** `handle_spell_cmd`（`CM_SPELL(3017)`），不得直调 `World::cast_*` 充数；
直调只用于 `magic_effect` 纯函数的单元牙。夹具沿用 `SpellFrameRig`/`a5_spell_rig` 的布法：
`world`（64×64）+ 在线登记/会话/gate 通道 + `magic_table` + `saves` + 观察者。

| 槽位 | 统一要求 |
|---|---|
| **同场景** | 施法者（Job/等级/`MagicList` 中该魔号 `level`/`tran_point` 已知）+ 目标（`race`、`life_attrib`、`anti_magic`、`death` 已知）+ 地图（可走/可掩码格明确）；目标在 `MagicAttackRage` 内 |
| **固定种子** | `world.rng = RandomNumber::with_seed(SEED)`（缺省 42 或显式 `20261017`）；对照流 = 同种子、不施法、只按**预期抽数**前进的第二个 `RandomNumber` |
| **连续抽** | 施法后从同一 `world.rng` 再取 **≥3** 个数，与对照流逐 k 比较；抽数不对齐时**先报「第 k 抽不一致」**，不要只报最终值 |
| **帧序** | 断言完整有序序列：`SM_HEALTHSPELLCHANGED(53)`（MP>0 时）→ `SM_SPELL(17)` 恰 1 → case 帧（0..N）→ `RM_MAGICFIRE(10120) → SM_MAGICFIRE(638)`（`boSpellFire` 为真时恰 1）→ `SM_MAGIC_LVEXP(640)`（仅训练尾命中时）；延迟帧在 `deliver_self_delay_msgs` 后断言 |
| **收件人** | `SM_SPELL`：AOI 内非 Ghost 观察者，**不含施法者自己**；`SM_MAGICFIRE`：`broadcast_ref_msg_with_self`，**含自己**；`RM_DELAYMAGIC`/`RM_MAGIC_LVEXP`：self-only；`RM_MAGSTRUCK`/`RM_MAGHEALING`/`RM_POISON`/`RM_TRANSPARENT`/`RM_DOOPENHEALTH`：目标-only；`RM_10205`：AOI 广播 |
| **落库** | 训练尾命中时 `saves[...].record.data.magic[mag_idx].level/tran_point` 必须等于 `do_spell_tail` 的 `train` 结果；`FastTrain=false` 登记 |
| **反例门** | 距离越界：零抽、无 `SM_SPELL`/无尾、恰 1 条 639、MP 已扣 + 53 已发（既有牙）；`boSpellFail`：无 `SM_MAGICFIRE`、无训练抽、恰 1 条 639；`MagicId>80`：`SM_SPELL` 1 条、`SM_MAGICFIRE` 0 条 |

### 4.2 每批的抽数/帧序「金标准」槽（实现时必须写进测试注释）

| 批 | 抽数（成功路径，`Luck=0`） | 帧序（成功路径） |
|---|---|---|
| B1 | 1/5：`R(10)` + `MPow+GP+AP` = 5；11/13/45 同；44：`R(10)`+4+`R(20)`+`R(Level)` = 7 | 53 → `SM_SPELL` → `RM_DELAYMAGIC`（self，延迟）→ `SM_MAGICFIRE` →（训练尾）640；44 在 `RM_DELAYMAGIC` 后加 `RM_DELAYPUSHED` |
| B2 | 2：4（成功路径；`IsProperFriend` 失败 ⇒ 0）；29/23/24/33/47：4；治疗 HP 满仍 4 | 53 → `SM_SPELL` →（`RM_MAGHEALING`/`RM_MAGSTRUCK`×N）→ `SM_MAGICFIRE` → 640 |
| B3 | 16：1–2+`R(4)`×3×3；18：1–2+`R(2)`×M；19：1–2；32：0–4；28：2–3；40：1 | 53 → `SM_SPELL` →（`RM_MAKEHOLYSEIZEMODE`/`RM_TRANSPARENT`/`RM_DOOPENHEALTH`；18 另 `RM_DURACHANGE`→self + `RM_CHARSTATUSCHANGED`）→ `SM_MAGICFIRE` → 640 |
| B4 | 6：2–3（成功路径；门失败 1；无护身符/非 proper target 0）；48：Σ(0–3)×N（无护身符 0；门失败 1；命中 2–3）；50：9–10（成功路径；整体 4–10；含 `MSD`）；42：Σ(1 或 5)×N | 53 → `SM_SPELL` →（42：helper 的 `SM_MAGICFIRE` **先于**逐目标 `RM_DELAYMAGIC`/`RM_10205`；其余 `RM_POISON`/`RM_DELAYMAGIC`）→（42 尾不再发）`SM_MAGICFIRE` → 640 |
| B5 | 9/10：4+`R(10)`×≤12；22：5–6；36：4 | 53 → `SM_SPELL` →（9/10：`RM_MAGSTRUCK`×N）→ `SM_MAGICFIRE` → 640（22 无 case 帧；36 `RM_DURACHANGE`→self 后 `RM_ABILITY`→目标） |

### 4.3 改坏必红（每批 ≥3 条 + 全局 3 条）

**全局（每批都跑）**

| # | 改坏什么 | 哪条断言红 |
|---|---|---|
| G1 | 删掉该批的 `finish_spell_tail` 调用（或 `bo_spell_fire=false` 误传） | `SM_MAGICFIRE` 计数 ≠ 期望（少 1/多 1） |
| G2 | 把尾抽 `Random(3)+1` 移到 case 抽数之前（或把尾整体前移到效果体之前） | 施法后连续 3 抽与对照流错位；`rng_trace` 站点顺序红 |
| G3 | 用 `broadcast_ref_msg` 代替 `broadcast_ref_msg_with_self` | 施法者自己缺 `SM_MAGICFIRE` |
| G4 | 目标死亡不过滤（`filter(!death)` 删除） | `SM_MAGICFIRE.ActorId` = 死者 id（应 0） |

**B1**

| # | 改坏什么 | 红 |
|---|---|---|
| B1-1 | 去掉 `R(10) >= AntiMagic` 门 | 高 `AntiMagic` 目标也命中；抽数少 1；伤害帧出现 |
| B1-2 | 交换 `MPow`/`GP` 求值顺序 | `rng_trace` 的 `Next(max)` 参数序错（值也可能错） |
| B1-3 | 13 的 `R(10)` 失败时把 target 置 null（照 1/5 写） | `SM_MAGICFIRE.ActorId` 错（应保留目标 id） |
| B1-4 | 44 漏 `R(20)`/`R(Level)` | 后续连续 3 抽全部错位 |

**B2**

| # | 改坏什么 | 红 |
|---|---|---|
| B2-1 | 治疗在 `HP==MaxHP` 时提前返回（不抽 4 次） | 连续 3 抽与对照流错位 |
| B2-2 | `MagBigExplosion` 无目标时仍置 `bo_train=true` | `TranPoint` 增加而 oracle 不增 |
| B2-3 | 24 对所有目标都用全额 `nPower`（漏 `/10`） | `RM_MAGSTRUCK` 的 `nParam2` 错 |

**B3**

| # | 改坏什么 | 红 |
|---|---|---|
| B3-1 | 16 的 helper 不保留「任一不合格 ⇒ result=0」 | `bo_train` 误真 ⇒ `TranPoint`/640 帧多出 |
| B3-2 | 18 漏 `R(2)`（只改状态不抽） | 连续 3 抽错位 |
| B3-3 | 32 对非不死目标仍抽 `R(2)`/`R(100)` | 抽数多 2 |
| B3-4 | 28 用 `GP13→RP` 顺序（反了） | `rng_trace` 参数序红 |

**B4**

| # | 改坏什么 | 红 |
|---|---|---|
| B4-1 | 目标枚举用 `HashMap`/实体表序代替 `GetMapBaseObjects` 序 | 逐目标抽数/帧序按目标颠倒 |
| B4-2 | 42 的 null 目标按 `ActorId=0` 处理（不抛） | 多发 1 条 `SM_MAGICFIRE`（oracle 是 639、无尾） |
| B4-3 | 48 按 MagID 首行取 `Idx=38`（群体施毒术）而非按读到的书取行（书 503 ⇒ `Idx=48` 气功波） | `train_level`/威力/`NeedL1` 错（气功波施法被按群体施毒术行算） |
| B4-4 | 48 只消耗一次护身符（不是每目标一次） | `RM_DURACHANGE` 计数/耐久错 |
| B4-5 | 50 命中臂漏掉 `GetMagStruckDamage` 的 `MSD` 抽（或把 `R(Level)` 当命中前抽） | 成功路径少 1 抽 ⇒ 后续连续 3 抽错位；玩家目标的麻痹 `R(Level)` 顺移 |

**B5**

| # | 改坏什么 | 红 |
|---|---|---|
| B5-1 | 9/10 的逐格循环次数与 `GetNextPosition` 不一致 | `R(10)` 次数错 ⇒ 连续抽错位 |
| B5-2 | 22 漏安全区门 | 安全区也落火 ⇒ `bo_train`/事件数错 |
| B5-3 | 36 在 `Level==0` 时跳过 `R(Level)` | 少 1 抽（`Random(0)` 仍推进） |
| B5-4 | 9/10 尾帧用点击坐标而非穿透终点 | `SM_MAGICFIRE` 的 `nParam1/nParam2` 错 |

---

## 5. 与既有卡的关系

| 卡 | 关系 | 边界 |
|---|---|---|
| `openmir2-spell-shared-tail-wiring` | **父卡**（本件是其 P3 设计件） | P3 只做效果体；共享尾/门/639 归父卡的 P0/P1/P2 |
| `openmir2-m3-attack-magic-wiring` | 拥有 `CM_SPELL`→`ClientSpellXY` 入口与已接效果体 | P3 只在 `handle_spell_cmd` 的 `default` 支内加臂；不得重写入口/MP 包装层 |
| `openmir2-buff-producer-wiring` | 拥有 14/15/31（P2）的 buff 生产者 | P2 是 P3 的**前置批**：先把 14/15/31 的尾/护身符门接完，再做 B2/B3（B3 的透明/ShowHp 也归 buff/状态面） |
| `openmir2-spell-frame-bypass-paths` | 拥有机器人 `AutoSpell` 与 `FireballMonster` 硬编码 `RM_SPELL(48)` 两条旁路 | B4 实现 48 时**不得**把怪物旁路当玩家路径；旁路仍未建模 ⇒ 「帧面缺口全清」结论不覆盖它 |
| `openmir2-default-arm-spell-surface` | 拥有无条件 `SM_SPELL` 帧面 | P3 必须保持「恰 1 条 `SM_SPELL`、先于 case 帧」；不得在效果体里再发 `sm_spell` |
| `openmir2-spell-distance-fail` / `openmir2-spell-dospell-fail-rest` | 拥有距离门/战士门/`boSpellFail` 的 639 | P3 的 `boSpellFail` 只返回 `false`，639 由既有单点发；**不得**在批内各发一次 |
| `openmir2-magic-draws-move` | 拥有 RNG 站点分组（`magic_effect` vs `attack`） | P3 新抽数必须落在 `crates/world/src/magic_effect.rs`（或新模块并登记组），**不得**落在 `magic.rs`（会被 `RUST_ATTACK_RE` 吞成 attack 组） |
| `openmir2-buff-system` | 拥有 buff 状态/下行 657 | B3 的透明/ShowHp/HolySeize 与 P2 的 31 需与 buff 状态字段对齐 |
| `openmir2-m3-pet-slave-summon` | 17/30/41/20/46 召唤面 | 不在 P3；但 P3 的共享尾函数被它们复用（P3-0 重构不得改其行为） |

---

## 6. 未决项与自失效声明

### 6.1 未决项

1. **40/42 的客户端触发**：`ReadBook` + `default` 支只能证明服务端可达；MirClient 不在本仓，
   无法证明客户端会为这两个魔号发 `CM_SPELL`（它们也是 hit-mode 8/10 的训练技能）。
   **解除**：抓包或客户端源码确认；若只走 hit-mode，则从真机 P3 判据移出（实现保留）。
2. **目标 AntiMagic / AntiPoison 的注入形态**：`Entity` 无 `anti_magic`/`anti_poison`；SC/MC 已由
   `handle_spell_cmd` 从 `SessionGround.state.wabil` 取到宿主本地 `(mc, sc)`，不是缺口。本件推荐照
   `cast_defence_area_spell` 的宿主传值，但这会让 `World::cast_*` 的签名变长；若改为扩 `Entity`，
   属 `crates/world` 接口变更，需通知并行车道（§12.4）。**未定前 B1/B4/B5 不开工**。
3. **延迟队列的承载**：`slave.rs::SelfDelayMsg` 只带 `ident/delay`，没有 `wParam/nParam1..3/Buff`；
   P3-2 需要扩展它或新建通用队列。**未定前只能做「帧集合」不能做「帧序」R 级验收**。
4. **目标枚举序**：`GetMapBaseObjects` 的 `ObjList` 插入序在 Rust 侧应对应哪个容器（实体表插入序/
   格子对象列表）尚未定；现有 `World::mag_make_defence_area` 用实体表 `iter()` 序，
   与 oracle 的格内 `ObjList` 序只在该格单对象时等价。**多目标夹具会暴露**。
5. **配置项**：`FireBoomRage`/`SnowWindRange`/`ElecBlizzardRange`/`MabMabeHit*`/
   `AmyOunsulPoint`/`MagTurnUndeadLevel` 的装载面未逐一核对；实现前按 `SystemShare.Config` 的 oracle 默认值钉死。
6. **`Random(0)` 的 .NET 行为**：本件按「`Next(0)` 仍调用 `Sample()` ⇒ 推进」记；
   本仓 `rng::next_max(0)` 同口径。若真机窗口发现 .NET 版本差异，以窗口实测为准（会改变全部抽数账本）。
7. **35/39 的「乌龟跑步」错位**：`sql/mir2_data.sql:107` 与在线 `magics` 都只有一行 `MagID=39 乌龟跑步`；`MagID=35` 无行、39 无书。若后续内容更新（加行/加书），
   可达性结论失效；本件按 2026-10-17 的部署表判定。

### 6.2 自失效声明

本件在以下任一条件变化时**整体失效**，必须重跑 §7 的命令并重写 §1/§2/§3：

- `MagicManager.cs` 的 `:145-671` 行号或内容变化（oracle 冻结线被触碰）；
- 部署 `mir2_data.magics` / `stditems` 的 32 行字段、行数、书行变化（含新增/删除/改名）；
- `sql/mir2_data.sql` 与在线表出现漂移（本件已验证 32 行一致）；
- `skills_registry.json` 的 `dospell_class`/`magic_id` 集合变化；
- 上游件 `design-spell-shared-tail-20261017.md` 的 DeferredCase 集合或 P1/P2 分期变化；
- P0/P1 已落地代码被回退，或 P2（14/15/31）先于本件落地导致 `spell_tail_bo_train`/尾函数重构；
- 新增并行车道修改 `bins/game-svr/src/main.rs::handle_spell_cmd` 或 `crates/world/src/world.rs` 的
  `cast_*_spell` 家族（写集冲突 ⇒ 本件的「写集预计」失效）。

---

## 7. 证据与复现命令（全部只读）

```sh
# ① 32 个 case 的起止（oracle；行号逐条实测）
awk 'NR>=172 && NR<=643 {printf "%d\t%s\n", NR, $0}' src/M2Server/Magic/MagicManager.cs

# ② 共享尾与前置门
awk 'NR>=145 && NR<=171 {printf "%d\t%s\n", NR, $0}' src/M2Server/Magic/MagicManager.cs
awk 'NR>=644 && NR<=671 {printf "%d\t%s\n", NR, $0}' src/M2Server/Magic/MagicManager.cs

# ③ 部署魔表（只读 SELECT；本件实际执行）
D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe -h127.0.0.1 -uroot --batch -e "
  SELECT Idx,MagID,MagName,Job,EffectType,Effect,Spell,Power,MaxPower,DefSpell,DefPower,DefMaxPower,
         NeedL1,L1Train,NeedL2,L2Train,NeedL3,L3Train,Delay
  FROM mir2_data.magics
  WHERE MagID IN (1,2,5,6,9,10,11,13,14,15,16,18,19,22,23,24,28,29,31,32,33,35,36,39,40,42,43,44,45,47,48,50)
  ORDER BY MagID, Idx;"

# ④ 技能书（只读 SELECT；按 MagName 精确匹配，StdMode=4）
D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe -h127.0.0.1 -uroot --batch -e "
  SELECT Id,Name,StdMode,Shape FROM mir2_data.stditems WHERE StdMode=4 ORDER BY Name;"

# ⑤ 在线表 vs SQL dump 的 32 行一致性（本件已比对；空输出 = 无差异）
#    两侧字段：Idx,MagID,Spell,DefSpell,Job,NeedL1,L1Train,NeedL2,L2Train,NeedL3,L3Train,Delay

# ⑥ 学技门（job/level/书）
awk 'NR>=2171 && NR<=2203 {printf "%d\t%s\n", NR, $0}' src/M2Server/Player/PlayObject.Base.cs
awk 'NR>=615 && NR<=645 {printf "%d\t%s\n", NR, $0}' src/M2Server/Player/PlayObject.Operate.cs
awk 'NR>=1559 && NR<=1570 {printf "%d\t%s\n", NR, $0}' src/GameSrv/Word/WorldServer.cs

# ⑦ 施法派发（8 条显式 case + default + DoSpell）
awk 'NR>=1110 && NR<=1290 && (/switch \(UserMagic.MagIdx\)/ || /case MagicConst/ ||
     /default:/ || /DoSpell/ || /RM_MAGICFIREFAIL/ || /CretInNearXy/)' \
  src/M2Server/Player/PlayObject.Attack.cs

# ⑧ helper 抽数/帧站点（逐条）
awk 'NR>=24 && NR<=133 {printf "%d\t%s\n", NR, $0}' src/M2Server/Magic/MagicManager.cs
awk 'NR>=673 && NR<=1354 {printf "%d\t%s\n", NR, $0}' src/M2Server/Magic/MagicManager.cs
awk 'NR>=2059 && NR<=2187 {printf "%d\t%s\n", NR, $0}' src/M2Server/Actor/BaseObject.cs
awk 'NR>=336 && NR<=359 {printf "%d\t%s\n", NR, $0}' src/M2Server/Actor/AnimalObject.cs
awk 'NR>=2909 && NR<=2952 {printf "%d\t%s\n", NR, $0}' src/M2Server/Player/PlayObject.cs

# ⑨ 技能分类（现状交叉核对；上游件口径）
python -c "import json;d=json.load(open('mir2-rs/tests/parity/m3/skill-probe/vectors/skills_registry.json',encoding='utf-8'));print(sorted({x['magic_id'] for x in d['skills'] if x['dospell_class']=='no_switch_case'}))"
```

**关键输出摘录**：

- `mir2_data.magics`：108 行 / 95 个唯一 `MagID`；32 行与 `sql/mir2_data.sql` 逐字段一致。
- 32 行中：`MagID=35` **无行**；`MagID=39` 有行（Job=99）但 `stditems` 无同名 StdMode=4 书；
  `MagID=43` 有书（Id=502）、但 `PlayObject.Attack.cs:1253` 显式 case；
  `MagID=48` 两行（`Idx=38` 群体施毒术、`Idx=48` 气功波）；`ReadBook` 按书名取行（书 559/503 各自可达），不能按 MagID 首行解析。
- `ReadBook` 门：`magic.Job == 99 || magic.Job == (byte)Job`（`:2179`）+ `Abil.Level >= TrainLevel[0]`（`:2181`）。
- 共享尾：`:644-647` `boSpellFail` 早退；`:648-658` `RM_MAGICFIRE`；`:659-669` 训练尾唯一 `R(3)+1`。
- 42 的 null 目标 NPE：`:984` 置 `boSpellFire=false`、`:987` 读 `targetObject.ActorId`；
  `PlayObject.Base.cs:1525-1529` 吞异常 ⇒ 调用方 639、无尾。

---

## 8. 残留物清单 / 只读声明

- 新增文件（唯一产物）：`mir2-rs/tests/parity/m3/design-spell-effect-bodies-p3-20261017.md`。
- worktree：首版 `/e/tmp/impl-p3plan`（`dce6642f4 --detach`，patch `/e/tmp/p3plan.patch`）；本修订版 `/e/tmp/impl-p3plan3`（`0d4b61840 --detach`，patch `/e/tmp/p3plan-fix2.patch`）。
- 未修改 `src/**`、`sql/**`、`MirClient/`、`crates/{protocol,storage}/**`、`E:/MirServer/**`；
  未启动任何服务/客户端/假人；未占用端口；未删除任何文件。
- 数据库：只对 `mir2_data.magics` / `mir2_data.stditems` 执行 SELECT（只读）；未 INSERT/UPDATE/DELETE/DDL。
- 只读打开过：`src/**`（oracle 源）、`sql/mir2_data.sql`、`mir2-rs/**`（Rust 现状与测试向量）、
  `E:/MirServer/M2GameSvr/Server.conf`（仅读取 DB 连接串）、`E:/MirServer/M2GameSvr/Envir/data.db`
  （仅 8 字节头识别，未解析、未复制）。
- 临时物：无（未写 `/e/tmp` 下的中间文件；本件内容直接落盘到 worktree 内）。
