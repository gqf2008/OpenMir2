# playdice ⑩ 全量：帧发射点分类表（SendSocket vs SendMsg）— 2026-10-25

- 卡：`openmir2-playdice-frame-queue-full`（tier **L**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；分支 `card/playdice-frame-queue-full`）。
- 基线：首版 `master` `9a7b8c798`；返工（review `f43dadb279` F1/F2/F3）在 `master` `bff927e11` 上重做。
- 写集：**只读取证报告**（本文件）。未改代码、未改场景/规则表、未起栈、未连库。
- 定位：T-142 ⑩-full 的**第一步**——先取证再改。结论见 §0；`⑩` 原文见
  `mir2-rs/tests/parity/whitelist.md` T-142 行（`:624`）。

## 0. 结论（先给判定）

**playdice / OpenYBDeal 动作自身产生的帧全部走「入队」**（`NpcPlayer.frames` → 宿主
`NetPriorityQueue` 按 `(byte)ident` 出队），**没有任何一条走 SendSocket 直连** ⇒ 本卡**无需改代码**。

同一商人 handler 里确有**直连**帧：`SM_SENDGOODSLIST(144)` / `SM_SENDUSERSELL(145)` 由
`shop::user_select` 的 `out` 经 `emit_shop_frames` 直接下发（oracle 用 `SendMsg` 入队）。但它们
**不是 playdice/OpenYBDeal 动作产生的帧**，而是买卖面板帧；这正是 T-142 ⑩ ① 已登记、
且部署当前不可观测的那条差异。修它 = 把 `out` 也并进 handler 级队列（**R 级、影响全部商店帧**），
不在本 L 卡写集内；建议另开 R 卡（§4）。

## 1. 方法

- Rust 侧：`grep -rn` 四类动作的实现（`crates/script/src/engine/actions.rs`）→ `NpcPlayer::send_msg`
  → 宿主排水函数（`bins/game-svr/src/lib.rs::emit_script_frames_raw` / `selloff::Emitter::finish`
  / `emit_shop_frames`）→ `pickwire::script_frame` 映射。
- oracle 侧：动作实现（`src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs`）与消息泵
  （`src/M2Server/Player/PlayObject.Message.cs`）逐帧对照。
- 判定口径：oracle `SendMsg` = 入 `MsgQueue`（handler 末按 `(byte)wIdent` 出队）⇒ 等价「入队」；
  `SendSocket` = 绕过队列直接写 socket ⇒ 等价「直连」。Rust 对应：先入
  `game_svr::msgqueue::NetPriorityQueue` 再 `SimToGate::SendFrame` = 入队；直接
  `tx.send(SimToGate::SendFrame)` 且无队列 = 直连。

## 2. 分类表（playdice / OpenYBDeal 动作自身）

| # | 帧 | Rust 发射点（动作） | Rust 排水（入队/直连） | oracle 发射点 | oracle 泵 | 判定 |
|---|---|---|---|---|---|---|
| 1 | `SM_PLAYDICE(8001)` ← `RM_PLAYDICE(10500)` | `crates/script/src/engine/actions.rs:889-903`（`ActionOfPlayDice`，`send_msg` 在 `:896-903`）→ `bins/game-svr/src/npc.rs:1904-1920`（`send_msg` 把帧 push 进 `NpcPlayer.frames`） | **入队**：`lib.rs:7879-7921` `emit_script_frames_raw` 先 `NetPriorityQueue::enqueue(f, f.ident as u8)`、再 `dequeue` 下发；映射 `pickwire.rs:232` `RM_SCRIPT_PLAYDICE` | `ExecutionProcessingSys.cs:2030-2034`（`SendMsg` 在 `:2033`） | `PlayObject.Message.cs:2034-2041` | **等价（入队）** |
| 2 | `SM_MERCHANTSAY(643)` ← `RM_MERCHANTSAY`（`OPENYBDEAL` 三支） | `actions.rs:905-938`（`ActionOfOpenSaleDeal`；`send_msg` 在 `:916` 已开通 / `:932` 成功 / `:938` 不足）→ `npc.rs:1904-1920` | **入队**：同 #1（`emit_script_frames_raw`）；映射 `pickwire::script_frame` 的 `RM_MERCHANTSAY` 支 | `ExecutionProcessingSys.cs:2329-2357`（`SendMsg` 在 `:2336` / `:2345` / `:2349`） | `PlayObject.Message.cs:1723-1725` | **等价（入队）** |
| 3 | `SM_GAMEGOLDNAME` ← `RM_GAMEGOLDCHANGED(8084)`（`GAMEGOLD` 动作） | `actions.rs:290` / `:314` 调 `game_gold_changed()` → `npc.rs:1539-1544`（先 `frames.retain` 同 ident 再 `send_msg`，= `SendUpdateMsg` 语义） | **入队**：同 #1；映射 `pickwire.rs:192-197`（含 `SoftVersionDateEx == 0` 门） | `BaseObject.cs:593-599` `GameGoldChanged()` → `:597 SendUpdateMsg` | `PlayObject.Message.cs:1836-1837` → `SendGoldInfo(false)` | **等价（入队；`SendUpdateMsg` 语义也一致）** |
| 4 | `SM_SENDDEALOFFFORM(23001)` ← `RM_SENDDEALOFFFORM(23000)` | `bins/game-svr/src/selloff.rs:1095-1131` `open_deal_off_form`（成功支 `em.later(PRIO_RM_SENDDEALOFFFORM, …)` 在 `:1123-1131`） | **入队**：`selloff.rs:317-370` `Emitter::later` 入 `queued`，`finish()` 按收件人各建 `NetPriorityQueue`（`:353-370`） | `Merchant.cs:1038-1056`（`SendMsg` 在 `:1046`） | `PlayObject.Message.cs:2046-2048` | **等价（入队；优先级 216）** |

