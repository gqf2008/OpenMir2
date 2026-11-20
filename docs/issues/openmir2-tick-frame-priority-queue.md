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

> **坐标系 = master `165c6edd1`**（行号以该树为准）。

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

> **坐标系 = master `165c6edd1`**。

| 机制 | 代表站点 | 已有 | 缺 |
|---|---|---|---|
| 直发 `tx.send` | `#3` CM_ADJUST_BONUS、`#4` 穿脱 | `socket`/`gate_idx`（`online` 里现成）、`status`、`hit_speed` | 无（可包 `FrameSink` 的"立即发"实现） |
| `pickwire::notice_frame` | `#6` 吃路径 | `WireSession`（含 `hit_speed`）、`QueuedNotice` 队列、泵位置 | `status` 不在 `WireSession` 里（657 用 `session.hit_speed` + 记录 `n_param1`）⇒ 若收敛需把 `status` 并入队列记录 |
| `SpellFrameBuf::push_broadcast_cached` | `#7` 施法 | buf + 每收件人 `NetPriorityQueue` + `ref_msg_cache` 收件人 | **泵序位置**（必须在本支所有帧入队后统一 flush）⇒ `FrameSink` 需暴露 `flush` 时机 |

## 12. 7 个"两条都没有"点的 oracle 判定（只认 oracle 代码行，2026-11-20）

> **坐标系 = master `165c6edd1`**；oracle 行号与分支无关。

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

> **坐标系 = master `165c6edd1`**。

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

> **坐标系 = master `165c6edd1`**。

`tests/parity/golden/**` 下含 657 的 JSON 共 **10 个文件**（含 `magic/legacy/*` 副本），
每 case 出现次数：`bigcloak` 6、`cloak` 3、`dejiwonho` 4、`hangmajinbub` **196**、`shield` 3
（均 ×3 cases，三 seed 一致）；`uenhancer` 的 golden 尚未入 `golden/`（`da` 手扫给出 2/1/1）。
这些文件被 `docs/issues/parity-gap-magic.json` 的条目引用；比较器在 `tests/parity/tests/magic_fireball_parity.rs`。

## 13. 交接点（下一单）

**下一单 = 12 个站点逐个给出"oracle 调用序列 → 期望 657 条数"的表**：

- **7 个缺口**：`#5 handle_shop_cmd_inner`、`#8`（**已转 `da`**，36 臂，不重复）、`#11/#12 apply_pending_gm_target_ops`、
  `#13 drain_player_need_recheck_with_magic`、`#14 drain_player_hunger`、`#15 drain_player_revival_with_magic`；
- **5 个已接点复核**：`#1 apply_pending_level_up`、`#2 apply_gm_use_item_write`、`#3 CM_ADJUST_BONUS`、
  `#4 穿脱装备`、`#7 施法支 A`（含 `MagBubbleDefenceUp` 的条件发反例）；
- **外加 `#9 drain_buff_downlink` 的"半接"**：`src/M2Server/ActorBuffSystem.cs:220-231` 两段——
  `boChg` 的 `StatusChanged()` 已接、`boNeedRecalc` 的 `RecalcAbilitys()` 未接。**不得丢**。

**每个站点要填**：oracle 函数 + 行号 → 该路径上的 657 产出调用（`StatusChanged()` / `SendUpdateMsg(...)` / 其它）
→ 每种收件人的期望条数（caster/target/obs）→ 对应的 `golden/` 证据（有则引，无则写明"无 golden"）。

**已有坐标系**：§12（7 缺口判定）、§12b（条数按产出路径定 + 反例）、§12c（golden 657 现状）、
§11b（`FrameSink` 三条实现缺什么）、§10b（seam 候选签名）；全部以 `165c6edd1` 为准。

**不要重复的活**：36（`uenhancer`）的 4 条帧已由 `da` 实测并落地；`#7` 的"条件发"已证为 oracle 原样，别再翻。

## 14. 12 站点 oracle 调用序列 → 期望 657 条数（2026-11-20，只读；坐标系 = `165c6edd1`）

**共同事实（依据，不作外推公式）**：`PlayObject.RecalcAbilitys()` 在 `if (Race==ActorRace.Play)` 块内
`:2874 CharStatus=GetCharStatus(); :2875 StatusChanged(); :2876 SendUpdateMsg(RM_CHARSTATUSCHANGED,HitSpeed,CharStatus,0,0,"")`。
⇒ **凡是"只调一次 `RecalcAbilitys()`、无其它 657 产出点"的站点，期望 = 自 2 / 目标 1 / 观察 1**
（`uenhancer`(36) 实测 2/1/1 即此形态的直接印证）。
**不经 `RecalcAbilitys()` 的路径不适用**（见 §12b 反例：shield 1/1/1 条件发、bigcloak 2/2/2 两次广播等）。

