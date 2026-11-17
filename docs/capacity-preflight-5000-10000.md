# 容量前置检查清单（5000 / 10000 档开窗前，卡 `openmir2-capacity-preflight`）

- 定稿：2026-11-17（只读普查：`mir2-rs/bins/{login,sel,game}-gate` 源码 + 2000 档窗口实测日志）
- 用途：开 5000 / 10000 档窗口**之前**逐条过一遍；任一条判"会撞"就先处置，别用一轮窗口去发现墙。

## 1. 三网关上限（源码事实）

| 网关 | 上限与位置 | 是否强制 | 5000 档 | 10000 档 |
| --- | --- | --- | --- | --- |
| `sel-gate` | `MAX_SESSION = 2_000`（`sel-gate/src/slots.rs`，oracle `SelGate/Services/ClientThread.cs`）；**无配置键、无启动参数** | **否**。oracle 分配循环漏写回 `SessionArray[nIdx] = sessionInfo`（LoginGate 同段有写回）⇒ 槽表恒空、`Full` 分支不可达。本仓 1:1 照抄，运行时显式打印 `full_unreachable=true` | 不撞墙 | 不撞墙 |
| `sel-gate`（实际会话表） | 无界 `HashMap`（`sel-gate/src/lib.rs` 的 `Sessions`），断开/空闲超时摘除 | — | 内存线性增长 | 同左 |
| `game-gate` | `MAX_SESSION = 10_000` 槽 + **`SessionId` 4 位空间 `0..=9999` 且强制唯一**（`SESSION_ID_SPACE` + `claim_sid`） | 槽满 ⇒ 照 oracle 复用最后非空槽的句柄（串投）；sid 耗尽 ⇒ 回落 oracle 覆盖语义（串投） | 富余 2 倍 | **正好贴顶、零余量**；目标压 ≤9000 |
| `login-gate` | `MAX_SESSION = 10_000` 槽（`login-gate/src/slots.rs`，oracle `GateShare.MaxSession`），无配置键 | 满 ⇒ `Admission::Full`：打「获取登陆服务失败。」、**不关 socket、不建会话**、拒绝新登录 | 首轮无碍 | 贴顶；且见下条泄漏 |

**`MIR2_GAME_GATE_MAX_SESSION` 是测试注入**（改动会打「测试注入」WRN），**不能当生产配置**用；要 >10000 必须改代码（sid 在 `ServerDataMessage` 线上，属协议面，需 D-* 登记）。

## 2. `sid` 不是容量

- `login-gate` / `sel-gate` 的 `sid_max` = 进程内单调 `AtomicU32`，从 1 起、每 accept +1、**从不复用** ⇒ 它是**已接受连接计数**，与容量无关。
- `game-gate` 的 `GAME_GATE_ADMISSION` 行打印的是 `sid=<本次 accept 的 4 位会话号>`（hash 派生，逐条跳动），活会话看 `sessions_now` / `sessions_max`。
- 读日志时别把三行当同一字段：`accepted=N` 只说明"接了 N 条"，不说明"还能接多少"。

## 3. login-gate 槽泄漏（oracle 缺陷，本仓照抄）

- 回收只有两处：下游断开（`reset()` 全清）与空闲超时（`clear(index)` 清命中槽）。
- **客户端正常断开不清槽** ⇒ 每次正常断开泄漏一格；累计 `MAX_SESSION`(10000) 次正常断开后，该网关**拒绝一切新登录**，直到进程重启。
- 预警信号：`LOGIN_GATE_ADMISSION … full=` 从 0 变正、日志出现「获取登陆服务失败。」。
- **处置（10000 档前二选一）**：① **每个 tier 重启 login-gate**（零代码改动、行为等价；窗口脚本若用 `-RestartStackPerTier` 已天然覆盖）；② 登记 `D-*` 偏差「正常断开清槽」（改 oracle 行为，走白名单流程）。

## 4. 与网关无关、但可能先撞的两道墙

1. **bot 宿主的 TCP 临时端口**：Windows 默认动态端口 49152–65535（≈16,384）。若每个 bot 同时持有 login + game 两条连接（sel 为瞬时），10000 客户端 ≈ 20,000 个出向 socket ⇒ **先在客户端侧耗尽**。开窗前须确认：登录切换后 login 侧 socket 是否关闭；必要时扩大动态端口范围并复量。
2. **服务端容量**：2000 档实测 DBSrv 在最后一轮进世界后 0.65s 回完全部 2000 条（线性外推 10k ≈ 3.3s，可接受但须实测）；**真正的未知量是 10k 实体单世界 tick 耗时**——tick 饥饿修复后才测得到，属容量模型待测项。

## 5. 开窗前检查清单

- [ ] 5000 档：三网关均"不撞墙"，**无需任何改动**；
- [ ] 10000 档：`game-gate` 目标 ≤9000（或已完成 sid 空间扩展 + D-* 登记）；
- [ ] 10000 档：login-gate 每 tier 重启（或 D-* 偏差已登记）；
- [ ] bot 宿主临时端口范围已量、切换后旧 socket 会关闭；
- [ ] 归档里采集三网关 `*_ADMISSION` 行（`full` / `not_ready` / `sessions_now` / `sessions_max`）与 `获取登陆服务失败` 计数；
- [ ] 判据口径照 runbook：报绿必须给"集合 + 总数"，窗口结论带 `login/no_resp/internal_errors/OOM` 四项。

> 关联：`docs/loadgate-criteria-runbook-20261115.md`（窗口流程与开窗冻结）、`docs/binary-provenance-stamp-20261115.md`（产物身份只认戳）。
