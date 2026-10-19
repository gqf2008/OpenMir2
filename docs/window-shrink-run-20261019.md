# 缩窗 W0-1..W0-5 第二轮实跑记录（2026-10-19 11:5x–12:3x，owner 授权「只开缩窗」）

> 范围 = **缩窗**：W0-1..W0-5，**不含 RNG 双侧账本**（整包 W0 的 O1–O7 仍未裁决）。
> 代码/二进制基线 = **`7b2a8e7d5`**（`E:/tmp/m2svr-window-target/window-provenance.json`
> `source_head=7b2a8e7d53d5343b1fac4fed12a9216541e30c86`，六件 exe 重编后 `--record-provenance`）。
> 上一轮记录：`docs/window-shrink-run-20261018.md`（W0-1/W0-2 未达成、W0-4 未观测）。

## 0. 开窗前置（全 GREEN）

| 步 | 命令 | 结果 |
| --- | --- | --- |
| ① 端口 | `E:/MirServer/port-status.ps1` | 九游戏端口全 DOWN；仅既有 `mysqld:3306` |
| ② Rust 窗口门 | `client-e2e-window.fixed.sh.txt guard` | **`WINDOW_GUARD_OK`**（R1..R8） |
| ③ C# 抓包前置 | `check_csharp_capture_preflight.py --repo . --shadow-config …/shadow/run-gate/config.conf` | **`RESULT: GREEN`** |
| ④ 摆位 | `UPDATE characters SET MapName='0', CX=605, CY=578`（+ `characters_ablity.Hp`） | 每趟起栈前重跑 |
| ⑤ 无写手 | 代码车道（MB-4）在独立 worktree/target 跑，未占端口；抓包期间实测 `cargo/rustc` 进程数 = 0 | OK |

- 拓扑与上一轮一致：真 RunGate(7200) 停掉 → **影子 RunGate(7600)**（GBK conf 仅 `Count=1/GatePort1=7600` 两处差异，
  `GameGate.exe/.dll` 与部署件 sha256 相同）→ `mir2_proxy` 占 7200 抓包。
- 起/停同一份脚本、同一 `-LogDir`；C# 侧 `start-csharp-hook.ps1 -Seed 42`，Rust 侧
  `start_rust_stack.ps1 -RustBinDir E:/tmp/m2svr-window-target/debug -Seed 42`。

## 1. W0-1（C# 阳性对照）——**首次拿到 GREEN** ✅

- 点位 `(605,578)`（`MonGen.txt` 5 行刷怪中心），角色 level 1 / HP 17/19（oracle 原生布局，D-9 关闭侧）。
- 驱动（入库件 `mir_flow.ps1`，`-AttackTurn` 配方）：
  `-WalkClicks 0 -AutoPlaySec 0 -AttackClicks 4 -AttackTurn "0,-1" -CenterY 259 -SkipReenter -SkipLogout -SkipTalk`
- `check_attack_observability.py --expect any --allow-unknown-self`：
  **`cm_hit=6 出站 sm_struck=4 sm_nowdeath=0｜入站 sm_struck=2｜self=941945091`** ⇒ **GREEN**。
- 证据：`E:/tmp/window/cliente2e/csharp/{proxy-attack,s2c-w01n,shots-w01n}`。
- 意义：**「玩家→怪」伤害帧两侧此前都没有实拍样本**，本轮先把 GREEN 钉在 oracle 实拍足迹上（判据校准完成）。

## 2. W0-2（Rust 同流程）——GREEN ✅（附一条口径说明）

- 同点位 `(605,578)`、同客户端/账号/角色。**失败分支记录（如实）**：
  - level 1 + `-AttackTurn "0,-1"/"0,1"` 三次尝试均 `cm_hit=0`：该点刷怪密度高，客户端进图后
    ~14s 内即被打死（`SM_NOWDEATH`），而 `mir_flow` 的固定节拍（notice_poll 12×1.2s）要到 ~20s 才发攻击；
    且死亡后左键全落成 `CM_WALK`。
  - 把 DB 临时抬到 `Level=30`（`characters_ablity.MaxHP` 由 Level 重算，直接改 MaxHP 会被
    `RecalcAbilitys` 覆盖——两侧一致，不是偏差）后角色可存活；但 `-AttackTurn` 的 48×32 像素步长
    在 Rust 侧连续 3 次仍落在空格（见 §6 F2）。
- **改用截图逐像素点击**（新入库件 `tools/client/mir_click.ps1`：`PrintWindow(PW_RENDERFULLCONTENT)` 截图 +
  `WM_LBUTTONDOWN/UP`/`WM_RBUTTONDOWN/UP`，消息路径与 `mir_flow.Send-Click/Send-RightClick` 同一套）：
  先右键点**截图里怪的格心**（触发 `CM_TURN`），再左键同点。
- 结果：**`cm_hit=8 出站 sm_struck=7 sm_nowdeath=1（击杀）｜入站 sm_struck=59｜self=13463`** ⇒ **GREEN**。
- 证据：`E:/tmp/window/cliente2e/rust/{proxy-live,s2c-live,live2.png,live3.png}`。
- ⚠ 口径：W0-1 的 GREEN 是 **level 1** 取的；W0-2 的 GREEN 是 **level 30** 取的（存活需要）。
  两侧都证明「真实客户端 → 服务端 → 出站伤害帧」端到端成立；**数值不用于逐字节对照**。

## 3. W0-3（③a 反向点 `(311,666)`）——两侧 0 对 ✅

