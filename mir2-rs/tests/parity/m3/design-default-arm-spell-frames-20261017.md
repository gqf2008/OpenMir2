# 界定件：`DoSpell` default 支的 `RM_SPELL` 帧面缺口（`openmir2-default-arm-spell-surface`）

- **卡**：`openmir2-default-arm-spell-surface`。**本件只做界定（scoping），不实现任何发射逻辑**。
- **车道**：界定车道（无实现）。**worktree** `E:/tmp/impl-defaultarm` / branch `worktree-default-arm-scope` / HEAD `95eacf185`。
- **行号快照声明**：本件所有 `file:line` 为**写作时（`95eacf185`）快照**。落地方按**符号**定位；
  复核命令见 §7，若 grep 结果与本件不符即**源已漂移**。
- **纪律**：只读 oracle 与 Rust 源码；未跑 `cargo`、未起服务端/客户端、未占端口、未写数据库；
  未改任何既有文件（唯一新增 = 本 md）。
- **判级**：本件是**设计/界定件**，自身无判据执行；其中 §5 给的两级范围**预判**为 **① R（帧面/序）**、**② R（数值/落库）**，
  理由逐条见 §5。**本件不签发任何 `approve`／不宣称「已实现」。**
- **交叉核对对象**：`mir2-rs/tests/parity/m3/M3技能分派面审计与通用路径对齐报告-20261012.md`（下称「审计报告」）。

---

## 0. 结论速览

| # | 命题 | 机器判据 | 结果 |
| --- | --- | --- | --- |
| 1 | oracle `DoSpell` **在 `switch` 之前无条件**发 `RM_SPELL` | `MagicManager.cs:159` | **成立**（`:151` 战士门 / `:155` 距离门之后，`:167` 版本门**之前**）|
| 2 | oracle `switch` 有 **45 条**顶层 `case` | 45 个 `case MagicConst.*`（`:174`–`:640`，另 7 条属内层 `switch :326`）| **成立**（行号全表见 §1.1）|
| 3 | oracle 走 **default**（无 `case`）的魔号 | 魔表 108 行 − 45 `case` − 8 战士门 = **55 行 / 43 个不同魔号** | **成立**（§1.2）|
| 4 | Rust 侧 `RM_SPELL`（`sm_spell(`）发射点 | **恰 3 处块 / 5 个魔号：17、30、41、20、46** | **成立**（§2，`main.rs:4997/5025`、`:5146/5168`、`:5242/5264`）|
| 5 | **缺口**：走 default 且 Rust 不发 `RM_SPELL` | **55 行 / 43 个魔号**（其中 1 个 `56` 另有攻击路径分派器）| **成立**（§3）|
| 6 | 更宽的缺口（oracle 会发、Rust 不发） | 108 行 − 11（战士门 9 ∪ 外层 `switch (MagIdx)` 独立 case 8 的**并集**）− 5 已接 = **93 行 / 80 个魔号** | **成立**（§3.2；**2026-10-17 订正**：原写 95/82，漏扣外层独立 case `SKILL_43(43)` 与 `SKILL_REDBANWOL(56)`——这两号 `DoSpell` 根本不被调用 ⇒ 不发帧）|

> **一句话**：oracle 对**任何**非战士技能、在 `MagicAttackRage` 内的施法**都会广播 `RM_SPELL`**；
> Rust 目前只对 5 个魔号（17/20/30/41/46）广播，其余 **103 行**不发。其中 8 行是战士技能（oracle 也不发，无缺口），
> 剩下 **93 行**是**真实帧面缺口**（2026-10-17 订正：原写 95）。

---

## 1. oracle 侧清单（逐行号）

### 1.1 `MagicManager.DoSpell` 的 `switch` 全 case 表（45 条顶层，行号已自检）

`src/M2Server/Magic/MagicManager.cs`，`switch (userMagic.Magic.MagicId)` 起于 `:172`，体止于 `:643`
（`:643` 闭括号；共享尾 `:644`–`:670`，`return true` 在 `:670`）。

