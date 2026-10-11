# B2/M4 var 家族 / 剩余动作批次证据（语料派发目标未实现归零）

> 卡片：`openmir2-b2-script-dispatch`（owner `openmir2-worker-3`，worktree `b2`，分支 `worktree-b-script-engine`）
> 目标（卡 DoD）：`语料用到的派发目标未实现数为 0`。本批收口上一批遗留的 **17 个**目标。

## 1. 一句话结论

语料派发目标覆盖 **89 已实现 / 17 未实现（10509 次调用）→ 106 已实现 / 0 未实现（11829 次调用）**；
flow-diff 保持 **3 一致 / 0 不一致 / 1 exempt-OK（exit 0）**；本地门禁全绿。本批**未自行 merge、未签 review**。

## 2. 改动面

| 文件 | 改动 |
| --- | --- |
| `crates/script/src/engine/vars.rs` | 新增 `DynVar` / `DynVarKind` / `VarVal` 与 `dyn_var_kind_of`（`GetDynamicVarMap` 类型前缀解析） |
| `crates/script/src/engine/mod.rs` | `ScriptPlayer` 新增编号槽位 setter 9 个 + `DynamicVar` 存储钩子 3 个 + 世界面钩子（道具 `Desc`/`DuraMax`、刷怪/清图、日期、婚姻自/对方侧）14 个；新增 `ProcessingBase` 取值层移植（`get_mov_data_val_name_value` / `set_val_name_value` / `get_mov_data_dynamic_var_value` / `set_mov_data_dynamic_var_value` / `get_mov_data_type` / `get_mov_data_human_info_value` / `arrest_str_param` / `is_var_number`）与 `script_action_error_msg`；`PoseInfo` 增 `chr_name` |
| `crates/script/src/engine/actions.rs` | 15 个动作处理器：`MovData/MovrData/IncInteger/DecInteger/MulData/Var/LoadVar/SaveVar/CalcVar/UpgradeItemsEx/Marry/UnMarry/AddUseDateList/ClearMapMon/MonGenEx`；`ConfFile.Load` 读侧与数值/字符串写回、`DEC` 的 C# `Substring` 移除、`trim_var_file` 等 |
| `crates/script/src/engine/conditions.rs` | `ConditionOfCheckVar(139)`、`ConditionOfCheckNameDateList(144)`，含 `GetVarValue/GetValNameValue/GetDynamicValue/SetValNameValue/SetDynamicValue` 子层 |
| `crates/script/src/engine/dispatch.rs` | `IMPLEMENTED_EXECUTIONS` +15、`IMPLEMENTED_CONDITIONS` +2 |
| `crates/script-tool/src/flowrun.rs` | **夹具缺口**修正：`FlowPlayer.use_items_len()=12`（对齐 C# `UseItemsStub` 的 12 个空 `UserItem`） |
| `crates/script/tests/engine_tests.rs` | MockPlayer 补 20+ 钩子（含动态变量表/GlobalVal/Desc/pose 侧）；更新 2 个批次收口断言；新增 8 个语义单测 |
| `tests/parity/flow-cases.json` | `@rw03` 豁免用例 `rust_expect` 重登记（`ActionOfUpgradeItemsEx` 已实现，两侧现逐字一致） |

## 3. 关键结论（可与独立复核对表）

### 3.1 C# 缺陷照搬（行为等价优先于修 bug）

| 处理器 | C# 原样行为 | Rust 照搬方式 |
| --- | --- | --- |
| `ActionOfCalcVar` | `IsStringNumber` 恒真 ⇒ `sParam4` **永不展开**；且读写**总是玩家 `DynamicVarMap`**（无视 `sType`） | 同式（`DynVarKind::Human`），`sType` 只用于 `GetDynamicVarMap` 的 null 判定 |
| `ActionOfIncInteger` / `DecInteger` | `!IsStringNumber(sParam3)` 恒假 ⇒ 只走 `n3c = nParam3`、`s01 = ""` | 同步收敛，去掉死分支 |
| `ActionOfVar` / `LoadVar` / `SaveVar` | 类型错误时报错码用的是 `ExecutionCode.Var`（非 `LoadVar`/`SaveVar`） | 同式 |
| `ActionOfSaveVar` | `ConfFile.WriteInteger` 空实现、`WriteString` 不 `Save()` ⇒ **不落盘** | 只做存在性/类型校验，不写盘 |
| `ActionOfUnMarry` | `REQUESTUNMARRY` 的 `FORCE` 分支走 `GetPlayObject(DearName)`；对方离线时仅注释代码 | 世界层未接入 ⇒ 按对方离线分支（清 DearName、MarryCount++） |
| `ConditionOfCheckVar`(139) | `char cMethod = sMethod[1];` 对单字操作符（语料全部为 `>`/`<`/`=`）会 `IndexOutOfRangeException` | **S2 崩溃面**：Rust 落 `'\0'` ⇒ 默认 `>=`，不 panic（见 §5 残留） |
| `ConditionOfCheckNameDateList`(144) | 日期解析整段注释 ⇒ `nDay = int.MaxValue`；`>`/`>=` 恒真 | 同式；`nDayCount - nDay` 用 `wrapping_sub`（C# unchecked int） |

