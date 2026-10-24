# carried 判据缺口补牙记录（2026-10-24，卡 `openmir2-carried-tooth-fix`）

- 卡：`openmir2-carried-tooth-fix`（tier=L）；来源 = `docs/carried-backlog-20261024.md` §4
  「建议立刻修」前 7 条中不涉及行为重写的 5 个工具（#9/#118 已由 `fd2db51e9` 修掉，
  #86 的家族谓词重构属行为面另卡）。
- 数据时点：`@ a7fbe4a33 @ 2026-10-24 13:0x +08`；五颗牙全部「缺 → 补 → 变异验证」三步，
  变异**串行**执行（每次只动一个文件，验证后 `git checkout --` 还原再下一个）。
- 纪律说明：#51/#52 的牙按卡面 ① 需要 `normalize_reviewer` / `report()` 两处**最小
  fail-closed 行为修正**（⑤ 的「只加测试」按「不动词表 / skip 区间 / 判定主逻辑」口径执行，
  已在本卡 comment `a7ff2d920` 向 Leader 报备）；其余三颗均为纯测试。

## 1. review_principal_check（#51/#52）

- **缺**：`normalize_reviewer` 只拒「空串/纯分隔符」，`None`/`null`/`n/a`/`a,b`/
  `openmir2-worker-a（作者）` 都归一成非 None ⇒ verdict=OK rc=0；`malformed_entries_skipped`
  只打印不影响 rc ⇒ 损坏条目（含 reviewer 自己的 claim）被跳过后仍可能给 OK。
- **补**：`normalize_reviewer` 增加占位字面表与「principal 字符集」`fullmatch` 校验
  （非法 ⇒ INVALID_REVIEWER rc=2）；`report()` 在判 OK 且 `malformed>0` 时改判
  `MALFORMED_SKIPPED` rc=2（fail-closed）。selftest 20 → **22**（⑰b/⑰c）。
- **变异**：
  - 删占位字面拒绝 ⇒ `FAIL ⑰b 占位字面/带注记/多值 reviewer => INVALID_REVIEWER rc=2（carried F2）`，
    `selftest: 21/22 passed`，rc=1；
  - 损坏条目照常给 OK（去掉 `rc = 2`）⇒ `FAIL ⑰c 损坏条目（actor=123）被跳过 ⇒ 不得给 OK
    （rc=2，carried F2 不变量）`，`selftest: 21/22 passed`，rc=1。

## 2. stale_pointer_gate（#101 / #38）

- **缺（#101）**：`_blame_delta` 的「blame 执行失败（rc≠0）」分支无夹具——
  自造变异「`return 0`（当可信）」在旧 selftest 存活。
- **补（㉝）**：单测直接以不存在的路径调 `_blame_delta(..., reasons=[...])`，
  断言返回 `None` 且 reason 以 `git blame rc=` 开头。
- **缺（#38）**：⑰ 未钉 span 级语义——把字面排除改成「整行否决」旧 selftest 仍全绿。
- **补（㉜）**：新夹具 `| \`openmir2-synthetic-closed\` | 未接线，且未合并 |` ⇒ 必须红
  （`未合并` 在 `未接线` 字面区间之外）。selftest 34 → **36**（㉜/㉝）。
- **变异**：
  - 字面排除改整行否决（`not any(...)` → `not noise_spans`）⇒ `FAIL ㉜ span 级语义：
    「未接线」同行还有真断言「未合并」⇒ 仍红（整行否决变异必红）`，`selftest: 35/36`，rc=1；
  - blame rc≠0 当可信（分支内 `return 0`）⇒ `FAIL ㉝ blame 执行失败（rc≠0）⇒ 不可用
    （返回 None + 点名 rc），绝不豁免`，`selftest: 35/36`，rc=1。

## 3. board_coverage_gate（#76）

- **缺**：selftest 未钉「已知卡片 kind 不得进 `NON_CARD_KINDS`」——把 `status` 加进集合后
  旧 selftest 仍全绿（现网线程会整类消失）。
- **补**：新断言 `NON_CARD_KINDS.isdisjoint({"issue","patch","status","review",
  "merge_result","comment"})`。selftest 8 → **9**（⑨）。
- **变异**：`NON_CARD_KINDS` 混入 `"status"` ⇒ `FAIL ⑨ known card kinds stay out of
  NON_CARD_KINDS (carried #76)`，`selftest: 8/9 passed`，rc=1。

## 4. card_window（#119）

- **缺**：22b 第 4 合取项 `wtd_builtin_db == from_alt_db` 恒真（alt 的 C# db 本就是
  test/mir2_test_db），纯别名 deepcopy 也绿。
- **补**：改为 `BUILTIN_PROFILES["window-testdb"].get("name") == "window-testdb"`
  （纯别名 deepcopy(alt) 的 `name` 仍是 `"alt"` ⇒ 红），原 `from_alt_db` 探针删除。
  selftest **240/240** 不变（改的是既有断言，不加条数）。
- **变异**：`_derived_testdb_profile` 退化为纯别名（不赋名、不指 test 库）⇒
  22b 断言红（实际 `{'preset': 'prod', 'characters_db': 'mir2_db'}` / `name='alt'`），rc=1。

## 5. 串行执行与回归

- 六次变异**串行**执行（rpc M-a → rpc M-b → stale ㉜ → stale ㉝ → board ⑨ → card_window 22b），
  每次只动一个文件、验证后 `git checkout -- <file>` 还原；六次还原后工作树干净。
- 还原后回归：review_principal_check **22/22**、stale_pointer_gate **36/36**、
  dynamic_count_stamp_gate **32/32**、board_coverage_gate **9/9**、card_window **240/240**。

## 6. 残留

- `carried-backlog` §4 其余三条不在本卡：#86（drain-order 家族谓词重构，行为面）、
  #9/#118（已由 `fd2db51e9` 消解）。#119 的「恒真合取项」按卡面给的两个方案选了改写。
- 未改任何判定主逻辑/词表/skip 区间/`--max-gap`；未改 `src/**`、`rules.json`、场景 JSON、
  `whitelist.md`；未起栈、未占端口、未连库。
