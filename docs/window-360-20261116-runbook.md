# 360s 结论窗口 Runbook（2026-11-16 09:00–09:45，45 分钟独占）

> 目标：给出 **500 档 × 360s 稳态**的结论级容量数据（替换外推）；同时补 200 档稳态对照。
> 前置已合入 master `fa8a00539`：sha 归档（rc 三态）+ per-bot MySelf 接线牙 + 错误普查 20 类 + 共享态根因修复。

## 0. 空载判据（总监定义）

- **起跑前 CPU <20%，且全程无外部构建进程**（cargo/rustc/dotnet/zig/msbuild = 0）。
- 「外部」显式：被测栈自身负载的 CPU 峰值不算违规。
- 记录：起跑前 CPU、全程 `heavy_samples.ndjson`（n / names / CPU min-mean-max）。

## 1. 时间线（09:00–09:45）

| 时间 | 动作 |
| --- | --- |
| 08:50 | 总监机器检查放行；本车道自查前置（构建进程、端口、内存） |
| 09:00 | **60s smoke 验流程**：200 档单档新栈、`HoldSec 60`，确认每档新栈流程与 census 全 0 |
| 09:05 | 重建/确认二进制 sha；起 500 档 **新栈** |
| 09:06–09:13 | 500 档 `HoldSec 360`（单档、新栈；BotHeapLimit 8GB，双阈值监视） |
| 09:13–09:44 | **立刻归档并算 sha manifest**、抽取 TICKPROF 稳态段、写报告 |
| 09:45 | 硬截止；未完成则如实报「未完成 + 卡在哪」 |

## 2. 规则

- **每档新栈、单档**；同栈只跑单档（换档离世广播风暴会让后一档进世界慢 4×）。
- 500 档 `HoldSec 360`、`-SkipBuild`、`-MemKillThresholdGB 8`、`-BotHeapLimitGB 8`。
- 内存监视 60s；可用 <3GB 或 BotSrv WS >7GB 立即停，窗口判无效。
- 报告三限定：**census 口径 / 360s 实测稳态 / 空载**；`conclusion_valid` 按**实测**窗口判。
- 报告必列：逐类 census 20 类、`scene_null_actor_skipped`/`scene_rounds`、四项字段、延迟分位（口径=盖戳后投递延迟）、丢弃/分流计数、实体数曲线与**是否饱和**。
- **360s 的 tick 数才可入容量模型**；smoke/短窗的数只能作冒烟。

## 3. 交付

- 归档：`E:/UserFleet/archive/load-360-20261116/`（含 sha manifest；`rc=2` 表示不可判）。
- 报告：`docs/load-360-result-20261116.md` 或卡面 comment；失败也写（含 shutdown 状态）。
