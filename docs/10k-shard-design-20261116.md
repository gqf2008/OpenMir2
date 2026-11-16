# 10k 并发世界分片设计（2026-11-16）

> **第一道墙不在游戏服务端（2026-11-16 实测补记）**：C# DBSrv 取档对 `characters_bagitem` /
> `characters_storageitem` / `characters_item` **无 `PlayerId` 索引**，每次进世界全表扫 ≈96 万行；
> 角色数增长 1.20× ⇒ 进世界速率下降 1.24×（实测吻合）。⇒ **10k 的第一道墙在夹具/数据库，
> 不在 game-svr；分片不解决它，索引才解决**（D-* 偏差卡加 PlayerId 非唯一索引；见 §0.1a 与
> `docs/dispersion-1000-conclusion-20261116.md` §7）。

- 卡：本设计件（建议卡号 `openmir2-world-shard-design`；本批只交付文档与拆卡清单，**不写生产代码**）
- 身份：`openmir2-worker-92`
- 分支：`card/world-shard-design`，基 master `34c20e7d3`
- 状态：设计稿（needs-review）。**本文不签名、不认领卡、不合并。**
- 前序证据：
  - `docs/capacity-model-20261114.md`（10k 容量模型 + T10c 优化）
  - `docs/tick-hotspot-profile-20261113.md`（World::tick 分段与线性缩放）
  - `docs/gateway-multiline-sharding-20261115.md`（多网关线实验）
  - `docs/loadgate-criteria-runbook-20261115.md`（压测判据与窗口有效性）
  - `docs/g11-planes-design-20261108.md`（跨服/Planes 的 oracle 真实状态）
  - `docs/AGENTS.project.md`（四条确定性硬约束、判据分级、写集切分）

> **口径声明（先读，全文适用）**
>
> 1. 本文的**实测数**只有两个来源：`docs/capacity-model-20261114.md` 与其引用的归档
>    （`E:/UserFleet/load-360-errorstack-20261115-2116/`、`E:/UserFleet/archive/gate-capacity-360-20261115b/`、
>    `E:/UserFleet/archive/gate-saturated-500-20261115/`、`E:/UserFleet/archive/gate-criticality-20261115-w83/`）；
>    以及 `docs/tick-hotspot-profile-20261113.md` 的 release bench。
> 2. 本文标「推算」的数字全部由上面两个来源的系数外推，**外推区间未实测**；每条推算都写明公式与假设。
> 3. **不把 500 档的 88.2ms 工作段与同一份报告里的 tick 15/16/32/172ms 混用**：两者口径不一致
>    （工作段是分段计时之和的窗口均值；tick 分位是另一路直方图），本文容量计算一律以 **88.2ms 工作段**
>    为成本基准，tick 分位只作门禁字段引用。
> 4. 「10k 在线」与「10k 恒压活跃」是**两个不同目标**（§1.1）。容量模型里 0.115–0.126ms/客户端
>    测的是**恒压活跃**成本；**空闲连接成本从未测过**（容量模型「假设 A」）。本文不把两者混写。
> 5. 本文**不提议**打开单 World 内并行；`bevy_ecs` 的单线程执行器与 `TickOwnerThread` 断言保持不变。
>    分片是「多个各自单线程的 World」，不是「一个 World 多线程」。

---

## 0. 结论摘要

### 0.1 推荐路线（一句话）

**先把每客户端/每实体的成本系数「量准并压下来」（P0 度量 + P1 单进程优化），再按地图组做
「同进程多 World、每个 World 固定单线程、World 间无共享可变状态、无共享 RNG」的分片（P2）；
多进程/多机只在单机 6c/12t 实测不足、且 Owner 批准协议级改动时启用（P3）。**

> **0.1a（2026-11-16 晚，D3/D1 对照实测补记 —— 分片不是这个瓶颈的解）**
> 把 1000 bot 从「单图 999/1000」分散到「6 图」后实测：网关侧**非关键帧丢弃 −69.5%、AVI −64.3%、
> `timeout_flushes` −98%**；服务端**工作段仅 −9.2%（SD≈19，噪声带内）**、`HostInbound` 反升 **+7.8%**，
> 但 `World::tick_tail` **−73.3%**（与 Σ(每图实体²) 同向）。
> ⇒ **多 World / 多进程分片不是「每客户端 AOI 扇出」这一层的解**：扇出由**同一张图里的实体数**决定，
> 分片只有在它同时降低每图实体数时才顺带降低扇出。**这一层的直接解是 AOI 空间索引 / 按图分桶**（归入 P1）。
> 分片的正当理由只有 CPU/内存横向扩展（§2）。证据与局限见 `docs/dispersion-1000-conclusion-20261116.md`。
> 另：`tick_tail ≈ 1.1 + 1.6×10⁻⁵ × Σ(每图实体²) ms` 是**两点拟合的假设（未验证）**，不得据此定系数。

### 0.2 三个候选的判定

| 方案 | 机制 | 单机 6c/12t 的定位（按实测系数推算） | 确定性/对拍 | 判定 |
| --- | --- | --- | --- | --- |
| **C 先抬单进程上限**（网关流水线 + HostInbound 批处理/索引） | 不加分片；把 `online`/`DisplayTable` 的 O(n) 查找、每事件 O(sessions) 扫描、可离线的纯解析搬到 I/O 线程 | 单进程上限从 ~1k 活跃抬到 ~1.1–2.8k（取决于压到多少）；**单独做不到 10k** | 保持 1 World 单线程；若只改数据结构/索引且不改变事件顺序 ⇒ 行为等价（tier L）；任何重排 ⇒ R | **必做第一步**（最便宜、风险最低、且是 A 的前置收益） |
| **A 同进程多 World**（地图组分片，每 World 固定单线程） | N 个 World 分到 T 个固定线程（N≥T，每线程顺序跑自己的 World）；协调线程管全局服务与跨片消息 | 把总量 813–937ms/tick 摊到多核：6 核墙钟 ~135–156ms（不含开销；含 1.1–1.3 开销 135–203ms）；**满足 200ms，不满足 100ms**；要 ≤100ms 需 ~8–10 个有效核或总量再降 1.5–2× | 每 World 自身确定性保持；**N>1 不再是 oracle 逐字节等价**（RNG 流被切开）；1-shard 模式保持逐字节等价 | **T2（10k 恒压活跃）的主路线**；T1（10k 在线、≤0.9k 同时活跃）可先不做 |
| **B 多进程世界分片** | 每片独立进程 + 跨片消息/握手 | 单机上不比 A 好（固定成本更高、IPC）；多机才线性放大：2×6c 或 1×10–12c 可到 ≤100ms | 跨进程异步 ⇒ 确定性更难；需实现 oracle 里**本就停用**的 G11 跨服 | **只在需要多机时启用**；需 Owner 批准协议级/多机 |

### 0.3 关键数字（全部为实测或明确标注的推算）

| 量 | 值 | 来源 |
| --- | --- | --- |
| 500 档有效轮次 | `in_world 500/500`、`HoldSec=360`、`conclusion_valid=true`、`internal_errors=0`、`rc=0` | 归档 `E:/UserFleet/load-360-errorstack-20261115-2116/`；`docs/capacity-model-20261114.md` §4.6 |
| 工作段（500 档、360s 窗口） | **88.198 ms/tick = 200ms 的 44.1%** | `docs/capacity-model-20261114.md` §4.6（提交 `262a95855`） |
| `HostInbound` | **62.932 ms/tick = 工作段的 71%** | 同上 |
| 每客户端 inbound | **0.126 ms/客户端/tick** | 同上 |
| `ΣWorld` / `HostAoiDownlink` / `HostPeriodic` | 10.786 / 3.952 / 4.654 ms | 同上 |
| 实体（该窗口） | 205→7,461（max 7,930），**仍在上升、未饱和 ⇒ 44.1% 是下界** | 同上 |
| 双因子拟合 | 工作段 `23.22 + 0.008834×E`（R²=0.9951，实体≥2,000）；`HostInbound/客户端 = 0.0300 + 1.1998e-5×E`（R²=0.9953） | `docs/capacity-model-20261114.md` §4.6「双因子模型」；归档 `gate-capacity-360-20261115b`、`gate-saturated-500-20261115` |
| 50% / 满预算客户端上限（仅 `HostInbound`） | **≈790**（0.126ms 口径）/ **≈913**（拟合 0.115ms）；满 200ms ≈1.6k / ≈1.78k | `docs/capacity-model-20261114.md` §4.6 |
| 按实测密度 17.7 的联合上限 | **50% ≈490；满 200ms ≈1,130** | 同上 |
| 10k 恒压活跃外推 | **≈1,147 ms/tick = 5.7× 预算** | 同上（拟合 `−5.25 + 0.11526×clients`） |
| 实体轴 | `World::tick` **1.209 µs/实体**；39.5k 实体 = 49.2ms = 24.6% 预算；World 单独触顶 ≈160.6k 实体 | `docs/capacity-model-20261114.md` §3/§4.2 |
| 10k 场景实体估算 | 基准 39,500；内容上限 53,519（MonGen 39,019 + 10k 玩家 + 宝宝 + 其它） | `docs/capacity-model-20261114.md` §4.1 |
| 同图夹具密度 | **17.7 实体/客户端**（500 客户端 → 8,874 实体）；同图 AOI O(N²) 是上界，分散场景未测 | `docs/capacity-model-20261114.md` §4.6「同图口径警告」 |
| 空闲连接成本 | **未测** | `docs/capacity-model-20261114.md` 顶部「假设 A」 |
| 网关多线 | 关键性版 N=4：`in_world 500/500`、`close_inworld=0`、`overflow=0`；但多线只并行发送侧，**所有线仍喂同一个 sim 线程** | `docs/gateway-multiline-sharding-20261115.md` |
| 1000 档 | 425/1000 进世界；BotSrv 4GiB GC 上限 OOM、网关 `SendChecked>0` 静默丢帧、进世界串行载入 | `docs/capacity-model-20261114.md` §4.6；`docs/botsrv-heap-1000bot-20261115.md` |

