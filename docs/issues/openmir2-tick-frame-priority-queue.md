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

## 3b. 范围（逐条清点后回填）

1. **现状清点**：`route_to_gate(...)` / `tx.send(SimToGate::SendFrame ...)` 直发点清单（按模块分组）；
2. **改造面**：哪些帧要改为入队（657 / 210/212 / 45/52/752 / 其它直发 SM）；哪些**不能**入队
   （`MoveFail` 这类"当帧必须到"的？——需按 oracle 逐条核，不自作主张）；
3. **用例影响**：预计多少既有"精确帧序"断言要重写（先出数字，再动手）；
4. **判据**：同一场景下"入队版"与"直发版"的帧序差异，必须逐条能对上 oracle 的 `(byte)ident` 序。

## 4. 不做

- 不在本卡之前再新增"按阶段/按结束点"的近似（T-288–T-294 维持登记，不各自发明新序）；
- 不改 oracle 侧语义、不改客户端冻结契约。
