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
| `load-20261011-022821/` | 200 / 500 / 1000 | 第 3 轮（修掉“一条命令”三处硬伤后的三档连跑）：**tier-200 GREEN；tier-500/1000 RED**——RED 卡在跨轮登录爬坡，当时未定位。**告警：tier-1000 的 `load_stats.ndjson` 已被下一轮冒烟覆写（见「证据完整性告警」），该档结论只引 `report.json`** |
| `load-20261011-030205/` | 200 / 500 / (1000) | 干净库重跑（诊断轮）：tier-200 GREEN（login 100% / samples=912）、tier-500 RED 70.6%（peak 353/500 / internal_errors=125）——用来证伪“开号重复=爬坡根因”。**部分产物**：tier-1000 只有 `load_stats.ndjson`（无 `report.json`/`mem.ndjson`），整目录无 `load_gate_summary.json`（该轮被中止）⇒ 仅 `load_stats.ndjson` 可用 |
| `load-20261011-104822/` | 500 | 第 4 轮 **T-9 修前**诊断：`login_ok=179/500`，`ident_counts[529]=500`（服务端全回）vs `520=530=179`（假人侧只处理了 179）——把“服务端吞吐上限”证伪 |
| `load-20261011-114831/` | 200 / 500 / 1000 | 第 4 轮 **T-10 修前**：三档 verdict GREEN 但 `internal_errors` 非零（500:159 / 1000:16，自动挂机越界——见图下根因）。**告警：tier-1000 的 `load_stats.ndjson` 已被下一轮冒烟覆写，`internal_errors=16` 见 `report.json`** |
| `load-20261011-121254/` | 200 / 500 / 1000 | 第 4 轮 **验收 A**（最终代码）：三档 GREEN、`login=100%`、`conn_lost_steady=0`、`internal_errors=0`、`samples>0`；同目录 `stack_e2e.out.txt` = 整条命令 stdout（含开号幂等行）。**告警：tier-1000 的 `load_stats.ndjson` 已被下一轮冒烟覆写（该档结论引 `report.json` + `stack_e2e.out.txt`）；tier-200/500 的 ndjson 与 report 自洽** |
| `load-20261011-121953/` | 200 / 500 / 1000 | 第 4 轮 **验收 B**（同一命令第二次）：三档 verdict 与 A **完全一致**、`samples>0`、`internal_errors=0`；三档 ndjson 与 report **自洽**（未被覆写） |

## 证据完整性告警（2026-10-11 自查，C6 原卡回复轮）

**现象**：`load-*/tier-1000/load_stats.ndjson`（每轮**最后一档**）被**下一次运行**的 bot-smoke（3 假人）覆写，与同目录 `report.json` 不同源。

实锤（逐档对照 `report.json` / `stack_e2e.out.txt`）：

| 目录 | 入库 `tier-1000/load_stats.ndjson` | 同目录 `report.json` | 判定 |
| --- | --- | --- | --- |
| `load-20261011-022821/` | 36 行，全程 `spawned=3`、`tick_samples=0` | login 38.6% / peak 386 / samples 6066 | **被覆写** |
| `load-20261011-114831/` | 3 行，`spawned=3` | login 100% / peak 1000 / internal_errors 16 | **被覆写** |
| `load-20261011-121254/`（A） | 3 行，`spawned=3`、`tick_samples=0` | login 100% / peak 1000 / samples 877 | **被覆写** |
| `load-20261011-030205/` | 65 行，`spawned=1000`、internal_errors 3463 | （该档无 `report.json`） | 自洽 |
| `load-20261011-121953/`（B） | 18 行，`spawned=1000` | login 100% / samples 765 | 自洽 |

**根因**：`load_gate.ps1` 每档把 `src/BotSrv/bin/Release/load_out.conf` 写成该档目录（“Env 不继承”的双保险），跑完**不清除**；`run_bots.ps1` 原先不设 `MIR2_BOT_OUT`，BotSrv 的 `LoadMetrics.ResolveOutDir` 于是回落到该 conf（指向**上一轮**的最后一档）⇒ 3 假人冒烟的 `load_stats.ndjson` 以 `FileMode.Create` 覆写归档件。时序与 `load_out.conf` 现值（指向 `121953/tier-1000`）可复核。

