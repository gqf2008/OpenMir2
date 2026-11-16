# 进世界平台（③）串行点梳理与诊断探针轮设计（2026-11-16）

> **本文状态：分析件（只读码 + 归档复读）** —— **未跑任何窗口、未起栈、未改代码**。
> 用途 = 给 `openmir2-world-entry-serial-saves`（A1）提供「玩家进入世界 → BotSrv `in_world` 计数」
> 全链路的串行点清单，并设计一个**只用于定位串行点**的诊断探针轮。**本文任何数字都不是容量结论。**

> **锚的形式**：本仓 `docs_anchor_gate.py` 对高 churn 目录（`bins/game-svr/**`、`crates/world-bevy/**`）
> **禁止行号锚**（`HIGH_CHURN_LINE_ANCHOR`），因此本文一律用 **路径 + 符号** 形式
> （`` `path` 的 `symbol` ``），用 grep 搜符号即可定位；不写 `path:line`。

## 0. 结论先读

**主判断（分级）：③ 是【已证实】的负载来源与放大器；① 是【最可能假设】的直接丢帧点。**

**③ 的量化佐证（硬数据）**：plateau 末行 `top_idents` 里 `41:15,047,739 + 10:15,046,464 = 30,094,203`，占 `hold_frames=30,352,774` 的 **99.15%**（top-5 = 99.82%，`SM_LOGON(50)` **不在 top-5**）⇒ **占满发送口的绝大多数是 AOI 帧**，这把「③ 作为放大器」从假设推向硬数据。
现有证据只到「丢失发生在 S1 之后、S2/S3/S4 未分」；在探针给出 S2 的 SM_LOGON `decision` 分布
与 S3 的 `MISDELIVER_DELIVER ident=50` 数之前，**不把 ① 写成已证实**。

1. **【硬】丢失发生在 S1 之后**。两个 1000 档归档里，
   `进世界 ChrName` / `进世界帧下发` / `进世界标记` 各 **1000 次**，`收到角色数据` **1000 次**，
   `全字段解析失败` **0 次**，「槽不存在，丢弃」 2 次（`supp-hold360`）/ 8 次（`plateau-094127`），
   且每批都是 `共 15 条`（10 个批次 16 条）。而 BotSrv 侧只解出 **425 / 327** 个 `SM_LOGON`。
   ⇒ 丢包在 **game-svr 进世界段（S1）之后**；S2/S3/S4 三段**现在分不开**。
   **口径限定**：`进世界帧下发` 是「入队意图」（`SimToGate::SendFrame` 之后的那条日志），
   不是线上已发；`进世界标记` 只表示槽表存在。
2. **【硬】BotSrv 的单消费者确实饱和了，且 AOI 是 O(N²)**。`plateau-094127` 末值
   `frames_decoded=5,442,742`，其中 `SM_TURN(10)=2,599,152` + `SM_FEATURECHANGED(41)=2,598,875`
   = **95.5%**；解码速率在 30–47k 帧/s 饱和（90s 后 27–38k）；`in_world` 增速与已进世界人口
   **总体下行**（12.4 → 10.4 → 4.1 → 0.9 → 2.9 → 1.0 → 0.2 → 0.8 → 0 个/s；中间
   0.90→2.90、0.20→0.80 两次回升，不是严格单调，见 §2.3 表）。
   `supp-hold360` 同形（`10=4,866,822` / `41=4,866,217`，97.3%）。
3. **【假设】① 发送口流控是最可能的直接丢帧点**，但支撑只有两条间接证据：
   (a) pre-patch 语义下 `SendChecked>0` 的帧是被**丢弃**（不是排队）；
   (b) 同窗 `gate_stats` 的 `hold_frames=30,352,774`（旧 schema 的键是
   `hold_frames/hold_bytes/oversize_frames/oversize_bytes/overflow_idents/overflow_sessions`，
   **没有 `overflow_frames`**；`hold_frames` 是**全帧** hold，不是 SM_LOGON 计数；末行
   `oversize_frames=0`、`overflow_idents=0`、`overflow_sessions=28,811,584` 是**直方图键**
   溢出，不是帧溢出）。`gate_backpressure.rs` 模块头另记了一轮 03:00 窗口的
   「716 条 SM_LOGON → 330 Send / 386 Hold」，那是**另一轮**观测、**不在本归档里**，只能作旁证。
   把 ① 变成事实还差两个数：S2 的 SM_LOGON `decision` 分布、S3 的 `MISDELIVER_DELIVER ident=50`。

**关键限定：以上归档都在队列补丁（`openmir2-gateway-backpressure-frame-drop` +
`openmir2-gateway-frame-criticality-queue`）之前。** 现 master 上 `Hold` 已改为「关键帧入有界队列」，
500 档也因此从 404 平台升到 500/500；**1000 档在补丁树上没有实测**，所以 ① 是否已被消掉、
③ 是否会成为新树上的唯一上限，**现在无法判定**——这就是探针轮要回答的问题。

