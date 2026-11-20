# openmir2-firecharm-nondeterminism —— MG-27 `firecharm` golden 不可复现（2026-11-20 立卡）

## 现象（`ae` 独立复现，连跑两次 `--case firecharm` 的 hit 支）

| 字段 | 第一次 | 第二次 |
|---|---|---|
| `Rng` 条数 | 5 | 8 |
| `DrainPasses` | 2 | 4 |
| `TargetHp` | 100 | 85 |
| `NotDue` | 含 8021 到期 | 不等价 |

**本卡不是本次改动引入**：`firecharm.json` 末次改动为 `3886e297b`（重生成批），此后无人动它；
即**既有装置/夹具存在墙钟依赖**。我这侧连续两次运行**结果相同**（三支全等）⇒ 触发条件是时序相关，
不是每次必现。

## 根因假设（待验证）

1. `RM_MAGIC_LVEXP(1000ms)`/`RM_MAGICFIRE` 的到点判定用 `HUtil32.GetTickCount()` 实时比较
   ⇒ 在 600/1200ms 窗口边界上，排水趟数与到点与否会抖动；
2. `NotDue` 记的是"窗口结束时未到期"，其归属取决于两次 `Sleep` 的实际时长（OS 调度）。
⇒ 修法方向：装置改用**注入时钟**（与 Rust 侧 `clock_ms` 同一口径）或在排水时把等待改成
"推进虚拟时钟"而不是 `Thread.Sleep`；不能靠"多跑几次取多数"掩盖。

## 影响

- MG-27 的 `verified` 当前建立在一份**未必可复现**的 golden 上；清单已加 note 留痕，
  **status 未改**（降级由 leader 定）。
- 同源风险需普查：**其它 26 条 case 是否也有同类墙钟依赖**。

## 普查方法（可复跑）

```bash
for c in <case1> <case2> …; do
  dotnet run --project mir2-rs/tests/parity/csharp/MagicGolden -- --case $c --out /tmp/a_$c.json
  dotnet run --project mir2-rs/tests/parity/csharp/MagicGolden -- --case $c --out /tmp/b_$c.json
done
# 比较：剥掉 `delivery` 字段后逐字段相等；不等即登记（记录 Rng 条数/DrainPasses/TargetHp/NotDue 条数）
```

## 验收

- 装置改为确定性时钟后，连续 3 次运行产物逐字段相等（除既有墙钟 `delivery`）；
- 普查清单中每条 case 至少两次一致；
- 修正后 MG-27 的 golden 重新生成，比较器复跑绿。
