# 任务：C6 续 —— tier-500/1000 登录上限定位 + tier-1000 补跑（worker-5）

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\c6`（分支 `worktree-c6`）。卡：`openmir2-c6-e2e-loadgate`，身份 `openmir2-worker-5`。

## 已查清（别重复）
- 工具链 5 处缺陷已修（stack_e2e 管道挂死、档位 -File 逗号拼接、BotSrv 账号复跑幂等、RunAutoPlay 越界、开号幂等），tier-200 可复跑 GREEN。
- 登录上限根因排查：**已排除** LoginGate per-IP 上限（`GateShare.IsConnLimited` 计数循环被注释，不生效）、`MaxClientPacketCount`（无使用点）、网关发送队列上限（`CreateUnbounded`）。证据指向**服务端**：tier-1000 假人 `开始登陆` 1000 次但仅 386 次 `帐号登录成功`（conn_refused=1/conn_lost=0）。

## 本轮目标
1. 定位那 614 个登录请求为什么没拿到 `SM_PASSWD_OK`：在 **BotSrv（测试工具，可改，登记 T-*）** 加「登录响应」观测（收到 SM_PASSWD_OK/FAIL 都计数并打日志），最小复现（如 20/100/500 档）判定是「假人没发对包」还是「服务端没回/回错」。
2. 若是假人侧共享态（`MShare.*`/全局标志）并发写坏包 ⇒ 修 BotSrv 使其按会话隔离（登记 T-*）。
3. 补跑 tier-1000（用 `tools/e2e/stack_e2e.ps1 -ProvisionAccounts -LoadTiers 200,500,1000 -SkipFlow -StopStackAtEnd`），产物入库。
4. 若属 oracle 真实吞吐上限（非假人 bug）⇒ 如实登记 + 给 M5 阈值建议，别硬改 oracle。

## 硬门禁
`cargo fmt/clippy/test --workspace --offline`；BotSrv 改动需 T-* 登记；tier-200 仍须 GREEN。
## 交付
patch（base master，head worktree-c6）+ `status: needs-review`。**不自行 merge / 不签 review。** 收工停栈。
