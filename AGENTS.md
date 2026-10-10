# AGENTS.md — OpenMir2 协作与开发规范

本文件是**所有 AI agent（含子代理）在本仓库工作时的强制规范**。它是 host 规范
`http://walgit.localhost:8081/SKILL.md`「Agent collaboration standard (normative)」的本仓库落地版；
项目背景与逐批细目见 `docs/Rust重写设计文档.md`（尤其 §12.4 并行前提、§14 协作流程、§14.3 收尾清单）。

> **权威来源**：规范以 host 的 `SKILL.md` 为准，本文件只做收敛与项目特化；冲突时以 host 原文 + 本仓库 §14 为准。
> 取原文（本机 `walgit.localhost` 不解析、服务端又重定向到该名，必须 `--resolve` 绕过，勿改 hosts）：
>
> ```sh
> curl.exe -sS --resolve walgit.localhost:8081:127.0.0.1 http://walgit.localhost:8081/SKILL.md -o SKILL.md
> ```

## 项目目标（开工前必须知道）

**一句话目标**：把 OpenMir2（C# 的经典 Mir2 服务端）用 Rust 重写，让现存的经典客户端（MirClient/Delphi）
与现存的游戏内容（MySQL 三库 + Envir 脚本）原样可用，验收方式是**行为与 C# 参照实现逐行/逐字节等价**。

**三条硬约束（不可协商，违反即作废）**：

1. **客户端冻结**：`mirbeta/MirClient`（Delphi）不改一行，协议逐字节兼容。
2. **内容冻结**：不改表结构、不改脚本语法、不改数据文件格式。
3. **行为等价优先于修 bug**：先 1:1 连 bug 一起搬，对拍全绿后再单开批次修。

> 完整判据、里程碑与门禁见 `docs/Rust重写设计文档.md` §1（目标与硬约束）、§6（里程碑与验收）、§7（测试与门禁）。

---

核心原则一句话：**协作只在 `refs/collab/*` 的签名线程里发生。** agent 之间不私聊、不拿共享临时文件当事实源；
线程是唯一共享记忆，任何人重放 refs 都得到同一视图。

---

## 0. 身份与首次接触（§0 / §0a）

- **一个 worktree = 一个身份 = 一把 Ed25519 私钥。** 身份来自 `walgit collab join`，落在
  `<git-dir>/walgit/identity` 与该 git dir 下的 key（**不入库、`git clean -fdx` 删不掉**）。
- 每个 worktree 开工前先跑一次（`--key` 指向**已有种子文件路径**，绝不把种子内容贴到命令行/历史里）：

  ```sh
  walgit collab join --repo <worktree> --principal <proj>-<role>-N --key <既有种子> --push walgit
  ```

  注册完成后再执行命令即可**省略 `--actor` / `--key`**（自动从 identity 解析）。
  注册被拒 = 硬停：不得用未注册的 principal 写条目。
- **禁止共用密钥 / 代签。** 若作者、审查者、合并者都签同一个 principal，看板只剩一个 owner，
  独立审查与审计链全部失效。永远只写自己的 inbox（`refs/collab/inbox/<principal>/*`），
  验证时 `actor != inbox` 的条目一律视为不可信。
- **本仓库 principal 池**（coordinator 已注册；钥匙在 `~/.walgit/keys/<principal>.ed25519`）：
  `openmir2-coordinator`、`openmir2-worker-1..N`、`openmir2-reviewer-1..N`、`openmir2-svc-*`。
  审查者 principal **不要以 `svc-` 开头**——`merge_rule_eval` 会把 `svc-*` 从人类批准里剔除。
- **首次接触例行四步**（读板/抢卡/动文件之前先做完）：
  1. 读命名约定：`git for-each-ref refs/collab/meta/principals` 找 `<proj>` 前缀与角色模式；
  2. 沿用本 worktree 已有 identity，否则取该角色最高编号 +1 的下一个空闲名；
  3. `walgit collab join`（存在则采用、不存在则生成）；
  4. 同步协作视图：`git fetch origin '+refs/collab/inbox/*:refs/collab/inbox/*' '+refs/collab/meta/*:refs/collab/meta/*'`，
     再 `walgit collab board`，然后才谈认领。

## 0b. 并行拓扑（§0b）

默认等式：**N 个在跑的 agent = N 个 principal/密钥 = N 张卡（thread）= N 个 worktree/分支**。

