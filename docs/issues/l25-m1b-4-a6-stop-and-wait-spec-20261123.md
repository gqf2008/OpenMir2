# M1b 第 4 步实施规格：A6 同玩家跨类顺序（stop-and-wait，2026-11-23）

> 父 `b38b9c42b179f52e00b635165d1b34b70a62b20f`（master，双远端同 sha；第 3 步最终形态 = `7b118c376`
> 入站有界化 + `b38b9c42b` 删 `inbound_carry_all_zones` 派生列、派生 6→5、零行为）。上游依据：
> `l25-m1b-plan-20261123.md` §3（A6 v2.1）与 §5.2/§5.3（逐点位 + 决策/执行分离）；**依赖第 3 步规格 r6**
> `l25-m1b-3-zone-drain-budget-spec-20261123.md`（`TickIo.inbound_local_buffer` /
> `inbound_local_carry_prefix` / `inbound_outstanding`、`pending_inbound` scratch、P1 swap 不变式、
> P4 共享执行预算、P5 单 tick ≤2000、**6 个** `zonek_inbound_*` 列、**派生 5 个**）。本文件只定实施规格、
> 不写码；引用以**符号锚**为准。
>
> **r3（2026-11-23，父 `b38b9c42b`，99 预读 + opc-27 裁决）**：①控制邮箱**有界**
> `CONTROL_MAILBOX_CAP = 256`（区 k 每区一份，按实测可调），满 ⇒ 不置位 + 丢弃 + 第 3 列
> `zonek_control_mailbox_rejected` + 限速 WRN；②清位用 **RAII 作用域守卫**（任何离开路径都清；panic
> 不在守卫责任范围）；③`deferred_local` 溢出 = **8+1 条按到达序就地执行**（不搬 `pending_local`、
> 不重新分类、**不动在飞计数**）；④顺序按**到达序号**（不按 `chr_name` 排序），`pending_local` = 区 k
> **全局有序 FIFO**（`BTreeMap<u64,_>`），段内与 `inbound_local_buffer` **按序号合并**；在飞改为
> **`control_pending` 计数 + `control_first_tick` + `control_epoch`**（支持同玩家多条 `@` 连发）；
> `wait_timeout` = **判定解除的次数**；⑤列数按第 3 步 r6：6 + 本步 **3** = **9 列**（**r4 订正为 4 列 / 10 列**，见 r4），派生 5 + 0；
> ⑥`defer_overflow` / `mailbox_rejected` / `wait_timeout` 是**被观测到的降级、不作废**；作废只看第 3 步
> `overflow_dropped` / `pending_discarded`；⑦`zonek_inbound_drained` 口径按 §6 **覆盖**（含邮箱满
> 丢弃等「处置但未执行」的条数）；⑧接缝元素带 `seq` 的**覆盖声明**见 §1（不动第 3 步 r6 文本）；
> ⑨§4 顺序判据以三个 A6 降级计数全 0 为前提；⑩超时降级是已知代价（§6）。
> **r4（2026-11-23，父 `b5d123c24`，按 99 实现与 opc-27 五条裁决对齐）**：
> **D1** 邮箱判满/push/置计数**同一临界区**；**并发前提不在单线程负例覆盖内**（必须写进代码注释，评审核）。
> **D2** 溢出 `8+1` 批次**整体占预算**（`remaining_events >= batch.len()` 且墙钟满足才执行；否则**整批按
> seq 移入 `pending_local`** + `OverflowPostponed` ⇒ 驱动置 `budget_hit` 并收手；载体取实现形态，不另设
> 容器）。**D3** 守卫持会话表 `&mut`（执行经短借用）。**D4** `PendingLocalCmd` 携带完整 `GateEvent`。
> **D5** 新增 A6 专属 `zonek_control_shutdown_discarded`（关服清点 `pending_local` + `deferred_local`，
> **不作废**；不共用第 3 步 `pending_discarded`）。负例 7 → **9** 条；`zonek_control_*` **4 列**、合计
> **10 列**。

## 0. 一句话

同一玩家 P 先发 GM `@`（PROCESS，回区 0 执行、T+1）再发本地命令（LOCAL，T）时，**P 的本地命令
暂缓到 `@` 执行完再放行** ⇒ 保住"同收件人帧相对序"。延迟命令按**到达序号**放行与执行（跨玩家亦然、
不按玩家排序）；窗口内本地命令最多延迟 1 tick；`@` 低频。

