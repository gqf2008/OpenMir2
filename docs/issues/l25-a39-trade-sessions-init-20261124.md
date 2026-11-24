> 原路径 E:/UserTemp/a39-trade-sessions-init.md（2026-11-24 入仓，内容未改）
> 入仓时锚已规范化（oracle `src/**` 行号保留、本仓高 churn 文件改符号锚），结论与事实未改；原文锚对应 `250c8a97d`。
# A39 只读核查：`trade_sessions` 的生产初始化与完整交易写回面

> 只读、未编译、未写码。证据取自 `a34-slice3` 工作树（= master `2798afde1` + A34 片 3），
> 行号为该树现值；**符号锚**为准（master `7a654716f` 行号会小幅漂移）。
> 核查对象：`handle_deal_cmd` 只回写“已存在”的 `trade_sessions` 条目 ⇒ 交易第二方状态是否落地。

---

## 1. `trade_sessions` 的全部生产插入点

grep 结果（`mir2-rs/bins/game-svr/src/*.rs`，排除测试）：生产代码里 **只有两处 insert**，都在
`handle_deal_cmd`（`fn handle_deal_cmd(`，当前 `mir2-rs/bins/game-svr/src/lib.rs` `handle_deal_cmd`）：

| # | 位置 | 代码 | 触发/时机 | 覆盖哪些会话 |
|---|---|---|---|---|
| 1 | `handle_deal_cmd` 回写尾（`mir2-rs/bins/game-svr/src/lib.rs` `handle_deal_cmd`） | `trade_sessions.insert(chr_name.to_string(), me_tr);` | **无条件**：任何 `CM_DEAL*` 命令经过本函数时，为**命令发起者**写入（缺条目则以 `DealState::default()` 起步） | 仅**发起命令的那一侧** |
| 2 | 同上（`mir2-rs/bins/game-svr/src/lib.rs` `handle_deal_cmd`） | `if let (Some(c), Some(tr)) = (other_chr.clone(), other_tr) { trade_sessions.insert(c, tr); }` | **有条件**：仅当**对手方原本已有条目**时回写 | 对手方——**原本没有条目则完全不写** |

读取/摘除点（不是初始化）：
- `other_tr = other_chr.as_ref().and_then(|c| trade_sessions.remove(c))`（`mir2-rs/bins/game-svr/src/lib.rs` `handle_deal_cmd`）：
  对手无条目 ⇒ `None` ⇒ 后续用 **`dummy_tr = DealState::default()`** 充当对手侧状态。
- `let mut me_tr = trade_sessions.remove(chr_name).unwrap_or_default();`（`mir2-rs/bins/game-svr/src/lib.rs` `handle_deal_cmd`）。
- `cancel_cross_zone_trade_and_selloff` 的 `trade_sessions.remove(chr_name)`（`mir2-rs/bins/game-svr/src/lib.rs` `cancel_cross_zone_trade_and_selloff`，A35-b
  跨区前取消；**只删不建**）。
- `drain_deal_tick` / ground / shop / selloff 全部**只读**（形参 `&HashMap`）。

**登录/进世界是否给每个会话建条目**：**没有**。证据：
- `finish_login(ctx)`（`mir2-rs/bins/game-svr/src/lib.rs` `finish_login`）与 `finish_login_on_zone_state(...)`（`mir2-rs/bins/game-svr/src/lib.rs` `finish_login_on_zone_state`）区间内
  **零** `trade_sessions` 提及（脚本化 grep：`34504..=35904` 命中 0）。
- `sim_loop` 只在构造 `HostLoop` 时 `let trade_sessions = HashMap::new();`（`mir2-rs/bins/game-svr/src/lib.rs` `sim_loop`）并
  原样装结构体；区 k `ZoneState::new` 同样是空表。
