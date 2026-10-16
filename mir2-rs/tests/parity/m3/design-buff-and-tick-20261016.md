# 设计件：buff 系统 / NPC Run / 世界时钟（Rust 侧口径，实现前必须先定）

- **车道**：S2（设计件，不含实现）。
- **worktree**：`E:/tmp/doc-buff`（detached HEAD = master `b08e065fc`），**只写本新文件**，未动任何既有文件。
- **原始数据**：`E:/tmp/spec/redlist2.json`（findings V4-03 / V4-08 / V3-11 + V4-09）、`E:/tmp/spec/redlist2-digest.txt`；
  旁证 `E:/tmp/spec/critic.json`（NL2 商人脚本 / NL4 MakeGhost；对 L2/L4/L5/L6 的证伪——**来源与本文档不同，引用时已标注**）。
- **行号快照声明（2026-10-16 对抗复核）**：本件所有 `file:line` 锚为**写作时（`b08e065fc`）快照**；此后 `main.rs`/`enter.rs`/`world.rs`
  因片101/103/106 变动**已漂移**。**落地前以符号（函数/字段名）为准，勿按行号直接定位**。已实测漂移（现址）：
  `main.rs:8310-8314`（`char_status: 0`）→ 现 `main.rs:8546`；`main.rs:5824`（切图 `game_time_now`）→ 现 `main.rs:6021`；
  `main.rs:8364`（进世界 `game_time_now`）→ 现 `main.rs:8596`；`world.rs:630-650`/`637`/`648`（`tick` 次序与插入点）→
  现 `world.rs:659`（`settle_deaths`）/`:680`（`monster_ai_tick`）/`:691`（`merchant_tick`，`tick` 首行 `:510`）；`enter.rs:203-215`（`game_time_from_hour`）→
  现 `:208-215`；`enter.rs:217-226`（`game_time_now`）→ 现 `:223-226`；`enter.rs:329-341`（`day_bright`）→ 现 `:332`；
  RNG 两件实体路径 = `crates/shared/src/rng.rs` / `crates/shared/src/rng_trace.rs`（**非** `crates/world/src/`）。
  `merchant.rs` 多数锚**未漂移**（`:547`/`:649`/`:696/697/702` 实测命中），但 §4.7 U-B4 引的 `merchant.rs:597` **已漂移**
  （丢预算口径现存于 `merchant.rs:526-545` 的 `merchant_tick` 文档第 3 条，`:532`）。`Merchant.cs:112-171/159/163/143-163` **未漂移**（实测命中）。
- **纪律**：只读 oracle 与 Rust 源码；未跑 cargo、未起服务、未占端口；LF 换行。
- **判级**：三件**全部 R**（理由逐条见 §2）。R 级未跑真机窗口前 `review` 不得 `closed`（AGENTS.md「判据分级」）。
- **订正说明（2026-10-16，S2-FIX 复核）**：本文档原 §1 逐字转写了原始 redlist2 JSON 的**错误标题**「`V4-08：SystemProcessor.ProcessNpcs`」——经复核 oracle，NPC 的 200ms 周期驱动实为 **`MerchantProcessor.cs:7`**（`:31` 调 `SystemShare.WorldEngine.ProcessNpcs()`）；`SystemProcessor.cs` 内**不存在** `ProcessNpcs`。全文已统一到 `MerchantProcessor`（§0 / §4.1.1 原即正确）。

---

## 0. 结论速览

| # | 断口 | oracle 锚（节奏 / 消费者） | Rust 现状 | tier | 先决依赖 |
| --- | --- | --- | --- | --- | --- |
| A | **buff 系统**（`ActorBuffSystem`）| `ActorBuffProcessor` 50ms → `DoWork`（`ActorBuffSystem.cs:58-91`）→ `Operate`（`:164-263`）| **全仓零对应物**；`GetCharStatus` 恒 0（`main.rs:8310-8314`）| **R**（战斗数值 + 状态位）| 无（可最先落）|
| B | **NPC Run**（`ProcessNpcs` 200ms）| `MerchantProcessor.cs:7` 200ms → `WorldServer.Merchant.cs:118-160` → `NormNpc.Run()` 家族 | 固定 NPC 实体**已在世界**，但 `Run()` **无 tick 驱动**（`monai.rs:861-874`、`merchant.rs` 双跳过）| **R**（共享 RNG 抽数序列）| 依赖 A 的 state 承载口径（同属 world 侧实体状态），但可独立先行 |
| C | **世界时钟**（`GetGameTime` + `GameTimeChanged`）| `SystemProcessor.cs:12/48/98-134`（50ms 刷 `M2Share.GameTime`）→ `WorldServer.cs:665-666` → `PlayObject.Base.cs:824-830` | `game_time_now()` 存在但**无周期调用者**；`Bright` 只在**进世界**发一次 | **R**（时序 / 时间门）| 依赖 sim 循环节奏口径（`clock_ms`，已具备）|

三件的共同形态：**oracle 有明确的「定时器线程 + 每 tick 逐条语义」，Rust 侧是「零对应物」或「零 tick」**。这不是「实现得不同」，而是**未建模轴**——按 T-45/T-49/T-51 体例属「未实现 ≠ 差异」，但一旦上线会在真机上以**战斗数值偏移（A）／大范围对拍漂移（B）／天色不变（C）** 暴露，故全部 R。（注：A 的 `StateTick` 实递减节奏「每 50ms 减 1 ⇒ 30s 防御 buff ≈1.5s」为**离线模型推导**、**待 A-R2 真机证实**，非已确立的 oracle 事实；见 §3.1.4 第 2 条。）

---

## 1. 覆盖与来源

本文档覆盖 redlist2 五条车道报告里的四条 finding（同一「周期 tick 无对应物」家族），合并为一件设计件：

- **V4-03**：`ActorBuffSystem.DoWork`（50ms：buff 到期 / 毒伤 DoT / CharStatus）全仓无对应实现与 tick（tier=R，confidence=high）。
- **V4-08**：`MerchantProcessor.ProcessNpcs`（200ms）— 固定 NPC 的 `Run()`（含 RNG 取数）无 tick 驱动（tier=R，confidence=medium）。**（原始 redlist2 JSON 标题作 `SystemProcessor.ProcessNpcs`，经复核有误：见文首订正说明）**
- **V3-11**：游戏时间变化重广播（`GameTimeChanged`/`RM_DAYCHANGING`）：Rust 只在进图发一次（tier=R，confidence=medium）。
- **V4-09**：`SystemProcessor.GetGameTime`（50ms 刷新 `M2Share.GameTime`）+ `PlayObject.Run` 的昼夜广播无 tick（tier=R，confidence=medium）。

> V3-11 与 V4-09 是同一件事的两半（**时钟推进** vs **变化广播**），本文档 §5 合并处理。

**旁证（来源 = `critic.json`，非 redlist2）**，与本文档相交但不混淆：

- **NL2**（商人脚本 `Market_Def` 未装载/未执行）：影响 **B 的可达面**——NPC 的 `Run()` 不依赖脚本，但固定 NPC 的 `Click/UserSelect` 交互链依赖 `Npc_def`/`Market_Def` 脚本。B 的 tick 实现**不**解除 NL2；两者互为独立缺口。
- **NL4**（怪物死亡幽灵 `MakeGhost`）：与 A/B/C 无关，但同属「`BaseObject.Run` 死亡支未建模」——**B 的 `Run` 载体若要接 `BaseObject.Run` 全段，会与 NL4 的 `MakeGhost` 支重叠**（见 §4.7 未定项 U-B3）。

---

## 2. 为什么全是 R 级

按 AGENTS.md「判据分级」的四条 R 判据逐条落位：

- **A（buff）落 ④ 战斗数值**：`Operate` 每 pass 调 `DamageHealth`（毒伤扣血，`ActorBuffSystem.cs:252`）与 `RecalcAbilitys`（防御/魔防倒计时恢复，`:228`）；`GetBuffStatus` → `CharStatus` → **每一帧的 `Status` 尾**（走/跑/转向/受击/死亡/骷髅…，`PlayObject.Message.cs:1409-2027`）。
  **不做真机全量对拍就不许放行**：数值列（`characters_ablity.Hp` 等）与逐帧 `Status` 差异都在此处。其中 `Operate` 的 `StateTick` 递减节奏（每 50ms 减 1）同属**离线模型推导**、**待 A-R2 真机证实**，非定案。
- **B（NPC Run）落 ① RNG 抽数序列**：`GuildOfficial.Run`/`Merchant.Run` 从**全服共享 `RandomNumber`** 抽数（§4.1）；Rust 不消耗这些抽数 ⇒ **共享流后续整体错位**。RNG 逐次流水（`site`/`order` 差）是 R 判据的明文反例。
- **C（世界时钟）落 ② 时序**：`M2Share.GameTime` 是**跨小时边界**的状态机，`GameTimeChanged` 只在**变化时**广播（`PlayObject.Base.cs:826` `if (Bright != M2Share.GameTime)`）。「何时发第 46 帧」是纯时序判据，且 `Bright` 进 `DayBright()`（`Base.cs:244-269`）影响每帧光照参数。
- 三件**不**触及 ③ 持久化落库（无 DB 写入），故 R 判据只落在 ①②④。

---

## 3. Part A — buff 系统

### 3.1 oracle 逐行实读

#### 3.1.1 数据结构（`src/M2Server/ActorBuffSystem.cs`）

| 元素 | 行 | 语义 |
| --- | --- | --- |
| `enum BuffType : byte` | `:10-48` | 12 值：`GreenPoison=0, RedPoison=1, Defense=2, CanNotRun=3, CanNotMove=4, Paralysis=5, ReduceHP=6, Frozen=7, Transparent=8, DefensePower=9, MagicDefensePower=10, MagicShield=11` |
| `ActorBuffMap` | `:55` | `Dictionary<int /*ActorId*/, IList<ActorBuffData>>` |
| `ActorBuffs` | `:56` | `IList<IActor>`——**有序**的「有 buff 的 actor」列表（操作序） |
| `class ActorBuffData` | `:339-350` | `ActorId:int`、`BuffType`、`StateVal:int`、`StateTick:int`、`StatusArrTick:int`、`DestroyTick:int` |

**索引口径（关键）**：buff 的「层数/序号」不是 `BuffType` 本身，而是 **`ActorBuffData` 在该 actor 的 `IList` 里的位置 `i`**（`GetBuffStatus`/`GetBuff` 都按 `i` 取值，见 §3.1.5）。

