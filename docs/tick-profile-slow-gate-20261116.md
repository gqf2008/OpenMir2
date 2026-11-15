# 慢 tick 诊断子门的环境变量口径（2026-11-16）

卡：`openmir2-host-segment-profiler-500`（tier L）。本文只定义 `MIR2_TICK_PROFILE_SLOW_MS`
的取值口径与「为什么不与 `MIR2_CLOCK_MS` / `MIR2_RNG_SEED` 一样 fail-closed」。

## 1. 这个门做什么

`MIR2_TICK_PROFILE=1` 开启宿主段计时后，`MIR2_TICK_PROFILE_SLOW_MS=N` 再加一层**逐 tick 慢 tick 段快照**：
某个 tick 的工作段（`HostTickTotal − HostSleep`）≥ 阈值时，打印该 tick 的段构成、该 tick 的命名子探针
与未分段残差。它只观测，不改阈值、帧面、RNG、落库；默认关闭。

## 2. 取值口径（与 `clock.rs` / `seed.rs` 对齐的部分）

| 环境变量原文 | 判定 | 门 | 日志 |
| --- | --- | --- | --- |
| 未设 / 空串 `""` | 未启用 | 关 | 无（与 `parse_clock` 的 `None`/`""` 一致） |
| 只含空白 `"   "` / `" \t "` | 非法 | 关 | `WRN`（`clock.rs` 对空白串也走 parse 失败，只有空串才当未设） |
| 显式 `0` / `" 0 "` | 显式关闭，不是非法 | 关 | `DBG`（可审计：「显式关闭」而不是「没设」） |
| 合法正整数毫秒，允许前后空白（如 `" 100 "`） | 启用 | 开 | `INF`（起栈时打印阈值那行） |
| 其余（`10O`、`1.5`、`-1`、`+1`、`100ms`、非 ASCII 数字、ms→ns 溢出） | 非法 | 关 | `WRN`，含变量名与**原文值** |

WARN 形如（`{raw:?}` 保留原文与引号，便于机器 grep）：

```text
[WRN] MIR2_TICK_PROFILE_SLOW_MS="10O" 非法（期望正整数毫秒；空白会被 trim；0=显式关闭），慢 tick 快照已关闭
```

把「没设」「显式关」「设错」区分开，是为了避免把「没探针」误判成「没有慢 tick」——
后者是对容量结论有实质影响的误读。

## 3. 为什么不与 clock/seed 一样 fail-closed

- `MIR2_CLOCK_MS` / `MIR2_RNG_SEED` 是**行为性注入**：非法时若带着默认值继续跑，会产出
  **看似有效、实际不可比**的结果（虚拟时钟没注入 / 随机序列不是预期种子）。因此它们的解析走
  `Err`，由启动路径 fail-closed 退出——宁可不起，也不许「以为注入了、其实没有」。
- `MIR2_TICK_PROFILE_SLOW_MS` 是**纯诊断子门**：默认关、只加日志、不影响任何行为。为一个可选
  开关的 typo 杀掉生产进程，代价与收益不成比例。仓库已有的同族先例是 `rng_trace`：
  诊断 trace 在种子/环境不合法时打印 `[rng-trace][ERROR] …（trace 关闭）` 并**关闭 trace**，
  不退出进程。
- 所以本门采取**同一纪律的降级版**：**绝不静默**（非法值必有 `WRN`，显式 `0` 必有 `DBG`），
  但**不杀进程**。这是「诊断子门 WARN + 关」，不是「可以静默忽略」。

## 4. 牙（变异）与证据

`tick_profile.rs` 的单测钉住三件事：

1. 非法值 ⇒ `threshold_ns() == None` **且** `warn_message()` 含变量名与原文；
2. 显式 `0` ⇒ 门关、无 WARN、`notice_message()` 含变量名；
3. `" 100 "` ⇒ `On(100ms)`；仅空白 ⇒ `Invalid`。

变异重放（每条恢复后 sha256 必须等于原文件）：

| 变异 | 期望 |
| --- | --- |
| 删掉非法分支（`Invalid` → `Unset`） | `slow_gate_invalid_value_warns_and_stays_closed` 红 |
| 删掉 `trim()` | `slow_threshold_parse_trims_then_fail_closed` / `slow_gate_trims_like_clock_and_seed` 红 |
| 删 `end`/`probe` 的 slow 门守卫 | `slow_gate_off_records_no_per_tick_rows` 红 |
| 阈值判定改恒真 / 删跨 tick 清空 | `slow_gate_snapshot_only_over_threshold` 红 |

## 5. 与「默认关闭零开销」的关系

主门 `MIR2_TICK_PROFILE` 未开时，`HostTickProfile::from_env` 连 `MIR2_TICK_PROFILE_SLOW_MS` 都不读；
主门开启但 SLOW 门关闭时，`end`/`probe` 每段只多一次 `Option::is_some` 判别，不取时钟、不分配、
不打日志。慢 tick 开启时每 tick 使用复用缓冲，容量稳定后不再分配。

## 6. 采集建议

- 容量窗口用 `MIR2_TICK_PROFILE_SLOW_MS=100`（与「尾部 tick ≥100ms」口径一致），并用窗口汇总的
  逐 tick 分位数交叉核对是否漏采（阈值过高会看不到 P99 以下）；
- 起栈日志必须能看到「TICK_PROFILE 慢 tick 段快照开启：工作段 ≥ 100 ms」或非法值的 `WRN`——
  两者都没有时，先怀疑环境变量没传进进程，而不是「没有慢 tick」。
