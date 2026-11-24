# L2-5 P1 · M0 设计要点：每区 host 循环骨架（2026-11-23）

> 依据：已入 master 的评估稿 [`l25-per-zone-host-loop-scope-20261123.md`](l25-per-zone-host-loop-scope-20261123.md)
> §4（M0–M5）与 §12（今晚容量重估）。**本稿只覆盖 M0**（骨架），不写 M1 及以后。
> 派工：opc-27（2026-11-23）；复核：0c（评估稿作者）；实现：本会话（`card/zone-loop-m0`，自 `05be6a65e`）。
> 状态：**设计已改版，待 0c 最终确认后开 A1**。
>
> **M0 的实例化单元（写死）**：`HostLoop::tick`（含段级 `PhaseSet` 内联守卫）+ `HostLoop::for_zone`
> 构造。**段级方法化（`fn phase_*`）推迟到 M1**，与区 k 命令接线、源码扫描用例更新一起做——
> M0 不含 phase 方法**不是**不完整，是本阶段的范围定义。

## 0. M0 的验收（原文）

1. 把 `sim_loop` 的主循环体抽成「每区可实例化」的函数/结构，参数化 zone 局部态 + 借用进程服务；
2. `slicing=0` / N=1 **逐字节不变**：两树 stdout 对拍（`order_sha=b78156a7…`、
   `multiset_sha=bd4b13ba…`、86/86 行）——硬门；
3. **区 k 空实例能跑通一个 tick 不 panic**（不是「能出帧」，那是 M1）；
4. 全量门 `FMT/CLIPPY/TEST` 绿、`msgcodegen` 绿；不动既有行为。

## 1. 现状事实（已核；用符号锚，不贴会漂的行号）

| 事实 | 锚 |
| --- | --- |
| `sim_loop` 是一个**非 async** 的 `fn`，跑在 `std::thread::Builder::spawn` 里 | 生产调用点唯一：`sim_loop(` 在 spawn 闭包内 |
| 结构 = 启动装载（约 600 行 `let` 初始化）+ 主 `loop { … }`（约 2,750 行） | `loop {` 在 `fn sim_loop(` 之内唯一 |
| 主循环已被 `host_prof` 切成 17 段 | `host_prof.end("…")`：`HostShutdown`（关服支）+ 主循环 16 段：`HostInbound`、`HostDbPoll`、`HostPeriodic`、`HostWorldHead`、`HostPostHead`、`HostPlayerRun`、`HostWorldTail`、`HostPlanes`、`HostRecovery`、`HostFrames`、`HostDealTick`、`HostAoiDownlink`、`HostPersist`、`HostSwitch`、`HostDiag`、`HostSleep` |
| 段与段之间还有**未计入**的代码（如 `drain_yedo_events`/`drain_skill_train_events`/`clear_status_changes` 在 `HostWorldHead` 的 `end` 与下一段 `begin` 之间） | 段归属只覆盖 `begin…end` 之间 |
| 区 k 已有自己的世界侧状态 `ZoneState`（world/displays/online/ground_sessions/saves/login_tails/aoi_frames_total/shop_sessions/sell_off_sessions/save_outbox/alive/mon_gen） | `struct ZoneState`；被 `Arc<Mutex<ZoneState>>` 持有，区 0 线程按 tick 借锁写入 |
| 区 k 的 tick 入口是 `region_loop::run_zone_tick`（phase1→快照→barrier1→apply_mirrors→phase2→barrier2），**丢弃 `TickRecord`** | `spawn_minimal_zone` 内 |
| `World` 有 `owner_thread` 断言：只能在其构造线程 tick | `crates/world-bevy/src/world.rs` |
| **约 18 处源码扫描用例以 `fn sim_loop(` 位置为锚**，断言某生产调用「在其之后」；其中 `drain_pk_flag_downlink` 那条要求 `sim_loop < call < pk`（夹逼） | 用例名：`*_in_sim_loop` / `*_is_wired_in_sim_loop` 等 |

> 读法：第 8 行是本设计的**硬约束**——抽取出来的代码必须**文本上位于 `fn sim_loop(` 之后、
> `mod tests {` 之前**，且正文里那些被扫描的字面行**各保持恰一处、相对顺序不变**。这不难满足
> （把 `impl HostLoop` 紧贴 `fn sim_loop` 之后放），但必须当成设计前提，否则 18 条用例一起红。

