# M1b 第 3 步实施规格：区 k 排水预算与有界化（2026-11-23）

> 父 `f0853f793`。上游依据：`l25-m1b-plan-20261123.md` §3.5（v2.1：新旧两套列并存、
> 溢出=硬安全阀非 0 即作废）。**本文件只定实施规格、不写码**；行号为 `f0853f793` 当日值，
> 代码自 `07b00e85` 未变；引用以**符号锚**为准（行号会漂）。
>
> **r2（2026-11-23，父 `30306bba`）补三处**：①§1.5 **覆盖范围硬边界**（只覆盖 zone-slicing，
> `shard_total>1` 的 `m2-event-router` 拓扑不覆盖）；②**锁内时长选 (ii) 时间上界**——
> `BATCH_LOCK_WALL=10ms`/`CHECK_EVERY=16` + `batch_remainder`（含依据与 FIFO 论证，见 §3.2/§3.3）；
> ③源码扫描牙按 qa-charter §12 明标"**接线提示，非行为证据**"（行为证据 = 计数牙 + 变异证据）。

## 0. 改动清单（一句话版）

- `lib.rs`：①区 k 线程循环（`spawn_minimal_zone`）把**两段裸 `try_recv`** 换成**有预算的排水 seam**；
  ②`spawn_zone_demux` 增加**每区 `outstanding` 计数 + `Command` 硬上限丢新**；
  ③`perf_counters` 增 **4 个 `zonek_*` 列 + 4 个 `*_all_zones` 派生列**（区 0 三列语义不动）。
- 不新增 cargo feature；多区路径才有区 k ⇒ **N=1 零参与**（`zonek_*` 恒 0、派生列 == 区 0 列）。
- **覆盖范围 = 仅 zone-slicing 拓扑（`MIR2_ZONES`）；`shard_total>1` 的分片拓扑不覆盖**（见 §1.5）。

## 1. `outstanding[z]` 的真实可行性（mpsc 没有长度，怎么数）

### 1.1 类型、所有权与创建点

- 类型：`type ZoneOutstanding = Vec<Arc<std::sync::atomic::AtomicUsize>>`，长度 = `zone_count`。
  **下标 z 与区号一致；z=0 恒不使用**（区 0 不设上限、不丢）。
- 创建点：多区规划处（与 `senders`/`receivers` 的 `for _ in 0..zone_count { let (tx,rx)=std::sync::mpsc::channel(); … }`
  同一作用域）——`let outstanding: ZoneOutstanding = (0..zone_count).map(|_| Arc::new(AtomicUsize::new(0))).collect();`
- 分两路 `Arc` 克隆：
  - `spawn_zone_demux(..., outstanding: ZoneOutstanding)`（demux 只读/加）；
  - 每区线程 `spawn_minimal_zone(..., outstanding: Arc<AtomicUsize>, zone_index: usize)`（只减）。
- 单区 / 非切片路径：不创建（不进入该规划分支）⇒ 零参与。

### 1.2 谁 +1、谁 -1（逐行路径）

**+1 = demux 转发 `Command` 且**真正 `send` 成功**之后。** 位置：`spawn_zone_demux` 的
`ZoneTarget::Zone(z)` 分支，现有代码为 `let _ = target.send(ev);`：

```text
ZoneTarget::Zone(z) => {
    if z >= 1
        && matches!(ev, GateEvent::Command { .. })
        && outstanding[z].load(Relaxed) >= ZONE_INBOUND_MAX_EVENTS
    {
        perf_counters::note_zonek_inbound_overflow_dropped();
        zonek_overflow_warn_limited(&ev);   // 限速 WRN（§1.4）
        continue;                            // 不入队、不 +1
    }
    let (normal, priority) = &senders[z];
    let target = if ev.is_priority() { priority } else { normal };
    let _ = target.send(ev);
    if z >= 1 && matches!(ev, GateEvent::Command { .. }) {
        outstanding[z].fetch_add(1, Relaxed); // 只跟在成功 send 之后
    }
}
```

- **只有 `GateEvent::Command` 参与计数与上限**：`Closed`/`Opened`/`RegisterGate`/`UnregisterGate`/
  `KickAccount`/`AdminGm` 一律**不限不丢**（丢 `Closed` 会泄漏会话、丢注册会让区失联）。
  `Broadcast`（注册/注销）走另一分支，**永不计**。
