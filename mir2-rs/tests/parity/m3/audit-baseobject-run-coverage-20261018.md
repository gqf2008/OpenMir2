# `BaseObject.Run` 主循环覆盖审计（oracle ↔ Rust 等价物）

- 审计日期：2026-10-18
- 状态基线：审计基线 `e9cc6a8fd`（detached worktree `/e/tmp/impl-runaudit`）；修订基线 `d3e09228e`（复验后 rebase；已落地 `8b4f5df5e`：`BaseObject.Run` 的 30s `VerifyMapTime` + 玩家 viewer 过期分流 + NPC/商人 `Run` 尾部接线 + AOI 诊断口径）
- 触发：`audit-aoi-guard-visibility-20261018.md` 定位到「Rust 未移植 `VerifyTick/VerifyMapTime` ⇒ 静止 actor 60s 后被 AOI 逐出」，说明「Run 主循环逐步覆盖」存在静默缺口。本审计把 oracle 的 `Run` 与各子类覆写**逐句**对照 Rust 等价物，只产出覆盖矩阵与缺口清单，**不实现**。
- 方法：只读源码实读（`src/**`、`mir2-rs/crates/**`、`mir2-rs/bins/**`）+ 既有证据包复核；**未改 `src/**`、`sql/**`、`MirClient/`、`crates/{protocol,storage}/**`、`E:/MirServer/**`；未起服务、未占端口、未写库、未删文件**。
- 引用纪律：**oracle 行号逐条实测**——本次修订在 `d3e09228e` 基线上用 `sed -n` 逐条重量，并由自检脚本（见 §6）对全文所有 `文件:起-止` 区间校验「起止在文件行数内 + 首/末行与描述对得上 + 函数体区间与花括号配对一致」；**Rust 侧只引符号/文件路径，不写 `:行号`**。
- 判级：按 `AGENTS.md`「判据分级」；判不准一律按 **R**。
- 状态回写（2026-10-18 晚，累计）：**B08 → `b12b59ca8`；P16 → `706c95cae`；M06/M08 → `209023120`；O03/O04/O05 → `f46e4f03f`**（另有 PR-2 类常量 `1821078dc`、PR-3 归属 spike `f64d9f241`、分批计划 `32c72c120` 入库）。
- 状态回写（2026-10-18，`58da49071`）：**A01 / M11 / P03 / P04 已实现**（本审计是时点文档，正文行保留审计时的判定，此处统一标注最新状态）。另两处 oracle 订正：`BreakHolySeizeMode`（`AnimalObject.cs:330-334`）只置 `HolySeize=false` + `RefNameColor()`、**不复位** tick/interval；FireHit 到期实际在 `PlayObject.Message.cs:54-59`、TwinHit 在 `:60-64`。

---

## 0. 结论摘要

1. **结构缺口**：oracle 的玩家主循环是「`UserProcessor` 50ms → `ProcessPlayObjectData` → 逐玩家 `RunTime` 门 → `PlayObject.Run()`（自身消息泵 + 40 余段）→ `BaseObject.Run()`（公共段）」。Rust **没有玩家 `Run` 等价物**：玩家只有命令 FIFO + 若干宿主侧散段（`drain_recovery`/`drain_stamina`/`drain_deal_tick`/`drain_periodic_save`/buff pass）以及 `World::verify_players_map_time`（`World::tick` 的 AOI 前，仅覆盖 `base.Run` 的 `VerifyMapTime` 一段），`PlayObject.Run` 的 38 段中 **18 段缺失、15 段部分、4 段已实现、1 段不适用**（P36 已随 `8b4f5df5e` 落地）。
2. **怪物主循环**：`monster_run` + `base_object_run_monster` 覆盖了 `MonsterObject.Run` 的普通怪路径与 `BaseObject.Run` 的恢复/清理段，但 **`RunTime/RunTick` 每 actor 节拍、`ProcessMonsterInterval` 可见性降频、死亡 `MakeGhost`、`Master/SlaveRelax`、`CrazyMode`、43 个专用怪 `Run` 中的 34 个**完全未建模（另 7 个部分、2 个与父类等价）；`VerifyTick` 的 `VerifyMapTime` 段已由 `8b4f5df5e` 接线（HP/MP 夹 `Max-1` 仍缺）。
3. **`VerifyMapTime` 不是孤例**：同族 30s/500ms/1000ms/2000ms 周期门里，`AutoRecoveryTick` 已实现、`DecLightItemDrugTick` 只实现了 `CheckPkStatus`（`UseLamp` 缺失）、`SearchTime` 的调用点与节拍**均与 oracle 不同**；`VerifyTick` 的 `VerifyMapTime` 段已由 `8b4f5df5e` 接线（`World::verify_map_time`，30s 严格大于门），HP/MP 超上限夹 `Max-1` 仍未建模。
4. **量化差（关键订正）**：`UserProcessor` 名义 50ms，但 `ProcessPlayObjectData` 的 `(now - RunTick) > RunTime(250)` 门使**真实玩家 `Run()` 名义上每 300ms 一次**（不是 50ms）。因此 `BaseObject.Run` 的 `recoveryTick=(now-AutoRecoveryTick)/20` 对真实玩家名义上是 **15/次 = 50/s**（.NET 抖动到 250–300ms 时为 12–15/次 = 40–50/s），而 `bins/game-svr/src/stamina.rs` 按「50ms 一次、每次 +2 = 40/s」重放。**40/s 的推导依赖「实测击间隔≈662ms」这一推断，而证据文件没有时间戳**；若真实击间隔≈530ms（50/s 假说），同样能解释「第 86 击掉 1 HP」。另：`300ms/Run` 是「定时器恰好 50ms」的名义量化，.NET 定时器抖动可落到 250–300ms/Run（40–50/s 区间），所以 40/s 并未被证伪。这是 R 级未决口径，只能由带时间戳的真机 dump 裁定（见 §4.3、§5 MCS-8）。
5. **怪物节拍**：oracle 普通怪受 `RunTime=250` 门（≈4/s）+「不可见 ⇒ `ProcessMonsterInterval=2` 再降频」（≈1.3/s）；Rust 全部怪物**每 200ms 一次（5/s）**。即 **AI/`Think`/`CheckRoyalty` 的评估频率**约 1.25×（可见）/3.75×（不可见）；但 `Wondering` 本身只在 `walk_due` 门内调用（`crates/world/src/monai.rs`），`walk_speed` 载入下限 200ms（`crates/data/src/loaders.rs`）、world tick 200ms 且用严格 `>` ⇒ **Rust `Wondering` 实际 ≤2.5/s**，oracle 可见怪约 4/s（Rust 反而更慢）。~3.75× 不是 `Wondering` 的 RNG 抽取次数，只适用于评估频率与 `dup_mode` 等条件分支（R 级）。
6. **状态 tick 不在 `Run` 里**：`Burning` 在本 oracle 版本**不存在**；毒/石/麻痹由 `ActorBuffProcessor` 50ms 的 `ActorBuffSystem.Operate` 驱动，Rust `buff.rs` 已 1:1 复刻并每 200ms 补 4 个 pass。缺口在**帧面**：`buff_sysmsgs`（`SM_SYSMESSAGE(100)`）与 `buff_recalcs`（`RM_ABILITY`）登记为 deferred，且 buff 对 AC/MAC 的加成未进入 Rust 的能力重算。

覆盖矩阵共 **139 行**（§2）：`B`17 + `A`2 + `M`13 + `R`43 + `O`11 + `P`38 + `N`6 + `D`9（O 段含 §1.6 新增 O10/O11）。

---

## 1. Oracle 侧清册

### 1.1 驱动模型（先定节拍，否则逐句对照没有意义）

| 驱动 | oracle 位置（实测） | 节拍/门 | 说明 |
|---|---|---|---|
| 玩家主循环 | `src/GameSrv/Word/Threads/UserProcessor.cs:7` | `TimerScheduledService(50ms)` | `ExecuteInternal` 调 `SystemShare.WorldEngine.ProcessHumans()`（`:31`） |
| 玩家逐人门 | `src/GameSrv/Word/WorldServer.cs:645-647` | `(dwCurTick - playObject.RunTick) > playObject.RunTime` | `PlayObject.RunTime=250`（`PlayObject.cs:899`）；50ms 量化 ⇒ **名义每 300ms 一次**（抖动 250–300ms） |
| 玩家视野门 | `src/GameSrv/Word/WorldServer.cs:663-667` | `(now - SearchTick) > SearchTime` | `PlayObject.SearchTime=1000`（`PlayObject.cs:900`）⇒ ≈1050ms 一次 |
| 玩家消息泵预算 | `src/M2Server/Player/PlayObject.Message.cs:360` | `(now - GetMessageTick) < HumanGetMsgTime(200)`（`GameSvrConf.cs:1401`） | 每次 `Run` 最多处理 200ms 的消息 |
| 机器人 | `src/GameSrv/Word/Threads/SystemProcessor.cs:39` + `src/GameSrv/Robots/RobotObject.cs:382` | `SystemProcessor` 50ms 调 `AutoBot.Run()` | **`RobotObject.Run` 覆写且不调 `base.Run()`** ⇒ 机器人不走恢复/公共段 |
| 怪物主循环 | `src/GameSrv/Word/WorldServer.MonGen.cs:192` | 独立线程 `while(true)` + `(currentTick - monster.RunTick) > RunTime(250)`（`:280`） | 连续循环，非固定 timer |
| 怪物可见性降频 | `src/GameSrv/Word/WorldServer.MonGen.cs:294-299` | `!IsVisibleActive && ProcessRunCount < ProcessMonsterInterval` | 缺省 `ProcessMonsterInterval=2` ⇒ 每 3 个到点 pass 才 `Run` 一次 |
| 怪物视野 | `src/GameSrv/Word/WorldServer.MonGen.cs:300-313` | 仅 `IsSlave \|\| Race ∈ {Guard, ArcherGuard, SlaveMonster}` 且 `(now-SearchTick)>SearchTime` | 普通怪**不**主动 `SearchViewRange` |
| 固定 NPC / 商人 | `src/GameSrv/Word/Threads/MerchantProcessor.cs:7` | 200ms，先 `ProcessNpcs()` 后 `ProcessMerchants()`（`:31-32`） | 逐 NPC `RunTime=250` ⇒ 每 400ms 一次 |
| Buff / 状态 | `src/GameSrv/Word/Threads/ActorBuffProcessor.cs:7` | 50ms → `M2Share.ActorBuffSystem.DoWork(null)`（`:21`） | 毒伤 DoT、状态秒数递减、657/53 帧都在这里，**不在 `Run`** |

> 结论：`BaseObject.Run` 的公共段对**真实玩家**名义上是 300ms 一次（抖动 250–300ms）；对**怪物**是 ≥250ms 一次且受可见性降频；对 **NPC/商人** 是 400ms 一次。把「50ms」直接当成 `Run` 频率会同时算错玩家与怪物两条线。

### 1.2 `BaseObject.Run`（`src/M2Server/Actor/BaseObject.Base.cs:25-316`）逐句

