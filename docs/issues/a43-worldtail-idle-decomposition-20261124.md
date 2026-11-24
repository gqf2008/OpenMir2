# A43：`HostWorldTail` 空载 67.6 ms/tick 拆解（只读）

> **状态**：只读勘察（2026-11-24，`card/a43-worldtail`，base master `cf2732d93`）。**不改产品码、不跑构建**；结论来自读码 + 既有 idle 产物（`E:/UserTemp/l25-slice/zone-slice-20261124-202609/` 与 `E:/OPC/teststack/rust-root/logs/mir2-server/game-svr.out.log` 的同一窗口行）。
> **符号锚**：`lib.rs` = `mir2-rs/bins/game-svr/src/lib.rs`；`world.rs` = `mir2-rs/crates/world-bevy/src/world.rs`；`region_loop.rs` = `mir2-rs/bins/game-svr/src/region_loop.rs`（高 churn 文件一律只写符号）。

## 0. 先纠正一个前提：这次 idle 不是「没有任何怪物」

同一 run 的 TICKPROF 窗口（`12:31:57` 行，与 `202609` 目录的读数一致）：

- 宿主 `实体内 6635`；`World.SimMonsterAi counts/tick：slot 6616，scanned 6480（inactive 6480），throttle-skip 4322，ran 2158`；
- 区 1 静态装载日志：`MonGen 0/2827 条（过滤 2827）`——**区 1 无怪**，怪全在宿主区（区 0 的 MonGen 正常刷，从启动后持续涨到 ~6.6k）。

⇒「零假人」成立，但**实体不是 0**；本报告里的 O(实体数) 成本都要按 **~6.6k** 读，不能按「空世界应≈0」读。需要一个「真空世界」基线时，当前**没有** MonGen 关闭开关（读码确认无 env），只能改装置；这是后话。

### 0.1 实体规模事实块（原始行 + 完整路径）

**日志**：`E:/OPC/teststack/rust-root/logs/mir2-server/game-svr.out.log`（该 run 的窗口行；行号按当前文件）
**perf TSV**：`E:/UserTemp/l25-slice/zone-slice-20261124-202609/perf_counters.tsv`
**run 元数据**：`E:/UserTemp/l25-slice/zone-slice-20261124-202609/run_identity.json`、`per_zone.json`

| 事实 | 原文行（精确引用） | 位置 |
| --- | --- | --- |
| 宿主实体数 6635 | `[INF] [TICKPROF] 窗口 50 tick：整 tick 均值 200.35 ms/tick（墙钟合计 10.02s，实体内 6635，门控 MIR2_TICK_PROFILE=1）` | 上列 log，`12:31:57.668` |
| SimMonsterAi 扫描 6480 / 6 616 | `counts/tick：slot 6616，scanned 6480（inactive 6480），throttle-skip 4322，ran 2158（dead 0 / frozen 159 / no-target 2151 / idle-wander 2151 / walk-due 884 …` | 上列 log，`12:31:57.675` |
| 区 1 MonGen 全过滤 | `[INF] 区 1 静态装载完成：MonGen 0/2827 条（过滤 2827）；商人 1（解析前过滤 130）；固定 NPC 0/3 条（挂图 0）；守卫 0/49 条（放置 0）` | 上列 log，`12:26:15.744` |
| 镜像**已启用** | `[INF] L2-5 ② 镜像已启用（MIR2_ZONE_MIRROR=1）：barrier1 之后、AOI 之前用邻区本 tick 快照刷新镜像（每区 id 段 ⇒ 同一全局 ActorId）` | 上列 log，`12:26:15.187` |
| 区 0 **不发起**跨界移动 | `[WRN] 区 0 多 span（或多段边界不明）⇒ 不设 handoff 边界（区 0 不发起跨界移动）` | 上列 log，`12:26:15.273` |
| 镜像 / handoff 活动全 0 | perf TSV 首数据行（tick 0）列 `handoff_offered/accepted/refused/promoted/inserted_fresh/dup_dropped=0`、`mirror_shadowed_by_local/static/handoff=0`、`mirror_write_refused_cell/occupied=0`、`mirror_own_segment_ids=0`、`handoff_fresh_own_segment=0`；同列 `zone_metrics.handoff={"canceled_trades":0,"session_failed":0,"session_moved":0,"target_display_missing":0}` | 上列 TSV 第 1 数据行 |
| 区 1 在等宿主 | `per_zone.json` 区 1 `barrier`：`p50=199929`（µs）、`mean=199976.9` | `E:/UserTemp/l25-slice/zone-slice-20261124-202609/per_zone.json` |

