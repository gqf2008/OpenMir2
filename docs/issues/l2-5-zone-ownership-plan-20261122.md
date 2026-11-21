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

## 8. 已知差异表（L2-5 起生效，必须进发布说明）

| 差异 | 量级 | 计数/判据 | 处置 |
| --- | --- | --- | --- |
| 区界命中延迟 | **+1 tick**（`CrossZoneIntent` 目标区下一 tick 处理） | `cross_hit_delayed`；断言 = 脚本注入的跨界命中数 | 登记为已知差异；不做「拒绝」默认 |
| 未支持意图拒绝 | 按类型 | `cross_intent_refused` | 逐类型登记，类型表随实现更新 |
| 区界附近镜像不可交互（交易/拾取/脚本查找） | 1 tick 或拒绝 | 同上 | 随 `CrossZoneIntent` 类型表推进 |
