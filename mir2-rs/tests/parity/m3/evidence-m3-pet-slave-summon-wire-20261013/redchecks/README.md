# 改坏必红（mutation → FAILED → 还原）

每对 = `*-mutated.txt`（改坏后跑该用例）+ `*-restored.txt`（`git checkout --` 还原后重跑）。
全部：mutated rc=101 / test result FAILED；restored rc=0 / ok；还原后 `git status` 干净。

| RC | 改坏点 | 文件 | 命中的用例 |
| --- | --- | --- | --- |
| RC1 | use_amulet 边界 `dura > duration` → `dura >= duration` | crates/world/src/magic.rs | magic::amulet_use_tests::use_amulet_deducts_when_enough_and_zeroes_when_not |
| RC2 | sm_spell 的 Series 用 magic_id 而非 Magic.Effect | bins/game-svr/src/smframe.rs | smframe::tests::spell_and_magicfire_field_layout |
| RC3 | cast_slave_spell 不消耗护身符（amulet = None） | bins/game-svr/src/slave.rs | slave::tests::cast_with_amulet_consumes_and_reaches_summon |
| RC4 | 距离门传 0 而非 MagicAttackRage 配置 | bins/game-svr/src/main.rs | tests::spell_cmd_summon_wires_amulet_and_frames |
