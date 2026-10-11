# M1 S4b ③「退出路径」跨实现互读 —— 证据与口径（2026-10-12，worktree `dbsrv6k`）

卡：`openmir2-m1-dbsrv-6000-face`（issue `370e332f`）｜owner `openmir2-worker-17`｜
分支 `worktree-m1-dbsrv-6000`（base = `refs/heads/master`）。

> 本片是卡内 **③ 的最后两条退出路径**：`softclose`（小退，`CM_SOFTCLOSE 1009`）与 `shutdown`
> （服务端优雅关）。`disconnect`（掉线）路径已由片 4（`mir2-rs/tests/parity/evidence/M1/s4b-slice4-crossread/`）覆盖。
> ② 6000 `DataService` / ④ `TimedService` / ④ 5700 `MarketService` 均已合入 master。

---

## 0. 结论（先看）

| 判据 | 结果 | 证据 |
| --- | --- | --- |
| **softclose**：C# DBSrv 写 → **Rust DBSrv 读/写**，存档字段级 diff = 0 | ✅ `GREEN: 作用域内 9 张表逐行一致` | `09-diff-reports.txt` #1 |
| **softclose**：Rust DBSrv 写 → **C# DBSrv 读/写**，存档字段级 diff = 0 | ✅ 同上 | `09-diff-reports.txt` #2 |
| **shutdown**：C# 写 → Rust 读，DB diff = 0 | ✅ 同上 | `09-diff-reports.txt` #3 |
| **shutdown**：Rust 写 → C# 读，DB diff = 0 | ✅ 同上 | `09-diff-reports.txt` #4 |
| softclose 确实发生了**存档**（非空跑） | ✅ `characters.LASTUPDATE`/`ablity.ModifyTime` 前→后推进 | `14-save-vs-nowrite-evidence.txt` |
| shutdown **没有**发生存档（两侧一致地不写） | ✅ 两实现各 2 轮 before==after | `14-save-vs-nowrite-evidence.txt`；登记 `M1-EXIT-2` |
| 红检（牙齿）：改坏被比对列必 RED | ✅ `characters.Level`、`ablity.Hp` 各 1 例 | `10-redcheck.txt` |
| 四门禁（隔离 target `E:\tmp\openmir2-dbsrv6k-target`，`--offline`） | ✅ fmt 0 / clippy 0 warn / test **71 suites 526 passed 0 failed 16 ignored** / msgcodegen **604** | `11-gates.txt` |

**关键归因**：`shutdown` 路径的 diff=0 是「**两侧都不写**」——C# oracle（`628d8024` 部署）的优雅停机
**不 flush** 在世界玩家的存档（登记 `M1-EXIT-2`），故它不是 DBSrv 存档保真度的证据。真正的
in-world 存档函数 `WorldServer.SaveHumanRcd`（= `SavePlayer` 调用的同一个）由 **softclose 路径**
（`MakeGhost` 后调用）覆盖，且跨实现双向 diff = 0 ⇒ DBSrv 的存档序列化（`DB_SAVEHUMANRCD`）在
双向互读下 1:1。

---

## 1. 为什么用 BotSrv 夹具而不是真 GUI 客户端

`softclose` 需要客户端发 `CM_SOFTCLOSE(1009)`。真客户端（`D:\MirClient-run`，冻结）的小退走
`TfrmMain.AppLogout → 确认框 → CM_SOFTCLOSE`，但**合成输入打不穿那个模态框**（`tools/client/mir_flow.ps1`
头部注释与实测：Alt+X / 底部「小退」按钮都能弹出确认框，`logout_confirm` 点击后截图仍是确认框
`08_back_charsel.png`，客户端没回到选角界面）。⇒ 本轮用仓库自带 headless 客户端 `src/BotSrv`，
新增一个**默认关**的夹具开关直接发该包（登记 `M1-EXIT-1`，同 T-22 之例）。

`shutdown` 不需要客户端发特殊包：bot 停在世界里，由夹具给 `GameSvr` 的控制台发 `CTRL_BREAK`
触发 host 优雅停机。

---

## 2. 栈构成（唯一变量 = DBSrv 实现）