### 0.4 必须拍板的决策点（详见 §5）

1. **目标到底是哪一个**：10k「在线」（允许同时活跃 ≤0.9k，容量模型的设计点）还是 10k「恒压活跃」？
2. **是否接受 N>1 分片模式不是 oracle 逐字节等价**（每 World 独立 RNG；跨片消息一拍延迟）？
3. **100ms 是否作为 10k 的硬验收**？若是，单机 6c/12t 在实测系数下做不到（§3.4）。
4. **是否允许完成 G11 跨服/协议级改动**（多进程方案的前提；oracle 里 G11 全停用）。
5. **是否允许跨片换图有 1 tick 的过渡**（换图瞬间的可见性/攻击窗口）？
6. **是否必须支持「10k 全在同一张图」**（地图是分片原子单位，单图不能拆；该场景只能靠降密度/AOI 改造）。
7. **是否接受 200ms（而非 100ms）作为 10k 的 tick 上限**，以及是否接受多机资源窗口。

---

## 1. 目标、现状与基线

### 1.1 两个目标必须分开写

容量模型已经明确（`docs/capacity-model-20261114.md` 顶部「假设 A」）：

- **T1 = 10k 在线、同时活跃 ≤0.9k**：0.9k 是给整栈 50% 预算（100ms）的设计口径；
  ≈1.78k 是 `HostInbound` 独占 200ms 的理论上界，**不是设计点**。
- **T2 = 10k 恒压活跃**：每个客户端每 tick 都可能产生需要解析/派发的动作，
  即 0.115–0.126ms/客户端/tick 的实测口径。

**空闲在线连接成本未测**，所以 T1 目前只有「设计口径」，没有实测证据；T2 有 200/500/1000 三档实测与拟合。
本文的容量表默认按 **T2 的系数**算（保守）；T1 的结论必须等 P0 的空闲连接测量。

> 这条不是文字游戏：同一个 500 档，恒压夹具测出 62.9ms 的 `HostInbound`，而真实玩家不会每 tick 都动作。
> 若把 T2 的数字直接写成「只能接 500 个玩家」，和把它当乐观估计一样不准确（容量模型 §4.6 原文警告）。

### 1.2 不可动项（硬约束）

1. **客户端冻结**：协议逐字节兼容，不改客户端。
2. **内容冻结**：不改表结构、脚本语法、数据文件格式。
3. **行为等价优先于修 bug**：oracle 的 bug 也照搬，偏离走 `T-*`/`D-*`/`B-*` 白名单。
4. **单 World 内不并行**：`bevy_ecs` 显式 `SingleThreadedExecutor`；`crates/world-bevy/tests/no_parallel.rs` 的源码扫描
   禁止 `MultiThreadedExecutor`/`bevy_tasks`/`rayon`/`crossbeam` 等并行 API；`World::tick` 有
   `TickOwnerThread` 运行期线程断言（`crates/world-bevy/src/tick.rs`）。
5. **遍历顺序显式化**：顺序只能来自 `SlotTable`/`OrderIndex`，不能来自 `Query` 迭代序；
   `TickStage` 阶段链是契约（`crates/world-bevy/src/tick.rs`）。
6. **跨图分布式「另行设计」**：`docs/AGENTS.project.md` 明确写了 oracle 是「单 GameSvr 单区单 World」，
   跨图靠会话进/出世界，不存在无缝大地图/万人同服；本文就是那份「另行设计」。

### 1.3 架构现状（与本设计相关的部分）

- **一个 game-svr 进程、一个 sim 线程**：`bins/game-svr/src/lib.rs` 的 `sim_loop` 拥有唯一 `World`；
  I/O（tokio）侧只发 `GateEvent`，绝不碰 `World`。
- **一个 tick 的顺序**（宿主段）：`HostInbound` → `HostDbPoll` → `HostPeriodic` → `HostWorldHead`
  → `HostPostHead` → `HostPlayerRun` → `HostWorldTail` → `HostPlanes` → `HostRecovery` → `HostFrames`
  → `HostDealTick` → `HostAoiDownlink` → `HostPersist` → `HostSwitch` → `HostDiag` → `HostSleep`。
  分段名由 `bins/game-svr/src/tick_profile.rs` 定义，慢 tick 门是 `MIR2_TICK_PROFILE_SLOW_MS`。
- **一个 World 装全部地图**：`World.maps: Vec<MapGrid>`（`crates/world-bevy/src/world.rs`），
  实体有 `MapBinding`/`Transform.map_id`；AOI 按 `map_id` 分桶（`crates/world-bevy/src/aoi.rs`），
  **地图之间除全局服务外没有模拟交互**。
- **网关线**：一个 game-gate 进程的 N 个端点各持一条到 game-svr 的连接，`service_id` 是端点下标；
  game-svr 用 `gates: HashMap<u8, Sender>` 按线号回帧。`GATE_SLOT_CAPACITY=20`、每连接
  `MAX_SESSION=10,000`（`bins/game-gate/src/lib.rs`、`bins/game-svr/src/gate_queue.rs`）。
  **所有线的事件都进同一个 `ev_rx`，由同一个 sim 线程 drain**。
- **全局服务在宿主侧，不在 World 里**：`online`（在线路由表）、`ground_sessions`、`displays`
  （展示表）、`saves`、`trade_sessions`、`shop_sessions`、`guilds`、`castle`、`item_number`、
  DB 写队列、IDS 上报。这是分片时最需要切割/搬迁的部分。
- **跨服 G11 是死的**：oracle 的 PlanesServer 主服从不监听；41 个 case / 32 个 handler 方法中
  EMPTY/ASSIGN/NOOP/PARSES-DISCARD/THROWS 全部覆盖，**0 个有真实下游副作用**
  （`docs/g11-planes-design-20261108.md` §4）。
  Rust 侧 1:1 复刻该停用状态。**多进程方案不能「复用现成 G11」，必须新造或补全。**

### 1.4 已看见的放大项（代码事实，不是新测量）

这些是「为什么不能把 500 档线性外推到 10k」的具体理由，也是方案 C 的抓手：

1. **`OnlineRegistry = Vec<OnlineEntry>`**（`bins/game-svr/src/lib.rs`）：按 `chr_name`/`actor_id`/`socket`
   线性查找；仓内 `online.iter().find/position` 约 **100 处（含测试模块）**。单次 O(在线数)，每命令可能多次。
   AOI 出现帧路径的注释也明确写了「`online` 是按插入序的 `Vec` ⇒ 这里的按名查找是 O(会话数)」。
2. **`DisplayTable { entries: Vec<(i32, ActorDisplay)> }`**（`bins/game-svr/src/downlink.rs`）：
   `set/get/remove` 全是线性扫描。展示表里既有玩家也有怪物/NPC/分身，规模随实体数增长。
3. **每事件构造 O(session) 集合**：例如断线清理里为每个断开者重建 `guild_ban: HashMap`（扫全部
   `ground_sessions`）；聊天路径构造 `guild_recipients`。这些在 500 档是毫秒级，在 10k 档会变成
   每事件 O(10k) 的常数项。
4. **`world.entities.get` 是 O(1)（槽位表）**，但部分命令会走 ±12 格 `SendRefMsg` 冷路径与
   `map_cell_object_ids`；在同图高密度夹具里，单命令的可见目标数随密度增长。
5. **tick 内多次全量扫描**：World 侧五个全量扫描段已由 T10c 降到 ~0.057µs/实体/段
   （`docs/capacity-model-20261114.md` §3），但宿主侧的 O(在线数) 查找不在那次优化范围内。

> **结论**：10k 外推的 1,147ms 是「每客户端 0.115ms × 10k」的线性外推；**若上述 O(n) 项是真实
> 超线性来源，真实值会更高**。容量模型自己也写了「线性仅在 500–8,500 实体、500 客户端验证；
> 10k 活跃客户端在实体密集场景不成立」。这是 P0 必须先量 2D 网格、而不是直接上分片的原因。

### 1.5 基线模型（本文所有推算的唯一公式）

从 500 档 360s 窗口的双因子拟合（`docs/capacity-model-20261114.md` §4.6）：

```
工作段 W(C,E) = 23.22 + 0.008834×E          (C=500, E=实体数, ms/tick)
HostInbound H(C,E) = C × (0.0300 + 1.1998e-5×E)
```

把固定分量按客户端数线性化（23.22 / 500 = 0.04644），得到**本文使用的总成本模型**：

