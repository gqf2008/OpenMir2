# M1b 第 3 步实施规格：区 k 排水预算与有界化（2026-11-23，r3）

> 父 `d23f194dc`。上游依据：`l25-m1b-plan-20261123.md` §3.5（v2.1：新旧两套列并存、
> 溢出=硬安全阀非 0 即作废）与 §5.3（执行留在 `INBOUND_LOCAL` 段内）。**本文件只定实施规格、
> 不写码**；引用以**符号锚**为准（行号会漂）。
>
> **r3（2026-11-23，父 `d23f194dc`，opc-27 裁决 + 0c 复核的 9 条一次落盘）**：
> ①**执行落点按 §5.3**——**填充在循环顶（锁外），执行在 `INBOUND_LOCAL` 段内**（§2/§3）；
> ②`-1` 挪到**从 `pending_inbound` 消费/执行时**（单缓冲、无双 carry；§1.2/§3.3）；
> ③`zonek_inbound_carry` 用**水位法**（`carry_prefix`，不加 tick 标签；§3.3）；
> ④预算/执行参数细分 & `budget_hit` 两处判据（§3.2）；⑤五条测试按确定性收紧（§5）；
> ⑥shutdown 残留**丢弃并计数**（新增 `zonek_inbound_pending_discarded`；§1.4/§4）；
> ⑦§6 补批/段持锁的两条传导与 p99.9 守门（无实测不伪造）；⑧plan §5.3 的 `> 2` 同步改 `>= 2`。
>
> **r4（2026-11-23，父 `7e54d4f9b`，0c 二轮 M0–M3 措辞定稿）**：
> **M0 缓冲归属钉死**——`pending_inbound` 是**区 k 线程循环的局部 scratch**（**不进 `HostLoop`**）；
> 交接走 **`TickIo` 新字段** `inbound_local_buffer: VecDeque<GateEvent>` +
> `inbound_local_carry_prefix: usize`（driver **锁外** swap；N=1 常空、零参与）。
> **M1** 删除 `clear()` 措辞，改显式 `pop_front` 执行循环（超界只停「下一条」，剩余自然保留）。
> **M2** 写死计时分工：`tick_max_wall` 仅名义总上界、**执行以 `execute_wall` 为准**（不跨阶段累加）。
> **M3** 守门补越界项 `execute_wall + (execute_check_every−1) × 单条 p99.9`。

## 0. 改动清单（一句话版）

- `lib.rs`：①区 k 线程循环（`spawn_minimal_zone`）把**两段裸 `try_recv`** 换成**有预算的填充**
  （锁外）→ 命令进 `TickIo.inbound_local_buffer`；②`INBOUND_LOCAL` **段内**消费+分类+执行+记账；
  ③`spawn_zone_demux` 增加**每区 `outstanding` 计数 + `Command` 硬上限丢新**；
  ④`perf_counters` 增 **5 个 `zonek_*` 列 + 5 个 `*_all_zones` 派生列**（区 0 三列语义不动）。
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

### 1.2 谁 +1、谁 -1（逐行路径）与不变量

