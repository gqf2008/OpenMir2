# 玩家 Run 真实节拍裁定证据设计（2026-10-18）

- 卡：`openmir2-run-gap-player-run-quantification`
- 性质：**只读证据设计件**——回答「怎么用机器可判的方式裁定玩家 `Run` 的真实节拍」，不实现、不改代码。
- 基线：`b2df310fc`（detached worktree `/e/tmp/impl-runquant`）。
- 写集：仅本文件；**零** `.rs` / `src/**` / `sql/**` / 冻结面；未起服务、未占端口。
- 环境记录：裁定日 9 个游戏端口被外部 C# 栈占用（`E:\MirServer\port-status.ps1` 实测 9 个游戏端口 5000/5100/5500/5600/5700/6000/7000/7100/7200 全部 LISTENING，另有 MySQL 3306），本文件**只记录、不停栈**。
- 判级：本设计件本身 **L**；其裁定对象（玩家 `Run` 节拍 / 恢复数值 / 掉血击次）按 `AGENTS.md`「判据分级」为 **R**（时序 + 战斗数值）。真机判据未跑/未绿时，不得用「语义等价 / 代码看起来一样 / 与 oracle 同构」放行。
- 引用纪律：oracle 行号逐条 `sed -n` 实测；Rust 侧只引符号/文件路径，不写 `:行号`；未实测/推断的结论逐条标注。

> **一句话**：oracle 的玩家 `Run()` 不是 50ms 一次——`WorldServer.ProcessPlayObjectData` 的 `(now - RunTick) > RunTime(250)` 门把它名义上钉在 **300ms/次**；由此 `BaseObject.Run` 的 `recoveryTick=(now - AutoRecoveryTick)/20` 名义上是 **15/次 = 50/s**（任意 P≥250ms 时 **46.3–50.0/s**），而 `bins/game-svr/src/stamina.rs` 以「50ms/次、每次 +2 = 40/s」重放。既有「第 86 击掉 1 HP」证据没有时间戳，`≈662ms` 是从 40/s 反解出来的，不能单独裁定；本文件给出 A（构造级/注入）与 B（真机 53 帧）两套可执行证据方案与判据阈值。

---

## 0. 结论摘要

1. **名义节拍**：`UserProcessor` 50ms → `ProcessHumans` → `ProcessPlayObjectData`；逐玩家门 `(dwCurTick - RunTick) > RunTime(250)`；`RunTime=250`；50ms 量化 ⇒ **300ms/次**，即 **3.333 Run/s**。通过门时 `RunTick` 被写成当轮 `dwCurTick`。
2. **恢复速率闭式**：`R(P)=floor(P/20)/P`（P=Run 间隔；`AutoRecoveryTick` 运行期只在 `Run` 内更新，另有 ctor 初始化）。P=300ms ⇒ `15/300ms = 50/s`；**任意 P≥250ms ⇒ R∈[46.3, 50.0]/s**（锯齿：P=20k 时 50/s；P=20k+19 时最低，最小可达 P=259ms 的 46.3/s）。**40/s 在 oracle 结构上只对应 P≈50ms**（`floor(50/20)/50 = 2/50 = 40/s`）——而门禁止真人玩家 50ms 一次 `Run`，除非 `RunTick -= 100` 被高频动作消息连续下推（≥3 条/50ms 可持续到 P=50ms；见 §1.2）。
3. **Rust 现状**：`stamina.rs` 的 `RUN_INTERVAL_MS=50` + `stamina_tick` 每 50ms `+2` ⇒ **40/s**；`recover.rs` 的 `recovery_tick` 每个 world tick（200ms）调用一次 ⇒ 池药消费速率 **`5/ceil(D/200)` per s**（`D=600-min(400,10L)`）：仅 **L≥40（D=200）是 5/s（1.5×）**；L=30..39（D=210..300）是 **2.5/s（0.75×）**；L=20..29（D=310..400）是 2.5/s（1.5×，oracle 为 1.67/s）；L≤19（D≥410）约 **1.67/s（1.0×）**。两者都是**调用节拍**差，不是公式差。
4. **既有证据缺口**：`17-attacker-hp-trajectory-csharp-c2.txt` 只有 `tick` 序号，没有 `ts_ms`；`≈662ms` 是从「40/s × Δ − 30」反解的，和「50/s × Δ − 30」是同一方程的两组解。构造级敏感度仿真显示：P=300ms + `RunTick -=100` 时，Δ≈500ms 也会在第 86 击掉 1 HP（附录 A，**仿真不是真机证据**）。
5. **首选方案**：A1（构造级 C# harness，非虚调用 `BaseObject.Run`——`DynamicMethod`+`OpCodes.Call` 或 `ProbeObject.RunBase()`，不占游戏端口、不改 oracle 源码）；A2（真正确定性时钟 + 定时器注入）需要改 `src/OpenMir2`/`src/GameSrv`，**不在 T-* 白名单** ⇒ **需 owner 裁决 O3**。现有 `MIR2_CLOCK_MS` **只有 Rust 半，且语义是「origin + 真实单调经过」，不是可快进/可暂停的虚拟时钟**。
6. **判据分流**：50/s ⇒ 改 `stamina.rs` 的 Run 周期（300ms）与 `drain_recovery` 的调用节拍；40/s ⇒ 保留 40/s 并登记 T-*（先解释门为何被绕过）；不可判 ⇒ 保留现状 + T-* + R 卡保持 `in-progress`（`window_required: true`）。

---

## 1. oracle 侧事实（逐条实测）

### 1.1 驱动链

| 环节 | 实测位置 | 实测行原文 | 结论 |
|---|---|---|---|
| 玩家线程周期 | `src/GameSrv/Word/Threads/UserProcessor.cs:7` | `public UserProcessor() : base(TimeSpan.FromMilliseconds(50), "UserProcessor")` | **50ms** |
| 线程体 | 同文件 `:31` | `SystemShare.WorldEngine.ProcessHumans();` | 每次 tick 跑 `ProcessHumans` |
| 定时器实现 | `src/OpenMir2/TimerScheduledService.cs:19` | `_timer = new PeriodicTimer(timeSpan);` | `PeriodicTimer`，真实墙钟 |
| 等待循环 | 同文件 `:49` | `while (await _timer.WaitForNextTickAsync(stoppingToken))` | 每 tick 调 `ExecuteInternal` |
| 启动点 | `src/GameSrv/GameServer.cs:18` | `_ = GameShare.UserProcessor.StartAsync(stoppingToken);` | 单实例；全仓 `grep ProcessHumans()` 只有本调用点 |
| 玩家列表扫描 | `src/GameSrv/Word/WorldServer.cs:481` / `:617` / `:625` | `ProcessHumans()` / `ProcessPlayObjectData();` / `private void ProcessPlayObjectData()` | 每 50ms 扫一遍 |
| 单次扫描预算 | `:720` / `:723` | `if ((HUtil32.GetTickCount() - dwCheckTime) > M2Share.HumLimit)` / `ProcHumIdx = nIdx;` | `HumLimit=30ms`；超过则断点续扫（玩家多/负载高时单个玩家**可能不是**每 50ms 被访问一次） |

### 1.2 逐玩家 `Run` 门与 `RunTick` 写入

```csharp
// src/GameSrv/Word/WorldServer.cs:645-647（实测）
if ((dwCurTick - playObject.RunTick) > playObject.RunTime)
{
    playObject.RunTick = dwCurTick;
```

