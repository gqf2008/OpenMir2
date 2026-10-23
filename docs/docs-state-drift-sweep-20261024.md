# 文档「状态词」漂移清扫（`openmir2-docs-state-drift-sweep`）

- 卡：`openmir2-docs-state-drift-sweep`（tier=L）；分支 `card/openmir2-docs-state-drift-sweep`；写集 `docs/`。
- **扫描时点**：`walgit/master = 2e3b8500a`（2026-10-24）；**交付 head 基于 `01d4f5877`**（rebase 链：`5a6ed54c1` 窗口判据审计 → `bb4317a01` 背包夹具 → `a112e59b5` recog 打包 → 最新 master）；每次 rebase 后相关行已同步订正（§1 #16/#19 与 §3），其余判定不受影响。本报告与所有订正都以这个 commit 为判据；**本报告自己也会过期**，引用前先带上这个戳。
- 起因：今晚至少三次「文档写着未合/在办，实际已合」，而 `docs/window-batch-plan-20261023.md` 正是 owner 开窗时的执行依据。

## 0. 扫描口径（可复算）

1. 扫描面 = `docs/` 下**今晚（2026-10-23 00:00 起）在 master 上被改过**的文件：

   ```sh
   git log --since="2026-10-23 00:00" --name-only --pretty=format: walgit/master -- docs/ \
     | sort -u | sed '/^$/d'        # 67 个文件
   ```

2. 状态词 = `未合` / `在办` / `待建` / `不可跑` / `不支持` / `尚未`。逐行命中后，把行内反引号 token 当路径，用
   `git ls-tree -r --name-only walgit/master` 的路径集判定「在不在 master」；对状态词指向的**工具/场景/规则/卡**，
   再分别用 `git ls-tree`、`git show master:<file>`、`walgit collab thread <卡>` 的 `merge_result` 判「合没合」。
3. 处置：
   - **过期** ⇒ 就地订正，**保留原说法**（写成「原文：… / 2026-10-24 订正：…」），并给合入 commit；
   - **仍准确** ⇒ 不动，本报告 §3 记「已核，仍准确」；
   - **判不了** ⇒ 写「判不了 + 缺什么」（§4），**不写成已核**。

**命中统计**：67 个文件里 **35 个**含状态词，共 **123 行**命中；其中 **18 个文件 / 32 个「2026-10-24 订正」标记行**（同一行内出现 **33** 次——`window-batch-plan` 的「在办」行含 2 个）判为过期并已订正（**本报告自身不计**），其余归 §3/§4。计数可复算（**base 相对**）：`base=$(git merge-base walgit/master HEAD); git diff $base..HEAD -- docs/ ':!docs/docs-state-drift-sweep-20261024.md' | grep "^+" | grep -c "2026-10-24 订正"` ⇒ **32**（出现次数用 `grep -o … | wc -l` ⇒ 33）。

> 说明：多数命中是**交卡时自述**（「未合并、未自签 review」）、**分类名**（`未开工/未合并` 桶）、**引用原文**（脚本说词「尚未开放」）或**带戳快照**——这些不算漂移，见 §3。

## 1. 订正清单（22 行对应 18 个文件 / 32 个订正标记行；#10、#19 已撤回，见下表说明）

每条格式：**位置** ⇒ 原说法 → 订正（判据 commit）。

