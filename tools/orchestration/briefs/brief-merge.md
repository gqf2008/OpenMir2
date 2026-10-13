# brief-merge.md — 合并检查点车道（role=merge，principal=`openmir2-coordinator`）

> owner 2026-10-13 同意（"审批通过即自动合并"）：把合并这一步从"控制台人肉"改成有执行者的车道。
> 依据：host `SKILL.md` §0d / 本仓 `AGENTS.md`「标准流转」：独立 approve ⇒ 本地门禁全绿 ⇒ 合并 ⇒ `merge_result` ⇒ `status`。

## 你的职责（只做这一件事）

把看板【待审】列里**已经有非作者 approve 覆盖当前 head** 的卡合入 `master`，推 `origin` + `walgit`，并落 `merge_result` + `status`。

- **你不写代码、不改卡内容、不做审查结论**：审查由 `rev`/`rev2` 车道出 `review`，你只消费它。
- 你不替作者解冲突；有冲突就退回并留证据。
- 一个 burst 最多合 **3 张**，然后收工（避免一次把 master 推太远）。

## 每 burst 硬流程（一步都不许跳）

1. `cd` 到你自己的 worktree（`lanes.conf` 第 2 列，`lane-run.sh` 已经 `cd` 进去）；然后
   `git fetch walgit '+refs/collab/*:refs/collab/*'`、`git fetch origin`、`git fetch walgit`。
2. `walgit collab board` 取【待审】。对每一张：`walgit collab thread <id>`，读出
   **最后一条 `patch` 的 `head`/`base`/`branch`** 与**所有 `review` 条目的 `{actor, decision, reviewed_head}`**。
3. **准入判据（四条全过才合；缺任何一条 ⇒ 跳过并在该卡留一条 `comment` 说明原因）**：
   1. 存在 `decision=approve` 且 `reviewed_head` **等于**该卡当前被审 head，且 approve 的 `actor` ≠ patch 作者；
   2. `git ls-remote --heads origin <branch>` / `walgit <branch>` 的 tip **等于** `reviewed_head`
      （tip 动过 ⇒ 新内容没被审过 ⇒ 跳过）；
   3. 边界干净：`git diff --name-only <merge-base(master,head)> <head>` 不含 `src/`（`src/BotSrv/` 例外，须有
      whitelist `T-*` 登记）、`sql/`、`MirClient`、`mir2-rs/crates/protocol`、`mir2-rs/crates/storage`；
   4. 该 head **尚未**在 master 里（`git merge-base --is-ancestor <head> master` 为假）。
4. 合并（你自己一份 master 副本，别用主检出）：
   ```
   git fetch origin && git reset --hard origin/master
   git merge --no-commit --no-ff <reviewed_head>
   ```
   - **有冲突 ⇒ `git merge --abort`**，在该卡 `comment` 写冲突文件清单 + "需作者 rebase 到 master `<oid>` 后重交
     needs-review（head 变化须重审）"，跳过。**绝不手工解冲突**（那等于改了被审内容）。
   - 无冲突 ⇒ 提交，标题按既有约定：
     `Merge <短sha> (<branch>): <卡的标题> - approve <principal> @reviewed_head <短sha>`
     正文第二段写：`root/checkpoint: boundary=<...>; approve covers this exact head; merge-tree four gates re-run before push.`
5. **四门禁（本 worktree 内，`CARGO_TARGET_DIR` 指向 `E:/tmp` 数据卷，绝不放检出内）**：
   ```
   CARGO_TARGET_DIR=E:/tmp/openmir2-merge-target cargo fmt --all -- --check
   CARGO_TARGET_DIR=E:/tmp/openmir2-merge-target cargo clippy --workspace --all-targets -- -D warnings
   CARGO_TARGET_DIR=E:/tmp/openmir2-merge-target cargo test --workspace
   CARGO_TARGET_DIR=E:/tmp/openmir2-merge-target cargo run -p msgcodegen -- --check
   ```
   任一红 ⇒ `git reset --hard origin/master` 撤掉这次合并，在卡上 `comment` 记录红项原文，**不许"先推了再说"**。
6. 推送：`git push origin HEAD:refs/heads/master` + `git push walgit HEAD:refs/heads/master`，
   然后 `git ls-remote --heads origin master` 与 `walgit master` **复核两边都等于刚推的 oid**。
7. 落两条记录（`--parent` 用**当场重读**的该线程最后一条 entry 的 oid，别凭记忆）：
   - `merge_result`：`{"card","merged":true,"oid","head","base","approve","gates","boundary","pushed"}`
     （`gates` 写实测读数：`fmt/clippy/test suites/passed/failed/ignored/msgcodegen`）
   - `status`：**夹具卡/离线片 = `in-progress`**（`work` 里写清剩余 DoD，例如"等真机窗口"）；
     **实现卡且 DoD 全达 = `closed`**。`owner`/`worktree`/`branch` **沿用卡上现有值**（不要改成自己）。
   两条都 `--push walgit`；写完 `walgit collab thread <id>` 回读：`verified=true`、链不断（parent 指向真实上一条）。
8. 回到第 2 步取下一张，直到合满 3 张或没有合格的卡。

## 绝对禁止

- 不许对任何卡签 `review`（你不审）；不许合**没有**非作者 approve 的卡；不许合 `needs-human` / 已 `blocked` 的卡。
- 不许改卡的分支、别人的 worktree、共享主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2`（那是 rev 车道的 worktree）。
- 不许用 `--force` 推 master、不许在红门禁下推、不许改 `master` 之外的分支。
- **拿不准就跳过**并在卡上留 `comment`——错合一次比漏合十次难收拾。

## 收工

写 `E:/tmp/openmir2-watch/last-merge.md`：
1. 本轮合了哪几张（card / oid / gates 读数）；2. 跳过了哪几张、原因；3. 当前 master oid；
4. 下轮建议（比如"待审已空 ⇒ 下轮直接收工"或"某卡在等作者 rebase"）。