- `dwCurTick = HUtil32.GetTickCount()` 在 `ProcessPlayObjectData` 入口取一次（同一次扫描内所有玩家共用）。
- 严格 `>`：elapsed=250 **不**通过；50ms 调用网格上第一个通过点是 **300ms**（正常定时器）。若 `PeriodicTimer` 实际周期 T≠50ms，则名义周期 `P=(floor(250/T)+1)*T`（T=50⇒300、T=51⇒255、T=55⇒275、T=60⇒300、T=63⇒252；见 §1.4）。
- **动作消息会下推 `RunTick`**：`src/GameSrv/Word/WorldServer.cs:867-880`（`switch (defMsg.Ident)` 的 `CM_TURN/CM_WALK/CM_SITDOWN/CM_RUN/CM_HIT/CM_HEAVYHIT/CM_BIGHIT/CM_POWERHIT/CM_LONGHIT/CM_WIDEHIT/CM_FIREHIT/CM_CRSHIT/CM_TWINHIT`）执行 `playObject.RunTick -= 100;`。**`CM_EAT`/`CM_SPELL` 不在该列表**——这是 §4.2 无扰动测量的依据。`ProcessUserMessage` 由网络线程 `src/M2Server/Net/ChannelMessageHandler.cs:213/218` 调用，按消息到达即下推，不等 50ms tick。
- ⇒ 「玩家真实 Run 周期」不是单一常数：静止 ≈300ms；动作消息（每条 `RunTick -= 100`）可把它下推——按 50ms 网格：1 条/300ms ⇒ P≈300ms，1 条/100ms ⇒ P≈200ms，1 条/50ms 或 2 条/50ms ⇒ P≈100ms，**≥3 条/50ms 的持续/突发 ⇒ P=50ms（40/s）**。即下界是 50ms（见 §0 第 2 条与附录 A）。

### 1.3 `RunTime` 初值（基类 ctor vs 派生覆盖）

| 位置 | 实测行原文 | 值 |
|---|---|---|
| `src/M2Server/Actor/BaseObject.cs:439` | `RunTick = HUtil32.GetTickCount() + M2Share.RandomNumber.Random(1500);` | 基类先给随机相位 |
| 同文件 `:440` | `RunTime = 250;` | 基类默认 250 |
| `src/M2Server/Player/PlayObject.cs:898` | `RunTick = HUtil32.GetTickCount();` | **玩家 ctor 覆盖**：无随机相位 |
| 同文件 `:899` | `RunTime = 250;` | 玩家 250（派生 ctor 在基类 ctor 之后运行） |

### 1.4 定时器抖动与 .NET 行为（**未在本机实测**，标为运行时风险）

- `PeriodicTimer` 是真实墙钟定时器；Windows 默认时钟中断粒度约 15.6ms（未抬 `timeBeginPeriod` 时）。⇒ 实际 T 可能落在 50–65ms，而不是精确 50ms。
- .NET `PeriodicTimer` 不累积漂移，但**不排队错过的 tick**：`ExecuteInternal` 超过一个周期时，下一次 `WaitForNextTickAsync` 可能立即返回（合并/跳过）。⇒ 高负载下有效周期只会**变长**，不会变短。
- 名义 Run 周期与 T 的关系：`P=(floor(250/T)+1)*T`（公式推导，非运行时实测）。T=50→300、51→255、55→275、60→300、63→252、65→260。**P 的下界 ≈252ms，上界随负载增大**。
- `Environment.TickCount`（`src/OpenMir2/HUtil32.cs:36-39`，实测 `return Environment.TickCount;`）本身也受系统时钟粒度量化；短窗口内的 `dwCurTick` 差值是 15.6ms 的倍数级，不是 1ms 精度。判据阈值必须按这个粒度留余量。

### 1.5 `BaseObject.Run` 的体力/魔力点段（`src/M2Server/Actor/BaseObject.Base.cs:25-109`）

> 文件归属：下表前 9 行（`:25`–`:109`）在 `src/M2Server/Actor/BaseObject.Base.cs`；最后两行（`HealthSpellChanged` / `IncHealthSpell`）在 **`src/M2Server/Actor/BaseObject.cs`**（同名 partial 类的另一个文件），**不在** `BaseObject.Base.cs`。

| 行 | 实测文本/语义 |
|---|---|
| `:25` | `public virtual void Run()` |
| `:37` | `if (!Death)` |
| `:39` | `int recoveryTick = (HUtil32.GetTickCount() - AutoRecoveryTick) / 20;` |
| `:40` | `AutoRecoveryTick = HUtil32.GetTickCount();`（**`Run` 路径内唯一写点**；另有 ctor 初始化 `src/M2Server/Actor/BaseObject.cs:445`，`grep AutoRecoveryTick src/` 实测） |
| `:41-42` | `HealthTick += recoveryTick; SpellTick += recoveryTick;` |
| `:44-55` | `HP<MaxHP && HealthTick>=HealthFillTime` ⇒ `HP += MaxHP/75+1`（夹 MaxHP）⇒ `HealthSpellChanged()` |
| `:57-68` | `MP<MaxMP && SpellTick>=SpellFillTime` ⇒ `MP += MaxMP/18+1` ⇒ `HealthSpellChanged()` |
| `:74-81` | `HealthTick/SpellTick >= FillTime` ⇒ 归零（**满血时也会清零**，所以正增量在满血时会被截掉） |
| `:105-109` | `HealthTick < -HealthFillTime && HP>1` ⇒ `HP -= 1; HealthTick += HealthFillTime; HealthSpellChanged()` |
| `BaseObject.cs:700-710` | `HealthSpellChanged()`：`Race==Play` ⇒ `SendUpdateMsg(RM_HEALTHSPELLCHANGED,...)`；`ShowHp` ⇒ `SendRefMsg(...)` |
| `BaseObject.cs:952-975` | `IncHealthSpell(nHp,nMp)`：夹 `MaxHP/MaxMP` 后调 `HealthSpellChanged()` |

配置（实测）：`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1323-1324` ⇒ `HealthFillTime=300; SpellFillTime=800;`；部署件 `/e/MirServer/M2GameSvr/Server.conf:785-786` ⇒ 同值（只读实测）。⇒ 除非窗口使用的部署件改过这两键，否则按 300/800 计算。

### 1.6 `PlayObject.Run` 的药品池恢复段（`src/M2Server/Player/PlayObject.Message.cs:19-766`）

> **口径订正**：任务书写的 `:64-110` 与实测不符——药品池段实测是 `:70-141`；`HealthSpellChanged` 不在该段直接调用，而是经 `:126 IncHealthSpell(...)` → `src/M2Server/Actor/BaseObject.cs:952-975` → `HealthSpellChanged()`。

| 行 | 实测文本/语义 |
|---|---|
| `:19` | `public override void Run()` |
| `:70` | `if (!Death && ((IncSpell > 0) \|\| (IncHealth > 0) \|\| (IncHealing > 0)))` |
| `:72` | `int dwInChsTime = 600 - HUtil32._MIN(400, WAbil.Level * 10);` |
| `:73` | `if (((HUtil32.GetTickCount() - IncHealthSpellTick) >= dwInChsTime) && !Death)` |
| `:75-76` | `incHealthTick = _MIN(200, now - IncHealthSpellTick - dwInChsTime); IncHealthSpellTick = now + incHealthTick;` |
| `:92-122` | `nHP/nMP` 按 `PerHealth/PerSpell/PerHealing` 从 `IncHealth/IncSpell/IncHealing` 扣减/置零 |
| `:123-125` | `PerHealth = PerSpell = Level/10+5; PerHealing = 5;` |
| `:126` | `IncHealthSpell(nHP, nMP);` ⇒ `HealthSpellChanged()` |
| `:127-135` | `HP==MaxHP ⇒ IncHealth=0, IncHealing=0; MP==MaxMP ⇒ IncSpell=0;` |
| `:139-141` | else 支：`IncHealthSpellTick = HUtil32.GetTickCount();`（**空池时每次 Run 都把相位对齐到当前 Run**） |
| `:765` | `base.Run();`（先跑药品池段，再到 §1.5 的体力段） |
| `:1736-1739` | `RM_HEALTHSPELLCHANGED` ⇒ `SM_HEALTHSPELLCHANGED(53)`，载荷 `(ActorId, HP, MP, MaxHP)` ⇒ 抓包字段 `param=HP, tag=MP, series=MaxHP` |