**最大不确定性**：现有证据只排除了「S1 之前丢」与「网关**会话路由**」两项
（`会话[..]不存在，丢弃下行`：`supp-hold360` = 0 条；`plateau-094127` 1803 条全在窗口末
teardown，首条与首条 `断开链接` 同一毫秒）。**没有**排除：
(i) S2 发送口队列/回显流控、(ii) S3 网关的**单读循环**（W1；本轮 BotSrv 解码 5.4M–10.0M 帧，网关读循环的帧量级相当或更高）、
(iii) S4 BotSrv 单消费者积压、(iv) S4 BotSrv 组帧解析丢弃（`ProcessPacket` 对跨 read 的残帧不缓存）。
⇒ 探针的第一目标就是把 S2/S3/S4 分开。

**探针轮预计成本**：2 臂 × 1000 档 × 180s，单窗口约 **20–25 分钟**（含整栈起停），
**零代码改动**（全部走既有环境变量探针），不产出任何容量/验收结论。

---

## 1. 全链路与串行点清单

链路四跳（一跳一个进程边界）：

```
BotSrv 发送 CM_SELCHR / **登录串
   └→ game-gate(客户端面) ── GM_OPEN / GM_DATA ──→ game-svr
                                                    ├ 读档（DBSvr 往返，v2 实测 14.4/s，已排除）
                                                    ├ 进图 + 装配 15 帧（含 50 SM_LOGON）
                                                    └→ 发送口（每 gate 线一个 KeepAlive）
                                                         └── GM_DATA ──→ game-gate(引擎面)
                                                              └── 按 SessionId 投递 ──→ BotSrv 连接
                                                                   └── 全局单消费者队列 → MarkInWorld → in_world
```

### 1.1 串行点表

类别代号：**① 真串行**（单线程 / 一进一出）｜**② 限速**（每 tick / 每秒 / 字节预算上限）｜
**③ 排队但可并行**（消费者不足，队列只是变长）。

