# R 卡窗口登记——B2 批 2（卡 `openmir2-r-window-registry-batch`）

- worktree `E:/tmp/wt-worker-69`，分支 `card/r-window-registry-batch`（批 1 已合 `19c59598a`，本批从该 master 起）；tier = **L**。
- 本批名单：`last_status_ts` 升序最旧 **10 张**；**登记 8 张（T-236..T-243）+ 2 张需多会话/第二观察者（只发 comment）**。
- 名单来源（现取）：`python -I tools/goal/goal_gate.py --json`，2026-10-23T14:58:31Z，`generated_ts=1792767532`，commit `19c59598a`。

---

## 0. 结论

| 指标 | 前（`1792767532`） | 后（`1792767684`） |
|---|---:|---:|
| 条件② `whitelist` 桶 | 17 | **25** |
| 条件② `merged 未 closed 且无登记(需人工)` | **35** | **27** |
| 条件② `需人工` 总数 | 40 | 32 |

- 本批 8 张全部转入 **`whitelist_t`**（`strong_t` 恰为 T-236..T-243）；2 张（`checkdupobj` / `disappear-recipients-gridscan`）**有窗口判据但需多会话** ⇒ 不登记、发 comment，仍在桶里（有说明）。
- **未改任何 `approved_by`、未动任何卡片状态**；`T-*` 全部走 `python -I tools/whitelist/next_t.py --card <卡>`。

---

## 1. 名单现取（原文）

**前**（`generated_ts=1792767532`）条件② `detail` 原文：

```text
R 卡 192 张：已完成 10 / window_required 121 / whitelist 17 / 已裁定登记 3 / comment 证据 0 / 三者皆无 1 /
merged 未 closed 且无登记(需人工) 35 / 未开工或未合并 0（不计缺口）/ 其它需人工 5；
FAIL 卡：openmir2-gm-batch-5-guild-castle；需人工 40 张（按 last_status_ts 最旧优先，见 JSON need_items）；…
```

**后**（`generated_ts=1792767684`）条件② `detail` 原文：

```text
R 卡 192 张：已完成 10 / window_required 121 / whitelist 25 / 已裁定登记 3 / comment 证据 0 / 三者皆无 1 /
merged 未 closed 且无登记(需人工) 27 / 未开工或未合并 0（不计缺口）/ 其它需人工 5；
FAIL 卡：openmir2-gm-batch-5-guild-castle；需人工 32 张（按 last_status_ts 最旧优先，见 JSON need_items）；…
```

> 取数：`need_items` 中 reason 以 `merge_result merged=true` 开头的条目按 `last_status_ts` 升序取前 10
> （`openmir2-enter-map-cell-rejected-vs-oracle` 是其中最旧的一张，但它在批 1 已按「无窗口判据」发过 comment，
> 本批顺延到下一张，故实际取到的是表 2 的 10 张）。

---

## 2. 逐卡登记表（T / 卡 / 该卡自己的 status oid / 场景与可跑命令）

