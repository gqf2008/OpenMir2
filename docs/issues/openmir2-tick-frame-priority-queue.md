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

## 8. 已批准的范围与切片（2026-11-20，opc-19）

**顺序：① → ② → ③**（③ 里的"统一出口"是面最大的一步，必须最后、单独评审）

- **片①（今晚）**：抽 `TickFrameQueue`（每收件人一条 `NetPriorityQueue`，出队即 `route_to_gate`），
  把三处"各自发明局部队列"的现状收敛：`castle.rs` 两处、T-290 升级链、吃/换装路径；
  预计动 ≤6 条既有断言。
- **片②**：其余 tick 高频路径入队（44/152/131/52/252 类）。
  **开批前置**："明确不需要入队"的 7 条（635/636/200/203/610/611/27/30/32）**逐条给
  `file:line` + "为什么是直发"的 oracle 依据**——不得"看着像直发"就归类。
- **片③**：拆成两片——**③a 边缘路径**（可早做）、**③b 统一出口**（禁止生产路径再直接
  `tx.send`；碰所有发帧点，单独评审）。

### 8b. `ref_msg_cache` 交互（硬约束 + 断言）

- **规则**：657/41/656 走 `SendRefMsg` 的 500ms 缓存，**缓存命中判定必须留在产生点**
  （`ref_msg_cache::targets` 的调用处），**不得挪进出队时**——否则收件人集合会随出队时机变化。
- **断言**：改造后同一场景下，657/41/656 的**收件人集合与改造前逐人相等**（每人收/不收 + 帧数），
  以"改造前快照"为基线机判，不靠人看。

### 8c. 验收（四条，opc-19 已认）

1. **静态计数**：生产路径里 `route_to_gate(`/`SimToGate::SendFrame {` 只允许出现在统一出口函数内；
   计数从 N 降到**白名单长度**（白名单每条附 oracle `SendDefMessage` 依据）。
2. **行为**：T-295/T-296 的 18 个残差点各一条"同 tick 两帧"用例，机器比帧序。
3. **回归**：81 处帧序断言逐条重算 + 附"旧为什么错"。
4. **双栈对拍**（R 级）：同场景 C#/Rust 逐帧比。

## 8d. 片① 实施规格（供接手者机械执行）

**新 helper（`bins/game-svr/src/lib.rs`，紧邻 `broadcast_charstatuschanged_except_self`）**

```rust
/// 卡 `openmir2-tick-frame-priority-queue` 片①：**每条收件人一条** `NetPriorityQueue` 的
/// 路径级排序装置（范例来源 = `castle.rs` 两处 + T-290 升级链）。
/// 语义：`push(socket, priority, body)` 只入队；`flush(socket, gate_idx)` 按 `(byte)ident`
/// 出队并 `route_to_gate` 逐条下发。**不做跨路径/全 tick 归并**（那属片③b）。
pub(crate) struct TickFrameQueue { /* HashMap<i32 /*socket*/, NetPriorityQueue<(u8, Vec<u8>)>> */ }
impl TickFrameQueue {
    pub fn new() -> Self;
    pub fn push(&mut self, socket: i32, priority: u8, body: Vec<u8>);
    pub fn flush(&mut self, online: &OnlineRegistry,
                 gates: &HashMap<u8, UnboundedSender<SimToGate>>, what: &str);
}
```

**四条接线点（本片范围）**
1. `castle.rs` 两处（`RM_USERNAME` 165 段）：把"自建队列 → 出队 → 逐条 send"替换为
   `TickFrameQueue::push/flush`（行为应逐字节不变 ⇒ 既有断言不动）。
2. T-290 升级链（`drain_kill_exp_chain_with_magic`）：把局部 `NetPriorityQueue` 换成
   `TickFrameQueue`（`push` 时带 body，`flush` 在链末）。
3. 吃路径 `handle_eat_cmd` 的 53/657：现状是"通知泵顺序立即发"，改为入队后统一 flush
   （**注意**：吃路径的多帧跨 socket，需按 socket 分桶）。
