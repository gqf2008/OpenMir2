# 已合并且无独立审查的卡：只读普查（2026-11-01）

- 卡：`openmir2-merge-without-review-audit`（tier=L）
- 快照：`5f5ec4243154ceff6074c07e794f3f402f4a8df0`（`refs/remotes/walgit/master`，本卡分支 base）
- 工具：`tools/collab/merge_without_review_audit.py`（**只读**；selftest **38/38**；已登记
  `tools/hygiene/tool_selftest_gate.py` 的 `REGISTRY`，认树锚同步为 60 项 / 清点 65 = 60 + 5 + 0）
- 普查域：本地 `refs/collab/inbox/*` 的全部条目 blob。快照时共 13829 条；本卡自身另有 1 条
  认领 `status`（不构成已合并卡，不影响三张表）。

复跑（在仓库根）：

```
python -I -B tools/collab/merge_without_review_audit.py selftest
python -I -B tools/collab/merge_without_review_audit.py report --ref refs/remotes/walgit/master
```

## 0. 结论摘要

| 桶 | 判定 | 卡数 |
|---|---|---:|
| ① | 已合并且有 ≥1 条非作者 approve（`decision` 或 `verdict`） | 845 |
| ② | 已合并但**零 review**（线程里没有任何 `kind=review` 条目） | 13 |
| ③ | 已合并且**只有作者自审**（每条 review 的 actor 都在 patch/issue 作者集合里） | 5 |
| ④ | 已合并、有非作者 review 但**无 approve**（卡面 ②/③ 之外） | 7 |
| 合计 | 已合并线程 | 870 |

补充两个计数：**7 张**卡的账本 `merge_result.oid` 与真实合并提交不同（§4）；
**7 张**卡的真实合并提交未解析（§5，原因逐条给出，未猜）。

## 1. 判据与口径

### 1.1 已合并

该线程任一 `kind=merge_result` 条目的 `body.merged is True`。普查共 986 个线程，其中 870 个已合并。

### 1.2 approve 认两个字段（另计第三个）

- **主判据**（卡面要求）：`body.decision == "approve"` **或** `body.verdict == "approve"`。
  快照里 `decision` 独占 757 条、`verdict` 独占 948 条、两者同时出现 111 条。
- **另计的第三个字段**：快照里有 9 条 review 把结论写在 `body.review`（reviewer-20/21）。
  卡面只要求认 `verdict` 与 `decision`，所以它们**不进主判据**，但单独列出：
  7 张卡（§3 的「④」表）正是靠 `body.review: "approve"` 才有的非作者 approve。若把该字段也认进主判据，
  桶①变成 852、桶④清零；**桶②/③ 不变**。

### 1.3 作者集合（卡面口径）

`kind in {patch, issue}` 的 actor 集合。approve 的 actor 必须**不在**这个集合里，否则算作者自审。
**定义敏感性**：若把「作者」收窄为**只看 patch 作者**，桶③ 只剩 1 张（`openmir2-aoi-verify-maptime`，
审阅者本人也是 patch 作者），另外 4 张会回到桶①（审阅者只是 issue 作者、并非 patch 作者）。
本表按卡面的 patch/issue 并集口径给出 ③=5，并在 §3 的「③ 补充」表里逐张标出审阅者与两种作者身份的关系。

### 1.4 真实合并提交（不猜）

对每条 `kind=patch` 条目的 `entry.refs.head`（patch 的 head 在 `entry.refs`，不在 `body`；
缺失时兜底 `body.head`）解析为 commit，再取
`git rev-list --merges --ancestry-path <head>..<快照>` 的**最老一项**
（即把该 head 第一次并入主线的 merge 提交）。一个卡有多片时逐片求、去重。
求不出就写「未解析」+ 原因，**绝不猜**（§5 逐条列原因）。

### 1.5 账本 oid ≠ 合并提交

`merge_result.oid` 是协作账本记录的值；它有时是真实合并提交，有时只是**当时的 master tip**，
有时甚至不是 commit（见 §6.2）。本表逐卡比较账本 oid 与 §1.4 求出的合并提交，
不同者标「账本 oid ≠ 合并提交」并在 §4 单列。快照内可解析的比对里共 **7 张**不同。

