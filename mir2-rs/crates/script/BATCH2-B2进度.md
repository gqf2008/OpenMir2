
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

---

## B2 进展（2026-10-10 第五轮）

### ① parser.rs 位移翻转：后果实测（未落地，等前置）

指令要求"Rust 位移同批翻转（code-1→code）并期望 flow-diff 3 一致/0 不一致"。实测（临时翻转 → 量数 → 复原）：

| 门禁 | 翻转后 | 复原后 |
| --- | --- | --- |
| 解析对拍（验收①） | DIFF 结构摘要不一致: 115 个文件（cmd_code 参与指纹） | == 全部一致 == |
| flow-diff | 0 一致 / 4 不一致（含豁免用例 EXEMPT-DRIFT） | 3 一致 / 0 不一致 / 1 豁免 |

根因：C# 参照 `src/Modules/ScriptEngine/ScriptParsers.cs:329/501` **仍是 `nCMDCode = code - 1`**（该文件无新提交）。只翻 Rust ⟹ 落点与 C# 全部错位，两条门禁同时变红。

**前置条件**：该翻转必须与"oracle 侧同批修复"一起做（S1/S2 把 `-1` 修掉并登记 T-* 条目），届时本线执行：

```powershell
# crates/script/src/parser.rs 两处：
#   n_cmd_code = def.field_index - 1;  →  n_cmd_code = def.field_index;
# 并更新 whitelist B-8 口径（复刻 → 已按 oracle 修复对齐），重跑解析对拍 + flow-diff
```

在此之前保持"复刻位移"口径（上一张卡片"复刻 bug 沿用白名单口径"的要求），以维持验收① 与 flow-diff 全绿。

### ② 本轮新增 handler

- `ConditionOfCheckItem`（语料 36 次）：`QuestCheckItem` 计数 < `nParam2` → false；按背包件数实现，佩戴位/耐久统计范围待与 C# `PlayObject.QuestCheckItem` 核对（已写入代码注释，差异会由 flow-diff 暴露）。
- 豁免用例 `rust_expect` 快照同步更新（落点少一行 NotImplemented）——快照机制按设计生效。

### 当前门禁

- 解析对拍：639/639 结构指纹 + 原文指纹一致；427 错误逐行多重集差 0/0；
- flow-diff：3 一致 / 0 不一致 / 1 登记豁免；
- workspace 114 测试全过、fmt ✓、clippy -D warnings 0。
