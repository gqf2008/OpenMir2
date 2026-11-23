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
>
> **r5（2026-11-23，99 预读 5 歧义，opc-27 裁决）**：P1 swap 回点=承载段的那次 `tick_phase_*`
> 返回后立即、`carry_prefix` 清零、tick 间缓冲必在 scratch（`debug_assert`）；P2 新增第 6 列
> `zonek_inbound_unsupported`（+派生），**归属计数**（命中即计、不丢——见 r6 R-a）；P3 shutdown 落在区 k 线程尾（先校正
> P1 不变式、逐条 -1 计数、通道残留只按 demux 对账）；P4 `pending_local` 与缓冲共享
> `execute_wall/execute_max_effective`；P5 单 tick 总条数 ≤2000（缓冲非空 ⇒ fill 减半、执行=2000−filled）。
> **r6（2026-11-23，父 `2aed4247b`，opc-27 三条裁决 + 一条口径修正）**：
> **R-a** 第 3 步**处置逻辑不变**——每条 `Command` 照旧走 `handle_zone_command`（`zone_cmd_class`
> 只决定计数归属、不决定处置）；全部计入 `zonek_inbound_drained`，`Unsupported` **另计**
> `zonek_inbound_unsupported`（归属，**不是丢弃**）；真丢弃留第 5 步；第 4 步接管 `Process`。
> **R-b** `TickIo` 增 `inbound_outstanding: Option<Arc<AtomicUsize>>`（执行器 -1；区 0 / N=1 = `None`）。
> **R-c** 区 k A 段相位集 = `M1B_ZONE_PROD = M1A_PROD | INBOUND_LOCAL`（**`tick_phase_rest` 不动**）；
> 两区四跑为必跑验收（a7 执行）。**口径**：`zonek_inbound_drained` 与区 0 的**命令分量**
> （`drained_command`）同分母；派生列 `inbound_drained_all_zones` 随之重定义；**删除
> `inbound_carry_all_zones`**（区 0 carry 无命令分量 ⇒ 混合分母、不可合计）。

## 0. 改动清单（一句话版）

- `lib.rs`：①区 k 线程循环（`spawn_minimal_zone`）把**两段裸 `try_recv`** 换成**有预算的填充**
  （锁外）→ 命令进区线程局部 scratch、经 `TickIo` 交接（`inbound_local_buffer` /
  `inbound_local_carry_prefix` / `inbound_outstanding`，见 r6 R-b）；②`INBOUND_LOCAL` **段内**消费 +
  **照旧执行**（既有 `handle_zone_command`；**本步零处置变更**：分类只决定计数归属，见 r6 R-a）；
  ③`spawn_zone_demux` 增加**每区 `outstanding` 计数 + `Command` 硬上限丢新**；
  ④`perf_counters` 增 **6 个 `zonek_*` 列 + 5 个 `*_all_zones` 派生列**（区 0 三列语义不动；第 6 列
  `zonek_inbound_unsupported` = **归属计数**、第 3 步不丢，见 r6 R-a；派生列删 `inbound_carry_all_zones`，
  见 §4 口径）。
- ⑤区 k 的 A 段相位集 `M1A_PROD` → **`M1B_ZONE_PROD = M1A_PROD | INBOUND_LOCAL`**（**只此一处**；
  `tick_phase_rest` 的 `M1A_PROD` **不动**——两处都含 ⇒ 同 tick 双跑，见 r6 R-c 与 §6 风险）。
- 不新增 cargo feature；多区路径才有区 k ⇒ **N=1 零参与**（`zonek_*` 恒 0；派生列口径见 §4：`inbound_drained_all_zones` == 区 0 `drained_command`，其余派生 0）。
- **覆盖范围 = 仅 zone-slicing 拓扑（`MIR2_ZONES`）；`shard_total>1` 的分片拓扑不覆盖**（见 §1.5）。

## 1. `outstanding[z]` 的真实可行性（mpsc 没有长度，怎么数）

### 1.1 类型、所有权与创建点

- 类型：`type ZoneOutstanding = Vec<Arc<std::sync::atomic::AtomicUsize>>`，长度 = `zone_count`。
  **下标 z 与区号一致；z=0 恒不使用**（区 0 不设上限、不丢）。
- 创建点：多区规划处（与 `senders`/`receivers` 的 `for _ in 0..zone_count { let (tx,rx)=std::sync::mpsc::channel(); … }`
  同一作用域）——`let outstanding: ZoneOutstanding = (0..zone_count).map(|_| Arc::new(AtomicUsize::new(0))).collect();`
