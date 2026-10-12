# evidence-m3-party-trade-inventory-20261013 —— M3⑦ 组队/交易现状盘点证据包

> 卡 `openmir2-m3-party-trade-align`（M3 逐系统对齐 ⑦）；对应报告
> [`../M3⑦组队交易现状盘点报告-20261013.md`](../M3⑦组队交易现状盘点报告-20261013.md)。
> 仓库基线 `origin/master a96ef0e6`（本片分支 `worktree-m3-party-trade-align`）。
> 本包**全部离线**：未起栈、未占端口、零 `.rs`。

## 件数与口径

| 文件 | 内容 |
| --- | --- |
| `00-oracle-chain-lines.txt` | oracle 组队/交易调用链行号锚点（`rg -n`，含 `PlayObject.*` / 配置 / DB 列） |
| `01-rust-status-greps.txt` | Rust 现状只读 grep（[1] 常量 35/35 … [11] DB 列；组队/交易派发/状态/配置全 no match） |
| `02-frames-config-and-db.txt` | C#↔Rust 帧常量对照 + 组队/交易落库事实 + 配置默认值/现网 `Server.conf` |
| `03-fixture-reuse-survey.txt` | `run_pair.ps1` / `run_attack_pair.ps1` 的 peer 双护栏 + `scoped_diff.py` + BotSrv 现成件调查 |
| `04-gate-fmt.txt` | `cargo fmt --all -- --check`（空 = 干净）rc=0 |
| `05-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` rc=0 |
| `06-gate-test.txt` | `cargo test --offline --workspace` rc=0（86 suites / 714 passed / 0 failed / 18 ignored） |
| `07-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` rc=0（604 条） |

## 复现命令（在本 worktree 的 `mir2-rs/` 下）

```bash
cat mir2-rs/tests/parity/m3/evidence-m3-party-trade-inventory-20261013/00-oracle-chain-lines.txt
cat mir2-rs/tests/parity/m3/evidence-m3-party-trade-inventory-20261013/01-rust-status-greps.txt
cat mir2-rs/tests/parity/m3/evidence-m3-party-trade-inventory-20261013/02-frames-config-and-db.txt
cat mir2-rs/tests/parity/m3/evidence-m3-party-trade-inventory-20261013/03-fixture-reuse-survey.txt

cd mir2-rs
export CARGO_TARGET_DIR=E:/tmp/m3cs-party-target
cargo fmt --all -- --check
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo test --offline --workspace
cargo run --offline -p msgcodegen -- --check
```

## 复核（仓库字节 / LF 归一 / git-blob 口径）

```bash
python -I tools/evidence-hash/verify_sha256sums.py --check-extra \
  mir2-rs/tests/parity/m3/evidence-m3-party-trade-inventory-20261013
```

## 复核器（hash 口径）

`SHA256SUMS.txt` 按**仓库字节口径（LF 归一 / git blob）**记（`docs/Rust重写设计文档.md` §6.0-4）。
本包生成方式：先按清单条目名建表 → `python -I tools/evidence-hash/verify_sha256sums.py write <dir>` 回填
（默认 repo 口径）→ `verify --check-extra` 复核 GREEN。
