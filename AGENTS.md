# AGENTS.md — OpenMir2 项目协作章程（通用模板）

> **本项目特化规范（必读，优先级高于本文件）**：[`docs/AGENTS.project.md`](docs/AGENTS.project.md)。
> 该文件是 `walgit` host 规范 `SKILL.md` 的本仓库落地版，含：项目目标与三条硬约束
> （客户端冻结 / 内容冻结 / 行为等价优先于修 bug）、principal 与 Ed25519 身份体系、
> `refs/collab/*` 签名线程协议、写集切分与并行拓扑、判据分级 R/L、五门禁、`walgit` 主源与看板、
> 每批收尾清单、已知偏差。**开工前必须先读该文件**；本文件与它冲突时以它为准
> （全局安全规则与人类指令仍最优先）。
>
> 本文件是项目无关的协作章程模板（来自 `team-charter` 技能），§0 参数表已按本仓库填写；
> 未填的以 `<...>` 标出。与全局规则（如 `~/.claude/CLAUDE.md`、`~/.agents/rules/`）冲突时，
> 以全局规则为准，尤其是安全、授权、删除类规则。

## 0. 参数表（复制后先填这里）

| 参数 | 值 |
| --- | --- |
| 项目名 `<PROJECT>` | OpenMir2 — 用 Rust 重写经典 Mir2 服务端，C# 参照实现为 oracle |
| 仓库根 `<REPO_ROOT>` | 本仓库根（`E:/Users/gxh/Documents/GitHub/OpenMir2`） |
| 主分支 `<MAIN_BRANCH>` | `master` |
| 成员 worktree 根 `<WORKTREE_ROOT>` | `<REPO_ROOT>/.worktrees/<role>`（一个 worktree = 一个身份） |
| 成员分支前缀 `<BRANCH_PREFIX>` | `card/<短名>`（按卡片建分支）；历史分支亦见 `worktree-<wt 名>` |
| 构建命令 `<BUILD_CMD>` | `cd mir2-rs && cargo build --workspace` |
| 检查命令 `<CHECK_CMD>` | `cd mir2-rs && cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo run -p msgcodegen -- --check` |
| 测试命令 `<TEST_CMD>` | `cd mir2-rs && cargo test --workspace` |
| 集成测试 `<INTEGRATION_CMD>` | 真机对拍：`mir2-rs/tests/parity` 场景（C# / Rust 双栈），R 级片占用独占窗口 |
| 评审方式 `<REVIEW_CMD/流程>` | 另一个 principal 在**钉住的 commit** 上签名 `review`（`wg collab`），不是作者自审、也不是 `/code-review` 代签 |
| 定时推进间隔 `<TICK_INTERVAL>` | 不适用会话内定时；见 §4 注（外部 watcher + Windows 计划任务） |
| 决策辅助模型 `<DECISION_MODEL>` | 留空 —— 本项目不启用 §6，判定依赖确定性门禁与卡面证据 |
| 跨团队 tracker `<TRACKER>` | `walgit` 看板卡片（即 issue）；GitHub `origin` 仅为镜像，见 §7 注 |

> 注意：`cargo` 命令必须在 `mir2-rs/` 下执行（仓库根没有 `Cargo.toml`）；含 `tools/...` 的命令在仓库根执行。
> 门禁清单与判据的权威定义在 [`docs/AGENTS.project.md`](docs/AGENTS.project.md)「本地门禁」「判据分级」两节，本表只是同一命令的参数化复述。

## 1. 角色

> **治理模型（本项目约定）：Owner 定目标与验收，Leader 做决策。**
> 项目决策权在 Leader；Owner 只回答"要做成什么、怎么算做成"，
> 不介入方案、排期、派工、合并等过程性决策。只有目标不明确时才与 Owner 交换意见。

### Owner（人类：目标与验收责任人）

- 制定目标：要解决的问题、范围边界、成功标准（验收条件）
- 验收结果：按验收条件判断交付是否达标，不验收过程
- 授权与资源：提供账号、权限、环境、预算；一次说清，不逐次请示
- 不参与日常决策：不指定实现方案、不安排具体任务、不审批常规合并

### Leader（架构师兼项目负责人，决策责任人）

- 在目标范围内拥有**完整项目决策权**：架构与方案取舍、优先级、排期、技术选型、
  依赖升级、合并与发布节奏、风险处置
