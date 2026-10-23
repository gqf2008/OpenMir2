# 场景 BLOCKED 硬门：schema 与消费侧 fail-closed

卡：`openmir2-window-blocked-scenario-hard-gate`（tier=L）
落地提交：本文件同批（`card/window-blocked-hard-gate`）

## 1. 为什么改成机判

`combat_strike` 的封口此前只写在**人话**里（`description` / `steps[].note` / README / 设计文档 §7.0
四处）。但窗口批次读的是机器字段，不读人话；本日已三次出现「文字写了、机判不拦」的假绿。
本卡把「BLOCKED 场景不得被窗口批次消费」从**流程约定**升级为**技术级硬门**：
封存理由进场景**顶层机器可读字段**，`tools/conformance/card_window.py` 见到就直接 `rc=2`，
连 out 目录、栈、report 都不产生。

## 2. schema

场景 JSON 顶层（与 `name` / `steps` / `params` / `required` / `computed` / `placement` 同级）：

```json
{
  "name": "combat_strike",
  "blocked": {
    "reason": "夹具不存在：基线角色没有一档同时满足 DC_min>59 与 HitPoint>=17",
    "missing_fixture": "combat_strike 夹具矩阵（docs/combat-strike-scenario-20261023.md §6）"
  },
  "steps": ["..."]
}
```

| 键 | 必填 | 类型 | 口径 |
|---|---|---|---|
| `reason` | 是 | 非空字符串 | 为什么不可跑；缺/空/非字符串 ⇒ 加载期拒绝 |
| `missing_fixture` | 否 | 非空字符串 | 缺什么夹具；给了空串按非法处理（空串会掩盖信息） |

未知键 **fail-closed**（与 `scenario.placement` 同口径）。`blocked` 可以只写 `reason`。

语义要点：**`blocked` 是形状合法的场景**。它只是不可跑，所以：

* `mir2_probe --check-scenario` 对合法 `blocked` 场景仍是 **rc=0**（形状通过）；
* 结构非法（缺 `reason` / 类型错 / 未知键 / 空串）⇒ **rc=2**，错误信息点名 `scenario.blocked.*`；
* `card_window.py`（dry-run 与真跑都算）见到合法 `blocked` ⇒ **rc=2**，点名 reason 与
  missing_fixture，且**不建 out 目录、不起栈、不跑 discovery、不写 report**、连命令序列都不预览。
  失败等级与「端口被占」相同。

## 3. 落点

| 侧 | 位置 | 行为 |
|---|---|---|
| 形状校验 | `tools/probe/mir2_probe.py` `load_scenario()`（`blocked` 进 optional 键，`:390`）→ `_check_blocked()`（`:442`） | 合法 ⇒ 放行；非法 ⇒ `ScenarioError`（`--check-scenario` rc=2） |
| 消费侧硬门 | `tools/conformance/card_window.py` `scenario_blocked()`（`:1166`）读声明 → `execute()` 里读入（`:2256`）→ 硬门（`:2335`） | 命中 ⇒ `verdict=BLOCKED` / `exit_code=2`，在任何 `os.makedirs`、起栈、probe、report 之前 `return` |
| 自检 | `card_window.py --selftest` 第 19 组 `BL-1..BL-4`（rc 判定 `if fail_n: return 1` **之前**） | 10 条断言：dry-run/真跑 rc=2、不建 out、不起栈、不写 report、形状非法 ConfigError、无 blocked 不误伤 |
| 变异牙 | `tools/conformance/selftest_mutations.py` `B1` | 把 `if blocked:` 改成 `if False and blocked:` 必须红，`expect` 锁在 `BL-1` 组 ⇒ 证明断言的是**调用点**而不是仅函数 |

两处实现**各自重读场景文件、各自 fail-closed**（与 `scenario_required` / `scenario_computed` /
`scenario_placement` 一致）：probe 没跑过也不影响 card_window 的判定。

## 4. 回归（本批实测）

* 18 个既有场景逐个 `--check-scenario`（各用**自己的** `required` 变量）与
  `card_window --dry-run`：全部 **rc=0**，无误伤（表见卡线程的 status）。
* 三颗牙的真实退出码：

| 牙 | 操作（临时副本） | 命令 | 真实 rc |
|---|---|---|---|
| ① | 给正常场景加 `blocked` | `card_window --dry-run` | **2**（打印 `scenario is BLOCKED: <reason>` + `missing_fixture`） |
| ①' | 同上（正常场景加 `blocked`） | `mir2_probe --check-scenario` | **0**（合法块，只是不可跑） |
| ② | `blocked` 缺 `reason` | `mir2_probe --check-scenario` | **2**（`scenario.blocked missing key(s): ['reason']`） |
| ②' | 同上 | `card_window --dry-run` | **2**（`scenario.blocked.reason 必须是非空字符串`） |
| ③ | 去掉 `blocked` | 两条命令 | **0 / 0** |

牙用的副本与原场景 sha256 一致（逐字节还原），且仓库内 `tools/probe/scenarios/` 本批 **零改动**。

## 5. 如何封存 / 解除

1. 封存：在场景顶层加 `blocked`（写清 reason 与 missing_fixture），提交前跑
   `mir2_probe --check-scenario`（应 rc=0，证明形状合法）与
   `card_window.py --dry-run`（应 rc=2，证明门生效）。
2. 解除：夹具补齐后删掉 `blocked` 整块，两条命令回到 rc=0/0。
3. **不要**用 `blocked` 代替描述性说明：它只表达「不可跑」，不表达「跑绿说明什么」。

## 6. 边界

* 本批只交付**机制**；`tools/probe/scenarios/*.json` 与 `scenarios/README.md` 属另一车道
  （`openmir2-scenario-upgrade-retrieve`）的写集，未改动，`combat_strike.json` 也**未**加 `blocked`
  （由卡 owner 在合并后单独处理）。
* schema 说明同时写进 `mir2_probe.py --check-scenario --help` 与 `card_window.py --scenario --help`。
* 不起真栈、不连库、不占端口；`E:/MirServer` 内容冻结。
