# `window_required_unmerged` 的仓内 selftest 与 hygiene 口径（`openmir2-goal-gate-unmerged-selftest`）

- 卡：`openmir2-goal-gate-unmerged-selftest`（tier=L）；分支 `card/openmir2-goal-gate-unmerged-selftest`；写集 `tools/goal/goal_gate.py`、`docs/`。
- 基线：`walgit/master = a30a6fa79`（开工时）。被收口的卡：`openmir2-goal-gate-unmerged-window-bucket`（已合 `19009c6a0`）。
- 起因（review `7e8eb6e3`，reviewer-10）：新分类 `window_required_unmerged` **仓内零覆盖**——把条件改成恒真/恒假时 `selftest` 仍 68/68 绿；另 `hygiene_categories` 未纳入新分类。

## 1. 两条仓内合成用例（断言到**分类名**）

在 `_synth_index()` / `_synth_board()` 里加两张卡（其余 17 张不变）：

| 合成卡 | 线程 | 期望 |
|---|---|---|
| `openmir2-card-pi` | `issue(tier=R)` + `review{window_required:true}`，**无 `merge_result`** | `r_category` = **`window_required_unmerged`**，`classify_r_evidence` = **PASS**（不新增 FAIL） |
| `openmir2-card-rho` | 同上 + `merge_result{merged:true}` | `r_category` 回到 **`window_required`**，仍 PASS |

新增断言（`selftest` 输出里的名字即判据）：

1. `未合并窗口桶① window_required + 无 merge_result -> window_required_unmerged（仍 PASS）`
2. `未合并窗口桶② 同卡补 merge_result -> 回到 window_required`

同时把「识别 N 张 tier=R」的期望从 17 改为 **19**（新增两张卡都计入）。

**断言数**：`python -I -B tools/goal/goal_gate.py selftest` ⇒ **70 条断言，70 通过**（改前 68；净增 2，符合「≥ 68 + 新增条数」）。

> 注意 CLI 口径：本脚本自测是**位置参数** `selftest`（不是 `--selftest`）；`--json` 等是 `run` 的选项。

## 2. 反向变异（两条都必须变红并点名）

| 变异（`r_category` 的未合并条件） | 结果 |
|---|---|
| 改成**恒真**（总是 `window_required_unmerged`） | `selftest` **rc=1**，点名 `FAIL 未合并窗口桶② … -> 有 window_required 登记但**尚未合并**…` |
| 改成**恒假**（总是 `window_required`） | `selftest` **rc=1**，点名 `FAIL 未合并窗口桶① … -> 有 window_required=true 登记` |

还原一律 `git checkout -- tools/goal/goal_gate.py`，还原后 `git status` 干净且
`sha256(file) == sha256(git show HEAD:tools/goal/goal_gate.py)`（实测两者均 `True`）。

## 3. `hygiene_categories`：选择「**纳入**」

```python
hygiene_categories = {"window_required", "window_required_unmerged", "whitelist_t",
                      "adjudicated", "comment_evidence"}
```

理由（写进代码注释）：卫生清单的门是 `rec["merged"]`，而 `window_required_unmerged` **定义上就是
`merged=false`** ⇒ 纳入**不产生任何条目**、行为零变化；收益是「登记类分类集合」只有一处枚举，
下次新增分类不容易漏（这次的 nit 正是漏在枚举上）。

## 4. 真实数据不漂移（交付 head vs `19009c6a0`）

同一份仓库状态上分别跑**旧脚本**（`git show 19009c6a0:tools/goal/goal_gate.py`）与新脚本：

```sh
python -I -B tools/goal/goal_gate.py --repo . --json --skip-tests --skip-script
```

条件② detail **逐字节相同**（`overall`、`state`、`rc` 亦相同）：

```
R 卡 194 张：已完成 11 / window_required 116（其中未合并 5） / whitelist 48 / 已裁定登记 3 /
comment 证据 0 / 三者皆无 0 / merged 未 closed 且无登记(需人工) 5 / 未开工或未合并 0（不计缺口）/
其它需人工 6；需人工 11 张 …；merged 未 closed 但已凭登记 PASS：165 张
```

> 〔2026-10-24 订正：`goal_gate` 的 detail 措辞已由「其中」改为「另」；本文件内该串引用均为当时原文〕

逐类：`R 卡 194`、`closed 11`、`window_required 116`（含 `window_required_unmerged 5`）、`whitelist_t 48`、
`adjudicated 3`、`comment_evidence 0`、`none 0`、`merged_not_closed 5`、`not_merged 0`、其它需人工 6、
卫生清单 165——**全部 0 差异**；`overall = need_human`、`rc = 2`（两版一致）。

## 5. 门禁（本分支）

| 门 | 结果 |
|---|---|
| `python -I -B tools/goal/goal_gate.py selftest` | **70/70 绿** |
| `python -I -B tools/whitelist/t_reservation_gate.py check` | **GREEN**（rows=250 / whitelist=250 / max=T-264 / fence_ok=True） |
| `python -I -B tools/hygiene/conflict_marker_gate.py` | **OK**（0 markers / 7431 files） |
| `git merge-tree --write-tree walgit/master HEAD` | **rc=0**（无冲突） |

## 6. 残留

- 只改 `tools/goal/goal_gate.py` 与本文；未改 `whitelist.md`、任何卡状态、`card_window.py`、`window_criteria_lint.py`。
- 条件② 的**汇总文案**仍是「window_required 116（其中未合并 5）」——这是 `openmir2-goal-gate-unmerged-window-bucket` 定的报告口径，本卡不重排。
- 无临时产物；变异/还原证据见 §2（sha256 校验）。
