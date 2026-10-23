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

10. **返工单必须发给「作者车道」，绝不能发给审查者**（2026-10-23 真发生：`openmir2-sm-skeleton-wiring`
    的 request_changes 被投给了 reviewer-14 的 agent，审查者当场拒收）。理由：reviewer 一旦动手改代码，
    这张卡就**没有合法审查人**了（作者自审不算数），而「让 reviewer 转任作者」还要改 owner、换分支、
    另点复审者，代价远高于重发一条消息。**Reviewer 拒收返工单是正确行为，不是不配合**——它同时应把
    「修法与牙」写进 review body，让作者照单执行。派单前核对：这条卡最近一条 `patch` 的 actor 是谁，
    消息就发给谁的 agent。
    **反向同样成立**：**复审指令绝不能发给作者**（2026-10-23 一小时内两个方向各犯一次：先把返工单发给
    reviewer-14、后把复审单发给作者 worker-54）。发之前先看这条卡最近一条 `patch` 的 actor：要改代码 ⇒
    发给他；要审代码 ⇒ **发给不是他的那个人**。发错后立即发「撤回」并转投，别指望对方自己判断——
    对方若照做，要么是作者自审、要么是审查者改代码，两种都会废掉这张卡的审查链。

11. **合并树门禁可能撞上 stale 测试二进制**（2026-10-23 实测）：合并后首次 `cargo test --workspace` 命中一个 cargo 判定为 fresh 的旧测试二进制，报一条 panic 用例红；`touch` 源文件强制重编后该用例绿、全量 3041/0，源码 sha256 未变。
    ⇒ 门禁报红时**先分三态**：① 真回归（源码确变）；② 环境噪声（stale 二进制 / 并发抢锁）；③ 夹具漂移。判环境噪声的办法：`touch` 相关源文件或换一个空 `CARGO_TARGET_DIR` 重跑一次，**并在 merge_result 里写明是噪声、附两次的 rc**。
    这条与 10-23 早前那次「并发 cargo 抢锁 ⇒ clippy 假红」同族；合并时一律单跑门禁、不要与车道并行。
    **第三种噪声（10-23 17:2x 新增，reviewer-5 实测）**：**共享 `CARGO_TARGET_DIR` 会内嵌旧的 `CARGO_MANIFEST_DIR`**。
    他在共享 target 上跑全量时 `mir2-protocol oracle_vectors` 误报，原因是该二进制编译时嵌的 manifest 路径指向**已被删除的临时 worktree**；
    `cargo clean -p mir2-protocol` 后 2 passed，换成**全新 target dir** 全量即绿。
    ⇒ **复核别人的 head 时用全新 target dir**（`CARGO_TARGET_DIR=/e/tmp/<lane>-review-target`），或在误报后先 `cargo clean -p <crate>` 再判。

12. **并行车道的真正代价不是文本冲突，是签名演化**（2026-10-23 一天两次）：
    ① `emit_script_frames` → `emit_script_frames_raw`：pickwire 加帧时改签名，frame-drain 随后重写排水；
    ② `emit_login_script_frames` 6 参 → 7 参（frame-drain 加了 `fallback: (i32,i32,i32)` 与 `frames: &[SendMsg]`）：
    CHANGEEXP 分支切出得早，复审时 `git merge` 解完文本冲突仍 **clippy/test rc=101**（`E0425 player 未定义`、`E0107 生命周期个数`、`E0061 参数个数`）。
    ⇒ **分支越长命，越要在合并前 rebase 到当前 master 再自证**：文本冲突 git 会告诉你，**签名冲突只有编译器会告诉你**。
    reviewer 的复核必须在**试合并树**上跑完整门禁；作者的自证同理（「分支绿」不是证据）。
    推论：动 `lib.rs`/`npc.rs` 这类热点、且改动会碰到**函数签名**的卡，**不要长命**——要么拆小，要么中途 rebase 一次。

13. **动过 `npc.rs` / `mod.rs` 的卡，合并前跑一次 `script_host_noop_audit.py check --strict-lines`**（2026-10-23 定）：
    默认档只对行号 WARN、永不判红（避免热文件误红），`--strict-lines` 才是「台账没欠账」的机判点。
    当天 `npc.rs` 被 frame-drain / gm-host-seam / CHANGEEXP 连续改，evidence 行号漂移 9~53 处；**strict 绿才说明台账与代码同步**。
    **不要给 WARN 设阈值**（会引入新的假红）；也不要让 strict 常年红（会训练大家忽略门）。