## 2. 结构切分（三层）

```text
sim_loop(...)                       // 薄驱动：启动装载 → 组两个结构 → loop { 收关闭信号 + tick }
   ├─ struct HostLoop               // 区级状态（owned；本区私有，可每区一个实例）
   ├─ struct HostServices<'a>       // 进程级服务（借用；区 0 线程持有，区 k 只读借/经通道）
   └─ struct TickIo<'a>             // 本区输入/关闭信号（区 0 = ev_rx/ev_ctl_rx/shutdown；区 k = 自己的 rx/stop）

impl HostLoop {
    fn new_host(...) -> Self        // M0：不搬启动装载，只在 loop 前用字段简写把既有局部装进结构
    fn for_zone(zone, map_sizes, rng_seed) -> Self   // 区 k 空实例（复用 ZoneState::new 的构造口径）
    fn tick(self, svc: HostServices, io: TickIo) -> (Self, HostServices, TickIo, TickOutcome)
}
```

### 2.1 `HostLoop`（区级，owned）

**世界与身份侧**：`world`、`displays`、`online`、`ground_sessions`、`saves`、`login_tails`、
`aoi_frames_total`、`shop_sessions`、`trade_sessions`、`sell_off_sessions`、`mon_gen`。

**区级调度/观测（含跨 tick 结转，**不是**段内局部）**：`ticks`、`tick`（200ms 节拍）、
`event_carry` / `event_carry_ctl`（`EventDrain` 结转槽——**跨 tick 保序状态**，放段内局部会改
跨 tick 排水行为）、`entry_telemetry_next_ms` / `entry_telemetry_last`（5s 遥测门与去重串）、
`host_prof`（`HostTickProfile`）、`world_prof_last`、`line_notice_states`、`monster_walk_seen`、
`handoff_runtime`、`gates`（每区一份的 `SimToGate` 发送端表，由广播填充）、`ready_delay`
（`test_seams` 注入）、`is_host` / `telemetry_prefix`（片级派生量，只读）。

> **`gm_filter_word` / `gm_mission` / `gm_spirit_mutiny_tick` 不放这里**（0c 复核 P1，2026-11-23）：
> 它们是**进程级单例**——代码注释原文「进程级单例 ⇒ 由 `sim_loop` 持有」、
> `ChatHost.filter_word → GmWorld.filter_word` 注入 `@` 命令体。每区一份 `Cell`/`RefCell` 会让
> `@DisableFilter`/`@祈祷生效`/`@Mission` 只改一个区的副本 ⇒ 分区语义分裂。归 `HostServices`。

- 判据：**该状态只被本区会话/实体读写，或每区各有一份且由广播填充**（`gates`）。
- `ZoneState` 改为持有 `host: HostLoop` + 区 k 专属的 `save_outbox`/`alive`（`mon_gen` 进 `host`，
  区 0 的 `mon_gen_host` 也是 `HostLoop` 字段）；现 `st.*` 访问点只有 11 处（`st.world` 7、
  `st.displays`/`st.online`/`st.save_outbox`/`st.alive` 各 1），改动面小。

### 2.2 `HostServices<'a>`（进程级，借用）

`db`、`engine`、`guilds` + `guild_run_tick`、`castle_gates`/`castle_store`/`castle_war_states`/
`pending_gm_castle_broadcasts`、`npc_host`/`merchant_book`/`npc_vars_base`（`Rc` ⇒ `!Send`，但
`sim_loop` 在独立 std 线程，无 `Send` 约束）、`sell_off_book`、`item_number`/`item_number_ex`、
`planes`、`registry`、`enter_cfg`（含 `mon_gen.table`/`catalog`/`pet_cfg`）、`shard_runtime`、
`shutdown`/`shutdown_done_tx`、`ids_online`/`ids_out`、`world_start`、`win_lottery`、`zone_host`、
`server_table`、`colors`/`notice`/`party_cfg`、`pwd_sessions`/`reconnect_pending`/`kick_pending`/
`soft_close_pending`、`state_log`、`perf`（原子，无需借）、
**`gm_filter_word` / `gm_mission` / `gm_spirit_mutiny_tick`**（进程级单例，见 §2.1 注）、
**`magic_table_arc`**（进程级只读 `Arc`；每区 clone `Arc` 即可——M1 区 k 施法要用）。