**已修**：`run_bots.ps1` 显式 `$env:MIR2_BOT_OUT = <本次运行统计目录>`（`tests/botload/bots-<时间戳>/`），不再读 `load_out.conf`——本轮 `-SelfTestRed` 实跑后 `load_out.conf` 与 `121953/tier-1000/load_stats.ndjson` 的 sha256 均未变。

**影响与引法**：上表「被覆写」三档的 tier-1000 结论**不得引 `load_stats.ndjson`**，只引同目录 `report.json`（A 轮另有 `stack_e2e.out.txt`）；tier-200/500（非最后一档）不受影响。**B 轮三档自洽**，是“同一命令两连跑 verdict 一致”的首选证据。

**为什么不再复算**：`report.json` 是运行当时由正确的 ndjson 归算的，数值可信但源件已被覆写；本轮端口被其它线占用，不起栈重跑。若后续要恢复可复算性，可重跑 A 轮并把 `tier-1000` 完整入库。

## 第 3 轮结论与遗留（2026-10-11）

本轮修掉的是“一条命令根本跑不完”的三处硬伤（都属于工具链，不是 oracle）：

1. `tools/e2e/stack_e2e.ps1` 把 `start-all.ps1` 的输出接成管道再等退出 ⇒ 服务进程继承管道写端、父进程退出后管道不关，**整条命令永久挂在 “STACK start...”**；改为 `Start-Process` + 文件重定向 + 只等端口（与 `load_gate.ps1` 的 `Restart-Stack` 同源）。
2. 档位经 `powershell -File ... -Tiers 200,500,1000` 传递会被拼成单个整数 **2005001000**（假人数失控、整档 RED）；`stack_e2e` 现在用 `-Command "& '<load_gate.ps1>' ..."` 传。
3. `src/BotSrv` 两处真 bug：① 账号已存在（重跑）时不再卡死，转入登录（复跑幂等，冒烟回到 3/3）；② `ClientManager.RunAutoPlay` 错用 `_clientList[i]`（循环变量属于 `_autoList`，两表长度不一致）⇒ 越界 `IndexOutOfRangeException`，1000 档 `internal_errors` 10 万+、登录卡在 555/1000；改按 `SessionId` 取回假人后 `internal_errors=0`、登录爬到满载。

**遗留（已开新卡 `openmir2-c6-loadgate-idempotent`）**：

- 开号确实不幂等，且已修：`account.Account` 上**没有唯一索引**（只有非唯一的 `_WA_Sys_FLD_LOGINID`），所以旧的 `INSERT IGNORE` 每跑一轮就重复插 1000 行——loadbot0 累积到 6 行、全表 11354 行 / 7151 不同名（已去重回 7151）。**内容冻结不允许给 oracle 表加唯一索引，所以幂等只能在开号 SQL 里用 `WHERE NOT EXISTS` 做**（已改，现在跑一轮只报 `account=1000`，不再增长）。
- 但**去重 + 开号幂等并没有把 tier-500/1000 拉绿**：在干净（无重复）库上重跑，tier-500 仍卡在 **~353/500（70.6%）**、tier-1000 更低，`internal_errors` 0~125、tick 本身健康（P99 16ms、样本充足）。说明除「账号重复」外，还有一个**负载客户端/服务端的并发登录上限（~350–390/500）**未被定位，因此**不能**把 RED 归结为已修项。
- 结论：tier-200 可绿且可复跑（P99 16ms、登录 100%、`internal_errors=0`），可直接给 M5 当「同屏 small」基线；tier-500/1000 的登录爬坡上限是**独立的未解项**，需单独定位（建议下一步查 LoginSrv/LoginGate 的并发上限与 BotSrv 登录错峰/重试，而不是继续查开号）。
- **历史结论更正（2026-10-11 第 4 轮后）**：commit `383eb916` 的根因判断——“开号重复是跨轮登录爬坡变差（500:78% / 1000:38.6%）的根因”——**已被 `030205` 干净库重跑推翻**：去重 + 开号幂等之后 tier-500 仍 RED 70.6%（peak 353/500）。真正的根因在假人侧（跨假人共享 `MShare.MySelf` 判据，T-9），不是开号重复。

> **第 4 轮已把上述“遗留”定位并修掉**：登录上限的真因在**假人侧**（T-9），不是服务端吞吐。见下。

