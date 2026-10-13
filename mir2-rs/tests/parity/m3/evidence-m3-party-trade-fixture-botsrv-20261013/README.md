# evidence-m3-party-trade-fixture-botsrv-20261013

M3⑦ 组队/交易动作序列夹具骨架（卡 `openmir2-m3-party-trade-fixture-botsrv`）的证据包。

判据本体与入口命令见报告
[`../M3⑦组队交易动作序列夹具骨架报告-20261013.md`](../M3⑦组队交易动作序列夹具骨架报告-20261013.md)。

## 文件

| 文件 | 内容 | rc |
| --- | --- | --- |
| `01-party-trade-diff-selftest.txt` | `python -I party_trade_diff.py selftest`（27/27，含 fail-closed + 改坏必红的逐用例输出） | 0 |
| `02-party-trade-pair-selfcheck.txt` | `run_party_trade_pair.ps1 -SelfCheckNoStack`（19/19，**本脚本自己以 LF 落盘**） | 0 |
| `03-botsrv-pairtradeseq-selftest.txt` | `BotSrv.exe --selftest-pairtrade`（生产纯函数 29/29：`ParsePairTradeSteps`/`IsPairTradeActor`/`PairTradeRunsForRole`/`PairTradeStep*`/`ClassifyPairTradeFrames`） | 0 |
| `04-run-bots-selfcheck.txt` | `run_bots.ps1 -SelfCheckNoStack`（新增 pairtrade 收尾计数的五向自检后仍绿） | 0 |
| `05-selftest-gate.txt` | `selftest_gate.ps1`（共享 `gate_guards.ps1` 的十条机判护栏回归；`SUMMARY total=65 failed=0`） | 0 |
| `06-run-bag-equip-selfcheck-regression.txt` | `run_bag_equip_pair.ps1 -SelfCheckNoStack`（T-22 夹具回归；改动 `gate_guards.ps1`/`run_pair.ps1`/`run_bots.ps1` 后仍全绿） | 0 |
| `07-run-npc-shop-selfcheck-regression.txt` | `run_npc_shop_pair.ps1 -SelfCheckNoStack`（T-41 商店夹具回归 17/17；共用 `gate_guards.ps1`/`run_pair.ps1`/`run_bots.ps1`） | 0 |
| `08-run-quest-selfcheck-regression.txt` | `run_quest_pair.ps1 -SelfCheckNoStack`（T-42 任务夹具回归 19/19；共用 `gate_guards.ps1`/`run_pair.ps1`/`run_bots.ps1`） | 0 |
| `09-assemble-diff-cli.txt` | CLI 端到端：`assemble`（2 假人事件流 → 4 记录）+ `diff`（同件 GREEN / 帧种类改坏必红 rc=1 / `--frame-scalars` 标量改坏必红 rc=1 / 缺件 PREREQ rc=2） | 0/1/2 |
| `10-external-break-comparator-branch.txt` | **外部改坏必红**：把 `party_trade_diff.py` 的 `branch` 比较删掉 ⇒ `selftest 26/27`（`mutate-branch-red` FAIL）、rc=1；还原后 `27/27`、rc=0 | 1→0 |
| `11-gate-fmt.txt` | `cargo fmt --all -- --check`（cwd=`mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/m3cs-pt-target`） | 0 |
| `12-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` | 0 |
| `13-gate-test.txt` | `cargo test --offline --workspace --no-fail-fast` = **93 suites / 847 passed / 0 failed / 20 ignored** | 0 |
| `14-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` = 604 条 | 0 |

## 口径

- `SHA256SUMS.txt` 按**仓库字节口径（LF 归一 / Git blob）**记；`verify_sha256sums.py --check-extra` 应 GREEN。
- 所有 `.txt` 由生产者以 **LF** 落盘（`Write-M3LfText`/`Write-M3LfLines` 或采集后 `CRLF→LF` 归一），避免 EOL 陷阱。
- 本证据包**未起栈、未占端口**；`09` 的合成件在 `E:/tmp/m3cs-pt-evidence-tmp` 生成，采集后已清理。

## 残留物清单

- `E:/tmp/m3cs-pt-target`：cargo 隔离 target（编译缓存，保留）。
- `E:/tmp/m3cs-pt-evidence-tmp`：本片合成件/中间日志（**采集后已删**，见报告 §5）。
- `src/BotSrv/bin/Release/BotSrv.exe`：`-SelfCheckNoStack` 第 ⑭ 项构建产物（gitignored）。
- 无本线专属进程残留；端口状态与开工前一致（十端口未监听）。
