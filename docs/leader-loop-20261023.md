# 协作模式切换：Leader 循环（owner 2026-10-23 裁决）

## 角色

| 角色 | 谁 | 职责 |
| --- | --- | --- |
| **Owner** | 人 | 派活、拍板（冻结点/端口/优先级） |
| **Leader** | 本会话（console agent） | **拆卡 → 派车道 → 独立复核 → 合并 → 收卡**；不亲自写业务代码 |
| **Worker 车道** | 子代理（一次性） | 一个 worktree / 一个 principal / 一张卡；只交付到 `needs-review` |
| **Reviewer** | 子代理（不同 principal） | 钉住 head 自跑门禁与牙，出 `review`（带 tier / tier_why） |

## 触发：cron `*/5 * * * *`（会话内，7 天自动过期）

每次巡检**只做一件事**，优先级：合并 > 派活 > NOOP。硬约束：

- 一次只处理**一张卡**；在跑车道 < 3 才派新活；
- 复核者 principal 必须 ≠ 作者（reviewer-3 / reviewer-6 轮换；worker-5 / worker-4 轮换）；
- **同一时刻只跑一个** workspace 门禁；合并前必须 fmt / clippy / test --workspace / msgcodegen 全绿；
- 不改 `src/**`（owner 批准 + whitelist 登记的唯一例外：T-172 SelGate 端口）；
- 不起服务端/客户端/代理、不占端口、不写生产库；R 级真机窗口项只登记不跑；
- R 级卡 merge 后保持 `in-progress`，L 级可 `closed`；连续 3 次 NOOP ⇒ 输出 needs-human 清单。

## 首个用该模式跑的批次

- `openmir2-login-529-status-text`（R）：529 服务器列表状态段 `Idle` vs `0`，由对拍台首跑发现；
- `openmir2-conformance-harness` 规则表补齐 + `run_dual.sh`（L）。

## 判据分级仍然生效

Leader 合并时按 `TIER-POLICY.md`：R 级必须有真机证据或显式 `window_required`；L 级需等价证明 + 抽样冒烟。

## 清理规则（owner 2026-10-23 更正）

**合并后只删分支，不删 worker 的 worktree。**

- 删：本地分支（`git branch -d`）、远端分支（`git push walgit --delete`）、合并后的临时 `*-target` 构建缓存；
- **保留**：`/e/tmp/wt-*` 等 worker worktree 目录——里面是 `refs/collab` 的签名身份
  （`<git-dir>/worktrees/<name>/walgit/identity`，一个 worktree = 一个身份 = 一把 Ed25519 私钥）；
  删掉它等于抹掉该 principal 的身份与可重放性。
- 新车道优先复用身份匹配的既有 worktree，而不是重建。

## 身份治理（Leader 2026-10-23 裁决）

- `openmir2-reviewer-5` 的唯一归属 worktree = `E:/tmp/wt-reviewer-5`；其他位置签出的 reviewer-5 视为历史，**不计入合并规则的 approve 数**。
- **主检出** `.git/walgit/identity` 已由 reviewer-5 重绑为 `openmir2-orchestrator`（Leader），旧文件备份 `.git/walgit/identity.bak-reviewer5`；此后从主检出默认签名 = Leader。
- 每个车道：worktree 身份 = 执行者 principal；Leader 只签 orchestrator；review 只能由 reviewer-3 / reviewer-6 出；合并只删分支、保留 worktree。
- 定规前由 reviewer-5 代签的 card_claim（`openmir2-gm-batch-4-numeric`、`openmir2-dualrun-triage-enterworld-frames`）保留留痕但不作认领依据，进复核前须由实际车道用自己的 principal 重发 status。

## 交叉审查（owner 2026-10-23 追加）

Leader 忙不过来时，**空闲 worker 可以互相审查**：唯一硬约束是「复核者 principal ≠ 作者」，
且复核者必须钉住 head、自跑门禁与牙、给完整 findings。专职 reviewer（reviewer-3 / reviewer-6）不再是唯一来源。

## 分支清理的例外

「只删分支」的前提是分支**没有**被保留的 worktree 检出。若该 worktree 仍 checkout 该分支
（如 `wt-triage2 @ dualrun-triage`），则**分支与 worktree 一起保留**，不得 `git branch -D` 强删。

## 台账纪律（2026-10-23 教训）

- **永远从线程输出里整段复制 oid**，绝不凭前缀补全：凭前缀猜后缀会造成 `parent` 指向不存在的条目（链断裂）。
  今天因此产生 2 条断链（orphan-encoder、gm-batch-4），均以「更正条目」接回；以后 `parent` 一律用
  `walgit collab thread <id>` 输出的最后一个 `"oid"` 原值。
- 并发会话可能在同一张卡上抢写：Leader 每周期的「进度一行」必须核对**同一卡是否有多个 claim**，
  按「先交付者优先 / 空 worktree 视为未开工」裁决，并把裁决写成签名条目。

## 并发车道共享临时文件的坑（2026-10-23）

复核者曾把 review body 写到通用路径 `/e/tmp/review-body.json`，被并发车道覆盖，导致一条 review **投错线程**（正文是别的卡的）。
**规则**：任何车道/复核者的临时文件必须带车道唯一前缀（`/e/tmp/<lane>-review-body.json`），禁止使用通用文件名。

## 合并前必须断言当前分支（2026-10-23 事故）

