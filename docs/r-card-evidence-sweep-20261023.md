# R 卡窗口欠账 → 机读登记（走查报告）

卡：`openmir2-r-evidence-registry-sweep`（tier=L，写集 = `whitelist.md` / `T-RESERVATIONS.md` / `docs/`）
快照：base = master `04366adc8`，登记时间 `2026-10-23T12:38Z`（tier=L，无生产代码改动、零帧）。

## 1. 名单来源（现取，不手抄）

```sh
python -I tools/goal/goal_gate.py --json
```

**之前**（同一天，master `ada004320` 与本卡开工时的 `04366adc8` 两跑一致）：

```
条件② fail：R 卡 192 张：已完成 10 / window_required 117 / whitelist 6 / 已裁定登记 3 /
comment 证据 0 / 三者皆无 5 / merged 未 closed 且无登记(需人工) 44 / 未开工或未合并 2（不计缺口）/ 其它需人工 5
FAIL 卡：openmir2-gm-batch-5-guild-castle, openmir2-monster-attack-cadence-lead,
         openmir2-multi-session-probe, openmir2-oracle-humitems-align, openmir2-script-user-account
```

**之后**（本卡改动仍在工作树、未提交时自测；提交后同值）：

```
条件② fail：R 卡 192 张：已完成 10 / window_required 119 / whitelist 8 / 已裁定登记 3 /
comment 证据 0 / 三者皆无 1 / merged 未 closed 且无登记(需人工) 44 / 未开工或未合并 2（不计缺口）/ 其它需人工 5
FAIL 卡：openmir2-gm-batch-5-guild-castle（已在该卡留 comment 说明，见 §3）
```

⇒ 「三者皆无」5 → 1；FAIL 名单 5 → 1（该卡有签名 comment 说明，满足本卡判据的「或」支）。

## 2. 逐卡处置（5 张）

| 卡 | 定性 | 处置 | 机器可判性 |
|---|---|---|---|
| `openmir2-monster-attack-cadence-lead` | ② 真缺证据（需真机窗口） | 新增 **T-225**（判据 6 条 + 可复跑命令 + `window_required=true` + 作者/复核人） | `whitelist` 强关联 ⇒ PASS |
| `openmir2-oracle-humitems-align` | ② 真缺证据（需真机窗口） | 新增 **T-226**（字段级槽 3/4 判据 + provenance 记录要求） | `whitelist` 强关联 ⇒ PASS |
| `openmir2-multi-session-probe` | ① `tier=R` 欠真机窗口（slice-2 `--sessions N`） | 签名 `comment`（`2b02680c`）带 `window_required:true` + 窗口判据；T 号沿用本卡分支上的 **T-221**（不另取号） | `window_required` ⇒ PASS |
| `openmir2-script-user-account` | ① `tier=R` 欠真机窗口（CHECKACCOUNTLIST/DELACCOUNTLIST 端到端） | 签名 `comment`（`ae470be2`）带 `window_required:true` + 窗口判据；T 号沿用分支上的 **T-223** | `window_required` ⇒ PASS |
| `openmir2-gm-batch-5-guild-castle` | ③ **卡本身有问题**：A 片 6 条未实现（非窗口欠账）、B 片依赖未移植的 G11 跨服、C 片赛事子系统不存在 | **不登记 T-***（为避免刷绿硬登记，违反 acceptance ③）；在该卡留签名 `comment`（`a20cc17d70`）说明机判正确并给三种处置建议 | 仍 FAIL，但 FAIL 名单有该卡说明 |

依据类型分布：**真机窗口登记 4 张**（其中 whitelist 行 2、`window_required` comment 2）、**卡问题说明 1 张**、**需人工 0 张**（其余 44 张 merged-未 closed 卫生项按卡面 notes 不在本批）。

## 3. 为什么 `gm-batch-5-guild-castle` 不登记 T-*

