# 魔法夹具 selftest 补洞：MG-7 + `/* magic */` 标记（2026-10-23）

> 卡：`openmir2-magic-fixture-selftest-gaps`（tier=L）
> 产出：`openmir2-worker-66`（worktree `wt-worker-66`，branch `card/magic-selftest-gaps`）
> 基线：`walgit/master` = `ed4dd05c6`
> 写集：`tools/conformance/card_window.py`（+44/−3）、本文件。未改 `src/**`、`rules.json`、`scenarios/*.json`。
> 来源：`openmir2-window-magic-fixture` 的 review `085b5de4`（reviewer-6）minor-1 / minor-2。

---

## 0. 结论

1. **MG-7**：新增 4 条断言，钉住 `sides.<side>.magic` 覆盖逻辑——顶层 `magic=1` + `sides.csharp.magic=2` ⇒ csharp 声明/写入 `MagicId=2`、rust 保持 `MagicId=1`。加在 selftest 的 rc 判定路径**之前**（最后一条 MG-6 之后、`finally` 之前，纪律 15）。
2. **`/* magic */` 标记**：`run_magic` 的 `SHOW COLUMNS`（`:1991`）加标记后，测试里真正走 magic 桩的 `SHOW COLUMNS` 分支（原 `:3238`，现 `:3238-3249`）。
3. **顺带修正（必要）**：magic 桩的 `SHOW COLUMNS` 原来回的是 **`characters_magic` 的列**（`Id/PlayerId/MagicId/Level/UseKey/CurrTrain`），而查询对象是 **`characters`**——分支一旦被走到，`run_magic` 会因缺 `ChrName/LoginID` 报 rc=2。本次改为回 `characters` 表列（`Id/ServerIndex/LoginID/ChrName/MapName/CX/CY/Level/HomeMap/Deleted`），并加 `magic-columns <side>` 日志标记。
4. 自检 **197/197 → 201/201**（只增不减）；两颗牙 rc 均为 **1**；逐字节还原后 `sha256(card_window.py)=cec0f997…`（= 提交树 blob）。

---

## 1. MG-7 的四条断言

用例：`magic_prof(MAGIC_OK)`（顶层 `magic = [{magic_id:1, level:0, use_key:"1", curr_train:0}]`）
+ `prof["sides"]["csharp"]["magic"] = [{magic_id:2, level:1, use_key:"2", curr_train:3}]`；
两侧 `STUB_MAGIC_INIT` 预置「已学 MagicId=1」。

| # | 断言 | 实测 |
| --- | --- | --- |
| 1 | `declared` 两侧不同：csharp=`[2]`、rust=`[1]` | PASS（`csharp=[2] rust=[1]`） |
| 2 | 写入按侧分派：csharp `action=written` 且 `sql.write` 含 `VALUES (6228, 2,`；rust `action=noop` 且 `sql.write is None` | PASS（`cs='written' rs='noop'`） |
| 3 | 读回：csharp `MagicId=2/level=1`、rust `MagicId=1/level=0` | PASS |
| 4 | 两侧 `SHOW COLUMNS` 都走 magic 桩（日志含 `magic-columns csharp` / `magic-columns rust`） | PASS |

**这条用例有牙**：把 `profile_magic` 的 `sides.<side>.magic` 覆盖分支删掉（只取顶层）⇒ 断言 1–3 全部变红（见 §3 牙 A）。

---

## 2. `/* magic */` 标记与 magic 桩分流

- 改动：`run_magic` 的 `cols_out = sql("magic.columns", "/* magic */ SHOW COLUMNS FROM ...")`。
- 桩分流（`STUB_SOURCE`）：`if "/* magic */" in sql_text:` ⇒ magic 桩；其 `SHOW COLUMNS` 分支现在
  1. 追加日志 `magic-columns <side>`；
  2. 回 `characters` 表列。
- **为什么必须同时改列**：查询是 `SHOW COLUMNS FROM characters`，但旧分支返回的是 `characters_magic` 的列；分支被走到后，`run_magic` 的 `missing = ["Id","ChrName","LoginID"]` 检查会失败（rc=2）。旧分支因缺标记从未被执行，所以这个错误一直没暴露——这正是「实现有、用例无」的第二个洞。
- **被走到怎么证明**：MG-7 第 4 条断言直接读 selftest 桩日志（`read_log()`），要求两侧都出现 `magic-columns <side>`；另有牙 B（去掉标记 ⇒ 该断言红）作反向证明。

---

## 3. 牙（真实 rc）

### 牙 A：覆盖逻辑改成「只取顶层」

把 `profile_magic` 的 `if "magic" in sc: return ...` 分支删掉，重跑 `card_window --selftest`：

```
FAIL MG-7 sides.csharp.magic 覆盖顶层 => declared 两侧不同（csharp=[1] rust=[1]，rc=0）
FAIL MG-7 写入按侧分派：csharp 写 MagicId=2、rust 不写（cs='noop' rs='noop'）
FAIL MG-7 读回：csharp=MagicId2/level1、rust=MagicId1/level0（两者都是 MagicId1/level0）
RED: card_window.py selftest 198/201
rc=1
```

### 牙 B：去掉 `/* magic */` 标记

```
FAIL MG-7 两侧 SHOW COLUMNS 都走 magic 桩（日志 magic-columns 标记）
RED: card_window.py selftest 200/201
rc=1
```

**还原**：两处变异均从备份逐字节还原；还原后
`sha256(tools/conformance/card_window.py) = cec0f997192300a6cb09879ff5c86242b05cfd5dfb54af071df42aa0371a57d6`，
重跑 `--selftest` 回 **201/201 GREEN**，rc=0。

---

## 4. 门禁（实跑）

| 门禁 | 结果 |
| --- | --- |
| `python -I -B tools/conformance/card_window.py --selftest` | **GREEN 201/201**，rc=0（改前 197/197；+4，只增不减） |
| `python -I tools/hygiene/conflict_marker_gate.py` | **0 markers / 7412 tracked files**，rc=0 |
| `python -I tools/whitelist/t_reservation_gate.py check` | **GREEN**（246 行 / 246 号），rc=0 |

---

## 5. 边界与残留

- 未改 `src/**`、`rules.json`、`scenarios/*.json`；未起栈、未连库、未占端口。
- 未合并、未自签 review、未删 worktree。
- 与在审的 `openmir2-window-inventory-fixture` 同写集（`card_window.py`）：本分支从 master `ed4dd05c6` 起；若合并时冲突，按「保留两边」解（本卡改动集中在 `run_magic` 的 SHOW COLUMNS、magic 桩、selftest 末尾 MG-7）。
