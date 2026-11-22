# L2-5 ⑤ 设计：跨区收件人帧合成（2026-11-22，v2 设计稿）

> 状态：**设计稿，未写代码**。a7 的 1k 容量窗口期内**只读代码、写文档、提交文档**；
> **不 `cargo build/test`、不起任何栈**（总监放行前）。
> 接手：`l25-step5-5b`；前手 `ff` 的 v1 稿是起点不是定稿。本文**重写**其 §2–§6（v1 的 §3 序键
> 不成立，理由见 §1）。
> 基线：`fb4d08301`（= master `03b103c25` + v1 稿）。
> 事实源：计数口径与六面定义以 `docs/issues/l2-5-step3-zone-slicing-plan-20261122.md`
> §11 / §11.1 / §12.4A / §12.5 / §12.8 为准；顺序判据以 `docs/acceptance-tier-a-20261121.md`
> §2 / §3 为准。本文与它们冲突时，先按本文提出的偏差上卡请总监裁决，**不得**静默改口径。

## 0. 结论摘要

1. **推荐方案 A（非对称延迟）**：只有**跨区帧**延 1 tick（在收件区 `barrier1` 之后、`phase2`
   之前排水发射；V2，见 §1.4），本地帧路径不动。理由见 §2.4：它是唯一同时满足「改动面最小 / 内部（非边界）收件人零变化 /
   可回滚」的方案，且 **B 声称的「全序与 N=1 完全同构」不成立**（§1.3），C 的双写者序不可判。
2. **前手 §3 的序键 `(tick, phase_rank=0 跨区 / 1 本地)` 与现实相反**：同一源 tick 里本地帧
   **先**发（产生即发），跨区帧 **后**发（下一 tick，见 §1.4 的 V2 排水点）⇒ 任何「跨区在前」的序键都是
   反的（§1.1）。
3. **比序键更根本**：N=1 的同 tick 收件人帧序来自**单线程实体遍历 + 每收件人四叉堆**；分区后
   任何合并只能得到「区内序 + 区际 canonical 序」，**跨源交错不可能逐帧复刻 N=1**（§1.3）。
   因此边界收件人的「同收件人相对顺序」硬面**必然偏离**；偏离量级可量化（§1.5），是否接受由
   Owner 定（总监已去问）。文档**不写**「顺序完全保持」。
4. **barrier2 不足以保证 I3**：六面里有产生点在 `barrier2` **之后**（NPC/商人转身/受击、
   商人消失），它们与收件区下一 tick 的排水点之间没有 happens-before ⇒ 延迟会在 1/2 tick 间漂移。
   两个修法：**V1 = 加 `barrier3`**（tick 尾会合、下一 tick 最开头排水）；**V2 = 排水挪到
   `barrier1` 之后**（`barrier1` 时全部区已完成上一 tick 全部产帧，不加会合点）。按总监规则
   「测不出差别默认 V2」，**选 V2**（§1.4 给对比与判据），并用 `src_tick < 当前 tick` 过滤把
   延迟钉死为**恰好** 1 tick。
5. **本步实测覆盖面不是「全向全通」**：区 0→区 k 六面全量；区 k→区 0 只有**移动类 RefMsg**
   （区 k 无 host 循环，但 `handle_move_cmd → broadcast_ref_msg` 会产 RefMsg 面）。验收结论必须
   带方向限定（§0.1）。

### 0.1 实测覆盖面（先说边界）

区 k 线程（`spawn_minimal_zone`）跑 `phase1 → snapshot → barrier1 → apply_mirrors → phase2 →
barrier2`，外加移动命令 / 断线 / handoff。**移动命令路径**（`handle_zone_command` →
`handle_move_cmd` → `broadcast_ref_msg`）会产 **RefMsg 面**的帧；`hear_targets` 在区 k 的 World
里能看到邻区玩家的**镜像**（带 route）⇒ ⑤ 的邮箱一旦在区 k 安装，**移动类 RefMsg 的
区 k → 区 0/其它区方向也立即打通**（这是本设计按「每区都有 sink」写接线的直接收益）。

但区 k **没有 host 循环**，所以六面里的其余五面（`run_frames` / `npc_merchant` /
`feature_changed` / `mountride` / `merchant_disappear`）、AOI 出现/消失、登录/进图、
战斗/技能/物品/商店/聊天/53/657 **都不在区 k 产生**（计划页 §12.8 残余 2）。⇒ ⑤ 的实测覆盖面是：

- **区 0 → 区 k**：六面全量；
- **区 k → 区 0/其它区**：**只有移动类 RefMsg**（区 k 唯一已接的 host 式路径）。

**任何 `missing=0` 的结论必须带这两句限定**；反向的其余五面与 AOI/战斗等要等区 k 有自己的
host 循环（另立路线项）。

**第一片（最小切片）的覆盖面更正**：第一片只改了 `send_ref_msg_to_targets` 的未命中支，但
`npc_merchant` 的两个排水点（`drain_npc_merchant_turns/hits`）经 `broadcast_ref_msg_with_surface`
**共用同一投递段** ⇒ 第一片实际覆盖 **`refmsg` + `npc_merchant` 两面**，不是一面。**由共用
投递段导致、不是额外接线**；读覆盖率时按两面计。其余四面（`run_frames` / `feature_changed` /
`mountride` / `merchant_disappear`）在各自接线前走兼容支（计 `unroutable`）。

### 0.2 既有事实（符号锚；不重查）

- 六面 = `perf_counters::CrossRecipientSurface::{RefMsg, RunFrames, NpcMerchant, FeatureChanged,
  MountRide, MerchantDisappear}`；计数点条件 = 收件人 id 来自 World 扫描/缓存、本地会话查表
  **未命中**、且 World 里 `mirror == true`。
- 缺口成因：快照里的 `HaloEntity::route` **没有落到世界侧**（`apply_mirrors` 只 upsert 镜像
  外观，不保存 route）⇒ 收件区无法把帧路由给跨区会话。**实现**把 route 存进
  `World::mirror_routes`（`set_mirror_route`），与镜像同生命周期；**不扩 `MirrorSpec`**（避免
  20 处字面量与 offer 载荷膨胀）。
