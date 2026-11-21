# 生产 actor tick 节拍（2026-11-21，只读核查）

> **三行结论**
> ① **生产 actor 拍 ≈250–300ms**（玩家：`UserProcessor` 50ms 周期 + `RunTime=250` 门；怪物：`ProcessMonsters` 每趟 `Thread.Sleep(20)` + 250 门）；**NPC ≈400ms**（200ms 周期 + 250 门）；**假人 50/100ms**（`RobotManage` 50ms、`RobotProcessor` 100ms）。
> ② **与"100ms 步进"假设【不一致】**——actor 侧在源码里没有任何 100ms 出处。
> ③ **`Random(1500)` 初始相位**：`BaseObject` ctor `RunTick = GetTickCount() + Random(1500)` ⇒ **怪物首跑时刻本身是随机的**（`PlayObject` ctor 覆盖为当前 tick，玩家相位确定）。

> **100ms 出处的唯一候选（重要）**：全仓 `FromMilliseconds(100)` 只有 `src/Modules/RobotSystem/RobotProcessor.cs` 的**假人模块**；魔法装置的 `StepDrainMs=100` 是**装置自己的约定**。
> 两者都不是 actor 生产节拍 ⇒ 疑似"**一个装置/假人约定被当成生产事实**"的活例。
>
> 本卡登入装置盲区总表为 **B23**（`docs/issues/golden-harness-blind-spots.md`）：**"对拍的时间节拍是假设值"**。
> 处置口径（opc-19 2026-11-21）：harness/Rust 是否改按生产节拍建模**由总控裁定**；**裁定前 B 型两支的所有改动暂停**。
>
> 只读核查：未改代码、未构建、未 checkout。锚点一律"文件 + 符号"，不写行号。


> 结论先行：**生产 actor `Run()` 不是 100ms 一次。**

## 0. 裁决

**生产的 actor Run 间隔 = 玩家 ≈300ms 名义（`RunTime=250ms` 门 + `UserProcessor` 50ms 周期，严格大于 ⇒ 首次落在 300ms）；怪物 ≈250–300ms（`RunTime=250ms` 门 + 每趟 `Thread.Sleep(20)` 循环）；NPC 商人/任务 NPC ≈400ms（200ms 周期 + 250ms 门）。名义常量、负载下可变；怪物首跑相位 0–1500ms 随机。与 100ms 假设【不一致】。**

## 1. 谁在调用 `Run()`（调用链，符号锚）

- 基础泵：`src/M2Server/Actor/BaseObject.Base.cs::Run()` = `while (GetMessage(ref pm)) Operate(pm)`；之后做回血/回蓝等 tick 逻辑。
- **玩家**：`src/GameSrv/Word/Threads/UserProcessor.cs::ExecuteInternal`（`TimerScheduledService`，`PeriodicTimer` 50ms）
  → `src/GameSrv/Word/WorldServer.cs::ProcessHumans`
  → `WorldServer.cs::ProcessPlayObjectData`（门：`(dwCurTick - playObject.RunTick) > playObject.RunTime`）
  → `src/M2Server/Player/PlayObject.Message.cs::Run()`（override，尾部 `base.Run()`）。
- **怪物**：`src/GameSrv/Word/WorldServer.MonGen.cs::InitMonsterThreads` 起 `MobThreading[i] = new Thread(() => ProcessMonsters(mobThread))`
  → `MonGen.cs::ProcessMonsters`（`finally Thread.Sleep(20)`；门：`(currentTick - monster.RunTick) > monster.RunTime`）
  → `src/M2Server/Monster/MonsterObject.cs::Run()`（override，尾部 `base.Run()`）。
