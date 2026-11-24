# A43：`HostInbound` 按 ident 细分计时 + 慢事件日志（设计；写于实施前）

> **状态**：设计稿（2026-11-25，`card/a43-inbound-ident`，base master `87134db07`）。**本卡只落 `max_single_event_ident` 一列**；本文的细分计时与慢事件日志是**下一张卡**的范围，尚未写码。
> **符号锚**：`lib.rs` / `zone.rs` / `region_loop.rs` / `zone_router.rs` / `tick_profile.rs` 均为 `mir2-rs/bins/game-svr/src/` 下的文件名；高 churn 文件只写符号（函数名 / 常量名 / 列名），不写行号。
> **读数来源**：a7 的 tier-1000 档（树 `87134db07`、1830 tick）。

## 0. 三处口径问题（结论先行）

### 0.1 `inbound_enqueued` 含不含"被 demux 走去区 k 的量"？——**含**

- 唯一 bump 点在 `emit_gate_event`：网关事件投进**区 0 的通道之前**就 +1，与事件最终归哪个区无关。demux（`spawn_zone_demux` → `route_zone_target`）在这之后才按 `(line,socket)` 把事件搬去区 k 的通道。
- 区 k 的投递另有自己的列：`zonek_inbound_enqueued`（`note_zonek_inbound_enqueued`，只计 **`Command`** 且目标区 ≥1 的 `send` 成功）。
- ⇒ 区 0 的"净入站"≈ `inbound_enqueued − Σ zonek_inbound_enqueued`（只对 `Command` 严格；非 `Command` 的区 k 转发与溢出丢弃都不在后者里，所以是近似）。
- **对 a7 那组读数的含义**：`inbound_enqueued` 中位 370、`inbound_drained` 中位 122 的差**不是积压**，主要是"1/3 会话在区 1"。`inbound_wall_us`（58.1 ms）与 `inbound_drained`（122）**同属区 0 同一条 drain**（`inbound_wall_us` 已按卡 1 只由宿主写）⇒ "≈475 µs/条"这个商本身口径一致，可以继续用；但**不要**拿 370 当分母。

### 0.2 `drained_close` 1556 → 645 是谁改的？——**卡 1（`begin_uncounted`），不是卡 2；预期行为**

- `drained_close` 只在 `note_inbound_drained` 里按事件变体 `Closed | CertificationPending` 累加；它的全部调用点都在 `EventDrain::next` 与测试。**卡 2（CPU 计时）不碰任何计数路径**。
- 卡 1 之前：demux/路由线程复用 `EventDrain::begin`，它每搬一条 `Closed` 就记一次 `drained_close`——与区 0 真正的消费**双计**。卡 1 让路由线程走 `begin_uncounted`（`count_host=false`）⇒ 这类搬运不再计入区 0 的分类列。
- ⇒ 下降量 = 每 tick 被 demux 投往区 k 的 `Closed` 条数。z0=652 / z1=348 的落区比例与 59% 的降幅同向，量级自洽。
- **可判定的旁证**：切片关（N=1）时 demux 不搬 ⇒ `drained_close` 与卡 1 之前**逐值不变**。若 N=1 读数也下降，就是漏计，必须停下查。
- **未改动的口径**：`inbound_drained` 仍是"区 0 消费的全部 `GateEvent`"，`drained_command/close/control` 是其三分量；三者之和仍应等于 `inbound_drained`。这条守恒是下一张卡要加牙的（当前没有牙）。

### 0.3 `max_single_event_us` 与 `max_single_event_cpu_us` 各自来自哪条事件？——**同一个定时器、同一批事件，但两个 max 未必是同一条**

- 两列都来自 `perf_counters::EventTimer`：在 `HostInbound` 的 `INBOUND_PROCESS` drain 循环体首行构造 `EventTimer::start(&ev)`，`Drop` 时各自 `fetch_max`。
- 该循环**只有宿主线程进得去**（区 k 的段集 `M1B_ZONE_PROD` = `M1A_PROD | INBOUND_LOCAL`，**不含** `INBOUND_PROCESS`）⇒ 两列都是**单写者**，不存在多区覆盖。
- 语义差：`max_single_event_us` = `Instant::elapsed`（墙钟，含被 OS 抢占、写日志/写盘的阻塞）；`max_single_event_cpu_us` = `GetThreadTimes` 差值（**15.625 ms 量化**，实测取值集 `{0,15625,31250,46875}` 就是它）。所以 cpu 中位 15,625 的真实含义是"落在 (0, 15.625] ms 的第一个量子内"，**不能**读成"精确 15.6 ms"。
- 本卡新列 `max_single_event_ident` **与墙钟列配对**（只有刷新 `SINGLE_EVENT_MAX_US` 的那条事件才写 ident）；它与 cpu 列**不保证同一事件**。列值语义：`Command` ⇒ `cmd.ident`（十进制）；非 `Command` ⇒ 哨兵 `65535`；`0` = 本 tick 没有事件刷新过。

## 1. 下一层设计：按 ident 细分计时

### 1.1 目标

回答"每 tick 那条十几毫秒的事件是谁、花在哪个子步"。现状只能给"最大值 + 它的 ident"；给不了**分布**（是不是所有 `CM_SAY` 都贵？还是某一条日志把一次 `Command` 卡了 18 ms？）。

### 1.2 分桶（第一层）

| 桶 | 判定 | 来源 |
| --- | --- | --- |
| `Command` | `CM_*` 命令类（除下面两个特例） | `mir2_protocol::messages::CM_*` 常量表 |
| `Say` | `CM_SAY`（含频道 `!` 判据的同一族） | 同上 |
| `Move` | `CM_WALK` / `CM_RUN` / `CM_HIT` / `CM_TURN` …（移动族） | 同上 |
| `Other` | 其余 `GateEvent`（`Opened`/`Closed`/`LoadDbRequest`/…） | 哨兵 |

