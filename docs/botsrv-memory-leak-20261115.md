# BotSrv 内存泄漏追加分析（2026-11-15，卡 `openmir2-botsrv-heap-limit-oom`）

身份：`openmir2-worker-82`；分支 `card/botsrv-memory-leak`；只读分析，未起栈、未构建、未改代码、未改冻结客户端。
触发：总监 2026-11-15 在卡上重定级（≈11.2MB/s 稳定增长到 4GiB ⇒ 泄漏，不是堆上限问题），并提两问。

本报告回答两问，并给出**代码级根因**（夹具侧端口缺陷，非产品级）。

---

## 0. TL;DR

| 问题 | 结论 |
| --- | --- |
| 泄漏速率是否与被丢弃帧相关 | **不相关**。内存增长对「交付帧率」拟合 r²=0.717（表内 8 点；口径敏感区间 0.72–0.76，见 §2），对「丢弃帧率」拟合 r²=0.007（斜率 2.5e-5，≈0）。反例：cN1 丢弃 126,014 帧/s 时增长 45.45MB/s；cap360 丢弃同量级（129,302 帧/s）时仅 12.4MB/s 且后期归零；ab3 丢弃 0 时增长 34.1MB/s。**泄漏不在丢弃路径上**。 |
| 真实 Delphi 客户端有无等价累积模式 | **未发现无界累积**。actor 死亡真释放（60s TTL + `TActor.Free`，且 `ClearActors` 释放全池）；消息队列每条 `Dispose`/`Delete`；聊天板硬上限 200 行；伤害飘字在绘制路径按计数删除；收包在主线程同步解析，无后台无界队列。⇒ **不是产品级问题**。 |
| 泄漏来源 | `src/BotSrv` 的 **actor 自由池只增不减**：AOI 驱逐（`PlayScene` tick）以及 `DeleteActor` / `DelActor` 三个入口都把 Actor 塞进 `MShare.g_FreeActorList`，而唯一清池函数 `MinTimerTimer` **从未被任何定时器订阅**（死代码）；`NewActor` 也从不复用池。客户端对应清理函数是接线好的（`ClMain.pas:13828`，TTL 后 `TActor.Free`）。 |
| 对 1000 档的影响 | 泄漏按 bot 计、随时间无界：≈12.4MB/s 每 500 个 in-world bot ⇒ 1000 个约 25MB/s，360s 窗口额外约 9GB。**加 `-BotHeapLimitGB` 不能救 1000×360s**（6GiB 下 500 bot 在 361s 已到 5.9GB）。要先修泄漏，再重测每 bot 常数。 |

> 口径更正：总监 cited 的「11.2MB/s 稳定增长」是 658→4320MB / 328.4s 的**全窗平均**。实测形状并不线性（OLS r²=0.71）：t≈60–167s 稳态约 12.4MB/s，t≈167–320s 在 4GiB 硬上限附近平台化（≈4233MB），期间间歇抛 `OutOfMemoryException`（11:31:49 起共 7 条），320.9s 转致命。平均 11.2 与稳态 12.4 接近，但「平台」是 cap 产物——6GiB 重放证明抬 cap 后同一档位继续涨。

---

## 1. 多轮归档并置（数据表）

窗口一律取 `mem.ndjson` 的 BotSrv 段（`mem60.ndjson` / `botsrv_ws_mb` 对 cap360b）；交付帧率取 BotSrv 侧 `load_stats.ndjson` 的 `frames_decoded`（终值/跨度），丢弃率取 `gate_stats.ndjson` 的差分（**各轮计数器口径不同，见表末列**）。