- mpsc 是无界 `std::sync::mpsc::channel`，`send` 不会失败 ⇒ "send 返回即入队"，+1 无歧义。

**-1 = 区 k 每 `try_recv` 拿到一条 `Command` 时减一次**（**在取出处**，不在处理处）：
- 区 k 排水 seam 的取数处：`match rx.try_recv() { Ok(ev) => { if let GateEvent::Command{..} = &ev { outstanding.fetch_sub(1, Relaxed); } batch.push(ev) } … }`。
- carry 探针取到的那一条也是 `try_recv` 的 `Ok` ⇒ **也 -1**（它虽然下 tick 才处理，但已离开通道）。
- 不变量：**每个 `Ok(Command)` 恰好一次 `fetch_sub`**；**每个实际入队的 `Command` 恰好一次 `fetch_add`**。

### 1.3 竞态、下界与泄漏面

- 内存序一律 `Relaxed`：这是**软上限**，只用于"避免无界"，不承担同步；计数偏差只影响阈值判定。
- **不会永久高估**：+1 只跟实际入队；-1 绑定每个 `try_recv Ok(Command)`；无界通道 + 单消费者 ⇒
  每条入队 `Command` 最终必被取出一次（区线程退出时仍在通道里的残留见下）。
  **丢弃路径绝不 +1**（丢新时 `continue` 在 `send` 之前）⇒ 不会因丢事件而永久虚高。
- **低估上界 = `BATCH_MAX + 2`（≤258）**：carry 探针取到的事件（≤2）与**时间上界收手时留在
  `batch_remainder` 里的批剩余（≤`BATCH_MAX`）**都已 -1 但尚未处理 ⇒ `outstanding` 比通道真实长度少
  ≤258。`>= CAP`（10,000）下最多放宽 2.6% ⇒ 可接受（写进常量注释）。
- **不会下溢**：`fetch_sub` 只在 `try_recv Ok(Command)` 处发生，而该条必先有一次 +1 ⇒ 计数 ≥ 0。
  debug 构建可在减前断言 `>0`（release 用裸 `fetch_sub`）。
- **shutdown 残留**：区线程退出时通道里可能还有 `Command`（`outstanding > 0`）——退出路径打一行
  WRN（zone + outstanding 值），**不视为错误**（进程随后整体退出；`Arc` 随进程释放）。
- 其它事件的通道积压不设上限：本切片只保护"**每玩家命令**"这条最可能被负载打爆的路径；
  `Closed/Opened` 等控制类不在上限内（数量级由连接数约束，且丢它们的代价更高）。

### 1.4 上限常量与观测

- `const ZONE_INBOUND_MAX_EVENTS: usize = 10_000;`（= 2000 条/tick × 5 拍）。注释写死：
  "软上限；正确性不依赖精确值；按窗口实测的区间到达率调整；低估 ≤ `BATCH_MAX+2`（批剩余 + carry 探针）"。
- 丢弃：`perf_counters::note_zonek_inbound_overflow_dropped()` + **限速 WRN**（复用
  `DepartWarnLimiter` 同型限流器：每分钟 ≤1 条，字段含 zone/line/socket/ident）。
- **生产应恒 0**；非 0 ⇒ 该窗口验收作废（v2.1 §3.5/§4 判据；测试见 §5-⑤）。

### 1.5 覆盖范围（**硬边界，必须写明**）

**本文的 `outstanding`/上限/计数只覆盖 zone-slicing 拓扑（`MIR2_ZONES`，事件经 `spawn_zone_demux`
→ `ZoneTarget::Zone(z)`）。** 代码里还有**第二条并行拓扑**：`shard_total > 1` 时的 `m2-event-router` +
`shard_router::route_target`（按 `chr_name` 路由到 per-shard 通道）——它**不经过 demux、也不经过本计数**。
- **M1b 不使用分片拓扑**（单 shard 多区 = 本规格覆盖的形态）。
- **将来若启用分片**，本机制必须按同一不变量**另接**（+1 在分片转发成功之后、-1 在分片消费取出处、
  各自一套 `outstanding` 与上限），**不得**假设"有计数器 = 全覆盖"。

## 2. 插入点（区 k 线程循环，符号锚）

当前形态（`spawn_minimal_zone`，`while !stop.load(...)` 内）：