- `HaloRoute` 只在 `race == Play` 且在线登记命中时 `Some`；非玩家/未登记 ⇒ `None`。
- `region_loop::CrossZoneOutbox` 是 ② 的**测试 rig**（`EmitterCriterion::RecipientZone`），
  **生产未接线**；本步要的是生产形态的跨区帧邮箱，不复用它当生产队列。
- `ref_msg_cache::targets` 是 oracle `VisibleHumanList` 的 500ms 缓存：收件人 id 可能来自
  **≤500ms 前的扫描快照**，不是本 tick 现扫。
- `TICK_INTERVAL_MS = 200`（`world_bevy::TICK_INTERVAL_MS`，用例钉死 200）。

## 1. 前手稿 §3 不成立：序键、交错、屏障

### 1.1 物理发射点（本 tick 内 vs 下一 tick）

区 0 host 循环一个 tick 的段序（符号锚，不写行号）：

```text
tick 顶（handoff tick_top）→ 命令/事件排水 → tick_world_head_with_specials
  → 宿主 P21/P32 段（HostPlayerRun：flush_run_frames 在此）
  → tick_tail_before_aoi → 快照/发布 → barrier1 → 读邻居 → apply_mirrors
  → tick_phase2 → 心跳 → barrier2
  → barrier2 之后：handoff offer → HostFrames → HostAoiDownlink
       （drain_run_downlinks_with_magic：NPC/商人转身/受击、AOI、商人消失）
  → HostPersist / HostSwitch / HostDiag / HostSleep → perf write_tick
```

- **本地帧在该 tick 内就 emit**：`flush_run_frames` 在 `HostPlayerRun`（`barrier1` 之前）；
  `send_ref_msg_to_targets` / `broadcast_feature_changed_except` / `apply_pending_gm_mountride`
  在命令段或 `HostFrames`；`drain_npc_merchant_*` / `drain_merchant_disappears` / AOI 在
  `HostAoiDownlink`（`barrier2` 之后）。这些都是 `gates[line].send(...)` 当场发。
- **跨区帧在收件区 T+1 的 `barrier1` 之后、`phase2` 之前排水发射（V2）**：源区在 tick T 把帧
  写进目标区邮箱；目标区在 T+1 的 `barrier1`（`await_neighbors` 返回）之后、`apply_mirrors` /
  `tick_phase2` 之前取走并 `send`。`barrier1` 时全部区已完成**上一 tick 的全部产帧**（含
  `barrier2` 之后的段）⇒ 延迟确定性 1 tick；排水点用 `src_tick < 当前 tick` 过滤，把「本 tick
  pre-barrier1 产生、已在邮箱里」的帧留到下一轮，保证**恰好** 1 tick（不是 0/1 混）。

⇒ v1 §3 的 `phase_rank`（跨区=0、本地=1）与物理事实**相反**：同一源 tick T 的本地帧在 T 内
先发，跨区帧在 T+1 `barrier1` 之后才发（即在 T 的所有本地帧之后；相对 T+1 的本地帧则落在
「P0/P2 之后、P3/P4 之前」——V2 的代价，见 §1.4）。

### 1.2 结论：不许用「序键」冒充顺序契约

正确可检查的表述只有**分层**的：

- **I2（同源保持）**：同一来源实体 → 同一收件人的帧序 = 源区产生序（源区把帧按产生序入同一个
  源区槽，收件区按槽序排水）⇒ 与 N=1 的**同一来源子序列**逐项相同。
- **跨源序是声明的 canonical 序，不是 N=1 序**：收件区按「源区号升序 → 源区 seq 升序」合并
  （§2.2 邮箱形状）。它确定、可复现、可断言，但**不是** N=1 的单线程交错。

### 1.3 为什么跨源交错不可复刻（更强的事实）

N=1 里同一收件人在同一 tick 收到的帧序来自：

1. 单线程 tick 内的**实体遍历/段序**（`Run()` 的消息段、宿主 P21…P32 等）；
2. oracle 每个收件人一条 `MsgQueue`（四叉最小堆，键 `(byte)wIdent`），**同优先级不是入队序**
   （`docs/frame-drain-order-audit-20261023.md`：3 条出队 `[0,2,1]`、4 条 `[0,3,2,1]`）。

分区后，两个来源区的产生动作发生在两个线程上；任何「合并」都只能在**事后**用某个键重排。
可选键只有：

- `(源区号, 源区 seq)`：zone-major，不是 N=1 交错；
- 墙钟/原子计数器：线程调度相关，**不可复现**；
- 全局实体序号：能复刻一部分「按实体」的序，但复刻不了**段序**（tick head vs AOI vs 宿主段）
  与**每收件人四叉堆**的同键 sift 序——而且分区后全局实体遍历本身已不存在。

因此：**不存在一个可实现的合并键能逐帧复刻 N=1 的跨源交错**。这不是本方案的缺陷，是分区的
数学后果。任何声称「全序完全同构」的写法必须给出上述键的构造与证明，否则是过度承诺。

### 1.4 barrier2 不足以保证 1 tick：V1 vs V2，选 V2

`barrier2` 只保证「所有区完成 `phase2`」。但六面里有产生点在 `barrier2` **之后**：

- `NpcMerchant`：`drain_npc_merchant_turns` / `drain_npc_merchant_hits`；
- `MerchantDisappear`：`drain_merchant_disappears`；
- 以及部分 `RefMsg` 调用点（`HostAoiDownlink` 内的各 `drain_*`）。

源区在 `barrier2` 之后 `push` 邮箱；收件区若在「本 tick 的 `tick_top` / `barrier2` 之后」排水，
两者**没有 happens-before**：收件区可能先排空、源区后写 ⇒ 该帧落到 T+2。线程调度一变，延迟
在 1/2 tick 间漂移，违反 I3。两个修法：

**V1（barrier3）**：tick 尾最后一个入队点之后加第三道会合点（区 0 在
`drain_run_downlinks_with_magic` 之后、HostPersist 之前；区 k 在 `handoff.after_phase2` 之后），
下一 tick 的**最开头**排水（早于本 tick 任何本地帧）。

