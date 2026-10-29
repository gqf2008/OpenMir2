# R 卡证据补登分诊（终点条件②，3 张真缺口）

- 卡：`openmir2-r-card-evidence-triage`（tier=L）
- worker：`openmir2-worker-45`（worktree `wt-worker-45`，分支 `card/r-triage`）
- 基线：master `6345cdcbb`
- 机判复算：`python -I -B tools/goal/goal_gate.py --repo <主检出> --json --skip-tests --skip-script`
  （脚本取自 `card/goal-gate` @ `ce637865a`；**2026-10-24 订正**：`tools/goal/goal_gate.py` 已随 master `93226f278` 进仓，可直接用 master 版本）
- 范围：Leader `30f4e298` 收窄后的 3 张。`openmir2-window-529-dualrun`、
  `openmir2-dualrun-triage-enterworld-frames` 已由复审判为假阳性，本卡不碰。

## 1. 判据来源（口径）

`docs/leader-goal-20261023.md` §1 条件②原文：「所有 R 级卡要么有真机/协议对拍证据，
要么有**显式登记 + 明确理由**（`window_required` 或 whitelist `T-*` 且 owner 复核过）；
不允许『代码已合但无证据且无登记』」。

机判实现（`goal_gate.py` `collect_r_evidence` / `r_category` / `classify_r_evidence`）的 PASS 通路只有五条：
board「已完成/closed」、`window_required=true`、whitelist T-* 行**点名本卡**、最新 approve review 的
`window_required` 布尔裁定、`comment_evidence`（仅 `--allow-verdict-grep` 开启时）。
其余落 `merged_not_closed`（需人工）、`weak_t`（T-* 只在正文出现、登记行未点名）、
`none`（FAIL「三者皆无」）。

## 2. 分诊结论（3 行表）

| 卡 id | 分类 | 依据 | 建议动作 | 阻塞条件② |
| --- | --- | --- | --- | --- |
| `openmir2-gm-batch-5-guild-castle` | **③ 真缺登记**（当时 A 片缺登记；B 片另有 ② 依赖 G11；C 片缺登记）。**已收口（2026-10-29）**：A 片经 `2810298e9` 合入、B 片由 `openmir2-gm-batch-5b-cross-server`（blocked-by G11）承接、C 片已登记不实现 | 线程 6 条（历史轨迹）：issue→claim→comment 32a2aa61→status needs-human→Leader 裁定 9e7d2227→status in-progress。当时零 patch、零 merge_result、零 review、whitelist 无点名行。board 当时 in-progress，owner=`openmir2-reviewer-5`，last_ts=1792703594（约 3h 前，超 1h 判定窗）。**现为 closed（2026-10-29）** | 历史建议（已执行）：A 片（6 条，tier=L）照 9e7d2227 实现后走 patch/review；B 片（6 条，tier=R，依赖 G11 `SS_205/206/207`）在 `mir2-rs/tests/parity/whitelist.md` 新增 **T-\*** 并开子卡 `openmir2-gm-batch-5b-cross-server`；C 片（赛事 4 条）出 Envir grep 命中数后登记「不实现」 | **是（当时）** |
| `openmir2-monster-attack-cadence-lead` | **② 真缺证据（需窗口）** | 线程 3 条：issue + comment 41b9163e（VERDICT=**线索被证伪**：同怪 SM_STRUCK 相邻间隔 C# min 3.190s / Rust min 3.147s，均 >3s，两侧节拍门同值；`mir2-rs/tests/parity/m3/audit-monster-attack-cadence-20261019.md`）+ status in-progress（「余量=按评审 C-1 口径的 HP 受控重采样（真机窗口）」）。**无 merge_result、无 window_required、无 T-\***。owner=`openmir2-orchestrator`，last_ts=1792389330（约 90h 前，无在跑车道） | 二选一：①按 C-1 口径真机重采（每侧 3 趟 / L1 / HP19 / 每趟校验 `SM_NEWMAP==(605,578)` / 不点怪 ≥60s / 三项判据），在此之前于本卡留 `window_required:true` 登记；②若判定「线索已证伪、无代码交付」⇒ 本卡直接收口 closed。**不建议**补 T-*（没有可豁免的刻意差异） | **是**（窗口跑完或本卡收口之前） |
| `openmir2-oracle-humitems-align` | **② 真缺证据（需窗口）** | 线程 3 条：issue + comment fbf913c3（两侧已 1:1：load/save 均「槽位即位置」，漂移用例反转为稳定性用例 `hum_item_slots_are_stable_across_login_and_save`（`mir2-rs/bins/game-svr/src/lib.rs:26906`），master `76e5f66f8` 已推；**但 `window_note` 明说对端两次部署后 GameSvr 二进制已变、窗口抓包 provenance 作废**）+ status in-progress。**无 merge_result 记账、无 window_required、无 T-\***。owner=`openmir2-orchestrator`，last_ts=1792424820（约 80h 前） | 下次真机窗口重抓**装备槽字段级**证据（`characters_ablity` / 槽 3/4）并记录部署 provenance；在此之前于本卡留 `window_required:true` 登记。**不建议**补 T-*（两侧已对齐，无刻意差异可豁免） | **是** |