| ID | 跳 | 位置（路径 + 符号） | 为什么可能是串行点 | 类 | 可观测代理 |
| --- | --- | --- | --- | --- | --- |
| G1 | game-svr | `mir2-rs/bins/game-svr/src/lib.rs` 的主 tick 循环（`host_prof.begin` / `TICK_INTERVAL_MS`，200ms 一拍） | **整个世界只有一个线程**：入站命令、读档发起、DB 应答消费、进图、AOI、发包全在同一拍里顺序执行；一拍超时后续全部顺延 | ① | `MIR2_TICK_PROFILE=1` 的段耗时日志；`sim: tick #N` 日志 |
| G2 | game-svr | 同文件 DB「收侧轮询」块（`w.outstanding` + `r.take(qid)`） | 每拍把**所有已到达的应答一次处理完**，逐个做 parse → RNG → `enter_map` → 15 帧装配；**没有每拍上限**，1000 个应答同拍到达即同拍串行做完 | ①/③ | `收到角色数据 queryId=…` INF 行的时间分布；该块所在的 host 段计时 |
| G3 | game-svr | 同文件 `进世界帧下发` 日志**之前**的 `for body in bodies { tx.send(SimToGate::SendFrame …) }` + `MarkInWorld`（日志在 `tx.send` 循环**之前**打印，是入队意图不是已上线） | 15 帧 × 1000 bot 的入队是**单线程顺序**，但目标是无界 mpsc，不阻塞。**注意**：`进世界帧下发` 是「入队意图」日志，不代表已上线 | ③ | `进世界帧下发：socket=… 共 N 条`、`进世界标记：socket=…`（各 1 行/bot） |
| G4 | game-svr | 同文件 `send_gate_msg` / `send_decision` / `send_direct` / `flush_pending` / `queue_or_reject`；`mir2-rs/bins/game-svr/src/gate_criticality.rs` 的 `channel_of`；`mir2-rs/bins/game-svr/src/gate_queue.rs` 的 `PendingQueue` | **每条 gate 线只有 1 个 `SendChecked` 信用**：撞 `CheckBlock*10` 字节即发 `GM_RECEIVE_OK` 并停发，等网关回显或 50ms 超时；pre-patch 树「等回显期间 = 丢帧」，现树「关键帧入 256KiB/4096 帧队列，非关键帧丢弃」 | **①+②**（一进一出 + 字节预算） | `gate_stats.ndjson`：`queued/flushed/overflow/reject_login/degrade_inworld/close_inworld/max_depth`；`event=gate_frame_queued\|overflow\|degrade`；`MIR2_LOG_MISDELIVER=1` 的 `MISDELIVER_SRV event=send … decision=…` |
| G5 | game-svr | `mir2-rs/bins/game-svr/src/dbclient.rs` 的 `run_db_link`（单写任务）+ `DbReplies::feed`（单读任务） | DB 写是单任务顺序 `write_all`，但请求是小帧且已实测 14.4/s，**不是瓶颈** | ①（证据排除） | `执行任务Id:{qid}成功` 速率；`收到角色数据` |
| G6 | game-svr | `lib.rs` 的 `gate_conn`（每条 gate 线一个 tokio 任务）+ `apply_sim_cmd_async` | 同一条 gate 线的所有会话帧在**一个任务里逐帧 `await`**（写 socket + 流控）；`sim_rx` 一次只取一条 | ①（单线） | `WRN`「下发 SM_* 帧时 socket=… 槽不存在」；队列深度 |
| W1 | game-gate | `mir2-rs/bins/game-gate/src/lib.rs` 的 `reader` → `handle_server_message` → `deliver_gm_data` | **引擎→网关只有一条 TCP 链路，一个读循环逐帧分派**；每帧取两把 async 锁（`route_owner`、`sessions`）。10M+ 帧/窗口时这是热路径 | ①（单线） | 网关日志时间戳密度；`MISDELIVER_DELIVER … send_ok=` |
| W2 | game-gate | 同文件 `client_loop` 的 `writer` 任务 + `sessions` 里的 `UnboundedSender` | **每会话一个写任务 + 无界通道**：不会阻塞投递，但客户端不读时内存无界增长 | ③ | 无直接计数（探针靠 `send_ok` + 客户端侧证据） |
| W3 | game-gate | 同文件 `register_session` / `SessionSlots::allocate`（`会话 SessionId 撞号` 分支） | 4 位随机 `SessionId` 撞号时改分配；撞号会把「服务端寻址的 sid」与「客户端连接」错配 | ①（罕见） | `WRN 会话 SessionId 撞号：candidate=N ⇒ 改用空闲值 M`（本轮 54 次） |
| B1 | BotSrv | `src/BotSrv/ClientManager.cs` 的 `ProcessReviceMessage`（**全部 bot 共用一个** `Channel<RecvicePacket>`，`TryRead` 一次处理一个） | **整进程只有一个收包消费者**，处理一个包才取下一个；这是 `in_world` 计数的必经之路 | **①**（单消费者） | `load_stats.ndjson` 的 `recv_chunks`、`frames_decoded`（速率） |
| B2 | BotSrv | `src/BotSrv/SocketShim.cs` 的 `ScoketClient.RecvLoop` | 每连接一条 OS 线程（1000 bot ≈ 3000 条连接 ⇒ 3000 线程），只做 read + 字符串化 + 入队 | ③（线程数，不是串行） | `recv_chunks` / `recv_chunk_bytes` |
| B3 | BotSrv | `src/BotSrv/Player/RobotPlayer.Operate.cs` 的 `ProcessPacket`；`src/BotSrv/Player/RobotPlayer.cs` 的 `ReceivedData` | **组帧不跨 read 缓存**：`!` 缺失即 `PartialFrame` 并丢弃整段残帧；`MShare.MapMovingWait` 为真时直接 `break` 丢弃本块剩余帧；`ReceivedData` 遇到 `"*"` 有截断改写分支 | ②（丢帧，不是限速） | `partial_frames` / `partial_frame_bytes` / `chunks_without_hash`；`internal_errors` |
| B4 | BotSrv | `src/BotSrv/ClientManager.cs` 的 `Run`（20ms 预算遍历全部 bot）+ `RunAutoPlay`（每 bot 800ms 门） | **一个 run 线程**顺序跑全部 bot 的挂机/走位，并驱动 CM_TURN 产生 AOI 扇出 | ①（单线程）/ ③ | `tick_p50/p90/p99_ms`、`tick_samples`、`probe_sent`、`autoplay_in_world` |
| B5 | BotSrv | `src/BotSrv/LoadMetrics.cs` 的 `InWorld` / `IdentSeen`（全局 `lock (Sync)`）；`src/BotSrv/MShare.cs` 的 `MySelf`（`[ThreadStatic]`）与 `ConnectionStep` / `RunServerAddr` / `MapMovingWait` / `g_boMapMoving`（仍是**跨 bot 全局**） | 全局锁本身很轻；真正的风险是全局**语义态**：`MapMovingWait`/`g_boMapMoving` 为真时所有 bot 的包都被 `return` 拦下 | ①/正确性风险 | `WRN`/`internal_errors`；bots.log 中同批 bot 同时无响应 |

### 1.2 明确**不是**串行点的（本轮证据排除）

- **DB 往返/读档速率**：v2 前 1000 条 `执行任务Id:N成功` 在 69.3s 内完成（≈14.4/s），同轮
  `in_world` 仍 425 ⇒ 平台在 DB 应答之后（见 `docs/1000-tier-prereq-inventory-20261116.md` §3.1）。
- **网关会话路由**：`supp-hold360` 的 `会话[..]不存在，丢弃下行` = **0** 条；
  `plateau-094127` 的 1803 条全部落在窗口末 teardown（首条 01:52:03.284 与首条 `断开链接`
  01:52:03.282 同批）。