| 行 | 语义 |
|---|---|
| `27-31` | 消息泵：`while (GetMessage(ref msg)) Operate(msg)`；`GetMessage` 取优先队列头，**延时消息未到点 ⇒ 重新入队 Lowest 并 `return false`（整泵中止）**（`BaseObject.Message.cs:334-355`） |
| `32-36` | `SuperMan` ⇒ HP/MP 回满 |
| `37-42` | `!Death` ⇒ `recoveryTick=(now-AutoRecoveryTick)/20`；`AutoRecoveryTick=now`；`HealthTick/SpellTick += recoveryTick` |
| `44-56` | `HP<MaxHP && HealthTick>=HealthFillTime(300)` ⇒ `HP += MaxHP/75+1`（夹 MaxHP）⇒ `HealthSpellChanged()` |
| `57-69` | `MP<MaxMP && SpellTick>=SpellFillTime(800)` ⇒ `MP += MaxMP/18+1` ⇒ `HealthSpellChanged()` |
| `70-73` | `HP==0` ⇒ `Die()` |
| `74-81` | `HealthTick/SpellTick >= FillTime` ⇒ 归零 |
| `83-101` | `Death` 支：`CanReAlive && MonGen != null` ⇒ `makeGhostTime = max(10s, ZenTime-20s)` 再夹 `Config.MakeGhostTime(3min)`；否则 `> MakeGhostTime` ⇒ `MakeGhost()` |
| `105-110` | `HealthTick < -HealthFillTime && HP>1` ⇒ `HP -= 1; HealthTick += HealthFillTime; HealthSpellChanged()` |
| `112-118` | `TargetCret` 清理：`>30s \|\| Death \|\| Ghost \|\| 异图 \|\| \|Δx\|>15 \|\| \|Δy\|>15` ⇒ `ClearTargetCreat` |
| `119-124` | `LastHiter`：`>30s \|\| Death \|\| Ghost` ⇒ 清空 |
| `126-131` | `ExpHitter`：`>6s \|\| Death \|\| Ghost` ⇒ 清空 |
| `133-207` | `Master != null` 支：`NoItem=true`；`AutoChangeColor`（`BBMonAutoChangeColorTime`）轮换 7 种 `CharStatus` + `StatusChanged()`；`FixColor` 同步 `FixStatus`（`180-207`） |
| `209-217` | `SlaveList` 倒序清理 `Death/Ghost/Master != this` |
| `219-221` | `ShowHp && (now-ShowHpTick)>ShowHpInterval` ⇒ `BreakOpenHealth()` |
| `223-246` | `VerifyTick` 30s：`VerifyTick=now`；`!DenyRefStatus` ⇒ `Envir.VerifyMapTime(CurrX,CurrY,this)`；并把 `HP/MaxHP`、`MP/MaxMP` 超上限夹到 `Max-1` + `HealthSpellChanged()` |
| `247-315` | **整段注释**（状态秒数递减 + 毒伤）⇒ oracle 死代码，不实现才是等价 |

`Envir.VerifyMapTime`（`src/M2Server/Maps/Envirnoment.cs:858-901`）：遍历当前格 `ObjList`，
- 任何 `ActorObject` 且 `(now-AddTime)>=60s` ⇒ 删除（`Count>0` 继续，否则 `Clear(); break;`）；
- 找到自己（`CellObjId==ActorId`）⇒ `MapCellInfo.Update(i, ref cellObject)`，`Update` 把 `AddTime=now`（`src/Modules/SystemModule/Data/MapCellInfo.cs:37-41`）；
- 都没命中 ⇒ `AddMapObject(...)` 重新登记。

⇒ 静止 actor 只要每 30s `Run` 一次，格子项永不过期。删除 Rust 的这条链就是既有 AOI 缺陷的根因；`8b4f5df5e` 已按此链在怪物/NPC/商人/玩家等价段接线（见 §2.2 B16）。

### 1.3 `PlayObject.Run`（`src/M2Server/Player/PlayObject.Message.cs:19-766`）逐段

| 行 | 段 | 关键语义/取数 |
|---|---|---|
| `30-36` | 交易自动取消 | `Dealing` 且 `GetPoseCreate()!=DealCreat \|\| DealCreat==this \|\| null` ⇒ `DealCancel()` |
| `37-51` | 计费 | `PayMentMode==3` 每分钟扣 `ExpireTime`，到期两条 SysMsg + `BoEmergencyClose` |
| `54-59` | 烈火剑法 | `FireHitSkill && >20s` ⇒ 清标志 + `SpiritsGone` + `+UFIR` |
| `60-64` | 双龙斩 | `TwinHitSkill && >60s` ⇒ 清标志 + `+UTWN` |
| `65-69` | 延时回城 | `IsTimeRecall && now>TimeRecallTick` ⇒ `SpaceMove(...)` |
| `70-141` | 药品恢复 | `IncHealth/IncSpell/IncHealing` 按 `dwInChsTime=600-min(400,Level*10)` 消费；`Per*=Level/10+5`/5；`IncHealthSpell` ⇒ 53 |
| `143-157` | 个人定时器 | `AutoTimerStatus[0..19]`：`>500` 且到点 ⇒ `@OnTimer<i>` |
| `159-161` | 魔法盾 | `StatusTimeArr[STATETRANSPARENT]==0` ⇒ `AbilMagBubbleDefence=false` |
| `163-226` | `ExtraAbil` 过期 | 到点清 7 项 + `HealthSpellChanged()`（`boNeedRecalc` 实际调的是 `HealthSpellChanged`，不是 `RecalcAbilitys`）；到期前 10s 发提示 |
| `228` | `PlaySuperRock()` | 挂机/自动 |
| `229-234` | `IsTimeGoto` | 到点 `GotoLable(TimeGotoLable)` |
| `235-239` | 挂机踢线 | `OffLineFlag && now>KickOffLineTick` ⇒ `BoSoftClose=true` |
| `240-248` | 延时脚本 | `IsDelayCall` 到点 ⇒ 查 NPC `GotoLable(DelayCallLabel)` |
| `249-256` | PK 值衰减 | `> DecPkPointTime(2min)` ⇒ `DecPkPoint(DecPkPointCount)` |
| `257-262` | 灯/ PK 色 | `> DecLightItemDrugTime(500ms)` ⇒ `DecLightItemDrugTick += 周期; UseLamp(); CheckPkStatus();` |
| `263-285` | 重叠推挤 | `>3s`：`GetStartPoint` + `GetXYObjCount`；≥2 记录 `BoDuplication/DupStartTick`；满足 3s/10s/20s 窗口 ⇒ `CharPushed(RandomByte(8),1)` |
| `286-290` | 攻城区 | `InCastleWarArea && UnderWar` ⇒ `ChangePkStatus(true)` |
| `291-349` | 夜间折扣/安全区/攻城 | 1s 门：`LogonTimcCost`、安全区变化 `RefNameColor`、`GetCastle`、`ChangePkStatus(false)`、`RefUserState/RefShowName` |
| `356-365` | 消息泵 | 同 1.2 的 `GetMessage` 语义，但受 `HumanGetMsgTime(200)` 时间预算；`Operate` 返回 false 提前退出 |
| `367-400` | 关/踢 | `BoEmergencyClose/BoKickFlag/BoSoftClose` ⇒ 切图数据/`MakeGhost`/`SM_OUTOFCONNECTION`/公会退会；异常且 `wIdent==0` ⇒ `MakeGhost` |
| `403-440` | 装备需求校验 | 每件 `UseItems` 不满足 ⇒ 回包/落地 + `RecalcAbilitys` |
| `443-529` | 游戏币/地图旗标 | `BoDec/IncGameGold` 定时；`Envir.Flag` 的 `boDECGAMEGOLD/boINCGAMEGOLD` |
| `530-547` | 3v3 战场 | 死亡扣分/击杀加分 |
| `548-568` | 游戏点 | `boINCGAMEPOINT` 定时 |
| `569-594` | 地图 HP 事件 | `boDECHP/boINCHP` 定时改 HP |
| `596-634` | 饥饿 | 1s 门；`HungerStatus--`；`HungerDecHP` 扣血/死 |
| `636-657` | 经验/攻击倍率 | 1s 门：`KillMonExpRateTime--`、`PowerRateTime--`，归零恢复 100 + SysMsg |
| `658-668` | 转生变色 | `ReNewChangeColor` 轮换 `NameColor` + `RefNameColor` |
| `670-676` | 私聊对象 | `WhisperHuman` 死/幽灵 ⇒ 清空 |
| `677-737` | 30s 失效清理 | `DearHuman`、`MasterList/MasterHuman`、`GroupOwner/GroupMembers`、`DealCreat` |
| `738-745` | 自动经验 | `AutoGetExpPoint` 到点且地图/安全区命中 ⇒ `GetExp` |
| `747-763` | 复活戒指 | `HP==0 && (!Death)` + 条件 ⇒ `ItemDamageRevivalRing`、HP 回满、53 |
| `765` | `base.Run()` | 进入 §1.2 |

`PlayObject.SearchViewRange`（`PlayObject.cs:3073-3294`）与基类不同：
- `VisibleItems/VisibleEvents/VisibleActors` 三表打 `Hidden`（`:3075-3086`）；
- **`CellType.Play/Monster/Merchant` 的 60s 过期判断被整段注释**（`:3114-3123`）——玩家扫描**不**淘汰格子项；
- 命中怪时向怪发 `RM_UPDATEVIEWRANGE`（`:3136`）；
- 收尾按可见表顺序发 `RM_DISAPPEAR`/`RM_DEATH|RM_SKELETON`/`RM_TURN`（`:3203-3245`）。

### 1.4 `AnimalObject.Run` / `MonsterObject.Run` / `NormNpc.Run` / `Merchant.Run` / `GuildOfficial.Run`

| 位置 | 语义 |
|---|---|
| `AnimalObject.cs:142-149` | `HolySeize && (now-HolySeizeTick)>HolySeizeInterval` ⇒ `BreakHolySeizeMode()`；再 `base.Run()` |
| `MonsterObject.cs:251-358` | 外层 `CanMove() && !FixedHideMode && !StoneMode`；`Think()` 真 ⇒ `base.Run(); return`；`WalkWaitLocked` 释放；`(now-WalkTick)>WalkSpeed` ⇒ `WalkTick=now; WalkCount++`（超 `WalkStep` ⇒ `WalkWaitLocked`）；`!RunAwayMode` 内：`!NoAttackMode` ⇒ `TargetCret` 有则 `AttackTarget()`（真 ⇒ `base.Run(); return`），无则 `TargetX=-1`（`Mission` 时取任务点）；`Master` 跟随/`SlaveRelax`/`SpaceMove`；`RunAwayTime` 到点复位；`Master.SlaveRelax` ⇒ `base.Run(); return`；`TargetX!=-1` ⇒ `GotoTargetXY`，否则无目标 ⇒ `Wondering`；`CheckRoyalty()`；`CrazyMode` 到点 `BreakCrazyMode()`；`base.Run()` |
| `NormNpc.cs:1604-1611` | `Master != null` ⇒ 清空；`base.Run()` |
| `Merchant.cs:112-171` | 5min `RefillGoods()`；10min `ClearExpreUpgradeListData()`；`Random(50)==0` ⇒ `TurnTo(RandomByte(8))`，否则再 `Random(50)==0` ⇒ `RM_HIT`；攻城战中 `FixedHideMode` 切换 + `RM_DISAPPEAR/RM_HIT`；`BoCanMove` 到点 ⇒ `RM_SPACEMOVE_FIRE + MapRandomMove`；`base.Run()` |
| `GuildOfficial.cs:39-53` | `Random(40)==0` ⇒ `TurnTo(RandomByte(8))`，否则 `Random(30)==0` ⇒ `RM_HIT`；`base.Run()` |
| `src/GameSrv/Npc/Trainer.cs:40-52` | 练功师：`AttackCount>0 && (now-AttackTick)>3s` ⇒ `ProcessSayMsg` 总/平均破坏力并清零；`base.Run()` |
| `src/Modules/RobotSystem/Services/RobotPlayer.Base.cs:18-315` | 假人专用移动/攻击/寻路状态机（`RobotPlayer : PlayObject`）；多数分支 `base.Run(); return`，尾部 `base.Run()`（`:314`）⇒ 公共段会跑，AI 自成一类 |