- 顺序：跨区帧领跑 T+1 ⇒ **最接近 N=1**（N=1 里 T 的帧都在 T+1 的任何帧之前）；
- 代价：每 tick 多一个**全局会合点**。现有 2 区窗口是**空区臂**（1000 bot 全在区 0，区 1 空转），
  测不到 V1 的真实代价——`region_barrier_us` 的 202ms 均值是**相位差 + 区 0 节拍 sleep**
  （§12.2 已澄清），不是屏障开销；V1 的增量在空区臂≈一次 condvar 往返（µs 级），在边界负载臂
  则可能等于被消除的 overlap（区 k 下一 tick phase1 与 host 帧产生段的重叠）。

**V2（排水挪到 `barrier1` 之后，不加会合点）**：

- `barrier1` 时全部区已完成**本 tick phase1**；而每区上一 tick 的全部产帧（含 barrier2 之后的段）
  在它自己的本 tick phase1 之前 ⇒ **全部 T 入队 happens-before barrier1(T+1)**。在 barrier1(T+1)
  之后排水，延迟确定 1 tick；
- 但 barrier1(T+1) 也可能看见某些区**本 tick pre-barrier1** 产的帧（`src_tick == T+1`）。排水时
  按 `src_tick < 当前 tick` **过滤**（槽内按 tick 单调，front-scan 到 `src_tick == 当前` 即停），
  把这些帧留到下一轮 ⇒ **恰好** 1 tick，不是 0/1 混；
- 顺序：跨区帧落在 T+1 的「P0/P2 本地帧之后、P3/P4 本地帧之前」⇒ 相对 V1 多出
  「跨区 T 帧 vs T+1 的 P0/P2 本地帧」的**跨 tick 倒置**；
- 代价：**零新增会合点**；多出的倒置暴露面 = `cross_recipient_order_overlap_early`
  （本 tick X 非空 且 T+1 的 P0/P2 本地帧非空 的 (R,T) 对）。

**判据与选择**：

| 维度 | V1 barrier3 | V2 barrier1 后排水 |
| --- | --- | --- |
| 延迟 | 恰好 1 tick | 恰好 1 tick（靠 `src_tick` 过滤） |
| 顺序 | 跨区 T 帧领跑 T+1 全部本地帧（最接近 N=1） | 跨区 T 帧在 T+1 的 P0/P2 之后（多一类跨 tick 倒置） |
| 代价 | 每 tick 一个全局会合点（空区臂测不出；边界臂未知） | **零新增会合点** |
| 今天可测差异 | `cross_recipient_order_overlap_early == 0`（收件区 k 无本地帧）⇒ **测不出** | 同左 |
| 未来风险 | 会合点可能压 tick 尾 | 区 k 接 host 后多出 P0/P2 倒置 |

按总监规则「两种都可行且测不出差别 ⇒ 默认 V2」，**选 V2**：今天收件区 k 连本地帧都没有，
V1 的顺序收益不可测；V2 零新增会合点。**未来 revisit 条件**：区 k 接 host 循环后，若
`cross_recipient_order_overlap_early > 0` 且不可接受，再评估 V1。

> v1 §2 说「`tick_top` 排水 + barrier2 已给保证」——**不成立**，因为产生点跨过了 barrier2。
> 本稿把排水点钉在 `barrier1` 之后、`phase2` 之前（V2），并以 `src_tick` 过滤保证恰好 1 tick。

### 1.5 偏离的形态与量级（可检查）

**形态**（方案 A）：对收件人 R 与源 tick T：

```text
N=1    : [T 的本地帧 L 与跨区帧 X 按单线程序交错]
A/V2   : L(T) 全部照旧在 T 内发；X(T) 全部推迟到 T+1 barrier1 之后，排在 L(T) 之后、
         L_early(T+1)（P0/P2）之后、L_late(T+1)（P3/P4）之前
⇒ 倒置对 = {(x, l) | x ∈ X(T), l ∈ L(T), 且 N=1 里 x 在 l 之前}
```

**上界**：每个 (R, T) 的倒置对数 ≤ `|L|·|X|`；形态是「跨区帧整体后移一个 tick 边界」，
同源子序列（I2）不变。**不是**随机丢帧、不是内容变化。

**量级**：用 ④a 窗口（段 4a，2 区 1k 走廊铺位）做同条件基线：
`cross_recipient_missing = 6,432,455 / refmsg_targets = 64,296,771 = 10.004%`（385.7s）。
其中 `cross_recipient_missing` 是**收件人次**（每次广播 × 每个跨区收件人），不是帧数。
按窗口折算约 **3.3k 收件人次/tick**（2 区、全部六面合计）。

**需要新测的暴露面**（实现时加，窗口里报）：

- `cross_recipient_order_overlap` = 「同一收件人在同一源 tick 既有 ≥1 本地帧又有 ≥1 跨区帧」的
  (R, T) 对数 —— **这是顺序面可能偏离的充要暴露面**；
- `cross_recipient_inverted_pair_bound` = `Σ |L(T)|·|X(T)|`（倒置对的上界）。

两者可从 `frames.ndjson`（按收件人 + tick + 来源区离线分组）算，也可在切片模式加轻量计数
（按本区会话集合，成本可控）。**没有这两个数，10.004% 只能说明「量」，不能说明「序的暴露面」。**

## 2. 三方案对比与推荐

### 2.1 方案 A：非对称延迟（只跨区帧延 1 tick）

- **实现面**：`World::mirror_routes` 存镜像 route（`apply_mirrors` 写）；六面在「本地会话未命中且 mirror」处改为
  `classify_and_enqueue`；按 (dst, src) 的邮箱；收件区 `barrier1` 之后排水（V2，`src_tick`
  过滤保确定性）。
  本地帧路径**一行不改**。
- **延迟**：只有跨区收件人的跨区帧 +1 tick（200ms）；内部收件人**零变化**。
- **可验证性**：I2（同源子序列）与 canonical 跨源序可逐项断言；与 N=1 的对比只能对
  「内部收件人全额相等」+「边界收件人按声明规则」，不能逐帧全等。
- **失败模式**：边界收件人的跨源相对序与 N=1 不同（§1.5）；overflow 时丢尾部；route 为 None
  时进 `unroutable` 残差。
- **回滚**：切片闸关（`slicing=0`）⇒ 邮箱不构造、六面走原计数路径，N=1 逐字节不变。

### 2.2 方案 B：整 tick 统一暂存后发射

