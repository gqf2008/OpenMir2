# 任务：M3-Rust 首验（worker-4）

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m3`（分支 `worktree-m3-firstrun`）。卡：`openmir2-m3-rust-firstrun`，身份 `openmir2-worker-4`（worktree 已 join）。

## 前置（先判，再动）
本卡真实前置=**M2 的 Rust RunGate/GameGate(7200) 合入 master**（不是 A2/M1）。开工先 `git -C <repo> fetch walgit +refs/heads/*:refs/remotes/walgit/*` 并 `git merge --ff-only master`（或 rebase），确认 `mir2-rs/bins/game-gate`（或等价 GameGate 7200 实现）已在 master；若尚未合入，先做**可先做的部分**（读卡、读 `mir2-rs/tests/parity/m3/README`、把 `-Side csharp` 基线复跑一遍、准备好 runner），并每 ~10 分钟重查一次 master。

## 目标
Rust 顶住 7000/7100/7200 后跑 **M3 跨侧门禁**：`mir2-rs/tests/parity/m3/run_pair.ps1 -Side rust`，与 `-Side csharp` 对拍，作用域内**字段级 diff=0**；把证据四件套（命令+输出+路径+hash）落卡。

## 硬门禁
`cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets --offline -- -D warnings` / `cargo test --workspace --offline`。
## 交付
patch（base refs/heads/master，head refs/heads/worktree-m3-firstrun）+ `status: needs-review`；**不自行 merge / 不签 review**。
## 红线
不得改 `src/**`（oracle）；不得动客户端；不得把并行引进 `crates/world`。收工停栈。

## 现状（2026-10-11 13:40，owner 侧更新）

- **前置①已解除**：master 已推到 **`50b8d774`**（含 m2-svr 已批准切片 + gamegate）——`mir2-rs/bins/game-gate/src/main.rs` 与 `mir2-rs/tests/parity/m3/run_m3_gate.ps1` 都在 master 上。**不要再等/轮询**，直接开工。
- **端口窗口**：5000/7000/7100/7200 此刻**空着**（owner 实测 listening=0）。你先跑先占；C6 车道的 brief 要求它"先查端口再跑"，所以不会跟你抢。
- 先把 master 合进本分支（`git merge master`），再跑 `-Side csharp` 基线与 `-Side rust` 首验；diff 不为 0 就按卡内判据如实出结论（**不许改 oracle 去凑**）。
- 收工**停栈**（`-StopStackAtEnd` 或 `E:\MirServer\start-all.ps1 -Stop`），别留端口占用与残留进程。
