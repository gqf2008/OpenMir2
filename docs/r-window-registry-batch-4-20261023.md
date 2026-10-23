# R 卡窗口登记 B2 批 4（T-254..T-262，2026-10-23）

> 卡：`openmir2-r-window-registry-batch`（第 4 批，tier=L）
> 产出：`openmir2-worker-66`（worktree `wt-worker-66`，branch `card/r-window-registry-batch`）
> 基线：`walgit/master` = `512d9ecf5`（批 3 已合入）
> 写集：`mir2-rs/tests/parity/whitelist.md`（+批 4 节）、`T-RESERVATIONS.md`（+9 行预约）、本文件。
> 未改 `approved_by`、未动任何卡片 status。

---

## 0. 结论

- 名单**现取**：`goal_gate --json` 的「merged 未 closed 且无登记」= **17** 张（`generated_ts=1792769214`，15:26:54Z，commit `512d9ecf5`）。
- 按 `last_status_ts` 升序；**显式顺延**最旧 3 张（批 1/批 2 已 comment）⇒ 取接下来 10 张。
- **登记 9 张（T-254..T-262）**；**1 张无窗口判据 ⇒ 只发 comment**（`gate-gmclose-race`，comment `9aae1e69…`）。
- 复算（15:29:35Z，`generated_ts=1792769375`）：`whitelist 35→44`、`merged 未 closed 且无登记 17→8`、`需人工 22→13`；9 张全部 `whitelist_t`。
- 取号：9 次 `python -I tools/whitelist/next_t.py --card <卡 id>` ⇒ T-254..T-262（`appended=True journal=on remote=fresh`）。

---

## 1. 名单现取（命令 + 原文）

```sh
python -I tools/goal/goal_gate.py --json > /tmp/goal-before.json
# 过滤 condition==2 且 reason 以 "merge_result merged=true" 开头（另 5 条 board 未列出计入其它需人工）
# 排序 need_items[].last_status_ts 升序；显式顺延最旧 3 张已 comment 的卡
```

**批前**（`generated_ts=1792769214`，2026-10-23T15:26:54Z，commit `512d9ecf5`）：

```
R 卡 192 张：已完成 10 / window_required 121 / whitelist 35 / 已裁定登记 3 / comment 证据 0 /
三者皆无 1 / merged 未 closed 且无登记(需人工) 17 / 未开工或未合并 0（不计缺口）/ 其它需人工 5；
FAIL 卡：openmir2-gm-batch-5-guild-castle；需人工 22 张…
```

**批后**（`generated_ts=1792769375`，2026-10-23T15:29:35Z，同 commit）：

```
R 卡 192 张：已完成 10 / window_required 121 / whitelist 44 / 已裁定登记 3 / comment 证据 0 /
三者皆无 1 / merged 未 closed 且无登记(需人工) 8 / 未开工或未合并 0（不计缺口）/ 其它需人工 5；
FAIL 卡：openmir2-gm-batch-5-guild-castle；需人工 13 张…
```

逐卡复核（`conditions[id=2].evidence.merged_open_hygiene`）：9 张 `category=whitelist_t`。

**本批 10 张（按 `last_status_ts` 升序；第 4 行是 comment-only）**：

| T | 卡 | status oid（登记行引用） | last_status_ts |
| --- | --- | --- | ---: |
| T-254 | `openmir2-search-target-clear-target` | `d63ea986b23a46d05471cc87d9e7625245976034` | 1792340591 |
| T-255 | `openmir2-attacktarget-overrides` | `fbfe582628678bb7998ff96348521aa443670d69` | 1792346571 |
| T-256 | `openmir2-persist-single-sided-columns` | `30c4ae0f0975169d4c3ea19e8874d95400d34d27` | 1792346580 |
| （comment） | `openmir2-gate-gmclose-race` | `c77d9772a1a3d1fb469946f22f36bd3c437890e9` | 1792499332 |
| T-262 | `openmir2-db-error-reply` | `4685a4bc936721c163fb4a42b3833d8c5380587f` | 1792346589 |
| T-257 | `openmir2-save-misc-gaps` | `08ae888f48600f8dd57cffd4c4b1df89ce1abdc0` | 1792346598 |
| T-258 | `openmir2-mapcell-attributes-loading` | `4dda39d08ec20346f3ff62f80046ebc3ae8a9d03` | 1792714263 |
| T-259 | `openmir2-login-notice-clienttick` | `46b350f5cc51e6a7b046f24a769ba3a24e785e95` | 1792714285 |
| T-260 | `openmir2-spell-shared-tail` | `f9bc711a2b9cf59550223f79e50f6a4681e1dad1` | 1792714288 |
| T-261 | `openmir2-sm-additem-tail` | `8b252336efaca93c1394fd646cbb69131fe01ab3` | 1792714406 |

