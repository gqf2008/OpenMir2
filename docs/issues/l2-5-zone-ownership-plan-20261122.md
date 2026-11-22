# L2-5 设计：每区 session/host 切片（区级归属，兑现容量）

> 状态：**设计（待 Leader 审；只读调研，无代码、未构建）**。基线 master `ac7d2b1df`（L2-4 收口）。
> 前序：`docs/issues/l2-4-multi-zone-plan-20261122.md` §3/§7、`docs/issues/l2-4-6b-hook-plan-20261122.md`、
> `docs/issues/shard-session-placement-design-20261121.md`。
> 一句话：**L2-4 交付的是结构；L2-5 才把负载真正分到区上**——所有结论都按「能与单区基线逐帧对拍」立。

## 0. 结论速览

| # | 问题 | 结论 |
| --- | --- | --- |
| Q1 | 实体归属 | 一律按坐标矩形；**区 0 的 World 也只装自己 rect 的实体**，DB/guild/castle/IDS 是进程服务不是实体；ctor fold 与 MonGen 必须**全局按原序产生、再分发**（RNG 不自乘） |
| Q2 | 登录落区 | DB 读仍在区 0，读完把 `LoadedHuman`+会话**移交**目标区；`zone_of == None` ⇒ fail-closed 拒登（不回落区 0）；与 P0-3 无关 |
| Q3 | 跨区可见性 | **镜像代理 + 收件人区发帧**（不是 owner 直发）；否则与基线**逐帧同序不可达**；代价是 6b 的单一发射者口径要改（**语义修订留痕见 §7**） |
| Q4 | 跨界移动 | **(b) 两阶段 handoff**：源区冻结 → 目标区 ack 后才提交；失败 = 拒移（原子性 = 源从不放手）。(a) 只作布局优选，不作运行时规则 |

## 1. Q1 实体归属 —— 按坐标，逐类切片

- 规则：owner 区 = `RegionLayout::zone_of(map_id, x, y)`；L2-2 已保证「一个坐标只有一个 owner」。
- 逐类：
  - **静态 NPC/商人/行会NPC**（`ctor_fold`）：**全局按文件序生成**（抽数与基线逐条相同），生成后按坐标分发进目标区 World。反例：各区分头 fold ⇒ NPC ctor 随机值随「谁先 fold」漂移 ⇒ A1 红。
  - **MonGen**：**全局驱动产生刷怪决策**（`(map,x,y,monster)`，`rng_trace` 序不变），把 spawn 命令投给 owner 区落实体。反例：各区独立跑 MonGen ⇒ RNG 流按区切分，与基线不可对拍（A2 红）。
  - **掉落物**：由「死亡实体所属区」创建并持有（坐标必在该区）。
  - **地图事件/火墙**（`fire_burn_events`）：贴格 ⇒ owner 区；跨区效果留给后续跨区消息（不在本步）。
  - **分身/宝宝/召唤物**：与主体同区；跨界随主体 handoff。
- **每区必须有独立的 actor id 段**（`World::set_id_base`，与分片同款：`base = zone * ACTOR_ID_SEGMENT`）。不给段 ⇒ 两区各自从 1 分配，出现帧 `recog` 撞车，账本会把不同实体的同身份帧判 `Duplicate`——**确定性失败，不是概率问题**。
- 可证伪断言：
  - **A1** 同 seed 下，N 区与单区的「全部 NPC 实体字段向量的多重集」逐字段相等（id 按区段归一后）。
  - **A2** MonGen 的 RNG 消耗序列（`rng_trace`）在 N 区与单区**逐抽相同**。

## 2. Q2 登录落区 —— 区 0 读档、目标区入场

- 流程：demux 把 `LoadDbRequest` 投区 0（6a 已定）→ 区 0 发 DB 查询 → 回包后组装 `LoadedHuman` →
  `zone_of(saved_map, x, y)`：
  - `Some(0)` ⇒ 区 0 原地 `enter_map`；
  - `Some(k≠0)` ⇒ 装箱投给区 k 的「入场队列」；区 k 在自己 tick 顶排水：`open_session` + `enter_map`，
    回执给区 0（`saves`/`engine` 记账）后由区 0 调 `ZoneRouter::assign(line, socket, k)`（写表只需要路由键，
    但**回执先于 assign**，避免「事件已按区 k 分派、实体还没进 World」的窗口）；
  - `None`（未覆盖/越界/负坐标）⇒ **拒登** + `ERR` + 计数 `zone_login_uncovered`，**不回落区 0**：
    回落会把实体放进错的 World，正是 L2-5 要消灭的形态。
