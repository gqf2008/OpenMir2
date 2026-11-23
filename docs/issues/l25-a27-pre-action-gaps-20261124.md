# A27 前置动作缺口规格：`UPGRADEITEM` / `CLEARNEEDITEMS` / `CLEARMAKEITEMS` / `TAKECHECKITEM`（2026-11-24）

> **状态**：只读规格草案（分支 `card/a27-pre-spec`，父 `87afc3080`）。**不写码、不跑构建。**
> **上游**：`docs/issues/l25-a27-t157-frame-audit-20261124.md` §0「应发未发」与 §2 的 #7/#8/#9/#10
> 行（动作本身未实现，帧随动作）；`mir2-rs/tests/parity/whitelist.md` 的 T-157「仍缺 ①」四点；
> `mir2-rs/tests/parity/m3/audit-script-actions-20261018.md` 的 #74/#234/#235/#236 行。
> **锚点口径**：oracle `src/**` 是冻结面，可写行号；Rust `bins/game-svr/**` 与 `crates/world-bevy/**`
> 是高 churn，**只用符号锚**（prose 门对行号锚判红）。本文不含任何 `.rs` 行号锚。

## 0. 结论摘要

| # | 脚本 token（`ScriptDefName`） | 枚举值 | oracle 处理器 | 部署引用 | Rust 现状 | 建议 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `UPGRADEITEM` | 212 | `ActionOfUpgradeItems` | **0** | 有注册、未实现（`IMPLEMENTED_EXECUTIONS` 缺名） | **可做（P1，低成本）** |
| 2 | `CLEARNEEDITEMS` | 209 | `ActionOfClearNeedItems` | **0** | 同上 | 低优先（P2） |
| 3 | `CLEARMAKEITEMS` | 210 | `ActionOfClearMakeItems` | **0** | 同上 | 低优先（P2） |
| 4 | `TAKECHECKITEM` | 19 | `ScriptEngine` switch 分支（无独立处理器） | **0** | switch 分支显式 `NotImplemented`（`IMPLEMENTED_SWITCH_EXECUTIONS` 缺名） | **可不做（P3，需扩模型）** |

> **命名订正**：脚本 token 是 `UPGRADEITEM`（不是 `UPGRADEITEMS`），Ex 变体 token 是 `UPGRADEITEMEX`
> （C# 枚举名 `UpgradeItemSex`，不是 `UPGRADEITEMSEX`）。T-157 行与派工文里的写法按脚本字面应为前者；
> 建议下次触碰 T-157 时顺带订正（纯文档）。

四条 oracle 语义均已逐行核对，见 §1；部署零引用证据见 §3；切片与先决见 §4；牙草案见 §5；
是否值得做的判据见 §6。

## 1. oracle 语义（逐动作）

### 1.1 `UPGRADEITEM`(212) —— `ActionOfUpgradeItems`

原文 `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:3979` `ActionOfUpgradeItems`
（注册在 `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:87`；token 声明
`src/Modules/ScriptEngine/Consts/ExecutionCode.cs:431` `UpgradeItems`（脚本 token `UPGRADEITEM`））。

逐行语义：

1. 参数：`sParam1`→`nWhere`、`sParam2`→`nRate`、`sParam3`→`nPoint`，均 `HUtil32.StrToInt(..., -1)`。
2. 门：`nWhere < 0 || nWhere > playerActor.UseItems.Length || nRate < 0 || nPoint < 0 || nPoint > 255`
   ⇒ `ScriptActionError` 后 `return`（**无副作用、无帧**）。`ScriptActionError` 只写一行
   `[脚本错误] …` 到 `LogService.Error`（`src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:5987`
   `ScriptActionError`），不影响 `Success`、不终止动作列表。
   - **怪癖**：比较符是 `>` 而非 `>=`（`UseItems.Length == 13`）⇒ `nWhere == 13` 通过门后
     `UseItems[13]` 抛 `IndexOutOfRangeException`，异常穿出 `GotoLable`（Rust 侧现有等价物是
     `abort_goto` 旗，见 `whitelist` B-108）。规格建议：照抄为 `abort_goto`（零帧、后续动作不执行），
     而不是把门改成 `>=`。