### 1.5 43 个 `Monster/Monsters/*` 的 `override void Run`

全部 43 个函数体均已实测（`grep -n "public override void Run()"` + 花括号配对）；行号区间为函数体（含签名）实测范围。

| ID | 文件:行 | 与父类 `Run` 的差异 |
|---|---|---|
| R01 | `ATMonster.cs:13-24` | `CanMove` 且 `(now-SearchEnemyTick)>8000 \|\| (>1000 && TargetCret==null)` ⇒ `SearchTarget()`；`base.Run()`。继承此 `Run` 但**不在** `is_active_seek_race` 的 race：82/84/90/91/97/130（Rust 对这 6 个 race 不索敌） |
| R02 | `ArcherGuard.cs:39-91` | 每 `WalkSpeed` 扫 `VisibleActors` 取最近 `IsProperTarget` 目标远程攻击；`base.Run()` |
| R03 | `BeeQueen.cs:47-74` | 每 `WalkSpeed`：`SearchTarget` + `MakeChildBee()`；清 `BeeList` 死亡项；`base.Run()` |
| R04 | `BigHeartMonster.cs:44-54` | `VisibleActors.Count>0` ⇒ `AttackTarget()`；`base.Run()` |
| R05 | `BoneKingMonster.cs:48-76` | 8s/1s 索敌 + `DangerLevel` 机制 `CallSlave()`；清 `SlaveObjectList`；`base.Run()` |
| R06 | `CastleDoor.cs:84-104` | 死亡且 `Castle != null` ⇒ 刷新 `DeathTick`；否则 `HealthTick=0`；`!IsOpened` 时按 HP 比例 `Dir=3-Round(HP/MaxHP*3)`，`Dir != n08 && n08 < 3` ⇒ 改 `Dir` 并 `RM_TURN`；`base.Run()` |
| R07 | `CentipedeKingMonster.cs:91-143` | `FixedHideMode` 下 10s 扫 3 格内目标 `Turned/WalkTick+=1000`；`base.Run()` |
| R08 | `ChickenDeer.cs:13-63` | 每 `WalkSpeed` 扫 `VisibleActors` 找最近目标并逃跑/移动；`base.Run()` |
| R09 | `Clone.cs:62-88` | 8s/1s 索敌；近身 4 格 `GetBackPosition`；6 格内 `LightingAttack`；`Random(3)`；`base.Run()` |
| R10 | `CowKingMonster.cs:40-94` | 30s `JumpTime`：`SiegeLockCount>=5` ⇒ `SpaceMove/MapRandomMove`；`CrazyCount`/`CrazyReadyMode`；`base.Run()` |
| R11 | `DefendMonster.cs:11-14` | 只 `base.Run()` |
| R12 | `DigOutZombi.cs:18-55` | `FixedHideMode` 下 3 格内目标 ⇒ `Turned(); WalkTick=now+1000`；`base.Run()` |
| R13 | `DualAxeMonster.cs:84-137` | 5s 扫最近目标；`Random` 取数；`base.Run()` |
| R14 | `ElectronicScolpionMon.cs:33-70` | HP<Max/2 ⇒ `UseMagic=true`，否则 false；索敌门仅 `(now-SearchEnemyTick)>1000 && TargetCret==null`（**无 8s 分支**）；目标 `\|dx\|<=2 && \|dy\|<=2` 且 `(UseMagic \|\| dx==2 \|\| dy==2)` ⇒ `NextHitTime` 门内 `LightingAttack`；`base.Run()` |
| R15 | `ElfMonster.cs:43-86` | 首次 `RM_DIGUP` + `ResetElfMon`；死亡 2s `MakeGhost`；目标/主人联动；`base.Run()` |
| R16 | `ElfWarriorMonster.cs:112-166` | 同上 + 主人目标联动/`MakeGhost`；`base.Run()` |
| R17 | `ExplosionSpider.cs:79-90` | 60s ⇒ `DoSelfExplosion()`；`base.Run()` |
| R18 | `FireMonster.cs:23-82` | 每 tick 在自身周围铺 **9 个** `FireBurnEvent`（`ny-1`、`ny-2`、`nx-1`、`nx-2`、`nx`（自身格）、`nx+1`、`nx+2`、`ny+1`、`ny+2`；各格无既有事件才加）；`base.Run()` |
| R19 | `FireballMonster.cs:14-62` | 目标可命中 ⇒ `Random(DC 差+1)` 取 `nPower` + `StruckDamage` + `RM_SPELL`；`base.Run()` |
| R20 | `FrostTiger.cs:14-36` | 无目标 ⇒ `MagMakePrivateTransparent(this,180)`，有目标清透明；8s/1s 索敌；`base.Run()` |
| R21 | `GasMothMonster.cs:24-35` | 每 `WalkSpeed`：8s/1s 门 ⇒ `GasMothAttack()`；`base.Run()` |
| R22 | `GreenMonster.cs:13-36` | 邻接对角 ⇒ `Random(AntiPoison+7)<=6` ⇒ `MakePosion(DECHEALTH,30,1)`；8s/1s 索敌；`base.Run()` |
| R23 | `Khazard.cs:13-59` | 对角 2 格 ⇒ `Random(2)==0` ⇒ `SpaceMove` + `Random(1)==0 && Random(AntiPoison+7)<=6` ⇒ 绿毒；`base.Run()` |
| R24 | `LightingZombi.cs:32-58` | 8s/1s 索敌；近身 `Random(3)`；6 格内 `LightingAttack`；`base.Run()` |
| R25 | `MagicMonObject.cs:23-60` | HP<Max/2 ⇒ `UseMagic`；1s 索敌；`Master==null` 提前返回；主人联动；`base.Run()` |
| R26 | `MagicMonster.cs:84-186` | `Run` 体与父类 `MonsterObject.Run` 的移动骨架逐句相同（`Think`/`WalkWait`/`WalkSpeed`/`AttackTarget`/`Master` 跟随/`RunAway`/`GotoTargetXY`/`Wondering`），但**尾部只 `base.Run()`**：不调 `CheckRoyalty()`、无 `CrazyMode` 到期；`Run` 内**没有**额外魔法支（魔法在 `AttackTarget` 等处） |
| R27 | `MinorNumaObject.cs:12-23` | 未死亡则 8s/1s 索敌；`base.Run()` |
| R28 | `PercentMonster.cs:79-181` | 同 R26：移动骨架与父类逐句相同，尾部只 `base.Run()`（不调 `CheckRoyalty()`、无 `CrazyMode` 到期）；`Run` 内**没有**百分比魔法支（百分比在 `AttackTarget` 等处） |
| R29 | `RedMonster.cs:13-36` | 邻接对角 ⇒ `Random(AntiPoison+7)<=6` ⇒ `MakePosion(DAMAGEARMOR,30,1)`；8s/1s 索敌；`base.Run()` |
| R30 | `RockManObject.cs:10-13` | 只 `base.Run()` |
| R31 | `RonObject.cs:35-51` | 8s/1s 索敌；6 格内到点 ⇒ `AroundAttack()`；`base.Run()` |
| R32 | `RunAway.cs:12-64` | HP≤半 ⇒ `SpaceMove(nx-2,ny-2)`；HP≥半恢复；`WalkTo`/`Random` 取数；`base.Run()` |
| R33 | `SandMobObject.cs:14-57` | `FixedHideMode` 下 HP>Max/20 且 `CheckComeOut` ⇒ 出场；HP<Max/20 且 3s ⇒ `ComeDown`；目标远 15 格重置；`base.Run()` |
| R34 | `ScultureKingMonster.cs:61-115` | `StoneMode` 下 2 格内目标 ⇒ `MeltStone()`；`base.Run()` |
| R35 | `ScultureMonster.cs:43-79` | 同 R34 的 `MeltStoneAll()`；`base.Run()` |
| R36 | `SoccerBall.cs:19-74` | 足球运动：按 `Dir` 反弹 + `N550` 计数；`base.Run()` |
| R37 | `SpiderHouseMonster.cs:51-78` | 每 `WalkSpeed`：到点 `SearchTarget` + `GenBb()`；清 `_bbList`；`base.Run()` |
| R38 | `StickMonster.cs:101-149` | `FixedHideMode` ⇒ `CheckComeOut/FindAttackTarget`；否则 `SearchTarget` + 目标距离判定；`base.Run()` |
| R39 | `SuperGuard.cs:68-115` | 清 `Master`；每 `NextHitTime` 扫 `VisibleActors` 过滤 `CanAttckTarget`；对 PvP 玩家/红名攻击；`base.Run()` |
| R40 | `TeleMonster.cs:12-31` | 目标距 >5 格 ⇒ `RM_SPACEMOVE_FIRE + SpaceMove`；8s/1s 索敌；`base.Run()` |
| R41 | `WallStructure.cs:47-82` | 死亡 ⇒ `DeathTick=now` + 首次 `SetMapXyFlag(true)`；否则 `HealthTick=0` + 首次 `SetMapXyFlag(false)`；然后按 HP 比例 `Dir=3-Round(HP/MaxHP*3)`（HP=0 ⇒ 4），`Dir != n08 && n08 < 5` ⇒ 改 `Dir` 并 `RM_DIGUP`；`base.Run()` |
| R42 | `WhiteSkeleton.cs:29-39` | 首次 `Dir=5; FixedHideMode=false; RM_DIGUP`；`base.Run()` |
| R43 | `ZilKinZombi.cs:35-47` | 死亡且非 Ghost/非 Stone/`ZilKillCount>=0`/有可见者/`ReAliveTick` 到点 ⇒ 半血 `ReAlive()`；`base.Run()` |

### 1.6 `Operate` 覆写（M2Server 范围实测 9 个；全仓 `src/` 共 11 个）

