# evidence-m3-drop-exp-consumers-rust-20261013

**M3④ 纯度门「两侧实测站点表」补齐**（卡 `openmir2-m3-rng-trace-scene-groups`；站点表本体属 ④ 家族，
由 m2-svr 车道 comment `868440f3` 指明「`drop_exp_consumers.tsv` 的 Rust 侧站点行 = m3bag 写集」）。

- 写集 `mir2-rs/tests/parity/m3/**`；**零 `.rs`**、零 `crates`/`bins`/`src`；未起栈、未占端口（仅共享 3306 在听）。
- 全文报告：`../M3④纯度门两侧实测站点表补齐报告-20261013.md`。
- 证据 hash 口径：仓库字节（git blob / LF 归一），由 `tools/evidence-hash/verify_sha256sums.py` 生成。

| 件 | 内容 | 读数 |
| --- | --- | --- |
| `01-before-rust-prereq.txt` | 旧表（`origin/master`）+ rust 流水 | `PREREQ_MISSING ... 没有 measured 行` **rc=2** |
| `02-after-rust-green.txt` | 新表 + rust 流水 | **GREEN rc=0**（measured=2 setup=2 wallclock=0 other=0 unknown=0） |
| `03-purity-gate-selftest.txt` | `drop_exp_purity_gate.py selftest` | **ok=23/23 rc=0**（原 16/16，+7 真表承重） |
| `04-runner-selfcheck-nostack.txt` | `run_drop_exp_pair.ps1 -SelfCheckNoStack` | **ok=13/13 rc=0**（原 12/12） |
| `05-mut-rust-drop-unknown-red.txt` | 删 rust `formula/drop.rs` measured 行 | **RED rc=1** 点名 unknown |
| `06-mut-rust-kill-unknown-red.txt` | 删 rust `combat.rs` measured 行 | **RED rc=1** 点名 unknown |
| `07-mut-csharp-drop-unknown-red.txt` | 删 csharp `MonGetRandomItems` measured 行 | **RED rc=1** 点名 unknown |
| `08-after-csharp-green.txt` | 新表 + csharp 流水 | **GREEN rc=0** |
| `09-rust-pin-failed-wallclock-red.txt` | rust pin 抑制失效（`monai.rs`） | **RED rc=1** 点名 `wallclock_consumer` |
| `10-gates-fmt.txt` / `11-gates-clippy.txt` / `12-gates-test.txt` / `13-gates-msgcodegen.txt` | 四门禁 | rc=0 / rc=0 / **100 suites 1174 passed 0 failed 22 ignored** / rc=0（604） |
| `14-boundary.txt` | 边界 | 全在 `mir2-rs/tests/parity/m3/**`；冻结面空；`*.rs`=0 |

输入件（合成、LF）：`rust-measured-synthetic.tsv`、`csharp-measured-synthetic.tsv`、
`rust-pin-failed-synthetic.tsv`；变异表副本：`table-before.tsv`、`table-mut-*.tsv`。