| # | 轮次（归档） | 窗口 s | in-world | WS 起→止 MB | OLS MB/s (r²) | 交付帧/s | 接收 MB/s | 丢弃帧/s（口径） |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | cap360-4G（`gate-capacity-360-20261115`） | 328.4 | 500 | 658→4320 | 8.01 (0.71) | 39,934 | 0.95 | 129,302（`drop_noncritical`） |
| 2 | cap360b-6G（`gate-capacity-360-20261115b`，本轮窗口） | 361 | 500 | 2535→5907 | ≈12.1 | —（`load_stats` 395.7s 时 `internal_errors`=4287） | — | —（护栏 5632MB 停止；`bots.err.log` 0 字节） |
| 3 | cN1（`gate-criticality-20261115-w83/tier-N1`） | 59.0 | 500 | 663→3320 | 45.45 (0.93) | 48,754 | 1.18 | 126,014（`drop_noncritical`） |
| 4 | cN4（同归档 `tier-N4`，4 网关） | 59.2 | 500 | 713→4176 | 59.97 (0.94) | 78,520 | 1.87 | 80,639（4 网关 `drop_noncritical` 合计） |
| 5 | ab1（`gate-ab-20261115-w83`，队列 4096） | 58.8 | 432 | 695→790 | 1.42 (0.65)；尾 30s −0.28 | 4,433 | 0.12 | 1,638（`overflow`） |
| 6 | ab2（`run2-frames16384`） | 58.8 | 427 | 710→842 | 1.85 (0.74)；尾 30s −0.19 | 4,717 | 0.12 | 1,913（`overflow`） |
| 7 | ab3（`run3-queue-off`） | 58.9 | 290 | 707→2691 | 34.10 (0.92)；尾 30s 14.13 | 29,193 | 0.71 | **0**（队列关；73,023 帧/s 走 `queue_disabled` 绕行，不是丢弃） |
| 8 | plateau500（`botsrv-inworld-plateau-20261115-094127/tier-500`） | 87.1 | 283 | 654→3472 | 30.70 (0.94)；尾 59s 21.86 | 33,180 | 0.80 | 69,084（`hold_frames`） |
| 9 | plateau1000（同归档 `tier-1000`） | 149.9 | 327 | 1170→3176（峰 3666） | 8.67 (0.36)；尾 59s **−8.19** | 31,931 | 0.78 | 202,425（`hold_frames`） |

**取数公式（可逐行复算）**：

- 交付帧/s = `load_stats.ndjson` 末行 `frames_decoded` ÷（末行 `uptime_ms` − 首行 `uptime_ms`）×1000；接收 MB/s 同法对 `recv_chunk_bytes` ÷1e6。
- 丢弃帧/s = 按 `gate` 分组，各取 `gate_stats.ndjson` **首末行目标列之差 ÷ BotSrv 内存窗口**（`mem.ndjson` 跨度），再对 gate 求和。目标列：cap360/cN1/cN4 用 `drop_noncritical_frames`，ab1/ab2 用 `overflow_frames`，plateau500/1000 用 `hold_frames`；ab3 的 `queue_disabled_frames` **不计为丢弃**（队列关闭 ⇒ 帧绕行照发）。
- `hold_frames` 是发送层 Hold 丢帧（oracle-faithful），与 A+B 队列的 `overflow`、关键性通道的 `drop_noncritical` 是三种不同机制，**不可跨 build 当同一物理量**；该列只用于「与增长是否同向」的粗判，自变量统一取 BotSrv 侧交付帧率。
- 本报告早前版本按 `gate_stats` 自身首末行跨度求速率（cN1 得 98,628/s）：那一段跨度 75.3s 而 bot 只跑 59s，**分母偏大**；本表统一改为「总差分 ÷ BotSrv 窗口」。

---

## 2. 拟合：增长速率 vs 交付帧率 / 丢弃帧率

自变量/因变量**都取表内数值**（cap360b 缺内存点，不入拟合，n=8）——交付帧/s 用表第 7 列，增长用表第 6 列 OLS 值：

- **对交付帧率**：`growth = -2.63 + 7.80e-4 × fps`（MB/s 与 帧/s），**r² = 0.717**，即每交付 1 帧约保留 780 B。
  - 口径敏感（同一 8 点）：cap360 改用 post-ramp 12.4MB/s ⇒ **0.756**；剔除 plateau1000（峰后回落、其 OLS r² 仅 0.36）⇒ **0.761**。区间 **0.72–0.76**，结论不随口径变。