| # | 位置 | 原说法 | 现在的事实（判据） |
|---:|---|---|---|
| 1 | `castle-zone-unreachable-20261023.md:160` | 「多会话探针**仍在办**（`card/openmir2-multi-session-probe`）」 | 该卡已合入 master `2a5101606`。合入的是**收件人断言**（`mir2_probe.py` 的 `recipients(..., sessions=/statuses=)`、`online_sessions()`），**不是**起第二个客户端；`card_window.py` 仍每侧一个 probe ⇒ 「多会话窗口跑不了」的结论不变 |
| 2 | `combat-strike-scenario-20261023.md:463` | 「`rules.json` **缺 `31`**，卡 `openmir2-conformance-rule-sm-struck-31` **在办**」 | ident 31 窄掩码已合（`57302d3df`），卡已 closed（approve reviewer-1 `38b40efc`）。注：比较器能判 ≠ Rust 出帧；场景本身仍 `blocked`（`2a69cd6e0`） |
| 3 | `conformance-batch1-login-20261023.md:399` | 「规则表**尚未登记 `643`/`767`**」 | 已随 `776e72e7c`（补齐场景用到的 13 个未登记 ident）进 `rules.json`；当时命令里的 `--allow-unclassified 643,767` 保留仍可复跑 |
| 4 | `conformance-recog-unmask-20261023.md:173` | 「分支 `card/rules-recog-unmask`…**未合并**」 | 已合入 `b42f20a28`（卡 `openmir2-conformance-rules-recog-unmask` closed，approve reviewer-5 `98f6b49a`） |
| 5 | `conformance-rules-202-203-642-709-20261023.md:184` | 「**未合并**、未自签 review、未删 worktree」 | 卡 `openmir2-conformance-rules-updateitem-durachg` 已合入 `6c2da8bf3`（approve reviewer-5 `699b6315`） |
| 6 | `gm-impl-shutup-release-20261023.md:93` | T-212 前置「**前置卡未合入**」 | 前置卡 `openmir2-gm-script-exec-seams` 已合 `7bbb5ea7`；whitelist T-212 行已由 Leader 改为「前置已齐、待排期」 |
| 7 | `magic-fixture-assertion-strength-20261024.md:115` | 「**未合并**…」 | 已合 `51a450c20`（approve reviewer-3 `d2685535`，closed） |
| 8 | `magic-fixture-selftest-gaps-20261023.md:89` | 「**未合并**…」 | 已合 `e115339e3`（approve reviewer-6 `7a9b847a`，closed） |
| 9 | `needs-human-20261023-evening.md:11` | `eat_item`「仍阻塞于 `openmir2-discovery-bagitems-201`」 | 该卡已合 `f79f2d2a`（**只解规则层**）；夹具工具随后合入 `bb4317a01`（卡 `openmir2-window-inventory-fixture`，不 closed：`window_required=true`），但 `eat_item` 场景**尚未声明** `inventory` ⇒ 仍不可跑 |
| 10 | `needs-human-20261023-night.md:23` | `spell_cast` R5 缺 `characters_magic` 夹具（magic-fixture 卡**在办**） | **已撤回**：master（`7bb48239a` 起）已自带正确文本「夹具随 `512d9ecf5` 落地、改由 profile `magic` 段声明；场景已解封（`openmir2-spell-cast-unblock`），Rust 魔法未完成 ⇒ 真机跑不了」；本卡取 master 侧，不再保留旧注 |
| 11 | `needs-human-20261023-night.md:~70` | batch-plan「卡 `openmir2-window-batch-plan-refresh` **在更新它**」 | 刷新已合 `ccf74b2b4` |
| 12 | `r-card-evidence-triage-20261023.md:7` | 「`tools/goal/goal_gate.py` **尚未进 master**」 | 已随 `93226f278` 进仓 |
| 13 | `r-card-ledger-sweep-20261023.md:17/170/235/245` | U 类「未合并/等返工」**1 张**；该卡「无 merge_result」 | `openmir2-monster-showhp-53-broadcast` 已合 `65a8af8d6`（approve reviewer-2 `593ae8bf`，tier=R 保持 in-progress）⇒ U 桶现为 **0**（4 处同步订正） |
| 14 | `r-window-registry-batch-1-20261023.md:122` | 「`T-223..T-226`…本仓 `T-RESERVATIONS.md` **尚未合并那些行**」 | `T-225`/`T-226` 已在 master 的 `T-RESERVATIONS.md`；`T-223`/`T-224` 仍不在（见 §4 判不了） |
| 15 | `rewrite-remaining-inventory-20261022.md:539-544/752` | `SM_RECONNECT`/`SM_SKELETON`「**无卡（待建）**」；12 个场景文件「**尚未进 master**」 | 卡 `openmir2-sm-reconnect-wiring` 已合 `283a27460`（R 卡 in-progress）；卡 `openmir2-sm-skeleton-wiring` 在办。场景分支 `card/openmir2-dualrun-window-burndown` 已合 `33b40970`，master `tools/probe/scenarios/` 现 **20** 个场景 JSON（3 处） |
| 16 | `window-batch-plan-20261023.md:433/439/440/450×3/457/495/503/506` | §7「preflight.py **尚未合入 master**」「inventory 夹具 head `515eb2804`」等 | preflight 已合 `2e3b8500a`（卡 closed，approve reviewer-5 `f13a28be`）⇒ 命令恢复为 `python -I tools/conformance/preflight.py --profile alt`；inventory 夹具随后合入 `bb4317a01`（`card_window.py` 已支持 `inventory`，但 `bag_equip`/`eat_item` 场景未声明）——「跑不了」的结论不变、理由从「工具没合」改成「场景没声明」（`window-batch-plan` 内 9 个标记行 + 1 处不带标记的 §7 题注订正） |
| 17 | `window-batch-plan-20261023.md:177` | `openmir2-m3-monster-ai-tick`「**needs-review**、未 merge」 | 已合入 master（最新 merge `711bdb149`，tier=R 不 closed） |
| 18 | `window-debt-reclass-20261023.md:21` | 15 个场景文件「**尚未进 master** `be9c5ac6b`」 | 已合 `33b40970`；「① 现有场景实际只有 3 个」的推论随之失效 |
| 19 | `window-runbook-20261023.md:234` | 背包夹具「由卡 `openmir2-discovery-bagitems-201` 承接」 | **已撤回**：rebase 到 `bb4317a01` 时该段已被背包夹具卡整段重写为「声明即可」（§1.6），本卡取 master 版本；原订正不再适用 |
| 20 | `goal-snapshot-late-20261023.md:314` | `openmir2-window-criteria-quality-audit`「**在办**」 | 已合入 master `5a6ed54c1`（返工 v2，纯 docs，tier=L） |