3. 取件：`UseItems[nWhere]` + `GetStdItem`；`Index <= 0 || stdItem == null` ⇒ 红字
   `SysMsg("你身上没有戴指定物品!!!", Red, Hint)` 后 `return`（**这里也不抽 RNG**）。
4. RNG（固定 **3 次**，顺序敏感）：`nRate = Next(nRate)` → `nPoint = Next(nPoint)` →
   `nValType = Next(14)`。`Next(0)` 在 .NET 仍消耗一次采样并返回 0（Rust `DotNetRandom::next_max`
   与 `random_below` 同口径，已有 `next_max_zero_returns_zero` 牙）。
5. `nRate != 0` ⇒ 红字 `SysMsg("装备升级失败!!!")` 后 `return`（已抽 3 次、**零帧**）。
6. **死分支**：`if (nValType == 14)` 的 `DuraMax` 升级路径**永不命中**（`Next(14) ∈ [0,13]`）——
   oracle 怪癖，必须照抄（若实现成 `Next(15)` 就多出一个可达路径，属偏离）。
7. 命中路径：`nAddPoint = nPoint`，`Desc[nValType] + nAddPoint > 255` 时钳到 `255 - Desc[nValType]`，
   `Desc[nValType] += nAddPoint`（`nPoint == 0` 也会"成功"并出帧）。
8. 帧与文本：`playerActor.SendUpdateItem(userItem)`（`SM_UPDATEITEM(203)` 直发，
   `src/M2Server/Player/PlayObject.Base.cs:1668` `SendUpdateItem`）→ 绿字 `装备升级成功` → 蓝字
   `名称: Dura/DuraMax/Desc[0]/…/Desc[13]`（**所有分隔符都是 `/`**，与 Ex 变体的
   `Dura/DuraMax-Desc…` 不同，注意 `Desc[ItemAttr.WeaponUpgrade]` 是 `Desc[10]`）。

### 1.2 `CLEARNEEDITEMS`(209) —— `ActionOfClearNeedItems`

原文 `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:4239` `ActionOfClearNeedItems`
（注册 `:84`；token `src/Modules/ScriptEngine/Consts/ExecutionCode.cs:425` `ClearNeedItems`（脚本 token `CLEARNEEDITEMS`））。

1. 参数：`sParam1`→`nNeed`（`StrToInt(..., -1)`）；`nNeed < 0` ⇒ `ScriptActionError` 后 `return`。
2. 背包（尾→头，`for (i = Count-1; i >= 0; i--)`）：`GetStdItem(Index) != null && stdItem.Need == nNeed`
   ⇒ `SendDelItems(userItem)`（**每件一条 `SM_DELITEM(202)` 直发**，
   `src/M2Server/Player/PlayObject.Base.cs:1648` `SendDelItems`）→ `Dispose`（空操作）→ `RemoveAt(i)`。
3. 仓库 `StorageItemList`（同样尾→头、同样 `Need` 判据）：只 `RemoveAt`，**零帧**。
4. 无返回值、不改 `Success`；不碰佩戴位、不碰 `MakeIndex` 计数器。

### 1.3 `CLEARMAKEITEMS`(210) —— `ActionOfClearMakeItems`

原文 `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:4272` `ActionOfClearMakeItems`
（注册 `:85`；token `src/Modules/ScriptEngine/Consts/ExecutionCode.cs:427` `ClearMakeItems`（脚本 token `CLEARMAKEITEMS`））。

1. 参数：`sParam1`→`sItemName`、`nParam2`→`nMakeIndex`（**数值参数位**）、`sParam3 == "1"`→`boMatchName`
   （严格等值，`"01"`/`"true"` 都算 false）。`sItemName` 为空或 `nMakeIndex <= 0` ⇒ `ScriptActionError` 后 `return`。
2. 背包：逐件（尾→头）`MakeIndex != nMakeIndex` 跳过；命中后若 `!boMatchName || 名称 OrdinalIgnoreCase 相等`
   ⇒ `SendDelItems`（202 直发）+ `RemoveAt`。
3. 仓库循环**有 oracle 缺陷**：循环变量取自 `playerActor.ItemList[i]`（背包），却 `RemoveAt(i)` 到
   `StorageItemList` ⇒ 实际按背包下标判 `MakeIndex`、按同一下标删仓库件（可能越界/删错件）。
   规格要求：**先裁决**——照抄该缺陷（等价优先）或登记为新白名单条目；不得"顺手修好"。