4. 换装 `handle_bag_cmd_with_magic` 的 52/62/155：同上。

**预计断言的变更（≤6 条）**：`tick_frame_order_puts_stamina_after_kill_exp_chain`、
`p31_…`、`c5_…`、`levelup_downlink_…`（若 flush 点不变则**不动**）、吃/换装各一条。
**判据**：改造前后**逐帧字节**相同（除已登记的 T-288/T-295/T-296 外不得有新差异）。

## 9. 判据修正与"不需要入队"复核结果（2026-11-20，`a2` 独立复核）

### 9a. 判据：产出点优先，低字节为辅

**队列类不能只按"已知低字节集合"判**——例如 `169/125/37/146/149` 不在最初那 11 个里，
但它们的产出点在 `PlayObject.Message.cs` 的 **`case Messages.RM_*`** 里 ⇒ **就是经 `MsgQueue`**。

⇒ 分类判据改为：
1. **主判据**：该帧的**产出点**是否在 `case Messages.RM_*`（或任何 `SendMsg`/`SendUpdateMsg`/`SendRefMsg` 入队点）；
2. **辅助**：`(byte)RM_*` 低字节（用于定序，不用于分类）。

### 9b. 我先前判"不需要入队"的 7 类：复核结果

| 类 | 我的判断 | 复核 | 依据 |
|---|---|---|---|
| 635/636 `SM_EAT_OK/FAIL` | 不需要 | **对** | `ClientUseItems` 命令体内 `SendDefMessage` 直发 |
| 200 `SM_ADDITEM`、203 `SM_UPDATEITEM` | 不需要 | **条件成立** | 帧本身 `SendSocket` 直发，但**调用点有落在 RM 处理器里的**（如 `PlayObject.Message.cs:416`）⇒ **时序继承调用方**，不能无条件归"无需入队" |
| 610/611 `SM_ITEMSHOW/HIDE` | 不需要 | **错** | 产自 `case Messages.RM_*`（`PlayObject.Message.cs:1745/1748` 一带）⇒ 经队列 |
| 27 `SM_ALIVE` | 不需要 | **错** | `case Messages.RM_ALIVE`（`:1885`）⇒ 经队列 |
| 30 `SM_DISAPPEAR` | 不需要 | **错** | `case Messages.RM_DISAPPEAR`（`:1652`）⇒ 经队列（低字节 **125**） |
| 32 `SM_DEATH` | 不需要 | **错** | `case Messages.RM_DEATH`（`:1644`）⇒ 经队列（低字节 **37**） |

对应低字节（`a2` 实测）：610→**146**、611→**149**、27→**169**、30→**125**、32→**37**。

⇒ **片②的范围要按本表扩张**：610/611/27/30/32 五类从"不需要"移入"必须入队"。

### 9c. 清点口径统一

"待定点"的数目此前有三种口径（TSV 行 **175** / 源码站点 **124** / 卡面 **136**，互不一致）。
**以后统一按"源码站点行"计数**，并在报告里注明用的是哪一种口径。

## 10. `#7`（施法支 A）"条件发 657"的读穿结论（2026-11-20）

**产生链**：`BuffApplyEvent.char_status` ← `world.get_char_status(actor_id)`（`crates/world-bevy/src/world.rs`
的 `defence_up` / `mag_defence_up` / `magic_shield_on`）→ 宿主 `handle_spell_cmd_with_magic` 支 A
（`smframe_ext::sm_charstatuschanged(ev.actor_id, ev.char_status, 0)` 经 `SpellFrameBuf` 广播）。**是同一个量**
（`GetBuffStatus() | (CharStatusEx & 0xFFFF)`），不是局部判断。

**"条件发"对应 oracle 的哪一段**——两类 buff 不同：
- `DefenceUp`（`BaseObject.cs:2159-2162`）/ `MagDefenceUp`（`:2183-2186`）：**无条件**
  `SysMsg` + `RecalcAbilitys()` + `SendMsg(RM_ABILITY)` ⇒ 两侧都无条件发；
