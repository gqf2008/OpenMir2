# 玩法缺口表：物品（item parity gap）

> 卡：`card/parity-gap-survey`（只读分析）。本文件**先出物品一块**，任务与 NPC 脚本按 opc-07 要求后续追加。
> 口径基准时间：2026-11-19 21:5x（+0800），仓库 HEAD 见文末"计数时刻"。

## 1. 分母定义（定下后不再改；要改必须写明改前/改后/原因）

| 角色 | 定义 | 可复跑命令 | 本次结果 |
| --- | --- | --- | --- |
| **主分母** | **物品使用分派里的分支数**：C# 玩家物品使用入口（`UseItem`/吃/穿/用路径）按 `StdMode`（或等价分派）展开的 `case`/`if` 分支集合，逐条记 `文件:行` | 见 §4"定位状态"（尚未钉死，见风险说明） | **待补** |
| 旁证① | 代码里按物品类型分叉的**位点数**（含非使用路径，如商店/机器人） | `grep -rn 'StdMode ==\|Shape ==' --include=*.cs src/ \| wc -l` | **111** |
| 旁证② | **数据侧** `StdItems` 行数（`sql/mir2_db.sql`） | `grep -c 'INSERT INTO .*StdItems' sql/mir2_db.sql` 或直接读建库脚本 | 待数 |

**为什么主分母不用 111**：111 是"代码位点"，同一行为可被拆成多处判断（重复计数），也可能一处判断覆盖多种物品（漏计）；
它只作旁证，并用于**核对主分母是否有明显遗漏**（主分母应 ≤ 111）。

## 2. 状态口径（照 opc-07 章程 §5）
- `verified`：必须在 `mir2-rs` 里有**对拍测试名**可指；本表出现的每一条 `verified` 都要写在"测试名"列；
- `implemented（未验证）`：Rust 有实现但没有对拍测试；
- `missing`：C# 有、Rust 无。

## 3. 主分母的定位状态（为什么现在还填不了）
已排除的候选（附证据）：
- `src/M2Server/Player/PlayObject.Base.cs:1758` 的 `switch (clientItem.Item.Need)` —— 是**装备需求**分派（case 0/1/10/… 为需求类型），**不是**物品使用行为；
- `src/GameSrv/Npc/Merchant.cs:421` 与 `:1146` —— 商店买卖/收购的 `StdMode` 分派；
- `src/BotSrv/BotHelper.cs:1053`、`src/BotSrv/MShare.cs:2374`、`src/BotSrv/Player/RobotPlayer.cs:3563` —— 机器人侧，非玩家使用入口。

剩余可能落点（下一步逐文件核）：`src/M2Server/Player/PlayObject.Base.cs` 的 `UseItem` 调用链、`src/M2Server/Player/CharacterObject.cs`、`src/GameSrv/Player/*`。
**在钉死落点前，主分母不填数**——避免拿"看起来能数"的 111 冒充分母。

## 4. 计数时刻
- 仓库 HEAD：`a5643481a`（master）；命令执行时间：2026-11-19 21:5x（+0800）；
- 旁证①命令原文与输出：
```
$ grep -rn 'StdMode ==\|Shape ==' --include=*.cs src/ | wc -l
111
```

## 5. 结论（物品块，暂定）
离"能玩"还差的是：**先把主分母钉死**（玩家物品使用入口的分派集合），再逐条对 `mir2-rs` 标 `missing / implemented（未验证） / verified`。
在此之前不能给覆盖率——111 只是旁证，不是分母。
