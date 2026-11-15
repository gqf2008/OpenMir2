# BotSrv LOH 增长归因（2026-11-15）

卡：`openmir2-botsrv-loh-growth`（tier=R，`window_required`）。
身份：`openmir2-worker-88`；分支 `card/botsrv-loh-growth`；基线 master `52040984a`。
**本轮未起栈、未构建、未改任何被测进程代码**；只读分析已归档的两轮 500 档 360s counters CSV、同轮
`load_stats.ndjson` / `mem.ndjson` / `bots.log` / `preflight.txt` 与 BotSrv 源码。

> 口径声明：本文所有 LOH 数字来自**连续 counters 流**（两份 CSV 各 372 个采样、跨度 371.0s），
> 不是端点两次差。结论分三层：**已定量归因** / **由 GC 时机解释** / **现有数据不足**（明确标注）。

---

## 0. TL;DR

| 问题 | 结论 | 证据强度 |
| --- | --- | --- |
| LOH 基线 ~216MB 是什么 | **500 × `new MapInfo[90,120]`**（`TMap` 构造，每 bot 432KB）→ 216.0MB；观测 LOH 216.15MB（差 0.07%） | 启动单步 +120.2MB、总量算术、Gen2 不回收（对象存活）三线吻合 |
| 稳态 +0.079~0.081MB/s 是什么 | **`LoadMetrics.WriteLoginConnsLocked` 每 5s 整表重写**：`StringBuilder(80,064 chars)=160,128B` + `ToString()=234,204B` ≈ **394,4xxB/5s = 0.0789MB/s** | 两轮稳态 LOH 增量唯一值均 ≈394,4xxB，间隔主频 **5s**；算术误差 ≤3% |
| 两轮端点符号为何相反 | **主要是 GC 时机**：LOH 平的那轮在 **362s** 发生稳态 Gen2，回收 **−25.26MB**，与 320s × 0.0789MB/s ≈ 25.2MB 的 flush 垃圾吻合；上升那轮稳态段无 Gen2 | 时刻表 + 回收量与分配账本同时对上 |
| 还有没有未解释的部分 | **有**：LOH 上升那轮在 **~300s 后**额外出现 ~131KB 量级的 LOH 分配（1–11 次/s，尾段超额 ≈28–31MB（随窗口）），两轮同段行为不同；**现有数据不足以归因** | 步长量子 131,128B；需要分配采样（Phase 2） |
| 单轮 360s 能否判内存健康 | **不可靠**。本轮 A8-conclusion 仍不成立（爬坡后只有 1 个 Gen2 after-GC 点，要求 ≥2 个） | 卡面硬要求，原样保留 |

**不得**写成「LOH 已修复/不复现」：本轮只完成归因；两轮 binary 与 counters 实例口径都还有缺口（§1.3）。

---

## 1. 数据口径与可复现性

### 1.1 归档与样本

| 轮次 | 归档 | counters 文件 | 样本 | 跨度 | t0 |
| --- | --- | --- | --- | --- | --- |
| A：上升轮 | `E:/UserFleet/load-actorpool500-20261115-rust-fix360b/` | `dotnet-counters-500.csv` | 372（每计数器） | 371.0s | 2026-11-15 20:36:41 |
| B：平轮 | `E:/UserFleet/load-360-errorstack-20261115-2116/` | `dotnet-counters-500.csv` | 372（每计数器） | 371.0s | 2026-11-15 21:15:54 |

复算入口（本卡新增，含 13 条合成自测）：

```bash
python -I -B tools/botload/counters_series.py \
  "E:/UserFleet/load-actorpool500-20261115-rust-fix360b/dotnet-counters-500.csv" \
  "E:/UserFleet/load-360-errorstack-20261115-2116/dotnet-counters-500.csv"
python -I -B tools/botload/counters_series.py --selftest   # 13/13 PASS
```

### 1.2 两轮时间序列（A8-trend 口径）

