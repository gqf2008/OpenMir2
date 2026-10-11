# 任务简报：A2/M1 卡第二批（边界服务）+ M3 首验 — 交给 worker-1

仓库：`E:\Users\gxh\Documents\GitHub\OpenMir2`
看板卡（唯一事实来源）：

- **「A2/M1：Rust 版边界服务（LoginGate/SelGate/LoginSrv/DBSrv）」**，线程 id `openmir2-a2-m1-services`，owner 身份 `openmir2-worker-1`，分支 `worktree-a2-m1-services`。
- 配套卡 **「M3-Rust 首验：Rust 顶住 7000/7100/7200 后跑门禁」**，线程 id `openmir2-m3-rust-firstrun`，看板状态「待认领」，owner 也挂在 worker-1 名下。

已交付并复核的部分：Rust `bins/login-gate` 顶住 7000 + 进程级中继自测 4 条全绿。
本轮候选范围（**以卡内条目与设计文档 §2 端口表为准**）：① 补 SelGate / LoginSrv / DBSrv 的 Rust 版；② 真客户端经 Rust LoginGate 登录成功；③ 与 C# 侧服务的混跑链；④ M1 顶住端口后跑 `-Side rust` 首验并把证据落条目。

## 0. 身份纪律（不可协商）

- 你是 `openmir2-worker-1`，**一个 agent 一个 principal**：只用一个身份签发条目，绝不借用 `openmir2-coordinator`（已退役）、`openmir2-reviewer-*`、或别的 worker 身份。
- 协调者是 `/root` 控制台（principal `openmir2-orchestrator`）；独立审查由 `openmir2-reviewer-1`（pi 控制台）做。
- 交付只到 `needs-review`：**不要自己 merge**，不要签 review。

## 1. 开工前的固定动作

1. 读 `E:\Users\gxh\Documents\GitHub\OpenMir2\AGENTS.md`，以及 `docs/Rust重写设计文档.md` 的 §14.5、§2（oracle 与端口表）、§6（里程碑验收）、§6.0（证据四件套）。
2. `git -C E:\Users\gxh\Documents\GitHub\OpenMir2 fetch walgit '+refs/collab/*:refs/collab/*'`，读 `collab board` 与上面两张卡的 `collab thread`。
3. 建独立 worktree：`git -C <repo> worktree add .claude\worktrees\a2m1 worktree-a2-m1-services`
4. 认领：在该 worktree 里
   `walgit --config $env:USERPROFILE\.walgit\walgit.toml collab join --repo <worktree> --principal openmir2-worker-1 --key $env:USERPROFILE\.walgit\keys\openmir2-worker-1.ed25519 --push walgit`
   先发 `status: in-progress` + `comment` 写清本轮范围，再动手。若已有别人更新的认领，停手回报。

## 2. 注意

- C6 线正在 `.claude\worktrees\c6` 里改 BotSrv，**不要碰**那个 worktree 与主检出。
- 真客户端是冻结版：`D:\MirClient-run\run-release.cmd`，账号 `mir2test` / `mir2pass`；不改客户端源码。

## 3. 硬门禁（自己跑，贴原始输出；RED 不许交）

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --offline -- -D warnings`
- `cargo test --workspace --offline`
- 卡内写的端口/登录验收命令（含 `-Side rust` 首验）

## 4. 交付

patch 挂卡（带 `base`/`head`/`branch`/`files`/`why`）+ `status: needs-review`，parent 用上一条 status 的 oid；证据四件套写进 body。

## 5. 红线

不得改动 `src/**`（C# oracle）行为；不得动客户端 `MirClient` 仓库。

## 6. 完成后回报（给 /root 控制台）

新增/改了哪些文件、门禁与端口验收的原始结果、卡片条目 oid、卡在哪。
