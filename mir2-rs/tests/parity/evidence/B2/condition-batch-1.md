# B2/M4 条件批次 1 证据（`ConditionProcessingSys` 注册条件面收敛）

> 卡片：`openmir2-b2-script-dispatch`（owner `openmir2-worker-3`，worktree `b2`，分支 `worktree-b-script-engine`）
> 目标（卡 DoD）：`93 个派发目标语义实现 + 未实现清单归零`。本批**只做条件面**，动作面留待下一批。

## 1. 一句话结论

把语料用到的条件派发目标从 **30 个未实现收敛到 2 个**（仅剩 `ConditionOfCheckVar(139)` 与
`ConditionOfCheckNameDateList(144)`，两者依赖动态变量层与日期列表文件）；语料派发目标总未实现数
**83 → 55**，已实现数 **23 → 51**，且 flow-diff 保持 **3 一致 / 0 不一致 / 1 exempt-OK（exit 0）**，
本地门禁全绿。同时把「未实现清单 → 0」从口头 DoD 变成**可证伪门禁**
（`parse-stats … --require-implemented`，当前按预期 **exit 1**）。

## 2. 改动面

| 文件 | 改动 |
| --- | --- |
| `crates/script/src/engine/mod.rs` | `ScriptPlayer`（40 个）/`ScriptNpc`（2 个）新增 42 个钩子（默认值 = 世界缺省，与 C# flow 夹具的 DispatchProxy 同口径）；新增 `PoseInfo`；新增 `script_condition_error` / `check_string_list` / `get_val_value_int` / `get_val_value_string` |
| `crates/script/src/engine/conditions.rs` | 新增 32 个条件处理器（语料命中 28 个；另 4 个顺带补齐：`CheckBagGage`/`Issysop`/`IsGuildMaster`/`IsCastleMaster`），含 C# 缺陷照搬 3 处 |
| `crates/script/src/engine/actions.rs` | `run_unregistered_condition` 补齐 9 个 `ScriptEngine` switch 条件分支 |
| `crates/script/src/engine/dispatch.rs` | `IMPLEMENTED_CONDITIONS` / `IMPLEMENTED_SWITCH_CONDITIONS` 扩容；新增覆盖三态 API（`condition_target_status` / `execution_target_status` / `switch_*_status`） |
| `crates/script-tool/src/parsestats.rs` + `main.rs` | 覆盖报告段 + `--require-implemented` 硬判据（DoD 门禁） |
| `crates/script/tests/engine_tests.rs` | 新增 3 个测试（语义、错误文本/缺陷照搬、覆盖清单登记） |
| `tools/script-parity-cs/RunMode.cs` | **夹具缺口**修正 2 处（见 §3.3） |
| `tests/parity/flow-cases.json` | `@rw03` 豁免用例的 `rust_expect` 按新行为重登记（见 §3.3） |

## 3. 关键结论（可与独立复核对表）

### 3.1 条件错误文本走的是 `ConditionProcessingSys` 的**本地重载**

`ScriptEngine` 的 `ScriptConditionError(…, string sErrMsg, …, ConditionCode sCmd)` 产出的
`[脚本错误] … 地图:x(y) …参数6` **不是**条件处理器用的那条。条件处理器（在
`ConditionProcessingSys` 类内）绑到同类里的本地重载：

```
[脚本参数不正确] Cmd:{枚举成员名} NPC名称:{ChrName} 地图:{MapName} 座标:{CurrX}:{CurrY} 参数1:{p1} … 参数5:{p5}
```

未接入前 Rust 会写出 `[脚本错误] CHECK… 脚本命令:…`，与 oracle 逐字不同；已按上表改正，并用
C# 侧实跑同一脚本比对（§3.3）。

### 3.2 照搬的 C# 缺陷（行为等价优先于修 bug）

| 处理器 | C# 原样行为 | Rust 照搬方式 |
| --- | --- | --- |
| `ConditionOfCheckItemAddValue`(130) | 比较符取 **`sParam1[0]`**（佩戴位下标）而非 `sParam2[0]`；`nAddValue = StrToInt(sParam3,-1)`；范围校验 `nWhere <= UseItems.Length`（上界多 1） | 同式；上界取 `use_items_upper_bound()+1`，越界不 panic（C# 会抛） |
| `ConditionOfCheckOnLine`(150) | `success = false` 后**只会**写 false（从不置 true）⇒ 恒假 | 恒 `false`（仍执行 `GetPlayObject` 查询以保持调用面） |
| `ConditionOfCheckReNewLevel`(105) | 报错码写的是 `CHECKLEVELEX` | 报错用 `CHECKLEVELEX` |
| `ConditionOfCheckSlaveListCount`(9) | 恒假（此前已照搬） | 不变 |

### 3.3 C# 侧实跑复核（豁免用例 `商店-比奇国王@rw03`）

S3 修掉 B-8 位移后，该用例的 C# 侧**不再崩溃**（原 S2 崩溃是旧位移落到
`ConditionOfCheckRangeMonCount` 所致）。跑通前先修了夹具两处缺口（都属夹具、非 oracle 行为差异）：