- **对丢弃帧率**（同一 8 点）：`growth = 21.86 + 2.5e-5 × drop_fps`，**r² = 0.007**（斜率≈0，截距无意义）。

**判读（回答第一问）**：不同斜率 ⇒ 泄漏**不是**丢弃路径造成。更强的证据是「同丢弃、不同漏率」与「零丢弃、高漏率」的并置：

- cN1 丢弃 126,014/s → 45.45MB/s；cap360 丢弃 129,302/s（同量级）→ post-ramp 12.4MB/s，且后 153s 约 0。
- ab3 丢弃 0（队列关闭，交付 29,193 帧/s）→ 34.10MB/s；ab1/ab2 丢弃 1.6–1.9k/s 且交付只有 4.4–4.7k 帧/s → 1.4–1.9MB/s（近零）。
- 60s 轮次（cN1/cN4/ab3）受进世界斜坡放大（每 bot AOI/世界态一次性分配），所以其「每帧字节」偏高（≈760–1170 B/帧）——这一点是噪声来源，不影响「与丢弃无关」的结论。

---

## 3. 泄漏存在性：GC 证据与抬 cap 重放

### 3.1 Gen2「完整 GC 后存活量」单调上升（plateau500，283 in-world）

`dotnet-counters-500.csv` 的 `Gen 2 Size` 在每次 Gen2 GC（含后台 GC）之后即「本次 GC 后存活量」：

| 时刻 | Gen2 存活 | GC 停顿 | 备注 |
| --- | --- | --- | --- |
| 09:45:41 | 369 MB | 164.2 ms | 第 3 次 Gen2 GC |
| 09:46:00 | 801 MB | 208.3 ms | 第 4 次 |
| 09:46:18 | 1,108 MB | 152.7 ms | 后台 Gen2 GC（该秒 `Gen 2 GC Count` 采样为 0，但堆 2683→1468MB + 停顿 152.7ms 可证；801→1108 是这一步的存活量台阶） |
| 09:46:49 | 1,626 MB | 445.6 ms | 第 5 次 |

68s 内**完整 GC 后仍存活**的对象沿 369 → 801 → 1108 → 1626MB 单调上升（≈18.5MB/s），期间 GC 停顿均为百毫秒级（无多秒停顿、无 GC 抖动）。⇒ 这些是**被引用而存活**的对象，不是「GC 没来得及收」的垃圾。LOH 同期 96→236MB（慢），POH 恒 32,712B。

### 3.2 4GiB 的「平台」是 cap 产物；抬到 6GiB 继续涨

- 4GiB（cap360-4G）：t≈167s 起 WS 在 4233±20MB 平台化，持续到 320s；11:31:49（t≈225s）起抛 `OutOfMemoryException` 共 7 条，11:33:41（t≈337s）致命（`bots.err.log` 4 行 `Out of memory.` + `Fatal error.`）。
- 6GiB（cap360b-6G，同一二进制/同一 500 档，仅 `-BotHeapLimitGB` 不同）`env/mem60.ndjson` 的 `botsrv_ws_mb`：11:36:08 2535 → 11:37:08 3264 → 11:38:08 4015 → 11:39:08 4819 → 11:40:09 5321 → 11:42:09 **5907**（**全段 361s 平均 ≈9.34MB/s**；首段 11:36–11:38 约 12.1MB/s；`alert_bot_ws=1`，护栏阈值 5632 触发停止；`bots.err.log` 全程 0 字节，无 OOM 文本）。同期 `load_stats` 395.7s 仍 `in_world=500`。

⇒ 抬 cap 只把爆点推后，增长本身不变。这与「每 bot 常数 + 时间项」模型一致，但**时间项存在且无界**。

### 3.3 反向对照：交付帧少 ⇒ 不涨

