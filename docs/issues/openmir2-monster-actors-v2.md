# openmir2-monster-actors-v2 —— 非玩家对象"效果面"（主动怪物）装置与对拍

> 立卡：2026-11-20（opc-19 裁定）。v1（被动怪物，只记 RM）见 `openmir2-magic-parity.md` §36。

## 为什么 v1 不够

v1 只把怪物放进世界参与**对象选择**，不处理它们收到的消息（不 `Operate`）。而 oracle 里大量行为是
**目标自己 Operate 时产生**的：`RM_MAKEHOLYSEIZEMODE`（16 的冻结状态）、受击（31/53/656）、
诱捕（`MagTamming` 的 Master/几率分支）、`MagTurnUndead` 的 `SuperMan || LifeAttrib != LA_UNDEAD` 等。
⇒ 这些分支在 v1 下只能判"门"，不能判"效果"，不得升格 `verified`（§26）。

## 要改什么

1. **`IRecordingActor` 抽象**：把 `DrainRaw/OperatePublic/PushBack/ActorId` 从 `RecordingPlayObject`
   抽成接口，`RecordingMonster` 实现同一套（怪物侧 `Operate` 走 `MonsterObject` 的 Message 分部）。
2. **记录面扩展**：怪物出帧（它们的 `GateIdx`/socket 记录）进 golden `Receivers.mon{i}.Sent`；
   比较器按与玩家同口径逐字段比（含 RNG 逐值、RM 多重集）。
3. **装置输入**：`Monsters[].life_attrib`/`master`/`target_cret` 已由 v1 落；v2 追加
   `holy_mode_after`/`poison`/`buff` 等效果读数（与 `status_time_arr` 同形式的"术前/术后"）。

## 它解锁哪些条目

- **16 `SKILL_HOLYSHIELD`**：`RM_MAKEHOLYSEIZEMODE` 的效果（冻结状态置位）→ 16 才可升格；
- **5/11/13/45/44** 的**怪物 target 分支效果**（卡 §35 的 7 条范围限定里 5 条）；
- 后续未升格条目：`SKILL_TAMMING`（诱捕的 Master/几率/undead 分支）、`MagTurnUndead`、
  `MabMabe`（`Race == Play` 两支）、`SKILL_GROUPLIGHTENING(42)`/`SKILL_GROUPAMYOUNSUL(48)`/
  `SKILL_GROUPDEDING(39)` 的动物分支。

## 验收

- 怪物 Operate 后的帧/RM/效果读数与 oracle golden 逐值一致；
- 负对照：把某条效果门改成恒真/恒假 ⇒ 对应支必红；
- 既有 18 件装置输出零变化（`Monsters[]` 缺省）。