```
W(C,E) ≈ 0.04644×C + 0.008834×E   (ms/tick)
```

- **校验**：C=500、E≈8,000 → 93.9ms；实测 88.2ms（+6.5%，拟合本身的误差）。
- **假设**：固定分量随 C 线性；E 的系数在 10k 档不变；**没有 C×E 交互项**。
- **可辨识性缺口（重要）**：`H(C,E)` 的 `1.1998e-5×C×E` 项只在 C=500 上测过。
  若它是因果的（每命令成本真的随世界实体数线性增长），外推到 C=10k、E=39.5k 会得到
  `10k×(0.03+0.474) = 5,040ms`，比线性总模型大 6 倍；若它只是「同图密度/会话年龄」的代理，
  则不会这样爆炸。**现有数据无法区分这两者**——这正是 P0 的 2D 网格实验要回答的问题。
  在得到答案之前，本文的容量表用线性总模型（较乐观），并在 §6 把它列为一级不确定性。

### 1.6 单线程上限（由模型直接推出）

`W(C,E) ≤ 200ms`（满预算）或 `≤100ms`（50% 预算）：

| 密度 d=E/C | 满预算 200ms 的单线程客户端上限 | 50% 预算 100ms 的上限 | 备注 |
| --- | --- | --- | --- |
| 17.7（实测同图夹具） | ≈986 | ≈493 | 与容量模型给出的 ≈790（50%）/≈1,130（按密度联合上限）同量级 |
| 5.35（10k 客户端 / 内容上限 53.5k） | ≈2,134 | ≈1,067 | **分散场景，未测** |

⇒ **「把玩家分散到 207 张图」本身就能把单进程上限抬约 2×**（因为每客户端成本随同图实体密度下降）。
这不是分片，是部署/密度问题；P0 必须先量它。

---

## 2. 方案对比

### 2.1 方案 A：同进程多 World（推荐主路线）

#### 2.1.1 机制

- **分片原子 = 地图组**：把 207 张图按连通性/密度/业务（城堡、行会驻地、新手村）分成 N 个
  **地图组**，每个 World 只装自己组的 `MapGrid` 与实体。一张图不能拆给两个 World
  （地图内的 AOI/碰撞/战斗是强交互的）。
- **每 World 固定单线程**：每个 World 有且只有一个 owner OS 线程（`TickOwnerThread` 断言保持）。
  N 个 World 分配到 T 个固定 worker 线程（**N ≥ T**），每个 worker 顺序 tick 自己的 World；
  **World 不迁移线程**（否则 owner 断言与确定性都破）。
- **World 间无共享可变状态**：每个 World 有自己的 `SessionRegistry`、`ground_sessions` 分片、
  `displays` 分片、`saves` 分片、**自己的 RNG**。不共享 `World`、不共享 RNG、不共享可变借用。
- **协调线程（Coordinator）**：拥有全局服务——在线索引（chr_name→shard/actor/socket/gate）、
  行会、组队、城堡、全局聊天/私聊、GM、DB 存档队列、`item_number`、IDS/公告。
  跨片消息走有界通道；**协调线程不抽 RNG**。
- **tick 相位**：所有 worker 在同一 200ms 节拍上；每个 World 在自己的 tick 内固定位置
  （建议在 `HostInbound` 之前）drain 跨片 inbox；tick 结束后把 outbox 交协调线程；
  协调线程在下一拍的开头按 `(thread_id, shard_id, seq)` 顺序分发。**跨片消息一拍延迟**。
- **跨片换图（TransMap 到别组地图）**：在 tick 边界做受控 handoff：冻结玩家、序列化最小状态
  （坐标/属性/HP/MP/buff/冷却/背包/组队/行会/宝宝）、经协调线程转到目标 World、下一拍插入。
  源 World 先发离开帧、目标 World 再发进入帧；**中间一拍玩家不可见**。这是偏离，必须登记。

#### 2.1.2 容量估算（推算）

在 §1.5 的线性总模型下，**分片不改变总 CPU**，只把它摊到多核：

```
总工作段 W_total ≈ 0.04644×10,000 + 0.008834×E
                = 464.4ms + 0.008834×E
基准 E=39,500 → 813ms/tick
内容上限 E=53,500 → 937ms/tick
```

每片工作段（线性模型下等于 `W_total/N`）：

| N（World 数） | 基准 39.5k 实体，每片 ms | 内容上限 53.5k 实体，每片 ms | 每片 ≤100ms？ |
| --- | --- | --- | --- |
| 4 | 203 | 234 | 否 |
| 6 | 136 | 156 | 否 |
| 8 | 102 | 117 | 否（刚好超） |
| **10** | **81** | **94** | **是** |
| **12** | **68** | **78** | **是** |
| 16 | 51 | 59 | 是（但收益递减） |

- **片数 N 的作用**：让单片工作在 200ms 死线内、并尽量 ≤100ms；N=10–12 是甜点。
- **线程数 T 的作用**：决定墙钟。`每线程墙钟 = W_total / T`（因为每线程跑 N/T 片，
  `(N/T)×(W_total/N) = W_total/T`，与 N 无关）。
  - T=6（6c/12t 按 6 个有效核，扣掉网关/DB/OS）：813/6=**135ms**（基准）、937/6=**156ms**（内容上限）。
  - T=8：102 / 117ms。
  - T=10：81 / 94ms。
- **结论**：
  - **6c/12t 单机可以做到 10k 的 200ms 预算（135–156ms），但不能保证 100ms**：
    100ms 需要 T≥8.1（基准）/T≥9.4（内容上限）个有效核。
  - 若 P1 能把总成本再降 25%：基准 697ms、内容 821ms ⇒ T=6 墙钟 116/137ms，T=8 为 87/103ms。
  - 若降 50%：基准 581ms、内容 705ms ⇒ T=6 墙钟 97/118ms，T=8 为 73/88ms。
  - ⇒ **要同时满足「10k 恒压活跃」与「tick P99 ≤100ms」在 6c/12t 上，需要 P1 总量降约 50%
    （或更准的测量证明总量本来就更低），否则只能接受 ≤200ms 或多机。**
- **上表是推算，不是承诺**：它假设线性总模型、无 C×E 交互、无 O(n²)。若 P0 证明 C×E 是因果的，
  10k 的真实总量可能是数秒/tick，6 核分片也救不了，必须先做 P1 的 O(n) 消除与密度治理。

#### 2.1.3 确定性/对拍

- **每 World 单线程**：`TickOwnerThread` 断言不变；每个 World 的 `STAGE_ORDER`、`SlotTable` 顺序、
  实体次序都不变 ⇒ **单片内确定性保持**。
- **每 World 独立 RNG**：`World.rng` 是每 World 私有；协调线程不抽 RNG；跨片消息不抽 RNG。
  ⇒ 同一 N、同一种子、同一输入 ⇒ 同一输出（可重放）。
- **但 N>1 不是 oracle 逐字节等价**：oracle 只有一条 RNG 流，按全局槽位序抽数；分片把流切开，
  RNG 依赖的行为（掉落、AI 决策、随机落点）在跨片场景下与 oracle 不同。
  **1-shard（`MIR2_SHARDS=1`）必须保持与 master/oracle 逐字节等价**；N>1 是新的、有自己判据的模式。
- **跨片消息一拍延迟**：聊天/行会/组队有 ≤200ms 延迟；换图有一拍不可见。登记为偏离（D-*）。
- **守卫**：扩展 `no_parallel`/`TickOwnerThread`，增加「一个 World 只能被一个线程 tick」
  「协调线程不持有 RNG」「线程不共享 `&mut World`」的源码/运行期牙（见卡 P2-6）。

#### 2.1.4 改动面

| 区域 | 改动 |
| --- | --- |
| `bins/game-svr/src/lib.rs`（91,823 行） | 把单 World 宿主循环拆成 `ShardHost`；`online`/`ground_sessions`/`displays`/`saves` 分片化；事件按 shard 路由；跨片消息类型 |
| 新模块 `bins/game-svr/src/shard.rs` | ShardId、World 实例、线程归属、tick 相位、handoff 载体 |
| 新模块 `bins/game-svr/src/coordinator.rs` | 全局服务、跨片消息、DB 写队列、在线索引、tick barrier |
| `bins/game-svr/src/config.rs` | `MIR2_SHARDS`、地图组文件、默认 1 |
| `bins/game-svr/src/tick_profile.rs` | 每行加 `shard=<id>`/`thread=<id>` |
| `bins/game-gate/src/lib.rs` | 不需要分片感知（会话仍按端点/线号粘性）；可能需要「一个 gate 端点可带多 shard 的会话」的路由字段 |
| `bins/db-srv/src/*` | 同进程方案**不需要**改；存档仍按账号/角色走 |
| `crates/world-bevy/src/*` | **语义不改**（World 本来就是独立实例）；只在守卫测试里加「多实例/线程归属」断言 |
| `mir2-rs/tests/parity/*` | 1-shard 回归不变；新增 N-shard 的构造级/场景级测试 |

#### 2.1.5 风险

