# 装置 sweep 分类（§5.1 四类兼容判据）——脚本与口径（2026-11-21）

> 状态：**已跑三次**。第三版（按 label 分审计文件）`rc=0`，全表通过；分类表见
> `docs/issues/device-sweep-classification-20261121-233522.md`。**前两份表（`…-232908.md` / `…-233311.md`）作废**。
> 目的：把 `docs/issues/device-per-recipient-model-design-20261121.md` §3 的"兼容判据"从纸面
> 落成可执行的分类表——对 60 支装置 casing 逐支给出类别，并**单独标出**属于
> 「跨 actor 且收件人本步已跑过」那一类的差异；该类之外的变化一律按"疑模型"报红。

## 脚本位置与用法

- 脚本：**`tools/parity/sweep_classify.py`（已入仓并登记进工具门**，`--selftest` 3/3：
  `INSTRUMENT_RED` 两态 + 有 cross 证据时 `rc=0`）。
  自检与两次仪器坑的说明见本卡「仪器坑与自检」节。
- 用法（判 rc 不接管道）：
  ```
  python -I -B tools/parity/sweep_classify.py \
    --before <旧装置输出目录> --after <新装置输出目录> \
    --audit <新装置 frames-audit.jsonl 目录> \
    --out E:/UserFleet/qa/device-sweep-classification-<时间戳>.md
  echo RC=$?
  ```
- 输出/审计文件命名约定：`<stem>.json`（装置 `--out`）与 **`<stem>.<label>.audit.jsonl`**
  （按 case label 分文件；旧单文件 `<stem>.audit.jsonl` 仍兼容，但会互相覆盖——见「仪器坑与自检」坑 2）
  （装置 `MIR2_FRAMES_AUDIT`）。

## 分类口径（与设计 §3 一一对应）

| 类别 | 判据 | 处置 |
| --- | --- | --- |
| `ok` | 两侧 `Sent` 序列（含 `due_ms_rel`）与 `Rm` 序列逐项相同 | 通过 |
| `仅顺序` | 多重集相同、仅序列顺序不同 | 通过（但需在报告里列出，扩面时说明帧序是否被判） |
| `仅 due` | 除 `due_ms_rel` 外逐字段相同 | **只有** 带 `cross_actor_deferred=True` 才通过 |
| `内容` | 其它任何差异（含收件人集合不同） | **疑模型**，报红 |
| `无台账` | 某侧缺文件 / JSON 解析失败 | 报红（不得静默"未跑"） |

- `cross_actor_deferred` 的判定：对每个差异帧 `(who, ident, recog)`，在审计侧车里找同帧行且
  `enqueue_step < run_step`（即该 RM 在收件人本步 Run 之后入队、下一步才 Operate）。
- 退出码：全部 `ok/仅顺序` 且 `仅 due` 全部带 `cross_actor_deferred` ⇒ 0；否则 2。

## 已知覆盖缺口（与 R3 同源）

- 若 60 支里**没有任何** `enqueue_step < run_step`，则"仅 due"这一类在该批不可覆盖（bigcloak 三支已属此情形）。
  届时报告必须明写「当前 60 支无跨 step 入队样本，R3/仅 due 不可覆盖」，并进扩面验收缺口；
  **不得**把"没有样本"写成"通过"。

## 仪器坑与自检（2026-11-21，两次都朝错误方向推过结论）

### 坑 1：`record struct` 装箱做键 ⇒ 引用比较恒 miss

- **现象**：`SendMessage` 是 `record struct`；用 `object` + `ReferenceEqualityComparer` 记录"首见步"时，
  `PeekAll()` 与消费端各得一个新装箱 ⇒ 查表永远 miss ⇒ `enqueue_step` 恒回落到当前步。
- **错误结论**：641 行审计全 `enqueue_step == run_step`，据此差点把"per-recipient 机制不成立"写成事实，
  并把 R3 判成"60 支不可覆盖"。
- **修法**：改**值键**（ident/wParam/n1/n2/n3/delivery/late）；QueueTrace 再叠加队列属主。
- **现有自检**：`sweep_classify.py` 的 `INSTRUMENT_RED`——`audit_total > 100 且 enqueue<run 行数 == 0` ⇒ rc=2；
  `audit_total == 0 且 stems > 0` 也 ⇒ rc=2。

### 坑 2：每个 case 覆盖写同一个审计文件

- **现象**：`--case <stem>` 一支会跑 s1/s5/s13，审计写同一路径 ⇒ 只剩最后一支（s13）的行。
- **错误结论**：`mabmabe` 的 `31/656` 差异（发生在 s1/s5）看起来像"没有审计行"，又差点被判成内容差异；
  `CROSS_ROWS=61/CROSS_STEMS=28` 也是残缺值。
- **修法**：`Flush(label)` 按 `<stem>.<label>.audit.jsonl` 分文件；分类器按 label 汇总（并保留旧单文件兼容）。
- **现有自检**：逐帧覆盖断言 `AssertCoverage`（按 buffer 下标集合，任何帧无审计行 ⇒ rc=2 点名），
  加上 `INSTRUMENT_RED` 的零行判定。

### 改前 / 改后两次 sweep 对照（同一 `before/`，同一命令）

| 指标 | 改前（单文件覆盖） | 改后（按 label 分文件） | 变化 |
| --- | ---: | ---: | --- |
| `AUDIT_ROWS` | 641 | **1916** | +1275 |
| `CROSS_ROWS` | 61 | **177** | +116 |
| `CROSS_STEMS` | 28 | **31** | +3 |
| `仅 due` 且 `cross=False` | 1（`mabmabe`） | **0** | −1 |

**命令行（原文）与 rc**
```
python -I -B tools/parity/sweep_classify.py --before E:/UserTemp/sweep/before --after E:/UserTemp/sweep/after3 \
  --audit E:/UserTemp/sweep/audit4 --out E:/UserFleet/qa/device-sweep-classification-20261121-233522.md
RC=0
```
分类结果：`ok 21 / 仅 due 22（全部 cross=True）/ 仅顺序 0 / 内容 0 / 仅窗口尾 1（firecharm）/ 无台账 1（warrskill 装置 rc=2）`。
完整表见同目录 `device-sweep-classification-20261121-233522.md`。

> **旧表作废**：`device-sweep-classification-20261121-232908.md`（首跑，641/61）与 `…-233311.md`（第二跑）
> 均建立在残缺仪器上，**不作为结论**。
