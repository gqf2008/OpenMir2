# BotSrv 夹具错误类别普查（卡 `openmir2-botsrv-recv-thread-nre`）

> 结论先行：**「全绿」不再等于 `internal_errors == 0`，而是「枚举出的每一类错误都出现、且为 0
> （或已由卡面评审过的白名单登记）」**。本文件是枚举表、计数器接线、自检/变异证据与 runbook 的
> 权威落点；`tools/botload/load_report.py` 的 `ERROR_CENSUS_*` 与 `src/BotSrv/BotErrorCensus.cs`
> 的 `Order` 必须与本表逐字一致。

## 1. 为什么改口径

- N1 窗口的收包线程 `[ERR] Object reference not set` 约 **3.95 万条**：只进了 `LogService.Error`，
  **没有进 `internal_errors`**（见 parent 卡 `openmir2-botsrv-autoplay-nre` 的 residual）。
- 随后又发现 `ArgumentOutOfRangeException`（ActorList 并发增删）**没有进任何计数**。
- 两次都是「修好了我们看得见的那类」。所以把判据从「我知道的两类为 0」改成
  **枚举全部 `catch` / 线程入口 / 事件处理 / fire-and-forget 边界，逐类计数**。

## 2. 枚举方法（可复核）

对 `src/BotSrv/**` 做了四类扫描：

1. **所有 `try/catch`**：运行时 catch、仅自检文件的 JSON 解析 catch、主动 teardown 的 catch 分开列出；
2. **所有线程入口**：`new Thread(...)`（runThread、`ScoketClient-recvs`、`LoadMetricsFlush`）、
   `Channel` 消费循环（async）、`Task`/`async void`/`BeginInvoke`（本仓库无）、`ThreadPool`（无）；
3. **所有事件处理**：`OnConnected` / `OnDisconnected` / `OnReceivedData` / `OnError` 的注册与调用点
   （`RobotPlayer`、`LoginScene`、`SelectChrScene`）；
4. **进程级**：`AppDomain.UnhandledException`、`TaskScheduler.UnobservedTaskException`、
   `Application.ThreadException`（WinForms，不适用，本项目无 WinForms）。

扫描结果：运行时 catch 17 处、仅自检 catch 6 处、显式 teardown catch 4 处、线程入口 3 条、
socket 事件调用点 4 类、进程级钩子 2 个。下表逐项给出归属。

## 3. 类别 × 计数器 × 触发点 × 自检

所有计数器都在 `load_stats.ndjson` 的 `error_census` 对象里（`error_census_schema = 1`）。
「自检」列对应 `BotSrv.exe --selftest-error-census` 的用例名（`inject-<category>`）。

