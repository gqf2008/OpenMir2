# evidence-m3-shop-fixture-botsrv-20261013

M3⑤ NPC/商店动作序列夹具骨架（卡 `openmir2-m3-shop-fixture-botsrv`）的证据包。

判据本体与入口命令见报告
[`../M3⑤NPC商店动作序列夹具骨架报告-20261013.md`](../M3⑤NPC商店动作序列夹具骨架报告-20261013.md)。

## 文件

| 文件 | 内容 | rc |
| --- | --- | --- |
| `01-npc-shop-diff-selftest.txt` | `python -I npc_shop_diff.py selftest`（含每个用例的 diff 原始输出） | 0 |
| `02-npc-shop-pair-selfcheck.txt` | `run_npc_shop_pair.ps1 -SelfCheckNoStack`（17/17，**本脚本自己以 LF 落盘**） | 0 |
| `03-botsrv-shopseq-selftest.txt` | `BotSrv.exe --selftest-shopseq`（生产纯函数 19/19：`ParseShopOps`/`ClassifyShopFrames`/`ExtractShopPrice`） | 0 |
| `04-run-bots-selfcheck.txt` | `run_bots.ps1 -SelfCheckNoStack`（新增 shop 收尾计数的两向自检后仍绿） | 0 |
| `05-selftest-gate.txt` | `selftest_gate.ps1`（共享 `gate_guards.ps1` 的十条机判护栏回归；`SUMMARY total=65 failed=0`） | 0 |
| `06-run-bag-equip-selfcheck-regression.txt` | `run_bag_equip_pair.ps1 -SelfCheckNoStack`（T-22 夹具回归；改动 `gate_guards.ps1`/`run_pair.ps1` 后仍全绿） | 0 |
| `07-mutation-branch-red.txt` | 合成件：两条相同记录 ⇒ GREEN；把 rs 换成 `649`+`fail_sell_reject` ⇒ **RED 5 项**（点名 `branch`/`frames.sm`/`gold_after`） | 0/1 |
| `08-external-break-comparator-branch.txt` | **外部改坏必红**：把 `npc_shop_diff.py` 的 `branch` 比较删掉 ⇒ `selftest 26/27`（`mutate-branch-red` FAIL）、rc=1；还原后追加 `selftest 27/27`、rc=0 | 1→0 |
| `10-gate-fmt.txt` | `cargo fmt --all -- --check`（cwd=`mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/m3cs-target`） | 0 |
| `11-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` | 0 |
| `12-gate-test.txt` | `cargo test --offline --workspace` = **93 suites / 847 passed / 0 failed / 20 ignored** | 0 |
| `13-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` = 604 条 | 0 |

## 口径

- `SHA256SUMS.txt` 按**仓库字节口径（LF 归一 / Git blob）**记；`verify_sha256sums.py --check-extra` 应 GREEN。
- 所有 `.txt` 由生产者以 **LF** 落盘（`Write-M3LfText`/`Write-M3LfLines` 或 `sed -i 's/\r$//'`），避免 EOL 陷阱。
- 本证据包**未起栈、未占端口**；`07/08` 的合成件在临时目录生成，已清理。
