# A43：`HostInbound` 59.17 ms/tick 拆解与候选（只读）

> **状态**：只读分析（2026-11-24，`card/a43-inbound-plan`，base master `6eafd75d8`）。不改产品码。
> 数据：a7 的 tier-1000 真档（1000 在线、树 `6eafd75d8`）：工作段 **80.408 ms/tick**，
> `HostInbound **59.172**（73.6%）`、`HostAoiDownlink 5.870`、`HostWorldTail 5.454`、
> `HostWorldHead 3.721`、`HostPlayerRun 2.052`、`HostPeriodic 1.807`、`HostFrames 0.895`。
> bot 侧 `walk_sent=464194 / 360s / 1000 bots` ⇒ ≈0.26 条/玩家/tick（≈260 事件/tick）。
> **符号锚**：`lib.rs` / `ref_msg_cache.rs` / `chat.rs` / `world.rs` / `monai.rs` 均为对应 crate 文件名
>（高 churn 文件只写符号）。

## 0. 先看一个会把 227 µs 读歪的事实：`HostInbound` 有 **50 ms 墙钟预算**

`lib.rs` `EventDrainBudget::PRODUCTION` = **`max_events=2000` + `max_wall=50ms`**；`EventDrain::next`
在**取事件之前**检查预算：`processed >= 2000 || started.elapsed() >= 50ms` ⇒ 记 `drain_budget_hit`
并把「探到的那一条」结转下一轮，返回 `None`。

⇒ 一个 tick 内 `HostInbound` 的墙钟 ≈ **min(50ms, 把队列排空的时间) + 最后一条事件的处理尾**。
tier-1000 测得 **59.17 ms > 50 ms**，说明两件事之一（或同时）：
1. 该段**每 tick 都撞预算墙**（队列排不空、`inbound_budget_hit` 非 0、`inbound_carry` 非 0）；
2. 截断前最后处理的**单条事件**特别慢（`max_single_event_us` 可以回答：59.17 − 50 ≈ 9 ms 的尾）。

所以「59.17 / 260 ≈ 227 µs/事件」只有在「这 260 条都真的被处理了」时才成立；若墙钟阀先触发，实际吞吐是
「50 ms 能处理多少条」，被延后的事件在队列里累积（容量墙的形态就是**排队变长 + 世界 tick 拉长**，
不是丢事件）。**第一步不需要加仪表：该 run 的 perf TSV 已经有列能判定**（见 §3 的清单）。

## 1. `HostInbound` 段覆盖什么（符号锚 + 代码顺序）

**begin**：`lib.rs` `tick_phase_a` 的 `if phases.has(PhaseSet::INBOUND) { let t = host_prof.begin();`
**end**：同函数尾部 `host_prof.end("HostInbound", t);`（`HostChannelDrain` / `HostControlDrain` 是**另外两个段**，不在本段内）。

段内顺序：
1. **LOCAL 支**（`PhaseSet::INBOUND_LOCAL`，区 k/切片用）：预分类 `classify_local_event` → `execute_local_budget` → 执行体 `handle_zone_command_on`（N=1 宿主本 tick 的 LOCAL 缓冲通常为空）。
2. **PROCESS 支**（N=1/区 0 的命令主路径）：
   `while let Some(ev) = drain.next(&ev_rx, &ev_ctl_rx, &mut event_carry, &mut event_carry_ctl)`：
   - `EventDrain::next`：优先结转 → 普通结转 → 优先通道（反饥饿上限 1000 条）→ 普通通道 `try_recv`；**不阻塞**；
   - `match ev`：
     - `Opened` / `RegisterGate` / `UnregisterGate` / `Closed` / `KickAccount` / `AdminGm` / `LoadDbRequest` 等分支；
     - **`GateEvent::Command`**（1000 bots 走位的主路径）：
       ① 世界就绪门；② **每事件一条 `glog!("DBG", "sim: index=… 命令 …")`**（含 `tail={:?}`；tier-1000 该档 DBG 未开，日志里 0 条 `sim: index=`）；
       ③ `dlgselect_route_for(cmd.ident, world.entities.get(cmd.recog))` —— **对每条命令都算一次**；
       ④ 按 `ident` 分派：`CM_SAY`→`handle_say_event`；`CM_PICKUP/DROPITEM`→`handle_ground_cmd`；bag/storage/trade/party/guild/shop/selloff/eat/script…；**`is_move_command`→`handle_move_cmd`**；
     - 循环结束后：`drain_b4_self_deliveries`、`drain_extra_abil_expiries`、`drain_run_frames`、踢人/存盘落地等（量级远小于事件处理）。