| 类别 | 分类 | 触发点（符号） | 自检 | 全绿要求 |
| --- | --- | --- | --- | --- |
| `recv_packet_null` | fault | `ClientManager.ProcessOnePacket → RunPacketGuard`（收包线程处理一帧时的 NRE；N1 的 3.95 万条） | `inject-recv_packet_null` | 0 |
| `recv_packet_index` | fault | 同上（AOORE/IOORE：ActorList 无锁并发增删） | `inject-recv_packet_index` | 0 |
| `recv_packet_other` | fault | 同上（其它异常兜底） | `inject-recv_packet_other` | 0 |
| `run_round_null` | fault | `ClientManager.SafeRun → RunBotRound`（单假人一轮的 NRE） | `inject-run_round_null` | 0 |
| `run_round_index` | fault | 同上（AOORE/IOORE） | `inject-run_round_index` | 0 |
| `run_round_other` | fault | 同上（其它异常兜底） | `inject-run_round_other` | 0 |
| `run_loop_exception` | fault | `AppService.Run → RunLoopGuard`（runThread 整轮，含建号/AddClient；以前建号异常会静默打死线程） | `inject-run_loop_exception` | 0 |
| `socket_handler_exception` | fault | `ScoketClient.InvokeHandlerGuarded`（OnConnected/OnDisconnected/OnReceivedData/OnError 的 fire-and-forget 边界） | `inject-socket_handler_exception` | 0 |
| `socket_recv_fault` | fault | `ScoketClient.RecvLoop` 的非生命周期 `SocketException` + 非 socket 异常兜底 | `inject-socket_recv_fault` | 0 |
| `socket_send_fault` | fault | `ScoketClient.SendText/SendBuffer` 的非生命周期 `SocketException` | `inject-socket_send_fault` | 0 |
| `socket_connect_fault` | fault | `LoginScene.Login` 的 catch + `SocketExceptionSeen("connect")` 的非生命周期码 | `inject-socket_connect_fault` | 0 |
| `metric_flush_failure` | fault | `LoadMetrics.Flush` / `WriteLoginConnsLocked` 的 catch（证据链断） | `inject-metric_flush_failure` | 0 |
| `self_report_write_failure` | fault | `SelfReport.Write` 的 catch（自报证据断） | `inject-self_report_write_failure` | 0 |
| `drop_exp_writer_open_failure` | fault | `RobotPlayer.ArmDropExpSequence` writer 打开失败 | `inject-drop_exp_writer_open_failure` | 0 |
| `drop_exp_writer_write_failure` | fault | `RobotPlayer.DropExpWriteRow` 写行失败 | `inject-drop_exp_writer_write_failure` | 0 |
| `bag_item_decode_failure` | fault | `RobotPlayer` 的 `SM_BAGITEMS/SM_ADDITEM` 解码 catch | `inject-bag_item_decode_failure` | 0 |
| `unhandled_exception` | fault | `AppDomain.UnhandledException → BotErrorCensus.OnUnhandledException`（另落 `fatal.ndjson`） | `inject-unhandled_exception` | 0 |
| `unobserved_task_exception` | fault | `TaskScheduler.UnobservedTaskException` | `inject-unobserved_task_exception` | 0 |
| `internal_error_unclassified` | fault | 无类别的 `LoadMetrics.InternalError()`（新增 catch 漏登记时的可见兜底） | `inject-internal_error_unclassified` | 0 |
| `teardown_race` | operational | `RecvLoop` / `SendText` / `SendBuffer` 在主动 `Disconnect` 后撞 `ObjectDisposedException` | `inject-teardown_race` | 允许非 0，**必须逐类报告** |

**fault 类别默认容忍度 0。** 豁免只有两条：

1. `feeds_internal=True` 的类别，且 `internal_errors` 已被
   `tools/botload/internal-errors-allowlist.json`（按异常类型 + 消息正则 + max）整体豁免
   （要求 `internal_errors > 0` 且所有日志样本都命中白名单；`internal_errors=0` 本身不是豁免依据）；
2. 其余 fault 类别由 `tools/botload/error-census-allowlist.json` 按 `category + max` 登记。

### 枚举到但**有意不计数**的 catch（不是遗漏）

| 位置 | 为什么不计入错误面 |
| --- | --- |
| `MShare.TryGetFrameInt` 的 `JsonException` | 夹具帧分类的纯函数，解析失败返回 `false`；语义是「这一帧不是该字段」，不是异常面。真正的观测差异由夹具记录的 `skip`/分支体现。 |
| `DropExpSeqSelfCheck` / `QuestSeqSelfCheck` / `SceneSeqSelfCheck` 的 `JsonException` | 无栈自检内部断言；失败会直接让自检 RED。 |
| `ScoketClient.Disconnect` 的 `Shutdown`/`Close` catch | 显式主动关闭；预期路径。关闭后的 recv/send 竞态另计 `teardown_race`。 |
| `LoadMetrics.WriteFatalMarker` 与 `BotErrorCensus.SafeLog` 的 catch | 最后兜底：正在写失败/崩溃路径上不能再次抛出，也不能可靠计数（计数本身也可能写不进去）。 |
| `AppService.RunLoopGuard` / `RunBotRound` / `RunPacketGuard` / `InvokeHandlerGuarded` 内的 `SafeLog` | 日志失败不改变错误计数与进程存活；错误计数在 `SafeLog` 之前已经完成。 |

operational 类别（`teardown_race`）与下列**既有事件计数**不是「故障」，不计入 `internal_errors`，
但必须逐项出现在报告里；缺失即判 RED（枚举不完整）。