| # | 行号 | 常量名 | 值 | 内层 `switch :326` 亦覆盖 | 攻击路径 8 支之一（`PlayObject.Attack.cs:1118`）|
| --- | --- | --- | --- | --- | --- |
| 1 | 174 | `SKILL_FIREBALL` | 1 | | |
| 2 | 175 | `SKILL_FIREBALL2` | 5 | | |
| 3 | 204 | `SKILL_HEALLING` | 2 | | |
| 4 | 225 | `SKILL_AMYOUNSUL` | 6 | | |
| 5 | 259 | `SKILL_FIREWIND` | 8 | | |
| 6 | 265 | `SKILL_FIRE` | 9 | | |
| 7 | 277 | `SKILL_SHOOTLIGHTEN` | 10 | | |
| 8 | 289 | `SKILL_LIGHTENING` | 11 | | |
| 9 | 315 | `SKILL_FIRECHARM` | 13 | Y（`:328`）| |
| 10 | 316 | `SKILL_HANGMAJINBUB` | **14** | Y（`:352`）| |
| 11 | 317 | `SKILL_DEJIWONHO` | **15** | Y（`:359`）| |
| 12 | 318 | `SKILL_HOLYSHIELD` | 16 | Y（`:366`）| |
| 13 | 319 | `SKILL_SKELLETON` | **17** | Y（`:372`）| |
| 14 | 320 | `SKILL_CLOAK` | 18 | Y（`:378`）| |
| 15 | 321 | `SKILL_BIGCLOAK` | 19 | Y（`:384`）| |
| 16 | 394 | `SKILL_TAMMING` | **20** | | |
| 17 | 403 | `SKILL_SPACEMOVE` | 21 | | |
| 18 | 412 | `SKILL_EARTHFIRE` | 22 | | |
| 19 | 418 | `SKILL_FIREBOOM` | 23 | | |
| 20 | 424 | `SKILL_LIGHTFLOWER` | 24 | | |
| 21 | 431 | `SKILL_SHOWHP` | 28 | | |
| 22 | 443 | `SKILL_BIGHEALLING` | 29 | | |
| 23 | 450 | `SKILL_SINSU` | **30** | | |
| 24 | 462 | `SKILL_ANGEL` | **41** | | |
| 25 | 474 | `SKILL_SHIELD` | **31** | | |
| 26 | 480 | `SKILL_KILLUNDEAD` | 32 | | |
| 27 | 489 | `SKILL_SNOWWIND` | 33 | | |
| 28 | 495 | `SKILL_UNAMYOUNSUL` | 40 | | |
| 29 | 524 | `SKILL_WINDTEBO` | 35 | | |
| 30 | 530 | `SKILL_MABE` | 50 | | |
| 31 | 537 | `SKILL_GROUPLIGHTENING` | 42 | | |
| 32 | 543 | `SKILL_GROUPAMYOUNSUL` | 48 | | |
| 33 | 549 | `SKILL_GROUPDEDING` | 39 | | |
| 34 | 555 | `SKILL_43` | **43** | | Y（`Attack.cs:1253`）|
| 35 | 557 | `SKILL_44` | 44 | | |
| 36 | 563 | `SKILL_45` | 45 | | |
| 37 | 589 | `SKILL_46` | **46** | | |
| 38 | 595 | `SKILL_47` | 47 | | |
| 39 | 601 | `SKILL_ENERGYREPULSOR` | 37 | | |
| 40 | 607 | `SKILL_49` | 49 | | |
| 41 | 610 | `SKILL_UENHANCER` | 36 | | |
| 42 | 631 | `SKILL_51` | 51 | | |
| 43 | 634 | `SKILL_52` | 52 | | |
| 44 | 637 | `SKILL_53` | 53 | | |
| 45 | 640 | `SKILL_54` | 54 | | |

- **顶层 case 计数 = 45**（`case MagicConst.*` 标签共 **52** 条在 `:172`–`:643` 内；差 7 = 内层 `switch (:326)` 的
  `:328/352/359/366/372/378/384`，全部与 `:315`–`:321` 重复 ⇒ 不同魔号仍是 **45** 个）。
- **`switch` 内没有 `default:` 标签**——`default` 是**隐式落空**（无匹配即跳到 `:644` 共享尾）。
- 值表来源：`src/Modules/SystemModule/Const/MagicConst.cs`（60 条 `public const byte`）。

**8 条独立 `case`（攻击路径，另一分派器）**：`PlayObject.Attack.cs:1118` 的 `switch (UserMagic.MagIdx)`，
`case` 行号 `:1120`(12 `SKILL_ERGUM`) / `:1136`(25 `SKILL_BANWOL`) / `:1152`(**56** `SKILL_REDBANWOL`) /
`:1168`(26 `SKILL_FIRESWORD`) / `:1187`(27 `SKILL_MOOTEBO`) / `:1218`(34 `SKILL_CROSSMOON`) /
`:1234`(38 `SKILL_TWINBLADE`) / `:1253`(43 `SKILL_43`)；`default:` 在 `:1269`–`:1283`
（`:1270` 转向 → `:1278` 调 `DoSpell(...)` → 假则 `:1280` 发 `RM_MAGICFIREFAIL`）。

### 1.2 oracle 侧「走 default」的魔号全集

分派口径（三段互斥，按代码顺序）：

```text
MagicManager.cs:159   SendRefMsg(RM_SPELL, Magic.Effect, nTargetX, nTargetY, Magic.MagicId, "")   <- 无条件（过了战士门+距离门）
MagicManager.cs:167   (SoftVersionDateEx==0 && ClientTick==0) || MagicId > 80  => return false    <- >80 早退（switch 之前）
MagicManager.cs:172   switch (MagicId) { ...45 个 case... }                                      <- 无 case 者 = 隐式 default
MagicManager.cs:644   共享尾（boSpellFail / boSpellFire -> RM_MAGICFIRE / 训练计数）-> :670 return true
```

> **行号订正（本件实测）**：任务卡背景写「**:158-159** 无条件 `SendRefMsg(RM_SPELL,…)`」；`grep -n "SendRefMsg(Messages.RM_SPELL" ` 实测该**语句**在 **`:159`**，`:158` 是上一行（距离门 `:155` 的闭括号 `}`）——`:158-159` 是「块尾+语句」的区间写法，本件统一写 `:159`。

**A. 字面 default（`<=80`，到达 `switch` 但无 `case`）— 18 个不同魔号 / 20 行**

| 魔号（值）| 行数 | oracle 落点 |
| --- | --- | --- |
| 55、57、58、59、66、67、68、69、71、72、74、75、76、77、78、79、80 | 19 行（17 个魔号：`66`/`71` 各 2 行，其余各 1）| `:172` 落空 ⇒ 共享尾 `:644`–`:669`，`:670` `return true` |
| **56** `SKILL_REDBANWOL` | 1 行 | 同上（**但另见 A'：攻击路径有独立 `case`**）|

> **行数 ≠ 不同魔号数**：本桶 **20 行 / 18 个不同魔号**（`66`、`71` 各占 2 行；`56` 只占 1 行）。
> `56` 在魔表内**只有 1 行**（该号不跨桶）。行集见 §7 命令 `C3`。

**B. `switch` 之前早退（`>80`）— 25 个不同魔号 / 35 行**