> 范围声明：本节原表只清点 `src/M2Server/**`（9 个）。全仓另有 `src/GameSrv/Npc/Trainer.cs:20`（练功师）与 `src/Modules/RobotSystem/Services/RobotPlayer.cs:1134`（假人）两处，现补为 O10/O11。注：复验意见提到「RobotSystem 的两处 Operate」，实测 `RobotPlayer.Base.cs:18` 是 `public override void Run()`（非 `Operate`），`src/Modules/RobotSystem/Services/RobotPlayer.cs:1134` 才是 `Operate`；`RobotPlayer.Run` 已补入 §1.4，`Operate` 入本节 O11。

| ID | 位置 | 语义 |
|---|---|---|
| O01 | `AnimalObject.cs:114-140` | `RM_STRUCK` ⇒ `Struck`；`RM_MAKEHOLYSEIZEMODE` ⇒ `OpenHolySeizeMode(wParam)`；其余 `base.Operate` |
| O02 | `MonsterObject.cs:177-185` | `RM_UPDATEVIEWRANGE` ⇒ `UpdateMonsterVisible(wParam)`（`SearchTime` 门；玩家/宝宝 ⇒ `IsVisibleActive=true`） |
| O03 | `BeeQueen.cs:33-45` | `RM_ZEN_BEE` ⇒ `RegenMonsterByName(Bee)` + `SetTargetCreat` + `BeeList.Add` |
| O04 | `Clone.cs:14-35` | `RM_STRUCK/RM_MAGSTRUCK/RM_SPELL` ⇒ 扣 `Master.WAbil.MP`，MP≤0 ⇒ 自身 HP=0；**恒 `return false`**（中断消息泵） |
| O05 | `SpiderHouseMonster.cs:32-49` | `RM_ZEN_BEE` ⇒ 下方一格 `RegenMonsterByName(Spider)` + `SetTargetCreat` + `_bbList.Add` |
| O06 | `StickMonster.cs:96-99` | 纯 `base.Operate` |
| O07 | `SuperGuard.cs:58-61` | 纯 `base.Operate` |
| O08 | `PlayCloneObject.cs:22-25` | 纯 `base.Operate` |
| O09 | `PlayObject.Message.cs:768-2127` | 巨型客户端消息分派（`CM_*`/`RM_*`），含移动/攻击/施法/交易/公会/组队/商店/任务等 |
| O10 | `src/GameSrv/Npc/Trainer.cs:20-38` | `RM_STRUCK`/`RM_MAGSTRUCK` ⇒ 累计 `AttackPower/AttackCount` 并回话；`RM_MAGSTRUCK` 再 `base.Operate`；其余只返回 false（不落基类） |
| O11 | `src/Modules/RobotSystem/Services/RobotPlayer.cs:1134-1179` | `RM_STRUCK` ⇒ `SetPkFlag`/`SetLastHiter`/`Struck`/攻城犯罪标记/清 `HealthTick/SpellTick` 并 `PerHealth/PerSpell -= 1`；其余 `base.Operate` |

---

## 2. Rust 对照（覆盖矩阵）

状态口径：**已实现** = 语义/取数/门序均有等价物；**部分** = 主路径有、分支/帧面/节拍缺；**缺失** = 无等价物；**不适用** = oracle 该分支在本项目生产配置下不可达。
「后果」只写可观测面：`RNG`（取数序列/次数）、`帧`（线上字节/条数/顺序）、`时序`（何时发生）、`状态`（HP/MP/标志/DB）。

### 2.1 驱动与节拍

| ID | oracle（位置/语义） | Rust 符号/文件 | 状态 | 可观测后果 | 级 |
|---|---|---|---|---|---|
| D01 | `UserProcessor` 50ms（`UserProcessor.cs:7`） | `bins/game-svr/src/main.rs` sim 循环（`TICK_INTERVAL_MS=200`）+ `stamina.rs::RUN_INTERVAL_MS` 50ms 重放 | 部分 | 只有恢复/buff 按 50ms 重放；其余「每 Run」效果按 200ms 或缺失；真实玩家 oracle `Run` 名义上 300ms、抖动 250–300ms（见 D02） | R |
| D02 | 逐玩家 `RunTime=250` 门（`WorldServer.cs:645`、`PlayObject.cs:899`） | 无（玩家无 `Run`） | 缺失 | oracle 真实玩家名义上每 300ms 一次（抖动 250–300ms）；Rust 无逐玩家节拍 ⇒ 所有无时间门的每-Run 语义频率错 | R |
| D03 | 逐玩家 `SearchTime=1000` 门（`WorldServer.cs:663`、`PlayObject.cs:900`） | `World::tick` 的 AOI 段每 200ms 对每实体 `search_view_range` | 缺失 | 出现/消失帧最多提前 850ms；扫描频率 5/s vs ≈0.95/s（5.25×） | R |
| D04 | 逐怪 `RunTime=250` 门（`WorldServer.MonGen.cs:280`） | `monster_ai_tick` 每 tick 无条件跑所有怪 | 缺失 | AI/`Think`/`CheckRoyalty` 评估 5/s vs ≈4/s（+25%）；`Wondering` 受 `walk_due` 门限 ⇒ ≤2.5/s vs 可见怪 ≈4/s（Rust 反而更慢） | R |
| D05 | `ProcessMonsterInterval=2` 可见性降频（`WorldServer.MonGen.cs:294-299`） | `Entity::is_visible_active` 只写不读；`process_monster_interval` 只加载不消费 | 缺失 | 不可见怪 AI/`Think`/`CheckRoyalty` 评估 ≈5/s vs ≈1.3/s（~3.75× 评估频率）；`Wondering` 仍受 `walk_due`/`walk_speed`（下限 200ms）门限 ⇒ ≤2.5/s，**不是** 3.75× RNG 抽取 | R |
| D06 | `ActorBuffProcessor` 50ms（`ActorBuffProcessor.cs:7`） | `World::buff_tick` 每 200ms 跑 4 个 pass（`buff.rs::BUFF_PASSES_PER_TICK`） | 已实现 | 单 pass 语义等价；跨线程相对次序不可复现（登记 U-A2） | R |
| D07 | `MerchantProcessor` 200ms 先 NPC 后商人（`MerchantProcessor.cs:7`、`:31-32`） | `World::npc_tick` 后 `World::merchant_tick`（`world.rs` tick 次序） | 已实现 | 节拍与 RNG 消费者次序对齐 | R |
| D08 | `GetMessage` 延时头阻塞整泵（`BaseObject.Message.cs:334-355`） | `slave.rs::deliver_self_delay_msgs` 扫描全部到点项并继续 | 部分 | 延时消息不再阻塞其后的普通消息 ⇒ 出队序/帧序可能不同 | R |
| D09 | 多线程：玩家/怪物/NPC/商人/buff 各自线程 | 单线程 tick 串行（设计硬约束①） | 部分 | oracle 跨线程交错不可复现；仅同线程内次序可判（已登记） | R |

### 2.2 `BaseObject.Run`

| ID | oracle（行/语义） | Rust 符号/文件 | 状态 | 可观测后果 | 级 |
|---|---|---|---|---|---|
| B01 | `27-31` 消息泵（优先级队列 + 延时头中止） | `monai.rs::base_object_run_monster` 只出队 `pending_struck`；玩家走 `bins/game-svr/src/inbound.rs` 命令 FIFO + `slave.rs::deliver_self_delay_msgs` | 部分 | 怪物只支持 `RM_STRUCK` 一类消息；玩家消息按到达序而非 `wIdent` 优先级；延时头不阻塞 | R |
| B02 | `32-36` `SuperMan` 回满 | `monai.rs::base_object_run_monster`（消息泵之后、`!Death` 之前；**2026-10-21 建模**） | 部分 | **race 120 怪物路径已建模**（可达性口径见本表下方注）⇒ 每 `Run` HP/MP 回满（构造级用例 `soccer_ball_super_man_refills_hp_each_base_run`）；**NormNpc 派生（守卫 11/12/20/112、城门 110、城墙 111、固定 NPC/商人）的 `SuperMan=true` 与 `BaseObject.Die` 首行 `if (SuperMan) return;` 早退仍未建模（另卡）** ⇒ 这些实体在 Rust 侧可被击杀、累计掉血，与 oracle 相反 | R |
| B03 | `37-42` 恢复 tick 折算 | `stamina.rs::stamina_tick`（玩家）、`monai.rs::base_object_run_monster`（怪） | 已实现 | 怪物 50/s 与 oracle 同；**玩家 Rust 40/s vs oracle 名义 50/s（未决；抖动区间 40–50/s）** | R |
| B04 | `44-56` HP 回填 `MaxHP/75+1` + 53 | 同上 | 已实现 | 帧内容/次序对齐（`drain_stamina` 按快照逐条发） | R |
| B05 | `57-69` MP 回填 `MaxMP/18+1` + 53 | 同上 | 已实现 | 同上 | R |
| B06 | `70-73` `HP==0 ⇒ Die()` | `world.rs::settle_deaths`（同 tick 统一结算） | 部分 | 死亡时点由「下一 Run」提前到「同 tick 末」；死亡帧/经验/退组次序可能提前 ≤300ms | R |
| B07 | `74-81` 到量归零 | `stamina_tick`、`base_object_run_monster` | 已实现 | — | — |
| B08 | `83-101` 死亡 `MakeGhost`（`MakeGhostTime=3min`/`ZenTime-20s`/`CanReAlive`） | **已实现**（见状态回写） | 已实现 | 怪物尸体永不 Ghost/移除；`GhostTick` 5min 清理不可达；`CanReAlive` 复活不可达；AOI 永远保留尸体格项 | R |
| B09 | `105-110` 体力点负值扣血 | `stamina_tick`、`base_object_run_monster` | 已实现 | — | — |
| B10 | `112-118` `TargetCret` 清理 | `monai.rs::target_cret_cleanup`（仅怪物/守卫） | 部分 | 玩家 `ai.target_creature` 无 30s/死/距>15 清理 ⇒ 目标态滞留（影响后续攻击方向/技能目标） | L |
| B11 | `119-124` `LastHiter` 30s | `world.rs::expire_hitters` | 已实现 | 全局每 tick 统一清，等价 | — |
| B12 | `126-131` `ExpHitter` 6s | `world.rs::expire_hitters` | 已实现 | 同上 | — |
| B13 | `133-207` `Master` 支（`NoItem`/`AutoChangeColor`/`FixColor`） | `slave.rs` 有宝宝/叛变，无变色轮换 | 缺失 | 宝宝颜色/状态位不轮换 ⇒ 657 帧缺失；`NoItem` 语义未置 | L |
| B14 | `209-217` `SlaveList` 清理 | `slave.rs::slave_list_cleanup_all`（`World::tick` 统一跑） | 已实现 | 次序与 oracle 同为「死亡结算后、AOI 前」 | — |
| B15 | `219-221` `ShowHp` ⇒ `BreakOpenHealth()` | `World::cast_show_hp_spell`（`world.rs`）已在 `bins/game-svr/src/main.rs::handle_spell_cmd`（`SKILL_SHOWHP=28`）接线；`make_open_health` 走生产下行（`crates/world/src/slave.rs::make_open_health`，宿主出帧 `bins/game-svr/src/main.rs::push_open_health`） | **部分**（已接线，缺 Run 周期段） | 缺的只是 `(now-ShowHpTick)>ShowHpInterval ⇒ BreakOpenHealth()` 的周期刷新；施法即时 `1100`/`OpenHealth` 下行已存在 | L |
| B16 | `223-246` `VerifyTick` 30s：`VerifyMapTime` + HP/MP 超上限夹 `Max-1` | **已修复（`8b4f5df5e`）**：`World::verify_map_time`（30s 严格大于门）接线到 `monai.rs::base_object_run_monster` 尾部/死亡支、`npc.rs::npc_run`、`merchant.rs::merchant_run`、`World::verify_players_map_time`（AOI 前）；`aoi.rs` 玩家 viewer 跳过 60s 淘汰 | **已实现（VerifyMapTime）** | 静止 actor 不再被 60s 逐出；**余量**：HP/MP 超上限夹 `Max-1` + `HealthSpellChanged()` 仍未建模 | R |
| B17 | `247-315` 状态秒数/毒伤（整段注释） | 不实现（`buff.rs` 在正确位置实现） | 不适用 | oracle 死代码 | — |

