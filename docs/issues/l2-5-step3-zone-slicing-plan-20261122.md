# L2-5 第③步 实施计划：每区 session/host 切片（会话与宿主真正切到各区）

> 状态：**计划页（只读 + 文档；不写码、不构建）**。基线 master `35c735724`。
> 前序：`docs/issues/l2-5-zone-ownership-plan-20261122.md` §2（登录落区）、§4（handoff）、§7
> （收件人区发帧口径）；已合入 ①②步（每区 `ActorId` 段、镜像 + 跨区可见、`EmitterCriterion`
> 双口径、AOI 离场标记）。本页只回答"切什么/怎么切/哪里会破/怎么验"，不含实现。

## 0. 一句话

把 `sim_loop` 里**全图一份**的会话与宿主状态按区切开：每区有自己的 `World` / `DisplayTable` /
`OnlineRegistry` / `ground_sessions`；登录按坐标落到目标区；跨界移动走两阶段 handoff；出现/消失帧
仍由**收件人区**发出（§7 口径不变）。

## 1. 切什么（逐项：切 / 不切 / 暂不切）

| 状态 | 现状（`sim_loop` 内） | 第③步 | 理由 |
| --- | --- | --- | --- |
| `World`（含地图网格） | 区 0 一份真 `World`；非 0 区是 stub | **切** | 实体必须落在 owner 区；v1 同图多区每区复制地图网格（L2-4 附记），含城堡地图不进分区布局（启动门） |
| `displays`（`downlink::DisplayTable`） | 全局一份 | **切** | 镜像展示项要进**收件人区**的展示表（②已按此写 `apply_mirrors`） |
| `online`（`OnlineRegistry`） | 全局一份 | **切** | 每区只需本区会话的 `(line,socket)`；`find_linear_chr` 只查本地，跨区收件人靠镜像 `route`（见 §3） |
| `ground_sessions: HashMap<String, SessionGround>` | 全局一份 | **切** | 会话态随 handoff 迁移；目标区在 tick 顶 `insert` |
| `saves: HashMap<String, SavedHuman>` | 全局一份 | **切**（区 0 汇总） | 存档缓冲跟会话走；关服由区 0 汇总（L2-5 §6、第⑥步收口） |
| `run_frames`（`RunFrames`） | 每 tick 一份 | **切** | 纯帧缓冲，按 tick/区一份 |
| `gates: HashMap<u8, UnboundedSender<SimToGate>>` | 每线一条 | **不切** | demux 已 `Broadcast` 给每区（6a） |
| `registry`（`MapRegistry`） | 启动期装载 | **不切** | 只读，`Arc` 共享 |
| `enter_cfg`（`EnterConfig`） | 启动期消费 | **不切** | 构造期一份；装载产物按区分发 |
| `db`（`DbWiring`） | 区 0 持有 | **不切**（共享链路） | `req_tx` 可 clone；非 0 区经共享链路发 `DB_SAVEHUMANRCD` |
| `ids_online: Arc<AtomicI32>` | 进程级 | **不切** | 在线数是进程读，区 0 写（P2-3b 决定 A） |
| `mon_gen_host` + `step_mon_gen` | 区 0 全局驱动 | **暂不切**（第④步） | L2-5 §1 A2：MonGen 的 RNG 消耗序列必须与单区逐抽相同；按区分头跑会让 RNG 流按区切分。第③步保持全局决策，spawn 目的地在第④步按坐标投递 |
| `world.rng`（随机流） | 一条流 | **切（每区一条）** | 切片后非 0 区有玩家就有战斗/AI ⇒ 各 `World` 各带一条流，抽数序列从第③步起按区交错（**与 MonGen 切不切无关**）。**每区 seed 用 `seed ⊕ zone_id` 派生**——禁止各区同 seed（两区随机数相关会出现「同一次暴击同时发生」）。RNG 逐抽等价留到第④步全局产生+分发 |
| 城堡/行会服务（`castle::initialize_castle`、`guild_runtime`、城堡实体） | 区 0 | **暂不切** | 进程级服务 + 城堡地图不进分区（启动门）；跨区事件留第⑥步 |
| `world_start` / `shutdown` / `win_lottery` / `planes` | 进程级 | **不切** | 与区无关 |
| `ZoneRouter` / `TickBarriers` / `HaloExchange` / `ZoneWatch` | 已有 | **不切** | ①已就位；登录落区时 `ZoneRouter::assign`，handoff 时更新 |

