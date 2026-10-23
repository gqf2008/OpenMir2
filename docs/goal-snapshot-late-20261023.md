# 终点四条件快照（晚场后）：截至 `de7cb7779c` @ 2026-10-23T14:21:23Z（`--full`；快速模式首跑 14:16:35Z）

- 卡：`openmir2-goal-snapshot-late-20261023`（tier=L，写集 `docs/`）；worktree `E:/tmp/wt-worker-69`，分支 `card/goal-snapshot-late`。
- 复算命令（本文件所有动态数都由它产出）：`python -I tools/goal/goal_gate.py --json`（cwd=仓库根；`envir=E:/MirServer/M2GameSvr/Envir`）。
- **动态数一律标「截至 `<commit>` @ `<ts>`」**；历史量另标来源（§6）。**查不到的写查不到**，不猜。

---

## 0. 四条件总表（原文，未改写；以 `--full` 快照为准）

| # | 条件 | state | detail（原文） |
|---|---|---|---|
| ① | 脚本派发 100% | **pass**（`--full`）／~~need_human~~（快速模式，见 §1） | parse-stats 实测：目标 106/106（去重派发目标）；调用 11829 次；未实现 0 个 |
| ② | R 卡证据 | **fail** | R 卡 192 张：已完成 10 / window_required 121 / whitelist 6 / 已裁定登记 3 / comment 证据 0 / 三者皆无 3 / merged 未 closed 且无登记(需人工) 44 / 未开工或未合并 0（不计缺口）/ 其它需人工 5；FAIL 卡：`openmir2-gm-batch-5-guild-castle`, `openmir2-monster-attack-cadence-lead`, `openmir2-oracle-humitems-align`；需人工 49 张（按 last_status_ts 最旧优先，见 JSON need_items）；merged 未 closed 但已凭登记 PASS：124 张（卫生清单，见 evidence.merged_open_hygiene；登记型 PASS ≠ 已有真机证据） |
| ③ | B 类清零 | **pass** | B 类清零：recon tracked=0（rows 191 条非空、counts 一致）（recon 实测 tracked=0） |
| ④ | 门禁恒绿 | **pass**（`--full`）／~~need_human~~（快速模式，见 §1） | 门禁 7 项：绿 7 / 红 0 / 需人工 0 |

**overall = fail**（`--full`，`generated_ts=1792765283`）——**唯一未过的是条件②**（R 卡证据）；①③④ 全部 pass。
快速模式（`--json`，`generated_ts=1792765013`）的 ①④ 是 `need_human`（没跑），两次对照见 §1。

### 0.1 条件④的 7 项明细（`--full` 快照）

| 检查 | state | rc | detail |
|---|---|---|---|
| `fmt` | pass | 0 | fmt rc=0（2.3s） |
| `clippy` | pass | 0 | clippy rc=0（0.5s） |
| `msgcodegen` | pass | 0 | msgcodegen rc=0（0.2s） |
| `whitelist_anchor_gate` | pass | 0 | rc=0（0.6s） |
| `m3_whitelist_registry_audit` | pass | 0 | rc=0（0.1s） |
| `cargo_test_workspace` | **pass** | 0 | `cargo test --workspace rc=0（224.0s）` |
| `master_sync` | pass | — | 本地 master == walgit master（`de7cb7779c53`） |

> 条件④ **7/7 绿、红 0、需人工 0**。快速模式（未跑 cargo test）时这一项是 `need_human`——那不是「门禁红」，是「没跑」。

---

## 1. 快照是现取的（两次，数字逐项相同）

| 时点 | 命令 | overall | ① | ② | ③ | ④ |
|---|---|---|---|---|---|---|
| 2026-10-23T14:16:35Z（`de7cb7779c`） | `goal_gate.py --json`（快速） | fail | need_human | fail | pass | need_human |
| 2026-10-23T14:17:16Z（`git fetch --collab` 之后重跑） | 同上（快速） | fail | need_human | fail | pass | need_human |
| 2026-10-23T14:21:23Z（`generated_ts=1792765283`，同一 commit） | **`goal_gate.py --full --json`** | fail | **pass** | fail | pass | **pass** |