| 魔号（值）| 行数 | oracle 落点 |
| --- | --- | --- |
| 81–103（连续 23 个）、500、501 | 35 | `:167` 早退 `return false`。**但 `:159` 的 `RM_SPELL` 已发出** |

**A ∪ B = 43 个不同魔号 / 55 行**（= 魔表 108 行 − 45 行有 `case` − 8 行战士门）。

**A' — 扣掉 56 的「无第二分派器」口径**：`56 SKILL_REDBANWOL` 在 `PlayObject.Attack.cs:1152` 有独立 `case`
（`RedHalfMoonOnOff` 开关 + `SendSocket("+WID"/"+UWID")`），**其效果路径不在 `DoSpell`**。
⇒ 「在 `M2Server` 内**无任何 case**」的魔号 = **42 个 / 54 行**（与审计报告 §2.3 的「其余 54 个魔号」同数，见 §4）。

---

## 2. Rust 侧清单（`sm_spell(` 发射点，逐处读条件）

`grep -n "sm_spell(" mir2-rs/bins/game-svr/src/main.rs` ⇒ **恰 3 处调用**（生产路径；其余命中是 `smframe.rs` 的定义/单测与注释）。

| 块 | 条件行 | **条件读法（覆盖魔号）** | 调用行 | 帧参 |
| --- | --- | --- | --- | --- |
| ① 召唤面 | `:4997` `if matches!(` / `:4998` `req.magic_id,` / `:4999` `mir2_world::slave::SKILL_SKELLETON` / `:5000` `SKILL_SINSU` / `:5001` `SKILL_ANGEL` | **17 ∨ 30 ∨ 41** | `:5025` | `sm_spell(actor_id, wire_coord(tx), wire_coord(ty), magic.effect, req.magic_id)` |
| ② 诱惑面 | `:5146` `if req.magic_id == mir2_world::slave::SKILL_TAMMING` | **20** | `:5168` | 同上 |
| ③ 分身面 | `:5242` `if req.magic_id == mir2_world::slave::SKILL_46` | **46** | `:5264` | 同上 |

- 常量值：`mir2-rs/crates/world/src/slave.rs:72`(`SKILL_SKELLETON=17`) / `:76`(`SKILL_SINSU=30`) /
  `:78`(`SKILL_ANGEL=41`) / `:74`(`SKILL_TAMMING=20`) / `:80`(`SKILL_46=46`)。
- **Rust 覆盖的魔号集合 = {17, 20, 30, 41, 46}，共 5 个**（全部属魔表的 `context_required` 桶，**全部有 oracle `case`**）。
- 三处块**都另有** `magic::spell_in_range(...)` 距离门（`:5017`/`:5160`/`:5256` 附近）复刻 `MagicManager.cs:155`；
  ⇒ 与 oracle 一致：**距离门之外不发帧**。
- 帧编码：`smframe::sm_spell`（`mir2-rs/bins/game-svr/src/smframe.rs:690-693`）
  = `command_header(actor_id, SM_SPELL, x, y, u16::from(effect))` + 尾 `magic_id.to_string()` 字节
  （对应 oracle `PlayObject.Message.cs:1457-1458`：`MakeMessage(SM_SPELL, ActorId, nParam1, nParam2, wParam)`
  + `SendSocket(msg, processMsg.nParam3.ToString())`）。
- 广播集合：`broadcast_ref_msg`（`main.rs:5454`）— **不含施法者自己**
  （oracle `:1455` `if (processMsg.ActorId != ActorId)`）；范围 = ±`SendRefMsgRange` 内 `Race==Play && !Ghost`。
- 其余魔号：`handle_spell_cmd`（`main.rs:4928`）走守卫 `magicwire::spell_reject`（`:4976`）与
  `takes_default_dir`（`:4991`，只有**转向**，不发帧）后**直接结束**——**不发 `RM_SPELL`**。

---

## 3. 差集

### 3.1 窄口径（本卡字面对象）：走 default 支 **且** Rust 无 `RM_SPELL`

**缺口 = 43 个魔号 / 55 行**（Rust 覆盖的 5 个魔号**全部**在 `case` 侧，与 default 集**不相交** ⇒ 差集 = default 集本身）。

| 魔号 | oracle case 行号 | 为什么是 default | Rust 为何不发 | 现状说明 |
| --- | --- | --- | --- | --- |
| **55** | **无**（`:172`–`:643` 内无 `case MagicConst.SKILL_55`）| 落 `:644` 共享尾 → `:670 return true` | `handle_spell_cmd` 无对应块 | 审计桶 `no_switch_case`；魔表 1 行 |
| **57、58、59** | **无** | 同上 | 同上 | 同上 |
| **66、67、68、69、71、72、74、75、76、77、78、79、80** | **无** | 同上 | 同上 | 同上；`66`/`71` 各 2 行 |
| **56** | **无**（`DoSpell` 内）| 同上（`DoSpell` 落空）| 同上 | **例外**：`PlayObject.Attack.cs:1152` 有 `case` ⇒ 效果路径在攻击路径；审计桶 `no_switch_case` 单行特判 |
| **81–103、500、501** | **无**（`:167` 早退，**未到达 `switch`**）| `MagicId > 80` ⇒ `:167 return false` | 同上 | 审计桶 `magicid_reject`；**MP 已扣、`RM_SPELL` 已发**（见 §5.2）|

**窄口径计数三写**（口径务必写清，勿混用）：
- 含 56：**43 个不同魔号 / 55 行**；
- 不含 56（「无第二分派器」）：**42 个不同魔号 / 54 行**；
- 只算「到达 `switch` 且落空」的字面 default：**18 个不同魔号 / 20 行**。

