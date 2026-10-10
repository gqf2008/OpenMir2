# tools/ —— C 线工具与夹具

> 设计文档：`docs/Rust重写设计文档.md` §7 / §11 / §12.2（C 线：抓包代理、假人驱动、日志 diff、DB 快照、E2E）。
> 本目录只放工具，不放业务实现。所有脚本假定：客户端 `D:\MirClient-run`、
> 服务端 `E:\MirServer`（**只读不改**）、MySQL（MariaDB，`127.0.0.1:3306`，root 无密码）。

## 一图看懂

```
工具                          作用                                         红检（改坏必红）
tools/capture/mir2_proxy.py   客户端↔网关 TCP 透传 + 原始字节 dump          selftest_proxy.py（3 项）
tools/capture/segment_frames.py  原始流按帧界切分 → frames.ndjson + manifest
tools/capture/verify_golden.py   金标准完整性对账（流/帧/总 hash）
tools/capture/GoldenExport/      帧 → 契约 JSONL（C# oracle 解码字段）      decode_error>0 即退出码 3
tools/capture/capture_baseline.ps1  金标准抓包编排（影子网关+代理+真实客户端）
tools/client/mir_flow.ps1     真实客户端流程驱动（登录/选角/建角/进游戏/挂机/小退/再进）
tools/client/account_provision.ps1  测试账号开号（account + account_protection 两表）
tools/botload/run_bots.ps1    假人压测驱动（BotSrv N 并发登录）              -SelfTestRed
tools/logdiff/timeline.py     多源日志归一 + 时间线对拍                      selftest（2 项）
tools/dbsnap/dbsnap.py        MySQL 快照/字段级比对                          selftest（2 项）
tools/e2e/run_e2e.ps1         E2E 回归套件（工具自测+金标准+流程+冒烟）      -SelfTestRed
tools/oracle/                 S1 线：oracle 部署件对账 / 铺部署 / 网关重连门禁（见 tools/oracle/README.md）  -SelfTestRed
```

## 0. 一次跑完全部自测（不需起服务端）

```powershell
powershell -ExecutionPolicy Bypass -File tools/e2e/run_e2e.ps1 -SkipFlow -SkipBots
```

## 1. 金标准抓包（C 线第一优先，A 线 M0 的输入）

```powershell
# 前置：服务端全栈在跑（E:\MirServer\start-all.cmd）
powershell -ExecutionPolicy Bypass -File tools/capture/capture_baseline.ps1
```

它做四件事，**全程不改 `E:\MirServer` 与 `D:\MirClient-run`**：

1. `start-all.ps1 -Stop` → 起 MySQL/DBSrv/LoginSrv/GameSvr（核心服务）；网关换成**影子副本**：
   robocopy 到会话目录，`config.conf` 的 `GatePort` +10000；SelGate 的监听端口是编译期常量
   （`src/SelGate/GateShare.cs:15`，config 里那份是死配置），所以同时对**影子副本的 DLL**
   做 IL 常量替换（`ldc.i4 7100` → `ldc.i4 17100`，实测 3 处，预检会核对处数）。
2. 抓包代理占住官方端口 7000/7100/7200，转发到影子网关 17000/17100/17200。
3. 真实客户端跑 `tools/client/mir_flow.ps1` 全流程，截图 + 阶段时间线。
4. 无论成败：停影子 → `start-all.ps1 -Stop` → `start-all.ps1` 全量恢复正常栈，然后切帧、校验。

产物（`tests/golden/session-<时间戳>/`）：`proxy/`（原始 dump）、`frames.ndjson`、
`manifest.json`（N 帧 + 每帧 sha256 + 总 hash + 复现命令）、`shots/`（截图 + stages.ndjson）、
`logs/`（服务端日志切片 + 客户端 cl_trace 切片）。`tests/golden/LATEST` 指向最新会话。

### 为什么必须「整栈重启」而不是只重启网关

实机踩过（2026-10-10）：GameSvr 的网关表经不起只重启网关——旧网关断开会把槽位
`UserList` 置 null（`M2Server/Net/TCP/TCPNetChannel.cs` `CloseGate`），之后进世界的玩家在
`SetGateUserList` 上 NRE 死循环（`NewHumanList` 清不掉），客户端永久黑屏。
`capture_baseline.ps1` 因此走完整栈循环。

### 契约 JSONL（A 线 replay 直接消费）

```powershell
dotnet run --project tools/capture/GoldenExport -c Release -- `
  --session tests/golden/session-<时间戳>/proxy --out <capture>.jsonl
