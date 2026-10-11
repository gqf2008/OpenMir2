# 任务：C6 原卡 openmir2-c6-e2e-loadgate —— 逐条回复 reviewer-4 的 request_changes 并闭环

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\c6`（分支 `worktree-c6`，HEAD `c54cd30c`）。
身份 **openmir2-worker-5**（该 worktree 已 join，命令可省略 `--actor`/`--key`）。
两卡：原卡 `openmir2-c6-e2e-loadgate`（**本轮主战场**）+ 后续卡 `openmir2-c6-loadgate-idempotent`（已在 needs-review，保持不动）。

## 0. 背景（先读后写，别凭记忆作答）

- 原卡最后一条是 reviewer-4 的 `review`（oid `a9e5ec0d…`，`reviewed_commit=7fd20f70`）：**request_changes**
  （4 Important + 5 Minor），此后**作者侧没有逐条回复** ⇒ 原卡无法闭环、M5 前置一直挂着。
- 卡内纪律（§2 第 3 条 + owner §5.2 指示）：可判的技术问题必须自己判完并记 `comment`；把可决问题挂起就是卡壳。
- 新证据已在后续卡（`7c0a3226 @81e387d8` + docs-only `c54cd30c`）：开号跨轮幂等、同一命令两连跑三档 verdict 一致
  且 200 档 GREEN、tier-500/1000 登录上限根因=假人侧 T-9/T-10 越界。这些正好逐条覆盖 reviewer-4 的 findings。
- **rev 车道现在正占着 5000/7000/7100/7200 在起栈独立复跑 C6 三档** ⇒ 本轮**不要起栈**；只在端口空闲时
  （`E:\MirServer\port-status.ps1` 十端口无监听）才做实机复核，否则把实机项登记为「待端口窗口」。

## 1. 本轮要做的

1. `git fetch walgit '+refs/collab/*:refs/collab/*'`；读原卡与后续卡两条线程（含 reviewer-4 的 findings 原文与
   后续卡的证据台账）。
2. 在**原卡**发一条 `comment`（parent 用该线程最后一条 entry 的 oid），**逐条**对 reviewer-4 的 9 条 findings 给结论：
   - 每条写：结论（已修 / 已登记到哪张卡 / 仍未做）+ 依据（**已入库、盘上仍在**的证据路径或 commit）
     + 若「仍未做」写明归宿卡号。
   - 引用证据前先自查该路径在 HEAD 里存在（`git ls-tree`/`Test-Path`）——上一轮台账就栽在引用了从未入库的
     `tests/loadgate/load-20261011-110129`、`111850`、`_r4/diag*` 上。
   - 已知要点（自己核，别照抄）：① 三档 DoD 与 tier-500/1000 登录爬坡 ⇒ 后续卡两连跑证据
     `tests/loadgate/load-20261011-121254`、`121953`；② 冒烟 3 次运行 1 绿 2 红（2/3）这条**必须如实写**
     （不要保留「修后 3/3」的说法）；③ 「第三条假人没走上登录」与 tier-500/1000 掉包收窄到服务端侧，
     作为**仍未定位**项写清（附观测建议），别含糊。
3. 把新 head（`c54cd30c`，或你本轮为回复新增的 docs-only commit）以
   `patch --base refs/heads/master --head refs/heads/worktree-c6` + `status: needs-review` **挂到原卡**，
   让复审者能在原卡上闭环（后续卡保持它自己的 needs-review 不动）。
4. 顺手收掉两条可直接做的 Minor（若尚未做）：
   - `run_bots.ps1` 改读 `load_stats.ndjson` 的 `spawned`/`login_ok`（别再用「帐号登录成功」日志计数：
     T-5⑤ 之后有剩余时长的账号只打「个人帐户的期限」会被误判登录失败 ⇒ 假红）；
   - README「基线件清单」把 `load-20261011-030205` 登记进去并注明「部分产物，仅 load_stats 可用」，
     或在第 3 轮结论里写「历史结论更正：383eb916 的根因判断已被 030205 重跑推翻」。

## 2. 门禁

`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets --offline -- -D warnings`；
`cargo test --workspace --offline`；`cargo run -p msgcodegen -- --check`（只改文档也要照跑）。
隔离 target 见 `E:\tmp\openmir2-m2svr-target` 同款做法（任选一个只归本线的目录）。

## 3. 红线

- **不 merge、不签 review**；只由你自己签本卡状态，别动别人的卡。
- 不改 `src/**`(C# oracle) 行为；BotSrv 改动必须 `T-*` 登记并单独提交。
- 不重复起栈（端口正被 rev 车道占用）；不清别人的进程。

## 4. 收工

写 `E:\tmp\openmir2-watch\last-c6f.md`：原卡回复条目 oid、新 patch/status oid、门禁原始结果、未做项与归宿卡。