- **game-svr 逐帧发送槽**：`槽不存在，丢弃` 只有 2/8 条（1000 bot）。

---

## 2. 三类候选：判据、证据与排序

### 2.1 三类怎么用数据区分

| 类 | 定义 | 数据特征（判据） | 当前证据指向 |
| --- | --- | --- | --- |
| **① 真串行** | 单线程或「一进一出」信用；吞吐 = 单位作业时间 × 并发度 | 速率**与人口无关**、恒定；队列深度低或稳定；控制臂（去掉 AOI）**速率不变**；发送口 `decision=Hold` 与 `SendChecked` 回显间隔直接成比例 | **候选（未证实）**：game-svr 发送口（G4）一轮只有 1 个未回显信用 + 50ms 超时；决定性数据 = S2 的 SM_LOGON `decision` 分布 |
| **② 限速** | 显式每 tick / 每秒 / 字节上限 | 速率是**整数台阶**且与负载无关；去掉负载后仍是同一台阶；应有可读配置项/常量 | 找不到「每 tick N 条」的常量（`drain_read_storage` 无上限、tick 循环无上限）；只有 G4 的字节预算属于 ② |
| **③ 排队但可并行** | 消费者吞吐有限，队列只变长；作业本身可并行 | 吞吐**饱和在某个固定值**；队列/内存单调增长；速率随已进世界人口**总体下行**；控制臂（去掉 AOI）显著改善 | **已证实为负载来源**：BotSrv 单消费者（B1）+ O(N²) AOI 扇出；`frames_decoded` 平台 30–47k/s（后段 27–38k），`in_world` 增速总体下行到 0。是否为**唯一**上限仍待探针 |

### 2.2 归档复读（1000 档，两次长窗口）

| 量 | `supp-hold360`（360s 窗，OOM 前） | `plateau-094127`（150s 窗） |
| --- | --- | --- |
| game-svr `进世界 ChrName` / `进世界帧下发` / `进世界标记` | 1000 / 1000 / 1000 | 1000 / 1000 / 1000 |
| game-svr `收到角色数据` / `全字段解析失败` / `槽不存在` | 1000 / 0 / 2 | 1000 / 0 / 8 |
| 每批帧数 | 990×15 + 10×16 | 同形 |
| BotSrv `准备进入游戏` / `机器人:[x] 进入游戏`（SendRunLogin） | 1000 / 1000 | 1000 / 1000（**重连 0**；「进入游戏」子串共 2327 = 准备+进入+成功） |
| BotSrv `SM_LOGON actor=` / `成功进入游戏` | 425 / 425 | 327 / 327 |
| BotSrv `in_world` 末值 | 425 | 327 |
| `frames_decoded` 末值 | 9,997,424 | 5,442,742 |
| `SM_TURN(10)` + `SM_FEATURECHANGED(41)` 占比 | 4,866,822 + 4,866,217 = **97.3%** | 2,599,152 + 2,598,875 = **95.5%** |
| game-svr `hold_frames` 末值（**全帧**，非 SM_LOGON） | 该轮未落盘 | **30,352,774**；同行为 `oversize_frames=0`、`overflow_idents=0`、`overflow_sessions=28,811,584`（**直方图键**溢出） |
| 网关 `会话[..]不存在，丢弃下行` | **0** | 1803（全在 teardown） |

⇒ **S1（game-svr 进世界 1000）→ S4（bot 计入 327/425）之间有 575/673 帧丢失**；
这是**跨 S2/S3/S4 的合并结论**，不是「发送口丢了 575」——那需要探针的分段计数。
「流失与已进世界人口正相关」目前只是与第 2.3 节表的**同向共现**，不能当因果（缺反例组）。

### 2.3 `in_world` 增速与人口（`plateau-094127`，10s 采样点，相邻采样差分）

| uptime | in_world | 增速（个/s） | frames_decoded 速率（帧/s） |
| --- | --- | --- | --- |
| 10s | 124 | 12.40 | 4.4k |
| 20s | 228 | 10.37 | 25.1k |
| 30s | 269 | 4.09 | 31.6k |
| 40s | 278 | 0.90 | 35.6k |
| 50s | 307 | 2.90 | 40.1k |
| 60s | 317 | 1.00 | 46.7k |
| 70s | 319 | 0.20 | 44.8k |
| 80s | 327 | 0.80 | 32.9k |
| 90s+ | 327 | **0.00** | 27–38k（持续到 170s） |

⇒ 这张表能支撑 **③**（消费者饱和 + AOI 占绝对多数 + 增速总体下行）；
它**不能**单独支撑 ①——「SM_LOGON 撞上 `SendChecked>0` 窗口的概率随 AOI 上升」只是机制上的
合理推想，需要 S2 的逐帧 `decision` 计数才能变成观测。增速两次回升（0.90→2.90、0.20→0.80）
也说明「随人口单调衰减」的写法不成立，只能写**总体下行**。