### 3.2 语料实测的 `<$...>` 只有 `$HP` / `$MP` / `$HUMAN(...)` 命中 `GetMovDataHumanInfoValue`

`GetMovDataHumanInfoValue` 的 `switch` **不含** `$STR(...)`（`<$STR(S24)>` 等）与 `$H.HP`/`$H.MP`
⇒ C# 原文即返回未命中并 `ScriptActionError(Mov)`；Rust 同式。已覆盖的固定变量以「有 trait getter」
为界（`$HP/$MAXHP/$MP/$MAXMP/$LEVEL/$EXP/$PKPOINT/$CREDITPOINT/$GOLDCOUNT/$GAMEGOLD/$GAMEPOINT/$X/$Y/$USERNAME/$MAP/$GUILDNAME/$RELEVEL`）
+ `$HUMAN/$GLOBAL/$ACCOUNT(...)`，其余返回未命中，待世界适配层接入后补齐（见 §5）。

### 3.3 夹具缺口（非 oracle 行为差异）

`FlowPlayer` 原先 `use_items_len()` 缺省 0，而 C# flow 夹具 `RunMode.cs` 的 `UseItemsStub` 是 12 个空
`UserItem()`。`ActionOfUpgradeItemsEx` 实现后该差异显现（Rust 多报一行 `UpgradeItemSex`）。补
`FlowPlayer.use_items_len()=12` 后，`@rw03` 的 Rust 输出与 C# **逐字一致**（4 行）。

## 4. 门禁证据（原始输出见 `var-batch-gates.txt`）

| 门禁 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | clean |
| `cargo test --workspace --offline` | 42 个 test target 全 ok，0 failed |
| `cargo run -q -p msgcodegen -- --check` | 604 条常量与 C# 源一致 |
| 覆盖报告（`parse-stats`） | 106 已实现 / **0 未实现**（`coverage-var-after.txt`） |
| `python -I tools/flow-diff.py tests/parity/flow-cases.json` | **3 一致 / 0 不一致 / 1 exempt-OK，exit 0**（`flow-diff-var.txt`） |
| 「登记即实现」守卫 | `implemented_registry_entries_are_actually_implemented` ok |

- 证据四件套：命令 = `var-batch-gates.txt` / `flow-diff-var.txt` / `coverage-var-after.txt`；
  输出 = 上述文件内容；路径 = `mir2-rs/tests/parity/evidence/B2/`；hash = 见提交 `531c36e3` 之后的 HEAD。
- before 基线：`coverage-action-1.txt:116`（89 已实现 / 17 未实现）。

## 5. 残留物清单（诚实登记）

1. **`GetMovDataHumanInfoValue` 未覆盖的固定变量**：依赖世界未接入的 `WAbil.AC/DC/MC/SC/HW/BW/WW`、
   `Abil.MaxExp`、`GetMyStatus`、`LogonTime/LogonTick`、佩戴位物品名、`$DATE/$DATETIME/$USERCOUNT`
   等；Rust 现返回未命中（C# 在这些变量上会返回值）。接入世界适配层时按 `ProcessingBase.cs:35-566` 补齐。
2. **`ConditionOfCheckVar(139)` 的 `sMethod[1]`**：C# 对单字操作符越界崩溃；Rust 落 `>=` 语义。
   属 S2 崩溃面，未单独挂白名单条目（语料暂无两侧可跑通的 C# 基线可比对）。
3. **`ActionOfMarry` 的对方侧**：`GotoLable`/`DearHuman`/`MarryCount` 通过新增 `ScriptPlayer` pose 钩子表达，
   世界适配层尚未实现（默认 no-op），故 `@MarryCheckDir` 等分支由脚本自身处理。
4. **`ActionOfClearMapMon` / `ActionOfMonGenEx` / `ActionOfAddUseDateList`**：分别委派
   `clear_map_monsters` / `regen_monster` / `today_string` 钩子；世界层未接入时记录台账/返回缺省。
5. `ConditionOfCheckNameDateList` 的 `GetValNameValue`/`SetValNameValue` 槽号区间与 `GetValNameNo` 偏移
   **不一致**（C# 遗留），已逐行照搬；`S/N` 动态串表分支未实现（语料未用）。
6. 临时物：本批只改仓库内文件；`E:/tmp/b2_tests_append.rs` 为生成测试补丁用的临时脚本，
   已随本报告登记（无运行中进程、无端口占用）。