> 四类动作的 `send_msg` 都只写 `NpcPlayer.frames`，**没有**任何 `SimToGate::SendFrame` 直调
> （`grep -n "SendFrame" npc.rs` 无生产命中；`crates/script` 不持有 gate 通道）⇒ 动作层不可能直连。
>
> **合并顺序提示（review `f43dadb279` observation）**：第 3 行引用的 `npc.rs:1539-1544`
> （`frames.retain` 同 ident）是基线 `9a7b8c798` 的事实；并行 R 卡
> `openmir2-script-playdice-openybdeal`（head `651b042ae`）已把该段改为「队首-only retain」。
> 两卡**合并顺序**会改变该行号与措辞，但「入队」结论不变；Leader 合并时请顺手更新行号。

## 3. 同 handler 的相邻帧（不在本卡目标集，但 ⑩ 原文点名）

### 3.1 已入队的相邻帧

| 帧 | Rust 发射点 | Rust 排水 | oracle 发射点 | 判定 |
|---|---|---|---|---|
| `SM_MERCHANTDLGCLOSE(644)` ← `RM_MERCHANTDLGCLOSE` | `actions.rs:109-114`（`ActionOfClose`）→ `npc.rs:1904-1920`；`guild_official.rs:790-793`（`queue_script_output`） | **入队**（脚本帧队列） | `ExecutionProcessingSys.cs:2183` / `Merchant.cs:922` `SendMsg` | 等价（入队） |
| `SM_MENU_OK(767)` ← `RM_MENU_OK` | `pickwire.rs:258`（脚本 `MESSAGEBOX`） | **入队**（脚本帧队列） | `ExecutionProcessingSys.cs:3418` `SendMsg` | 等价（入队） |

### 3.2 直连帧（`shop::user_select` 的 `out`，6 个发射点 + 说词路径）

`out` 的全部 `out.push(frame(...))` 都经 `lib.rs:5112-5129 emit_shop_frames` 按向量序
直接 `tx.send(SimToGate::SendFrame)`（**无** `NetPriorityQueue`）；oracle 对应全部是 `SendMsg` 入队。

| # | 帧 | Rust 发射点（`shop.rs`，`user_select` 的 `out`） | oracle 发射点（`Merchant.cs`，全部 `SendMsg`） | 判定 |
|---:|---|---|---|---|
| 1 | `SM_SENDGOODSLIST` ← `RM_SENDGOODSLIST(10128)` | `shop.rs:311-319` | `:991`（`UserSelectBuyItem`） | **不等价**（oracle 入队） |
| 2 | `SM_SENDUSERSELL` ← `RM_SENDUSERSELL(10129)` | `shop.rs:320-328` | `:996`（`UserSelectSellItem`） | **不等价** |
| 3 | `SM_SENDUSERREPAIR` ← `RM_SENDUSERREPAIR(10141)`（超级修理支） | `shop.rs:329-337` | `:1001`（`UserSelectRepairItem`） | **不等价** |
| 4 | `SM_SENDUSERREPAIR` ← `RM_SENDUSERREPAIR(10141)`（普通修理支） | `shop.rs:339-347` | `:1001`（同上） | **不等价** |
| 5 | `SM_SENDUSERSTORAGEITEM` ← `RM_USERSTORAGEITEM(10146)` | `shop.rs:350-358` | `:1028`（`UserSelectStorage`） | **不等价** |
| 6 | `SM_SENDUSERMAKEDRUGITEMLIST` ← `RM_USERMAKEDRUGITEMLIST(10149)` | `shop.rs:367-383` | `:1017`（`UserSelectMakeDurg`） | **不等价** |
| 7 | `SM_MERCHANTSAY(643)`（**说词路径**，非动作帧） | `lib.rs:7564-7587 emit_npc_says`：排序用 `npc.rs:112-124 dequeue_order`（**独立** `NetPriorityQueue`），随后直接 `tx.send` | `ScriptEngine.cs:993/997` `SendPriorityMsg`/`SendMsg`（入 `MsgQueue`） | **当前等价**：说词优先级 0/1 恒小于脚本帧最小 4，且独立堆序与 oracle 同口径；**未来合并队列时必须一起收口**（否则会把「等价」当「已入队」） |