1. **`lib.rs` 写集瓶颈**：几乎每张卡都碰这个 9 万行文件；必须串行或先抽模块（卡 P1-5）。
2. **全局状态切割错**：`online`/`displays`/`guild`/`castle` 只要有一处仍被两个 World 共享可变访问，
   就回到「单线程」或引入数据竞争。Rust 借用检查能挡一部分，但逻辑上的「分片视图不一致」挡不住。
3. **跨片一致性与顺序**：聊天/组队/行会/城堡的可见顺序、重复投递、丢失，需要逐类定义并测试。
4. **handoff 的正确性**：属性/背包/buff/宝宝/组队/行会/存档，任何一项漏传都会造成状态丢失；
   必须做「源+目标指纹」对账。
5. **片间不均衡**：地图组按静态实体/在线数分；热点地图（城堡战、新手村）会漂移。需要运行期
   观测与再平衡策略（但再平衡本身很难，建议先做静态+告警）。
6. **内存**：同进程 N 个 World 不减少总内存（39.5k–53.5k 实体 + N 份副本状态），
   N 份 `World` 的固定开销远小于实体本身，但要注意每 World 的缓冲/缓存。

### 2.2 方案 B：多进程世界分片

#### 2.2.1 机制

- 每个进程一个完整 game-svr（一个 World + 自己的宿主状态），按地图组划分。
- 客户端登录/选角时由 DBSrv/LoginSrv 按角色当前地图路由到对应进程：
  `MapInfo.txt`（地图名→服务器编号）与 `ServerInfo.txt`（服务器→游戏网关地址/端口）已存在于
  `bins/db-srv/src/route.rs`，但当前生产未使用（`MapFile` 缺省空 ⇒ 恒 0）。
- 跨片换图：用 `SM_RECONNECT`/`SendChangeServer`（`!servertable.txt` 路由）+ 角色存档过渡；
  或新造跨进程 broker。**oracle 的 `.shr`/`ChangeSpaceMove` 未移植，G11 全停用**，
  所以这不是「接线」而是「实现一个 oracle 没有的行为」。
- 全局服务（行会/城堡/聊天/在线索引）需要中心进程或共享 DB；DB 写并发要重新设计。

#### 2.2.2 容量估算（推算）

- 单进程的墙钟与 A 的每线程相同（`W_total/M`）；单机上 M 个进程不会比 A 的 T 个线程更好
  （每个进程还有自己的 tokio/DB/GC/网络固定成本与 IPC）。
- **多机**：2 台 6c/12t（每台 6 有效核）⇒ T=12 ⇒ 813/12=68ms（基准）、937/12=78ms（内容上限），
  满足 100ms；1 台 10–12 物理核同理。
- 网络/序列化：跨进程消息与 handoff 的延迟比同进程通道高一个量级；跨片战斗/交易不存在
  （同图），主要是聊天/行会/组队/换图。
- 内存：每进程独立地址空间，总量不降；BotSrv 压测机的内存是另一个约束（`docs/botsrv-heap-1000bot-20261115.md`）。

#### 2.2.3 确定性/对拍

- 每个进程内仍是单 World 单线程，片内确定性保持；但跨进程消息是异步网络 ⇒ **时序不确定**。
  要可重放，需要中心 broker 的确定性排序 + 显式 tick barrier；比 A 难得多。
- oracle 没有多进程世界（G11 停用），所以 **B 没有 oracle 对拍真值**；只能做：
  1-shard 单进程回归；N 进程的确定性重放（同输入同输出）；跨进程业务场景测试；
  与 A 的 N-shard 结果做行为对照（若 A 先落地）。
- **结论：B 的对拍面比 A 更薄，必须作为「新系统」验收，不能声称 oracle 等价。**

#### 2.2.4 改动面

| 区域 | 改动 |
| --- | --- |
| `bins/db-srv/src/route.rs` / `MapInfo.txt` / `ServerInfo.txt` | 启用地图→服务器路由；登录/选角按图下发 |
| `bins/game-gate/src/lib.rs` | 多 game-svr 端点/多地址；会话粘性 |
| `bins/game-svr/src/lib.rs` | 跨进程 handoff、`SM_RECONNECT` 真路径、全局服务代理 |
| `crates/planes/*` 或新 broker | 跨进程消息（oracle 的 Planes 是停用桩；要么补全，要么新造） |
| `bins/login-srv` / `sel-gate` | 可能需要按图/负载分配服务器 |
| 运维 | 多进程起停、配置、日志聚合、DB 连接池、监控 |

#### 2.2.5 风险

- **协议级/行为级偏离**：`SM_RECONNECT` 换服在 oracle 里几乎不可达（G11 停用）；实现它等于新增协议行为，
  必须 Owner 批准并登记 D-*。
- **DB 竞争**：多进程同时写同一角色/行会/城堡；oracle 的单进程假设被打破，需要事务/租约。
- **故障域**：一个进程挂掉影响一片玩家；跨片状态（行会/城堡）需要一致性协议。
- **运维复杂度**：多机窗口、二进制一致性（已有 `docs/binary-provenance-stamp-20261115.md` 的构建戳要求）。
- **收益**：只有在多机时才有线性收益；单机上不如 A。

### 2.3 方案 C：网关流水线化 + HostInbound 批处理/索引（不分片）

#### 2.3.1 机制

目标不是「加线」，而是**降低每客户端/每实体的成本**，把单进程上限抬高，并给 A 减负。

1. **索引化 O(n) 表**：
   - `OnlineRegistry`：`Vec` + `HashMap<chr_name, idx>` + `HashMap<actor_id, idx>`，保留 `Vec` 的插入序
     用于需要按序广播的路径（`fan_out_sysmessage_to_all`/`guild_recipients`）。
   - `DisplayTable`：`Vec` + `HashMap<actor_id, idx>`（或直接按 actor_id 的稠密槽位），保留 `set/get/remove` 语义。
2. **hoist 每事件 O(sessions) 构造**：`guild_ban`、`guild_recipients`、重复的 `online`/`displays` 查找，
   改为按 tick/按变更集构造一次；保持输出集合与顺序不变。
3. **网关流水线**：I/O 线程做纯解析/校验/预分类，产出带**全局序号**的事件；sim 线程按序号处理，
   保证「同一 tick 内的事件顺序 = 今天的 `ev_rx` 顺序」。任何跨 tick 的重排都不允许。
4. **批处理**：同一会话一个 tick 的多条命令合并查找（一次 `online`/实体快照），但**逐条执行顺序不变**。
5. **可离线的纯函数**：`dlgselect_route_for`、目录/配置查表、字符串格式化等搬到 I/O 侧；
   仍然不碰 `World`。

#### 2.3.2 容量估算（推算）

- **单进程上限**（§1.6：`W(C,E) ≤ 200ms` / `≤100ms`）：
  - 未优化、d=17.7：≈986 / ≈493 客户端。
  - 先分散到 d=5.35：≈2,134 / ≈1,067。
  - 再把总成本降 50%：≈2,836 / ≈1,418（d=5.35）；d=17.7 时 ≈1,113 / ≈556。
- **10k 恒压活跃**：需要总 CPU 从 813–937ms 降到 200ms（单线程 4–4.7×）或 100ms（8–9.4×）。
  C 的现实目标是 **1.25–2×**，不是 4–9×；所以 **C 单独做不到 10k**，但能把 10k 的分片硬件需求
  从「8–10 有效核」降到「6–8 有效核」，并降低超线性风险。
- **收益来源必须实测**：上面的 25%/50% 是**目标区间**，不是承诺；P0-1 的分项探针要先回答
  「62.9ms 里有多少是 O(online)/O(display)/可离线解析」。

#### 2.3.3 确定性/对拍

- 只改数据结构/索引、保持输出集合与顺序 ⇒ **行为等价，tier L**。
- 任何改变「命令何时相对于 World tick 生效」的批处理/流水线 ⇒ **tier R**（时序是 R 级判据）。
- 对拍：现有 parity/golden 必须逐字节不变；新增「同一输入下新旧实现的帧序列逐条相等」的构造级测试；
  500 档 360s GREEN。

#### 2.3.4 改动面

| 区域 | 改动 |
| --- | --- |
| `bins/game-svr/src/lib.rs` | `OnlineRegistry` 索引化、hoist、批处理 |
| `bins/game-svr/src/downlink.rs` | `DisplayTable` 索引化 |
| `bins/game-svr/src/chat.rs` / `guild_*` | 收件人集合 hoist |
| `bins/game-svr/src/inbound.rs` / `gate_*` | 纯解析/预分类、序号 |
| `bins/game-gate/src/lib.rs` | 若做流水线，需要事件序号与背压 |
| `bins/game-svr/src/tick_profile.rs` | 分项探针 |

#### 2.3.5 风险

- 索引化本身低风险，但 **100 处 `online.iter()` 的语义不完全一样**（有的要插入序、有的要「每线最多一个」、
  有的要按 actor_id）；逐处核对，不能一刀切。
- 批处理/流水线容易改变事件顺序；一旦改变，parity 会 flaky。必须用序号 + 逐条顺序测试钉住。
- 收益可能低于预期（如果 62.9ms 主要是 World 侧/协议编码，而不是查找）；P0-1 先量再动手。

### 2.4 方案对比总表