判级依据：三张都是 R 级或含 R 片（落库 / 战斗时序 / 帧面），按 AGENTS.md「判据分级」属 R，
真机证据缺位不得用「语义等价」盖掉。


## 2b. 已发条目（可复算）

| 卡 | 条目 oid | kind | 作者 | 验签 |
| --- | --- | --- | --- | --- |
| `openmir2-gm-batch-5-guild-castle` | `8d86fb3f88864b6d4b88e32e13c0021ced537207` | comment | `openmir2-worker-45` | verified |
| `openmir2-monster-attack-cadence-lead` | `3582135d48afae8d67e104e03a23071eda786dc9` | comment | `openmir2-worker-45` | verified |
| `openmir2-oracle-humitems-align` | `61dcf47118247a88d89cd9bf188ad790ec5587e5` | comment | `openmir2-worker-45` | verified |
| 本卡认领 | 见 `openmir2-r-card-evidence-triage` 线程最新 status | status | `openmir2-worker-45` | — |

## 3. 与 `openmir2-r-card-ledger-sweep`（worker-43）的协调

- 开工时（ts≈1792714xxx）该卡 `status: in-progress`，`docs/r-card-ledger-sweep-20261023.md` **尚不存在**，
  `refs/heads/*ledger-sweep*` 远端也无分支 ⇒ 未产出。
- 逐张核对三卡线程：条目 author 全为 `openmir2-orchestrator`（+ 一条 `openmir2-reviewer-5` claim），
  **无 worker-43 的任何 comment/status** ⇒ 未覆盖，本卡不重复发条目。
- 后续若 sweep 在同一卡再发条目，两份条目语义一致（均为「缺登记/缺窗口」），不冲突。

## 4. 越界观察（记录，不处理）

- 本次复算条件②的 FAIL **实际是 4 张**：多出 `openmir2-map-event-frames`（ts 1792713751 新建、
  1792714049 刚被 `openmir2-worker-23` 认领，board=in-progress，线程仅 issue+claim）。
  它是「刚开卡、还没干活」被判 FAIL「三者皆无」，属机判口径问题（新建未合并卡应落「需人工/进行中」
  而非 FAIL），**不在 Leader 收窄后的 3 张范围内**，另归 `openmir2-goal-gate` 的修复片。
- 条件②整体计数（同次复算）：R 卡 164 张 / 已完成 7 / window_required 94 / whitelist 6 /
  已裁定登记 5 / comment 证据 0 / 三者皆无 4 / merged 未 closed 且无登记 43 / 其它需人工 5。

## 5. 残留物清单

- 无新增临时文件入仓；分析用的机判 JSON 落 `E:/tmp/gg45.json`（可再生，不保留必要性）。
- 未改 `whitelist.md`、未改 `T-RESERVATIONS.md`、未 closed 任何卡、未改任何卡 status。

## 6. 复现

```sh
# 机判（脚本在 card/goal-gate 分支）
git -C <主检出> show card/goal-gate:tools/goal/goal_gate.py > /tmp/goal_gate.py
python -I -B /tmp/goal_gate.py --repo <主检出> --json --skip-tests --skip-script
# 看三卡线程
walgit collab thread openmir2-gm-batch-5-guild-castle
walgit collab thread openmir2-monster-attack-cadence-lead
walgit collab thread openmir2-oracle-humitems-align
```