### 3.2 宽口径（「只补帧」片实际要覆盖的面）

oracle **会发 `RM_SPELL`** 的魔表行 = 108 − **11** = **98 行 / 85 个不同魔号**。被扣的 11 = 两个集合并：
① 战士门（`MagicManager.cs:151` `IsWarrSkill` 早退）= `{3,4,7,12,25,26,27,34,38}`（`MagicManager.cs:82-100`，**9 值**）；
② 外层 `switch (UserMagic.MagIdx)`（`PlayObject.Attack.cs:1118`）的独立 case = `{12,25,26,27,34,38,43,56}`
（行号 `:1120/1136/1152/1168/1187/1218/1234/1253`）——**这两号 `DoSpell` 不被调用 ⇒ 不发帧**。
⇒ 并集 = `{3,4,7,12,25,26,27,34,38,43,56}`（11 值）。Rust 已发 5 ⇒ **宽缺口 = 93 行 / 80 个不同魔号**。
> **2026-10-17 订正**：本段原按「108 − 8 战士 = 100 ⇒ 缺口 95/82」计算，**漏扣 ②**（`56` 虽在 §1.2 A' 被点明「效果路径不在 DoSpell」，此处却未扣；`43` 全文未提）。订正后 98/85、缺口 93/80，与实现侧口径一致（实现条件 = 不进外层 default ∧ 非战士 ⇒ 集合逐 id 相等）。

> **对「只补帧」片的建议范围**：以宽口径 **93 行 / 80 个魔号**为准（**帧面与是否有 `case` 无关**——
> oracle `:159` 在 `switch` 之前，`case` 只决定效果体）。
> 若只补窄口径（**内层 `switch` 无该魔号 case 且会发**）= **54 行 / 42 个魔号**，则 **44 行 / 43 个魔号**
> （内层**有** `case`、Rust 未接者）**仍不发帧**，真机对拍仍会在这些魔号上出现帧缺失。
> ⇒ **编排侧已裁决：按宽口径做**（卡上 comment `e6d8e835`）。

**宽口径 80 个不同魔号** = fire 85 − 已接 5；fire 85 = 魔表 95 − (战士 9 ∪ 外层独立 case 8) 的并集 11。逐 id 复算见 §7 命令 `C5`（已订正）。

---

## 4. 与审计报告的分类交叉核对

对象：`mir2-rs/tests/parity/m3/M3技能分派面审计与通用路径对齐报告-20261012.md`（base `ee4de412`）。

| 项 | 审计报告 | 本件 | 关系 |
| --- | --- | --- | --- |
| 分派站点 | `MagicManager.cs:172`（体至 `:643`）「45 case」（§2.1 表）| 同（§1.1 逐行复核 45 条，含内层 7 条 = 标签 52）| **一致**（行号逐条对得上）|
| 桶计数 | `implemented 29 / context_required 10 / no_effect_case 6 / no_switch_case 20 / warr_reject 8 / magicid_reject 35`（§3.3）| 直读 `skills_registry.json`（`total_magic_rows=108`）复算**逐桶同数** | **一致** |
| `no_switch_case` | 「20 行（55–80）」，其中 **56** 单行特判走攻击路径（§2.3，`:1152`）| 同：20 行；不同魔号 **18**（含 56）| **补充**（本件补出：**行数 20 ≠ 不同魔号 18**；不同值 = `55,56,57,58,59,66,67,68,69,71,72,74,75,76,77,78,79,80`）|
| `magicid_reject` | 「35 行」，「`:167` 版本门 `MagicId > 80` 直接 `return false`」；「**MP 已扣、`RM_SPELL` 已发**」（§2.2 关键事实）| 同 35 行 / 不同魔号 **25**（`81–103,500,501`）| **补充**（补齐不同值清单 + 行号）|
| 「其余 54 个魔号在 M2Server 内没有任何 case」 | 20 + 35 − 1(56) = 54 **行** | 同数（54 **行**）⇒ 本件区分**行数 54 / 不同魔号 42** | **一致（口径澄清，非冲突）** |
| 通用路径三发帧 | `:159 RM_SPELL` → `:648 RM_MAGICFIRE`；`magicid_reject` 出 2 帧、`no_switch_case` 出 3 帧（§2.2 表）| 本件 §5.2 逐行号重列（`:159`/`:644`–`:669`）| **一致** |
| `no_effect_case` 6 行 = `43,49,51,52,53,54` | §3.3 桶计数 6 | 复算同 6 行，`case` 行号 `:555/607/631/634/637/640` | **一致** |
| 8 条独立 `case` | `EXPLICIT_CASE_MAG_IDX = [12,25,26,27,34,38,43,56]`（`magicwire.rs:31`；`Attack.cs` 行号 `:1120/1136/1152/1168/1187/1218/1234/1253`）| 逐行核对同 8 值 / 同行号 | **一致** |
| `warr_reject` 8 值 | 本件复算 = `3,4,7,12,25,26,27,34`（`IsWarrSkill` `:82–:100`）| 同 | **一致** |

**冲突：无。** 一处**必须显式区分**的口径差（不是冲突，是三个数容易混）：
- 审计报告 §1 写「`no_switch_case` 20 行 …（55–80）」，§2.3 写「其余 19 个 … （55、57–80）」，两处**行数/枚举不一致**
  （`55–80` 是 26 个整数，而桶只有 20 行；`55、57–80` 是 25 个整数）。
  **机器事实**：桶内**不同魔号 = 18**（含 56）、**不含 56 = 17**；**行数 = 20**（含 56）、**不含 56 = 19**。
  即 §2.3 的「19」是**行数（不含 56）**、枚举串是**值区间误写**。⇒ 本件以 **`skills_registry.json` 直读**为准（§7 命令 `C3`）。
