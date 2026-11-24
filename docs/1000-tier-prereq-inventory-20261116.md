# 1000 档复测三项前置：证据盘点（2026-11-16）

> **本文状态：盘点件（证据盘点）** —— **未跑任何窗口**；结论为「三项前置均未解除，禁止启动 1000 档窗口」。

> 卡：`openmir2-load-1000-rerun-prereq`（tier=R，window_required=true）
> 身份：`openmir2-worker-97`；基线 master `268e15ef8`；分支 `card/1000-prereq-inventory`
> 边界：**只读盘点**——本卡不改代码、不起栈、不跑窗口、不重跑任何归档。所有数字要么取自既有归档，
> 要么是对既有归档的离线重算；推算一律标注「推算」。

## 0. 结论（先读）

| 项 | 现状（1000 档） | 判据 | 是否解除 |
| --- | --- | --- | --- |
| ① BotSrv 4GiB 堆上限 OOM | 修复前：OOM 实证（`supp-hold360` 峰值 WS 4410.1MB / priv 4409.4MB，`bots.err.log` = `Out of memory.`）；修复后：**1000 档无实测**，只有 500 档平坦（WS 852–1215MB，OOM=0） | `OOM=0` 且 `bots.err.log` 无 `OutOfMemory` | **未解除**（修复后 1000 档未测；4GiB 是否够未证——按修复后 500 档外推约 2–2.5GB，推算上可能够） |
| ② 网关背压丢帧 | **无 1000 档后修复归档**。唯一 1000 档网关归档（`botsrv-inworld-plateau-20261115-094127`）早于关键性两通道补丁，且 schema 里没有 `close_inworld` / `reject_login` / `degrade_inworld`；其 `in_world` 只有 327/1000 | `in_world ≥95%`、`close_inworld=0`、`reject_login`/`degrade_inworld` 有数且可解释 | **未解除**（500 档全过；1000 档未测；单线吞吐在 500 已接近上界） |
| ③ 进世界串行载入 | 1000 档 `in_world` 在 **327–425 处硬平台**，从未达到 95%（950）；旧树 1000 档进世界速率 ≈5.3/s | 窗口内 `in_world ≥95%` | **未解除**（平台未归因；`openmir2-world-entry-serial-saves` 已交付分析文档并合并（needs-review），仍无修复后 1000 档实测） |

**结论：现在不能开 1000 档窗口。** 三项里没有一项拿到「1000 档 + 后修复树」的可机检证据；
即使把窗口拉长到 360s，第 ③ 项的平台仍会让 `in_world ≥95%` 直接失败。开窗会怎样失败、解除顺序见 §4。

---

## 1. ① BotSrv 4GiB 堆上限 OOM

### 1.1 现状值（含出处）

**1000 档有实测，但都是内存修复前的树**（`master 77c19cedd` 一带，2026-11-15 02:44–03:01）：

| 归档 | 参数 | BotSrv 峰值 WS / priv | `bots.err.log` | `world_ok` | 结果 |
| --- | --- | --- | --- | --- | --- |
| `archive/load-tiers-20261115-0245/tier-1000/`（60s） | `-BotHeapLimitGB 4`，HoldSec 60 | WS 760.0MB / priv 797.1MB | 空 | 0/1000 | 60s 内 `in_world=0`，无可判稳态 |
| `archive/load-tiers-20261115-0245/supp-hold360/tier-1000/`（360s） | `-BotHeapLimitGB 4`，HoldSec 360 | **WS 4410.1MB / priv 4409.4MB** | **`Out of memory.`**（第 1 行） | 425/1000 | BotSrv 02:59:39 `OutOfMemoryException` 退出（`env.txt` 时间线；`mem-watch.csv` 02:59:37 `botsrv_ws_mb=4255.1`） |
| `archive/botsrv-inworld-plateau-20261115-094127/tier-1000/`（150s） | `-BotHeapLimitGB 6`，HoldSec 150 | WS 3665.9MB | 空 | 327/1000 | 平台；未 OOM（heap 6GiB） |

补充反证（500 档 + 360s 也会 OOM，修复前）：

