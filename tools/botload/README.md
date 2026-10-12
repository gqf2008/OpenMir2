# tools/botload/ —— 假人压测 / 载荷门禁（C6）

对应设计文档 §6 **M5 压测三档**、§4.2.1「用假人量单线程世界的 tick P99 曲线」。
三个件：`run_bots.ps1`（单档冒烟，C1 就有）、`load_gate.ps1`（档位编排）、`load_report.py`（指标归算）；
外加 `verify_artifacts.py`（复核 F9 的 `artifacts.sha256.json` 产物清单，字节口径，见
`tests/loadgate/README.md》产物清单哈希口径与复核`）。
整栈一条命令见 `tools/e2e/stack_e2e.ps1`。

**M3 总验收 DoD② 判据 (b) 的机判器**：`in_world_stability.py`（卡 `openmir2-m3-final-acceptance`）。
长跑判据「假人 `in_world` 曲线在稳态段无跌落」里，连接数一侧归 `load_report.py` 的 `conn_lost_steady == 0`；
本工具判 `in_world` 一侧：稳态段（`[t0+ramp, t1-tail_ms]`）按 `bucket_ms` 分桶，判 `min(桶均值) >= min_ratio × max(桶均值)`。
缺 `in_world`/`t_ms` 一律 PREREQ（rc=2，fail-closed，不静默当绿）。

```powershell
python tools/botload/in_world_stability.py <load_stats.ndjson> --ramp-ms 3600000 --bucket-ms 300000 --tail-ms 300000
python tools/botload/in_world_stability.py selftest      # 7/7（改坏一桶⇒RED；缺字段/坏 JSON⇒PREREQ；无稳态段⇒PREREQ）
```

```powershell
# 整栈一条命令：停栈 → 批量开号 → 起栈 → 工具自测+金标准 → 压测档位 200/500/1000 → 收尾/停栈
# （C6 第 5 轮的验收命令：同一命令 3 连跑取最差；tier<1000 登录 100%、tier≥1000 ≥ 95%，见下《阈值与理由》）
powershell -ExecutionPolicy Bypass -File tools/e2e/stack_e2e.ps1 `
  -ProvisionAccounts -LoadTiers 200,500,1000 -SkipFlow -Repeat 3 -StopStackAtEnd

# 只跑单档（栈已在跑）
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -Tiers 200 -HoldSec 60

# 归算（也可单独跑）
python tools/botload/load_report.py summarize --stats <tier>/load_stats.ndjson --mem <tier>/mem.ndjson --count 200
python tools/botload/load_report.py selftest         # 口径自测（含尾部敏感/改坏必变）
python tools/botload/verify_artifacts.py tests/loadgate/load-*/artifacts.sha256.json  # 复核产物清单（字节口径；期望 105/105, rc=0）
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -SelfTestRed   # 红检：无监听端口必须判红
```

## 口径（与 M2 的 p99-baseline **同一份**，两侧共用）

| 指标 | 定义 | 来源 |
| --- | --- | --- |
| **tick 服务时延** | 假人收到明文动作帧 `#+GD/<rtime>!` 的时刻 − 帧里的 `rtime`；两者都是 `Environment.TickCount`（**开机毫秒、全机同域**）⇒ = 「服务端取时间戳 → 客户端收完这一帧」（本机环回，无网络时延），反映世界线程/发送队列被拖住的程度 | `M2Share.GetGoodTick` = `HUtil32.GetTickCount()` = `Environment.TickCount`；BotSrv 内 `LoadMetrics.Tick()` |
| P50/P90/P99/max | 稳态窗（登录数首次达标之后）的直方图分位数，1ms 桶；爬坡期单列不混入 | `load_stats.ndjson` 相邻两行差分 |
| 登录成功率 | 稳态窗内 `login_ok` 峰值 ÷ 档位人数 | `LoadMetrics.LoginOk()`（不抓日志行：原日志只在"无期限提示"分支打印，会漏） |
| 掉线 | 登录后 socket 关闭/超时（`ConnectionReset`/`TimedOut`/其它）；与"连不上"（`ConnectionRefused`）分开；判据只看稳态窗 | `LoadMetrics.ConnLost()` / `ConnRefused()` |
| 内存曲线 | 各服务进程 WS / 私有字节 / CPU 时间序列；峰值、均值、**末段斜率(MB/s)**（判 M5 的"最后一段斜率≈0"） | 驱动侧按 `-MemSampleSec` 轮询 `Get-Process` |
| 上线速率 | `ConnectStaggerMs`（第 i 个假人等 i×该值）——**它是压测口径的一部分**，报告里必须记录 | `RobotOptions.ConnectStaggerMs`（默认 3000 = 原行为；档位压测取 20~50ms） |

