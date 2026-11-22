# L2-5 第③步 实施计划：每区 session/host 切片（会话与宿主真正切到各区）

> 状态：**计划页（只读 + 文档；不写码、不构建）**。基线 master `35c735724`。
> 前序：`docs/issues/l2-5-zone-ownership-plan-20261122.md` §2（登录落区）、§4（handoff）、§7
> （收件人区发帧口径）；已合入 ①②步（每区 `ActorId` 段、镜像 + 跨区可见、`EmitterCriterion`
> 双口径、AOI 离场标记）。本页只回答"切什么/怎么切/哪里会破/怎么验"，不含实现。

## 0. 一句话

把 `sim_loop` 里**全图一份**的会话与宿主状态按区切开：每区有自己的 `World` / `DisplayTable` /
`OnlineRegistry` / `ground_sessions`；登录按坐标落到目标区；跨界移动走两阶段 handoff；出现/消失帧
仍由**收件人区**发出（§7 口径不变）。

### 0.1 开关语义（Leader 裁决：新开关，与 `MIR2_ZONES` 解耦）

切片**不**绑到 `MIR2_ZONES` 上——现有全部多区用例（`l2_two_zone`、②的 `step2_*`、L2-4 的
`MIR2_ZONES` 结构态）都是「N≥2 但玩家仍在区 0」，不能因为切片未实现就判启动失败。

- **`MIR2_ZONE_SLICING`**（`0`/未设 = 关；`1` = 开），三条全部 fail-closed 并点名变量：
  1. `=1` 且 `MIR2_ZONES` 未设或 <2 区 ⇒ 拒绝启动；
  2. `=1` 且 `MIR2_ZONE_MIRROR≠1` ⇒ 拒绝启动（零闪烁与跨区可见都依赖镜像）；
  3. 未设 ⇒ 一切照旧（现有 `MIR2_ZONES`/`MIR2_ZONE_MIRROR` 组合语义**逐字节不变**）。
- 开关可叠加：`SLICING=1` 意味着 `ZONES≥2` 且 `MIRROR=1`；反向不成立。
- N=1 判据的基线树保持干净（切片关 ⇒ 非 0 区仍是 stub World）。

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

### 2.4 outgoing 带的四个后果（选 (a) 后必须现在定；不可留到实现时想）

1. **同格序复活（C1–C3 的跨区形态）**：此前镜像恒在 owner 的 rect 内，与收件人**本地**真实体
   **不可能同格**（C1–C3 只覆盖"多镜像同格"）；**outgoing 镜像落在收件人 rect 内** ⇒ 会与本地真实
   体同格 ⇒ 格内序问题在跨区场景**重新可达**。
   - 定序规则（写死）：基线里跨界那一步是 `move_object` 把移动者**追加到目标格尾部**
     （`MapCellInfo::add` = push）⇒ 收件人区对 `outgoing: true` 的镜像**一律追加到格尾**（**忽略**
     其 `cell_pos`——那个下标是在**源区**世界算的，混有源区的镜像，与收件人格内序无关）；
     `outgoing: false` 的普通边界镜像仍按 `cell_pos` 定序（②的口径不变）。
   - 必带夹具：**出带镜像 + 本地真实体同格**，逐帧顺序与单区基线一致（legacy + 增量各一条）。
2. **阻挡变活（这一次"挡"是对的）**：outgoing 镜像在收件人 rect 内会**真挡路**（`is_blocking`
   不看 `mirror`）——身体确实在那里。仍按 C7 口径量一次"**两侧阻挡者 id 一致**"；**不得**因为
   "镜像不该挡"的旧直觉去改 `is_blocking`（C7 的裁决：测量到手前不改）。
3. **提升不许重复登记格表**：同 id、同格的镜像提升为真身时，**只改标志与状态**（`mirror=false` +
   全量状态覆盖 + 挂会话），**不得**再调 `add_map_object`——否则格内出现两条同 id 对象，
   `cell_pos` 取第一个、AOI 扫描出双份。用例：提升后该格 `obj_list` 里该 id 恰一条。
4. **带规模门的口径（二选一，选"算"）**：`MIR2_MAX_HALO_BAND_ENTITIES`（缺省 512，fail-closed）
   **计入 outgoing 条目**——带门 = 「rect 内 + outgoing」的总数（更保守，单个门管全部）。
   启动门（`check_band_gate`）与运行期上限都按这个总数判；不另设 `halo_outgoing_overflow`。

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
  **RSS 随区数 N 的伸缩（Leader 点名，可能是 1 万的真正瓶颈）**：采 `RSS(2 区) − RSS(1 区基线)`，
  给出**每区地图网格 / 实体内存的粗估**（每区复制一份地图网格，N=8 可能翻数倍）——这条决定 1 万
  要几个区、机器够不够；在 2 区冒烟里就采，不等 10k 长跑；
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
- 城堡地图参与分区（启动门继续排除）；
- **区 0 与区 k 的断线清理物理共用同一个函数**（卡
  `openmir2-zone-close-shared-core`，段 1c 后登记）：现在两条路径调**同一批生产函数**、
  顺序由两处顺序锚钉住；物理共用要先重排 `Closed` 臂（`online_take` 在
  交易之后、`disconnect` 在组队之后，中间不连续）并让两处顺序锚退休。
  **验收 = 区 0 与区 k 共用同一个函数 + 两处顺序锚可以退休**。

## 6. 风险与验收边界

- **最大风险 = 屏障活性**：任区 panic/超时 ⇒ 已有失败快传（`TickBarriers::abort`）+ 看门狗 5s abort；
  容量窗口必须记录 abort 次数（应为 0）。
- **正确性判据**：对**不消耗 RNG** 的子场景仍按"与单区基线逐帧对拍"立（L2-5 §3 的 A6/A8 口径）；
  **含随机（战斗/AI）的场景在 RNG 分发落地前不作为逐帧判据**（可固定种子做回归）；容量窗口
  不替代对拍。
- 本步交付的是**会话与宿主切片**；"容量"结论只对 4.2 的指标与窗口负责。

## 7. 段 0 运行时冒烟记录（2026-11-22，段 1 的第一个提交）

起栈：`game-svr.exe E:\MirServer\M2GameSvr\Server.conf`；env `MIR2_ZONES=0:0=0,0-349,699;1:0=350,0-699,699`
（真实图 700×700 切两半；布局必须**覆盖整图**，未提到的图默认整图归区 0）、`MIR2_ZONE_MIRROR=1`、
`MIR2_ZONE_SLICING=1`。

- ① 启动 INF：`L2-4 多区装配：N=2 区、边界带宽 12 格、边界带上限 512`；
  `L2-5 ② 镜像已启用（MIR2_ZONE_MIRROR=1）…`；
  `L2-5 ③ 切片已启用（MIR2_ZONE_SLICING=1）：非 0 区建真 World（322 张地图，rng=缺省随机）`。