- `archive/gate-capacity-360-20261115/`：**归档根**的 `metrics.json` 的 `behavior_four` 记
  `oom_hits=5`、`botsrv_crash=true`；同轮 `run/tier-500/report.json` 的 BotSrv 峰值为 **WS 4320.0MB / priv 4357.8MB**；
  `run/tier-500/bots.err.log` 前 4 行是 `Out of memory.`，第 5 行 `Fatal error.`；同轮 `in_world` 仍到 500/500（先完成进世界后才崩）。
  总监据此把增长重定级为「内存泄漏」（≈11.2MB/s），不是单纯堆上限问题。

**修复后的现状（关键：500 档已平坦，1000 档没测）**：

- 自由池泄漏修复已合入 master `05a0e4815`（2026-11-15 21:10，tier=R），
  证据见 `docs/botsrv-actor-pool-fix-20261115.md` §3：修复树 500/360 轮 BotSrv WS 896→987MB，峰 1609MB（进世界瞬态）。
- LOH 归因已合入 master `5692250d5`（2026-11-16 08:33）：**LOH 不是泄漏**——固定存活基线 216MB
  （500 × `MapInfo[90,120]`，约 432KB/bot）+ 稳态定时垃圾 394KB/5s（`WriteLoginConnsLocked` 整表重写，
  已由 `289fdd8bd` 的 login_conns 复用缓冲消除）。
- 2026-11-16 的三轮 500 档 × 360s 干净栈实测（`E:/UserFleet/load-360-recvnull-verify-20261116/`、
  `load-360-slowsep-A-20261116/`、`load-360-slowsep-B-20261116/`）：
  BotSrv WS 峰 987.1–1215.6MB、末值 852.5–980.5MB、尾段斜率 −0.31…+0.37MB/s（噪声量级）；
  `report.json` 的 `botsrv.err_log_oom=false`、`oom_hits=0`、`crash=false`。
- **修复后 1000 档没有任何实测归档**：`E:/UserFleet/archive/` 下的 `tier-1000` 目录共 **4 处**——
  `load-tiers-20261115-0245/tier-1000`（60s）、`supp-hold360/tier-1000`（360s）、
  `diag-misdeliver/tier-1000`（45s，`world_ok=330`、login 77%）、
  `botsrv-inworld-plateau-20261115-094127/tier-1000`（150s）；`rollback-test` 下另有历史副本。
  这些都在内存修复前；2026-11-15 之后没有再跑 1000 档。

### 1.2 判据阈值

- `OOM=0`：`tools/botload/load_gate.ps1` / `load_report.py` 现行口径（`docs/loadgate-criteria-runbook-20261115.md` §1.2）
  ——`bots.err.log` 全文（大小写不敏感）命中 `Out of memory` ⇒ RED；`report.json` 的
  `required_fields.oom_hits` 取「err.log 命中行数 + Windows Application Error 事件数」。
- 另需满足同 runbook 的四项：`login`、`no_resp`、`internal_errors`、`OOM` 逐项列数。

### 1.3 差距

1. **1000 档修复后未测。** 修复前的 4GiB OOM 是「泄漏 + 时间项」的复合结果，修复后每 bot 常数已变化，
   不能把 `docs/botsrv-heap-1000bot-20261115.md` 的 8.0–9.2MB/bot 模型直接搬来判 1000；
   该文也已在 actor 池卡里被标注「旧口径作废：它把时间项摊进了常数」。
2. **4GiB 在 1000 bot 下是否够，当前只能给推算，不能给结论。**
   按修复后 500 档 ~1.0GB 稳态、LOH 基线 432KB/bot 线性外推，1000 档约 **2–2.5GB**，小于 4GiB；
   但这是推算（500 档实测 ×2 + LOH 基线 ×2），未含 1000 档的未知非线性项。
3. **机器内存不是本卡的既定障碍，但也没有实测支撑。** 修复前模型（`docs/botsrv-heap-1000bot-20261115.md` §4–5）
   给 60s 需 ≈9.0–9.5GB 托管堆、360s 需 ≈16–17GB、机器可用内存 ≥16GB/≥24GB——**那是含泄漏与时间项的旧口径，
   不能直接拿来判修复后的 1000 档**。修复后 500 档 × 360s 的 BotSrv 稳态只有 ~0.9–1.0GB（WS 峰 987–1216MB），
   按 bot 数线性外推 1000 档约 **2–2.5GB**（含 LOH 基线 432KB/bot）⇒ **4GiB 在推算上够**。
   本机物理 32,615,808 KB（≈31.1GiB）；窗口期可用 7,250.7–11,637.1MB（`mem-watch.csv` 的 `free_mb`，实为 MiB）；
   2026-11-16 实测空闲 14,414,520 KB（≈13.75GiB）。若 1000 档真在 2–2.5GB，机器内存不是障碍；
   **但这是推算，未实测，不能据此开窗。**