**+1 = demux 转发 `Command` 且 `send` 成功之后。** 位置：`spawn_zone_demux` 的 `ZoneTarget::Zone(z)`：

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
    if target.send(ev).is_ok() && z >= 1 && matches!(ev, GateEvent::Command { .. }) {
        outstanding[z].fetch_add(1, Relaxed);   // 只在真正的 send 成功后 +1
    }
}
```

- **只有 `GateEvent::Command` 参与计数与上限**：`Closed`/`Opened`/`RegisterGate`/`UnregisterGate`/
  `KickAccount`/`AdminGm` 一律**不限不丢**（丢 `Closed` 会泄漏会话、丢注册会让区失联）；
  `Broadcast`（注册/注销）走另一分支，**永不计**。
- `send` 失败的唯一情形是接收端已 drop（区线程已死）⇒ 此时不 +1，事件随区死整机 abort 一次性消失。

**-1 = 区 k 从 `pending_inbound` `pop_front` 取出、执行体之前**（**消费时**，不在 fill、不在执行后）：

```text
// INBOUND_LOCAL 段内，逐条执行前：
if let Some(ev) = buffer.items.pop_front() {
    if let GateEvent::Command { .. } = &ev { outstanding.fetch_sub(1, Relaxed); } // 取出即 -1
    execute(ev);   // 执行体
}
```

- **不变量（写死）**：
  1. **每条 `Command` 恰 `+1` 一次**——在 demux `send` 成功之后；
  2. **恰 `-1` 一次**——在从 `pending_inbound` 取出执行时；**丢新路径不 +1**；
  3. **shutdown 时 `pending_inbound` 里残留的 `Command` 逐条 `-1` 并计 `zonek_inbound_pending_discarded`**
     （不静默）；**通道里尚未 fill 的残留**无法 -1——退出路径打 WRN（zone + outstanding），不视为错误
     （进程整体退出，`Arc` 释放）。

### 1.3 竞态、下界与泄漏面

- 内存序一律 `Relaxed`：**软上限**，只用于"避免无界"，不承担同步。
- **`outstanding` = 通道 + `pending_inbound` = 真实未处理积压**（-1 在消费时）⇒ cap 守的正是该守的；
  fill 快于执行 ⇒ pending 增长会进 `outstanding` ⇒ demux 到 CAP 停投 ⇒ **真·有界**。
- **低估 ≤ 1**：执行中那一条已 -1、未算；相对 CAP=10,000 可忽略（写进常量注释）。
- **不会下溢**：`fetch_sub` 只对"曾 +1 过的 `Command`"发生（消费或 shutdown 丢弃）。debug 构建可在减前断言 `>0`。
- **pop 后执行体 panic**：该条已 -1 未执行；但 panic = 区死 = `catch_unwind` → 整机 abort，**良性**。
- 其它事件（`Closed/Opened/注册`）不设上限：本切片只保护"每玩家命令"这条最可能被负载打爆的路径。

### 1.4 上限常量与观测

- `const ZONE_INBOUND_MAX_EVENTS: usize = 10_000;`（= 2000 条/tick × 5 拍）。注释写死：
  "软上限；正确性不依赖精确值；按窗口实测的区间到达率调整；**低估 ≤1**（消费时 -1）"。
- 丢弃观测：`note_zonek_inbound_overflow_dropped()`（丢新）+ `note_zonek_inbound_pending_discarded()`
  （shutdown 残留丢弃），各自**限速 WRN**（复用 `DepartWarnLimiter` 同型：每分钟 ≤1 条，字段含
  zone/line/socket/ident）。
- **生产应恒 0**；`overflow_dropped` 非 0 ⇒ 该窗口验收作废（v2.1 §3.5/§4 判据，测试见 §5-③）。

### 1.5 覆盖范围（**硬边界**）

**本机制的 `outstanding`/上限/计数只覆盖 zone-slicing 拓扑（`MIR2_ZONES`，事件经 `spawn_zone_demux`
→ `ZoneTarget::Zone(z)`）。** `shard_total > 1` 的 `m2-event-router` + `shard_router::route_target`
（按 `chr_name`、per-shard 通道）**不经过 demux、也不经过本计数**；M1b 不使用分片拓扑；将来启用分片
必须按同一不变量**另接**，**不得假设"有计数器 = 全覆盖"**。

## 2. 插入点（区 k 线程循环，符号锚）

当前形态（`spawn_minimal_zone`，`while !stop` 内）：

```text
cross_zone_frames::begin_tick(tick);
[现有] while let Ok(ev) = zone_io.ev_rx.try_recv() { … 收集 closed_events / cmd_events / gates … }
[现有] while let Ok(ev) = zone_io.ev_ctl_rx.try_recv() { … 同上 … }
if !closed_events.is_empty() { 锁一次、逐条 handle_zone_closed }
if !cmd_events.is_empty()     { 锁一次、逐条 handle_zone_command }
[现有] 锁1：zone_tick_top(tick,&gates) + tick_phase_a(M1A_PROD)
[现有] 锁外：await_neighbors；锁2：tick_phase_b；锁3：handoff offer + 其余段
```

**r3 形态（执行落点按 plan §5.3）**：

1. **循环顶（锁外）只做填充**：把两段裸 `try_recv` 换成 `fill_pending(...)`：按预算从两通道
   `try_recv` **命令**，append 进**区线程循环的局部 scratch** `pending_inbound: VecDeque<GateEvent>`
   （声明在 `while !stop` 之外，与 `gates`/`tick` 同级；**fill 全程不持 `state` 锁**，与今天
   "锁外 `try_recv` 到局部"的锁形态一致）；`carry_prefix` 就在这块 scratch 上算（§3.3）。
   `Closed/Register/Unregister` 仍在循环顶就地处理（归集 `closed_events` / 更新 `gates`），**不动**。
2. **`INBOUND_LOCAL` 段内执行（锁 1 下）**：交接**走 `TickIo` 新增的两个字段**（§3.3 已选这一种并写死，
   不用段函数入参）——driver 在**锁外**（`TickIo` 是 driver 局部，不持 `state` 锁）把 scratch
   `mem::swap` 进 `zone_io.inbound_local_buffer` 并写入 `zone_io.inbound_local_carry_prefix`；
   `tick_phase_a` 跑段；tick 结束后再 swap 回 scratch（剩余自然带回）。段内**先消费 A6 的
   `pending_local`（延迟队列）→ 再消费 `io.inbound_local_buffer`**，逐条分类 / 执行 / 记账
   （`zonek_*`、`RAN_SEGMENTS |= INBOUND_LOCAL`；`HostInbound` 计时天然覆盖）。
3. **顺序不变性（写死）**：今天是"循环顶先 `Closed` 后 `Command`"；改后 `Closed` 仍在循环顶、
   `Command` 挪到段内 ⇒ **相对序不变（`Closed` 先于 `Command`）**，只是 `Command` 的**执行时刻**
   从"tick 前"变"tick 内 A 段"。**不得**据此认为顺序改变。
4. `zone_tick_top` 与锁 1/2/3 的位置不动（A6 的 `at_tick` 一律用循环本地 `tick`，见 §6 末）。

## 3. 填充与执行（seam 形态、单缓冲、carry 水位）

### 3.1 为什么不是 `EventDrain`

`EventDrain::next(&Receiver, &Receiver, &mut carry, &mut carry)` **可以**用在区 k（两个 `Receiver` 都在）；
不直接复用的原因是**处理模型不同**：它的契约是"取出即处理、处理耗时计入 `max_wall`"（区 0 形态），
而 r3 是"**fill（锁外）→ 段内执行（锁 1 下）**"，两段分离且各自有预算。预算**数值**仍复用
`EventDrainBudget::PRODUCTION`（2000 条 / 50ms；不手抄）。

### 3.2 参数与预算（细分、可注入）

```text
struct ZoneDrainLimits {
    tick_max_events: u64,       // 2000（PRODUCTION）：本 tick「fill + 执行」的**总条数**上限
    tick_max_wall: Duration,    // 50ms（PRODUCTION）：本 tick 总墙钟上界（fill 已有 + 段执行）
    pull_cap_events: u64,       // = tick_max_events（一次 fill 的上限；测试可设小值）
    execute_max_events: u64,    // = tick_max_events（本 tick 段执行的条数上限；测试可设小值）
    execute_wall: Duration,     // 10ms（**段内执行**的墙钟上界；加到 A 段/屏障1 到达，见 §6）
    execute_check_every: usize, // 16（每 N 条查一次 execute_wall，批末必查）
}
```

- **fill**（循环顶、锁外）：每调用最多 pull `pull_cap_events` 条；**预算耗尽后探一次**：
  `try_recv Ok` ⇒ **直接 push 进 `pending_inbound`（scratch）** 且 `fill_hit=true`；`Err(Empty)` ⇒ `fill_hit=false`。
  （**探针不另设 carry**——它进的就是同一缓冲。）
- **段执行**（锁 1 内）：从 `pending_local` 起、再 `io.inbound_local_buffer`；每 `execute_check_every` 条
  （及工作区末条）查 `execute_wall`，超界 ⇒ **在一整条执行完之后**收手，剩余留在工作区（保序）；
  `execute_max_events` 到顶同理。**收手后工作区非空 ⇒ `budget_hit=true`（不做探针）**。
- **`budget_hit` 两处判据（写死）**：`fill_hit`（fill 探针取到事件并留在缓冲）**或** 段执行截断且工作区非空。
- **计时分工（写死）**：**执行以 `execute_wall` 为准**；`tick_max_wall`（PRODUCTION 的 50ms）只作
  **名义总上界**——fill 与段执行是分离阶段，**不在二者之间实测/累加墙钟**（避免实现者纠结跨阶段计时）；
  fill 侧唯一硬上限是 `pull_cap_events`（条数）。

### 3.3 单缓冲、carry 水位与保序

- **缓冲归属（钉死，二选一已定：走 `TickIo`，不用段函数入参）**：
  - **跨 tick 缓冲 = 区 k 线程循环的局部 scratch** `pending_inbound: VecDeque<GateEvent>`
    （声明在 `while !stop` 之外，与 `gates`/`tick` 同级）；**fill 锁外 append**、`carry_prefix`
    在这块 scratch 上算。
  - `TickIo` 新增 **`inbound_local_buffer: VecDeque<GateEvent>` + `inbound_local_carry_prefix: usize`**
    （本 tick 的输入队列工作区）。driver 在**锁外**（`TickIo` 是 driver 局部，不持 `state` 锁）
    `mem::swap` 进、tick 结束后 swap 回；`carry_prefix` 同步写入字段。**N=1 时该字段常空、零参与。**
  - **为什么不放 `HostLoop`**：`HostLoop` 字段 = zone-owned 状态，改它必须持锁；而缓冲是
    "**本 tick 的输入队列**"，本质属 `TickIo` 一类 ⇒ 放循环局部/`TickIo` 既不破 `HostLoop` 不变量，
    也不需要锁外改 `HostLoop`（也不增加 `state` 锁次数）。
- `carry_prefix` 在**每次 fill 追加之前**重算：`carry_prefix = pending_inbound.len()`（= 上 tick 遗留
  条数），随 swap 写进 `inbound_local_carry_prefix`；段内按**工作区下标**：`idx < carry_prefix` ⇒
  `from_carry`（`zonek_inbound_carry += 1`），否则普通 drained。**每条恰计一次**（执行才 `drained++`；
  未执行剩余不计，下一 tick 按新 `carry_prefix` 计 from_carry）。`pending_local`（A6 延迟队列）
  **不计入 carry**、走自己的计数。
- **执行循环不写 `clear()`**（避免抹掉剩余）：段内写成
  `while let Some(ev) = io.inbound_local_buffer.pop_front() { 取出即 -1；执行整条；每 execute_check_every
  条（及末条）查 execute_wall，超界即停（不再 pop 下一条） }` ⇒ 未执行的自然留在工作区，tick 后
  swap 回 scratch。**全程不重建、不逐 tick 分配**（`VecDeque` 容量随 swap 复用）。
- **保序（到达序）**：段内顺序 = `pending_local`（已处理、缓执行，更早）→ `inbound_local_buffer`
  （未处理，次早；按 fill 追加序）→ 本 tick 尚未 fill 的新命令（更晚）。三类严格按到达序分段，
  不会乱序。`Closed/Register` 在循环顶先于任何 `Command`（§2-3 顺序不变性）。
- 探针取到的事件已进入缓冲、下标 ≥ `carry_prefix` ⇒ **不计 from_carry**（它是"新 fill"）。

### 3.4 与 A6 延迟队列的分层（封口）

- `deferred_local` / `pending_local`（A6 的每玩家延迟队列）**不在通道、不计 `outstanding`、不受通道丢新影响**；
- 通道级丢新只发生在 demux，丢的是**尚未分类、尚未进通道**的新 `Command`，与 `control_in_flight` **无交互**；
- 因此「控制命令生效 + 某条本地命令被丢」的唯一形态只出现在 `zonek_inbound_overflow_dropped > 0` 的
  **作废运行**（v2.1 判据：该窗口作废 + 开缺陷卡）；**有效运行里不存在**。

## 4. 计数与 TSV 列（区 0 三列一字不动）

- `perf_counters` 新增（区 k 聚合，全部区 k 实例求和）：
  `ZONEK_INBOUND_DRAINED / ZONEK_INBOUND_CARRY / ZONEK_INBOUND_BUDGET_HIT /
  ZONEK_INBOUND_OVERFLOW_DROPPED / ZONEK_INBOUND_PENDING_DISCARDED: AtomicU64`
- note 函数（同形于 `note_inbound_drained`；**只写 `zonek_*`，禁止调 `note_inbound_drained`**）：
  `note_zonek_inbound_drained(from_carry, ev)`、`note_zonek_inbound_budget_hit()`、
  `note_zonek_inbound_overflow_dropped()`、`note_zonek_inbound_pending_discarded()`；
  `#[cfg(test)]` 读点各一。