| # | 站点（`lib.rs`@`165c6edd1`） | oracle 调用序列（行号） | 期望 657（自/目标/观察） | golden |
|---|---|---|---|---|
| 1 | `apply_pending_level_up` :2967 | `ChangeLevelCommand` → `HasLevelUp`（`PlayObject.cs:5242-5252`）= `RecalcLevelAbilitys(); RecalcAbilitys(); SendMsg(RM_LEVELUP)` | **2 / 1 / 1** | 无（GM 路径） |
| 2 | `apply_gm_use_item_write` :3202 | `src/Modules/GameCommand/Commands/SmakeItemCommand.cs:60` `RecalcAbilitys()`（+`SendUpdateItem`） | **2 / 1 / 1** | 无 |
| 3 | `handle_inbound_cmd`(CM_ADJUST_BONUS) :4173 | `ClientAdjustBonus`（`src/M2Server/Player/PlayObject.Base.cs:1173`）`RecalcAbilitys()` | **2 / 1 / 1** | 无 |
| 4 | `handle_bag_cmd_with_magic` :5327 | `ClientTakeOn/TakeOffItems`（`Operate.cs:455/544`）`RecalcAbilitys()` | **2 / 1 / 1** | 无 |
| 5 | `handle_shop_cmd_inner`（升级武器）:7818 | `src/GameSrv/Npc/Merchant.cs:569-571`：`Index=0; RecalcAbilitys(); FeatureChanged(); SendMsg(RM_ABILITY)` | **2 / 1 / 1**（另有 41、52） | 无 |
| 7 | spell 支 A（按魔法分） | ① `MagBubbleDefenceUp`（`CharacterObject.cs:96-114`）：`CharStatus=GetCharStatus(); if(nOldStatus!=CharStatus) StatusChanged();` ⇒ **条件发、无 SendUpdateMsg**；② `DefenceUp/MagDefenceUp`（`BaseObject.cs:2159-2186`）：`RecalcAbilitys(); SendMsg(RM_ABILITY)` | ① 条件成立 **1 / 1 / 1**、不成立 **0/0/0**；② **2 / 1 / 1** | `shield.json` 1/1/1；`cloak.json` 1/1/1；`dejiwonho.json` 1/**2**/1；`bigcloak.json` **2/2/2**；`hangmajinbub.json` 49/98/49 |
| 9 | `drain_buff_downlink`（半接）:18774 | `src/M2Server/ActorBuffSystem.cs:220-231` 两段：`if(boChg){ CharStatus=GetCharStatus(); StatusChanged(); }`（已接）/ `if(boNeedRecalc){ RecalcAbilitys(); SendMsg(RM_ABILITY); }`（**未接**） | `boChg` 支 **1/1/1**；`boNeedRecalc` 支 **2/1/1** ⇒ **2026-11-20 修正见 §14f：头去重模型下夹具场景两支都是 1** | 无（buff 侧仅单测） |
| 11 | `apply_pending_gm_target_ops`（@删除技能）:22398 | `src/Modules/GameCommand/Commands/DelSkillCommand.cs:71` `mIPlayerActor.RecalcAbilitys()`（+`SendDelMagic` 直接帧 212） | **2 / 1 / 1** | 无（GM） |
| 12 | 同上（第二分支）:22405 | 同上 | **2 / 1 / 1** | 无（GM） |
| 13 | `drain_player_need_recheck_with_magic` :25072 | `src/M2Server/Player/PlayObject.Message.cs:430-433`：`if(boTakeItem){ SendDelItems; UseItems[i].Index=0; RecalcAbilitys(); }` | **2 / 1 / 1**（条件：该格确实被摘下） | 无 |
| 14 | `drain_player_hunger` :25359 | `RefMyStatus()`（`src/M2Server/Player/PlayObject.Base.cs:288-292`）= `RecalcAbilitys(); SendMsg(RM_MYSTATUS)` | **2 / 1 / 1**（+708） | 无 |
| 15 | `drain_player_revival_with_magic` :25947 | `PlayObject.cs:5371-5378`：耐久归零 ⇒ `SendDelItems; Index=0; RecalcAbilitys()` | **2 / 1 / 1** | 无 |

**反例（保留在表里，防止归纳）**：`shield` 1/1/1（条件 `StatusChanged`、无 `SendUpdateMsg`）、
`cloak` 1/1/1、`bigcloak` 2/2/2（两次广播）、`dejiwonho` 1/**2**/1（目标多一条）、
`hangmajinbub` 49/98/49（逐格×2）、`uenhancer` 2/1/1（RecalcAbilitys 标准形态）。

**对已接 5 处的含义**：#1/#2/#3/#4 若现状是"广播 1（含自）+ 自更新 1" ⇒ 已符合；
若只有"1 条自己 + 广播一次" ⇒ **缺 `SendUpdateMsg` 那条**，需在接线批次里补。
`#7` 的 ① 形态（条件发、无自更新）**现状即正确**，不得按 ② 补第二条。

## 14b. 接线实施规则（Batch 1–3 的处方，2026-11-20）