### 1.4 解除它需要什么

按优先级：

1. **拿到 1000 档修复后实测**（可在 60s 冒烟或 360s 窗口内）：同时采 `dotnet-counters`
   （Gen2 after-GC / LOH / GC Heap）与 `bots.err.log`，证明 `OOM=0`。这是把 §1.3 的推算变成结论的唯一路径。
2. **若实测 >4GiB**：把 `-BotHeapLimitGB` 提到实测值的 1.2–1.5×（修复前模型曾给 12/20GB，那只是旧口径上界参考），
   并确保机器可用内存 ≥ 新堆上限 + 其它服务开销；同时保留 `-MemKillThresholdGB` 看门狗（现行默认 8GiB）。
3. **若不想抬堆**：降每 bot 分配或分片。分片（多个 BotSrv 进程按账号段分摊，各自 4GiB 上限）当前**没有实现**，
   也没在归档里出现过；`docs/botsrv-heap-1000bot-20261115.md` §8 只列了候选方向，未做。
4. 注意：修复前 `load_gate` 对 OOM **不判 RED**（只打 WARN）；该判据缺口已由 `docs/loadgate-criteria-runbook-20261115.md`
   补上，2026-11-16 的归档里已能看到 `botsrv.err_log_oom` / `oom_hits` 字段。

---

## 2. ② 网关背压丢帧

### 2.1 现状值（含出处）

**1000 档：没有后修复数据。** 唯一带 `gate_stats` 的 1000 档归档是
`archive/botsrv-inworld-plateau-20261115-094127/`（2026-11-15 09:41–09:52），它：
- 早于 A+B（有界排队 + 溢出拒绝）与关键性两通道补丁（那两张卡 11:00 之后才合并）；
- `gate_stats.ndjson` 的字段只有 `hold_frames` / `hold_bytes` / `oversize_*` / `overflow_*`，
  **没有** `close_inworld_sessions` / `reject_login_frames` / `degrade_inworld_frames`（旧 schema）；
- 末行 `gate_stats.ndjson`（`t_ms=175404`）：`hold_frames=30,352,774`、`overflow_frames=0`；
- 同轮 `report.json`：`world_ok=327/1000`。

因此三项任务里点名的 1000 档 `in_world` / `close_inworld` / `reject_login` / `degrade_inworld` / 丢弃率，
**只有 `in_world=327/1000` 一个数有 1000 档出处；其余四项在 1000 档「无」**。

**500 档后修复值（可作参考，不是 1000 档结论）**：

| 归档 | 档/窗口 | in_world | close_inworld | reject_login | degrade_inworld | overflow | 非关键丢帧率 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `E:/UserFleet/load-360-recvnull-verify-20261116/` | 500 / 360s | **500/500** | 0 | 0 | 0 | 0 | **85.93%** |
| `E:/UserFleet/load-360-slowsep-A-20261116/` | 500 / 360s | **500/500** | 0 | 0 | 0 | 0 | **89.01%** |
| `E:/UserFleet/load-360-slowsep-B-20261116/` | 500 / 360s | **500/500** | 0 | 0 | 0 | 0 | **88.96%** |
| `archive/gate-criticality-20261115-w83/tier-N1/` | 500 / 60s | **500/500** | 0 | 0 | 0 | 0 | 未算（60s） |
| `archive/gate-criticality-20261115-w83/tier-N4/` | 500 / 60s | **500/500** | 0 | 0 | 0 | 0 | 未算（60s） |

- 丢弃率口径：`metrics.json` 的 `gate_summary.drop_pct` = `drop_noncritical_frames /
  (drop_noncritical_frames + noncritical_sent_frames)`，只统计**非关键**帧；关键帧走后端有界队列，
  `overflow_frames=0` 表示关键队列从未溢出。
