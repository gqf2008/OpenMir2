
---

## B2 进展（2026-10-10 第二轮）

### 两侧效果对拍已跑通

- **Rust 侧 `flow-run`**（`crates/script-tool/src/flowrun.rs`）与 **C# 侧 `run`**（`tools/script-parity-cs/RunMode.cs`）
  同参数、同日志格式；`tools/flow-diff.py` 逐例两侧跑并 diff（用例表 `tests/parity/flow-cases.json`）。
- 命令：`python -I mir2-rs/tools/flow-diff.py mir2-rs/tests/parity/flow-cases.json`

### 本轮实测结论

| 用例 | 结果 | 说明 |
| --- | --- | --- |
| 传送-幻境（金币不足/无物品） | ✅ **逐行一致** | 两侧均为：`err [脚本错误]  脚本命令:MobFireburn ... 参数1:幻境进入条件：10万金币 ...` + `#gold=200000` + `#items=` |
| 传送-幻境（背包有名为 100000 的物品） | ❌ 差 1 行 | Rust 多一条 `NotImplemented ActionOfSet`（即该分支第一个动作 `take` 落到 ActionOfSet，尚未实现；C# 侧执行但无可观测副作用） |
| 商店-比奇国王 @rw03 | ❌ | **C# 参照自身崩溃**：`EX IndexOutOfRangeException`（`ConditionProcessingSys.ConditionOfCheckRangeMonCount` 内 `String.get_Chars` 越界，由夹具数据触发）；Rust 侧报 NotImplemented 清单 |

### 本轮修出的语义缺陷（已修）

- **动作循环的"首个已注册动作后 return"**：C# `GotoLableQuestActionProcess` 里
  `if (ExecutionProcessing.IsRegister(cmd)) { Execute(...); return result; }` ⇒ 动作列表在第一个已注册
  动作后**整体停止**（后续动作不执行）。Rust 初版是 `continue`，导致 `messagebox` 之后还跑了 `break`
  （多出一条 `unitstatus 0 0`）。已按 C# 修正并复跑验证（传送用例由 DIFF 转 OK）。
- C# 侧 harness mock 补齐：`UseItems` 12 个空佩戴位（否则 `close`→TakeW 路径 NRE）、
  `SetQuestUnitStatus` 纳入日志、字节转换 clamp、参照崩溃时输出已收集日志 + `EX <异常名>` 行。

### 下一步（同前，按语料频次）

1. `ActionOfSet`（语料第一高频：`take`/`SET` 等的实际落点）——需要变量写入层
   （`SetMovDataValNameValue` 系）与两侧一致的 `setvar/flag` 日志行；
2. `ActionOfExeaction`、`ActionOfMovData`、`GotoLableTakeItem/TakeWItem`、`ActionOfGiveItem`、
   `ConditionOfCheckItem`、`ConditionOfCheckRangeMonCount`（含参照崩溃条件的复现/登记）等；
3. 每补一批即跑 `flow-diff.py` 并更新本表；最终 93 个派发目标清零 + 六类用例全绿。