## 2. ① 已合并且有非作者 approve

**845 张**，按卡面只给计数，不逐卡列出。判据：存在 `kind=review` 且 actor 不在作者集合、
且 `body.decision` 或 `body.verdict` 为 `"approve"`。

## 3. 逐卡普查表

下表由 `merge_without_review_audit.py report` 直接产出（时间为 `merge_result` 条目时间，+0800）。
「账本 oid ≠ 合并提交」只标在已解析出真实合并提交的行。

### ② 已合并但零 review（13）

| 卡 id | merge_result.oid | 真实合并提交 | patch 作者 | 是否 R 卡 | 时间 |
|---|---|---|---|---|---|
| openmir2-a3-encode-roundtrip | cc25214649fca9806fa7e4480d896bbf31040bab | 未解析：无线程 patch 条目（patch head 不可得） | —（无线程 patch；issue 作者 openmir2-coordinator） | 未标注 | 2026-10-10 18:56 |
| openmir2-a4-m0-green | ba9db4af | 未解析：无线程 patch 条目（patch head 不可得） | —（无线程 patch；issue 作者 openmir2-coordinator） | 未标注 | 2026-10-10 22:31 |
| openmir2-c3-logout-stage | cc25214649fca9806fa7e4480d896bbf31040bab | 未解析：无线程 patch 条目（patch head 不可得） | —（无线程 patch；issue 作者 openmir2-coordinator） | 未标注 | 2026-10-10 18:55 |
| openmir2-c4-golden-export | cfbb9f8c | 未解析：无线程 patch 条目（patch head 不可得） | —（无线程 patch；issue 作者 openmir2-coordinator） | 未标注 | 2026-10-10 21:56 |
| openmir2-c5-world-sampler | cfbb9f8c | 未解析：无线程 patch 条目（patch head 不可得） | —（无线程 patch；issue 作者 openmir2-coordinator） | 未标注 | 2026-10-10 21:56 |
| openmir2-d2-m3-fixtures | cc25214649fca9806fa7e4480d896bbf31040bab | a978da5cb632d10d1a00b46a5562875fad38149b（账本 oid ≠ 合并提交） | openmir2-coordinator, openmir2-worker-4 | 未标注 | 2026-10-10 18:56 |
| openmir2-m2-world-kickoff | cfbb9f8c | ca6cfa098d6c55d2e8f90be2f76ec86c4225f9e3（账本 oid ≠ 合并提交） | openmir2-worker-4 | 未标注 | 2026-10-10 21:56 |
| openmir2-m3-scopeddiff-multiset | 21b97c3d8d5b4e8074106274cf9d79ac3b98852f | 未解析：无线程 patch 条目（patch head 不可得） | —（无线程 patch；issue 作者 openmir2-orchestrator） | 未标注 | 2026-10-11 14:55 |
| openmir2-rcard-audit-time-bound | 2eb147e6e | 2eb147e6e9fc9efda58d8f1465188e28674686ee | openmir2-reviewer-20 | L | 2026-10-25 05:13 |
| openmir2-s1-oracle-defects | cc25214649fca9806fa7e4480d896bbf31040bab | 1894ccd50e54caf173c8dd9a8888a41093b034d1（账本 oid ≠ 合并提交） | openmir2-svc-1 | 未标注 | 2026-10-10 18:56 |
| openmir2-s2-oracle-cond-crash | cfbb9f8c | d5d236d1e383effdffb2016571c3a10d400fb0c7（账本 oid ≠ 合并提交） | openmir2-svc-1 | 未标注 | 2026-10-10 21:56 |
| openmir2-s3-b8-shift-fix | 777309cd | 777309cd467ec4af170fd63807d82b374d5df841 | openmir2-svc-1 | 未标注 | 2026-10-10 23:21 |
| openmir2-window-dice767 | d1bc60ba | 38c9e071e15c34f03d20a291168f31931e28d1e1（账本 oid ≠ 合并提交） | openmir2-worker-51 | R | 2026-10-23 09:59 |

