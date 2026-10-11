# M2 会话生命周期 · 夹具自检（DoD ① 的「夹具自检」半）—— 2026-10-12

- 卡：`openmir2-m2-session-lifecycle`（owner `openmir2-worker-11`）
- 分支 / worktree：`worktree-m2-gamesvr-host` / `.claude/worktrees/m2-svr`
- 写集：`tools/m2-acceptance/**`（不起栈、不起客户端、不占端口）

## 1. 这一片解决什么

卡 DoD ① 的原文要求「复跑 GREEN（**夹具自检** + 真机）」。真机那半必须占 5000/7000/7100/7200（窗口按
owner `e532961e` 的队列排在本 lane 之后），但**判定逻辑本身**是纯文本分类，可以离线自检——本片就是把
`session_lifecycle_fixture.ps1` 的观测逻辑抽成单一真源并纳入 `selftest.ps1`，让「起栈后怎么判」在起栈前就有牙齿。

- `session_lifecycle_fixture.ps1`：新增 offline 模式 `-OfflineLogA <旧日志> -OfflineLogB <新日志> [-OfflineBExited]`；
  判定抽成 `Get-KickObservation`（旧收到什么）/ `Get-NewSessionVerdict`（新连接去向），**offline 与真机共用同一实现**
  （不是第二份口径）。真机主流程行为不变，仍 `0=GREEN / 1=RED / 2=PREREQ`。
- `selftest.ps1`：+9 条 `life/*`（12m–12u），用构造日志把两条判据跑一遍 ⇒ `total=47 failed=0`。

## 2. 命令与结果

```powershell
# 绿：47/47，exit 0（本片新增 9 条在 ④ 段）
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/selftest.ps1
# 红检：把 Get-NewSessionVerdict 的 "成功进入游戏" 匹配串改坏 ⇒ 4 条 FAIL / failed=4 / exit 1；还原后 47/47
# （红检的原始输出见本目录 selftest-red.txt；是用「改文件→跑→git checkout 还原」的口径采的）
```

| 文件 | 内容 |
| --- | --- |
| `selftest-green.txt` | 绿跑原始输出：`SUMMARY total=47 failed=0` / exit 0 |
| `selftest-green.json` | 绿跑 `selftest.json`（逐条 name/expect/actual/ok） |
| `selftest-red.txt` | 红检原始输出：`SUMMARY total=47 failed=4` / `SELFTEST FAIL` / exit 1 |
| `selftest-red.json` | 红检 `selftest.json`（4 条 `life/*` = false） |
| `offline-recorded-roundA.txt` | 用**已入库真机证据**（2026-10-11 轮 A）喂 offline 模式：`old_received=KICKUSER_SYSTEM_MSG new_verdict=ADMITTED in_world=True` ⇒ 与 `fixtureA-botbot.json` 一致 |

## 3. 口径（照实测，不臆造）

- 旧连接：账号级 `SM_SYSMESSAGE`（文案 `当前登录帐号正在其它位置登录…`，`TCPNetChannel.KickUser`）**优先于**
  528（`服务器连接被强行中断`）与普通断开（`关闭连接`）。
- 新连接：**先判 `成功进入游戏`**（先放行、之后才断的会话仍算 `ADMITTED`，不得误报 `CLOSED`）；否则 `关闭连接`
  等断开标记 ⇒ `CLOSED`；两者皆无且进程已退 / 仍在跑 ⇒ `PROCESS_EXITED` / `TIMEOUT`（**非零退出，绝不静默通过**）。

## 4. 残留

- 真机 ①（起真 C# 栈 + 夹具复跑，两支竞态）**未做**：窗口队列（owner `e532961e`）在本 lane 之前，
  本片不抢窗口、未起栈、未占端口。真机跑完用 `-OfflineLogA/-OfflineLogB` 对已采日志复判即可。
- 本目录只保留本片新增件；真机 2026-10-11 的两轮证据在 `tools/m2-acceptance/evidence/session-lifecycle-20261011/`。
