# 任务简报：B2/M4 卡（Rust 侧翻转 + 派发目标实现）— 交给 worker-3

仓库：`E:\Users\gxh\Documents\GitHub\OpenMir2`
看板卡（唯一事实来源）：**「B2/M4 续：93 个派发目标语义实现 + 未实现清单归零」**，线程 id `openmir2-b2-script-dispatch`，owner 身份 `openmir2-worker-3`，分支 `worktree-b-script-engine`（origin/walgit 上已存在）。

## 0. 身份纪律（不可协商）

- 你是 `openmir2-worker-3`，**一个 agent 一个 principal**：只用这一个身份签发条目，绝不借用 `openmir2-coordinator`（已退役）、`openmir2-reviewer-*`、或别的 worker 身份。
- 计时器已拆除，协调者（`/root` 控制台，principal `openmir2-orchestrator`）通过看板与文件与你协作；审查由 `openmir2-reviewer-1`（pi 控制台）独立做。
- 交付只到 `needs-review`：**不要自己 merge**，不要签 review。

## 1. 开工前的固定动作

1. 读 `E:\Users\gxh\Documents\GitHub\OpenMir2\AGENTS.md`，以及 `docs/Rust重写设计文档.md` 的 §14.5（协作规范：身份/worktree/patch/审查换人）、§6（里程碑验收判据）、§6.0（证据四件套）。
2. 同步并读卡：`git -C E:\Users\gxh\Documents\GitHub\OpenMir2 fetch walgit '+refs/collab/*:refs/collab/*'`，然后
   `walgit --config $env:USERPROFILE\.walgit\walgit.toml collab board --repo E:\Users\gxh\Documents\GitHub\OpenMir2` 和
   `walgit ... collab thread --repo <repo> openmir2-b2-script-dispatch`。**卡里的范围与判据优先于本简报的转述。**
3. 建独立 worktree（不要在主检出上改）：
   `git -C <repo> worktree add .claude\worktrees\b2 worktree-b-script-engine`
4. 认领：在该 worktree 里
   `walgit --config $env:USERPROFILE\.walgit\walgit.toml collab join --repo <worktree> --principal openmir2-worker-3 --key $env:USERPROFILE\.walgit\keys\openmir2-worker-3.ed25519 --push walgit`
   然后先发一条 `status: in-progress` + `comment`（写清本轮范围），再动手。
   若卡上已有别人更新的认领/条目（别人正在做），**停手并回报**，不要抢。

## 2. 本轮范围（以卡内最新条目为准）

协调者最后一条指示：先做 Rust 同批翻转（`parser.rs` 的 `code-1` → `code`；whitelist B-8 改为两侧同步翻转），重跑 flow-diff 期望 **3 一致 / 0 不一致**；然后继续补 handler，直到未实现清单归零。

## 3. 硬门禁（自己跑，贴原始输出；RED 不许交）

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --offline -- -D warnings`
- `cargo test --workspace --offline`
- 卡里的 flow-diff 回归命令（阳性对照也要跑）

## 4. 交付

- 把 patch 挂到卡上（`--kind patch`，带 `base`/`head`/`branch`/`files`/`why`）+ `status: needs-review`，parent 用上一条 status 的 oid。
- 证据四件套写进条目 body。

## 5. 红线

不得改动 `src/**`（C# oracle）行为；不得动客户端 `MirClient`；不得把并行引进 `crates/world`。范围外的东西写成 comment 交回协调者。

## 6. 完成后回报（给 /root 控制台）

改了哪些文件、四条门禁的实际结果、卡片条目 oid、卡在哪、下一步建议。
