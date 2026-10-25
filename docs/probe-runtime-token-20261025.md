# probe 运行期参数令牌：`@last_kill_xy`（卡 `openmir2-probe-runtime-param-last-kill-xy`，tier L）

- 卡：`openmir2-probe-runtime-param-last-kill-xy`（issue `093f6561`；owner `openmir2-dev-1`；
  branch `card/probe-runtime-param-last-kill-xy`；base = 本 patch 的 `--base`）。
  写集：`tools/probe/mir2_probe.py`、本文件。
- 只改探针 DSL 的取值解析（工具面）：**不改协议/服务端**，确定性来自「用已收到的帧」，不靠等待或重试。

> **取数时点**：2026-10-25（本卡 head 与 `mir2_probe --selftest` 的输出；场景清单取自 `tools/probe/scenarios/*.json` 当次运行；无实时数值依赖）。

## 1. 为什么需要

`CM_BUTCH(1007)` 的 `recog = MakeLong(尸体X, 尸体Y)`（`PlayObject.Message.cs:829-831`）必须是**尸体所在格**。
固定格 `kill` 时尸体就在 `params.target_xy`；但**寻靶（`track`）击杀**后尸体不一定还在那个格子
（鸡会走位），而场景里 `send.recog` 只能写**静态** `params`（`resolve_param` 只查 `params`/字面量）
⇒ 写死旧格必错。**数据其实已经在运行期存在**：死亡帧 `SM_NOWDEATH(34)`/`SM_DEATH(32)` 的
`param/tag` 就是目标格，kill 成功时也把 `target_xy` 记进 `run.steps`——缺的只是把它接进下一步。

## 2. 令牌清单

| 令牌 | 取值 | 何时可引用 |
|---|---|---|
| `@last_kill_xy` | 最近一次 `kill` 成功（`result=dead`）的**尸体格** `MakeLong(X,Y)` | 该 `kill` 步执行成功之后；否则报错 |

- **只在 `send.recog` 上解析**（运行期）。其它动作（`dlg_select` / `click_npc` / `wait_for` 的
  `recog` 等）仍走加载期 `resolve_param`，写 `@...` 会被当成无法解析的名字报 `scenario-error`——
  这是刻意的：**没有接线的地方不要假装支持**。将来要扩，按同样方式加。
- 场景里写字符串即可：`{"send": {"ident": 1007, "recog": "@last_kill_xy"}}`。
- 典型用法（butch 链）：

  ```json
  [{"kill": {"track": "nearest:鸡", "max_retries": 3, "interval_ms": 1000}},
   {"send": {"ident": 1007, "recog": "@last_kill_xy"}}]
  ```

## 3. 取值来源与一致性检查（不猜）

`kill` 成功时写 `state["last_kill_xy"]`，**写之前**做一致性检查（`_write_last_kill_xy`）：

1. 先把**匹配到的那帧死亡帧**的 `param/tag` 折成格子，与击杀步的目标格比较；
2. 相等 ⇒ 采纳（两种 kill 模式的常态）；
3. 不相等（例如匹配到的是 `SM_DEATH(32)` 的可见性刷新、不带坐标）⇒ 在**本次 kill 窗口内**的死亡帧里
   找一帧 `param/tag == 目标格`；找到就用；
4. 两处都没有 ⇒ **fail-closed**：`StepFailure`，打印「matched 帧的 ident/param/tag」「步目标格」
   「窗口内全部死亡帧的 (seq, ident, param, tag)」，**绝不写一个猜出来的值**。

## 4. fail-closed 语义

- **kill 未执行 / 未成功就引用令牌** ⇒ `ScenarioError` 点名令牌与「尚未成功执行」，
  `sent` 一帧都不发；**不回落 0、不用静态 `params` 顶替**——回落等于退回「固定格打空」的形态，
  那正是本卡要消灭的假绿。
- **令牌拼写错**（如 `@last_kill_x`）⇒ **加载期** `ScenarioError`，并列出可用令牌。
- `state` 里的键不带 `@`（引擎内部用 `last_kill_xy`），`@` 只出现在场景文本里。

## 5. 与静态 `params` 的关系

| | 静态 `params` | 运行期令牌 |
|---|---|---|
| 定值时机 | 加载期（`--var` / 场景 `params`） | 执行到该步时从 `state` 取 |
| 可用位置 | 所有走 `resolve_param` 的字段 | 目前仅 `send.recog` |
| 失败形态 | 缺 `required` ⇒ 加载期拒 | 值未产生 ⇒ 运行期 `ScenarioError`（一帧不发） |

## 6. 牙（`_selftest_runtime_token`，5 条，**承重**）

| 牙 | 断言 | 变异实测（改坏 ⇒ 红） |
|---|---|---|
| a | kill 成功 ⇒ `last_kill_xy`=尸体格，且后续 `send` 真发出该 recog | 「不写 state」⇒ ScenarioError（红） |
| b | kill 失败（打了没死）⇒ 引用令牌 `ScenarioError` 且**一帧不发** | 「回落 0」⇒ 牙b 红 |
| c | 未执行 kill ⇒ 同上、零发送 | 「回落 0」⇒ 牙c 红 |
| d | 令牌拼写错 ⇒ **加载期**拒并列出可用令牌 | 「去掉加载期校验」⇒ 牙d 红 |
| e | 死亡帧 `param/tag` 与目标格不一致 ⇒ 拒绝写并打印两值 | 「去掉坐标一致性检查」⇒ 牙e 红 |

`mir2_probe --selftest`：**21 checks OK**（原 20 + 本套 1 组）。
注意：probe 的 selftest harness 只看**异常**（各套的返回值只当明细打印）——所以本套在末尾把
`False` 聚合成 `AssertionError`，与 `_selftest_kill` 的 `for cond, msg in checks: assert cond, msg`
同口径。少了这一步，五颗牙就只是装饰（改坏也不会红）。

## 7. 未做

- **真机 butch 链**（`kill(track)` → `CM_BUTCH(@last_kill_xy)` → `SM_BUTCH(637)`）：归
  `openmir2-butch-two-draws-unblock`，等窗口；本卡只保证 DSL 侧的取值正确（构造级牙已证）。
- 其它动作的令牌接线（若将来需要，按 `send.recog` 同一形态加，并在 §2 表格登记）。
- **删除不可运行的兜底场景**（Leader 裁决，并入本卡）：`tools/probe/scenarios/npc_click_king_slow.json`
  在 `--check-scenario` 下必然被拒（`steps[0].wait_for [10]` 之前没有 send 步，来自 `ff2fb1e5d`）；
  既然 `name:` 已是确定性寻址，兜底不再需要——留着会让人误以为有兜底，比没有更糟。已删文件，
  并把 `docs/probe-discovery-addressing-20261025.md` 的对应表行/说明改成「曾考虑、因确定性未采用」。

## 8. 门禁

`mir2_probe --selftest` 21 checks OK；`tools/probe/scenarios/*.json` 的 `--check-scenario`：
按各场景 `required` 补 `--var` 后 **31/31 rc=0**（不可运行的 `npc_click_king_slow` 已按 Leader
裁决删除；补 var 时 `butch_two_draws` 的 `target_xy` 要给**真实相邻格**——随便填 `1` 会被
八向相邻门按设计拒掉，那不是场景缺陷）；7 项卫生门 + `git diff --check <base> <head>` + `merge_tree_helper` 见 patch body。
**未跑 cargo、未起栈、未占端口。**