- 分两路 `Arc` 克隆：
  - `spawn_zone_demux(..., outstanding: ZoneOutstanding)`（demux 只读/加）；
  - 每区线程 `spawn_minimal_zone(..., outstanding: Arc<AtomicUsize>, zone_index: usize)`——**只为构造**
    `TickIo.inbound_outstanding = Some(Arc::clone(&outstanding))`（r6 R-b：`TickIo` 在区线程内
    **每线程一次**、`while !stop` 之外构造，见 `let mut zone_io = TickIo { … };`）；**不新增第二个
    线程局部变量**（`-1` 的唯一来源是 `io.inbound_outstanding`）。
- 区 0（`sim_loop` 的 `let mut tick_io = TickIo { … };`，lib.rs:32886/:34566）与 7 处测试构造一律
  `inbound_outstanding: None` ⇒ 区 0 / N=1 零参与。**全仓 9 处 `TickIo {` 字面量**
  （`grep -c "TickIo {" mir2-rs/bins/game-svr/src/lib.rs` = 10，减 1 处结构体定义）。
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

**-1 = 区 k 从工作区 `pop_front` 取出、执行体之前**（**消费时**，不在 fill、不在执行后）：

```text
// INBOUND_LOCAL 段内，逐条执行前（r6 R-b：经 io 字段，不经线程局部）：
if let Some(ev) = buffer.items.pop_front() {
    if let GateEvent::Command { .. } = &ev {
        if let Some(o) = io.inbound_outstanding.as_ref() {  // 区 0 / N=1 = None ⇒ 不减
            debug_assert!(o.load(Relaxed) > 0);
            o.fetch_sub(1, Relaxed);                         // 取出即 -1
        }
    }
    execute(ev);   // 执行体 = 既有 handle_zone_command（处置不变，见 r6 R-a）
}
```

- **不变量（写死）**：
  1. **每条 `Command` 恰 `+1` 一次**——在 demux `send` 成功之后；
  2. **恰 `-1` 一次**——在从 `pending_inbound` 取出执行时；**丢新路径不 +1**；
  3. **shutdown 时**：区 k 线程 `while !stop` 退出**之后、线程尾**——先按 P1 不变式确认缓冲在 scratch
     （若 `zone_io.inbound_local_buffer` 仍非空 ⇒ 先 swap 回 scratch 并打一条 WRN 记异常），再对该
     scratch `pop_front` 逐条 `-1` 并把 `Command` 计入 `zonek_inbound_pending_discarded`，最后打一条
     含总数的 WRN；**通道里尚未 fill 的残留不读、只按 demux 的 `outstanding` 计数对账**（WRN 含该值），
     不视为错误（进程整体退出，`Arc` 释放）。这里的逐条 `-1` 同样经 `zone_io.inbound_outstanding`
     （`zone_io` 在 `while` 之外、线程尾仍在作用域；不新增线程参数）。

### 1.3 竞态、下界与泄漏面

- 内存序一律 `Relaxed`：**软上限**，只用于"避免无界"，不承担同步。
- **`outstanding` = 通道 + `pending_inbound` = 真实未处理积压**（-1 在消费时）⇒ cap 守的正是该守的；
  fill 快于执行 ⇒ pending 增长会进 `outstanding` ⇒ demux 到 CAP 停投 ⇒ **真·有界**。
- **低估 ≤ 1**：执行中那一条已 -1、未算；相对 CAP=10,000 可忽略（写进常量注释）。
- **不会下溢**：`fetch_sub` 只对"曾 +1 过的 `Command`"发生（消费或 shutdown 丢弃）。debug 构建在 `io.inbound_outstanding` 的 `fetch_sub` 前断言 `>0`（r6 R-b）。
- **pop 后执行体 panic**：该条已 -1 未执行；但 panic = 区死 = `catch_unwind` → 整机 abort，**良性**。
- 其它事件（`Closed/Opened/注册`）不设上限：本切片只保护"每玩家命令"这条最可能被负载打爆的路径。

### 1.4 上限常量与观测

- `const ZONE_INBOUND_MAX_EVENTS: usize = 10_000;`（= 2000 条/tick × 5 拍）。注释写死：
  "软上限；正确性不依赖精确值；按窗口实测的区间到达率调整；**低估 ≤1**（消费时 -1）"。
