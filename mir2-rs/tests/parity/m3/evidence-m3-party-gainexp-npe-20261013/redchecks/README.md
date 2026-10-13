# 改坏必红（redchecks）—— M3⑦ Minor M-2 收口

对**唯一**实现改动（`World::gain_exp` 缺团长态）做定点改坏，在**同一命令**下复跑本卡判据，
确认从 GREEN 变 RED；随后 `cp` 还原并复跑确认恢复 GREEN（`RC0-restore-green.txt`）。

| 文件 | 改坏内容 | 变红判据 |
| --- | --- | --- |
| `RC1-gainexp-npe-red.txt` | `crates/world/src/world.rs`：把缺团长态从 fail-closed（返回 0）改回 `unwrap_or_default()`（= 抹掉本卡 fix，退回单人满经验） | `group_owner_missing_matches_oracle_npe`（`left: 600 / right: 0`） |
| `RC0-restore-green.txt` | 还原 `world.rs` 后 | `party_attribution` 10/10 GREEN；`git status` 只剩本卡改动 + 证据目录 |

> 改坏脚本作用对象是工作树（`cp` 备份 → 改 → 复跑 → `cp` 还原）；
> 还原后 `sha256sum world.rs` 与备份一致（`5f04cb5a…`），`git status --short` 无代码侧残留。
