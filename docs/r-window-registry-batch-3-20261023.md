# R 卡窗口登记 B2 批 3（T-244..T-253，2026-10-23）

> 卡：`openmir2-r-window-registry-batch`（issue `89dc6b48`，第 3 批，tier=L）
> 产出：`openmir2-worker-66`（worktree `wt-worker-66`，branch `card/r-window-registry-batch`）
> 基线：`walgit/master` = `ee292359b`（批 1 T-227..T-235、批 2 T-236..T-243 已在其中）
> 写集：`mir2-rs/tests/parity/whitelist.md`（+批 3 节）、`mir2-rs/tests/parity/T-RESERVATIONS.md`（+10 行预约）、本文件。
> 未改 `approved_by`、未动任何卡片 status。

---

## 0. 结论

- 名单**现取**：`goal_gate --json` 的「merged 未 closed 且无登记」= **27** 张（`generated_ts=1792768506`，15:15:06Z，commit `ee292359b`）。
- 按 `last_status_ts` 升序取最旧：最旧 3 张已由批 1/批 2 发过 comment（无窗口判据 / 需多会话）⇒ 顺延；取接下来 **10 张**。
- **10 张全部登记**（T-244..T-253），**本批无 comment-only 项**。
- 复算（同 commit，`generated_ts=1792768713`，15:18:33Z）：`whitelist 25→35`、`merged 未 closed 且无登记 27→17`、10 张全部 `whitelist_t`。
- 取号：10 次 `python -I tools/whitelist/next_t.py --card <卡 id>` ⇒ T-244..T-253（`appended=True journal=on remote=fresh`）。

---

## 1. 名单现取（命令 + 原文）

```sh
python -I tools/goal/goal_gate.py --json > /tmp/goal-before.json
# 过滤：condition==2 且 reason 以 "merge_result merged=true" 开头的条目
#      （另 5 条 "board 未列出该卡" 计入「其它需人工」，不在本批口径）
# 排序：need_items[].last_status_ts 升序
```

**批前 detail 原文**（`generated_ts=1792768506`，2026-10-23T15:15:06Z，commit `ee292359b`）：

```
R 卡 192 张：已完成 10 / window_required 121 / whitelist 25 / 已裁定登记 3 / comment 证据 0 /
三者皆无 1 / merged 未 closed 且无登记(需人工) 27 / 未开工或未合并 0（不计缺口）/ 其它需人工 5；
FAIL 卡：openmir2-gm-batch-5-guild-castle；需人工 32 张（按 last_status_ts 最旧优先，见 JSON need_items）；
merged 未 closed 但已凭登记 PASS：141 张（卫生清单…）
```

**批后 detail 原文**（`generated_ts=1792768713`，2026-10-23T15:18:33Z，同 commit）：

```
R 卡 192 张：已完成 10 / window_required 121 / whitelist 35 / 已裁定登记 3 / comment 证据 0 /
三者皆无 1 / merged 未 closed 且无登记(需人工) 17 / 未开工或未合并 0（不计缺口）/ 其它需人工 5；
FAIL 卡：openmir2-gm-batch-5-guild-castle；需人工 22 张…
```

逐卡复核（`conditions[id=2].evidence.merged_open_hygiene`）：10 张的 `category` 全部为 `whitelist_t`。

**本批 10 张（按 `last_status_ts` 升序）**：