**镜像数 / `handoff_pending` 数本身没有导出**（没有对应 TSV 列或日志行）：镜像只有「shadowed/refused」事件计数；handoff 只有 `offered/accepted/refused/promoted`。可用的**构造级**代理是：区 0 不设 handoff 边界 ⇒ 宿主 `handoff_pending ≡ 0`；区 1 无怪（MonGen 全过滤）⇒ 邻区快照里的实体只有极少数静态项，镜像集合必然很小。**真正的计数需要 §4 的仪表**（`mirror_count` / `handoff_pending_count`），这也是为什么结论是「先加仪表」。

另：`per_zone.json` 区 1 `barrier p50 = 199929 µs`（≈200ms）——**区 1 在等宿主**，不是区 1 自己算 200ms；宿主是慢的一侧。

## 1. 段边界与调用序列（按代码顺序）

`HostWorldTail` 的 begin/end（切片路径）：

- **begin**：`lib.rs` `tick_phase_a` 的 `PhaseSet::WORLD_TAIL` 分支 `let t = host_prof.begin();`
- **end**：`lib.rs` `tick_phase_b` 末尾 `self.host_prof.end("HostWorldTail", at);`（`at = scratch.world_tail.take()`）

⇒ 段计时**跨 driver**：A 段尾 + `await_neighbors`（barrier1）+ B 段前半。区 0（host）`inline_after_phase2=true`，`after_phase2` 与 `handoff.after_phase2` 也在段内；区 k 的段含 barrier 等待、不含 `after_phase2`（`lib.rs` `tick_phase_b` 的 4 组合表）。

段内依次调用（host，切片；符号 = 函数名）：

1. `world.tick_tail_before_aoi(&mut rec)`（`world.rs`）
   - 阶段：`TailSuperManRefill` → `TailPlayerDeaths` → `TailVerifyMapTime`（World profiler 计时）
2. `publish_snapshot(...)`（`lib.rs` `BarrierOps::publish_snapshot`）
   - → `region_loop.rs` `ZoneRuntime::snapshot_publish` → `snapshot_zone_border` → **`snapshot_border`**
   - → `HaloExchange::publish(zone, border.clone())`
3. driver：`await_neighbors(tick)`（barrier1 + 读邻区快照）
4. `cross_zone_frames::drain_zone(...)`
5. `ops.before_phase2(&mut world, &mut displays, tick)`（`BarrierOps::before_phase2`）
   - → `region_loop.rs` `apply_mirrors`（含**陈旧镜像清扫**）
   - → `zone_mirror_diag::maybe_write_world`（env 未设 ⇒ 立即 return）
6. `world.tick_phase2(&mut rec)`（`world.rs`）→ `tick_tail_aoi`
   - `TickStage::OutputAoi`（World profiler 计时）
   - `assert_no_entity_on_unowned_map()`（**release 第一行 return**）
   - `self.last_tick = record.clone()`
7. `ops.after_phase2(beat)`（心跳 + barrier2 + `note_region_barrier_us_for_zone`）
8. `handoff::HandoffRuntime::after_phase2(&mut world, tick)`（`handoff.rs`）

## 2. 逐项：这个实体规模下为什么仍耗时

### 2.1 三个「全实体扫描」（每 tick × 每区各一次）

| 扫描 | 出处 | 干什么 | 空载时实际需要吗 |
| --- | --- | --- | --- |
| S-a `entities.iter_ref()` | `region_loop.rs` `snapshot_border` | 找本区 rect 边界带 ∪ outgoing 带实体，再构造 `HaloEntity` | **需要**（快照口径），但**不必须全表扫**：边界带 = 固定格集，可按格取 `obj_list`，或先过一遍轻量视图（见 §4） |
| S-b `world.mirror_ids()` | `region_loop.rs` `apply_mirrors` 尾部（陈旧镜像清扫） | 拿到**本区全部镜像 id**，不在本 tick 邻区快照里的删除 | **空载完全不需要**：镜像数为 0 时这次全表扫纯浪费；镜像集合本可由 upsert/remove 增量维护 |
| S-c `entities.iter_ref()` | `handoff.rs` `HandoffRuntime::after_phase2` | 收集 `handoff_pending` 实体发 offer | **空载完全不需要**：`pending_since`/标志位为 0 时不应扫全表；可维护 pending 集合 |