| 端口 | C# DBSrv 侧 | Rust DBSrv 侧 |
| --- | --- | --- |
| 5000 | **同一 C# `E:\MirServer\M2GameSvr\GameSrv.exe`** | **同一 exe** |
| 5100/6000/5700 | C# `src\DBSrv\bin\Release\DBSrv.exe` | Rust `E:\tmp\openmir2-dbsrv6k-target\debug\db-srv.exe` |
| 5500/5600 | C# `src\LoginSrv\bin\Release\LoginSrv.exe` | 同左 |
| 7000/7100/7200 | C# `E:\MirServer\{LoginGate,SelGate,RunGate}` | 同左 |
| 3306 | MariaDB `D:\mysql\mariadb-10.11.19-winx64` | 同左 |
| 客户端 | headless `src/BotSrv`（完整客户端协议栈） | 同左 |

- 起停栈：`side_stack.ps1 -Dbsrv csharp|rust -Action start|stop`（Rust db-srv 用影子配置
  `E:\tmp\openmir2-dbsrv6k\stack\db-srv\dbsrv.conf`；**不写 `E:\MirServer`**）。
- 运行态归属逐进程见各 `*.baseline.json` 的 `runtime_procs`（记到绝对路径）；`12-ports-ownership.txt` 是端口→实现映射。

---

## 3. BotSrv 夹具（`M1-EXIT-1`，默认关）

- `RobotOptions.SoftCloseAfterWorldMs`（默认 0=关）：>0 时收到本假人 `SM_LOGON` 后等 N ms 发一条
  `CM_SOFTCLOSE(1009)`（`RobotPlayer.SoftCloseTick`，落 `M1SOFTCLOSE sent ident=1009` 标记），
  线上形状 = C# 客户端 `AppLogout()` 的 `SendClientMessage(CM_SOFTCLOSE,0,0,0,0)`。
- `RobotOptions.DisableAutoPlay`（默认 false）：关掉客户端自动挂机游走（`ClientManager.RunAutoPlay`
  早退）。挂机走位由**无种子 RNG** 驱动 ⇒ 存档 `CX/CY` 不可复现，跨侧对拍必然假红。
- 两开关的**默认关 = 逐字节不变**自检（`run_bots.ps1 -WriteConfigOnly` 两向）：
  默认 `SoftCloseAfterWorldMs=0` / `DisableAutoPlay=false`；打开为 `5000` / `true`。
- 只写**本假人自己的 socket**，不读不写跨假人共享世界态（同 T-22 纪律）。

---

## 4. 「同角色跨侧互读」口径 + 定点归一（为什么）

片 4 的 `disconnect` 用「两侧各全新前缀」比 after；那条路当时 `characters` 行 `MapName=NULL,CX=0`
（角色未落位即断线），没暴露建角/落位的随机性。本轮 softclose/shutdown 的存档**带世界坐标**，
于是暴露两类**夹具噪声**（都是 **C# GameSvr 产生**、不是 DBSrv 差异）：

1. **建角出生点随机**：`WorldServer.ProcessHumansMakeNewHuman` 的 `CurrX/Y = GetRandHomeX/Y`（无种子 RNG）；
   且 **DBSrv 的 5100 建角面**两侧落库本身也有可观测差异（同 Job=1 下 `Hair`、初始 `HP` 会不同）。
2. **在世界里随机挨打**：`XP`/`HP` 在两次运行之间随机变化（实测同一实现两轮可在 16/15 间跳）。

⇒ 本片改用 **同一角色、两侧轮流读/写**（owner 口径「两侧同 prefix 各跑一遍」），并在每轮**动作前**
用 SQL 把该角色**定点/回满**（`MapName='0', CX=291, CY=666` = 比奇省安全区；`ablity.HP=MaxHP, MP=MaxMP`）。
两侧因此从**同一初始状态**出发，`softclose`/`shutdown` 的存档即可逐字段比。

- 定点是**夹具归一**，不是判据：改的是夹具输入（位置/HP），不是被比对的服务端行为。
- 定向后 `Hp=16` 两侧一致（`09-diff-reports.txt` GREEN）；红检证明 `Hp` 列**真的在比**
  （把 `ablity.Hp` 16→9 ⇒ `RED: 作用域内 2 行不一致`，`10-redcheck.txt`）。

> 注：不把 `Hp` 挪进易变白名单。红检先证明它有牙齿，再证明两侧一致。

---

## 5. 采集序列（每条命令见 `run_exit_path.ps1` 头部）

