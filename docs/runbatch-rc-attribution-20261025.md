# run_batch 外层 rc 归因（`outer_rc` 的来由、缺口与修复）

- 卡：`openmir2-runbatch-rc-attribution`（tier=L，owner `openmir2-tester-1`）。
- 日期：2026-10-25。写集：`tools/conformance/card_window.py`（仅退出码）、
  `tools/window/run_batch.sh`、`tools/window/rc_attribution.py`（新增）、本文件。

## 1. 现象

W1 批次（`E:/tmp/window/w1c`，head `4b4405bf5`）21 行 `batch.log` 的
`outer_rc` **全部是 2**，包括两条 PASS：

```
RESULT n=6  name=gm_time    verdict=PASS     outer_rc=2 compare_rc=0 exit_code=0
RESULT n=16 name=spell_cast verdict=PASS     outer_rc=2 compare_rc=0 exit_code=0
```

同一份 `batch.jsonl` 里 `outer_rc` 与 `duration_s` 却恒为 `null`。

## 2. 探针（`E:/tmp/window/probe-rc`）

同一命令（`gm_time`，profile `alt`）跑两遍：直接执行、以及 `timeout -k 60 2700` 包裹：

```
DIRECT_RC=2    TIMEOUT_RC=2
两次日志末尾都是：VERDICT: PASS (compare_rc=0, exit=0)
```

⇒ 与 `timeout`/命令替换无关。

## 3. 根因（falsy-zero）

`card_window.py` 旧的退出路径：

```python
return int(summary.get("exit_code") or 2)
```

PASS 时 `exit_code == 0`，而 Python 里 `0 or 2` 求值为 **2** ⇒ **每个非 DRY-RUN 的真实
运行都以 rc=2 退出**（PASS/FAIL 一样）。DRY-RUN 分支用的是 `or 0`，所以只有 dry-run 返回 0
——这正是先前「dry-run 正常、真跑恒 2」的原因。

**影响**：`outer_rc` 列**不含任何 verdict 信息**；`run_batch` 的总 rc 由
`card-window.json` 的 verdict 聚合，结论不受影响，但任何依赖子进程 rc 的消费方都会误判。

## 4. 修复

### 4.1 `card_window.py`

```python
def main_exit_code(summary):
    code = summary.get("exit_code")
    # `None` 与 `bool` 都 fail-closed 到 2：`int(False)==0` 会假绿（review `ee0122a8` F1）
    if code is None or isinstance(code, bool):
        return 2
    return int(code)
```

`main()` 的非 DRY-RUN 分支改用它（DRY-RUN 分支保持 `or 0`）。自测牙：PASS(0)⇒0、
FAIL(2)⇒2、缺失/None⇒2、**bool（True/False）⇒2**（`False` 被 `int()` 会变 0，review F1）；
「不得再有 `or 2`」用 **AST 扫描**（`_falsy_or_exit_code_nodes`）而非文本匹配——两条对照牙：
注释里写旧写法**不红**、条件分支里保留 `or 2` 的换形**必须红**（review F2）。

另加一颗**存档落定 SQL 防回归牙** `RC 牙2`：`_save_settle_sql` 必须用
`characters.LASTUPDATE` + `characters_ablity.ModifyTime`、不得出现 `characters.ModifyTime`
（旧版 `749b20f9f` 曾用不存在的列；`7e2d04b6c` 已修，牙只防回归）。

### 4.2 `run_batch.sh` A（列真值）

- 传给 JSON 构建器的 `skipped` 曾是字符串 `"0"`，Python 里 `"0"` 为真 ⇒ `outer_rc`/`duration_s`
  恒 `null`。现改为 `skipped_i = int(skipped)` 再参与判断。
- `batch.jsonl` 新增 `verdict_note` 字段。
- 汇总改由 `rc_attribution.totals_for()` 统一分桶（`PASS(...)`⇒PASS、`-`/None⇒UNDECIDABLE、
  其余未知串⇒**OTHER**）；`TIMEOUT`/`KILLED` 独立成桶（review F3：此前会漏计，合成 6 行只计 4）。
  **守卫是真检测器而非恒真断言**（review `33309b67` F1）：`OTHER > 0` 时打 `RED` 并以 3 退出，
  `batch-summary.json` 的 `totals_consistent = (OTHER==0 且 rows==sum)`。
  语义：**`None`（缺 verdict 键）与未知字符串都算 OTHER**，`-` 才是「无自证」的合法值。
  牙：`RCA 牙6`（已知 verdict 分桶自洽、OTHER=0）、`RCA 牙7`（未知串 `WAT/PASSX` ⇒ OTHER=2）、
  `RCA 牙8/8b`（**驱动 run_batch.sh 里真实的 summary-builder heredoc**：`verdict=BOGUS` ⇒
  rc=3 且不写 batch-summary.json；合法状态 ⇒ rc=0、`totals_consistent=true`、TIMEOUT 成桶）。

### 4.3 `run_batch.sh` B（归因规则，`tools/window/rc_attribution.py`）

| 条件 | 结果 |
|---|---|
| `outer_rc == 124` | `TIMEOUT`（不论自证） |
| `outer_rc == 137` | `KILLED`（不论自证） |
| `verdict == PASS` 且 `outer_rc ∉ {0,2}` | 自证 `exit=0 且 compare_rc=0` ⇒ `PASS(outer_rc_exception)` + note；否则 `UNDECIDABLE` |
| `verdict == PASS` 且 `outer_rc == 2` | 同上（**防御性**规则：来由是本文件的 falsy-zero bug，不是常态） |
| 其它 | 原样保持 |

`RESULT` 行增加 `verdict_note=…`；`TIMEOUT`/`KILLED`/`UNDECIDABLE`/`-` 都会让 run_batch 返回 2；
`PASS(outer_rc_exception)` 计入 `PASS`（但带 note 留痕）。

自测：`python -I -B tools/window/rc_attribution.py --selftest` ⇒ 7/7（`RCA1/2`、`牙1..5`）。

## 5. 对已产出批次的影响

- `E:/tmp/window/w1c/batch.jsonl` 的 `outer_rc`/`duration_s` 列**不可用**（恒 null）；
  以 `batch.log` 的文本行为准，且那里 `outer_rc` 全为 2 是 falsy-zero 产物。
- 帧面/DB 面的判读不受此修复影响（verdict 来自 `card-window.json`）。
- 其它历史批次同理：修复前的 `outer_rc` 一律不可作为「外层失败」证据。

## 6. 未做 / 边界

- 本卡**不改判据**（不掩码任何 diff）；`PASS(outer_rc_exception)` 只是给例外留痕。
- 真机自证与 DB 面重跑排在 harness 卡之后（先修 rc，再让证据里的 rc 字段有意义）。
- 复审更正（留档）：我起初在 `749b20f9f` 上读到 `characters.ModifyTime` 错列，该问题已在
  `7e2d04b6c`（review `b48197f6` F1）修复；本卡只加防回归牙，未改该 SQL。