| 指标 | A：fix360b | B：2116 |
| --- | --- | --- |
| LOH 首→末 | 95.9 → **275.9MB**（原始端点 +180.0） | 216.0 → **218.7MB**（原始端点 +2.6） |
| 爬坡末端(60s) 单点 | **218.1MB** | **219.1MB** |
| **爬坡后端点变化（60s→末）** | **+57.8MB / 311s = +0.1860MB/s** | **−0.4MB / 311s = −0.0013MB/s** |
| OLS 后段(60–371s) | +0.1399MB/s（n=312） | +0.0652MB/s（n=312） |
| OLS 稳态前 240s(60–300s) | **+0.0810MB/s**（n=241） | **+0.0791MB/s**（n=241） |
| OLS 尾 60s | +0.5384MB/s | −0.2616MB/s |
| Gen2 时刻（rate>0，共 3 次） | **17 / 31 / 35s**（全在爬坡段） | **14 / 26 / 362s**（最后一个在稳态段） |
| Gen2 时刻的 LOH 变化 | −0.51 / −1.19 / −0.40MB | −0.51 / −0.79 / **−25.26MB** |
| Allocation Rate（MB/s） | mean 418.3 / median 436.9 / p95 501.3 / max 544.7 @363s | mean 433.6 / median 449.0 / p95 525.4 / max 570.6 @349s |
| Allocation Rate 稳态(≥60s) | mean 442.4 / median 446.0 / p95 506.9 | mean 460.8 / median 458.7 / p95 530.5 |
| 启动最大单步 | **+120.2MB @ 4.0s** | 观测窗内未见（发生在首采样之前；首采样已是 216.0MB） |
| BotSrv WS 尾段（末 60s，`mem.ndjson`） | **+0.4024MB/s**（931.0→987.4MB） | **−1.2615MB/s**（933.8→850.0MB） |

> WS 尾段窗口取 `mem.ndjson` 全程末 60s；与 `capacity-model` 四轮表的窗口/端点不同，数值不可直接并列，本表只用于 A8-trend 三件套。

两轮的**稳态增长速度几乎相同**（0.0810 vs 0.0791MB/s），端点结论相反只发生在 300s 之后
（§3.3/§3.4）。

### 1.3 实例口径缺口（必须随结论一起读）

- A 轮的 `preflight.txt` 明确：`COUNTERS_TARGET posthoc=BotSrv pid=5832 source=runner-poll-at-attach
  note=collection-time-provenance-missing` —— **采集时没有记录目标**，PID 是事后从 runner 日志补的。
- B 轮记录了 `COUNTERS_TARGET process=BotSrv pid=29532 ... time=21:15:53`，但 CSV 本身仍无 pid 列；
  `Process=BotSrv` 究竟命中哪一个实例，归档不能自证。
- 两轮 `dotnet-counters-500.csv` 都只有 `Provider/System.Runtime` 五列，**没有进程列**。
- 两轮 BotSrv 的二进制造型也不同：A 轮 `bots_runtime.json` 指向
  `.worktrees/worker-85/src/BotSrv/bin/Release/BotSrv.exe`（20:05 构建）；
  B 轮指向主检出 `src/BotSrv/bin/Release/BotSrv.exe`，其 mtime 是 **21:22（运行结束之后）**，
  即**被测二进制在窗口后被重建过**，归档无法钉住它。两轮可用的 exe 哈希也不同
  （`a6ea7eae…` vs `9f6df1bb…`）。所以「同树」只能是「同族树」，**不是可证的同一二进制**。

⇒ 本文的归因结论主要落在**代码路径与计数器形状**上（5s 步长、394KB 量子、启动 432KB×500），
这些不依赖具体二进制的少量差异；但**任何跨轮的速率绝对值比较都受上述缺口约束**。

---

## 2. 归因链

### 2.1 LOH 基线 216MB = 每 bot 一个 432KB 的地图视图数组（已定量）

**代码位置**：`src/BotSrv/Maps/TMap.cs` 构造器 `TMap(RobotPlayer robotClient)`：

```csharp
m_MArr = new MapInfo[BotConst.MAXX * 3, BotConst.MAXY * 3];   // MAXX=30, MAXY=40 ⇒ 90×120
```

`RobotPlayer` 构造时 `Map = new TMap(this)`（`src/BotSrv/Player/RobotPlayer.cs`），
`TMap.m_MArr` 是实例字段、被 `RobotPlayer.Map` 引用、`RobotPlayer` 被 `ClientManager._clientList/_clients`
持有 ⇒ **对象存活，Gen2 收不走**。

**尺寸算术**：`MapInfo` 是 struct，值字段 28B + `byte[] btUnknown` 引用 8B，按 8B 对齐 ⇒ **40B**
（编译器 `sizeof` 需 Phase 2 用 gcdump 复核，此处是布局推算）。于是：

