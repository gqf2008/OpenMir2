# 任务：C6 —— 落实 owner 裁决（tier-1000 口径收口）+ 先把残余丢包的方向证出来

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\c6`（分支 `worktree-c6`）。身份 **openmir2-worker-5**。
卡：`openmir2-c6-loadgate-idempotent`（主）+ `openmir2-c6-e2e-loadgate`（原卡）。

## 0. 先读后写

1. `git fetch walgit '+refs/collab/*:refs/collab/*'`，读三样东西：
   - `openmir2-c6-loadgate-idempotent` 的 reviewer-3 `review`（oid `67af4cac`）：**request_changes**——C6-1（Important，tier-1000 两连跑
     verdict 不一致：A 全 GREEN / B tier-1000 RED 94.8%、爬坡未完成）+ C6-2（Minor，T-9 证据措辞）；它另外独立核对了
     T-9/T-10/stack_e2e/档位参数/开号幂等（D-1..D-5）**全部成立**，别重做。
   - 本卡 owner 裁决 comment `11dbde39`（按 required 的 (b) 收口 + 前置观测）。
   - 原卡 owner 裁决 comment `17aa4753`（原卡「三档全绿」口径同批重基准化）。
2. 你已经交付的东西（7c0a3226 / 81e387d8、c54cd30c、b3e8e241 + comment 552b8b36）**不要重复交付**；本轮是在其之上收口。

## 1. 第一步（**必须**先做）：把残余丢包的方向证出来

reviewer-3 已证：B 轮 tier-1000 里 1000 个假人**全部连上并发出登录包**（`login_sent=1000`/`conn_ok=1000`/`conn_err_*=0`），
只有 948 个收到登录应答（`ident_counts[529]=948`）。现在要区分是「服务端没回」还是「假人侧收包/解析丢了」：

- 允许改 **BotSrv**（测试工具；改动按 `T-*` 登记、单独提交）：把「发出登录包后未在窗口内收到应答」的连接按
  `conn / SocketId / 发出时刻 / 时间窗` 分组打出来（现有 T-8 观测可扩展），并加一条能自证的对照（例如对同一连接重发一次是否成功）。
- 结论二分：
  - **(i) 证据指向服务端（C# 栈）** ⇒ 登记为**上游上限**，**不改 oracle 行为**（红线）；不要去改 `src/**`。
    若确需 C# 侧**只读**计数才能定性，写成建议并交给 `openmir2-svc-1`（服务端维护线）+ 人签字，别自己动。
  - **(ii) 证据指向假人侧（BotSrv 收包/解析）** ⇒ 这是**我们自己的工具缺陷**：**修它**，别用调阈值盖过去。

## 2. 第二步：按裁决收口（做完第一步再改）

1. tier-1000 的登录判据改成**可复现的下界**：同命令 **3 连跑取最差 ≥ 阈值**（先按 **95%** 起步；阈值与理由、
   证据（reviewer-3 的 A/B 两轮 + 你自己的连跑）写进 `tools/botload/README.md` 与阈值注释）。
2. **牙齿必须留**：改完阈值后，必须有一条红检证明它仍能抓回归——例如把阈值临时改回 `login=100%` 跑同一档 ⇒ 该轮必须 **RED**，
   或把某一档判据做坏 ⇒ 必须 RED；把「改坏必红、还原必绿」的原始输出入库（§6.0）。
3. 「同一命令两连跑 verdict 一致」的表述改成「两连跑的**判定规则**一致（重跑不产生 GREEN/RED 翻转）」，并附 **3 连跑 verdict 表**。
4. **C6-2**：T-9 的 evidence 措辞收成「消除了假人侧共享判据导致的系统性丢弃（179→948~1000）；残余 0~5% 服务端侧丢包另计（见 C6-1）」，
   删掉任何暗示「三档已稳定 100%」的说法。
5. 原卡也照 `17aa4753` 更新对应口径（一并在原卡 comment 里回一句），让 reviewer-4 能按新口径闭环。

## 3. 实跑纪律（现在机器上还有 3 条 worker 车道在编译）

- 跑压测前先看：`Get-CimInstance Win32_Process -Filter "Name='node.exe'"`（有没有别的车道）与
  `E:\MirServer\port-status.ps1`（端口是否被占）。
- **尽量避免与其它车道的 cargo 编译重叠**；若实在要与编译并行跑，必须在证据里写明「本轮与 X 车道编译并行」。
- 压测完按 `-StopStackAtEnd` 停栈，别留进程。

## 4. 门禁与交付

门禁：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --offline -- -D warnings`、
`cargo test --workspace --offline`、`cargo run -p msgcodegen -- --check`（隔离 target）。
交付：`patch --base refs/heads/master --head refs/heads/worktree-c6` + `status: needs-review`（parent 用该线程最后一条 entry 的 oid）；
**不 merge、不签 review**。收工写 `E:\tmp\openmir2-watch\last-c6f.md`（含 3 连跑 verdict 表、红检原始输出、口径变更说明）。