- `gate-criticality` 与 `load-360-*` 的 `close_inworld_sessions=0` 是**结构性的**：关键性两通道规定
  「已在世界」的会话不因关键帧溢出被关；非关键 AOI 帧只丢不关。

### 2.2 判据阈值

- `in_world ≥95%`（1000 档即 ≥950）；
- `close_inworld=0`；
- `reject_login` / `degrade_inworld` 有数且可解释；
- 掉帧率单列（不能用一个「全绿」盖住）。
- 注意：`world_ok` **不在** `load_gate` 默认 `verdict` 里（`docs/loadgate-criteria-runbook-20261115.md` §3.3），
  1000 档窗口必须显式加这组卡面判据，否则会得到「tick 绿但只有 40% 人在世界」的假绿。

### 2.3 差距

1. **1000 档整组未测（后修复树）。** 500 档的五项全过，不能外推成 1000 档——丢帧率在 500 已达 86–89%，
   说明单条 gate 线已接近饱和；1000 档只会更高。
2. **单线吞吐上界。** `openmir2-gateway-throughput-pipeline` 卡面（未认领，P0）记录：
   每个 gate 连接只有 1 个在途 `SendChecked`，周期上界 50ms、每周期 ≤122,880B ⇒ 单线理论 ≈62k 帧/s、
   本机实测有效 ≈38.1k 帧/s；按每进世界会话 ≈131 帧/s 外推，500 档 ≈65.7k（已≈理论上界）、
   1000 档 ≈131k（**2.1×**）、10k ≈1.31M（21×）。⇒ 1000 档如果仍用单线，非关键丢帧会继续上升；
   关键帧（登录/控制）走队列，理论上仍可通过，但「关键队列不溢出」在 1000 档没有实测。
3. **多线实验已有结论但只到 500 档。** `openmir2-gateway-multiline-sharding`（已完成）：
   关键性版本 N=1/N=4 均 500/500、`close_inworld=0`、`overflow=0`；无关键性版本 N=4 也只有 483/467。
   结论是「瓶颈是 AOI 帧堵关键帧，不是部署拓扑」，但这同样只在 500 档验证。

### 2.4 解除它需要什么

1. **在 1000 档、后修复树上跑一次带 `gate_stats` 的窗口**（N=1 与 N=4 各一轮更稳），逐项报
   `in_world` / `close_inworld` / `reject_login` / `degrade_inworld` / `overflow` / `drop_pct`。
   这是唯一能把 500 档结论升级到 1000 档的证据。
2. **若 1000 档单线 `in_world <95%` 或关键队列溢出**：走 `openmir2-gateway-throughput-pipeline`
   （流水线化或分线，P0，未认领）；在它落地前，至少用 N=4 多线部署顶过 1000 档。
3. **判据接线**：把 `world_ok ≥95%`、`close_inworld=0`、`reject_login/degrade_inworld` 解释
   作为卡面判据写进窗口脚本/报告，不要依赖默认 `verdict`（默认不含 `world_ok`）。

---

## 3. ③ 进世界串行载入（最关键，决定窗口长度）

### 3.1 现状值（含出处）

**1000 档 `in_world` 曲线（从各归档 `load_stats.ndjson` 离线重算；`uptime_ms` 与 `in_world` 字段）：**

| 归档 | 档 | 5s | 10s | 15s | 30s | 45s | 75s | 末值（窗口） | 达到 95% |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `load-tiers-20261115-0245/tier-200/` | 200 | 56 | 135 | 198 | 198 | 198 | — | 198 / 65s | **15.0s** |
| `load-tiers-20261115-0245/tier-500/` | 500 | 0 | 48 | 122 | 308 | 404 | 404 | 404 / 75s | **从未**（475） |
| `load-tiers-20261115-0245/tier-1000/` | 1000 | 0 | 0 | 0 | 0 | 0 | 0 | **0 / 80s** | **从未** |
| `load-tiers-20261115-0245/supp-hold360/tier-1000/` | 1000 | 56 | 133 | 186 | 288 | 338 | **425** | **425 / 297s**（75.2s 起平台，221.7s 零增长） | **从未**（950） |
| `botsrv-inworld-plateau-20261115-094127/tier-1000/` | 1000 | 49 | 124 | 185 | 269 | 290 | **327** | **327 / 170s**（75.2s 起平台，95.3s 零增长） | **从未**（950） |
| `load-360-recvnull-verify-20261116/tier-500/` | 500 | 52 | 130 | 206 | 426 | 500 | 500 | 500 / 375s | **35.0s** |
| `archive/gate-criticality-20261115-w83/tier-N1/tier-500/` | 500 | 56 | 133 | 209 | 415 | 500 | 500 | 500 / 75s | **35.1s** |
| `archive/gate-criticality-20261115-w83/tier-N4/tier-500/` | 500 | 56 | 133 | 209 | 411 | 500 | 500 | 500 / 75s | **40.1s** |