| T | 卡 | 窗口项（该卡 status 原文，出处 oid） | 场景 / 可跑命令 |
|---|---|---|---|
| **T-236** | `openmir2-player-searchviewrange-cadence` | 「余量：① **真机窗口**——首扫约 1.2s、之后每 1.2s；② 非玩家扫描节拍仍每 tick（另卡）；③ `RM_DISAPPEAR` 收件人近似（误差窗口随节流放大）——应改格扫或窗口验证」— `0550c8c490f4a5e84725ea07bbf2938839ed931b` | **场景已存在**：`login_only`（仅项 ① 可验；项 ③ 需第二观察者）。离线实跑：`python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/login_only.json` ⇒ **rc=0** |
| **T-237** | `openmir2-run-gap-player-run-quantification` | 「余量：① **裁定证据**——按 §3 方案取数（B1 静置压血看 53 填充间隔：50/s≈6.0s vs 40/s≈7.5s）；…③ A2 时钟注入需 owner 裁决 O3（无对应 T-*）；④ A1 harness 需先 spike」— `0a5391375242acfe358ef619c838a016ea1a395b` | **场景待建**：需 A1 harness spike + owner 裁决 O3（时钟注入）；未指定承接卡 ⇒ 需 owner/coordinator 派。复核：`goal_gate --json` |
| **T-238** | `openmir2-run-gap-monster-state-expiry` | 「余量：① **真机窗口**（CrazyMode 色变 0xF9 / RefNameColor 帧面 / FireHit-TwinHit 到期消息）；…」— `0add6624f0bb55f58f85474d70ea012806543c15` | **场景待建**：window-batch-plan §1 第 24 行「新：world_observe 60–180s」，未指定承接卡 |
| **T-239** | `openmir2-run-gap-corpse-ghost-revive` | 「余量：① **真机窗口**（3 分钟变尸 / 可复活支 ZenTime-20s / 5 分钟移除 / 两帧 RM_TURN）；…」— `b37880c0ef620e79bf7b928aae8272380cd9b836` | **场景待建**：§1 第 25 行「新：world_observe ≥300s」，未指定承接卡 |
| **T-240** | `openmir2-movetomovingobject-quirk` | 「余量：① **真机窗口**（多对象格移动时的源/目标格身份序列）；…③ 该怪癖同样影响走/跑/SpaceMove/召回——…窗口对拍需覆盖移动路径」— `aceda41633a8800f68f047a8715f069b89e18a63` | **场景待建**：§1 第 26 行「新：多对象同格移动」，未指定承接卡 |
| **T-241** | `openmir2-run-gap-monster-master-follow` | 「余量：① **真机窗口**（异图/超距/SlaveRelax 两侧流水）；② 玩家主的 `Dir` 供给另卡；…」— `15847cd8fe83a24092ecb809185073a3a37dc920` | **场景待建**：§1 第 27 行「新：world_observe（宝宝异图/超距）」，未指定承接卡 |
| **T-242** | `openmir2-run-gap-monster-specials` | 「余量：① **真机窗口**（同 tick 产仔抽数相对次序、`SM_HIT` 帧面、Clone 扣 MP）；…」— `ece643c506a86f5b9dabf1fa6ec50e059062ab88` | **场景部分已存在**：`combat_hit`（`--check-scenario … --var target_x=30 --var target_y=32 --var target_xy=15204608 --var hit_dir=4` **rc=0**，实跑）；**产仔观察待建**（§1 第 28 行） |
| **T-243** | `openmir2-run-gap-pr1-head-blocking` | 「余量：① 真机窗口；② 跨 actor 泵序（首次入队序 vs 实体序）；③ 宿主 `pending_mag_struck` 排水序与 7 参 `SendStruckDelayMsg` 生产者（MB-9）」— `4a92013550ae1b4fc69cbd1e5518019e12a2bab4` | **场景待建**：§1 第 29 行「新：队列序观察」，未指定承接卡 |

---

## 3. 未登记 2 张（需多会话 / 第二观察者 ⇒ 单会话探针跑不了，只发 comment）

| 卡 | 该卡 status 原文（oid） | 为什么单会话跑不了 | comment oid |
|---|---|---|---|
| `openmir2-run-gap-checkdupobj` | 「余量：① **真机窗口**——3 人同格推挤的源/目标格身份序列 + AOI 相位（1000ms 口径）」— `d758a92ff637e4a44a73e3d539335b50a6976df7` | 需 **≥3 会话**同格推挤；`card_window` 每侧只起一个 probe | `8515c9d00cca824e1c5888fb8acf19cb198c1f5c` |
| `openmir2-disappear-recipients-gridscan` | 「余量：① **真机窗口**（节流窗口内进入 ±12 的观察者、离世广播与 AOI/移动帧的全局次序）」— `2f85770698c56f5ead5ab9762409dfd45db36599` | 需**第二观察者**在节流窗口内进入 ±12 | `fea2fbd9962ca0b0a7630cde4ce549ab8b00a148` |

两张都在 comment 里请 owner 裁决：**等多会话探针卡 `openmir2-multi-session-probe` 落地后补窗口，或按构造级替代后 closed**。
**未改状态**。

---

## 4. reviewer 的两条 minor（本批已体现）

### 4.1 拒绝项（`enter-map-cell-rejected-vs-oracle`）的不可达理由补一跳
批 1 的理由停在「当前生产无 `switch_map` 调用者」。**补一跳**（本次逐行核过）：