```
90 × 120 × 40B = 432,000B/bot
× 500 bot     = 216,000,000B = 216.0MB（MB=10^6）
```

**观测吻合**：

- A 轮 LOH 在 **20:36:44→20:36:45** 一步从 **95.9 → 216.2MB（+120.3MB）**；此前首采样已是 95.9MB。
  按 432,000B/bot 折算：95.9MB ≈ 222 个 bot 的数组 + 120.3MB ≈ 278 个 bot 的数组 ⇒ **222+278=500**。
- 两轮 LOH 的稳态基线都是 **216.15MB**（A：216.152；B：216.036→216.153），与 216.0MB 差 **0.07%**。
- 三次爬坡段 Gen2 只让 LOH 降 0.4–1.2MB（A）——216MB 是**存活对象**，不是待回收垃圾。

⇒ **LOH 的绝对水位不是泄漏，也不是稳态分配，而是 500 个 bot 各持有一份 90×120 的 MapInfo 视图数组。**

### 2.2 稳态 +0.079~0.081MB/s = 每 5s 的 login_conns 整表重写（已定量）

**代码位置**：`src/BotSrv/LoadMetrics.cs`，`FlushLoop` 每 `_flushIntervalMs = 5000` 调 `Flush()`，
`Flush()` 内调 `WriteLoginConnsLocked()`：

```csharp
var sb = new StringBuilder(LoginConns.Count * 160 + 64);   // 500 × 160 + 64 = 80,064 chars
foreach (...) { sb.Append(...); }                          // 写 500 行
File.WriteAllText(_loginConnPath, sb.ToString(), new UTF8Encoding(false));
```

每次 flush 至少产生两个 >85KB 的对象：

| 对象 | 大小 | 依据 |
| --- | --- | --- |
| `StringBuilder` 初始 `char[]` | 80,064 chars × 2B = **160,128B** | 容量参数 |
| `sb.ToString()` 的 string | 117,102 chars × 2B ≈ **234,204B** | 归档 `tier-500/login_conns.ndjson` = 117,102B、500 行、232–235B/行 |
| 合计 | ≈ **394,4xxB / 5s = 0.0789MB/s** | 两对象之和 |

**观测吻合（这是最强的一段证据）**：

- 两轮稳态段（60–300s）的 LOH **正增量唯一值**几乎只有 `393,272 / 394,432 / 394,440 / 394,448` 几种，
  即每次增长 **≈394KB**；B 轮 49 个正增量**全部**落在 393,272–394,448B 区间，即一次 flush 的量。
  A 轮另有少数非整步值（131,128 / 196,664 / 262,256），是 flush 的 394KB 被 1s 采样边界切开、或与 131KB 量子叠加；基频仍是 5s（44 个 5s 间隔）。
- 增量间隔主频：A 轮 **5s×44**、B 轮 **5s×46**（240s 内 ~48 次 flush）——与 `_flushIntervalMs=5000` 一致。
- 算术 0.0789MB/s vs 观测稳态 OLS：A +0.0810（+2.7%）、B +0.0791（+0.3%）。
- **A/B 两轮斜率几乎相同**，因为这个路径在两轮代码里都没变。

⇒ 稳态 LOH 增长是**可回收垃圾**（每 5s 约 394KB），只在 Gen2 时被回收；它不是泄漏，
但在没有稳态 Gen2 的窗口里会按 0.08MB/s 累积（360s ≈ 28MB）。

### 2.3 362s 的 Gen2 解释了端点符号翻转（由 GC 时机解释）

- B 轮 Gen2 在 **362s**：LOH 从 **243.2 → 217.9MB（−25.26MB）**，回到 216MB 基线附近。
- 按 §2.2 的账本，从爬坡后最后一次 Gen2（26s）到 362s 共 336s：
  `336s × 0.0789MB/s ≈ 26.5MB`，与实测回收 25.26MB 同量级（差 5%）。
- A 轮稳态段**没有任何 Gen2**，同样的垃圾继续累积 ⇒ 端点自然更高。

端点符号对比（爬坡后 60s→末）：

- A：218.1 → 275.9（+57.8MB）；其中 60–300s 按 0.08MB/s 只应 +19.4MB，实际 +21.2MB；
  **剩余约 +28MB 发生在 300s 之后**（§3.4）。
