# 世界分片运行时（P2-1 / P2-2 / P2-3b）配置与边界

- 卡：`openmir2-shard-runtime-skeleton`（P2-1）+ `openmir2-shard-map-partition`（P2-2）+
  `card/shard-multi-world`（P2-3b：每片固定 worker 线程 + 事件路由 + ActorId 分段）
- 设计：`docs/10k-shard-design-20261116.md` §2.1（同进程多 World）与 §4.2 的 P2-1/P2-2/P2-3
- 状态：needs-review（**片 1..N 的 World 已构造并 tick；登录仍只落片 0**；见 §6）

本文只描述本批**已落地**的配置面与 fail-closed 边界。容量结论、N/T 选择与压测判据仍以设计件为准。

## 1. 配置项

| 环境变量 | 缺省 | 取值 | 非法值 |
| --- | --- | --- | --- |
| `MIR2_SHARDS` | `1` | `1..=16` | **fail-closed**：空串 / 非整数 / 0 / 超上界 ⇒ 非零退出（与 `MIR2_RNG_SEED` 同款纪律） |
| `MIR2_SHARD_MAP_GROUPS` | 未设 | `<片号>:<地图名>[,<地图名>…]`，组间 `;`；`*` = 注册表里尚未分配的其余地图 | 语法错 / 未知地图 / 重复分配 / 未覆盖 / 空片 / 片号越界 / 多个 `*` ⇒ 非零退出 |

- 地图名按**注册表口径**匹配：ASCII 大小写不敏感、忽略两端空白（与 `MapRegistry::id_of` 同源）。
- `MIR2_SHARDS=1` 且未设地图组 ⇒ 布局是「全部地图归片 0」，**不读分组、不打印任何新日志**，
  下游拿到的 `map_sizes` 与改动前逐元素相同。
- 片数 >1 时必须配 `MIR2_SHARD_MAP_GROUPS`；未配 ⇒ 拒绝启动（不猜分组）。

### 示例：N=2

```bash
MIR2_SHARDS=2 MIR2_SHARD_MAP_GROUPS="0:0,0129;1:*" ./mir2-server game-svr /path/to/Server.conf
```

含义：注册表前两张图（逻辑名 `0`、`0129`）归片 0，其余全部地图归片 1。
启动日志会打印一行覆盖校验：

```
地图组覆盖校验：shards=2 maps=207 assigned=207 unassigned=0 per_shard=[2,205]（片 0 = 注册表前缀 0..2）
```

## 2. 划分校验（rc = 0 或明确报错）

构造时逐条校验，任一条不满足即**启动失败**并给出人话错误：

1. **覆盖 100%**：每张注册表地图要么被显式列出、要么被唯一一个 `*` 收走；否则报「未覆盖 N 张（名单）」。
2. **每图唯一归属**：同一张图出现在两个组 ⇒ 报错并点名地图。
3. **片号合法且不重复**：`片号 < MIR2_SHARDS`，每片只能写一组。
4. **每片非空**：空片 = 一个永不接客的 World ⇒ 拒绝。
5. **~~片 0 必须是注册表前缀~~（P2-3a 已取消）**：`map_id` 的全局下标由宿主 World 的
   `owned_maps` + 1×1 stub 承载，任意分组都可表达；换图/落格入口在 world-bevy 单点
   fail-closed。见 `docs/shard-mapid-ownership-20261116.md`。

## 3. fail-closed 边界（N>1）

本批**不做**跨片 handoff（P2-5），因此任何「跨片」路径都必须**明确拒绝**，不得静默降级：

| 路径 | 行为 |
| --- | --- |
| 走路换图（`MapRoute`） | 跨片连接点在**注册阶段**被拒绝（不注册），启动时打印拒绝条数并累计计数；走到该格 = 普通格子 |
| 登录/进图 | 目标图属其它片 ⇒ 新增的匹配臂拒绝进图，打印 `拒绝进图（跨片 fail-closed…）` + 累计计数 |
| 已进入世界后的内部换图 | 宿主 World 的 `maps` 只到片 0 前缀 ⇒ 既有 `map_id >= maps.len()` 门（`NoSuchMap` / route 越界）全部拒绝 |
| 刷怪（`MonGen.txt`） | 只装载宿主片地图的条目；其它片的条目丢弃并打印条数 |
| 商人/NPC/MapQuest | 地图名 → `map_id` 的闭包按宿主片过滤，其它片的记录按「未知地图」口径丢弃（`MIR2_SHARDS=1` 时恒不过滤） |
| GM 换图命令 | oracle 里这些命令体本就是 no-op（`MapMove`/`PositionMove` 等），不新增行为 |

