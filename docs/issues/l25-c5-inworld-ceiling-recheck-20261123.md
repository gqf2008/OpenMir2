# C5 复核：「进世界天花板」不成立（短窗截断 + 机制已修，2026-11-23）

> **判决：C5「2k 只到 80.6%、4k 93.5%，服务端发送层 Hold 丢 SM_LOGON」的前提不成立。**
> 两个百分比都是**窗口短于进场 ramp、进场队列被截断**的读数；Hold 丢关键帧的机制在 2026-11-15
> 10:15 被 A+B 有界队列替换，现行二进制只在回退开关 `MIR2_GATE_SEND_QUEUE=0` 下才恢复 oracle 的
> 「直接丢」。本文件是**只读分析**：未起栈、未改任何代码；基线 master `b388a3317`。

## 0. 结论表

| C5 的说法 | 复核判定 | 章节 |
| --- | --- | --- |
| 2k 档 `in_world` 只到 80.6%（1612/2000） | **不成立**：hold=120s 短窗截断，序列到停时仍在 +9.4/s 上升 | §1 |
| 4k 档 93.5%（3739/4000） | **不成立**：420s 窗截断；且 ~3/s 进场速率下 420s 数学上满不了员 | §2 |
| 机制 = 发送层 Hold 丢 `SM_LOGON` | **对 2026-11-15 10:15 前的二进制成立；现行版本已修**（A+B 队列默认开） | §3 |
| — | **保留的两条真限制**：进场速率（4k ~3/s）+ 内存闸（可用内存 2560MB 阈值） | §4 |
| — | **口径缺口**：两臂产物都没记 `BotSrv.exe` sha | §5 |

## 1. 2k 80.6% 的出处与证伪

**出处（全量扫过 `E:/UserTemp/l25-slice/` 下所有 `report.json`，1612/80.6% 只此一处）**：
`E:/UserTemp/l25-slice/retest-2k-20261123-0333/`，即
`docs/l25-retest-report-20261123.md` §2「步骤 3：1k/2k 120s 单线复测对账」臂。

- `tier-2000/report.json` 原文：
  `"world_ok": 1612, "world_ok_pct": 80.6, "steady_window_ms": 115250, "peak_login_ok": 2000, "login_success_pct": 100.0, "internal_errors": 0`
- 门日志 `load_gate.log` 原文：
  `FIELDS tier=2000 login=100% no_resp=0 internal_errors=0 OOM=0 allowlist_budget=0 internal_excused=True hold_sec=120 window_valid=False verdict=RED`
  ⇒ **该臂自报 `hold_sec=120`、`window_valid=False`**（判据要求 ≥360s）。
- 同报告 §2 脚注原文已写明：「2k 120s 臂 `in_world=1612`（ramp 未完）且 `window_valid=False`
  （HoldSec=120 < 360），**只作参考，不作稳态对比**」。
- 同形状的只有一处：`E:/UserTemp/l25-slice/hin-info-2k-20261123-0240/tier-2000/report.json`
  （`world_ok=1610, world_ok_pct=80.5, steady_window_ms=115234`）。

**序列（决定性）**：该臂 `tier-2000/load_stats.ndjson` 共 33 个样本，`in_world` 单调线性，
到被 gate 停掉时**零减速**：

```text
up=140 1422 / up=145 1470 / up=150 1518 / up=155 1565 / up=160 1612
```

末段 +47/5s ≈ **9.4/s**，没有任何平台；`world_ok` 与末样本 `in_world` 精确相等（1612）。

**600s 合法臂对照**：`E:/UserTemp/l25-slice/curve-2k-hold600-detached-20261123-0931/`
（hold=600、`window_valid=True`）：

```text
up=195 1612 / up=250 1990 / up=255 2000 / up=641 2000（其后 380s 恒 2000）
```

末样本原文：`"spawned":2000, "login_ok":2000, "in_world":2000, "conn_lost":0, "internal_errors":0, "logon_misdelivered":0`。

**两次运行逐项差异**：

| 项 | 1612 臂（retest-2k-0333） | 600s 臂（curve-2k-hold600-0931） |
| --- | --- | --- |
| tier / bots | 2000 / 2000 | 同 |
| 窗口 | `hold=120s`、`window_valid=False`、总时长 160s | `hold=600s`、`window_valid=True`、总时长 641s |
| 网关线 | 单线 7200 | 单线（p50=201；三线臂 `3line-2k-20261123-0542` 为 p50=78，口径可区分） |
| BotSrv | 同 exe 路径；`BOTGUARD_ENV heap_limit=4GiB (4294967296 bytes) … [DOTNET_GCHeapHardLimit=0x100000000]`，`oom_hits:0` | **逐字相同**的 BOTGUARD_ENV/4GiB |
| server 二进制 | `ec5b7b18…`（master `3ac846e0`） | `7e1a541a…`（zone-mirror-step2 HEAD `2e40ad671`） |

