# handler 级 MsgQueue 统一（shop `out` + 说词 + 脚本帧）— 2026-10-25（rework per review `f215e887`）

- 卡：`openmir2-script-handler-msgqueue`（tier **R**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；
  分支 `card/script-handler-msgqueue`）。
- 基线：本地 `master` `1379a1833`（rebase 后）。
- 写集：`mir2-rs/bins/game-svr/src/{lib.rs,shop.rs}`、本文件。
- 红线：未改 protocol / storage 接口；未起栈、未占端口、未连库；不 merge、不自审。

## 0. 一句话

oracle 每个 `PlayObject` 有**一条** `PriorityQueue<SendMessage, byte>`（`ActorEntity.cs:16,21`），
入队优先级 `(byte)wIdent`（`BaseObject.Message.cs:181`），在**自己的 `Run`** 里一次排空
（`BaseObject.Base.cs:25-30`）。本片把 `handle_shop_cmd` 的三类帧——说词、脚本帧、`shop` 的 `out`
——并进同一条 per-recipient 队列，按 `(byte)RM_*` 四叉最小堆出队；**优先级由生产者标注**
（同一 SM 可来自不同 RM，不能反推）；多页取回是**一条队列消息内的原子批次**；发送者队列在
**世界扇出之前** flush（发送者 `Run` 先排空）。

## 1. 分类（每帧的 oracle 入队点；review `f215e887` 指正后）

### 1.1 入队帧（有 `RM_*` 泵 / `SendMsg`）

| SM（wire） | oracle 入队点 | 泵（`PlayObject.Message.cs`） | `(byte)RM` |
|---|---|---|---:|
| `SM_SENDGOODSLIST` | `Merchant.cs:991` `user.SendMsg(RM_SENDGOODSLIST,…)` | — | `RM_SENDGOODSLIST`=10128→**144** |
| `SM_SENDUSERSELL` | `Merchant.cs:996` | — | 10129→**145** |
| `SM_SENDUSERREPAIR`（普通修理） | `Merchant.cs:1001` `user.SendMsg(RM_SENDUSERREPAIR,…)` | `:1851-1853`（与超级修理**共用**分支） | 10141→**157** |
| `SM_SENDUSERREPAIR`（**超级修理**） | `Merchant.cs:958` `user.SendMsg(RM_SENDUSERSREPAIR,…)` | 同上 | `RM_SENDUSERSREPAIR`=50142→**222** |
| `SM_SENDUSERSTORAGEITEM` | `Merchant.cs:1028` | `:1865-1868` 区段 | 10146→**162** |
| `SM_SENDUSERMAKEDRUGITEMLIST` | `Merchant.cs:1017` | `:1876-1878` | 10149→**165** |
| `SM_SENDBUYPRICE` / `SM_SENDDETAILGOODSLIST` / `SM_SENDREPAIRCOST` | `Merchant.cs` 各 `SendMsg` | `:1816` / `:1831` / `:1859` | 10130→146 / 10135→151 / 10142→158 |
| `SM_USERSELLITEM_OK/FAIL`、`SM_BUYITEM_SUCCESS/FAIL`、`SM_USERREPAIRITEM_OK/FAIL`、`SM_MAKEDRUG_SUCCESS/FAIL` | `Operate.cs`/`Merchant.cs` 各 `SendMsg` | `:1819/1822/1825/1828`、`:1856/1862`、`:1879/1882` | 10131→147 / 10132→148 / 10133→149 / 10134→150 / 10143→159 / 10144→160 / 10150→166 / 10151→167 |
| `SM_DELITEMS` | `Merchant.cs:525/1501` `SendMsg(user, RM_SENDDELITEMLIST,…)` | **`:1870-1873` `SendDelItemList`** | `RM_SENDDELITEMLIST`=10148→**164** |
| `SM_SAVEITEMLIST`（**多页**） | `Operate.cs:1688` 区段 → `SendSaveItemList` | **`:1867-1868` `case RM_USERGETBACKITEM: SendSaveItemList`** | `RM_USERGETBACKITEM`=10147→**163** |
| `SM_FEATURECHANGED` | `BaseObject.cs:1509-1512 FeatureChanged()` = `SendRefMsg(RM_FEATURECHANGED,…)` → 每收件人 `SendMsg` | `:1762` | `RM_FEATURECHANGED`=10116→**132** |
| `SM_CHARSTATUSCHANGED` | `BaseObject.cs:1514-1517 StatusChanged()` = `SendRefMsg(RM_CHARSTATUSCHANGED,…)` | `:…` | 10139→**155** |
| `SM_GOLDCHANGED` | `BaseObject.cs:585-591 GoldChanged()` = `SendUpdateMsg(RM_GOLDCHANGED,…)` | `:1833-1835` | 10136→**152** |
| `SM_ABILITY` | `BaseObject.cs:2161/2185 SendMsg(RM_ABILITY,…)`（`:1385` 另有 `SendPriorityMsg(High)=0` 支） | `:1673` | 10051→**67** |
| `SM_MERCHANTSAY`（in `out`） | `Merchant.cs` 修理失败等 `SendMsg(RM_MERCHANTSAY,…)` | `:1723-1725` | 11009→**1** |
| `SM_SYSMESSAGE`（in `out`） | `BaseObject.cs:1617+ SysMsg` = `SendMsg(RM_SYSMESSAGE,…)` | `:1715-1721` | 11004→**252** |

