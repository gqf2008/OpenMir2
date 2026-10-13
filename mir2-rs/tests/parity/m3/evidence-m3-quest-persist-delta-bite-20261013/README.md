# evidence-m3-quest-persist-delta-bite-20261013

卡 `openmir2-m3-quest-persist-delta-bite` —— M3⑥-4 `openmir2-m3-quest-persist-observability`
的 review Minor **M-1** 收口（reviewer-6 独立复核条目 `0a4fee678d9c`，`kind=inert-sub-assertion`）。

> 作者 `openmir2-worker-21`（worktree `m3pt`）。本片**不 merge、不自签 review**。
> oracle / 客户端 / 表结构 / 脚本 / 数据格式零改动；**生产代码零改动**（只动一个测试门禁文件）。

## 1. 为什么（缺陷口径）

⑥-4 的门禁第 4 层「已捕获的两侧观测」写作：

```rust
if let Some(delta) = v.get("delta").and_then(|d| d.get("mir2_db.characters_quest")) {
    // ... assert (added, removed) == (0, 0)
}
```

而 M1 跨读基线（`tests/parity/evidence/M1/s4b-slice4-crossread/03|04`）的顶层 `delta` 只列
**净改动非零**的表（实测九张表，`mir2_db.characters_quest` 不在其中——该表 0 行、未被触碰），
故该 `if let Some` 分支**从不执行**：断言名不副实（恒绿），文档/交付件却写「delta 0/0」。
reviewer-6 的判定：`severity=Minor, kind=inert-sub-assertion`，建议「补 `delta` 或改文档口径」。

## 2. 修法（判据本体）

不复述「改文档口径」这种降级路线，而是把该子判据做成**真会执行 + fail-closed**：

| 件 | 作用 |
| --- | --- |
| `quest_delta_violation(&Value) -> Result<(), String>` | 抽成纯函数：顶层 `delta` 缺失/非对象 ⇒ `Err`；`delta["mir2_db.characters_quest"]` 不存在 ⇒ `Ok`；存在但缺 `added`/`removed` 或非整数 ⇒ `Err`；存在且非 0/0 ⇒ `Err` |
| `captured_cross_side_baselines_show_zero_characters_quest_rows` | 原惰性 `if let Some` 改为调用上述函数 + `panic` 携带 side；`rows == 0` 断言**原样保留**（原实现的真牙不降级） |
| `redcheck_quest_delta_subassertion_bites`（新增） | 用合成 JSON 强制五态执行，使「分支被执行」成为机判事实 |

fail-closed 的意义：若将来基线丢掉/改名 `delta` 字段，门禁**变红**而不是悄悄恒绿——这正是
reviewer-6 点名的「断言名不副实」根治口径。

## 3. oracle 锚

逐段原文 + 实测命令见 `01-oracle-anchors.txt`。要点：

| 位置 | 语义 |
| --- | --- |
| `M1/s4b-slice4-crossread/03|04-*-dbsrv-baseline.json` | 两侧 `delta` 均不含 `mir2_db.characters_quest`（0 行、未进写集） |
| `PlayDataStorage.Save.cs:85-88` | `UPDATE characters SET …` 列清单**无任务列** |
| `PlayDataStorage.Save.cs:495` | `SaveQuest` 只 `DELETE FROM characters_quest`，无 `INSERT`、无调用点 |
| `PlayDataStorage.Query.cs:194` | 读档 `SELECT * FROM characters WHERE ID=@ID`，不读任务数组 |
| `PlayerDataInfo.cs:101-103/114-116` | 任务数组只走会话 `byte[128]`×3，下线即丢 |

结论：oracle 的「任务态不落库」是**行为**；本片只修门禁的表达力，不动语义。

## 4. 判据 + 改坏必红

- 判据：`cargo test -p db-srv --test quest_no_persist` = **8 passed / 0 failed**（base 为 7 条，
  本片 +1 条 `redcheck_quest_delta_subassertion_bites`）。见 `03-tests.txt` / `gates/test.txt`。
- **RC-A**：把 `if (added, removed) != (0, 0)` 改成 `if false && …`（净改动牙拔掉）⇒
  `redcheck_quest_delta_subassertion_bites` **FAILED**（rc=101，点名「非零 added 的…被放行」）；
  `cp` 还原后复跑 **GREEN**、`git status` 只剩本片文件。
  见 `redchecks/RC-A-mutated.txt` / `RC-A-restored.txt`。
- **RC-B**：把顶层 fail-closed 的 `ok_or_else(…)?` 改成 `let Some(delta)=… else { return Ok(()) }`
  （缺 `delta` 即放行）⇒ 同一用例 **FAILED**（rc=101，[4] `no_delta` 子断言）；
  还原后复跑 **GREEN**。见 `redchecks/RC-B-mutated.txt` / `RC-B-restored.txt`。

两处变异各自只拔一条牙、互不覆盖：RC-A 咬「净改动必须红」，RC-B 咬「缺字段必须红」。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`，交付 head 实测）

| 门 | 读数 | 件 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0（无输出） | `gates/fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates/clippy.txt` |
| `cargo test --workspace` | 98 suites / **1097 passed** / 0 failed / 21 ignored | `gates/test.txt` |
| `cargo run -p msgcodegen -- --check` | 604 常量一致 | `gates/msgcodegen.txt` |

base `ddaca961c`（= master tip）本片只增不删 1 条 `#[test]`。

## 6. 边界

`git diff --name-only origin/master HEAD -- src sql MirClient` **空**；改动面 = 本门禁文件 + 本证据目录；
base 即 `origin/master` 本刻 tip，`merge-tree origin/master HEAD` rc=0。见 `20-boundary.txt`。
**零起栈、零端口**；未触碰共享主检出。

## 7. 复核

```bash
python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra mir2-rs/tests/parity/m3/evidence-m3-quest-persist-delta-bite-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-quest-persist-delta-bite-20261013
```

## 8. 残留物清单

- `E:/tmp/m3pt-rc/`（工作目录）：`quest_no_persist.rs.orig`（还原锚）、`delta-bite.diff`、
  `restored.txt`、`RC-{A,B}-{mutated,restored}.txt`、`gate-test-raw.txt`、`per-crate.txt`
  —— 皆为本轮中间件，**登记保留**（改坏必红的还原锚需要可复现）。
- `E:/tmp/openmir2-m3pt-target`：跨轮隔离构建缓存，保留。
- 本仓内不留临时件；`git status` 只含本片文件（见 `20-boundary.txt`）。