- 判据：**跨区共享/单点**（评估稿 §5.1 的七项 + 登录/踢/换服编排 + 只读配置）。
- 注：`host_prof`/`world_prof_last` **留在 `HostLoop`**（每循环实例一份诊断器）：段计时由循环体
  自己 `begin/end`，与段同属区级；区 k 的实例 M0_MIN 不需要段读数（不跑段计时）。

### 2.3 `TickIo<'a>`（本区 I/O）

区 0：`ev_rx`/`ev_ctl_rx`/`shutdown`（原样）。区 k：自己的 `rx`/`ctl_rx`/`stop`。
`HostInbound` 段整体搬进 `phase_inbound(&mut self, svc, io)`——这是 M1「区 k 命令分发」的落点，
M0 只把结构摆好（区 k 的 `PhaseSet` 暂不含它）。

### 2.4 `PhaseSet`（要跑哪些段）

```rust
struct PhaseSet(u32);   // 位集：INBOUND | DB_POLL | PERIODIC | WORLD_HEAD | … | DIAG | SLEEP
const HOST: PhaseSet = PhaseSet::ALL;          // 区 0：全集，顺序 = 现正文顺序
const M0_MIN: PhaseSet = HANDOFF_TOP | WORLD_HEAD | WORLD_TAIL | PHASE2_GAP;  // 区 k 空实例
```

`HostLoop::tick` 按**固定顺序**逐段调用，`phases` 只决定「跳过哪些」，不改变其余段的相对顺序。
区 0 传 `PhaseSet::ALL` ⇒ 与今天逐行同序。

**成本与边界（0c 复核，2026-11-23）**：
- `PhaseSet(u32)` 是 `const` 位集：**无分配、无锁**；每段一次整型与运算 + 分支
  （16–17 次/tick，N=1 热路径可忽略，但按「零新增开销」边界**记明在此**）。
- `TickScratch` 在 `HostLoop` 内**预分配**，跨段值只 **move/借用**，**不得 clone** `TickRecord`/`Vec`。
- **`PhaseSet` 只 gate 段，不 gate gap** ⇒ 见 §3.1 的 gap 归属规则（不满足就会被区 k 无条件执行）。

## 3. 段边界 = `PhaseSet` 内联守卫（M0 **不**做方法化；保住行为与 TICKPROF 口径）

- **M0 的段边界 = 现有 `host_prof` 的 `begin/end` 之间**，用**内联守卫**
  `if phases.has(Phase::X) { … }` 包住该段（段内自带 `begin()`/`end("NAME", t)`，段名、位置不变）。
- **为什么 M0 不抽 `fn phase_*` 方法**（设计修订，2026-11-23）：方法化要把段内 2,750 行的裸局部
  （`world`/`online`/…）改成 `self.world`/`self.online`，而这恰是 18 处源码扫描用例**逐字**匹配的
  行（如 `drain_pk_flag_downlink(&mut world, &online, &gates);`）——那会迫使在「零行为变化」的
  M0 里改成用例，风险与收益倒挂。**方法化推迟到 M1**（与命令接线、用例更新一起做）；
  M0 交付的"可实例化单元" = `HostLoop::tick` 本身 + 段级 `PhaseSet` 守卫（区 k 用位集跳过）；
- 段间 gap 的归属见 §3.1（归其后第一个段，受同一守卫控制）。
- 段与段之间**未计入**的代码留在 `tick()` 里、原相对位置不变 ⇒ TICKPROF 段归属逐字节不变
  （不是「行为」但同属不回归面）。
- 跨段值（`rec: TickRecord`、`t: Instant`、以及逐段核对出的少数值）放进 `struct TickScratch`，
  由 `tick()` 持有并传 `&mut`。**这是 M0 实现时最大的机械工作**：每段逐一核对「本段产出、
  后续段消费」的局部值，全部走 scratch 或结构字段，不许改表达式顺序。

### 3.0 A2 实施落地（2026-11-23，与 §3/§3.1 一致）

- 17 个段各加 `if phases.has(PhaseSet::X) { … }`；**守卫包住段体的 `host_prof.begin()/end()`**，
  段间 gap 归其后段（唯一例外 = 循环头：`tick_no`/`begin_tick`/5s 遥测/`db` 顶部采样/`next`
  保持无条件，见 §3.1）。