| 21 | `window-runbook-20261023.md:126/128` | 「`-WhatIf` 只打印路径、不打印 sha256（`:349`）」 | 脚本已打印 `SERVER_EXE`/`SERVER_EXE_SHA256`/`SERVER_EXE_BUILD` 三行（`start_mir2_server.ps1:381`）；两处已订正（review F4 漏项） |
| 22 | `upgrade-retrieve-scenario-20261023.md:298` | 「背包夹具不支持」 | 工具随 `bb4317a01` 支持 `inventory` 声明；本场景缺的是**声明**，且夹具卡真库读写回未跑（review F2 漏项） |

> 表内 22 行对应 **18 个文件 / 32 个订正标记行**（`window-batch-plan` **9**、`r-card-ledger-sweep` 4、`rewrite-remaining-inventory` 3，其余 15 个文件各 1；本报告自身不计；#10/#19 已撤回不计）。**#19（runbook:234）已撤回**：rebase 到 `bb4317a01` 时该段已被背包夹具卡整段重写为「声明即可」，本卡取 master 版本。

## 2. 三个卡面点名实例的结论

| 卡面实例 | 结论 |
|---|---|
| ① `window-batch-plan` 把 `preflight.py` 标「在办（未合）」 | **成立**，已订正（§1 #16）：合并点 `2e3b8500a`；§7 开头命令行、§7.1 行、§7.2 2B、§7.4、§7.5 全部同步 |
| ② `window-runbook` 曾把 `-WhatIf` 写成「只打印路径」 | **上一版误判为「已核，仍准确」**（review `808613798` F4）：§1.6 虽已写三行，但 §1.5 的 `:126/:128` 仍写「只打印路径、不打印 sha256（`:349`）」，与 master 脚本 `start_mir2_server.ps1:381` 矛盾 ⇒ **本版已就地订正**（§1 #21） |
| ③ `needs-human-night` 条件② / `goal-snapshot-late` 主表数字 | `needs-human-night` §7 与 `goal-snapshot-late` §「后续更新（00:5x，`5e6b4e12`）」都**明确覆盖**旧数字 ⇒ **已核，仍准确**（带戳）；仅 §1 第 3 条夹具状态过期（§1 #10）与 §1 的 `criteria-quality-audit` 状态过期（§1 #20）已订正 |

## 3. 已核，仍准确（不改；下一轮不必重扫）

