# A43：`HostHaloSnapshot` 61.25 ms/tick 优化方案（先方案、不写码）

> **状态**：只读方案（2026-11-24，`card/a43-halosnapshot-plan`，base master `992575d0d`）。**不改产品码**；窗口读数来自 a7 的 360s instrumented 档（树 `992575d0d`、exe `d27d56d6…`）。
> **符号锚**：`lib.rs` / `zone.rs` / `region_loop.rs` / `tick_profile.rs` 均为 `mir2-rs/bins/game-svr/src/` 下的文件名；高 churn 文件只写符号。

## 0. 读数与更正

- 7 段合计 62.929 ms ≈ `HostWorldTail` 62.9495（残差 0.02）；其中 **`HostHaloSnapshot` 61.2535 ms（86.13%）**，其余全在 0.7 ms 以内。
- 扫描/规模（宿主窗口末次）：`snapshot_scans=16050`（50 tick ⇒ **321 次/tick**）、`entities_total=6867`、`halo_entities=500`、`mirror_count=0`、`handoff_pending_count=0`。
- **更正我此前的两项排序**：`mirror_ids()` 清扫 0.1545 ms（0.22%）、`handoff` 扫描 0.1520 ms（0.21%）——**都不是大头**；真凶是快照发布的多 span 全表扫。无仪表时读码确实分不出占比，这正是仪表的意义。

## 1. 321 次/tick 是什么（代码事实）

- `zone.rs` `parse_zone_layout` → `RegionLayout::from_zone_spans(…, default_zone)`：**`MIR2_ZONES` 只显式写了两张图**（`0:GA0=…`、`1:D701=…`），其余**每张注册图**都追加一个「整图 rect、`default_zone`(=区 0)」的默认 span。本 run 注册表 322 张图 ⇒ **区 0 有 321 个 span**（GA0 + 320 张默认图），区 1 有 1 个。
- `region_loop.rs` `ZoneRuntime::snapshot_publish` → `snapshot_zone_border`：
  ```
  for span in spans.iter().filter(|s| s.zone == zone) {   // 区 0：321 次
      entities.extend(snapshot_border(world, span.map_id, span.rect, halo, tick, source).entities);
  }
  ```
- `region_loop.rs` `snapshot_border` **每次调用都 `world.entities.iter_ref()` 全表扫一遍**（`iter_ref` 每实体 3 次 archetype `get_manual` + 完整 `EntityRef` 构造），再用 `rect.border_contains` / `handoff_pending` 过滤。
- ⇒ 每 tick 实体访问量 ≈ `321 × 6867 ≈ 2.20 M`；`halo_entities=500` 说明**绝大多数 span 的带宽为空**，扫描绝大多数是白跑。
- `500` 与 `6867` 的关系：6867 是**宿主整个世界**的实体数（主要在 GA0；其余默认可图只有少量静态守卫/NPC）；500 是**本区所有 span 的边界带实体合计**（窗口末次）。每个 span 只应产出它自己那张图的边界带实体。

## 2. 优化候选（按语义风险排序）

### 候选 1（推荐）：**单遍分桶**——按 `map_id` 桶化实体，每 span 只扫自己图的桶

- 做法：`snapshot_zone_border` 先对 `world.entities.iter_ref()` **做一次**遍历，按 `map_id` 放入 `HashMap<usize, Vec<EntityRef>>`（保持槽位序）；随后按原 span 顺序，对每个 span 只遍历 `bucket[span.map_id]` 并施加**同一套** `border_contains / within_distance` 过滤与 `HaloEntity` 构造。
- 复杂度：`O(实体数 + Σ_span 该图实体数)`；空图 span 桶为空 ⇒ **零实体访问**（顺带覆盖候选 3）。
- 语义影响：**零**。每个 span 的实体集合与顺序都保持不变——旧实现是「按槽位序全表扫 + `map_id == span.map_id` 过滤」，新实现是「按槽位序入桶 + 同过滤」；`iter_ref` 的顺序即槽位序，入桶追加保持该序；span 拼接顺序不变 ⇒ `HaloSnapshot.entities` 逐项相同（含 `cell_pos` 口径）。
- 预期收益：61.25 ms 中 ≈ **59–61 ms**（只保留一次 `iter_ref` + 500 次构造；量级参考：旧实现单 span 扫全表 = 61.25/321 ≈ 0.19 ms）。
- 风险/工作量：小。`snapshot_border`（单图版）保留给测试与单 span 调用；把「过滤 + `HaloEntity` 构造」抽成一个私有 helper，两条路径共用（单一实现，防漂移）。
- 牙：
  - **等价性**：多图/多 span 世界，逐 span 断言实体集合与**顺序**（槽位序）等于「单图全表过滤」的期望；含一个**空图 span**（桶空 ⇒ 空快照）与一个**无边界实体 span**（有实体但不入带 ⇒ 空快照）。
  - **反例（防"跳过过头"）**：在非首图（非 GA0）的边界带放一个实体 ⇒ 必须出现在对应 span 的快照里；`handoff_pending` 的 outgoing 带仍按 `within_distance` 收入。
  - **访问量牙（可变异）**：把「实体访问次数」计数（见 §3）作为判据——2 个 span、1 个实体 ⇒ visits 必须 = 1；把实现退回「每 span 全表扫」⇒ visits = 2 ⇒ 红。