**每个"经 `RecalcAbilitys()`"的站点要发 3 个动作、且顺序固定**（oracle 源行序）：

1. **广播副本（含自己）** —— 对应 `PlayObject.cs:2875 StatusChanged()` 的 `SendRefMsg`（收件人 = ±`SendRefMsgRange` 含源）；
2. 观察者（除自己外的收件人）—— 同一次广播的其它收件人；
3. **自更新副本（仅自己）** —— 对应 `:2876 SendUpdateMsg(RM_CHARSTATUSCHANGED, HitSpeed, CharStatus, 0,0,"")`。

⇒ 对**自己**：**两条**，同 prio 155，**入队序 = 广播副本先、自更新副本后**；对**观察者**：一条。

**现状差距（`#1`/`#2` 的批A 实现）**：只有"自己 1 条 + 广播他人"，且自己那条**排在广播之前** ⇒
既要**补一条自更新**，也要**把顺序改回"广播副本在前"**。`#3`/`#4` 同形。

**测试断言写法**（两条 657 字节完全相同，不能用多重集）：
> **2026-11-20 修正（§14c）**：本节末条的"断言完整 `ids` 序列 / 两次出现之间无插入"**作废**——
> 跨 socket 先后不是线上可观测面，见 §14c。以 §14c 为准。
- 取**自己的帧序列**，断言其中 657 的**下标位置**与**条数**：`pos(657) 出现两次`，且第一条来自广播、
  第二条来自自更新 —— 由于字节相同，判据只能靠**顺序 + 条数**（例如断言 `ids.iter().filter(==657).count()==2`
  再断言两次出现之间**没有**更低优先级的帧插入，或直接断言完整 `ids` 序列）；
- 观察者侧断言 `count == 1`；范围外观察者 `count == 0`；
- **先写测试、在未接线代码上跑红（现状 1 条自），再接**——§26「只接不发测试」的防线。

**禁区**：`#7` 的 `MagBubbleDefenceUp` 形态（条件 `StatusChanged`、**无**自更新）原样不动；
任何人想"统一成 2/1/1"必须先推翻 `src/M2Server/Player/CharacterObject.cs:96-114` 的原文。

**口径提醒**：判定"有没有 657"按**行为**，不按"是否调用 helper"——
（`drain_monster_ref_msgs` 是内联 fan-out，逐收件人重编码），`a2` 已确认它是等价实现，不是缺口。

## 14c. 顺序面裁定：只验计数，不写跨 socket 序（2026-11-20，`opc-19` + 执行者 `openmir2-89`）

**事实（只读核对，`src/M2Server/Actor/BaseObject.Message.cs`）**：
- `:383 SendRefMsg`；扫描支 `:395-447` 按 x-major 逐格（`nCx` 从 `CurrX-12` 到 `CurrX+12`，`nCy` 同理）
  枚举 `ObjList` —— **self 位于 `(CurrX,CurrY)` 那一格，先后由坐标决定，既不在最前也不在最后**；
- 500ms 复用支 `:453-474` 按 `VisibleHumanList` 原序重放；隐身支 `:390-392` 只发自己。

⇒ **广播副本与观察者的相对先后是扫描序的函数**，不同交局会给出不同结果；
且线上**每 socket 各有自己的 `MsgQueue`**，跨 socket 先后根本不是线上可观测面（测试装置把多个
socket 汇到一条 mpsc 才产生顺序）。两条自帧字节完全相同 ⇒ "哪条是广播副本"也不可判。

**断言口径（本批及后续 `#3`/`#4` 一律）**：
1. **主断言 = 逐 socket 计数**：自己恰 2 条 657（`PlayObject.cs:2875` 广播副本 + `:2876` 自更新副本）、
   每个观察者恰 1、范围外 0。现状 1 条 ⇒ 改前必红，红的**原因必须是计数**；
2. **顺序只允许同一 socket 流内、且有参照帧**的相对位置（例：T-290 的 self 流 `……45/52/752 → 657 → 657`：
   657 在 45/52/752 之后、末尾连续两条）；
3. **禁止**：跨 socket 序、完整全局 `ids` 序列、"两条 self 谁先"、以 helper 的循环写法为判据的断言。
4. 夹具放**两侧观察者**（x-5 / x+5）⇒ 任何隐性依赖扫描序的实现会在计数/位置断言上露馅。

**实现侧结论**：`send_charstatuschanged_self_and_observers` 的三动作（自广播副本 → 广播他人 →
自自更新副本）作为实现保留；self 副本统一提到广播循环之前是对扫描序的**已登记近似**，
差异在线上不可观测（见上）。

## 14d. 批 1 合并后全量红 5 条：逐条期望更新（2026-11-20，`openmir2-89`）

**发现方式**：`e1` 在 rebase 后跑 **全量** `cargo test -p game-svr --lib`（`running 2220` →
`2142 passed; 5 failed; 73 ignored`）。此前我方只跑过滤集（`657`/`level0`/`smake`/`script_bridge_drain`），
这些用例名不含 "657" ⇒ **过滤跑绿 ≠ 套件绿**。

