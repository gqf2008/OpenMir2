# R 卡窗口登记——B2 批 1（卡 `openmir2-r-window-registry-batch`）

- worktree `E:/tmp/wt-worker-69`，分支 `card/r-window-registry-batch`；基线 `walgit/master` = `74f0fbb0d`。
- tier = **L**：只补 `mir2-rs/tests/parity/whitelist.md` 的 `T-*` 登记 + 本文件；零代码、零判据。
- 本批 **10 张**名单（B2 桶按 `last_status_ts` 最旧 10 张）：**登记 9 张（T-227..T-235）+ 1 张无窗口判据（只发 comment，不登记）**。

---

## 0. 结论

| 指标 | 前（`generated_ts=1792766674`） | 后（`generated_ts=1792766849`） |
|---|---|---|
| 条件② `whitelist` 桶 | 6 | **15** |
| 条件② `merged 未 closed 且无登记(需人工)` | **44** | **35** |
| 条件② `需人工` 总数 | 49 | 40 |

- 本批 9 张全部从「merged 未 closed 且无登记」转入 **`whitelist_t`**（`goal_gate` 的 `strong_t` 点名本卡）。
- 第 10 张（`openmir2-enter-map-cell-rejected-vs-oracle`）**没有可跑窗口判据**（生产无 `switch_map` 调用者 ⇒ 真机不可达）⇒ 按卡面规则**不登记**，改在卡片线程发 comment 说明（`da8a172a…`），条件② 里它仍是 `need_human`（有 comment 说明）。
- **未改任何 `approved_by`、未动任何卡片状态**；`T-*` 取自 `python -I tools/whitelist/next_t.py --card <卡>`（9 次调用，返回 T-227..T-235）。

---

## 1. 名单现取（命令 + 原文，不许手抄）

```sh
python -I tools/goal/goal_gate.py --json
```

**前**（2026-10-23T14:44:14Z 启动，`generated_ts=1792766674`，commit `74f0fbb0d`）条件② `detail` 原文：

```text
R 卡 192 张：已完成 10 / window_required 121 / whitelist 6 / 已裁定登记 3 / comment 证据 0 / 三者皆无 3 /
merged 未 closed 且无登记(需人工) 44 / 未开工或未合并 0（不计缺口）/ 其它需人工 5；
FAIL 卡：openmir2-gm-batch-5-guild-castle, openmir2-monster-attack-cadence-lead, openmir2-oracle-humitems-align；
需人工 49 张（按 last_status_ts 最旧优先，见 JSON need_items）；
merged 未 closed 但已凭登记 PASS：124 张（…登记型 PASS ≠ 已有真机证据）
```

**后**（2026-10-23T14:46:10Z 启动，`generated_ts=1792766849`，同 commit）条件② `detail` 原文：

```text
R 卡 192 张：已完成 10 / window_required 121 / whitelist 15 / 已裁定登记 3 / comment 证据 0 / 三者皆无 3 /
merged 未 closed 且无登记(需人工) 35 / 未开工或未合并 0（不计缺口）/ 其它需人工 5；
FAIL 卡：openmir2-gm-batch-5-guild-castle, openmir2-monster-attack-cadence-lead, openmir2-oracle-humitems-align；
需人工 40 张（按 last_status_ts 最旧优先，见 JSON need_items）；
merged 未 closed 但已凭登记 PASS：134 张（…）
```

> 名单取自 JSON 的 `need_items`（reason 以 `merge_result merged=true 但 board 状态=in-progress` 开头的 44 条），
> 按 `last_status_ts` 升序取前 10 —— 与上面的「最旧优先」口径一致。

---

## 2. 逐卡登记表（T 号 / 卡 / 窗口项原文 + 出处 oid / 场景与可跑命令）

> 每行的「窗口项」都是**该卡自己的 `status` 原文**（引号内逐字），出处 oid 可在 `walgit collab thread <卡>` 复核。
> 文档里的引文保留卡的全名（本文件不被 `goal_gate` 解析）；**whitelist 行内**的跨卡引用才去掉 `openmir2-` 前缀（见 §6.1）。
> **可跑命令都实跑过**：`python -I tools/goal/goal_gate.py --json`（全批，必跑）；`login_only` 场景的
> `--check-scenario` 实跑 **rc=0**。依赖未建场景的，写明「场景待建 + 承接」——不写跑不通的命令。