### ③ 已合并但只有作者自审（5）

| 卡 id | merge_result.oid | 真实合并提交 | patch 作者 | 是否 R 卡 | 时间 |
|---|---|---|---|---|---|
| openmir2-aoi-verify-maptime | 8e7ed7ea9, 8b4f5df5e | fef8776b8f82480aab37295657d2f10f9a9b170d（账本 oid ≠ 合并提交） | openmir2-orchestrator, openmir2-reviewer-5 | R | 2026-10-18 10:36 |
| openmir2-blocked-scenarios-audit | 362254623 | 721ea45b8025be9ae1eab08eccf044d540f3aa20（账本 oid ≠ 合并提交） | openmir2-reviewer-21 | L | 2026-10-25 08:31 |
| openmir2-collab-board-blocked-column | f4e8a29f077d2ddb33af9da9028be4191522b9d8 | f4e8a29f077d2ddb33af9da9028be4191522b9d8 | openmir2-worker-11 | L | 2026-10-13 21:26 |
| openmir2-m2-map-loading | 433e2fb46a3b2f1a80faf1f564ddc16c3d20d708 | 未解析：head 无法解析为 commit：'refs/heads/worktree-m2-map-loading' | openmir2-worker-8 | 未标注 | 2026-10-11 13:54 |
| openmir2-rewrite-inventory | 1258a1b88 | 1258a1b88dd890b0774b885b752343c333fed769 | openmir2-worker-14 | L | 2026-10-22 23:01 |

### ③ 补充：自审者与作者集合的关系

| 卡 id | review actor | 是 patch 作者 | 是 issue 作者 |
|---|---|---|---|
| openmir2-aoi-verify-maptime | openmir2-reviewer-5 | 是 | 否 |
| openmir2-blocked-scenarios-audit | openmir2-orchestrator | 否 | 是 |
| openmir2-collab-board-blocked-column | openmir2-reviewer-5 | 否 | 是 |
| openmir2-m2-map-loading | openmir2-reviewer-3 | 否 | 是 |
| openmir2-rewrite-inventory | openmir2-reviewer-14 | 否 | 是 |

### ④ 非作者 review 存在但无 approve（卡面 ②/③ 之外）（7）

| 卡 id | merge_result.oid | 真实合并提交 | patch 作者 | 是否 R 卡 | 时间 |
|---|---|---|---|---|---|
| openmir2-gm-residual-round2 | aad30901ff994557806e8203533efc5e66d1b963 | aad30901ff994557806e8203533efc5e66d1b963 | openmir2-dev-1 | L | 2026-11-01 01:39 |
| openmir2-gm-residual-teeth | 38439f539fd82fa0c88c62a197621960c37818b5 | 38439f539fd82fa0c88c62a197621960c37818b5 | openmir2-dev-2 | L | 2026-10-31 23:11 |
| openmir2-night-report-dyncount-fix | dce1098c3f0190a041549873952aaa141a8f4e2d | dce1098c3f0190a041549873952aaa141a8f4e2d | openmir2-dev-2 | L | 2026-11-01 03:48 |
| openmir2-night-window-delivery-report | bcf323eab18cca113d46d4e2ffaa34ff6e2da482 | bcf323eab18cca113d46d4e2ffaa34ff6e2da482 | openmir2-dev-2 | L | 2026-11-01 03:48 |
| openmir2-noop-audit-stale-row | b999f41397846dbcb622cdeec81914599e0b4b39 | b999f41397846dbcb622cdeec81914599e0b4b39 | openmir2-dev-3 | L | 2026-10-31 22:20 |
| openmir2-save-allowgroup-writeback | a46d3e3c3eb3ae805b88627bb95157ef56413e3d | a46d3e3c3eb3ae805b88627bb95157ef56413e3d | openmir2-dev-1 | R | 2026-10-31 22:26 |
| openmir2-whitelist-independent-review-1 | 729f1e385f638abcddb1edac40618f38510b1573 | 729f1e385f638abcddb1edac40618f38510b1573 | openmir2-dev-3 | L | 2026-11-01 04:44 |

