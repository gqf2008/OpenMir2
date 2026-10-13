# 证据包：M3⑤ 商店夹具「场景真源」收口（reviewer-6 M-3）——2026-10-13

- 卡：`openmir2-m3-shop-scenario-authority`；branch `worktree-m3-shop-scenario-authority`；
  交付时已 `git merge origin/master`（`d4d5b89c`，含 ④-C monster-ai / ⑤-5 storage-persist / whitelist-registry）⇒ 门禁/自检在**合并树**上重跑。
- 被审面：`mir2-rs/tests/parity/m3/{npc_shop_diff.py,run_npc_shop_pair.ps1}`；**零 `.rs`**；零起栈、零端口。
- hash 口径：`SHA256SUMS.txt` 按**仓库口径**（LF 归一 / git blob 字节）记；`eolcheck` 复核。

| 文件 | 内容 | rc |
|---|---|---|
| `01-npc-shop-diff-selftest.txt` | `python -I npc_shop_diff.py selftest` → **32/32**（含场景真源 5 例） | 0 |
| `02-npc-shop-pair-selfcheck.txt` | `run_npc_shop_pair.ps1 -SelfCheckNoStack` → **20/20**（含场景真源 runner 级 3 例） | 0 |
| `03-mutation-runner-red.txt` | 改坏：runner 去掉 `--scenario $ScenarioFile` → **18/20**（`scenario-authority-green`、`scenario-authority-prereq` 转红），已还原 | 1 |
| `04-mutation-comparator-red.txt` | 改坏：`load_scenario_frame_scalars` 缺键时静默退回内建表 → **31/32**（`scenario-missing-key-prereq` 转红），已还原 | 1 |
| `05-gate-fmt.txt` | `cargo fmt --all -- --check`（cwd=`mir2-rs`；`CARGO_TARGET_DIR=E:/tmp/m3cs-target`） | 0 |
| `06-gate-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` | 0 |
| `07-gate-test.txt` | `cargo test --workspace --offline`（合并树）→ **94 suites / 896 passed / 0 failed / 21 ignored** | 0 |
| `08-gate-msgcodegen.txt` | `cargo run --offline -q -p msgcodegen -- --check` → **604** | 0 |

复跑：

```sh
cd mir2-rs/tests/parity/m3
python -I npc_shop_diff.py selftest
cd <worktree>
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_npc_shop_pair.ps1 -SelfCheckNoStack
python -I tools/evidence-hash/verify_sha256sums.py --check-extra \
  mir2-rs/tests/parity/m3/evidence-m3-shop-scenario-authority-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck \
  mir2-rs/tests/parity/m3/evidence-m3-shop-scenario-authority-20261013
```

> `03` / `04` 的 RED 是**改坏必红**（牙齿）证据，非产品失败；运行前后工作树都以备份还原，
> 最终树 = `git diff --stat` 仅 2 个脚本。真机 DoD②③ 不在本片（等窗口）。
