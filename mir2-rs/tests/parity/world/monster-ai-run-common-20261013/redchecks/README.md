# 改坏必红（本片 6 条：原片 4 + 返工 2）

驱动脚本（临时、未入库）：`E:/tmp/m2svr-redcheck-monsterai.py`。逐条：改一处 → 编译并跑
对应单测 → 期望 `test result: FAILED`（exit 101）→ 还原 → `git status` 干净。

| 编号 | mutation（文件 / 从 → 到） | 目标用例 | 结果 |
| --- | --- | --- | --- |
| RC-M1 | `monai.rs`：`if hp < max_hp && health_tick >= health_fill_time {` → `if hp < max_hp {` | `base_recovery_heals_hp_at_fill_time_and_resets_tick` | FAILED（`未到 HealthFillTime 不得回血`：left 12 right 10） |
| RC-M2 | `monai.rs`：`if health_tick < -health_fill_time && hp > 1 {` → `if health_tick < -health_fill_time {` | `health_tick_negative_costs_one_hp_and_is_gated` | FAILED（`HP == 1 时不得再扣`） |
| RC-M3 | `world.rs`：`if (now - e.last_hiter_tick) > 30_000 \|\| gone.contains(&h) {` → `if gone.contains(&h) {` | `expire_hitters_uses_wall_clock_last_hiter_30s_exp_hitter_6s` | FAILED（`31s > 30s ⇒ 过期`） |
| RC-M4 | `monai.rs`：`    target_cret_cleanup(world, actor_id, now);` → `    let _ = now;` | `target_cret_cleanup_covers_age_death_map_and_distance` | FAILED |
| **RC-I1**（返工·reviewer-6 `I-1`） | `monai.rs` 恢复段：`i32::from(hp) + n`（int 语义、截断到 MaxHP）→ `hp + n`（`u16` 加） | `base_recovery_high_max_hp_does_not_overflow_u16` | FAILED（`attempt to add with overflow` @`monai.rs:617`，exit=101） |
| **RC-M1b**（返工·reviewer-6 `M-1`） | `monai.rs` 清理段：`clear_target_creature(world, actor_id, target_id)`（`ClearTargetCreat`）→ `del_target_creature(world, actor_id)`（`DelTargetCreat`） | `target_cret_cleanup_also_removes_visible_actor` | FAILED（`目标应从 VisibleActors 移除（ClearTargetCreat）`，exit=101） |

原始输出见同目录 `rc-M1-*.txt` … `rc-M4-*.txt` + `rc-I1-*.txt` + `rc-M1b-*.txt`
（每条均含 `mutation:` 行、`exit=101`、`verdict=RED`、`panicked at` / `test result: FAILED`）。
还原后 `git status --short` 只剩本片预期改动文件（无 mutation 残留；`sha256sum` 与备份逐字节相同）。