**量化下限（必须一起读）**：`Environment.TickCount` 在 Windows 上粒度约 **15.6ms** ⇒ 单样本有 ±16ms 量化。
所以 P99 的"低端"没有意义（P50 常落在 0~16ms），**有意义的是尾部**（世界被拖住时整段上移）。
报告里以 `clock_granularity_note` 显式带出。

**为什么"每动作一次"而不是"每 tick 一次"**：tick 边界是**实现内部**概念（Rust 侧单线程世界循环更明显），
而"动作 → 回帧"是**线上可观测**的（§6.0-1 要的就是"命令 + 阈值"）。要更高压力就加假人或加动作速率，
口径不变；若将来确实需要 tick 级内部指标，单列一条曲线，别与线上 P99 混。

## ⚠️ 对 Rust 侧（M2/M5）的硬约束 —— 否则本曲线不可比

**Rust 实现必须同样发出 `#+GD/<rtime>`，且 `rtime` 用同一时钟语义**：
- 时钟：开机毫秒（等价 `Environment.TickCount`），**不是** Unix 毫秒、也不是"从启动起算的 tick 序号；
- 语义：**"被接受的客户端动作 → 回一帧"**（一对一；拒绝/超速走 `+FL/<t>` 或 delay 消息）；
- 文法：`+` `G` `D` `/` + 十进制正整数（协议层 `frame::parse_act_frame` 已按第四种 s2c 帧形态建模，
  发送模板 `MessageSettings.cs:6 sSTATUS_GOOD = "+GD/{0}"`，C3 金标准 31 条 act 帧已逐字节复现锚定）。

不满足以上任一条 ⇒ 同一条假人在 Rust 侧量出的数值与 C# 基线**不可比**（本门禁会失效，而不是变红——注意这点）。

## 判据（§6 M5）

| 判据 | 阈值 | 出处 |
| --- | --- | --- |
| 登录成功率 | **tier ≥ 1000：≥ 95%（可复现下界）；tier < 1000：100%** | §6 M5-3 + owner 裁决 `11dbde39`（见下《阈值与理由》） |
| 稳态掉线 | **0** | §6 M5-3 |
| tick P99 | **≤ 100ms** | §6 M5-3 |
| 内存末段斜率 | ≈ 0（M5 长跑另判：峰值-均值 < 5%） | §6 M5-2 |

`load_gate.ps1 -Repeat N` 逐档逐轮判 GREEN/RED，再**按档位取 N 轮里最差**：`worst_login_pct ≥ 阈值`
**且每一轮的 conn/tick/internal 都 GREEN** 才算该档 GREEN；任一档 RED ⇒ 退出码 1。

## 阈值与理由（2026-10-11 第 5 轮；owner 裁决 `11dbde39` ①）

**tier-1000 登录判据 = 同命令 3 连跑取最差 ≥ 95%**（`load_gate.ps1 -Repeat 3`，或 `stack_e2e.ps1 -Repeat 3`）；
其余档位仍是 100%。

为什么不是「100%」：

- 被测栈是 **C# 部署栈**（`tools/e2e/stack_e2e.ps1` → `E:\MirServer\start-all.ps1`），**不是 Rust 侧**；
  改它等于改 oracle 行为（红线）。
- 两轮独立实测已证否「100%」：reviewer-3 在自己检出上同命令两连跑，A 三档全 GREEN；B 的 tier-1000 =
  **94.8%（948/1000）**、爬坡未完成 ⇒ RED（`cdea5e32` / `67af4cac`）。第 5 轮三连跑 tier-1000 登录 =
  100%/100%/100%（`load-20261011-155529`）⇒ 登录率**通常**是 100%，95% 是给「偶发丢包窗口」留的下界。
- T-11 把丢包**方向**证出来（`tests/loadgate/README.md》残余丢包方向`）：无应答连接 `rx_bytes==0`、
  同连接重发也不回 ⇒ **不是假人侧收包/解析丢失**，是服务端在这条连接上没回。但「服务端为什么没回」
  （C# 栈自身的 1000 并发上限，还是同机并发编译造成的 CPU 饥饿）**未归属**，必须空载窗口复测才能定。
  复现件：`load-20261011-162213/run-3`（865/1000）、`load-20261011-132401/run-4`（646/1000）。
