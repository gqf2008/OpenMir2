# 证据包：M3⑤-实现 1 商人 `MerChant.txt` 加载（`openmir2-m3-merchant-load`，2026-10-13）

base = master `a443296e`；分支 `worktree-m3-merchant-load`；principal `openmir2-worker-5`；worktree `m3cs`。
报告：[`../M3⑤商人MerChant加载报告-20261013.md`](../M3⑤商人MerChant加载报告-20261013.md)。

## 文件

| 文件 | 内容 | 复现 |
| --- | --- | --- |
| `00-oracle-chain-lines.txt` | oracle 调用链行号锚（`LocalDB.cs` / `ScriptParsers.cs` / `Merchant.cs` / `WorldServer.Merchant.cs` / `SystemShare.cs`） | 文件内各行即 `grep -n` 命令 |
| `01-rust-status-greps.txt` | 新增/改动面 + 公开 API + 依赖/重导出 + 冻结边界口径 | `rg -n …`（见文件） |
| `02-envir-tests.txt` | `cargo test -p mir2-envir`（含 `merchant` 内联 6 条 + `tests/merchant.rs` 4 条，1 ignored） | 见文件首行 |
| `03-deployment-run.txt` | **真机向量**：读部署件 `E:\MirServer\M2GameSvr`（131 商人 / 63 带 `[goods]` / 582 条目 / 67 带 `+StdMode`） | `MIR2_GAMESVR_DIR='E:\MirServer\M2GameSvr' cargo test -p mir2-envir --test merchant -- --ignored --nocapture` |
| `04-redcheck-a-pricerate-threshold.txt` | 改坏必红 A：`%PRICERATE` 阈值 `>=55` → `>=0` ⇒ RED；还原 sha256 一致 | 见文件 |
| `05-redcheck-b-canmove-movetime.txt` | 改坏必红 B：`BoCanMove` 去掉 `&& move_time > 0` ⇒ RED；还原 sha256 一致 | 见文件 |
| `06-gate-fmt.txt` | `cargo fmt --all -- --check` = rc 0 | 见文件 |
| `07-gate-clippy.txt` | `cargo clippy --workspace --all-targets -- -D warnings` = rc 0 | 见文件 |
| `08-gate-test.txt` | `cargo test --workspace --no-fail-fast` = 92 suites / 825 passed / 0 failed / 19 ignored | 见文件 |
| `09-gate-msgcodegen.txt` | `cargo run -p msgcodegen -- --check` = 604 条一致 | 见文件 |
| `10-boundary.txt` | `src/sql/MirClient` 与 `crates/world/bins/data/script` 零改动 | 见文件 |

## 口径

- `SHA256SUMS.txt` 按**仓库字节（LF 归一 / git blob）**记；复核：
  `python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-merchant-load-20261013`
  与 `python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-merchant-load-20261013`。
- 编译隔离 `CARGO_TARGET_DIR=E:/tmp/m3cs-merchant-target`；未起栈、未占端口。