## 1. 依赖面（第 2/3 步的符号；命名不符只报差异清单、不改本稿）

**来自第 3 步（r4 缓冲字段 + r6 列/执行句柄）**：
- `pending_inbound: VecDeque<SeqEvent>`（区 k 线程循环**局部 scratch**，锁外 fill；元素同样带 `seq`——
  到达序号在 **fill 时**分配，每区线程单调递增）；
- `TickIo.inbound_local_buffer` + `TickIo.inbound_local_carry_prefix: usize`（段的工作区，锁外交接、
  tick 后 swap 回）；**本步把元素类型从 `VecDeque<GateEvent>` 改为带 `seq` 的元素**（`SeqEvent { seq, ev }`
  或 `(u64, GateEvent)`）；**`carry_prefix` 仍按下标语义**。**此句覆盖第 3 步 r6 §3.3 的元素类型一句**
  （不动 r6 文本）；若第 3 步实现已自带序号则直接复用、只报差异清单。
- **段内消费顺序**：`pending_local` 与 `io.inbound_local_buffer` 的条目**按到达序号合并**（谁小先执行，
  §3.4）；r4 的"先 `pending_local` → 再 buffer"是它的**自然结果**，不再单列规则。
- 计数（第 3 步 **r6** 仍是 **6 列**）：`zonek_inbound_drained/carry/budget_hit/overflow_dropped/
  pending_discarded/unsupported`（**`unsupported` = 分类归属计数**，第 3 步不丢；第 5 步接管丢弃时
  另加 `zonek_inbound_unsupported_dropped`）；**A6 自身新增 4 列**（`zonek_control_defer_overflow` /
  `zonek_control_wait_timeout` / `zonek_control_mailbox_rejected` / `zonek_control_shutdown_discarded`，
  见 §6；若实施后实际列名不同，只报差异清单）。

**来自第 2 步（分类器）**：
- `zone_cmd_class(ident, payload) -> ZoneCmdClass`，`ZoneCmdClass::{Local, Process, Unsupported}`；
  `Process` = `CM_SAY` 且 payload 以 `@` 起（复用 `gm_drain_gate` 的严格 `starts_with('@')`）。
  **若第 2 步落地后类名/函数名不同（如 `InboundOwnership::Process`），只报差异清单，不改本稿。**

**来自第 1 步/既有代码**：`INBOUND_LOCAL` 段位与 `RAN_SEGMENTS`；`SessionGround`（`ground_sessions`
的键 = `chr_name`）；区 0 的 GM 执行入口（`handle_say_with_saves` → `handle_gm_command` 的调用链）。

## 2. 在飞控制命令：`control_pending` 计数 + `control_first_tick` + `control_epoch`（只有区 k 用）

- **键 = `chr_name: String`**（不是 actor id、不是 socket）：区 k 的排水/分派天然粒度就是
  `(chr_name, socket, cmd)`；A6 的状态放在 `SessionGround`（`ground_sessions: HashMap<String, SessionGround>`）。
- **字段（三个控制字段 + 既有 `deferred_local`；`A6PlayerState` 小表，供实施/评审对齐）**：

  | 字段 | 类型 | 语义 | 谁写 | 谁读 |
  | --- | --- | --- | --- | --- |
  | `control_pending` | `usize` | 该玩家**尚未执行完**的控制请求数；0 = 无在飞 | 区 k 置位（**与 push 同一临界区** +1）；区 0 守卫（epoch 匹配且 >0 才 −1） | 区 k 暂缓判定、tick 顶决策 |
  | `control_first_tick` | `Option<u64>` | **最早一条**未执行控制命令的 tick（超时只按它算） | 置位（仅 `None` 时）；守卫减到 0 清；超时清零 | tick 顶决策 |
  | `control_epoch` | `u64` | 代际；超时 +1 作废旧请求的减法 | tick 顶决策（超时） | 置位盖章、守卫比对 |
  | `deferred_local` | `VecDeque<PendingLocalCmd>` | `@` 在飞期间的本地命令暂缓队列（上限 8） | 段内分类（暂缓入队/溢出清空）；tick 顶决策整队搬出 | 段内分类、tick 顶决策 |

  构造点（`session_ground_from_login` + 测试构造）补默认 `0` / `None` / `0` / 空队列。
  **推迟批次不另设容器**：溢出装不下时整批按 `seq` 移入 `pending_local`（§3.1，实现选择的等价形态）。