⇒ 实质差异只有**窗口长度**（两个二进制都已含 §3 的 A+B 队列）；堆设置相同、均无 OOM。
**80.6% = 短窗截断，不是天花板。**

## 2. 4k 93.5% 的出处与证伪

- 出处：`E:/UserTemp/l25-slice/4k-single-heap8-20261123-0413/tier-4000/report.json`：
  `"world_ok": 3739, "world_ok_pct": 93.47, "steady_window_ms": 415859`（hold=420s、heap 8GiB、
  `window_valid=True`）。该臂在 501s 被截断时序列**仍在上升**：

```text
up=490 3317 / up=496 3725 / up=501 3739
```

- 同一现象的另一臂：`4k-single-retry-20261123-0349` 的 73.2%（`world_ok=2928`）是 **BotSrv 进程内
  OOM**（`internal_errors=44`，既有卡 `capacity-4k-botsrv-oom`），不是服务端拒绝。
- 今天两个 4k 臂都是**内存闸停的**（不是进场天花板）：
  - `E:/UserTemp/l25-slice/curve-4k-hold600-detached-20261123-0942/MEM_STOP.md` 原文：
    「可用物理内存：2533 MB（阈值 2560 MB）」「63096 | BotSrv | 4789」「58408 | mir2-server | 4928」
    （`world_ok=3241`）。
  - `E:/UserTemp/l25-slice/curve-4k-m1d-20261123-1041/MEM_STOP.md` 原文：
    「可用物理内存：2431 MB（阈值 2560 MB）」「28668 | BotSrv | 4518」「61216 | mir2-server | 4969」
    （`world_ok=2978`）。
  - `curve-4k-hold600-retry2-20261123-0951` 无 `report.json`（未跑完）。
- **数学**：4k 进场速率实测 ≈ **3/s**（`4k-m1d` 序列 2871→2978 / 36s），而 4000 假人 / 420s
  需要 **9.5/s** ⇒ **任何 420s 臂都不可能满员**，`world_ok_pct` 必然 < 100。这不是容量结论。

## 3. Hold 丢 `SM_LOGON`：现行版本只在回退开关下成立

**机制来源（历史真事）**：`docs/botsrv-inworld-plateau-20261115.md`（窗口 2026-11-15 09:41–09:58）
原文「服务端发出 1000 次 SM_LOGON，**Send=327 / Hold=673**，bot `in_world=327`，**Send(50) 与
in_world 精确相等（差 0）**」；同一报告末尾写明当时「修复选项已由卡 … 交总监裁决；本卡不改策略」。

**修复（晚于该窗口）**：

- `5ae3fdaa8` 2026-11-15 **10:15:41** `feat(gate): 背压 A+B——有界排队 + 溢出拒绝`；
- `17f70c2b3` 2026-11-15 **10:41:53** `feat(gate): 下行帧按关键性分两条通道，关键帧溢出按会话阶段分流`；
- `775e5d641` 2026-11-19 观测口径修正（行为不变）。

**代码自述**：

- `mir2-rs/bins/game-svr/src/gate_backpressure.rs` 模块头：oracle `ProcessBufferSend` 在
  `SendChecked > 0` 与单包超阈值两处**直接 return、不留计数/日志**；本模块按总监裁决实现
  A（FIFO 有界队列，默认 256KiB/4096 帧）+ B（溢出按会话阶段分流，已进世界不关会话）。
- `mir2-rs/bins/game-svr/src/gate_queue.rs` 模块头「回退（D-* 登记项）」：
  「`MIR2_GATE_SEND_QUEUE=0`（或任意一个容量设 0）⇒ 本队列**禁用**：`Hold` 帧恢复 oracle 的
  「直接丢」，但仍走既有观测面（计数 + 限速日志），因此不会退回「静默丢帧」。」
  同文件 `ENV_QUEUE_ENABLED` 注释：「发送队列开关（`0`/`false`/`off`/`no` ⇒ 关闭，回退 oracle
  直接丢）。**默认开**。」
- `mir2-rs/bins/game-svr/src/lib.rs` 的 `sim_loop` 侧注释与 `PendingQueue::from_env()` 接线同上
  （默认 256KiB / 4096 帧；`MIR2_GATE_SEND_QUEUE=0` 才回退）。

**今天 2k 600s 臂的 gate 汇总原文**（`game-svr.out.log` 末条 `gate_backpressure_summary`，节选）：