- 任务拆解、派工与节奏管理
- 代码/设计审查、验收与合并分支
- 只有下列情形才与 Owner 交换意见：目标不明确或自相矛盾、验收条件缺失或冲突、
  超出已授权范围（预算/合规/对外承诺）、触发全局安全规则（密钥、删除、强推等）
- 决策留痕：重大决策写明背景、选项、结论与理由（`docs/decisions.md` 或合并提交信息）

**原则上 Leader 不写代码。** 代码、测试、配置、脚本的变更由团队成员完成；
Owner 明确要求时，Leader 可以直接处理非代码文件（如本文件、文档），但仍应优先
交给团队成员。

> 本项目的实际角色划分（Leader / worker / reviewer / coordinator / 验收子代理，
> 以及「审查者与作者必须不同 principal」「coordinator 只合并不出 approve」）见
> `docs/AGENTS.project.md` §0b / §0d / §7。两者冲突时以后者为准。

### 团队成员（子代理）

- 以**子代理**形式工作，每个成员拥有独立的 git worktree 与分支。
- worktree 位于 `<WORKTREE_ROOT>/<role>`，不得放在项目目录之外；分支命名
  `<BRANCH_PREFIX><role>`。
- 只在自身 worktree 内工作，不直接修改主工作区或其他成员的工作区。
- 完成后必须提交，并在最终报告给出：分支名、commit SHA、改动摘要、
  运行的命令与结果、风险或未验证项。
- 成员不得再派生下级子代理，避免任务失控。

## 2. 交付流程

1. Leader 在 `<MAIN_BRANCH>` 上维护可构建、可测试的基线。
2. Leader 拆解任务并创建 worktree/分支派工；任务边界必须互不重叠。
3. 成员在各自 worktree 内实现、自测、提交。
4. Leader 审查 diff 与设计，按验收标准逐项验证。
5. 通过后由 Leader 合并；未通过则附具体失败信息退回返工。
6. 以**可交付**为目标：文档、测试、示例配置与实现同等重要。

## 3. 验收标准

合并任何分支前，Leader 至少执行：

```bash
cd mir2-rs && cargo fmt --all -- --check
cd mir2-rs && cargo clippy --workspace --all-targets -- -D warnings
cd mir2-rs && cargo test --workspace
cd mir2-rs && cargo run -p msgcodegen -- --check
python -I -B tools/hygiene/docs_anchor_gate.py --prose --base <合并目标 ref>
```

> 上述五门在**待合并分支上**跑，`--base` 取合并目标 ref；详见 `docs/AGENTS.project.md`「本地门禁」。
> `<INTEGRATION_CMD>`（真机对拍）是否必须跑由**判据分级 R/L** 决定，判级在 `review` 里显式写 `tier` / `tier_why`。

并通过以下检查：

- 不破坏对外契约（协议格式、公开 API、配置项、工具 schema）；
- 审查发现的 P0/P1 问题已修复，或有明确、可接受的处置结论；
- 新增配置项同步更新文档与示例；
- 新增行为有测试；修复缺陷有回归测试；
- 日志只写 stderr（或项目约定通道），标准输出不被调试信息污染。

## 4. 推进节奏

- Leader 可创建 `<TICK_INTERVAL>` 间隔的定时任务推进项目：检查团队分支、
  审查已完成工作、运行验收、合并，并汇总阻塞项。
- 定时任务仅在当前会话内有效；需要长期维护时重新评估。
- 若没有新进展，只汇报状态，不重复派发相同任务。

> **本项目注**：不使用会话内定时任务。推进由**外部进程**驱动 ——
> `wg collab watch --once` 交由 Windows 计划任务，失败批次下次 tick 自动重放；
> 勿用控制台程序做常驻 watcher。车道运行时见 `tools/orchestration/README.md`。

## 5. 沟通与权限

- Leader 拥有项目所需访问权限，用于构建、测试、审查、合并。
- 影响交付方向的判断属于 Leader 的决策权，自行拍板并留痕；**只有目标不明确时才向 Owner 提问**，并附候选解释与建议。
- **不要求在对话中粘贴密钥**。凭据统一放在约定的密钥存放位置（环境变量、
  本地凭据文件、密钥管理服务），由人类配置后再通知验收。
- 常态的开发性外向操作（推团队分支、建 MR/PR、触发构建、内部联调）由 Leader 决定，不需逐次请示。
- **不可逆或具法律/合规后果的操作**（对外发布版本、删除远端数据、改写历史、强推、对外承诺）
  由 Owner 在目标里一次性授权边界；越界时先确认再执行。