**共同依据**：oracle `PlayObject.cs:2874 CharStatus=GetCharStatus(); :2875 StatusChanged();`
`:2876 SendUpdateMsg(RM_CHARSTATUSCHANGED, HitSpeed, CharStatus, 0,0,"")`；
`:2875` 的 `SendRefMsg`（`src/M2Server/Actor/BaseObject.Message.cs:383`，扫描支 `:395-447`）收件人**含源**；
`:2876` 再给源一条 ⇒ 经 `RecalcAbilitys()` 的路径在**自己 socket** 上恰两条 657。

| # | 用例 | 改前 | 改后 | 依据 |
|---|---|---|---|---|
| 1 | `levelup_downlink_sends_levelup_frames_heals_and_syncs_entity` | `[…,45,52,752,657]` | `[…,45,52,752,657,657]` | `:2875` 广播副本 + `:2876` 自更新；657(155) 排在 45/52/752 之后同前（`HasLevelUp` `:5242-5250` 的 RM_LEVELUP=150 先出队） |
| 2 | `c5_recovery_then_levelup_emit_two_frames` | `[53,53,44,45,52,752,657]` | `[…,752,657,657]` | 同上；两条 53 与 T-296 口径不变 |
| 3 | `p31_levelup_reuses_kill_exp_chain_order` | `[53,44,45,52,752,657]` | `[…,752,657,657]` | 同上（P31 复用击杀链，不另造帧序） |
| 4 | `tick_frame_order_puts_stamina_after_kill_exp_chain` | `[53,44,45,52,752,657,53]` | `[…,752,657,657,53]` | 同上；657 与 stamina 53 之间的**偏差 T-296（方向 B）**不变 |
| 5 | `spell_p3b5_36_uenhancer_frames_amulet_and_gates` | F36-A caster `[53,638,642]`、target `[17,52,638,100]`；自施法 `[52,53,638,642,100]` | caster `[53,638,642,657]`（**只 1 条**＝target `StatusChanged` 的广播副本，收件人含近邻 caster）；target `[17,52,638,657,657,100]`；自施法 `[52,53,638,642,657,657,100]` | `:5328 RecalcAbilitys()` 作用于**目标**：广播副本给 ±range（含 caster 与 target 自己）、自更新只给 target ⇒ target 2 / 旁观 caster 1；自施法两者同体 ⇒ 911 上 2 |

**第 5 条的归属（重要，不是批 1 引入）**：657 发射块来自 `da` 的 36 卡接线
`47064136e feat(uenhancer): 36 臂补 657（StatusChanged 广播3 + SendUpdateMsg 自更新1，复用 SpellFrameBuf）`
（13:49 提交、经 `ef47f6a00` 合入 master，**未同步更新本条用例**）。
**复现**：在批 1 合并前的树 `1d07d2010` 上把 `lib.rs` 单独取回跑同一条用例 ⇒
`panicked at lib.rs:82165: assertion left == right failed: caster 出队序 … left: [53, 638, 642, 657] right: [53, 638, 642]`
⇒ **批 1 合并前它就是红的**；批 1 只是让它在全量跑里被看见。（`e1` 的"657 发射点 8→9"确实是批 1 的一处净增，
但与本条红无因果。）36 的接线形态（函数内两条 push）与批 1 的三动作 helper 是两套实现；本批只更新期望，不改 36 实现。

**判据边界**：以上全部只到**计数与同 socket 参照帧位置**（§14c/§47）；
两条 657 的相互先后、跨 socket 先后一律不判。

## 14e. `#13` `drain_player_need_recheck_with_magic`（P21）接线（2026-11-20）

**oracle**：`PlayObject.Message.cs:430-433` 的 `if (boTakeItem) { … UseItems[i].Index=0; RecalcAbilitys(); }`
⇒ `RecalcAbilitys`（`PlayObject.cs:2874-2876`）产 自 2（`:2875` 广播副本 + `:2876` 自更新）+ 观察者各 1；
**条件不成立 ⇒ 0 条 657，且不得有 delitem / `Index=0` / recalc**。

**发送位置（顺序）**：P21 已有 `immediate`（201/202/212，直发）与 `WeightChanged()` 的 **622**。
oracle 队列序 **622(131) < 657(155)** ⇒ 657 **不得**放进 `immediate`，必须发在 622 **之后**。
实现：循环内 `boTakeItem` 支只**收集** `(status, hit_speed)`，`immediate` 与 622 发完后按序调用
`send_charstatuschanged_self_and_observers`（观察者副本逐收件人重编码，`Series` = 收件人自己 `HitSpeed`）。
多项移出时每次 recalc 各产一对（与 oracle 逐项 `RecalcAbilitys()` 同形）。

