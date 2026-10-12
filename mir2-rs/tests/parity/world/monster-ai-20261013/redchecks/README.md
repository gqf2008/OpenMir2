# 改坏必红（本片 3 条，均在交付树上重跑）

交付态 `crates/world/src/monai.rs` 的 sha256 =
`b8512cef5a8de5fa4f87011babb3e5b144f6fda87fc29f7bba3ed98d1a4a8ba0`（LF/git-blob 口径）。

复现：在交付树上做单点改动 → 跑指定用例 ⇒ 期望 FAILED（rc=101）→ 用备份还原并核对 sha256。

| 编号 | 改动 | 命令 | 结果 |
| --- | --- | --- | --- |
| RC-C1 | `wondering` 的 `Random(20) != 0` 门反置为 `== 0` | `cargo test -p mir2-world --lib monai::tests::wondering` | 0 passed / **3 failed**（3 条 Wondering 用例全红）⇒ `rc-C1-wondering-gate-inverted.txt` |
| RC-C2 | `goto_target_xy` 删除无条件的 `Random(3)`（改常量 0） | `cargo test -p mir2-world --lib monai::tests::goto_target` | 0 passed / **1 failed**（取数次序断言红）⇒ `rc-C2-goto-random3-removed.txt` |
| RC-C3 | `search_target` 的 `dist < bd` 改为 `dist > bd`（选最远） | `cargo test -p mir2-world --lib monai::tests::search_target` | 0 passed / **1 failed**（目标断言红）⇒ `rc-C3-search-farthest.txt` |

还原后 sha256 与上表一致；`cargo test -p mir2-world --lib monai` = 8 passed。