## 6. 决策辅助（默认 Jev）

项目默认使用 **Jev**（TypeSafe System One）作为独立第三方决策参考；可替换为等价
决策模型，但效力与记录规则不变。

- **适用范围**：重大且非确定性的判断——架构方案取舍、分支合并/退回、风险等级、
  优先级排序、是否接受某项变更。
- **不适用**：构建、测试、验签、格式校验、权限门禁等确定性校验必须由代码/命令完成，
  不得用模型替代。
- **调用方式**：优先使用 `jev-decision` 技能
  （`~/.agents/skills/jev-decision/scripts/jev.py` 的 `choice` / `score` / `noul`
  子命令；若会话配置了 jev MCP，则使用对应工具）。MCP 方式先跑 `jev_health`，
  区分网络问题与模型拒绝；一次请求尽量合并多个问题以降低输入 token 成本。
- **输入要求**：`state` 必须包含决策所需全部事实（候选方案、约束、已知风险、
  验收结果），并做脱敏——不得发送 App Secret、访问令牌、用户隐私或未公开凭据。
- **效力定位**：Jev 输出只是**独立第三方参考**，不是决定。Leader 保留最终决定权，
  并必须在决策记录（合并提交信息或 `docs/decisions.md`）写明：问题、Jev 的
  答案/概率/confidence、最终决定及理由。Jev 不能覆盖人类指令、全局安全规则或
  确定性门禁。
- **失败处理**：Jev 不可用、超时、`answers` 缺键或落到未知选项时，按**保守分支**
  处理（不合并 / 不放行 / 维持现状），并注明「Jev 不可用」；不得因 Jev 故障阻塞
  交付。目标不明确时向 Owner 提问；其余按 Leader 的决策权处理。
- **成本与幂等**：按输入 token 计费；同一决策不要反复调用，结果按输入摘要缓存。
  不打印 key，key 的解析顺序遵循 `jev-decision` 技能文档。
- **替换**：若项目改用其他决策模型，在参数表 `<DECISION_MODEL>` 写明，并保持
  上述效力、失败与记录规则。

参数表留空 `<DECISION_MODEL>` 时，本节不生效。

> **本项目注**：`<DECISION_MODEL>` 已留空 —— 本节不生效。本项目按要求「确定性门禁 + 卡面证据」判定，
> 不用模型投票替代命令退出码。

## 7. 跨团队协同：以 Issue 为准

- **适用场景**：涉及其他团队、仓库、外部贡献者，或需跨团队排期与验收的工作。
- **唯一事实源**：每个跨团队请求在 `<TRACKER>` 开 issue，至少写清：背景、
  期望结果、验收标准、负责人/团队、截止时间、相关链接（分支、commit、PR/MR）。
- **流程**：
  1. Leader 创建或认领 issue，打标签与优先级，指定 owner。
  2. 成员在独立 worktree 开发，提交信息引用 issue（`Refs #123`，完成 `Closes #123`），
     PR/MR 描述链接 issue 并写验收证据。
  3. 方案/接口变更、延期与风险在 issue 留痕；涉及目标层面的变更由 Leader 在 issue 中
     @ Owner 确认；过程决策 Leader 直接定并留痕。
  4. 验收标准满足、issue 关闭后，相关分支才合并到 `<MAIN_BRANCH>`。
- **工具**：优先使用对应 CLI（`gh` / `glab` 等），避免手工抄状态。
- **安全**：issue/PR 中不得粘贴密钥、令牌、用户隐私；日志与配置必须脱敏。

若暂时没有 tracker：把跨团队事项记录在仓库内的 issue 草稿目录（如
`docs/issues/`），并在 tracker 就绪后迁移为正式 issue，不得省略。

> **本项目注**：tracker 就是 `walgit` 看板 —— 一条卡片（thread）= 一个 issue，状态流转靠**签名条目**
> 而不是编辑看板文件；GitHub `origin` 只是镜像，不承接协作。完整流程（认领、patch、review、
> merge_result、closed）见 `docs/AGENTS.project.md` §1–7 与「远端与看板」节。

## 8. 软件工程纪律

- **小步提交**：一个提交只做一件事；提交信息用
  `feat|fix|test|docs|refactor|chore|merge:` 前缀，说明「为什么」而不只是「改了什么」；
  每个提交都必须可构建。
- **主分支保护**：代码变更一律走成员分支 + 独立 worktree；禁止直接向
  `<MAIN_BRANCH>` 提交代码；合并使用 `--no-ff` 并记录验收结果与合并点；
  合并后删除已合并分支。
