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
- **控制量**：`load_stats.ndjson` 的 `scene_actor_ejected`（累计值与 Δ/Δuptime）用来证明「池不涨」不是
  「驱逐没发生」。实测修复树 500/90 轮：累计 **223 ≈ 2.12 次/s**（500 bot ⇒ 每 bot 0.004/s）；
  按单 actor 1.5–3KB 估，池即使保留也只能以 ~6KB/s 增长 ⇒ 90s 内 ~0.3–0.7MB，不可判。
  因此「内存平」既可能是修好了、也可能是效应量太小；必须与 A8 的 Gen2 数据一起读。

### A8 Gen2-after-GC 重测

- 口径：`dotnet-counters collect` 采 `EventCounters\System.Runtime` 的 `Gen 2 Size (B)` /
  `Gen 2 GC Count` / `GC Heap Size (MB)`；取 `Gen 2 GC Count > 0` 的采样点的 `Gen 2 Size`
  作为「本次完整 GC 后存活量」。**没有稳态 after-GC 点就不算斜率，也不拿堆大小替代。**
- **有效栈 = Rust `game-svr`**（`tools/m2-acceptance/start_rust_stack.ps1`：C# DBSrv/LoginSrv +
  Rust game-svr + Rust 三网关）。C# `GameSvr.exe` **不能**作为 500 bot 的窗口栈：
  `WorldServer.ProcessHumans` 的玩家数据加载是「待实现」，实测 500 bot 只进 21/500，
  且同窗口刷 487 条 `List.RemoveAt` 异常（卡外栈问题，已上报总监另开卡）。
- 旧「8.0–9.2MB/bot」口径作废：它把时间项摊进了常数。

**实测**：

| 轮次 | 栈 | 提交 | HoldSec | in-world | BotSrv WS | Gen2 GC 次数 / after-GC 序列 | scene_actor_ejected | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 修前 master 360s（归档 `load-360-20261115-now`） | Rust | master | 360 | 500/500 | 961.6→883.5，峰 1015.4，尾 **−0.218MB/s**（358s） | 未采 counters | 旧码无此计数 | 内存平坦；无 Gen2 数据 |
| 修复树 90s（`rust-fix90`） | Rust | `5910ac42d` | 90 | **500/500**（60s 闸门 PASS） | 956→914，峰 1014，尾 **+0.31MB/s** | **3 次**：14s 77MB / 21s 130MB / 35s 243MB（均在爬坡段） | **223 ≈ 2.12/s** | 90s 无稳态 after-GC 点 |
| master 90s（`rust-master90b`） | Rust | master `0627a0f36` | 90 | **500/500**（60s 闸门 PASS） | 874→912，峰 1036，尾 **−0.22MB/s** | **2 次**：16s 117MB / 27s 189MB（均在爬坡段） | n/a（旧码无此计数） | 同栈同口径对照；preflight CPU 偏离见 §3.1 |
| 修复树 360s（`rust-fix360b`） | Rust | `24a608c54` | 360 | **500/500**（60s 闸门 PASS） | 896→987，峰 **1609**（22s 进世界瞬态），min 872；报告尾段 **+0.946MB/s**（59.6s） | **3 次**：17s 150MB / 31s 231MB / 35s 261MB（均在爬坡段；**无稳态 after-GC 点**） | **4435（11.97/s；末 120s 18.71/s）** | `recv_packet_null=1`（首轮 4）；verdict RED（internal_errors=1） |

口径说明：

- 90s 轮只有爬坡段 Gen2 GC（in_world 0→500），**不得**用它算斜率；若 360s 仍无稳态 after-GC 点，
  报告写「**本轮未观测到稳态 after-GC 点**」，不拿堆大小替代。
- 修前 master 在 Rust 栈上 360s/500 in-world 的 **BotSrv** 内存已平坦/略降（961.6→883.5，−0.218MB/s），
  说明**当前健康栈不复现历史 ~10MB/s 的 BotSrv 内存表现**；`game-svr` 不在旧 `mem.ndjson` 采样集里，
  不对服务端内存下任何结论。
- 采样口径不对称（显式标注）：90s 两轮的 `mem.ndjson` 只含 C# 进程 + BotSrv（旧 `load_gate`）；
  修复树 360s 起额外采样 `game-svr`/`game-gate`/`login-gate`/`sel-gate`（**只落盘、不参与越界判定**，
  commit `24a608c54`），报告里以 `mem_proc_names_extra` / `mem_proc_names_extra_present` 标注。