- B：219.1 → 243.2（+24.1MB，到 361s），362s Gen2 回收 −25.3MB ⇒ 末 218.7MB。

⇒ **「同一族树两轮 LOH 斜率相反」的主要原因是稳态段有没有撞上一次 Gen2**；
只看端点会把 GC 时机当成内存行为差异。

### 2.4 尾段剩余差异（现有数据不足以归因）

A 轮 300s 之后 LOH 从 239.3 涨到 275.9（72s +36.6MB，OLS +0.538MB/s），减去 §2.2 的 0.08MB/s，
**超额 ≈+0.46MB/s（尾 60s ≈28MB）；300s 后累计超额 ≈+31MB**。这段时间的单步增量呈现 **131,128B 的量子**（1×~11×），例如
131,128 / 262,256 / 393,384 / 655,640 / 918,952 / 1,311,280；而 B 轮同段只有 394,4xxB 的 flush 步长。
**131,128B 对应当前代码里的哪一个分配点，现有数据不能判定**：它不匹配 flush 的两个对象，也不匹配
`MapInfo[90,120]`（432,000B）；同段 `recv_chunks/recv_chunk_bytes` 的平均 chunk 仍约 24B，
所以「整块 64KiB 收包 → `GetString` 128KB string」这一候选**不能由现有聚合计数证明**（需要 chunk 大小分布）。

候选（Phase 2 逐个证伪）：
1. 收包侧 `HUtil32.GetString(e.Buff, 0, e.BuffLen)` 在收到大 chunk 时生成 ≥85KB string；
2. 低频但单次 >85KB 的协议体解码（`EDCode.DeCodeString` / `ClientGetBagItmes` 等）；
3. 与 actor 驱逐/重建相关的偶发大对象（`scene_actor_ejected` A=4435 vs B=4049，量级差 9.5%，不足以解释 6 倍速率差）；
4. 计数器实例口径问题（§1.3）：CSV 可能混入/切到别的 BotSrv 实例。

> 明确写法：**尾段超额增长 = 未归因**。不得在报告里写「由收包缓冲造成」；目前只到「候选 + 待采样」。

---

## 3. 静态候选清单（>85KB 单次分配）

LOH 阈值按 .NET 默认 **85,000B**。逐条给出代码位置、对象类型、尺寸、触发频率假设与当前判据。

