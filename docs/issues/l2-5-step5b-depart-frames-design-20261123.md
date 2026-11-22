# L2-5 ⑤-第七面 设计：离世帧跨区投递（`DepartFrames`，2026-11-23）

> 状态：**设计稿 → 实施中（未编译）**。基线：当前 master `000f09346`（含 ⑤、perf-index 迁移、docs-only 复测）。
> 决策（opc-27 2026-11-23）：**做第七面**，不走「接受 AOI 覆盖」。理由见 §0。
> 实施：本文审过后再写码；a7 实验窗口结束后才编译。**0c 复核**。

## 0. 为什么必须做第七面（调查结论）

`drain_depart_frames` 对「观察者是镜像/无网关坐标」逐个 WRN + 跳过 `SM_DISAPPEAR(30)`，实测约
**305 行/秒**。收件区的本地 AOI **部分覆盖**这条消失（`remove_mirror` → `record_departure_of`
→ 观察者下一次 `search_view_range` 的 `departure_mark` hide → `drain_aoi_disappear`），但与
oracle 的 `DisappearA` 有三条硬差距：

| 差距 | oracle `DisappearA`（`drain_depart_frames`） | 收件区本地 AOI |
| --- | --- | --- |
| **集合** | 离开时刻 **±12 格扫**（`session.rs` 的 `send_ref_msg_targets_at`；含上次扫描后才进 ±12、不在 `visible_actors` 的玩家） | 只覆盖 `visible_actors` ⇒ **可能是子集** |
| **时序** | 离开当轮**即时**（不依赖观察者 `SearchViewRange`） | 观察者下一次扫描（`SearchTime=1000ms`，200ms tick ⇒ **≤~1.2s**） |
| **ghost/fixed-hide** | 不抑制（观察者本身 `!Ghost`） | `departure_mark` 的 `!ghost && !fixed_hide_mode` **抑制** |

任何一条都会在「同屏可见性 / 帧时序」硬面上留证据；⑤ 的机器（邮箱/三桶/门禁）已经在了，加第七面
的边际成本远小于登记一条永久偏差。**结论：第七面。**

`merchant_disappear` 的 else 支**不是**同一类：那里只报**非镜像**未命中（镜像已走 ⑤）；离世 WRN
**不判 mirror**，镜像观察者正是它要报的。两者「无网关⇒跳过」语义像，判据与生产点不同。

## 1. 生产点与帧体（`drain_depart_frames`，`lib.rs`）

### 1.1 改造形态（伪代码，逐字节与本地路径同一构造）

```text
fn drain_depart_frames(world, online, gates) -> usize {
    for (observer_id, target_id) in world.drain_departs() {
        let body = downlink::disappear_frame(target_id);      // 唯一构造函数（提前，本地/跨区共用）
        if let Some(observer) = online.find_by_actor(observer_id) {  // 当前 master 已是索引版
            // 本地路径：不变
            route_to_gate(...)?.send(SimToGate::SendFrame { socket, body });
            sent += 1;
            continue;
        }
        if world.entities.get(observer_id).is_some_and(|e| e.mirror) {
            // 第七面：镜像观察者 → 目标区邮箱
            let outcome = crate::cross_zone_frames::note_mirror_recipient(
                perf_counters::CrossRecipientSurface::DepartFrames,
                world.mirror_route(observer_id),
                std::slice::from_ref(&body),
            );
            if outcome != Delivered { warn_depart_rate_limited(observer_id, outcome); }
            continue;
        }
        // 非镜像 / 离线 / 克隆等残差：WRN 限流（§3）
        warn_depart_rate_limited(observer_id, Unroutable);
    }
    sent
}
```

- **帧体逐字节**：本地与跨区都用 `downlink::disappear_frame(target_id)`；跨区帧存的是同一个
  `Vec<u8>`（`body` 在本地 send 里被 move、在镜像分支里 `from_ref` 后由 `note_mirror_recipient`
  克隆；两分支互斥）。用例断言「跨区收到的 body == 本地路径 body == `downlink::disappear_frame`」，
  **不许各写一遍**。
