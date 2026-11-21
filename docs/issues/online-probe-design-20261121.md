# 端到端探针设计：OnlineRegistry 查找计数（只设计，不实现）

日期：2026-11-21　范围：容量线 L1/L2/L3（`card/perf-index-d8` follow-up）

## 0. 目的

回答三个问题：

1. 每 tick 里 `online.iter().find` / `find_by_*` 各被调多少次（按 `handle_say` /
   `send_ref_msg_to_targets` / 顶号 / 路由 / 逐 tick 排水分项）；
2. 用**索引 vs 线性的单次耗时差**折算，理论上限能省多少 ms/tick；
3. 若上限 < 预算的 5%，回去重排 `bd` 清单，而不是继续加索引。

**边界**：这是探针设计；端到端收益 = 每 tick 次数 × 单次差值，二者缺一不可。
微基准（`online_lookup_micro_index_vs_linear`）只提供后者。

## 1. 静态盘点（本分支，生产行 < `mod tests`）

**关键事实（比任何基准数字都重要）：106 处线性查找（63 actor + 43 chr）里当前只有 3 处真走索引** ⇒ L1/L2 覆盖仍属边角；`bd` 清单"收益最大"的排序在"只改 3 处"的前提下未必成立。

- 改后仍有 **63 处 `online.iter().find(|e| e.actor_id == …)`**、
  **43 处 `online.iter().find(|e| e.chr_name == …)`**；
- 已索引 3 处：`handle_say_with_saves` 的 chr（说话者）与 actor（`RM_HEAR` 收件人）、
  `send_ref_msg_to_targets` 的 actor（扇出路由）。

| 类别 | 现场 | 每 tick 次数形态 | 是否已走索引 |
| --- | --- | --- | --- |
| A. 扇出路由 | `send_ref_msg_to_targets`（`broadcast_ref_msg*` / 走位候选集） | Σ每次广播 收件人数 | 是 |
| B. 说话 | `handle_say_with_saves` | 说话条数 × (1 + 收件人数) | 是 |
| C. 逐 tick 排水 | ~35 处 `drain_*`（AOI/战斗/buff/merchant/run/耐久/等级…） | Σ每类事件数 × 每事件 1–2 次 | 否（线性） |
| D. 入站命令 | ~20 个 `handle_*_cmd` 各 1 处 chr | 命令条数（活跃玩家 × 命令率） | 否 |
| E. 常量/低频 | `sim_loop` 自身 3 actor + 5 chr/tick；`online_find_for_kick`（按网关扫 Vec） | 常数 / 每会话 | 否 |

## 2. 探针字段（一行/tick，TSV）

```text
tick actor_indexed chr_indexed actor_linear chr_linear refmsg_calls refmsg_targets
say_calls say_recipients kick_scans
```

- `*_indexed`：`find_by_actor` / `find_by_chr` 入口 +1；
- `refmsg_calls` / `refmsg_targets`：`send_ref_msg_to_targets` 调用次数与 `targets.len()` 之和；
- `say_calls` / `say_recipients`：`handle_say` 条数与广播收件人数；
- `*_linear`：C/D 类 `online.iter().find` 用 `online_lookup!(tag, …)` 包装宏机械替换后计数
  （只加计数，不改查找与顺序）；
- `kick_scans`：`online_find_for_kick` 的扫描长度。

**开关**：`MIR2_PERF_COUNTERS=<path>` 才启用；默认关闭时零分配、分支可预测。
tick 尾写 TSV；env 设了但文件未产生 ⇒ rc≠0（fail-closed，防"探针没开还报数"）。
探针 commit 独立于产品语义 commit。

## 3. 跑法

- release 真栈 + botload，C ∈ {500, 986, 1000, 2800}，每档 ≥300 tick；报 p50/p95 与**每 tick
  分项计数**（不是只报均值）。
- 同时用 `cargo test --release` 重跑 `online_lookup_micro_index_vs_linear`，取 **release** 的
  `L_a/L_c`（线性）与 `I_a/I_c`（索引）；决策只用 release 差值。

## 4. 理论上限

```text
Δms/tick(n) = N_a·(L_a(n) − I_a) / 1e6 + N_c·(L_c(n) − I_c) / 1e6
```

今天 debug 实测差值**只作代入示例**（决策前换 release）：

| n | Δ actor | Δ chr |
| --- | --- | --- |
| 986 | 832.6 ns | 2675.7 ns |
| 2800 | 2667.6 ns | 7210.2 ns |

达到"预算 5%"所需每 tick 次数：

| 预算口径 | n=986 actor / chr | n=2800 actor / chr |
| --- | --- | --- |
| 5% × 200ms = 10ms | 12,010 / 3,737 | 3,749 / 1,387 |
| 5% × 100ms = 5ms | 6,005 / 1,869 | 1,875 / 694 |

（actor/chr 可相加；chr 单次更贵、所需次数更少。若 C/D 类合计远低于表中数值，索引吃不满 5%。）

## 5. 判据（先定，再看数）

1. 实测 `Δms/tick` ≥ 预算的 5%（200ms 口径 10ms，或 100ms 设计点 5ms）⇒ 方向成立；
   报告必须点名**哪一类贡献了绝大部分**。
2. <5% ⇒ 按总控的话回改 `bd` 清单排序，不再加索引；先查是否某单一类（如 D 入站命令）
   占大头——若是，只优化那一类，不扩大面。
3. 若 A 类收件人数大而 C/D 小 ⇒ 下一步不是"更多 HashMap"，而是**每 tick 的
   收件人→(gate,socket) 快照/缓存**（减少每收件人一次查找）；另立卡，不并入本卡。
4. 单次 129 ns 是**下限**（hash + 边界校验）；收益必须乘 N，"零成本"不成立。

## 6. 实现状态（2026-11-21，follow-up）

- `perf_counters` 模块（默认关闭；关闭态每入口一次 `Relaxed` 读、无分配/无锁/无字符串）。
- 117 处 C/D 类线性调用点改 `find_linear_actor` / `find_linear_chr`（只包计数，查找语义与
  `entries.iter().find` 逐字相同，**未改成索引**）；3 处索引查找、`send_ref_msg_to_targets`、
  `handle_say`、`online_find_for_kick` 各自计数。
- `MIR2_PERF_COUNTERS` 由 `sim_loop` 启动时打开/截断；每 tick 尾 `write_tick()`；
  启动打不开或关服时 **0 个 tick 行 ⇒ rc≠0**（fail-closed；硬杀进程无法验证，已知边界）。
- 测试：`perf_counters_fail_closed_and_disabled_noop`（env 设了 0 tick ⇒ Err；关闭态 bump no-op）、
  `perf_counters_write_tick_cost`（ignored 微基准）。
- 探针自身开销（debug）：`write_tick` 关闭 **6.47 ns/call**、启用 **2059 ns/tick**（1 行）；
  索引查找在计数启用时 130 → 144 ns（≈+10%，单次噪声较大；线性臂同期也漂移）。
