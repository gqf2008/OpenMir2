# handler 级 MsgQueue 统一（shop `out` 6 发射点 + 说词 + 脚本帧）— 2026-10-25

- 卡：`openmir2-script-handler-msgqueue`（tier **R**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；
  分支 `card/script-handler-msgqueue`）。
- 基线：本地 `master` `1379a1833`（rebase 后；原 `263e77d7e`）。
- 写集：`mir2-rs/bins/game-svr/src/lib.rs`、本文件（`shop.rs`/`npc.rs` 未改——分类表与入队版都落在
  lib.rs 的宿主排水层，见 §3）。
- 红线：未改 protocol / storage 接口；未改 `shop.rs` 的 6 个发射点（`out` 的产生面不动，只在排水侧分流）；
  未起栈、未占端口、未连库；不 merge、不自审。

## 0. 一句话

oracle 每个 `PlayObject` 有**一条** `PriorityQueue<SendMessage, byte>`（`ActorEntity.cs:16`，优先级
`(byte)wIdent`，`BaseObject.Message.cs:181`），handler 末尾（`Run`）一次按四叉最小堆出队。本片把
`handle_shop_cmd` 的三类帧——**说词**（`ScriptEngine.cs:993/997`）、**脚本帧**（`NpcPlayer.frames`）、
**面板/结果帧**（`shop::user_select` 的 `out`，§3.2 的 6 个发射点）——并进**同一条** per-recipient
队列（`HandlerMsgQueue`），按 `(byte)RM_*` 升序出队；无 RM 泵的直连帧（`SendSocket`/`SendDefMessage`）
保持「调用点即发」并在堆排水之前保序下发。

## 1. 发射点与 oracle 行号（acceptance ①）

| # | Rust 发射点（本片改动前：直连） | oracle 发射点（全部 `SendMsg` = 入队） | RM → 优先级 |
|---:|---|---|---:|
| 1 | `shop::user_select` `SM_SENDGOODSLIST`（`out` 第 1 点） | `Merchant.cs:991`（`UserSelectBuyItem`） | `RM_SENDGOODSLIST`=10128→**144** |
| 2 | `SM_SENDUSERSELL`（第 2 点） | `Merchant.cs:996`（`UserSelectSellItem`） | 10129→**145** |
| 3 | `SM_SENDUSERREPAIR`（超级修理支，第 3 点） | `Merchant.cs:1001`（`UserSelectRepairItem`） | 10141→**157** |
| 4 | `SM_SENDUSERREPAIR`（普通修理支，第 4 点） | `Merchant.cs:1001`（同上） | 10141→**157** |
| 5 | `SM_SENDUSERSTORAGEITEM`（第 5 点） | `Merchant.cs:1028`（`UserSelectStorage`） | 10146→**162** |
| 6 | `SM_SENDUSERMAKEDRUGITEMLIST`（第 6 点） | `Merchant.cs:1017`（`UserSelectMakeDurg`） | 10149→**165** |
| 7 | 说词 `emit_npc_says`（原独立堆直发） | `ScriptEngine.cs:993`（`SendPriorityMsg(High)`）/ `:997`（`SendMsg`） | `High`=**0** / `RM_MERCHANTSAY`=11009→**1** |
| — | 脚本帧（原 `emit_script_frames_raw` 独立堆） | 各动作 `SendMsg`（`PlayObject.Message.cs` 的 `case RM_X` 泵） | `(byte)RM_*` |

**直连帧分类（`shop_frame_queue_priority → None`，oracle 无 RM 泵）**：`SM_ADDITEM`（`SendAddItem`，
`PlayObject.Base.cs:226` `SendSocket`）、`SM_DELITEMS`/`SM_DELITEM`（`:1644/:1663`）、
`SM_STORAGE_OK/FULL/FAIL`（`Operate.cs:1634/1643/1652`）、`SM_TAKEBACKSTORAGEITEM_OK/FULLBAG/FAIL`
（`:1688/1697/1711`）、`SM_SAVEITEMLIST`（`Base.cs:1620`）、`SM_FEATURECHANGED`（`PlayObject.cs:1145`
`FeatureChanged()` 直发）。

**F2 订正**：`RM_SENDDEALOFFFORM = 23000 ⇒ 216`（不是旧注释的 152；152 是 `RM_GOLDCHANGED`）。
本片在 `shop_frame_queue_priority` 的 `SM_SENDDEALOFFFORM` 分支与判据表里同时钉住。

## 2. 实现（`bins/game-svr/src/lib.rs`）