- 丢弃观测（**丢弃必须有计数、不许静默**）：`note_zonek_inbound_overflow_dropped()`（丢新）+
  `note_zonek_inbound_pending_discarded()`（shutdown 残留丢弃），各自**限速 WRN**（复用
  `DepartWarnLimiter` 同型：每分钟 ≤1 条，字段含 zone/line/socket/ident）。
- **归属观测（r6 R-a，不是丢弃）**：`note_zonek_inbound_unsupported()`——`zone_cmd_class` 结果为
  `Unsupported` 的命令**仍交 `handle_zone_command`**（该函数内部对非移动命令照旧丢弃并计
  `zone_cmd_unsupported`——**既有行为、本步不动**），本列只记**分类归属**；`zonek_inbound_drained`
  **照计**（双计、非互斥）。⇒ **分类器的 `Unsupported` ≠ handler 的「非移动」集合**（分类为 `Local`
  的 hit/spell/query… 同样被 handler 丢），两列**不得互替、不得对账**。不另开 WRN（handler 侧既有
  按 ident 采样日志，lib.rs:24984–24999）。
- **生产应恒 0 的只有 `overflow_dropped` / `pending_discarded`**；`overflow_dropped` 非 0 ⇒ 该窗口
  验收作废（v2.1 §3.5/§4 判据，测试见 §5-③）。`unsupported` 是观测量，**可非 0、不作废**。

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

**r3 形态（执行落点按 plan §5.3）**；**r6 追加（R-c）**：区 k 的 A 段相位参数 `M1A_PROD` →
`M1B_ZONE_PROD = M1A_PROD | INBOUND_LOCAL`，`tick_phase_rest` 的参数**不动**：

1. **循环顶（锁外）只做填充**：把两段裸 `try_recv` 换成 `fill_pending(...)`：按预算从两通道
   `try_recv` **命令**，append 进**区线程循环的局部 scratch** `pending_inbound: VecDeque<GateEvent>`
   （声明在 `while !stop` 之外，与 `gates`/`tick` 同级；**fill 全程不持 `state` 锁**，与今天
   "锁外 `try_recv` 到局部"的锁形态一致）；`carry_prefix` 就在这块 scratch 上算（§3.3）。
   `Closed/Register/Unregister` 仍在循环顶就地处理（归集 `closed_events` / 更新 `gates`），**不动**。
