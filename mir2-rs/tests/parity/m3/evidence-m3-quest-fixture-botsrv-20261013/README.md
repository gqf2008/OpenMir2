# evidence-m3-quest-fixture-botsrv-20261013

M3⑥ 任务动作序列夹具骨架（卡 `openmir2-m3-quest-fixture-botsrv`）的证据包。

判据本体与入口命令见报告
[`../M3⑥任务动作序列夹具骨架报告-20261013.md`](../M3⑥任务动作序列夹具骨架报告-20261013.md)。

## 文件

| 文件 | 内容 | rc |
| --- | --- | --- |
| `01-quest-diff-selftest.txt` | `python -I quest_diff.py selftest`（36/36，含每个用例的 diff 原始输出） | 0 |
| `02-quest-pair-selfcheck.txt` | `run_quest_pair.ps1 -SelfCheckNoStack`（19/19，**本脚本自己以 LF 落盘**） | 0 |
| `03-botsrv-questseq-selftest.txt` | `BotSrv.exe --selftest-questseq`（生产纯函数 16/16：`ParseQuestSteps`/`ClassifyQuestFrames`/`ExtractQuestSay`） | 0 |
| `04-run-bots-selfcheck.txt` | `run_bots.ps1 -SelfCheckNoStack`（新增 quest 收尾计数的两向自检后仍绿） | 0 |
| `05-selftest-gate.txt` | `selftest_gate.ps1`（共享 `gate_guards.ps1` 的十条机判护栏回归；`SUMMARY total=65 failed=0`） | 0 |
| `06-run-bag-equip-selfcheck-regression.txt` | `run_bag_equip_pair.ps1 -SelfCheckNoStack`（T-22 夹具回归；改动 `gate_guards.ps1`/`run_pair.ps1` 后仍全绿） | 0 |
| `07-run-npc-shop-selfcheck-regression.txt` | `run_npc_shop_pair.ps1 -SelfCheckNoStack`（T-41 商店夹具回归 17/17；共用 `gate_guards.ps1`/`run_pair.ps1`/`run_bots.ps1`） | 0 |
| `08-assemble-diff-cli.txt` | CLI 端到端：`-AssembleOnly`（带/不带 `--state`）+ `-DiffOnly`（同件 GREEN / 改一位任务态必红 rc=1 / 缺任务态 PREREQ rc=2） | 0/1/2 |
| `09-external-break-comparator-branch.txt` | **外部改坏必红**：把 `quest_diff.py` 的 `branch` 比较删掉 ⇒ `selftest 35/36`（`mutate-branch-red` FAIL）、rc=1；还原后 `36/36`、rc=0 | 1→0 |
| `10-gate-fmt.txt` | `cargo fmt --all -- --check`（cwd=`mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/m3cs-quest-target`） | 0 |
| `11-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` | 0 |
| `12-gate-test.txt` | `cargo test --offline --workspace` = **93 suites / 847 passed / 0 failed / 20 ignored** | 0 |
| `13-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` = 604 条 | 0 |

## 口径

- `SHA256SUMS.txt` 按**仓库字节口径（LF 归一 / Git blob）**记；`verify_sha256sums.py --check-extra` 应 GREEN。
- 所有 `.txt` 由生产者以 **LF** 落盘（`Write-M3LfText`/`Write-M3LfLines` 或 `sed -i 's/\r$//'`），避免 EOL 陷阱。
- 本证据包**未起栈、未占端口**；`08/09` 的合成件在 `E:/tmp/m3cs-quest-evidence-tmp` 生成，采集后已清理。
