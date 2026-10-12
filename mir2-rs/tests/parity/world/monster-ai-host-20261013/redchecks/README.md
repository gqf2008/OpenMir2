# 改坏必红（RC-C2-A..D）

每条 = 一次**单点变异** + 命中用例的**原始输出**（rc=101）。变异点都落在本片新增/修改的生产路径上，
还原后 `git status` 干净、被测文件 sha256 与提交一致。

| 编号 | 变异 | 命中用例 | 结果 |
| --- | --- | --- | --- |
| RC-C2-A | `enable_production_monster_ai` 改成空函数（宿主不开闸） | `game-svr mongen::tests::production_host_enables_monster_ai` | FAILED（`monster_ai_enabled()` 仍 false） |
| RC-C2-B | `WorldMonsterSink::spawn_monster` 不再转发 `req.dir`/`req.cool_eye` | `game-svr mongen::tests::world_monster_sink_seeds_ai_from_spawn_request` | FAILED（`ai.dir` 0≠5） |
| RC-C2-C | `spawn_monster_precomputed` 不调 `seed_from_spawn` | `mir2-world world::tests::spawn_monster_precomputed_consumes_no_rng` | FAILED（`ai.dir` 0≠5） |
| RC-C2-D | `search_target` 隐藏门退回 `fixed_hide_mode` | `mir2-world monai::tests::search_target_hide_gate_reads_hide_mode_not_fixed_hide_mode` | FAILED（`hide_mode=false,fixed=true` 被误挡） |

复核：`git stash`（或 `git checkout -- <file>`）后可逐条复现；每条 .txt 顶部写了变异点与命中用例。