- ② 两区线程都起来（区 1 切片区日志）；运行 20s **无看门狗 abort**。
- ③ 关服命名管道发 ASCII `SHUTDOWN` ⇒ **30s 内 `EXIT=0`**。
- 旁注：独立冒烟未起 DB/IDS ⇒ 启动期有 `链接数据库服务器[127.0.0.1:6000]失败` 重试行，属预期（非切片问题）。
- 证据文件：`E:	mp\zone-mirror-gates\seg1-smoke2.out.txt`。

## 8. 段 1b 实施记录（登录落区，2026-11-22）

开关：`MIR2_ZONE_SLICING=1`（另需 `MIR2_ZONES` ≥2 区 + `MIR2_ZONE_MIRROR=1`）。`slicing=0` ⇒ 登录路径与
段 1a **同一份** `finish_login` 调用、同一组实参（N=1 的 stdout 等价门复跑过）。

**机制**：`DBR_LOADHUMANRCD(1100) recog==1` 回执到达时，先 `decide_login_landing`（纯函数：解回包 +
`resolve_enter_map` + `RegionLayout::zone_of`）定区，再选 ctx：

- 归区 0（或未切片）⇒ 原 `sim_loop` 局部 ctx 调 `finish_login`（不变）；切片模式下计 `zone_login_landed{0}`；
- 归区 k≠0 ⇒ `lock` 该区 `ZoneState`、用 `finish_login_on_zone_state`（世界侧表全指向该区，
  进程级句柄借用区 0 线程的 `LoginProc`），**回执完成之后**才 `ZoneRouter::assign(gate_idx, socket, k)`，
  并计 `zone_login_landed{k}`；
- 未覆盖（越界坐标 / 图不在注册表 / 新角色 `HomeMap` 空）⇒ 计 `zone_login_uncovered`、踢该会话；
- 目标区不可用（状态缺失 / 线程已退出）⇒ 计 `zone_target_unavailable`、踢该会话。**两条都不回落区 0**。

**锁纪律**（死锁边界）：区线程只在 `tick_phase1`+`snapshot_publish` 与 `apply_mirrors`+`tick_phase2`
两段持 `ZoneState` 锁；屏障等待（`await_neighbors`/`after_phase2`）**不持锁**。登录线程持锁期间只跑
`finish_login`（不碰屏障）⇒ 两边不会互等。`ZoneRuntime` 为此把 `after_phase1` 拆成
`snapshot_publish` + `await_neighbors`（合成路径与拆开路径同序）。

**计数**（`perf_counters`）：`zone_login_uncovered`、`zone_login_landed{zone}`（下标 = 区号）、
`zone_router_unassigned`（demux 回落现场计，采样 WRN 注明来源 `Command`/`Closed`/`assign_rejected`/
`assign_bad_line`）、`zone_target_unavailable`、`zone_login_offzone`（诊断：落点最终不属于判定区）。

**所有权**：`world`/`displays`/`online`/`ground_sessions`/`saves`/`login_tails`/`aoi_frames_total` 属
**落地区**（`ZoneState`），区 0 不保留副本；`gates`/`engine`/`guilds`/`npc_host`/`pwd_sessions`/
`reconnect_pending`/`kick_pending` 属**进程**，由区 0 线程借用（登录落区只在区 0 线程执行）。

**已知限制（段 1b 明确不做，登记给后续段）**：

1. **新角色**（记录 `HomeMap` 空）：出生点由区世界 RNG 抽数决定 ⇒ 落区前不可知 ⇒ 计
   `zone_login_uncovered` 并拒登（不猜区）。定区前移抽数或按 `[Setup]` Home 图定区，属交接段；
2. **落点偏移**：墙格移位（≤3 格）或死亡/红名改写可能改变归属 ⇒ `zone_login_offzone` 计数 +
   WRN 暴露；不改判据（跨区搬迁在交接段）；
3. **区 k 无命令处理/退出路径**：`Closed` 与后续命令投到区 k 后由区循环丢弃 ⇒ 会话保留到进程结束；
   关服存档只覆盖区 0 的 `ground_sessions`（段 2 交接 + 逐区存档）；
4. 区 k 的 `World` 不装物品目录（`production_world` 的 `item_catalog` 由区 0 独占；区 k 无掉落路径）。

**证据**：

- 单测：`zone_login_landing_tests::landing_decision_maps_position_to_zone`（区 0/区 k/越界/新角色/
  图缺失五臂）、`zone_login_landing_tests::landing_in_zone_one_leaves_no_residue_in_zone_zero`
  （硬要求 4：区 k 有会话 + 区 0 六张表零残留 + actor id 落在区 k 段）、
  `zone_login_landing_tests::zone_login_counters_note_and_reset`；
- 真栈冒烟（切片 N=2，20s + 优雅关服）：三条启动 INF、无 abort/看门狗、`EXIT=0`（§7 同口径复跑）；
- N=1 stdout 等价门（`tools/hygiene/n1_stdout_equivalence.ps1`）：基线 `55e738ce8` vs 本段 head，绿。

## 9. 段 1c 实施记录（区 k 断线，2026-11-22）

切片区的 `Closed` 由区 k 自己处理（`handle_zone_closed`），世界侧全在区 k 线程内完成：
`prepare_closed_guild`（陈旧早退 + `bo_soft_close`）→ `online_take` → 存档快照
（`build_save_record_with_map_name`，实时态）→ `close_cleanup_residue`（四容器 + 展示表）→
`world.disconnect`（移除门：实体摘除、`actor_id=None`、`stage=Left`）。存档载荷进
`ZoneState::save_outbox`，区 0 每 tick `drain_zone_saves` 发进共享 DB 链（区 0 不读区 k 的世界）。

**判据边界（必读）**：`g3_anchor_a5_prepare_closed_guild_and_flags` 与
`closed_arm_calls_close_cleanup_residue_once_in_decommented_production_view` 属
**接线提示、非行为证据**（§12）：它们只能证明「生产臂没改序」，**不能**证明「区 0
参考重放与生产臂等价」。1c 的行为证据 = 结果向量对拍（区 0 参考 vs 区 1 处理
逐项相等）+ 两侧调的是**同一批生产函数**。物理共用同一函数列为后续卡
`openmir2-zone-close-shared-core`（见 §5）。

## 10. 段 2 实施记录（handoff 引擎，2026-11-22）

**引擎层（`world-bevy` + `region_loop`，本段已交）**：

- `Flags.handoff_pending`（`EntitySpawn`/`EntityRef` 投影同步）；`SimState.zone_bounds` +
  `World::set_zone_bounds`：移动判定成功且新格在本区 rect 之外 ⇒ 置位（未设边界 ⇒ 零参与）。
- `HaloEntity.outgoing`；源区快照过滤 = rect 边界带 **∪ outgoing 带**
  （`handoff_pending && ZoneRect::within_distance(≤ HALO)`）——T 时源区仍发布被挪出的实体。
- `apply_mirrors` 定序：`outgoing` 条目**追加格尾、忽略 `cell_pos`**（普通镜像仍按 `cell_pos`）。
- `apply_mirrors` 兜底：镜像 id 已是本区**真实**实体 ⇒ 跳过 + `mirror_shadowed_by_local`，
  **不 abort、不动本地实体**（防御计数；时序正确时恒 0）。