| T | 卡 | status oid（登记行引用） | last_status_ts |
| --- | --- | --- | ---: |
| T-244 | `openmir2-run-gap-mb1` | `12d3f1ea6c0d1b595fd87c15a6841fe2bb645be9` | 1792318300 |
| T-245 | `openmir2-monster-view-throttle` | `6764b63826f159b7f6d99aa39ebd4ea2a049d7f7` | 1792320248 |
| T-246 | `openmir2-persist-create-rows` | `9b0ff6894ea191f2e6a077f2ab82d9532c6c75eb` | 1792327508 |
| T-247 | `openmir2-persist-useitems-wiring` | `f7a2a1cd84f0118db26a03f9752d5159e7c2bb0f` | 1792329879 |
| T-248 | `openmir2-persist-item-attr-and-gold` | `e9e732ee06ee5ed4dc9854e07325bd9e437a71a7` | 1792331816 |
| T-249 | `openmir2-run-gap-mb3` | `94d5a5375e377fc51d63d4cfbe35c431e7b4229f` | 1792332963 |
| T-250 | `openmir2-save-on-shutdown` | `e06df3ce4f4a1c86e671753a17dd113f83bf9d70` | 1792336074 |
| T-251 | `openmir2-rm-refmessage-delay-layer` | `8f965cf0ea68b761d7058ecfd118b56c576483bf` | 1792337140 |
| T-252 | `openmir2-active-seek-race-missing` | `0be96efceadc743ad6daba1f6b454bf6948f657f` | 1792340567 |
| T-253 | `openmir2-proper-target-unfolded` | `70779c78cf731407aac8ac68ffb005eebb15c43c` | 1792340578 |

---

## 2. 逐卡：窗口判据与可复跑命令

| T | 卡 / 窗口判据（该卡 `status.work` 原文摘要） | 场景 / 可复跑命令 | 实跑 |
| --- | --- | --- | --- |
| T-244 | `run-gap-mb1`：「① **真机窗口**（race 101/95 各 ≥1、R38 抽样）；② `CellType.Event` 未落格；③ 通用怪 `RM_HIT` 未接；④ `MeltStoneAll` 的 102 属 MB-3；⑤ `EventProcessor` 20ms 节拍。」 | **场景待建**：window-batch-plan §1 第 30 行「新：`world_observe` race 101/95」；`tools/probe/scenarios/world_observe.json` 不存在，未指定承接卡 ⇒ 需 owner/coordinator 派场景卡 | 无（不写跑不通的命令） |
| T-245 | `monster-view-throttle`：「① **真机窗口**；② GuardUnit 的 6 格/CanPassiveAttack；③ 自扫相位差 ≤1 tick；④ parity 驱动不启 AI；⑤ 多玩家投递序。」 | **场景待建**：§1 第 31 行「新：`world_observe`」；项 ⑤ 另需多会话 | 无 |
| T-246 | `persist-create-rows`：「① 真机窗口（建角/兜底建角往返）；② `name_quick`/`name_text` 分裂的同名判重（另卡）。」 | **场景已存在**：`login_only` + 停栈后 `db_snapshot`；`python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/login_only.json` | **rc=0** |
| T-247 | `persist-useitems-wiring`：「① **真机窗口**（耐久磨损/破零/存档往返）；② 破零后 oracle 收尾未接…；③ `DAMAGEARMOR` 1.2× 既有登记。」 | **场景已存在**：R7 `equip_struck_save`（§1 第 32 行）；`--check-scenario --scenario tools/probe/scenarios/equip_struck_save.json` | **rc=0** |
| T-248 | `persist-item-attr-and-gold`：「③ 真机窗口（建角/存档往返，虽实测零语句）。」 | **场景已存在**：`login_only` + `db_snapshot`；同上命令 | **rc=0** |
| T-249 | `run-gap-mb3`：「① **真机窗口**（107/105/102/112）；② `SendStruckDelayMsg` 延时面（归 `rm-refmessage-delay-layer`）；③ `flush_sculture_spawns` 排水时序；④ `GuardUnit.IsProperTarget` 的 Castle 分支。」 | **场景待建**：§1 第 33 行「新：`combat_hit` race 107/105/102/112」——现有 `combat_hit` 只能对指定格砍一刀、不能选怪 race；未指定承接卡 | 无 |
| T-250 | `save-on-shutdown`：「① **真机窗口**（`DB_SAVEHUMANRCD` 帧与落库 diff、`-Stop` graceful 日志；并裁决 D-12）；② `CTRL_CLOSE_EVENT` 5s 压缩；③ 请求排序口径；④ 命名管道无鉴权。」 | **场景已存在**：`login_only` + 停栈后 `db_snapshot`；`--check-scenario login_only` **rc=0**；`-Stop` graceful 日志需真栈（`card_window.py` 的 stop 命令） | **rc=0** |
| T-251 | `rm-refmessage-delay-layer`：「① **真机窗口**（200ms 后二次 `Struck` 的全局帧序）；② 未接的 7 参调用点…；③ 广播缓存口径。」 | **场景待建**：§1 第 34 行「新：`combat_hit`（200ms 二次 Struck）」——现有 `combat_hit` 无延时面 | 无 |
| T-252 | `active-seek-race-missing`：「① 真机窗口；② 这些 race 的 `AttackTarget` 覆写属 `attacktarget-overrides`；③ 118/130 数据不可达。」 | **场景待建**：§1 第 35 行「新：`world_observe`（9 seek race）」 | 无 |
| T-253 | `proper-target-unfolded`：「① 真机窗口（攻击已死目标的抽数流水）；② 其余折入版调用点未扩大范围。」 | **场景待建**：§1 第 36 行「新：`combat_hit`（攻击已死目标）」——现有 `combat_hit` 无「已死目标」支 | 无 |