`EntityStore::iter_ref()`（`world-bevy/src/entity.rs`）不是轻量视图：它对每个实体做 **3 次 archetype `get_manual`**（15+4+5 组件）并构造完整 `EntityRef`；AOI 段正是因为这条路径贵，已换成「只查 `Transform` 的 `map_view`」（`aoi.rs` `output_aoi_system` 的 T10 Top2 注释：**旧实现为拿 `map_id` 构造完整 24 组件 `EntityRef`**）。同一模式仍留在 S-a/S-c/S-b 里，且 **每 tick 每区各来一次**。

### 2.2 O(实体数) 但已被计时、且当前不大

- `TailSuperManRefill`/`TailPlayerDeaths`/`TailVerifyMapTime`：窗口读者 0.25–0.37 ms/tick（`TailVerifyMapTime` 有 30s 门，空载只扫格表）。
- `World.OutputAoi`：0.88 ms/tick（同样 6.6k 实体，但走 SlotTable + 单 archetype 视图）。
- `World.SimMonsterAi`：4.98 ms/tick（6.6k inactive 怪的 collect+gate），属 tick 前半（`HostWorldHead`），不在本段。

### 2.3 单位实体口径（把 76.0 ms 换算成可比较的量）

`76.001 ms / 6635 实体 ≈ 11.46 µs/实体/tick`。**这不是单次扫描成本**：它是本段对「每 tick 全部实体」
做**多轮**扫描（S-a/S-b/S-c 至少三轮，另加各 World 段自己的 O(N) 遍历）的**合计**。换算用途只在于：
改实体规模（如 10k 目标里玩家也是实体）时，这些扫描按 O(N)×轮数 近似放大——这正是本卡要盯「每 tick 固定成本」
之外的真正风险：**它不是与负载无关的常数，而是随实体数线性膨胀的基数**。

### 2.4 小项开销 / 分配（与实体数弱相关）

- `snapshot_zone_border`：每 tick `HaloSnapshot` + `entities Vec` 新建；`exchange.publish` 再 `clone()` 一次（band 小时小）。
- `apply_mirrors`：每 tick `HashSet` + `ordered: Vec<&HaloEntity>` + **`sort_by_key`**（邻居带实体数 k；空载 k≈1）。
- `handoff::after_phase2`：每 tick `ids: Vec<i32>`；S-c 的扫描是主体。
- `tick_tail_aoi`：`self.last_tick = record.clone()`（`aoi_order` ≈ 6.6k 个 id 的 Vec + 其它小向量；µs 级）。
- `zone_mirror_diag::maybe_write_world`：env 未设 ⇒ **不**调 `mirror_ids()`（有牙），可忽略。

### 2.5 对账缺口

窗口日志自带对账行：

```
对账：HostWorldHead+HostWorldTail = 106.613 ms/tick；ΣWorld.* = 9.599 ms/tick；宿主附加（专用怪产仔/ref 等）= 97.015 ms/tick
```

即 **97 ms 在 `HostWorldHead`/`HostWorldTail` 的宿主侧代码里、不在任何 World 段计时内**（这是「约 6635 实体规模下的每 tick 成本」，不是「与负载无关的固定成本」）。本段内的宿主侧代码就是 §1 的 2/3/4/5/7/8 六个调用；其中 S-a/S-b/S-c 三次全表扫与 `apply_mirrors` 的排序/分配是唯一的 O(6.6k) 候选。**代码事实能把「为什么 O(实体)」说清，但无法仅凭读码把 67.6 ms 按调用拆开**——缺的正是 2/3/4/5/7/8 的段计时（§4）。

## 3. 可疑项排序（按「空载可避免的浪费」）

1. **S-b `mirror_ids()` 全表扫（`apply_mirrors` 尾部）** — 空载镜像恒 0，**纯浪费一次全表扫**；修法简单（增量镜像 id 集合，或 `mirror_ids` 空集时跳过清扫），**不影响 oracle 语义**（清扫结果集不变）。
2. **S-c `handoff::after_phase2` 全表扫** — 空载 `handoff_pending` 恒 0，**纯浪费一次全表扫**；改为维护 pending 集合（标志置位/清位处同步），**不影响语义**。
3. **S-a `snapshot_border` 全表扫** — 语义上必须产出「本区边界带 ∪ outgoing 带」快照；**可换实现**（按边界格 `obj_list` 取，或先用 `Transform` 轻量视图过滤再取完整 EntityRef）。风险在**顺序/内容等价**（同格序、outgoing 追加规则），需要两区对拍牙兜底。
4. `apply_mirrors` 的排序/分配 — k 小，空载不是大头；保留。
5. `exchange.publish` 的 `clone`、`last_tick` clone、`ids`/`ordered` 分配 — µs 级，先不动（若 §4 计量后显示占比再议）。
6. barrier 等待 — 区 0 µs 级（`barrier p50=3µs`）；区 1 的 200ms 是**等宿主**的结果，不是本段成本来源。