初值（实测）：`src/M2Server/Player/CharacterObject.cs:83-86` ⇒ `PerHealth=5; PerHealing=5; PerSpell=5; IncHealthSpellTick=GetTickCount();`。

**节拍闭式**（`P`=Run 周期，`D=dwInChsTime`，`s_k=t_k - IncHealthSpellTick_{k-1}`）：
- 若 `s_k >= D`：消费一次，`IncHealthSpellTick_k = t_k + min(200, s_k-D)`；下一轮 `s_{k+1}=P - min(200, s_k-D)`。
- 空池时 else 支把相位重置为 `s≈P`。因此**正常「空池→吃药」路径**的首个 `s=P`：
  - `L≥30`（`D=600-min(400,10L) ≤ 300 = P`）：`s=300≥D`，此后每个 Run 都通过门 ⇒ **每 Run 一次 = 3.33/s**。从 `s=P` 出发不会停在 250ms：P=300、D=200 时 `s` 走 **300↔200 两周期**（300→200→300，250 只是均值；递推在 `s=250` 有一个中性不动点 `250→50→250`，但从初值 300 不可达），D=200..300 时走 300↔D 两周期；两种情况都是每 Run 消费；
  - `L<30`（`D>P`）：`s=P` 不通过，隔一次再通过 ⇒ **每 2 Run 一次 = 1.67/s**（公式推导；B2 必须记录前 2 个间隔并剔除相位）。
- 每次消费量 = `PerHealth/PerSpell`（L255 ⇒ `255/10+5=30`；L51 ⇒ 10），直到池空或 HP/MP 满。

### 1.7 结论：名义值（不是真机实测值）

| 量 | 闭式 | 名义值（L255/Job0，MaxHP=20669，MaxMP=15） |
|---|---|---|
| Run 周期 P | `(floor(250/T)+1)*T`，T=50ms | **300ms** |
| Run 频率 | `1/P` | **3.333/s** |
| `recoveryTick`/Run | `floor(P/20)` | **15** |
| 体力点速率 | `floor(P/20)/P` | **50/s**（P≥250ms 时 46.3–50.0/s） |
| HP 填充量 | `MaxHP/75+1` | **276** |
| HP 填充周期 | `HealthFillTime / R` | **6.0s**（R=50）；R=48 ⇒ 6.25s |
| MP 填充量 | `MaxMP/18+1` | **1**（15/18+1=0+1） |
| MP 填充周期 | `SpellFillTime / R` | **16.0s**（R=50）；R=40 ⇒ 20.0s |
| 池药消费（L255） | 1/Run（L≥30） | **3.333/s**，每次 30 HP/MP 池量 |

> **未实测标注**：上表是源码闭式 + 部署配置（300/800）的**名义值**；真实 `P` 与 `R` 必须由 §3/§4 的证据读出。`PeriodicTimer` 抖动、系统时钟粒度、`HumLimit` 切片的组合效应**未在本机实测**。

---

## 2. Rust 侧现状（符号级）

| 项 | 符号/文件 | 行为 |
|---|---|---|
| 体力段重放周期 | `mir2-rs/bins/game-svr/src/stamina.rs` 的 `RUN_INTERVAL_MS` | `50ms` |
| 体力段重放体 | 同文件 `stamina_tick` | `while cursor + 50 <= now { cursor += 50; recovery=(cursor-auto)/20; auto=cursor; health += recovery; }` ⇒ **每 50ms +2，长期 40/s** |
| 体力段调用 | `mir2-rs/bins/game-svr/src/main.rs` 的 `drain_stamina`（由 `drain_run_downlinks` 每 world tick 调） | 每个 200ms tick 一次；内部按 50ms 补齐 ⇒ 200ms 内 +8 |
| 药品池段 | `mir2-rs/bins/game-svr/src/recover.rs` 的 `recovery_tick` | 公式按 `PlayObject.Message.cs:70-141` 1:1；**由 `drain_recovery` 每个 world tick 调一次** |
| world tick | `mir2-rs/crates/world/src/world.rs` 的 `TICK_INTERVAL_MS` / `World::tick` | **200ms** |
| 怪物恢复路径 | `mir2-rs/crates/world/src/monai.rs` 的 `base_object_run_monster`（内联式 `(now - e.auto_recovery_tick) / 20`；`recovery_tick` 只是该函数内的**局部变量**，不是可引用的符号） | 每个 200ms tick 跑该内联式 ⇒ `+10/200ms = 50/s`；**与玩家 `stamina.rs` 的 40/s 不是同一条路径** |
| 宿主节拍 | `main.rs` 的 `sim_loop` | 每轮 `world.tick()` 后跑 `drain_recovery`/`drain_run_downlinks`；`sleep` 只补足到下一个 200ms，超时则立即下一轮 |
| 时钟注入 | `mir2-rs/bins/game-svr/src/clock.rs` 的 `MIR2_CLOCK_MS`/`now_ms` | `origin + 真实单调经过`；**不是**可控制/可快进的虚拟时钟 |
| 配置 | `bins/game-svr/src/config.rs` 读 `HealthFillTime/SpellFillTime` | 默认 300/800（与 oracle 同） |

### 2.1 同角色对照表（L255/Job0，MaxHP=20669，MaxMP=15，HealthFillTime=300，SpellFillTime=800）

| 恢复路径 | oracle 名义 | Rust 当前 | 比值（Rust/oracle） |
|---|---|---|---|
| 体力点累积 | `15/Run × 3.333 = 50/s` | `+2/50ms = 40/s` | **0.80×** |
| 体力点 HP 填充 | 每 6.0s +276 ⇒ **46.0 HP/s** | 每 7.5s +276 ⇒ **36.8 HP/s** | 0.80× |
| 体力点 MP 填充 | 每 16.0s +1 ⇒ **0.0625 MP/s** | 每 20.0s +1 ⇒ **0.05 MP/s** | 0.80× |
| 药品池消费（L255，PerHealth=30） | 3.333/s ⇒ **100 池量/s** | 5/s ⇒ **150 池量/s** | **1.50×** |
| 药品池消费（L51，PerHealth=10） | 3.333/s ⇒ 33.3/s | 5/s ⇒ 50/s | 1.50× |