```

字段口径见 `mir2-rs/tests/golden/README.md`。解码用的是**仓库自己的** `OpenMir2.EncryptUtil`
（C# oracle），不是 Rust replay 的产物——满足抓包契约的防伪约定。帧界：
`c2s` = `#1` + 编码块 + `!`；`s2c` = `#` + 编码块 + `!$`；裸 `*` 是心跳。
进 GameGate 的首个上行包是 `**账号/角色/...` 登录串（走 `DeCodeString`，不当 12B 头解，
导出器标 `"string_frame":true`）。

## 2. 假人压测

```powershell
# 200 并发登录（账号 = 前缀+序号，密码 = 账号名；-NewAccount 先注册）
powershell -ExecutionPolicy Bypass -File tools/botload/run_bots.ps1 -Count 200 -Prefix loadbot -NewAccount

# 红检（指向无监听端口，必须判失败）
powershell -ExecutionPolicy Bypass -File tools/botload/run_bots.ps1 -SelfTestRed
```

底层是车间里的 `src/BotSrv`（headless 机器人，完整客户端协议栈）。BotSrv 上游被「调整项目
结构」改坏过，无法编译，本项目补了 `src/BotSrv/SocketShim.cs`（`ScoketClient`/`DSCClient*`
按原 API 表面用 `System.Net.Sockets` 重写）并修 `AppServer` 的 Host 装配。
产物：`tests/botload/bots-<时间戳>.log` + `.summary.json`（login_ok / success_rate）。

## 3. 日志时间线对齐与对拍

```powershell
# 把一次抓包的 proxy 时间线 + 阶段标记 + 客户端 hook + 服务端日志合并成一条时间线
python tools/logdiff/timeline.py merge --out tl.ndjson `
  --src proxy=tests/golden/session-.../proxy/chunks.ndjson `
  --src stages=tests/golden/session-.../shots/stages.ndjson `
  --src clt=tests/golden/session-.../logs/cl_trace.txt `
  --src gamesvr=tests/golden/session-.../logs/GameSvr.out.log

# 两次跑的时间线对拍（基线 vs 新跑），第一个分叉点即红
python tools/logdiff/timeline.py diff baseline.ndjson new.ndjson
```

`HH:mm:ss.fff` 类日志无日期，用 `--anchor YYYY-MM-DD` 或首个源文件 mtime 补日期。

## 4. DB 快照 / 比对

```powershell
python tools/dbsnap/dbsnap.py snap --out tests/dbsnap/before      # 默认表集见脚本 DEFAULT_TABLES
python tools/dbsnap/dbsnap.py diff tests/dbsnap/before tests/dbsnap/after
```

每表一份确定性 TSV（`ORDER BY` 全列）+ `manifest.json`（行数/sha256）。diff 以**文件实况**
为准（不信 manifest 旧 hash），发现快照被事后改动会打 `TAMPER` 并判红。

## 5. E2E 回归套件

```powershell
powershell -ExecutionPolicy Bypass -File tools/e2e/run_e2e.ps1            # 全量（含真实客户端流程 + 3 假人冒烟）
powershell -ExecutionPolicy Bypass -File tools/e2e/run_e2e.ps1 -SelfTestRed   # 红检：断言一个不存在的阶段，必须 RED
```

## 实机标定与坑（2026-10-10 实测，全部有现场证据）

### 客户端坐标（1024×768，客户端原点 = 窗口图 - (3,29)）

| 位置 | 客户端坐标 | 备注 |
| --- | --- | --- |
| 登录：用户名框 / 密码框 | (530,350) / (530,380) | WM_CHAR 逐字输入 |
| 登录：提交 | (575,435) | 实测能过；(567,411) 会落到别处，别用 |
| 选角：选择 1 号 / 2 号 | (276,550) / (828,550) | |
| 选角：开始（进游戏） | (517,552) | |
| 选角：创建人物 | (514,581) | 打开「新加入」面板 |
| 新加入：姓名框 / 职业战士 / 性别男 / 提交 | (670,209) / (598,267) / (632,340) / (670,466) | 建角全流程实测可用 |
| 进游戏后「公告」模态框 确定 | (522,525) | 不点掉不进世界 |

### 账号与登录

- **账号名 ≤ 10 字符**：客户端登录框会把超长账号截断（实测 15 字符的 scratch 账号被截成前 10 位，
  服务端按截断后的名字查库 → `SM_PASSWD_FAIL`）。`capture_baseline.ps1 -CreateChar` 已在开号前挡这一条。
- **LoginSrv 只在启动时把账号读进内存**（`AccountStorage.LoadAccountList`）。用 SQL 开的号在
  服务端重启前一律「此帐号不存在」——开号必须先于起栈（`capture_baseline.ps1 -CreateChar`
  已把开号排在起 MySQL 之后、起 LoginSrv 之前）。
