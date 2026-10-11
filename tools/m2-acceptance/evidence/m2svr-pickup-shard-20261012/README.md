# 地面物品只读分片（owner §25.1 裁决 (A)）—— 红检与证据

- 卡：`openmir2-m2-gamesvr-host`（M2 验收② 的 `pickup` 格）
- 分片声明：`openmir2-m2-gamesvr-host` 线程 comment `86cefdb66c7c46d3a18ea3a6cda007dc409269f3`（先声明后动代码，owner §25.1）
- 写集：`crates/world/src/{map.rs,world.rs,lib.rs}`（owner §25.1 授权的最小只读分片）

## 1. 交付的只读 API

```rust
pub fn World::floor_items_view_sorted(&self) -> Vec<FloorItemView>
// FloorItemView { map_id, x, y, obj_id, kind, index, count, item_id, of_base_object, can_pick_up_tick }
// FloorItemKind = Item | Gold
```

- **只读**：不改任何 `Command`/攻击/掉落结算语义；`floor_items_sorted()` 的输出逐项不变（`obj_id` 口径不变）。
- 记账点 = 掉落落地时（`World::place_floor_item`），字段来源：
  - `kind/index/count`：物品 = (`Item`, `StdItem` 序号, 1)，金币 = (`Gold`, 0, `mon.Gold`)；
  - `item_id`：物品 = `UserItem.MakeIndex`（`GetItemNumber` 唯一），金币 = 落主 `ActorId`；
  - `of_base_object` = oracle `RM_DIEDROPITEM` 的 `wParam`（`ExpHitter.Master?.ActorId ?? ExpHitter.ActorId`，`BaseObject.cs:2437/2442`）；
  - `can_pick_up_tick` = `CellObject.add_time`（既有虚拟时钟）。

## 2. 红检（改坏映射 ⇒ 必红）

```bash
cd mir2-rs
CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target cargo test -p mir2-world --offline \
  monster_death_floor_item_view_matches_world_state
```

- **改坏**：把 `floor_items_view_sorted` 的 `item_id: meta.item_id` 改成 `meta.item_id + 1`
  → `redcheck-mutated.txt`：`panicked ... 金币 item_id = 落主 ActorId: left: 3 right: 2`，`FAILED`，exit 101。
- **恢复**：`redcheck-restored.txt`：`test ... ok`，1 passed，exit 0。

## 3. 残留物清单

- 本目录：`redcheck-mutated.txt` / `redcheck-restored.txt`（raw 输出，入库）。
- 无仓库外残留物；未起栈、未占端口、未改现网配置。