| 维度 | A 同进程多 World | B 多进程 | C 单进程优化 |
| --- | --- | --- | --- |
| 机制 | 地图组 × N World，固定线程，协调线程 | N 进程 + 跨进程消息 | 索引/hoist/流水线 |
| 单机 6c/12t 容量（推算） | 10k 在 135–156ms（不含开销；含开销 135–203ms）；≤100ms 需 8–10 有效核 | 单机不比 A 好；多机线性 | 上限 ~1.1–2.8k（单进程） |
| 达到 10k 需要的硬件（推算） | 1×8–10 物理核（≤100ms）或 1×6 核（≤200ms） | 2×6c 或 1×10–12c | 不能单独达到 |
| oracle 逐字节对拍 | 仅 1-shard；N>1 是新模式 | 无 oracle 真值 | 保持（若顺序不变） |
| 跨图换图 | 同进程 handoff（一拍延迟） | 跨进程 handoff/`SM_RECONNECT`（协议级） | 不变 |
| 故障域 | 单进程（与今天相同） | 多进程，片间隔离 | 单进程 |
| 改动面 | 大（宿主拆分 + 协调器 + handoff） | 很大（协议 + DB + 运维） | 中（单文件热点） |
| 主要风险 | 全局状态切割、跨片一致性、handoff | 协议偏离、DB 竞争、运维 | 收益不确定、顺序保持 |
| 建议 | **T2 主路线** | 条件性（多机） | **必做第一步** |

### 2.5 明确排除的形态

1. **单 World 内并行**：违反 `no_parallel`/确定性硬约束；本文不提议。
2. **把一张图拆给两个 World**：地图内 AOI/碰撞/战斗强交互；拆了就不是同一个游戏。
3. **只加网关线**：`docs/gateway-multiline-sharding-20261115.md` 已证明多线只并行发送侧，
   所有线仍喂同一个 sim 线程；对 `HostInbound` 无效。
4. **复用现成 G11**：oracle 的跨服是停用桩（0 个真实副作用）；不能当作多进程的现成基础。

---

## 3. 推荐路线与分步验收

### 3.1 一句话

**先 C（度量 + 单进程优化），再 A（同进程地图组多 World），B 条件启用。**

### 3.2 分步与验收指标（可机检）

> 每步的「验收命令」都以仓库既有门禁为准：`cargo fmt/clippy/test/msgcodegen` 四门 +
> `docs_anchor_gate --prose`；R 级片另加真机窗口。压测判据用
> `tools/botload/load_gate.ps1` + `tools/botload/load_report.py` 的 `report.json` 字段。

| 步 | 对应卡 | 内容 | 机检验收（全部同时满足） | 回退 |
| --- | --- | --- | --- | --- |
| **S0**（度量，无生产行为） | P0-1/P0-2/P0-3 | HostInbound 分项探针；C×E 2D 网格；分散地图；空闲连接；地图图分析 | ① 探针关闭时 500 档 360s 与 master 同判（GREEN，`internal_errors=0`/`OOM=0`/`conn_lost_steady=0`/`no_resp=0`）；② 报告给出 `online`/`display`/解析/World 查找的 ms 占比；③ 2D 网格报告回答「C×E 交互项是否显著」；④ 空闲连接每客户端成本；⑤ 无生产 `src/` 行为变化（diff 只有探针/文档/夹具） | 只回退探针提交；无数据迁移 |
| **S1a**（O(n) 索引） | P1-1/P1-2 | `OnlineRegistry`/`DisplayTable` 索引化 | ① 五门全绿；② `cargo test -p mir2-parity-tests` 0 failed；③ 500 档 360s：`in_world≥500/500`、`internal_errors=0`、`tick_p99_ms≤100`、`conclusion_valid=true`；④ 探针显示查找类 ms 下降（目标 ≥30% 的总 HostInbound 降幅） | 还原单文件 diff；无 schema/落库变化 |
| **S1b**（hoist/批处理） | P1-3 | 每事件 O(sessions) 构造 hoist；同会话批查找 | ① 五门；② 帧序列构造级测试「新旧逐条相等」；③ 500 档 360s 同上；④ 每客户端 inbound ≤0.09ms（目标；基线 0.126） | 还原单文件/模块 diff；顺序测试是回退门 |
| **S1c**（网关流水线） | P1-4 | 纯解析/预分类离线 + 全局序号 | ① 五门；② 事件顺序测试（同 tick、跨 tick 边界、跨会话）；③ 500 档 360s 同上；④ 每客户端 inbound ≤0.063ms（stretch） | 序号层可关（`MIR2_INBOUND_PIPELINE=0`）；关闭 = 今天路径 |
| **S1d**（总成本目标） | （里程碑；P1-1..P1-4 合成） | 由 S1a..c 合成 | ① 10k 推算输入更新：`W_total(10k,39.5k) ≤ 700ms` 或 `(10k,53.5k) ≤ 820ms`；② 实测 500 档成本系数下降 ≥25%；③ 全部门禁 | 逐步回退到上一个绿点 |
| **S2a**（1-shard 骨架） | P2-1 | `ShardHost` + `MIR2_SHARDS=1`，行为不变 | ① 五门；② 1-shard golden/parity **逐字节**与 master 相同；③ 500 档 360s GREEN | 配置默认 1；骨架可整体 revert |
| **S2b**（地图组 + N=2） | P2-2 | 静态地图组、两个 World、跨片换图 fail-closed | ① 地图覆盖/连通性检查 rc=0；② N=2 下每片实体/地图正确；③ 500 客户端分两片：`in_world≥475/500`（95%）、`internal_errors=0`、每片 `HostTickTotal−HostSleep` ≤100ms；④ 跨片换图未实现时**明确拒绝**、不静默 | 配置回到 1-shard；地图组文件可空 |
| **S2c**（会话路由 + 协调器） | P2-3/P2-4 | 事件按 shard 路由；全局服务/在线索引/DB 队列 | ① 五门；② 每会话帧审计：不收到别片帧、不丢帧；③ 跨片私聊/行会/组队/城堡广播场景测试；④ N=2 500 档 360s：全局 `in_world≥475`、`internal_errors=0`、`close_inworld=0`、每片段 ≤100ms | 配置回 1-shard；协调器可旁路（单 shard 时直连） |
| **S2d**（handoff） | P2-5 | 跨片换图受控 handoff | ① 2-map 连通场景：坐标/HP/MP/buff/冷却/背包/组队/行会/宝宝对账全等；② 无重复帧/无丢失帧；③ 换图后 1 tick 内不可见**登记为 D-***；④ 1-shard 回归不变 | 关闭跨片换图（拒绝）+ 回 1-shard |
| **S2e**（确定性守卫） | P2-6 | 多 World 线程归属/RNG/无共享可变状态 | ① 变异重放：删守卫 ⇒ 牙红；② `cargo test -p mir2-world-bevy`；③ 五门；④ 运行期断言在 N=2/4 下不触发 | 守卫可加不可减；失败即停 |
| **S2f**（N=10–12 容量窗） | P2-7/P2-8 | 10k 实测/或分片容量窗 | ① `in_world ≥ 95%`（≤360s 有效窗）；② 每片 `HostTickTotal−HostSleep` P99 ≤100ms；③ `internal_errors=0`、`OOM=0`、`close_inworld=0`、`conclusion_valid=true`；④ 全局 `tick_p99_ms ≤100`（若 Owner 定 100ms 硬指标）；⑤ 归档 + 复算脚本 rc=0 | 降到上一个满足的 N；或转 B |
| **S3**（仅条件） | P3-1 | 多进程/多机 spike | ① 设计件 + D-* 清单；② 2 进程确定性重放；③ 跨进程业务场景；④ Owner 决策记录 | 不合并生产，只留 spike |

> **注**：`in_world ≥95%` 取判据的「60s 闸门 ≥90%」之上：60s 闸门是无效线，95% 是本文建议的
> 结论线；两者都要报。`tick_p99_ms` 是 `load_report.py` 的既有阈值（`DEFAULT_THRESHOLDS`）。

### 3.3 每步的风险与回退（重点：确定性与对拍）

#### 3.3.1 确定性

- **每 World 单线程**：不迁移线程、不共享 `&mut World`；`TickOwnerThread` 断言保留并扩展到多实例。
- **World 间无共享 RNG**：每 World 一个 `RandomNumber`；协调线程零 RNG；跨片消息零 RNG。
  同 N、同种子、同输入 ⇒ 同输出（可重放）。
- **固定跨片顺序**：协调线程按 `(thread_id, shard_id, seq)` 分发；worker 按 shard_id 升序 tick；
  所有跨片事件在 tick 的固定相位 drain。不依赖 `HashMap` 迭代序（今天的代码已多处显式排序）。
- **不发散**：任何需要「全局唯一序号」的东西（`item_number`、DB 存档顺序）留在协调线程；
  World 只发请求、不自行分配全局号。

#### 3.3.2 对拍（分片后怎么跑）

1. **1-shard 模式是 oracle 对拍模式**：`MIR2_SHARDS=1` 时，所有现有 parity/golden/五门原样跑，
   必须与 master 逐字节一致。这是每次合并的硬门。
