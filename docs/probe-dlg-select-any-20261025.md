# probe DSL：`dlg_select_any` + `repeat`（随机分支场景确定性化）— 2026-10-25

- 卡：`openmir2-probe-conditional-dlg-select`（tier **L**；owner `openmir2-dev-1`；
  worktree `.worktrees/dev-1`；分支 `card/probe-conditional-dlg-select`）。
- 写集：`tools/probe/mir2_probe.py`、`tools/probe/scenarios/clearmapmon_t118.json`、本文件。
- 基线：本地 `master`（见 patch body 的 base）。
- 红线：不改生产代码 / `card_window.py`；未知步型仍 fail-closed；既有场景既有步骤语义不变。

## 0. 一句话

`武士-0114` 的 `[@rw01]` 用 `#act movr M3 3` 随机 0..2，只有赢页有 `@rw02` 链接（输页只有 `/@exit`、
平局页只有 `再来/@rw01`）。本片给探针加两个 DSL 步型——`dlg_select_any`（按候选顺序选**当前页
实际存在**的链接，全不中即显式失败并打印链接清单）与 `repeat`（有界重试，`stop_on_select` 命中
即终止）——把该场景从「靠同一 seed 赌赢页」改成**确定性重试 6 轮**，并把 `clearmapmon_t118`
改用它。

## 1. 语义

### 1.1 `dlg_select_any`

```json
{"dlg_select_any": ["@rw02", "@rw01"], "note": "…"}
```

- 展开：`CM_MERCHANTDLGSELECT`（C2S）；`recog` 由运行期从最近一条 `SM_MERCHANTSAY(643).recog`
  推断（与 `dlg_select` 同口径；没有 643 ⇒ fail-closed）。
- 运行期：取**当前页**（最近一条 643/767 的文本）解析 `<文字/@label>` 链接，按**候选顺序**取第一个
  存在的（大小写不敏感）；发送该 label。
- **失败模式（fail-closed，均 rc=2）**：
  1. 还没见过任何页 ⇒ `none of [...] is a link on the current page (page ident=None …); page links=[]`；
  2. 候选一个都不在页上（如输页只有 `/@exit`）⇒ 同上并把**整页链接清单与页文本**打进错误
     （`page links=['@exit']; page text='…'`）——不静默跳过、不随便发一个；
  3. 没有 643 ⇒ `no SM_MERCHANTSAY(643) seen yet; cannot infer NPC recog`。
- 形状校验（加载期，`ScenarioError` ⇒ rc=2）：必须是非空数组；每项是 `@` 开头的非空字符串；
  **不允许重复**（有序候选重复无意义）；未知键/非数组/repeat 混用都拒。

### 1.2 `repeat`

```json
{"repeat": {"times": 6, "stop_on_select": "@rw02", "steps": [ … ]}, "note": "…"}
```

- `times`：1..100；`steps`：非空步骤数组（递归展开，嵌套 ≤3 层）；`stop_on_select`：可选字符串。
- 运行期：最多 `times` 轮，每轮**顺序执行** `steps`；轮末若最近一次 `dlg_select_any` 选中的标签
  等于 `stop_on_select` ⇒ 立即停止并把 `attempts`（每轮 links/selected）记入 run.json。
- **失败模式**：`times` 轮仍未命中 ⇒ `StepFailure`：
  `stop_on_select '@rw02' not reached in 6 attempt(s); per-attempt pages (links/selected)=[…]`
  （打印每轮页面，便于区分「seed 未命中」与「场景写错」）。

### 1.3 三张表（acceptance ②：缺一张就会被拒或静默不发）

| 表 | 位置 | 本次改动 |
|---|---|---|
| **fail-closed schema** | `ACTION_KEYS` + `plan_step()`（`dlg_select_any`/`repeat` 分支） | 两者加入 `ACTION_KEYS`；分支内做上述形状/范围/深度校验 |
| **展开/解析** | `expand_scenario()` → `plan_step()`；`_iter_plan_leaves()`；`_var_refs()` | `repeat` 内层递归展开（索引 `0.0`/`0.1`…）；语义校验（R1/R2/R3/R6）改看**叶子**；变量引用递归收集 |
| **SEND_KINDS** | `SEND_KINDS` | 加入 `dlg_select_any`（`repeat` 是组合步，不计入；其内层 send 由叶子展开覆盖） |

