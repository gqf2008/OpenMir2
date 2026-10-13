# evidence-m3-fixture-chain-gate-20261013 —— M3 夹具链深检门（runner 级）

本证据包对应 `mir2-rs/tests/parity/m3/fixture_chain_gate.py`（只读、零端口；companion 至
`openmir2-m3-fixture-matrix`）。采集环境：worktree `.claude/worktrees/m3cs`，branch
`worktree-m3-fixture-chain-gate`，base = `origin/master` **`15510a47`**（本分支由已 approve 的
`075a9d63` 起，再 `git merge origin/master` 到 `15510a47`；⑧-10 / ⑨-5 两张夹具仍在各自 branch
待审、尚未合入 master）。

## 件

| 文件 | 内容 |
| --- | --- |
| `01-selftest.txt` | `fixture_chain_gate.py selftest`：注入式 stub + fail-closed 三级 + `--require-all` + 共享管线红/缺件 + 改坏必红，**21/21** rc=0 |
| `02-check-green.{txt,json}` | `check`（默认）：矩阵门（比较器 selftest）+ 各 `READY` runner `-SelfCheckNoStack` + 共享管线 ⇒ **GREEN runner_ok=8 prereq_or_skip=2 red=0** rc=0 |
| `03-check-require-all-red.{txt,json}` | `check --require-all`：PENDING / PREREQ 也判 RED ⇒ 点名 **⑧/⑨ 夹具未合 + whitelist 缺 T-45/T-46**，`RED red=4` rc=1 |
| `04-runner-selfcheck/**` | `check --evidence-dir` 落盘的逐 runner（`runner-<key>-RUN-OK.txt`）与共享管线（`shared-*.txt`）末行读数 |
| `05-mutation-remove-runner-red.txt` | 外部改坏：`mv run_quest_pair.ps1 …` ⇒ ⑥ `READY→INCONSISTENT`，**RED red=1** rc=1；随后还原（`--skip-shared`） |
| `06-restore-clean.txt` | 还原后 `git status --porcelain`（仅本片新增未跟踪件）+ `check` 回 **GREEN red=0** rc=0 |
| `10-gate-fmt.txt` | `cargo fmt --all -- --check` rc=0 |
| `11-gate-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` rc=0 |
| `12-gate-test.txt` | `cargo test --workspace --no-fail-fast --offline -- --test-threads=16` = **94 suites 878 passed 0 failed 21 ignored** rc=0 |
| `13-gate-msgcodegen.txt` | `cargo run -p msgcodegen -- --check` = OK 604 rc=0 |
| `README.md` | 本文件 |

## 口径

- **单一真源**：登记表取自 `fixture_matrix.MATRIX`（不复制第二真源）；比较器面复用
  `fixture_matrix.evaluate`（与矩阵门逐字同一口径）。
- **只读 / 零端口**：`check` 只读文件 + 跑各件自带的无栈自检（`python -I` / `powershell -NoProfile`），
  **不写任何仓库文件、不起栈、不占端口**（`--json` / `--evidence-dir` 的目标由调用方给，本包落在证据目录内）。
- **fail-closed**：在场却坏（比较器 rc≠0、runner rc∉{0,2}、共享管线 rc≠0）与 `INCONSISTENT`
  **恒 RED**；缺件 / PENDING / runner PREREQ 默认只登记、`--require-all` 判 RED；绝不对「没跑」判绿。
- 记录件为 `.txt`/`.json`/`.md`，按**仓库 LF/git-blob 口径**计入 `SHA256SUMS.txt`
  （`verify_sha256sums.py write`）。

## 关联件

- `tests/parity/m3/M3夹具链深检门报告-20261013.md`（人读报告，不在本清单覆盖范围）
- `tests/parity/m3/fixture_chain_gate.py`（被测门本体，不在本清单覆盖范围）
