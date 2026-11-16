# map_id 归属模型（P2-3a）

- 卡：`openmir2-shard-mapid-ownership`
- 前序：`docs/10k-shard-design-20261116.md` §2.1；`docs/shard-runtime-20261116.md`（P2-1/P2-2）
- 状态：needs-review。**只离线验收**；真机 N>1 窗口归 P2-3b。

## 1. 路线（已定，不采用本地下标换算）

`map_id` 仍是**注册表全局下标**（`Transform.map_id`、`RouteHop.dst_map_id`、城堡、脚本共用）。
每个 `World` 增加：

- `SimState::owned_maps: Vec<bool>`（长度 = `maps.len()`；缺省全 `true`）；
- `SimState::set_map_ownership(&[bool])`（`[]` = 全拥有；长度不符 ⇒ panic，禁止半套归属）；
- `SimState::is_map_owned(map_id)` 与 `SimState::owned_map_ids()`。

宿主把**非拥有图**构造成 **1×1 stub grid**（`MapGrid::new` 要求尺寸 > 0），从而保持全局下标空间；
stub 上**禁止实体**，所有换图/落格入口统一 fail-closed。**不采用**「每 World 本地下标 + 宿主侧
双向换算」——那需要把每个裸 `usize` 边界都换算，漏一处即静默串片。

## 2. 换图/落格入口（单点 fail-closed）

统一返回可区分错误 `SessionError::MapNotInShard`（不是 `NoSuchMap`：图存在、但不归本 World）：

| 入口（符号） | 非拥有目标的处置 |
| --- | --- |
| `SessionRegistry::enter_map_at` | `Err(MapNotInShard)`；不建实体、不改会话 |
| `SessionRegistry::switch_map_inner`（`switch_map` / `switch_map_space`） | `Err(MapNotInShard)`；旧图旧格保持、不记 `MapChange` |
| `World::route_enter`（经 `walk_route_check` / `step_with_route`） | 视同 `EnterAnotherMap` 前置失败：不换图、不记 `MapChange` |
| `World::base_object_move_player`（home move） | 视同 `FindMap == null`：`map_found=false`、不动、不抽数 |
| `space_move::monster_space_move` | 返回 `false`；**在任何写入之前**返回（不摘旧格、不改坐标） |
| `World::spawn_monster_precomputed` / `spawn_monster_now_impl` | `None` / `false`；门在掉落预算之前 ⇒ 被拒时零抽数 |
| `World::enter_world_impl` | `false`（stub 不落格） |
| `World::insert_merchant` / `insert_npc` | `None`；门在 ctor 取数之前 |
| `World::spawn_clone`（防御） | `None`（主人不可能在非拥有图上） |

`switch_map_inner` 与 `enter_map_at` 是玩家换图的**共同咽喉**：`route_enter` / home move /
脚本与 GM 落点最终都经它们落地；其余入口在各自取数/写坐标之前加同类检查。

## 3. tick 期全量 `maps` 扫描审计

- **AOI**（`aoi` 的 `OutputAoi`）是**实体驱动**：先按 `Transform.map_id` 分桶，再按桶取图；
  不变式保证 stub 桶为空 ⇒ 不需要改迭代。
- **逐图工作**（掉落实体清单 `floor_items_sorted` / `floor_items_view_sorted`）改为遍历
  `owned_map_ids()`；单片时与「遍历全部」逐字节同序。
- 宿主侧逐图灌入（flag、格子属性、门表、守卫/商人/NPC/怪物过滤）只对**拥有图**执行
  （`apply_map_cells_owned` / `apply_map_doors_owned` / `owns` 过滤）；stub 不被当成真图。
- 城堡门/墙掩码写回落格改为 `world.is_map_owned(id)` 过滤（同时兜住越界）。
- `0..maps.len()` 形式的循环已逐处改为 `maps_of(host)`；剩余 `maps.len()` 只用于下标空间日志/探针。

## 4. 运行期不变式（debug）

`World::tick_tail` 末尾调用 `assert_no_entity_on_unowned_map()`：遍历全部实体，**已落格**
（`add_to_mapped == true`）者必须满足 `is_map_owned(map_id)`，否则 panic 并打印 id/name/map_id。
release 构建由 `cfg!` 常量折叠为零开销。未落格实体允许停放任意 `map_id`（既有测试用它构造边界态）。

## 5. 跨片 `MapChange`

world 已在换图入口拒绝，正常不会产生跨片 `MapChange`。宿主侧仍做防御：

- `ShardRuntime::accept_map_change(map_id)` 按 `map_to_shard` 判可路由；不可路由 ⇒
  `cross_shard_map_change_refused += 1`（日志）。
- `drain_map_change` 对 `!world.is_map_owned(ev.map_id)` 的事件跳过不发 TransMap 簇。
- handoff 归 P2-5；本卡只拒绝与计数。

## 6. `MIR2_SHARDS=1` 等价与验收

- 单片布局 `host_ownership()` 全 `true`、`host_map_sizes()` 与输入逐元素相同 ⇒ 全部归属检查恒过、
  逐图扫描与改动前同序；没有任何新日志。
- 判据：五门禁 + golden/parity 逐字节 + `map_ownership_tests`（每个入口一条非拥有用例）+
  三个不变式变异（删入口守卫 / 放 stub 实体 / 扫描遍历全部 id）逐条红。
- 真机 N>1 不跑（R 窗口留 P2-3b）。
