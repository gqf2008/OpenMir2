# 改坏必红（M3⑩-实现 1 / 世界侧元宝寄售）

口径：精确 mutation（Python 单次替换，命中数断言为 1）→ 跑命中用例（必 FAILED rc=101）→
`git restore --source=HEAD -- <file>` 还原 → 重跑（必 ok）。每条给原始件；还原后 `git status` 干净。

> 注（rebase 片 r2 2026-10-13，worktree `m3hp` / principal `openmir2-worker-20`）：RC-1/RC-2/RC-3 与还原复核均在
> **与 ⑩-2 同栈重放到 master `ddaca961c` 后的 head**（r2 重放结果 `43dc2c95c`，本 r2 证据提交叠其上）上重跑；
> `CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`（检出外）。原始件内的 panic 行号随 rebase 位移
> （如 RC-1 的 `main.rs`、RC-3 的 `selloff.rs:1492` 附近），**语义与断言不变**；被审实现文件
> 相对 rebase 前的 ⑩-1 内容逐字节不变（见 `20-boundary.txt` 末尾注）。

| # | 针对 | mutation | 命中用例（必红） | 原始件 |
|---|------|----------|------------------|--------|
| RC-1 | 派发接线 | `main.rs::handle_selloff_cmd` 的 `CM_SELLOFFEND => client_sell_off_end(...)` → `=> Vec::new()` | `game-svr::tests::selloff_cm_dispatch_emits_oracle_frames_on_both_connections` + `..._cancel_paths_dispatch_and_mutate_global_book`（**2 FAILED**） | `RC1-cm-selloffend-arm-removed-mutated.txt` |
| RC-2 | 缺陷①照搬 | `selloff.rs::client_buy_sell_off_item` 空槽中止 `return em.finish();` → `continue;` | 构造级 `buy_single_item_aborts_at_first_null_slot` / `buy_nine_items_still_aborts_at_out_of_range` / `buy_with_offline_seller_sets_flag1`（**3 FAILED**，rc=101）+ 进程级 `selloff_cm_dispatch_emits_oracle_frames_on_both_connections`（**1 FAILED**，rc=101） | `RC2-buy-first-null-slot-abort-removed-mutated.txt` |
| RC-3 | **F-1 返工**（成交成功支不得有负重守卫） | `selloff.rs::client_buy_sell_off_item` 的 `Some(_) => { if is_enough_bag(...) {` → `Some(tpl) => { if is_enough_bag(...) && weight_available(me, i32::from(tpl.weight)) {` | 构造级 `buy_enters_bag_even_when_weight_insufficient`（**1 FAILED**，rc=101） | `RC3-buy-weight-guard-readded-mutated.txt` |
| — | 还原 | 反向替换同一 mutation（或 `git restore --source=HEAD -- mir2-rs/bins/game-svr/src/{main,selloff}.rs`） | 上列用例全部 ok（24 + 2 passed）；相对 HEAD 只剩 F-1 修复自身的 diff | `RC-restored-ok.txt` |

RC-2 的 mutation 等价于「删掉 oracle 缺陷①的照搬」：首个空槽不再中止后，`SM_SELLOFFBUY_OK(23027)`
变为可达 ⇒ 断言「BUY_OK 不可达」的用例逐条红。RC-1 的 mutation 等价于「删掉 `CM_SELLOFFEND` 派发」，
上架不再入 `SellOffBook`、不回 `SM_SELLOFFEND_OK` ⇒ 进程级用例红（`left: []` vs `right: [767, 23016]`）。
RC-3 的 mutation 等价于「把 F-1 钉的原始缺陷（成交成功支多一条 oracle 没有的负重守卫）加回去」：
「背包未满 + 负重不足 + 元宝够」时物品不再入包（`left: 0` vs `right: 1`）⇒ F-1 新增向量必红。
**注意**：`client_cancel_sell_off_ing`（`selloff.rs:685`）的 `weight_available` 是**正确的**
（oracle `CancelSellOffIng` `PlayObject.Sale.cs:217` 确有 `IsAddWeightAvailable`），F-1 只删成交支的那一处。
