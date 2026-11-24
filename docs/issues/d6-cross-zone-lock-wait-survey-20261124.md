# D6 勘察：跨区加锁的「阻塞等待」风险（只读）

> **状态**：只读勘察（2026-11-24，`card/d6-lock-wait`，base master `9cd484c04`）。**不改产品码、不跑构建、无运行时实验**；结论全部来自读码与 `grep`，已标注不确定处。
> **对象**：`MIR2_ZONE_SLICING=1`（N 区切片）下的跨区锁路径。N=1 不进入本报告任何点。
> **符号锚**：`lib.rs` = `mir2-rs/bins/game-svr/src/lib.rs`（高 churn 文件按仓规只写符号，不写行号）。

## 0. 一句话结论

**开 `MIR2_ZONE_SLICING` 之前，必须先把两类「看门狗豁免窗口」的跨区锁改成有界等待：①关服存档收集（`disarm` 之后）、②启动期静态装载（`arm` 之前）；其余跨区锁（登录落区、A6 控制排水、频道排水、区存档排水、双向会话迁移）目前只靠「目标区卡死 ⇒ 看门狗 5s 整机 abort」兜底——不是无限挂，但也**没有超时、没有计数、没有降级**，应当统一改成有界获取 + `zone_lock_wait_timeout` 计数 + 按既有计数归类（fail-closed）。**

## 1. 锁与线程模型（读码事实）

- 状态容器：`ZoneHost.zone_states: Vec<Option<Arc<std::sync::Mutex<ZoneState>>>>`；**下标 0 恒 `None`**（区 0 的 `HostLoop` 是 `sim_loop` 局部变量，不在表里）。见 `lib.rs` `ZoneHost`。
- 线程：区 0 = `sim_loop` → `HostLoop::tick`；区 k（k≥1）= `spawn_minimal_zone` 线程循环（`lib.rs` `spawn_minimal_zone`）。
- **全部跨区获取都是阻塞 `Mutex::lock()`**：
  - `grep -rn "try_lock" mir2-rs/bins/game-svr/src/` ⇒ **0 命中**（生产与测试都没有）；
  - 无超时、无 timed lock、无「有界重试」；
  - 毒化一律**回收**：区锁站点全部是 `lock().unwrap_or_else(std::sync::PoisonError::into_inner)`（`lib.rs` 内 `PoisonError::into_inner` 共 33 处，含非区锁）⇒ 某区线程 panic 后，其它线程**照常拿到锁继续干**（fail-open），不是失败退出。
- **持锁期间不等屏障**：屏障等待（`BarrierOps::await_neighbors` / `after_phase2`）一律在**锁外**；区 0 `HostLoop::tick` 与区 k `spawn_minimal_zone` 的注释与代码都如此（`lib.rs` `tick`、`tick_phase_b`、`spawn_minimal_zone`）。
- **无嵌套双区锁**（当前）：`deliver_session_migrations` 一次只锁目标区；`drain_guild_say` 按区号升序**逐个**锁、每次只持一把；关服/A6/存档都是「锁一个区 → 释放 → 下一个」。
- **锁序（邮箱 vs 区锁）**：区 k 路径是「持本区锁 → 入控制/频道邮箱锁」（`a6_enqueue_control` / `channel_enqueue` 在 `tick_phase_a` 锁内调用）；区 0 drain 是「先取空邮箱（锁内 drain）→ **释放邮箱锁** → 再取区锁」（`a6_take_control_requests` / `channel_take_requests`）。两侧**没有同时持两把**，当前不构成环；但将来若有人把「取邮箱」挪进区锁内，就会与区 k 的方向形成反转。
- **看门狗**（`mir2-rs/bins/game-svr/src/tick_watchdog.rs` `ZoneWatch`）：
  - 心跳按区记录（**含区 0**：`HostLoop` 在 `tick_phase_b` 的 inline `after_phase2` 里写心跳）；
  - **`arm()` 在区 0 第一轮 barrier2 之后**（`HostLoop::tick_phase_b` 的 `self.ticks == 0` 分支）；**装载期不跑**；
  - 阈值缺省 5000ms（`MIR2_ZONE_TICK_TIMEOUT_MS` 可改，0 拒绝）；越阈值 ⇒ `std::process::abort()` **整机**；
  - **关服时 `disarm()`**（`sim_loop` 关服分支先 `host.watch.disarm()` 再收存档）——存档期没有 abort 源。
  - ⇒ 含义：**arm 之后**，任何区（含区 0）停跳 >5s ⇒ 整机 abort；一个「卡死并持锁」的区会把等待方一起拖到 abort，而不是永久挂。**arm 之前 / disarm 之后**没有这层兜底。