2. **`INBOUND_LOCAL` 段内执行（锁 1 下）**：交接**走 `TickIo` 新增的三个字段**（§3.3 已选这一种并写死，
   不用段函数入参）——driver 在**锁外**（`TickIo` 是 driver 局部，不持 `state` 锁）把 scratch
   `mem::swap` 进 `zone_io.inbound_local_buffer` 并写入 `zone_io.inbound_local_carry_prefix`；
   随后调 `tick_phase_a` 跑段。**swap 回点（P1 写死）：承载该段的那次 `tick_phase_*` 调用一返回
   就立刻 swap 回 scratch（无论返回 `Continue`/`Exited`），并把 `inbound_local_carry_prefix` 清零。**
   段现在在 `tick_phase_a`（该处相位参数本步改为 `M1B_ZONE_PROD`，§0-⑤）；**若将来把它移到 rest 段，
   swap 回点随段函数一起移动**（不得留在旧位）。
   不变式：**tick 之间缓冲一定在 scratch**，`TickIo.inbound_local_buffer` 在 tick 之外应为空
   （可加 `debug_assert!`）。段内**先消费 A6 的 `pending_local`（延迟队列）→ 再消费
   `io.inbound_local_buffer`**，逐条分类（**只写计数归属**）/ 执行（**既有 `handle_zone_command`**；`Process` 的接管在第 4 步 A6）/
   记账（`zonek_*`、`RAN_SEGMENTS |= INBOUND_LOCAL`；
   `HostInbound` 计时天然覆盖）。
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
    tick_max_events: u64,       // 2000（PRODUCTION）：**单 tick「fill + 执行」总条数上限**
    tick_max_wall: Duration,    // PRODUCTION 的 50ms：**只作 `execute_wall` 的来源常量**，
                                //   不表示"fill + 执行跨阶段累加"（见计时分工）
    pull_cap_base: u64,         // = tick_max_events（fill 条数上限的基准；测试可设小值）
    execute_max_events: u64,    // = tick_max_events（**上限参数**，实际值见公式；测试可注入）
    execute_wall: Duration,     // 10ms（**段内执行**的墙钟上界；加到 A 段/屏障1 到达，见 §6）
    execute_check_every: usize, // 16（每 N 条查一次 execute_wall，末条必查）
}
```

- **单 tick 总预算（写死，消除"各自 2000"的矛盾）**：
  - `pull_cap_effective = if 本 tick 开始时缓冲非空 { pull_cap_base / 2 } else { pull_cap_base }`
    ⇒ **缓冲非空时 fill 上限减半**，保证执行至少拿到一半预算；
  - `execute_max_effective = min(execute_max_events, tick_max_events − 本 tick 实际 filled)`
    （saturating 减）；
  - ⇒ **单 tick 总条数恒 ≤ `tick_max_events`（2000），与区 0 的 `PRODUCTION` 口径一致**；因此
    `zonek_inbound_drained` 与区 0 的**命令分量** `drained_command` 同分母（与
    `inbound_drained`（全事件）**不可比**，见 §4 口径）。**若将来改动本规则，必须同时改这句话与 TSV 口径说明。**
- **fill**（循环顶、锁外）：每调用最多 pull `pull_cap_effective` 条；**预算耗尽后探一次**：
  `try_recv Ok` ⇒ **直接 push 进 `pending_inbound`（scratch）** 且 `fill_hit=true`；`Err(Empty)` ⇒ `fill_hit=false`。
  （**探针不另设 carry**——它进的就是同一缓冲。）
- **段执行**（锁 1 内）：从 `pending_local` 起、再 `io.inbound_local_buffer`；**两者共享同一个
  `execute_max_effective` 与 `execute_wall`**（都在锁内执行，理应同预算；`pending_local` 先消费、
  不会被缓冲饿死，它自己的条数上限沿用 A6 的 8）。每 `execute_check_every` 条（及工作区末条）查
  `execute_wall`，超界 ⇒ **在一整条执行完之后**收手，剩余留在工作区（保序）；`execute_max_effective`
  到顶同理。**收手后工作区非空 ⇒ `budget_hit=true`（不做探针）**。这里的「执行」= **既有
  `handle_zone_command`**（r6 R-a：分类不改变处置；`Process` 的接管在第 4 步）。
- **`budget_hit` 两处判据（写死）**：`fill_hit`（fill 探针取到事件并留在缓冲）**或** 段执行截断且工作区非空。
- **计时分工（写死）**：**执行以 `execute_wall` 为准**（取值 = `tick_max_wall` 的一部分，10ms）；
  `tick_max_wall` **不参与任何跨阶段累加**（fill 与段执行是分离阶段）；fill 侧唯一硬上限是
  `pull_cap_effective`（条数）。

### 3.3 单缓冲、carry 水位与保序

- **缓冲归属（钉死，二选一已定：走 `TickIo`，不用段函数入参）**：
  - **跨 tick 缓冲 = 区 k 线程循环的局部 scratch** `pending_inbound: VecDeque<GateEvent>`
    （声明在 `while !stop` 之外，与 `gates`/`tick` 同级）；**fill 锁外 append**、`carry_prefix`
    在这块 scratch 上算。
  - `TickIo` 新增 **`inbound_local_buffer: VecDeque<GateEvent>` + `inbound_local_carry_prefix: usize`
    + `inbound_outstanding: Option<Arc<AtomicUsize>>`**（前两个 = 本 tick 的输入队列工作区；第三个 =
    r6 R-b 的 -1 句柄，区 0 / N=1 = `None`）。driver 在**锁外**（`TickIo` 是 driver 局部，不持
    `state` 锁）`mem::swap` 进、tick 结束后 swap 回；`carry_prefix` 同步写入字段；**`inbound_outstanding`
    不参与 swap（只读）**。**N=1 时缓冲区常空、`inbound_outstanding == None`、零参与。**
  - **为什么不放 `HostLoop`**：`HostLoop` 字段 = zone-owned 状态，改它必须持锁；而缓冲是
    "**本 tick 的输入队列**"，本质属 `TickIo` 一类 ⇒ 放循环局部/`TickIo` 既不破 `HostLoop` 不变量，
    也不需要锁外改 `HostLoop`（也不增加 `state` 锁次数）。
- `carry_prefix` 在**每次 fill 追加之前**重算：`carry_prefix = pending_inbound.len()`（= 上 tick 遗留
  条数），随 swap 写进 `inbound_local_carry_prefix`；段内按**工作区下标**：`idx < carry_prefix` ⇒
  `from_carry`（`zonek_inbound_carry += 1`），否则普通 drained。**每条恰计一次**（执行才 `drained++`；
  未执行剩余不计，下一 tick 按新 `carry_prefix` 计 from_carry）。`pending_local`（A6 延迟队列）
  **不计入 carry**、走自己的计数。
- **执行循环不写 `clear()`**（避免抹掉剩余）：段内写成
  `while let Some(ev) = io.inbound_local_buffer.pop_front() { 取出即经 io.inbound_outstanding -1；
  执行整条（既有 handle_zone_command）；每 execute_check_every 条（及末条）查 execute_wall，超界即停
  （不再 pop 下一条） }` ⇒ 未执行的自然留在工作区，tick 后
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
- **第 3 步段内零丢弃（r6 R-a）**：`Unsupported` 也交 `handle_zone_command`（其内部照旧丢非移动命令）；
  真正按类丢弃留第 5 步。

## 4. 计数与 TSV 列（区 0 三列一字不动）

- `perf_counters` 新增（区 k 聚合，全部区 k 实例求和）：
  `ZONEK_INBOUND_DRAINED / ZONEK_INBOUND_CARRY / ZONEK_INBOUND_BUDGET_HIT /
  ZONEK_INBOUND_OVERFLOW_DROPPED / ZONEK_INBOUND_PENDING_DISCARDED / ZONEK_INBOUND_UNSUPPORTED: AtomicU64`
  （第 6 列 **`_UNSUPPORTED` = 分类归属计数**；第 5 步接管丢弃时**另加** `ZONEK_INBOUND_UNSUPPORTED_DROPPED`，
  **不改本列**——r6 裁决 1。）
- note 函数（同形于 `note_inbound_drained`；**只写 `zonek_*`，禁止调 `note_inbound_drained`**）：
  `note_zonek_inbound_drained(from_carry, ev)`、`note_zonek_inbound_budget_hit()`、
  `note_zonek_inbound_overflow_dropped()`、`note_zonek_inbound_pending_discarded()`、
  `note_zonek_inbound_unsupported()`；`#[cfg(test)]` 读点各一。