| 既有事件计数 | 语义 | 归属判据 |
| --- | --- | --- |
| `conn_refused` | 连接被拒 | 登录成功率 / 爬坡 |
| `conn_lost` | 登录后掉线 | `conn_lost_steady == 0` |
| `conn_err_refused/reset/timeout/other` | 连接建立阶段的 SocketError 分类 | T-11 方向归算 |
| `login_send_failed` / `login_send_closed` | 登录包写失败 / socket 未连接而丢弃 | T-8/T-11 |
| `register_send_closed` | 注册包因 socket 未连接而丢弃 | F3(a) |
| `partial_frames` / `chunks_without_hash` | 组帧残帧/无 `#` 块 | 帧完整性观测 |
| `ident_overflow` | ident 直方图键溢出 | 防异常输入撑爆 |
| `logon_misdelivered` | 已绑定 actor 后收到别的 actor 的 SM_LOGON 被拒 | F12②（不等于错投帧数） |
| `in_world_suppressed` | 牙齿钩子显式抹掉进世界证据 | 仅自检 |
| `autoplay_pre_world` | 未进世界仍跑挂机（仅 legacy 对照组） | 正常组必须 0 |
| `autoplay_skipped_pre_world` | 因本会话未进世界而跳过挂机 | 正常行为，可非 0 |

### 父卡依赖（明确登记，不静默）

卡 `openmir2-botsrv-autoplay-nre`（已 approve、待合并）在 BeginScene 落
`scene_null_actor_skipped` / `scene_list_race_skipped`。本卡 base 上这两个字段不存在，报告会显式写
`pending_parent_card`（**不计失败**，但不是静默当 0）；父卡合并后它们自动进入必判集，非 0 即 RED。
若只出现一个、另一个缺失，直接 fail-closed。

## 4. 接线与不变式

- `BotErrorCensus`（`src/BotSrv/BotErrorCensus.cs`）持有封闭类别表与累计值；
  `LoadMetrics.StatsLine()` 每 5s 把 `error_census` 写进 `load_stats.ndjson`。
- `LoadMetrics.InternalError(category)` 同时增加 `internal_errors` 与该类别；`LoadMetrics.InternalError()`
  无参入口落到 `internal_error_unclassified`。
- **不变式**：`internal_errors == Σ(error_census 中 feeds_internal=True 的类别)`。
  `load_report.py` 逐次校验；任何绕过分类入口直接加 `_internalErrors` 的路径都会红。
- `load_report.py` 对以下情况全部 fail-closed：
  缺 `error_census` 对象、schema 不符、缺类别、多出未登记类别、fault 类别 > 0（未豁免）、
  `internal_errors` 与分类之和不一致、缺任一既有事件计数、父卡计数只出现一半或非 0。
- 自检 `--selftest-error-census` 逐类注入 +1，并额外验证：
  JSON 键集合 == 类别表、`internal_errors == Σ`、真实 `LoadMetrics.Init/Flush` 落盘的
  `error_census` 逐类数值一致、负例（处理器不抛异常 / ConnectionReset 等生命周期码不串类）。
- 跨语言契约：`load_report.py selftest` 解析 `BotErrorCensus.cs` 的 `Order` 与
  `FeedsInternalErrors`，与 Python 注册表逐字对账（改坏 C# 常量或删 Order 一项必红）。

## 5. 跨 bot 共享可变状态审计（A6）

