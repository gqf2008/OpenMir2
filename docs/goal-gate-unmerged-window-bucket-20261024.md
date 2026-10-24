# `goal_gate` 条件②：`window_required` 拆出「未合并」子类（`openmir2-goal-gate-unmerged-window-bucket`）

- 卡：`openmir2-goal-gate-unmerged-window-bucket`（tier=L）；分支 `card/openmir2-goal-gate-unmerged-window-bucket`
- **快照戳**：master `2000e1cc5`（2026-10-24）。**只改报告分类，不改 PASS/FAIL 语义。**

## 1. 改动

`tools/goal/goal_gate.py`

| 位置 | 改动 |
|---|---|
| `r_category` | `window_required`（历史 body 或 approve 裁定）且 **`merged=false`** ⇒ 新分类 `window_required_unmerged`；已合并仍为 `window_required`；`adjudicated(no_window)` 分支不变 |
| `classify_r_evidence` | 新分类 **仍返回 PASS**，文案「有 `window_required` 登记但尚未合并（窗口在合并前跑不了，单列计数；登记本身有效，仍算 PASS）」 |
| `judge_r_cards` 的 `detail` | `window_required N（其中未合并 M）`；`state` / FAIL / need 语义与条数不变 |

> 〔2026-10-24 订正：`goal_gate` 的 detail 措辞已由「其中」改为「另」；本文件内该串引用均为当时原文〕

## 2. 前后对照（真实数据，同一快照）

```sh
# 改前（master 版脚本）
git show walgit/master:tools/goal/goal_gate.py > /e/tmp/gg_master.py
python -I -B /e/tmp/gg_master.py --repo . --json
# 改后
python -I -B tools/goal/goal_gate.py --repo . --json
```

**改前**（条件② `detail` 原文，节选）：

```
R 卡 193 张：已完成 11 / window_required 121 / whitelist 44 / 已裁定登记 3 / … / 其它需人工 5
state = need_human，rc = 2
```

**改后**：

```
R 卡 193 张：已完成 11 / window_required 116（其中未合并 5） / whitelist 44 / 已裁定登记 3 / … / 其它需人工 5
state = need_human，rc = 2
```

- `window_required` **121 → 116**，新增「未合并 **5**」——与卡面预期一致；
- `state`、`FAIL`/`need` 名单、`merged 未 closed 但已凭登记 PASS`（161）**均不变** ⇒ 只是计数拆分。
- 5 张（`evidence.r_cards` 里 `category=window_required_unmerged` 现算）：
  `openmir2-gm-batch-5-guild-castle`、`openmir2-script-envir-root-host`、`openmir2-script-sendmsg-host`、
  `openmir2-script-user-account`、`openmir2-window-gate-two-runs`。

## 3. 构造级检查（4/4）

`/e/tmp/gg_cases.py`（直接 import `goal_gate` 调 `r_category` / `classify_r_evidence`）：

| 用例 | 输入 | 结果 |
|---|---|---|
| ① | `window_required=true, merged=false` | `window_required_unmerged` / **PASS** |
| ② | 同上 + `merged=true` | `window_required` / PASS |
| ③ | `window_required=false, merged=true`（无登记） | `merged_not_closed` / **NEED**（不变） |
| ④ | approve 裁定 `window_required` + 未合并 | `window_required_unmerged` / PASS |

## 4. 牙（突变 → 真实 `--json`；逐字节还原）

| 牙 | 突变 | 真实结果 | 还原 |
|---|---|---|---|
| T1 | 新分支恒返回 `window_required`（等于撤掉拆分） | `window_required 121（其中未合并 0）` ⇒ 拆分是承重的 | `git show HEAD:<path>` 回写，sha256 = `13dd2fcab5ff5e9cabb929e34cef2ffdaa2fbc5c5fa07182358bf39806b7d346`（= 提交后树） |
| T2 | 无视 `merged`（全判未合并） | `window_required 0（其中未合并 121）` ⇒ `merged` 门是承重的 | 同上 |

> 两次突变的 `state` 都仍是 `need_human`、`rc=2` —— 这**正是本卡要证明的**：拆分只动计数，不动判定。

## 5. 门禁

- `goal_gate.py --json`：改前/改后 `state` 与 `rc` 一致（need_human / 2）。
- **`goal_gate.py selftest` 存在且仍绿**：改后 `python -I -B tools/goal/goal_gate.py selftest`
  ⇒ `selftest：68 条断言，68 通过，0 失败` rc=0（master 版同样是 68/68 ⇒ 本次改动未加/减自我断言）；
  其合成输入**没有**对 `window_required` 计数做断言（用的是 review body 的 `window_required` 字段），
  故无需同步更新；新分类的构造级覆盖见 §3。
- 未改 `whitelist.md`、未改任何卡片状态。

## 6. 残留物

`E:/tmp/gg_master.py`（改前脚本副本）、`E:/tmp/gg-before.json`、`E:/tmp/gg-after.json`、
`E:/tmp/gg_cases.py`、`E:/tmp/gg_teeth.py`。