- `World::promote_mirror_to_real`：同 id 同格镜像**原地提升**（清 `mirror`、逐字段覆盖、
  续期格条目；**不** `add_map_object`、**不**先删后插），计数 `handoff_mirror_promoted`；
  非镜像 ⇒ `NotMirror`（幂等），位置不一致 ⇒ `CellMismatch`（fail-closed）。
- 带规模门 `check_band_gate` 用同一份 `snapshot_zone_border` ⇒ **自动计入 outgoing**（§2.4 第 4 条）。

**测试**（`l2_handoff_tests`，7 条）：零闪烁两区真屏障夹具（T 跨界、T+1 tick 顶提升/移除，
`handoff_mirror_promoted==1` 且 `mirror_shadowed_by_local==0`）、原地提升（格内恰一条/幂等/
位置不一致）、outgoing 格尾定序、遮蔽跳过+计数、outgoing 带正反例、距离谓词。

**接线（同一分支下一提交，2026-11-22）**：

- `handoff::HandoffRuntime`（每区一个）：`tick_top` = 排 ack（源侧提交/回滚）+ 排 offer
  （目标侧 `zone_of` 校验 → `promote_mirror_to_real` → 回 ack）；`after_phase2` = 发 offer +
  `HANDOFF_TIMEOUT_TICKS=3` 超时回滚（`handoff_refused` 计数 + WRN）。
- `ZoneHost.handoff_boxes`（每区一个 offer/ack 邮箱）；`spawn_minimal_zone` 与 `sim_loop`
  （区 0）都按「tick 顶排水 / barrier2 后发 offer」接入；运行时只在 `slicing=1` 构造。
- `set_zone_bounds` 生产设置：区 0 在 `sim_loop` 序言、区 k 在 `ZoneState::new` 之后；
  **多 span 区不设边界**（WRN 登记——本区不发起跨界移动）。
- **默认关闭**：`slicing=0` ⇒ 不建邮箱、不设边界 ⇒ `handoff_pending` 恒 false
  （用例 `walk_without_zone_bounds_never_sets_pending` 钉住）。

**证据（生产运行时 + 两 AOI 窗口）**：`l2_handoff_prod_tests::production_runtime_crossing_keeps_recipient_stream_continuous`
——两区真屏障线程 + **真实 `World::walk`** 驱动跨界，offer/ack/提升/移除全走生产
`HandoffRuntime`；断言**收件人自己的 `visible_actors` 台账**（appear/disappear 帧的直接来源）
从首次可见起逐 tick 连续、`promoted==1`、`refused==0`。默认（增量）与
`MIR2_AOI_INCREMENTAL=0`（legacy）两个进程窗口各跑一次，均绿。

**并发竞态（总监 2026-11-22 裁决：接受 ≤2，不加第三道屏障、不把 offer 提前到 barrier1）**：

- **界的推导（不是魔数）**：目标区在 T+1 的 tick 顶提升并回 ack；源区要**在自己的** T+1 顶收到
  ack 才移除。两线程自由跑时 ack 可能晚 **1 tick** ⇒ 源区在自己的 T+1 快照里仍发布该 id
  （outgoing）、在 T+2 的顶才移除 ⇒ 目标区在 **T+1、T+2 这两 tick** 命中「已是真实体」的遮蔽
  兜底。故**一次跨界最多遮蔽 2 tick**；N 次跨界累计 ≤2N，且 `promoted == N`（每次跨界恰一次提升）。
- **计数口径是两区合计（2026-11-22 实测修正）**：`mirror_shadowed_by_local` 是**进程级**计数，
  而 `pending` 期间**两区都持有该 id**（源区真实体未移除、目标区已提升），两区各自应用邻区快照
  都会命中「本地已是真实体」⇒ **每次跨界两区各计一次**。实测分区读数恒等（5/5、6/6…），
  证明旧推导「每次收下 ≤4、合计 ≤4N」只算了单区，是错的。
- **代码里的断言上界是 ≤6N（超集上界，不是目标）**：对每个区，遮蔽窗口 = 目标区提升 tick P 到
  源区移除 tick C-1，`P ≥ T+1`、`C ≤ T+4`（`HANDOFF_TIMEOUT_TICKS+1`）⇒ 每区每次 ≤3，两区
  合计 ≤6N。测试同时断言「进程级计数 == 两区自记之和」（不等 ⇒ 被并发用例污染，读数不可引用）。
- **异常口径（裁定 ③ 的两区版）**：**每区**每次收下实测 ≈0.5–0.7（合计 ≈1.0–1.4N）；每区接近
  3/次（合计接近 6N）⇒ 判定**时序设计破了**，要查，不许拿上界掩盖。
- **两条硬判据不松**：`refused == 0`；受害者（该 id）**全程无 appear/disappear**（收件人
  `visible_actors` 台账逐 tick 连续）。
- **「恒 0」的正确范围**：**引擎级单 tick 显式定序夹具**恒 0（`l2_handoff_tests` 的断言保留）；
  自由运行的两区线程 ≤2/次（`l2_handoff_prod_tests::production_runtime_multi_crossing_bounded_shadow`
  用**多次跨界**断言 `promoted == N`、`shadowed ≤ 2N`、`refused == 0`、受害者台账连续）。

**被证伪的假设：「目的地必有该 id 的镜像」（2026-11-22 合并门红原文）**

```text
[WRN] handoff 拒绝：区 0 收到实体 1 但镜像缺失或位置不符（NotMirror），来自区 1
[WRN] handoff 被拒：区 1 实体 1 保留本区（回滚 1）tick=3
assertion `left == right` failed: 每次跨界恰一次提升（累计 promoted=2 crossings=3）
```
现象：玩家跨过去后**立刻跨回**时，目的地（原区）此刻没有该 id 的镜像（它刚把真实体移除、邻区
快照还没送到）⇒ 旧实现按 `NotMirror` 拒收、回滚 ⇒ **该走却被拒**（跨区边界的可走性被内部记账
左右），这是口径 A 不能接受的。

修正（总监裁决）：handoff 收下不得以「目的地已有镜像」为前提——
1. 有镜像 ⇒ 原地提升（`handoff_mirror_promoted`）；
2. **无该 id 实体 ⇒ 直接落真身**（`handoff_inserted_fresh`；落格一次、不建空镜像、不先插后升）；
3. 该 id 已被**真实体**占用 ⇒ 才允许拒（`RefusedOccupied`；`NotMirror` 不再作为拒收理由）。
判据：`promoted + inserted_fresh == 收下次数`（N 的口径 = 被收下的跨界），**`refused == 0` 仍是
硬红线**；新增**同 tick 往返**用例（跨过去下一 tick 立刻跨回，两次都成功、台账连续、
落点无重复实体）。**回滚路径本身保留且正确**（拒收后实体留在原区、无丢失无重复）。

**幂等支（`AlreadyHeld`）的三点裁定（总监 2026-11-22）**：

