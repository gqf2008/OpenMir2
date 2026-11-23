# M1b 第 3 步实施规格：区 k 排水预算与有界化（2026-11-23）

> 父 `f0853f793`。上游依据：`l25-m1b-plan-20261123.md` §3.5（v2.1：新旧两套列并存、
> 溢出=硬安全阀非 0 即作废）。**本文件只定实施规格、不写码**；行号为 `f0853f793` 当日值，
> 代码自 `07b00e85` 未变；引用以**符号锚**为准（行号会漂）。

## 0. 改动清单（一句话版）

- `lib.rs`：①区 k 线程循环（`spawn_minimal_zone`）把**两段裸 `try_recv`** 换成**有预算的排水 seam**；
  ②`spawn_zone_demux` 增加**每区 `outstanding` 计数 + `Command` 硬上限丢新**；
  ③`perf_counters` 增 **4 个 `zonek_*` 列 + 4 个 `*_all_zones` 派生列**（区 0 三列语义不动）。
- 不新增 cargo feature；多区路径才有区 k ⇒ **N=1 零参与**（`zonek_*` 恒 0、派生列 == 区 0 列）。

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
- **低估上界 = 2**：carry 探针取到的事件已 -1 但未处理 ⇒ `outstanding` 比通道真实长度少 ≤2。
  `>= CAP` 的判定下，低估 2 只让实际通道容量多 2 条，可接受（写进常量注释）。
- **不会下溢**：`fetch_sub` 只在 `try_recv Ok(Command)` 处发生，而该条必先有一次 +1 ⇒ 计数 ≥ 0。
  debug 构建可在减前断言 `>0`（release 用裸 `fetch_sub`）。
- **shutdown 残留**：区线程退出时通道里可能还有 `Command`（`outstanding > 0`）——退出路径打一行
  WRN（zone + outstanding 值），**不视为错误**（进程随后整体退出；`Arc` 随进程释放）。
- 其它事件的通道积压不设上限：本切片只保护"**每玩家命令**"这条最可能被负载打爆的路径；
  `Closed/Opened` 等控制类不在上限内（数量级由连接数约束，且丢它们的代价更高）。

### 1.4 上限常量与观测

- `const ZONE_INBOUND_MAX_EVENTS: usize = 10_000;`（= 2000 条/tick × 5 拍）。注释写死：
  "软上限；正确性不依赖精确值；按窗口实测的区间到达率调整；低估 ≤2（carry 探针）"。
- 丢弃：`perf_counters::note_zonek_inbound_overflow_dropped()` + **限速 WRN**（复用
  `DepartWarnLimiter` 同型限流器：每分钟 ≤1 条，字段含 zone/line/socket/ident）。
- **生产应恒 0**；非 0 ⇒ 该窗口验收作废（v2.1 §3.5/§4 判据；测试见 §5-⑤）。

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