- TSV：`write_tick` 表头与 values **同位置**追加 5 列（紧跟 `depart_frames_mirror_suppressed` 之后）；
  另加 **5 个派生列**（写行时显式求和、不独立采集、列文注 derived、不可与单区列混比）：
  `inbound_drained_all_zones / inbound_carry_all_zones / inbound_budget_hit_all_zones /
  inbound_overflow_dropped_all_zones / inbound_pending_discarded_all_zones`
  = 区 0 列 + 对应 `zonek_*` 列（区 0 无同名时按 0 起算）。
- 列文写死：`zonek_*` = "仅区 k 聚合（slicing）"；`*_all_zones` = derived。旧列语义不变
  （新旧两套并存、互不覆盖）。N=1：`zonek_*` 恒 0、派生 == 区 0 列。

## 5. 五条放行测试（确定性；不改产品语义）

测试全部作用于 **`fill_pending` / 段执行函数 / demux 转发**三个可测单元（不驱动整条区线程）。

1. **`zonek_fill_honors_budget`**（fill 预算；墙钟只打印）
   - 构造：真 `mpsc::channel()` 注入 2500 条 `Command`（+ 少量 `Closed`）；`limits` 注入生产值，
     `execute_max_events` 设大（让执行不是瓶颈）；先 `fill_pending` 再执行。
   - **硬断言**：`filled <= pull_cap_events`、`drained <= execute_max_events`、`budget_hit == true`、
     **`zonek_inbound_drained` 增量 == drained**、**`outstanding == 通道残留 + 缓冲残留`**；
     墙钟 **只打印**（`elapsed_ms=…`）不判红。
