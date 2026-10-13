# 证据包：M3 RNG 站点组 DoD① 单一入口（`-SceneGroupDiff`）收口（2026-10-13）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-trace-scene-groups`
- 报告：`../M3RNG场景DoD①单一入口SceneGroupDiff收口报告-20261013.md`
- 夹具：`../rng-trace-scene/`（scenario + 前置门 + 驱动 + README）

## 这一片修的是什么（DoD① 缺口）

DoD① 原文：两侧 trace 必须同场景/同起点/同种子（MonGen 与 Monster 集合同口径），**不满足 ⇒
`groupdiff` fail-closed 拒比（不许把场景差异当实现差异，也不许当绿）**。此前 `-GroupDiffOnly` 只跑
`groupdiff --fold --strict`，**从不**跑场景门 ⇒ 「场景差异当实现差异」这条不是机判拦下的，而是靠操作者
记得先跑 `-GateOnly`（操作纪律）。本片加 **`-SceneGroupDiff`** 单一入口：**先**跑场景一致性前置门，
非 GREEN（rc=1/2）⇒ **拒比**（不跑 groupdiff、绝不输出任何判据结论）；GREEN 才跑 groupdiff 并原样
传播其 rc/文本。`-GateOnly` / `-GroupDiffOnly` 语义不变（后者保留为跨场景对照的**裸比较**口径）。

## 件清单

| 文件 | 内容 |
|---|---|
| `00-commands.txt` | 全部可复算命令（含合成件生成口径） |
| `01-selftest-nostack.txt` | `run_rng_trace_pair.ps1 -SelfCheckNoStack` ⇒ **23/23 GREEN**（原 19 + 新 4；LF 转录由脚本自己落盘） |
| `02-red-mutation-refusal-removed.txt` | **改坏必红 A**：去掉「门非 GREEN ⇒ 拒比」⇒ **21/23**，⑳ `scene-groupdiff-scene-drift-refused` + ㉑ `…missing-prereq-refused` FAIL |
| `03-red-mutation-always-refuse.txt` | **改坏必红 B**（反向对照）：门绿后仍恒拒比/吞结论 ⇒ **21/23**，⑲ `…identical-green` + ㉒ `…diff-red-propagates` FAIL |
| `04-scene-gate-selftest.txt` | `rng_scene_gate.py selftest` ⇒ **16/16 GREEN** |
| `05-ledger-selftest.txt` | `rng_ledger_diff.py selftest` ⇒ **32/32 GREEN**（含 R-6 folded；回归：未改语义） |
| `06-attack-ledger-selftest.txt` | `attack_rng_ledger.py selftest` ⇒ **32/32 GREEN**（回归） |
| `07-scenario.txt` | `rng_scene_gate.py scenario` ⇒ **VERDICT GREEN** |
| `08-groupdiff-only-unchanged-green.txt` | 回归：`-GroupDiffOnly` 等价账本 ⇒ rc=0（原语义不变） |
| `09-groupdiff-only-unchanged-red.txt` | 回归：`-GroupDiffOnly` 缺组 ⇒ rc=1（原语义不变） |
| `10-gate-only-unchanged.txt` | 回归：`-GateOnly` 同场景 ⇒ **VERDICT GREEN** rc=0（原语义不变） |
| `11-boundary.txt` | 边界：仅 `rng-trace-scene/{run_rng_trace_pair.ps1,README.md,rng_trace_scenario.json}` + 本证据包；`src sql MirClient mir2-rs/crates mir2-rs/bins` 空、零 `.rs` |
| `12-fixture-matrix.txt` | `fixture_matrix.py check` ⇒ **FIXTURE_MATRIX GREEN ready=9 pending=1 red=0**（B=READY） |
| `13-fixture-chain.txt` | `fixture_chain_gate.py check` ⇒ **FIXTURE_CHAIN GREEN runner_ok=9 prereq_or_skip=1 red=0** |
| `14-gate-fmt.txt` | `cargo fmt --all -- --check` rc=0 |
| `15-gate-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` rc=0 |
| `16-gate-test.txt` | `cargo test --workspace --offline` ⇒ **98 suites / 1038 passed / 0 failed / 21 ignored** rc=0 |
| `17-gate-msgcodegen.txt` | `cargo run -q --offline -p msgcodegen -- --check` ⇒ 604 条 rc=0 |

## 改坏必红（原始件）

改坏在**仓外副本**（`E:/tmp/m3bag-sgd/mut/`，`cp -r` 自仓内两目录）完成，**未改动仓库树**：

- MUT-1：把 `if ($gateRc -ne 0)` 改成 `if ($false)`（门红仍继续比）⇒ ⑳/㉑ FAIL（`rc=0 ranDiff=True` / `rc=0`），rc=1；
- MUT-2：门绿后把 `groupdiff` 跳过（`$res.ranDiff=$false; $res.rc=1`）⇒ ⑲/㉒ FAIL，rc=1。

两条一起，正是本入口的一对牙：**既不许「门红还比」，也不许「恒拒比/吞结论」**。

## 口径

- `SHA256SUMS.txt` 按**仓库字节（LF 归一 / git blob）**记；复核：
  `python -I tools/evidence-hash/verify_sha256sums.py --check-extra <本目录>` 与 `… eolcheck <本目录>`。
- 合成两相件（descriptors/streams）由内联 python 生成到 `E:/tmp/m3bag-sgd/dbg`（仓外），生成口径见
  `00-commands.txt`；转录里的该路径为当次运行产物，不随包入库。
- 真机两相 trace **未跑**（等 owner 发独占窗口 + 两侧驱动承接卡）；本包为**离线**证据，零起栈、零端口。
  十端口实拍（本 burst）：5000/5100/5500/5600/5700/6000/7000/7100/7200 **全 DOWN**（仅共享 3306）。