v1 的表述是「切片模式下**所有**面向收件人的帧都缓一 tick，下一 `tick_top` 按
`(源 tick, 源区, seq)` 合并 ⇒ 全序与 N=1 完全同构」。**逐项核**：

- 实现面：所有 `gates[].send` 的收件人帧都要改成入缓冲（AOI、run_frames、六面、登录尾…），
  改动面远大于 A；同样需要 V2 的排水相位与 `src_tick` 过滤。
- 延迟：**全局 +1 tick（+200ms）**，包括内部收件人。
- 可验证性：内部收件人 = N=1 流**整体平移 1 tick**（可机械对比）；边界收件人 = 每源子序列
  相同 + canonical 跨源序——**与 A 的偏离形态相同**（§1.3），并没有变好。
- **「完全同构」不成立**：合并键 `(源区, seq)` 是 zone-major；它不恢复 N=1 的单线程交错
  （§1.3），也不恢复每收件人四叉堆的同键序（§1.3 第 2 条）。必须降格为「平移 + 声明合并序」。
- 失败模式：全服延迟 +200ms；缓冲内存与 gate 突发转移到 tick 顶；改动面大、回归面大。
- 回滚：同 A（闸关），但改动面大 ⇒ 回滚验证成本高。

### 2.3 方案 C：源区直写收件人（每收件人一条有序队列）

- 实现面：源区拿到收件人的 route 后**直接写**该收件人的有序队列，绕开收件区会话表。
- 延迟：零额外 tick（若在源区 tick 内写并立即发）。
- **致命问题**：同一收件人此时有**两个写者**——源区（跨区帧）与收件区（本地帧/AOI）。
  若两者都直接 `send`，帧序由**线程抢锁顺序**决定 ⇒ 不可复现、不可判（违反 I1 的「唯一有序
  发射者」）。加「每收件人一把锁」只保证不撕裂，不保证顺序；加「按 tick 排序」就需要等所有
  写者到齐 ⇒ 又回到 B/barrier 模型。
- 去重键只能挡重复，**挡不了重排**。
- 结论：**否决**。零延迟的诱惑来自忽略「第二写者」，而第二写者正是本地帧路径。

### 2.4 推荐：A（附条件）

推荐 A，理由按优先级：

1. **目标最小**：⑤ 的目标是「看得见就收得到」，A 以最小改动让跨区收件人收到帧；
2. **不伤内部收件人**：内部（非边界）收件人的帧流与延迟**逐字节不变**——这对 1 万在线容量
   验收（p50/p99）与 tier A §2「允许毫秒级差」都最稳；
3. **B 的前提被证伪**（§2.2）：B 用全局 +200ms 换不到边界交错的改善，只换来内部收件人的
   平移；在 A 与 B 的「边界偏离」相同的前提下，B 是纯加价；
4. **C 不可判**（§2.3）。

**附条件（Owner/总监必须拍板）**：

- 若验收要求「每个收件人的帧流必须是 N=1 的**整体平移**、可机械对拍」⇒ 只能选 B（并接受
  +200ms 与 §1.3 的降格声明）；
- 若接受「内部收件人全等 + 边界收件人按声明 canonical 序、偏离量级如实上报」⇒ 选 A。
  **我的建议是后者**：它把「不可能」限制在真正跨区的边界收件人，而不是让全服陪绑。

> 无论 A/B，§1.3 的结论都不变：跨源交错不能逐帧复刻。差别只是「谁来承担偏离的延迟」。

## 3. 六面 × 一个面一行

相位记号：**P0** = 命令/事件段（phase1 之前）；**P2** = HostPlayerRun（barrier1 之前）；
**P3** = HostFrames（barrier2 之后、HostAoiDownlink 之前）；**P4** = HostAoiDownlink
（`drain_run_downlinks_with_magic`，barrier2 之后）；**P5** = 收件区 T+1 的 `barrier1` 之后、`phase2` 之前的排水（V2）。**交付相位一律 P5(T+1)**
（方案 A）；`src_tick` 过滤保证恰好 1 tick。

| 面 | 产生相位（tick 内） | 交付相位 | 与本地帧的全序位置 | N=1 相对顺序是否保持 | 不保持时的偏离形态与量级 |
| --- | --- | --- | --- | --- | --- |
| `refmsg` | **P0–P4 全跨**：`send_ref_msg_to_targets` 被命令段、HostPlayerRun 之后各 `drain_*`、HostAoiDownlink 多处调用；收件人可能来自 500ms 缓存 | P5(T+1)（barrier1 后） | L(T) 之后；T+1 的 P0/P2 之后、P3/P4 之前 | **否**（跨源）；同一来源子序列**是** | 每个 (R,T) 的跨区 `refmsg` 帧整体后移 1 tick；倒置对 ≤ `|L|·|X|`。这是六面里**量最大**的一类（10.004% 分母就是它的投递段） |
| `run_frames` | **P2**（`flush_run_frames` 的 `SendRefMsg(53)` 观察者副本，barrier1 之前） | P5(T+1) | 同上 | **否**（跨源）；同源 **是** | 只影响 53 健康变化观察者副本；低频、每源每次 `ref_copies` 份。偏离形态同 A：跨区副本后移 1 tick；同源副本序不变 |
| `npc_merchant` | **P4**（`drain_npc_merchant_turns/hits`，barrier2 之后） | P5(T+1) | 同上 | **否**（跨源）；同源 **是** | 仅当 NPC/商人在**源区**、收件人在邻区。今天区 k 的静态 NPC 无 host 循环 ⇒ 只覆盖「区 0 NPC → 区 k 玩家」（§0.1）；区 k NPC 的转身/受击**今天不产生**，不是本路径能补 |
| `feature_changed` | **P0**（`apply_pending_gm_feature_changed` / 装备重算命令段） | P5(T+1) | 同上 | **否**（跨源）；同源 **是** | 低频（换装/`@OnHorse` 的 41）。偏离：跨区观察者收 41 晚 1 tick；同源序不变 |
| `mountride` | **P0**（`apply_pending_gm_mountride`，命令段） | P5(T+1) | 同上 | **否**（跨源）；同源 **是** | 极低频（GM `@OnHorse`/`@OffHorse`）。它按**本地会话表**枚举收件人，跨区玩家靠**镜像 + range** 单独枚举；偏离：跨区玩家收 1118 晚 1 tick；含自己那帧仍走本地 |
| `merchant_disappear` | **P4**（`drain_merchant_disappears`，barrier2 之后） | P5(T+1) | 同上 | **否**（跨源）；同源 **是** | 低频、局部（城堡商人战中 `FixedHideMode`）。偏离：跨区观察者收 30 晚 1 tick；收件人是**发帧时刻快照**，不是本 tick 现扫 |