| 状态 | 写者 | 读者 | 锁 | 风险 | 处置 |
| --- | --- | --- | --- | --- | --- |
| `PlayScene.ActorList`（每假人视图） | 收包线程（`SendMsg` 的 NewActor/DeleteActor/DelActor）+ runThread（`BeginScene`） | runThread 与夹具读取函数 | 无 | 高 | parent 卡给 BeginScene 加轮首快照/null 槽跳过/删除点核对；本卡把收包/run 两侧异常逐类计数。彻底修（锁或 per-bot actor store）列入后续。 |
| `MShare.MySelf` | 收包线程（SM_LOGON 重绑 / ClearActors 置 null）+ runThread | 几乎所有挂机/移动/探针路径 | 无 | 高 | parent 卡轮首快照关「轮内变 null/换人」窗口；跨 bot 重绑仍在（reviewer F6 residual）。 |
| `MShare.g_FreeActorList` | `BeginScene`（runThread）、组队清理 | runner 与复用路径 | 无 | 高 | 已登记为已知；与 ActorList 同源，后续随 per-bot actor store 一起修。 |
| `ClientManager._clientList` / `_autoList` | runThread（AddClient/Run/RunAutoPlay）+ 收包线程（DelClient） | Run/RunAutoPlay/DelClient | **本卡加 `_clientSync` + 快照** | 高 → 已修 | 迭代前快照、增删加锁；不再可能并发改坏 `List` 内部状态。 |
| `MShare` 的每假人玩法全局（`TargetX/Y`、`PlayerAction`、`ItemArr`、`DropedItemList`、`AutoPlay`、`MapPath`、`AutoStep`、`TargetCret/FocusCret/MagicLockActor/MagicTarget`、`MapTitle`） | 每个假人的 run/recv 路径 | 每个假人 | 无 | 高 | 已登记为已知（T-22/T-29 约定「只读自己的视图」但无机制保证）；后续清单：per-bot 状态化。 |
| `MShare` 连接/会话全局（`SelChrAddr/Port`、`ServerConnected`、`SendLogin`、`ConnectionStep`、`SoftClosed`） | 各假人的 LoginScene/SelectChrScene | 其它假人的登录路径 | 无 | 高 | 已登记为已知；错投有 per-session actor 身份守卫（`logon_misdelivered`）兜底，但共享连接状态仍是跨 bot 污染源。 |
| `MShare` 夹具配置静态（`g_bo*Sequence`、`g_s*`、`g_n*`） | `AppService` 构造器（起线程前一次性） | 全部 | 无需 | 低 | 只读；不改。 |
| `TMap` 每假人的 `Map` / `m_MapBuf` | runThread + 收包线程（`SendMsg` 地图跳转分支） | runThread | 无 | 中 | 已登记为已知；`m_MapBuf` 置 null 与读取的竞态在 parent 卡归因范围内。 |
| `HeroActor` 每假人列表 | runThread | runThread | 无 | 中 | 已登记为已知；英雄/宠物夹具固定步序下未观测到异常。 |
| `LoadMetrics` 全部静态计数 | 全部线程 | flush 线程 | `Sync` | 低 | 安全。 |
| `SelfReport` 文件与 `_broken` | 全部线程 | 写者自身 | `Sync` | 低 | 安全；写失败已计入 `self_report_write_failure`。 |

## 6. 明确未覆盖的面（不硬凑）

- **不可捕获的进程级死亡**：native crash、栈溢出、OOM。`load_gate.ps1` 的 `bots_runtime.json`
  仍按退出码 / `Out of memory.` / Windows Application Error 0xe0434352 判红；本普查不重复造轮子。
- **日志系统自身**（NLog/`LogService`）初始化或写盘异常：护栏里的日志已用 `SafeLog` 包住，不让
  日志失败打断错误处理；但日志系统自身的其它故障不在本表类别内（不是夹具错误面）。
- **关停期取消**：`Channel.WaitToReadAsync` 的 `OperationCanceledException` 属于 teardown，
  不是稳态错误类；不计数。
- **服务端（C#/Rust 七进程）错误**：不在 BotSrv 夹具范围。
- **父卡的两个场景计数**：本卡 base 上未接线，已按 `pending_parent_card` 显式登记；合并后自动必判。

## 7. Runbook（「全绿」的唯一口径）

```bash
# 1) C# 逐类牙（无栈；每类注入 ⇒ +1；删计数器/删类别必红）
src/BotSrv/bin/Release/BotSrv.exe --selftest-error-census

# 2) Python 口径与跨语言契约（无栈）
python -I -B tools/botload/load_report.py selftest

# 3) 门禁 CLI 注入红检（无栈）
pwsh -NoProfile -File tools/botload/load_gate.ps1 -SelfTestCriteria

# 4) 真机一轮（窗口内）
pwsh -NoProfile -File tools/botload/load_gate.ps1 -Tiers 200 -HoldSec 360 ...
python -I -B tools/botload/load_report.py summarize \
  --stats <tier>/load_stats.ndjson --bots-log <tier>/bots.log \
  --bots-runtime <tier>/bots_runtime.json --count 200 --hold-sec 360
```

**全绿 = 同时满足**：

1. `report.json.error_census.present == true` 且 `complete == true`；
2. `categories` 里**每一个 fault 类别** `count == 0`，或状态为
   `excused_by_internal_allowlist` / `excused_by_census_allowlist`（`count <= max`）；