### 2.4 主判断与依据

**我判：③ 是已证实的负载来源；① 是最可能的直接丢帧点（假设，待探针）；② 不成立。**

1. **③（B1 单消费者 + O(N²) AOI）＝ 已证实的负载来源与放大器。** AOI 占解码帧 95%+，
   `frames_decoded` 饱和在 30–47k/s（后段 27–38k），`in_world` 增速总体下行到 0。
   这部分不需要新窗口就能成立。
2. **①（G4 发送口流控）＝ 最可能的直接丢帧点，但证据只到「合理」。** 支撑是 pre-patch 的
   Hold=丢弃语义与同窗 30.35M 的**全帧** hold 量；它**不能**证明其中包含多少个 SM_LOGON。
   旁证「716→330/386」来自另一轮窗口，不属本归档。**决定性数据尚未采集**：S2 的 SM_LOGON
   `decision` 分布、S3 的 `MISDELIVER_DELIVER ident=50`。
3. **② 基本不成立**——没有找到「每 tick / 每秒 N 条」的常量；唯一的上限是 G4 的字节预算，
   它只是 ① 的判定输入，不是独立的台限。**「5.3/s 串行」是观测值，不是代码里的限速。**
4. **丢帧跳的排序给不出**：现有数据只到「S1 之后」。若探针在 B 臂（无 AOI）下仍平台，
   则 ①/③ 都不足以解释，要转向 S3 单读循环或 S4 组帧解析。

---

### 2.5 对照 oracle（A2 的结构层答案；速率层未测）

- **结构层：① 是两栈共享的限制，不是 Rust 移植引入的。**
  `mir2-rs/bins/game-svr/src/lib.rs` 的 `send_decision` / `send_gate_msg` 是 C# 参照实现
  `ChannelMessageHandler.ProcessBufferSend` 的**逐行移植**：`SendChecked > 0` 时无条件 `return`
  （丢帧）、单包超 `CheckBlock*10` 且累计为 0 时直接丢、发送前撞阈值先发 `GM_RECEIVE_OK`。
  该口径在 `mir2-rs/bins/game-svr/src/gate_backpressure.rs` 模块头已逐条登记（含 C# 行号引用）。
  ⇒ 若探针证实 ① 仍在新树丢帧，则**同一压力在 oracle 上也会丢**，「行为等价」要求我们先把
  它的规模量出来，再决定是修 Rust 还是改窗口设计。
- **速率层：oracle 的 1000 档进世界速率本轮没有数据。** 既有归档的 1000 档都是 Rust 栈
  （C# 侧只有 DBSrv/LoginSrv 日志），没有 C# game-svr + 1000 bot 的 `in_world` 曲线。
  因此「C# 是否同样卡在 327/425」**未测**，属 A2 的剩余项，需要窗口（本卡不做）。
- **ramp 公式的形式**（给 A4 用，数值待探针标定）：若能测出发送口每批字节 `B`
  与回显 RTT `R`（或 50ms 超时），则一条 gate 线的关键帧吞吐上限约为 `B / R` 帧预算/秒；
  1000 个进世界所需字节 = `1000 × 15帧 × 平均帧长`。**ramp 时间应按这个信用模型算，
  而不是把观测到的 5.3/s 当成常数**——前者随 AOI 帧量变化，后者只是某一轮的症候值。

## 3. 诊断探针轮设计（**不跑，先报批**）

### 3.1 目的（一句话）

**定位 `SM_LOGON` 在四跳中的哪一跳丢失、以及能否由类别 ①/②/③ 解释**；
**不是**验 `in_world ≥ 95%`，**不产出任何容量/验收结论**。

### 3.2 合规声明（对照 `openmir2-load-1000-rerun-prereq` 的唯一例外三条）

1. **开跑前登记目的与判据**：本文即登记件；开跑前在**禁令卡 `openmir2-load-1000-rerun-prereq`**
   签一条 `comment` 引用本文 §3——该卡的 `diagnostic_exception` 是「不违反禁令」的授权来源，
   登记必须落在**那张卡**上，不是本卡。
2. **归档名带 `*-diag-*`，报告首行 `diagnostic=true / 不可用于容量结论`**：
   归档目录固定为 `E:/UserFleet/archive/world-entry-serial-diag-<YYYYMMDD-HHMM>/`；
   `report.json` 与任何人读报告首行写该字符串；`window.conclusion_valid=false`。
3. **判据 = 定位串行点**，判据表见 §3.5；**不出现 `in_world ≥ 95%` 作为 pass/fail**。

### 3.3 两臂（同一 1000 档，只改「AOI 负载」，不改代码）