| 文档（命中数） | 为什么仍准确（判据） |
|---|---|
| `castle-zone-unreachable` :5/:60/:132（3 处除 #1 外） | 城堡区**仍不可跑**：`T-38` 申请链 `NotImplementedException`、`StartCastlewarTime=20` 整点门、`AttackSabukWall.txt` 空；`git ls-tree master tools/probe/scenarios/` **无** `castle_zone` 文件 |
| `combat-hit-waitfix`:72 | `combat_hit.json` 在 master 仍含 `blocked`（实读 20 个场景 JSON：只有 `combat_hit`/`combat_strike` 带 `blocked`；`spell_cast` 已由 `openmir2-spell-cast-unblock`（`7bb48239a`）解封） |
| `combat-strike`:417 | T3（删攻击步）缺口仍在：卡 `openmir2-probe-scenario-validation-gaps` 已 closed，但 T3 仍登记为残留 `R2-producer-map` |
| `goal-snapshot-late`（其余 10 处） | §1 是**带戳快照**且被「后续更新（`5e6b4e12`）」显式覆盖；§4 的「不可跑 22 张」口径在该戳上自洽 |
| `r-window-registry-batch-1`（其余 10 处） | 基线戳 `74f0fbb0d` + `generated_ts` 前后对比；批次数字是历史快照 |
| `r-window-registry-batch-2`(10) / `batch-3`(10) / `batch-4`(7) / `closeout`(1) | 均带基线戳（`19c59598a` / `ee292359b` / `512d9ecf5` / `ed4dd05c6`）；「场景待建」指向的 `world_observe.json`、`cadence.json`、`humitems-slots.json` 在 master 中**确实不存在**（`git ls-tree` 0 命中） |
| `r-card-evidence-sweep`:16/25/86/97 | 数字带 `04366adc8` 戳；`cadence`/`humitems-slots` 场景仍不存在 ⇒「尚未存在/待建」准确 |
| `rewrite-remaining-inventory`:539-542 | `SM_BREAKWEAPON`/`SM_HIDEEVENT`/`SM_LAMPCHANGEDURA`/`SM_QUERYYB*` 在 479 张卡里**仍无同名卡** ⇒「无卡（待建）」准确 |
| `probe-scenario-blocked-gate`(4) | `blocked` 的形状/rc 语义定义，与 master `card_window.py` 的 rc=2 行为一致 |
| `leader-goal`(3) / `leader-loop`(1) | 「未开工/未合并」「不可跑」是**分类定义与口径建议**，不是事实断言 |
| `needs-human-night` §1①/§1②、§7 数字 | 带戳 `ee292359b` / `38ffb0bfe`；§7 的条件② `need_human` 结论与当时快照一致 |
| `param-discovery-design`:81 | master 的 `validate_computed` 仍拒绝 `derived_from` 指向 computed（防环）；`computed.pack` 的 `lo/hi` 可引用 computed 是**另一条**路径，不矛盾 |
| `rules-pending-approval-pack`:574 | master `rules.json` 的 `approved_by` 仍是 `PENDING owner/coordinator…` |
| `runbook-tool-consistency-audit`:32 | 该行本身已标「**已过期需改**」并给合入点（审计件，不是断言件） |
| `session-summary`:185 | 「21:44 快照时…尚未」是**带时点的历史陈述** |
| ~~`upgrade-retrieve-scenario`:298/314~~ | 原判「仍准确」**不成立**（review F2）：夹具工具已随 `bb4317a01` 支持 `inventory` ⇒ 本版已订正 `:298`（§1 #22），不再列「仍准确」 |
| `window-529-dualrun`:153 | review F3 的**处置记录**（「已写实」），非状态断言 |
| `window-debt-reclass`:414 | 逐卡数字带基线 `be9c5ac6b` |
| `window-runbook`:16 | 文档自带的「原文…已订正」说明（`bb4317a01` 后又整段重写了背包夹具面，见 §1 #19） |
| `script-action-var-text-audit`:273 | 误命中：`E:/MirServer/...` 被当路径 token，语义是「原样不展开」 |
| `orphan-encoder-trigger-audit`:404/418 | 「事件尚未发生 / 尚未 Close」是**运行时语义**，不是合并状态 |
| `r-card-evidence-triage`:56 | 口径讨论（新建未合并卡该落哪一桶） |
| ~~`window-batch-plan`:159~~ | 原判「准确」，但 `bb4317a01` 后 `card_window.py` 已支持 `inventory`（grep 80）⇒ 该行**转入 §1 #16 订正**，不再列「仍准确」 |

