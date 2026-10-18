# 索敌与攻击判定覆盖审计（oracle ↔ Rust 等价物）

- 审计日期：2026-10-18
- 审计基线：`7e8d47426`（detached worktree `/e/tmp/impl-ataudit`）
- 触发：`plan-run-gap-batches-20261018.md`（MB-1..MB-9）只排了 `Run` 覆写，**明确声明不排 `AttackTarget`**（该件 §0 结论摘要第 4 条，`:23`；§2 节头注，`:189`；§2.3.0，`:269`），而 MB-2/MB-3/MB-6/MB-7/MB-9 的「同批必接」列又点名了若干 `AttackTarget` 覆写。本审计把 oracle 的**索敌（SearchTarget）→ 目标有效性（Think）→ 攻击判定（AttackTarget/_Attack）→ 目标选择优先级**整条链逐条对照 Rust，产出覆盖矩阵与缺口清单，**不实现**。
- 方法：只读源码实读（`src/**`、`mir2-rs/crates/**`、`mir2-rs/bins/**`）+ 既有审计/设计/规划件交叉复核；**未改 `src/**`、`sql/**`、`MirClient/`、`crates/{protocol,storage}/**`、`E:/MirServer/**`；未起服务、未占端口、未写库**。
- 引用纪律：**oracle 行号逐条实测**（本基线上用 `awk`/`sed -n` 逐条重量，§7 自检脚本校验全文所有 `文件:起-止` 区间在文件行数内且首末行与描述对得上）；**Rust 侧只引符号/文件路径，不写 `:行号`**。
- 判级：按 `AGENTS.md`「判据分级」；凡触及 RNG 抽数序列、时序、战斗数值的缺口一律 **R**。

---

## 0. 结论摘要

1. **10 个 `AttackTarget` 覆写全部未建模**（`grep "override.*AttackTarget"` 实测 **12 命中 / 10 条真覆写**（多出的 2 条是 `CentipedeKingMonster.cs:85` 的 `override void FindAttackTarget()` 与 `PlayObject.Attack.cs:627` 的 `override bool IsAttackTarget(...)`；真覆写恰为下列 10 条）：`DoubleCritical` / `DualAxe` / `ElfWarrior` / `ExplosionSpider` / `GasAttackMonster` / `MagCow` / `MagicMonster` / `PercentMonster` / `SpitSpider` / `CentipedeKing`），另有 2 个**虚函数式**自实现（`BigHeartMonster`、`StickMonster`）与 2 个守卫私有攻击入口（`SuperGuard.AttackTarget`、`ArcherGuard.AttackTarger`）。Rust 只有 `monai.rs::attack_target` 一个**基类**等价物；race 82/85/87/90/91/93/104/105/106/107/115/119 落到它上面时，**攻击方式、伤害公式、帧、RNG 抽数全部是基类语义**——不是「少一个特判」，而是**打错**。
2. **这些覆写并非数据不可达**：即使怪物不在 `is_active_seek_race`（不会主动索敌），只要被玩家/宝宝打中，`AnimalObject.Struck`（`src/M2Server/Actor/AnimalObject.cs:336-359`）在 `TargetCret == null` 时**无条件**把命中者设为目标（另有 `Random(6)==0` 支）。Rust `monai.rs::animal_struck` 同样会置 `target_creature`。⇒ 从「第一次挨打」起，上述 race 就进入各自覆写的攻击路径；Rust 走的是基类 `attack_target`。**真机可达 × 每次挨打后必现**。
3. **索敌节拍**：怪物侧**不存在 `SearchTargetTick`**（全仓实测只有机器人 `RobotPlayer.Base.cs:869-876` 与 `RobotPlayer.cs:36` 有该字段）。怪物的索敌节拍来自 `ATMonster.Run`（`src/M2Server/Monster/Monsters/ATMonster.cs:13-24`）的 `SearchEnemyTick` 门（8s / 有目标时 1s）。Rust 用 `is_active_seek_race` 复刻该门，但集合只有 `{53,81,83,86,88,89}`；**继承 `AtMonster.Run` 而未覆写 `Run` 的 82/84/90/91/97/106/118/119/130 共 9 个 race 完全不索敌**（其中 82/84/90/91/97/106/119 真机可达）。
4. **可见性门**是两套机制：oracle 野怪**不自己扫视野**，靠玩家 `SearchViewRange` 发 `RM_UPDATEVIEWRANGE`（`src/M2Server/Player/PlayObject.cs:3129-3139`）由 `MonsterObject.UpdateMonsterVisible` 收下（`src/M2Server/Monster/MonsterObject.cs:136-185`），且**受 `SearchTime` 节流**（`:138-141`）；Rust 的 `World::tick` AOI 段对**所有非玩家 viewer 每 200ms 全量扫一次**（`crates/world/src/world.rs` 的 AOI pass），没有 `RM_UPDATEVIEWRANGE`、没有 `SearchTime` 节流、也没有玩家视野的 `ViewRange - 目标.ViewRange` 距离门。
5. **`SearchTarget` 的清理副作用只搬了一半**：oracle 的无效项分支调 `ClearTargetCreat`（`src/M2Server/Actor/BaseObject.Base.cs:568-585`）⇒ 先移出 `VisibleActors`、再 `DelTargetCreat()`（`TargetCret = null`、`TargetX/Y = -1`）。Rust `monai.rs::search_target` 把无效项从 `visible_actors` 移除，但**不调 `del_target_creature`**。但这不等于「见到无效项就清目标」：oracle 末尾是 `if (searchTarget != null) SetTargetCreat(searchTarget)`（`src/M2Server/Actor/AnimalObject.cs:413-416`），本 pass 只要检查过任一合法候选，收尾就会**重新置目标**。真实差异取决于 `VisibleActors` 顺序与 `for` + `RemoveAt` 的「删后跳过下一项」怪癖（本文 G20/A17）：初始目标为空、列表 `[无效, 合法]` 时，oracle 删掉 index 0 后 `i++` 跳过已前移的合法项，`searchTarget == null` ⇒ **保持无目标**；而 `[合法, 无效]` 两边收尾都是 `Some(合法)`。Rust `retain` 无跳项，前一种顺序下会置 `Some(合法)`。
6. **`IsProperTarget` 口径不同**：oracle `BaseObject.IsProperTarget` 就是 `IsAttackTarget`（`src/M2Server/Actor/BaseObject.Base.cs:470-473`），**不检查 Death/Ghost/Invisible**；Rust `combat.rs::is_proper_target_grouped` 在顶部折入了 `death || ghost || invisible`。`is_proper_target_grouped_direct` 已为魔法路径保留直通口径，但 `monai.rs::search_target/think/animal_struck` 与 `World::attack_once` 用的仍是折入版。
7. **矩阵 57 行**（§3）；**缺口 26 条**（§4，G01–G26），其中 **17 条会改变 RNG 抽数次序或次数**（标 `RNG`）。

---

## 1. Oracle 侧清册

### 1.1 驱动与节拍

| 驱动 | oracle 位置（实测） | 节拍/门 | 说明 |
|---|---|---|---|
| 怪物主循环 | `src/GameSrv/Word/WorldServer.MonGen.cs:192-330` | 每 actor `(now - RunTick) > RunTime(250)`（`:280`） | 连续线程；`RunTick = dwRunTick`（整批开始时刻），不是 now |
| 怪物可见性降频 | `src/GameSrv/Word/WorldServer.MonGen.cs:294-297` | `!IsVisibleActive && ProcessRunCount < ProcessMonsterInterval` ⇒ 跳过 | 缺省 2 ⇒ 不可见怪约 1/3 频率 |
| 怪物主动扫视野 | `src/GameSrv/Word/WorldServer.MonGen.cs:300-313` | 仅 `IsSlave \|\| Race ∈ {Guard, ArcherGuard, SlaveMonster}` 且 `(now - SearchTick) > SearchTime` | **普通野怪不扫**；注释明说改由玩家侧驱动 |
| 玩家主循环 + 视野 | `src/GameSrv/Word/WorldServer.cs:645-668` | `(now - RunTick) > RunTime(250)`；`(now - SearchTick) > SearchTime(1000)` | 玩家扫描后向附近怪物发视野更新 |
| 玩家 → 怪物视野注入 | `src/M2Server/Player/PlayObject.cs:3129-3139` | 过滤通过且目标为 Monster ⇒ `SendMessage(RM_UPDATEVIEWRANGE)` | 距离门 `|Δx|,|Δy| <= ViewRange - 目标.ViewRange` |
| 怪物接收视野 | `src/M2Server/Monster/MonsterObject.cs:177-185` | `RM_UPDATEVIEWRANGE` ⇒ `UpdateMonsterVisible(wParam)`，`return true` | **只有 `MonsterObject` 覆写 `Operate` 处理它** |
| 怪物视野节流 | `src/M2Server/Monster/MonsterObject.cs:136-141` | `(now - SearchTick) <= SearchTime` ⇒ 丢弃 | 每怪 1.5–3.5s 才接受一次视野注入 |
| 守卫扫视野 | `src/M2Server/Monster/Monsters/GuardUnit.cs:127-194` | `(now - SearchTick) > SearchTime`（由 `ProcessMonsters` 驱动，`:300-313`） | 6 格距离门 + `CanPassiveAttack` 过滤 |
| 怪物构造抽数 | `src/M2Server/Actor/AnimalObject.cs:76-78`；`src/M2Server/Monster/MonsterObject.cs:65`；`src/M2Server/Monster/Monsters/ATMonster.cs:9-10` | `Random(3000)`×2、`Random(2000)`、`Random(1500)` | 逐层 ctor；派生类另有 `Random(1500)+base` |
| 机器人 `SearchTargetTick` | `src/Modules/RobotSystem/Services/RobotPlayer.Base.cs:869-876`；字段 `src/Modules/RobotSystem/Services/RobotPlayer.cs:36` | `TargetCret == null \|\| now - SearchTargetTick > 1000` | **仅机器人**；怪物路径无此字段 |