- 存档：`DB_SAVEHUMANRCD` 由 owner 区直接走共享 DB 链路（`req_tx` 可 clone，`next_query_id` 已是
  `Arc<AtomicI32>`）；关服保存由各区先收敛、区 0 汇总。
- 与 P0-3 无关：这里是**同图按矩形**切，不需要地图连通性（P0-3 是按地图组，被 159 图大分量否掉）。
- 可证伪断言：
  - **A3** 登录到区 k 后 `zone_router.lookup(line, socket) == k`，且该 actor **只**出现在区 k 的 World。
  - **A4** 构造越界存档坐标 ⇒ 登录被拒且 `zone_login_uncovered` +1（含原始坐标的日志行）。
  - **A5** `MIR2_ZONES` 未设 / 单区 ⇒ 逐字节不变（本阶段第一步的硬条件）。

## 3. Q3 跨区可见性 —— 镜像代理（推荐）vs owner 直发

- **模型 O（owner 直发）**：owner 区给邻区观察者发帧。需要 halo 带收件人路由 + 每 tick 跨区 outbox 合并。
  **致命**：oracle 的出现帧序 = 观察者自己的 `VisibleActors` 入队序（扫描格序），owner 区看不到观察者的
  扫描序列 ⇒ 只能把远端帧追加在本地帧之后。**反例与数字**：观察者视野内 3 个本地 + 2 个远端（5 个目标，
  共 `C(5,2)=10` 种两帧相对序），只要 1 个远端早于 1 个本地出现，序就与基线不同——远端各占 2/5 的目标数，
  随机排序下「序全对」的概率约 `1/10` ⇒ **约 90% 的场景不同序**。
- **模型 M（镜像代理，推荐）**：每区 World 为邻区实体维护**只读镜像**（位置/朝向/死亡/骑乘/展示/名字），
  tick 顶从邻区快照刷新，**不参与模拟**。本地 AOI 扫描看到镜像 ⇒ 出现/消失帧由**收件人所在区**发出，
  内容取镜像（= owner 的本 tick 终态）⇒ 与基线**同序同字节**（镜像占同一格，扫描序一致）。
- 要扩的东西：
  - `HaloEntity` 增帧面字段：`dir`、`death`、`mount_id`、展示项（feature/light/name）与玩家 `route`；
    体量上限沿用 `MIR2_MAX_HALO_BAND_ENTITIES`（512，6a 启动门已拒超限布局）。
  - 镜像实体：新 `mirror` 标志；**不进 AI / 仇恨 / 攻击目标 / 碰撞权威**。
  - **指向镜像的攻击/交互不默认拒绝**（Leader 2026-11-22 裁决）：打包成 `CrossZoneIntent` 投给
    owner 区的**下一 tick** 处理（1 tick 语义差见 §8 已知差异表），并计数 `cross_hit_delayed`；
    **只有未支持的类型**才拒绝并计数（`cross_intent_refused`）。
    说明：`CrossZoneIntent` 是**本阶段新增**的构件（仓内尚无此类型；`region_loop` 现有的是
    `CrossZoneOutbox`，方向相反）：入站意图队列 + 目标区 tick 顶排水 + 每 tick 合并账本。
  - **6b 账本口径要改写**：单一发射者从「`src == owner_zone`（按坐标）」改为「每
    `(tick, owner_entity, recipient)` 唯一发射者，**发射者 = 收件人所在区**」；否则镜像模型恒红。
- 可证伪断言：
  - **A6** 脚本场景（边界两侧各 2 实体、观察者扫过 10 tick）下，每收件人**全量有序帧流（字节）**
    在拆区与单区基线间逐帧相同。
  - **A7** 镜像从不被模拟：`mirror_ai_actions=0`、`mirror_damage_taken=0`，`cross_hit_refused`
    与脚本注入的攻击次数相等。
  - **A8** 新口径账本：`dup=0 / sent==recv / never=0`。

## 4. Q4 跨界移动 —— 推荐 (b) 两阶段 handoff

- (a) 拒绝（仅边界全不可走）：只能作**布局优选**，不能作运行时规则——怪物游走与
  `SPACEMOVE`/随机传送不看地形；一张 20×10 的区若边界在主路，玩家会撞「看不见的墙」。
