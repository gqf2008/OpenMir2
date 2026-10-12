# evidence-m3-drop-exp-inventory-20261013

> 卡 `openmir2-m3-drop-exp-align`（M3④ 掉落/经验，issue `980a2e51`）。本包 = 本片「不占窗口」部分的
> 证据：① 现状盘点（oracle 调用链行号 + Rust 现状 grep）+ ② 夹具骨架（比较器 fail-closed + 改坏必红）。
> 仓库基线 `2313a2b8`，分支 `worktree-m3-drop-exp`。

## 文件与退出码

| 文件 | 命令（在仓库根执行；`python` 为 `python -I`） | rc |
| --- | --- | --- |
| `00-selfcheck-nostack.txt` | `run_drop_exp_pair.ps1 -SelfCheckNoStack -SelfCheckOut <本文件>` | 0 |
| `01-comparator-selftest.txt` | `python mir2-rs/tests/parity/m3/drop-exp/drop_exp_diff.py selftest` | 0 |
| `02-mutation-red-i32-truncation.txt` | 把 `drop_exp_diff.py` 的标量比较**改成有符号 i32 截断**后跑 `selftest`（改坏必红） | 1 |
| `03-real-run-fail-closed.txt` | `run_drop_exp_pair.ps1 -Side csharp`（未给 `-Go`；等 owner 发窗口） | 2 |
| `04-rust-status-greps.txt` | Rust 现状只读 grep（无 MonGen 加载 / 无怪物 AI / 无生产接线） | — |
| `05-oracle-chain-lines.txt` | oracle 掉落/经验调用链行号锚点 grep | — |
| `06-gate-fmt.txt` | `cargo fmt --all -- --check`（无输出 = 干净） | 0 |
| `07-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` | 0 |
| `08-gate-test.txt` | `cargo test --offline --workspace` | 0 |
| `09-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` | 0 |

> 四门禁在**隔离 target**（`CARGO_TARGET_DIR=E:/tmp/openmir2-m3bag-target`）跑；本片零 `.rs` 改动，
> 门禁通过为回归确认。`08-gate-test.txt` 为完整输出（~64KB）。

## 判据要点

- `01` 的 `drop_exp_diff.py selftest` = **12/12**：含 `identical-green`、`mutate-drop-name-red`、
  `mutate-gold-red`、`mutate-exp-red`、`mutate-drop-count-red`、`mutate-kill-count-red`、
  `i32-truncation-red`、`big-value-equal-green`、`missing-file-prereq`、`missing-field-prereq`、
  `invalid-json-prereq`、`k-gap-prereq`。
- `00` 的 `-SelfCheckNoStack` = **7/7**：场景固定输入 + 比较器 selftest + 进程内 identical-green +
  改坏必红 + 缺文件 PREREQ + LF 写出 selftest + 零端口。真机 real-run 在获得 `-Go` 之前**直接退出 2**，
  不静默起栈（`03`）。
- `02`：把比较器截成 i32 后 `i32-truncation-red` FAIL、整体 `ok=11/12`、rc=1；还原后 12/12 绿
  （`01`）—— 证明「不得 i32 截断」这条牙真的咬得住。

## 两个已消除的坑（本包不重犯）

1. **返回值混 stdout**：`run_drop_exp_pair.ps1::Invoke-DropExpDiff/Invoke-DropExpSelfTest` 把子进程
   stdout 走 `-Out` 引用、**只**返回 `[int]$LASTEXITCODE`，自检里直接断言 rc（不接管道、不 `Out-Null`）。
2. **EOL 陷阱**：`00` 是 `-SelfCheckOut` 由脚本自己用 `Write-M3LfText` 写出的 **LF** 件；
   `01/02/03` 由重定向产生后经 LF 归一。`sha256` 一律按**仓库字节**（git blob / LF 归一）记，
   复核用 `tools/evidence-hash/verify_sha256sums.py`（默认口径 + EOL 牙）。

## 复现（干净检出）

```bash
python -I tools/evidence-hash/verify_sha256sums.py mir2-rs/tests/parity/m3/evidence-m3-drop-exp-inventory-20261013 --check-extra --exclude README.md
```

期望 GREEN（清单件数见 `SHA256SUMS.txt`）。
