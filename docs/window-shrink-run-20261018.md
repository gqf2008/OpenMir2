# 缩窗 W0-1..W0-5 实跑记录（2026-10-18 06:1x–06:4x，owner 授权「只开缩窗」）

> 范围 = **缩窗**：W0-1..W0-5，**不含 RNG 两侧账本**（整包 W0 的 O1–O7 未裁决）。
> 代码基线 `84b696b05`（P3 B1–B5 全落）；开窗前三连全 GREEN。
> 收窗后九游戏端口全 DOWN、shadow gate 已杀、游戏进程 0、DB 角色位置已还原（Hp 由 0 补到满，便于下次登录）。

## 1. 开窗前（W0-00，全 GREEN）

| 步 | 命令 | 结果 |
| --- | --- | --- |
| ① 端口 | `E:/MirServer/port-status.ps1` | 九游戏端口全 DOWN；仅 `mysqld pid=3306` |
| ② Rust 窗口门 | `client-e2e-window.fixed.sh.txt guard` | `WINDOW_GUARD_OK`（R1..R8 OK） |
| ③ C# 抓包前置 | `check_csharp_capture_preflight.py --shadow-config …/shadow/run-gate/config.conf` | `RESULT: GREEN` |
| ④ 摆位 | `UPDATE characters SET MapName='0', CX=605, CY=578` | `0 605 578` |
| ⑤ 无写手 | 停掉 `p3-residuals` 车道（cargo/rustc 写手） | 无 `cargo/rustc/game-svr` 残留 |

## 2. C# 侧（W0-1 / W0-5 证据）

- 起栈：`start-csharp-hook.ps1 -Seed 42 -LogDir E:/tmp/window/cliente2e/csharp/stack`
  （DBSrv/LoginSrv/GameSvr-shadow/gates 全起，pid 清单见该 LogDir）。
- 网关拓扑：停真 `RunGate`(7200) → 起 **shadow RunGate(7600)**（`E:/tmp/window/cliente2e/shadow/run-gate`，
  `Count=1/GatePort1=7600`）→ `mir2_proxy` 占 7200。
- 真客户端 `mir_flow.ps1`（账号 `mir2test`/角色 `aaa`）成功进世界；
  `export_s2c.ps1` 导出（`proxy`、`proxy2`..`proxy6` 各一份，`s2c*`）。
- **世界数据到达**：一份 capture 内 15 个 actor 的 `SM_TURN(10)/SM_WALK(11)` 坐标 + `SM_FEATURECHANGED(41)`；
  另有公告 658 / `SM_LOGON(50)` / `SM_NEWMAP(51)` 等。

**W0-1（玩家→怪 阳性对照）：未达成（如实记录）**
- 尝试 5 次（`-AttackTurn "0,1"` + `-AttackRing "0,40…"`、不同 AutoPlaySec/HP）：
  一次拿到了 `CM_TURN(3010)×1 + CM_HIT(3014)×1`（proxy5），但**没有出站 `SM_STRUCK/SM_NOWDEATH`**；
  其余各次点击被客户端判为走位（`CM_WALK/RUN`），玩家位置漂移（HUD 与摆位不一致）。
- 根因（待办）：自动化点击的像素锚（精灵中心 512/259 vs 格心锚 512/304）与怪物实时格位不同步；
  摆位到怪旁后，登录 ~10s 内怪已游走。**需要窗口内人工点怪一次**（或先抓当帧截图再按格心点击）。
- **结论：W0-1 仍未取得 GREEN 样本；W0-2 因此没有基准，未执行。**

**W0-5（怪→玩家 探针）：两侧都有入站 `SM_STRUCK`**
- `check_attack_observability.py --expect any --allow-unknown-self`：
  - C#：入站 `SM_STRUCK(31)` **6** 条、入站 `SM_NOWDEATH` 0（出站 0 属 W0-1 项）。
  - Rust（同一 `(605,581)` 摆位、同客户端）：入站 `SM_STRUCK` **3** 条、入站 `SM_NOWDEATH` **1**。
- 按 runbook §2d W0-5 的判读口径「看入站栏 ≥1」⇒ **两侧均出现怪打玩家**；
  工具整体 RED 只因它同时要求 `CM_HIT≥1` 且出站伤害（那是 W0-1 的判据）。
- 备注：Rust 侧玩家在 5000 有效 HP 下仍出现 1 次死亡（`SM_NOWDEATH` 入站）——HP 可能被服务端
  会话能力重算覆盖（后续窗口需用服务端权威 HP 摆位）。

## 3. Rust 侧（W0-2/W0-3/W0-4）

- 起栈：`start_rust_stack.ps1 -RustBinDir E:/tmp/m2svr-window-target/debug -Seed 42
  -LogDir E:/tmp/window/cliente2e/rust/stack`
  （Rust game-svr 5000 + login/sel gate + C# DBSrv/LoginSrv；Rust game-gate 因先于 5000 就绪退出，
  按 runbook 拓扑不用它——shadow gate 7600 + proxy 7200 服务两侧）。
- **W0-3 ③a 反向点 `(311,666)`：Rust 侧 0 个其它 actor（PASS，单侧）**
  - 前置 `check_reverse_point_clean.py --point 311,666 --map 0 --vr 12` ⇒ `CLEAN`（最近对象 GuardList (281,661) d=30）。
  - Rust capture：`other actors = 0`。**C# 侧同点本窗未跑**（需再起 C# 栈），W0-3 双侧结论待补。
- **W0-4 GuardList 帧：未观测到（不结论）**
  - 摆位 `(291,666)`，客户端 HUD 实际落在 **`(286,652)`**——**进图前移位环**把角色移走了
    （这是 `openmir2-enter-shift-loop` 的真机旁证：摆点不可走时会移位）。
  - capture 内有 5 个 actor 的 `SM_TURN` 坐标（261..269）与 `SM_FEATURECHANGED`，但**无 `(281,661)`**；
    可能原因：卫士静止 ⇒ 只发 `FEATURECHANGED`（无坐标帧），或移位后实际视距内无该卫士。
    **需窗口内人工/截图核对**。
- **W0-2：未执行**（依赖 W0-1 的 GREEN 基准）。

## 4. 收窗与残留

- `start_rust_stack.ps1 -Stop`；`Stop-Process` 杀 shadow gate(pid 3480)。
- 九游戏端口全 DOWN；仅有 `mysqld 3306`；无 `game-svr/GameGate/LoginSrv/DBSrv/sel-gate/login-gate` 进程。
- DB 角色还原：`MapName='0', CX=291, CY=652`，`Hp=MaxHP=19`（原 `Hp=0`——补满以便下次登录）。
- 证据：`E:/tmp/window/cliente2e/{csharp,rust}/{proxy*,s2c*,shots*}`（保留）。
- 挂起：`p3-residuals` 车道在开窗前暂停（其 patch 在 `/e/tmp/p3res.patch`），窗口后再验收。

## 5. 下次窗口的最小后续（按优先级）

1. **W0-1 人工点怪**：登录后先在截图里确认怪在相邻格，再右键该格 + 左键该格（或给 `mir_flow` 补
   「按格心锚 512/304 点击相邻格」的确定性模式）。拿到 C# 出站 `SM_STRUCK` 后再跑 W0-2。
2. **W0-4**：改用「先登录、截图定实际落点与卫士相对位置」再摆位，或直接用截图判卫士出现。
3. **W0-3 C# 侧**同点复跑，凑齐双侧 `--expect none`。
4. 整包 W0（RNG 账本）仍待 O1–O7 裁决。
