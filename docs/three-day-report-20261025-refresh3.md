- 数据时点：**刷新快照 commit = `6bb1f9839`**（2026-10-25 11:13 +08，本地 master，与远端 `walgit` 一致）。
- **今日合并**：**110 笔**（命令：`git log --first-parent --merges --since="2026-10-24 23:40"`）。

> 相对上一版报告的增量（**77 笔**）与差异说明见本文件末节「与上一版的差异」；上一版快照 oid 与时间见 `docs/three-day-report-20261025.md` 头部声明，本节不再重复引用旧 oid（避免与本节计数同段）。

| 条件 | 现状态 | 依据 |
|---|---|---|
| ① 脚本派发 100% | **pass** | `parse-stats` 106/106、调用 11829、未实现 0（第三方复核 @`2135f6154`，本轮未变） |
| ② R 卡证据 | **need_human** | 缺的是**真机窗口证据**本身（不是工具缺件）。见下方「今日窗口工具链修复」 |
| ③ B 类清零 | **pass** | recon `tracked=0`（rows 191 条非空、counts 一致） |
| ④ 门禁恒绿 | **仅差 `cargo_test_workspace`** | 7 项卫生门 + `cargo fmt` + `msgcodegen --check`(604) 全绿；**`master_sync` 已 PASS**（本地 `6bb1f9839` == 远端 `walgit`，walgit 已恢复）；`cargo_test_workspace` 需一次 `goal_gate --tests` 正式跑 |

### 今日「窗口工具链」实质修复（本轮主要产出）

这些改动不直接产出 R 卡证据，但**使窗口证据第一次可信**：

1. **`card_window` rc falsy-zero**（`d858508a1`）：`int(summary.get("exit_code") or 2)` ⇒ PASS 也返回 rc=2；改为显式判 None，并加 AST 牙（源码不得再有 `BoolOp(Or, 常量 2)`）。真机/批次双证：`run/` 与 `run2/` 的 PASS 行 `outer_rc=0`。
2. **`run_batch` 汇总口径**（同卡）：`skipped` 字符串 ⇒ `outer_rc/duration_s` 恒 null；verdict 不看 `outer_rc`。改为列真值 + `rc_attribution.py`（124⇒TIMEOUT、137⇒KILLED、`PASS(outer_rc_exception)` 准入）+ `OTHER>0 ⇒ rc=3 不写 summary`。
3. **harness 存档落定**（`7e2d04b6c` + `a07930f90`）：probe 后、拍库前等 `characters.LASTUPDATE` + `characters_ablity.ModifyTime` 指纹连续静默且 `changes>=1`，否则 `save_unsettled` rc=2 且不拍库。**真机四项判据 ①②③④ 通过**（双人独立复核），W1 的 `Weight 14/0` 假差异消失。
4. **TZ-1 会话时区**（`059babb83`）+ **硬化**（`6286d3666`）：sqlx 默认 `+00:00` vs C# `SYSTEM` ⇒ 同刻落库差 8h；改为贴 oracle，并把时间列见证从「全域 presence 豁免」改成**逐列登记 + 限定本角色行 + 窗口 `[start−600s, end+600s]`**，缺 `db.time_columns` 键 ⇒ rc 2（fail-closed）。
5. **`rules.json` 判据裁定**（`721ea45b8`）：53 条 PENDING ⇒ **50 转正 / 3 保留**（10/14/50），主标志保持保守口径；`ignore` 规则加**形态多重集比较**（`e335578bc`），**ident 14 裁定保持 PENDING**（转正会取消「形态不等 ⇒ UNDECIDABLE」的兜底）。
6. **夹具与场景**：`fixtures.monsters` 改**前插**（`6f35b049f`，追加到 39020/3081 行表尾在窗口内永远轮不到）；`inventory_replace/drop`；`#14/#15` 迁到 replace（修的是「**发现的输入源**」：`SM_BAGITEMS(201)` 里没该物品 ⇒ 变量推不出）；`discovery` 改 `name:/pos:` 确定性寻址；probe 加 `track` 寻靶与 `@last_kill_xy` 运行期令牌。
7. **两处真实行为差异修复**：`SM_SKELETON(33)` 非首刀必发（收官帧是 637）；**商人挂图空货架**（oracle `LoadNpcData` 末 `RefillGoods()`；Rust 只在 30s+ 计时支，且走过 `Dura==0` 过滤的路径 ⇒ 14 条 `[goods]` 只得 6 组）。

### 真机结果（今日）

| 批次 | 结果 |
|---|---|
| **W1**（21 条，旧工具链） | PASS 2 / FAIL 5 / UNDECIDABLE 14；DB 面因存档时机**不可用** |
| **run2**（6 条，DB 面复跑，`--ignore-idents 14`） | **PASS 3 / FAIL 3 / UNDECIDABLE 0**、`run_batch rc=0`；`login_only`/`npc_click_auto` 由 W1 的 FAIL **翻为 PASS** ⇒ 存档假差异的直接证据 |
| **#23 复拍**（E 类修复后） | **进行中** |

### 仍缺（按优先级）

1. **真机窗口时段**（owner）：Q1 全量 21 条；G11 裁决是条件② 唯一 `merged_not_closed` 卡。
2. **DB 面证据回填台账**：`run2` 3 条 PASS + 3 条 FAIL 的逐卡映射（`move_walk` 的 `HomeMap/HomeX/HomeY` 稳定差异已开卡 `openmir2-home-coords-divergence`）。
3. **butch 真机链**（装备 Dc>0 → track 三段 → `@last_kill_xy` → 637 → 两抽账本）、ignore-count / neighbor-drift 的 ≥3 轮复跑。
4. **`cargo_test_workspace`** 一次 `goal_gate --tests`。

---