- 表中「HP/s」是**长期平均**（HP 始终低于 MaxHP、无其他扣血/加血）；HP 填满后 `HealthTick` 仍累积但不再加 HP，平均会下降。
- `stamina.rs` 的 200ms tick 内补 4 个 50ms 步，所以 world tick 的 200ms ≠ 40/s 的来源；**40/s 的来源是「每次 Run +2」的模型**。即使 world tick 改成 300ms，按 50ms 补步仍是 40/s。
- `recover.rs` 的公式本身与 oracle 同；差在 `drain_recovery` 的**调用节拍**（200ms vs 名义 300ms）。Rust 侧速率 = `5/ceil(D/200)` per s（`D=600-min(400,10L)`）：**只有 L≥40（D=200）是 5/s = 1.5×**；L=30..39（D=210..300）是 2.5/s = **0.75×**；L=20..29（D=310..400）是 2.5/s = 1.5×；L≤19（D≥410）约 1.67/s = **1.0×**（与 oracle 每 2 Run 一次相同）。上表 L255/L51 都落在 L≥40 档，故 1.5× 只对这两行成立。⇒ 若只改 `stamina.rs` 而不改 `drain_recovery`：L≥40 或 L=20..29 的池药场景仍快 1.5×；L=30..39 会慢 0.75×；L≤19 基本无差。
- `crates/world` 的 200ms `World::tick` **不是**本卡必须改的面：最小修法是宿主 `bins/game-svr` 侧的每玩家 `Run` 调度；只有把玩家 `Run` 迁进 ECS 才需要动 `crates/world`（当前迁移第 2 批已按 §6.0-5 暂缓）。

---

## 3. 方案 A：构造级 / 注入（首选，不占游戏端口）

### 3.1 现状：`MIR2_CLOCK_MS` 只有 Rust 半，且不是虚拟时钟

- **Rust 半存在**：`clock.rs` 定义 `MIR2_CLOCK_MS`；设了合法值 ⇒ `now_ms() = origin + 自首次取钟起的真实单调毫秒`，`world.clock_ms` 以同一 origin 起算。**未设/空 ⇒ 与改动前逐字节相同**；非法值 fail-closed。
- **C# 半不存在**（实测）：`src/OpenMir2/HUtil32.cs:36-39` 的 `GetTickCount()` 直接 `return Environment.TickCount;`；全仓 `grep MIR2_CLOCK|Environment.GetEnvironmentVariable` 在 `src/GameSrv`/`src/OpenMir2`/`src/M2Server` 无时钟注入入口。既有 `mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs` 的 `DOTNET_STARTUP_HOOKS` 手法是用反射替换 `OpenMir2.RandomNumber` 的私有 `random` 字段；**拦不住静态方法 `HUtil32.GetTickCount`**。
- **关键语义**：`MIR2_CLOCK_MS` 是「同起点」对齐，不是「可快进/可暂停」的虚拟时钟。即便 C# 半按同样语义落地，它也只消除两侧 epoch 差，**不消除真实定时器抖动、不能把 60s 压成 1s**。⇒ 真正的确定性方案（A2）需要**额外的虚拟时钟 + 虚拟定时器**契约，不是现有 `MIR2_CLOCK_MS` 的直接扩展。
- 卡 `openmir2-m3-clock-injection` 已记录 C# 半的待裁决路径：(a) Mono.Cecil IL patch / (b) 影子源码重建 / (c) 不改 oracle、夹具按墙钟编排（见 `clock.rs` 模块头注释与卡上 worker-5 盘点 comment）。本文件不替 owner 选路。

### 3.2 A1（推荐先做）：构造级 C# harness，不改 oracle 源码

**目标**：在**不启动游戏栈、不占 7000/7100/7200 等端口**的独立测试进程里，驱动 oracle 的 `BaseObject.Run()`，记录每次 Run 的 `t_ms`、`RunTick`、`recoveryTick`、`HealthTick/SpellTick`、`HP/MP`。

**做法**：
1. 新建测试工程（建议 `tools/run-cadence-probe/` 或 `mir2-rs/tests/parity/m3/run-cadence-probe/`，**不进 oracle 部署包**），引用 `src/M2Server/M2Server.csproj` 与 `src/OpenMir2/OpenMir2.csproj`（`PlayObject` ctor 是 `public`，`HealthTick/SpellTick/AutoRecoveryTick/WAbil/RunTick/RunTime` 都是 `public`，实测）。
2. `new PlayObject()`；设 `WAbil.Level=255, Job=0, HP/MaxHP=20669, MP/MaxMP=15`；`HealthTick=SpellTick=0`；`AutoRecoveryTick=RunTick=now`；`Death=false`；`Master=null`；`SlaveList` 空；`TargetCret/LastHiter/ExpHitter=null`；每次调用后把 `VerifyTick` 推到未来（跳过 30s `VerifyMapTime` 分支，避免 `Envir` 依赖）。
3. **非虚调用基类**：`typeof(BaseObject).GetMethod("Run").Invoke(obj, null)` **不行**——`BaseObject.Run` 是 `public virtual`，`MethodInfo.Invoke` 对 virtual 方法走**虚分派**，实际执行的是 `PlayObject.Run`（本机 net8.0/net9.0 最小 `Base/Derived` 例实测打印 `DERIVED`）。正确做法二选一：① `DynamicMethod` + `OpCodes.Call` 直调 `BaseObject.Run`；② `sealed class ProbeObject : BaseObject { public void RunBase() => base.Run(); }`（`BaseObject` 是具体类、ctor 为 `protected`，见 `src/M2Server/Actor/BaseObject.cs:19/371`；不能直接 `new BaseObject()`），`base.Run()` 编译为非虚 `call`，等价于直调 `BaseObject.Run`。这样才能绕过 `PlayObject.Run` 的 40 余段（避免 `CastleMgr/ManageNPC/Envir` 等未初始化依赖）。这是 A1 能「不改源码」的关键；**两种非虚调用方案需先 spike 验证**。
4. 用 `PeriodicTimer(50ms)`（与 oracle 同类型）驱动；每 tick 读 `now=Environment.TickCount`，复刻 `:645` 的 `(now - obj.RunTick) > obj.RunTime` 门；通过则记录并调用基类 `Run`，然后 `obj.RunTick=now`。
5. 记录 NDJSON（schema 见附录 B）：`seq,t_ms,delta_ms,run_tick_before,recovery_tick,health_tick_before/after,spell_tick_before/after,hp_before/after,mp_before/after,gated`。
6. 运行时长 ≥120s（≥400 个名义 Run）或直到 500 次 Run；重复 3 次。

**判据**：
- 每次 Run 的 `delta_ms` 中位数 ∈ [250, 350]ms；`recovery_tick = floor(delta_ms/20)`（300ms ⇒ 15）；稳定段（排除首尾相位）**逐 Run 断言 `HealthTick` 增量 = `recovery_tick`**；或 **3s 窗口增量 ≈150**（10 个 Run；窗口边界量化 ±15）；或 ≥10s 斜率 ≈500/s。**不要用 1s 窗口**：300ms Run 网格上 1s 只含 3 或 4 个 Run，增量是 **45 或 60**，不是 46–50 ⇒ **支持 50/s**。
- `delta_ms` 中位数 ∈ [40, 60]ms，或 3s 增量 = 120（50ms 步长下精确） ⇒ **支持 40/s**（同时意味着门被绕过；必须解释）。
- `delta_ms` 的 P90 > 400ms、双峰、或 Run 次数 < 名义值的 80% ⇒ **flaky/不可判**（定时器被负载拖长；加隔离或改用 B）。
- 红检（A1 的「改坏必红」牙齿）：① 把 `PeriodicTimer` 改成 200ms ⇒ 应得到 ~400ms 周期（严格 `>` + 250ms 门 ⇒ 400ms）。② 把 `RunTime` 改成 50ms ⇒ **期望 P=100ms、`recovery=5/100ms=50/s`**——50ms 网格上严格 `(now-RunTick)>50` 的首个通过点是 **100ms，不是 50ms**。③ 若要构造 40/s 红检，须把 `RunTime` 设为 **≤49**（如 0 ⇒ P=50ms、`2/50ms=40/s`），或改用 **25ms 网格**（RunTime=50 ⇒ 首个通过点 75ms、`3/75ms=40/s`）。三条都通过，才证明 harness 能区分两假设且门逻辑正确。