14. **派活前先查「同 principal 是否已有活人在干」**（2026-10-23 20:2x 实测）：我按「返工给作者」把
    `openmir2-gm-impl-shutup-release` 派给 `openmir2-worker-24`（`OpenMir2-wt-cronclaim`），到场才发现
    **同一个 principal 已被另一个会话的 agent 占用**——那边正在 `git merge`（`MERGE_HEAD` 存在、4 文件 UU）、
    跑 `cargo test/clippy/check`，且 90 秒前刚发过签名 comment。
    代价：两条车道抢一个身份，其中一条必须空跑；更糟的是**同身份的两个人互相不能审对方的卡**
    （合并规则只数「不同的、非作者」的 approve），一旦后到者接手，这张卡就没有合法审查人了。
    ⇒ **派活前三查**：① `Get-Process cargo,rustc` 与该 worktree 的最近提交时间；② 该线程最后 2 条 entry 的
    `actor`+`ts`（90 秒内有签名写入 ⇒ 有人在场）；③ `git -C <wt> status` 有没有 `MERGE_HEAD`/未提交改动。
    命中任一条 ⇒ **不要派**，改派到**另一个**空闲 principal（换身份 = 换 worktree），或让它跑完再由
    不同 principal 复审。**「返工给作者」这条（第 10 条）的前提是那个作者身份此刻空闲**。

15. **「打印变红」≠「门禁变红」：自检必须挂在 rc 判定路径之前**（2026-10-23 20:5x，reviewer-5 一轮抓到两处）：
    - `openmir2-window-inventory-fixture`：`_selftest_inventory` 被放在 `if fail_n: return 1` **之后** ⇒ reviewer-5 把「预置整段旁路 / 写库改成 `pass` / 失败 rc 2→0 / 删 std_mode 校验」四种破坏全做完，selftest 仍打印 **171/171 绿、rc=0**。
    - `openmir2-window-rust-binary-evidence`：新增的 4 条检查同样在判定之后 ⇒ 旁路整段接线仍 **169/169 rc=0**。
    ⇒ **牙的判据只能是 rc，不是 stdout**。写牙/审牙时固定问一句：「把它改坏，**退出码**会变吗？」只变文本不变 rc 的牙是假牙。
    推论：新增自检代码的位置本身就是判据——加在 `return` 之后等于没加。review 时看**调用点在哪个分支里**，不看它是否被执行过。
16. **审计链口径：git author ≠ collab principal，不是冒名**（2026-10-23 记录）：本机主检出与多个 worktree 共用一份 `git config user`，所以
    `git log` 里大量提交作者是 `openmir2-worker-39`，而卡片的 owner / 签名条目是 `openmir2-worker-24` / `-70` / `-71` 等。
    **权威身份是 `refs/collab/*` 的签名条目**（每个 worktree 一把 Ed25519，见 §0），git author 只是工作副本的配置噪声。
    引用证据时写「签名条目 actor + oid」，不要把 git author 当作者；也不要因此判定审计链断裂。

17. **mutation runner 就地改写 tracked 文件——绝不能跑在「即将提交的树」上**（2026-10-23 21:4x 实测）：
    我在主检出对合并树跑 `selftest_mutations.py`，它在 `card_window.py` 里留了一条 **live mutation**
    （`verdict_reason: "compare_fail" → "ok"`）、而合并**已 staged** ⇒ 那一刻提交就会把这条变异一起提交。
    发现方式：`git status` 出现 `MM`（staged + unstaged 同时改同一文件）、`git diff` 看到一行语义反转。
    ⇒ **规则**：① mutation/牙 一律在 **worktree 副本**上跑（`E:/tmp/<lane>-teeth`），不在主检出跑；
    ② 每次提交前 `git diff` 必须为空（`MM` 就是警报）；③ 若 runner 被中断，**先 `git checkout -- <file>`
    还原**，再比 `sha256sum <file>` 与 `git show :<file> | sha256sum` 一致才提交。
    与第 11 条同族：门禁工具本身会弄脏树（stale 二进制是另一例）。
    **补充（2026-10-24 00:4x，reviewer-3 自曝）**：**还原方式本身也会弄脏树**——它用 Python 文本写回还原变异，
    Windows 下把 LF 翻成 CRLF ⇒ 工作树变脏、sha 变化。⇒ **还原一律 `git checkout -- <file>`**（从 index/提交恢复），
    绝不用文本写回；还原后必须 `sha256` 与 `git show :<file>` 比对一致。

