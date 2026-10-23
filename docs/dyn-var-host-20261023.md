# `NpcPlayer` 动态变量表（T-222，最终交付）

- 卡：`openmir2-script-dyn-var-host`（tier=R）；分支 `card/openmir2-script-dyn-var-host`
- 初版作者：`openmir2-worker-24`（slice-1/2/3）；**本版本 = 复审 `24132993`（reviewer-5 request_changes）返工**，
  返工作者 `openmir2-worker-68`；基线 `43ce8dca`（rebase 后）

## 1. oracle 口径

| 项 | oracle | 出处 |
| --- | --- | --- |
| `HUMAN` / `Account` | **同一张** `playerActor.DynamicVarMap`（`StringComparer.OrdinalIgnoreCase`）；`GetDynamicVarMap` 的 `sName` 在 get/set 里**未被使用**，隔离靠「每个 `PlayObject` 一张表」 | `ProcessingBase.cs:1020-1045`、`PlayObject.cs:1023` |
| `GLOBAL` | `SystemShare.DynamicVarList`（**服务器级**、跨玩家共享） | 同上 |
| `GUILD` | `MyGuild.DynamicVarList`（无行会 ⇒ null） | 同上 |
| `VAR` | `ContainsKey` 才报「已存在」，否则 `Add`（**只插不改**） | `ProcessingBase.cs:4147-4155` |
| 写回 | `SetMovDataDynamicVarValue`：int 需 `nDataType==1 && VarType==Integer`，**string 支不检查 `nDataType`**（怪癖照搬） | `ProcessingBase.cs:892-944` |
| `CALCVAR` | 用 `GetDynamicVarMap` 只为判类型，实际读写**恒是 `playerActor.DynamicVarMap`**（忽略 `sType` 的怪癖）；`+ - * /` 为 C# unchecked wrap；`/ 0` 抛异常被 catch | `ExecutionProcessingSys.cs:3787-3871`、`BaseObject.Base.cs:771-775` |
| `SAVEVAR` | **oracle 不落盘**：`ConfFile.WriteInteger` 空实现、`WriteString` 只改内存缓存且从不 `Save()` | `ConfFile.cs:370-395` |

## 2. 实现

- `DynVarStore`（`bins/game-svr/src/lib.rs`）：`BTreeMap` + 键 `to_uppercase()` 折叠（`OrdinalIgnoreCase`）；
  `insert` 不存在才插；`set` **存在且类型相同**才写回（否则 `false`，调用方报错）。
- `NpcVars`（`npc.rs`）两张共享表：
  - `global_dyn_vars: Rc<RefCell<DynVarStore>>` —— 服务器级（等价 `SystemShare.DynamicVarList`）；
  - `session_dyn_vars: Rc<RefCell<BTreeMap<ChrName, DynVarStore>>>` —— HUMAN/ACCOUNT 按角色寻址。
  两者都是 `Rc` ⇒ `NpcVars::clone()` 仍指向同一张宿主表（脚本漏斗零拷贝共享）。
- `NpcPlayer` 覆写 `dyn_var_get` / `dyn_var_set` / `dyn_var_insert`（`npc.rs`）：
  `Human | Account` → 本角色 `session_dyn_vars[chr_name.to_uppercase()]`；`Global` → `global_dyn_vars`；
  `Guild` → `None`/`false`（未建模，登记 T-222）。

## 3. 三个脚本漏斗的接线现状（review I-1 修复后）

| 漏斗 | 构造点 | 是否拿到共享 `NpcVars` |
| --- | --- | --- |
| 商人 / 固定 NPC 说词 | `npc_player_from_session`（`p.vars = vars.clone()`） | ✅（初版即有） |
| **`@Login`（QManage）** | `npc_player_from(..., &npc_vars_base)` | ✅ **本返工修复**：`npc_player_from` 新增 `vars: &NpcVars` 形参并 `p.vars = vars.clone()`（+ `map_names`/`sys_msg`） |
| **QFunction（组队 `GotoLable`）** | `handle_party_cmd` → `run_party_goto_label` → `npc_player_from` | ✅ **本返工修复**：两层各加 `npc_vars: &NpcVars` 形参，由 `sim_loop` 传 `&npc_vars_base` |

