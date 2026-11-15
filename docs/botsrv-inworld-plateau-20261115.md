# BotSrv in-world 平台与 22.2s 单 tick：诊断报告（2026-11-15 窗口）

卡：`openmir2-botsrv-inworld-plateau`（P1，tier R，window_required）
窗口：2026-11-15 09:41–09:58（本地 +0800）
归档：`E:/UserFleet/archive/botsrv-inworld-plateau-20261115-094127/`（含 `artifacts.sha256.json`）
代码：master `ac533176f` 的 `card/botsrv-inworld-plateau` 分支（本报告为唯一代码改动：docs-only）

## 0. 结论（TL;DR）

| 问题 | 判定 | 关键证据 |
| --- | --- | --- |
| 425 平台的根因 | **服务端发送层 `ProcessBufferSend` 的 Hold 丢帧**（oracle-faithful），不是 BotSrv 单协程积压、不是 game-svr tick 饱和 | 1000 档：服务端发出 1000 次 SM_LOGON，**Send=327 / Hold=673**，bot `in_world=327`，**Send(50) 与 in_world 精确相等（差 0）**；服务端 `在世界会话` 到 1000 |
| 22.2s 单 tick 的根因 | **BotSrv 退出导致 1000 会话同刻断开 → 离世广播扇出逐对打 WRN 的日志放大**；与 425 平台**不同根**。**非稳态触发，但真实场景可达：大规模断线时服务端离世广播风暴**（网关重启/网络抖动/维护同样会造成成百上千连接同时断开），不是「夹具才有」，只是夹具更容易触发 | 1000 档档尾：单 tick max **22,646.491ms**；同 tick「离世广播」WRN **232,667 行 / 24s / 中位 10,479 行/s** ⇒ 预测 22.22s（差 2%）；`HostPeriodic` 7,399.98ms/tick，`HostDbPoll` 0.089、`HostInbound` 163.8 |
| 是否为 GC / 锁 / 消费者堵死 | **都不是** | BotSrv `dotnet-counters`：单秒 GC 停顿 max 301.8ms（无多秒停顿）、gen2 仅 3 次、ThreadPool 队列恒 0、CPU max 23%；game-svr 是 Rust（无 GC），22.6s 期间时间落在 HostPeriodic 的离世下行 |
| A5「容量墙是夹具还是服务端」 | **服务端发送策略（协议层）+ 夹具同图拓扑**；不是夹具 CPU/内存 | 服务端在 bot 只进 327 时仍保持 1000 在世界；gate 侧累计 Hold 30,352,774 帧（top ident 41/10）；全假人同一张图 ⇒ AOI 出站 O(N²) |
| A0 空闲在线成本 | **未采集**（按总监裁决：正式 A0 属平台修复后第一窗口；本轮优先释放构建） | 卡面 comment（总监 2026-11-15 裁决） |

## 1. 判据与实测（判据在起栈前已写死，见卡面「阶段1设计」comment）

