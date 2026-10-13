# M3⑦-实现 4 · M-1 收口证据包 —— deal_last_tick 生产取数被用例咬住

> 卡：`openmir2-m3-party-deal-last-tick-bite`（M3⑦-实现 4 父卡 `openmir2-m3-party-attribution`
> 的非阻塞 Minor **M-1**；reviewer-6 在 `c16a7365` 提出）。
> 分支 `worktree-m3-party-deal-last-tick-bite`（worktree `m3pt` / principal `openmir2-worker-21`）。
> base = `refs/heads/master` @ `5d499cd9c`（= origin/master = walgit `refs/heads/master`）。
> 交付口径：`patch(--base refs/heads/master --head worktree-m3-party-deal-last-tick-bite)`
> + `status: needs-review`。**不 merge、不自签 review。**

## 背景（为什么有这张卡）

⑦-4（已合入 master `751a20a06`）把 `PlayObject.Dealing` / `DealLastTick` 接到真交易会话，
但 reviewer-6 的 Minor M-1：**该接线没有任何用例/红检咬住**——`main.rs` 内 4 处
`handle_ground_cmd(...)` 测试调用全部直接传 `0`，把调用点换回 `0` 不会变红。

本片收口方式：把两个取值**内聚进 `handle_ground_cmd`**（调用点只传 `&trade_sessions`），
并新增一条走**同一生产函数**的 `CM_DROPITEM` 进程级判据。

## DoD ① oracle 锚定

见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（行号在本仓 `src/` 实测）：
`PlayObject.Operate.cs:246/265-269`（`ClientDropItem` 与 3000ms 时间门）、
`PlayObject.cs:122`（`DealLastTick` 字段）、`BaseObject.cs:541-548`（StdMode40 原地耐久扣减）。

## DoD ② 实现

见 [`02-change-summary.txt`](02-change-summary.txt)。净 diff 仅 `mir2-rs/bins/game-svr/src/main.rs`：

1. `handle_ground_cmd` 形参 `dealing: bool, deal_last_tick: i32` ⇒
   `trade_sessions: &HashMap<String, game_svr::trade::DealState>`，取值在函数内完成；
   `sim_loop` 调用点改传 `&trade_sessions`。**生产数值逐字节不变**。
2. 新增 `tests::drop_blocked_by_fresh_deal_last_tick`（进程级，走 `CM_DROPITEM` 派发分支）。

遍历/顺序：同一 `ItemList` 顺序，零并行、tick 确定；未新增/改动 oracle 行为。

## DoD ③ 判据 + 改坏必红

- 新增 `game-svr` 进程级判据 **1/1**：`drop_blocked_by_fresh_deal_last_tick`
  （新鲜 `DealLastTick` ⇒ 不进丢弃体 ⇒ `Dura` 不变；陈旧 ⇒ 进体 ⇒ `Dura 5000→3000`）。
  原始输出见 [`03-tests.txt`](03-tests.txt)。
- 既有 4 处 `handle_ground_cmd` 测试调用同步改传空表（等价 `false`/`0`），全绿。
- **改坏必红 1 条**：见 [`redchecks/`](redchecks/)。改坏读数
  `left: 3000 / right: 5000`、`FAILED. 0 passed; 1 failed`；`cp` 还原后 `sha256` 一致、复跑 GREEN。

## DoD ④ 四门禁 + 边界

隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`，在 head `9dda06f7f` 上实测（工作区仅多本证据目录）：

| 门禁 | 文件 | 读数 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `gates/10-fmt.txt` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | `gates/11-clippy.txt` | rc=0 |
| `cargo test --workspace` | `gates/13-workspace-test.txt` | rc=0，**98 suites / 1025 passed / 0 failed / 21 ignored** |
| `cargo run -p msgcodegen -- --check` | `gates/12-msgcodegen.txt` | rc=0，604 条常量 |

（base `5d499cd9c` 读数 = 98 suites / 1024 passed ⇒ 本片 **+1**，无回归。）

边界 [`20-boundary.txt`](20-boundary.txt)：`git diff --name-only origin/master HEAD -- src sql MirClient`
**为空**；`merge-tree origin/master HEAD` **rc=0**；净 diff 仅
`mir2-rs/bins/game-svr/src/main.rs` + 本证据目录。

## 未做 / 残留（登记）

- **真机两侧同序列 diff=0**：属 ⑦ 对齐夹具卡（`openmir2-m3-party-trade-align`）+ 独占窗口；
  本片**未起栈、未占端口** ⇒ 真机半边 **`not_verified`**（如实标注）。
- 本片只收口 M-1（测试接线），不改任何 drop/交易行为；⑦-4 的另一条 Minor M-2 由
  `openmir2-m3-party-gainexp-npe`（head `2475d38c3`，待审）处理。
- 无临时目录 / 库 / 进程残留；端口状态与开工前一致。

## 文件

| 文件 | 说明 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 行号锚点（实测）与关键源码 |
| `02-change-summary.txt` | 改动清单（逐条） |
| `03-tests.txt` | 新增判据原始输出 |
| `04-eolcheck.txt` | `eolcheck` 原始输出 |
| `20-boundary.txt` | 边界证据 |
| `gates/` | 四门禁原始输出 |
| `redchecks/` | 改坏必红（mutated/restored + README） |
| `SHA256SUMS.txt` | 本包逐件哈希（仓库 LF/git blob 口径） |