> **结论**：怪物索敌节拍 = `ATMonster.Run` 的 `SearchEnemyTick` 门（§1.3），不是 `SearchTargetTick`。`SearchTarget` 本身**不消费随机数**，它改变的是「有没有目标」这一分支导向，从而间接改变后续 `AttackTarget`/`Wondering` 的抽数。

### 1.2 `AnimalObject.SearchTarget`（`src/M2Server/Actor/AnimalObject.cs:388-417`）逐句

| 行 | 语义 |
|---|---|
| `388-390` | `protected virtual void SearchTarget()`；`searchTarget = null`，`n10 = 999` |
| `392-399` | 遍历 `VisibleActors`（插入序）：`Death \|\| Ghost \|\| 异图 \|\| \|Δx\|>15 \|\| \|Δy\|>15` ⇒ **`ClearTargetCreat(baseObject)` + `continue`** |
| `400-411` | `!Death`（上一支已排除 ⇒ 恒真）且 `IsProperTarget && (!HideMode \|\| CoolEye)` ⇒ 曼哈顿距离 `nC`，`nC < n10` 才更新（**严格小于**，平手保留先出现者） |
| `413-416` | 只有 `searchTarget != null` 才 `SetTargetCreat`；**找不到不清旧目标** |

配套：`SetTargetCreat`（`src/M2Server/Actor/BaseObject.Base.cs:557-561`）写 `TargetCret + TargetFocusTick = now`。
`ClearTargetCreat`（`src/M2Server/Actor/BaseObject.Base.cs:568-585`）从 `VisibleActors` 删首个匹配项 + `DelTargetCreat()`；`AnimalObject.DelTargetCreat`（`src/M2Server/Actor/AnimalObject.cs:151-156`）追加 `TargetX = TargetY = -1`。
`CanMove`（`src/M2Server/Actor/AnimalObject.cs:162-165`）= `!Ghost && !Death && StatusTimeArr[STONE] == 0`。

### 1.3 索敌节拍：`ATMonster.Run`（`src/M2Server/Monster/Monsters/ATMonster.cs:13-24`）

| 行 | 语义 |
|---|---|
| `15-16` | `if (CanMove())` |
| `17` | `(now - SearchEnemyTick) > 8000 \|\| ((now - SearchEnemyTick) > 1000 && TargetCret == null)` |
| `19-20` | 命中 ⇒ `SearchEnemyTick = now; SearchTarget();` |
| `23` | 之后 `base.Run()`（= `MonsterObject.Run`） |

`ATMonster.SlaveAttackTarget`（`src/M2Server/Monster/Monsters/ATMonster.cs:26-68`）另有一套「共享主人 `VisibleActors` 选最近 proper」的实现，末尾 `SetTargetCreat`（`:64`）；`ATMonster` 本体没有 `Run` 覆写的派生类仍继承上面的门。

### 1.4 `MonsterObject.Think`（`src/M2Server/Monster/MonsterObject.cs:187-218`）逐句

| 行 | 语义 |
|---|---|
| `190-192` | `(now - ThinkTick) > 3000` ⇒ 刷新 `ThinkTick` |
| `193-196` | `GetXYObjCount(CurrX,CurrY) >= 2` ⇒ `DupMode = true` |
| `197-200` | `!IsProperTarget(TargetCret)` ⇒ `TargetCret = null`（直接赋值，**不**走 `DelTargetCreat`，不动 `TargetX/Y`） |
| `202-217` | `DupMode`：`HolySeize` ⇒ return false；否则 `WalkTo(RandomByte(8), false, BoFearFire)`，位置变了才 `DupMode=false; result=true` |

### 1.5 `MonsterObject.AttackTarget`（`src/M2Server/Monster/MonsterObject.cs:220-249`）逐句

| 行 | 语义 |
|---|---|
| `223-225` | `TargetCret != null` 且 `GetAttackDir(TargetCret, ref btDir)` |
| `227-233` | `(now - AttackTick) > NextHitTime`（**严格大于**）⇒ `AttackTick = now; TargetFocusTick = now; Attack(TargetCret, btDir); BreakHolySeizeMode();` |
| `234` | **只要邻接就 `return true`**（即使未到攻击间隔、即使 `_Attack` 未命中） |
| `236-246` | 不邻接：同图 ⇒ `SetTargetXy(目标格)`；异图 ⇒ `DelTargetCreat()` |
| `248` | 返回 false |

`Attack`（`src/M2Server/Actor/AnimalObject.cs:167-172`）：`nPower = GetBaseAttackPoewr(); AttackDir(target, nPower, nDir); SendAttackMsg(RM_HIT, Dir, CurrX, CurrY);`
`AttackDir`（`src/M2Server/Actor/BaseObject.Attack.cs:10-19`）：`Dir = nDir; if (_Attack(...)) { SetTargetCreat(target); return true; }`。
`_Attack`（`src/M2Server/Actor/BaseObject.Attack.cs:30-66`）的取数次序：
1. `IsProperTarget(target)` 为真且 `target.HitPoint > 0` ⇒ `RandomByte(target.SpeedPoint)`，`HitPoint < 该值` 则 `nPower = 0`；
2. `nPower > 0` ⇒ `GetHitStruckDamage`（`Random(|ΔAC|+1)`，可能再 `Random(nRnd)`）；
3. `StruckDamage` + `SendStruckDelayMsg(RM_REFMESSAGE, …, 200)`；
4. 目标 `Race > Play` ⇒ `SendMsg(RM_STRUCK, nPower, …, ActorId)`（目标的 `Run` 里 `SetLastHiter` + `Struck`）。

`GetBaseAttackPoewr`（`src/M2Server/Actor/BaseObject.Attack.cs:25-28`）⇒ `GetAttackPower(LoByte(DC), (sbyte)(HiByte(DC)-LoByte(DC)))`，基版抽 `Random(|Hi-Lo|+1)`。

### 1.6 判定链

| 函数 | oracle 位置（实测） | 语义要点 |
|---|---|---|
| `IsAttackTarget` | `src/M2Server/Actor/BaseObject.Base.cs:377-464` | `null/self` ⇒ false；`AdminMode/StoneMode` ⇒ false；`Race >= Animal`：`Master != null` 走 7 条主从规则（含 `(AnimalObject)target.HolySeize`、`Master.SlaveRelax`、安全区、`((MonsterObject)this).BreakCrazyMode()` 的强制转型），否则「`Play` ⇒ true；`PeaceNpc < Race < Animal` ⇒ true；`target.Master != null` ⇒ true」；再叠 `CrazyMode`/`NastyMode`。`Race < Animal` ⇒ true |
| `IsProperTarget` | `src/M2Server/Actor/BaseObject.Base.cs:470-473` | **直接 `return IsAttackTarget(baseObject)`**；不查 `Death/Ghost/Invisible` |
| 玩家版 `IsProperTarget` | `src/M2Server/Player/PlayObject.cs:1924-1962` | `IsAttackTarget` 后按 `Race == Play`/`Master` 分派 `IsProtectTarget` 与 `base.IsAttackTarget` |
| 玩家版 `IsAttackTarget` | `src/M2Server/Player/PlayObject.Attack.cs:627` | 按 `AttatckMode` 分派；本审计只作目标有效性输入 |
| `GetAttackDir` | `src/M2Server/Actor/BaseObject.cs:1264-1313` | 八邻格（同格 false）；判定顺序 Left→Right→Up→Down→UpLeft→UpRight→DownLeft→DownRight |
| `GetAttackDir(range)` | `src/M2Server/Actor/BaseObject.cs:1315-1325` | 按 `GetNextDirection` 取 `nRange` 格，**只判落点是否有该目标**（不判路径） |
| `GetLongAttackDirDis` | `src/M2Server/Monster/MonsterObject.cs:399-463` | `nC` 从 0 递增到 `dis`，环形扩张；**Left 支不 break**（其余七支 break）⇒ 同时命中时后写的方向胜 |
| `TargetInSpitRange` | `src/M2Server/Actor/BaseObject.cs:1327-1351` | `|Δ|<=2`；`|Δ|<=1` 直接 `GetAttackDir` 并 **return true**（即使 `GetAttackDir` 失败）；否则查 `SpitMap[dir, nY+2, nX+2] == 1` |
| `MagCanHitTarget` | `src/M2Server/Actor/BaseObject.cs:2059-2092` | 至多 13 步逐格逼近；命中目标格 ⇒ true；曼哈顿距离**变大** ⇒ true；越界/不可走 ⇒ false |
| `WantRefMsg` | 字段 `src/M2Server/Actor/BaseObject.cs:236`；缺省 false `:431`；玩家置 true `src/M2Server/Player/PlayObject.cs:889`；`ArcherGuard` 置 true `src/M2Server/Monster/Monsters/ArcherGuard.cs:15` | 决定 AOI 扫描时**观察者**是否收下非玩家目标（`src/M2Server/Actor/BaseObject.ViewRange.cs:112`） |
| `ClearTargetCreat` | `src/M2Server/Actor/BaseObject.Base.cs:568-585` | 删首个匹配的可见项 + `DelTargetCreat()` |