两次②的计数**逐项相同**（192 / 10 / 121 / 6 / 3 / 0 / 3 / 44 / 0 / 5）⇒ **本机协作视图的同步状态不是这些数字的差异源**；§3.4 的 5 张缺卡另有原因。

---

## 2. ①④ 的补全（`--full` **已跑完**）

- 命令：`python -I tools/goal/goal_gate.py --full --json > E:/tmp/goalsnap/full.json`（`--full` = `--tests --script`：`cargo test --workspace` + release 构建并运行 `mir2-script-tool parse-stats`）。
- 起止：2026-10-23T14:17Z 启动 → **2026-10-23T14:21:23Z 返回**（`generated_ts=1792765283`，同一 commit `de7cb7779c`；总耗时约 4 分钟）。
- **① 结果（pass）**：`parse-stats` rc=0 —— `implemented=106`（目标 106/106 去重派发目标）、`calls=11829`、`unimplemented=0`；文件 639/639 装载、缺失 0、异常 0。
  复算：`cargo run --release --locked -p mir2-script-tool -- parse-stats E:/MirServer/M2GameSvr/Envir --json <tmp> --require-implemented`。
- **④ 结果（pass）**：门禁 **7/7 绿**（fmt/clippy/msgcodegen/whitelist_anchor_gate/m3_whitelist_registry_audit/**cargo_test_workspace rc=0（224.0s）**/master_sync）。
- ⇒ **①③④ 现在都是 pass，条件② 是唯一未过项**（分诊见 §3）。

复核命令（任何人可原样复跑）：

```sh
python -I tools/goal/goal_gate.py --full --json | python -c "import json,sys;d=json.load(sys.stdin);print(d['overall'],[c['state'] for c in d['conditions']])"
```

---

## 3. 条件② 分诊（三个桶逐个给清单 + 定性 + 依据）

### 3.1 总账（截至 `de7cb7779c` @ 2026-10-23T14:17:16Z，192 张 R 卡）

| 桶 | 数 | 本文字段 |
|---|---:|---|
| 已完成（board） | 10 | 不计缺口 |
| `window_required` 登记 | 121 | 机器可读，等窗口 |
| whitelist `T-*` 登记 | 6 | 机器可读（**登记型 PASS ≠ 有真机证据**） |
| 已裁定登记 | 3 | `adjudicated` |
| comment 证据 | 0 | — |
| **三者皆无** | **3** | §3.2（= FAIL 卡） |
| **merged 未 closed 且无登记** | **44** | §3.3 |
| 未开工或未合并 | 0 | 不计缺口 |
| **其它需人工（board 未列出）** | **5** | §3.4 |

### 3.2 桶 A：三者皆无（3 张）——**真缺证据（需窗口）**，且缺机器可读登记

三张都有 `openmir2-worker-45` 的 triage comment（`docs/r-card-evidence-triage-20261023.md`），原文 `verdict`：
「真缺口（机判 FAIL 正确，非假阳性）」，`class`：「② 真缺证据（需窗口）」。

| 卡 | tier | 机判依据（thread 复核） | 定性 | 建议动作（worker-45 `suggested_registration`） |
|---|---|---|---|---|
| `openmir2-gm-batch-5-guild-castle` | R | 线程 8 条，无 patch/merge_result/review/window_required/T-* | **真欠窗口**（缺真机证据）+ 缺登记 | 发一条带 `"window_required": true` 的签名 status/comment（窗口判据写死） |
| `openmir2-monster-attack-cadence-lead` | R | 线程 4 条，同上 | 同上 | 同上 |
| `openmir2-oracle-humitems-align` | R | 线程 4 条；`suggested_registration` 明确「**不建议补 whitelist T-***：两侧语义已 1:1，没有可豁免的刻意差异」 | 同上 | 同上 |

> 复算：`walgit collab thread <卡>` 看最后一条 comment 的 `verdict` / `class` / `suggested_registration`。

### 3.3 桶 B：merged 未 closed 且无登记（44 张）

**分诊口径**：对 44 张逐张读线程，取「2026-10-23 记账清扫」（`openmir2-worker-43`，源表 `docs/r-card-ledger-sweep-20261023.md`，基线 `master 2ecd4b887`）写入的 `ledger_class`，并按本次快照复核 `merged` / `window_required`：