**3.1 CPU 采样方法学与 90s 对照的偏离（原样记录）**

- **方法学更正（2026-11-15 20:34）**：`Get-CimInstance Win32_Processor` 的连续采样**会自污染**——
  8 采样循环里 top consumer 就是发起采样的 `powershell`（单次归属快照 `powershell:82%`），所以 90s 两轮
  记录的 `cpu_max=38% / 62%` 是**测量工具产物**，不是环境负载。改用低开销 `typeperf` 后测得
  `11.77,17.41,12.09,13.83,18.73,11.44` ⇒ **max 18.7% / mean 14.2%**（master 90s 同时段；说明该轮
  环境实际是干净的）。**结论：90s 两轮按 typeperf 口径不构成 CPU 偏离；原 CIM 数字不再作为
  「环境不干净」的依据。360s 起跑前一律 typeperf + 单次 CIM 归属快照 `cpu_attrib`。**
- 90s 对照是 master vs 修复树两个**各自独立起栈**的轮次（修复树 90s = 20:23 `rust-fix90`；master 90s =
  20:29 `rust-master90b`）：同栈、同 500/90 口径，但**不是同窗口紧邻的 A/B**，按「同口径非同窗口」读。
- **外部 I/O（如实记录，不写成「环境噪声」）**：2026-11-15 **20:31:00–20:35:55** 有 `C:\` 顶层全量递归扫描
  （powershell `Get-ChildItem -Recurse`；20:35:55 起的 `C:\Users\gxh` 扫描于 ~20:36:00 被杀），来源为
  **项目总监自查**。20:31–20:36 之间的 strict 采样（含 20:35 那批 typeperf）若见 `powershell`/`System`/
  `WmiPrvSE` 抬升，归因给该扫描；**重采后（20:36:10 之后）的数字才是 360s 起跑判据**。
  master 90s（20:29:14–20:30:44）与扫描无重叠，**该份数据不受影响**。
- 360s 起跑前：typeperf 8 点；`max<20%` 直接起；`20%≤max<30%` 且归因到 `claude`/`System`/`WmiPrvSE`
  可起并记录 `cpu_attrib`；`max≥30%` 或归因不清 ⇒ 延后。

### A8 结论（本轮未取得稳态 after-GC 点）

- 90s 与 360s 两轮都**没有观测到稳态段（爬坡后）的 `Gen 2 GC Count > 0` 采样点**：360s 的 3 次 Gen2 GC
  全在 t≤35s（150→231→261MB）。按总监口径：**写「本轮未观测到稳态 after-GC 点」，不拿堆大小替代斜率**；
  A8 的「每 bot Gen2 / 斜率≈0」**本轮未取得**。
- 修复树 360s 的 BotSrv WS 尾段 **+0.946MB/s**（59.6s）与首轮 master 的 +0.013MB/s（59.1s）差异**未归因**。
  可能来源：无稳态 Gen2 GC 时的 GC 惰性增长、同机背景负载、2000ms 守卫在进世界瞬态保留更多 actor。
  要判定需下一轮 `dotnet-gcdump` 数 `Actor` 实例或强制 GC 前后存活量，**不能仅凭 WS 归因**。
- 池的量级核算：`scene_actor_ejected=4435` 若在修前全部入池，按 1.5–3KB/actor ≈ **6.6–13.3MB**，
  小于尾段 ~80MB 的 WS 增长 ⇒ **尾段增长不能归因于池**（也说明删除池不是「内存平」的充分解释）。

### 跨轮对照（19:56 首轮 master 360s vs 本轮修复树 360s，同栈同档）

| 指标 | 19:56 首轮（master） | 本轮（修复树 `24a608c54`） | 差异归因 |
| --- | --- | --- | --- |
| 栈 | C# DBSrv/LoginSrv + Rust game-svr + Rust 三网关 | 同 | — |
| in_world 峰值/稳态 | 500/500 | 500/500 | 无差异 |
| tick P50/P90/P99/max | 15 / 16 / 32 / **172ms** | 15 / 16 / 32 / **63ms** | max 更低；P50/P90/P99 相同 |
| 稳态 ≥201ms | 0（max 172） | 0（max 63） | — |
| `recv_packet_null` | **4** | **1** | 降低但未归零 ⇒ **罕见残余竞态**，不宣称归零 |
| internal_errors | 4 | 1 | 同上 |
| Gen2 次数 / after-GC 序列 | 未采 counters | 3 次：17s 150 / 31s 231 / 35s 261MB（全爬坡） | 两轮都无稳态 after-GC 点 ⇒ A8 未取得 |
| `scene_actor_ejected` | 旧码无字段 | **4435（11.97/s 全窗；18.71/s 末 120s）** | 控制量：驱逐路径确在跑 |
| BotSrv WS | 961.6→883.5，峰 1015.4，尾 +0.013MB/s（59.1s） | 896→987，峰 1609（22s 瞬态），尾 +0.946MB/s（59.6s） | **未归因**（见 A8 结论；池量级 6.6–13.3MB < 尾段 ~80MB） |
| game-svr WS | 未采样 | 523→558，尾 +0.045MB/s（近似平） | 采样口径新增（仅 360s），无首轮对照 |
| 三网关 WS | 未采样 | game-gate 42 平、login-gate 5–11、sel-gate 11 平 | 同上 |
| gate 丢弃率 | 未采集 `gate_stats` | 未采集 `gate_stats` | **N/A（两轮一致）** |
| host_load | cpu_mean 48.5 / peak 76 / heavy 0（CIM 口径） | cpu_mean 45.8 / peak 84 / heavy 0（CIM 口径） | 同量级；起跑前 typeperf max 16.2%/mean 6%，外部扫描与构建均在起跑前结束 |

**结论口径**：无界结构已移除（A7 机制证明 + 4 颗变异逐个红）；当前健康栈不复现历史 BotSrv 内存表现。
**不写「泄漏已修复」——历史泄漏的根因归属与当前栈的表现是两件事。**

## 4. 变异（逐条真跑，恢复后核对 sha）

| 变异 | 期望 |
| --- | --- |
| 在 AOI 驱逐路径重新加入静态保留（`static List<Actor>` + `Add`） | ⑫ `aoi-evict-not-retained-after-gc` 红（`aliveAfterGc > 0`） |
| 删除 AOI 的 2000ms 空闲守卫（还原为只看距离） | ⑩ `aoi-idle-guard-skips-recent` 红（近消息越界 actor 被误删） |
| 删除 `ProcMsg` 里的 `m_dwLastGetMessageTime` 刷新 | ⑪ `procmessage-refreshes-lastget` 红（`freshAge` 仍 >2000） |
| 删除 `ClientManager.ProcessOnePacket` 的 `finally { MShare.MySelf = prev; }` | M3c `processonepacket-context-per-bot` 红（`after=leaked`） |

## 5. 残留与未验证

- **A8 未取得**：90s/360s 均无稳态段 after-GC 点；「每 bot Gen2 / 斜率≈0」本轮不可判（见 §3 A8 结论）。
  要判定需下一轮用 `dotnet-gcdump` 数 `Actor` 实例或强制 GC 前后存活量。
- **`recv_packet_null` 未归零**：首轮 360s=4，本轮=**1**（降低但非零）；`bots.log` 无异常类型样本
  （`internal_error_diag.logged_blocks=0`）⇒ 记为**罕见残余竞态**，不宣称已消除。本轮 verdict 因此为 RED。
- **C# `GameSvr` 不能作为 500 bot 的窗口栈**：`WorldServer.ProcessHumans` 的玩家数据加载是「待实现」，
  实测 500 bot 只进 21/500 且刷 487 条 `List.RemoveAt` 异常；部署的 `E:\MirServer\M2GameSvr\GameSrv.exe`
  （2026-10-22，来源 worktree 已删）另属栈卫生问题，已上报另开卡。历史 ~10MB/s 泄漏是在 C# 栈上观测的，
  当前 Rust 栈不复现该内存表现。
- **外部 I/O 与构建窗口**：20:31:00–20:36:00 项目总监自查的全盘递归扫描、20:35:30–20:36:10 审查者构建
  均在 360s 起跑前结束；360s 的 strict 采样为 20:36:20（typeperf max 16.2%/mean 6%，heavy=0）。
- **gate 丢弃率**：本卡两轮均未采集 `gate_stats`（Rust 栈的 load_gate 不产出），对照标 N/A；不是 0。
- 未改动 `_reviceQueue` 的无界通道（分析报告的次要候选；本卡范围外，登记为后续风险）。
- `Actor` 单对象尺寸仍未用 gcdump 测；§3 的池量级 6.6–13.3MB 是估计值（1.5–3KB/actor）。
- 真机档位只跑 500 单档（总监指定）；200/1000 未重测。