**每区所需的"启动期装载切片"**（ctor fold / 商人 / NPC / 守卫）属第④步；第③步的分区世界里，
非 0 区的静态实体**暂时为空**（玩家登录落区后只看到镜像边界带 + 本区会话），这是**中间态**，
验收口径按第③步的"会话切分 + 帧归属"立，不按"静态装载完整"立。

## 2. 登录落区与跨界移动 handoff（落到函数）

### 2.1 登录落区

1. `route_zone_target` 已把 `LoadDbRequest` 投区 0（6a）；
2. 区 0 组装 `LoadedHuman` 后调 `RegionLayout::zone_of(saved_map, x, y)`：
   - `Some(0)` ⇒ 原地 `enter_map_at`；
   - `Some(k≠0)` ⇒ 投目标区入场队列（新 `ZoneInbound::EnterZone { loaded, session }`）；区 k 在
     tick 顶 `open_session` + `enter_map_at`，回执后**区 0** 才 `ZoneRouter::assign(line, socket, k)`
     （回执先于 assign，避免"事件已按区 k 分派、实体还没进 World"）；
   - `None` ⇒ 拒登 + `zone_login_uncovered` 计数（fail-closed，**不回落区 0**）。
3. 会话/存档缓冲随入场迁移：`ground_sessions` 在目标区 `insert`，区 0 删除；`saves` 同理（或统一
   留在区 0，见 §1 的"区 0 汇总"选型）。

### 2.2 Handoff（两阶段、源从不放手）

- **检测**（源区，`World::try_step_blocked_impl`）：目标格不在本区 rect（`RegionLayout::owns`）⇒
  实体**移到目标格**（已在邻区 rect 内，仍在本区 World 中）、置 `HandoffPending`（后续输入拒绝）、
  **仍归源区所有**；源区在 tick T 继续模拟它；
- **目标区**：tick 顶排水 `HandoffOffer { spawn_cloned, session, at_tick }` → `RegionLayout::require_owned`
  校验 → **原地提升镜像**（见 §2.3；不新建第二条、不先删后插）→ 挂会话/在线表/展示表 → `HandoffAck`；
  校验失败/队列满 ⇒ `HandoffReject`；
- **提交/回滚**（源区）：收到 `Ack` 才移除本地实体（`World::leave_world*`）；`Reject`/超时 ⇒
  **不提交**，实体退回原格、移动被拒、`handoff_refused += 1`——原子性来自"所有权只在 Ack 后切换，
  源侧从未放手"；
- **时序**：offer 在 T 的 barrier2 之后发出；目标区的提升与源区的移除都在 **T+1 的 tick 顶、phase1
  之前**完成（两侧屏障同 tick 号 ⇒ 同一 tick 不会双区同时模拟）；
- **会话迁移**：`ground_sessions` 随会话移动；`OnlineRegistry`/`ZoneRouter` 在目标区更新（route 不变）；
- **离场标记**：源区提交时实体被删 ⇒ 源区走离场标记（`ghost`/`fixed_hide` 按删除瞬间）；目标区是
  既有镜像的**原地提升**（不是新增、不记标记）。

### 2.3 帧原子性（零闪烁；顺序写死，不留实现猜测）

**核心口径（Leader 裁决）**：跨界全程该 id **不产生任何 appear/disappear**——源区与目标区观察者
都连续可见；**不接受**"目标区闪 1 tick"。实现靠两件事：**outgoing 带**（T 时源区仍发布该实体）
与**镜像原地提升**（T+1 目标区把同 id 镜像升成真身）。