- 另：审计报告 **`warr_reject` 含 34 而 `IsWarrSkill` 不含 `SKILL_TWINBLADE(38)`**；`EXPLICIT_CASE_MAG_IDX` 含 38 不含 34；
  两者是不同集合（战士门 vs 攻击路径独立 `case`），**不可互相替换**——`12/25/26/27/34` 交集 5 个。

---

## 5. 两级范围（分开写）

### 5.1 ① 只补帧（无条件广播 `RM_SPELL`；效果体仍不建模）

**范围**：宽口径 **93 行 / 80 个魔号**（§3.2 订正后），或在 `MagicId <= 80` 内取字面 default + 有 `case` 未接者的并集。
**行为**：在 `handle_spell_cmd` 的 `magicwire::spell_reject` 守卫之后、`takes_default_dir` 附近，
按 oracle `:151`/`:155`/`:159` 的**同一顺序**插入无条件广播：

```text
(1) IsWarrSkill(MagIdx)            => 不发（MagicManager.cs:151，由攻击面承接）
(2) |dX|/|dY| > MagicAttackRage    => 不发（:155）
(3) SendRefMsg(RM_SPELL, Effect, nTargetX, nTargetY, MagicId, "")   => 无条件发（:159）  <<<< 本片
```

**可判判据（构造级，不占端口、不连 MySQL）**——与既有 `main.rs:15617` `spell_cmd_summon_wires_amulet_and_frames`
同夹具口径（`World::new(64,64)` + 施法者 + 观察者 + `OnlineRegistry` + `gates` 假 channel）：

| # | 断言 | 期望 |
| --- | --- | --- |
| J1 | 宿主 `handle_spell_cmd` 对**未建模魔号**（建议取 `55`/`81` 各一，覆盖 default 与 `>80` 两类）⇒ 观察者收到 **恰 1 帧 `SM_SPELL`** | `ident_of(b) == SM_SPELL` |
| J2 | 帧字段：`ActorId`=施法者、`nParam1`=`nTargetX`、`nParam2`=`nTargetY`、`wParam`=`Magic.Effect`、**尾 = `MagicId` 的 ASCII** | 逐字段等；**尾取 `MagicId` 而非 `Effect`**（红检点）|
| J3 | **施法者自己那连接不收 `SM_SPELL`**（oracle `:1455` 有 `ActorId != ActorId` 守卫）| `me` 里**无** `SM_SPELL` |
| J4 | **广播范围** = ±`SendRefMsgRange` 内、`Race==Play && !Ghost` 的**其它玩家**；同图远处观察者与 Ghost 观察者均**不收** | 距离外/Ghost 观察者帧表为空 |
| J5 | **距离门**：目标超 `MagicAttackRage` ⇒ J1 断言**必红**（即无 `SM_SPELL`）| 0 帧 |
| J6 | **顺序**：`SM_SPELL` 在 `SM_MAGICFIRE` **之前**出（oracle `:159` 早于 `:648`）——按每连接帧序断言 | 序号 `SM_SPELL` < `SM_MAGICFIRE` |
| J7 | **战士技能不发**：取 `warr_reject` 之一（如 12）⇒ 0 `SM_SPELL` | 0 帧 |
| J8（红检）| 去掉 `broadcast_ref_msg` / 把 `Effect` 换成 `MagicId` / 把范围改成全图 ⇒ J1/J2/J4 **必红** | 红 |

**tier 判定 = R**。理由（逐条）：
1. **帧序**：本片改变的是**已有帧流里插入新帧**——`SM_SPELL` 与 `SM_MAGICFIRE`／`SM_HEALTHSPELLCHANGED` 的**相对次序**
   是真机对拍判据（AGENTS.md「R 级 = 时序：tick 顺序、出队序、settle/延迟、竞态」）；
2. **广播收件人集合**：`SendRefMsgRange` 的 AOI 边界上，「谁收到/谁没收到」是**集合级差异**，构造级只能用假 channel 自证，
   **真实 AOI 边界**（跨格、同格多人、Ghost/隐身）只有真机可判；
3. **协议帧面**：这是给**冻结的 Delphi 客户端**看的帧 ⇒ 一旦字段/顺序错就是客户端表现差异（`SM_SPELL` 触发法术动画）；
4. **判不准 ⇒ 按 R**（AGENTS.md 判级纪律）。

**是否需要真机窗口：需要。** 最小窗口判据 = 同种子两侧对拍，构造一次「施法者 A 对 default 魔号施法 + 同图观察者 B」
（建议在现有 `E:\MirServer` 活栈的既有对拍场景里**加一个魔号**，不新起栈）。
**合并后卡片状态**：R 级 ⇒ `merge_result` 之后**保持 `in-progress`**（`work` 写明待窗口项），**不得 `closed`**。

### 5.2 ② 补效果体（MP 扣减／训练计数／失败支等）——**本卡不实现**

oracle 这段代码的副作用（逐行号）：

