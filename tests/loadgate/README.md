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

## 基线件清单（当前入库）

| 目录 | 档位 | 说明 |
| --- | --- | --- |
| `load-20261011-022821/` | 200 / 500 / 1000 | C6 退回补做第 3 轮（修掉“一条命令”三处硬伤后的三档连跑）：**tier-200 GREEN；tier-500/1000 RED**——RED 只卡在跨轮登录爬坡，tick 本身健康（P99 16ms、`internal_errors=0`）。见下“第 3 轮结论与遗留” |

## 第 3 轮结论与遗留（2026-10-11）

本轮修掉的是“一条命令根本跑不完”的三处硬伤（都属于工具链，不是 oracle）：

1. `tools/e2e/stack_e2e.ps1` 把 `start-all.ps1` 的输出接成管道再等退出 ⇒ 服务进程继承管道写端、父进程退出后管道不关，**整条命令永久挂在 “STACK start...”**；改为 `Start-Process` + 文件重定向 + 只等端口（与 `load_gate.ps1` 的 `Restart-Stack` 同源）。
2. 档位经 `powershell -File ... -Tiers 200,500,1000` 传递会被拼成单个整数 **2005001000**（假人数失控、整档 RED）；`stack_e2e` 现在用 `-Command "& '<load_gate.ps1>' ..."` 传。
3. `src/BotSrv` 两处真 bug：① 账号已存在（重跑）时不再卡死，转入登录（复跑幂等，冒烟回到 3/3）；② `ClientManager.RunAutoPlay` 错用 `_clientList[i]`（循环变量属于 `_autoList`，两表长度不一致）⇒ 越界 `IndexOutOfRangeException`，1000 档 `internal_errors` 10 万+、登录卡在 555/1000；改按 `SessionId` 取回假人后 `internal_errors=0`、登录爬到满载。

**遗留（已开新卡）**：tier-500/1000 在**跨轮重跑**时登录爬坡不达标（500→78% / 1000→38.6%），而同一套账号在**首次干净轮**能绿。根因指向“负载门禁跨轮不幂等”——持久库里的账号状态（锁定/已存在角色/开号计数递增 `2200→3200→4200→5200`）跨轮累积。**这是负载客户端的幂等问题，不是服务端/Rust 的能力上限**。给 M5 用的门禁需要能做到“同一命令重复跑得到同一结论”，因此单列后续卡处理；在此之前，引用本目录基线时只把 tier-200 当 GREEN 证据。

每档固定四件：`load_stats.ndjson`（BotSrv 每 5s 累计快照）、`mem.ndjson`（各服务进程 WS/私有字节/CPU）、
`report.json`（该档归算 + verdict + fails）、`load_gate_summary.json`（全档汇总，在目录根）。
`bots.log` / `bots.err.log` / `stack-*.log` 是过程日志，**不入库**（`.gitignore: tests/loadgate/**/*.log`）。
三档曲线与判据表由 `load_report.py aggregate` 生成到 `SUMMARY.md`。

## 三个新增读数（2026-10-11 起，必须一起读）

| 字段 | 含义 | 为什么必须一起读 |
| --- | --- | --- |
| `in_world` / `world_ok_pct` | 收到 `SM_LOGON`（真进世界）的假人数与占比 | **它是 tick 样本的覆盖基数**：tick 只可能来自进世界的假人。本工具假人的世界态建立在跨假人共享的 `MShare.*` 上，实测 200 档只有 ~10% 能进世界 ⇒ tick 是"少数人身上量的"，样本量（不是分辨率）是它现在的短板 |
| `probe_sent` | 探针实际发出的动作数 | 与 `samples` 对比才知道"发了没回 ack"（服务端不认）还是"根本没发"（没武装/没进世界）——这两种故障的修法完全不同 |
| `internal_errors` | 假人侧被兜住的异常数 | >0 说明假人过程有异常，读 P99 时要一起看 |

`tick_missing` / `tick_missing_acknowledged` / `missing_tick_note`：**缺 tick 样本默认判 RED**（与 `--criteria` 解耦）；
只有显式加 `-AllowMissingTick` 才放行，且 `report.json` 里会留下"本次未采集 tick P99"的标注——
**带这个标注的基线不得当作 M5 的 tick 判据引用**。

## 如何复跑（一条命令，三档含干净栈）

```powershell
# 前置：目标账号必须已建好且在 LoginSrv 启动之前
powershell -ExecutionPolicy Bypass -File tools/client/account_provision.ps1 -Prefix loadbot -Count 1000

# 三档：每档都从干净栈开始（-RestartStackPerTier 现在**含第一档**），跑完自动归算 + 汇总
powershell -ExecutionPolicy Bypass -Command "& 'tools\botload\load_gate.ps1' -Tiers 200,500,1000 `
  -HoldSec 60 -RestartStackPerTier -OutDir 'tests\loadgate\load-<时间戳>'"
python tools/botload/load_report.py aggregate --summary tests/loadgate/load-<时间戳>/load_gate_summary.json
```

**注意数组参数的传法**：`-Tiers 200,500,1000` 必须经 `-Command "& …"` 传（`powershell -File` 会把逗号列表
拼成一个整数 ⇒ 档位变 `2005001000`、假人数失控，2026-10-11 实测踩过）。红检：
`load_report.py selftest`（含"缺 tick 默认判 RED"回归项）与 `load_gate.ps1 -SelfTestRed`。
