# M3⑦-实现 3 交易会话状态机 —— 改坏必红（RC1–RC3）

判据本体 = `mir2-rs/bins/game-svr/src/trade.rs` 的 `#[cfg(test)] mod tests`（18 条，见
`03-tests.txt`）。三条 red check 各自把**一处实现**改坏后跑**对应用例**必红；跑完即从备份还原，
还原后 `sha256` 与改坏前一致（`eedfd9576505b2121e0fc0f5ede0bf1899608f7dd0a33bae11de2f1b8072bd71`）。

运行命令（隔离 `CARGO_TARGET_DIR`）：

```sh
CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target cargo test -p game-svr --lib trade::tests::<用例>
```

| 编号 | 改坏点（oracle 锚） | 用命令 | 读数 |
| --- | --- | --- | --- |
| RC1 | `ClientDealEnd` 容量四检的「我的背包装不下对方物品」（`PlayObject.Operate.cs:1102`）置 `if false` | `deal_end_capacity_check_cancels` | 改前：`1 passed`；改后：`FAILED`（`容量不足必提示`） |
| RC2 | `ClientChangeDealGold` 的「`DealGolds>0 && CanNotGetBackDeal`」守卫回帧由 oracle 原样的 `SM_DEALDELITEM_FAIL(678)`（`Operate.cs:1055`）改成 `SM_DEALCHGGOLD_FAIL(685)` | `change_deal_gold_ok_and_fail` | 改前：`1 passed`；改后：`FAILED`（`left [(1,685),…] != right [(1,678),…]`） |
| RC3 | `ClientDealEnd` 的双向锁定门 `if (DealCreat.DealSuccess)`（`Operate.cs:1096`）置 `if true`（未确认也结算） | `deal_end_first_side_waits` | 改前：`1 passed`；改后：`FAILED`（多出 `SM_DEALSUCCESS(687)` 两条） |

原始输出见同目录 `RC1-*.txt` / `RC2-*.txt` / `RC3-*.txt`。还原后 `git status` 只余本片
应交付的 4 个改动文件 + 新文件 `trade.rs`（无临时残留）。