| # | 代码位置 | 分配 | 单次大小 | 频率假设 | 现状 |
| --- | --- | --- | --- | --- | --- |
| 1 | `src/BotSrv/Maps/TMap.cs` `TMap` 构造器（`m_MArr = new MapInfo[BotConst.MAXX*3, BotConst.MAXY*3]`） | `MapInfo[,]`（struct，40B） | **432,000B/bot** | 每 bot 一次（构造时） | **已定量**：500 bot → 216MB 基线（§2.1） |
| 2 | `src/BotSrv/LoadMetrics.cs` `WriteLoginConnsLocked`（`new StringBuilder(LoginConns.Count*160+64)`） | `char[]` | **160,128B** | 每 5s 一次 | **已定量**：稳态主因之一（§2.2） |
| 3 | 同上（`sb.ToString()` 传给 `File.WriteAllText`） | `string` | **≈234,204B** | 每 5s 一次 | **已定量**：与 #2 合计 394,4xxB/5s |
| 4 | `src/BotSrv/Player/RobotPlayer.cs` `ReceivedData`（`HUtil32.GetString(e.Buff, 0, e.BuffLen)`） | `string` | 最多 **131,072B**（65,536 ASCII chars） | 仅当单次 recv ≥ ~42.5KB；实测平均 chunk 24B | **未证实**；尾段 131,128B 量子与它形状相同，但缺分布证据 |
| 5 | `src/BotSrv/Maps/PathMap.cs` `FillPathMap`（`new PathcellSuccess[h, w]`，8B/cell） | 二维 struct 数组 | 200×200 = **320,000B/次**；1000×1000 = 8MB | 每次 `TMap.FindPath` 调用 | **当前环境惰性**：Windows 地图根写死 `D:/Mirserver/Mir200/Map/`，该路径不存在 ⇒ `LoadMap` 失败、header 为 0，数组退化为 [0,0]；生产有图时是真候选 |
| 6 | `src/BotSrv/Maps/TMap.cs` `LoadMapData`（`new CellParams[w, h]`，2B/cell） | 二维 struct 数组 | ≥207×207 时 >85KB | 每 bot 每次加载/切图 | **当前环境惰性**（同 #5） |
| 7 | `src/BotSrv/Objects/HeroActor.cs` `AutoFindPath`（`(ushort[,])MShare.g_APPassEmpty.Clone()`，2B/cell） | 二维数组克隆 | ≥207×207 时 >85KB | 每 hero 每次寻路 | **当前环境惰性**（同 #5；且本压测无 hero 证据） |
| 8 | `src/BotSrv/ClientManager.cs` `new RobotPlayer[_clientList.Count]` | 引用数组 | 500 × 8B = 4,000B | 每 tick | **排除**（<85KB） |
| 9 | `src/BotSrv/SocketShim.cs` `RecvLoop`：`new byte[65536]`（每连接）+ `new byte[n]`（每 chunk） | `byte[]` | 65,536B / ≤65,536B | 每连接一次 + 每 chunk | **排除为 LOH 来源**（单个 ≤64KiB <85KB）；只影响 Gen0/Gen2 |
| 10 | `src/BotSrv/ClientManager.cs` `Channel.CreateUnbounded<RecvicePacket>()` | 队列段 | 默认选项走 `ConcurrentQueue`，段最大 1024 引用 ≈8KB | 持续 | **排除**（<85KB；Phase 2 gcdump 可复核段大小） |
| 11 | `src/OpenMir2/EDcode.cs` `DecodePacket/DeCodeString`（`GetBytes` + `EncryptUtil.Decode`） | `byte[]` | ≈0.75 × body；body ≤ 单 chunk 64KiB ⇒ ≤48KB | 每帧 | **排除为 LOH 来源**（单帧 ≤64KiB 时解码缓冲 <85KB）；>113KB 的单帧在当前组帧器下会被当残帧丢弃，不会累积 |
| 12 | `src/BotSrv/Player/RobotPlayer.Operate.cs` `ProcessPacket`（`ArrestStringEx` 每帧切 2 个子串） | `string` | 与 chunk 同阶（≤128KB） | 每帧 | 与 #4 同源；**未证实** |
| 13 | `src/BotSrv/LoadMetrics.cs` `StatsLine()`（直方图 + ident_counts） | `StringBuilder`/`string` | 实测行宽 **2.35KB**（75/76 行） | 每 5s | **排除**（<85KB） |
| 14 | `src/BotSrv/Player/RobotPlayer.cs` `ClientGetBagItmes` / `ClientItemCodec.Decode` | `ClientItem` 列表 + 临时 `MemoryStream` | 100 件 × ~124B ≈12.4KB | 每次 SM_BAGITEMS/ADDITEM | **排除为稳态来源**（单包 <85KB；一次性） |
| 15 | `src/BotSrv/Maps/TMap.cs` `LoadMapArr`（每 cell `new MapInfo(readData)`） | 小对象 | 40B/个 | 每 cell | 非 LOH（单个小对象）；当前环境惰性 |

**静态结论**：#1–#3 已被现有 counters 定量证实；#4–#7 是「生产/大包/有图」条件下必须复核的候选；
其余要么尺寸不足，要么在当前环境惰性。

---

## 4. 代价与处置建议（本卡不预设修法）

### 4.1 代价（按已证实部分算）

| 项 | 500 bot | 1000 bot（线性外推） | 性质 |
| --- | --- | --- | --- |
| `MapInfo[90,120]` 基线 | **216MB** | **432MB** | **存活**，Gen2 收不走；占 8GiB 上限的 2.6%/5.3% |
| login_conns flush 垃圾 | **≈28MB / 360s** | ≈56MB / 360s（行宽约翻倍） | **可回收**，只在 Gen2 回收 |
| 1000 档 4GiB 上限 OOM 余量 | — | 432MB 基线 + Gen2 ~300MB + 其他 | 基线本身不是 OOM 主因，但会压缩余量 |

> 1000 档 OOM 的既有归因是历史 actor 池泄漏（已由 `openmir2-botsrv-actor-pool-leak` 移除）；
> 本节的 432MB 是**独立**的常驻成本，不能拿来解释那次 OOM，也不能因为删池后仍 OOM 就归因于它——
> 需要 Phase 2 的 1000 档实测。

### 4.2 候选处置（列出代价，不选定）