**快照字段（先定后写码）**：`HaloEntity` 增 `outgoing: bool`。源区快照的过滤从
"rect 内"扩为：

```text
rect.contains(x, y)
|| (outgoing 条件：该实体 HandoffPending
    && 本区仍拥有它
    && 距本区 rect 的边界 ≤ HALO 宽度)
```

`outgoing` 只用于观测/诊断；目标区把它当普通镜像（落同一格、同一 id）。

**T/T+1 的精确顺序**：

```text
TICK T（源区）：
  phase1 的移动判定：目标格不在本区 rect ⇒ 实体移到目标格（邻区 rect 内）、置 HandoffPending；
  源区快照 = rect 内 + outgoing 带 ⇒ 本 tick 快照**仍含该 id**（outgoing=true）
  → barrier1 → 读邻居 → phase2（源区 AOI 按坐标仍可见）→ barrier2 → 发 HandoffOffer

TICK T+1（目标区，tick 顶、phase1 之前）：
  排水 Offer → require_owned 校验 → **原地提升镜像**：
  用 Offer 的全量状态覆盖该实体、清 `Flags.mirror`、挂会话/在线表/展示表（展示项以会话为准）
  ⇒ 不新建实体、不先删后插（先删后插会产生一次假消失）
  → HandoffAck

TICK T+1（源区，tick 顶、phase1 之前，收到 Ack 之后）：
  World::leave_world*（源区移除；离场标记按删除瞬间记下）

TICK T+1（两区 phase1）：源区快照不再含该 id；目标区快照含真实体（在 rect 内）
TICK T+1（barrier1 之后 apply_mirrors）：
  源区从目标区快照得到该 id 的**镜像**（同一格）⇒ 源区观察者继续可见（连续）；
  目标区陈旧清扫只看 `mirror_ids()`，该 id 已不是镜像 ⇒ 不会误删真实体
TICK T+1（phase2）：两区扫描：该 id 在两侧世界都连续存在 ⇒ 两侧都没有 appear/disappear
```

**必带要求（不是可选项）**：

1. **outgoing 带**：T 时源区快照必须仍发布该实体（否则目标区 T 就把镜像摘掉 ⇒ 目标区 T 消失、
   T+1 出现，正是要消灭的一帧缺陷）。
2. **镜像原地提升**：目标区插入真身时，若本区已有**同 id 镜像**（全局 id 相同）⇒ 必须
   **原地提升**（`mirror=false` + 全量状态覆盖 + 挂会话），**不许**新建第二条、**不许**先删后插。
   计数 `handoff_mirror_promoted`，用例覆盖"提升后不产生假离开"。
3. **兜底（防御，不是牙；Leader 2026-11-22 裁决）**：`apply_mirrors` 把「镜像 id 已是本区**真实**
   实体」视为 **handoff 收敛**——**跳过该条 + 计数 `mirror_shadowed_by_local`，不许 abort**。
   兜底分支**不得** `remove_mirror`、不得改动本地真实体（只跳过 + 计数）。**时序正确时该计数恒 0**：
   它是「时序设计被破了」的读数，**不是牙**；不许用人工变异把它当牙宣传；>0 要查时序。
4. **时序（设计）**：源区移除必须早于**源区自己的** T+1 phase1；目标区提升必须早于**目标区自己的**
   T+1 phase1。

**验收用例（帧原子性）**：跨界全程**该 id 在源区与目标区的每收件人有序帧流中都不得出现
appear/disappear**（不产生任何 `SM_TURN`/`SM_FEATURECHANGED`/`SM_DEATH`/`SM_SKELETON` 的
新增或消失，也不出现"闪一下又回来"）；逐 tick `VisibleActors` 也必须连续。legacy 与增量两条 AOI
路径（`MIR2_AOI_INCREMENTAL=0/1` 两个进程级窗口）各验一次。