| 类 | 数 | 定性 |
|---|---:|---|
| **A** | **8** | **真欠窗口**：窗口项已写进 `status.work`（`work` 文本里有，但 approve 未带 `window_required`） |
| **B2** | **35** | **真欠窗口 + 只差登记**：approve 无 `window_required`、whitelist 无 `T-*`；窗口项只存在于 `status.work` |
| **B1** | **1** | **判据口径（记账债）**：卡面无待窗口项，`worker-43` 建议 owner 复核后 `closed`（未自行改状态） |
| 合计 | **44** | 已合并 **44/44**；**带 `window_required` 的 0 张**（本次快照实测） |

**抽样规则（可复算）**：44 张按 `last_status_ts` 升序取**最旧 10 张**，再加**5 张确定性伪随机**
（`sha1(card)[:8]` 升序、**排除已在前 10 的卡**）⇒ 15 张样本。

| # | 抽样卡 | ledger_class | last_status_ts |
|---|---|---|---|
| 1 | `openmir2-buff-producer-wiring` | B2 | 1792181652 |
| 2 | `openmir2-default-arm-spell-surface` | B2 | 1792183041 |
| 3 | `openmir2-snap-cell-fidelity` | B2 | 1792183820 |
| 4 | `openmir2-recall-slave-shift-live` | B2 | 1792203025 |
| 5 | `openmir2-map-random-move-shift-loop` | B2 | 1792209736 |
| 6 | `openmir2-enter-shift-loop` | B2 | 1792209745 |
| 7 | `openmir2-enter-map-cell-rejected-vs-oracle` | B2 | 1792224446 |
| 8 | `openmir2-magic-8-21-wiring` | B2 | 1792226043 |
| 9 | `openmir2-spell-mp-wrapper` | B2 | 1792239462 |
| 10 | `openmir2-aoi-verify-maptime` | B2 | 1792291008 |
| 11 | `openmir2-db-error-reply` | B2 | 1792346589 |
| 12 | `openmir2-run-gap-pr1-head-blocking` | B2 | 1792315070 |
| 13 | `openmir2-movetomovingobject-quirk` | B2 | 1792300298 |
| 14 | `openmir2-spell-shared-tail` | A | 1792714288 |
| 15 | `openmir2-mapcell-attributes-loading` | A | 1792714263 |

> 抽样结论：A/B2 都是「真欠窗口」（差别只在登记是否机器可读），B1 是「记账债、建议 closed」——
> 与全量分布（8 / 35 / 1）一致；样本里没有「未合并」或「有真差异」的卡。

**全量 44 张（`ledger_class` / `last_status_ts` / merged / 定性）**：