| 行 | 副作用 | 备注 |
| --- | --- | --- |
| `PlayObject.Base.cs:1510` | `if (!IsWarrSkill(MagIdx))` 才进 | 战士技能**连 MP 都不扣** |
| `PlayObject.Base.cs:1512` | `nSpellPoint = GetSpellPoint(userMagic)` | `Round(Spell/4*(Level+1)) + DefSpell` |
| `PlayObject.Base.cs:1513` | `if (nSpellPoint > 0)` | |
| `PlayObject.Base.cs:1515-1517` | **MP 不足 ⇒ `return false`**（**注意：`RM_SPELL` 尚未发**——`DoSpell` 还没被调）| 与 `:159` 的顺序关系是硬判据 |
| `PlayObject.Base.cs:1519` | **`DamageSpell(nSpellPoint)` ⇒ 扣 MP** | 落 `WAbil.MP` |
| `PlayObject.Base.cs:1520` | **`HealthSpellChanged()` ⇒ 发 `RM_HEALTHSPELLCHANGED`** | 自身帧 |
| `PlayObject.Base.cs:1522` | 进入 `MagicManager.DoSpell(...)` ⇒ 才有 `:159 RM_SPELL` | **扣 MP 早于发帧** |
| `MagicManager.cs:160-163` | `targetObject.Death ⇒ targetObject = null` | |
| `MagicManager.cs:167-168` | **版本门**：`(SoftVersionDateEx==0 && ClientTick==0) \|\| MagicId > 80` ⇒ **`return false`** | `magicid_reject` 35 行在此早退；**MP 已扣、`RM_SPELL` 已发** |
| `MagicManager.cs:174`–`:640` | 45 条 `case` 的**效果体**（伤害/召唤/护身符消耗/位移/…）| 各自另有写集 |
| `MagicManager.cs:644-647` | `boSpellFail ⇒ return false` | |
| `MagicManager.cs:648-658` | **`boSpellFire` ⇒ `SendRefMsg(RM_MAGICFIRE, 0, MakeWord(EffectType,Effect), MakeLong(x,y), targetId?, "")`** | `targetObject==null` 支 `:650` → `:652`；否则 `:656`。**自身也收**（`PlayObject.Message.cs:1788-1793` 无 `ActorId!=ActorId` 守卫）|
| `MagicManager.cs:659-668` | **训练计数尾**：`if (Level < 3 && boTrain)` → `:661` 条件门 → `:663` `TrainSkill(userMagic, Random(3)+1)` → `:664` `CheckMagicLevelUp` → `:666` `RM_MAGIC_LVEXP` | **含 RNG 抽数**（`Random(3)`）+ **落库**（熟练度回写 `MagicList`）|
| `MagicManager.cs:670` | `return true` | ⇒ 攻击路径 `:1278` 的 `if (!DoSpell(...))` 为假 ⇒ **不发 `RM_MAGICFIREFAIL`** |
| `PlayObject.Attack.cs:1280` | `DoSpell` 返回 false ⇒ **`SendRefMsg(RM_MAGICFIREFAIL, …)`** | 失败支；Rust 侧 `magicwire.rs:43-44` 已登记未接 ⇒ `SM_MAGICFIRE_FAIL(639)` |

**本卡边界（明确「不实现」）**：
1. **不实现** `PlayObject.Base.cs:1512-1520` 的 `GetSpellPoint`／`DamageSpell`／`HealthSpellChanged`
   （即**不扣 MP、不发 `RM_HEALTHSPELLCHANGED`**）——**注意**：这会让帧序列**少一帧**，
   与「只补 §5.1 的 `RM_SPELL`」叠加后，真机对拍仍会差 `SM_HEALTHSPELLCHANGED`。**这是范围外、需另开片或与本片同批登记。**
2. **不实现** `:648` `RM_MAGICFIRE` 的 `boSpellFire` 支（default 支下 `boSpellFire` 恒 `true` ⇒ 真机会多一帧 `SM_MAGICFIRE`）；
3. **不实现** `:659-668` 训练计数（含 `Random(3)` RNG 抽数与 `MagicList` 落库）；
4. **不实现** 45 条 `case` 的效果体（除已接的 17/20/30/41/46）；
5. **不实现** `:1280` `RM_MAGICFIREFAIL` 失败支。
⇒ 以上 5 条**全部登记**，本卡只登记与界定，**不下「已实现」结论**。

---

## 6. 风险段：若不实现会在真机对拍的哪些场景出帧序差

| # | 具体场景 | 期望（oracle）| 现状（Rust）| 差的表现 |
| --- | --- | --- | --- | --- |
| **S1** | 玩家施 **14 `SKILL_HANGMAJINBUB` / 15 `SKILL_DEJIWONHO` / 31 `SKILL_SHIELD`**（A-5 片刚接上 buff 效果，但**不发 `RM_SPELL`**）| 观察者收 **`SM_SPELL`**（法术动画）+ 共享尾 `SM_MAGICFIRE`；施法者自己收 `SM_MAGICFIRE`（+ `SM_HEALTHSPELLCHANGED`）| 只有效果体生效（buff 上了），**观察者一条 `SM_SPELL` 都没有** | **客户端看不到法术动画**（法术「无声生效」）；其余玩家看不到施法动作；帧列表**缺 1 帧且序号整体前移** ⇒ 逐帧对拍在第一个差异帧起**全错位** |
| **S2** | 玩家施任一 **default 魔号**（如 55 / 81）| `:159 SM_SPELL` → `:167 return false`（81）或 `:644` 共享尾（55）→ `SM_MAGICFIRE` | **0 帧**（`handle_spell_cmd` 直接返回）| 双向：Rust 侧**一帧不出**；若同时把「扣 MP」也做了，会出现**「蓝扣了、特效没有」**的不一致（客户端本地预判与服务器帧矛盾）|
| **S3** | 施法者在**同格有多名玩家**时施 default 魔号 | 全部非 Ghost 玩家各收 1 帧 `SM_SPELL` | 全 0 | 不是「少发」，是**广播集合整集缺失**；AOI 边界用例（跨格/隐身）全红 |

> 帧序差的性质：`SM_SPELL` 位于帧流**最前**（`HealthSpellChanged` 之后、`RM_MAGICFIRE` 之前），
> 缺它会**同时**造成「缺帧」与「后续帧序号前移」两类差异——这正是判 **R 级**的直接理由（§5.1）。

