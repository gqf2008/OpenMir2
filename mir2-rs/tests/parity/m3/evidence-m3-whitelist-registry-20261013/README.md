# evidence-m3-whitelist-registry-20261013

`openmir2-m3-whitelist-registry-integrity` 的证据包（base = `origin/master` `15510a47`）。

| 文件 | 内容 | 命令 |
| --- | --- | --- |
| `00-oracle-anchor-t40.txt` | T-40 的 oracle 锚（`CastleOfficial.cs:227-236` 的 `Substring(startIndex,length)` + `ScriptFlagConst.cs:81-82` 常量） | `sed -n '226,236p' src/GameSrv/Npc/CastleOfficial.cs` 等 |
| `01-t40-edit-diff.txt` | T-40 行的改前/改后（仅措辞，行为口径不变） | `git --no-pager diff -U1 -- mir2-rs/tests/parity/whitelist.md` |
| `02-audit-report.txt` | 登记号审计（默认=报告）：`duplicates=[42]`、rc=0 | `python -I m3_whitelist_registry_audit.py audit` |
| `03-audit-strict-red.txt` | `--strict` 判红：点名 `DUPLICATE T-42 @ L209,L222`，rc=1 | `… audit --strict` |
| `04-audit-selftest.txt` | 工具自检（合成文本 + 注入式 classify + 改坏必红）**9/9 GREEN** | `… selftest` |
| `05-audit-external-mutation-red.txt` | 外部改坏（临时副本把 `T-43`→`T-44`）⇒ strict RED 2 条 | `… audit --strict --whitelist <copy>` |
| `06-gate-fmt.txt` | 四门禁①（cwd=`mir2-rs`）rc=0 | `cargo fmt --all -- --check` |
| `07-gate-clippy.txt` | 四门禁② rc=0 | `cargo clippy --workspace --all-targets --offline -- -D warnings` |
| `08-gate-test.txt` | 四门禁③ **94 suites / 878 passed / 0 failed / 21 ignored** rc=0 | `cargo test --workspace --no-fail-fast --offline -- --test-threads=16` |
| `09-gate-msgcodegen.txt` | 四门禁④ OK 604 rc=0 | `cargo run -p msgcodegen --offline -- --check` |

口径：

- `SHA256SUMS.txt` 按**仓库字节口径（LF 归一 / git blob）**记（`verify_sha256sums.py write`）。
- 四门禁在 `CARGO_TARGET_DIR=E:/tmp/m3cs-target`、`--offline`、cwd=`mir2-rs` 下跑。
- 本片**只读 / 零端口**；未起栈、未占端口。
- `03`/`05` 的 RED 是**审计发现**（预存的 T-42 跨车道撞号），已路由 owner；非本片引入、本片不单方面重编号
  （见报告 §2 处置与禁动写集 `mir2-rs/tests/parity/world/**`）。