- **置位（区 k，`INBOUND_LOCAL` 段内、分类之后）**：分类结果为 `Process` ⇒ ①在**邮箱锁的同一
  临界区**内判 `mailbox.len() >= CONTROL_MAILBOX_CAP`；②**未满才 `push`**（请求盖 `epoch`）；
  ③**`push` 成功后仍在同一临界区内**：`sg.control_pending += 1`，且 `control_first_tick.is_none()`
  时置 `Some(tick)`（= **最早一条**的段内 tick，见 §2.1；后续同玩家请求**不覆盖**它）——
  **置计数与 push 必须原子**（r4·D1）：区 0 的 drain 与区 k 段并发；计数若落在锁外，会被
  "执行先于置位"抢先 ⇒ 守卫见 0 跳过减法、计数粘住直到超时；
  ④**满 ⇒ 丢弃该命令** + `zonek_control_mailbox_rejected += 1` + 限速 WRN（`DepartWarnLimiter`
  同型：每分钟 ≤1 条，字段 zone/chr/ident）+ **不置位、不改在飞计数**（`drained` 仍按 §6 口径
  由段内计）；⑤满/异常路径**不得**静默，也**不得**先置位后入队。
  - **锁序（为什么无环）**：区 k 段内 = 持 `state` → 取邮箱；区 0 drain = **取空邮箱即释放** →
    再取区 k `state` ⇒ 不存在"持邮箱等区锁"的路径。
  - **并发前提（写死）**：§5 的负例是**确定性单线程**，**不覆盖**上述并发窗口；正确性由临界区
    保证。**该前提必须同时写进代码注释**（`a6_enqueue_control` 与守卫 `Drop` 附近）——评审核注释。
  - 位置描述（可验证）：在 `tick_phase_a` 的 `INBOUND_LOCAL` 分支内、`zone_cmd_class(...)`
    返回 `Process` 的那个 `match` arm 里。
- **清位（区 0，执行完 P 的 GM 命令时）**：区 0 的**控制邮箱 drain**（M1b 新增，紧随区 0 的
  `INBOUND` 段/`HostInbound` 之后）逐条取出请求：
  1. 先**持区 k 的 `state` 锁**（form (d)：形状同既有 `finish_login_on_zone_state` /
     `handle_zone_closed` 的"持区 k 锁 + `&mut` 区 k `SessionGround`"）；
  2. **取到会话表 `&mut` 后立即构造 RAII 作用域守卫**（**持会话表 `&mut`**，执行体经**短借用**
     `run(...)` 访问；不拆三字段——r4·D3 以实现为准、行为等价），**再**用区 0 的 GM 上下文执行该
     `@` 命令（`handle_say_with_saves` 同一入口，`chr_name` = P）；
  3. **守卫 `Drop`（任何离开路径：正常返回 / `continue` / 早跳都会走）**：仅当 `req.epoch ==
     sg.control_epoch` 时 `control_pending = control_pending.saturating_sub(1)`；**只有减到 0 才清
     `control_first_tick = None`**。epoch 不匹配 ⇒ **不动计数** + 限速 WRN（该请求已被超时写掉）。
     **panic 不在守卫责任范围**——本仓失败即整机 abort（`catch_unwind` 后退出），不做恢复；
     放行仍由 §3.3 的 tick 顶决策在下一次读 `control_pending == 0` 时统一 `mem::take`，清位处不直接执行。
  4. **会话缺失**：执行时 `ground_sessions` 已无该玩家 ⇒ 该请求**丢弃 + 限速 WRN**、**不触碰任何计数**
     （没有可减的对象）；**这不算失败路径**。
- **控制邮箱形态**：M1b 新增；`Arc<Mutex<VecDeque<ControlRequest>>>`（一次规划处创建，区 k 持 clone
  推入、区 0 每 tick drain）——**不新增 `state` 锁次数**（推入不持区锁）；区 0 执行时才持区 k 锁。
  **有界**：`const CONTROL_MAILBOX_CAP: usize = 256;`（**区 k 每区一份**，常量旁注"按实测可调"）。
  `ControlRequest = { chr_name: String, line: String, at_tick: u64, epoch: u64 }`。
  **区 0 线程已退出**（控制邮箱 drain 不存在）⇒ 邮箱**只涨不清**：表现为 `control_pending` 常设、
  `zonek_control_wait_timeout` 持续增长（最终由 tick 顶超时降级放行）。
