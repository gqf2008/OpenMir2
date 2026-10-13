# evidence-m3-party-deal-try-anchor-nits-20261013

卡 `openmir2-m3-party-deal-try-null-pose` —— M3⑦-3 `openmir2-m3-party-trade-state` 的
review Minor **M-1/M-2 收口（doc 锚订正片）**（reviewer-5 `3eb4a6a05`）。

> 作者 `openmir2-worker-21`（worktree `m3pt`）。本片**不 merge、不自签 review**。
> oracle/客户端/表结构/脚本/数据格式零改动（`20-boundary.txt`）。

## 1. 为什么（reviewer findings 与复核）

- **M-1（doc-line-number-drift，成立）**：`bins/game-svr/src/{main,trade}.rs` 的 `deal_try`
  内联注释把 oracle 行号写成 `:929`(Race)/`:934`(内层 if)/`:955-958`(非玩家 else)/
  `:928-929`(先取后读)，与卡内 ① 的实测锚不符（正确：`:932`/`:933`/`:936`/`:958-960`）。
- **M-2（doc-attribution-nit，复核为误判）**：reviewer 称 catch 属 `ProcessHumans`、
  应改为 `ProcessHumans`。实测 `src/GameSrv/Word/WorldServer.cs`：`ProcessHumans` 在 `:481`
  调 `ProcessPlayObjectData`（`:617`）；后者方法体 `:625-737`，其 `catch` 在 `:732-735`。
  `:734` 的日志串写作 `WorldServer::ProcessHumans` 是 oracle 的复制粘贴痕迹，**不改变
  包围方法名**。故注释保留 `WorldServer.ProcessPlayObjectData` 归属，并加一句说明该日志串，
  使后续审查不再误判。

## 2. oracle 锚（实测）

见 `01-oracle-anchors.txt`。要点：

| 位置 | 语义 |
| --- | --- |
| `PlayObject.Operate.cs:932` | `IActor poseObject = GetPoseCreate();` |
| `PlayObject.Operate.cs:933` | `if (poseObject.Race == ActorRace.Play)`（null ⇒ NRE） |
| `PlayObject.Operate.cs:936` | 内层 `if (targetPlayObject != this)` 无 `else` |
| `PlayObject.Operate.cs:938` | 对方 `GetPoseCreate() == this && !Dealing`（`faces_me`） |
| `PlayObject.Operate.cs:954/960` | 两处 `SM_DEALTRY_FAIL`；`:958-960` = 非玩家 `else` |
| `GameSrv/Word/WorldServer.cs:732-735` | `ProcessPlayObjectData` 的 catch（吞 NRE） |

## 3. 实现

`02-change-summary.txt`（full diff）。

- `mir2-rs/bins/game-svr/src/main.rs`：`deal_try_target_of` 文档块、两处测试文档块与断言
  消息的锚点订正为 `:932/:933/:936/:938/:958-960`，catch 引 `WorldServer.cs:732-735`
  并标注日志串痕迹。
- `mir2-rs/bins/game-svr/src/trade.rs`：`deal_try`/`DealTryTarget`/测试文档的同类订正；
  `Other{faces_me}` 补 `Operate.cs:938` 锚。
- `mir2-rs/bins/game-svr/tests/deal_try_oracle_anchors.rs`（新）：**锚漂移门**——把注释引用的
  行号对 oracle 原文逐个断言，并锁死 catch 归属；注释改回旧号码或改成 `ProcessHumans` ⇒ 必红。
- 零行为改动：只改注释 + 新增只读测试；`trade::`/`deal_` 既有 23 条用例逐条不变。

## 4. 判据 + 改坏必红

- 新增 1 条：`deal_try_oracle_anchors_match_source`（`03-tests.txt` [1]，ok）。
- RC1（`redchecks/RC1-mutated.txt`）：`main.rs` 的 `:958-960` 改回 `:955-958` ⇒ 门 FAILED
  （`main.rs 仍残留旧漂移锚 :955-958`）；还原（`RC1-restored.txt`）⇒ ok，sha256 与改前一致。
- RC2（`redchecks/RC2-mutated.txt`）：`trade.rs` 的 catch 归属改成 `WorldServer.ProcessHumans`
  ⇒ 门 FAILED（`应把 catch 归属写为 WorldServer.ProcessPlayObjectData`）；还原
  （`RC2-restored.txt`）⇒ ok，sha256 与改前一致。
- 回归：`game-svr --lib trade::` 20/20、`--bin game-svr deal_` 3/3（`03-tests.txt` [2][3]）。

## 5. 四门禁（交付树；`gates/`）

- `fmt --all -- --check` rc=0
- `clippy --workspace --all-targets -- -D warnings` rc=0
- `test --workspace` rc=0：**100 suites / 1160 passed / 0 failed / 22 ignored**（基线 = 99/1159）
- `msgcodegen --check` rc=0：**604** 条一致

## 6. 边界 / 未验证

- `git diff --name-only origin/master HEAD -- src sql MirClient` 空；merge-tree rc=0
  （`20-boundary.txt`）。
- 未验证：真机两侧同序列 `diff=0` 属 ⑦ 家族 DoD，归 `openmir2-m3-party-trade-align` + 独占窗口；
  本片零起栈/零占用端口。