- 95% 是**可复现的下界**（3 连跑取最差），不是把已知失败当通过：它把「波动区间」与「回归」分开——
  真正的回归（如 T-9 那类系统性丢弃，tier-500 179/500 = 35.8%）会被 95% 稳稳抓住（见《红检》）。

阈值与理由同时写在 `load_gate.ps1` 的 `Get-LoginThreshold` 与 `load_report.py` 的
`LOGIN_THRESHOLD_BY_MIN_TIER`（注释），二者必须一致；`load_report.py selftest` 有断言。

### 本机负载探针与「独占窗口」（owner 裁决 `82f8e0c3` ④）

`load_gate.ps1` 每档采 `hostload.ndjson`（CPU=`Win32_Processor.LoadPercentage` 均值 + 重型进程数
`rustc/cargo/cargo-clippy/msbuild`，口径照 tickbench `probe_load_window`），`load_report.py` 归算成
`report.json.host_load`（`cpu_mean/cpu_peak/heavy_procs_max/heavy_names_seen/loaded_window`），并写进
`SUMMARY.md` 的 verdict 表。**只标注、不判红。**

该 gate **必须在没有其它车道编译/压测的窗口跑**：tick P99 = 「服务端 rtime → 客户端收帧耗时」，
本机并发编译会让 P99 出现 125/201ms 这种离散度，且**目前无法归属**（C# 参考栈 vs 同机负载）。
`host_load.loaded_window=true`（CPU 峰 ≥ 40% 或 heavy 峰 ≥ 4）⇒ 该轮 P99 **未归属**，报告里必须写明
「待空载窗口复测」。`load-20261011-155529`（与 m2svr/wbevy 并行）、`load-20261011-162213`（与 wbevy 并行）
两轮即属加载窗口；安静窗口的 P99 复测登记为后续项。

### M5 tick 判据的现状（写实）

**§6 M5 的「tick P99 ≤ 100ms」目前在任何栈上都未验证通过**——C# 参考栈 1000 档实测 125/201ms
（加载窗口内，归属待空载复测）；待 Rust 栈接管后（M5 灰度）复测。另：`in_world` 实测只有 ~10%
（tier-500/1000 稳态 15~26）⇒ **不得**用这三档宣称「M5 三档：1000 假人在线」；「假人真正进世界」
是 M5 收尾缺口（归宿：卡 `openmir2-botsrv-world-evidence`）。

**实测（第 5 轮三次整命令，均 `-Repeat 3`）**：`load-20261011-155529`（与 m2svr/wbevy 构建并行）、
`load-20261011-162213`（与 wbevy 构建并行）、`load-20261011-165805`（带 host_load 探针；见
`tests/loadgate/README.md》第 5 轮结论` 的 3 连跑表）——tier-200/500 各轮全 GREEN；tier-1000 的**登录率**
通常 100%（A），但仍有**两条独立波动源**：① `tick P99` 在 tier-1000 上多轮越界（A：125/201ms）——加载窗口内
**未归属**，待空载复测（C6-3）；② 服务端在 1000 并发登录下会停摆不恢复（C r3：865/1000 无应答、`rx_bytes=0`）——
方向已证为服务端未回，触发条件未归属（C6-4）。两条都不归因于登录阈值，均已登记在
`tests/loadgate/README.md》第 5 轮遗留`；判据未授权改的不动。

### 探针样本缺口与落点偏置（F5，2026-10-11 第 5 轮登记）

`probe_sent`（探针实际发出的 `CM_TURN` 数）与 `samples`（稳态窗 tick 样本数）之间的缺口：

1. **落点偏置（主因）**：`RobotPlayer.ProbeActionTick` 的 `CM_TURN.Recog = MakeLong(_probeX,_probeY)`，
   而 `_probeX/_probeY` 是 `MarkInWorld`（收到本假人 `SM_LOGON`）时记下的**入图落点**；服务端
   `ClientChangeDir` 只在 `nX==CurrX && nY==CurrY` 时才回 `#+GD`。假人一旦移动（自动挂机/被推/切图），
   落点就不等于服务端坐标 ⇒ 该假人的后续探针**全部被服务端判 false**（发了不回 ack）。
2. **窗口边界**：稳态窗起点之前发出的探针不计入。

实测缺口（`probe_sent / samples`）：`load-20261011-155529` tier-200 621/569、tier-1000 1086/775；
`load-20261011-165805` tier-200 1206/916。⇒ tick P99 是「仍停在入图落点的那部分假人」的样本，
**不是全体 1000 假人**；与 `in_world`（覆盖基数）一起读。修法（按本假人当前坐标刷新 Recog）登记为后续项。