- **N=1**：不构造控制邮箱、不走本套（区 0 直执行） ⇒ 零参与（三个字段保持默认 `0` / `None` / `0`）。

### 2.1 tick 源（写死）
置位/暂缓/超时**全部在 `INBOUND_LOCAL` 段内**完成（段在 `zone_tick_top(tick)` 之后、同一把
`state` 锁内）⇒ 段内 `self.ticks` 与驱动循环的本地 `tick` **同值**（`zone_tick_top` 在 A 段前写）。
**禁止**在 `zone_tick_top` 之前的位置读 `st.host.ticks`（那是上一 tick）。段外用本地 `tick`；
段内用 `self.ticks`；二者同值，不存在跨侧比较。

## 3. 延迟队列：`deferred_local` / `pending_local`

### 3.1 容器与上限
- `deferred_local: VecDeque<PendingLocalCmd>`（**每玩家**，放 `SessionGround`）；
  `pending_local: BTreeMap<u64, PendingLocalCmd>`（**区 k 单份**、按到达序号有序的**全局 FIFO**，放
  `HostLoop`；N=1 恒空）。`PendingLocalCmd = { seq: u64, ev: GateEvent, at_tick: u64 }`（**携带完整
  事件**：放行时走与正常 LOCAL 同一执行路径，`recog/tag/series/tail` 都参与执行，拆成 ident/payload
  就不是同一路径——r4·D4）。
  **`pending_local` 不是 `VecDeque`，语义等价于全局 FIFO、不是按玩家分桶**；取最小 =
  `iter().next()`（`BTreeMap` 按 `seq` 有序 ⇒ 省掉排序、也不需要按玩家遍历顺序执行）。
- **每玩家上限 N = 8**（只约束 `deferred_local`）：`deferred_local.len() == 8` 再来 ⇒ 组批
  **8 + 1 = 9 条**（按 `seq` 排序），**整批原子处置**（r4·D2）：
  - **装得下本 tick 剩余预算**（`remaining_events >= batch.len()` **且**墙钟仍在 `execute_wall` 内；
    `remaining_events = execute_cap − executed`，**含当前这条**）⇒ **就地整批执行**（按到达序、逐步
    计 `zonek_inbound_drained`、**按 `batch.len()` 占执行预算**）+ `zonek_control_defer_overflow += 1`
    + 限速 WRN；`control_pending` / `control_first_tick` 不动；
  - **装不下 ⇒ 整批按 `seq` 移入 `pending_local`**、本 tick **零执行**、返回 `OverflowPostponed`
    ⇒ 驱动置 `budget_hit = true` 并**立即收手**（本段不再执行后续条目）——**不 partial、不丢、不越序**。
    **载体 = `pending_local`**（实现选择的等价形态；不另设 `overflow_batch` 容器）。本路径**不计**
    `defer_overflow`（实现口径：该列只计**已执行**的溢出批次）。
  - **原子性边界（写死）**：原子性保证在**触发 tick**；推迟后的批次成为 `pending_local` 的普通有序
    条目，下一 tick 起按 §3.4 合并**逐条受预算约束**（跨 tick 可能分批完成；若要求跨 tick 原子，
    须另加批次标记，另卡）。
  - 溢出路径**不重新分类**、与正常 LOCAL 同一执行路径；执行点在段内 ⇒ `RAN_SEGMENTS |=
    INBOUND_LOCAL` 与 `HostInbound` 计时天然覆盖。**越序只影响该玩家**、不改变其他玩家的到达序。

### 3.2 暂缓（段内分类处）
在 `INBOUND_LOCAL` 段内逐事件分类时：
- `Process` ⇒ 控制邮箱 + 置位（§2）；
- `Local` 且 `control_pending > 0`：未满 N ⇒ **不执行**，`deferred_local.push_back(PendingLocalCmd {
  seq, ev, at_tick: 段内 tick })`（`seq` = 该条 Command 的到达序号，§1）；**已满 N（8）⇒ 溢出路径
  （§3.1）：整批原子——装得下就地执行，装不下整批移入 `pending_local` 并收手**；