- **质量门禁**：合并前必须通过 §3 的命令；新增行为必须有测试，修复缺陷必须有
  回归测试；禁止通过删除/跳过测试让流水线变绿。
- **退出码是唯一权威**：门禁命令不要接管道（`cmd | tail` 会返回 `tail` 的状态），
  脚本里先捕获 `$?` 再决定分支；构建工具可能对**成功**的运行也打印
  "failed command" 之类的字样，判断失败要看是否另有 `error:`/失败计数。
- **评审**：由作者之外的人评审；P0/P1 findings 必须修复或有明确处置结论；
  评审意见具体到 `file:line` 与失败场景；评审记录随代码入库。
- **缺陷处理**：先复现 → 写失败测试 → 修复 → 全量验证 → 记录根因；
  不得用重试、忽略或放宽断言掩盖可复现缺陷。
- **契约与兼容**：对外协议、公开 API、配置项、工具 schema 属于契约；变更必须
  同步文档、示例与迁移说明；破坏性变更须由 Owner 在目标里预先授权，未授权先确认。
- **可观测性**：日志与错误信息不得静默吞掉；资源上限、丢弃与截断必须记录；
  标准输出保持干净（按项目约定）。
- **依赖与复用**：**不重复造轮子**——优先复用社区认可、活跃维护的成熟依赖。
  选型判据：持续提交与发布、被广泛使用、issue/PR 响应及时、许可证清晰、跟踪本
  项目所用的语言/运行时版本。**自研只允许**在「无合格依赖」或「依赖带来不可接受
  的供应链/许可/体积风险」时进行，且必须在设计记录里写明理由与移除条件。新增依赖
  必须固定版本（tag 或 commit），评估供应链风险与维护状态，并同步更新文档与
  CHANGELOG。
- **安全**：仓库中不得出现密钥、令牌或个人信息；外部输入（webhook、消息、路径、
  URL、HTTP 请求）必须校验。
- **发布**：版本遵循 SemVer，维护 CHANGELOG；发布产物来自干净提交；
  发布前跑完整验收。
- **技术债**：临时方案必须关联 issue 并写明移除条件；不允许无记录的「临时」代码
  长期存在。

## 9. 派工模板（Leader 复制使用）

```markdown
你是本项目团队成员。仓库：<REPO_ROOT>（Rust workspace 在 mir2-rs/，oracle 为 C# 参照实现）。
工作区/分支（已建好）：<WORKTREE_ROOT>/<role>，分支 card/<短名>。
只在该 worktree 内改动；不要 push、不要合并、不要派生下级代理。

## 任务
<背景、目标、边界、明确不做的事；关联的 walgit 卡号>

## 约束
- 优先复用成熟依赖；新增依赖必须固定版本、说明理由并评估供应链风险
- 日志写 stderr，stdout 保持协议/输出约定
- 不改其他成员的文件；任务边界不重叠；先核写集（改动文件清单）再动手
- 行为等价优先于修 bug；偏离 oracle 必须走 T-*/D-*/B-* 白名单登记

## 验收
cd mir2-rs && cargo fmt --all -- --check
cd mir2-rs && cargo clippy --workspace --all-targets -- -D warnings
cd mir2-rs && cargo test --workspace
cd mir2-rs && cargo run -p msgcodegen -- --check
全绿；新增行为有测试。R 级片另需真机对拍证据。

## 交付
小步提交，每个提交可构建；不合并；交付到 needs-review 为止（不自行 approve、不自行 merge）。
报告：分支、commit SHA、改动摘要、红→绿证据、命令与结果、剩余风险、残留物清单。
```

## 10. Definition of Done

一个分支可以合并，当且仅当：

- [ ] 目标行为已实现，边界外未扩散
- [ ] §3 全部命令通过（含集成测试，或按 R/L 分级给出免跑理由）
- [ ] 新增/修改行为有测试，缺陷有回归测试
- [ ] 文档、示例、CHANGELOG 同步（如涉及对外契约）
- [ ] 无硬编码密钥、无调试输出污染标准通道
- [ ] 审查完成，P0/P1 已处置
- [ ] 提交信息清晰、可构建、范围单一

> 本项目的收口还有额外五项（worktree / 分支 / 看板卡片收口 / 进程 / 临时物），
> 见 `docs/AGENTS.project.md`「每批收尾清单（Cleanup DoD）」。两者同时满足才算完。