- `close_cleanup_residue`（`mir2-rs/bins/game-svr/src/lib.rs` `close_cleanup_residue`）摘 `saves`/`ground_sessions`/
  `shop_sessions`/`sell_off_sessions`/展示表——**不含 `trade_sessions`**（登出后条目残留；
  见 §5 观察 2）。
- 测试夹具普遍**预置双方条目**（如 `lib.rs` 测试区 `46071/46185/78441/78682` 一带），
  因此既有用例没有覆盖“对手方无条目”的首次交易。

⇒ **结论证据链**：生产里每个会话的交易条目是**惰性**的，且惰性创建只发生在“该玩家自己发
`CM_DEAL*`”时；被邀请方在收到 A 的 TRY 之前不会有条目。

---

## 2. 一次完整交易的写回面（A 发起 → B 接受 → 改价 → 成交）

记 A = 发起方（`CM_DEALTRY` 发令者），B = 被邀请方，双方**此前从未有过任何交易命令**
（进程新起后的首次）。

### 步骤 0：前置
- A、B 均有 `ground_sessions`/`online`；`trade_sessions` 里 **A、B 都没有条目**。

### 步骤 1：A 发 `CM_DEALTRY`
- `handle_deal_cmd(A)`：`me_tr = remove(A)` → `None` → **default**；`other_tr = remove(B)` → `None`。
- `deal_try(me=A, other=dummy_tr)`：`faces_me` 通过、`other.allow_deal/is_can_deal` 取 **dummy 的
  default（都是 true）** ⇒ 成功：
  - `open_deal_dlg(me)`：A.deal_creature=Some(B)、A.dealing=true；发 `SM_DEALMENU` 给 A；
  - `open_deal_dlg(other)`：**写在 dummy_tr 上**（B.deal_creature=A、B.dealing=true）；发
    `SM_DEALMENU` 给 B（帧经 `online` 路由，正常到达）。
- 回写（~8430/8435）：`insert(A, me_tr)` ✅；对手分支 `other_tr == None` ⇒ **B 的条目被丢弃**
  ❌。⇒ **B 收到了开窗帧，但服务端没有 B 的任何交易状态**。

### 步骤 2：B「接受」并操作（客户端只收到 `SM_DEALMENU`，无需额外收包；下一步就是放物品/改价）
- B 发 `CM_DEALADDITEM` → `handle_deal_cmd(B)`：`me_tr = remove(B)` → `None` → default
  （`deal_creature: None`）。`ident != DEALTRY` ⇒ `other_id = me.deal_creature = None` ⇒
  `other_chr = None`；`deal_add_item` 的第一道门 `me.state.deal_creature.is_none() ⇒ return`
  ⇒ **零帧**；回写 `insert(B, default)` ⇒ B 此时才获得一个**与交易无关的**空条目。
- 同理 B 的 `CM_DEALCHGGOLD` / `CM_DEALDELITEM` / `CM_DEALCANCEL` / `CM_DEALEND` **全部无效果**
  （每一条都以 `me.deal_creature` 为门）。

### 步骤 3：A 侧操作仍可用
- A 的 `CM_DEALADDITEM`/`CHGGOLD`：`me.deal_creature = Some(B)` ⇒ 状态机正常；给 B 的「远端帧」
  经 `online` 路由（不依赖 B 的条目）。
- A 的 `CM_DEALCANCEL`：正常取消 A 侧；对手是 dummy（或 B 后建的空条目）⇒ 只向 B 发
  `SM_DEALCANCEL` 帧。

### 步骤 4：成交（`CM_DEALEND`）
- `deal_end` 开头 `me.state.deal_success = true;`，随后 `if me.state.deal_creature.is_none() return;`
  ⇒ **B 的 END 只把自己标 success，不参与交换**（B 的 `deal_creature` 恒 None）。
- A 的 END：`other` = B 的（后建）条目；B 的 `deal_success=true` 后 A 再 END 才可能进入交换，
  但 B 的 `deal_item_list` 永远是空（B 无法放物品）、B 侧 `dealing` 也从未为 true ⇒ 交换面
  只剩 A 侧物品，且「双方锁定」语义失真。

