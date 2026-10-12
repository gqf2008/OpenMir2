# evidence-m3-guild-castle-inventory-20261013 —— M3⑧ 公会/攻城现状盘点证据包

> 卡 `openmir2-m3-guild-castle-align`（M3 逐系统对齐 ⑧）；对应报告
> [`../M3⑧公会攻城现状盘点报告-20261013.md`](../M3⑧公会攻城现状盘点报告-20261013.md)。
> 仓库基线 `origin/master f2eb43d6`（本片分支 `worktree-m3-guild-castle-align`）。
> 本包**全部离线**：未起栈、未占端口、零 `.rs`。

## 件数与口径

| 文件 | 内容 |
| --- | --- |
| `00-oracle-chain-lines.txt` | oracle 侧行会/攻城完整链行号锚点（`rg -n` + 逐段 `sed` 原文；含 `NotImplementedException` 断点与四处 oracle 侧 bug 证据） |
| `01-rust-status-greps.txt` | Rust 现状只读 grep（[1] 常量表 … [14] 落库；派发/帧/实体/配置/加载器全 no match） |
| `02-frames-config-and-db.txt` | C#↔Rust 帧常量对照 + 触发点 + 配置默认值/现网 `Server.conf` + 落库事实 + 冻结数据文件实测 + Npcs.txt |
| `03-fixture-reuse-survey.txt` | `run_pair.ps1` / `run_attack_pair.ps1` peer 护栏 / `scoped_diff.py` / C# 影子栈 / Rust 栈起法 / **时间门清单** |
| `04-gate-fmt.txt` | `cargo fmt --all -- --check`（空 = 干净）rc=0 |
| `05-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` rc=0 |
| `06-gate-test.txt` | `cargo test --offline --workspace` rc=0（86 suites / 714 passed / 0 failed / 18 ignored） |
| `07-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` rc=0（604 条） |

## 复现命令（cwd = `mir2-rs/`）

```bash
cat tests/parity/m3/evidence-m3-guild-castle-inventory-20261013/00-oracle-chain-lines.txt
cat tests/parity/m3/evidence-m3-guild-castle-inventory-20261013/01-rust-status-greps.txt
cat tests/parity/m3/evidence-m3-guild-castle-inventory-20261013/02-frames-config-and-db.txt
cat tests/parity/m3/evidence-m3-guild-castle-inventory-20261013/03-fixture-reuse-survey.txt

export CARGO_TARGET_DIR=E:/tmp/m3cs-guild-target
cargo fmt --all -- --check
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo test --offline --workspace
cargo run --offline -p msgcodegen -- --check
```

## 复核（仓库字节 / LF 归一 / git-blob 口径）

```bash
python -I tools/evidence-hash/verify_sha256sums.py --check-extra \
  mir2-rs/tests/parity/m3/evidence-m3-guild-castle-inventory-20261013
```

## 复核器（hash 口径）

`SHA256SUMS.txt` 按**仓库字节口径（LF 归一 / git blob）**记（`docs/Rust重写设计文档.md` §6.0-4）：
`write` 回填（默认 repo 口径）→ `verify --check-extra` 复核 GREEN。
本包**不并入** `mir2-rs/tests/parity/m3/sessions/` 下 ⑤/⑥/⑦ 遗留的未跟踪会话目录。