- `account_protection` 必须有行（§8.1：缺行报「获取账号资料出错」），且 **UserName / Quiz2
  不能为空**：LoginSrv 查到空会回 `SM_NEEDUPDATE_ACCOUNT`，客户端弹「新的帐户」补填表单，
  看起来像登录失败。`account_provision.ps1` 两件事一起防。
- `account.CreateTime/ModifyTime/LastLoginTime` 是 `int(11)` 存**秒**（`int(13)` 是显示宽度，不是容量）。
- 服务端允许协议注册（`logsrv.conf EnableMakingID=1`）：假人用 `-NewAccount` 走
  `CM_ADDNEWUSER` 自助开号，不受上面的内存缓存影响。

### 网关/服务端的脆弱点（会影响抓包判读）

- **GameSvr 的网关表经不起网关连接抖动**：旧网关断开会把槽位 `UserList` 置 null
  （`M2Server/Net/TCP/TCPNetChannel.cs CloseGate`），此后进世界的玩家在 `SetGateUserList`
  上 NRE 死循环（`NewHumanList` 清不掉），客户端黑屏且**之后每次登录都失败**，直到 GameSvr 重启。
  `capture_baseline.ps1` 因此每次抓包都整栈重起。
- **探活一律用 `netstat` 文本解析，禁止 TCP 连上去碰网关**：一次 connect 就会在 GameGate
  造出幻影用户（GameSvr 日志出现 `新用户链接`），并触发上面那条 NRE。
- 假人压测（BotSrv 进程被杀）导致的连接抖动会踩同一条坑：**压测后要起真实客户端前，先重启栈**。

### 抓包拓扑的代价：进世界数据在游戏跳拿不到（2026-10-10 定位）

**先说结论**：同样的客户端 + 栈，**不经抓包插桩时进世界是成功的**
（`run_e2e.ps1` 的 `flow-world-render` 判据：`05b_after_notice.png` 非黑像素 98.82%，
角色站在比奇省、HUD 完整）。所以"金标准里进世界为空"是**抓包拓扑**的代价，不是服务端不能进。

原因：抓包需要代理占住 7000/7100/7200，影子网关因此被挪到 17000/17100/17200；
而 GameSvr 侧进世界的路由按网关端口对齐（`!servertable.txt` 发给客户端的端口就是 7200）
⇒ 客户端能连、能收公告，`CM_LOGINNOTICEOK` 之后的世界数据到不了客户端。
替代拓扑（影子绑 `127.0.0.2`、保原端口、代理占 `127.0.0.1` 原端口）试过但没走通：
影子 RunGate 仍占着 `127.0.0.1:7200`，代理绑不上（GameGate 的 `GateAddress*` 只有一个键，
另外两个 Gate 条目回落默认 `127.0.0.1`）。解除判据三条写在
`mir2-rs/tests/golden/C线-交付说明.md`。

顺带两条**只登记、未擅改 oracle** 的观察：

- 部署件不是同一批构建：网关 2026-10-09 01:18、`M2GameSvr\OpenMir2.dll` 2026-10-09 21:53、
  `M2GameSvr\M2Server.dll` 2026-10-10 00:10（= `src/GameSvr/bin/Release`）；
  `E:\MirServer\Mir200\` 那份完整部署的 `GameSvr` 是 Mach-O（macOS），本机跑不了。
  GameSvr 日志里的登录脚本报错（`CHANGEATTACKMODE`/`SendScrollMsg`/`WebBrowser`/`RecallHero`
  在当前源码 `src/Modules` 查不到）同属这一漂移面。
- **残留的 BotSrv 会让真实客户端进不了世界**：压测被中断留下的旧 BotSrv 持有 GameGate 连接期间，
  真实客户端一律黑屏（实测 08:09–09:52 如此，杀掉后立刻恢复）。压测后先确认没有残留 BotSrv。

## 已知边界（accepted risk）

- `capture_baseline.ps1` 会整栈重启（含 MySQL）。**别与其他线正在跑的服务端任务并行执行**；
  单次约 4~6 分钟。
- SelGate 影子副本的端口是**改 IL 常量**实现的（原部署文件不动，只改会话目录里的副本）。
  部署版本一变，`ldc.i4 7100` 处数就会变，脚本预检会直接报错要求人工核对——不会静默错跑。
- 假人（BotSrv）默认每个机器人间隔 3 秒上线（`AppService.cs` 内硬编码 stagger），
  200 个约 10 分钟放满；这是刻意保守，避免瞬间打爆 LoginSrv。
- `tools/capture/GoldenExport/` 引用 `src/OpenMir2/OpenMir2.csproj`，仅用于**离线解码**，
  不参与服务端构建（`OpenMir2.sln` 未包含它）。
