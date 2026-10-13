# 改坏必红（M3⑩-实现 1 / 世界侧元宝寄售）

口径：精确 mutation（Python 单次替换，命中数断言为 1）→ 跑命中用例（必 FAILED rc=101）→
`git restore --source=HEAD -- <file>` 还原 → 重跑（必 ok）。每条给原始件；还原后 `git status` 干净。

| # | 针对 | mutation | 命中用例（必红） | 原始件 |
|---|------|----------|------------------|--------|
| RC-1 | 派发接线 | `main.rs::handle_selloff_cmd` 的 `CM_SELLOFFEND => client_sell_off_end(...)` → `=> Vec::new()` | `game-svr::tests::selloff_cm_dispatch_emits_oracle_frames_on_both_connections` + `..._cancel_paths_dispatch_and_mutate_global_book`（**2 FAILED**） | `RC1-cm-selloffend-arm-removed-mutated.txt` |
| RC-2 | 缺陷①照搬 | `selloff.rs::client_buy_sell_off_item` 空槽中止 `return em.finish();` → `continue;` | 构造级 `buy_single_item_aborts_at_first_null_slot` / `buy_nine_items_still_aborts_at_out_of_range` / `buy_with_offline_seller_sets_flag1`（**3 FAILED**，rc=101）+ 进程级 `selloff_cm_dispatch_emits_oracle_frames_on_both_connections`（**1 FAILED**，rc=101） | `RC2-buy-first-null-slot-abort-removed-mutated.txt` |
| — | 还原 | `git restore --source=HEAD -- mir2-rs/bins/game-svr/src/{main,selloff}.rs` | 上列用例全部 ok（23 + 2 passed）；`git status` 干净 | `RC-restored-ok.txt` |

RC-2 的 mutation 等价于「删掉 oracle 缺陷①的照搬」：首个空槽不再中止后，`SM_SELLOFFBUY_OK(23027)`
变为可达 ⇒ 断言「BUY_OK 不可达」的用例逐条红。RC-1 的 mutation 等价于「删掉 `CM_SELLOFFEND` 派发」，
上架不再入 `SellOffBook`、不回 `SM_SELLOFFEND_OK` ⇒ 进程级用例红（`left: []` vs `right: [767, 23016]`）。
