# anchor-count-fix-20261012 —— 锚点自检计数订正（第十二片）的原始件

> 卡 `openmir2-m3-combat-skills-align`（issue `a2ef74aa`）。背景：reviewer-5 `0b222072`
> 指出 `M3技能内联case端到端对齐报告-20261012.md` 把锚点条数写成 26（实为 28）。
> 本目录是第十二片的证据四件套原始件（文件名一律 `.txt`，仓库字节/git-blob 口径）。

| 文件 | 命令 / 说明 |
| --- | --- |
| `probe-rerun-residual.txt` | `dotnet run --project mir2-rs/tests/parity/m3/residual-case-probe -c Release -- . E:/tmp/m3cs12/residual_case.json`（rc=0，15 行） |
| `probe-rerun-dispatch.txt` | `dotnet run --project mir2-rs/tests/parity/m3/dispatch-audit-probe -c Release -- . E:/tmp/m3cs12/dispatch_audit.json`（rc=0） |
| `cmp-probe-rerun-residual.txt` | 重生成向量与入库件 `cmp`（json/tsv 均逐字节相同） |
| `cmp-probe-rerun-dispatch.txt` | 同上（dispatch-audit） |
| `red-anchor-count-residual.txt` | 牙齿：`ExpectedAnchorCount` 28→27 ⇒ 探针终止（`实跑 28 != 期望 27`），已还原 |
| `red-anchor-count-dispatch.txt` | 牙齿：`ExpectedAnchorCount` 19→18 ⇒ 探针终止（`实跑 19 != 期望 18`），已还原 |
| `gates-fmt.txt` | `cargo fmt --all -- --check`（rc=0） |
| `gates-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings`（rc=0） |
| `gates-test-workspace.txt` | `cargo test --workspace --offline`（78 suites / 638 passed / 0 failed / 16 ignored） |
| `gates-msgcodegen.txt` | `cargo run -p msgcodegen --offline -- --check`（604 OK） |

复核：

```bash
python -I tools/evidence-hash/verify_sha256sums.py mir2-rs/tests/parity/m3/evidence/anchor-count-fix-20261012
```

探针本身四件套仍由各自目录的 `SHA256SUMS.txt` 管（`residual-case-probe` = GREEN 4/4、
`dispatch-audit-probe` = GREEN 4/4）。