**T-* / O3 状态**：
- A1 **不改** `src/OpenMir2`/`src/GameSrv`/`src/M2Server` 的任何字节；新增的是测试工具工程（T-1 同族的工具侧例外），不需要 oracle 偏离白名单。
- 如果 harness 为了跳过 `VerifyMapTime` 而需要在 oracle 里加 `internal` 可见性/测试钩子，那就**越出 A1**，按 A2 走 O3。
- **未实测风险（P0 spike）**：`BaseObject.Run` 的 30s `VerifyTick` 分支、`HP==0 Die()`、`Master/SlaveList` 清理、`TargetCret` 清理都可能触碰未初始化状态。非虚调用（`DynamicMethod`/`OpCodes.Call` 或 `ProbeObject.RunBase()`）+ 把 `VerifyTick` 推远可以避开大部分；**必须先做 30 分钟可行性 spike 再承诺 A1 可跑**。若 spike 失败，退回 A2（O3）或 B。

### 3.3 A2（确定性最强，但需 owner O3）：oracle 时钟 + 定时器 + 探针注入

需要三处 C# 改动（均在 oracle 源码面）：
1. `src/OpenMir2/HUtil32.cs` 的 `GetTickCount()`：读注入的虚拟毫秒（或 `TimeProvider`），未设 ⇒ 原样 `Environment.TickCount`（逐字节不变）。
2. `src/OpenMir2/TimerScheduledService.cs` 的 `PeriodicTimer`：注入可控 tick 源（虚拟调度器），用于把 50ms 节拍变成可快进/可暂停。
3. `src/GameSrv/Word/WorldServer.cs:625-647` 与 `src/M2Server/Actor/BaseObject.Base.cs:37-42`：测试模式下每 Run 落一条探针（或将探针做成 `internal` 事件）。

**T-* / O3 状态**：这三处都在 oracle 七进程的源码面。白名单里**有** oracle 源改动先例（T-2 改 `src/M2Server/Net/TCP/TCPNetChannel.cs` + `ChannelMessageHandler.cs`），但**没有**覆盖 `HUtil32.GetTickCount` / `TimerScheduledService` / `WorldServer.ProcessPlayObjectData` / `BaseObject.Run` 探针的条目 ⇒ **需 owner 裁决 O3**（即使 owner 选择新增 T-* 条目，也必须先批准再改）。与卡 `openmir2-m3-clock-injection` 的 (a)/(b)/(c) 同一决策。在 owner 裁决前，**不得**为了本卡改 oracle 源码。

### 3.4 A 方案判据小结

| 读数 | 50/s | 40/s | flaky/不可判 |
|---|---|---|---|
| 中位 Run 间隔 | 250–350ms | 40–60ms | P90>400ms / 双峰 / 缺失 >20% |
| 稳定段 3s `HealthTick` 增量 | ≈150（±15 边界量化） | 120 | <135 或 >165 |
| `recovery_tick/delta_ms` | ≈1/20 | ≈1/25（每步固定 2） | 不稳定 |

---

## 4. 方案 B：真机 53 帧（需要窗口）

### 4.1 B1（首选真机方案）：静置压血 → 体力点填充间隔

**前置**：
- 单角色；同图无其他玩家/假人（避免 `HumLimit=30ms` 切片）；记录 `Level/Job/MaxHP/MaxMP`。
- 把 HP 压到 **20%–40% MaxHP**（留 ≥20 次填充空间；L255/MaxHP=20669 时每次 +276，20 次 = 5520 ≈ 27%）；MP 压到 < MaxMP。
- 确保 `characters.IncHealth=0 && characters.IncSpell=0`（只读 SQL 复核；池药/血石会污染）；卸下气血石/魔血石（`ItemLocation.Charm`，`CharacterObject.PlaySuperRock` 的输入）；无中毒/buff DoT。
- **测量窗口内零客户端输入**（不走路/不转身/不攻击；`CM_EAT`/`CM_SPELL` 也不发）⇒ 不触发 `RunTick -= 100`。

**抓包路径（仓库既有，不新写）**：
1. `python tools/capture/mir2_proxy.py --out <dir> --map <listen>=<upstream-host>:<upstream-port>`（`chunks.ndjson` 的每行带 `t_wall_ms`/`t_rel_us`，实测 `mir2_proxy.py` 的 `Dump._row`）。监听端口与上游端口必须不同；用影子端口或让 bot 连代理端口。
2. `python tools/capture/segment_frames.py --session <dir> --out <dir>`。
3. `powershell tools/m2-acceptance/export_s2c.ps1 -ProxyDir <dir> -OutDir <out>`（内部 `GoldenExport`）⇒ `capture.jsonl`，每帧带 `ts_ms`（`GoldenExport/Program.cs` 的 `TsMs` 来自 `t_wall_ms`，实测）。
4. 过滤 `dir="s2c" && ident==53`；字段 `param`=HP、`tag`=MP、`series`=MaxHP（`PlayObject.Message.cs:1737` 实测）。
5. HP 填充事件 = `param` 增加 `MaxHP/75+1`（或夹到 MaxHP；**允许同帧 `tag` 也变**——HP/MP 同一 Run 命中时 `SendUpdateMsg` 的 HP 支 53 会被随后的 MP 支 53 替换）；MP 填充事件 = `tag` 增加 `MaxMP/18+1`（同帧 `param` 可变）。

**样本量与判据**：
- 目标 ≥20 个 HP 填充事件（名义 6.0s/次 ⇒ 120s；若 40/s 则 7.5s/次 ⇒ 150s）。取 3 次独立运行。
- `I` = HP 填充间隔的中位数（剔除第一个间隔，避免起测相位）。
- **50/s 判据**：`I ∈ [5.7, 6.4]s`，且 20 个间隔的 95% CI 不含 7.5s；每次 HP 增量 = 276（或最后一次夹到 MaxHP）。
- **40/s 判据**：`I ∈ [7.0, 8.0]s`，且 CI 不含 6.0s。
- **flaky/不可判**：`I` 的 IQR > 0.8s；HP 提前到 MaxHP（填充事件 <15 个）；53 帧缺失/重复（`SendUpdateMsg` 去重，见 §6）；服务器同时有其他玩家/假人；捕获时间戳分辨率不足（`t_wall_ms` 是毫秒，理论够用，但代理排队会加抖动）。
- **为什么选填充间隔而不是逐 Run**：填充每 20 个 Run 才发一次 53（名义 6s），不会被 `SendUpdateMsg` 的同 ident 去重吞掉；且它直接测「体力点累积速率」= 裁决目标。

### 4.2 B2（可选，直接测 Run 周期）：非空池药 + 定期耗蓝 → 每 Run 一条 53

**场景**：
- L≥30（`D≤P`）角色，MP < MaxMP。
- 吃一份大 `IncSpell` 池药（`PlayObject.Base.cs:2079` 的 `IncSpell += stdItem.MAC`），使池量 ≫ `PerSpell`。
- 定期发 `CM_SPELL` 消耗 MP，保持 MP < MaxMP（否则 `PlayObject.Message.cs:133-135` 会把 `IncSpell=0` 终结序列）。
- `CM_EAT`/`CM_SPELL` **不在** `RunTick -= 100` 列表（§1.2 实测）⇒ 不扰动 Run 节拍。