2. **`zonek_pending_remainder_and_carry_prefix`**（剩余保序 + 水位；limits **按调用**注入）
   - 注入 A..H；call1：`pull_cap_events=8`、`execute_max_events=2`、`execute_wall=极大`、
     `execute_check_every=1` ⇒ 执行 2、剩 6 留缓冲、`budget_hit=true`、`from_carry=0`；
     call2：**`execute_max_events=6`**（其余同）⇒ 执行剩余 6、**`from_carry=6`**、`budget_hit=false`。
   - 断言：两次合计顺序 A..H（不丢/不重/不乱序）；**call2 后 `outstanding == 0`**（通道空 + 缓冲空
     ⇒ 每条 +1/-1 归零）；注释写明"limits 是**每调用**参数，不是跨调用常量"。
3. **`zonek_overflow_drops_new_and_counts`**（硬安全阀）
   - `outstanding = CAP-1`，转发 2 条 `Command`：第 1 条 `send().is_ok()` ⇒ +1（=CAP）；
     第 2 条被丢（`continue`）⇒ `zonek_inbound_overflow_dropped` +1 且 **`outstanding` 仍 = CAP（未 +1）**；
     `Closed`/`Broadcast` 永不丢（各发一条对照）。
4. **`zonek_columns_aligned_and_derived`**（列不混比 + 派生恒等式）
   - writer 温度同 `perf_tsv_header_matches_values_with_new_column`：bump 区 0 三列与 5 个 `zonek_*`；
     写一行；再写一行 **N=1 形态**（只 bump 区 0）。
   - 断言：表头/值列数一致；5 个 `zonek_*` 落在同名列；5 个派生列 == 区 0 + zonek；
     N=1 行 `zonek_*`==0 且派生 == 区 0；**旧列值未被污染**。