| ID | 假设 | 实测 | 判定 |
| --- | --- | --- | --- |
| H1 | BotSrv 单协程消费积压 | 500 档 217 个缺失者**连 welcome 帧都没收到**（500 进入 → 366 welcome → 283 SM_LOGON）；消费端 `recv_chunks/frames_decoded` 稳定、`frames_decoded/recv_chunks`≈1.07–1.09；无界队列若有积压，SM_LOGON 只会迟到、终将出队，但 90–150s 内 in_world 零上涨 | **证伪** |
| H2 | game-svr tick 饱和 | 服务端 `在世界会话` 1000 时 `工作段合计` 仅 75–89ms/tick（预算 200ms），`HostInbound` 30–41ms；1000 档 BotSrv tick p99=32ms | **证伪** |
| H3 | 发送层背压 Hold 丢弃 SM_LOGON | 服务端逐条记录：`ident=50` 发送 1000 次，**Send=327 / Hold=673**；bot 侧 `in_world=327`、`ident_counts[50]=327`；**Send(50) == in_world（整数相等，容差 0）**；服务端 in-world=1000 | **确认** |
| H4 | bot 侧丢点（partial / 全局 MapMovingWait / 身份守卫） | `logon_misdelivered=0`；`partial_frames=2000`（1000 档，字节级小片段，远小于 673 条缺失）；缺失者没有 welcome 帧 ⇒ 不是「收到 SM_LOGON 后处理异常」；consumer NRE 爆发（10,462/s）与缺失集合无对应 | **证伪** |
| H5 | 22.2s = 批量断开的离世广播日志放大 | 1000 档：WRN 232,667 / 24s / 中位 10,479 行/s ⇒ 预测 22.22s，实测 tick max 22,646.491ms；500 档：WRN 54,573 / 6.3s / 中位 10,347 行/s ⇒ 预测 5.27s，实测 5,557.128ms。两档均 `HostPeriodic` 占满、`DbPoll`≈0 | **确认（两档各一次）** |
| H6 | GC / 锁竞争 / 消费者串行堵死 | BotSrv：`Time paused by GC` max 301.8ms/秒（无多秒停顿）、`Gen 2 GC Count` 3 次、`ThreadPool Queue Length` 恒 0、`Monitor Lock Contention` max 326/s；game-svr：无 GC，`HostPeriodic` 与 WRN 速率匹配 | **证伪** |

## 2. 500 档（`-HoldSec 90`，单跑干净栈）

- 登录 500/500（100%）；服务端 `在世界会话` 在 01:45:50 到 500；bot `in_world` 最终 **283**（ident50=283）。
- bot 日志：`准备进入游戏` 500、`进入游戏` 500、`欢迎进入` **366**、`SM_LOGON` **283**、`成功进入游戏` 283。
- gate `gate_stats.ndjson` 终值：`hold_frames=6,019,179`、`hold_bytes=225,859,607`、top idents `41:2,984,335,10:2,983,778,657:20,913,…`。
- 服务端 TICKPROF：平台期 `工作段合计` 45–52ms/tick、`HostInbound` 24–31ms；档尾断线风暴单 tick max **5,557.128ms**。
- 档尾 gate 日志：**500 条「断开链接」全在同一秒**（01:46:53），随后离世广播 WRN **54,573 行 / 6.3s**（中位 10,347 行/s）。
- BotSrv：WS 峰 3472.2MB、priv 峰 3615.8MB；`dotnet-counters` GC 单秒停顿 max 445.6ms、gen2 4 次、ThreadPool 队列 ≤1；无 OOM。
- `report.json` 显示 GREEN 只是因为判据集仍为 `login,conn,tick`（不含 in-world）；**本报告不把它当验收结论**。

## 3. 1000 档（`-HoldSec 150`，`-BotHeapLimitGB 6`，单跑干净栈）

- 登录 1000/1000（100%）；服务端 `在世界会话` 从 29（01:49:12）→1000（01:50:22）；bot `in_world` 最终 **327**。
- 服务端逐条发送记录（`MIR2_LOG_MISDELIVER=1` 只对 game-svr 开启，gate 未开启）：**1000 次 `ident=50`，Send=327、Hold=673，distinct Send 会话=327**。
- bot 日志：`准备进入游戏` 1000、`欢迎进入` 560、`SM_LOGON` 327、`成功进入游戏` 327、consumer NRE 10,582 行。
- gate `gate_stats.ndjson` 终值：`hold_frames=30,352,774`、`hold_bytes=1,138,908,867`、top idents `41:15,047,739,10:15,046,464,…`。
- 服务端 TICKPROF：in-world 到 1000 时 `工作段合计` 81.38ms/tick、`HostInbound` 29.127ms；平台期 75–89ms/tick；**无饱和**。
- 档尾：BotSrv 被 load_gate 正常停止（不是 OOM；`bots.err.log` 空），gate 侧 1000 会话同刻断开，单 tick max **22,646.491ms**；同 tick WRN **232,667 行 / 24s / 中位 10,479 行/s**。
- BotSrv：WS 峰 3665.9MB、priv 峰 3813.6MB、末段斜率 -8.2MB/s（6GiB 堆上限下 GC 已介入）；`dotnet-counters` GC 单秒停顿 max 301.8ms、gen2 3 次、ThreadPool 队列恒 0、CPU max 23%。
- **未 OOM**：总监预期的「6GB 下 1000 bot 可能 ~600–700 OOM」未发生，因为 bot 侧只进了 327，实际堆未触顶；本轮价值是**更高会话数（服务端 1000 in-world）下的平台与断线行为**。