| 臂 | 配置 | 目的 |
| --- | --- | --- |
| **A（负载臂）** | 现默认：`MIR2_BOT_ACTION_PROBE=1`（load_gate 默认）+ BotSrv `DisableAutoPlay=false` | 复现 327/425 形状 |
| **B（空载臂）** | `MIR2_BOT_ACTION_PROBE=0` + `AppSetting.json` 的 `BotPlay.DisableAutoPlay=true` | 去掉 CM_TURN/走位 ⇒ 几乎无 AOI 扇出；**若 B 能到 ≥950、A 不能，则 ③ 的 AOI 扇出被证实是触发条件** |
| **C（可选，仅当 A/B 都平台时才跑）** | 默认负载 + 网关多条 gate 线（N=4） | 检验 G4/W1 是否「单线」限制：单线变四线若吞吐上升 ⇒ ① 为真 |

时间轴（每臂）：起栈（复用 `-RestartStackPerTier`，约 60–90s）→ 1000 bot 登录/进世界 ≤180s →
保持到 180s 处（平台在约 80–95s 已出现，180s 足够看到「零增长」）→ 收尾。
**建议 `HoldSec=180`；`-NoActionProbe` 只用于 B 臂。** 每臂另存 `gate_stats.ndjson`、
`load_stats.ndjson`、`bots.log`、`game-svr.out.log`、`game-gate.out.log`、`selfreport.ndjson`。

### 3.4 采什么（全部是既有开关，零代码改动）

| 探针 | 设在哪 | 产出 | 用在哪一跳 |
| --- | --- | --- | --- |
| `MIR2_LOG_MISDELIVER=1`（game-svr） | game-svr 进程 | 每条 SM_LOGON 一行 `MISDELIVER_SRV event=send ident=50 actor=… chr=… socket=… session_id=… decision={Send\|CheckFirst\|Hold\|Drop}` | **S2**（发送口判定，含 Hold/丢） |
| `MIR2_GATE_STATS_OUT=<dir>`（game-svr） | game-svr 进程 | `gate_stats.ndjson`（5s 一行，累计）：`queue_enabled/queued_frames/flushed_frames/overflow_frames/reject_login_frames/degrade_inworld_frames/close_inworld_sessions/max_depth_frames/top_idents` | **S2**（队列/丢弃/回显） |
| `event=gate_frame_queued\|overflow\|degrade` 日志（默认开、1s 限速） | game-svr stdout | 带 `ident=` 与 `depth_frames=`，可看 SM_LOGON(50) 是否被排队/拒绝 | **S2** |
| `进世界 ChrName` / `进世界帧下发` / `进世界标记`（默认开） | game-svr stdout | 每 bot 各 1 行 ⇒ **S1 精确计数** | S1 |
| `MIR2_LOG_MISDELIVER=1`（game-gate） | game-gate 进程 | `MISDELIVER_ACCEPT peer= sid= handle= collision=`、`MISDELIVER_DELIVER sid= owner= ident=50 send_ok=`、`MISDELIVER_NOROUTE`；无条件 `WRN 会话[..]不存在，丢弃下行` | **S3**（网关收到/投递/无路由） |
| `MIR2_BOT_SELFREPORT=<file>`（BotSrv） | BotSrv 进程 | 每连接 `connect` 行（`chr/login_id/conn_seq/local_ep/remote_ep`）+ 每条 `sm_logon` 行（+`actor/x/y/in_world_counted`） | **S4**（bot 侧到达与身份） |
| `bots.log` 的 `机器人:[x] SM_LOGON actor=` / `成功进入游戏` | BotSrv stdout | **每 bot 精确**的 S4 计数（不受 top-24 截断影响） | **S4** |
| `load_stats.ndjson`（默认） | BotSrv | `in_world` 曲线、`recv_chunks`/`frames_decoded` 速率、`partial_frames`、`internal_errors`、`probe_sent` | S4 + ③ 判据 |
| `MIR2_TICK_PROFILE=1` + `MIR2_TICK_PROFILE_SLOW_MS=<N>`（可选） | game-svr | host 段耗时（`HostInbound`/`HostPersist`/`HostSwitch`…） | G1/G2 |

**三方对齐键**：game-svr 的 `session_id` ↔ 网关的 `sid` ↔ 网关的 `peer` ↔ BotSrv 的 `local_ep` ↔ BotSrv 的 `chr`。
用这四列把「同一个 bot 的 SM_LOGON」在四跳上串成一行，得到**逐 bot 的丢失跳**。
（`SM_LOGON` 帧本身不带 chr，所以必须靠网关的 `sid↔peer` 注册行与 bot 的 `local_ep` 对齐；
`SelfReport` 就是为此设计的。）

### 3.5 判据（定位串行点，不是判容量）

先把每个 bot 归到一个 `stage`（S1 发出、S2 发送口决定、S3 网关投递、S4 bot 计数），
然后按下表判「哪一类、哪一跳」：

