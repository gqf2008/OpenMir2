# M2 会话生命周期 DoD ① —— 真机夹具证据（2026-10-11）

- 卡：`openmir2-m2-session-lifecycle`（issue `8a629398`，owner `openmir2-worker-11`）
- 分支 / worktree：`worktree-m2-gamesvr-host` / `m2-svr`
- 栈：真 C# 栈（`E:\MirServer\start-all.ps1`：MySQL + DBSrv + LoginSrv + GameSrv + 三网关），端口 5000/7000/7100/7200 独占
- 客户端：`src/BotSrv`（仓库自带 headless 客户端，完整客户端协议栈）+ 真 Delphi 客户端 `D:\MirClient-run\run-release.cmd`
- 夹具脚本：`tools/m2-acceptance/session_lifecycle_fixture.ps1`
- 题面（issue DoD ①，原话）：「起真 C# 栈（E:/MirServer，端口窗口独占），同账号/角色在旧会话仍在线时再登录一次；产出『命令+输出+路径+hash』，写清：旧连接收到什么、新连接被放行还是被 CloseUser 关、旧 PlayObject 何时从 PlayObjectList 消失、此后何时能进世界」

> **口径纪律**：owner comment `272582e1` 明确「口径必须以夹具实测为准」⇒ 本目录只登记**实测**；
> oracle 源码路径（`WorldServer.cs:503/535/536-537/568-570` 的 else 分支）只在报告里作对照，不作为结论。

## 1. 两轮实测（都命中「旧会话在线时同账号再登录」）

| 轮 | 旧会话 | 新会话 | 结果 | 证据 |
| --- | --- | --- | --- | --- |
| A | BotSrv | 另一个 BotSrv（同账号 `fixt0`/同角色） | **新连接被放行**（进世界成功） | `fixtureA-*.log` / `fixtureA-botbot.json` |
| B | **真 Delphi 客户端** | BotSrv（同账号 `fixt0`/同角色） | **新连接被关闭**（RunGate/SelGate 在 0.7s 后断，未进世界） | `fixtureB-*` |

两轮的旧连接收到的是**同一种**东西。⇒ 见 §2。**新连接放行/关闭取决于竞态**，见 §3。

## 2. 观测项 ①：旧连接收到什么（两轮一致）

- 文本：`当前登录帐号正在其它位置登录，本机已被强行离线!!!`
- 来源（源码定位，非臆测）：`M2Server/Net/TCP/TCPNetChannel.cs:229` 的 `KickUser` → `PlayObject.SysMsg(sKickUserMsg, MsgColor.Red, MsgType.Hint)`
  （`BaseObject.cs:1588+`）→ `RM_SYSMESSAGE` → `SM_SYSMESSAGE`（**不是** `SM_OUTOFCONNECTION(528)`）。
- 证据：
  - BotSrv 侧：`fixtureA-botA-old.out.txt` `17:32:38.967 [INF] 当前登录帐号正在其它位置登录，本机已被强行离线!!!`
  - 真客户端侧：`fixtureB-realclient-06b-after-kick.png`
    —— 截图里角色**仍在比奇省世界里**，聊天框最后一行是红字 `当前登录帐号正在其它位置登录，本机已被强行离线!!!`。
- **旧连接不立即断开**：`fixtureA-closeobs-old.out.txt`（同账号再登录那轮）里旧会话在 kick 提示后仍持续动作，
  `17:34:22.106 [WRN] 游戏服务器[127.0.0.1:7000]关闭连接...` 才断（≈49s 后）。⇒ 不是「收到 528 即断」。

## 3. 观测项 ②③④：新连接去向 / 旧 PlayObject 何时离场 / 何时能再进

判定变量是 `WorldServer.ProcessHumansIsLogined`（`WorldServer.cs:200-217`）= `FrontEngine.InSaveRcdList(chr)` **或** `PlayObjectList` 有同名。
它命中时 `:535` 置旧 `BoKickFlag`、`:536-537` 记新、`:568-570` `NetChannel.CloseUser(新)`。

### 轮 A（BotSrv 旧 + BotSrv 新）——新被放行

| 时刻 | 事件 | 证据 |
| --- | --- | --- |
| 17:32:34.979 | 旧进世界 | `fixtureA-botA-old.out.txt` |
| 17:32:38.967 | 旧收到 kick 提示 | 同上 |
| 17:32:39.741 | 新连 RunGate（SessionId 1） | `fixtureA-gamesvr-1732.txt` 同窗口 |
| 17:32:40.074 | GameSvr：`角色数据保存中,稍后重新读取. Account:fixt0 ChrName:fixt0` | `fixtureA-gamesvr-1732.txt` |
| 17:32:42.383 | **新进世界成功**（`成功进入游戏`） | `fixtureA-botB-new.out.txt` |

