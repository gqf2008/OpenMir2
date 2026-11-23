# M1b 第 4 步实施规格：A6 同玩家跨类顺序（stop-and-wait，2026-11-23）

> 父 `7e54d4f9b`。上游依据：`l25-m1b-plan-20261123.md` §3（A6 v2.1）与 §5.2/§5.3（逐点位 +
> 决策/执行分离）；**依赖第 3 步规格** `l25-m1b-3-zone-drain-budget-spec-20261123.md` 的 **r5**（r4+r5：
> `TickIo.inbound_local_buffer` / `inbound_local_carry_prefix`、`pending_inbound` scratch、P1 swap 不变式、
> P4 `pending_local` 共享执行预算、P5 单 tick ≤2000、**6 个** `zonek_inbound_*` 列）。本文件只定实施规格、
> 不写码；引用以**符号锚**为准。

## 0. 一句话

同一玩家 P 先发 GM `@`（PROCESS，回区 0 执行、T+1）再发本地命令（LOCAL，T）时，**P 的本地命令
暂缓到 `@` 执行完再放行** ⇒ 保住"同收件人帧相对序"。窗口内本地命令最多延迟 1 tick；`@` 低频。

## 1. 依赖面（第 2/3 步的符号；命名不符只报差异清单、不改本稿）

**来自第 3 步 r4**：
- `pending_inbound: VecDeque<GateEvent>`（区 k 线程循环**局部 scratch**，锁外 fill）；
- `TickIo.inbound_local_buffer: VecDeque<GateEvent>` + `TickIo.inbound_local_carry_prefix: usize`
  （段的工作区，锁外交接、tick 后 swap 回）；段内消费顺序 **先 `pending_local` → 再
  `io.inbound_local_buffer`**（r4 §3.3，原句引用，不另立顺序）；
- 计数（第 3 步 **r6** 仍是 **6 列**）：`zonek_inbound_drained/carry/budget_hit/overflow_dropped/
  pending_discarded/unsupported`（**`unsupported` = 分类归属计数**，第 3 步不丢；第 5 步接管丢弃时
  另加 `zonek_inbound_unsupported_dropped`）；**A6 自身新增 2 列**（`zonek_control_defer_overflow` /
  `zonek_control_wait_timeout`，见 §6 末结论；若实施后实际列名不同，只报差异清单）。

**来自第 2 步（分类器）**：
- `zone_cmd_class(ident, payload) -> ZoneCmdClass`，`ZoneCmdClass::{Local, Process, Unsupported}`；
  `Process` = `CM_SAY` 且 payload 以 `@` 起（复用 `gm_drain_gate` 的严格 `starts_with('@')`）。
  **若第 2 步落地后类名/函数名不同（如 `InboundOwnership::Process`），只报差异清单，不改本稿。**

**来自第 1 步/既有代码**：`INBOUND_LOCAL` 段位与 `RAN_SEGMENTS`；`SessionGround`（`ground_sessions`
的键 = `chr_name`）；区 0 的 GM 执行入口（`handle_say_with_saves` → `handle_gm_command` 的调用链）。

## 2. `control_in_flight` 的键与两个点位（只有区 k 用）

- **键 = `chr_name: String`**（不是 actor id、不是 socket）：区 k 的排水/分派天然粒度就是
  `(chr_name, socket, cmd)`；A6 的状态放在 `SessionGround`（`ground_sessions: HashMap<String, SessionGround>`）。
- **字段**：`SessionGround.control_in_flight: Option<u64>`（值 = 置位时的 tick；`None` = 无在飞）。
  构造点（`session_ground_from_login` + 测试构造）补默认 `None`。
- **置位（区 k，`INBOUND_LOCAL` 段内、分类之后）**：分类结果为 `Process` ⇒ ①把 `(chr_name, line,
  local_tick)` 推入**控制邮箱**；②**入队成功后**置 `sg.control_in_flight = Some(tick)`
  （值为**段内 tick**——见 §2.1）。
  - 位置描述（可验证）：在 `tick_phase_a` 的 `INBOUND_LOCAL` 分支内、`zone_cmd_class(...)`
    返回 `Process` 的那个 `match` arm 里，紧随"控制邮箱 `push` 成功"的语句之后；同级异常/满
    处理不得跳过置位（满 ⇒ 丢弃该命令 + WRN + 计数，且**不置位**）。
- **清位（区 0，执行完 P 的 GM 命令时）**：区 0 的**控制邮箱 drain**（M1b 新增，紧随区 0 的
  `INBOUND` 段/`HostInbound` 之后）逐条取出请求：
  1. 先**持区 k 的 `state` 锁**（form (d)：形状同既有 `finish_login_on_zone_state` /
     `handle_zone_closed` 的"持区 k 锁 + `&mut` 区 k `SessionGround`"）；
  2. 用区 0 的 GM 上下文执行该 `@` 命令（`handle_say_with_saves` 同一入口，`chr_name` = P）；
  3. **执行返回后、释放锁之前**：`sg.control_in_flight = None`；同时把 `sg` 上的**就绪延迟队列**
     标记为可放行（§3.3 的 tick 顶决策会在本 tick 或下一 tick 生效——由 tick 顶统一 `mem::take`，
     清位处不直接执行，见 §3.4）。