## 2. 逐点表（跨区/跨线程锁获取点）

| # | 获取点（符号锚） | 调用线程 | 目标 | 持锁期间做什么 | 有界？ | 检查 `alive`？ |
|---|---|---|---|---|---|---|
| L1 | `lib.rs` `deliver_session_migrations`（两处获取：attach；router 失败回滚 take） | 区 0（`HostLoop::tick`）/ 区 k（`spawn_minimal_zone`） | 目标区 k | `attach_session` / `take_session_for_migration`；`router.assign` 在锁外 | **否**（阻塞） | **是**（锁内先读 `alive`） |
| L2 | `lib.rs` `tick_phase_a` 的 `PhaseSet::SHUTDOWN` 分支（`collect_shutdown_zone_saves` 循环） | 区 0 | 每个区 k（顺序） | **持锁内编码所有会话存档体**（`collect_shutdown_zone_saves`），再统一走有界 `shutdown_save_requests`（DB 等待在锁外） | **否** | 否 |
| L3 | `lib.rs` `tick_phase_a` 登录落区（`finish_login_on_zone_state`） | 区 0 | 目标区 k | 建会话/实体/展示表（内存）；`engine.in_save_rcd_list` 是内存查询；发帧走 mpsc | **否** | **是**（`st.alive` 检查后才进入） |
| L4 | `lib.rs` `tick_phase_a` 的 A6 drain（`a6_take_control_requests` 后逐条 `state_arc.lock()`） | 区 0 | 目标区 k | 执行 `GmSay` / `EatProcess` / `GuildProcess` / `SellOffProcess` / `ScriptProcess`；**guild 支在锁内做文件写**（`guild_cmd.rs` `save_guild`/`save_record`/`save_guild_list`）；发帧走 mpsc | **否** | 否 |
| L5 | `lib.rs` `drain_channel_says`（陈旧判定） | 区 0 | 说话者区 k | 只拷快照（会话/epoch/行会名） | **否** | 否 |
| L6 | `lib.rs` `drain_group_say` | 区 0 | 说话者区 k | 锁内算组队事件 + 收件人路由快照；发帧在锁外 | **否** | 否 |
| L7 | `lib.rs` `drain_guild_say` | 区 0 | 每个区 k（升序、逐个） | 锁内按名解析成员 + `BanGuildChat` 快照；发帧在锁外 | **否** | 否 |
| L8 | `lib.rs` `drain_zone_saves`（DB_POLL） | 区 0 | 每个区 k | 只 `mem::take` 两条 outbox 队列；DB 发送在锁外 | **否** | 否 |
| L9 | `lib.rs` `sim_loop` 启动期静态装载循环（`spawn_statics_for_zone`） | 区 0 | 每个区 k | 锁内做整区静态装载（MonGen/商人/NPC/守卫） | **否** | 否 |
| L10 | `lib.rs` `ZoneAliveOnDrop::drop` | 区 k 自身 | 自己 | 置 `alive=false` | 自锁（不跨区） | — |

**不计入本表的「看起来像跨区」的点**（读码确认不持区锁）：

- A36 每区仪表：`lib.rs` `zone_metrics_json` 只读进程级原子（`ZONEK_*_BY_ZONE` / `REGION_BARRIER_US_BY_ZONE`），**不锁任何区**；`note_region_barrier_us_for_zone` 也是原子写；
- 跨区收件人帧 / handoff 邮箱：`cross_zone_frames.rs` / `handoff.rs` 用**自己的邮箱锁**（同样是 `lock().unwrap_or_else(PoisonError::into_inner)`），投递是 push 队列、由目标区自己排水（owner-thread 模式），不锁 `ZoneState`；
- A6/频道**邮箱**锁：`a6_take_control_requests` / `channel_take_requests` / `channel_enqueue` / `a6_enqueue_control` 只锁 `Mutex<VecDeque<…>>`，持锁区间是 O(队列长度) 的 drain/push，无 I/O。