> **B02 可达性口径（2026-10-21 复核写死）**：本行的「可达」只指「**可由 GM/脚本按名创建**」——仓库数据 `sql/mir2_data.sql:573` 有 `热血足球`（Race=120）行，`CreateMonster` 有 `case 120 ⇒ new SoccerBall()`（`src/GameSrv/Word/WorldServer.MonGen.cs:683`）；部署 `MonGen.txt` **无** race 120 映射（证据来源：`design-monster-run-overrides-20261017.md` §2.4 的在线只读实测——207 个名字 → 31 个 race，不含 120；`plan-run-gap-batches-20261018.md` §2.2 的「数据不可达」同为此口径）。本次复核**未连库**，仓库 SQL 快照与部署库是否一致未实测。

### 2.3 `AnimalObject.Run` / `MonsterObject.Run`

| ID | oracle（行/语义） | Rust 符号/文件 | 状态 | 可观测后果 | 级 |
|---|---|---|---|---|---|
| A01 | `AnimalObject.cs:142-149` `HolySeize` 到点 `BreakHolySeizeMode()` | `entity.rs` 有 `holy_seize*` 字段；**已实现（`58da49071`）**：`monai.rs::animal_run` 到期清除（严格 `>`） | 已实现 | 被定身的怪若无人攻击则**永久不动**（不 `Wondering`/不攻击）⇒ AI 帧与 RNG 流长期停摆 | R |
| A02 | `AnimalObject.cs:114-140` `Operate`：`RM_STRUCK` / `RM_MAKEHOLYSEIZEMODE` | `monai.rs::animal_struck`（struck 已实现）；`world.rs` 直接写 holy 字段 | 部分 | struck 等价；定身消息的效果体在但到期缺失（见 A01） | R |
| M01 | `MonsterObject.cs:253` 外层 `CanMove() && !FixedHideMode && !StoneMode` | `monai.rs::monster_run` + `can_move` | 已实现 | — | — |
| M02 | `255-258` `Think()` 真 ⇒ `base.Run(); return` | `monai.rs::think` + `monster_run` 提前返回 | 已实现 | `DupMode` 的 `RandomByte(8)` 取数次序对齐；`CheckRoyalty` 跳过也对齐 | R |
| M03 | `260-266` `WalkWaitLocked` 到点释放 | `monster_run` | 已实现 | — | — |
| M04 | `267-277` `WalkTick/WalkSpeed/WalkCount/WalkStep/WalkWait` | `monster_run` | 已实现 | — | R |
| M05 | `277-298` `!RunAwayMode` + `!NoAttackMode` + `TargetCret` / `TargetX=-1` / `Mission` | `monster_run`（无 `Mission`） | 部分 | 无目标时 `TargetX=-1` 对齐；`Mission`/任务点未建模 ⇒ 任务怪不走任务点 | L |
| M06 | `299-324` `Master` 跟随、20 格 `SpaceMove`、`SlaveRelax` | **已实现**（见状态回写） | 已实现 | 宝宝不跟随/不回归主人；主人跨图不召唤 ⇒ 位置/帧/时序差 | R |
| M07 | `326-333` `RunAwayTime` 到点复位 | `monster_run` | 已实现 | — | — |
| M08 | `334-338` `Master.SlaveRelax` ⇒ `base.Run(); return` | **已实现**（见状态回写） | 已实现 | 与 M06 同族；`SlaveRelax` 下 AI 应整段跳过 | R |
| M09 | `339-349` `TargetX!=-1 ⇒ GotoTargetXY`，否则 `TargetCret==null ⇒ Wondering` | `monai.rs::goto_target_xy` / `wondering` | 已实现 | 取数次序见 monai 模块头（`Random(3)` 无条件、`Random(20)`→`Random(4)`） | R |
| M10 | `352` `CheckRoyalty()` | `slave.rs::check_royalty`（各出口按 oracle 调/跳过） | 已实现 | — | R |
| M11 | `353-356` `CrazyMode` 到点 `BreakCrazyMode()` | **已实现（`58da49071`）**：`entity.rs` 新增实体级 `crazy_mode/tick/interval`，`monai.rs::crazy_mode_expiry` 接三处非提前返回出口 | 已实现 | **狂暴态永不进入**（`OpenCrazyMode` 落状态缺失）⇒ 攻速/伤害不会进入狂暴修正 | R |
| M12 | `357` `base.Run()` | `monster_ai_tick` 在 `monster_run` 后调 `base_object_run_monster` | 已实现 | 所有出口都跑一次，与 oracle 同 | — |
| M13 | `280` + `294-299` 每 actor `RunTick/RunTime` + `ProcessRunCount` 降频 | 无（全局 200ms） | 缺失 | 见 D04/D05：AI/`Think`/`CheckRoyalty` 评估频率 ~1.25×/~3.75×；`Wondering` 抽数 ≤2.5/s（oracle 可见怪 ~4/s，Rust 更慢） | R |

### 2.4 43 个专用怪 `Run` 覆写

Rust 侧没有按 race 分派专用类的机制：`monster_ai_tick` 只把 `is_active_seek_race`（race 53/81/83/86/88/89）走「额外索敌」，`is_guard_race`（11/12/20/112）走「静止 + `base.Run`」，其余全部走 `monster_run` 普通路径。因此下表除 R01/R11/R30 外均为「专用行为未建模」。

| ID | oracle | Rust 对应 | 状态 | 可观测后果 | 级 |
|---|---|---|---|---|---|
| R01 | `ATMonster` 8s/1s 索敌 | `monster_run` 的 `is_active_seek_race` 分支 + `search_target` | 部分 | 53/81/83/86/88/89 六个 race 对齐；继承 `ATMonster.Run` 但不在该集合的 race（82/84/90/91/97/130）不索敌 | R |
| R02 | `ArcherGuard` 扫可见目标远程攻击 | 无（守卫被 `monster_ai_tick` 排除，只跑 `base_object_run_monster`） | 缺失 | 弓箭守卫完全不攻击 ⇒ 无 `RM_HIT/SM_STRUCK`，玩家可安全穿过 | R |
| R03 | `BeeQueen` `MakeChildBee` | 无 | 缺失 | 不产小怪、`BeeList` 不存在 ⇒ 怪物数量/RNG 差 | R |
| R04 | `BigHeartMonster` 有可见者即 `AttackTarget` | 无 | 缺失 | 不攻击 | R |
| R05 | `BoneKingMonster` `DangerLevel/CallSlave` | 无 | 缺失 | 不召唤、血量机制缺失 | R |
| R06 | `CastleDoor` HP 比例转向 + `RM_TURN` | `castle.rs` 有城门状态机，但无按 HP 的 `Dir/RM_TURN` | 部分 | 城门外观/朝向帧缺 | L |
| R07 | `CentipedeKingMonster` 藏地出土 | 无 | 缺失 | 不出土/不攻击 | R |
| R08 | `ChickenDeer` 逃跑 AI | 无（走 `wondering`） | 缺失 | 鹿/鸡不逃跑，取数序列不同 | R |
| R09 | `Clone` 分身 `LightingAttack`/`Random(3)` | 无 | 缺失 | 分身不攻击；RNG 少抽 | R |
| R10 | `CowKingMonster` 30s 跳/狂暴 | 无 | 缺失 | 不跳跃、不进入 `CrazyReadyMode` | R |
| R11 | `DefendMonster` 只 `base.Run` | `monster_run` + `base_object_run_monster` 对普通怪等价 | 已实现 | — | — |
| R12 | `DigOutZombi` 藏地 3 格出土 | 无 | 缺失 | 不出土 | R |
| R13 | `DualAxeMonster` 5s 索敌 + `Random` | 无 | 缺失 | 不索敌；RNG 少抽 | R |
| R14 | `ElectronicScolpionMon` 半血 `UseMagic` + 2 格内 `LightingAttack` | 无 | 缺失 | 不施法；索敌门（仅 `>1000 && 无目标`，无 8s 分支）与 `(UseMagic \|\| dx==2 \|\| dy==2)` 条件缺 | R |
| R15 | `ElfMonster` 首次 `RM_DIGUP` + 死亡 2s `MakeGhost` | 无 | 缺失 | 无出土帧、死亡尸体不消失 | R |
| R16 | `ElfWarriorMonster` 同上 + 主人联动 | 无 | 缺失 | 同上 | R |
| R17 | `ExplosionSpider` 60s 自爆 | 无 | 缺失 | 不自爆；伤害/帧缺 | R |
| R18 | `FireMonster` 每 tick 在自身周围铺 9 个 `FireBurnEvent` | 无 | 缺失 | 火墙事件不存在（9 格/次，而不是 4 格） | R |
| R19 | `FireballMonster` 魔法弹 | 无 | 缺失 | 不远程攻击；RNG 少抽 | R |
| R20 | `FrostTiger` 隐身 | 无 | 缺失 | 不隐身；可见性差 | R |
| R21 | `GasMothMonster` 毒气 | 无 | 缺失 | 不攻击 | R |
| R22 | `GreenMonster` 绿毒 | 无 | 缺失 | 不上毒 | R |
| R23 | `Khazard` 瞬移 + 毒 | 无 | 缺失 | 不瞬移；RNG 少抽 | R |
| R24 | `LightingZombi` 雷电攻击 + `Random(3)` | 无 | 缺失 | 不攻击；RNG 少抽 | R |
| R25 | `MagicMonObject` 主人联动魔法 | 无 | 缺失 | 不施法 | R |
| R26 | `MagicMonster` Run 体=父类移动骨架，尾部只 `base.Run()`（无 `CheckRoyalty`/`CrazyMode`/魔法支） | `monster_run` 覆盖移动骨架，但尾部恒调 `check_royalty`、无按 race 的覆写差异 | 部分 | 移动/攻击可等价；尾部多跑 `CheckRoyalty` 的差异需按 race 屏蔽；魔法行为不在 Run 内（`AttackTarget` 等） | R |
| R27 | `MinorNumaObject` 8s/1s 索敌 | 取决于 race 是否在 `is_active_seek_race` | 部分 | 非 6 个 race 则不索敌 | R |
| R28 | `PercentMonster` Run 体=父类移动骨架，尾部只 `base.Run()`（无 `CheckRoyalty`/`CrazyMode`/百分比魔法支） | 同上 | 部分 | 同上；百分比语义在 `AttackTarget` 等处，不在 Run | R |
| R29 | `RedMonster` 红毒 | 无 | 缺失 | 不上红毒 | R |
| R30 | `RockManObject` 只 `base.Run` | 普通路径等价 | 已实现 | — | — |
| R31 | `RonObject` `AroundAttack` | 无 | 缺失 | 不群攻 | R |
| R32 | `RunAway` 逃跑模板 | 无 | 缺失 | 不逃跑；`Random` 少抽 | R |
| R33 | `SandMobObject` 出土/入土 | 无 | 缺失 | 状态切换缺 | R |
| R34 | `ScultureKingMonster` `MeltStone` | 无 | 缺失 | 石像不解除 | R |
| R35 | `ScultureMonster` `MeltStoneAll` | 无 | 缺失 | 同上 | R |
| R36 | `SoccerBall` 足球反弹 | 无 | 缺失 | 足球不动 | R |
| R37 | `SpiderHouseMonster` `GenBb` | 无 | 缺失 | 不产小蜘蛛 | R |
| R38 | `StickMonster` 藏地/攻击 | 无 | 缺失 | 行为缺 | R |
| R39 | `SuperGuard` 主动攻击 PvP 玩家 | 无（守卫静止） | 部分 | 超级守卫不还手 ⇒ 战斗数值/帧缺 | R |
| R40 | `TeleMonster` 远距瞬移 | 无 | 缺失 | 不瞬移 | R |
| R41 | `WallStructure` 地图旗标 + 按 HP 比例改 `Dir` 并 `RM_DIGUP` | `castle.rs` 有墙状态机；无 Run 帧面 | 部分 | 城墙 `Dir`/`RM_DIGUP` 帧与旗标时点可能差（不是 HP 广播） | L |
| R42 | `WhiteSkeleton` 首次 `RM_DIGUP` | 无 | 缺失 | 少一条出现前帧（`FixedHideMode` 切换时序） | L |
| R43 | `ZilKinZombi` 半血复活 | 无 | 缺失 | 不复活；HP/经验减半逻辑缺 | R |

