# 证据包 —— 承接卡规格「源码锚点保鲜」机判（2026-10-13 第二次窗口前置审计）

卡：`openmir2-m3-rng-trace-scene-groups`（m3bag / `openmir2-worker-4`）
分支：`wip/m3bag-window-batch-premise`
base：`origin/master` = `1b0dad401`（本卡前两片已合入该 head；`05c3b4283` 是其祖先）
tier：**L**（纯离线表/文档/工具片，零 `.rs`）；卡级真机 DoD 仍 **R / not_verified**。

## 1. 本片修什么（真缺陷，不是空转）

卡的**离线①②③ 均已达成并入 master**（Claim B 账本工具、同场景两相 trace 场景夹具 + 前置门、
README drop/④ 兼容口径、纯度门站点表）。剩余 DoD = **真机两侧两相 trace + `groupdiff --fold --strict`**，
阻塞在两张卡外条件上（承接卡 `openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口）。

本 burst 不重复复核既有结论，而是继续**跨车道漂移审计**（master 一直在动 ⇒ 固定场景的判据
与文档会被新落地的实现甩开），发现一个会在承接卡落地时**误导实现**的真缺陷：

- `DRIVER-CONTRACT.md`（承接卡规格，2026-10-13 落库）§4 用 `path:line` 引用承重源码锚点；
- master 在落库**之后**又合入 ④-D（`4514352b7`）与 client-e2e（`0f38d642d`）两条片，
  都改了 `mir2-rs/bins/game-svr/src/main.rs` ⇒ 行号增长；
- 实测 `enable_production_monster_ai(&mut world)` 从 **`:5229` 漂到 `:5440`**，而文档仍写 `:5229`。
  ⇒ 承接卡按旧行号去源码里找「宿主 AI 调用点」会找错位，且**没有任何门会拦**。

## 2. 交付物

| 文件 | 变更 |
| --- | --- |
| `rng-trace-scene/DRIVER-CONTRACT.md` | §4 锚点 `main.rs:5229` → **`:5440`**；§6 新增一条「本文档自身锚点保鲜」说明 |
| `rng-trace-scene/check_anchors.py` | **新增**：把「文档行号 == 当前源码里该符号的**首个**匹配行」钉成 fail-closed 机判；`check` / `check --doc` / `selftest`（含 3 条改坏必红） |
| `rng-trace-scene/run_rng_trace_pair.ps1` | `-SelfCheckNoStack` 新增 `anchors-freshness-selftest` ⇒ 自检 **29/29 → 30/30** |
| `rng-trace-scene/README.md` | 新增 §5.2（锚点保鲜口径 + 命令 + 改坏必红）+ §6 命令清单 |

`check_anchors.py` 的锚点表只收**文档真正承重的 5 条**：宿主 AI 调用点 / 宿主 AI 开关接缝 /
世界层 `monster_ai_tick` / `wondering` 抽数入口 / oracle `AnimalObject.Wondering`（冻结面，仅防误抄）。
判据 = 「文档里 `` `<literal>:<line>` `` 取出的行号 == 该文件里符号正则的**首个**匹配行」：
缺失 / 重复（歧义）/ 文件缺失 / 符号缺失 / 行号漂移 ⇒ rc=1 逐个点名（漂移还报出正确实际行号）。
它**不写任何文件**（selftest 只用内存文本）。

## 3. 判据读数（全 GREEN / 改坏必红真跑）

| 项 | 命令（cwd） | 读数 |
| --- | --- | --- |
| **改前复现**（仓库内 HEAD 的旧文档） | `python -I check_anchors.py check --doc /e/tmp/m3bag-old-driver-contract.md` | **rc=1** `ANCHOR_DRIFT 文档写 main.rs:5229，但首个匹配在 :5440` |
| **改后** | `python -I check_anchors.py check` | **rc=0** `GREEN 5/5 锚点保鲜` |
| 机判（新门） | `python -I check_anchors.py selftest` | **4/4 GREEN**（含行号漂移 / 锚点缺失 / 锚点歧义 3 条改坏必红） |
| 无栈自检（接入后） | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | **30/30 GREEN**（含 `anchors-freshness-selftest`；本片之前 29/29） |
| 离线全链回归 | 见 `04-offline-regression.txt` | `rng_consumer_groups 12/12`、`rng_ledger_diff 37/37`、`rng_scene_gate 22/22`、`check_window_batch 13/13`、`check_evidence_counts 7/7`、`check_anchors 4/4`、`attack_rng_ledger 32/32`、`drop_exp_purity_gate 23/23`、`drop_exp_diff 12/12` 全 rc=0 |
| 四门禁（`CARGO_TARGET_DIR=E:/tmp/m3bag-target`, cwd=mir2-rs） | fmt / clippy `-D warnings` / workspace test / msgcodegen | rc=0 / rc=0 / **101 suites 1199 passed 0 failed 22 ignored** / 604 rc=0 |

## 4. 边界

`git diff --name-only 1b0dad401 HEAD` = **16 路径**（4 源：`DRIVER-CONTRACT.md` / `README.md` /
`run_rng_trace_pair.ps1` / 新增 `check_anchors.py`；证据 12 件：本目录 `01`–`09` + `10-boundary.txt`
+ `README.md` + `SHA256SUMS.txt`），**全在 `mir2-rs/tests/parity/m3/rng-trace-scene/**` 与
`mir2-rs/tests/parity/m3/evidence-m3-rng-scene-anchor-freshness-20261013/**`**，冻结面（`src/`、`sql/`、
`MirClient`、`mir2-rs/crates/protocol`、`mir2-rs/crates/storage`、`mir2-rs/bins`）**空**，`*.rs` = 0。

## 5. 残留 / 登记

- 卡级 DoD（真机两相 trace + `groupdiff --fold --strict`）**未做**：唯一阻塞为
  ① 承接卡 `openmir2-m3-rng-scene-driver` 未开（两侧 `<side>-scene.json` + `<side>-rng-stream.tsv`
  生产者在 `crates/world/**` + `bins/game-svr/**` + `src/**`，本卡 10:35 派单明令禁动）；
  ② owner 独占窗口（与本卡 + ④ 真机 100 杀同批串行）。⇒ 按 R 保持 `not_verified`。
- 非阻塞观察（随卡带走）：`consumer_groups.tsv` 与 `drop_exp_consumers.tsv` 对
  `WorldServer.MonGen.cs` 的 `MonGetRandomItems` 分别记 `:440` / `:439`（同一方法签名在 439、
  方法体起始在 440）；C# 为冻结面 ⇒ 无行为影响，本片不churn。
- 未动 `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`、`drop-exp/**`（④ 线在写）；
  只在自己 worktree 写；共享主检出零改动；零起栈 / 零端口。