**用例** `p21_take_item_emits_two_self_657_after_weightchanged`（本批新增）：
- 条件真：自己流 `200 → 202 → 622 → 657 → 657`；近邻 902 恰 1、±12 外 905 零；`Index=0`。
- 条件假（`Need` 达标）：**零帧（无 201/202/622/657）**、`Index` 保留、`wabil.level` 哨兵 0 未被
  `RecalcAbilitys` 从 Equip 反推（**负例是"条件接对了"的唯一证据**）。
- 牙（顺序）：把 657 移回 `immediate` ⇒ 实测 `left [200, 202, 657, 657, 622] / right [200, 202, 622, 657, 657]` 红。
  变异 diff：`E:\UserTemp\openmir2-89-mutations\b13-order-mutation.diff`；还原=反向 Edit；
  还原后 `grep MUTATION`=0、复跑绿。**（疏漏：未在变异前预存 blob hash，"还原相等"缺 sha 证据；
  用"反向 Edit 逐行还原 + 无 MUTATION 残留 + 复跑绿"替代，后续变异一律先存 hash。）**

**既有期望更新（同一语义变化，逐条依据同上）**：
| 用例 | 改前 | 改后 |
|---|---|---|
| `sc_gate_blocks_p21_need_recheck_after_soft_close`（反向牙） | `[201,202,622]` | `[201,202,622,657,657]` |
| `p21_need6_item_moves_to_bag_need0_low_level_stays_and_recalc_runs` | `[200,202,622]` | `[200,202,622,657,657]` |

**全量三数（§51）**：`cargo test -p game-svr --lib` → `running 2226` / **2153 passed · 0 failed · 73 ignored**，rc=0。
判据边界按 §14c/§47：只判逐 socket 计数与同 socket 参照帧位置；两条 657 相互先后、跨 socket 先后不判。

## 14f. 657 自见条数是**队列状态**的函数：头去重模型与逐路径裁定（2026-11-20）

**机制（只读核对，树 `walgit/master @ fd02c1b6d`）**：`SendUpdateMsg`（`src/M2Server/Actor/BaseObject.Message.cs:283-304`）
在入队前只把**队首且同 `wIdent`** 的消息弹掉，再 `SendMsg(this, …)` 重新入队。`RecalcAbilitys` 在 `:2876`
之前唯一入队是 `:2875 StatusChanged()` 的那条 657（扫过 `PlayObject.cs:2008-2876`，`:2884` 的另一 `SendRefMsg`
在之后）。⇒
- **`:2876` 时队列为空**（或只有 ≥155 的消息）⇒ 657 在队首 ⇒ 被自己弹掉再入队 ⇒ 自见净 **1**；
- **队列里有 <155 的待办**（45=150/44=139/622=131/53=68…）⇒ 不去重 ⇒ 自见 **2**。

**"自 2"不是规则，是队列状态的后果。** 6 份 magic golden 的条数是 **oracle 实测**（`mir2-rs/tests/parity/csharp/MagicGolden`
反射替换 RNG、跑真实 `PlayObject`，golden 存每收件人真实 RM 队列 dump）：
`shield` 1/1/1、`cloak` 1/1/1、`bigcloak` 2/2/2、`dejiwonho` 1/**2**/1、`hangmajinbub` 49/**98**/49、`uenhancer` **2**/1/1
⇒ 这些是**权威值**，不得改。

### 逐路径裁定（`opc-19` 2026-11-20 裁定：期望匹配**夹具场景**，不以另一条路径的可能值为准）
| 路径 | `:2876` 时队首（依据） | 裁定 |
|---|---|---|
| 升级链 4 例（`c5_recovery`/`levelup_downlink`/`p31_levelup`/`tick_frame_order`） | `RM_WINEXP(139)` 先入队——`GetExp`（`PlayObject.Base.cs:149-169`）`:153` 先 `SendMsg(RM_WINEXP)`、`:162` 才 `HasLevelUp` | **2**（不变） |
| `#9 boChg` | **无第二条**：`src/M2Server/ActorBuffSystem.cs:220-224` 只有 `CharStatus=GetCharStatus(); StatusChanged();`，**无 `SendUpdateMsg`** | **1**（master 原为 2 ⇒ 本批修正） |
| `#9 boNeedRecalc` | `src/M2Server/ActorBuffSystem.cs:226-230` `RecalcAbilitys()`；夹具场景队列空 | **1**（master 原为 2 ⇒ 本批修正） |
| `#1` GM `@Level0` | `HasLevelUp`（`PlayObject.cs:5242-5250`）在 `:2876` 前无入队；`RM_LEVELUP` 在其后 | **1**（夹具=空队列） |
| `#2` GM `@改属性` | `src/Modules/GameCommand/Commands/SmakeItemCommand.cs:59-60`：改槽 → `RecalcAbilitys()`；`:2876` 前无入队 | **1** |
| `#13` P21 `boTakeItem` | P21 在消息泵之后（`PlayObject.Message.cs:163-227`）；夹具无 P1/P2 事件 | **1** |
| `#11/#12` DelSkill（**未接线**） | `src/Modules/GameCommand/Commands/DelSkillCommand.cs:59-71`：先 `SendDelMagic`（直接帧）+ 两条 `SysMsg`（100=**252**）再 `RecalcAbilitys` ⇒ 252>155，657 仍是队首 | 接线按 **1** 设计 |
| `readbook`（已有 golden） | 队列空 ⇒ 头去重 ⇒ 1；golden `readbook_vectors.json` 实测 `pump_idents [10115,10139,11004]` = 1 条 | **1**（Rust 正确） |
| `CM_ADJUST_BONUS`（`lib.rs` 自队列） | 宿主用 `msgqueue::send_update_msg` 已实现同一头去重 | 1（与 oracle 同口径） |

