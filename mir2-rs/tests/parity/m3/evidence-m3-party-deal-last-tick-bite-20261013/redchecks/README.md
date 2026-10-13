# 改坏必红（redchecks）—— M3⑦-实现 4 · M-1 收口

对本片唯一的生产逻辑改动（`handle_ground_cmd` 内的 `deal_last_tick` 取数）做定点改坏，
**同一命令**复跑本片判据，确认从 GREEN 变 RED；随后 `cp` 还原并复跑确认恢复 GREEN。

命令（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`）：

    cargo test -p game-svr drop_blocked_by_fresh_deal_last_tick

| 文件 | 改坏内容 | 读数 |
| --- | --- | --- |
| `RC1-mutated.txt` | `mir2-rs/bins/game-svr/src/main.rs`：把 | 第一次调用即过门 ⇒ `Dura` 从 5000 变 3000 |
| | `let deal_last_tick = trade_sessions.get(chr_name).map_or(0, …);` | `assertion left == right failed … left: 3000 / right: 5000` |
| | 改回 `let deal_last_tick = 0;`（= ⑦-4 之前调用点写死的 0） | `test result: FAILED. 0 passed; 1 failed` |
| `RC1-restored.txt` | `cp` 还原（`sha256sum` 与改坏前一致，见下） | `test tests::drop_blocked_by_fresh_deal_last_tick ... ok` / `1 passed; 0 failed` |

还原同一性：

    $ sha256sum mir2-rs/bins/game-svr/src/main.rs   # 改坏前
    a00ba28604a57446c869bbb4b96dd16b255611a8c182f4db3bac784cde49d5bc
    $ sha256sum mir2-rs/bins/game-svr/src/main.rs   # cp 还原后
    a00ba28604a57446c869bbb4b96dd16b255611a8c182f4db3bac784cde49d5bc   (IDENTICAL)

> 改坏脚本作用对象是工作树（`cp` 备份 → 改 → 复跑 → `cp` 还原）；还原后 `git status`
> 仅剩本证据目录未跟踪，代码侧零残留。