**表内「同上」的精确含义**：交付相位 = P5(T+1)；全序位置 = **L(T) 之后、L_early(T+1)
（P0/P2）之后、L_late(T+1)（P3/P4）之前**。V2 与 V1 的唯一差别是「落在 L_early 之前还是
之后」；今天跨区收件人只在区 k、而区 k 无本地帧 ⇒ `cross_recipient_order_overlap_early == 0`
（§1.4），两者**测不出差别**，故按总监规则选 V2。区 k 接 host 后如有 L_early，需要重测。

**三列不留空的证据**：每行的「产生相位」是符号锚（函数名 + 所在段），「交付相位」由 §1.4 的
V2 的 `barrier1` 排水点保证，「全序位置」由 §1.1 的段序推出，「N=1 是否保持」按 §1.3 的通则逐个面判
（同源保持、跨源不保持），「偏离形态与量级」按面的频率/收件人来源分别写。

**补充（六面之外）**：AOI 出现/消失帧**不是**跨区面——观察者在本区，帧由收件区从本区 World
（含镜像）直接发（计划页 §7 口径），P4 内当场发，不经邮箱；因此不在本表，也不进
`cross_recipient_missing`。

## 4. 不变量 I1–I5（本方案必须满足）

### I1 同一收件人的帧流只有一个有序发射者

- **机制**：源区只把帧写进**目标区邮箱**；唯一的 gate `send` 在**收件区线程**（`barrier1` 之后排水
  + 本地帧路径）。源区永不直接写目标收件人的 gate。
- **邮箱形状（关键）**：目标区一个邮箱，内部按**源区**分槽
  `inboxes[dst][src] = Mutex<VecDeque<CrossZoneFrame>>`；每个槽只有一个写者（源区线程）⇒
  无跨源抢锁交错；收件区按源区号升序排空 ⇒ 跨源序是**声明序**，不是抢锁序。
- **断言**：收件区排水只在 `barrier1` 之后一处；`CrossZoneSink` 不暴露 `gates`（源区拿不到 gate
  发送端）；测试 `single_emitter_only_recipient_zone_drains`（源区写入后，源区侧 gate 收端 0 帧，
  收件区侧恰 1 帧）。

### I2 同一来源实体 → 同一收件人的帧序与 N=1 相同

- **机制**：源区对每次跨区投递按 `(src_zone, src_tick)` 单调 `seq` 入**同一个源区槽**；槽是
  FIFO；收件区按槽序转发。overflow 只**拒新**（§5）⇒ 保留每个源的前缀，不挖中段。
- **断言**：`same_source_subsequence_equals_n1`（合成两帧同源、同收件人，断言输出序 = 入队序）；
  以及 T7（§8）。
- **注意**：跨来源的帧序**不在 I2 里**（I2 只承诺同源）。

### I3 跨区交付延迟确定性且上界 1 tick

- **机制（V2）**：排水点 = 收件区 T+1 的 `barrier1` 之后、`phase2` 之前；`barrier1` 时全部 T
  入队已 happens-before（§1.4）。排水按 `src_tick < 当前 tick` 过滤，保证**恰好** 1 tick
  （pre-barrier1 产的 T+1 帧留到下一轮）。
- **断言**：`cross_delay_is_exactly_one_tick`（人为让源区 post-barrier2 段变慢 N ms，收件区仍在
  T+1 收到，不是 T+2；人为让源区 pre-barrier1 产帧，断言它落在 T+2 而不是 T+1）→ 证明过滤生效；
  变异「去掉 `src_tick` 过滤」⇒ 该帧 0 tick 到达 ⇒ 红；「把排水挪到 barrier1 之前」⇒ 与源区产帧
  无序 / 丢帧 ⇒ 红。
- **守卫**：排水点必须**在 `barrier1` 返回之后**（源码锚 + 调用序断言）；任何把排水挪回
  `barrier2` 之后或 `barrier1` 之前的改动 ⇒ 顺序守卫红。

### I4 切片关闭（`slicing=0`）时新路径零参与，N=1 逐字节不变

**断言位置（写死，不能只在文档里说）**：

1. **构造入口**：`CrossZoneMailboxes` 只在 `slicing == 1 && mirror_enabled` 时构造
   （`run_inner`）；测试 `cross_zone_mailboxes_absent_without_slicing` 断言 `None`。
2. **route 字段**：N=1 不调用 `apply_mirrors`，`World::mirror_routes` 只由 `apply_mirrors` 写；
   测试 `mirror_route_lifecycle`（upsert 后写入；remove/promote 后清除）。
3. **计数**：`counting_recipients_enabled()` 闸扩展覆盖 `delivered/unroutable/overflow`；
   测试 `slicing_off_counts_nothing_three_buckets`（切片关 ⇒ 三桶 `delivered/unroutable/`
   `overflow` + 诊断列 `dup/drop_closed/zone_unknown` **各自 0**）。
4. **排水点**：V2 的 `barrier1` 后排水只在切片路径存在；N=1 走 `world.tick_tail`、无
   `ZoneRuntime`，测试 `cross_drain_absent_without_slicing`。
5. **端到端**：`n1_stdout_equivalence.ps1` 两树归一化 stdout sha 相等（沿用 ④a/④b-0 的
   `order_sha` / `multiset_sha` 同一对）+ `l2_two_zone` 全绿。

### I5 队列有界，溢出策略只许一种

见 §5：**拒新**，唯一策略，写死。

### I5 附：待落地断言（完整片；**不写进本文档就等于没有**）

以下三条是**完整片的验收项**，最小切片故意未做；实施时必须落到代码，测试名写死：

1. **`single_emitter_only_recipient_zone_drains`**：源区写入后，源区侧 gate 收端 **0 帧**，
   收件区侧**恰 1 帧**——证明「唯一有序发射者 = 收件区」不是靠注释（I1）。
