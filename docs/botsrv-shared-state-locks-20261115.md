# BotSrv 跨 bot 共享态根因修复（卡 `openmir2-botsrv-shared-state-locks`）

> 结论先行：A6 清单的**三个高危项**（`PlayScene.ActorList`、`MShare.MySelf`、
> `MShare.g_FreeActorList`）已按「细粒度锁 / 快照 / per-bot 上下文」修复；**200 与 500 单档
> 新栈、90s 稳态窗口均为 GREEN，20 类 fault 全 0**。修复前 200 档 `recv_packet_null=6072` /
> `recv_packet_index=1`；修复后 200/500 两档 `recv_packet_null=0`/`recv_packet_index=0`/
> `internal_errors=0`。
>
> 三限定：**census 口径**（20 类逐类 + `error_census_complete`）/ **smoke 90s**
> （`HoldSec=90 < 360`，`conclusion_valid=false`）/ **空载**（起栈前 `cargo/rustc/dotnet/zig/msbuild=0`、
> CPU 2.7%/3.8%；全程 `heavy_samples` n 恒 0、names 恒空）。三者同时成立才是本报告的「全绿」。

## 1. A6 审计清单逐项处置

| 状态 | 写者 / 读者 | 并发面 | 处置（本卡） |
| --- | --- | --- | --- |
| `PlayScene.ActorList` | 收包线程（SendMsg/NewActor/DeleteActor/DelActor/ClearActors）+ runThread（BeginScene） | 高 | **已修**：per-scene `_actorSync`；`NewActor`/`ActorDied`/`DeleteActor`/`DelActor`/`ClearActors`/`SetActorDrawLevel` 全锁；run 侧 `BeginScene` 用 `ActorCountLocked` + `TryGetActorAt`（原子 count+下标）；读循环（FindActor/CrashMan/GetCharacter/HeroActor/TMap/探针）改 `SnapshotActors()` 快照，**不持锁跑 actor 逻辑**（保持 per-bot 并行度）。 |
| `MShare.MySelf` | 收包线程（SM_LOGON / ClearActors）+ runThread | 高 | **已修**：`[ThreadStatic]` 属性（per-thread 上下文）+ per-bot `RobotPlayer.MyActor`（权威引用）；`ClientManager.ProcessOnePacket` 按包设置/恢复、`RunBotRound` 按 bot 轮设置/恢复；SM_LOGON 只写本 bot；ClearActors 清本 bot。**不再有跨 bot 全局 `MySelf`**。 |
| `MShare.g_FreeActorList` | BeginScene/DeleteActor/DelActor（加）+ RobotPlayer 清理（删） | 高 | **已修**：进程级初始化（不再每个 bot 构造时 `new List` 重置）+ `MShare.FreeActorSync`；所有 Add/Contains/清理持锁；删除路径先 RemoveAt 成功再登记（保留原语义）。 |
| `ClientManager._clientList` / `_autoList` | runThread + 收包线程 DelClient | 高 | 既有 `_clientSync` + 快照；本卡复用，不改。 |
| 每假人玩法全局（`TargetX/Y`、`PlayerAction`、`ItemArr`、`DropedItemList`、`AutoPlay`、`MapPath`、`AutoStep`、`TargetCret/FocusCret/MagicLockActor/MagicTarget`、`MapTitle`） | 各 bot 的 run/recv | 高 | **登记后续 per-bot 状态化**（不是本轮 NRE/AOORE 的来源）。本卡先移除了每个 bot 构造时对共享列表/`MySelf` 的**破坏性重置**；`ItemArr/UseItems` 属 P2 物品卡。 |
| `MShare` 连接/会话全局（`SelChrAddr/Port`、`ServerConnected`、`SendLogin`、`ConnectionStep`、`SoftClosed`） | 各 bot 的 LoginScene/SelectChrScene | 高 | **登记**；per-session actor 身份守卫（`logon_misdelivered`）保留；本卡不改登录流程。 |
| `MShare` 夹具配置静态（`g_bo*Sequence`、`g_s*`、`g_n*`） | AppService 构造器 | 低 | 只读；不改。 |
| `TMap.Map` / `m_MapBuf` | runThread + 收包线程（SendMsg 地图跳转） | 中 | **登记**；per-bot TMap；本窗口未观测到 NRE；如后续窗口出现，按 ActorList 同法加锁/快照。 |
| `HeroActor` 每假人列表 | runThread | 中 | run-only；不改。 |
| `LoadMetrics` / `SelfReport` | 全部线程 | 低 | 既有 `Sync`；不改。 |