| 观察 | 结论 |
| --- | --- |
| S1≈1000，且 S2 的 `decision=Hold` 中 SM_LOGON 占比高 / `overflow_frames>0` 且 `reject_login_frames>0` / `queued_frames` 单调涨而 `flushed_frames` 不涨 | **① 发送口串行/流控**（G4）；`queue_enabled=1` 而 `reject_login>0` ⇒ 队列容量不够（② 的字节上限起作用） |
| S2 决定都正常，S3 `MISDELIVER_DELIVER ident=50` 数 ≥950，但 S4 < 950 | **③ 或 bot 侧丢帧**：再看 `frames_decoded` 速率是否饱和 + `partial_frames` 是否随人口涨；A/B 两臂差异 ⇒ 若 B 臂达标则是 AOI 扇出（③），若 B 臂仍不达标则是解析/消费（B1/B3） |
| S3 `send_ok=false` 或 `MISDELIVER_NOROUTE>0`（非 teardown 时段） | **网关会话路由**（W2/W3） |
| S1 < 950 | **game-svr 进世界段**（G1/G2）——与现有归档矛盾，需先查配置/读档 |
| A 臂平台、B 臂 ≥950，且 A 臂 `frames_decoded` 饱和、B 臂低负载 | **③ 被证实为触发条件**；再按 A 臂的 S2/S3 定位 ① 是否同时存在 |
| A/B 两臂速率完全相同且与人口无关 | **② 显式限速**（目前无此常量，出现则说明有未发现的计数上限） |

**停机条件**：任一臂出现 OOM（`bots.err.log` 命中 `Out of memory`）⇒ 立即停臂并记 `arm_aborted=oom`，
不重跑、不延长（探针不是验收窗口）。

### 3.6 归档与报告格式

- 目录：`E:/UserFleet/archive/world-entry-serial-diag-<YYYYMMDD-HHMM>/{arm-A,arm-B[,arm-C]}/`
- 每臂：`bots.log`、`bots.err.log`、`load_stats.ndjson`、`gate_stats.ndjson`、`game-svr.out.log`、
  `game-gate.out.log`、`selfreport.ndjson`、`report.json`
- **`report.json` 首行字段**：`"diagnostic": true`、`"usable_for_capacity": false`、
  `"purpose": "locate serial point of SM_LOGON (not in_world>=95%)"`
- 人读报告首行：`diagnostic=true / 不可用于容量结论`
- 产物：`stage_table.csv`（逐 bot：`chr,s1,s2_decision,s3_delivered,s4_counted,loss_stage`）

---

## 4. 如果证实是某一类：修法方向（**本卡不实现**）

| 证实的类 | 修法方向 | 风险/前置 |
| --- | --- | --- |
| **① 真串行（G4 发送口）** | 现树已有 A+B 队列；若仍丢，方向是**提高每条 gate 线的并发度**：多 gate 线（`GateSlots` 上限 20，配置多条线并均匀分会话）或**按会话分队列**（消除全 gate 共享 FIFO 的顺序约束），而不是加大单队列容量 | 顺序硬约束（`SM_SERVERUSERINDEX → SM_LOGON` 不得乱序）必须保留；协议帧面不变；`close_inworld` 必须仍为 0 |
| **② 限速（字节预算）** | `CheckBlock*10` 的字节预算与 50ms 超时是 oracle 语义，改它属**行为变更**；可先只做「预算外排队 + 回显合并」（现树已做），不擅动 oracle 常数 | 需偏差登记与真机对拍 |
| **③ 排队但可并行（B1 单消费者 / AOI O(N²)）** | 两条互补路线：(a) **消费者分片**——按 bot/session 分 N 条处理队列（每 bot 包天然可并行；只需保证 `MShare` 全局态按包设置/恢复，已有 `ProcessOnePacket` 的缝）；(b) **降低 AOI 帧量**——`SM_TURN`/`SM_FEATURECHANGED` 已是非关键可丢帧，可在网关/服务端合并同一 actor 的连续转身帧（现状是每次转身 2 帧 × 全视野） | (a) 会触及 BotSrv 全局态（`MShare.ConnectionStep`/`MapMovingWait` 等仍是全局），必须先 per-bot 化；(b) 属可观测行为变化，需对照 oracle |
| **网关路由（W2/W3）** | 先修 `SessionId` 撞号分配与 stale `sid` 清理；`deliver_gm_data` 的 `send_ok=false` 必须计数（现在只有日志） | 低风险，纯修复 |

---

## 5. 未覆盖 / 已知不确定

1. **新树 1000 档无任何实测**：本文所有归档都在队列补丁之前，① 是否已消掉未知。
2. **`top_idents` 只有 top-5 且混入非关键丢弃**：SM_LOGON 被 AOI 挤出榜单时**不可见**，
   所以探针应以 `MISDELIVER_SRV`/`MISDELIVER_DELIVER` 的逐帧行与 `bots.log` 为准，
   不能只看 `gate_stats` 的直方图。