**速率口径：**

- 官方 runbook 的口径（`docs/loadgate-criteria-runbook-20261115.md` §1.3）：
  「进世界是串行载入，实测 ~5–7/s：`supp-hold360` 的 `in_world` 从 5s 的 56 涨到 75s 的 425（≈5.3/s）后平台」。
  本次重算一致：(425−56)/(75.234−5) = **5.25/s**。
- 1000 档 game-svr 的 DB 应答（`执行任务Id:N成功`，**全量 DB 应答计数，不严格等于 bot 数**）有**两套值，必须分开读**：
  (a) `tier-1000`（v1 栈，60s）：取窗 **18:51:08.072–18:52:32.935（UTC）**，该窗 **363 条 / 84.863s = 4.277/s**。
  同期 v1 有一段「下发 SM_* 帧时 socket=… 的槽不存在」告警风暴（18:49:02.849–18:54:08.533，696,873 行，
  整份 `game-svr.out.log` 约 83MB）；它和 4.28/s 只是**共现**——同一风暴区间也覆盖了 tier-500 的 13–15/s 阶段，
  因此**不能把 4.28/s 归因给风暴**。（同日志取更晚的 84.9s 窗会得到 1027 行，属不同阶段；本处只取 tier-1000 窗口。）
  (b) `supp-hold360`（v2 栈，重启后）：`执行任务Id` 共 **1312 条**；前 **1000 条在 69.3s 内完成（≈14.4/s）**，
  尾部 **312 条约 4.2–4.4/s**（取窗边界差异）；同一轮 `in_world` 仍只到 425/1000。
  ⇒ **4.28/s 不能当作串行载入的固有上限；v2 的 14.4/s 说明「DB 应答」不是 425 平台的那一环。**
  作为参照：tier-200 窗口 15.6/s、tier-500 窗口稳态约 13–15/s。
- `supp-hold360` 的每 bot 进世界时延（**按 bot 名配对**：起点 = 该 bot 自己的 `机器人:[x] 进入游戏` 行，
  终点 = `机器人:[x] 成功进入游戏` 行；425 个成功者全部按名配上）：
  中位 **12.13s**、p90 **42.28s**、p99 **48.50s**、max **49.49s**（nearest-rank 分位）。
  **不能用「第 k 条准备 → 第 k 条成功」的全序配对**：本轮 425/1000 成功 ⇒ 成功行会配到别人的准备行，
  按全序会得到 13.13/51.85/59.77/61.06 的错配值（结论不变，但数字必须用按名配对的）。
  1000 个 bot 里只有 425 个成功，**575 个停在准备阶段**（`bots.log` 中 `成功进入游戏` 425 条）。
- 500 档后修复树的速率：500/500 在 40.0s 到齐，`5s→35s` 段 (498−52)/30 = **14.9/s**。

**两张卡的状态：**

- `openmir2-world-entry-serial-saves`：**已合并（needs-review）**，交付 `docs/world-entry-serial-plateau-analysis-20261116.md`
  （13 处串行点清单 + ①/②/③ 判据与排序 + 2 臂诊断探针轮设计）；验收 A1–A4 的判据与探针设计见该文档，
  修复后 1000 档实测仍无（本节归档为修复前）。
- 它引用的背景（「执行任务Id ~5–7/s，02:52:40 才处理到 loadbot455」）与本次重算方向一致；
  `02:52:40` 那个具体时刻来自当时现场日志，本归档的 `tier-1000/bots.log` 在 02:51:29 结束
  （全 1000 人只到「准备进入游戏」，无 `SM_LOGON`），无法在归档里复现该行。