#### 3.1.2 `AddBuff`（`:100-159`）逐条

1. 若该 `ActorId` 已在 map：若 `buffs` 里**无**同 `BuffType` ⇒ `Add(new ActorBuffData{ StateVal=stateval, StateTick=duraTime, DestroyTick=GetTickCount()+40000 })`；
   否则**遍历**同 `BuffType` 的每一项（不是 first）⇒ `StateVal=stateval; StateTick=duraTime; DestroyTick=now+40000`。
2. 若该 `ActorId` **不在** map ⇒ `ActorBuffMap.Add(actorId, new List<ActorBuffData>{ buffState })`。
3. **无条件**：若 `ActorBuffs` 不含 `actor` ⇒ 追加。⇒ `ActorBuffs` 顺序 = **首次获得 buff 的时间序**。
4. **`StatusArrTick` 从不被初始化**（全仓唯一的赋值在 `:182` 的 `+= 1000`；`BaseObject.cs:1961/2158/2182`、`CharacterObject.cs:105`、`PlayObject.cs:1304` 的同类赋值**全被注释**）⇒ 新 buff 的 `StatusArrTick == 0`。**这决定 §3.1.4 的节奏，不是笔误**。

#### 3.1.3 `DoWork`（`:58-91`）逐条

1. `if (ActorBuffs.Any())` 才继续（空表 ⇒ 零动作）。
2. `Operate();`（**先**跑，见 §3.1.4）。
3. 扫 `ActorBuffs`：对每个 actor 取其 buff 列表，`for j` 里 `if (now >= actorBuffs[j].DestroyTick) actorBuffs.RemoveAt(j);`
   —— **⚠ `RemoveAt(j)` 后 `j++` 会跳过紧邻的下一个元素**（经典 off-by-one）：同一 pass 内连续两个到期的 buff 只删第一个，第二个留到下一 pass。
4. `if (actorBuffs != null && actorBuffs.Count == 0)` ⇒ `ActorBuffs.RemoveAt(idx); actorCount--;` —— 同样 `idx++` 会跳过移入 `idx` 的 actor。
   （`actorBuffs` 变量在循环外复用，此处读的是**上一次 TryGetValue 的残留值**——oracle 原样搬迁。）

#### 3.1.4 `Operate`（`:164-263`）逐条

**(a) 每 pass 的递减段（仅 `actor.Race == ActorRace.Play`，`Play == 0`）**

```
for j in actorBuffs:
    if (StateTick > 0) && (StateTick < 60000):
        if (GetTickCount() - StatusArrTick) > 1000:
            StateTick -= 1
            StatusArrTick += 1000
            if StateTick == 0:
                boChg = true
                switch BuffType:
                    DefensePower:      boNeedRecalc = true; SysMsg("防御力回复正常.", Green, Hint)
                    MagicDefensePower: boNeedRecalc = true; SysMsg("魔法防御力回复正常.", Green, Hint)
                    Transparent:       HideMode = false
            else if StateTick == 10:
                if BuffType == DefensePower:      SysMsg("防御力10秒后恢复正常。", ...); break   // 跳出 for(j)！
                if BuffType == MagicDefensePower: SysMsg("魔法防御力10秒后恢复正常。", ...); break // 跳出 for(j)！
```

**⚠ 三条必须 1:1 复刻的细节（第 2 条为待证模型，非已确立的 oracle 真值）：**

1. **`break` 跳出的是 `for (j)` 循环**（`:206`/`:211`），不是 `switch`——`StateTick==10` 且类型是 `DefensePower`/`MagicDefensePower` 时，本 pass **不再递减后续 buff**。这既改出帧（多打一条 SysMsg）也改递减节奏。
2. **`StateTick` 的递减节奏（离线模型推导，待窗口证实）**：本模型据代码推导为「每 DoWork pass（50ms）一次」、**非**「每 1s 一次」；此属**离线模型结论**，**不是已确立的 oracle 真值**，须真机窗口（A-R2）证实。推导（保留证据链）：`StatusArrTick` 初值 0，每 decrement 只 `+= 1000`；门 `(now - StatusArrTick) > 1000` 在第 `k` 个 pass（`now = T_c + 50k`，`StatusArrTick = 1000k`）为真，当且仅当 `T_c - 950k > 1000`，即 `k < (T_c - 1000)/950`。`T_c` = 该 buff 建立时的 `GetTickCount()`（**开机毫秒数**，`HUtil32.cs:36-39` = `Environment.TickCount`）。
   - 在 **前 ~`T_c/950` 个 pass** 门恒开 ⇒ `StateTick` **每 50ms 减 1**；
   - 该窗口时长 ≈ `50 × T_c/950 = T_c/19` 毫秒；`T_c = 100,000,000ms`（≈27.8h 开机）时 ≈ **87.7 分钟**。
   - 而任何合法 buff 的 `StateTick` 都 `≪ 60000`（`DefenceUp(nSec)`/`MagDefenceUp(nSec)`/透明/毒 的 `duraTime` = 秒数，几十~几百）⇒ **本模型给出实际时长 = `StateTick × 50ms`**（30 秒的防御 buff 模型上 ≈1.5s 回正常），**不是** `StateTick × 1s`。
   - ⇒ **redlist2 V4-03 原文「每 1s `StateTick-=1`」与本模型读数不一致**；**孰真只能由真机窗口判定**（§3.5 R 判据 A-R2）——本模型是**可检验的假设**，**不得写成定案**。
3. `if (actor.Race == Play)` 才跑递减段 ⇒ **怪物/动物（race>0）的 buff 永不到期递减**，只有毒伤段（(b)）对它们生效。这是刻意的（oracle 现状），必须照搬。

**收尾（同在 `for i` 体内，`:220-231`）**：

```
if (boChg)        { actor.CharStatus = actor.GetCharStatus(); actor.StatusChanged(); }
if (boNeedRecalc) { actor.RecalcAbilitys(); actor.SendMsg(RM_ABILITY, 0,0,0,0); }
```

**(b) 毒伤段（对所有 `!actor.Death` 的 actor，`:233-261`）**

```
if (!actor.Death):
    if (GetTickCount() - actor.PoisoningTick) > Config.PosionDecHealthTime:   // 缺省 2500ms
        actor.PoisoningTick = GetTickCount()
        for j in actorBuffs:
            if BuffType == GreenPoison && StateTick > 0:
                if (actor.Animal) ((AnimalObject)actor).MeatQuality -= 1000
                actor.DamageHealth(actor.GreenPoisoningPoint + 1)
                actor.HealthTick = 0
                actor.SpellTick = 0
                actor.HealthSpellChanged()
```

- **周期门 `PosionDecHealthTime` 缺省 2500ms**（`GameSvrConf.cs:893` 声明 / `:1766` 缺省 2500；`ServerConf.cs:544` 从 `[Setup] PosionDecHealthTime` 读）⇒ 毒伤**约每 2.5s 一跳**，与递减段（50ms pass）**不同节奏**。
- 扣血口径 = `DamageHealth(GreenPoisoningPoint + 1)`；`GreenPoisoningPoint = (byte)nPoint`（`MakePosion`，`BaseObject.cs:1949/1963`）。
- `DamageHealth`（`BaseObject.cs:775-818`）自身**含魔法盾抵扣，且外层有 `LastHiter` 守卫**：首行 `if ((LastHiter == null) || (LastHiter.Race == ActorRace.Play && !((IPlayerActor)LastHiter).UnMagicShield))`（`:777`）为真才进 MP 抵扣（`if (Race==Play && MagicShield && nDamage>0 && MP>0)` ⇒ `nSpdam = Round(nDamage*1.5)`，先扣 MP，余额再扣 HP）；`LastHiter` 为玩家且已开 `UnMagicShield` 时**跳过** MP 抵扣、直接扣 HP。⇒ **A 的 DoT 与 `MagicShield` 及 `LastHiter` 双双耦合**，设计必须把二者一并纳入。
- `HealthSpellChanged()`（`BaseObject.cs:700-708`）：`Race==Play` ⇒ `SendUpdateMsg(RM_HEALTHSPELLCHANGED)`；`ShowHp` ⇒ `SendRefMsg(...)` ⇒ 出帧 `SM_HEALTHSPELLCHANGED(53)`。

#### 3.1.5 `GetBuffStatus` / `GetBuff`（`:265-296`）

```csharp
public int GetBuffStatus(IActor actor) {
    int nStatus = 0;
    for i in actorBuffs:
        if (actorBuffs[i].StateTick > 0)
            nStatus |= (int)(0x80000000 >> i);      // i = 列表位置，不是 BuffType
    return nStatus;
}
public int GetBuff(IActor actor, BuffType t) {   // 返回 i+1（1-based 位置）
    for i: if (ActorId match && BuffType match) return i + 1;
    return 0;
}
```

- **位号 = 插入序 `i`**（`0x80000000 >> i`，最高位起）。第 0 个 buff → bit31，第 1 个 → bit30…（共 32 上限；`i≥32` 时 C# `>>32` 是 **shift & 31**，回绕到 bit31——oracle 未防，照搬风险低但须登记）。
- **`GetBuff` 返回 `i+1`**，被 `RecalcAbilitys` 用来叠加防御力（见 3.1.7）。
- 只有 `StateTick > 0` 的 buff 才置位 ⇒ 到 0 的 buff 仍留在列表（等 `DestroyTick` 40s 后删），但**不再贡献状态位**。

#### 3.1.6 与 `CharStatus` / 帧的关系

- `BaseObject.cs:57` `CharStatusEx`；`:1501` `GetCharStatus() => GetBuffStatus(this) | (CharStatusEx & 0x0000FFFF)`。
  ⇒ **低 16 位** = `CharStatusEx`（`STONEMODE=0x1`/`OPENHEATH=0x2`，`PoisonState.cs:53-54`）；**高 16 位** = buff 位。
- `StatusChanged()`（`BaseObject.cs:1503` 附近）= `SendRefMsg(RM_CHARSTATUSCHANGED(10139), 0, CharStatus, 0,0,"")` → `PlayObject.Message.cs:1845-1846` 出 `SM_CHARSTATUSCHANGED(657)`：
  `MakeMessage(SM_CHARSTATUSCHANGED, ActorId, LoWord(CharStatus), HiWord(CharStatus), HitSpeed)`。