2. **`same_source_subsequence_equals_n1`**：合成「同源、同收件人」多帧，断言收件人观察到的
   **同源子序列**与 N=1 基线逐项相同（跨源交错不在 I2 里；只比同源）。
3. **`cross_delay_is_exactly_one_tick`**：人为让源区 **post-barrier2 段变慢 N ms**，收件区仍在
   T+1 收到（不是 T+2）；并覆盖「本 tick pre-barrier1 产帧 ⇒ 落 T+2」的 `src_tick` 过滤面。

## 5. 有界与溢出：拒新（并修掉 v1 自相矛盾）

- **容量**：`MAX_FRAMES_PER_SLOT` = **每 `(dst, src)` 槽的单 tick 上限**（启动常量，初值
  **16384**；从 §1.5 的 3.3k/tick（1k 2 区）起，按 1万 ≈ 10 倍预留；**禁止无界**）。
  槽每 tick 排空 ⇒ 上界含义 = 「单源单 tick 的跨区帧数」。
- **溢出策略（唯一）**：**拒新**——槽满时丢弃**新来的**帧，`cross_frame_queue_overflow` +1，
  限流 WRN。**不**丢最旧、**不**无界增长、**不**阻塞源区。
- **为什么拒新不是丢旧**：
  - **拒新 = 保前缀**：每个源槽是 FIFO，丢尾部 ⇒ 该源对该 dst 的已交付序列仍是**连续前缀**，
    下一 tick 的帧照常到达；收件人看到「本 tick 尾部缺失」，没有中段空洞。
  - **丢旧 = 可能抽掉多来源的最早帧**：跨源槽若共享一个队列，丢最旧会删掉「最早进入的源」的
    帧 ⇒ 该源的序列出现**中段空洞**，直接违反 I2。
  - 由于本设计按源分槽（I1），拒新进一步保证**每个源各自保前缀**。
- **计数口径**：overflow 是三个结果桶之一（§7），**不再**额外计入 `missing`，保证
  `delivered + unroutable + overflow == missing(合计)`。
- **v1 自相矛盾的裁决**：v1 §2 写「拒新（保留已排入序）」，v1 §6 T6 又写「拒新或丢最旧，
  二选一」——**统一为拒新**，T6 的「或丢最旧」删除（§8）。

### 5.1 已知成本：每收件人克隆帧体（先记账，再优化）

`note_mirror_recipient` 当前对**每个镜像收件人**做 `bodies.to_vec()`：一次广播 B 个帧体 × R 个
跨区收件人 = **B×R 次克隆**。1k 档入站已过载（256 事件/tick vs 排水 151–169，净积压 76.6k），
而 ⑤ 的成本加在**源区的收件人循环**里——接五面后这条路径占比会更大。

**可测的替代（按推荐顺序）**：

1. **同一次广播的 bodies 用 `Arc<[u8]>` 共享**：广播方构造 `Vec<Arc<[u8]>>` 一次，同槽所有
   收件人只 clone Arc（O(1)），收件区按 `&[u8]` **逐字节转发**。不改变「逐字节转发」前提，
   只改所有权。代价：六面投递段签名/构造点要改成 Arc（或加 `bodies_arc` 入口），并保证
   `Send`。**首选**。
2. **只存 `(ident, targets)`、由收件区自行编码**：省掉帧体，但**改变「逐字节转发」前提**——
   收件区必须有与源区相同的编码上下文（displays/WAbil/布局），最容易出「同一事件两边编码
   不同」。必须先证明编码等价，否则不采用。

**测量（下一个窗口必须能读）**：`perf_counters` 新增两列
`cross_enqueue_us`（本 tick `note_mirror_recipient` 整段总耗时，含 `bodies.to_vec()` + 邮箱
push）与 `cross_enqueue_events`（本 tick 入队次数 = 六面镜像收件人次）；摘要打印
`每事件 us = Σus / Σevents`。探针双闸（`MIRROR_PERF_ENABLED && SLICING`），关闭态只多两次
原子读、不取钟。现有四段探针**不隔离**该路径，故单独补这两列。

## 6. 去重：结构性恰一次（不设去重键）

**原则**：谁决定收件人集合，就只由谁发。

- **唯一产生者**：一帧（一个事件 × 一个收件人）只在**源区**的产生点被判一次；收件区只转发
  邮箱里已有的帧，**没有第二条产生该事件的路**。⇒ 结构性恰一次。
- **不设 `body_sha` 去重键**：v1 的去重键会**掩盖**重发缺陷（重复被静默吞掉，计数不动）。
  本设计**不做丢弃式去重**；重复帧若出现，就是缺陷，必须暴露。
- **canary（只计数、不丢弃）**：`cross_frame_dup`——收件区排空时按
  `(src_zone, src_tick, seq, dst)` 查本 tick 的已见集合，第二次出现就 +1（**不丢帧**）。
  正常恒 0；非 0 是重发缺陷。仅切片 + 探针时计算（成本 O(帧数)）。
- **handoff 提升的重复分析**：T 时 id 在源区（真实体）、在收件区是镜像；T+1 收件区把镜像
  提升为真身。事件在 T 由源区产生一次（入邮箱）；收件区 T+1 不会因为提升而**重放**该事件
  （offer 只带全量状态，不带 pending 帧）。⇒ 不重复。若将来 offer 带帧，必须走同一邮箱。
  测试 T5 钉住「提升窗口中间恰一帧」。
- **T4 的写法**（§8）：不再是「去掉去重键出现重复」，而是「**破掉结构性恰一次**（让收件区
  本地路径也发同一事件，或让源区入队两次）⇒ 重复帧出现 ⇒ 红」——证明结构约束是承重的，
  而不是靠去重键遮盖。

## 7. 计数口径（不漂移；10.004% 仍可比）

**原则**：在**同一表达式嵌套**把 v1 的未命中计数换成 `note_cross_recipient_outcome`（六面
全接后旧的兼容支已删除，无生产调用）
`classify_cross_recipient(surface, outcome, ...)`——每个「收件人 id 来自 World 扫描/缓存、
本地会话未命中、World 里 `mirror==true`」的事件**恰好记一次**，且**只**归入一个结果桶。

