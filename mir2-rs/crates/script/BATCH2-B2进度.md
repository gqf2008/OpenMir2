
---

## B2 进度与效果对拍机制（2026-10-10 更新）

### 效果对拍：两侧同脚本同初始状态 → 同一份效果日志

- **C# 侧**（`mir2-rs/tools/script-parity-cs run`，只读引用参照实现）：
  ```powershell
  dotnet run --project mir2-rs/tools/script-parity-cs -- run <脚本文件> <label> `
      --gold 200000 --level 10 --items "金币:1:1" --player-items "金币:5"
  ```
  输出规范行：`msg <ident> <wParam> <nParam1..3> <文本>` / `gold <delta>` / `additem 名 件数` /
  `delitem 名 件数` / `err <C# 错误原文>`，末尾附 `#gold=` `#items=` 状态行。
  物品表由夹具给定（内存 `IItemSystem`），玩家/NPC 为 DispatchProxy mock，脚本经真实
  `ScriptParsers` + `ScriptEngine` 执行。
- **Rust 侧**：`Engine` + `ScriptPlayer`/`ScriptNpc` mock，产出同样的规范行（`flow-run` 命令
  为下一步待补；当前以 `engine_tests` 内的 mock 逐步对齐）。

### 已实现（Rust 侧，随语料落点）

| 类别 | 已实现 |
| --- | --- |
| 控制流 | `GotoLable` 全流程、`break`、`goto`（位移后落到 EndQuest）、`GoQuest` |
| 条件处理器 | CHECK(注册键 1) / RANDOM(2) / CHECKLEVEL(7) / CHECKGOLD(12) / EQUAL(25) / LAPGE(26) / SMALL(27) |
| 条件 switch | CHECKITEMW（`CheckGotoLableItemW`：佩戴位前缀比较 + 背包件数），其余显式报未实现 |
| 动作处理器 | Close(→10127) / MessageBox(→10309) / MobFireBurn（参数不全时 1:1 复刻 `[脚本错误]` 文本） / ResetUnit / Set（待补变量写入） |
| 动作 switch | Break / Param1-4 / Map / MapMove / PlayDice / GoQuest / EndQuest / Goto / AddBatch 系 / Take、Takew（显式报未实现） |

### 六类流程实测要点（数据驱动，来自 `tools/flow-commands.py`）

- 传送流程（`Market_Def/传送员/比奇省传送员-0.txt`）实测：`checkgold 100000` 在位移语义下
  → CmdCode 11 → **CHECKITEMW**（查名为 "100000" 的物品）⇒ 金币检查实际不生效，传送条件
  恒假、走 `#elseact` 的 `messagebox`（→ CmdCode 233 → `ActionOfMobFireBurn` → 参数不全
  → `[脚本错误]`）。C# 侧 `run` 模式实测输出与上述完全一致（gold 不变、无物品变化、一条脚本错误）。
- 各流程命令集（次数排序）：
  - 商店：BREAK / GMEXECUTE / SENDMSG / GAMEGOLD / DELAYCALL / MESSAGEBOX / GIVE / TAKE / CHECKITEM / GOTO / MAPMOVE
  - 传送：BREAK / MAPMOVE / MESSAGEBOX / CHECKGOLD / TAKE / SET / MAP / CHECKLEVEL
  - 修理：BREAK / TAKE / CHECKITEM / DELAYCALL / GIVE / GOTO / MESSAGEBOX / CHECKGOLD / CHECKITEMADDVALUE / UPGRADEITEMEX
  - 仓库：TAKE / CHECKITEM / GIVE / CHECKGOLD / MESSAGEBOX / GOTO / BREAK
  - 行会：BREAK / GMEXECUTE / MOVR / MESSAGEBOX / GOTO / LARGE / TAKE / SMALL / CHECKGAMEGIRD
  - 任务：BREAK / SENDMSG / GMEXECUTE / DELAYCALL / MESSAGEBOX / GAMEGOLD / GIVE / MAPMOVE / CHECKLEVELEX / MOV

### 下一步（按此顺序）

1. Rust 侧 `flow-run` 子命令（与 C# 侧同参数、同日志格式）+ 两侧日志 diff 工具；
2. 按六类流程命令集补齐处理器：TAKE/GIVE（物品路径）、MAPMOVE/MAP（传送）、SET/MOV/MOVR（变量）、
   GMEXECUTE/SENDMSG/DELAYCALL、CHECKITEM/CHECKITEMADDVALUE/UPGRADEITEMEX、CHECKLEVELEX、
   CHECKNAMELIST/ADDNAMELIST、CHECKGAMEGOLD/GAMEGOLD 等；
3. 六类关键 NPC 各 ≥1 用例（同脚本同初始状态，日志逐行一致）；
4. 未实现清单（93 个派发目标）压到 0，并把 `unimplemented_handlers()` 作为门禁断言进测试。