4. 佩戴位：`for (i = 0; i < UseItems.Length; i++)`，`MakeIndex` 命中（+ 可选名称门）⇒ `Index = 0`，
   **不发帧**（`SendDelItems` 只出现在背包支）。

### 1.4 `TAKECHECKITEM`(19) —— `ScriptEngine` switch 分支

原文 `src/Modules/ScriptEngine/ScriptEngine.cs:904` `TakecheckItem`（**switch case，不是
`ExecutionProcessing` 注册动作** ⇒ 执行后**不**终止动作列表，与 209/210/212 的"执行后 return"不同；
token `src/Modules/ScriptEngine/Consts/ExecutionCode.cs:45` `TakecheckItem`（脚本 token `TAKECHECKITEM`））。

1. 语义：若 `gotoLabParams.UserItem != null` ⇒ `playerActor.QuestTakeCheckItem(UserItem)`；
   否则 `ScriptActionError(... TakecheckItem)`（只记日志）。
2. `gotoLabParams.UserItem` 的唯一来源是**同一次条件求值**设置的字段：
   - `CHECKITEMW`（`src/Modules/ScriptEngine/ScriptEngine.cs:559` `CHECKITEMW`）⇒ `CheckGotoLableItemW`
     （`src/Modules/ScriptEngine/ScriptEngine.cs:175` `CheckGotoLableItemW`）：
     `[NECKLACE]/[RING]/[ARMRING]/[WEAPON]/[HELMET]/[BUJUK]/[BELT]/[BOOTS]/[CHARM]` 前缀直接返回对应
     佩戴位物品（`[RING]`/`[ARMRING]` 先左后右、后写覆盖）；**其余名字**走
     `playerActor.CheckItemCount(sItemType, ref nCount)`（`src/M2Server/Player/PlayObject.cs:5332`
     `CheckItemCount`）——该方法**只扫 `UseItems`**（不是背包），且 `nCount < nParam` ⇒ `null`。
     - **现有偏差（另卡）**：Rust `CHECKITEMW`（`crates/script/src/engine/actions.rs` 的
       `"CHECKITEMW"` 臂）用 `ScriptPlayer::item_count`（会话背包件数）判，而 oracle 计数面是佩戴位
       ⇒ 名字型 `CHECKITEMW` 的判定在两侧不同。与 `TAKECHECKITEM` 同源，建议一并立卡。
   - `CHECKDURAEVA`（`src/Modules/ScriptEngine/ScriptEngine.cs:572` `CHECKDURAEVA`）⇒ `QuestCheckItem`
     （`src/M2Server/Player/PlayObject.Base.cs:2600` `QuestCheckItem`）：**背包**里同名件取 `Dura` 最大者
     （并列取先出现者），返回该件。
3. `QuestTakeCheckItem`（`src/M2Server/Player/PlayObject.Base.cs:2627` `QuestTakeCheckItem`）：先按
   **引用同一性**（`userItem == checkItem`）在 `ItemList` 找第一件 ⇒ `SendDelItems`（202 直发）→
   `RemoveAt` → 返回 true；否则在 `UseItems` 找同一引用 ⇒ `SendDelItems`（`Index = 0` **之前**发，
   帧带原物品）→ `Index = 0` → 返回 true；两处都没命中 ⇒ false（返回值被忽略，零副作用零帧）。
4. 关键点：`UserItem` 是**引用类型**，"同一个物品"是身份而不是名字 ⇒ 同名两件时只摘条件实际挑中的那件。

## 2. 当前 Rust 侧现状

### 2.1 派发表

- `crates/script/src/handlers.rs` 的注册表 `EXECUTION_HANDLERS` 有 `209 → ActionOfClearNeedItems`、
  `210 → ActionOfClearMakeItems`、`212 → ActionOfUpgradeItems`（与 C# `ProcessExecutionMessage` 一一对应）；
  `ENGINE_SWITCH_EXECUTIONS` 有 `19 → TakecheckItem`。
- `crates/script/src/engine/dispatch.rs` 的 `IMPLEMENTED_EXECUTIONS` **不含**上述三个处理器名；
  `IMPLEMENTED_SWITCH_EXECUTIONS` **不含** `TakecheckItem`。因此
  `execution_target_status(209/210/212) == Some(false)`、`switch_execution_status(19) == Some(false)`。