```text
cross_zone_frames::begin_tick(tick);
[现有] while let Ok(ev) = zone_io.ev_rx.try_recv() { … 收集 closed_events / cmd_events / gates … }
[现有] while let Ok(ev) = zone_io.ev_ctl_rx.try_recv() { … 同上 … }
if !closed_events.is_empty() { 锁一次、逐条 handle_zone_closed }
if !cmd_events.is_empty()     { 锁一次、逐条 handle_zone_command }
[现有] 锁1：zone_tick_top(tick,&gates) + tick_phase_a
[现有] 锁外：await_neighbors
[现有] 锁2：tick_phase_b；锁3：handoff offer + 其余段
```

**替换范围**：上面两段 `while let Ok(ev) = …try_recv()` **整段**换成"§3 的预算排水 seam 调用"；
位置**不变**（仍在 `begin_tick` 之后、`if !closed_events…` 之前）。`closed_events`/`cmd_events`
的收集语义由 seam 通过批次回调完成（回调内按变体归集，处理仍沿用现有"锁一次处理整批"）。
**`zone_tick_top` 与三把锁的位置一律不动**（A6 的 tick 口径已改本地 `tick`，见 v2 §3.2）。

## 3. 排水 seam 的形态（carry 与预算）

### 3.1 为什么不直接复用 `EventDrain`

`EventDrain::next(&Receiver, &Receiver, &mut carry, &mut carry)` **本身可以用在区 k**
（`zone_io.ev_rx`/`ev_ctl_rx` 就是 `Receiver`）；不能直接复用的原因是**处理模型不同**：
`EventDrain` 的契约是"**取出即处理、处理耗时计入 `max_wall`**"（区 0 的形态）；区 k 现有形态是
"**先收集一批、再锁一次处理**"（避免每条命令一次锁）。若把处理塞进 `EventDrain` 循环，要么变成
每事件一次锁、要么长时间持锁——两者都比"按批"差。故区 k 自建**同型**预算（数值**复用**
`EventDrainBudget::PRODUCTION`，不新写数字；若其字段不可读，给该常量加
`pub(crate) const PRODUCTION_MAX_EVENTS/PRODUCTION_MAX_WALL` 或 `fn limits()`，**不得手抄**）。

### 3.2 seam 签名与循环

```text
struct ZoneDrainStats { drained: u64, from_carry: u64, budget_hit: bool, batches: u32 }

struct ZoneDrainLimits {           // 生产 = 由 EventDrainBudget::PRODUCTION 推出的常量；测试可注入小值
    max_events: u64,               // 2000
    max_wall: Duration,            // 50ms（本 tick 总预算：取数 + 处理）
    batch_max: usize,              // 256（填批上限）
    batch_lock_wall: Duration,     // 10ms（单批**锁内**时长上界）
    check_every: usize,            // 16（锁内每 N 条检查一次墙钟；批末条必查）
}

fn drain_zone_events(
    ctl_rx: &std::sync::mpsc::Receiver<GateEvent>,
    rx: &std::sync::mpsc::Receiver<GateEvent>,
    carry_ctl: &mut Option<GateEvent>,        // 跨 tick 保活（循环外局部）
    carry_normal: &mut Option<GateEvent>,
    batch_remainder: &mut VecDeque<GateEvent>, // 时间上界收手时的批尾（跨 tick 保活）
    outstanding: &AtomicUsize,                // 取 Command 时 -1
    limits: &ZoneDrainLimits,
    mut on_batch: impl FnMut(&mut Vec<GateEvent>) -> usize, // 生产：锁一次 + 逐条处理，返回已处理条数
) -> ZoneDrainStats
```

循环（每 tick 调用一次）：

0. **时间参照**：`let tick_t0 = Instant::now();`（本 tick 总预算的墙钟基准）。
1. **先把"上 tick 的剩余"放进批首（保序）**：`batch_remainder`（`VecDeque`，≤`BATCH_MAX`）整段前插 →
   `carry_ctl.take()` → `carry_normal.take()`（ctl 先）；`stats.from_carry` 记这些"上 tick 已取出未处理"的条数。
2. **填批**：若 `batch_remainder` 起点已非空则不填（先把剩余跑完）；否则
   `while batch.len() < limits.batch_max`：先 `ctl_rx.try_recv()`、再 `rx.try_recv()`；
   每次 `Ok` 时若为 `Command` 就 `outstanding.fetch_sub(1)`。空批 ⇒ 结束。
