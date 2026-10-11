# M2 会话生命周期 DoD ① —— 真机夹具复跑证据（2026-10-12）

- 卡：`openmir2-m2-session-lifecycle`（issue `8a629398`，owner `openmir2-worker-11`）
- 分支 / worktree：`worktree-m2-gamesvr-host` / `m2-svr`（head 见卡上 patch）
- 栈：**真 C# 栈**（`E:\MirServer\start-all.ps1`：MySQL + DBSrv + LoginSrv + GameSrv + 三网关），
  端口 5000/5100/5500/5600/5700/6000/7000/7100/7200 独占
- 客户端：`src/BotSrv`（仓库自带 headless 客户端，完整客户端协议栈）
- 夹具脚本：`tools/m2-acceptance/session_lifecycle_fixture.ps1`（本片**未改脚本**，只复跑取证）
- 台账 / 首轮报告：`docs/M2会话生命周期-对齐Rust-20261011.md`、`docs/M2会话生命周期夹具报告-20261011.md`

> 本目录是 **DoD ①『真机』半的复跑证据**：证明夹具脚本在当前树上、在真 C# 栈上能跑通，
> 且观测口径与 2026-10-11 记录的实测一致。**不预先断言分支**，只登记实得。

## 0. 一句话结论

复跑两轮（`run2` / `run3`）都 **GREEN（exit 0）**，实得 **同一口径**：
**旧会话收到账号级 `SM_SYSMESSAGE` 提示「当前登录帐号正在其它位置登录，本机已被强行离线!!!」且不立即断开；
新会话被放行（ADMITTED，`成功进入游戏`）** —— 与 2026-10-11 轮 A（bot 旧 + bot 新）逐条一致。

## 1. 复跑结果（两轮）

| 轮 | 命令 | 旧会话（BotA）收到 | 新会话（BotB）去向 | 退出码 | 原始件 |
| --- | --- | --- | --- | --- | --- |
| run2 | `session_lifecycle_fixture.ps1 -OutDir E:/tmp/m2acc-life/run2 -SkipBuild` | `KICKUSER_SYSTEM_MSG` @05:29:11.366，其后仍持续动作（未立即断） | `ADMITTED`（`成功进入游戏` @05:29:14.779） | 0 | `run2-fixture.json`、`run2-botA-old.out.txt`、`run2-botB-new.out.txt` |
| run3 | 同上（`-OutDir …/run3`） | `KICKUSER_SYSTEM_MSG` | `ADMITTED`（`成功进入游戏`） | 0 | `run3-fixture.json`、`run3-botA-old.out.txt`、`run3-botB-new.out.txt` |

- 旧会话（`run2-botA-old.out.txt`）：`05:29:11.366 [INF] 当前登录帐号正在其它位置登录，本机已被强行离线!!!`，
  之后仍连续 `随机搜寻目标...` 到 05:29:18 ⇒ **不是「收到 528 即断」**（口令与 2026-10-11 §2 一致）。
- 新会话（`run2-botB-new.out.txt`）：`05:29:14.779 [INF] 机器人:[fixt0] 成功进入游戏`。
- `run2/run3-gamesvr-window.txt`：观测窗口内 GameSvr 增量（`新用户链接...`）。

## 2. 分支登记（照 owner brief §8 的「无法稳定复现 ⇒ 登记」）

本夹具（**bot 旧 + bot 新**，`Stage2DelaySec=3`）在真栈上**两轮都命中 `ADMITTED` 支**。
另一支（**新连接被 `CloseUser` 关**）在 2026-10-11 轮 B 实测到过一次，其旧会话是**真 Delphi 客户端**：

- 证据（已入库）：`tools/m2-acceptance/evidence/session-lifecycle-20261011/` 的
  `fixtureB-realclient-*` / `fixtureB-rungate-1736.txt` / `fixtureB-selgate-1736.txt`
  （新 BotSrv 止于 `进入游戏`、无 `成功进入游戏`；RunGate/SelGate 在 ~0.7s 后断开）。
- 竞态窗口（源码锚点，非臆测）：`WorldServer.ProcessHumansIsLogined`（`WorldServer.cs:200-217`）
  = `FrontEngine.InSaveRcdList(chr)` **或** `PlayObjectList` 有同名；命中 ⇒ `:535` 置旧 `BoKickFlag`、
  `:536-537` 记新、`:568-570` `NetChannel.CloseUser(新)`。
  该判定发生在**新会话的登录处理内**，与「旧会话的数据保存/离场」在同一 GameSvr 线程上竞速 ⇒
  用 `Stage2DelaySec` 之类的外部延时**控制不到**这个微竞态，故 bot/bot 夹具**不可稳定构造 `CLOSED` 支**。
- 观测方法（要复现 `CLOSED` 支）：按 2026-10-11 轮 B 的形态——旧会话用**真 Delphi 客户端**
  （`D:\MirClient-run\run-release.cmd` + `tools/client/mir_flow.ps1 -Account fixt0 -KickVia fixt`）先进世界，
  再由 BotSrv（前缀 `fixt`）同账号再登录；新 BotSrv 日志无 `成功进入游戏` 即判 `CLOSED`。
  **本片未再跑轮 B**（GUI 驱动 + 写 `tests/botload/**` 日志，收益不抵风险）⇒ 以上述已入库件为 `CLOSED` 支证据。

## 3. 端口 / 收工

| 阶段 | 5000/7000/7100/7200 | 其余 | 证据 |
| --- | --- | --- | --- |
| 取窗口前 | 全空 | 仅共享 MySQL 3306 | `ports-before.txt` |
| 起栈后 | 全 C#（GameSrv / LoginGate / SelGate / GameGate） | 5100/5700/6000 DB、5500/5600 Login | `commands.txt` §1 |
| 停栈后 | 全空 | 仅共享 MySQL 3306 | `ports-after.txt` |

- 收工 `Stop-Process` 只停 6 个 C# 组件，**保留共享 MySQL 3306**，与开工前一致。
- 未触碰他人栈；未起 Rust 侧服务；未改 `src/**`、`MirClient`、表结构/脚本/数据格式。

## 4. 证据 hash

见 `SHA256SUMS.txt`（**按仓库字节记 = `SHA256(Git blob 内容)`**：本目录件是 `text` 文件，
即 **CRLF→LF 归一后的 sha256**；复核器 = `python -I tools/evidence-hash/verify_sha256sums.py <本目录>`）。

> **口径订正**（2026-10-12，卡 `openmir2-evidence-hash-convention`，`docs/Rust重写设计文档.md` §6.0-4）：
> 本清单原按**盘上 CRLF**记 ⇒ 对仓库字节复核 8/11 mismatch（N-1）；本片已改记为仓库字节口径，
> 复核 **11/11 GREEN**（`E:/tmp/...` 已在同卡报告登记；对账表见 `tools/evidence-hash/RECONCILE.md`）。

脚本文件 `tools/m2-acceptance/session_lifecycle_fixture.ps1` 本片**未改动**（hash 以分支 head 上的 blob 为准）。
