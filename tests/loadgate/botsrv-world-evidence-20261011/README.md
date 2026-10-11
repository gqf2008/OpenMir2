# 假人侧「进世界 / 在线」证据完整性 —— 原始件（卡 `openmir2-botsrv-world-evidence`，T-12）

时间：2026-10-11 17:37–17:44（本机）
作者：`openmir2-worker-5`（c6f 车道，worktree `c6`）
被测栈：**当时已在跑的 C# 参考栈**（`E:\MirServer`：3306/5100/5500/5600/5700/6000/7000/7100/7200）。
本轮**未起栈、未停栈**（栈由 `m2svr` 车道的真机夹具占用；本线的三个 `run_bots.ps1` 只是短连接用它做假人冒烟）。
BotSrv：`src/BotSrv`（测试工具，改动登记 `T-12`），`dotnet build -c Release` 0 error。

> **测试数据副作用**：本轮在 `mir2_db` 里新增了测试账号/角色 `wbev0..2`、`wbevx0..19`、`wbevt0..2`
> 与 3 个 `loadbot` 自测账号（`run_bots -SelfTestRed` 没连上，不落库）。未改表结构、未删数据。

## 1. 改前/改后（F4）：挂机轮次按「本会话是否进世界」分账

| 运行 | 假人数 | login | in_world | `autoplay_in_world` | `autoplay_pre_world` | `autoplay_skipped_pre_world` | internal_errors | 原始件 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 修复后（默认） | 20 | 20/20 | **20/20** | 761 | **0** | 939 | 0 | `f4-fixed20.ndjson` / `report-fixed20.json` / `f4-fixed20.botsrv.log` |
| 改前对照组（`MIR2_BOT_LEGACY_SHARED_AUTOPLAY=1`） | 20 | 12/20（进程提前退出，见 §4） | 11/20 | 211 | **529** | 0 | 0 | `f4-legacy20.ndjson` / `report-legacy20.json` / `f4-legacy20.botsrv.log` |

**读法**：`autoplay_pre_world` = 该假人**本会话从未收到 `SM_LOGON`**、却仍跑了一轮挂机（作用在跨假人共享的
`MShare` 世界态上）。旧门只看 `MShare.MySelf != null`，所以任意一个假人进世界后，其余未进世界的假人也被挂机
⇒ **按 bot 名记的挂机日志不能当作「该假人进世界」的证据**（20 假人一批里 529 次这种轮次）。
T-12 后挂机门改成只认**本会话** `InWorld`（`RobotPlayer.InWorld`），改后 `autoplay_pre_world == 0`，
未进世界的假人被跳过并计入 `autoplay_skipped_pre_world`。

复现（同一批大小、同一栈）：

```powershell
# 改后
powershell -NoProfile -ExecutionPolicy Bypass -File tools/botload/run_bots.ps1 -Count 20 -Prefix wbevx -NewAccount -RequireWorld -SkipBuild -LogFile <out>.log
# 改前对照组（只看共享 MShare）
$env:MIR2_BOT_LEGACY_SHARED_AUTOPLAY="1"
powershell ... -Count 20 -Prefix wbevx -NewAccount -RequireWorld -SkipBuild -LogFile <out>.log
```

## 2. F3(a)：bot-smoke 第三条假人在 `CM_ADDNEWUSER` 后无应答 —— 根因 = T-9 的共享判据，已随 T-9 修复

`f3a-smoke3.*`（3 假人 `-NewAccount -RequireWorld`）：

- `register_sent=3` / `register_send_closed=0` ⇒ 三个 `CM_ADDNEWUSER` 都真的写进了 socket；
- `ident_counts[504]=3`（`SM_NEWID_SUCCESS`）、`ident_counts[505]=0`（`SM_NEWID_FAIL`）⇒ 服务端对三个都回了注册应答；
- `in_world=3`，日志里 `wbev0/wbev1/wbev2` 各一行 `成功进入游戏`。