- **跨段值只有两个**（`TickScratch`）：`rec`（WorldHead 产出，其后用到它的段消费）与
  `castle_broadcast_texts`（Periodic 产出 → PostHead 消费）。`rec` 在 WorldHead 守卫**之后**
  统一绑定一次（`let mut rec = scratch.rec.as_mut().expect(...)`）⇒ **凡启用「用 rec 的段」就必须
  同时启用 `WORLD_HEAD`**（M0_MIN 满足；违者 panic，属编程错误）。
  **A3 已落成运行期断言**（opc-27 2026-11-23 要求）：取 `scratch.rec` 之前
  `debug_assert!(phases.has(PhaseSet::WORLD_HEAD), "…（phases=0x{:05x}）", phases.0)`；
  负例 `zone_tick_without_world_head_panics`（`should_panic(expected = "WORLD_HEAD")`）——
  删掉该断言后 panic 来自 `expect`（文案不含 `WORLD_HEAD`）⇒ 负例红，已实测。
- **`let mut rec = tick_world_head_with_specials(` 字面量保留**（3 处源码扫描牙用它做 `sim_loop`
  顺序锚），`mut` 已无用 ⇒ 该行加 `#[allow(unused_mut)]`。
- `&mut rec` / `&rec` 保持 A1 原调用形状（`rec` 现为 `&mut TickRecord`）⇒ fn 级
  `#[allow(clippy::needless_borrow)]`。
- 本 fn 加 `#[rustfmt::skip]`（**临时**）：给 2,800 行正文套 `if` 会让 rustfmt 整段重排缩进，
  把「纯搬移」的 review 变成不可读的大 diff；代价 = 此 fn 内新增代码需手工排版（已在 fn doc 写明）。
  **移除条件（写死）：M1 方法化把段体拆成 `fn phase_*` 时，同步移除本 `#[rustfmt::skip]` 并统一
  格式化**——不得让这个函数永久脱离 rustfmt（opc-27 2026-11-23 裁决）。
- 两处 `#[allow]` 的理由同上并保留：`#[allow(unused_mut)]`（`let mut rec = tick_world_head_with_specials(`
  是 3 处源码扫描牙的 `sim_loop` 顺序锚，字面量不得改）、`#[allow(clippy::needless_borrow)]`
  （`&mut rec` / `&rec` 保持 A1 原调用形状）。
- **自证**（去缩进后与 A1 比）：正文语句 **2,865 → 2,908（+44，−1）**——新增的 44 行全是
  17 个守卫的开关、`scratch` 初始化与两处跨段搬运；**唯一被改的 1 行 = `fn tick` 签名**；
  17 个 `host_prof.end` 段名与顺序与 A1 完全相同。

### 3.2 段集见证（A3 补牙）与**源码扫描窗口**的坑（2026-11-23）

- **段集见证（仅测试期）**：`RAN_SEGMENTS` 是 `#[cfg(test)] thread_local!` 的 `Cell<u32>`；
  每个 `phases.has(X)` 守卫入口在 `cfg(test)` 下记一位；空实例用例断言
  `ran_segments == PhaseSet::M0_MIN.0`（**直接**证明只有 M0_MIN 的段被执行），并另有一条
  「`M0_MIN` 的组成写死 = `WORLD_HEAD | WORLD_TAIL`」——常量本身被塞位也红。
- **坑（实测踩到）**：本仓有多条源码扫描用例用
  `body = lines[fn sim_loop( .. 其后第一条以 "fn " 开头的行]` 作扫描窗口。**任何新增的顶层 `fn`
  只要插在 `fn sim_loop` 之后、被扫描调用点之前，就会把窗口截断** ⇒ 扫描计数变 0、用例假红
  （本次实际打到 `p31_production_wiring_anchors` 与
  `closed_arm_calls_close_cleanup_residue_once_in_decommented_production_view`）。
  ⇒ 见证的读写一律**内联**成 `RAN_SEGMENTS.with(...)`，**不新增顶层 `fn`**。
  M1 若要在这一带新增顶层函数，必须同时核这两条及同类扫描用例的窗口边界。

### 3.1 gap 的归属（0c/opc-27 要求，2026-11-23 定稿）

- **规则**：段与段之间**未计入** TICKPROF 的代码（如 `drain_yedo_events`/`drain_skill_train_events`/
  `clear_status_changes`/`clear_struck_log`）**归入其后第一个段**的 `phase_*` 方法，并放在该段
  `let t = host_prof.begin();` **之前**。⇒ 代码顺序不变、TICKPROF 段归属不变（gap 仍在 begin/end
  之外），但 gap 受**该段的 `PhaseSet` 位**控制。