3. **处理（锁内时间上界）**：`on_batch(&mut batch)` 返回**实际处理条数** `n`——生产闭包持锁逐条处理，
   **每 `limits.check_every` 条（及批末）**检查 `lock_elapsed >= limits.batch_lock_wall`；超界 ⇒ 在
   **一整条处理完之后**收手，把**未处理的批尾整体**移进 `batch_remainder`（不丢、保序），
   `stats.budget_hit = true`，`stats.drained += n`，**结束本 tick**（下一 tick 先跑剩余）。
4. **总预算检查**（批与批之间）：未收手时，若 `stats.drained >= limits.max_events ||
   tick_t0.elapsed() >= limits.max_wall` ⇒ 转 5（探针）；否则回 2。
5. **探针**（预算耗尽且无剩余）：`ctl_rx.try_recv()` 有 ⇒ 存 `carry_ctl` 且 `budget_hit=true`；否则试
   `rx.try_recv()` ⇒ 存 `carry_normal` 且 `budget_hit=true`；两个都空 ⇒ `budget_hit=false`（本 tick 已排空）。
6. 出口：`note_zonek_inbound_drained(from_carry, ev)`（**写 `zonek_*`，禁止 `note_inbound_drained`**）；
   `budget_hit` ⇒ `note_zonek_inbound_budget_hit()`。

**锁内时长的依据（二选一：本规格选 (ii) 时长上界，理由如下）**：
- **参考推算（不是保证）**：区 0 的 `EventDrainBudget::PRODUCTION`（2000 条 / 50ms）隐含"平均处理
  ≤25µs/条"的近似上界；`256 × 25µs ≈ 6.4ms`，`batch_lock_wall = 10ms` 留 ~1.5× 余量。
- **不能只靠均值**：`CM_SAY` 的广播等单条成本可能远高于均值（收件人 O(n)），"256×均值"不构成
  单条上界 ⇒ **必须有时间上界**：即使单条慢，锁内也只在超过 10ms 的**那条处理完后**收手，
  最坏锁内 = 10ms + 单条成本，仍远小于 200ms tick；总预算 50ms 由第 4 步兜住。
- **不破 FIFO**：收手点只在"一整条处理完"之后，未处理批尾**整段**移进 `batch_remainder`，
  下一 tick 按序先跑（见 §3.3 装配顺序）。

### 3.3 跨 tick 载体（carry + 批剩余）的语义与保序

- **三类"已取出、未处理"的载体**，全部声明在**区 k 线程循环之外**（`while !stop` 之前，与 `gates` 同级，
  跨 tick 保活；不放进 `HostLoop`/`ZoneState`——排水在 `HostLoop::tick` 之外）：
  - `carry_ctl` / `carry_normal: Option<GateEvent>`：预算耗尽后**探针**取到的通道队头一条（≤2 条）；
  - `batch_remainder: VecDeque<GateEvent>`：**时间上界收手**时未处理的批尾（≤`BATCH_MAX` 条）。
- **下一 tick 的装配顺序（写死）**：`batch_remainder` → `carry_ctl` → `carry_normal` → 通道。
  理由：剩余比探针**更早取出**（剩余来自本轮批，探针永远取通道当时的队头）⇒ 顺序即到达序，
  不越过任何更早事件；两通道各自 FIFO，跨通道仍 **ctl 优先**（与现有两段 `try_recv` 次序一致）。
- 与 `EventDrain` 的 carry 语义差别：EventDrain 只有一个 carry 槽、且契约保证"取出即处理"；
  本设计的处理是**成批 + 锁内时长上界**，所以除了探针 carry 还需要**批剩余**（用 `VecDeque`，不是 Option）。
- 统计口径：`stats.from_carry` / `zonek_inbound_carry` = "本 tick 处理的、**上 tick 已取出**的条数"
  （含 `batch_remainder` 与两个探针槽）。
- `BATCH_MAX=256` 只是**填批上限**；**锁内时长由 `BATCH_LOCK_WALL=10ms` 约束**（依据见 §3.2）。

## 4. 计数与 TSV 列（区 0 三列一字不动）

- `perf_counters` 新增（区 k 聚合，全部区 k 实例求和）：
  `ZONEK_INBOUND_DRAINED / ZONEK_INBOUND_CARRY / ZONEK_INBOUND_BUDGET_HIT / ZONEK_INBOUND_OVERFLOW_DROPPED: AtomicU64`