### 1.7 `AttackTarget` 覆写清册（全仓 grep 实测）

`grep "override.*AttackTarget" src/ --include=*.cs` ⇒ **12 条命中 / 10 条真覆写**（多出的 `CentipedeKingMonster.cs:85` 的 `override void FindAttackTarget()` 与 `PlayObject.Attack.cs:627` 的 `override bool IsAttackTarget(...)` 不属本族）；另有 `protected virtual bool AttackTarget` 3 条（基类 + 2 个自实现）。

| # | 类（race） | 位置（实测） | 判定/取数差异 | 真机可达 |
|---:|---|---|---|---|
| 1 | `DoubleCriticalMonster`（130） | `src/M2Server/Monster/Monsters/DoubleCriticalMonster.cs:14-41`；`DoubleAttack` `:43-73` | 用 `TargetInSpitRange`（非邻接）替代 `GetAttackDir`；`Random(|Hi-Lo|+1)` 一次；4×4 `SpitMap` 逐格 `Random(SpeedPoint) < HitPoint`；`GetHitStruckDamage`（非魔法）；`RM_REFMESSAGE` 300ms | 否（部署 `MonGen`/库均无 130） |
| 2 | `DualAxeMonster`（87；`ThornDarkMonster` 93、`ArcherMonster` 104 继承） | `src/M2Server/Monster/Monsters/DualAxeMonster.cs:33-72`；`FlyAxeAttack` `:14-31` | 7 格门（**两轴都误写成 `CurrX`**）；`AttackMax-1 > _mNAttackCount` ⇒ 计数 + `FlyAxeAttack`（`Random(|Hi-Lo|+1)`、`CanFly`、`RM_FLYAXE`、延迟 `max(|Δ|)*50+600`）；否则 `Random(5)==0` 才清计数；11 格内同图才 `SetTargetXy` | **是**（87/93/104 共 237 条） |
| 3 | `ElfWarriorMonster`（114，`: SpitSpider`） | `src/M2Server/Monster/Monsters/ElfWarriorMonster.cs:72-103`；`SpitAttack` `:41-70` | `Appr` 不是 704/706/708 ⇒ `base.AttackTarget()`；否则 `TargetInSpitRange \|\| GetLongAttackDirDis(target,3)`；`SpitAttack` 的 `while(true)` **无 break/递增**（见 §4 G19） | 否（114 无 MonGen 映射） |
| 4 | `ExplosionSpider`（117） | `src/M2Server/Monster/Monsters/ExplosionSpider.cs:47-77`；`DoSelfExplosion` `:19-45` | 邻接且到点 ⇒ `DoSelfExplosion()`：`HP=0`、`Random(|Hi-Lo|+1)`、对 `VisibleActors` 中 `|Δ|<=1` 的 proper 目标各扣 `GetHitStruckDamage(nPower/2)+GetMagStruckDamage(nPower/2)`、`RM_REFMESSAGE` 700ms；**不调 `BreakHolySeizeMode`** | 否（117 无映射） |
| 5 | `GasAttackMonster`（90；`GasMothMonster` 105、`GasDungMonster` 106 继承） | `src/M2Server/Monster/Monsters/GasAttackMonster.cs:42-73`；`GasAttack` `:14-40` | 邻接 + 到点 ⇒ `GasAttack(btDir)`：`Random(|Hi-Lo|+1)`、`GetPoseCreate`、`IsProperTarget`、`Random(target.SpeedPoint) < HitPoint`、`GetMagStruckDamage`、`RM_REFMESSAGE` 300ms、`Random(target.AntiPoison+20)==0` ⇒ `STONE,5,0`；随后 `BreakHolySeizeMode` | **是**（90=25、105=279、106=2） |
| 6 | `MagCowMonster`（91） | `src/M2Server/Monster/Monsters/MagCowMonster.cs:33-64`；`MagicAttack` `:13-31` | 邻接 + 到点 ⇒ `MagicAttack`：`Random(|Hi-Lo|+1)`、`GetPoseCreate`、`IsProperTarget`、**`AntiMagic >= 0`**（不是命中率）⇒ `GetMagStruckDamage`、300ms；`BreakHolySeizeMode` | **是**（18） |
| 7 | `MagicMonster`（215 `FireballMonster`） | `src/M2Server/Monster/Monsters/MagicMonster.cs:47-82` | `TargetCret == Master` ⇒ 清目标；邻接 + 到点**只刷新 `AttackTick/TargetFocusTick`，完全无伤害/无帧/无 `BreakHolySeizeMode`** | 否（215 无 MonGen/库行） |
| 8 | `PercentMonster`（无 race） | `src/M2Server/Monster/Monsters/PercentMonster.cs:47-77` | 邻接 + 到点 ⇒ `Attack(target, btDir)`（基类 `Attack`，抽 DC + 命中）；`BreakHolySeizeMode` | 否（**死类**：全仓无 `new PercentMonster`） |
| 9 | `SpitSpider`（82；`HighRiskSpider` 118、`BigPoisionSpider` 119 继承） | `src/M2Server/Monster/Monsters/SpitSpider.cs:58-85`；`SpitAttack` `:19-...` | `TargetInSpitRange`；`SpitAttack`：`Random(|Hi-Lo|+1)`、5×5 `SpitMap` 逐格 `Random(target.SpeedPoint) < HitPoint`、`GetMagStruckDamage`；`BreakHolySeizeMode` | **是**（82=10、119=31） |
| 10 | `CentipedeKingMonster`（107，`: StickMonster`） | `src/M2Server/Monster/Monsters/CentipedeKingMonster.cs:42-83`；`CheckAttackTarget` `:20-40`；`FindAttackTarget` `:85-89` | **完全不看 `TargetCret`**：扫 `VisibleActors` 找 ViewRange 内首个 proper ⇒ 刷新 `AttackTick`、`SendAttackMsg(RM_HIT)`、`Random(|Hi-Lo|+1)`，对每个 `<ViewRange`（严格）的 proper 目标发 `RM_DELAYMAGIC(…,600)`，并对**每个**目标抽 `Random(4)`，==0 再抽 `Random(3)`（!=0 ⇒ DECHEALTH 60/3，==0 ⇒ STONE 5/0）并 `TargetCret = baseObject` | **是**（19） |
| 11 | `BigHeartMonster`（115，`: AnimalObject`，`protected virtual`） | `src/M2Server/Monster/Monsters/BigHeartMonster.cs:15-42` | **无目标门、无邻接门**；`NextHitTime` 到点即扫全部 `VisibleActors` 的 `ViewRange(16)` 内 proper 目标，各发 `RM_DELAYMAGIC(…,200)` + `RM_10205`；`Random(|Hi-Lo|+1)` 一次 | **是**（4） |
| 12 | `StickMonster`（85；`SandMobObject` 132 继承，`protected virtual`） | `src/M2Server/Monster/Monsters/StickMonster.cs:26-52`；`FindAttackTarget` `:54-58` | 邻接 + 到点 ⇒ `Attack`；**不调 `BreakHolySeizeMode`**；异图 `DelTargetCreat` | **是**（85=5；132 无映射） |
| 13 | `SuperGuard`（11；`PetSuperGuard` 12） | `src/M2Server/Monster/Monsters/SuperGuard.cs:15-49` | 不看邻接：同图 + 到点 ⇒ 临时把 `CurrX/Y` 移到目标身后、`_Attack(GetBaseAttackPoewr(), TargetCret)`、`SetLastHiter`、`ExpHitter=null`、恢复坐标、`TurnTo` | **是**（守卫由 GuardList 放置） |
| 14 | `ArcherGuard`（112；`ArcherPolice` 20） | `src/M2Server/Monster/Monsters/ArcherGuard.cs:21-37`；`Run` `:39-91` | `Run` 内每 `WalkSpeed` 扫最近 proper ⇒ `AttackTarger`：`Random(|Hi-Lo|+1)`、`GetHitStruckDamage`、`StruckDamage`、延迟 `max(|Δ|)*50+600`、`RM_FLYAXE`；无 `AttackTarget` override，但不走 `MonsterObject.Run` | **是**（21 条 + 57 条出生失败） |