### 「同一命令多连跑」的表述（owner 裁决 `11dbde39` ③）

「两连跑 verdict 一致」的说法**作废**，改为「两连跑的**判定规则**一致（重跑不产生 GREEN/RED 翻转）」；
门禁按 `-Repeat N` 一次跑 N 轮并取最差。第 5 轮的 3 连跑 verdict 表见
`tests/loadgate/README.md》第 5 轮结论`。

## 红检（改坏必红）

- `load_gate.ps1 -SelfTestRed`：指向无监听端口 ⇒ 爬坡不可能完成 ⇒ 必须 `verdict=RED`（脚本自身退出 0 = 红检通过）。
- `load_report.py selftest`：分位数正确 / **尾部敏感**（挪走 20 个慢样本 P99 必须 150→5）/ 登录没达阈值判红 /
  掉线分爬坡与稳态 / 内存斜率（线性 1MB/s vs 平坦 0）/ **阈值牙齿**（同一份 960/1000 产物：阈值 100 ⇒ RED、
  阈值 95 ⇒ GREEN，判定必须随阈值变）/ **残余丢包方向判据**（`rx_bytes==0` ⇒ 上游；`rx_bytes>0` 且无 `529` ⇒ 假人侧；
  分窗与重发对照计数）。
- 阈值本身的真实红检（原始输出见 `tests/loadgate/README.md》第 5 轮·红检`）：把 tier-1000 阈值**临时改回
  100%**（`load_gate.ps1 ... -LoginThresholdPct 100`，或对 `<100%` 的入库 ndjson 用 `load_report.py summarize
  --login-threshold 100` 重判）⇒ 该档必须 RED；阈值还原（95%）后同一份数据回到 GREEN。

## 实机踩过的坑（都已修，写下来免得重踩）

1. **开号必须在 `LoginSrv` 启动之前**：它在启动时把账号读进内存，之后再建的账号**登录时看不到**
   ⇒ 顺序必须是「只起 MySQL → 开号 → 起整栈」。`start-all.ps1 -Stop` 会把 MySQL 一起停掉，所以
   `stack_e2e.ps1 -ProvisionAccounts` 里单独起了 MySQL 再起整栈。
2. **MariaDB 的 `seq_1_to_N` 要有默认库上下文**：`INSERT ... SELECT ... FROM seq_1_to_1000` 会报
   `ERROR 1046 No database selected`，必须写成 `mir2_account.seq_1_to_1000`。
3. **子进程的 stderr 会被父进程的 `$ErrorActionPreference="Stop"` 当成终止错误**：表现为"整条链卡住/无输出"。
   `stack_e2e.ps1` 里所有子脚本调用统一走 `Invoke-Child`（临时 Continue + 只看退出码）。
4. **一档结束时成百上千假人同时断连 = 网关抖动**，而本工程实机证明过"网关抖动 → GameSvr 网关槽位
   `UserList` 置空 → 进世界玩家在 `SetGateUserList` 上 NRE 死循环"。所以多档连跑要加
   `-RestartStackPerTier`（`stack_e2e.ps1` 的多档路径会自动带上）：宁可每档多花 ~40s 重启，也不拿被污染的栈当基线。
5. **采样点要放在全局去重之前**：`ProcessActMsg` 里 `MShare.g_rtime` 是**跨假人共享**的去重值，
   采样若放在它后面，同一毫秒内其它假人的样本会被吞掉（`LoadMetrics.Tick()` 放在其前）。
6. **登录成功计数别抓日志行**：BotSrv 的「帐号登录成功！」只在"无无期限提示"分支打印 ⇒ 会漏计数。
7. **同一栈上连跑两档会被网关抖动污染**（2026-10-10 实测）：第一档结束时成百上千假人同时断连，
   第二档的假人**能登录但进不了世界**（日志里「成功进入游戏」= 0，`tick_samples` 恒为 0）。
   与设计文档记的"网关抖动 → GameSvr 网关槽位 `UserList` 置空"同源。⇒ 档位之间必须整栈重启
   （`-RestartStackPerTier`；`stack_e2e.ps1` 多档自动带上）。**看到"登录 100% 但 tick 样本 0"先查这个**。