- 前置 `check_reverse_point_clean.py --point 311,666 --map 0 --vr 12` ⇒ **CLEAN**（最近确定性对象 GuardList `(281,661)` d=30）。
- C#：`appearance_pairs=0` ⇒ **GREEN**（`s2c-w03`）。
- Rust：`appearance_pairs=0` ⇒ 该项通过；整条命令仍 **RED**，原因见 §6 **F1**（入图帧序 52/752，与本步无关）。

## 4. W0-4（GuardList 帧一致 `(291,666)`）——两侧对称出帧 ✅

- C#：**恰 1 对** `SM_TURN(recog=761229890, 281, 661)` + `SM_FEATURECHANGED`（`s2c-w04`）。
- Rust：**恰 1 对** `SM_TURN(recog=152, 281, 661)` + `SM_FEATURECHANGED`（`s2c-w04`）。
- 与 `docs/window-shrink-run-20261018.md` §4c 的修后预期一致（上轮「卫士无帧」的 AOI 缺陷已修复且**真机复验通过**）。
- 两侧 `recog` 不同（C# 大整数随机、Rust 小序号）⇒ `--strict-recogs` 仍**不得**当判据（与会话 ID 生成方式有关，已登记）。

## 5. W0-5（怪打玩家探针）——两侧入站 ≥1 ✅

- C#（同点）：入站 `SM_STRUCK` **2**、出站 0（W0-1 之前的那趟）。
- Rust（同点）：入站 `SM_STRUCK` **59**、`SM_NOWDEATH` 0（level 30）；level 1 三趟均在 ~14s 内入站致死。
- 判读口径「看入站栏 ≥1」⇒ 两侧都成立；**主动索敌在两侧都真实发生**。
- 备注：Rust 侧同样点位击杀速度显著快于 C#（level 1：14s 内 5–8 次受击即死；C# 同点同血 ~30s 仅 2 次）。
  这是**待查线索**（怪 AI 攻击节拍/目标选择），本轮不下结论。

## 6. 本轮发现

### F1【R 级，新缺陷】入图帧序 `52 SM_ABILITY` / `752 SM_SUBABILITY` 两侧相反

- oracle（C#，本机**四个独立会话**：`s2c`(探测) / `s2c-w01n` / `s2c-w03` / `s2c-w04`）入图前缀恒为
  `658,51,50,41,213,11029,42,766,54,5008,1106,**52,752**,811,46`。
- Rust（两个会话：`s2c-w04` / `s2c-live`）恒为 `…,1106,**752,52**,811,46`。
- `bins/game-svr/src/smframe.rs` 的注释与单测显式按「模型 C」（部署 `M2Server.dll` 的 IL + net8.0 穷举）
  断言 `752` 在 `52` 前，并在注释里写明「与 2026-10-10 capture 的差别只有 52/752 的先后」——
  **本轮四个真机会话与 2026-10-10 capture 一致地否定了模型 C**。
- 性质：帧序 = 时序类，**R 级**；`check_client_s2c_frames.py` 因此对 Rust capture 判 RED
  （`enter_seq` 前缀不符），这是**判据正确报警**，不是夹具问题。
- 处置：登记卡片并按 oracle 改回 `52 → 752`（含单测期望、`smframe` 头注订正）。

### F2【工具/L 级】`mir_flow -AttackTurn "dx,dy"` 的像素步长会落到非目标格

- 实现按 `(512 + dx*48, 304 + dy*32)` 点「相邻格」。本轮 Rust 侧同一配方 3 次（`0,-1`×2、`0,1`×1）
  全部落成 `CM_WALK`（角色走位），而 `CM_TURN` 只在 C# 那次出现。
- 现成可靠的替代路径 = **按截图里怪的像素位置点**（`tools/client/mir_click.ps1`，本轮已入库并用于 W0-2）。
- 处置：登记为工具改进（把「格心锚 + 48/32」换成由客户端 `CXYfromMouseXY` 反推的等距投影步长，
  或直接把 `mir_click` 的截图点击编进 `mir_flow`）。

### F3【已知偏差复验】D-9 的 52 体长（oracle 40B vs Rust 58B）真机复现

- 同一 DB 状态下：C# `52` body_len=**40**，Rust `52` body_len=**58**（`MIR2_CLIENT_ABILITY_LAYOUT` 默认开）。
- 冻结客户端上 C# 本尊 HUD 显示 `131072/1310`（垃圾值），Rust 显示正常的 `290/419`——与 D-9 登记描述完全一致。
- 处置：无新动作（D-9 已裁决）。

## 7. 收窗

- C#/Rust 栈全停；**九游戏端口全 DOWN**（含影子 7600），仅 `mysqld:3306`；无 `MirClinet` 残留。
- DB 角色还原到开工前：`MapName='0', CX=296, CY=653, Level=1, Hp=19, MaxHP=19, Exp=0, MaxExp=10`。
- 证据保留：`E:/tmp/window/cliente2e/{csharp,rust}/`（`proxy*`、`s2c*`、`shots*`、`live*.png`）。
- 新增入库件：`tools/client/mir_click.ps1`（截图 / 右键 / 左键，供后续窗口按像素驱动）。

## 8. 下次窗口的剩余项

1. F1 修复合入后，重编六件并**重跑一次 Rust 侧入图抓包**，确认 `enter_seq` 与 oracle 同序（`check_client_s2c_frames.py` GREEN）。
2. `-AttackTurn` 的投影步长修正后，用**纯 `mir_flow` 配方**在 Rust 侧复现 W0-2（不再依赖截图点击）。
3. Rust 侧「level 1 同点 14s 内被围殴致死 vs C# 同点 30s 仅 2 次受击」的差异，另立卡查怪物攻击节拍。
4. 整包 W0（RNG 两侧账本）仍待 O1–O7 裁决。