- **商人 NPC**：`src/GameSrv/Word/Threads/MerchantProcessor.cs`（200ms）→ `WorldServer.Merchant.cs::ProcessMerchants`（门同上，`RunTime`）→ `Merchant.Run()`。
- **任务 NPC**：`MerchantProcessor` → `WorldServer.Merchant.cs::ProcessNpcs` → `NormNpc.Run()`（门同上）。
- **脚本机器人（Robot.txt）**：`SystemProcessor`（50ms）→ `M2Share.AutoBot.Run()`（`src/GameSrv/Robots/RobotManage.cs::Run`）→ `RobotObject.Run()`（继承 `PlayObject.Run`，**没有 250ms 门**）⇒ 机器人每 50ms 一次。
- **登录前的特例**：`src/M2Server/Player/PlayObject.cs::Run` 的 `WaitLoginNoticeOk` 分支会自己 `while (GetMessage(ref msg))`，但只等 `CM_LOGINNOTICEOK`，之后回正常路径（不是常驻泵）。
- 客户端消息**不进另一条泵**：`src/M2Server/Net/ChannelMessageHandler.cs` 只把 `CM_*` 转成 `SendMsg`/`SendUpdateMsg`/`SendActionMsg`（**入队**）；真正执行仍在上述 `Run()`。

## 2. 具体数值与推导

| actor | 门（`RunTime`） | 驱动周期 | 名义 `Run()` 间隔 | 依据 |
| --- | ---: | ---: | ---: | --- |
| 玩家 | **250ms** | 50ms（`PeriodicTimer`） | **≈300ms**（250 不满足严格 `>`，下一拍 300） | `PlayObject` ctor `RunTime = 250`；`BaseObject.Base.cs::Run` 无关；`ProcessPlayObjectData` 门为 `>` |
| 怪物 | **250ms** | 每趟 `Thread.Sleep(20)` + `MonLimit` 30ms 预算 | **≈260–300ms**（pass 时长 + 20ms + 门粒度） | `MonsterObject` ctor `RunTime = 250`；`ProcessMonsters` 尾部 `Thread.Sleep(20)` |
| 商人/任务 NPC | **250ms** | 200ms | **≈400ms** | `MerchantProcessor` 200ms + `ProcessMerchants/ProcessNpcs` 的 250ms 门 |
| 怪物子类 | 250ms（同） | 同上 | 同怪物 | 全仓怪物子类的 `RunTime = 250`（见 §3） |

- 门是**严格大于**（代码 `> RunTime`），不是 ≥ ⇒ 50ms 粒度下第一次必然落在 300ms（50..250 的拍点都不过门）。
- **初始相位**：`BaseObject` ctor 写 `RunTick = GetTickCount() + Random(1500)` ⇒ 怪物/基类对象的**第一次 Run 有 0–1500ms 随机相位**；`PlayObject` ctor 覆盖为 `RunTick = GetTickCount()` ⇒ 玩家相位确定（自构造起约 300ms）。⇒ 怪物节奏不是全局对齐的，**奇偶会随实例随机**。
- `TimerScheduledService` 用 `PeriodicTimer.WaitForNextTickAsync`（`src/OpenMir2/TimerScheduledService.cs`）：一次 `ExecuteInternal` 超过周期时**不补拍**，实际间隔 ≥ max(周期, 单次耗时) ⇒ 负载下只会更慢。
- 负载下额外的"分批"：玩家循环有 `M2Share.HumLimit`（缺省 30ms）预算，超时把剩余玩家留到下一拍；怪物循环有 `M2Share.MonLimit`（缺省 30ms）预算 + `ProcessMonsterInterval`（缺省 2，非活跃怪隔趟跑）。⇒ **分布右侧可以明显大于 300ms**。

## 3. 常量还是可配置