8. **假人进图后挂机定时器会被停掉**：PlayScene 的 `SM_NEWMAP` 分支执行
   `TimerAutoPlay.Enabled = false`（"地图跳转，停止自动挂机"），此后假人静止 ⇒ 没有动作 ⇒ 收不到
   `#+GD` ack ⇒ tick 样本恒为 0。已在 BotSrv 加 `EnsureAutoPlay()`（**不 toggle**；`OpenAutoPlay()` 是开关，
   压测里盲调会来回切）并在 `ClientManager.RunAutoPlay` 每轮重臂。
9. **假人工具自己会崩，而且崩一次废一档**：实测两处——`TMap.CanMove` 只挡负方向不挡上界（挂机走到
   地图边界外越界）、`RobotPlayer.AttackTarget` 解引用共享全局态 `MShare.MySelf`（地图切换窗口为 null，
   调用点守卫挡不住**竞态**）。两处已补守卫；另外 `AppService.Run` 每轮包 try/catch 并把异常计数写进
   `load_stats.ndjson.internal_errors`（前 20 条打日志）——**压测进程不许被单个假人打死**。
   读数时 `internal_errors > 0` 要一起看，别只看 P99。
10. **别用"杀任务树"的方式停外层脚本**（2026-10-10 实测）：编排脚本是被后台任务启动的话，
    停任务时可能连带把 mysqld / 网关一起带走 ⇒ 下一档假人全部"连上后被关闭连接"
    （LoginSrv 日志里 `Unable to connect to any of the specified MySQL hosts`）。栈的生命周期要与
    编排脚本的生命周期解耦（用 `start-all.ps1` 前台起完即退出，子进程独立存活）。

## 已知限制与下一步（2026-10-10 首轮结论）

> ⚠️ **本节是 2026-10-10 首轮的观察与推测，多处判断已被下一节《2026-10-11 复跑结论》推翻/取代**——
> 优先读那一节：零样本的根因已经定位到三处（并已修复、已复跑复现阳性样本），不再需要"探针+负载分离"那条规避路线。

**能测的**：登录成功率、连接/掉线、各服务进程内存曲线（这三项在 20 与 200 档都真实取到）。

**② 已实现动作探针（`ProbeActionTick`，本批新增）——但端到端验证仍受环境阻塞**：
探针 `MIR2_BOT_ACTION_PROBE=1`（默认开，`-ActionProbeMs` 可调）在假人**进世界后**每 N ms 发一个 `CM_TURN`，
只依赖本假人自己的 socket 与 `DScreen`（**不碰 `MShare` 共享世界态**）⇒ 服务端回 `#+GD/<rtime>!` ⇒ 有 tick 样本。
它在 `DScreen.CurrentScene == PlayScene` 时才发，所以**前提是"进得了世界"**；本机当前这一栈在进世界这一步失败
（实测证据：200 档 47 个假人走到「准备进入游戏」后，RunGate 7200 直接关连接 398 次 ⇒ 无 `+GD` ack）。
进世界失败与"批量断连污染网关槽位"同源，需要**干净栈**；本机内存吃紧（系统已终止过一次后台整栈重启），
三档实测待环境恢复后按 `stack_e2e.ps1`（多档自动整栈重启）重跑。

**已知的第二个待查项（本批新发现，未定性）**：该轮 47 个假人已走到选角/进游戏阶段，但 `LoadMetrics.LoginOk()`
计数仍为 0 ⇒ 说明"登录成功"这条路径**不止我挂钩的那一处**（或者这批账号走的是另一分支）。
只影响"登录成功率"这一项读数的完整性，不影响 tick 探针。

**结构背景（为什么需要探针而不是直接开挂机）**：
BotSrv 的挂机/移动层建立在**跨假人共享的全局态**上（`MShare.MySelf` / `MShare.AutoMove` / `MShare.MapPath` …），
只有"登录→选服→选角→进世界"这条链路是无状态的；世界内的"动作"层无法让 N 个假人各自动作。实测证据（20 档）：
- 假人**进得了世界**（`成功进入游戏` = 7/20 ✓）、无崩溃、无兜住异常；
- 但 `开始自动挂机` = 0、`+GD` ack **0 条** ⇒ `tick_samples = 0`（`MShare.MySelf` 是人人都想写的那一个全局变量）；
- 假人还会被怪打死（`啊,我死了`）后停止动作。

⇒ 两条可行路线（**推荐第二条**）：
1. **一进程一假人**：每个假人独占进程 ⇒ 各自持有自己的全局态。200/1000 个进程对内存压力很大（本轮实测已因内存被系统终止过一次后台栈重启）。
2. **探针 + 负载分离**（推荐）：**动作/tick 探针**用真客户端（`D:\MirClient-run`）+ 已有的进程内 hook
   （`tools/capture/clienthook`）——它天然产生真实动作与 `+GD` ack；**负载**用假人（登录/连接/世界占用）。
   两侧口径不变（探针量的仍是同一条线上字节 `#+GD/<rtime>`），而且这正是"真实客户端行为"的更硬证据。
   代价：探针需要 GUI 客户端在跑，且**每档要换干净栈**（见坑位 7）。