> `grep "override.*AttackTarget"` 的 10 条真覆写与上表 1–10 一一对应（12 条命中中另 2 条已在上文排除）；11/12 是 `virtual` 自实现，13/14 是守卫私有入口（不在该 grep 命中内，但同属「索敌→攻击」链，必须一起看）。

### 1.8 `SearchTarget` 调用点（全仓实测 23 个 M2Server 文件 + 1 个机器人文件）

| 文件（实测） | 行 | 门 / 备注 |
|---|---:|---|
| `src/M2Server/Monster/Monsters/ATMonster.cs` | 20 | 8s/1s 门（§1.3）；派生 `Run` 未覆写者继承 |
| `src/M2Server/Monster/Monsters/BeeQueen.cs` | 57 | 自有 `Run` |
| `src/M2Server/Monster/Monsters/BoneKingMonster.cs` | 55 | 8s/1s |
| `src/M2Server/Monster/Monsters/Clone.cs` | 69 | 自有 `Run` |
| `src/M2Server/Monster/Monsters/DigOutZombi.cs` | 50 | 非隐藏支 |
| `src/M2Server/Monster/Monsters/ElectronicScolpionMon.cs` | 48 | `>1000 && TargetCret == null` |
| `src/M2Server/Monster/Monsters/FireballMonster.cs` | 58 | 自有 `Run` |
| `src/M2Server/Monster/Monsters/FireMonster.cs` | 78 | 8s/1s |
| `src/M2Server/Monster/Monsters/FrostTiger.cs` | 32 | 8s/1s |
| `src/M2Server/Monster/Monsters/GreenMonster.cs` | 32 | 8s/1s |
| `src/M2Server/Monster/Monsters/Khazard.cs` | 55 | 8s/1s |
| `src/M2Server/Monster/Monsters/LightingZombi.cs` | 39 | 8s/1s |
| `src/M2Server/Monster/Monsters/MagicMonObject.cs` | 38 | `>1000 && 无目标` |
| `src/M2Server/Monster/Monsters/MinorNumaObject.cs` | 19 | 自有 `Run` |
| `src/M2Server/Monster/Monsters/RedMonster.cs` | 32 | 8s/1s |
| `src/M2Server/Monster/Monsters/RonObject.cs` | 42 | 8s/1s |
| `src/M2Server/Monster/Monsters/RunAway.cs` | 60 | 死类 |
| `src/M2Server/Monster/Monsters/SandMobObject.cs` | 44 | `NextHitTime` 门 |
| `src/M2Server/Monster/Monsters/ScultureKingMonster.cs` | 93 | 非石化支 |
| `src/M2Server/Monster/Monsters/ScultureMonster.cs` | 74 | 非石化支 |
| `src/M2Server/Monster/Monsters/SpiderHouseMonster.cs` | 61 | 自有 `Run` |
| `src/M2Server/Monster/Monsters/StickMonster.cs` | 119 | 非隐藏支 |
| `src/M2Server/Monster/Monsters/TeleMonster.cs` | 27 | 8s/1s |
| `src/Modules/RobotSystem/Services/RobotPlayer.Base.cs` | 39/876 | 机器人；`SearchTargetTick` 1000ms |

---

## 2. Rust 侧清册

### 2.1 等价符号

| 行为 | Rust（符号/路径） | 备注 |
|---|---|---|
| 索敌 | `crates/world/src/monai.rs::search_target` | 逐行复刻 `AnimalObject.SearchTarget`，但清理分支不调 `clear_target_creature`（见 G05） |
| 索敌节拍 | `crates/world/src/monai.rs::monster_run` 顶部 `is_active_seek_race` 门 | 集合 `{53,81,83,86,88,89}` |
| race 分类 | `crates/world/src/monai.rs::is_active_seek_race` / `is_animal_race` / `is_guard_race` | `is_active_seek_race` 是 Rust 概念，oracle 无同名 |
| 思考 | `crates/world/src/monai.rs::think` | 3s 门、`DupMode`、`RandomByte(8)` |
| 攻击 | `crates/world/src/monai.rs::attack_target` | 基类语义；到点 ⇒ `turn_to` + `World::attack_once` + `pending_ai_attacks` |
| 邻接方向 | `crates/world/src/monai.rs::get_attack_dir` | 八邻格 + 顺序 |
| 目标有效性 | `crates/world/src/combat.rs::is_proper_target` / `is_proper_target_grouped` / `is_attack_target_base` | 顶部折入 death/ghost/invisible（G09） |
| 攻击结算 | `crates/world/src/world.rs::World::attack_once` → `crates/world/src/combat.rs::attack_dir_grouped_poison` | `_Attack` 的 4 步取数 |
| 施法可达性 | `crates/world/src/magic.rs::mag_can_hit_target` | 13 步逐格；仅玩家魔法路径消费 |
| 目标写入 | `crates/world/src/monai.rs::set_target_creature` / `del_target_creature` / `clear_target_creature` | 三者语义分开 |
| 怪物 Run | `crates/world/src/monai.rs::monster_run` / `monster_ai_tick` / `base_object_run_monster` | 普通怪路径 |
| 专用怪 | `crates/world/src/special_monster.rs::special_monster_run` | 仅 BeeQueen/SpiderHouse（103/116） |
| 守卫 | `crates/world/src/monai.rs::is_guard_race` ⇒ 静止 + `animal_run` + `base_object_run_monster` | 不索敌、不攻击 |
| 视野集合 | `crates/world/src/aoi.rs::search_view_range` / `update_visible_gay` | 每 tick 全量扫 |

### 2.2 `entity.ai.target_creature` 的写入点（非测试）

| # | Rust 位置（符号/路径） | 对应 oracle | 语义 |
|---:|---|---|---|
| W01 | `monai.rs::set_target_creature`（被 `search_target` / `animal_struck` / `special_monster` 调用） | `src/M2Server/Actor/BaseObject.Base.cs:557-561` `SetTargetCreat` | 写 `target_creature + target_focus_tick` |
| W02 | `monai.rs::del_target_creature` | `src/M2Server/Actor/AnimalObject.cs:151-156` `DelTargetCreat` | 清目标 + `target_x/y=-1` |
| W03 | `monai.rs::clear_target_creature` | `src/M2Server/Actor/BaseObject.Base.cs:568-585` `ClearTargetCreat` | 先从 `visible_actors` 删首个匹配，再清目标 |
| W04 | `monai.rs::think` 内直接 `target_creature = None` | `src/M2Server/Monster/MonsterObject.cs:197-200` 直接 `TargetCret = null` | 不清 `target_x/y` |
| W05 | `monai.rs::target_cret_cleanup`（`base_object_run_monster` 内） | `src/M2Server/Actor/BaseObject.Base.cs:111-118` | 30s/死亡/幽灵/异图/15 格 ⇒ `clear_target_creature` |
| W06 | `special_monster.rs` 的宝宝目标 | `src/M2Server/Monster/Monsters/BeeQueen.cs:40` / `src/M2Server/Monster/Monsters/SpiderHouseMonster.cs:43` | 宝宝继承母体目标 |
| W07 | `slave.rs` 的诱惑/召唤路径 | `BaseObject` 的 `MakeSlave` 系 | 召唤者/宝宝互设 |
| W08 | `world.rs` 的魔法/毒/复活路径（若干写入点） | `src/M2Server/Magic/MagicManager.cs:253` 等 | 施法目标写入 |
| W09 | `merchant.rs` 商人清理 | `Merchant.Run` | 仅清空 |

> 生产缺口：**W03 未被 `search_target` 使用**；W01/W02/W04/W05 与 oracle 的差异见 §3 矩阵 A16/A17 与 §4 G05/G20。

---

## 3. 覆盖矩阵（57 行）

图例：状态 = 已实现 / 部分 / 缺失 / 口径不同 / 不适用；tier 按 `AGENTS.md`（判不准按 R）。

### 3.1 索敌节拍（S）