- `CharStatus` 还作为**尾字段**被这些帧携带（全部走 `baseObject.CharStatus`，`PlayObject.Message.cs`）：
  走 `:1409`、骑跑 `:1419`、跑 `:1429`、走失败 `:1472`、转向 `:1547`、受击 `desc.Status` `:1604`、死亡 `:1648`、骷髅 `:1659`、复活 `:1888`、挖尸 `:1895`、显形 `:1979`、换脸 `:2027`。
  ⇒ buff 一变，**这些帧的尾字节全变**。这是 A 的最广可观测面。

#### 3.1.7 消费者与生产者全表

| 方向 | 站点 | 语义 |
| --- | --- | --- |
| 读 `GetBuff` | `BaseObject.Abil.cs:79` | `WAbil.AC` 高字节 `+ (Level/7) + GetBuff(DefensePower)`（`_MIN(255, ...)`）|
| 读 `GetBuff` | `BaseObject.Abil.cs:83` | `WAbil.MAC` 高字节 `+ (Level/7) + GetBuff(MagicDefensePower)` |
| 读 `GetBuffStatus` | `BaseObject.cs:1500` | `GetCharStatus()`（§3.1.6）|
| 写 `AddBuff` | `BaseObject.cs:1949` | `MakePosion(nType, nTime, nPoint)` → `(BuffType)nType`（毒/石化，`nType<MAX_STATUS_ATTRIBUTE`=12）|
| 写 `AddBuff` | `BaseObject.cs:2144` | `DefenceUp(nSec)`（先 `HasBuff` → `TryUpdate` 刷新，否则 Add）|
| 写 `AddBuff` | `BaseObject.cs:2168` | `MagDefenceUp(nSec)` |
| 写 `AddBuff` | `CharacterObject.cs:103` | `MagicShield`（`nSec`）|
| `HasBuff`/`TryUpdate` | `BaseObject.cs:2139/2141` | `DefenceUp` 的去重/刷新 |
| `MakePosion` 调用点 | `PlayObject.Attack.cs:223` | 玩家攻击 ⇒ `MakePosion(STONE, Config.AttackPosionTime, 0)` |
| `MakePosion` 调用点 | 怪物 7 处 | `CentipedeKingMonster.cs:70(:74)`、`GasAttackMonster.cs:33`、`GreenMonster.cs:25`、`Khazard.cs:32/46`、`RedMonster.cs:25`、`SpitSpider.cs:48` |

> ⇒ **A 的生产链（毒）与消费链（AC/MAC/状态位/帧）都在战斗路径上**，属 R④ 数值面。

#### 3.1.8 并存的第二套状态机制（`StatusTimeArr`）— 设计不能装作看不见

`BaseObject.cs:81` `public ushort[] StatusTimeArr { get; set; }`（`:394` `new ushort[15]`）是 **旧机制**，`ActorBuffSystem` 未完全接管。仍在**活跃**的读点：

- `DamageBubbleDefence`（`BaseObject.cs:2044-2057`）：`StatusTimeArr[BubbleDefenceUP] -= 3`（**不**通过 `ActorBuffSystem`）；
- `StatusTimeArr[STONE]`（`BaseObject.Base.cs:683/695/707`）；`StatusTimeArr[DAMAGEARMOR]`（`BaseObject.Abil.cs:852`）；`StatusTimeArr[STATETRANSPARENT]`（`Abil.cs:33/49`）。

⇒ 设计必须**显式声明**：先只建 `ActorBuffSystem` 一套，还是**两套并存**（含 `StatusTimeArr` 的 15 槽语义）。见未定项 **U-A1**。

#### 3.1.9 `PoisonState` ↔ `BuffType` 索引对齐（易错点）

`MakePosion(nType)` 用 `(BuffType)nType` 直接索引 ⇒ `PoisonState` 常量**按数值**映射到 `BuffType`：

| idx | `BuffType`（`ActorBuffSystem.cs:10-48`）| `PoisonState`（`PoisonState.cs`）| 命名是否一致 |
| --- | --- | --- | --- |
| 0 | GreenPoison | `DECHEALTH=0`（绿毒）| 一致 |
| 1 | RedPoison | `DAMAGEARMOR=1`（红毒）| 一致 |
| 2 | Defense | `LOCKSPELL=2` | 名不同（值同）|
| 3 | CanNotRun | `LOCKRUN=3` | 名不同 |
| 4 | CanNotMove | `DONTMOVE=4` | 名不同 |
| 5 | Paralysis | `STONE=5` | 名不同 |
| 6 | ReduceHP | （无 6；`POISON_68=68`）| 名不同 |
| 7 | Frozen | `FASTMOVE=7` | 名不同 |
| 8 | Transparent | `STATETRANSPARENT=8` | 一致 |
| 9 | DefensePower | `DefenceUP=9` | 一致 |
| 10 | MagicDefensePower | `MagDefenceUP=10` | 一致 |
| 11 | MagicShield | `BubbleDefenceUP=11` | 名不同 |

⇒ Rust 侧若把 `PoisonState` 常量与 `BuffType` 建成两张表，**必须按数值对齐**（`MAX_STATUS_ATTRIBUTE = 12`，`Grobal2.cs:39`）。**不要**按名字重排。

### 3.2 Rust 现状（file:line）

- **全仓 grep 0 命中**：`BuffType|AddBuff|PoisoningTick|GreenPoison|DestroyTick|StateTick`（`mir2-rs/**/*.rs`）⇒ 无 buff 表、无 50ms tick。
- `bins/game-svr/src/main.rs:8310-8314`：`char_status: 0`，注释自陈「会话初生无 buff、`CharStatusEx=0` ⇒ 恒 0（金标准 #3 印证）」。
- `bins/game-svr/src/enter.rs`（模块头注释）：帧 50/41 的 `CharStatus` buff 分量未移植，初生恒 0（已登记）。
- `crates/world/src/entity.rs`：**已有** `meat_quality: u16`（`:82`）、`hide_mode: bool`（`:302`）、`fixed_hide_mode`（`:303`）；**缺** `poisoning_tick`、`green_poisoning_point`、`char_status`、`char_status_ex`。
- `crates/world/src/combat.rs` 无 `DamageHealth` 的 buff 抵扣入口（魔法盾路径未接）。

### 3.3 Rust 建议模型（结构体 / 字段 / 函数签名）

**新增文件** `mir2-rs/crates/world/src/buff.rs`：

```rust
/// `ActorBuffSystem.cs:10-48` 的 1:1。
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuffType {
    GreenPoison = 0, RedPoison = 1, Defense = 2, CanNotRun = 3, CanNotMove = 4,
    Paralysis = 5, ReduceHP = 6, Frozen = 7, Transparent = 8,
    DefensePower = 9, MagicDefensePower = 10, MagicShield = 11,
}
pub const MAX_STATUS_ATTRIBUTE: u8 = 12;         // Grobal2.cs:39

/// `ActorBuffData`（:339-350）。`actor_id` 由 map 键承担，不入结构。
#[derive(Clone, Copy, Debug)]
pub struct ActorBuffData {
    pub buff_type: BuffType,     // BuffType
    pub state_val: i32,          // StateVal
    pub state_tick: i32,         // StateTick
    pub status_arr_tick: i64,    // StatusArrTick —— 初值必须为 0（见 §3.1.2 第 4 条）
    pub destroy_tick: i64,       // DestroyTick
}

/// `ActorBuffMap` + `ActorBuffs`（:55-56）。**两条秩序都要保留**：
/// - `map` 的 Vec 是 `GetBuffStatus` 的位序来源（插入序）；
/// - `actors` 是 `ActorBuffs` 的「首获 buff 序」，`Operate` 按它遍历。
#[derive(Default)]
pub struct BuffSystem {
    map: std::collections::HashMap<i32, Vec<ActorBuffData>>,
    actors: Vec<i32>,
}

impl BuffSystem {
    pub fn add_buff(&mut self, actor_id: i32, t: BuffType, dura: i32, stateval: i32, now_ms: i64);
    pub fn get_buff(&self, actor_id: i32, t: BuffType) -> i32;   // 返回 i+1
    pub fn has_buff(&self, actor_id: i32, t: BuffType) -> bool;
    pub fn try_update(&mut self, actor_id: i32, t: BuffType, dura: i32) -> bool;
    pub fn get_buff_status(&self, actor_id: i32) -> i32;         // 0x80000000 >> i
    /// `DoWork`：先 `operate`，再删 `DestroyTick` 到期（含 §3.1.3 的两处 off-by-one）。
    pub fn do_work(&mut self, world: &mut World);
    fn operate(&mut self, world: &mut World);
}
```

**`Entity` 增字段**（`crates/world/src/entity.rs`）：`poisoning_tick: i64`、`green_poisoning_point: u8`、`char_status: i32`、`char_status_ex: i32`。

**`World` 增**（`crates/world/src/world.rs`）：`pub buffs: BuffSystem`、`buff_pass_tick: i64`、`buff_enabled: bool`（缺省关，照 `merchant_enabled` 体例）；`World::tick` 在**死亡结算之后、怪物 AI 之前**插 `if self.buff_enabled { self.buff_tick(); }`（节奏门 `BUFF_PASS_INTERVAL_MS = 50`）。

**`DamageHealth` 等价物**（`combat.rs`）：新增 `pub fn damage_health(world: &mut World, actor_id: i32, n_damage: i32, last_hiter: Option<i32>)`，1:1 复刻 `BaseObject.cs:775-818`。**必须带 `last_hiter` 入参**：首行守卫 `last_hiter.is_none() || (last_hiter.race == Race::Play && !last_hiter.un_magic_shield)`（oracle `:777`）为真才进 MagicShield 的 MP 抵扣；否则跳过、直接扣 HP（`:796-817`）。**仅 `(world, actor_id, n_damage)` 的签名无法 1:1 复刻 oracle 的首行守卫。**

### 3.4 逐条对应表（oracle ↔ Rust）