1. **必须计数** `handoff_already_held`（`World::handoff_accept` 唯一计数点，`mirror_metrics` 进程级）。
   它是「源区拿到 ack 前每 tick 重发」的直接读数，也是「幂等支被删」的护栏：删掉该支 ⇒ 重发落
   `RefusedOccupied` ⇒ 源区回滚 ⇒ `handoff_refused > 0`（硬红线），本计数同时掉回 0。
   **为 0 正常**（ack 恰好不迟到）；每次跨界收下的结构性上界是 **3**（第一次 offer 被收下；
   pending 寿命 ≤4 tick ⇒ 至多还有 3 次重发），远大于 3 的突变要查。用例
   `l2_handoff_tests::handoff_accept_idempotent_resend_is_counted`（引擎级、确定性）与
   `l2_handoff_prod_tests::production_runtime_multi_crossing_bounded_shadow`（`already_held ≤ 3N`）。
2. **「位置不参与判断」的依据写死**：actor 业务 id **全局唯一**——`zone_id_base(z) = z ×
   shard::ACTOR_ID_SEGMENT`，各区 id 段不相交（`zone_id_base_is_zero_for_zone_zero_and_linear`）
   ⇒ 同 id 的真实体**就是**同一逻辑实体；重发携带的坐标必然旧（目标区已在走），拿旧坐标判
   「位置不符」只会把幂等重发误判成拒收。
   **反证**：若将来允许同一 id 跨代复用（死亡/下线后回池），旧实体的迟到重发会与新实体撞 id，
   本幂等会把**不同实体**误判成同一 ⇒ 不变量须变成「id + 代际（generation）」，
   届时必须按代际区分，不能沿用本支。
3. **`≤6N` 是上界不是目标**（见上节「异常口径」，两区口径）：每区实测 ≈0.5–0.7 次/跨界，
   `shadowed/N` 接近 6 同样判时序破了，不许用上界掩盖。修复前 spec 的 `≤4N`/`≤2N` 推导漏了
   「两区各计一次」（这层是**推导错误**，与下面的迟到重发是两回事）；修复前实测最高 52（=5.2N）
   则**同时**混入了迟到重发造成的「实体被传送一次、两区各多遮蔽若干 tick」——去重修复后同一
   用例降到 10–14（=1.0–1.4N），两层原因都已消除/校正。

**迟到重发必须按 offer 身份去重（2026-11-22 总监裁定；`flake_leg_1.log` 根因）**：

- **证伪**：`AlreadyHeld` 只能挡「目标区此刻还持有该实体」的重复。目标区在提交后**移除**了实体
  （回程已收下）时，一条**迟到的重发 offer** 会被当成新的跨界收下 ⇒ 实体被「传送」一次。
  原始证据：往返用例 `accepted=3`（应为 2），事件序为「区 1 移除 tick=10 → 区 1 落真身（来自区 0，
  应在 tick 8）」——同一次 pending epoch 的重发在移除后才被排水。
- **修法**：offer 带**身份** `epoch`（= 源端该 id 这次 `pending` 的起始 tick；同一次跨界的所有
  重发共享，下一次跨界必然更大）。目标区维护 `(源区, id) → 已处理的最大 epoch`，
  `epoch <= 已处理` ⇒ **丢弃**（不 ack、不动作），按 `HANDOFF_DUP_TTL_TICKS = 2×4` 回收（有界）。
  `AlreadyHeld` 保留作第二道幂等（去重后几乎恒 0）。
- **计数**：`handoff_offer_dup_dropped`（每次跨界收下的重发 ≤3 ⇒ ≤3N）。实测 N=10 时 14–17，
  即每轮都在拦真迟到重发。
- **牙**：引擎级确定性用例 `l2_handoff_tests::tick_top_drops_late_duplicate_offer_epochs`
  （同 epoch 迟到重发 ⇒ 丢弃 + 计数 +1；**新 epoch ⇒ 必须收下**，防去重过度）；
  生产路径 `shadowed_by_zone`/`dup_dropped` 上界断言 + 进程级/分区对账。
- **验证**：两个 AOI 窗口各 **20 轮**连跑 `cargo test -p game-svr --lib l2_handoff`，
  **红=0**（40/40）；取证轮（`--nocapture`，`MIR2_HANDOFF_TEST_DIAG=1`）读数：`refused=0`、
  `shadowed` 10–14（N=10）、分区恒等、`dup_dropped` 14–17、`already_held=0`。

**仍未做（明确登记）**：真机 bot 过网关跨界的批量冒烟（`tools/botload`）——本段的生产证据是
「两区真线程 + 真实移动命令」，尚未跑完整网关栈。

## 11. 段 3 实施记录（跨区收件人帧缺失计数，2026-11-22）

**范围（总监 2026-11-22）**：只加计数、**不实现跨区帧合成**（留第④/⑤步）。六个广播面各一条
独立计数，`slicing=0 ⇒ 六面全 0`（N=1 逐字节不变的护栏）。

| 面（`CrossRecipientSurface`） | 计数点 | 判据 |
| --- | --- | --- |
| `refmsg` | `send_ref_msg_to_targets` 的 `find_by_actor` 未命中 | `world.entities.get(id).mirror` |
| `run_frames` | `flush_run_frames` 的 `SendRefMsg(53)` 观察者副本未命中 | 同上 |
| `npc_merchant` | `drain_npc_merchant_turns/hits` 经 `broadcast_ref_msg_with_surface` | 同上 |
| `feature_changed` | `broadcast_feature_changed_except` 自身循环未命中 | 同上 |
| `mountride` | `apply_pending_gm_mountride`：本地会话表枚举**看不到**跨区玩家 ⇒ 同图 ±range 内**镜像玩家**逐个计 | `mirror && race==Play && !ghost` |
| `merchant_disappear` | `drain_merchant_disappears` 的收件人未命中 | 同上（镜像） |

**判据反证（总监要求写死）**：收件人谓词（`chat::hear_targets*` / `ref_msg_cache`）已要求
`race == Play && !ghost`；镜像每 tick 由 `apply_mirrors` 刷新、陈旧镜像**同 tick** 清扫 ⇒ 候选里的
镜像 = **本 tick 邻区仍在发布的活玩家**。唯一残差是「同一 tick 内下线」（≤1 tick，投递点不可辨，
按跨区计）；不另造不可达的 `target_mirror_not_deliverable` 计数器（等 ⑤ 步镜像带 route 后可按
route 有无再分）。

**开关/读点**：计数闸 = 启动时 `plan_slicing` 的结果（`perf_counters::set_slicing_enabled`）；
`perf_counters` TSV **末尾新增 7 列**（六面 + `cross_recipient_missing` 合计；合计列 = 六面本
tick 增量之和，由构造保证），表头已同步 ⇒ 段 4 容量脚本按列名读取并打印。读函数
`cross_recipient_missing()` = 六面之和。

**④a 追加 3 列（2026-11-22）**：`zone_cmd_unsupported`（区 k 未接线的非移动命令，按 `ident`
采样日志）、`handoff_move_after_transfer_dropped`（handoff 提升后源区丢走位 = ④b「会话/路由随迁」
的缺口基线）、`move_map_change_unsupported`（无 `MapRegistry` 的换图批次 = 区 k 换图帧降级）。
`perf_tsv_summary.py` 首屏同时给「窗口（ticks/时长）+ `refmsg_targets` 分母 + 占比」（§12.4A 口径）。