3. `internal_errors == error_census.internal_census_sum`；
4. `operational` 列表里每一项都 `present == true`（operational 非 0 允许，但必须列出）；
5. `pending_parent` 每项要么 `pending_parent_card`（父卡未合并），要么 `count == 0`；
6. `required_fields.error_census_complete == true`，且原有四项必列字段仍齐。

**以下任一情况直接 RED（不是「警告」）**：缺 `error_census` / schema 不符 / 缺类别 / 多出未登记类别 /
fault 非 0 未豁免 / `internal_errors` 对账不平 / 缺任一既有事件计数 / 父卡计数只出现一半或非 0。

## 8. 残留物与后续

- 本卡**不**修 `PlayScene.SendMsg`/`ActorList` 的根因（reviewer F1/F6 的并发面仍在）；它负责让
  3.95 万条那类错误**可见且可归因**。根因修复（锁 / per-bot actor store / per-bot self）需另开卡，
  按 A6 清单逐项迁移。
- 父卡 `openmir2-botsrv-autoplay-nre` 合并后，`scene_null_actor_skipped` / `scene_list_race_skipped`
  自动进入必判集；本文件与 `load_report.py` 无需再改（字段名已登记）。
- 真机 200/500 档验证需窗口；本卡交付的是代码 + 无栈自检 + 判断口径，未起栈。

## 附录 A：逐站点枚举清单（可复核）

下表覆盖 `src/BotSrv/**` 全部 `try/catch`、线程入口、socket 事件类与进程钩子。**「当前计数」列**
写明本轮之后的落点；标「否（登记）」的项在 §3「有意不计数」表或本节说明理由。

### A1. 运行时 catch / 护栏

| # | 位置（符号） | 触发条件 | 当前计数 | 类别 |
| --- | --- | --- | --- | --- |
| 1 | `AppService.Run` → `RunLoopGuard` | runThread 整轮任意异常（含建号/AddClient） | 是 | `run_loop_exception` |
| 2 | `ClientManager.ProcessReviceMessage` → `ProcessOnePacket` → `RunPacketGuard` | 收包线程处理一帧抛异常 | 是 | `recv_packet_null/index/other` |
| 3 | `ClientManager.SafeRun` → `RunBotRound` | 单假人一轮抛异常 | 是 | `run_round_null/index/other` |
| 4 | `LoadMetrics.Flush` catch | 统计落盘失败 | 是 | `metric_flush_failure` |
| 5 | `LoadMetrics.WriteLoginConnsLocked` catch | login_conns 重写失败 | 是 | `metric_flush_failure` |
| 6 | `RobotPlayer.SocketError`（try/finally，无 catch） | 处理器体抛异常 | 是（调用点护栏） | `socket_handler_exception` |
| 7 | `RobotPlayer` drop-exp writer open catch | writer 打不开 | 是 | `drop_exp_writer_open_failure` |
| 8 | `RobotPlayer.DropExpWriteRow` catch | 写行失败 | 是 | `drop_exp_writer_write_failure` |
| 9 | `RobotPlayer.AppLogout`（try/finally） | 无 catch；异常上抛 | 是（上游护栏） | 调用方 `recv_packet_*` / `run_round_*` |
| 10 | `RobotPlayer.AppExit`（try/finally） | 无 catch；异常上抛 | 是（上游护栏） | 同上 |
| 11 | `RobotPlayer` 自动寻路 try/finally（3 处） | 无 catch；异常上抛 | 是（上游护栏） | 同上 |
| 12 | `RobotPlayer` 背包解码 catch | `SM_BAGITEMS/SM_ADDITEM` 解码失败 | 是 | `bag_item_decode_failure` |
| 13 | `PlayScene.SendMsg`（try/finally） | 无 catch；异常上抛 | 是（上游护栏） | 收包路径 `recv_packet_*` |
| 14 | `LoginScene.Login` catch | 连接建立阶段非 SocketException | 是 | `socket_connect_fault` |
| 15 | `SelfReport.Write` catch | 自报文件追加失败 | 是 | `self_report_write_failure` |
| 16 | `ScoketClient.Connect` SocketException catch | 连接失败 | 是（区分） | 生命周期码 → `conn_err_*`；其余 → `socket_connect_fault` |
| 17 | `ScoketClient.RecvLoop` SocketException catch | 收包 Socket 错误 | 是（区分） | reset 等 → `conn_lost`；其余 → `socket_recv_fault` |
| 18 | `ScoketClient.RecvLoop` ObjectDisposedException | 主动关闭后收包 | 是 | `teardown_race` |
| 19 | `ScoketClient.RecvLoop` 兜底 Exception | 非 socket 异常 | 是 | `socket_recv_fault` + fatal.ndjson |
| 20 | `ScoketClient.SendText` SocketException / ObjectDisposed | 发送失败 / 关闭后发送 | 是 | `socket_send_fault` / `teardown_race` |
| 21 | `ScoketClient.SendBuffer` SocketException / ObjectDisposed | 同上 | 是 | 同上 |
| 22 | `ScoketClient.Disconnect` Shutdown/Close catch | 显式关闭 | 否（登记） | 主动 teardown，不是错误 |