1. **`switch_map_space` 有 2 个生产调用点**（`crates/world/src/world.rs:4601`、`:4681`），但它们的 `Err` 支**在真机不会发生**：
   落点先经 `space_move_get_rand_xy`（`crates/world/src/space_move.rs:26`，带目标图 `get_cell_info(..).1` 的可走闭包）预检，
   取不到落点就 `let Some((fx, fy)) = found else { return out; }` 提前返回 ⇒ 走到 `switch_map_space` 时目标格已可走。
2. **`switch_map` 除测试外另有一处生产调用**（`world.rs:7288` 的 MapRoute / `EnterAnotherMap` 落地支）——
   这一处与 review 原话「调用点全在测试」**不一致**（本次 grep 实测）。但它的 `CellRejected` 只在**目标格阻挡**时触发，
   而**部署 route 表的所有目标格都是可走格**：本次扫描 `Envir/MapInfo.txt` 的 `->` 行 **1560 条**，
   目标格实测 **1556 条可走（OK）**、4 条目标图文件缺失（`NOMAP`：`M100B`/`M100C`/`0123A`，走 `NoSuchMap` 这一 Rust 独有护栏，不是 `CellRejected`）⇒ **`CellRejected` 在部署里不可达**。
3. ⇒ 该卡的「切图失败路径」窗口**在真机不可达**；批 1 的「不登记 + comment」结论不变，理由现在有两跳证据。
   复算：`grep -n '\.switch_map(' mir2-rs/crates/world/src/world.rs` + 上述 route 扫描（见本节脚本口径）。

### 4.2 「一行登记两张卡」会被 `CARD_ID_RE` 漏关联
`goal_gate.py:62` 的 `CARD_ID_RE = re.compile(r"openmir2-[A-Za-z0-9][A-Za-z0-9._-]*")` 是**整行匹配**：
一行里出现两个卡 id 时，两卡都会关联到该行（不是「漏」而是「多关联」；反之若把第二个卡 id 写成别的形式，
第二张卡就**漏关联**）。本批的落地口径：
- **一行只登记一张卡**（T-236..T-243 每行只有一个卡 id）；
- 行内**跨卡引用不带 `openmir2-` 前缀**（如 `mapcell-attributes-loading`、`multi-session-probe`），避免制造错误的强关联
  ——批 1 实测踩到过：首版 T-229/T-230 的引文点了别的卡全名，导致 `mapcell-attributes-loading` 被误算 `whitelist_t`、bucket 少算 1。

---

## 5. 牙（删 T-238 ⇒ 该卡回 need_human，bucket +1）

| 状态 | `generated_ts` | `merged 未 closed 且无登记` | `openmir2-run-gap-monster-state-expiry` |
|---|---|---:|---|
| 删 `T-238` 行后 | 1792767714 | **28** | **need_human / merged_not_closed**（`strong_t=[]`） |
| 逐字节还原后 | 1792767732 | **27** | **pass / whitelist_t `['T-238']`** |

`sha256sum mir2-rs/tests/parity/whitelist.md` = **`8b8d9ef3e7eae92bc3dc05a1b45e6c94d81e5f9942c07461dc8e27761f299297`**（提交树同值）。

---

## 6. 门禁（实跑）

| 命令 | 结果 |
|---|---|
| `python -I tools/whitelist/t_reservation_gate.py check` | **0（GREEN：预约表 227 行 / whitelist 227 号，max=T-243）** |
| `python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit` | **0（WHITELIST_REGISTRY GREEN）** |
| `python -I -B mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` | **0（RESULT: ALL PASS，537 条锚）** |
| `python -I tools/hygiene/conflict_marker_gate.py` | **0（0 markers / 7405 tracked files）** |

---

## 7. 残留

1. 剩余 B2：本批后「merged 未 closed 且无登记」= **27**（其中 2 张已有 comment、1 张为无窗口判据），下一批仍按同口径 ≤10 张继续。
2. 依赖未建场景的 6 张（T-237..T-241、T-243）都**没有承接卡**——需要 owner/coordinator 派场景卡；T-242 的产仔观察也待建。
3. 2 张多会话卡（`checkdupobj` / `disappear-recipients-gridscan`）的解冻依赖 `openmir2-multi-session-probe`。
4. 本批只动 `whitelist.md`（+25 行新节）、`T-RESERVATIONS.md`（+8 行预约）、本文件；未改任何 `approved_by`、未动卡片状态。