| oracle | Rust 建议 | 备注 |
| --- | --- | --- |
| `ActorBuffProcessor` 50ms（`ActorBuffProcessor.cs:7`）| `World::buff_tick` + `BUFF_PASS_INTERVAL_MS=50` | 现有 `clock_ms` 步长 200ms ⇒ **50 不整除 200**：见 U-A2 |
| `DoWork`（`:58-91`）| `BuffSystem::do_work` | 含两处 `RemoveAt` off-by-one |
| `Operate` 递减段（`:168-231`）| `operate` 之 (a) 段 | 含 `break` 跳出 for(j) 的 quirk |
| `Operate` 毒伤段（`:233-261`）| `operate` 之 (b) 段 + `damage_health` | `PosionDecHealthTime` 缺省 2500 |
| `GetBuffStatus`（`:282-296`）| `BuffSystem::get_buff_status` | bit = 插入序 |
| `GetCharStatus`（`BaseObject.cs:1499-1503`）| `fn get_char_status(world, id) -> i32` | `| (char_status_ex & 0xFFFF)` |
| `StatusChanged`（`BaseObject.cs:1503`）| 出帧队列条目 `RM_CHARSTATUSCHANGED` | 归接线层（`main.rs`）|
| `AddBuff` 四调用点（§3.1.7）| `MakePosion`/`DefenceUp`/`MagDefenceUp`/`MagicShield` 等价物 | 毒生产链在 `magic.rs`/`combat.rs` |
| `Abil.cs:79/83` 的 AC/MAC 叠 buff | `ability::recalc_abilitys`（已合入）加 `+get_buff(...)` | 改既有函数，属 **A 的实现片** |
| `PosionDecHealthTime`（`GameSvrConf.cs:893/1766`）| `GameSvrConf` 对应字段 | 需确认 `crates/envir` 是否已解析该键 |

### 3.5 判据

**离线可判（构造级，不占窗口）**

- **A-L1（表语义）**：单测钉死 `add_buff` 的「无则加 / 有则刷」两臂、`destroy_tick = now+40000`、`status_arr_tick` 初值 0、`actors` 仅追加一次（重复 AddBuff 不重复入列）。
- **A-L2（位序）**：构造 3 个 buff（GreenPoison→DefensePower→Transparent）断言 `get_buff_status` 依次得 `0x80000000`、`0xC0000000`、`0xE0000000`（插入序）。
- **A-L3（off-by-one 复刻）**：同一 pass 造两个 `destroy_tick` 同时到期的 buff，断言**只删第一个**（第二个留下）——把 `for j` 改成安全的 `retain` 即红。
- **A-L4（毒伤节奏）**：给动物上 `GreenPoison(StateTick=3)`，推进 sim 时钟跨 `PosionDecHealthTime`，断言 `DamageHealth(GreenPoisoningPoint+1)` 命中次数与 `MeatQuality -= 1000` 次数（`Animal` 真值待定，见 U-A3）。
- **A-L5（递减节奏，纯构造断言）**：以 `T_c`（注入时钟起点）为参，断言「`StateTick` 在 `(0,60000)` 内、`status_arr_tick=0` 时**每个 50ms pass 减 1**」；用一个「假设每 1s 减 1」的反向断言必红 ⇒ 把 V4-03 原文的不精确读法钉成红检。
- **A-L6（`break` quirk）**：列表 `[DefensePower(StateTick=10), Transparent(StateTick=5)]`，推进一 pass，断言 `Transparent.StateTick` **不变**（被 `break` 跳过）；去掉 `break` 即红。

**必须真机窗口（R 级理由）**

- **A-R1（帧面）**：两侧同种子 + 同注入时钟，对同一「中毒→受击→走/跑」序列，逐帧比 `SM_CHARSTATUSCHANGED(657)` 与走/跑/受击帧的 `Status` 尾 = **diff 0**。理由：状态位进每一帧尾（R④）。
- **A-R2（节奏真值）**：真机测「防御 buff 从下到恢复到 `AC` 回落」的**实际毫秒数**，验证 §3.1.4 第 2 条的 50ms 推导（R② 时序项；离线构造只能验证「我们的模型」，不能验证「oracle 真值」）。
- **A-R3（毒伤数值）**：`characters_ablity.Hp` 等数值列逐行 diff = 0（R④ 落库数值面的对照；A 本身不写库，此处比对同刻内存态导出的 HP）。
- **必须显式写 `window_required: true`**；未跑 ⇒ `request_changes` 或 `approve` + 缺证据声明。

### 3.6 最小可先行切分

| 片 | 内容 | 依赖 | 可先落？ |
| --- | --- | --- | --- |
| A-1 | `buff.rs`（表 + AddBuff/GetBuffStatus/GetBuff/HasBuff/TryUpdate）+ 单测 A-L1/L2 | 无 | ✅ **最先** |
| A-2 | `DoWork`/`Operate` 骨架 + 递减段 + A-L3/L5/L6 | A-1 | ✅ |
| A-3 | `Entity` 加 4 字段；`World::buff_tick` + `buff_enabled` | A-2 | ✅ |
| A-4 | 毒伤段 + `damage_health`（含 MagicShield 抵扣）+ A-L4 | A-3、`combat.rs` | ✅ |
| A-5 | `MakePosion`/`DefenceUp`/`MagDefenceUp`/`MagicShield` 生产链接线 | A-3 | ⚠ 依赖毒来源（怪物/玩家攻击）已在 world |
| A-6 | `GetCharStatus` 接进帧面（`main.rs` 的 `char_status`、走/跑/受击尾、`SM_CHARSTATUSCHANGED`）| A-3 | ⚠ 与 M2 帧接线车道争写集，需串行 |
| A-7 | `ability::recalc_abilitys` 补 `+get_buff` 叠 AC/MAC | A-5 | ⚠ 改既有函数（非新文件）|

### 3.7 设计未定项（A）

- **U-A1**：`StatusTimeArr`（旧 15 槽机制，§3.1.8）是否**同时建模**？只建 `ActorBuffSystem` 会漏 `BubbleDefenceUP`/`STONE`/`DAMAGEARMOR`/`STATETRANSPARENT` 的旧读点。需 owner 裁决范围。
- **U-A2**：sim 时钟步长 200ms vs `ActorBuffProcessor` 50ms。Rust `World::tick` 步长固定 200ms（`world.rs` 文档）⇒ 用 `clock_ms` 做 50ms 节奏门**永远不满足**（每 tick 已 +200）。需 owner 裁决：(a) 把 `buff_tick` 提到世界 tick 外 4 次/200ms；(b) sim 时钟改 50ms；(c) 用 `tick_seq` 计数每 4 tick 一跑。**这是 A 能否实现的前置口径**。
- **U-A3**：`actor.Animal` 的真值口径（`ActorRace.Animal = 50`）；`meat_quality` 在 `ai.meat_quality`（`entity.rs:82`）还是 `ctor_draws.meat_quality`（`entity.rs:181`）两处同名需统一。
- **U-A4**：`i ≥ 32` 时 `0x80000000 >> i` 的 C# 移位回绕语义是否照搬（oracle 未防）。
- **U-A5**：`GetBuffStatus` 的 `int` 符号位（bit31）在 Rust 用 `i32` 还是 `u32` 承载；出帧时 `LoWord/HiWord` 的符号处理须与 `PlayObject.Message.cs:1846` 一致。

---

## 4. Part B — NPC Run（`ProcessNpcs` 200ms）

### 4.1 oracle 逐行实读

#### 4.1.1 驱动链

| 站点 | 行 | 语义 |
| --- | --- | --- |
| `MerchantProcessor` | `MerchantProcessor.cs:7` | `base(TimeSpan.FromMilliseconds(200), "MerchantProcessor")` |
| `ExecuteInternal` | `MerchantProcessor.cs:31` | `SystemShare.WorldEngine.ProcessNpcs();`（同行 `:32` `ProcessMerchants()`）|
| `ProcessNpcs` | `WorldServer.Merchant.cs:118-160` | 从 `NpcPosition` 正序遍历 `QuestNpcList`，`if (!normNpc.Ghost)` 且 `(now - RunTick) > RunTime` ⇒ `RunTick = now; normNpc.Run();` |
| — | `WorldServer.Merchant.cs:131-134` | 上述门 + `Run()` 调用 |
| — | `WorldServer.Merchant.cs:140-145` | ghost 支：`(now - GhostTick) > 60_000` ⇒ `QuestNpcList.RemoveAt(i); break;` |
| — | `WorldServer.Merchant.cs:147-151` | **墙钟预算** `(now - dwRunTick) > M2Share.NpcLimit`（=5，`M2Share.cs:178`）⇒ 存 `NpcPosition` 断点、本 pass 停 |

#### 4.1.2 `Run()` 家族（哪些类覆写、抽什么数）

| 类 | 文件:行 | Run 体 | **共享 RNG 抽数** |
| --- | --- | --- | --- |
| `NormNpc` | `NormNpc.cs:1604-1611` | `if (Master != null) Master = null; base.Run();` | **0** |
| `GuildOfficial` | `GuildOfficial.cs:39-53` | `if (Random(40)==0) TurnTo(RandomByte(8)); else if (Random(30)==0) RM_HIT; base.Run();` | **恒 2**（`Random(40)` + 命中 0 时 `RandomByte(8)`、否则 `Random(30)`；`RandomByte(8)` **也是一抽**）|
| `Merchant` | `Merchant.cs:112-171` | refill/clear 门 + `if (Random(50)==0) TurnTo(RandomByte(8)); else if (Random(50)==0) RM_HIT;` + 沙巴克显隐 + `if (BoCanMove && now-MoveTick > MoveTime*1000) MapRandomMove(...)` + `base.Run();` | **2 或 4**（转身/命中段恒 2 抽；`BoCanMove` 到点 ⇒ `MapRandomMove` 再 2 抽）|
| `Trainer` | `Trainer.cs:40-51` | 攻击均值播报门（`AttackCount>0 && now-AttackTick>3000`）+ `base.Run();` | **0** |
| `CastleOfficial` | `CastleOfficial.cs:13` | `: Merchant` ⇒ **继承 `Merchant.Run`** | **2 或 4**（同 `Merchant`）|

**抽数精确计数（关键；`TurnTo`/`RM_HIT` 的 `RandomByte(8)` 也是一抽）**：

