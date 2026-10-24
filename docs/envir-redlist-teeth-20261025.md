# envir-redlist T3 待补牙（4 颗常驻反向牙）— 2026-10-25

- 卡：`openmir2-envir-redlist-teeth`（tier **L**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；分支 `card/envir-redlist-teeth`）。
- 基线：本地 `master` `a6fc02d0a`（含 `d0b380ab4` 的 `crates/data/src/lists.rs` 红名单装载实现）。
- 写集：`mir2-rs/crates/data/src/lists.rs` 的 `#[cfg(test)] mod tests`（**只加测试**）+ 本文件。
- 红线：不改生产逻辑、不改场景/规则表；未起栈、未占端口、未连库。

## 0. 一句话

在 `lists.rs` 测试模块新增 **4 颗常驻反向牙**（`tooth_*`），覆盖 T3 原片登记的四类未捕获变异：
**空文件/缺行、重复行、非法行（类型错/越界）、边界（空名/超长名）**。每颗都做了
「改坏 ⇒ 红 ⇒ 还原 ⇒ 绿」的实测（共 7 个变异，全部红后还原绿）。

## 1. 四颗牙与 oracle 依据

| 牙 | 测试名 | 钉什么 | oracle 锚 |
|---|---|---|---|
| ① 缺行 / 空文件 | `tooth_missing_rows_and_empty_files` | 空输入 ⇒ 空表（**不是**通配、不是兜底行）；2 行且首行 `*` **不**触发通配（`Count==1` 门）；0 字节文件 ⇒ `Some(空)`（区别于缺文件 `None`）；ItemNameList 缺第 3 列收录（空名）、缺第 2 列跳过 | `M2Share.cs:1512-1526`（`LoadList.Count == 1 && StartsWith("*")`）；`CustomItemSystem.cs:80-112` |
| ② 重复行 | `tooth_duplicate_rows_are_preserved` | 解析层不去重；ItemName 取**最后一条**（倒序）、Admin 取**第一条**（正序）、GameLog 任一条命中即真、Unbind 重复键保留首个（`TryAdd`） | `CustomItemSystem.cs:26-41`（倒序 Get）；`LocalDB.cs:1021-1039` 查找；`LocalDB.cs:872-915`（`TryAdd`） |
| ③ 非法行（类型错/越界） | `tooth_illegal_rows_type_and_overflow` | `i32::MAX+1` 溢出、负值、非数字 ⇒ ItemNameList 全部跳过（`StrToInt(...,-1)`）；Unbind 溢出 ⇒ `StrToInt(...,0)` ⇒ 非正 ⇒ `result = -i; break`；Admin 等级字符非法 ⇒ 跳过 | `HUtil32.StrToInt`（失败取默认值）；`LocalDB.cs:872-915`；`LocalDB.cs:16-97` |
| ④ 边界（空名/超长名） | `tooth_boundary_empty_and_long_names` | 空名按 oracle 收录（ItemName 两列行、Unbind 正数 id + 空名）；4096 个 CJK 字符的名字逐字符保留、不截断（ItemName / Unbind / Admin 三处） | 同上（oracle 不补默认名、不截断 `string`） |

> 两颗既有牙（`item_name_list_skips_non_numeric_rows`、`unbind_list_duplicate_key_keeps_first`）与新牙有交集；
> 新牙补的是它们**没钉**的面：空输入 vs 通配的边界、`Count==1` 门、0 字节文件、溢出（而非仅非数字）、
> 空名收录、超长名不截断、以及 Admin/GameLog 的重复行次序。

## 2. 变异验证（改坏必红 + 还原）

方法：在 `lists.rs` 生产代码上做单点变异 → 只跑对应牙 → 期望 `test result: FAILED`（cargo rc=101）
→ 还原（逐字节回到变异前）→ 再跑同一牙 → 期望 `test result: ok`。全部 7 个变异均红后还原绿：

| 牙 | 变异 | 改坏后 | 还原后 |
|---|---|---|---|
| ① | `parse_game_log_item_name_list`：空输入也当通配（`lines.is_empty() \|\| …`） | FAILED | ok |
| ① | `parse_item_name_list`：跳过空名（`name_s.is_empty() ⇒ continue`） | FAILED | ok |
| ② | `parse_item_name_list`：解析层按 `(make,item)` 去重 | FAILED | ok |
| ② | `get_custom_item_name`：去掉 `.rev()`（改取首条） | FAILED | ok |
| ③ | `str_to_int(&make_s, -1)` → 默认值 `0`（溢出/类型错被收录） | FAILED | ok |
| ③ | `str_to_int(&data_s, 0)` → 默认值 `1`（溢出被当合法 key） | FAILED | ok |
| ④ | `item_name: name_s` → `name_s.chars().take(64).collect()`（截断） | FAILED | ok |

复现（本检出）：

```sh
cd mir2-rs
cargo test -p mir2-data --lib lists::tests::tooth          # 4 passed
# 变异：编辑 crates/data/src/lists.rs 的对应生产行 → 复跑单条 tooth_* → 应 FAILED
# 还原：git diff 确认生产行零改动（本卡 diff 只有 mod tests 尾部的 +128 行）
```

## 3. 门禁

- `cargo fmt --all -- --check` rc=0；`cargo clippy --workspace --all-targets -- -D warnings` rc=0；
  `cargo test --workspace` rc=0；`cargo run -p msgcodegen -- --check` rc=0。
- 卫生门禁：conflict-marker / stale-pointer / dynamic-count-stamp / board-coverage / t_reservation /
  whitelist-anchor / m3-registry 全 rc=0。
- `git diff --check` rc=0；`git merge-tree --write-tree master HEAD` 无冲突。
- 生产逻辑零改动（`git diff` 仅 `mod tests` 尾部新增）。

## 4. 判不准 / 残余

- 「4 颗待补牙」的原始 `E:/tmp/impl-lists/mutation.log` 已不在本机；本卡按 issue 的**四类**定义补牙，
  未逐条复核 v3 当初的 4 个具体反向变异是否一字不差。
- 未重跑全量 `cargo test --workspace` 之外的其它工具链；未触发任何真机/协议面（本卡纯测试面）。
- 残留：无（变异临时文件在 `%TEMP%`，已随脚本删除）。