> **「泵一对一」不成立**：`RM_SENDUSERREPAIR`(157) 与 `RM_SENDUSERSREPAIR`(**222**) 共用泵分支
> `:1851-1853`，同一 SM `SM_SENDUSERREPAIR` 无法反推优先级 ⇒ 优先级由**生产者**（`shop.rs` 的
> `ShopFrame::queue_priority`）标注。

### 1.2 真直连帧（调用点 `SendSocket`/`SendDefMessage`，无 RM 泵）

`SM_DELITEM`（`SendDelItem`，`PlayObject.Base.cs:1663`）、`SM_ADDITEM`（`:226`）、
`SM_STORAGE_OK/FULL/FAIL`（`Operate.cs:1634/1643/1652`）、
`SM_TAKEBACKSTORAGEITEM_OK/FULLBAG/FAIL`（`:1688/1697/1711`）。

> **订正（review M-1/M-2/M-3）**：`SM_DELITEMS` / `SM_FEATURECHANGED` / `SM_SAVEITEMLIST`
> **不是**直连——它们的函数体里虽有 `SendSocket`，但**唯一调用点是泵**（`MainMessage` 的
> `case RM_…`），入队发生在 `SendMsg` 发射点，`SendSocket` 只是出队后的写动作。第一版把它们
> 归为直连是错的，C8a 的期望也因此被改反；本版已按泵归属纠正。

## 2. 实现

- `shop::ShopFrame` 新增 `queue_priority: Option<u8>` / `atomic_batch: bool`；每个 `frame(...)`
  发射点用 `.queued(RM_*)` 标注（直接帧保持 `None`），取回多页用 `.atomic()`。
- `HandlerMsgQueue`：`NetPriorityQueue<Vec<Vec<u8>>>`（**一条队列消息 = 一组按序 body**）+
  `direct` 批 + 显式 `flush()`（直连批 → 堆，批次内按页序）；`Drop` 仅作早退兜底。
- `emit_shop_frames_into`：`Some(prio)` 入堆；**连续同优先级的 `atomic_batch` 帧合并成一条消息**。
- `handle_shop_cmd` = 薄壳：建队列 → `handle_shop_cmd_inner`（只入队/登记）→
  **`queue.flush()`（发送者帧先上 wire）** → 执行登记的其它收件人发送（`SENDMSG` 扇出 / 41 / 657）。
  `DeferredSend` 承载延后发送；早退路径同样走 flush + 已登记发送。

## 3. 判据（构造级；acceptance ②③④）

| 用例 | 钉什么 |
|---|---|
| `producer_frames_carry_oracle_queue_priorities`（`shop.rs`） | 生产者标注：buy 144 / sell 145 / 普通修理 **157** / 超级修理 **222** / storage 162 / makedrug 165；`del_items_frame` **164**；`back_item_list` 120 件 ⇒ ≥3 页、每页 **163 + atomic** |
| `handler_msgqueue_says_plus_script_order_is_unchanged_by_merge` | 说词(1) 恒先于脚本帧(69)；合并前后一致 |
| `handler_msgqueue_panel_and_script_interleave_by_rm_priority` | `@buy`：643(1) → 767(69) → 44(139) → 645(144) → 5008(148)；且≠旧直连序 |
| `handler_msgqueue_ties_keep_four_ary_heap_order` | 3 条同 `RM_MENU_OK`(69) ⇒ 四叉堆 `[0,2,1]` |
| `handler_msgqueue_getback_pages_keep_page_order_as_one_message` | 3 页同优先级 163 **一条消息** ⇒ 线序 `[0,1,2]`（堆不会打散） |
| `sendmsg_case0..4` / `sendmsg_same_ident_frames_dequeue_in_four_ary_heap_order`（**恢复原断言**） | 发送者自己的帧（901）先于其它收件人（902）：发送者 `Run` 先排空；同 ident 252 仍 `[0,2,1]` |
| `upgrade_npc_submit_and_getback_full_chain` C8a（**恢复原断言**） | `202(DELITEM,直连) → 52(67) → 41(132) → 653(152) → 657(155)×2 → 709(164)` |

### 牙（逐条实跑：变异 → 目标用例红 → 还原绿）