- **(b) 推荐**：
  1. source 区在移动判定里发现目标格不在本区 rect ⇒ 实体置 `HandoffPending`（拒绝后续输入），
     向目标区发 `HandoffOffer{实体全量状态, 会话, at_tick}`；
  2. 目标区 tick 顶排水：校验坐标属本区 ⇒ 落实体（**同一全局 actor id**）+ 挂会话 ⇒ `HandoffAck`；
     校验失败/队列满 ⇒ `HandoffReject`；
  3. source 拿到 Ack 才**提交**；Reject / 超时 ⇒ **回滚 = 不提交**（实体停在原格、移动被拒、
     `handoff_refused` +1）。**原子性由此而来：所有权只在 Ack 后切换，源侧从未放手，
     失败路径不需要"恢复状态"**；
  4. 时序：offer/ack 在 barrier2 之后、下一 tick 之前交换；实体在 tick T 由 source 模拟、
     tick T+1 起由 target 模拟——两侧屏障同一 tick 号 ⇒ **不会两区同 tick 同时模拟**；
  5. 滞回：只在实体中心格进入目标区 ≥1 格时 handoff；边界带靠镜像保持可见 ⇒ 不闪烁。
- 可证伪断言：
  - **A9** 脚本走位（跨边界 3 次）下每收件人有序帧流与单区基线相同（**跨边界不产生额外
    appear/disappear**）。
  - **A10** 注入目标区拒收 ⇒ 实体仍在 source 区、位置未变、`handoff_refused` +1。
  - **A11** 任意 tick 实体数守恒：`Σ_z |World_z| == 基线实体数`。

## 5. 分步落地（每步独立可验收；第一步 = 零变化）

1. **零变化骨架 + 每区 actor id 段（硬前置）**：`MIR2_ZONES` 未设 ⇒ 新代码不执行；单区
   （`zone_count==1`）也走原路径。本步必须同时落地 **`World::set_id_base(zone * ACTOR_ID_SEGMENT)`**
   并把「**任意两区的 actor id 集合不相交**」写成断言（否则第②步一开就撞 `recog` 重复、
   账本确定性误判 `Duplicate`）。另含：`HaloEntity` 扩字段（N=1 不构造快照 ⇒ 零影响）、
   镜像存储与标志（feature 关）。
2. **镜像 + 跨区可见性（静态实体）**：A6/A7/A8 在 N=2 用例里立。
3. **登录落区**：入场队列 + fail-closed；A3/A4/A5。
4. **装载切片**：ctor fold 全局生成 + 分发；MonGen 全局决策 + 分发；A1/A2。
5. **handoff（先玩家、同图）**：A9/A10/A11；再推广到怪/宝宝。
6. **跨区收口**：存档/关服保存按区聚合（区 0 汇总）；guild/castle 事件按需跨区。

每步的合并条件：`MIR2_ZONES` 未设 ⇒ 五门 + `-Mode Both` 逐字节不变；单区 ⇒ 同上。

## 6. 我认为不可行 / 有确定性风险的点（反例与数字）

- **owner 直发模型**：逐帧同序不可达（§3 的约 90% 反例）⇒ 只能退到「同 tick 同多重集」的弱验收；
  不建议把它当 L2-5 的验收口径。
- **(a) 仅按地形拒绝**：需要 159 张图的边界格全不可走，且约束游走/传送不用地形——两者都做不到。
- **不给每区 id 段**：两区同 tick 可分配相同 id ⇒ `recog` 撞车、账本误判 `Duplicate`，**确定性失败**。
- **镜像的交互代价**（登记为已知差异，见 §8）：指向镜像的命中走 `CrossZoneIntent`、**目标区下一
  tick 生效** ⇒ 区界附近的命中比单区**晚 1 tick**（`cross_hit_delayed` 计数）；未支持的类型才拒绝
  （`cross_intent_refused`）。**不以"拒绝"作 v1 默认**：单图切 10 区后边界线横穿地图，
  "打不到"是玩家可感知的玩法缺陷，比 1 tick 延迟严重得多（Leader 裁决）。

## 7. 语义修订留痕：L2-4 的「实体所属区 = 唯一发射者」被 L2-5 取代

**保留历史形态，不就地删除**（半年后要能说清为什么改）。

**L2-4 形态（已被取代，仍适用于 N=1 与 L2-4 验收）**
- 判据：`CrossZoneOutbox::submit` 的来源闸 `src == owner_zone`（owner 区 = 实体坐标所属区）；
  账本 `check()` 对 `NotOwner` 报红。