**六条独立牙**（`l2_s3_cross_recipient_tests`）：夹具 = 本地玩家（有会话）+ 镜像玩家（race=Play、
无本地会话）；每次注入只用**一个**面，断言「该面 +1、其余五面 0」（增量口径，避免与并行用例的
全局计数互扰）；另有一条 `slicing=0 ⇒ 六面全 0` 护栏。

**变异验证（逐面，恢复后 sha 回合 `ace073fe`）**：
- 删 `refmsg` 面调用 ⇒ `refmsg_face_counts_one` 红（同时 `npc_merchant_face_counts_one` 红——
  两面共用同一投递段，符合预期）；
- 删 `run_frames` / `feature_changed` / `mountride` / `merchant_disappear` 各面调用 ⇒ 各自牙红；
- 把 NPC 面参数写成 `RefMsg` ⇒ `npc_merchant_face_counts_one` 红（面隔离被抓）；
- 删 `SLICING_ENABLED` 早退 ⇒ `slicing_off_counts_nothing` 红。

**未做（按范围）**：跨区帧合成（镜像 route 消费、事件跨区队列）——第④/⑤步；真机 bot 跨界冒烟
由总监排窗口。

### 10.1 §10 的适用范围收窄（总监 2026-11-22 裁决）

§10 的「遮蔽 ≤6N（每区 ≤3）/ 上界不是目标」**只对 handoff 窗口成立**。`mirror_shadowed_by_local`
的唯一自增点是「镜像 id 撞本区真实体」（`apply_mirrors` 的 `RefusedOccupied`），而两区各装 322 张图
⇒ **静态 NPC/怪在两区同 id** ⇒ 只要两区同时在线，该计数**稳态也不为 0**。因此：
- 它**不能**当回归红线；看到非 0 不代表 handoff 时序破了；
- ④/⑤ 必须把它拆成「handoff 窗口内」与「静态实体撞名」两类计数后，才恢复「窗口内应 ≤6N」的判据。

### 11.1 评审补充（总监 2026-11-22）

- **`refmsg` 没有独立牙，隔离证据由一对变异承担（写死）**：`refmsg` 与 `npc_merchant` **共用同一
  投递段**（`send_ref_msg_to_targets`），所以**不存在**只打红 `refmsg` 面而 `npc_merchant` 仍绿的
  变异——删投递段 note 两面同红（M1）。`refmsg` 的**面隔离**证据 = **{M1 双红 + M3 单红}**：
  M3（NPC 面的 surface 参数写成 `RefMsg`）只打红 `npc_merchant`、`refmsg` 仍绿，证明面参数确实
  分流。将来看到 M1 双红**不要**当成牙有问题。
- **`mirror` 判据的残差不再分桶（写死）**：收件人谓词已要求 `race == Play && !ghost`；镜像每 tick
  由 `apply_mirrors` 刷新、陈旧镜像**同 tick** 清扫 ⇒ 候选里的镜像 = 本 tick 邻区仍在发布的活玩家。
  唯一残差「同一 tick 内下线」（≤1 tick）在**投递点不可辨**（本地会话表未命中、World 里只有镜像、
  没有 route）⇒ **不新增** `target_mirror_not_deliverable`（在不可辨处造计数器就是造死代码），
  残差按跨区计并如实写明；若第 ⑤ 步让镜像带上 `route`，届时可按 route 有无再分。

## 12. 段 4 实施记录（容量冒烟：2 区 1k + 1 区基线，2026-11-22）

**三条硬要求（总监）**：①脚本**按列名**读 7 列，`cross_recipient_missing` 合计在**摘要首屏**；
②2 区 1k 冒烟落 TSV/JSON 产物，摘要含有效 tick 率、p50、`region_barrier_us` max/total、六面分解、
RSS；③额外跑 **1 区基线**（同机同时长，`MIR2_ZONES`/`MIR2_ZONE_MIRROR`/`MIR2_ZONE_SLICING`
**一律 Remove**，作内存伸缩对照），给 `RSS(2区)−RSS(1区)` 与每区粗估。

**工具**：
- `tools/botload/perf_tsv_summary.py`（已入工具门 `tool_selftest_gate` 的 `REGISTRY`）：
  按列名读；缺列/列宽不一/零 tick 行/合计≠六面之和/缺 p50 ⇒ rc=2；`--selftest` 覆盖列序打散、
  缺列、合计漂移、零 tick 行、缺 p50、compare 区数不递减；`--compare` 给 RSS 差与每区粗估。
- `tools/botload/run_l25_slice.ps1`（**纯编排、无自测** ⇒ 由 2 区 1k 冒烟端到端覆盖；证据 =
  perf TSV + report.json + rss.ndjson + summary.json）：注入三开关 + `MIR2_PERF_COUNTERS` → 起栈 →
  按 pid 采 game-svr RSS → `load_gate` → graceful 停栈 → `run.json` → 摘要 →（有基线时）对比。

**窗口规则（总监）**：单次只跑一个压测、起栈后不跑 cargo；先基线后 2 区、同机同时长；
RSS 单调增长则停下贴曲线；跑完栈停干净、game-svr 进程 0。

**实测**：见 §12.1（本卡交付时回填）。

### 12.1 实测（2026-11-22，窗口内单跑，Rust 栈 + BotSrv）

**窗口口径**：一次只跑一个压测；起栈后不跑 cargo；跑完 graceful 停栈、`game-svr` 进程 0。
两次运行同机同时长（`Bots=1000`、`HoldSec=120`）。产物**一律仓外**：
`E:/UserTemp/l25-slice/1z-20261122-175230/`、`.../2z-20261122-175527/`（perf TSV、tier-1000/report.json、
rss.ndjson、run.json、summary.json；2 区另有 load_gate.log / stack-start.log / stack-stop.log）。
首次尝试（17:27）卡在编排 bug（见下），其 idle perf TSV 原样移到
`E:/UserTemp/l25-slice/1z-20261122-1727-failed-startcapture/`（只作证据，不参与结论）。

**编排 bug（已修，随段 4 第二提交）**：`& pwsh -File <起栈脚本> 2>&1` 的捕获会等**孙进程**退出才
返回（常驻角色永不退）⇒ 起栈调用永不返回。修法：`Start-Process -PassThru` + 输出重定向到文件 +
`WaitForExit()`（只等直接子进程）；RSS 采样改独立 `tools/botload/rss_sampler.ps1`（按 pid 文件
`{roles:[…]}` 的 `role/pid` 采）；产物默认根 = `[IO.Path]::GetTempPath()/mir2-l25-slice`。

**1 区基线（`-Zones 1`：`MIR2_ZONES`/`MIR2_ZONE_MIRROR`/`MIR2_ZONE_SLICING` 三开关一律 Remove）**