### 2.5 `Operate` 覆写

| ID | oracle | Rust 对应 | 状态 | 可观测后果 | 级 |
|---|---|---|---|---|---|
| O01 | `AnimalObject` `RM_STRUCK`⇒`Struck`；`RM_MAKEHOLYSEIZEMODE`⇒`OpenHolySeizeMode` | `monai.rs::animal_struck`；`world.rs` 写 `holy_seize*` | 已实现 | struck 的动物肉量/反击等价；定身到期见 A01 | R |
| O02 | `MonsterObject` `RM_UPDATEVIEWRANGE`⇒`UpdateMonsterVisible`（`SearchTime` 门 + `IsVisibleActive`） | `aoi.rs::update_visible_gay` 直接改 `visible_actors`；`is_visible_active` 未被降频消费 | 部分 | 可见集合能建立；但 `SearchTime` 门与 `ProcessRunCount` 联动缺失（D05） | R |
| O03 | `BeeQueen` `RM_ZEN_BEE` 产小怪 | **已实现**（`f46e4f03f`） | 已实现 | 见 R03 | R |
| O04 | `Clone` 受击扣主人 MP 且恒 `return false` | **已实现**（`f46e4f03f`） | 已实现 | 分身不耗蓝；消息泵中断语义缺 | R |
| O05 | `SpiderHouseMonster` `RM_ZEN_BEE` 产小蜘蛛 | **已实现**（`f46e4f03f`） | 已实现 | 见 R37 | R |
| O06 | `StickMonster` 纯 `base.Operate` | 不适用（无专用类） | 不适用 | 普通怪路径已覆盖 | — |
| O07 | `SuperGuard` 纯 `base.Operate` | 不适用 | 不适用 | 同上 | — |
| O08 | `PlayCloneObject` 纯 `base.Operate` | `crates/world/src/slave.rs::spawn_clone` 已被 `bins/game-svr/src/slave.rs` 生产调用（静态克隆体、展示/`RM_TURN`、格占用） | **部分** | 缺 `PlayCloneObject.Operate` 的受击转发（扣主人 MP、恒 `return false`）与完整 `Run` | L |
| O09 | `PlayObject` 巨型 `CM_*`/`RM_*` 分派（`PlayObject.Message.cs:768-2127`） | `bins/game-svr/src/inbound.rs` 命令映射 + `world.rs` 命令 FIFO + `slave.rs` 延时消息 | 部分 | 覆盖主要 CM（走/跑/攻/法/买卖/交易/组队/公会）；优先级/延时头阻塞/200ms 预算不同（B01/D08） | R |
| O10 | `Trainer`（练功师）`RM_STRUCK`/`RM_MAGSTRUCK` 累计 `AttackPower/AttackCount` 并回话 | 无（`crates/world` 的 NPC 侧只有 `npc_run` 的抽数/朝向，无练功师受击统计） | 缺失 | 练功师不统计破坏力、不回「破坏力/平均值」⇒ 训练交互帧与数值缺 | L |
| O11 | `RobotPlayer`（假人）`RM_STRUCK` ⇒ PK 标记/`Struck`/清 `HealthTick/SpellTick` | 无（`crates/world/src/ctor_fold.rs` 只 fold `RobotObject` 的 ctor 抽数，不建假人实体/AI） | 缺失 | 假人被击的 PK 标记/受击清理语义缺；与 §1.4 `RobotPlayer.Run` 的 AI 缺口同族 | L |

### 2.6 `PlayObject.Run`

| ID | oracle（行/段） | Rust 对应 | 状态 | 可观测后果 | 级 |
|---|---|---|---|---|---|
| P01 | `30-36` 交易取消 | `bins/game-svr/src/main.rs::drain_deal_tick` | 已实现 | 每 tick 跑一次（oracle 名义每 300ms）；抖动不改变结论 | L |
| P02 | `37-51` 计费 `PayMentMode==3` | 无 | 不适用 | 生产配置非 3 | — |
| P03 | `54-59` `FireHitSkill` 20s 过期 | **已实现（`58da49071`）**：`world.rs::expire_player_run_state` 到期清标志 + `SpiritsGone`/`+UFIR` 帧面 | 已实现 | 烈火加成永不过期 ⇒ 后续伤害数值偏高 | R |
| P04 | `60-64` `TwinHitSkill` 60s 过期 | **已实现（`58da49071`）**：同上，`+UTWN` 短包 | 已实现 | 双龙加成不过期 | R |
| P05 | `65-69` `IsTimeRecall` 延时回城 | 魔法系统部分（`script`/`world` 有回城技能） | 部分 | 延时回城时点/帧可能缺 | R |
| P06 | `70-141` 药品恢复 | `bins/game-svr/src/recover.rs::recovery_tick` + `drain_recovery` | 已实现 | 消费节奏/Per* 重算对齐；`IncHealthSpellTick` 初值照 ctor | R |
| P07 | `143-157` 20 个个人定时器 `@OnTimer` | 无 | 缺失 | 脚本定时器永不触发 ⇒ 任务/活动脚本失效 | R |
| P08 | `159-161` 透明结束清 `AbilMagBubbleDefence` | 无 | 缺失 | 魔法盾标志滞留 ⇒ 受伤数值可能偏低 | R |
| P09 | `163-226` `ExtraAbil` 过期 | `world.rs::expire_extra_abil` + `drain_b5_self_deliveries` | 已实现 | 7 项清空 + 提示帧 | R |
| P10 | `228` `PlaySuperRock()` | 无 | 缺失 | 该状态机不运行 | L |
| P11 | `229-234` `IsTimeGoto` | `crates/script` 有 `TimeGoto` 条件/动作 | 部分 | 脚本侧可设置，无 per-tick 到点触发 | L |
| P12 | `235-239` `OffLineFlag` 踢线 | `selloff.rs` / `script` 有挂机标志 | 部分 | 到点软关可能缺 | L |
| P13 | `240-248` `IsDelayCall` | `crates/script` 有 `DelayCall` | 部分 | 到点回调可能缺 | L |
| P14 | `249-256` `DecPkPoint` | 无 | 缺失 | PK 值不随时间下降 ⇒ PK 等级/名字颜色长期偏高 | L |
| P15 | `257-262` `DecLightItemDrug`：`UseLamp` + `CheckPkStatus` | `world.rs::check_pk_status`（`drain_pk_flag_downlink`）；`UseLamp` 无 | 部分 | PvP 标志 60s 到期有；灯耐久不扣/不掉落/无 `RM_LAMPCHANGEDURA` | L |
| P16 | `263-285` `CheckDupObj` 重叠推挤 | **已实现**（见状态回写） | 已实现 | 站重叠不推挤；`RandomByte(8)` 少抽 | R |
| P17 | `286-290` 攻城区 `ChangePkStatus(true)` | `castle.rs` 有攻城态；`world.rs` 有 `check_pk_status` | 部分 | 攻城区强制 PK 标志可能缺 | L |
| P18 | `291-349` 夜间折扣/安全区/攻城获胜/名字颜色 | 零散（`castle.rs` 有 `GetCastle`）；无 1s 聚合门 | 部分 | 安全区变色/半价/攻城结果帧时点差 | L |
| P19 | `356-365` 200ms 消息泵 | 宿主异步命令 FIFO（tick 边界排空） | 部分 | 命令延迟量化 200ms；无优先级队/延时头阻塞 | R |
| P20 | `367-400` 关/踢 `MakeGhost` | `main.rs::drain_kicks`/`drain_soft_close` | 部分 | 当轮落地 vs oracle ≥1 tick；帧序基本对齐 | L |
| P21 | `403-440` 装备需求校验 | 无 | 缺失 | 换装后属性不满足时装备不回包/不 `RecalcAbilitys` | R |
| P22 | `443-529` 游戏币自动增减/地图旗标 | 无 | 缺失 | 计时扣/加币不执行；DB 金币轨迹差 | R |
| P23 | `530-547` 3v3 战场计分 | 无 | 缺失 | 战场比分不动 | L |
| P24 | `548-568` 游戏点 `boINCGAMEPOINT` | 无 | 缺失 | 地图游戏点不增 | L |
| P25 | `569-594` 地图 `boDECHP/boINCHP` | 无 | 缺失 | 地图 HP 事件不生效 ⇒ HP 轨迹差 | R |
| P26 | `596-634` 饥饿系统 | 无（DB 有 `HungerStatus` 列） | 缺失 | 饥饿不递减/不扣血 | R |
| P27 | `636-657` 经验/攻击倍率计时 | `world.rs` 有 `KillMonExpRate` 使用点；无 1s 递减 | 缺失 | 倍率永不过期 ⇒ 经验/攻击数值偏高 | R |
| P28 | `658-668` 转生变色 | 无 | 缺失 | 名字颜色不轮换 | L |
| P29 | `670-676` `WhisperHuman` 清理 | `main.rs` 私聊面未建模（已登记） | 部分 | 私聊目标幽灵后不清空 | L |
| P30 | `677-737` 30s 失效清理（Dear/Master/Group/Deal） | `party.rs`/`world.rs` 有死亡/离线清理；无 30s 聚合 | 部分 | 清理时点由即时事件驱动，可能与 30s 门不同 | L |
| P31 | `738-745` `AutoGetExpPoint` | `crates/script` 可设置 `AutoGetExp`；无 per-tick `GetExp` | 部分 | 自动经验不结算 | R |
| P32 | `747-763` 复活戒指 | 无 | 缺失 | HP=0 不自动复活 ⇒ 死亡态滞留 | R |
| P33 | `765` `base.Run()` | 见 B01-B17 | 部分 | 公共段散在宿主/世界两处 | R |
| P34 | `WorldServer.cs:645-697` 逐玩家 `RunTick/RunTime` + `SearchTime` 门 | 无 | 缺失 | 见 D02/D03 | R |
| P35 | `WorldServer.cs:481-623` 每 50ms 的登录加载/`HumanFreeDelayTime` 清理/`SaveRcd` | `main.rs` 的进世界/`drain_periodic_save` 近似 | 部分 | 登录/存档节拍不同（存档在 Run 后，已对齐） | L |
| P36 | `PlayObject.cs:3073-3294` 玩家扫描**不**淘汰 60s 格子项（`:3114-3123` 整段注释） | `aoi.rs::search_view_range` 以 `viewer_is_play` 分流：玩家 viewer 跳过 `CELL_OBJECT_EXPIRE_MS` 淘汰；非玩家 viewer 保留 | 已实现 | `8b4f5df5e` 后玩家扫描不再逐出正常 actor；保留的 60s 淘汰只作用于怪物/非玩家 viewer 的基类扫描语义 | R |
| P37 | `PlayObject.cs:3247-3287` `VisibleItems/VisibleEvents` 与 `RM_ITEMHIDE` | `aoi.rs` 只有 actor 可见集；地面物走 `ground.rs` 的独立下行 | 缺失 | 地面物/事件的隐藏帧与玩家可见表语义不同 | L |
| P38 | `PlayObject.cs:3136` 向怪发 `RM_UPDATEVIEWRANGE` | `aoi.rs` 直接写目标的 `visible_actors`/`is_visible_active` | 部分 | 省掉消息门 ⇒ 可见性更新时点/降频不同（D05） | R |