```
# 侧 = csharp|rust；每侧一套栈。prefix 全片同一个：m1ex2（账号/角色 m1ex20）
1) C# 栈   run_exit_path.ps1 -Side csharp -Path softclose  -Prefix m1ex2 ...            # 建角（NewAccount），存档丢弃
2) C# 栈   run_exit_path.ps1 -Side csharp -Path softclose  -Prefix m1ex2 -NoNewAccount   # W1
3) C# 栈   run_exit_path.ps1 -Side csharp -Path shutdown   -Prefix m1ex2 -NoNewAccount   # S1（CTRL_BREAK 停 GameSvr）
4) Rust 栈 run_exit_path.ps1 -Side rust   -Path softclose  -Prefix m1ex2 -NoNewAccount   # W2
5) Rust 栈 run_exit_path.ps1 -Side rust   -Path shutdown   -Prefix m1ex2 -NoNewAccount   # S2
6) C# 栈   run_exit_path.ps1 -Side csharp -Path softclose  -Prefix m1ex2 -NoNewAccount   # W3
7) C# 栈   run_exit_path.ps1 -Side csharp -Path shutdown   -Prefix m1ex2 -NoNewAccount   # S3
```

对拍（`scoped_diff.py diff --canonical --fixed-newchr`）：
`W1↔W2`、`W2↔W3`、`S1↔S2`、`S2↔S3` ⇒ 四条全 GREEN（`09-diff-reports.txt`）。

---

## 6. `shutdown`：CTRL_BREAK 的优雅停机（登记 `M1-EXIT-2`）

`send_ctrlc.ps1` 的机理：`FreeConsole` → `AttachConsole(GameSvr pid)` → `SetConsoleCtrlHandler(NULL,true)`
（自己忽略）→ `GenerateConsoleCtrlEvent(1, 0)`（CTRL_BREAK，发给该控制台上的所有进程）。

- 实测 `CTRL_C_EVENT(0)` **不**触发（`Program.cs` 的 `Ctrl+C pressed` 不出现）；`CTRL_BREAK_EVENT(1)` 触发，
  GameSvr 日志出现 `Ctrl+C pressed`、`后台服务停止`（`TimedService.StopAsync`），进程优雅退出。
- 但 `characters.LASTUPDATE` 不变、Rust db-srv 无 `保存角色` ⇒ `AppService.OnShutdown → SavePlayer` 未落库。
  两实现各 2 轮 shutdown 的 before==after（`14-save-vs-nowrite-evidence.txt`）。
- 归因（登记 `M1-EXIT-2`）：两侧同一 C# GameSvr ⇒ 行为一致；该路径 diff=0 是「两侧都不写」，
  **不构成 DBSrv 存档保真度证据**；由 softclose 路径覆盖同一 `SaveHumanRcd`。

---

## 7. 附带观测（未进判据）

- **Rust db-srv 的 `DATETIME` 存 UTC、C# DBSrv 存本地时**：同一保存时刻，C# 落 `2026-10-12 05:07:01`、
  Rust 落 `2026-10-11 21:11:07`（= UTC+8）。`LASTUPDATE`/`ModifyTime` 等墙钟列在 `scoped_diff` 里
  登记为易变（占位），故本片把它当噪声；**是否属 Rust 侧应修的行为差异，另开卡/另裁**（本片不改
  存储层，避免与 S4b 已合入的存档 SQL 口径冲突）。

---

## 8. 未覆盖 / 登记

| 项 | 状态 | 依据 |
| --- | --- | --- |
| `shutdown` 的 in-world `SavePlayer` 存档 | **oracle 侧不可达** | 登记 `M1-EXIT-2`（C# 优雅停机不 flush；两侧一致） |
| BotSrv 夹具改动 | **登记例外** | `mir2-rs/tests/parity/whitelist.md` 的 `M1-EXIT-1`（默认关、逐字节不变自检） |
| 真 GUI 客户端 小退 | 不采用 | 合成输入打不穿确认框（§1） |
| NAT `softclose` 用真 GUI 客户端 | — | 同上 |

---

## 9. 四门禁 / 残留

- 门禁原始件 `11-gates.txt`（隔离 target `E:\tmp\openmir2-dbsrv6k-target`，`--offline`）。
- 证据件冻结字节：本目录 `.gitattributes = * -text`，`SHA256SUMS.txt` 可在任何检出上复核。
- 残留物：`E:\tmp\openmir2-dbsrv6k\`（影子配置/日志/隔离 target，跨片复用）、`E:\tmp\openmir2-dbsrv6k\slice5\`
  （本轮原始件，未入库）。仓库内新增仅本目录与本片代码改动。
- 收工停栈：见 `E:\tmp\openmir2-watch\last-dbsrv6k.md`（端口状态 + 窗口已释放）。