**事故**：一次冲突消解把主检出切到 `name-neg-rebased` 后没有切回，之后 5 次合并（searchtick / closehealth / verif-tick / 文档）都落在该分支，
而 `git push walgit master` 一直在推**旧 master**（远端停在 `f164ccd2`，直到被 `git ls-remote` 发现）。
修复：`git checkout master && git merge --ff-only name-neg-rebased && git push walgit master`（`f164ccd2..e0114909e`），内容无丢失。

**规则**：每次 `git merge` 与 `git push walgit master` **之前**先跑
`git rev-parse --abbrev-ref HEAD` 断言 == `master`；合并后立刻 `git ls-remote walgit refs/heads/master` 与本地 `rev-parse master` 对比，不一致即红灯处理。

## 门禁优先级：Leader 合并门禁优先（2026-10-23）

多条 Rust 车道同时跑测试时，「同一时刻只跑一个门禁」会让**合并门禁被饿死**（实测排队 25 分钟未启动）。
**规则**：Leader 的合并门禁**抢占优先**——直接开跑，不等车道；若因此撞到已知的两条 flake
（`game-gate relay::gm_checkclient_is_sent_every_keepalive_period`、`relay::bad_login…`），
**单跑复验通过即视为绿**，并在 merge_result 里如实注明「并发负载下 flake，单跑绿」。

---

# 深夜补充（2026-10-23 23:4x，master `512d9ecf5`）

> 本节是**会话重启后最先读的一段**：它记录本日晚场新增的协作规则、当前状态快照、
> 以及「不要重蹈的六类坑」。动态数字一律带快照戳。

## A. 当前状态（截至 `512d9ecf5` @ 23:4x）

| 项 | 值 | 复算命令 |
| --- | --- | --- |
| 四条件 | ① pass（`--full`）② **fail** ③ pass ④ pass（`--full`） | `python -I tools/goal/goal_gate.py --json`（`--full` 含 cargo test） |
| 条件② R 卡 192 | 已完成 10 / `window_required` 121 / whitelist 25→35（批 3 已合）/ 已裁定 3 / **三者皆无 1** / merged 未 closed 且无登记 **17**（批 4 在跑）/ 其它需人工 5 | 同上 |
| 唯一 FAIL 卡 | `openmir2-gm-batch-5-guild-castle`（A 片 6 条已派 `openmir2-gm-batch-5a-guild-impl`） | 同上 |
| 窗口 | runbook §0.5 有 60 秒自查清单；`preflight.py` 卡在待认领；**真窗口需 owner 授权** | `docs/window-runbook-20261023.md` |
| 需 owner 决策 | 6 项 | `docs/needs-human-20261023-night.md` |

## B. 今晚新增的六条纪律（都是踩坑换来的，全文见 `docs/leader-queue-20261023.md`）

| # | 一句话 | 触发实例 |
| --- | --- | --- |
| 14 | 派活前查「同 principal 是否已有活人」 | 把 shutup-release 返工派给正在忙的 worker-24 |
| 15 | **牙只认退出码**，自检必须在 rc 判定路径之前 | inventory/rust-binary 两张卡的自检放在 `if fail_n: return 1` 之后 |
| 16 | `git author` ≠ collab principal（共享 config 噪声） | 大量提交作者显示 worker-39/66，签名者是别人 |
| 17 | **mutation runner 就地改写 tracked 文件**，绝不在即将提交的树上跑 | `selftest_mutations` 留下 live mutation，合并已 staged |
| 18 | **reviewer 的 worktree 只能用于 review**（一 worktree = 一身份） | worker 卡被 reviewer-5 认领并签在 `wt-reviewer-5` |
| 19 | 派同身份 agent 前加**协作层第 4 查**（该卡线程最近 10 分钟有无同 principal 写入） | 我派的 worker-24 与外部车道在 cronclaim 撞车，线程分叉 |

## C. 六类「假绿/假红」（`docs/session-summary-20261023.md` 已扩到 9 类，这里只列今晚新增的三类）

1. **自检在 rc 判定之后** ⇒ 实现整段旁路仍 GREEN rc=0；
2. **新校验判错因果** ⇒ R6 把「越界 tag」当「必然超时」，实际两侧都是先取目标后写 nDir（降级 WARN）；
3. **工具弄脏树** ⇒ mutation runner / stale 二进制 / 共享 `CARGO_TARGET_DIR` 内嵌旧 `MANIFEST_DIR`。

另有两类是**数据层**的：**7 条规则被全局 `recog` 掩码吞掉语义字段**（`213`/`766` 曾「什么都不比」）、
**`rules.json` 缺 202/203/642/709** ⇒ 已合场景的窗口必然 UNDECIDABLE。

## D. 重启后的前三步

1. `git fetch walgit '+refs/collab/inbox/*:refs/collab/inbox/*'` + `walgit collab board`（**看板是权威，本文只是镜像**）；
2. 读本文件 A/B/C 三节 + `docs/needs-human-20261023-night.md`；
3. 按「合并 > 派活 > 拆卡」继续；**待认领 0 且无卡可派时**，下一批仍是
   `openmir2-r-window-registry-batch`（第 4 批起，≤10 张/批，名单每次现取）。

## E. 明确不要做的事

- 不要**凭记忆**引用行号/数字：今晚已发现多处抄错（`lib.rs` pose_dir 实为 `:8828` 而非 8836、
  `GMEXECUTE` 去重是 16 名不是 5 名、`332,317` 不是 `332,318`）——一律现取。
- 不要把「不可跑」当「欠窗口」记账（两者混在一起会让窗口排期整体失真）。
- 不要在 `OpenMir2-wt-cronclaim` 或任何 reviewer worktree 里派 agent（纪律 18/19）。