| 项 | 值 |
| --- | --- |
| load_gate | tier=1000 **GREEN**（login=100%、conn_lost_steady=0、internal_errors=0、in_world=1000；`GREEN_SMOKE_ONLY`：HoldSec=120<360，`window_valid=false`） |
| 有效 tick 率 | **5.02/s**（rows=725，144.4s） |
| tick p50 / p99 / max | **16 / 47 / 78 ms**（p90=31） |
| `cross_recipient_missing` | **0**（六面全 0；N=1 护栏） |
| `region_barrier_us` | max 0 / mean 0（单区） |
| RSS(game-svr) | ws max **714.33MB** / mean 688.69；priv max 713.50 |
| RSS 斜率（0–144 / 60–144 / 90–144 / 110–144s） | +29.6 / +22.2 / +14.6 / **+3.2 MB/min**（假人 ramp 分配，末段趋平台） |

**2 区（`-Zones 2`，布局 `0:0=0,0-349,699;1:0=350,0-699,699`，真图 700×700）**

| 项 | 值 |
| --- | --- |
| 启动证据 | game-svr.out.log：`L2-4 多区装配：N=2 区…`、`L2-5 ② 镜像已启用…`、`L2-5 ③ 切片已启用…（322 张地图）` |
| load_gate | tier=1000 **GREEN**（同上四项；`GREEN_SMOKE_ONLY`） |
| 有效 tick 率 | **5.03/s**（rows=726，144.4s） |
| tick p50 / p99 / max | **16 / 47 / 78 ms**（p90=31） |
| `cross_recipient_missing` | **0**（六面全 0）——**本轮 1000 个假人全部落在区 0**（无边界带负载），六面未被打到；计数有效性由段 3 的六条牙 + 7 条变异承担 |
| `region_barrier_us` | max **548,910µs** / mean **202,143.8µs**；`region_barrier_total_us` mean 202,168.2µs/tick |
| RSS(game-svr) | ws max **1,076.75MB** / mean 1,051.90；priv max 1,075.88 |
| RSS 斜率（同上四窗） | +29.6 / +19.5 / +10.5 / **+3.9 MB/min**（与 1 区同形：ramp 后趋平台） |

**内存伸缩（`--compare`）**：`RSS(2区) − RSS(1区)` ws **Δ362.42MB**（priv Δ362.38MB）⇒
**每区粗估 ≈362.42MB**。线性粗估（**仅供参考，不是结论**）：8 区 ≈ 714 + 7×362 ≈ **3.25GB**；
1 万在线的真实瓶颈仍需 4/8 区窗口实测（地图网格/镜像带/假人分布都随布局变）。

**屏障读数的口径**：本轮负载全在区 0 ⇒ 区 1 空转、每 tick 在屏障上等区 0 的整 tick 工作
（max ≈ 548.9ms 是登录/进世界尖峰一次；mean ≈ 202ms ≈ tick 周期）——这是 **Amdahl 代价的真实
读数**，不是"屏障有 bug"；tick 从假人视角仍是 p50 16ms / p99 47ms、有效 5.03/s。
要压边界带，需按计划 §4.1 的「跨区行走（边界带集中）」另排窗口。

### 12.2 `region_barrier_us` 口径澄清（2026-11-22，总监质询）

**结论（一句话）**：2 区臂的 202ms 均值**不是**「区 0 的整 tick 工作量」，而是「**相位差 +
区 0 的节拍空闲**」——`wait_with_probe` 只包 `barrier.wait()`，而区 0 的 5/s 节拍 `sleep` 位于
**barrier2 之后、下一 tick 的 barrier1 之前**，非 0 区（无节拍 sleep）早到 barrier1 后就在那里
等区 0 睡完。

**依据（符号锚；高 churn 文件不写行号）**：
- `wait_with_probe`（`region_loop` 模块）：计时只包 `barrier.wait()`，计时区间内**没有** sleep；
- `sim_loop` 每轮尾部：`let next = Instant::now() + tick;` 之后进入 `HostSleep` 段
  （`next.checked_duration_since(now)` 后 `thread::sleep`）——节拍 sleep 的落点；
- `sim_loop` 的区 0 切片分支里 `runtime.after_phase2(...)`（barrier2）在 tick 内，
  **先 barrier2、后节拍 sleep、再下一轮 barrier1**；
- 非 0 区线程：`spawn_minimal_zone` 里的 `while !stop { … run_zone_tick(…) }` 循环
  **没有 sleep**，`run_zone_tick` 依次 phase1 → barrier1 → … → barrier2 → 立刻下一轮 ⇒
  它们到 barrier1 时区 0 还在上一轮的节拍 sleep 里。

**读数解释**：均值 ≈ 200ms（tick 周期）− 区 0 单轮工作量（+ 调度抖动）；max 548.9ms 对应登录/进世界
尖峰时 sim_loop 单轮超出 200ms（此时 `checked_duration_since` 为 None、pacing sleep 归零，区 1 多等
到 ~550ms）。因此该列在**有空区的布局**下度量的是「空区被忙区节拍拖着等」的 Amdahl 相位代价，
**不是**屏障实现的开销；屏障本身（`FailFastBarrier::wait`）只是 condvar 等待。

### 12.3 段 4 跨区冒烟记录（2026-11-22，窗口内）

**结论先说**：**真机客户端的跨界 handoff 目前无法端到端触发**——不是夹具问题，是**接线缺口**：
切片模式下**非 0 区线程只处理 `Closed`，其余命令一律丢弃**（「命令处理属段 2」），所以落在区 1
的假人发的 `CM_WALK` 根本到不了世界；而区 0 多 span（321 张未提图默认归区 0）⇒ `bounds_for`
返回 None、**区 0 不设 handoff 边界**。两个条件叠加 ⇒ 当前布局下**没有任何一侧既能发起 handoff
又能收到真实客户端移动命令**。`handoff_offered=0` 是接线缺口的必然结果，**不是**「跨了没计数」。

**夹具（BotSrv 侧，默认关，env 门控）**：`MIR2_BOT_BOUNDARY_WALK=1` +
`MIR2_BOT_BOUNDARY_WALK_HALF`（各自落点 ±HALF，默认 3）+ `_STEP_MS`（默认 700）。
- 走的是**真协议**：`MakeMessage(CM_WALK, MakeLong(目的地X, 目的地Y), 0, dir, 0)` +
  `SendSocket(EDCode.EncodeMessage(...))`（与压测 `CM_TURN` 探针同一编码路径；协议实证：
  oracle `ClientWalkXY` 用 `GetNextDirection(CurrX,CurrY,nX,nY)` 走一步并要求到达 ⇒ **Recog 是目的地**）。
- **绕开客户端 `PlayScene.CanWalk` 预检**（夹具行为，已登记）：该预检依赖 `Map.LoadMapData()`
  的块窗口，压测环境不可用 ⇒ 曾把消息在排空时静默丢弃（证据：`bots.log` 1000 条
  `[BOUNDARY_WALK] blocked … CanWalk=false`）。绕开后服务端仍是唯一裁决者。
- **夹具有效性证据**：短诊断轮（100 bots/60s，`MIR2_LOG_MOVES=1`）服务端日志
  `MIR2_MOVE ident=3011 … moved=true after=Some((x,y))` **3928 条**（区 0 侧）；区 1 侧 0 条。

