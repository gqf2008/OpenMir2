# 改坏必红 —— M3⑨-2 F-1 返工（`RM_MAGICFIRE` 自身帧）

命令：`cargo test -p game-svr --offline spell_cmd_summon_wires_amulet_and_frames`
（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`）

| # | 变异 | 目标用例 | mutated | restored |
| --- | --- | --- | --- | --- |
| RC5 | `broadcast_ref_msg_with_self` 的 `include_self` 由 `true` 改回 `false`（= 退回 F-1 缺陷） | `tests::spell_cmd_summon_wires_amulet_and_frames` | rc=101 / FAILED | rc=0 |

步骤：改 → 跑（`RC5-*-mutated.txt` + `.rc`）→ `git checkout -- mir2-rs/bins/game-svr/src/main.rs`
还原 → 重跑（`RC5-*-restored.txt` + `.rc`）；还原后 `git status --porcelain` 为空。

（20 片自身的 4 条改坏必红 RC1..RC4 见 `evidence-m3-pet-slave-taming-20261013/redchecks/`。）