- 落地位置：`region_loop::CrossZoneOutbox` 的来源闸；`AoIFrameLedger::note` 按坐标算 owner 区；
  用例 `l2_multi_zone_ledger_tests::ledger_rejects_a_frame_from_the_wrong_zone`。
- **取代理由**：oracle 的出现帧序 = 观察者自身的 `VisibleActors` 入队序（扫描格序）；owner 区看不到
  观察者的扫描序列 ⇒ 直发只能把远端帧追加在本地帧之后。反例与数字：观察者视野内 3 本地 + 2 远端
  （5 个目标，10 种两帧相对序），只要 1 个远端早于 1 个本地出现就不同序 ⇒ 约 90% 场景错序。
  若坚持旧判据，镜像模型下「收件人区代发」会被判 `NotOwner` ⇒ 判据与模型二选一。

**L2-5 形态（新判据，可执行）**
- 断言：对每个 tick 与每个 `(owner_entity, recipient)` 组合，**恰有一个发射者**；该发射者 =
  **收件人所在区**（镜像模型）；实体状态只从 owner 区的本 tick halo 快照读取（`HaloExchange::read`
  对陈旧槽返回 `None` ⇒ 失败而非旧值）。
- 落地：账本 key 保持 `(tick, owner, recipient, kind)`（重复检测不变）；`src == owner_zone` 的
  来源闸在 L2-5 模式下**改为**「`src == recipient 所在区`」，旧模式保留为启动参数/构建期二选一，
  旧用例保留并标注「L2-4 形态（已被 §7 取代）」。
- 新断言的最小失败用例：让非收件人区替收件人区发帧 ⇒ 新闸红；让 owner 区直发（旧形态）⇒ 新闸红
  （两个方向都要有负例，防「闸被拆掉后两边都绿」）。
- **实现形态（②第 2 步落地）**：`region_loop::EmitterCriterion::{OwnerByRect, RecipientZone}`；
  `CrossZoneOutbox::new` 保持旧口径（L2-4 对照/旧用例不动），镜像 rig 用
  `CrossZoneOutbox::with_criterion(.., RecipientZone)`。双向负例
  `emitter_criterion_recipient_zone_has_two_way_teeth`：收件人区发 ⇒ 绿；非收件人区发 ⇒
  `NotOwner`；旧口径下同一帧仍红。
- **待办（latent 双轨风险）**：当前生产 `sim_loop` 没有 `CrossZoneOutbox`（账本恒 `None`）⇒
  不存在双轨；将来生产启用账本时，**生产构造点必须 `with_criterion(RecipientZone)`**，并加
  一条源码牙（结构判定/grep）钉住生产构造点，防止顺手 `new(..)` 落回旧口径。

## 8. 已知差异表（L2-5 起生效，必须进发布说明）

| 差异 | 量级 | 计数/判据 | 处置 |
| --- | --- | --- | --- |
| 区界命中延迟 | **+1 tick**（`CrossZoneIntent` 目标区下一 tick 处理） | `cross_hit_delayed`；断言 = 脚本注入的跨界命中数 | 登记为已知差异；不做「拒绝」默认 |
| 未支持意图拒绝 | 按类型 | `cross_intent_refused` | 逐类型登记，类型表随实现更新 |
| 区界附近镜像不可交互（交易/拾取/脚本查找） | 1 tick 或拒绝 | 同上 | 随 `CrossZoneIntent` 类型表推进 |

## 9. 交接要点（第②步：镜像 + 跨区可见）

前提：第①步已合（每区 `ActorId` 段、两区 id 集合不相交、区数启动门 512）。

**接缝清单（实现时逐个核）**
1. **唯一发射点**：`drain_aoi_downlink` 的 `for body in bodies` 循环。镜像必须作为**本地 `World` 的实体**
   进入 AOI 扫描（否则观察者的扫描序不可复现）；不要在发射点旁加第二处记账。
2. **镜像用 owner 的全局 actor id 插入**：`EntityStore` 的 `insert(id, value)` 支持指定 id ⇒ 镜像不分配
   新 id，`recog` 与 owner 一致（第①步的 id 段保证不冲突）。
3. **`HaloEntity` 扩帧面字段**：`dir`、`death`、`mount_id`、展示项（feature/light/name）与玩家
   `route(line, socket)`——只为让镜像能生成**与基线逐字节相同**的出现帧。