| T | 卡 | 窗口项（该卡 status 原文，出处 oid） | 场景 / 可跑命令 |
|---|---|---|---|
| **T-227** | `openmir2-buff-producer-wiring` | 「余量=真机窗口项 + 新开片：四块 snap 改用忠实镜像 `cetin_near_xy`（复验新发现的格子层差异）+ 补窗口宽度上界牙」— `status 40ffdbb80c95a9a528f7b470c8d40985b954fa6f` | **场景待建**：window-batch-plan §1 第 16 行「新：snap/宠物场景」，**未指定承接卡** ⇒ 需 owner/coordinator 派场景卡。复核：`goal_gate --json`（本卡落 `whitelist_t`） |
| **T-228** | `openmir2-default-arm-spell-surface` | 「余量：① 真机逐帧对拍（窗口项）；② 效果体层未接（MP 扣减 + `RM_HEALTHSPELLCHANGED` 先于 RM_SPELL ⇒ 仍缺一帧…）」— `status baaab5e99f69f51d7d927149ee2511922dd40fb2` | **场景待建**：R5 `spell_cast`，承接卡 `openmir2-scenario-spell-cast`（needs-review，分支 `card/scenario-spell-cast`，**未合**）。复核：`goal_gate --json` |
| **T-229** | `openmir2-snap-cell-fidelity` | 「余量：① 真机窗口项；② 两个前置（`.map` 格子属性未灌进 `world.maps`；`apply_structure_events` 无生产调用方）——卡 `openmir2-mapcell-attributes-loading`」— `status dca13e43a3710c32135c93296a7811244e7ba133` | **场景待建**：window-batch-plan §1 第 18 行「新：mapcell + 异图 spell」；前置卡 `openmir2-mapcell-attributes-loading`。复核：`goal_gate --json` |
| **T-230** | `openmir2-recall-slave-shift-live` | 「余量：① 真机窗口（W0 §2e：两侧 RNG 账本 `VERDICT IDENTICAL` + 回召落点同坐标）；② 同窗登记的 `merchant_run → map_random_move` 实为**真缺口**（oracle 移位环继续抽数）⇒ 卡 `openmir2-map-random-move-shift-loop`」— `status f2cbc88069347a07ae462f1a27dd03ad94be5eda` | **场景待建**：R5 `spell_cast`（回召臂），承接卡 `openmir2-scenario-spell-cast`（未合）；需宝宝 + 异图。复核：`goal_gate --json` |
| **T-231** | `openmir2-map-random-move-shift-loop` | 「余量：① 真机窗口（商人到点移动：RNG 流水 + 落点）；② 登记未做：201 轮回滚的 `DeleteFromMap`/`AddMapObject` 语义（现只 return）、落格 `CellType`、`SpaceMove` 出帧面」— `status 483a6375cf31823c3c9eb536a08fa3bc00c2348a` | **场景待建**：window-batch-plan §1 第 20 行「新：商人到点移动」，**未指定承接卡** ⇒ 需 owner/coordinator 派场景卡。复核：`goal_gate --json` |
| **T-232** | `openmir2-enter-shift-loop` | 「余量：① 真机窗口（墙格出生登录：RNG 流水 + 落点；默认零抽）；② `HomeMap` 不可解析分支…（当前不可达，已登记）」— `status 3ef3520d67321804322927647b5c9f9b7d06f824` | **场景已存在**：`login_only`。离线实跑：`python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/login_only.json` ⇒ **rc=0**；真机窗口需 owner 授权的私栈窗口（`card_window.py --profile alt`） |
| **T-233** | `openmir2-magic-8-21-wiring` | 「余量：① 真机窗口（37 的 `Random(20)`/`Random(2)` 流水 + 逐格 `SM_BACKSTEP`，可与 8 同场景）；…③ 魔号 8 的逐格 `SM_BACKSTEP` 邻域、魔号 21 的 806 ±12 含自己 + 换图落点」— `status 183900e96905aaf3791449804b52c30b6424da3a` | **场景待建**：R5 `spell_cast`，承接卡 `openmir2-scenario-spell-cast`（未合）。复核：`goal_gate --json` |
| **T-234** | `openmir2-spell-mp-wrapper` | 「余量：① 真机窗口（失败回帧 + MP/53/尾时序）；② 6/13/14/15/16/18/19/36 的护身符/失败门未建…」— `status a4823d889c88cdd53cdbb4cf733875afee16e8f1` | **场景待建**：R5 `spell_cast`，承接卡 `openmir2-scenario-spell-cast`（未合）。复核：`goal_gate --json` |
| **T-235** | `openmir2-aoi-verify-maptime` | 「余量：① **真机窗口**——同缩窗场景需验 vis>=1、nearest<=vr、capture 出 `SM_TURN(281,661)` + 配对 `SM_FEATURECHANGED`、`SM_WALK=0`（W0-4 复跑）」— `status 02cc8fb6298beb6144f3489dcc01d02bc6c6525a` | **场景已存在**：`login_only`（离线 `--check-scenario` rc=0）；真机窗口需 owner 授权的私栈窗口 |

