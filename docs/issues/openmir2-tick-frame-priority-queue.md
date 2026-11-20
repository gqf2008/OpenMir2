# openmir2-tick-frame-priority-queue —— tick 内帧统一走 `NetPriorityQueue`

> 状态：**登记卡（2026-11-20，`openmir2-2f` 提议、`opc-19` 定为唯一出口）**。**不实现**，只定义范围与判据。
> 起因：657（`RM_CHARSTATUSCHANGED`，优先级字节 **155**）在 oracle 走 `MsgQueue`，本仓多处路径改为
> **直发** ⇒ 帧内序不可控。今天已有四处"泵的近似"栽在这上面（等键序、两遍排水、排序当泵、直发当泵），
> 本卡是这类问题的**唯一出口**：以后所有"差不多按序"的近似都往这里靠，不再各自发明。

## 1. 事实（为什么必须有一个统一解）

- oracle 出队键 = `(byte)wIdent` 升序：`RM_ABILITY(67)` < `RM_HEALTHSPELLCHANGED(68)` <
  `RM_LEVELUP(150)` < `RM_CHARSTATUSCHANGED(155)` < `RM_SYSMESSAGE(252)`。
- 本仓现状是"**混用**"：一部分帧走 `SpellFrameBuf` 的每收件人 `NetPriorityQueue`，一部分
  （657、210/212、部分 45/52/752）是**直发**（`route_to_gate` / `tx.send`）⇒ 序 = 调用序。
- 已登记的偏差正是它的产物：**T-288**（GM 改属性：657 先于 203）、**T-289–T-294**
  （六处 recalc 路径干脆没接 657——因为一接就会按调用序插错位置）。
- **反例（必须写进判据）**：把 657"延后到 tick 末端统一冲刷"**不是解**——`RM_SYSMESSAGE` 的 252
  **大于** 155，oracle 里 sysmsg 必须排在 657 **之后**；延后冲刷会让 657 跑到 sysmsg 后面，
  换个方向错。任何"按阶段排"的近似都会被 252 类帧打脸。

## 2. 目标

让 **tick 内所有下行帧**（不论产生点是函数内 recalc、drain、脚本还是 GM 命令）都进同一张
**每收件人 `NetPriorityQueue`**，出队序 = `(byte)ident` 升序（同键按入队序，§29 已确认堆出队语义）。

## 3. 清点（第一遍，2026-11-20，只读）

方法：扫 `bins/game-svr/src/**/*.rs` 的 `route_to_gate(` 与 `SimToGate::SendFrame {`，
按"最近的 `fn`"归属，±12 行内取 `sm_*` 作为该点帧 ident；lib.rs 以首个 `^mod tests {` 为生产边界。

| 口径 | 数字 |
|---|---|
| `route_to_gate(` 调用（全仓 game-svr） | **138**（lib.rs 130 / guild_official 4 / castle 2 / guild_cmd 1 / readbook 1） |
| `SimToGate::SendFrame {` 出现（含测试） | 368 |
| **lib.rs 生产面**直发点（边界行 33024 之前） | **239** |
| 其中 ident 可机读（±12 行有 `sm_*`） | **103**（86% 的分布见下表） |
| 其中 body 是变量、ident 不可机读 | **136** ⇒ 需逐点读函数才能分类（估 0.5–1 天） |

**可机读部分按 ident 分布（前 10）**：`SM_SYSMESSAGE 17`、`SM_ATTACKMODE 10`、
`SM_CHARSTATUSCHANGED 9`、`SM_HEALTHSPELLCHANGED 9(+2) 11`、`SM_ABILITY 5`、
`SM_GUILD_CHANGENAME 3`、`SM_MENU_OK 2`、`SM_ADDITEM 2`、`SM_BAGITEMS 2`、`SM_FEATURECHANGED 2`。

**初判分类（按 oracle 侧是不是队列消息）**
- **必须入队才可能对序**（oracle 走 `MsgQueue`，会与 67/68/150/155/252 共存）：
  `SM_SYSMESSAGE(100→252)`、`SM_CHARSTATUSCHANGED(657→155)`、`SM_HEALTHSPELLCHANGED(53→68)`、
  `SM_ABILITY(52→67)`、`SM_SUBABILITY(752→62)`、`SM_LEVELUP(45→150)`、`SM_WINEXP(44→139)`、
  `SM_WEIGHTCHANGED(622→131)`、`SM_GOLDCHANGED(653→152)`、`SM_FEATURECHANGED(41→132)`、
  `SM_MENU_OK(767→69)`。**这组就是本卡的改造目标**，上表里已可机读的约 **60+** 个点。
- **不需要**（oracle 就是 `SendDefMessage` 直发，天然无竞序）：`SM_EAT_OK/FAIL(635/636)`、
  `SM_ADDITEM(200)`、`SM_UPDATEITEM(203)`、`SM_DROPITEM_*(?)`、`SM_ITEMHIDE(611)`、
  `SM_ALIVE(27)/SM_DISAPPEAR(30)/SM_DEATH(32)`、`SM_MOUNTRIDE` 等——**逐条需按 oracle 侧再核，不自作主张**。
