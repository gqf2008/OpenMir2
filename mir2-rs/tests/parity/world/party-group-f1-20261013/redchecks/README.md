# 改坏必红（F-1 / M-1 返工）

口径：`cp` 备份 → 精确 mutation → 跑命中用例（必 FAILED rc=101）→ `git checkout -- <file>` 还原 → 重跑（必 ok）。
每条给原始件；还原后 `git status --porcelain` 为空。

| # | 针对 | mutation | 命中用例（必红） | 原始件 |
|---|------|----------|------------------|--------|
| RC-1 | F-1（reviewer 要求：删掉 DelMember ⇒ 必红） | `world.rs::settle_deaths` 的 `if group_owner != 0` → `if false && group_owner != 0` | `party_vectors::die_member_triggers_oracle_del_member_and_group_survives`、`die_leader_disbands_group`（2 FAILED）；`game-svr::tests::die_triggers_del_member_frames_on_production_path`（1 FAILED） | `RC1-f1-delmember-removed-mutated.txt` |
| RC-2 | M-1 | `party.rs::party_play_object_by_name` 去掉 `e.race == ACTOR_RACE_PLAY &&` | `party_vectors::play_object_by_name_ignores_non_player_entities`（1 FAILED） | `RC2-M1-race-filter-removed-mutated.txt` |
| — | 还原 | `git checkout -- crates/world/src/world.rs crates/world/src/party.rs` | 上列用例全部 ok；`git status` 空 | `RC1RC2-restored.txt` |

RC-1 的 mutation 等价于 reviewer F-1 要求的「删掉 `settle_deaths` 里的 `party_del_member` 调用」：
把守卫改 `false` 后，死亡写入点不再触发 DelMember ⇒ 死亡者 `group_owner` 残留 1、团长侧零组队帧。