- 四个 note 函数（同形于 `note_inbound_drained`；**只写 `zonek_*`**）：
  - `note_zonek_inbound_drained(from_carry: bool, ev: &GateEvent)`：drained += 1；from_carry ⇒ carry += 1；
  - `note_zonek_inbound_budget_hit()`；`note_zonek_inbound_overflow_dropped()`；
  - `#[cfg(test)]` 读点（四列各一）。
- TSV：`write_tick` 的表头与 values **同位置**追加 4 列（紧跟 `depart_frames_mirror_suppressed` 之后）；
  另加 **4 个派生列**（写行时**显式求和**，不独立采集）：
  `inbound_drained_all_zones / inbound_carry_all_zones / inbound_budget_hit_all_zones /
  inbound_overflow_dropped_all_zones` = 区 0 列 + 对应 `zonek_*` 列。
- 列文写死：`zonek_*` = "仅区 k 聚合（slicing）"；`*_all_zones` = **derived（区 0 + zonek）**，
  **不可与单区列混比**。旧列语义不变（新旧两套并存、互不覆盖）。
- N=1：`zonek_*` 恒 0；派生列 == 区 0 列（测试钉住）。

## 5. 五条放行测试（构造 / 断言 / 失败打印）

测试全部作用于 **seam 与 demux 转发两个可测函数**（不驱动整条区线程），因此不改产品语义。

1. **`zonek_drain_honors_event_budget`**（预算 + 行为计数牙）
   - 构造：`std::sync::mpsc::channel()` 注入 2500 条 `GateEvent::Command`（+ 少量 `Closed` 作对照）；
     调 `drain_zone_events`（`on_batch` 只计数、不锁）；`limits` 用生产值（`PRODUCTION` 推得）。
   - 断言：`stats.drained <= 2000`；`elapsed <= 50ms + ε`；通道剩余 = 2500 - drained（用随后
     清空再计数）；**`zonek_inbound_drained` 的增量 == `stats.drained`**（行为计数牙——计数只随
     seam 的真实排水增长）。
   - 打印：`drained={} elapsed_ms={} left={} batches={} zonek_drained_delta={}`。
2. **`zonek_carry_and_remainder_preserve_fifo`**（carry/批剩余与保序）
   - **2a 探针 carry**：注入 A、B、C 三条 `Command`；`limits.max_events = 1`（测试注入）⇒ 第一次调用
     处理 A、探针把 B 存入 `carry_ctl`/`carry_normal`；第二次调用断言**先**处理 B（`stats.from_carry==1`），再到 C。
   - **2b 时间上界 + 批剩余**：注入 A..H 八条；`limits.batch_lock_wall = 1ms`、`check_every = 1`；
     `on_batch` 每条 sleep 500µs（测试闭包）⇒ 第一次调用在 1ms 后**一整条处理完**处收手，剩余批尾
     进 `batch_remainder` 且 `budget_hit=true`；第二次调用断言**先**跑剩余、顺序仍 A..H。
   - 断言：两次调用的合计顺序恰为 A..H（**不丢、不重、不乱序**）；`stats.from_carry` 覆盖 2a 的 1 条 +
     2b 的剩余条数；`budget_hit` 第一次 true、跑空后 false。
   - 打印：`order={:?} from_carry={} remainder_len={} hit={}`。
3. **`zonek_overflow_drops_new_and_counts`**（硬安全阀）
   - 构造：把 demux 转发抽成 `fn forward_to_zone(ev:&GateEvent, zone:usize, outstanding:&AtomicUsize,
     senders:&..) -> bool`；`outstanding = CAP-1`，转发 2 条 `Command`。
   - 断言：第 1 条 true（并 +1 ⇒ CAP）；第 2 条 **false**（被丢）；`zonek_inbound_overflow_dropped` +1；
     `outstanding` 仍 = CAP（**未 +1**）；`Closed`/`Broadcast` 永不丢（对照各发一条）。
   - 打印：`first={} second={} outstanding={} dropped_counter={}`。
4. **`zonek_columns_aligned_and_derived`**（列不混比 + 派生恒等式）
   - 构造：writer 温度同 `perf_tsv_header_matches_values_with_new_column`：bump 区 0 三列与 `zonek_*` 四列各 2；
     写一行；再写一行 **N=1 形态**（只 bump 区 0）。
   - 断言：表头/值列数一致；四个 `zonek_*` 落在同名列；派生列 == 区 0 + zonek（第一行）；
     N=1 行 `zonek_*` == 0 且派生 == 区 0；**旧列值在两行里各自等于"只 bump 区 0 的部分"**（未被污染）。
   - 打印：`header_cols={} value_cols={} derived_ok={} legacy_untainted={}`。
