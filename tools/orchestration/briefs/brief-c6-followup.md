# 任务：C6 后续 —— 负载门禁跨轮幂等（worker-5）

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\c6`（分支 `worktree-c6`）。卡：`openmir2-c6-loadgate-idempotent`，身份 `openmir2-worker-5`。

## 目标
让 `tools/e2e/stack_e2e.ps1 -ProvisionAccounts -LoadTiers 200,500,1000 -SkipFlow -StopStackAtEnd` **同一命令重复跑得到同一结论**：
1. 开号/账号状态**跨轮幂等**（不再每轮 +N；auth/角色状态不因重跑而坏）；
2. 三档 verdict 两次一致；200 档保持 GREEN；
3. **定位并尽量修** tier-500/1000 的登录爬坡上限（已排除：LoginGate per-IP 上限失效、MaxClientPacketCount 无用点、网关队列无界；证据指向服务端 LoginGate→LoginSrv 单通道/或 BotSrv 并发写坏包——在 **BotSrv（测试工具，可改，登记 T-*）** 加登录响应观测判定）。
4. 若属 oracle 真实吞吐上限 ⇒ 如实登记 + 给 M5 阈值建议，别硬改 oracle。

## 硬门禁
`cargo fmt/clippy/test --workspace --offline`；BotSrv 改动 T-* 登记；tier-200 仍 GREEN。
## 交付
patch（base master，head worktree-c6）+ `status: needs-review`；**不 merge / 不签 review**。收工停栈。