**读数**：每次池消费调 `IncHealthSpell` ⇒ `HealthSpellChanged` ⇒ 53；`tag` 每次 +`PerSpell`（L255 ⇒ +30），`param` 不变。相邻 53 的 `ts_ms` 差 = Run 周期（L≥30 时每 Run 一次）。

**判据**：
- 中位间隔 ∈ [250, 350]ms ⇒ 50/s（名义 300ms）。
- 中位间隔 ≤100ms ⇒ 40/s（50ms 模型）。
- >20% 的间隔 >500ms、或 53 条数明显少于 Run 次数（去重） ⇒ 不可判。
- 剔除前 2 个间隔（相位，见 §1.6）；要求 ≥30 个间隔（≈10s）。

**夹具前置**：需要一个「吃药 + 定期施法」的 BotSrv 序列。BotSrv 不在 oracle 七进程内（T-1）；新增只写本假人 socket 的默认关序列属于 **T-* 工具侧**，不需要 O3。若 owner 认为 B2 的复杂度不值得，B1 已足以裁定 50 vs 40。

### 4.3 B0（补既有 17 文件的缺口）：带时间戳重跑攻击对拍

**目的**：把 `17-attacker-hp-trajectory-csharp-c2.txt` 的「无时间戳」补上，直接测 `CM_HIT` 间隔 Δ，再用「第 86 击掉 1 HP」反推速率。

**做法**（二选一）：
- (a) 用 `run_attack_pair.ps1 -AttackSequence -AttackPinPair` + `mir2_proxy.py` 抓 `c2s` 的 `CM_HIT(3014)` 与 `s2c` 的 `SM_STRUCK(31)`，`ts_ms` 由 `capture.jsonl` 给出；
- (b) 给 `M3ATTSEQ`/`SERVER_STATE` 行加 `ts_ms`（BotSrv/hook 工具侧，T-* 同族，默认只加字段不改行为）。

**判据**：
- 计算 119 个 `CM_HIT` 间隔的 median/IQR；再对 `SERVER_STATE` 的攻方 HP 序列找第一次 `20669→20668` 的击次 k。
- **50/s**：把实测 `(Δ,k)` 对 50/s 仿真曲线（P=300ms + 每次 `RunTick -=100`）对表：Δ≈500ms ⇒ k=86；Δ≈529ms ⇒ k=111。`(529,111)` 是有效的 **50/s 签名**（不能因为 k 不在 80–95 就判不可判）。
- **40/s**：把实测 `(Δ,k)` 对 40/s 仿真曲线（每 50ms `+2`、无 `-100`）对表：**Δ≈633–635ms ⇒ k≈86**（本机仿真 633/634/635 ⇒ 86/86/87，事件序取法不同有 ±1–3）；**Δ≈662ms ⇒ k≈113**（不是 86）。注意：`RunTick -=100` 属于 **P=300 的 oracle 模型**，不属于 Rust 40/s 模型；oracle 模型在 Δ=550ms 时无界 k≈142（落在 120 击窗口之外，窗口内表现为「不掉」），而 Rust 40/s 模型在 Δ=550ms 的 k≈51 仍会掉。⇒ 若实测落在 40/s 曲线的其他组合，必须额外解释 `RunTick -=100` 为何没有把结果推到「不掉」。
- **不可判**：Δ 的 IQR >100ms（bot 主循环抖动/服务器负载）；或实测 `(Δ,k)` 与两条仿真曲线在 Δ 邻域的可达 k 都不吻合（差 >±5 击）。**单看 k 不够**：单个 k 落在 80–95 之外**不自动**等于不可判。
- **重要**：B0 受 `RunTick -=100` 与满血 `HealthTick` 300 清零截断的双重污染，**只能作为旁证**，不能单独裁定；B1 才是主判据。

### 4.4 方案 B 的共同纪律

- 只在 owner 发的窗口内跑；**不得停掉当前占用 9 个游戏端口的外部 C# 栈**（本文件只记录占用：`E:\MirServer\port-status.ps1` 实测 9 个游戏端口 5000/5100/5500/5600/5700/6000/7000/7100/7200 全部 LISTENING）。
- 两侧（C#/Rust）用同一账号、同一角色属性、同一 `Server.conf` 的 300/800；记录部署件 hash。
- 抓包必须在**代理/服务端边界**取时间戳，不要用客户端渲染时刻（客户端 tick 会二次量化）。
- 每次运行前记录 `characters_ablity` 的 HP/MP/Level/IncHealth/IncSpell；运行后记录同样字段。
- 原始件入库（`mir2-rs/tests/parity/m3/evidence-run-cadence-20261018/**`），带 `SHA256SUMS.txt`、`commands.txt`、`README.md`。

---

## 5. 决策矩阵

| 裁定 | 证据 | Rust 要改什么 | 判据/测试 | 卡与验收 |
|---|---|---|---|---|
| **50/s**（oracle 名义 300ms/Run） | B1 的 `I≈6.0s` 或 A1 的中位 250–350ms + 逐 Run `+15`（或 3s 增量 ≈150） | ① `stamina.rs`：`RUN_INTERVAL_MS` 50 → **300**（或改成「每玩家 `run_tick` + 300ms 门」）；`stamina_tick` 的 `+2/50ms` 重放改为 `+15/300ms`；② `main.rs::drain_recovery`：从「每 200ms tick」改为「每 modeled Run（300ms）」调用（否则池药在 L≥40 或 L=20..29 仍快 1.5×；L=30..39 为 0.75×；L≤19 约 1.0×，见 §2.1）；③ 若要把动作消息的 `RunTick -=100` 也建模，需在 `movewire`/`attackwire`/`main.rs` 加每会话 `run_tick` 下推（可选，先登记） | 单测：300ms 边界 ⇒ `+15`；3s ⇒ 150（或 1.5s ⇒ 75；不要用 1s——只含 3/4 个 Run ⇒ 45/60）；`recover` 的 300ms 调用节拍；攻击对拍按实测 Δ 重算；R 级真机 `scoped_diff` | **R**；M3 T-14 `characters_ablity.Hp` 可能从 20669 变为 20668（或反之）；M2 验收不受影响（M2 不含长时静置恢复）；窗口全绿后卡可 `closed` |
| **40/s**（oracle 有效 40/s） | B1 的 `I≈7.5s` 或 A1 的中位 40–60ms | **不改** `stamina.rs`；把模块头「50ms 一次」改成「经时间戳实测的有效 40/s」；`recover.rs` 的节拍差另判（按等级 0.75×/1.0×/1.5×，见 §2.1） | 保留现有 `health_tick_recovers_two_per_fifty_ms`；新增时间戳证据引用；R 级复跑 | **R**；登记 T-*「oracle 有效 40/s 与 50ms 重放等价」；必须先解释门为何被绕过（否则 40/s 不成立）；M2 不受影响；卡可 `closed`（带 T-*） |
| **不可判** | B1 的 IQR >0.8s / A1 的 P90>400ms / B0 的 Δ IQR>100ms | **不改代码**；`stamina.rs` 模块头改为「未裁定，按 40/s 暂存，R 级待窗口」 | 保留现有测试；新增 `window_required: true` 记录 | **R**；登记 T-*「玩家 Run 节拍未裁定」；按 tier policy **R 卡 `status ≠ closed`**，保持 `in-progress` 并写明待窗口项；M2 不受影响；M3 T-14 的 `Hp` 差保留为已知未决 |
| **附带项**（无论 50/40） | `recover.rs` 的调用节拍 | 若裁定为 300ms Run，`drain_recovery` 必须同改；若 40/s 但 Run 仍 300ms，则 `recover` 的误差按等级为 L≥40/L=20..29 的 1.5×、L=30..39 的 0.75×、L≤19 约 1.0× | 池药场景的 53 间隔（B2） | 同上；不与 `stamina` 混为一条判据 |

