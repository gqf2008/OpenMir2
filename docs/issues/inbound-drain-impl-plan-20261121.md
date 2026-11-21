# 每区入站队列并行排水：四步可施工方案（2026-11-21，只读设计）

> 依据：`docs/issues/inbound-drain-parallel-plan-20261121.md`、`docs/map-zone-split-design-20261122.md`
> §0/§0.1/Q3（L2）。本文**不写产品代码、不跑构建**；所有位置写**文件+符号**（`bins/game-svr/**`
> 不写 `path:line`，行号会漂移）。基线代码：master `dbb3b9ad7` 的 follow-up 工作树。

## 1. 现状：`EventDrain` 的精确接点

**通道（`bins/game-svr/src/lib.rs`）**
- `run_inner` 创建两条**无界** `std::sync::mpsc`：`ev_tx/ev_rx`（命令/登记）与
  `ev_ctl_tx/ev_ctl_rx`（控制+进入）。**没有长度/字节读数**。
- 生产者是**每条网关线一个读循环**（`gate_conn` 及其 `dispatch`）：`GM_DATA` 解出
  `CommandMessage`+EDCode 尾后调 `emit_gate_event(ev_tx, ev_ctl_tx, GateEvent::Command{..})`；
  `GM_OPEN`→`Opened`、`GM_CLOSE`→`Closed`、认证通过→`LoadDbRequest`、失败→`Closed`、
  `Ignored`→`CertificationPending`；网关注册/注销走 `RegisterGate`/`UnregisterGate`；
  admin 走 `AdminGm`。全部经 `emit_gate_event`。
- `emit_gate_event` 按 `GateEvent::is_priority()` 选通道：控制（`Closed`/`RegisterGate`/
  `UnregisterGate`/`KickAccount`）+ 进入（`LoadDbRequest`）走优先通道，其余走普通通道。
- 消费者**唯一**：`sim_loop(ev_rx, ev_ctl_rx, …)`（sim 线程同时驱动 `World`）。

**每 tick 排水（`sim_loop`）**
- 跨 tick 周转槽：`event_carry` / `event_carry_ctl`（`Option<GateEvent>`，循环外持有）。
- 每轮：`let mut drain = EventDrain::begin(EventDrainBudget::PRODUCTION);`，然后
  `while let Some(ev) = drain.next(&ev_rx, &ev_ctl_rx, &mut event_carry, &mut event_carry_ctl)`。
- 预算常量：`EventDrainBudget::PRODUCTION = { max_events: 2000, max_wall: 50ms }`。
- `EventDrain::next` 的内部次序：优先结转 → 普通结转 → 优先通道（每轮 ≤`max_events/2`，反饥饿）
  → 普通通道；预算用尽时只 `try_recv` 一条判断「是否被截断」，该条进结转槽，
  `hit_budget = true`，并在 `sim_loop` 里触发 `telemetry::record_drain_budget_hit()`；
  每次真正取走一条记 `telemetry::record_drain_event_drained()`。
- 段计时：`sim_loop` 在排水前 `host_prof.begin()`，排水块结束 `host_prof.end("HostInbound", t)`。
- 事件体（登录/命令/关断）在**同一线程**直接读写 `World`、`online`、`sessions`、`ground_sessions`
  等；不存在「解析后只读」的分层。

## 2. 结论先行：**不能**先做「分片 N 队列 + 单消费者」

- 单消费者的吞吐上限 = 事件体处理速度；**事件体和 `World` 同线程**。把一条无界 mpsc 换成
  N 条按 `chr/socket` 分片的队列、仍由 `sim_loop` 一个线程按序合并，**处理吞吐不变**，
  只把「生产者侧 mpsc 争用」挪走——而当前的瓶颈是处理成本（w37 tick 201→513ms），不是入队争用。
- 分片还会**新引入**跨队列顺序问题：同一收件人的帧可能由不同队列的事件产生，单消费者按什么键
  合并？今天没有全局 `(tick, region, seq)` 键；`EventDrain` 的「优先通道/结转/反饥饿」语义
  也要在每条分片队列上复制，N 条队列的优先级交错如何保序没有免费答案。
- 门禁后果：`R=1` 必保 oracle 逐字节；一个「改了入站结构但吞吐不变」的中间态，只会增加
  「看起来并行、实际单线程」的假象与回归面。
- **结论**：真正的并行排水**必须等 `ea` 的 Step 2 多 World / 每区一个 owner**（每个 owner 线程
  独占一个 World，各自消费自己的区队列）。在那之前，可先做的只有 §3 的 **Step 0（测量骨架）**
  与 **Step 1（有界队列/计数，仍单消费者）**——它们不改吞吐结论，只为后续施工提供判据。

## 3. 四步方案

### Step 0：先量 M1/M7（零语义改动）

- 写集：`bins/game-svr/src/telemetry.rs`（新增 `inbound_queue_depth/bytes` 采样与
  `inbound_drain_events/wall_ms` 汇总字段）、`bins/game-svr/src/lib.rs`（仅在
  `run_inner` 通道创建处与 `EventDrain` 上挂**只读计数**）、测试模块。
- 做法：把 `std::sync::mpsc` 换成计数器包装（`record_enqueue`/`record_dequeue`）；
  「假消费者」压测先用 `#[ignore]` 测试（只入队+排水，不跑 World）。
