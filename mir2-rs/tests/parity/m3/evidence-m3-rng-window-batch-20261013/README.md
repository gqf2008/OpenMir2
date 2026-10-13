# 证据：RNG 同窗口批次前提机判（本卡 scenario vs ④ drop-exp scenario）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道：`m3bag` / principal `openmir2-worker-4` / 分支 `wip/m3bag-window-batch-premise`
- base = `origin/master` `56d397865`（开工前 `git fetch && git merge --ff-only`）
- 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs`**；未起栈、未占端口（判据全离线）

## 1. 缺口

卡 DoD④ / `rng-trace-scene/README.md §4` 把本卡与 ④ `openmir2-m3-drop-exp-align`
编排在**同一窗口批次**串行跑，前提是两个场景件**同图 / 同种子 / 同起点**。这条此前只有散文：
`rng_scene_gate.py` 只比同一张卡的两侧 trace、`drop_exp_purity_gate.py` 只看 ④ 纯度 ⇒ 任一
scenario 漂移时批次不可比，而两个门各自 GREEN。详见
`../M3RNG同窗口批次前提机判报告-20261013.md`。

## 2. 命令（可复算）

```sh
cd mir2-rs/tests/parity/m3
python -I rng-trace-scene/check_window_batch.py check \
  --rng-scenario  rng-trace-scene/rng_trace_scenario.json \
  --drop-scenario drop-exp/drop_exp_scenario.json
python -I rng-trace-scene/check_window_batch.py selftest
# 四门禁（隔离 target，离线）：
cd ../../..      # mir2-rs
CARGO_TARGET_DIR=E:/tmp/m3bag-target cargo fmt --all -- --check
CARGO_TARGET_DIR=E:/tmp/m3bag-target cargo clippy --workspace --all-targets --offline -- -D warnings
CARGO_TARGET_DIR=E:/tmp/m3bag-target cargo test --workspace --offline --no-fail-fast
CARGO_TARGET_DIR=E:/tmp/m3bag-target cargo run -q -p msgcodegen --offline -- --check
# 证据复核（仓库 LF/git-blob 口径）
python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra mir2-rs/tests/parity/m3/evidence-m3-rng-window-batch-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-rng-window-batch-20261013
```

## 3. 原始件

| 件 | 内容 |
|---|---|
| `00-commands.txt` | 本片实际执行的命令与 cwd |
| `01-real-pair-green.txt` | 真件 `check` ⇒ `VERDICT GREEN` rc=0 |
| `02-compared-items.txt` | 从两个 scenario JSON 抽取的比较项（唯一真源） |
| `03-mutations-red.txt` | **改坏必红**：换图 / 换种子 / 挪 x / y / dir 各 rc=1 且点名 |
| `04-prereq-fail-closed.txt` | **PREREQ fail-closed**：缺件 / 坏 JSON / 缺字段 / 自比 / 传错件 / map 类型漂移 |
| `05-selftest.txt` | 工具 `selftest` 13/13 GREEN |
| `06-regression.txt` | 回归：`rng_scene_gate scenario` + ledger 37/37 + scene-gate 22/22 |
| `07-gates-fmt.txt` | `cargo fmt --all -- --check` rc=0 |
| `08-gates-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` rc=0 |
| `09-gates-test.txt` | `cargo test --workspace --offline --no-fail-fast`：101 suites / 1187 passed / 0 failed / 22 ignored / rc=0 |
| `10-gates-msgcodegen.txt` | `msgcodegen --check` 604 条 rc=0 |
| `11-boundary.txt` | `base..head` 路径清单 + 冻结面 + `.rs` 计数 |

## 4. 口径

- 本片是**窗口批次前提的牙**，不构成任何判据阈值；真机 DoD 仍 not_verified（需 owner 独占窗口 +
  两侧驱动承接卡）。
- `11-boundary.txt` 的 `head` 是**源代码/证据提交**；最终交付 head 只在其上追加 `11-boundary.txt`
  与 `SHA256SUMS.txt`（同目录内自指两件），边界结论不受影响（同 reviewer-5 O-2 口径）。
- `SHA256SUMS.txt` 按**仓库字节（LF 归一 / Git blob）**口径；`README.md` 也在清单内。