18. **worktree 的身份是固定的：reviewer 的 worktree 绝不能拿来干 worker 的活**（2026-10-23 22:3x 实测）：
    卡 `openmir2-start-server-whatif-sha`（worker 卡）被 **`openmir2-reviewer-5`** 用 `card_claim` 认领，
    而且是在 `E:/tmp/wt-reviewer-5` 里签的——那个 worktree 的 Ed25519 身份就是 reviewer-5，
    **谁在里面操作都会签成 reviewer-5**。本轮 reviewer-5 自己撞上了：它的 worktree HEAD 被切到
    `card/start-server-whatif-sha`，它只能在独立 detached worktree 里核完、再回原 worktree 签名。
    两个后果：① **角色混淆**——reviewer 当作者，这张卡就没有合法审查人（作者不能自审）；
    ② **身份串用**——别人借这个 worktree 就会以 reviewer-5 的名义签名，审计链直接失真。
    ⇒ **认领前先核身份**：`git -C <wt> rev-parse --git-dir` 下的 `walgit/identity` 的 `principal`
    必须与你这次要扮演的角色一致（worker 卡 → `wt-worker-*` + worker principal）；
    reviewer worktree 只用于 review，且 review 时**不要长期切分支**（detached 核完即回）。
    这类 claim 我按队列第 9 条（裸 claim 无跟进）直接撤销并退回待认领。

19. **派同身份 agent 前的第 4 查：这条线程最近几分钟有没有同 principal 的写入**（2026-10-23 22:3x 事故）：
    我按第 14 条三查（commit 时间 / 脏文件 / cargo 进程）判 `OpenMir2-wt-cronclaim` 空闲，派了
    `openmir2-worker-24` 去做 `openmir2-r-evidence-registry-sweep` 的返工；**同一条外部车道当时也在用
    同一个 worktree/身份**，结果线程在 `5b1aca66` 处分叉（两个写入者互相看不到），reviewer 判
    request_changes「签名链不干净」。
    ⇒ **第 4 查**：`walgit collab thread <card>` 看**该卡线程**最近 N 条 entry 的 `actor`+`ts`；
    若同 principal 在**最近 10 分钟内**有过写入（尤其是 comment/patch），视为在干活，**不派**。
    三查看的是 git 层，第 4 查看的是**协作层**——两层都要查。
    **追加操作（2026-10-24 01:2x，一晚四次同卡双写换来的）**：`shutup-release` 返工误派、`sweep` 同 principal 双车道、
    `script-user-account` 双修、`window-batch-plan-refresh` 双返工——**四次里有三次是「原车道在我派活之后才动手」**。
    ⇒ **派返工/接管前，先在该卡线程发一条签名「接管声明」**（写明：本卡由 `<principal>` 接管、原车道请停写、
    已有改动请让渡或说明），**再开工**。这条声明不是仪式：它让原车道在下一次读线程时**必然看到**，从而收敛。
    同理，**合并前先看线程有没有第二条并行分支**，别把两版都合进去。

    连带口径（与第 16 条一起读）：`git author` 是共享工作副本配置（`OpenMir2-wt-cronclaim` 的
    `user.name` 就是 `openmir2-worker-66`，谁提交都显示 worker-66），所以**author ≠ 签名者不等于代签**；
    但「同一 principal 两条车道并发写同一张卡」本身就是污染，与 author 字段无关，必须重交付、重签。
    干净的交付 = **一号一车一 worktree**：签名 principal、worktree 的 `walgit/identity`、提交所在 worktree 三者一致。

