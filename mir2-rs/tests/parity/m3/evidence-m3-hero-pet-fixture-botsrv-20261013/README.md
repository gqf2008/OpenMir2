# 证据包 —— M3⑨ 英雄/宠物（宝宝/下属 slave）动作序列夹具骨架（2026-10-13）

卡 `openmir2-m3-hero-pet-fixture-botsrv`（父卡 `openmir2-m3-hero-pet-align`）；报告
[`../M3⑨英雄宠物动作序列夹具骨架报告-20261013.md`](../M3⑨英雄宠物动作序列夹具骨架报告-20261013.md)。
分支 `worktree-m3-hero-pet-fixture-botsrv`；base `origin/master` `0dde01b7`。

| 件 | 内容 | 期望 |
| --- | --- | --- |
| `00-commands.txt` | 复跑命令 | — |
| `01-heropet-diff-selftest.txt` | `hero_pet_diff.py selftest` | `SELFTEST ok=36/36` rc=0 |
| `02-run-heropet-pair-selfcheck.txt` | `run_hero_pet_pair.ps1 -SelfCheckNoStack`（脚本自写 LF） | `SELFTEST ok=19/19` rc=0 |
| `03-botsrv-heropetseq-selftest.txt` | `BotSrv.exe --selftest-heropet` | `HEROPETSEQ SELFTEST GREEN` rc=0 |
| `04-run-bots-selfcheck.txt` | `run_bots.ps1 -SelfCheckNoStack`（新增 heropet kind） | `SELFTEST_RED GREEN` rc=0 |
| `05-selftest-gate.txt` | `selftest_gate.ps1 -SelfCheckNoStack`（回归） | `SUMMARY total=65 failed=0` |
| `06-reg-run-npc-shop-selfcheck.txt` | `run_npc_shop_pair.ps1 -SelfCheckNoStack`（回归） | `SELFTEST ok=17/17` |
| `07-reg-run-bag-equip-selfcheck.txt` | `run_bag_equip_pair.ps1 -SelfCheckNoStack`（回归） | GREEN rc=0 |
| `10-external-break-comparator.txt` | 外部改坏：去掉 `branch` 比较 | `FAIL mutate-branch-red` + `SELFTEST ok=35/36` rc=1 |
| `11-gate-fmt.txt` | `cargo fmt --all -- --check` | `FMT_RC=0` |
| `12-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` | `CLIPPY_RC=0` |
| `13-gate-test.txt` | `cargo test --offline --workspace --no-fail-fast` | 93 suites / 853 passed / 0 failed / 21 ignored，`TEST_RC=0` |
| `14-gate-msgcodegen.txt` | `cargo run -p msgcodegen -- --check` | 604 条，`MSGCODEGEN_RC=0` |
| `15-boundary.txt` | 写集边界（`src/sql/MirClient` + `crates/bins` 零改动） | 见文件 |
| `SHA256SUMS.txt` | 本目录全部文件（仓库 LF/git-blob 口径） | `--check-extra` GREEN；`eolcheck` GREEN |

口径：`SHA256SUMS.txt` 按**仓库字节（Git blob 内容 / LF 归一）**记；全部证据文件为 `.txt`/`.md`。
真机两侧同序列跑等 ⑨-2/⑨-3/⑨-4 + 时钟注入 + owner 窗口；本片未起栈、未占端口。
