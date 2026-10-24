# 文档计数清扫（2026-10-24，卡 `openmir2-doc-count-sweep`）

- 卡：`openmir2-doc-count-sweep`（tier=L，写集 `docs/`）。
- 数据时点：`@ fd2db51e9 @ 2026-10-24 12:17 +08`（master）；全部「当值」都在该 commit 实跑。
- 目标：把 `docs/` 里引用各工具 selftest / 测试通过数的**过期计数**扫出来并订正；
  标了时点的历史记录一律保留。

## 1. 当值（逐条实跑）

| 工具 | 命令 | 当值 |
|---|---|---|
| stale_pointer_gate | `python -I tools/hygiene/stale_pointer_gate.py --selftest` | **34/34** |
| dynamic_count_stamp_gate | `python -I tools/hygiene/dynamic_count_stamp_gate.py --selftest` | **32/32** |
| card_window | `python -I -B tools/conformance/card_window.py --selftest` | **240/240** |
| goal_gate | `python -I tools/goal/goal_gate.py selftest` | **71/71** |
| board_coverage_gate | `python -I tools/hygiene/board_coverage_gate.py --board .walgit/board.toml` | 0 hidden / **547** card threads |
| whitelist_anchor_gate | `python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` | **590** 锚 ALL PASS |
| goal_gate 条件② 六桶 | `python -I tools/goal/goal_gate.py run --json --skip-tests --skip-script` | `window_required` **131**（**2** 未合入，合计 **133**）、whitelist **48**、已裁定 **3**、merged 未 closed 且无登记 **5**、需人工 **5**、三者皆无 **0**；R 卡 **200** 张（`@ fd2db51e9 @ 2026-10-24 12:17 +08`） |

另核对两个**同名数字但不同工具**的当值（避免误判过期）：`attack_rng_ledger.py selftest` = **70/70**、
`rng_ledger_diff.py selftest` = **57/57**（ability-weaponstrong 文档引用的就是它们，**不是** goal_gate）；
`gate_timer_gate.py --selftest` = **32/32**（m3-remaining / r-card-evidence 的 B-41 引用，**不是**
dynamic_count_stamp_gate）。

## 2. 扫描矩阵（整树 `git grep`，逐处判定）

| 旧数 | 命中 | 判定 | 处置 |
|---|---|---|---|
| `21/21` | dynamic-count 文档 ×4 | 全部带「旧写 / 当时」标注 ⇒ 历史记录 | 保留 |
| `31/31` | dynamic-count :324「骨架现为 31/31」 | **未标时点的现时断言，已过期（现行 34/34）** | **订正**（见 §3-1） |
| `32/32` | dynamic-count 文档 ×5（现行）＋ B-41 ×2（gate_timer_gate，实跑 32/32 现行） | 现值正确 | 保留 |
| `34/34` | stale-pointer 文档（现行） | 现值正确 | 保留 |
| `70/70` | ability 文档 ×2（attack_rng_ledger，现行 70/70）；goal-gate-unmerged :74（goal_gate，已过期 → 71/71） | 后者**订正**（见 §3-2） | 1 保留 + 1 订正 |
| `70 条断言，70 通过` | goal-gate-unmerged :23 | **未标时点，已过期（现行 71/71）** | **订正**（见 §3-2） |
| `237/237` | inventory-fixture :73、gate-baseline（历史实测 234 对照） | 带时点的历史记录 | 保留 |
| `240/240` | runbook ×2、dryrun-sweep ×2 | 现值正确（本卡实跑复核） | 保留 |
| `3159 passed` | 无命中 | — | — |
| `71 条` | 无命中（本卡在 §3-2 补上） | — | — |
| `旧：123` | needs-human :14 | **算术错误：旧总数应为 125（123 已合并 + 2 未合并）** | **订正**（见 §3-4） |
| `（其中未合并 2）` | goal-gate-full、unmerged-window-bucket、needs-human 历史块 | 全部是带时点的历史输出引用（header 快照戳/「历史留存」块） | 保留；`goal_gate` 源已改「另未合并」 |
| `13 checks` | needs-human :85（preflight，实跑 13）；inventory-fixture（mir2_probe 动态计数） | 现值正确 | 保留 |
| `590/589/583`（anchors） | gate-baseline（历史）、本卡当值 590 | gate-baseline 带时点 | 保留 |

## 3. 订正（旧 → 新，每处已复核）

1. `docs/dynamic-count-stamp-gate-20261024.md:324`：「骨架现为 31/31」→「骨架现为 **34/34**（旧写 31/31，
   2026-10-24「另未合并」卡 +3 后实跑）」。证据：本卡 §1 第 1 行实跑输出。
2. `docs/goal-gate-unmerged-selftest-20261024.md:23`：「70 条断言，70 通过」后补「2026-10-24 现行为
   **71 条断言，71 通过**（+1 来自 `openmir2-goal-gate-wording-fix`，本次清扫实跑）」。证据：本卡 §1 第 4 行。
3. 同文件 :74 表格：「**70/70 绿**」→「**70/70 绿**（当时；2026-10-24 现行 71/71）」。
4. `docs/needs-human-20261024.md:14`：「旧：123」→「旧：**125**（123 已合并 + 2 未合并；2026-10-24 订正）」。
   依据：旧 detail 的 123 是 merged-only 计数，未合并 2 张在独立桶（`goal_gate` 旧输出逐字可查，见同文件
   :128 的历史留存块）；needs-human-refresh 卡的 carried 也点名过同一问题。

## 4. 保留的历史记录（未改，理由）

- dynamic-count :72/:115/:143/:301 的 `21/21`：均为「旧写 / 当时」标注，且 :72 已指向现行 32/32。
- inventory-fixture :73 的 `237/237`、gate-baseline 全文：报告的是**当时 commit** 的实测，头部带时点。
- goal-gate-full / unmerged-window-bucket / needs-human 历史块的 `（其中未合并 2）`：是旧版 `goal_gate`
  输出的**逐字引用**，所在文档头部均有快照戳；`goal_gate` 现行措辞已是「另未合并」（本仓
  `tools/goal/goal_gate.py:765`）。

## 5. 判不准

- 无。本次扫的 8 类旧数全部能定位并判定；B-41 的 `32/32` 与 ability 的 `70/70、57/57` 经实跑确认为
  **不同工具的现值**，不按过期处理（这是本轮最容易误判的两处）。

## 6. 复算命令

```sh
python -I tools/hygiene/stale_pointer_gate.py --selftest
python -I tools/hygiene/dynamic_count_stamp_gate.py --selftest
python -I -B tools/conformance/card_window.py --selftest
python -I tools/goal/goal_gate.py selftest
python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check
python -I tools/goal/goal_gate.py run --json --skip-tests --skip-script
git grep -n "21/21\|237/237\|70/70\|31/31\|旧：123" -- docs
```

## 7. 残留

- 未改 `src/**`、`tools/`、`rules.json`、场景 JSON、`whitelist.md`、任何卡状态；未起栈、未占端口。
- 计数会继续漂移：本文的「当值」只对 `@ fd2db51e9` 有效；后来者请按 §6 现取，不要抄本文数字。