20. **长命 agent 会耗尽上下文——每张卡起新 agent，不要 resume 一个跑了很多卡的 agent**（2026-10-24 01:0x）：
    `openmir2-worker-69` 的 agent 今晚连做了 6+ 张卡（场景、登记批、GM 5A、夹具、审计），
    最后在派第 7 张时**因上下文超限（请求 1,060,113 tokens > 1,048,576）直接失败**，
    且失败前没有任何提交——白跑一次派单。
    ⇒ **同一个 worktree 可以复用，但 agent 不要复用**：每张新卡 `Agent(...)` 起新实例（新实例会重新读卡面与 worktree 状态）；
    resume（`SendMessage`）只用于**同一张卡的返工**，且返工超过 2 轮就换新实例并把「已确认的结论」写进 brief。
    同理：**给 agent 的 brief 里不要塞整段历史**，只给结论 + 文件路径。
    **实测阈值（2026-10-24 01:2x，一晚三例）**：`worker-69`、`reviewer-5`（两次）都因 1,060k+ tokens 爆掉；
    共同点是**每个实例都被塞了 2–4 张卡**。⇒ **一个实例只做一张卡**（review 也一样：一次一条 review，别打包 2–3 张）；
    需要清积压时**并行起多个新实例**，不要往一个实例里塞。

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

**⚠️ 2026-10-23 20:3x 实例（这次真的误判了，幸而 finisher 自己发现并撤了）**：
`openmir2-script-sendmsg-host` 车道转录 mtime 停在 20:07:35，我据此判死（22 分钟 > 20 分钟）并起了 finisher；
实际那条车道 **20:30:18 又提交了 `74555d3ce`（10 文件 1358 行）并正在 rebase 到新 master + 并发跑四个 cargo**。
finisher 到场看到活进程后拒绝接手，才没把 rebase 打断。
⇒ **判死三查缺一不得判死**，且必须在**判死那一刻**重跑（不是几分钟前的快照）：
1. `Get-Process cargo,rustc` —— **只要有进程在跑就不许判死**（这条最强，优先级高于 mtime）；
2. worktree 里 `git log -1 --format=%ci` 与 `git status --porcelain` 的**文件 mtime**（提交可能刚发生）；
3. transcript mtime **仅作辅助**，阈值 **40–45 分钟**，不是 20。
`--out-dir` 指向该 worktree 的 target 也说明车道活着（`Get-CimInstance Win32_Process` 可看命令行）。

## 5. 窗口批处理：一条命令收 R 卡窗口项（`tools/conformance/card_window.py`）

**这一节取代** `tools/conformance/README.md` 里「8 个参数拼 `run_dual.sh`」的写法
（`run_dual.sh` 本身仍是底层语义参照，**不改**；新工具按同一顺序编排，并补上端口预检、
就绪等待、统一产物与 ledger 回执）。

```sh
# 密码只走环境变量，不写进命令行历史
CONFORMANCE_PASSWORD='<口令>' python -I -B tools/conformance/card_window.py \
  --card <卡 id> \
  --scenario tools/probe/scenarios/<场景>.json \
  --out E:/tmp/conformance/runs/<窗口名> \
  --profile alt \
  --post-comment        # 可选：跑完在该卡发一条签名 comment（只发 comment，绝不改卡状态）
```

编排顺序（与 `run_dual.sh` 同语义）：

```
restore 基线 -> 起 C# 栈 -> 等就绪 -> probe -> 抓库 -> 停栈
             -> restore 基线 -> 起 Rust 栈 -> 等就绪 -> probe -> 抓库 -> 停栈
             -> compare -> 归档
```

### 5.1 profile

| profile | 含义 |
| --- | --- |
| `alt`（默认） | 2026-10-23 双栈窗口的备用端口剖面：`E:/tmp/conformance` 私有根、同一组 15xxx/17xxx 端口、测试库 `mir2_test_*` / `mir2_test_*_r` |
| `csharp-default` | C# 默认端口栈（`E:/MirServer`，端口 5000/5100/5500/5600/5700/6000/7000/7100/7200）。它指向**生产库**（`mir2_db`/`mir2_data`/`Mir2`），**默认拒绝运行**；只有 owner 授权的生产窗口才加 `--allow-production-db` |
| `<路径>.json` | 自定义 profile（`format = card-window-profile/1`）：两侧 `stack/stop/restore/db/ports/ready_ports/binaries/probe_extra` 全在文件里。自定义 `db.command` 必须同时用 `db.databases` 声明它读的库名（静态安全判定用） |

`stack` / `stop` / `restore` / `db.command` / `probe` 必须是**非空字符串数组**（写字符串 / `[]` / `null`
在加载期就 rc=2）——停栈 argv 非法绝不能被当成「已停」。