5. **`zonek_wiring_behavior_primary`**（接线：行为为主、源码为提示）
   - **行为断言（主牙）**：驱动交付 seam（`fill_pending` + 段执行）后 **`zonek_inbound_drained` 必须增长**；
     驱动为空 ⇒ 不增长（对照）。若有人把循环改回裸 `try_recv`（不经 seam），计数恒 0 ⇒ 本牙红。
   - **源码窗口扫描（辅助，qa-charter §12：接线提示，非行为证据）**：`spawn_minimal_zone` 的
     `begin_tick..zone_tick_top` 窗口内 `fill_pending(` 恰一次、且无裸 `ev_rx.try_recv()`。
   - **变异证据（真正证明"计数跟着真实排水走"）**：删预算分支 ⇒ 测试 1 的 `filled<=pull_cap` 红；
     恢复后文件 sha 相等。

## 6. 风险、护栏与回滚

- **面**：只动"区 k 线程循环填充段 + `INBOUND_LOCAL` 段执行 + demux 转发 + perf 计数/TSV + 测试"。
  **N=1（无区 k）零参与**；不新增 cargo feature。
- **段执行的锁与屏障（两条传导，写实）**：
  1. 执行在**锁 1（A 段）内** ⇒ 段时长**直接加到该区到达 barrier1 的时刻**；其余区在 barrier1
     等最慢区 ⇒ 「某区段执行慢 ⇒ 全体 tick 被拖」。
  2. 段持 `state` 锁更久 ⇒ 与区 0 form(d)（`finish_login_on_zone_state` / `handle_zone_closed` 持区 k 锁）
     的**双向等待窗**随段时长增大。
  - 守门：`execute_wall = 10ms`（每 `execute_check_every` 条查、**一整条执行完后**收手，剩余留缓冲
    下一 tick）；**锁内/段内执行墙钟 p99.9 > 10ms ⇒ 下调 `execute_max_events`**（用 p99.9 不用均值，
    尾部才是持锁上界）。
  - **越界项（写进守门注释）**：每 `execute_check_every`(=16) 条才查一次 ⇒ 段内/持锁上界 ≈
    **`execute_wall + (execute_check_every − 1) × 单条 p99.9`**（= 10ms + 15×单条）。当前命令 µs 级，
    远低于 `10ms / 15 ≈ 0.67ms` ⇒ 安全；**单条若逼近 0.67ms ⇒ 下调 `execute_check_every` 或
    `execute_max_events`**（同一扩展门）。
