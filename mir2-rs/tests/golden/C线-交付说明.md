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

## 已知阻塞：进世界之后服务端不再发字节（不是回放的问题）

症状（本次抓包的裸字节就是证据）：客户端在 7200 跳发出 `**账号/角色/...` 登录串后，
服务端只回了 `SM_SENDNOTICE`(658, 320B 公告文本)，客户端回 `CM_LOGINNOTICEOK`(1018) 之后
**服务端再无任何字节**（进程内 `在线数: 0`，但客户端断开时角色仍被存档）。

旁证：

- GameSvr 日志 `新用户链接...` 有，之后没有世界数据相关日志；`在线数: 0`。
- `src/M2Server/Player/PlayObject.cs` 的入口是 `LoginNoticeOk → UserLogon()`，即客户端确认公告后
  应由服务端主动下发世界数据；本次没发。
- GameSvr 日志里有登录脚本报错：`QManage.txt` 第 27-31/350/351 行、`QFunction-0.txt` 第 90/1551/1556 行，
  命令 `CHANGEATTACKMODE`/`SendScrollMsg`/`WebBrowser`/`RecallHero` 在**当前源码** `src/Modules` 里查不到。
- `E:\MirServer` 部署件不是同一批构建：三个网关 2026-10-09 01:18、`M2GameSvr\OpenMir2.dll` 2026-10-09 21:53、
  `M2GameSvr\M2Server.dll` **2026-10-10 00:10**（与 `src/GameSvr/bin/Release` 同刻）；
  `E:\MirServer\Mir200\`（另一份完整部署，2026-10-09 01:32）里的 `GameSvr` 是 **Mach-O（macOS）二进制**，本机无法启动。

⇒ 结论：这是**部署件与源码版本漂移**（登录脚本用到的命令在当前源码里不存在），属于服务端侧问题，
不属于协议层、也不属于本次抓包；C 线按"只做工具、不改 oracle"的边界只登记与取证，未擅自改 `E:\MirServer`。
解除判据：把 oracle 换成可用的原始整套部署（或补齐脚本引擎命令后整套重建部署——那是 B 线/owner 的范围），
届时用同一条复现命令重抓即可补齐 移动/攻击/小退 三个阶段。

## 抓包侧的两个坑（复现时会遇到）

1. **账号名 ≤10 字符**：客户端登录框会截断超长账号（实测 15 字符被截成前 10 位 → `SM_PASSWD_FAIL`，
   表现为"登录失败、后续阶段全空"）。`capture_baseline.ps1 -CreateChar` 已加了长度前置校验。
2. **服务端必须整栈全新启动**：GameSvr 的网关表经不起网关连接抖动（旧网关断开把槽位 `UserList` 置 null，
   此后进世界的玩家在 `SetGateUserList` 上 NRE 死循环、客户端黑屏且**之后每次登录都失败**）。
   所以抓包脚本每次整栈重起；探活也一律被动查监听表，绝不用 TCP 连上去碰网关（一次 connect 就造幻影用户）。