**为什么这些不在本卡改**：

1. #1-#6 由**买卖/修理/仓库/制药面板**（`@buy`/`@sell`/`@repair`/`@storage`/`@makedrug`）产生，
   不是 playdice/OpenYBDeal 动作的帧；本卡目标是后者。#7 是 NPC 说词路径，当前等价。
2. 面板帧的差异只在「同一 handler 同时有面板帧与脚本帧」时可见；T-142 ⑩ ① 已登记部署当前**无该组合**
   （`whitelist.md:624` 原文：扫 `Market_Def` 无「面板标签 + GAMEGOLD」组合、无 `[@back]`）。
3. 正确修法是把 `out`（#1-#6）与 `says`（#7）一起并进 handler 级 `MsgQueue`（含 `script_frames` 的统一出队），
   属**帧时序/出队序**面 ⇒ 按 tier 政策应为 **R**，且影响全部商店帧与既有断言；本卡 tier=L
   且 `forbidden` 含「顺手改其它卡面」。

## 4. 建议（不在本卡执行）

- 若 owner 要收口 ⑩ ①：另开一张 **R** 卡，内容 = `handle_shop_cmd` 的 `out`（§3.2 的 **6 个发射点**）
  + `says`（`emit_npc_says`，§3.2 #7）+ `script_frames` 合并进同一 per-recipient
  `NetPriorityQueue`，并补「面板帧 + 脚本帧同 handler」的构造级判据与真机窗口。
- 判据输入（`(byte)RM_*` 优先级，低字节；`RM_*` 常量见 `crates/protocol/src/messages.rs`）：
  `RM_PLAYDICE=10500→4`、`RM_MENU_OK=10309→69`、`RM_MERCHANTDLGCLOSE=10127→143`、
  `RM_SENDGOODSLIST=10128→144`、`RM_SENDUSERSELL=10129→145`、`RM_GAMEGOLDCHANGED=8084→148`、
  `RM_SENDUSERREPAIR=10141→157`、`RM_USERSTORAGEITEM=10146→162`、
  `RM_USERMAKEDRUGITEMLIST=10149→165`、`RM_SENDDEALOFFFORM=23000→**216**`。
- **订正（F2）**：`RM_SENDDEALOFFFORM` 的首版值 152 是抄了 `selloff.rs:1055` 的旧错误注释；
  正确值 = `23000 & 0xFF = 216`（0xD8）。`selloff.rs` 的代码常量本身计算正确，本轮已把该注释一并改为 216。

## 5. 门禁（只报告）

- 卫生门禁：conflict-marker / stale-pointer / dynamic-count / board-coverage / t_reservation /
  whitelist-anchor / m3-registry：全 rc=0（本卡只新增本文件）。
- `git diff --check` rc=0；`git merge-tree --write-tree master HEAD` vs `bff927e11` = `5bd69910…` rc=0
  （首版 patch 记的 `b7f8f3c88` 是 **base tree**，不是 merge 结果——review F3 已订正；
  对首版 head `f5d206885` 的 merge tree 是 `50c9bc6d…`）。
- 未动代码 ⇒ 未跑 cargo 全量门禁（卡面 acceptance ③：只报告则卫生门禁 + diff --check）。

## 6. 判定与收口

- 卡面分叉：**「确有直连应入队」** ⇒ 本卡目标集（playdice/OpenYBDeal 动作帧）**没有**；
  **「无」** ⇒ 已逐帧登记为「等价入队」（§2），按卡面规则 **closed**。
- 相邻的面板直连帧（§3）已如实登记并给出 R 卡建议，**不伪造「等价」**、也不在本 L 卡顺手改。

## 7. 残余

- 无临时物；本文件是唯一仓库改动。
- 判不准项：`SM_SENDGOODSLIST/SENDUSERSELL` 的「部署当前不可观测」沿用 T-142 ⑩ 原文的扫描结论；
  本卡未重新扫描部署 `Envir`（只读仓内事实）。
