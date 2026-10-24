# 条件② 收口：5 张 merged_not_closed R 卡逐张裁定（2026-10-25）

- 卡：`openmir2-merged-not-closed-5-sweep`（tier **L**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；分支 `card/merged-not-closed-5-sweep`）。
- 取数时点：master `99156c2c6` @ 2026-10-25 01:04 +08；`goal_gate --json --skip-tests --skip-script` 前后各跑一次。
- 写集：5 张目标卡的线程 `status` 条目 + 本文件。**未改代码、未起栈、未连库。**

## 0. 结论

5 张 merged_not_closed R 卡逐张读线程后：**close 2 张、登记窗口项 2 张、保持 blocked 1 张**。
条件② 的 need_human 从 **5 → 1**（不是 0）：唯一剩余是 `openmir2-gm-batch-5b-cross-server`，
它有**未完成的实现工作**（依赖 G11 跨服基础设施），按裁定规则**不得**登记成窗口项、也**不得** close。

| # | 卡 | merge oid | 判定 | 本轮 status 条目 |
|---:|---|---|---|---|
| 1 | `openmir2-disappear-recipients-gridscan` | `8dea847c5` | **in-progress + `window_required=true`**（±12 收件人集合需第二观察者） | `3fe843d3b` |
| 2 | `openmir2-enter-map-cell-rejected-vs-oracle` | `ed9ab035d` + `df5a260ea` | **closed**（无新工作；生产无 `switch_map` 调用者 ⇒ 真机不可达；构造级已覆盖） | `0f0c16d69` |
| 3 | `openmir2-gate-gmclose-race` | `c83a9ae24` | **closed**（approve 显式 `window_required=false`；20 轮 relay + 2 次全量 workspace 0 失败；「长跑 flush」只是观察点） | `8da01a1cc` |
| 4 | `openmir2-gm-batch-5b-cross-server` | `99a4d30c4` | **保持 blocked**（blocked-by G11；依赖清单已合入，但 6 条命令实现未做） | `a662151e9` |
| 5 | `openmir2-run-gap-checkdupobj` | `706c95cae` | **in-progress + `window_required=true`**（3 人同格推挤 + AOI 相位需 ≥3 会话） | `cf9664831` |

## 1. 逐卡依据

### 1.1 disappear-recipients-gridscan → 窗口登记

- merge/review：`merge_result` `bf02536e8`（merged=true，oid `8dea847c5`）；approve `e23c8882b`（tier R，CONFIRMED）。
- merge 之后**无新 patch**（只有记账 comment）。
- 剩余项（status `2f8577069` 原文）：① 真机窗口（节流窗口内进入 ±12 的观察者、离世广播与 AOI/移动帧的全局次序）；
  ② 冷路径 vs oracle 500ms `VisibleHumanList` 缓存命中（`<11`）的既有偏差；③ `ObMode/FixedHideMode` 只发自己的分支。
- 定性：±12 收件人集合需要**第二个会话**（comment `fea2fbd99` / `cff88bf13` 独立确认），属「仍欠真机窗口」
  ⇒ 登记 `window_required=true` + `window_items`（登记型 pass，**不是**真机证据）。

### 1.2 enter-map-cell-rejected-vs-oracle → closed

- merge：`639707079`（`ed9ab035d`，enter_map 落格差异）+ `4a1afaff5`（`df5a260ea`，switch_map 回滚语义）。
- review：`8bb0c9568` / `a18db2ed1` 均 approve（tier R；复验者独立读 oracle 行并实测）。
- merge 后无新工作；剩余项均为**独立登记**：宿主缺 oracle 进图前移位环（另卡）、失败支帧面未建模（既有约定）、
  `route_enter` 对 switch_map 失败的分类缺口（另跟踪）、`DelFormMaped`/地图计数未建模。
- 本卡自身 status `8c5f59e16` 自述「当前生产无 `switch_map` 调用者 ⇒ 真机不可达」；构造级用例覆盖 `CellRejected` 语义。
- comment `da8a172a9`（worker-69，无窗口判据）与 `3730b07aa`（worker-24，bucket ③ 建议关闭）独立建议 closed。

### 1.3 gate-gmclose-race → closed

- merge：`d7424c4b8`（`c83a9ae24`，fast-forward）；review `8a2773d26` approve 且**显式 `window_required=false`**。
- 证据：根因证实（GM_CLOSE 入队即返回、进程退出前 writer 未 flush）；修法=控制帧 oneshot ack 同步写；
  relay 连续 20 轮 0 失败 + 全量 workspace 连续 2 次 0 失败（patch `cd0665ce3` / review 原文）。
- 本卡 status `c77d9772` 自述「无待窗口项；按 R 级规则不 closed，观察点=长跑下控制帧 flush」——「观察点」不是可判判据。
- comment `d7630aab3`（worker-43，B1）、`9aae1e697`（worker-66）、`6ace05bf3`（worker-24）三处独立建议 closed。