- **待定**：`SM_ATTACKMODE`、`SM_GUILD_CHANGENAME`、`SM_BAGITEMS` 等 136 个"body 是变量"的点。

**用例影响预估（粗口径）**：`lib.rs` 测试面里形如 `vec![<数字>, …]` 的"精确帧序"断言约 **81 处**
（`assert_eq!(idents, vec![…])` 一类；**上限**，其中含非帧 ident 的用法）。改造真正开始时按组重跑计数。

## 3c. 裁定后范围（2026-11-20，`opc-19`）

**入卡范围（先开工）**：
1. 已机读的 **29 个队列类直发点**（252×9、155×9、69×2、67×2、152×2、139×2、68×1、62×1、131×1）；
2. **T-290（战斗升级 `drain_levelup_healthspell_with_magic`）**——玩家高频可达的真缺口，同族一起改。

**不在本卡、不阻塞**：136 处"body 是变量"的待定点由 `a2` 做逐函数读（产出"每个点发什么帧 + 是否队列类"清单），
本卡按清单**增量**接入。

**硬规矩**：
- **不得**因为"看着像直发"就把 635/636/200/203/611/27/30/32 归类——**逐条仍要按 oracle 侧核**；
- 改造前先复用**既有入队原语**：`game_svr::msgqueue::NetPriorityQueue`（`world-bevy` 里同一份四叉堆实现，
  `send_update_msg` 去重模型同源；`castle.rs` 已有两处按它自建"每收件人一条队列"的先例），
  **不要新造第二个发送原语**；
- 81 处精确帧序断言是**上限**；实际会动多少，按改造点逐批报数。

## 3b. 范围（逐条清点后回填）

1. **现状清点**：`route_to_gate(...)` / `tx.send(SimToGate::SendFrame ...)` 直发点清单（按模块分组）；
2. **改造面**：哪些帧要改为入队（657 / 210/212 / 45/52/752 / 其它直发 SM）；哪些**不能**入队
   （`MoveFail` 这类"当帧必须到"的？——需按 oracle 逐条核，不自作主张）；
3. **用例影响**：预计多少既有"精确帧序"断言要重写（先出数字，再动手）；
4. **判据**：同一场景下"入队版"与"直发版"的帧序差异，必须逐条能对上 oracle 的 `(byte)ident` 序。

## 4. 不做

- 不在本卡之前再新增"按阶段/按结束点"的近似（T-288–T-294 维持登记，不各自发明新序）；
- 不改 oracle 侧语义、不改客户端冻结契约。

## 3d. T-290 的 scope 与两侧残差（登记 T-295 / T-296）

**T-290 只证这一件事**：升级链**内部**的帧序（`53 → 44 → 45/52/752 → 657`）与"657 在链末"。
**不得**读成"657 全局已对齐"——链前/链后各有残差，见下（登记，不修、不挪出队点）。

方向记法：**A** = 该路径的帧在 657 **之前**发出 ⇒ 657 被排到它**后面**（应当在其前）；
**B** = 该路径的帧在 657 **之后**发出 ⇒ 657 被排到它**前面**（应当在其后）。

### T-295 链前 >155（方向 A：657 被排到它们之后）

| drain | 帧 → RM 低字节 | 方向 |
|---|---|---|
| `drain_line_notice` | SM_SYSMESSAGE → 11004 = **252** | A |
| `drain_player_run_rates` | SM_SYSMESSAGE **252**、SM_MYSTATUS → 8102 = **166**（**边界敏感**） | A |
| `drain_run_state_expiries` | SM_SYSMESSAGE **252** | A |
| `drain_death_use_item_drops` | SM_SYSMESSAGE **252** | A |
| `drain_skeleton_downlink` | SM_SYSMESSAGE **252** | A |
| `drain_buff_downlink` | SM_SYSMESSAGE **252** | A |
| `drain_b4_with_enter_cfg` | SM_SYSMESSAGE **252** | A |
| `drain_b5_self_deliveries` | SM_SYSMESSAGE **252** | A |
| `drain_pk_flag_downlink` | SM_CLOSEHEALTH → 10411 = **171**、SM_OPENHEALTH → 10410 = **170**（**边界敏感**） | A |

**边界敏感**：166/170/171 与 155 只差 11–16；一旦有人调整 `RM_*` ident 表，这三组要重新核。

### T-296 链后 <155（方向 B：657 被排到它们之前）

| drain | 帧 → RM 低字节 | 方向 |
|---|---|---|
| `drain_npc_merchant_turns` | SM_DISAPPEAR → 8061 = **125** | B |
| `drain_npc_merchant_hits` | SM_DISAPPEAR **125** | B |
| `drain_aoi_downlink` | SM_HIDEEVENT → 10333 = **93** | B |
| `drain_aoi_events` | SM_HIDEEVENT **93** / SM_SHOWEVENT **94** | B |
| `drain_monster_walk_downlink` | RM_WALK = **18** | B |
| `drain_stamina` | SM_HEALTHSPELLCHANGED → 10052 = **68** | B |

