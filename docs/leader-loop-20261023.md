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