- `Local` 且 `control_pending == 0` ⇒ 就地执行（正常路径）；
- `Unsupported` ⇒ **第 3 步**：`zone_cmd_class` 归属计 `zonek_inbound_unsupported`、**仍交
  `handle_zone_command`**（handler 侧 `zone_cmd_unsupported` 照计，行为不变）；**第 5 步接管丢弃**
  （届时本行改为「丢弃并计数、不进 `zonek_inbound_drained`」，并新增
  `zonek_inbound_unsupported_dropped` 列）；**A6（第 4 步）不涉及本机制。**

### 3.3 决策（tick 顶、锁内、**不执行**）
区 k 线程循环顶（`fill_pending` 之后、锁 1 之前）用**一次短锁**做决策（与现有 `closed/cmd` 的
锁一次同形）：
- 对每个 `deferred_local` 非空的玩家：若 `control_pending == 0` **或** 超时
  （**`tick - control_first_tick >= 2`**，tick = 循环本地 `tick`）⇒ 把该玩家 `deferred_local` 整体
  搬入 `pending_local`（`BTreeMap::insert(seq, cmd)`；**不按 `chr_name` 排序、遍历序不影响执行序**——
  执行序只看 `seq`）。超时另做：`control_epoch += 1`，**`control_pending = 0` 且
  `control_first_tick = None` 一并清零**（防重复触发；旧请求之后执行时 epoch 不匹配 ⇒ 不动计数）+
  WRN + `zonek_control_wait_timeout += 1`（**语义 = 判定解除的次数**，每次超时判定记 1；**不是**
  已执行条数）——仍放行，降级不永卡。
- **这里只搬队列、不执行**；tick 顶不持有任何对 `Run` 的调用。
- **与执行预算的关系（第 3 步 r6 P4）**：`pending_local` 与 `io.inbound_local_buffer` 在段内**共享
  同一个 `execute_max_effective` / `execute_wall`**；`pending_local` 按序号先消费、不会被饿死；每玩家
  自身条数上限 8（§3.1）。
- **推迟批次不在决策面内**：A6 不设 `overflow_batch` 容器——推迟批次已在 `pending_local`（§3.1）；
  决策只搬 `deferred_local`，tick 顶不触碰 `pending_local` 里的任何条目。

### 3.4 执行（段内，按到达序号合并）
`INBOUND_LOCAL` 段内消费：把 `pending_local` 的首项（`iter().next()`，最小 `seq`）与
`io.inbound_local_buffer` 队首（其 `seq` = 到达序号）**按序号比较、谁小先执行**；两侧各自内部按
`seq` 升序（`BTreeMap` 有序；buffer 按 fill 追加序）⇒ 双指针合并即**全局到达序**（"先 `pending_local`
再 buffer"是自然结果，不是独立规则）。被合并执行的条目走**与正常 LOCAL 命令同一执行路径**（不另造
分派）；执行时 `RAN_SEGMENTS |= INBOUND_LOCAL`、`HostInbound` 计时天然覆盖。**到达序号只在 fill 时
分配、每区线程单调递增，不跨线程比较**。
段内执行预算：普通条目每条占 1；溢出批次（就地执行那批）按 `batch.len()` 计入 `executed`（§3.1）；
`OverflowPostponed` 不算执行、只置 `budget_hit` 并收手。

## 4. 同收件人帧序：为什么保得住 + 可观测判据

**机制**：`@` 与本地命令同玩家到达序为 `@` 先；`@` 走 PROCESS（区 0，T+1），本地命令若不等就是 T。
stop-and-wait 让本地命令等在飞 `@`：`@` 在 T+1 执行、清位；延迟命令在 T+1（或 T+2 超时）放行 ⇒
**P 的结果帧序与 oracle 的"同玩家命令序"一致**（帧内容不保证同 tick，只保证**相对序**）。

**执行序（r3/r4）**：段内按**到达序号**执行（跨玩家亦然）；**不得**按 `chr_name` 排序或按玩家分组——
`chr_name` 序是 oracle 没有的新契约（观测/回溯会看到与 oracle 不同的跨玩家交错）。"清位 / 放行 /
溢出"三条路径只在各自条件满足时生效；**超时降级**属已知代价（§6 写死）。溢出批次是**原子单位**
（触发 tick 内全执行或整体推迟），不拆半批；推迟批次按普通有序条目参与合并（§3.1 边界）。