3. **阻塞等待**：本段**没有**屏障/条件变量/DB 等待；唯一「停」是预算墙。区锁：N=1 宿主路径**不持 ZoneState 锁**（`world`/`sessions` 是 `&mut` 局部）。

## 2. 每条走位事件的路径（1000 bots 主路径）

`handle_move_cmd`（`lib.rs`）：
1. `online.find_by_chr` —— **O(1)**（`OnlineRegistry.by_chr` 哈希索引；非旧线性查找）。
2. `world.entities.get(actor_id)` → 读 `(x,y,death)`：**完整 `EntityRef` 构造**（3 次 archetype `get_manual`，24 组件）——AOI 的 T10 Top2 已为同一代价换成轻量视图。
3. `world.entities.get_mut(actor_id)` 写 `dir`（`EntityMut`，同量级）。
4. `world.step_with_route`：
   - `try_step_now_blocked_impl`：逐 step `cell_free_blocked_impl` → `map.get_cell_info` + **遍历该格 `obj_list`**，对每个 actor 对象 `access.get(id)`（再构造 EntityRef）；
   - `route_enter`：读当前格 `obj_list` 找 `CellType::MapRoute`（每格全遍历）；
   - 成功后 `move_object` 改格表；失败分支回滚。
5. `sync_display_dir`（展示表投影，含名字拷贝）；`sessions.get_mut(chr_name)` 更新体力/恢复；
6. `downlink::move_frame(...)` → **一次** `Vec<u8>` 帧体构造；
7. `broadcast_ref_msg` → `broadcast_ref_msg_opt`：
   - `ref_msg_cache::targets*`：**cache-first**。命中（500ms 窗口内）⇒ 克隆缓存 `Vec<i32>` + 按 `|Δ|<11` 过滤；未命中 ⇒ `chat::hear_targets(_at)` **扫 ±12 格（25×25 格）的 `obj_list`** 并重建快照；
   - `send_ref_msg_to_targets`：每个收件人 `online.find_by_actor`（O(1)）→ `tx.send(SimToGate::SendFrame{ body: body.clone() })` —— **每个收件人一次帧体 `clone` + 一次 mpsc send**；镜像收件人走跨区邮箱；
8. `send_good_tick`（+GD，一个单帧）；未到目标格时 `send_movefail`。

**其它事件**：`CM_SAY` 走 `handle_say_event`（普通说话还在其中展开 `broadcast_ref_msg`，`say_recipients` 列可读）；`Closed`/`Logon` 分支在 tier-1000 稳态占比小。

## 3. 数量级对账：先读现有 TSV，再谈仪表

该 run 的 perf TSV **已经**有这些列（`perf_counters::write_tick` 表头）：
`inbound_enqueued / inbound_drained / inbound_carry / inbound_budget_hit / inbound_wall_us / max_single_event_us`、
`refmsg_calls / refmsg_targets`、`say_calls / say_recipients`、`drained_command / drained_close / drained_control`、
`actor_indexed / chr_indexed`（索引 vs 线性探针）、`region_barrier_us`。

