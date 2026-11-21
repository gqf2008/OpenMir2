# 在线索引收益实测：**<5% ⇒ 不回改索引**（2026-11-21/22，debug 栈）

> 卡：容量线 L1/L2/L3（`card/perf-index-d8` 收口件）。
> 设计：`docs/issues/online-probe-design-20261121.md`；实现与探针：`a8701bf11`（已合 master `f3583ee2d`）。
> 本次取数工作树 head：`000b91bf8`（merge master `32ae18f55`）。
> **profile = debug**：计数与 tick 耗时分开读；本卡的百分比用 debug 单次差值折算，**方向未知**，
> 不得当作 release 端到端结论。

## 1. 口径与数据有效性

- 栈：Rust debug `mir2-server`（mtime 2026-11-22 00:04、构建 RC=0、二进制含 `MIR2_PERF_COUNTERS`
  特征串），`start_mir2_server.ps1 -Root E:\MirServer`，探针 env 注入 game-svr。
- 负载：`tools/botload/load_gate.ps1`（主检出侧），`-HoldSec 240`，单轮。
- **行数 = 真实 tick 数**：debug 栈实际 ~1.5–2 tick/s（不是 200ms/tick 的 5/s）；不按 hold 时长
  硬凑 tick 数，两档均取 **≥300 个真实 tick** 的稳态段。

| 档 | TSV 行区间 | 有效 tick 行 | 稳态取用 | load_gate 结果 |
| --- | --- | --- | --- | --- |
| 500 | 1250→1666 | **416** | 最后 300 | login 100%、in_world 500、GREEN（smoke；`window_valid=false` 因 hold<360s） |
| 986 | 1677→2024 | **347** | 最后 300 | login 100%、in_world **863**、**internal_errors=107 ⇒ RED** |

986 的 RED 记录在此、**不展开分析**（见 §5 未解释项）；计数仍来自有效 tick，但负载退化可能低估。

## 2. 分项计数（稳态 300 tick；mean / p50 / p95）

广播类是"每几 tick 爆发"的双峰（p50 常为 0），**看 mean 与 p95**，不要只看 p50。

| 计数 | tier 500（n≈500） | tier 986（n≈863 在册） |
| --- | --- | --- |
| `actor_indexed` | 1533.7 / 0 / 7447 | 848.5 / 0 / 5400 |
| `chr_indexed` | 0 / 0 / 0 | 0 / 0 / 0 |
| `actor_linear` | 178.2 / 3 / 1108 | 339.8 / 6 / 1405 |
| `chr_linear` | 502.3 / 58 / 1787 | 632.2 / 251 / 2593 |
| `refmsg_calls` | 36.9 / 28 / 86 | 36.3 / 31 / 83 |
| `refmsg_targets` | 1546.6 / 0 / 7508 | 856.4 / 0 / 5445 |
| `say_calls` | 0 | 0 |
| `say_recipients` | 0 | 0 |
| `kick_scans` | 0 | 0 |

## 3. 折算与判据

公式：`Δms/tick = Σ N_i × (L_i − I_i)`（N 为本次实测每 tick 均值；L−I 取 debug 微基准）。

| 档 | 差值（debug） | 已实现（索引项） | 潜在（剩余线性也索引） | 合计 | 200ms 预算 | 100ms 设计点 |
| --- | --- | --- | --- | --- | --- | --- |
| 500 | Δa≈338ns、Δc≈1463ns（n=500 插值） | 0.52ms | 0.80ms | **1.31ms** | 0.66% | 1.31% |
| 986 | Δa≈832.6ns、Δc≈2675.7ns（n=986 实测） | 0.71ms | 1.97ms | **2.68ms** | 1.34% | 2.68% |

**结论：两个预算口径都 <5% ⇒ 不回改索引。**（最接近的是 986 对 100ms 设计点的 2.68%。）

## 4. 主贡献分析

- **A 类扇出计数最大**（`refmsg_targets` 爆发段 p95 5.4k–7.5k），但**已经走索引**，单次差值小；
  它解释了绝大多数 `actor_indexed`。
- **若将来还要做，最值得的是 C/D 类 `chr_name` 线性查找**：986 档 `chr_linear` 单项
  1.69ms，占"潜在"1.97ms 的 **86%**，对应全仓 **43 处未索引 `online.iter().find(chr_name)`**。
  但即使把 1.69ms 全部拿回，对 200ms 仍只有 ~1%，不足以改变排序结论。

## 5. 未解释项与覆盖缺口（不展开、不补数）

1. **986 `internal_errors=107`**：只记录。可能是 debug 栈吞吐不足导致的负载退化（in_world 863/986），
   **未证**；不属本卡，未做根因。
2. **`say` / `kick` 两档均为 0**：bot 不说话、不顶号 ⇒ `handle_say`（chr+actor 两路）与
   `online_find_for_kick` 的**真实负载未被测到**，本卡结论不含这两条路径。
3. **差值取自 debug 微基准**（`online_lookup_micro_index_vs_linear`），偏大/偏小方向未知；
   release 差值需另一次构建（内存风险，未做）。
4. 986 的 `n` 实际是在册 863（掉线后），不是登录 986；计数按在册规模解释。

## 6. 产物

- TSV：`E:\UserTemp\d8_perf_counters.tsv`（含两档全部 tick；表头 10 列）
- 摘要：`E:\UserTemp\d8_perf_summary.json`
- load 输出：`E:\UserTemp\d8_load_500b`、`E:\UserTemp\d8_load_986`
- 栈日志：`E:\UserTemp\d8_stack_logs`（栈已 graceful 停，端口全 down）