2. **N-shard 模式不是 oracle 对拍模式**：oracle 没有多 World，且 RNG 流被切开。N>1 的验收改为：
   - **可重放**：同 N/种子/输入跑两遍，输出逐字节相同；
   - **帧完整性**：每会话不丢帧、不重复帧、不收到别片帧（构造级 + 真机）；
   - **跨片业务场景**：私聊/行会/组队/城堡广播/GM/换图，逐场景断言；
   - **无 RNG 对照**：关闭掉落/AI 随机等 RNG 依赖后，N-shard 与 1-shard 的帧序列应相等
     （这是最强的跨片等价测试；不能覆盖 RNG 依赖行为）。
3. **不在 N>1 上声称逐字节等价**：写进卡面与 review；任何「分片后与 oracle 一致」的说法都是过度承诺。

#### 3.3.3 回退

- P0/P1：单文件/单模块 diff，可逐个 revert；无数据迁移。
- P2：`MIR2_SHARDS` 默认 1；地图组文件可空；协调器在 1-shard 下旁路；handoff 可关闭并 fail-closed。
- P3：只做 spike，不合并生产，无回退问题。

### 3.4 单机 6c/12t 能到多少（区间，不是承诺）

基于 §1.5 模型与 §2.1.2 的推算：

| 场景 | 总 CPU/tick（推算） | 6c/12t（T=6，×1.1–1.3 开销） | 判定 |
| --- | --- | --- | --- |
| 10k 恒压活跃、39.5k 实体、P1 总量未降 | 813ms | **135–176ms** | ≤200ms 可；≤100ms 不可 |
| 10k 恒压活跃、53.5k 实体、P1 总量未降 | 937ms | **156–203ms** | ≤200ms 边缘；≤100ms 不可 |
| 10k、39.5k、P1 总量降 25% | 697ms | **116–151ms** | ≤200ms 可；≤100ms 仍不可 |
| 10k、53.5k、P1 总量降 25% | 821ms | **137–178ms** | ≤200ms 可；≤100ms 仍不可 |
| 10k、39.5k、P1 总量降 50% | 581ms | **97–126ms** | ≤100ms 边缘（需 T≥8 才稳） |
| 10k、53.5k、P1 总量降 50% | 705ms | **118–153ms** | ≤200ms 可；≤100ms 需 T≥8–9 |
| 10k、53.5k、P1 总量降 50%、T=8 | 705ms | **88–115ms** | ≤100ms 边缘 |

**结论（写死，避免过度承诺）**：

- **6c/12t 单机在实测系数下不能保证「10k 恒压活跃 + tick P99 ≤100ms」**；
  它能在 135–203ms 量级跑 10k，也就是「200ms 预算内，但不是 50% 预算」。
- 要 ≤100ms：**需要约 8–10 个有效核**（10–12 物理核，或 2×6c）；若只有 6–8 个有效核，
  则还需 P1 把总量降 25–50%。两者至少满足一个，通常要同时满足。
- 若 Owner 接受 ≤200ms，6c/12t + P1 + A（N=10–12）是有机会的；但必须用 P0 的真实系数替换本文推算。
- 若目标是 T1（10k 在线、≤0.9k 同时活跃），**P0+P1 可能就够**，A 可以先不做；先用 P0 的空闲连接
  成本与真实动作频率分布判定。

### 3.5 何时转 B（多进程/多机）

满足以下任一条才考虑 B：

1. P0/P1 后，单机 6c/12t（或目标机器）的 10k 实测墙钟 >200ms；
2. Owner 要求 ≤100ms 且单机核数不足（<8–10 有效核）；
3. 需要故障隔离/滚动升级（多进程的运维收益）；
4. 需要跨机水平扩展（10k 以上）。

进入 B 之前必须先有：A 的 1-shard 等价、跨片消息/协调器、handoff 的测试；否则 B 会把 A 的问题
乘以进程数。

---

## 4. 拆分卡清单（只写卡面，不写代码）

### 4.1 写集与串行约束（先读）

- **`bins/game-svr/src/lib.rs` 是 91,823 行的单文件**，P1-1..4 / P2-1..7 都会碰它。
  按 `docs/AGENTS.project.md` §0b「按写集切分」，这些卡**必须串行**，不能并行派工。
- 建议先做 **P1-5 宿主模块抽取**（把 `online`/`display`/`shard`/`coordinator` 从 `lib.rs` 拆出），
  否则 Phase 2 的并行度是 1。
- `crates/world-bevy/**` 是 HIGH_CHURN 目录：P2-5（handoff）会碰 `crates/world-bevy/src/session.rs`，
  P2-6（守卫）会碰测试与 `crates/world-bevy/src/tick.rs`；其它卡不改世界 tick 语义。
- 所有卡都按 `docs/AGENTS.project.md` 的分级写 `tier`/`tier_why`；R 级卡必须占真机窗口。

### 4.2 卡列表

> 工作量：S ≈ 0.5–1 天，M ≈ 1–3 天，L ≈ 3–5+ 天（相对本项目历史卡；不是承诺）。
> 依赖写作 `A → B` 表示 B 依赖 A。

#### Phase 0：度量与图分析（无生产行为）

| 卡 | 目标 | 写集范围 | 依赖 | 量 | 验收命令/判据 |
| --- | --- | --- | --- | --- | --- |
| **P0-1** `openmir2-hostinbound-cost-decomposition` | 把 `HostInbound` 拆成：mpsc drain / 每命令派发 / `online` 查找 / `display` 查找 / World 实体查找 / 帧入队；加 `online` 扫描长度、`DisplayTable` 长度、每 ident 计数。探针关闭零开销（照 `HostTickProfile` 模式） | `bins/game-svr/src/tick_profile.rs`、`bins/game-svr/src/lib.rs`（只加探针调用）、`docs/` 报告 | 无 | S | `cargo test -p game-svr --lib`；探针关 A/B 开销 <2%；500 档 360s `internal_errors=0`/`OOM=0`/`conn_lost_steady=0`/`no_resp=0`；报告给出分项 ms 占比 |
| **P0-2** `openmir2-capacity-2d-grid` | 在 C∈{200,500,1000} × E∈{2k,8k,内容上限} 的 2D 网格上量 `W(C,E)`，拟合 `aC+bE+cCE` 并判 `c` 是否显著；补分散地图与空闲连接测量。回答 §1.5 的「可辨识性缺口」 | `tools/botload/`（脚本/夹具参数）、`docs/` 报告；若需地图分散，改 BotSrv 夹具（T-* 登记） | P0-1 | M | 归档原始 `ndjson` + 报告；每轮按 `loadgate-criteria-runbook` 判窗口（结论 ≥360s，<360s 只冒烟）；报告逐条回答 c≈0？、空闲成本、分散密度 |
| **P0-3** `openmir2-map-shard-partition-analysis` | 分析 207 张图：连通分量、每图实体/在线密度、城堡/行会驻地/新手村热点；产出「地图→组」候选与跨组边数 | `docs/`、`tools/` 新分析脚本、配置样例；不改运行时 | P0-2（用实测密度） | S | 覆盖 100% 地图；每图只属一组；跨组边数与组间不均衡有报告；脚本 rc=0 |

#### Phase 1：单进程成本（C）

| 卡 | 目标 | 写集范围 | 依赖 | 量 | 验收命令/判据 |
| --- | --- | --- | --- | --- | --- |
| **P1-1** `openmir2-online-registry-index` | `OnlineRegistry = Vec` → `Vec + HashMap(chr_name/actor_id)`；保留插入序迭代；逐处核对 100 个查找点的语义（按名/按 actor/按线号/每线最多一个） | `bins/game-svr/src/lib.rs`、新 `bins/game-svr/src/online.rs`、测试 | P0-1 | M | 五门 + `cargo test -p mir2-parity-tests`；500 档 360s GREEN；探针查找类降幅 ≥30%；索引一致性单测（插入/覆盖/移除/顶号） |
| **P1-2** `openmir2-display-table-index` | `DisplayTable` `Vec` → 索引化；保留 `set/get/remove` 语义与可观察顺序 | `bins/game-svr/src/downlink.rs`、`lib.rs` 调用点、测试 | P0-1 | M | 五门 + parity；500 档 360s GREEN；展示表操作计数/耗时下降；`append`/`remove` 顺序单测 |
| **P1-3** `openmir2-inbound-hotpath-hoist` | hoist 每事件 O(sessions) 构造（`guild_ban`/`guild_recipients`/重复查找）；同会话批查找；输出集合与顺序不变 | `bins/game-svr/src/lib.rs`、`chat.rs`、`guild_*` | P1-1, P1-2 | M | 五门；帧序列构造级「新旧逐条相等」；500 档 360s GREEN；每客户端 inbound ≤0.09ms |
| **P1-4** `openmir2-inbound-pipeline-sequence` | 纯解析/预分类移到 I/O 线程，带全局序号；sim 线程按序号处理；`MIR2_INBOUND_PIPELINE=0` 可关 | `bins/game-svr/src/inbound.rs`、`gate_*`、`bins/game-gate/src/lib.rs`、`lib.rs` | P1-3 | M | 五门；同 tick/跨 tick/跨会话顺序测试；500 档 GREEN；每客户端 inbound ≤0.063ms（stretch） |
| **P1-5** `openmir2-host-module-extraction`（建议） | 从 `lib.rs` 抽出 `online`/`display`/`shard`/`coordinator` 模块；纯移动，无行为变化 | `bins/game-svr/src/lib.rs` + 新模块 | P1-1..4 | M | 五门；golden/parity 逐字节不变；无 `lib.rs` 之外的语义改动 |