- `RunTime = 250`：**硬编码字面值**，出现在 `BaseObject.cs` ctor、`PlayObject.cs` ctor、`MonsterObject.cs` ctor，以及怪物子类（`BeeQueen`/`ExplosionSpider`/`MagicMonster`/`PercentMonster`/`SpiderHouseMonster`/`StickMonster`/`DualAxeMonster`）——**不是配置项**，也没有读到 `Server.conf` 的路径。
- 循环周期：**硬编码 `TimeSpan` 字面值**（`UserProcessor` 50ms、`MerchantProcessor` 200ms、`EventProcessor` 20ms、`ActorBuffProcessor` 50ms、`CharacterDataProcessor` 500ms、`TimedRobotProcessor` 20ms、`GeneratorProcessor` 10s、`RobotProcessor` 100ms）。
- 预算/限值：`HumLimit` / `MonLimit` 缺省 30（`M2Share.cs` / `SystemShare.cs` 静态初值 + `GameApp` 赋值），可由 `src/Modules/SystemModule/Conf/ServerConf.cs` 读 `[Server] HumLimit/MonLimit` ⇒ **可配置**。
- 结论：**名义常量（250ms 门 + 50/20ms 循环），实际可变（负载/预算/定时器漂移）**；**没有 100ms 的 actor 常量**。

## 4. 有多少个驱动循环（全列）

| 循环/服务 | 周期 | 驱动内容 | 备注 |
| --- | ---: | --- | --- |
| `UserProcessor` | 50ms | `ProcessHumans` → 玩家 `Run()` | 玩家主循环 |
| 怪物线程 `MobThreading`（`ProcessMonsters`） | 每趟 `Sleep(20)` | 怪物 `Run()` | 线程数 = `ProcessMonsterMultiThreadLimit`（缺省 1） |
| `MerchantProcessor` | 200ms | `ProcessNpcs` / `ProcessMerchants` | NPC 200ms + 250ms 门 ⇒ 400ms |
| `SystemProcessor` | 50ms | `WorldEngine.Run()`（missions 1s / doors 500ms）、NetChannel/Auth/AutoBot/Planes | **不驱动 actor Run** |
| `ActorBuffProcessor` | 50ms | `ActorBuffSystem.DoWork` | Buff 结算 |
| `EventProcessor` | 20ms | 地图事件 `MapEvent.Run()` | 按事件自己的 `RunTick` |
| `CharacterDataProcessor` | 500ms | 存/取角色数据 | 非 actor |
| `TimedRobotProcessor` | 20ms | 机器人（循环体**已注释**） | 当前空转 |
| `GeneratorProcessor` | 10s | ActorId 队列生成 | 非 actor |
| `RobotProcessor`（`Modules/RobotSystem`） | **100ms** | 假人（机器人）列表 | 属模块/假人，不是 PlayObject/Monster 的生产路径 |
| `RobotManage`（`GameSrv/Robots`，由 `SystemProcessor` 50ms 驱动） | 50ms | `Robot.txt` 脚本机器人 `RobotObject.Run()` | **PlayObject 子类且无 250ms 门** ⇒ 机器人 50ms 一拍；与玩家不同频 |
| `Map.StartMakeStoneThread`（`src/GameSrv/Maps/Maps.cs::MakeStoneMines`） | 一次性 | 地图矿物初始化 | 非循环 |
| `BotSrv`（独立进程） | 自有 | `BotSrv` 的对象 `Run()` | 独立进程，未在 GameSrv 路径内 |

⇒ **有多个不同频率的驱动**：玩家 50ms 拍、怪物 20ms 睡眠拍、NPC 200ms 拍；它们各自的 actor `Run()` 又被 250ms 门二次限速。**"奇偶"结论取决于这张表，不取决于单一 tick。**

## 5. `IWorldEngine` 实现在哪、`M2Server` 有没有自己的 tick

- 接口：`src/Modules/SystemModule/SubSystem/IWorldEngine.cs`。
- 实现：`src/GameSrv/Word/WorldServer.cs` 的 `public partial class WorldServer : IWorldEngine`（分文件 `WorldServer.MonGen.cs` / `WorldServer.Merchant.cs` / …）。
- 驱动：`src/GameSrv/Word/Threads/SystemProcessor.cs::ExecuteInternal`（50ms）里 `SystemShare.WorldEngine.Run()`；`WorldServer::Run()` 只做 missions（内部 1s 门）与 map doors（500ms 门），**不泵 actor 消息**。
- `M2Server` 项目内**没有**自己的 actor tick：`ChannelMessageHandler` 只入队；`BaseObject`/`PlayObject`/`MonsterObject` 的 `Run()` 都由 GameSrv 的上述循环调用。