数据完整性提醒：`load_stats.ndjson` 里 `internal_errors > 0` 时说明假人侧有异常被兜住，要连同 P99 一起看。

## 2026-10-11 复跑结论：零样本的**三处根因**（已修，附证据）

上一节的推测（"进不了世界需要干净栈 / 走真实客户端 + hook 的负载分离路线"）**不是根因**。用 A/B 实跑定性如下，
三处都在**假人工具侧**（`src/BotSrv/**`，走 `whitelist T-6` 登记），oracle 一行未改：

1. **探针抢在网关登录握手之前发包 ⇒ 网关按坏首包 `Kick(1)`（决定性证据）**
   探针原来的武装条件是 `DScreen.CurrentScene == PlayScene`，而该标志在收到 `SM_STARTPLAY` 时就置上了，
   **早于** `SendRunLogin()`（往 7200 写的 `**account/chr/...` 登录串）。于是同一秒内 `CM_TURN` 可能先落线，
   网关把这条当首包去解登录串 ⇒ 失败 ⇒ `src/GameGate/Services/ClientSession.cs` 的
   `ClientLogin(...); if (!success) { LogService.Info("客户端登陆消息处理失败，剔除链接"); Kick(1); }` 重置连接。
   **A/B 证据（同一栈、20 假人）**：探针开 ⇒ `成功进入游戏 = 0/20`、连接被重置、进程随后爆栈（见 2）；
   探针关 ⇒ `成功进入游戏 = 8/20`、`0` 次断连、无崩溃。
   **修法**：探针改为**收到 `SM_LOGON`（服务端确认进世界）才武装**，且发送前确认本连接仍连着。

2. **错误路径重入 ⇒ 栈溢出打死整个 BotSrv 进程（一份数据全废）**
   `ScoketClient.SendText` 在已坏的 socket 上抛 `SocketException` 时会**同步**回调 `OnError`，而
   `RobotPlayer.SocketError` 的 ConnectionReset 分支会去 `LoginOut() → SendClientMessage() → SendSocket() → SendText()`，
   于是同一个调用栈里递归：`SendText → SocketError → LoginOut → SendSocket → SendText → …`。
   **证据**：`tier-20/bots.err.log` 开头 `Stack overflow. Repeat 134 times: … at ScoketClient.SendText / RobotPlayer.SendSocket /
   SendClientMessage / LoginOut / SocketError`，`bots.log` 里 `退出游戏`＋`关闭连接` 各 **1742** 次（= 递归次数），
   进程退出 ⇒ 那一档 `load_stats.ndjson` 只剩 Init 一行。
   **修法**：`SocketError` 加重入闸门（一次错误 = 一条记录 + 该假人退场），并对 refused/reset 先摘 `IsConnected`。

3. **探针把 Recog 当时间戳用 ⇒ 服务端每个动作都判 false，回不了 `+GD`**
   修好 1、2 之后仍是 0 样本，但 `probe_sent=360` 已发出 ⇒ 问题在服务端不认这条动作：
   `WorldServer.ProcessUserMessage` 对 `CM_TURN` 走 `SendMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), 0)`，
   于是 `X = LoWord(Recog)`、`Y = HiWord(Recog)`、方向 = `Tag`；而 `PlayObject.ClientChangeDir`
   只有在 `nX == CurrX && nY == CurrY` 时才 `SendSocket(M2Share.GetGoodTick)`。探针原来塞的是
   `MakeMessage(CM_TURN, now, 0, dir, 0)`（Recog = 时间戳）⇒ 永不相等 ⇒ 无 ack。
   **证据**：修前 `probe_sent=360 / tick_samples=0`；改成 `MakeMessage(CM_TURN, MakeLong(X, Y), 0, dir, 0)`
   （X/Y 取 `SM_LOGON` 里服务端给本假人的落点）后 ⇒ `samples=326`、`P99=0ms`。
   C3 金标准里那条 `CM_WALK` 的 `Recog=0x026B011F` 就是坐标 `X=287,Y=619`，不是时间戳（早先注释读错了）。

### 复跑后的口径补充（读报告时一起看）