- `GuildOfficial.Run`：`Random(40)` **恒 1 抽**；命中 `0` ⇒ `TurnTo(RandomByte(8))`，`RandomByte(8)` = `random.Next(8)`（`RandomNumber.cs:117-124`）**再 1 抽**；未命中 ⇒ `Random(30)` **再 1 抽**。⇒ **两条路径都恒 2 抽**（原稿「1 或 2」漏算了 `RandomByte(8)`）。
- `Merchant.Run`（含 `CastleOfficial`）：转身/命中段 `Random(50)` **恒 1 抽**；若 `==0` ⇒ `RandomByte(8)` 再 **1 抽**；否则再 `Random(50)` **1 抽** ⇒ 此段**恒 2 抽**。**另**：`BoCanMove && (HUtil32.GetTickCount() - MoveTick) > MoveTime * 1000`（`Merchant.cs:159`）为真 ⇒ `MapRandomMove(MapName, 0)`（`:163`），其内 **2 抽**（`Random(envir.Width - nEgdey - 1)` + `Random(envir.Height - nEgdey - 1)`，`BaseObject.cs:1235-1236`）。⇒ **单次 `Run` 实为 2 抽（不可移动或未到移动门）或 4 抽（可移动且到点）**。
- `TurnTo(...)` 与 `RM_HIT` 只负责出帧（`RM_TURN`/`RM_HIT`）；但 `TurnTo` 的入参 `RandomByte(8)` **本身消耗一抽**（见上）。
- `base.Run()`（`BaseObject.Base.cs:25-316`）对无消息 NPC：`GetMessage` 空、恢复段无 RNG、`SuperMan` 否、`!Death` ⇒ 恢复/`Die`/靶标清理；**不抽数**。⇒ NPC Run 的 RNG 消耗**完全由本表（含 `BoCanMove` 移动门条件）决定**。

> **`Random(n)` = `System.Random.Next(n)`**（`RandomNumber.cs:93-100`）⇒ 与 Rust `RandomNumber::random_below` 同源（`rng.rs:274`）。**但 C# 生产路径是 `new Random()`（无参，.NET6+ = xoshiro256\*\*）**，与 Rust 复刻的**固定种子 legacy 算法**（`rng.rs:116`）**不是同一序列** ⇒ B 的 RNG 对拍**只能在 `MIR2_RNG_SEED` 注入后的可仲裁路径上做**（T-30；`crates/shared/src/rng.rs` 头部已声明此限制）。

#### 4.1.3 NPC 从哪来（与 Rust 现状对齐）

`LocalDB.LoadNpcs`（`LocalDB.cs:688-712` 的 `switch`）：`type 0 ⇒ new Merchant()`、`1 ⇒ new GuildOfficial()`、`2 ⇒ new CastleOfficial()`；`AddQuestNpc(npc)` ⇒ `QuestNpcList.Add`（`WorldServer.cs:1924-1927`）。`NpCinitialize`（`WorldServer.Merchant.cs:37-62`）倒序 `Initialize()`（`AddToMap`），失败 `RemoveAt`。
⇒ **`QuestNpcList` 序 = Npcs.txt 文件正序**（Rust `insert_npc` 的槽位序同口径，`npc.rs:81/160`）。

### 4.2 Rust 现状（file:line）

- **实体已在世界**：`crates/world/src/npc.rs`（`NpcState` `:47`、`insert_npc` `:81`、`attach_npc_to_map` `:111`、`npc_ids_sorted` `:160`）；`entity.rs:422` `pub npc: Option<NpcState>`；宿主接线 `bins/game-svr/src/npcs.rs`。
- **`Run()` 无驱动**：
  - `monai.rs:871-874`：`monster_ai_tick` 的过滤显式 `&& e.npc.is_none()` ⇒ 固定 NPC 不跑怪物 AI（注释自陈「NPC 由 oracle `ProcessNpcs` 驱动，本仓未建模」）；
  - `merchant.rs::merchant_tick` 只认 `merchant.is_some()`（`:547` 用 `merchant_ids_sorted`）⇒ 也跳过 `npc`；
  - `npc.rs` 模块头（`:20-26`）自陈「不跑 NPC 的 `Run()`」。
- **构造抽已照搬（⚠ 2026-10-17 订正）**：`insert_npc` 按 oracle **整条** ctor 公共链 `BaseObject`（`BaseObject.cs:439/441`）+ `AnimalObject`（`AnimalObject.cs:76/77`）抽 **4 次**（`RunTick=now+Random(1500)`、`SearchTime=Random(2000)+2000`、`AttackTick=now-Random(3000)`、`WalkTick=now-Random(3000)`）⇒ **构造期 4 抽已在账本里**。**原稿误作「2 抽」**：只照搬了 `BaseObject` 的 2 抽、漏掉 `AnimalObject` 的 2 抽 ⇒ 共享 RNG 流自首只 NPC 构造起整体错位（本批 critical 订正；唯一抽数点 `mir2_world::BaseAnimalCtorDraws::draw`）。
- **`Merchant.Run` 的模板已存在**：`merchant.rs::merchant_run`（`:649`）已 1:1 复刻 `Merchant.cs:112-171`（转身/命中段 2 抽：`:696/702` 的 `random_below(50)`、`:697` 的 `random_byte(8)`；另含条件 `MapRandomMove` 2 抽 ⇒ **2 或 4 抽**，见 §4.1.2）⇒ **NPC Run 是「把 `merchant_run` 的通用 Run 段抽出、按 `npc.npc_type` 分派子类覆写」**。

> ⚠ **一个现存的重复风险**：`npc_type == 0`（Merchant 类）的固定 NPC **与** `MerchantList` 的商人**是两个不同的 oracle 列表**（`QuestNpcList` vs `MerchantList`），但**同一 `Merchant.Run` 体**。Rust 侧目前 `merchant` 与 `npc` 是**互斥的两个字段**（`entity.rs:417/422`），一个实体不会同时有两者 ⇒ 不会双跑；但设计必须写明**分派依据是 `npc.npc_type`，不是字段存在性**（见 U-B1）。

### 4.3 Rust 建议模型

**扩展 `crates/world/src/npc.rs`**（`NpcState` 加 Run 计时态；或新增 `npc_run` 子模块）：

```rust
/// `BaseObject.RunTick` / `RunTime`（`BaseObject.cs:440` `RunTime = 250`）。
/// 现值：`run_tick_delta`（构造抽分量）；绝对 tick 在 insert 时合成（照 merchant.rs 体例）。
pub const NPC_RUN_TIME: i64 = 250;              // BaseObject.cs:440
pub const NPC_PASS_INTERVAL_MS: i64 = 200;      // MerchantProcessor.cs:7
pub const NPC_GHOST_EXPIRE_MS: i64 = 60_000;    // WorldServer.Merchant.cs:141

impl World {
    /// `ProcessNpcs` 的单线程等价物（200ms 节奏门 + `NpcPosition` 正序 + `RunTime` 门）。
    pub fn npc_tick(&mut self) -> NpcTickOutcome;
    /// `QuestNpcList` 的确定性等价物（槽位序 = Npcs.txt 文件序）。
    fn npc_ids_sorted(&self) -> Vec<i32>;       // 已存在（npc.rs:160）
}

/// 一个 NPC 的一次 `Run()`（供 `npc_tick` 与单测）。
pub fn npc_run(world: &mut World, actor_id: i32) -> NpcRunOutcome;

/// 按 `npc.npc_type` 分派子类覆写体（`LocalDB.cs:688-698` 的 switch 同口径）：
///   0 => merchant_run_body（与 merchant.rs::merchant_run 同一份，2 或 4 抽，见 §4.1.2）
///   1 => guild_official_run_body（Random(40) + [RandomByte(8) | Random(30)]，恒 2 抽）
///   2 => merchant_run_body（CastleOfficial : Merchant）
fn npc_run_body(world: &mut World, npc_type: i32) -> NpcRunOutcome;
```

**`NpcState` 增字段**：`run_tick: i64`（绝对值，`insert_npc` 时 = `now + run_tick_delta`）、`ghost_tick: i64`、`ghost: bool`、`direction: u8`（`TurnTo`/`Dir` 出帧用）、`move_tick: i64`、`bo_can_move: bool`。

**`World` 增**：`pub npc_enabled: bool`、`npc_pass_tick: i64`、`npc_position: usize`（断点，若复刻预算分支）；`World::tick` 在 `merchant_tick` **之后**（或按 U-B2 裁决位置）插 `if self.npc_enabled { self.npc_tick(); }`。

### 4.4 「Rust 不消耗这些抽数 ⇒ 后续序列整体错位」的精确定义（N 离线不可判，须窗口实测）

**命题**：oracle 里 NPC 的 `Run()` 从**全服共享** `M2Share.RandomNumber`（`M2Share.cs:59/194`，单例 + 单 `System.Random`）抽数；Rust 侧该段**零消耗** ⇒ 两侧 RNG 流的**逐次指数**从此处起整体偏移 N（N = 被跳过的抽数累计）。

**精确定义**：

1. **固定在「第一个满足 `RunTime` 门并被 `ProcessNpcs` 触发的 NPC 的第一次抽数」**。在该时刻之前，两侧账本逐项相同（同种子 + 同注入时钟 + 同构造抽）——**但「同构造抽」是必须机的判据、不是默认前提**（⚠ 2026-10-17 订正）：构造链本身抽错数（如固定 NPC 只抽 `BaseObject` 的 2 抽、漏 `AnimalObject` 的 2 抽）会让 `k0` 之前的账本**已经**分岔；若把 k0 前默认成「一致」，该分岔会被静默吸收进 `N`、**跑绿也假绿**。⇒ **构造段必须显式进账本并逐项对**（固定 NPC / 商人列表各自 `new` 时的整条 ctor 公共链 4 抽 = `BaseObject`×2 + `AnimalObject`×2；见 B-L5）。
2. **偏移量 N** = 从该点起、到本次比较窗口结束，Rust **少消耗**的抽数总和。单次 `Run()` 的抽数见 §4.1.2（`npc_type 1`：**恒 2 抽**；`npc_type 0/2`：**2 或 4 抽**，取决于 `BoCanMove` 与 `MoveTime` 移动门；`Trainer` 类 0 抽）。**⇒ 因单次抽数依赖运行期状态（`BoCanMove` / `MoveTick` / `Random` 命中），N 离线不可判定**——只能在真机窗口里、按声明的消费者序**实测**（B-R1）；本设计件**不对 N 给出预判值**。
3. **判据（site/order 账本）**：两侧用 `MIR2_RNG_LOG`（`rng_trace.rs:31`）落 `seq \t kind \t max/arg \t result \t site` 流水；
   - 按 `seq` 对齐后，**第一处分歧 index `k0`** 即 oracle 第一条 NPC Run 抽数所在 index（Rust 在此处记的是**下一个消费者的** site/result）；
   - 从 `k0` 起，Rust 账本的第 `k` 项 = oracle 账本的第 `k + N` 项（N 单调不减，等于已越过的 NPC 抽数）；
   - **验证方法**：把 Rust 侧按「补上 N 个 NPC Run 抽数」对齐后，`site` 列应从 `k0` 起**逐项相等**；若不等 ⇒ 说明除数量外还有**次序错了**（见下）。
