# C 线金标准交付说明（2026-10-10）

本目录下的 C 线产物：

| 文件 | 内容 | sha256 |
| --- | --- | --- |
| `c-line-baseline-20261010-092858.jsonl` | 金标准抓包（契约 JSONL，replay 直接消费） | `5d0ab4e66825dd133d5d7c3686db77986b465a4ed0d7179d215ede05afbcc64f` |
| `c-line-baseline-20261010-092858.manifest.json` | 帧/流对账清单（N 帧、每帧 sha256、总 hash、复现命令） | — |
| `c-line-baseline-20261010-092858.frames.ndjson` | 逐帧表（含原始线上帧的起始偏移，可对回裸 dump） | — |

- 来源机器：本机（Windows 10 Pro 19045）；真实客户端 `D:\MirClient-run\run-release.cmd`（冻结，未改一行）；
  服务端 `E:\MirServer`（只读未改，网关用会话目录里的影子副本接管端口）。
- 抓取时间：2026-10-10 09:28:58（会话目录 `tests/golden/session-20261010-092858`）。
- 账号：scratch 账号 `gp092857`（建角用；≤10 字符，见下"坑"），流程含「创建人物」全流程。
- 裸 dump（字节真值）在仓库 `tests/golden/session-20261010-092858/proxy/conn-*.bin`，
  `frames.ndjson` 的 `off/len` 可逐帧对回去。

## 复现命令

```powershell
# 前置：服务端全栈在跑
$acct = "gp" + (Get-Date -Format HHmmss)      # 必须 <=10 字符（客户端登录框会截断）
powershell -ExecutionPolicy Bypass -File tools/capture/capture_baseline.ps1 `
  -Account $acct -Password $acct -CreateChar -CharName ("gc" + (Get-Date -Format HHmmss))
