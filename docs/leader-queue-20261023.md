# Leader 派工队列（人读镜像）

> **权威 = walgit 看板**：`walgit collab board` / `walgit collab thread <id>`。
> 本文件只是给人看的**镜像**，任何派活/认领/状态以签名条目为准；两者不一致时以看板为准。
> 最后更新：2026-10-23 06:2x（`master = be9c5ac6b`）。

## 0. 当前状态（每次巡检后更新这一节）

- **A 窗口欠账 152**（三档见 `docs/window-debt-reclass-20261023.md`：① 现有场景可覆盖 ~60 / ② 只差新场景 43 / ③ 只能 GUI 或离线 49）
- **B 未接线 14**（recon dispatch 实测：入站 `CM_ADJUST_BONUS`；出站 12 孤儿编码器 + `SM_DONATE_FAIL`）
- **终点四条件**：见 `docs/leader-goal-20261023.md` §1；机判脚本卡 `openmir2-goal-gate`（待做）
- **门禁基线**（master `2ecd4b887`）：140 suites / 2923 passed / 0 failed / 62 ignored；anchor **415** ALL PASS；registry **151** 无重复（audit 已改全量 `min_num=1`）

## 1. 合并纪律（本轮踩过的坑，逐条都有代价）

1. **merge_result 不能代替 status**：看板按 `entry.kind` 归列，body 里写 `status: closed` 不会移卡。
2. **merge 前先 `git branch --show-current` 确认在 master**；merge 后 `git ls-remote walgit refs/heads/master` 复核。
3. **只合被审 head**：approve 覆盖 `X` 时 `git merge --no-ff X`，别合分支 tip（tip 可能已前进）。
4. **whitelist 编号是全局资源**：合并前 `grep -n 'T-1[89][0-9]' mir2-rs/tests/parity/whitelist.md` 确认空号（已用到 T-192）。**取号一律走第 9 条的预约表**，本条的 grep 只作合并时复核，不是分配手段。
5. **同文件同一时刻只许一条车道**：`lib.rs` / `whitelist.md` / `tools/conformance/**` 是三大热点。
6. **派活前必须重读该卡线程**，不要只信 board 快照：board 是聚合视图、有滞后，而认领是逐卡签名条目。
   2026-10-23 实测过两次撞车——派出去的 worker 到场时发现另一条会话已在 22 秒前 `card_claim`。
   撞了按「先认领者持有」处理：后到者停手、保留 worktree（身份）、清掉未提交改动、线程留让渡 comment；
   并把后到者顺手探到的**范围清点**作为 Leader comment 回灌给卡主（常常比卡面引用的旧行号更准）。
7. **whitelist 编号**：本日一天撞号 **4 次**（T-174..T-179 vs T-174..T-183、T-4 重复、T-6 同号不同义、T-194 被 menu-ok 与 gm-admin 同时认领）。根因是「先写先占」+ 并行车道各自 grep 到旧值。已拆卡 `openmir2-t-number-reservation` 做**预约表 + 门禁**，**该卡已交付**：取号统一走 `python -I tools/whitelist/next_t.py --card <卡 id>`（写 `mir2-rs/tests/parity/T-RESERVATIONS.md`），「开工时 grep 最大号」的做法**作废**——并行时它必然读到旧值。合并前检查见第 9 条。
9. **裸 `card_claim` 不构成有效认领**：认领账本以「最近一条 **status** 的 owner/worktree/branch」为准；
   只有 `card_claim` 而没有任何后续 status/patch/comment 的，视为占位不干活（2026-10-23 实测 worker-23
   一天认领 8 张卡、交付 0 张，8 张全是别人做完的）。遇到这种 claim 可在自己的 status 里写明
   「接管未交付 claim」直接开工，不必等释放。**这条不削弱「先认领者持有」**——那条针对真在干活的
   车道（有 status/patch 跟进）。
8. **跨工具链的 `/tmp` 不是同一个目录**（Python 的 `/tmp` = `E:/tmp`，Git Bash 的 `/tmp` = `C:/Users/gxh/AppData/Local/Temp`）。`--body "$(cat /tmp/x.json)"` 可能读到**上一次的旧 payload**。2026-10-23 真发生过：一条 review 因此发错内容（作者已删 ref 并在 review 里声明 incident）。临时文件一律用 `E:/tmp/<lane>-<用途>.json`，`cat` 前先 `sha256sum` 核对。

9. **合并前必跑 `T-*` 预约门禁——在「合并结果树」上跑，不是在自己的分支上跑**（卡 `openmir2-t-number-reservation` 交付）：
   **两条分支各自 `check` 恒绿，只有合并树才红**（复审实测：两条车道各占 T-196，分支树各 rc=0、合并树 ② rc=1）。
   做法：先把待合 head 合进本地 master（`git merge --no-commit`，或临时 worktree 合完再验），在**结果树**上跑
   - `python -I tools/whitelist/t_reservation_gate.py check` —— **rc=0 才许合**。①`whitelist.md` 每个 `T-*`
     在预约表有「在用/已落地」行 ②同号不得两个「在用」/不得多行 ③「已落地」号必须在 `whitelist.md`
     ④`whitelist.md` 内部同号 ⑤同卡多「在用」 ⓪坏行/未闭合围栏；缺文件或空表 rc=2。
   - `python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit`（**既有的** whitelist 同号门；
     新门 ④ 已覆盖同一类，这步是双保险）。
   取号：`python -I tools/whitelist/next_t.py --card <卡 id>`——同机所有 worktree 共用
   `$(git rev-parse --git-common-dir)/walgit/t-reservations.journal`（真互斥），并 `ls-remote`+`fetch`
   保证读的是远端**当前** master；远端不可达 ⇒ 拒绝发号（`--allow-stale` 才放行且大声告警）。
   卡片合并后把该行状态改 `已落地`，放弃/换号改成 `已释放`（**别删行**；换号用 `--force-new`）。
   自检 / 牙齿：`... selftest`（18 项，含解析器 cross-check）、`... teeth`（5 颗：②①③④⓪ 各一，逐字节还原 + sha256）。

## 2. 正在跑（每次巡检更新）

以看板为准。本轮在飞：castle-merchant 复审、ya52 修复、window-debt-reclass 修复、port-hygiene 复审。

## 3. 待认领（按价值排序）

| 卡 | 单元 | 写集 | tier |
|---|---|---|---|
| `openmir2-orphan-encoder-trigger-audit` | 12 个孤儿编码器的触发源审计（文档） | `docs/` | L |
| `openmir2-goal-gate` | 终点四条件机判脚本 | `tools/goal/` + 终论文档 | L |

## 4. 车道死活判据（本轮修正）

**只看 transcript mtime 会误杀**：跑 `cargo test --workspace` 的车道可以静默 20–40 分钟。
判死前三个信号一起看：① transcript mtime（测试密集车道阈值 40–45 分钟）；② `Get-Process cargo,rustc`；
③ worktree 里有没有未提交进展。起 finisher 用**原 worktree 的原 principal**（身份在 git-dir 里）。
若原车道复活：**首交付为准**，立刻停 finisher 并核对远端 head 没被交叉污染。