### 3.2 判据阈值

- 卡面：窗口内 `in_world ≥95%`（1000 档 ≥950）。
- 窗口准入：`HoldSec ≥ 360s`（`docs/loadgate-criteria-runbook-20261115.md` §1.3）；
  `60s` 只能冒烟。

### 3.3 差距——直接回答「1000 档达 95% 大约要多久、360s 够不够」

**先给推算（按 X/s，X 的来源写在括号里）：**

| X（进世界速率） | 来源 | 950 个所需时间 |
| --- | --- | --- |
| **5.3/s** | `supp-hold360` 1000 档实测（5s 56 → 75s 425） | **≈179s** |
| **4.2/s** | v1 tier-1000 窗口实测 4.277/s（保守取 4.2；这是窗口值，不等于固有上限；v2 同环已到 ≈14.4/s） | **≈226s** |
| **12.5–14.9/s** | 后修复树 500 档（500/500 在 40s 到齐） | **≈64–76s** |

**在「速率保持不变」的前提下，360s 是够的**：最慢的 4.2/s（v1 窗口实测 4.277/s 的保守取整）也只要 ≈226s，
还剩 ≈134s 稳态；5.3/s 只要 ≈179s，还剩 ≈181s。**窗口长度本身不是约束。**

**但现有证据里 1000 档从来没有达到 95%**：两次 1000 档长窗口都停在
**327/1000（32.7%）** 和 **425/1000（42.5%）**，且分别在 95.3s、221.7s 内零增长。
这不是「慢」，是**平台/天花板**；平台不解除，窗口拉到 600s 也不会到 950。
因此对「360s 够不够」的准确回答是：

> **按速率算术够；按现有实测不够——因为平台把上限压在 327–425，95% 判据在到达 360s 之前就已经注定失败。**

平台归因目前**未定**（这正是 `openmir2-world-entry-serial-saves` 要回答的）。候选（均未证）：
串行载入链路的**后续环节**（`SM_LOGON` 下发 / 网关 / BotSrv 消费）——注意 v2 轮 DB 应答已到 ≈14.4/s
但仍平台在 425，所以候选**不应写成「DB 应答速率」**；关键帧/AOI 互相阻塞、BotSrv 内存压力（修复前 OOM 前兆）。
后修复树的 500 档已从 404 平台升到 500/500，说明网关关键性修复至少解除了 500 档的平台；
**1000 档是否同样解除，没有数据。**

### 3.4 解除它需要什么

1. **认领并完成 `openmir2-world-entry-serial-saves`**：按 A1 量出「连接 → 选角 → `SM_LOGON`」每段耗时与
   串行点（DB 查询次数/延迟、单线程队列），并特别定位**「DB 应答完成后 → `SM_LOGON`」这一段**
   （v2 证据显示平台在这一段或之后，不在 DB 应答）；按 A2 对照 oracle；按 A3 评估批量/并发读存档的收益；
   按 A4 把 ramp 判据写进 runbook。
2. **在后修复树（含关键性 + actor 池 + LOH 三项）上重测 1000 档 ramp**：至少 150–360s，
   记录 `load_stats.ndjson` 的 `in_world` 曲线、`bots.log` 的进世界时延分布、game-svr 的 `执行任务Id` 速率。
   要证明的不是「更快」，而是**平台是否消失**（能否越过 425）。
3. **只有在平台消失后**，才用 §3.3 的 X/s 定 `HoldSec`：按最慢实测速率留 ≥1.5× 余量；
   若实测 ≥5.3/s，950 的 ramp ≈179s，360s 窗口留 181s 稳态，可行。
4. 若平台确由串行 DB 载入造成：优化方向是批量/并发读存档（A3），但属行为变更，需对照 oracle 与窗口证据。

---

## 4. 是否可以开 1000 档窗口

### 4.1 判定

**不可以。** 三项前置全部未解除：

- ① 修复后 1000 档无实测；4GiB 够不够未证——按修复后 500 档外推约 2–2.5GB（推算），可能够；
  本机当前空闲 14,414,520 KB（≈13.75GiB），若推算成立则不是障碍，但仍需实测确认。