| # | 卡 | 类 | last_status_ts | merged | 定性 |
|---:|---|---|---:|---|---|
| 1 | `openmir2-buff-producer-wiring` | B2 | 1792181652 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 2 | `openmir2-default-arm-spell-surface` | B2 | 1792183041 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 3 | `openmir2-snap-cell-fidelity` | B2 | 1792183820 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 4 | `openmir2-recall-slave-shift-live` | B2 | 1792203025 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 5 | `openmir2-map-random-move-shift-loop` | B2 | 1792209736 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 6 | `openmir2-enter-shift-loop` | B2 | 1792209745 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 7 | `openmir2-enter-map-cell-rejected-vs-oracle` | B2 | 1792224446 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 8 | `openmir2-magic-8-21-wiring` | B2 | 1792226043 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 9 | `openmir2-spell-mp-wrapper` | B2 | 1792239462 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 10 | `openmir2-aoi-verify-maptime` | B2 | 1792291008 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 11 | `openmir2-player-searchviewrange-cadence` | B2 | 1792293898 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 12 | `openmir2-run-gap-player-run-quantification` | B2 | 1792294777 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 13 | `openmir2-run-gap-monster-state-expiry` | B2 | 1792295243 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 14 | `openmir2-run-gap-corpse-ghost-revive` | B2 | 1792297093 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 15 | `openmir2-movetomovingobject-quirk` | B2 | 1792300298 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 16 | `openmir2-run-gap-monster-master-follow` | B2 | 1792300306 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 17 | `openmir2-run-gap-checkdupobj` | B2 | 1792300796 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 18 | `openmir2-run-gap-monster-specials` | B2 | 1792309010 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 19 | `openmir2-disappear-recipients-gridscan` | B2 | 1792311538 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 20 | `openmir2-run-gap-pr1-head-blocking` | B2 | 1792315070 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 21 | `openmir2-run-gap-mb1` | B2 | 1792318300 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 22 | `openmir2-monster-view-throttle` | B2 | 1792320248 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 23 | `openmir2-persist-create-rows` | B2 | 1792327508 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 24 | `openmir2-persist-useitems-wiring` | B2 | 1792329879 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 25 | `openmir2-persist-item-attr-and-gold` | B2 | 1792331816 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 26 | `openmir2-run-gap-mb3` | B2 | 1792332963 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 27 | `openmir2-save-on-shutdown` | B2 | 1792336074 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 28 | `openmir2-rm-refmessage-delay-layer` | B2 | 1792337140 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 29 | `openmir2-active-seek-race-missing` | B2 | 1792340567 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 30 | `openmir2-proper-target-unfolded` | B2 | 1792340578 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 31 | `openmir2-search-target-clear-target` | B2 | 1792340591 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 32 | `openmir2-attacktarget-overrides` | B2 | 1792346571 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 33 | `openmir2-persist-single-sided-columns` | B2 | 1792346580 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 34 | `openmir2-db-error-reply` | B2 | 1792346589 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 35 | `openmir2-save-misc-gaps` | B2 | 1792346598 | 是 | 真欠窗口 + 缺机器可读登记（approve 无 window_required / 无 T-*） |
| 36 | `openmir2-gate-gmclose-race` | B1 | 1792499332 | 是 | 判据口径（卡面无可判窗口项，建议 closed） |
| 37 | `openmir2-mapcell-attributes-loading` | A | 1792714263 | 是 | 真欠窗口（窗口项已写进 status.work） |
| 38 | `openmir2-login-notice-clienttick` | A | 1792714285 | 是 | 真欠窗口（窗口项已写进 status.work） |
| 39 | `openmir2-spell-shared-tail` | A | 1792714288 | 是 | 真欠窗口（窗口项已写进 status.work） |
| 40 | `openmir2-sm-additem-tail` | A | 1792714406 | 是 | 真欠窗口（窗口项已写进 status.work） |
| 41 | `openmir2-sm-updateitem` | A | 1792714415 | 是 | 真欠窗口（窗口项已写进 status.work） |
| 42 | `openmir2-login-useitems-magic` | A | 1792714419 | 是 | 真欠窗口（窗口项已写进 status.work） |
| 43 | `openmir2-sm-merchantsay-recog` | A | 1792714423 | 是 | 真欠窗口（窗口项已写进 status.work） |
| 44 | `openmir2-smframe-minor-gaps` | A | 1792714431 | 是 | 真欠窗口（窗口项已写进 status.work） |

> **A 与 B2 的区别只影响机判可见性，不影响「欠不欠窗口」**：两类的窗口项都只在 `status.work` 里、
> 都没进 `review.window_required` / whitelist ⇒ goal_gate 只能看到「merged 未 closed 且无登记」。
> **补一条带 `window_required: true` 的签名 status 就能把 A/B2 一起变成机判可见的窗口欠账**（B1 除外——它建议直接 closed）。

### 3.4 桶 C：board 未列出（5 张）——**判据口径 / 索引视图问题**，不是缺证据

| 卡 | 线程可读 | 索引内证据（goal_gate 原文 hints） | 定性 |
|---|---|---|---|
| `openmir2-m3-monster-ai-tick` | 36 条 | merged=true、window_required=true | **真欠窗口**（被视图挡住） |
| `openmir2-client-e2e-enter-gaps` | 549 条 | merged=true；最后一次 merge 的 tier 是 **L** | **视图问题**，需按最新 L 复核是否可 closed |
| `openmir2-conformance-rng-nondeterminism` | 11 条 | merged=true（含 tier=R approve） | **真欠窗口**（被视图挡住） |
| `openmir2-menu-ok-unify-pickwire` | 10 条 | merged=true、window_required=true | **真欠窗口**（被视图挡住） |
| `openmir2-ybdeal-frames` | 6 条 | merged=true、window_required=true | **真欠窗口**（被视图挡住） |