- 由此 **M0_MIN 下 gap 不会无条件执行**：被跳过段的 gap 随该段一起跳过。
- **唯一的无条件 gap = 循环头**。逐项与区 k 空实例上的 no-op **代码依据**（不是「跑一下不 panic」）：

  | 循环头项 | 区 0 语义 | 区 k 空实例（`for_zone` + `M0_MIN`）上为何是 no-op |
  | --- | --- | --- |
  | `let tick_no = ticks;` | 本 tick 单源 tick 号 | 纯整数读取，无副作用 |
  | `cross_zone_frames::begin_tick(tick_no)` | 跨区邮箱 `src_tick/seq` 起点 | 线程局部 `SINK` 为 `None`（未 `install`）⇒ 直接返回，**无分配** |
  | 5s 遥测行（`entry_telemetry_*`） | `is_host && clock >= next` 才 `format!` | `is_host=false` ⇒ 短路，**零 `String` 分配**、无日志 |
  | `db` 应答表顶部采样 | `if let Some(w) = db.as_ref()` | `db=None` ⇒ 跳过（`sample_reply_table_at_tick_top` 不被调用） |
  | `let next = Instant::now() + tick;` | HostSleep 截止点 | 一次 `Instant::now()`（M0_MIN 的 tick 里 HostSleep 也被跳过，`next` 不参与） |

  ⇒ 区 k 空实例的循环头 = 两次原子读 + 一次整数赋值，**无 I/O、无分配、无锁**。
- 逐段的 gap 清单在 A2 搬移时**逐条列出**并核对「空世界下不发帧、不分配、不取墙钟做门」。

### 3.1 「构造入口闸」的准确范围（opc-27 2026-11-23 裁决）

- **N=1（`slicing=0`）照常构造 `HostLoop`**：它就是被抽出来的 `sim_loop` 状态，构造属纯重构，
  不是「额外区实例」；
- 闸针对的是**额外的区实例**：`slicing=0` 时**不得**构造任何 zone-k 的 `HostLoop`/`ZoneState`
  （M0 不接线 `spawn_minimal_zone` 正是为此）；
- 判据落点：`slicing_layout.is_none()`（既有判据）之下，`HostLoop` 构造点唯一，且区 k 的
  `for_zone` 在 `slicing=0` 路径上**不可达**（源码扫描用例钉调用点，见 §6）。

## 4. N=1 / slicing=0 逐字节不变：怎么保

1. **纯搬移**：只挪代码位置，不改任何表达式、分支、调用顺序、分配次数；`PhaseSet::ALL`
   下方法调用序列 = 原正文顺序。
   - **传递方式（A1 实施决定，2026-11-23）**：`tick` 按**值**拿走 `HostLoop`/`HostServices`/
     `TickIo` 并在返回时交还（`type TickOut`）。理由：只有按值解构才能让循环体 2,767 行
     **逐字不变**（裸局部 `world`/`online`/… 原样可用）；按引用解构会让每个 `&mut world` 变成
     `&mut &mut World`、每处 `&world` 变成 `&&mut World`，全靠 deref 强制转换，风险大。
     代价：每 tick 一次结构 **move**（无堆分配、无 clone、无锁）：`size_of::<TickOut>() = 17,760 B`
     （`HostLoop` 10,256 含 `World` 8,400；`HostServices` 7,448；`TickIo` 48）⇒ 进出 ≈35 KB memcpy，
     按 ~10 GB/s 粗算 ≈3–5 µs/tick（预算 200 ms）；编译器还可能省掉大部分。
   - 副作用：tick 期间三块状态被消费；panic 语义不变（sim 线程 panic 本就让该线程退出）。
   - **这不是永久架构决定**：`&mut self` 方法化（M1）落地后，循环体改用 `self.field`，
     不再需要按值搬入搬出 ⇒ 这笔 move 在 **M1 方法化时消除**（opc-27 2026-11-23 裁决）。
2. **构造顺序不变**：`HostLoop`/`HostServices` 在 `loop {` 之前用字段简写从既有局部「装入」，
   不重排启动装载（M0 不搬 `new_host`；`for_zone` 是另一条只给区 k 用的构造）。
