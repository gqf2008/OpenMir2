# evidence-m3-party-deal-try-anchor-scope-20261013

卡 `openmir2-m3-party-deal-try-null-pose`（M3⑦-3 · M-1 收口）—— reviewer-5 `494ba02e5`
的 **O-1 收口（锚门收窄于 `deal_try` 注释块）**。

> 作者 `openmir2-worker-21`（worktree `m3pt`）。本片**不 merge、不自签 review**。
> oracle / 客户端 / 表结构 / 脚本 / 数据格式零改动（`20-boundary.txt`）。
> 零行为改动：只改**只读门测试**（`bins/game-svr/tests/deal_try_oracle_anchors.rs`），生产代码一行未动。

## 1. 为什么（reviewer finding）

reviewer-5 `494ba02e5`（对已合入 `acfab176a` 的 approve）留 1 条非阻塞 O-1：

> Rust 侧锚检查用 `src.contains(":932")` 等**全文子串匹配**，未限定在 `deal_try` 注释块内；
> 未来别处注释出现同串会产生假绿/假红。建议收紧为定位 `deal_try` 段（或注释行）后再断言。

本片照此收口：断言面从「整份 `main.rs` / `trade.rs`」收窄到「**`deal_try` 相关注释块**」。
这既消除**假红**（无关特性注释里的 `:929`/`:934` 不再误伤，RC2 正控），也保住**假绿防线**
（`deal_try` 注释块内的回退锚仍必被咬住，RC1）。

## 2. 实现（`02-change-summary.txt` full diff）

新增 `fn deal_try_comment_bytes(src) -> String`：

- 只取**整行注释**（`trim_start` 后以 `//` 起头的连续行成块）；
- 块内（小写）含 `DEAL_TRY_COMMENT_MARKERS` 任一标记才计入断言面：
  `clientdealtry` / `deal_try` / `getposecreate` / `poseobject` / `processplayobjectdata` / `dealtry`；
- 主判据 `deal_try_oracle_anchors_match_source` 改为在这段文本上做正锚（`:932/:933/:936/:938/:958-960`、
  `WorldServer.cs:732-735`、`WorldServer.ProcessPlayObjectData`）与反锚（`:929/:928-929/:934/:955-958`）断言；
- 提取过短即 fail-closed 报红（标记漂移 ⇒ 正锚断言必红，不会静默恒绿）。

新增 `redcheck_anchor_scope_is_deal_try_only`（4 态自红检）：[1] 无关注释旧锚不计入、
[2] 代码/字符串同串不计入、[3] `deal_try` 块内回退锚必被咬住、[4] 无标记注释块整体不计入。

## 3. oracle 锚（实测，`01-oracle-anchors.txt`）

| 位置 | 语义 |
| --- | --- |
| `PlayObject.Operate.cs:932` | `IActor poseObject = GetPoseCreate();` |
| `PlayObject.Operate.cs:933` | `if (poseObject.Race == ActorRace.Play)`（null ⇒ NRE） |
| `PlayObject.Operate.cs:936` | 内层 `if (targetPlayObject != this)` 无 `else` |
| `PlayObject.Operate.cs:938` | 对方 `GetPoseCreate() == this && !Dealing`（`faces_me`） |
| `PlayObject.Operate.cs:958-960` | 非玩家 `else` ⇒ `SM_DEALTRY_FAIL` |
| `GameSrv/Word/WorldServer.cs:732-735` | `ProcessPlayObjectData` 的 `catch`（吞 NRE；`:734` 日志串写作 `ProcessHumans` 系 oracle 复制粘贴痕迹） |

## 4. 判据 + 改坏必红（`03-tests.txt` / `redchecks/`）

- `deal_try_oracle_anchors_match_source` **ok**、`redcheck_anchor_scope_is_deal_try_only` **ok**
  （本门 2/2）。
- **RC1（scoped 回归必红）**：`main.rs` 的 `deal_try` 注释块内 `:958-960` 改回 `:955-958`
  ⇒ 门 **FAILED**（`main.rs 仍残留旧漂移锚 :955-958`，rc=101）；还原 ⇒ ok，`main.rs`
  sha256 `fd296d71…` 与改前一致（`RC1-restored.txt`）。
- **RC2（正控：假红已消除）**：把**无 `deal_try` 标记**的无关注释
  `// … Operate.cs:929 / :934 …` 注入 `main.rs` ⇒ 门仍 **ok**（rc=0）——证明旧实现的
  整文件 `contains(":929")` 会造成的假红已被收窄规则消除；还原 ⇒ ok，sha256 一致
  （`RC2-restored.txt`）。
- 回归：`game-svr --lib trade::` 20/20、`--bin game-svr deal_` 3/3。

> 注：首版 RC2 注入串里含 `deal_try` 字样，被标记规则**正确地**纳入断言面而判红；随即改为
> 真正无标记的注释重跑得绿 —— 这本身也验证了「标记即边界」的语义。

## 5. 四门禁（交付树 HEAD `caa8e9642`，base = `origin/master f4e8a29f`；`gates/`）

| 门 | 读数 |
| --- | --- |
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | **100 suites / 1161 passed / 0 failed / 22 ignored**，rc=0（基线 1160，本片 +1 条红检） |
| `cargo run -p msgcodegen -- --check` | **604** 条一致，rc=0 |

隔离构建：`CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`（检出外）。

## 6. 边界 / 未验证

- `git diff --name-only origin/master HEAD -- src sql MirClient` 空；
  `-- mir2-rs/crates/protocol mir2-rs/crates/storage` 空；`merge-tree` rc=0；`merge-base --is-ancestor` YES
  （`20-boundary.txt`）。
- 交付面仅 `mir2-rs/bins/game-svr/tests/deal_try_oracle_anchors.rs`（只读门）。
- 未验证：真机两侧同序列 `diff=0` 属 ⑦ 家族 DoD，归 `openmir2-m3-party-trade-align` + 独占窗口；
  本片零起栈 / 零占用端口。