| ID | 行为点 | oracle（实测） | Rust | 状态 | 可观测后果 | tier | 判据 |
|---|---|---|---|---|---|---|---|
| S01 | `ATMonster.Run` 8s/1s 门 | `src/M2Server/Monster/Monsters/ATMonster.cs:13-24` | `monai.rs::monster_run` 的 `is_active_seek_race` 门 | 部分 | 6 个 race 对齐；其余继承者不索敌 | R | 固定时钟，8s/1s 边界各一例；断言 `search_enemy_tick` 刷新 + 目标出现 |
| S02 | 索敌 race 集合完整性 | `ATMonster.cs` 的继承关系（`CreateMonster` `src/GameSrv/Word/WorldServer.MonGen.cs:507-728`） | `monai.rs::is_active_seek_race` = `{53,81,83,86,88,89}` | 缺失 | 82/84/90/91/97/106/118/119/130 永不主动索敌 | R | 对每个 race 生成 1 只、固定玩家在 5 格内，推进 8.1s；断言 `target_creature` 出现 |
| S03 | `SearchTargetTick` | 仅 `src/Modules/RobotSystem/Services/RobotPlayer.Base.cs:869-876` / `src/Modules/RobotSystem/Services/RobotPlayer.cs:36` | 无（怪物路径无此概念） | 不适用 | 无（前提订正：怪物侧不存在该字段） | L | 代码对照：`monai.rs` 无同名 1s 门 |
| S04 | `SearchTarget` 调用点覆盖 | 23 个 M2Server 文件（§1.8） | 仅 `monster_run`（6 race）+ `special_monster_run`（103/116） | 缺失 | 其余 15 个类的自有索敌门全无 | R | 按 §1.8 文件表逐类构造 1 例（MB 批次验收会覆盖大部分） |
| S05 | ctor `SearchTime/SearchTick/SearchEnemyTick` 抽数 | `src/M2Server/Actor/AnimalObject.cs:76-78`、`src/M2Server/Monster/MonsterObject.cs:65`、`src/M2Server/Monster/Monsters/ATMonster.cs:9-10` | `entity.rs::BaseAnimalCtorDraws` / `MonsterCtorDraws`；`bins/game-svr/src/mongen.rs::draw_monster_construct` | 已实现 | 生成期 RNG 流水 | R | 既有 ctor 账本机判 |
| S06 | 每 actor `RunTime/RunTick` | `src/GameSrv/Word/WorldServer.MonGen.cs:280` | `World::tick` 每 200ms 全量 | 口径不同 | 评估频率 ≈1.25×（可见）/3.75×（不可见） | R | 前件 `audit-baseobject-run-coverage-20261018.md` MCS-2 |
| S07 | 不可见降频 | `src/GameSrv/Word/WorldServer.MonGen.cs:294-297` | `is_visible_active` 只写不读 | 缺失 | 不可见怪 AI 频率过高 | R | 前件 MCS-3（前置未接） |

### 3.2 可见性门（V）

| ID | 行为点 | oracle（实测） | Rust | 状态 | 可观测后果 | tier | 判据 |
|---|---|---|---|---|---|---|---|
| V01 | 野怪不自己扫视野 | `src/GameSrv/Word/WorldServer.MonGen.cs:300-313` | `world.rs` AOI 段对所有非玩家 viewer 每 tick 扫 | 口径不同 | 索敌集合的**到达时刻**不同（Rust 更早/更广） | R | 固定时钟 + 玩家在怪物边缘，断言首次 `target_creature` 的时刻 |
| V02 | `RM_UPDATEVIEWRANGE` 注入 | `src/M2Server/Player/PlayObject.cs:3129-3139`；`src/M2Server/Monster/MonsterObject.cs:177-185` | 无消息、无 handler（`crates/protocol` 只有常量） | 缺失 | 怪物可见集的**写入者**不同 | R | 构造：把玩家扫描结果按 oracle 距离门过滤，断言边缘怪不在 `visible_actors` |
| V03 | `UpdateMonsterVisible` 节流 | `src/M2Server/Monster/MonsterObject.cs:136-141` | 无（`ai.search_tick` 只为玩家节流） | 缺失 | 视野注入频率 200ms vs 1.5–3.5s | R | 固定时钟，连续 10 tick 注入，断言只接受 1 次 |
| V04 | 视野过滤条件 | `src/M2Server/Actor/BaseObject.ViewRange.cs:112` | `aoi.rs::search_view_range` 的 `visible` 布尔式 | 已实现 | 同一过滤式 | R | 既有 AOI 机判 |
| V05 | 格子 60s 过期 | `src/M2Server/Actor/BaseObject.ViewRange.cs:95`；玩家跳过 `src/M2Server/Player/PlayObject.cs:3114-3123` | `aoi.rs::search_view_range` 的 `viewer_is_play` 分流 | 已实现 | 静止怪被逐出的根因已由 `8b4f5df5e` 修 | R | 前件 MCS-1 |
| V06 | `HideMode \|\| CoolEye` 门 | `src/M2Server/Actor/AnimalObject.cs:402` | `monai.rs::search_target` 的 `(!t.hide_mode \|\| cool_eye)` | 已实现 | 隐身目标可否被索敌 | R | 构造 `hide_mode=true` 两态 |
| V07 | `CoolEye` 生成抽数 | `src/GameSrv/Word/WorldServer.MonGen.cs:745-747` | `bins/game-svr/src/mongen.rs` 的 `cool_eye` + `Entity::seed_from_spawn` | 已实现 | 生成期 RNG | R | 既有 mongen 账本 |
| V08 | AnimalObject 派生类无 `RM_UPDATEVIEWRANGE` handler | `src/M2Server/Actor/AnimalObject.cs:114-140` 只处理 `RM_STRUCK` 与 `RM_MAKEHOLYSEIZEMODE`（其余转 `base.Operate`），无 `RM_UPDATEVIEWRANGE` handler | Rust 对所有实体统一 AOI | 口径不同 | 107/132/85/115 的视野来源不同 | R | 构造 107 野怪 + 玩家，断言 oracle 侧 `VisibleActors` 为空（需真机确认） |
| V09 | `WantRefMsg` | `src/M2Server/Actor/BaseObject.cs:236`；`src/M2Server/Player/PlayObject.cs:889`；`src/M2Server/Monster/Monsters/ArcherGuard.cs:15` | `entity.rs::want_ref_msg`；玩家/克隆体置 true | 部分 | 守卫 112 未置 true（守卫路径整体未建模） | L | 断言 race 112 的 `want_ref_msg`（当前 false） |

### 3.3 攻击判定（A）

| ID | 行为点 | oracle（实测） | Rust | 状态 | 可观测后果 | tier | 判据 |
|---|---|---|---|---|---|---|---|
| A01 | `GetAttackDir` 八邻格 | `src/M2Server/Actor/BaseObject.cs:1264-1313` | `monai.rs::get_attack_dir` | 已实现 | 邻接/方向 | R | 8 向 + 同格用例 |
| A02 | `GetLongAttackDirDis` | `src/M2Server/Monster/MonsterObject.cs:399-463` | 无 | 缺失 | 仅 114 使用（不可达） | R | 构造 3 格/对角；注意 Left 不 break |
| A03 | `TargetInSpitRange` | `src/M2Server/Actor/BaseObject.cs:1327-1351` | 无 | 缺失 | 82/118/119/130/114 的命中距离全错 | R | `SpitMap` 四向 1/2 格用例 |
| A04 | `MagCanHitTarget` | `src/M2Server/Actor/BaseObject.cs:2059-2092` | `magic.rs::mag_can_hit_target`（已实现） | 部分 | 怪物侧**零消费**（215 未建模） | R | MB-9 的 A7 用例 |
| A05 | `_Attack` 命中率抽数 | `src/M2Server/Actor/BaseObject.Attack.cs:39-45` | `combat.rs::attack_with_grouped` | 已实现 | `RandomByte(SpeedPoint)` | R | 既有战斗账本 |
| A06 | `_Attack` 攻击力抽数 | `src/M2Server/Actor/BaseObject.Attack.cs:25-28` | `combat.rs::get_base_attack_power` | 已实现 | `Random(|Hi-Lo|+1)` | R | 既有战斗账本 |
| A07 | `RM_STRUCK` 回灌 | `src/M2Server/Actor/BaseObject.Attack.cs:61-64`；`src/M2Server/Actor/AnimalObject.cs:118-133` | `World::attack_once` 的 `pending_struck` + `base_object_run_monster` 出队 | 已实现 | `SetLastHiter` + `Struck` | R | 既有用例 |
| A08 | 到点门严格大于 | `src/M2Server/Monster/MonsterObject.cs:227` | `monai.rs::attack_target` 的 `>` | 已实现 | 攻击间隔 | R | 边界 `== NextHitTime` 不攻 |
| A09 | 邻接即返回 true | `src/M2Server/Monster/MonsterObject.cs:234` | `monai.rs::attack_target` | 已实现 | `base.Run` 早退 | R | 未到点也返回 true |
| A10 | 异图 `DelTargetCreat` | `src/M2Server/Monster/MonsterObject.cs:236-246` | `monai.rs::attack_target` | 已实现 | `target_x/y=-1` | R | 跨图用例 |
| A11 | `BreakHolySeizeMode` | 基类 `:232`；各覆写不同（§1.7） | `attack_target` 在到点时清 `holy_seize` | 部分 | Stick/Centipede/Explosion 不清，Rust 清 | R | 各覆写用例断言 `holy_seize` 保持 true |
| A12 | `RM_HIT` 帧 | `src/M2Server/Actor/AnimalObject.cs:171` | `pending_ai_attacks` + 宿主下行 | 部分 | 命中/未命中都发 | R | 帧面机判 |
| A13 | `Think` 3s/`DupMode`/`RandomByte(8)` | `src/M2Server/Monster/MonsterObject.cs:187-218` | `monai.rs::think` | 已实现 | 位置/RNG | R | 既有用例 |
| A14 | `Think` 的目标有效性刷新 | `src/M2Server/Monster/MonsterObject.cs:197-200` | `monai.rs::think` | 部分 | Rust 用折入版 `is_proper_target`（G09） | R | 用 `invisible=false/true` 两态 |
| A15 | `GetXYObjCount >= 2` | `src/M2Server/Maps/Envirnoment.cs:1062-1094` | `monai.rs::count_at_cell` | 已实现 | `DupMode` | R | 既有用例；CastleDoor 特例未建模（登记） |
| A16 | `SearchTarget` 无效项清理副作用 | `src/M2Server/Actor/AnimalObject.cs:392-417`（无效项分支 `:395-399`、收尾 `SetTargetCreat` `:413-416`）+ `src/M2Server/Actor/BaseObject.Base.cs:568-585` | `monai.rs::search_target` 从 `visible_actors` 移除无效项，但不调 `del_target_creature` | **口径不同** | 「无效项在合法项之前」时 oracle 因 `RemoveAt` 跳过合法项：初始无目标 ⇒ 保持 `None`（Rust 置 `Some(合法)`）；初始**已有**目标 ⇒ oracle 经 `DelTargetCreat` 落到 `None`（Rust 仍 `Some(合法)`）。`[合法, 无效]` 顺序两边都是 `Some(合法)` | R | **初始 `target_creature = None`**、`visible_actors = [无效, 合法]`：断言 `target_creature == None`（oracle 口径；当前 `Some(合法)` ⇒ 红）。**不得**用 `[合法, 无效]`（断言 `None` 会误红） |
| A17 | `ClearTargetCreat` 删后跳项 | `src/M2Server/Actor/AnimalObject.cs:392-399` 的 `for` + `RemoveAt` | `monai.rs::search_target` 的 `retain` | 口径不同 | 连续多个无效项时保留/删除集合不同 | R | 两个连续的无效项，断言剩余 `visible_actors` 数量 |

