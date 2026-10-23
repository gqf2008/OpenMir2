# 背包夹具返工（接管重交付）：rebase 到当前 master + 三条 fail-closed 牙（2026-10-24）

> 卡：`openmir2-window-inventory-fixture`（tier=L，返工）
> 产出：**接管** `openmir2-worker-66`（worktree `wt-worker-66`，branch `card/openmir2-window-inventory-fixture`）
> 原作者：外部车道 `openmir2-worker-24`（按纪律 19 不再向同身份重复派活；本卡由 worker-66 接管重交付）
> 基线：`walgit/master` = `bae1b20c4`（交付时 rebase 到最新；返工起点 `99a4d30c4`）
> 来源：review `fdebfa92`（reviewer-5）F1/m1 + orchestrator comment `532d45cf`
> 写集：`tools/conformance/`、`docs/`

---

## 0. 结论

1. **F1（当前 head 不能干净合并）**：已 rebase 到当前 master，`card_window.py` 的 5 处冲突按 **keep-both** 解决（inventory 1b 在 magic 1c 之前；`res` 同时有 `inventory`/`magic`；`blocked` 与 `inventory` 变量并存；selftest 的 pid 组与 inventory 组并存；inventory 组号保持 `18m`（placement 子项），不与 magic 的 21 冲突）。解完**自己重跑全部门禁**，未做机械解冲突直接合。
2. **m1（三条 fail-closed 无牙）**：新增 3 条用例（⑩⑪⑫）+ 3 颗牙（INV1/INV2/INV3）；三处破坏均使 `--selftest` **rc≠0**，逐字节还原。
3. `card_window --selftest`：**202（master）→ 216**（master 另含 multi-session-probe 的 +1；本卡 inventory 组 + 本次 m1 3 条）。
4. §「已确认好的」未重做：四种破坏、夹具语义、runbook §1.6 均沿用上一轮结论。

---

## 1. F1：rebase 与 keep-both 解冲突

### 1.1 冲突面（5 处，均在 `card_window.py`）

| # | 位置 | 解法 |
| --- | --- | --- |
| 1 | `run_side` 的 `res` 字典 | 同时保留 `"magic": None` 与 `"inventory": None` |
| 2 | restore→stack 之间的预置块 | **1b 背包夹具在前、1c 魔法夹具在后**；两侧失败都 `return res`（rc=2 路径保留） |
| 3 | `execute`/`main` 的 `blocked` 与 `inventory` 变量 | 两个变量都声明；两个 `scenario_*()` 调用都保留 |
| 4 | selftest 前的辅助函数区 | master 的 `same_path` / `_selftest_pid_binaries` / `_selftest_pid_wiring` 与 incoming 的 `_selftest_inventory` **并存** |
| 5 | selftest 组号 | inventory 保持 `18m)`（placement 子项），magic 保持 `21)`；无重号 |

### 1.2 一处「机械解冲突会踩」的坑（已修，记录在案）

冲突 hunk 把 `_selftest_pid_wiring` **从函数体中段切开**：HEAD 侧止于该函数的一条 `out.append(...)`，其后的
`return out` 落在 hunk 的共享尾上下文里。若把「HEAD 侧 + incoming 侧」直接拼接，`_selftest_inventory` 会被
插进 `_selftest_pid_wiring` 体内，导致后者**没有返回值**（`TypeError: 'NoneType' object is not iterable`），
且 `_selftest_inventory` 吞掉共享的 `return out`。

**修法**：从 master 取**完整的** `_selftest_pid_wiring`（到下一个顶层 `def` 为止）替换该段，再放
`_selftest_inventory`。修后 `python -I -c "ast.parse(...)"` 通过、`--selftest` 212/212。

⇒ 这正是「不做机械解冲突直接合」的原因：冲突标记消掉不等于语义正确，必须自己跑门禁。

---

## 2. m1：三条 fail-closed 的用例与牙

### 2.1 新增用例（在 `_selftest_inventory` 里，随 `18m)` 组执行）

| # | 场景 | 期望 |
| --- | --- | --- |
| ⑩ | **不 stub** `_check_db_pairing`：临时 `Server.conf` 声明 `database=mir2_test_data_r`（Rust 侧），profile 却拿 C# 的 `mir2_test_db` ⇒ `run_inventory` rc=2 | 错误含「不配对」 |
| ⑪ | 假 runner 的 `inventory.occupancy` 返回 1（声明槽位已被占用）⇒ rc=2 | 错误含「已有 1 行」 |
| ⑫ | 假 runner 的 `before_count=0`、`after_count=1`（其它槽位行数变了）⇒ rc=2 | 错误含「行数变了」 |

实测（`card_window --selftest` 输出）：

```
PASS ⑪ 声明槽位已被占用 => rc=2（'…声明槽位里已有 1 行（[3]）…'）
PASS ⑫ 其它槽位行数变了 => rc=2（'…其它槽位的行数变了（0 → 1）…'）
PASS ⑩ 库配对不匹配（部署 _r / profile C# 库）=> rc=2（"…与该侧部署不配对…"）
```

### 2.2 三颗牙（固化进 `selftest_mutations.py`）

| 牙 | 变异（精确替换） | expect | 真实结果 |
| --- | --- | --- | --- |
| INV1 | `res["db_pairing"] = _check_db_pairing(...)` → `= None` | `⑩ 库配对不匹配` | **rc=1**，该 FAIL 命中 |
| INV2 | `if occ_n != 0:` → `if False:` | `⑪ 声明槽位已被占用` | **rc=1**，该 FAIL 命中 |
| INV3 | `if after["count"] != before["count"]:` → `if False:` | `⑫ 其它槽位行数变了` | **rc=1**，该 FAIL 命中 |

`--only INV1,INV2,INV3` 实跑：`3/3 颗牙全红 + 逐字节还原`，还原后
`sha256(card_window.py) = 2e68df2e38be3cf2ee35dc39493a935a73ae721d44a1af2da98109580d84bc7c`。

---

## 3. 门禁（实跑）

| 门禁 | 结果 |
| --- | --- |
| `python -I -B tools/conformance/card_window.py --selftest` | **GREEN 216/216**，rc=0 |
| `python -I -B tools/conformance/selftest_mutations.py`（全量，不套外层 timeout） | **MUTATION HARNESS GREEN: 41/41**，rc=0（含 INV1..INV3；逐字节还原） |
| `python -I tools/whitelist/t_reservation_gate.py check` | **GREEN 250/250**，rc=0（master 前进后） |
| `python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` | **ALL PASS（548 条锚）**，rc=0 |
| `python -I tools/hygiene/conflict_marker_gate.py` | **0 markers**，rc=0 |

---

## 4. 未重做（沿用上一轮已确认结论）

- 四种破坏（旁路 `if inventory:` / `sql("inventory.update")`→`pass` / 失败 rc 2→0 / 删 std_mode 校验）已能红。
- 夹具语义（MakeIndex 两侧一致、空槽前置、排除声明槽位的指纹不变、写后行数按槽位、rc=2 fail-closed）。
- `docs/window-runbook-20261023.md` §1.6 背包夹具已落地。

---

## 5. 边界与残留

- 未改 `src/**`、`rules.json`、`scenarios/*.json`；未起栈/未连库/未占端口。
- 未合并、未自签 review、未删 worktree。
- 残留：`E:/tmp/worker66-scratch/`（冲突解析/修复脚本、变异输出；仓库外）。