---

## 7. 自带复核命令（可复制；在**主检出** `E:\Users\gxh\Documents\GitHub\OpenMir2` 下跑，全部只读、无副作用）

```sh
# C1 -- oracle 侧：DoSpell 在 switch 之前无条件发 RM_SPELL，且 switch 无 default 标签
grep -n "SendRefMsg(Messages.RM_SPELL" src/M2Server/Magic/MagicManager.cs
#   期望：恰 1 行 = 159: playObject.SendRefMsg(Messages.RM_SPELL, userMagic.Magic.Effect, nTargetX, nTargetY, userMagic.Magic.MagicId, "");
awk 'NR==172 {print NR": "$0}' src/M2Server/Magic/MagicManager.cs
#   期望：172:             switch (userMagic.Magic.MagicId)   （内层同名 switch 在 :326，不参与本判据）
awk 'NR>=172 && NR<=643 && /default:/ {print NR": "$0}' src/M2Server/Magic/MagicManager.cs
#   期望：无输出  <-- switch 内没有 default 标签（default 是隐式落空）

# C2 -- oracle 侧：case 标签数与顶层 case 数
awk 'NR>=172 && NR<=643 && /case MagicConst\./' src/M2Server/Magic/MagicManager.cs | wc -l
#   期望：52
awk 'NR>=172 && NR<=643 && /case MagicConst\./ && NR!=328 && NR!=352 && NR!=359 && NR!=366 && NR!=372 && NR!=378 && NR!=384' \
    src/M2Server/Magic/MagicManager.cs | wc -l
#   期望：45   （52 减去内层 switch 的 7 条重复标签）

# C3 -- 独立复算 default 桶（直读注册表；不依赖本件任何数字）
python -c "import json,collections; d=json.load(open('mir2-rs/tests/parity/m3/skill-probe/vectors/skills_registry.json',encoding='utf-8')); b=collections.defaultdict(list); [b[s['dospell_class']].append(int(s['magic_id'])) for s in d['skills']]; nsc,mre=sorted(set(b['no_switch_case'])),sorted(set(b['magicid_reject'])); print('rows',len(b['no_switch_case']),len(b['magicid_reject']),'total',len(d['skills'])); print('nsc',len(nsc),nsc); print('mre',len(mre),mre); print('default_rows',len(b['no_switch_case'])+len(b['magicid_reject'])-1,'default_distinct',len(set(nsc+mre)-{56}))"
#   期望：rows 20 35 total 108
#         nsc 18 [55,56,57,58,59,66,67,68,69,71,72,74,75,76,77,78,79,80]
#         mre 25 [81,...,103,500,501]
#         default_rows 54 default_distinct 42

# C4 -- Rust 侧：sm_spell 生产调用恰 3 处 + 覆盖魔号
grep -n "sm_spell(" mir2-rs/bins/game-svr/src/main.rs
#   期望：恰 3 行 = 5025 / 5168 / 5264
grep -n "pub const SKILL_SKELLETON\|pub const SKILL_TAMMING\|pub const SKILL_SINSU\|pub const SKILL_ANGEL\|pub const SKILL_46" mir2-rs/crates/world/src/slave.rs
#   期望：72 -> 17 / 74 -> 20 / 76 -> 30 / 78 -> 41 / 80 -> 46
sed -n '4997,5001p;5146p;5242p' mir2-rs/bins/game-svr/src/main.rs
#   期望：4997 if matches!( / 4998 req.magic_id, / 4999 SKILL_SKELLETON / 5000 SKILL_SINSU / 5001 SKILL_ANGEL
#         5146 if req.magic_id == mir2_world::slave::SKILL_TAMMING {
#         5242 if req.magic_id == mir2_world::slave::SKILL_46 {

# C5 -- 独立复算宽缺口（oracle 会发帧 vs Rust 已发）
python -c "import json; d=json.load(open('mir2-rs/tests/parity/m3/skill-probe/vectors/skills_registry.json',encoding='utf-8')); rows=d['skills']; rust={17,20,30,41,46}; E={12,25,26,27,34,38,43,56}; W={3,4,7,12,25,26,27,34,38}; ids=[s['magic_id'] for s in rows]; fire=[i for i in ids if i not in E and i not in W]; gap=[i for i in fire if i not in rust]; inner=set(s['magic_id'] for s in rows if s.get('dospell_class') in ('implemented','context_required','no_effect_case')) ; print('rows',len(rows),'E',len(E),'W',len(W),'fire_rows',len(fire),'fire_distinct',len(set(fire)),'gap_rows',len(gap),'gap_distinct',len(set(gap)),'narrow_fire_rows',len([i for i in fire if i not in inner]))"
#   期望：rows 108 E 8 W 9 fire_rows 98 fire_distinct 85 gap_rows 93 gap_distinct 80
#   注：`E`=外层 switch(MagIdx) 独立 case、`W`=IsWarrSkill 9 值（并集 11）；`inner` 用 registry 三类近似内层有 case 者。
#   订正前本命令只减 `warr_reject`（registry 桶仅 8 值、且缺 38）⇒ 得 100/95/82（错）。

# C6 -- oracle 侧：8 条独立 case 的行号（攻击路径分派器）
grep -n "case MagicConst" src/M2Server/Player/PlayObject.Attack.cs
#   期望：1120(ERGUM=12) 1136(BANWOL=25) 1152(REDBANWOL=56) 1168(FIRESWORD=26)
#         1187(MOOTEBO=27) 1218(CROSSMOON=34) 1234(TWINBLADE=38) 1253(SKILL_43=43)

# C7 -- oracle 侧副作用行号（② 效果体，见 §5.2）
awk 'NR>=1505 && NR<=1531 {print NR": "$0}' src/M2Server/Player/PlayObject.Base.cs
#   期望：1510 IsWarrSkill 门 / 1512 GetSpellPoint / 1515-1517 MP 不足早退 / 1519 DamageSpell
#         / 1520 HealthSpellChanged / 1522 MagicManager.DoSpell
awk 'NR>=644 && NR<=670 {print NR": "$0}' src/M2Server/Magic/MagicManager.cs
#   期望：644-647 boSpellFail 支 / 648 boSpellFire / 652 与 656 各一条 RM_MAGICFIRE / 659-668 训练计数尾 / 670 return true

# C8 -- Rust 侧帧字段（尾 = MagicId 的 ASCII，不是 Effect）
sed -n '689,693p' mir2-rs/bins/game-svr/src/smframe.rs
#   期望：690 pub fn sm_spell(actor_id: i32, x: u16, y: u16, effect: u8, magic_id: u16) -> Vec<u8>
#         691 command_header(actor_id, messages::SM_SPELL, x, y, u16::from(effect))
#         692 gm_data_body(header, magic_id.to_string().as_bytes())
awk 'NR>=1454 && NR<=1460 {print NR": "$0}' src/M2Server/Player/PlayObject.Message.cs
#   期望：1455 if (processMsg.ActorId != ActorId)（自身守卫）/ 1457 MakeMessage(SM_SPELL, ...)
#         / 1458 SendSocket(ClientMsg, processMsg.nParam3.ToString())

# C9 -- oracle 侧版本门（>80 早退，但 RM_SPELL 已发）
awk 'NR>=165 && NR<=170 {print NR": "$0}' src/M2Server/Magic/MagicManager.cs
#   期望：167 (SoftVersionDateEx == 0 && ClientTick == 0) || userMagic.Magic.MagicId > 80
#         168 { / 169 return false; / 170 }
```