### 候选 2：只扫边界带（按格 `obj_list` 取）——次选，风险中

- 做法：对每个 span，遍历其边界带格（`rect` 四边 × `halo_width`），从 `map.obj_list` 取候选实体，再过滤/构造。
- 收益：与候选 1 同量级或略好（候选 1 的 `iter_ref` 一次 ≈ 0.2 ms）。
- 风险：**顺序**。当前快照强调「同一槽位序（`cell_pos` 用于跨格定序）」，按格取会变成「格序 + 格内序」；镜像的格内追加序（`outgoing` 规则）依赖这个序。要引 §9.1.2 的同格序裁决、并配「两区快照对拍」牙才敢动。**建议候选 1 落地并复测后再评估是否需要它。**

### 候选 3：空图 span 跳过 —— 被候选 1 覆盖，无需单列

- 候选 1 的桶化天然使「该图无实体 ⇒ 桶空 ⇒ 不访问任何实体」；单独做「跳过空图」只是它的子集，且同样零语义影响。

### 候选 4：按需发布 / 降频 —— **不建议**（语义改动需新裁决）

- 「邻区无观察者就不发布」：需要跨区「本区是否有玩家」的共享信号；`halo` 快照同时喂镜像与 handoff 的 `outgoing` 带，跳过会让**镜像状态**与当前 tick 的邻居读取出现时间差；且「无观察者」随登录/下线逐 tick 变化，容易在边界 tick 制造可见性闪差。
- 「降频 N tick」：现语义是「跨区可见延迟 ≤1 tick」（已登记偏离），放宽到 N tick 是**新的语义偏离**，必须引 D1/新裁决，不能为了指标改。
- 两者都**不解决根因**：根因是「321 个 span 各扫全表」，候选 1 已经把这块消掉。

## 3. 仪表契约更新（复测可比）

- 现有 `snapshot_scans` 语义 = **span 数**（321/tick），不是实体访问量；复测时它不会变，无法直接体现优化。
- 建议增一个真实计数 `snapshot_entity_visits`（本窗口实体访问次数），窗口行「扫描/规模」加该字段；`snapshot_zone_border` 返回 `(HaloSnapshot, spans, visits)` → `snapshot_publish` → `publish_snapshot` → `note_snapshot_scan(…, visits)`。
- 复测判据：优化后 `snapshot_entity_visits/tick` 应从 ≈2.20 M 降到 ≈6.9 k（6867 一次 + 500 次构造侧访问），`HostHaloSnapshot` 从 61.25 ms 降到 **≤2 ms**；其余 6 段不应劣化（±0.1 ms 内）。

## 4. 对既有牙的影响面（必须全绿才交付）

- `region_loop.rs` 现有：`two_zones_exchange_this_tick_border_snapshots`、`missing_neighbor_snapshot_is_an_error`、镜像可见性/flags/同格序、`snapshot` 过滤 mirror 等。
- `lib.rs` 跨区可见性/镜像接线牙（L2-5 ②/③/⑤/⑦）、`check_band_gate`（启动门用同一函数；优化后启动门也变快，但必须仍对**每区**带宽实体计数）。
- 新增：§2 候选 1 的等价性/反例/访问量牙；关闭 `MIR2_TICK_PROFILE` 时行为不变（本次优化与 profiling 无关，但复测需同时证明）。

## 5. 结论

**做候选 1（单遍分桶），不做 4（语义改动），候选 2 留到复测后按需。** 预期把 61.25 ms/tick 压到 **≤2 ms**（`HostWorldTail` 62.9 → ~3.5 ms，工作段 71.1 → ~11 ms），语义零改动；实现 + 等价牙 + 访问量牙可在一次提交内完成，再由 a7 用同树 baseline 复测同 360s 档对比。