3. **BotSrv `ident_counts` 只报 top-24**（`IdentCountsJson`）：稳态里 `50` 会被 AOI 挤出
   （`plateau-094127` 从 60s 起就查不到 `50`），**禁止用 `ident_counts["50"]` 断言 bot 侧 SM_LOGON 数**。
4. **BotSrv 组帧无损性未验证**：`ProcessPacket` 对跨 read 残帧丢弃（`partial_frames` 2000/轮）；
   本轮量级不足以解释 575，但 A/B 臂仍需看它是否随人口增长。
5. **G2 同拍处理 1000 个应答**没有每拍上限：如果将来出现「一拍吃掉数百 ms」，
   它会与 AOI 一起放大 G1；本轮 `tick_p99` 只在 500 档测过（47ms），1000 档缺。
6. **`MShare` 仍有一批跨 bot 全局会话态**（`ConnectionStep`、`RunServerAddr`、`MapMovingWait`、
   `g_boMapMoving`）：本轮不是平台来源，但做 ③(a) 分片前必须先清理，否则分片会引入新竞态。

## 6. 本批边界

- 只读码 + 复读既有归档；**未起栈、未跑窗口、未改代码、未重跑任何归档**。
- 归档复读的命令与口径见附录 A；所有数值可离线重算。
- 分支 `card/world-entry-serial-probe`（基于 master `531aa544a`）；本文是唯一改动。

---

## 附录 A：本批数字的复读命令（只读）

**两轮归档的日志目录不同**（`supp-hold360` 的 game-svr / 网关日志在**归档根**下的
`stack-logs-v2/`，不在 `tier-1000/` 里；`plateau-094127` 的在归档根下的 `stack-logs/`）：

```sh
SUPP=E:/UserFleet/archive/load-tiers-20261115-0245
PLAT=E:/UserFleet/archive/botsrv-inworld-plateau-20261115-094127

# S1：game-svr 侧进世界（supp 用 stack-logs-v2，plateau 用 stack-logs）
grep -c "进世界 ChrName"   "$SUPP/stack-logs-v2/game-svr.out.log"
grep -c "进世界帧下发"     "$SUPP/stack-logs-v2/game-svr.out.log"
grep -c "进世界标记"       "$SUPP/stack-logs-v2/game-svr.out.log"
grep -c "收到角色数据"     "$SUPP/stack-logs-v2/game-svr.out.log"
grep -o "共 [0-9]* 条"     "$SUPP/stack-logs-v2/game-svr.out.log" | sort | uniq -c
grep -c "进世界 ChrName"   "$PLAT/stack-logs/game-svr.out.log"

# S4：bot 侧精确计数（不受 top-24 截断；「进入游戏」是子串，必须带 bot 前缀）
grep -cE "机器人:\[[^]]+\] SM_LOGON actor=" "$PLAT/tier-1000/bots.log"
grep -cE "机器人:\[[^]]+\] 进入游戏"        "$PLAT/tier-1000/bots.log"
grep -c  "准备进入游戏"                       "$PLAT/tier-1000/bots.log"

# ③：解码帧构成与速率
python -I -c "import json;o=json.loads(open(r'$PLAT/tier-1000/load_stats.ndjson',encoding='utf-8').read().splitlines()[-1]);print(o['frames_decoded'],o['ident_counts'])"

# S2：发送口统计（旧 schema 无 overflow_frames；hold_frames 是全帧 hold，不是 SM_LOGON）
tail -1 "$PLAT/gate_stats/gate_stats.ndjson"

# 网关路由
grep -c "不存在，丢弃下行" "$PLAT/stack-logs/game-gate.out.log"
```

## 附录 B：探针轮的开关清单（开跑前逐条核）

| 进程 | 变量 | 值 | 默认 |
| --- | --- | --- | --- |
| game-svr | `MIR2_LOG_MISDELIVER` | `1` | 关 |
| game-svr | `MIR2_GATE_STATS_OUT` | `<arm>/gate_stats` | 关 |
| game-svr | `MIR2_GATE_SEND_QUEUE` | 不设（=1，队列开） | 开；**探针必须断言 `queue_enabled=1`** |
| game-svr | `MIR2_TICK_PROFILE` / `MIR2_TICK_PROFILE_SLOW_MS` | `1` / `50`（可选） | 关 |
| game-gate | `MIR2_LOG_MISDELIVER` | `1` | 关 |
| BotSrv | `MIR2_BOT_SELFREPORT` | `<arm>/selfreport.ndjson` | 关 |
| BotSrv | `MIR2_BOT_ACTION_PROBE` | A 臂 `1` / B 臂 `0` | 关（load_gate 默认置 1） |
| BotSrv | `AppSetting.json` `BotPlay.DisableAutoPlay` | A 臂 `false` / B 臂 `true` | `false` |
| BotSrv | `MIR2_BOT_OUT` | `<arm>` | load_gate 设置 |