其中 7 张的 approve 写在 `body.review`（非 card 面要求的 decision/verdict）：
- openmir2-gm-residual-round2：openmir2-reviewer-20 `4900aeb298dc0a7b7daecf5f60e41f38cd498bae`, openmir2-reviewer-20 `06a157e0c2f1f761bc8c948bfcaca371d13894e2`
- openmir2-gm-residual-teeth：openmir2-reviewer-21 `65f66e3011bc21476ca7239273ec58490e5f40a6`
- openmir2-night-report-dyncount-fix：openmir2-reviewer-20 `df532c79f7554a8fa31d057db38350e899f30dff`
- openmir2-night-window-delivery-report：openmir2-reviewer-21 `4881446f9b3a811ffb690eef02710cdad8fa0e93`
- openmir2-noop-audit-stale-row：openmir2-reviewer-21 `5cf5a1705720a8b0e60fdfc45102cd28a7da84b4`
- openmir2-save-allowgroup-writeback：openmir2-reviewer-20 `918e0702e3145c9b2baa2bdff41840c0c5323eba`
- openmir2-whitelist-independent-review-1：openmir2-reviewer-21 `a5283ecf6ddb26936cce0ad23542e714cfb9000d`

## 4. 账本 oid ≠ 真实合并提交（已解析部分的 7 张）

| 卡 | 桶 | merge_result.oid（账本） | 真实合并提交 |
|---|---|---|---|
| openmir2-d2-m3-fixtures | ② | cc25214649fca9806fa7e4480d896bbf31040bab | a978da5cb632d10d1a00b46a5562875fad38149b |
| openmir2-m2-world-kickoff | ② | cfbb9f8c | ca6cfa098d6c55d2e8f90be2f76ec86c4225f9e3 |
| openmir2-s1-oracle-defects | ② | cc25214649fca9806fa7e4480d896bbf31040bab | 1894ccd50e54caf173c8dd9a8888a41093b034d1 |
| openmir2-s2-oracle-cond-crash | ② | cfbb9f8c | d5d236d1e383effdffb2016571c3a10d400fb0c7 |
| openmir2-window-dice767 | ② | d1bc60ba | 38c9e071e15c34f03d20a291168f31931e28d1e1 |
| openmir2-aoi-verify-maptime | ③ | 8e7ed7ea9, 8b4f5df5e | fef8776b8f82480aab37295657d2f10f9a9b170d |
| openmir2-blocked-scenarios-audit | ③ | 362254623 | 721ea45b8025be9ae1eab08eccf044d540f3aa20 |

其中 `cc25214649fca9806fa7e4480d896bbf31040bab` 被 **4 张卡共用**（a3、c3、d2、s1），
`cfbb9f8c` 被 **3 张卡共用**（c4、c5、m2-world-kickoff）——同一个值不可能同时是这些卡各自的合并提交，
说明账本当时记的是 **master tip**，不是该卡的 merge 提交。

## 5. 真实合并提交未解析（7 张，原因逐条）

| 卡 | 桶 | 原因 |
|---|---|---|
| openmir2-a3-encode-roundtrip | ② | 无线程 patch 条目（patch head 不可得） |
| openmir2-a4-m0-green | ② | 无线程 patch 条目（patch head 不可得） |
| openmir2-c3-logout-stage | ② | 无线程 patch 条目（patch head 不可得） |
| openmir2-c4-golden-export | ② | 无线程 patch 条目（patch head 不可得） |
| openmir2-c5-world-sampler | ② | 无线程 patch 条目（patch head 不可得） |
| openmir2-m3-scopeddiff-multiset | ② | 无线程 patch 条目（patch head 不可得） |
| openmir2-m2-map-loading | ③ | patch head 是符号 ref `refs/heads/worktree-m2-map-loading`，本仓不可解析为 commit |

这 7 张的账本 oid 多为 2-parent 合并提交或当时的 tip，但**没有可复核的 patch head 链**，
按卡面「不许猜」的要求一律记「未解析」，不拿账本 oid 反推。

## 6. 关键判例

### 6.1 openmir2-s1-oracle-defects（卡面点名的已知例子，复现一致）

