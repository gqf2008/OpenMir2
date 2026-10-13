# 证据：M3 RNG 场景两相 trace「承接卡」规格落库（`openmir2-m3-rng-scene-driver`）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道：`m3bag` / principal `openmir2-worker-4` / 分支 `wip/m3bag-window-batch-premise`
- base = `origin/master` `5964479f3`（开工前 `git fetch origin && git merge origin/master`，FF）
- 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs`**；未起栈、未占端口（判据全离线）

## 1. 缺口与依据

卡级真机 DoD（同场景含怪两相 trace + `groupdiff --fold --strict` / `-SceneGroupDiff` diff=0）**唯一阻塞**
= 两侧『场景描述件 `<side>-scene.json` + 全场景抽数流水 `<side>-rng-stream.tsv`』的生产者不存在；
其写集在 `crates/world/**` + `bins/game-svr/**` + `src/**`，超出本卡 10:35 派单的写集禁令。
owner 已就此给出承接卡规格（thread 条目 `222bffe3011c1adeed6bffdb6ccfedf82c791ca5` 的 `missing_driver` +
`4ca2854c0b44cecf9027f6ff83c48486dba73093` 的 `driver_card_spec_delta`），但此前**只留在 thread body**。

依 `AGENTS.md` §5「面向人的报告作为文件进仓库（`docs/`），不要只留在 thread body」，本片把该规格
**落成仓内文件** `rng-trace-scene/DRIVER-CONTRACT.md`（+ README §8 指针），使承接卡落地时无需回翻
thread / 现场误配（尤其避免浪费一次本卡与 ④ 共用的**独占窗口**）。

> 本片**不新增判据阈值、不改任何门的口径**；只是把 owner 已定的规格转写入库，并把其引用的每个
> 锚点在本片实测（`01-anchor-check.txt`）。

## 2. 命令（可复算）

见 `00-commands.txt`（含 anchor grep、real-run fail-closed、离线回归、四门禁、边界、verify/eolcheck）。

## 3. 原始件

| 件 | 内容 |
|---|---|
| `00-commands.txt` | 本片实际执行的命令与 cwd |
| `01-anchor-check.txt` | 规格里每个 anchor 的实测行号（宿主 AI / oracle / ④ 纯度门 / R-6 折叠 / scenario 字段 / MonGen 0 行） |
| `02-driver-unwired-fail-closed.txt` | `run_rng_trace_pair.ps1 -Go -PinScene` ⇒ `PREREQ_MISSING` rc=2（承接卡未落地，正确 fail-closed） |
| `03-regression.txt` | 离线回归：ledger 37/37、consumer_groups rc=0、scene gate 22/22 + scenario GREEN、window-batch check GREEN + selftest 13/13、purity gate 23/23、`-SelfCheckNoStack` 29/29 + 13/13 |
| `04-gates-fmt.txt` | `cargo fmt --all -- --check` rc=0 |
| `05-gates-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` rc=0 |
| `06-gates-test.txt` | `cargo test --workspace --offline --no-fail-fast`：101 suites / 1190 passed / 0 failed / 22 ignored / rc=0 |
| `07-gates-msgcodegen.txt` | `msgcodegen --check` 604 条 rc=0 |
| `08-boundary.txt` | `base..head` 路径清单 + 冻结面 + `.rs` 计数 |

## 4. 口径

- **层级 = L**（文档片：不可能只在真机暴露差异）。审查者在**钉住的 commit** 上自证：本片改的
  `DRIVER-CONTRACT.md` 内容逐条来自 owner thread 条目 + `01-anchor-check.txt` 的实测锚点，且不改
  任何代码/工具；抽样冒烟 = `03-regression.txt` 的离线链（只读复跑全 GREEN）。
- **边界**：`08-boundary.txt` 的 `head` 是**源代码/证据提交**；最终交付 head 只在其上追加
  `08-boundary.txt` 与 `SHA256SUMS.txt`（同目录自指两件），边界结论不受影响（同既有 O-2 口径）。
- `SHA256SUMS.txt` 按**仓库字节（LF 归一 / Git blob）**口径；`README.md` 也在清单内。
- 真机 DoD 仍 **not_verified**（需承接卡落地 + owner 独占窗口）；本片不改变该状态。