### 2.2 运行时行为（未实现时）

- 注册动作：`Engine::run_actions`（`crates/script/src/engine/mod.rs`）遇到"已注册但未实现" ⇒ push
  `EngineError::NotImplemented { kind: "action", … }` 并**立即 return**（复刻 C# `IsRegister ⇒
  Execute + return` 的"执行后终止本动作列表"语义）⇒ 该 `#ACT` 里它后面的动作都不会执行。
- switch 动作：`run_switch_action` 的 `"TakecheckItem"` 臂 push `NotImplemented` 后返回 `true`
  ⇒ 后续动作照常执行（与 oracle"此 case 后继续 switch 循环"一致，只是本 case 无行为）。
- **生产入口静默**：`bins/game-svr/src/npc.rs` 的 `goto_label` 构造 `Engine` 后**不读** `Engine::errors`
  （作用域结束即丢弃）⇒ 线上既无日志也无计数；`errors` 的消费者只有测试
  （`crates/script/tests/engine_tests.rs`）与 `crates/script-tool`（`flowrun`）。这也是"部署 0 引用、
  没人发现"的原因。

### 2.3 可复用的既有装置（A27-1/2/3 已合，父 `87afc3080` 含）

- 帧载荷：`ScriptItemFrames { added, removed, removed_use, updated }` + `emit_script_item_frames`
  （`bins/game-svr/src/lib.rs`，符号锚）——写回后按序直发 200/202/203。
- 写回：`write_back_player_bag` 已返回 `(warnings, added, removed)`（新增/摘除件，写回序），
  `write_back_player_use_items` 产出 `removed_use`/`updated`。
- `ScriptPlayer` 钩子：`send_del_use_item`、`send_update_item`（A27-2/3 起有生产实现，
  快照进 `NpcPlayer.del_use_items`/`update_use_items`）；`remove_item(name, count)` 会把背包净减
  反映进写回 diff ⇒ **自动**得到 202。
- 模型缺口（本规格的 P2/P3 成本来源）：`ScriptPlayer` **没有**背包枚举/仓库枚举/`StdItem.Need`
  访问器；`NpcPlayer.items` 是 `(名字, 件数)` 快照，**没有** `MakeIndex`/`Desc` 身份；
  `GotoLabParams` 只有 `item_name`/`send_say_msg`，**没有** `user_item`。

## 3. 部署脚本引用频次（证据）

口径：两个部署根各一份 `Envir`（`E:/MirServer/M2GameSvr/Envir` 与
`E:/OPC/teststack/rust-root/M2GameSvr/Envir`，各 **638** 个 `.txt`，GBK）。逐 token 全文件 grep
（含 `#IF`/`#ACT`/注释，宁宽勿窄；`UPGRADEITEM` 用词边界以区别于已实现的 `UPGRADEITEMEX`）：

```sh
cd E:/MirServer/M2GameSvr/Envir
grep -rniE '(^|[^A-Z])CLEARNEEDITEMS([^A-Z]|$)' . | wc -l   # 0
grep -rniE '(^|[^A-Z])CLEARMAKEITEMS([^A-Z]|$)' . | wc -l   # 0
grep -rniE '(^|[^A-Z])TAKECHECKITEM([^A-Z]|$)' .  | wc -l   # 0
grep -rniE '(^|[^A-Z])UPGRADEITEM([^A-Z]|$)' .    | wc -l   # 0（不误吃 UPGRADEITEMEX）
grep -rniE '(^|[^A-Z])UPGRADEITEMEX([^A-Z]|$)' .  | wc -l   # 34（已实现的 213，边界 sanity）
```

`E:/OPC/teststack/rust-root/M2GameSvr/Envir` 的同一组命令结果**逐项相同**（0/0/0/0/34）。

- 34 条 `UPGRADEITEMEX` 分布在 4 个文件（`Market_Def/功能NPC/勋章-0146.txt` 等），说明部署内容
  **用 Ex 变体、不用 212**。两个独立根一致 ⇒ 不是单棵树缺文件。
- 仓库内命中（`mir2-rs/tests/**`）只有测试夹具/审计文档本身（`m3/audit-script-actions-20261018.md`
  的 #74/#234/#235/#236 行已记"部署可达=0"），不是内容面。

