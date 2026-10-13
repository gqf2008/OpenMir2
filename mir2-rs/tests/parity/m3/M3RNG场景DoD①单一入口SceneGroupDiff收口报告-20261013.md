# M3 RNG 站点组 DoD① 单一入口（`-SceneGroupDiff`）收口报告（2026-10-13）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道：`m3bag`／principal `openmir2-worker-4`／worktree `m3bag`／分支 `worktree-m3-rng-trace-scene-groups`
- base = `refs/heads/master` `029d23eae`（开工即头部；零 `.rs`、未起栈、未占端口）；**最终交付前已跟 master 至 `636bdce53`（见 §6）**
- 写集：`mir2-rs/tests/parity/m3/**`。**零** `crates/**`、`bins/**`、`src/**`、`tests/parity/world/**`。
- 证据包：`evidence-m3-rng-trace-scene-groups-20261013-scenegroupdiff/**`（23 件；含 M-1 订正件 `19-`/`20-`）

> 一句话：把卡 DoD① 从「靠操作者记得先跑门」补成**机判的单一入口**——`-SceneGroupDiff` 先跑场景一致性
> 前置门，**非 GREEN ⇒ 拒比**（不跑 `groupdiff`、绝不输出判据结论），GREEN 才比并原样传播 groupdiff 的
> rc/结论；配一对改坏必红的牙（既不许「门红还比」，也不许「恒拒比/吞结论」）。

---

## 1. 缺口（DoD① 未被机判拦住）

DoD① 原文：两侧 trace 必须同场景/同起点/同种子（MonGen 与 Monster 集合同口径），**不满足 ⇒
`groupdiff` fail-closed 拒比（不许把场景差异当实现差异，也不许当绿）**。

此前夹具提供的路径是两条**独立**命令：

```
run_rng_trace_pair.ps1 -GateOnly      -AScene … -BScene …          # 场景门（可判红/绿）
run_rng_trace_pair.ps1 -GroupDiffOnly -AStream … -BStream …        # groupdiff --fold --strict
```

`-GroupDiffOnly` **从不**调用场景门 ⇒ 只要操作者漏跑第一条（或把两条分开在不同时刻跑），就会在
**不同场景**的两份流水上照样得到 `VERDICT IDENTICAL/RED`，把场景差异读成实现差异。DoD① 的「不满足 ⇒
`groupdiff` fail-closed 拒比」**没有落在机器上**。既有 19 项无栈自检也**没有**任何一项覆盖
「门非绿 ⇒ groupdiff 不得出结论」。

## 2. 收口（本片改动）

只改 `mir2-rs/tests/parity/m3/rng-trace-scene/**`（+ 证据/报告），**零 `.rs`**：

1. **新增模式 `-SceneGroupDiff`（DoD① 单一入口）**，参数 `-AScene -BScene -AStream -BStream`（缺任一 ⇒
   `PREREQ_MISSING` exit 2）。顺序固定：
   - 跑 `rng_scene_gate.py gate`（用与 `-GateOnly` **同一个** `Invoke-RngSceneGate` 实现，不另立一套）；
   - `gate rc != 0` ⇒ 打印门输出 + `REFUSED: …不跑 groupdiff（DoD① fail-closed 拒比）`，`exit gate rc`
     （rc=1 场景漂移 / rc=2 缺件）；**绝不**执行 groupdiff、**绝不**输出 `RNG_CONSUMER_GROUPDIFF` 行；
   - `gate rc == 0` ⇒ 打印 `GREEN(scene): …` 后跑 `groupdiff --fold --strict`，`exit diff rc`，**原样传播**
     groupdiff 的文本与退出码（不吞结论）。
2. 核心逻辑抽成 `Invoke-RngSceneGroupDiffCore`（**无 stdout 副作用**，返回 `{rc, gateRc, diffRc,
   ranDiff, text}`），供 mode 块与无栈自检**共用同一实现**（避免 mode 与 selftest 两套断言漂移）。
3. `-GateOnly` / `-GroupDiffOnly` **语义不变**（后者保留为跨场景对照的**裸比较**口径）；README/场景件
   写清「`-GroupDiffOnly` 不构成 DoD① 判据」。
