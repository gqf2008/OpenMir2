# `NpcPlayer` 动态变量表（T-222，slice-1）

- 卡：`openmir2-script-dyn-var-host`（tier=R）；分支 `card/openmir2-script-dyn-var-host`，base `f3716d0c6`
- 作者：`openmir2-worker-24`；worktree `OpenMir2-wt-cronclaim`

## oracle 口径（`ProcessingBase.cs:1020-1045`、`PlayObject.cs:1023`）

- `HUMAN` 与 `Account` 返回**同一张** `PlayObject.DynamicVarMap`（`StringComparer.OrdinalIgnoreCase`）；
- `GLOBAL` = `SystemShare.DynamicVarList`（服务器级、跨玩家共享）；`GUILD` = 行会表；
- `VAR` = 不存在才插入；`SetMovDataDynamicVarValue` = 存在且**类型相同**才写回。

**与卡面偏差**（comment `8ddb8ef7`）：卡面写「`$HUMAN`/`$ACCOUNT` 各自独立表」，与 oracle 不符；
按 §1 行为等价实现为**共用一张**表（偏差登记在 T-222）。

## 本片实现（slice-1）

- `DynVarStore`：`BTreeMap<String, DynVar>`，键 `to_uppercase` 折叠（`OrdinalIgnoreCase`）；
  `insert` 不存在才插；`set` 类型相同才写；
- `NpcPlayer.dyn_vars: Option<&mut DynVarStore>` + `dyn_var_get/set/insert` 覆写
  （Human/Account 走存储；Global/Guild 返回 `None`/`false`）；
- 用例 1（插入/重复/读出/Account 同表/大小写/写回/类型不匹配/无存储/GLOBAL 未接）；
  牙 1（删 `dyn_var_get` ⇒ `left None vs right Some(5)`）。

## 未接（登记）

1. `$GLOBAL` 的服务器级表（宿主级存储 + 三漏斗注入）；
2. `$GUILD` 行会表；
3. **脚本漏斗尚未挂 `dyn_vars`**（会话级 per-session 表 + seed/写回，下一片）。

## 门禁

fmt / clippy `-D warnings` / `cargo test --workspace` / msgcodegen / anchor / registry / T 预约表（见卡面）。