## 4. 实现切片建议（按影响面 + 是否碰进程态分组）

### P1 `UPGRADEITEM`(212) —— 引擎内可完成，零新钩子（**推荐先做**）

- 影响面：本会话 `UseItems[nWhere]` 的 `Desc`（写回 `write_back_player_use_items` 落 `hum_items`）+
  `SendUpdateItem`（203 已有通道）+ 会话 RNG（3 次）+ 两条 `SysMsg`。
- 改动点：`IMPLEMENTED_EXECUTIONS` 加 `"ActionOfUpgradeItems"`；`actions.rs` 的 `run_action` 分派
  加同名匹配臂；实现 `action_of_upgrade_items`（照 §1.1，含 `nValType` 死分支与 `nWhere == len`
  的 `abort_goto` 复刻）。
- 先决：**无**（A27-3 已合，`send_update_item`/`set_use_item_desc` 等已在父提交里）。
- 风险：RNG 抽数顺序（3 次，含 `Next(0)` 也消耗）与失败支不发帧这两点最容易被"优化"掉，需牙钉住。

### P2 `CLEARNEEDITEMS`(209) / `CLEARMAKEITEMS`(210) —— 会话物品面 + 一条新宿主请求钩子

- 影响面：会话背包（202 帧）+ 仓库（**无帧**）+ 佩戴位（仅 210，无帧）+ `MakeIndex`/`StdItem.Need`
  只读。**不碰进程态**（模板表只读、不抽 RNG、不碰 `item_number`）。
- 建议形态（二选一，倾向 a）：
  - **(a) 请求 + 写回期执行**：`ScriptPlayer` 加两条 default no-op 钩子（如 `clear_items_by_need(need)`、
    `clear_items_by_make_index(name, make_index, match_name)`）；`NpcPlayer` 记入新字段；宿主写回期
    在 `write_back_player_bag` 的 diff **之后**执行请求、把摘掉的件 push 进同一个 `removed` 流
    （⇒ 202 直发**免费**），仓库/佩戴位在同处清理。语义差：动作效果延后到写回期，落在 T-158 同族
    "按动作序 vs 写回序"口径下（需在 T-158 扩注或新条目登记）。
  - **(b) 引擎内即时执行**：需要新枚举钩子（`names_with_need(need)`、`make_indexes()`）与
    `remove_item` 组合，能保证动作序，但模型接口面积更大、且 `remove_item` 按名计数无法表达
    "同名不同 MakeIndex"，实际仍要改模型。
- 先决：`StdItem.need`（`crates/data/src/models.rs` 已有 `pub need: u8`）；会话仓库列表的访问点
  （`bins/game-svr` 的 storage 面）；**裁决 §1.3 的仓库循环缺陷**照抄或登记。

### P3 `TAKECHECKITEM`(19) —— 需扩 goto_params 物品身份（成本最高）

- 影响面：条件求值期挑件（`CHECKITEMW`/`CHECKDURAEVA`）+ `gotoLabParams` 新字段 + 动作期按身份摘件
  （背包或佩戴位）+ 202 直发。
- 先决：① `GotoLabParams` 增加身份载体（如 `checked_item: Option<CheckedItem>`，含槽位/背包下标 +
  `make_index` 等足以唯一定位）；② `CHECKITEMW`/`CHECKDURAEVA` 改为**返回身份**并写入 goto_params；
  ③ 顺带修 §1.4 的 `CheckItemCount` 计数面偏差（另卡，与 TAKECHECKITEM 同源）；
  ④ 摘件的帧走既有写回机制（按身份从 `sg.state.item_list` 移除 + `removed` 流），佩戴位走
  `del_use_items` 快照。
- 该动作是 **switch** 动作（不终止动作列表），所以延迟执行会真正改变后续动作可见性 ⇒ 必须引擎内即时
  执行，不能像 P2 那样"请求 + 写回期"。

### 通用（三组共用）

- 不许顺手补帧：200/202/203 只能出现在 oracle 有 `SendAddItem`/`SendDelItems`/`SendUpdateItem` 的
  对应支；209/210 的仓库/佩戴位支、210 的 `Index = 0` 都是**零帧**。
