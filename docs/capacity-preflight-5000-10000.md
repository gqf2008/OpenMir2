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

## 5. 5000 档实测发现（2026-11-17，归档 `evidence-w35-20261117-2048`）

### 5.1 入口瓶颈在 game-svr 的单一事件队列，不在网关/DB

- 四层对账：gate `accepted=5000`（`send_raw_fail=0`，`gm_data_forwarded=1,184,298`）→ game-svr
  `会话验证通过=5000`（事件已入队）→ `sim: 认证通过=3403`（出队处理）→ 客户端 `in_world=1948`。
- 按 Account 配对「入队 ↔ 出队」：3403 条已处理，**1597 条在窗口结束时仍在队列里**（验证时刻全在 ramp 尾段）；
  延迟 **p50 56.9s / p90 223.5s / p99 287.8s / max 296.0s**（窗口仅 360s）。
- 到达 `frames_in 1,194,705 / 360s ≈ 3,319 帧/s` vs 排水 `drain_events_drained 124,211 ≈ 345 事件/s`（10.4%），
  队列净增 ≈ 2,970/s，窗口末积压 ≈ **1.07M 事件**。
- `drain_budget_hit=1671 / 3150 轮`（53%）：预算不是根因，是「为保 tick 而截断」的症状；
  即使把墙钟阀开到整轮 200ms（约 4–5 倍），吞吐 ~1.4k/s 仍低于 3.3k/s 到达 ⇒ **调预算不能闭合差距，只会把 tick 饿死**。
- 另 1453 条「enter 了但客户端没进世界」是另一段损失：logon 被处理了但客户端在等待期（>60s）超时；
  其尾巴表现为 game-gate `downlink_unknown=62,362` 与 game-svr「下发 SM_* 帧时槽不存在」数万条。

### 5.2 内存风险项：`ev_rx` 是无界通道且没有长度读数

- `sim_loop` 的事件通道是 `std::sync::mpsc::channel`（无界），**没有积压长度读数**，本轮的积压只能靠
  「到达 − 排水」离线推算。
- 粗估每一项 `GateEvent::Command` ≈ 150–250 B（枚举内联 + `chr_name`/`tail` 堆分配）；本轮 1.07M 事件 ≈
  **150–250 MB**。线性外推到 10000 档（帧率约 2–2.5×）：同样 360s 窗口积压可达 **2.5–3M 事件 ≈ 0.4–0.8 GB**，
  且无上限增长 ⇒ **OOM 风险**。
- 处置方向（未改代码）：给通道加积压读数（生产侧计数）、设上限与丢弃策略（落 `logon_disconnect_drop` 一类的记账），
  或从根本上提高消费吞吐/拆分世界。

### 5.3 game-gate `SessionId` 撞号实测

- 本轮日志：`会话 SessionId 撞号：candidate=…` **1271 次**。5000 会话占 4 位空间（10,000）的 ≈47%，
  生日碰撞预期 ≈ n²/2m ≈ 1100，与实测同量级；撞号被 `claim_sid` 重定位（D-18），功能无碍。
- 但 **10000 会话时空间打满**（n=m）⇒ 撞号只能回落 oracle 覆盖语义（串投）。这是「10000 档零余量」的实测证据。

### 5.4 待验证

- botfleet 侧 play 相超时阈值（若 <60s 直接解释 1453 条客户端侧损失）；
- 每 bot 命令间隔（决定 `frames_in/s` 的分子，用于实验 A/C 的设计）。

## 6. 实验设计（数据量 vs 并发）

| 实验 | 固定 | 变化 | 判据 |
| --- | --- | --- | --- |
| C（先跑） | 5000 bot 登录后不发游戏命令 | — | 队列不增长、logon 数秒内全处理 ⇒ 命令洪流是瓶颈 |
| A | 同一份 DB 快照 | 2000 vs 5000 bot | 延迟随并发陡增、DB 延迟不变 ⇒ 并发/事件量主因 |
| B | 2000 bot | 空库 vs 1.5× 满库 | DBSrv 单请求延迟随数据量升、队列延迟不变 ⇒ 数据量主因 |

配套离线指标（无需改代码）：① DBSrv 每请求延迟（`load_request_seen`→`responded_ok` 时间戳配对）；
② 登录队列延迟（`会话验证通过` ↔ `sim: 认证通过` 按 Account 配对）。

## 5. 开窗前检查清单

- [ ] 5000 档：三网关均"不撞墙"，**无需任何改动**；
- [ ] 10000 档：`game-gate` 目标 ≤9000（或已完成 sid 空间扩展 + D-* 登记）；
- [ ] 10000 档：login-gate 每 tier 重启（或 D-* 偏差已登记）；
- [ ] bot 宿主临时端口范围已量、切换后旧 socket 会关闭；
- [ ] 归档里采集三网关 `*_ADMISSION` 行（`full` / `not_ready` / `sessions_now` / `sessions_max`）与 `获取登陆服务失败` 计数；
- [ ] 判据口径照 runbook：报绿必须给"集合 + 总数"，窗口结论带 `login/no_resp/internal_errors/OOM` 四项。

> 关联：`docs/loadgate-criteria-runbook-20261115.md`（窗口流程与开窗冻结）、`docs/binary-provenance-stamp-20261115.md`（产物身份只认戳）。
