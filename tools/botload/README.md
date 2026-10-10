# tools/botload/ —— 假人压测 / 载荷门禁（C6）

对应设计文档 §6 **M5 压测三档**、§4.2.1「用假人量单线程世界的 tick P99 曲线」。
三个件：`run_bots.ps1`（单档冒烟，C1 就有）、`load_gate.ps1`（档位编排）、`load_report.py`（指标归算）。
整栈一条命令见 `tools/e2e/stack_e2e.ps1`。

```powershell
# 整栈一条命令：停栈 → 批量开号 → 起栈 → 工具自测+金标准 → 压测档位 200/500/1000 → 收尾
powershell -ExecutionPolicy Bypass -File tools/e2e/stack_e2e.ps1 `
  -ProvisionAccounts -ProvisionCount 1000 -SkipFlow -LoadTiers 200,500,1000

# 只跑单档（栈已在跑）
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -Tiers 200 -HoldSec 60

# 归算（也可单独跑）
python tools/botload/load_report.py summarize --stats <tier>/load_stats.ndjson --mem <tier>/mem.ndjson --count 200
python tools/botload/load_report.py selftest         # 口径自测（含尾部敏感/改坏必变）
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
| 登录成功率 | **100%** | §6 M5-3 |
| 稳态掉线 | **0** | §6 M5-3 |
| tick P99 | **≤ 100ms** | §6 M5-3 |
| 内存末段斜率 | ≈ 0（M5 长跑另判：峰值-均值 < 5%） | §6 M5-2 |

`load_gate.ps1` 逐档判 GREEN/RED，任一档 RED ⇒ 退出码 1；三档全绿才 `GREEN`。

## 红检（改坏必红）

- `load_gate.ps1 -SelfTestRed`：指向无监听端口 ⇒ 爬坡不可能完成 ⇒ 必须 `verdict=RED`（脚本自身退出 0 = 红检通过）。
- `load_report.py selftest`：分位数正确 / **尾部敏感**（挪走 20 个慢样本 P99 必须 150→5）/ 爬坡未完成判红 /
  掉线分爬坡与稳态 / 内存斜率（线性 1MB/s vs 平坦 0）。

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
