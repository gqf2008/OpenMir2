# M3⑥-4 · O-2 收口证据包 —— 注释剥离器识别 Rust 原始串（假绿漏检面）

> 卡：`openmir2-m3-quest-nopersist-rawstring-gate`（父卡 `openmir2-m3-quest-nopersist-workspace-gate`，
> **已 closed** 合入 master `f4d7173fb`；本片为其 `approve 4b283fad18` 的**非阻塞 O-2** 收口）。
> 分支 `worktree-m3-quest-nopersist-rawstring-gate`（worktree `m3pt` / principal `openmir2-worker-21`）。
> base = `refs/heads/master` = origin/master = walgit master = `8b2466b9c`.
> head = `bfa9be9cf97fe5ec014ee571c2385ad177f5fe78`（单提交，rebase 至当前 master 之上）。
> 交付口径：`patch(--base refs/heads/master --head worktree-m3-quest-nopersist-rawstring-gate)`
> + `status: needs-review`。**不 merge、不自签 review。**

## 背景（为什么有这张卡）

`openmir2-m3-quest-nopersist-workspace-gate` 把「任务态不落库」升级为**全仓 SQL 字面量扫描**，
其注释剥离器 `strip_comments_keep_strings` 只识别普通串 `"…"`。reviewer-5 `4b283fad18` **O-2**：
原始串 `r#"…"#` 内含引号时，剥离器会在**内层引号**处提前收尾，其后的 `//` / `/*` 被当作注释剥掉，
于是原始串里写的 SQL 任务列**被漏检**（门禁假绿）。O-1 为测试头 doc-comment 的 oracle 行号近似。

本片按 Rust 字面量规则重写剥离器，并新增一条红检把该漏检面钉死；零生产/协议/表/脚本改动。

## DoD ① oracle 锚定

本片为**测试工具/门禁**修复，不引入 oracle 行为，oracle 冻结面（`src/`）零改动。
锚点见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)（复核门禁所固化的「任务态不落库」oracle 事实 + O-1 行号订正）：
`PlayDataStorage.Save.cs:82` `SaveRecord`、`:85-88` `UPDATE characters` 列清单（无任务列）、
`:493/:495` `SaveQuest` 只 `DELETE FROM characters_quest`、`:505` 自身异常日志、
`PlayDataStorage.Query.cs:192/:194` 读档 `SELECT * FROM characters`（不读任务数组）、
`PlayerDataInfo.cs:101-103` + `:112-114` 会话态 `QuestFlag/QuestUnit/QuestUnitOpen = new byte[128]`。

## DoD ② 实现

见 [`02-change-summary.txt`](02-change-summary.txt)。净 diff 仅 **1 个测试文件**
`mir2-rs/bins/db-srv/tests/quest_no_persist.rs`（+190 / −34）：

- 重写 `strip_comments_keep_strings`：按 Rust 字面量规则识别普通串（`\` 转义）、
  原始串 `r"…"` / `r#"…"#` / `r##"…"##`（按 `#` 个数吃到配平收尾，内部引号与 `//`/`/*` 都是内容）、
  字符字面量 `'"'`、以及**嵌套**块注释；
- 拆出小函数 `skip_line_comment` / `skip_block_comment` / `starts_raw_string` /
  `copy_raw_string` / `copy_char_literal` / `copy_normal_string`（clippy `too_many_lines` 阈值内）；
- doc-comment 补 O-2 收口说明；顺带订正 O-1：`Save.cs:81-86` → `:85-88`（实测）。

顺序与旧实现一致（逐字符、无并行），不改任何扫描口径的**判定规则**，只修边界识别。

## DoD ③ 判据 + 改坏必红

- 新增判据 **1 条**
  `redcheck_comment_stripper_handles_raw_strings_with_comment_markers`（[`03-tests.txt`](03-tests.txt)）：
  内层引号为**奇数**时构成区分度（偶数引号旧实现会因重新开串而碰巧保住内容）——
  [1] `//`、[2] `/* */` 在原始串内且在内层引号之后不得吞掉串尾 SQL；[3] `r##"…"##` 同理；
  [4] 对照：真正的行注释仍被剥掉。
- `quest_no_persist` 全量 **9 passed / 0 failed**（原 8 条 → +1）。
- **改坏必红 1 条**：[`redchecks/RC1-mutated.txt`](redchecks/RC1-mutated.txt) —— 把原始串分支
  置为 `false && …`（等价旧实现不识别原始串）⇒ 新判据 **FAILED rc=101**
  （`0 passed; 1 failed`，`红检失效：原始串内引号后的 // 把 SQL 任务列吞掉了`）。
  还原见 [`redchecks/RC1-restored.txt`](redchecks/RC1-restored.txt) 与 [`05-hashes.txt`](05-hashes.txt)：
  `sha256` 与改坏前一致（`d6952966…`）、`git status` 除证据目录外干净、复跑 9/9 GREEN。

## DoD ④ 四门禁 + 边界

隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`；在 head `bfa9be9cf` 实测：

| 门禁 | 文件 | 读数 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `gates/fmt.txt` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | `gates/clippy.txt` | rc=0 |
| `cargo test --workspace` | `gates/test.txt` | rc=0，**100 suites / 1175 passed / 0 failed / 22 ignored** |
| `cargo run -p msgcodegen -- --check` | `gates/msgcodegen.txt` | rc=0，604 条常量 |

（base `8b2466b9c` 无本片时为 1174 passed ⇒ 本片 **+1**，无回归。）

边界 [`20-boundary.txt`](20-boundary.txt)：`git diff --name-only origin/master HEAD -- src sql MirClient`
**为空**；`-- mir2-rs/crates/protocol mir2-rs/crates/storage` **为空**；净 diff 仅
`mir2-rs/bins/db-srv/tests/quest_no_persist.rs` + 本证据目录；base 是 head 祖先；
`merge-tree origin/master HEAD` **rc=0**。

## 未做 / 残留（登记）

- **真机两侧 diff=0**：属 ⑥-4 父卡 + 独占窗口，不在本片范围 ⇒ **未起栈、未占端口**，如实 `not_verified`。
- 同类剥离器在 `crates/world/tests/no_parallel.rs`、`crates/world-bevy/tests/bevy_drop_parity.rs` 也存在，
  但属**其它车道的写集**（crates/world / world-bevy），本片**不越界**触碰；留给对应 owner。
- 本线在审的两张片（`openmir2-m3-party-deal-try-null-pose` head `da159c298`、
  `openmir2-m3-party-deal-capacity-int` head `ff2ae7f41`）仍**无 approve**，未被本片覆盖。
- 无临时目录/库/进程残留；未 merge、未自签 review。

## 文件

| 文件 | 说明 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 事实锚点（实测）+ O-1 行号订正 |
| `02-change-summary.txt` | 改动清单（逐条 + 全 diff） |
| `03-tests.txt` | 判据原始输出（9/9） |
| `04-eolcheck.txt` | `eolcheck` 原始输出 |
| `05-hashes.txt` | 改坏/还原哈希 + git status |
| `20-boundary.txt` | 边界证据 |
| `gates/` | 四门禁原始输出 |
| `redchecks/` | 改坏必红（RC1-mutated / RC1-restored） |
| `SHA256SUMS.txt` | 本包逐件哈希（仓库 LF/git blob 口径） |