### 3.4 目标选择优先级（P）

| ID | 行为点 | oracle（实测） | Rust | 状态 | 可观测后果 | tier | 判据 |
|---|---|---|---|---|---|---|---|
| P01 | 最近曼哈顿 + 严格 `<` | `src/M2Server/Actor/AnimalObject.cs:404-409` | `monai.rs::search_target` | 已实现 | 目标选择 | R | 等距两目标取先出现者 |
| P02 | `n10 = 999` 初值 | `src/M2Server/Actor/AnimalObject.cs:391` | `monai.rs::search_target` | 已实现 | 不可达（±15 内上限 30） | L | 代码对照 |
| P03 | 平手依赖 `VisibleActors` 插入序 | `src/M2Server/Actor/BaseObject.ViewRange.cs:12-40` | `aoi.rs::update_visible_gay` 追加 | 部分 | 两侧插入时机不同（V01/V02）⇒ 平手可不同 | R | 固定两侧列表序，断言平手选择 |
| P04 | 无目标不清旧目标 | `src/M2Server/Actor/AnimalObject.cs:413-416` | `monai.rs::search_target` | 已实现 | 目标保持 | R | 空视野两次调用，目标不变 |
| P05 | `CentipedeKing` 多目标选择 | `src/M2Server/Monster/Monsters/CentipedeKingMonster.cs:53-80` | 无（走基类单目标） | 缺失 | 多目标伤害/毒/RNG | R | 见 G06 |
| P06 | `BigHeart` 无目标门 | `src/M2Server/Monster/Monsters/BigHeartMonster.cs:15-42` | 基类要求 `TargetCret != null` | 缺失 | 不挨打也全体 AoE | R | 见 G07 |
| P07 | `GasMothAttack` 最近且无 hide/±15 清理 | `src/M2Server/Monster/Monsters/GasMothMonster.cs:39-66` | 无 | 缺失 | 105 的索敌集合不同 | R | MB-3 R21 |
| P08 | `DualAxe.Run` 最近 proper | `src/M2Server/Monster/Monsters/DualAxeMonster.cs:84-137` | 无 | 缺失 | MB-2 R13 | R | MB-2 |
| P09 | 守卫最近 proper | `src/M2Server/Monster/Monsters/ArcherGuard.cs:39-91` | 守卫静止、不索敌 | 缺失 | 112 不攻击 | R | MB-3 R02 |
| P10 | 主人视野共享 | `src/M2Server/Monster/Monsters/ATMonster.cs:26-68` | `slave.rs`/`monai.rs` 部分 | 部分 | 宝宝索敌 | R | MB 宝宝批次 |

### 3.5 覆写族的差异（E）

| ID | 类/race | oracle（实测） | Rust | 状态 | 可观测后果 | tier | 判据 |
|---|---|---|---|---|---|---|---|
| E01 | `DoubleCritical` 130 | `src/M2Server/Monster/Monsters/DoubleCriticalMonster.cs:14-73` | `monai.rs::attack_target` 基类 | 缺失 | 4×4 溅射/伤害公式/`Random` 位置全错 | R | 构造邻接 + 2 格目标；断言 `SpitMap` 逐格伤害与抽数 |
| E02 | `DualAxe` 87/93/104 | `src/M2Server/Monster/Monsters/DualAxeMonster.cs:14-72` | 基类 | 缺失 | 7 格飞斧/`Random(5)` 计数/`RM_FLYAXE` | R | 7 格/11 格边界 + 计数复位 |
| E03 | `ElfWarrior` 114 | `src/M2Server/Monster/Monsters/ElfWarriorMonster.cs:41-103` | 基类 | 缺失 | `Appr` 形态/3 格 spit/无限循环怪癖 | R | 不可达；登记 G19 |
| E04 | `ExplosionSpider` 117 | `src/M2Server/Monster/Monsters/ExplosionSpider.cs:19-77` | 基类 | 缺失 | 自爆（`HP=0` + 半物理半魔法 AoE + 700ms） | R | 不可达 |
| E05 | `GasAttackMonster` 90/105/106 | `src/M2Server/Monster/Monsters/GasAttackMonster.cs:14-73` | 基类 | 缺失 | `Random(SpeedPoint)`/`AntiPoison+20`/STONE/300ms | R | **可达**：90/106；105 走 MB-3 |
| E06 | `MagCow` 91 | `src/M2Server/Monster/Monsters/MagCowMonster.cs:13-64` | 基类 | 缺失 | `AntiMagic>=0` 门 + 魔法减伤 + 300ms | R | **可达**（18 条） |
| E07 | `MagicMonster` 215 | `src/M2Server/Monster/Monsters/MagicMonster.cs:47-82` | 基类 | 缺失 | 无伤害 AttackTarget + 双 pass | R | MB-9 |
| E08 | `PercentMonster` | `src/M2Server/Monster/Monsters/PercentMonster.cs:47-77` | 无 | 不适用 | 死类 | L | 确认无 `new PercentMonster` |
| E09 | `SpitSpider` 82/118/119 | `src/M2Server/Monster/Monsters/SpitSpider.cs:19-85` | 基类 | 缺失 | 5×5 喷吐/毒/`Random(SpeedPoint)` | R | **可达**：82/119 |
| E10 | `CentipedeKing` 107 | `src/M2Server/Monster/Monsters/CentipedeKingMonster.cs:20-89` | 基类 | 缺失 | 多目标 + 600ms 自延时 + 毒 | R | MB-3 R07；但见 G18 |
| E11 | `BigHeart` 115 | `src/M2Server/Monster/Monsters/BigHeartMonster.cs:15-42` | 基类 | 缺失 | 16 格 AoE + 200ms | R | MB-1 R04 |
| E12 | `StickMonster` 85/132 | `src/M2Server/Monster/Monsters/StickMonster.cs:26-52` | 基类（且 Rust 多跑 `Think/Wondering`） | 口径不同 | 出土/缩地 + 无 `BreakHolySeize` | R | MB-1 R38 |
| E13 | 守卫 11/12/20/112 | `src/M2Server/Monster/Monsters/SuperGuard.cs:15-49`；`src/M2Server/Monster/Monsters/ArcherGuard.cs:21-91` | 静止 + `base.Run` | 缺失 | 守卫完全不打人 | R | MB-3 R02 |
| E14 | `AttackTarget` 分发机制 | C# 虚调用（`src/M2Server/Monster/MonsterObject.cs:220` 的 `virtual`） | `monai.rs::monster_run` 固定调 `attack_target`，无 per-race 分派 | 缺失 | 所有覆写无法接入 | R | 结构判据：新增 dispatch 后逐 race 断言所选入口 |

