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
| `world.rng`（共享随机流） | 一条流 | **暂不切** | 同上：按区切开会让同 seed 的抽数序列按区交错；R 级对拍仍是单区场景，容量验收不以 RNG 逐抽等价为前提 |
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

- **检测**（源区）：`World::try_step_blocked_impl` 在 `cell_match` 之后、`move_object` 之前比对
  `RegionLayout::owns(zone, map_id, tx, ty)`；跨界 ⇒ 实体置 `HandoffPending`（后续输入拒绝），
  打包 `HandoffOffer { spawn_cloned, session, at_tick }` 投目标区；
- **目标区**：tick 顶排水 → `RegionLayout::require_owned` 校验 → 玩家走 `SessionRegistry::enter_map_at`
  （**同一全局 actor id**）、怪/宝宝走落格路径 → `HandoffAck`；校验失败/队列满 ⇒ `HandoffReject`；
- **提交/回滚**（源区）：收到 `Ack` 才移除本地实体（`World::leave_world*`）；`Reject`/超时 ⇒ **不提交**，
  实体停在原格、移动被拒、`handoff_refused += 1`——原子性来自"所有权只在 Ack 后切换，源侧从未放手"；
- **时序**：offer/ack 在 barrier2 之后、下一 tick 之前交换；目标区从 T+1 起模拟，**同一 tick 不会
  双区同时模拟**（两侧屏障同 tick 号）；
- **会话迁移**：`ground_sessions` 随会话移动；`OnlineRegistry` 在目标区重建（route 不变）；
  `ZoneRouter::assign` 更新到新区（与 2.1 同一入口）；
- **离场标记（本卡刚合入）**：源区提交时实体被删 ⇒ 源区观察者应看到"离开"，标记语义正确；目标区
  不做离场标记（实体是新增）。

### 2.3 帧原子性（可验判据；顺序写不清就会在边界出现一帧双现或双缺）

**同一 tick 内的精确顺序（handoff 版）**：

```text
TICK T（源区）：
  phase1：检测跨界 ⇒ 实体置 HandoffPending（仍留在源 World 的原格，不模拟跨界）
          发布源区快照（含该实体）→ barrier1 → 读邻居
  phase2：AOI 扫描（实体仍在原格 ⇒ 源区观察者本 tick 仍看见它）
          发布/交换后 barrier2
  barrier2 之后：源区发 HandoffOffer（全量状态 + 会话 + at_tick=T）

TICK T+1（目标区在 tick 顶、phase1 之前）：
  排水 HandoffOffer → RegionLayout::require_owned 校验 → enter_map_at（同一全局 actor id）
  → 发 HandoffAck
TICK T+1（源区在 tick 顶、phase1 之前，收到 Ack 之后）：
  提交：World::leave_world*（源 World 移除；AOI 离场标记随删除记下）
TICK T+1（两区 phase1）：各自发布本 tick 快照——源区快照已不含该实体；目标区快照含它
TICK T+1（两区 phase2）：源区观察者扫到"格子已空" ⇒ 恰一条离开；目标区观察者扫到实体 ⇒ 恰一条出现
```

**要写死的两件事**：
1. **源区移除必须在 T+1 的 phase1 之前**（这样源区 T+1 快照不含它；否则目标区 `apply_mirrors`
   仍会收到它的快照项，而目标区已有同 id **真实体** ⇒ 今天 `upsert_mirror` 会返回
   `RefusedOccupied` ⇒ `apply_mirrors` 失败即 **abort**）。若时序做不到，则 `apply_mirrors` 必须把
   「镜像 id 已是本区真实体」视为 **handoff 收敛**（跳过 + 计数，不 abort）——二选一，本步选定后在
   代码与用例里写死。
2. **目标区插入必须在 T+1 的 phase1 之前**（这样目标区 T+1 扫描能看到它）。

**验收用例（帧原子性）**：源区观察者恰收到 **一次** 离开、目标区观察者恰收到 **一次** 出现，
且整段窗口内**两侧都不出现"闪一下又回来"**（同一 recog 在任一收件人流中不得出现 出现→消失→出现，
或 消失→出现→消失）。判据用 P1（每收件人有序帧流）+ 逐 tick `VisibleActors`；跑 legacy 与增量
两条 AOI 路径（`MIR2_AOI_INCREMENTAL=0/1` 两个进程级窗口）。

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

- **主指标**：tick 墙钟 p50/p99/p999（目标 **p99 ≤ 200ms**，即一个 tick 周期）；`TICKPROF` 分段
  （`HostPlayerRun` / `HostWorldTail` / `HostAoi*` 等）；
- **区维度**：`region_barrier_us` 的 max（最慢区决定墙钟）与 total（对账）；每区实体数/镜像数
  （受 `MIR2_MAX_HALO_BAND_ENTITIES` 约束）；`mirror_cell_pos_unknown` / `mirror_ai_actions` /
  `cross_hit_delayed`（应恒 0 / 0 / 按注入）；
- **正确性**：outbox `sent==recv`、`dup=0`、`never=0`；`handoff_offered/accepted/refused`；
  `cross_recipient_missing`（第③步已知缺口计数；**容量报告必须标注「跨区收件人帧未合入」；
  面向 Owner 的"1 万在线验收通过"结论必须等该计数为 0**）；`departure_mark_held_past_cap` 恒 0；
- **资源**：进程 RSS 与每区线程 CPU；网关队列深度/背压（`gate_queue`/`gate_backpressure`）；
  登录成功率、登录→首帧时延；关服存档时长（目标 ≤ 60s 收敛）；
- **稳定性**：30min 内无看门狗 abort、无 `Barrier` 超时、无内存单调增长（镜像/离场标记有界）。

### 4.3 窗口与时长

- 真栈（容量测只跑 Rust 栈，不与 C# 并行）+ `botsrv`；10k 会话前先确认网关/DB 连接数与端口容量；
- 每档 ≥ 3 次、报中位数与 p99（`test-count-triple` 同款口径：跑数/通过/失败分开写）；
- 与 Owner 对表的验收句：**单图 1 万在线、tick p99 ≤ 200ms、无丢失/重复帧、关服 60s 内收敛**。

## 5. 第③步不做（登记）

- RNG 切片（战斗/AI 仍共享一条流；容量验收不以逐抽等价为前提）；
- MonGen 全局产生 + 分发（第④步）；
- 静态装载切片（ctor fold/商人/NPC/守卫，第④步）；
- 跨区收件人帧合成（镜像 route 消费，第④/⑤步）；
- 存档按区聚合与关服收口（第⑥步）；
- 城堡地图参与分区（启动门继续排除）。

## 6. 风险与验收边界

- **最大风险 = 屏障活性**：任区 panic/超时 ⇒ 已有失败快传（`TickBarriers::abort`）+ 看门狗 5s abort；
  容量窗口必须记录 abort 次数（应为 0）。
- **正确性判据仍按"能与单区基线逐帧对拍"立**（L2-5 §3 的 A6/A8 口径）；容量窗口不替代对拍。
- 本步交付的是**会话与宿主切片**；"容量"结论只对 4.2 的指标与窗口负责。