# 脚本会整栈循环（含 MySQL）→ 影子网关 + 抓包代理占 7000/7100/7200 → 真实客户端全流程 → 恢复原栈
# 结束后：tests/golden/session-<时间戳>/{proxy,frames.ndjson,manifest.json,capture.jsonl,shots}
# 导出契约 JSONL：dotnet run --project tools/capture/GoldenExport -c Release -- \
#   --session <会话>/proxy --out <capture>.jsonl
```

## 阶段覆盖（对照本目录 README 的八阶段表）

| 阶段 | 覆盖 | 证据（ident） |
| --- | --- | --- |
| 登录 | ✅ | c2s 2001(CM_IDPASSWORD)；s2c 529 |
| 选服 | ✅ | c2s 104(CM_SELECTSERVER)；s2c 530 |
| 选角 | ✅ | c2s 100；s2c 520 |
| 建角 | ✅ | c2s 101(CM_NEWCHR)；s2c 521(成功)；随后再查询 100/520 已含新角色 |
| 进世界 | ✅（到"公告确认"为止） | c2s 103 + `**账号/角色/...` 登录串；s2c 525、658(SM_SENDNOTICE)；c2s 1018(CM_LOGINNOTICEOK) |
| 移动 | ❌ 缺 | 需要真进世界；见下"阻塞" |
| 攻击 | ❌ 缺 | 同上（比奇省安全区无怪，且当前进不了世界） |
| 小退 | ❌ 缺 | 同上（Alt+X 只在世界内有效） |

16 帧（c2s 9 / s2c 7；7000 跳 5、7100 跳 8、7200 跳 3），0 心跳，0 解码失败。

## 已知缺口：进世界之后服务端不再发字节（抓包拓扑所致，不是回放的问题）

症状（本次抓包的裸字节就是证据）：客户端在 7200 跳发出 `**账号/角色/...` 登录串后，
服务端只回了 `SM_SENDNOTICE`(658, 320B 公告文本)，客户端回 `CM_LOGINNOTICEOK`(1018) 之后
**服务端再无任何字节**（进程内 `在线数: 0`，但客户端断开时角色仍被存档）。

### 关键对照：**同样的客户端 + 栈，不经抓包拓扑时进世界是成功的**

用 `tools/e2e/run_e2e.ps1` 直接对活栈跑真实客户端（无影子网关、无代理），
`05b_after_notice.png` 是完整世界画面（角色站在比奇省 639,645，HUD/聊天框齐全，
非黑像素占比 98.82%；黑屏判据见 `flow-world-render`）。⇒ 缺口出在**抓包插桩的方式**上，
不是客户端、不是协议、也不是"服务端根本进不去世界"。

### 定位到的原因：影子网关的**端口**被挪了

抓包需要代理占住 7000/7100/7200，而这几个端口原本由 oracle 的网关监听，
所以影子网关被挪到 17000/17100/17200。登录跳与选角跳对此无感（数据走连接本身），
但**游戏跳的进世界路由在 GameSvr 侧按网关端口对齐**（`!servertable.txt` 发给客户端的端口就是 7200）：
端口一挪，客户端能连、能收公告，`CM_LOGINNOTICEOK` 之后的世界数据就再也到不了客户端。

替代拓扑也试过、但**没走通**（记在这里免得别人从头再试）：把影子网关绑到 `127.0.0.2`、
端口保持原值（7000/7200），让代理占 `127.0.0.1` 的原端口 —— 理论上端口语义不变，
实测卡在「影子 RunGate 仍占着 `127.0.0.1:7200`，代理绑不上」这一步
（GameGate 的 `GateAddress*` 配置只有一个键，另外两个 Gate 条目回落默认 `127.0.0.1`）。

### 解除判据（三条任选其一，都属 oracle/服务端侧，不是 C 线工具能自行决定的）

1. 让 oracle 的网关**监听端口即对外端口**（例如 oracle 自己把网关挂在 17000/17100/17200，
   同时把 `!servertable.txt` 与 LoginSrv 的 `AddrTable.txt` 一起指向这些端口）——代理即可占官方端口做插桩；
2. 或给 GameGate 的另外两个 Gate 条目补上 `GateAddress2/3`，使 `127.0.0.2` 方案真正成立（需实测复核）；
3. 或按"抓包不需要插桩"的方向改（例如 oracle 侧支持把客户端协议 dump 到文件）。

补齐后同一条复现命令重抓即可拿到 移动/攻击/小退。

### 另外两条独立发现（都只登记、未擅改 oracle）

- **部署件与源码版本漂移**：`E:\MirServer` 各组件时间戳不同批（网关 2026-10-09 01:18、
  `M2GameSvr\OpenMir2.dll` 2026-10-09 21:53、`M2GameSvr\M2Server.dll` **2026-10-10 00:10**，
  与 `src/GameSvr/bin/Release` 同刻）；`E:\MirServer\Mir200\`（另一份完整部署）里的 `GameSvr`
  是 **Mach-O（macOS）二进制**，本机启动不了。GameSvr 日志里还有登录脚本报错：
  `QManage.txt`/`QFunction-0.txt` 用到 `CHANGEATTACKMODE`/`SendScrollMsg`/`WebBrowser`/`RecallHero`，
  这些命令在当前源码 `src/Modules` 里查不到。是否影响其它行为需 owner 判断。
- **残留的 BotSrv 进程会让别的客户端进不了世界**：一个 08:09 起就在跑的旧 BotSrv
  （上一次压测被中断留下的）持有 GameGate 连接期间，真实客户端一律卡在黑屏；
  把它杀掉后立刻恢复正常（09:53 那次进世界的直接前后对照）。压测后请确认没有残留 BotSrv，
  再跑真实客户端。

## 抓包侧的两个坑（复现时会遇到）

1. **账号名 ≤10 字符**：客户端登录框会截断超长账号（实测 15 字符被截成前 10 位 → `SM_PASSWD_FAIL`，
   表现为"登录失败、后续阶段全空"）。`capture_baseline.ps1 -CreateChar` 已加了长度前置校验。
2. **服务端必须整栈全新启动**：GameSvr 的网关表经不起网关连接抖动（旧网关断开把槽位 `UserList` 置 null，
   此后进世界的玩家在 `SetGateUserList` 上 NRE 死循环、客户端黑屏且**之后每次登录都失败**）。
   所以抓包脚本每次整栈重起；探活也一律被动查监听表，绝不用 TCP 连上去碰网关（一次 connect 就造幻影用户）。

## M0 回放验收结果（A 线执行；已合入 master `0cd9f04c`）

A 线用同一份 JSONL 跑 `cargo run -p replay`，结论原文如下（**不得读成全绿**）：

```
== replay report: mir2-rs/tests/golden/c-line-baseline-20261010-092858.jsonl ==
frames: 16 (其中纯字符串帧 1)
byte mismatches (①): 0          ← 逐字节回放全过
field mismatches (②): 0         ← ident/recog/param/tag/series/body长/hash 全过
distinct idents: 13   unknown idents (未实现): 0
stage coverage (③): 登录2 选服2 选角2 建角2 进世界2 ｜ 移动0 攻击0 小退0
RESULT: RED（exit 1）—— 仅因阶段覆盖 5/8，缺 移动/攻击/小退
```

- ①②（逐字节回放 + 字段对拍）**已验收闭环，差异数 0**；③ 未满足，按 RED 记录。
- 抓包 sha256 复核：A 线用 `git show HEAD:<file> | sha256sum` 与本文登记的
  `5d0ab4e6…c64f` 逐字节一致。
- 帧尾 `hop/tail` 模型被真实数据自证：login 跳 s2c（529/530）实测尾字节 `21 24`=`!$`，
  sel/game 跳（520/521/525/658）为 `21`=`!`；A 线另做了「去掉 login 帧的 `$`」红检
  → 精确报帧尾与 hop/tail 期望不符、exit 1。
- 覆盖断言顺带抓到一个真问题并已登记：第 1 帧 `ident=3501` 是**客户端独有**的
  `CM_QUERYDYNCODE`（冻结客户端 `ClMain.pas:16542`，体 = `g_LoginKey`，默认字面量 `"password"`；
  该帧 body 8 字节、sha256 `5e884898…` 正是 ASCII `password`）⇒ 服务端语义为静默忽略、不回包。
  这条同时反证了抓包确为真客户端流量。
- 完整报告与证据：`mir2-rs/tests/parity/evidence/M0/金标准验收报告.md`。

### 补齐剩余阶段的最小动作

按上文"解除判据"任一条修好后，用同一条 `capture_baseline.ps1` 复现命令重抓 + `GoldenExport`
导出新 JSONL，A 线 `cargo run -p replay -- tests/golden/<新文件>.jsonl` 即出完整验收；
新文件请按 `README.md` 的表登记来源/时间/sha256（含 `hop`；如出现"7000 跳但无 `$`"的
LoginGate 自身帧，加 `"tail":"!"` 显式覆盖）。