5. **负例：`zonek_drain_wiring_tooth`（"改回无界 ⇒ 计数恒 0 ⇒ 红"，不改产品语义）**
   - **做法**：排水 seam 是区 k 循环里 `drain_zone_events` 的**唯一调用点**；加一条**窗口受限的源码扫描牙**
     （与仓内既有 window-bound 用例同法：窗口 = `spawn_minimal_zone` 里 `begin_tick` 之后到 `zone_tick_top`
     之前的行）：断言 ①含 `drain_zone_events(` **恰一次**；②该窗口**不含** `ev_rx.try_recv()`/
     `ev_ctl_rx.try_recv()`（裸排已删）。
   - **定性（qa-charter §12）：这条源码牙是"接线提示（wiring hint）"，*不是*行为证据。**
     行为证据 = **§5-1 的 `zonek_inbound_drained` 增量断言**（驱动 seam 后计数必须增长）+
     下面的变异证据。
   - **配合行为断言**：测试 1 驱动 seam 后断言 `zonek_inbound_drained` 增加；若有人把循环改回裸
     `try_recv`（不经 seam），计数恒 0 ⇒ 行为计数牙 + 源码提示双重暴露。
   - **不改产品语义**：牙只读源码与调用 seam，不引入开关；真正的"无界"变异作为**变异证据**做
     （删预算检查分支 ⇒ 测试 1 的 `drained<=2000` 红；恢复后文件 sha 相等）。
   - 打印：源码牙失败时打印窗口原文与命中数；行为牙打印 `zonek_drained_before/after`。

## 6. 风险与回滚

- **面**：只动"区 k 线程循环排水段 + demux 转发 `ZoneTarget::Zone` 分支 + perf 计数/TSV + 测试"。
  **N=1（无区 k）零参与**：`spawn_minimal_zone` 只在多区模式调用；`zonek_*` 恒 0、派生列 == 区 0 列。
- **不新增 cargo feature**：默认构建即新路径（区 k 路径只在 `MIR2_ZONE_SLICING` 下存在）。
- **回滚**：单一提交 revert 即可；若需线上临时绕过，可加 `MIR2_ZONEK_DRAIN_BUDGET=0` 回退到旧裸
  `try_recv`——**但**该回退会把 `zonek_*` 置 absent/0 并在文档注明"该窗口不可作验收"（默认不启用，
  避免两套语义并存成为常态）。
- **风险点与缓解**：
  1. 锁内批处理时长：`BATCH_MAX=256` 只是**填批上限**；**锁内时长由 `BATCH_LOCK_WALL=10ms`
     （`CHECK_EVERY=16` 条一查、批末必查、只在一整条处理完后收手）**约束——即使单条很慢，
     锁内最坏 = 10ms + 单条；窗口实测若 `BATCH_LOCK_WALL` 仍偏大则下调；
  2. `outstanding` 低估 ≤ `BATCH_MAX + 2`（批剩余 + carry 探针）：CAP=10,000 下最多放宽 2.6%，注释写明；
  3. 丢新只对 `Command`：控制类事件（`Closed/Opened/注册`）永不丢——若将来要限控制类，另立卡。
- **不合并/回滚判据**：§5 五条不全绿；或任一验收窗口 `zonek_inbound_overflow_dropped != 0`
  （v2.1：该窗口作废并开缺陷卡）。

## 7. 交付时报告什么

- 改动文件与 `+/-` 行数；常量值（`ZONE_INBOUND_MAX_EVENTS`、`BATCH_MAX`、`BATCH_LOCK_WALL`、
  `CHECK_EVERY`）与取值理由（含 §3.2 的均值参考 vs 单条上界论证）；
- 覆盖范围声明：仅 zone-slicing（§1.5），`shard_total>1` 不覆盖；
- 五条测试的原文输出（各自一行打印）+ 变异证据（删预算检查 ⇒ 测试 1 红；恢复后 sha 相等）；
- TSV 表头新增列清单与一行样例值（含派生列恒等式）；
- N=1 零参与证据（区 k 不存在时 `zonek_*` 全 0，派生列 == 区 0 列）。