- TSV：`write_tick` 表头与 values **同位置**追加 6 列（紧跟 `depart_frames_mirror_suppressed` 之后）；
  另加 **5 个派生列**（写行时显式求和、不独立采集、列文注 derived、不可与单区列混比）：
  `inbound_drained_all_zones / inbound_budget_hit_all_zones / inbound_overflow_dropped_all_zones /
  inbound_pending_discarded_all_zones / inbound_unsupported_all_zones`
  = 区 0 对应列 + Σ `zonek_*`（区 0 无同名时按 0 起算），**唯一例外见下条 `drained` 口径**。
- **口径（r6，硬约束）**：
  - `inbound_drained_all_zones` = 区 0 的**命令分量** `DRAINED_COMMAND`（TSV 列 `drained_command`）
    + Σ `zonek_inbound_drained`，两者同分母（命令条数）。**列文写死：`inbound_drained`（全事件）与
    本派生列不同分母，禁止并列比较或算比率。** 锚：`INBOUND_DRAINED`/`DRAINED_COMMAND` 定义 lib.rs:24340/24349；
    `note_inbound_drained` 体内 `DRAINED_COMMAND.fetch_add` lib.rs:24740（`INBOUND_DRAINED` 在 :24734、
    不分变体）；表头列 `drained_command` lib.rs:24776；values 行 `DRAINED_COMMAND.swap` lib.rs:24828。
  - `inbound_budget_hit_all_zones`：区 0 `INBOUND_BUDGET_HIT` 每次 drain 命中记一次（lib.rs:36602–36603
    `if drain.hit_budget { bump }`），单位 =「本 tick 是否被截断」；区 k 同为每 tick 一次 ⇒ **同单位、可合计**。
  - **不派生 `inbound_carry_all_zones`（删除）**：区 0 的 `INBOUND_CARRY` 只在 `note_inbound_drained` 内
    按事件自增（lib.rs:24736，**无命令分量**），区 k carry 只数命令 ⇒ 相加是混合分母、没有可比的分母。
    若将来要 carry 合计，先在区 0 加命令分量列（列口径变更须另卡）。
  - `inbound_unsupported_all_zones` / `inbound_overflow_dropped_all_zones` /
    `inbound_pending_discarded_all_zones`：区 0 无对应列 ⇒ 等于 Σ `zonek_*`（命令单位）。
