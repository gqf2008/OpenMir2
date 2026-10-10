
---

## B2 进展（2026-10-10 第三轮）

### 用例结果：**2/4 逐行一致**

| 用例 | 结果 |
| --- | --- |
| 传送-幻境（金币不足/无物品） | ✅ 一致（含 `[脚本错误] …MobFireburn…` 原文） |
| 传送-幻境（背包有名为 100000 的物品） | ✅ 一致（`flag 0 100000` + `#gold` + `#items`） |
| 商店-比奇国王@main | ❌ C# 侧 `EX NullReferenceException`（栈顶：`ConditionOfCheckSlaveListCount` @ConditionProcessingSys.cs:224 —— 我的 mock 未提供从属列表）；Rust 侧该条件走显式 NotImplemented |
| 商店-比奇国王@rw03 | ❌ C# 侧 `EX IndexOutOfRangeException`（S2 已接管的 `ConditionOfCheckRangeMonCount` 崩溃） |

### 本轮实现（Rust 侧）

- **`ActionOfSet`**（语料第一高频动作；`take`/`SET` 的实际落点）：
  `StrToInt(sParam1,0)` / `StrToInt(sParam2,0)` → `SetQuestFlagStatus(flag, value)`，与 C# 逐行等价；
- 同族 `ActionOfReSet` / `ActionOfSetOpen` / `ActionOfSetUnit` 一并移植；
- `ScriptPlayer` 增 `set_quest_flag_status` / `set_quest_unit_open_status`；两侧 mock 都实现
  **C# `PlayObject.QuestFlag[]` 的位语义**（`flag-1 → byte_idx = idx/8, bit = 128 >> idx%8`，
  value==0 清位、非 0 置位；flag<=0 直接返回），并输出规范日志行 `flag <flag> <value>`。
- C# 侧 harness：崩溃时除 `EX <异常名>` 外打印**栈顶 4 帧到 stderr**（定位 mock 缺口用，
  不参与 stdout diff）。

### 下一步（按卡片的顺序）

1. **`ConditionOfCheckSlaveListCount`**（语料 354 次，当前 #1 条件；也是商店用例红的原因）：
   需要在 `ScriptPlayer` 增从属列表接口（C# `playerActor.SlaveList`/等价物），两侧 mock 同步，
   再按 C# 语义实现计数比较；实现后商店@main 用例应转绿。
2. `ActionOfExeaction` / `ActionOfMovData` / `GotoLableTakeItem` / `GotoLableTakeWItem` /
   `ActionOfGiveItem`（给物/扣物路径，需要物品层：名字→索引→背包变更 → 两侧 `additem/delitem` 日志）。
3. `ConditionOfCheckItem`、`ConditionOfCheckRangeMonCount`（后者等 S2 的崩溃处置结论后再定：
   按同条件复现崩溃，或登记白名单）。
4. 六类流程用例补齐（修理/仓库/行会/任务各 ≥1），每批跑 `flow-diff.py` 并回写本文件。
