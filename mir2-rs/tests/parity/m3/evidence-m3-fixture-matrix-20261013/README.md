# evidence-m3-fixture-matrix-20261013 —— M3 夹具矩阵与就绪度门

本证据包对应 `mir2-rs/tests/parity/m3/fixture_matrix.py`（只读、零端口）。
采集环境：worktree `.claude/worktrees/m3cs`，branch `worktree-m3-fixture-matrix`，
base `origin/master` = `78ae6319`（④ 方案 A / Claim B 离线片 / ⑤-6 / ⑥-5 / ⑦-5 / ⑩-3 均已合入；
**⑧-10 `37dda4ee` 与 ⑨-5 `e4d5fbb7` 两张夹具仍在各自 branch 待审，尚未合入 master**）。

## 件

| 文件 | 内容 |
| --- | --- |
| `01-list.txt` | `fixture_matrix.py list`：矩阵（十项系统 + Claim B）的驱动/比较器/场景/T/承接卡 |
| `02-check-green.txt` / `.json` | `check`（默认）：在场件核对 + 比较器 `selftest`，`ready=8 pending=2 red=0` rc=0 |
| `03-check-require-all-red.txt` / `.json` | `check --require-all`：PENDING 也判 RED + whitelist 登记号核对 ⇒ 点名 ⑧/⑨ + T-45/T-46 |
| `04-selftest.txt` | `selftest`：合成树 + fail-closed + 改坏必红，`17/17` rc=0 |
| `05-mutation-remove-comparator-red.txt` | 外部改坏①：`mv npc_shop_diff.py …` ⇒ ⑤ `READY→INCONSISTENT`，`FIXTURE_MATRIX RED red=1` rc=1；还原 |
| `06-mutation-partial-scenario-red.txt` | 外部改坏②：`mv drop-exp/drop_exp_scenario.json …` ⇒ ④ `READY→INCONSISTENT`，`RED red=1` rc=1；还原 |
| `07-restore-clean.txt` | 还原后：`git status` 仅本片两个新文件（未跟踪）+ `check` 回 `ready=8 pending=2 red=0` rc=0 |

## 口径

- **只读**：`check` 只 `os.path.isfile` + 跑各比较器自带 `selftest`（`python -I`，cwd=本目录），
  **不写任何仓库文件、不起栈、不占端口**（`check --json` 的目标路径由调用方给，本包落在证据目录内）。
- **fail-closed 三级**：三件套全在场且比较器 `selftest` rc=0 ⇒ `READY`；全不在场 ⇒ `PENDING`
  （默认只登记，`--require-all` 判 RED）；**部分在场 ⇒ `INCONSISTENT` 恒 RED**（半拉子合并绝不当绿）。
- 记录件为 `.txt`/`.json`/`.md`，按仓库 LF/git-blob 口径计入 `SHA256SUMS.txt`（`verify_sha256sums.py write`）。

## 关联件

- `tests/parity/m3/M3夹具矩阵与就绪度门报告-20261013.md` sha256 `e59366308356cb6eb0cee9c31001e939dec1b721a48a715b35523b44b4caee2b`（人读报告，不在本清单覆盖范围）
