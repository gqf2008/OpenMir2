# evidence-m3-quest-nopersist-workspace-gate-20261013

卡 `openmir2-m3-quest-nopersist-workspace-gate` —— M3⑥-4 `openmir2-m3-quest-persist-observability`
的 review Minor **M-1** 收口（reviewer-5 approve 条目 `3dc49a830e`）。

> 作者 `openmir2-worker-21`（worktree `m3pt`）。本片**不 merge、不自签 review**。
> oracle / 客户端 / 表结构 / 脚本 / 数据格式零改动；**生产代码零改动**（只动一个测试门禁文件）。

## 1. 为什么（缺陷口径）

reviewer-5 在 ⑥-4 的 approve 里登记 M-1：

> 工作区级扫描只禁表名 `characters_quest`（`TABLE_TOKEN`）；五个任务列 token
> （`QUESTFLAG`/`QUESTUNIT`/`QUESTUNITOPEN`/`QUESTOPENINDEX`/`QUESTFININDEX`）**仅在
> `bins/db-srv/src/playdata.rs` 单文件断言**。若未来在别的 crate 把任务列写进**别的表**，
> 此门禁不会咬住。

⑥-4 的门禁做了三件事：① 全仓 `.rs` 不得出现 `characters_quest`；② `playdata.rs` 单文件不得
出现任务列；③ 红检。**缺口正是「任务列 × 非 `characters_quest` 表 × 非 `playdata.rs` 文件」**——
① 只看表名、② 只看一个文件，两者都漏。

## 2. 修法（判据本体）

把工作区级扫描从「表名」升级为「**SQL 字面量里的任务列/表**」：

| 件 | 作用 |
| --- | --- |
| `string_literals(code)` | 从**去注释后的源码**抽字符串字面量（标准串 + `r"…"`/`r#"…"#`），跳过字符字面量 |
| `sql_quest_hits(code)` | 字面量小写后**同时**含 SQL 关键字（`insert`/`update`/`delete`/`select`）与任一任务 token ⇒ 记一条 |
| `workspace_sql_literals_never_mention_quest_columns` | 全仓 `.rs`（跳过本门禁自身）跑上面的函数，命中即红、点名文件 + 字面量 + token |
| `redcheck_sql_scanner_bites_injected_quest_columns` | 自动红检：注入别的表的任务列必须被咬、展示串/注释/非任务 SQL 不得误报、原始串在扫描面内 |

为什么不能只看 token：本仓真实存在展示串 `d.quest_flag.as_ref().expect("QuestFlag")`
（`bins/game-svr/src/record.rs:1158`）与 `crates/script` 的会话态代码，只按 token 扫会误报。
「SQL 关键字 + 任务 token 同处一个字符串字面量」才等价于「把任务态写进 SQL」。

## 3. oracle 锚

逐段原文 + 实跑命令见 `01-oracle-anchors.txt`。要点：

| 位置 | 语义 |
| --- | --- |
| `PlayDataStorage.Save.cs:85-88` | `UPDATE characters SET …` 列清单**无任务列** |
| `PlayDataStorage.Save.cs:495` | `SaveQuest` 只 `DELETE FROM characters_quest`，无 `INSERT` |
| `PlayDataStorage.Save.cs:493/505` | `SaveQuest` 全仓**无调用点**（唯一下文=自身异常日志） |
| `PlayDataStorage.Query.cs:194` | 读档 `SELECT * FROM characters WHERE ID=@ID`，不读任务数组 |
| `PlayerDataInfo.cs:101-103/114-116` | 任务数组只走**会话记录** `byte[128]`×3，下线即丢 |

结论：oracle 的「任务态不落库」是**行为**，本门禁把它钉死；若哪天要落库，属规格变更，必须 owner 另立卡。

## 4. 判据 + 改坏必红

- 判据：`cargo test -p db-srv --test quest_no_persist` = **7 passed / 0 failed**（本片 +2 条，
  见 `03-tests.txt`；`gates/test.txt` 是全 workspace 98 suites / 1046 passed / 0 failed / 21 ignored）。
- **RC-A（工作区级）**：在 `mir2-rs/crates/world/src/` 落一个把任务列写进**别的表**的 `.rs` ⇒
  `workspace_sql_literals_never_mention_quest_columns` **FAILED**（rc=101）且点名该文件；
  旧的 `characters_quest` 全文件扫描**不响**（证新增覆盖面料）；删除夹具后复跑 **GREEN**、`git status` 只剩本片文件。
  见 `redchecks/RC-A-workspace-mutated.txt` / `RC-A-restored.txt`。
- **RC-B（函数级）**：把 `sql_quest_hits` 首行改成 `if true { return Vec::new(); }`（门禁形同虚设）⇒
  `redcheck_sql_scanner_bites_injected_quest_columns` **FAILED**（rc=101）；`cp` 还原后
  `sha256(quest_no_persist.rs)=806a853d…ce71` 与改坏前一致、复跑 GREEN。见
  `redchecks/RC-B-function-mutated.txt` / `RC-B-restored.txt`。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`，交付 head 实测）

| 门 | 读数 | 件 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0（无输出） | `gates/fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates/clippy.txt` |
| `cargo test --workspace` | 98 suites / **1045 passed** / 0 failed / 21 ignored | `gates/test.txt` |
| `cargo run -p msgcodegen -- --check` | 604 常量一致 | `gates/msgcodegen.txt` |

base `636bdce53`（= master tip）= 1044 passed（本片只增不删，+2 条 `#[test]` ⇒ 1046）。

## 6. 边界

`git diff --name-only origin/master -- src sql MirClient` **空**；改动面 = 本门禁文件 + 本证据目录；
`merge-tree origin/master HEAD` rc=0。见 `20-boundary.txt`。零起栈、零端口。

## 7. 复核

```bash
python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra mir2-rs/tests/parity/m3/evidence-m3-quest-nopersist-workspace-gate-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-quest-nopersist-workspace-gate-20261013
```