> T-262 在 ts 序里位于 T-256 与 T-257 之间；号码按取号顺序（comment-only 那张先跳过）分配，故 T 序与 ts 序有这一处交叉。

---

## 2. 逐卡：窗口判据与可复跑命令

| T | 窗口判据（该卡 `status.work` 原文摘要） | 场景 / 命令 | 实跑 |
| --- | --- | --- | --- |
| T-254 | `search-target-clear-target`：「① 真机窗口；② 其余折入版调用点未扩大范围。」 | **场景待建**：plan §1 第 37 行「新：`world_observe` / combat」；未指定承接卡 | 无（不写跑不通的命令） |
| T-255 | `attacktarget-overrides`：「真机窗口（82/119/130/91/107/105）；114/117/118/215 不可达；`UndeadPower` 口径另卡。」 | **场景待建**：§1 第 38 行「新：`combat_hit`（82/119/130/91/107/105 race）」——现有 `combat_hit` 不能选怪 race | 无 |
| T-256 | `persist-single-sided-columns`：「真机窗口；失败移动后 `DisplayTable.dir` 未随实体（建议单卡）。」 | **场景已存在**：`login_only` + 停栈后 `db_snapshot`；`--check-scenario --scenario tools/probe/scenarios/login_only.json` | **rc=0** |
| T-262 | `db-error-reply`：「真机窗口（失败注入的 C# 对拍）。」 | **场景待建**：§1 第 263 行场景列「—；替代=离线注入 + 帧构造」——无现成 probe 场景，需 DB 失败注入夹具/构造级替代；替代未建、未指定承接卡 | 无 |
| T-257 | `save-misc-gaps`：「真机窗口；G11 传输面未移植；G7 读到一半失败未构造。」 | **场景已存在**：`login_only` + `db_snapshot`；同上命令 | **rc=0** |
| T-258 | `mapcell-attributes-loading`：「① 真机窗口（**必核**：route added=1649、刷怪位置与 RNG 流水、进图/落格/AOI/CanWalk(true)/snap）…」 | **场景已存在**：`login_only` + `db_snapshot`；`--check-scenario`；RNG 流水需双开 `MIR2_RNG_LOG` | **rc=0** |
| T-259 | `login-notice-clienttick`：「① 真机窗口（含 `SoftVersionDateEx==0` 两侧构造）；② 方案 A…；③ 8 个显式 MagIdx case 的 MP 逻辑。」 | **场景已存在**：`login_only`；`--check-scenario`；`SoftVersionDateEx==0` 两侧构造需 C# 侧等价夹具 | **rc=0** |
| T-260 | `spell-shared-tail`：「① 真机窗口（注入时钟下 656 帧序）；② T-78 剩余三偏差；③ `vmt`…在飞。」 | **场景部分已存在**：`spell_cast`（§1 第 23 行「新：R5 spell_cast」），`--check-scenario` **rc=0**；**656 帧序/注入时钟不在该场景内** ⇒ 待建，未指定承接卡 | **rc=0**（场景本身） |
| T-261 | `sm-additem-tail`：「真机窗口 + T-79/T-80 登记项。」 | **场景已存在**：`login_only`；`--check-scenario`；判据=T-79/T-80 帧尾逐字节 | **rc=0** |

