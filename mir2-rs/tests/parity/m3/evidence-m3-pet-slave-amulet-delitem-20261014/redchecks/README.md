# redchecks —— 改坏必红（3 条，均 rc=101）

`mutate.py` 首次变异前把原文写成 `<file>.mutbak`；`restore` 从 `.mutbak` 还原并删除。
还原口径：`git hash-object <file>` == `git rev-parse :<file>`（index，本分支改动已 staged）。

| 文件 | 变异 | 命中用例 | 读数 |
| --- | --- | --- | --- |
| `RC1-delitem-frame-not-sent-mutated.txt` | `main.rs`：deleted 支清槽但**不发** `SM_DELITEM`（回到登记前） | `tests::spell_cmd_summon_exhausted_amulet_emits_delitem` | `CARGO_TEST_RC=101` |
| `RC2-amulet-dura-not-zeroed-mutated.txt` | `slave.rs`：`deleted_amulet_body` 的 `dura: 0` → `dura: item.dura`（不照搬 oracle 先置 0） | `slave::tests::deleted_amulet_body_encodes_delitem_with_zero_dura` | `CARGO_TEST_RC=101` |
| `RC3-stdmode50-suffix-dropped-mutated.txt` | `slave.rs`：去掉 `StdMode==50` 的 `" #" + Dura` 名称后缀 | 同上 | `CARGO_TEST_RC=101` |

`restored-green.txt`：三条还原后跑 `cargo test -p game-svr --no-fail-fast delitem` = 全绿 rc=0。

用法：

```
python -I mutate.py m1        # / m2 / m3
python -I mutate.py restore
```