---

## 4. 缺口清单（按「真机可达 × 影响」排序）

`RNG` 列 = 是否改变随机数抽数次序/次数（**本列标「是」的缺口会直接污染同种子跨侧账本**）。

| 序 | ID | 缺口 | oracle 依据 | Rust 现状 | 真机可达 | 影响 | RNG | 归属 |
|---:|---|---|---|---|---|---|---|---|
| 1 | G01 | `SpitSpider.AttackTarget`（82/118/119）未建模 | `src/M2Server/Monster/Monsters/SpitSpider.cs:58-85` | 基类单目标物理攻击 | **82=10 / 119=31 条** | 喷吐范围/毒/魔法减伤/抽数全错 | 是 | **无归属**（需新卡） |
| 2 | G02 | `GasAttackMonster.AttackTarget`（90/105/106）未建模 | `src/M2Server/Monster/Monsters/GasAttackMonster.cs:42-73` | 基类 | **90=25 / 105=279 / 106=2** | 命中率/STONE/300ms/抽数全错 | 是 | MB-3 只覆盖 105 的 `Run` 与 `GasAttack`，**AttackTarget 本身无归属** |
| 3 | G03 | `DualAxe.AttackTarget`（87/93/104）未建模 | `src/M2Server/Monster/Monsters/DualAxeMonster.cs:33-72` | 基类 | **87=18 / 93=50 / 104=169** | 飞斧 7 格/`Random(5)`/计数/帧 | 是 | MB-2（R13 同批必接） |
| 4 | G04 | `MagCow.AttackTarget`（91）未建模 | `src/M2Server/Monster/Monsters/MagCowMonster.cs:33-64` | 基类 | **91=18** | 魔法攻击/`AntiMagic>=0`/抽数 | 是 | **无归属** |
| 5 | G05 | `SearchTarget` 无效项不调 `DelTargetCreat`（且无 `RemoveAt` 跳项） | `src/M2Server/Actor/AnimalObject.cs:395-399` + `src/M2Server/Actor/BaseObject.Base.cs:568-585` | `monai.rs::search_target` 只从 `visible_actors` 移除无效项，不调 `del_target_creature` | 任意视野含死怪/超距怪 | 目标状态取决于顺序：`[无效, 合法]` + 初始无目标 ⇒ oracle 保持 `None`、Rust 置 `Some(合法)` | 是（后续攻击抽数） | **无归属** |
| 6 | G06 | `CentipedeKing.AttackTarget`（107）未建模 | `src/M2Server/Monster/Monsters/CentipedeKingMonster.cs:42-83` | 基类 | **107=19** | 多目标 + 600ms + 毒 + `Random(4)/(3)` | 是 | MB-3（R07 同批必接） |
| 7 | G07 | `BigHeart.AttackTarget`（115）未建模 | `src/M2Server/Monster/Monsters/BigHeartMonster.cs:15-42` | 基类 | **115=4** | 16 格 AoE + 200ms | 是 | MB-1（R04 同批必接） |
| 8 | G08 | `is_active_seek_race` 集合缺 9 个 race | `src/M2Server/Monster/Monsters/ATMonster.cs:13-24` 的继承关系 | `{53,81,83,86,88,89}` | **82/84/90/91/97/106/119 可达**；118/130 不可达 | 这 9 个 race 永不主动索敌 | 是（分支导向） | 计划件 §6.9 明说「不得顺手扩大，另立卡」⇒ **无归属** |
| 9 | G09 | `IsProperTarget` 折入 death/ghost/invisible | `src/M2Server/Actor/BaseObject.Base.cs:470-473` | `combat.rs::is_proper_target_grouped` 顶部三查 | 全怪 | 死目标/隐形目标的攻击与索敌被拒 | 是（death/ghost/invisible 目标只要 oracle `IsProperTarget` 判真就**仍抽** `RandomByte(SpeedPoint)`——注意 `HitPoint` 是**准确度**不是 HP，`Die()` 并不清零，仅 `target.HitPoint == 0` 才跳过；Rust 折入版少这一次，以及后续 `GetHitStruckDamage` 等伤害抽数） | **无归属** |
| 10 | G10 | 怪物视野驱动（`RM_UPDATEVIEWRANGE` + `SearchTime` 节流 + 距离门）未建模 | `src/M2Server/Player/PlayObject.cs:3129-3139`；`src/M2Server/Monster/MonsterObject.cs:136-185`；`src/GameSrv/Word/WorldServer.MonGen.cs:300-313` | `world.rs` AOI 每 200ms 全量扫 | 全怪 | 索敌的**可见集与到达时刻**不同 | 是（首次索敌时刻 ⇒ 首次攻击抽数） | **无归属** |
| 11 | G11 | `ExplosionSpider.AttackTarget`（117）未建模 | `src/M2Server/Monster/Monsters/ExplosionSpider.cs:19-77` | 基类 | 不可达 | 自爆 AoE | 是 | MB-6（R17） |
| 12 | G12 | `ElfWarrior.AttackTarget`（114）未建模 | `src/M2Server/Monster/Monsters/ElfWarriorMonster.cs:41-103` | 基类 | 不可达 | 形态 spit | 是 | MB-7（R16） |
| 13 | G13 | `StickMonster.AttackTarget`（85/132）与 AnimalObject 链 | `src/M2Server/Monster/Monsters/StickMonster.cs:26-52` | Rust 走 `MonsterObject` 全套 AI | **85=5** | 多出 Think/Wondering/AttackTarget | 是 | MB-1（R38） |
| 14 | G14 | `PercentMonster` | `src/M2Server/Monster/Monsters/PercentMonster.cs:47-77` | 无 | 死类 | 无 | 否 | 不排（死类） |
| 15 | G15 | `DoubleCritical` 130 | `src/M2Server/Monster/Monsters/DoubleCriticalMonster.cs:14-73` | 基类 | 不可达 | 4×4 溅射 | 是 | **无归属** |
| 16 | G16 | 守卫攻击路径（11/12/20/112） | `src/M2Server/Monster/Monsters/SuperGuard.cs:15-49`；`src/M2Server/Monster/Monsters/ArcherGuard.cs:21-91` | `is_guard_race` ⇒ 静止 | **112=21** | 守卫不打人 | 是 | MB-3（R02）仅覆盖 ArcherGuard；SuperGuard 11/12 需新卡（计划件 §2.5 R39 建议 `openmir2-superguard-run`） |
| 17 | G17 | `GasMoth.Run` 自有索敌门 | `src/M2Server/Monster/Monsters/GasMothMonster.cs:24-35` | 无 | **105=279** | 105 不索敌 | 是 | MB-3（R21） |
| 18 | G18 | `AttackTarget` per-race 分派机制 | `src/M2Server/Monster/MonsterObject.cs:220`（virtual） | `monster_run` 固定入口 | 全怪 | 任何覆写都无法接入 | 否 | 建议并入 MB-0 前置 |
| 19 | G19 | `ElfWarrior.SpitAttack` 无限循环怪癖 | `src/M2Server/Monster/Monsters/ElfWarriorMonster.cs:54-69`（`while(true)` 无 break、`nDamage` 不递增） | 无 | 不可达 | 若未来可达会挂线程；**不得「修」** | 否 | 登记，不实现 |
| 20 | G20 | `ClearTargetCreat` 删后跳项怪癖 | `src/M2Server/Actor/AnimalObject.cs:392-399` | `retain` | 多无效项相邻 | 剩余可见集不同 | 否（后续索敌） | **无归属** |
| 21 | G21 | `GetLongAttackDirDis` | `src/M2Server/Monster/MonsterObject.cs:399-463` | 无 | 仅 114 | 3 格直刺 | 否 | MB-7 |
| 22 | G22 | `TargetInSpitRange` | `src/M2Server/Actor/BaseObject.cs:1327-1351` | 无 | 82/118/119/130/114 | 喷吐几何 | 否 | 与 G01/G11/G15 同批 |
| 23 | G23 | `MagCanHitTarget` 怪物侧消费 | `src/M2Server/Monster/Monsters/FireballMonster.cs:20` | `magic.rs::mag_can_hit_target` 仅玩家路径 | 不可达 | Fireball 判定 | 否 | MB-9 |
| 24 | G24 | `MagicMonster` 双 pass / 无伤害 AttackTarget | `src/M2Server/Monster/Monsters/MagicMonster.cs:47-82` | 基类 | 不可达 | 两 pass 时序 | 是 | MB-9 |
| 25 | G25 | `Think` 目标有效性用折入版 | `src/M2Server/Monster/MonsterObject.cs:197-200` | `monai.rs::think` | 全怪 | 与 G09 同源 | 是 | 与 G09 合并 |
| 26 | G26 | `ArcherGuard` 的 `WantRefMsg=true` 未置 | `src/M2Server/Monster/Monsters/ArcherGuard.cs:15` | `entity.rs` 缺省 false | 112 | 守卫视野过滤更窄 | 否 | 与 G16 同批 |