**可观测判据（不是"设计上应该"）**：
- **前提（r3）**：下列"相对序"判据**只在** `zonek_control_wait_timeout == 0` **且**
  `zonek_control_mailbox_rejected == 0` **且** `zonek_control_defer_overflow == 0` 时成立；任一 >0 的
  窗口**仍有效**（降级可见、不作废，见 §6），但**不得**用作相对序验收证据——超时/邮箱满/溢出都会
  **合法**造成跨类序偏离。
- **装置**：`mir2-rs/tests/parity` 的两区场景（`walkA`/`walkB` 同款运行器）+ 逐帧产物
  `frames.ndjson`；比较面 = **收件人序（per-recipient subsequence）**——把 `frames.ndjson` 按
  `(recipient socket)` 过滤，取 B 侧收到的帧序列，与 oracle 同脚本运行的 B 侧序列做**子序列比对**。
- **场景**：P（区 k）脚本 = `@1`（GM 命令，产生可观测帧/副作用）→ 立刻一条本地移动
  （产生 `SM_TURN`/`SM_RUN` 回显）。
- **判据**：B 侧帧序列中，`@` 产生的那一帧**先于**移动回显帧；与 oracle 同场景逐帧对照
  （相对序一致，绝对 tick 允许 ±1）。
- **仓内先验（单元级）**：`INBOUND_LOCAL` 段测试直接断言执行顺序列表（`[Process先, Local后]`），
  不依赖真实栈；上述两区运行是**窗口级确认**。

## 5. 负例（九条；每条都配"如何证明不是恒真"）

1. **`a6_no_inflight_executes_immediately`（无在飞不暂缓）**
   - 构造：`control_pending = 0`，向段内投一条 `Local`（move）；断言**本 tick 执行**、
     `deferred_local` 为空、`pending_local` 增量 0。
   - 非恒真：变异"无条件暂缓" ⇒ 本条红（执行被推迟到下一 tick）。
2. **`a6_timeout_releases`（超时必放行）**
   - 构造：`control_pending = 1`、`control_first_tick = Some(t0)`、`deferred_local` 放 1 条；模拟区 0
     不回（不清位）；tick 顶决策在 `tick - t0 == 1` 断言**不**放行、`== 2` 断言放行 +
     `zonek_control_wait_timeout == 1`，且 `control_pending == 0`、`control_first_tick == None`、
     `control_epoch` **+1**。
   - 非恒真：变异"永不超时" ⇒ 红；变异"`>= 1` 即超时" ⇒ T+1 不放行断言红。
3. **`a6_clear_resumes_immediately`（计数归零后立刻恢复）**
   - 构造：`control_pending = 1` + 延迟 1 条；模拟区 0 执行完**唯一一条**控制命令并清位（计数减到
     0、`first_tick = None`）；下一次 tick 顶决策须把该条搬入 `pending_local`，段内执行顺序 = 延迟条
     **先于**同 tick 新到的 `inbound_local_buffer` 条目（到达序号更小）。
   - 非恒真：变异"执行时忽略 `pending_local`（只跑新缓冲）" ⇒ 顺序断言红；变异"决策不搬队列" ⇒
     `pending_local` 增量为 0 ⇒ 红。
4. **`a6_mailbox_full_rejects_without_marking`（邮箱满不置位）**
   - 构造：邮箱填到 `CONTROL_MAILBOX_CAP`，再投一条 `Process`；断言：该命令**未入队**、
     `control_pending == 0`、`zonek_control_mailbox_rejected == 1`、`zonek_inbound_drained` **+1**
     （§6 口径：被消费并作出处置）、限速 WRN ≤1。
   - 非恒真：变异"删满判" ⇒ 入队成功、rejected 0 ⇒ 红；变异"先置位后入队" ⇒ `control_pending != 0` ⇒ 红。