## 4. 机制解释

### 4.1 平台（发送层 Hold）

`game-svr` 的 `send_decision`/`send_gate_msg` 逐条照搬 oracle `ProcessBufferSend`：`SendChecked > 0` 时**无条件 return（Hold）**，直到 gate 回 `GM_RECEIVE_OK` 或 50ms 超时；帧不排队、不重试。Hold 是**每条 gate 线全局**的：一条线上所有会话的帧一起被压掉。SM_LOGON 是一次性控制帧，被 Hold 后没有补发路径，因此该 bot 永远停在“准备进入游戏”，而服务端已把它计入 `在世界会话` 并继续给它发 AOI 帧。

夹具同图放大：全部假人落在同一张图（map 26），每个 bot 可见 ~500 人，出站帧量 O(N²)（ident 41/10 各占 Hold 的绝大多数）；N 越大，`SendChecked>0` 越常成立，丢 SM_LOGON 的概率越高 ⇒ 形成 ~280–430 的**平衡点**，与档位无关。这解释了 200 档只丢 2、500 档丢 96–217、1000 档丢 575–673 的非线性，也解释了为什么平台值与档位无关而随运行/拓扑波动。

**这是 oracle-faithful 行为**（低负载正确、高负载错误的同类，见 D-18）。修复选项（排队/拒绝/降级、内存上界）已由 `openmir2-gateway-backpressure-frame-drop` 的 A2 comment 交总监裁决；本卡不改策略。

### 4.2 22.2s 单 tick（断线风暴的日志放大）

`drain_depart_frames`（`mir2-rs/bins/game-svr/src/lib.rs`）对 `world.drain_departs()` 里的每个 `(observer, departed)` 对查 `online`；观察者已不在线时**逐对打一条 WRN**。1000 会话同刻断开时，一个 tick 内产生 232,667 对，同步日志写入速率 ~10.5k 行/s ⇒ tick 被拖到 22.6s。同一函数是离世广播的**生产接缝**（帧面本身正确），所以修复方向应是「同 tick 聚合/限速 WRN + 给扇出加时间预算」，不能删掉广播或改变帧面。

**定性边界（总监 2026-11-15 措辞更正）**：这是**「非稳态触发，但真实场景可达：大规模断线时服务端离世广播风暴」**——网关重启、网络抖动、维护都会造成成百上千连接同时断开，此时同样会产生每 tick 十万量级的离世广播；它不是「夹具才有」，只是「夹具更容易触发」。**「22.2s 不代表 1000 档稳态性能，但代表大规模断线时可能出现的尖峰」**——两件事必须分开说，避免拿 22.2s 说「性能不行」或拿稳态 32ms 说「没问题」。修复已单独立卡 `openmir2-disconnect-broadcast-storm`（P2：批量/限速 + 每分钟 N 条上限），本报告不展开方案。

**不是**：GC（Rust 无 GC；game-svr WS 峰 558MB）、锁竞争（时间在 HostPeriodic 的逐对日志，不在锁等待）、BotSrv 消费者（发生在 game-svr tick 内）。该 WRN 是 Rust 侧诊断语句，oracle 对同情形静默跳过 ⇒ 这是 **Rust 诊断路径的性能缺陷**，不是 oracle 行为差异。

## 5. 对 10k 外推的含义（A5）