### 本批（`card/charstatus-657-b13`）据此修正
- 新增原语 `send_charstatuschanged_self_once_and_observers`（自见 1 + 观察者逐收件人）；
  `send_charstatuschanged_self_and_observers`（三动作 = 2 条）**只保留给队列非空的场景**（升级链）。
- `#1`/`#2`/`#13`/`#9` 两支改为 1；对应期望回退（`level0` 五条→四条、`gm_smake` `[657,657,203,100]`→`[657,203,100]`、
  `p21` 两例与 buff 三例同步）。
- T-288 仍是"657 先于 203"的已登记偏差（条数 1，位置差位不变）。

### `#13` 多物品的判据（2026-11-20 补）
一次 Run 移出 **N 件**时：**观察者每件 1 条**（每次 `StatusChanged()` 各广播一次）；
**自见恒 1 条**，载荷 = **最后一次** recalc 的 `(CharStatus, HitSpeed)`。
为什么不是 N：`SendUpdateMsg`（`src/M2Server/Actor/BaseObject.Message.cs:283-304`）的 `TryPeek`
**不前进**，循环把**队首连续的同 ident 657 全部弹掉**再入 1（退出条件是 `MsgQueue.Count <= i`）；
队列空场景下每件 recalc 后队列恒剩 1 条 ⇒ N 件之后仍 1 条。
用例：`p21_two_items_self_657_once_observers_twice`（自见/观察者**分开断言**），
牙 = 把实现改回"每件一次自见" ⇒ **自见断言**必红。
同一机制也适用于 `#9 boNeedRecalc`：`BUFF_PASSES_PER_TICK`（=4，`mir2-rs/crates/world-bevy/src/buff.rs`）+ `world.rs` 的
`out.recalcs.extend(pass.recalcs)`（跨 pass 累积） ⇒ 同 actor 可被推多次；修法相同（广播按事件、自见在循环末每人一次）。

### 片① 之后（`977d260a1` rebase）的 2 条来源与删除记录（2026-11-20）
rebase 后"升级链 = 2 条"**由 `TickFrameQueue` 入队版产出**：`drain_kill_exp_chain_with_magic` 里
`push_charstatuschanged_self_and_observers(&mut frame_q, …)` → `frame_q.flush(online, gates, "657(战斗升级)")`，
它按 ①自广播副本/②观察者/③自更新 入**每个收件人各自的堆** ⇒ 自见 2 条（oracle 队首 `RM_WINEXP=139`
⇒ 不去重）。**即发版**（入队后立刻 flush 的薄包装）在 rebase 后**无语义使用者**：两处 GM 调用点按
头去重裁定已改用 once 版 ⇒ 删除（提交 `refactor(game-svr): 删即发版三动作 helper`）。
**删薄包装 ≠ 删掉"2"**：2 条仍由队列版产出，且升级链四例
（`c5_recovery_then_levelup_emit_two_frames` / `levelup_downlink_sends_levelup_frames_heals_and_syncs_entity` /
`p31_levelup_reuses_kill_exp_chain_order` / `tick_frame_order_puts_stamina_after_kill_exp_chain`）实测绿。

### 657 `Series` 的值断言（2026-11-20，响应 `43` 的 `recalc-entity-mirror-gap`）
条数对 ≠ 值对：本卡补两类**区分性**用例（实体 `hit_speed=99` 哨兵、会话重算后 0；
**这是构造态，不代表线上存在实体/会话不一致**，只为钉"读的是哪一侧"）。
- `#1 @Level0`（`gm_level_emits_657_self_and_nearby_observers`）：自见 `Series = 0`（重算后），
  不用实体哨兵 99 ⇒ **绿**。`apply_pending_level_up` 自见走 `sg.hit_speed`
  （`recalc_session_ability` 内 `sg.hit_speed = recalc.hit_speed`）⇒ **不受 mirror-gap 影响**。