- **控制邮箱形态**：M1b 新增；建议 `Arc<Mutex<VecDeque<ControlRequest>>>`（一次规划处创建，区 k
  持 clone 推入、区 0 每 tick drain）——**不新增 `state` 锁次数**（推入不持区锁）；区 0 执行时才持
  区 k 锁。`ControlRequest = { chr_name: String, line: String, at_tick: u64 }`。
- **N=1**：不构造控制邮箱、不走本套（区 0 直执行） ⇒ 零参与。

### 2.1 tick 源（写死）
置位/暂缓/超时**全部在 `INBOUND_LOCAL` 段内**完成（段在 `zone_tick_top(tick)` 之后、同一把
`state` 锁内）⇒ 段内 `self.ticks` 与驱动循环的本地 `tick` **同值**（`zone_tick_top` 在 A 段前写）。
**禁止**在 `zone_tick_top` 之前的位置读 `st.host.ticks`（那是上一 tick）。段外用本地 `tick`；
段内用 `self.ticks`；二者同值，不存在跨侧比较。

## 3. 延迟队列：`deferred_local` / `pending_local`

### 3.1 容器与上限
- `deferred_local: VecDeque<PendingLocalCmd>`、`pending_local: VecDeque<PendingLocalCmd>`，
  `PendingLocalCmd = { ident: u16, payload: Vec<u8>, at_tick: u64 }`；均为**每玩家**（放 `SessionGround`）。
- **上限 N = 8**：`deferred_local.len() == 8` 再来 ⇒ **强制解除**（`control_in_flight = None`）+
  WRN + `zonek_control_defer_overflow += 1`，然后**按到达序连同新命令**把它们**放行**（不丢命令，
  宁可越序）。此后该玩家恢复无在飞状态。

### 3.2 暂缓（段内分类处）
在 `INBOUND_LOCAL` 段内逐事件分类时：
- `Process` ⇒ 控制邮箱 + 置位（§2）；
- `Local` 且 `control_in_flight.is_some()` ⇒ **不执行**，`deferred_local.push_back({ident, payload,
  at_tick: 段内 tick})`；
- `Local` 且 `None` ⇒ 就地执行（正常路径）；
- `Unsupported` ⇒ **第 3 步**：`zone_cmd_class` 归属计 `zonek_inbound_unsupported`、**仍交
  `handle_zone_command`**（handler 侧 `zone_cmd_unsupported` 照计，行为不变）；**第 5 步接管丢弃**
  （届时本行改为「丢弃并计数、不进 `zonek_inbound_drained`」，并新增
  `zonek_inbound_unsupported_dropped` 列）；**A6（第 4 步）不涉及本机制。**

### 3.3 决策（tick 顶、锁内、**不执行**）
区 k 线程循环顶（`fill_pending` 之后、锁 1 之前）用**一次短锁**做决策（与现有 `closed/cmd` 的
锁一次同形）：
- 对每个 `deferred_local` 非空的玩家：若 `control_in_flight == None` **或** 超时
  （**`tick - at_tick >= 2`**，tick = 循环本地 `tick`）⇒ `pending_local.extend(mem::take(deferred_local))`；
  超时另清标记 + WRN + `zonek_control_wait_timeout += 1`（仍放行，降级不永卡）。
- **这里只搬队列、不执行**；tick 顶不持有任何对 `Run` 的调用。
- **与执行预算的关系（第 3 步 r5 P4）**：`pending_local` 与 `io.inbound_local_buffer` 在段内**共享
  同一个 `execute_max_effective` / `execute_wall`**；`pending_local` 先消费、不会被饿死，其自身条数上限
  为 8（§3.1）。

### 3.4 执行（段内，顺序与第 3 步同一句）
`INBOUND_LOCAL` 段内消费顺序（**引用 r4 §3.3，不另立**）：
**先 `pending_local` → 再 `io.inbound_local_buffer`**。`pending_local` 的条目走**与正常 LOCAL 命令
同一执行路径**（不另造分派）；执行时 `RAN_SEGMENTS |= INBOUND_LOCAL`、`HostInbound` 计时天然覆盖。

## 4. 同收件人帧序：为什么保得住 + 可观测判据

**机制**：`@` 与本地命令同玩家到达序为 `@` 先；`@` 走 PROCESS（区 0，T+1），本地命令若不等就是 T。
stop-and-wait 让本地命令等在飞 `@`：`@` 在 T+1 执行、清位；延迟命令在 T+1（或 T+2 超时）放行 ⇒
**P 的结果帧序与 oracle 的"同玩家命令序"一致**（帧内容不保证同 tick，只保证**相对序**）。