## 6. 我是怎么确定的（质证口径）

1. **字面值**：`RunTime = 250` 是 ctor 里的硬编码赋值（`PlayObject` / `MonsterObject` / `BaseObject` 及怪物子类）；循环周期是 `TimerScheduledService` 构造参数的字面 `TimeSpan`（50/200/20/500/20/10000/100ms）；怪物循环尾部是字面 `Thread.Sleep(20)`。
2. **门条件**：`ProcessPlayObjectData` / `ProcessMonsters` / `ProcessMerchants` 都是 `(now - RunTick) > RunTime`（严格大于），`RunTick` 在调用前赋当前拍时间 ⇒ 250ms 门 + 50ms 拍 = 300ms 节奏。
3. **初始相位**：`BaseObject` ctor 的 `RunTick = GetTickCount() + Random(1500)` 是显式随机相位（`PlayObject` ctor 覆盖为当前 tick）；这不是配置，来源是 ctor 字面式。
4. **没有配置覆盖**：`RunTime` 未见读取配置文件；`HumLimit`/`MonLimit` 才是配置项（预算，不是节拍）。
5. **100ms 的出处**：全仓 `FromMilliseconds(100)` 只有 `RobotProcessor`（假人模块）；魔法装置的 `StepDrainMs=100` 是**装置自己的约定**，不是生产实现。⇒ "100ms actor tick" 在源码里**没有出处**。

## 7. 对"同一时刻到期两条消息谁先处理"的影响（一句话）

到 t=800ms 为止，玩家 actor 的 `Run()` 只发生 **≈2–3 次**（≈300/600/900ms），不是 8 次（100ms 假设）；怪物同量级。**装置与 Rust 共同假设的 100ms 步进与生产节拍不一致**，这足以改变"未到期根重入"的次数与"哪条已到点消息被压到下一步"——很可能就是该分歧的根因。

## 8. 未定位 / 边界

- `BotSrv` 独立进程的 actor 节奏未核（本卡只覆盖 GameSrv 生产路径；若对拍涉及假人，需另核 `RobotProcessor` 100ms 的实际效果）。
- 负载下的实际间隔分布未实测（只读静态推断；`HumLimit/MonLimit` 预算与 `PeriodicTimer` 不补拍会让右侧变长）。
- 若未来参数化 `RunTime`，本卡结论需重核；当前全仓没有这种配置项。

## 9. 迭代序：tick 内各 actor 按什么顺序被 `Run()`

### 9.1 容器与顺序（符号锚）

| actor 类 | 容器（符号锚） | 顺序 | 跨趟续位 |
| --- | --- | --- | --- |
| 玩家 | `src/GameSrv/Word/WorldServer.cs` 的 `PlayObjectList`（`IList<IPlayerActor>`，ctor 里 `new List<IPlayerActor>()`） | **List 插入序 = 登录完成序**（`ProcessHumans` 内 `PlayObjectList.Add`），**不是 ActorId**、不排序（`WorldServer` 内无 `Sort`） | `ProcessPlayObjectData` 用 `ProcHumIdx` 续位 + `HumLimit`（缺省 30ms）预算：一趟可能只覆盖前 K 个，下一趟从 K 继续；整趟走完归 0 |
| 怪物（按线程分组） | `WorldServer.MonGen.cs` 的 `MonGenInfoThreadMap`（`Dictionary<int, IList<MonGenInfo>>`） | 线程 id 升序（键是 0..N-1 的稠密整数）；**每线程内 `MonGenInfo` 的顺序 = 加载/按 `MonName` 分组后的插入序** | `MonsterThread.MonGenListPosition` 续位 + `MonLimit`（缺省 30ms）预算 |
| 怪物（单只） | `MonGenInfo.CertList`（`IList<IMonsterActor>`，`MonGenInfo.TryAdd` 里 `CertList.Add`） | **刷怪/收养的追加序**（`RegenMonsters`/`RegenMonsterByName` 的创建序） | `MonsterThread.MonGenCertListPosition` 续位 |
| NPC / 商人 | `WorldServer.cs` 的 `MerchantList` / `QuestNpcList`（`new List<...>()`） | List 插入序（加载序） | `MerchantPosition` / `NpcPosition` 续位 + `NpcLimit` 预算 |
| 脚本机器人 | `src/GameSrv/Robots/RobotManage.cs` 的 `RobotHumanList`（`List<RobotObject>`） | `Robot.txt` 加载序 | 无门，`SystemProcessor` 50ms 全量 |
| 地图事件 | `EventMgr.Events`（`IList<MapEvent>`） | 插入序 | 按事件自身 `RunTick` |