---

## 8. 残留物清单（§14.3）

- **仓内新增**：仅本件 `mir2-rs/tests/parity/m3/design-default-arm-spell-frames-20261017.md`。
- **仓内修改**：无。**未动** `mir2-rs/tests/parity/coverage/recon_gates.py`（另一条车道正在改；本卡不需要改门）。
- **冻结面无涉**：未动 `src/**`（C# oracle）、`sql/**`、`MirClient/`、`mir2-rs/crates/protocol`、`mir2-rs/crates/storage`。
- **仓内临时物**：无。**仓外**：无（本件未跑 `cargo`，未用 `E:/tmp/darm-target`）。
- **进程/端口**：未起服务端/客户端/假人；未占任何端口；`E:\MirServer` 基线未改。
- **未做（诚实声明）**：
  1. 未跑任何门禁（本件是纯界定件、无代码变更 ⇒ `cargo fmt/clippy/test` 不适用）；
  2. 未做真机对拍（§5.1 的 R 级窗口判据**待 owner 排窗口**）；
  3. **未实现任何发射逻辑**（本卡只登记与界定）；
  4. 未覆盖 `RobotPlayer`/`BotSrv` 的机器人选技路径（审计报告 §6.3 同口径）。

---

## 8. `DoSpell` 帧面的**两条旁路**（2026-10-17 登记；卡 `openmir2-spell-frame-bypass-paths`）

戏路之外的调用面（两条都**绕过**玩家 `default` 支那条链），本仓当前**不可达/未建模**，但必须入册，
以免将来接上时**静默漏帧**：

| # | oracle 位置 | 形态 | 与玩家路径的差别 | 本仓现状（机判） |
| --- | --- | --- | --- | --- |
| ① | `src/GameSrv/Robots/RobotPlayer.cs:1846`（派发）/ `:1856` `AutoSpell` | 直调 `MagicManager.DoSpell`，**绕过外层 `switch (UserMagic.MagIdx)`**（`PlayObject.Attack.cs:1118`） | ⇒ 对机器人而言，连 `SKILL_43(43)`/`SKILL_REDBANWOL(56)` 这类**独立 case** 魔号**也会**发 `RM_SPELL`（触发集合与玩家不同）；失败支发 `RM_MAGICFIREFAIL`（`:1848`） | **不可达**：`RobotProcessor` 未移植（`bins/game-svr/src/main.rs:7092` 已登记「本宿主不建机器人会话」） |
| ② | `src/M2Server/Monsters/*/FireballMonster.cs:39` | **硬编码** `SendRefMsg(RM_SPELL, 48, TargetCret.CurrX, TargetCret.CurrY, 48, "")`（不走 `DoSpell` 的通用发帧） | 怪物施法帧，与 `Effect/MagicId` 无关；随后 `:40` 另发 `RM_MAGICFIRE`、`:41` `SendStruckDelayMsg(RM_DELAYMAGIC,…)` | **未建模**：`mongen.rs:445-446` 只建模了该类 ctor 的抽数，**没有**该帧面 |

**解冻条件**：① 当「机器人会话 / `RobotProcessor`」被排进里程碑（或 `CM_*` 机器人面开工）时，
必须回来补「机器人路径的触发集合（= 不走外层 default 过滤）」这一条，并加构装级用例；
② 当「怪物施法帧」面开工（MagicMonster/FireballMonster 族 AI 接线）时，必须补该硬编码帧与
`RM_MAGICFIRE`/`DELAYMAGIC` 的帧序。⇒ 在此之前，任何「帧面缺口已全清」的结论都**不覆盖**这两条。

**登记锚**：本节即锚；`cards.json` 无需指派（这两条不对应 coverage 门的行）。