## 4. 判不了（缺什么）

| 位置 | 缺什么 |
|---|---|
| `window-batch-plan:83`「`needs-review` 的两张是返工后未合入当前 head 的卡」 | **原文没点名是哪两张** ⇒ 无法逐卡判定；只能确认该快照后至少 `openmir2-m3-monster-ai-tick` 已合并（§1 #17）。按卡面规则记「判不了」 |
| `r-window-registry-batch-1:122` 的 `T-223`/`T-224` | master `T-RESERVATIONS.md` 里没有这两行，仓库外 journal 未随仓 ⇒ **判不了**是「占用后废弃」还是「尚未合并」；需 owner 或 journal 原文 |
| `openmir2-monster-showhp-53-broadcast` 等 **board 不列出的卡** | `walgit collab board` 不列出该卡（`walgit collab board | grep -c openmir2-monster-showhp-53-broadcast` ⇒ 0；已知「其它需人工 5」），本报告改用 `walgit collab thread` 的 `merge_result` 判定；board 侧的修正在卡 `openmir2-board-index-reconcile` |

## 5. 复算命令（给下一轮）

```sh
# 1) 今晚改过的 docs
git log --since="2026-10-23 00:00" --name-only --pretty=format: walgit/master -- docs/ | sort -u | sed '/^$/d'

# 2) 状态词命中（只看行）
git grep -nE '未合|在办|待建|不可跑|不支持|尚未' walgit/master -- docs/

# 3) 关键工件是否在 master（本报告用到的全部判据）
git ls-tree walgit/master tools/conformance/preflight.py tools/goal/goal_gate.py
git ls-tree --name-only walgit/master tools/probe/scenarios/
git show walgit/master:tools/conformance/rules.json | grep -c '"ident"'   # 42 条，含 31/643/767
git merge-base --is-ancestor <merge-commit> walgit/master && echo merged

# 4) 卡片状态（看最后一条 status 与 merge_result）
walgit collab thread <卡> --repo .
```

本报告用到的合并点一览（可直接 `git show --stat` 复核）：
`2e3b8500a` preflight ／ `2a5101606` multi-session-probe ／ `57302d3df` ident 31 ／ `776e72e7c` 补 13 个 ident ／
`b42f20a28` recog-unmask ／ `6c2da8bf3` 202/203/642/709 ／ `7bbb5ea7` gm-script-exec-seams ／
`512d9ecf5` magic fixture ／ `e115339e3`／`51a450c20` magic fixture 两卡 ／ `f79f2d2a` bagitems-201 ／
`93226f278` goal_gate ／ `65a8af8d6` showhp-53 ／ `33b40970` dualrun burndown ／ `ccf74b2b4` batch-plan refresh ／
`283a27460` sm-reconnect ／ `711bdb149` m3-monster-ai-tick。

## 6. 残留与边界

- 写集仅 `docs/`：**未改** `whitelist.md`、`T-RESERVATIONS.md`、工具、场景、任何卡片状态。
- 本卡**不修**判据本身：例如 `window-batch-plan` 里「现在就能跑 = 14 个」的场景计数（preflight 合入不改变场景集，因此仍成立），以及 §7.3 的结论均原样保留——**`combat_strike`/`combat_hit` 仍 `blocked`；`spell_cast` 已解封（`7bb48239a`），但仍缺 Rust 魔法落点 + 第二观察者 ⇒ 真机仍跑不了，且 dry-run 取证须带 `--profile`（`magic` 段声明）**。
- 本报告**是快照**：master 一旦再动，§1 的「现在的事实」会再次过期；下一轮按 §5 重跑即可，不要直接引用 §1 的 commit 而不核 master。
- 临时物：`E:/tmp/tonight-docs.txt`、`E:/tmp/master-files.txt`、`E:/tmp/drift_scan.py`、`E:/tmp/drift-scan.txt`（扫描原始命中 123 行）、`E:/tmp/drift_edits.py`、`E:/tmp/drift_edits2.py`、`E:/tmp/board.txt`、`E:/tmp/cards.txt`（479 张卡清单）——均为复核证据，保留；无进程、无库改动。