fn drain_zone_events(
    ctl_rx: &std::sync::mpsc::Receiver<GateEvent>,
    rx: &std::sync::mpsc::Receiver<GateEvent>,
    carry_ctl: &mut Option<GateEvent>,     // 跨 tick 保活（循环外局部）
    carry_normal: &mut Option<GateEvent>,
    outstanding: &AtomicUsize,             // 取 Command 时 -1
    mut on_batch: impl FnMut(&mut Vec<GateEvent>),   // 生产：锁一次 + 处理整批
) -> ZoneDrainStats
```

循环（每 tick 调用一次）：

1. **先把 carry 放进批首（保序）**：`carry_ctl.take()` → `carry_normal.take()`（ctl 先）；
   `stats.from_carry` 记这两条（若存在）。
2. **填批**：`while batch.len() < BATCH_MAX (=256)`：先 `ctl_rx.try_recv()`、再 `rx.try_recv()`；
   每次 `Ok` 时若为 `Command` 就 `outstanding.fetch_sub(1)`。空批 ⇒ 结束。
3. **处理**：`on_batch(&mut batch)`（生产闭包：`let mut st = state.lock(); for ev in batch.drain(..) { …归集/处理… }`），
   `stats.drained += n; stats.batches += 1`。
4. **预算检查**（在**批与批之间**，整批要么全处理、要么不开始）：
   若 `stats.drained >= PRODUCTION_MAX_EVENTS || tick_elapsed >= PRODUCTION_MAX_WALL`
   ⇒ **探针**：`ctl_rx.try_recv()` 有 ⇒ 存 `carry_ctl` 且 `budget_hit=true`；否则试 `rx.try_recv()`
   ⇒ 存 `carry_normal` 且 `budget_hit=true`；两个都空 ⇒ `budget_hit=false`（本 tick 已排空）。结束。
   否则回到 2。
5. 出口：`note_zonek_inbound_drained(from_carry, ev)` 由 `on_batch` 内的调用方或 seam 统一记
   （**写 `zonek_*` 列，禁止调 `note_inbound_drained`**）；`budget_hit` ⇒ `note_zonek_inbound_budget_hit()`。

### 3.3 carry 的语义与保序

- 两个 `Option<GateEvent>` 声明在**区 k 线程循环之外**（`while !stop` 之前，与 `gates` 同级），
  跨 tick 保活；不放进 `HostLoop`/`ZoneState`（排水在 `HostLoop::tick` 之外）。
- 与 `EventDrain` 的 carry 语义**相同角色、不同取法**：EventDrain 的 carry 是它自己在预算边界
  多取的那一条；本设计是**预算耗尽后由 seam 主动探针**取一条，用来判定 `budget_hit` 并保住它。
- **保序**：探针取走的必然是通道**队头**那条；下 tick 把它放在批首 ⇒ 不越过任何更早事件。
  两通道各自 FIFO；跨通道仍 **ctl 优先**（与现有两段 `try_recv` 的次序一致）。
- `BATCH_MAX = 256`：锁内一次处理 256 条命令的锁持有量级与现有 `cmd_events` 同阶；
  窗口实测若锁内 >10ms 则下调（常量注释写"按实测调"）。

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

1. **`zonek_drain_honors_event_budget`**（预算）
   - 构造：`std::sync::mpsc::channel()` 注入 2500 条 `GateEvent::Command`（+ 少量 `Closed` 作对照）；
     调 `drain_zone_events`（`on_batch` 只计数、不锁）；预算用 PRODUCTION。
   - 断言：`stats.drained <= 2000`；`tick_elapsed <= 50ms + ε`；通道剩余 = 2500 - drained（用随后
     清空调验）；`std::sync::mpsc` 无长度 ⇒ 剩余数用"再排一次清空"计数。
   - 打印：`drained={} elapsed_ms={} left={} batches={}`。
2. **`zonek_carry_preserves_fifo`**（carry 与保序）
   - 构造：注入 A、B、C 三条 `Command`（payload 可区分）；预算传**测试用小值**
     （`EventDrainBudget { max_events:1, max_wall:大 }`，seam 接受注入）⇒ 第一次调用的探针把 B 存入 carry；
     第二次调用断言**先**处理 B（`stats.from_carry==1`），再到 C。
   - 断言：处理顺序 `A,B,C`；`budget_hit` 第一次 true、第二次 false。
   - 打印：`order={:?} from_carry={} hit={}`。
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
   - **配合行为断言**：测试 1 驱动 seam 后断言 `zonek_inbound_drained` 增加；若有人把循环改回裸
     `try_recv`（不经 seam），计数恒 0 ⇒ 源码牙红 + 行为计数为 0 双重暴露。
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
  1. 锁内批处理时长：`BATCH_MAX=256` 为初值，窗口实测 >10ms 则下调（常量注释写"按实测调"）；
  2. `outstanding` 低估 ≤2（carry 探针）：只放宽容量 2 条，注释写明；
  3. 丢新只对 `Command`：控制类事件（`Closed/Opened/注册`）永不丢——若将来要限控制类，另立卡。
- **不合并/回滚判据**：§5 五条不全绿；或任一验收窗口 `zonek_inbound_overflow_dropped != 0`
  （v2.1：该窗口作废并开缺陷卡）。

## 7. 交付时报告什么

- 改动文件与 `+/-` 行数；常量值（`ZONE_INBOUND_MAX_EVENTS`、`BATCH_MAX`）与取值理由；
- 五条测试的原文输出（各自一行打印）+ 变异证据（删预算检查 ⇒ 测试 1 红；恢复后 sha 相等）；
- TSV 表头新增列清单与一行样例值（含派生列恒等式）；
- N=1 零参与证据（区 k 不存在时 `zonek_*` 全 0，派生列 == 区 0 列）。
