# 85 — ③ 窗口「C# 侧抓包拓扑」根因订正 + 离线前置门

卡 `openmir2-client-e2e-enter-gaps`（DoD③ 两侧对照的窗口准备）／m2-svr（`openmir2-worker-11`）／2026-10-14。
只读 + 离线工具，零起栈、零端口。**真机窗口仍由 root 开**；本片不采真机读数。

一句话：C 线（2026-10-10）把「抓包拓扑下 `CM_LOGINNOTICEOK` 之后世界数据下不来」归因于
「影子网关**端口**被挪」；按**当前源码**逐路径复核，该归因不成立 —— 同症状的确定根因是
**S1 缺陷①**（网关槽位按 TCP accept 序绑定 + `CloseGate` 不释放槽位），修复 `607e9b773`
（2026-10-10 17:01）已进部署件（`!deployed-from.json` = master `628d8024`，2026-10-10 23:54）。
⇒ `82` 手册 §3/§4 的「只挪 game-gate + 代理占 7200」拓扑**有根因级依据**；窗口仍须实拍确认。

---

## A. 旧归因写在哪（订正对象）

| 位置 | 原文要点 |
| --- | --- |
| `tools/capture/capture_baseline.ps1:75-83` | 「RunGate 挪到 17200 后，GameSvr 侧『进世界』的路由**按网关端口对齐**（`!servertable.txt` 发给客户端的端口就是 7200）…回完 `CM_LOGINNOTICEOK` 之后世界数据再也下不来」 |
| `tools/README.md:213-225` | 「抓包需要代理占住 7000/7100/7200，影子网关因此被挪到 17000/17100/17200；而 GameSvr 侧进世界的路由按网关端口对齐」 |
| `mir2-rs/tests/golden/C线-交付说明.md`「已知缺口」 | 「**游戏跳**的进世界路由在 GameSvr 侧按网关端口对齐…端口一挪…世界数据就再也到不了客户端」 |

> 这三处**不改**（`tools/**`、`tests/golden/**` 非本卡写集；已在 §E 登记为待订正的过期注释）。

## B. 复核：为什么「按端口对齐」站不住（源码锚点）

1. **`!servertable.txt` 不进单机进世界路径。**
   - 读取点唯一：`src/GameSrv/GameApp.cs:262-306` `LoadServerTable` → `M2Share.ServerTableList`。
   - 消费者只有两处：`src/GameSrv/GameShare.cs:57-90` `GetMultiServerAddrPort`，其调用点是
     `src/GameSrv/AppService.cs:221`（**关机做多机转移**）与 `src/GameSrv/Word/WorldServer.cs:1063-1074`
     `SendChangeServer`（→ `SM_RECONNECT`，**多机转移**）。
   - ⇒ 单机流程里，客户端的目标 GameGate 端口**不是**由 GameSvr 用 `!servertable.txt` 现下发的
     （客户端自己的选服/登录配置决定它连 7200；Rust 侧同拓扑抓包成功也是同一事实）。
2. **GameSvr 对网关的寻址按槽位，不按端口。**
   - `src/M2Server/Net/TCP/TCPNetChannel.cs:26-120`：`Connecting` 里 `AllocGateSlot()` 分配槽位并登记
     `_gateSlotByConnection[SocketId] → slot`；`Received` 按**连接**查槽位；世界侧用 `_gameGates[gateIdx]`。
   - 世界侧的 `gateIdx` = `playObject.GateIdx` = 网关在**客户端认证串**里自报的 `ServiceId`
     （`src/GameGate/Services/ClientSession.cs:1203` 拼 `**account/hum/.../ServiceId`；
     `src/GameGate/Conf/ConfigManager.cs:11-23` `ServiceId = i`）。
   - ⇒ 端口只决定客户端连到哪里；影子网关改监听端口 + 代理占原端口，不影响 GameSvr 的槽位寻址。
3. **同症状的确定根因 = S1 缺陷①**（`mir2-rs/tests/parity/evidence/S1/S1-报告.md` §1）：
   槽位曾按 `int.Parse(SocketId)-1`（accept 序）绑定，`CloseGate` 又把 `UserList/Socket` 置 null
   却不释放槽位 ⇒ 网关**重连**后世界侧寻址踩死槽位 ⇒ 下发静默丢弃（**黑屏**）+ `SetGateUserList`
   每 200ms NRE（`NewHumanList` 清不掉 ⇒ 此后每次登录都失败）。C 线这份抓包用的是**每次整栈重起 +
   影子网关**（`capture_baseline.ps1` 头部注释「网关表经不起网关连接抖动」），正落在这个触发面上。