> **明确禁止**：把 `characters_ablity.Hp` 这一列塞进 `VOLATILE_COLUMNS`、或用「语义等价 / 代码看起来一样 / 与 oracle 同构」盖掉 40 vs 50 的差异。按 `AGENTS.md` tier policy，这是 R 级。

---

## 6. 已知风险与对策

| # | 风险 | 影响 | 对策 |
|---|---|---|---|
| R1 | `.NET PeriodicTimer` 抖动 / 系统时钟粒度（~15.6ms） | Run 间隔不是精确 50/300ms；`recoveryTick` 在相邻 20ms 边界跳变 | 判据用区间而非单点；A1 用同类型 `PeriodicTimer`；B1 用 20 个填充间隔的中位数 |
| R2 | 服务器负载 / `HumLimit=30ms` 切片 | 多玩家时单个玩家不是每 50ms 被访问，Run 周期变长 | B1/B2 单玩家、同图无 AI 消费者；记录 `HumLimit` 与在线人数；A1 无网络 |
| R3 | 动作消息 `RunTick -=100`（`:880`） | 攻击/移动玩家的 Run 周期被下推 100ms/条 | B1/B2 零输入；B0 单独记录 Δ 并把 `-100` 纳入仿真；不把 B0 当唯一判据 |
| R4 | 满血 `HealthTick >= 300 ⇒ 0`（`:74-77`）截断 | 满血时正的体力点累积被清零，掉血阈值不是简单的「速率×时间」 | B1 把 HP 压到 < MaxHP；A1 记录 `health_tick` 每 Run 的前后值；B0 仿真显式建模清零 |
| R5 | `SendUpdateMsg` 同 ident 去重（`BaseObject.Message.cs:283-304`） | 同一 53 在队列里被替换/丢弃，逐 Run 计数会少 | B1 用 6s 间隔的填充事件（不会同 tick 重复）；B2 只作旁证并剔除缺失段；A1 直接读内存，不依赖帧 |
| R6 | 客户端 tick / 渲染耦合 | 客户端侧抓包会把 53 二次量化 | 用 `mir2_proxy.py` 在服务端边界抓；不要用客户端 hook 的时间戳 |
| R7 | 既有 `17-attacker-hp-trajectory-csharp-c2.txt` 无时间戳 | `≈662ms` 是反解，40/s 与 50/s 都能解释第 86 击 | B0 重跑加 `ts_ms`；在拿到时间戳前，**不把 40/s 当已证事实** |
| R8 | 时钟注入语义被误解 | 以为 `MIR2_CLOCK_MS` 能快进/暂停，实际只是 origin 对齐 | §3.1 明文；A2 的虚拟时钟是**新契约**，需 O3 |
| R9 | C# harness 依赖未初始化静态（`CastleMgr`/`ManageNPC`/`Envir`） | `PlayObject.Run` 前 40 余段在未初始化对象上可能抛异常/产生副作用。控制流实测：`PlayObject.Message.cs:19-765` 全段**没有 `return`**；`try` 在 `:28`，`catch` 在 `:733-737`，`base.Run()` 在 `:765` 且在 try/catch **之外** ⇒ 异常被 catch 后继续执行 `:738-763`，**仍会到 `:765 base.Run()`**。真实风险是前 40 余段的副作用/日志/异常风暴，**不是跳过 `base.Run()`** | A1 用非虚调用（`DynamicMethod`+`OpCodes.Call` 或 `ProbeObject.RunBase()`）直达 `BaseObject.Run` 而非 `PlayObject.Run`；`VerifyTick` 推远；先做 spike 验证非虚调用语义 |
| R10 | 部署件与源码不一致 | 真机读数与源码闭式不符 | 记录部署件 hash、`Server.conf:785-786`、进程路径；用 `M2GameSvr` 的实际 DLL 版本对账 |
| R11 | 端口占用 | 当前 9 个游戏端口被外部 C# 栈占用 | 只读记录，不 stop；窗口由 owner 发；A1 不碰端口 |

---

## 7. 待 owner / O3 清单

1. **O3-A2**：是否为 A2 开 oracle 源码面的时钟 + 定时器 + 探针注入（三处：`HUtil32.GetTickCount`、`TimerScheduledService`、`WorldServer.ProcessPlayObjectData`/`BaseObject.Run`）。**当前无对应 T-* 条目（T-2 是 oracle 源改动先例，但不是本项）⇒ 需 owner 裁决 O3**。
2. **O3-B0**：是否允许给 `M3ATTSEQ`/`SERVER_STATE` 行加 `ts_ms`（BotSrv/hook 工具侧；默认只加字段，不改行为）。倾向按 T-* 工具侧登记，不需要 O3；请 owner 确认。
3. **B2 夹具**：是否值得新增「吃药 + 定期施法」的 BotSrv 重复序列（T-* 工具侧，默认关）。若不做，B1 足以裁定 50 vs 40；B2 只用于直接测 Run 周期。
4. **`RunTick -=100` 建模范围**：即使裁定 50/s，是否在本卡内建模动作消息对 Run 周期的下推？建议**不在本卡**（另开线程，因为它会改 `movewire`/`attackwire` 的每消息副作用），本卡只把静止玩家对齐到 300ms。
5. **`recover.rs` 节拍**：若裁定 300ms Run，`drain_recovery` 的调用节拍是否与本卡同批修？建议同批（否则 L≥40/L=20..29 的池药仍快 1.5×；等级条件见 §2.1），但需在卡上写明「两段共用同一 Run 调度」。
6. **R 卡收口**：若证据不足，按 `AGENTS.md` tier policy，`merge_result` 后 `status ≠ closed`，保持 `in-progress` + `window_required: true`。

---

## 8. 本设计件的自检与复现

- 所有 oracle 行号由 `sed -n '<n>p'` 逐条实测；关键行原文见 §1.1–§1.6 的表格。未实测的运行时行为（定时器抖动、`PeriodicTimer` 合并语义、`Environment.TickCount` 粒度）已在 §1.4/§6 标注。
- Rust 侧只引符号/文件路径（`stamina.rs::RUN_INTERVAL_MS`、`stamina_tick`、`recover.rs::recovery_tick`、`main.rs::drain_recovery`/`drain_stamina`/`sim_loop`、`world.rs::TICK_INTERVAL_MS`/`World::tick`、`monai.rs::base_object_run_monster`、`clock.rs::MIR2_CLOCK_MS`/`now_ms`），不写行号。
- 本文件**未**启动任何服务、**未**占用端口、**未**改 `.rs`/`src`/`sql`/冻结面。
- 复现本文件的源码事实（只读）：
  ```sh
  sed -n '7p;31p' src/GameSrv/Word/Threads/UserProcessor.cs
  sed -n '645,647p;697p;867,880p' src/GameSrv/Word/WorldServer.cs
  sed -n '37,42p;44,55p;57,68p;74,81p;105,109p' src/M2Server/Actor/BaseObject.Base.cs
  sed -n '70,76p;123,126p;133,135p;139,141p;765p' src/M2Server/Player/PlayObject.Message.cs
  sed -n '898,899p' src/M2Server/Player/PlayObject.cs
  sed -n '36,39p' src/OpenMir2/HUtil32.cs
  sed -n '1323,1324p' src/Modules/SystemModule/Conf/Model/GameSvrConf.cs
  sed -n '785,786p' /e/MirServer/M2GameSvr/Server.conf
  ```