5. **`a6_overflow_executes_in_place_in_arrival_order`（溢出整批就地执行、不动在飞）**
   - 构造：`control_pending = 1` + `deferred_local` 8 条 + 第 9 条到达，**预算充足** ⇒ 本 tick **9 条按
     到达序全部执行**（执行顺序列表断言）、`deferred_local` 空、`pending_local` 增量 0（装得下不搬）、
     **`control_pending` 仍 = 1、`control_first_tick` 不变**、`zonek_control_defer_overflow == 1`、
     `zonek_inbound_drained` +9。
   - 非恒真：变异"预算充足却整批推迟" ⇒ 本 tick 执行 0 / 顺序断言红；变异"丢新" ⇒ 执行 8 ⇒ 红；
     变异"溢出顺手清在飞计数" ⇒ `control_pending == 0` ⇒ 红。
6. **`a6_two_control_commands_hold_until_both_done`（同玩家多条 `@`）**
   - 构造：按序注入 `@`、`@`、move；每执行完一条控制命令只 `control_pending -= 1`（减到 0 才清
     `first_tick`）⇒ move 必须在**两条 `@` 都执行完之后**才执行；**r4：直接断言**
     `order == [`@1`, `@2`, `move`]`（不再只断放行时机）。
   - 非恒真：变异"只清最早一条"（首个 `@` 执行完就清 `first_tick`）⇒ move 被提前放行 ⇒ 红；
     变异"永不减" ⇒ move 永不执行（直到超时） ⇒ 红。
7. **`a6_timeout_then_stale_clear_is_ignored`（超时后的旧请求不污染新计数）**
   - 构造：超时清零（`epoch` +1）→ 新 `@` 入队（`control_pending = 1`、`first_tick = Some(t1)`）→
     **旧** `@` 执行完（其 `epoch` 不匹配）⇒ 断言 `control_pending` 仍 = 1、`first_tick` 仍 = `t1`、
     限速 WRN；再执行新 `@` ⇒ 计数归零、`first_tick = None`。
   - 非恒真：变异"无条件减" ⇒ 旧请求执行后 `control_pending == 0`、`first_tick == None` ⇒ 红。
8. **`a6_overflow_batch_occupies_budget_as_a_whole`（批次整体占预算）**
   - 构造：两名玩家各 8 条 deferred + 各 1 条新到（共 18），预算恰 18、墙钟充裕；另放 1 条第三项。
   - 断言：两批各 9 条**整批执行**（各按到达序）、`executed == 18`（**按 `batch.len()` 占预算**，
     不是 2）、`zonek_control_defer_overflow == 2`、`drained +18`、`pending_local` 空（装得下不搬）、
     第三项留缓冲且 `budget_hit`。
   - 非恒真：变异"按 1 条占预算" ⇒ 第三项被执行 / `order.len() != 18` ⇒ 红；变异"partial 执行" ⇒
     顺序/条数红。
9. **`a6_overflow_batch_postponed_when_budget_short`（装不下整批推迟）**
   - 构造：剩余预算 8 < 9；触发溢出。
   - 断言：本 tick **零执行**（`order` 空）、**整批 9 条按 `seq` 在 `pending_local`**、`deferred_local`
     空、`drained +0`、`budget_hit`；下一 tick 预算充足 ⇒ 9 条按 `seq` 全部执行（不丢、不越序）。
   - 非恒真：变异"partial 执行" ⇒ `order` 非空 ⇒ 红；变异"丢新" ⇒ `pending_local == 8` ⇒ 红。

**共同要求**：每条断言都含 ①**行为量**（执行/未执行、顺序列表、计数是否变化）与 ②**计数 delta**
（`zonek_control_*` / `zonek_inbound_drained`）；变异证据（删/改对应分支 ⇒ 本条红）作为"不是恒真"
的凭据，恢复后文件 sha 相等。

**并发前提（r4）**：以上负例是**确定性单线程**，**不覆盖** §2 置位的临界区窗口与守卫的并发时序；
该前提**必须写进代码注释**（`a6_enqueue_control` 与 `A6ControlGuard::drop` 附近）——评审逐条核注释
在不在（没有注释 = 未交付）。

## 6. 回滚、影响面与计数列

- **单提交 revert** 可整体回退（控制邮箱 + `SessionGround` 的 A6 子结构 + 两个延迟队列 + 置位/清位/
  决策/超时/计数 + 接缝元素的 `seq`）。
- **N=1 零参与**：区 0 不走本套（不构造控制邮箱、不加分类、不分配序号）；A6 全部在区 k 路径与
  `INBOUND_LOCAL` 段内；`zonek_control_*` 恒 0。