4. **镜像刷新位置**：`ZoneRuntime::after_phase1`（barrier1 之后、本 tick 快照已是终态）返回 `neighbors`
   后、`world.tick_phase2`（AOI）**之前**应用；不要在 phase1 之前刷新（会读到上一 tick 的旧值）。
5. **镜像必须被隔离**：`mirror` 标志；不进 AI/仇恨/攻击目标/碰撞权威；MonGen 不生成镜像；
   指向镜像的命中走 `CrossZoneIntent`（目标区下一 tick；计数 `cross_hit_delayed`）。逐个列出并加用例。
6. **消失帧天然覆盖（已证伪，2026-11-22）**：原判断「镜像被移除时 `rec.hides` 会包含它」**不成立**。
   镜像离带 = **删除实体**；而 `hides` 对**被删实体**恒不产出——`aoi.rs` 的 legacy
   （`search_view_range_with_updates`）与增量（`incremental_gate`）两处收尾都过滤
   `store.get(t).is_some_and(|e| !e.fixed_hide_mode && !e.ghost)`，被删目标过不了 `is_some_and`。oracle 用**保留引用**
   （`PlayObject.cs:3206-3218`，取 `visibleBaseObject.BaseObject`，**不查环境里还在不在**），
   守卫只有 `!FixedHideMode && !Ghost` ⇒ 这是**移植引入的既有分歧**，不是镜像带来的新语义。
   修复独立开卡 `card/aoi-disappear-on-removal`；C10 复现用例以 `#[ignore]` 冻结在树里。
7. **账本口径切换**（§7）：`CrossZoneOutbox::submit` 的来源闸由「`src == owner_zone`（坐标）」改为
   「`src == 收件人所在区`」；旧判据保留为 L2-4 形态并标注；两个方向都要负例。
8. **跨线程 outbox 形态**：6b 的 N=2 用例用 `Mutex<CrossZoneOutbox>` + 第三道屏障（tick 末 seal）。
   生产形态二选一——「每区一个 outbox + 区 0 汇总」或「共享大锁 + 屏障」；**建议前者**（锁粒度小，
   汇总点唯一），并在文档里写明选择理由。
9. **规模**：镜像数 = 邻区边界带实体数，受 `MIR2_MAX_HALO_BAND_ENTITIES`（512，启动门）约束；
   镜像存储建议复用实体存储 + 标志位，不另建索引。

**②内部的落地顺序（每步保持 `MIR2_ZONES` 未设零变化）**
(a) `HaloEntity` 扩字段 + 镜像插入/刷新（feature 关、单区零变化）；
(b) 边界带可见集与出现/消失帧对拍（静态实体，A6/A7/A8）；
(c) 账本口径切换（§7 新判据 + 双向负例）；
(d) 移动实体的镜像同步（先静态→后走位），再谈 handoff（§4）。

### 9.1 ②第 1 步（零变化骨架）落地备注

- **镜像插入不走 `Command::Enter` 队列**（`World::upsert_mirror` 直接进实体存储 + 同步挂格）：
  本 tick 的 phase2（AOI）能看见它，但经 `Enter` 队列消费的系统（`record.processed` 等）
  **本 tick 看不到**。这是**已知不对称**；②第 3 步若出现依赖「Enter 后状态」的隔离判据，
  必须回头改 `upsert_mirror` 的时序，而不是在调用方补写记录。
- **`MIR2_ZONE_MIRROR`（fail-closed）**：`unset`/`0` ⇒ 关；`1` ⇒ 开；其它取值、或
  `MIR2_ZONES` 未设/只解析出 <2 区时置 `1` ⇒ **拒绝启动**（点名两个变量；避免「以为开了
  镜像、其实没有」）。闸关时 `apply_mirrors` 整条路径零参与 ⇒ 段 6a 的 N≥2 行为逐字节不变。
- **`cell_order` 不进第 1 步**（Leader 2026-11-22 裁决）：第 2 步先立「同格多实体逐帧同序」
  判据，再决定快照该带格内追加序、还是 owner 侧全局序；若最终需要，必须在该步把
  「为什么是这个序」写进 doc 与断言，不许只加字段。
- **快照过滤 `mirror` 必须配反向牙**：镜像不得被再次发布成 owner 实体（否则镜像的镜像），
  但**必须进本地 AOI 扫描**（否则「屏蔽」会把可见性一起屏蔽、帧消失）。两条断言同处一测。