- `struct HandlerMsgQueue`：`heap: NetPriorityQueue<Vec<u8>>` + `direct: Vec<Vec<u8>>` + 绑定的网关线；
  `Drop` 时**先直连批、再堆排水**（oracle：直连帧在 `UserSelect*` 调用点即发，队列在 `Run` 尾排水）。
  用 `Drop` 是因为历史上 handler 有 20+ 条早退路径（`return;`），逐点 flush 易漏。
- `shop_frame_queue_priority(ident) -> Option<u8>`：SM → `(byte)RM_*`（表见 §1）；`None` = 直连。
- `emit_npc_says_into` / `emit_script_frames_into` / `emit_shop_frames_into`：三个「只入队」版本；
  原 `emit_npc_says`/`emit_script_frames`/`emit_shop_frames` 保留给其它路径（@Login/@main/点击/仓库）。
- `emit_merchant_goto_result(..., queue)`：说词与脚本帧入队；`SENDMSG 0-4` 的世界侧扇出仍即时
  （其它玩家各有自己的队列，与本会话队序无关）；`RM_HEAR` 的 ±12 格过滤口径不变。
- `handle_shop_cmd`：入口建 `HandlerMsgQueue`（`route_to_gate` 一次），5 个 `emit_merchant_goto_result`
  传入同一队列，末尾 `emit_shop_frames_into(&mut handler_queue, &out)`；`Drop` 统一 flush。

**不改 `shop.rs` 的原因**：优先级由 SM ident 反查 `(byte)RM`（oracle 泵一对一），比在每个
`out.push(frame(...))` 处再带一个 RM 参数更不容易漏；`shop.rs` 的 `emit_shop_frames` 单测（尾编码
口径）也无需改动。

## 3. 判据（构造级为主；acceptance ②③④）

| 用例 | 钉什么 |
|---|---|
| `shop_frame_queue_priority_matches_oracle_pump_table` | 22 个 SM→RM 字节 + 11 个无泵 `None`；含 `SM_SENDDEALOFFFORM→216`（F2） |
| `handler_msgqueue_says_plus_script_order_is_unchanged_by_merge` | **合并前后一致**：说词(1) 恒先于脚本帧(69)（有/无面板都不变的部分） |
| `handler_msgqueue_panel_and_script_interleave_by_rm_priority` | `@buy`：说词(1) → `SM_MENU_OK`(69) → `SM_WINEXP`(139) → `SM_SENDGOODSLIST`(144) → `SM_GAMEGOLDNAME`(148)；并断言它**不等于**旧直连序（645 在 148 之后），证明「改变只发生在 oracle 要求的组合」 |
| `handler_msgqueue_ties_keep_four_ary_heap_order` | 3 条同 `RM_MENU_OK`（69）⇒ 四叉堆序 `[0,2,1]`（文本 a,c,b），并入同一队后不得退化成稳定排序 |

### 牙（逐条实跑：变异 → 目标用例红 → 还原绿）

| # | 变异 | 实际红 |
|---:|---|---|
| 1 | `emit_shop_frames_into` 全部 `queue.direct`（面板不进堆） | 交织用例红（面板被提前到堆前） |
| 2 | 面板优先级改用 `SM ident & 0xFF`（133）而非 `(byte)RM`（144） | 交织用例红（645 与 44 换位） |
| 3 | 说词优先级打乱成 200（大于 69） | 说词恒先用例红 |
| 4 | 脚本帧绕过队列（`emit_script_frames_into` 改 `direct`） | 交织 + 同优先级 + 说词恒先 3 条全红 |
| 5 | `SM_SENDDEALOFFFORM` 取值改回 152 | 优先级表用例红（期望 216） |

### 既有断言的同步（必读）

- **跨收件人全局顺序不受 oracle 约束**（每个 `PlayObject` 各自一条队列，墙钟上谁先发不构成契约）。
  合并后本会话的帧在 `Drop` 才发 ⇒ 全局 wire 序里「本例玩家」不再天然排在扇出帧之前。6 条
  `sendmsg_*` 用例改为**按 socket 稳定排序后断言**（组内序不变，组间序不再声明）。
- `upgrade_npc_submit_and_getback_full_chain`（C8a）：oracle 语义为「直连帧先、堆后」——
  新期望 `202(DELITEM) → 41(FEATURECHANGED) → 709(DELITEMS)`（直连，按产生序）→
  `52(67) → 653(152) → 657(155)×2`（堆）；逐帧内容断言改用「语义分组视图」，避免内容与线序耦合。