3. **不新增热路径开销**：无新克隆/新 `Vec`/新锁；`Cell`/`RefCell` 仍是原实例。
4. **「零新增分配/克隆/锁」要可检查，不是承诺**（opc-27 要求）：
   - `PhaseSet` 是 `u32` 位集（Copy，参数不分配）；`TickScratch` 在 `HostLoop` 内**预分配**
     （字段是既有容器/`Option`，逐 tick 复用，不 new）；
   - **A1 交付时贴结构证据**：`HostLoop`/`HostServices`/`TickIo` 的字段数与
     `size_of`（含 `TickScratch`），并说明「每 tick 新增分配 = 0（无新 `Vec`/`String`/锁）」；
   - 运行期旁证（1k 臂 TICKPROF 各段差 < 噪声）**由窗口排期执行**，不在 A1 交付内。
5. 门（三条，缺一不合并）：
   - N=1 两树 stdout 对拍（`n1_stdout_equivalence.ps1`，最终树，`order_sha`/`multiset_sha`/86 行）；
     **真机窗口，由 a7 在最终合并树执行**——我不把结构论证当证据；
   - `crates/world-bevy/tests/tick_phases.rs` 等价用例 + `cargo test --workspace` 全绿；
   - 18 处 `fn sim_loop(` 源码锚用例不红（它们本身就是回归门）。

## 5. 区 k 空实例跑一个 tick（M0 第 3 条）

- `HostLoop::for_zone(zone, map_sizes, rng_seed)`：与 `ZoneState::new` 同口径（`World::with_maps`、
  `seed ^ zone`、`set_id_base`、`set_id_segment`），其余表空。
- `HostLoop::tick(…, PhaseSet::M0_MIN)`：handoff 顶 → `tick_world_head_with_specials`（空世界）→
  `tick_tail` → `tick_phase2`；**不跑 inbound/db/persist/switch/diag/sleep**（无 I/O、无帧）。
- 用例 `empty_zone_loop_runs_one_tick_without_panic`：
  - 断言不 panic、`world.entities.len() == 0`、`ticks == 1`；
  - 断言 `PhaseSet::M0_MIN` **之外**的段没有执行（用段计数器/`TickScratch` 里的段标记，不用日志）；
  - 负例（变异）：把 `PhaseSet` 判定改成恒真 ⇒ 空实例会走 `phase_inbound` 且无 `ev_rx` ⇒
    本用例要红（证明位集真的在起作用，不是装饰）。
- **M0 不接线**：`spawn_minimal_zone` 仍按原样跑 `run_zone_tick`；不把区 k 切到 `HostLoop::tick`，
  不产生任何帧（那是 M1）。

## 6. 源码扫描用例的兼容约束（实现时逐条现场核）

被扫描的字面行必须**恰一处**且顺序不变，例：`} else if movewire::is_move_command(cmd.ident) {`、
`drain_deal_tick(`（含实参形状）、`drain_action_downlink(&rec, &world, &displays, &online, &gates);`、
`drain_skeleton_downlink(&rec, &world, &displays, &online, &gates);`、`drain_pk_flag_downlink(&mut world, &online, &gates);`、
两处 `spawn_statics_for_zone(` 与区 0 商人装载、`event_carry/event_carry_ctl` 的预算排水分组等。
**约束**：`struct HostLoop` 与 `impl` 放在 `fn sim_loop(` 之后、`mod tests {` 之前；正文逐行搬。
若某条扫描用例因抽取而语义含糊（如按「`sim_loop` 体内」理解），**先报告再改用例**，不静默放宽。

**锚形态确认（opc-27 第 3 点）**：这 18 处用例**全部按符号/字面内容定位**——用
`lines.iter().position(|l| l.trim() == "fn sim_loop(")` 一类**文本匹配**求位置，再与目标行的
`position` 比大小；**没有任何硬编码行号**（本文档自身亦只用符号锚，未写 `path:line`，
docs-anchor gate 对新增行「检查 0」即依据）。

## 7. 风险与规避