> 修复前 `npc_player_from` 从不设 `p.vars` ⇒ 每个快照 `NpcVars::default()`，两张表**按调用即弃**。
> 部署直接踩中：`Envir/MapQuest_def/QManage.txt [@Login]` 的
> `VAR INTEGER HUMAN 时长` + `LOADVAR HUMAN 时长 ..\QuestDiary\代练系统\代练数据.txt` 写入即丢，
> 后续 `QuestDiary/代练系统/代练.txt` 的 `CALCVAR HUMAN 时长 + 180` 报「变量不存在」。

## 4. 已知偏差（登记）

1. **生命周期（M-3）**：HUMAN/ACCOUNT 表是**进程级**、按 `ChrName` 寻址；oracle 是 `PlayObject` 级、
   登出即销毁 ⇒ **同角色重登后仍见到上一会话的变量**。本返工**不接登出清账**（会话销毁点不在本卡
   写集内且涉及世界生命周期），改为**窗口必跑项**：见 §6 W-2。
2. `$GUILD` 行会表未建模（`None`/`false`）。
3. `CALCVAR / 0`：oracle 抛 `DivideByZeroException` 被 `BaseObject.Operate` 的 catch 吞掉 ⇒ 动作中止、
   变量不变、服务器继续。Rust `wrapping_div(0)` 会 panic（生产派发无 `catch_unwind`）⇒ 引擎加等价守卫
   （报 `CalcVar` 动作错误 + 不写回，见 `actions.rs` 的 `action_of_calc_var`）。
4. `SAVEVAR` 不落盘（与 oracle 一致，非缺口）。
5. `to_uppercase()` 与 `OrdinalIgnoreCase` 对 ASCII/CJK 变量名等价；Unicode 特殊边界（`ß`→`SS`、
   土耳其 `ı`）理论可差，脚本变量名实为 ASCII/CJK，风险低（登记）。

## 5. 用例与牙

用例（6 条）：

| 用例 | 位置 | 覆盖 |
| --- | --- | --- |
| `npc_player_dynamic_var_map_matches_oracle_semantics` | `npc.rs` | 插入/重复 false/读出/Account 同表/大小写/写回/类型不匹配 false/不存在 false/GLOBAL 跨玩家 |
| `dyn_var_human_table_is_per_character_and_case_folded` | `npc.rs` | 同账号双角色互不可见（两向）、大小写折叠、HUMAN↔ACCOUNT 同格 |
| `npc_player_from_shares_dyn_var_tables` | `lib.rs` | **漏斗级**：`@Login` 式预写 → `npc_player_from` 读回 → 写回共享表 → 另一角色看 GLOBAL 不看 HUMAN |
| `npc_player_snapshot_maps_record_fields` | `lib.rs` | 既有（不回归） |
| `var_calcvar_div_by_zero_reports_error_without_panic` | `engine_tests.rs` | `CALCVAR / 0` ⇒ 报错、变量不变、不 panic；`/ 2` 正常 |
| `var_dynamic_var_declare_calc_and_check_var` / `var_loadvar_and_savevar_file_io` | `engine_tests.rs` | 既有引擎侧 VAR/CALCVAR/LOADVAR |

牙（3 颗，均逐字节还原并核对提交树 sha256）：

1. 删 `dyn_var_get` 覆写 ⇒ 宿主用例红；
2. `DynVarStore::key` 去掉 `to_uppercase()` ⇒ 大小写断言红；
3. 删 `npc_player_from` 的 `p.vars = vars.clone();` ⇒ 漏斗级用例红（I-1 的定向牙）。

## 6. 窗口项（R 卡收口条件）

| # | 场景 | 判据 |
| --- | --- | --- |
| W-1 | 部署 `VAR`/`CALCVAR` 实景（`QuestDiary` 代练/充值脚本） | 两侧分支走向与结果一致；`CALCVAR HUMAN 时长 + 180` 不得报「变量不存在」 |
| W-2 | **重登隔离（M-3 必跑）** | 同角色登出再登录后，`$HUMAN`/`$ACCOUNT` 变量应为**空**（oracle 语义） |

> W-2 是**已接受的已知偏差**（进程级表）；窗口若观察到残留，不得记为 PASS，须回到「接登出清账」。