- ② 1000 档无后修复 `gate_stats`；500 档虽全过，但单线丢帧已 86–89%，1000 档必然更高，关键帧是否仍不溢出未证。
- ③ 1000 档 `in_world` 平台 327–425，95% 从未达到；窗口长度不是约束，平台才是。

### 4.2 如果现在硬开，会怎样失败

1. **`in_world ≥95%` 直接 FAIL**：大概率重演 425/1000 平台；按 42.5% 计算，窗口结束时仍有约 575 个 bot
   停在「准备进入游戏」。tick 样本只来自 ~40% 的子集，**不是 1000 档稳态**，任何 tick/延迟结论都不能引用。
2. **BotSrv OOM 风险**：修复前 425 bot 在 4GiB 下就 OOM；修复后 1000 档未测。若沿用 4GiB 且真实需求 >4GiB，
   会在窗口中途崩溃，`oom_hits>0` ⇒ 该轮直接 RED，且可能提前打断进世界。
3. **网关丢帧率更高**：500 档非关键丢帧已 86–89%；1000 档单线只会更差。关键队列若溢出，
   会出现 `reject_login` / `degrade_inworld`（此前 500 档一直为 0）；`close_inworld` 按设计仍应为 0，
   但这需要实测确认，不能假设。
4. **判据假绿风险**：`load_gate` 默认 `verdict` 不含 `world_ok`，若只跑默认判据，会得到
   「tick/登录绿、但只有 40% 人在世界」的报告。窗口脚本必须显式加卡面五项判据。

### 4.3 开窗前的解除顺序（建议）

1. 先做 ③ 的归因（`openmir2-world-entry-serial-saves`）——它决定窗口能不能有 95% 的进世界，优先级最高。
2. 在 ③ 的修复树上，用 60s 冒烟 + `dotnet-counters`/`bots.err.log` 把 ① 的 1000 档内存实测出来；
   若 >4GiB，再决定抬堆/换机/分片。
3. 同一次 1000 档重测里带上 `gate_stats`（②），N=1 与 N=4 各一轮，逐项验证 `in_world`/`close_inworld`/
   `reject_login`/`degrade_inworld`/`drop_pct`。
4. 三项都有「1000 档 + 后修复树」的可机检证据后，再按 §4.3 的速率定 `HoldSec`（360s 起）并申请窗口。

---

## 5. 数据来源清单（归档路径 + 文件 + 字段/行）

> 归档在 `E:/UserFleet/` 下，不在仓库内；下表只写路径与字段，便于逐项重算。
> 时区口径：`bots.log` 是本地 +0800；`game-svr.out.log` 是 UTC（差 8h），比较时已按 +8h 对齐。