4. 无栈自检新增 4 项（19 → **23**）并各带改坏必红；`scenario-runner-params-declared` 依旧绿（新参数都
   在 `param()` 里声明）。

## 3. 判据与读数（原始件见证据包）

| # | 项 | 读数 | 件 |
|---|---|---|---|
| 1 | `-SelfCheckNoStack` | **23/23 GREEN** rc=0（原 19 + 新 4） | `01-` |
| 2 | 新 ⑲ `scene-groupdiff-identical-green` | 同场景 + 等价账本 ⇒ rc=0，文本含 `GREEN(scene)` 且 `VERDICT IDENTICAL` | `01-` |
| 3 | 新 ⑳ `scene-groupdiff-scene-drift-refused` | 怪集合漂移 ⇒ rc=1 且 `ranDiff=False`、文本 `REFUSED` 且**无** `RNG_CONSUMER_GROUPDIFF` | `01-` |
| 4 | 新 ㉑ `scene-groupdiff-missing-prereq-refused` | 缺场景件 ⇒ rc=2 且同样拒比 | `01-` |
| 5 | 新 ㉒ `scene-groupdiff-diff-red-propagates` | 门绿 + 账本确缺一组 ⇒ rc=1 且**传播** `RNG_CONSUMER_GROUPDIFF`（防恒拒比） | `01-` |
| 6 | 改坏必红 MUT-1（去掉拒比） | **21/23**，⑳/㉑ FAIL，rc=1 | `02-` |
| 7 | 改坏必红 MUT-2（恒拒比/吞结论） | **21/23**，⑲/㉒ FAIL，rc=1 | `03-` |
| 8 | 回归：场景门 selftest / 账本 selftest / attack 账本 / scenario 模式 | 16/16 · 32/32 · 32/32 · GREEN | `04-`–`07-` |
| 9 | 回归：`-GroupDiffOnly` 绿/红、`-GateOnly` 绿 | rc=0 / rc=1 / VERDICT GREEN rc=0（语义不变） | `08-`–`10-` |
| 10 | 夹具矩阵 / 链深检（runner 级） | `FIXTURE_MATRIX GREEN ready=9 pending=1 red=0` · `FIXTURE_CHAIN GREEN runner_ok=9 prereq_or_skip=1 red=0` | `12-`/`13-` |

四门禁（cwd=`mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/m3bag-target`，`--offline`）：`fmt` rc=0 · `clippy -D
warnings` rc=0 · `test` **98 suites 1038 passed 0 failed 21 ignored** rc=0 · `msgcodegen --check` 604 条 rc=0
（`14-`–`17-`）。

边界（`11-`）：唯一非证据改动 = `rng-trace-scene/{run_rng_trace_pair.ps1,README.md,rng_trace_scenario.json}`；
`git diff --name-only HEAD -- src sql MirClient mir2-rs/crates mir2-rs/bins` = **空**；`*.rs` = **空**。

## 4. 与既有结论的关系

- R-6 folded 映射、场景一致性前置门、`drop`↔④ 兼容性**均未改动**（本片只把「门 → 比」串成一个
  fail-closed 入口）；
- 本片**不**放宽任何判据：`-SceneGroupDiff` 在门红时比原来**更严**（原来裸 `-GroupDiffOnly` 会给结论）；
- 真机两相 trace 仍需 owner 独占窗口 + 两侧驱动承接卡（写集 `crates/world`/`bins/**`/`src/**`，超出本卡
  写集）；`run_rng_trace_pair.ps1` 的 real-run 在无 `-Go`/`-PinScene` 时仍 fail-closed exit 2。

## 5. 残留物清单

- 仓外临时件：`E:/tmp/m3bag-sgd/**`（本片证据原始件 + `dbg/` 合成两相件 + `mut/` 改坏副本，均可清）。
- worktree 内未跟踪 `mir2-rs/tests/parity/m3/sessions/csharp-20261012-2323..2346/`（8 个，**别车道遗留**，
  未纳入本片、未清）。
- 十端口（5000/5100/5500/5600/5700/6000/7000/7100/7200）本 burst **全 DOWN**（仅共享 3306）⇒ 未起栈、未占端口。

## 6. 追加：交付前 base 跟 master（2026-10-13，DoD① 片重交）