## 第 4 轮结论（2026-10-11）：跨轮幂等 + 登录上限根因（T-9）+ 边界越界（T-10）

验收命令（**同一命令连跑两次**，`-LoadTiers` 写成逗号串 —— 第 4 轮已把该参数改成 string 接收，
`-File` 下不再被当千分位拼成 2005001000）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/e2e/stack_e2e.ps1 `
  -ProvisionAccounts -LoadTiers 200,500,1000 -SkipFlow -StopStackAtEnd
```

两轮结果（`load-20261011-121254` = A、`load-20261011-121953` = B）：

| 档位 | A verdict | B verdict | A/B `login` | A/B `conn_lost_steady` | A/B `internal_errors` | A/B `samples` |
| --- | --- | --- | --- | --- | --- | --- |
| 200 | GREEN | GREEN | 100% / 100% | 0 / 0 | 0 / 0 | 1033 / 940 |
| 500 | GREEN | GREEN | 100% / 100% | 0 / 0 | 0 / 0 | 777 / 927 |
| 1000 | GREEN | GREEN | 100% / 100% | 0 / 0 | 0 / 0 | 877 / 765 |

开号也幂等：两轮日志均 `BULK_OK ... account=1000 protection=1000`（不再每轮 +1000）。

**根因 1 — 登录上限 ~179/500 是假人侧丢弃（已修，T-9）**：`RobotPlayer.Operate.DecodeMessagePacket`
的“登录期响应”闸门用了**跨假人共享**的 `MShare.MySelf == null`——第一个假人 `SM_LOGON` 把它置上后
再不为 null，其余假人的登录响应全被静默丢弃。`104822/tier-500` 的观测直接证伪了“服务端上限”：
`ident_counts[529]=500`（服务端对全部 500 个回了 `SM_PASSOK_SELECTSERVER`）而 `login_ok=179=ident_counts[520]=ident_counts[530]`。
改成本会话的 `!_inWorld` 后三档 100% 登录（单假人行为逐字节不变）。

**根因 2 — 自动挂机越界 ⇒ `internal_errors` 非零（已修，T-10）**：T-6 只给 `TMap.CanMove` 补了上界守卫，
`GetDoor`/`IsDoorOpen`/`CanFly`/`OpenDoor`/`CloseDoor` 仍会在地图右/下边界外越界。tier-1000 稳态每 tick 抛
`IndexOutOfRangeException`（`114831` 里 `internal_errors` 线性增长）；加完整堆栈后定位：

```
System.IndexOutOfRangeException: Index was outside the bounds of the array.
   at BotSrv.Maps.TMap.GetDoor(Int32 mx, Int32 my) in src/BotSrv/Maps/TMap.cs:line 392
   at BotSrv.Player.RobotPlayer.CheckDoorAction(Int32 dx, Int32 dy) in src/BotSrv/Player/RobotPlayer.cs:line 1934
   at BotSrv.Player.RobotPlayer.ProcessActionMessages() in src/BotSrv/Player/RobotPlayer.cs:line 673
```

统一补 `TMap.InMapRange(cx, cy)` 后两轮三档 `internal_errors=0`。

**工具链第三处同源坑 — 命令跑完却不退出（已修）**：`stack_e2e` 原来用 `& powershell ... 2>&1` 收集
`load_gate` 输出；`2>&1` 是**管道**，load_gate 起的**长驻服务**（GameSvr/网关/MySQL）继承管道写端 ⇒ 父进程
读到 EOF 之前一直挂着（表现：summary 已落盘、栈还在跑、命令永不返回）。改成直接继承 stdout（不建管道）后，
两轮都自行退出。**外部也别用管道包 `stack_e2e`**（`... | tee log` 会复发；请重定向到文件）。

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

**注意数组参数的传法**：`-Tiers` / `-LoadTiers` 自 C6 第 4 轮起都按 **string** 接收并自己拆
（`[,\s]+`）⇒ `powershell -File ... -LoadTiers 200,500,1000` 与 `-Command "& … -LoadTiers 200,500,1000"`
两种写法都正确（旧写法会被当千分位整数拼成 `2005001000`）；单档数值超过上限（>5000，疑似被拼接）会**响亮失败**
而不是静默跑 50 万假人。红检：`load_report.py selftest`（含“缺 tick 默认判 RED”回归项）与 `load_gate.ps1 -SelfTestRed`。
