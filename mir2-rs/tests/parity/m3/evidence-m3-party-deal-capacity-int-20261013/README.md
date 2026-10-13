# M3⑦-3 · M-3 收口证据包 —— 容量四检用 oracle 精确 int 算术（差量可负）

> 卡：`openmir2-m3-party-deal-capacity-int`（M3⑦-3 父卡 `openmir2-m3-party-trade-state`
> 的**非阻塞 Minor M-3**；reviewer-6 在 `fbb382aa056a` 提出）。
> 分支 `worktree-m3-party-deal-capacity-int`（worktree `m3pt` / principal `openmir2-worker-21`）。
> base = `refs/heads/master` = origin/master = walgit master = `ad0bba3016`.
> 交付口径：`patch(--base refs/heads/master --head worktree-m3-party-deal-capacity-int)`
> + `status: needs-review`。**不 merge、不自签 review。**

## 背景（为什么有这张卡）

⑦-3（已合入 master `7febf884`）按 oracle 落了 `ClientDealEnd` 的容量四检。reviewer-6 的 Minor M-3：
`PlayObject.Operate.cs:1101`/`:1111` 的两条背包容量检是 **C# int 减法**，差量**可为负**；
Rust 侧用 `usize::saturating_sub` 把负差量钳成 `0`，在「背包已超上限 + 交易栏空」这一状态下
把 oracle 的**拒绝**改成了**放行**（oracle `-1 < 0` 真，Rust `0 < 0` 假）。

本片把这两条检改成精确整型算术（新增 `bag_capacity_left`），金币两检（本就 i32 精确）不动，
其余分支/帧面/顺序逐字节不变。

## DoD ① oracle 锚定

见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（行号在本仓 `src/` 实测）与
[`01b-oracle-source-excerpt.txt`](01b-oracle-source-excerpt.txt)（源码摘录）：
`Operate.cs:1101`/`:1106`/`:1111`/`:1116` 四检；常量 `Grobal2.MaxBagItem` = 本仓
`bag.rs:137 MAX_BAG_ITEM = 46`。

## DoD ② 实现

见 [`02-change-summary.txt`](02-change-summary.txt)。净 diff 仅 `mir2-rs/bins/game-svr/src/trade.rs`：

- 新增 `fn bag_capacity_left(max_bag_item: usize, used: usize) -> i64`
  （`as i64` 精确减法，注释钉死「禁止 saturating_sub」）；
- 两条背包容量检改用它；金币两检（i32）不动；
- 四检仍**全评估**、红提示文案与顺序、`DealCancel` 链、结算搬运序不变。

零并行、tick 确定；未新增/改动 oracle 行为（oracle 冻结面零改动）。

## DoD ③ 判据 + 改坏必红

- 新增构造级判据 **2/2**（[`03-tests.txt`](03-tests.txt)，`trade::` 全量 **22 passed**）：
  - `deal_end_over_capacity_my_bag_with_empty_opposite_deal_list`（甲 47 > 46、乙交易栏空）；
  - `deal_end_over_capacity_human_bag_with_empty_my_deal_list`（对称侧）。
- **改坏必红 1 条**：[`redchecks/RC1-mutated.txt`](redchecks/RC1-mutated.txt) —— 把
  `bag_capacity_left` 体改回 `max_bag_item.saturating_sub(used) as i64` ⇒ 两条判据 **FAILED**
  （`test result: FAILED. 0 passed; 2 failed`）。还原见 [`redchecks/RC1-restored.txt`](redchecks/RC1-restored.txt)
  与 [`05-hashes.txt`](05-hashes.txt)：`sha256` 与改坏前一致（`1b47b3ca…`）、复跑 22/22 GREEN。

## DoD ④ 四门禁 + 边界

隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`；在 head `abb053b91`（代码提交）实测：

| 门禁 | 文件 | 读数 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `gates/fmt.txt` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | `gates/clippy.txt` | rc=0 |
| `cargo test --workspace` | `gates/test.txt` | rc=0，**100 suites / 1176 passed / 0 failed / 22 ignored** |
| `cargo run -p msgcodegen -- --check` | `gates/msgcodegen.txt` | rc=0，604 条常量 |

（base `ad0bba301` 合并检查点读数 = 100 suites / 1174 passed ⇒ 本片 **+2**，无回归。）

边界 [`20-boundary.txt`](20-boundary.txt)：`git diff --name-only origin/master HEAD -- src sql MirClient`
**为空**；`-- mir2-rs/crates/protocol mir2-rs/crates/storage` **为空**；净 diff 仅
`mir2-rs/bins/game-svr/src/trade.rs` + 本证据目录；`merge-tree origin/master HEAD` **rc=0**。

## 未做 / 残留（登记）

- **真机两侧同序列 diff=0**：属 ⑦ 对齐夹具卡（`openmir2-m3-party-trade-align`）+ 独占窗口；
  本片**未起栈、未占端口** ⇒ 真机半边 **`not_verified`**（如实标注）。
- ⑦-3 其余非阻塞 Minor 的处置：**M-2**（`name_matches` 大小写口径）保持现状——数据域为
  GB2312/ASCII，`to_lowercase` 与 .NET OrdinalIgnoreCase 在该域等价，改 `eq_ignore_ascii_case`
  对非 ASCII 大小写对反而更远 ⇒ 不动，留痕；**M-4**（旧证据 head 漂移）在父卡线程以 comment 登记；
  **M-5**（`ClientItem` 尾升级/自定义名缺口）与 T-24 同源，**待 owner 裁定**后另片。
- 无临时目录 / 库 / 进程残留；端口状态与开工前一致。

## 文件

| 文件 | 说明 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 行号锚点（实测） |
| `01b-oracle-source-excerpt.txt` | oracle 源码摘录（1097-1120 行） |
| `02-change-summary.txt` | 改动清单（逐条） |
| `03-tests.txt` | 新增判据原始输出 |
| `04-eolcheck.txt` | `eolcheck` 原始输出 |
| `05-hashes.txt` | 改坏/还原哈希 + git status |
| `20-boundary.txt` | 边界证据 |
| `gates/` | 四门禁原始输出 |
| `redchecks/` | 改坏必红（mutated/restored） |
| `SHA256SUMS.txt` | 本包逐件哈希（仓库 LF/git blob 口径） |
