# openmir2-attack-yedo-batch-b-followup-stale-lvexp

> 状态（2026-11-20 立卡，来源 = 攻击批 B 段 2 的 `3b` 复核）：**待认领**。
> 本卡只覆盖一件事：**`UpdateDelayMsg` 留下的"陈旧 640 条"到点仍会发帧**，目前**没有帧面用例覆盖**。

## 0. 现象（oracle 事实，非本仓缺陷）

`BaseObject.UpdateDelayMsg`（`src/M2Server/Actor/BaseObject.Message.cs:257-278`）的删除段是
**i 循环**：`i` 每轮自增、摘一条 `Count` 减一、`Count <= i` 提前退出 ⇒ 队首连续 N 条同键时
**只摘 `ceil(N/2)`，留 `floor(N/2)` 条陈旧条目**。留下的陈旧条**不会被清掉**，到 `DeliveryTime`
仍经 `RM_MAGIC_LVEXP` 投递 → `SM_MAGIC_LVEXP(640)`，携带**旧的** `Level`/`TranPoint`。

⇒ 帧面结果：一次升级命中若前面已累积 2 条同键未升级条，则除新的 800ms 帧外，**还会多出一条
旧时刻、旧值的 640**（本仓已按此复刻，见 `crates/world-bevy/src/slave.rs` 的
`remove_head_matching`；段 2 的两条 N=2 用例只验了**队列内容**，没有验**到点投递出的帧**）。

## 1. 与旧实现的差异

旧实现（`while head matches { dequeue }`，即"摘光"）会把陈旧条全部清掉 ⇒ **永远少这一帧**。
因此本卡要钉的正是"新旧实现在帧面的唯一差异"。

## 2. 复现所需调用序列

1. 构造一次训练态（`World::set_attack_skill_train`），`max_train[0]` 很大 ⇒ 前两次命中**未升级**，
   各追加一条 3000ms 的 `RM_MAGIC_LVEXP`（同 `(ident, MagicId)`）；
2. 改快照使第三次命中**升级** ⇒ 走 `UpdateDelayMsg`（摘 `ceil(2/2)=1` 条队首 + 入 800ms 新条）；
3. 把 `world.clock_ms` 推到**第一条陈旧条的 `deliver_at_ms`**，调 `World::deliver_self_delay_msgs()`；
4. 断言：**投递出一条 640**，且其载荷是**陈旧条的旧 `Level`/`TranPoint`**（不是新条的值）。

## 3. 验收条件

- 有帧：该时刻**恰一条** `SM_MAGIC_LVEXP(640)` 被发往**施法者自己的 socket**（旁观者零帧）；
- 值仍旧：`Recog=MagicId`、`Param=Level(旧)`、`Tag=TranPoint(旧)`；
- 反向牙：把 `remove_head_matching` 改回"摘光" ⇒ 该用例**必红**（旧实现下这一帧不存在）。

## 4. 边界

不改生产行为（本仓已与 oracle 一致）；只补**帧面用例**。若认为"陈旧帧"应当被抑制，那是偏离
oracle 的决策，必须另有裁决记录，不能在本卡里顺手改。