- **`note_mirror_recipient` 返回 `CrossRecipientOutcome`**（`()` → 返回值；现有六面调用方忽略返回，
  行为不变）。第七面据返回值决定是否打限流 WRN（`Delivered` 不打；其余打）。
- **`route=None`**：`note_mirror_recipient(..., None, ...)` 计 `unroutable`；WRN 限流。
- **顺序**：`drain_depart_frames` 在源区 `world.tick()` **之前**（现有调用点不变）⇒ 帧在源 tick T
  产生；收件区 `barrier1(T+1)` 排水（V2，`src_tick < 当前 tick` 过滤）⇒ **T+1**。与其余六面同相。

### 1.2 集合与 AOI 并存（不新增重复类）

- 第七面补的是 **`drain_departs`（离世广播）** 这条即时路径；收件区的 AOI hide 仍是**既有**扫描
  路径（镜像**离带**、以及扫描可见的删除）。
- N=1 同区观察者本来就可能收到**两条不同机制**的帧：`DisappearA`（即时）+ `SearchViewRange` 的
  hide（下一扫，且 `!ghost && !fixed_hide` 才发）。第七面把**同一条 `DisappearA`** 补到跨区观察者，
  收件区的 AOI hide 原样保留 ⇒ 跨区观察者收到的**机制对**与 N=1 同区观察者一致，**不新增重复类**。
- 顺序：源 tick T 产生 depart 帧 → 收件区 `barrier1(T+1)` 排水（早于 `apply_mirrors`/`phase2`）；
  收件区 AOI hide 在 `phase2(T+1)`（若镜像恰在本 tick 被清扫）或更晚的观察者扫描 ⇒ **depart 先于
  scan**，与 N=1 的「即时 depart → 下一扫」同序。
- 用例①只断言 **T+1 的 depart 帧**（字节点对）；若要断言「不双发」，必须先证明该夹具下镜像的
  `ghost/fixed_hide` 状态使 AOI 侧不 hide（软关/ghost 为 true 时）。**默认不写「恰一帧」强断言**，
  除非夹具状态能钉死。

## 2. 七面计数与基线口径（防漂移）

### 2.0 先修一个既有 writer 列错位（实施中发现，必须一起修）

`write_tick` 的 **vals 顺序与表头列名顺序不一致**：
- 表头把 ⑤ 的 outcome/诊断列放在 `cross_recipient_missing` 之后，但 vals 把它们放在
  `handoff`/zone-k 之后；
- **既有**的 own-segment 两列（`hd[11..12]`）在 vals 里排在 zone-k 三列之前，表头却排在之后。
⇒ 列数对得上、**位置错**；`perf_tsv_summary` 按名读会读到错列。历史 10.004% 的六面列在错位段
**之前**，不受影响；但 ④a 的 zone-k/own-segment 读数与 ⑤ 的 outcome 读数可能被误读。

**修法**：header 与 vals 统一为同一顺序（六面 → `missing` → `depart_frames` → `seven_total` →
三桶 → dup/drop_closed/zone_unknown → enqueue_us/events → `depart_wrn_suppressed` → handoff 11 →
zone-k 3 → own-segment 2）；并加**行为对齐用例**
`perf_writer_alignment_tests::write_tick_columns_align_with_header`（`init_with_path` + 设已知
计数 + `write_tick` + **按列名**读回断言）。修复前该用例必红。

### 2.0.1 既有产物回算（as_read vs true，2026-11-23）

旧产物只能按**旧 vals 位置**读；按列名读（`as_read`）在错位段之后是错的。工具：
`tools/hygiene/tsv_recompute_legacy.py --tsv <perf_counters.tsv>`（映射常量 + 自测已入工具门）。