`T-*` 行登记的是「**已实现、但刻意偏离 oracle 或待真机窗口**」的豁免点。该卡 A 片（6 条，`tier=L`）**一行代码都没写**，B 片连依赖的 `WorldEngine.SendServerGroupMsg`（`SS_205/206/207`）都不存在，C 片赛事子系统仓内没有。给它登记 T-* 只会让 `goal_gate` 变绿而事实不变——正是卡面 acceptance ③ 明令禁止的「为了刷绿硬登记」。已在该卡 comment 里请 Leader 择一：owner 按裁定 `9e7d2227` 推进 A 片 / 显式 `blocked-by` 记账 / 先开 B 片子卡。

## 4. 牙（≥1）：删登记 ⇒ 条件② 重新 FAIL

挑 **T-226**（`openmir2-oracle-humitems-align`）：

```sh
sha256sum mir2-rs/tests/parity/whitelist.md            # 从**提交后的树**取
# 临时删掉 T-226 整行
python -I tools/goal/goal_gate.py --json                # ⇒ 条件② 三者皆无 2，FAIL 名单重新出现该卡
git checkout -- mir2-rs/tests/parity/whitelist.md      # 逐字节还原（提交后树内容）
sha256sum mir2-rs/tests/parity/whitelist.md            # 与上面一致
```

实测（提交后树 `8dfc7c068`，`sha256sum` 前 = `8d5189b2ef08f9d1a5fef720d5b6391524435afad1ed88dbfdbb910f24c83e7c`）：

```
# 删 T-226 行后
条件② fail：… whitelist 7 / 三者皆无 2 …
FAIL 卡：openmir2-gm-batch-5-guild-castle, openmir2-oracle-humitems-align   ← 该卡重新入列

# git checkout -- mir2-rs/tests/parity/whitelist.md 后
sha256sum = 8d5189b2ef08f9d1a5fef720d5b6391524435afad1ed88dbfdbb910f24c83e7c   ← 与删前一致
条件② fail：… whitelist 8 / 三者皆无 1 …
FAIL 卡：openmir2-gm-batch-5-guild-castle
```

## 5. 门禁

```
python -I tools/whitelist/t_reservation_gate.py check          GREEN（rows=209 whitelist=209 max=T-226）
python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit   GREEN
python -I -B mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check      ALL PASS（534 条锚）
```

（T-226 初稿用了 `lib.rs:26906` 行号锚 ⇒ anchor gate 红「高 churn 文件不得写行号锚」，已改符号锚。）

## 5b. 复审 `06121bdb` 处置

| 编号 | 级别 | 处置 |
|---|---|---|
| F1 | important | T-225「可复跑命令」里的 `--recipe` 在 `mir2_probe.py` 不存在（`--help` 无此项）⇒ 改为真实可执行的 `card_window.py --card … --scenario … --profile alt --out …`（工具内部调 `compare_runs.py compare`），并补手工等价命令（`mir2_probe.py --scenario … --out …` + `compare_runs.py compare --oracle … --rust … --out …`）；同时写明该 cadence 场景**尚未存在**（本卡只登记，不建场景）。 |
| F2 | minor | T-226 的 `--scenario <humitems-slots>` 不存在、SQL 被截断 ⇒ 改为模板 + 前置说明，并给出**完整**的两侧查询（`characters_item` 的 `Position IN (3,4)`，Rust 侧库名 `mir2_test_db_r`）。 |
| F3 | minor | guild-castle 的说明 comment oid 写错：`2b344ac3` 在任何 ref 里都不存在，实际签名 comment 是 `a20cc17d70`（openmir2-worker-24）⇒ 全文订正。 |
| F4 | minor | 合并时条件② 为「三者皆无 2」（多出 `openmir2-window-gate-two-runs`）——**不是本卡漏跑**：该卡到 ts 1792759488/1792759491 才第一次被 `goal_gate` 记为 judged（orchestrator comment / blocked status），比本卡 needs-review（1792759173）晚约 5 分钟，作者快照时它还是「未开工（不计缺口）」。该卡现由 Leader 掌握（我已在该卡留问题条 `256b9c2c`）。**复审后已补登记**：在 `openmir2-window-gate-two-runs` 留签名 comment `a780928a`（`window_required=true` + 窗口判据与阻塞说明）⇒ 合并时条件② 复算为**三者皆无 1**（只剩 `openmir2-gm-batch-5-guild-castle`，其说明 comment 为 `a20cc17d70`）。 |
| F5 | minor | T-226 判据 ①③ 的槽号/「不同物品」出处只引了 3 条 ⇒ 补引 `openmir2-worker-45` 的 comment `61dcf471`。 |