ab1/ab2（`in_world` 432/427 但只交付 4.4–4.7k 帧/s，`tick_max` 达 8.6–13.3s）WS 58.8s 内 695→790 / 710→842MB，尾 30s 斜率负。⇒ 不是「有 bot 在线就漏」的后台定时泄漏，而是**与帧处理/移动驱动的事件速率成正比**。

---

## 4. 泄漏来源（代码级，夹具侧）

### 4.1 根因链

1. **AOI 驱逐缺守卫**：`PlayScene` tick 的驱逐条件是「`actor.DelActor` 或与自身坐标差 >16 格」（`src/BotSrv/Scenes/Scene/PlayScene.cs:206`；master 上自身引用已改名为 `self`）。Delphi 客户端同一分支还要求「距上次取消息 >2000ms」（`PlayScn.pas:1219`）。C# 侧**没有**这条空闲守卫：`m_dwLastGetMessageTime` 这个字段在 `src/BotSrv` 全目录 grep 无命中（端口没有搬过来）。
2. **驱逐即入池，永不重用**：三个入口都往 `MShare.g_FreeActorList`（静态字段，`src/BotSrv/MShare.cs:1531`）里塞 Actor——tick 的 AOI 驱逐（`src/BotSrv/Scenes/Scene/PlayScene.cs:223`）、`DeleteActor`（`src/BotSrv/Scenes/Scene/PlayScene.cs:1111`）、`DelActor`（`src/BotSrv/Scenes/Scene/PlayScene.cs:1134`）；而 `NewActor`（`src/BotSrv/Scenes/Scene/PlayScene.cs:631`）永远 `new`，**从不从池里取**。
3. **唯一的清池函数是死代码**：`MShare.g_FreeActorList` 的 60s TTL 清理写在 `src/BotSrv/Player/RobotPlayer.cs:6320` 的 `MinTimerTimer`；全仓搜索 `MinTimer` **只有这一处定义，没有任何 `Timer` 订阅或调用**。对照客户端：`MinTimer: TTimer`（`ClMain.pas:76`）、`MinTimer.Enabled := True`（`ClMain.pas:1478`）、处理函数 `ClMain.pas:13828`，TTL 后调 `TActor.Free`（`ClMain.pas:13842`–`13847`）。⇒ 端口把「定时器接线」丢了，清池逻辑成了摆设。
4. **换图也不清池**：`ClearActors`（`src/BotSrv/Scenes/Scene/PlayScene.cs:1070`）只 `ActorList.Clear()`；客户端 `ClearActors` 会 `Free` 并清空 `g_FreeActorList`（`PlayScn.pas:3457`–`3459`）。⇒ 每次 `SM_NEWMAP` 再叠加一份泄漏。

**结论**：每次 AOI 驱逐永久保留一个 Actor 对象（含其 `m_MsgList`、`RealActionMsg`、子类态与 `THumActor.SlaveObject`）。驱逐速率由移动与收帧驱动 ⇒ 增长率与交付帧率同向、与丢弃无关，且**无上界**（既有 60s TTL 从不执行）。

### 4.2 量级自洽

稳态 12.4MB/s ÷ 单 Actor 估计 1.5–3KB ≈ **4k–8k 次驱逐/s**（500 bot ⇒ 每 bot 8–16 次/s）。与「500 个 bot 挤在同一张图、开启 autoplay 来回跑图」的 AOI 边界穿越率同量级。`Actor` 实际尺寸未测（需 gcdump），此数为估计。

### 4.3 次要候选（已排除为主因）