| artifact | column | as_read | **true** |
| --- | --- | --- | --- |
| `2z-20261122-2359`（④b-1 2z） | `zone_cmd_unsupported` | 126 | **1425** |
| 同上 | `handoff_move_after_transfer_dropped` | 0 | **2225** |
| 同上 | `move_map_change_unsupported` | 1425 | **0** |
| 同上 | `mirror_own_segment_ids` | 2225 | **126** |
| 同上 | `handoff_fresh_own_segment` | 0 | **0** |
| `hin-mv-2z-20261123-0125`（⑤ 2z） | `cross_recipient_delivered` | 116 | **5,160,199** |
| 同上 | `cross_recipient_unroutable` | 58 | **192** |
| 同上 | `cross_frame_queue_overflow` | 0 | **471,317（8.4%）** |
| 同上 | `cross_frame_dup` | 58 | **0** |
| 同上 | `cross_frame_drop_closed` | 0 | **15,245** |
| 同上 | `cross_recipient_zone_unknown` | 116 | **192** |

- **成立不动**：④a 全部（`2z-205726`：handoff 334/58/0/58/166，zone 1446/60502/0）；④b-1
  `mirror_shadowed_static == 0`；**10.004% 基线**（六面/`missing` 在错位段之前）。
- **据此更正**：④b-1 2z 的 zone-k/own；⑤ 的 `delivered/unroutable/overflow/dup/drop_closed/
  zone_unknown`。`overflow` 真值 471,317 表示 `MAX_FRAMES_PER_SLOT=16384` 在该臂溢出过；
  此前「overflow=0」的判读不成立（进 ⑤ 容量结论）。
- 其余 ⑤/`retest`/`4k`/`twogate`/`singleline`/`exp1` 臂六面/⑤ 全 0（无跨区活动），不受影响；
  `1z-2350`/`hin-mv-n1` 全 0。

### 2.1 七面口径


- 新增 `CrossRecipientSurface::DepartFrames`（第 7 面）；`CROSS_RECIPIENT_BY_SURFACE` 6 → 7。
- **`cross_recipient_missing` 保持六面口径（不变）**；历史 10.004% 只与它比。
- 新增 TSV 列：
  - `cross_recipient_depart_frames`（第七面本 tick 增量）；
  - `cross_recipient_seven_total`（= 六面小计 + 第七面，派生列，便于自描述）。
- 恒等式（窗口合计，构造保证）：

```text
cross_recipient_seven_total == delivered + unroutable + overflow
cross_recipient_seven_total == cross_recipient_missing(六面) + cross_recipient_depart_frames
```

- **报告/摘要必须同时给「六面小计」与「七面合计」**；**旧基线只与六面小计比**。`perf_tsv_summary.py`
  打印：`missing(六面)=… | depart_frames=… | seven_total=… | delivered/unroutable/overflow=…`；
  七面恒等式 fail-closed（新产物）；旧产物缺第七列 ⇒ `depart_frames` absent、`seven_total = 六面`
  （标注 legacy），**不得**把七面合计与 10.004% 直接比。
- `note_cross_recipient_outcome` 增第 7 个 surface 下标；三桶恒等式现在覆盖**七面**。

## 3. WRN 限流（≤1 条摘要/分钟 + 抑制数进 TSV）

- 上线后镜像观察者全部走投递；WRN 只剩「非镜像/离线/克隆、或镜像 route=None/未登记/溢出」残差。
- 改成：**≤1 条摘要/分钟**，摘要里带「本轮抑制条数」；被抑制条数进 TSV 列
  `depart_wrn_suppressed`（本 tick 增量）。
- 实现：一个可注入时钟的纯限流器（`DepartWarnLimiter { last_ms, suppressed }`，`should_log(now)`
  + `suppressed`）；宿主用进程级静态实例；**门禁用例直接驱动限流器**（不进 `glog!` 捕获）：
  同一分钟内 N 次 ⇒ 1 次放行、N-1 抑制；跨分钟 ⇒ 下一次放行且摘要带计数。
