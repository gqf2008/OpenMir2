# goal_gate 四条件全量快照（master `81ecc0d28`，2026-10-24 07:41:49 +08）

> **适用范围**：本文件只对 `master = 81ecc0d28` @ `generated_ts = 1792798909`（2026-10-24 07:41:49 +08）有效；账本/白名单一变，数字即过期。
> **怎么跑的**：在 worktree `E:/Users/gxh/Documents/GitHub/OpenMir2-wt-cronclaim`（HEAD=`81ecc0d28`）逐字执行 `python -I -B tools/goal/goal_gate.py --full --json`；耗时 **2m10s**，stderr 为空，**退出码 rc=2**（= `need_human`，不是运行错误），`mode = {"tests": true, "script_coverage": true}`（含 `cargo test --workspace`，检查项里实测 109.5s）。
> **过程说明**：开跑时 master 是 `e4a872e43`；跑到一半 `batchplan-count-18` 合入（`81ecc0d28`，只改 `docs/window-batch-plan-20261023.md`），于是先把分支 fast-forward 到 `81ecc0d28` **重跑**。两次（`e4a872e43` @ 2026-10-24 07:39:08 +08、`81ecc0d28` @ 2026-10-24 07:41:49 +08）四条件 state 与条件② 计数**完全相同**——本文件引用后者。

## 0. 四条件结论

| # | 条件 | state | 一句话 |
|---:|---|---|---|
| 1 | 脚本派发 | **pass** | parse-stats 实测：目标 106/106（去重派发目标）；调用 11829 次；未实现 0 个 |
| 2 | R 卡证据 | **need_human** | R 卡 198 张；`window_required` 124（其中未合并 2）；需人工 10；FAIL 0 |
| 3 | B 类清零 | **pass** | B 类清零：recon tracked=0（rows 191 条非空、counts 一致）（recon 实测 tracked=0） |
| 4 | 门禁恒绿 | **pass** | 门禁 7 项：绿 7 / 红 0 / 需人工 0 |

`overall = need_human`（需人工）；`fail_items = 0`；`not_merged_items = 0`。

> 〔2026-10-24 订正：`goal_gate` 的 detail 措辞已由「其中」改为「另」；本处为当时原文〕

## 1. 四条件 detail 原文（逐字，未删节改写）

```
conditions[1].id = 1
conditions[1].state = pass
conditions[1].detail = parse-stats 实测：目标 106/106（去重派发目标）；调用 11829 次；未实现 0 个
conditions[2].id = 2
conditions[2].state = need_human
conditions[2].detail = R 卡 198 张：已完成 11 / window_required 124（其中未合并 2） / whitelist 48 / 已裁定登记 3 / comment 证据 0 / 三者皆无 0 / merged 未 closed 且无登记(需人工) 5 / 未开工或未合并 0（不计缺口）/ 其它需人工 5；需人工 10 张（按 last_status_ts 最旧优先，见 JSON need_items）；merged 未 closed 但已凭登记 PASS：173 张（卫生清单，见 evidence.merged_open_hygiene；登记型 PASS ≠ 已有真机证据）
conditions[3].id = 3
conditions[3].state = pass
conditions[3].detail = B 类清零：recon tracked=0（rows 191 条非空、counts 一致）（recon 实测 tracked=0）
conditions[4].id = 4
conditions[4].state = pass
conditions[4].detail = 门禁 7 项：绿 7 / 红 0 / 需人工 0
overall = need_human（需人工）
fail_items = []
```

条件② 附带的证据数字（`evidence`，非 detail 文本）：`board_cards=507`、`index_threads=518`、`index_entries=8101`、`whitelist_t_rows=255`、`whitelist_duplicates=[]`、`merged_open_hygiene=173` 张、`not_merged=0`。

## 2. 条件② 分类计数：与旧快照逐桶对比

旧快照 = `docs/needs-human-20261024.md` §1.5 在 `55a8e7fca` 的逐字记录（`generated_ts=1792791977`，2026-10-24 05:46:17 +08）。**口径说明**：本卡 issue 写的是「`47fd96ca8` 时点」，但仓库里留存的 goal_gate 记录只有 `55a8e7fca` 这一份（`47fd96ca8` 是它之后若干次合并出来的 master）；且条件② 的计数来自**当前账本**，用旧 commit 的代码今天重跑也复现不出旧计数，所以基线只能取这份**留档原文**。

