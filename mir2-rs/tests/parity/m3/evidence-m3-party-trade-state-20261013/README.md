# M3⑦-实现 3 证据包 —— 交易会话状态机 + 6 个 `CM_DEAL*` 派发 + 交易 `SM_*` 帧

> 卡：`openmir2-m3-party-trade-state`（M3 逐系统对齐 ⑦ 的实现片 3）。
> 分支 `worktree-m3-party-trade-state`；base = `refs/heads/master` @ `9ff7ff064`（rebase 到当时 master；patch 的 head 见 walgit patch 条目）。
> 交付口径：`patch(--base refs/heads/master --head worktree-m3-party-trade-state)` + `status: needs-review`。

## 做了什么（DoD ②）

按 oracle 逐行落 Rust（`mir2-rs/bins/game-svr/src/trade.rs`，纯函数 + 事件，无 I/O、
无时间源、确定性）：

- **状态机**：`DealState`（`DealCreat`/`Dealing`/`DealLastTick`/`DealGolds`/`DealSuccess`/
  `DealItemList`/`AllowDeal`/`IsCanDeal`），与背包/金币同住在 bins 会话态（`GroundState`）。
- **6 条收包派发**：`CM_DEALTRY/ADDITEM/DELITEM/CANCEL/CHGGOLD/END`（`handle_deal_cmd`）。
  入帧映射照 `WorldServer.cs:820/857`：`nItemIdx`/`nGold` 取 **`recog`**（32 位）。
- **全部交易 `SM_*` 帧**：`SM_DEALMENU/DEALTRY_FAIL/DEALADDITEM_OK/FAIL/DEALDELITEM_OK/FAIL/
  DEALCANCEL/DEALREMOTEADDITEM/DEALREMOTEDELITEM/DEALCHGGOLD_OK/FAIL/DEALREMOTECHGGOLD/
  DEALSUCCESS` + 结算副产物 `SM_ADDITEM`（带 `ClientItem` 尾）/`SM_GOLDCHANGED`/
  `SM_WEIGHTCHANGED`/`SM_SYSMESSAGE`/`SM_MENU_OK`。
- **时间门**：`TryDealTime`（`Operate.cs:922`）、`DealOKTime`（`:1092`，双方任一）。
- **双向锁定**：`DealCreat.DealSuccess`（`:1096`）确认后才结算。
- **容量四检**（`:1102/1106/1112/1116`，全评估不短路）。
- **取消 / 掉线中断**：`DealCancel` 递归（`Base.cs:730-746`）、`GetBackDealItems`、
  `DealCancelA`（掉线，`WorldServer.cs:712`）、`Run()` 每 tick 自动取消（`Message.cs:30-36`）。
- **oracle 缺陷照搬**：`ClientChangeDealGold` 守卫回 `SM_DEALDELITEM_FAIL`（`:1055`）。

## 判据（DoD ③）

- `trade::tests` **18/18**（构造级，逐 oracle 分支；见 `03-tests.txt`）。
- **改坏必红 3 条**：容量检 / oracle 缺陷回帧 / 双向锁定（见 `redchecks/`，含改前改后读数）。
- 入帧映射与帧字段另由 `event_bodies_match_oracle_shapes` 逐字节钉住。

## 门禁（DoD ④，隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`）

见 `gates/`：
- `10-fmt.txt` rc=0
- `11-clippy.txt` rc=0（`-D warnings`）
- `13-workspace-test.txt` rc=0（**94 suites / 914 passed / 0 failed / 21 ignored**）
- `12-msgcodegen.txt` rc=0（604 条常量）

## 边界（DoD ④）

`git diff --name-only master <head> -- src sql MirClient` **为空**（`20-boundary.txt`）；
客户端 / 表结构 / 脚本语法 / 数据格式全冻结。

## 未做 / 残留（登记）

- **真机两侧同序列 diff=0**：属夹具卡（`openmir2-m3-party-trade-fixture-botsrv`）+ 独占窗口；
  本片不占端口、不起栈，如实标注。`-Side rust` 帧常量现已落地，夹具 preflight 可解。
- `SendAddItem`/`SendAddDealItem` 的 `ClientItem` 里 `GetUpgradeStdItem` desc 升级差量 +
  自定义名：与既有 `T-24` 同源，未实现（登记在 `trade.rs` 模块头）。
- `DealCancelA` 的 `Abil.HP = WAbil.HP`：无可观测路径（登记，见 `01-oracle-anchors.txt`）。
- 无临时目录 / 库 / 进程残留；未起服务端 / 客户端 / 假人；端口状态与开工前一致。

## 文件

| 文件 | 说明 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 行号锚点（实测） |
| `02-change-summary.txt` | 改动清单 |
| `03-tests.txt` | 判据原始输出（18/18） |
| `20-boundary.txt` | 边界证据 |
| `gates/` | 四门禁原始输出 |
| `redchecks/` | 改坏必红 3 条（含 README） |
| `SHA256SUMS.txt` | 本包逐件哈希（git blob 口径） |