### 2.7 固定 NPC / 商人

| ID | oracle | Rust 对应 | 状态 | 可观测后果 | 级 |
|---|---|---|---|---|---|
| N01 | `NormNpc.Run` `Master=null` + `base.Run()`（`NormNpc.cs:1604-1611`） | `npc.rs::npc_tick`/`npc_run` 做抽数/朝向；尾部已接 `world.verify_map_time`（`npc.rs::npc_run` 尾部） | 部分 | **60s 逐出已修复（`8b4f5df5e`）**；仍缺 `base.Run` 的恢复段（`HealthTick`/HP/MP 回填、`TargetCret` 清理） | R |
| N02 | `Merchant.Run` 补货/清经验/转身/攻城隐身/随机移动（`Merchant.cs:112-171`） | `merchant.rs::merchant_run`（refill/clear/turn/`map_random_move`） | 部分 | 主路径与 RNG 次序对齐；攻城 `FixedHideMode` 切换与帧面缺 | L |
| N03 | `GuildOfficial.Run` `Random(40)`/`RandomByte(8)`/`Random(30)` | `npc.rs::guild_official_run_body` | 已实现 | 取数次序对齐 | R |
| N04 | `ProcessNpcs` 200ms + `NpcLimit` 预算断点续跑（`WorldServer.Merchant.cs:120-166`） | `npc.rs::npc_tick`（预算分支丢弃，登记 T-42） | 部分 | 跨 pass 交错不可复现；单 pass 内次序对齐 | L |
| N05 | `ProcessMerchants` 200ms + `NpcLimit`（`WorldServer.Merchant.cs:65-118`） | `merchant.rs::merchant_tick`（预算丢弃） | 部分 | 同上 | L |
| N06 | NPC/商人也走 `base.Run()` 的 `HealthTick/VerifyTick` | `npc.rs::npc_run` / `merchant.rs::merchant_run` 尾部已接 `verify_map_time`；恢复段无 | 部分 | **60s 逐出已修复（`8b4f5df5e`）**；`HealthTick` 恢复/HP 回填仍缺（MCS-15 只余恢复段） | R |

---

## 3. 缺口排序（真机可观测 × R）

排序原则：先「R 级 + 已在真机造成可见差异」→「R 级 + 会改变 RNG/战斗数值」→「R 级 + 仅时序」→「L 级」。

| 序 | 缺口 | 依据 | 真机可观测面 |
|---|---|---|---|
| 1 | **B16 `VerifyTick/VerifyMapTime`（已修复 `8b4f5df5e`）** + **P36 玩家扫描 60s 淘汰（已实现）** | `verify_map_time` 接线到怪物/NPC/商人/玩家等价段；`aoi.rs` 玩家 viewer 跳过淘汰 | 已收口；余量仅 HP/MP 夹 `Max-1` |
| 2 | **D04/D05/M13 怪物每 actor 节拍与可见性降频** | 评估频率 ~1.25×（可见）/~3.75×（不可见）；`Wondering` 只在 `walk_due` 门内抽数 ⇒ ≤2.5/s（oracle 可见怪 ~4/s） | 时序/评估频率；RNG 抽取方向相反（Rust 更慢） |
| 3 | **D01/B03 玩家恢复速率 40/s vs oracle 名义 50/s（未决）** | `RunTime=250`+50ms 定时 ⇒ 名义 300ms/次；.NET 抖动可落 250–300ms ⇒ 40–50/s；`stamina.rs` 40/s 落在区间内 | HP 数值、掉血击次（需带时间戳 dump 裁定） |
| 4 | **D02 玩家无 `Run` 节拍**：`PlayObject.Run` 18 段缺失（P36 已落地） | 见 §2.6 | 帧/状态/数值综合 |
| 5 | **B08 死亡 `MakeGhost` + ReAlive 未接线** | `monster_due_for_revive` 只有谓词；无置 Ghost | 帧（尸体消失/复活）、实体生命周期 |
| 6 | **M06/M08 宝宝跟随/`SlaveRelax`** | 无 Master 分支 | 位置/帧/时序 |
| 7 | **A01 `HolySeize` 到期缺失** | 只写不清 | AI 停摆、RNG 停摆 |
| 8 | **P03/P04 FireHit/TwinHit 过期缺失** | 只写不清 | 战斗数值 |
| 9 | **P27 经验/攻击倍率不过期** | 只写不清 | 经验/伤害数值 |
| 10 | **P16 重叠推挤缺失** | `RandomByte(8)` 不抽 | RNG、位置 |
| 11 | **P21/P22/P25/P26/P31/P32 玩家段缺失** | 见 §2.6 | 状态/HP/DB |
| 12 | **R02/R03/... 专用怪行为**（34 个缺失） | 见 §2.4 | 战斗/帧/RNG |
| 13 | **O09/D08 消息泵优先级与延时头语义** | 见 §2.1/§2.5 | 出队序/帧序 |
| 14 | **B13/B15/O10/O11/P07/P10/P14/P15/P28/P29/P30/P37 等 L 级** | 纯帧/纯状态 | 低（B15 已接线，只缺 Run 周期段；仍按 L） |

---

## 4. 特别核对

### 4.1 `VerifyMapTime` 同族周期门

| 周期门 | oracle 位置 | 周期 | Rust | 结论 |
|---|---|---|---|---|
| `AutoRecoveryTick` | `BaseObject.Base.cs:39-42` | 每 `Run`（玩家名义 300ms、抖动 250–300ms / 怪 ≥250ms） | `stamina_tick` / `base_object_run_monster` | **已实现**；玩家速率待定（§4.3） |
| `VerifyTick` | `BaseObject.Base.cs:223-246` | 30s | `World::verify_map_time` + 怪物/NPC/商人/玩家接线（`8b4f5df5e`） | **已实现**（VerifyMapTime）；HP/MP 夹 `Max-1` 余量 |
| `SearchTick/SearchTime` | `WorldServer.cs:663`（玩家 1000ms）、`WorldServer.MonGen.cs:300`（守卫/宝宝 `SearchTime`） | 1000ms / 2000-5000ms | `World::tick` 每 200ms 全实体扫描 | **调用点/节拍/门全不同**（D03/P38） |
| `DecLightItemDrugTick` | `PlayObject.Message.cs:257-262` | 500ms（`DecLightItemDrugTime`） | `world.rs::check_pk_status` 节流；`UseLamp` 无 | **部分** |
| `DecPkPointTick` | `PlayObject.Message.cs:249-256` | 2min | 无 | 缺失 |
| `CheckDupObjTick` | `PlayObject.Message.cs:263-285` | 3s | 无 | 缺失 |
| `ShowHpTick` | `BaseObject.Base.cs:219-221` | `ShowHpInterval` | `cast_show_hp_spell` + `make_open_health` 已接线；无周期段 | **部分** |
| `PoisoningTick`（毒 DoT） | `M2Server/ActorBuffSystem.cs:233-260` | 2500ms | `buff.rs::do_work` | **已实现**（4 pass/tick） |

### 4.2 `RunTime/RunTick` 每 actor 节拍 vs 全局 tick

- oracle：每个 actor 自带 `RunTick/RunTime`（玩家 250、怪 250、NPC/商人 250），每个 driver 只处理到点者；怪物还叠加 `ProcessRunCount < 2` 的可见性降频。
- Rust：`World::tick` 每 200ms 对**所有**怪物跑 AI、对**所有**实体跑 AOI；只有 `npc.rs`/`merchant.rs` 保留了 `RunTick/RunTime` 门（200ms 节拍 + >250 ⇒ 400ms，与 oracle 200ms 定时器一致），两者尾部均接 `verify_map_time`（`8b4f5df5e`）。
- 结论：**玩家与怪物两条线都没有 per-actor 节拍**。玩家侧后果是「所有每-Run 效果按 200ms 或缺失」；怪物侧后果是 `Think`/`CheckRoyalty` 等**评估频率**系统性偏高（可见 ~1.25×、不可见 ~3.75×），但 `Wondering` 的 RNG 抽取受 `walk_due`/`walk_speed`（下限 200ms、200ms tick、严格 `>`）限制为 ≤2.5/s，低于 oracle 可见怪的 ~4/s。

### 4.3 50ms `UserProcessor` 与 200ms world tick 的量化差

