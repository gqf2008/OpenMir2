# Step 0 入站测量：debug 栈上**入站确实积压**（2026-11-22）

> 前置：`docs/issues/inbound-drain-impl-plan-20261121.md`（四步方案）Step 0。
> 范围：**只读计数**——不改队列结构、不改语义、不加丢弃。探针沿用
> `MIR2_PERF_COUNTERS=<path>`（默认关闭；关闭态零分配；0 tick 行 ⇒ rc≠0）。
> **profile = debug**；工作树 head：merge master `0a57cd4a3` 后的 Step 0 分支。

## 1. 新增计数（本步代码）

在既有探针（`perf_counters`）上新增 5 列，TSV 表头共 15 列：

| 列 | 定义 |
| --- | --- |
| `inbound_enqueued` | `emit_gate_event` 投递成功的事件数（两条通道合计，生产端） |
| `inbound_drained` | `EventDrain::next` 返回的事件数（含结转） |
| `inbound_carry` | 其中来自结转槽（上一轮预算探测取走、本轮才处理）的条数 |
| `inbound_budget_hit` | 本轮 `EventDrain::hit_budget` 为真的 tick 数（每 tick ≤1） |
| `inbound_wall_us` | 本轮 `HostInbound` 排水墙钟（微秒，每 tick 覆盖） |

挂点（符号）：`emit_gate_event`、`EventDrain::next` 四个返回路径、`sim_loop` 的
`EventDrain::begin(PRODUCTION)` 前后与 `drain.hit_budget` 块。无队列结构/语义改动。

## 2. 数据有效性

| 档 | TSV 行区间 | 有效 tick 行 | 稳态取用 | load_gate |
| --- | --- | --- | --- | --- |
| 500 | 12→773 | **761** | 最后 300 | RED：**唯一红项 tick P99 110ms > 100ms**；login 100%、conn 0、internal_errors 0、in_world 500 |
| 986 | 789→1297 | **508** | 最后 300 | RED：同上（P99 110ms）；login 100%、internal_errors 0、in_world **921**（掉线后） |

两档都按「行数 = 真实 tick 数」取 ≥300；debug 栈 ~1.5–2 tick/s，HoldSec=240 对应 761/508
行，不是 200ms/tick 的 5/s。load_gate 的 RED 只来自共享机 + debug 的 tick P99，不影响计数面。

## 3. 入站计数（稳态 300 tick，mean / p50 / p95）

| 列 | tier 500 | tier 986 |
| --- | --- | --- |
| `inbound_enqueued` | **260.7** / 252 / 505 | **164.0** / 0 / 723 |
| `inbound_drained` | **71.5** / 78 / 87 | **169.5** / 243 / 251 |
| `inbound_carry` | **1.0** / 1 / 1 | **1.0** / 1 / 1 |
| `inbound_budget_hit` | **1.0** / 1 / 1 | **1.0** / 1 / 1 |
| `inbound_wall_us` | 52,692 / 50,313 / 59,854 | 49,406 / 50,146 / 51,031 |

（同段其它列可对照 `d8_step0_counters.tsv`；`say`/`kick` 仍为 0——bot 不说话/不顶号。）

## 4. 结论（按 Step 0 判据，**debug 口径**）

1. **入站确实积压**（与本卡之前"可能不是瓶颈"的假设相反）：
   - 500 档：`enqueued 260.7` vs `drained 71.5` ⇒ **净积压 ≈189 事件/tick**；
   - 986 档：`enqueued 164.0` vs `drained 169.5` ⇒ 基本持平但**仍排不空**（见下）。
2. **瓶颈是墙钟阀，不是条数阀**：两档 `inbound_budget_hit` **每 tick 都是 1**、
   `inbound_wall_us` 均值 ≈50ms（p95 51–60ms），而 `drained` 仅 71.5/169.5 条
   ——离 `EventDrainBudget::PRODUCTION.max_events = 2000` 差一个量级 ⇒ **单条处理成本**
   才是限制；调大条数阀无效。
3. **队列从未排空**：`inbound_carry` 恒为 1（每 tick 预算探测都还能取到一条），
   两档都是「预算到点即停、剩余留到下一轮」的稳定饱和态。
4. **不能直接外推到生产**：debug 单条处理成本按容量文档口径约放大 25×；本次只证明
   **排水机制在 debug 下被墙钟预算卡死且有积压**，**不能**据此给 release 的 3319/s vs x/s 数字，
   也不能据此直接上 Step 1 丢弃策略。生产判定需要 release 栈同法复测（内存窗口另排）。
5. 因此 Step 0 的回答是：**"入站是不是瓶颈"在 debug 口径下 = 是（积压+预算饱和）；
   release 口径 = 未测，不能引用本次数字下结论。** Step 1（有界队列/丢弃）在生产数据出来前
   仍不启动——这与总控"先测后治"的口径一致。

## 5. 产物

- TSV：`E:\UserTemp\d8_step0_counters.tsv`（两档全部 tick，15 列）
- load 输出：`E:\UserTemp\d8_step0_500`、`E:\UserTemp\d8_step0_986`
- 栈日志：`E:\UserTemp\d8_step0_stack_logs`（栈已 graceful 停，端口全 down）
- 二进制：`mir2-rs/target/debug/mir2-server.exe`（mtime 2026-11-22 01:26，
  含 `MIR2_PERF_COUNTERS` 与 `inbound_enqueued` 特征串）
