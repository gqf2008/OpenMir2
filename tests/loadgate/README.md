# tests/loadgate/ —— 压测档位产物（C6 证据）

每次跑 `tools/botload/load_gate.ps1`（或 `tools/e2e/stack_e2e.ps1 -LoadTiers ...`）落一个目录：

```
load-<时间戳>/
  load_gate_summary.json     全档汇总（含口径字符串与每档 report）
  SUMMARY.md                 三档报告（判据表 + SVG 曲线；由 load_report.py aggregate 生成）
  tier-200/
    load_stats.ndjson        BotSrv 每 5s 一行累计快照（spawned/login_ok/conn_*/tick 直方图）
    mem.ndjson               各服务进程 WS/私有字节/CPU 时间序列（默认 2s 一次）
    report.json              该档归算结果（P50/P90/P99/max、登录率、掉线、内存峰值/均值/斜率、判据 fails）
    bots.log                 假人进程 stdout（**不入库**，见 .gitignore）
```

口径（tick 服务时延、为什么每动作一次、对 Rust 侧的硬约束、量化下限）见
[tools/botload/README.md](../../tools/botload/README.md) —— **以那份为单一真源**，本目录只放产物。

读数提醒：
- `P50` 常落在 0~16ms 且**没有意义**（`Environment.TickCount` 粒度 ≈15.6ms），有意义的是**尾部**（P99/max）；
- 判据只看**稳态窗**（登录数首次达标之后）：爬坡期的掉线不计入 `conn_lost_steady`，也不混进 P99；
- 内存看**末段斜率**：`slope_tail_mb_per_s ≈ 0` 才是"没有泄漏"，单看峰值会误判（分配器高水位）。