⇒ 当旧数据保存已结束、旧已离场时，`ProcessHumansIsLogined=false` ⇒ 新放行。

### 轮 B（真客户端旧 + BotSrv 新）——新被关

| 时刻 | 事件 | 证据 |
| --- | --- | --- |
| 17:36:27.744 | 真客户端旧连 RunGate（SessionId 0） | `fixtureB-rungate-1736.txt` |
| 17:36:55.213 | 新连 SelGate | `fixtureB-selgate-1736.txt` |
| 17:36:56.281 | 新连 RunGate（SessionId 1） | `fixtureB-rungate-1736.txt` |
| 17:36:56.572 | GameSvr：`角色数据保存中,稍后重新读取. Account:fixt0 ChrName:fixt0` | `fixtureB-gamesvr-1736.txt` |
| 17:36:57.012 / .013 | **新的 RunGate/SelGate 断开**（`CloseUser(新)`）；新 BotSrv 日志止于 `进入游戏`，**没有** `成功进入游戏` | `fixtureB-rungate-1736.txt` / `fixtureB-selgate-1736.txt` / `fixtureB-realclient-bot.out.txt` |
| 17:37:39.168 | 旧的 RunGate 才在真客户端小退时断（此前一直在世界） | `fixtureB-rungate-1736.txt` |

⇒ 当旧数据仍在保存（`InSaveRcdList=true`）时，`ProcessHumansIsLogined=true` ⇒ `CloseUser(新)`，新进不了世界；
而 `KickOnlineUser(旧)` 此时在 `PlayObjectList` 里已找不到旧（旧早被账号级 kick 软关并移出），所以旧没收到 528。

### 观测项 ③④ 的实测答案

- **旧 PlayObject 何时离场**：账号级 kick（旧收 `SM_SYSMESSAGE` 提示、置 `BoEmergencyClose/BoSoftClose`）后由
  `PlayObject` 循环收尾 → `PlayObjectList.Remove` + `AddToHumanFreeList` + `SaveHumanRcd`（`WorldServer.cs:705-710`）；
  实测旧数据保存日志在 kick 后 ~1.1s 出现，此后旧即可被同名会话取代。
- **此后何时能进世界**：数据保存一结束、同名不再命中 `InSaveRcdList/PlayObjectList` 时可进（轮 A 新会话在 kick 后 ≈3.4s 进世界成功）。

## 4. 结论（交给 owner 裁决）

1. **旧的处置两轮一致**：账号级 kick 发 `SM_SYSMESSAGE` 提示（`TCPNetChannel.KickUser`），**不是** `SM_OUTOFCONNECTION(528)`；
   旧连接不立即断开。
2. **新的去向是竞态**：命中 `InSaveRcdList`/`PlayObjectList` ⇒ `CloseUser(新)`（轮 B，真客户端）；否则放行（轮 A）。
   owner 原口径「踢旧 + 关新，两边都不放行」对应的是**命中时**那支；轮 A 证明该支**不一定命中**。
3. 现 Rust `bins/game-svr` 的 ③（`enter_map` 返回 `AlreadyInWorld` ⇒ 踢旧 + 关新、旧收 `528`）——
   - 「关新」与 oracle 命中支一致；
   - 「旧收 `528`」与实测不一致（实测旧收的是账号级 `SM_SYSMESSAGE`，且由登录链路的账号级 kick 触发），
     若 Rust 登录链路没有对应的账号级 kick，则 Rust 的旧/新行为都会与 oracle 端到端不同。**请 owner 裁决。**

## 5. 证据清单（`tools/m2-acceptance/evidence/session-lifecycle-20261011/`）

| 文件 | 内容 |
| --- | --- |
| `fixtureA-botbot.json` | 轮 A 夹具判定（旧收到/新 verdict/时间线） |
| `fixtureA-botA-old.out.txt` / `fixtureA-botB-new.out.txt` | 轮 A 两侧客户端日志 |
| `fixtureA-gamesvr-1732.txt` | 轮 A GameSvr 窗口日志 |
| `fixtureA-closeobs-old.out.txt` | 旧连接 kick 后不断开的补测（≈49s 才断） |
| `fixtureB-realclient-06b-after-kick.png` | 真客户端在旧会话被 kick 后仍在世界 + 红字提示（第一轮） |
| `fixtureB-realclient-bot.out.txt` | 轮 B 新会话（BotSrv）日志：止于 `进入游戏`，未 `成功进入游戏` |
| `fixtureB-rungate-1736.txt` / `fixtureB-selgate-1736.txt` | 轮 B 网关侧：新连接 0.7s 后断开 |
| `fixtureB-gamesvr-1736.txt` / `fixtureB-loginsrv-1736.txt` | 轮 B GameSvr / LoginSrv 窗口日志 |

复现脚本：`tools/m2-acceptance/session_lifecycle_fixture.ps1`（前置不足时 `exit 2`，绝不静默通过）。