---

## 3. 未登记 1 张（无窗口判据）

| 卡 | 该卡 status 原文 | 为什么不登记 | 处置 |
|---|---|---|---|
| `openmir2-enter-map-cell-rejected-vs-oracle` | 「余量：① 真机窗口（切图失败路径；**当前生产无 `switch_map` 调用者 ⇒ 真机不可达**）」— `status 8c5f59e1654893f8a52e26ade65785d376129ae6` | 窗口**跑不了**（生产无调用点）⇒ 按卡面规则「找不到窗口判据的卡不登记」 | 已在卡片线程发 comment `da8a172a99d152a26181371964334bbf997de825`：说明无窗口判据，建议 owner 裁决「构造级替代后 closed」或「保留 in-progress 等生产调用点」；**未改卡片状态** |

---

## 4. 牙（删一行 T-* ⇒ 条件② 重新把该卡列为需人工）

```sh
# 删掉 T-231 那一行
python - <<'EOF'
import io
p="mir2-rs/tests/parity/whitelist.md"
lines=io.open(p,encoding="utf-8").read().splitlines(keepends=True)
io.open(p,"wb").write("".join(l for l in lines if not l.startswith("| T-231 ")).encode("utf-8"))
EOF
python -I tools/goal/goal_gate.py --json
```

| 状态 | `generated_ts` | `whitelist` 桶 | `merged 未 closed 且无登记` | `openmir2-map-random-move-shift-loop` |
|---|---|---:|---:|---|
| 删 T-231 后 | 1792766891 | 14 | **36** | **need_human / merged_not_closed**（`strong_t=[]`） |
| 还原后 | 1792766909 | 15 | **35** | **pass / whitelist_t `['T-231']`** |

逐字节还原后 `sha256sum mir2-rs/tests/parity/whitelist.md` = **`de938e9ae6857c22638c62b630ff87d327229e3ffac5ff71fc06b1f651e867c1`**（提交树同值）。

---

## 5. 门禁（实跑）

| 命令 | 结果 |
|---|---|
| `python -I tools/whitelist/t_reservation_gate.py check` | **0（GREEN：预约表 217 行 / whitelist 217 号，max=T-235）** |
| `python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit` | **0（WHITELIST_REGISTRY GREEN：无重复编号；夹具 token 解析唯一或不可用）** |
| `python -I tools/hygiene/conflict_marker_gate.py` | **0（0 markers / 7400 tracked files）** |

---

## 6. 已知侧效应与口径

1. **跨卡引用不带 `openmir2-` 前缀**：`goal_gate` 把行内任何 `openmir2-*` 当作本行的卡关联。
   本批实测踩到过——T-229/T-230 的引文里点了别的卡的全名，导致 `mapcell-attributes-loading` 被错误地算作
   `whitelist_t`、bucket 少算 1（44→34 而不是 35）。已把引文里的跨卡引用改成不带前缀的写法，复算回到 44−9=35。
2. **`T-*` 行是「窗口证据未采」的登记，不是行为豁免**：每行都写「真机窗口待采 + 场景现状」，
   没有把 R 卡的窗口项写成「无害」。
3. **`T-223..T-226` 在别的 worktree 的 journal 里已被占用**（本仓 `T-RESERVATIONS.md` 尚未合并那些行）；
   `next_t.py` 通过共享 journal + 远端新鲜度检查正确跳到 **T-227**，未与之撞号。
4. 本批未改任何 `approved_by`、未动任何卡片 status（只对 1 张无窗口判据卡发了 comment）。
5. 后续批次仍是同一张卡（`openmir2-r-window-registry-batch`）：剩余 B2 约 26 张 + 三者皆无 3 张，
   按同样口径分批补；**每批 ≤10 张**。