## 3. "收件人区发帧"（§7 `RecipientZone`）在切片后是否仍成立

**成立的部分**：AOI 出现/消失帧由收件人区从**本区 World**（本地实体 + 镜像）发出；
`drain_aoi_downlink` 的观察者是本区会话（`online.find_linear_chr` 本地命中）⇒ 账本 `(tick, owner,
recipient, kind)` 的"唯一发射者 = 收件人区"直接成立。

**会破的地方（跨区收件人帧）**：

1. `send_ref_msg_targets_at`（world-bevy，`send_ref_msg_targets_at_impl`）：按**本区**格扫
   ±`SEND_REF_MSG_RANGE` 找收件人 ⇒ 收件人在邻区时漏发。修法：镜像快照已带
   `HaloEntity::route(line, socket)`（①步字段，当前无消费者）⇒ **事件经跨区事件队列送到收件人区，
   由收件人区用镜像 route 合成收件人**（帧仍由收件人区发）。
2. `flush_run_frames` / `drain_run_downlinks_with_magic`：会话顺序 = 本区顺序；跨区收件人须在目标区
   按同一收件人出帧；账本去重键保证不会两处发。
3. NPC/商人 `drain_npc_merchant_*` 的距离广播、`broadcast_feature_changed`、`SM_MOUNTRIDE` 广播：
   同样按本区 online 距离 ⇒ 需要镜像 route 合成。
4. **`ZoneRouter` 漏 assign 的破法**：未 assign 的 `(line,socket)` 会回落区 0，账本 `dst` 记错区
   ⇒ 表现为 `never`/`dup` 或帧发错区。登录落区（2.1）与 handoff（2.2）都必须 assign，并加断言/计数
   `zone_router_unassigned`。

**第③步的边界**：先把"收件人全在本区"的路径切干净；**跨区收件人帧（RefMsg/NPC 广播等）列为本步
已知缺口**，计数 `cross_recipient_missing`，在第④/⑤步按镜像 route 合成补齐。**不以"拒绝"作默认**。

**硬约束（Leader 裁决，写死）**：
- （i）第③步可以交容量数字，但**任何容量报告必须显式标注「跨区收件人帧未合入」**；
- （ii）**任何面向 Owner 的"1 万在线验收通过"结论，必须在该计数为 0 之后才允许下**——否则就是
  拿缺帧的栈去报容量。

## 4. 容量验收口径（1 万在线、单图）

### 4.1 场景

- N 区 = 2 / 4 / 8（同一张图按矩形切）；`MIR2_ZONES` 起栈，`botsrv` 驱动；
- 会话档：1k → 5k → 10k 渐变（每档 ≥ 3 次重复），10k 稳态 ≥ 30min；
- 附加压力：登录风暴（1k/10s）、跨界行走（边界带集中，handoff 压测）、关服存档。

### 4.2 指标（全部落 TSV/`perf_counters`/`TICKPROF`）

- **主指标（Leader 2026-11-22 裁决：不用"p99 ≤ 200ms"——与 tick 周期同值是循环论证）**：
  **有效 tick 率 5/s（无跳 tick/漂移）+ p50 ≤ 100ms（留 100ms 余量）**；p99/p999 只作分布参考；
  `TICKPROF` 分段（`HostPlayerRun` / `HostWorldTail` / `HostAoi*` 等）；
- **积压判据（与主指标并列）**：网关队列深度（`gate_queue`）、`hit_budget`/`inbound_carry`、
  DB 请求滞留；**阶梯加压找拐点**（拐点 = 有效 tick 率开始掉、或积压开始单调上升的档位）；
- **区维度**：`region_barrier_us` 的 max（最慢区决定墙钟）与 total（对账）；每区实体数/镜像数
  （受 `MIR2_MAX_HALO_BAND_ENTITIES` 约束）；`mirror_cell_pos_unknown` / `mirror_ai_actions` /
  `cross_hit_delayed`（应恒 0 / 0 / 按注入）；