| 桶（detail 原文用词） | `55a8e7fca` | `81ecc0d28` | Δ | 归因 |
|---|---:|---:|---:|---|
| R 卡总数 | 198 | 198 | 0 | — |
| 已完成 | 11 | 11 | 0 | — |
| window_required（含未合并） | 123 | 124 | +1 | `openmir2-weaponstrong-dura-zero-skip` 合入 `d54fa4581` 且 `window_required=true` |
| 其中未合并 | 2 | 2 | 0 | — |
| whitelist | 48 | 48 | 0 | — |
| 已裁定登记 | 3 | 3 | 0 | — |
| comment 证据 | 0 | 0 | 0 | — |
| 三者皆无 | 0 | 0 | 0 | — |
| merged 未 closed 且无登记(需人工) | 5 | 5 | 0 | — |
| 未开工或未合并（不计缺口） | 1 | 0 | −1 | 同一张 `weaponstrong-dura-zero-skip` 已合入，移出该桶 |
| 其它需人工 | 5 | 5 | 0 | 清单见 §3（`board 未列出` 5 张） |
| 需人工合计 | 10 | 10 | 0 | 计数相同；旧快照未留 10 张名单，集合是否逐张相同无法比对 |
| merged 未 closed 但已凭登记 PASS（卫生清单） | 172 | 173 | +1 | 同上，`weaponstrong-dura-zero-skip` 入卫生清单 |
| whitelist_t_rows | 255 | 255 | 0 | — |

⇒ 与旧快照相比：**四条件 state 未变**（条件② 仍 `need_human`，其余三条 `pass`），唯一变化是同一张卡从「未开工」走到「已合并 + `window_required` 登记」，因此 `window_required` +1、卫生清单 +1，缺口总数仍为 **0 FAIL**。

## 3. need_items（需人工 10 张，逐条照抄 JSON）

| 卡 | reason（原文） | last_status_ts |
|---|---|---:|
| `openmir2-m3-monster-ai-tick` | board 未列出该卡（本地协作视图未同步或线程状态异常），证据无法核对；索引内有 merge_result merged=true、索引内有 window_required=true | 1792131336 |
| `openmir2-client-e2e-enter-gaps` | board 未列出该卡（本地协作视图未同步或线程状态异常），证据无法核对；索引内有 merge_result merged=true、索引内有 window_required=true | 1792135704 |
| `openmir2-enter-map-cell-rejected-vs-oracle` | merge_result merged=true 但 board 状态=in-progress（非 closed）且无显式登记；需确认未重开新工作或无证据 | 1792224446 |
| `openmir2-run-gap-checkdupobj` | merge_result merged=true 但 board 状态=in-progress（非 closed）且无显式登记；需确认未重开新工作或无证据 | 1792300796 |
| `openmir2-disappear-recipients-gridscan` | merge_result merged=true 但 board 状态=in-progress（非 closed）且无显式登记；需确认未重开新工作或无证据 | 1792311538 |
| `openmir2-gate-gmclose-race` | merge_result merged=true 但 board 状态=in-progress（非 closed）且无显式登记；需确认未重开新工作或无证据 | 1792499332 |
| `openmir2-conformance-rng-nondeterminism` | board 未列出该卡（本地协作视图未同步或线程状态异常），证据无法核对；索引内有 merge_result merged=true | 1792703473 |
| `openmir2-menu-ok-unify-pickwire` | board 未列出该卡（本地协作视图未同步或线程状态异常），证据无法核对；索引内有 merge_result merged=true、索引内有 window_required=true | 1792714385 |
| `openmir2-ybdeal-frames` | board 未列出该卡（本地协作视图未同步或线程状态异常），证据无法核对；索引内有 merge_result merged=true、索引内有 window_required=true | 1792716819 |
| `openmir2-gm-batch-5b-cross-server` | merge_result merged=true 但 board 状态=blocked（非 closed）且无显式登记；需确认未重开新工作或无证据 | 1792773481 |

分两类：**board 未列出（5 张）** = `board_cards=507` 比 `index_threads=518` 少 11 条索引，协作视图里没有这些线程（本地 refs 未同步或线程状态异常）；**merged 但非 closed（5 张）** = 已 `merge_result{merged:true}`，board 仍 `in-progress`/`blocked` 且无显式登记。

