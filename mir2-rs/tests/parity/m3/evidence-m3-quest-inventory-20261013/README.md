# evidence-m3-quest-inventory-20261013 —— M3⑥ 任务现状盘点证据包

> 卡 `openmir2-m3-quest-align`（M3 逐系统对齐 ⑥）；对应报告
> [`../M3⑥任务现状盘点报告-20261013.md`](../M3⑥任务现状盘点报告-20261013.md)。
> 仓库基线 `origin/master 1b576172`（本片分支 `worktree-m3-quest-align`）。
> 本包**全部离线**：未起栈、未占端口、零 `.rs`。

## 件数与口径

| 文件 | 内容 |
| --- | --- |
| `00-oracle-chain-lines.txt` | oracle 调用链行号锚点（`grep -n`；含 `MapQuest.txt` / 真实脚本） |
| `01-rust-status-greps.txt` | Rust 现状只读 grep（[1] 数组已解析 … [8] 不落库） |
| `02-quest-fixture-crossref.txt` | 现存脚本任务 接/交 链 + 语料用量 + M4 用例对表 |
| `03-fixture-reuse-survey.txt` | `run_pair.ps1` 参数面 / `scoped_diff.py` / BotSrv 序列可复用性 |
| `04-gate-fmt.txt` | `cargo fmt --all -- --check`（空 = 干净）rc=0 |
| `05-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` rc=0 |
| `06-gate-test.txt` | `cargo test --offline --workspace` rc=0（85 suites / 705 passed / 0 failed / 17 ignored） |
| `07-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` rc=0（604 条） |

## 复现命令（在仓库根）

```bash
cat mir2-rs/tests/parity/m3/evidence-m3-quest-inventory-20261013/00-oracle-chain-lines.txt
cat mir2-rs/tests/parity/m3/evidence-m3-quest-inventory-20261013/01-rust-status-greps.txt
cat mir2-rs/tests/parity/m3/evidence-m3-quest-inventory-20261013/02-quest-fixture-crossref.txt
cat mir2-rs/tests/parity/m3/evidence-m3-quest-inventory-20261013/03-fixture-reuse-survey.txt

cd mir2-rs
export CARGO_TARGET_DIR=E:/tmp/m3cs-quest-target
cargo fmt --all -- --check
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo test --offline --workspace
cargo run --offline -p msgcodegen -- --check
```

## hash 口径

`SHA256SUMS.txt` 按**仓库字节口径（LF 归一 / git blob）**记（`docs/Rust重写设计文档.md` §6.0-4）。
复核：

```bash
python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-quest-inventory-20261013
```

> `02-quest-fixture-crossref.txt` 里的真实语料片段来自 **仓外输入** `E:/MirServer/M2GameSvr/Envir`
> （GBK 原文经 `iconv -f gbk -t utf-8` 转出）；该文件本身在仓库内是 `*.txt text eol=lf`，按 LF 归一记。