## 5c. 复审 `bae36f10` 处置（round 3）

| 编号 | 级别 | 处置 |
|---|---|---|
| F2-SQL | important | T-226 的字段级 SQL 用了 `characters_item` 里不存在的列（`ChrName`/`Index`）。**按 DDL 复核**（`sql/mir2_db.sql`）：`characters_item` 实列 = `Id / PlayerId / Position / MakeIndex / StdIndex / Dura / DuraMax`（`StdIndex` 注释「物品编号」、`MakeIndex` 注释「物品唯一ID」），`characters` 有 `Id / ChrName`；`ItemLocation.Necklace=3 / Helmet=4`（`src/OpenMir2/Enums/ItemLocation.cs:20,24`）亦已核。⇒ 改为 `characters_item AS i JOIN characters AS c ON c.Id = i.PlayerId`，取 `c.ChrName, i.Position, i.StdIndex, i.MakeIndex`，Rust 侧**两处**库名换 `mir2_test_db_r`。 |
| F1-manual | minor | T-225 手工等价命令缺 4 个必填参数（`--login-gate/--account/--password/--server-name`；连 `--scenario/--out` 共 6 项必填，见 `tools/probe/mir2_probe.py:1080 _check_required`）⇒ 补全为 alt 剖面实值（`127.0.0.1:17000` / `winplay0` / `热血传奇`，Rust 侧补 `--game-gate 127.0.0.1:17600`），口令沿用 `CONFORMANCE_PASSWORD` 环境变量（`mir2_probe.py` 无 env 回退，仍要作为 `--password` 传入，子进程 argv 可见）；尖括号占位全部换成 shell 安全的具体占位路径（`tools/probe/scenarios/cadence.json` / `humitems-slots.json`，均注明**待建**），并在行内说明 `<` 在 shell 里是重定向、不要照抄。 |
| F5-precision | trivial | 「各放不同物品」并非 `61dcf471` 原文（全文唯一一处「不同」说的是二进制版本）⇒ 措辞改为「槽号 3/4 与项链/头盔两类见 `61dcf471`」，并注明「不同物品」是判据 ① 由项链 vs 头盔推出的要求。 |

**本轮（round 3）提交前复算**：`t_reservation_gate check` ⇒ GREEN（rows=209 / whitelist=209 / max=T-226）；`whitelist_anchor_gate check` ⇒ ALL PASS（534 条锚）；`goal_gate.py --json` 条件② 仍为「三者皆无 **1**」（FAIL 卡 = `openmir2-gm-batch-5-guild-castle`，有签名 comment `a20cc17d70` 说明；`window_required` 计数随其它线新登记增至 121，与本卡口径无关）；`git merge-tree --write-tree walgit/master HEAD` ⇒ rc=0（无冲突）。本轮**未起栈、未连库**，SQL 的列名/join 键只做 DDL 静态核对。

## 6. 残留物清单

- 无新增临时文件/进程；`E:/tmp/w24-goal-before.json`、`E:/tmp/w24-goal-after.json`、
  `E:/tmp/w24-goal-tooth.json`（删 T-226 后）、`E:/tmp/w24-goal-final.json`（还原后）保留作证据。
