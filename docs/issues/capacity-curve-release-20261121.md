# 容量曲线（release 真实栈）：986 在预算内、2000 越过 100ms（2026-11-21/22）

> 目的：回答"tick 随在线人数怎么长、在哪个档位首次超过 100ms/200ms、首次超限时最大单事件多少"。
> **profile = release**；`window_valid=false` 是 load_gate 的 smoke 口径（HoldSec < 360s），
> 不是压测 GREEN 结论。**`HostTickTotal` 未测**（未开 `MIR2_TICK_PROFILE`）；本卡只有
> `HostInbound`（探针 `inbound_wall_us`）与 bot 视角 tick 分位。
> 探针：`MIR2_PERF_COUNTERS`（`perf_counters`），检测到开关才启用；开而 0 tick 行 ⇒ rc≠0。
> 栈：`start_mir2_server.ps1 -Root E:\MirServer -ServerExe <本 worktree release mir2-server>`。

## 1. 前置与二进制

- release 二进制：`cargo build --release -p mir2-server`（`CARGO_BUILD_JOBS=1`，3m28s，RC=0），
  mtime 2026-11-22 01:54；含 `MIR2_PERF_COUNTERS` / `max_single_event_us` 特征串。
- 账号：`account_provision.ps1 -Prefix loadbot -Count 8000`（BULK_OK，幂等）。
- 2000 档开跑前记录：02:08:21 free **10,856MB**、BotSrv=0、栈端口 0；02:08:24 起栈、
  02:08:29 就绪（free 9,318MB）；`RUN_START=02:08:32`、`RUN_END=02:12:20`（≈228s）。
- 每次取数后栈 graceful 停、端口全 down、BotSrv=0。

## 2. 两轮实测（同一 release 二进制；栈内不重启，按 TSV 行区间切片）

### 轮 A：500 / 986（HoldSec=240）

| 档 | tick 行（稳态取用） | login | in_world | tick p99 | load_gate |
| --- | --- | --- | --- | --- | --- |
| 500 | 1274（300） | 100% | 500 | **32ms** | GREEN |
| 986 | 1319（300） | 100% | 986 | **47ms** | GREEN |

探针（稳态均值）：

| 列 | 500 | 986 |
| --- | --- | --- |
| `inbound_enqueued` | 123.5 | 145.6 |
| `inbound_drained` | **123.3** | **143.6** |
| `inbound_carry` | 0.0 | 0.1 |
| `inbound_budget_hit` | **0.0** | **0.1** |
| `inbound_wall_us` | 23,243 | 27,121 |
| `drained_command` / `close` / `control` | 122.3 / 1.0 / 0 | 142.4 / 1.2 / 0 |
| `max_single_event_us` p50/p95/max | 482 / 1,426 / 5,723 | 600 / 1,613 / 4,334 |

（500/986 的 tick **p50/p90 未取**——这两轮只记录了 p99；不补测、不插值。）

### 轮 B：2000（HoldSec=180）

| 指标 | 值 |
| --- | --- |
| tick 行（稳态取用） | 1132（**300**） |
| login | **100%** |
| `in_world` | **1973**（world_ok 98.65%） |
| conn_lost_steady / internal_errors | 0 / 0 |
| tick p50 / p90 / p99 / max | **31 / 125 / 201 / 390ms**（mean 48.04） |
| load_gate | **RED，唯一红项：tick P99 201ms > 100ms** |

探针（稳态 300 tick）：

| 列 | mean | p50 | p95 | max |
| --- | --- | --- | --- | --- |
| `inbound_enqueued` | 287.2 | 169 | 1830 | 2484 |
| `inbound_drained` | **237.0** | 223 | 335 | 371 |
| `inbound_carry` | **1.0** | 1 | 1 | 1 |
| `inbound_budget_hit` | **1.0** | 1 | 1 | 1 |
| `inbound_wall_us` | 50,110 | 50,098 | 50,239 | 50,830 |
| `drained_command` | 236.4 | — | — | — |
| `drained_close` | 0.6 | — | — | — |
| `drained_control` | 0.0 | — | — | — |
| `max_single_event_us` | 1,166 | 735 | **2,480** | **22,276** |
| `refmsg_targets` | 88,567 | 97,139 | 134,621 | 144,442 |

## 3. 曲线表

| 档 | login | in_world | tick p50 | p90 | p99 | max | load_gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 500 | 100% | 500 | — | — | 32ms | — | GREEN |
| 986 | 100% | 986 | — | — | 47ms | — | GREEN |
| **2000** | 100% | **1973** | **31ms** | **125ms** | **201ms** | **390ms** | **RED（仅 tick P99）** |
| 4000 / 8000 | — | — | — | — | — | — | **未测（本机内存不足）** |