> **RNG 统计**：标「是」18 处；G25 与 G09 同源合并计 ⇒ **17 条独立缺口**。

---

## 5. 与计划件 MB-1..MB-9 的交叉引用

| 缺口 | 计划件覆盖 | 结论 |
|---|---|---|
| G01 `SpitSpider.AttackTarget`（82/119） | 无 | **需新卡**（82/119 真机可达） |
| G02 `GasAttackMonster.AttackTarget`（90/105/106） | MB-3 仅覆盖 105 的 `Run`（R21）与 `GasAttack` 的接线说明；**不覆盖 AttackTarget 本身** | 90/106 需新卡；105 与 MB-3 合并时补 AttackTarget |
| G03 `DualAxe.AttackTarget` | **MB-2**（§2.3.0 明列 `:33-72` 同批必接） | 已归属 |
| G04 `MagCow.AttackTarget`（91） | 无 | **需新卡**（91 可达） |
| G05 `SearchTarget` 不清目标 | 无 | **需新卡**（跨所有索敌路径） |
| G06 `CentipedeKing.AttackTarget` | **MB-3**（§2.3.0 明列 `:42-83`） | 已归属 |
| G07 `BigHeart.AttackTarget` | **MB-1**（§2.3.0 trace + R04） | 已归属 |
| G08 `is_active_seek_race` 9 race | **明确排除**（计划件 §6.9：「不得顺手扩大，另立卡」） | **需新卡** |
| G09 `IsProperTarget` 折入 | 无 | **需新卡**（与 G25 合并） |
| G10 视野驱动 | 前件 `audit-baseobject-run-coverage-20261018.md` D03/P38 已登记「调用点/节拍/门全不同」 | 无实现归属 ⇒ **需新卡** |
| G11 `ExplosionSpider.AttackTarget` | **MB-6**（§2.3.0 明列 `:47-77`） | 已归属（数据不可达 ⇒ window 需 owner 裁决） |
| G12 `ElfWarrior.AttackTarget` | **MB-7**（§2.3.0 明列 `:72-103`） | 已归属（不可达） |
| G13 `StickMonster` 链 | **MB-1**（R38） | 已归属 |
| G15 `DoubleCritical` | 无 | **需新卡**（不可达；可与 E01 同卡登记） |
| G16 守卫攻击 | **MB-3** 覆盖 `ArcherGuard`（R02）；`SuperGuard` 无 | 112 已归属；11/12 需新卡（计划件 §2.5 R39 建议 `openmir2-superguard-run`） |
| G17 `GasMoth.Run` | **MB-3**（R21） | 已归属 |
| G18 `AttackTarget` 分派骨架 | 无；可与计划件 MB-0（Run dispatch，L）合并设计 | 建议并入 MB-0 |
| G20 `ClearTargetCreat` 跳项 | 无 | **需新卡**（与 G05 同卡） |
| G21/G22/G23/G24/G25/G26 | MB-7 / 与 G01/G11/G15 同批 / MB-9 / MB-9 / 与 G09 / 与 G16 | 见各行 |

> **统计（G01–G26 共 26 行）**：已被 MB 批次覆盖 **10 行**（G03、G06、G07、G11、G12、G13、G17，另 G21→MB-7、G23/G24→MB-9）；**部分覆盖 1 行**（G16：ArcherGuard-112 随 MB-3 R02，SuperGuard-11/12 需新卡——计划件 §2.5 R39 已建议 `openmir2-superguard-run`）；**随同批不单列 2 行**（G22 随 G01/G11/G15 批、G26 随 G16-112 批）；**无归属需新卡 11 行**（G01、G02、G04、G05、G08、G09、G10、G15、G18、G20、G25；G25 与 G09 同源合并实现）；**死类/登记 2 行**（G14、G19）。10 + 1 + 2 + 11 + 2 = 26。

---

## 6. 机判建议（固定时钟、无 RNG 优先）

| 编号 | 针对 | 判据 |
|---|---|---|
| MCS-A1 | G05 / A16 | **初始 `target_creature = None`**，构造 `visible_actors = [death=true 的无关怪, 合法目标]`（**无效项必须在合法项之前**）；调 `monai::search_target`：oracle `ClearTargetCreat` 后 `RemoveAt` 使合法项被跳过、收尾 `searchTarget == null` ⇒ 断言 `target_creature == None`。当前为 `Some(合法)` ⇒ 红。**不得**用 `[合法, 无效]`（oracle 收尾 `SetTargetCreat(合法)`，两边都 `Some`，断言 `None` 会误红）。 |
| MCS-A2 | G20 / A17 | 构造 `visible_actors` 两个连续的无效项 + 一个合法项；断言 oracle 口径下第二个无效项仍在列表中（索引跳项），Rust 当前全删 ⇒ 红。 |
| MCS-A3 | G08 / S02 | 固定时钟，race 82/84/90/91/97/106/119 各一只 + 玩家 5 格内；推进 `>8s`，断言 `target_creature` 出现。当前全为 `None` ⇒ 红。 |
| MCS-A4 | G01 / E09 | race 82 + 目标在 2 格对角（`SpitMap` 命中）：断言 `pending_ai_attacks` 的伤害走 `GetMagStruckDamage` 且 `Random(target.SpeedPoint)` 被抽一次；当前 `get_attack_dir` 对 2 格目标失败 ⇒ Rust 根本不攻击（只 `SetTargetXy`，无伤害/无抽数），与 oracle 的喷吐路径不同 ⇒ 红。 |
| MCS-A5 | G02 / E05 | race 90 邻接 + 到点：断言 `Random(AntiPoison+20)` 在 `GetMagStruckDamage` 之后被抽；当前不抽 ⇒ 红。 |
| MCS-A6 | G03 / E02 | race 87 + 目标 4 格：断言 `FlyAxeAttack` 的 `CanFly`/`Random(DC)`/`RM_FLYAXE` 与 `_mNAttackCount` 计数；当前走邻接门直接 false（不攻击）⇒ 红。 |
| MCS-A7 | G04 / E06 | race 91 + 邻接：断言 `AntiMagic>=0` 门与 `GetMagStruckDamage`；当前物理路径 ⇒ 红。 |
| MCS-A8 | G06 / E10 | race 107 + ViewRange 内两个 proper 目标：断言每个目标各抽一次 `Random(4)`、`==0` 才抽 `Random(3)`、`RM_DELAYMAGIC` 600ms、`TargetCret` 取最后一个；当前基类 ⇒ 红。 |
| MCS-A9 | G07 / E11 | race 115 + 16 格内目标、**无 `TargetCret`**：断言仍发 `RM_10205` + 200ms 延时；当前要求 `TargetCret` ⇒ 红。 |
| MCS-A10 | G09 / A14 | `is_proper_target` 对 `invisible=true` 且 `death=false` 的目标：oracle `IsProperTarget` 为 true，Rust 为 false ⇒ 红。 |
| MCS-A11 | G18 / E14 | 结构判据：race → 期望 `AttackTarget` 入口表；当前 `monster_run` 只有一条固定调用 ⇒ 红。 |
| MCS-A12 | G10 / V01–V03 | 固定时钟，玩家在怪物 `ViewRange` 边缘：按 oracle 距离门该怪不应收到 `RM_UPDATEVIEWRANGE`；断言 Rust `visible_actors` 不含该玩家（当前含）⇒ 红。 |

> 固定时钟优先用 `World::clock_ms` 注入；需要 RNG 的判据用 `RandomNumber::with_seed` 并把期望抽取次数写成断言（沿用 `monai.rs`/`combat.rs` 既有测试口径）。

---

## 7. 自检与残留物清单

- 本文档为唯一交付物；**未修改任何 `.rs`/`src`/`sql`**，不改 `crates/{protocol,storage}`，不起服务、不占端口。
- 交付方式：审计 worktree `/e/tmp/impl-ataudit`（detached `7e8d47426`）内 commit（不 push），`git format-patch -1 --stdout > /e/tmp/ataudit.patch`。
- oracle 行号自检：临时脚本 `/e/tmp/ataudit-check.py` 扫描全文所有 `文件:起-止` 区间并核对「文件存在 + 起止在 1..行数内」；另按 §1 逐条抽查首/末行文本。临时脚本不属于仓库。**单行号引用（`文件:NNN`）不在该脚本覆盖内**，复验时逐条人工核对（含订正 `TeleMonster.cs:27`、`CentipedeKingMonster.cs:85`、`PlayObject.Attack.cs:627`）。
- 主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2` 未动；`E:/MirServer` 未触碰；未跑任何构建/测试。
- 残留物：`/e/tmp/impl-ataudit`（本 worktree，commit 后按收尾清单移除）、`/e/tmp/ataudit.patch`、`/e/tmp/ataudit-check.py`（取证用临时物）。