**证据（本次复跑）**：

```sh
walgit collab board --format json   # goal_gate 的 board 源：6 列共 447 张卡，上述 5 个 id 一个都不在里面
walgit collab thread openmir2-ybdeal-frames   # 6 条可读（issue/patch/status/review/merge_result）
git cat-file -p refs/collab/meta/snapshot | head -c 120   # snapshot ts=1792066344（早于这 5 张的 last_status_ts）
```

⇒ **「board 未列出」是协作视图/索引口径问题**（thread 与索引都在），不是这 5 张缺证据。逐卡实测
（本次快照 `r_cards[].window_required` / `adjudicated` + 线程复核）：

| 卡 | 索引内 `window_required` | `adjudicated.kind` | 最后一次 merge 的 tier |
|---|---|---|---|
| `openmir2-m3-monster-ai-tick` | **true** | window_required | R |
| `openmir2-client-e2e-enter-gaps` | **true** | window_required | **L**（末次 merge；但历史多次 R+wr，需按最新一次判定复核是否可 closed） |
| `openmir2-conformance-rng-nondeterminism` | **false** | —（无） | R（review 有 tier=R，但没有任何 `window_required` 标注） |
| `openmir2-menu-ok-unify-pickwire` | **true** | window_required | R |
| `openmir2-ybdeal-frames` | **true** | window_required | R |

⇒ **5 张里 4 张有 `window_required=true`**（m3-monster-ai-tick / client-e2e-enter-gaps / menu-ok-unify-pickwire /
ybdeal-frames），**1 张（`conformance-rng-nondeterminism`）没有**、只有 tier=R 的 review。
> **与 review `bfc49ebf` 的对照（保留原错）**：review 说「4 张应为 **5** 张」并点名 `client-e2e-enter-gaps`——
> **它说对了一半**：第一版把例外卡错写成 `client-e2e-enter-gaps`（它其实**有** `window_required=true`），
> 真正的例外是 `conformance-rng-nondeterminism`；所以「有 wr 的张数」仍是 **4**（不是 5）。本版改成逐卡表，
> 计数与例外卡都以此表为准（任何人可按上表逐行复算）。
**修复不在本卡写集**：需要 coordinator 重放/重折快照并核对 `walgit collab board --format json` 的生成逻辑。

### 3.5 三类定性汇总（同一快照，桶间互斥，不重复计数）

| 定性 | 张数 | 来源桶 |
|---|---:|---|
| **真欠窗口（含当前不可跑，见下两行）** | 121 + 6 + 3 + 3 + 43 + 4 = **180** | window_required 121 / whitelist 6 / adjudicated 3 / 三者皆无 3 / 44 桶里 A8+B2 35=43 / board 缺卡 4 |
| ↳ 其中**当前不可跑**（§4：窗口跑也会失败，**已含在 180 内**） | **22** = R3 **13**（6 window_required + 7 merged_not_closed）+ castle_zone **4** + upgrade_retrieve **4**（均 window_required）+ equip_struck_save **1**（merged_not_closed） | 见 §4 逐条（本次快照逐卡复核：14 张落在 window_required 121 桶、8 张落在 merged_not_closed 43 桶） |
| ↳ **可跑欠窗口（排期按这个算）** | **158** = 180 − 22 | 窗口能跑、只是还没轮到 |
| ↳ 其中**只差登记**（真欠窗口但机判看不见） | **38**（B2 35 + 三者皆无 3） | 补 `window_required` 即可转成机判可见 |
| **判据口径 / 记账债** | **1**（B1：`openmir2-gate-gmclose-race`，建议 closed）+ **1**（`client-e2e-enter-gaps` 需按最新 L 复核） | 44 桶 B1 + 5 桶里的 L |
| **视图/索引口径** | **5**（整桶） | board 未列出 |
| 已完成 / 无缺口 | 10 | 已完成 |