- `ClientManager` 的无界收包通道 `_reviceQueue = Channel.CreateUnbounded<RecvicePacket>()`（`src/BotSrv/ClientManager.cs:35`）+ 单消费协程：确实是设计级无界，但 cap360 的 `frames_decoded`（13,413,964）≈ 网关 `noncritical_sent`（13,415,413，差 0.01%）⇒ 消费端没掉队，队列不是主因。仍是应在压测夹具里加界的风险点。
- `TActor.m_MsgList`：每条消息 `new TChrMsg`（`src/BotSrv/Objects/TActor.cs:298`）后 `Add` 入队（`src/BotSrv/Objects/TActor.cs:315`），`ProcMsg` 空闲时逐条排空（与客户端 `Actor.pas:2049` 同构），实测每 actor 收帧率远低于排空率；且 `ApplyDamage`（`m_StruckDamage`）在 C# 侧已注释（`src/BotSrv/Objects/TActor.cs:1516`）。

---

## 5. 真实 Delphi 客户端（冻结，只读）：有无等价累积

结论：**没有找到无界累积模式**。逐项证据（`E:/Users/gxh/Documents/GitHub/MirClient/Source/MirClient/`）：

| 关注点 | 证据 | 是否有界 |
| --- | --- | --- |
| actor 死亡是否真释放 | `PlayScn.pas:3479` `DeleteActor` → 入 `g_FreeActorList`（`PlayScn.pas:3506`、`:3520`）→ `ClMain.pas:13828` 的 `MinTimerTimer` 在 60s 后 `TActor.Free` 并从列表删除（`ClMain.pas:13842`–`13847`，`Free` 在 `:13846`）；`ClearActors` 对池与 ActorList 都 `Free`+`Clear`（`PlayScn.pas:3453`、`:3457`–`3459`） | **是**（60s TTL 缓冲，接线有效） |
| 消息队列是否有界 | `TActor.m_MsgList`（`Actor.pas:1071`），`SendMsg` `New`+`Add`（`Actor.pas:1713`、`:1733`），`GetMessage` **`Dispose` 后 `Delete`**（`Actor.pas:2023`、`:2039`），析构时把剩余项全部 `Dispose`（`Actor.pas:1698`–`1707`），`ProcMsg` 空闲排空（`Actor.pas:2049`） | 无硬上限，但逐帧排空（与 BotSrv 同构；实测未见积压） |
| 日志/聊天缓冲 | `AddChatBoardString` 硬上限 200 行：`if ChatStrs.Count > 200 then ... Delete(0)`（`DrawScrn.pas:734`–`736`） | **是** |
| 伤害飘字累积 | `StruckShowDamage`/`2` 追加（`Actor.pas:3623`、`:3631`），在绘制路径按计数删除（>32 / >38，`DrawScrn.pas:1221`–`1258`） | **是**（随绘制帧衰减） |
| 收包缓冲/队列 | `CSocketRead`（`ClMain.pas:6594`）直接 `SocStr := SocStr + data`（`ClMain.pas:6610`），主循环 `BufferStr := BufferStr + SocStr` 后同步逐帧 `DecodeMessagePacket`（`ClMain.pas:15208`、`:15222`） | 主线程同步解析，无后台无界队列 |
| 地面事件表 | `TClEventManager.AddEvent` 按 `m_nServerId` 去重（`clEvent.pas:187`、`:198`），`DelEvent*` 释放（`clEvent.pas:202`、`:214`） | **是** |

保留意见：客户端的 `m_MsgList` 与 BotSrv 一样没有硬上限——若某 actor 的收帧率长期高于其动作排空率，理论上有无界风险；但客户端每个可见 actor 每帧都会走 `ProcMsg`，实际不会触发。客户端的自由池同样是「churn 驱动大小」的缓冲，只因 TTL 接线有效而有界——**BotSrv 缺的正是这条接线**。

⇒ 该泄漏是**压测夹具（`src/BotSrv` 的 C# 端口）的端口缺陷**，与产品客户端行为无关；不需要按产品级问题上报。

---

## 6. 对 1000 档需求的影响（泄漏 vs 每 bot 常数）