- **一张卡同一时刻只有一个 owner**；最近一条 `status` 里的 `owner` / `worktree` / `branch` 就是认领账本，
  交接必须靠新的签名 `status`。两个 agent 不许做同一张卡——要并行就**新开线程**。
- **审查必须是不同 principal**：`review` 的签名者既非 patch 作者、最终审查也非合并者；作者自审不算数。
  合并规则计的是**不同的、非作者、已验证**的 approve。
- **按写集切分，不按标题切分**：文件/接口/评审面重叠的改动要串行；同一文件、schema、迁移一次只动一路。
- 常见形态：**2–4 workers + 1–2 reviewers + 1 coordinator**；coordinator 的写集尽量最小，保证合并路径易重放。

## 0c. 无人值守（§0c）

唤醒原语是 `walgit collab watch`：把 `refs/collab/*` 变化变成进程调用（trigger，不是 agent）。

- 事件只当**重新读取的触发器**，绝不是真相快照：收到后先 `walgit collab thread <id>` 重读、以 head oid 确认
  卡片仍无主或归你，再用签名 `status` 认领——**认领即互斥锁**，两个 agent 抢同一张卡靠"先重读"收敛。
- `--exec` 契约（**必须遵守**）：每个新/变更 ref 调用一次，原始 entry JSON 在 stdin，五个变量
  `WALGIT_COLLAB_REF/KIND/THREAD/ACTOR/VERIFIED` 从解析后的 entry 来（body 里写 `kind=` 无法伪造）。
  **handler 只做 park + return 0**（落到队列，比如 `sha256`/`hash-object` 命名），重活交给 worker；
  **非零退出会终止整个 watcher**，at-least-once 因此依赖你的 supervisor 重启。
- 三个坑：① **自触发**——自己的写入也会回来，用 `WALGIT_COLLAB_ACTOR` 过滤自己的 principal；
  ② **hook 里跑长任务**——循环串行阻塞，一个失败拖垮整个 watcher；
  ③ **未验证输入**——`WALGIT_COLLAB_VERIFIED=false` 不许行动，也不许当自己的卡。
- **本机（Windows）用 `--once` + 计划任务**：不要用控制台程序做常驻 watcher（D53）；
  `collab watch --once` 交给计划任务，失败批次下次 tick 自动重放。

## 0d. 自主交付（D59，§0d）

人类只做两件事：**派活**与**观察**；验收由**验收子代理**判定，干净的工作**自动合并**。

- 角色：**Human**（一个控制台，派卡 + 看看板，只在 `needs-human` 时被拉入）、
  **Orchestrator**（控制台 agent 或其启动的常驻循环：观察 refs、派发、起/收子代理、记录合并，纯客户端无服务端状态）、
  **Worker 子代理**（一卡一 owner 一 worktree 一身份）、**Acceptance 子代理**（与作者不同 principal，
  对卡片的**机器可判验收**独立评审）。
- 流程：assign(issue) → dispatch(签名 claim) → work(patch + needs-review) → accept(独立 review)
  → **无 Critical/Important 且机判全绿则自动合并**（merge_result + status: closed）→ 阻塞项回流 worker 或 `needs-human`。
- 子代理用宿主自身的后台能力，不与其它子代理共享 checkout 或密钥；orchestrator 负责生命周期、超时、预算、崩溃重启、清 worktree。

## 1–7. 工作单元 / 分支 / 协议 / 审查 / 合并 / CI / 纪律

1. **工作单元 = 一条线程**：`issue` 必须写清目标、角色、唯一 owner、**机器可判的验收判据**；大任务拆子线程
   （`--related` / `--depends-on`，越界引用会在 thread 视图报 `broken_refs`）。
2. **改树**：绝不编辑共享检出。`git worktree add .worktrees/<unit> -b <unit> <base>`，本地提交后
   `git push <remote> <branch>`，再补 `--kind patch --base … --head …` 把交付挂到线程上——**评审人读的是 diff/线程，不是聊天**。
3. **线程协议**：**先读后写**（以上一条 entry 的 oid 作 `--parent`，绝不凭记忆作答）；用 kind 而非散文表达状态
   （`status` / `review` / `merge_result` / `comment`）；`needs-human` 只留给真正需要人的事（授权、优先级、外部输入），
   可判的技术/产品问题必须自己判完并记 `comment`——**把可决问题挂起就是卡壳**。