| 方向 | 预期收益 | 风险/代价 | 需要先证 |
| --- | --- | --- | --- |
| 缩小 `m_MArr` 视口（90×120 是否等于真实索引上界） | 按比例降 216MB；若只需 60×80 则 −70% | 越界索引会改变 `MarkCanWalk`/`InMapRange` 行为；需逐路径核 bounds | 先证 `m_MArr` 的真实索引范围（代码 + self-check） |
| `m_MArr` 惰性分配（仅在有图/需要时） | 无图环境降到 0；有图时仍 432KB | 改变 `Map` 生命周期与判空分支；影响 #5/#6 的联动 | 先证无图路径不依赖非空 `m_MArr` |
| login_conns 增量写（StreamWriter 逐行）或变更时才重写 | 去掉 0.08MB/s 稳态垃圾与 394KB 尖峰 | 文件语义从「整文件快照」变「追加/增量」；读者要改 | 先盘点 `login_conns.ndjson` 的消费者（判据脚本） |
| 直接预留/复用 char 缓冲（ArrayPool） | 同上 | 引用生命周期/清零成本；收益仅 394KB/5s | 与上行二选一 |
| 不管，靠 Gen2 回收 | 0 改动 | 360s 窗口内 LOH 可能 +28MB；A8-trend 读数需注明 | 已是现状 |

**本卡不做任何代码修改**；以上只作为归因后的处置选项，需另卡评审。

---

## 5. Phase 2：工具方案（报批后执行）

> 今晚不采集。Phase 2 需要新开诊断窗口（不能污染 GREEN 判据），且需要安装工具。

### 5.1 工具现状

| 工具 | 现状 | 用途 |
| --- | --- | --- |
| `dotnet-counters` | **已装**（10.0.750501） | LOH/GC/Alloc 连续流（现有 CSV 的来源） |
| `dotnet-gcdump` | **未装** | LOH 对象**构成**（类型/大小/存活） |
| `dotnet-trace` | **未装** | `GCAllocationTick` 分配事件 + 调用栈（归因到 `file:function`） |
| `dotnet-dump`（可选） | **未装** | 事后 `dumpheap -stat` 交叉复核 |
| PerfView（可选） | **未装** | 读 nettrace 的 `GC Heap Alloc Stacks` |

安装（需要批准，属于环境变更）：

```powershell
dotnet tool install -g dotnet-gcdump
dotnet tool install -g dotnet-trace
# 可选：dotnet tool install -g dotnet-dump
```

### 5.2 采集步骤（可复跑）

前置：拿到**被测 BotSrv 的确切 PID**，并在采集时落 provenance（不能事后补）：

```powershell
$pid = (Get-Process BotSrv | Select-Object -First 1).Id
$start = (Get-Process -Id $pid).StartTime
"COUNTERS_TARGET process=BotSrv pid=$pid start=$($start.ToString('o'))" | Tee-Object -Append counters.log
```

1) 连续 counters（与现有口径一致，1s 刷新）：

```powershell
dotnet-counters collect -p $pid --refresh-interval 1 --format csv `
  --counters "System.Runtime[gc-heap-size,loh-size,alloc-rate,gen-0-gc-count,gen-1-gc-count,gen-2-gc-count]" `
  -o counters.csv
```

2) LOH **构成**（在窗口结束后或诊断轮的 t≈120s/350s 各一次；gcdump 会触发一次阻塞 GC）：

```powershell
dotnet-gcdump collect -p $pid -o snap-120.gcdump
dotnet-gcdump report snap-120.gcdump -o snap-120.csv
# 找 LOH 头号类型（期望看到 MapInfo[,] / System.String 的尺寸分布）
```

3) 分配 **调用栈**（短、只在一段时间内开；见 §5.3 开销）：

```powershell
dotnet-trace collect -p $pid --profile gc-verbose --duration 00:00:30 `
  --format nettrace -o alloc.nettrace