**命令说明**：
- `login_only` / `equip_struck_save` 的 `--check-scenario` 只做离线校验（解析/必填/placement 声明），**不连栈**；上表 4 个 rc=0 均为本卡实跑。
- `combat_hit` 的 4 个 `--var`（`target_x`/`target_y`/`target_xy`/`hit_dir`）当前**全部在 `required` 里**（`scenario-error` 实测缺 `target_xy` 即 rc=2）；本批没有任何场景用到它，也没有多余 `--var`。
- 场景待建的 6 张**没有**写任何命令（避免「跑不通的命令」）。

---

## 3. 顺延的 3 张（已由批 1/批 2 发 comment，不重复登记）

| 卡 | status oid | 既有 comment | 理由 |
| --- | --- | --- | --- |
| `enter-map-cell-rejected-vs-oracle` | `8c5f59e1654893f8a52e26ade65785d376129ae6` | `da8a172a99d152a26181371964334bbf997de825`（worker-69，批 1） | 无窗口判据（`CellRejected` 部署内不可达），见批 1 |
| `run-gap-checkdupobj` | `d758a92ff637e4a44a73e3d539335b50a6976df7` | `8515c9d0cca824e1c5888fb8acf19cb198c1f5c`（批 2） | 需 ≥3 会话 |
| `disappear-recipients-gridscan` | `2f85770698c56f5ead5ab9762409dfd45db36599` | `fea2fbd9962ca0b0a7630cde4ce549ab8b00a148`（批 2） | 需第二观察者 |

这 3 张仍计入 `merged 未 closed 且无登记`（comment 不会让机判翻绿），等 owner 裁决多会话探针/构造级替代。

---

## 4. 牙（删一行 ⇒ 回到 need_human）

删掉 `| T-249 |` 整行（临时），复算（`generated_ts=1792768750`）：

```
whitelist 35→34 / merged 未 closed 且无登记 17→18 / 需人工 22→23
need_items 重新出现：{"condition":2,"item":"openmir2-run-gap-mb3",
  "reason":"merge_result merged=true 但 board 状态=in-progress（非 closed）且无显式登记；…"}
```

逐字节还原后 `sha256(mir2-rs/tests/parity/whitelist.md)`：
`c287868c726acf787ffe941d60ddbda47759addb06623a5b43b5bb2713f613bf`（= 提交树 blob）。
复算（`generated_ts=1792768773`）回到 `whitelist 35 / merged 17`。

---

## 5. 门禁（实跑）

| 门禁 | 结果 |
| --- | --- |
| `python -I tools/whitelist/t_reservation_gate.py check` | **GREEN**：`rows=237 whitelist=237 live=46 landed=191 malformed=0 wl_dups=0 max=T-253` |
| `python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit` | **GREEN**：`entries=237 duplicates=none tokens=10` |
| `python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` | **ALL PASS**（538 条锚） |
| `python -I tools/hygiene/conflict_marker_gate.py` | **0 markers** |

---

## 6. 边界与残留

- 只动写集三文件；未改任何卡片 status（无 comment——本批无需裁决项）、未改 `approved_by`。
- 未起栈、未连库、未占端口；未合并、未自签 review、未删 worktree。
- 剩余 **17 张**（下一批仍是本卡；名单每批现取）。