`CM_ADDNEWUSER` 的应答（`SM_NEWID_SUCCESS/FAIL`）与登录应答走**同一个** `DecodeMessagePacket` 分支；
02:59 的旧冒烟之所以是「第三条无应答」，是因为该分支当时的判据是跨假人共享的 `MShare.MySelf == null`
（前两个假人之一进世界后 `MySelf != null` ⇒ 第三条的注册应答被静默丢弃）。T-9 把判据换成**本会话** `!_inWorld` 后，
同一根因消失。**这不是 C# 参照行为**，不需要 C# 侧只读计数。

## 3. 牙齿：抹掉一个假人的进世界证据 ⇒ 在线判据必红

| 运行 | 假人数 | `MIR2_BOT_SUPPRESS_INWORLD` | login | in_world | `in_world_suppressed` | `-RequireWorld` verdict | 原始件 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 还原（不抑制） | 3 | （未设） | 3/3 | **3/3** | 0 | GREEN | `f3a-smoke3.*` / `report-good3.json` |
| 抹掉 wbev2 的证据 | 3 | `2` | 3/3 | **2/3** | 1 | **RED** | `teeth-suppress.*` / `report-teeth.json` |

- 被抑制的假人**照常进世界**（日志仍有 `wbev2 成功进入游戏`），只是**不计** `in_world` ⇒ 判据必须红。
- 该判据（`-RequireWorld` / `load_report.py --world-threshold 100`）的默认是**关**的：M5 三档的 `in_world` 实测只有 ~10%，
  不能当默认阈值；缺省不判、显式给出才判（见 `report-*.json` 的 `world_threshold_pct`）。
- `report-teeth.json` 的 `fails`：`在线（本会话收到 SM_LOGON 的假人数）66.67% < 100.0%`。

## 4. 已知边界（如实登记）

1. **本轮的「同一批」是 20 假人，不是卡面举例的 200**：200 档需要独占端口窗口（`load_gate` 每档会
   `stop -> start` 整栈），本轮窗口被 `m2svr` 车道的真机夹具占用，按纪律**没起/停栈**。
   机制与计数器**与批量无关**（同一份 `ClientManager.RunAutoPlay`），改前/改后的判定（`>0` vs `==0`）已成立。
2. **legacy 对照组进程提前退出**（`f4-legacy20.botsrv.log` 在 17:41:12.825 后无输出、stderr 空、`internal_errors=0`）：
   这正是「共享态挂机作用在未进世界假人上」的危险面（旧行为本身的缺陷），不影响「改前 `autoplay_pre_world=529`」这条结论；
   修复后的路径（`f4-fixed20`）跑满 20/20、`internal_errors=0`。
3. 本轮与 `m2svr` 车道**共用当时已跑的 C# 栈**（未停未起）；`-SelfTestRed` 与 `load_report selftest` 不占端口。

## 5. 离线红检（不占端口）

- `01-load_report-selftest.txt`：`GREEN: load_report selftest 全过（…/在线判据牙齿/…）`（rc=0）。
  新增项：同一份 `in_world=2/3` 产物在 `world_threshold=100` 下必 RED、`3/3` 必 GREEN、不给阈值不得判红。
- `02-run_bots-selftest-red.txt`：指向无监听端口 ⇒ `SELFTEST_RED GREEN`（rc=0）。

## 6. report.json 的在线/基数语义（卡面 ④）

`load_report.py summarize` 写进 `report.json`：
`online_bots`（= 本会话收到 `SM_LOGON` 的假人数）、`online_base`（= 档位人数）、
`online_source`（`per-session SM_LOGON（RobotPlayer.MarkInWorld ⇒ LoadMetrics.InWorld）`）、
`world_ok_pct`、`in_world_suppressed`、`autoplay_in_world/pre_world/skipped_pre_world`、`register_sent/register_send_closed`。
详见 `tests/loadgate/README.md`《三个新增读数》。