```text
queue_enabled=1 queue_max_bytes=262144 queue_max_frames=4096 hold_frames=741034
queued_frames=630735 flushed_frames=630718 oversize_frames=0 rejected_sessions=0
reject_login_frames=0 degrade_inworld_frames=110299 close_inworld_sessions=0
```

且对该日志逐条检索：`gate_frame_queued.*ident=50` 与 `gate_frame_overflow.*ident=50` 命中数**均为 0**
⇒ `SM_LOGON` 既没被 Hold 吞掉、也没进队列/溢出；2000 个假人全部进世界。
⇒ **C5 机制解释对现行 master 已过期**；只有显式设 `MIR2_GATE_SEND_QUEUE=0` 才回到 oracle 丢帧行为。

## 4. 保留的两条真限制（与 C5 无关，但影响 1 万可测性）

1. **进场 ramp 速率**：2k ≈8–9.4/s（`retest-2k-0333` 末段 9.4/s；`curve-2k-hold600-0931` 255s 到 2000）；
   4k ≈3/s（`curve-4k-m1d-1041`）。任何短于 `tier / 速率` 的窗口读数都不是容量结论。
2. **内存闸**：本机守卫阈值 2560MB 可用内存；4k 时 BotSrv 4.5–4.8GB + 单条 `mir2-server` 约 5GB
   即触发 `MEM_STOP`（§2 两处原文）。1 万的可测性仍受 BotSrv 分片/堆上界约束（卡
   `capacity-4k-botsrv-oom`、`botsrv-heap-cap-4gib-oom`）。

## 5. 口径缺口（建议登记）

两个对照臂的产物（`artifacts.sha256.json` 只哈希 tier 目录内的 ndjson/report；`bots_runtime.json`
只记 exe 路径与 PID）**都没有 `BotSrv.exe` 的 sha256** ⇒ 只能证明「同路径 + 同 `BOTGUARD_ENV`」，
不能证明「同一 BotSrv 构建」。建议 `load_gate.ps1` 在 `BOTGUARD_ENV` 行追加 `bot_exe_sha256=`，
并写进 `artifacts.sha256.json`。

## 6. 证据索引

| 用途 | 路径 |
| --- | --- |
| 1612/80.6% 臂 | `E:/UserTemp/l25-slice/retest-2k-20261123-0333/` |
| 同形状短窗臂 | `E:/UserTemp/l25-slice/hin-info-2k-20261123-0240/` |
| 2k 600s 合法臂 | `E:/UserTemp/l25-slice/curve-2k-hold600-detached-20261123-0931/` |
| 2k 三线（口径对照） | `E:/UserTemp/l25-slice/3line-2k-20261123-0542/` |
| 4k 3739/93.47% 臂 | `E:/UserTemp/l25-slice/4k-single-heap8-20261123-0413/` |
| 4k BotSrv OOM（73.2%） | `E:/UserTemp/l25-slice/4k-single-retry-20261123-0349/` |
| 4k 内存闸（今天） | `E:/UserTemp/l25-slice/curve-4k-hold600-detached-20261123-0942/`、`E:/UserTemp/l25-slice/curve-4k-m1d-20261123-1041/` |
| 机制来源（11-15） | `docs/botsrv-inworld-plateau-20261115.md` |
| 复测报告（短窗脚注） | `docs/l25-retest-report-20261123.md` §2 |

序列提取方式（只读）：`python -I -B` 逐行读 `tier-*/load_stats.ndjson`，取 `uptime_ms / spawned /
login_ok / in_world / conn_lost / internal_errors`；判定「是否平台」看末段增量是否趋 0。

## 7. 建议的 C5 改写文本（可直接引用）

> **C5（已不成立 / 不可复现）**：旧记录「2k `in_world` 只到 80.6%（1612/2000）、4k 93.5%
> （3739/4000），发送层 Hold 丢 `SM_LOGON`」是两类假象：①读数取自短于进场 ramp 的窗口
> （2k 120s、4k 420s），`world_ok_pct` 取窗口末值，而进场队列在该时刻仍在线性上升
> （2k 末段 +9.4/s；4k ~3/s，420s 窗需 9.5/s 才能满员）；②Hold 丢关键帧只对 2026-11-15 10:15
> 之前的二进制成立，现行版本由 A+B 有界队列兜住（`MIR2_GATE_SEND_QUEUE` 默认开，
> 2k 满员臂 `reject_login_frames=0`、`SM_LOGON` 入队/溢出均为 0）。真正限制 1 万可测性的是
> **进场速率**与**内存闸**，不是发送层丢帧。

## 8. 声明

- 本文件**不含任何代码改动**，也不改任何既有文档；只新增本复核记录。
- 全部结论来自已有产物与当前 master 代码的只读核对；未起栈、未跑负载、未构建。