**夹具/布局踩坑（已修，留证）**：
- 首次把假人放在 (344/356,618) —— **墙格**（解析 `0.map`：`bk/fr` bit15 置位 ⇒ 非 Walk）⇒
  镜像写失败 `RefusedCell` ⇒ 旧代码 abort 整机（见下「abort 裁决」）。`0.map` 实测：
  (330..332,265) Walk、(344,618)/(356,618) 非 Walk。
- 第二轮把 1973 个角色铺到 **108 个已解析为 Walk 的格**（x 316..339 × y 250..254；
  `E:/UserTemp/walkable_cells.txt`），布局分界 x=326（区 1 侧 58 格 / 区 0 侧 50 格）。

**abort 裁决落地（总监 2026-11-22）**：`apply_mirrors` 遇 `RefusedCell` **不再 abort**——丢弃该条
镜像 + `mirror_write_refused_cell`（进 perf TSV）+ 限流 WRN（id/map/坐标/原因，前 8 条 + 每 1000 条）；
`RefusedOccupied` 仍跳过 + `mirror_shadowed_by_local`（同时计 `mirror_write_refused_occupied`，同事件）。
**口径纠正（写死）**：`mirror_shadowed_by_local` **不是 handoff 指标**——唯一自增点是「镜像 id 撞本区
真实体」，而两区各装 322 张图 ⇒ 静态 NPC/怪在两区同 id ⇒ 稳态也非 0。§10 的「遮蔽 ≤6N」只对
**handoff 窗口**成立；带静态实体共存时该计数**不能当红线**，④/⑤ 再拆两类计数。

**六面缺口基线（近边界负载，供 ④/⑤ 对照）**：`cross_recipient_missing` 总计
**1.46M / 1.63M / 3.01M**（三次近边界轮，随边界带实体密度变化）；分解主要为
`refmsg`（1.46M–3.00M）+ `npc_merchant`（1.2k–13.4k），其余四面 0（本轮负载不触发）。
`mirror_shadowed_by_local` 0.75M–1.38M；`mirror_write_refused cell` 0–3603（非走格上的静态实体镜像）。
tick p50 0–47ms / p99 78–79ms / 有效 4.95–4.99/s；`region_barrier_us` mean ≈198–211ms（§12.2 口径）。
RSS(game-svr) 2 区 ≈1.03–1.14GB（Δ 313–421MB vs 1 区基线）。

**跨界冒烟的前提（写死，缺一条只能算稳定性跑）**：
1. 假人武装后的 **range 必须覆盖分界**（日志回显 `[BOUNDARY_WALK] armed … range=[x0,x1]`，必须与布局分界对照，不能只看 env 配置）；
2. 假人必须落在**允许发起 handoff 的源区**（今天只有区 1；区 0 多 span 不设边界）；
3. 该区线程必须**真的处理 `Command` 事件**（当前只有区 0 处理；非 0 区丢弃 —— ④/⑤ 的接线前提）。

**待 ④/⑤**：①非 0 区命令接线；②区 0 的 span 问题（322 张未提图默认归区 0）；③静态实体「每区恰一份」
（本次两区各装 322 张图，静态 NPC/怪同 id 各模拟一次 —— 内存 + 行为分歧）；④拆分
`mirror_shadowed_by_local` 的「handoff 窗口」与「静态撞名」两类计数。

### 12.4 段 4 补证（总监 2026-11-22 三点）

**A. `cross_recipient_missing` 的口径与分母（防止误读成"丢了 N 帧"）**

它是**广播收件人次**（每次广播 × 每个落在邻区、本地会话查表未命中的镜像收件人），**不是帧数**、
也不是"只有 N 帧"。分母 = 同一投递段的候选收件人次 `refmsg_targets`（`send_ref_msg_to_targets`
对**全部 `broadcast_ref_msg*` 面**统一计）与窗口（ticks/时长）：

| 轮 | 窗口 | ticks | refmsg calls | refmsg targets（分母） | cross 合计 | 占比 |
| --- | --- | --- | --- | --- | --- | --- |
| `2z-20261122-182056` | 446.1s | 2222 | 208,129 | 77,620,190 | 325,378 | 0.42% |
| `2z-20261122-183911` | 446.1s | 2210 | 208,649 | 74,942,165 | 1,630,474 | 2.2% |
| `2z-20261122-185934` | 444.8s | 2220 | 86,412 | 15,046,874 | 3,014,231 | 20.0% |
| `2z-20261122-190833`（诊断，100 bots） | 70.6s | 344 | — | 50,189 | 5,515 | 11.0% |

读法：表内三轮的 bots 位置/密度不同（前两轮边界带只有少量实体，第三轮铺满 108 格），所以占比
在 0.4%–20% 间变化——**只能作 ④/⑤ 的同条件对照基线**，不能单独当"缺帧率"结论。

**B. BotSrv 产物证据（被测/部署二进制自报 sha+mtime）**

| 项 | 值 |
| --- | --- |
| 构建命令 | `dotnet build src/BotSrv/BotSrv.csproj -c Release -v q --nologo`（工作树根执行） |
| 产物路径 | `<本工作树>/src/BotSrv/bin/Release/BotSrv.exe` |
| sha256 | `10299a2c85189d8b9e6a96bf9af2e91946c9b445426542ed567a6c27fcff3a7d` |
| mtime | 2026-11-22 **18:59**（与最后一次构建日志 `E:/UserTemp/s4c_botsrv_build2.log` 同时刻） |
| 本轮读数所用 | 185934（起于 18:59:34）与 190833（起于 19:08:33）**都晚于该构建**；`load_gate` 的 `BOTGUARD_ENV` 行打印 `exe=<本工作树>/src/BotSrv/bin/Release/BotSrv.exe` ⇒ **同一份 exe** |

**C. 数据库写入留痕（保留，不删除；删除需 Owner 授权）**

| 项 | 值 |
| --- | --- |
| 库/表 | `mir2_db.characters`（只此一张表被写） |
| 写入列 | `MapName, CX, CY, HomeMap, HomeX, HomeY, Dir`（铺到 `0.map` 已验证 Walk 的 108 格） |
| 行数 | **1973** 行（`WHERE LoginID LIKE 'loadbot%'`） |
| 角色名前缀集合 | `LoginID` 全部 `loadbot*`；`ChrName` 1972 行 `loadbot*` + 1 行 `lb1a` |
| 识别为测试数据 | 账号前缀 `loadbot`（由 `tools/client/account_provision.ps1 -Prefix loadbot` 创建）；真实玩家/其他卡不使用该前缀 |
| 唯一回滚依据 | `E:/UserTemp/l25-slice/db_chars_before.tsv`（1973 行，改前快照，含上述 10 列） |

**D. 验收说明补充（防止翻旧账误读）**：本轮三轮 `handoff_offered` 全 0，**按"驱动不成立"记录**，不是
"跨了没计数"，也**不得**被当成"无缺口"——缺口基线请看 §12.3 的六面读数；跨界冒烟的有效性前提见
§12.3 末尾三条。

### 12.5 段 4a 实施与窗口实测（2026-11-22）