> **180 / 22 / 158 的口径**：§4 说「不可跑 ≠ 欠窗口」指的是**排期口径**——这 22 张在机器分类上落在
> 欠窗口桶里（所以 **180 包含它们**），但它们的窗口现在跑也会失败，**排期不能按可跑窗口估** ⇒ **可跑 158**。
> 两者不是两个集合，是同一个 180 的两种切法（第一版 §3.5 只写了 180，没剔除这 22，属自相矛盾，已订正）。

> **不要把「登记型 PASS」当证据**：whitelist 6 + adjudicated 3 + `merged_open_hygiene` 124 都是
> 「有登记」而不是「有真机证据」（快照 detail 原文：`登记型 PASS ≠ 已有真机证据`）。

---

## 4. 今晚新增的「**不可跑**」（与「欠窗口」分开，否则误导排期）

| 条目 | 为什么不可跑（今晚证据） | 它卡住哪些 R 卡 | 这些卡当前 goal_gate 状态（本快照） | 解锁前置 | 谁能做 |
|---|---|---|---|---|---|
| 场景 `combat_strike` | **BLOCKED-FIXTURE**：全库无一对同时满足 `DC_min>2*AC-1` 与 `HitPoint≥SPEED-1`（最好 joint 24%）；且目标守卫挨打也产 `SM_STRUCK(31)`（`recog=self`）、DSL 无 recog 过滤 ⇒ 跑绿 ≠ 玩家命中。已由场景顶层 `blocked` 在工具层强制（`card_window` rc=2） | `docs/window-debt-reclass-20261023.md` §R3 的 **13 张** | 各自在 121/window_required 等桶里（场景不是 R 卡） | 新夹具（`DC_min≥36..60` 且 `HitPoint≥17..34` 的配装）**或** DSL 的 `wait_for` recog 过滤 | owner 裁决夹具 / 工具卡 |
| 场景 `castle_zone`（不可达登记） | 城堡战需 `UnderWar`：玩家申请链抛 `NotImplementedException`（T-38）、GM 开战命令需 Permission 10（Release）、开战仅墙钟 20 点、`AttackSabukWall.txt` 为空 | `openmir2-castle-guard-target`、`openmir2-castle-war-area-impl`、`openmir2-castle-merchant-fixedhide-hit`、`openmir2-castle-merchant-disappear-downlink`（**4 张**） | 4 张全部 `window_required=true`（状态 pass，但窗口跑不了） | 双行会 DB + 私有根 `AttackSabukWall.txt` + 20 点窗口（或 owner 授权改私有根 `StartCastlewarTime`）+ 多会话探针 | owner（内容/配置面 + 多会话工具） |
| 场景 `upgrade_retrieve` | 等 **Rust 接线**：`@upgradenow`/`@getbackupgnow` 的结果 say 未执行（T-145④ / T-95④），失败支 Rust 零帧 | `openmir2-upgrade-npc`、`openmir2-a09-upgrade-item`、`openmir2-weapon-upgrade-values`、`openmir2-outbound-encoders-red-batch`（**4 张**，全 window_required） | 4 张 `window_required=true` | Rust 把 `GotoLable(~@upgradenow_* / ~@getbackupgnow_*)` 说到宿主（`MerchantScriptBook` 已装载，只差调用） | 循环内（Rust 接线卡） |
| 场景 `equip_struck_save` | 等 **Rust 642 接线**：`AttackSuccess` 的 `DoDamageWeapon` 与 `RM_DURACHANGE` 未接（T-199 相邻登记）；受击方 `wear_durability` 无事件通道 | `openmir2-persist-useitems-wiring`（**1 张**；当前在 §3.3 的 44 桶里，`merged_not_closed`、`window_required=false`） | `need_human`（merged_not_closed） | Rust 接 642（武器/护甲两条路至少一条）+ 本卡补 `window_required` | 循环内（Rust 接线卡）+ 循环内（补登记） |

> **两类必须分开排期**：「欠窗口」= 窗口能跑、只是还没轮到（121+6+3+3+43+…）；
> 「不可跑」= 窗口**现在跑也会失败**（上表 4 类，合计卡住 13 + 4 + 4 + 1 张 R 卡的窗口批次）。
> 把后者塞进前者会让窗口批次的估算整体失真。