交付待审期间 `origin/master` 前进（`029d23eae` → `636bdce53`）。若评审直接 `git diff origin/master <旧 head>`，
会带出本片**未改动**的他线文件（`bins/game-svr/src/{enter,main,record,trade}.rs` 呈回退、他线已合入的
`evidence-m3-party-deal-try-null-pose-20261013/**` 与 `tests/parity/world/client-e2e-enter-gaps-20261013/**`
呈删除）——正是本项目登记过的「base 未含 master ⇒ 回退假差量」坑。处置：

- `git fetch origin && git merge origin/master`（**零冲突、无内容改动**；本片 3 个源件逐字节不变）；
- 假差量读数：合并前 `git diff --name-status origin/master 989a8c9af | wc -l` = **63**；合并后
  （尚未加入本记录时）`git diff --name-status origin/master HEAD | wc -l` = **24**。**最终交付边界 = 25 路径**
  （1 报告 + 21 证据件（`00`–`18` + `README.md` + `SHA256SUMS.txt`）+ 3 源件），与 `11-boundary.txt` 同口径；
- 合并树四门禁重跑（隔离 `CARGO_TARGET_DIR=E:/tmp/m3bag-target`，`--offline`）：fmt rc=0 · clippy rc=0 ·
  **98 suites / 1044 passed / 0 failed / 21 ignored** rc=0 · msgcodegen 604 条 rc=0；
- 合并树离线判据重跑：无栈自检 **23/23** · ledger selftest **32/32** · scene gate selftest **16/16** ·
  scenario **VERDICT GREEN** · attack ledger **32/32** · `FIXTURE_MATRIX GREEN ready=9 pending=1 red=0` ·
  `FIXTURE_CHAIN GREEN runner_ok=9 prereq_or_skip=1 red=0`；
- 原始读数入库：证据包 `18-post-merge-master-regate.txt`（`16-gate-test.txt` 记的是未合 master 的
  `989a8c9af` 时读数 1038，**合并树读数以 18 为准**）。

> 注：合并 `origin/master` 只推进 base，**不**改变本片任何判据或内容；`-SceneGroupDiff` 的行为与
> 改坏必红牙齿不变。真机两相 trace 仍等 owner 独占窗口。

## 7. 追加：reviewer-5 M-1 收口（2026-10-13，非阻塞 Minor）

coordinator 合并检查点（oid `2785da26`）携带 reviewer-5 的非阻塞 Minor M-1：证据 README / `18-post-merge-
master-regate.txt` 记「24 路径」而交付头实测 **25**。复核与处置：

- 复核 `git diff --name-only 636bdce53 8f30c8e68` = **25 路径**（1 报告 + 21 证据件 + 3 源件）；`18-` 末段
  已给出正确口径（「24 是尚未加入 `18-` 本文件时的读数；最终交付头 = 25」），缺的是 README 不再硬编码。
- **根因**：证据包每增删一件，路径计数就变一位 ⇒ 硬编码计数下次触碰必再变 doc-vs-事实（与本卡
  `scenario-fold-matches-code` 同族）。
- **处置**：证据 README 的 18- 行删去硬编码数字，改为只写**计数规则**（报告 + 本证据包全部文件 + 源件），
  新增原始件 `19-reviewer5-M1-pathcount-correction.txt`（含复核读数与本订正片改动面）。`18-` **不改**
  （原始转录，不事后编辑）。
- `SHA256SUMS.txt` 已按仓库字节（LF 归一 / git blob）重写以覆盖新增件；`--check-extra` 与 `eolcheck` 重跑绿。
- 订正片验证读数入库 `20-m1-correction-verify.txt`：四门禁 fmt rc=0 · clippy rc=0 · test **98 suites 1057 passed
  0 failed 21 ignored** rc=0 · msgcodegen 604 rc=0；离线判据 ledger 32/32 · scene gate 16/16 · scenario GREEN ·
  attack ledger 32/32 · `FIXTURE_MATRIX GREEN ready=9 pending=1 red=0` · `FIXTURE_CHAIN runner_ok=9`；无栈自检
  **23/23**；`--check-extra` 全绿。

真机两相 trace（DoD 剩余半边）仍等 owner 独占窗口 + 两侧驱动承接卡；本订正片零 `.rs`、零起栈、零端口。