| # | 变异 | 实际红 |
|---:|---|---|
| 1 | `emit_shop_frames_into` 的 `Some` 支改 `queue.direct` | 面板交织用例红 |
| 2 | 超级修理优先级改回 `RM_SENDUSERREPAIR`(157) | 生产者用例红（222） |
| 3 | 去掉 `atomic_batch` 分组合并 | getback 页序用例红（`[0,2,1]`） |
| 4 | **M-6**：`queue.flush()` 移到扇出之后（交给 `Drop`） | `sendmsg_case0` 红：`left=[(902,other),(901,hero)]`（复现 review 的牙 C） |
| 5 | `SM_DELITEMS` 去掉 `.queued(164)` | 生产者用例红 + C8a 红 |
| 6 | `SM_FEATURECHANGED` 去掉 `.queued(132)` | C8a 红 |

## 4. 部署可达性（acceptance ⑥，2026-10-25 复扫 `E:/MirServer/M2GameSvr/Envir`）

- 面板标签计数（`Market_Def/`，文件数/行数）：`@buy` 63/199、`@sell` 62/195、`@repair` 41/146、
  `@s_repair` 41/87、`@storage` 16/48、`@getback` 17/53、`@makedrug` 1/3、`@dealybme` **0/0**。
- **同一脚本标签内既有面板标签又有帧产生动作 = 0 处**（逐文件按 `[label]` 段落扫
  `MESSAGEBOX|SENDMSG|PLAYDICE|CHANGEEXP|GAMEGOLD|GAMEPOINT|OPENYBDEAL`）⇒ T-142 ⑩ 的
  「部署当前无该组合」复核成立。
- **`@back` 字面量 = 0**；部署只有 `@back2`（2 文件 4 行：`功能NPC/真爱-G002.txt`、`真爱666-G002.txt`）。
  `handle_shop_cmd` 的 `is_back` 走的是**脚本标签派发**（`Merchant.cs:808` 的 `sLabel`），
  不等于「客户端字面发 `@back`」⇒ 窗口请按 `@back2`/脚本标签构造。

## 5. 真机窗口项（R 级，`window_required: true`）

- **W-MQ-1**：构造「面板标签 + 脚本帧」同 handler ⇒ `SM_MERCHANTSAY(643) → SM_MENU_OK(767) →
  SM_WINEXP(44) → SM_SENDGOODSLIST(645) → SM_GAMEGOLDNAME(5008)`，与 C# 侧纯字节序 diff=0。
- **W-MQ-2**：同标签 3 条同 ident 脚本帧 ⇒ 出队 `[0,2,1]`（四叉堆序，非入队序）。
- **W-MQ-3**：**发送者先于收件人**——`SENDMSG 0` 场景下，发送者 socket 的 `SM_SYSMESSAGE` 先于
  其它在线玩家的同帧（两侧 wire 序一致）；不得出现「收件人先」。
- **W-MQ-4**：`@getback` 仓库取回 ≥51 件 ⇒ 多页 `SM_SAVEITEMLIST` 按页序连发（不被同优先级重排）。
- 命令模板：`python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/mq/sel --map 7101=127.0.0.1:7100 --gate sel --direction s2c`，
  跑完 `--verify`；同命令 C# 侧重跑，按帧 sha256 对拍。

## 6. 已知偏差 / 残余

1. **`@dealybme` 元宝寄售开窗帧仍未并入本队列**：该分支在 `selloff` 自己的 emitter 里即时
   `tx.send`（`RM_SENDDEALOFFFORM`=23000→**216** / `RM_MERCHANTSAY`=1）。部署 `@dealybme` = **0 行**
   ⇒ 当前不可观测；按卡面写集（`shop.rs`/`lib.rs`/`npc.rs`）未改 `selloff.rs`，登记为后续片。
2. `Drop` 兜底 flush 只在早退路径生效（正常路径显式 flush），两者都满足「发送者先于收件人」。
3. 其它收件人的帧仍按批顺序 `tx.send`（各收件人自己的 `MsgQueue` 未逐一建模，属既有实现）；
   W-MQ-3 只判发送者 vs 收件人的**相对**先后。
4. 部署无同标签组合（§4）⇒ 判据以构造级为主（R 级政策要求）。

## 7. 门禁（实测）

- `cargo fmt --all -- --check` rc=0；`cargo clippy --workspace --all-targets -- -D warnings` rc=0。
- `cargo test --workspace -- --test-threads=2`：值见 patch body（本轮返工后重跑）。
- `cargo run -p msgcodegen -- --check` rc=0。
- 7 项卫生门禁（故障期：`stale_pointer_gate`/`dynamic_count_stamp_gate` 带 `--ref master`）+
  `script_host_noop_audit check` + `merge_tree_helper --selftest` 全 rc=0。
- `git diff --check <base> <head>` rc=0（**带范围**）。
- `merge_tree_helper --base master --head <head>`：值见 patch body。