4. **审查**：审查者与作者不同 principal，且自己在**钉住的 commit** 上跑命令；`review` 要写**完整 findings**
   （位置 / 问题 / 建议 / 可复现验证），"无证据的 approve" 视为噪声。`request_changes` → 作者按点改并在线程逐条回复 → 重审 → `approve`。
5. **合并与归档**：批准后本地合并（优先 fast-forward）、推回本 host，写**一条** `merge_result {"merged":true,"oid":…}`，
   再用 `status: closed` 收卡片。面向人的报告作为文件进仓库（`docs/`），不要只留在 thread body。
6. **CI（可选）**：`.walgit/ci.toml` 声明任务；runner `walgit ci run --once` 认领、执行、回签结果；合并前必须绿。
7. **纪律**：优先机器可读字段；一个 patch 一个逻辑变更；不改写已推送历史而不声明；不写别人的 inbox；
   不信任未验证条目；遵守 per-repo push policy（policy 管"谁能写"，签名管"谁签了"）。

---

## 本项目落地细则

### 远端与看板

- 代码主源 = GitHub `origin`；walgit 承载 D1 协作层，remote 名 = `walgit`
  → `http://127.0.0.1:8081/gqf2008/OpenMir2.git`。
- 看板列定义在 `.walgit/board.toml`（改它走提交 + 评审）；**移动卡片 = 追加一条签名 `status` 条目**，
  只通过签名条目移卡，绝不编辑 board、状态文件或别人的 inbox。
- 抢卡示例（worktree 内可省略 `--actor` / `--key`）：

  ```sh
  walgit collab entry --repo . --kind status --id <thread-id> --actor <你的 principal> \
    --parent <该线程最后一条 entry 的 oid> \
    --body '{"status":"in-progress","owner":"<principal>","worktree":"<wt 名>","branch":"worktree-<wt 名>","work":"<一句话>"}' \
    --push walgit
  ```

- 标准流转：`issue` → `status: in-progress` → worktree 干活 → `patch(--base/--head)` → `status: needs-review`
  → **另一 principal** `review` → coordinator 合并并推 `origin` + `walgit` → `merge_result{oid}` → `status: closed`。
- 无人值守：`walgit collab watch --once`（§0c）；巡检跑 `E:\MirServer\port-status.ps1` + `walgit collab board` + `walgit collab report`。

### 每批收尾清单（Cleanup DoD，缺一不算完）

1. **worktree**：`git worktree remove <路径>`；`git worktree list` 只剩主检出 + 进行中的线；
2. **分支**：已合并的本地/远端删除（`git push origin --delete <分支>`），未合并的写明原因与归宿；
3. **看板**：该卡收口为 `status: closed` 且在【已完成】列——**只写代码不收卡片 = 没收尾**；
4. **进程**：本线起的客户端/服务端/假人/代理进程全部结束，`E:\MirServer` 十端口状态与开工前一致；
5. **临时物**：临时目录/库/抓包中间件/日志清掉或登记到报告（写清路径 + 为何保留）；`E:\tmp` 不留本线专属残留；
6. **冻结基线**：`git status` 干净；`git diff --stat <基线> -- src/ sql/` 为空（例外仅登记过的 `T-*`）；
7. **报告**：patch 带**证据四件套**（命令 + 输出 + 路径 + hash），报告末段列"残留物清单"。

### 本地门禁（合并前全绿；reviewer 自己重跑，不采信作者自述）

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p msgcodegen -- --check   # 消息号表与 C# 源漂移门禁
```

### 编码约定

- **行为等价优先于修 bug**：Rust 侧默认复刻 C#（oracle）旧行为以便对拍；确需偏离的走白名单
  （`T-*` / `D-*` / `B-*`）登记，并记录改前/改后行为。
- 协议层一律按**原始字节**处理文本（线上是 GB2312/GBK 字节流），本层不做 UTF-8 转换；
  展示时由上层显式解码（参照 `HUtil32.GetString` 的 gb2312 语义）。
- 接口冻结线：`crates/protocol`、`crates/storage` 的改动必须通知受影响的线（见 §12.4）。
- 一线一判据、一模块一 owner。

### 已知偏差（记录在案，非忽略）

- 种子实际集中在 `~/.walgit/keys/`，identity 指过去；本项目身份需跨 worktree 复用而 worktree 合并后会删。
  明文规定的红线（一个目录堆多把私钥、agent 互相借用）**未违反**：每条线的 identity 只指向自己那把。
- 合并由 coordinator 执行；**审查自本批起改为 reviewer-1/2 出 `approve`，coordinator 只做 merge**。