| 风险 | 规避 |
| --- | --- |
| 借用检查：段内要同时 `&mut` 多个字段 | 每段顶部 `let HostLoop { world, displays, online, ground_sessions, .. } = self;` 逐字段解构；`svc` 同样解构 |
| `enter_cfg`/`registry` 同时被 `&` 与 `&mut` 借 | 段内只在需要处取，不跨段持有；必要时把 `&mut` 借用缩到最小作用域 |
| 段间未计入代码被误并入某段 | 方法只含 `begin…end` 之间；gap 留在 `tick()`（§3） |
| 跨段值遗漏 | 逐段核对清单（`rec`/`t`/…），M0 作为独立提交前先对每段做「产出-消费」表 |
| `World.owner_thread` 断言 | `HostLoop` 只在其构造线程 tick；区 k 的实例只在区 k 线程构造 |
| 源码扫描用例红 | §6 的放置规则；提交前跑全量测试 |
| `perf_counters` 段语义漂移 | M0 不加任何列、不改 `write_tick`；分区计数列属后续（标注 additive） |

## 8. M0 内部实施顺序（每步可编译、可测、可回滚）

1. **A1 结构**：加 `HostLoop`/`HostServices`/`TickIo`/`PhaseSet` 类型 + 空实现；`loop {` 前装入
   （`HostLoop::new_host` 用字段简写），`sim_loop` 仍跑原正文 ⇒ 编译 + 全量测试绿（此时零行为变化）。
2. **A2 段级守卫**：按 §3 给每个 `host_prof` 段加内联 `PhaseSet` 守卫（gap 一并归入后段守卫），
   一段一提交（或两段一提交）；每步跑 `cargo test -p game-svr --lib` + 该段相关用例；
   `PhaseSet::ALL` 路径逐字节不变。
3. **A3 区 k 构造**：`for_zone` + `ZoneState { host, … }` + `empty_zone_loop_runs_one_tick_without_panic`
   用例；`spawn_minimal_zone` 行为不变（仍走 `run_zone_tick`）。
4. **A4 收口**：`PhaseSet` 位集负例变异、18 处源码锚用例核对、全量门（FMT/CLIPPY/TEST/msgcodegen/
   anchor/tool）、N=1 两树对拍由 a7 在最终树执行。

## 9. 明确不做（属 M1+ 或另有卡）

- 不接线区 k 的 drain/命令分发（M1/M2）；不改 `spawn_minimal_zone` 的循环。
- 不动 `World.OutputAoi`/`AoiMonsterWalk` 算法、不动 `collect+sort` 顺序契约。
- 不加分区计数列（后续标 additive）、不改日志/帧内容与计时。
- 不搬启动装载（`new_host` 留在 `sim_loop` 内联；M1 之后再评估）。

## 附：测试端口探针（A1 合并门被阻塞后的修复，2026-11-23）

**现象**：A1 合并门红在 `login-srv` 的 `session_face_accepts_loopback_despite_whitelist_miss`——
子进程绑定 `0.0.0.0:24670` 报 `os error 10048`，就绪门超时。

**根因（订正过一版认知）**：`24670`/`26200` **不是硬编码**，而是 `next_port()` 从
`[20000,32000)` 算出的动态候选；缺陷在**探针地址与子进程绑定地址不同形**：
探针 `bind("127.0.0.1", port)`，子进程 `bind("0.0.0.0", port)`。ToDesk 在 `127.0.0.1:24670`
上有一条 **established 连接**（无 listener）时，Windows 让 `127.0.0.1` 绑成功、`0.0.0.0`
报 10048 ⇒ 旧探针把端口判为空闲发出去。

**修复**：`port_bindable_for_child(port) = bind(("0.0.0.0", port))`，`next_port()` 改用它；
五个 `tests/support/mod.rs`（db-srv / game-svr / login-gate / login-srv / sel-gate）同步同一文本
（该文件头声明五份「逐行相同」）。提交 `2d1829786`。

**没有合成牙——回归保护是缺位的，别误以为有**：已在进程内尝试复现 ToDesk 形状
（先 `listen` 再 `accept`、随后关闭 listener，只留 established 连接），Windows 上复现不出
「`127.0.0.1` 可绑 / `0.0.0.0` 冲突」；改用 `SO_EXCLUSIVEADDRUSE` 占用得到的是相反形状
（`127.0.0.1` 端口 10048 ⇒ FAIL / `0.0.0.0` ⇒ OK）。⇒ **不放假牙**；验收依赖**真实环境条件**
（ToDesk 占 24670 时该用例通过）+ 进程内一次性判别式
（`old_127_probe_ok=true / new_0_probe_ok=false`，见提交信息）。
