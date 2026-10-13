# M3⑦ Minor M-2 收口证据包 —— GainExp 缺失团长态对齐 oracle NPE 吞错

> 卡：`openmir2-m3-party-gainexp-npe`（来源 = `openmir2-m3-party-attribution` 合并检查点携带的
> 非阻塞 Minor M-2）。分支 `worktree-m3-party-gainexp-npe`（worktree `m3pt` / principal
> `openmir2-worker-21`）。base = `refs/heads/master` @ `d5be14f40`。
> 交付口径：`patch(--base refs/heads/master --head worktree-m3-party-gainexp-npe)`
> + `status: needs-review`。

## 为什么开这张卡

⑦-4 交付时把 `GainExp` 组队支的**缺团长态**登记为「边界」（Minor M-2）：Rust 对缺失团长走
`unwrap_or_default()` ⇒ 空成员表 ⇒ `sumlv == 0` ⇒ 退回**单人满经验**；oracle 同态下
`ActorMgr.Get(GroupOwner)` 取不到 ⇒ `GroupMembers` 空引用抛异常、被 `catch` 吞掉 ⇒
**本次零经验/零授奖**。两者行为不同且**可达**（`World::leave_world` 移除实体时不清同组成员的
`group_owner`）⇒ 违反 §1 硬约束③「行为与 C# 参照实现等价」。本卡做口径对齐，不改 oracle。

## DoD ① oracle 锚定

见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（行号为在本仓 `src/` 实测）：
`PlayObject.Base.cs:764-821`（`:771` `GroupOwner != 0`、`:775` `ActorMgr.Get`、
`:776`/`:792` `GroupMembers` 解引用、`:818-821` `catch` 吞错）、调用点
`PlayObject.cs:1632-1653`（`:1652` `GainExp(monsterExp)`）。

## DoD ② 实现

见 [`02-change-summary.txt`](02-change-summary.txt)。仅 `mir2-rs/crates/world/**`：

- `world.rs`：`World::gain_exp` 组队支 fail-closed —— 团长不在注册表 ⇒ 归还 `need_exps`、
  返回 0、不追加 `exp_awards`；其余分支（含 `|CurrX-x|` 漏 `CurrY` 的 oracle 缺陷照搬）逐字节不变。
- `tests/party_attribution.rs`：新增缺团长态判据 + 团长在场正面控制。

零并行、tick 顺序确定、零 RNG 消费。

## DoD ③ 判据 + 改坏必红

- 新增 `crates/world/tests/party_attribution.rs::group_owner_missing_matches_oracle_npe`
  （团长在场控制 + 团长离场悬空态）；本文件 **10/10**。原始输出见 [`03-tests.txt`](03-tests.txt)。
- **改坏必红**：见 [`redchecks/`](redchecks/) —— 改回 `unwrap_or_default()` ⇒
  `left: 600 / right: 0` 必红（`RC1`）；还原后 GREEN（`RC0`），`sha256sum world.rs` 与备份一致。

## DoD ④ 四门禁 + 边界

隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`，本 head（base `d5be14f40`）上实测：

| 门禁 | 文件 | 读数 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `gates/10-fmt.txt` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | `gates/11-clippy.txt` | rc=0 |
| `cargo test --workspace` | `gates/13-workspace-test.txt` | rc=0，**98 suites / 1014 passed / 0 failed / 21 ignored** |
| `cargo run -p msgcodegen -- --check` | `gates/12-msgcodegen.txt` | rc=0，604 条常量 |

> 基线 `d5be14f40` 为 98 suites / 1013 passed（⑨-3 合并检查点读数）⇒ 本卡 **+1**（新增判据），
> 无回归、无其它套件变动。边界 [`20-boundary.txt`](20-boundary.txt)：`src sql MirClient`
> **零改动**；代码侧净 diff 仅 `crates/world/` 2 文件。

## 未做 / 残留（登记）

- **真机两侧同序列 diff=0**：本卡为纯离线口径对齐，真机面归既有 ⑦ 对齐夹具卡
  （`openmir2-m3-party-trade-align` / `openmir2-m3-party-trade-fixture-botsrv`）+ 独占窗口；
  本卡**未起栈、未占端口** ⇒ 真机半边 **`not_verified`**。
- ⑦-4 携带的另一条 Minor **M-1**（`deal_last_tick` 生产接线的进程级用例）不在本卡写集
  （属 ⑦-5 夹具面 `tests/parity/m3/**`），保持原登记、未动。
- 无临时目录 / 库 / 进程残留；未起服务端 / 客户端 / 假人；端口状态与开工前一致。

## 文件

| 文件 | 说明 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 行号锚点（实测）与关键源码 |
| `02-change-summary.txt` | 改动清单（逐文件） |
| `03-tests.txt` | 本卡判据原始输出（10/10） |
| `04-eolcheck.txt` | `eolcheck` 原始输出 |
| `20-boundary.txt` | 边界证据 |
| `gates/` | 四门禁原始输出 |
| `redchecks/` | 改坏必红（含 README 与还原读数） |
| `SHA256SUMS.txt` | 本包逐件哈希（仓库 LF/git blob 口径） |