**判定顺序（零代码）**：
1. 若 `inbound_budget_hit` ≈ 每 tick 且 `inbound_carry > 0`、`inbound_drained < inbound_enqueued` ⇒ **容量墙 = 预算饱和**，59 ms 的主体是 50 ms 预算；优化目标是**降单条成本**（让 50 ms 能处理更多条），不是「59 里多少是处理」。
2. `max_single_event_us` 若 ~9 ms ⇒ 解释 59−50 的尾；若 ~0.2 ms ⇒ 尾可忽略，59 是预算 + 测量。
3. `refmsg_targets / refmsg_calls` = **每事件平均收件人**。若 ≥100 ⇒ §2.7 的「每收件人 clone+send」是头号候选（1000 bots 聚在走位热区时，25×25 视窗内可达数百人）。

**按代码可给出的成本上界（假设平均收件人 T、事件 260/tick）**：
- 帧体 clone + mpsc send：`260 × T` 次；每次 ~0.1–0.3 µs ⇒ T=200 时 ≈ **13–16 ms/tick**；
- 缓存命中的克隆+过滤：`260` 次 × O(T) ⇒ T=200 时 ~1–2 ms；
- 每 500ms/发送者的全量扫（1000 bots ⇒ ~400 次/tick × 625 格 + 格内实体）⇒ **5–12 ms/tick**；
- `EntityRef` 构造（每事件 ~6–10 次，每次 ~1–3 µs）⇒ **2–8 ms/tick**；
- 单帧构造/展示同步/体力/`+GD` ⇒ 每事件 ~2–5 µs ⇒ 1–2 ms。
合计可到 **20–40 ms**；加预算/调度噪声与 `HostAoiDownlink` 之外的尾部，**59 ms 与「fanout 为主 + 缓存扫描次之」自洽**。

## 4. 候选（按语义风险排序）

### 候选 A（低风险，预期最大）：广播共享帧体 + 批量投递
- `SimToGate::SendFrame.body: Vec<u8>` 在 `send_ref_msg_to_targets` 里**每收件人 clone 一次**。改为**同一帧体共享**（`Arc<[u8]>`/`Arc<Vec<u8>>`，或 gate 侧接受 `Arc` 后自己拆）⇒ 去掉 `260×T` 次分配/拷贝。
- 收益：T=200 时 ~10–15 ms（取决于 clone 实价，用现有 `refmsg_targets` 可线性外推）；语义零改动（字节不变、每收件人一帧、通道 FIFO 不变）。
- 牙：①**逐字节**：同事件同收件人收到的 body 与改造前完全相同（帧面/顺序既有 golden/两区牙全绿）；②**共享不串扰**：发给 A 后改 B 的 body 不影响 A（Arc 不可变，测试用两个收件人对比字节）；③**发送次数**：`refmsg_targets` 与每 socket 收帧数不变。

### 候选 B（低–中风险）：`EntityRef` 热路径减重
- `handle_move_cmd`/`step_with_route`/`cell_free` 每事件多次完整 `EntityRef`（24 组件）。加**轻量访问器**（只需 `map_id/x/y/race/ghost/death/ob_mode/fixed_hide_mode` 时用类似 AOI `map_view` 的单/双 archetype 查询）。
- 收益：~2–8 ms（每事件省 3–6 次全量构造）。
- 语义风险：低（只读同一字段、判定式不变）；牙：移动/阻挡/换图既有用例全绿 + 一个「轻量视图 vs 完整 EntityRef 同值」的字段级等价牙。

### 候选 C（中风险）：收件人扫描与 AOI 结果复用
- `hear_targets` 每 500ms/玩家重扫 25×25 格。AOI 段每 tick 已算 `visible_actors`；可让缓存**复用 AOI 可见集**（或维护每格玩家索引，扫描只遍历 25×25 格的**玩家桶**而不是全部对象）。
- 收益：~3–8 ms（扫格成本下降）。
- 语义风险：中（oracle `VisibleHumanList` 的刷新相位与 AOI 不同：500ms vs 每 tick；复用会改「何时看到谁」的边界）。需引 T-287 的口径并加帧面牙；**建议 A/B 之后再做**。

### 候选 D：完全不动
- 不动预算（`max_wall=50ms` 是防饥饿的安全阀，调大只会把 tick 拉长）；
- 不动帧内容/顺序/可见性；不做「合并多事件为一帧」「延迟广播」类语义改动。