4. **次序即位置**：错位不仅取决于抽数**总数**，还取决于 NPC Run 在**线性化消费者序**里的**位置**。oracle 的 `ProcessNpcs` 在**独立 200ms 线程**，与 `UserProcessor`/`MonsterProcessor`/`ActorBuffProcessor` 并发 ⇒ **oracle 内部跨线程交错本身不可复现**（与 T-42 口径同族）。
   ⇒ **Rust 侧能且只能对「一个声明过的串行化消费者序」负责**：`World::tick` 的显式次序（`world.rs:630-650`：死亡结算 → 宝宝清理 → `monster_ai_tick` → AI 攻击转抄 → `merchant_tick` → AOI）。`npc_tick` 插在哪一格，**决定了 `k0` 与 N 的相对关系**；这一格必须有 owner 裁决并写进判据的「同窗口批次前提」（照 `M3RNG同窗口批次前提机判报告-20261013.md`）。

> **⇒ 结论（收敛后）**：B 的 R 判据不是「Rust 消耗了和 oracle 一样的抽数」就够，而是**「在同一个声明过的消费者序下，两侧账本逐项相等」**。而且**偏移量 N 离线不可预判**（单次 `Run` 抽数依赖 `BoCanMove` 等运行期状态）⇒ 只能**在窗口内逐次实测/对齐**；cross-thread 交错**显式不在判据内**（同 T-42 的「跨 pass 交错不在判据内」）。
>
> **⚠ 2026-10-17 订正（构造段必须覆盖）**：上式「两侧账本逐项相等」**必须从 `seq=0`（构造期）起对齐**，不能只从 `k0`（首次 NPC Run）起。原稿以 `k0` 为锚、默认「k0 之前两侧一致」⇒ 构造期分岔（抽数 kind/顺序错）会被 `N` 掩蔽。⇒ 窗口判据**必须含构造段 site/order 逐项相等**：固定 NPC / 商人列表各自 `new` 时的整条 ctor 公共链 4 抽（`BaseObject`×2 `:439/441` + `AnimalObject`×2 `:76/77`）也要落进同一 `MIR2_RNG_LOG` 账本、与 oracle 逐项对齐（R 级窗口项）。

#### 4.4.1 构造点全表（oracle `new Merchant(...)` 全仓枚举，2026-10-17 补登）

§4.4 的构造段判据要求「两侧账本从 `seq=0`（构造期）起逐项相等」。oracle 的 `new Merchant(…)`
**不止** `LoadMerchant`/`LoadNpcs` 两处——全仓 `grep -rn "new Merchant" src/` 另有 10 处
（其中 1 处源码已注释）。**每条 `new Merchant(…)` 都经同一 ctor 公共链抽 4 个数**（`BaseObject`×2
`:439/441` + `AnimalObject`×2 `:76/77`；`Merchant()` ctor 体 `Merchant.cs:82` 不加抽）⇒
构造段总长与次序由下表决定：

| # | oracle 锚 | 归属方法 / 触发 | Rust 现状 | 处置 |
| --- | --- | --- | --- | --- |
| G1 | `src/GameSrv/GameApp.cs:85`（`SystemShare.ManageNPC`） | `InitializeSystem()`，启动期（`GameApp` ctor，**早于** `LoadMerchant`） | **未建模**（Rust 启动期无对应 `new Merchant()`） | 列进窗口项（补 4 抽或显式登记 fold） |
| G2 | `src/GameSrv/GameApp.cs:86`（`RobotNPC`） | 同上 | 未建模 | 同上 |
| G3 | `src/GameSrv/GameApp.cs:87`（`FunctionNPC`） | 同上 | 未建模 | 同上 |
| L1 | `src/GameSrv/DB/LocalDB.cs:210`（`QFunctionNpc()`） | `LoadMapQuest()` 尾 `:404`；`AddMerchant` | **只作脚本宿主标签**（`crates/envir/src/mapquest.rs` `HiddenNpcSpec` → `QuestNpcRecord` → `bins/game-svr/src/npc.rs` `NpcHost`），未建世界实体 ⇒ 不抽 | 列进窗口项 / 显式登记未建模 |
| L2 | `src/GameSrv/DB/LocalDB.cs:268`（`QMangeNpc()`） | `LoadMapQuest()` 尾 `:403`；`AddQuestNpc` | 同上（`QManage`） | 同上 |
| L3 | `src/GameSrv/DB/LocalDB.cs:312`（`RobotNpc()`） | `LoadMapQuest()` 尾 `:405`；`AddQuestNpc` | 同上（`RobotManage`） | 同上 |
| L4 | `src/GameSrv/DB/LocalDB.cs:450`（`LoadMerchant()`） | `Merchant.txt` 逐条；`AddMerchant`（MerchantList） | **已建模**：`bins/game-svr/src/merchant.rs::spawn_merchants` → `World::merchant_ctor_run_delay`（正序，4 抽/条） | 已建模含 4 抽 |
| L5 | `src/GameSrv/DB/LocalDB.cs:691`（`LoadNpcs()` `case 0`） | `Npcs.txt` 逐条；`AddQuestNpc`（QuestNpcList） | **已建模**：`bins/game-svr/src/npcs.rs::place_npcs` → `World::insert_npc`（正序，4 抽/条） | 已建模含 4 抽 |
| L6 | `src/GameSrv/DB/LocalDB.cs:1067`（`ReLoadMerchants()`） | 运行期重载；`AddMerchant` | **未建模**（Rust 无 `ReLoadMerchants` 对应） | 列进窗口项 |
| M1 | `src/GameSrv/Maps/MapQuestManager.cs:40`（`CreateQuest()`） | `LocalDB.LoadMapQuest` 的 `CreateQuest(...)` 调用点（`LocalDB.cs:386`；MapQuest.txt 读取起于 `:352`、for 循环 `:358`）；`AddQuestNpc` | **只作记录**（`crates/envir/src/mapquest.rs::create_quest` → `QuestNpcRecord`），未建世界实体 ⇒ 不抽 | 列进窗口项 / 显式登记未建模 |
| M2 | `src/GameSrv/Maps/Maps.cs:454`（`LoadMapQuest(sName)`） | `Maps.cs` `CHECKQUEST` 图命令（`:137`）；`AddQuestNpc` | **未建模**（Rust 未建世界实体） | 列进窗口项 |
| X1 | `src/Modules/GameCommand/Commands/MobNpcCommand.cs:36` | GM 命令（**源码已注释** `//var merchant = new Merchant();`） | N/A | **显式登记 N/A**（不抽） |

**⇒ 一句话回答「构造段 `seq=0` 对齐在补齐前是否可达」**：**不可达**。oracle 构造段**头部**
（`GameApp` ctor 的 `InitializeSystem`，早于 `LoadMerchant`/`LoadNpcs`/`LoadMapQuest`）就**无条件**
抽 **12 个**（G1–G3 各 4），Rust 启动期一个都不抽；随后 `LoadMapQuest` 尾的三只隐藏 NPC
（L1–L3）与 `MapQuest_def` 任务商人（M1/M2）在 Rust 侧只落成**记录 / 脚本宿主**、**不建世界实体**
⇒ 也不抽。⇒ 只要 G1–G3、L1–L3、L6、M1/M2 尚未**逐条建模（含 4 抽）或显式登记为 fold / N-A**，
构造段两侧账本就不可能逐项相等（`seq=0` 起第一项即分歧）。L4/L5 已建模，并由
`bins/game-svr/src/npcs.rs::base_animal_ctor_chain_matches_oracle_on_all_three_paths`
（含构造后**流位置**比对）钉住 4 抽——是构造段**已对齐的全部**；其余 **10 处**（`MobNpcCommand.cs:36` 源码已注释 ⇒ 待处置 **9 处**）为未对齐窗口项。

### 4.5 判据

**离线可判（构造级）**

- **B-L1（抽数计数）**：`npc_type=1` ⇒ 断言该 `Run()` **恒 2** 抽（`Random(40)` + 命中 0 时 `RandomByte(8)`、否则 `Random(30)`）。`npc_type=0/2` ⇒ 未到 `BoCanMove` 移动门断言 **2** 抽、到点断言 **4** 抽（构造上强制/不强制 `BoCanMove && now-MoveTick > MoveTime*1000` 两臂）。
- **B-L2（节奏门）**：`run_tick` 未到 `RunTime`（250ms）时不抽数；到点后 `run_tick` 被刷成 `now`（照 `merchant.rs:567-571` 体例），下一 pass 不再抽。
- **B-L3（次序）**：多个 NPC（含不同 `npc_type`）按槽位序触发，断言抽数次序 = 槽位序 × 每 NPC 的抽数序列（用 `MIR2_RNG_LOG` 比对 site/order）。
- **B-L4（`base.Run` 无抽数）**：无消息 NPC 的 `npc_run` 尾部（`base.Run` 段）**新增 0 抽**（负向断言）。
- **B-L5（构造抽不回归，⚠ 2026-10-17 订正）**：`insert_npc` 按 oracle **整条** ctor 公共链抽 **4 次**（`BaseObject` `Random(1500)`→`Random(2000)+2000`，再 `AnimalObject` `Random(3000)`→`Random(3000)`；`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`）——**原稿「恰 2 抽」与 oracle 相反**（少抽 `AnimalObject` 的 2 抽、流整体错位）。跨路径机判 `bins/game-svr/src/npcs.rs::base_animal_ctor_chain_matches_oracle_on_all_three_paths` 把怪物/固定 NPC/商人列表三路径钉在同一张表（`mir2_world::BASE_ANIMAL_CTOR_DRAW_MODS` + `BaseAnimalCtorDraws::draw`，一处定义、两模块三调用点直接消费 + 一条被门钉齐的同值副本（怪物路径 `bins/game-svr/src/mongen.rs` 硬编码））上；加入 `npc_tick` 后构造段账本不变。

**必须真机窗口（R 级理由）**