# 分析：PerfView 打开 alloc.nettrace，看 "GC Heap Alloc Stacks"（按分配字节排序）
#   过滤 85KB 以上对象的分配栈；或先按类型聚合，再回到栈。
```

4) 归因到调用点：PerfView 里按 **Alloc Size** 排序，取 top stack，记录 `模块!类型!方法`；
把栈顶映射回 `src/BotSrv/**` 的方法名，形成 `file:function` 证据。**不只看类型名**：
`MapInfo[,]` 只能说明「地图数组」，调用栈才能把稳态尾段的 131KB 量子钉到具体方法。

### 5.3 开销与规避

| 工具 | 对 500 档的影响 | 规避 |
| --- | --- | --- |
| `dotnet-counters` | 低（现有窗口已在用；1s 刷新） | 保持现状；只加 provenance |
| `dotnet-gcdump` | **会触发一次阻塞 GC + 堆遍历**，按 ~600MB 堆估计停顿数百 ms～秒级；会污染 tick P99 | **只在窗口结束后**（gate 已收口）采；或另开**诊断轮**，该轮不得作为 GREEN 证据 |
| `dotnet-trace gc-verbose` | Allocation Tick 在 ~418MB/s 下约每 100KB/类型触发一次 ⇒ 数万事件/分钟；EventPipe 缓冲与栈回溯带来 CPU/延迟开销（经验 5–15%，需实测） | 短时窗（10–30s）、`--buffersize` 限流、只在诊断轮的稳态段采；先跑 10s 试采看 `tick_max_ms` 是否劣化 |
| `dotnet-dump` | 全堆转储，停顿与磁盘 IO 大 | 只做事后交叉复核，不在判据窗口内 |

**原则**：诊断轮与判据轮分离；`dotnet-gcdump`/`dotnet-trace` 产生的窗口**不得**用于 GREEN 结论，
也不得把受污染的 tick 样本混进 A8/A1 判据。若必须在同一轮采，至少记录采样时刻并在报告里剔除/注明。

### 5.4 Phase 2 要回答的三个问题（判据）

1. `MapInfo[,]` 是否就是 216MB 的 LOH 主体（gcdump 类型尺寸）？`MapInfo` 的实测 `sizeof` 是否 40B？
2. 尾段 131KB 量子的分配栈是什么（`GCAllocationTick` top stack）？是否与 `ReceivedData`/`GetString` 相关？
3. 1000 档时 `m_MArr` 基线是否线性到 ~432MB，并对 4GiB 上限的余量造成多少压缩？

---

## 6. 未做的事与原因

- **未起真机窗口**：总监规定今晚不启栈；本卡只做已归档数据 + 静态分析。
- **未跑 `dotnet` 构建/未装新工具**：`dotnet-gcdump`/`dotnet-trace` 未安装，安装属环境变更，需报批。
- **未改 BotSrv 代码**：卡面要求「只做归因与证据，不预设修法」。
- **未验证 `MapInfo` 的 `sizeof`**：只能给布局推算（40B）+ 计数器算术吻合；Phase 2 用 gcdump 复核。
- **未归因尾段 131KB 量子**：现有聚合计数没有 chunk 大小分布，无法区分「大收包 string」与其他路径。
- **未取得 master 对照**：卡面要求 master vs 修复树同口径；master 360s 没有 counters（既有文档已记）。
  本卡用「同族两轮」对照，并明确标注实例/二进制缺口。
- **未签成 patch/review**（若提交时出现）：见卡面 comment 的阻塞说明；不自行 review/merge。

---

## 7. 复现命令汇总

```bash
# A8-trend 两轮对照（本文所有 LOH/Gen2/Alloc 数字）
python -I -B tools/botload/counters_series.py \
  "E:/UserFleet/load-actorpool500-20261115-rust-fix360b/dotnet-counters-500.csv" \
  "E:/UserFleet/load-360-errorstack-20261115-2116/dotnet-counters-500.csv"

# 工具自测
python -I -B tools/botload/counters_series.py --selftest

# login_conns 尺寸（§2.2 的 117,102B / 500 行）
wc -c -l "E:/UserFleet/load-actorpool500-20261115-rust-fix360b/tier-500/login_conns.ndjson"

# 启动单步（§2.1 的 95.9→216.2MB）
python -I -B tools/botload/counters_series.py --json \
  "E:/UserFleet/load-actorpool500-20261115-rust-fix360b/dotnet-counters-500.csv" \
  | python -c "import json,sys; d=json.load(sys.stdin)[0]; print(d['loh']['max_step'])"
```

---

## 8. 判据/口径引用

- A8 两档定义：`docs/capacity-model-20261114.md`「内存判据两档」。
- 报告模板：`docs/memory-report-template-20261115.md`。
- counters provenance 缺口与「单轮 360s 不可靠」：本卡卡面 + `docs/loadgate-criteria-runbook-20261115.md`。