## 5. 结论

**「需要先加仪表」——但先用现有 TSV 列即可把范围缩到一到两个**（§3 的判定顺序；这也是一步「零代码仪表」）。
若现有列仍不足以分配 59 ms：加**细分段**（`b4` 只设计，实现另派）——
- `HostInboundFetch`（`drain.next` 全部，含通道 recv/结转/预算判定）
- `HostInboundMove` / `HostInboundSay` / `HostInboundOther` / `HostInboundCloseLogon`（按分支）
- move 内部子探针：`HostMoveLookup`（online+entity get）、`HostMoveStep`（含 `cell_free`/`route_enter`）、`HostMoveFrame`（帧构造）、`HostMoveTargets`（cache/scan）、`HostMoveSend`（逐收件人 clone+send）
- 计数：每类事件条数、`refmsg_targets` 的**逐事件分布（sum/max）**、缓存刷新次数、每事件 `EntityRef` 构造次数。

优先级建议：**先读 TSV 判定扇出**；若 `refmsg_targets/event` 高，直接做候选 A（低风险、收益与 T 成正比）；B 排第二；C 需语义裁决后再议。


## 6. 附：三处 tier-1000 读数的对账（2026-11-24 追加）

拿到的原列：`inbound_budget_hit` 95.6% 非零、但 `inbound_wall_us` 中位 20.5ms（<50ms）；
`max_single_event_us` 中位 18.9ms；`drained/enqueued = 1.34`。三处都有代码级解释，**其中两处是观测面缺陷**：

1. **`inbound_wall_us` 是单槽全局量、多区共写**：`PhaseSet::INBOUND` 是
   `INBOUND_LOCAL | INBOUND_PROCESS` 的别名，`PhaseSet::has` 是**任一子位**；区 k 的
   `M1B_ZONE_PROD` 含 `INBOUND_LOCAL` ⇒ 区 k 也进入 `if phases.has(PhaseSet::INBOUND)` 块，
   跑完 `INBOUND_LOCAL` 支后**无条件** `note_inbound_wall_us(...)`（`INBOUND_WALL_US.store` 覆盖写）。
   同一 tick「区 0 写 ~50ms+尾、区 k 写它 LOCAL 段 ~20ms」，最后写者赢 ⇒ 中位 20.5ms 与
   `budget_hit`（只有区 0 bump）同时成立。`HostInbound 59.17` 来自 `host_prof`（区 0 单写），仍可信。
2. **`EventTimer` 是墙钟**：起于 `while` 体首行、止于迭代末（含 `continue`），**不含** `drain.next`
   的通道等待；19ms 可能是**日志同步写**、**OS 抢占**（BotSrv/gate/DB 同机）或 say 路径的
   `apply_pending_gm_*` 链，现有列分不出 CPU vs off-CPU；需 CPU 时间或隔离环境对照。
3. **demux 线程复用 `EventDrain::next` ⇒ drained 双计**：`INBOUND_ENQUEUED` 只在
   `emit_gate_event` bump 一次，而 `INBOUND_DRAINED`/`drained_*`/`carry` 在 `EventDrain::next`
   内部无条件记；切片 run 的 `spawn_zone_demux` 路由线程也 `EventDrain::begin/next` ⇒ 全部入站先计
   一次，区 0 再对本区份额计第二次（871k ≈ 651k + 220k）。`RegisterGate/UnregisterGate`、
   `KickAccount`、`AdminGm` 等旁路只进 drained 不进 enqueued。

**影响**：切片 run 的 `inbound_wall_us / drained / carry / drained_*` 不能当区 0 容量证据；可用的是
`HostInbound` 段墙钟与 `max_single_event_us`（均 host 单写）。**应先修这两处观测缺陷**（不改产品语义），
再谈候选 A/B/C 的排序；`refmsg_targets/calls=79` 下「共享帧体」收益有限，不是第一优先。
