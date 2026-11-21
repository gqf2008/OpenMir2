# L2-3 最小实现提案：两阶段 tick + 边界 halo（构造级）

> 状态：**提案（等审，无代码）**。日期 2026-11-22。基线 master `db2ae9254`。
> 依据：`docs/issues/l2-zone-split-plan-20261122.md` §2.2/§4（L2-3）。
> 审阅重点（Leader）：**阶段 2 的 AOI 用到的实体状态必须已在本 tick 更新完**——见 §1 末尾。

## 0. 一句话

把 `World::tick` 在「所有模拟系统跑完」与「AOI 出帧」之间切开；两区各自跑完 phase1 后交换
**本 tick 的**边界实体快照，再各自跑 phase2。L2-3 只做**可见性对拍**（构造级、双区、同图），
**不改生产帧投递**。

## 1. 两阶段接缝（函数级）

现状：`tick()` = `tick_head()` + `tick_tail()`；`tick_tail` 依次
TailSuperManRefill → TailPlayerDeaths → TailVerifyMapTime → **AOI 块** →
`assert_no_entity_on_unowned_map` → `last_tick = …`。

改法（`crates/world-bevy/src/world.rs`）：

- `pub fn tick_phase1(&mut self) -> TickRecord`：`tick_head()` + TailSuperManRefill /
  TailPlayerDeaths / TailVerifyMapTime（= 原 `tick_tail` 的 AOI 之前部分）。
- `pub fn tick_phase2(&mut self, record: &mut TickRecord)`：原 AOI 块 + 不变式 + `last_tick` 更新。
- `pub fn tick(&mut self)`：`let mut r = self.tick_phase1(); self.tick_phase2(&mut r); r`——
  **阶段序逐条不变**；`tick_tail` 保留为「phase1 尾段 + phase2」的组合兼容入口。
- 阶段归属：phase1 = Clock / Command / Sim* / OutputDrain / TailSuperManRefill /
  TailPlayerDeaths / TailVerifyMapTime；**phase2 = OutputAoi 单阶段**（仅此一个系统；
  `STAGE_ORDER` 不改）。

**审阅重点的回答**：phase1 覆盖所有写位置/属性/HP 的段（移动、AI、buff、死亡结算、
VerifyMapTime 钳制），快照在 phase1 **之后**取 ⇒ 阶段 2 的 AOI 看到的是**本 tick 终态**。
跨区命中按设计走「目标区下一 tick」（1 tick），不在本 tick 的可见性面内。

## 2. 屏障交换什么（格式/所有权/清空）

- 交换物：`HaloSnapshot { tick: u64, entities: Vec<HaloEntity> }`；`HaloEntity` = AOI 扫描与
  出帧所需字段（`actor_id, map_id, x, y, dir, race/race_img, name(Arc<str>), feature,
  hp/mp/max, status/feature_ex, add_to_mapped`）。字段集以 `OutputAoi`/`appear_frames` 实际
  读取为准；**加字段必须同步本结构**（L2-3 用一条「字段清单」测试钉住）。
- 范围：**边界带**（距区界 ≤ `view_range` 的已落格实体）**每 tick 全量**，不做增量。
  理由：全量最不容易漏（HP/外形变化也必须反映）；带内实体数有界（需实测）。增量优化留到有测量后。
- 所有权：**owner 区只写自己的槽**；邻居只读。槽按 `tick & 1` 双缓冲，避免下一 tick 覆盖时仍被读。
- 同步：每 tick **两个 `std::sync::Barrier`**：① phase1 后发布 → wait → 读邻居快照 → phase2；
  ② phase2 后 wait（保证没人还在读上一槽，下一 tick 才能覆盖）。N≤16 时 barrier 成本可忽略。
- 清空：槽按 tick 覆盖写（不清空）；`HaloSnapshot.tick` 用于断言「读到的就是本 tick」。
- 跨区命中：独立 `CrossZoneIntent { tick, src, dst, seq, attacker, target }` 队列，
  **目标区下一 tick 的 phase1 开头**消费（1 tick 延迟，与计划一致）；不混进 HaloSnapshot。
  L2-3 只落类型 + 顺序单测。

## 3. 本步可验证形态（构造级，单图双区）

- **场景**：20×10 图；界 x=9|10；区 A=[0,9]、B=[10,19]；`view_range=3`。
  实体：观察者 O 在 A(9,5)（有会话）；A 侧静止 E_A(8,5)；B 侧静止 E_B(10,5)；
  B 侧移动 M_B 在 (10,5)↔(12,5)，脚本在第 3 tick 移出 O 视野。
- **驱动**：两个线程各跑 `phase1 → 发布 → barrier → 读镜像 → phase2 → barrier`；
  基线 = 单 World 全区跑同一脚本。
- **断言（每 tick）**：
  1. O 的**可见 actor 集合**与基线相同（含跨区实体）；
  2. O 的 appear/disappear **有序序列**与基线相同；
  3. 跨区实体**恰好一个发射者**（owner 区）：合并两区的可见性事件后
     `sent==recv`、`dup=0`、`never=0`；
  4. **陈旧镜像牙**：把快照改成 phase1 **之前**取（上一 tick）⇒ 第 3 tick 的 disappear 晚一拍
     ⇒ 断言 1/2 红（直接钉住 Leader 的审阅点）。
- L2-3 **不改生产 `OutputAoi` 的帧投递**：可见性对拍用一个复用同一扫描谓词的 helper
  （新增 `pub` 函数，测试与未来系统共用）；跨区帧投递/命中接线在 L2-4。

## 4. N=1 仍逐字节

- `zone_count == 1` ⇒ 不建 barrier、不建线程、不取快照；驱动侧直接 `world.tick()`
  （与 master 同路径）。
- 单测：同一 seeded World 上 `tick()` 与「显式 phase1+phase2」的 `TickRecord` 与帧字节
  逐项相等（保阶段序不变）。
- 常规证据：五门 + `-Mode Both` 150/50 + N=1 归一化 stdout（方法同 P2-3b Step 4）。

## 5. 明确不做（L2-4）

- 生产 `OutputAoi` 的跨区收件人扩展与跨区帧投递；
- 跨区命中接线（只落类型/顺序单测）；
- 区界 handoff、入站 demux、每区线程；
- 多区启动/布局文件/覆盖 100% 校验。

## 6. 风险

- **字段漂移**：出帧新增字段未镜像 ⇒ 用 §2 的字段清单测试；L2-4 再加「读字段即断言」的守卫。
- **边界带规模未知**（全量快照的 memcpy 成本）⇒ L2-3 先测带内实体数；过大再讨论增量。
- **双 barrier 对不均衡区敏感**（Amdahl）⇒ 与计划 §6 R4 一致；L2-3 只证明正确性，不承诺吞吐。