⇒ **B（被邀请方）在成交前后都不会落地任何交易状态**。首次交易在 B 侧完全不可用。

---

## 3. 结论

**`缺陷`**。

- 触发面：任何**进程内首次**作为“被邀请方”的玩家（其 `trade_sessions` 无条目）。
- 症状：B 能看到开窗帧，但放物品/改价/取消/确认全部无效果（服务端零帧、无状态）；
  B 的 `DealState` 永远不会携带 `deal_creature`。
- 为何未被发现：既有测试在夹具里**预置双方条目**（见 §1 证据）；A35-b 的跨区前取消路径
  （`cancel_cross_zone_trade_and_selloff`）会 `remove` 玩家条目，跨区后再交易也回到“无条目”态。
- 非本次 A34 片 3 引入：`lib.rs` 的两处 insert 条件在 A34 片 3 之前即如此（片 3 只把该函数接进
  区 k，未改回写逻辑）。

---

## 4. 修法建议（**先不动手**，待裁决）

### F1（推荐，最小、局部）
`handle_deal_cmd` 回写尾部：对手存在（`other_chr: Some`）时**总是**回写对手条目——
`trade_sessions.insert(c, other_tr.unwrap_or_default())`（或先 `or_default` 再走统一回写）。
- 语义：与 oracle「`OpenDealDlg(target)` 直接改 target 的 PlayObject 字段」同形；条目是**瞬态**
  内存态（不落库），无回滚语义影响。
- 风险：为每个参与交易者建一条 `DealState::default()`；无跨区影响（A35-b 仍先取消后迁移）。
- 需要牙（主机级、区 0 与区 k 各一条，**不预置 B 的条目**）：
  1) A `TRY` → **B** `ADDITEM` ⇒ B 收 `SM_DEALADDITEM_OK`、`trade_sessions["B"].deal_creature == Some(A)`；
  B `END`（两次 END 后）⇒ 双方 `SM_DEALSUCCESS`、dealing 复位；
  2) 变异：把 8435 改回 `if let (Some, Some)` ⇒ 用例 1 红。
  （区 k 版走 A34 片 3 的 `handle_zone_command` 路径。）

### F2（可选补充，生命周期对齐 + 止漏）
1. `finish_login`/`finish_login_on_zone_state` 为每个会话建 `DealState::default()`（oracle
   `PlayObject` ctor 字段恒存在）；区 k 迁移 attach 时同点补建（A35-b 不搬交易表，目标区需新条目）。
2. `close_cleanup_residue` 增加 `trade_sessions.remove(chr_name)`（当前登出后条目残留，见 §5 观察 2）。
- 影响：改动登录热路径与断线清理；牙：登录即有条目（正例）+ 断线后条目清空（正例）。
- 若时间紧，**F1 已足以修正确性**；F2 建议随后单独一张小卡（清理+生命周期对齐）。

---

## 5. 附带观察（非本卡结论，供裁决）

1. **对手 `ground_sessions` 缺席**：`other_sg = None` 时同样用 dummy ground（空背包/金币 0），
   回写也只在 `Some` 时发生——与交易条目同形，但对手若在线则必有 ground（登录即建），
   实际风险低于 trade 表。
2. **`close_cleanup_residue` 不清 `trade_sessions`**（`mir2-rs/bins/game-svr/src/lib.rs` `close_cleanup_residue`）：断线后条目残留，
   且若残留 `dealing=true`，`drain_deal_tick` 会尝试 `deal_cancel_between`（对已下线者）；
   条目不落库、无数据损坏，但属**内存残留 + 潜在迟到取消帧**，建议随 F2 一并清理。
3. `drain_deal_tick` 的自动取消对“只读表”工作正常，但它同样要求 `trade_sessions` 侧先有
   `dealing=true` 的条目——与 §2 的缺陷相互印证（B 侧永远进不了该分支）。