## 3. 风险分级

### P0 —— 真「无限阻塞」（看门狗在窗口外）

- **L2 关服存档收集**：`sim_loop` 关服分支**先 `disarm()`**，随后逐区 `lock()` + 编码。任一区线程在该区锁内停住（死循环/卡在用户态/被调试器停住），区 0 永远拿不到锁 ⇒ **进程永远退不出去**（DB 侧的有界等待还没开始）。
- **L9 启动期静态装载**：`arm()` 在第一轮 barrier2 之后，**装载期没有看门狗**。区 k 线程若在第一把锁内停住，区 0 的静态装载循环永久阻塞（表现为「启动卡住」，没有 abort、没有超时）。

> 这两处的共同点：**恰好落在 `arm`/`disarm` 豁免窗口内**；不是概率问题，是窗口存在。

### P1 —— 有界只靠「看门狗整机 abort」

L1（两个线程）、L3、L4、L5、L6、L7、L8：目标区卡死并持锁时，调用方阻塞，目标区心跳停 ⇒ 看门狗在阈值（缺省 5s）后 **abort 整机**。所以不是永久挂，但：

- **没有超时/计数/降级**：abort 是唯一出口；`MIR2_ZONE_TICK_TIMEOUT_MS` 调大多少，「看起来像慢」的窗口就有多长；
- 区 0 被卡住时，**所有区**的登录/入站/A6/频道都停（区 0 是单线程）；
- 区 k 被卡住时，该区自己停跳，barrier2 让其它区等，最终同样 abort；
- **A36 的 `barrier_us` 看不到这类等待**：它度量屏障等待，而 mutex 等待发生在段内/段间、无 profiler 槽 ⇒ 只能看到「整体变慢」，不能归因。

### P2 —— 不阻塞但有语义风险（登记，不在本卡修）

- **毒化回收 = fail-open**：任一区 panic 后，L2/L4/L5/L6/L7/L8/L9 不检查 `alive`，会继续在**已死/可能不一致**的状态上编码、执行命令、拷快照。只有 L1/L3 用 `alive` 拒绝。
- **L4 在区锁内做文件 I/O**（guild 存档）：磁盘慢 ⇒ 区 k 的 tick 起点被拉长（是「慢」，不是挂）；同时把「进程级文件」写进「区锁」临界区，后续若改成有界锁要处理「超时后写到一半」的语义。
- 邮箱锁与区锁的**方向**（区 k：区锁→邮箱锁；区 0：邮箱锁→释放→区锁）目前无环，但属于「靠调用顺序维持」的不变量，应写进注释/牙。

## 4. 修复候选（只设计，不实施）

### (a) 有界获取 + 超时计数 + fail-closed（推荐作为统一底座）

- 新增小工具：`try_lock` 轮询 + 退避（`yield_now` / 1ms sleep，总 deadline 可配，例如 2–5ms）替代 `lock()`；超时返回 `None`。
- 新增计数 `zone_lock_wait_timeout`（按区下标/获取点维度），并复用既有降级计数分类：
  - L3 登录：走既有 `reject_login_landing`（目标区不可用类），计数 `zone_target_unavailable`；
  - L1 迁移：走既有回滚路径（`restore` + `zone_handoff_session_failed`）；
  - L4 A6：丢弃该请求（**不计** `zone_cmd_unsupported`——归属不是 Unsupported），WRN 限速；
  - L5/L6/L7 频道：丢弃 + `zonek_channel_rejected`；
  - L8 存档：本 tick 跳过该区，下一个 tick 重试（outbox 不丢）；
  - L2 关服：跳过该区 + 计数 + WRN（**总比永远退不出去好**）；
  - L9 启动：超时 ⇒ `abort`（启动期 fail-fast，不带着半装区上线）。
- 代价：新增等待语义与 6 处分支；需要 2 颗牙（超时路径计数 + 正常路径零行为变化），用「测试线程故意持锁」构造。
- 不引入新依赖（`std` 足够；`parking_lot::Mutex::try_lock_for` 可选，但会新增依赖）。