- **失败可观测面（A6 专属 4 列）**：`zonek_control_defer_overflow`（**已执行的溢出批次数**；推迟
  未执行的批次**不计**——实现口径）、`zonek_control_wait_timeout`（**判定解除的次数**）、
  `zonek_control_mailbox_rejected`（**邮箱满丢弃的条数**）、`zonek_control_shutdown_discarded`
  （**关服清点的 A6 残留条数**：`pending_local` + `deferred_local`——推迟批次已在 `pending_local`，
  天然覆盖）；**不共用**第 3 步 `pending_discarded`（那列非 0 会作废窗口，正常关服清 A6 队列不得
  把合格窗口判废）。每 tick 增量；TSV 同位置追加；**不设 `*_all_zones` 派生列**（区 0 无同名语义）。
  CAP 是**区 k 每区一份**（`Arc<Mutex<VecDeque>>` 每区一个实例），计数按**区 k 聚合求和**。⇒ 全部
  `zonek_*` 合计 = 第 3 步 6 列 + 本步 **4** 列 = **10 列**（派生列第 3 步 **5 个**——
  `inbound_carry_all_zones` 因混合分母已删，见第 3 步 r6 §4；本步 0 个）。
- **`zonek_inbound_drained` 口径（r4 覆盖第 3 步 r6 §4 的表述）**：= 段内**被消费并作出处置**的
  `Command` 条数——**含 `Unsupported` 归属、含邮箱满丢弃、含溢出就地执行**；**不是"成功执行条数"**，
  后续任何人不得用它当执行数。
- **作废口径（r4）**：`defer_overflow` / `mailbox_rejected` / `wait_timeout` / `shutdown_discarded`
  是**被观测到的降级/清理**，**不作废**该运行；**只有第 3 步的 `zonek_inbound_overflow_dropped` /
  `zonek_inbound_pending_discarded` 非 0 才作废**（该窗口作废 + 开缺陷卡，v2.1 判据）。
- **超时降级的已知代价（写死）**：超时后那些旧控制命令（`@`）**仍会被区 0 执行**，其效果可能落在
  **已放行**的本地命令之后——这是超时降级（`zonek_control_wait_timeout`）的**已知代价，不是缺陷**；
  该情形由 `zonek_control_wait_timeout` 计数可见。没有这句，将来对拍出现偶发跨类乱序时分不清缺陷与降级。
- **A6 是否还需要新的计数列（一句话结论）**：**需要 4 个**（均仅区 k、无派生），其余复用第 3 步的
  `zonek_inbound_*`。
- **边界（写死）**：A6 保证"同玩家跨类命令的相对帧序"，**但超时/邮箱满/溢出三种降级除外**（见上两条，
  由计数可见）；**不**保证同 tick 到达、不改帧内容；GM `@` 的副作用仍在区 0 世界/`saves`（跨区副作用
  不在本步范围，另卡）。**溢出批次原子性只在触发 tick 内保证**：推迟批次按普通有序条目参与后续合并
  （跨 tick 可能分批完成，见 §3.1）；溢出造成的越序**只影响该玩家**，不改变其他玩家之间的到达序。

## 7. 交付时报告什么
- 改动文件与 `+/-`；控制邮箱与两个延迟队列的落点（`SessionGround` 的 A6 子结构、`HostLoop.pending_local`、
  区 0 drain 位置）、接缝元素带 `seq` 的落点（与第 3 步实现的差异清单）；
- **九条**负例的原文输出（含顺序列表与计数 delta）+ 变异证据（各自红、恢复后 sha 相等）；
- 两区场景（P 发 `@`+move、收件人 B）的 `frames.ndjson` 逐帧对照（相对序一致、绝对 tick ±1），并声明
  该窗口三个 A6 降级计数均为 0（否则只作功能证据、不作顺序证据）；
- N=1 零参与证据（`zonek_control_*` 恒 0、区 0 不构造控制邮箱）；
- **10 列**清单（`zonek_inbound_*` 6 + `zonek_control_*` 4）与 `drained` 覆盖口径的证据（邮箱满丢弃
  仍计 drained）；
- **并发前提的代码注释落点**（`a6_enqueue_control` / 守卫 `Drop` 附近）；
- 溢出批次的预算占用证据（测试 8：`executed == 18`）与推迟路径证据（测试 9）。
