# P2-3b 交接：同进程多 World × 固定 worker 线程（只读侦察，2026-11-21）

> 卡 `card/shard-p2-3b-handoff`（base master `84da1b71f`）。**本文件只做侦察与风险清单，不实现**；
> 接手的会话按此开工。基线文档：`docs/10k-shard-design-20261116.md`、`docs/map-zone-split-design-20261122.md`。

## 1. 现状：World 的所有权与驱动形态

- **单 World**：`mir2-rs/bins/game-svr/src/lib.rs::sim_loop` 在**本线程内**建唯一 World：
  `sim_loop` 内调 `production_world(map_sizes, …)`（同文件），后者 `World::with_maps(map_sizes)`。
  `map_sizes` 含**全部**地图 ⇒ 今天的"片"只是 `ShardRuntime::host_owns` 的**过滤**（P2-2），
  不是多个 World。
- **事件入口**：所有网关事件走**单通道** `ev_rx`/`ev_ctl_rx`（`sim_loop` 形参），由一个线程消费
  ⇒ 天然单线程、无 demux。会话态 `sessions`/`ground_sessions` 是 `sim_loop` 内的局部 `HashMap`，
  同样单线程独占。
- **shard 侧已就绪的静态件**（`mir2-rs/bins/game-svr/src/shard.rs`）：
  `ShardConfig::{from_env,from_raw}`（`MIR2_SHARDS`）、`ShardLayout::{single,partition,owns,maps_of,
  is_cross_shard}`、`ShardRuntime{layout,counters}` 的 `host_owns`/`route_allowed`/分类口径。
  **缺口**：`ShardRuntime` 是**一个**（= HOST_SHARD）；没有"每片一个 World/一个 runtime/一个线程"，
  也没有跨片消息面（P2-4）。

## 2. `TickOwnerThread` 的层级与多 World 下如何保留

- 实现在 `mir2-rs/crates/world-bevy/src/tick.rs::TickRunner::run`：首次 tick 把调用线程写入
  `TickOwnerThread` 资源，之后**每次 tick 断言同一线程**（"世界 tick 必须始终由同一线程发起"）。
  资源在 `World::with_maps` 内注入（`crates/world-bevy/src/world.rs`）。
- 含义：**每个 `World` 自带亲和性断言**。多 World 方案 = N 个 `World` 各在各的 worker 线程上跑，
  每个 `TickRunner` 的断言自动生效 ⇒ **不要**在片间共享 tick、也不要把同一 `&mut World` 交给
  两个线程（会立即 panic，这是可用的守卫）。

## 3. N=1 零行为变化的验证方式（建议照抄）

1. `parity-gate.ps1 -RepoRoot <树> -Mode Both -RunId <id>` ⇒ **Driver 150/150 + Comparator 50/50 +
   `PARITY_GATE_OK`**（判决看 `RED_SET` 与 `PARITY_GATE_OK`，不只看 rc）；
2. **stdout 逐字节**：同一构建、同一环境变量下，把"改动前/改动后"各跑一次并把 stdout 落文件
   （`cmd > out.txt 2> err.txt; echo RC=$?`），`sha256sum out.txt` 相等 + `cmp` rc=0；glog 走 stderr，
   若 stdout 为空则记"空文件 sha 相同"（并把 err 的行数/首尾 20 行作为旁证）；
3. 断言 `ShardConfig::single` 下 `host_owns` 恒真、`ShardCounters` 全 0（P2-2 已有的零变化口径）。

## 4. 三个最大风险（按严重度）

1. **事件分发**：多 World 后 `ev_rx` 必须按"该会话/地图属于哪片"**恰好一次**投递到对应 worker；
   漏投=丢登录/丢帧，重投=重复帧。现有单通道语义是很多帧序用例的隐含前提（`frames.ndjson` 顺序）。
2. **ActorId/ID 分配**：`EntityStore::alloc_id` 是每 World 的；`MIR2_SHARDS>1` 时跨片 ActorId 必须
   全局唯一（客户端/网关按 ActorId 路由）——需要全局分配器或按片分段，且不得改变 N=1 的号序。
3. **启动装载与 RNG 账本**：构造段 fold（`ctor_fold`）与 `world.rng` 是**单 World** 语义；N>1 时
   每片如何重放/切分 RNG、地图脚本与商人/NPC 装载归属哪片，必须保证 **N=1 逐字节不变**、
   N>1 可解释（否则窗口对拍全部失真）。

## 5. 建议的落地顺序（给接手的会话）

1. 先把"每片一个 `ShardRuntime` + `maps_of(shard)` 子集"抽成 `sim_loop` 的参数化入口（N=1 行为不变）；
2. 起 N 个 worker（每线程 `production_world(片地图子集)` + 自己的 `TickRunner`），事件入口按会话
   出生地图 demux（先只做单播、fail-closed）；
3. 最后才碰 ActorId 全局分配与装载/RNG 归属（风险 2/3），并补 N=2 集成用例
   （两 World 固定线程 tick、跨片 `route_allowed` 拒、`TickOwnerThread` panic 反例）。