## 4. 部署可达性（acceptance ⑥，2026-10-25 复扫 `E:/MirServer/M2GameSvr/Envir`）

- 面板/面板标签计数（`Market_Def/`，文件数/行数）：`@buy` 63/199、`@sell` 62/195、`@repair` 41/146、
  `@s_repair` 41/87、`@storage` 16/48、`@getback` 17/53、`@makedrug` 1/3、`@dealybme` **0/0**。
- **「同一脚本标签内既有面板标签又有帧产生动作」= 0 处**（逐文件按 `[label]` 段落扫
  `MESSAGEBOX|SENDMSG|PLAYDICE|CHANGEEXP|GAMEGOLD|GAMEPOINT|OPENYBDEAL`）⇒ T-142 ⑩ 的
  「部署当前无该组合」**复核成立**（此前 GAMEGOLD 专项扫描 = 0，本片扩大到全部帧产生动作仍 = 0）。
- **`@back` 字面量 = 0**（`grep -ria '@back>' Envir` = 0）；部署只有 `@back2`（2 文件 4 行：
  `Market_Def/功能NPC/真爱-G002.txt`、`真爱666-G002.txt`）。`handle_shop_cmd` 里的 `is_back`
  分支服务的是**脚本标签 `@back`/`@back2` 的通用回跳**（`Merchant.cs:808` 的 `sLabel` 派发），
  不是「客户端字面发 `@back`」⇒ 窗口场景请按 `@back2`/脚本标签构造，别按字面 `@back` 找不到。
- 结论：部署当前**观测不到**本片的帧序变化（无同标签组合；说词 1 恒小于面板 144，顺序不变）；
  真机窗口项用于**复核**而非发现。

## 5. 真机窗口项（R 级，`window_required: true`）

- **W-MQ-1**：构造/入侵一条「面板标签 + 脚本帧」同 handler 场景（例如私有测试商人 `[@buy]` 内放
  `MESSAGEBOX` + `GAMEGOLD`），客户端收帧序 = `SM_MERCHANTSAY(643) → SM_MENU_OK(767) →
  SM_SENDGOODSLIST(645) → SM_GAMEGOLDNAME(5008)`（按 `(byte)RM`：1→69→144→148），与 C# 栈
  同场景纯字节序 diff=0。
- **W-MQ-2**：同标签 3 条同 ident 脚本帧（如 3×`MESSAGEBOX`）出队 `[0,2,1]`（四叉堆序，非入队序），
  两侧一致。
- **W-MQ-3**：直连帧（如修理/升级链 `SM_DELITEM`/`SM_DELITEMS`/`SM_FEATURECHANGED`）仍**先于**
  堆排水；C# 栈同场景逐帧对拍。
- 命令模板：`python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/mq/sel --map 7101=127.0.0.1:7100 --gate sel --direction s2c`，
  跑完 `--verify`；同命令 C# 侧重跑，按帧 sha256 对拍。

## 6. 已知偏差 / 残余

1. `Drop` 时机 ≈ handler 末尾（oracle 是 `PlayObject.Run` 尾）；同 handler 内没有其它队列消费者 ⇒
   观测等价；跨收件人的全局序不建模也不声明（见 §3）。
2. 直连帧在 Rust 里统一在 flush 前按产生序发（oracle 是调用点即发）；同一 handler 内两者之间
   没有其它帧插入 ⇒ 观测等价；若将来有第三方消费者插入，需要按调用点拆 flush。
3. `SM_SYSMESSAGE` 只按 `RM_SYSMESSAGE`(252) 入堆；`RM_GROUPMESSAGE`(253) 走 `SendGroupText`，
   不在本 handler 的 `out`（分类表注释已写明）。
4. 面板标签 + 帧动作组合当前部署不可达（§4）⇒ 本卡判据以**构造级**为主（R 级政策要求）。

## 7. 门禁（实测）

- `cargo fmt --all -- --check` rc=0；`cargo clippy --workspace --all-targets -- -D warnings` rc=0。
- `cargo test --workspace -- --test-threads=2`：**3221 passed / 0 failed**（141 suites）。
- `cargo run -p msgcodegen -- --check` rc=0。
- 7 项卫生门禁（故障期口径：`stale_pointer_gate`/`dynamic_count_stamp_gate` 带 `--ref master`）+
  `script_host_noop_audit check` + `merge_tree_helper --selftest` 全 rc=0。
- `git diff --check 1379a1833 <head>` rc=0（**带范围**，不用无参 `git diff --check`）。
- `merge_tree_helper --base master --head <head>`：值见 patch body（rebase 后）。