**反例（明确不许）**：把「其它片的图」也塞进宿主 World 当占位图，再靠运行期守卫兜住。
占位图在 `map_id < maps.len()` 的既有检查下是**可进入**的，任何漏掉的入口都会变成静默跨片。
本批的做法是让宿主 World **物理上只有片 0 的地图**（`maps` 前缀裁切），拒绝来自结构而不是逐点断言。

## 4. 未实现与已知硬问题

### 4.1 未实现（后续卡）

1. **片 1..N 已构造并 tick，但暂不接客**（P2-3b，§6）：每片一个 World + 固定 sim 线程；
   事件路由已接线（按 `chr→片` 目录单播）。但登录链仍只落片 0——`GateEvent.index` 是网关连接
   自己的槽位号、`socket` 跨网关线会重复，没有全局唯一会话键 ⇒ 不能按 index 轮转。当前片 1..N
   只有静态内容、无会话；跨片登图由既有 guard 拒绝（handoff 归 P2-5）。
2. **跨片 handoff**（P2-5）：换图一旦跨片就拒绝，不存在「一拍过渡」。
3. **协调器/全局服务分片**（P2-4）：P2-3b 起进程级服务只在片 0 运行（DB/IDS/guild/castle/
   关服汇总/G11 planes，见 §6.3）；`online`/`display`/`saves` 是每片 sim 的局部状态，非 0 片为空。

### 4.2 分片会话路由（P2-3）的第一个硬问题（**已由 P2-3a 解决**）

`map_id` 是**注册表全局下标**，被实体（`Transform.map_id`）、地图连接点（`RouteHop.dst_map_id`）、
城堡、脚本地图名解析共同使用；而 `World.maps` 是按下标索引的 `Vec`。要让每个 World 只装自己的
地图组又不破坏全局下标，只有两条路：

- **全局下标保序 + 非本片占位**：需要 `crates/world-bevy` 支持「本 World 不拥有的地图」
  （所有换图入口统一拒绝），否则占位图会被当成真图进入；
- **每个 World 独立本地下标 + 宿主侧双向换算**：所有「宿主 ↔ World」的 `map_id` 边界都要换算，
  漏一处就是跨片串图。

P2-2 用「片 0 = 注册表前缀」绕开该问题；**P2-3a 已按第一条路线落地**：`owned_maps` + 1×1 stub +
所有换图/落格入口 fail-closed + 每 tick 不变式。详见 `docs/shard-mapid-ownership-20261116.md`。
P2-3b 的会话路由可以直接依赖该模型。

## 5. 复现与证据

```bash
cd mir2-rs
cargo test -p game-svr --lib shard          # 配置/划分/覆盖/守卫的单元判据
cargo test -p game-svr --lib -j 2           # 宿主回归（含既有帧面/日志判据）
cargo test --workspace -j 2                 # 五门之一
```

- `MIR2_SHARDS=1` 的等价性：`ShardLayout::single(n).host_map_sizes(&sizes) == sizes`（单测），
  且片数为 1 时 `host_owns`/`route_allowed` 恒真、`plan_layout` 走不读注册表的快路径、
  `startup_log_lines` 返回零行 ⇒ 生产路径的帧序列与 master 相同。
  合并门禁另跑 `mir2-parity-tests` 的 golden 逐字节对拍。
- **反向牙**：上述单片早退各有变异用例——把「1 片走单 World」的分支改坏（删掉/放宽早退、
  让 1 片也走分组路径、单片也裁切 `map_sizes`、单片也打印分片日志）时，对应用例必红；
  变异重放的做法与结果写在卡面 patch 的交付报告里。

## 6. P2-3b：同进程多 World（每片固定 worker 线程）

> 卡 `card/shard-multi-world`（前序 P2-3a）。范围：N 个 World 实例各固定一个 sim 线程；
> 协调器/跨片 handoff（P2-4/P2-5）**不在本批**。

### 6.1 形态