## 2. 实现

- `src/BotSrv/Scenes/Scene/PlayScene.cs`：`_actorSync` + `SnapshotActors()` + `TryGetActorAt()`
  + `ActorCountLocked()` + `AddFreeActor()`；mutators 全锁；读循环快照；`SM_LOGON` 写
  `RobotClient.MyActor`；`ClearActors` 清本 bot。
- `src/BotSrv/MShare.cs`：`MySelf` 改 `[ThreadStatic]` 属性；`g_FreeActorList` 进程级 +
  `FreeActorSync`；`g_MagicList`/`g_DropedItemList`/`g_ChangeFaceReadyList`/`g_SendSayList`/
  `AutoPathList` 字段初始化器进程级。
- `src/BotSrv/Player/RobotPlayer.cs`：新增 per-bot `MyActor`；移除构造器里对共享列表/`MySelf`
  的每 bot 重置（旧代码会把其它 bot 的状态清掉）；释放池清理持 `FreeActorSync`；探针/NPC/
  商店/任务/掉落/组队读循环改 `SnapshotActors()`。
- `src/BotSrv/ClientManager.cs`：`ProcessOnePacket` / `RunBotRound` 设置/恢复 `MShare.MySelf` 的
  per-bot 上下文；`RunBotRound` 的异常护栏保持原语义。
- `src/BotSrv/Objects/HeroActor.cs`、`src/BotSrv/Maps/TMap.cs`：读循环改快照。
- 新增 `src/BotSrv/SharedStateSelfCheck.cs` + `--selftest-shared-state`。

## 3. 牙与变异（删护栏必红）

`BotSrv.exe --selftest-shared-state` → rc=0，6/6 PASS：

| 用例 | 断言 |
| --- | --- |
| `myself-thread-local` | 两线程各设各读 `MShare.MySelf`，读回自己的 actor（11/22），不互相覆盖。 |
| `plain-static-negative-control` | 负对照：plain static 被最后写者覆盖（2/2）⇒ 上面的断言不是恒真。 |
| `production-beginscene-guarded-no-throw` | `SceneIndexRaceHook` 在 count→下标之间缩表，生产 `BeginScene` 不抛且 `SceneIndexRaceSkipped+1`。 |
| `legacy-raw-index-negative-control` | 修复前的裸 `Count→ActorList[i]` 在同一缩表下**必抛 AOORE**。 |
| `concurrent-newactor-delactor-snapshot` | 写线程 20,000 次 NewActor/DelActor + 读线程 Snapshot 并发，无异常。 |
| `actorlist-empty-after-stress` | 压测后列表为空（无泄漏/重复）。 |

**变异重放**（本工作区；文件恢复后 sha 相等；恢复后 5 个自检全绿）：

| 变异 | 结果 |
| --- | --- |
| M1：`[ThreadStatic]` 删掉（`MySelf` 退回 plain static） | `myself-thread-local` **FAIL**（read1=22/read2=22，跨线程覆盖；`FAILED failures=1`） |
| M2：`TryGetActorAt` 退回裸 `ActorList[index]`（删锁+范围检查） | `production-beginscene-guarded-no-throw` **FAIL**（threw=True，skipped+=0；`FAILED failures=1`） |
| 恢复 | `SharedStateSelfCheck.cs`/`PlayScene.cs`/`MShare.cs` 恢复为变异前内容（sha 相等）；`--selftest-shared-state`/`scene-null`/`decode-compat`/`error-census`/`probecoords` 全 rc=0。 |

## 4. 修复后窗口（单档新栈；三限定）

### 4.1 空载证据

- 200：起栈前 `cargo/rustc/dotnet/zig/msbuild=0`、CPU 2.7%；500：0、CPU 3.8%。
- `heavy_samples.ndjson` 23 个样本（14:23:27–14:27:39），**n 恒 0、names 恒空**；
  CPU min/mean/max = 4 / 33.5 / 59%（峰值为被测 BotSrv+栈负载，不是外部构建）。
- 两档 `host_load.heavy_procs_max=0`、`heavy_names_seen=[]`。

### 4.2 200 单档（新栈，`HoldSec=90`）

