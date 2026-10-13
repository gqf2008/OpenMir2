# 改坏必红（46 分身，`openmir2-m3-pet-slave-clone46`）

统一被测用例：`cargo test -p game-svr --bin game-svr spell_cmd_clone --offline`
（`tests::spell_cmd_clone_wires_entity_and_frames`，走生产 `handle_spell_cmd`）。

流程：改 → 跑（`*-mutated.txt`，期望 RED/rc=101）→ 还原（用备份副本，**不**用 `git checkout --`：
本片代码当时尚未提交，`checkout --` 会连未提交改动一起覆盖）→ 重跑（`*-restored.txt`，期望 GREEN/rc=0）。

| 编号 | 改坏点 | 文件 | mutated 读数 | restored |
| --- | --- | --- | --- | --- |
| RC1 | `Dir` 用主人正向（`mag_make_clone` 内 `back_dir(inp.dir)` → `inp.dir`） | `crates/world/src/magic.rs` | RED：`left: 2, right: 6`（展示 dir 断言） | GREEN |
| RC2 | 不发克隆体 `SM_TURN`（去 `broadcast_ref_msg(world, online, gates, clone.clone_id, &frames)`） | `bins/game-svr/src/main.rs` | RED：`turns.len()` `left: 0, right: 2` | GREEN |
| RC3 | 不插实体（去 `world.entities.insert(e);`） | `crates/world/src/slave.rs` | RED：`应造出克隆体实体` | GREEN |

> RC3 文件 `crates/world/src/slave.rs` 在还原后已 `cmp` 与备份一致；三处 `git status --porcelain`
> 在还原后回到「仅 5 个 feature 文件 + whitelist.md 变更」的状态，且最终提交树中 grep `MUTATION` = 0。