#### Phase 2：同进程多 World（A）

| 卡 | 目标 | 写集范围 | 依赖 | 量 | 验收命令/判据 |
| --- | --- | --- | --- | --- | --- |
| **P2-1** `openmir2-shard-runtime-skeleton` | 引入 `ShardId`/`ShardHost`/`ShardRuntime`；`MIR2_SHARDS` 默认 1；单 World 行为不变 | `lib.rs`、新 `shard.rs`、`config.rs`、`bins/mir2-server` | P1-5 | M | 五门；`MIR2_SHARDS=1` golden 逐字节；500 档 360s GREEN |
| **P2-2** `openmir2-shard-map-partition` | 静态地图组→World；N=2 两个不相交组；覆盖/连通性校验；跨片换图 fail-closed 并明确报错 | `lib.rs`、`shard.rs`、`config.rs`、地图加载接线、配置样例 | P2-1, P0-3 | M | 地图覆盖 rc=0；N=2 每片地图/实体正确；500 分两片 `in_world≥475`、每片段 ≤100ms；跨片拒绝有日志/计数 |
| **P2-3** `openmir2-shard-session-router` | gate 事件按会话所属 shard 路由；每片 `ground_sessions`/`displays`/`saves`；全局在线索引在协调器；每会话命令顺序不变 | `lib.rs`、`shard.rs`、新 `coordinator.rs`、`gate_*` | P2-2 | M | 五门；每会话帧审计（不串片/不丢帧）；N=2 500 档 360s `in_world≥475`、`internal_errors=0`、`close_inworld=0` |
| **P2-4** `openmir2-shard-coordinator` | 协调线程：在线索引、行会、组队、城堡、全局聊天/私聊、GM、DB 存档队列、`item_number`、IDS/公告；固定跨片顺序；零 RNG | `lib.rs`、`coordinator.rs`、`chat.rs`、`guild_runtime.rs`、`party.rs`、`castle.rs`、`dbclient.rs` | P2-3 | L | 跨片私聊/行会/组队/城堡/GM 场景测试；顺序钉住；协调器零 RNG 断言；N=2 500 档 GREEN |
| **P2-5** `openmir2-map-handoff` | 跨片换图 tick 边界 handoff：冻结/序列化/转移/下一拍插入；AOI clear/show、属性/HP/MP/buff/冷却/背包/组队/行会/宝宝/存档 | `lib.rs`、`shard.rs`、`coordinator.rs`、`crates/world-bevy/src/session.rs`、`crates/world-bevy/src/space_move.rs` | P2-3, P2-4 | M | 2-map 连通场景对账全等；无重复/丢失帧；1 tick 不可见登记 D-*；1-shard 回归不变；五门 |
| **P2-6** `openmir2-shard-determinism-guards` | 守卫：一 World 一 owner 线程、不迁移、不共享 `&mut World`、每 World RNG、协调器零 RNG、无 World 内并行；源码/运行期牙 | `crates/world-bevy/tests/no_parallel.rs`、`crates/world-bevy/tests/bevy_drop_parity.rs`、`crates/world-bevy/src/tick.rs`、`bins/game-svr/src/shard.rs`、测试 | P2-1 | M | 变异重放逐支红；`cargo test -p mir2-world-bevy`；五门；N=2/4 运行期断言不触发 |
| **P2-7** `openmir2-shard-capacity-report` | 压测报告加 `shard=<id>`/`thread=<id>`；断言「每片段 ≤100ms + 全局 in_world/四元组」；归档/复算脚本 | `bins/game-svr/src/tick_profile.rs`、`lib.rs`（shard 标记）、`tools/botload/load_report.py`、`in_world_stability.py`、docs | P2-3 | M | 脚本 rc=0 只在全部判据满足时；输出每片表；复算可重放 |
| **P2-8** `openmir2-shard-10k-window` | 10k 容量窗（需要分布式/多进程 bots 或更轻的夹具）；记录达到的 ceiling 与缺口 | `tools/botload/`、docs；不改生产 | P2-4, P2-5, P2-7, P1-4 | L | `in_world≥95%`、每片 ≤100ms、`internal_errors=0`、`OOM=0`、`close_inworld=0`、`conclusion_valid=true`、全局 `tick_p99_ms≤100`（若定硬指标）；否则报告实测 ceiling + 缺口 |

#### Phase 3：多进程/多机（条件，需 Owner 批准）

| 卡 | 目标 | 写集范围 | 依赖 | 量 | 验收命令/判据 |
| --- | --- | --- | --- | --- | --- |
| **P3-1** `openmir2-multi-process-shard-spike` | 设计/spike：地图→进程路由（`MapInfo`/`ServerInfo`）、DB 竞争、跨进程 broker、handoff 序列化；评估「补全 G11」vs「新造 broker」；D-* 清单 | `docs/`、`tools/`、prototype 分支；**不动生产 `src/`** | P2-8 结论「单机不足」+ Owner 决策 | L | 设计件 + D-* 清单 + 2 进程确定性重放 + 跨进程业务场景；Owner 决策记录；不合并生产 |

### 4.3 依赖图

```
P0-1 ──> P0-2 ──> P0-3
  │        │        │
  ├─> P1-1 ─> P1-2 ─> P1-3 ─> P1-4
  │                              │
  └────────────────> P1-5 ─> P2-1 ─> P2-2 ─> P2-3 ─> P2-4 ─> P2-5
                              │                 │
                              └─> P2-6          └─> P2-7 ─> P2-8
                                                            │
                                                            v
                                                          P3-1（条件）
```

> 表外的交叉依赖：P2-2 还依赖 P0-3（地图组）；P2-8 还依赖 P1-4（入站成本已降）与 P2-5（handoff 可用）。

### 4.4 不建议先做的卡

- **直接上 N=10 的分片**：没有 P0 的系数与 P1 的成本下降，N=10 只是把未知放大。
- **先做多进程**：G11 停用、DB 竞争、运维成本；单机收益为负。
- **先改 `crates/world-bevy` 内部**：分片不需要改世界语义；改它只会扩大 R 级面。
- **先做网关多线扩容**：已证明不解决 sim 线程瓶颈。

---

## 5. 必须由 Owner/总监拍板

### 5.1 Owner（目标/授权/合规边界）

| # | 决策 | 选项 | 默认建议 |
| --- | --- | --- | --- |
| O1 | **目标定义**：T1（10k 在线、≤0.9k 同时活跃）还是 T2（10k 恒压活跃）？ | T1 / T2 / 两者 | 先按 T1 交付并量空闲成本；T2 作为容量上界目标 |
| O2 | **是否接受 N>1 不是 oracle 逐字节等价** | 接受 / 只允许 1-shard | 接受（否则只能做 P1；多进程同样无 oracle 逐字节等价） |
| O3 | **100ms 是否硬验收** | 100ms / 200ms / 分阶段 | 先 200ms 交付 10k，再以 100ms 为目标优化；若必须 100ms，需多机或更多核 |
| O4 | **是否允许协议级/跨服改动（G11/`SM_RECONNECT`/MapInfo 路由）** | 允许 / 不允许 | 不允许则 P3 不做；P2 仍可做 |
| O5 | **是否允许跨片换图 1 tick 过渡** | 允许 / 不允许 | 允许并登记 D-*；不允许则地图组必须做成连通闭包（更难） |
| O6 | **是否必须支持 10k 同图** | 必须 / 不必须 | 不必须；同图 10k 需 AOI/密度改造，另立卡 |
| O7 | **多机资源窗口**（2×6c 或 10–12c） | 批准 / 不批准 | 若 O3 要 100ms，则需要 |

### 5.2 总监/Leader（项目内方案、排期、风险）

| # | 决策 | 建议 |
| --- | --- | --- |
| L1 | P0 是否先于一切（包括其它性能卡） | 是；P0-1/P0-2 是后续所有卡的前提 |
| L2 | P2 的 N/T 目标（N=10–12、T=6–8） | 先 N=2 正确性，再 N=10 容量；T 按目标机器定 |
| L3 | 是否先做 P1-5 模块抽取（牺牲短期速度换并行度） | 建议做；否则 Phase 2 串行 |
| L4 | 地图组划分策略（连通性优先 vs 负载均衡优先） | 先连通性+静态均衡，热点再告警 |
| L5 | 跨片全局服务的边界（行会/城堡/组队/交易） | 行会/城堡/组队走协调器；交易/同图战斗保持片内 |
| L6 | 是否接受「N-shard 不可 oracle 对拍」的验收替代（可重放+帧完整性+场景） | 接受；写进 review 的 tier/tier_why |

---

## 6. 未验证项与残余风险（按影响排序）

