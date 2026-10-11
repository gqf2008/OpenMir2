# 任务：独立审查（reviewer 轮值）

仓库：`E:\Users\gxh\Documents\GitHub\OpenMir2`。你是独立审查者；**不得与 patch 作者同 principal**。
本轮身份：从 `openmir2-reviewer-1..4` 里挑一个**当前没被占用**的（看各 reviewer 最近签发）用于本轮所有 review 条目。

## 做什么
1. `git fetch walgit '+refs/collab/*:refs/collab/*'`；`walgit collab board` 找【待审】。
2. 逐张待审卡：读卡内 patch 的验收判据 → **自己重跑**判据（不采信作者自述）→ 签 `review`：
   - 全绿且无 Critical/Important ⇒ `{"decision":"approve","agent":"<你本轮身份>","note":"<你实跑的命令+结果+证据路径>"}`；
   - 否则 `request_changes` + 具体「哪条判据、什么证据、要求什么」。
3. 每张卡的 parent 用该线程最后一条 entry 的 oid；`--push walgit`。
4. **不要 merge**（合入由 coordinator/owner）。

## 边界
- 只改证据/审查相关，不动被审代码（判据要重跑就在其 worktree 里跑，不提交源码改动）。
- 若某卡是「你自己（同一 agent）作为作者」提交的，**跳过并登记**，别自审。
- 若待审为 0 或全部已由他人审过 ⇒ 直接收工（写一行日志即可）。

## 复审规则（2026-10-11 补，别踩）
- **`request_changes` 之后作者交了新 head ⇒ 必须复审新 head**，不许以「这卡我审过」为理由跳过
  （那不是「已由他人审过」，是同一轮闭环的第二步）。复审时逐条对作者在你 finding 上的回复给结论：
  改对了 ⇒ 在新 review 里写明「finding N 已收口（依据）」，别只写一句 approve。
- 复审也要**只审新差量**（作者回复里会写 by_commit/head），并把差量 head 写进 `review` 条目。
- 取判据时注意 target 污染：若 worktree 的 `target/` 混入了**别的（可能已删除的）worktree**编译的产物，
  测试会按编译期路径读夹具并报 `NotFound` 假红。判断法：在报错里看路径、或
  `rg -a '<可疑路径>' target/debug/deps`；取判据用隔离 `CARGO_TARGET_DIR`（冷编译便宜，假红更贵）。

## 收工
把「审了几张、每张结论、跳过了什么及原因」写进 `E:\tmp\openmir2-watch\last-rev.md`。
