# M3⑦-实现 4 证据包 —— 归属/分配的组队与交易分支

> 卡：`openmir2-m3-party-attribution`（M3 逐系统对齐 ⑦ 的实现片 4）。
> 分支 `worktree-m3-party-attribution`（worktree `m3pt` / principal `openmir2-worker-21`）。
> base = `refs/heads/master` @ `0f9fe9997`（交付前 `git merge origin/master` 对齐；head 见 walgit `patch` 条目）。
> 交付口径：`patch(--base refs/heads/master --head worktree-m3-party-attribution)` + `status: needs-review`。

## DoD ① oracle 锚定

见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（行号为在本仓 `src/` 实测）：
`PlayObject.Base.cs:38-54/55-58/764-818`、`PlayObject.Attack.cs:686-698`、
`PlayObject.cs:5158-5175`、`PlayObject.Operate.cs:251-269/499-535`、`Grobal2.cs:35`。

## DoD ② 实现

见 [`02-change-summary.txt`](02-change-summary.txt)。四处去硬编码 / 补分支：

1. **拾取归属接真注册表**：`handle_ground_cmd` 的 `group_owner: 0`/`&[]` ⇒ 从 `world` 取
   角色 `GroupOwner` 与该团长 `GroupMembers`（oracle `ClientPickUpItemIsOfGroup`）。
2. **交易态接真会话**：`handle_bag_cmd` 的 `dealing: false` 与 `handle_ground_cmd` 的
   `deal_last_tick: 0` ⇒ 从 `trade_sessions` 传入（oracle `!Dealing` / `DealLastTick` 门）。
3. **`HAM_GROUP` 的 `IsGroupMember` 过滤**：`is_attack_target_player_grouped` 新增剔除支，
   `World::attack_once` 传团长成员表。
4. **`GainExp` 组队分配**：`World::gain_exp` 逐行照 `Base.cs:764-818`（含 **两次都比 `CurrX`、
   漏 `CurrY`** 的 oracle 缺陷照搬）+ `win_exp` 拆分。

遍历/取数顺序与 oracle 一致（成员表序），零并行、tick 顺序确定；oracle 缺陷照搬（§1 硬约束③）。

## DoD ③ 判据 + 改坏必红

- 新增 `crates/world/tests/party_attribution.rs` **9/9**；`bins/game-svr` 新增 2 条进程级判据；
  既有 `party_vectors` 15/15、`trade` 既有用例、`combat_skill_vectors` 等全绿。
  原始输出见 [`03-tests.txt`](03-tests.txt)。
- **改坏必红 4 处**：见 [`redchecks/`](redchecks/)（含改前改后读数与还原后 GREEN）。

## DoD ④ 四门禁 + 边界

隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`，合并树（base + master `0f9fe9997`）上实测：

| 门禁 | 文件 | 读数 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `gates/10-fmt.txt` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | `gates/11-clippy.txt` | rc=0 |
| `cargo test --workspace` | `gates/13-workspace-test.txt` | rc=0，**96 suites / 977 passed / 0 failed / 21 ignored** |
| `cargo run -p msgcodegen -- --check` | `gates/12-msgcodegen.txt` | rc=0，604 条常量 |

边界 [`20-boundary.txt`](20-boundary.txt)：`git diff --name-only origin/master -- src sql MirClient`
**为空**（oracle/客户端/表结构/数据格式冻结）；本片净 diff 仅在
`mir2-rs/crates/world/**` + `mir2-rs/bins/game-svr/**` + 本证据目录。

## 未做 / 残留（登记）

- **真机两侧同序列 diff=0**：属 ⑦ 对齐夹具卡（`openmir2-m3-party-trade-fixture-botsrv` /
  `openmir2-m3-party-trade-align`）+ 独占窗口；本片**未起栈、未占端口** ⇒ 真机半边
  **`not_verified`**（如实标注）。本片把 ⑦-3 已落地的 `CM_*` 侧与 `crates/world` 的分配分支补齐，
  `-Side rust` 的组队/交易常量与派发面现已在 master。
- `ExpHitter.Master`（宝宝击杀给主人）与组队分配的交互：oracle 对每个 `RM_PLAYERKILLMONSTER`
  获得者各跑一次 `GainExp`，本片按同结构实现；主人与宠物同组时的重叠归属未另测（登记）。
- `GainExp` 的 `LimitExpLevel` 固定值支未建模（`LimitExpLevel=1000`，现网不触发，与既有口径一致）。
- 无临时目录 / 库 / 进程残留；未起服务端 / 客户端 / 假人；端口状态与开工前一致。

## 文件

| 文件 | 说明 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 行号锚点（实测）与关键源码 |
| `02-change-summary.txt` | 改动清单（逐文件） |
| `03-tests.txt` | 判据原始输出 |
| `04-eolcheck.txt` | `eolcheck` 原始输出（16/16） |
| `20-boundary.txt` | 边界证据 |
| `gates/` | 四门禁原始输出 |
| `redchecks/` | 改坏必红 4 处（含 README 与还原读数） |
| `SHA256SUMS.txt` | 本包逐件哈希（仓库 LF/git blob 口径） |
