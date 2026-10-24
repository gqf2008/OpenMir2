# 18 场景 `--dry-run` 可用性矩阵（2026-10-24 @ `e9d00c6b3` @ 09:58 +08）

- 卡：`openmir2-window-scenario-dryrun-sweep`（tier=L）；写集 `tools/probe/scenarios/`、`docs/`。
- 口径：只做 `--dry-run`（不起栈、不连库、不占端口、不碰 `E:/MirServer` 的默认端口栈）；
  场景清单**只读**取自 `docs/window-batch-plan-20261023.md` §7.4（**12 行 / 18 个场景**，本文件不改它）。
- **快照戳**：`alt` / `csharp-default` 两列 `@ e9d00c6b3 @ 2026-10-24 09:58 +0800`；
  **`window-testdb` 一列 `@ 7cc81fbfa @ 2026-10-24 11:04 +0800`**（原测于 feature commit `633e2d8a1`，
  同一组数字在 tip `7cc81fbfa` 上复验一致；卡 `openmir2-window-profile-testdb-preset`，只加 profile 数据，
  见 §2.1）。两套 profile 各跑一遍；rc 与关键输出均可按 §4 命令复跑。

## 1. 矩阵（rc 与结论）

| 场景 | profile | rc | 结论 | 关键输出 / 依据 |
|---|---|---:|---|---|
| `login_only` | alt / csharp-default | 0 / 0 | 可跑 | 只需角色预置位（§7.4） |
| `pickup_item` | alt / csharp-default | 0 / 0 | 可跑 | `self_x/self_y` 由 `SM_LOGON` 推导 |
| `move_walk` | alt / csharp-default | 0 / 0 | 可跑 | `move_x/move_y` 走 `computed.pack` |
| `upgrade_retrieve` | alt / csharp-default / **window-testdb** | **0 / 2 / 0** | alt 与 window-testdb 可跑；csharp-default 被**生产库守卫**拒绝 | csharp-default：`dry-run … profile 指向生产库（prod, mir2_db）` ⇒ 真跑会被拒（设计内 fail-closed）；window-testdb：`--db test` / `db=mir2_test_db`（**仅测试库**） |
| `equip_struck_save` | alt / csharp-default / **window-testdb** | **0 / 2 / 0** | 同上 | 同上 |
| `bag_equip` | alt / csharp-default / **window-testdb** | **0 / 2 / 0** | alt 与 window-testdb 可跑；`inventory` 预览两侧各 1 行 | alt：`[csharp] 背包夹具（dry-run 不连库）：db=mir2_test_db … positions=[40] make_index=[5001]`、`[rust] … mir2_test_db_r …`；window-testdb 同形；`--var` 走 discovery（`item_name/make_index/takeon_where`） |
| `eat_item` | alt / csharp-default / **window-testdb** | **0 / 2 / 0** | 同上 | alt / window-testdb：`positions=[41] make_index=[5002]` 两侧各 1 行 |
| `dice_chain` | alt / csharp-default | 0 / 0 | 可跑 | 需 `--var npc_actor=<本次 id>`（默认 119 是旧抓包值） |
| `dice_chain_auto` | alt / csharp-default | 0 / 0 | 可跑 | actor 自动发现 |
| `npc_click` / `npc_click_auto` | alt / csharp-default | 0 / 0 | 可跑 | 场景 `placement` |
| `king_dialog` / `merchant_dialog` | alt / csharp-default | 0 / 0 | 可跑 | `king_actor` / `merchant_actor` 发现 |
| `npc_script_chain` | alt / csharp-default / **window-testdb** | **0 / 2 / 0** | alt 与 window-testdb 可跑；csharp-default 生产库守卫 | 同上 |
| `king_castle_war` | alt / csharp-default | 0 / 0 | 可跑（语境另注） | 跑前确认城堡战语境（§7.3 castle_zone 缺口） |
| `gm_time` / `gm_batch` | alt / csharp-default | 0 / 0 | 可跑 | 无前置 |
| `spell_cast` | alt / csharp-default / **window-magic** | **2 / 2 / 0** | **可跑（用 `window-magic`；alt/csharp-default 缺 magic 前置）** | 前三格为缺少前置的 fail-closed；`window-magic`（2026-10-24 新增，卡 `openmir2-window-profile-magic-preset`）⇒ rc=0 且两侧 `magic_ids=1` 预览（证据 `@ 72c26da9f @ 10:33 +08`） |

**20 个场景文件的另两个**：`combat_hit` / `combat_strike`（不在 §7.4）实测仍 `rc=2 BLOCKED`
（缺同屏怪物/伤害夹具）——设计内，未被本卡改动。

## 2. 分类结论

- **可跑（alt）**：**17/18** 场景 `rc=0`；`spell_cast` 现由**内置 `window-magic`** 承担（`--profile window-magic`
  ⇒ rc=0 + 两侧 `magic_ids=1` 预览，2026-10-24 @ `72c26da9f`），**不是坏场景**。
- **无坏场景**：本卡**未改任何场景 JSON**（18 个文件 dry-run 均按语义返回 0 或设计内 2）；
  因此不需要 `card_window --selftest` 复跑（未动代码/场景；如需复跑见 §4）。