- 实现后 `unimplemented_handlers()` 少三项；`implemented_registry_entries_are_actually_implemented`
  会用**空参数**把每个登记臂跑一遍（必须落进"脚本错误"路径，不得 panic）。
- `TakecheckItem` 的显式 `NotImplemented` 臂要删掉/改成真实现，并同步 `IMPLEMENTED_SWITCH_EXECUTIONS`。

## 5. 牙草案（每动作一条正例 + 一条边界负例；均需回答"空输入/缺参数"）

### P1 `UPGRADEITEM`

- **正例** `upgrade_item_success_updates_desc_and_emits_203`：`UseItems[1]` 有模板、`Desc[0]=0`；
  脚本 `#ACT` + `UPGRADEITEM 1 0 10`（`nRate=0` ⇒ 必成功分支；`nPoint=10` 但钳位按 `255-Desc`），
  固定 RNG 序列使 `Next(0)=0`、`Next(10)=k`、`Next(14)=v` ⇒ 断言 `Desc[v]` 增加 `k`、**不发** 202、
  恰好 1 条 `SM_UPDATEITEM(203)`（`Recog=ActorId`、尾 = `trade::update_item_tail`）、绿字 `装备升级成功`、
  蓝字分隔符是 `/`。RNG 抽数恰好 3 次（用带 trace 的 RNG 或计数器断言）。
- **负例 A（失败支）** `upgrade_item_fail_roll_emits_nothing`：同一夹具但 `Next(nRate) != 0` ⇒
  红字 `装备升级失败!!!`、`Desc` 不变、**零帧**、抽数仍 3 次。
- **负例 B（空输入）** `upgrade_item_bad_params_log_error_no_effect`：`UPGRADEITEM`（缺参）或
  `nPoint=256` ⇒ `ScriptActionError` 文本（`[脚本错误]` 前缀）入 `Engine::errors`、零帧、零状态变化、
  **零 RNG 抽数**。
- **负例 C（死分支）** `upgrade_item_valtype_never_14`：连续跑 N 次成功支（N > 14 的 `Next(14)` 取样）
  断言 `DuraMax` **从未**变化；变异方向：实现里把 `Next(14)` 改成 `Next(15)` ⇒ 本牙可能红（配合
  "DuraMax 只在…"的显式断言或定向 RNG 夹具）。
- 变异清单：改抽数顺序（先 `Next(nPoint)`）⇒ 正例红；失败支仍出 203 ⇒ 负例 A 红；把
  `nWhere > len` 改成 `>=` ⇒ `nWhere == 13` 用例（`abort_goto` 断言）红。

### P2 `CLEARNEEDITEMS`

- **正例** `clear_need_items_removes_bag_and_emits_202`：背包 3 件（Need=5 两件、Need=6 一件）+ 仓库
  1 件 Need=5；`CLEARNEEDITEMS 5` ⇒ 背包只剩 Need=6、**2 条** `SM_DELITEM(202)`（尾 = `trade::del_item_tail`）、
  仓库件消失且**无帧**、`item_number` 不动。
- **负例（空输入/无匹配）** `clear_need_items_invalid_or_no_match_no_effect`：`CLEARNEEDITEMS`（缺参）⇒
  `[脚本错误]` 入 errors、零帧、背包/仓库不变；`CLEARNEEDITEMS 99`（无匹配）⇒ 零帧、零变化。

### P2 `CLEARMAKEITEMS`

- **正例** `clear_make_items_by_make_index_and_name`：背包两件同名不同 `MakeIndex`；`CLEARMAKEITEMS 木剑 777 1`
  ⇒ 只删 777 那件、1 条 202；再验证 `CLEARMAKEITEMS 铁剑 778 1`（名字不符）⇒ 不删；
  `CLEARMAKEITEMS 任意 779 0`（`match_name` 关）⇒ 删 779 那件。
- **负例（空输入）** `clear_make_items_invalid_params_no_effect`：`CLEARMAKEITEMS 木剑 0 1`（`MakeIndex<=0`）
  或名字为空 ⇒ `[脚本错误]`、零帧、零变化。
- 佩戴位支：`UseItems` 里 `MakeIndex` 命中 ⇒ `Index = 0` 且**零帧**（不得发 202）；变异：给它发 202 ⇒ 红。
- 仓库缺陷：若裁决"照抄"，夹具要按 oracle 缺陷形态（背包下标命中、删仓库同下标）钉住；若裁决"登记"，
  牙改为"不删仓库件 + 白名单条目存在"。