1. `FixtureItemSystem.GetStdItemName`：`(ushort)args[0]` 对 `int` 实参拆箱抛 `InvalidCastException`
   ⇒ 改 `Convert.ToUInt16(args[0])`。
2. `RunPlayer.SendAddItem` / `SendDelItems`：夹具原来在这里**又**改了一次 `ItemList`，而真实
   `PlayObject.SendAddItem`/`SendDelItems`（`src/M2Server/Player/PlayObject.Base.cs:198`）只下发消息；
   重复改列表导致 `GotoLableTakeItem` 随后的 `ItemList.RemoveAt(i)` 下标错位
   （`ArgumentOutOfRangeException`）⇒ 夹具改为只记日志。

两侧对同一命令行的输出（脚本真实行 `CHECKITEMADDVALUE 3 10 = 1`）：

```
C#  : delitem 祝福油 1
      err [脚本参数不正确] Cmd:CHECKITEMADDVALUE NPC名称:测试NPC 地图:0 座标:100:200 参数1:3 参数2:10 参数3:= 参数4:1 参数5:
      #gold=100000
      #items=祈福项链
Rust: delitem 祝福油 1
      err [脚本参数不正确] Cmd:CHECKITEMADDVALUE NPC名称:测试NPC 地图:0 座标:100:200 参数1:3 参数2:10 参数3:= 参数4:1 参数5:
      err(rust) NotImplemented { kind: "action", code: 213, handler: "ActionOfUpgradeItemsEx" }
      #gold=100000
      #items=祈福项链
```

⇒ 唯一残差是尚未实现的 `ActionOfUpgradeItemsEx(213)`（动作面，下一批）；该用例继续按 `exempt` +
`rust_expect` 比对（`EXEMPT-OK`）。

## 4. 门禁（命令 + 结果）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| 格式 | `cargo fmt --all -- --check` | exit 0 |
| 静态检查 | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| 单测 | `cargo test --workspace` | exit 0（42 个测试二进制全 ok；日志 `E:/tmp/b2-test.log`） |
| 消息号表 | `cargo run -p msgcodegen -- --check` | exit 0（604 条一致） |
| 派发回归门禁 | `powershell -ExecutionPolicy Bypass -File tests/parity/script-dispatch/run_dispatch_regression.ps1 -RequireRustFlipped` | GREEN（5 PASS/0 FAIL + Rust 已翻转） |
| 六类流程对拍 | `python -I tools/flow-diff.py tests/parity/flow-cases.json` | exit 0：3 一致 / 0 不一致 / 1 exempt-OK（`flow-diff.log`） |
| 覆盖（DoD 门禁） | `cargo run -p mir2-script-tool -- parse-stats E:/MirServer/M2GameSvr/Envir` | 未实现 55 / 已实现 51（`coverage-after.txt`） |
| 覆盖硬判据 | 同上 `+ --require-implemented` | **按预期 exit 1**（未实现 55） |

覆盖前后（同一工具、同一语料，`coverage-before.txt` / `coverage-after.txt`）：

```
before: 语料派发目标覆盖：已实现 23 个（5817 次调用）；未实现 83 个
after : 语料派发目标覆盖：已实现 51 个（6870 次调用）；未实现 55 个
条件面：未实现 30 → 2（仅剩 ConditionOfCheckVar(139) / ConditionOfCheckNameDateList(144)）
```

## 5. 覆盖缺口的诚实登记

以下钩子当前是**世界缺省值**（0 / 空 / false / None），与 C# flow 夹具的 mock 同口径；
真世界（M2 世界层）接入后由适配层覆写，届时需按 §6.0 重新对拍：

`is_enough_bag`（背包上限 46 已按 C# 实现）、`permission`、`gender`、`job`、`game_gold`、
`game_point`、`credit_point`、`re_level`、`member_level`、`bonus_total_point`、`dear_name`、
`master_name`、`is_master`、`guild_name`、`is_guild_master`、`is_new_human`、`map_name`、
`user_account`、`curr_pos`、`use_items_upper_bound` / `use_item_index` / `worn_item_add_value`、
`quest_check_item`、`magic_level`、`in_safe_zone`、`is_player_online`、`map_human_count`、
`map_exists`、`envir_map_name`、`xy_obj_count`、`last_hiter_*`、`target_name`、`pose`、
`global_val` / `globa_dy_mval` / `global_a_val` / `server_int_val` / `server_str_val`。

条件面剩余 2 项（下一批）：`ConditionOfCheckVar(139)`（`GeDynamicVarList` 动态变量层）、
`ConditionOfCheckNameDateList(144)`（列表文件 + 日期差，C# 里日期解析已注释 ⇒ `nDay = int.MaxValue`）。

## 6. 残留物清单

- `E:/tmp/b2-gates.log`、`E:/tmp/b2-test.log`、`E:/tmp/b2-coverage-*.txt`：临时日志，内容已归档到本目录，可删。
- 无新增进程、无新增数据库、无抓包中间件；`E:/tmp` 下本线专属残留仅上述日志。
- 本目录 `coverage-before.txt` / `coverage-after.txt` / `flow-diff.log` / 本报告：**保留**（复核用证据）。
