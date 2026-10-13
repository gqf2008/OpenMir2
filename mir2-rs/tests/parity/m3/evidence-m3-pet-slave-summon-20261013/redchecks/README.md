# 改坏必红（RC1–RC4）

每条 = 在**已提交的** `crates/world/src/slave.rs` 上做一次最小 mutation → 跑
`cargo test -p mir2-world --offline --lib slave` → 必出 FAILED → 用备份还原（还原后
`git status` 对 `slave.rs` 为空，sha256 与提交一致）。

| 编号 | mutation | 必红用例 | 读数 |
| --- | --- | --- | --- |
| RC1 | `make_slave_gate` 的比较由 `<` 改成 `<=` | `make_slave_gate_is_strictly_less`、`redcheck_gate_must_be_strict`、`summon_slave_rejected_when_slave_list_full` | 16 passed / **3 failed** |
| RC2 | 骷髅选型循环里补上 `break`（= 把 oracle 无 break 的缺陷“修好”） | `skeleton_selection_keeps_last_match_no_break` | 18 passed / **1 failed** |
| RC3 | `make_slave_adopt` 的 HP 补半由 `(MaxHP-HP)/2` 改成 `/3` | `make_slave_adopt_writes_oracle_tail`、`summon_slave_skeleton_end_to_end` | 17 passed / **2 failed** |
| RC4 | `recall_slave_target` 的遍历由 `iter().rev()` 改成 `iter()`（从头找） | `recall_slave_target_scans_from_tail` | 18 passed / **1 failed** |

原始输出见同目录 `RC*.txt`（含 `rc=` 行）。还原后 `crates/world/src/slave.rs`
sha256 = `e4a1a4f31c1752385a805a70bc2796a4904d33c2879c5a48ab3f6f98a4301d76`。