- `MagBubbleDefenceUp`（`src/M2Server/Player/CharacterObject.cs:96-114`）：
  `CharStatus = GetCharStatus(); if (nOldStatus != CharStatus) StatusChanged();` ⇒ **oracle 自己就是条件发**。
  本仓 `magic_shield_on` 的 `if new_status != old_status { push(ev) }` 与之逐字对应。

⇒ **结论：`#7` 的"条件发"不是语义差异，是 oracle `MagBubbleDefenceUp` 的原样**；
**不改为无条件**。（若 `SKILL_UENHANCER`(36) 实测缺 657，缺的不在 `#7` 这条链上——它走的是另一条路径。）

## 10b. 四种 657 机制的收敛 seam（记录，不实现）

| 站点 | 现机制 | 若收敛需要的上下文 |
|---|---|---|
| `#3` `CM_ADJUST_BONUS` | 直发 `tx.send` | `socket`/`gate_idx`（现成）、`status`、`hit_speed` |
| `#4` 穿脱装备 | 直发 `tx.send` | 同上 |
| `#6` 吃路径 | `pickwire::notice_frame`（队列记录 → 泵） | 已具备（`WireSession`） |
| `#7` 施法支 A | `SpellFrameBuf::push_broadcast_cached` | 已具备（buf + ref_msg_cache） |

统一 seam 的签名候选：`emit_charstatuschanged(world, online, gates, sink: &mut dyn FrameSink, actor, status, hit_speed, range)`
——`FrameSink` 由三条路径各自实现（直发 / 队列 / 帧缓冲）。**本单不实现**，属收敛单。

## 11. 15/16 处矩阵在 master `165c6edd1` 上的重算（2026-11-20）

方法：对 `git show 165c6edd1:mir2-rs/bins/game-svr/src/lib.rs` 取生产面（首个 `^mod tests {` 之前）
的全部 `recalc_session_ability(` 调用，按**顶层函数括号**归属，在函数体内数
`sm_charstatuschanged(` / `broadcast_charstatuschanged_except_self(` / `push_broadcast_cached(` / `notice_frame`。
**本表 16 行**（同一函数可有多个调用点，不可按函数名合并）。

| 行号（master） | 函数 | 自 | 广 | 证据/备注 |
|---|---|---|---|---|
| 2967 | `apply_pending_level_up` | Y | Y | sm_csc=1、bcast=1（批A #1） |
| 3202 | `apply_gm_use_item_write` | Y | Y | sm_csc=1、bcast=1（批A #2） |
| 4173 | `handle_inbound_cmd`（CM_ADJUST_BONUS） | Y | Y | sm_csc=1、bcast=1 |
| 5327 | `handle_bag_cmd_with_magic` | Y | Y | sm_csc=1、bcast=1 |
| **7818** | `handle_shop_cmd_inner`（升级武器） | **n** | **n** | 局部窗口无任何 657 ⇒ **真缺口（#5 / T-289）** |
| 10980 / 11049 | `handle_eat_cmd` | Y | Y | 自走 `notice_frame`（泵）、广 bcast=1 |
| 14429 | `handle_spell_cmd_with_magic` 支 A | Y | Y | `sm_charstatuschanged` 14403 + `push_broadcast_cached` 14413/14419 |
| **16553** | `handle_spell_cmd_with_magic` 支 B | **n** | **n** | ±80 行内无 657 ⇒ **真缺口（#8）** |
| 18774 | `drain_buff_downlink` | Y | **待核** | `sm_charstatuschanged(*actor_id,*status,0)` 18794；广播未在窗口内 |
| 19414 | `drain_levelup_healthspell_with_magic` | Y | Y | T-290：链内入队(155)、链末出队并自+广（RM_CSC=1） |
| **22398 / 22405** | `apply_pending_gm_target_ops` | **n** | **n** | **真缺口（#11/#12 / T-291）** |
| **25072** | `drain_player_need_recheck_with_magic` | **n** | **n** | **真缺口（#13 / T-292）** |
| **25359** | `drain_player_hunger` | **n** | **n** | **真缺口（#14 / T-293）** |
| **25947** | `drain_player_revival_with_magic` | **n** | **n** | **真缺口（#15 / T-294）** |

