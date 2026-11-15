# BotSrv actor 自由池泄漏：修复与验证（卡 `openmir2-botsrv-actor-pool-leak`）

身份：`openmir2-worker-85`；分支 `card/botsrv-actor-pool-leak`（base = master `262a95855`）。
分析来源：`docs/botsrv-memory-leak-20261115.md`（worker-82）。本卡只改 BotSrv 夹具，不改服务端、不改冻结客户端。

## 0. 结论与取舍（移除 vs 复用）

**修法：删除自由池，驱逐点立即解除引用。** 不接线定时器、不做对象复用。

依据（oracle 证据，不是偏好）：

- 冻结客户端的 `TPlayScene.NewActor`（`PlayScn.pas:3255` 起）按 `RACEfeature` **恒 `Create`**，
  从不读 `g_FreeActorList`；全文件对 `g_FreeActorList` 只有「Add（3 处）/ ClearActors 释放 / MinTimerTimer 60s 释放」。
  ⇒ 这个池在 oracle 里也**不是复用池**，而是「延迟 60s `Free`」的缓冲。
- 端口把「延迟释放」丢了（`MinTimerTimer` 从未被任何定时器订阅），却保留了「入池」——
  于是池成为只增不减的强引用集合，这正是 `docs/botsrv-memory-leak-20261115.md` 观测到的
  Gen2-after-GC 存活量单调上升（68s 369→1626MB）。
- 托管运行时不需要「延迟释放」：最后一个强引用消失后 GC 自会回收；保留一个 N>0 的池只会额外占用
  N×ActorSize 的内存、并保留「忘了排空」的失效模式。**上界取 N=0 是唯一有依据的 N**。

因此对 `docs/botsrv-memory-leak-20261115.md` §7 建议 1 的更正：**不要**「`NewActor` 优先从池取」——
oracle 的 `NewActor` 不这么做；也不建议补一条有界池，因为它的唯一效果是延迟释放。

对 AOI 驱逐路径的影响：**没有行为变化**。池在端口里从无读取方，删除它只改变「actor 何时可被回收」，
不改变 actor 的创建/删除/消息处理顺序；驱逐仍由 `PlayScene.BeginScene` 的同一分支完成。

## 1. 改动

| 文件 | 改动 |
| --- | --- |
| `src/BotSrv/Scenes/Scene/PlayScene.cs` | 删除 `AddFreeActor`；三个出口（AOI 驱逐 / `DeleteActor` / `DelActor`）改为 `ActorList.Remove` + `LoadMetrics.SceneActorEjected()`；AOI 驱逐补 2000ms 空闲守卫（见 §2）；`ClearActors` 注释说明与 oracle 的等价性 |
| `src/BotSrv/Objects/TActor.cs` | 新增 `m_dwLastGetMessageTime`：构造时置当前 tick（`Actor.pas:1695`），`ProcMsg` 每取走一条消息刷新（`Actor.pas:2055`） |
| `src/BotSrv/MShare.cs` | 删除 `g_FreeActorList` / `FreeActorSync` |
| `src/BotSrv/Player/RobotPlayer.cs` | 删除死代码 `MinTimerTimer`（组队标志刷新 + 池 TTL 清理）；`m_boGrouped` 没有任何读取方 |
| `src/BotSrv/LoadMetrics.cs` | 新增只作观测的 `scene_actor_ejected`（三个出口共用），用于区分「驱逐发生过」与「驱逐后无保留」 |
| `src/BotSrv/SceneGuardSelfCheck.cs` | 新增 ⑩–⑫ 用例：2000ms 守卫、`ProcMsg` 时间戳刷新、A7 机制证明；既有 ④/④c/⑥a/⑥b 的 actor 拨老以满足守卫 |
| `src/BotSrv/SharedStateSelfCheck.cs` | M3c：`processonepacket-context-per-bot` 追加「两次调用后 `MShare.MySelf == null`」断言 |

保留：`Actor.m_dwDeleteTime` 与三处赋值未动（仍对齐 oracle；当前没有读取方，但属于既有契约，不在本卡范围内删除）。

## 2. 2000ms 空闲守卫（对齐 `PlayScn.pas:1219`）

oracle：