### A2. 自检/最后兜底 catch（有意不计数）

| 位置 | 触发条件 | 当前计数 | 说明 |
| --- | --- | --- | --- |
| `MShare.TryGetFrameInt` JsonException | 帧里没有该字段/非法 JSON | 否（登记） | 纯分类函数返回 false；差异靠夹具 `skip` 记录 |
| `DropExpSeqSelfCheck`（2 处） | 自检 JSON 断言 | 否（自检） | 失败直接让自检 RED |
| `QuestSeqSelfCheck` | 自检 JSON 断言 | 否（自检） | 同上 |
| `SceneSeqSelfCheck`（3 处） | 自检 JSON 断言 | 否（自检） | 同上 |
| `ErrorCensusSelfCheck`（json/stats/finally） | 自检自身断言/清理 | 否（自检） | 失败直接让自检 RED |
| `BotErrorCensus.SafeLog` catch | 日志写失败 | 否（登记） | 最后兜底：不能二次抛出，也不能可靠计数 |
| `LoadMetrics.WriteFatalMarker` catch | fatal 记录写失败 | 否（登记） | 同上 |

### A3. 线程入口 / 事件 / 进程钩子

| 入口 | 位置 | 异常落点 | 备注 |
| --- | --- | --- | --- |
| runThread | `AppService` 构造器 `new Thread(Run)` | `RunLoopGuard` → `run_loop_exception` | 整轮（含建号）都兜住 |
| recv 线程 | `ScoketClient.Connect` 的 `new Thread(RecvLoop)` | 处理器护栏 + socket catch + 兜底 | 不再被处理器异常打死 |
| flush 线程 | `LoadMetrics.Init` 的 `new Thread(FlushLoop)` | `Flush` 内部 catch → `metric_flush_failure` | `Thread.Sleep` 中断仅在关停 |
| async/channel | `ClientManager.ProcessReviceMessage` | per-packet 护栏；取消 = teardown | 无 `async void` / 未 await Task / `BeginInvoke` |
| RobotPlayer 事件 | `OnConnected/OnDisconnected/OnReceivedData/OnError` | `socket_handler_exception` | 注册见构造器 |
| LoginScene 事件 | `CSocketConnect/CSocketDisconnect/CSocketRead/CSocketError` | `socket_handler_exception` | 注册见构造器 |
| SelectChrScene 事件 | `CSocketConnect/CSocketDisconnect/CSocketRead/CSocketError` | `socket_handler_exception` | 注册见构造器 |
| 进程钩子 | `AppDomain.UnhandledException` | `unhandled_exception` + `fatal.ndjson` | `Program.Main` 注册 |
| 进程钩子 | `TaskScheduler.UnobservedTaskException` | `unobserved_task_exception` + `fatal.ndjson` | `Program.Main` 注册 |
| WinForms 钩子 | `Application.ThreadException` | 不适用 | 本项目无 WinForms |

### A4. 复核命令

```bash
grep -rn "\btry\b" src/BotSrv --include='*.cs'
grep -rn "catch" src/BotSrv --include='*.cs'
grep -rn "new Thread\|Task.Run\|async void\|BeginInvoke\|UnhandledException\|ThreadException" src/BotSrv --include='*.cs'
grep -rn "+= CSocket\|+= Socket\|+= ReceivedData" src/BotSrv --include='*.cs'
```