- 列文写死：`zonek_*` = "仅区 k 聚合（slicing）"；`*_all_zones` = derived。旧列语义不变
  （新旧两套并存、互不覆盖）。N=1：`zonek_*` 恒 0；`inbound_drained_all_zones` == 区 0
  `drained_command`（**不是** `inbound_drained`），其余派生列 0。

## 5. 七条放行测试（确定性；不改产品语义）

测试全部作用于 **`fill_pending` / 段执行函数 / demux 转发** 三个可测单元 + **相位集常量**
（不驱动整条区线程）。

1. **`zonek_fill_honors_budget`**（fill 预算；墙钟只打印）
   - 构造：真 `mpsc::channel()` 注入 2500 条 `Command`（+ 少量 `Closed`）；`limits` 注入生产值，
     `execute_max_events` 设大（让执行不是瓶颈）；先 `fill_pending` 再执行。
   - **硬断言**：`filled <= pull_cap_effective`、`drained <= execute_max_effective`、
     **`filled + drained <= tick_max_events`（单 tick 总条数 ≤2000）**、`budget_hit == true`、
     **`zonek_inbound_drained` 增量 == drained**、**`outstanding == 通道残留 + 缓冲残留`**；
     （r6 R-b）seam 夹具须构造 `TickIo.inbound_outstanding = Some(Arc::new(AtomicUsize::new(0)))`；
     另跑一条 `None` 对照（执行照跑、计数照增、`outstanding` 不减、不 panic）；
     墙钟 **只打印**（`elapsed_ms=…`）不判红。
   - **半额规则子断言**：hold 一笔非空缓冲再跑一次 fill ⇒ `filled <= pull_cap_base / 2`（P5）。
2. **`zonek_pending_remainder_and_carry_prefix`**（剩余保序 + 水位；limits **按调用**注入）
   - 注入 A..H；call1：`pull_cap_base=8`、`execute_max_events=2`、`execute_wall=极大`、
     `execute_check_every=1` ⇒ 执行 2、剩 6 留缓冲、`budget_hit=true`、`from_carry=0`；
     call2：**`execute_max_events=6`**（其余同）⇒ 执行剩余 6、**`from_carry=6`**、`budget_hit=false`。
   - 断言：两次合计顺序 A..H（不丢/不重/不乱序）；**call2 后 `outstanding == 0`**（通道空 + 缓冲空
     ⇒ 每条 +1/-1 归零）；注释写明"limits 是**每调用**参数，不是跨调用常量"。
3. **`zonek_overflow_drops_new_and_counts`**（硬安全阀）
   - `outstanding = CAP-1`，转发 2 条 `Command`：第 1 条 `send().is_ok()` ⇒ +1（=CAP）；
     第 2 条被丢（`continue`）⇒ `zonek_inbound_overflow_dropped` +1 且 **`outstanding` 仍 = CAP（未 +1）**；
     `Closed`/`Broadcast` 永不丢（各发一条对照）。
4. **`zonek_columns_aligned_and_derived`**（列不混比 + 派生恒等式）
   - writer 温度同 `perf_tsv_header_matches_values_with_new_column`：bump 区 0 三列 + `DRAINED_COMMAND`
     与 6 个 `zonek_*`；写一行；再写一行 **N=1 形态**（只 bump 区 0 三列 + `DRAINED_COMMAND`）。
   - 断言：表头/值列数一致；6 个 `zonek_*` 落在同名列；**5 个**派生列：`inbound_drained_all_zones`
     == 区 0 `drained_command` + Σ zonek drained（**≠** `inbound_drained`——同一行断言两者此刻不相等，
     证口径分离），其余 4 个 == zonek（区 0 无同名）；N=1 行 `zonek_*`==0 且 `inbound_drained_all_zones`
     == 区 0 `drained_command`、其余派生 0；**旧列值未被污染**；**不存在** `inbound_carry_all_zones` 列。
5. **`zonek_wiring_behavior_primary`**（接线：行为为主、源码为提示）
   - **行为断言（主牙）**：驱动交付 seam（`fill_pending` + 段执行）后 **`zonek_inbound_drained` 必须增长**；
     驱动为空 ⇒ 不增长（对照）。若有人把循环改回裸 `try_recv`（不经 seam），计数恒 0 ⇒ 本牙红。
   - **源码窗口扫描（辅助，qa-charter §12：接线提示，非行为证据）**：`spawn_minimal_zone` 的
     `begin_tick..zone_tick_top` 窗口内 `fill_pending(` 恰一次、且无裸 `ev_rx.try_recv()`；另扫
     `tick_phase_a(` 的相位参数含 `INBOUND_LOCAL`、`tick_phase_rest(` 的参数**不含**（r6 R-c：两处都含
     ⇒ 同 tick 双跑）。
   - **变异证据（真正证明"计数跟着真实排水走"）**：删预算分支 ⇒ 测试 1 的 `filled<=pull_cap` 红；
     恢复后文件 sha 相等。