---

## 5. 下一步清单（按优先级）

| P | 动作 | 前置 | 谁能做 |
|---|---|---|---|
| **P0-1** | 跑一次完整的 `goal_gate.py --full`，把条件①（parse-stats）与④（`cargo test --workspace`）从 `need_human` 补成绿/红 | 独占构建/测试窗口（`--test-timeout 7200`）；干净树 | 循环内（任意 worker + 窗口 baton） |
| **P0-2** | 给 §3.2 的 3 张 FAIL 卡补 `window_required: true` 的签名 status（按 worker-45 的 `suggested_registration`） | 无 | 循环内 |
| **P0-3** | 给 §3.3 的 **35 张 B2** 补 `window_required` 机器可读登记（A 类 8 张同批补一次即可） | 无（窗口项已在 `status.work`，只需转录一次 status） | 循环内（记账清扫车道） |
| **P0-4** | `openmir2-gate-gmclose-race`（B1）按 `worker-43` 建议复核后 `closed`（或补一条可判窗口项） | owner/coordinator 复核 | owner |
| **P0-5** | §3.4 的 5 张「board 未列出」：重新折/发布协作快照并核对 `walgit collab board --format json`（snapshot ts=1792066344 明显早于这些卡） | 无 | coordinator |
| **P1-1** | **ident-14 `ignore` 的 `approved_by: PENDING`** 走复核（`tools/conformance/rules.json` 的 `rules[2]`）：批准后改成非 PENDING；注意规则原文已声明「城堡商人 `FixedHideMode` 解除的确定性 `SM_HIT` **不被本规则覆盖**」 | owner/coordinator 复核；窗口若真的剔了 14 而规则仍 PENDING ⇒ 报告 UNDECIDABLE | owner |
| **P1-2** | **`rules_pending_review` 清零**：`rules.json` 里 **38 条 ident 规则 + 4 条 global 规则**全部是 `approved_by: PENDING owner/coordinator review (draft: openmir2-worker-4)` ⇒ 任何窗口报告都会带 `rules.rules_pending_review=true`（`compare_runs.py:1403`） | 同 P1-1（一次评审批处理） | owner |
| **P1-3** | **私有栈窗口权限**：`card_window.py --profile alt` 需要的 `CONFORMANCE_PASSWORD`、`E:/tmp/conformance` 私栈、15xxx/17xxx 独占端口、两侧 `restore/start/stop` baton | 端口空闲 + 私栈目录可用 | owner 授权给某条窗口车道 |
| **P1-4** | **解锁「不可跑」**：Rust 接 `@upgradenow`/`@getbackupgnow` 说词（unblock upgrade_retrieve 4 张）+ Rust 接 642（unblock equip_struck_save 1 张） | 两张 Rust 接线卡排进循环；castle/combat 需 owner 裁决 | 循环内（Rust）+ owner（castle/combat 配置面） |
| **P2-1** | **R 卡登记清扫**：按本快照重算欠账口径（历史 173 已过期，见 §6），把 A/B2 合并成「补登记」一批做完 | P0-3 完成 | 循环内 |
| **P2-2** | `merged_open_hygiene` 124 张的清单复核（快照 evidence 已列）：区分「登记型 PASS」与「真机证据」，后者才进窗口批次 | 无 | 循环内 |

---

## 6. 数字口径与来源