### P3 `TAKECHECKITEM`

- **正例** `take_check_item_consumes_identity_not_name`：`#IF CHECKITEMW 木剑 1` + `#ACT TAKECHECKITEM`
  且背包有两件同名（`Dura` 不同）⇒ 只摘条件挑中的那件（`QuestCheckItem` 取最大 `Dura`；
  `CHECKITEMW` 名字型按 oracle 是佩戴位计数，夹具按裁决形态布置）、1 条 202。
- **负例（空输入）** `take_check_item_without_condition_logs_error`：`@main` 直接 `#ACT TAKECHECKITEM`
  （前面没有 `CHECKITEMW`/`CHECKDURAEVA`）⇒ `gotoLabParams.UserItem == null` ⇒ `[脚本错误]`、零帧、零变化。
- 反例组：`CHECKITEMW` 条件为假时后续 `#IF` 分支/`#ELSEACT` 不走 `TAKECHECKITEM`（条件门不被越过）。

## 6. 是否值得做（判据 + 建议）

| 切片 | 部署引用 | 成本 | 收益 | 建议 |
| --- | --- | --- | --- | --- |
| P1 `UPGRADEITEM`(212) | 0 | **低**：一个分派臂 + 一个方法，全部复用 Ex 的 trait/frame 通道 | 关掉"登记未实现"缺口；T-157 ① 从 4 点收敛到 3 点 | **做**（tier L：纯逻辑 + 会话态；RNG 只涉抽数次数，无内容面抽数对拍） |
| P2 `CLEARNEEDITEMS`/`CLEARMAKEITEMS` | 0 | 中：两条新钩子 + 写回期执行 + 仓库面 + 缺陷裁决 | 同上收敛 | **低优先/可延后**：等 owner 对 T-158 净差口径的裁定；若接受，按请求+写回期落地 |
| P3 `TAKECHECKITEM` | 0 | **高**：扩 `GotoLabParams` 物品身份 + 条件改造 + 修 `CheckItemCount` 计数面 | 最小 | **可不做**：在 T-157 与 `audit-script-actions-20261018.md` 保留"仍缺"字样即可；若做，先立"脚本物品身份模型"卡 |

**证据门槛**：以上"0 引用"只证明**冻结部署内容**不会触发；不证明引擎不可达（`@` GM 脚本、未来内容更新
都能引用）。因此建议：P1 做（便宜且封口），P2/P3 不做但**必须**保持"显式 `NotImplemented` + 清单可见"
现状（不许为了清零把登记名从 `IMPLEMENTED_EXECUTIONS` 里补进去而没实现——`implemented_registry_entries_are_actually_implemented`
会抓这种假绿）。

## 7. 风险与登记

- **R1 帧序**：P2 若走"请求 + 写回期"，其 202 帧发生在写回期（T-158 同族：与 oracle 动作序不同）；
  209/210 是"执行后终止动作列表"的注册动作，常规 `#ACT` 里它后面没有动作 ⇒ 影响面主要是
  "同一 GotoLable 的后续过程/条件读不到已清空状态"。
- **R2 仓库循环缺陷**（§1.3 第 3 条）：必须显式裁决（照抄 / 登记），不得顺手修。
- **R3 `CHECKITEMW` 计数面偏差**（§1.4）：Rust 用背包件数、oracle 用佩戴位件数；与 TAKECHECKITEM
  同源，建议另卡，不并入本规格实施。
- **R4 `nWhere == UseItems.Length` 异常**：照抄为 `abort_goto`；不要改成 `>=`。
- **R5 死分支 `nValType == 14`**：照抄 `Next(14)`，不得"修好"成 15。
- **R6 命名**：T-157 与派工文里的 `UPGRADEITEMS`/`UPGRADEITEMSEX` 与脚本字面 `UPGRADEITEM`/
  `UPGRADEITEMEX` 不一致，建议下次触碰 T-157 时订正（纯文档）。
- **R7 生产静默**：当前 `Engine::errors` 在生产入口被丢弃；实现后若要"部署脚本真的跑坏时可见"，
  需另卡把 `NotImplemented`/`ScriptError` 接进 stderr 日志（本规格不改）。