- 泄漏项按 bot 计且随时间无界：500 in-world bot 稳态 ≈12.4MB/s（≈25KB/s/bot，与总监「每 bot 每秒约 22KB」同量级）。1000 bot ⇒ ≈25MB/s；360s 窗口额外 ≈9GB（还没算每 bot 的 AOI 工作集）。
- 6GiB 重放已证：500 bot 在 361s 内到 5.9GB。**抬 `-BotHeapLimitGB` 无法让 1000×360s 成立**。
- 既有「8.0–9.2MB/bot」拟合（`docs/botsrv-heap-1000bot-20261115.md`）是在泄漏未修的前提下测得的，把「时间项」摊进了每 bot 常数。修泄漏后该常数需重测（同时给 Gen2-GC 后存活量作为口径）。
- 建议顺序（不实施）：先修池生命周期 → 用 Gen2-after-GC 重测 200/500 的每 bot 常数 → 再决定 1000 档机器的堆上限。

---

## 7. 修复方向（不实施，供另卡）

1. **给池补接线**：把 `MinTimerTimer` 接到一个真实定时器（1–5s），或直接在驱逐点 `Free`/复用；最贴近 oracle 的做法是前者 + `NewActor` 优先从池取。
2. **恢复客户端空闲守卫**：驱逐条件加「距上次取消息 >2000ms」（对齐 `PlayScn.pas:1219`），避免边界 actor 反复驱逐/重建。
3. **`ClearActors` 清池**：对齐 `PlayScn.pas:3457`–`3459`。
4. （可选）给 `_reviceQueue` 设界并计数丢弃，避免下一个「无界通道」问题。
5. 修后重跑：`-Tiers 500`（≥360s）+ `dotnet-counters` 的 `Gen 2 Size`（GC 后存活量）应转为平台；并做一次 500 档静止 bot 对照（不移动 ⇒ 不驱逐 ⇒ 不涨）。

---

## 8. 证据、复现与残留

**归档（只读）**：`gate-capacity-360-20261115/`、`gate-capacity-360-20261115b/`（写入中截取）、`gate-criticality-20261115-w83/{tier-N1,tier-N4}`、`gate-ab-20261115-w83/{,run2-frames16384,run3-queue-off}`、`botsrv-inworld-plateau-20261115-094127/`（含 `dotnet-counters-500.csv`、`dotnet-counters-1000.csv`）。

**复现口径**：

- 内存：`mem.ndjson` 中 `proc == "BotSrv"` 的 `(t_ms, ws_mb)`；cap360b 用 `env/mem60.ndjson` 的 `botsrv_ws_mb`。
- 交付帧：`load_stats.ndjson` 的 `frames_decoded` 终值 / `uptime_ms` 跨度。
- 丢弃：`gate_stats.ndjson` 差分（按 `gate` 分组后求和；cap360 用 `drop_noncritical_frames`，ab 系列用 `overflow_frames`，plateau 系列用 `hold_frames`）。
- GC：`dotnet-counters-*.csv` 的 `Gen 2 Size (B)` 在 `Gen 2 GC Count > 0` 的采样点即完整 GC 后存活量。

**残留/未验证**：

- 泄漏源未用堆快照坐实（未跑 `dotnet-gcdump`；本轮 360s 窗口独占机器，不构建不起栈）。建议下一窗口用 gcdump 数 `Actor` 实例数与 `g_FreeActorList.Count` 对拍。
- `Actor` 单对象尺寸未测，§4.2 的驱逐率为估计值。
- cap360b-6G 在写入中读取（护栏 5632MB 停止，无 OOM 文本），其最终归档以窗口执行者为准。
- cN1/cN4/ab3 只有 60s，进世界斜坡主导，不宜用于估计每帧字节。
- 代码锚已按 **master `ab06da44f`** 逐一核对行号（`src/BotSrv` 行号与产生窗口的二进制 `93e0a3e7e`/`0482f715d` 不同；被分析行为在两者中相同）；客户端锚按冻结树 `E:/Users/gxh/Documents/GitHub/MirClient/Source/MirClient` 核对。
- 本报告未改任何代码、未改 `tools/**`、未改冻结客户端；未起进程、未构建。
