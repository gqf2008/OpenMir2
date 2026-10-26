# 窗口场景缺口登记（2026-10-27）

- 卡：`openmir2-window-scenarios-for-new-rcards`（tier L，owner `openmir2-dev-2`，branch `card/window-scenarios`，base `79bed3315`）。
- 背景：2026-10-27 合入的三张 R 卡，其 `window_required` 项在 `tools/window/run3-scenarios.json` 里**零命中**
  ⇒ 条件② 按「`window_required` 卡仍 in-progress」计，**没有场景的卡，窗口开多少次都不会减少一张**。
- 本文逐卡二选一（补场景 / 显式登记「需新场景」+ 缺什么前置）。**「登记 ≠ 已有真机证据」**：
  本文件登记的是**可跑性**，不是结果；两条新场景都**没有**跑过真机（本轮不占窗口）。

## 1. 逐卡处置

| 卡 | 窗口项 | 处置 | 落点 / 证据 |
|---|---|---|---|
| `openmir2-makeghost-lifecycle` | **M-2 A5**：ghost 的**周期**存档不发、**移除时**存档要发且发的是本人（DB 字段级）<br>**M-3 A6**：软关→断开窗口内两侧帧面 | **已补场景**：#25 `softclose_save`（落库面）+ #26 `softclose_relogin`（软关→同账号重连，`assert_same_actor`）。**A6 的「观察者看 ghost」腿仍缺**（见 §2.1，与 kick 同一能力） | `tools/probe/scenarios/softclose_save.json`、`softclose_relogin.json`；`run3-scenarios.json` #25/#26 |
| `openmir2-kick-sessions-residue` | 顶号后新会话**不继承** `ScriptLable`（旧会话停在子标签时，新会话点同一 NPC 应回 `@main`）与 `SellOff` 面板态 | **需新场景**：缺「**并发第二会话 / 顶号驱动**」能力（§2.1） | 见 §2.1 |
| `openmir2-merchant-dlgselect-type1` | 对 `type=1` 目标发 `CM_MERCHANTDLGSELECT`，两侧零帧 | **需新场景**：缺 (a) `type=1` NPC 的 `ActorId` 确定性来源、(b) 负向帧断言动作（§2.2） | 见 §2.2 |

## 2. 缺什么前置（逐条带证据）

### 2.1 并发第二会话 / 顶号驱动

- **为什么必须是「并发」**：顶号 = 新 `enter` 到达时**旧会话仍在世界**（`WorldServer.cs:503/535/536-537/568-570`；
  本仓 `enter_map` 的 `AlreadyInWorld` 支 → 踢旧）。旧会话一旦先断开，走的就不是顶号路径而是普通断线。
- **现状**：`tools/probe/mir2_probe.py` 一次 run 只有**一条** login→selgate→gamegate 链；
  `relogin` 步的语义是「**先关掉**当前 GameGate 连接 → settle → 再用同一 account 重跑三条链」
  （`mir2_probe.py` 的 `plan_step` `action == "relogin"` 注释），**关在前、登在后** ⇒ 表达不了「旧连接仍活着时新连接进世界」。
- **动作键清单**（`mir2_probe.py` `ACTION_KEYS`）：`send / wait_for / wait_frames / wait_idents / sleep / say /
  click_npc / dlg_select / dlg_select_any / repeat / newchr / relogin / kill` —— 没有任何「开第二会话」的键。
- **需要的新能力**（二选一，均属 probe 车道）：
  1. 一个 `peer_session` 动作：在**另一条** login→…→gamegate 链上登录同一账号/角色（保留第一条连接）⇒ 由服务端触发顶号；
     退出时两条链各自落 `phase`；或
  2. `card_window.py` 层支持「同卡双 probe」：两条 probe 进程时序编排（`peer` 段先登录并停在子标签，
     `main` 段再登录并点同一 NPC），产物合并到同一 `frames.ndjson` 的 `conn` 列。
- **同样被这条能力挡住的是**：M-3 **A6 的观察者腿**（「观察者看 ghost 角色」需要第二个客户端站在旁边）。

### 2.2 type=1 目标的确定性 ActorId + 负向帧断言

- **(a) ActorId 来源**：`click_npc` 的 `name:` / `pos:` 寻址把候选集建立在 `scan_actors()` 上，
  而 `scan_actors` 解析的是 **`SM_TURN(10)`** 帧的名字尾（`mir2_probe.py` 的 `_resolve_npc_selector` 注释：
  「`name:<NPC 名>` —— 匹配 `SM_TURN(10)` 出现帧的名尾 `ShowName`」）。`GuildOfficial`（type=1）的转身是
  `Random(40)==0`（`src/GameSrv/Npc/GuildOfficial.cs:36-47`，≈10s 期望）⇒ **寻址本身是概率性的**。
  而点它又必须先知道它的 `ActorId`（`CM_CLICKNPC` 的 recog），而 oracle 对 type=1 的点击**恒 0 帧**
  （`docs/window-runbook-20261023.md` §10：`FindMerchant=(IMerchant)actor` 硬转抛 `InvalidCastException`，
  `PlayObject.Message.cs:392-400` 吞掉）⇒ 没有「点一次 → 从 643.recog 学 id」这条路。
- **(b) 负向断言**：本卡与 `king_dialog`（已封存）的判据都是「**两侧零帧**」。现有动作里
  `wait_for` 超时即 fail-loud、`wait_idents`/`wait_frames` 都要求**出现**；
  **没有**「某区间内不得出现 ident X」的动作 ⇒ 「零帧」只能由人在 `frames.ndjson` 上目视，无法机判。
- **需要的新能力**：① 一种非 RNG 的 `type=1` 目标寻址（例如按 `Envir/Npcs.txt` 的 placement 由 harness 侧解析后
  `--var npc_actor=<id>` 注入——与现有 `--var` 机制同形，但需要有脚本从 `Npcs.txt`+placement 推出 id 并作为 discovery 来源）；
  ② 一个 `expect_absent`（`{idents:[643,645], after_step:N, window_ms:M}`）动作，超时**成功**、命中即失败。

## 3. 已落地（本轮）

| # | 场景 | 覆盖 | 证据 |
|---|---|---|---|
| 25 | `softclose_save` | M-3 A1 的**移除时存档**：走一格 → `CM_SOFTCLOSE(1009)` → 不重连 ⇒ 两侧 `characters` 行必须是走位后的实时坐标（(321,328)，不是基线 (320,328)） | `card_window.py` 在 probe 之后**无条件**跑 `wait_save_settled` + `dbsnap`（`tools/conformance/card_window.py:5426-5466`）；`--check-scenario` rc=0；`card_window --dry-run` rc=0（两侧 plan 都含 dbsnap） |
| 26 | `softclose_relogin` | 软关 → 同账号重连（两段 ActorId 相同，`assert_same_actor`） | 该场景 **2026-10-25 就已合入**（`e57024dbc`），但**从未进过批次**（`run3-scenarios.json` 24 条里没有它）⇒ 今天补进 #26 |

**证据边界（必须写进窗口记录）**：
- ghost 的**周期**存档（`SaveHumanRcdTime` 默认 10 分钟）在窗口内**不可观测**；#25 只证「移除时那次」。
- #25/#26 都是**单客户端**视角；A6 的观察者腿见 §2.1。

## 4. 两来源同步

- `tools/window/run3-scenarios.json`：#25 / #26（`--list` 实况 `场景数 26（blocked 4 ⇒ 入批 22）`）。
- 本文 + `docs/window-exec-sheet-20261025.md` ①3 / ②（新增两行）同时改。
