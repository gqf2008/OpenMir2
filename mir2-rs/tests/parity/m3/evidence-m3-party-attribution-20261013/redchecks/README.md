# 改坏必红（redchecks）—— M3⑦-实现 4

对四处实现做定点改坏，**在同一命令下**复跑本片判据，确认从 GREEN 变 RED；
随后 `cp` 还原并复跑确认恢复 GREEN（`RC0-restore-green.txt`）。

| 文件 | 改坏内容 | 变红判据 |
| --- | --- | --- |
| `RC1-pickup-group-red.txt` | `bins/game-svr/src/main.rs`：`handle_ground_cmd` 的 `group_owner`/`group_member_actor_ids` 换回 `0`/`&[]` | `pickup_group_attribution_uses_registry` |
| `RC2-bag-dealing-red.txt` | `bins/game-svr/src/main.rs`：`handle_bag_cmd` 的 `dealing` 换回 `false` | `bag_takeoff_blocked_while_dealing` |
| `RC3-ham-group-and-RC4-gainexp-curry-red.txt` | ① `crates/world/src/combat.rs`：注释掉 `HAM_GROUP` 的 `IsGroupMember` 剔除；② `crates/world/src/world.rs`：把 `GainExp` 第二次距离判定改用 `CurrY`（"修掉" oracle 缺陷） | `ham_group_filter_matches_oracle` / `world_attack_excludes_group_mate` / `group_exp_replicates_oracle_curry_missing_bug` |
| `RC0-restore-green.txt` | 还原全部四处后 | 3 条 game-svr + 9 条 world 全 GREEN；`git status` 只剩本片改动 |

> 改坏脚本作用的对象是工作树（`cp` 备份 → 改 → 复跑 → `cp` 还原），
> 还原后 `grep -rn BREAK` 无命中、`git status` 无代码侧残留。