```text
if (GetTickCount - Actor.m_dwLastGetMessageTime > 2000) and
   ((ABS(Actor.m_nCurrX - g_MySelf.m_nCurrX) > 16) or (ABS(Actor.m_nCurrY - g_MySelf.m_nCurrY) > 16)) then
  Actor.m_boDelActor := True;
```

端口原先只有「越界 >16 格」就驱逐。现在：

```text
actor.DelActor || (aoiIdleExpired && (|dx| > 16 || |dy| > 16))
aoiIdleExpired = MShare.GetTickCount() - actor.m_dwLastGetMessageTime > 2000
```

`m_dwLastGetMessageTime` 的刷新点与 oracle 一致：**只在 `ProcMsg` 真正取走一条消息时**刷新
（`ProcHurryMsg` 不刷新，与 `Actor.pas` 的 `ProcHurryMsg` 一致）。

## 3. 验收（A7/A8）

### A7 池大小有界

- **上界 N=0**：池结构已删除，代码里不存在持有 `Actor` 的静态集合（`src/BotSrv` 只剩 `THumActor.SlaveObject`，
  它是 actor 自身的子对象表，随 actor 一起回收）。
- **机制证明**（不是「内存没涨」）：`SceneGuardSelfCheck` 的 ⑫ 构造 16 个「越界且空闲」actor 交给
  `BeginScene` 驱逐，只保留 `WeakReference`；`GC.Collect()×2` 后断言 `aliveAfterGc == 0`。
  在驱逐路径重新加入任意静态保留（旧池写法）会令该断言变红。
- 运行时观测：`load_stats.ndjson` 新增 `scene_actor_ejected`；它不是判据，只用来确认「驱逐确实发生」
  （若为 0，则说明 2000ms 守卫把驱逐压到接近零，Gen2 平台不能归因于修好了池）。

### A8 Gen2-after-GC 重测

- 口径：`dotnet-counters collect` 采 `System.Runtime` 的 `gen-2-size` / `gen-2-gc-count` / `gc-heap-size`，
  取 `gen-2-gc-count` 递增采样点的 `gen-2-size` 作为「本次完整 GC 后存活量」。
- 结论字段：稳态段（进世界平台之后）的 Gen2-after-GC 斜率（MB/s）与每 in-world bot 的 Gen2 存活量。
- 旧「8.0–9.2MB/bot」口径作废：它把时间项摊进了常数。

**实测（待窗口回填）**：

| 档 | 提交 | HoldSec | in-world | Gen2-after-GC 起→止 | 斜率 | 每 bot Gen2 | WS 峰 | OOM |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 500（修前对照，归档 `botsrv-inworld-plateau-20261115-094127`） | 修前 | ~87s | 283 | 352→1551MB（101s） | **+15.7MB/s**（r²=0.988） | 5.5MB/bot 且仍在涨 | 3472MB | 无（4GB cap 那轮才有） |
| 500（修后） | TBD | 90 | TBD | TBD | TBD | TBD | TBD | TBD |

修前对照用同一个分析脚本重算（`Gen 2 Size` 在 `Gen 2 GC Count > 0` 的采样点），确认该口径**能**看见泄漏；
修后表同理，斜率接近 0 才有意义。

## 4. 变异（逐条真跑，恢复后核对 sha）

| 变异 | 期望 |
| --- | --- |
| 在 AOI 驱逐路径重新加入静态保留（`static List<Actor>` + `Add`） | ⑫ `aoi-evict-not-retained-after-gc` 红（`aliveAfterGc > 0`） |
| 删除 AOI 的 2000ms 空闲守卫（还原为只看距离） | ⑩ `aoi-idle-guard-skips-recent` 红（近消息越界 actor 被误删） |
| 删除 `ProcMsg` 里的 `m_dwLastGetMessageTime` 刷新 | ⑪ `procmessage-refreshes-lastget` 红（`freshAge` 仍 >2000） |
| 删除 `ClientManager.ProcessOnePacket` 的 `finally { MShare.MySelf = prev; }` | M3c `processonepacket-context-per-bot` 红（`after=leaked`） |

## 5. 残留与未验证

- 未改动 `_reviceQueue` 的无界通道（分析报告的次要候选；本卡范围外，登记为后续风险）。
- `Actor` 单对象尺寸仍未用 gcdump 测；A8 的「每 bot Gen2」来自 counters 的存活量，不是对象计数。
- 真机档位只跑 500 单档（总监指定）；200/1000 未重测。