**实施（已合入 `master`）**：区 k 线程处理移动类 `Command`（复用 `handle_move_cmd`）；区 k 的 World
灌入与区 0 同一份 `.map` 格子属性/传送点/门（`new_zone_state` 单点构造）；三处缺口计数进
`perf_counters` TSV：`zone_cmd_unsupported`、`handoff_move_after_transfer_dropped`、
`move_map_change_unsupported`。

**读数口径（写死，防止再踩）**：`perf_counters` TSV 是**逐 tick 值**，汇总必须**按列求和**——
不要把最后一行当累计（2026-11-22 总管亲自踩过一次：末行 `offered=0` ≠ 全轮 0）。

**窗口实测（2 区臂，`2z-20261122-205726`，385.7s，走廊铺位）**：
- `handoff`：offered=334 / accepted=58 / refused=0 / promoted=58 / dup_dropped=166；
  **offered/accepted ≈ 5.8 次/跨界**、dup≈50%——④b 优化 offer 重发策略的直接输入。
- A1：`MIR2_MOVE` 区 1 侧 `moved=true` **4164** 条。
- A4 首屏：`cross_recipient_missing = 6,432,455 / refmsg_targets=64,296,771 = 10.004%`（归一 100 万/min）。
- zone-k 计数：`zone_cmd_unsupported=1446`、`handoff_move_after_transfer_dropped=60502`（归一 9410/min）、
  `move_map_change_unsupported=0`。
- 同负载 1 区臂（`1z-20261122-210502`）：**两臂 p99 均 109ms**（p50 46 vs 31）⇒ `p99>100ms` 的
  load_gate RED 属**负载本身**，与分区/④a 无关；同负载 `ΔRSS = 1128.00 − 799.68 = 328.32MB/区`
  （旧 362/425MB 因负载自变量不同作废）。
- 走廊夹具：`x=321..339 × y=263..280`（342 格，7×7 邻域全 Walk、跨分界 x=326）；DB 铺位快照
  `db_chars_before_4a_corridor.tsv`；逐 bot 断言（落点在矩形/band 全 Walk/7×7）为 840/936/992/861（2z 臂）。

**判据裁决（2026-11-22，总管）**：`tick P99` 的 FAIL 线改为 **Owner 验收线 200ms**；`>100ms` 降为
**WARN**（内部余量提示，不判红）；`tick P50 ≤ 100ms` 保留为硬门。依据 = 4a 窗口两臂同负载 p99
均 109ms（1z 的 p50 反而更高）⇒ 100ms 红线是负载本身打的，不是分区代价。改动落点：
`tools/botload/load_report.py` 的 `DEFAULT_THRESHOLDS`/`judge`（新增 `warns` 字段与两侧牙）、
`tools/botload/load_gate.ps1` 的 GREEN 行、`tools/botload/README.md` 判据表。

**④b 登记（本轮窗口实测暴露，未做）**：
1. `handoff_refused` 拆成 `handoff_rejected`（目标区显式拒）与 `handoff_timeout`（等 ack 超时）——
   两者语义不同（红线 vs 抖动）。
2. `handoff_move_after_transfer_dropped` 改名 `zone_move_online_without_entity`（自增点是通用
   「在线表有条目、世界无实体」，来源含提升后与 `AlreadyHeld` 后）。
3. `handoff_already_held` 进 TSV（本轮日志 `handoff 提交=168 vs 提升=58`，~110 条只能靠日志推）。
4. 会话/路由随迁（提升后源区丢走位 = 本轮的 60,502 条）。
5. 走廊外异常落点 2 例（bot 自报 CurrX=839 > map 宽 700，与 DB 铺位不符）——数据/夹具异常单独记，
   不混入通道断言通过率。

### 12.6 段 4b-0：跨区 ActorId 撞车修复（2026-11-22）

**症状**：100 人 2 区冒烟里，`MIRROR_SHADOW` 同一 id 出现两个不同玩家（`loadbot24`/`loadbot26`
都是 4194330，共 10 组），一只怪（`暗之双头血魔`）与玩家同 id；`进世界` 的 100 条实体 id 里
97 条 ≥ 4,194,304（区 1 段）——**设计上的"每区分段"没有落到实际分配器**。

**根因**：`EcsStore::bind` 对**任何** id 都执行 `next_id = id + 1`。区 0 的世界收到区 1 玩家的
**镜像**（id ≥ 4,194,305）后被登记，区 0 的分配器即被推到区 1 段 ⇒ 之后在区 0 落地的玩家
拿到区 1 的 id，与区 1 玩家撞车（镜像互相遮蔽；AOI/帧/会话表全被污染）。

**修法**（最小、单点）：
- 新增 `EcsStore::insert_foreign` / `SlotTable::bind_foreign`：**外来 id 只登记、不推进 `next_id`**；
  `upsert_mirror`（镜像插入）与 `handoff_accept`（提升落真身）改走该入口。
- **启动门（fail-closed）**：`check_zone_id_base` 要求每个区 World 在任何分配之前
  `next_actor_id == zone*SEGMENT + 1`，否则 ERP + `exit(2)`；启动期打印每区
  `ZONE_ID_BASE` 与首个玩家 `ZONE_ID_FIRST`（实测分配结果，不靠配置值推断）。
- 遮蔽计数拆两列（`mirror_shadowed_static` / `mirror_shadowed_handoff`）：修复后 100 人冒烟
  **static=0**，handoff=46（全部是同一玩家的提升收敛窗口，≤1 tick，属正常面）。

**外来 id 归属校验（2026-11-22 追加）**：`insert_foreign` 不只"不推进"——`set_id_segment`
登记段长后，`bind_foreign` **断言外来 id 落在别的区段**（`id / SEGMENT != id_base / SEGMENT`），
本区 id 走外来路径直接 panic（否则会掩盖真正的分配错误）；启动门 `check_zone_id_base`
同时要求段长已登记，未登记即拒绝启动。

**handoff 分量的正常面与越界判据（2026-11-22 追加）**：`mirror_shadowed_handoff` 的正常面 =
「提升已发生、源区快照还带它一个 tick 的残留」——同一 id/同名/同格的提升实体，每次跨界在
源区**至多 1 tick** 残留 ⇒ 上界 `≤ 1 × 每秒邻区发布次数`；按 §10 的两阶段口径记
**≤ 6N（N = 提升次数）**。实测：100 人/10s 冒烟 `44 / 22 = 2.0 次/提升`（界 132），在界内。
**越界判据**：持续 `mirror_shadowed_handoff > 6 × handoff_promoted`（同窗口）⇒ 查快照残留
超窗 / 重复提升 / 源区未释放；`mirror_shadowed_static` 必须**恒 0**（任何非 0 都是跨区
静态同 id 复现）。

**牙**：`foreign_mirror_id_does_not_advance_local_allocator`（镜像外来 id 不改分配器）、
`check_zone_id_base_is_fail_closed`（启动门两侧 + 段长未登记拒绝）、
`foreign_insert_rejects_local_segment_id`（本区段 id 走外来路径必须 panic）；变异
M8/M9/M10 各自只红对应用例。
**纪律更正**：变异脚本的还原**不许**用 `git checkout`（未提交工作会被退回 HEAD 丢掉），
必须用内存原件写回 + sha 相等断言。