**可观测判据（不是"设计上应该"）**：
- **装置**：`mir2-rs/tests/parity` 的两区场景（`walkA`/`walkB` 同款运行器）+ 逐帧产物
  `frames.ndjson`；比较面 = **收件人序（per-recipient subsequence）**——把 `frames.ndjson` 按
  `(recipient socket)` 过滤，取 B 侧收到的帧序列，与 oracle 同脚本运行的 B 侧序列做**子序列比对**。
- **场景**：P（区 k）脚本 = `@1`（GM 命令，产生可观测帧/副作用）→ 立刻一条本地移动
  （产生 `SM_TURN`/`SM_RUN` 回显）。
- **判据**：B 侧帧序列中，`@` 产生的那一帧**先于**移动回显帧；与 oracle 同场景逐帧对照
  （相对序一致，绝对 tick 允许 ±1）。
- **仓内先验（单元级）**：`INBOUND_LOCAL` 段测试直接断言执行顺序列表（`[Process先, Local后]`），
  不依赖真实栈；上述两区运行是**窗口级确认**。

## 5. 负例（三条；每条都配"如何证明不是恒真"）

1. **`a6_no_inflight_executes_immediately`（无在飞不暂缓）**
   - 构造：`control_in_flight=None`，向段内投一条 `Local`（move）；断言**本 tick 执行**、
     `deferred_local` 为空、`pending_local` 增量 0。
   - 非恒真：变异"无条件暂缓" ⇒ 本条红（执行被推迟到下一 tick）。
2. **`a6_timeout_releases`（超时必放行）**
   - 构造：置位 `Some(t0)`、`deferred_local` 放 1 条；模拟区 0 不回（不调清位）；tick 顶决策在
     `tick - t0 == 1` 断言**不**放行、`== 2` 断言放行 + `zonek_control_wait_timeout == 1`。
   - 非恒真：变异"永不超时" ⇒ 本条红；变异"`>= 1` 即超时" ⇒ 第一条（T+1 不放行）红。
3. **`a6_clear_resumes_immediately`（清位后立刻恢复）**
   - 构造：置位 + 延迟 1 条；随后模拟区 0 执行完并清位（`None`）；下一次 tick 顶决策须把该条搬入
     `pending_local`，段内执行顺序 = 延迟条**先于**同 tick 新到的 `inbound_local_buffer` 条目。
   - 非恒真：变异"执行时忽略 `pending_local`（只跑新缓冲）" ⇒ 顺序断言红；变异"决策不搬队列" ⇒
     `pending_local` 增量为 0 ⇒ 红。

**共同要求**：每条断言都含 ①**行为量**（执行/未执行、顺序列表）与 ②**计数 delta**
（`zonek_control_*`/`zonek_inbound_drained`）；变异证据（删/改对应分支 ⇒ 本条红）作为"不是恒真"
的凭据，恢复后文件 sha 相等。

## 6. 回滚、影响面与计数列

- **单提交 revert** 可整体回退（控制邮箱 + 两个 `deferred` 队列 + 置位/清位/决策/超时/计数）。
- **N=1 零参与**：区 0 不走本套（不构造控制邮箱、不加分类）；A6 全部在区 k 路径与
  `INBOUND_LOCAL` 段内；`zonek_control_*` 恒 0。
- **失败可观测面（新列）**：`zonek_control_defer_overflow`、`zonek_control_wait_timeout`
  （每 tick 增量；TSV 同位置追加；**不设 `*_all_zones` 派生列**——区 0 无同名语义，派生只有在区 0
  也接 A6 时才有意义）。⇒ 全部 `zonek_*` 合计 = 第 3 步 6 列 + 本步 2 列 = **8 列**（派生列第 3 步
  **5 个**——`inbound_carry_all_zones` 因混合分母已删，见第 3 步 r6 §4；本步 0 个）。
- **A6 是否还需要新的计数列（一句话结论）**：**需要 2 个**（`zonek_control_defer_overflow` /
  `zonek_control_wait_timeout`，均仅区 k、无派生），其余复用第 3 步的 `zonek_inbound_*`。
- **边界（写死）**：A6 只保证"同玩家跨类命令的相对帧序"；**不**保证同 tick 到达、不改帧内容；
  GM `@` 的副作用仍在区 0 世界/`saves`（跨区副作用不在本步范围，另卡）。

## 7. 交付时报告什么
- 改动文件与 `+/-`；控制邮箱与两个队列的落点（`SessionGround` 字段、区 0 drain 位置）；
- 三条负例的原文输出（含顺序列表与计数 delta）+ 变异证据（各自红、恢复后 sha 相等）；
- 两区场景（P 发 `@`+move、收件人 B）的 `frames.ndjson` 逐帧对照（相对序一致、绝对 tick ±1）；
- N=1 零参与证据（`zonek_control_*` 恒 0、区 0 不构造控制邮箱）。