- 分类函数是**纯函数**：`ident_bucket(ident) -> Bucket`，未知 ident 落 `Other`（fail-closed，不 panic、不猜）。
- **必须与现有分类器共用同一份常量**（不手抄 ident 数字）：分类表从 `mir2_protocol::messages` 的 `CM_*` 常量构造，牙要断言"表里每个 ident 都能被分类且分类稳定"。

### 1.3 计时与产出

- **不走 TSV 新列**：8 个桶 × (us 和 + 条数) 是 16 列，会压垮 TSV 的列纪律与对拍测试。改为**独立探针行**（`PROF`，stderr），只在 `MIR2_INBOUND_PROFILE=1` 时按窗口打印：
  ```
  PROF inbound_bucket bucket=<name> ticks=<n> events=<e> us_sum=<s> us_p50=<…> us_max=<m>
  ```
- 计时接缝：`EventTimer` 已经拿到 `&ev`；`Drop` 里在"刷新 max"之外再加一次**无条件**累加（仅当 `ENABLED && INBOUND_PROFILE`）——累加是 `fetch_add`，比 `fetch_max` 多一次原子写，仍在同一线程，无锁竞争。
- 零成本：探针关 ⇒ `EventTimer::start` 的 `wall`/`cpu`/累加全 `None`/no-op（与现状同一条早退路径）。

### 1.4 Move 子步（第二层）

在移动族处理函数内加 5 个**嵌套探针**（`HostTickProfile::probe`，不计入 `tick_sum_ns`）：

| 子步 | 覆盖 |
| --- | --- |
| `MoveLookup` | 会话/实体查找（`session()` / `entities.get`） |
| `MoveStep` | 走格判定与提交（`World::walk` / `try_step_blocked_impl`） |
| `MoveFrame` | 出帧（AOI 增删/自身帧） |
| `MoveTargets` | 收件人集合构造（`VisibleHumanList` / ref_msg 缓存） |
| `MoveSend` | 逐收件人投递 |

- 约束：嵌套探针必须包在 `perf_counters::enabled()` 早退之后，探针关时**不取钟**（与 A43 已有 7 段同规矩）。
- 只对 `Move` 桶开启，避免全量事件被探针拉高。

### 1.5 慢事件日志开关

- `MIR2_SLOW_EVENT_LOG_US=<us>`（缺省**关**）；事件墙钟 ≥ 阈值 ⇒ 打一行 `WRN`（stderr）：
  ```
  WRN 慢事件 ident=<id> bucket=<name> chr=<chr> line=<l> socket=<s> wall_us=<w> cpu_us=<c> targets=<n> tail=<首 32 字节>
  ```
- 限速：≤1 条 / 100 ms（复用 `DepartWarnLimiter` 同形的限流器），附"上一窗口抑制 N 条"。
- `targets` 只在 `Move`/`Say` 桶可取（出帧路径自己知道收件人数）；取不到写 `-`，**不猜**。
- 安全：默认关；打开会改 stderr 输出 ⇒ 任何对拍窗口必须保持关。

## 2. 牙的设计（下一张卡）

1. **分类牙**：`ident_bucket` 对表内每个 `CM_*` 都返回预期桶；未知 ident（如 `0`、`0xFFFE`）⇒ `Other`；`CM_SAY` ⇒ `Say`；移动族每个都是 `Move`（逐值，不抽样）。
2. **累加牙（可变异）**：合成两条事件（`CM_SAY` 慢 20 ms、`CM_WALK` 快）⇒ `Say.us_sum ≥ 20 ms` 且 `Walk.us_sum ≈ 0`；变异：把累加挪到"只对 Command"以外的分支（比如只对 `Other`）⇒ 红。
3. **零成本牙**：`MIR2_INBOUND_PROFILE` 关 ⇒ 所有桶计数恒 0（合成慢事件后仍 0）。
4. **慢日志牙**：阈值设 1 µs + 合成事件 ⇒ 恰好 1 行（断言包含 ident 与 wall_us）；阈值设 1 000 000 µs ⇒ 0 行。日志断言用与既有 `glog` 捕获同形的接缝（不读真实文件）。
5. **守恒牙**：`drained_command + drained_close + drained_control == inbound_drained`（§0.2 的口径守恒；当前**没有**牙，建议随本设计一起补）。

## 3. 顺序与风险

- **顺序**：本卡（ident 列）→ 观察一轮（哪些 ident 反复刷新 max）→ 分桶（1.2/1.3）→ 只在 Move 真的贵时才做子步（1.4）→ 慢日志按需开（1.5）。
- **风险 1（最大值代表的不是常态）**：`max_single_event_*` 是**单条最坏**；中位 18.7 ms + 每 tick 只排 122 条 ⇒ 也可能"一条病态事件把整轮墙钟吃满"。分桶的 `us_sum` 才能区分"一条贵"与"条条贵"——这正是 1.3 要 `us_sum` 而不只是 `max` 的原因。
- **风险 2（cpu 量化）**：15.625 ms 粒度下"1 ms 的事件"与"15 ms 的事件"读数相同。若下一轮需要亚毫秒 CPU，应改用 `QueryThreadCycleTime`（同族 API，周期计数无量化），需同样的最小 `unsafe` 例外范围，**先报再动**。
- **风险 3（探针本身变成本体）**：桶累加是每条事件一次原子 `fetch_add`；1000 在线 × 每 tick 数百条时可忽略，但要**用 a7 的对照档证一次**（开/关 `MIR2_INBOUND_PROFILE` 的 `HostInbound` 段差），不能只靠推理。
