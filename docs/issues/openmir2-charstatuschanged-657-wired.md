# openmir2-charstatuschanged-657-wired —— `RM_CHARSTATUSCHANGED(10139)` → `SM_CHARSTATUSCHANGED(657)` 接线

> 状态：**已实现，待 QA**（2026-11-20，`card/charstatus-657`）。上游卡：`openmir2-charstatuschanged-gap`
> （43 登记，含产生点、丢弃点、blast radius）；本卡只做接线与证据，不改清单（`parity-gap-items.json`
> 由 43 统一维护）。

## 1. oracle 口径（照抄）

- 产生点（同 ident 两处）：`RecalcAbilitys()` 尾部 `SendRefMsg(RM_CHARSTATUSCHANGED, 0, CharStatus, 0, 0, "")`
  （`BaseObject.cs:1516`）；去重变体 `SendUpdateMsg(...)`（`PlayObject.cs:2876`，按 ident 先删旧条目）。
- 出队：`PlayObject.Message.cs:1845-1847` 的
  `SendDefMessage(SM_CHARSTATUSCHANGED, processMsg.ActorId, LoWord(nParam1), HiWord(nParam1), this.HitSpeed)`。
- 泵序键：`10139 & 0xFF = 155` ⇒ `RM_WEIGHTCHANGED(131) < RM_CHARSTATUSCHANGED(155) < RM_SYSMESSAGE(252)`。

## 2. 实现

- `pickwire::notice_frame` 增 `RM_CHARSTATUSCHANGED` 臂（与其它 `SM_*` 同一条路，不另开通道）：
  `Recog = session.actor_id`（= `processMsg.ActorId`，自通知的发送者）、`Param/Tag = LoWord/HiWord(nParam1)`、
  `Series = session.hit_speed`（`this` = 接收者 = 发送者 ⇒ **本会话**的 `HitSpeed`）。
- `pickwire::WireSession` 增 `hit_speed: u16`（宿主两处构造点传 `sg.hit_speed`）。
- `handle_eat_cmd` 泵的 `None` 支改为通用兜底（不再是「StdMode 1 的 10139/8102 未建模」——两者均已映射）。
- **观察者副本（新调用点）**：`handle_eat_cmd` 在排水之后，若本批确实产生过
  `RM_CHARSTATUSCHANGED`，调一次 `broadcast_charstatuschanged_except_self`（含 `SendRefMsg` 的
  500ms 缓存口径）。一次 `SendRefMsg` = 一次自通知 + 一次广播。

## 3. 自通知与广播不重复（依据 + 用例）

- 自通知的收件人 = 本会话 socket（泵只 `tx.send` 给该会话）；
- 广播 `broadcast_charstatuschanged_except_self` 循环里显式 `if target_id == actor_id { continue; }`
  ⇒ 观察者集合按构造不含发送者。两者收件人集合**不相交**。
- 用例 `charstatuschanged_self_and_broadcast_do_not_duplicate` 用三个**独立 fixture** 跑三种配置：
  只自通知（自己 1、观察者 0）/ 只广播（自己 0、+5/+12 各 1、+13 零帧）/ 两者都开（901–904 各**恰好 1**）。
- 用例 `eat_path_charstatuschanged_hits_self_and_observers_exactly_once` **单列吃路径**（新调用点）：
  StdMode 1 食物跨档触发，断言自己 1、+5/+11/+12 各 1、+13 零帧——没有任何 socket 收到 2 条。

## 4. 用例与牙

- `charstatuschanged_maps_to_sm_657_with_recipient_hit_speed`：`Recog/LoWord/HiWord/Series` 逐字段。
- `charstatuschanged_orders_between_weightchanged_and_sysmessage`：三条同批乱序入队，按
  `sort_by_key(|n| n.ident as u8)` 排序后逐条出帧 ident = `622 → 657 → 100`。
- 牙：把 `notice_frame` 的 10139 臂删掉（回退到 `None`）⇒ 两条用例红；把 `Series` 改成 0 ⇒ 首条红。

## 5. 口径说明（不隐藏）

- 吃路径两条都已接（自通知 + 观察者）。广播**复用** `broadcast_charstatuschanged_except_self`
  （`SendRefMsg` 的 500ms 缓存口径也在内），不自造第二条通道。
- `SendUpdateMsg` 的**去重**只作用于发送者自己的队列（oracle 按 ident 先删旧条目）；观察者侧
  没有去重语义。本卡不改变这一点，只保证"一次 `SendRefMsg` = 自己一条 + 观察者各一条"。
- 泵序用例的说明：真实泵是堆出队（`NetPriorityQueue`，章程 §29）；该用例用 `sort_by_key`
  近似**仅因三条低字节互不相同（131/155/252）**，等键场景不得照抄。

## 6. IT-16 复评条件

本卡落地后，IT-16（彩票）的升格障碍减少一条（`RM_CHARSTATUSCHANGED` 不再被丢弃）。**复评还需**：

1. `IncGold` 失败支（`Gold + nGold > HumanMaxGold`）需要 oracle 侧证据（当前夹具无该用例）；
2. 43 的清单把 IT-16 的 `missing` 状态改成与实测一致。