- `#9 boNeedRecalc`（`buff_657_needrecalc_series_reads_post_recalc_value`）：同哨兵下
  **实测红**——`assertion left == right failed: 自见 Series = 重算后的 HitSpeed（0），不是实体旧值 99`
  / `left: [99]` / `right: [0]`（`lib.rs` 第 34769 行）。根因 = 该路径 `recalc_session_ability` 后**未把
  `sg` 镜像回实体**，而自见取 `world.entities…hit_speed`；归 `43` 的
  `openmir2-recalc-entity-mirror-gap`。本用例以
  `#[ignore = "recalc-entity-mirror-gap: 实体未镜像 hit_speed（recalc 后待 apply_entity 接线），自见读实体旧值；见 openmir2-recalc-entity-mirror-gap"]`
  登记（不红主套件，缺口修好去掉 ignore 即转绿）。

### 硬约束与未覆盖面
- **在逐路径判定前，任何新站点不得按"自 2"接线**；默认按"夹具队列空 ⇒ 1"，除非能指出 `:2876` 前确有的 <155 入队。
- **"2 条情形"（recalc 前已有 622/638/53 待办）本批未覆盖**（夹具都是空队列）——这是**未覆盖**，不是"不成立"；
  需要时另立卡造该场景（如 `uenhancer` 的 638 前置入队形态可复用）。
- 登录批的两条 657（`smframe.rs` 两槽）**已核：oracle 实测 2 条**——2026-10-23 真机窗口两轮一致
  （`1cca49d22` 的提交记录：C# 在 654 之后发 `657,657`，Recog=自己/Param=0/Tag=0/Series=0），
  机制也吻合：`SendUpdateMsg` 去重时队首是 `RM_LOGON`（`10050 & 0xFF = 66`，`src/OpenMir2/Messages.cs:574`）
  ⇒ `TryPeek` 不前进、657 不在队首 ⇒ **不去重 ⇒ 2 帧**。这同时是"队列非空 ⇒ 2"的**真实在生产例证**，
  本批"未覆盖"指的是**我们自己没造**该场景的夹具，不是 oracle 没有该形态。

## 15. 片① 交付记录（2026-11-20，`openmir2-10`）

- 分支 `card/tick-frame-queue-slice1`，base master `bb160e2ad`（2026-11-20 第三次 rebase；
  前序 base `f8c0b4b7d` / `126e57e41`，原 base `165c6edd1`），代码 head `db60e9f32`
  （提交 `refactor(game-svr): 抽出 TickFrameQueue …`；纯 Rust，2 文件）。
  本段是交付后的 docs-only 补充，不改代码。
- 第二/三次 rebase 均无代码冲突；两次 `git range-diff` 都是 **`=`**（同一补丁）：
  `126e57e41..751d0cfd5` vs `f8c0b4b7d..3640c6208`、`f8c0b4b7d..3640c6208` vs
  `bb160e2ad..db60e9f32`。
- rebase 冲突解决（§38 逐支比对）：master 的 657 三动作 helper 已改为「广播副本 + 观察者 +
  **自更新副本**」（源自己 2 条）⇒ 入队版按 ①/②/③ 逐支合并：①③ 仍同 socket 同键 FIFO、
  ② 走观察者入队；换装路径保持 master 的 **1 条**自通知口径（没有误用三动作 helper 的 2 条）。
- 四个接线点全部落地：`castle.rs` 两处（252 / 165）、T-290 升级链 657、吃路径（53 + 通知 + 657）、
  换装（752/52、657）。**既有断言 0 条改动**；新增 1 条牙
  `per_recipient_heap_orders_subset_by_own_insertion`。

### 15a. 与 §8d 草稿的两处实现偏离（`opc-19` 2026-11-20 认可，`2f` 复核）

1. **收件人键 = `(gate_idx, socket)`，不是 §8d 草稿里的 `HashMap<i32 /*socket*/>`。**
   `socket` 是**各网关进程内的句柄**（网关侧 `raw_socket_i32`），跨网关线号会重复；本文件的
   墓碑本来就按 `(line, socket)` 键控，`route_to_gate` 也按 `gate_idx` 选线。只用 `socket`
   会把两条连接并进同一条队（发给 A 的帧可能落到 B 的线）。**验证**：快照场景 `castle_fanout`
   特意让 0 线与 1 线都有 `socket=901`，改造前后逐字节相同。
2. **`flush` 的 `route_to_gate` 检查粒度：仍是「每收件人一次」，但时点从原发送循环内移到 `flush` 内。**
   改造前是「每个收件人 `route_to_gate` 一次 → 该收件人整组帧直接 `send`」；改造后是「入队阶段不查，
   `flush` 里逐收件人查一次 → 整组 `send`」。**粒度不变，时点是同一函数调用内的后移**。
   若并发墓碑恰在这段之间落下，帧也到不了客户端：`downlink_slot`（网关连接任务里 `SendFrame` 的
   落地检查）会再查一次墓碑，槽已死 ⇒ 丢弃。⇒ 这个挪动只影响「生产者侧是否早丢」，
   不影响「客户端是否收到」，快照与全量用例均未出现差异。

### 15b. 预期差异（单列，**不进** T-288/T-295/T-296）