（"9 处"的另外 3 处即下节"待核"里的链后三项——**未计入本表**。）

### 待核（5，未计入上面的 9 + 9）

| 点 | 缺什么信息 |
|---|---|
| `drain_player_hunger` | 帧走 healthspell **槽**（不可机读）；oracle `RefMyStatus` 的 708 → 8102 = 166 只在文档层面成立。**故从"链前 10"移出** ⇒ 链前实为 **9**。 |
| `drain_kicks` | SM_OUTOFCONNECTION 疑为 `SendDefMessage` 直发（无 RM 侧）；需按 oracle 核它是否入队。 |
| `drain_aoi_disappear` | 消失帧走槽/批量路径，ident 未在函数体内出现。 |
| `drain_merchant_disappears` | 同上（帧由下游组帧函数给）。 |
| `drain_login_tail` | 登录尾帧走槽。 |

## 3e. T-290 编译后要改的四条用例：新期望序与依据（先写、后编译）

帧源与低字节（oracle 侧入队点）：

| 帧 | RM ident → 低字节 | oracle 入队点 |
|---|---|---|
| 53 `SM_HEALTHSPELLCHANGED` | `RM_HEALTHSPELLCHANGED` 10052 → **68** | `BaseObject.IncHealthSpell` 尾 `HealthSpellChanged()` |
| 44 `SM_WINEXP` | `RM_WINEXP` 8075 → **139** | `GetExp` 的 `SendMsg(RM_WINEXP, …)` |
| 45 `SM_LEVELUP` | `RM_LEVELUP` → **150** | `HasLevelUp` 的 `SendMsg(RM_LEVELUP)`；**45 由该消息处理器直发** |
| 52 `SM_ABILITY` | `RM_ABILITY` 10051 → **67** | 同上（`RM_LEVELUP` 处理器内直发） |
| 752 `SM_SUBABILITY` | `RM_SUBABILITY` 10302 → **62** | 同上（`RM_LEVELUP` 处理器内直发） |
| 657 `SM_CHARSTATUSCHANGED` | `RM_CHARSTATUSCHANGED` 10139 → **155** | `RecalcAbilitys()` 内 `SendUpdateMsg` |

⇒ 链内序：**68 → 139 →（150 出队后其处理器直发 67、62）→ 155**，即 `53 → 44 → 45 → 52 → 752 → 657`。

| 用例 | 旧期望 | **新期望** | 依据 |
|---|---|---|---|
| `levelup_downlink_sends_levelup_frames_heals_and_syncs_entity` | `[53,44,45,52,752]` | `[53,44,45,52,752,657]` | 657(155) 在 53/44/45/52/752 之后 |
| `p31_levelup_reuses_kill_exp_chain_order` | `[53,44,45,52,752]` | `[53,44,45,52,752,657]` | 同上 |
| `c5_recovery_then_levelup_emit_two_frames` | `[53,53,44,45,52,752]` | `[53,53,44,45,52,752,657]` | 首条 53 来自 `drain_recovery`（链前，68<155）、次条来自升级段；657 仍在链末 |
| `tick_frame_order_puts_stamina_after_kill_exp_chain` | `[…,752,53(stamina)]` | `[…,752,657,53(stamina)]` | **这条编码的是偏差 T-296（方向 B）**：`drain_stamina` 在链后、其 53(68) 应排在 657(155) 之前，而本宿主排在其后；断言处必须点名 T-296 |

**"旧期望为什么错"**：旧期望只覆盖了"链内被建模的帧"，**657 当时根本没发**（被 `pickwire` 丢弃）——所以不是"旧序对、新序错"，而是"旧期望本身缺一条帧"。这一点必须写进每条更新后的注释（第三次改既有帧序期望，按 opc-19 口径）。

## 7. T-290 交付的牙记录与门禁名口径（2026-11-20）

**牙（均在交付 head 重放，还原后 sha MATCH、基线复绿）**

- **v1（删 T-290 的入队）** ⇒ 专用用例红，原文：
  `thread 'tests::t290_levelup_chain_emits_657_at_chain_end_self_and_observers' panicked at ...: 必须发 657`
  / `test result: FAILED. 0 passed; 1 failed`；还原后 `test result: ok. 1 passed; 0 failed`。
- **v2（把出队挪到 45/52/752 之前）** ⇒ 红，原文：
  `657 必须排在 45/52/752 之后（帧序 [53, 44, 657, 45, 52, 752]）：45 的下标 3 应 < 657 的下标 2`。

**门禁名口径（两个 manifest 门别混写）**
- `tools/hygiene/scenario_manifest_gate.py`：本地 collab refs 未 fetch 全时的 **RED 5 属 §23 假红**
  （fetch 后 refs 16045 ⇒ GREEN，RED 0 / BAD 0）——**不是 master 既有红**；
- `validate-parity-manifests.ps1`（四域清单门）：**本批不含**，由 opc-19 报 GREEN。
- **以后跑这个门用包装器** `E:\OPC\tools\scenario-gate.ps1`（先 fetch 全 collab refs 再跑，避免 §23 假红）。