## C. 修复与部署状态（改前/改后锚）

| 项 | 值 |
| --- | --- |
| 修复提交 | `607e9b77347f94820c1d57da7502d0a2826b680a`（`fix(oracle): 修 GameSvr 网关重连后进世界 NRE 死循环 + 部署件版本对账（S1 缺陷①②）`，2026-10-10 17:01） |
| 登记 | `whitelist.md` **T-2**；全文 `docs/Rust重写设计文档.md` §16 缺陷① |
| 部署件 | `E:/MirServer/M2GameSvr/!deployed-from.json`：`git_head=628d8024ab85b49ce36591dacb1952ac1996698a`（2026-10-10 23:53:02，`Merge branch 'worktree-s1-oracle-fixes'`，2026-10-10 23:54 铺） |
| 包含关系 | `git merge-base --is-ancestor 607e9b773 628d8024` = **YES**（本片实跑，见 `86-…txt` §1） |
| 残留边界 | S1 §2.1/§8：「多网关下各网关不按自己声明的编号顺序重连，最小空闲槽仍可能错配」（accepted risk）。**本部署 `RunGate/config.conf` 的 `ServerWorkThread=1` ⇒ 只激活 1 个网关（`ConfigManager.cs:31-45` 用它当循环上界、`ServerManager.cs:38-41` 用它 new 服务数组），ServiceId 0 ↔ 槽位 0，该边界**当前不活**。 |

## D. 窗口 actionable（并入 `82` 手册 §4）

1. **前置门（离线，起栈前先跑）**：

   ```powershell
   python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_csharp_capture_preflight.py `
     --repo E:/Users/gxh/Documents/GitHub/OpenMir2/.claude/worktrees/m2-svr `
     --mirserver E:/MirServer --shadow-config <影子 RunGate/config.conf>
   ```

   退出码 `0/1/2`（GREEN/RED/PREREQ_RED）。判据：部署件含 `607e9b773`；真实与影子 `ServerWorkThread=1`；
   真实 `GatePort1=7200`（代理端口）；`!servertable.txt` GameGate 端口 `=7200`；上游 `127.0.0.1:5000`。
2. **影子 RunGate 配置**：从 `E:/MirServer/RunGate/config.conf` 拷一份，**只改 `GatePort1=7600`**
   （绑定地址/上游地址/`ServerWorkThread` 一律不动）；不要引入第 2/3 个活动网关。
3. 其余照 `82` §3（代理 `7200 → 127.0.0.1:7600`、真客户端、`export_s2c.ps1`）。

## E. 仍未证 / 登记（不静默）

- **「应该能」不是「已经能」**：本片只做静态复核；C# 世界数据能否经代理下来，**必须窗口实拍**
  （看 `chunks.ndjson` 里 7200 跳在公告后是否还有 s2c 字节）。失败则按 `82` §4 降级判据
  （截图级两侧对照）并在卡上写明。
- `tools/capture/capture_baseline.ps1:75-83`、`tools/README.md:213-225`、
  `mir2-rs/tests/golden/C线-交付说明.md`「已知缺口」三处过期注释**未改**（非本卡写集）⇒ 登记待订正。
- `ServerWorkThread=1` 是**当前部署**的读数；若窗口前有人把它改成 3，前置门会判 RED（这正是它的牙）。
- 本片**零生产代码改动**；只加只读判据工具 + 文档。

## F. 涉及文件

- 工具（新）：`mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_csharp_capture_preflight.py`
- 证据（新）：本文件、`86-csharp-capture-preflight.txt`、`87-gates.txt`、`88-boundary.txt`
- 手册（改）：`82-two-side-window-runbook.md` §4
- 只读引用：`src/GameSrv/GameApp.cs:262-306`、`src/GameSrv/GameShare.cs:57-90`、`src/GameSrv/AppService.cs:221`、
  `src/GameSrv/Word/WorldServer.cs:1063-1074`、`src/M2Server/Net/TCP/TCPNetChannel.cs:26-120`、
  `src/GameGate/Conf/ConfigManager.cs:11-46`、`src/GameGate/Services/ServerManager.cs:38-41`、
  `src/GameGate/Services/ClientSession.cs:1203`、`mir2-rs/tests/parity/evidence/S1/S1-报告.md` §1/§2.1/§8