账号/服务器名/角色名可用 `--account`、`--server-name`、`--char-name` 覆盖，或走
`CONFORMANCE_ACCOUNT` / `CONFORMANCE_SERVER_NAME` / `CONFORMANCE_CHAR_NAME`；密码**只**认
`CONFORMANCE_PASSWORD`。

### 5.2 产物（固定布局）

```
<out>/csharp/frames.ndjson   <out>/csharp/run.json   <out>/csharp/db/   <out>/csharp/db_snapshot.json
<out>/rust/frames.ndjson     <out>/rust/run.json     <out>/rust/db/     <out>/rust/db_snapshot.json
<out>/report.json            # compare_runs.py 原样报告（帧 + 库后态）
<out>/card-window.json       # 编排摘要（两侧 rc、命令全文、sha256、verdict）
<out>/card-window.log        # 逐条命令 + stdout/stderr
<out>/README.txt             # 人读：两侧二进制 sha256、场景 sha256、命令全文、时间戳、verdict
```

`<out>/report.json` 只可能属于**本次**运行：真跑要求显式 `--out`（省略 => rc=2，绝不写进 cwd）；
若 out 里已有上次产物，开跑前整体归档到 `<out>/_stale-<ts>-<pid>/`，失败时也**无条件**写本次的
fallback `report.json`（`format=card-window-report/1`, `verdict=UNDECIDABLE`, 带 `run_id`）。

`--post-comment` 的 body 顶层只有一个**三态** `verdict`（compare FAIL => `FAIL`；无结论或证据
缺件 => `UNDECIDABLE`；完整且 PASS 才 `PASS`），并带 `verdict_reason` / `blockers` / `exit_code` /
`report_kind` / `finalize_ok`；compare 的事实（`verdict` / `rc` / **帧数** / `db_state` / `diffs` /
`undecidable` / `whitelisted`）在**嵌套 `compare`** 里，顶层**没有** `report_verdict` / `frames`
——只扫顶层 `PASS` 的读者不会把缺件运行误读成「窗口通过」。body 另含两侧二进制 sha256、场景
sha256、证据路径与时间戳。comment 在 report.json / README.txt / card-window.json 全部写完之后
才发；**只发 `kind=comment`，不写 `status`**，因此看板列不变。

### 5.3 fail-closed 口径（退出码 0 = PASS，其余一律 2）

- **起每一侧栈之前**检查该侧端口；被占 => rc=2 并点名 `端口(pid N)`，**不启动栈**。
- 起栈命令 rc=0 之后仍**轮询就绪端口**；端口不监听 => rc=2（**栈没起来绝不等于对拍通过**）。
- `frames.ndjson` 为空 / 0 帧 => rc=2；`run.json` 必须非空且 `exit.code == probe rc == 0`；库快照失败或缺 `manifest.json` => rc=2。
- **停栈在 finally 执行**：恢复/起栈/probe/抓库任一步失败都会尝试停栈；A 侧没跑完不起 B 侧。
  **起过栈但停栈没跑成（`stop_rc=None`）或 rc!=0 一律 rc=2**，绝不当成功。
- **不碰 MySQL 服务本身**：停栈命令先归一化（去 `.exe`、折叠大小写/空白）再按效果判 —— 任何
  mysql/mysqld 提及、或服务控制族（`Stop-Service`/`spsv`/`net stop`/`sc stop`）直接拒绝。
- **生产库默认拒绝（结构性判定）**：把 `preset` / `tables` / `allow_db` / `databases` / `db.command`
  的 `--db`/`--table` / env 的 `database=` **展开成最终库名**再判；`prod` 预设、`mir2_db`、
  `mir2_account`、`mir2_data`、`Mir2` 均算生产库；无法判定为测试库的未知库名同样保守拒绝。
- compare 的 FAIL(1) / UNDECIDABLE(2) 都映射成工具 rc=2；真实 `compare_rc` 记在 `report.json` 与 `card-window.json`。

### 5.4 不真的开窗也能验

```sh
# 只打印完整命令序列：不起栈、不建目录、不需要密码
python -I -B tools/conformance/card_window.py --card X --scenario S --out O --dry-run

# 离线自检（桩命令 + 合成 frames；62 条断言，含端口占用/起栈失败/就绪失败/差分/停栈 argv 非法/
# stale out/生产库 preset/net.exe 停 MySQL/缺 --out/run.json exit.code 牙）
python -I -B tools/conformance/card_window.py --selftest
```