### 1.4 gm-batch-5b-cross-server → 保持 blocked（本轮唯一残余）

- merge：`6af73da3d`（`99a4d30c4`，**blocked=true**，写集仅 docs）；review `d3b76fba5` approve（tier R，0 findings）。
- 本卡 status `8d7abba32` 原文：「G11 就绪后按 docs §6 清单实现 ①-④、⑤ 照 T-38 走 no-op、⑥ 零 SS。**G11 就绪前不得 closed**」。
- 复核 G11：仓内 `PlanesServer` 仅出现在注释（`bins/game-svr/src/lib.rs:17505` 的 oracle 归因），**无传输实现**；
  卡板无 G11 实现卡。⇒ 本卡**确有未完成工作**（6 条命令移植 + 用例 + 怪癖牙），不是「只差窗口」。
- 按裁定规则：**不得**把「有未完成工作」的卡登记成窗口项、**不得** close ⇒ 保持 `blocked`，写明剩余项与 owner。

### 1.5 run-gap-checkdupobj → 窗口登记

- merge：`0ea102863`（`706c95cae`）；review `9e29fd527` approve（tier R；含 `MoveToMovingObject` 怪癖卡的跨卡判据）。
- merge 后无新 patch。
- 剩余项（status `d758a92ff`）：① 真机窗口——3 人同格推挤的源/目标格身份序列 + AOI 相位（1000ms 口径）；
  ② `player_run` 组在五批 trace 中未出现，需活栈覆盖相对次序；③ `GetStartPoint` per-entity Home 与
  `CastleDoor.HoldPlace` 支未建模。
- 定性：需 ≥3 个会话同格推挤（comment `8515c9d00` / `40514dd66` 独立确认，单 probe 无法构造）
  ⇒ 登记 `window_required=true` + `window_items`。

## 2. goal_gate 复算（条件② 前后）

| 项 | 本轮前 | 本轮后 |
|---|---:|---:|
| R 卡总数 | 207 | 207 |
| `closed` 桶 | 11 | 13（+2：enter-map、gate-gmclose） |
| `window_required` 桶（已合入） | 138 | 140（+2：disappear、run-gap） |
| `merged_not_closed`（需人工） | **5** | **1**（gm-batch-5b） |
| 其它桶（whitelist / adjudicated / 未合并 / 三者皆无） | 48 / 3 / 2 / 0 | 48 / 3 / 2 / 0 |

条件② 仍是 `need_human`（rc=2），但 need 项只剩 1 张：`openmir2-gm-batch-5b-cross-server`
（`merge_result merged=true` 但 board=`blocked` 且无 `window_required` 登记——**这是有意的**，原因见 §1.4）。

> **为什么不是 0**：卡面 acceptance ② 明确「若某张确需保持 need_human，如实报告并说明为何」；
> 根规则是「禁止把有未完成工作的卡登记成窗口项」。gm-batch-5b 的剩余是 G11 就绪后的**实现**，不是窗口证据。
> 若 owner 仍要条件② 机判为 0，只有两条合法路径：① G11 落地后实现本卡并走窗口；② owner 明确改写本卡判据口径
> （例如把「G11 外部依赖」列入排除项）——**不能**靠给它补一条 `window_required` 来假装通过。

复算命令（只读）：

```sh
cd <本检出>
python -I -B tools/goal/goal_gate.py --json --skip-tests --skip-script > "$TEMP/gate.json"
python -I -c "import json,os;d=json.load(open(os.path.join(os.environ['TEMP'],'gate.json')));c=d['conditions'][1];print(c['state']);print(c['detail']);print([x['card'] for x in c['evidence']['r_cards'] if x['category']=='merged_not_closed'])"
```

## 3. 门禁

fmt / clippy / `cargo test --workspace` / msgcodegen 全 rc=0（本轮无 `.rs` 改动，仍全量复跑）；
卫生门禁（conflict-marker / stale-pointer / dynamic-count / board-coverage / t_reservation / whitelist-anchor /
m3-registry）全 rc=0；`git diff --check` rc=0。

## 4. 判不准 / 残余

- **未 close、未登记的是 gm-batch-5b**（见 §1.4）；它的 owner 仍是 `openmir2-orchestrator`，实现车道待派。
- 两张窗口登记卡（disappear / run-gap）的「登记型 pass」**不等于**已有真机证据；窗口批次仍按 Q1 独立推进。
- 本轮只改 5 张卡的线程 `status`（`ledger_by=openmir2-dev-1`，`adjudication_card=openmir2-merged-not-closed-5-sweep`），
  未改任何代码、未改场景/规则表、未起栈、未连库。
- 残留：`%TEMP%/gate-sweep.json`（本次复算缓存，可删）；仓库内只新增本文件。
