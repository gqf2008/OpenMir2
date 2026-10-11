# 任务：M3 夹具确定性补强 —— BotSrv 建角参数可配（worker-4）

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m3`（分支 `worktree-m3-firstrun`）。
卡：`openmir2-m3-botsrv-fixed-newchr`（唯一事实来源）。身份：`openmir2-worker-4`（worktree 已 join）。

## 目标（照卡内 machine_checkable_acceptance 做，不自行改判据）
把 M3 跨侧对拍里「夹具自身随机」消掉：`src/BotSrv/Scenes/Scene/SelectChrScene.cs:154-176` 的 **Hair/Job/Sex 随机、RandomNumber 无种子**改为**可配置固定值**，使 C# 侧与 Rust 侧跑同一输入序列：
1. 加配置项（`[Switch] FixedNewChr` / `[Integer] NewChrJob,Hair,Sex` 或等价物）能固定建角参数；**未配置时行为与现在一致**（默认随机，行为等价优先）。
2. 同一命令连跑两次：`mir2_db.characters` 的 Job/Hair/Sex 两次相同（同序列成立）。
3. 固定 Job 场景下跨侧 diff（scoped_diff 规范化后）=0，且该场景 Job 列**不**置占位。
4. 改动登记 `T-*`（BotSrv 是测试工具；`src/**` 只动 BotSrv，不改 oracle 行为）。
5. 硬门禁：`cargo fmt/clippy/test --workspace --offline` + msgcodegen 全绿；若动 C# 侧，另出 C# 构建证据。

## 先做
`git fetch walgit '+refs/heads/*:refs/remotes/walgit/*'` 并 `git merge --ff-only master` 拿最新（A2/M1、B2/M4 已合入）；读卡 + 读 `mir2-rs/tests/parity/m3/`（run_pair/scoped_diff）与 M3 首验报告 §2.3/§2.4。

## 交付
patch（base refs/heads/master，head refs/heads/worktree-m3-firstrun）+ `status: needs-review`。**不自行 merge / 不签 review。** 收工停栈、清临时物。

## 红线
不得改 `src/**` 里 BotSrv 之外的行为；不得动客户端；不得把并行引进 `crates/world`。