## 4. 门禁 7 项（条件④ 明细，逐条照抄）

| 检查 | rc | detail |
|---|---:|---|
| `fmt` | 0 | fmt rc=0（2.6s） |
| `clippy` | 0 | clippy rc=0（2.1s） |
| `msgcodegen` | 0 | msgcodegen rc=0（0.3s） |
| `whitelist_anchor_gate` | 0 | whitelist_anchor_gate rc=0（1.5s） |
| `m3_whitelist_registry_audit` | 0 | m3_whitelist_registry_audit rc=0（0.1s） |
| `cargo_test_workspace` | 0 | cargo test --workspace rc=0（109.2s） |
| `master_sync` | —（比对项，无 rc） | 本地 master == walgit master（81ecc0d28ec7） |

`master_sync` 对的是本地 master 与 `walgit` master：运行时刻两者都是 `81ecc0d28`。

## 5. warnings（逐字，两条）

- whitelist.md 有 10 行列数≠4（格式异常，按已解析列处理）：T-23, T-69, T-71, T-157, T-158, T-159, T-172, T-201, T-206, T-263
- whitelist.md 有 108 行无法结构化关联卡号（不计入任何卡，需人工补『卡 `openmir2-…`』）：T-1, T-2, T-3, T-5, T-6, T-7, T-8, T-9, T-10, T-11, T-13, T-14, T-15, T-16, T-17, T-18, T-19, T-20, T-21, T-23, T-24, T-25, T-27, T-28, T-30, T-33, T-34, T-35, T-37, T-41, T-42, T-43, T-44, T-45, T-46, T-47, T-49, T-50, T-51, T-53, T-55, T-56, T-57, T-58, T-60, T-61, T-62, T-63, T-64, T-66, T-67, T-68, T-69, T-71, T-86, T-87, T-88, T-89, T-90, T-91, T-92, T-93, T-94, T-97, T-98, T-99, T-100, T-102, T-103, T-104, T-105, T-106, T-107, T-108, T-109, T-110, T-111, T-112, T-113, T-114, T-115, T-116, T-119, T-125, T-126, T-127, T-128, T-129, T-130, T-131, T-132, T-134, T-135, T-138, T-139, T-140, T-141, T-142, T-145, T-148, T-149, T-150, T-151, T-152, T-153, T-155, T-156, T-172

## 6. 复算命令

```sh
cd E:/Users/gxh/Documents/GitHub/OpenMir2-wt-cronclaim   # 或任意 master=81ecc0d28 的检出
git log -1 --format='%h %ci'                            # 应为 81ecc0d28
python -I -B tools/goal/goal_gate.py --full --json > /tmp/gate.json   # 约 2m10s，rc=2 = need_human
python -I -c "import json;d=json.load(open('/tmp/gate.json'));print(d['overall'],d['generated_ts']);[print(c['id'],c['state'],c['detail']) for c in d['conditions']]"
```

rc=2 只表示 `overall=need_human`；不是脚本报错（stderr 为空）。

## 7. 红项与承接建议（本卡不修）

- **FAIL 卡 0、未合并 0**：没有新红项。
- 条件② 的 **10 张需人工**是既有欠账：其中 `openmir2-run-gap-checkdupobj` / `openmir2-disappear-recipients-gridscan` / `openmir2-enter-map-cell-rejected-vs-oracle` 已作为「三张卡的替代判据」挂在 `docs/needs-human-20261024.md` §3，等 owner 一句话；另两张 `openmir2-gate-gmclose-race` / `openmir2-gm-batch-5b-cross-server` 需 Leader 补登记或裁决（本卡只列，不代裁）。
- **board 未列出的 5 张**（`board_cards 507 < index_threads 518`）建议 Leader 跑一次协作视图同步/索引核对，否则条件② 永远把这 5 张算「证据无法核对」。
- **whitelist 两条警告**（10 行列数≠4；108 行无法关联卡号）与 owner 清单里「`T-161..T-222` 复核口径」是同一件事，不属本卡；本卡只保证原文照抄。
- 本文件只写 `docs/`，未改 `src/**`、`tools/**`、`rules.json`、场景 JSON、`whitelist.md`、任何卡片状态。
