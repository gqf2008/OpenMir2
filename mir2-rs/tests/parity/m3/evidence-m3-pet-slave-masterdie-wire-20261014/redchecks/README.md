# redchecks —— 改坏必红（M3⑨ masterdie-wire）

逐条：变异源码 → 跑钉住的判据 → 期望 rc=101 RED → `git checkout -- crates/world/src/slave.rs` 还原 → 重跑 rc=0。
变异脚本：`E:/tmp/m3hp-t54-redcheck/mutate.py`（RC1–RC4）。原始件 `7cdab58ba8112f3e93b611986fa39d7a375451d8`。

| 文件 | 变异 | 判据 | 读数 |
| --- | --- | --- | --- |
| `RC1-delay-1000-to-0-mutated.txt` | `MASTER_DIE_SELF_DELAY_MS: 1000 → 0` | `master_die_self_delays_1000ms_then_mutinies_in_reverse` | rc=101 RED（`Die() 尾应入队一条自投递` left=0 right=1） |
| `RC2-reverse-to-forward-mutated.txt` | mutiny 遍历 `iter().rev() → iter()` | 同上 | rc=101 RED（`倒序遍历 SlaveList`） |
| `RC3-death-gate-removed-mutated.txt` | `if !master.death \|\| !cfg.master_die_mutiny` → `if !cfg.master_die_mutiny` | `process_slave_mutiny_early_return` | rc=101 RED |
| `RC4-ghost-gate-removed-mutated.txt` | 删 `send_self_delay_msg` 的 `if ghost { return; }` | `master_die_ghost_self_delay_is_blocked_by_ghost_gate` | rc=101 RED |
| `restored-green.txt` | —（还原后全 `slave::` 套件） | 45 passed 0 failed | rc=0 |

每个 `*-mutated.txt` 首两行记录变异后源文件的 `git hash-object`（≠ pristine）与 pristine 哈希。