**命令说明**：5 张用 `login_only`（`check-scenario` rc=0）、1 张用 `spell_cast`（rc=0）；其余 3 张（T-254/255/262）场景/夹具未建，**没有写任何命令**。本批未用 `combat_hit`，**无无关 `--var`**。

---

## 3. 未登记 1 张（无窗口判据，已发 comment）

`openmir2-gate-gmclose-race`：

- status `c77d9772a1a3d1fb469946f22f36bd3c437890e9` 原文：「已合入 master（c83a9ae24，tier=R）。**无待窗口项**；按 R 级规则不 closed，观察点=长跑下控制帧 flush」。
- review `8a2773d26e08c44d442feb3db8e24fe7d28de2ae`：`approve` + `window_required: false`。
- 记账 comment `d7630aab349497b0b366b4d274d2d383e127b410`：建议 owner/coordinator 复核后改 `closed`。
- 本批已发 comment **`9aae1e697fcf16f3a605cd3523fbc7c5f25f8f47`**，请 owner 二选一：① 按 review 关闭；② 把「长跑控制帧 flush」写成可判窗口项（时长/帧序/时刻口径）后再登记。**未改状态**。

---

## 4. 显式顺延 3 张（延续批 3 规则）

| 卡 | 既有 comment | 理由 |
| --- | --- | --- |
| `enter-map-cell-rejected-vs-oracle` | `da8a172a99d152a26181371964334bbf997de825` | 无窗口判据（批 1） |
| `run-gap-checkdupobj` | `8515c9d0cca824e1c5888fb8acf19cb198c1f5c` | 需 ≥3 会话（批 2） |
| `disappear-recipients-gridscan` | `fea2fbd9962ca0b0a7630cde4ce549ab8b00a148` | 需第二观察者（批 2） |

这 3 张仍计入 `merged 未 closed 且无登记`，等 owner 裁决；本批**不重复登记**（避免同卡多 T 行）。

---

## 5. 牙（删一行 ⇒ 回到 need_human）

删掉 `| T-258 |` 整行（临时），复算（`generated_ts=1792769401`）：

```
whitelist 44→43 / merged 未 closed 且无登记 8→9 / 需人工 13→14
need_items 重现：{"condition":2,"item":"openmir2-mapcell-attributes-loading",
  "reason":"merge_result merged=true 但 board 状态=in-progress（非 closed）且无显式登记；…"}
```

逐字节还原后 `sha256(mir2-rs/tests/parity/whitelist.md)`：
`b0796dff5f4e1cd1676278192657ff348a2e7abaa2d8cf4b4416e3085af2721b`（= 提交树 blob）。

---

## 6. 线程父指针订正（自查）

本批第一条 status `84c530aa6b673bc28f3d53703dc8421f5329fa1e` 的 `parent` 误取本人批 3 status
`47851e8d`（与 review `05e2fa8a` 分叉）；已在**同一条链**上按当前 head `f6e57782` 重挂认领
`b073d5d2619c7bd67814b7ad9d953166c4ab9036`，并在 body 写明 `84c530aa` 作废（内容相同）。
后续 patch/status 以 `b073d5d2` 为父。**单写入者、无并发写**，不影响本批证据。

---

## 7. 门禁（实跑）

| 门禁 | 结果 |
| --- | --- |
| `python -I tools/whitelist/t_reservation_gate.py check` | **GREEN**：`rows=246 whitelist=246 live=55 landed=191 malformed=0 wl_dups=0 max=T-262` |
| `python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit` | **GREEN**（无重复编号；token 解析唯一） |
| `python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` | **ALL PASS**（538 条锚） |
| `python -I tools/hygiene/conflict_marker_gate.py` | **0 markers** |

---

## 8. 边界与残留

- 写集仅三文件；未改 `approved_by`；未动任何卡片 status（本批 1 条 comment 属说明性）。
- 未起栈、未连库、未占端口；未合并、未自签 review、未删 worktree。
- 剩余 **8 张**（3 张已 comment 顺延 + 1 张无窗口判据 + 4 张未开班），下一批仍是本卡。