- **B-R1（同窗口逐项账本 + N 实测）**：真机两侧 + `MIR2_RNG_SEED` + 同 `MIR2_RNG_LOG`，跑「多个固定 NPC 存活 ≥ 1 个 `RunTime` 窗口」的场景，沿线：`seq`/`site`/`result` **逐项 diff = 0**（或按 §4.4 的对齐规则相等）；**偏移量 N 只在窗口内实测/对齐**（离线不可预判，见 §4.4 第 2 条）。理由：R①（RNG 抽数序列）。
- **B-R2（NPC 出帧面）**：`TurnTo`/`RM_HIT` 的 `RM_TURN`/`RM_HIT` 逐帧比（AOI 内可见）。
- **B-R3（后续漂移面）**：证明修复后「怪物走位/命中/掉落逐步偏离」消失（大范围对拍不再漂移）——这是 B 的**最终目的**，须真机。
- 全部 `window_required: true`；未跑 ⇒ 按 §2 规则处理。

### 4.6 最小可先行切分

| 片 | 内容 | 依赖 | 可先落？ |
| --- | --- | --- | --- |
| B-1 | `npc_run_body` 抽数骨架 + `NpcState` Run 计时字段 + B-L1/L2/L5（**先只抽数，不出帧**）| `npc.rs`（已存在）| ✅ **最先** |
| B-2 | `World::npc_tick`（200ms 门 + 槽位序 + `RunTime` 门）+ B-L3/L4 | B-1 | ✅ |
| B-3 | `npc_enabled` 开关 + sim 循环接线（照 `enable_merchant_tick` 体例）| B-2 | ✅ |
| B-4 | 出帧（`RM_TURN`/`RM_HIT`）+ B-R2 | B-2、AOI 帧面 | ⚠ 依赖帧接线车道 |
| B-5 | 沙巴克显隐 / `BoCanMove` 随机移（`Merchant.cs:143-163`）| B-3、castle/地图随机移 | ⚠ 依赖别的东西多 |

### 4.7 设计未定项（B）

- **U-B1**：分派依据 = `npc.npc_type`（0/1/2）。**隐患**：`NpcState` 的 `#[derive(Default)]`（`npc.rs:47`）令未显式赋值的 `npc_type = 0` ⇒ 会被当 Merchant 跑 2 抽。需确认宿主 `npcs.rs` 是否已按 `LocalDB` 的 `switch` 正确赋 `npc_type`（**见 `must_be_verified_elsewhere`**）。
- **U-B2**：`npc_tick` 在 `World::tick` 的**哪一格**（`merchant_tick` 前/后？）——直接决定 RNG 账本的 `k0`/N。需 owner 裁决 + 记入「同窗口批次前提」。
- **U-B3**：`npc_run` 是否接 `BaseObject.Run` 的**死亡支**（`MakeGhost`，`BaseObject.Base.cs:84-102`）？该支与 critic **NL4** 重叠。NPC（race=10）一般不会 death，但 ghost 支（`WorldServer.Merchant.cs:140-145` 的 `RemoveAt`）**确实在 `ProcessNpcs` 里**。需裁决：B 是否连带实现 ghost 移除。
- **U-B4**：`NpcLimit = 5` 的**墙钟预算**（`WorldServer.Merchant.cs:147-151`）是否照搬？照 T-42 先例，Rust 单线程确定性 tick **丢预算**（`merchant.rs:532` 的 `merchant_tick` 文档第 3 条已如此，原 `:597` 已漂移），B 应同口径并登记 whitelist。
- **U-B5**：`Merchant.Run` 与固定 NPC 的 `Merchant` 类**共用一份 run body** vs **两套**（`merchant.rs::merchant_run` 已存在）——复用会让两处耦合，复制会漂移。需裁决（**建议复用**，把 `merchant_run` 提到公共模块）。
- **U-B6**：`merchant_tick` 的 250ms `RunTime` 与 `ProcessNpcs` 的 250ms `RunTime` **同值**（`BaseObject.cs:440`），但 `MerchantList` 与 `QuestNpcList` 是两个列表、`MerchantProcessor` 同一 200ms 线程里**先后**调 `ProcessNpcs` 再 `ProcessMerchants`（`MerchantProcessor.cs:31/32`）——Rust 是两个独立 `*_tick`，**谁先谁后**决定 RNG 次序。需裁决（oracle 是 `ProcessNpcs` 先）。

---

## 5. Part C — 世界时钟

### 5.1 oracle 逐行实读

| 站点 | 行 | 语义 |
| --- | --- | --- |
| `SystemProcessor` 节奏 | `SystemProcessor.cs:12` | `base(TimeSpan.FromMilliseconds(50), "SystemThread")` |
| `ExecuteInternal` | `SystemProcessor.cs:48` | `GetGameTime();`（每 50ms）|
| `GetGameTime` | `SystemProcessor.cs:98-134` | 按 `DateTime.Now.Hour` 置 `M2Share.GameTime`：`1`=白天(5-10,16-19,21)、`2`=日落(11,20,23)、`0`=日出(4,15)、`3`=夜晚(0-3,12-14,22) |
| `M2Share.GameTime` | `M2Share.cs:182` | `public static byte GameTime = 0;` |
| 登录时初值 | `PlayObject.cs:1384` | `Bright = M2Share.GameTime;`（进世界处；同一处 `:1385-1386` 发帧 52/752）|
| 变化广播 | `PlayObject.Base.cs:824-830` | `if (Bright != M2Share.GameTime) { Bright = M2Share.GameTime; SendMsg(RM_DAYCHANGING, 0,0,0,0); }` |
| 广播触发点 | `WorldServer.cs:665-666` | 在 `ProcessPlayObjectData` 里，`SearchTime` 门（`playObject.SearchTime`，ctor `Random(2000)+2000`）到点后 `SearchViewRange(); GameTimeChanged();` |
| 每玩家 Run 驱动 | `WorldServer.cs:625-670`（`ProcessPlayObjectData`）← `WorldServer.cs:481 ProcessHumans` ← `UserProcessor.cs:31`（50ms）| 每玩家按**自己的** `RunTime`/`SearchTime` 门跑 |
| 出帧 | `PlayObject.Message.cs:1741` | `SM_DAYCHANGING(46) = MakeMessage(SM_DAYCHANGING, 0, Bright, DayBright(), 0)` |
| `DayBright` | `PlayObject.Base.cs:244-269` | `Flag.DayLight ⇒ 0`；`Flag.boDarkness ⇒ 1`；否则 `Bright`：`1⇒0, 3⇒1, 其它⇒2` |

**⇒ 触发条件三点：**

1. `M2Share.GameTime` **每 50ms 重算**（按真实本地小时桶），**但** `GameTimeChanged` 只在 `Bright != M2Share.GameTime` 时出帧 ⇒ **实际出帧发生在「真实小时跨桶」的时刻**（每个玩家各自的下一个 `SearchTime` 门到达时）。
2. 每个玩家的 `Bright` 是**自己缓存的**（`PlayObject.cs:602` 字段，登录时 = 当时的 `M2Share.GameTime`）⇒ 跨小时时，**每个在线玩家在下一次 `SearchTime` 门各发一条** `SM_DAYCHANGING(46)`。
3. `RM_DAYCHANGING`（10053）是内部消息号；实际出帧是 `SM_DAYCHANGING`（46）。

### 5.2 Rust 现状（file:line）

- `bins/game-svr/src/enter.rs:203-215` `game_time_from_hour(hour)`：**已 1:1 复刻** `GetGameTime` 的 switch（`:1004-1021` 单测逐值钉死）。
- `enter.rs:217-226` `game_time_now()`：读 `chrono::Local::now().hour()`（真实时区），注释自陈「与 oracle 的 50ms 刷新在小时桶上等价」。
- `enter.rs:71-72` `WorldFrameInputs::game_time: Option<u8>`；`enter.rs:329-341` `day_bright(flag, game_time)`；`enter.rs:524-540` `apply` 里**只在进世界**发帧 46（`if let Some(gt) = wx.game_time { base.day_changing = Some((gt, d)); }`）。
- `bins/game-svr/src/main.rs:8364`：`game_time: Some(enter::game_time_now())`（**唯一生产调用点 = 进世界**）。
- `main.rs:5824`：切图下行也用 `game_time_now()` 重算 `day_bright`（**切图时刷新，非周期**）。
- **无周期调用者**：全仓 `game_time_now()` 仅上述两处，无 tick；无 `PlayObject.Bright` 等价缓存字段（`enter.rs` 用 `Option<u8>` 一次性传参）。
- **可注入时钟已具备**：`bins/game-svr/src/clock.rs`（`MIR2_CLOCK_MS`，Rust 半已落地；C# 半未落地，模块头 `:22-26` 自陈）。

### 5.3 Rust 建议模型

```rust
// crates/world/src/world.rs
/// oracle `M2Share.GameTime`（世界时钟桶值）。**world 级单值**，不是 per-entity。
pub game_time: u8,
pub game_time_pass_tick: i64,

impl World {
    /// `SystemProcessor` 的 50ms `GetGameTime` 等价物（节奏门 + 小时桶映射）。
    /// hour 来源：可注入时钟的**本地小时**（`chrono::Local`），或由 `MIR2_CLOCK_MS`
    /// origin 换算（见 U-C1）。换算函数复用已合入的 `enter::game_time_from_hour`。
    pub fn game_time_tick(&mut self);      // 每 50ms pass 调一次
}

// 每会话的 Bright 缓存 + 变化广播（归接线层，main.rs）
struct SessionGameTime { bright: u8 }   // PlayObject.Bright 等价物
/// `GameTimeChanged()`：`if bright != world.game_time { bright = world.game_time; 出帧 46 }`。
```

- `game_time_from_hour` / `day_bright` **已存在且已钉死**（`enter.rs`）⇒ C 只需**加周期驱动 + per-session `Bright` 缓存 + 变化出帧**，**不改**这两个换算函数。
- 出帧点：per-session 节拍里比对 `session.bright != world.game_time` ⇒ 出 `SM_DAYCHANGING(46)`（复用已合入的 `enter::` 编码）。
- 节奏门与 U-A2 同题（50ms vs 200ms）。

### 5.4 判据

**离线可判（构造级）**

- **C-L1（桶映射）**：`game_time_from_hour` 24 值全覆盖断言（**已存在**，`enter.rs:1008-1021`）——复用为回归。
- **C-L2（变化才出帧）**：构造 `world.game_time=1`、`session.bright=1` ⇒ 比对后**零帧**；把 `world.game_time` 改 `3` ⇒ 恰 **1 帧** `SM_DAYCHANGING(46)`，帧体 `(Bright=3, DayBright=1)`。
- **C-L3（跨桶）**：注入时钟跨小时边界（如 10:59→11:00）⇒ 断言 `game_time` 从 `1` 翻到 `2`，`DayBright` 从 `0` 变 `2`，出 1 帧。
- **C-L4（DayBright 三分支）**：`DayLight`/`boDarkness`/`Bright` 三支（**已存在**，`enter.rs:955-975`）。