### (b) owner-thread 化（谁的数据谁锁；长期正确解）

- **L8 存档 outbox 低成本先做**：把 `ZoneState.host.zone_save_outbox` / `save_outbox` 改成**区锁之外**的每区 `Arc<Mutex<VecDeque<…>>>`（或通道）——区 k push、区 0 pop，**完全不锁 ZoneState**。
- **L5/L6/L7 频道**：说话者区在**入队时**就带齐「发送者快照 + 收件人路由」所需字段（行会名已带；`BanGuildChat`/成员所在区可在入队时由区 k 自己算——行会名册在 `GuildManager` 进程态，需另设计共享读法）。
- **L1 迁移 / L3 登录 / L4 A6**：改成「投递到目标区自己执行」的邮箱请求 + 应答，代价大（登录延迟、A6 stop-and-wait 语义、router 一致性窗口都会变），不建议作为开切片前置，先做 (a)。
- 代价：改动面大、牙影响面广；L8 可单独先做（小、可测）。

### (c) 不做

- 不给 `std::sync::Barrier`/`Mutex` 加超时封装成隐式全局行为（会掩盖调用点的语义选择）；
- 不把「看门狗 abort」当作有界等待——它是**整机失败**，不是降级。

## 5. 回答「开 `MIR2_ZONE_SLICING` 前，哪些阻塞点必须先改成有界等待？」

1. **必须先改（P0，看门狗窗口外）**：**L2 关服存档收集**、**L9 启动期静态装载**——不改就没有任何机制阻止永久挂。
2. **应同时改（P1，统一底座）**：L1/L3/L4/L5/L6/L7/L8 全部改成有界获取 + `zone_lock_wait_timeout` + 按路径 fail-closed；否则「目标区卡死」只能以整机 abort 收场，且 `barrier_us` 无法归因。
3. **可延后（P2）**：毒化回收后的 `alive` 一致性语义、L4 锁内文件 I/O 的拆分、邮箱↔区锁方向不变量入牙。

## 7. 观测契约（P0 两处实现后）

perf tick TSV 在 `zone_metrics` **之前**新增两列（A36 的末列契约不变；表头/值同改，
「表头列数 = 每行字段数」既有牙覆盖）：

| 列 | 语义 | 何时非 0 | N=1 |
|---|---|---|---|
| `zone_startup_lock_timeout` | 启动期静态装载「区状态锁有界获取超时」次数（本 tick 增量） | 超时即 fail-closed 退出（`exit(2)`），正常启动恒 0；计数器主要供测试/证据 | 恒 0 |
| `zone_shutdown_lock_timeout` | 关服存档收集「锁超时 ⇒ 跳过该区」次数（本 tick 增量） | 关服时若某区锁在 `SHUTDOWN_LOCK_WAIT` 内不可得 | 恒 0 |

常量（与 DB 等待分开）：`SHUTDOWN_LOCK_WAIT` = 2s/区（超时跳过该区 + WRN + 进程继续退出）；
`STARTUP_LOCK_WAIT` = 5s/区（超时 ERR + `exit(2)`，原因含区号）。
实现符号：`lib.rs` `try_lock_zone_state_until` / `collect_shutdown_zone_saves_bounded` /
`require_startup_zone_lock` / `startup_lock_failure_message` / `shutdown_lock_skip_warn`。
牙：`lib.rs` `d6_lock_wait_tests`（持锁线程 + `recv_timeout` 兜底；改回阻塞 `lock()` ⇒ 红）。

## 6. 本勘察的边界

- 无运行时实验、无变异；未测「锁等待」实测分布（需要注入持有者线程的测试或故障注入窗口）。
- 看门狗行为来自 `tick_watchdog.rs` 读码（arm 点、disarm 点、心跳含区 0、abort 整机）；未实测。
- 未覆盖 `MIR2_ZONE_SLICING` 之外的多世界/分片形态（本卡只问切片开关）。
- 行号一律不写（`lib.rs` 高 churn）；复现用符号 + `grep`（`try_lock` 0 命中、`PoisonError::into_inner` 33 处）。