oracle 语义要求、不能动的：快照**内容/顺序**、镜像清扫**结果集**、handoff offer **发送集合与 epoch**、`TickRecord` 字段语义。

## 4. 可测化建议（只设计，不动手；由 `a7` 出 instrumented 版跑空闲窗）

在 `lib.rs` 的 host 切片分支（`barriers.is_some()`）加 host_prof 段（N=1 不进这些分支 ⇒ 既有 N=1 输出零变化）：

| 段名 | begin/end 位置 |
| --- | --- |
| `HostTailBeforeAoi` | `tick_phase_a`：`world.tick_tail_before_aoi` 前后 |
| `HostHaloSnapshot` | `publish_snapshot(...)` 前后 |
| `HostBarrier1` | driver `await_neighbors` 前后（与 `barrier_us` 对账） |
| `HostMirrorApply` | `tick_phase_b` 的 `ops.before_phase2(...)` 前后 |
| `HostAoiPhase2` | `world.tick_phase2(...)` 前后（内含 `World.OutputAoi`，两者差值 = 宿主侧包装成本） |
| `HostAfterPhase2` | `ops.after_phase2(...)` 前后 |
| `HostHandoffAfter` | `handoff.after_phase2(...)` 前后 |

外加最小计数（每窗口打印一次，默认关或仅 `MIR2_TICK_PROFILE=1` 时打印）：
- `scan_full_entities{snapshot,mirror_ids,handoff}` 次数与每次的 `entities_total`；
- `halo_entities`（本区快照条数）、`mirror_count`、`handoff_pending_count`。

判定：instrumented idle 360s 一轮即可回答「67.6 ms 落在哪几段」；若 S-a/S-b/S-c 三段合计 ≥ 50ms，按 §3 的 1→2→3 顺序做（1、2 可先做，语义零风险；3 需两区快照对拍牙）。

## 5. 结论

**「需要先加仪表才能判断」**（三选一里的第三项），按**两段式**走：

1. **先加仪表定量**：由 `a7` 出 instrumented 版，在空闲窗口跑 360s，回答「S-a/S-b/S-c 三个全实体扫描
   各占多少毫秒、各自扫了多少实体」。本报告 §4 已给出 7 个段 + 3 个计数的落点；在此之前，任何
   「先删哪一段」的排序都只是代码推测，不是读数。
2. **再按占比决定优化顺序**：`mirror_ids` 清理（S-b）与 `handoff` 扫描（S-c）即便占比不高也值得做——
   空载/无镜像/无 pending 时它们是**纯浪费的全表扫**；但**必须先由仪表确认它们没有承担别的语义**
   （例如 S-b 的清扫是否还承担了「邻区快照过期」的兜底、S-c 的扫描是否还承担了 pending 集合的公平性），
   再按「语义零风险 → 需要两区对拍牙 → 大改」的顺序动手。

补充：代码事实足以指出这三个扫描与「完整 `EntityRef` 构造贵」的先例（AOI 的 T10 Top2），也解释了
为什么 97ms 只能落在宿主侧；但读码**分不出** 67.6 ms 在三者之间的分配，本次 idle 也没有「真空世界」
基线（有 ~6.6k 实体）。所以结论是待仪表，而不是现在就承诺收益。

## 6. 边界

- 无运行时实验；未改任何代码/装置；未动 `E:/UserTemp/l25-slice/**` 与 `E:/OPC/tools/**`（只读日志与产物）。
- 引用的日志窗口行与 `202609` run 的 `per_zone.json` 对应（宿主 work p50 75–110ms、HostWorldTail 67–99ms；区 1 barrier≈200ms）；日志时间戳为当时进程时钟，与目录名的小时数不同不构成两个 run。
- 未覆盖 N=1（`barriers=None` 整段 `world.tick_tail` 在 A 内）与多区镜像非空形态。