6. **`zonek_unsupported_attributed_but_executed`**（r6 R-a：分类不改变处置）
   - 构造：经 seam 注入 1 条**分类为 `Unsupported` 的 ident**（取 M1b 命令集外的确定值）；持
     `perf_counters::TEST_LOCK` 做精确 delta。
   - 断言：①`zonek_inbound_drained` +1；②`zonek_inbound_unsupported` +1；③**handler 侧
     `zone_cmd_unsupported` +1**（证明真调了 `handle_zone_command`——它对非移动命令的既有副作用，
     lib.rs:1429–1441）。
   - 非恒真（变异）：把 seam 对 `Unsupported` 改成 `continue`（提前丢）⇒ ③增量为 0 ⇒ 红。
   - **退化条款**：若 ③ 给不出精确锁（`zone_cmd_unsupported` 被并行用例 bump），退化为 `> before`，
     **必须在测试注释写明退化理由**（引 lib.rs:123766–123781 的先例）。
7. **`zonek_zone_phase_set_includes_inbound_local`**（r6 R-c：常量级接线）
   - 断言 `M1B_ZONE_PROD.0 & PhaseSet::INBOUND_LOCAL != 0`；**且** `M1A_PROD.0 & PhaseSet::INBOUND_LOCAL == 0`
     （基线被误改也红——防止有人图省事直接改 `M1A_PROD`）。
   - 源码扫描（辅助，qa-charter §12）：`spawn_minimal_zone` 的 A 段 `tick_phase_a(` 相位参数含
     `INBOUND_LOCAL`、`tick_phase_rest(` 的参数**不含**（含 ⇒ 同 tick 双跑 ⇒ 命令双执行）。

## 6. 风险、护栏与回滚

- **面**：只动"区 k 线程循环填充段 + `INBOUND_LOCAL` 段执行 + demux 转发 + perf 计数/TSV + 测试 + 区 k A 段相位集（`M1B_ZONE_PROD`）+ `TickIo` 三字段"。
  **N=1（无区 k）零参与**（区 0 的 `TickIo.inbound_outstanding == None`，测试断言）；不新增 cargo feature。
- **段执行的锁与屏障（两条传导，写实）**：
  1. 执行在**锁 1（A 段）内** ⇒ 段时长**直接加到该区到达 barrier1 的时刻**；其余区在 barrier1
     等最慢区 ⇒ 「某区段执行慢 ⇒ 全体 tick 被拖」。
  2. 段持 `state` 锁更久 ⇒ 与区 0 form(d)（`finish_login_on_zone_state` / `handle_zone_closed` 持区 k 锁）
     的**双向等待窗**随段时长增大。
  - 守门：`execute_wall = 10ms`（每 `execute_check_every` 条查、**一整条执行完后**收手，剩余留缓冲
    下一 tick）；**锁内/段内执行墙钟 p99.9 > 10ms ⇒ 下调 `execute_max_events`**（用 p99.9 不用均值，
    尾部才是持锁上界）。**该上界对"整段"成立**：`pending_local`（A6 延迟队列）与缓冲**共享同一个
    `execute_wall`/`execute_max_effective`**（P4），不是只有缓冲受约束。
  - **越界项（写进守门注释）**：每 `execute_check_every`(=16) 条才查一次 ⇒ 段内/持锁上界 ≈
    **`execute_wall + (execute_check_every − 1) × 单条 p99.9`**（= 10ms + 15×单条）。当前命令 µs 级，
    远低于 `10ms / 15 ≈ 0.67ms` ⇒ 安全；**单条若逼近 0.67ms ⇒ 下调 `execute_check_every` 或
    `execute_max_events`**（同一扩展门）。
- **R-c 双跑（硬风险，写死）**：区 k 的 A 段相位集是 `M1B_ZONE_PROD = M1A_PROD | INBOUND_LOCAL`；
  `tick_phase_rest` 仍是 `M1A_PROD`。**若两者都含 `INBOUND_LOCAL` ⇒ 同 tick 段双跑 ⇒ 命令双执行**、
  `zonek_inbound_drained`/`RAN_SEGMENTS` 双记 ⇒ 验收读数作废。牙见 §5-7。