- **正确性**：outbox `sent==recv`、`dup=0`、`never=0`；`handoff_offered/accepted/refused`；
  `cross_recipient_missing`（第③步已知缺口计数；**容量报告必须标注「跨区收件人帧未合入」；
  面向 Owner 的"1 万在线验收通过"结论必须等该计数为 0**）；`departure_mark_held_past_cap` 恒 0；
  `mirror_shadowed_by_local` 恒 0（handoff 收敛兜底；恒 0 才说明时序设计成立，**不是牙**）；
- **资源**：进程 RSS 与每区线程 CPU；登录成功率、登录→首帧时延；
- **稳定性（改成计数斜率，不用"无增长"这种不可判句）**：`mirror_ids` 数、`DepartureMarks` 数、
  `process_run_counts`、各 `pending` 表——**后 20min 斜率 ≈ 前 20min 斜率**（稳态），并报
  RSS 前/后 20min 斜率对比；看门狗 abort / `Barrier` 超时次数必须为 0。

### 4.3 窗口与时长

- 真栈（容量测只跑 Rust 栈，不与 C# 并行）+ `botsrv`；10k 会话前先确认网关/DB 连接数与端口容量；
- 每档 ≥ 3 次、报中位数与 p99（`test-count-triple` 同款口径：跑数/通过/失败分开写）；
  **ramp（含登录风暴尖峰）与稳态的 p99 分母分开报**——尖峰不许被 30min 稳态稀释；
- 与 Owner 对表的验收句（Leader 改法）：**单图 1 万在线、有效 tick 率 5/s（无跳 tick/漂移）、
  p50 ≤ 100ms 留余量、无积压单调上升（`gate_queue`/`hit_budget`/`inbound_carry`/DB 滞留）、
  「本区收件人」无丢失/重复帧**；**跨区收件人帧记 `cross_recipient_missing`（已知缺口，第④/⑤步
  补；`bf` 粗估：边界带集中战斗时可丢到约 50%）⇒ 容量数字可报，但**"1 万在线验收通过"结论必须等
  该计数为 0**；
- **关服收敛面必须定义**（60s 是判据不是口号）：①每区存档请求 ack 全到（`DB_SAVEHUMANRCD` 回执
  计数 == 请求数）；②各区入站/出站队列为空且无在飞 handoff；③进程在 60s 内正常退出。任一项
  未满足 ⇒ 记 **FAIL**（不以 kill 冒充通过）。

## 5. 第③步不做（登记）

- **RNG（本步接受 per-zone 独立流）**：每区 `World.rng` 用 `seed ⊕ zone_id` 派生（禁止同 seed）；
  抽数序列从第③步起按区交错是**预期行为**；RNG 逐抽等价（与单区基线）留到第④步全局产生+分发；
- MonGen 全局产生 + 分发（第④步）；
- 静态装载切片（ctor fold/商人/NPC/守卫，第④步）；
- 跨区收件人帧合成（镜像 route 消费，第④/⑤步）；
- 存档按区聚合与关服收口（第⑥步）；
- 城堡地图参与分区（启动门继续排除）。

## 6. 风险与验收边界

- **最大风险 = 屏障活性**：任区 panic/超时 ⇒ 已有失败快传（`TickBarriers::abort`）+ 看门狗 5s abort；
  容量窗口必须记录 abort 次数（应为 0）。
- **正确性判据**：对**不消耗 RNG** 的子场景仍按"与单区基线逐帧对拍"立（L2-5 §3 的 A6/A8 口径）；
  **含随机（战斗/AI）的场景在 RNG 分发落地前不作为逐帧判据**（可固定种子做回归）；容量窗口
  不替代对拍。
- 本步交付的是**会话与宿主切片**；"容量"结论只对 4.2 的指标与窗口负责。