- **p99.9 校准（不伪造实测）**：`execute_max_events <= floor(execute_wall / p99.9_per_command)`；
  **当前没有 LOCAL 命令集的 p99.9 实测** ⇒ **先取 `execute_max_events = 2000`，由 `execute_wall`
  直接兜底**；首次有 LOCAL 命令集实测后再按公式校准并在常量旁写数值来源。
- **扩展门**：LOCAL 命令集一旦纳入**任何可阻塞 handler**（DB 往返 / 文件 / 大 fan-out `say`），
  必须**复评**（下调 `execute_max_events` 或改为更细的时间切片）。
- **回滚**：单提交 revert；可选 `MIR2_ZONEK_DRAIN_BUDGET=0` 临时绕过（默认不启用；该窗口不可作验收）。
- **验收自查（必须满足）**：窗口设 `MIR2_ZONES >= 2`、**不设** `MIR2_SHARDS > 1`（互斥且 shard 拓扑
  不产生 `zonek_*` 证据）；`zonek_*` 仅在 `MIR2_ZONE_SLICING` 下存在。
- **不合并/回滚判据**：§5 五条不全绿；或任一验收窗口 `zonek_inbound_overflow_dropped != 0`
  （v2.1：该窗口作废并开缺陷卡）。
- **A6 的 tick 口径（r3 重申）**：`at_tick` 置位与超时判定**一律用区 k 线程循环的本地 `tick`**；
  `self.ticks` 只给段内既有 ⑤；「两位置同一 tick 号」**仅在 `zone_tick_top` 同侧成立**；
  超时一律 `>= 2`（T 暂缓、T+1 不触发、T+2 触发）——plan §5.3 的 `> 2` 已同步改为 `>= 2`。

## 7. 交付时报告什么

- 改动文件与 `+/-` 行数；常量值（`ZONE_INBOUND_MAX_EVENTS`、`tick_max_events/wall`、
  `pull_cap_events`、`execute_max_events`、`execute_wall`、`execute_check_every`）与取值理由
  （p99.9 未实测 ⇒ 先 2000 + `execute_wall` 兜底，见 §6）；
- 覆盖范围声明：仅 zone-slicing（§1.5），`shard_total>1` 不覆盖；
- 五条测试的原文输出（各自一行打印）+ 变异证据（删预算分支 ⇒ 测试 1 红；恢复后 sha 相等）；
- TSV 表头新增列清单与一行样例值（含 5 个派生列恒等式）；
- N=1 零参与证据（区 k 不存在时 `zonek_*` 全 0，派生列 == 区 0 列）；
- shutdown 残留路径证据（不静默：WRN + `zonek_inbound_pending_discarded`）。