- **p99.9 校准（不伪造实测）**：`execute_max_events <= floor(execute_wall / p99.9_per_command)`；
  **当前没有 LOCAL 命令集的 p99.9 实测** ⇒ **先取 `execute_max_events = 2000`，由 `execute_wall`
  直接兜底**；首次有 LOCAL 命令集实测后再按公式校准并在常量旁写数值来源。
- **扩展门**：LOCAL 命令集一旦纳入**任何可阻塞 handler**（DB 往返 / 文件 / 大 fan-out `say`），
  必须**复评**（下调 `execute_max_events` 或改为更细的时间切片）。
- **回滚**：单提交 revert；可选 `MIR2_ZONEK_DRAIN_BUDGET=0` 临时绕过（默认不启用；该窗口不可作验收）。绕过时**必须**同时把区 k A 段相位集回退 `M1A_PROD`（否则段无预算运行）。
- **验收自查（必须满足）**：窗口设 `MIR2_ZONES >= 2`、**不设** `MIR2_SHARDS > 1`（互斥且 shard 拓扑
  不产生 `zonek_*` 证据）；`zonek_*` 仅在 `MIR2_ZONE_SLICING` 下存在。
- **两区四跑必跑（a7 执行，r6 R-c）**：本步起区 k **首次真正执行** `INBOUND_LOCAL`，N=1 单跑只证零
  参与（`zonek_*` 恒 0）、不产生本步行为证据；四跑清单以 a7 派工为准，本规格只定「必须」。
- **不合并/回滚判据**：§5 七条不全绿；或任一验收窗口 `zonek_inbound_overflow_dropped != 0`
  （v2.1：该窗口作废并开缺陷卡）；`zonek_inbound_unsupported` 非 0 **不作废**（归属计数）。
- **A6 的 tick 口径（r3 重申）**：`at_tick` 置位与超时判定**一律用区 k 线程循环的本地 `tick`**；
  `self.ticks` 只给段内既有 ⑤；「两位置同一 tick 号」**仅在 `zone_tick_top` 同侧成立**；
  超时一律 `>= 2`（T 暂缓、T+1 不触发、T+2 触发）——plan §5.3 的 `> 2` 已同步改为 `>= 2`。

## 7. 交付时报告什么

- 改动文件与 `+/-` 行数；常量值（`ZONE_INBOUND_MAX_EVENTS`、`M1B_ZONE_PROD`、`tick_max_events/wall`、
  `pull_cap_base`、`execute_max_events`、`execute_wall`、`execute_check_every`，以及 P5 的两条
  有效值公式）与取值理由
  （p99.9 未实测 ⇒ 先 2000 + `execute_wall` 兜底，见 §6）；
- 覆盖范围声明：仅 zone-slicing（§1.5），`shard_total>1` 不覆盖；
- 七条测试的原文输出（各自一行打印）+ 变异证据（删预算分支 ⇒ 测试 1 红；Unsupported 提前丢 ⇒
  测试 6 红；相位集漏 `INBOUND_LOCAL` ⇒ 测试 7 红；恢复后 sha 相等）；
- TSV 表头新增列清单与一行样例值（含 5 个派生列恒等式，并给出 `inbound_drained_all_zones` == 区 0 `drained_command` 的证据）；
- N=1 零参与证据（区 k 不存在时 `zonek_*` 全 0、`inbound_drained_all_zones` == 区 0
  `drained_command`、其余派生 0；区 0 `TickIo.inbound_outstanding == None`）；
- shutdown 残留路径证据（不静默：WRN + `zonek_inbound_pending_discarded`）；
- `grep -c "TickIo {" mir2-rs/bins/game-svr/src/lib.rs` = 10（含 1 处结构体定义）⇒ 9 处字面量；
  逐处落点（生产 2 处 + 测试 7 处）列出；
- 口径证据：`inbound_drained_all_zones` == 区 0 `drained_command` + Σ `zonek_inbound_drained`；
  TSV **不存在** `inbound_carry_all_zones` 列；
- 两区四跑结果（a7）：`MIR2_ZONES >= 2`、不设 `MIR2_SHARDS > 1`；
- R-a 归属证据：Unsupported 一条经 seam ⇒ drained/unsupported 双计 + handler 侧 `zone_cmd_unsupported`
  增量（测试 6 原文）。