- 「10k 在线、活跃并发 ≤0.9k」的假设**不含进世界洪峰**：即使稳态活跃 ≤0.9k，登录/进世界阶段仍会瞬时产生大量 AOI 与登录帧，触发上述 Hold 丢帧。若不改发送策略，也不做入场上限/错峰，本栈的 in-world 天花板仍是 **~300–430**，10k 在线无法达到。
- 该墙**不是**机器算力（服务端 1000 in-world 时 <90ms/tick）也**不是**夹具内存（1000 档 BotSrv WS 3.67GB）。它是**协议层的背压丢帧策略 + 同图拓扑**。
- 因此「10k 在线」的外推必须二选一：① 按 A2 推荐实现 per-gate 有界排队（有意偏离 oracle，需 D-* 裁决，内存上界 256KiB/gate）；或 ② 入场分批/分图，使瞬时扇出低于触发阈值（仍需给出阈值实测）。

## 6. 缺口、残留物与复现

**缺口**
1. 500 档未开 `MIR2_LOG_MISDELIVER`（该 env 在 500 档跑完后才加到 game-svr），所以 500 档没有 Send(50)/Hold(50) 的逐条计数；H3 以 1000 档的精确等号为准。
2. `tools/botload/load_gate.ps1` 的 artifact manifest 步骤调用 `Get-FileHash` 在本机 PowerShell 不可用 ⇒ 两档 rc=1（`report.json`/`load_stats`/`mem.ndjson` 均已完整落盘；不是数据缺失）。另开卡修。
3. `memwatch.ps1` 在 09:48:47 后停写（进程仍在，原因未查；未影响 3GB 护栏判断——期间手工采样 free 8.3–8.7GB、BotSrv WS ≤2.4GB，均高于阈值）。
4. 每档只跑一次；平台值 283（500 档）/327（1000 档）与归档 404/425 不同，属平衡点对拓扑/时序的敏感性，不改变机制判定。
5. A0 空闲在线未采集（见 §0）。

**证据四件套**
- 命令：`tools/botload/load_gate.ps1 -Tiers 500 -HoldSec 90`、`-Tiers 1000 -HoldSec 150`（`-SkipBuild -MemKillThresholdGB 8 -BotHeapLimitGB 6`）；栈由 `E:/UserTemp/wt82-plateau/start_rust.ps1` 起，`MIR2_TICK_PROFILE=1`、`MIR2_GATE_STATS_OUT=<out>/gate_stats`、1000 档对 game-svr 额外 `MIR2_LOG_MISDELIVER=1`。
- 证据路径：`<archive>/tier-500/`、`<archive>/tier-1000/`、`<archive>/stack-logs-run500/`、`<archive>/stack-logs/`、`<archive>/gate_stats-run500-moved/`、`<archive>/gate_stats/`、`<archive>/dotnet-counters-500.csv`、`<archive>/dotnet-counters-1000.csv`、`<archive>/mem-watch.csv`。
- hash：`<archive>/artifacts.sha256.json`（60 个文件）。窗口构建：game-svr.exe `5b9788ef…`（master + T-281 可观测）；BotSrv.exe `bf8f417c…`（无代码变更，复用）。
- 复现要点：起栈 → 跑档 → 档尾读 game-svr 的 `[TICKPROF]` 逐 tick 分位与「离世广播」WRN、读 `gate_stats.ndjson` 的 Hold 终值、读 bot `load_stats.ndjson` 的 `in_world/ident_counts["50"]`。

**收尾**
- 端口 5000/7000/7100/7200/6000/5100/5700/5500/5600 全 DOWN；game-svr/gates/BotSrv/DBSrv/LoginSrv 无进程；mysqld 23852（11-08 起）未动。
- 窗口期无并发 cargo（构建在跑批前完成；跑批期间未构建）。
- 一次性分析脚本在 `E:/UserTemp/wt82-plateau/`（`analyze.py`/`analyze2.py`/启动与护栏脚本），未入库；归档目录之外无需长期保留。