| 列 | 含义 | 与旧口径关系 |
| --- | --- | --- |
| `cross_recipient_missing` | **保持旧义：六面未命中合计**（= `sum(six faces)`） | **原列不动**；10.004% 直接按此列比 |
| `cross_recipient_delivered` | route=Some 且已入目标邮箱 | 新增 |
| `cross_recipient_unroutable` | route=None（镜像无 route / 未登记 / 同 tick 下线在快照里已无 route） | 新增；**残差桶** |
| `cross_frame_queue_overflow` | 槽满拒新的条数 | 新增 |
| `cross_frame_drop_closed` | 排空时 gate 线/socket 已不在（**诊断**，属 delivered 的子集） | 新增；**不进三桶恒等式** |

**第一片的两面**：`delivered` 桶同时含 `refmsg` 与 `npc_merchant`（共用 `send_ref_msg_to_targets`；
不是额外接线）。读首屏三桶时不要把 `npc_merchant` 当成「未接线」面。

**恒等式（结构保证，不是事后核对）**：

```text
cross_recipient_missing (窗口合计) == cross_recipient_delivered
                                   + cross_recipient_unroutable
                                   + cross_frame_queue_overflow
```

> 逐行（单 tick）可能因「区 k 线程 increment」与 host `write_tick` 的多个原子 swap 交错而错一
> 拍；每个 incidence 同时给「六面」与「一个结果桶」各 +1 ⇒ **窗口合计严格相等**，脚本按窗口
> 校验（不是逐行）。

- 每个面计数仍 +1（六面列照旧），同时恰好一个结果桶 +1 ⇒ 恒等式由构造保证。
- `drop_closed` 是「已交付到网关、但 socket/gate 已关」的诊断，**不**再加进三桶（否则重复计）。
- **残差口径写死**：`unroutable` 允许的来源只有「route=None（非玩家/未登记/同 tick 下线已在
  快照里消失）」；route=Some 却查不到 zone（`lookup_registered == None`）也进 `unroutable`，
  但额外计 `cross_recipient_zone_unknown`（缺陷信号，应恒 0）。**禁止**把未 assign 的
  `(line,socket)` 回落区 0——`ZoneRouter::lookup` 缺省回落 HOST_ZONE，本路径必须用
  `lookup_registered` 区分「真属区 0」与「未登记」。

**`perf_tsv_summary.py` 同步**：

- 首屏同时打印**新三桶 + 合计**：
  `missing(合计) = delivered + unroutable + overflow`，并保留六面分解与
  `refmsg_targets` 分母/占比（10.004% 口径不变）；
- **逐行/窗口恒等式校验**：窗口求和后 `delivered+unroutable+overflow != missing` ⇒ 判 PREREQ
  （fail-closed；旧产物缺新列 ⇒ 标注 `absent`，仍可读，但不得输出「恒等式通过」）；
- `--selftest` 增：三桶恒等、恒等破坏（互补错误）、旧产物缺列、六面与合计的既有校验不回归；
- 新列加入 `CROSS_OUTCOME_COLS`，`REQUIRED` 只对新产物生效（旧产物 tolerant）。

> **命名裁决**：v1 把 `cross_recipient_missing` 改成「route=None 残差」，使旧基线不可直接比。
> 本稿把「合计」保留在原列（旧义），残差另起 `unroutable`。若总监坚持「missing = 残差」，
> 则需把合计列改名 `cross_recipient_total` 并在脚本里做旧产物兼容；这是**一行决策**，本文
> 默认保留旧列义。

## 8. 牙 T1–T7 与变异清单

**纪律（本项目既有）**：每条用例先证明**未实现时红**且红因是断言而非编译错误；实现后绿；
变异逐支重放、恢复后 sha 相等、无 `MUTATION` 残留；变异只在本人 worktree 跑；夹具用合成
root/tempdir，不 assert 真实仓库状态；每颗牙写「退化形态 → 结果」（空输入/单元素）。

- **T1 route 丢失**：删 `apply_mirrors` 的 route 传递 ⇒ 跨区帧不投递、`delivered==0`、
  `unroutable` 吃掉全部 ⇒「跨区收件人收到该帧」红。
- **T2 镜像过期**：邻区快照清掉该 id ⇒ 镜像（含 route）同 tick 被清扫 ⇒ 不投递、无 panic、
  `cross_frame_dup == 0`；该 id 的旧 route 不得残留（`World::mirror_routes` 同 tick 移除）。
- **T3 收件人同 tick 下线**：`SM_CLOSED` 与该帧同 tick ⇒ 若快照里 route 已 None ⇒ `unroutable`；
  若 enqueue 后 socket 关 ⇒ `drop_closed` +1；不 panic、不重试、不 dup。
- **T4 结构性恰一次**（改写）：**破掉**「只有源区产生」——让收件区本地路径也发同一事件
  （或源区对同一事件入队两次）⇒ 该收件人该事件出现 2 帧 ⇒ `cross_frame_dup > 0`、rig 红。
  **不要**用「删去重键」当变异；本设计没有去重键。
- **T5 handoff 提升中途**：镜像在 T 是 route=Some、T+1 被提升为真实体 ⇒ 已入队帧与提升不
  重放该事件 ⇒ 恰一帧；断言 `handoff_promoted` 与 `cross_frame_dup == 0`。
- **T6 有界队列**：注入超过槽上限 ⇒ `overflow` +1、**拒新**（保留前缀，断言尾部缺失、头部
  完整）、不无界增长；断言上限处行为与计数。**删掉「或丢最旧」。**
- **T7 同一收件人「本地帧 + 跨区帧」全序用例**（§1 的核心，新增）：
  - 夹具：收件人 R 在区 1；区 1 本地事件产生 L（如本地 NPC 转身）；区 0 的边 source 产生 X
    （同源 tick T，走 `refmsg`）；
  - 断言：L 在 T 内到达；X 在 **T+1 `barrier1` 之后、`phase2` 之前**到达，且排在 L(T) 之后、
    L_early(T+1) 之后（= V2 声明的 canonical 位置）；同源子序列与 N=1 相同；
  - **先红后绿**：未实现时 X 不出现（`delivered==0`）⇒ 红；实现后绿；
  - 变异：把排水挪到 `phase2` 之后（应红：X 出现在 T 而不是 T+1，或顺序错）；把延迟改成
    **2 tick**（应红）；**去掉 `src_tick` 过滤**（pre-barrier1 产的帧应在 T+1 排、却在本轮被排
    ⇒ 0 tick，应红）；**把排水挪到 barrier1 之前**（应红：与源区产帧无序、可能丢帧）。