- `MIR2_SHARDS=1`：与 master 完全同路径——单个 `m2-sim` 线程、单个 World、无路由线程、
  无目录；所有 `local == HOST_SHARD` 的 gate 恒真。
- `MIR2_SHARDS=N>1`：每片一个 `m2-sim-<id>` 线程（线程不迁移；`TickOwnerThread` 与
  `World.owner_thread` 两处断言逐 World 生效）；每片
  `World::with_maps(local_map_sizes(...))` + `set_map_ownership(local_ownership(...))`；
  `ctor_fold` 计划与 MonGen 条目按片过滤。
- 事件：一个 `m2-event-router` 线程把单条输入通道按 `chr→片` 目录单播；
  `RegisterGate`/`UnregisterGate` **广播**到每片；其余（`Opened`/`CertificationPending`/
  `LoadDbRequest`）落片 0。关服：一个 `m2-shutdown-broadcast` 线程把单消费者 `Receiver`
  翻成共享旗标（各片读同一旗标）。

### 6.2 已知限制（必须知道）

1. **登录永远落片 0**：`GateEvent.index` 是网关连接自己的槽位号、`socket` 跨网关线会重复，
   没有全局唯一会话键 ⇒ 不能按 index 轮转。N>1 时非 0 片暂无会话；跨片登图由既有 guard
   继续 fail-closed（handoff 归 P2-5；届时要先给事件加全局会话键）。
2. **城堡实体只在片 0**：城堡是进程级服务（落盘/战况/税），`initialize_castle` 只在片 0 跑；
   若城堡地图分到非 0 片，该片无门/墙/弓手实体。
3. **ActorId 全局唯一**：`SlotTable` 的 id 按片分段（`base = shard * (1<<22)`）；N=1 不设段
   （首个 id 仍为 1）。跨片引用（handoff/跨片消息）目前不存在。
4. **item_number / MakeIndex**：仍是 per-World 计数器；非 0 片本批无玩家 ⇒ 不会铸物品；
   协调器（P2-4）接管全局分配。
5. **MonGen 的 `PlayObjectCount` 门**：按本 World 计数（oracle 是全局在线数）；N>1 时各片
   的 UserFull/Zen 加速门比 oracle 更保守（登记）。
6. **RNG trace**：`MIR2_RNG_LOG` 是进程级开关；N>1 时非 0 片在构造 World **之前**调用
   `rng_trace::suppress_current_thread()`，**只有片 0 的取数落账**（跨片交错账不可对拍）。
7. **遥测**：计数器跨片累加；周期/终值行只片 0 打印，N>1 行首为 `shards=N aggregated`
   （数字是跨片合计，不标会被当成单片读数）。
8. **准入闸共享**：任一片装载失败 ⇒ 共享 `WorldStartGate` 置 Aborted ⇒ 整机拒连
   （设计意图：一个片起不来不该带病接客，不是副作用）。

### 6.3 进程级服务 gate 表

| 服务 | N>1 处置 |
| --- | --- |
| DB wiring（取档/存档/周期存档/关服存档） | 只片 0（非 0 片 `db=None`） |
| IDS（`ids_online` / `SS_SERVERINFO`） | 只片 0 |
| 行会（startup + step store + 断线存档） | 只片 0（非 0 片空表 + `store=None`） |
| 城堡（装载/落盘/战况/税） | 只片 0 |
| 关服汇总回传 | 只片 0 |
| G11 planes | 只片 0（非 0 片 `None`） |
| 彩票 / item_number | 共享句柄 / 仅登记缺口（非 0 片无玩家不触达） |

### 6.4 证据

- 构造级：`tests/shard_multi_world.rs`（两 World 异线程 tick、跨线程 tick panic、按图分片）。
- 路由：`shard_router` 单测（广播/单播/片 0 兜底/目录生命周期/关服信号）。
- `MIR2_SHARDS=1` 零行为：`cargo test -p game-svr --lib shard` + `-Mode Both` 150/50 +
  同一构建前后 stdout 比对（**剥每行开头 12 位墙钟时间戳**后顺序敏感逐字节 + 行多重集）。
  为什么不能原样 sha：`glog!` 宏展开为 `println!`（`bins/game-svr/src/lib.rs` 的 `macro_rules! glog`）且带 `HH:MM:SS.mmm` 前缀。