### 9.2 稳定性与并发（关键）

- **同线程内**：顺序由 List 下标决定（插入序），基本稳定；但**遍历中会改容器**——玩家 ghost 走 `PlayObjectList.Remove`（后续元素前移，本趟 `continue` 重读同一索引）、怪物 ghost 走 `CertList.RemoveAt`；新登录/新刷怪**追加到表尾**。
- **跨类无全局序**：玩家（`UserProcessor` 的 `PeriodicTimer` 线程）、每个怪物线程（`MobThreading` 各自独立 `Thread`）、NPC（`MerchantProcessor`）、机器人（`SystemProcessor`）是**不同 OS 线程** ⇒ **玩家与怪物谁先 `Run()` 不由任何容器决定，由调度器决定**，不可当判据。
- **怪物→线程的分配本身是随机的**：`WorldServer.MonGen.cs` 用 `M2Share.RandomNumber.Random(ProcessMonsterMultiThreadLimit)` 把 `MonName` 分到线程；`OpenMir2/RandomNumber.cs` 的底层是 `new Random()`（**时间种子**）⇒ 多线程时**跨怪顺序每次启动都不同**。缺省 `ProcessMonsterMultiThreadLimit=1` ⇒ 全部落线程 0，顺序退化为单条 List 序。
- **非活跃怪隔趟**：`WorldServer.MonGen.cs` 的 `ProcessRunCount < ProcessMonsterInterval`（缺省 2）分支 ⇒ 未被玩家激活的怪不是每趟都 `Run()`。

### 9.3 一句话结论

**`tick 内同线程 actor 的处理顺序 = 各自 List 的插入序`（玩家 = 登录完成序、怪物 = 刷怪追加序、NPC = 加载序）；同一个 tick 的玩家与怪物分属不同线程 ⇒ 跨类无全局序；怪物→线程分配又是时间种子随机。**

### 9.4 对装置/驱动的含义（第四条未核实假设）

装置（`MagicGolden` 的 `byWho`/`DrainOnce`）与 Rust 驱动都**显式按固定顺序**遍历少数几个 actor（`caster`→`target`→`obs_in`→`obs_out`，或多趟固定轮转）。生产不是这样：

1. 顺序不是 ActorId，而是**容器的插入/追加序**；
2. 玩家/怪物/NPC/机器人**跨线程并发**；
3. 怪物线程分配**启动随机**（多线程时）；
4. 预算（`HumLimit`/`MonLimit` 缺省 30ms、`NpcLimit` 缺省 5ms）会把一趟**截断到 K 个**，下一趟续位——时序随负载；
5. 非活跃怪**隔趟**（`ProcessMonsterInterval`）；
6. 遍历中 `Remove` 前移，顺序可变。

⇒ 凡"跨 actor 消息在同一 tick/下一 tick 落到谁"的判据，都隐含"装置迭代序 = 生产迭代序"这一**未核实假设**（本卡 §9 为出处）。

---

**裁决（单行）**：`生产的 actor Run 间隔 = 玩家 ≈300ms / 怪物 ≈250–300ms / NPC ≈400ms（名义常量、负载可变；怪物首跑相位 0–1500ms 随机），与 100ms 假设 不一致。`