⇒ **只有"两条都没有"的点进接线批次**：`7818`(#5)、`16553`(#8)、`22398/22405`(#11/#12)、
`25072`(#13)、`25359`(#14)、`25947`(#15) 共 **7 个调用点**；`18774` 的"广"待核后再定。

> 审计 §3.2 的原句"11 处都没有"**作废**：当前 master 上已有多处接线（批A #1/#2 与 T-290 已合入）。

## 11b. `FrameSink` 三条实现：已有/缺什么（决定下一单能否收敛）

| 机制 | 代表站点 | 已有 | 缺 |
|---|---|---|---|
| 直发 `tx.send` | `#3` CM_ADJUST_BONUS、`#4` 穿脱 | `socket`/`gate_idx`（`online` 里现成）、`status`、`hit_speed` | 无（可包 `FrameSink` 的"立即发"实现） |
| `pickwire::notice_frame` | `#6` 吃路径 | `WireSession`（含 `hit_speed`）、`QueuedNotice` 队列、泵位置 | `status` 不在 `WireSession` 里（657 用 `session.hit_speed` + 记录 `n_param1`）⇒ 若收敛需把 `status` 并入队列记录 |
| `SpellFrameBuf::push_broadcast_cached` | `#7` 施法 | buf + 每收件人 `NetPriorityQueue` + `ref_msg_cache` 收件人 | **泵序位置**（必须在本支所有帧入队后统一 flush）⇒ `FrameSink` 需暴露 `flush` 时机 |

## 12. 7 个"两条都没有"点的 oracle 判定（只认 oracle 代码行，2026-11-20）

**共同的 oracle 事实（先立）**：玩家侧 `PlayObject.RecalcAbilitys()` 在 `if (Race == ActorRace.Play)`
块内**无条件**执行 `CharStatus = GetCharStatus(); StatusChanged(); SendUpdateMsg(RM_CHARSTATUSCHANGED, HitSpeed, CharStatus, 0,0,"")`
（`src/M2Server/Player/PlayObject.cs:2848` 起、`:2874-2876`）。
⇒ 任何"调用 `RecalcAbilitys()`"的玩家路径，**每次都会**产出：`StatusChanged()`（`BaseObject.cs:1515-1517`
的 `SendRefMsg`，广播含自己）**+** `SendUpdateMsg`（仅自己）。`da` 在 36 技能上实测到 **caster 2 条**
（广播 1 + 自更新 1）、target/obs 各 1 条（`golden/magic/uenhancer.json` 三 seed 一致）——
**这就是"每次 recalc = 广播 1 + 自更新 1"的实证**。

| # | Rust 点 | oracle 锚 | 该函数发 657？ | 判 |
|---|---|---|---|---|
| #5 | `handle_shop_cmd_inner`（升级武器） | `src/GameSrv/Npc/Merchant.cs:569-571`：`UseItems[Weapon].Index = 0; user.RecalcAbilitys(); user.FeatureChanged(); SendMsg(RM_ABILITY)` | **无条件** | **真缺口** |
| #8 | spell 支 B `AttPowerUp` | `PlayObject.cs:5323-5331`：`RecalcAbilitys(); SendMsg(RM_ABILITY)` | **无条件** | **真缺口**（`da` 帧表 4 条实证） |
| #11/#12 | `apply_pending_gm_target_ops`（`@删除技能`） | `src/Modules/GameCommand/Commands/DelSkillCommand.cs:71`：`mIPlayerActor.RecalcAbilitys();` | **无条件** | **真缺口** |
| #13 | `drain_player_need_recheck_with_magic` | `src/M2Server/Player/PlayObject.Message.cs:430-433`：`if (boTakeItem) { SendDelItems(...); UseItems[i].Index = 0; RecalcAbilitys(); }` | **有条件**（`boTakeItem` = 该格被摘下并回包/掉落成功） | **真缺口**（Rust 的 `bo_take_item` 已是同一条件） |
| #14 | `drain_player_hunger` | `src/M2Server/Player/PlayObject.Base.cs:288-292`：`RefMyStatus() { RecalcAbilitys(); SendMsg(RM_MYSTATUS); }` | **无条件**（跨档时 oracle 调 `RefMyStatus`） | **真缺口** |
| #15 | `drain_player_revival_with_magic` | `PlayObject.cs:5371-5378`：耐久归零 ⇒ `SendDelItems`、`Index = 0`、`RecalcAbilitys()` | **无条件**（进入该支即调） | **真缺口** |
| #9 | `drain_buff_downlink`（`buff_recalcs` 支） | `src/M2Server/ActorBuffSystem.cs:220-231`：`if (boChg) { CharStatus=...; StatusChanged(); }` 与 `if (boNeedRecalc) { RecalcAbilitys(); SendMsg(RM_ABILITY); }` **是两段** | `boChg` 支发（Rust 已接）；`boNeedRecalc` 支经 `RecalcAbilitys()` **也发** | **真缺口**（Rust 的 recalcs 支只有 52，没有 657） |

⇒ **7 个点全部为真缺口**（#9 是"半个已接"：#buff_status_changes 支已有，recalcs 支没有）。

### 12b. 发射模式：**按 oracle 产出点，不能外推**（2026-11-20，`da` 实测修正）

**先前假设作废**：我一度以为"每次 recalc = 广播 1 + 自更新 1"是通用模板——`da` 扫全部
`golden/magic/*.json` 后证明**不同技能的多重集不同，由 oracle 产出路径决定**：

| golden | caster | target | obs_in | 形态 |
|---|---|---|---|---|
| `uenhancer`(36) | **2** | 1 | 1 | `RecalcAbilitys` 无条件 `:2875 StatusChanged()` + `:2876 SendUpdateMsg`（广播1含self + 自更新1） |
| `bigcloak`(19) | 2 | 2 | 2 | **两次广播**（每收件人 2），无单独自更新 |
| `cloak`(18) / `shield`(31) | 1 | 1 | 1 | **单广播**（shield = `MagBubbleDefenceUp` 的 `if (nOldStatus != CharStatus) StatusChanged()`） |
| `dejiwonho`(15) | 1 | **2** | 1 | target 多一条（路径特定） |
| `hangmajinbub`(14) | 49 | 98 | 49 | 逐格 ×2 |

⇒ **"已接 5 处只发 1 条自己 + 广播一次"不构成缺口证据**：若该路径 oracle 只有 `StatusChanged()`
（无 `SendUpdateMsg`），1/1/1 就是对的（`shield` 即此）。**复核必须逐路径对 oracle 产出点**，
不得拿 36 的 2/1/1 外推。

**模板契约（草案，仍待逐点锚定后才可作为实现依据）**：
> 每条 657 = **一次 oracle 产出调用**（`StatusChanged()` 或 `SendUpdateMsg(...)`）对应一条帧；
> 收件人集合由该调用决定（`SendRefMsg` = 半径内含自己；`SendUpdateMsg` = 仅自己）；
> 帧序 = `(byte)RM_CHARSTATUSCHANGED = 155`，同键按**入队序**（源行序：广播先、自更新后）。
> **反例**：`MagBubbleDefenceUp` 条件不满足时**一条都没有**（条件 StatusChanged）。

### 12c. golden 全量 657 扫描（2026-11-20，只读）

`tests/parity/golden/**` 下含 657 的 JSON 共 **10 个文件**（含 `magic/legacy/*` 副本），
每 case 出现次数：`bigcloak` 6、`cloak` 3、`dejiwonho` 4、`hangmajinbub` **196**、`shield` 3
（均 ×3 cases，三 seed 一致）；`uenhancer` 的 golden 尚未入 `golden/`（`da` 手扫给出 2/1/1）。
这些文件被 `docs/issues/parity-gap-magic.json` 的条目引用；比较器在 `tests/parity/tests/magic_fireball_parity.rs`。