- 账本 `merge_result.oid = cc25214649fca9806fa7e4480d896bbf31040bab`；
- 交付 patch head = `607e9b77347f94820c1d57da7502d0a2826b680a`；
- `git rev-list --merges --ancestry-path 607e9b77..5f5ec4243` 最老一项 =
  `1894ccd50e54caf173c8dd9a8888a41093b034d1` ⇒ **真实合并提交**；
- 与卡面给出的「账本 oid = 当时 master tip、真实合并提交不同」一致。

### 6.2 openmir2-window-dice767（账本 oid 甚至不是 commit）

- 账本 `merge_result.oid = d1bc60ba`（短串）；在本仓 `d1bc60ba` 解析到的是 **blob**
  `d1bc60baad0743e3cfb998d313617d933773aff8`，不是 commit；
- patch head = `018da45bea9fef29055c8500db3ae623e29d7cb9`（该 head 本身是 2-parent merge）；
- `git rev-list --merges --ancestry-path 018da45be..5f5ec4243` 最老一项 =
  `38c9e071e15c34f03d20a291168f31931e28d1e1`（父提交含 `018da45be`）⇒ **真实合并提交**。

### 6.3 ③ 的 5 张里，只有 1 张是「审阅者本人也写了 patch」

按卡面「patch/issue 作者并集」口径，③ 记 5 张；但审阅者身份需要分开看（见正文 §3 的
「③ 补充」表）：

- **openmir2-aoi-verify-maptime**：审阅者 `openmir2-reviewer-5` **本身是 patch 作者**
  （patch 条目 `1516f5f2` 的 actor），是真正的「自审自己的 patch」。
- 其余 4 张（`openmir2-blocked-scenarios-audit`、`openmir2-collab-board-blocked-column`、
  `openmir2-m2-map-loading`、`openmir2-rewrite-inventory`）：审阅者只是 **issue 作者**
  （卡是他们开的），patch 是别人写的。例如 `openmir2-collab-board-blocked-column` 的
  `merge_result` 备注当时写的是「approve openmir2-reviewer-5 … **非作者：作者
  openmir2-worker-11**」——即当时按 **patch 作者** 判非作者。若按这个更窄口径，
  该卡回桶①，③ 只剩 1 张。

**本表不替 owner 选口径**：两种口径的差异已在此列明；主表按卡面文字（patch/issue 并集）给 ③=5。

## 7. 方法与只读保证

- 工具 `tools/collab/merge_without_review_audit.py` 把所有 git 调用收敛到 `_git()`，
  子命令白名单只有 `for-each-ref / cat-file / rev-parse / rev-list / merge-base`；
  任何写子命令（commit/merge/push/update-ref/…）直接抛 `ReadOnlyViolation`（selftest 有负例）。
- 条目读取：`git for-each-ref refs/collab/inbox` + `git cat-file --batch`，按 `id` 分组；
  条目是**扁平 JSON**（inbox blob 没有 `entry` 包层）。
- 自测 **38/38**：verdict/decision 双认（含同时出现且内容不同的 7 条实测）、`review` 字段只另计、作者集合排除
  （patch 与 issue 都算）、四桶分类、空输入退化、tier 回退、只读守卫负例，
  以及**临时 git 仓库**里的真实合并提交解析（merge 提交 / fast-forward 未解析 / 不可解析 head 未解析 /
  账本 oid 相等与不等两种标注）。
- 本卡只读：未签除本卡认领/交付以外的任何条目，未改 `whitelist.md` 或任何卡状态。

## 8. 残留物清单

- 探索脚本与中间产物在仓外 `E:/tmp/mwr-audit/`（`explore*.py`、`classify*.py`、`detail.py`、
  `resolve.py`、`mr_inspect.py`、`report.json`、`report.md`）；**不入库**，可在 owner 复核后删除。
  `report.json` 是本表产出的机器可读证据。
- 仓内新增/改动仅本卡 write_set：审计文档、审计工具、`tool_selftest_gate.py` 登记一行、
  真机窗口计划的认树锚数字。
- 无后台进程、无临时库、无端口占用变更。