| 项 | oracle | Rust | 差 |
|---|---|---|---|
| 线程节拍 | 50ms | 200ms | 4× |
| 真实玩家 `Run` | **名义 300ms**（50ms × `>250` 量化；`WorldServer.cs:645`），.NET 抖动可落 250–300ms | 无 / 散段 200ms（VerifyMapTime 已接） | 频率 1.5× 或缺失 |
| 真实玩家恢复 | `(250..300)/20 = 12..15` 每 Run ⇒ **40–50/s**（名义 300ms 时 50/s） | 4×50ms 每 tick，+2/步 ⇒ **40/s** | 落在抖动区间内；不能判 -20%（须带时间戳 dump） |
| 怪物恢复 | `(≈250-300)/20 = 12-15` ⇒ 40-50/s | 10/200ms ⇒ 50/s | 依赖 oracle 循环耗时 |
| 玩家视野扫描 | `SearchTime=1000` ⇒ ≈0.95/s | 5/s | 5.25× |
| 怪物 AI | ≥250ms + 不可见 3× 降频 | 200ms | 评估频率 1.25× / 3.75×（非 RNG 抽取次数） |
| Buff pass | 50ms | 4×50ms/200ms tick | pass 数对齐；相对次序不可复现 |
| 消息处理 | 每次 Run 200ms 预算、优先级队列 | tick 边界排空、FIFO | 延迟/次序 |

> **订正记录**：`bins/game-svr/src/stamina.rs` 模块头称「oracle `Run` 每 50ms 一次、每次 +2 = 40/s」。实测 `ProcessPlayObjectData` 有 `(dwCurTick - RunTick) > RunTime(250)` 门，真实玩家名义上是 300ms 一次。40/s 与 50/s 都能解释「第 86 击掉 1 HP」：按 `86*(30-50*I) > 300 ⇒ I < 0.530s`，50/s 假说对应击间隔 ≈530ms（不是 ≈550ms；I=550ms 时 `HealthTick=-215`，第 86 击不会掉血），40/s 假说对应 ≈662ms。另 `300ms/Run` 只是「定时器恰好 50ms」的名义量化，.NET 定时器抖动可落到 250–300ms/Run（40–50/s 区间），所以 40/s 未被证伪。证据文件 `17-attacker-hp-trajectory-csharp-c2.txt` **没有时间戳**。该口径必须由带时间戳的真机 dump 裁定（MCS-8）。

### 4.4 状态 tick（Burning/Poison/Stone）与帧（53/657/100）

- `Burning` 在本 oracle 版本**不存在**（全仓 grep 无 `Burning`）。
- `Stone` 在本 oracle 版本主要来自 `StoneMode`（城墙/雕像），`StatusTimeArr[STONE]` 只由 `FastParalysis` 写，而 `FastParalysis` **全仓无置 true 点**（`BaseObject.cs:461` 只初始化 false）⇒ 死代码。Rust `can_move` 用 `stone_mode`，与 oracle 的 `!StoneMode` 门一致。
- 毒：`ActorBuffSystem.Operate`（50ms）先做玩家状态秒数递减（`boChg ⇒ StatusChanged()` ⇒ 657），再对**所有非死** actor 做绿毒 DoT（`DamageHealth` + `HealthTick/SpellTick=0` + `HealthSpellChanged()` ⇒ 53）。
- Rust 次序：`buff.rs::do_work` 先 `status_changes`（657）后 `health_spell_changes`（53），宿主 `drain_buff_downlink` 按同序发；**100（`SM_SYSMESSAGE`，防御力恢复提示）与 `RM_ABILITY`（`buff_recalcs`）未接线**。
- 时序量化：oracle 每个 50ms pass 都可能递减/DoT；Rust 每 200ms 补 4 个 pass，单 pass 门对齐，但**同一 tick 内 657/53 与玩家 `Run` 帧的相对次序**不再是 oracle 的两条独立线程交错（登记 U-A2，R）。

---

## 5. 机判建议（每类缺口一条，固定时钟、无 RNG 优先）

| 编号 | 针对缺口 | 判据（构造级、可复现） |
|---|---|---|
| MCS-1 | B16/P36/N06（`8b4f5df5e` 已接线，作为回归门） | 固定 `world.clock_ms`；`1x1` 地图放一只静止守卫/商人，`add_map_object` 后把时钟推到 `60_100ms`，调 `World::tick`（AOI 段）后断言：格子 `obj_list` 仍含该 actor 且 `add_time` 已被刷新到 `now`；再断言其进入下一玩家的 `visible_actors` 为 `Show`。删除/未接 `VerifyMapTime` ⇒ 红（现应绿）。 |
| MCS-2 | D04/M13 | 固定时钟，单怪 `target_creature=None/target_x=-1`；在 1s 内按 200ms 推进 `World::tick`，用测试计数器统计 `monster_run` 实际执行次数：oracle 口径应为 4（`>250ms` 门），当前 5 ⇒ 红。 |
| MCS-3 | D05 | **前置未接线：不可直接执行。** `process_monster_interval` 目前只在 `crates/world/src/mongen.rs` 定义/装载，AI 路径（`crates/world/src/monai.rs`）从不消费；`is_visible_active` 也只写不读。须先把「`ProcessRunCount < interval` 才跑 `monster_run`」接进 AI 路径，本判据与「置 0 ⇒ 每次都跑」的对照才有意义（否则两态都是空操作）。接线后：固定时钟，怪 `is_visible_active=false`；连续 3 个到点 pass 只允许 1 次 `monster_run`；interval=0 ⇒ 每次都跑。 |
| MCS-4 | A01 | 固定时钟；置 `holy_seize=true/holy_seize_tick=0/holy_seize_interval=1000`，推进 1001ms 后调 `monster_ai_tick`，断言 `holy_seize=false`（当前恒 true ⇒ 红）。 |
| MCS-5 | B08 | 固定时钟；怪物 `hp=0` 经 `settle_deaths` 置 `death=true`，推进 `MakeGhostTime+1`（3min）后调 `World::tick`，断言 `ghost=true` 且格子项被移除；`CanReAlive` 支再断言 `invisible=true`/`ReAlive`。 |
| MCS-6 | B16 余量 | 固定时钟；实体 `hp=max_hp+5/mp=max_mp+5`，推进 30s 后调 `base_object_run_monster`（或玩家等价段），断言 `hp=max_hp-1/mp=max_mp-1` 且产生一次 53。`VerifyMapTime` 部分已由 MCS-1 覆盖；本判据现在只针对**残余的 HP/MP 夹 `Max-1`**（当前仍红）。 |
| MCS-7 | B10 | **前置入口不存在：不可直接执行。** `target_cret_cleanup` 只由 `crates/world/src/monai.rs::base_object_run_monster` 调用，玩家等价段没有该入口。须先在玩家 `Run` 等价段加入口（或在 `World::tick` 的玩家 pass 中调用）才能执行本判据。入口就绪后：固定时钟；玩家 `ai.target_creature=Some(t)`，把 `target_focus_tick` 设为 `now-30_001`，调该入口，断言目标被清空。 |
| MCS-8 | B03/D02 | 固定时钟；真实玩家按名义 300ms `Run` 下 1s 内 `HealthTick` 增量应为 50（15×3.33），但 .NET 抖动 250–300ms 时对应 40–50/s；`stamina_tick` 当前 40 ⇒ 落在区间内，不能单独判红。需先用带时间戳真机 dump 钉住真实击间隔（≈662ms⇒40/s vs ≈530ms⇒50/s，见 §4.3），再按裁定值断言。 |
| MCS-9 | §4.4 / O02 | 固定时钟；加 `DefensePower` buff（`state_tick=2`），推 2 个 pass，断言 `buff_tick` 输出含 `sysmsgs` 与 `recalcs`，且宿主下行含 `SM_SYSMESSAGE(100)` 与 `RM_ABILITY` 触发的 `SM_ABILITY`；并断言 AC/MAC 在 buff 到期后回落（当前 buff 不参与 `ability.rs` ⇒ 红）。 |
| MCS-10 | P14/P15 | 固定时钟；`PkPoint=100`、`DecPkPointTime=120000`，推进 120_001ms，断言 `PkPoint` 减 `DecPkPointCount`；灯：右持灯 `Dura=5000`，推进 500ms，断言 `Dura` 减 1（当前无 `UseLamp` ⇒ 红）。 |
| MCS-11 | P07 | 固定时钟；注入 `AutoTimerStatus[0]=600`，在 599ms 不触发、601ms 触发一次 `@OnTimer0`（用脚本引擎的调用计数桩）。 |
| MCS-12 | D08/B01 | 构造自身延时队列：头项 `deliver_at=now+1000`，次项 `deliver_at=now`；调等价消息泵一次，断言次项**未**被处理（oracle 的 `GetMessage` 返回 false 中止整泵）；当前 `deliver_self_delay_msgs` 会处理次项 ⇒ 红。 |
| MCS-13 | D03 | 固定时钟；玩家静止，新 actor 在视野外进入；按 200ms 推进，断言出现帧首次出现在 `SearchTick+1050ms` 附近而不是 200ms；`SearchTime` 改写为 1000 的注入面。 |
| MCS-14 | R01-R43 | 按 race 建一张「专用怪行为」表：每个 race 生成 1 只，固定时钟推 1 个 oracle 特征周期（如 R10 的 30s、R17 的 60s、R34 的 2 格接近），断言对应事件（`SpaceMove`/`DoSelfExplosion`/`MeltStone`/`RM_DIGUP`/`GenBb`）出现过；当前 R01/R11/R30 应全绿、R26/R27/R28/R39/R41 允许部分绿，其余应红。 |
| MCS-15 | N01/N06 | 固定时钟；商人 `hp=max_hp-10`，推进 5 分钟 `npc_tick`/`merchant_tick`，断言 HP 按 `HealthFillTime` 回满（oracle 商人走 `base.Run`）。`VerifyMapTime` 已接（`8b4f5df5e`）⇒ 60s 逐出部分已绿；恢复段未接 ⇒ 本判据仍红。 |
| MCS-16 | O03/O05/O04 | 固定时钟；向 BeeQueen/SpiderHouse 投 `RM_ZEN_BEE`，断言宝宝实体被创建且 `target_creature=母体目标`；向 Clone 投 `RM_STRUCK`，断言 `Master.WAbil.MP` 下降且消息泵后续消息未被处理。 |

> 固定时钟优先用 `World::clock_ms` 注入；需要 RNG 的判据才用 `RandomNumber::with_seed`，并把期望的抽取次数写成断言（同 `monai.rs`/`buff.rs` 既有测试口径）。

---

## 6. 残留物清单

- 本文档为唯一交付物；未修改任何源码、SQL、客户端或 `crates/{protocol,storage}`。
- 临时目录：`/e/tmp/runaudit-tools/`（`extract.py`、`run_funcs.json`）与 `/e/tmp/runaudit_overrides.txt`（43 个 Run 覆写的实测摘录）——**仅为审计取证保留**，不属于仓库，可在收尾时删除。
- 本次修订自检：`/e/tmp/rev-audit-check.py`（全文区间校验表）、`/e/tmp/audit-rev.patch`（修订 patch）——临时物，不属于仓库。
- 审计 worktree：`/e/tmp/impl-runaudit`（detached `e9cc6a8fd`）；修订 worktree：`/e/tmp/rev-audit2`（detached `d3e09228e`）；主检出未动。
- 未起服务、未占端口、未写库；`E:/MirServer` 未触碰。