| # | 未验证项 | 为什么没测 | 影响 | 怎么测 |
| --- | --- | --- | --- | --- |
| U1 | **C×E 交互项是否因果** | 只在 C=500 上测过 E 变化；C 与 E 在夹具里相关 | 若因果，10k 真实成本可能是数秒/tick，分片也救不了 | P0-2 的 C×E 2D 网格；固定 E 变 C、固定 C 变 E |
| U2 | **空闲连接成本** | 夹具恒压，未测空闲 | T1（10k 在线）无依据 | P0-2：C=1000/5000 空闲/低频，量 `HostInbound` 与保活 |
| U3 | **分散地图的真实密度与成本** | 所有档都在同一张图；同图 O(N²) 是上界 | 真实 10k 可能显著低于同图上界；也可能因跨图热点而集中 | P0-2：把 bot 分散到多图（改 BotSrv 夹具或预置角色位置） |
| U4 | **`online`/`DisplayTable` 的 O(n) 占比** | 没有分项探针 | P1 收益不确定（可能 10%，也可能 50%） | P0-1 |
| U5 | **6c/12t 的真实有效核数** | 未测 HT/IO/DB 竞争 | §3.4 的墙钟区间依赖它 | P0/P2 窗口的空载 CPU 与 `heavy_procs` 采样 |
| U6 | **BotSrv 1000 档 OOM/网关丢帧/串行进世界** | 三条专卡未闭 | P0-2 的 C=1000+ 可能跑不起来 | 依赖 `openmir2-botsrv-heap-limit-oom`、`openmir2-gateway-backpressure-frame-drop`、`openmir2-world-entry-serial-saves`；或改用多 BotSrv/更轻夹具 |
| U7 | **N-shard 的跨片顺序与延迟** | 还没实现 | 聊天/组队/换图的用户体验 | P2-4/P2-5 的场景测试与 N 次重放 |
| U8 | **handoff 的状态完整性** | 还没实现 | 换图丢状态（最严重的正确性风险） | P2-5 的源/目标指纹对账 |
| U9 | **多进程的 DB 竞争** | oracle 是单进程假设 | 存档覆盖/行会/城堡一致性 | P3 spike |
| U10 | **内存**：N 个 World/进程的总内存 | 只测过单进程 | 53.5k 实体 + N 份状态；BotSrv 另算 | P2/P3 窗口的 WS/私有字节/LOH 趋势（按 A8-trend 口径，不单独下结论） |
| U11 | **tick 分位与工作段的口径不一致** | 同一报告里 88.2ms 工作段 vs 15/16/32/172ms 分位 | 不能拿分位当容量基准 | 已在本文口径声明；建议 P0 统一两路测量 |
| U12 | **同图 10k 的可行性** | 地图不能拆 | O6 若要求「必须」，则分片无用 | 另立 AOI/密度卡；本文不建议 |

### 6.1 模型置信度

- **高置信**：500 档 360s 的工作段 88.2ms、`HostInbound` 62.9ms、每客户端 0.126ms、实体轴 1.209µs/实体。
- **中置信**：双因子拟合在 500–8,500 实体、500 客户端内的线性；单线程上限 ≈1k。
- **低置信**：10k 外推（线性总模型 813–937ms）；C×E 交互；分散地图；6c/12t 墙钟。
- **不作承诺**：任何「6c/12t 一定 10k」或「分片后一定 ≤100ms」的说法。

---

## 7. 证据与来源清单

### 7.1 文档

- `docs/capacity-model-20261114.md`：10k 容量模型、双因子拟合、500 档 360s 窗口、同图警告、假设 A。
- `docs/tick-hotspot-profile-20261113.md`：World::tick 分段、1k/5k/11k 线性缩放、release 5.82µs/实体 vs dev 144µs/实体（≈24.7×）。
- `docs/gateway-multiline-sharding-20261115.md`：N=1/2/4 多线实验；多线只并行发送侧；`service_id=端点下标` 的硬约束。
- `docs/loadgate-criteria-runbook-20261115.md`：压测判据（login/no_resp/internal_errors/OOM、360s 有效性、60s 闸门）。
- `docs/botsrv-heap-1000bot-20261115.md`：1000 档 BotSrv 内存与 4GiB 上限。
- `docs/g11-planes-design-20261108.md`：跨服/Planes 的 oracle 停用状态（0 个真实副作用）。
- `docs/AGENTS.project.md`：四条确定性硬约束、判据分级 R/L、按写集切分、跨图分布式「另行设计」。
- `docs/binary-provenance-stamp-20261115.md`：多机/多进程的二进制来源要求（若走 P3）。

### 7.2 归档（实测原始数据）

- `E:/UserFleet/load-360-errorstack-20261115-2116/`：500 档 360s GREEN（`in_world=500/500`、`internal_errors=0`）。
- `E:/UserFleet/archive/gate-capacity-360-20261115b/`：380.7s 窗口、按实体分箱、`entity_bins_380s.json`。
- `E:/UserFleet/archive/gate-saturated-500-20261115/`：493s 窗口、`saturation_analysis.json`。
- `E:/UserFleet/archive/gate-criticality-20261115-w83/`：关键性版 N=1/N=4 500 档。
- `E:/UserFleet/archive/gate-multiline-20261115/`：多线实验（N=1/2/4）。
- `E:/UserTemp/w83c-prof/final2.txt`：11k 实体 release bench（`docs/capacity-model-20261114.md` §7 引用）。

### 7.3 代码锚（只写符号/文件，不写行号，避免高 churn 锚腐烂）

- `bins/game-svr/src/lib.rs`：`sim_loop`、`HostInbound`/`HostWorldHead`/`HostPlayerRun`/`HostAoiDownlink`
  等段边界、`OnlineRegistry`、`OnlineEntry`、`online_set`/`online_take`、`route_to_gate`。
- `bins/game-svr/src/downlink.rs`：`DisplayTable`、`ActorDisplay`。
- `bins/game-svr/src/tick_profile.rs`：`HostTickProfile`、`ENV_TICK_PROFILE_SLOW_MS`、段名。
- `bins/game-gate/src/lib.rs`：`MAX_SESSION`、`service_id`（端点下标）、`ClientSession`。
- `bins/game-svr/src/gate_queue.rs`：`DEFAULT_QUEUE_BYTES`/`DEFAULT_QUEUE_FRAMES`、`GATE_SLOT_CAPACITY`。
- `bins/db-srv/src/route.rs`：`GateRoute`、`Share.map_list`、`load_map_list`（MapInfo 地图→服务器）。
- `crates/world-bevy/src/world.rs`：`World`、`SimState`、`sessions`、`maps`、`rng`。
- `crates/world-bevy/src/tick.rs`：`TickOwnerThread`、`TickStage`、`STAGE_ORDER`、`SingleThreadedExecutor` 使用点。
- `crates/world-bevy/src/aoi.rs`：按 `map_id` 分桶的 `OutputAoi`。
- `crates/world-bevy/tests/no_parallel.rs`、`crates/world-bevy/tests/bevy_drop_parity.rs`：`no_parallel_guard` 源码扫描。
- `crates/world-bevy/src/session.rs`：`Session`、`SessionRegistry`、`MapChange`。

---

## 8. 交付与复现

- 本批只新增/修改 `docs/10k-shard-design-20261116.md`（本文件）；**不改任何 `src/`、不动协议、不起栈、不跑 cargo 全量**。
- 未签名（按派工说明，由 Leader 统一处理）。
- 复现本文数字：按 §7.1/§7.2 打开对应文档/归档；本文所有推算都可由 §1.5 的公式复算。
- 残留物：无（未生成临时测量文件、未改部署目录、未起进程）。

---

## 附录 A：公式汇总

```
# 实测拟合（500 档 360s 窗口，docs/capacity-model-20261114.md §4.6）
W(C,E) = 23.22 + 0.008834×E                 # ms/tick, C=500, E≥2000
H(C,E) = C × (0.0300 + 1.1998e-5×E)         # HostInbound, ms/tick
ΣWorld(E) = 1.731 + 0.001813×E              # ms/tick
SimMonsterAi(E) = 0.000962×E                # ms/tick

# 本文线性总模型（推算，假设固定分量随 C 线性、无 C×E 交互）
W_total(C,E) ≈ 0.04644×C + 0.008834×E       # ms/tick
  23.22 / 500 = 0.04644

# 单线程上限
W_total(C,E) ≤ 200  → 满预算
W_total(C,E) ≤ 100  → 50% 预算

# 分片（线性模型）
W_shard = W_total / N
wall_per_thread = W_total / T               # T = 有效 worker 线程数

# 10k 场景
W_total(10k, 39.5k) ≈ 464.4 + 348.9 = 813 ms/tick
W_total(10k, 53.5k) ≈ 464.4 + 472.6 = 937 ms/tick
```

## 附录 B：判据字段（来自 `tools/botload/load_report.py` 的默认阈值）

- `login_success_pct = 100.0`
- `conn_lost_steady = 0`
- `tick_p99_ms = 100.0`
- `internal_errors = 0`（唯一放宽通道 = 评审过的 `internal-errors-allowlist.json`）
- 窗口：`HoldSec ≥ 360` 才能作为结论；`<360` 只可冒烟（`window.conclusion_valid=false`）。
- 60s `in_world` 闸门：低于档位 90% ⇒ 本轮无效。
- 本文建议的结论线：`in_world ≥ 95%`、`internal_errors=0`、`OOM=0`、`close_inworld=0`、
  `tick_p99_ms ≤100`（若 Owner 定 100ms）、`conclusion_valid=true`。