**必须真机窗口（R 级理由）**

- **C-R1（跨小时真机）**：真机把注入时钟设为「跨小时前 1 分钟」，跑 ≥ 2 分钟，抓客户端收帧日志：两侧 `SM_DAYCHANGING(46)` 的**条数、时刻、参数**一致。理由：R②（时序）。
- **C-R2（每玩家各发一条）**：多会话在线时，跨小时各玩家各收 1 条（不是广播 1 条）。
- 全 `window_required: true`。

### 5.5 最小可先行切分

| 片 | 内容 | 依赖 | 可先落？ |
| --- | --- | --- | --- |
| C-1 | `World.game_time` + `game_time_tick`（节奏门 + 桶映射）+ C-L1/L2/L4 | `clock.rs`（已存在）| ✅ **最先** |
| C-2 | per-session `Bright` 缓存 + 变化出帧（会话态）| C-1 | ⚠ 依赖会话态承载（`main.rs`）|
| C-3 | 每玩家 `SearchTime` 门对齐（oracle 是 per-player 门，不是全局广播）| C-2 | ⚠ 需 owner 裁决（见 U-C2）|

### 5.6 设计未定项（C）

- **U-C1**：`MIR2_CLOCK_MS` 注入时，`game_time` 的小时**从哪来**？注入 origin 是「毫秒时间戳」，`chrono::Local` 是墙钟；两者在注入下**必须一致**（否则 C-R1 不可判）。需 owner 裁决注入时钟的时区/换算口径（与 `clock.rs:22-26` 的 C# 半未落地相关）。
- **U-C2**：oracle 是 **per-player `SearchTime` 门**（每玩家各自 `Random(2000)+2000` ms 节奏），Rust 是否照搬 per-session 门，还是退化为「每 tick 全局比对」？照搬 = 每个会话的 `Bright` 缓存 + 各自的 `SearchTime` 门（更忠实，但要会话态）；退化 = 全局比对（更简单，但出帧时刻与 oracle 不逐帧同）。**直接决定 C-R1 的可判性**。
- **U-C3**：`game_time_tick` 与 `npc_tick`/`buff_tick` **共用 50ms 门**还是各自独立？若共用，需与 U-A2/U-B2 的 50ms vs 200ms 一并裁决。
- **U-C4**：切图下行（`main.rs:5824`）现在**用 `game_time_now()` 重算** `day_bright`——周期时钟落地后，切图应读 **`world.game_time`**（与 `Bright` 缓存同源），否则两条路径口径分叉。

---

## 6. 设计未定项汇总（需 owner 或进一步取证）

| ID | 件 | 问题 | 阻塞什么 |
| --- | --- | --- | --- |
| U-A1 | A | `StatusTimeArr` 旧 15 槽是否同时建模 | A 的范围与完整度 |
| U-A2 | A/C | sim 时钟 200ms 步长 vs `ActorBuffProcessor` 50ms | A 与 C 能否用 `clock_ms` 做节奏门 |
| U-A3 | A | `actor.Animal` 真值口径 / `meat_quality` 两处同名 | A-L4 的 `MeatQuality -= 1000` |
| U-A4 | A | `i≥32` 的 `0x80000000 >> i` 回绕是否照搬 | A 的边界 |
| U-A5 | A | `CharStatus` 符号位承载（`i32`/`u32`）| A-R1 帧字节 |
| U-B1 | B | `npc_type` 是否被宿主正确赋值（`default=0` 有隐患）| B 的分派正确性 |
| U-B2 | B | `npc_tick` 在 `World::tick` 的哪一格 | B-R1 账本 `k0`/N |
| U-B3 | B | `npc_run` 是否接 `BaseObject.Run` 死亡/ghost 支（与 NL4 重叠）| B 的范围 |
| U-B4 | B | `NpcLimit=5` 墙钟预算是否丢（照 T-42）| B 的 whitelist 登记 |
| U-B5 | B | `Merchant.Run` body 复用还是复制 | B 的代码结构 |
| U-B6 | B | `ProcessNpcs` 先于 `ProcessMerchants` 的次序是否照搬 | B-R1 账本 |
| U-C1 | C | 注入时钟下 `game_time` 小时来源 | C-R1 可判性 |
| U-C2 | C | per-session `SearchTime` 门 vs 全局周期比对 | C-R1 逐帧一致 |
| U-C3 | C | 三个 50ms 门共用还是独立 | 实现结构 |
| U-C4 | C | 切图下行是否改读 `world.game_time` | 两条路径口径一致 |

---

## 7. 最小可先行切分总表 + 依赖图

```
A-1 (buff 表)  --> A-2 (DoWork/Operate) --> A-3 (Entity 字段 + World::buff_tick) --> A-4 (毒伤 + damage_health) --> A-5/A-6/A-7 (生产/帧/ability)
B-1 (npc_run 抽数) --> B-2 (World::npc_tick) --> B-3 (开关+sim 接线) --> B-4/B-5 (帧/沙巴克)
C-1 (World.game_time + tick) --> C-2 (session Bright + 出帧) --> C-3 (per-player 门)
```

- **三条 A/B/C 可独立并行**（不同新文件：`buff.rs` / `npc.rs` 扩展 / C 在 `world.rs`+会话态）。
- **公共前置 = U-A2**（50ms vs 200ms 节奏门口径）：A 与 C 都卡它；B 用 200ms（`MerchantProcessor`）**不卡**。
- **A-6/A-7 与 M2 帧接线车道争写集**（走/跑/受击帧尾、`ability.rs`）⇒ 按 AGENTS.md「按写集切分」，A-6/A-7 须与帧车道串行。
- **B-4 与 AOI 帧面争写集** ⇒ 串行。
- **A/B/C 都改 `world.rs` 的 `tick` 体**（插 `buff_tick`/`npc_tick`/`game_time_tick`）⇒ 三者需约定插入点（见 U-A2/U-B2/U-C3），否则写集重叠。可先把「插入点」写成一条 `tick` 内的稳定次序，再由三片各加一行。

## 8. 附：Rust 现状锚全表（本文档实读）

| 件 | Rust 站点 | 现状 |
| --- | --- | --- |
| A | `main.rs:8310-8314` | `char_status: 0`（登记）|
| A | `crates/world` 全仓 | 无 buff 表（grep 0）|
| A | `entity.rs:82/302/303` | 有 `meat_quality`/`hide_mode`/`fixed_hide_mode`；缺 `poisoning_tick`/`green_poisoning_point`/`char_status(_ex)` |
| B | `npc.rs:47/81/111/160` | `NpcState`/`insert_npc`/`attach_npc_to_map`/`npc_ids_sorted` |
| B | `entity.rs:422` | `pub npc: Option<NpcState>` |
| B | `monai.rs:871-874` | 过滤 `&& e.npc.is_none()`（跳过）|
| B | `merchant.rs:539-597` | `merchant_tick`（只认 `merchant.is_some()`）|
| B | `merchant.rs:649-740` | `merchant_run`（`Merchant.Run` 模板，2 或 4 抽，见 §4.1.2）|
| B | `world.rs:637/648` | `monster_ai_tick` / `merchant_tick` 插入点 |
| C | `enter.rs:203-226` | `game_time_from_hour` / `game_time_now`（已 1:1）|
| C | `enter.rs:71-72/329-341/524-540` | `game_time: Option<u8>` / `day_bright` / 只进世界发帧 46 |
| C | `main.rs:5824/8364` | 两个 `game_time_now()` 调用点（切图 / 进世界）|
| C | `clock.rs` | `MIR2_CLOCK_MS` 注入（Rust 半已落地）|
| 公共 | `rng.rs:234-320` | `RandomNumber`（legacy 固定种子复刻）|
| 公共 | `rng_trace.rs:31/140` | `MIR2_RNG_LOG` 逐次流水（`seq/kind/arg/result/site`）|

## 9. 残留物清单

- 本文档为本车道**唯一新增文件**（LF）；未改任何既有文件（别的车道正改 `recon_gates.py`/`whitelist.md`/采集器/`main.rs`）。
- 未起服务、未跑 cargo、未占端口。
- **S2-FIX 复核轮（2026-10-16）**：按批判者抽查结论订正 §1 锚点（`SystemProcessor.ProcessNpcs` → `MerchantProcessor.ProcessNpcs`）、§4.1.2 抽数模型（Merchant `2 或 4`、GuildOfficial `恒 2`、`MapRandomMove` 2 抽）、§3.3/§3.1.4 的 `damage_health` `LastHiter` 守卫、§0/§2/§3.1.4 的 50ms 节奏措辞（降为模型推导）。
- 未验证项（见 `must_be_verified_elsewhere`）：宿主 `bins/game-svr/src/npcs.rs` 是否按 `LocalDB` 的 `switch` 正确赋 `npc_type`；`crates/envir` 是否已解析 `[Setup] PosionDecHealthTime`；`merchant.rs::merchant_run` 与 `Merchant.cs:112-171` 的逐行一致性是否已被既有 review 覆核。

## 10. 落点建议（whitelist / 卡）

若 owner 采纳本设计件，建议**先立三张卡**（一卡一件，写集互不重叠）并在实现后按 §3.5/§4.5/§5.4 写 `review` 的 `tier`/`window_required`：

- `openmir2-m3-buff-system`（写集：`crates/world/src/buff.rs`、`entity.rs`、`world.rs`、`combat.rs`）—— tier=R。
- `openmir2-m3-npc-run-tick`（写集：`crates/world/src/npc.rs`、`merchant.rs`（若复用 body）、`world.rs`、`bins/game-svr/src/main.rs`）—— tier=R。
- `openmir2-m3-world-clock-tick`（写集：`crates/world/src/world.rs`、`bins/game-svr/src/main.rs`、`enter.rs`）—— tier=R。

whitelist 候选（实现时登记，照 T-42/T-51 体例）：`NpcLimit` 墙钟预算丢（U-B4）、`StatusTimeArr` 旧机制未建模（若 U-A1 选「不建」）、U-A2 的 50ms 门适配口径。