| 量 | 值 | 来源 / 时点 |
|---|---:|---|
| 本次快照的 R 卡总数 | **192** | 本文件 §0，`de7cb7779c` @ 2026-10-23T14:17:16Z（复算命令见抬头） |
| 本次快照的 `board_cards` / `index_entries` | **446 / 7243** | 同一快照 `evidence.board_cards` / `evidence.index_entries` |
| `walgit collab board --format json` 的卡数 | **447**（6 列） | 本次另跑（`E:/tmp/goalsnap/board.json`）——与上行 446 差 1，**未查明原因**，不影响本卡结论 |
| `merged_open_hygiene` | **124** | 本次快照 `evidence.merged_open_hygiene`（卫生清单，登记型 PASS） |
| whitelist `T-*` 行数 | **208** | 本次快照 `evidence.whitelist_t_rows` |
| parse-stats 覆盖 | **implemented 106 / calls 11829 / unimplemented 0** | `--full` 快照 `conditions[0].evidence`（`generated_ts=1792765283`） |
| `cargo test --workspace` | **rc=0（224.0s）** | `--full` 快照 `conditions[3].evidence.checks[cargo_test_workspace]` |
| 历史欠账「173 张」 | 173 | **`docs/window-batch-plan-20261023.md` §1（`:9/:19/:58`）**，口径 = merged-not-closed 217 / 最新 approve tier=R 155 / 无 tier 18 ⇒ 173；**该文是早前时点，今晚合并后必然变化，不得当现量** |
| 历史量「178」 | **查不到来源** | 本卡未在仓库任何文档/快照里找到 178 的可核出处（找到的近邻：192（本次 R 卡）、173（上表）、124（merged_open_hygiene））。**不引用**；若 owner 记得出处请补 |
| 记账清扫分类总数（A 107 / B1 1 / B2 35 / U 1 / SKIP 3 / no-tier 16） | — | `docs/r-card-ledger-sweep-20261023.md`（基线 `master 2ecd4b887`，早于本次快照；本次只用到其中的 A/B1/B2 三类） |
| 城堡/守卫等事实 | — | 见 `docs/castle-zone-unreachable-20261023.md`（本日 doc-fixes 后） |

---

## 7. 未核 / 残留

1. **条件①③④ 已由 `--full` 判绿**（§2），但**条件② 仍 fail** ⇒ overall=fail；别把「①③④ 绿」读成终点达成。
2. **5 张 board 缺卡的原因未查明**（§3.4）：现象已复现（board JSON 447 张不含这 5 个 id、thread 可读、snapshot 更早），但 walgit board builder 的具体路径未读——归 coordinator 排查。
3. **178 的来源查不到**（§6）。
4. 本卡只做只读复算 + 文档：未改任何工具/代码/场景，未起栈、未连库；临时产物在 `E:/tmp/goalsnap/`（`fast.json` / `fast2.json` / `board.json` / `full.*`），交付后清理。

---

## 后续更新（2026-10-24 00:5x，`5e6b4e12`）

> **2026-10-24 再更新（卡 `openmir2-docs-state-drift-sweep`）**：§4「不可跑」与 :248/:268 的 `equip_struck_save` 行**已被 master `8dcb8c8ca` 取代**——642 攻击方武器磨损 + `SM_DURACHANGE(642)` 已接（T-266），`card_window --dry-run --scenario equip_struck_save.json` rc=0。本文件其余数字仍是带戳快照。

> 本节的数字**覆盖 §1 的旧快照**；引用时以本节为准。

- **条件② `fail` → `need_human`**：`三者皆无 0`、**FAIL 卡 0**。
- `merged 未 closed 且无登记` **44 → 8**（四批登记共 36 张：T-227..T-262），
  剩余 8 张 = 3 张显式顺延（真机不可达 1 / 多会话 2）+ 1 张无窗口判据 + 4 张未开班，
  由卡 `openmir2-r-window-registry-closeout` 三桶收口。
- `其它需人工 5`（线程可读但 `board --json` 不列出）由卡 `openmir2-board-index-reconcile` 处理。
- 唯一 FAIL 卡 `openmir2-gm-batch-5-guild-castle` 已按 A/B/C 处置：**A 片 6 条合入 `2810298e9`**、
  B 片由 `openmir2-gm-batch-5b-cross-server` 承接（**已合 `99a4d30c4`，保持 blocked-by G11**）、
  C 片部署 `GMEXECUTE` 0 命中登记不实现。
- **登记质量未审**：`window_required 122` 这个桶从未审计过「判据是否机器可判」——
  卡 `openmir2-window-criteria-quality-audit` 在办（30 张抽样、三档定性）；**2026-10-24 订正**：该卡已合入 master `5a6ed54c1`（返工 v2，纯 docs，tier=L），不再「在办」。

⇒ **条件② 仍差的两件事都在 owner 手里**：① 真机窗口授权（`docs/needs-human-20261023-night.md` §1）；
② `rules.json` 43 条 PENDING（其中 ident 14 需单独裁定，§2）。
