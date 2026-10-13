# 改坏必红 —— M3⑨-2 第③片（20 诱惑）

统一命令：`cargo test -p game-svr --lib --offline tamming`
（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`）

| # | 变异 | 目标用例 | mutated | restored |
| --- | --- | --- | --- | --- |
| RC1 | `cast_tamming_spell` 删 `world.tame_adopt(...)` 调用 | `tamming_adopts_slave_and_clears_target_creature` | rc=101 / FAILED | rc=0 |
| RC2 | `tamming_input` 删 `!race_is_animal` 门 | `tamming_animal_gate_blocks_and_consumes_no_draw` | rc=101 / FAILED | rc=0 |
| RC3 | 受奴判定退化为 `tamed = out.target_master_is_caster;` | `tamming_does_not_re_adopt_existing_slave` | rc=101 / FAILED | rc=0 |
| RC4 | `cast_tamming_spell` 删 `t.ai.target_creature = None;` | `tamming_adopts_slave_and_clears_target_creature` + `tamming_does_not_re_adopt_existing_slave` | rc=101 / FAILED | rc=0 |

每条：改 → 跑（`*-mutated.txt`）→ `git checkout -- mir2-rs/bins/game-svr/src/slave.rs` 还原 →
重跑（`*-restored.txt`）；还原后 `git status --porcelain` 为空。