## 2. `clearmapmon_t118` 改造（acceptance ③）

- 旧：`dlg_select @st` → `wait 643` → `dlg_select @rw02`（平局/输分支直接超时/错页）。
- 新（8 步）：`click_npc` → `wait 643` → `dlg_select @rw01` → `wait 643` →
  **`repeat(times=6, stop_on_select=@rw02, steps=[dlg_select @st, wait 643, dlg_select_any [@rw02,@rw01], wait 643])`**
  → `dlg_select @rw03`（终止态：只有赢页 `@rw02` 页才有 `@rw03`）→ `wait 643` → `wait 30`。
- 分支语义：赢 ⇒ `dlg_select_any` 选中 `@rw02`（终止）；平 ⇒ 选中 `@rw01` 回 `[@rw01]` 页再掷；
  输 ⇒ 页上只有 `/@exit`，两候选皆无 ⇒ **显式失败**并打印链接清单（不掩盖）。
- `--check-scenario`：`scenario OK: clearmapmon_t118 (8 steps, params=npc_actor=1)` rc=0。
  场景仍 `blocked`（C# 侧 RNG seed/hook 未接线，见卡 `openmir2-window-csharp-rng`）——本卡只改 DSL
  与场景结构，不解除封存条件。

## 3. 用例与证据（acceptance ②/④/⑤）

- `mir2_probe.py --selftest`：**OK (15 checks)**，其中新增
  `[ok] dsl-dlg-select-any  parse/choose 7 项；形状 10 坏例；repeat 叶子 0.0-0.3；
   语义正/反；运行期 选中/清单打印/终止/超限 4 例`
  - 纯函数：`parse_page_links`（出现序去重）、`choose_link`（候选优先/大小写不敏感/全不中 None）；
  - 形状 10 坏例：空/非数组/非 `@`/重复候选；times 0/101；steps 空；`stop_on_select` 非串；
    `repeat` 未知键；嵌套 4 层 ⇒ 全部 `ScenarioError`；
  - 语义：合成场景（repeat 内 send/select/wait）`check_scenario` 通过；「wait 先于 send」被 R2 拒；
  - **运行期（合成帧、假 socket，不连网）**：正例选中 `@rw02` 且线上 body 含其编码；负例（只有
    `/@exit`）抛 `StepFailure` 且错误里带链接清单；`repeat` 首轮命中即停（attempts=1）；两轮平局
    未命中 ⇒ `not reached … attempt(s)` 并带每轮 links/selected。
- 场景全量：`tools/probe/scenarios/*.json` 全部 `--check-scenario` rc=0（见 patch body 的条数）。
- 牙（实跑，变异→红→还原）：见 §5。

## 4. 已知边界

- 「当前页」= 最近一条 643/767 帧解析出的链接。探针不建页面栈/历史；若一次 pump 已经读到下一页，
  判定就基于**最新**那页（真实服务端只在收到选择后才发下一页，正常流程不会发生）。
- 输分支（只有 `/@exit`）在本场景里**设计为失败**：它意味着 `movr M3` 抽到 0，需要重新准备夹具
  （`blocked.missing_fixture` 的操作约束）；`dlg_select_any` 不会替你选 `/@exit` 蒙混过去。
- `repeat` 不设总超时（每步用自己的 `timeout_ms`，默认 `--timeout-ms`）；`times×steps` 建议 <50。

## 5. 牙（实测：变异 → 目标断言红 → 还原绿）

| # | 变异 | 预期红 |
|---:|---|---|
| 1 | `choose_link` 改为「按链接顺序」而非候选顺序 | selftest `choose_link(["@rw01","@rw02"], links)` 断言红 |
| 2 | `choose_link` 全不中时返回第一个候选（fail-open） | 运行期负例（`none of …`）红 |
| 3 | `repeat` 忽略 `stop_on_select`（跑满 times） | `repeat` 首轮终止用例红（attempts/stopped） |
| 4 | `dlg_select_any` 去掉重复候选校验 | 形状坏例（duplicate）红 |

> 结果值与门禁见 patch body（`--selftest` 输出、`--check-scenario` 全量、diff-range、merge-tree）。
