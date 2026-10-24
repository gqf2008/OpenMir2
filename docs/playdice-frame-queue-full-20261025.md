# playdice ⑩ 全量：帧发射点分类表（SendSocket vs SendMsg）— 2026-10-25

- 卡：`openmir2-playdice-frame-queue-full`（tier **L**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；分支 `card/playdice-frame-queue-full`）。
- 基线：本地 `master` `9a7b8c798`。
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
| 4 | `SM_SENDDEALOFFFORM(23001)` ← `RM_SENDDEALOFFFORM(23000)` | `bins/game-svr/src/selloff.rs:1095-1131` `open_deal_off_form`（成功支 `em.later(PRIO_RM_SENDDEALOFFFORM, …)` 在 `:1123-1131`） | **入队**：`selloff.rs:317-370` `Emitter::later` 入 `queued`，`finish()` 按收件人各建 `NetPriorityQueue`（`:353-370`） | `Merchant.cs:1038-1056`（`SendMsg` 在 `:1046`） | `PlayObject.Message.cs:2046-2048` | **等价（入队；优先级 152）** |

> 四类动作的 `send_msg` 都只写 `NpcPlayer.frames`，**没有**任何 `SimToGate::SendFrame` 直调
> （`grep -n "SendFrame" npc.rs` 无生产命中；`crates/script` 不持有 gate 通道）⇒ 动作层不可能直连。

## 3. 同 handler 的相邻帧（不在本卡目标集，但 ⑩ 原文点名）

| 帧 | Rust 发射点 | Rust 排水 | oracle | 判定 |
|---|---|---|---|---|
| `SM_MERCHANTDLGCLOSE(644)` ← `RM_MERCHANTDLGCLOSE` | `actions.rs:109-114`（`ActionOfClose`）→ `npc.rs:1904-1920`；`guild_official.rs:790-793`（`queue_script_output`） | **入队**（脚本帧队列 / `NetPriorityQueue`） | `ExecutionProcessingSys.cs:2183` / `Merchant.cs:922` `SendMsg` | 等价（入队） |
| `SM_MENU_OK(767)` ← `RM_MENU_OK` | `pickwire.rs:258`（脚本 `MESSAGEBOX`） | **入队**（脚本帧队列） | `ExecutionProcessingSys.cs:3418` `SendMsg` | 等价（入队） |
| `SM_SENDGOODSLIST(144)` | `shop.rs:311-319`（`user_select` 的 `out`） | **直连**：`lib.rs:5112-5129` `emit_shop_frames` 按向量序 `tx.send`（无队列） | `Merchant.cs:988-992` `SendMsg` | **不等价**（oracle 入队） |
| `SM_SENDUSERSELL(145)` | `shop.rs:321-328`（`out`） | **直连**：同上 | `Merchant.cs:994-996` `SendMsg` | **不等价**（oracle 入队） |

**为什么这两条不在本卡改**：

1. 它们由**买卖面板**（`@buy`/`@sell`）产生，不是 playdice/OpenYBDeal 动作的帧；本卡目标是后者。
2. 差异只在「同一 handler 同时有面板帧与脚本帧」时可见；T-142 ⑩ ① 已登记部署当前**无该组合**
   （`whitelist.md:624` 原文：扫 `Market_Def` 无「面板标签 + GAMEGOLD」组合、无 `[@back]`）。
3. 正确修法是把 `out` 并进 handler 级 `MsgQueue`（含脚本帧/说词/says 的统一出队），属
   **帧时序/出队序**面 ⇒ 按 tier 政策应为 **R**，且影响全部商店帧与既有断言；本卡 tier=L
   且 `forbidden` 含「顺手改其它卡面」。

## 4. 建议（不在本卡执行）

- 若 owner 要收口 ⑩ ①：另开一张 **R** 卡，内容 = `handle_shop_cmd` 的 `out` + `says` + `script_frames`
  合并进同一 per-recipient `NetPriorityQueue`，并补「面板帧 + 脚本帧同 handler」的构造级判据与真机窗口。
- 判据输入：本报告 §2/§3 的发射点与优先级（`(byte)ident`：`RM_PLAYDICE=4`、`RM_MENU_OK=69`、
  `RM_MERCHANTDLGCLOSE=143`、`RM_SENDGOODSLIST=144`、`RM_SENDUSERSELL=145`、`RM_GAMEGOLDCHANGED=148`）。

## 5. 门禁（只报告）

- 卫生门禁：conflict-marker / stale-pointer / dynamic-count / board-coverage / t_reservation /
  whitelist-anchor / m3-registry：全 rc=0（本卡只新增本文件）。
- `git diff --check` rc=0；`git merge-tree` 无冲突。
- 未动代码 ⇒ 未跑 cargo 全量门禁（卡面 acceptance ③：只报告则卫生门禁 + diff --check）。

## 6. 判定与收口

- 卡面分叉：**「确有直连应入队」** ⇒ 本卡目标集（playdice/OpenYBDeal 动作帧）**没有**；
  **「无」** ⇒ 已逐帧登记为「等价入队」（§2），按卡面规则 **closed**。
- 相邻的面板直连帧（§3）已如实登记并给出 R 卡建议，**不伪造「等价」**、也不在本 L 卡顺手改。

## 7. 残余

- 无临时物；本文件是唯一仓库改动。
- 判不准项：`SM_SENDGOODSLIST/SENDUSERSELL` 的「部署当前不可观测」沿用 T-142 ⑩ 原文的扫描结论；
  本卡未重新扫描部署 `Envir`（只读仓内事实）。