- `load_stats.ndjson` / `report.json` 新增三个字段：`in_world`（收到 `SM_LOGON` 的假人数 = **tick 样本的覆盖基数**）、
  `probe_sent`（探针实际发出的动作数）、`internal_errors`。`in_world / 档位人数` 就是 `world_ok_pct`。
  **为什么必须一起看**：tick 样本只可能来自进世界的假人；本工具的假人世界态建立在**跨假人共享**的 `MShare.*` 上，
  实测 200 档只有 ~10%（21/200）的假人能真正进世界 ⇒ tick 样本是"少数人身上量的"，样本量而不是分辨率是它现在的短板。
- **缺 tick 样本默认判 RED**，与 `--criteria` 解耦：`load_report.py` 的"缺样本"由独立开关
  `--allow-missing-tick`（`load_gate.ps1 -AllowMissingTick`）管辖；放行时报告里会写
  `tick_missing: true` / `tick_missing_acknowledged: true` / `missing_tick_note: 本次未采集 tick P99`。
  没有这个开关，`--criteria login,conn` **不再**能把 0 样本产物变成 GREEN（`load_report.py selftest` 有回归断言，
  把判定改回旧写法立刻红）。
- **多档连跑必须 `-RestartStackPerTier`（含第一档）**：本脚本用的是 `Restart-Stack`，
  它**不能**把 `start-all.ps1` 的输出接成管道再等它退出——服务进程会继承管道写端，父进程退出后管道不关闭 ⇒
  调用方永久挂住（上一批"多档连跑要杀任务树"的真因）。现在的做法：stop 重定向到文件、start 用独立进程起、
  **只等端口就绪**。

## 2026-10-11 第 4 轮：跨轮幂等 + 登录上限根因（T-9）+ 越界（T-10）+ `2>&1` 挂死

`tools/e2e/stack_e2e.ps1 -ProvisionAccounts -LoadTiers 200,500,1000 -SkipFlow -StopStackAtEnd` 连跑两轮：

| 轮 | 产物目录 | tier-200 | tier-500 | tier-1000 | 开号 |
| --- | --- | --- | --- | --- | --- |
| A | `tests/loadgate/load-20261011-121254/` | GREEN | GREEN | GREEN | `account=1000` |
| B | `tests/loadgate/load-20261011-121953/` | GREEN | GREEN | GREEN | `account=1000`（未增长） |

两轮三档全部 `login=100% / conn_lost_steady=0 / samples>0 / internal_errors=0`，verdict 完全一致。

### 1. tier-500/1000 登录爬坡上限 = **假人侧丢弃**，不是服务端吞吐（已修，T-9）

第 3 轮把上限归因为"服务端/客户端并发上限 ~350-390"，**错了**。第 4 轮把假人侧观测补全后，tier-500 的
`load_stats.ndjson` 直接给出反证：`ident_counts[529]=500`（服务端对**全部** 500 个假人回了 `SM_PASSOK_SELECTSERVER`）
而 `login_ok=179=ident_counts[520]=ident_counts[530]`（只有 179 个被假人处理）⇒ 丢包 100% 在假人侧：
`RobotPlayer.Operate.DecodeMessagePacket` 的"登录期响应"闸门用的是**跨假人共享**的 `MShare.MySelf == null`，
第一个假人 `SM_LOGON` 把它置上后再不为 null ⇒ 其余假人的登录响应全被静默丢弃。
改成本会话的 `!_inWorld`（`MarkInWorld` 在收到本假人自己的 `SM_LOGON` 时置位）后，三档 100% 登录（T-9）。

### 2. 开号跨轮幂等（第 3 轮已修，本轮复核）

`account.Account` 无唯一索引 ⇒ 幂等只能在开号 SQL 里用 `WHERE NOT EXISTS`（内容冻结不许加唯一索引，见 T-7）。
本轮两连跑日志均为 `BULK_OK ... account=1000 protection=1000`，**不再每轮 +1000**。

### 3. 自动挂机越界 ⇒ `internal_errors` 非零（已修，T-10）

T-6 只给 `TMap.CanMove` 补了上界守卫；`GetDoor`/`IsDoorOpen`/`CanFly`/`OpenDoor`/`CloseDoor` 仍会在
地图右/下边界外越界。tier-1000 稳态每 tick 抛 `IndexOutOfRangeException`（`internal_errors` 线性增长到 1667）；
加完整堆栈定位到 `TMap.GetDoor ← RobotPlayer.CheckDoorAction ← ProcessActionMessages` 后统一补 `InMapRange`（T-10）。