| 用途 | 归档 | 文件与字段/行 |
| --- | --- | --- |
| ①②③ 1000 档主证据 | `archive/load-tiers-20261115-0245/supp-hold360/tier-1000/` | `report.json`：`world_ok=425`、`world_ok_pct=42.5`、`mem.BotSrv.ws_peak_mb=4410.1`、`priv_peak_mb=4409.4`、`mem.BotSrv.samples=96`（**mem 采样点数**；`load_stats.ndjson` 为 60 条）、`steady_window_ms=271890`；`bots.err.log` 第 1 行 `Out of memory.`；`load_stats.ndjson` 的 `uptime_ms`/`in_world`；`bots.log` 的 `成功进入游戏`(425) |
| ① OOM 时间线 | 同上上级目录 | `env.txt` 的「崩溃 1: BotSrv 02:59:39 OutOfMemoryException…」与「内存护栏」；`mem-watch.csv` 02:59:37 行 |
| ① 500 档 OOM 反证 | `archive/gate-capacity-360-20261115/`（`metrics.json` 在**归档根**；`run/tier-500/` 放 `report.json` 与 `bots.err.log`） | `metrics.json` 的 `behavior_four.oom_hits=5`、`botsrv_crash=true`；`run/tier-500/report.json` 的 `mem.BotSrv.ws_peak_mb=4320.0`、`priv_peak_mb=4357.8`；`run/tier-500/bots.err.log` 前 5 行 |
| ① 修复后 500 档 | `E:/UserFleet/load-360-recvnull-verify-20261116/`（及 slowsep-A/B） | `report.json` 的 `mem.BotSrv`（`ws_peak_mb`/`ws_last_mb`/`slope_tail_mb_per_s`）、`botsrv.err_log_oom=false`、`oom_hits=0`、`crash=false` |
| ① 内存模型（修复前，已部分作废） | `docs/botsrv-heap-1000bot-20261115.md` | §4 的 60s 9153MB/12GB、360s 16638MB/20GB；§5 的 ≥16GB/≥24GB；文末勘误 |
| ① 修复与归因 | `docs/botsrv-actor-pool-fix-20261115.md`；master `05a0e4815`、`5692250d5`、`289fdd8bd` | §3 的 500/360 WS 896→987、峰 1609、LOH +0.186MB/s；LOH 卡结论「不是泄漏：216MB 基线 + 394KB/5s」 |
| ② 1000 档网关（旧 schema） | `archive/botsrv-inworld-plateau-20261115-094127/` | `gate_stats/gate_stats.ndjson` 末行 `t_ms=175404`、`hold_frames=30352774`、`overflow_frames=0`；`report.json` 的 `world_ok=327`；`loadgate-1000.log` 的 `heap_limit_gb=6`、`hold=150s` |
| ② 500 档后修复丢帧 | `E:/UserFleet/load-360-recvnull-verify-20261116/`、`load-360-slowsep-A/B-20261116/` | `metrics.json` 的 `gate_summary.totals`（`drop_noncritical_frames`/`noncritical_sent_frames`/`overflow_frames`/`reject_login_frames`/`degrade_inworld_frames`/`close_inworld_sessions`）与 `drop_pct` |
| ② 500 档关键性 | `archive/gate-criticality-20261115-w83/tier-N1/`、`tier-N4/` | `gate_stats/gate_stats.ndjson` 末行；`tier-500/report.json` 的 `world_ok=500` |
| ② 单线吞吐上界 | 卡 `openmir2-gateway-throughput-pipeline`（issue body） | 「单线理论 ≈62k 帧/s、实测 ≈38.1k；1000 ≈131k（2.1×）」 |
| ③ ramp 曲线 | 各归档的 `tier-*/load_stats.ndjson` | `uptime_ms` 与 `in_world` |
| ③ 官方速率口径 | `docs/loadgate-criteria-runbook-20261115.md` | §1.3「~5–7/s；5s 56 → 75s 425（≈5.3/s）后平台」 |
| ③ DB 应答速率 | `archive/load-tiers-20261115-0245/stack-logs-v1/game-svr.out.log`（tier-1000 窗 `18:51:08.072–18:52:32.935`，363 条 / 84.863s = 4.277/s）与 `stack-logs-v2/game-svr.out.log`（共 1312 条；前 1000 条约 14.4/s，尾部 312 条约 4.2–4.4/s） | `执行任务Id:N成功`（**全量 DB 应答计数**，UTC 时间戳，按 +8h 对齐本地） |
| ③ 进世界时延 | `archive/load-tiers-20261115-0245/supp-hold360/tier-1000/bots.log` | **按 bot 名配对**：`机器人:[x] 进入游戏` → `机器人:[x] 成功进入游戏`；425/425 配对；分位用 nearest-rank |
| ③ 来源卡 | 看板 `openmir2-world-entry-serial-saves`（已合并，needs-review） | issue body 的验收 A1–A4 |

## 6. 未找到 / 明确为「无」的数据

- **修复后 1000 档 BotSrv 内存实测：无。** 修复后只有 500 档（2026-11-16 三轮）。
- **1000 档后修复 `gate_stats`：无。** 唯一 1000 档网关归档早于 A+B/关键性补丁，且无 `close_inworld` 等字段。
- **1000 档 `close_inworld` / `reject_login` / `degrade_inworld` / 后修复丢帧率：无。**
- **1000 档 ≥95% 进世界时刻：无**（从未达到；两次长窗口分别停在 327 和 425）。
- **平台归因（串行载入 / 网关 / 内存）：无定论。** 这正是 `openmir2-world-entry-serial-saves` 的待办。
- **本机之外的大内存机器：无可用记录。** §1.4 的 ≥16GB / ≥24GB 是**修复前模型的旧上界参考**，不再是本卡的前置结论；
  修复后 1000 档的实际内存需求未测。
