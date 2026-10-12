M3④-实现 C 第 ③ 片（怪物帧面）—— 改坏必红（4 条，全部 rc=101）
================================================================

每条都：① 备份原文件 → ② 单点变异 → ③ 跑指定用例 → ④ 还原并核对 sha256 与备份一致。

| 编号 | 文件 | 变异 | 期望变红用例 | 文件 |
| --- | --- | --- | --- | --- |
| RC-C3-A | `bins/game-svr/src/mongen.rs` | `WorldMonsterSink::spawn_monster` 删掉怪物展示层登记 | `mongen::tests::world_monster_sink_registers_monster_display` | `rc-C3-A-display-not-registered.txt` |
| RC-C3-B | `bins/game-svr/src/downlink.rs` | `monster_display` 用 `mp` 高字节代替 `HUtil32.LoByte(mp)` | `downlink::tests::monster_display_matches_oracle_fields` | `rc-C3-B-lobyte-instead-of-highbyte.txt` |
| RC-C3-C | `bins/game-svr/src/main.rs` | `drain_aoi_downlink` 不再用世界 `ai.dir` 刷新怪物出现帧的 `Dir` | `tests::aoi_show_emits_monster_turn_pair_with_world_dir` | `rc-C3-C-monster-dir-stale.txt` |
| RC-C3-D | `bins/game-svr/src/main.rs` | `new_player_entity` 不置 `WantRefMsg=true` | `tests::new_player_entity_sets_want_ref_msg` | `rc-C3-D-wantrefmsg-not-set.txt` |

还原核对（源码 sha256，变异前 = 变异还原后）：

    mongen.rs            = 1e9eeed5b93980e7ad2fca40e743eb7dbe84dd1ee817ea10c8fcca69d1c2b545
    downlink.rs          = 8f2ce51431162979774f286234e997794ef22ad37eed4646318cb2ae87a671f8
    main.rs              = 7142532f0a1c07ad7d8e537c7f365f85f3517019e5642e0b2363de5fc21df035
    （三份与 E:/tmp/m2svr-c3-redcheck/*.orig 逐字节相同）