- 限流只改日志量与计数，不改任何帧/行为；N=1 的同区路径不经过该 WRN（观察者在 `online`）。

## 4. 用例（四类，先红后绿）

1. **`depart_frames_cross_zone_delivers_at_next_tick`（②）**：合成 2 区夹具——区 0 World 里放「离开者」
   + 「镜像观察者（挂 route→区 1）」；`drain_depart_frames` 后 `drain_zone(zone1, T+1)`；断言网关收到
   `SendFrame { socket, body }`，且 `body == downlink::disappear_frame(target)`（**与本地路径同一构造**。
   本地路径的 body 在同夹具的另一观察者上取一次，逐字节相等）。
   - 镜像观察者的 route 由 `World::mirror_route(observer_id)` 提供；`note_mirror_recipient` 的目标区
     排水在 `barrier1(T+1)`（V2）。
2. **`depart_frames_local_path_unchanged`（同区不变）**：现有
   `disconnect_broadcast_reachable_through_depart_downlink` 保持绿 + N=1 两树 stdout sha 复跑
   （`n1_stdout_equivalence`，沿用 ⑤ 的同一对 sha）。本地路径只把 `disappear_frame` 提前到分支前，
   字节不变。
3. **`depart_frames_route_none_counts_unroutable_and_rate_limits_wrn`**：镜像观察者**无 route**；
   `drain_depart_frames`；断言 `unroutable +1`、`delivered +0`；限流器**只放行 1 次**、抑制计数到
   `depart_wrn_suppressed`；同一分钟内再 N 次 ⇒ 不再放行。
4. **`seven_face_identity_holds`**：七面各触发一次；
   `cross_recipient_missing(六面) + cross_recipient_depart_frames == delivered+unroutable+overflow`，
   且 `cross_recipient_seven_total` 同上；六面小计不因第七面变化。
- 附加：`depart_frames_body_is_same_constructor`——跨区 body 与 `downlink::disappear_frame` 逐字节
  相等（防「各写一遍」）。

## 5. N=1 与边界

- **N=1 零参与**：无邮箱/sink；`drain_departs` 的观察者都是本地会话（`online` 命中）⇒ 镜像分支不可达；
  本地路径字节不变；限流只影响日志。`n1_stdout_equivalence` 两树 sha 与 ⑤ 同一对。
- **不改**：`online_find_for_kick`、`find_linear_*` 探针、既有六面语义、`merchant_disappear` 的 else
  支（已证两者判据不同）。
- **不改** `drain_departs` 的集合/时序（`session.rs` 的 ±12 格扫）；只在 `drain_depart_frames` 加
  镜像分支。

## 6. 风险与回滚

1. **与 AOI 的机制对**：如 §1.2，第七面复刻的是 `DisappearA`，收件区 AOI hide 原样；不新增重复类。
   若实现后发现某夹具出现双帧，先证明 N=1 同夹具是否本就双帧（depart+scan），再决定是否收窄断言。
2. **计数口径漂移**（最高风险）：`cross_recipient_missing` 必须保持六面；七面只进
   `cross_recipient_depart_frames`/`seven_total`。摘要同时打印两组，旧基线只对六面。
3. **限流误伤**：限流只作用于 WRN，不影响投递；门禁用例钉住「1 条/分钟 + 抑制数」。
4. **回滚**：第七面是一个独立 surface + 一个生产分支；回滚 = 撤该分支与列（六面行为不变）。

## 7. 实施顺序

1. 本文审过（opc-27）→
2. 写码：surface/列 + `note_mirror_recipient` 返回 outcome + `drain_depart_frames` 分支 +
   `WarnRateLimiter` + 四类用例 →
3. 单 crate 测试 → 全量门（fmt/clippy/test/msgcodegen/anchor/py selftest）→
4. opc-27 合并。