### 4. `2>&1` 收集子进程输出 ⇒ 整条命令跑完却不退出（工具链第三处同源坑）

`stack_e2e` 原来用 `$out = & powershell ... -Command ("& '<load_gate.ps1>' ...") 2>&1` 收集压测档位输出。
`2>&1` 建的是**管道**；load_gate 用 `Start-Process` 起的**长驻服务**（GameSvr/网关/MySQL）会继承管道写端，
于是父进程要等 EOF，而 EOF 要等所有服务死 ⇒ **summary 已落盘、栈还在跑、命令永不返回**（104822/105943 与上一轮遗留的
`stack_e2e` 进程都是它）。改为直接继承 stdout（`& powershell -File ...`，不建管道）后，两轮命令都自行退出。
**注意**：若在外部用管道包 `stack_e2e`（如 `... | tee log`），同样会因服务进程继承管道而挂住——请重定向到文件。

## 2026-10-11 第 6 轮（卡 `openmir2-botsrv-world-evidence`，T-12）：进世界证据只认本连接 + F3(a)/F4 收口

原始件全在 `tests/loadgate/botsrv-world-evidence-20261011/`（含 README + ndjson + report.json + BotSrv 日志）。

### 1. F4：挂机只对本会话已进世界的假人执行（`autoplay_pre_world` / `autoplay_skipped_pre_world`）

`RunAutoPlay` 原来只看跨假人共享的 `MShare.MySelf != null`：任意一个假人进世界后，其余**未**进世界的假人也被挂机，
脚本写在共享态上 ⇒ **按 bot 名记的挂机日志不能当作「该假人进世界」的证据**。T-12 把门换成**本会话** `RobotPlayer.InWorld`
（`SM_LOGON` 时置位），并在 `load_stats.ndjson` 分账：

- `autoplay_in_world`：本会话已进世界的挂机轮次（修复后只有它执行）；
- `autoplay_pre_world`：本会话未进世界却仍跑挂机的轮次 —— **修复后必须为 0**；>0 只可能来自
  `MIR2_BOT_LEGACY_SHARED_AUTOPLAY=1` 的「改前」对照组；
- `autoplay_skipped_pre_world`：修复后未进世界被跳过的次数。

同一批 20 假人：改后 `in_world=20/20, autoplay_pre_world=0, skipped=939`；改前对照组
`in_world=11/20, autoplay_pre_world=529`（§见证据 README 的 §1/§4；20 而非 200 的原因=当时端口被 m2svr 真机夹具占用）。

### 2. F3(a)：第三条假人 `CM_ADDNEWUSER` 后无应答 = T-9 的同一根因（已修）

`CM_ADDNEWUSER` 的应答（`SM_NEWID_SUCCESS/FAIL`）与登录应答走**同一个** `DecodeMessagePacket` 分支；
T-9 之前该分支的判据是跨假人共享的 `MShare.MySelf == null`，前一个假人进世界后就不再处理后续假人的应答
⇒ 02:59 冒烟里第三条的注册应答被静默丢弃。换成**本会话** `!_inWorld` 后消失。观测（T-12）：`register_sent` /
`register_send_closed`（注册包发出/丢弃）落 `load_stats.ndjson`，服务端回没回看既有 `ident_counts[504/505]`。
证据：3 假人冒烟 `register_sent=3`、`ident_counts[504]=3`（`505=0`）、`in_world=3`。

### 3. 牙齿（改坏必红）

`run_bots.ps1` 新增 `-RequireWorld`：要求 `in_world == Count` 才 GREEN（默认关；M5 三档只有 ~10% 不能当默认阈值）。
BotSrv 新增测试钩子 `MIR2_BOT_SUPPRESS_INWORLD=<0 基序号>`：命中的假人照常进世界，但**不计** `in_world`
（`in_world_suppressed` 可见）。抹掉 3 假人里第 3 个的证据 ⇒ `in_world=2/3` ⇒ `-RequireWorld` **RED**；
不抑制 ⇒ **GREEN**。`load_report.py summarize --world-threshold PCT` 提供同一判据（`selftest` 里也有它的牙齿项）。

### 4. report.json 的在线/基数语义

`online_bots`（= `in_world`）/ `online_base`（档位人数）/ `online_source`；另落
`in_world_suppressed`、`autoplay_*`、`register_*`。**`in_world` 的语义 = 该假人本连接收到 `SM_LOGON`**（不是
「本地切到 PlayScene」、也不是共享 `MShare`）。默认不按它判红，除非显式 `-RequireWorld` / `--world-threshold`。
