# redchecks —— 改坏必红（M3⑨-实现 3 宝宝生命周期）

4 条牙齿均由作者在 `@lifecycle head` 本分支实跑：**改坏 ⇒ rc=101 / 1 failed；还原 ⇒ rc=0**。
原始件成对存放：`RCn-*-mutated.txt`（改坏读数）+ `RCn-*-restored.txt`（还原读数）。

| 编号 | 改坏点 | 必红用例 |
| --- | --- | --- |
| RC1 | `check_royalty` 的 `SpiritMutinyTick` 支去掉 `SlaveExpLevel < 5` 门 | `check_royalty_spirit_window_gated_by_level` |
| RC2 | `slave_list_cleanup` 去掉 `Master != this` 支（只删死亡/幽灵） | `slave_list_cleanup_removes_dead_ghost_and_mismaster` |
| RC3 | `process_slave_mutiny` 去掉 `LastHiter` 短路（无 `LastHiter` 也抽数） | `process_slave_mutiny_no_rng_without_last_hiter` |
| RC4 | `tame_royalty_tick` 的 `* 60 * 1000` 改为 `* 1000` | `tame_royalty_tick_formula` |

还原后 `git status --porcelain` 仅剩本证据目录（未跟踪）；`slave.rs` 与提交逐字节一致
（sha256 见 SHA256SUMS.txt）。复现步骤见 `../commands.txt` §6。