---

## 附录 A：体力点速率的闭式与敏感度（仿真，**非真机证据**）

### A.1 `R(P)=floor(P/20)/P`

| P (ms) | floor(P/20) | R (/s) |
|---|---|---|
| 50 | 2 | 40.00 |
| 100 | 5 | 50.00 |
| 150 | 7 | 46.67 |
| 200 | 10 | 50.00 |
| 250 | 12 | 48.00 |
| 255 | 12 | 47.06 |
| 259 | 12 | 46.33 |
| 275 | 13 | 47.27 |
| 300 | 15 | 50.00 |
| 350 | 17 | 48.57 |
| 400 | 20 | 50.00 |
| 600 | 30 | 50.00 |

> 结论：**P≥250ms ⇒ R∈[46.3, 50.0]/s**（P=259ms 最低 46.3；P≥400ms 时 ≈47.7–50.0（最小值在 P=419：`floor(419/20)/419 = 20/419`））；40/s 只在 P≈50ms（或「每 50ms 更新 `AutoRecoveryTick`」的模型）出现。这是把 40/s 与 50/s 分开的结构性依据。

### A.2 第 86 击掉 1 HP 的敏感度（构造级仿真）

模型（oracle 结构；Rust 40/s 变体见表）：50ms 调用网格；Run 门 `(now-RunTick)>250` 且 `RunTick=now`；每次 Run `HealthTick += floor(elapsed/20)`；满血时 `HealthTick>=300 ⇒ 0`；每次 `CM_HIT` `HealthTick -=30` 且 `RunTick -=100`（Rust 40/s 变体无 `-100`，改为每 50ms `+2`）；`HealthTick < -300 && HP>1 ⇒ HP-=1, HealthTick+=300`；初始 `HP=MaxHP=20669`，第 1 击在 3000ms。

**事件序（每个 50ms 网格步）**：① 先处理本步到达的 `CM_HIT`（`HealthTick -=30`；oracle 结构另有 `RunTick -=100`）；② 再跑门 `(now-RunTick)>250`（通过则 `RunTick=now`，`recovery=(now-AutoRecoveryTick)/20`、`AutoRecoveryTick=now`、`HealthTick+=recovery`，满血 `HealthTick>=300 ⇒ 0`）；③ **仅当本步通过了门**（Rust 模型则是每个 50ms 步）判 `HealthTick < -300 && HP>1 ⇒ HP-=1, HealthTick+=300`——oracle 的掉血判定在 `BaseObject.Run` 内，不能在两次 Run 之间触发。「先落击/后落门」与 c2s 到达量化会使 k 差 ±1–3。

| 模型（均限定 **120 击窗口内**） | Δ=500ms | Δ=529ms | Δ=550ms | Δ=662ms |
|---|---|---|---|---|
| P=300 + `RunTick-=100`（oracle 结构，50/s 名义） | **k=86** | k=111 | 不掉（无界 k≈142，在窗口外） | 不掉（无界） |
| P=300 无 `-100` | k=88 | 不掉 | 不掉 | 不掉 |
| Rust 40/s（+2/50ms，无 `-100`） | k=41 | k=46 | k=51 | k=113 |

复现脚本（Python 3，可直接运行；`model` 取 `oracle_shift` / `oracle_noshift` / `rust40`）：

```python
def sim(delta, model, hits=120, first=3000, grid=50):
    """返回 120 击窗口内首次掉 1 HP 的击次 k（1-based）；不掉返回 None。"""
    HP, ht, auto, run_tick, k, t = 20669, 0, 0, 0, 0, 0
    t_end = first + (hits - 1) * delta + 2000
    while t <= t_end:
        while k < hits and first + k * delta <= t:      # ① 命中先落
            k += 1
            ht -= 30
            if model == 'oracle_shift':
                run_tick -= 100
        if model == 'rust40':                            # Rust：每 50ms +2
            ht += 2
            if ht >= 300:
                ht = 0
            if ht < -300 and HP > 1:                     # ③ 掉血
                return k
        elif t - run_tick > 250:                         # ② oracle 门
            ht += (t - auto) // 20
            auto = t
            run_tick = t
            if ht >= 300:
                ht = 0
            if ht < -300 and HP > 1:                     # ③ 仅 Run 内判掉血
                return k
        t += grid
    return None
```

> 口径订正：本表 Rust 40/s 的 k 由初版的 40/46/50/110 重跑为 **41/46/51/113**（同一模型；差异来自首击 k 计数与事件序，±1–3）。读法：**k=86 不是 40/s 的独有签名**——oracle 结构（带 `-100`）在 Δ≈500ms 也给出 k=86；Rust 40/s 在 Δ≈633–635ms 也给出 k≈86、在 Δ≈662ms 给出 k≈113（不是 86）。所以既有证据**既不能证明 40/s，也不能证明 50/s**，必须先测 Δ（B0）或直接测恢复率（B1/A1）。本表是构造级仿真，**不是真机证据**。

## 附录 B：A1 探针 NDJSON schema

```json
{"seq":1,"t_ms":123456,"delta_ms":300,"run_tick_before":123156,"recovery_tick":15,"health_tick_before":45,"health_tick_after":60,"spell_tick_before":45,"spell_tick_after":60,"hp_before":15000,"hp_after":15000,"mp_before":15,"mp_after":15,"gated":true}
```

- `t_ms`：`Environment.TickCount`（与 oracle 同源）。
- `delta_ms`：本次 Run 与上一次 Run 的 `t_ms` 差（首条为 0）。
- `recovery_tick`：`delta_ms/20` 的整数部分（A1 由 `health_tick_after-before` 复核）。
- `gated`：本次 50ms 调用是否通过 `RunTick/RunTime` 门（便于统计门通过率）。
- 建议每 50ms 调用都落一行（`gated=false` 也落），便于区分「门没到」与「定时器没跑」。

## 附录 C：B1/B2 的机判脚本骨架（设计，不实现）

```text
输入: capture.jsonl（export_s2c.ps1 产物）
1. 过滤 dir=s2c, ident=53, conn=目标连接
2. HP 填充: param 增加 == MaxHP/75+1 (或夹到 MaxHP；同帧 tag 可变)
3. MP 填充: tag 增加 == MaxMP/18+1（同帧 param 可变）
4. I = 相邻 HP 填充 ts_ms 差（剔除首段）
5. 判据: median(I) ∈ [5700,6400]ms ⇒ 50/s; ∈ [7000,8000]ms ⇒ 40/s; else 不可判
6. B2: 相邻 53（param 不变、tag 每次 +PerSpell）的 ts_ms 差; median ∈ [250,350]ms ⇒ 50/s; ≤100ms ⇒ 40/s
```

---

## 9. 一句话交付口径（给下一片实现者）

先跑 A1（不改源码、不占端口）拿 `delta_ms` 分布；同时用 B1 的 53 填充间隔做真机复核。只有两者一致指向 50/s 或 40/s，才动 `stamina.rs`；若只拿到一个或读数落在 46.3–50/s 与 40/s 之间的模糊带，按「不可判」处理，登记 T-*，保持 R 卡 `in-progress`。