#### 9.1.1 已知直写点（`bf` 复核 Q4 采纳；取代/补齐上面的「不走 Enter 队列」一条）

`World::upsert_mirror` 及其辅助函数**全部绕过 oracle 的模拟入口**直写世界/格表。逐点清单
（②第 3 步做隔离判据时必须逐条对，任一漏掉都会让「不参与模拟」变成注释）：

1. **新插入**：`EntityStore::insert` + `map_binding.add_to_mapped = true` + `MapGrid::add_map_object`
   （**不**入 `Command::Enter` 队列 ⇒ `record.processed` 等本 tick 看不到）。
2. **换位**：新格 `add_map_object` + 摘旧格（`remove_mirror_cell_object`）+ 直写 `Transform`
   （x/y/map_id/view_range）。基线里真实实体的换位走 `MoveToMovingObject`（含其**跳过前移项**
   的 oracle 怪癖）；镜像换位不复刻该怪癖 ⇒ **同格相对序会不同**（见 9.1.2）。
3. **`remove_mirror`**：摘格 + `EntityStore::remove`（不经过 `leave_world`/`Closed` 路径，
   也不产生玩家离场语义）。
4. **`touch_mirror_cell_object`**：直写格内 `CellObject.add_time`（模拟 owner 的
   `verify_map_time` 续期，但节奏是每 tick 而非 30s）。
5. **`update_mirror_frame_fields`**：直写 `Appearance`（race/race_img/appr/light/name_color）、
   `LifeCycle.death`、`AiState.dir`。**`death` 绕过 `Die()` 语义**（无死亡结算/无 `DeathSettled`）
   ⇒ ②第 3 步若有「镜像不参与死亡结算」的判据，必须以这里为准；不得假设镜像的 `death`
   只能经 `Die()` 到达。

#### 9.1.2 第 2 步落地（Q1/Q2 裁决）：`cell_pos` 格内序

- **快照新增 `HaloEntity.cell_pos`** = owner 侧该格 `obj_list` 内的下标（非 actor 对象也计入，
  保证相对序单调）。**已落格却查不到 ⇒ `debug_assert!`**（格表与 `add_to_mapped` 失同步 =
  程序错误；静默退化成槽位序会掩盖错位），release 兜底 `usize::MAX`。
- **`apply_mirrors` 先按 `(map_id, x, y, cell_pos, actor_id)` 定序再 upsert**：镜像的格子登记
  是「追加」，只有按 owner 侧格内序追加，收件人世界的 `obj_list` 序才与基线一致。
- **为什么是「格内进入序」而不是槽位序**：oracle 扫描按 `obj_list` 下标序出帧
  （`search_view_range_with_updates` → `VisibleActors` 入队序 → `drain_aoi_downlink`），而槽位
  序是实体存储的分配序；「后进先出 / 延迟 `Enter`」时两者不等。用槽位序会让同格出现帧相对序
  与基线不同（Q1 裁决认可的口径）。
- **牙的映射**：**布局 2/3 是真牙**——`add_entity` 把 `Enter` 入队、`enter_map` 当场进格 ⇒
  「槽位序 ≠ 进入序」在生产可达；删掉定序（回到槽位序）即红。**布局 1 是回归/对照，不是牙**
  （槽位序==进入序，结构上恒绿）；反序定序能打红它，但那是**人工变异、仅证断言可动，不是可达
  缺陷**（没人会把格内序写成逆序）。

#### 9.1.2 原判据草案（Q2）

镜像新插入恒排在格尾（`add_map_object` 追加）；基线里「远端实体先于本地实体落格」时相对序会
反转。**本步不改**；②第 2 步必须立「同格多实体逐帧同序」的**正反牙**（含：同格 本地+镜像、
同格多镜像、远端先落格三种夹具）。`cell_order` 是否加，由该判据的数据决定；若要加，必须把
「为什么是这个序」写进 doc 与断言。

#### 9.1.3 待办（Q3）

`upsert_mirror` 是 `pub`：`id <= 0` 目前会静默登记进 `IdIndex.sparse`（不 panic，污染
`recog=0` 与 `mirror_ids()`）。当前调用链 `HaloEntity.actor_id` 恒 >0 不可触发，但**②第 3 步
必须补**：`MirrorUpsert` 新变体（如 `RefusedInvalidId`）或 `debug_assert!`，并在用例里钉住。