**机制**：oracle 里 657 的收件人在 `SendRefMsg` 发送时枚举、逐目标 `SendMsg`，消息进**每个 target
自己的 `MsgQueue`**（每 `BaseObject` 一条 `PriorityQueue<SendMessage, byte>`）。所以**每个收件人的
入队序 = 产生序**（本场景 A→B→C 的处理器顺序；三动作语义下源自己收**两条**：广播副本 + 自更新副本），
出队序只由**他自己队列里有多少条同键元素**决定。改造前的实现是「一张全局 tuple 堆」：先把 A/B/C 三条
657 入同一张堆，出队序 = 三条同键的四叉堆序 `[A,C,B]`，**再按收件人过滤**——这等于把「A 那条」
（Y 根本不在 A 的收件人里）也算进了 Y 的位形。

实测（`t290_chain`：A=1/B=2/C=3，X 全见、Y 只见 B/C；坐标 A(10,10)/B(10,32)/C(10,33)/
X(10,22)/Y(10,44)，range 12；源自己两条）：

| socket | 新实现入队序（= 产生序） | 新出队序 | 改造前出队序 |
|---|---|---|---|
| 901 (A 自己，2 条) | [1,1] | [1,1]（2 条同键 = FIFO） | [1,1] |
| 904 (X，3 条) | [1,2,3] | [1,3,2]（3 条同键 = 四叉堆序 [0,2,1]） | [1,3,2]（同一张全局堆，逐位相同） |
| 902 (B，3 条) | [2,2,3]（B 的两条自帧，再 C 的广播） | **[2,3,2]**（[0,2,1]） | [3,2,2]（全局 [A,C,B] 过滤） |
| 903 (C，3 条) | [2,3,3]（B 的广播，再 C 的两条自帧） | **[2,3,3]**（[0,2,1]） | [3,3,2] |
| 905 (Y，2 条) | [2,3] | **[2,3]**（FIFO） | [3,2] |

每 socket 帧数不变（901/904 不变；902/903/905 换序）。**904 不翻**是结构决定的（它的队列恰好也是
3 条、入队序也相同 ⇒ 同一置换）。牙 `per_recipient_heap_orders_subset_by_own_insertion`（rebase 后
断 901=[a,a]、904=[1,3,2]、905=[2,3]）在改造前树实测红（Y：`left: Some([3, 2])` /
`right: Some([2, 3])`），改造后绿；另在新树做变异（临时改回全局堆）也红（X：
`left: Some([1, 2, 3])` / `right: Some([1, 3, 2])`）。

### 15c. 门禁（rebase 后代码 head `db60e9f32`，作者侧）

`cargo fmt --check` / `cargo clippy --workspace --all-targets -- -D warnings`（0 error，rc=0）/
`cargo test --workspace`（4388 passed / 0 failed / 159 ignored，161 suites）/ `-p game-svr --lib`
（2156 / 0 / 73）/ `msgcodegen --check`（604 常量）/ 散文锚门（0/0/0 GREEN，`--base bb160e2ad`）/
工具门全量（71/71）/ dispatch 守卫（31/31，+0/−0）——全部 rc=0。

留痕：上一轮 clippy 曾红在 master `f8c0b4b7d` 自带的 `eat_move.rs` 文档续行
（`doc_lazy_continuation`，不在本卡写集；加 `-A` 该 lint 后 rc=0 已反证）；master `bb160e2ad`
修正后本 head clippy 全绿。

### 15d. 「逐帧字节相同」的准确口径

快照共 70 行 = 68 帧 + 2 条 `sent` 注记。**逐字节相同的只有这四个场景的 40 帧**：
`castle_fanout` 12 帧、`castle_refshow` 15 帧、`eat_stdmode1` 7 帧、`bag_takeoff` 6 帧。
`t290_chain` 28 帧中 20 帧相同、8 帧换序（§15b 的 902 三条 / 903 三条 / 905 两条 657）。
**不得**把它写成「整体逐字节相同」。

## 16. 片① 复跑装置（5 个场景，逐帧字节比）

装置是**临时 test 模块**（未入交付），源码与两棵树的输出放在 `docs/evidence/tickframe-slice1/`：
`tickframe_snapshot.rs`（装置）、`before.ndjson`（master `bb160e2ad`）、`after.ndjson`
（代码 head `db60e9f32`）、
`README.md`（逐步命令）。

复跑：把 `tickframe_snapshot.rs` 复制到 `mir2-rs/bins/game-svr/src/`，在 `lib.rs` 的测试模块声明区加一行
`#[cfg(test)] mod tickframe_snapshot;`，然后
`cd mir2-rs && TICKFRAME_SNAPSHOT_OUT=<out>.ndjson cargo test -p game-svr --lib -- tickframe_slice1_snapshot`。
判据：`castle_fanout` / `castle_refshow` / `eat_stdmode1` / `bag_takeoff` 与 `after.ndjson`
逐字节相同；`t290_chain` 只允许 §15b 的三处顺序差异（每 socket 帧数相同）。