- 验证：M1 得到真实队列长度曲线（替代 1.07M 推算）；M7 得到 drain/s 上限；
  全部现有测试与 `N=1` 对拍逐字节不变（无语义改动）。
- 与 `ea` Step 2 冲突面：`lib.rs` 的 `run_inner`/通道创建、`sim_loop` 排水块；Step 2 改
  `shard.rs` 与 `sim_loop` 的 World 构造——**同文件不同符号**为主，但 `sim_loop` 首尾有交集。

### Step 1：有界入站队列 + 分级丢弃（仍单消费者）

- 写集：新模块 `bins/game-svr/src/inbound.rs`（`InboundQueue`：帧数/字节双阀、`P0..P3`
  丢弃/合并、gauge），`lib.rs`（两条通道换成每区一条 `InboundQueue`，**先 1 区**），测试。
- 做法：`emit_gate_event` 改为投递到 `InboundQueue`；`EventDrain` 从队列取；`P0/P1` 永不丢、
  `P2` 同会话合并、`P3` 丢最旧；计数进 `ENTRY_TELEMETRY`。
- 验证：守恒恒等式、`N=1` 逐字节不变、溢出/合并可证（见 §4）。
- 冲突面：`lib.rs` 的 `emit_gate_event`/`EventDrain`/`sim_loop`；与 Step 2 的 `sim_loop`
  World 生命周期相邻，需先合 `ea` 的 Step 2 再落。

### Step 2：每区一个 owner（依赖 `ea` Step 2）

- 写集：`bins/game-svr/src/lib.rs`（`sim_loop` 拆成每区 `region_loop`；`World` 构造点）、
  `bins/game-svr/src/shard.rs`（区归属/`ShardRuntime` 下探到区）、`bins/game-svr/src/inbound.rs`。
- 做法：**每个新区 = 一个 `World` + 一条 `InboundQueue` + 一个 owner 线程**；入队前用
  「会话 → 区」查表路由；区 owner 按 FIFO 排水，单区语义与今天完全一致。
- 验证：A1/A2/A3（§4）+ 每区 tick 墙钟；先 N 区 **共用一个 worker**（串行）证明守恒/顺序，
  再逐区放开到多 worker。
- 冲突面：**与 `ea` Step 2 高度重叠**（同一 `sim_loop`/`shard.rs`）；必须排在 Step 2 之后，
  由 `ea` 的接口（区归属表、World 构造）承接。

### Step 3：并行排水 + 合并键

- 写集：`inbound.rs`（跨区投递与合并键）、`lib.rs`（跨区效果投递点）、`world-bevy` 的
  AOI/可见集（若跨区 halo 需要）。
- 做法：跨区效果不直接写对方 `World`，投到收件人所在区的队列，带
  `(tick, region_id, seq)`；收件人侧按该键有序合并；全服广播扇出在收件人区/网关线做。
- 验证：A4/A5（注入乱序、A 打 B + B 移动）+ 拆区设计 §Q3 合并键测试。
- 冲突面：跨区可见性/AOI 与`map-zone-split` 的 halo 实现同源；需与 `ea`/拆区线共同排期。

## 4. 三条验收断言（机判）

1. **守恒**：任意采样窗口、每区
   `enqueued == drained + dropped{all classes} + coalesced_dropped + pending`；
   `pending` 必须取队列 gauge，不得用「到达−排水」推算；偏差 >0 ⇒ RED（打印四项原值）。
2. **同一收件人帧序不变**：同一场景 `N=1` 与拆后 `N=区数` 各跑一遍，对每个收件人比较
   **帧序列逐字节相同**（`ident/param/tag/series/tail/due`；不是「条数相同」）；
   加强版：注入到达乱序但保留合并键 ⇒ 输出仍按 `(tick, region_id, seq)` 有序。
3. **`N=1` 逐字节不变**：`R=1` 对拍模式下现有 golden/parity/五门原样全绿，
   入站结构替换不得改变任何帧/数值/顺序；`EventDrainBudget` 与 `EventDrain::next` 的
   对外语义（优先/结转/反饥饿）在 `N=1` 下逐字节等价。

## 5. 最大风险（三个）

1. **单 World 所有权**：`World`/`online`/会话表都绑定 sim 线程；在任何多 World 之前做并行排水，
   只能得到「并行入队、串行处理」，**吞吐不变**且序更难证。这是本方案把 Step 2 放在并行之前的
   唯一理由。
2. **跨区合并键缺失**：没有 `(tick, region_id, seq)` 就并行处理，同一收件人的帧必然可能乱序；
   而 AOI 的 500ms 缓存（`ref_msg_cache`）与 657/640 的 due 语义会让乱序**表现为延迟/漏帧**，
   不一定立刻红。必须先有键、再并行。
3. **有界丢弃改变行为**：`P2` 合并/`P3` 丢弃在旧实现里不存在；若计数面不精确（`pending` 用推算、
   分类错误），会把「丢命令」伪装成「积压」，违反守恒断言。控制/进入（P0）任何情况下丢一条
   都是回归，必须有独立断言。

## 6. 施工顺序一句话

**先 Step 0 量 M1/M7 → Step 1 有界队列（N=1 逐字节不变）→ 等 `ea` Step 2 多 World →
Step 2 每区 owner（先串行）→ Step 3 并行 + 合并键。** 在 Step 2 之前，任何「分片 N 队列 +
单消费者」都只会增加复杂度，不会提高吞吐。