- **profile 结论（重要，给开窗人）**：`csharp-default` 把 C# 侧 DB 指向 **生产库**（`prod` / `mir2_db`）。
  dry-run 会在 `预置位/参数发现不可行` 处 fail-closed（rc=2），**真跑会被拒绝**；
  **不要**为绕过它加 `--allow-production-db`（违反「不碰生产库」红线）。开窗请用 **`alt`**
  （或 `window-testdb`——**今天两者等价**，见 §2.1；也可自建指向 `mir2_test_db*` 的 profile）。
- **新增内置 profile `window-testdb`**（卡 `openmir2-window-profile-testdb-preset`，见 §2.1）：
  它是 `alt` 的深拷贝 + C# 侧显式指向测试库，专门给那 5 个**带 `placement`** 的场景一个稳定的
  测试库句柄。它**只把输入指向测试库**——生产库守卫一行未改，`csharp-default` 下那 5 个仍然 rc=2。
  **仅测试库，绝不可用于生产。**
- **设计内 blocked**：`combat_hit`/`combat_strike`（夹具，§7.3「跑也没用」）。`spell_cast` 已**不再是 blocked**：
  新内置 `window-magic` 让它 rc=0（本卡只复核，不改判据；`requires:["magic"]` 保持）。

### 2.1 内置 profile `window-testdb`（**仅测试库，绝不可用于生产**）

- **形状**：`alt` 的 `copy.deepcopy` + C# 侧显式指向测试库（`db.preset=test` / `db.characters_db=mir2_test_db`；
  Rust 侧沿用 `mir2_test_db_r`）。**只加 profile 数据，不改判定逻辑，不削弱生产库守卫。**
- **安全边界**：守卫 `check_profile_safety` 的 `prod(...)` 代码 / 报错文案 / 触发条件**一字未动**；
  该 profile 既无 `production_db` 标记，也不需要 `--allow-production-db`。**它是测试库 profile，
  不是生产窗口 profile。**
- **实测（原测于 `633e2d8a1`；tip `7cc81fbfa` 复验一致；18 场景 `--dry-run`）**：与 `alt` 逐格一致 —— **17×rc=0 + `spell_cast` rc=2**；
  卡面点名的 5 个场景（`upgrade_retrieve` / `equip_struck_save` / `bag_equip` / `eat_item` /
  `npc_script_chain`）全部 **rc=0**，C# 预览命令为 `--db test`，Rust 为 `mir2_test_db_r` /
  `mir2_test_account_r`（无一处 `mir2_db`）。
- **回归**：既有 `alt` 与 `csharp-default` 的 18 格 rc **逐格不变**；`csharp-default` 下那 5 个**仍然 rc=2**
  （报错仍是 `db 规格展开后命中生产库：prod(sides.csharp.db.preset), mir2_db(sides.csharp.db.characters_db)`）。
- **selftest**：新增 2 条断言——22) `window-testdb` 存在、C# 侧指向 `mir2_test_db`、Rust 侧测试库、
  且非 dry-run 也不触守卫；22b) 钉住 `_derived_testdb_profile` 的**两行显式赋值**（给它一个 C# 侧
  指向生产库的合成基线，仍被强制改写成 `test` / `mir2_test_db`）⇒ `240/240` GREEN（基线 `238/238`）。
  22b 是返工补的牙：`alt` 的 C# 侧本来就有同值，若只比值，删掉两行会退化成纯别名而测试仍绿。

## 3. 与 §7.4 的一致性

未发现 §7.4 与实测冲突：18 个场景全部落在「可跑 / 有明确前置可跑」两类，没有需要「另开卡修场景」的坏件。
若 owner 后续要跑，按 §7.4 的「跑前还需」列准备（`upgrade_retrieve` 的武器/金币/黑铁矿、`equip_struck_save`
的商店 NPC 摆位、`dice_chain` 的 `--var npc_actor` 等）。**本文件不改 §7.4，也不改场景 JSON。**

## 4. 复算命令

```sh
python -I -B tools/conformance/card_window.py --dry-run \
  --scenario tools/probe/scenarios/<scene>.json --profile alt           # 或 csharp-default / window-testdb
# 18 个场景：login_only pickup_item move_walk upgrade_retrieve equip_struck_save
#   bag_equip eat_item dice_chain dice_chain_auto npc_click npc_click_auto
#   king_dialog merchant_dialog npc_script_chain king_castle_war gm_time gm_batch spell_cast
# 期望：alt 17×rc=0 + spell_cast rc=2；csharp-default 12×rc=0 + 5×生产库守卫 rc=2 + spell_cast rc=2
#       window-testdb 17×rc=0 + spell_cast rc=2（5 个预置位场景均 rc=0，库名 mir2_test_db*）
# 两个设计内 blocked（不在 §7.4）：combat_hit / combat_strike ⇒ rc=2 BLOCKED
```

门禁（本卡：`tools/conformance/card_window.py` + `docs/`；@ `7cc81fbfa`）：

```text
card_window.py --selftest                     GREEN 240/240
t_reservation_gate check                      GREEN 255/255
conflict_marker_gate                          OK 0 markers
stale_pointer_gate                            OK rc=0
dynamic_count_stamp_gate                      OK rc=0
git diff --check                              rc=0
CRLF=0 / BOM=false
git merge-tree --write-tree walgit/master HEAD  rc=0
```