| verdict | login / no_resp / internal / OOM | world_ok | tick P50/P90/P99 | census 20 类 | scene_null / rounds | host（均/峰/heavy） |
| --- | --- | --- | --- | --- | --- | --- |
| **GREEN** | 100% / 0 / **0** / 0 | 200/200 | 0/16/16ms | **全部 0**（complete=true） | 0/265,725 | 36.7% / 80% / 0 |

### 4.3 500 单档（新栈，`HoldSec=90`）

| verdict | login / no_resp / internal / OOM | world_ok | tick P50/P90/P99 | census 20 类 | scene_null / rounds | host（均/峰/heavy） |
| --- | --- | --- | --- | --- | --- | --- |
| **GREEN** | 100% / 0 / **0** / 0 | **500/500** | 15/31/47ms | **全部 0**（complete=true） | 0/211,992 | 44.6% / 66% / 0 |

### 4.4 修复前后对照

| 指标 | 修复前 | 修复后 |
| --- | --- | --- |
| 200 `recv_packet_null` / `recv_packet_index` | **6072 / 1**（14:00 严格空载 200） | **0 / 0** |
| 500 稳态 `internal_errors` | **758**（criticality N=4 500/500 稳态，同族 BeginScene/ActorList NRE） | **0** |
| 500 tick P99 | 187ms（criticality N=4 稳态，不同窗口/配置，仅参考） | 47ms（本窗口） |
| 200 `scene_null_actor_skipped` | 685/190,194（0.36%） | **0**/265,725 |
| 500 `scene_list_race_skipped` | 1（decode 窗口） | **0** |

## 5. 窗口设计教训：「换档风暴」

严格空载的 200→500 **同栈**窗口（decode 卡 14:00）里，500 的进世界被推迟：
`enter_sent→in_world` min/median/p90/max = 39.4/49.7/69.0/73.0s，而 **criticality 单档新栈**
= 0.25/11.7/23.5/26.9s；game-svr 的 `离世广播` warning **62,750 条**（首条 14:01:33.6，
正好在 200 档结束/500 档开始前）。200 个 bot 同时断开的离世广播风暴占住服务端发送/tick，
把 500 的 `取角色进世界/进世界` 整体推迟 ⇒ `world_ok` 471/500 与 60s 窗口截断，
**与 tick P99 不是一回事**。

**本卡复验按此教训执行**：200 与 500 **各自单档新栈**，`HoldSec=90`；
500 在 ~40s 内 500/500，稳态 ≥50s。以后多档复验：
- 混合栈必须**手工 stop/start**（`load_gate -RestartStackPerTier` 走的是 C# `start-all.ps1`，不适用于 Rust 四缝）；
- 不要用同栈连续 200→500 的 `world_ok`/`in_world`/NRE 数作基线（phase artifact）。

## 6. 未覆盖 / 后续

- **每假人玩法全局仍共享**（`TargetX/Y`、`ItemArr`、`g_MagicList`、`DropedItemList`、
  `TargetCret` 等）：A6 已登记「per-bot 状态化」；本卡只移除每 bot 构造器的破坏性重置，
  **不做 per-bot 重构**（避免与 P2 物品/魔法行为混入）。它们不是本轮 NRE/AOORE 来源。
- **P2 物品流程**：`MShare.UseItems = new ClientItem[14]` 全 null，203/202 消费点解引用
  `.Item.Name`；本卡未触发（假人无物品交互），**仍待 P2 卡**。
- **Q2 段切分**：500 的 tick 47ms 是本窗口的顺带观测；`HostInbound`/`World`/`HostPeriodic`
  精确切分另开 profiler 窗口，不在本卡。
- **90s 仍是 smoke**：`conclusion_valid=false`；360s 结论窗口另排（root 卡/负载门禁）。

## 7. 复现

```sh
# 牙
src/BotSrv/bin/Release/BotSrv.exe --selftest-shared-state
# 200 单档（新栈）
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 \
  -RustBinDir E:/UserTemp/stack-rel-target-w83/release -LogDir <...>/200/stack-logs
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 \
  -Tiers 200 -HoldSec 90 -SkipBuild -MemKillThresholdGB 8 -BotHeapLimitGB 4 -OutDir <...>/200
# 500 单档：先 -Stop 200 栈，再以新的 -LogDir 起栈，-Tiers 500 -HoldSec 90
```
