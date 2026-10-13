# evidence-m3-tiered-acceptance-20261013 — 验收判据分级（tiered）仓库侧落地自证

- **卡**：`openmir2-tiered-acceptance-policy`（issue `2732dfc96f38`；owner 2026-10-13 经 jev 决策，置信度 1.00）。
- **车道**：principal `openmir2-worker-5` ／ worktree `.claude/worktrees/m3cs` ／ branch `worktree-m3-tiered-acceptance`。
- **base**：`origin/master bdc508ef131b504e7b801204a6b0ec23c413b773`。
- **本片**：把该卡 `landing.repo_pending` 的三处文本落地，并对 `machine_checkable_acceptance` ①-⑤ 给可复现原始件。
  **零 `.rs` / 零 `src/**` / 零表结构·脚本语法·数据格式改动；零起栈、零端口。**

## 落地了什么（repo_pending → 实际改动）

| # | 目标 | 改动 |
| --- | --- | --- |
| 1 | `AGENTS.md` 补「判据分级」节 | 新增 `### 判据分级（tiered acceptance …）`：R/L 定义、`review` 必带 `tier`/`tier_why`、缺 `tier` 按 R、R 级卡合并后不得 `closed`；顶部目标段加一行指针。 |
| 2 | `docs/Rust重写设计文档.md` §17.1 + §6 判据表 | §17.1 新增分级段（窗口要求只对 R 级生效）；§6 M3 判据表后加分级标注；§6 M5 加「**①72h 长跑 / ②`mem-probe` 内存斜率 / ③压测 P99 / ④灰度-回滚演练 / ⑤DB 快照 五项 = R 级占窗**」（**F-1 订正**：初版误把 ②④ 列为 L 级，与政策 L 定义及本节 §17.1 的「M5 需独占窗口」冲突，现改回 R）。 |
| 3 | `mir2-rs/tests/parity/M3-report.md` §2 | §2 顶部与 §2.5 标注：24h 长跑与逐系统对拍 = **R 级窗口项**，**L 级片不再占用窗口**。 |

## 验收 ①-⑤ 自证（逐条 → 原始件）

| 验收 | 结论 | 原始件 |
| --- | --- | --- |
| ① ≥1 条已落地 review 带 `tier`/`tier_why` 且结论与之一致 | **成立** | `01-runtime-tier-fields.txt`（123 条带 tier 字段的条目）；`04-l-tier-smoke-sample.txt`（3 张 L 级卡：approve 的 `tier_why` + 等价方法 + 抽样冒烟读数 + merge gates）。例：`openmir2-m3-guild-castle-fixture-botsrv`（tier=L，比较器 selftest 36/36 + mutation 牙 + 证据 19/19）。 |
| ② R 级卡合并后 `status ≠ closed` | **成立** | `02-r-tier-merge-not-closed.txt`。例：`openmir2-m3-castle-gold-tax`（review tier=R, window_required=true → merge `510daba5a` → 最新 status `in-progress`）；`openmir2-m3-rng-trace-scene-groups`（card_tier=R，多次合并后仍非 closed）。 |
| ③ 缺 `tier` 的旧 approve ⇒ 合并按 R 处理并在卡上留 `comment`（实测） | **规则已落地；post-policy 无实测实例 ⇒ `not_verified`（不伪造）** | `03-missing-tier-rule.txt`：规则原文 `brief-merge.md:67-69`（「**review 缺 `tier` 字段 ⇒ 按 R 处理**（保守），并在卡上 `comment` 说明」）。tier 政策 2026-10-13 08:37Z 发布后所有 `review` 均带 `tier`，`merge_result` 亦带 `tier`，故尚无「缺 tier 的旧 approve」被提交；pre-policy（<08:37Z）的无 tier merge 属政策发布前行为、不属本口径。 |
| ④ 抽查 3 张 L 级卡的抽样冒烟证据存在 | **成立** | `04-l-tier-smoke-sample.txt`（guild-castle-fixture-botsrv / pet-slave-summon / selloff-consignment-world 各自的 approve `tier_why` + 构造级/进程级冒烟 + 证据 hash + merge gates）。 |
| ⑤ 三处文本落地后 grep `tier` 命中 | **成立** | `05-grep-tier-hits.txt`（`AGENTS.md` / `docs/Rust重写设计文档.md` §17.1+§6 / `M3-report.md` §2）。 |

## 四门禁

`06-gates.txt`：隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`、`--offline`、cwd=`mir2-rs/`：
**fmt rc=0 / clippy rc=0(-D warnings) / test 99 suites 1159 passed 0 failed 22 ignored rc=0 / msgcodegen 604 rc=0**。

## 清单与复核

> **F-1/O-1/O-2 返工（reviewer-6 review `4126a17a` @ `76bd2ceb8`）**：
> - **F-1（Important）**：§6 M5 把 ②`mem-probe` 内存斜率 / ④灰度-回滚演练 判成 L —— 已**改为 R**（见上表 #2；
>   ② 只能真机测得且并发编译/压测会污染斜率、④ 写回存档/DB 命中「持久化落库」，均不满足 L「不可能只在真机暴露差异」的定义）。
> - **O-1**：§6 M3 分级段原把「经验曲线」归入「持久化落库」桶 —— 已**移到「RNG 序列」桶**（本文件行 316）。
> - **O-2**：`AGENTS.md` 顶注「见本文件「判据分级」节与 §17.1」易误读为 `AGENTS.md` 自身节 —— 已写成
>   `docs/Rust重写设计文档.md` §17.1。
> - 三条均属文本订正，**本片仍为零 `.rs`**，上表 `#2` 的落地文本与 `05-grep-tier-hits.txt` 已按订正后重采。

- `SHA256SUMS.txt`：按**仓库字节（LF 归一 = git blob）**口径记。
- 复核：`python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-tiered-acceptance-20261013`。
- 采集器：`collect.py`（只读 `refs/collab/*` 的 `git cat-file --batch`，不联网/不起栈/不占端口）。

## 边界与纪律

- 只在本 worktree 写/提交/推（`AGENTS.md`、`docs/**`、`mir2-rs/tests/parity/**`）；**未动共享主检出**；**未 merge、未自签 review**。
- 未与其它车道写集相撞：本片不含 `crates/**`、`bins/**`、`src/**`、`sql/**`、`MirClient`、`tests/parity/m3/{drop-exp,rng-trace-probe,run_npc_shop_pair.ps1,…}`。
- **残留物**：无（采集只读 git 对象库与 owner 侧 runtime briefs；未生成 worktree 之外的需登记物）。
- `not_verified` 项：验收③（见上）；owner 侧 runtime 落地（`brief-review*.md` / `brief-merge.md` / `TIER-POLICY.md`）不在本仓、本片只引用原文。
