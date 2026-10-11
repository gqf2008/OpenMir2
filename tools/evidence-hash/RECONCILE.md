# 全仓 `SHA256SUMS.txt` 逐目录对账（③）

> 生成：`python -I tools/evidence-hash/verify_sha256sums.py reconcile --md tools/evidence-hash/RECONCILE.md --json tools/evidence-hash/RECONCILE.json`
> 口径：仓库字节（Git blob 内容；`text` 文件 = LF 归一，`-text` = 原始字节，仓外输入 = 盘上字节）。
> 「仓库」列 = 按仓库字节命中的件数，「盘上」列 = 按盘上原始字节命中的件数。

| 目录 | 件数 | 仓库字节 | 盘上字节 | 缺件 | 全目录冻结(`* -text`) | 结论 | 说明 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `mir2-rs/tests/parity/envir-2026-10-10` | 6 | 6 | 6 | 0 | 否 | **GREEN** | 仓库字节 == 盘上字节（两种口径一致） |
| `mir2-rs/tests/parity/envir-2026-10-11` | 11 | 11 | 11 | 0 | 否 | **GREEN** | 仓库字节 == 盘上字节（两种口径一致） |
| `mir2-rs/tests/parity/evidence/M1/s4b-slice4-crossread` | 51 | 51 | 51 | 0 | 是 | **GREEN** | 仓库字节 == 盘上字节（两种口径一致） |
| `mir2-rs/tests/parity/evidence/M1/s4b-slice5-exitpaths` | 25 | 25 | 25 | 0 | 是 | **GREEN** | 仓库字节 == 盘上字节（两种口径一致） |
| `mir2-rs/tests/parity/m1-boundary-load/evidence` | 26 | 26 | 26 | 0 | 是 | **GREEN** | 仓库字节 == 盘上字节（两种口径一致） |
| `tools/m2-acceptance/evidence/m2svr-o3-pergateway-kick-20261012` | 4 | 4 | 4 | 0 | 否 | **GREEN** | 仓库字节 == 盘上字节（两种口径一致） |
| `tools/m2-acceptance/evidence/session-lifecycle-rerun-20261012` | 11 | 11 | 3 | 0 | 否 | **GREEN** | 仓库字节口径（盘上 CRLF ≠ blob，复核器默认口径） |
| `tools/m2-acceptance/evidence/session-lifecycle-selftest-20261012` | 6 | 6 | 1 | 0 | 否 | **GREEN** | 仓库字节口径（盘上 CRLF ≠ blob，复核器默认口径） |