## 4. 结论（五条）

1. **首次超 100ms 在 986 与 2000 之间**；2000 的 p99 201ms 已贴住 200ms 预算，p90 125ms 亦超 100。
   没有 1500 档，**不插值**。
2. **增长呈超线性迹象**（p99 47→201ms ≈ 4.3×，人数 2×；p50 16→31、p90 31→125），但只有三点且
   含爬坡差异，**不宣称函数形态**。
3. **入站饱和点同样落在 986–2000 之间**：986 `enqueued≈drained`、`budget_hit≈0`、wall 27ms；
   2000 `enq 287 vs drained 237`（净 +50/tick）、`budget_hit=100%`、`carry` 恒 1、wall 钉 50.1ms
   ⇒ 入站在此跨过饱和点（这是直接计数证据，不是模型）。
4. **2000 档 max 单事件不是主因**：p95 2.48ms、max 22.3ms，远小于 50ms 预算；同 tick
   `refmsg_targets` 88.5k（AOI 扇出）⇒ 该档性质是**扇出/总工作量**，不是"一条重事件吃掉预算"。
   （"重事件吃掉预算"的证据只在 debug 栈出现，见 `docs/issues/inbound-step0-measurement-20261121.md`。）
5. 2000 档 load_gate RED **只来自 tick P99>100ms**；login/conn/internal/OOM 全绿。`window_valid=false`
   意味着这是 smoke 口径数据，不是压测 GREEN 结论。

## 5. 4000 / 8000 不可得（如实）

- 第一次尝试 tier 2000 时（HoldSec=240）在约 190s 被**系统内存 reaper** 杀掉（BotSrv 被杀，
  load_gate 无 `report.json`）；复核后仅获批单档重试（HoldSec=180）才得到上表 2000 数据。
- **4000 / 8000 未尝试**：BotSrv 单进程 + 本机可用内存在 2000 档已逼近上限；按总控口径
  写明"**本机内存不足以测更高档**"，不估算。

## 6. 产物

- 轮 A：`E:\UserTemp\d8_release_counters.tsv`、`d8_release_summary.json`、
  `d8_rel_500`、`d8_rel_986`、`d8_rel_stack_logs`
- 轮 B：`E:\UserTemp\d8_curve2_counters.tsv`、`d8_curve2_2000`、`d8_curve2_logs`
- 被杀的首次 2000 尝试（仅非稳态部分观测）：`E:\UserTemp\d8_curve_2000`、`d8_curve_counters.tsv`

## 7. 补测（986 档｜探针 + tick profile 同开）：同档两轮差 1.6×

第二次 986 运行（release 同一二进制、`HoldSec=180`、`MIR2_PERF_COUNTERS` + `MIR2_TICK_PROFILE=1`）：

- load_gate **GREEN**：login 100%、in_world 986、internal_errors 0；tick p50/p90/p99 **16/31/47ms**。
- `refmsg_targets`（稳态 300 tick）：**mean 26,207.5 / p50 27,932 / p95 38,595**；`refmsg_calls` mean 231.1。
- `HostFrames`（tick profile 稳态末段）：**0.842 ms/tick = 0.42% tick**（分母 200ms 名义 tick；
  按该档 tick p50 16ms 折算约 5%）；窗口从 0.047 增长到 0.842 ms/tick。
- 对照：`inbound_enqueued 221.1 ≈ drained 219.7`/tick、carry 0.25、budget_hit 0.25、
  wall 37.98ms 均值；`max_single_event_us` p95 1.59ms。

**同档两轮差异（必须带着读）**：

| 986 档轮次 | `refmsg_targets` mean | p50 | p95 |
| --- | --- | --- | --- |
| 轮 A（500/986 那一轮） | **16,146.6** | 15,216 | 34,253 |
| 轮 B（本轮，profile 同开） | **26,207.5** | 27,932 | 38,595 |

两轮相差 **1.6×**，成因是**场景/站位（bot 聚簇与探针节奏）而非人数**——同一 986 档、同一二进制。
⇒ **N 决策请用区间 16k–26k/tick**，不要拿单点当确定值；2000 档 88.5k 与 986 的比值在
**3.4–5.5×**（人数 2×，超线性迹象，但同样不是单点结论）。

**2000 档未开 tick profile ⇒ `HostFrames` 未测**；不拿 986 的 0.42% 外推 2000。

### 7.1 测量教训（默认区间）

**同参数两轮可差 1.6×**：单点测量不能当确定值。以后容量结论默认给**区间**；只有同一参数
跑三次且离散度小，才允许给单值。给单值时必须在卡片里附三次原始值与离散度。