**变异清单（逐支，恢复后 sha 回合）**：

1. 删 route 传递（`apply_mirrors` 不写 `World::mirror_routes`）⇒ T1 红、T7 红；
2. **破掉结构性恰一次**（收件区本地路径也发同一事件 / 源区入队两次）⇒ T4 红
   （对应总监清单的「去重键」一支；本设计**没有**去重键，故改为破结构约束，而不是删键）；
3. **排水挪相位**（barrier1 之后 → phase2 之后 / barrier1 之前）⇒ T7 红；
4. **去掉上限**（无界）⇒ T6 红（无界增长断言）；
5. **拆 N=1 闸**（切片关仍构造邮箱/计数）⇒ `slicing_off_counts_nothing_three_buckets` 红；
6. **延迟改成 2 tick** ⇒ `cross_delay_is_exactly_one_tick` 红；
7. **去掉 `src_tick` 过滤 / 去掉 barrier1 排水点** ⇒ `cross_delay_is_exactly_one_tick` 红。

## 9. 交付顺序（最先落地的「单事件跑通」最小切片）

1. **route 传递**：`world_bevy::MirrorRoute { line, socket }`；`World::mirror_routes`
   （**不扩 `MirrorSpec`**：避免 20 处字面量与 offer 载荷膨胀）。`apply_mirrors` 在 `upsert_mirror`
   成功后 `set_mirror_route(e.actor_id, e.route)`，`RefusedOccupied`/`RefusedCell` 清空；
   `remove_mirror` / `promote_mirror_to_real` / `handoff_accept(AlreadyHeld)` 清空；
   `region_loop::HaloRoute` = `MirrorRoute` 的别名。单测 route 往返 + 清扫移除。
2. **邮箱 + sink**：`CrossZoneMailboxes`（按 dst，内含按 src 的槽）、`CrossZoneSink`（源区写、
   不暴露 gates）；只在 `slicing && mirror` 构造（I4 闸 + 测试）。
3. **单个面**：先接 `RefMsg` 的 `send_ref_msg_to_targets` 未命中支（同一嵌套处
   `classify_cross_recipient` + 入邮箱）；三桶计数 + `missing` 合计恒等。**注意：`npc_merchant`
   经 `broadcast_ref_msg_with_surface` 共用本段 ⇒ 本步实际覆盖 `refmsg` + `npc_merchant` 两面**
   （不是额外接线）。
4. **收件区排水（V2）**：`spawn_minimal_zone` 与区 0 host 的 **`barrier1`（`await_neighbors`）
   返回之后、`apply_mirrors`/`tick_phase2` 之前**各加一处排空 → `gates.send`；排水时按
   `src_tick < 当前 tick` 过滤（保恰好 1 tick）。**不加新会合点。**
6. **单事件测试**：合成 2 区夹具，区 0 发送者 → 区 1 收件人（镜像带 route），断言 T+1 恰一帧、
   bytes 相等、`delivered=1 / unroutable=0 / overflow=0 / missing=1`。
7. 再依次接其余**四面**（`run_frames` / `feature_changed` / `mountride` / `merchant_disappear`），
   并为 `npc_merchant` 补迁移牙（共用投递段，**无单独接线**）→ T1–T7 → N=1 复跑（三件套 + 两树 sha）
   → 2 区走廊窗口（三桶 + 恒等式 + `order_overlap` / `inverted_pair_bound`）。

## 10. 风险与待决

### 最大三个风险

1. **区 k 无帧产生循环（范围风险，非本步能补）**：§0.1 / §12.8 残余 2 ⇒ ⑤ 实测只覆盖
   「区 0 → 区 k」。任何「跨区帧已全通」的结论在区 k host 循环落地前都是**越界结论**。
   缓解：验收显式标注方向；把区 k host 循环登记为 ⑤ 之后的前置卡。
2. **V2 的排水点代价与未来 revisit**：V2 不加会合点 ⇒ 无 `region_barrier3_us` 增量；代价是
   跨区帧落在 T+1 的 `barrier1` 之后（排在 P0/P2 本地帧之后）。今天收件区 k 无本地帧 ⇒
   暴露为 0；区 k 接 host 循环后，若 `cross_recipient_order_overlap_early > 0` 且不可接受，
   再评估 V1（`barrier3`）。窗口必须报 `cross_recipient_order_overlap` /
   `inverted_pair_bound` / `overflow` / `unroutable`。
3. **顺序面在边界收件人上必然偏离 N=1**：§1.3 的数学结论。若 Owner 不接受「边界收件人排除
   在顺序硬面外」，⑤ 无法按 tier A §3 立判据；此时只能走 B（+200ms）并**仍然**要接受跨源
   交错的降格声明。缓解：用 §1.5 的 `order_overlap` / `inverted_pair_bound` 把暴露面量化后
   交 Owner 拍板。

### 次级风险（登记）

- **route 时新性 vs 500ms 缓存**：收件人 id 可能来自 ≤500ms 快照；route 必须从**当前镜像**
  （本 tick `apply_mirrors` 后）取，而不是缓存里的旧 route；镜像已被清扫的旧 id ⇒ `unroutable`
  （可解释残差），不是静默丢。
- **`lookup_registered` 缺省回落**：本路径禁止用 `ZoneRouter::lookup` 的 HOST_ZONE 回落；
  未登记一律 `unroutable` + `zone_unknown`。
- **overflow 容量**：初值 16384/槽 必须按 1 万在线重测；`overflow > 0` 即容量缺陷，不许当
  「常态」掩盖。

### 需要总监/Owner 拍板的两件事

1. **方案 A vs B**（§2.4）：A = 边界收件人偏离、内部零变化；B = 全服 +200ms、内部可平移对拍。
   推荐 A。
2. **顺序硬面在边界收件人上的处置**（§1.3 / §1.5）：接受「内部全等 + 边界按声明 canonical 序」
   （推荐），还是要求「全服可平移逐帧对拍」（只能 B，且仍有跨源降格）。
